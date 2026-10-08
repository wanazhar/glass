#!/usr/bin/env python3
"""Readability checks against actual streamed bytes in a raw PTY, offline."""
import base64
import http.server
import importlib.util
import json
import os
from pathlib import Path
import select
import tempfile
import threading
import time
import unicodedata

spec = importlib.util.spec_from_file_location('features', Path(__file__).with_name('smoke-features.py'))
f = importlib.util.module_from_spec(spec)
spec.loader.exec_module(f)
from pty_vt import VT

INTRO = '# **Workspace overview**\n\nThis is a Rust workspace for **glass-core**, a coding harness with readable conversation rendering. '
PROSE = 'The concurrency implementation coordinates commands while preserving clear feedback and predictable interaction. '
EARLY = INTRO + PROSE * 4 + 'STREAMREADY '
TAIL = '\n\n- **`crates/glass`** contains the main application and concurrency implementation with responsive interaction.\n  * Reading and editing project files should remain comfortable on a phone.\n\n## Next steps\n\n```rust\nfn main() {\n    println!("界 👋 café");\n}\n```\n\nFinal partial line FINISHEDCHAT'
SOURCE = EARLY + TAIL

class Endpoint(http.server.BaseHTTPRequestHandler):
    release = threading.Event()
    mode = 'rich'
    def log_message(self, *_): pass
    def do_POST(self):
        body = json.loads(self.rfile.read(int(self.headers['Content-Length'])))
        mode = type(self).mode
        if mode in ('success', 'failure'):
            results = [message for message in body['messages'] if message['role'] == 'tool']
            latest_user = max(i for i, message in enumerate(body['messages']) if message['role'] == 'user')
            results = [message for message in body['messages'][latest_user + 1:] if message['role'] == 'tool']
            if not results:
                command = "printf 'tool-output-preserved\\n'" if mode == 'success' else "printf 'failure-preview\\n'; exit 3"
                frames = [{'choices': [{'delta': {'tool_calls': [{'index': 0, 'id': mode, 'function': {'name': 'bash', 'arguments': json.dumps({'command': command})}}]}}]}]
            else: frames = [{'choices': [{'delta': {'content': 'Task completed.'}}]}]
        elif mode == 'error':
            data = json.dumps({'error': {'message': 'bad \u001b]52;c;EVIL\u0007 endpoint'}}).encode()
            self.send_response(401); self.send_header('Content-Length', str(len(data))); self.end_headers(); self.wfile.write(data); return
        else: frames = None
        self.send_response(200)
        self.send_header('Content-Type', 'text/event-stream')
        self.end_headers()
        def emit(frame):
            self.wfile.write(('data: ' + json.dumps(frame) + '\n\n').encode()); self.wfile.flush()
        try:
            if frames is not None:
                for frame in frames: emit(frame)
            else:
                # Three-character chunks split delimiters and words repeatedly.
                for index in range(0, len(EARLY), 3):
                    emit({'choices': [{'delta': {'content': EARLY[index:index + 3]}}]})
                assert type(self).release.wait(15), 'Test must release streamed response'
                for index in range(0, len(TAIL), 3):
                    emit({'choices': [{'delta': {'content': TAIL[index:index + 3]}}]})
                    time.sleep(.001)
            emit({'choices': [], 'usage': {'prompt_tokens': 773, 'completion_tokens': 151}})
            self.wfile.write(b'data: [DONE]\n\n'); self.wfile.flush()
        except (BrokenPipeError, ConnectionResetError): pass

class ChatTerminal(f.Terminal):
    """Keep the fixture helpers while parsing actual cursor, width, and screen VT."""
    def __init__(self, workspace, env, size):
        super().__init__(workspace, env, size=size)
        self.vt = VT(*size, alternate_history=True)
        # Cumulative raw stream, for byte-level assertions (colors, bold).
        # Mirrors pty_vt.Terminal.seen.
        self.seen = ''

    def read(self):
        start = len(self.received)
        super().read()
        self.seen += self.received[start:]
        self.vt.feed(self.received[start:])


def session_records(root):
    paths = list((root / 'data/glass-core').glob('*.jsonl'))
    assert paths, 'interactive turn must be recorded'
    records = []
    for path in paths:
        for line in path.read_text().splitlines():
            # Skip anything that is not a record object: a read can race the
            # app's append, and a stray non-object line is not what this
            # assertion is about.
            try:
                obj = json.loads(line)
            except ValueError:
                continue
            if isinstance(obj, dict):
                records.append(obj)
    return records


def screen_rows(t):
    return t.vt.text().splitlines()

def cells(text):
    return sum(0 if unicodedata.combining(char) else 2 if unicodedata.east_asian_width(char) in ('W', 'F') else 1 for char in text)

def collect(t, duration=.08):
    deadline = time.monotonic() + duration
    while time.monotonic() < deadline:
        if select.select([t.master], [], [], .02)[0]: t.read()

def choose(t, label): os.write(t.master, (label + '\r').encode())
def back(t): t.received = ''; os.write(t.master, b'\x1b'); t.wait('\x1b[?25h')

