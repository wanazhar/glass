#!/usr/bin/env python3
"""End-to-end read-only profile check through the real headless bash tool path."""
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

if sys.platform != "linux":
    print("SKIP Linux Landlock permission profiles; unsupported strict policies refuse")
    raise SystemExit(0)


class Endpoint(http.server.BaseHTTPRequestHandler):
    tool_command = ""
    tool_results = []

    def log_message(self, *_):
        pass

    def do_GET(self):
        body = json.dumps({"data": [{"id": "security-mock", "name": "Security mock"}]}).encode()
        self.send_response(200)
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def do_POST(self):
        body = json.loads(self.rfile.read(int(self.headers["Content-Length"])))
        results = [m for m in body.get("messages", []) if m.get("role") == "tool"]
        if results:
            type(self).tool_results = results
            frames = [{"choices": [{"delta": {"content": "security smoke done"}}]}]
        else:
            frames = [{"choices": [{"delta": {"tool_calls": [{
                "index": 0, "id": "security-call",
                "function": {"name": "bash", "arguments": json.dumps({"command": type(self).tool_command})},
            }]}}]}]
        data = ("".join("data: " + json.dumps(frame) + "\n\n" for frame in frames) + "data: [DONE]\n\n").encode()
        self.send_response(200)
        self.send_header("Content-Type", "text/event-stream")
        self.send_header("Content-Length", str(len(data)))
        self.end_headers()
        self.wfile.write(data)


with tempfile.TemporaryDirectory(prefix="gc-security-") as temp:
    root = Path(temp)
    workspace = root / "workspace"
    workspace.mkdir()
    outside = root / "outside.txt"
    outside_home = Path.home() / f"gc-security-outside-{os.getpid()}.txt"
    cfg_root = root / "config"
    cfg_dir = cfg_root / "glass-core"
    cfg_dir.mkdir(parents=True)
    server = http.server.ThreadingHTTPServer(("127.0.0.1", 0), Endpoint)
    threading.Thread(target=server.serve_forever, daemon=True).start()
    env = dict(os.environ, XDG_CONFIG_HOME=str(cfg_root), XDG_DATA_HOME=str(root / "data"), GLASS_TEST_API_KEY="never-in-child")
    try:
        def run_profile(profile, command, data_name):
            Endpoint.tool_command = command
            Endpoint.tool_results = []
            (cfg_dir / "config.toml").write_text(
                'provider="openai"\nmodel="mock"\napi_key_env=""\n'
                f'base_url="http://127.0.0.1:{server.server_port}/v1"\n'
                f'permission_profile="{profile}"\ntrust="always"\n'
            )
            run_env = dict(env, XDG_DATA_HOME=str(root / data_name))
            result = subprocess.run(
                [str(BINARY), "run", "--json", "security profile probe"],
                cwd=workspace, env=run_env, text=True, capture_output=True, timeout=120,
            )
            assert result.returncode == 0, result
            payload = json.loads(result.stdout)
            assert payload["status"] == "finished", payload
            assert Endpoint.tool_results, "mock provider must observe the completed tool round trip"
            return result

        run_profile("read-only", f"printf blocked > inside.txt; printf blocked > '{outside}'", "data-readonly")
        assert not (workspace / "inside.txt").exists(), "read-only must deny workspace writes"
        assert not outside.exists(), "read-only must deny writes outside the workspace"
        result = run_profile(
            "workspace-edit",
            f"printf ok > inside.txt; printf ok > '{outside_home}'; printf temp > '{root}/tmp-ok.txt'",
            "data-workspace-edit",
        )
        assert (workspace / "inside.txt").exists(), "workspace-edit must permit workspace writes: " + result.stderr + repr(Endpoint.tool_results)
        assert not outside_home.exists(), "workspace-edit must deny writes outside workspace and temp roots"
        assert (root / "tmp-ok.txt").exists(), "workspace-edit must permit temporary writes"
        print("security smoke passed: read-only and workspace-edit boundaries enforced via bash")
    finally:
        outside_home.unlink(missing_ok=True)
        server.shutdown()
