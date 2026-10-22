#!/usr/bin/env python3
"""Exercise adaptive opt-out and explicit server deadlines in the shipped binary."""
import hashlib
import http.server
import json
import os
from pathlib import Path
import subprocess
import tempfile
import threading
import time
from pty_vt import Terminal
from core_binary import core_binary_path

BIN = Path(os.environ.get('GLASS_CORE_TEST_BINARY', str(core_binary_path('release')))).resolve()
os.environ['GLASS_CORE_TEST_BINARY'] = str(BIN)

class Endpoint(http.server.BaseHTTPRequestHandler):
    def log_message(self, *_):
        pass

    def do_POST(self):
        self.rfile.read(int(self.headers['Content-Length']))
        self.server.starts.append(time.monotonic())
        self.send_response(200)
        self.send_header('Content-Type', 'text/event-stream')
        self.end_headers()
        event = {'choices': [{'delta': {'content': 'PACING_OPTION_OK'}, 'finish_reason': 'stop'}]}
        self.wfile.write(('data: ' + json.dumps(event) + '\n\ndata: [DONE]\n\n').encode())

with tempfile.TemporaryDirectory(prefix='gc-pacing-option-') as tmp:
    root = Path(tmp)
    server = http.server.ThreadingHTTPServer(('127.0.0.1', 0), Endpoint)
    server.starts = []
    threading.Thread(target=server.serve_forever, daemon=True).start()
    endpoint = f'http://127.0.0.1:{server.server_port}/v1'
    config = root / 'config/glass-core/config.toml'
    config.parent.mkdir(parents=True)
    config.write_text(f'provider="openai"\nmodel="fixture"\napi_key_env=""\nbase_url="{endpoint}"\ntrust="always"\nadaptive_pacing=false\n')
    env = dict(os.environ, XDG_CONFIG_HOME=str(root / 'config'), XDG_DATA_HOME=str(root / 'data'))
    directory = root / 'data/glass-core/provider-pacing'
    directory.mkdir(parents=True)
    record = directory / (hashlib.sha256(endpoint.encode() + b'\0').hexdigest() + '.json')

    def run(learned_ms, server_ms, version=1):
        now = int(time.time() * 1000)
        record.write_text(json.dumps(dict(version=version, last=now, previous=now, cooldown=now+learned_ms, server_cooldown=now+server_ms if server_ms else 0, gap=60000, healthy=0)))
        start = time.monotonic()
        result = subprocess.run([str(BIN), 'run', '--json', 'say hello'], cwd=root, env=env, capture_output=True, text=True, timeout=10)
        assert result.returncode == 0 and 'PACING_OPTION_OK' in result.stdout, result.stdout + result.stderr
        return server.starts[-1] - start

    delay = run(60000, 0)
    assert delay < 1.5, delay
    print('PASS disabled learned and burst delay:', round(delay, 3))
    delay = run(60000, 650)
    assert .5 <= delay < 2, delay
    print('PASS explicit server cooldown with adaptation disabled:', round(delay, 3))
    delay = run(650, 0, version=0)
    assert .5 <= delay < 2, delay
    print('PASS legacy combined server deadline:', round(delay, 3))
    record.unlink()
    result = subprocess.run([str(BIN), 'settings', '--json'], cwd=root, env=env, capture_output=True, text=True, check=True)
    values = {item['setting']:item['value'] for item in json.loads(result.stdout)['settings']}
    assert values['adaptive_pacing'] is False
    for size in ((80, 30), (40, 24), (20, 8)):
        terminal = Terminal(root, env, size)
        try:
            terminal.idle()
            terminal.send('/pacing\r')
            terminal.visible('Provider pacing')
            terminal.send('\x1b[B\r')
            terminal.idle()
            terminal.send('/pacing\r')
            terminal.visible('Provider pacing')
            terminal.send('\x1b')
            terminal.collect(.2)
            # Escape returns through the slash palette before closing it.
            if 'Search actions' in terminal.vt.text():
                terminal.send('\x1b')
                terminal.collect(.2)
            terminal.idle()
            terminal.send('\x15hello\r')
            terminal.wait('PACING_OPTION_OK')
            print('PASS pacing menu, Escape and subsequent request:', size)
        finally:
            terminal.close()
    server.shutdown()
