#!/usr/bin/env python3
"""Verify that distributed Glass packages use the same development version."""

import json
import pathlib
import subprocess
import sys
import tomllib

root = pathlib.Path(__file__).resolve().parent.parent
metadata = json.loads(
    subprocess.check_output(
        ["cargo", "metadata", "--no-deps", "--format-version", "1"],
        cwd=root,
        text=True,
    )
)
cargo_version = next(
    package["version"]
    for package in metadata["packages"]
    if package["name"] == "glass-browser"
)

versions = {
    "Cargo.toml": cargo_version,
    "clients/python/pyproject.toml": tomllib.loads(
        (root / "clients/python/pyproject.toml").read_text()
    )["project"]["version"],
    "clients/typescript/package.json": json.loads(
        (root / "clients/typescript/package.json").read_text()
    )["version"],
    "clients/npm/package.json": json.loads(
        (root / "clients/npm/package.json").read_text()
    )["version"],
}

if len(set(versions.values())) != 1:
    for path, version in versions.items():
        print(f"{path}: {version}", file=sys.stderr)
    raise SystemExit("Glass package versions are not synchronized")

print(f"Glass package versions synchronized at {cargo_version}")
