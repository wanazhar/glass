#!/usr/bin/env python3
"""Throttle recovery, cross-process spacing, and primary/reader error colors."""
import http.server,json,os,subprocess,tempfile,threading,time
from pathlib import Path
from pty_vt import Terminal
from core_binary import core_binary_path
BIN=Path(os.environ.get('GLASS_CORE_TEST_BINARY',str(core_binary_path('debug')))).resolve()
class Endpoint(http.server.BaseHTTPRequestHandler):
    def log_message(self,*_):pass
    def do_POST(self):
        s=self.server; s.bodies.append(json.loads(self.rfile.read(int(self.headers['Content-Length'])))); s.starts.append(time.monotonic()); n=len(s.starts)
        if s.mode in ('http-quota-code', 'http-quota-type', 'http-billing', 'http-quota-root'):
            field = 'type' if s.mode == 'http-quota-type' else 'code'
            code = 'billing_required' if s.mode == 'http-billing' else 'insufficient_quota'
            self.send_response(429);self.end_headers()
            error = {field:code,'message':'Provider returned error'}
            self.wfile.write(json.dumps(error if s.mode=='http-quota-root' else {'error':error}).encode());return
        if s.mode=='http-overload' and n==1:
            self.send_response(502);self.end_headers();self.wfile.write(b'{"error":{"message":"Provider returned error"}}');return
        if s.mode=='recover' and n==3:
            self.send_response(429);self.send_header('Retry-After','0.1');self.end_headers();self.wfile.write(b'{"error":{"message":"too many requests"}}');return
        self.send_response(200);self.send_header('Content-Type','text/event-stream');self.end_headers()
        if s.mode=='recover' and n==1: event={'choices':[{'delta':{'tool_calls':[{'index':0,'id':'once','function':{'name':'bash','arguments':json.dumps({'command':'printf x >> once.txt'})}}]}}]}
        elif s.mode=='recover' and n==2:event={'error':{'type':'rate_limit_error','message':'Provider returned error'}}
        elif s.mode=='overload' and n==1:event={'error':{'code':502,'message':'Provider returned error'}}
        elif s.mode=='status-only' and n==1:event={'error':{'status':'502'}}
        elif s.mode=='auth':event={'error':{'code':401,'message':'invalid API key'}}
        elif s.mode=='quota':event={'error':{'code':429,'message':'insufficient_quota'}}
        elif s.mode=='fail':event={'error':{'message':'fixture explicit failure'}}
        else:event={'choices':[{'delta':{'content':'PRESSURE_RECOVERED'}}]}
        self.wfile.write(('data: '+json.dumps(event)+'\n\ndata: [DONE]\n\n').encode())
