#!/usr/bin/env python3
"""Measure cold request bytes and automatic storage with an offline provider.

GLASS_CORE_TEST_BINARY selects the executable. --bounded checks the current
capture limits. Byte counts are reproducible; they are not tokenizer estimates.
All configuration, logs, and provider traffic stay in disposable local fixtures.
"""
import argparse
import http.server
import json
import os
from pathlib import Path
import subprocess
import tempfile
import threading

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('--bounded', action='store_true')
options = parser.parse_args()
binary = Path(os.environ.get('GLASS_CORE_TEST_BINARY', 'target/release/glass-core')).resolve()
report = {'binary_bytes': binary.stat().st_size}

for scenario in ('hello', 'verbose', 'native_partial'):
    requests = []

    class Endpoint(http.server.BaseHTTPRequestHandler):
        def log_message(self, *_):
            pass

        def do_POST(self):
            raw = self.rfile.read(int(self.headers['Content-Length']))
            body = json.loads(raw)
            requests.append((body, len(raw)))
            self.send_response(200)
            self.send_header('Content-Type', 'text/event-stream')
            self.end_headers()
            if scenario == 'verbose' and len(requests) == 1:
                delta = {'tool_calls': [{'index': 0, 'id': 'capture', 'function': {
                    'name': 'bash', 'arguments': json.dumps({
                        'command': "head -c 6000000 /dev/zero | tr '\\000' x; printf GC_FINAL_TAIL"
                    })}}]}
            elif scenario == 'native_partial' and len(requests) == 1:
                delta = {'tool_calls': [{'index': 0, 'id': 'read', 'function': {
                    'name': 'read', 'arguments': json.dumps({'path': 'sample.txt', 'limit': 1})}}]}
            else:
                delta = {'content': 'FOOTPRINT_DONE'}
            event = {'choices': [{'delta': delta}]}
            self.wfile.write(('data: ' + json.dumps(event) + '\n\ndata: [DONE]\n\n').encode())
            self.wfile.flush()

    server = http.server.ThreadingHTTPServer(('127.0.0.1', 0), Endpoint)
    worker = threading.Thread(target=server.serve_forever, daemon=True)
    worker.start()
    try:
        with tempfile.TemporaryDirectory(prefix='gc-footprint-') as tmp:
            root = Path(tmp)
            if scenario == 'native_partial':
                (root / 'sample.txt').write_text('first\nsecond\nthird\n')
            config = root / 'config/glass-core'
            config.mkdir(parents=True)
            (config / 'config.toml').write_text(
                'provider="openai"\nmodel="fixture"\napi_key_env=""\n'
                f'base_url="http://127.0.0.1:{server.server_port}/v1"\n'
                'adaptive_pacing=false\n'
            )
            env = dict(os.environ, XDG_CONFIG_HOME=str(root / 'config'),
                       XDG_DATA_HOME=str(root / 'data'))
            result = subprocess.run([str(binary), 'run', '--json', scenario],
                                    cwd=root, env=env, capture_output=True, text=True, timeout=30)
            assert result.returncode == 0, result.stderr
            assert 'FOOTPRINT_DONE' in result.stdout, result.stdout
            assert len(requests) == (1 if scenario == 'hello' else 2)
            first = requests[0][0]
            system_bytes = sum(len(message.get('content', '').encode())
                               for message in first['messages'] if message['role'] == 'system')
            schemas = json.dumps(first.get('tools', []), separators=(',', ':')).encode()
            results = [message['content'] for message in requests[-1][0]['messages']
                       if message['role'] == 'tool']
            files = {str(path.relative_to(root / 'data')): path.stat().st_size
                     for path in (root / 'data').rglob('*') if path.is_file()}
            item = {'system_bytes': system_bytes, 'tool_schema_bytes': len(schemas),
                    'request_bytes': [size for _, size in requests],
                    'tool_result_bytes': sum(len(text.encode()) for text in results),
                    'automatic_storage_bytes': sum(files.values()),
                    'largest_files': sorted(files.items(), key=lambda pair: pair[1], reverse=True)[:5]}
            report[scenario] = item
            if options.bounded:
                assert system_bytes < 2100, item
                if scenario == 'hello':
                    assert item['automatic_storage_bytes'] < 100_000, item
                elif scenario == 'verbose':
                    assert item['tool_result_bytes'] < 20_000, item
                    assert 'GC_FINAL_TAIL' in results[0], results[0][-300:]
                    assert 'bytes discarded' in results[0], results[0][-300:]
                    assert max(files.values()) <= 4 * 1024 * 1024, item
                    assert item['automatic_storage_bytes'] < 5 * 1024 * 1024, item
                    assert not any(name.startswith('glass-core/output/') for name in files), item
                else:
                    assert results[0].count('next_read:') == 1, results
                    assert 'continue with file read' not in results[0], results
                    assert item['automatic_storage_bytes'] < 100_000, item
    finally:
        server.shutdown()
        server.server_close()
        worker.join(timeout=5)

print(json.dumps(report, indent=2))
