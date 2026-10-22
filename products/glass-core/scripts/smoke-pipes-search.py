#!/usr/bin/env python3
"""Release PTY regression: inherited pipes, dual overflow, exact search placement."""
import http.server, json, os, tempfile, threading, time
from pathlib import Path
from pty_vt import Terminal

class Provider(http.server.BaseHTTPRequestHandler):
    def log_message(self, *_): pass
    def do_POST(self):
        request = json.loads(self.rfile.read(int(self.headers['Content-Length'])))
        results = [m['content'] for m in request['messages'] if m['role'] == 'tool']
        if not results:
            delta = {'tool_calls': [{'index': 0, 'id': 'pipes', 'function': {'name': 'bash', 'arguments': json.dumps({'command': 'sleep 30 & echo PIPE_DONE', 'timeout_ms': 200})}}]}
        elif len(results) == 1:
            assert 'PIPE_DONE' in results[0]
            delta = {'tool_calls': [{'index': 0, 'id': 'detached', 'function': {'name': 'bash', 'arguments': json.dumps({'command': "setsid sh -c 'sleep 2' & echo DETACHED_DONE", 'timeout_ms': 200})}}]}
        elif len(results) == 2:
            assert 'capture incomplete' in results[-1], results[-1]
            delta = {'tool_calls': [{'index': 0, 'id': 'overflow', 'function': {'name': 'bash', 'arguments': json.dumps({'command': "printf 'SEARCH_NEEDLE\\n'; seq 1 40000; seq 40001 80000 >&2"})}}]}
        else:
            assert 'stderr overflow saved to ' in results[-1], results[-1]
            delta = {'content': 'ROUND3_DONE'}
        self.send_response(200)
        self.send_header('Content-Type', 'text/event-stream')
        self.end_headers()
        self.wfile.write(('data: ' + json.dumps({'choices': [{'delta': delta}]}) + '\n\ndata: [DONE]\n\n').encode())

for size in [(80,30), (40,24), (20,8)]:
    with tempfile.TemporaryDirectory(prefix='gc-round3-') as tmp:
        root = Path(tmp)
        cfg = root/'config/glass-core'
        cfg.mkdir(parents=True)
        server = http.server.ThreadingHTTPServer(('127.0.0.1',0), Provider)
        threading.Thread(target=server.serve_forever,daemon=True).start()
        (cfg/'config.toml').write_text(f'provider="openai"\nmodel="fixture"\napi_key_env=""\nbase_url="http://127.0.0.1:{server.server_port}/v1"\ntrust="always"\n')
        env = dict(os.environ, XDG_CONFIG_HOME=str(root/'config'), XDG_DATA_HOME=str(root/'data'))
        terminal = Terminal(root, env, size)
        try:
            terminal.idle()
            start = time.monotonic()
            terminal.send('exercise regression\r')
            terminal.wait('ROUND3_DONE',15)
            assert time.monotonic()-start < 15, 'inherited pipes blocked turn'
            terminal.collect(.3)
            terminal.send('\x1b[5~\x06SEARCH_NEEDLE')
            terminal.collect(.5)
            assert 'SEARCH_NEEDLE' in terminal.vt.text(), terminal.vt.text()
            if size[0] >= 40:
                assert any('SEARCH_NEEDLE' in ''.join(row[x].data for x in sorted(row)) for y,row in terminal.vt.active.buffer.items() if 1 <= y < size[1]-5), terminal.vt.text()
            print('PASS inherited pipes, dual spill, search',size,flush=True)
        finally:
            terminal.close()
            server.shutdown()
            server.server_close()
