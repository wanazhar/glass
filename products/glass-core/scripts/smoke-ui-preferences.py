#!/usr/bin/env python3
"""Real PTY checks for animation selection and Ctrl-D confirmation."""
import http.server
import json
import os
from pathlib import Path
import tempfile
import threading
import time
from pty_vt import Terminal

class Endpoint(http.server.BaseHTTPRequestHandler):
    def log_message(self, *_): pass
    def do_POST(self):
        self.rfile.read(int(self.headers['Content-Length']))
        self.send_response(200)
        self.send_header('Content-Type', 'text/event-stream')
        self.end_headers()
        try:
            self.wfile.write(b'data: {"choices":[{"delta":{"reasoning_content":"Working"}}]}\n\n')
            self.wfile.flush()
            time.sleep(4)
            self.wfile.write(b'data: {"choices":[{"delta":{"content":"Done"}}]}\n\ndata: [DONE]\n\n')
        except (BrokenPipeError, ConnectionResetError): pass

with tempfile.TemporaryDirectory(prefix='gc-ui-preferences-') as tmp:
    root = Path(tmp)
    server = http.server.ThreadingHTTPServer(('127.0.0.1', 0), Endpoint)
    server.daemon_threads = True
    threading.Thread(target=server.serve_forever, daemon=True).start()
    cfg = root/'config/glass-core/config.toml'
    cfg.parent.mkdir(parents=True)
    cfg.write_text(f'provider="openai"\nmodel="fixture"\napi_key_env=""\nbase_url="http://127.0.0.1:{server.server_port}/v1"\ntrust="always"\n')
    env = dict(os.environ, XDG_CONFIG_HOME=str(root/'config'), XDG_DATA_HOME=str(root/'data'))
    env.pop('NO_COLOR', None)
    for size in ((20,8), (40,24), (80,30)):
        t = Terminal(root, env, size)
        try:
            t.idle()
            t.send('/animation\r')
            t.visible('Activity animation')
            t.send('\x1b[B' * 7)
            t.visible('Blink')
            t.send('\r')
            t.idle()
            if size == (80,30):
                t.send('/settings\r')
                t.visible('Preview ·')
                rows = t.vt.text().splitlines()
                row = next(i for i, text in enumerate(rows) if 'Preview ·' in text)
                column = rows[row].index('Preview')
                assert t.vt.active.buffer[row][column].fg != 'default'
                assert 'Command("' not in t.vt.text()
                t.send('\x1b'); t.collect(.2); t.send('\x1b'); t.idle()
                print('PASS distinct preview color and no internal enum text')
            t.send('draft stays')
            t.collect(.1)
            t.send('\x10')
            t.visible('Search actions')
            t.send('\x04'); t.visible('Exit glass-core?'); t.send('\x1b')
            t.visible('Search actions')
            t.send('\x1b'); t.visible('draft stays')
            t.send('\x04')
            t.visible('Exit glass-core?')
            t.send('\x1b')
            t.visible('draft stays')
            assert t.p.poll() is None
            t.send('\x04')
            t.visible('Exit glass-core?')
            t.send('\r')
            assert t.p.wait(timeout=3) == 0
            print('PASS animation scroll, exit cancel keeps draft, exit confirm:', size)
        finally: t.close()
    t = Terminal(root, env, (40,24))
    try:
        t.idle(); t.send('work\r'); t.wait('Reasoning')
        t.send('\x04'); t.visible('Exit glass-core?'); t.send('\x1b')
        t.collect(.2); assert t.p.poll() is None
        t.send('\x04'); t.visible('Exit glass-core?'); t.send('\r')
        assert t.p.wait(timeout=3) == 0
        print('PASS active-turn exit cancellation and confirmation')
    finally: t.close()
    server.shutdown()
