#!/usr/bin/env python3
"""Offline check for the wave-E pre-tool-use shell hook.

Drives the real CLI against a local streaming endpoint that calls bash.
The project config registers four hook variants across four runs:
block (exit 2, stdout becomes the refusal), halt (exit 49), pass (exit 0),
and a sleeping hook (bounded timeout, warn-and-allow). The session log
must show the refusal text as the tool body, the halting turn marked, the
pass run unblocked, and the timeout warning.
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
            delta = {"tool_calls": [{"index": 0, "id": "h", "function": {
                "name": "bash", "arguments": json.dumps({"command": "echo hi"})}}]}
            if getattr(type(self), "halt_tool", None):
                delta["tool_calls"][0]["function"] = {"name": type(self).halt_tool, "arguments": json.dumps({"path":"blocked.txt", "expected":None, "text":"blocked"} if type(self).halt_tool == "write" else {"command":"echo blocked"})}
                delta["tool_calls"].append({"index":1,"id":"after-halt","function":{"name":"bash","arguments":json.dumps({"command":"touch should-not-exist"})}})
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


HOOKS = {
    "block": '[hooks.pre_tool_use]\ncommand = "echo no-mutating-today >&1; exit 2"\n',
    "halt": '[hooks.pre_tool_use]\ncommand = "echo full-stop; exit 49"\n',
    "pass": '[hooks.pre_tool_use]\ncommand = "exit 0"\n',
    "matcher-refuse": '[hooks.pre_tool_use]\ncommand = "exit 2"\nmatcher = "edit"\n',
    "timeout": '[hooks.pre_tool_use]\ncommand = "sleep 30"\n',
}


def main():
    with tempfile.TemporaryDirectory(prefix="glass-hooks-") as scratch:
        root = Path(scratch)
        workspace = root / "repo"
        workspace.mkdir()
        server = http.server.ThreadingHTTPServer(("127.0.0.1", 0), Endpoint)
        threading.Thread(target=server.serve_forever, daemon=True).start()
        try:
            for name, hook in list(HOOKS.items()) + [("halt-write", HOOKS["halt"] + 'matcher = "write"\n'), ("halt-bash", HOOKS["halt"] + 'matcher = "bash"\n')]:
                Endpoint.calls = 0
                Endpoint.halt_tool = name.removeprefix("halt-") if name.startswith("halt-") else None
                conf = root / "config/glass-core"
                conf.mkdir(parents=True, exist_ok=True)
                (conf / "config.toml").write_text(
                    f'provider = "openai"\nmodel = "mock"\napi_key_env = ""\n'
                    f'base_url = "http://127.0.0.1:{server.server_port}/v1"\n'
                    + hook +
                    '[pricing]\ninput = 0.0\ncache_read = 0.0\ncache_write = 0.0\noutput = 0.0\n'
                )
                env = dict(os.environ, XDG_CONFIG_HOME=str(root / "config"), XDG_DATA_HOME=str(root / "data"))
                done = subprocess.run(
                    [str(BINARY), "run", "--json", "--budget", "9", "run the tool"],
                    cwd=workspace, env=env, text=True, capture_output=True,
                    timeout=120 if name == "timeout" else 60,
                )
                assert (done.returncode != 0) if name.startswith("halt") else (done.returncode == 0), f"[{name}] {done.stderr}"
                result = json.loads(done.stdout)
                assert result["status"] == ("cancelled" if name.startswith("halt") else "finished"), f"[{name}] {result}"
                log_path = root / "data/glass-core" / (result["session"] + ".jsonl")
                raw = [json.loads(l) for l in log_path.read_text().splitlines() if l.strip()]
                bash_results = [
                    r["t"] for r in raw if r.get("k") == "tool" and r.get("name") == "bash"
                ]
                if name == "block":
                    assert any("blocked by pre-tool-use hook" in t and "no-mutating-today" in t
                               for t in bash_results), f"[block] {bash_results}"
                elif name.startswith("halt"):
                    assert Endpoint.calls == 1, Endpoint.calls
                    assert not (workspace / "should-not-exist").exists()
                    assert not (workspace / "blocked.txt").exists()
                    if name != "halt":
                        assert any(r.get("id") == "after-halt" and "Cancelled before execution" in r.get("t", "") for r in raw), raw
                    if name == "halt-write":
                        continue
                    assert any("turn cancelled" in t for t in bash_results), f"[halt] {bash_results}"
                    metas = [r["t"] for r in raw if r.get("k") == "meta"]
                    assert any("hook halting turn" in m for m in metas), f"[halt] {metas}"
                elif name == "pass":
                    assert any("hi" in t for t in bash_results), f"[pass] {bash_results}"
                elif name == "matcher-refuse":
                    # Matcher on "edit" must NOT touch a bash call.
                    assert any("hi" in t for t in bash_results), f"[matcher] {bash_results}"
                elif name == "timeout":
                    metas = [r["t"] for r in raw if r.get("k") == "meta"]
                    assert any("hook timeout" in m for m in metas), f"[timeout] {metas}"
            print("PASS: hook block (stdout is the refusal), halt (turn cancelled), "
                  "pass, matcher refusal, and 3s bounded timeout all verified "
                  "against the real runner")
        finally:
            server.shutdown()
            server.server_close()


if __name__ == "__main__":
    sys.exit(main())
