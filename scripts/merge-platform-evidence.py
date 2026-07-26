#!/usr/bin/env python3
"""Merge one platform evidence report from each supported release runner."""

import json
import pathlib
import subprocess
import sys


REQUIRED_TARGETS = {
    "x86_64-unknown-linux-gnu",
    "x86_64-apple-darwin",
    "aarch64-apple-darwin",
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
        rows.extend(report.get("platforms", []))

    targets = [row.get("target") for row in rows]
    if set(targets) != REQUIRED_TARGETS or len(targets) != len(REQUIRED_TARGETS):
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
