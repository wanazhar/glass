#!/usr/bin/env python3
"""Offline protocol check for the MCP bridge: namespaced tools from a fixture
stdio server, error text, the deferred mcp_search/mcp_call mode, the stateless
2026-07-28 revision (server/discover + per-request _meta), and Streamable
HTTP."""
import json
import importlib.util
import os
import socket
import subprocess
import sys
import tempfile
import time
from pathlib import Path

REPO = Path(__file__).resolve().parents[1]
BRIDGE = REPO / 'extensions' / 'mcp-bridge' / 'mcp_bridge.py'
FIXTURE = REPO / 'scripts' / 'mcp_fixture_server.py'
DISCOVER_FIXTURE = REPO / 'scripts' / 'mcp_discover_fixture.py'
HTTP_FIXTURE = REPO / 'scripts' / 'mcp_http_fixture.py'
BRIDGE_MODULE_SPEC = importlib.util.spec_from_file_location('mcp_bridge_smoke', BRIDGE)
BRIDGE_MODULE = importlib.util.module_from_spec(BRIDGE_MODULE_SPEC)
BRIDGE_MODULE_SPEC.loader.exec_module(BRIDGE_MODULE)


class Bridge:
    """One bridge subprocess speaking the glass extension protocol."""

    def __init__(self, config_path):
        env = dict(os.environ, GC_MCP_CONFIG=str(config_path))
        self.proc = subprocess.Popen(
            [sys.executable, str(BRIDGE)],
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
            env=env,
        )
        self.id = 0

    def send(self, obj):
        self.proc.stdin.write(json.dumps(obj) + '\n')
        self.proc.stdin.flush()

    def recv(self):
        line = self.proc.stdout.readline()
        assert line, f'bridge closed the stream: {self.proc.stderr.read()}'
        return json.loads(line)

    def hello(self):
        self.id += 1
        self.send({'type': 'hello', 'id': self.id})
        return self.recv()

    def tool(self, name, arguments):
        self.id += 1
        self.send(
            {
                'type': 'run_tool',
                'id': self.id,
                'name': name,
                'arguments': json.dumps(arguments),
            }
        )
        return self.recv()['output']

    def close(self):
        self.id += 1
        self.send({'type': 'goodbye', 'id': self.id})
        self.proc.stdin.close()
        self.proc.wait(timeout=10)


def config(path, tools=None, extra=()):
    fixture = [sys.executable, str(FIXTURE)]
    if tools is not None:
        fixture += ['--tools', str(tools)]
    fixture += list(extra)
    path.write_text(json.dumps({'servers': {'fixture': {'command': fixture}}}))
    return path


