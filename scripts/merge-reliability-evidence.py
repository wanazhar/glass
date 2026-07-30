#!/usr/bin/env python3
"""Merge native reliability-suite evidence and prepare per-target gate inputs."""

import json
import pathlib
import subprocess
import sys


TARGETS = {
    "glass-linux-x86_64": "linux-x86-64",
    "glass-linux-arm64": "linux-arm64",
    "glass-macos-x86_64": "macos-x86-64",
    "glass-macos-aarch64": "macos-arm64",
}


def fail(message: str) -> None:
    raise SystemExit(f"reliability evidence merge failed: {message}")


def load_json(path: pathlib.Path) -> object:
    try:
        return json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as error:
        fail(f"{path}: {error}")


def main() -> None:
    if len(sys.argv) != 3:
        raise SystemExit("usage: merge-reliability-evidence.py OUTPUT.json EVIDENCE_ROOT")

    output = pathlib.Path(sys.argv[1])
    evidence_root = pathlib.Path(sys.argv[2])
    suite_path = pathlib.Path("tests/fixtures/reliability-capability-suite-v1.json")
    scenarios = load_json(suite_path)
    if not isinstance(scenarios, list):
        fail("capability suite is not a JSON array")
    scenario_ids = [scenario.get("id") for scenario in scenarios]
    if any(not isinstance(scenario_id, str) for scenario_id in scenario_ids):
        fail("capability suite contains a scenario without a string ID")
    if len(set(scenario_ids)) != len(scenario_ids):
        fail("capability suite contains duplicate scenario IDs")

    targets = []
    for artifact, platform in TARGETS.items():
        target_dir = evidence_root / artifact
        if not target_dir.is_dir():
            fail(f"missing evidence directory {target_dir}")
        evidence_paths = sorted(target_dir.glob("*.json"))
        if {path.stem for path in evidence_paths} != set(scenario_ids):
            fail(
                f"{artifact} must contain exactly one evidence file for each scenario; "
                f"found {[path.name for path in evidence_paths]}"
            )

        observations = []
        replays = []
        hashes = {}
        fixture_hashes = set()
        artifacts = set()
        source_revisions = set()
        for path in evidence_paths:
            value = load_json(path)
            if not isinstance(value, dict):
                fail(f"{path} is not an evidence object")
            observation = value.get("observation")
            replay = value.get("replay")
            if not isinstance(observation, dict) or not isinstance(replay, dict):
                fail(f"{path} must contain observation and replay objects")
            scenario_id = observation.get("scenarioId")
            if scenario_id != path.stem:
                fail(f"{path} has mismatched scenarioId {scenario_id!r}")
            metadata = observation.get("metadata", {})
            if metadata.get("platform") != platform:
                fail(
                    f"{path} reports platform {metadata.get('platform')!r}; "
                    f"expected {platform!r}"
                )
            if observation.get("classification") not in {"passed", "safe_refusal"}:
                fail(f"{path} has a non-certifying classification")
            if observation.get("forbiddenOutcomes"):
                fail(f"{path} reports forbidden outcomes")
            if observation.get("oracleEvidence") is not True:
                fail(f"{path} is missing independent oracle evidence")
            if observation.get("artifactsComplete") is not True:
                fail(f"{path} is missing complete run artifacts")
            if replay.get("scenarioId") != scenario_id:
                fail(f"{path} replay has a mismatched scenarioId")
            if replay.get("scenarioHash") != observation.get("scenarioHash"):
                fail(f"{path} replay and observation scenario hashes differ")
            if replay.get("observation") != observation:
                fail(f"{path} replay observation differs from the run observation")
            if not replay.get("events"):
                fail(f"{path} replay contains no redacted events")
            artifact_metadata = value.get("artifact")
            if not isinstance(artifact_metadata, dict):
                fail(f"{path} is not bound to a packaged artifact")
            if artifact_metadata.get("name") != artifact:
                fail(f"{path} is bound to the wrong artifact")
            if artifact_metadata.get("target") != platform:
                fail(f"{path} is bound to the wrong target")
            artifact_hash = artifact_metadata.get("sha256", "")
            if len(artifact_hash) != 64 or any(
                character not in "0123456789abcdef" for character in artifact_hash
            ):
                fail(f"{path} has an invalid artifact SHA-256")
            if artifact_metadata.get("size_bytes", 0) <= 0:
                fail(f"{path} has an invalid artifact size")
            artifacts.add(json.dumps(artifact_metadata, sort_keys=True))
            source_revision = value.get("source_revision")
            if (
                not isinstance(source_revision, str)
                or len(source_revision) != 40
                or any(character not in "0123456789abcdef" for character in source_revision)
            ):
                fail(f"{path} is missing a valid source revision")
            source_revisions.add(source_revision)
            hashes[scenario_id] = observation.get("scenarioHash")
            fixture_hashes.add(replay.get("fixtureHash"))
            observations.append(observation)
            replays.append(replay)

        if len(fixture_hashes) != 1:
            fail(f"{artifact} contains inconsistent fixture hashes")
        if len(artifacts) != 1:
            fail(f"{artifact} contains inconsistent artifact bindings")
        if len(source_revisions) != 1:
            fail(f"{artifact} contains inconsistent source revisions")
        observation_path = output.parent / f"reliability-observations-{artifact}.json"
        replay_path = output.parent / f"reliability-replays-{artifact}.json"
        output.parent.mkdir(parents=True, exist_ok=True)
        observation_path.write_text(
            json.dumps(observations, indent=2) + "\n", encoding="utf-8"
        )
        replay_path.write_text(json.dumps(replays, indent=2) + "\n", encoding="utf-8")
        targets.append(
            {
                "artifact": artifact,
                "platform": platform,
                "scenario_count": len(observations),
                "passed": sum(
                    observation["classification"] == "passed"
                    for observation in observations
                ),
                "safe_refusals": sum(
                    observation["classification"] == "safe_refusal"
                    for observation in observations
                ),
                "scenario_hashes": hashes,
                "fixture_hash": next(iter(fixture_hashes)),
                "artifact": json.loads(next(iter(artifacts))),
                "source_revision": next(iter(source_revisions)),
                "observations_file": observation_path.name,
                "replays_file": replay_path.name,
                "runtime_certification": "pending_release_gate",
            }
        )

    source_revision = subprocess.check_output(
        ["git", "rev-parse", "HEAD"], text=True
    ).strip()
    report = {
        "schema_version": 1,
        "type": "reliability_runtime_matrix",
        "source_revision": source_revision,
        "scenario_count": len(scenario_ids),
        "target_count": len(targets),
        "targets": targets,
        "runtime_certification": "pending_release_gate",
    }
    output.parent.mkdir(parents=True, exist_ok=True)
    output.write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
    print(
        f"reliability evidence merged: {len(scenario_ids)} scenarios across "
        f"{len(targets)} targets; release gate pending"
    )


if __name__ == "__main__":
    main()
