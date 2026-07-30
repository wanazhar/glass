#!/usr/bin/env python3
"""Merge the four native extension sandbox evidence rows."""

import json
import pathlib
import subprocess
import sys


TARGETS = {
    "sandbox-glass-linux-x86_64": ("glass-linux-x86_64", "linux-x86-64", "linux-bubblewrap"),
    "sandbox-glass-linux-arm64": ("glass-linux-arm64", "linux-arm64", "linux-bubblewrap"),
    "sandbox-glass-macos-x86_64": ("glass-macos-x86_64", "macos-x86-64", "macos-sandbox-exec"),
    "sandbox-glass-macos-aarch64": ("glass-macos-aarch64", "macos-arm64", "macos-sandbox-exec"),
}


def fail(message: str) -> None:
    raise SystemExit(f"extension sandbox evidence merge failed: {message}")


def main() -> None:
    if len(sys.argv) != 3:
        raise SystemExit("usage: merge-extension-sandbox-evidence.py OUTPUT.json EVIDENCE_ROOT")
    output = pathlib.Path(sys.argv[1])
    root = pathlib.Path(sys.argv[2])
    rows = []
    source_revisions = set()
    for directory, (artifact, target, sandbox) in TARGETS.items():
        path = root / directory / f"sandbox-{artifact}.json"
        if not path.is_file():
            fail(f"missing {path}")
        try:
            row = json.loads(path.read_text(encoding="utf-8"))
        except (OSError, json.JSONDecodeError) as error:
            fail(f"{path}: {error}")
        if row.get("target") != target or row.get("sandbox") != sandbox:
            fail(f"{path} has the wrong target or sandbox")
        if row.get("status") != "passed":
            fail(f"{path} is not passed")
        artifact_metadata = row.get("artifact")
        if not isinstance(artifact_metadata, dict):
            fail(f"{path} is not bound to a packaged artifact")
        if artifact_metadata.get("name") != artifact or artifact_metadata.get("target") != target:
            fail(f"{path} has the wrong artifact binding")
        artifact_hash = artifact_metadata.get("sha256", "")
        if len(artifact_hash) != 64 or any(
            character not in "0123456789abcdef" for character in artifact_hash
        ):
            fail(f"{path} has an invalid artifact SHA-256")
        if artifact_metadata.get("size_bytes", 0) <= 0:
            fail(f"{path} has an invalid artifact size")
        source_revision = row.get("source_revision")
        if (
            not isinstance(source_revision, str)
            or len(source_revision) != 40
            or any(character not in "0123456789abcdef" for character in source_revision)
        ):
            fail(f"{path} is missing a valid source revision")
        source_revisions.add(source_revision)
        rows.append(row)
    if len(source_revisions) != 1:
        fail("sandbox rows are not bound to one source revision")
    artifact_bindings = {json.dumps(row["artifact"], sort_keys=True) for row in rows}
    if len(artifact_bindings) != len(rows):
        fail("sandbox rows unexpectedly share one artifact binding")
    report = {
        "schema_version": 1,
        "type": "native_extension_sandbox_matrix",
        "source_revision": next(iter(source_revisions)),
        "target_count": len(rows),
        "targets": rows,
        "capability_status": "blockedBySecurityGate",
        "runtime_certification": "certified",
    }
    output.parent.mkdir(parents=True, exist_ok=True)
    output.write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
    print(f"native extension sandbox matrix merged: {len(rows)} targets")


if __name__ == "__main__":
    main()
