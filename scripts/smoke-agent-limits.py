#!/usr/bin/env python3
"""Actual release PTYs: model-managed teammate limits and bounded recovery."""
import importlib.util
import json
import os
from pathlib import Path
import tempfile

ROOT = Path(__file__).resolve().parents[1]
os.environ.setdefault('GLASS_CORE_TEST_BINARY', str(ROOT / 'target/release/glass-core'))
spec = importlib.util.spec_from_file_location('agents', ROOT / 'scripts/smoke-agents.py')
a = importlib.util.module_from_spec(spec)
spec.loader.exec_module(a)


class Endpoint(a.Endpoint):
    def do_POST(self):
        body = json.loads(self.rfile.read(int(self.headers['Content-Length'])))
        self.server.requests.append(body)
        history = json.dumps(body.get('messages', []))
        tools = [m for m in body.get('messages', []) if m['role'] == 'tool']
        usage = {'prompt_tokens': 100, 'completion_tokens': 20}
        if 'OUTPUT_REPAIR' in history and body.get('max_tokens', 0) < 24000:
            delta = {'reasoning_content': 'provider-exposed reasoning summary'}
            finish = 'length'
            usage['completion_tokens'] = 4096
        elif 'BUDGET_HOG' in history and not tools:
            delta = {'tool_calls': [{'index': 0, 'id': 'read-evidence', 'function': {
                'name': 'inspect', 'arguments': json.dumps({'action': 'read', 'path': 'large.txt'})}}]}
            finish = 'tool_calls'
            usage['prompt_tokens'] = 97000
        else:
            delta = {'content': 'WORKER_DONE'}
            finish = 'stop'
        payload = ''.join('data: ' + json.dumps(v) + '\n\n' for v in [
            {'choices': [{'delta': delta, 'finish_reason': finish}]},
            {'choices': [], 'usage': usage},
        ]) + 'data: [DONE]\n\n'
        self.send_response(200)
        self.send_header('Content-Type', 'text/event-stream')
        self.end_headers()
        self.wfile.write(payload.encode())


def run(size):
    with tempfile.TemporaryDirectory(prefix='gc-agent-limits-') as temp:
        root = Path(temp)
        server, env = a.setup(root)
        server.RequestHandlerClass = Endpoint
        (root / 'large.txt').write_text(('x' * 600 + '\n') * 100)
        t = a.Terminal(root, env, size)
        try:
            t.idle()
            owner = a.owner_id(root)
            ctl = lambda *args, **kw: a.ctl(root, env, owner, *args, **kw)
            sessions = root / 'd/glass-core/agents' / owner / 'sessions'
            before = list(sessions.glob('*.jsonl'))
            ctl('create', 'bad', 'invalid', '--output', '128', ok=False)
            assert list(sessions.glob('*.jsonl')) == before
            ctl('create', 'reasoner', 'OUTPUT_REPAIR', '--output', '4096')
            blocked = ctl('wait', 'reasoner', '10')['agent']
            assert blocked['failure_kind'] == 'output_exhausted', blocked
            assert blocked['output_limit'] == 4096 and blocked['remaining_tokens'] == 95804, blocked
            assert 'limits' in blocked['recovery'] and blocked['last_request_input_estimate'] > 0
            count = len(server.requests)
            ctl('limits', 'reasoner', '--output', '24000', '--reasoning', '1024', '--rounds', '30')
            assert len(server.requests) == count, 'limits must not trigger a provider call'
            ctl('limits', 'reasoner', '--output', '512', ok=False)
            assert ctl('inspect', 'reasoner')['agent']['output_limit'] == 24000
            ctl('resume', 'reasoner', 'Continue from existing work; provide the report.')
            done = ctl('wait', 'reasoner', '10')['agent']
            assert done['status'] == 'idle' and done['failure_kind'] is None, done
            assert server.requests[-1]['max_tokens'] == 24000
            assert 'OUTPUT_REPAIR' in json.dumps(server.requests[-1])
            ctl('create', 'focused', 'BUDGET_HOG')
            recovered = ctl('wait', 'focused', '10')['agent']
            assert recovered['status'] == 'idle', recovered
            worker_requests = [b for b in server.requests if 'BUDGET_HOG' in json.dumps(b)]
            assert len(worker_requests) == 2, len(worker_requests)
            assert worker_requests[-1]['max_tokens'] < 3000, worker_requests[-1]['max_tokens']
            assert 'dropped to recover' in json.dumps(worker_requests[-1]), worker_requests[-1]
            assert recovered['input'] == 97100 and recovered['budget_warning_sent'], recovered
            assert recovered['last_request_input_estimate'] < recovered['remaining_tokens']
            listed = ctl('list')
            assert all(isinstance(i['pending_messages'], int) for i in listed)
            t.send('/agents\r'); t.collect(.3)
            assert 'Agents' in t.vt.text(), t.vt.text()
            t.send('\t\t'); t.collect(.2)
            t.send('\x1b[6~'); t.collect(.1)
            t.send('\x1b'); t.collect(.2)
            t.send('hello\r'); t.wait('WORKER_DONE', 10)
            assert t.p.poll() is None
            print(f'PASS {size}: output failure → explicit limits → persistent resume; budget compaction; bounded receipts; palette/composer')
        finally:
            t.close(); server.shutdown(); server.server_close()


if __name__ == '__main__':
    for size in [(80, 24), (40, 12), (20, 8)]:
        run(size)
