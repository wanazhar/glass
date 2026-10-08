#!/usr/bin/env python3
"""Offline protocol check for the index extension: exact line numbers, query
search, unknown types, path containment, and the per-file cap."""
import json
import os
import subprocess
import sys
import tempfile
from pathlib import Path

REPO = Path(__file__).resolve().parents[1]
EXTENSION = REPO / 'extensions' / 'index' / 'index.py'


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

    def tool(self, **args):
        self.id += 1
        self.send({'type': 'run_tool', 'id': self.id, 'name': 'index', 'arguments': json.dumps(args)})
        return self.recv()['output']

    def close(self):
        self.id += 1
        self.send({'type': 'goodbye', 'id': self.id})
        self.proc.stdin.close()
        self.proc.wait(timeout=10)


def main():
    with tempfile.TemporaryDirectory(prefix='gc-index-') as tmp:
        ws = Path(tmp)
        (ws / 'sample.py').write_text(
            'import os\n\n\n'
            'def first(x):\n    return x\n\n\n'
            'class Second:\n    def method(self):\n        pass\n'
        )
        (ws / 'sample.rs').write_text(
            'use std::io;\n\n'
            'pub struct Agent {\n    name: String,\n}\n\n'
            'impl Agent {\n    pub fn new() -> Self {}\n}\n\n'
            'fn alone() {}\n'
        )
        (ws / 'notes.txt').write_text('needle here\n')
        (ws / 'many.py').write_text('\n'.join(f'def f{i}(): pass' for i in range(100)))

        ext = Ext(dict(os.environ))
        try:
            manifest = ext.hello(str(ws))
            assert {t['name'] for t in manifest['tools']} == {'index'}
            assert 'index' in manifest['system_prompt']

            out = ext.tool(path='sample.py')
            assert '4: def first' in out and '8: class Second' in out and '9: def method' in out, out
            out = ext.tool(path='sample.rs')
            assert '3: struct Agent' in out and '7: impl Agent' in out and '8: fn new' in out, out

            # Query finds files, including by a literal in a text file.
            out = ext.tool(query='def first')
            assert 'sample.py' in out and '4: def first' in out, out
            out = ext.tool(query='needle')
            assert 'notes.txt' in out, out

            # Non-source files say so instead of guessing.
            out = ext.tool(path='notes.txt')
            assert 'no outline' in out, out
            # Containment: index cannot be used to read outside the workspace.
            out = ext.tool(path='../sample.py')
            assert 'escapes the workspace' in out, out
            # Caps: a 100-def file reports at most the per-file cap.
            out = ext.tool(path='many.py')
            assert 'elided' in out and len(out.splitlines()) <= 62, len(out.splitlines())
        finally:
            ext.close()
    print(
        'PASS index: exact line numbers, query search, containment, unknown '
        'types explained, per-file cap enforced'
    )


if __name__ == '__main__':
    main()
