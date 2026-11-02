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
            terminal.send('\x1b[H')
            terminal.collect(.15)
            pages = [terminal.vt.text()]
            for _ in range(8):
                terminal.send('\x1b[6~')
                terminal.collect(.1)
                pages.append(terminal.vt.text())
            scroll_text = '\n'.join(pages)
            assert 'Last request' in scroll_text, scroll_text
            assert '**Last request**' not in scroll_text, scroll_text
            assert '**Latest reply' not in scroll_text, scroll_text
            terminal.send('\x1b')
            terminal.collect(.15)
            terminal.bytes = ''
            terminal.send('LATEST_STREAM_2: give a new reply after this recap.\r')
            terminal.wait('LATEST_REPLY_2', 10)
            terminal.collect(.2)
            for key in ['\x07', '\x1b[5~']:
                terminal.send(key)
                terminal.collect(.2)
                assert 'LATEST_LINE_' in terminal.vt.text() or 'LATEST_REPLY_2' in terminal.vt.text(), terminal.vt.text()
                terminal.send('\x1b[F')
                terminal.collect(.2)
                assert 'LATEST_REPLY_2' in terminal.vt.text(), terminal.vt.text()
                assert 'Recap' not in terminal.vt.text(), terminal.vt.text()
                terminal.send('\x1b')
                terminal.collect(.15)
            assert not (root/'d/glass-core/connect.json').exists()
            Path(f'/tmp/gc-notices-{size[0]}.ansi').write_text(terminal.seen)
            print(f'PASS {size}: resumed recap and desktop receipt in normal and scroll views')
        finally:
            terminal.close()
            server.shutdown()
            server.server_close()
