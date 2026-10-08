#!/usr/bin/env python3
"""Minimal LSP server for the offline smoke: initialize/shutdown, and one
FIXTURE-ERROR diagnostic per opened or changed file."""
import json
import sys


def read_message():
    headers = {}
    while True:
        line = sys.stdin.buffer.readline()
        if not line:
            return None
        line = line.strip()
        if not line:
            break
        key, _, value = line.decode().partition(':')
        headers[key.lower()] = value.strip()
    length = int(headers.get('content-length', 0))
    payload = sys.stdin.buffer.read(length)
    return json.loads(payload.decode())


def send(obj):
    body = json.dumps(obj).encode()
    sys.stdout.buffer.write(b'Content-Length: %d\r\n\r\n' % len(body) + body)
    sys.stdout.buffer.flush()


def main():
    while True:
        message = read_message()
        if message is None:
            break
        method = message.get('method')
        if method == 'initialize':
            send({'jsonrpc': '2.0', 'id': message['id'], 'result': {'capabilities': {}}})
        elif method == 'shutdown':
            send({'jsonrpc': '2.0', 'id': message['id'], 'result': None})
        elif method == 'exit':
            break
        elif method in ('textDocument/didOpen', 'textDocument/didChange'):
            uri = message['params']['textDocument']['uri']
            send(
                {
                    'jsonrpc': '2.0',
                    'method': 'textDocument/publishDiagnostics',
                    'params': {
                        'uri': uri,
                        'diagnostics': [
                            {
                                'range': {
                                    'start': {'line': 0, 'character': 0},
                                    'end': {'line': 0, 'character': 1},
                                },
                                'severity': 1,
                                'message': 'FIXTURE-ERROR undefined thing',
                            }
                        ],
                    },
                }
            )


if __name__ == '__main__':
    main()
