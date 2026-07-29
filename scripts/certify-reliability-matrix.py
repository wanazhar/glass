#!/usr/bin/env python3
"""Run the release gate independently for every native reliability target."""

import argparse
import json
import pathlib
import subprocess


def fail(message: str) -> None:
    raise SystemExit(f"reliability certification failed: {message}")


def load_json(path: pathlib.Path) -> object:
    try:
        return json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as error:
        fail(f"{path}: {error}")


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("matrix", type=pathlib.Path)
    parser.add_argument("--scenarios", required=True, type=pathlib.Path)
    parser.add_argument("--binary", required=True, type=pathlib.Path)
    parser.add_argument("--version", required=True)
    parser.add_argument("--output", required=True, type=pathlib.Path)
    args = parser.parse_args()

    matrix = load_json(args.matrix)
    if not isinstance(matrix, dict) or matrix.get("type") != "reliability_runtime_matrix":
        fail("input is not a reliability runtime matrix")
    targets = matrix.get("targets")
    if not isinstance(targets, list) or not targets:
        fail("runtime matrix contains no targets")

    reports = []
    for target in targets:
        if not isinstance(target, dict):
            fail("runtime matrix contains an invalid target row")
        root = args.matrix.parent
        observations = root / target["observations_file"]
        replays = root / target["replays_file"]
        command = [
            str(args.binary),
            "certify",
            "release",
            "--version",
            args.version,
            "--scenarios",
            str(args.scenarios),
            "--observations",
            str(observations),
            "--replays",
            str(replays),
        ]
        result = subprocess.run(command, capture_output=True, text=True)
        if result.returncode != 0:
            if result.stdout:
                print(result.stdout, end="")
            if result.stderr:
                print(result.stderr, end="", flush=True)
            fail(f"release gate blocked for {target.get('platform', '<unknown>')}")
        try:
            report = json.loads(result.stdout)
        except json.JSONDecodeError as error:
            fail(f"release gate returned invalid JSON: {error}")
        if report.get("status") != "certified" or not report.get("gate", {}).get(
            "certified"
        ):
            fail(f"release gate did not certify {target.get('platform', '<unknown>')}")
        reports.append(
            {
                "artifact": target["artifact"],
                "platform": target["platform"],
                "scenario_count": report["gate"]["scenarioCount"],
                "passed": report["gate"]["passed"],
                "safe_refusals": report["gate"]["safeRefusals"],
                "gate": report["gate"],
                "scorecard": report["scorecard"],
            }
        )

    output = {
        "schema_version": 1,
        "type": "reliability_runtime_scorecard",
        "source_revision": matrix["source_revision"],
        "version": args.version,
        "scenario_count": matrix["scenario_count"],
        "target_count": len(reports),
        "targets": reports,
        "runtime_certification": "certified",
    }
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(output, indent=2) + "\n", encoding="utf-8")
    print(
        f"reliability matrix certified: {matrix['scenario_count']} scenarios across "
        f"{len(reports)} native targets"
    )


if __name__ == "__main__":
    main()
