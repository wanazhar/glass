#!/usr/bin/env python3
"""Release PTYs for TypeScript previews, native host calls and Charm Dot motion."""
import http.server
import json
import os
from pathlib import Path
import re
import tempfile
import subprocess
import threading
import time
from pty_vt import Terminal
from core_binary import core_binary_path

ROOT = Path(__file__).resolve().parents[1]
ENTRY = ROOT / 'extensions/typescript/dist/index.js'
SDK = ROOT / 'packages/extension-sdk/dist/index.js'
DOT = '⣾⣽⣻⢿⡿⣟⣯⣷'
assert ENTRY.exists() and SDK.exists(), 'Run npm ci --ignore-scripts && npm run build first'
os.environ.setdefault('GLASS_CORE_TEST_BINARY', str(core_binary_path('release')))

class Provider(http.server.BaseHTTPRequestHandler):
    gate = threading.Event()
    requests = []
    mode = 'preview'
    def log_message(self, *_):
        pass
    def do_POST(self):
        body = json.loads(self.rfile.read(int(self.headers['Content-Length'])))
        self.requests.append(body)
        latest = max(i for i, message in enumerate(body['messages']) if message['role'] == 'user')
        results = [message for message in body['messages'][latest + 1:] if message['role'] == 'tool']
        self.send_response(200)
        self.send_header('Content-Type', 'text/event-stream')
        self.end_headers()
        try:
            assert self.gate.wait(15)
            if not results:
                name = 'sdk_host_read' if self.mode == 'host' else 'ts_preview'
                args = {'path': 'fixture.txt'} if self.mode == 'host' else {'before': 'old value', 'after': 'new value'}
                delta = {'tool_calls': [{'index': 0, 'id': 'ts-call', 'function': {'name': name, 'arguments': json.dumps(args)}}]}
            else:
                output = results[-1]['content']
                if self.mode == 'host':
                    assert 'SDK-HOST-CONTENT' in output, output
                else:
                    assert '### Text preview' in output and '### Diagnostics' in output, output
                    assert 'old value' in output and 'new value' in output, output
                delta = {'content': '# Verified\n\n**TypeScript result received.**\n\nTS-SDK-FINISHED'}
            self.wfile.write(('data: ' + json.dumps({'choices': [{'delta': delta}]}) + '\n\ndata: [DONE]\n\n').encode())
            self.wfile.flush()
        except (BrokenPipeError, ConnectionResetError):
            pass

def visible_without_spaces(tty):
    return ''.join(tty.vt.text().split())

def settle(tty, seconds=.25):
    tty.collect(seconds)
    return tty.vt.text()

with tempfile.TemporaryDirectory(prefix='gc-typescript-') as tmp:
    root = Path(tmp)
    cfg = root / 'config/glass-core'
    cfg.mkdir(parents=True)
    server = http.server.ThreadingHTTPServer(('127.0.0.1', 0), Provider)
    threading.Thread(target=server.serve_forever, daemon=True).start()
    env = dict(os.environ, XDG_CONFIG_HOME=str(root / 'config'), XDG_DATA_HOME=str(root / 'data'))
    env.pop('NO_COLOR', None)
    config_path = cfg / 'config.toml'
    def configure(entry):
        command = json.dumps(['node', str(entry)])
        config_path.write_text(f'provider="openai"\nmodel="mock"\napi_key_env=""\ntrust="always"\nbase_url="http://127.0.0.1:{server.server_port}/v1"\nextensions=[{json.dumps(command)}]\n')
    configure(ENTRY)
    try:
        for size in [(20, 8), (40, 24), (80, 30)]:
            Provider.mode = 'preview'
            Provider.gate.clear()
            tty = Terminal(root, env, size)
            try:
                tty.idle()
                tty.send('/animation\r')
                tty.visible('Activity animation')
                before = len(Provider.requests)
                tty.collect(.85)
                recent = tty.bytes
                assert len(set(re.findall(f'[{DOT}]', recent))) >= 4, recent
                assert 'Dot' in tty.vt.text() and 'Esc back' in tty.vt.text(), tty.vt.text()
                assert len(Provider.requests) == before, 'Local preview contacted provider'
                tty.send('\x1b')
                tty.visible('Esc close')
                tty.send('\x1b')
                tty.idle()
                tty.send('/ts-demo\r')
                tty.visible('/ts-demo')
                tty.collect(.2)
                assert 'TypeScript' in tty.vt.text(), tty.vt.text()
                tty.send('\x1b[F')
                tty.collect(.2)
                assert 'Diagnostics' in tty.vt.text() or 'files changed' in tty.vt.text(), tty.vt.text()
                assert '\x1b[38;5;' in tty.bytes, 'Native inspector lost semantic colors'
                tty.send('\x1b')
                tty.visible('Esc close')
                tty.send('\x1b')
                tty.idle()
                tty.send('preview this replacement\r')
                tty.wait('Waiting')
                tty.send('draft survives')
                tty.collect(.8)
                assert len(set(re.findall(f'[{DOT}]', tty.bytes))) >= 3, tty.bytes
                assert 'draft survives' in tty.vt.text(), tty.vt.text()
                assert not re.search('[◐◓◑◒]', tty.bytes), 'Old wheel survived'
                Provider.gate.set()
                tty.wait('TS-SDK-FINISHED')
                tty.collect(.3)
                assert 'draft survives' in tty.vt.text(), tty.vt.text()
                assert not re.search(f'[{DOT}]', tty.bytes), 'Completed turn kept animating'
                logs = sorted((root / 'data/glass-core').glob('*.jsonl'), key=lambda path: path.stat().st_mtime_ns)
                records = [json.loads(line) for line in logs[-1].read_text().splitlines()]
                result = next(record for record in records if record.get('k') == 'tool' and record.get('name') == 'ts_preview')
                assert 'Diagnostics' in str(result), result
                Path(f'/tmp/gc-typescript-{size[0]}x{size[1]}.txt').write_text(tty.vt.text())
                print(f'PASS {size}: native preview, diagnostics, Charm Dot, draft retention, idle stop')
            finally:
                Provider.gate.set()
                tty.close()
        # Verify an SDK callback uses GC's real shell bridge and state records.
        (root / 'fixture.txt').write_text('SDK-HOST-CONTENT\n')
        bridge = root / 'bridge.mjs'
        bridge.write_text(f'''import {{runExtension}} from {json.dumps(SDK.as_uri())};
await runExtension({{name:'sdk-host',version:'1',tools:[{{definition:{{name:'sdk_host_read',description:'Read fixture',schema:{{type:'object'}}}},execute:async(args,ctx)=>{{
const output=await ctx.callTool('bash',{{command:'cat fixture.txt'}}); await ctx.appendCustom('receipt','host read completed');
const entries=await ctx.readCustom('receipt'); return output+'\\n'+entries.join('\\n');
}}}}]}});''')
        configure(bridge)
        Provider.mode = 'host'
        tty = Terminal(root, env, (40, 24))
        try:
            tty.idle()
            tty.send('read fixture\r')
            tty.wait('TS-SDK-FINISHED')
            print('PASS GC host-tool bridge and custom state')
        finally:
            tty.close()
        configure(ENTRY)
        Provider.mode = 'preview'
        headless = subprocess.run([os.environ['GLASS_CORE_TEST_BINARY'], 'run', 'preview replacement'], cwd=root, env=env, capture_output=True, text=True, timeout=20)
        assert headless.returncode == 0 and 'TS-SDK-FINISHED' in headless.stdout, (headless.stdout, headless.stderr)
        print('PASS headless TypeScript tool execution')
    finally:
        server.shutdown()
