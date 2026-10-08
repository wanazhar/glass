#!/usr/bin/env python3
"""Offline protocol check for the MCP bridge: namespaced tools from a fixture
stdio server, error text, and the deferred mcp_search/mcp_call mode."""
import json
import os
import subprocess
import sys
import tempfile
from pathlib import Path

REPO = Path(__file__).resolve().parents[1]
BRIDGE = REPO / 'extensions' / 'mcp-bridge' / 'mcp_bridge.py'
FIXTURE = REPO / 'scripts' / 'mcp_fixture_server.py'


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


def config(path, tools=None):
    fixture = [sys.executable, str(FIXTURE)]
    if tools is not None:
        fixture += ['--tools', str(tools)]
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

    print(
        'PASS mcp bridge: namespaced tools with pass-through schemas, error text, '
        'unknown-tool refusal, deferred mcp_search/mcp_call'
    )


if __name__ == '__main__':
    main()
