#!/usr/bin/env python3
"""Validate release truth markers in the current user-facing documentation."""

import json
import pathlib
import re
import subprocess


ROOT = pathlib.Path(__file__).resolve().parent.parent
REQUIRED_MARKERS = {
    "README.md": [
        "| 0.3.1 | Current source release |",
        "docs/feature-parity.md",
        "docs/release-evidence.md",
    ],
    "CHANGELOG.md": [
        "## [0.3.1] - 2026-08-07",
        "## [Unreleased]",
    ],
    "docs/plan/README.md": [
        "[ir-030-081](tasks/ir-030-081.md)",
        "[ir-030-089](tasks/ir-030-089.md)",
        "`glass-browser 0.3.0` is published on crates.io",
        "release delivery record are complete",
    ],
    "docs/release-checklist.md": [
        "release checkout is `glass-browser` version `0.3.1`",
        "## 0.3.1 pre-publication checklist",
        "GitHub release binaries, checksum manifests",
    ],
    "docs/feature-parity.md": [
        "0.3.0 release baseline",
        "runtime verification",
        "feature parity matrix](feature-parity.json)",
    ],
    "docs/release-evidence.md": [
        "## 0.3.1 pre-publication evidence",
        "final stable `0.3.1` package metadata",
        "`feature-parity.json`",
        "cargo publish --locked --dry-run",
        "`glass-browser 0.3.0` to crates.io",
        "85 advertised MCP tools",
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
FORBIDDEN_PRERELEASE_PATTERNS = (
    re.compile(
        r"\b0\.3\.1-(?:alpha|beta|rc)[.0-9-]*\b",
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
    try:
        metadata = json.loads(
            subprocess.check_output(
                ["cargo", "metadata", "--no-deps", "--locked", "--format-version", "1"],
                cwd=ROOT,
                text=True,
            )
        )
        package_version = next(
            package["version"]
            for package in metadata["packages"]
            if package["name"] == "glass-browser"
        )
    except (OSError, subprocess.CalledProcessError, KeyError, json.JSONDecodeError, StopIteration) as error:
        fail(f"cannot read package version: {error}")
    if package_version != "0.3.1":
        fail(f"release checkout must use stable package version 0.3.1, not {package_version}")
    marker_sets = REQUIRED_MARKERS
    failures = []
    for relative, markers in marker_sets.items():
        path = ROOT / relative
        try:
            text = path.read_text(encoding="utf-8")
        except OSError as error:
            fail(f"cannot read {relative}: {error}")
        for marker in markers:
            if marker not in text:
                failures.append(f"{relative} is missing {marker!r}")
        lowered = text.lower()
        for pattern in FORBIDDEN_PRERELEASE_PATTERNS:
            if pattern.search(lowered):
                failures.append(
                    f"{relative} contains stale 0.3.1 prerelease metadata"
                )
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
        for pattern in FORBIDDEN_PRERELEASE_PATTERNS:
            if pattern.search(lowered):
                failures.append(
                    f"{relative} contains stale 0.3.1 prerelease metadata"
                )
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
    print(f"release documentation truth validated: {len(marker_sets)} documents")


if __name__ == "__main__":
    main()
