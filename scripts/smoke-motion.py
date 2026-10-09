#!/usr/bin/env python3
"""Offline PTY proof for palette colors, active-work motion and input ownership."""
import fcntl
import http.server
import importlib.util
import json
import os
from pathlib import Path
import re
import select
import signal
import struct
import subprocess
import tempfile
import threading
import time

spec = importlib.util.spec_from_file_location('features', Path(__file__).with_name('smoke-features.py'))
f = importlib.util.module_from_spec(spec); spec.loader.exec_module(f)
ANSWER = '# Motion and color\n\n**Warm emphasis**, `mint code`, and a readable neutral body.\n\n- Active work animates.\n- Completed answers stay still.\n\n```rust\nfn main() { println!("hello"); }\n```\n\nFinished.'

class Endpoint(http.server.BaseHTTPRequestHandler):
    mode = 'waiting'; gate = threading.Event(); models = threading.Event()
    def log_message(self, *_): pass
    def do_GET(self):
        assert self.models.wait(15)
        data = json.dumps({'data': [{'id': 'mock', 'name': 'Mock'}]}).encode()
        self.send_response(200); self.send_header('Content-Length', str(len(data))); self.end_headers(); self.wfile.write(data)
    def do_POST(self):
        body = json.loads(self.rfile.read(int(self.headers['Content-Length'])))
        latest = max(i for i, message in enumerate(body['messages']) if message['role'] == 'user')
        results = [message for message in body['messages'][latest + 1:] if message['role'] == 'tool']
        self.send_response(200); self.send_header('Content-Type', 'text/event-stream'); self.end_headers()
        try:
            if self.mode == 'tool' and not results:
                delta = {'tool_calls': [{'index': 0, 'id': 'slow', 'function': {'name': 'bash', 'arguments': json.dumps({'command': 'sleep 2; printf motion-tool-output'})}}]}
            else:
                assert self.gate.wait(15)
                delta = {'content': ANSWER}
            self.wfile.write(('data: ' + json.dumps({'choices': [{'delta': delta}]}) + '\n\ndata: [DONE]\n\n').encode()); self.wfile.flush()
        except (BrokenPipeError, ConnectionResetError): pass

def collect(t, duration=.8):
    t.received = ''; deadline = time.monotonic() + duration
    while time.monotonic() < deadline:
        if select.select([t.master], [], [], .02)[0]: t.read()
    return t.received

def plain(text): return re.sub(r'\x1b\[[0-9;?]*[ -/]*[@-~]', '', text)
def key(t, text): os.write(t.master, text.encode())
def choose(t, text): key(t, '\x15' + text + '\r')
def back(t): t.received = ''; key(t, '\x1b'); t.wait('\x1b[?25h')
def animated(output, colored=True):
    assert len(set(re.findall(r'[◐◓◑◒]', output))) >= 3, repr(output)
    assert '\n' not in output, 'Animation must not enter terminal scrollback'
    # The pump runs often; visual redraws remain bounded to eight per second.
    assert output.count('\x1b[2K') <= 8, repr(output)
    if colored: assert '\x1b[38;5;81m' in output and '\x1b[38;5;183m' in output

def has_color(output):
    return any(any((30 <= int(code) <= 49) or (90 <= int(code) <= 107) for code in args.split(';') if code) for args in re.findall(r'\x1b\[([0-9;]*)m', output))

