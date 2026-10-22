#!/usr/bin/env python3
"""Offline real-PTY and headless checks for native files and owned bash jobs."""
import http.server,json,os,re,subprocess,tempfile,threading,time
from pathlib import Path
from pty_vt import Terminal
from core_binary import core_binary_path
BIN=Path(os.environ.get('GLASS_CORE_TEST_BINARY',core_binary_path('debug'))).resolve()
class Provider(http.server.BaseHTTPRequestHandler):
    requests=[]
    def log_message(self,*_):pass
    def do_POST(self):
        body=json.loads(self.rfile.read(int(self.headers['Content-Length'])));type(self).requests.append(body)
        latest=max(i for i,m in enumerate(body['messages']) if m['role']=='user')
        results=[m['content'] for m in body['messages'][latest+1:] if m['role']=='tool'];n=len(results)
        if n==0:args={'command':'"$GC_BIN" file read sample.json'}
        elif n==1:
            revision=re.search(r'revision ([a-f0-9]{64})',results[0]).group(1)
            payload=json.dumps({'path':'sample.json','expected':revision,'edits':[{'start':2,'end':2,'text':'  "value": 42'}],'validate':True})
            args={'command':"\"$GC_BIN\" file edit <<'GC_EDIT'\n"+payload+'\nGC_EDIT'}
        elif n==2:args={'command':"values=(one two); [[ ${values[1]} == two ]] || exit 9; printf 'JOBREADY\\n'; read line; printf 'ECHO:%s\\n' \"$line\"; sleep 30",'job':{'action':'start'}}
        elif n==3:args={'job':{'action':'input','id':1,'text':'native-input\n'}}
        elif n==4:args={'command':'sleep 0.1'}
        elif n==5:args={'job':{'action':'poll','id':1,'offset':0}}
        elif n==6:
            assert 'ECHO:native-input' in results[-1],results[-1]
            metadata=json.loads(results[-1].split('[job metadata] ')[1]);args={'job':{'action':'poll','id':1,'offset':metadata['next_offset']}}
        elif n==7:
            assert 'ECHO:native-input' not in results[-1],results[-1];args={'job':{'action':'stop','id':1}}
        elif n==8:args={'command':'"$GC_BIN" file check broken.json'}
        else:
            assert '[exit 1]' in results[-1] and 'broken.json:2:' in results[-1],results[-1];args=None
        if args is None:frames=[{'choices':[{'delta':{'content':'NATIVEFLOWDONE'}}]}]
        else:frames=[{'choices':[{'delta':{'tool_calls':[{'index':0,'id':f'call-{n}','function':{'name':'bash','arguments':json.dumps(args)}}]}}]}]
        self.send_response(200);self.send_header('Content-Type','text/event-stream');self.end_headers()
        for frame in frames:self.wfile.write(('data: '+json.dumps(frame)+'\n\n').encode());self.wfile.flush()
        self.wfile.write(b'data: [DONE]\n\n');self.wfile.flush()
for size in [(80,30),(40,24),(20,8)]:
    with tempfile.TemporaryDirectory(prefix='gc-native-pty-') as tmp:
        root=Path(tmp);config=root/'config/glass-core';config.mkdir(parents=True);data=root/'data/glass-core';data.mkdir(parents=True)
        (root/'sample.json').write_text('{\n  "value": 1\n}\n');(root/'broken.json').write_text('{\n bad\n}\n')
        server=http.server.ThreadingHTTPServer(('127.0.0.1',0),Provider);threading.Thread(target=server.serve_forever,daemon=True).start();Provider.requests=[]
        (config/'config.toml').write_text(f'provider="openai"\nmodel="mock"\napi_key_env=""\nbase_url="http://127.0.0.1:{server.server_port}/v1"\n')
        env=dict(os.environ,XDG_CONFIG_HOME=str(root/'config'),XDG_DATA_HOME=str(root/'data'));env.pop('NO_COLOR',None)
        t=Terminal(root,env,size)
        try:
            t.idle();t.send('exercise native files and jobs\r');t.wait('NATIVEFLOWDONE',25);t.collect(.2)
            assert json.loads((root/'sample.json').read_text())['value']==42
            assert '✓ syntax' in t.seen and '◆ sample.json' in t.seen,t.seen
            assert '\x1b[38;5;' in t.seen,t.seen
            if size[0]>=40: assert '-   "value": 1' in re.sub(r'\x1b\[[0-?]*[ -/]*[@-~]','',t.seen),t.seen
            assert all({'bash','state','read','edit','write'} <= {x['function']['name'] for x in request['tools']} for request in Provider.requests)
            t.send('draft');t.collect(.1);t.resize(20,8);t.resize(*size);assert 'draft' in t.vt.text(),t.vt.text()
            records=[json.loads(line) for p in data.glob('*.jsonl') if p.stem!='history' for line in p.read_text().splitlines()]
            assert any(r.get('k')=='tool' and 'ECHO:native-input' in str(r) for r in records),records
            assert any(r.get('k')=='tool' and 'stopped job 1' in str(r) for r in records),records
            if size==(40,24):
                schemas=Provider.requests[0]['tools'];print('model tool schema bytes:',len(json.dumps(schemas,separators=(',',':')).encode()))
        finally:t.close();server.shutdown();server.server_close()
# CLI stays usable outside Git and without provider/configuration.
with tempfile.TemporaryDirectory(prefix='gc-native-headless-') as tmp:
    root=Path(tmp);env=dict(os.environ,GC_FILE_JSON='1',XDG_CONFIG_HOME=str(root/'empty'))
    payload={'path':'new.txt','expected':None,'text':'hello\n'}
    p=subprocess.run([str(BIN),'file','write'],cwd=root,env=env,input=json.dumps(payload),text=True,capture_output=True);assert p.returncode==0,p.stderr
    v=json.loads(p.stdout);assert v['kind']=='file_change'
    p=subprocess.run([str(BIN),'file','write'],cwd=root,env=env,input=json.dumps(payload),text=True,capture_output=True);assert p.returncode!=0 and 'stale' in p.stderr,p.stderr
    p=subprocess.run([str(BIN),'file','read','new.txt'],cwd=root,env=env,text=True,capture_output=True);assert json.loads(p.stdout)['text']=='1|hello\n'
    p=subprocess.run([str(BIN),'file','--json','unknown'],cwd=root,env=env,input='',text=True,capture_output=True,timeout=3);assert p.returncode!=0 and json.loads(p.stdout)['status']=='error'
    (root/'bad.json').write_text('{bad}')
    p=subprocess.run([str(BIN),'file','--json','check','bad.json'],cwd=root,env=env,text=True,capture_output=True);assert p.returncode!=0 and json.loads(p.stdout)['status']=='error'
print('PASS: native CAS edits, colored previews, diagnostics, owned background start/input/incremental poll/stop, exact records, five core tools, headless/non-Git and PTY 80x30/40x24/20x8')
