#!/usr/bin/env python3
"""Real non-Git PTY: submit and steer prompts while the inline reader is open."""
import http.server, json, os, tempfile, threading, time
from pathlib import Path
from pty_vt import Terminal

class Endpoint(http.server.BaseHTTPRequestHandler):
    calls = []
    histories = []
    gate = threading.Event()
    def log_message(self, *_): pass
    def do_POST(self):
        body = json.loads(self.rfile.read(int(self.headers['Content-Length'])))
        text = [m['content'] for m in body['messages'] if m['role'] == 'user'][-1]
        type(self).calls.append(text)
        type(self).histories.append(body["messages"])
        self.send_response(200); self.send_header('Content-Type', 'text/event-stream'); self.end_headers()
        def emit(text):
            self.wfile.write(('data: '+json.dumps({'choices':[{'delta':{'content':text}}]})+'\n\n').encode()); self.wfile.flush()
        try:
            if text == 'hello after scroll':
                emit('BUSY')
                assert type(self).gate.wait(15), 'fixture must release first turn'
            emit('REPLYDONE')
            self.wfile.write(b'data: [DONE]\n\n'); self.wfile.flush()
        except (BrokenPipeError, ConnectionResetError): pass

for size in [(40,24), (20,8)]:
    with tempfile.TemporaryDirectory(prefix='gc-scroll-submit-') as tmp:
        root=Path(tmp); cfg=root/'config/glass-core'; cfg.mkdir(parents=True)
        data=root/'data/glass-core'; data.mkdir(parents=True)
        server=http.server.ThreadingHTTPServer(('127.0.0.1',0),Endpoint)
        threading.Thread(target=server.serve_forever,daemon=True).start()
        (cfg/'config.toml').write_text(f'provider="openai"\nmodel="mock"\napi_key_env=""\nbase_url="http://127.0.0.1:{server.server_port}/v1"\n')
        history=[{'k':role,'t':f'HISTORY {i} '+('long text '*20)} for i in range(20) for role in ['user','assistant']]
        (data/'fixture.jsonl').write_text(''.join(json.dumps(r)+'\n' for r in history))
        env=dict(os.environ,XDG_CONFIG_HOME=str(root/'config'),XDG_DATA_HOME=str(root/'data'))
        Endpoint.calls=[]; Endpoint.histories=[]; Endpoint.gate.clear()
        t=Terminal(root,env,size,args=('resume','fixture'))
        try:
            t.idle(); t.send('\x1b[<64;1;1M'); t.collect(.2)
            assert 'Conversation' in t.vt.text() or 'Log' in t.vt.text()
            t.send('hello after scroll\r')
            until=time.monotonic()+8
            while not Endpoint.calls:
                assert time.monotonic()<until, t.vt.text()
                t.collect(.05)
            assert Endpoint.calls == ['hello after scroll'], Endpoint.calls
            t.send('\x1b[<64;1;1M'); t.collect(.2)
            t.send('queued after scroll\r'); t.collect(.3)
            until=time.monotonic()+3
            while len(Endpoint.calls)<2:
                assert time.monotonic()<until, (Endpoint.calls,t.vt.text())
                t.collect(.05)
            assert Endpoint.calls == ['hello after scroll','queued after scroll'], Endpoint.calls
            resumed = Endpoint.histories[1]
            assert any(m['role']=='user' and m['content'].startswith('HISTORY 0 ') for m in resumed), resumed
            assert any(m['role']=='user' and m['content']=='hello after scroll' for m in resumed), resumed
            # The new request must start while the original provider is held.
            Endpoint.gate.set()
            until=time.monotonic()+5
            while True:
                t.collect(.05)
                saved=[json.loads(line) for line in (data/'fixture.jsonl').read_text().splitlines()]
                if any(r.get('k')=='assistant' and 'REPLYDONE' in r.get('t','') for r in saved): break
                assert time.monotonic()<until, (saved,t.vt.text())
            records=[json.loads(line) for line in (data/'fixture.jsonl').read_text().splitlines()]
            users=[r['t'] for r in records if r['k']=='user']
            assert users.count('hello after scroll')==1 and users.count('queued after scroll')==1, users
            assert not (root/'.git').exists(), 'must not initialize a repository'
            assert 'fatal: not a git repository' not in t.seen, t.seen
            assert 'checkpoint unavailable' not in t.seen, t.seen
        finally:
            Endpoint.gate.set(); t.close(); server.shutdown(); server.server_close()
print('PASS: paused-reader Enter submits and steers promptly exactly once; plain directory, no Git initialization or fatal warning; 40x24/20x8')
