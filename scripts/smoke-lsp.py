#!/usr/bin/env python3
"""Offline protocol check for the lsp extension: a write's path is checked
against a real (fixture) language server, diagnostics land in the tool
result, and a missing config keeps the extension silent. No binary needed."""
import json
import os
import subprocess
import sys
import tempfile
from pathlib import Path

REPO = Path(__file__).resolve().parents[1]
EXTENSION = REPO / 'extensions' / 'lsp' / 'lsp.py'
FIXTURE = REPO / 'scripts' / 'lsp_fixture_server.py'


class Ext:
    def __init__(self, env):
        self.proc = subprocess.Popen(
            [sys.executable, str(EXTENSION)],
            stdin=subprocess.PIPE, stdout=subprocess.PIPE, text=True, env=env,
        )
        self.id = 0

    def send(self, obj):
        self.proc.stdin.write(json.dumps(obj) + '\n')
        self.proc.stdin.flush()

    def recv(self):
        line = self.proc.stdout.readline()
        assert line, 'extension closed the stream'
        return json.loads(line)

    def hello(self, workspace):
        self.id += 1
        self.send({'type': 'hello', 'id': self.id, 'workspace': workspace})
        return self.recv()

    def tool_result(self, body, timeout=30):
        self.id += 1
        self.send({'type': 'tool_result', 'id': self.id, 'call_id': 'c1', 'name': 'bash', 'body': body})
        return self.recv()

    def tool(self, **args):
        self.id += 1
        self.send({'type': 'run_tool', 'id': self.id, 'name': 'lsp', 'arguments': json.dumps(args)})
        return self.recv()['output']

    def command(self, name):
        self.id += 1
        self.send({'type': 'command', 'id': self.id, 'name': name, 'args': ''})
        return self.recv()['output']

    def close(self):
        self.id += 1
        self.send({'type': 'goodbye', 'id': self.id})
        self.proc.stdin.close()
        self.proc.wait(timeout=15)


def main():
    selftest = subprocess.run(
        [sys.executable, str(EXTENSION), '--selftest'],
        check=True,
        timeout=30,
        capture_output=True,
        text=True,
    )
    assert 'PASS lsp extension selftest' in selftest.stdout, selftest.stdout

    with tempfile.TemporaryDirectory(prefix='gc-lsp-') as tmp:
        ws = Path(tmp) / 'ws'
        ws.mkdir()
        (ws / 'a.py').write_text('printed = value\n')

        config = Path(tmp) / 'lsp.json'
        config.write_text(json.dumps({
            '.py': {'command': [sys.executable, str(FIXTURE)], 'timeout_s': 10, 'debounce_s': 0.2}
        }))
        env = dict(os.environ, GC_LSP_CONFIG=str(config))
        ext = Ext(env)
        try:
            manifest = ext.hello(str(ws))
            assert {t['name'] for t in manifest['tools']} == {'lsp'}
            reply = ext.tool_result('[exit 0]\n[wrote a.py — 1 lines]\ndone\n')
            body = reply['body']
            assert '[lsp] a.py:1:1 error: FIXTURE-ERROR undefined thing' in body, body
            assert body.startswith('[exit 0]'), 'the original body is preserved'

            # Unrelated tool results are untouched.
            reply = ext.tool_result('[exit 0]\nhello\n')
            assert reply['body'] == '[exit 0]\nhello\n', reply

            # The explicit tool and the status command work too.
            out = ext.tool(path='a.py')
            assert 'FIXTURE-ERROR' in out and 'a.py:1:1' in out, out
            out = ext.command('lsp')
            assert 'configured .py' in out and 'running .py' in out, out

            (ws / 'a.py').write_text('a😀b = 1\n')
            for operation in ('definition', 'references', 'hover', 'symbols', 'rename', 'code_actions'):
                result = ext.tool(path='a.py', operation=operation, line=1, column=3, new_name='renamed')
                data = json.loads(result.split('\n', 1)[1])
                assert data['method'].startswith('textDocument/'), result
                if operation != 'symbols':
                    position = data['params'].get('position', data['params'].get('range', {}).get('start'))
                    assert position == {'line': 0, 'character': 3}, result
                if operation in ('rename', 'code_actions'):
                    assert result.startswith('PREVIEW ONLY'), result
                assert (ws / 'a.py').read_text() == 'a😀b = 1\n'
            assert 'positive integers' in ext.tool(path='a.py', operation='hover', line=True, column=1)
            assert 'outside the file' in ext.tool(path='a.py', operation='hover', line=1, column=99)
            (ws / 'large.py').write_bytes(b'x' * (1024 * 1024 + 1))
            assert 'could not open file' in ext.tool(path='large.py', operation='hover', line=1, column=1)

            # A path that escapes the workspace is refused.
            out = ext.tool(path='../a.py')
            assert 'escapes the workspace' in out, out
        finally:
            ext.close()

        # No config: the extension must be silent — bodies pass through.
        silent = Ext(dict(os.environ, GC_LSP_CONFIG=str(Path(tmp) / 'missing.json')))
        try:
            silent.hello(str(ws))
            reply = silent.tool_result('[exit 0]\n[wrote a.py]\n')
            assert reply['body'] == '[exit 0]\n[wrote a.py]\n', reply
        finally:
            silent.close()
    print(
        'PASS lsp: diagnostics appended after a written file, body preserved, '
        'explicit tool and status work, no-config stays silent'
    )


if __name__ == '__main__':
    main()
