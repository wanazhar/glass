#!/usr/bin/env python3
"""Recaps and local command receipts stay readable in normal and reader views."""
import importlib.util
import tempfile
import re
from pathlib import Path

spec = importlib.util.spec_from_file_location('agent_smoke', Path(__file__).with_name('smoke-agents.py'))
fixture = importlib.util.module_from_spec(spec)
spec.loader.exec_module(fixture)
for size in [(80, 30), (40, 20)]:
    with tempfile.TemporaryDirectory(prefix='gc-notices-') as tmp:
        root = Path(tmp)
        server, env = fixture.setup(root)
        terminal = fixture.Terminal(root, env, size)
        try:
            terminal.idle()
            owner = fixture.owner_id(root)
            terminal.send('Please verify the saved conversation.\r')
            terminal.wait('MAIN_DONE', 10)
            terminal.close()
            terminal = fixture.Terminal(root, env, size, args=('resume', owner))
            terminal.idle()
            shown = re.sub(r'\x1b\[[0-?]*[ -/]*[@-~]', '', terminal.seen)
            assert 'Recap' in shown and 'Last request' in shown, terminal.vt.text()
            assert 'MAIN_DONE' in shown, terminal.vt.text()
            terminal.bytes = ''
            terminal.send('/desktop status\r')
            terminal.wait('No desktop is running', 10)
            terminal.collect(.15)
            assert 'Desktop' in terminal.vt.text() and 'Enter' in terminal.vt.text(), terminal.vt.text()
            terminal.send('\x1b')
            terminal.collect(.15)
            terminal.send('\x1b')
            terminal.collect(.15)
            assert 'No desktop is running' in terminal.vt.text(), terminal.vt.text()
            terminal.send('\x07')
            terminal.collect(.2)
            terminal.send('\x1b[F')
            terminal.collect(.2)
            assert 'No desktop is running' in terminal.vt.text(), terminal.vt.text()
            assert not (root/'d/glass-core/connect.json').exists()
            Path(f'/tmp/gc-notices-{size[0]}.ansi').write_text(terminal.seen)
            print(f'PASS {size}: resumed recap and desktop receipt in normal and scroll views')
        finally:
            terminal.close()
            server.shutdown()
            server.server_close()
