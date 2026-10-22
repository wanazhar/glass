#!/usr/bin/env python3
"""Release PTY: Unicode diff truncation and failed review is not a finding."""
import http.server,json,os,subprocess,tempfile,threading
from pathlib import Path
from pty_vt import Terminal
class Endpoint(http.server.BaseHTTPRequestHandler):
    requests=[]
    def log_message(self,*_):pass
    def do_POST(self):
        body=json.loads(self.rfile.read(int(self.headers['Content-Length'])));self.requests.append(body)
        data=b'{"error":{"message":"REVIEW_FAILURE_FIXTURE"}}'
        self.send_response(400);self.send_header('Content-Type','application/json');self.send_header('Content-Length',str(len(data)));self.end_headers();self.wfile.write(data)
with tempfile.TemporaryDirectory(prefix='gc-review-lifecycle-') as temp:
    root=Path(temp);subprocess.run(['git','init','-q',str(root)],check=True)
    (root/'unicode.txt').write_text('original\n')
    subprocess.run(['git','-C',str(root),'add','unicode.txt'],check=True)
    subprocess.run(['git','-C',str(root),'-c','user.name=Fixture','-c','user.email=fixture@local','commit','-qm','fixture'],check=True)
    (root/'unicode.txt').write_text('🦀'*30000+'\n')
    cfg=root/'config/glass-core';cfg.mkdir(parents=True)
    server=http.server.ThreadingHTTPServer(('127.0.0.1',0),Endpoint);threading.Thread(target=server.serve_forever,daemon=True).start()
    (cfg/'config.toml').write_text(f'provider="openai"\nmodel="offline"\napi_key_env=""\nbase_url="http://127.0.0.1:{server.server_port}/v1"\n')
    env=dict(os.environ,XDG_CONFIG_HOME=str(root/'config'),XDG_DATA_HOME=str(root/'data'))
    t=Terminal(root,env,(40,24))
    try:
        t.idle();t.send('/code-review\r');t.wait('REVIEW_FAILURE_FIXTURE',15);t.collect(.3)
        assert t.p.poll() is None,'Unicode diff crashed process'
        assert Endpoint.requests
        request=Endpoint.requests[-1]
        assert 'diff truncated at 98304 bytes' in json.dumps(request,ensure_ascii=False)
        names={tool['function']['name'] for tool in request.get('tools',[])}
        assert 'bash' not in names and 'write' not in names and 'edit' not in names,names
        records=[json.loads(line) for path in (root/'data/glass-core').glob('*.jsonl') for line in path.read_text().splitlines()]
        assert not any(isinstance(r,dict) and r.get('k')=='system' and r.get('t','').startswith('code review findings:') for r in records),'Failed review saved as success'
        print('PASS Unicode review truncation, read-only tool schema and failed review propagation')
    finally:t.close();server.shutdown();server.server_close()
