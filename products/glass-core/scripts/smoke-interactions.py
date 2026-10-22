#!/usr/bin/env python3
"""Paused native questions and exact-call approvals with an isolated SSE fixture."""
import http.server
import json
import os
from pathlib import Path
import subprocess
import tempfile
import threading
from pty_vt import Terminal
from core_binary import core_binary_path

binary=Path(os.environ.get('GLASS_CORE_TEST_BINARY',str(core_binary_path('release')))).resolve()
os.environ['GLASS_CORE_TEST_BINARY']=str(binary)
class Endpoint(http.server.BaseHTTPRequestHandler):
    def log_message(self,*_): pass
    def do_POST(self):
        body=json.loads(self.rfile.read(int(self.headers['Content-Length'])))
        self.server.requests.append(body)
        results=[m for m in body['messages'] if m['role']=='tool']
        mode=self.server.mode
        if not results or (mode=='repeat' and len(results)==1):
            if mode in ('question','free','multi','batch'):
                assert 'ask_user_question' in [tool['function']['name'] for tool in body['tools']]
                name='ask_user_question'; args={'questions':[{'header':'Choose storage','question':'Which storage should we use?','options':[{'label':'SQLite','description':'A local database'},{'label':'Files','description':'Plain files'}]}]}
                if mode=='free': args['questions'][0]['options']=[]
                if mode=='multi': args['questions'][0]['multiSelect']=True
            else:
                name='write' if mode=='native' else 'bash'
                args={'permission':'require_escalated','justification':'Fixture needs one outside-workspace write'}
                if name=='write':args.update(path=str(self.server.target),expected=None,text='approved')
                else:args['command']="printf approved > '"+str(self.server.target)+"'"
            delta={'tool_calls':[{'index':0,'id':f'interaction-{len(results)+1}','function':{'name':name,'arguments':json.dumps(args)}}]}
            if mode=='batch':delta['tool_calls'].append({'index':1,'id':'premature','function':{'name':'bash','arguments':json.dumps({'command':"printf premature > '"+str(self.server.workspace/'premature')+"'"})}})
            if mode=='batch':
                delta['tool_calls'].reverse()
                for index,call in enumerate(delta['tool_calls']):call['index']=index
        else:
            if mode in ('question','free','multi','batch'):
                answer=json.loads(results[0]['content'])
                expected={'question':['SQLite'],'free':['PostgreSQL'],'multi':['SQLite','Files'],'batch':['SQLite']}[mode]
                assert answer['status']=='answered' and answer['answers'][0]['answers']==expected,answer
                if mode=='batch':assert 'Not executed' in results[-1]['content'] and not (self.server.workspace/'premature').exists(),results
            self.server.result=results[-1]['content']
            delta={'content':'INTERACTION_DONE'}
        self.send_response(200);self.send_header('Content-Type','text/event-stream');self.end_headers()
        self.wfile.write(('data: '+json.dumps({'choices':[{'delta':delta}]})+'\n\ndata: [DONE]\n\n').encode())

