#!/usr/bin/env python3
"""Offline PTY coverage for reader search and mobile anchors."""
import json
import os
from pathlib import Path
import tempfile
import subprocess
from pty_vt import Terminal

with tempfile.TemporaryDirectory(prefix='gc-reader-pty-') as tmp:
    root = Path(tmp)
    repo = root / 'workspace'
    repo.mkdir()
    (repo / 'src').mkdir()
    (repo / 'src/a.rs').write_text('\n'.join([f'line-{i}' for i in range(24)]) + '\n')
    (repo / 'src/b.rs').write_text('before-' + 'x' * 300 + '\n')
    subprocess.run(['git', 'init', '-q'], cwd=repo, check=True)
    subprocess.run(['git', 'config', 'user.email', 'smoke@example.test'], cwd=repo, check=True)
    subprocess.run(['git', 'config', 'user.name', 'Smoke'], cwd=repo, check=True)
    subprocess.run(['git', 'add', '.'], cwd=repo, check=True)
    subprocess.run(['git', 'commit', '-qm', 'baseline'], cwd=repo, check=True)
    before = subprocess.check_output(['git', 'rev-parse', 'HEAD^{tree}'], cwd=repo, text=True).strip()
    (repo / 'src/a.rs').write_text('\n'.join([('changed-1' + 'x' * 300) if i == 1 else (f'changed-{i}' if i == 20 else f'line-{i}') for i in range(24)]) + '\n')
    (repo / 'src/b.rs').write_text('after-' + 'y' * 300 + '\n')
    subprocess.run(['git', 'add', '.'], cwd=repo, check=True)
    after = subprocess.check_output(['git', 'write-tree'], cwd=repo, text=True).strip()
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
        call_id = f'call-{turn}'
        marker = 'TOOL_OUTPUT_SEARCH_MARKER' if turn == 79 else f'end of output {turn}'
        full_output = '\n'.join([f'ordinary output line {i}' for i in range(4)]) + '\n' + marker
        records.extend([
            {'k': 'user', 't': f'question {turn} stable history text'},
            {'k': 'assistant', 't': f'answer {turn} stable response'},
            {'k': 'call', 'id': call_id, 'name': 'bash', 'args': json.dumps({'command': f'echo output-for-{turn}'})},
            {'k': 'bash', 'id': call_id, 'cmd': f'echo output-for-{turn}', 'out': full_output, 'code': 0, 'trunc': False, 'spill': None},
            {'k': 'tool', 'id': call_id, 'name': 'bash', 't': full_output},
        ])
    (data / 'reader-fixture.jsonl').write_text(''.join(json.dumps(r) + '\n' for r in records))
    env = dict(os.environ, XDG_CONFIG_HOME=str(root / 'config'), XDG_DATA_HOME=str(root / 'data'))
    binary = os.environ.get('GLASS_CORE_TEST_BINARY', str(Path(__file__).resolve().parents[1] / 'target/debug/glass-core'))
    for size in [(40, 24), (20, 8)]:
        terminal = Terminal(repo, env, size, args=('resume', 'reader-fixture'))
        try:
            terminal.idle()
            terminal.send('DRAFT_REMAINS')
            terminal.collect(.2)
            terminal.send('\x06')  # Ctrl+F opens directly into reader search.
            terminal.send('TOOL_OUTPUT_SEARCH_MARKER\r')
            terminal.visible('TOOL_OUTPUT_SEARCH_MARKER' if size[0] > 20 else 'TOOL_OUTPUT_SE')
            if size[0] > 20:
                terminal.visible('1 matches')
            terminal.send('\x1b')  # leave search with the result selected
            terminal.collect(.2)
            if size[0] > 20:
                terminal.visible('Tool call-79 · Enter collapse')
            terminal.send('\r')  # collapse the selected exact-ID tool card
            terminal.visible('Tool call-79 · Enter expand' if size[0] > 20 else 'Tool call')
            terminal.send('\r')  # restore the complete output
            if size[0] > 20:
                terminal.visible('Tool call-79 · Enter collapse')
            terminal.send('\x1b')  # close reader
            terminal.collect(.2)
            terminal.visible('DRAFT_REMAINS')
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
        # The palette's /search entry dispatches to the same reader API and
        # starts with a live search query while keeping the draft editor free.
        terminal = Terminal(repo, env, size, args=('resume', 'reader-fixture'))
        try:
            terminal.idle()
            terminal.send('/search\r')
            terminal.send('TOOL_OUTPUT_SEARCH_MARKER\r')
            terminal.visible('TOOL_OUTPUT_SEARCH_MARKER' if size[0] > 20 else 'TOOL_OUTPUT_SE')
            terminal.send('\x1b')
            terminal.collect(.1)
            terminal.send('\x1b')
            terminal.collect(.2)
        finally:
            terminal.close()
        # A real Git tree fixture exercises the file/hunk diff browser and
        # horizontal long-line navigation at both approved terminal sizes.
        checkpoint_dir = data / 'checkpoints/reader-fixture'
        checkpoint_dir.mkdir(parents=True, exist_ok=True)
        (checkpoint_dir / 'fixture.json').write_text(json.dumps({
            'before': before, 'after': after, 'workspace': str(repo), 'label': 'fixture'
        }))
        terminal = Terminal(repo, env, size, args=('resume', 'reader-fixture'))
        try:
            terminal.idle()
            terminal.send('/diff fixture\r')
            terminal.visible('src/a.rs')
            terminal.send('\r')
            terminal.visible('@@')
            terminal.send('\r')
            terminal.visible('+changed-1')
            if size[0] > 20:
                terminal.visible('column 1')
                terminal.send('\x1b[C')
                terminal.visible('column 18')
                terminal.visible('xxxxxxxx')
            else:
                terminal.send('\x1b[C')
                terminal.collect(.2)
                terminal.visible('xxxxxxxx')
            terminal.send('\x1b')
            terminal.collect(.1)
            terminal.send('\x1b')
            terminal.collect(.1)
            terminal.send('\x1b[B')
            terminal.collect(.2)
            terminal.send('\r')
            terminal.visible('src/b.rs')
            terminal.send('\r')
            terminal.visible('@@')
            terminal.send('\x1b')
            terminal.send('\x1b')
        finally:
            terminal.close()

        # Empty activity state, then a real owned job's output and stop action.
        terminal = Terminal(repo, env, size, args=('resume', 'reader-fixture'))
        try:
            terminal.idle()
            terminal.send('/activity\r')
            terminal.visible('echo output-for-' if size[0] > 20 else 'exit 0')
            if size[0] > 20:
                terminal.visible('80 choices')
            terminal.send('\x1b')
            terminal.collect(.2)
            terminal.send('/bg echo ACTIVITY_JOB; sleep 30\r')
            terminal.visible('job 1' if size[0] > 20 else 'job-1-')
            terminal.send('/activity\r')
            terminal.collect(.2)
            terminal.send('job 1')
            terminal.visible('job 1')
            terminal.collect(.2)
            if size[0] > 20:
                terminal.visible('1 choices')
            terminal.send('\r')
            terminal.visible('Read output' if size[0] > 20 else 'Read output')
            terminal.send('\r')
            terminal.visible('ACTIVITY_JOB')
            terminal.send('\x1b')
            terminal.collect(.2)
            terminal.send('\r')
            terminal.visible('Stop job')
            terminal.send('\x1b[B\r')
            terminal.visible('Stop background job?')
            terminal.send('\x1b[B\r')
            terminal.visible('stopped job 1')
        finally:
            terminal.close()
print('PASS: reader full-output search/tool cards, draft preservation, diff file/hunk and long-line navigation, activity job actions, 40x24 and 20x8 PTY')
