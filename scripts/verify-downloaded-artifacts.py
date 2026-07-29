#!/usr/bin/env python3
"""Verify downloaded release artifacts against their collected evidence."""

import hashlib
import json
import pathlib
import subprocess
import sys


REQUIRED_TARGETS = {
    "x86_64-unknown-linux-gnu",
    "aarch64-unknown-linux-gnu",
    "x86_64-apple-darwin",
    "aarch64-apple-darwin",
}


def command(*args: str) -> str:
    return subprocess.check_output(args, text=True).strip()


def fail(message: str) -> "NoReturn":
    raise SystemExit(f"downloaded artifact verification failed: {message}")


def main() -> None:
    if len(sys.argv) != 3:
        raise SystemExit("usage: verify-downloaded-artifacts.py CONTRACT.json ARTIFACT_DIR")

    report_path = pathlib.Path(sys.argv[1])
    artifact_dir = pathlib.Path(sys.argv[2])
    report = json.loads(report_path.read_text(encoding="utf-8"))
    revision = command("git", "rev-parse", "HEAD")
    if report.get("schema_version") != 1 or report.get("type") != "cross_artifact_contract_matrix":
        fail("contract matrix has an invalid schema or type")
    if report.get("git_revision") != revision:
        fail("contract matrix does not match the checked-out revision")

    artifacts = report.get("artifacts", [])
    if {artifact.get("target") for artifact in artifacts} != REQUIRED_TARGETS:
        fail("contract matrix does not contain exactly the four supported targets")

    for artifact in artifacts:
        name = artifact.get("name", "")
        if not name or pathlib.PurePath(name).name != name:
            fail(f"artifact has an unsafe or missing name: {name!r}")
        path = artifact_dir / name
        if not path.is_file():
            fail(f"downloaded artifact is missing: {path}")
        contents = path.read_bytes()
        actual_hash = hashlib.sha256(contents).hexdigest()
        if actual_hash != artifact.get("sha256"):
            fail(f"artifact hash differs from contract evidence: {name}")
        if len(contents) != artifact.get("size_bytes"):
            fail(f"artifact size differs from contract evidence: {name}")

    print(f"downloaded artifacts verified against contract evidence: {len(artifacts)} targets")


if __name__ == "__main__":
    main()
