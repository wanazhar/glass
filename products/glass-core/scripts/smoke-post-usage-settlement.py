#!/usr/bin/env python3
"""Repeated replay-backed turns must settle and answer after provider usage.

Each turn runs through the real `glass-core runtime` against a local
OpenAI-compatible replay endpoint that always reports usage. The check fails
closed: if usage is observed and settlement or the prompt response does not
follow within a fixed bound, the run stops with a typed stall error and the
privacy-safe phase trace instead of waiting indefinitely.
"""
import glob
import http.server
import importlib.util
import json
import os
from pathlib import Path
import queue
import tempfile
import threading
import time
from core_binary import core_binary_path

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location("host", ROOT / "examples/runtime_host.py")
host = importlib.util.module_from_spec(spec)
spec.loader.exec_module(host)
BIN = Path(os.environ.get("GLASS_CORE_TEST_BINARY", core_binary_path("release"))).resolve()
TURNS = int(os.environ.get("GLASS_POST_USAGE_TURNS", "36"))
POST_USAGE_BOUND_S = float(os.environ.get("GLASS_POST_USAGE_BOUND_S", "10"))
TURN_BOUND_S = 30.0
SCENARIOS = ("text", "tool", "late-usage", "many-deltas", "two-tools")
EXPECTED_REQUESTS = {"text": 1, "tool": 2, "late-usage": 1, "many-deltas": 1, "two-tools": 3}


class PostUsageStall(AssertionError):
    pass


def observe_turn(receive, turn_id, tool_result, *, post_usage_bound=POST_USAGE_BOUND_S, turn_bound=TURN_BOUND_S):
    """Return the ordered event kinds of one turn, failing closed on a stall."""
    started = time.monotonic()
    kinds = []
    last_usage = None
    settled_at = None
    while True:
        now = time.monotonic()
        remaining = turn_bound - (now - started)
        if last_usage is not None and settled_at is None:
            remaining = min(remaining, post_usage_bound - (now - last_usage))
        if settled_at is not None:
            remaining = min(remaining, post_usage_bound - (now - settled_at))
        try:
            if remaining <= 0:
                raise queue.Empty
            message = receive(remaining)
        except queue.Empty:
            if settled_at is not None:
                raise PostUsageStall(f"{turn_id}: settled without a prompt response within {post_usage_bound}s; events={kinds}") from None
            if last_usage is not None:
                raise PostUsageStall(f"{turn_id}: usage observed without settlement within {post_usage_bound}s; events={kinds}") from None
            raise AssertionError(f"{turn_id}: no usage or settlement within {turn_bound}s; events={kinds}") from None
        if message.get("method") == "tool/execute":
            tool_result(message)
            kinds.append("tool/execute")
            last_usage = None
            continue
        if message.get("method") == "runtime/event":
            kind = message["params"]["type"]
            kinds.append(kind)
            if kind == "usage":
                last_usage = time.monotonic()
            elif kind == "settled":
                assert settled_at is None, f"{turn_id}: duplicate settlement; events={kinds}"
                settled_at = time.monotonic()
            elif settled_at is None:
                last_usage = None
            continue
        if message.get("id") == turn_id:
            assert "result" in message, message
            assert settled_at is not None, f"{turn_id}: response before settlement; events={kinds}"
            assert message["result"].get("settled") is True, message
            kinds.append("response")
            return kinds
        raise AssertionError(f"{turn_id}: unexpected frame {message}")


def self_test():
    """The checker must reject a stalled stream rather than wait on it."""
    def stream(frames):
        pending = list(frames)

        def receive(timeout):
            if pending:
                return pending.pop(0)
            time.sleep(min(timeout, 0.05))
            raise queue.Empty
        return receive

    usage = {"method": "runtime/event", "params": {"type": "usage"}}
    settled = {"method": "runtime/event", "params": {"type": "settled"}}
    for frames, expected in (([usage], "usage observed without settlement"), ([usage, settled], "settled without a prompt response")):
        try:
            observe_turn(stream(frames), "host-0", None, post_usage_bound=0.05, turn_bound=1)
        except PostUsageStall as error:
            assert expected in str(error), error
        else:
            raise AssertionError(f"checker accepted a stalled stream: {expected}")
    complete = [usage, settled, {"id": "host-0", "result": {"settled": True}}]
    assert observe_turn(stream(complete), "host-0", None, post_usage_bound=0.05, turn_bound=1) == ["usage", "settled", "response"]


served = []
served_lock = threading.Lock()