with tempfile.TemporaryDirectory(prefix='gc-interactions-') as tmp:
    root=Path(tmp);workspace=root/'workspace';workspace.mkdir()
    config=root/'config/glass-core';config.mkdir(parents=True)
    server=http.server.ThreadingHTTPServer(('127.0.0.1',0),Endpoint)
    server.workspace=workspace
    server.daemon_threads=True
    threading.Thread(target=server.serve_forever,daemon=True).start()
    (config/'config.toml').write_text(f'provider="openai"\nmodel="fixture"\nbase_url="http://127.0.0.1:{server.server_port}/v1"\napi_key_env=""\npermission_profile="workspace-edit"\ntrust="always"\nadaptive_pacing=false\n')
    env=dict(os.environ,XDG_CONFIG_HOME=str(root/'config'),XDG_DATA_HOME=str(root/'data'))
    def start(mode,size=(40,24),args=()):
        server.mode=mode;server.requests=[];server.result=None
        server.target=root/(mode+'-outside.txt')
        server.target.unlink(missing_ok=True)
        terminal=Terminal(workspace,env,size,args=args)
        terminal.idle();terminal.send('Run isolated interaction fixture\r')
        return terminal
    try:
        for size in ((80,30),(40,24),(20,8)):
            terminal=start('question',size)
            try:
                terminal.visible('Choose storage' if size[0]>20 else 'storage')
                terminal.collect(.4)
                assert len(server.requests)==1,'Provider ran while waiting for an answer'
                terminal.send('\r');terminal.wait('INTERACTION_DONE')
            finally:terminal.close()
        print('PASS paused native choices at three sizes',flush=True)
        for mode in ('free','multi','batch'):
            terminal=start(mode)
            try:
                terminal.visible('Choose storage')
                terminal.send('\rPostgreSQL\r' if mode=='free' else ('\r' if mode=='batch' else '\r\x1b[B\r\x1b[B\x1b[B\x1b[B\r'))
                terminal.wait('INTERACTION_DONE')
            finally:terminal.close()
        terminal=start('question')
        try:
            terminal.visible('Choose storage');terminal.send('\x1b');terminal.collect(.7)
            assert len(server.requests)==1,'Cancellation requested another model answer'
            records=[json.loads(line) for path in (root/'data/glass-core').glob('*.jsonl') for line in path.read_text().splitlines()]
            assert any(isinstance(value,str) and '"status":"cancelled"' in value for record in records if isinstance(record,dict) for value in record.values())
        finally:terminal.close()
        print('PASS written answers, multi-select and cancellation without invented answers',flush=True)

        for mode,allow,size in (('bash',False,(40,24)),('bash',True,(40,24)),('native',True,(40,24)),('native',True,(20,8))):
            terminal=start(mode,size)
            try:
                terminal.visible('Approval required')
                terminal.collect(.4)
                assert len(server.requests)==1 and not server.target.exists(),'Execution before approval'
                if mode=='bash' and allow:
                    terminal.resize(40,3);terminal.collect(.2);terminal.send('a');terminal.collect(.2)
                    assert len(server.requests)==1 and not server.target.exists(),'Approved an invisible operation'
                    terminal.resize(*size);terminal.collect(.2)
                terminal.send('a' if allow else '\r')
                terminal.wait('INTERACTION_DONE')
                assert server.target.exists()==allow,(server.result,allow)
                if allow:assert server.target.read_text()=='approved'
                else:assert 'User denied' in server.result,server.result
            finally:terminal.close()
        print('PASS paused shell/native grants and default denial',flush=True)
        terminal=start('repeat')
        try:
            terminal.visible('Approval required')
            terminal.send('a')
            terminal.collect(.7)
            terminal.visible('Approval required')
            assert len(server.requests)==2 and server.target.read_text()=='approved'
            terminal.send('\r')
            terminal.wait('INTERACTION_DONE')
            assert 'User denied' in server.result
        finally:terminal.close()
        print('PASS allow-once expires before an identical later command',flush=True)
        terminal=start('bash',args=('--yolo',))
        try:
            terminal.wait('INTERACTION_DONE')
            assert server.target.read_text()=='approved'
        finally:terminal.close()
        print('PASS yolo executes without an approval modal',flush=True)
        server.mode='bash';server.requests=[];server.target=root/'headless-outside.txt'
        result=subprocess.run([str(binary),'--restricted','run','--json','Run headless fixture'],cwd=workspace,env=env,capture_output=True,text=True,timeout=20)
        assert result.returncode==0,result.stderr
        assert not server.target.exists() and 'no interactive human' in server.result,server.result
        assert 'ask_user_question' not in [tool['function']['name'] for tool in server.requests[0]['tools']]
        print('PASS headless denies escalation and advertises no fake human',flush=True)
    finally:server.shutdown()
