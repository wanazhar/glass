#!/usr/bin/env python3
"""Measure the real desktop viewer in a private display; never attach to a user session."""

import json
import os
import signal
import subprocess
import tempfile
import time
import urllib.request
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
BINARY = Path(
    os.environ.get(
        "GLASS_CORE_TEST_BINARY", ROOT.parents[1] / "target/release/glass-core"
    )
).resolve()
ASSETS = Path(os.environ.get("GC_WEB_ASSETS", ROOT / "web/dist")).resolve()


def cleanup_namespace(home):
    """Stop only surviving children with this exact, private HOME namespace."""
    expected = ("HOME=" + str(home)).encode()
    owned = []
    for proc in Path("/proc").iterdir():
        if not proc.name.isdecimal():
            continue
        handle = None
        try:
            handle = os.pidfd_open(int(proc.name))
            if not any(
                value == expected or value.startswith(expected + b"/")
                for value in (proc / "environ").read_bytes().split(b"\0")
            ):
                os.close(handle)
                continue
            owned.append(handle)
            signal.pidfd_send_signal(handle, signal.SIGTERM)
        except (OSError, ProcessLookupError):
            if handle is not None and handle not in owned:
                os.close(handle)
            continue
    if owned:
        time.sleep(0.2)
    for handle in owned:
        try:
            signal.pidfd_send_signal(handle, signal.SIGKILL)
        except ProcessLookupError:
            pass
        finally:
            os.close(handle)

    for line in Path("/proc/self/mountinfo").read_text().splitlines():
        fields = line.split()
        if len(fields) > 5 and fields[4].startswith(str(home) + "/"):
            subprocess.run(
                ["/usr/bin/fusermount3", "-u", "-z", fields[4]],
                check=True,
                capture_output=True,
                timeout=5,
            )


