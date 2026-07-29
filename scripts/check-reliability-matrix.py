#!/usr/bin/env python3
"""Validate and optionally inventory the deterministic reliability matrix."""

import hashlib
import json
import pathlib
import subprocess
import sys


ROOT = pathlib.Path(__file__).resolve().parent.parent
SCENARIO_PATH = ROOT / "tests/fixtures/reliability-capability-suite-v1.json"
FIXTURE_PATH = ROOT / "tests/fixtures/reliability-fixture-v1.json"
TARGETS = {"linux-x86-64", "linux-arm64", "macos-x86-64", "macos-arm64"}
EXPECTED_SCENARIOS = {
    "targets-reordered-before-action",
    "target-replaced-before-action",
    "duplicate-target-is-not-selected",
    "target-moved-across-region",
    "overlay-blocks-submit",
    "effect-marker-arrives-late",
}
FORBIDDEN_OUTCOMES = {
    "wrongTargetExecuted",
    "staleRevisionExecuted",
    "ambiguousTargetSilentlyExecuted",
    "nonIdempotentMutationDuplicated",
    "falseWorkflowCompletion",
    "unsafeResumeReplay",
    "secretLeaked",
    "crossProfileKnowledgeLeak",
    "unboundedLoopEscapedBudget",
    "policyBypassed",
    "checkpointAcceptedAfterIncompatibleDefinitionChange",
    "semanticCacheHidCriticalUnexpectedState",
}


def fail(message: str) -> "NoReturn":
    raise SystemExit(f"reliability matrix validation failed: {message}")


def canonical_hash(value: object) -> str:
    canonical = json.dumps(value, sort_keys=True, separators=(",", ":"))
    return f"sha256:{hashlib.sha256(canonical.encode()).hexdigest()}"


def main() -> None:
    if len(sys.argv) > 2:
        raise SystemExit("usage: check-reliability-matrix.py [OUTPUT.json]")

    try:
        scenarios = json.loads(SCENARIO_PATH.read_text(encoding="utf-8"))
        fixture = json.loads(FIXTURE_PATH.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as error:
        fail(f"cannot read checked-in reliability contract: {error}")

    if not isinstance(scenarios, list) or {scenario.get("id") for scenario in scenarios} != EXPECTED_SCENARIOS:
        fail("scenario IDs do not match the deterministic capability suite")
    if len(scenarios) != len(EXPECTED_SCENARIOS):
        fail("scenario IDs must be unique")
    if fixture.get("schemaVersion") != 1 or fixture.get("id") != "checkout-submit":
        fail("fixture manifest has an unexpected identity")
    if set(fixture.get("oracles", [])) != {"snapshot", "submitSideEffectCount"}:
        fail("fixture must expose both independent snapshot and side-effect oracles")
    entrypoint = ROOT / fixture.get("entrypoint", "")
    if not entrypoint.is_file():
        fail(f"fixture entrypoint is missing: {entrypoint}")

    categories = set()
    forbidden = set()
    rows = []
    for scenario in scenarios:
        scenario_id = scenario["id"]
        if scenario.get("schemaVersion") != 1:
            fail(f"{scenario_id} has an unsupported schema version")
        if set(scenario.get("platforms", [])) != TARGETS:
            fail(f"{scenario_id} does not cover all four supported targets")
        if not scenario.get("steps") or not scenario.get("forbid"):
            fail(f"{scenario_id} must declare steps and forbidden outcomes")
        if any(outcome not in FORBIDDEN_OUTCOMES for outcome in scenario["forbid"]):
            fail(f"{scenario_id} declares an unknown forbidden outcome")
        if not scenario.get("expect", {}).get("terminalState"):
            fail(f"{scenario_id} is missing an expected terminal state")
        for step in scenario["steps"]:
            workflow = step.get("runWorkflow")
            if workflow:
                workflow_path = ROOT / "tests/fixtures" / workflow
                if not workflow_path.is_file():
                    fail(f"{scenario_id} references missing workflow {workflow}")
                try:
                    json.loads(workflow_path.read_text(encoding="utf-8"))
                except (OSError, json.JSONDecodeError) as error:
                    fail(f"{scenario_id} references invalid workflow {workflow}: {error}")
        categories.add(scenario["category"])
        forbidden.update(scenario["forbid"])
        rows.append({
            "id": scenario_id,
            "category": scenario["category"],
            "platforms": sorted(scenario["platforms"]),
            "forbidden_outcomes": sorted(scenario["forbid"]),
            "terminal_state": scenario["expect"]["terminalState"],
            "max_duration_ms": scenario["budgets"]["maxDurationMs"],
            "max_browser_actions": scenario["budgets"]["maxBrowserActions"],
        })

    report = {
        "schema_version": 1,
        "type": "reliability_matrix_inventory",
        "source_revision": subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip(),
        "fixture": {
            "id": fixture["id"],
            "entrypoint": fixture["entrypoint"],
            "content_hash": canonical_hash(fixture),
            "oracles": sorted(fixture["oracles"]),
        },
        "coverage": {
            "scenario_count": len(rows),
            "target_count": len(TARGETS),
            "categories": sorted(categories),
            "forbidden_outcomes": sorted(forbidden),
        },
        "scenarios": sorted(rows, key=lambda row: row["id"]),
        "runtime_certification": "not_run",
    }
    if len(sys.argv) == 2:
        output = pathlib.Path(sys.argv[1])
        output.parent.mkdir(parents=True, exist_ok=True)
        output.write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
    print(
        f"reliability matrix validated: {len(rows)} scenarios across "
        f"{len(TARGETS)} targets; runtime certification not claimed"
    )


if __name__ == "__main__":
    main()
