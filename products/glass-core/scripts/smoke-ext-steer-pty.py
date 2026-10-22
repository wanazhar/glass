#!/usr/bin/env python3
"""The agent loop's per-turn cancel token, seen from a real terminal.

One PTY, one local mock provider, one real Python extension process:

1. The user submits a prompt; the provider answers with a tool call into
   an extension tool that sleeps far longer than the steer arrives.
2. While the tool call runs, the user types a correction and presses
   Enter. The composer shows the steering marker.
3. The turn stops on the steer *promptly* — not after the extension's
   full sleep — and the transcript records:
   - one tool result for the abandoned call (no silent hole),
   - the recorded user correction,
   - the steering note, and a tool result that names cancellation
     rather than pretending the tool returned.
4. The next provider request carries the correction: the turn resumed
   with the steer applied, and the extension's eventual output (if the
   process answered anyway) is not what the session recorded as the
   tool result.
"""
import http.server
import json
import os
from pathlib import Path
import shlex
import subprocess
import sys
import tempfile
import threading
import time

from pty_vt import Terminal
from core_binary import core_binary_path

REPO = Path(__file__).resolve().parents[1]
BINARY = Path(os.environ.get("GLASS_CORE_TEST_BINARY", str(core_binary_path('debug'))))

STEER = "actually, stop that and answer briefly"
SLOW_TOOL = "slow_tool"
TOOL_SLEEP = 15.0


class Endpoint(http.server.BaseHTTPRequestHandler):
    requests = []

    def log_message(self, *_):
        pass

    def _respond(self, delta):
        body = ("data: " + json.dumps({"choices": [{"delta": delta}]}) + "\n\n"
                + "data: [DONE]\n\n").encode()
        self.send_response(200)
        self.send_header("Content-Type", "text/event-stream")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def do_GET(self):
        body = json.dumps({"data": [{"id": "mock", "name": "Mock model"}]}).encode()
        self.send_response(200)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def do_POST(self):
        request = json.loads(self.rfile.read(int(self.headers["Content-Length"])))
        type(self).requests.append(request)
        n = len(self.requests)
        if n == 1:
            self._respond({"tool_calls": [{"index": 0, "id": "call_slow", "function": {
                "name": SLOW_TOOL, "arguments": "{}"}}]})
        else:
            # After the steer, the next request's history must carry the
            # correction and the abandoned tool result, and the final reply
            # is the model acknowledging it.
            self._respond({"content": "Steered answering."})


EXTENSION = '''
import json, sys, time
SLOW_TOOL = "slow_tool"
TOOL_SLEEP = 15.0

def send(o):
    sys.stdout.write(json.dumps(o) + "\\n")
    sys.stdout.flush()

for line in sys.stdin:
    line = line.strip()
    if not line:
        continue
    req = json.loads(line)
    t = req.get("type")
    rid = req.get("id")
    if t == "hello":
        send({"id": rid, "protocol": 1, "name": "slow", "version": "1",
              "tools": [{"name": SLOW_TOOL, "description": "sleeps",
                         "schema": {"type": "object"}}]})
    elif t in ("run_tool", "run_tool_input"):
        send({"type": "call_tool", "call_id": "nested-note", "name": "state",
              "arguments": json.dumps({"items": ["retained nested note"], "request_id": "nested-note"})})
        nested = json.loads(sys.stdin.readline())
        time.sleep(TOOL_SLEEP)
        send({"id": rid, "output": "SLOW_LATE_OUTPUT"})
    elif t == "goodbye":
        break
    else:
        send({"id": rid, "allow": True})
'''


def records_of(data_dir):
    lines = []
    for path in sorted(data_dir.rglob("*.jsonl")):
        for line in path.read_text(errors="replace").splitlines():
            if line.strip():
                lines.append(json.loads(line))
    return lines


def record_text(r):
    # JSONL records carry their payload in `t` under a kind tag `k`.
    return r.get("t", "") if isinstance(r, dict) else ""


with tempfile.TemporaryDirectory(prefix="gc-ext-steer-pty-") as temp:
    root = Path(temp)
    config_dir = root / "config/glass-core"
    data_dir = root / "data/glass-core"
    config_dir.mkdir(parents=True)
    data_dir.mkdir(parents=True)
    ext_path = root / "slow_ext.py"
    ext_path.write_text(EXTENSION)
    extension = shlex.join([sys.executable, str(ext_path)])
    endpoint = http.server.ThreadingHTTPServer(("127.0.0.1", 0), Endpoint)
    threading.Thread(target=endpoint.serve_forever, daemon=True).start()
    subprocess.run(["git", "init", "-q"], cwd=root, check=True)
    (config_dir / "config.toml").write_text(
        f'provider="openai"\nmodel="mock"\napi_key_env=""\n'
        f'base_url="http://127.0.0.1:{endpoint.server_port}/v1"\n'
        f"trust=\"always\"\nextensions = [{json.dumps(extension)}]\n"
    )
    env = dict(os.environ, XDG_CONFIG_HOME=str(root / "config"),
               XDG_DATA_HOME=str(root / "data"))
    try:
        terminal = Terminal(root, env, tuple(map(int, os.environ.get("GC_PTY_SIZE", "40x24").split("x"))))
        try:
            terminal.idle()
            terminal.send("use the slow tool\r")
            terminal.visible(SLOW_TOOL, seconds=10)
            # The extension tool card is up (or its start marker is); the
            # tool is now blocked in its sleep. Steer.
            terminal.collect(0.3)
            terminal.send(STEER + "\r")
            terminal.collect(0.1)
            # The turn must stop on the steer promptly — well inside the
            # tool's 15 s sleep — and resume with the correction.
            deadline = time.monotonic() + 8
            while not any(r.get("k") == "assistant" and r.get("t") == "Steered answering." for r in records_of(data_dir) if isinstance(r, dict)):
                assert time.monotonic() < deadline, terminal.vt.text()
                terminal.collect(0.1)
            # The transcript records the steer path verbatim.
            records = records_of(data_dir)
            texts = [json.dumps(r) for r in records]
            bodies = texts
            tool_result = [b for b in bodies if b and "cancelled" in b.lower()]
            assert tool_result, f"no cancellation named in tool results: {bodies}"
            # The slow tool's eventual answer is not recorded as this
            # call's result (the host abandoned the exchange).
            transcript = "\n".join(
                p.read_text(errors="replace")
                for p in data_dir.rglob("*.jsonl"))
            assert "retained nested note" in transcript, "nested state disappeared"
            assert "state/receipt" in transcript, "nested state receipt was not durable"
            assert "SLOW_LATE_OUTPUT" not in transcript, \
                "the abandoned exchange's late output entered the session log"
            # The provider saw exactly two requests: issue the tool call,
            # then resume with the correction.
            assert len(Endpoint.requests) == 2, len(Endpoint.requests)
            resumed = Endpoint.requests[1]["messages"]
            assert any(
                m.get("role") == "user" and STEER in m.get("content", "")
                for m in resumed
            ), f"correction not in the resumed request: {resumed[-1]}"
            assert any(
                m.get("role") == "tool" for m in resumed
            ), "the abandoned tool call has no result in the resumed request"
            terminal.send("\x03")
        finally:
            terminal.close()
    finally:
        endpoint.shutdown()
        endpoint.server_close()
print("PASS extension steer: the per-turn cancel token stops a slow extension "
      "call on steer; the transcript records the cancellation; the steer is in "
      "the resumed request")
