#!/usr/bin/env python3
"""Run the packaged Glass TUI in a pseudo-terminal and exit with ``q``."""

import fcntl
import os
import pathlib
import select
import signal
import struct
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
    deadline = time.monotonic() + 10
    try:
        while b"\x1b[?1049h" not in output and time.monotonic() < deadline:
            readable, _, _ = select.select([master_fd], [], [], 0.05)
            if readable:
                output.extend(os.read(master_fd, 16 * 1024))
        if b"\x1b[?1049h" not in output:
            fail("did not enter the alternate screen")

        # Exercise the exact terminal modes the product enables: resize, focus,
        # SGR mouse, bracketed paste, ordinary keys, and bounded shutdown.
        fcntl.ioctl(master_fd, termios.TIOCSWINSZ, struct.pack("HHHH", 24, 64, 0, 0))
        os.write(master_fd, b"\x1b[I\x1b[<64;4;4M\x1b[<64;4;4m")
        os.write(master_fd, b":\x1b[200~help\x1b[201~\r")
        time.sleep(0.1)
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
        (b"\x1b[?1004h", "enable focus events"),
        (b"\x1b[?2004h", "enable bracketed paste"),
        (b"\x1b[?1004l", "disable focus events"),
        (b"\x1b[?2004l", "disable bracketed paste"),
        (b"\x1b[?1049l", "leave alternate screen"),
        (b"\x1b[?25h", "show cursor"),
    ):
        if marker not in output:
            fail(f"missing {description} terminal evidence")

    print(f"packaged TUI smoke passed: {binary}")


if __name__ == "__main__":
    main()
