#!/usr/bin/env python3
"""Offline checks for OSC 7501 program status: a turn reports `working` then
`done`, and `program_status = "off"` keeps the byte stream clean. Local mock
endpoint only."""
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


class Endpoint(http.server.BaseHTTPRequestHandler):
    def log_message(self, *_):
        pass

    def do_GET(self):
        data = json.dumps({'data': [{'id': 'mock', 'name': 'Mock model'}]}).encode()
        self.send_response(200)
        self.send_header('Content-Length', str(len(data)))
        self.end_headers()
        self.wfile.write(data)

    def do_POST(self):
        self.rfile.read(int(self.headers['Content-Length']))
        frames = [{'choices': [{'delta': {'content': 'STATUS-OK'}}]}]
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
    with tempfile.TemporaryDirectory(prefix='gc-status-') as tmp:
        root = Path(tmp)
        repo = root / 'repo'
        repo.mkdir()
        cfg = root / 'config/glass-core'
        cfg.mkdir(parents=True)
        server = http.server.ThreadingHTTPServer(('127.0.0.1', 0), Endpoint)
        threading.Thread(target=server.serve_forever, daemon=True).start()
        base = (
            f'provider="openai"\nmodel="mock"\napi_key_env=""\n'
            f'base_url="http://127.0.0.1:{server.server_port}/v1"\ntrust="always"\n'
        )
        (cfg / 'config.toml').write_text(base)  # program_status defaults to on
        env = dict(os.environ, XDG_CONFIG_HOME=str(root / 'config'), XDG_DATA_HOME=str(root / 'data'))
        try:
            t = f.Terminal(repo, env, size=(100, 30))
            try:
                t.idle()
                t.send('status probe')
                # `wait` consumes the buffer on a match, so ask for the two
                # frames in the order they arrive and let each one match.
                assert t.wait('7501;state=working', timeout=15), t.received
                assert t.wait('7501;state=done', timeout=15), t.received
                t.idle()
            finally:
                t.close()

            # program_status = "off" must keep the stream clean.
            (cfg / 'config.toml').write_text(base + 'program_status = "off"\n')
            off = f.Terminal(repo, env, size=(100, 30))
            try:
                off.idle()
                assert ']7501;' not in off.received, off.received
                off.send('status probe off')
                off.wait('STATUS-OK', timeout=15)
                off.idle()
                assert ']7501;' not in off.received, off.received
            finally:
                off.close()
        finally:
            server.shutdown()
    print(
        'PASS program status: OSC 7501 reports working then done, and '
        'program_status = "off" emits nothing'
    )


if __name__ == '__main__':
    main()
