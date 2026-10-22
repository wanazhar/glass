#!/usr/bin/env python3
"""Persistent teammates against actual provider wire and actual binary/PTY."""
import http.server, json, os, subprocess, tempfile, threading, time
from pathlib import Path
from pty_vt import Terminal
from core_binary import core_binary_path
BIN=Path(os.environ.get('GLASS_CORE_TEST_BINARY',core_binary_path('debug'))).resolve()
class Server(http.server.ThreadingHTTPServer): daemon_threads=True
class Endpoint(http.server.BaseHTTPRequestHandler):
    def log_message(self,*_): pass
    def do_POST(self):
        s=self.server;b=json.loads(self.rfile.read(int(self.headers['Content-Length'])));s.requests.append(b)
        messages=b.get('messages',[]);system=messages[0].get('content','') if messages else ''
        worker='You are a subagent' in system;history=json.dumps(messages,ensure_ascii=False)
        calls=[];text='MAIN_DONE'
        if worker:
            if 'HOLD' in history:time.sleep(3)
            if 'CHECK_READ' in history and not any(m.get('role')=='tool' for m in messages):calls=[('inspect',{'action':'read','path':'evidence.txt'})]
            else:text='REVIEW_FOLLOWUP' if 'EOF_FOLLOWUP' in history else 'REVIEW_COMPLETE'
        elif 'MAIN_HOLD' in history and not s.headless:time.sleep(2)
        elif s.headless:
            commands=['"$GC_BIN" agent create review CHECK_READ','"$GC_BIN" agent wait review 10','"$GC_BIN" agent send review EOF_FOLLOWUP','"$GC_BIN" agent wait review 10','"$GC_BIN" agent transcript review']
            step=s.main_step;s.main_step+=1
            if step<len(commands):calls=[('bash',{'command':commands[step]})]
        try:
            self.send_response(200);self.send_header('Content-Type','text/event-stream');self.end_headers()
            def emit(v):self.wfile.write(('data: '+json.dumps(v)+'\n\n').encode());self.wfile.flush()
            if calls:emit({'choices':[{'delta':{'tool_calls':[{'index':i,'id':f'call-{len(s.requests)}-{i}','function':{'name':name,'arguments':json.dumps(args)}} for i,(name,args) in enumerate(calls)]}}]})
            else:emit({'choices':[{'delta':{'content':text}}]})
            emit({'choices':[],'usage':{'prompt_tokens':100,'completion_tokens':20}})
            self.wfile.write(b'data: [DONE]\n\n');self.wfile.flush()
        except (BrokenPipeError,ConnectionResetError):pass

def setup(root,headless=False,priced=False):
    cfg=root/'c/glass-core';cfg.mkdir(parents=True);(root/'evidence.txt').write_text('parser evidence\n')
    server=Server(('127.0.0.1',0),Endpoint);server.requests=[];server.headless=headless;server.main_step=0
    threading.Thread(target=server.serve_forever,daemon=True).start()
    (cfg/'config.toml').write_text(f'provider="openai"\nmodel="mock"\napi_key_env=""\nbase_url="http://127.0.0.1:{server.server_port}/v1"\ntrust="always"\n')
    if priced:
        with (cfg/'config.toml').open('a') as f:f.write('[pricing]\ninput=1.0\ncache_read=0.1\ncache_write=1.0\noutput=2.0\n')
    return server,dict(os.environ,XDG_CONFIG_HOME=str(root/'c'),XDG_DATA_HOME=str(root/'d'))
def ctl(root,env,owner,*args,ok=True):
    p=subprocess.run([str(BIN),'agent','--session',owner,*args],cwd=root,env=env,text=True,capture_output=True,timeout=15)
    assert (p.returncode==0)==ok,(args,p.stdout,p.stderr)
    return json.loads(p.stdout) if ok else p.stderr

def owner_id(root):
    deadline=time.monotonic()+5
    while time.monotonic()<deadline:
        paths=list((root/'d/glass-core/agents').glob('*/control.sock'))
        if paths:return paths[0].parent.name
        time.sleep(.02)
    raise AssertionError('no hub socket')

def headless(priced=False):
    with tempfile.TemporaryDirectory(prefix='gc-ag-') as tmp:
        root=Path(tmp);server,env=setup(root,True,priced)
        try:
            p=subprocess.run([str(BIN),'run','--json','review the parser using persistent teammates'],cwd=root,env=env,text=True,capture_output=True,timeout=40)
            assert p.returncode==0,(p.stdout,p.stderr);out=json.loads(p.stdout);assert out['text']=='MAIN_DONE',out
            agents=list((root/'d/glass-core/agents').glob('*/*.json'));assert len(agents)==1,agents
            info=json.loads(agents[0].read_text());assert info['turns']==2 and info['read_only'] and info['status']=='idle',info
            assert info['input']==300 and info['output']==60,info
            assert (abs(info['cost_usd']-.00042)<1e-9 if priced else info['cost_usd'] is None),info
            workers=[b for b in server.requests if 'You are a subagent' in b['messages'][0]['content']]
            assert len(workers)==3,workers
            assert all([t['function']['name'] for t in b['tools']]==['inspect'] for b in workers)
            assert 'REVIEW_COMPLETE' in json.dumps(workers[-1]) and 'EOF_FOLLOWUP' in json.dumps(workers[-1])
            roots=[b for b in server.requests if b not in workers]
            assert 'Teammate review' in json.dumps(roots[-1])
            assert all(len(b['tools'])==5 for b in roots)
            print(f'PASS headless/non-Git priced={priced}: shell control, inspection, reused context, summaries, exact/unknown cost, five core tools')
        finally:server.shutdown();server.server_close()

