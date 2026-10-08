#!/usr/bin/env python3
"""Offline checks for /oracle: the configured alternate model receives the
diff, the read-only path offers the inspect tool and no shell, and the
critique prints inline. Local mock endpoint only."""
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

REPO = Path(__file__).resolve().parents[1]
ORACLE = REPO / 'extensions' / 'oracle' / 'oracle.py'


class Endpoint(http.server.BaseHTTPRequestHandler):
    saw_model = False
    saw_read_only = False
    saw_diff = False

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
        if body.get('model') == 'oracle-model':
            type(self).saw_model = True
        tool_names = [t.get('function', {}).get('name') for t in body.get('tools', [])]
        if 'inspect' in tool_names and 'bash' not in tool_names:
            type(self).saw_read_only = True
        if '+changed-line' in text:
            type(self).saw_diff = True
        frames = [{'choices': [{'delta': {'content': 'ORACLE-SAYS'}}]}]
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
    with tempfile.TemporaryDirectory(prefix='gc-oracle-') as tmp:
        root = Path(tmp)
        repo = root / 'repo'
        repo.mkdir()
        subprocess.run(['git', 'init', '-q', str(repo)], check=True)
        subprocess.run(
            ['git', '-C', str(repo), '-c', 'user.email=t@t', '-c', 'user.name=t',
             'commit', '-q', '--allow-empty', '-m', 'init'],
            check=True,
        )
        (repo / 'app.py').write_text('value = 1\nchanged-line = True\n')
        subprocess.run(['git', '-C', str(repo), 'add', 'app.py'], check=True)
        subprocess.run(
            ['git', '-C', str(repo), '-c', 'user.email=t@t', '-c', 'user.name=t',
             'commit', '-q', '-m', 'add app'],
            check=True,
        )
        (repo / 'app.py').write_text('value = 1\nchanged-line = False\n')
        cfg = root / 'config/glass-core'
        cfg.mkdir(parents=True)
        server = http.server.ThreadingHTTPServer(('127.0.0.1', 0), Endpoint)
        threading.Thread(target=server.serve_forever, daemon=True).start()
        extensions_json = json.dumps([f'python3 {ORACLE}'])
        (cfg / 'config.toml').write_text(
            f'provider="openai"\nmodel="mock"\napi_key_env=""\n'
            f'base_url="http://127.0.0.1:{server.server_port}/v1"\n'
            f'trust="always"\nextensions={extensions_json}\n'
        )
        (cfg / 'oracle.json').write_text(json.dumps({'model': 'oracle-model'}))
        env = dict(os.environ, XDG_CONFIG_HOME=str(root / 'config'), XDG_DATA_HOME=str(root / 'data'))
        try:
            t = f.Terminal(repo, env, size=(100, 30))
            try:
                t.idle()
                t.send('/oracle any risks?')
                t.visible('ORACLE-SAYS', timeout=30)
            finally:
                t.close()
            assert Endpoint.saw_model, 'the oracle did not use the configured model'
            assert Endpoint.saw_read_only, 'the oracle must run read-only (inspect, no bash)'
            assert Endpoint.saw_diff, 'the diff did not reach the reviewer'
        finally:
            server.shutdown()
    print(
        'PASS oracle: the configured model reviews the current diff through a '
        'read-only nested agent and the critique prints inline'
    )


if __name__ == '__main__':
    main()
