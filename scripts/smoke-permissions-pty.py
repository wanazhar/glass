#!/usr/bin/env python3
"""Real modal approval -> background execution -> session-clear denial."""
import json
import http.server
import os
from pathlib import Path
import sys
import tempfile
import time
import threading

from pty_vt import Terminal

if sys.platform != "linux":
    print("SKIP Linux permission-profile PTY")
    raise SystemExit(0)


def command(terminal, text):
    terminal.send(text + "\r")
    terminal.collect(0.35)


class Endpoint(http.server.BaseHTTPRequestHandler):
    shell = ""

    def log_message(self, *_):
        pass

    def do_GET(self):
        self.send_response(200)
        self.end_headers()
        self.wfile.write(b'{"data":[{"id":"fixture"}]}')

    def do_POST(self):
        body = json.loads(self.rfile.read(int(self.headers['Content-Length'])))
        latest_user = max(i for i, message in enumerate(body['messages']) if message['role'] == 'user')
        results = [message for message in body['messages'][latest_user + 1:] if message['role'] == 'tool']
        delta = {'content': 'Permission fixture done.'} if results else {'tool_calls': [{
            'index': 0, 'id': 'permission-command', 'function': {'name': 'bash', 'arguments': json.dumps({'command': type(self).shell})}
        }]}
        data = ('data: ' + json.dumps({'choices': [{'delta': delta}]}) + '\n\ndata: [DONE]\n\n').encode()
        self.send_response(200)
        self.send_header('Content-Type', 'text/event-stream')
        self.send_header('Content-Length', str(len(data)))
        self.end_headers()
        self.wfile.write(data)


def model_command(terminal):
    terminal.seen = ''
    command(terminal, 'execute the approved exact command')
    terminal.wait('state=done:', seconds=8)


with tempfile.TemporaryDirectory(prefix="gc-permissions-pty-") as temporary:
    root = Path(temporary)
    config = root / "config/glass-core/config.toml"
    endpoint = http.server.ThreadingHTTPServer(('127.0.0.1', 0), Endpoint)
    threading.Thread(target=endpoint.serve_forever, daemon=True).start()
    config.parent.mkdir(parents=True)
    config.write_text('provider="openai"\nmodel="fixture"\napi_key_env=""\n'
                      f'base_url="http://127.0.0.1:{endpoint.server_port}/v1"\n'
                      'permission_profile="workspace-edit"\ntrust="always"\n')
    env = dict(os.environ, XDG_CONFIG_HOME=str(root / "config"),
               XDG_DATA_HOME=str(root / "data"))
    for size in ((40, 24), (20, 8)):
        outside = Path.home() / f"gc-approved-pty-{os.getpid()}-{size[0]}"
        shell = f"printf approved > {outside}"
        Endpoint.shell = shell
        terminal = Terminal(root, env, size)
        try:
            terminal.idle()
            command(terminal, "/permissions")
            terminal.visible("Security")
            terminal.send("\x1b[B\x1b[B\r")
            terminal.visible("Approve one")
            terminal.send(shell + "\r")
            terminal.visible("Review command")
            assert not outside.exists(), "review must not execute the command"
            terminal.send("\r")
            terminal.visible("Session exception")
            terminal.send("\r")
            terminal.collect(0.2)
            model_command(terminal)
            assert outside.read_text() == "approved", terminal.vt.text()
            outside.unlink()
            command(terminal, "/bg " + shell)
            deadline = time.monotonic() + 5
            while not outside.exists() and time.monotonic() < deadline:
                terminal.collect(0.05)
            assert outside.read_text() == "approved", terminal.vt.text()

            # A new conversation must not retain the previous session's grant.
            command(terminal, "/clear")
            outside.unlink()
            model_command(terminal)
            assert not outside.exists(), "model inherited approval across session clear"
            command(terminal, "/bg " + shell)
            terminal.collect(0.5)
            assert not outside.exists(), "exact approval leaked across session clear"

            # Saving a profile changes only the next launch, and keeps TOML valid.
            command(terminal, "/permissions")
            terminal.visible("Security")
            terminal.send("\x1b[B\r")
            terminal.visible("Permission profile")
            terminal.send("\x1b[B\x1b[B\r")
            terminal.visible("Profile saved")
            assert 'permission_profile = "read-only"' in config.read_text()
            terminal.send("\r")
            terminal.collect(0.2)
            command(terminal, "/bg printf current-policy > current-policy.txt")
            deadline = time.monotonic() + 5
            while not (root / "current-policy.txt").exists() and time.monotonic() < deadline:
                terminal.collect(0.05)
            assert (root / "current-policy.txt").exists(), "saved profile changed startup policy mid-session"
            (root / "current-policy.txt").unlink()
        finally:
            terminal.close()
            outside.unlink(missing_ok=True)
        # Each size starts with the intended workspace-edit startup policy.
        config.write_text(config.read_text().replace('permission_profile = "read-only"', 'permission_profile = "workspace-edit"'))
    endpoint.shutdown()
    endpoint.server_close()
print("PASS permission modal, exact background approval, session clear, next-launch profile at 40x24/20x8")
