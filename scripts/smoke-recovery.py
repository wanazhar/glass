#!/usr/bin/env python3
"""Offline checks for crash recovery: a SIGKILL mid-tool leaves an interrupted
call and a partial-output sidecar; reopening must report both, never re-run
the command, and repair a torn log tail. Local mock endpoint only."""
import http.server
import importlib.util
import json
import os
import signal
import subprocess
import tempfile
import threading
import time
from pathlib import Path

spec = importlib.util.spec_from_file_location(
    'features', Path(__file__).with_name('smoke-features.py')
)
f = importlib.util.module_from_spec(spec)
spec.loader.exec_module(f)

PARTIAL = 'PARTIAL-TEXT-BEFORE-CRASH'
INTERRUPTED = 'was killed while running and may have partially run'


class Endpoint(http.server.BaseHTTPRequestHandler):
    requests = []

    def log_message(self, *_):
        pass

    def do_GET(self):
        data = json.dumps({'data': [{'id': 'mock', 'name': 'Mock model'}]}).encode()
        self.send_response(200)
        self.send_header('Content-Length', str(len(data)))
        self.end_headers()
        self.wfile.write(data)

    def do_POST(self):
        body = json.loads(self.rfile.read(int(self.headers['Content-Length'])))
        type(self).requests.append(body)
        has_tool = any(m.get('role') == 'tool' for m in body.get('messages', []))
        if has_tool:
            frames = [{'choices': [{'delta': {'content': 'RESUMED-OK'}}]}]
        else:
            command = 'echo run >> counter.txt; echo WORKING; sleep 10'
            frames = [
                {'choices': [{'delta': {'content': PARTIAL + ' '}}]},
                {
                    'choices': [
                        {
                            'delta': {
                                'tool_calls': [
                                    {
                                        'index': 0,
                                        'id': 'crash-1',
                                        'function': {
                                            'name': 'bash',
                                            'arguments': json.dumps({'command': command}),
                                        },
                                    }
                                ]
                            }
                        }
                    ]
                },
            ]
        data = (''.join('data: ' + json.dumps(x) + '\n\n' for x in frames) + 'data: [DONE]\n\n').encode()
        self.send_response(200)
        self.send_header('Content-Type', 'text/event-stream')
        self.send_header('Content-Length', str(len(data)))
        self.end_headers()
        try:
            self.wfile.write(data)
        except (BrokenPipeError, ConnectionResetError):
            pass


def main():
    with tempfile.TemporaryDirectory(prefix='gc-recovery-') as tmp:
        root = Path(tmp)
        repo = root / 'repo'
        repo.mkdir()
        data = root / 'data/glass-core'
        cfg = root / 'config/glass-core'
        cfg.mkdir(parents=True)
        server = http.server.ThreadingHTTPServer(('127.0.0.1', 0), Endpoint)
        threading.Thread(target=server.serve_forever, daemon=True).start()
        (cfg / 'config.toml').write_text(
            f'provider="openai"\nmodel="mock"\napi_key_env=""\n'
            f'base_url="http://127.0.0.1:{server.server_port}/v1"\ntrust="always"\n'
            f'live_flush_ms = 50\n'
        )
        env = dict(os.environ, XDG_CONFIG_HOME=str(root / 'config'), XDG_DATA_HOME=str(root / 'data'))

        def run(*args):
            return subprocess.run(
                [str(f.BINARY), 'run', '--json', *args],
                cwd=repo, env=env, text=True, capture_output=True, timeout=120,
            )

        try:
            # 1. Start a turn that streams text and then runs a slow tool, and
            #    SIGKILL the process while the tool is running.
            crash = subprocess.Popen(
                [str(f.BINARY), 'run', '--json', 'crash probe'],
                cwd=repo, env=env,
                stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True,
            )
            counter = repo / 'counter.txt'
            deadline = time.monotonic() + 30
            while not counter.exists():
                assert time.monotonic() < deadline, 'the tool never started'
                time.sleep(0.05)
            sidecars = list(data.glob('*.live.json'))
            deadline = time.monotonic() + 15
            while time.monotonic() < deadline:
                sidecars = list(data.glob('*.live.json'))
                if sidecars and PARTIAL in sidecars[0].read_text():
                    break
                time.sleep(0.05)
            assert sidecars and PARTIAL in sidecars[0].read_text(), (
                'the partial must be committed before the crash',
                [p.read_text() for p in sidecars],
            )
            crash.send_signal(signal.SIGKILL)
            crash.wait(timeout=10)
            assert counter.read_text().count('run') == 1

            logs = sorted(data.glob('*.jsonl'), key=lambda p: p.stat().st_mtime)
            session_id = logs[-1].stem
            sidecar = data / f'{session_id}.live.json'
            assert sidecar.exists(), 'a kill leaves the sidecar behind'
            # 2. Resume: the interrupted call and the partial must both be in
            #    the next request, and the command must NOT run again.
            deadline = time.monotonic() + 20
            while True:
                resumed = run('--resume', session_id, 'continue')
                if resumed.returncode == 0:
                    break
                if 'session is already open for writing' not in resumed.stderr or time.monotonic() >= deadline:
                    raise AssertionError(resumed)
                # A killed harness may leave a spawned command finishing.
                # The lock correctly prevents recovery until its descriptor
                # closes; retry the same non-effectful resume after release.
                time.sleep(0.1)
            assert resumed.returncode == 0, resumed
            body = json.loads(resumed.stdout)
            assert body['status'] == 'finished' and body['text'] == 'RESUMED-OK', body

            last = json.dumps(Endpoint.requests[-1])
            assert INTERRUPTED in last, 'the interrupted call must be answered'
            assert PARTIAL in last, 'the recovered partial must reach the model'
            assert 'crash-1' in last, 'the tool result is paired with its call'
            assert counter.read_text().count('run') == 1, (
                'the interrupted command must not be re-run'
            )
            assert not sidecar.exists(), 'the sidecar is consumed on open'

            # 3. A hard kill mid-append leaves a torn tail; the next open
            #    truncates it and the next append parses cleanly.
            with open(logs[-1], 'a') as fh:
                fh.write('{"k":"assis')  # torn fragment, no newline
            after_tear = run('--resume', session_id, 'after tear')
            assert after_tear.returncode == 0, after_tear
            assert 'repaired a truncated session log' in after_tear.stderr, after_tear.stderr
            healed = json.loads(after_tear.stdout)
            assert healed['status'] == 'finished', healed
            # The recovered-then-healed log parses line by line.
            for line in (data / f'{session_id}.jsonl').read_text().splitlines():
                json.loads(line)
            assert not list(data.glob('*.live.json')), 'a clean run leaves no sidecar'
        finally:
            server.shutdown()
    print(
        'PASS crash recovery: the killed tool is answered as interrupted, the '
        'partial output is recovered, the command is not re-run, and a torn '
        'log tail is truncated before the next append'
    )


if __name__ == '__main__':
    main()
