#!/usr/bin/env python3
"""Validate complete-product routing and substantive guide contracts."""

from __future__ import annotations

import pathlib
import subprocess


ROOT = pathlib.Path(__file__).resolve().parent.parent

DEPTH_CONTRACTS = {
    "README.md": (
        "## Choose the product", "## Five-minute development loop",
        "## Terminal workspace", "## Browser verification",
        "## Agents and collaboration", "## SSH, Mosh, Herdr, and iPhone",
        "## State, privacy, and ownership", "## Support and evidence",
    ),
    "docs/getting-started.md": (
        "## Path A: inspect and run a project", "## Path B: use the terminal workspace",
        "## Path C: observe and act in a browser",
        "## Path D: use Glass from an iPhone or remote shell",
        "## Path E: connect an MCP client", "## Path F: embed the Rust library",
        "## Common first-run failures", "## Close safely",
    ),
    "docs/installation.md": (
        "## Install from source", "## Fully uninstall Glass",
        "## Diagnose an installation", "## Select a browser",
        "## Attach to a browser", "## Use a safety policy", "## Deploy Glass",
    ),
    "docs/development-runtime.md": (
        "## Ownership and lifecycle", "## Project files and tree",
        "## Native editor and conflict rules", "## Processes and PTYs",
        "## Diagnostics and persistent LSP", "## Source/runtime graph and live evidence",
        "## Timeline, inbox, and replay", "## Agents and actor authority",
        "## Failure and recovery matrix",
    ),
    "docs/mobile-remote.md": (
        "## Start on an iPhone", "## Preserve work with Herdr",
        "## Recover the browser without leaving the TUI",
        "## Open the full application in Safari", "## Terminal compatibility",
    ),
    "docs/harness-architecture.md": (
        "## Runtime planes", "## Event and snapshot rules",
        "## Human interaction model", "## Verification",
    ),
    "docs/actions.md": (
        "## Choose a target", "## Guard an action", "## Failure kinds",
        "## Execution phases", "## Dispatch and idempotency", "## Troubleshooting",
    ),
    "docs/semantic-observation.md": (
        "## Observation levels", "## Observation lifecycle",
        "## Regions, records, and Web IR", "## Revisions and diffs",
        "## Failure and recovery", "## Privacy and limits",
    ),
    "docs/profile-ergonomics.md": (
        "## Create and use a profile", "## Stored browser state",
        "## Profile lifecycle", "## Attach mode", "## Failure and recovery",
    ),
    "docs/daemon.md": (
        "## Access control", "## Reuse one live MCP session", "## Status and recovery",
        "## Mutation leases", "## Project-session registry",
        "## Failure and shutdown matrix",
    ),
    "docs/production-canary.md": (
        "## Preconditions", "## Create one bounded manifest", "## Run the canary",
        "## Interpret and record", "## Claim boundary",
    ),
    "docs/reliability-metrics.md": (
        "## Metric definitions", "## Separate populations",
        "## Recovery and retry accounting", "## Report acceptance",
    ),
    "docs/cli.md": (
        "cli-inventory.json", "### Family contracts", "## Targets and revisions",
        "## Project development runtime", "## Output",
    ),
    "docs/mcp.md": (
        "## Configure a client", "## Negotiation", "## Transport limits",
        "## Cancellation", "## Errors and privacy", "## Session security",
    ),
    "docs/rust-sdk.md": (
        "## Session ownership", "## Structured observation and guarded actions",
        "## Task Protocol and deterministic compilation", "## Development Runtime",
        "## Public module map", "## Errors and privacy",
    ),
    "docs/policy.md": (
        "## Presets", "## Hardened policy", "## Capability decisions",
        "## Confirmation", "## Decision matrix",
    ),
    "docs/workflows.md": (
        "## Definition", "## Retry safety", "## Outputs and evidence",
        "## Checkpoints and resume", "## Authoring",
    ),
}

FORBIDDEN_CURRENT_TEXT = (
    "Phone mode exposes Home, Agent, App, Diff, and More",
    "Windows is unsupported.",
    "`glass-dev` contains only `glass`",
    "`agent` | `tool`, `tool-file`",
    "`daemon` | `start`, `status`, `stop`, `doctor`, `logs`, `acknowledge-recovery`, `serve`",
    "Glass ships 59 MCP tools",
    "**Total: 70 tools.**",
    "A bare\n`glass` invocation prints the concise start-here message",
    "Glass launches **headed** Chrome",
)


def tracked_markdown() -> list[pathlib.Path]:
    output = subprocess.check_output(["git", "ls-files", "*.md"], cwd=ROOT, text=True)
    return sorted(ROOT / value for value in output.splitlines())


def main() -> None:
    failures: list[str] = []
    for relative, markers in DEPTH_CONTRACTS.items():
        path = ROOT / relative
        if not path.is_file():
            failures.append(f"missing depth-contract document: {relative}")
            continue
        text = path.read_text(encoding="utf-8")
        for marker in markers:
            if marker not in text:
                failures.append(f"{relative} omits required depth marker: {marker}")

    index = (ROOT / "docs/INDEX.md").read_text(encoding="utf-8")
    audit = (ROOT / "docs/plan/analysis/documentation-depth-035.md").read_text(
        encoding="utf-8"
    )
    current_guides = [
        path for path in tracked_markdown()
        if path.is_relative_to(ROOT / "docs")
        and not path.is_relative_to(ROOT / "docs/plan")
        and path != ROOT / "docs/INDEX.md"
    ]
    for path in current_guides:
        relative = path.relative_to(ROOT / "docs").as_posix()
        if relative != "migration/issue-31.md" and f"({relative})" not in index:
            failures.append(f"docs/INDEX.md does not directly route current guide: {relative}")
        if path.name not in audit and relative not in audit:
            failures.append(f"depth audit does not account for current guide: {relative}")

    current_paths = [
        ROOT / "README.md", ROOT / "SECURITY.md", ROOT / "CONTRIBUTING.md",
        ROOT / "benchmarks/README.md", ROOT / "clients/python/README.md",
        ROOT / "clients/typescript/README.md", ROOT / "crates/glass-browser/README.md",
        ROOT / "crates/glass-dev/README.md", *current_guides,
    ]
    for path in current_paths:
        text = path.read_text(encoding="utf-8")
        for forbidden in FORBIDDEN_CURRENT_TEXT:
            if forbidden in text:
                failures.append(f"{path.relative_to(ROOT)} contains stale text: {forbidden}")

    if failures:
        detail = "\n".join(f"- {failure}" for failure in failures)
        raise SystemExit(f"documentation depth check failed:\n{detail}")

    print(
        "documentation depth validated: "
        f"{len(current_guides)} current guides routed/audited, "
        f"{len(DEPTH_CONTRACTS)} substantive contracts"
    )


if __name__ == "__main__":
    main()
