#!/usr/bin/env python3
"""Offline nested delegation: completion resumes each parent without model polling."""
import http.server, importlib.util, json, os, re, subprocess, sys, tempfile, threading, time, urllib.request, urllib.error
from pathlib import Path
sys.path.insert(0,str(Path(__file__).parent))
from core_binary import core_binary_path
BIN=Path(os.environ.get('GLASS_CORE_TEST_BINARY',core_binary_path('debug'))).resolve()
class Endpoint(http.server.BaseHTTPRequestHandler):
    def log_message(self,*_):pass
    def do_POST(self):
        raw=self.rfile.read(int(self.headers['Content-Length']))
        if not raw:return  # Cancellation may close an admitted transport before its body.
        body=json.loads(raw);self.server.requests.append(body)
        messages=body['messages'];worker='You are a subagent' in str(messages[0]['content']);users='\n'.join(str(m.get('content','')) for m in messages if m['role']=='user');results=[m for m in messages if m['role']=='tool']
        names={t['function']['name'] for t in body.get('tools',[])};assert 'teammate' in names
        if getattr(self.server,'actor_probe',False):
            if not results:
                delta={'tool_calls':[{'index':0,'id':'resume-attempt','function':{'name':'bash','arguments':json.dumps({'command':'"$GC_BIN" agent --session "$GC_AGENT_OWNER" workflow resume'})}}]}
            else:
                assert 'Only the human can resume' in str(results[-1]),results[-1]
                delta={'content':'RESUME_REFUSED'}
        elif worker and 'COORDINATOR_TASK' not in users:
            if getattr(self.server,'inspection_probe',False) and len(results)<2:
                action='git_status' if not results else 'system'
                delta={'tool_calls':[{'index':0,'id':'inspect-'+action,'function':{'name':'inspect','arguments':json.dumps({'action':action})}}]}
            else:
                if getattr(self.server,'inspection_probe',False):
                    assert json.loads(results[0]['content'])['verified'] is True
                    assert json.loads(results[1]['content'])['unix_time_seconds']>0
                    self.server.inspection_completed=True
                if getattr(self.server,'sibling_probe',False):self.server.leaf_barrier.wait(timeout=10)
                time.sleep(getattr(self.server,'leaf_delay',.35));delta={'content':'LEAF_DONE'}
        elif worker:
            if '[Teammate completion follow-up]' in users:delta={'content':'COORDINATOR_DONE'}
            elif not results:
                calls=[{'index':0,'id':'leaf-create','function':{'name':'teammate','arguments':json.dumps({'action':'create','name':'leaf','text':'LEAF_TASK'})}}]
                if getattr(self.server,'sibling_probe',False):calls.append({'index':1,'id':'leaf-create-two','function':{'name':'teammate','arguments':json.dumps({'action':'create','name':'leaf_two','text':'LEAF_TASK_TWO'})}})
                delta={'tool_calls':calls}
            else:delta={'content':'WAITING_FOR_CHILD'}
        elif '[Teammate completion follow-up]' in users:
            if getattr(self.server,'limit_probe',False):
                assert 'status=blocked' in users and 'Recovery:' in users and 'context_allowance' in users
                delta={'content':'BUDGET_BLOCKED'}
            elif getattr(self.server,'hook_probe',False) and not any(m.get('tool_call_id')=='auto-write' for m in results):
                delta={'tool_calls':[{'index':0,'id':'auto-write','function':{'name':'write','arguments':json.dumps({'path':'forbidden.txt','text':'must not be written'})}}]}
            else:
                assert 'COORDINATOR_DONE' in users
                if getattr(self.server,'hook_probe',False):assert 'HOOK_BLOCKED' in str(results[-1]),results[-1]
                delta={'content':'ROOT_DONE'}
        elif not results:delta={'tool_calls':[{'index':0,'id':'coordinator-create','function':{'name':'teammate','arguments':json.dumps(dict(action='create',name='coordinator',text='COORDINATOR_TASK',**({'tokens':1024} if getattr(self.server,'limit_probe',False) else {})))}}]}
        else:delta={'content':'DELEGATED_PENDING'}
        payload=('data: '+json.dumps({'choices':[{'delta':delta}]})+'\n\ndata: [DONE]\n\n').encode();self.send_response(200);self.send_header('Content-Type','text/event-stream');self.send_header('Content-Length',str(len(payload)));self.end_headers()
        try:self.wfile.write(payload)
        except (BrokenPipeError,ConnectionResetError):pass
