#!/usr/bin/env python3
"""A recovery replay must not turn subsequent identical calls into stale cache hits."""
import http.server, json, os, subprocess, tempfile, threading
from pathlib import Path
from pty_vt import Terminal
BIN=Path(os.environ.get('GLASS_CORE_TEST_BINARY',Path(__file__).resolve().parents[1]/'target/release/glass-core')).resolve()
class Provider(http.server.BaseHTTPRequestHandler):
    step=0
    def log_message(self,*_):pass
    def do_POST(self):
        self.rfile.read(int(self.headers['Content-Length']))
        step=type(self).step;type(self).step+=1
        if step==1:
            frame={'error':{'message':'fixture failure after completed action'}}
        elif step in (0,2,3):
            frame={'choices':[{'delta':{'tool_calls':[{'index':0,'id':f'call-{step}','function':{'name':'bash','arguments':json.dumps({'command':"python3 -c 'from pathlib import Path; p=Path(\"counter\"); p.write_text((p.read_text() if p.exists() else \"\")+\"x\"); print(p.read_text())'"})}}]}}]}
        else:frame={'choices':[{'delta':{'content':'FRESH_DONE'}}]}
        payload=('data: '+json.dumps(frame)+'\n\ndata: [DONE]\n\n').encode()
        self.send_response(200);self.send_header('Content-Type','text/event-stream');self.send_header('Content-Length',str(len(payload)));self.end_headers();self.wfile.write(payload)
for size in [(80,30),(40,24),(20,8)]:
    with tempfile.TemporaryDirectory(prefix='gc-retry-fresh-') as tmp:
        root=Path(tmp);cfg=root/'config/glass-core';cfg.mkdir(parents=True)
        Provider.step=0
        server=http.server.ThreadingHTTPServer(('127.0.0.1',0),Provider)
        threading.Thread(target=server.serve_forever,daemon=True).start()
        (cfg/'config.toml').write_text(f'provider="openai"\nmodel="fixture"\napi_key_env=""\nbase_url="http://127.0.0.1:{server.server_port}/v1"\ntrust="always"\n')
        env=dict(os.environ,XDG_CONFIG_HOME=str(root/'config'),XDG_DATA_HOME=str(root/'data'))
        first=subprocess.run([str(BIN),'run','--json','exercise recovery'],cwd=root,env=env,text=True,capture_output=True,timeout=15)
        assert first.returncode!=0,first.stdout
        session=json.loads(first.stdout)['session']
        assert (root/'counter').read_text()=='x'
        terminal=Terminal(root,env,size,args=('resume',session))
        try:
            terminal.idle();terminal.send('/retry\r');terminal.visible('Recover response');terminal.send('\r')
            terminal.wait('FRESH_DONE',15)
            assert (root/'counter').read_text()=='xx', ('retry counter', (root/'counter').read_text())
            print('PASS one replay then fresh identical action',size,flush=True)
        finally:terminal.close();server.shutdown();server.server_close()
