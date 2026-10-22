#!/usr/bin/env python3
"""Release-binary workflow regressions; local deterministic provider, isolated XDG.
Reports actual wall latency, provider usage, edit correctness and PTY byte volume.
No paid providers or dependencies. GLASS_CORE_TEST_BINARY overrides release path.
"""
import hashlib
import http.server
import importlib.util
import json
import os
from pathlib import Path
from core_binary import core_binary_path
import shlex
import subprocess
import tempfile
import threading
import time

ROOT = Path(__file__).resolve().parents[1]
BIN = Path(os.environ.get('GLASS_CORE_TEST_BINARY', str(core_binary_path('release')))).resolve()
spec = importlib.util.spec_from_file_location('features', ROOT / 'scripts/smoke-features.py')
f = importlib.util.module_from_spec(spec)
spec.loader.exec_module(f)
f.BINARY = BIN


class Provider(http.server.BaseHTTPRequestHandler):
    requests = []
    revision = ''
    def log_message(self, *_): pass
    def do_POST(self):
        body = json.loads(self.rfile.read(int(self.headers['Content-Length'])))
        type(self).requests.append(body)
        count = sum(m['role'] == 'tool' for m in body['messages'])
        commands = [
            '"$GC_BIN" file --json read sample.py',
            'printf %s ' + shlex.quote(json.dumps({'path': 'sample.py', 'expected': self.revision, 'edits': [{'old': 'return a - b', 'new': 'return a + b'}]})) + ' | "$GC_BIN" file --json edit',
            'python3 -m unittest test_sample.py',
        ]
        readonly=any(t['function']['name']=='inspect' for t in body.get('tools',[]))
        if readonly:
            delta={'content':'Review complete. No additional findings.'}
        elif count < len(commands):
            delta = {'tool_calls': [{'index': 0, 'id': 'call-' + str(count), 'function': {'name': 'bash', 'arguments': json.dumps({'command': commands[count]})}}]}
        else:
            delta = {'content': '# Verified\n\n| Check | Result |\n| --- | --- |\n| Addition | Passed |\n\nChanged `sample.py` using its current revision.\n'}
        frames = [{'choices': [{'delta': delta}]}, {'choices': [], 'usage': {'prompt_tokens': 500 + count * 50, 'completion_tokens': 30, 'prompt_tokens_details': {'cached_tokens': 100}}}]
        payload = (''.join('data: ' + json.dumps(v) + '\n\n' for v in frames) + 'data: [DONE]\n\n').encode()
        self.send_response(200)
        self.send_header('Content-Type', 'text/event-stream')
        self.send_header('Content-Length', str(len(payload)))
        self.end_headers()
        self.wfile.write(payload)


