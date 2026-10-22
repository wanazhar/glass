#!/usr/bin/env python3
"""Offline checks for /init and `gc init`: draft, review, save, overwrite, and
the headless --write/-json contract. Uses a local mock endpoint only."""
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

DRAFT = (
    '# AGENTS.md draft\n\nRun `cargo test` before committing.\n\n'
    '## Layout\n\n- `crates/` the workspace members.'
)


class Endpoint(http.server.BaseHTTPRequestHandler):
    mode = 'plain'

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
        text = DRAFT if type(self).mode == 'plain' else '```markdown\n' + DRAFT + '\n```'
        frames = [
            {'choices': [{'delta': {'content': text}}]},
            {'choices': [], 'usage': {'prompt_tokens': 12, 'completion_tokens': 8}},
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


def back(t):
    t.received = ''
    os.write(t.master, b'\x1b')
    t.wait('\x1b[?25h')


def cli(args, cwd, env):
    p = subprocess.run(
        [str(f.BINARY), *args], cwd=cwd, env=env, text=True, capture_output=True, timeout=120
    )
    assert '\x1b' not in p.stdout and '\x1b' not in p.stderr, p
    assert p.returncode == 0, p
    return p


def main():
    with tempfile.TemporaryDirectory(prefix='gc-init-') as tmp:
        root = Path(tmp)
        repo = root / 'repo'
        repo.mkdir()
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
            # Interactive: a fenced draft is unwrapped before it is saved.
            Endpoint.mode = 'fenced'
            t = f.Terminal(repo, env)
            try:
                t.idle()
                t.send('/init keep it short')
                t.visible('AGENTS.md draft', timeout=15)
                choose(t, 'Read the draft')
                t.visible('Run `cargo test`')
                # Esc returns from the info view to the choose modal.
                back(t)
                t.visible('Save AGENTS.md')
                choose(t, 'Save AGENTS.md')
                t.wait('wrote')
                saved = (repo / 'AGENTS.md').read_text()
                assert saved == DRAFT + '\n', saved
                assert '```' not in saved, 'the outer fence must not reach the file'
                # A second run refuses to overwrite without an explicit confirm.
                t.send('/init')
                t.visible('AGENTS.md draft', timeout=15)
                choose(t, 'Overwrite AGENTS.md')
                t.visible('Overwrite existing AGENTS.md?')
                choose(t, 'Cancel')
                t.visible('AGENTS.md draft')
                assert (repo / 'AGENTS.md').read_text() == DRAFT + '\n', 'cancel must change nothing'
                choose(t, 'Overwrite AGENTS.md')
                t.visible('Overwrite existing AGENTS.md?')
                choose(t, 'Overwrite')
                t.wait('wrote')
                assert (repo / 'AGENTS.md').read_text() == DRAFT + '\n'
                t.idle()
            finally:
                t.close()

            # Headless: --json reports without writing; --write creates the file.
            (repo / 'AGENTS.md').unlink()
            Endpoint.mode = 'plain'
            p = cli(['init', '--json'], repo, env)
            r = json.loads(p.stdout)
            assert r['status'] == 'ok' and r['written'] is False, r
            assert r['text'] == DRAFT, r
            assert not (repo / 'AGENTS.md').exists(), 'init without --write must not create the file'
            p = cli(['init', '--write', '--json'], repo, env)
            r = json.loads(p.stdout)
            assert r['written'] is True and r['path'].endswith('AGENTS.md'), r
            assert (repo / 'AGENTS.md').read_text() == DRAFT + '\n'
        finally:
            server.shutdown()
    print('PASS: /init drafts in a read-only run, strips an outer fence, saves only '
          'after confirmation (overwrite confirmed), and gc init reports/--writes headlessly')


if __name__ == '__main__':
    main()
