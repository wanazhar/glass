#!/usr/bin/env python3
"""Offline plugin receipt, tamper, capability and replacement checks."""
import importlib.util
import json
import os
import subprocess
import tempfile
from pathlib import Path

spec = importlib.util.spec_from_file_location('features', Path(__file__).with_name('smoke-features.py'))
f = importlib.util.module_from_spec(spec)
spec.loader.exec_module(f)
SCHEMA = 'https://agent-plugins.org/schemas/1.0.0/plugin.schema.json'


def fixture(path, capability=None, body='First!'):
    (path / 'skills/greet').mkdir(parents=True, exist_ok=True)
    manifest = {'$schema': SCHEMA, 'name': 'demo', 'version': '1.0.0'}
    if capability is not None:
        manifest['extensions'] = {'org.glass-core': {'capabilities': capability}}
    (path / 'plugin.json').write_text(json.dumps(manifest))
    (path / 'skills/greet/SKILL.md').write_text(f'---\nname: greet\ndescription: greet\n---\n\n{body}')


def main():
    with tempfile.TemporaryDirectory(prefix='gc-plugin-integrity-') as temp:
        root = Path(temp); repo = root / 'repo'; repo.mkdir()
        cfg = root / 'config/glass-core'; cfg.mkdir(parents=True)
        env = dict(os.environ, XDG_CONFIG_HOME=str(root / 'config'), XDG_DATA_HOME=str(root / 'data'))

        def cli(*args, ok=True):
            p = subprocess.run([str(f.BINARY), *args], cwd=repo, env=env, text=True, capture_output=True, timeout=60)
            if ok: assert p.returncode == 0, (p.stdout, p.stderr)
            else: assert p.returncode != 0, p.stdout
            return p

        src = root / 'source'; src.mkdir(); fixture(src)
        cli('plugin', 'add', str(src))
        installed = cfg / 'plugins/demo'
        receipt = json.loads((cfg / 'plugins/lock.json').read_text())['plugins']['demo']
        assert len(receipt['hash']) == 64 and receipt['version'] == '1.0.0'
        assert cli('plugin', 'verify').stdout.find('verified') >= 0
        # Same size and preserved mtime still fails cryptographic verification.
        skill = installed / 'skills/greet/SKILL.md'; old = skill.stat(); skill.write_text(skill.read_text().replace('First!', 'Other!'))
        os.utime(skill, ns=(old.st_atime_ns, old.st_mtime_ns))
        assert 'FAILED' in cli('plugin', 'verify', ok=False).stdout
        assert 'integrity verification failed' in cli('plugin', 'list').stderr
        skill.write_text(skill.read_text().replace('Other!', 'First!'))

        # A capability increase requires explicit review and acceptance.
        fixture(src, {'filesystem': ['workspace']}, body='Second!')
        denied = cli('plugin', 'update', 'demo', ok=False)
        assert 'capabilities changed' in denied.stdout and 'requires --accept-permissions' in denied.stderr
        assert 'First!' in skill.read_text()
        cli('plugin', 'update', 'demo', '--accept-permissions')
        assert 'Second!' in skill.read_text()
        data = cfg / 'plugins/data/demo'; data.mkdir(parents=True, exist_ok=True); (data / 'keep').write_text('persistent')
        cli('plugin', 'update', 'demo', '--accept-permissions')
        assert (data / 'keep').read_text() == 'persistent'

        legacy = cfg / 'plugins/legacy'; legacy.mkdir()
        fixture(legacy)
        (legacy / 'plugin.json').write_text((legacy / 'plugin.json').read_text().replace('"demo"', '"legacy"'))
        listing = cli('plugin', 'list')
        assert 'legacy unmanaged install; integrity unverified' in listing.stderr
        assert 'legacy v1.0.0' in listing.stdout

        # Failed source read leaves the installed version available.
        lock = json.loads((cfg / 'plugins/lock.json').read_text()); lock['plugins']['demo']['source'] = str(root / 'missing')
        (cfg / 'plugins/lock.json').write_text(json.dumps(lock))
        failed = cli('plugin', 'update', 'demo', ok=False)
        assert 'Second!' in skill.read_text()

        # Escaping symlinks are rejected at install; unmanaged legacy remains loadable and explicit.
        outside = root / 'outside'; outside.write_text('secret')
        (src / 'escape').symlink_to(outside)
        cli('plugin', 'add', str(src), ok=False)
    print('PASS plugin integrity: SHA256 content/path receipt, tamper detection, capability refusal, accepted staged update, persistent data, failed update preservation, symlink rejection')


if __name__ == '__main__': main()