def setup(root):
    server=http.server.ThreadingHTTPServer(('127.0.0.1',0),Endpoint);server.requests=[];threading.Thread(target=server.serve_forever,daemon=True).start();cfg=root/'config/glass-core';cfg.mkdir(parents=True);(cfg/'config.toml').write_text(f'provider="openai"\nmodel="mock"\napi_key_env=""\nbase_url="http://127.0.0.1:{server.server_port}/v1"\ntrust="always"\n');env=dict(os.environ,XDG_CONFIG_HOME=str(root/'config'),XDG_DATA_HOME=str(root/'data'));return server,env

def main():
    with tempfile.TemporaryDirectory(prefix='gc-continuations-') as tmp:
        root=Path(tmp);server,env=setup(root)
        try:
            p=subprocess.run([str(BIN),'run','--json','Finish a bounded nested task.'],cwd=root,env=env,text=True,capture_output=True,timeout=40)
            assert p.returncode==0,(p.stdout,p.stderr)
            if json.loads(p.stdout)['text']!='ROOT_DONE':
                print('Debug agent states:',[(v.name,v.read_text()) for v in (root/'data/glass-core/agents').glob('*/*.json')]);print('Mock requests:',len(server.requests))
            assert json.loads(p.stdout)['text']=='ROOT_DONE',p.stdout
            verify(root,server)
            print('PASS headless: root → coordinator → leaf → coordinator → root; no model status polling')
        finally:server.shutdown();server.server_close()
    with tempfile.TemporaryDirectory(prefix='gc-continuations-pty-') as tmp:
        from pty_vt import Terminal
        root=Path(tmp);server,env=setup(root);t=Terminal(root,env,(80,24))
        try:
            t.idle();t.send('Finish a bounded nested task.\r');t.wait('ROOT_DONE',20);t.collect(.5);verify(root,server);count=len(server.requests);t.collect(1);assert len(server.requests)==count
            print('PASS TUI: automatic completion continuation; idle consumes no further model requests')
        finally:t.close();server.shutdown();server.server_close()

    with tempfile.TemporaryDirectory(prefix='gc-continuations-runtime-') as tmp:
        root=Path(tmp);server,env=setup(root)
        spec=importlib.util.spec_from_file_location('runtime_host',Path(__file__).resolve().parents[1]/'examples/runtime_host.py');host=importlib.util.module_from_spec(spec);spec.loader.exec_module(host)
        client=host.RuntimeClient(BIN,root,env)
        try:
            call=lambda name,params:client.wait(client.request(name,params))
            call('initialize',{'protocolVersion':2,'requiredCapabilities':['mixedTools'],'storageDir':str(root/'sessions')})
            session=call('session/new',{'cwd':str(root),'toolMode':'mixed','tools':[],'instructions':'Finish delegated work using bounded teammate tools.'})
            events=[]
            client.wait(client.request('turn/prompt',{**session,'text':'Finish a bounded nested task.'}),event=events.append)
            done=False;deadline=time.monotonic()+20
            while time.monotonic()<deadline:
                message=client.receive(timeout=5)
                if message.get('method')=='runtime/event':
                    params=message['params'];events.append(params)
                    if params['type']=='text' and 'ROOT_DONE' in params['data'].get('text',''):done=True
                    if done and params['type']=='settled':break
            assert done,events
            roster=call('runtime/subagents/list',session)['children'];assert len(roster)==2,roster
            assert any(c.get('depth')==2 for c in roster),roster
            child=roster[-1]
            page=call('runtime/subagents/entries',{**session,'parentCallId':child['parentCallId'],'taskId':child['taskId'],'childSessionId':child['childSessionId']})
            assert page['childSessionId']==child['childSessionId']
            assert all(c['readOnly'] is True for c in roster)
            try:
                call('runtime/subagents/entries',{**session,'parentCallId':'foreign-call','taskId':child['taskId'],'childSessionId':child['childSessionId']})
                raise AssertionError('foreign parent call must be rejected')
            except RuntimeError as error:
                assert 'not linked' in str(error),str(error)
            verify(root,server)
            assert any(e['type']=='subagent' and e['data'].get('depth')==2 for e in events)
            print('PASS mixed runtime: automatic parent wake and nested read-only child streams without client polling')
        finally:client.close();server.shutdown();server.server_close()

    with tempfile.TemporaryDirectory(prefix='gc-continuations-serve-') as tmp:
        root=Path(tmp);server,env=setup(root)
        spec=importlib.util.spec_from_file_location('serve_smoke',Path(__file__).with_name('smoke-serve.py'));serve=importlib.util.module_from_spec(spec);spec.loader.exec_module(serve)
        path=root/'serve.sock';process=subprocess.Popen([str(BIN),'serve','--socket',str(path)],cwd=root,env=env,stdout=subprocess.DEVNULL,stderr=subprocess.PIPE)
        client=None;gateway=False
        try:
            deadline=time.monotonic()+10
            while not path.exists():
                assert process.poll() is None,process.stderr.read().decode()
                assert time.monotonic()<deadline
                time.sleep(.05)
            client=serve.Client(path);client.send({'t':'prompt','text':'Finish a bounded nested task.'})
            text,_=client.until_done();assert 'DELEGATED_PENDING' in text,text
            text,_=client.until_done();assert 'ROOT_DONE' in text,text
            verify(root,server);count=len(server.requests);time.sleep(.5);assert len(server.requests)==count
            print('PASS serve: root resumes from child completion without a new client prompt')
            env['GC_WEB_ASSETS']=str(Path(__file__).resolve().parents[1]/'web/dist')
            launched=subprocess.run([str(BIN),'connect','--local'],cwd=root,env=env,text=True,capture_output=True,timeout=15)
            assert launched.returncode==0,launched.stderr
            gateway=True
            private=re.search(r'http://127\.0\.0\.1:\d+/#\w+',launched.stdout)
            assert private,'No loopback Connect link'
            base,token=private.group(0).split('/#')
            owner=next((root/'data/glass-core/agents').iterdir()).name
            url=base+'/api/agent-events?id='+owner
            try:
                urllib.request.urlopen(url,timeout=5)
                raise AssertionError('Anonymous agent events must be denied')
            except urllib.error.HTTPError as error:assert error.code==403
            with urllib.request.urlopen(urllib.request.Request(url,headers={'Authorization':'Bearer '+token}),timeout=5) as response:
                assert response.headers['Content-Type']=='text/event-stream'
                assert response.readline().startswith(b'data: {"revision":')
            print('PASS Connect: authenticated real agent revision stream; anonymous access denied')
        finally:
            if gateway:subprocess.run([str(BIN),'connect','stop'],cwd=root,env=env,capture_output=True,timeout=10)
            if client:client.close()
            process.terminate();process.wait(timeout=10);server.shutdown();server.server_close()

    with tempfile.TemporaryDirectory(prefix='gc-continuations-stop-') as tmp:
        from pty_vt import Terminal
        root=Path(tmp);server,env=setup(root);server.leaf_delay=2;t=Terminal(root,env,(80,24))
        try:
            t.idle();t.send('Finish a bounded nested task.\r');t.wait('DELEGATED_PENDING',10);wait_leaf(root,server)
            owner=next((root/'data/glass-core/agents').iterdir()).name
            stopped=subprocess.run([str(BIN),'agent','--session',owner,'workflow','stop'],cwd=root,env=env,text=True,capture_output=True,timeout=10)
            assert stopped.returncode==0,stopped.stderr
            count=len(server.requests);t.collect(2.5);assert len(server.requests)==count
            assert 'ROOT_DONE' not in t.vt.text()
            assert json.loads((root/'data/glass-core/agents'/owner/'workflow.json').read_text())['stopped']
            print('PASS interruption: stopping an active nested tree prevents late completion wakeups')
        finally:t.close();server.shutdown();server.server_close()

    for matcher in ['teammate','write']:
        with tempfile.TemporaryDirectory(prefix='gc-continuations-hook-') as tmp:
            root=Path(tmp);server,env=setup(root);server.hook_probe=matcher=='write'
            with (root/'config/glass-core/config.toml').open('a') as config:
                config.write('[hooks.pre_tool_use]\ncommand="printf HOOK_BLOCKED; exit 2"\nmatcher="'+matcher+'"\n')
            try:
                result=subprocess.run([str(BIN),'run','--json','Finish a bounded nested task.'],cwd=root,env=env,text=True,capture_output=True,timeout=40)
                assert result.returncode==0,(result.stdout,result.stderr)
                assert not (root/'forbidden.txt').exists()
                if matcher=='teammate':
                    assert len(server.requests)==2,len(server.requests)
                    assert 'HOOK_BLOCKED' in str(server.requests[-1]['messages'])
                else:
                    assert json.loads(result.stdout)['text']=='ROOT_DONE',result.stdout
                    assert len(server.requests)==8,len(server.requests)
                print('PASS pre-tool hook: '+('native delegation honors the hook gate' if matcher=='teammate' else 'automatic parent follow-up retains the hook gate'))
            finally:server.shutdown();server.server_close()

    with tempfile.TemporaryDirectory(prefix='gc-continuations-actor-') as tmp:
        root=Path(tmp);server,env=setup(root);server.actor_probe=True
        try:
            result=subprocess.run([str(BIN),'run','--json','Check scoped control.'],cwd=root,env=env,text=True,capture_output=True,timeout=20)
            assert result.returncode==0,(result.stdout,result.stderr)
            assert json.loads(result.stdout)['text']=='RESUME_REFUSED'
            assert len(server.requests)==2
            print('PASS actor scope: model shell cannot become human through --session')
        finally:server.shutdown();server.server_close()

    with tempfile.TemporaryDirectory(prefix='gc-continuations-inspection-') as tmp:
        root=Path(tmp);server,env=setup(root);server.inspection_probe=True
        subprocess.run(['git','init','-q'],cwd=root,check=True);(root/'evidence.txt').write_text('fixture')
        try:
            result=subprocess.run([str(BIN),'run','--json','Verify nested read-only inspection.'],cwd=root,env=env,text=True,capture_output=True,timeout=40)
            assert result.returncode==0,(result.stdout,result.stderr)
            assert json.loads(result.stdout)['text']=='ROOT_DONE';verify(root,server)
            print('PASS nested read-only inspection: verified Git status and host time without bash or write permission')
        finally:server.shutdown();server.server_close()

    with tempfile.TemporaryDirectory(prefix='gc-continuations-restart-') as tmp:
        from pty_vt import Terminal
        root=Path(tmp);server,env=setup(root);server.leaf_delay=2;server.inspection_probe=True
        subprocess.run(['git','init','-q'],cwd=root,check=True)
        t=Terminal(root,env,(80,24));resumed=None
        try:
            t.idle();t.send('Finish a bounded nested task.\r');t.wait('DELEGATED_PENDING',10);wait_leaf(root,server)
            owner=next((root/'data/glass-core/agents').iterdir()).name
            leaf=next(json.loads(p.read_text()) for p in (root/'data/glass-core/agents'/owner).glob('*.json') if p.name!='workflow.json' and json.loads(p.read_text()).get('name')=='leaf')
            child_path=root/'data/glass-core/agents'/owner/'sessions'/(leaf['session']+'.jsonl');child_before=child_path.read_bytes()
            assert sum(json.loads(line).get('k')=='tool' for line in child_before.splitlines())==2
            t.p.kill();t.p.wait(timeout=5);t.close();count=len(server.requests)
            resumed=Terminal(root,env,(80,24),args=('resume',owner));resumed.idle();resumed.collect(2.5)
            assert len(server.requests)==count
            state=subprocess.run([str(BIN),'agent','--session',owner,'workflow','inspect'],cwd=root,env=env,text=True,capture_output=True,timeout=10)
            assert state.returncode==0,state.stderr
            assert json.loads(state.stdout)['stopped'] is True
            assert 'ROOT_DONE' not in resumed.vt.text()
            assert child_path.read_bytes()==child_before
            print('PASS crash restart: uncertain nested work is parked; no request or tool replay before human resume')
        finally:
            if resumed:resumed.close()
            if t.p.poll() is None:t.close()
            server.shutdown();server.server_close()

    for mode in ['sibling_probe','limit_probe']:
        with tempfile.TemporaryDirectory(prefix='gc-continuations-'+mode+'-') as tmp:
            root=Path(tmp);server,env=setup(root);setattr(server,mode,True)
            if mode=='sibling_probe':server.leaf_barrier=threading.Barrier(2)
            try:
                result=subprocess.run([str(BIN),'run','--json','Verify bounded delegation.'],cwd=root,env=env,text=True,capture_output=True,timeout=40)
                assert result.returncode==0,(result.stdout,result.stderr)
                infos=[json.loads(p.read_text()) for p in (root/'data/glass-core/agents').glob('*/*.json') if p.name!='workflow.json']
                if mode=='sibling_probe':
                    assert json.loads(result.stdout)['text']=='ROOT_DONE'
                    assert len(infos)==3 and sum(i['depth']==2 for i in infos)==2
                    assert next(i for i in infos if i['name']=='coordinator')['auto_wakes']==1
                    assert len(server.requests)==8,len(server.requests)
                    print('PASS simultaneous nested siblings: two real child requests coalesce into one coordinator continuation')
                else:
                    assert json.loads(result.stdout)['text']=='BUDGET_BLOCKED'
                    assert len(server.requests)==3 and not any('You are a subagent' in str(b['messages'][0]['content']) for b in server.requests)
                    assert infos[0]['status']=='blocked' and infos[0]['token_limit']==1024
                    print('PASS exhausted budget: no worker provider request, exact blocker delivered to root, no automatic increase')
            finally:server.shutdown();server.server_close()

