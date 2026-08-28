#!/usr/bin/env python3
"""Write evidence that all client surfaces passed against one exact artifact."""

import argparse
import hashlib
import json
import os
import pathlib
import platform
import subprocess


TARGETS = {
    "x86_64-unknown-linux-gnu": ("linux", "x86_64"),
    "aarch64-unknown-linux-gnu": ("linux", "aarch64"),
    "x86_64-apple-darwin": ("macos", "x86_64"),
    "aarch64-apple-darwin": ("macos", "aarch64"),
}


def fail(message: str) -> None:
    raise SystemExit(f"client compatibility evidence failed: {message}")


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--target", required=True)
    parser.add_argument("--artifact", required=True, type=pathlib.Path)
    parser.add_argument("--output", required=True, type=pathlib.Path)
    parser.add_argument(
        "--typescript-status",
        default=os.environ.get("GLASS_CLIENT_TS_STATUS"),
        choices=("passed", "failed"),
    )
    parser.add_argument(
        "--python-status",
        default=os.environ.get("GLASS_CLIENT_PY_STATUS"),
        choices=("passed", "failed"),
    )
    parser.add_argument(
        "--npm-launcher-status",
        default=os.environ.get("GLASS_CLIENT_NPM_STATUS"),
        choices=("passed", "failed"),
    )
    args = parser.parse_args()
    if args.typescript_status != "passed" or args.python_status != "passed" or args.npm_launcher_status != "passed":
        fail("client surfaces were not observed passing against this artifact")
    if args.target not in TARGETS:
        fail(f"unsupported target: {args.target}")
    if not args.artifact.is_file() or not args.artifact.stat().st_size:
        fail(f"artifact is missing or empty: {args.artifact}")

    os_name, architecture = TARGETS[args.target]
    report = {
        "schema_version": 1,
        "type": "client_compatibility_evidence",
        "source_revision": subprocess.check_output(
            ["git", "rev-parse", "HEAD"], text=True
        ).strip(),
        "target": args.target,
        "runner": {
            "os": os.environ.get("RUNNER_OS", platform.system()),
            "architecture": os.environ.get("RUNNER_ARCH", platform.machine()),
            "image": os.environ.get("ImageOS", "not-recorded"),
            "image_version": os.environ.get("ImageVersion", "not-recorded"),
        },
        "artifact": {
            "name": args.artifact.name,
            "sha256": hashlib.sha256(args.artifact.read_bytes()).hexdigest(),
            "size_bytes": args.artifact.stat().st_size,
        },
        "clients": {
            "typescript": {
                "status": args.typescript_status,
                "commands": ["npm run build", "npm run typecheck", "node smoke.mjs", "npm pack --dry-run"],
            },
            "python": {
                "status": args.python_status,
                "commands": ["python3 -m pip wheel --no-deps .", "python3 smoke.py"],
            },
            "npm_launcher": {
                "status": args.npm_launcher_status,
                "commands": ["npm pack", "npm install", "glass --version", "glass capabilities"],
            },
        },
        "runtime_certification": "certified",
    }
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
    print(f"client compatibility evidence recorded: {args.target}")


if __name__ == "__main__":
    main()
