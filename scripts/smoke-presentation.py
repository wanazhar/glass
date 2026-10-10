#!/usr/bin/env python3
"""Actual cell colors through cursor-scoped surface writes; stable scroll summaries."""
import http.server,json,os,tempfile,threading,time
from pathlib import Path
from pty_vt import Terminal
class Server(http.server.ThreadingHTTPServer):daemon_threads=True
class Provider(http.server.BaseHTTPRequestHandler):
 def log_message(self,*_):pass
 def do_POST(self):
  s=self.server;self.rfile.read(int(self.headers['Content-Length']));s.step+=1
  self.send_response(200);self.send_header('Content-Type','text/event-stream');self.end_headers()
  def emit(delta):self.wfile.write(('data: '+json.dumps({'choices':[{'delta':delta}]})+'\n\n').encode());self.wfile.flush()
  if s.step==1:
   emit({'reasoning_content':'checking the sample'})
   emit({'tool_calls':[{'index':0,'id':'call-secret-id','function':{'name':'bash','arguments':json.dumps({'command':'printf TOOLRESULT'})}}]})
  else:
   s.waiting.set();s.tool_gate.wait(10)
   emit({'content':'## Results\n\n| Crate | Lines |\n| --- | --- |\n| `src/lib.rs` | **READYROW** |\n'})
   s.text_gate.wait(10)
   emit({'content':'\nFINALDONE'})
  self.wfile.write(b'data: {"choices":[],"usage":{"prompt_tokens":120,"completion_tokens":20}}\n\ndata: [DONE]\n\n');self.wfile.flush()
def fg(t,needle):
 for row in t.vt.active.buffer.values():
  text=''.join(row[x].data for x in sorted(row))
  pos=text.find(needle)
  if pos>=0:return row[pos].fg
 raise AssertionError((needle,t.vt.text()))
for size in [(80,30),(40,24),(20,8)]:
 with tempfile.TemporaryDirectory(prefix='gc-style-') as tmp:
  root=Path(tmp);cfg=root/'config/glass-core';cfg.mkdir(parents=True)
  s=Server(('127.0.0.1',0),Provider);s.step=0;s.waiting=threading.Event();s.tool_gate=threading.Event();s.text_gate=threading.Event();threading.Thread(target=s.serve_forever,daemon=True).start()
  (cfg/'config.toml').write_text(f'provider="openai"\nmodel="mock"\napi_key_env=""\nbase_url="http://127.0.0.1:{s.server_port}/v1"\ntrust="always"\n')
  env=dict(os.environ,XDG_CONFIG_HOME=str(root/'config'),XDG_DATA_HOME=str(root/'data'));env.pop('NO_COLOR',None)
  t=Terminal(root,env,size)
  try:
   t.idle();t.send('show styles\r');assert s.waiting.wait(10);t.collect(.2)
   if size[1]>8:
    colors=[fg(t,k) for k in ['✓ bash','$ printf','└ TOOLRESULT']]
    assert len(set(colors))==3,(colors,t.vt.text())
   s.tool_gate.set();t.visible('READYROW',10);t.collect(.1)
   assert fg(t,'READYROW')!='default',t.vt.text()
   s.text_gate.set();t.wait('Context: request',10);t.collect(.2)
   data=root/'data/glass-core';records=[json.loads(l) for p in data.glob('*.jsonl') if p.stem!='history' for l in p.read_text().splitlines()]
   assert any(r.get('k')=='meta' and r.get('t','').startswith('Context: request') for r in records)
   t.send('\x1b[5~');t.collect(.2);t.send('\x1b[F');t.collect(.2)
   assert 'Tool call-secret-id' not in t.vt.text(),t.vt.text()
   assert 'Enter expand' not in t.vt.text(),t.vt.text()
   if size[1]>8:assert 'cost unavailable' in t.vt.text() and ('Context:' in t.vt.text() or size[0]>=80),t.vt.text()
   t.send('\x1b');t.collect(.2);t.send('draft');t.collect(.2);assert 'draft' in t.vt.text(),t.vt.text()
   print('PASS colors, table, summary, clean scroll footer, composer',size,flush=True)
  finally:s.tool_gate.set();s.text_gate.set();t.close();s.shutdown();s.server_close()
