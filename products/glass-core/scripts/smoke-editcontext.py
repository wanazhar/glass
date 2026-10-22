#!/usr/bin/env python3
"""Offline checks for `/forget` / `/unforget`: a hidden turn leaves the model
view while the raw log keeps it, and restoring brings it back. Local mock
endpoint only."""
import glob
import http.server
import importlib.util
import json
import os
import tempfile
import threading
from pathlib import Path

spec = importlib.util.spec_from_file_location(
    'features', Path(__file__).with_name('smoke-features.py')
)
f = importlib.util.module_from_spec(spec)
spec.loader.exec_module(f)

REPLIES = {
    'first probe': 'FIRST-ANSWER',
    'second probe': 'SECOND-ANSWER',
    'third probe': 'THIRD-ANSWER',
    'fourth probe': 'FOURTH-ANSWER',
}


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
        users = [m.get('content') for m in body.get('messages', []) if m.get('role') == 'user']
        reply = 'MOCK-ANSWER'
        for content in reversed(users):
            if isinstance(content, str) and content in REPLIES:
                reply = REPLIES[content]
                break
        frames = [{'choices': [{'delta': {'content': reply}}]}]
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
    with tempfile.TemporaryDirectory(prefix='gc-editcontext-') as tmp:
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
                t.send('first probe')
                t.wait('FIRST-ANSWER')
                t.idle()
                t.send('second probe')
                t.wait('SECOND-ANSWER')
                t.idle()

                t.send('/forget 1')
                t.visible('hidden from future context', timeout=10)
                t.send('third probe')
                t.wait('THIRD-ANSWER')
                t.idle()
                last = json.dumps(Endpoint.requests[-1])
                assert 'first probe' not in last and 'FIRST-ANSWER' not in last, last
                assert 'second probe' in last and 'SECOND-ANSWER' in last, last

                t.send('/unforget 1')
                t.visible('restored to future context', timeout=10)
                t.send('fourth probe')
                t.wait('FOURTH-ANSWER')
                t.idle()
                last = json.dumps(Endpoint.requests[-1])
                assert 'first probe' in last and 'FIRST-ANSWER' in last, last

                # The raw log kept everything the whole time.
                logs = glob.glob(str(root / 'data/glass-core/*.jsonl'))
                text = ''.join(Path(path).read_text() for path in logs)
                assert 'first probe' in text, 'raw history is untouched'
            finally:
                t.close()
        finally:
            server.shutdown()
    print(
        'PASS context edits: /forget removes a turn from the model view while '
        'the log keeps it, /unforget restores it'
    )


if __name__ == '__main__':
    main()
