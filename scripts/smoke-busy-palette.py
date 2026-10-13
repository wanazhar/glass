#!/usr/bin/env python3
"""Offline release PTYs: busy palette cancellation and reader restoration."""
import http.server
import json
import os
from pathlib import Path
import tempfile
import threading
from pty_vt import Terminal

ROOT = Path(__file__).resolve().parents[1]
os.environ.setdefault('GLASS_CORE_TEST_BINARY', str(ROOT / 'target/release/glass-core'))

class Server(http.server.ThreadingHTTPServer):
    daemon_threads = True

class Endpoint(http.server.BaseHTTPRequestHandler):
    def log_message(self, *_): pass
    def do_POST(self):
        self.rfile.read(int(self.headers['Content-Length']))
        hold = self.server.hold
        self.send_response(200)
        self.send_header('Content-Type', 'text/event-stream')
        self.end_headers()
        try:
            frame = {'choices': [{'delta': {'content': 'R4-WAIT\n' if hold else 'R4-DONE'}}]}
            self.wfile.write(('data: ' + json.dumps(frame) + '\n\n').encode())
            self.wfile.flush()
            if hold: self.server.release.wait(30)
            self.wfile.write(b'data: [DONE]\n\n')
            self.wfile.flush()
        except (BrokenPipeError, ConnectionResetError): pass

with tempfile.TemporaryDirectory(prefix='gc-busy-palette-') as tmp:
    root = Path(tmp)
    cfg = root / 'config/glass-core'
    cfg.mkdir(parents=True)
    server = Server(('127.0.0.1', 0), Endpoint)
    server.release = threading.Event()
    threading.Thread(target=server.serve_forever, daemon=True).start()
    (cfg / 'config.toml').write_text(f'provider="openai"\nmodel="mock"\napi_key_env=""\ntrust="always"\nbase_url="http://127.0.0.1:{server.server_port}/v1"\n')
    env = dict(os.environ, XDG_CONFIG_HOME=str(root / 'config'), XDG_DATA_HOME=str(root / 'data'))
    try:
        for size in [(20, 8), (40, 24), (80, 30)]:
            server.hold = True
            server.release.clear()
            tty = Terminal(root, env, size)
            try:
                tty.idle()
                tty.send('hello\r')
                tty.wait('R4-WAIT')
                tty.send('/')
                tty.collect(.2)
                tty.send('outputs\r')
                tty.wait('following')
                tty.send('/')
                tty.collect(.2)
                tty.send('\x1b')
                tty.collect(.3)
                assert 'following' in tty.vt.text(), tty.vt.text()
                tty.send('/')
                tty.collect(.2)
                tty.send('\x03')
                tty.wait('state=idle')
                assert 'Search actions' not in tty.vt.text(), tty.vt.text()
                server.hold = False
                server.release.set()
                tty.send('follow-up\r')
                tty.wait('R4-DONE')
                print(f'PASS {size}: busy reader restored, Ctrl-C cancels from palette, follow-up works')
            finally:
                server.release.set()
                tty.close()
    finally:
        server.shutdown()
