#!/usr/bin/env python3
"""Exercise the native durable API through an external host and local provider."""
import http.server
import importlib.util
import json
import os
from pathlib import Path
import tempfile
import threading
from core_binary import core_binary_path

ROOT=Path(__file__).resolve().parents[1]
spec=importlib.util.spec_from_file_location('host',ROOT/'examples/runtime_host.py')
host=importlib.util.module_from_spec(spec);spec.loader.exec_module(host)
BIN=Path(os.environ.get('GLASS_CORE_TEST_BINARY',core_binary_path('release'))).resolve()
requests=[]
class Endpoint(http.server.BaseHTTPRequestHandler):
    def log_message(self,*args):pass
    def do_POST(self):
        body=json.loads(self.rfile.read(int(self.headers['Content-Length'])))
        requests.append((body,dict(self.headers)))
        result=any(m['role']=='tool' for m in body['messages'])
        payload={'content':'DONE'} if result else {'tool_calls':[{'index':0,'id':'inspect-once','function':{'name':'inspect','arguments':'{}'}}]}
        self.send_response(200);self.send_header('Content-Type','text/event-stream');self.end_headers()
        for delta in [payload]:self.wfile.write(('data: '+json.dumps({'choices':[{'delta':delta}]})+'\n\n').encode())
        self.wfile.write(b'data: {"choices":[],"usage":{"prompt_tokens":10,"completion_tokens":3}}\n\ndata: [DONE]\n\n');self.wfile.flush()