with tempfile.TemporaryDirectory(prefix='gc-provider-pressure-') as tmp:
    root=Path(tmp);cfg=root/'config/glass-core/config.toml';cfg.parent.mkdir(parents=True)
    server=http.server.ThreadingHTTPServer(('127.0.0.1',0),Endpoint);server.daemon_threads=True;server.mode='recover';server.bodies=[];server.starts=[]
    threading.Thread(target=server.serve_forever,daemon=True).start()
    cfg.write_text(f'provider="openai"\nmodel="fixture"\napi_key_env=""\nbase_url="http://127.0.0.1:{server.server_port}/v1"\ntrust="always"\n')
    env=dict(os.environ,XDG_CONFIG_HOME=str(root/'config'),XDG_DATA_HOME=str(root/'data'));env.pop('NO_COLOR',None)
    run=lambda prompt:subprocess.run([str(BIN),'run','--json',prompt],cwd=root,env=env,text=True,capture_output=True,timeout=30)
    p=run('run the fixture')
    assert p.returncode==0 and 'PRESSURE_RECOVERED' in p.stdout,p.stdout+p.stderr
    assert (root/'once.txt').read_text()=='x'
    assert len(server.starts)==4,server.starts
    assert server.bodies[1]['messages']==server.bodies[2]['messages']==server.bodies[3]['messages'],'retry changed cached context'
    assert server.starts[2]-server.starts[1]>=1.9,server.starts
    assert server.starts[3]-server.starts[2]>=3.9,server.starts
    previous=server.starts[-1];p=run('second process')
    assert p.returncode==0,p.stdout+p.stderr
    assert server.starts[-1]-previous>=3.9,server.starts
    print('PASS SSE + HTTP throttle recovery, tools once, unchanged retry context, shared process spacing')
    for file in (root/'data/glass-core/provider-pacing').glob('*.json'):assert file.stat().st_mode & 0o777==0o600
    server.mode='overload';server.starts=[];server.bodies=[]
    env['XDG_DATA_HOME']=str(root/'overload-data')
    p=run('recover gateway rejection')
    assert p.returncode==0 and 'PRESSURE_RECOVERED' in p.stdout,p.stdout+p.stderr
    assert len(server.starts)==2 and server.starts[1]-server.starts[0]>=1.9,server.starts
    assert server.bodies[0]['messages']==server.bodies[1]['messages']
    print('PASS explicit SSE 502 recovery without changing context')
    server.mode='status-only';server.starts=[];server.bodies=[]
    env['XDG_DATA_HOME']=str(root/'status-only-data')
    p=run('recover code-only gateway rejection')
    assert p.returncode==0 and 'PRESSURE_RECOVERED' in p.stdout,p.stdout+p.stderr
    assert len(server.starts)==2 and server.starts[1]-server.starts[0]>=1.9,server.starts
    print('PASS message-free SSE status 502 recovery')
    server.mode='http-overload';server.starts=[];server.bodies=[]
    env['XDG_DATA_HOME']=str(root/'http-overload-data')
    p=run('recover HTTP gateway rejection')
    assert p.returncode==0 and 'PRESSURE_RECOVERED' in p.stdout,p.stdout+p.stderr
    assert len(server.starts)==2 and server.starts[1]-server.starts[0]>=1.9,server.starts
    print('PASS HTTP 502 learns cooldown')
    for mode in ('auth','quota','http-quota-code','http-quota-type','http-billing','http-quota-root'):
        server.mode=mode;server.starts=[];server.bodies=[]
        env['XDG_DATA_HOME']=str(root/(mode+'-data'))
        p=run('reject nonretryable provider error')
        assert p.returncode!=0 and len(server.starts)==1,(mode,server.starts,p.stdout+p.stderr)
        if mode.startswith('http-'):
            files = list((Path(env['XDG_DATA_HOME'])/'glass-core/provider-pacing').glob('*.json'))
            assert files and all(json.loads(file.read_text())['gap'] == 0 for file in files), 'quota learned false pressure'
        print('PASS no automatic replay:',mode)
    server.mode='fail'
    # Use a fresh data directory to isolate rendering from the learned cooldown.
    env['XDG_DATA_HOME']=str(root/'render-data');os.environ['GLASS_CORE_TEST_BINARY']=str(BIN)
    for size in ((80,30),(40,24),(20,8)):
        t=Terminal(root,env,size)
        try:
            t.idle();t.send('show failure\r');t.wait('/retry: Retry',10);t.collect(.2)
            def red():return any(c.fg in ('red','cd0000','ff5555','f38ba8','f7768e','ff0000','ff5f5f') and c.data.strip() for row in t.vt.active.buffer.values() for c in row.values())
            def error_style():
                for y,row in enumerate(t.vt.text().splitlines()):
                    if 'error: transport:' in row:
                        x=row.index('error: transport:')
                        return [(t.vt.active.buffer[y][x+i].fg,t.vt.active.buffer[y][x+i].bold) for i in range(17)]
                return None
            original=error_style()
            if size[1]>8:assert red(),('normal error lost red color',t.vt.text())
            t.send('\x1b[5~');t.collect(.2)
            t.send('\x1b[F');t.collect(.2)
            if size[1]>8:
                assert red(),('reader error lost red color',t.vt.text())
                assert error_style()==original,('reader style differs',original,error_style())
            t.send('\x1b');t.collect(.2)
            if size[1]>8:
                assert red(),('restored error lost red color',t.vt.text())
                assert error_style()==original,('restored style differs',original,error_style())
            print('PASS error style before/during/after scroll:',size)
        finally:t.close()
    server.shutdown()
