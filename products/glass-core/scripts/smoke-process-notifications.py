#!/usr/bin/env python3
"""Prove host-driven process resumption with a model fixture that would poll."""
import http.server
import importlib.util
import json
import os
from pathlib import Path
import subprocess
import tempfile
import threading
import time
from core_binary import core_binary_path

PRODUCT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location('runtime_host', PRODUCT / 'examples/runtime_host.py')
host = importlib.util.module_from_spec(spec)
spec.loader.exec_module(host)
BINARY = Path(core_binary_path('debug')).resolve()
COMMAND = 'while [ ! -f release-process ]; do sleep 0.05; done; printf PROCESS_FINISHED'

class Endpoint(http.server.BaseHTTPRequestHandler):
    def log_message(self, *_):
        pass

    def do_POST(self):
        request = json.loads(self.rfile.read(int(self.headers['Content-Length'])))
        self.server.requests.append(request)
        messages = request['messages']
        user = next((m['content'] for m in reversed(messages) if m['role'] == 'user'), '')
        if not isinstance(user, str):
            user = json.dumps(user)
        tool = None
        if '[Process completion follow-up]' in user:
            content = 'PROCESS_RESUMED_OK'
        elif '[Teammate completion follow-up]' in user:
            content = 'PARENT_RESUMED_OK'
        elif messages[-1]['role'] == 'tool':
            if messages[-1].get('name') == 'teammate' or 'capabilities' in str(messages[-1]['content']):
                content = 'Delegated; ending this turn.'
            else:
                # An intentionally weak model polls if the engine gives it a
                # request while waiting. Correct behavior never reaches this.
                tool = ('bash', {'job': {'action': 'poll', 'id': 1}})
                self.server.polls += 1
        elif 'NESTED_PARENT' in user:
            tool = ('teammate', {'action':'create', 'name':'process-worker', 'text':'NESTED_CHILD: run the finite process and report completion.', 'write':True, 'tokens':100000})
        else:
            tool = ('bash', {'command':COMMAND, 'job':{'action':'start'}})
        if tool:
            name, arguments = tool
            delta = {'tool_calls':[{'index':0, 'id':f'call-{len(self.server.requests)}', 'type':'function', 'function':{'name':name,'arguments':json.dumps(arguments)}}]}
            finish = 'tool_calls'
        else:
            delta = {'content':content}
            finish = 'stop'
        self.send_response(200)
        self.send_header('Content-Type','text/event-stream')
        self.end_headers()
        self.wfile.write(('data: '+json.dumps({'choices':[{'delta':delta,'finish_reason':finish}], 'usage':{'prompt_tokens':100,'completion_tokens':20}})+'\n\ndata: [DONE]\n\n').encode())


def until(predicate, seconds=20):
    deadline = time.monotonic() + seconds
    while not predicate():
        assert time.monotonic() < deadline, 'fixture deadline exceeded'
        time.sleep(0.02)


