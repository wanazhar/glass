#!/usr/bin/env python3
"""Offline release PTY checks for consolidated navigation and output inspection."""
import base64, json, os, tempfile
from pathlib import Path
from pty_vt import Terminal
def seek(t, marker):
    t.send('\x1b[H');t.collect(.1)
    for _ in range(30):
        if marker in t.vt.text():return
        t.send('\x1b[B');t.collect(.04)
    t.visible(marker)
with tempfile.TemporaryDirectory(prefix='gc-consolidation-pty-') as temp:
    root=Path(temp); workspace=root/'project'; workspace.mkdir()
    config=root/'config/glass-core'; config.mkdir(parents=True)
    data=root/'data/glass-core'; data.mkdir(parents=True)
    (config/'config.toml').write_text('provider="openai"\nmodel="offline-fixture"\napi_key_env=""\nbase_url="http://127.0.0.1:1/v1"\n')
    spill=root/'spill.txt'; spill.write_text('SAVED_RANGE_MARKER\n')
    rows=[{'k':'user','t':'question'}, {'k':'assistant','t':'## RESPONSE_MARKER\n\n```rust\nlet x = 1;\n```'},
          {'k':'bash','id':'call-test','cmd':'printf COMMAND_MARKER','out':'OUTPUT_MARKER\n','code':0,'trunc':True,'spill':str(spill)}]
    (data/'fixture.jsonl').write_text(''.join(json.dumps(row)+'\n' for row in rows))
    env=dict(os.environ,XDG_CONFIG_HOME=str(root/'config'),XDG_DATA_HOME=str(root/'data'));env.pop('NO_COLOR',None)
    for size in [(120,36),(40,24),(20,8)]:
        t=Terminal(workspace,env,size,args=('resume','fixture'))
        try:
            t.idle();t.send('/');t.visible('Conversations');t.visible('Settings');t.visible('Work')
            t.send('\x1b');t.collect(.2)
            t.send('/copy code\r');t.visible('Outputs');t.send('\r');t.visible('COMMAND_MARKER' if size[0]>20 else 'COMMAND')
            t.send('k');t.collect(.2);assert '\x1b]52;c;'+base64.b64encode(b'printf COMMAND_MARKER').decode()+'\x07' in t.bytes
            t.send('r');t.collect(.1);seek(t,'OUTPUT_MARKER');t.send('l');t.collect(.1);t.send('\x1b[F');t.visible('SAVED_RANGE_MARKER' if size[0]>20 else 'SAVED_RANGE_MARK');t.send('\x1b');t.collect(.1);t.send('\x1b');t.collect(.2)
            t.send('/responses\r');t.visible('Outputs');t.send('\r');t.visible('RESPONSE_MARKER');t.send('b');t.visible('Code blocks');t.send('\r');t.visible('let x = 1;')
            t.send('\x1b');t.collect(.1);t.send('\x1b');t.collect(.1);t.send('\x1b');t.collect(.2)
            t.send('/work\r');t.visible('Work');t.send('\x1b');t.collect(.2)
            t.send('/settings\r');t.visible('Settings');t.send('\x1b');t.collect(.2)
            t.send('DRAFT');t.collect(.1);t.send('\x10');t.visible('Settings');t.send('\x1b');t.visible('DRAFT')
            print('PASS consolidation',size)
        finally:t.close()
    assert not list(data.glob('*.requests.jsonl')), 'Local discovery unexpectedly invoked provider'
    print('PASS local navigation performs no model request')
