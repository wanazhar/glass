#!/usr/bin/env python3
"""Offline checks for image input: a PNG attached with --attach rides as an
image block on the wire, lands in the session log, and reaches the model.
Local mock endpoint only."""
import base64
import glob
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

# Smallest well-known 1x1 PNG; the harness relays bytes and never decodes.
PNG = base64.b64decode(
    'iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNkYPhfDwAChwGA60e6kgAAAABJRU5ErkJggg=='
)


class Endpoint(http.server.BaseHTTPRequestHandler):
    saw_image = False

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
        if 'data:image/png;base64,' in text and 'iVBORw0KGgo' in text:
            type(self).saw_image = True
            frames = [{'choices': [{'delta': {'content': 'SAW-IMAGE'}}]}]
        else:
            frames = [{'choices': [{'delta': {'content': 'NO-IMAGE'}}]}]
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
    with tempfile.TemporaryDirectory(prefix='gc-image-') as tmp:
        root = Path(tmp)
        repo = root / 'repo'
        repo.mkdir()
        (repo / 'shot.png').write_bytes(PNG)
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
            p = subprocess.run(
                [str(f.BINARY), 'run', '--json', '--attach', 'shot.png', 'look at this'],
                cwd=repo, env=env, text=True, capture_output=True, timeout=120,
            )
            assert p.returncode == 0, (p, p.stderr)
            r = json.loads(p.stdout)
            assert r['status'] == 'finished' and r['text'] == 'SAW-IMAGE', r
            assert Endpoint.saw_image, 'the request carried no image block'

            # The image is part of the conversation log, so it survives resume.
            logs = glob.glob(str(root / 'data/glass-core/*.jsonl'))
            assert logs, 'no session log written'
            kinds = [json.loads(line).get('k') for path in logs for line in open(path)]
            assert 'image' in kinds, kinds
        finally:
            server.shutdown()
    print(
        'PASS image input: PNG attaches, travels as an image block, '
        'appears in the session log'
    )


if __name__ == '__main__':
    main()
