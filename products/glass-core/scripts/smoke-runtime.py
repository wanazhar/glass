#!/usr/bin/env python3
"""Offline host-runtime contract checks on the actual release binary."""
import http.server
import importlib.util
import json
import os
import tempfile
import threading
import time
from pathlib import Path
from core_binary import core_binary_path

root = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location("runtime_host", root / "examples/runtime_host.py")
host = importlib.util.module_from_spec(spec)
spec.loader.exec_module(host)
binary = Path(os.environ.get("GLASS_CORE_TEST_BINARY", core_binary_path('release'))).resolve()
requests = []
failures = []
mode = "tool"


class Endpoint(http.server.BaseHTTPRequestHandler):
    def log_message(self, *_):
        pass

    def do_POST(self):
        body = json.loads(self.rfile.read(int(self.headers["Content-Length"])))
        requests.append(body)
        try:
            names = [t["function"]["name"] for t in body.get("tools", [])]
            assert names == (["bash"] if mode == "shadow" else ["bash", "state", "read", "edit", "write"] if mode == "local" else ["host_read", "ask_user_question"]), names
            assert "Host instruction marker" in body["messages"][0]["content"]
            if mode != "local":
                assert "neuralnets" not in body["messages"][0]["content"]
            results = [m for m in body["messages"] if m["role"] == "tool"]
            user_text = "\n".join(str(m.get("content", "")) for m in body["messages"] if m["role"] == "user")
            if mode == "shadow" and not results:
                payload = {"tool_calls": [{"index": 0, "id": "shadow-call",
                                          "function": {"name": "bash", "arguments": '{"command":"touch SHOULD_NOT_EXIST"}'}}]}
            elif mode == "large" and not results:
                payload = {"tool_calls": [{"index": 0, "id": "large-call",
                                          "function": {"name": "host_read", "arguments": '{"path":"x"}'}}]}
            elif mode == "cancel":
                payload = {"tool_calls": [{"index": 0, "id": "cancel-call",
                                          "function": {"name": "host_read", "arguments": '{"path":"x"}'}}]}
            elif mode == "steer" and "CORRECTED" not in user_text:
                payload = {"tool_calls": [{"index": 0, "id": "steer-call",
                                          "function": {"name": "host_read", "arguments": '{"path":"x"}'}}]}
            elif not results and mode == "tool":
                payload = {"tool_calls": [{"index": 0, "id": "real-read-id",
                                          "function": {"name": "host_read", "arguments": '{"path":"x"}'}}]}
            elif len(results) == 1 and mode == "tool":
                payload = {"tool_calls": [{"index": 0, "id": "real-question-id",
                                          "function": {"name": "ask_user_question", "arguments": '{"question":"Which style?"}'}}]}
            else:
                payload = {"content": "RUNTIME_DONE"}
        except Exception as exc:
            failures.append(repr(exc))
            payload = {"content": "MOCK_ASSERTION_FAILED"}
        self.send_response(200)
        self.send_header("Content-Type", "text/event-stream")
        self.end_headers()
        self.wfile.write(("data: " + json.dumps({"choices": [{"delta": payload}]}) +
                         "\n\ndata: [DONE]\n\n").encode())
        self.wfile.flush()


