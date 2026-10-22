#!/usr/bin/env python3
"""Exercise native file permissions in isolated CLI processes, with no provider."""
import json
import os
from pathlib import Path
import subprocess
import tempfile
from core_binary import core_binary_path

binary = Path(os.environ.get('GLASS_CORE_TEST_BINARY', str(core_binary_path('release')))).resolve()
with tempfile.TemporaryDirectory(prefix='gc-file-permissions-') as tmp:
    root = Path(tmp)
    workspace = root/'workspace'
    workspace.mkdir()
    external = root/'outside.txt'
    external.write_text('outside original')
    inside = workspace/'inside.txt'
    inside.write_text('inside original')
    config = root/'config/glass-core'
    config.mkdir(parents=True)
    env = dict(os.environ, XDG_CONFIG_HOME=str(root/'config'), XDG_DATA_HOME=str(root/'data'))
    def run(profile, args, value=None):
        (config/'config.toml').write_text(f'permission_profile="{profile}"\n')
        return subprocess.run([str(binary),'file','--json',*args],cwd=workspace,env=env,
                              input=None if value is None else json.dumps(value),capture_output=True,text=True,timeout=10)
    for path in (str(external), '../outside.txt'):
        result = run('unrestricted', ['read',path])
        assert result.returncode == 0, result.stderr
        revision = json.loads(result.stdout)['revision']
    result = run('unrestricted',['write'],{'path':str(external),'expected':revision,'text':'outside changed'})
    assert result.returncode == 0 and external.read_text() == 'outside changed', result.stderr
    for profile in ('read-only',):
        result = run(profile,['read',str(external)])
        assert result.returncode != 0 and 'permission profile' in result.stderr, result.stderr
    result = run('workspace-edit',['read',str(external)])
    assert result.returncode == 0, result.stderr
    result = run('read-only',['write'],{'path':'new.txt','expected':None,'text':'blocked'})
    assert result.returncode != 0 and not (workspace/'new.txt').exists(), result.stderr
    outside_new = root/'outside-new.txt'
    denied = run('workspace-edit',['write'],{'path':str(outside_new),'expected':None,'text':'blocked'})
    assert denied.returncode != 0 and not outside_new.exists(), denied.stderr
    result = run('workspace-edit',['write'],{'path':'new.txt','expected':None,'text':'allowed'})
    assert result.returncode == 0 and (workspace/'new.txt').read_text() == 'allowed', result.stderr
    (workspace/'link').symlink_to(external)
    result = run('unrestricted',['read','link'])
    assert result.returncode != 0, 'Symlink traversal unexpectedly accepted'
print('PASS unrestricted absolute/relative external reads and guarded writes; restrictive scope; read-only mutations; symlink refusal')
