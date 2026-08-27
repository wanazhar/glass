#!/usr/bin/env python3
"""Validate release truth markers in the current user-facing documentation."""

import json
import pathlib
import re
import subprocess


ROOT = pathlib.Path(__file__).resolve().parent.parent
REQUIRED_MARKERS = {
    "README.md": [
        "| `glass-browser 0.3.13`, `glass-dev 0.3.13` | Current release source; public registry state is recorded in release evidence |",
        "docs/feature-parity.md",
        "docs/release-evidence.md",
    ],
    "docs/INDEX.md": [
        "[Glass 0.3.13 release notes](releases/0.3.13.md)",
        "[Migrate from 0.3.12 to 0.3.13](migration/0.3.13.md)",
        "[Glass 0.3.12 release notes](releases/0.3.12.md)",
        "[Migrate from 0.3.11 to 0.3.12](migration/0.3.12.md)",
    ],
    "CHANGELOG.md": [
        "## [0.3.3] - 2026-08-10",
        "## [0.3.4] - 2026-08-10",
        "## [0.3.2] - 2026-08-08",
        "## [Unreleased]",
    ],
    "docs/releases/0.3.12.md": [
        "## Major features",
        "## Breaking changes",
        "## Security",
        "## Installation and migration",
        "## Known limitations",
        "## Validation evidence",
    ],
    "docs/releases/0.3.13.md": [
        "## Major features",
        "## Breaking changes",
        "## Security",
        "## Installation and migration",
        "## Known limitations",
        "## Validation evidence",
    ],
    "docs/plan/README.md": [
        "[ir-030-081](tasks/ir-030-081.md)",
        "[ir-030-089](tasks/ir-030-089.md)",
        "`glass-browser 0.3.0` is published on crates.io",
        "release delivery record are complete",
    ],
    "docs/release-checklist.md": [
        "release checkout is `glass-browser` and `glass-dev` version `0.3.13`",
        "## 0.3.2 release record",
        "GitHub release binaries, checksum manifests",
    ],
    "docs/installation.md": [
        "## Fully uninstall Glass",
        "cargo uninstall glass-dev",
        "cargo uninstall glass-browser",
        "`$GLASS_CONFIG_HOME/glass`",
    ],
    "docs/feature-parity.md": [
        "0.3.0 release baseline",
        "runtime verification",
        "feature parity matrix](feature-parity.json)",
    ],
    "docs/release-evidence.md": [
        "## 0.3.2 publication evidence",
        "## 0.3.3 release evidence",
        "## 0.3.4 release evidence",
        "## 0.3.12 release evidence",
        "## 0.3.13 release evidence",
        "Release workflow run 31254928934",
        "GitHub Release v0.3.2",
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
FORBIDDEN_ARCHITECTURE_PHRASES = (
    "pi rpc",
    "resident pi adapter",
    "optional pi adapter",
    "real pi rpc",
    "0.3.4 source line",
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
    if package_version != "0.3.13":
        fail(f"release checkout must use package version 0.3.13, not {package_version}")
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
    architecture_docs = [ROOT / "README.md", ROOT / "crates/glass-dev/README.md"]
    architecture_docs.extend(
        path
        for path in (ROOT / "docs").rglob("*.md")
        if "plan" not in path.relative_to(ROOT).parts
    )
    for path in architecture_docs:
        relative = path.relative_to(ROOT)
        try:
            lowered = path.read_text(encoding="utf-8").lower()
        except OSError as error:
            fail(f"cannot read {relative}: {error}")
        for phrase in FORBIDDEN_ARCHITECTURE_PHRASES:
            if phrase in lowered:
                failures.append(
                    f"{relative} contains retired architecture phrase {phrase!r}"
                )
    if failures:
        fail("; ".join(failures))
    print(f"release documentation truth validated: {len(marker_sets)} documents")


if __name__ == "__main__":
    main()
