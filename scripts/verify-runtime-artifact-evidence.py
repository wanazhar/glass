#!/usr/bin/env python3
"""Verify that runtime certification reports join to contract artifacts exactly."""

import json
import pathlib
import sys


REQUIRED_ARTIFACTS = {
    "glass-linux-x86_64",
    "glass-linux-arm64",
    "glass-macos-x86_64",
    "glass-macos-aarch64",
}


def fail(message: str) -> None:
    raise SystemExit(f"runtime artifact evidence verification failed: {message}")


def load(path: pathlib.Path) -> dict:
    try:
        value = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as error:
        fail(f"{path}: {error}")
    if not isinstance(value, dict):
        fail(f"{path} is not an object")
    return value


def artifact_from(row: dict, report_type: str, path: pathlib.Path) -> dict:
    if report_type == "knowledge_migration_matrix":
        binding = row.get("artifact_binding")
        if not isinstance(binding, dict):
            fail(f"{path} is missing a migration artifact binding")
        return binding
    artifact = row.get("artifact")
    if not isinstance(artifact, dict):
        fail(f"{path} is missing a runtime artifact binding")
    return artifact


def main() -> None:
    if len(sys.argv) != 5:
        raise SystemExit(
            "usage: verify-runtime-artifact-evidence.py "
            "CONTRACT.json RELIABILITY.json SANDBOX.json MIGRATION.json"
        )
    contract_path, reliability_path, sandbox_path, migration_path = map(
        pathlib.Path, sys.argv[1:]
    )
    contract = load(contract_path)
    if contract.get("type") != "cross_artifact_contract_matrix":
        fail("contract input has the wrong type")
    contracts = {
        row.get("name"): row
        for row in contract.get("artifacts", [])
        if isinstance(row, dict)
    }
    if set(contracts) != REQUIRED_ARTIFACTS:
        fail("contract input does not contain exactly the four release artifacts")

    runtime_reports = [
        (reliability_path, "reliability_runtime_scorecard"),
        (sandbox_path, "native_extension_sandbox_matrix"),
        (migration_path, "knowledge_migration_matrix"),
    ]
    source_revision = contract.get("git_revision")
    for path, expected_type in runtime_reports:
        report = load(path)
        if report.get("type") != expected_type:
            fail(f"{path} has the wrong type")
        if report.get("source_revision") != source_revision:
            fail(f"{path} does not match the contract source revision")
        if report.get("runtime_certification") != "certified":
            fail(f"{path} is not certified")
        rows = report.get("targets")
        if not isinstance(rows, list) or len(rows) != len(REQUIRED_ARTIFACTS):
            fail(f"{path} does not contain four target rows")
        seen = set()
        for row in rows:
            if not isinstance(row, dict):
                fail(f"{path} contains a non-object target row")
            artifact = artifact_from(row, expected_type, path)
            name = artifact.get("name")
            if name in seen or name not in REQUIRED_ARTIFACTS:
                fail(f"{path} contains an unsupported or duplicate artifact: {name!r}")
            seen.add(name)
            contract_artifact = contracts[name]
            for field in ("sha256", "size_bytes"):
                if artifact.get(field) != contract_artifact.get(field):
                    fail(f"{path} disagrees with contract artifact {name} on {field}")
        if seen != REQUIRED_ARTIFACTS:
            fail(f"{path} is missing one or more release artifacts")

    print("runtime certification reports join to all four exact contract artifacts")


if __name__ == "__main__":
    main()
