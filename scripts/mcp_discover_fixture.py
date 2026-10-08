#!/usr/bin/env python3
"""Fixture for the smoke: a stateless (2026-07-28) MCP server over stdio.

It requires `_meta` on every request, so working tools prove the bridge pads
requests per the stateless revision, and it answers `server/discover`.
"""
import json
import sys


def read_message():
    line = sys.stdin.buffer.readline()
    if not line:
        return None
    return json.loads(line.decode())


def send(obj):
    sys.stdout.write(json.dumps(obj) + '\n')
    sys.stdout.flush()


def main():
    while True:
        message = read_message()
        if message is None:
            break
        method = message.get('method')
        if method == 'server/discover':
            send({
                'jsonrpc': '2.0',
                'id': message['id'],
                'result': {
                    'protocolVersion': '2026-07-28',
                    'capabilities': {'tools': {}},
                    'serverInfo': {'name': 'discover-fixture', 'version': '1'},
                },
            })
            continue
        # Every other request must carry the stateless _meta block.
        meta = (message.get('params') or {}).get('_meta') or {}
        if 'io.modelcontextprotocol/protocolVersion' not in meta:
            send({
                'jsonrpc': '2.0',
                'id': message.get('id'),
                'error': {'code': -32600, 'message': 'missing stateless _meta'},
            })
            continue
        if method == 'tools/list':
            send({
                'jsonrpc': '2.0',
                'id': message['id'],
                'result': {
                    'tools': [
                        {
                            'name': 'echo',
                            'description': 'Echo text back, statelessly.',
                            'inputSchema': {
                                'type': 'object',
                                'properties': {'text': {'type': 'string'}},
                                'required': ['text'],
                            },
                        }
                    ],
                    'ttlMs': 60000,
                    'cacheScope': 'private',
                },
            })
        elif method == 'tools/call':
            arguments = (message.get('params') or {}).get('arguments') or {}
            send({
                'jsonrpc': '2.0',
                'id': message['id'],
                'result': {
                    'resultType': 'complete',
                    'content': [{'type': 'text', 'text': f"stateless echo: {arguments.get('text', '')}"}],
                },
            })
        elif 'id' in message:
            send({'jsonrpc': '2.0', 'id': message['id'], 'error': {'code': -32601, 'message': 'Method not found'}})


if __name__ == '__main__':
    main()
