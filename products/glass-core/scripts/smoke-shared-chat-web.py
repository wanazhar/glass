#!/usr/bin/env python3
"""Exercise shared GC/Glass chat resumption through two isolated Connect gateways."""
import http.server
import json
import os
from pathlib import Path
import re
import subprocess
import tempfile
import threading
import time

from core_binary import core_binary_path

ROOT = Path(__file__).resolve().parents[1]
BIN = Path(os.environ.get("GLASS_CORE_TEST_BINARY", core_binary_path("debug"))).resolve()
SESSION_ID = "shared-web-same-id"


class Provider(http.server.BaseHTTPRequestHandler):
    def log_message(self, *_):
        pass

    def do_GET(self):
        payload = json.dumps({"data": [{"id": "mock"}]}).encode()
        self.send_response(200)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(payload)))
        self.end_headers()
        self.wfile.write(payload)

    def do_POST(self):
        self.rfile.read(int(self.headers.get("Content-Length", "0")))
        time.sleep(0.6)
        self.send_response(200)
        self.send_header("Content-Type", "text/event-stream")
        self.send_header("Cache-Control", "no-cache")
        self.end_headers()
        frames = [
            {"choices": [{"delta": {"content": "Browser resumption response."}}]},
            "[DONE]",
        ]
        try:
            for frame in frames:
                body = frame if isinstance(frame, str) else json.dumps(frame)
                self.wfile.write(f"data: {body}\n\n".encode())
                self.wfile.flush()
        except (BrokenPipeError, ConnectionResetError):
            pass


def gateway(env, cwd):
    result = subprocess.run(
        [str(BIN), "connect", "--local"],
        cwd=cwd,
        env=env,
        capture_output=True,
        text=True,
        timeout=15,
    )
    assert result.returncode == 0, result.stderr
    match = re.search(r"http://127\.0\.0\.1:\d+/#\w+", result.stdout)
    assert match, result.stdout
    return match.group(0)


def stop_gateway(env, cwd):
    subprocess.run(
        [str(BIN), "connect", "stop"],
        cwd=cwd,
        env=env,
        capture_output=True,
        text=True,
        timeout=10,
    )


with tempfile.TemporaryDirectory(prefix="gc-shared-chat-web-") as temporary:
    root = Path(temporary)
    local_store = root / ".glass" / "agent-sessions"
    local_store.mkdir(parents=True)
    data_roots = [root / f"data-{index}" for index in (1, 2)]
    config_roots = [root / f"config-{index}" for index in (1, 2)]
    for path in [*data_roots, *config_roots]:
        (path / "glass-core").mkdir(parents=True)
    provider = http.server.ThreadingHTTPServer(("127.0.0.1", 0), Provider)
    threading.Thread(target=provider.serve_forever, daemon=True).start()

    glass_log = local_store / f"{SESSION_ID}.jsonl"
    glass_meta = local_store / f"{SESSION_ID}.meta.json"
    gc_log = data_roots[0] / "glass-core" / f"{SESSION_ID}.jsonl"
    gc_meta = data_roots[0] / "glass-core" / f"{SESSION_ID}.meta.json"
    glass_initial = (
        json.dumps({"k": "user", "t": "Glass saved question"})
        + "\n"
        + json.dumps({"k": "assistant", "t": "Glass saved answer"})
        + "\n"
    ).encode()
    gc_initial = (
        json.dumps({"k": "user", "t": "GC saved question"})
        + "\n"
        + json.dumps({"k": "assistant", "t": "GC saved answer"})
        + "\n"
    ).encode()
    glass_log.write_bytes(glass_initial)
    gc_log.write_bytes(gc_initial)
    glass_meta.write_text(json.dumps({"workspace": str(root), "title": "Shared Glass resume fixture"}))
    gc_meta.write_text(json.dumps({"workspace": str(root), "title": "Shared GC duplicate fixture"}))

    environments = []
    urls = []
    try:
        for index in range(2):
            config = config_roots[index] / "glass-core" / "config.toml"
            config.write_text(
                "provider=\"openai\"\n"
                "model=\"mock\"\n"
                "api_key_env=\"\"\n"
                f"base_url=\"http://127.0.0.1:{provider.server_port}/v1\"\n"
                "sandbox=\"workspace\"\n"
            )
            env = dict(
                os.environ,
                XDG_CONFIG_HOME=str(config_roots[index]),
                XDG_DATA_HOME=str(data_roots[index]),
                GC_WEB_ASSETS=str(ROOT / "web" / "dist"),
            )
            environments.append(env)
            urls.append(gateway(env, root))

        # The GC and Glass rows deliberately share an ID. Their canonical paths
        # are discovered through the real API and passed to a real browser.
        parsed = urls[0]
        base, bearer = parsed.split("/#", 1)
        import urllib.request

        request = urllib.request.Request(
            base + "/api/threads",
            headers={"Authorization": f"Bearer {bearer}", "Origin": base},
        )
        with urllib.request.urlopen(request, timeout=10) as response:
            rows = json.load(response)["threads"]
        matching = [row for row in rows if row["session"] == SESSION_ID]
        assert len(matching) == 2, matching
        by_origin = {row["origin"]: row for row in matching}
        assert set(by_origin) == {"glass", "gc"}, matching
        assert by_origin["glass"]["sourceRef"] == str(glass_log.resolve())
        assert by_origin["gc"]["sourceRef"] == str(gc_log.resolve())

        browser = subprocess.run(
            [
                "node",
                str(ROOT / "web" / "test" / "shared-chat-live.mjs"),
                urls[0],
                urls[1],
                SESSION_ID,
                by_origin["glass"]["sourceRef"],
                by_origin["gc"]["sourceRef"],
            ],
            cwd=ROOT / "web",
            capture_output=True,
            text=True,
            timeout=180,
        )
        print(browser.stdout, end="", flush=True)
        assert browser.returncode == 0, browser.stderr + "\nWorker logs:\n" + "\n".join(log.read_text(errors="replace")[-6000:] for data_root in data_roots for log in (data_root/"glass-core").glob("serve-*.log"))
        glass_after = glass_log.read_bytes()
        gc_after = gc_log.read_bytes()
        assert glass_after.startswith(glass_initial), "Glass transcript prefix changed"
        assert gc_after == gc_initial, "Web append was routed into the same-ID GC log"
        glass_records = [json.loads(line) for line in glass_after.splitlines()]
        assert sum(record.get("t") == "Append from the Web to the Glass log" for record in glass_records) == 1
        assert sum(record.get("t") == "Browser resumption response." for record in glass_records) == 1
        print("PASS same-ID source routing preserved the Glass log and GC log", flush=True)
    finally:
        import urllib.request
        for url in urls:
            base, bearer = url.split("/#", 1)
            for path in (glass_log, gc_log):
                try:
                    request = urllib.request.Request(base + "/api/close", data=json.dumps({"id": SESSION_ID, "sourceRef": str(path)}).encode(), headers={"Authorization": f"Bearer {bearer}", "Origin": base, "Content-Type": "application/json"})
                    urllib.request.urlopen(request, timeout=10).close()
                except Exception:
                    pass
        for env in environments:
            stop_gateway(env, root)
        provider.shutdown()
        provider.server_close()
