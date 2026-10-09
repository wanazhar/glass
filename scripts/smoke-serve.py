#!/usr/bin/env python3
"""Offline checks for `gc serve` / `gc attach`: socket protocol, replay,
prompts, multi-client broadcast, cancellation from any client, and the
terminal client in a PTY."""
import http.server
import importlib.util
import json
import os
import signal
import socket
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


class Endpoint(http.server.BaseHTTPRequestHandler):
    mode = 'text'
    calls = 0

    def log_message(self, *_):
        pass

    def do_GET(self):
        data = json.dumps({'data': [{'id': 'mock', 'name': 'Mock model'}]}).encode()
        self.send_response(200)
        self.send_header('Content-Length', str(len(data)))
        self.end_headers()
        self.wfile.write(data)

    def do_POST(self):
        self.rfile.read(int(self.headers['Content-Length']))
        type(self).calls += 1
        frames = [{'choices': [{'delta': {'content': 'SERVER-HELLO'}}]}]
        data = [('data: ' + json.dumps(x) + '\n\n').encode() for x in frames]
        self.send_response(200)
        self.send_header('Content-Type', 'text/event-stream')
        self.end_headers()
        try:
            for chunk in data:
                self.wfile.write(chunk)
                self.wfile.flush()
            if type(self).mode == 'hang':
                time.sleep(30)
            self.wfile.write(b'data: [DONE]\n\n')
            self.wfile.flush()
        except (BrokenPipeError, ConnectionResetError):
            pass


class Client:
    def __init__(self, path, since=None):
        self.sock = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
        self.sock.settimeout(20)
        self.sock.connect(str(path))
        self.buf = b''
        self.send({'t': 'attach', **({'since': since} if since is not None else {})})

    def send(self, obj):
        self.sock.sendall((json.dumps(obj) + '\n').encode())

    def next(self, timeout=30):
        deadline = time.monotonic() + timeout
        while b'\n' not in self.buf:
            assert time.monotonic() < deadline, self.buf
            chunk = self.sock.recv(65536)
            assert chunk, 'server closed the socket'
            self.buf += chunk
        line, _, self.buf = self.buf.partition(b'\n')
        return json.loads(line)

    def until_done(self, timeout=30):
        deltas, done = [], None
        deadline = time.monotonic() + timeout
        while time.monotonic() < deadline:
            message = self.next(timeout=deadline - time.monotonic())
            if message['t'] == 'text':
                deltas.append(message['delta'])
            elif message['t'] == 'done':
                done = message
                break
            elif message['t'] == 'error':
                raise AssertionError(message)
        assert done is not None, deltas
        return ''.join(deltas), done

    def close(self):
        self.sock.close()


