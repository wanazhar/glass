#!/usr/bin/env python3
"""Offline checks for the sandbox policies: a tool-run command must be able to
write inside the workspace and denied outside it, under `workspace` (and, when
the kernel supports Landlock, under `strict` with reads narrowed too)."""
import http.server
import importlib.util
import json
import os
import shutil
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
    command = "echo ok > inside.txt"

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
        has_result = any(m.get('role') == 'tool' for m in body.get('messages', []))
        if has_result:
            frames = [{'choices': [{'delta': {'content': 'SANDBOX-DONE'}}]}]
        else:
            frames = [
                {
                    'choices': [
                        {
                            'delta': {
                                'tool_calls': [
                                    {
                                        'index': 0,
                                        'id': 'sandbox-1',
                                        'function': {
                                            'name': 'bash',
                                            'arguments': json.dumps({'command': Endpoint.command}),
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


def run_probe(root, policy, command, outside):
    repo = root / f'repo-{policy}'
    repo.mkdir()
    cfg = root / f'config-{policy}/glass-core'
    cfg.mkdir(parents=True)
    (cfg / 'config.toml').write_text(
        f'provider="openai"\nmodel="mock"\napi_key_env=""\n'
        f'base_url="http://127.0.0.1:{PORT}/v1"\nsandbox="{policy}"\ntrust="always"\n'
    )
    env = dict(os.environ, XDG_CONFIG_HOME=str(root / f'config-{policy}'), XDG_DATA_HOME=str(root / f'data-{policy}'))
    Endpoint.command = command
    p = subprocess.run(
        [str(f.BINARY), 'run', '--json', 'sandbox probe'],
        cwd=repo, env=env, text=True, capture_output=True, timeout=120,
    )
    assert p.returncode == 0, p
    r = json.loads(p.stdout)
    assert r['status'] == 'finished', r
    return repo


with tempfile.TemporaryDirectory(prefix='gc-sandbox-') as tmp:
    root = Path(tmp)
    # A probe directory outside every allowlisted write path. The temp dir
    # itself is allowed on purpose (builds need it), so the probe lives under
    # the home directory, which the jail does not permit.
    outside = Path.home() / f'gc-sandbox-probe-{os.getpid()}'
    outside.mkdir(exist_ok=True)
    (outside / 'secret.txt').write_text('OUTSIDE-SECRET\n')
    server = http.server.ThreadingHTTPServer(('127.0.0.1', 0), Endpoint)
    threading.Thread(target=server.serve_forever, daemon=True).start()
    PORT = server.server_port
    try:
        # workspace: inside writes succeed, outside writes are denied.
        repo = run_probe(
            root,
            'workspace',
            f"echo ok > inside.txt && {{ echo bad > '{outside}/written.txt' 2>/dev/null || echo OUTSIDE-DENIED; }}",
            outside,
        )
        assert (repo / 'inside.txt').exists(), 'workspace write must succeed'
        assert not (outside / 'written.txt').exists(), 'outside write must be denied'

        # strict: same write jail, and reads outside the allowlist denied.
        repo = run_probe(
            root,
            'strict',
            f"cat '{outside}/secret.txt' > read.txt 2>/dev/null || echo READ-DENIED > read.txt; "
            "cat /etc/hostname > etc.txt 2>/dev/null || echo READ-DENIED > etc.txt; "
            "echo ok > inside.txt",
            outside,
        )
        if not (repo / 'inside.txt').exists():
            # Kernel without Landlock: strict refuses commands by design.
            print('SKIP strict assertions: this kernel cannot deliver the strict sandbox')
        else:
            assert 'READ-DENIED' in (repo / 'read.txt').read_text(), 'outside read must be denied'
            assert 'OUTSIDE-SECRET' not in (repo / 'read.txt').read_text()
            assert 'READ-DENIED' not in (repo / 'etc.txt').read_text(), (
                'system reads must stay allowed under strict'
            )
    finally:
        server.shutdown()
        shutil.rmtree(outside, ignore_errors=True)
    print(
        'PASS sandbox: workspace policy confines writes to the workspace; '
        'strict narrows reads as well (or is refused on unsupported kernels)'
    )
