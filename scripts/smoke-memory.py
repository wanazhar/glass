#!/usr/bin/env python3
"""Offline protocol check for the memory extension: index injection, add /
read / list / remove, caps, and name validation. No binary needed."""
import json
import os
import subprocess
import sys
import tempfile
from pathlib import Path

REPO = Path(__file__).resolve().parents[1]
EXTENSION = REPO / 'extensions' / 'memory' / 'memory.py'


class Ext:
    def __init__(self, env):
        self.proc = subprocess.Popen(
            [sys.executable, str(EXTENSION)],
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            text=True,
            env=env,
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
        self.send({'type': 'run_tool', 'id': self.id, 'name': 'memory', 'arguments': json.dumps(args)})
        return self.recv()['output']

    def close(self):
        self.id += 1
        self.send({'type': 'goodbye', 'id': self.id})
        self.proc.stdin.close()
        self.proc.wait(timeout=10)


def main():
    with tempfile.TemporaryDirectory(prefix='gc-memory-') as tmp:
        root = Path(tmp)
        workspace = root / 'ws'
        workspace.mkdir()
        memory_dir = root / 'mem'
        env = dict(os.environ, GC_MEMORY_DIR=str(memory_dir))
        ext = Ext(env)
        try:
            manifest = ext.hello(str(workspace))
            assert manifest['protocol'] == 1, manifest
            names = {t['name'] for t in manifest['tools']}
            assert names == {'memory'}, names
            assert 'Project memory lives in' in manifest['system_prompt'], manifest

            out = ext.tool(action='add', name='build', text='Run cargo test --locked.\nDetails.')
            assert out.startswith('memory saved'), out
            slug_dir = next(memory_dir.iterdir())
            assert (slug_dir / 'build.md').is_file(), slug_dir

            listing = ext.tool(action='list')
            assert 'build.md' in listing and 'cargo test --locked' in listing, listing
            body = ext.tool(action='read', name='build')
            assert 'Run cargo test --locked.' in body and '---' not in body, body
            assert 'lowercase' in ext.tool(action='add', name='../evil', text='x')
            assert 'no memory named' in ext.tool(action='read', name='ghost')
            assert ext.tool(action='remove', name='build').startswith('memory removed')
            assert not (slug_dir / 'build.md').exists()

            # Caps: an oversized index is truncated with a visible marker.
            (slug_dir / 'MEMORY.md').write_text(
                '\n'.join(f'- [m{i}](m{i}.md) — hook {i}' for i in range(300)) + '\n'
            )
            capped = ext.tool(action='list')
            assert 'truncated' in capped, capped
            assert len(capped.splitlines()) <= 202, len(capped.splitlines())
        finally:
            ext.close()
    print(
        'PASS memory extension: index injected, add/read/list/remove round-trip, '
        'caps and name validation enforced'
    )


if __name__ == '__main__':
    main()
