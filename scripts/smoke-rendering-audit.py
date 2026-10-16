#!/usr/bin/env python3
"""Actual release PTYs: native diagrams, explicit fallbacks, Markdown and replay."""
import http.server
import json
import os
from pathlib import Path
import re
import tempfile
import threading
from pty_vt import Terminal, VT

ARCHITECTURE='''flowchart TD
 %% ===== Entry points =====
 Entry["RootReady<br/>src/main.rs"]:::entry
 Serve["ServeReady<br/>Unix socket (0600)<br/>JSONL protocol"]:::entry
 Attach["AttachReady<br/>terminal client"]:::entry
 subgraph Crate["Rust workspace crates"]
  direction TB
  Core["CoreReady"]:::core
  Tui["TuiReady"]:::ui
  subgraph Engine["Agent engine"]
   Loop["LoopReady &amp; tools"]
  end
 end
 Entry --> Core & Tui
 Core --> Loop --> Serve
 Serve -.-> Attach
 classDef entry fill:#333,stroke:#888;
'''
CASES=[
 ('simple','flowchart TD\nA[RootReady] --> B[EndReady]',True,True),
 ('branches','flowchart TD\nA[RootReady] --> B{Is it mermaid?}\nB -->|yes| C[Output diagram]\nB -->|no| D[Convert to mermaid]\nD --> C\nC --> E[Done]',True,True),
 ('screenshot',ARCHITECTURE,True,True),
 ('live-kilo',(Path(__file__).parent/'fixtures/mermaid-architecture-kilo.mmd').read_text().replace('glass-core binary','RootReady').replace('ModAPI["glass-mod-api','ModAPI["RootReady glass-mod-api'),True,True),
 ('cycle','flowchart TD\nRootReady --> Other\nOther --> RootReady',True,True),
 ('chain','graph LR\nA[RootReady] --> B --> C & D\nD ==> E',True,True),
 ('quoted','flowchart TD\nA["RootReady --> ; %% :::"]:::entry\nA --> B["界 &quot;EndReady&quot;"]',True,True),
 ('sequence','sequenceDiagram\nparticipant A as RootReady\nparticipant B as Bob 界\nloop checks\nA->>B: request\nB-->>A: reply\nNote over A,B: context\nend',True,True),
 ('state','stateDiagram-v2\n[*] --> RootReady\nRootReady --> EndReady: finish',True,True),
 ('class','classDiagram\nRootReady <|-- EndReady\nRootReady *-- Data',True,True),
 ('er','erDiagram\nRootReady ||--o{ EndReady : contains',True,True),
 ('unsupported','packet-beta\n0-3: "RootReady"',True,False),
 ('malformed','flowchart TD\nsubgraph Unclosed\nA[RootReady]',True,False),
 ('disabled','flowchart TD\nA[RootReady] --> B',False,False),
]
CASES += [
 ('pie','pie showData\n title RootReady\n "First": 30\n "Second": 70',True,True),
 ('xy','xychart-beta\n title RootReady\n x-axis [Jan, Feb, Mar]\n y-axis "Count" 0 --> 100\n bar [10, 20, 30]\n line [5, 15, 25]',True,True),
 ('timeline','timeline\n title RootReady\n section Build\n 2025 : Coded : Tested\n : Shipped\n 2026 : Improved',True,True),
 ('mindmap','mindmap\n  root((RootReady))\n    First\n      Child\n    Second',True,True),
 ('journey','journey\n title RootReady\n section Start\n Login: 3: User, Service\n Read: 5: User',True,True),
 ('quadrant','quadrantChart\n title RootReady\n x-axis Low --> High\n y-axis Low --> High\n quadrant-1 Expand\n Alpha: [0.2, 0.7]\n Beta: [0.8, 0.2]',True,True),
 ('gantt','gantt\n title RootReady\n dateFormat YYYY-MM-DD\n section Build\n Code :done, code, 2026-10-16, 3d\n Review :crit, review, after code, 2d\n Ship :milestone, ship, after review, 0d',True,True),
 ('sankey','sankey-beta\nRootReady,Storage,10\n"Storage, primary",Output,5',True,True),
]

