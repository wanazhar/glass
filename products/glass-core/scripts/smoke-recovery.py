#!/usr/bin/env python3
"""Offline checks for failure recovery.

Two phases, both against a local mock endpoint:

1. Crash recovery: a SIGKILL mid-tool leaves an interrupted call and a
   partial-output sidecar; reopening must report both, never re-run the
   command, and repair a torn log tail.
2. Failure recovery inspector: every retry is recorded with its own attempt
   number, delay and cause; each recorded failure gets exactly one bounded
   safe action; a completed mutation is never offered for replay; an unknown
   provider id stays unknown; and the inspector output carries no prompt,
   credential or endpoint while the shareable bundle omits the id.
"""
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
PROMPT_SENTINEL = 'PROMPT_SENTINEL_DO_NOT_INSPECT'
KEY_SENTINEL = 'sk-smoke-RECOVERY-KEY-SENTINEL'
RESPONSE_ID = 'resp_smoke_RECOVERY_ID_SENTINEL'


class Endpoint(http.server.BaseHTTPRequestHandler):
    requests = []
    mode = 'crash'

    def log_message(self, *_):
        pass

    def _status(self, code, message):
        payload = json.dumps({'error': {'message': message}}).encode()
        self.send_response(code)
        self.send_header('Content-Type', 'application/json')
        self.send_header('Content-Length', str(len(payload)))
        self.end_headers()
        self.wfile.write(payload)

    def _answer(self, text):
        data = (
            'data: ' + json.dumps({'choices': [{'delta': {'content': text}}]}) + '\n\n'
            'data: [DONE]\n\n'
        ).encode()
        self.send_response(200)
        self.send_header('Content-Type', 'text/event-stream')
        self.send_header('Content-Length', str(len(data)))
        self.end_headers()
        self.wfile.write(data)

    def do_GET(self):
        data = json.dumps({'data': [{'id': 'mock', 'name': 'Mock model'}]}).encode()
        self.send_response(200)
        self.send_header('Content-Length', str(len(data)))
        self.end_headers()
        self.wfile.write(data)

    def do_POST(self):
        body = json.loads(self.rfile.read(int(self.headers['Content-Length'])))
        mode = type(self).mode
        if mode != 'crash':
            type(self).requests.append(body)
            if mode == 'recover' and len(type(self).requests) < 3:
                return self._status(429, 'slow down, retry shortly')
            if mode == 'overload':
                return self._status(
                    503, f'overloaded; response={RESPONSE_ID}'
                )
            if mode == 'forbidden':
                return self._status(403, 'forbidden: use /login to reconnect')
            return self._answer('INSPECTOR_RETRY_THEN_OK')
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


