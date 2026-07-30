#!/usr/bin/env python3
"""Merge exact-artifact client compatibility evidence across native targets."""

import json
import pathlib
import subprocess
import sys


TARGETS = {
    "client-glass-linux-x86_64": ("glass-linux-x86_64", "x86_64-unknown-linux-gnu"),
    "client-glass-linux-arm64": ("glass-linux-arm64", "aarch64-unknown-linux-gnu"),
    "client-glass-macos-x86_64": ("glass-macos-x86_64", "x86_64-apple-darwin"),
    "client-glass-macos-aarch64": ("glass-macos-aarch64", "aarch64-apple-darwin"),
}


def fail(message: str) -> None:
    raise SystemExit(f"client compatibility evidence merge failed: {message}")


def main() -> None:
    if len(sys.argv) != 3:
        raise SystemExit(
            "usage: merge-client-compatibility-evidence.py OUTPUT.json EVIDENCE_ROOT"
        )
    output = pathlib.Path(sys.argv[1])
    root = pathlib.Path(sys.argv[2])
    revision = subprocess.check_output(["git", "rev-parse", "HEAD"], text=True).strip()
    rows = []
    for directory, (artifact, target) in TARGETS.items():
        path = root / directory / f"client-compatibility-{artifact}.json"
        if not path.is_file():
            fail(f"missing {path}")
        try:
            row = json.loads(path.read_text(encoding="utf-8"))
        except (OSError, json.JSONDecodeError) as error:
            fail(f"{path}: {error}")
        if row.get("schema_version") != 1 or row.get("type") != "client_compatibility_evidence":
            fail(f"{path} has an invalid schema or type")
        if row.get("source_revision") != revision or row.get("target") != target:
            fail(f"{path} is not bound to this target and source revision")
        if row.get("runtime_certification") != "certified":
            fail(f"{path} is not certified")
        artifact_metadata = row.get("artifact")
        if not isinstance(artifact_metadata, dict):
            fail(f"{path} is missing artifact metadata")
        if artifact_metadata.get("name") != artifact:
            fail(f"{path} names the wrong artifact")
        artifact_hash = artifact_metadata.get("sha256", "")
        if len(artifact_hash) != 64 or any(
            character not in "0123456789abcdef" for character in artifact_hash
        ):
            fail(f"{path} has an invalid artifact SHA-256")
        if artifact_metadata.get("size_bytes", 0) <= 0:
            fail(f"{path} has an invalid artifact size")
        clients = row.get("clients")
        if not isinstance(clients, dict) or set(clients) != {"typescript", "python", "npm_launcher"}:
            fail(f"{path} has an incomplete client inventory")
        if any(client.get("status") != "passed" for client in clients.values()):
            fail(f"{path} contains a non-passing client")
        rows.append(row)

    report = {
        "schema_version": 1,
        "type": "client_compatibility_matrix",
        "source_revision": revision,
        "target_count": len(rows),
        "targets": rows,
        "runtime_certification": "certified",
    }
    output.parent.mkdir(parents=True, exist_ok=True)
    output.write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
    print(f"client compatibility matrix merged: {len(rows)} exact artifacts")


if __name__ == "__main__":
    main()
