#!/usr/bin/env python3
"""Offline checks for Agent Plugins: install/list/remove via `gc plugin`, the
skill reaches the system prompt, and a plugin's mcp.json server is callable
through mcp-bridge. Local fixtures only."""
import http.server
import importlib.util
import json
import os
import subprocess
import sys
import tempfile
import threading
from pathlib import Path

spec = importlib.util.spec_from_file_location(
    'features', Path(__file__).with_name('smoke-features.py')
)
f = importlib.util.module_from_spec(spec)
spec.loader.exec_module(f)

REPO = Path(__file__).resolve().parents[1]
MCP_FIXTURE = REPO / 'scripts' / 'mcp_fixture_server.py'
BRIDGE = REPO / 'extensions' / 'mcp-bridge' / 'mcp_bridge.py'
PLUGIN_SCHEMA = 'https://agent-plugins.org/schemas/1.0.0/plugin.schema.json'
MCP_SCHEMA = 'https://agent-plugins.org/schemas/1.0.0/mcp.schema.json'


def write_plugin(directory, name='demo-plugin'):
    (directory / 'skills' / 'greet').mkdir(parents=True, exist_ok=True)
    (directory / 'plugin.json').write_text(
        json.dumps(
            {
                '$schema': PLUGIN_SCHEMA,
                'name': name,
                'version': '1.2.0',
                'description': 'Smoke fixture plugin',
            }
        )
    )
    (directory / 'skills' / 'greet' / 'SKILL.md').write_text(
        '---\nname: greet\ndescription: PLUGIN-SKILL-DESCRIPTION says hello\n---\n\nHello!'
    )
    (directory / 'mcp.json').write_text(
        json.dumps(
            {
                '$schema': MCP_SCHEMA,
                'mcpServers': {
                    'fixture': {
                        'type': 'stdio',
                        'command': sys.executable,
                        'args': [str(MCP_FIXTURE)],
                        'env': {'DATA_DIR': '${PLUGIN_DATA}/fixture'},
                        'cwd': '${PLUGIN_ROOT}',
                    }
                },
            }
        )
    )


class Endpoint(http.server.BaseHTTPRequestHandler):
    saw_skill = False
    saw_tool = False

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
        text = json.dumps(body)
        tools = [m for m in body.get('messages', []) if m.get('role') == 'tool']
        if 'PLUGIN-SKILL-DESCRIPTION' in text:
            type(self).saw_skill = True
        if tools and 'echo: plugin-ok' in json.dumps(tools):
            type(self).saw_tool = True
            frames = [{'choices': [{'delta': {'content': 'PLUGIN-SMOKE-OK'}}]}]
        elif tools:
            frames = [{'choices': [{'delta': {'content': 'BAD-TOOL-OUTPUT'}}]}]
        else:
            frames = [
                {
                    'choices': [
                        {
                            'delta': {
                                'tool_calls': [
                                    {
                                        'index': 0,
                                        'id': 'plugin-call-1',
                                        'function': {
                                            'name': 'mcp__demo-plugin.fixture__echo',
                                            'arguments': json.dumps({'text': 'plugin-ok'}),
                                        },
                                    }
                                ]
                            }
                        }
                    ]
                }
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
    with tempfile.TemporaryDirectory(prefix='gc-plugin-') as tmp:
        root = Path(tmp)
        repo = root / 'repo'
        repo.mkdir()
        cfg = root / 'config/glass-core'
        cfg.mkdir(parents=True)
        server = http.server.ThreadingHTTPServer(('127.0.0.1', 0), Endpoint)
        threading.Thread(target=server.serve_forever, daemon=True).start()
        bridge = json.dumps([f'python3 {BRIDGE}'])
        (cfg / 'config.toml').write_text(
            f'provider="openai"\nmodel="mock"\napi_key_env=""\n'
            f'base_url="http://127.0.0.1:{server.server_port}/v1"\n'
            f'trust="always"\nextensions={bridge}\n'
        )
        env = dict(os.environ, XDG_CONFIG_HOME=str(root / 'config'), XDG_DATA_HOME=str(root / 'data'))

        def cli(*args, ok=True):
            p = subprocess.run(
                [str(f.BINARY), *args], cwd=repo, env=env, text=True, capture_output=True, timeout=60
            )
            if ok:
                assert p.returncode == 0, (p, p.stderr)
            return p

        try:
            # Install a fixture plugin from a directory path.
            source = root / 'source-plugin'
            write_plugin(source)
            p = cli('plugin', 'add', str(source))
            assert 'installed plugin `demo-plugin`' in p.stdout, p.stdout
            installed = cfg / 'plugins' / 'demo-plugin'
            assert installed.is_dir(), 'plugin must be copied under the config dir'

            p = cli('plugin', 'list', '--json')
            listing = json.loads(p.stdout)['plugins']
            assert listing and listing[0]['name'] == 'demo-plugin', listing
            assert listing[0]['skills'] == ['greet'] and listing[0]['mcp'] is True, listing
            assert listing[0]['drift'] is False, listing

            # Installing again is refused, not silently overwritten.
            p = cli('plugin', 'add', str(source), ok=False)
            assert p.returncode != 0 and 'already installed' in (p.stdout + p.stderr), p

            # End to end: the skill reaches the prompt, the plugin's MCP server
            # becomes a callable namespaced tool.
            p = cli('run', '--json', 'use the plugin')
            r = json.loads(p.stdout)
            assert r['status'] == 'finished' and r['text'] == 'PLUGIN-SMOKE-OK', r
            assert Endpoint.saw_skill, 'plugin skill missing from the prompt'
            assert Endpoint.saw_tool, 'plugin MCP tool did not run'
            assert (cfg / 'plugins' / 'data' / 'demo-plugin').is_dir(), (
                '${PLUGIN_DATA} must be created for plugin subprocesses'
            )

            # Remove: the tree goes, the data directory is kept and reported.
            p = cli('plugin', 'remove', 'demo-plugin')
            assert 'removed plugin `demo-plugin`' in p.stdout, p.stdout
            assert not installed.exists()
            assert (cfg / 'plugins' / 'data' / 'demo-plugin').is_dir()
        finally:
            server.shutdown()
    print(
        'PASS agent plugins: add/list/remove, skill injection, plugin mcp.json '
        'server callable, ${PLUGIN_DATA} created, duplicate install refused'
    )


if __name__ == '__main__':
    main()
