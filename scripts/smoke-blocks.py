#!/usr/bin/env python3
"""Native output blocks streamed in small chunks and replayed in a real PTY."""
import http.server, json, os, tempfile, threading, time
from pathlib import Path
from pty_vt import Terminal

EARLY='# Changes\n\n\n| File | Status |\n| --- | --- |\n| `src/lib.rs` | **READYROW** |\n'
TAIL='| 界 Unicode | reviewed |\n\n## Verification\n\n- [x] Build complete\n  - Nested detail\n- [ ] Review pending\n\n```rust\nlet result = 42;\n```\n\nBLOCKDONE'
SOURCE=EARLY+TAIL
class Provider(http.server.BaseHTTPRequestHandler):
    gate=threading.Event()
    def log_message(self,*_): pass
    def do_POST(self):
        self.rfile.read(int(self.headers['Content-Length']))
        self.send_response(200); self.send_header('Content-Type','text/event-stream'); self.end_headers()
        def emit(text):
            for c in text:
                self.wfile.write(('data: '+json.dumps({'choices':[{'delta':{'content':c}}]})+'\n\n').encode()); self.wfile.flush()
        try:
            emit(EARLY)
            assert type(self).gate.wait(15)
            emit(TAIL)
            self.wfile.write(b'data: [DONE]\n\n'); self.wfile.flush()
        except (BrokenPipeError,ConnectionResetError): pass

for size in [(90,30),(40,24),(20,8)]:
    with tempfile.TemporaryDirectory(prefix='gc-blocks-') as tmp:
        root=Path(tmp); cfg=root/'config/glass-core'; cfg.mkdir(parents=True)
        data=root/'data/glass-core'; data.mkdir(parents=True)
        server=http.server.ThreadingHTTPServer(('127.0.0.1',0),Provider)
        threading.Thread(target=server.serve_forever,daemon=True).start(); Provider.gate.clear()
        (cfg/'config.toml').write_text(f'provider="openai"\nmodel="mock"\napi_key_env=""\nbase_url="http://127.0.0.1:{server.server_port}/v1"\n')
        env=dict(os.environ,XDG_CONFIG_HOME=str(root/'config'),XDG_DATA_HOME=str(root/'data')); env.pop('NO_COLOR',None)
        t=Terminal(root,env,size)
        try:
            t.idle();t.send('show structured output\r');t.visible('READYROW')
            assert not Provider.gate.is_set(), 'table must stream before response completion'
            view=t.vt.text()
            if size[0]==90: assert ' │ ' in view,view
            elif size[0]==40: assert 'src/lib.rs' in view and 'Status' in view,view
            else: assert 'Status' in view,view
            Provider.gate.set()
            until=time.monotonic()+8
            while True:
                t.collect(.05)
                records=[json.loads(line) for p in data.glob('*.jsonl') if p.stem!='history' for line in p.read_text().splitlines()]
                if any(r.get('k')=='assistant' and r.get('t')==SOURCE for r in records):break
                assert time.monotonic()<until,t.vt.text()
            until=time.monotonic()+5
            while '╰' not in t.seen:
                assert time.monotonic()<until,t.vt.text()
                t.collect(.05)
            assert '╭─ rust' in t.seen,t.vt.text()
            assert '\x1b[38;5;' in t.seen and '\x1b[1m' in t.seen, 'semantic colors and heading emphasis'
            # Search the parsed table in the same inline reader used for scrolling.
            t.send('\x06');t.collect(.1);t.send('READYROW\r');t.visible('READYROW');t.send('\x1b');t.collect(.1);t.send('\x1b');t.collect(.1)
            t.send('draft');t.collect(.1);t.resize(20,8);t.resize(*size)
            assert 'draft' in t.vt.text(),t.vt.text()
        finally:
            Provider.gate.set();t.close();server.shutdown();server.server_close()
print('PASS: pretty native tables, live row delivery, narrow stacked fields, code frames, source preservation, reader search and resize at 90x30/40x24/20x8')
