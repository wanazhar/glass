#!/usr/bin/env python3
"""Exercise native state namespaces through a real release provider wire and PTY."""
import http.server, json, os, subprocess, tempfile, threading
from pathlib import Path
from pty_vt import Terminal
BIN=Path(os.environ.get('GLASS_CORE_TEST_BINARY','target/release/glass-core')).resolve()
class Server(http.server.ThreadingHTTPServer): daemon_threads=True
class Endpoint(http.server.BaseHTTPRequestHandler):
    def log_message(self,*_): pass
    def do_POST(self):
        body=json.loads(self.rfile.read(int(self.headers['Content-Length']))); self.server.requests.append(body)
        n=len(self.server.requests)
        assert any(t.get('function',{}).get('name')=='state' for t in body['tools'])
        if n==1: args={'guidance':{'action':'read','topic':'learning'}}
        elif n==2:
            assert 'no approval is required' in json.dumps(body)
            args={'neuralnets':{'action':'save','kind':'lesson','text':'Use bounded reads and exact revisions'}}
        elif n==3:
            assert 'session:' in json.dumps(body) and 'active' in json.dumps(body)
            args={'neuralnets':{'action':'search','query':'bounded'}}
        elif n in (4,5,6,7):
            result=json.loads([m['content'] for m in body['messages'] if m['role']=='tool'][-1])
            entry=next(e for e in result['entries'] if e['kind']=='lesson')
            action={4:'update',5:'forget',6:'restore',7:'read'}[n]
            memory={'action':action,'id':entry['id']}
            if n!=7: memory['expected']=result['revision']
            if n==4: memory['text']='Use bounded reads and verified exact revisions'
            args={'neuralnets':memory}
        else: args=None
        self.send_response(200); self.send_header('Content-Type','text/event-stream'); self.end_headers()
        delta={'tool_calls':[{'index':0,'id':f'learning-{n}','function':{'name':'state','arguments':json.dumps(args)}}]} if args else {'content':'NN_DONE'}
        self.wfile.write(('data: '+json.dumps({'choices':[{'delta':delta}]})+'\n\ndata: [DONE]\n\n').encode());self.wfile.flush()
for size in [None,(20,8),(40,24),(80,30)]:
    with tempfile.TemporaryDirectory(prefix='gc-neuralnets-wire-') as tmp:
        root=Path(tmp); cfg=root/'config/glass-core';cfg.mkdir(parents=True)
        server=Server(('127.0.0.1',0),Endpoint);server.requests=[]
        threading.Thread(target=server.serve_forever,daemon=True).start()
        (cfg/'config.toml').write_text(f'provider="openai"\nmodel="mock"\napi_key_env=""\nbase_url="http://127.0.0.1:{server.server_port}/v1"\ntrust="always"\n')
        env=dict(os.environ,XDG_CONFIG_HOME=str(root/'config'),XDG_DATA_HOME=str(root/'data'),GLASS_CORE_TEST_BINARY=str(BIN))
        try:
            proposal=subprocess.run([str(BIN),'neuralnets','propose','fact','Project checks use bounded reads','--json'],cwd=root,env=env,text=True,capture_output=True,check=True)
            saved=json.loads(proposal.stdout)
            if size:
                t=Terminal(root,env,size)
                try:
                    t.idle(); t.send('remember a verified learning with bounded reads\r'); t.wait('NN_DONE',20);t.collect(.3)
                    t.send('/guidance\r');t.collect(.5);t.send('\x1b');t.collect(.2);t.send('\x1b');t.collect(.2)
                    t.send('/neuralnets\r');t.collect(.5)
                    assert 'Neuralnets' in t.vt.text(),t.vt.text()
                    t.send('\x1b[B\x1b[B\r');t.collect(.3)
                    t.send('\r');t.collect(.3)
                    t.send('\x1b');t.collect(.2)
                finally:t.close()
            else:
                p=subprocess.run([str(BIN),'run','--json','remember a verified learning with bounded reads'],cwd=root,env=env,text=True,capture_output=True,timeout=20)
                assert p.returncode==0 and 'NN_DONE' in p.stdout,(p.stdout,p.stderr)
            assert len(server.requests)==8,len(server.requests)
            systems=[json.dumps([m for m in r['messages'] if m['role']=='system']) for r in server.requests]
            assert len(set(systems))==1,'proposal changed the cache prefix'
            store=json.loads(next((root/'data/glass-core/neuralnets').glob('*.json')).read_text())
            assert len(store['entries'])==2 and store['entries'][1]['status']=='active'
            assert 'Project checks use bounded reads' in systems[0]
            assert 'reviewed-project-learning' in systems[0]
            assert 'tool:learning-4' in store['entries'][1]['source']
            recalled=subprocess.run([str(BIN),'neuralnets','search','exact revisions','--json'],cwd=root,env=env,capture_output=True,text=True,check=True)
            assert json.loads(recalled.stdout)['entries'][0]['status']=='active'
        finally:server.shutdown();server.server_close()
    print('PASS neuralnets native wire',size or 'headless')
