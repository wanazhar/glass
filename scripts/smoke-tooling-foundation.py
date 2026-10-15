#!/usr/bin/env python3
"""Release CLI and PTY regressions for file cursors, previews and durable state."""
import hashlib
import http.server
import json
import os
import re
from pathlib import Path
import subprocess
import tempfile
import threading

from pty_vt import Terminal

BIN = Path(os.environ.get("GLASS_CORE_TEST_BINARY", Path(__file__).resolve().parents[1] / "target/release/glass-core")).resolve()
os.environ["GLASS_CORE_TEST_BINARY"] = str(BIN)

with tempfile.TemporaryDirectory(prefix="gc-foundation-cli-") as tmp:
    root = Path(tmp)
    env = dict(os.environ, XDG_CONFIG_HOME=str(root / "config"), XDG_DATA_HOME=str(root / "data"))

    def run(*args, request=None, ok=True):
        result = subprocess.run([str(BIN), "file", "--json", *args], cwd=root, env=env,
                                input=json.dumps(request) if request is not None else "",
                                capture_output=True, text=True, timeout=10)
        assert (result.returncode == 0) == ok, (args, result.stdout, result.stderr)
        return json.loads(result.stdout)

    # 1–4: dry-run creation, empty creation, unchanged receipt, bad preview type.
    request = {"path": "empty", "expected": None, "text": "", "preview": True}
    assert run("write", request=request)["status"] == "proposed"
    assert not (root / "empty").exists()
    request.pop("preview")
    receipt = run("write", request=request)
    assert receipt["created"] and receipt["changed"] and receipt["applied"]
    revision = receipt["revision"]
    request["expected"] = revision
    assert run("write", request=request)["status"] == "unchanged"
    run("write", request=dict(request, preview="yes"), ok=False)

    # 5–7: validated edit preview, failed validation, stale write.
    (root / "a.json").write_text('{"value":1}\n')
    rev = run("read", "a.json")["revision"]
    edit = {"path": "a.json", "expected": rev, "edits": [{"old": "1", "new": "2"}], "preview": True, "validate": True}
    assert run("edit", request=edit)["base_revision"] == rev
    assert (root / "a.json").read_text() == '{"value":1}\n'
    run("write", request={"path": "a.json", "expected": rev, "text": "{bad}", "validate": True}, ok=False)
    run("write", request={"path": "a.json", "expected": revision, "text": "lost"}, ok=False)

    # 8–10: recover long Unicode lines, refuse stale cursors, require revisions.
    content = "first\r\n" + "界" * 25_000 + "\r\nlast"
    (root / "long").write_bytes(content.encode())
    page = run("read", "long")
    assert page["partial_line"] and page["truncated"]
    saved = page["next_read"]
    rendered = re.sub(r"(?m)^\d+\|", "", page["text"])
    end = page["end_byte_offset"]
    while page["next_read"]:
        continuation = page["next_read"]
        page = run("read", "long", "--byte-offset", str(continuation["byte_offset"]), "--expected", continuation["expected"])
        rendered += re.sub(r"(?m)^\d+\|", "", page["text"])
        assert page["end_byte_offset"] > end
        end = page["end_byte_offset"]
    assert end == len(content.encode())
    assert rendered == content.replace("\r\n", "\n") + "\n"
    (root / "long").write_text("changed")
    run("read", "long", "--byte-offset", str(saved["byte_offset"]), "--expected", saved["expected"], ok=False)
    run("read", "long", "--byte-offset", "0", ok=False)

    # 11–15: UTF-8 boundary, unknown flag, extra check argument, ambiguous edit, overlap.
    (root / "unicode").write_text("界\n")
    rev = run("read", "unicode")["revision"]
    run("read", "unicode", "--byte-offset", "1", "--expected", rev, ok=False)
    run("read", "unicode", "--surprise", ok=False)
    run("check", "a.json", "extra", ok=False)
    (root / "repeat").write_text("same same\n")
    rev = run("read", "repeat")["revision"]
    run("edit", request={"path": "repeat", "expected": rev, "edits": [{"old": "same", "new": "x"}]}, ok=False)
    run("edit", request={"path": "repeat", "expected": rev, "edits": [{"start": 1, "end": 1, "text": "x"}, {"start": 1, "end": 1, "text": "y"}]}, ok=False)
    assert (root / "repeat").read_text() == "same same\n"