server = http.server.ThreadingHTTPServer(("127.0.0.1", 0), Endpoint)
threading.Thread(target=server.serve_forever, daemon=True).start()
tools = [
    {"name": "host_read", "inputSchema": {"type": "object", "properties": {"path": {"type": "string"}}, "required": ["path"]}},
    {"name": "ask_user_question", "inputSchema": {"type": "object", "properties": {"question": {"type": "string"}}, "required": ["question"]}},
]
with tempfile.TemporaryDirectory(prefix="gc-runtime-contract-") as tmp:
    tmp = Path(tmp)
    config = tmp / "config/glass-core"
    config.mkdir(parents=True)
    (config / "config.toml").write_text(
        f'provider = "openai"\nbase_url = "http://127.0.0.1:{server.server_port}/v1"\n'
        'model = "mock"\napi_key_env = "GC_RUNTIME_TEST_KEY"\nadaptive_pacing = false\nimage_input = false\n')
    env = dict(os.environ, XDG_CONFIG_HOME=str(tmp / "config"), XDG_DATA_HOME=str(tmp / "data"), GC_RUNTIME_TEST_KEY="test")
    client = host.RuntimeClient(binary, tmp, env)
    call = lambda name, params: client.wait(client.request(name, params))
    storage = tmp / "sessions"
    try:
        ident = client.request("session/new", {"cwd": str(tmp)})
        message = client.receive()
        assert message["id"] == ident and "error" in message
        init = call("initialize", {"protocolVersion": 1, "storageDir": str(storage)})
        assert init["capabilities"]["hostTools"] and init["capabilities"]["completion"]
        params = {"cwd": str(tmp), "toolMode": "host", "tools": tools, "instructions": "Host instruction marker"}
        session = call("session/new", params)
        sid = session["sessionId"]
        events = []
        calls = []
        def execute(tool):
            calls.append(tool)
            return "CUSTOM ANSWER violet" if tool["name"] == "ask_user_question" else "host file content"
        callid = client.request("turn/prompt", {**session, "text": "Inspect then ask me"})
        client.wait(callid, tool=execute, event=events.append)
        assert [t["callId"] for t in calls] == ["real-read-id", "real-question-id"]
        assert [e["data"]["callId"] for e in events if e["type"] == "tool_end"] == ["real-read-id", "real-question-id"]
        assert any(e["type"] == "settled" and e["data"]["reason"] == "completed" for e in events)
        assert any("CUSTOM ANSWER violet" in str(m) for m in requests[-1]["messages"])
        inspection = call("session/inspect", session)
        assert inspection["recordCount"] > 0
        fork = call("session/fork", {**session, "throughTurn": 1})
        assert (storage / (fork["sessionId"] + ".jsonl")).exists()
        call("session/rewind", {**session, "throughTurn": 0})
        call("session/close", session)
        resumed = call("session/resume", {**params, **session})
        assert resumed == session
        listing = call("session/list", {})
        assert len(listing["sessions"]) == 2

        for action in ("cancel", "steer"):
            mode = action
            active = call("session/new", params)
            turn = client.request("turn/prompt", {**active, "text": "Start"})
            controls = set()
            settled = None
            while True:
                m = client.receive()
                if m.get("method") == "tool/execute":
                    if action == "cancel":
                        controls.add(client.request("turn/cancel", {**active, "turnId": turn}))
                    else:
                        controls.add(client.request("turn/steer", {**active, "turnId": turn, "text": "CORRECTED"}))
                elif m.get("method") == "runtime/event":
                    if m["params"]["type"] == "settled":
                        settled = m["params"]["data"]["reason"]
                elif m.get("id") == turn:
                    assert "result" in m, m
                    break
                elif m.get("id") in controls:
                    assert m["result"]["accepted"]
                else:
                    raise AssertionError(m)
            assert settled == ("cancelled" if action == "cancel" else "completed"), settled
        assert not failures, failures
        mode = "large"
        large_session = call("session/new", params)
        large_events = []
        large_text = "\x01" * 100_000
        client.wait(client.request("turn/prompt", {**large_session, "text": "Large result"}),
                    tool=lambda _: large_text, event=large_events.append)
        chunks = [e["data"] for e in large_events if e["type"] == "tool_end"]
        assert "".join(c["text"] for c in chunks) == large_text
        assert len(chunks) == 2 and chunks[-1]["chunkFinal"]

        mode = "text"
        queued = call("session/new", params)
        first = client.request("turn/prompt", {**queued, "text": "First"})
        follow = client.request("turn/followUp", {**queued, "text": "Second"})
        responses = set()
        while len(responses) < 2:
            m = client.receive()
            if m.get("id") in (first, follow):
                assert "result" in m, m
                responses.add(m["id"])
            else:
                assert m.get("method") == "runtime/event", m

        mode = "shadow"
        shadow = call("session/new", {**params, "tools": [
            {"name": "bash", "inputSchema": {"type": "object", "properties": {"command": {"type": "string"}}, "required": ["command"]}}
        ]})
        shadow_events = []
        count = len(requests)
        def deny(tool):
            assert tool["name"] == "bash"
            time.sleep(0.05)
            assert len(requests) == count + 1, "provider advanced during host approval"
            raise PermissionError("Host denied mutation")
        client.wait(client.request("turn/prompt", {**shadow, "text": "Try mutation"}),
                    tool=deny, event=shadow_events.append)
        assert not (tmp / "SHOULD_NOT_EXIST").exists()
        assert any(e["type"] == "tool_end" and e["data"]["isError"] for e in shadow_events)

        mode = "text"
        active = call("session/new", params)
        call("session/configure", {**active, "model": "mock-updated", "reasoningBudget": 0})
        assert call("session/inspect", active)["model"] == "mock-updated"
        ident = client.request("session/configure", {**active, "toolMode": "local"})
        assert "error" in client.receive()
        empty_page = call("session/inspect", {**active, "limit": 1})
        assert empty_page["recordCount"] == 0
        ident = client.request("unknown/method", {})
        assert client.receive()["error"]["code"] == -32601
        ident = client.request("session/resume", {"sessionId": "../escape", **params})
        assert "error" in client.receive()
        ident = client.request("session/new", {**params, "cwd": "relative"})
        assert "error" in client.receive()
        ident = client.request("session/new", {**params, "tools": tools + [tools[0]]})
        assert "error" in client.receive()
        client.send({"jsonrpc": "2.0", "method": "session/list", "params": {}})
        assert client.receive()["error"]["code"] == -32600
        mode = "local"
        local_params = {k: v for k, v in params.items() if k != "tools"}
        local_params["toolMode"] = "local"
        local = call("session/new", local_params)
        client.wait(client.request("turn/prompt", {**local, "text": "Reply only"}))

        assert not failures, failures
        bad_id = client.request("session/new", {**params, "provider": "unsupported-provider"})
        assert "error" in client.receive()

        assert all((p.stat().st_mode & 0o777) == 0o600 for p in storage.glob("*.host.json"))
        call("shutdown", {})
        client.process.wait(timeout=5)
        assert client.process.returncode == 0
    finally:
        client.close()
    # EOF while a host operation is pending must settle, preserve the log, and exit.
    mode = "cancel"
    disconnected = host.RuntimeClient(binary, tmp, env)
    try:
        disconnected.wait(disconnected.request("initialize", {"protocolVersion": 1, "storageDir": str(tmp / "disconnect")}))
        active = disconnected.wait(disconnected.request("session/new", params))
        disconnected.request("turn/prompt", {**active, "text": "Wait for host"})
        while disconnected.receive().get("method") != "tool/execute":
            pass
        disconnected.process.stdin.close()
        settled = False
        while not settled:
            m = disconnected.receive()
            settled = m.get("method") == "runtime/event" and m["params"]["type"] == "settled"
        disconnected.process.wait(timeout=5)
        assert disconnected.process.returncode == 0
    finally:
        disconnected.close()
    oversized = host.RuntimeClient(binary, tmp, env)
    try:
        oversized.process.stdin.write(b" " * (host.MAX_FRAME + 1))
        oversized.process.stdin.flush()
        assert oversized.receive()["error"]["code"] == -32700
        oversized.process.wait(timeout=5)
    finally:
        oversized.close()
server.shutdown()
print("PASS: initialization, exclusive host tools, real call IDs, custom question answer, fork, rewind, resume, cancellation, steering and bounded shutdown")
