#!/usr/bin/env python3
"""Verify that distributed Glass packages share one release version."""

import json
import pathlib
import re
import subprocess
import sys
import tomllib

root = pathlib.Path(__file__).resolve().parent.parent
metadata = json.loads(
    subprocess.check_output(
        ["cargo", "metadata", "--no-deps", "--locked", "--format-version", "1"],
        cwd=root,
        text=True,
    )
)
cargo_versions = {
    package["name"]: package["version"]
    for package in metadata["packages"]
    if package["name"] in {"glass-browser", "glass-dev"}
}
cargo_version = cargo_versions["glass-browser"]

versions = {
    "Cargo.toml": cargo_version,
    "crates/glass-dev/Cargo.toml": cargo_versions["glass-dev"],
    "clients/python/pyproject.toml": tomllib.loads(
        (root / "clients/python/pyproject.toml").read_text()
    )["project"]["version"],
    "clients/typescript/package.json": json.loads(
        (root / "clients/typescript/package.json").read_text()
    )["version"],
    "clients/typescript/package-lock.json": json.loads(
        (root / "clients/typescript/package-lock.json").read_text()
    )["version"],
    "packages/pi-runtime/package.json": json.loads(
        (root / "packages/pi-runtime/package.json").read_text()
    )["version"],
    "packages/pi-runtime/package-lock.json": json.loads(
        (root / "packages/pi-runtime/package-lock.json").read_text()
    )["version"],

}

client_identity_patterns = {
    "clients/python/glass_client.py clientInfo": (
        root / "clients/python/glass_client.py",
        r'"clientInfo": \{"name": "glass-python-client", "version": "([^"]+)"\}',
    ),
    "clients/typescript/src/index.ts clientInfo": (
        root / "clients/typescript/src/index.ts",
        r'clientInfo: \{ name: "glass-typescript-client", version: "([^"]+)" \}',
    ),
}
for label, (path, pattern) in client_identity_patterns.items():
    match = re.search(pattern, path.read_text())
    if match is None:
        raise SystemExit(f"could not locate {label}")
    versions[label] = match.group(1)

if len(set(versions.values())) == 1:
    version_message = next(iter(versions.values()))
else:
    for path, version in versions.items():
        print(f"{path}: {version}", file=sys.stderr)
    raise SystemExit("Glass package versions are not synchronized")

print(f"Glass package versions synchronized at {version_message}")
