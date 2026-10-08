#!/usr/bin/env python3
"""Offline checks for `gc worktree`: list finds an isolated checkout and
apply brings its edits (including new files) back into the workspace."""
import importlib.util
import json
import os
import subprocess
import tempfile
from pathlib import Path

spec = importlib.util.spec_from_file_location(
    'features', Path(__file__).with_name('smoke-features.py')
)
f = importlib.util.module_from_spec(spec)
spec.loader.exec_module(f)


def run(args, cwd, env=None, ok=True):
    p = subprocess.run(args, cwd=cwd, env=env, text=True, capture_output=True, timeout=60)
    if ok:
        assert p.returncode == 0, (args, p.stdout, p.stderr)
    return p


def git(repo, *args):
    run(['git', '-C', str(repo), '-c', 'user.email=t@t', '-c', 'user.name=t', *args], repo)


def main():
    with tempfile.TemporaryDirectory(prefix='gc-worktree-') as tmp:
        root = Path(tmp)
        repo = root / 'repo'
        repo.mkdir()
        git(repo, 'init', '-q')
        (repo / 'app.py').write_text('value = 1\n')
        git(repo, 'add', '-A')
        git(repo, 'commit', '-qm', 'init')

        cfg = root / 'config/glass-core'
        cfg.mkdir(parents=True)
        (cfg / 'config.toml').write_text('trust="always"\n')
        env = dict(os.environ, XDG_CONFIG_HOME=str(root / 'config'), XDG_DATA_HOME=str(root / 'data'))
        wts = root / 'data/glass-core/subagents/worktrees'
        wts.mkdir(parents=True)
        checkout = wts / 'task-1'
        git(repo, 'worktree', 'add', '--detach', str(checkout), 'HEAD')
        (checkout / 'app.py').write_text('value = 2\n')
        (checkout / 'new.py').write_text('fresh = True\n')

        p = run([str(f.BINARY), 'worktree', 'list', '--json'], repo, env)
        listing = json.loads(p.stdout)['worktrees']
        assert len(listing) == 1 and listing[0]['name'] == 'task-1', listing
        assert listing[0]['changed_files'] == 2, listing

        p = run([str(f.BINARY), 'worktree', 'apply', 'task-1', '--remove'], repo, env)
        assert 'applied 2 changed file(s)' in p.stdout, p.stdout
        assert (repo / 'app.py').read_text() == 'value = 2\n'
        assert (repo / 'new.py').read_text() == 'fresh = True\n'
        assert not checkout.exists(), 'checkout removed on request'

        p = run([str(f.BINARY), 'worktree', 'list', '--json'], repo, env)
        assert json.loads(p.stdout)['worktrees'] == [], p.stdout
    print(
        'PASS worktree: list finds the checkout, apply brings edits and new '
        'files back, --remove cleans up'
    )


if __name__ == '__main__':
    main()
