#!/usr/bin/env python3
"""Offline PTY coverage for the inline conversation reader."""
import json
import os
from pathlib import Path
import tempfile

from pty_vt import Terminal


def line_text(screen, index):
    return ''.join(screen.buffer[index][x].data for x in range(screen.columns))


with tempfile.TemporaryDirectory(prefix='gc-inline-pty-') as tmp:
    root = Path(tmp)
    config = root / 'config/glass-core'
    config.mkdir(parents=True)
    data = root / 'data/glass-core'
    data.mkdir(parents=True)
    (config / 'config.toml').write_text(
        'provider="openai"\nmodel="offline-fixture"\napi_key_env=""\n'
        'base_url="http://127.0.0.1:1/v1"\n'
    )
    session_id = 'inline-fixture'
    records = []
    for turn in range(60):
        marker = 'OLDEST_MARK' if turn == 0 else f'history item {turn}'
        records.append({'k': 'user', 't': f'{marker} request with wrapping text ' * 2})
        records.append({'k': 'assistant', 't': f'assistant response {turn} with wrapping text ' * 2})
    records[-1] = {'k': 'assistant', 't': 'LATEST_INLINE_MARKER at the tail'}
    (data / f'{session_id}.jsonl').write_text(
        ''.join(json.dumps(record) + '\n' for record in records)
    )
    env = dict(os.environ, XDG_CONFIG_HOME=str(root / 'config'), XDG_DATA_HOME=str(root / 'data'))
    binary = os.environ.get(
        'GLASS_CORE_TEST_BINARY', str(Path(__file__).resolve().parents[1] / 'target/debug/glass-core')
    )

    for size in [(65, 30), (40, 24), (24, 16), (20, 12), (20, 8)]:
        terminal = Terminal(root, env, size, args=('resume', session_id))
        try:
            terminal.idle()
            terminal.send('\x1b[<64;1;1M')
            terminal.collect(.15)
            assert 'Conversation' in terminal.vt.text() or 'Log' in terminal.vt.text(), (
                'first idle wheel event must open the inline reader', terminal.vt.text()
            )
            terminal.send('\x1b[5~')
            terminal.collect(.15)
            assert 'LATEST_INLINE_MARKER' not in terminal.vt.text(), 'PageUp must leave the tail'
            terminal.send('\x1b[1;5H')  # Ctrl+Home: oldest recorded entry.
            terminal.visible('OLDEST_MARK')
            assert terminal.seen.count('\x1b[?1049h') == 1, 'inline reader must not nest screens'

            terminal.send('DRAFT_PRESERVED')
            terminal.visible('DRAFT_PRESERVED')
            terminal.send('\x1b[6~')
            terminal.collect(.1)
            terminal.send('\x1b[<64;1;1M')
            terminal.collect(.1)
            terminal.send('\x1b[F')
            terminal.collect(.2)
            assert 'DRAFT_PRESERVED' in terminal.vt.text(), 'draft must survive reader navigation'

            # SIGWINCH is unreliable in this PTY fixture. A navigation event
            # is no longer needed: idle size polling must re-query geometry.
            for resized in [(40, 24), (20, 8), size]:
                terminal.resize(*resized)
                terminal.collect(.2)
                width, height = resized
                divider = line_text(terminal.vt.active, height - 4)
                assert divider.count('─') == width - 1, (resized, divider, terminal.vt.text())
                assert 'DRAFT_PRESERVED' in terminal.vt.text()

            terminal.send('\x1b[5~')
            terminal.collect(.1)
            terminal.send('\x1b')
            terminal.collect(.2)
            assert 'DRAFT_PRESERVED' in terminal.vt.text()
            assert terminal.seen.count('\x1b[?1049h') == 1
        finally:
            terminal.close()

print('PASS: offline inline reading, retained draft, wheel input, five sizes, full-width divider, one app-owned screen')
