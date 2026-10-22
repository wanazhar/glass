#!/usr/bin/env python3
"""Check installer names, symlink behavior, and collision refusal without a build."""
import json
import os
from pathlib import Path
import subprocess
import tempfile

REPO = Path(__file__).resolve().parents[3]
PRODUCT = REPO / "products/glass-core"
INSTALLER = PRODUCT / "scripts/install.sh"

with tempfile.TemporaryDirectory(prefix="glass-core-install-smoke-") as scratch:
    root = Path(scratch)
    installed = root / "installed"
    bindir = installed / "bin"
    bindir.mkdir(parents=True)
    foreign = bindir / "glass"
    foreign.write_bytes(b"other project's Glass executable")
    alias = bindir / "gc"
    alias.write_bytes(b"unrelated gc executable")
    env = dict(os.environ, CARGO_INSTALL_ROOT=str(installed))
    blocked = subprocess.run([str(INSTALLER)], env=env, capture_output=True, text=True)
    assert blocked.returncode == 1 and "Refusing" in blocked.stderr
    assert alias.read_bytes() == b"unrelated gc executable"
    assert foreign.read_bytes() == b"other project's Glass executable"
    alias.unlink()
    alias.symlink_to("glass")
    blocked = subprocess.run([str(INSTALLER)], env=env, capture_output=True, text=True)
    assert blocked.returncode == 1 and alias.readlink() == Path("glass")
    alias.unlink()

    # Capture the exact cargo arguments; create the binary that cargo would install.
    fake_tools = root / "tools"
    fake_tools.mkdir()
    cargo = fake_tools / "cargo"
    cargo.write_text('''#!/usr/bin/env python3
import json, os, pathlib, sys
pathlib.Path(os.environ["INSTALL_ARGS"]).write_text(json.dumps(sys.argv[1:]))
bindir = pathlib.Path(os.environ["CARGO_INSTALL_ROOT"]) / "bin"
(bindir / "glass-core").write_bytes(b"new harness executable")
''')
    cargo.chmod(0o755)
    args_file = root / "args.json"
    env.update(PATH=str(fake_tools) + os.pathsep + env["PATH"], INSTALL_ARGS=str(args_file))
    for _ in range(2):
        subprocess.run([str(INSTALLER)], env=env, check=True, capture_output=True, text=True)
        assert alias.is_symlink() and alias.readlink() == Path("glass-core")
        assert alias.read_bytes() == b"new harness executable"
        assert foreign.read_bytes() == b"other project's Glass executable"
        actual_args = json.loads(args_file.read_text())
        assert actual_args == [
            "install", "--locked", "--force", "--profile", "core-release",
            "--root", str(installed), "--path", str(REPO / "crates/glass-core"), "--bin", "glass-core",
        ], actual_args
    metadata = json.loads(subprocess.check_output(["cargo", "metadata", "--no-deps", "--format-version", "1"], cwd=REPO, text=True))
    assert not any(p["name"] == "glass" for p in metadata["packages"])
    package = next(p for p in metadata["packages"] if p["name"] == "glass-core")
    assert package["default_run"] == "glass-core"
    assert [t["name"] for t in package["targets"] if "bin" in t["kind"]] == ["glass-core"]
    print("PASS: package/binary glass-core; gc symlink; repeat install; unrelated gc refusal; Glass preserved")
