#!/usr/bin/env python3
"""Offline checks for the workspace trust gate: prompt, allow, deny, content
change re-prompt, and the headless --trust contract. Local mock endpoint."""
import http.server
import importlib.util
import json
import os
import subprocess
import tempfile
import threading
from pathlib import Path

spec = importlib.util.spec_from_file_location(
    'features', Path(__file__).with_name('smoke-features.py')
)
f = importlib.util.module_from_spec(spec)
spec.loader.exec_module(f)

MARKER = 'PROJECT-INSTRUCTION-MARKER'


class Endpoint(http.server.BaseHTTPRequestHandler):
    requests = []

    def log_message(self, *_):
        pass

    def do_GET(self):
        data = json.dumps({'data': [{'id': 'mock', 'name': 'Mock model'}]}).encode()
        self.send_response(200)
        self.send_header('Content-Length', str(len(data)))
        self.end_headers()
        self.wfile.write(data)

    def do_POST(self):
        body = json.loads(self.rfile.read(int(self.headers['Content-Length'])))
        type(self).requests.append(body)
        frames = [
            {'choices': [{'delta': {'content': 'ack'}}]},
            {'choices': [], 'usage': {'prompt_tokens': 7, 'completion_tokens': 1}},
        ]
        data = (''.join('data: ' + json.dumps(x) + '\n\n' for x in frames) + 'data: [DONE]\n\n').encode()
        self.send_response(200)
        self.send_header('Content-Type', 'text/event-stream')
        self.send_header('Content-Length', str(len(data)))
        self.end_headers()
        try:
            self.wfile.write(data)
        except (BrokenPipeError, ConnectionResetError):
            pass


def choose(t, label):
    os.write(t.master, (label + '\r').encode())


def last_request_has(text):
    return text in json.dumps(Endpoint.requests[-1]) if Endpoint.requests else False


def ask(t, prompt):
    t.received = ''
    t.send(prompt)
    t.wait('ack')
    t.idle()


def main():
    with tempfile.TemporaryDirectory(prefix='gc-trust-') as tmp:
        root = Path(tmp)
        repo = root / 'repo'
        repo.mkdir()
        (repo / 'AGENTS.md').write_text(f'{MARKER}\n')
        cfg = root / 'config/glass-core'
        cfg.mkdir(parents=True)
        server = http.server.ThreadingHTTPServer(('127.0.0.1', 0), Endpoint)
        threading.Thread(target=server.serve_forever, daemon=True).start()
        (cfg / 'config.toml').write_text(
            f'provider="openai"\nmodel="mock"\napi_key_env=""\n'
            f'base_url="http://127.0.0.1:{server.server_port}/v1"\n'
        )
        env = dict(os.environ, XDG_CONFIG_HOME=str(root / 'config'), XDG_DATA_HOME=str(root / 'data'))
        try:
            # Deny: the prompt appears, and the marker never reaches a request.
            t = f.Terminal(repo, env)
            try:
                t.visible('Project instructions', timeout=10)
                choose(t, 'Deny')
                t.idle()
                ask(t, 'first')
                assert not last_request_has(MARKER), 'denied instructions must not be sent'
            finally:
                t.close()

            # Stored deny is remembered: no prompt on the next start.
            t = f.Terminal(repo, env)
            try:
                t.idle()
                ask(t, 'second')
                assert not last_request_has(MARKER), 'stored deny must persist'
            finally:
                t.close()

            # Changed content is a new decision: allow it, and the marker is
            # sent from then on.
            (repo / 'AGENTS.md').write_text(f'{MARKER}-V2\n')
            t = f.Terminal(repo, env)
            try:
                t.visible('Project instructions', timeout=10)
                choose(t, 'Allow for this project')
                t.idle()
                ask(t, 'third')
                assert last_request_has(f'{MARKER}-V2'), 'allowed instructions must be sent'
            finally:
                t.close()

            # Same content: remembered allow, no prompt, marker still present.
            t = f.Terminal(repo, env)
            try:
                t.idle()
                ask(t, 'fourth')
                assert last_request_has(f'{MARKER}-V2'), 'stored allow must persist'
            finally:
                t.close()

            # Changed again: the prompt returns; denying keeps the new
            # content out.
            (repo / 'AGENTS.md').write_text(f'{MARKER}-V3\n')
            t = f.Terminal(repo, env)
            try:
                t.visible('Project instructions', timeout=10)
                choose(t, 'Deny')
                t.idle()
                ask(t, 'fifth')
                assert not last_request_has(f'{MARKER}-V3'), (
                    'changed content must be decided again'
                )
            finally:
                t.close()

            # Headless: skipped with a warning, unless --trust is passed.
            # A fresh workspace so no interactive decision is remembered.
            fresh = root / 'fresh'
            fresh.mkdir()
            (fresh / 'AGENTS.md').write_text(f'{MARKER}\n')
            p = subprocess.run(
                [str(f.BINARY), 'run', '--json', 'headless one'],
                cwd=fresh, env=env, text=True, capture_output=True, timeout=60,
            )
            assert p.returncode == 0, p
            assert 'not trusted' in p.stderr, p.stderr
            assert not last_request_has(MARKER), 'headless must skip untrusted instructions'
            p = subprocess.run(
                [str(f.BINARY), 'run', '--json', '--trust', 'headless two'],
                cwd=fresh, env=env, text=True, capture_output=True, timeout=60,
            )
            assert p.returncode == 0, p
            assert last_request_has(MARKER), '--trust must accept for the run'
        finally:
            server.shutdown()
    print(
        'PASS trust gate: prompts once per content with deny/allow remembered, '
        'content changes re-prompt, headless skips with a warning and honours --trust'
    )


if __name__ == '__main__':
    main()
