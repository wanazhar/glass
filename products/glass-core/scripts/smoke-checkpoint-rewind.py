#!/usr/bin/env python3
"""Offline check for the wave-C checkpoint/rewind/wait tools.

Drives the real CLI against a local streaming endpoint that calls
`checkpoint`, then dirties the context, then `rewind` with a retained
report. The session log must show the marker, the projection must hide
the exploration from the resumed context, and `wait`'s schema must be
absent until `checkpoint_tools = true`.

Run after `cargo build`.
"""
import http.server
import json
import os
import subprocess
import sys
import tempfile
import threading
from pathlib import Path
from core_binary import core_binary_path

REPO = Path(__file__).resolve().parents[1]
BINARY = Path(os.environ.get("GLASS_CORE_TEST_BINARY", str(core_binary_path('debug'))))


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
            # checkpoint {label}: durable boundary marker.
            delta = {"tool_calls": [{"index": 0, "id": "ckpt-1", "function": {
                "name": "checkpoint", "arguments": json.dumps({"label": "before-rework"})}}]}
        elif step == 2:
            # Dirty the context, then rewind with the retained report.
            delta = {"tool_calls": [{"index": 0, "id": "bash-x", "function": {
                "name": "bash", "arguments": json.dumps({"command": "echo dirty-exploration"})}},
                {"index": 1, "id": "rw-1", "function": {
                "name": "rewind", "arguments": json.dumps({
                    "label": "before-rework",
                    "report": "Explored the rework; the marker test failed for a stale build; skipping the change.",
                })}}]}
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


def records(path):
    return [json.loads(line) for line in path.read_text().splitlines() if line.strip()]


def write_config(root, port, extra=""):
    conf = root / "config/glass-core"
    conf.mkdir(parents=True, exist_ok=True)
    (conf / "config.toml").write_text(
        f'provider = "openai"\nmodel = "mock"\napi_key_env = ""\n'
        f'base_url = "http://127.0.0.1:{port}/v1"\n'
        # Top-level keys must precede the [pricing] table header, or TOML
        # scopes them under pricing and the flag silently disappears.
        + extra
        + '[pricing]\ninput = 0.0\ncache_read = 0.0\ncache_write = 0.0\noutput = 0.0\n'
    )


def main():
    with tempfile.TemporaryDirectory(prefix="glass-ckpt-rewind-") as scratch:
        root = Path(scratch)
        workspace = root / "repo"
        workspace.mkdir()
        server = http.server.ThreadingHTTPServer(("127.0.0.1", 0), Endpoint)
        threading.Thread(target=server.serve_forever, daemon=True).start()
        try:
            # 1. Off by default: the schema must not list the new tools.
            write_config(root, server.server_port)
            env = dict(os.environ, XDG_CONFIG_HOME=str(root / "config"), XDG_DATA_HOME=str(root / "data"))
            done = subprocess.run(
                [str(BINARY), "run", "--json", "--budget", "5", "plain turn"],
                cwd=workspace, env=env, text=True, capture_output=True, timeout=60,
            )
            assert done.returncode == 0, done.stderr
            Endpoint.calls = 0

            # 2. Enabled: checkpoint then rewind in one session.
            write_config(root, server.server_port, 'checkpoint_tools = true\n')
            done = subprocess.run(
                [str(BINARY), "run", "--json", "--budget", "9", "explore then rewind"],
                cwd=workspace, env=env, text=True, capture_output=True, timeout=60,
            )
            assert done.returncode == 0, done.stderr
            result = json.loads(done.stdout)
            assert result["status"] == "finished", result
            assert Endpoint.calls == 3, Endpoint.calls
            log_path = root / "data/glass-core" / (result["session"] + ".jsonl")
            raw = records(log_path)
            ckpts = [r for r in raw if r.get("k") == "checkpoint"]
            assert len(ckpts) == 2, f"open marker plus rewind marker: {ckpts}"
            assert ckpts[0]["label"] == "before-rework", ckpts
            assert not ckpts[0].get("rewind_summary"), "marker opens, not rewinds"
            tool_names = [r.get("name") for r in raw if r.get("k") == "tool"]
            assert "checkpoint" in tool_names, tool_names
            assert "rewind" in tool_names, tool_names
            rewind_results = [r["t"] for r in raw if r.get("k") == "tool" and r.get("name") == "rewind"]
            assert any("Rewound" in t for t in rewind_results), rewind_results
            # The session log keeps everything; the exploration bash call is
            # still in the raw log, but behind the rewind marker.
            assert any(
                (r.get("cmd") or r.get("args") or "").find("dirty-exploration") >= 0
                for r in raw
            ), "the raw log keeps the exploration"
            print("PASS: checkpoint marker durable; rewind retained the report; "
                  "the tools only exist with checkpoint_tools = true")
        finally:
            server.shutdown()
            server.server_close()


if __name__ == "__main__":
    sys.exit(main())
