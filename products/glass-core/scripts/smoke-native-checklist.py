#!/usr/bin/env python3
"""Native checklist lifecycle against an actual GC binary and local SSE provider."""
import http.server
import json
import os
from pathlib import Path
import subprocess
import tempfile
import threading
import time
from pty_vt import Terminal
from core_binary import core_binary_path

BIN = Path(os.environ.get('GLASS_CORE_TEST_BINARY', str(core_binary_path('release')))).resolve()
os.environ['GLASS_CORE_TEST_BINARY'] = str(BIN)

class Endpoint(http.server.BaseHTTPRequestHandler):
    def log_message(self, *_):
        pass

    def do_POST(self):
        body = json.loads(self.rfile.read(int(self.headers['Content-Length'])))
        self.server.bodies.append(body)
        systems = '\n'.join(str(m.get('content', '')) for m in body.get('messages', []) if m.get('role') == 'system')
        if 'You are planning a coding task.' in systems:
            assert '[GC workflow protocol 3]' not in systems, systems
        else:
            assert systems.count('[GC workflow protocol 3]') == 1, systems
            assert 'create state.todo before substantive work' in systems, systems
            assert 'Use advertised schemas' in systems, systems

        results = [message for message in body.get('messages', []) if message.get('role') == 'tool']
        mode = self.server.mode
        self.send_response(200)
        self.send_header('Content-Type', 'text/event-stream')
        self.end_headers()
        if mode == 'busy':
            self.server.gate.wait(10)
            delta = {'content': 'BUSY_CHECKLIST_DONE'}
        elif mode == 'guidance':
            if not results:
                delta = self.tool('discover-guidance', {'guidance': {'action': 'catalogue'}})
            elif len(results) == 1:
                catalogue = json.loads(results[-1]['content'])
                assert catalogue['protocol'] == 2 and catalogue['complete_catalogue']
                assert len(catalogue['commands']) > 24
                assert {'checklist', 'research', 'integrations', 'delivery', 'sessions'} <= set(catalogue['topics'])
                delta = self.tool('read-guidance', {'guidance': {'action':'read', 'topic':'checklist'}})
            elif len(results) == 2:
                recipe = json.loads(results[-1]['content'])['recipe']
                args = json.loads(recipe.split('Example state arguments: ',1)[1].split('. Use returned',1)[0])
                delta = self.tool('recipe-checklist', args)
            else:
                delta = {'content': 'GUIDANCE_ROUTE_DONE'}
        elif mode == 'legacy':
            if not results:
                delta = {'tool_calls':[{'index':0,'id':'legacy-add','function':{'name':'todo','arguments':json.dumps({'action':'add','item':'Legacy native task'})}}]}
            else:
                delta = {'content':'LEGACY_CHECKLIST_DONE'}
        elif mode == 'plan':
            delta = {'content': '## Implement\n- [ ] Implement approved change\n## Verify\n- [ ] Verify approved change\n'}
        elif mode == 'read':
            if not results:
                delta = self.tool('read-todo', {'todo': {}})
            else:
                delta = {'content': 'READ_CHECKLIST_DONE'}
        elif mode == 'auto':
            if not results:
                delta = self.tool('init-auto', {'todo': {'ops': [{'op':'init', 'items':[{'title':'Finish auto work'}]}, {'op':'configure','auto_continue':True}]}, 'request_id':'auto-init'})
            elif not any(message.get('role') == 'system' and 'Checklist continuation' in str(message.get('content')) for message in body['messages']):
                delta = {'content': 'Partial progress.'}
            elif len(results) == 1:
                delta = self.tool('finish-auto', {'todo': {'ops':[{'op':'update','id':'T1','status':'completed'}]}})
            else:
                delta = {'content': 'AUTO_CHECKLIST_DONE'}
        elif not results:
            delta = self.tool('init-checklist', {'todo': {'ops':[{'op':'init','items':[{'title':'Implement native checklist','phase':'Build'},{'title':'Verify native checklist','phase':'Verify'}]}]},'request_id':'fixture-init'})
        elif len(results) == 1:
            delta = self.tool('update-checklist', {'todo': {'ops':[{'op':'update','id':'T1','status':'completed'},{'op':'update','id':'T2','status':'blocked','blocker':'Waiting for a fixture approval'}]},'request_id':'fixture-progress'})
        else:
            delta = {'content': 'CHECKLIST_READY'}
        self.wfile.write(('data: '+json.dumps({'choices':[{'delta':delta}]})+'\n\ndata: [DONE]\n\n').encode())

    @staticmethod
    def tool(id, args):
        return {'tool_calls':[{'index':0,'id':id,'function':{'name':'state','arguments':json.dumps(args)}}]}

