#!/usr/bin/env python3
"""Validate the checked-in release truth and feature parity inventory."""

import json
import pathlib
import subprocess


ROOT = pathlib.Path(__file__).resolve().parent.parent
MATRIX_PATH = ROOT / "docs/feature-parity.json"

EXPECTED_TARGETS = {
    "linux-x86_64": {
        "rust_target": "x86_64-unknown-linux-gnu",
        "os": "linux",
        "architecture": "x86_64",
    },
    "linux-arm64": {
        "rust_target": "aarch64-unknown-linux-gnu",
        "os": "linux",
        "architecture": "aarch64",
    },
    "macos-x86_64": {
        "rust_target": "x86_64-apple-darwin",
        "os": "macos",
        "architecture": "x86_64",
    },
    "macos-arm64": {
        "rust_target": "aarch64-apple-darwin",
        "os": "macos",
        "architecture": "aarch64",
    },
}
EXPECTED_CAPABILITIES = {
    "actions-and-verification",
    "semantic-observations-and-diffs",
    "intent-resolution",
    "workflow-runtime-and-resume",
    "persistent-knowledge",
    "workflow-authoring",
    "reliability-laboratory",
    "local-daemon",
    "rust-library",
    "mcp-stdio",
    "typescript-client",
    "python-client",
    "tui",
    "native-extensions",
}
ALLOWED_STATUSES = {
    "certified",
    "shippedUncertified",
    "experimental",
    "disabledByPolicy",
    "blockedBySecurityGate",
    "unsupported",
}


def fail(message: str) -> "NoReturn":
    raise SystemExit(f"feature parity validation failed: {message}")


def main() -> None:
    try:
        matrix = json.loads(MATRIX_PATH.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as error:
        fail(f"cannot read {MATRIX_PATH.relative_to(ROOT)}: {error}")

    if matrix.get("schema_version") != 1:
        fail("schema_version must be 1")
    if matrix.get("type") != "glass_feature_parity":
        fail("type must be glass_feature_parity")

    baseline = matrix.get("release_baseline", {})
    if baseline.get("version") != "0.3.0":
        fail("release baseline must be 0.3.0")
    if baseline.get("tag") != "v0.3.0":
        fail("release baseline tag must be v0.3.0")
    source_commit = baseline.get("source_commit", "")
    if len(source_commit) != 40 or any(character not in "0123456789abcdef" for character in source_commit):
        fail("release baseline source_commit must be a full lowercase commit SHA")
    if matrix.get("next_release") != "0.3.4":
        fail("next_release must be 0.3.4")

    targets = matrix.get("targets")
    if not isinstance(targets, list) or {target.get("id") for target in targets} != set(EXPECTED_TARGETS):
        fail("targets must contain exactly the four supported target IDs")
    if len(targets) != len(EXPECTED_TARGETS):
        fail("targets must not contain duplicates")
    for target in targets:
        target_id = target["id"]
        expected = EXPECTED_TARGETS[target_id]
        if target.get("rust_target") != expected["rust_target"]:
            fail(f"{target_id} has an unexpected Rust target")
        if target.get("os") != expected["os"] or target.get("architecture") != expected["architecture"]:
            fail(f"{target_id} has inconsistent OS or architecture metadata")

    capabilities = matrix.get("capabilities")
    if not isinstance(capabilities, list):
        fail("capabilities must be an array")
    capability_ids = [capability.get("id") for capability in capabilities]
    if set(capability_ids) != EXPECTED_CAPABILITIES:
        fail("capabilities do not match the supported feature inventory")
    if len(capability_ids) != len(set(capability_ids)):
        fail("capabilities must not contain duplicate IDs")

    target_ids = set(EXPECTED_TARGETS)
    for capability in capabilities:
        if capability.get("implementation") != "implemented":
            fail(f"{capability.get('id')} is not marked implemented")
        statuses = capability.get("target_status")
        if not isinstance(statuses, dict) or set(statuses) != target_ids:
            fail(f"{capability.get('id')} must declare one status for every target")
        if any(status not in ALLOWED_STATUSES for status in statuses.values()):
            fail(f"{capability.get('id')} contains an unknown target status")
        if not capability.get("reason"):
            fail(f"{capability.get('id')} must explain its current status")
        if capability.get("platform_dependency") in {"none", "browser"} and len(set(statuses.values())) != 1:
            fail(f"{capability.get('id')} has an unexplained cross-platform status difference")

    extensions = next(capability for capability in capabilities if capability["id"] == "native-extensions")
    extension_status = extensions["target_status"]
    if extension_status.get("linux-arm64") != "experimental" or any(
        extension_status.get(target) != "blockedBySecurityGate"
        for target in ("linux-x86_64", "macos-x86_64", "macos-arm64")
    ):
        fail("native extensions must be experimental only on verified Linux ARM64")

    metadata = json.loads(
        subprocess.check_output(
            ["cargo", "metadata", "--no-deps", "--locked", "--format-version", "1"],
            cwd=ROOT,
            text=True,
        )
    )
    cargo_version = next(
        package["version"]
        for package in metadata["packages"]
        if package["name"] == "glass-browser"
    )
    next_release = matrix["next_release"]
    prerelease_candidate = f"{next_release}-rc.1"
    if cargo_version not in {baseline["version"], next_release, prerelease_candidate}:
        fail(
            "Cargo.toml version is neither the published baseline, the next "
            "release, nor the accepted local prerelease candidate"
        )

    required_text = {
        "README.md": "| `glass-browser 0.3.4`, `glass-dev 0.3.4` | Local release candidate; publication requires the signed-tag workflow |",
        "CHANGELOG.md": "## [0.3.0] - 2026-08-06",
        "docs/release-checklist.md": "release checkout is `glass-browser` and `glass-dev` version `0.3.4`",
        "docs/plan/analysis/release-audit-028.md": "`0.2.7 published; source-only GitHub Release",
    }
    for relative_path, expected in required_text.items():
        text = (ROOT / relative_path).read_text(encoding="utf-8")
        if expected not in text:
            fail(f"{relative_path} is missing release truth marker {expected!r}")

    release_label = (
        f"local candidate {prerelease_candidate}"
        if cargo_version == prerelease_candidate
        else cargo_version
    )
    print(
        f"feature parity validated: {len(capabilities)} capabilities across "
        f"{len(targets)} targets; baseline {baseline['version']}; "
        f"next {next_release}; checkout {release_label}"
    )


if __name__ == "__main__":
    main()
