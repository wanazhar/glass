#!/usr/bin/env python3
"""Read-only teammate chat navigation over a real PTY with an offline provider."""
import importlib.util
import json
import socket
import tempfile
from pathlib import Path

spec = importlib.util.spec_from_file_location('agent_smoke', Path(__file__).with_name('smoke-agents.py'))
fixture = importlib.util.module_from_spec(spec)
spec.loader.exec_module(fixture)

with tempfile.TemporaryDirectory(prefix='gc-agent-chat-') as tmp:
    root = Path(tmp)
    server, env = fixture.setup(root)
    terminal = fixture.Terminal(root, env, (100, 35))
    try:
        terminal.idle()
        owner = fixture.owner_id(root)
        def request(value):
            with socket.socket(socket.AF_UNIX) as client:
                client.settimeout(10)
                client.connect(str(root/'d/glass-core/agents'/owner/'control.sock'))
                client.sendall((json.dumps(value)+'\n').encode())
                result = json.loads(client.makefile().readline())
                assert result['ok'], result
                return result['data']
        request({'action':'create', 'from':'main', 'name':'review', 'text':'CHECK_READ: inspect the desktop'})
        fixture.ctl(root, env, owner, 'wait', 'review', '10')
        request({'action':'send', 'from':'main', 'id':'review', 'text':'EOF_FOLLOWUP: verify mouse input'})
        fixture.ctl(root, env, owner, 'wait', 'review', '10')
        terminal.collect(.3)
        requests = len(server.requests)
        terminal.send('\x1ba')
        terminal.collect(.3)
        # Enter from the roster must immediately open the full chat.
        terminal.send('\r')
        terminal.collect(.3)
        assert 'Read-only conversation' in terminal.vt.text(), terminal.vt.text()
        assert 'Main agent → Subagent' in terminal.vt.text(), terminal.vt.text()
        assert 'inspect the desktop' in terminal.vt.text(), terminal.vt.text()
        terminal.send('\x1b[F')
        terminal.collect(.2)
        assert 'verify mouse input' in terminal.vt.text(), terminal.vt.text()
        assert 'REVIEW_FOLLOWUP' in terminal.vt.text(), terminal.vt.text()
        terminal.send('\x1b')
        terminal.collect(.2)
        # Enter from Interactions and Transcript must work too.
        terminal.send('\t\r')
        terminal.collect(.3)
        assert 'Main agent → Agent' in terminal.vt.text(), terminal.vt.text()
        terminal.send('\x1b')
        terminal.collect(.2)
        terminal.send('\t\r')
        terminal.collect(.3)
        assert 'Read-only conversation' in terminal.vt.text(), terminal.vt.text()
        terminal.send('this must not send a message')
        terminal.collect(.2)
        assert len(server.requests) == requests, 'viewer input invoked a provider'
        Path('/tmp/gc-agent-chat.ansi').write_text(terminal.seen)
        print('PASS: parent assignments/follow-ups, subagent replies, Enter from all tabs, read-only input; /tmp/gc-agent-chat.ansi')
    finally:
        terminal.close()
        server.shutdown()
        server.server_close()
