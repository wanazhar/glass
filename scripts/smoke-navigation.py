#!/usr/bin/env python3
"""PTY proof of searchable recorded tools, exact explicit clipboard and key help."""
import base64
import http.server
import json
import os
from pathlib import Path
import re
import subprocess
import tempfile
import threading
from pty_vt import Terminal

COMMANDS=["printf 'SEARCHABLE_OUTPUT_ONLY\\n'", "printf '\\033]52;c;EVIL\\007CONTROL_OUTPUT\\n'", ":"]
class Endpoint(http.server.BaseHTTPRequestHandler):
    requests=0
    def log_message(self,*_):pass
    def do_POST(self):
        body=json.loads(self.rfile.read(int(self.headers['Content-Length'])));type(self).requests+=1
        results=[m for m in body['messages'] if m['role']=='tool']
        delta={'tool_calls':[{'index':0,'id':f'tool-{len(results)}','function':{'name':'bash','arguments':json.dumps({'command':COMMANDS[len(results)]})}}]} if len(results)<len(COMMANDS) else {'content':'NAVIGATION_FIXTURE_READY'}
        data=('data: '+json.dumps({'choices':[{'delta':delta}]})+'\n\ndata: [DONE]\n\n').encode()
        self.send_response(200);self.send_header('Content-Type','text/event-stream');self.send_header('Content-Length',str(len(data)));self.end_headers();self.wfile.write(data)

def action(t,label):
    t.bytes='';t.send(label+'\r');t.collect(.2)
def back(t):t.send('\x1b');t.collect(.15)
def copied(t,label):
    t.bytes='';t.send(label+'\r');t.wait('\x1b]52;c;')
    payload=re.search(r'\x1b]52;c;([^\x07]*)\x07',t.bytes)
    assert payload,(t.vt.text(),t.bytes[-2000:])
    return base64.b64decode(payload.group(1)).decode()

with tempfile.TemporaryDirectory(prefix='gc-navigation-pty-') as tmp:
    root=Path(tmp);subprocess.run(['git','init','-q',str(root)],check=True)
    cfg=root/'config/glass-core';cfg.mkdir(parents=True)
    server=http.server.ThreadingHTTPServer(('127.0.0.1',0),Endpoint);threading.Thread(target=server.serve_forever,daemon=True).start()
    (cfg/'config.toml').write_text(f'provider="openai"\nmodel="mock"\napi_key_env=""\nbase_url="http://127.0.0.1:{server.server_port}/v1"\n')
    env=dict(os.environ,XDG_CONFIG_HOME=str(root/'config'),XDG_DATA_HOME=str(root/'data'))
    binary=os.environ.get('GLASS_CORE_TEST_BINARY',str(Path(__file__).resolve().parents[1]/'target/debug/glass-core'))
    try:
        run=subprocess.run([binary,'run','--json','navigation fixture'],cwd=root,env=env,text=True,capture_output=True,timeout=15)
        assert run.returncode==0,run.stderr
        session=json.loads(run.stdout)['session'];requests=Endpoint.requests
        records=[json.loads(s) for p in (root/'data/glass-core').glob('*.jsonl') for s in p.read_text().splitlines()]
        outputs={r['cmd']:r['out'] for r in records if r['k']=='bash'}
        for size in [(65,30),(40,24),(24,16),(20,12),(20,8)]:
            t=Terminal(root,env,size,args=('resume',session))
            try:
                t.idle();t.send('/details\r');t.visible('Tool details')
                t.send('DOES_NOT_EXIST');t.collect(.15);assert 'No matches' in t.vt.text(),t.vt.text()
                t.send('\x15SEARCHABLE_OUTPUT_ONLY\r');t.visible('Tool actions')
                assert copied(t,'Copy command')==COMMANDS[0]
                back(t);assert copied(t,'Copy recorded output')==outputs[COMMANDS[0]]
                back(t);back(t);back(t)
                t.send('/details\r');t.visible('Tool details');t.send('CONTROL_OUTPUT\r');t.visible('Tool actions')
                action(t,'Read');assert '\x1b]52;c;EVIL' not in t.bytes,t.bytes
                back(t);assert copied(t,'Copy recorded output')==outputs[COMMANDS[1]]
                assert '\x1b]52;c;EVIL\x07' in outputs[COMMANDS[1]],'Fixture must retain original controls'
                back(t);back(t);back(t)
                t.send('/details\r');t.visible('Tool details');t.send('exit 0 · :\r');t.visible('Tool actions')
                empty_copy=copied(t,'Copy recorded output')
                assert empty_copy==outputs[COMMANDS[2]],(repr(empty_copy),repr(outputs[COMMANDS[2]]))
                back(t);back(t);back(t)
                t.send('/keys\r');t.visible('Keyboard shortcuts');t.collect(.1)
                assert 'Draft' in t.vt.text(),t.vt.text()
                t.send('\x1b[6~');t.collect(.1);t.send('\x1b[F');t.collect(.1)
                assert 'Ctrl-G' in t.vt.text() or 'PageUp' in t.vt.text() or 'conversation' in t.vt.text(),t.vt.text()
                back(t);assert Endpoint.requests==requests,'Inspect/copy/help must not send provider requests'
                assert t.seen.count('\x1b[?1049h')==1,('Modal must not nest alternate screens',t.seen.count('\x1b[?1049h'),t.seen.count('\x1b[?1049l'))
            finally:t.close()
    finally:server.shutdown()
print('PASS: searchable tool outputs/names/commands, empty search/output, safe previews, exact OSC52 controls copy, no execution and keyboard discovery at five sizes')
