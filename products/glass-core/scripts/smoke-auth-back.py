#!/usr/bin/env python3
"""Offline release PTYs for sign-in hierarchy, narrow hints and canceled secrets."""
import http.server
import json
import os
from pathlib import Path
import tempfile
import threading
from pty_vt import Terminal

ROOT = Path(__file__).resolve().parents[1]
os.environ.setdefault('GLASS_CORE_TEST_BINARY', str(ROOT / 'target/release/glass-core'))
SECRET = 'not-a-real-api-key'
class Catalog(http.server.BaseHTTPRequestHandler):
    def log_message(self, *_): pass
    def do_GET(self):
        body = json.dumps({'data': [{'id': 'mock', 'name': 'Fixture model'}]}).encode()
        self.send_response(200); self.send_header('Content-Length', str(len(body))); self.end_headers(); self.wfile.write(body)

def choose(tty, text):
    tty.send(text + '\r')
    tty.collect(.15)
def field(tty, text):
    tty.send('\x15' + text + '\r')
    tty.collect(.15)
def back(tty, expected):
    tty.send('\x1b')
    tty.collect(.15)
    tty.visible(expected)

with tempfile.TemporaryDirectory(prefix='gc-auth-back-') as tmp:
    root = Path(tmp)
    cfg = root / 'config/glass-core'; cfg.mkdir(parents=True)
    # Suppress browser opening; no callback, token exchange or real sign-in occurs.
    bin_dir = root / 'bin'; bin_dir.mkdir()
    for name in ['xdg-open', 'open']:
        path = bin_dir / name; path.write_text('#!/bin/sh\nexit 0\n'); path.chmod(0o755)
    auth_path = cfg / 'auth.json'
    auth_path.write_text(json.dumps({'host_id': 'fixture-host', 'connections': [], 'credentials': {}}))
    original = auth_path.read_bytes()
    server = http.server.ThreadingHTTPServer(('127.0.0.1', 0), Catalog)
    threading.Thread(target=server.serve_forever, daemon=True).start()
    (cfg / 'config.toml').write_text(f'provider="openai"\nmodel="mock"\napi_key_env=""\ntrust="always"\nbase_url="http://127.0.0.1:{server.server_port}/v1"\n')
    env = dict(os.environ, XDG_CONFIG_HOME=str(root / 'config'), XDG_DATA_HOME=str(root / 'data'), PATH=str(bin_dir) + os.pathsep + os.environ['PATH'])
    try:
        for size in [(20, 8), (20, 24), (40, 24), (80, 30)]:
            tty = Terminal(root, env, size)
            try:
                tty.idle()
                tty.send('/settings\r'); tty.visible('Settings'); tty.collect(.2)
                assert 'Esc back' in tty.vt.text(), tty.vt.text()
                choose(tty, 'login'); tty.visible('Connect a provider')
                choose(tty, 'Continue with ChatGPT'); tty.visible('Waiting for sign-in')
                back(tty, 'Connect a provider')
                back(tty, 'Settings')
                back(tty, 'Esc close')
                back(tty, '❯'); tty.idle()
                assert auth_path.read_bytes() == original

                tty.send('/login\r'); tty.visible('Connect a provider')
                choose(tty, 'Custom provider'); tty.visible('Connection name')
                field(tty, 'Fixture provider'); tty.visible('OpenAI Chat')
                choose(tty, 'Chat Completions'); tty.visible('Base URL')
                back(tty, 'OpenAI Chat')
                back(tty, 'Connection name')
                field(tty, 'Fixture provider'); tty.visible('OpenAI Chat')
                choose(tty, 'Chat Completions'); tty.visible('Base URL')
                field(tty, f'http://127.0.0.1:{server.server_port}/v1'); tty.visible('Connect Fixture')
                # Escape authentication returns to the URL, preserving the provider draft.
                back(tty, 'Base URL')
                tty.send('\r'); tty.collect(.2)
                choose(tty, 'Enter API key'); tty.visible('API key')
                tty.send(SECRET); tty.collect(.1)
                assert SECRET not in tty.vt.text()
                back(tty, 'Connect Fixture')
                choose(tty, 'Enter API key'); tty.visible('API key')
                field(tty, SECRET); tty.visible('Provider and model')
                back(tty, 'API key')
                back(tty, 'Connect Fixture')
                back(tty, 'Base URL')
                back(tty, 'OpenAI Chat')
                back(tty, 'Connection name')
                back(tty, 'Connect a provider')
                back(tty, 'Esc close')
                back(tty, '❯'); tty.idle()
                assert auth_path.read_bytes() == original, 'Canceled secret was persisted'
                assert SECRET not in tty.seen, 'Secret leaked to terminal output'
                print(f'PASS {size}: OAuth and custom-provider Esc hierarchy, narrow hint, canceled secret')
            finally:
                tty.close()
        # Ctrl-C cancels the entire sign-in flow rather than navigating backward.
        tty = Terminal(root, env, (40, 24))
        try:
            tty.idle(); tty.send('/login\r'); tty.visible('Connect a provider')
            choose(tty, 'Continue with ChatGPT'); tty.visible('Waiting for sign-in')
            tty.send('\x03'); tty.collect(.2); tty.idle()
            assert 'Connect a provider' not in tty.vt.text()
            assert auth_path.read_bytes() == original
            print('PASS Ctrl-C closes sign-in without persisting credentials')
        finally:
            tty.close()
    finally:
        server.shutdown()
