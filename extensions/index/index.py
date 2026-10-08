#!/usr/bin/env python3
"""glass index extension: a file's skeleton, before the file itself.

Reads dominate a bash-first harness's token spend, and most reads only need
the shape of a file: which symbols exist and where. `index` answers that
with line-anchored regex outlines and exact line numbers, so the model can
then read a range instead of the whole file. Maki publishes the accounting
for this pattern (+59 tok/turn for the tool, −224 tok/turn on reads); this
is a stdlib-only version with no tree-sitter dependency.

Tool:
    index {path: "src/agent.rs"}          outline one file
    index {query: "retry_backoff"}        files containing it + outlines

Deliberately not a full parser: a regex that never lies about line numbers
beats a parser that occasionally does, and the model can always read the
file.
"""

import json
import os
import re
import shutil
import subprocess
import sys

PROTOCOL = 1
FILE_CAP = 60       # outline lines per file
TOTAL_CAP = 200     # outline lines per call
QUERY_FILES = 5     # files reported for a query
SCAN_MAX_BYTES = 262_144

SKIP_DIRS = {'.git', 'target', 'node_modules', '__pycache__', 'dist', 'build', '.venv', 'venv'}

PATTERNS = {
    '.rs': [
        (re.compile(r'^\s*(?:pub(?:\([^)]*\))?\s+)?(?:async\s+)?(?:unsafe\s+)?fn\s+([A-Za-z_]\w*)'), 'fn'),
        (re.compile(r'^\s*(?:pub(?:\([^)]*\))?\s+)?(struct|enum|trait|mod)\s+([A-Za-z_]\w*)'), None),
        (re.compile(r'^\s*impl(?:<[^>]*>)?\s+([A-Za-z_][\w:<> ,]*?)\s*(?:\{|for\s+([A-Za-z_][\w:<> ,]*))'), 'impl'),
        (re.compile(r'^\s*(?:pub(?:\([^)]*\))?\s+)?(?:const|static)\s+(?:mut\s+)?([A-Za-z_]\w*)'), 'const'),
    ],
    '.py': [
        (re.compile(r'^\s*(?:async\s+)?def\s+([A-Za-z_]\w*)'), 'def'),
        (re.compile(r'^\s*class\s+([A-Za-z_]\w*)'), 'class'),
    ],
    '.go': [
        (re.compile(r'^\s*func\s+(?:\([^)]*\)\s+)?([A-Za-z_]\w*)'), 'func'),
        (re.compile(r'^\s*type\s+([A-Za-z_]\w*)'), 'type'),
    ],
    '.js': [
        (re.compile(r'^\s*(?:export\s+)?(?:default\s+)?(?:async\s+)?function\s+([A-Za-z_$][\w$]*)'), 'function'),
        (re.compile(r'^\s*(?:export\s+)?class\s+([A-Za-z_$][\w$]*)'), 'class'),
        (re.compile(r'^\s*(?:export\s+)?(?:const|let|var)\s+([A-Za-z_$][\w$]*)\s*='), 'const'),
    ],
}
PATTERNS['.ts'] = PATTERNS['.js'] + [
    (re.compile(r'^\s*(?:export\s+)?(?:interface|type|enum)\s+([A-Za-z_$][\w$]*)'), 'type'),
]
PATTERNS['.tsx'] = PATTERNS['.ts']
PATTERNS['.jsx'] = PATTERNS['.js']
PATTERNS['.mjs'] = PATTERNS['.js']
PATTERNS['.cjs'] = PATTERNS['.js']


def outline(text, path):
    """Line-numbered symbols for one file, or an explanation of why not."""
    ext = os.path.splitext(path)[1].lower()
    patterns = PATTERNS.get(ext)
    if not patterns:
        return None
    rows = []
    for number, line in enumerate(text.splitlines(), start=1):
        stripped = line.lstrip()
        if stripped.startswith(('#', '//', '*')):
            continue
        for pattern, kind in patterns:
            match = pattern.match(line)
            if not match:
                continue
            groups = [g for g in match.groups() if g]
            if kind is None:
                kind = match.group(1)
                name = match.group(2)
            elif kind == 'impl':
                # Prefer the trait target when there is one; the type itself
                # otherwise. `impl fmt::Display for Agent` reads as Agent.
                name = groups[-1]
            else:
                name = groups[0] if groups else '?'
            name = name.strip().rstrip('{').strip()
            rows.append(f'{number}: {kind} {name}')
            break
        if len(rows) >= FILE_CAP:
            rows.append(f'… more symbols elided ({FILE_CAP} cap)')
            break
    return rows


def read_text(path):
    try:
        if os.path.getsize(path) > SCAN_MAX_BYTES:
            return None
        with open(path, encoding='utf-8', errors='replace') as fh:
            return fh.read()
    except OSError:
        return None