with tempfile.TemporaryDirectory(prefix='gc-chat-design-') as tmp:
    root = Path(tmp); cfg = root / 'config/glass-core'; cfg.mkdir(parents=True)
    server = http.server.ThreadingHTTPServer(('127.0.0.1', 0), Endpoint)
    threading.Thread(target=server.serve_forever, daemon=True).start()
    (cfg / 'config.toml').write_text(f'provider="openai"\nmodel="mock"\napi_key_env=""\nbase_url="http://127.0.0.1:{server.server_port}/v1"\n')
    env = dict(os.environ, XDG_CONFIG_HOME=str(root / 'config'), XDG_DATA_HOME=str(root / 'data'))
    env.pop('NO_COLOR', None)  # This suite exercises colored presentation explicitly.
    try:
        for size in [(65, 30), (40, 24), (24, 16), (20, 12), (20, 8)]:
            Endpoint.mode = 'rich'; Endpoint.release.clear()
            t = ChatTerminal(root, env, size)
            try:
                t.idle(); t.received = ''; t.send('readable chat')
                t.visible('STREAMREADY')
                # Text is visible while the server is paused, before final text/DONE.
                early_frame = t.vt.text()
                early_history = t.vt.history()
                assert 'STREAMREADY' in early_frame, early_frame
                assert 'FINISHEDCHAT' not in early_history + early_frame
                assert 'concurrency' in early_history + early_frame
                Endpoint.release.set()
                t.wait('cost unavailable'); t.idle()
                rows = screen_rows(t)
                history = t.vt.history().splitlines()
                assert all(cells(row) <= size[0] for row in rows + history), (size, rows, history)
                screen = '\n'.join(rows)
                rendered = '\n'.join(history + rows)
                assert '**' not in rendered and '```' not in rendered, rendered
                assert 'cost unavailable' in rendered, rendered
                records = session_records(root)
                assert any(r.get('k') == 'assistant' and r.get('t') == SOURCE for r in records), 'exact provider response must remain in the session'
                assert '\x1b[38;5;252m' in t.seen and '\x1b[1m' in t.seen
                if size[0] == 65:
                    # The transcript is taller than the viewport, so the
                    # heading lives in scrollback; search the full history,
                    # which already includes the visible display.
                    start = next(i for i, row in enumerate(history) if 'Workspace overview' in row)
                    end = next(i for i, row in enumerate(history) if '773 in' in row)
                    body = history[start:end]
                    assert any('Workspace overview' in row for row in body), body
                    assert any('• crates/glass' in row for row in body), body
                    assert any('│ ' in row for row in body), body
                    # This word fits the reading column and must stay whole.
                    assert sum(row.count('concurrency') for row in body) == 5, body
                    Path('/tmp/gc-chat-design-65.txt').write_text('\n'.join(body) + '\n')
                    # Copy the complete raw source (including all split markers).
                    t.send('/responses'); t.visible('Responses'); os.write(t.master, b'\r'); t.visible('Response actions')
                    choose(t, 'Copy response')
                    encoded = base64.b64encode(SOURCE.encode()).decode()
                    t.wait('\x1b]52;c;' + encoded + '\x07'); t.visible('Copy requested'); back(t)
                    t.visible('Response actions'); back(t); t.idle()
            finally:
                Endpoint.release.set(); t.close()
        # Resize while a paragraph is paused; subsequent rows obey the new width.
        Endpoint.mode = 'rich'; Endpoint.release.clear()
        t = ChatTerminal(root, env, (65, 30))
        try:
            t.idle(); t.send('resize streamed paragraph'); t.visible('STREAMREADY'); t.received = ''
            t.vt.resize(20, 12)
            t.resize(20, 12)
            Endpoint.release.set(); t.wait('cost unavailable'); t.idle()
            rows = screen_rows(t)
            assert all(cells(row) <= 20 for row in rows), rows
            assert 'cost unavailable' in '\n'.join(rows), rows
            assert any(r.get('k') == 'assistant' and r.get('t') == SOURCE for r in session_records(root))
        finally: Endpoint.release.set(); t.close()
        t = ChatTerminal(root, env, (40, 24))
        try:
            t.idle()
            for mode in ['success', 'failure']:
                Endpoint.mode = mode; t.received = ''; t.send(mode + ' tool')
                t.wait('Task completed.'); t.idle()
                plain = '\n'.join(screen_rows(t))
                if mode == 'success': assert plain.count('✓ bash') == 1 and '└' not in plain, plain
                else: assert '× bash · failed' in plain and 'failure-preview' in plain, plain
                t.send('/details'); t.visible('Tool details'); os.write(t.master, b'\r')
                t.visible('Tool actions'); choose(t, 'Read command and recorded output')
                t.visible('Command / recorded output')
                # The detail view shows the selected tool's own record.
                t.visible('tool-output-preserved' if mode == 'success' else 'failure-preview')
                back(t); t.visible('Tool actions'); back(t); t.visible('Tool details'); back(t); t.idle()
            Endpoint.mode = 'error'; t.send('bad endpoint'); output = t.wait('provider error'); t.idle(); output += t.received
            assert '\x1b]52' not in output, repr(output)
        finally: t.close()
        Endpoint.mode = 'rich'; Endpoint.release.clear()
        t = ChatTerminal(root, env, (20, 12))
        try:
            t.idle(); t.send('cancel long prose'); t.visible('STREAMREADY')
            assert 'concurrency' in t.vt.history() + t.vt.text()
            os.write(t.master, b'\x03'); t.wait('Cancelled'); t.idle()
            assert 'FINISHEDCHAT' not in t.vt.history() + t.vt.text()
        finally: Endpoint.release.set(); t.close()
    finally: Endpoint.release.set(); server.shutdown()
print('PASS: streamed Markdown, readable word wrapping, styled nested markers, Unicode/code, raw exact copy, live resize, compact tools/errors and cancellation in PTY at 65x30, 40x24, 24x16, 20x12, 20x8')
