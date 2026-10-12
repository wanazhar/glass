#!/usr/bin/env python3
"""Release PTY: cached recap, modal dismissal, Unicode and no model requests."""
import http.server
import json
import os
import tempfile
import threading
from pathlib import Path
from pty_vt import Terminal

class Provider(http.server.BaseHTTPRequestHandler):
    requests = 0
    def log_message(self, *_): pass
    def do_POST(self):
        type(self).requests += 1
        self.send_error(500, 'Recap must not call a provider')

for size in [(120, 36), (80, 30), (40, 24), (20, 8)]:
    with tempfile.TemporaryDirectory(prefix='gc-recap-pty-') as tmp:
        root = Path(tmp)
        config = root / 'config/glass-core'
        config.mkdir(parents=True)
        data = root / 'data/glass-core'
        data.mkdir(parents=True)
        records = [{'k': 'user', 't': 'Continue the Unicode 界面 task'},
                   {'k': 'assistant', 't': 'Saved progress: updated layout. Verification remains pending.'}]
        session = data / 'recap-fixture.jsonl'
        original = ''.join(json.dumps(r) + '\n' for r in records)
        session.write_text(original)
        server = http.server.ThreadingHTTPServer(('127.0.0.1', 0), Provider)
        threading.Thread(target=server.serve_forever, daemon=True).start()
        (config / 'config.toml').write_text(f'provider="openai"\nmodel="fixture"\napi_key_env=""\nbase_url="http://127.0.0.1:{server.server_port}/v1"\n')
        env = dict(os.environ, XDG_CONFIG_HOME=str(root/'config'), XDG_DATA_HOME=str(root/'data'))
        for attempt in range(2):
            terminal = Terminal(root, env, size, args=('resume', 'recap-fixture'))
            try:
                terminal.idle()
                assert 'Recap' in terminal.seen, terminal.vt.text()
                terminal.send('/recap\r')
                terminal.collect(.6)
                assert 'Welcome back' in terminal.vt.text(), terminal.vt.text()
                terminal.send('\x1b')
                terminal.collect(.4)
                terminal.send('draft')
                terminal.collect(.3)
                assert 'draft' in terminal.vt.text(), terminal.vt.text()
            finally:
                terminal.close()
        assert Provider.requests == 0, Provider.requests
        assert session.read_text() == original, 'Recap polluted durable model history'
        assert (data/'recap-fixture.recap.json').exists()
        server.shutdown()
        server.server_close()
        print('PASS recap resume/cache/modal/escape/no inference', size, flush=True)
