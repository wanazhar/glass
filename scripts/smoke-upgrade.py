#!/usr/bin/env python3
"""Install twice in a disposable home and verify durable data is preserved."""
import json
import os
from pathlib import Path
import subprocess
import tempfile

REPO = Path(__file__).resolve().parents[1]

with tempfile.TemporaryDirectory(prefix="glass-core-upgrade-") as scratch:
    root = Path(scratch)
    install = root / "cargo"
    bindir = install / "bin"
    bindir.mkdir(parents=True)
    foreign = bindir / "glass"
    foreign.write_bytes(b"separate Glass executable")
    config = root / "config/glass-core"
    data = root / "data/glass-core"
    config.mkdir(parents=True)
    data.mkdir(parents=True)
    retained = {
        config / "config.toml": b'provider = "openai"\nbase_url = "http://127.0.0.1:1/v1"\nmodel = "upgrade-sentinel"\n',
        config / "auth.json": b'{"active":null,"connections":[],"credentials":{}}',
        data / "history": b"retained composer history\n",
        data / "sentinel.jsonl": b'{"sentinel":"durable session"}\n',
    }
    for path, content in retained.items():
        path.write_bytes(content)
    (config / "auth.json").chmod(0o600)
    env = dict(os.environ, CARGO_INSTALL_ROOT=str(install),
               XDG_CONFIG_HOME=str(root / "config"),
               XDG_DATA_HOME=str(root / "data"))
    for stage in ("clean install", "upgrade"):
        subprocess.run([str(REPO / "scripts/install.sh")], cwd=REPO,
                       env=env, check=True)
        alias = bindir / "gc"
        assert alias.is_symlink() and alias.readlink() == Path("glass-core")
        assert foreign.read_bytes() == b"separate Glass executable"
        for path, content in retained.items():
            assert path.read_bytes() == content, (stage, path)
        assert (config / "auth.json").stat().st_mode & 0o777 == 0o600
        version = subprocess.check_output([str(alias), "--version"], env=env, text=True)
        assert version.startswith("glass-core "), version
        status = json.loads(subprocess.check_output(
            [str(alias), "status", "--json"], cwd=root, env=env, text=True))
        assert status["model"] == "upgrade-sentinel", status
    print("PASS: real clean install and upgrade; gc alias; config/auth/history/session bytes preserved; separate Glass preserved")