with tempfile.TemporaryDirectory(prefix='gc-motion-') as tmp:
    root = Path(tmp); repo = root / 'repo'; repo.mkdir(); subprocess.run(['git', 'init', '-q', str(repo)], check=True)
    cfg = root / 'config/glass-core'; cfg.mkdir(parents=True)
    server = http.server.ThreadingHTTPServer(('127.0.0.1', 0), Endpoint); threading.Thread(target=server.serve_forever, daemon=True).start()
    (cfg / 'config.toml').write_text(f'provider="openai"\nmodel="mock"\napi_key_env=""\nbase_url="http://127.0.0.1:{server.server_port}/v1"\n')
    env = dict(os.environ, XDG_CONFIG_HOME=str(root / 'config'), XDG_DATA_HOME=str(root / 'data')); env.pop('NO_COLOR', None)
    try:
        for size in [(65, 30), (40, 24), (24, 16), (20, 12), (20, 8)]:
            Endpoint.mode = 'waiting'; Endpoint.gate.clear()
            t = f.Terminal(repo, env, size=size)
            try:
                t.idle(); assert '─' in t.screen.text(), 'Composer divider should be visible'
                t.send('active motion'); t.wait('Waiting' if size[0] < 32 else 'Waiting for provider')
                output = collect(t); animated(output)
                if size == (65, 30): Path('/tmp/gc-visual-motion-65.ansi').write_text(output)
                # A draft and a multiline-paste viewport cannot be erased by ticks.
                key(t, 'draft'); t.visible('❯ draft'); output = collect(t, .35); assert 'draft' in t.screen.text()
                key(t, '\x15\x1b[200~first line\nsecond line\nlast line\x1b[201~'); t.visible('last line')
                output = collect(t, .35); assert 'last line' in t.screen.text(); assert '\n' not in output
                key(t, '\x03'); t.wait('Cancelled'); t.idle()
                settled = collect(t, .4); assert not re.search(r'[◐◓◑◒]', settled), repr(settled)
                # Finished prose uses coordinated distinct semantic colors.
                Endpoint.gate.set(); t.send('colorful answer'); output = t.wait('Finished.'); t.idle()
                if size[0] == 65:
                    Path('/tmp/gc-visual-chat-65.ansi').write_text(output)
                    assert '\x1b[38;5;183m' in output and '\x1b[38;5;114m' in output and '\x1b[38;5;252m' in output
                settled = collect(t, .4); assert not re.search(r'[◐◓◑◒]', settled)
                assert not t.screen.invalid_positions, (size, t.screen.invalid_positions)
            finally: Endpoint.gate.set(); t.close()
        # Active real shell tools use the same animation without erasing the draft.
        Endpoint.mode = 'tool'; Endpoint.gate.set(); t = f.Terminal(repo, env, size=(65, 30))
        try:
            t.idle(); t.send('slow tool'); t.wait('bash running'); animated(collect(t))
            key(t, 'tool draft'); t.visible('tool draft'); collect(t, .25); assert 'tool draft' in t.screen.text()
            t.wait('Finished.'); t.visible('❯ tool draft'); assert not re.search(r'[◐◓◑◒]', collect(t, .3)); key(t, '\x15'); t.idle()
            # Delayed catalog checks animate inside an alternate-screen modal.
            Endpoint.models.clear(); t.send('/doctor'); t.visible('Diagnostics'); choose(t, 'Check connection'); t.visible('Checking provider')
            output = collect(t); assert len(set(re.findall(r'[◐◓◑◒]', output))) >= 3
            assert '\x1b[38;5;81m' in output and '\x1b[38;5;183m' in output
            key(t, '\x1b'); t.wait('\x1b[?25h'); t.idle(); Endpoint.models.set()
            # Profile persistence keeps old indices and supports new palettes.
            t.send('/theme'); t.visible('Appearance'); choose(t, 'Orchid'); t.idle()
            t.send('/profiles'); t.visible('Workspace profiles'); choose(t, 'Save current'); t.visible('Save workspace profile'); key(t, 'motion profile\r'); t.visible('Profile saved'); back(t); t.visible('Workspace profiles'); back(t); t.idle()
            store = json.loads((cfg / 'profiles.json').read_text()); assert store['profiles']['motion profile']['theme'] == 3
            assert 'reduced_motion' not in store['profiles']['motion profile']
        finally: Endpoint.models.set(); t.close()
        Endpoint.mode = 'waiting'; Endpoint.gate.set(); t = f.Terminal(repo, env)
        try:
            startup = t.wait('glass-core'); t.idle(); assert '\x1b[38;5;177m' in startup + t.received
            t.send('/theme'); t.visible('Appearance'); choose(t, 'Aurora'); t.idle()
        finally: t.close()
        # Live resize preserves a busy draft and subsequent small-screen positions.
        Endpoint.gate.clear(); t = f.Terminal(repo, env, size=(65, 30))
        try:
            t.idle(); t.send('resize motion'); t.wait('Waiting for provider'); key(t, 'resize draft'); t.visible('resize draft')
            t.resize(20, 8); t.visible('resize draft'); output = collect(t); animated(output); assert 'resize draft' in t.screen.text()
            key(t, '\x03'); t.wait('Cancelled'); t.idle(); assert not t.screen.invalid_positions
        finally: Endpoint.gate.set(); t.close()
        # Neutral color affects presentation only; motion and exact headless JSON remain.
        neutral = dict(env, NO_COLOR='1'); Endpoint.gate.clear(); t = f.Terminal(repo, neutral, size=(65, 30))
        try:
            startup = t.wait('glass-core'); t.idle(); startup += t.received; assert not has_color(startup)
            t.send('neutral motion'); t.wait('Waiting for provider'); output = collect(t); animated(output, colored=False); assert not has_color(output)
            Endpoint.gate.set(); output = t.wait('Finished.'); t.idle(); assert not has_color(output)
            for command, title in [('/theme', 'Appearance'), ('/help', 'Help')]:
                t.received = ''; t.send(command); output = t.wait(title); assert not has_color(output); back(t); t.idle()
            t.received = ''; key(t, '\x07'); output = t.wait('following'); assert not has_color(output); back(t); t.idle()
        finally: Endpoint.gate.set(); t.close()
        result = subprocess.run([str(f.BINARY), 'run', '--json', 'headless source'], cwd=repo, env=neutral, text=True, capture_output=True, timeout=10)
        assert result.returncode == 0, result.stderr
        assert json.loads(result.stdout)['text'] == ANSWER and '\x1b' not in result.stdout
    finally: Endpoint.gate.set(); Endpoint.models.set(); server.shutdown()
print('PASS: five semantic palettes; changing provider/tool/modal frames and colors; bounded no-scroll ticks; busy single/multiline drafts; live resize and cancel; settled idle silence; profile persistence; NO_COLOR including transcript; exact headless JSON; PTY at 65x30, 40x24, 24x16, 20x12, 20x8')
