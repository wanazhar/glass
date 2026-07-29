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
        source_revisions.add(row.get("source_revision"))
        rows.append(row)
    if len(source_revisions) != 1:
        fail("sandbox rows are not bound to one source revision")
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
