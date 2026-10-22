#!/usr/bin/env python3
"""Offline ACP (Agent Client Protocol) checks: initialize, session/new,
streamed prompt updates, tool call updates, cancellation, unknown methods.
Drives `glass-core acp` over stdio against a local mock endpoint."""
import http.server
import importlib.util
import json
import os
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
        has_result = any(m.get('role') == 'tool' for m in body.get('messages', []))
        mode = type(self).mode
        if mode == 'tool' and not has_result:
            frames = [
                {
                    'choices': [
                        {
                            'delta': {
                                'tool_calls': [
                                    {
                                        'index': 0,
                                        'id': 'acp-tool-1',
                                        'function': {
                                            'name': 'bash',
                                            'arguments': json.dumps(
                                                {'command': "printf 'acp-tool-output\\n'"}
                                            ),
                                        },
                                    }
                                ]
                            }
                        }
                    ]
                }
            ]
        elif mode == 'tool':
            frames = [{'choices': [{'delta': {'content': 'ACP-TOOL-DONE'}}]}]
        else:
            frames = [{'choices': [{'delta': {'content': 'ACP-HELLO'}}]}]
        data = [('data: ' + json.dumps(x) + '\n\n').encode() for x in frames]
        self.send_response(200)
        self.send_header('Content-Type', 'text/event-stream')
        self.end_headers()
        try:
            for chunk in data:
                self.wfile.write(chunk)
                self.wfile.flush()
            if mode == 'hang':
                time.sleep(30)
            self.wfile.write(b'data: [DONE]\n\n')
            self.wfile.flush()
        except (BrokenPipeError, ConnectionResetError):
            pass


class Acp:
    def __init__(self, cwd, env):
        self.proc = subprocess.Popen(
            [str(f.BINARY), 'acp'],
            cwd=cwd,
            env=env,
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
        )
        self.id = 0

    def send(self, obj):
        self.proc.stdin.write(json.dumps(obj) + '\n')
        self.proc.stdin.flush()

    def request(self, method, params):
        self.id += 1
        request_id = self.id
        self.send({'jsonrpc': '2.0', 'id': request_id, 'method': method, 'params': params})
        updates = []
        deadline = time.monotonic() + 30
        while True:
            assert time.monotonic() < deadline, (method, updates, self.proc.stderr.read())
            line = self.proc.stdout.readline()
            assert line, f'acp closed the stream: {self.proc.stderr.read()}'
            message = json.loads(line)
            if message.get('method') == 'session/update':
                updates.append(message['params']['update'])
                continue
            if message.get('id') == request_id:
                return message, updates

    def notify(self, method, params):
        self.send({'jsonrpc': '2.0', 'method': method, 'params': params})

    def close(self):
        self.proc.stdin.close()
        self.proc.wait(timeout=10)


def main():
    with tempfile.TemporaryDirectory(prefix='gc-acp-') as tmp:
        root = Path(tmp)
        repo = root / 'repo'
        repo.mkdir()
        cfg = root / 'config/glass-core'
        cfg.mkdir(parents=True)
        server = http.server.ThreadingHTTPServer(('127.0.0.1', 0), Endpoint)
        threading.Thread(target=server.serve_forever, daemon=True).start()
        (cfg / 'config.toml').write_text(
            f'provider="openai"\nmodel="mock"\napi_key_env=""\n'
            f'base_url="http://127.0.0.1:{server.server_port}/v1"\n'
        )
        env = dict(os.environ, XDG_CONFIG_HOME=str(root / 'config'), XDG_DATA_HOME=str(root / 'data'))
        try:
            acp = Acp(repo, env)
            try:
                reply, _ = acp.request(
                    'initialize',
                    {
                        'protocolVersion': 1,
                        'clientCapabilities': {},
                        'clientInfo': {'name': 'smoke', 'version': '1'},
                    },
                )
                result = reply['result']
                assert result['protocolVersion'] == 1, result
                assert result['agentInfo']['name'] == 'glass-core', result
                assert result['authMethods'] == [], result

                reply, _ = acp.request('session/new', {'cwd': str(repo), 'mcpServers': []})
                session_id = reply['result']['sessionId']
                assert session_id, reply

                Endpoint.mode = 'text'
                reply, updates = acp.request(
                    'session/prompt',
                    {'sessionId': session_id, 'prompt': [{'type': 'text', 'text': 'say hi'}]},
                )
                assert reply['result']['stopReason'] == 'end_turn', reply
                chunks = [
                    u for u in updates if u.get('sessionUpdate') == 'agent_message_chunk'
                ]
                assert any('ACP-HELLO' in c['content']['text'] for c in chunks), updates

                Endpoint.mode = 'tool'
                reply, updates = acp.request(
                    'session/prompt',
                    {'sessionId': session_id, 'prompt': [{'type': 'text', 'text': 'use a tool'}]},
                )
                assert reply['result']['stopReason'] == 'end_turn', reply
                calls = [u for u in updates if u.get('sessionUpdate') == 'tool_call']
                assert calls and calls[0]['kind'] == 'execute', updates
                done = [
                    u
                    for u in updates
                    if u.get('sessionUpdate') == 'tool_call_update' and u.get('status') == 'completed'
                ]
                assert done, updates
                assert 'acp-tool-output' in json.dumps(done), done

                # Cancellation: a stalled stream ends with the protocol's
                # `cancelled` stop reason, not an error.
                Endpoint.mode = 'hang'
                acp.id += 1
                prompt_id = acp.id
                acp.send(
                    {
                        'jsonrpc': '2.0',
                        'id': prompt_id,
                        'method': 'session/prompt',
                        'params': {
                            'sessionId': session_id,
                            'prompt': [{'type': 'text', 'text': 'cancel me'}],
                        },
                    }
                )
                deadline = time.monotonic() + 15
                cancelled = False
                while time.monotonic() < deadline:
                    line = acp.proc.stdout.readline()
                    assert line, acp.proc.stderr.read()
                    message = json.loads(line)
                    if message.get('id') == prompt_id and 'result' in message:
                        assert message['result']['stopReason'] == 'cancelled', message
                        cancelled = True
                        break
                    if (
                        message.get('method') == 'session/update'
                        and message['params']['update'].get('sessionUpdate')
                        == 'agent_message_chunk'
                    ):
                        acp.notify('session/cancel', {'sessionId': session_id})
                assert cancelled, 'session/cancel must end the turn as cancelled'

                reply, _ = acp.request('session/load', {'sessionId': session_id, 'cwd': str(repo)})
                assert reply['error']['code'] == -32601, reply
            finally:
                acp.close()
        finally:
            server.shutdown()
    print(
        'PASS acp: initialize/session-new, streamed message chunks, tool_call updates, '
        'session/cancel ends as cancelled, unsupported methods answer -32601'
    )


if __name__ == '__main__':
    main()
