#!/usr/bin/env python3
"""Isolated native gateway + real-browser smoke checks. Never changes Tailscale."""
import base64,urllib.parse
import http.server,json,os,re,subprocess,tempfile,threading,time,urllib.request,urllib.error,socket,sys
from pathlib import Path
ROOT=Path(__file__).resolve().parents[1]
BIN=Path(os.environ.get('GLASS_CORE_TEST_BINARY',ROOT/'target/debug/glass-core')).resolve()
class Provider(http.server.BaseHTTPRequestHandler):
    calls=0
    received=[]
    hold=threading.Event()
    def log_message(self,*_): pass
    def do_GET(self):
        data=json.dumps({'data':[{'id':'mock'}]}).encode();self.send_response(200);self.send_header('Content-Length',str(len(data)));self.end_headers();self.wfile.write(data)
    def do_POST(self):
        body=json.loads(self.rfile.read(int(self.headers['Content-Length'])));type(self).calls+=1
        messages=body.get('messages',[]);last=messages[-1] if messages else {};text=str(last.get('content',''));type(self).received.append(text)
        if last.get('role')=='tool':
            frames=[{'choices':[{'delta':{'content':'Decision received. Work continues.'}}]}]
        elif 'cap fixture' in text.lower():
            frames=[{'choices':[{'delta':{'content':'Bounded output '+('x'*65536)}}]} for _ in range(5)]
        elif 'scroll reasoning fixture' in text.lower():
            frames=[{'choices':[{'delta':{'reasoning_content':('Reading section '+str(i)+'. '+('A detailed observation about the current task. '*20)+'\n\n')}}]} for i in range(40)]
            frames.append({'choices':[{'delta':{'content':'Reasoning scroll finished.'}}]})
        elif 'stability fixture' in text.lower():
            chunks=['StableParagraph **retained**.\n\n```javascript\nconst fixed = 1;\n```\n\n']+[f'Frame {i} remains readable.\n\n' for i in range(12)]
            frames=[{'choices':[{'delta':{'content':chunk}}]} for chunk in chunks]
        elif 'interleaved fixture' in text.lower():
            frames=[{'choices':[{'delta':{'content':'BeforeToolReady.\n\n'}}]},{'choices':[{'delta':{'tool_calls':[{'index':0,'id':'call-interleaved','type':'function','function':{'name':'bash','arguments':'{"command":"printf tool-ready"}'}}]},'finish_reason':'tool_calls'}]}]
        elif 'question' in text.lower():
            args={'questions':[{'question':'Which output style?','options':[{'label':'Compact','description':'Less space'},{'label':'Detailed','description':'More context'}]}]}
            frames=[{'choices':[{'delta':{'tool_calls':[{'index':0,'id':'call-question','type':'function','function':{'name':'ask_user_question','arguments':json.dumps(args)}}]},'finish_reason':'tool_calls'}]}]
        elif 'approval' in text.lower():
            args={'command':'printf approved','permission':'require_escalated','justification':'Run this exact harmless approval fixture'}
            frames=[{'choices':[{'delta':{'tool_calls':[{'index':0,'id':'call-approval','type':'function','function':{'name':'bash','arguments':json.dumps(args)}}]},'finish_reason':'tool_calls'}]}]
        elif 'command' in text.lower():
            frames=[{'choices':[{'delta':{'tool_calls':[{'index':0,'id':'call-command','type':'function','function':{'name':'bash','arguments':'{"command":"printf tool-ready"}'}}]},'finish_reason':'tool_calls'}]}]
        else:
            frames=[{'choices':[{'delta':{'reasoning_content':'Considering the workspace and preserving context.'}}]},{'choices':[{'delta':{'content':'StreamingReady. **Clear output**, same thread.'}}]}]
        self.send_response(200);self.send_header('Content-Type','text/event-stream');self.end_headers()
        try:
            for frame in frames:self.wfile.write(('data: '+json.dumps(frame)+'\n\n').encode());self.wfile.flush();time.sleep(.04)
        except (BrokenPipeError,ConnectionResetError):return
        if 'stability fixture' in text.lower() or 'cap fixture' in text.lower():time.sleep(.6)
        if 'HOLD-WEB-STREAM' in text:type(self).hold.wait(30)
        try:self.wfile.write(b'data: [DONE]\n\n');self.wfile.flush()
        except (BrokenPipeError,ConnectionResetError):pass
