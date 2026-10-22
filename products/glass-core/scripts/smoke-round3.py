#!/usr/bin/env python3
"""Offline PTY checks for the round-3 commands: /name (direct and modal),
`glass-core list` titles, /skill-doctor accounting, and /code-review over a
dirty diff — including the clean-tree refusal, which must not spend a
provider call. Local mock endpoint only."""
import importlib.util
import http.server
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
    requests = []
    review_marker = 'Review this diff for correctness bugs'

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
        if type(self).review_marker in json.dumps(body):
            frames = [
                {'choices': [{'delta': {'content': 'FINDING other.rs:7 off-by-one'}}]}
            ]
        else:
            frames = [{'choices': [{'delta': {'content': 'Round-three reply.'}}]}]
        data = (
            ''.join('data: ' + json.dumps(x) + '\n\n' for x in frames)
            + 'data: [DONE]\n\n'
        ).encode()
        self.send_response(200)
        self.send_header('Content-Type', 'text/event-stream')
        self.send_header('Content-Length', str(len(data)))
        self.end_headers()
        try:
            self.wfile.write(data)
        except (BrokenPipeError, ConnectionResetError):
            pass


def key(t, s):
    os.write(t.master, s.encode())


def main():
    with tempfile.TemporaryDirectory(prefix='gc-round3-') as d:
        root = Path(d)
        repo = root / 'repo'
        repo.mkdir()
        subprocess.run(['git', 'init', '-q', str(repo)], check=True)
        subprocess.run(['git', '-C', str(repo), 'config', 'user.email', 't@t'], check=True)
        subprocess.run(['git', '-C', str(repo), 'config', 'user.name', 't'], check=True)
        (repo / 'mail.rs').write_text('fn send() {}\n')
        subprocess.run(['git', '-C', str(repo), 'add', '.'], check=True)
        subprocess.run(['git', '-C', str(repo), 'commit', '-qm', 'seed'], check=True)
        (repo / 'mail.rs').write_text('fn send() { /* dirty */ }\n')

        skills = root / 'skills'
        skills.mkdir()
        (skills / 'release.md').write_text(
            '---\nname: release\ndescription: Cut a release.\n---\n' + 'step\n' * 200
        )
        (skills / 'alpha.md').write_text(
            '---\nname: alpha\ndescription: Alpha things.\n---\nbody\n'
        )

        cfg = root / 'config/glass-core'
        cfg.mkdir(parents=True)
        server = http.server.ThreadingHTTPServer(('127.0.0.1', 0), Endpoint)
        threading.Thread(target=server.serve_forever, daemon=True).start()
        (cfg / 'config.toml').write_text(
            f'provider="openai"\nmodel="mock"\napi_key_env=""\n'
            f'base_url="http://127.0.0.1:{server.server_port}/v1"\ntrust="always"\n'
            f'skills_dir="{skills}"\n'
        )
        env = dict(
            os.environ,
            XDG_CONFIG_HOME=str(root / 'config'),
            XDG_DATA_HOME=str(root / 'data'),
        )
        try:
            t = f.Terminal(repo, env)
            try:
                t.idle()
                # /name sets the title directly; `list` shows it.
                t.send('/name Round Three')
                t.wait('session title: Round Three', timeout=10)
                t.idle()
                out = subprocess.run(
                    [str(f.BINARY), 'list'],
                    cwd=repo, env=env, text=True, capture_output=True, timeout=15,
                )
                assert out.returncode == 0 and 'Round Three' in out.stdout, (
                    out.stdout, out.stderr,
                )

                # The modal form opens with the current title and saves
                # what you type.
                t.send('/name')
                t.visible('Name session', timeout=10)
                key(t, '\x15Modal Title\r')
                t.wait('session title: Modal Title', timeout=10)
                t.idle()

                # /skill-doctor accounts for every skill: body bytes that
                # cost only when read, catalogue bytes that always cost.
                t.send('/skill-doctor')
                t.visible('Skill doctor', timeout=10)
                t.visible('release', timeout=10)
                t.visible('alpha', timeout=10)
                t.visible('catalogue always costs', timeout=10)
                t.dismiss()
                t.idle()

                # /code-review reviews the dirty diff through a read-only
                # delegated agent and posts the findings.
                before = len(Endpoint.requests)
                t.send('/code-review')
                t.visible('Code review', timeout=30)
                t.visible('FINDING', timeout=10)
                t.visible('other.rs', timeout=10)
                assert any(
                    Endpoint.review_marker in json.dumps(r)
                    for r in Endpoint.requests[before:]
                ), 'the review prompt must reach the provider'
                t.dismiss()
                t.idle()

                # A clean tree is refused without a provider call.
                subprocess.run(
                    ['git', '-C', str(repo), 'checkout', '--', 'mail.rs'], check=True
                )
                before = len(Endpoint.requests)
                t.send('/code-review')
                t.visible('no uncommitted changes', timeout=15)
                t.dismiss()
                t.idle()
                assert len(Endpoint.requests) == before, (
                    'a clean tree must not spend a provider call'
                )

                # A normal turn still works after the commands.
                t.send('hello again')
                t.wait('Round-three reply.', timeout=15)
                t.idle()
                assert not t.screen.invalid_positions, t.screen.invalid_positions
            finally:
                t.close()
        finally:
            server.shutdown()
    print(
        'PASS round 3: /name direct and modal, list titles, /skill-doctor '
        'accounting, /code-review read-only findings, clean-tree refusal, '
        'normal turn after'
    )


if __name__ == '__main__':
    main()
