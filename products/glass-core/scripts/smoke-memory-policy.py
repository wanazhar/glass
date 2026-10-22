#!/usr/bin/env python3
"""Real provider-wire checks for bounded native recall and zero extra inference."""
import http.server
import json
import os
from pathlib import Path
import statistics
import subprocess
import tempfile
import threading
import time
from core_binary import core_binary_path

BIN = Path(os.environ.get('GLASS_CORE_TEST_BINARY', str(core_binary_path('debug')))).resolve()

class Endpoint(http.server.BaseHTTPRequestHandler):
    def log_message(self, *_):
        pass

    def do_POST(self):
        self.server.requests.append(json.loads(self.rfile.read(int(self.headers['Content-Length']))))
        self.send_response(200)
        self.send_header('Content-Type', 'text/event-stream')
        self.end_headers()
        self.wfile.write(b'data: {"choices":[{"delta":{"content":"DONE"}}]}\n\ndata: [DONE]\n\n')

with tempfile.TemporaryDirectory(prefix='gc-memory-policy-wire-') as tmp:
    root = Path(tmp)
    env = dict(os.environ, XDG_CONFIG_HOME=str(root/'config'), XDG_DATA_HOME=str(root/'data'))
    cfg = root/'config/glass-core'
    cfg.mkdir(parents=True)
    server = http.server.ThreadingHTTPServer(('127.0.0.1', 0), Endpoint)
    server.requests = []
    threading.Thread(target=server.serve_forever, daemon=True).start()
    (cfg/'config.toml').write_text(f'provider="openai"\nmodel="mock"\napi_key_env=""\nbase_url="http://127.0.0.1:{server.server_port}/v1"\ntrust="always"\n')

    def command(*args):
        result = subprocess.run([str(BIN), *args], cwd=root, env=env, capture_output=True, text=True, timeout=30)
        assert result.returncode == 0, (result.stdout, result.stderr)
        return result

    def turn(prompt):
        before = len(server.requests)
        command('run', '--json', prompt)
        assert len(server.requests) == before + 1, 'memory added an inference request'
        return '\n'.join(m.get('content', '') for m in server.requests[-1]['messages'] if m['role'] == 'system')

    try:
        assert 'cargo test' in turn('remember: Run cargo test for terminal changes')
        assert 'cargo test' in turn('Fix terminal layout')  # fresh session, no tool call needed
        store_path = next((root/'data/glass-core/neuralnets').glob('*.json'))
        store = json.loads(store_path.read_text())
        for i in range(511):
            store['entries'].append(dict(id=f'noise-{i}', kind='lesson', text=f'Database migration note {i}', source='fixture', status='active', saved_unix=i))
        store_path.write_text(json.dumps(store))
        before = store_path.read_bytes()
        timings = []
        for _ in range(5):
            start = time.perf_counter()
            context = turn('Fix terminal layout')
            timings.append((time.perf_counter() - start) * 1000)
            assert 'cargo test' in context and 'Database migration' not in context
        assert store_path.read_bytes() == before, 'recall mutated memory'
        turn('remember: Full store must preserve existing knowledge')
        assert store_path.read_bytes() == before, 'full-store capture changed existing knowledge'
        command('neuralnets', 'mode', 'recall', '--json')
        assert 'cargo test' in turn('terminal')
        turn('remember: This must not be saved')
        assert len(json.loads(store_path.read_text())['entries']) == 512
        command('neuralnets', 'mode', 'off', '--json')
        context = turn('terminal')
        assert 'cargo test' not in context and 'Memory is off' in context
        print(json.dumps({'entries': 512, 'full_process_mock_turn_median_ms': round(statistics.median(timings), 2), 'extra_inference_requests': 0}))
    finally:
        server.shutdown()
        server.server_close()
print('PASS: automatic direct capture, fresh-session relevance, bounded full store, recall/off and zero extra inference')
