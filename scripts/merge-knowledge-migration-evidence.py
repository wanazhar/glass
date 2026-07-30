#!/usr/bin/env python3
"""Merge migration evidence produced by each packaged native artifact."""

import json
import pathlib
import subprocess
import sys


TARGETS = {
    "migration-glass-linux-x86_64": ("glass-linux-x86_64", "x86_64-unknown-linux-gnu"),
    "migration-glass-linux-arm64": ("glass-linux-arm64", "aarch64-unknown-linux-gnu"),
    "migration-glass-macos-x86_64": ("glass-macos-x86_64", "x86_64-apple-darwin"),
    "migration-glass-macos-aarch64": ("glass-macos-aarch64", "aarch64-apple-darwin"),
}


def fail(message: str) -> None:
    raise SystemExit(f"knowledge migration evidence merge failed: {message}")


def main() -> None:
    if len(sys.argv) != 3:
        raise SystemExit(
            "usage: merge-knowledge-migration-evidence.py OUTPUT.json EVIDENCE_ROOT"
        )
    output = pathlib.Path(sys.argv[1])
    root = pathlib.Path(sys.argv[2])
    revision = subprocess.check_output(["git", "rev-parse", "HEAD"], text=True).strip()
    rows = []
    for directory, (artifact, target) in TARGETS.items():
        path = root / directory / f"knowledge-migration-{artifact}.json"
        if not path.is_file():
            fail(f"missing {path}")
        try:
            row = json.loads(path.read_text(encoding="utf-8"))
        except (OSError, json.JSONDecodeError) as error:
            fail(f"{path}: {error}")
        if row.get("schema_version") != 1 or row.get("type") != "knowledge_migration_evidence":
            fail(f"{path} has an invalid schema or type")
        if row.get("source_revision") != revision:
            fail(f"{path} does not match the checked-out revision")
        if row.get("runtime_certification") != "certified":
            fail(f"{path} is not certified")
        binding = row.get("artifact_binding")
        if not isinstance(binding, dict):
            fail(f"{path} is missing artifact binding")
        if binding.get("kind") != "packaged_artifact":
            fail(f"{path} is not bound to a packaged artifact")
        if binding.get("name") != artifact or binding.get("target") != target:
            fail(f"{path} has the wrong artifact identity")
        artifact_hash = binding.get("sha256", "")
        if len(artifact_hash) != 64 or any(
            character not in "0123456789abcdef" for character in artifact_hash
        ):
            fail(f"{path} has an invalid artifact SHA-256")
        if binding.get("size_bytes", 0) <= 0:
            fail(f"{path} has an invalid artifact size")
        rows.append(row)

    report = {
        "schema_version": 1,
        "type": "knowledge_migration_matrix",
        "source_revision": revision,
        "target_count": len(rows),
        "targets": rows,
        "runtime_certification": "certified",
    }
    output.parent.mkdir(parents=True, exist_ok=True)
    output.write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
    print(f"knowledge migration matrix merged: {len(rows)} packaged targets")


if __name__ == "__main__":
    main()
