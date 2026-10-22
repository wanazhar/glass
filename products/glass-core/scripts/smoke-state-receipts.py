#!/usr/bin/env python3
"""Offline check for `state` request ids and durable receipts.

Drives the real CLI against a local streaming endpoint that writes the same
`state` request twice, reads the revision back, and then reuses that id for
different notes. The session log must show one state record and one receipt,
the retry answered from the receipt, and the conflicting reuse refused.

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

REPO = Path(__file__).resolve().parents[1]
BINARY = Path(os.environ.get("GLASS_CORE_TEST_BINARY", str(REPO / "target/debug/glass-core")))

RETRY_ARGS = {"request_id": "req-1", "items": ["only once"]}
CONFLICT_ARGS = {"request_id": "req-1", "items": ["something else entirely"]}


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
            delta = {"tool_calls": [{"index": 0, "id": "state-write", "function": {
                "name": "state", "arguments": json.dumps(RETRY_ARGS)}}]}
        elif step == 2:
            # Exactly what a retrying client resends, byte for byte.
            delta = {"tool_calls": [{"index": 0, "id": "state-retry", "function": {
                "name": "state", "arguments": json.dumps(RETRY_ARGS)}}]}
        elif step == 3:
            delta = {"tool_calls": [{"index": 0, "id": "state-revision", "function": {
                "name": "state", "arguments": json.dumps({"revision": True})}}]}
        elif step == 4:
            # The same id, different notes: a client error, not a replay.
            delta = {"tool_calls": [{"index": 0, "id": "state-conflict", "function": {
                "name": "state", "arguments": json.dumps(CONFLICT_ARGS)}}]}
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


def records(log):
    return [json.loads(line) for line in log.splitlines() if line.strip()]


def main():
    with tempfile.TemporaryDirectory(prefix="glass-state-receipts-") as scratch:
        root = Path(scratch)
        workspace = root / "repo"
        workspace.mkdir()
        server = http.server.ThreadingHTTPServer(("127.0.0.1", 0), Endpoint)
        threading.Thread(target=server.serve_forever, daemon=True).start()
        try:
            conf = root / "config/glass-core"
            conf.mkdir(parents=True)
            (conf / "config.toml").write_text(
                f'provider = "openai"\nmodel = "mock"\napi_key_env = ""\nbase_url = "http://127.0.0.1:{server.server_port}/v1"\n'
                '[pricing]\ninput = 0.0\ncache_read = 0.0\ncache_write = 0.0\noutput = 0.0\n'
            )
            env = dict(os.environ, XDG_CONFIG_HOME=str(root / "config"), XDG_DATA_HOME=str(root / "data"))
            Endpoint.calls = 0
            done = subprocess.run(
                [str(BINARY), "run", "--json", "--budget", "5", "record the plan, twice"],
                cwd=workspace, env=env, text=True, capture_output=True, timeout=60,
            )
            assert done.returncode == 0, done.stderr
            result = json.loads(done.stdout)
            assert result["status"] == "finished", result
            assert Endpoint.calls == 5, Endpoint.calls

            log_path = root / "data/glass-core" / (result["session"] + ".jsonl")
            raw = records(log_path.read_text())

            states = [r for r in raw if r.get("k") == "state"]
            receipts = [r for r in raw if r.get("k") == "custom" and r.get("c") == "state/receipt"]
            assert len(states) == 1, f"a retry must not write state twice: {states}"
            assert states[0]["items"] == RETRY_ARGS["items"], states
            assert len(receipts) == 1, f"one accepted request, one receipt: {receipts}"
            receipt = json.loads(receipts[0]["d"])
            assert receipt["request"] == "req-1", receipt
            assert receipt["actor"] == result["session"], receipt
            assert receipt["items"] == 1, receipt

            # The retry was answered from the receipt, not applied again.
            tool_results = [r["t"] for r in raw if r.get("k") == "tool" and r.get("name") == "state"]
            assert any("already applied" in t for t in tool_results), tool_results
            # The conflicting reuse was refused and changed nothing.
            assert any("already applied with different content" in t for t in tool_results), tool_results
            assert not any("something else entirely" in json.dumps(r) for r in states), states

            # The revision read-back states real ownership and the revision
            # a caller must present.
            read_back = json.loads(next(
                (t for t in tool_results if t.startswith("{")),
                json.dumps({"unreachable": tool_results}),
            ))
            assert read_back["requires_revision"] == read_back["revision"], read_back
            assert read_back["owner"]["session"] == result["session"], read_back
            assert read_back["owner"]["durable_writer"] is True, read_back
            assert read_back["receipt_ledger"]["bound"] >= 1, read_back
            assert read_back["receipt_ledger"]["retained"] == 1, read_back
            print("PASS: retried state write answered from its durable receipt without double-applying; "
                  "same id with different notes refused; receipt names the request, the accepting session and the effect; "
                  "revision read-back reports computed ownership and the revision to present")
        finally:
            server.shutdown()
            server.server_close()


if __name__ == "__main__":
    sys.exit(main())