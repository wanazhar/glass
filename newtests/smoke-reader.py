#!/usr/bin/env python3
"""Offline PTY coverage for reader search and mobile anchors."""
import json
import os
from pathlib import Path
import tempfile
import sys
sys.path.insert(0, str(Path(__file__).resolve().parents[1] / 'scripts'))
from pty_vt import Terminal

with tempfile.TemporaryDirectory(prefix='gc-reader-pty-') as tmp:
    root = Path(tmp)
    config = root / 'config/glass-core'
    config.mkdir(parents=True)
    data = root / 'data/glass-core'
    data.mkdir(parents=True)
    (config / 'config.toml').write_text(
        'provider="openai"\nmodel="offline-fixture"\napi_key_env=""\n'
        'base_url="http://127.0.0.1:1/v1"\n'
    )
    records = []
    for turn in range(80):
        records.extend([
            {'k': 'user', 't': f'question {turn} stable history text'},
            {'k': 'assistant', 't': f'answer {turn} stable response'},
            {'k': 'tool', 't': f'command: echo TOOL_OUTPUT_SEARCH_MARKER\nfull output for {turn} ' + 'long line ' * 8},
        ])
    (data / 'reader-fixture.jsonl').write_text(''.join(json.dumps(r) + '\n' for r in records))
    env = dict(os.environ, XDG_CONFIG_HOME=str(root / 'config'), XDG_DATA_HOME=str(root / 'data'))
    binary = os.environ.get('GLASS_CORE_TEST_BINARY', str(Path(__file__).resolve().parents[1] / 'target/debug/glass-core'))
    for size in [(40, 24), (20, 8)]:
        terminal = Terminal(root, env, size, args=('resume', 'reader-fixture'))
        try:
            terminal.idle()
            terminal.send('DRAFT_REMAINS')
            terminal.send('\x02')  # Ctrl+B opens reader at the top.
            terminal.collect(.1)
            terminal.send('\x06')  # Ctrl+F enters reader search.
            terminal.send('TOOL_OUTPUT_SEARCH_MARKER\r')
            terminal.visible('TOOL_OUTPUT_SEARCH_MARKER' if size[0] > 20 else 'TOOL_OUTPUT_SE')
            if size[0] > 20:
                terminal.visible('matches')
            terminal.resize(20, 8)
            terminal.collect(.2)
            terminal.visible('TOOL_OUTPUT_SE')
            terminal.send('\x1b')  # cancel search
            terminal.collect(.1)
            terminal.send('\x1b')  # close reader
            terminal.collect(.2)
            terminal.visible('DRAFT_REMAINS')
        finally:
            terminal.close()
print('PASS: Ctrl+F full-output search, query cancellation, draft independence, 40x24 and 20x8 PTY')
