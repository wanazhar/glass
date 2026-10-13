#!/usr/bin/env python3
"""Real CLI migration, overwrite, review revision and backup preservation."""
import json,os,subprocess,tempfile
from pathlib import Path
BIN=Path(os.environ.get('GLASS_CORE_TEST_BINARY',Path(__file__).resolve().parents[1]/'target/release/glass-core')).resolve()
with tempfile.TemporaryDirectory(prefix='gc-knowledge-migration-') as temp:
    root=Path(temp);env=dict(os.environ,XDG_DATA_HOME=str(root/'data'),XDG_CONFIG_HOME=str(root/'config'))
    def run(*args,ok=True):
        result=subprocess.run([str(BIN),*args,'--json'],cwd=root,env=env,capture_output=True,text=True)
        assert (result.returncode==0)==ok,(args,result.stdout,result.stderr)
        return json.loads(result.stdout) if ok else result.stderr
    run('memory','set','build','Use bounded reads','human review')
    run('memory','enable')
    canonical=next((root/'data/glass-core/neuralnets').glob('*.json'))
    stored=json.loads(canonical.read_text());facts=stored.pop('reviewed')
    legacy=canonical.parent.parent/'memory'/canonical.name;legacy.parent.mkdir();legacy.write_text(json.dumps(facts))
    canonical.write_text(json.dumps(stored));before=canonical.read_bytes();backup=legacy.read_bytes()
    assert run('memory','list')['enabled']
    assert canonical.read_bytes()==before,'list unexpectedly migrated persistent data'
    proposal=run('neuralnets','propose','fact','Use bounded reads')
    migrated=json.loads(canonical.read_text());assert migrated['reviewed']==facts and legacy.read_bytes()==backup
    ident=proposal['entries'][0]['id'];revision=proposal['revision']
    run('memory','set','build','Use transactional edits','second review')
    assert run('memory','list')['facts']['build']['text']=='Use transactional edits'
    assert 'Stale revision' in run('neuralnets','approve',ident,str(revision),ok=False)
    current=run('neuralnets','list');run('neuralnets','approve',ident,str(current['revision']))
    run('memory','disable');assert not run('memory','list')['enabled']
    assert legacy.read_bytes()==backup,'migration changed legacy backup'
    run('memory','forget','build');assert run('memory','list')['facts']=={}
    assert canonical.stat().st_mode & 0o077==0
    print('PASS unified knowledge: explicit migration, read-only list, backups, opt-in, key overwrite, stale review, forget, private storage')
