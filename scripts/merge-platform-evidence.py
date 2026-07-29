#!/usr/bin/env python3
"""Merge one platform evidence report from each supported release runner."""

import json
import pathlib
import subprocess
import sys


REQUIRED_TARGETS = {
    "x86_64-unknown-linux-gnu": ("linux", "x86_64"),
    "aarch64-unknown-linux-gnu": ("linux", "aarch64"),
    "x86_64-apple-darwin": ("macos", "x86_64"),
    "aarch64-apple-darwin": ("macos", "aarch64"),
}


def command(*args: str) -> str:
    return subprocess.check_output(args, text=True).strip()


def main() -> None:
    if len(sys.argv) < 3:
        raise SystemExit("usage: merge-platform-evidence.py OUTPUT.json INPUT.json ...")

    reports = [json.loads(pathlib.Path(path).read_text(encoding="utf-8")) for path in sys.argv[2:]]
    revision = command("git", "rev-parse", "HEAD")
    rows = []
    producer = None
    for report in reports:
        if report.get("schema_version") != 1 or report.get("type") != "real_browser_platform_matrix":
            raise SystemExit("platform evidence has an invalid schema or type")
        if report.get("git_revision") != revision:
            raise SystemExit("platform evidence does not match the checked-out revision")
        if producer is None:
            producer = report.get("producer")
        if report.get("producer") != producer:
            raise SystemExit("platform evidence has inconsistent producers")
        platform_rows = report.get("platforms", [])
        if len(platform_rows) != 1:
            raise SystemExit("each platform evidence file must contain exactly one row")
        row = platform_rows[0]
        target = row.get("target")
        if target not in REQUIRED_TARGETS:
            raise SystemExit(f"platform evidence has an unsupported target: {target!r}")
        expected_os, expected_architecture = REQUIRED_TARGETS[target]
        if row.get("os") != expected_os or row.get("architecture") != expected_architecture:
            raise SystemExit(f"platform evidence has inconsistent target metadata: {target}")
        if row.get("status") != "passed":
            raise SystemExit(f"platform evidence did not pass: {target}")
        if not row.get("browser_version"):
            raise SystemExit(f"platform evidence is missing a browser version: {target}")
        artifact = row.get("artifact", {})
        artifact_hash = artifact.get("sha256", "")
        if len(artifact_hash) != 64 or any(character not in "0123456789abcdef" for character in artifact_hash):
            raise SystemExit(f"platform evidence has an invalid artifact SHA-256: {target}")
        if artifact.get("size_bytes", 0) <= 0 or not artifact.get("name"):
            raise SystemExit(f"platform evidence has incomplete artifact metadata: {target}")
        runner = row.get("runner", {})
        for field in ("os", "architecture", "image", "image_version"):
            if not runner.get(field):
                raise SystemExit(f"platform evidence is missing runner.{field}: {target}")
        if not row.get("smoke_command") or not row.get("raw_report"):
            raise SystemExit(f"platform evidence is missing command or raw report: {target}")
        rows.append(row)

    targets = [row.get("target") for row in rows]
    if set(targets) != set(REQUIRED_TARGETS) or len(targets) != len(REQUIRED_TARGETS):
        raise SystemExit(f"platform evidence is incomplete or duplicated: {targets!r}")

    merged = {
        "schema_version": 1,
        "git_revision": revision,
        "producer": producer,
        "type": "real_browser_platform_matrix",
        "platforms": sorted(rows, key=lambda row: row["target"]),
    }
    output = pathlib.Path(sys.argv[1])
    output.parent.mkdir(parents=True, exist_ok=True)
    output.write_text(json.dumps(merged, indent=2) + "\n", encoding="utf-8")


if __name__ == "__main__":
    main()
