#!/usr/bin/env python3
"""Offline doctor CLI and modal smoke: no network, no secret/path echo."""
import json
import os
from pathlib import Path
import socket
import subprocess
import tempfile

from pty_vt import Terminal

ROOT = Path(__file__).resolve().parents[1]
BIN = Path(os.environ.get('GLASS_CORE_TEST_BINARY', ROOT / 'target/debug/glass-core'))


def run(args, env):
    return subprocess.run([str(BIN), *args], cwd=ROOT, env=env, text=True,
                          stdout=subprocess.PIPE, stderr=subprocess.PIPE, timeout=10)


with tempfile.TemporaryDirectory(prefix='gc-doctor-smoke-') as tmp:
    root = Path(tmp)
    config_dir = root / 'config' / 'glass-core'
    config_dir.mkdir(parents=True)
    data_dir = root / 'data' / 'glass-core'
    data_dir.mkdir(parents=True)
    workspace = root / 'workspace'
    workspace.mkdir()

    # A listening local endpoint proves that ordinary doctor execution makes
    # no provider request. Its URL is intentionally sensitive fixture data.
    listener = socket.socket()
    listener.bind(('127.0.0.1', 0))
    listener.listen()
    listener.settimeout(.15)
    port = listener.getsockname()[1]
    env = dict(os.environ, XDG_CONFIG_HOME=str(root / 'config'),
               XDG_DATA_HOME=str(root / 'data'), GC_DOCTOR_SECRET_ENV='ENV_SECRET_DO_NOT_PRINT')
    missing_name = 'GC_DOCTOR_MISSING_ENV_NAME'
    config_path = config_dir / 'config.toml'
    config_path.write_text(
        'provider = "openai"\nmodel = "offline-fixture"\n'
        f'base_url = "http://url-user:url-pass@127.0.0.1:{port}/v1?access_token=URL_SECRET_DO_NOT_PRINT"\n'
        f'api_key_env = "{missing_name}"\n'
        f'extensions = ["{root / "not-an-executable"}"]\n'
        'sandbox = "strict"\n'
    )
    (data_dir / 'broken.jsonl').write_text('{"k":"user","t":"ok"}\nnot-json\n')
    result = run(['doctor', '--json'], env)
    assert result.returncode == 0, result.stderr
    report = json.loads(result.stdout)
    assert report['status'] == 'ok', result.stdout
    assert report['report']['schema'] == 1
    assert any(c['area'] == 'authentication' and c['status'] == 'notice' for c in report['report']['checks'])
    assert any(c['area'] == 'sessions' and 'malformed record' in c['detail'] for c in report['report']['checks'])
    for secret in ('url-user', 'url-pass', 'URL_SECRET_DO_NOT_PRINT', 'ENV_SECRET_DO_NOT_PRINT', missing_name, str(root)):
        assert secret not in result.stdout + result.stderr, secret
    try:
        listener.accept()
        raise AssertionError('default doctor connected to the configured endpoint')
    except TimeoutError:
        pass
    finally:
        listener.close()

    # An available credential source is reported as state only; neither its
    # variable name nor contents are emitted.
    config_path.write_text(
        'provider = "openai"\napi_key_env = "GC_DOCTOR_SECRET_ENV"\n'
        'base_url = "https://secret.invalid/path?token=OTHER_SECRET"\n'
    )
    result = run(['doctor', '--json'], env)
    assert result.returncode == 0, result.stderr
    for secret in ('GC_DOCTOR_SECRET_ENV', 'ENV_SECRET_DO_NOT_PRINT', 'OTHER_SECRET'):
        assert secret not in result.stdout + result.stderr, secret

    # Malformed TOML may contain a secret on the error span. Doctor JSON uses
    # a fixed error and never includes parser excerpts.
    config_path.write_text('provider = "openai"\ncredential = "TOML_SECRET_DO_NOT_PRINT"\ninvalid = [\n')
    result = run(['doctor', '--json'], env)
    assert result.returncode != 0
    assert 'TOML_SECRET_DO_NOT_PRINT' not in result.stdout + result.stderr
    assert 'Configuration could not be read or parsed' in result.stdout
    assert json.loads(result.stdout)['status'] == 'error'

    # Exercise the modal's first (offline) action at both supported terminal
    # sizes. A dead local URL catches accidental network activity in the modal.
    config_path.write_text('provider = "openai"\napi_key_env = ""\nbase_url = "http://127.0.0.1:1/v1"\n')
    env.pop('GC_DOCTOR_SECRET_ENV', None)
    for width, height in ((40, 24), (20, 8)):
        terminal = Terminal(workspace, env, (width, height))
        try:
            terminal.wait('glass-core')
            terminal.send('/doctor\r')
            terminal.wait('Local offline')
            terminal.send('\r')
            terminal.wait('terminal')
            terminal.send('\x1b')
            terminal.collect(.1)
        finally:
            terminal.close()

print('PASS doctor: offline/no-network JSON, missing/present auth redaction, malformed config redaction, 40x24 and 20x8 modal')
