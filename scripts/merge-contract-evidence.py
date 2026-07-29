#!/usr/bin/env python3
"""Compare contract evidence from every supported native artifact."""

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
    raise SystemExit(f"contract comparison failed: {message}")


def canonical(value: object) -> str:
    return json.dumps(value, sort_keys=True, separators=(",", ":"))


def main() -> None:
    if len(sys.argv) < 3:
        raise SystemExit("usage: merge-contract-evidence.py OUTPUT.json INPUT.json ...")

    reports = [json.loads(pathlib.Path(path).read_text(encoding="utf-8")) for path in sys.argv[2:]]
    revision = command("git", "rev-parse", "HEAD")
    by_target = {}
    contract_reference = None
    producer_reference = None
    for report in reports:
        if report.get("schema_version") != 1 or report.get("type") != "artifact_contract":
            fail("an input has an invalid schema or type")
        if report.get("git_revision") != revision:
            fail("artifact contract evidence does not match the checked-out revision")
        target = report.get("target", {}).get("id")
        if target not in REQUIRED_TARGETS or target in by_target:
            fail(f"artifact contract has an unsupported or duplicate target: {target!r}")
        if not isinstance(report.get("producer"), dict):
            fail(f"{target} is missing producer metadata")
        artifact = report.get("artifact", {})
        artifact_hash = artifact.get("sha256", "")
        if len(artifact_hash) != 64 or any(character not in "0123456789abcdef" for character in artifact_hash):
            fail(f"{target} has an invalid artifact SHA-256")
        if artifact.get("size_bytes", 0) <= 0:
            fail(f"{target} has an invalid artifact size")
        if report.get("producer") != producer_reference and producer_reference is not None:
            fail("artifact contract evidence has inconsistent producers")
        producer_reference = report.get("producer")
        contract = report.get("contract")
        if not isinstance(contract, dict):
            fail(f"{target} is missing its contract")
        if contract_reference is None:
            contract_reference = contract
        elif canonical(contract) != canonical(contract_reference):
            fail(f"{target} differs from the reference CLI/MCP contract")
        by_target[target] = report

    if set(by_target) != REQUIRED_TARGETS:
        fail(f"artifact contract evidence is incomplete: {sorted(by_target)}")

    merged = {
        "schema_version": 1,
        "type": "cross_artifact_contract_matrix",
        "git_revision": revision,
        "producer": producer_reference,
        "comparison": {
            "status": "passed",
            "fields": ["cli_help", "capability_manifest", "mcp_tools"],
        },
        "contract": contract_reference,
        "artifacts": [
            {
                "target": target,
                "name": by_target[target]["artifact"]["name"],
                "sha256": by_target[target]["artifact"]["sha256"],
                "size_bytes": by_target[target]["artifact"]["size_bytes"],
                "runner": by_target[target]["target"],
            }
            for target in sorted(by_target)
        ],
    }
    output = pathlib.Path(sys.argv[1])
    output.parent.mkdir(parents=True, exist_ok=True)
    output.write_text(json.dumps(merged, indent=2) + "\n", encoding="utf-8")
    print(f"cross-artifact contract comparison passed: {len(by_target)} targets")


if __name__ == "__main__":
    main()