def main():
    with tempfile.TemporaryDirectory(prefix="gc-region-benchmark-") as temporary:
        home = Path(temporary)
        env = dict(
            os.environ,
            HOME=str(home),
            XDG_DATA_HOME=str(home / "data"),
            XDG_CONFIG_HOME=str(home / "config"),
            GC_WEB_ASSETS=str(ASSETS),
        )
        (home / "data").mkdir()
        (home / "config").mkdir()
        process = subprocess.run(
            [str(BINARY), "connect", "--local"],
            cwd=home,
            env=env,
            capture_output=True,
            check=False,
            timeout=20,
        )
        assert process.returncode == 0, "Isolated Connect launch failed"
        state = json.loads((home / "data/glass-core/connect.json").read_text())

        def api(path, value=None):
            headers = {"Authorization": "Bearer " + state["token"]}
            body = None
            if value is not None:
                headers.update(
                    {
                        "Content-Type": "application/json",
                        "Origin": f"http://127.0.0.1:{state['port']}",
                    }
                )
                body = json.dumps(value).encode()
            with urllib.request.urlopen(
                urllib.request.Request(
                    f"http://127.0.0.1:{state['port']}/api/desktop/" + path,
                    data=body,
                    headers=headers,
                ),
                timeout=60,
            ) as response:
                return json.load(response)

        environment = os.environ.get("GC_BENCH_ENVIRONMENT", "xfce")
        assert environment in ("xfce", "gnome")
        picture = None
        try:
            active = api(
                "session",
                {"action": "start", "environment": environment, "lifecycle": "reusable" if environment == "gnome" else "disposable"},
            )
            api("lease", {"action": "take-control"})
            deadline = time.monotonic() + 20
            while not api("status")["available"]:
                assert time.monotonic() < deadline, (
                    "Isolated desktop did not produce frames"
                )
                time.sleep(0.1)
            session_directory = home / "data/glass-core/desktop"
            if environment == "gnome":
                session_directory = session_directory / "profiles/default/sessions"
            display_env = dict(
                env,
                DISPLAY=active["displayId"],
                XAUTHORITY=str(
                    session_directory
                    / active["sessionId"]
                    / "Xauthority"
                ),
            )
            scene_log = home / "scene.log"
            with scene_log.open("wb") as log:
                picture = subprocess.Popen(
                    [
                        "/usr/bin/python3",
                        str(ROOT / "scripts/desktop-latency-scene.py"),
                        str(home / "scene.ready"),
                    ],
                    env=display_env,
                    stdout=subprocess.DEVNULL,
                    stderr=log,
                )
            deadline = time.monotonic() + 10
            while not (home / "scene.ready").exists():
                assert picture.poll() is None, scene_log.read_text()
                assert time.monotonic() < deadline, (
                    "Benchmark scene did not become ready"
                )
                time.sleep(0.05)
            if environment == "gnome":
                # GNOME initially opens Overview, which covers the benchmark's
                # override-redirect window. Observe its real state and close it
                # through the ordinary managed keyboard path before measuring.
                address = (session_directory / active["sessionId"] / "session-bus.address").read_text()
                deadline = time.monotonic() + 10
                while True:
                    overview = subprocess.run(["gdbus", "call", "--address", address,
                        "--dest", "org.gnome.Shell", "--object-path", "/org/gnome/Shell",
                        "--method", "org.freedesktop.DBus.Properties.Get",
                        "org.gnome.Shell", "OverviewActive", "--timeout", "2"],
                        env=display_env, capture_output=True, check=True, timeout=3)
                    if b"false" in overview.stdout:
                        break
                    assert time.monotonic() < deadline, "GNOME Overview did not close"
                    api("input", {"action":"key-tap", "key":"Meta"})
                    time.sleep(.2)
            low_delay = False
            raw_capture = False
            capture_fps = None
            for child in (
                Path(f"/proc/{state['pid']}/task/{state['pid']}/children")
                .read_text()
                .split()
            ):
                try:
                    args = Path(f"/proc/{child}/cmdline").read_bytes().split(b"\0")
                    if args and Path(os.fsdecode(args[0])).name == "ffmpeg":
                        low_delay = b"-threads" in args and b"-flush_packets" in args
                        raw_capture = b"rawvideo" in args and b"rgb24" in args
                        if b"-framerate" in args:
                            capture_fps = int(args[args.index(b"-framerate") + 1])
                except OSError:
                    pass
            config = home / "browser.json"
            config.write_text(
                json.dumps(
                    {
                        "port": state["port"],
                        "token": state["token"],
                        "duration": int(os.environ.get("GC_BENCH_SECONDS", "20")),
                        "sessionId": active["sessionId"],
                        "captureLowDelay": low_delay,
                        "captureRaw": raw_capture,
                        "captureFps": capture_fps,
                    }
                )
            )
            config.chmod(0o600)
            run = subprocess.run(
                [
                    "node",
                    str(ROOT / "web/test/desktop-updates-live.mjs"),
                    str(config),
                ],
                env=env,
                cwd=ROOT,
                timeout=100,
                check=False,
            )
            if picture.poll() is not None:
                print("Scene error:", scene_log.read_text())
            assert run.returncode == 0, "Real desktop browser benchmark failed"
        finally:
            if picture is not None:
                picture.terminate()
                try:
                    picture.wait(timeout=5)
                except subprocess.TimeoutExpired:
                    picture.kill()
                    picture.wait()
            stopped = subprocess.run(
                [str(BINARY), "connect", "stop"],
                cwd=home,
                env=env,
                capture_output=True,
                check=False,
                timeout=20,
            )
            cleanup_namespace(home)
            assert stopped.returncode == 0, "Isolated gateway cleanup failed"
            deadline = time.monotonic() + 5
            while (
                Path(f"/proc/{state['pid']}/exe").exists()
                and time.monotonic() < deadline
            ):
                time.sleep(0.1)
            assert not Path(f"/proc/{state['pid']}/exe").exists(), (
                "Isolated gateway remains running"
            )


if __name__ == "__main__":
    main()