class Provider(http.server.BaseHTTPRequestHandler):
    def log_message(self, *_):
        pass

    def do_POST(self):
        body = json.loads(self.rfile.read(int(self.headers["Content-Length"])))
        messages = body["messages"]
        last_user = max(index for index, message in enumerate(messages) if message["role"] == "user")
        scenario = str(messages[last_user]["content"]).rsplit(" ", 1)[-1]
        results = [m for m in messages[last_user + 1:] if m["role"] == "tool"]
        with served_lock:
            served.append(scenario)
        frames = []
        if scenario == "tool" and not results:
            frames.append({"tool_calls": [{"index": 0, "id": "call-0", "function": {"name": "inspect", "arguments": "{}"}}]})
        elif scenario == "two-tools" and len(results) < 2:
            index = len(results)
            frames.append({"tool_calls": [{"index": 0, "id": f"call-{index}", "function": {"name": "inspect", "arguments": "{}"}}]})
        elif scenario == "many-deltas":
            frames.extend({"content": "x"} for _ in range(400))
        else:
            frames.append({"content": f"DONE {scenario}"})
        self.send_response(200)
        self.send_header("Content-Type", "text/event-stream")
        self.end_headers()
        try:
            for delta in frames:
                self.wfile.write(("data: " + json.dumps({"choices": [{"delta": delta}]}) + "\n\n").encode())
            self.wfile.flush()
            if scenario == "late-usage":
                time.sleep(0.4)
            usage = {"choices": [], "usage": {"prompt_tokens": 10, "completion_tokens": 3}}
            self.wfile.write(("data: " + json.dumps(usage) + "\n\ndata: [DONE]\n\n").encode())
            self.wfile.flush()
        except (BrokenPipeError, ConnectionResetError):
            pass


def phase_tail(trace_dir):
    lines = []
    for path in sorted(glob.glob(str(Path(trace_dir) / "*.jsonl"))):
        for line in Path(path).read_text().splitlines()[-12:]:
            record = json.loads(line)
            lines.append(f"{Path(path).name}: {record.get('component')} {record.get('phase')} {record.get('class')}")
    return "\n".join(lines)


def main():
    self_test()
    server = http.server.ThreadingHTTPServer(("127.0.0.1", 0), Provider)
    threading.Thread(target=server.serve_forever, daemon=True).start()
    with tempfile.TemporaryDirectory(prefix="gc-post-usage-") as tmp:
        root = Path(tmp)
        config = root / "config/glass-core"
        config.mkdir(parents=True)
        (config / "config.toml").write_text(
            f'provider="openai"\nmodel="replay"\napi_key_env=""\nbase_url="http://127.0.0.1:{server.server_port}/v1"\nadaptive_pacing=false\n'
        )
        trace_dir = root / "trace"
        trace_dir.mkdir()
        env = dict(
            os.environ,
            XDG_CONFIG_HOME=str(root / "config"),
            XDG_DATA_HOME=str(root / "data"),
            GLASS_RUNTIME_DIAGNOSTIC_TRACE=str(trace_dir / "runtime"),
        )
        client = host.RuntimeClient(BIN, root, env)

        def call(method, params):
            return client.wait(client.request(method, params), timeout=TURN_BOUND_S)

        def tool_result(message):
            client.result(message["id"], {"content": [{"type": "json", "value": {"ok": True}}]})

        options = {"cwd": str(root), "toolMode": "host", "tools": [{"name": "inspect", "inputSchema": {"type": "object"}}], "maxTokens": 256}
        try:
            call("initialize", {"protocolVersion": 1, "storageDir": str(root / "sessions"), "cancelGraceMs": 500})
            sessions = [call("session/new", options) for _ in range(2)]
            for index in range(TURNS):
                scenario = SCENARIOS[index % len(SCENARIOS)]
                session = sessions[index % len(sessions)]
                with served_lock:
                    before = len(served)
                turn = client.request("turn/prompt", {**session, "text": f"turn {index} {scenario}"})
                try:
                    kinds = observe_turn(client.receive, turn, tool_result)
                except AssertionError as error:
                    raise AssertionError(f"{error}\nphase trace tail:\n{phase_tail(trace_dir)}") from None
                with served_lock:
                    requests = len(served) - before
                assert requests == EXPECTED_REQUESTS[scenario], (index, scenario, requests, kinds)
                assert kinds.count("tool/execute") == requests - 1, (index, scenario, kinds)
                assert "usage" in kinds, (index, scenario, kinds)
                last_usage = len(kinds) - 1 - kinds[::-1].index("usage")
                assert kinds[last_usage + 1:] == ["settled", "response"], (index, scenario, kinds)
            stats = call("session/stats", sessions[0])
            assert stats["reportedUsage"]["samples"] >= TURNS // 2, stats
            call("shutdown", {})
            client.process.wait(timeout=5)
        finally:
            client.close()
            server.shutdown()
    print(f"PASS: {TURNS} replay-backed turns each settled and answered within {POST_USAGE_BOUND_S}s of usage ({len(served)} provider requests); a stalled stream fails closed")


if __name__ == "__main__":
    main()