def run_case(base, mode):
    nested, headless = mode == "nested", mode == "headless"
    root = base / mode
    root.mkdir()
    config = root / 'config/glass-core'
    config.mkdir(parents=True)
    server = http.server.ThreadingHTTPServer(('127.0.0.1',0), Endpoint)
    server.requests, server.polls = [], 0
    threading.Thread(target=server.serve_forever,daemon=True).start()
    (config/'config.toml').write_text(f'provider="openai"\nmodel="mock"\napi_key_env=""\ntrust="always"\nbase_url="http://127.0.0.1:{server.server_port}/v1"\n')
    env = dict(os.environ, XDG_CONFIG_HOME=str(root/'config'),XDG_DATA_HOME=str(root/'data'),XDG_STATE_HOME=str(root/'state'))
    client = None
    process = None
    terminal = None
    try:
        if mode == "web-worker":
            fixture_spec = importlib.util.spec_from_file_location('serve_smoke', Path(__file__).with_name('smoke-serve.py'))
            fixture = importlib.util.module_from_spec(fixture_spec)
            fixture_spec.loader.exec_module(fixture)
            path = root/'serve.sock'
            process = subprocess.Popen([str(BINARY),'serve','--socket',str(path)],cwd=root,env=env,stdout=subprocess.DEVNULL,stderr=subprocess.PIPE,text=True)
            until(path.exists)
            client = fixture.Client(path)
            client.send({'t':'prompt','text':'PROCESS_NOTIFY_DEMO'})
            client.until_done()
        elif mode in ("gc-tui", "glass-tui"):
            from pty_vt import Terminal
            selected = os.environ.get('GLASS_TEST_BINARY') if mode == "glass-tui" else str(BINARY)
            assert selected, 'GLASS_TEST_BINARY is required for the Glass terminal check'
            previous = os.environ.get('GLASS_CORE_TEST_BINARY')
            os.environ['GLASS_CORE_TEST_BINARY'] = selected
            try:
                terminal = Terminal(root,dict(env,GLASS_CORE_BINARY=str(BINARY)),(80,30))
            finally:
                if previous is None:
                    os.environ.pop('GLASS_CORE_TEST_BINARY',None)
                else:
                    os.environ['GLASS_CORE_TEST_BINARY'] = previous
            terminal.collect(3)
            terminal.send('PROCESS_NOTIFY_DEMO\r')
            until(lambda: len(server.requests) >= 1)
        elif headless:
            process = subprocess.Popen([str(BINARY),'run','--json','PROCESS_NOTIFY_DEMO'],cwd=root,env=env,stdout=subprocess.PIPE,stderr=subprocess.PIPE,text=True)
            until(lambda: len(server.requests) >= 1)
        else:
            client = host.RuntimeClient(BINARY,cwd=root,env=env)
            def call(method, params):
                return client.wait(client.request(method,params))
            call('initialize', {'protocolVersion':2,'requiredCapabilities':['mixedTools'],'storageDir':str(root/'sessions')})
            created = call('session/new', {'cwd':str(root),'toolMode':'mixed','tools':[]})
            session = {'sessionId':created['sessionId']}
            call('turn/prompt', {**session,'text':'NESTED_PARENT' if nested else 'PROCESS_NOTIFY_DEMO'})
        # The finite process cannot finish until released by the fixture host.
        # No provider requests should be made while it is waiting.
        if nested:
            until(lambda: len(server.requests) >= 3)
        time.sleep(0.4)
        before = len(server.requests)
        time.sleep(0.4)
        assert len(server.requests) == before, 'provider called while waiting'
        assert server.polls == 0, 'model was allowed to poll'
        (root/'release-process').touch()
        if mode == "web-worker":
            text, _ = client.until_done()
            assert 'PROCESS_RESUMED_OK' in text, text
        elif terminal:
            terminal.wait('PROCESS_RESUMED_OK',30)
            Path(f'/tmp/process-notify-{mode}.ansi').write_text(terminal.seen)
        elif headless:
            stdout, stderr = process.communicate(timeout=30)
            assert process.returncode == 0, stderr
            assert 'PROCESS_RESUMED_OK' in stdout, stdout
        else:
            expected = 'PARENT_RESUMED_OK' if nested else 'PROCESS_RESUMED_OK'
            deadline = time.monotonic() + 30
            observed = ''
            while expected not in observed:
                assert time.monotonic() < deadline, observed
                value = client.receive(timeout=30)
                observed += json.dumps(value)
        assert server.polls == 0
        if not nested:
            assert len(server.requests) == 2, len(server.requests)
        print(f'PASS {root.name}: no waiting requests or model polls; automatic same-conversation resumption ({len(server.requests)} provider requests)')
    finally:
        if terminal:
            terminal.close()
        if client:
            client.close()
        if process and process.poll() is None:
            process.terminate()
            process.wait(timeout=5)
        server.shutdown()
        server.server_close()

with tempfile.TemporaryDirectory(prefix='gc-process-notify-') as temporary:
    base = Path(temporary).resolve()
    for mode in ('runtime','headless','nested','web-worker','gc-tui','glass-tui'):
        run_case(base,mode)
