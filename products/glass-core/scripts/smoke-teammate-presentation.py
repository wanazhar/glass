#!/usr/bin/env python3
"""Readable teammate activity and complete-record chat pages, including huge tool output."""
import importlib.util
import json
import re
import socket
import tempfile
from pathlib import Path
spec = importlib.util.spec_from_file_location('fixture', Path(__file__).with_name('smoke-agents.py'))
fixture = importlib.util.module_from_spec(spec)
spec.loader.exec_module(fixture)
for size in [(80, 30), (40, 20)]:
    with tempfile.TemporaryDirectory(prefix='gc-present-') as tmp:
        root = Path(tmp)
        server, env = fixture.setup(root)
        terminal = fixture.Terminal(root, env, size)
        try:
            terminal.idle()
            def snapshot(name):
                cells = [[terminal.vt.active.buffer[y][x]._asdict() for x in range(size[0])] for y in range(size[1])]
                Path(f'/tmp/gc-teammate-{name}-{size[0]}.json').write_text(json.dumps(cells))
            owner = fixture.owner_id(root)
            with socket.socket(socket.AF_UNIX) as client:
                client.settimeout(10)
                client.connect(str(root/'d/glass-core/agents'/owner/'control.sock'))
                client.sendall((json.dumps({'action':'create','from':'main','name':'review','text':'CHECK_READ'})+'\n').encode())
                assert json.loads(client.makefile().readline())['ok']
            info = fixture.ctl(root, env, owner, 'wait', 'review', '10')['agent']
            terminal.wait('MAIN_DONE', 10)
            terminal.collect(.3)
            shown = re.sub(r'\x1b\[[0-?]*[ -/]*[@-~]', '', terminal.seen)
            assert 'Teammate · review · completed' in shown, terminal.vt.text()
            for protocol in ['[Teammate completion follow-up]', 'Capabilities:', 'Do not poll agent status', 'Recovery:']:
                assert protocol not in shown, protocol
            snapshot('main')
            log = root/'d/glass-core/agents'/owner/'sessions'/f"{info['session']}.jsonl"
            records = [
                {'k':'user','t':'[main → teammate]\nCHECK_LARGE_EVENT: review these files.'},
                {'k':'call','name':'inspect','args':json.dumps({'action':'list','path':'/tmp/fixture/list'})},
                {'k':'tool','name':'inspect','t':'\n'.join(f'File-{i:04d}-'+'long-path-'*5 for i in range(2400))+'\nEND_TOOL_OUTPUT'},
                {'k':'reasoning','t':'Planning '},
                {'k':'reasoning','t':'the review.'},
                {'k':'assistant','t':'LARGE_RESULT_DONE'},
            ]
            with log.open('a') as out:
                for record in records: out.write(json.dumps(record)+'\n')
            count = len(server.requests)
            terminal.send('\x1ba')
            terminal.collect(.2)
            terminal.send('\r')
            terminal.collect(.3)
            assert 'LARGE_RESULT_DONE' in terminal.vt.text(), terminal.vt.text()
            terminal.send('\x1b[H')
            terminal.collect(.2)
            first = terminal.vt.text()
            assert 'CHECK_LARGE_EVENT' in first, first
            assert 'Earlier record' not in first and 'bytes ' not in first and '"k"' not in first, first
            terminal.send('\x1b[F')
            terminal.collect(.2)
            assert 'LARGE_RESULT_DONE' in terminal.vt.text(), terminal.vt.text()
            assert 'END_TOOL_OUTPUT' not in terminal.vt.text(), terminal.vt.text()
            assert 'Planning the review.' not in terminal.vt.text(), terminal.vt.text()
            snapshot('chat')
            mark = len(terminal.seen)
            terminal.bytes = ''
            terminal.send('t')
            terminal.wait('END_TOOL_OUTPUT', 10)
            terminal.collect(.1)
            terminal.send('\x1b[F')
            terminal.collect(.2)
            assert 'END_TOOL_OUTPUT' in terminal.vt.text(), terminal.vt.text()
            terminal.bytes = ''
            terminal.send('r')
            terminal.wait('Planning the review.', 10)
            terminal.collect(.1)
            terminal.send('\x1b[F')
            terminal.collect(.2)
            assert 'Planning the review.' in terminal.vt.text(), terminal.vt.text()
            assert 'MAIN_DONE' not in terminal.seen[mark:], 'toggling details flashed the main chat'
            terminal.bytes = ''
            with log.open('a') as out: out.write(json.dumps({'k':'assistant','t':'\nLIVE_CHAT_UPDATE'})+'\n')
            terminal.wait('LIVE_CHAT_UPDATE', 10)
            terminal.collect(.1)
            assert 'LIVE_CHAT_UPDATE' in terminal.vt.text(), terminal.vt.text()
            assert len(server.requests) == count, 'viewing detail invoked a provider'
            Path(f'/tmp/gc-teammate-presentation-{size[0]}.ansi').write_text(terminal.seen)
            print(f'PASS {size}: compact main activity, decoded large records, sender labels, tool/reasoning controls, no flash or provider calls')
        finally:
            terminal.close()
            server.shutdown()
            server.server_close()
