#!/usr/bin/env python3
"""Actual-binary regression: tiny incremental job polls preserve split Unicode."""
import http.server, json, os, re, subprocess, tempfile, threading
from pathlib import Path
BIN = Path(os.environ.get("GLASS_CORE_TEST_BINARY", Path(__file__).resolve().parents[1] / "target/debug/glass-core"))
class Provider(http.server.BaseHTTPRequestHandler):
    output = ""
    def log_message(self, *_): pass
    def do_POST(self):
        body = json.loads(self.rfile.read(int(self.headers["Content-Length"])))
        results = [m["content"] for m in body["messages"] if m["role"] == "tool"]
        if not results:
            args = {"command": "printf 'A\\342'; sleep 0.1; printf '\\202\\254😀Z'", "job": {"action": "start"}}
        elif len(results) == 1:
            args = {"job": {"action": "poll", "id": 1, "offset": 0, "limit": 1}}
        else:
            assert "[job metadata] " in results[-1], repr(results)
            meta = json.loads(results[-1].split("[job metadata] ")[1])
            text = re.split(r"\[job 1 · [^\n]+\]\n", results[-1], maxsplit=1)[1].split("\n[job metadata] ")[0]
            type(self).output += text
            if meta["state"] == "exited" and not meta["draining"] and meta["next_offset"] == meta["captured_bytes"]:
                assert type(self).output == "A€😀Z", repr(type(self).output)
                args = None
            else:
                args = {"job": {"action": "poll", "id": 1, "offset": meta["next_offset"], "limit": 1}}
        delta = {"content": "UNICODE_JOB_DONE"} if args is None else {"tool_calls": [{"index": 0, "id": f"call-{len(results)}", "function": {"name": "bash", "arguments": json.dumps(args)}}]}
        payload = ("data: " + json.dumps({"choices": [{"delta": delta}]}) + "\n\ndata: [DONE]\n\n").encode()
        self.send_response(200); self.send_header("Content-Type", "text/event-stream"); self.end_headers(); self.wfile.write(payload)
with tempfile.TemporaryDirectory(prefix="gc-unicode-job-") as tmp:
    root = Path(tmp); cfg = root / "config/glass-core"; cfg.mkdir(parents=True)
    server = http.server.ThreadingHTTPServer(("127.0.0.1", 0), Provider)
    threading.Thread(target=server.serve_forever, daemon=True).start()
    (cfg / "config.toml").write_text(f'provider="openai"\nmodel="mock"\napi_key_env=""\nbase_url="http://127.0.0.1:{server.server_port}/v1"\ntrust="always"\nmax_rounds=100\n')
    env = dict(os.environ, XDG_CONFIG_HOME=str(root / "config"), XDG_DATA_HOME=str(root / "data"))
    p = subprocess.run([str(BIN), "run", "--json", "exercise Unicode job polling"], cwd=root, env=env, text=True, capture_output=True, timeout=30)
    assert p.returncode == 0 and "UNICODE_JOB_DONE" in p.stdout, (p.returncode, p.stdout, p.stderr)
    server.shutdown()
print("PASS: split UTF-8 writes and one-byte job poll cursors preserve euro and emoji output")
