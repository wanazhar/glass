#!/usr/bin/env python3
"""Real Glass -> Core -> provider-wire regression for bounded host tool discovery."""
import http.server, json, os, pathlib, sys, tempfile, threading, time
ROOT = pathlib.Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT/'products/glass-core/scripts'))
from pty_vt import Terminal
GLASS = pathlib.Path(os.environ.get('GLASS_TEST_BINARY', ROOT/'target/debug/glass')).resolve()
CORE = pathlib.Path(os.environ.get('GLASS_CORE_BINARY', ROOT/'target/debug/glass-core')).resolve()
class Server(http.server.ThreadingHTTPServer): daemon_threads = True
class Endpoint(http.server.BaseHTTPRequestHandler):
    def log_message(self, *_): pass
    def do_POST(self):
        request = json.loads(self.rfile.read(int(self.headers['Content-Length'])))
        self.server.requests.append(request)
        names = [tool['function']['name'] for tool in request.get('tools', [])]
        if len(names) > 128:
            self.send_response(400); self.end_headers(); self.wfile.write(b'{"error":{"message":"maximum number of tools is 128"}}'); return
        routes = {tool['function'].get('description','').split('. Invoke this using')[0].removeprefix('Glass route: '): tool['function']['name'] for tool in request.get('tools', [])}
        assert 'glass.tools.search' in routes and 'glass.tools.call' in routes, routes
        results = [message for message in request['messages'] if message['role'] == 'tool']
        if not results:
            name, arguments = routes['glass.tools.search'], {'query':'glass.workspace.list'}
        elif len(results) == 1:
            assert 'glass.workspace.list' in results[0]['content'], results
            name, arguments = routes['glass.tools.call'], {'name':'glass.workspace.list','arguments':{}}
        else:
            assert not results[-1]["content"].lower().startswith("error:"), results[-1]
            name = None
        delta = {'content':'HELLO_TOOL_LIMIT_OK'} if name is None else {'tool_calls':[{'index':0,'id':f'budget-{len(results)}','function':{'name':name,'arguments':json.dumps(arguments)}}]}
        self.send_response(200); self.send_header('Content-Type','text/event-stream'); self.end_headers()
        self.wfile.write(('data: '+json.dumps({'choices':[{'delta':delta}]})+'\n\ndata: [DONE]\n\n').encode())
        self.wfile.flush()
with tempfile.TemporaryDirectory(prefix='glass-budget-') as d:
    root = pathlib.Path(d); config = root/'config/glass-core'; config.mkdir(parents=True)
    server = Server(('127.0.0.1',0), Endpoint); server.requests = []
    threading.Thread(target=server.serve_forever, daemon=True).start()
    (config/'config.toml').write_text(f'provider="openai"\nmodel="mock"\napi_key_env=""\ntrust="always"\nbase_url="http://127.0.0.1:{server.server_port}/v1"\n')
    env = dict(os.environ, GLASS_CORE_BINARY=str(CORE), GLASS_CORE_TEST_BINARY=str(GLASS), XDG_CONFIG_HOME=str(root/'config'), XDG_DATA_HOME=str(root/'data'), XDG_STATE_HOME=str(root/'state'), XDG_CACHE_HOME=str(root/'cache'))
    os.environ['GLASS_CORE_TEST_BINARY'] = str(GLASS)
    terminal = Terminal(root, env, (100,30))
    try:
        terminal.collect(2)
        terminal.send('hello\r')
        terminal.wait('HELLO_TOOL_LIMIT_OK', 45)
        assert len(server.requests) == 3, len(server.requests)
        counts = [len(request['tools']) for request in server.requests]
        assert max(counts) <= 128, counts
        pathlib.Path('/tmp/glass-tool-budget.ansi').write_text(terminal.bytes)
        print(f'PASS real Glass/Core hello: {counts} tools; discovery and governed call round trip; /tmp/glass-tool-budget.ansi')
    finally:
        terminal.close(); server.shutdown(); server.server_close()
