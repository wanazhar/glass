#!/usr/bin/env python3
"""Replace an isolated running gateway's binary, then open a saved TUI thread."""
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import tempfile
import urllib.request

ROOT = Path(__file__).resolve().parents[1]
BIN = Path(os.environ.get('GLASS_CORE_TEST_BINARY', ROOT/'target/debug/glass-core')).resolve()
with tempfile.TemporaryDirectory(prefix='gc-gateway-upgrade-') as tmp:
    root = Path(tmp)
    data = root/'data/glass-core'
    cfg = root/'config/glass-core'
    data.mkdir(parents=True)
    cfg.mkdir(parents=True)
    (cfg/'config.toml').write_text('provider="openai"\nmodel="mock"\napi_key_env=""\nbase_url="http://127.0.0.1:1/v1"\ntrust="always"\n')
    (data/'tui-saved.jsonl').write_text(json.dumps({'k':'user', 't':'Saved TUI question'})+'\n'+json.dumps({'k':'assistant','t':'Saved TUI answer'})+'\n')
    (data/'tui-saved.meta.json').write_text(json.dumps({'workspace':str(root),'title':'TUI saved conversation'}))
    installed = root/'glass-core'
    shutil.copy2(BIN, installed)
    env = dict(os.environ, XDG_CONFIG_HOME=str(root/'config'), XDG_DATA_HOME=str(root/'data'), GC_WEB_ASSETS=str(ROOT/'web/dist'))
    gateway = subprocess.run([str(installed), 'connect', '--local'], cwd=root, env=env, capture_output=True, text=True, timeout=15)
    assert gateway.returncode == 0, gateway.stderr
    match = re.search(r'(http://127\.0\.0\.1:\d+)/#(\w+)', gateway.stdout)
    assert match, 'No isolated gateway URL'
    base, token = match.groups()
    def request(path, body=None):
        headers = {'Authorization':'Bearer '+token, 'Origin':base}
        if body is not None: headers['Content-Type'] = 'application/json'
        req = urllib.request.Request(base+path, data=json.dumps(body).encode() if body is not None else None, headers=headers)
        with urllib.request.urlopen(req, timeout=15) as response:
            return json.load(response)
    try:
        assert request('/api/history?id=tui-saved')['records'][-1]['record']['t'] == 'Saved TUI answer'
        # Atomic replacement recreates Linux's '/path/glass-core (deleted)' state.
        replacement = root/'glass-core.next'
        shutil.copy2(BIN, replacement)
        os.replace(replacement, installed)
        assert request('/api/open', {'id':'tui-saved'})['connected'] is True
        assert request('/api/history?id=tui-saved')['records'][-1]['record']['t'] == 'Saved TUI answer'
        print('PASS: a gateway whose executable was replaced can spawn a TUI thread worker without losing history')
    finally:
        subprocess.run([str(BIN), 'connect', 'stop'], cwd=root, env=env, capture_output=True, timeout=10)
        subprocess.run([str(BIN), 'stop', 'tui-saved'], cwd=root, env=env, capture_output=True, timeout=10)
