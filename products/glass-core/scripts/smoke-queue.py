#!/usr/bin/env python3
"""Release PTY durable queue and modal checks at desktop/mobile sizes."""
import http.server
import json
import os
from pathlib import Path
import tempfile
import threading
import time
from pty_vt import Terminal
from core_binary import core_binary_path

BIN = Path(os.environ.get('GLASS_CORE_TEST_BINARY', core_binary_path('release'))).resolve()
os.environ['GLASS_CORE_TEST_BINARY'] = str(BIN)

class Provider(http.server.BaseHTTPRequestHandler):
    def log_message(self, *_): pass
    def do_POST(self):
        body = json.loads(self.rfile.read(int(self.headers['Content-Length'])))
        user = [m['content'] for m in body['messages'] if m['role'] == 'user'][-1]
        self.server.prompts.append(user)
        self.server.started.set()
        if user == 'first': time.sleep(2)
        payload = ('data: '+json.dumps({'choices':[{'delta':{'content':'DONE '+user},'finish_reason':'stop'}]})+'\n\ndata: [DONE]\n\n').encode()
        self.send_response(200)
        self.send_header('Content-Type','text/event-stream')
        self.end_headers()
        self.wfile.write(payload)

for size in [(80,30),(40,24),(20,8)]:
    with tempfile.TemporaryDirectory(prefix='gc-queue-pty-') as tmp:
        root=Path(tmp)
        config=root/'config/glass-core'
        config.mkdir(parents=True)
        server=http.server.ThreadingHTTPServer(('127.0.0.1',0),Provider)
        server.prompts=[]
        server.started=threading.Event()
        threading.Thread(target=server.serve_forever,daemon=True).start()
        (config/'config.toml').write_text(f'provider="openai"\nmodel="mock"\napi_key_env=""\nbase_url="http://127.0.0.1:{server.server_port}/v1"\n')
        env=dict(os.environ,XDG_CONFIG_HOME=str(root/'config'),XDG_DATA_HOME=str(root/'data'))
        term=Terminal(root,env,size)
        try:
            term.idle()
            term.send('first\r')
            assert server.started.wait(8)
            term.send('second\x1b\r')
            term.wait('queued',8)
            files=list((root/'data').rglob('*.queue.json'))
            assert len(files)==1,files
            assert json.loads(files[0].read_text())['pending']==['second']
            term.wait('DONE second',15)
            term.collect(.3)
            assert server.prompts==['first','second'],server.prompts
            assert json.loads(files[0].read_text())['pending']==[]
            log=files[0].with_suffix('.jsonl')
            # queue.json -> session.jsonl
            log=Path(str(files[0]).replace('.queue.json','.jsonl'))
            rows=[json.loads(x) for x in log.read_text().splitlines()]
            assert any(r.get('c')=='queue/delivery' for r in rows),rows
            term.send('/queue\r')
            term.wait('Follow-up queue',8)
            term.send('\x1b')
            term.collect(.3)
            term.send('third\r')
            term.wait('DONE third',10)
            print('PASS queue persistence, receipts, modal Esc and follow-up',size,flush=True)
        finally:
            term.close()
            server.shutdown()
