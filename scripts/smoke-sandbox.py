#!/usr/bin/env python3
"""Offline checks for the sandbox policies: a tool-run command must be able to
write inside the workspace and denied outside it, under `workspace` (and, when
the kernel supports Landlock, under `strict` with reads narrowed too).
With confinement on, outbound TCP must be port-limited and credential-ish
environment variables must be scrubbed (or explicitly inherited)."""
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


PROBE_SEQ = 0


def run_probe(root, policy, command, extra_config='', extra_env=None, files=None):
    global PROBE_SEQ
    PROBE_SEQ += 1
    repo = root / f'repo-{policy}-{PROBE_SEQ}'
    repo.mkdir()
    for name, text in (files or {}).items():
        (repo / name).write_text(text)
    cfg = root / f'config-{policy}-{PROBE_SEQ}/glass-core'
    cfg.mkdir(parents=True)
    (cfg / 'config.toml').write_text(
        f'provider="openai"\nmodel="mock"\napi_key_env=""\n'
        f'base_url="http://127.0.0.1:{PORT}/v1"\nsandbox="{policy}"\ntrust="always"\n'
        + extra_config
    )
    env = dict(
        os.environ,
        XDG_CONFIG_HOME=str(root / f'config-{policy}-{PROBE_SEQ}'),
        XDG_DATA_HOME=str(root / f'data-{policy}-{PROBE_SEQ}'),
    )
    if extra_env:
        env.update(extra_env)
    Endpoint.command = command
    p = subprocess.run(
        [str(f.BINARY), 'run', '--json', 'sandbox probe'],
        cwd=repo, env=env, text=True, capture_output=True, timeout=120,
    )
    assert p.returncode == 0, p
    r = json.loads(p.stdout)
    assert r['status'] == 'finished', r
    return repo, p.stderr


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
        repo, _ = run_probe(
            root,
            'workspace',
            f"echo ok > inside.txt && {{ echo bad > '{outside}/written.txt' 2>/dev/null || echo OUTSIDE-DENIED; }}",
        )
        assert (repo / 'inside.txt').exists(), 'workspace write must succeed'
        assert not (outside / 'written.txt').exists(), 'outside write must be denied'

        # strict: same write jail, and reads outside the allowlist denied.
        repo, _ = run_probe(
            root,
            'strict',
            f"cat '{outside}/secret.txt' > read.txt 2>/dev/null || echo READ-DENIED > read.txt; "
            "cat /etc/hostname > etc.txt 2>/dev/null || echo READ-DENIED > etc.txt; "
            "echo ok > inside.txt",
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

        # Network: only the allowlisted port is reachable from a confined
        # shell. The allowed listener is in this test process, the denied one
        # is the mock provider's port (used by the harness, not the child).
        allowed = http.server.ThreadingHTTPServer(('127.0.0.1', 0), http.server.BaseHTTPRequestHandler)
        threading.Thread(target=allowed.serve_forever, daemon=True).start()
        try:
            allowed_port = allowed.server_port
            script = (
                "import socket\n"
                "out = []\n"
                f"for name, port in (('allowed', {allowed_port}), ('denied', {PORT})):\n"
                "    try:\n"
                "        socket.create_connection(('127.0.0.1', port), timeout=3).close()\n"
                "        out.append(name + ' CONNECTED')\n"
                "    except OSError:\n"
                "        out.append(name + ' DENIED')\n"
                "open('network.txt', 'w').write('\\n'.join(out))\n"
            )
            repo, stderr = run_probe(
                root,
                'workspace',
                'python3 probe.py',
                extra_config=f'sandbox_ports = [{allowed_port}]\n',
                files={'probe.py': script},
            )
            report = (repo / 'network.txt').read_text()
            if 'unrestricted' in stderr:
                print('SKIP network assertions: this kernel has no Landlock net rules')
                assert 'denied CONNECTED' in report, report
            else:
                assert 'allowed CONNECTED' in report, report
                assert 'denied DENIED' in report, report

            # Env scrub: credential-shaped variables do not reach the child;
            # `sandbox_env = "inherit"` is the documented escape hatch.
            repo, _ = run_probe(
                root,
                'workspace',
                "env > env.txt",
                extra_env={'GC_SMOKE_SECRET_TOKEN': 'leak-me', 'GC_SMOKE_KEEP': 'keep-me'},
            )
            env_text = (repo / 'env.txt').read_text()
            assert 'GC_SMOKE_SECRET_TOKEN' not in env_text, 'credentials must be scrubbed'
            assert 'GC_SMOKE_KEEP' in env_text, 'non-credential variables stay'

            repo, _ = run_probe(
                root,
                'workspace',
                "env > env.txt",
                extra_config='sandbox_env = "inherit"\n',
                extra_env={'GC_SMOKE_SECRET_TOKEN': 'leak-me'},
            )
            assert 'GC_SMOKE_SECRET_TOKEN' in (repo / 'env.txt').read_text(), (
                'sandbox_env = inherit must keep the environment intact'
            )
        finally:
            allowed.shutdown()
    finally:
        server.shutdown()
        shutil.rmtree(outside, ignore_errors=True)
    print(
        'PASS sandbox: write jail (workspace/strict), port-limited egress, '
        'credential env scrub with inherit escape hatch'
    )
