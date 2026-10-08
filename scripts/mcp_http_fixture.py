#!/usr/bin/env python3
"""Fixture for the smoke: a Streamable HTTP MCP server (2026-07-28).

Requires the standard `Mcp-Method` header on POSTs; answers `tools/list` as
plain JSON and `tools/call` as an SSE stream, so the bridge exercises both
reply shapes. The port is chosen by the caller (`--port N`)."""
import argparse
import http.server
import json
import time


class Handler(http.server.BaseHTTPRequestHandler):
    protocol_version = 'HTTP/1.1'
    mode = 'stateless'
    oversize_stream = False
    def log_message(self, *_):
        pass

    def do_POST(self):
        if self.mode == 'stateless' and not self.headers.get('Mcp-Method'):
            self.send_response(400)
            self.send_header('Content-Length', '0')
            self.end_headers()
            return
        body = json.loads(self.rfile.read(int(self.headers['Content-Length'])))
        method = body.get('method')
        if self.mode == 'stateful':
            if method == 'server/discover':
                reply = {'jsonrpc': '2.0', 'id': body['id'], 'error': {'code': -32601, 'message': 'Method not found'}}
                return self.json_reply(reply)
            if method == 'initialize':
                reply = {
                    'jsonrpc': '2.0', 'id': body['id'],
                    'result': {'protocolVersion': '2025-11-25', 'capabilities': {'tools': {}},
                               'serverInfo': {'name': 'legacy-fixture', 'version': '1'}},
                }
                data = json.dumps(reply).encode()
                self.send_response(200)
                self.send_header('Content-Type', 'application/json')
                self.send_header('MCP-Session-Id', 'fixture-session')
                self.send_header('Content-Length', str(len(data)))
                self.end_headers()
                self.wfile.write(data)
                return
            if self.headers.get('MCP-Session-Id') != 'fixture-session':
                return self.error(400, 'missing session')
            if self.headers.get('MCP-Protocol-Version') != '2025-11-25':
                return self.error(400, 'wrong protocol version')
            if method == 'notifications/initialized':
                self.send_response(202)
                self.send_header('Content-Length', '0')
                self.end_headers()
                return
        if method == 'server/discover':
            reply = {
                'jsonrpc': '2.0',
                'id': body['id'],
                'result': {
                    'protocolVersion': '2026-07-28',
                    'capabilities': {'tools': {}},
                    'serverInfo': {'name': 'http-fixture', 'version': '1'},
                },
            }
        elif method == 'tools/list':
            reply = {
                'jsonrpc': '2.0',
                'id': body['id'],
                'result': {
                    'tools': [
                        {
                            'name': 'echo',
                            'description': 'Echo text back over HTTP.',
                            'inputSchema': {
                                'type': 'object',
                                'properties': {'text': {'type': 'string'}},
                                'required': ['text'],
                            },
                        }
                    ],
                },
            }
        elif method == 'tools/call':
            arguments = (body.get('params') or {}).get('arguments') or {}
            reply = {
                'jsonrpc': '2.0',
                'id': body['id'],
                'result': {
                    'resultType': 'complete',
                    'content': [{'type': 'text', 'text': f"http echo: {arguments.get('text', '')}"}],
                },
            }
            data = ('data: ' + json.dumps(reply) + '\n\n').encode()
            self.send_response(200)
            self.send_header('Content-Type', 'text/event-stream')
            if self.mode == 'stateful':
                self.send_header('Transfer-Encoding', 'chunked')
                self.end_headers()
                if self.oversize_stream:
                    data = b'data: ' + b'x' * (8 * 1024 * 1024 + 1) + b'\n\n'
                    self.wfile.write(f'{len(data):X}\r\n'.encode() + data + b'\r\n')
                    self.wfile.flush()
                    return
                data_lines = ''.join(
                    f'data: {line}\n' for line in json.dumps(reply, indent=1).splitlines()
                ).encode()
                first_end = data_lines.index(b'\n') + 1
                first, second = data_lines[:first_end], data_lines[first_end:] + b'\n'
                self.wfile.write(f'{len(first):X}\r\n'.encode() + first + b'\r\n')
                self.wfile.flush()
                # The partial event must not be treated as a complete reply.
                time.sleep(0.35)
                self.wfile.write(f'{len(second):X}\r\n'.encode() + second + b'\r\n')
                self.wfile.flush()
                # Keep the connection open after the complete JSON-RPC event.
                time.sleep(3)
                self.wfile.write(b'0\r\n\r\n')
                return
            self.send_header('Content-Length', str(len(data)))
            self.end_headers()
            self.wfile.write(data)
            return
        else:
            reply = {'jsonrpc': '2.0', 'id': body.get('id'), 'error': {'code': -32601, 'message': 'Method not found'}}
        data = json.dumps(reply).encode()
        self.send_response(200)
        self.send_header('Content-Type', 'application/json')
        self.send_header('Content-Length', str(len(data)))
        self.end_headers()
        self.wfile.write(data)

    def error(self, status, message):
        self.send_response(status)
        data = message.encode()
        self.send_header('Content-Length', str(len(data)))
        self.end_headers()
        self.wfile.write(data)

    def json_reply(self, reply):
        data = json.dumps(reply).encode()
        self.send_response(200)
        self.send_header('Content-Type', 'application/json')
        self.send_header('Content-Length', str(len(data)))
        self.end_headers()
        self.wfile.write(data)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--port', type=int, default=0)
    parser.add_argument('--mode', choices=('stateless', 'stateful'), default='stateless')
    parser.add_argument('--oversize-stream', action='store_true')
    args = parser.parse_args()
    Handler.mode = args.mode
    Handler.oversize_stream = args.oversize_stream
    server = http.server.ThreadingHTTPServer(('127.0.0.1', args.port), Handler)
    server.daemon_threads = True
    print(server.server_port, flush=True)
    server.serve_forever()


if __name__ == '__main__':
    main()
