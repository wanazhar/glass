#!/usr/bin/env python3
"""Validate release truth markers in the current user-facing documentation."""

import pathlib
import re
import sys


ROOT = pathlib.Path(__file__).resolve().parent.parent
REQUIRED_MARKERS = {
    "README.md": [
        "| 0.2.8 | Release candidate; publication pending |",
        "docs/feature-parity.md",
        "docs/release-evidence.md",
    ],
    "CHANGELOG.md": ["## [0.2.8] - 2026-08-01", "## [Unreleased] — 0.2.9"],
    "docs/plan/README.md": [
        "[`ir-030-081`](tasks/ir-030-081.md)",
        "[`ir-030-089`](tasks/ir-030-089.md)",
        "0.2.8` changes",
        "remain unreleased.",
    ],
    "docs/release-checklist.md": [
        "release candidate is `glass-browser` version `0.2.8`",
        "The next development version is `0.2.9`",
        "GitHub release binaries, checksum manifests",
    ],
    "docs/feature-parity.md": [
        "published 0.2.7 baseline",
        "0.2.8 work stream",
        "feature parity matrix](feature-parity.json)",
    ],
    "docs/release-evidence.md": [
        "`feature-parity.json`",
        "cargo publish --locked --dry-run",
        "no native binary assets are expected",
    ],
    "docs/plan/analysis/release-audit-028.md": [
        "`0.2.0` publication boundary has been crossed",
        "`0.2.7 published; source-only GitHub Release",
        "`0.2.8 local development; not ready for",
    ],
}
FORBIDDEN_MARKERS = (
    "0.2.0 is unpublished",
    "0.2.0 is not published",
    "do not publish 0.2.0",
    "0.2.0 local release candidate",
)
FORBIDDEN_CURRENT_RELEASE_PATTERNS = (
    re.compile(r"\b0\.2\.8\s+is\s+published\b", re.IGNORECASE),
    re.compile(
        r"\b0\.2\.8\s+is\s+(?:the\s+)?(?:current|latest)\s+"
        r"(?:published\s+)?(?:release|version)\b",
        re.IGNORECASE,
    ),
    re.compile(
        r"\b(?:the\s+)?(?:current|latest)\s+"
        r"(?:published\s+)?(?:release|version)\s+is\s+"
        r"(?:`?glass-browser`?\s+version\s+)?`?0\.2\.8`?\b",
        re.IGNORECASE,
    ),
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
        if relative not in PUBLIC_FACING_DOCS:
            for pattern in FORBIDDEN_CURRENT_RELEASE_PATTERNS:
                if pattern.search(lowered):
                    failures.append(
                        f"{relative} contains current-release claim for 0.2.8"
                    )
    for relative in PUBLIC_FACING_DOCS:
        path = ROOT / relative
        try:
            lowered = path.read_text(encoding="utf-8").lower()
        except OSError as error:
            fail(f"cannot read {relative}: {error}")
        for pattern in FORBIDDEN_CURRENT_RELEASE_PATTERNS:
            if pattern.search(lowered):
                failures.append(
                    f"{relative} contains current-release claim for 0.2.8"
                )
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
