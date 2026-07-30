#!/usr/bin/env python3
"""Bind each runtime evidence row to one packaged release artifact."""

import argparse
import hashlib
import json
import pathlib
import subprocess


def fail(message: str) -> None:
    raise SystemExit(f"artifact evidence binding failed: {message}")


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("evidence_dir", type=pathlib.Path)
    parser.add_argument("artifact", type=pathlib.Path)
    parser.add_argument("--target", required=True)
    args = parser.parse_args()

    if not args.evidence_dir.is_dir():
        fail(f"evidence directory is missing: {args.evidence_dir}")
    if not args.artifact.is_file() or not args.artifact.stat().st_size:
        fail(f"packaged artifact is missing or empty: {args.artifact}")

    artifact = {
        "name": args.artifact.name,
        "target": args.target,
        "sha256": hashlib.sha256(args.artifact.read_bytes()).hexdigest(),
        "size_bytes": args.artifact.stat().st_size,
    }
    source_revision = subprocess.check_output(
        ["git", "rev-parse", "HEAD"], text=True
    ).strip()
    paths = sorted(args.evidence_dir.glob("*.json"))
    if not paths:
        fail(f"no JSON evidence rows found in {args.evidence_dir}")
    for path in paths:
        try:
            value = json.loads(path.read_text(encoding="utf-8"))
        except (OSError, json.JSONDecodeError) as error:
            fail(f"{path}: {error}")
        if not isinstance(value, dict):
            fail(f"{path} is not an evidence object")
        value["source_revision"] = source_revision
        value["artifact"] = artifact
        path.write_text(json.dumps(value, indent=2) + "\n", encoding="utf-8")
    print(
        f"bound {len(paths)} evidence rows to {artifact['name']} "
        f"({artifact['sha256']})"
    )


if __name__ == "__main__":
    main()
