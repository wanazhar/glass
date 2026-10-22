#!/usr/bin/env python3
"""Live search keeps a later selected match when streaming appends entries."""
import http.server,json,os,tempfile,threading
from pathlib import Path
from pty_vt import Terminal
class Provider(http.server.BaseHTTPRequestHandler):
    def log_message(self,*_):pass
    def do_POST(self):
        self.rfile.read(int(self.headers['Content-Length']))
        self.send_response(200);self.send_header('Content-Type','text/event-stream');self.end_headers()
        def emit(text):
            self.wfile.write(('data: '+json.dumps({'choices':[{'delta':{'content':text}}]})+'\n\n').encode());self.wfile.flush()
        emit('STREAM_STARTED\n')
        self.server.gate.wait(15)
        emit(' appended live content')
        self.wfile.write(b'data: [DONE]\n\n');self.wfile.flush();self.server.sent.set()
for size in [(80,30),(40,24),(20,8)]:
    with tempfile.TemporaryDirectory(prefix='gc-stream-search-') as tmp:
        root=Path(tmp);cfg=root/'config/glass-core';cfg.mkdir(parents=True)
        data=root/'data/glass-core';data.mkdir(parents=True)
        records=[{'k':'user','t':'FIRST_HIT'},{'k':'assistant','t':'first filler\n'*60},{'k':'user','t':'SECOND_HIT'},{'k':'assistant','t':'second filler\n'*60}]
        (data/'search-fixture.jsonl').write_text(''.join(json.dumps(r)+'\n' for r in records))
        server=http.server.ThreadingHTTPServer(('127.0.0.1',0),Provider);server.gate=threading.Event();server.sent=threading.Event()
        threading.Thread(target=server.serve_forever,daemon=True).start()
        (cfg/'config.toml').write_text(f'provider="openai"\nmodel="fixture"\napi_key_env=""\nbase_url="http://127.0.0.1:{server.server_port}/v1"\n')
        env=dict(os.environ,XDG_CONFIG_HOME=str(root/'config'),XDG_DATA_HOME=str(root/'data'))
        terminal=Terminal(root,env,size,args=('resume','search-fixture'))
        try:
            terminal.idle();terminal.send('stream now\r');terminal.wait('STREAM_STARTED',10)
            terminal.send('\x06HIT\r');terminal.collect(.3);terminal.visible('SECOND_HIT')
            server.gate.set();assert server.sent.wait(10);terminal.collect(.7)
            assert 'SECOND_HIT' in terminal.vt.text() and 'FIRST_HIT' not in terminal.vt.text(),terminal.vt.text()
            print('PASS selected live search survives deltas and completion',size,flush=True)
        finally:server.gate.set();terminal.close();server.shutdown();server.server_close()
