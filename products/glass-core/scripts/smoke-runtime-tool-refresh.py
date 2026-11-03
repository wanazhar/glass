#!/usr/bin/env python3
"""Verify resumed mixed catalogs refresh without changing history or ownership."""
import http.server, importlib.util, json, os, pathlib, tempfile, threading
from core_binary import core_binary_path
ROOT = pathlib.Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location('runtime_host', ROOT/'examples/runtime_host.py')
host = importlib.util.module_from_spec(spec); spec.loader.exec_module(host)
BINARY = pathlib.Path(os.environ.get('GLASS_CORE_TEST_BINARY', core_binary_path('debug'))).resolve()
class Endpoint(http.server.BaseHTTPRequestHandler):
    counts = []
    def log_message(self, *_): pass
    def do_POST(self):
        body = json.loads(self.rfile.read(int(self.headers['Content-Length'])))
        self.counts.append(len(body.get('tools', [])))
        self.send_response(200); self.send_header('Content-Type', 'text/event-stream'); self.end_headers()
        self.wfile.write(b'data: {"choices":[{"delta":{"content":"HELLO_REFRESH_OK"}}]}\n\ndata: [DONE]\n\n')
def catalog(n):
    return [{'name':f'glass.extra_{i}','description':'Fixture integration','inputSchema':{'type':'object','properties':{}}} for i in range(n)]
with tempfile.TemporaryDirectory(prefix='gc-refresh-') as d:
    root = pathlib.Path(d); cfg = root/'config/glass-core'; cfg.mkdir(parents=True)
    server = http.server.ThreadingHTTPServer(('127.0.0.1',0), Endpoint)
    threading.Thread(target=server.serve_forever, daemon=True).start()
    (cfg/'config.toml').write_text(f'provider="openai"\nmodel="mock"\napi_key_env=""\ntrust="always"\nbase_url="http://127.0.0.1:{server.server_port}/v1"\n')
    env = dict(os.environ, XDG_CONFIG_HOME=str(root/'config'), XDG_DATA_HOME=str(root/'data'))
    client = host.RuntimeClient(BINARY, cwd=root, env=env)
    def call(method, params): return client.wait(client.request(method, params))
    try:
        call('initialize', {'protocolVersion':2,'requiredCapabilities':['mixedTools'],'storageDir':str(root/'sessions')})
        created = call('session/new', {'cwd':str(root),'toolMode':'mixed','tools':catalog(150)})
        session = {'sessionId':created['sessionId']}
        call('session/close', session)
        call('session/resume', {**session,'expectedToolMode':'mixed'})
        before = call('session/inspect', session)['recordCount']
        call('session/tools', {**session,'tools':catalog(50)})
        description = call('session/describe', session)
        assert description['toolMode'] == 'mixed', description
        assert call('session/inspect', session)['recordCount'] == before
        try:
            call('session/tools', {**session,'tools':[{'name':'bash','inputSchema':{'type':'object'}}]})
            raise AssertionError('native tool collision accepted')
        except RuntimeError: pass
        call('turn/prompt', {**session,'text':'hello'})
        assert Endpoint.counts and max(Endpoint.counts) <= 128, Endpoint.counts
        after = call('session/inspect', session)['recordCount']
        assert after > before
        call('session/close', session)
        call('session/resume', {**session,'expectedToolMode':'mixed'})
        call('session/tools', {**session,'tools':catalog(50)})
        assert call('session/inspect', session)['recordCount'] == after
        print(f'PASS resumed 150-tool catalog -> 50 host tools; provider counts {Endpoint.counts}; history/ownership preserved; native collision rejected')
    finally:
        client.close(); server.shutdown(); server.server_close()
