#!/usr/bin/env python3
"""Exercise real GC -> Glass TUI -> GC resumption against an isolated provider."""
import http.server
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import threading

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / 'products/glass-core/scripts'))
from pty_vt import Terminal

GLASS = Path(os.environ.get('GLASS_TEST_BINARY', ROOT / 'target/debug/glass')).resolve()
CORE = Path(os.environ.get('GLASS_CORE_BINARY', ROOT / 'target/debug/glass-core')).resolve()


class Endpoint(http.server.BaseHTTPRequestHandler):
    def log_message(self, *_):
        pass

    def do_POST(self):
        self.server.requests.append(json.loads(self.rfile.read(int(self.headers['Content-Length']))))
        self.send_response(200)
        self.send_header('Content-Type', 'text/event-stream')
        self.end_headers()
        self.wfile.write(b'data: {"choices":[{"delta":{"content":"SHARED_TUI_APPEND_OK"}}]}\n\ndata: [DONE]\n\n')


with tempfile.TemporaryDirectory(prefix='glass-shared-tui-') as temporary:
    root = Path(temporary).resolve()
    config = root / 'config/glass-core'
    config.mkdir(parents=True)
    data = root / 'data/glass-core'
    data.mkdir(parents=True)
    source = data / 'fixture-native-gc.jsonl'
    seed = b'{"k":"user","t":"PRIOR_GC_QUESTION"}\n{"k":"assistant","t":"PRIOR_GC_ANSWER"}\n'
    source.write_bytes(seed)
    source.with_suffix('.meta.json').write_text(json.dumps({'workspace':str(root), 'title':'Native GC fixture', 'origin':'gc'}))
    server = http.server.ThreadingHTTPServer(('127.0.0.1', 0), Endpoint)
    server.requests = []
    threading.Thread(target=server.serve_forever, daemon=True).start()
    (config / 'config.toml').write_text(f'provider="openai"\nmodel="mock"\napi_key_env=""\ntrust="always"\nbase_url="http://127.0.0.1:{server.server_port}/v1"\n')
    env = dict(os.environ, GLASS_CORE_BINARY=str(CORE), XDG_DATA_HOME=str(root/'data'), XDG_CONFIG_HOME=str(root/'config'), XDG_STATE_HOME=str(root/'state'), XDG_CACHE_HOME=str(root/'cache'))
    os.environ['GLASS_CORE_TEST_BINARY'] = str(GLASS)
    terminal = Terminal(root, env, (100, 35))
    try:
        terminal.collect(3)
        terminal.send('\x1b[84;6u')  # Ctrl-Shift-T: project conversations.
        terminal.wait('Native GC fixture', 20)
        assert 'Created in GC' in terminal.vt.text(), terminal.vt.text()
        terminal.send('\r')
        terminal.collect(.5)
        terminal.send('\x0b')  # Ctrl-K: command palette.
        terminal.collect(.5)
        terminal.send('agent resume agent-0001\r')
        terminal.wait('CONFIRMATION', 15)
        terminal.send('\r')
        terminal.wait('PRIOR_GC_ANSWER', 20)
        terminal.visible('CORE  Idle', 20)
        terminal.send('Continue the previous question\r')
        terminal.wait('SHARED_TUI_APPEND_OK', 45)
        Path('/tmp/glass-shared-chat-tui.ansi').write_text(terminal.seen)
        assert source.read_bytes().startswith(seed)
        assert not list((root/'.glass/agent-sessions').glob('*.jsonl')), 'Glass created a replacement log'
        assert len(server.requests) == 1
        messages = json.dumps(server.requests[0]['messages'])
        assert 'PRIOR_GC_QUESTION' in messages and 'PRIOR_GC_ANSWER' in messages
    finally:
        terminal.close()
    try:
        result = subprocess.run([str(CORE), 'run', '--json', '--resume', str(source), 'Continue after Glass'], cwd=root, env=env, capture_output=True, text=True, timeout=45)
        assert result.returncode == 0, result.stderr
        assert 'SHARED_TUI_APPEND_OK' in result.stdout
        records = [json.loads(line) for line in source.read_text().splitlines()]
        assert sum(record.get('t') == 'Continue the previous question' for record in records) == 1
        assert sum(record.get('t') == 'Continue after Glass' for record in records) == 1
        assert source.read_bytes().startswith(seed)
        assert json.loads(source.with_suffix('.meta.json').read_text())['origin'] == 'gc'
        second_project = root / 'glass-origin-project'
        second_project.mkdir()
        glass_terminal = Terminal(second_project, env, (100, 35))
        try:
            glass_terminal.collect(3)
            glass_terminal.send('Create this conversation in Glass\r')
            glass_terminal.wait('SHARED_TUI_APPEND_OK', 45)
        finally:
            glass_terminal.close()
        glass_logs = [path for path in (second_project/'.glass/agent-sessions').glob('*.jsonl') if path.with_suffix('.meta.json').exists()]
        assert len(glass_logs) == 1, glass_logs
        glass_source = glass_logs[0]
        prefix = glass_source.read_bytes()
        assert json.loads(glass_source.with_suffix('.meta.json').read_text())['origin'] == 'glass'
        resumed = subprocess.run([str(CORE), 'run', '--json', '--resume', str(glass_source), 'Resume the Glass conversation in GC'], cwd=second_project, env=env, capture_output=True, text=True, timeout=45)
        assert resumed.returncode == 0, resumed.stderr
        assert 'SHARED_TUI_APPEND_OK' in resumed.stdout
        assert glass_source.read_bytes().startswith(prefix)
        assert json.loads(glass_source.with_suffix('.meta.json').read_text())['origin'] == 'glass'
        records = [json.loads(line) for line in glass_source.read_text().splitlines()]
        assert sum(record.get('t') == 'Resume the Glass conversation in GC' for record in records) == 1
        print('PASS real GC -> Glass TUI -> GC and Glass TUI -> GC: same logs/IDs, origin, history and appended replies; /tmp/glass-shared-chat-tui.ansi')
    finally:
        server.shutdown()
        server.server_close()
