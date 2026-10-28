#!/usr/bin/env python3
"""Exercise Connect's real managed desktop, stream, auth, and shared lease."""

import http.client
import json
import os
import socket
import subprocess
import tempfile
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
BIN = Path(os.environ.get("GLASS_CORE_TEST_BINARY", ROOT.parent.parent / "target/debug/glass-core")).resolve()
ASSETS = Path(os.environ.get("GC_WEB_ASSETS", ROOT / "web/dist")).resolve()


def request(port, path, token=None, method="GET", value=None, read_body=True):
    connection = http.client.HTTPConnection("127.0.0.1", port, timeout=10)
    headers = {}
    body = None
    if token:
        headers["Authorization"] = f"Bearer {token}"
    if value is not None:
        headers["Content-Type"] = "application/json"
        body = json.dumps(value)
    connection.request(method, path, body=body, headers=headers)
    response = connection.getresponse()
    data = response.read() if read_body else b""
    return connection, response, response.status, response.headers, data


def post(port, token, path, value):
    connection, _, status, _, body = request(port, path, token, "POST", value)
    connection.close()
    return status, json.loads(body or b"{}")


def agent_request(path, value):
    connection = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
    connection.settimeout(8)
    connection.connect(str(path))
    connection.sendall(json.dumps(value).encode() + b"\n")
    data = bytearray()
    while not data.endswith(b"\n"):
        chunk = connection.recv(64 * 1024)
        if not chunk:
            break
        data.extend(chunk)
        if len(data) > 6 * 1024 * 1024:
            raise AssertionError("desktop socket response exceeded its bound")
    connection.close()
    return json.loads(data)


def pids_for_display(display, auth_file):
    expected = {f"DISPLAY={display}".encode(), f"XAUTHORITY={auth_file}".encode()}
    pids = []
    for entry in Path("/proc").iterdir():
        if not entry.name.isdigit():
            continue
        try:
            environment = set((entry / "environ").read_bytes().split(b"\0"))
        except (OSError, PermissionError):
            continue
        if expected.issubset(environment):
            pids.append(int(entry.name))
    return pids