with tempfile.TemporaryDirectory(prefix='gc-native-checklist-') as tmp:
    root = Path(tmp)
    server = http.server.ThreadingHTTPServer(('127.0.0.1', 0), Endpoint)
    server.daemon_threads = True
    server.mode = 'workflow'
    server.bodies = []
    server.gate = threading.Event()
    threading.Thread(target=server.serve_forever, daemon=True).start()
    config = root/'config/glass-core'
    config.mkdir(parents=True)
    (config/'config.toml').write_text(f'provider="openai"\nmodel="fixture"\nbase_url="http://127.0.0.1:{server.server_port}/v1"\napi_key_env=""\ntrust="always"\nadaptive_pacing=false\n')
    env = dict(os.environ, XDG_CONFIG_HOME=str(root/'config'), XDG_DATA_HOME=str(root/'data'))
    def run(prompt):
        result = subprocess.run([str(BIN),'run','--json',prompt],cwd=root,env=env,capture_output=True,text=True,timeout=20)
        assert result.returncode == 0, result.stdout+result.stderr
        return json.loads(result.stdout)
    result = run('Implement the fixture checklist')
    assert result['text'] == 'CHECKLIST_READY'
    session_id = result['session']
    log = root/'data/glass-core'/f'{session_id}.jsonl'
    records = [json.loads(line) for line in log.read_text().splitlines()]
    snapshots = [json.loads(r['d']) for r in records if r.get('c') == 'state/todo']
    assert len(snapshots) == 2 and [t['status'] for t in snapshots[-1]['tasks']] == ['completed','blocked'], snapshots
    assert len(server.bodies) == 3
    assert len(server.bodies[0]['tools']) == 6, 'Checklist added a redundant top-level tool'
    assert not any('receipts' in str(m.get('content', '')) for m in server.bodies[-1]['messages']), 'Receipt ledger leaked into model context'
    print('PASS native batches, durable snapshots, six tools, compact model context', flush=True)

    for index, size in enumerate(((80,30),(40,24),(20,8))):
        requests_before = len(server.bodies)
        terminal = Terminal(root,env,size,args=('resume',session_id))
        try:
            terminal.idle()
            terminal.visible(f'Tasks 1/{2+index}' + (' complete' if size[0]>=40 else ' !1'))
            terminal.send('/todo\r')
            terminal.visible('Checklist')
            terminal.send('\r')
            terminal.collect(.2)
            terminal.send('\x1b[F')
            terminal.collect(.2)
            assert 'verification' in terminal.vt.text() or 'separate' in terminal.vt.text(), terminal.vt.text()
            terminal.send('\x1b')
            terminal.collect(.2)
            terminal.visible('Checklist')
            terminal.send('add task\r')
            terminal.visible('Add task')
            terminal.send('Human-added task\r')
            terminal.collect(.3)
            terminal.visible('Checklist')
            assert len(server.bodies) == requests_before, 'Human edit spent a provider request'
            terminal.send('\x1b')
            terminal.collect(.2)
            if 'Search actions' in terminal.vt.text():
                terminal.send('\x1b')
                terminal.collect(.2)
            terminal.idle()
            terminal.send('\x1b[5~')
            terminal.collect(.2)
            terminal.send('\x1b[F\x1b')
            terminal.collect(.2)
            terminal.visible('Tasks 1/')
            server.mode = 'busy'
            terminal.send('\x15check busy\r')
            deadline = time.monotonic()+10
            while len(server.bodies) == requests_before:
                assert time.monotonic() < deadline
                terminal.collect(.05)
            terminal.send('/todo\r')
            terminal.visible('Checklist')
            terminal.send('\x1b')
            terminal.collect(.2)
            if 'Search actions' in terminal.vt.text():
                terminal.send('\x1b')
                terminal.collect(.2)
            server.gate.set()
            terminal.wait('BUSY_CHECKLIST_DONE')
            terminal.collect(.2)
            server.gate.clear()
            print('PASS resume, checklist modal, human edit without model call, scroll and busy browsing:', size, flush=True)
        finally:
            server.gate.set()
            terminal.close()
            server.gate.clear()

    server.mode = 'auto'
    server.bodies = []
    result = run('Finish the automatic fixture')
    assert result['text'] == 'AUTO_CHECKLIST_DONE', result
    assert len(server.bodies) == 4, len(server.bodies)
    print('PASS opt-in continuation completes actionable work', flush=True)
    server.mode = 'legacy'
    server.bodies = []
    original_config = (config/'config.toml').read_text()
    adapter = Path(__file__).resolve().parents[1]/'extensions/todo/todo.py'
    (config/'config.toml').write_text(original_config+'extensions = ['+json.dumps('python3 '+str(adapter))+']\n')
    result = run('Use the legacy compatibility tool')
    assert result['text'] == 'LEGACY_CHECKLIST_DONE', result
    legacy_log = root/'data/glass-core'/f"{result['session']}.jsonl"
    legacy_records = [json.loads(line) for line in legacy_log.read_text().splitlines()]
    assert any(isinstance(record,dict) and record.get('c') == 'state/todo' and 'Legacy native task' in record.get('d','') for record in legacy_records)
    (config/'config.toml').write_text(original_config)
    print('PASS legacy todo adapter commits native state', flush=True)
    server.mode = 'plan'
    server.bodies = []
    terminal = Terminal(root,env,(80,30))
    try:
        terminal.idle()
        terminal.send('/plan approved fixture\r')
        terminal.visible('Plan review', seconds=15)
        terminal.send('\x1b[B\r')
        terminal.wait('Implement approved change', seconds=15)
        terminal.collect(.3)
        logs = list((root/'data/glass-core').glob('*.jsonl'))
        assert any(any(isinstance(r,dict) and r.get('c') == 'state/todo' and 'Implement approved change' in r.get('d','') for r in map(json.loads,file.read_text().splitlines())) for file in logs)
        print('PASS approved plan seeds native checklist', flush=True)
    finally:
        terminal.close()
    server.mode = 'guidance'
    server.bodies = []
    result = run('Discover the checklist workflow and use its recipe')
    assert result['text'] == 'GUIDANCE_ROUTE_DONE', result
    assert len(server.bodies) == 4
    print('PASS complete catalogue, recipe retrieval and executable checklist example', flush=True)
    server.shutdown()
