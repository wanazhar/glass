#!/usr/bin/env python3
"""Offline live-turn PTY gates with real Unicode/autowrap/scroll-region VT parsing."""
import http.server
import json
import os
from pathlib import Path
import re
import subprocess
import tempfile
import threading
import time
from pty_vt import Terminal

class Endpoint(http.server.BaseHTTPRequestHandler):
    gate = threading.Event()
    second = threading.Event()
    mode = 'reasoning'
    def log_message(self, *_): pass
    def do_POST(self):
        body = json.loads(self.rfile.read(int(self.headers['Content-Length'])))
        latest = max(i for i, m in enumerate(body['messages']) if m['role'] == 'user')
        results = [m for m in body['messages'][latest + 1:] if m['role'] == 'tool']
        self.send_response(200); self.send_header('Content-Type', 'text/event-stream'); self.end_headers()
        def emit(delta):
            self.wfile.write(('data: ' + json.dumps({'choices': [{'delta': delta}]}) + '\n\n').encode()); self.wfile.flush()
        try:
            if not results:
                if self.mode == 'reasoning':
                    emit({'reasoning_content': 'Public summary first.\n'})
                    assert self.gate.wait(15)
                    emit({'reasoning_content': '\n'.join(f'Provider summary row {i}.' for i in range(25)) + '\nPublic summary last.\n'})
                emit({'tool_calls': [{'index': 0, 'id': 'one', 'function': {'name': 'bash', 'arguments': json.dumps({'command': 'sleep 1; printf first-live-output'})}}]})
            elif len(results) == 1:
                self.second.set()
                emit({'tool_calls': [{'index': 0, 'id': 'two', 'function': {'name': 'bash', 'arguments': json.dumps({'command': 'sleep 1; printf second-live-output'})}}]})
            else:
                emit({'content': '# Final answer\n\n' + 'Readable complete conversation remains recorded. ' * 30 + '\n\nFINAL-LIVE-END'})
            self.wfile.write(b'data: [DONE]\n\n'); self.wfile.flush()
        except (BrokenPipeError, ConnectionResetError): pass

def compact(text):
    # The modal frames every physical row; strip the frame before joining
    # wrapped text so a word split at a row edge remains searchable.
    return ''.join(c for c in text.replace('│', '') if not c.isspace() and c not in '▲▼█║')

with tempfile.TemporaryDirectory(prefix='gc-live-pty-') as tmp:
    root = Path(tmp); subprocess.run(['git', 'init', '-q', str(root)], check=True)
    cfg = root / 'config/glass-core'; cfg.mkdir(parents=True)
    server = http.server.ThreadingHTTPServer(('127.0.0.1', 0), Endpoint)
    threading.Thread(target=server.serve_forever, daemon=True).start()
    (cfg / 'config.toml').write_text(f'provider="openai"\nmodel="mock"\napi_key_env=""\nbase_url="http://127.0.0.1:{server.server_port}/v1"\n')
    env = dict(os.environ, XDG_CONFIG_HOME=str(root / 'config'), XDG_DATA_HOME=str(root / 'data'))
    env.pop('NO_COLOR', None)
    try:
        for size in [(65,30), (40,24), (24,16), (20,12), (20,8)]:
            Endpoint.mode = 'reasoning'; Endpoint.gate.clear(); Endpoint.second.clear()
            t = Terminal(root, env, size)
            try:
                t.idle(); t.send('live fixture\r'); t.wait('Reasoning')
                t.send('draft'); t.visible('draft'); t.send('\x14'); t.visible('Reasoning' if size[0]<=40 else 'Provider reasonin')
                Endpoint.gate.set(); t.collect(.4)
                assert 'Publicsummarylast.' in compact(t.vt.text()), t.vt.text()
                t.send('\x1b[H'); t.collect(.15)
                assert 'Publicsummaryfirst.' in compact(t.vt.text()), t.vt.text()
                t.send('\x1b[F'); t.collect(.15)
                assert 'Publicsummarylast.' in compact(t.vt.text()), t.vt.text()
                t.send('\x07'); t.visible('following')
                assert Endpoint.second.wait(8), 'Provider/tool execution stopped in a viewer'
                t.send('\x07'); t.collect(.2); t.visible('draft')
                for width,height in [(20,8), (65,30), (20,12), size]:
                    t.resize(width,height); t.send('\x07'); t.visible('following')
                    t.resize(*size); t.send('\x1b'); t.collect(.15)
                    t.visible('draft')
                    assert len([r for r in t.vt.text().splitlines() if r.strip() and set(r.strip()) == {'─'}]) <= 1, t.vt.text()

                if 'FINAL-LIVE-END' not in t.seen: t.wait('FINAL-LIVE-END')
                t.collect(.2)
                assert 'draft' in t.vt.text(), t.vt.text()
                logs = sorted((root/'data/glass-core').glob('*.jsonl'), key=lambda p:p.stat().st_mtime_ns)
                records = [json.loads(line) for line in logs[-1].read_text().splitlines()]
                assert len([r for r in records if r.get('k') == 'assistant' and 'FINAL-LIVE-END' in r.get('t','')]) == 1
                reasoning = ''.join(r.get('t','') for r in records if r.get('k') == 'reasoning')
                assert reasoning.count('Public summary first.') == 1 and reasoning.count('Public summary last.') == 1
                assert t.seen.count('\x1b[?1049h') == 1, 'Nested alternate screens'
                Path(f'/tmp/gc-live-final-{size[0]}x{size[1]}.txt').write_text(t.vt.text())
            finally: Endpoint.gate.set(); t.close()
        Endpoint.mode='unsupported'; t=Terminal(root, env, (20,8))
        try:
            t.idle();t.send('unsupported fixture\r');t.wait('running');t.send('\x14');t.visible('Reasoning');t.collect(.1)
            t.send('\x1b[H');t.collect(.1)
            assert 'No provider' in t.vt.text(), t.vt.text()
            t.send('\x03');t.wait('Cancelled');t.collect(.2)
            assert '❯' in t.vt.text() and 'Provider reasoning' not in t.vt.text(), t.vt.text()
        finally:t.close()
    finally:Endpoint.gate.set();server.shutdown()
print('PASS: accurate VT live summaries, scroll/follow, busy transcript and continued tools, preserved drafts, resize, one composer, cancellation and exact records at five sizes')
