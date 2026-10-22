#!/usr/bin/env python3
"""Offline protocol check for the bundled web extension: handshake, web_fetch,
web_search, and explicit errors, against a local fixture server."""
import http.server
import json
import os
import subprocess
import sys
import tempfile
import threading
from pathlib import Path

REPO = Path(__file__).resolve().parents[1]
EXTENSION = REPO / 'extensions' / 'web' / 'web.py'


class Fixture(http.server.BaseHTTPRequestHandler):
    def log_message(self, *_):
        pass

    def do_GET(self):
        if self.path.startswith('/page'):
            body = (
                b'<html><body><h1>Doc</h1><p>Alpha '
                b'<a href="https://example.com/x">link</a> beta.</p>'
                b'<script>EVIL-SCRIPT</script></body></html>'
            )
        elif self.path.startswith('/results'):
            body = (
                b'<html><body>'
                b'<a class="result__a" href="//duckduckgo.com/l/?uddg=https%3A%2F%2Fexample.org%2Fa">Alpha Docs</a>'
                b'<a class="result__snippet">Alpha snippet.</a>'
                b'</body></html>'
            )
        else:
            self.send_response(404)
            self.send_header('Content-Length', '0')
            self.end_headers()
            return
        self.send_response(200)
        self.send_header('Content-Type', 'text/html; charset=utf-8')
        self.send_header('Content-Length', str(len(body)))
        self.end_headers()
        self.wfile.write(body)


def main():
    server = http.server.ThreadingHTTPServer(('127.0.0.1', 0), Fixture)
    threading.Thread(target=server.serve_forever, daemon=True).start()
    port = server.server_port
    with tempfile.TemporaryDirectory(prefix='gc-web-') as tmp:
        cfg = Path(tmp) / 'web.json'
        cfg.write_text(
            json.dumps({'search': 'ddg', 'allow_private': True, 'ddg_url': f'http://127.0.0.1:{port}/results'})
        )
        env = dict(os.environ, GC_WEB_CONFIG=str(cfg))
        p = subprocess.Popen(
            [sys.executable, str(EXTENSION)],
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            text=True,
            env=env,
        )

        def send(obj):
            p.stdin.write(json.dumps(obj) + '\n')
            p.stdin.flush()

        def recv():
            line = p.stdout.readline()
            assert line, 'extension closed the stream'
            return json.loads(line)

        send({'type': 'hello', 'id': 1})
        manifest = recv()
        assert manifest.get('protocol') == 1, manifest
        assert {t['name'] for t in manifest['tools']} == {'web_fetch', 'web_search'}, manifest

        send(
            {
                'type': 'run_tool',
                'id': 2,
                'name': 'web_fetch',
                'arguments': json.dumps({'url': f'http://127.0.0.1:{port}/page'}),
            }
        )
        out = recv()['output']
        assert 'Doc' in out and 'Alpha' in out and 'beta.' in out, out
        assert '<https://example.com/x>' in out, out
        assert 'EVIL-SCRIPT' not in out, 'scripts must not reach the model'

        send(
            {
                'type': 'run_tool',
                'id': 3,
                'name': 'web_search',
                'arguments': json.dumps({'query': 'alpha'}),
            }
        )
        out = recv()['output']
        assert 'Alpha Docs' in out and 'https://example.org/a' in out, out
        assert 'Alpha snippet.' in out, out

        send(
            {
                'type': 'run_tool',
                'id': 4,
                'name': 'web_fetch',
                'arguments': json.dumps({'url': f'http://127.0.0.1:{port}/missing'}),
            }
        )
        assert 'HTTP 404' in recv()['output']

        send({'type':'tool_result','id':5,'name':'view_image','body':'unchanged image reference'})
        assert recv()=={'id':5,'body':'unchanged image reference'}
        send({'type': 'goodbye', 'id': 6})
        p.stdin.close()
        p.wait(timeout=10)
    server.shutdown()
    print(
        'PASS web extension: fetch extracts prose and links without scripts, '
        'search parses and unwraps results, errors are explicit'
    )


if __name__ == '__main__':
    main()