def main():
    with tempfile.TemporaryDirectory(prefix='gc-mcp-') as tmp:
        root = Path(tmp)

        # Normal mode: every tool declared namespaced, schemas passed through.
        bridge = Bridge(config(root / 'mcp.json'))
        manifest = bridge.hello()
        assert manifest.get('protocol') == 1, manifest
        names = {t['name'] for t in manifest['tools']}
        assert names == {'mcp__fixture__echo', 'mcp__fixture__fail'}, names
        echo = next(t for t in manifest['tools'] if t['name'] == 'mcp__fixture__echo')
        assert echo['schema']['properties']['text']['type'] == 'string', echo
        assert echo['schema']['required'] == ['text'], echo
        out = bridge.tool('mcp__fixture__echo', {'text': 'hi'})
        assert out == 'echo: hi', out
        out = bridge.tool('mcp__fixture__fail', {})
        assert out.startswith('error:') and 'it broke' in out, out
        out = bridge.tool('mcp__fixture__nope', {})
        assert 'unknown tool' in out, out
        bridge.close()

        # Deferred mode: above the threshold the manifest is two meta-tools.
        bridge = Bridge(config(root / 'mcp-many.json', tools=45))
        manifest = bridge.hello()
        names = {t['name'] for t in manifest['tools']}
        assert names == {'mcp_search', 'mcp_call'}, names
        out = bridge.tool('mcp_search', {'query': 'tool_4'})
        assert 'mcp__fixture__tool_4' in out, out
        out = bridge.tool('mcp_call', {'tool': 'mcp__fixture__echo', 'arguments': '{"text":"x"}'})
        assert out == 'echo: x', out
        out = bridge.tool('mcp_call', {'tool': 'mcp__fixture__echo', 'arguments': 'not json'})
        assert 'not valid JSON' in out, out
        bridge.close()

        # CacheableResult TTL: the deferred index re-fetches tools/list once
        # the server's ttlMs has expired, so a catalogue that shrank on the
        # server side stops being searchable — while the declared manifest,
        # which the harness has already frozen, stays the two meta-tools.
        bridge = Bridge(
            config(
                root / 'mcp-ttl.json',
                tools=45,
                extra=['--ttl-ms', '0', '--drop-after-first', 'tool_7'],
            )
        )
        manifest = bridge.hello()
        names = {t['name'] for t in manifest['tools']}
        assert names == {'mcp_search', 'mcp_call'}, names
        # tool_7 was dropped by the server's second tools/list: with the
        # refresh it is unsearchable, without it the stale index still
        # answers.
        out = bridge.tool('mcp_search', {'query': 'fixture__tool_7'})
        assert 'mcp__fixture__tool_7' not in out, f'an expired index must be re-fetched: {out}'
        assert 'no tools match' in out, out
        # And the refreshed index still answers for the tools that remain.
        out = bridge.tool('mcp_search', {'query': 'fixture__tool_6'})
        assert 'mcp__fixture__tool_6 —' in out, out
        bridge.close()

        # Stateless stdio (2026-07-28): the server/discover probe selects the
        # revision, and every later request must carry _meta — the fixture
        # errors otherwise, so a working call proves the padding.
        (root / 'discover.json').write_text(json.dumps({
            'servers': {'new': {'command': [sys.executable, str(DISCOVER_FIXTURE)]}}
        }))
        bridge = Bridge(root / 'discover.json')
        manifest = bridge.hello()
        names = {t['name'] for t in manifest['tools']}
        assert names == {'mcp__new__echo'}, names
        out = bridge.tool('mcp__new__echo', {'text': 'x'})
        assert out == 'stateless echo: x', out
        bridge.close()

        # Streamable HTTP: tools/list answered as JSON, tools/call as SSE, the
        # Mcp-Method header required by the fixture; the deprecated SSE
        # transport is refused with a pointer, not silently dropped.
        http = subprocess.Popen(
            [sys.executable, str(HTTP_FIXTURE), '--port', '0'],
            stdout=subprocess.PIPE, text=True,
        )
        try:
            port = int(http.stdout.readline())
            (root / 'http.json').write_text(json.dumps({
                'servers': {
                    'remote': {'type': 'streamable-http', 'url': f'http://127.0.0.1:{port}/mcp'},
                    'legacy': {'type': 'sse', 'url': f'http://127.0.0.1:{port}/sse'},
                }
            }))
            bridge = Bridge(root / 'http.json')
            manifest = bridge.hello()
            names = {t['name'] for t in manifest['tools']}
            assert names == {'mcp__remote__echo'}, names
            out = bridge.tool('mcp__remote__echo', {'text': 'y'})
            assert out == 'http echo: y', out
            bridge.close()
            stderr = bridge.proc.stderr.read()
            assert 'deprecated' in stderr, stderr
        finally:
            http.terminate()
            try:
                http.wait(timeout=5)
            except subprocess.TimeoutExpired:
                http.kill()

        # A classic initialization response may negotiate a different
        # protocol version and assign a transport session. Notifications
        # return 202 without JSON; later requests must echo both headers.
        # Its SSE response deliberately stays open after the matching event.
        legacy_http = subprocess.Popen(
            [sys.executable, str(HTTP_FIXTURE), '--mode', 'stateful', '--port', '0'],
            stdout=subprocess.PIPE, text=True,
        )
        try:
            port = int(legacy_http.stdout.readline())
            (root / 'legacy-http.json').write_text(json.dumps({
                'servers': {
                    'stateful': {'type': 'streamable-http', 'url': f'http://127.0.0.1:{port}/mcp'},
                }
            }))
            bridge = Bridge(root / 'legacy-http.json')
            manifest = bridge.hello()
            names = {t['name'] for t in manifest['tools']}
            assert names == {'mcp__stateful__echo'}, names
            started = time.monotonic()
            out = bridge.tool('mcp__stateful__echo', {'text': 'legacy'})
            elapsed = time.monotonic() - started
            assert out == 'http echo: legacy', out
            assert elapsed >= 0.25, f'partial SSE event was accepted early ({elapsed:.2f}s)'
            assert elapsed < 1.5, f'SSE response waited for stream EOF ({elapsed:.2f}s)'
            bridge.close()
        finally:
            legacy_http.terminate()
            try:
                legacy_http.wait(timeout=5)
            except subprocess.TimeoutExpired:
                legacy_http.kill()

        # A stream that drips bytes cannot extend the original deadline, and
        # oversized event streams are rejected with a bounded memory footprint.
        class FakeSocket:
            timeout = None
            def settimeout(self, value):
                self.timeout = value

        class FakeRaw:
            def __init__(self):
                self._sock = FakeSocket()

        class FakeFile:
            raw = FakeRaw()

        class DripResponse:
            fp = FakeFile()
            def read1(self, _size):
                delay = 0.08
                if self.fp.raw._sock.timeout < delay:
                    raise socket.timeout()
                time.sleep(delay)
                return b': keepalive\n'

        reader = BRIDGE_MODULE.HttpServer.__new__(BRIDGE_MODULE.HttpServer)
        started = time.monotonic()
        try:
            reader._read_sse_response(DripResponse(), {'id': 9}, 0.18)
            raise AssertionError('slow SSE stream did not time out')
        except TimeoutError:
            elapsed = time.monotonic() - started
            assert elapsed < 0.3, f'total deadline was extended ({elapsed:.2f}s)'

        # The numeric loopback range must be checked as an IP address, not as
        # a hostname prefix. These constructor checks do not perform requests.
        original_handshake = BRIDGE_MODULE.HttpServer.handshake
        BRIDGE_MODULE.HttpServer.handshake = lambda _self: BRIDGE_MODULE.LEGACY_VERSION
        try:
            BRIDGE_MODULE.HttpServer('loopback', {'url': 'http://127.0.0.1:1/mcp'})
            try:
                BRIDGE_MODULE.HttpServer(
                    'spoofed', {'url': 'http://127.attacker.test:1/mcp'}
                )
                raise AssertionError('127.* DNS name was accepted for plaintext HTTP')
            except ValueError as error:
                assert 'HTTPS' in str(error), error
        finally:
            BRIDGE_MODULE.HttpServer.handshake = original_handshake

        malformed = BRIDGE_MODULE.HttpServer.__new__(BRIDGE_MODULE.HttpServer)
        malformed.next_id = 0
        malformed.stateless = False
        malformed.post = lambda _payload, _timeout: None
        try:
            malformed.request('tools/list', {}, 1)
            raise AssertionError('empty JSON-RPC response was accepted')
        except RuntimeError as error:
            assert 'invalid JSON-RPC response' in str(error), error

        oversize_http = subprocess.Popen(
            [sys.executable, str(HTTP_FIXTURE), '--mode', 'stateful', '--oversize-stream', '--port', '0'],
            stdout=subprocess.PIPE, text=True,
        )
        try:
            port = int(oversize_http.stdout.readline())
            (root / 'oversize-http.json').write_text(json.dumps({
                'servers': {
                    'oversize': {'type': 'streamable-http', 'url': f'http://127.0.0.1:{port}/mcp'},
                }
            }))
            bridge = Bridge(root / 'oversize-http.json')
            bridge.hello()
            out = bridge.tool('mcp__oversize__echo', {'text': 'large'})
            assert 'exceeds 8 MiB limit' in out, out
            bridge.close()
        finally:
            oversize_http.terminate()
            try:
                oversize_http.wait(timeout=5)
            except subprocess.TimeoutExpired:
                oversize_http.kill()

    print(
        'PASS mcp bridge: namespaced tools with pass-through schemas, error text, '
        'unknown-tool refusal, deferred mcp_search/mcp_call, stateless '
        'server/discover with _meta, Streamable HTTP (JSON + SSE replies), '
        'legacy HTTP version/session negotiation, 202 notification, complete multiline SSE, '
        'open SSE response, total read deadline, 8 MiB response cap, '
        'SSE transport refused'
    )


if __name__ == '__main__':
    main()