server=http.server.ThreadingHTTPServer(('127.0.0.1',0),Endpoint)
threading.Thread(target=server.serve_forever,daemon=True).start()
with tempfile.TemporaryDirectory(prefix='gc-durable-conformance-') as tmp:
    root=Path(tmp);config=root/'config/glass-core';config.mkdir(parents=True)
    (config/'config.toml').write_text(f'provider="openai"\nmodel="mock"\napi_key_env=""\nbase_url="http://127.0.0.1:{server.server_port}/v1"\nadaptive_pacing=false\n')
    env=dict(os.environ,XDG_CONFIG_HOME=str(root/'config'),XDG_DATA_HOME=str(root/'data'))
    def launch():
        client=host.RuntimeClient(BIN,root,env)
        client.wait(client.request('initialize',{'protocolVersion':1,'storageDir':str(root/'sessions')}))
        return client
    client=launch()
    def call(method,params,tool=None):return client.wait(client.request(method,params),tool=tool)
    def seq():return call('durable/inspect',{})['sequence']
    def ops(items):return call('durable/commit',{'expectedSequence':seq(),'operations':items})
    options={'cwd':str(root),'tools':[{'name':'inspect','inputSchema':{'type':'object'}}],'instructions':'Answer using inspect when necessary.'}
    try:
        assert not (root/'sessions/durable').exists(),'ordinary initialization must stay lazy'
        cv=call('durable/create',options)['conversationId']
        first=call('durable/submit',{'conversationId':cv,'requestId':'one','input':{'text':'FIRST_ONLY'}})['submission']
        second=call('durable/submit',{'conversationId':cv,'requestId':'two','input':{'text':'SECOND_LATER'}})['submission']
        assert not call('durable/entries',{'conversationId':cv})['entries']
        assert call('durable/submit',{'conversationId':cv,'requestId':'one','input':{'text':'FIRST_ONLY'}})['duplicate']
        effects=[]
        def tool(p):effects.append(p['callId']);return 'INSPECT_RESULT'
        call('durable/run',{'taskId':first['id']},tool)
        assert effects==['inspect-once']
        assert all('SECOND_LATER' not in str(body) for body,_ in requests)
        affinity=requests[0][1].get('x-session-id') or requests[0][1].get('X-Session-Id')
        assert affinity and all((headers.get('x-session-id') or headers.get('X-Session-Id'))==affinity for _,headers in requests)
        assert all('prompt_cache_key' not in body for body,_ in requests),'custom providers must not receive unsupported cache keys'
        assert call('durable/task',{'taskId':first['id']})['task']['status']=='completed'
        assert call('durable/task',{'taskId':first['id']})['task']['input'] is None
        cursor=0
        page=call('durable/changes',{'cursor':cursor,'limit':2});assert len(page['commits'])==2 and page['hasMore']
        items=[];cursor=''
        while True:
            page=call('durable/inspect',{'cursor':cursor,'limit':1});items+=page['items'];cursor=page['cursor']
            if not page['hasMore']:break
        assert len(items)>=4 and len({json.dumps(i,sort_keys=True) for i in items})==len(items)
        before=seq()
        doc={'op':'document.put','conversation':cv,'kind':'app.todo','version':1,'expectedRevision':0,'value':{'done':False},'history':True,'fork':'as_of'}
        task={'op':'task.create','id':'custom','conversation':cv,'kind':'app.step','version':1,'replay':'safe','checkpoint':{'phase':'start'}}
        ops([doc,task]);snapshot=seq()
        call('durable/register',{'tasks':[{'kind':'app.step','version':1,'replay':'safe'}]})
        custom=call('durable/claim',{})['task'];assert custom['id']=='custom'
        ops([{'op':'task.update','id':'custom','expectedRevision':custom['revision'],'status':'running','checkpoint':{'phase':'saved'}}])
        client.close();client=launch()
        assert call('durable/task',{'taskId':'custom'})['task']['status']=='pending'
        assert call('durable/submission',{'conversationId':cv,'requestId':'one'})['submission']['id']==first['id']
        assert call('durable/submit',{'conversationId':cv,'requestId':'one','input':{'text':'FIRST_ONLY'}})['duplicate']
        call('durable/run',{'taskId':second['id']},tool)
        assert len(effects)==1,'earlier tool results remain in context; no replay of completed effects'
        assert all((headers.get('x-session-id') or headers.get('X-Session-Id'))==affinity for _,headers in requests)
        fork=call('durable/fork',{'conversationId':cv,'asOf':snapshot})['conversationId']
        assert call('durable/document',{'conversationId':fork,'kind':'app.todo'})['document']['value']=={'done':False}
        changes=call('durable/changes',{'cursor':before,'limit':100});assert changes['commits']
        assert call('durable/taskGraph',{})['tasks']
        # Abort must reach a blocked host tool, not wait for its result.
        abort_cv=call('durable/create',options)['conversationId']
        abort_task=call('durable/submit',{'conversationId':abort_cv,'requestId':'abort','input':{'text':'ABORT_NOW'}})['submission']['id']
        abort_run=client.request('durable/run',{'taskId':abort_task})
        while True:
            message=client.receive()
            if message.get('method')=='tool/execute':break
        tool_request=message
        abort_request=client.request('durable/abort',{'taskId':abort_task})
        got_abort=False;got_run=False;got_cancel=False
        while not (got_abort and got_run and got_cancel):
            message=client.receive()
            if message.get('id')==abort_request:assert 'result' in message;got_abort=True
            if message.get('id')==abort_run:assert 'result' in message,message;got_run=True
            if message.get('method')=='runtime/event' and message['params']['type']=='tool_cancel':
                client.request('tool/cancelled',{'sessionId':abort_cv,'requestId':tool_request['id'],'executionStatus':'cancelled'});got_cancel=True
        assert call('durable/task',{'taskId':abort_task})['task']['status']=='aborted'
        # Kill only this fixture's runtime after dispatch but before a tool result.
        crash_cv=call('durable/create',options)['conversationId']
        crash_task=call('durable/submit',{'conversationId':crash_cv,'requestId':'crash','input':{'text':'CRASH_TOOL_ONCE'}})['submission']['id']
        pending=client.request('durable/run',{'taskId':crash_task})
        while True:
            message=client.receive()
            if message.get('method')=='tool/execute':break
        client.process.kill();client.process.wait(timeout=5);client=launch()
        before_recovery=len(requests)
        def forbidden(p):raise AssertionError('unsafe interrupted tool was dispatched again')
        call('durable/run',{'taskId':crash_task},forbidden)
        assert len(requests)==before_recovery+1
        entries=call('durable/entries',{'conversationId':crash_cv,'limit':128})['entries']
        assert 'external effect may have completed' in str(entries)
        assert call('durable/task',{'taskId':crash_task})['task']['status']=='completed'
        call('durable/abort',{'taskId':'custom'})
        call('durable/reset',{'conversationId':cv})
        assert len(call('durable/entries',{'conversationId':cv})['entries'])==1
        print('PASS: native durable external-host admission, FIFO, atomic documents/tasks, checkpoints, restart, dedupe, affinity, pagination, fork, abort and reset')
    finally:client.close()
server.shutdown();server.server_close()
