#!/usr/bin/env python3
"""Human workflow controls recover delegation without changing token allowances."""
import importlib.util
import tempfile
import json
from pathlib import Path
spec = importlib.util.spec_from_file_location('agent_smoke', Path(__file__).with_name('smoke-agents.py'))
fixture = importlib.util.module_from_spec(spec)
spec.loader.exec_module(fixture)
for size in [(80, 24), (40, 20)]:
    with tempfile.TemporaryDirectory(prefix='gc-workflow-ui-') as tmp:
        root = Path(tmp)
        server, env = fixture.setup(root)
        terminal = fixture.Terminal(root, env, size)
        try:
            terminal.idle()
            owner = fixture.owner_id(root)
            def state(): return fixture.ctl(root, env, owner, 'workflow', 'inspect')
            allowance = state()['tokenLimit']
            def command(text, expected):
                terminal.bytes = ''
                terminal.send(text+'\r')
                terminal.wait(expected, 10)
                terminal.collect(.15)
            command('/agents stop', 'Teammate workflow')
            assert state()['stopped']
            assert 'stopped' in fixture.ctl(root, env, owner, 'create', 'blocked', 'inspect', ok=False)
            command('/agents resume', 'Delegation is enabled')
            assert not state()['stopped'] and state()['tokenLimit'] == allowance
            records = [json.loads(line) for path in (root/'d/glass-core').rglob('*.jsonl') for line in path.read_text().splitlines() if line.strip()]
            assert any(row.get('k') == 'system' and 'Delegation is enabled' in row.get('t','') for row in records), 'main agent needs a durable human-resume receipt'
            fixture.ctl(root, env, owner, 'create', 'review', 'CHECK_READ')
            worker = fixture.ctl(root, env, owner, 'wait', 'review', '10')['agent']
            assert worker['result'] == 'REVIEW_COMPLETE', worker
            command('/agents stop', 'Teammate workflow')
            terminal.send('\x1ba')
            terminal.collect(.25)
            assert 'r resume' in terminal.vt.text(), terminal.vt.text()
            count = len(server.requests)
            terminal.send('r')
            terminal.collect(.25)
            assert not state()['stopped'] and state()['tokenLimit'] == allowance
            assert len(server.requests) == count, 'resume must not invoke parked agents'
            terminal.send('\x1b')
            terminal.collect(.15)
            command('/agents status', 'Teammate workflow')
            print(f'PASS {size}: stop/resume/status, panel resume, fresh delegation, unchanged allowance, no implicit provider activation')
        finally:
            terminal.close()
            server.shutdown()
            server.server_close()