def main():
    with tempfile.TemporaryDirectory(prefix='gc-serve-') as tmp:
        root = Path(tmp)
        repo = root / 'repo'
        repo.mkdir()
        cfg = root / 'config/glass-core'
        cfg.mkdir(parents=True)
        server = http.server.ThreadingHTTPServer(('127.0.0.1', 0), Endpoint)
        threading.Thread(target=server.serve_forever, daemon=True).start()
        (cfg / 'config.toml').write_text(
            f'provider="openai"\nmodel="mock"\napi_key_env=""\n'
            f'base_url="http://127.0.0.1:{server.server_port}/v1"\ntrust="always"\n'
        )
        env = dict(os.environ, XDG_CONFIG_HOME=str(root / 'config'), XDG_DATA_HOME=str(root / 'data'))
        sock = root / 'session.sock'
        serve = subprocess.Popen(
            [str(f.BINARY), 'serve', '--socket', str(sock)],
            cwd=repo, env=env,
            stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
        )
        deadline = time.monotonic() + 10
        while not sock.exists():
            assert time.monotonic() < deadline, 'serve never created its socket'
            time.sleep(0.05)
        try:
            # First client: replay is empty, a prompt round-trips.
            one = Client(sock)
            replay = one.next()
            assert replay['t'] == 'replay' and replay['entries'] == [], replay
            session_id = replay['session']
            one.send({'t': 'prompt', 'text': 'hello'})
            text, done = one.until_done()
            assert text == 'SERVER-HELLO' and done['stop'] == 'finished', (text, done)

            # A second client attaches alongside the first (no eviction) and
            # gets its own history replay.
            two = Client(sock)
            replay = two.next()
            roles = [e['role'] for e in replay['entries']]
            assert roles == ['user', 'assistant'], replay
            assert replay['entries'][0]['text'] == 'hello', replay

            # Broadcast: a prompt from one client streams to every attached
            # client, each ending with its own done frame.
            one.send({'t': 'prompt', 'text': 'broadcast me'})
            text_one, done_one = one.until_done()
            text_two, done_two = two.until_done()
            assert text_one == 'SERVER-HELLO' and done_one['stop'] == 'finished', (text_one, done_one)
            assert text_two == 'SERVER-HELLO' and done_two['stop'] == 'finished', (text_two, done_two)

            # Cancellation works from any attached client.
            Endpoint.mode = 'hang'
            one.send({'t': 'prompt', 'text': 'cancel me'})
            while True:
                message = one.next()
                if message['t'] == 'text':
                    break
            two.send({'t': 'cancel'})
            _, done = one.until_done(timeout=15)
            assert done['stop'] == 'cancelled', done
            _, done_two = two.until_done(timeout=15)
            assert done_two['stop'] == 'cancelled', done_two
            Endpoint.mode = 'text'

            # Idempotent submissions: the same request_id runs once; a
            # duplicate gets an explicit frame instead of a second turn.
            before = Endpoint.calls
            two.send({'t': 'prompt', 'text': 'dedupe me', 'request_id': 'req-1'})
            _, done = two.until_done()
            assert done['stop'] == 'finished', done
            assert Endpoint.calls == before + 1, Endpoint.calls
            two.send({'t': 'prompt', 'text': 'dedupe me', 'request_id': 'req-1'})
            frame = two.next()
            assert frame['t'] == 'duplicate' and frame['request_id'] == 'req-1', frame
            assert Endpoint.calls == before + 1, 'a duplicate must not reach the model'

            # Request IDs survive a server restart with the same session log.
            one.close()
            two.close()
            serve.terminate()
            serve.wait(timeout=10)
            sock.unlink(missing_ok=True)
            serve = subprocess.Popen(
                [str(f.BINARY), 'serve', session_id, '--socket', str(sock)],
                cwd=repo, env=env,
                stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
            )
            deadline = time.monotonic() + 10
            while not sock.exists():
                assert time.monotonic() < deadline, 'restarted serve never created its socket'
                time.sleep(0.05)
            after_restart = Client(sock)
            replay = after_restart.next()
            assert replay['session'] == session_id, replay
            after_restart.send({'t': 'prompt', 'text': 'dedupe me', 'request_id': 'req-1'})
            frame = after_restart.next()
            assert frame['t'] == 'duplicate' and frame['status'] == 'completed', frame
            assert Endpoint.calls == before + 1, 'a persisted duplicate must not reach the model'
            after_restart.close()
            Endpoint.mode = 'hang'
            crash_client = Client(sock)
            crash_client.next()
            before_crash = Endpoint.calls
            crash_client.send({'t': 'prompt', 'text': 'interrupted request', 'request_id': 'req-crash'})
            assert crash_client.next()['t'] == 'text'
            serve.kill()
            serve.wait(timeout=10)
            crash_client.close()
            sock.unlink(missing_ok=True)

            # An accepted request without a completion record is reported as
            # interrupted and is never automatically submitted a second time.
            serve = subprocess.Popen(
                [str(f.BINARY), 'serve', session_id, '--socket', str(sock)],
                cwd=repo, env=env,
                stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
            )
            deadline = time.monotonic() + 10
            while not sock.exists():
                assert time.monotonic() < deadline, 'serve never resumed the same session'
                time.sleep(0.05)
            after_restart = Client(sock)
            after_restart.next()
            before_duplicate = Endpoint.calls
            after_restart.send({'t': 'prompt', 'text': 'interrupted request', 'request_id': 'req-crash'})
            frame = after_restart.next()
            assert frame['t'] == 'duplicate' and frame['status'] == 'accepted', frame
            assert 'interrupted' in frame['message'], frame
            assert Endpoint.calls == before_duplicate == before_crash + 1
            after_restart.close()
            serve.terminate()
            serve.wait(timeout=10)
            sock.unlink(missing_ok=True)

            Endpoint.mode = 'text'
            serve = subprocess.Popen(
                [str(f.BINARY), 'serve', session_id, '--socket', str(sock)],
                cwd=repo, env=env,
                stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
            )
            deadline = time.monotonic() + 10
            while not sock.exists():
                assert time.monotonic() < deadline, 'serve never resumed after interrupted request'
                time.sleep(0.05)

            # The server survives disconnects and a late attach still gets
            # the whole history.
            one.close()
            two.close()
            time.sleep(0.3)
            three = Client(sock)
            replay = three.next()
            assert len(replay['entries']) >= 3, replay
            assert replay.get('reset') is False, replay
            cursor_before = replay['cursor']
            three.send({'t': 'prompt', 'text': 'again'})
            text, done = three.until_done()
            assert text == 'SERVER-HELLO' and done['stop'] == 'finished', (text, done)

            # A cursor makes reattach incremental: only what is new arrives.
            incremental = Client(sock, since=cursor_before)
            frame = incremental.next()
            assert frame['t'] == 'replay' and frame['reset'] is False, frame
            texts = [e['text'] for e in frame['entries']]
            assert texts == ['again', 'SERVER-HELLO'], texts
            incremental.close()

            # A cursor from another lineage resets instead of pretending to
            # be current.
            stale = Client(sock, since=1_000_000)
            frame = stale.next()
            assert frame['t'] == 'replay' and frame['reset'] is True, frame
            assert len(frame['entries']) >= 6, frame
            stale.close()
            three.close()

            # The terminal client: attach in a PTY, submit one prompt. Screen
            # assertions, not byte windows: the replay already contains older
            # SERVER-HELLO text, and wait() consumes its window on match.
            t = f.Terminal(repo, env, args=('attach', '--socket', str(sock)), size=(80, 24))
            try:
                t.visible('attached', timeout=10)
                t.send('pty hello')
                t.send('\r')
                t.visible('SERVER-HELLO', timeout=15)
                t.visible('— finished —', timeout=15)
            finally:
                t.close()

            # A dropped SSH-style attach reconnects with its saved raw cursor
            # and resends the exact pending ID. Killing the serving process
            # after acceptance must not execute the prompt a second time.
            Endpoint.mode = 'hang'
            t = f.Terminal(repo, env, args=('attach', '--socket', str(sock)), size=(40, 24))
            try:
                t.visible('attached', timeout=10)
                before = Endpoint.calls
                t.send('restart attach exactly once')
                t.send('\r')
                t.visible('waiting for the prior request to resolve', timeout=10)
                deadline = time.monotonic() + 5
                while Endpoint.calls == before:
                    assert time.monotonic() < deadline, 'mock provider did not receive attach request'
                    time.sleep(0.02)
                time.sleep(0.15)  # ensure the request acceptance is durable
                serve.kill()
                serve.wait(timeout=10)
                sock.unlink(missing_ok=True)
                Endpoint.mode = 'text'
                serve = subprocess.Popen(
                    [str(f.BINARY), 'serve', session_id, '--socket', str(sock)],
                    cwd=repo, env=env,
                    stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
                )
                deadline = time.monotonic() + 10
                while not sock.exists():
                    assert time.monotonic() < deadline, 'restarted attach server never created its socket'
                    time.sleep(0.05)
                t.visible('request already handled', timeout=15)
                assert Endpoint.calls == before + 1, 'pending request ID executed twice after reconnect'
                records = [json.loads(line) for path in (root / 'data/glass-core').rglob('*.jsonl')
                           for line in path.read_text().splitlines() if line.strip()]
                occurrences = sum(1 for record in records
                                  if record.get('k') == 'user' and record.get('t') == 'restart attach exactly once')
                assert occurrences == 1, f'prompt was durably appended {occurrences} times'
            finally:
                t.close()
                Endpoint.mode = 'text'

            # Lifecycle: `stop` shuts a server down over its socket, `logs`
            # reads the durable log, `respawn` restarts a detached server.
            # A fresh workspace so `serve` with no id creates its own
            # session instead of shadowing the one above.
            lifecycle = root / 'repo2'
            lifecycle.mkdir()
            default = subprocess.Popen(
                [str(f.BINARY), 'serve'],
                cwd=lifecycle, env=env,
                stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
            )
            data = root / 'data/glass-core'
            deadline = time.monotonic() + 10
            found = []
            while not found:
                assert time.monotonic() < deadline, 'serve never created its default socket'
                found = sorted(data.glob('serve-*.sock'))
                if not found:
                    time.sleep(0.05)
            sock_path = found[0]
            session_id = sock_path.stem[len('serve-'):]

            # logs reads the session log whether or not a server runs.
            out = subprocess.run(
                [str(f.BINARY), 'logs', session_id],
                cwd=lifecycle, env=env, capture_output=True, text=True, timeout=30,
            )
            assert out.returncode == 0, (out.stdout, out.stderr)
            assert session_id in out.stdout, out.stdout

            # stop: acknowledged, then the server exits and removes its
            # socket.
            out = subprocess.run(
                [str(f.BINARY), 'stop', session_id],
                cwd=lifecycle, env=env, capture_output=True, text=True, timeout=30,
            )
            assert out.returncode == 0, (out.stdout, out.stderr)
            assert 'stopped session' in out.stdout, out.stdout
            default.wait(timeout=10)
            deadline = time.monotonic() + 10
            while sock_path.exists():
                assert time.monotonic() < deadline, 'the socket must be removed on exit'
                time.sleep(0.05)

            # respawn: a fresh detached server for the same session, and
            # the same socket path back in place. stdout is DEVNULL so the
            # detached child cannot hold a capture pipe open.
            out = subprocess.run(
                [str(f.BINARY), 'respawn', session_id],
                cwd=lifecycle, env=env,
                stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, timeout=30,
            )
            assert out.returncode == 0, out.returncode
            deadline = time.monotonic() + 10
            while not sock_path.exists():
                assert time.monotonic() < deadline, 'respawn never rebound the socket'
                time.sleep(0.05)
            revived = Client(sock_path)
            frame = revived.next()
            assert frame['t'] == 'replay', frame
            revived.close()
            # Clean the detached server up; nothing else can.
            out = subprocess.run(
                [str(f.BINARY), 'stop', session_id],
                cwd=lifecycle, env=env, capture_output=True, text=True, timeout=30,
            )
            assert out.returncode == 0, (out.stdout, out.stderr)
        finally:
            serve.send_signal(signal.SIGTERM)
            try:
                serve.wait(timeout=5)
            except subprocess.TimeoutExpired:
                serve.kill()
                serve.wait()
            # Best-effort: a detached server left behind by a failed run.
            try:
                subprocess.run(
                    [str(f.BINARY), 'stop'],
                    cwd=repo, env=env,
                    stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, timeout=10,
                )
            except Exception:
                pass
            server.shutdown()
    print(
        'PASS serve/attach: replay, prompt streaming, multi-client broadcast, '
        'cancel from any client, idempotent request_id, cursor-based reattach '
        'with reset, durable PTY reconnect after mock restart with one execution, '
        'stop/logs/respawn'
    )


if __name__ == '__main__':
    main()
