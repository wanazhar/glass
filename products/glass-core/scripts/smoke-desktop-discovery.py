#!/usr/bin/env python3
"""Verify standalone and Glass mixed-runtime desktop discovery without a provider account."""
import http.server
import importlib.util
import json
import os
import subprocess
import tempfile
import threading
import time
import urllib.request
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
BINARY = Path(os.environ.get('GLASS_CORE_TEST_BINARY', ROOT.parents[1] / 'target/release/glass-core')).resolve()
def load(name, path):
    spec = importlib.util.spec_from_file_location(name, path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module
host = load('desktop_runtime_host', ROOT / 'examples/runtime_host.py')
bench = load('desktop_bench_cleanup', ROOT / 'scripts/benchmark-desktop-updates.py')
required = {'desktop_status', 'desktop_screenshot', 'desktop_observe', 'desktop_act', 'desktop_input', 'desktop_launch_app'}
failures, observed = [], set()
mode = 'standalone'
class Endpoint(http.server.BaseHTTPRequestHandler):
    def log_message(self, *_):
        pass
    def do_POST(self):
        body = json.loads(self.rfile.read(int(self.headers['Content-Length'])))
        try:
            names = {tool['function']['name'] for tool in body.get('tools', [])}
            assert required <= names, 'Desktop tools missing from model request'
            system = str(body['messages'][0]['content'])
            assert 'OS-level computer use' in system and 'persistent GNOME' in system
            if mode == 'glass':
                assert '## Shared computer use' in system
                assert any('glass.workspace.list' in tool['function'].get('description','') for tool in body['tools'])
            results = [item for item in body['messages'] if item['role'] == 'tool']
            if results:
                assert 'sessionId' in str(results[-1]), 'Native desktop status did not reach the model'
                observed.add(mode)
                delta = {'content': 'DESKTOP_DISCOVERED'}
            else:
                delta = {'tool_calls': [{'index': 0, 'id': 'native-desktop-status', 'function': {'name': 'desktop_status', 'arguments': '{}'}}]}
        except Exception as error:
            failures.append(str(error))
            delta = {'content': 'DISCOVERY_FAILED'}
        payload = ('data: ' + json.dumps({'choices': [{'delta': delta}]}) + '\n\ndata: [DONE]\n\n').encode()
        self.send_response(200)
        self.send_header('Content-Type', 'text/event-stream')
        self.send_header('Content-Length', str(len(payload)))
        self.end_headers()
        self.wfile.write(payload)

def main():
    global mode
    environment = os.environ.get('GC_BENCH_ENVIRONMENT', 'xfce')
    assert environment in ('xfce', 'gnome')
    server = http.server.ThreadingHTTPServer(('127.0.0.1', 0), Endpoint)
    threading.Thread(target=server.serve_forever, daemon=True).start()
    try:
        with tempfile.TemporaryDirectory(prefix='gc-desktop-discovery-') as temporary:
            root = Path(temporary)
            config = root / 'config/glass-core'
            config.mkdir(parents=True)
            (config / 'config.toml').write_text(f'provider="openai"\nmodel="mock"\napi_key_env="GC_DISCOVERY_TEST_KEY"\nbase_url="http://127.0.0.1:{server.server_port}/v1"\ntrust="always"\nadaptive_pacing=false\n')
            env = dict(os.environ, HOME=temporary, XDG_CONFIG_HOME=str(root/'config'), XDG_DATA_HOME=str(root/'data'), GC_DISCOVERY_TEST_KEY='local-test', GC_WEB_ASSETS=str(ROOT/'web/dist'))
            def cli(*args):
                result = subprocess.run([str(BINARY), *args], cwd=root, env=env, capture_output=True, timeout=60)
                assert result.returncode == 0, 'Private Core command failed'
                return result
            cli('connect', '--local')
            client = None
            try:
                state = json.loads((root/'data/glass-core/connect.json').read_text())
                origin = f"http://127.0.0.1:{state['port']}"
                request = urllib.request.Request(origin+'/api/desktop/session', data=json.dumps({'action':'start','environment':environment,'lifecycle':'reusable' if environment=='gnome' else 'disposable'}).encode(), headers={'Authorization':'Bearer '+state['token'], 'Origin':origin,'Content-Type':'application/json'})
                with urllib.request.urlopen(request, timeout=60):
                    pass
                deadline = time.monotonic() + 15
                while True:
                    status_request = urllib.request.Request(origin+'/api/desktop/status', headers={'Authorization':'Bearer '+state['token']})
                    with urllib.request.urlopen(status_request, timeout=5) as response:
                        status = json.load(response)
                    if status['available']:
                        break
                    assert time.monotonic() < deadline, 'Private desktop capture did not become ready'
                    time.sleep(.1)
                cli('run', '--json', 'Check the shared computer capability using desktop_status.')
                assert 'standalone' in observed, failures
                mode = 'glass'
                client = host.RuntimeClient(BINARY, root, env)
                call = lambda name, params: client.wait(client.request(name, params))
                call('initialize', {'protocolVersion':2, 'requiredCapabilities':['mixedTools','maskedSecretInput'], 'storageDir':str(root/'runtime-sessions')})
                instructions = (ROOT.parents[1]/'crates/glass-dev/assets/glass-core-system.md').read_text()
                session = call('session/new', {'cwd':str(root),'toolMode':'mixed','instructions':instructions,'tools':[{'name':'glass.workspace.list','description':'Inspect Glass workspaces','inputSchema':{'type':'object','properties':{}}}]})
                def unexpected_host_call(_):
                    raise AssertionError('Desktop status must execute natively, not through the Glass host router')
                client.wait(client.request('turn/prompt', {**session,'text':'Check the shared computer capability using desktop_status.'}), tool=unexpected_host_call)
                assert observed == {'standalone', 'glass'} and not failures, failures
                print(f'PASS {environment}: standalone GC and Glass mixed runtime advertise all desktop tools, explain OS-level use, and execute native desktop_status')
            finally:
                if client:
                    client.close()
                cli('connect','stop')
                bench.cleanup_namespace(root)
    finally:
        server.shutdown()
        server.server_close()
if __name__ == '__main__':
    main()
