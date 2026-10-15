#!/usr/bin/env python3
"""Compare recall-journal write traffic in an offline Linux PTY fixture.

GLASS_CORE_TEST_BINARY selects the executable; --bounded checks the new limits.
Process wchar includes terminal, network, and diagnostic writes, not just history.
All provider traffic and retained data belong to disposable local fixtures.
"""
import argparse
import http.server
import json
import os
from pathlib import Path
import subprocess
import tempfile
import threading
from pty_vt import Terminal

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('--bounded', action='store_true')
options = parser.parse_args()
binary = Path(os.environ.get('GLASS_CORE_TEST_BINARY', 'target/release/glass-core')).resolve()
os.environ['GLASS_CORE_TEST_BINARY'] = str(binary)


class Endpoint(http.server.BaseHTTPRequestHandler):
    def log_message(self, *_):
        pass

    def do_POST(self):
        body = json.loads(self.rfile.read(int(self.headers['Content-Length'])))
        prompt = next(message['content'] for message in reversed(body['messages'])
                      if message['role'] == 'user')
        self.send_response(200)
        self.send_header('Content-Type', 'text/event-stream')
        self.end_headers()
        event = {'choices': [{'delta': {'content': f'HISTORY_DONE_{prompt}'}}]}
        self.wfile.write(('data: ' + json.dumps(event) + '\n\ndata: [DONE]\n\n').encode())
        self.wfile.flush()


def wchar(pid):
    file = Path(f'/proc/{pid}/io')
    if not file.exists():
        return None
    return int(next(line.split(':')[1] for line in file.read_text().splitlines()
                    if line.startswith('wchar:')))


with tempfile.TemporaryDirectory(prefix='gc-history-footprint-') as tmp:
    root = Path(tmp)
    subprocess.run(['git', 'init', '-q', str(root)], check=True)
    data = root / 'data/glass-core'
    config = root / 'config/glass-core'
    data.mkdir(parents=True)
    config.mkdir(parents=True)
    history = data / 'history.jsonl'
    # A full journal makes the old per-submission rewrite visible. These are
    # synthetic recall entries, never user conversation records.
    history.write_text(''.join(json.dumps(f'old-{index}:' + 'x' * 8192) + '\n'
                               for index in range(200)))
    server = http.server.ThreadingHTTPServer(('127.0.0.1', 0), Endpoint)
    thread = threading.Thread(target=server.serve_forever, daemon=True)
    thread.start()
    (config / 'config.toml').write_text(
        'provider="openai"\nmodel="fixture"\napi_key_env=""\n'
        f'base_url="http://127.0.0.1:{server.server_port}/v1"\nadaptive_pacing=false\n'
    )
    env = dict(os.environ, XDG_CONFIG_HOME=str(root / 'config'), XDG_DATA_HOME=str(root / 'data'))
    terminal = Terminal(root, env, (40, 24))
    try:
        terminal.idle()
        before = wchar(terminal.p.pid)
        for index in range(40):
            terminal.send(f'{index}\r')
            terminal.wait(f'HISTORY_DONE_{index}')
        terminal.collect(.15)
        after = wchar(terminal.p.pid)
        report = {'turns': 40, 'history_bytes': history.stat().st_size,
                  'history_entries_on_disk': len(history.read_text().splitlines()),
                  'process_written_chars': None if before is None else after - before}
        assert json.loads(history.read_text().splitlines()[-1]) == '39', report
        if options.bounded:
            assert report['history_bytes'] <= 1024 * 1024, report
            assert report['history_entries_on_disk'] <= 232, report
            if report['process_written_chars'] is not None:
                assert report['process_written_chars'] < 8 * 1024 * 1024, report
        print(json.dumps(report, indent=2))
    finally:
        terminal.close()
        server.shutdown()
        server.server_close()
        thread.join(timeout=5)
