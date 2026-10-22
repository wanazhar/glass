#!/usr/bin/env python3
"""Actual-release PTY recovery, compaction, scrolling and long-turn checks."""
import http.server
import json
import os
from pathlib import Path
import tempfile
import threading
from pty_vt import Terminal

BIN = Path(os.environ.get('GLASS_CORE_TEST_BINARY', Path(__file__).resolve().parents[1] / 'target/release/glass-core')).resolve()
os.environ['GLASS_CORE_TEST_BINARY'] = str(BIN)

class Provider(http.server.BaseHTTPRequestHandler):
    def log_message(self, *_): pass
    def do_POST(self):
        body = json.loads(self.rfile.read(int(self.headers['Content-Length'])))
        tools = body.get('tools', [])
        messages = body['messages']
        if not tools:
            delta = {'reasoning_content': 'Summary unavailable'}
        else:
            system = messages[0]['content']
            if self.server.prefix is None: self.server.prefix = system
            assert system == self.server.prefix, 'Recovery changed the cached system prefix'
            latest = [m['content'] for m in messages if m['role'] == 'user'][-1]
            if latest == 'followup': delta = {'content': 'FOLLOWUPDONE'}
            else:
                results = [m for m in messages if m['role'] == 'tool']
                n = len(results)
                if n == 2 and not self.server.failed:
                    self.server.failed = True
                    delta = {'reasoning_content': 'Need to finish the task'}
                elif n < 28:
                    name = 'bash' if n in (0,27) else 'state'
                    args = {'command': "printf 'readable tool output\\n'"} if n in (0,27) else {}
                    delta = {'tool_calls': [{'index': 0, 'id': 'recovery-' + str(n), 'function': {'name': name, 'arguments': json.dumps(args)}}]}
                else: delta = {'content': 'RECOVERYDONE\n\n| Result | Value |\n| --- | --- |\n| status | complete |'}
        payload = ('data: ' + json.dumps({'model':'mock','choices':[{'delta':delta, 'finish_reason':'stop'}]}) + '\n\ndata: [DONE]\n\n').encode()
        self.send_response(200)
        self.send_header('Content-Type','text/event-stream')
        self.end_headers()
        self.wfile.write(payload)

for size in [(80,30), (40,24), (20,8)]:
    with tempfile.TemporaryDirectory(prefix='gc-recovery-pty-') as tmp:
        root = Path(tmp)
        config = root / 'config/glass-core'
        config.mkdir(parents=True)
        server = http.server.ThreadingHTTPServer(('127.0.0.1',0), Provider)
        server.prefix = None
        server.failed = False
        threading.Thread(target=server.serve_forever, daemon=True).start()
        (config / 'config.toml').write_text(f'provider="openai"\nmodel="mock"\napi_key_env=""\nbase_url="http://127.0.0.1:{server.server_port}/v1"\n')
        env = dict(os.environ, XDG_CONFIG_HOME=str(root/'config'), XDG_DATA_HOME=str(root/'data'))
        term = Terminal(root, env, size)
        try:
            term.idle()
            term.send('complete task\r')
            term.wait('RECOVERYDONE', 45)
            assert server.failed
            assert 'reached the tool-call budget' not in term.seen
            term.send('\x1b[5~')
            term.collect(.5)
            term.send('\x1b[F')
            term.collect(.3)
            if size[0] >= 40:
                assert '✓ bash' in term.vt.text(), term.vt.text()
                assert 'Tool call_' not in term.vt.text(), term.vt.text()
            term.send('\x1b')
            term.collect(.3)
            term.send('/compaction\r')
            term.wait('compacted', 15)
            term.collect(.3)
            assert 'compact failed' not in term.seen
            records = [json.loads(line) for p in (root/'data/glass-core').glob('*.jsonl') if p.stem != 'history' for line in p.read_text().splitlines()]
            assert any(r.get('k') == 'compacted' and 'Extractive checkpoint' in r.get('sum','') for r in records), records[-5:]
            term.send('followup\r')
            term.wait('FOLLOWUPDONE', 15)
        finally:
            term.close()
            server.shutdown()
            server.server_close()
    print('PASS release recovery, 28 tool rounds, stable prefix, compaction fallback, scroll and follow-up', size)
