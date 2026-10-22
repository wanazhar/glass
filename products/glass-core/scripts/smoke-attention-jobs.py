#!/usr/bin/env python3
"""Opening a completion inspector must preserve session-owned sibling jobs."""
import json,os,subprocess,tempfile
from pathlib import Path
from pty_vt import Terminal
for size in [(80,30),(40,24),(20,8)]:
    with tempfile.TemporaryDirectory(prefix='gc-attention-jobs-') as tmp:
        root=Path(tmp);cfg=root/'config/glass-core';cfg.mkdir(parents=True)
        (cfg/'config.toml').write_text('provider="openai"\nmodel="mock"\napi_key_env=""\nbase_url="http://127.0.0.1:1/v1"\ntrust="always"\n')
        env=dict(os.environ,XDG_CONFIG_HOME=str(root/'config'),XDG_DATA_HOME=str(root/'data'))
        name='safe\u202etxt';(root/name).write_text('hello\u2066world\n')
        binary=os.environ.get('GLASS_CORE_TEST_BINARY',str(Path(__file__).resolve().parents[1]/'target/debug/glass-core'))
        output=subprocess.run([binary,'file','read',name],cwd=root,env=env,text=True,capture_output=True,check=True).stdout
        assert '\u202e' not in output and '\\u{202e}' in output and '\\u{2066}' in output,output
        raw=subprocess.run([binary,'file','--json','read',name],cwd=root,env=env,text=True,capture_output=True,check=True).stdout
        assert json.loads(raw)['path']==name and '\u2066' in json.loads(raw)['text']
        t=Terminal(root,env,size)
        try:
            t.idle();t.send('/bg echo ATTENTION_DONE\u202e\r');t.wait('job 1 pid');t.collect(.2)
            t.send('/bg sleep 30\r');t.wait('job 2 pid');t.collect(.2)
            t.send('/attention\r');t.visible('Attention');t.send('background completion');t.collect(.1);t.send('\r');t.visible('Job 1');t.visible('ATTENTION_DONE');assert '\\u{202e}' in t.vt.text().replace('\n','').replace(' ',''),t.vt.text()
            t.send('\x1b');t.collect(.2);t.visible('Attention');t.send('\x1b');t.collect(.2)
            t.send('/jobs\r');t.visible('Background jobs');t.visible('2: running')
            assert 'No jobs yet' not in t.vt.text(),t.vt.text()
        finally:t.close()
    print('PASS completion inspector preserves sibling job',size,flush=True)