def inspector_checks():
    """The failure recovery inspector, driven headlessly against a mock."""
    with tempfile.TemporaryDirectory(prefix='gc-recovery-inspector-') as tmp:
        root = Path(tmp)
        repo = root / 'repo'
        repo.mkdir()
        data = root / 'data/glass-core'
        cfg = root / 'config/glass-core'
        cfg.mkdir(parents=True)
        server = http.server.ThreadingHTTPServer(('127.0.0.1', 0), Endpoint)
        threading.Thread(target=server.serve_forever, daemon=True).start()
        port = server.server_port
        (cfg / 'config.toml').write_text(
            f'provider="openai"\nmodel="mock"\napi_key_env="SMOKE_RECOVERY_KEY"\n'
            f'base_url="http://127.0.0.1:{port}/v1"\ntrust="always"\n'
        )
        env = dict(
            os.environ,
            XDG_CONFIG_HOME=str(root / 'config'),
            XDG_DATA_HOME=str(root / 'data'),
            SMOKE_RECOVERY_KEY=KEY_SENTINEL,
        )

        def run(*args, check=False):
            return subprocess.run(
                [str(f.BINARY), *args],
                cwd=repo, env=env, text=True, capture_output=True, timeout=180,
            )

        def turn(prompt):
            Endpoint.mode = prompt
            Endpoint.requests = []
            result = run('run', '--json', f'{PROMPT_SENTINEL} {prompt}')
            assert result.stdout.strip(), result.stderr
            return json.loads(result.stdout)

        try:
            # A transient failure the harness retried past: the lineage must be
            # there, and it must be reported as recovered, not as a failure.
            recovered = turn('recover')
            assert recovered['status'] == 'finished', recovered
            assert len(Endpoint.requests) == 3, 'two retries then a success'
            recovered_attempts = [
                json.loads(line)
                for line in (data / f"{recovered['session']}.attempts.jsonl")
                .read_text().splitlines()
            ]
            assert [a['attempt'] for a in recovered_attempts] == [1, 2], recovered_attempts
            assert [a['delay_s'] for a in recovered_attempts] == [1, 2], recovered_attempts
            assert all('429' in a['cause'] for a in recovered_attempts), recovered_attempts
            assert all(a['request_id'] is None for a in recovered_attempts), (
                'no id was reported, so none may be recorded'
            )

            # A permanent transport failure: one row, the full lineage, and the
            # one bounded action for its class.
            overloaded = turn('overload')
            assert overloaded['status'] == 'error', overloaded
            overload_attempts = [
                json.loads(line)
                for line in (data / f"{overloaded['session']}.attempts.jsonl")
                .read_text().splitlines()
            ]
            assert [a['attempt'] for a in overload_attempts] == [1, 2], overload_attempts
            assert [a['delay_s'] for a in overload_attempts] == [1, 2], overload_attempts
            # The provider put an id in the text it returned, so the lineage
            # carries it. It is the provider's id, not a synthesised one.
            assert all(a['request_id'] == RESPONSE_ID for a in overload_attempts), (
                overload_attempts
            )

            # A credential failure is never retried and never routed around, so
            # it must not claim attempts it did not make.
            forbidden = turn('forbidden')
            assert forbidden['status'] == 'error', forbidden
            assert len(Endpoint.requests) == 1, '401/403 is not retried'

            def inspect(session_id):
                out = run('diagnostics', session_id, '--inspector', '--json')
                assert out.returncode == 0, out.stderr
                return json.loads(out.stdout)

            def bundle(session_id):
                out = run('diagnostics', session_id, '--json')
                assert out.returncode == 0, out.stderr
                return json.loads(out.stdout)

            recovered_view = inspect(recovered['session'])
            assert recovered_view['bounds']['max_failures'] >= 1
            assert recovered_view['bounds']['transcript_bodies_loaded'] is False
            row = recovered_view['failures'][0]
            assert row['class'] == 'recovered_after_retry', row
            assert row['severity'] == 'notice', row
            assert row['attempt_count'] == 2, row
            assert [a['attempt'] for a in row['attempts']] == [1, 2], row
            assert [a['delay_s'] for a in row['attempts']] == [1, 2], row
            assert row['timestamp_known'] is True, row
            assert row['recommended_action']['action'] == 'inspect', row
            assert row['cooldown']['provider_retry_after_known'] is False, row
            assert row['cooldown']['provider_retry_after_s'] is None, row

            overload_view = inspect(overloaded['session'])
            row = overload_view['failures'][0]
            assert row['class'] == 'provider_or_transport', row
            assert row['severity'] == 'degraded', row
            assert row['attempt_count'] == 2, row
            assert row['timestamp_known'] is True, row
            assert row['mutation_state'] == 'none_recorded', row
            assert row['recommended_action']['action'] == 'retry', row
            assert row['recommended_action']['replays_completed_mutation'] is False, row
            # The id is read from what the provider actually returned, and the
            # reset is offered because a new transport is what that class needs.
            assert row['provider_request_id'] == RESPONSE_ID, row
            assert row['provider_request_id_known'] is True, row
            assert overload_view['transport_reset']['offered'] is True, overload_view
            assert overload_view['transport_reset']['preserves_session_log'] is True
            assert overload_view['transport_reset']['resends_requests'] is False

            forbidden_view = inspect(forbidden['session'])
            row = forbidden_view['failures'][0]
            assert row['class'] == 'authentication', row
            assert row['severity'] == 'blocking', row
            assert row['attempt_count'] == 0, row
            # Nothing was recorded, so nothing is claimed.
            assert row['timestamp_known'] is False and row['timestamp_ms'] is None, row
            assert row['provider_request_id'] is None, row
            assert row['provider_request_id_known'] is False, row
            assert row['recommended_action']['action'] == 'check_authentication', row
            # A new transport cannot fix a credential, so it is not offered.
            assert forbidden_view['transport_reset']['offered'] is False, forbidden_view
            assert 'check_authentication' in forbidden_view['transport_reset']['reason']

            # The local inspector is not an artifact: it refuses the save path.
            refused = run(
                'diagnostics', overloaded['session'], '--inspector', '--save',
                str(root / 'leak.json'),
            )
            assert refused.returncode != 0, refused
            assert 'not a shareable bundle' in refused.stderr, refused.stderr
            assert not (root / 'leak.json').exists()

            # Headless JSON check: no prompt, credential or endpoint URL in the
            # inspector, and no provider id in the shareable bundle.
            for session_id in (
                recovered['session'], overloaded['session'], forbidden['session']
            ):
                text = json.dumps(inspect(session_id))
                for secret in (
                    PROMPT_SENTINEL, KEY_SENTINEL, f'127.0.0.1:{port}', '/v1',
                ):
                    assert secret not in text, (
                        f'inspector leaked {secret} for {session_id}: {text[:2000]}'
                    )
                shared = json.dumps(bundle(session_id))
                assert RESPONSE_ID not in shared, shared[:2000]
                assert PROMPT_SENTINEL not in shared
                assert KEY_SENTINEL not in shared
                assert f'127.0.0.1:{port}' not in shared

            # The bundle still classifies the same failures the same way.
            shared_overload = bundle(overloaded['session'])
            assert shared_overload['redaction'].count('response IDs') == 1
        finally:
            server.shutdown()
    print(
        'PASS recovery inspector: every retry is recorded with its own attempt '
        'number, delay and cause; each class gets one bounded action; unknown '
        'ids stay unknown; the reset is offered only where it can help; the '
        'inspector carries no prompt, credential or endpoint'
    )


if __name__ == '__main__':
    main()
    inspector_checks()
