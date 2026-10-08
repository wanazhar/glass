#!/usr/bin/env python3
"""Fixture for the smoke: a Streamable HTTP MCP server (2026-07-28).

Requires the standard `Mcp-Method` header on POSTs; answers `tools/list` as
plain JSON and `tools/call` as an SSE stream, so the bridge exercises both
reply shapes. The port is chosen by the caller (`--port N`)."""
import argparse
import http.server
import json


class Handler(http.server.BaseHTTPRequestHandler):
    def log_message(self, *_):
        pass

    def do_POST(self):
        if not self.headers.get('Mcp-Method'):
            self.send_response(400)
            self.send_header('Content-Length', '0')
            self.end_headers()
            return
        body = json.loads(self.rfile.read(int(self.headers['Content-Length'])))
        method = body.get('method')
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


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--port', type=int, default=0)
    args = parser.parse_args()
    server = http.server.ThreadingHTTPServer(('127.0.0.1', args.port), Handler)
    print(server.server_port, flush=True)
    server.serve_forever()


if __name__ == '__main__':
    main()
