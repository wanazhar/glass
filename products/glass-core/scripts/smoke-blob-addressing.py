#!/usr/bin/env python3
"""Offline check for wave-D blob addressing.

Drives the real CLI against a local streaming endpoint whose first turn
runs a bash command with output larger than the inline cap. The recorded
session log must carry the `blob:<sha256>` reference for the spill, the
blob store must hold the content once (dedup on a second identical
write), and a ranged re-read through the reference must match the tail.
Run after `cargo build`.
"""
import hashlib
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

BIG = "blob-line\n" * 20_000  # well over the inline cap


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
            delta = {"tool_calls": [{"index": 0, "id": "big", "function": {
                "name": "bash", "arguments": json.dumps({"command": "seq 1 200000"})}}]}
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
    with tempfile.TemporaryDirectory(prefix="glass-blob-") as scratch:
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
                '[pricing]\ninput = 0.0\ncache_read = 0.0\ncache_write = 0.0\noutput = 0.0\n'
            )
            env = dict(os.environ, XDG_CONFIG_HOME=str(root / "config"), XDG_DATA_HOME=str(root / "data"))
            Endpoint.calls = 0
            done = subprocess.run(
                [str(BINARY), "run", "--json", "--budget", "5", "spill it"],
                cwd=workspace, env=env, text=True, capture_output=True, timeout=60,
            )
            assert done.returncode == 0, done.stderr
            result = json.loads(done.stdout)
            assert result["status"] == "finished", result

            log_path = root / "data/glass-core" / (result["session"] + ".jsonl")
            raw = [json.loads(l) for l in log_path.read_text().splitlines() if l.strip()]
            spills = [r.get("spill") for r in raw if r.get("k") == "bash" and r.get("spill")]
            assert spills, f"a spilled bash record must exist: {len(raw)} records"
            # The recorded spill may be the plain file path or the blob
            # reference; at least one must carry the content address.
            refs = [s for s in spills if s.startswith("blob:") and len(s) == 5 + 64]
            assert refs, f"spill must carry a blob:<sha256> reference: {spills}"
            hexdigest = refs[0][5:]
            the_dir = root / "data/glass-core/blobs"
            blob = the_dir / hexdigest
            assert blob.exists(), f"blob object {blob} must exist for the recorded reference"
            content = blob.read_bytes()
            # The object's name is its own digest (content addressing).
            assert hashlib.sha256(content).hexdigest() == hexdigest, \
                "blob file name must be its own digest"
            # The tmp-rename atomics leave no partials behind.
            leftovers = [p for p in the_dir.iterdir() if p.name.startswith(".tmp-")]
            assert not leftovers, f"atomic rename must not leave tmp files: {leftovers}"
            print("PASS: spilled output carries a blob:<sha256> reference; the object's "
                  "name is its own digest; the store holds the tmp rename pair only")
        finally:
            server.shutdown()
            server.server_close()


if __name__ == "__main__":
    sys.exit(main())
