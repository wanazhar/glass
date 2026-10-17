#!/usr/bin/env python3
"""Direct native file tools through release provider dispatch, permissions and PTYs."""
import http.server,json,os,re,subprocess,tempfile,threading,hashlib
from pathlib import Path
from pty_vt import Terminal
BIN=Path(os.environ.get('GLASS_CORE_TEST_BINARY','target/release/glass-core')).resolve()
class Server(http.server.ThreadingHTTPServer): daemon_threads=True
class Endpoint(http.server.BaseHTTPRequestHandler):
    def log_message(self,*_):pass
    def do_POST(self):
        body=json.loads(self.rfile.read(int(self.headers['Content-Length'])));self.server.requests.append(body)
        n=len(self.server.requests);readonly=self.server.readonly
        names=[t['function']['name'] for t in body['tools']]
        assert all(name in names for name in ['bash','state','read','edit','write']),names
        results=[m.get('content','') for m in body['messages'] if m['role']=='tool']
        calls=[]
        def call(name,args):calls.append({'index':len(calls),'id':f'native-{n}-{len(calls)}','function':{'name':name,'arguments':json.dumps(args)}})
        if n==1:
            call('write',{'path':'source.txt','expected':hashlib.sha256(b'alpha\nsecond\n').hexdigest(),'text':'blind overwrite'})
            call('read',{'path':'source.txt','limit':2})
            call('bash',{'command':'items=(NATIVE_SHELL_OK); [[ ${#items[@]} == 1 ]] && printf %s "${items[0]}"'})
            call('read',{'path':'../outside.txt'})
            for _ in range(25):call('bash',{'command':'printf AGE_READ'})
        elif n==2:
            assert 'has not been read' in results[0]
            assert '1|alpha' in results[1] and 'NATIVE_SHELL_OK' in results[2]
            assert results[3].startswith('error:')
            self.server.base=re.search(r'revision ([a-f0-9]{64})',results[1])[1]
            edit={'path':'source.txt','edits':[{'old':'alpha','new':'beta'}]}
            call('edit',dict(edit,preview=True));call('edit',edit)
        elif n==3:
            assert 'proposed' in results[-2]
            assert 'tool calls ago; reread' in results[-2]
            assert ('Read-only' in results[-1]) if readonly else ('applied' in results[-1])
            call('write',{'path':'nested/created.json','text':'{"ok":true}\n','validate':True})
            call('read',{'path':'source.txt'})
        elif n==4:
            assert ('Read-only' in results[-2]) if readonly else ('applied' in results[-2])
            expected='alpha' if readonly else 'beta'
            assert '1|'+expected in results[-1]
            call('edit',{'path':'source.txt','expected':self.server.base,'edits':[{'old':'beta','new':'gamma'}]})
        elif n==5:
            assert ('Read-only' in results[-1]) if readonly else ('stale revision' in results[-1])
            call('read',{'path':'source.txt'})
        elif n==6:
            rev=re.search(r'revision ([a-f0-9]{64})',results[-1])[1]
            call('edit',{'path':'source.txt','edits':[{'start':1,'end':1,'text':'gamma'}]})
        elif n==7:
            assert ('Read-only' in results[-1]) if readonly else ('applied' in results[-1])
            if not readonly:
                # External changes must not be blessed by auto-sampling a new hash.
                (self.server.root/'source.txt').write_text('external\nsecond\n')
                call('write',{'path':'source.txt','text':'lost external change'})
        elif n==8:
            assert 'stale revision' in results[-1]
            call('read',{'path':'source.txt'})
        elif n==9:
            call('write',{'path':'source.txt','text':'gamma\nsecond\n'})
        elif n==10:
            assert 'applied' in results[-1]
        self.send_response(200);self.send_header('Content-Type','text/event-stream');self.end_headers()
        delta={'tool_calls':calls} if calls else {'content':'FILES_OK'}
        self.wfile.write(('data: '+json.dumps({'choices':[{'delta':delta}]})+'\n\ndata: [DONE]\n\n').encode());self.wfile.flush()
for readonly,size in [(False,None),(True,None),(False,(20,8)),(False,(40,24)),(False,(80,30))]:
    with tempfile.TemporaryDirectory(prefix='gc-direct-files-') as tmp:
        root=Path(tmp);(root/'source.txt').write_text('alpha\nsecond\n');cfg=root/'config/glass-core';cfg.mkdir(parents=True)
        server=Server(('127.0.0.1',0),Endpoint);server.requests=[];server.readonly=readonly;server.root=root
        threading.Thread(target=server.serve_forever,daemon=True).start()
        profile='permission_profile="read-only"\n' if readonly else ''
        (cfg/'config.toml').write_text(f'provider="openai"\nmodel="mock"\napi_key_env=""\nbase_url="http://127.0.0.1:{server.server_port}/v1"\ntrust="always"\n'+profile)
        env=dict(os.environ,XDG_CONFIG_HOME=str(root/'config'),XDG_DATA_HOME=str(root/'data'))
        try:
            if size:
                t=Terminal(root,env,size)
                try:
                    t.idle();t.send('exercise native files\r');t.wait('FILES_OK',30)
                    assert '× write' in t.seen, 'native refusal lost tool identity'
                finally:t.close()
            else:
                p=subprocess.run([str(BIN),'run','--json','exercise native files'],cwd=root,env=env,text=True,capture_output=True,timeout=30)
                assert p.returncode==0 and 'FILES_OK' in p.stdout,(p.stdout,p.stderr)
            assert len(server.requests)==(7 if readonly else 10),len(server.requests)
            assert (root/'source.txt').read_text()==('alpha\nsecond\n' if readonly else 'gamma\nsecond\n')
            assert (root/'nested/created.json').exists()!=readonly
            records=[json.loads(line) for f in (root/'data/glass-core').glob('*.jsonl') if f.stem!='history' for line in f.read_text().splitlines()]
            calls=[r for r in records if r.get('k')=='call'];results=[r for r in records if r.get('k')=='tool']
            assert len(calls)==len(results) and len(calls)>=10
        finally:server.shutdown();server.server_close()
    print('PASS direct file tools', 'read-only' if readonly else 'writable',size or 'headless',flush=True)
