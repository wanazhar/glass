#!/usr/bin/env python3
"""Release-wire regressions for effective reasoning, retry cost and read tracking."""
import http.server
import json
import os
from pathlib import Path
import socket
import subprocess
import tempfile
import threading
from pty_vt import Terminal

ROOT = Path(__file__).resolve().parents[1]
BIN = Path(os.environ.get('GLASS_CORE_TEST_BINARY', ROOT / 'target/release/glass-core')).resolve()
os.environ.setdefault('GLASS_CORE_TEST_BINARY', str(BIN))
class Server(http.server.ThreadingHTTPServer):
    daemon_threads = True
class Endpoint(http.server.BaseHTTPRequestHandler):
    def log_message(self, *_): pass
    def do_POST(self):
        body = json.loads(self.rfile.read(int(self.headers['Content-Length'])))
        state = self.server
        state.requests.append(body)
        number = len(state.requests)
        mode = state.mode
        if mode == 'drop' or mode == 'fallback-drop' and number == 2:
            self.connection.shutdown(socket.SHUT_RDWR); self.connection.close(); return
        if mode == 'http' and number == 1 or mode == 'fallback-drop' and number == 1:
            data = b'{"error":{"message":"busy"}}'
            self.send_response(503 if mode == 'http' else 429)
            self.send_header('Content-Length', str(len(data))); self.end_headers(); self.wfile.write(data); return
        if mode == 'reasoning' and number == 1:
            delta = {'reasoning_content': 'Provider summary without an answer.'}
        elif mode == 'read' and number <= 2:
            command = 'head -n 5 a.rs' if number == 1 else 'printf NEW > a.rs'
            delta = {'tool_calls': [{'index': 0, 'id': f'call-{number}', 'function': {'name': 'bash', 'arguments': json.dumps({'command': command})}}]}
        else:
            delta = {'content': 'R2-DONE' if mode == 'reasoning' else f'R2-{mode.upper()}-DONE'}
        data = ('data: ' + json.dumps({'choices': [{'delta': delta}]}) + '\n\n' +
                'data: ' + json.dumps({'choices': [{'delta': {}, 'finish_reason': 'stop'}]}) + '\n\ndata: [DONE]\n\n').encode()
        self.send_response(200); self.send_header('Content-Type', 'text/event-stream'); self.send_header('Content-Length', str(len(data)))
        self.end_headers(); self.wfile.write(data)

with tempfile.TemporaryDirectory(prefix='gc-round2-wire-') as tmp:
    root = Path(tmp); cfg = root / 'config/glass-core'; cfg.mkdir(parents=True)
    server = Server(('127.0.0.1', 0), Endpoint); server.mode = 'reasoning'; server.requests = []
    threading.Thread(target=server.serve_forever, daemon=True).start()
    env = dict(os.environ, XDG_CONFIG_HOME=str(root / 'config'), XDG_DATA_HOME=str(root / 'data'))
    def configure(fallback=()):
        (cfg / 'config.toml').write_text(f'provider="openai"\nmodel="gpt-5-fixture"\napi_key_env=""\ntrust="always"\nreasoning_budget=32768\nbase_url="http://127.0.0.1:{server.server_port}/v1"\nfallback={json.dumps(list(fallback))}\n')
    try:
        configure()
        for size in [(20, 8), (40, 24), (80, 30)]:
            server.mode = 'reasoning'; server.requests = []
            tty = Terminal(root, env, size)
            try:
                tty.idle(); tty.send('finish this task\r'); tty.wait('R2-DONE')
                assert len(server.requests) == 2
                assert [request.get('reasoning_effort') for request in server.requests] == ['high', 'low'], server.requests
                print(f'PASS {size}: async reasoning recovery sends the reduced budget')
            finally:
                tty.close()
        for mode, expected in [('drop', 1), ('fallback-drop', 2), ('http', 2), ('read', 3)]:
            configure(['gpt-5-backup', 'gpt-5-third'] if mode in ['drop', 'fallback-drop'] else [])
            server.mode = mode; server.requests = []
            if mode == 'read': (root / 'a.rs').write_text('OLD\n')
            result = subprocess.run([str(BIN), 'run', '--json', 'finish task'], cwd=root, env=env, capture_output=True, text=True, timeout=20)
            if mode in ['drop', 'fallback-drop']:
                assert result.returncode != 0, (result.stdout, result.stderr)
                assert len(server.requests) == expected, (mode, server.requests)
            else:
                assert result.returncode == 0 and f'R2-{mode.upper()}-DONE' in result.stdout, (result.stdout, result.stderr)
                assert len(server.requests) == expected
            if mode == 'read': assert (root / 'a.rs').read_text() == 'NEW', result.stdout
            print(f'PASS {mode}: expected {expected} model requests; no ambiguous replay')
    finally:
        server.shutdown()
