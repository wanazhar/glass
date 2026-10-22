#!/usr/bin/env python3
"""Shared conversation chooser, bounded preview, paging and active writer safety."""
import json,os,tempfile
from pathlib import Path
from pty_vt import Terminal
with tempfile.TemporaryDirectory(prefix='gc-conversations-') as temp:
    root=Path(temp);workspace=root/'project';workspace.mkdir();other=root/'other';other.mkdir()
    cfg=root/'config/glass-core';cfg.mkdir(parents=True)
    data=root/'data/glass-core';data.mkdir(parents=True)
    (cfg/'config.toml').write_text('provider="openai"\nmodel="offline"\napi_key_env=""\nbase_url="http://127.0.0.1:1/v1"\n')
    for i in range(70):
        name=f'thread-{i:03}'
        (data/f'{name}.jsonl').write_text(json.dumps({'k':'user','t':f'QUESTION_{i}'})+'\n'+json.dumps({'k':'assistant','t':f'PREVIEW_{i}'})+'\n')
        (data/f'{name}.meta.json').write_text(json.dumps({'workspace':str(workspace),'title':f'TITLE_{i:03}'}))
        (data/f'{name}.nav.json').write_text(json.dumps({'model':'offline','status':'done','events':2,'viewed':0,'parent':'thread-069' if i==68 else None}))
    (data/'foreign.jsonl').write_text('{"k":"user","t":"FOREIGN_PREVIEW"}\n')
    (data/'foreign.meta.json').write_text(json.dumps({'workspace':str(other),'title':'FOREIGN_TITLE'}))
    env=dict(os.environ,XDG_CONFIG_HOME=str(root/'config'),XDG_DATA_HOME=str(root/'data'));env.pop('NO_COLOR',None)
    for size in [(80,30),(40,24),(20,8)]:
        t=Terminal(workspace,env,size,args=('resume','thread-069'))
        try:
            t.idle();t.send('/sessions\r');t.visible('Conversations')
            t.send('Search all\r');t.visible('Search conversations');t.send('TITLE_000\r');t.collect(.15);t.send('\r');t.visible('Conversation');t.visible('TITLE_000')
            t.send('p');t.visible('PREVIEW_0');t.send('\x1b');t.collect(.1);t.send('\x1b');t.collect(.1);t.send('\x1b');t.collect(.15)
            t.send('/branches\r');t.visible('Conversation altern' if size[0]>20 else 'Conversation');t.send('TITLE_068\r');t.visible('Conversation');t.send('\x1b[F');t.collect(.1);t.send('\x1b');t.collect(.1);t.send('\x1b');t.collect(.15)
            t.send('/sessions\r');t.visible('Conversations');t.send('TITLE_069\r');t.visible('Conversation');t.send('d');t.visible('Switch');t.send('\x1b');t.collect(.1);t.send('\x1b');t.collect(.1);t.send('\x1b');t.collect(.15)
            assert (data/'thread-069.jsonl').exists()
            print('PASS shared conversations',size,flush=True)
        finally:t.close()
    for size in [(40,24),(20,8)]:
        t=Terminal(workspace,env,size,args=('resume','--browse'))
        try:
            t.visible('Conversations');t.send('TITLE_069\r');t.visible('Conversation');t.send('r');t.idle();t.send('/sessions\r');t.visible('Conversations');t.send('Next page\r');t.collect(.15);t.send('TITLE_000\r');t.visible('TITLE_000')
            print('PASS CLI browse and older page',size,flush=True)
        finally:t.close()
    owner=Terminal(workspace,env,(40,24),args=('resume','thread-069'))
    browser=Terminal(workspace,env,(40,24),args=('resume','--browse'))
    try:
        owner.idle();browser.visible('Conversations');browser.send('TITLE_069\r');browser.visible('Conversation');browser.send('d');browser.visible('Delete conversation?');browser.send('Delete permanently\r');browser.visible('already open')
        assert (data/'thread-069.jsonl').exists(),'Deleted another active writer'
        print('PASS other active writer deletion refused',flush=True)
    finally:browser.close();owner.close()
    assert json.loads((data/'thread-000.nav.json').read_text())['viewed']==0,'Preview mutated unread cursor'
    t=Terminal(workspace,env,(40,24),args=('resume','thread-069'))
    try:
        t.idle();t.send('/sessions\r');t.visible('Conversations');t.send('Search all\r');t.visible('Search conversations');t.send('TITLE_000\r');t.collect(.15);t.send('\r');t.visible('TITLE_000');t.send('n');t.visible('Rename conversation');t.send('\x15RENAMED_FIXTURE\r');t.collect(.15);t.send('\x1b');t.collect(.15)
        assert json.loads((data/'thread-000.meta.json').read_text())['title']=='RENAMED_FIXTURE'
        before=set(data.glob('*.jsonl'))
        t.send('/sessions\r');t.visible('Conversations');t.send('Search all\r');t.visible('Search conversations');t.send('RENAMED_FIXTURE\r');t.collect(.15);t.send('\r');t.visible('RENAMED_FIXTURE');t.send('f');t.visible('Fork conversation');t.send('\r');t.collect(.5)
        forks=set(data.glob('*.jsonl'))-before;assert len(forks)==1,forks
        fork=next(iter(forks));assert json.loads(fork.with_suffix('.nav.json').read_text())['parent']=='thread-000'
        assert json.loads(fork.with_suffix('.meta.json').read_text())['title']=='RENAMED_FIXTURE (branch)'
        t.send('/sessions\r');t.visible('Conversations');t.send('Search all\r');t.visible('Search conversations');t.send('thread-000\r');t.collect(.15);t.send('\r');t.visible('RENAMED_FIXTURE');t.send('d');t.visible('Delete conversation?');t.send('Delete permanently\r');t.collect(.3)
        assert not (data/'thread-000.jsonl').exists();assert fork.exists()
        print('PASS explicit rename, fork lineage, resume and confirmed inactive deletion',flush=True)
    finally:t.close()
    assert not list(data.glob('*.requests.jsonl')),'Local chooser invoked provider'
    print('PASS global search beyond first page, shared branches, active writer guard, read-only preview, no provider calls')
