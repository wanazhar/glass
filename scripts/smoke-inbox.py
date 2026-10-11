#!/usr/bin/env python3
"""Offline checks for the session inbox: `gc send` delivers a message to an
idle session, which runs a turn containing it. Local mock endpoint only."""
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


class Endpoint(http.server.BaseHTTPRequestHandler):
    saw_inbox = False

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
        text = json.dumps(body)
        if 'ping-from-test' in text and '[Background follow-up from cli]' in text:
            type(self).saw_inbox = True
        frames = [{'choices': [{'delta': {'content': 'INBOX-ACK'}}]}]
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
    with tempfile.TemporaryDirectory(prefix='gc-inbox-') as tmp:
        root = Path(tmp)
        repo = root / 'repo'
        repo.mkdir()
        cfg = root / 'config/glass-core'
        cfg.mkdir(parents=True)
        data = root / 'data/glass-core'
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
                logs = sorted(data.glob('*.jsonl'), key=lambda p: p.stat().st_mtime)
                assert logs, 'the session log must exist'
                session_id = logs[-1].stem

                # Invalid targets are refused before any file is touched.
                bad = subprocess.run(
                    [str(f.BINARY), 'send', '../evil', 'x'],
                    cwd=repo, env=env, text=True, capture_output=True, timeout=30,
                )
                assert bad.returncode != 0 and 'not a session id' in (bad.stdout + bad.stderr), bad

                p = subprocess.run(
                    [str(f.BINARY), 'send', session_id, 'ping-from-test'],
                    cwd=repo, env=env, text=True, capture_output=True, timeout=30,
                )
                assert p.returncode == 0, p
                # The idle session picks it up and runs a turn.
                t.wait('INBOX-ACK', timeout=20)
                assert Endpoint.saw_inbox, (
                    'the turn must carry the message and the attributed background follow-up'
                )
            finally:
                t.close()
        finally:
            server.shutdown()
    print(
        'PASS inbox: gc send delivers to an idle session, which runs a turn '
        'carrying the message and its provenance; invalid targets are refused'
    )


if __name__ == '__main__':
    main()
