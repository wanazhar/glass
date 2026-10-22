#!/usr/bin/env python3
"""Offline checks for /loop: a scheduled prompt fires by itself while the
session is idle, and keeps firing. Local mock endpoint only."""
import http.server
import importlib.util
import json
import os
import tempfile
import threading
import time
from pathlib import Path

spec = importlib.util.spec_from_file_location(
    'features', Path(__file__).with_name('smoke-features.py')
)
f = importlib.util.module_from_spec(spec)
spec.loader.exec_module(f)


class Endpoint(http.server.BaseHTTPRequestHandler):
    posts = 0

    def log_message(self, *_):
        pass

    def do_GET(self):
        data = json.dumps({'data': [{'id': 'mock', 'name': 'Mock model'}]}).encode()
        self.send_response(200)
        self.send_header('Content-Length', str(len(data)))
        self.end_headers()
        self.wfile.write(data)

    def do_POST(self):
        self.rfile.read(int(self.headers['Content-Length']))
        type(self).posts += 1
        frames = [{'choices': [{'delta': {'content': 'LOOP-ACK'}}]}]
        data = (''.join('data: ' + json.dumps(x) + '\n\n' for x in frames) + 'data: [DONE]\n\n').encode()
        self.send_response(200)
        self.send_header('Content-Type', 'text/event-stream')
        self.send_header('Content-Length', str(len(data)))
        self.end_headers()
        try:
            self.wfile.write(data)
        except (BrokenPipeError, ConnectionResetError):
            pass


def main():
    with tempfile.TemporaryDirectory(prefix='gc-loop-') as tmp:
        root = Path(tmp)
        repo = root / 'repo'
        repo.mkdir()
        cfg = root / 'config/glass-core'
        cfg.mkdir(parents=True)
        server = http.server.ThreadingHTTPServer(('127.0.0.1', 0), Endpoint)
        threading.Thread(target=server.serve_forever, daemon=True).start()
        (cfg / 'config.toml').write_text(
            f'provider="openai"\nmodel="mock"\napi_key_env=""\n'
            f'base_url="http://127.0.0.1:{server.server_port}/v1"\ntrust="always"\n'
        )
        env = dict(os.environ, XDG_CONFIG_HOME=str(root / 'config'), XDG_DATA_HOME=str(root / 'data'))
        try:
            t = f.Terminal(repo, env, size=(100, 30))
            try:
                t.idle()
                t.send('/loop 1s say tick')
                t.visible('loop added: every 1s', timeout=10)
                # No further input: the loop itself must produce turns.
                deadline = time.time() + 25
                while time.time() < deadline and Endpoint.posts < 2:
                    time.sleep(0.2)
                assert Endpoint.posts >= 2, (
                    f'the loop fired {Endpoint.posts} turn(s); expected at least 2'
                )
                assert t.wait('LOOP-ACK', timeout=10), 'loop turns must reach the model'
                t.send('/loop list')
                t.visible('ran', timeout=10)
                t.send('/loop clear')
                t.visible('all loops cleared', timeout=10)
            finally:
                t.close()
        finally:
            server.shutdown()
    print(
        'PASS /loop: a 1s loop fires turns by itself while idle, lists its '
        'run count, and clears'
    )


if __name__ == '__main__':
    main()
