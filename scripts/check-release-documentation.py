#!/usr/bin/env python3
"""Validate release truth markers in the current user-facing documentation."""

import pathlib
import sys


ROOT = pathlib.Path(__file__).resolve().parent.parent
REQUIRED_MARKERS = {
    "README.md": ["| 0.2.6 | Current published release |", "docs/feature-parity.md", "docs/release-evidence.md"],
    "CHANGELOG.md": ["## [0.2.6] - 2026-08-01", "## [Unreleased] — 0.2.7"],
    "docs/release-checklist.md": [
        "current published release is `glass-browser` version `0.2.6`",
        "next crates.io release, `0.2.7`",
        "GitHub release binaries, checksum manifests",
    ],
    "docs/feature-parity.md": [
        "published 0.2.6 baseline",
        "0.2.7 work stream",
        "feature parity matrix](feature-parity.json)",
    ],
    "docs/release-evidence.md": [
        "`feature-parity.json`",
        "cargo publish --locked --dry-run",
        "no native binary assets are expected",
    ],
    "docs/plan/analysis/release-audit-028.md": [
        "`0.2.0` publication boundary has been crossed",
        "`0.2.6 published; source-only GitHub Release",
        "`0.2.7 local development; not ready for",
    ],
}
FORBIDDEN_MARKERS = (
    "0.2.0 is unpublished",
    "0.2.0 is not published",
    "do not publish 0.2.0",
    "0.2.0 local release candidate",
)

PUBLIC_FACING_DOCS = (
    "README.md",
    "CHANGELOG.md",
    "docs/INDEX.md",
    "docs/ci-platform-certification.md",
    "docs/experimental-capabilities.md",
    "docs/extensions.md",
    "docs/feature-parity.md",
    "docs/installation.md",
    "docs/release-checklist.md",
    "docs/release-evidence.md",
)
FORBIDDEN_PUBLIC_MARKERS = (
    "this machine",
    "current machine",
    "locally verified",
    "verified locally",
    "current development machine",
    "on this host",
    "current host",
    "machine only",
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
    for relative in PUBLIC_FACING_DOCS:
        path = ROOT / relative
        try:
            lowered = path.read_text(encoding="utf-8").lower()
        except OSError as error:
            fail(f"cannot read {relative}: {error}")
        for marker in FORBIDDEN_PUBLIC_MARKERS:
            if marker in lowered:
                failures.append(
                    f"{relative} contains machine-scoped public wording {marker!r}"
                )
    if failures:
        fail("; ".join(failures))
    print(f"release documentation truth validated: {len(REQUIRED_MARKERS)} documents")


if __name__ == "__main__":
    main()
