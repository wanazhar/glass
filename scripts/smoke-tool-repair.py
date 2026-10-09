#!/usr/bin/env python3
"""Offline real-provider-wire tests for bounded tool repair, including release PTYs."""
import http.server, json, os, subprocess, tempfile, threading, time
from pathlib import Path
from pty_vt import Terminal
BIN = Path(os.environ.get('GLASS_CORE_TEST_BINARY',Path(__file__).resolve().parents[1]/'target/debug/glass-core')).resolve()
class Server(http.server.ThreadingHTTPServer):
    daemon_threads=True
class Endpoint(http.server.BaseHTTPRequestHandler):
    def log_message(self,*_): pass
    def do_POST(self):
        server=self.server; body=json.loads(self.rfile.read(int(self.headers['Content-Length']))); server.requests.append(body)
        step=len(server.requests)-1; mode=server.mode; wire=server.wire
        if mode=='cancel' and step==1:time.sleep(1)
        if step:
            # Every outgoing rejected argument must be protocol-safe, while
            # validation feedback remains visible to the model.
            for message in body.get('messages',body.get('input',[])):
                for call in message.get('tool_calls',[]): assert isinstance(json.loads(call['function']['arguments']),dict)
                if message.get('type')=='function_call': assert isinstance(json.loads(message['arguments']),dict)
                if wire=='anthropic':
                    for block in message.get('content',[]) if isinstance(message.get('content'),list) else []:
                        if block.get('type')=='tool_use': assert isinstance(block['input'],dict)
                        if block.get('type')=='tool_result' and 'validation failed' in json.dumps(block): assert block.get('is_error') is True
        followup=any(m.get('role')=='user' and 'follow-up' in json.dumps(m.get('content','')) for m in body.get('messages',body.get('input',[])))
        command="printf x >> executed.txt; printf REPAIR_TOOL_OK"
        calls=[]; text='FOLLOWUP_DONE' if followup else 'REPAIR_FINISHED'
        if mode=='after-completed' and step==0: calls=[('previous','bash',json.dumps({'command':'printf p >> previous.txt; printf PREVIOUS_OK'}))]
        elif mode=='after-completed' and step==1: calls=[('invalid','bash','{')]
        elif mode=='after-completed' and step==2: calls=[('corrected','bash',json.dumps({'command':command}))]
        elif mode=='execution' and step==0: calls=[('bad','bash','{"command":"exit 7"}')]
        elif step==0 or mode=='exhaust':
            args={'json':'{"command":"touch forbidden.txt"','type':'{"command":12}', 'missing':'{}', 'state':'{"items":[12]}', 'mixed':'{"command":false}', 'exhaust':'{', 'denied':'{', 'truncated':'{"command":"touch forbidden.txt"}', 'duplicate':'{}'}.get(mode,'{}')
            name='absent_tool' if mode=='unknown' else ('state' if mode in ('state','duplicate') else 'bash')
            calls=[(f'bad-{step}',name,args)]
            if mode=='mixed': calls.insert(0,('sibling','bash','{"command":"touch forbidden.txt"}'))
            if mode=='duplicate':calls.append((f'bad-{step}','state','{}'))
        elif step==1:
            calls=[('corrected','bash',json.dumps({'a_nested':{'command':'touch forbidden.txt'},'command': command if mode!='denied' else 'touch forbidden.txt'}))]
        elif not followup:
            history=json.dumps(body)
            if mode=='execution':assert '[exit 7]' in history
            elif mode=='denied':
                assert 'blocked' in history.lower() or 'denied' in history.lower() or '[exit ' in history
                if wire=='anthropic':
                    results=[block for m in body['messages'] for block in m.get('content',[]) if isinstance(block,dict) and block.get('type')=='tool_result']
                    assert results[-1]['is_error'] is True, results[-1]
            else:assert 'REPAIR_TOOL_OK' in history
        self.send_response(200);self.send_header('Content-Type','text/event-stream');self.end_headers()
        def emit(value,event=None):
            self.wfile.write(((('event: '+event+'\n') if event else '')+'data: '+json.dumps(value)+'\n\n').encode());self.wfile.flush()
        try:
            if wire=='anthropic':
                emit({'type':'message_start','message':{'usage':{'input_tokens':20}}},'message_start')
                for i,(ident,name,args) in enumerate(calls):
                    emit({'index':i,'content_block':{'type':'tool_use','id':ident,'name':name,'input':{}}},'content_block_start')
                    emit({'index':i,'delta':{'type':'input_json_delta','partial_json':args}},'content_block_delta')
                    emit({'index':i},'content_block_stop')
                if not calls:emit({'index':0,'delta':{'type':'text_delta','text':text}},'content_block_delta')
                if mode!='truncated':emit({},'message_stop')
            elif wire=='openai-responses':
                for i,(ident,name,args) in enumerate(calls):emit({'type':'response.output_item.done','output_index':i,'item':{'type':'function_call','call_id':ident,'name':name,'arguments':args}})
                if not calls:emit({'type':'response.output_text.delta','delta':text})
                if mode!='truncated':emit({'type':'response.completed','response':{}})
            else:
                if calls:emit({'choices':[{'delta':{'tool_calls':[{'index':i,'id':ident,'function':{'name':name,'arguments':args}} for i,(ident,name,args) in enumerate(calls)]}}]})
                else:emit({'choices':[{'delta':{'content':text}}]})
                if mode!='truncated':self.wfile.write(b'data: [DONE]\n\n');self.wfile.flush()
        except (BrokenPipeError,ConnectionResetError):pass