def main():
    if not BIN.is_file():
        raise SystemExit(f"Glass Core binary not found: {BIN}")
    if not (ASSETS / "index.html").is_file():
        raise SystemExit(f"Connect web assets not built: {ASSETS}")
    with tempfile.TemporaryDirectory(prefix="gc-connect-desktop-") as temporary:
        home = Path(temporary)
        env = os.environ.copy()
        env.update(
            HOME=str(home),
            XDG_DATA_HOME=str(home / "data"),
            XDG_CONFIG_HOME=str(home / "config"),
            GC_WEB_ASSETS=str(ASSETS),
        )
        (home / "data").mkdir()
        (home / "config").mkdir()
        gateway = subprocess.run(
            [str(BIN), "connect", "--local"],
            cwd=home,
            env=env,
            capture_output=True,
            text=True,
            timeout=15,
        )
        if gateway.returncode:
            raise AssertionError(f"Connect did not start: {gateway.stderr or gateway.stdout}")
        state_path = home / "data/glass-core/connect.json"
        try:
            deadline = time.monotonic() + 10
            while not state_path.exists() and time.monotonic() < deadline:
                time.sleep(0.05)
            settings = json.loads(state_path.read_text())
            port, token = settings["port"], settings["token"]

            _, _, denied, _, _ = request(port, "/api/desktop/status")
            assert denied == 403, f"desktop status was not authenticated: {denied}"
            _, _, status, _, body = request(port, "/api/desktop/status", token)
            assert status == 200, body
            assert json.loads(body)["sessionId"] is None

            started, result = post(port, token, "/api/desktop/session", {"action": "start"})
            assert started == 200, result
            assert result["mode"] == "managedVirtualSession"
            assert result["displayId"] != ":0", result
            session_id = result["sessionId"]

            _, _, status, _, body = request(port, "/api/desktop/status", token)
            current = json.loads(body)
            assert status == 200 and current["available"], current
            assert current["sessionId"] == session_id
            assert current["workspaceRoot"] == str(home.resolve())
            assert current["width"] == 1280 and current["height"] == 800
            assert current["frameSeq"] >= 6, current

            _, _, status, headers, image = request(port, "/api/desktop/frame", token)
            assert status == 200 and image.startswith(b"\xff\xd8") and image.endswith(b"\xff\xd9")
            assert int(headers["X-Frame-Seq"]) > 0

            stream, stream_response, status, headers, _ = request(
                port, "/api/desktop/stream", token, read_body=False
            )
            assert status == 200 and "multipart/x-mixed-replace" in headers["Content-Type"]
            assert stream_response.readline().strip() == b"--glassframe"
            part_headers = {}
            while True:
                line = stream_response.readline()
                if line in (b"\r\n", b"\n", b""):
                    break
                key, value = line.decode().split(":", 1)
                part_headers[key.lower()] = value.strip()
            frame = stream_response.read(int(part_headers["content-length"]))
            assert part_headers["content-type"] == "image/jpeg"
            assert frame.startswith(b"\xff\xd8") and frame.endswith(b"\xff\xd9")
            stream.close()

            code, conflict = post(
                port,
                token,
                "/api/desktop/input",
                {"action": "pointer-move", "x": 0.5, "y": 0.5},
            )
            assert code == 409 and conflict["lease"] == "agent", (code, conflict)

            code, owner = post(
                port, token, "/api/desktop/lease", {"action": "take-control"}
            )
            assert code == 200 and owner["lease"] == "human", (code, owner)
            code, accepted = post(
                port,
                token,
                "/api/desktop/input",
                {"action": "pointer-move", "x": 0.5, "y": 0.5},
            )
            assert code == 200 and accepted["lease"] == "human", (code, accepted)

            agent_socket = home / "data/glass-core/desktop-agent.sock"
            agent = agent_request(
                agent_socket,
                {"action": "input", "value": {"action": "key", "key": "A", "pressed": True}},
            )
            assert "error" in agent and "lease" in agent["error"], agent
            status, snapshot = post(
                port, token, "/api/desktop/lease", {"action": "return-to-agent"}
            )
            assert status == 200 and snapshot["lease"] == "agent", snapshot
            agent = agent_request(
                agent_socket,
                {"action": "input", "value": {"action": "key", "key": "A", "pressed": True}},
            )
            assert agent.get("sent") is True, agent
            agent_request(
                agent_socket,
                {"action": "input", "value": {"action": "key", "key": "A", "pressed": False}},
            )

            post(port, token, "/api/desktop/lease", {"action": "take-control"})
            viewer, _, status, _, _ = request(
                port, "/api/desktop/stream", token, read_body=False
            )
            assert status == 200
            viewer.close()
            time.sleep(0.25)
            _, _, status, _, body = request(port, "/api/desktop/status", token)
            current = json.loads(body)
            assert status == 200 and current["sessionId"] == session_id
            assert current["lease"] == "human", "disconnect returned the human lease implicitly"

            stopped, result = post(
                port, token, "/api/desktop/session", {"action": "stop"}
            )
            assert stopped == 200 and result["stopped"] is True, result
            _, _, status, _, body = request(port, "/api/desktop/status", token)
            assert status == 200 and json.loads(body)["sessionId"] is None

            started, active = post(
                port, token, "/api/desktop/session", {"action": "start"}
            )
            assert started == 200, active
            auth_file = (
                home
                / "data/glass-core/desktop"
                / active["sessionId"]
                / "Xauthority"
            )
            desktop_dir = auth_file.parent
            socket_path = home / "data/glass-core/desktop-agent.sock"
            stopped = subprocess.run(
                [str(BIN), "connect", "stop"],
                cwd=home,
                env=env,
                capture_output=True,
                text=True,
                timeout=12,
            )
            assert stopped.returncode == 0, stopped.stderr or stopped.stdout
            deadline = time.monotonic() + 10
            remaining = pids_for_display(active["displayId"], auth_file)
            while time.monotonic() < deadline:
                if not remaining:
                    time.sleep(0.3)
                    remaining = pids_for_display(active["displayId"], auth_file)
                    if not remaining:
                        break
                time.sleep(0.1)
                remaining = pids_for_display(active["displayId"], auth_file)
            assert not remaining, (active["displayId"], remaining)
            assert not desktop_dir.exists(), f"desktop storage was not removed: {desktop_dir}"
            assert not socket_path.exists(), f"agent socket was not removed: {socket_path}"
            print("Connect managed desktop auth, live frames, input lease, agent bridge, desktop stop, and gateway cleanup passed")
        finally:
            subprocess.run(
                [str(BIN), "connect", "stop"],
                cwd=home,
                env=env,
                capture_output=True,
                timeout=10,
                check=False,
            )


if __name__ == "__main__":
    main()
