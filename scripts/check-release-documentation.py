#!/usr/bin/env python3
"""Validate release truth markers in the current user-facing documentation."""

import pathlib
import sys


ROOT = pathlib.Path(__file__).resolve().parent.parent
REQUIRED_MARKERS = {
    "README.md": ["| 0.2.1 | Current release |", "docs/feature-parity.md", "docs/release-evidence.md"],
    "CHANGELOG.md": ["## [0.2.1] - 2026-07-30", "## [Unreleased] — 0.2.2"],
    "docs/release-checklist.md": [
        "current published release is `glass-browser` version `0.2.1`",
        "next crates.io release, `0.2.2`",
        "GitHub release binaries, checksum manifests, Sigstore bundles",
    ],
    "docs/feature-parity.md": [
        "published 0.2.1 baseline",
        "0.2.2 work stream",
        "feature parity matrix](feature-parity.json)",
    ],
    "docs/release-evidence.md": [
        "`feature-parity.json`",
        "cargo publish --locked --dry-run",
        "No GitHub binary release is expected",
    ],
    "docs/plan/analysis/release-audit-028.md": [
        "`0.2.0` publication boundary has been crossed",
        "`0.2.1 published; crates-only local",
        "`0.2.2 local development; not ready for public release",
    ],
}
FORBIDDEN_MARKERS = (
    "0.2.0 is unpublished",
    "0.2.0 is not published",
    "do not publish 0.2.0",
    "0.2.0 local release candidate",
)


def fail(message: str) -> None:
    raise SystemExit(f"release documentation check failed: {message}")


def main() -> None:
    failures = []
    for relative, markers in REQUIRED_MARKERS.items():
        path = ROOT / relative
        try:
            text = path.read_text(encoding="utf-8")
        except OSError as error:
            fail(f"cannot read {relative}: {error}")
        for marker in markers:
            if marker not in text:
                failures.append(f"{relative} is missing {marker!r}")
        lowered = text.lower()
        for marker in FORBIDDEN_MARKERS:
            if marker in lowered:
                failures.append(f"{relative} contains stale release claim {marker!r}")
    if failures:
        fail("; ".join(failures))
    print(f"release documentation truth validated: {len(REQUIRED_MARKERS)} documents")


if __name__ == "__main__":
    main()