def pty_case(size):
    with tempfile.TemporaryDirectory(prefix='gc-ag-') as tmp:
        root=Path(tmp);server,env=setup(root);t=Terminal(root,env,size)
        try:
            t.idle();owner=owner_id(root)
            # Invalid requests must not create sessions/worktrees.
            assert 'token budget' in ctl(root,env,owner,'create','bad','review','--tokens','1',ok=False)
            assert not list((root/'d/glass-core/agents'/owner/'sessions').glob('*.jsonl'))
            a=ctl(root,env,owner,'create','review','CHECK_READ');assert a['read_only']
            ctl(root,env,owner,'wait','review','10');t.collect(.2)
            t.send('\x1ba');t.collect(.25);assert 'Agents' in t.vt.text(),t.vt.text()
            t.send('\t');t.collect(.15);assert 'Interactions' in t.vt.text(),t.vt.text()
            t.send('\x1b[H');t.collect(.1);assert 'human' in t.vt.text(),t.vt.text()
            t.send('\t');t.collect(.15);t.send('\x1b[H');t.collect(.15)
            assert 'Transcript' in t.vt.text(),t.vt.text()
            t.send('\x1b[F');t.collect(.15);assert 'REVIEW_COMPLETE' in t.vt.text(),t.vt.text()
            assert not t.collect(.35),'unchanged transcript repainted'
            t.send('\x1b');t.collect(.2);assert 'Transcript' not in t.vt.text(),t.vt.text()
            ctl(root,env,owner,'send','review','EOF_FOLLOWUP');info=ctl(root,env,owner,'wait','review','10')['agent'];assert info['turns']==2 and info['result']=='REVIEW_FOLLOWUP',info
            # Slow provider header must remain interruptible and session restartable.
            ctl(root,env,owner,'create','slow','HOLD');ctl(root,env,owner,'create','second','HOLD')
            assert 'two agents' in ctl(root,env,owner,'create','third','review',ok=False)
            t.send('\x1ba');t.collect(.2);assert 'Agents' in t.vt.text()
            start=time.monotonic();ctl(root,env,owner,'interrupt','slow');parked=ctl(root,env,owner,'wait','slow','2')['agent'];assert parked['status']=='parked' and time.monotonic()-start<2.5,parked
            ctl(root,env,owner,'interrupt','second');ctl(root,env,owner,'wait','second','2')
            t.send('\x1b');t.collect(.15)
            # No unsolicited model turn for a completion or human intervention.
            count=len(server.requests);t.collect(.5);assert len(server.requests)==count
            for mode in ['interactions','transcript']:
                out=ctl(root,env,owner,mode,*(['review'] if mode=='transcript' else []));assert out['text']
            ctl(root,env,owner,'archive','second');assert 'archived' in ctl(root,env,owner,'send','second','test',ok=False)
            # Private audit data.
            directory=root/'d/glass-core/agents'/owner
            assert directory.stat().st_mode&0o777==0o700
            assert (directory/'interactions.jsonl').stat().st_mode&0o777==0o600
            t.close();assert ctl(root,env,owner,'list')[0]['offline'];assert ctl(root,env,owner,'transcript','review')['text'];assert 'not running' in ctl(root,env,owner,'send','review','test',ok=False)
            t=Terminal(root,env,size,args=('resume',owner));t.idle();assert owner_id(root)==owner
            infos=ctl(root,env,owner,'list');assert len(infos)==3
            ctl(root,env,owner,'resume','review','EOF_FOLLOWUP');info=ctl(root,env,owner,'wait','review','10')['agent'];assert info['turns']==3,info
            # Main composer and palette survive modal/scroll transitions.
            t.send('/agents\r');t.collect(.2);assert 'Agents' in t.vt.text(),t.vt.text();t.send('\x1b');t.collect(.15)
            t.send('MAIN_HOLD\r');t.collect(.2);t.send('\x1ba');t.collect(.3);assert 'Agents' in t.vt.text(),t.vt.text();t.send('\x1b');t.collect(.15);t.wait('MAIN_DONE',10);t.collect(.2)
            t.send('hello\r');t.wait('MAIN_DONE',10);t.collect(.2);assert t.p.poll() is None
            print(f'PASS PTY {size}: live hub, transcript, Esc, follow-up, interrupt, restart, controls, concurrency, privacy')
        finally:t.close();server.shutdown();server.server_close()
if __name__=='__main__':
    headless()
    headless(priced=True)
    for size in [(80,24),(40,12),(20,8)]:pty_case(size)
