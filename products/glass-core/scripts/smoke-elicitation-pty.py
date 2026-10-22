#!/usr/bin/env python3
"""Exercise MCP input through the real interactive agent and shared key reader."""
import http.server
import json
import os
from pathlib import Path
import shlex
import sys
import tempfile
import threading
import time
import subprocess

from pty_vt import Terminal
from core_binary import core_binary_path

REPO = Path(__file__).resolve().parents[1]
BINARY = Path(os.environ.get("GLASS_CORE_TEST_BINARY", str(core_binary_path('debug'))))
BRIDGE = REPO / "extensions/mcp-bridge/mcp_bridge.py"
FIXTURE = REPO / "scripts/mcp_fixture_server.py"
PRIVATE = "PRIVATE_FORM_VALUE_DO_NOT_PERSIST"


class Endpoint(http.server.BaseHTTPRequestHandler):
    requests = 0

    def log_message(self, *_):
        pass

    def do_GET(self):
        body = json.dumps({"data": [{"id": "fixture", "name": "fixture"}]}).encode()
        self.send_response(200)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def do_POST(self):
        type(self).requests += 1
        request = json.loads(self.rfile.read(int(self.headers["Content-Length"])))
        if type(self).requests == 1:
            delta = {"tool_calls": [{"index": 0, "id": "ask-1", "function": {
                "name": "mcp__fixture__ask", "arguments": json.dumps({"original": "unchanged"})
            }}]}
        else:
            # The bridge fixture accepts the response only if it preserved the
            # original arguments, echoed requestState, and allocated a fresh RPC id.
            results = [m for m in request["messages"] if m.get("role") == "tool"]
            assert results and "finished without echoing input" in results[-1]["content"], results
            delta = {"content": "Elicitation completed."}
        payload = ("data: " + json.dumps({"choices": [{"delta": delta}]}) + "\n\n"
                   + "data: [DONE]\n\n").encode()
        self.send_response(200)
        self.send_header("Content-Type", "text/event-stream")
        self.send_header("Content-Length", str(len(payload)))
        self.end_headers()
        self.wfile.write(payload)


with tempfile.TemporaryDirectory(prefix="gc-elicitation-pty-") as temp:
    root = Path(temp)
    config_dir = root / "config/glass-core"
    data_dir = root / "data/glass-core"
    config_dir.mkdir(parents=True)
    data_dir.mkdir(parents=True)
    mcp_config = root / "mcp.json"
    mcp_config.write_text(json.dumps({"servers": {"fixture": {"command": [
        sys.executable, str(FIXTURE), "--elicitation"
    ]}}}))
    extension = shlex.join([sys.executable, str(BRIDGE)])
    endpoint = http.server.ThreadingHTTPServer(("127.0.0.1", 0), Endpoint)
    threading.Thread(target=endpoint.serve_forever, daemon=True).start()
    subprocess.run(["git", "init", "-q"], cwd=root, check=True)
    (config_dir / "config.toml").write_text(
        f'provider="openai"\nmodel="fixture"\napi_key_env=""\n'
        f'base_url="http://127.0.0.1:{endpoint.server_port}/v1"\n'
        f"extensions = [{json.dumps(extension)}]\n"
    )
    env = dict(os.environ, XDG_CONFIG_HOME=str(root / "config"),
               XDG_DATA_HOME=str(root / "data"), GC_MCP_CONFIG=str(mcp_config))
    try:
        for size in ((40, 24), (20, 8)):
            Endpoint.requests = 0
            terminal = Terminal(root, env, size)
            try:
                terminal.idle()
                terminal.send("ask fixture\r")
                terminal.visible("Choose a label")
                # First card explains the sensitivity limits; dismiss it and
                # wait for the focused field before entering any private data.
                terminal.send("\r")
                terminal.visible("❯ │")
                # Type into the callback's field modal; the main composer is
                # not a second stdin reader and remains untouched.
                for char in PRIVATE:
                    terminal.send(char)
                    time.sleep(0.01)
                terminal.send("\r")
                terminal.visible("Review")
                if size[0] == 40:
                    terminal.visible('label: "PRIVATE_FORM_VALUE_DO_NOT_')
                terminal.send("\r")  # submit reviewed response
                deadline = time.monotonic() + 8
                while "state=done:" not in terminal.seen:
                    assert time.monotonic() < deadline, terminal.vt.text()
                    terminal.collect(0.05)
                if size[0] == 40:
                    assert "Elicitation completed." in terminal.vt.text()
                transcript = "\n".join(
                    p.read_text(errors="replace")
                    for p in data_dir.rglob("*") if p.is_file()
                )
                assert PRIVATE not in transcript, "private elicitation value entered session storage"
                assert Endpoint.requests == 2, f"provider did not receive the retried result: {Endpoint.requests}"
                terminal.send("\x03")
            finally:
                terminal.close()
    finally:
        endpoint.shutdown()
        endpoint.server_close()
print("PASS MCP elicitation: real agent callback/shared reader at 40x24 and 20x8; private value absent from session files")