def wait_leaf(root,server):
    deadline=time.monotonic()+5
    while time.monotonic()<deadline:
        for path in (root/'data/glass-core/agents').glob('*/*.json'):
            info=json.loads(path.read_text())
            if info.get('name')=='leaf' and info.get('status')=='working' and (not getattr(server,'inspection_probe',False) or getattr(server,'inspection_completed',False)) and any(any(m['role']=='user' and 'LEAF_TASK' in str(m.get('content','')) for m in b['messages']) for b in server.requests):return
        time.sleep(.02)
    raise AssertionError('Nested leaf did not start before interruption/restart')

def verify(root,server):
    infos=[]
    for path in (root/'data/glass-core/agents').glob('*/*.json'):
        value=json.loads(path.read_text())
        if 'id' in value and 'session' in value:infos.append(value)
    assert len(infos)==2,infos
    coordinator=next(i for i in infos if i['name']=='coordinator');leaf=next(i for i in infos if i['name']=='leaf')
    assert leaf['parent_id']==coordinator['id'] and leaf['depth']==2
    assert coordinator['auto_wakes']==1 and coordinator['result']=='COORDINATOR_DONE'
    assert len(server.requests)==7+(2 if getattr(server,"inspection_probe",False) else 0),len(server.requests)
    flow=json.loads(next((root/'data/glass-core/agents').glob('*/workflow.json')).read_text());assert flow['wakes']==2 and flow['root_spent']>0 and flow['root_reserved']==0,flow
    assert not any(t['function']['name'] in (('bash',) if getattr(server,'inspection_probe',False) else ('bash','inspect')) for b in server.requests for m in b['messages'] if m['role']=='assistant' for t in m.get('tool_calls',[]))
if __name__=='__main__':main()
