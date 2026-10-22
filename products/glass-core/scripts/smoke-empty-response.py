#!/usr/bin/env python3
"""Empty provider output diagnostics, bounded reasoning recovery and red replay."""
import http.server,json,os,subprocess,tempfile,threading,time
from pathlib import Path
from pty_vt import Terminal
from core_binary import core_binary_path
BIN=Path(os.environ.get('GLASS_CORE_TEST_BINARY',core_binary_path('debug'))).resolve()
class Server(http.server.ThreadingHTTPServer):daemon_threads=True
class Endpoint(http.server.BaseHTTPRequestHandler):
 def log_message(self,*args):pass
 def do_POST(self):
  s=self.server;b=json.loads(self.rfile.read(int(self.headers['Content-Length'])));s.requests.append(b);step=len(s.requests)
  self.send_response(200);self.send_header('Content-Type','text/event-stream');self.end_headers()
  def emit(v):self.wfile.write(('data: '+json.dumps(v)+'\n\n').encode())
  delta={};finish='stop'
  if s.mode=='recover' and step==1:
   delta={'tool_calls':[{'index':0,'id':'once','function':{'name':'bash','arguments':json.dumps({'command':'printf x >> once.txt'})}}]};finish='tool_calls'
  elif s.mode in ('recover','reasoning') and (s.mode=='reasoning' or step==2):delta={'reasoning_content':'PRIVATE_THOUGHT'}
  elif s.mode=='recover':delta={'content':'RECOVERED_ANSWER'}
  emit({'id':'empty-response-id','model':'wire-model','choices':[{'delta':delta,'finish_reason':finish}]})
  emit({'choices':[],'usage':{'prompt_tokens':30,'completion_tokens':10}})
  self.wfile.write(b'data: [DONE]\n\n');self.wfile.flush()
def run(mode,size=None):
 with tempfile.TemporaryDirectory(prefix='gc-empty-') as tmp:
  root=Path(tmp);cfg=root/'config/glass-core';cfg.mkdir(parents=True)
  s=Server(('127.0.0.1',0),Endpoint);s.mode=mode;s.requests=[];threading.Thread(target=s.serve_forever,daemon=True).start()
  (cfg/'config.toml').write_text(f'provider="openai"\nmodel="mock"\napi_key_env=""\nbase_url="http://127.0.0.1:{s.server_port}/v1"\ntrust="always"\n')
  env=dict(os.environ,XDG_CONFIG_HOME=str(root/'config'),XDG_DATA_HOME=str(root/'data'))
  env.pop('NO_COLOR',None)
  try:
   if size:
    t=Terminal(root,env,size)
    try:
     t.idle();t.send('exercise empty response\r');t.wait('/retry: Retry',10);t.collect(.2)
     assert t.seen.count('error: transport:')==1,t.seen
     t.send('\x1b[5~');t.collect(.3)
     assert 'finish=stop' in t.seen,t.seen
     # The error remains bright red in transcript replay, not dim metadata.
     red=[c for row in t.vt.active.buffer.values() for c in row.values() if c.fg in ('red','cd0000','ff5555','f38ba8','f7768e') and c.data.strip()]
     assert red,(t.vt.text(),{c.fg for row in t.vt.active.buffer.values() for c in row.values() if c.data.strip()})
     t.send('\x1b');t.collect(.2);t.send('/context\r');t.collect(.2);t.send('\r');t.collect(.3)
     assert 'Latest' in t.seen and 'provider' in t.seen,t.vt.text()
     t.send('\x1b');t.collect(.1)
    finally:t.close()
   else:
    p=subprocess.run([str(BIN),'run','--json','exercise empty response'],cwd=root,env=env,text=True,capture_output=True,timeout=20)
    if mode=='recover':
     assert p.returncode==0 and 'RECOVERED_ANSWER' in p.stdout,p.stdout+p.stderr
     assert (root/'once.txt').read_text()=='x';assert len(s.requests)==3
     assert 'preceding response contained reasoning' in json.dumps(s.requests[-1])
    else:
     assert p.returncode!=0 and 'finish=stop' in p.stdout and 'wire-model' in p.stdout,p.stdout
     assert 'PRIVATE_THOUGHT' not in p.stdout
     assert len(s.requests)==(2 if mode=='reasoning' else 1),s.requests
    owner=next((root/'data/glass-core').glob('*.jsonl')).stem
    p=subprocess.run([str(BIN),'context',owner],cwd=root,env=env,text=True,capture_output=True,timeout=10)
    if mode!='recover':assert 'Latest provider failure' in p.stdout and 'empty-response-id' in p.stdout,p.stdout
   print('PASS',mode,size or 'headless',flush=True)
  finally:s.shutdown();s.server_close()
for mode in ['empty','reasoning','recover']:run(mode)
for size in [(80,24),(40,24),(20,8)]:run('empty',size)
