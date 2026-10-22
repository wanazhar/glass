#!/usr/bin/env python3
"""Release transport cooldown and command-detail navigation regression checks."""
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

class Endpoint(http.server.BaseHTTPRequestHandler):
    def log_message(self, *_):
        pass

    def do_POST(self):
        self.rfile.read(int(self.headers['Content-Length']))
        self.server.starts.append(time.monotonic())
        if len(self.server.starts) == 1:
            self.send_response(429)
            self.send_header('Retry-After', '1.5')
            self.end_headers()
            self.wfile.write(b'{"error":{"message":"fixture burst limit"}}')
        else:
            self.send_response(200)
            self.send_header('Content-Type', 'text/event-stream')
            self.end_headers()
            data = {'choices': [{'delta': {'content': 'PACED_OK'}, 'finish_reason': 'stop'}]}
            self.wfile.write(('data: ' + json.dumps(data) + '\n\ndata: [DONE]\n\n').encode())

with tempfile.TemporaryDirectory(prefix='gc-pacing-pty-') as tmp:
    root = Path(tmp)
    server = http.server.ThreadingHTTPServer(('127.0.0.1', 0), Endpoint)
    server.starts = []
    threading.Thread(target=server.serve_forever, daemon=True).start()
    config = root / 'config/glass-core/config.toml'
    config.parent.mkdir(parents=True)
    config.write_text(f'provider="openai"\nmodel="fixture"\napi_key_env=""\nbase_url="http://127.0.0.1:{server.server_port}/v1"\ntrust="always"\n')
    env = dict(os.environ, XDG_CONFIG_HOME=str(root / 'config'), XDG_DATA_HOME=str(root / 'data'))
    result = subprocess.run([str(BIN), 'run', '--json', 'say hello'], cwd=root, env=env, capture_output=True, text=True, timeout=15)
    assert result.returncode == 0 and 'PACED_OK' in result.stdout, result.stdout + result.stderr
    assert len(server.starts) == 2, server.starts
    assert server.starts[1] - server.starts[0] >= 1.45, server.starts
    print('PASS headless Retry-After:', round(server.starts[1] - server.starts[0], 3), 'seconds')
    note = 'Long readable memory. ' * 35 + 'FULL_TEXT_END_MARKER'
    subprocess.run([str(BIN), 'neuralnets', 'save', 'lesson', note, '--json'], cwd=root, env=env, capture_output=True, check=True)
    os.environ['GLASS_CORE_TEST_BINARY'] = str(BIN)
    for size in ((20, 8), (40, 24), (80, 30)):
        terminal = Terminal(root, env, size)
        try:
            terminal.idle()
            terminal.send('/')
            terminal.visible('Search actions')
            terminal.send('\x1bOP')  # F1
            terminal.visible('Command details')
            terminal.send('\x1b[F')
            terminal.collect(.1)
            terminal.send('\x1b')
            terminal.visible('Search actions')
            terminal.send('\x1b')
            terminal.idle()
            print('PASS command details, scroll, Escape back:', size)
            terminal.send('/neuralnets\r')
            terminal.visible('Neuralnets')
            terminal.send('\x1b[B\x1b[C')
            terminal.visible('Selection details')
            terminal.send('\x1b[F')
            terminal.visible('FULL_TEXT_END_MARKER')
            terminal.send('\x1b')
            terminal.visible('Neuralnets')
            terminal.send('\x1b')
            terminal.idle()
            print('PASS full long memory reader:', size)
        finally:
            terminal.close()
    server.shutdown()
