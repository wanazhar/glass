#!/usr/bin/env python3
"""Actual binary: transaction lock, stale recovery, rollback and settings provenance."""
import fcntl
import hashlib
import json
import os
from pathlib import Path
import subprocess
import tempfile

ROOT=Path(__file__).resolve().parents[1]
BIN=Path(os.environ.get('GLASS_CORE_TEST_BINARY',ROOT/'target/release/glass-core')).resolve()
with tempfile.TemporaryDirectory(prefix='gc-transaction-review-') as tmp:
    root=Path(tmp)
    cfg=root/'config/glass-core';cfg.mkdir(parents=True)
    env=dict(os.environ,XDG_CONFIG_HOME=str(root/'config'),XDG_DATA_HOME=str(root/'data'))
    def run(*args, payload=None, ok=True):
        result=subprocess.run([str(BIN),*args],cwd=root,env=env,input=json.dumps(payload) if payload else '',text=True,capture_output=True,timeout=10)
        assert (result.returncode==0)==ok,(args,result.stdout,result.stderr)
        return json.loads(result.stdout) if ok else result
    original='original 界\n'
    (root/'a.txt').write_text(original)
    plan={'files':[{'path':'a.txt','expected':hashlib.sha256(original.encode()).hexdigest(),'text':'changed\n'}, {'path':'new.txt','expected':None,'text':'created\n'}]}
    preview=run('transaction','preview','--json',payload=plan)
    assert not preview['applied'] and (root/'a.txt').read_text()==original and not (root/'new.txt').exists()
    result=run('transaction','apply','--json',payload=plan)
    receipt=result['receipt']
    directory=root/'data/glass-core/transactions'
    with (directory/f'{receipt}.lock').open('r+') as guard:
        fcntl.flock(guard,fcntl.LOCK_EX|fcntl.LOCK_NB)
        run('transaction','recover',receipt,'--json',ok=False)
        assert (root/'a.txt').read_text()=='changed\n'
    (root/'a.txt').write_text('external edit\n')
    run('transaction','recover',receipt,'--json',ok=False)
    assert (root/'a.txt').read_text()=='external edit\n'
    (root/'a.txt').write_text('changed\n')
    assert run('transaction','recover',receipt,'--json')['status']=='recovered'
    assert (root/'a.txt').read_text()==original and not (root/'new.txt').exists()
    assert run('transaction','recover',receipt,'--json')['status']=='recovered'
    (cfg/'requirements.toml').write_text('sandbox="strict"\nsandbox_env="scrub"\n')
    settings=run('settings','--json')
    rows={row['setting']:row for row in settings['settings']}
    assert rows['sandbox']['source']=='admin requirement'
    assert rows['sandbox_env']['source']=='admin requirement'
    assert settings['precedence'][-1]=='admin requirement'
    (cfg/'requirements.toml').write_text('sandbox=[invalid\n')
    run('settings','--json',ok=False)
print('PASS transaction preview/apply, lock exclusion, stale recovery, retained external edits, rollback, repeat recovery, admin settings and corrupt requirements')
