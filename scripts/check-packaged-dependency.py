#!/usr/bin/env python3
"""Validate glass-dev's normalized package dependency contract."""

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
    dependency = manifest["dependencies"]["glass-browser"]
    if dependency.get("version") != f"={args.version}":
        raise SystemExit(f"expected exact glass-browser ={args.version}, got {dependency!r}")
    if "path" in dependency:
        raise SystemExit("packaged glass-dev manifest retained a local path dependency")
    print(f"packaged glass-dev resolves glass-browser exactly at {args.version}")


if __name__ == "__main__":
    main()
