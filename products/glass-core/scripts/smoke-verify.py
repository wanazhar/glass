#!/usr/bin/env python3
"""Offline checks for the auto-verify extension: a write triggers the verifier
through the host bridge, a failure is appended to the tool result, and the
next success reports back-to-green. Local mock endpoint only."""
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
    seen = None  # populated with the request that carried both tool results

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
        tool_messages = [m for m in body.get('messages', []) if m.get('role') == 'tool']
        if len(tool_messages) >= 2:
            type(self).seen = body
            frames = [{'choices': [{'delta': {'content': 'VERIFY-SMOKE-OK'}}]}]
        elif len(tool_messages) == 1:
            # The second write also creates the file the verifier looks for,
            # flipping it from failure to success.
            frames = [
                {
                    'choices': [
                        {
                            'delta': {
                                'tool_calls': [
                                    {
                                        'index': 0,
                                        'id': 'write-2',
                                        'function': {
                                            'name': 'bash',
                                            'arguments': json.dumps(
                                                {'command': 'echo done > pass'}
                                            ),
                                        },
                                    }
                                ]
                            }
                        }
                    ]
                }
            ]
        else:
            frames = [
                {
                    'choices': [
                        {
                            'delta': {
                                'tool_calls': [
                                    {
                                        'index': 0,
                                        'id': 'write-1',
                                        'function': {
                                            'name': 'bash',
                                            'arguments': json.dumps(
                                                {'command': "echo hi > out.txt"}
                                            ),
                                        },
                                    }
                                ]
                            }
                        }
                    ]
                }
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


def main():
    with tempfile.TemporaryDirectory(prefix='gc-verify-') as tmp:
        root = Path(tmp)
        repo = root / 'repo'
        repo.mkdir()
        cfg = root / 'config/glass-core'
        cfg.mkdir(parents=True)
        server = http.server.ThreadingHTTPServer(('127.0.0.1', 0), Endpoint)
        threading.Thread(target=server.serve_forever, daemon=True).start()
        extension = Path(__file__).resolve().parents[1] / 'extensions' / 'verify' / 'verify.py'
        extensions_json = json.dumps([f'python3 {extension}'])
        (cfg / 'config.toml').write_text(
            f'provider="openai"\nmodel="mock"\napi_key_env=""\n'
            f'base_url="http://127.0.0.1:{server.server_port}/v1"\n'
            f'trust="always"\nextensions={extensions_json}\n'
        )
        verify_cfg = root / 'verify.json'
        # Exits 0 only when ./pass exists, so the second write flips it.
        verify_cfg.write_text(json.dumps({'command': 'sh -c "test -f pass"', 'debounce_s': 0}))
        env = dict(
            os.environ,
            XDG_CONFIG_HOME=str(root / 'config'),
            XDG_DATA_HOME=str(root / 'data'),
            GC_VERIFY_CONFIG=str(verify_cfg),
        )
        try:
            p = subprocess.run(
                [str(f.BINARY), 'run', '--json', 'verify probe'],
                cwd=repo, env=env, text=True, capture_output=True, timeout=120,
            )
            assert p.returncode == 0, (p, p.stderr)
            r = json.loads(p.stdout)
            assert r['status'] == 'finished' and r['text'] == 'VERIFY-SMOKE-OK', r
            seen = Endpoint.seen
            assert seen, 'the mock never saw both tool results'
            tools = [m for m in seen['messages'] if m.get('role') == 'tool']
            first, second = json.dumps(tools[0]), json.dumps(tools[1])
            assert '[verify exit 1]' in first, first
            assert '[verify ok]' in second, second
        finally:
            server.shutdown()
    print(
        'PASS verify extension: writes trigger the verifier through the host '
        'bridge, failures append, recovery reports back-to-green'
    )


if __name__ == '__main__':
    main()
