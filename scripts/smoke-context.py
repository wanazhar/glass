#!/usr/bin/env python3
"""Offline real PTY request provenance, token labels and retained manifests."""
import http.server, json, os, re, subprocess, tempfile, threading, time
from pathlib import Path
from pty_vt import Terminal

class Provider(http.server.BaseHTTPRequestHandler):
    calls = []
    def log_message(self, *_): pass
    def do_POST(self):
        body=json.loads(self.rfile.read(int(self.headers['Content-Length'])))
        type(self).calls.append(body)
        self.send_response(200); self.send_header('Content-Type','text/event-stream'); self.end_headers()
        for frame in [{'choices':[{'delta':{'content':'Hello there!'}}]}, {'choices':[], 'usage':{'prompt_tokens':2668,'completion_tokens':44}}]:
            self.wfile.write(('data: '+json.dumps(frame)+'\n\n').encode())
        self.wfile.write(b'data: [DONE]\n\n'); self.wfile.flush()

for size in [(40,24),(20,8)]:
    with tempfile.TemporaryDirectory(prefix='gc-context-') as tmp:
        root=Path(tmp); cfg=root/'config/glass-core'; cfg.mkdir(parents=True)
        data=root/'data/glass-core'; data.mkdir(parents=True)
        skills=root/'skills'; skills.mkdir()
        (skills/'build.md').write_text('---\nname: build\ndescription: Build project\n---\nSKILL_BODY_SECRET\n')
        instruction=root/'AGENTS.md'; instruction.write_text('INSTRUCTION_SECRET\n')
        server=http.server.ThreadingHTTPServer(('127.0.0.1',0),Provider)
        threading.Thread(target=server.serve_forever,daemon=True).start(); Provider.calls=[]
        (cfg/'config.toml').write_text(f'provider="openai"\nmodel="mock"\napi_key_env=""\ntrust="always"\nskills_dir="{skills}"\nbase_url="http://127.0.0.1:{server.server_port}/v1"\n')
        env=dict(os.environ,XDG_CONFIG_HOME=str(root/'config'),XDG_DATA_HOME=str(root/'data'))
        t=Terminal(root,env,size)
        try:
            t.idle(); t.send('hello\r')
            until=time.monotonic()+8
            def plain(): return ' '.join(re.sub(r'\x1b\[[0-?]*[ -/]*[@-~]', '', t.seen).split())
            while 'session total tokens' not in plain():
                assert time.monotonic()<until,t.vt.text()
                t.collect(.05)
            compact=plain()
            assert '2,668 input' in compact and '44 output' in compact,compact
            assert '2,712 session total' in compact,compact
            manifests=list(data.glob('*.context')); assert len(manifests)==1,manifests
            reports=[json.loads(line) for line in manifests[0].read_text().splitlines()]
            assert len(reports)==1,reports
            report=reports[0]
            assert str(instruction) in report and str(skills/'build.md') in report, report
            assert 'descriptions only' in report and 'bash' in report and '1 user' in report,report
            assert 'INSTRUCTION_SECRET' not in report and 'SKILL_BODY_SECRET' not in report,report
            wire=json.dumps(Provider.calls[0])
            assert 'INSTRUCTION_SECRET' in wire and 'SKILL_BODY_SECRET' not in wire,wire
            assert 'Captured before dispatch' not in wire,wire
            # Captured sources survive edits and are accessible without a terminal.
            instruction.unlink()
            session_path=manifests[0].with_suffix('.jsonl')
            binary=os.environ.get('GLASS_CORE_TEST_BINARY',str(Path(__file__).resolve().parents[1]/'target/debug/glass-core'))
            out=subprocess.check_output([binary,'context',session_path.stem,'--json'],cwd=root,env=env,text=True)
            assert str(instruction) in json.loads(out)['text'],out
            t.send('/context\r'); t.visible('Context')
            t.send('\r'); t.visible('Last request context' if size[0]>20 else 'Last request')
            t.send('\x1b'); t.collect(.1)
            t.send('\x1b[B'*5+'\r'); t.visible('Model requests')
            t.send('\r'); t.visible('Request context')
            t.send('\x1b[F'); t.visible('followed it.')
            t.send('\x1b'); t.collect(.1); t.send('\x1b'); t.collect(.1); t.send('\x1b'); t.collect(.1)
        finally:
            t.close();server.shutdown();server.server_close()
print('PASS: actual request sources, catalogue vs bodies, metadata isolation, token labels, durable headless report and request-history modals at 40x24/20x8')
