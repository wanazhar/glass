#!/usr/bin/env python3
"""Verify that distributed Glass packages share one release version."""

import json
import pathlib
import subprocess
import sys
import tomllib

root = pathlib.Path(__file__).resolve().parent.parent
EXPECTED_CANDIDATE = "0.3.1-rc.1"
EXPECTED_PYTHON_CANDIDATE = "0.3.1rc1"
metadata = json.loads(
    subprocess.check_output(
        ["cargo", "metadata", "--no-deps", "--locked", "--format-version", "1"],
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
}

if versions["Cargo.toml"] == EXPECTED_CANDIDATE:
    if versions["clients/typescript/package.json"] != EXPECTED_CANDIDATE:
        raise SystemExit(
            f"clients/typescript/package.json must identify the local candidate "
            f"{EXPECTED_CANDIDATE}, not {versions['clients/typescript/package.json']}"
        )
    if versions["clients/python/pyproject.toml"] != EXPECTED_PYTHON_CANDIDATE:
        raise SystemExit(
            "clients/python/pyproject.toml must use the PEP 440 spelling "
            f"{EXPECTED_PYTHON_CANDIDATE} for {EXPECTED_CANDIDATE}, not "
            f"{versions['clients/python/pyproject.toml']}"
        )
    version_message = (
        f"local candidate {EXPECTED_CANDIDATE} "
        f"(Python spelling: {EXPECTED_PYTHON_CANDIDATE})"
    )
elif len(set(versions.values())) == 1:
    version_message = next(iter(versions.values()))
else:
    for path, version in versions.items():
        print(f"{path}: {version}", file=sys.stderr)
    raise SystemExit("Glass package versions are not synchronized")

print(f"Glass package versions synchronized at {version_message}")