def main():
    with tempfile.TemporaryDirectory(prefix='gc-workflow-next-') as temp:
        root = Path(temp)
        ws = root / 'workspace with spaces'
        ws.mkdir()
        cfg = root / 'config/glass-core'
        cfg.mkdir(parents=True)
        env = dict(os.environ, XDG_CONFIG_HOME=str(root / 'config'), XDG_DATA_HOME=str(root / 'data'), GC_API_KEY='fixture-not-a-secret')
        server = http.server.ThreadingHTTPServer(('127.0.0.1', 0), Provider)
        threading.Thread(target=server.serve_forever, daemon=True).start()
        (cfg / 'config.toml').write_text(f'provider="openai"\nmodel="mock"\napi_key_env="GC_API_KEY"\nbase_url="http://127.0.0.1:{server.server_port}/v1"\n')
        cases = []
        def cli(*args, ok=True, data=None):
            result = subprocess.run([str(BIN), *args], cwd=ws, env=env, input=data, text=True, capture_output=True, timeout=25)
            assert (result.returncode == 0) == ok, (args, result.returncode, result.stdout, result.stderr)
            return result.stdout
        def record(name): cases.append(name)
        try:
            assert 'Tasks' in cli('help','commands'); record('help discovers workflows')
            assert json.loads(cli('policy', '--json'))['deny_shell'] == []; record('policy JSON outside Git')
            cli('workspace','task','add','bad','--accept','checks','--paths','src/../sample.py',ok=False)
            (ws/'sample.py').write_text('placeholder\n')
            (ws/'alias.py').symlink_to(ws/'sample.py')
            task = json.loads(cli('workspace', 'task', 'add', 'parser', '--accept', 'unit tests', '--paths', 'sample.py', '--json'))['task']; record('normalize ownership paths')
            dep = json.loads(cli('workspace', 'task', 'add', 'integration', '--accept', 'checks', '--depends', str(task), '--json'))['task']
            cli('workspace', 'task', 'claim', str(dep), 'reviewer', ok=False); record('blocked dependency refused')
            cli('workspace', 'task', 'claim', str(task), 'main')
            cli('workspace', 'task', 'claim', str(task), 'other', ok=False); record('duplicate claim refused')
            overlap = json.loads(cli('workspace', 'task', 'add', 'overlap', '--accept', 'checks', '--paths', 'alias.py', '--json'))['task']
            cli('workspace', 'task', 'claim', str(overlap), 'other', ok=False); record('aliased ownership conflict refused')
            cli('workspace', 'task', 'finish', str(task), 'other', 'pass', ok=False); record('wrong owner finish refused')
            cli('workspace', 'task', 'finish', str(task), 'main', 'fixture check')
            cli('workspace', 'task', 'claim', str(dep), 'reviewer'); record('completed dependency unblocks')
            hypothesis = json.loads(cli('workspace', 'debug', 'add', 'addition uses subtraction', '--json'))['hypothesis']
            cli('workspace', 'debug', 'resolve', str(hypothesis), ok=False)
            cli('workspace', 'debug', 'evidence', str(hypothesis), 'failing addition case')
            cli('workspace', 'debug', 'resolve', str(hypothesis)); record('debug resolution requires evidence')
            cli('workspace', 'role', 'set', 'reviewer', 'mock', '1', ok=False)
            cli('workspace', 'role', 'set', 'reviewer', 'mock', '2048'); record('role budget validation')
            (ws / 'report.json').write_text('{"checks":[{"name":"unit","passed":true}]}')
            aid = json.loads(cli('workspace', 'artifact', 'add', 'checks', 'report.json', '--json'))['artifact']
            assert 'checks' in cli('workspace', 'artifact', 'preview', str(aid)); record('artifact register and preview')
            (ws / 'report.json').write_text('{}')
            cli('workspace', 'artifact', 'preview', str(aid), ok=False); record('stale artifact refused')
            (ws / 'handoff.json').write_text(json.dumps({'agent':'main','task':task,'changed_files':['sample.py'],'checks':['fixture'],'findings':[],'questions':[],'artifacts':[]}))
            cli('workspace', 'handoff', 'handoff.json')
            (ws / 'handoff.json').write_text('{"task":999,"unknown":true}')
            cli('workspace', 'handoff', 'handoff.json', ok=False); record('strict handoff schema')
            for shell in ('bash','zsh','fish'):
                script = cli('completions', shell)
                assert '\x1b' not in script and 'gc' in script
            subprocess.run(['bash','-n'], input=cli('completions','bash'), text=True, check=True); record('plain valid completions')
            cli('plugin','scaffold','example','plugin')
            cli('plugin','validate','plugin'); record('plugin scaffold validates without execution')
            (ws/'report.json').write_text('{"checks":[]}')
            assert 'checks' in cli('inspect','report.json'); record('native structured CLI')
            (cfg / 'policy.json').write_text('{"deny_shell":["*forbidden-marker*"],"deny_providers":["openai"]}')
            assert json.loads(cli('policy','shell','touch forbidden-marker','--json'))['decision'] == 'deny'
            cli('run','hello',ok=False)
            (cfg / 'policy.json').unlink(); record('global provider and shell denials')
            source = 'def add(a, b):\n    return a - b\n'
            (ws / 'sample.py').write_text(source)
            (ws / 'test_sample.py').write_text('import unittest\nfrom sample import add\nclass Addition(unittest.TestCase):\n def test_add(self): self.assertEqual(add(2, 3), 5)\n')
            Provider.revision = hashlib.sha256(source.encode()).hexdigest()
            started = time.monotonic()
            output = cli('run','Fix addition in sample.py and verify it with its tests.')
            elapsed = time.monotonic() - started
            assert elapsed*1000<float(env.get('GC_WORKFLOW_BUDGET_MS','10000')),elapsed
            assert 'Verified' in output, output
            assert 'return a + b' in (ws / 'sample.py').read_text()
            logs = list((root / 'data/glass-core').glob('*.requests.jsonl'))
            assert len(logs) == 1, logs
            reports = [json.loads(line) for line in logs[0].read_text().splitlines()]
            assert len(reports) == 4 and all(r['outcome'] == 'completed' for r in reports), reports
            assert reports[1]['context_delta']['unchanged_message_prefix'] > 0
            assert not any('fixture-not-a-secret' in json.dumps(r) for r in reports)
            record('real edit task and request metadata')
            sid = logs[0].name.removesuffix('.requests.jsonl')
            assert len(json.loads(cli('requests',sid,'--json'))['requests']) == 4
            draft = cli('skill','capture',sid,'addition-check','1','1')
            (ws / 'draft.md').write_text(draft)
            cli('skill','save','addition-check','draft.md')
            cli('skill','save','addition-check','draft.md',ok=False); record('capture reviewed workflow without overwriting')
            subprocess.run(['git','init','-q'],cwd=ws,check=True)
            subprocess.run(['git','add','sample.py'],cwd=ws,check=True)
            subprocess.run(['git','-c','user.name=Fixture','-c','user.email=fixture@example.invalid','commit','-qm','baseline'],cwd=ws,check=True)
            with open(ws/'sample.py','a') as changed: changed.write('# checkpoint fixture\n')
            total_bytes = 0
            for size in ((20,8),(40,24),(80,30)):
                before=set((root/'data/glass-core').glob('*.jsonl'))
                tty = f.Terminal(ws,env,size=size)
                try:
                    tty.visible('❯',timeout=8)
                    for command,title in (('/workspace','Workspace'),('/requests','Requests'),('/outputs','Outputs'),('/help','Help')):
                        tty.send(command)
                        tty.visible(title,timeout=5)
                        total_bytes += len(tty.received.encode())
                        tty.received=''
                        os.write(tty.master,b'\x1b')
                        tty.wait('\x1b[?25h')
                        tty.idle()
                    # Palette open/close leaves composer usable.
                    os.write(tty.master,b'/')
                    tty.wait('Commands',timeout=5)
                    tty.received=''
                    os.write(tty.master,b'\x1b')
                    tty.wait('\x1b[?25h')
                    os.write(tty.master,b'\x15')  # Esc preserves the draft slash.
                    tty.idle()
                    assert not tty.screen.invalid_positions, tty.screen.invalid_positions
                    if size==(80,30):
                        active=[p for p in set((root/'data/glass-core').glob('*.jsonl'))-before if p.name.count('.')==1]
                        assert len(active)==1,active
                        owner=active[0].stem
                        cli('agent','--session',owner,'create','overbudget','Review parser','--role','reviewer','--tokens','9999',ok=False)
                        checkpoint=json.loads(cli('agent','--session',owner,'checkpoint','changes'))
                        assert checkpoint['read_only'] and checkpoint['token_limit']==2048,checkpoint
                        cli('agent','--session',owner,'wait',checkpoint['id'],'10')
                        repeat=json.loads(cli('agent','--session',owner,'checkpoint','changes'))
                        assert 'already requested' in repeat['checkpoint'],repeat
                        (ws/'handoff.json').write_text(json.dumps({'agent':'main','task':task,'changed_files':['sample.py'],'checks':['fixture'],'findings':[],'questions':[],'artifacts':[]}))
                        report=json.loads(cli('agent','--session',owner,'handoff','handoff.json'))
                        assert report['handoff']=='saved'
                        record('bounded role checkpoint, duplicate suppression, parent handoff notice')
                finally:
                    tty.close()
            record('15 release PTY overlay cases at three sizes')
            source_bytes=(root/'data/glass-core'/f'{sid}.jsonl').read_bytes()
            before=set((root/'data/glass-core').glob('*.jsonl'))
            other=root/'other project';other.mkdir()
            picker=f.Terminal(other,env,args=('resume','--all'),size=(80,30))
            try:
                picker.visible('Resume conversation',timeout=8)
                os.write(picker.master,sid.encode())
                picker.visible(sid[:12])
                os.write(picker.master,b'\r')
                picker.visible('Preview conversation')
                os.write(picker.master,b'\x1b[B\r')
                picker.visible('Conversation preview')
                picker.visible('Verified')
                os.write(picker.master,b'\x1b')
                picker.visible('Preview conversation')
                os.write(picker.master,b'\x1b[B\x1b[B\r')
                picker.visible('Fork conversation')
                os.write(picker.master,b'\r')
                picker.visible('❯',timeout=8)
                assert (root/'data/glass-core'/f'{sid}.jsonl').read_bytes()==source_bytes
                deadline=time.monotonic()+5
                while True:
                    created=[p for p in set((root/'data/glass-core').glob('*.jsonl'))-before if p.name.count('.')==1]
                    if created: break
                    assert time.monotonic()<deadline,'fork did not create a new session'
                    time.sleep(.05)
                assert len(created)==1,created
                metadata=created[0].with_suffix('.meta.json')
                while not metadata.exists():
                    assert time.monotonic()<deadline,'fork metadata was not saved'
                    time.sleep(.05)
                meta=json.loads(metadata.read_text())
                assert meta['workspace']==str(ws.resolve()),meta
            finally: picker.close()
            record('cross-project picker preview and fork preserves source and workspace')
            assert len(Provider.requests)==5,'views, duplicate checkpoint or picker made an unsolicited request'
            assert 0<Provider.requests[-1]['max_tokens']<=2048
            assert total_bytes<1024*1024,'excessive idle modal repaint volume'
            print(json.dumps({'cases':len(cases),'passed':cases,'real_task_ms':round(elapsed*1000),'requests':len(Provider.requests),'uncached_input_tokens':sum(r['usage']['input'] for r in reports),'cache_read_tokens':sum(r['usage']['cache_read'] for r in reports),'input_tokens':sum(r['usage']['input']+r['usage']['cache_read']+r['usage']['cache_write'] for r in reports),'output_tokens':sum(r['usage']['output'] for r in reports),'pty_bytes':total_bytes,'binary':str(BIN)},indent=2))
        finally:
            server.shutdown()


if __name__ == '__main__': main()