def run(wire,mode,size=None):
    with tempfile.TemporaryDirectory(prefix='gc-tool-repair-') as tmp:
        root=Path(tmp); cfg=root/'config/glass-core';cfg.mkdir(parents=True)
        server=Server(('127.0.0.1',0),Endpoint);server.mode=mode;server.wire=wire;server.requests=[]
        threading.Thread(target=server.serve_forever,daemon=True).start()
        profile='permission_profile="read-only"\n' if mode=='denied' else ''
        (cfg/'config.toml').write_text(f'provider="{wire}"\nmodel="mock"\napi_key_env=""\nbase_url="http://127.0.0.1:{server.server_port}/v1"\ntrust="always"\n'+profile)
        env=dict(os.environ,XDG_CONFIG_HOME=str(root/'config'),XDG_DATA_HOME=str(root/'data'))
        try:
            if size:
                t=Terminal(root,env,size)
                try:
                    t.idle();t.send('exercise repair\r')
                    if mode=='cancel':
                        t.wait('Tool repair',10)
                        deadline=time.monotonic()+5
                        while len(server.requests)<2:
                            assert time.monotonic()<deadline
                            t.collect(.02)
                        t.send('\x03');t.wait('Cancelled',10);t.collect(.2)
                    else:
                        t.wait('REPAIR_FINISHED',20);t.collect(.2)
                    assert 'Tool repair' in t.seen,t.seen
                    t.send('follow-up\r');t.wait('FOLLOWUP_DONE',15);t.collect(.1)
                finally:t.close()
            else:
                p=subprocess.run([str(BIN),'run','--json','exercise repair'],cwd=root,env=env,text=True,capture_output=True,timeout=20)
                failed=mode in ('exhaust','truncated','duplicate')
                assert (p.returncode!=0)==failed,(wire,mode,p.stdout,p.stderr)
                if not failed:assert 'REPAIR_FINISHED' in p.stdout,(wire,mode,p.stdout)
                if mode=='exhaust':assert len(server.requests)==3 and 'repair exhausted' in p.stdout,p.stdout
                if mode in ('truncated','duplicate'):assert len(server.requests)==1
            assert not (root/'forbidden.txt').exists(),(wire,mode,'invalid or denied side effect ran')
            if mode not in ('exhaust','truncated','duplicate','denied','cancel'):
                assert (root/'executed.txt').read_text()=='x',(wire,mode,'action repeated')
            rows=[json.loads(x) for p in (root/'data/glass-core').glob('*.jsonl') for x in p.read_text().splitlines()]
            rows=[r for r in rows if isinstance(r,dict)]
            calls=[r for r in rows if r.get('k')=='call'];results=[r for r in rows if r.get('k')=='tool']
            assert len(calls)==len(results),(wire,mode,calls,results)
            if mode=='after-completed':assert (root/'previous.txt').read_text()=='p','completed action replayed'
            if mode=='json':assert any(r.get('args')=='{"command":"touch forbidden.txt"' for r in calls),'raw call audit lost'
        finally:server.shutdown();server.server_close()
    print('PASS:',wire,mode,size or 'headless',flush=True)
for wire in ('openai','openai-responses','anthropic'):
    for mode in ('json','type','missing','unknown','state','mixed','execution','after-completed','exhaust','truncated','duplicate'):
        run(wire,mode)
if os.uname().sysname=='Linux':
    for wire in ('openai','openai-responses','anthropic'):run(wire,'denied')
for wire in ('openai','openai-responses','anthropic'):
    for size in ((80,24),(40,24),(20,8)):run(wire,'json',size)
run('openai','cancel',(40,24))
print('PASS: bounded validation repair across three wire formats; no rejected batch side effects, no action replay, raw audit, exhaustion, fatal incomplete streams, authorization, PTY and follow-up')
