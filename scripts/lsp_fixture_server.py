#!/usr/bin/env python3
"""Minimal LSP server for the offline smoke: initialize/shutdown, diagnostics
derived from the file text, and rename/code-action WorkspaceEdits.

Diagnostics are a pure function of the text so a smoke can drive a real delta
over the wire: `FIXTURE-OK` in the text silences the error, `FIXTURE-WARN`
adds a warning, and re-publishing the same text leaves the set unchanged.

`GC_LSP_FIXTURE_TRIPWIRE` names a file this server writes to if it is ever
asked to run `workspace/applyEdit` or `executeCommand` — the extension must
never do that, and the smoke asserts the file is never created.
"""
import json
import os
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


def tripwire(method):
    path = os.environ.get('GC_LSP_FIXTURE_TRIPWIRE')
    if path:
        with open(path, 'a', encoding='utf-8') as stream:
            stream.write(method + '\n')


def diagnostics_for(text):
    found = []
    if 'FIXTURE-OK' not in text:
        found.append({
            'range': {
                'start': {'line': 0, 'character': 0},
                'end': {'line': 0, 'character': 1},
            },
            'severity': 1,
            'message': 'FIXTURE-ERROR undefined thing',
        })
    if 'FIXTURE-WARN' in text:
        found.append({
            'range': {
                'start': {'line': 1, 'character': 0},
                'end': {'line': 1, 'character': 1},
            },
            'severity': 2,
            'message': 'FIXTURE-WARN unused variable',
        })
    return found


def workspace_edit(params, new_name):
    """One insertion at the requested position, as a WorkspaceEdit."""
    position = params.get('position') or (params.get('range') or {}).get('start') or {'line': 0, 'character': 0}
    return {
        'documentChanges': [
            {
                'textDocument': {'uri': params['textDocument']['uri'], 'version': None},
                'edits': [
                    {'range': {'start': position, 'end': position}, 'newText': new_name + '='},
                ],
            }
        ]
    }


def main():
    while True:
        message = read_message()
        if message is None:
            break
        method = message.get('method')
        if method == 'initialize':
            send({'jsonrpc': '2.0', 'id': message['id'], 'result': {'capabilities': {}}})
        elif method == 'textDocument/rename':
            params = message['params']
            send({'jsonrpc': '2.0', 'id': message['id'],
                  'result': workspace_edit(params, params.get('newName', 'renamed'))})
        elif method == 'textDocument/codeAction':
            params = message['params']
            edit = workspace_edit(params, 'FIXTURE')
            send({'jsonrpc': '2.0', 'id': message['id'], 'result': [
                {'title': 'FIXTURE-RENAME', 'kind': 'refactor', 'edit': edit},
                {'title': 'FIXTURE-SERVER-COMMAND', 'command': 'fixture.doThing'},
            ]})
        elif method in ('workspace/applyEdit', 'workspace/executeCommand'):
            tripwire(method)
            send({'jsonrpc': '2.0', 'id': message['id'], 'error': {'code': -32601, 'message': 'not supported'}})
        elif method.startswith('textDocument/') and 'id' in message:
            send({'jsonrpc': '2.0', 'id': message['id'], 'result': {'method': method, 'params': message['params']}})
        elif method == 'shutdown':
            send({'jsonrpc': '2.0', 'id': message['id'], 'result': None})
        elif method == 'exit':
            break
        elif method in ('textDocument/didOpen', 'textDocument/didChange'):
            document = message['params']['textDocument']
            changes = message['params'].get('contentChanges') or []
            text = changes[0]['text'] if changes else document.get('text', '')
            send(
                {
                    'jsonrpc': '2.0',
                    'method': 'textDocument/publishDiagnostics',
                    'params': {'uri': document['uri'], 'diagnostics': diagnostics_for(text)},
                }
            )


if __name__ == '__main__':
    main()