#!/usr/bin/env python3
"""Verify that browser and contract evidence describe the same artifacts."""

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
    raise SystemExit(f"artifact evidence verification failed: {message}")


def main() -> None:
    if len(sys.argv) != 3:
        raise SystemExit("usage: verify-artifact-evidence.py PLATFORM.json CONTRACT.json")

    platform_report = json.loads(pathlib.Path(sys.argv[1]).read_text(encoding="utf-8"))
    contract_report = json.loads(pathlib.Path(sys.argv[2]).read_text(encoding="utf-8"))
    revision = command("git", "rev-parse", "HEAD")
    for report, expected_type in (
        (platform_report, "real_browser_platform_matrix"),
        (contract_report, "cross_artifact_contract_matrix"),
    ):
        if report.get("schema_version") != 1 or report.get("type") != expected_type:
            fail(f"invalid {expected_type} report")
        if report.get("git_revision") != revision:
            fail(f"{expected_type} report does not match the checked-out revision")

    platform_rows = {row.get("target"): row for row in platform_report.get("platforms", [])}
    contract_rows = {row.get("target"): row for row in contract_report.get("artifacts", [])}
    if set(platform_rows) != REQUIRED_TARGETS or set(contract_rows) != REQUIRED_TARGETS:
        fail("platform and contract reports do not contain exactly the four supported targets")

    for target in sorted(REQUIRED_TARGETS):
        platform_artifact = platform_rows[target].get("artifact", {})
        contract_artifact = contract_rows[target]
        for field in ("name", "sha256", "size_bytes"):
            if platform_artifact.get(field) != contract_artifact.get(field):
                fail(f"{target} has mismatched artifact {field}")

    print(f"artifact evidence verified: {len(REQUIRED_TARGETS)} targets share exact artifact identity")


if __name__ == "__main__":
    main()