FIXTURE=r'''## A quieter place to work

The runtime and your workspace stay together. Here is a **readable preview** of the changes.

| Surface | Status | Detail |
| --- | --- | --- |
| Terminal | Ready | Shared thread |
| iPhone | Connected | Private tailnet |
| Runtime | Working | Same tools |

| Feature | Implementation | Why it helps |
| --- | --- | --- |
| Boundaries | A long explanation with `some_long_identifier_without_spaces` and inline code. A long explanation with `some_long_identifier_without_spaces` and inline code. A long explanation with `some_long_identifier_without_spaces` and inline code. A long explanation with `some_long_identifier_without_spaces` and inline code. A long explanation with `some_long_identifier_without_spaces` and inline code. A long explanation with `some_long_identifier_without_spaces` and inline code. A long explanation with `some_long_identifier_without_spaces` and inline code. A long explanation with `some_long_identifier_without_spaces` and inline code. A long explanation with `some_long_identifier_without_spaces` and inline code. A long explanation with `some_long_identifier_without_spaces` and inline code. A long explanation with `some_long_identifier_without_spaces` and inline code. A long explanation with `some_long_identifier_without_spaces` and inline code.  | A separate explanation in the next cell. A separate explanation in the next cell. A separate explanation in the next cell. A separate explanation in the next cell. A separate explanation in the next cell. A separate explanation in the next cell. A separate explanation in the next cell. A separate explanation in the next cell. A separate explanation in the next cell. A separate explanation in the next cell. A separate explanation in the next cell. A separate explanation in the next cell. A separate explanation in the next cell. A separate explanation in the next cell. A separate explanation in the next cell.  |

```typescript
interface Thread { id: string; status: "working" | "ready" }
const resume = (thread: Thread) => thread.id;
```

An equation, rendered in place: $E=mc^2$.

```mermaid
flowchart LR
 A[Your iPhone] --> B[Private tailnet]
 B --> C[Glass runtime]
 C --> D[Same thread]
```

```diff
- const transcript = duplicateThread();
+ const transcript = attachToThread(id);
```

```html
<h1>Preview ready</h1><script>parent.__unsafe = true</script>
```

Open [the project notes](notes.md) or [a small JSON artifact](result.json), and [a PDF artifact](sample.pdf).
'''
with tempfile.TemporaryDirectory(prefix='gc-connect-smoke-') as tmp:
    root=Path(tmp);data=root/'data/glass-core';data.mkdir(parents=True);cfg=root/'config/glass-core';cfg.mkdir(parents=True)
    provider=http.server.ThreadingHTTPServer(('127.0.0.1',0),Provider);threading.Thread(target=provider.serve_forever,daemon=True).start()
    (cfg/'config.toml').write_text(f'provider="openai"\nmodel="mock"\napi_key_env=""\nbase_url="http://127.0.0.1:{provider.server_port}/v1"\nsandbox="workspace"\n')
    (root/'project-two').mkdir();(root/'project-two'/'readme.md').write_text('# Second project\n\n![Fixture icon](icon.png)');(root/'project-two'/'icon.png').write_bytes(base64.b64decode('iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+aU1kAAAAASUVORK5CYII='));(root/'outside-link').symlink_to('/etc',target_is_directory=True)
    (root/'notes.md').write_text('# Project notes\n\nPreviewReady notes stay on the host.\n\n'+('Additional file detail.\n\n'*150)+'```javascript\nconst nestedPreview = true;\n```');(root/'result.json').write_text('{"state":"ready","files":2}')
    # One-page fixture, generated without a PDF library.
    objects=[b'<< /Type /Catalog /Pages 2 0 R >>',b'<< /Type /Pages /Kids [3 0 R] /Count 1 >>',b'<< /Type /Page /Parent 2 0 R /MediaBox [0 0 300 200] /Resources << /Font << /F1 5 0 R >> >> /Contents 4 0 R >>',b'<< /Length 44 >>\nstream\nBT /F1 18 Tf 30 100 Td (PreviewReady) Tj ET\nendstream',b'<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>']
    pdf=bytearray(b'%PDF-1.4\n');offsets=[0]
    for index,obj in enumerate(objects,1):offsets.append(len(pdf));pdf.extend(f'{index} 0 obj\n'.encode()+obj+b'\nendobj\n')
    xref=len(pdf);pdf.extend(f'xref\n0 {len(offsets)}\n0000000000 65535 f \n'.encode())
    for offset in offsets[1:]:pdf.extend(f'{offset:010} 00000 n \n'.encode())
    pdf.extend(f'trailer << /Size {len(offsets)} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF'.encode());(root/'sample.pdf').write_bytes(pdf)
    records=[{'k':'user','t':'Help me keep this readable.'},{'k':'call','id':'fixture-tool','name':'read','args':'{"path":"src/runtime.rs"}'},{'k':'tool','id':'fixture-tool','name':'read','t':'Read 120 lines · src/runtime.rs'},{'k':'assistant','t':FIXTURE}]
    (data/'web-fixture.jsonl').write_text(''.join(json.dumps(r)+'\n' for r in records));(data/'web-fixture.meta.json').write_text(json.dumps({'workspace':str(root),'title':'Shared runtime workspace'}))
    reasoning_records=[{'k':'reasoning','t':'Archived thought '+str(i)+'. '+('Earlier investigation details. '*35)+'\n\n'} for i in range(80)]
    reasoning_records += [{'k':'meta','t':'Internal fixture record'} for _ in range(70)]
    reasoning_records += [{'k':'reasoning','t':'Latest short thought.'},{'k':'assistant','t':'Reasoning archive response.'}]
    (data/'reasoning-archive.jsonl').write_text(''.join(json.dumps(r)+'\n' for r in reasoning_records))
    (data/'reasoning-archive.meta.json').write_text(json.dumps({'workspace':str(root),'title':'Reasoning archive fixture'}))
    (data/'other-project.jsonl').write_text(json.dumps({'k':'user','t':'Other project fixture'})+'\n')
    (data/'other-project.meta.json').write_text(json.dumps({'workspace':str(root/'project-two'),'title':'Other project fixture'}))
    (data/'legacy-archive.jsonl').write_text(json.dumps({'k':'user','t':'Legacy archive fixture'})+'\n')
    env=dict(os.environ,XDG_CONFIG_HOME=str(root/'config'),XDG_DATA_HOME=str(root/'data'),GC_WEB_ASSETS=str(ROOT/'web/dist'))
    result=subprocess.run([str(BIN),'connect','--local'],cwd=root,env=env,capture_output=True,text=True,timeout=15);assert result.returncode==0,result.stderr
    match=re.search(r'http://127\.0\.0\.1:\d+/#\w+',result.stdout);assert match,result.stdout;url=match.group(0);base,token=url.split('/#');settings=json.loads((data/'connect.json').read_text())
    def req(path,method='GET',body=None,auth=True,origin=None,headers=None):
        hs=dict(headers or {});hs['Origin']=origin or base
        if auth:hs['Authorization']='Bearer '+token
        if body is not None:hs['Content-Type']='application/json'
        r=urllib.request.Request(base+path,data=json.dumps(body).encode() if body is not None else None,method=method,headers=hs)
        try:
            with urllib.request.urlopen(r,timeout=10)as response:return response.status,response.read(),response.headers
        except urllib.error.HTTPError as e:return e.code,e.read(),e.headers
    try:
        assert req('/api/threads',auth=False)[0]==403
        catalogue=json.loads(req('/api/commands')[1])['commands'];assert len(catalogue)>60
        assert all(c['portable'] or c['name'] in ['conversations','work','settings','help','outputs','thinking','theme','redraw','keys','animation','tools','clear','editor'] for c in catalogue)
        assert req('/api/command','POST',{'id':'web-fixture','name':'name','values':{'args':'no'} })[0]==400
        assert req('/api/export?id=web-fixture',auth=False)[0]==403

        assert req('/api/threads',origin='https://evil.test')[0]==403
        threads=json.loads(req('/api/threads')[1]);by_id={t['session']:t for t in threads['threads']}
        assert {'web-fixture','other-project','legacy-archive'} <= by_id.keys(),by_id
        assert by_id['other-project']['workspace']==str(root/'project-two')
        assert by_id['legacy-archive']['workspace_assigned']==False
        assert threads['history_loaded']==False

        assert req('/api/send','POST',{'id':'../bad','message':{'t':'prompt','text':'no'}})[0]==400
        assert req('/api/file?id=web-fixture&path=/etc/passwd')[0]==400
        assert req('/api/history?id=web-fixture')[0]==200
        assert json.loads(req('/api/history?id=web-fixture')[1])['records'][-1]['record']['t']==FIXTURE
        assert req('/api/send','POST',{'id':'web-fixture','message':{'t':'unsupported'}})[0]==400
        assert req('/api/new','POST',{})[0]==200
        listing=json.loads(req('/api/browse')[1]);assert len(listing['entries'])<=256;assert any(e['name']=='project-two' for e in listing['entries'])
        assert req('/api/browse?path=/etc')[0]==400
        assert req('/api/browse?path='+urllib.parse.quote(str(root/'outside-link')))[0]==400
        assert req('/api/file?workspace=/etc&path=passwd')[0]==400
        assert req('/api/new','POST',{'workspace':'/etc'})[0]==400
        code,body,_=req('/api/new','POST',{'workspace':str(root/'project-two')});assert code==200
        new=json.loads(body)['id'];assert json.loads((data/(new+'.meta.json')).read_text())['workspace']==str(root/'project-two')
        usage=json.loads(req('/api/usage?path='+urllib.parse.quote(str(root/'project-two')))[1]);assert usage['entries']==2 and not usage['partial']
        r=subprocess.run(['node',str(ROOT/'web/test/browser.mjs'),url],cwd=ROOT,capture_output=True,text=True,timeout=120)
        print(r.stdout,flush=True);assert r.returncode==0,r.stderr
        # Older pages remain searchable without loading their transcripts.
        for number in range(520):
            path=data/f'page-{number}.jsonl';path.write_text(json.dumps({'k':'user','t':f'Paging fixture {number}'})+'\n');os.utime(path,(1,1))
        path=data/'deep-archive.jsonl';path.write_text(json.dumps({'k':'user','t':'Deep pagination fixture'})+'\n');os.utime(path,(0,0))
        first=json.loads(req('/api/threads')[1]);assert len(first['threads'])==512 and first['next_offset']==512
        second=json.loads(req('/api/threads?offset=512')[1]);assert any(t['session']=='deep-archive' for t in second['threads'])
        found=json.loads(req('/api/threads?q=Deep%20pagination')[1]);assert [t['session'] for t in found['threads']]==['deep-archive']
        direct=json.loads(req('/api/threads?id=deep-archive')[1]);assert direct['threads'][0]['workspace_assigned']==False
        scoped=json.loads(req('/api/threads?project='+urllib.parse.quote(str(root/'project-two')))[1]);assert all(t['workspace']==str(root/'project-two') for t in scoped['threads'])
        print('PASS: all-project indexing, unassigned archives, global search and older-page lookup',flush=True)
        assert len(list(data.glob('serve-*.sock')))>=1
        assert Provider.calls>=6,Provider.calls
        before=(data/'web-fixture.jsonl').read_bytes()
        assert req('/api/upgrade','POST',{'id':'web-fixture','confirmed':False})[0]==400
        code,body,_=req('/api/upgrade','POST',{'id':'web-fixture','confirmed':True});assert code==200,body
        assert (data/'web-fixture.jsonl').read_bytes()==before
        print('PASS: explicit idle runtime restart preserves the transcript',flush=True)

        # A worker crash after durable queuing must not lose or duplicate a prompt.
        (data/'queue-fixture.jsonl').write_text(json.dumps({'k':'user','t':'Queue fixture'})+'\n')
        (data/'queue-fixture.meta.json').write_text(json.dumps({'workspace':str(root),'title':'Queue fixture'}))
        worker=subprocess.Popen([str(BIN),'serve','queue-fixture'],cwd=root,env=env,stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL)
        queue_socket=data/'serve-queue-fixture.sock'
        def connect_queue():
            deadline=time.monotonic()+8
            while time.monotonic()<deadline:
                client=socket.socket(socket.AF_UNIX);client.settimeout(10)
                try:client.connect(str(queue_socket));return client
                except OSError:client.close();time.sleep(.04)
            raise AssertionError('Queue worker did not start')
        def line(reader):return json.loads(reader.readline())
        def wire(client,message):client.sendall((json.dumps(message)+'\n').encode())
        client=None
        try:
            client=connect_queue();reader=client.makefile('rb');wire(client,{'t':'attach'});assert line(reader)['t']=='replay'
            def command(name,values,request_id):
                wire(client,{'t':'command','name':name,'values':values,'request_id':request_id})
                while True:
                    value=line(reader)
                    if value.get('t')=='command_result' and value.get('request_id')==request_id:return value
            local_calls=Provider.calls
            value=command('todo',{'action':'add','args':'Exactly once checklist'},'todo-once');assert 'error' not in value,value
            assert command('todo',{'action':'add','args':'Exactly once checklist'},'todo-once')==value
            inspected=command('todo',{},'inspect-todo');assert inspected['result']['body'].count('Exactly once checklist')==1,inspected
            assert Provider.calls==local_calls
            observer=connect_queue();observed=observer.makefile('rb');wire(observer,{'t':'attach','observe':True});assert line(observed)['can_write']==False
            wire(observer,{'t':'command','name':'model','values':{'args':'not-authorized'},'request_id':'denied'});assert line(observed)['t']=='error'
            wire(observer,{'t':'command','name':'status','values':{},'request_id':'read-status'});assert line(observed)['t']=='command_result'
            observed.close();observer.close()
            wire(client,{'t':'command','name':'btw','values':{'args':'HOLD-WEB-STREAM'},'request_id':'cancel-btw'})
            while line(reader).get('t')!='command_start':pass
            value=command('status',{},'during-btw');assert 'error' not in value,value
            for name in ('queue','extensions'):
                value=command(name,{},'during-btw-'+name);assert 'error' not in value,value
            deadline=time.monotonic()+2
            while Provider.calls==local_calls and time.monotonic()<deadline:time.sleep(.02)
            wire(client,{'t':'cancel_command','request_id':'cancel-btw'})
            while True:
                value=line(reader)
                if value.get('t')=='command_result' and value.get('request_id')=='cancel-btw':break
            assert 'cancel' in value.get('error','').lower(),value
            assert 'HOLD-WEB-STREAM' not in (data/'queue-fixture.jsonl').read_text()
            assert 'error' not in command('todo',{'action':'clear','confirmed':True},'clear-todo')
            (data/'queue-fixture.queue.json').write_text(json.dumps({'pending':['SavedQueueReady'],'steer':None,'delivered':0}))
            value=command('queue',{'action':'saved-edit','args':'1','text':'SavedQueueReady'},'edit-saved');assert 'error' not in value,value
            value=command('queue-run',{},'run-saved');assert 'error' not in value,value
            while line(reader).get('t')!='done':pass
            assert json.loads((data/'queue-fixture.queue.json').read_text())['pending']==[]
            assert Provider.received.count('SavedQueueReady')==1,Provider.received
            print('PASS: local commands, duplicate prevention, observer guards, saved queue and cancellable model commands',flush=True)
            wire(client,{'t':'prompt','text':'HOLD-WEB-STREAM','request_id':'held'})
            while line(reader).get('t')!='text':pass
            queued={'t':'prompt','text':'RecoveredQueueReady','request_id':'durable-one'};wire(client,queued)
            while True:
                message=line(reader)
                if message.get('t')=='queued':assert message['durable'];break
            receipt=json.loads((data/'queue-fixture.serve-queue.json').read_text());assert receipt[0]['request_id']=='durable-one'
            wire(client,queued)
            while line(reader).get('t')!='duplicate':pass
            worker.kill();worker.wait(timeout=5);reader.close();client.close();client=None
            worker=subprocess.Popen([str(BIN),'serve','queue-fixture'],cwd=root,env=env,stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL)
            deadline=time.monotonic()+10
            while time.monotonic()<deadline:
                records=[json.loads(x)for x in(data/'queue-fixture.jsonl').read_text().splitlines()]
                if any(r.get('k')=='serve_request'and r.get('id')=='durable-one'and r.get('status')=='completed'for r in records):break
                time.sleep(.05)
            assert sum('RecoveredQueueReady'==r.get('t')for r in records)==1,records
            assert Provider.received.count('RecoveredQueueReady')==1,Provider.received
            assert json.loads((data/'queue-fixture.serve-queue.json').read_text())==[]
            print('PASS: queued prompt survives worker crash; duplicate request executes once',flush=True)
        finally:
            Provider.hold.set()
            if client:client.close()
            subprocess.run([str(BIN),'stop','queue-fixture'],cwd=root,env=env,capture_output=True,timeout=5)
            if worker.poll() is None:worker.terminate();worker.wait(timeout=5)
        assert req('/api/status',auth=False)[0]==403
        print('PASS: auth/origin, paths, bounded history, shared writer, browser UI, tools, questions, approvals, PWA and rich previews',flush=True)
    finally:
        Provider.hold.set()
        subprocess.run([str(BIN),'connect','stop'],cwd=root,env=env,capture_output=True,timeout=10)
        for path in data.glob('serve-*.sock'):
            try:
                client=socket.socket(socket.AF_UNIX);client.settimeout(1);client.connect(str(path));client.sendall(b'{"t":"shutdown"}\n');client.close()
            except OSError:pass
        # Workers are this fixture's processes; stop through the supported command.
        for path in data.glob('serve-*.sock'):subprocess.run([str(BIN),'stop',path.stem.removeprefix('serve-')],cwd=root,env=env,capture_output=True,timeout=5)
        provider.shutdown()
