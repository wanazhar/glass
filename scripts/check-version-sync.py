#!/usr/bin/env python3
"""Verify synchronized Rust, npm, and runtime product versions."""

import json
import pathlib
import re
import subprocess
import sys
import tomllib

root = pathlib.Path(__file__).resolve().parent.parent
workspace = tomllib.loads((root / "Cargo.toml").read_text())["workspace"]
version = workspace["package"]["version"]
metadata = json.loads(
    subprocess.check_output(
        ["cargo", "metadata", "--no-deps", "--locked", "--format-version", "1"],
        cwd=root,
        text=True,
    )
)
packages = {package["name"]: package for package in metadata["packages"]}

glass_packages = {"glass-browser-core", "glass-browser", "glass-dev"}
core_packages = {
    "glass-core",
    "glass-mod-api",
    "glass-runtime-client",
    "glass-tui",
}
errors = []

glass_client_versions = {
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
for path, client_version in glass_client_versions.items():
    if client_version != version:
        errors.append(f"{path}: {client_version} != {version}")

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
    match = re.search(pattern, path.read_text(encoding="utf-8"))
    if match is None:
        errors.append(f"could not locate {label}")
    elif match.group(1) != version:
        errors.append(f"{label}: {match.group(1)} != {version}")

for name in sorted(glass_packages | core_packages):
    package = packages.get(name)
    if package is None:
        errors.append(f"Cargo metadata is missing {name}")
    elif package["version"] != version:
        errors.append(f"Cargo package {name}: {package['version']} != {version}")
    elif name in core_packages and package["license"] != "MIT OR Apache-2.0":
        errors.append(f"Cargo package {name} lost its Core license declaration")

npm_paths = {
    "@glass-core/extension-sdk": "products/glass-core/packages/extension-sdk",
    "@glass-core/typescript-starter": "products/glass-core/extensions/typescript",
    "@glass-core/runtime-client": "products/glass-core/packages/runtime-client",
}
lock = json.loads((root / "products/glass-core/package-lock.json").read_text())
for name, relative in npm_paths.items():
    manifest_path = pathlib.Path(relative) / "package.json"
    manifest = json.loads((root / manifest_path).read_text())
    if manifest.get("version") != version:
        errors.append(f"{manifest_path}: {manifest.get('version')} != {version}")
    lock_key = relative.removeprefix("products/glass-core/")
    lock_entry = lock.get("packages", {}).get(lock_key)
    if lock_entry is None:
        errors.append(f"products/glass-core/package-lock.json is missing {lock_key}")
    elif lock_entry.get("version") != version:
        errors.append(
            f"products/glass-core/package-lock.json {lock_key}: "
            f"{lock_entry.get('version')} != {version}"
        )

runtime_claims = {
    "products/glass-core/extensions/adapters/adapters.py ACP clientInfo": (
        root / "products/glass-core/extensions/adapters/adapters.py",
        r"'clientInfo': \{'name': 'glass-core', 'version': '([^']+)'\}",
    ),
    "products/glass-core/extensions/typescript/src/index.ts extension": (
        root / "products/glass-core/extensions/typescript/src/index.ts",
        r"^\s*version: '([^']+)',",
    ),
    "docs/glass-core-product.md displayed workspace version": (
        root / "docs/glass-core-product.md",
        r"root package version, currently\s+`([^`]+)`",
    ),
}
for label, (path, pattern) in runtime_claims.items():
    match = re.search(pattern, path.read_text(encoding="utf-8"), re.MULTILINE)
    if match is None:
        errors.append(f"could not locate version claim: {label}")
    elif match.group(1) != version:
        errors.append(f"{label}: {match.group(1)} != {version}")

if errors:
    print("\n".join(errors), file=sys.stderr)
    raise SystemExit("Glass package versions are not synchronized to the root workspace")

print(f"Glass and Glass Core package versions synchronized at {version}")