MARKDOWN='''# RootReady

__BoldReady__ _ItalicReady_ ~~OldReady~~ ***BothReady***

[GuideReady](https://example.test/guide) ![PreviewReady](image.png)

> Quoted text
> > Nested quote

1) Numbered item
    Indented continuation
- [x] Checked
  - Nested bullet

| File | Count |
| :--- | ---: |
| `src/main.rs` | **42** |

~~~rust
let CodeReady = "界";
~~~
'''
class Server(http.server.ThreadingHTTPServer):daemon_threads=True
class Provider(http.server.BaseHTTPRequestHandler):
 def log_message(self,*_):pass
 def do_POST(self):
  self.rfile.read(int(self.headers['Content-Length']));self.server.calls+=1
  self.send_response(200);self.send_header('Content-Type','text/event-stream');self.end_headers()
  def emit(text):
   for at in range(0,len(text),7):
    self.wfile.write(('data: '+json.dumps({'choices':[{'delta':{'content':text[at:at+7]}}]})+'\n\n').encode());self.wfile.flush()
  try:
   emit(self.server.early)
   assert self.server.gate.wait(12),'host never observed the streamed block'
   emit('\nAfter AUDITDONE')
   self.wfile.write(b'data: [DONE]\n\n');self.wfile.flush()
  except (BrokenPipeError,ConnectionResetError):pass

def plain(text):
 return re.sub(r'\x1b\][^\x07]*(?:\x07|\x1b\\)','',re.sub(r'\x1b\[[0-?]*[ -/]*[@-~]','',text))

def run(name,body,enabled,native,size,markdown=False):
 with tempfile.TemporaryDirectory(prefix='gc-render-audit-') as tmp:
  root=Path(tmp);cfg=root/'config/glass-core';cfg.mkdir(parents=True)
  server=Server(('127.0.0.1',0),Provider);server.calls=0;server.gate=threading.Event()
  # Omitting mermaid proves the new default, rather than enabling only the fixture.
  setting='' if enabled else 'mermaid=false\n'
  (cfg/'config.toml').write_text(f'provider="openai"\nmodel="mock"\napi_key_env=""\nbase_url="http://127.0.0.1:{server.server_port}/v1"\n{setting}')
  server.early='Before\n\n'+(body if markdown else '```mermaid\n'+body+'\n```\n')
  threading.Thread(target=server.serve_forever,daemon=True).start()
  env=dict(os.environ,XDG_CONFIG_HOME=str(root/'config'),XDG_DATA_HOME=str(root/'data'));env.pop('NO_COLOR',None)
  t=Terminal(root,env,size)
  try:
   t.idle();t.send('render fixture\r')
   t.wait('packet-beta' if name=='unsupported' else 'RootReady',12);t.collect(.1)
   rendered=plain(t.seen)
   if not markdown:
    raw=body.splitlines()[0]
    if native:
     assert raw not in rendered,(name,size,rendered)
     assert '<br' not in rendered and 'classDef' not in rendered,(name,size,rendered)
     if name=='live-kilo':assert '╭─ [ModAPI]' in rendered,(name,size,rendered)
    elif enabled:
     assert 'Diagramsource' in ''.join(rendered.split()),(name,size,rendered)
   assert not server.gate.is_set(),'native block must arrive before final response text'
   server.gate.set();t.wait('AUDITDONE',12);t.wait('Context: request',12);t.collect(.15)
   source=server.early+'\nAfter AUDITDONE'
   records=[json.loads(line) for path in (root/'data/glass-core').glob('*.jsonl') if path.stem!='history' for line in path.read_text().splitlines()]
   assert any(r.get('k')=='assistant' and r.get('t')==source for r in records),(name,'source changed')
   t.send('\x06');t.collect(.1);t.send('RootReady\r');t.collect(.2);t.visible('RootReady')
   if native and not markdown:assert body.splitlines()[0] not in t.vt.text(),(name,size,t.vt.text())
   Path(f'/tmp/gc-render-audit-{name}-{size[0]}x{size[1]}.txt').write_text(t.vt.text())
   t.send('\x1b');t.collect(.05);t.send('\x1b');t.collect(.05);t.send('draft');t.visible('draft')
   assert server.calls==1,(name,'extra model calls')
   print('PASS',name,size,flush=True)
  except Exception:
   Path(f'/tmp/gc-render-audit-failed-{name}-{size[0]}x{size[1]}.ansi').write_text(t.seen)
   raise
  finally:
   server.gate.set();t.close();server.shutdown();server.server_close()

if __name__ == '__main__':
 vt=VT(8,2);vt.feed('界');assert vt.text().splitlines()[0]=='界      '
 vt.feed('\r ');assert vt.text().splitlines()[0]=='        '
 for size in [(80,30),(40,24),(20,8)]:
  for case in CASES:run(*case,size)
  run('markdown',MARKDOWN,True,True,size,markdown=True)
 print(f'PASS: {(len(CASES)+1)*3} release PTY rendering cases; thirteen bounded chart families, explicit source fallback, Markdown, exact logs and drafts')
