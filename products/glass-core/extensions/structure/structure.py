#!/usr/bin/env python3
"""Optional Python AST transformations, preserving source spelling and comments.
Only selected Name nodes or a uniquely located expression change. The shared
native transaction validates syntax and revisions before replacement.
"""
import argparse
import ast
import hashlib
import json
import os
import stat
import subprocess
import sys


def source(path):
    if os.path.isabs(path) or '..' in path.split('/'):
        raise ValueError('Choose a workspace-relative file without ..')
    parts = [p for p in path.split('/') if p not in ('', '.')]
    if not parts: raise ValueError('File path required')
    descriptor = os.open('.', os.O_RDONLY | os.O_DIRECTORY)
    try:
        for part in parts[:-1]:
            child = os.open(part, os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW, dir_fd=descriptor)
            os.close(descriptor); descriptor = child
        file = os.open(parts[-1], os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK, dir_fd=descriptor)
        try:
            meta = os.fstat(file)
            if not stat.S_ISREG(meta.st_mode) or meta.st_size > 512*1024:
                raise ValueError('Select a UTF-8 regular file up to 512 KiB')
            chunks = []; count = 0
            while True:
                chunk = os.read(file, 65536)
                if not chunk: break
                chunks.append(chunk); count += len(chunk)
                if count > 512*1024: raise ValueError('File exceeds 512 KiB')
            data = b''.join(chunks)
            after = os.fstat(file)
            if (meta.st_size, meta.st_mtime_ns, meta.st_ino) != (after.st_size, after.st_mtime_ns, after.st_ino):
                raise ValueError('File changed while reading; reread revision')
            return data
        finally: os.close(file)
    finally: os.close(descriptor)


def transform(data, old, new, line=None, column=None, expression=False):
    if len(data) > 512*1024: raise ValueError('AST input exceeds 512 KiB')
    text = data.decode('utf-8'); tree = ast.parse(text)
    offsets = [0]
    for row in data.splitlines(keepends=True): offsets.append(offsets[-1]+len(row))
    selected = []
    if expression: ast.parse(new, mode='eval')
    elif not new.isidentifier() or not old.isidentifier(): raise ValueError('Identifier names required')
    for node in ast.walk(tree):
        if expression:
            if not isinstance(node, ast.expr): continue
        elif not isinstance(node, ast.Name) or node.id != old: continue
        if line is not None and node.lineno != line: continue
        if column is not None and node.col_offset != column-1: continue
        start = offsets[node.lineno-1]+node.col_offset
        end = offsets[node.end_lineno-1]+node.end_col_offset
        if expression and data[start:end].decode() != old: continue
        selected.append((start, end))
    if not selected: raise ValueError('No matching AST node; source preserved')
    if expression and len(selected) != 1: raise ValueError('Expression is ambiguous; supply --line and --column (UTF-8 byte column)')
    result = data
    for start, end in sorted(selected, reverse=True): result = result[:start]+new.encode()+result[end:]
    ast.parse(result.decode())
    return result.decode(), len(selected)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('action', choices=['preview', 'apply'])
    parser.add_argument('path'); parser.add_argument('--expected', required=True)
    parser.add_argument('--old', required=True); parser.add_argument('--new', required=True)
    parser.add_argument('--line', type=int); parser.add_argument('--column', type=int)
    parser.add_argument('--expression', action='store_true'); parser.add_argument('--json', action='store_true')
    args = parser.parse_args()
    data = source(args.path)
    if hashlib.sha256(data).hexdigest() != args.expected: raise ValueError('Stale AST revision; reread the file before planning')
    text, _ = transform(data, args.old, args.new, args.line, args.column, args.expression)
    plan = {'files': [{'path': args.path, 'expected': args.expected, 'text': text, 'validate': True}]}
    cmd = [os.environ.get('GC_BIN', 'glass-core'), 'transaction', args.action]
    if args.json: cmd.append('--json')
    raise SystemExit(subprocess.run(cmd, input=json.dumps(plan).encode()).returncode)

if __name__ == '__main__':
    try: main()
    except (ValueError, OSError, SyntaxError) as error:
        print('error: '+str(error), file=sys.stderr); raise SystemExit(2)
