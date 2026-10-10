#!/usr/bin/env python3
"""Offline protocol check for the lsp extension: a write's path is checked
against a real (fixture) language server, diagnostics land in the tool result
as deltas, every written file is covered, code actions are refused once a file
moved, and a missing config keeps the extension silent. No binary needed."""
import json
import os
import re
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

    def children(self):
        """PIDs this process has spawned, or [] where /proc is unavailable."""
        found = []
        try:
            for task in Path(f'/proc/{self.proc.pid}/task').iterdir():
                try:
                    found.extend(int(pid) for pid in (task / 'children').read_text().split())
                except OSError:
                    pass
        except OSError:
            pass
        return found

    def close(self):
        self.id += 1
        self.send({'type': 'goodbye', 'id': self.id})
        self.proc.stdin.close()
        self.proc.wait(timeout=15)


def token_of(preview):
    match = re.search(r'\b(rename|code_actions):\d+:', preview)
    assert match, preview
    return match.group(0)[:-1]


def main():
    selftest = subprocess.run(
        [sys.executable, str(EXTENSION), '--selftest'],
        check=True,
        timeout=120,
        capture_output=True,
        text=True,
    )
    assert 'PASS lsp extension selftest' in selftest.stdout, selftest.stdout

    with tempfile.TemporaryDirectory(prefix='gc-lsp-') as tmp:
        ws = Path(tmp) / 'ws'
        ws.mkdir()
        (ws / 'a.py').write_text('printed = value\n')
        (ws / 'b.py').write_text('FIXTURE-WARN\nsecond\n')
        for index in range(8):
            (ws / f'm{index}.py').write_text(f'value{index} = {index}\n')

        config = Path(tmp) / 'lsp.json'
        config.write_text(json.dumps({
            '.py': {'command': [sys.executable, str(FIXTURE)], 'timeout_s': 10, 'debounce_s': 0.2}
        }))
        tripwire = Path(tmp) / 'tripwire.txt'
        env = dict(os.environ, GC_LSP_CONFIG=str(config), GC_LSP_FIXTURE_TRIPWIRE=str(tripwire))
        ext = Ext(env)
        try:
            manifest = ext.hello(str(ws))
            assert {t['name'] for t in manifest['tools']} == {'lsp'}
            assert 'apply' in manifest['tools'][0]['schema']['properties']['operation']['enum'], manifest
            # Enabling the extension starts nothing: a server appears only when
            # a file with a configured language is checked.
            assert ext.children() == [], ext.children()
            reply = ext.tool_result('[exit 0]\nhello\n')
            assert reply['body'] == '[exit 0]\nhello\n', reply
            assert ext.children() == [], ext.children()

            # First report: the whole set, once.
            reply = ext.tool_result('[exit 0]\n[wrote a.py — 1 lines]\ndone\n')
            body = reply['body']
            assert '[lsp] a.py: 1 problem (first report)' in body, body
            assert '[lsp] a.py:1:1 error: FIXTURE-ERROR undefined thing' in body, body
            assert body.startswith('[exit 0]'), 'the original body is preserved'
            assert ext.children(), 'a configured language starts its server on demand'

            # An unchanged file is one line, not the list again.
            out = ext.tool(path='a.py')
            assert 'checking a.py' in out and 'a.py: no change, 1 problem still open' in out, out

            # A new problem is a delta; the current set still follows it.
            (ws / 'a.py').write_text('FIXTURE-WARN\nprinted = value\n')
            body = ext.tool_result('[wrote a.py — 2 lines]\n')['body']
            assert '[lsp] a.py: +1 new (2 open)' in body, body
            assert '[lsp] + a.py:2:1 warning: FIXTURE-WARN unused variable' in body, body
            assert body.index('+ a.py:2:1') < body.index('a.py — current set:'), body
            assert body.rstrip().endswith('[lsp] a.py:2:1 warning: FIXTURE-WARN unused variable'), body

            # A problem that disappears is reported as resolved.
            (ws / 'a.py').write_text('FIXTURE-WARN FIXTURE-OK\nprinted = value\n')
            body = ext.tool_result('[wrote a.py — 3 lines]\n')['body']
            assert '[lsp] a.py: -1 resolved (1 open)' in body, body
            assert '[lsp] - a.py:1:1 error: FIXTURE-ERROR undefined thing' in body, body

            # Every file the edit wrote is checked, not just the newest.
            body = ext.tool_result('[wrote a.py — 1 lines]\n[wrote b.py — 2 lines]\n')['body']
            assert '[lsp] b.py:' in body and '[lsp] a.py:' in body, body
            assert body.index('[lsp] b.py:') < body.index('[lsp] a.py:'), body

            # The bound on files is stated rather than silent.
            many = '\n'.join(f'[wrote m{index}.py — 1 lines]' for index in range(8))
            body = ext.tool_result(many + '\n')['body']
            assert '[lsp] 8 written files, checked the 5 newest; 3 not checked' in body, body
            assert len([line for line in body.splitlines() if line.endswith('(first report)')]) == 5, body
            assert 'm0.py' in body.split('not checked')[1], body

            # Rename previews carry a token and the revision they were computed
            # against; nothing is written until `apply`.
            preview = ext.tool(path='a.py', operation='rename', line=1, column=1, new_name='renamed')
            assert preview.startswith('PREVIEW ONLY'), preview
            assert 'workspace/applyEdit' in preview and 'never sends' in preview, preview
            token = token_of(preview)
            assert f'{token}:' in preview and 'expected=' in preview, preview
            assert f'operation="apply" token="{token}"' in preview, preview
            before = (ws / 'a.py').read_text()
            assert before == 'FIXTURE-WARN FIXTURE-OK\nprinted = value\n', before

            applied = ext.tool(operation='apply', token=token)
            assert applied.startswith(f'applied {token}'), applied
            assert (ws / 'a.py').read_text() == 'renamed=' + before, (ws / 'a.py').read_text()
            # A token is spent by the apply, refused or not.
            assert 'unknown' in ext.tool(operation='apply', token=token)

            # A file that moved refuses the action, and writes nothing.
            preview = ext.tool(path='a.py', operation='rename', line=1, column=1, new_name='moved')
            stale = (ws / 'a.py').read_text()
            (ws / 'a.py').write_text(stale + '# someone else edited this\n')
            refusal = ext.tool(operation='apply', token=token_of(preview))
            assert refusal.startswith('REFUSED') and 'a.py' in refusal, refusal
            assert 'changed since the action was computed' in refusal and 'reread' in refusal, refusal
            assert (ws / 'a.py').read_text() == stale + '# someone else edited this\n', 'a stale action must not write'

            # Code actions: the edit-bearing one is applicable, the server
            # command is never run.
            preview = ext.tool(path='a.py', operation='code_actions', line=1, column=1)
            assert 'FIXTURE-RENAME' in preview and 'FIXTURE-SERVER-COMMAND' in preview, preview
            assert 'never run here' in preview, preview
            assert (ws / 'a.py').read_text() == stale + '# someone else edited this\n', 'preview must not write'
            applied = ext.tool(operation='apply', token=token_of(preview))
            assert applied.startswith('applied code_actions:'), applied
            assert (ws / 'a.py').read_text().startswith('FIXTURE='), (ws / 'a.py').read_text()

            # Diagnostics follow the applied edit.
            body = ext.tool_result('[wrote a.py — 1 lines]\n')['body']
            assert '[lsp] a.py:' in body, body

            out = ext.command('lsp')
            assert 'configured .py' in out and 'running .py' in out, out

            (ws / 'a.py').write_text('a😀b = 1\n')
            for operation in ('definition', 'references', 'hover', 'symbols'):
                result = ext.tool(path='a.py', operation=operation, line=1, column=3)
                data = json.loads(result.split('\n', 1)[1])
                assert data['method'].startswith('textDocument/'), result
                position = data['params'].get('position', data['params'].get('range', {}).get('start'))
                if operation != 'symbols':
                    assert position == {'line': 0, 'character': 3}, result
                assert (ws / 'a.py').read_text() == 'a😀b = 1\n'
            assert 'positive integers' in ext.tool(path='a.py', operation='hover', line=True, column=1)
            assert 'outside the file' in ext.tool(path='a.py', operation='hover', line=1, column=99)
            (ws / 'large.py').write_bytes(b'x' * (1024 * 1024 + 1))
            assert 'could not open file' in ext.tool(path='large.py', operation='hover', line=1, column=1)

            # A path that escapes the workspace is refused.
            out = ext.tool(path='../a.py')
            assert 'escapes the workspace' in out, out
            assert 'token' in ext.tool(operation='apply')
            assert not tripwire.exists(), tripwire.read_text()
        finally:
            ext.close()

        # No config: the extension must be silent — bodies pass through.
        silent = Ext(dict(os.environ, GC_LSP_CONFIG=str(Path(tmp) / 'missing.json')))
        try:
            silent.hello(str(ws))
            reply = silent.tool_result('[exit 0]\n[wrote a.py]\n')
            assert reply['body'] == '[exit 0]\n[wrote a.py]\n', reply
            assert 'apply needs the token' in silent.tool(path='a.py', operation='apply')
            assert 'unknown or already-used' in silent.tool(operation='apply', token='rename:1')
        finally:
            silent.close()
    print(
        'PASS lsp: diagnostic deltas after a write with the body preserved, every written '
        'file checked with the bound stated, code actions applied only while every '
        'revision holds, no-config stays silent'
    )


if __name__ == '__main__':
    main()
