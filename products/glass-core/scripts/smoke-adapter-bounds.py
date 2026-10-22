#!/usr/bin/env python3
"""Offline adapter frame, cleanup and AST size regressions; no accounts used."""
import collections
import importlib.util
import json
import os
from pathlib import Path
import sys
import tempfile
import time

ROOT = Path(__file__).resolve().parents[1]
def load(name, path):
    spec = importlib.util.spec_from_file_location(name, ROOT / path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module

adapters = load('adapters', 'extensions/adapters/adapters.py')
structure = load('structure', 'extensions/structure/structure.py')
with tempfile.TemporaryDirectory(prefix='gc-adapter-bounds-') as tmp:
    root = Path(tmp)
    script = root / 'fixture.py'
    script.write_text('''import json,sys,time
for line in sys.stdin:
    req=json.loads(line)
    print(json.dumps({'jsonrpc':'2.0','id':req['id'],'result':{'protocolVersion':1}}),flush=True)
''')
    adapter = adapters.Adapter('fixture', {'protocol':'acp', 'command':[sys.executable, str(script)]}, tmp)
    try:
        deadline = time.monotonic()+3
        while not adapter.ready:
            assert time.monotonic() < deadline
            adapter.poll(); time.sleep(.01)
        for invalid in [[], {'id':[]}, {'id':{}}, {'id':True}]:
            try:
                adapter.receive(invalid)
            except ValueError:
                pass
            else:
                raise AssertionError(f'accepted malformed frame {invalid}')
        try:
            adapter.request('session/prompt', {})
        except PermissionError:
            pass
        else:
            raise AssertionError('external execution must be opt-in')
        ticket = adapter.request('session/new', {})
        assert isinstance(ticket, int)
    finally:
        adapter.stop()
        adapter.stop()
    assert adapter.child.returncode is not None
    for old, new in [('value', 'updated'), ('界', 'renamed')]:
        source = f'{old} = 1\nprint({old}) # {old}\n'.encode()
        transformed, count = structure.transform(source, old, new)
        assert count == 2 and f'# {old}' in transformed and f'print({new})' in transformed
    try:
        structure.transform(b'x=1\n' * 150000, 'x', 'y')
    except ValueError:
        pass
    else:
        raise AssertionError('oversized AST accepted')
print('PASS adapter malformed frames, opt-in execution, tickets, idempotent cleanup, Unicode AST and AST bounds')