def find_files(workspace, query):
    """Files containing the literal, rg when available."""
    rg = shutil.which('rg')
    if rg:
        try:
            done = subprocess.run(
                [rg, '-l', '-F', '--max-count', '1', '--', query, '.'],
                cwd=workspace, capture_output=True, text=True, timeout=20,
            )
            if done.returncode == 0:
                return [
                    os.path.normpath(line)
                    for line in done.stdout.splitlines()[:QUERY_FILES]
                ]
        except (OSError, subprocess.TimeoutExpired):
            pass
    found = []
    for root, dirs, files in os.walk(workspace):
        dirs[:] = sorted(d for d in dirs if d not in SKIP_DIRS)
        for name in sorted(files):
            full = os.path.join(root, name)
            try:
                if os.path.getsize(full) > SCAN_MAX_BYTES:
                    continue
            except OSError:
                continue
            text = read_text(full)
            if text and query in text:
                found.append(os.path.relpath(full, workspace))
                if len(found) >= QUERY_FILES:
                    return found
    return found


def run(workspace, args):
    path = (args.get('path') or '').strip()
    query = (args.get('query') or '').strip()
    if not path and not query:
        return 'index: give `path` for one file or `query` to find files'
    lines = []
    if path:
        full = os.path.realpath(os.path.join(workspace, path))
        if not full.startswith(os.path.realpath(workspace) + os.sep):
            return 'index: path escapes the workspace'
        text = read_text(full)
        if text is None:
            return f'index: cannot read {path}'
        rows = outline(text, path)
        if rows is None:
            ext = os.path.splitext(path)[1] or '(none)'
            return f'index: no outline for {ext} files; read it instead'
        lines.append(f'{path} ({len(text.splitlines())} lines)')
        lines.extend(f'  {row}' for row in rows)
    if query:
        matches = find_files(workspace, query)
        if not matches:
            return f'index: no files contain {query!r}'
        for rel in matches:
            text = read_text(os.path.join(workspace, rel))
            if text is None:
                continue
            rows = outline(text, rel)
            lines.append(f'{rel} ({len(text.splitlines())} lines)')
            if rows:
                lines.extend(f'  {row}' for row in rows)
            else:
                lines.append('  (no outline for this type; read it)')
    if len(lines) > TOTAL_CAP:
        lines = lines[:TOTAL_CAP] + ['… truncated (200-line cap)']
    return '\n'.join(lines)


MANIFEST = {
    'name': 'index',
    'version': '1.0.0',
    'system_prompt': (
        'Before reading a file whole, call `index` to see its structure '
        '(one line per symbol, with exact line numbers), then read only the '
        'ranges you need. `index` with a `query` finds the files containing '
        'a literal and shows their outlines.'
    ),
    'tools': [
        {
            'name': 'index',
            'description': (
                'Outline a source file (symbols with exact line numbers) or '
                'find files containing a literal and outline them. Use before '
                'read to avoid paying for whole files.'
            ),
            'schema': {
                'type': 'object',
                'properties': {
                    'path': {'type': 'string', 'description': 'File to outline, relative to the workspace.'},
                    'query': {'type': 'string', 'description': 'Literal text to find files by.'},
                },
            },
            'concurrency': 'parallel',
        }
    ],
}


def send(obj):
    sys.stdout.write(json.dumps(obj) + '\n')
    sys.stdout.flush()


def read_line():
    line = sys.stdin.readline()
    if not line:
        return None
    line = line.strip()
    return json.loads(line) if line else None


def main():
    workspace = os.getcwd()
    while True:
        req = read_line()
        if req is None:
            break
        kind = req.get('type')
        if kind == 'hello':
            workspace = req.get('workspace') or workspace
            reply = {'id': req.get('id'), 'protocol': PROTOCOL}
            reply.update(MANIFEST)
            send(reply)
        elif kind == 'system_prompt':
            send({'id': req.get('id'), 'text': MANIFEST['system_prompt']})
        elif kind == 'authorize':
            send({'id': req.get('id'), 'allow': True})
        elif kind == 'run_tool':
            try:
                args = json.loads(req.get('arguments') or '{}')
            except ValueError:
                output = 'index: arguments are not valid JSON'
            else:
                output = run(workspace, args if isinstance(args, dict) else {})
            send({'id': req.get('id'), 'output': output})
        elif kind in ('goodbye', 'shutdown'):
            break


def selftest():
    sample = (
        'import os\n\n\n'
        'def helper(x):\n    return x\n\n\n'
        'class Thing:\n    def method(self):\n        pass\n'
    )
    rows = outline(sample, 'a.py')
    assert rows == ['4: def helper', '8: class Thing', '9: def method'], rows

    rust = (
        'use std::io;\n\n'
        'pub struct Agent {\n}\n\n'
        'impl Agent {\n    pub fn new() -> Self {}\n}\n\n'
        'fn helper() {}\n'
    )
    rows = outline(rust, 'a.rs')
    assert rows[0] == '3: struct Agent', rows
    assert rows[1] == '6: impl Agent', rows
    assert rows[2] == '7: fn new', rows
    assert rows[3] == '10: fn helper', rows

    assert outline('x = 1', 'a.txt') is None
    assert outline('// def not_real\n', 'a.py') == [], 'comments are not symbols'
    big = '\n'.join(f'def f{i}(): pass' for i in range(100))
    capped = outline(big, 'a.py')
    assert len(capped) == FILE_CAP + 1 and 'elided' in capped[-1], len(capped)
    print('PASS index extension selftest')


if __name__ == '__main__':
    if '--selftest' in sys.argv:
        selftest()
    else:
        main()
