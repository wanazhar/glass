#!/usr/bin/env python3
"""Validate the normalized Glass package dependency chain."""

import argparse
import pathlib
import tarfile
import tomllib


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("crate", type=pathlib.Path)
    parser.add_argument("--version", required=True)
    args = parser.parse_args()
    with tarfile.open(args.crate, "r:gz") as archive:
        manifest_name = next(
            name for name in archive.getnames() if name.endswith("/Cargo.toml")
        )
        manifest = tomllib.loads(archive.extractfile(manifest_name).read().decode())
    product = manifest["package"]["name"]
    dependency_name = {
        "glass-dev": "glass-browser",
        "glass-browser": "glass-browser-core",
    }.get(product)
    if dependency_name is None:
        raise SystemExit(f"unsupported package dependency contract: {product}")
    dependency = manifest["dependencies"][dependency_name]
    if dependency.get("version") != f"={args.version}":
        raise SystemExit(
            f"expected exact {dependency_name} ={args.version}, got {dependency!r}"
        )
    if "path" in dependency:
        raise SystemExit(f"packaged {product} retained a local path dependency")
    expected_features = ["native-engine"] if product == "glass-dev" else []
    if dependency.get("features", []) != expected_features:
        raise SystemExit(
            f"packaged {product} must request {expected_features!r} on {dependency_name}"
        )
    if product == "glass-browser" and dependency.get("default-features") is not False:
        raise SystemExit("packaged glass-browser must disable core default features")
    print(f"packaged {product} resolves {dependency_name} exactly at {args.version}")


if __name__ == "__main__":
    main()
