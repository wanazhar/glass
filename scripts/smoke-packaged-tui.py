#!/usr/bin/env python3
"""Run the packaged Glass TUI in a pseudo-terminal and exit with ``q``."""

import fcntl
import os
import pathlib
import select
import signal
import subprocess
import sys
import termios
import time


def fail(message: str) -> "NoReturn":
    raise SystemExit(f"packaged TUI smoke failed: {message}")


def child_session(slave_fd: int) -> None:
    os.setsid()
    fcntl.ioctl(slave_fd, termios.TIOCSCTTY, 0)


def main() -> None:
    if len(sys.argv) != 2:
        raise SystemExit("usage: smoke-packaged-tui.py BINARY")

    binary = pathlib.Path(sys.argv[1])
    if not binary.is_file() or not os.access(binary, os.X_OK):
        fail(f"packaged artifact is not executable: {binary}")

    master_fd, slave_fd = os.openpty()
    try:
        environment = os.environ.copy()
        environment.setdefault("TERM", "xterm-256color")
        child = subprocess.Popen(
            [str(binary), "tui"],
            stdin=slave_fd,
            stdout=slave_fd,
            stderr=slave_fd,
            close_fds=True,
            env=environment,
            preexec_fn=lambda: child_session(slave_fd),
        )
    finally:
        os.close(slave_fd)

    output = bytearray()
    deadline = time.monotonic() + 5
    try:
        os.write(master_fd, b"q")
        while child.poll() is None and time.monotonic() < deadline:
            readable, _, _ = select.select([master_fd], [], [], 0.05)
            if readable:
                try:
                    output.extend(os.read(master_fd, 16 * 1024))
                except OSError as error:
                    if error.errno != 5:  # EIO: the PTY closed after process exit.
                        raise
        if child.poll() is None:
            os.killpg(child.pid, signal.SIGKILL)
            child.wait(timeout=2)
            fail("did not exit after the bounded quit request")
        while True:
            try:
                chunk = os.read(master_fd, 16 * 1024)
            except OSError as error:
                if error.errno == 5:
                    break
                raise
            if not chunk:
                break
            output.extend(chunk)
    finally:
        os.close(master_fd)

    if child.returncode != 0:
        fail(f"exited unsuccessfully: {child.returncode}")
    for marker, description in (
        (b"\x1b[?1049h", "enter alternate screen"),
        (b"\x1b[?25l", "hide cursor"),
        (b"\x1b[?1049l", "leave alternate screen"),
        (b"\x1b[?25h", "show cursor"),
    ):
        if marker not in output:
            fail(f"missing {description} terminal evidence")

    print(f"packaged TUI smoke passed: {binary}")


if __name__ == "__main__":
    main()