print("PASS: 15 file CLI edge cases on release build, outside Git")


class Provider(http.server.BaseHTTPRequestHandler):
    def log_message(self, *_):
        pass

    def do_POST(self):
        body = json.loads(self.rfile.read(int(self.headers["Content-Length"])))
        assert [t["function"]["name"] for t in body["tools"]] == ["bash", "state", "view_image", "read", "edit", "write", "ask_user_question"]
        latest = max(i for i, m in enumerate(body["messages"]) if m["role"] == "user")
        results = [m["content"] for m in body["messages"][latest + 1:] if m["role"] == "tool"]
        n = len(results)
        if body["messages"][latest]["content"] == "followup":
            calls = None
            answer = "FOLLOWUPDONE"
        elif n == 0:
            calls = ("state", {"items": ["temporary note"]})
        elif n == 1:
            calls = ("state", {"items": []})
        elif n == 2:
            calls = ("state", {})
        elif n == 3:
            assert "no state recorded" in results[-1]
            calls = ("bash", {"command": "touch forbidden", "cwd": "missing"})
        elif n == 4:
            assert "nothing executed" in results[-1]
            request = {"path": "sample", "expected": hashlib.sha256(b"before\n").hexdigest(), "edits": [{"old": "before", "new": "after"}], "preview": True}
            calls = ("bash", {"command": "\"$GC_BIN\" file edit <<'GC_PREVIEW'\n" + json.dumps(request) + "\nGC_PREVIEW"})
        else:
            assert "proposed" in results[-1]
            calls = None
            answer = "FOUNDATIONDONE"
        if calls:
            name, args = calls
            delta = {"tool_calls": [{"index": 0, "id": "foundation-" + str(n), "function": {"name": name, "arguments": json.dumps(args)}}]}
        else:
            delta = {"content": answer}
        payload = ("data: " + json.dumps({"choices": [{"delta": delta}]}) + "\n\ndata: [DONE]\n\n").encode()
        self.send_response(200)
        self.send_header("Content-Type", "text/event-stream")
        self.end_headers()
        self.wfile.write(payload)


for size in [(80, 30), (40, 24), (20, 8)]:
    with tempfile.TemporaryDirectory(prefix="gc-foundation-pty-") as tmp:
        root = Path(tmp)
        config = root / "config/glass-core"
        config.mkdir(parents=True)
        (root / "sample").write_text("before\n")
        server = http.server.ThreadingHTTPServer(("127.0.0.1", 0), Provider)
        threading.Thread(target=server.serve_forever, daemon=True).start()
        (config / "config.toml").write_text(f'provider="openai"\nmodel="mock"\napi_key_env=""\nbase_url="http://127.0.0.1:{server.server_port}/v1"\n')
        env = dict(os.environ, XDG_CONFIG_HOME=str(root / "config"), XDG_DATA_HOME=str(root / "data"))
        terminal = Terminal(root, env, size)
        try:
            terminal.idle()
            terminal.send("exercise foundation\r")
            terminal.wait("FOUNDATIONDONE", 25)
            assert not (root / "forbidden").exists()
            assert (root / "sample").read_text() == "before\n"
            terminal.send("followup\r")
            terminal.wait("FOLLOWUPDONE", 15)
            records = [json.loads(line) for p in (root / "data/glass-core").glob("*.jsonl") if p.stem != "history" for line in p.read_text().splitlines()]
            states = [r["items"] for r in records if r.get("k") == "state"]
            assert states == [["temporary note"], []], states
        finally:
            terminal.close()
            server.shutdown()
            server.server_close()
print("PASS: release PTYs 80x30/40x24/20x8, durable clear, rejected cwd, edit preview, follow-up and seven-tool surface")
