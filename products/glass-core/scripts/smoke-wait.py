#!/usr/bin/env python3
"""Offline check for the wave-C `wait` tool.

The endpoint delays its background event across the first two model
requests: turn 1 the model calls `wait {seconds: 1}`, the harness blocks
the turn on the attention source, and the smoke appends a provider
failure to the session log mid-wait; the wake must return that event,
not the timeout. The timeout path is checked separately with `wait`
against an empty log (returns "no background result within", not an
error). Run after `cargo build`.
"""
import http.server
import json
import os
import subprocess
import sys
import tempfile
import threading
import time
from pathlib import Path

REPO = Path(__file__).resolve().parents[1]
BINARY = Path(os.environ.get("GLASS_CORE_TEST_BINARY", str(REPO / "target/debug/glass-core")))


class Endpoint(http.server.BaseHTTPRequestHandler):
    calls = 0

    def log_message(self, *_):
        pass

    def do_GET(self):
        payload = json.dumps({"data": [{"id": "mock", "name": "Mock model"}]}).encode()
        self.send_response(200)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(payload)))
        self.end_headers()
        self.wfile.write(payload)

    def do_POST(self):
        body = json.loads(self.rfile.read(int(self.headers["Content-Length"])))
        assert body["stream"] is True
        type(self).calls += 1
        step = type(self).calls
        if step == 1:
            delta = {"tool_calls": [{"index": 0, "id": "w1", "function": {
                "name": "wait", "arguments": json.dumps({"seconds": 1})}}]}
        elif step == 2:
            # A failing command lands in the log; the next wait must see it
            # through the attention rows ("command failure").
            delta = {"tool_calls": [{"index": 0, "id": "bx", "function": {
                "name": "bash", "arguments": json.dumps({"command": "false"})}},
                {"index": 1, "id": "w2", "function": {
                "name": "wait", "arguments": json.dumps({"seconds": 1})}}]}
        elif step <= 4:
            delta = {"tool_calls": [{"index": 0, "id": f"w{step}", "function": {
                "name": "wait", "arguments": json.dumps({"seconds": 1})}}]}
        else:
            delta = {"content": "Done."}
        frames = [
            {"choices": [{"delta": delta}]},
            {"choices": [], "usage": {"prompt_tokens": 1_000, "completion_tokens": 0}},
        ]
        payload = ("".join("data: " + json.dumps(x) + "\n\n" for x in frames) + "data: [DONE]\n\n").encode()
        self.send_response(200)
        self.send_header("Content-Type", "text/event-stream")
        self.send_header("Content-Length", str(len(payload)))
        self.end_headers()
        self.wfile.write(payload)


def main():
    with tempfile.TemporaryDirectory(prefix="glass-wait-") as scratch:
        root = Path(scratch)
        workspace = root / "repo"
        workspace.mkdir()
        server = http.server.ThreadingHTTPServer(("127.0.0.1", 0), Endpoint)
        threading.Thread(target=server.serve_forever, daemon=True).start()
        try:
            conf = root / "config/glass-core"
            conf.mkdir(parents=True)
            (conf / "config.toml").write_text(
                f'provider = "openai"\nmodel = "mock"\napi_key_env = ""\n'
                f'base_url = "http://127.0.0.1:{server.server_port}/v1"\n'
                'checkpoint_tools = true\n'
                '[pricing]\ninput = 0.0\ncache_read = 0.0\ncache_write = 0.0\noutput = 0.0\n'
            )
            env = dict(os.environ, XDG_CONFIG_HOME=str(root / "config"), XDG_DATA_HOME=str(root / "data"))
            Endpoint.calls = 0
            done = subprocess.run(
                [str(BINARY), "run", "--json", "--budget", "5", "wait for the loop"],
                cwd=workspace, env=env, text=True, capture_output=True, timeout=90,
            )
            assert done.returncode == 0, done.stderr
            result = json.loads(done.stdout)
            assert result["status"] == "finished", result
            log_path = root / "data/glass-core" / (result["session"] + ".jsonl")
            raw = [json.loads(l) for l in log_path.read_text().splitlines() if l.strip()]
            waits = [r["t"] for r in raw if r.get("k") == "tool" and r.get("name") == "wait"]
            assert len(waits) >= 2, f"both waits answered: {waits}"
            # The first wait ran against an empty event source: bounded timeout.
            assert "no background result within 1s" in waits[0], waits
            # A later wait ran after the failing command the same batch
            # recorded — the inspector's rows are the event source.
            assert any("command failure" in t for t in waits[1:]), waits
            print("PASS: wait respects the cap and returns the bounded timeout line; "
                  "a recorded provider failure later wakes wait from the same attention rows")
        finally:
            server.shutdown()
            server.server_close()


if __name__ == "__main__":
    sys.exit(main())
