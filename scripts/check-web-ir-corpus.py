#!/usr/bin/env python3
"""Validate the issue 30 fixture corpus and emit a deterministic baseline."""

from __future__ import annotations

import argparse
import json
import re
from pathlib import Path
from typing import NoReturn


ROOT = Path(__file__).resolve().parent.parent
CORPUS_PATH = ROOT / "tests/fixtures/web-ir/corpus-v1.json"
SCENARIO_PATH = ROOT / "benchmarks/scenarios/web-ir-v1.json"
ALLOWED_COVERAGE = {"strong", "partial", "opaque"}
ALLOWED_HINT_DIAGNOSTIC_STATUSES = {"validated", "emitted", "unmatchedParent"}
ALLOWED_RISKS = {
    "readOnly",
    "localMutation",
    "remoteReversible",
    "remoteIrreversible",
    "requiresConfirmation",
    "authentication",
    "dataDisclosure",
    "unknownRisk",
    "staleState",
}
HTML_OPENING_TAG = re.compile(rb"<([A-Za-z][A-Za-z0-9:-]*)\b")
REMOTE_RESOURCE = re.compile(
    rb"<(?:script|link|img)\b[^>]+(?:src|href)\s*=\s*['\"]https?://",
    re.IGNORECASE,
)


def fail(message: str) -> NoReturn:
    raise SystemExit(f"web-ir corpus validation failed: {message}")


def load_json(path: Path) -> dict:
    try:
        value = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as error:
        fail(f"cannot read {path.relative_to(ROOT)}: {error}")
    if not isinstance(value, dict):
        fail(f"{path.relative_to(ROOT)} must contain an object")
    return value


def require_string(value: object, path: str) -> str:
    if not isinstance(value, str) or not value.strip():
        fail(f"{path} must be a non-empty string")
    return value


def require_unique(values: list[str], path: str) -> None:
    if len(values) != len(set(values)):
        fail(f"{path} must not contain duplicate values")


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, help="write the baseline report to this path")
    args = parser.parse_args()

    corpus = load_json(CORPUS_PATH)
    if corpus.get("schemaVersion") != 1:
        fail("corpus schemaVersion must be 1")
    if corpus.get("corpus") != "glass-semantic-execution-v1":
        fail("corpus identifier must be glass-semantic-execution-v1")
    max_fixture_bytes = corpus.get("maxFixtureBytes")
    if not isinstance(max_fixture_bytes, int) or not 1 <= max_fixture_bytes <= 1_048_576:
        fail("maxFixtureBytes must be a bounded positive integer")

    required_categories = corpus.get("requiredCategories")
    if not isinstance(required_categories, list) or not required_categories:
        fail("requiredCategories must be a non-empty array")
    required_categories = [require_string(value, "requiredCategories[]") for value in required_categories]
    require_unique(required_categories, "requiredCategories")

    fixtures = corpus.get("fixtures")
    if not isinstance(fixtures, list) or not fixtures:
        fail("fixtures must be a non-empty array")

    fixture_ids: list[str] = []
    category_coverage: set[str] = set()
    reports: list[dict] = []
    for index, fixture in enumerate(fixtures):
        path = f"fixtures[{index}]"
        if not isinstance(fixture, dict):
            fail(f"{path} must be an object")
        fixture_id = require_string(fixture.get("id"), f"{path}.id")
        fixture_path = require_string(fixture.get("path"), f"{path}.path")
        fixture_ids.append(fixture_id)

        categories = fixture.get("categories")
        if not isinstance(categories, list) or not categories:
            fail(f"{path}.categories must be a non-empty array")
        categories = [require_string(value, f"{path}.categories[]") for value in categories]
        require_unique(categories, f"{path}.categories")
        category_coverage.update(categories)

        source_kinds = fixture.get("sourceKinds")
        if not isinstance(source_kinds, list) or not source_kinds:
            fail(f"{path}.sourceKinds must be a non-empty array")
        source_kinds = [require_string(value, f"{path}.sourceKinds[]") for value in source_kinds]
        require_unique(source_kinds, f"{path}.sourceKinds")

        coverage = fixture.get("coverage")
        if coverage not in ALLOWED_COVERAGE:
            fail(f"{path}.coverage must be one of {sorted(ALLOWED_COVERAGE)}")
        opaque_regions = fixture.get("opaqueRegions")
        if not isinstance(opaque_regions, int) or not 0 <= opaque_regions <= 64:
            fail(f"{path}.opaqueRegions must be between 0 and 64")
        if coverage == "opaque" and opaque_regions == 0:
            fail(f"{path} opaque coverage requires an opaque region")

        expected_entities = fixture.get("expectedEntities")
        if not isinstance(expected_entities, list) or not expected_entities:
            fail(f"{path}.expectedEntities must be a non-empty array")
        expected_entities = [require_string(value, f"{path}.expectedEntities[]") for value in expected_entities]
        if len(expected_entities) > 256:
            fail(f"{path}.expectedEntities exceeds the 256-entity baseline bound")

        expected_relationships = fixture.get("expectedRelationships")
        if not isinstance(expected_relationships, list) or not expected_relationships:
            fail(f"{path}.expectedRelationships must be a non-empty array")
        expected_relationships = [
            require_string(value, f"{path}.expectedRelationships[]") for value in expected_relationships
        ]
        if len(expected_relationships) > 512:
            fail(f"{path}.expectedRelationships exceeds the 512-relationship baseline bound")

        expected_hint_diagnostics = fixture.get("expectedHintDiagnostics", [])
        if not isinstance(expected_hint_diagnostics, list):
            fail(f"{path}.expectedHintDiagnostics must be an array")
        hint_statuses = []
        hint_diagnostic_count = 0
        for hint_index, diagnostic in enumerate(expected_hint_diagnostics):
            diagnostic_path = f"{path}.expectedHintDiagnostics[{hint_index}]"
            if not isinstance(diagnostic, dict):
                fail(f"{diagnostic_path} must be an object")
            status = require_string(diagnostic.get("status"), f"{diagnostic_path}.status")
            if status not in ALLOWED_HINT_DIAGNOSTIC_STATUSES:
                fail(
                    f"{diagnostic_path}.status must be one of "
                    f"{sorted(ALLOWED_HINT_DIAGNOSTIC_STATUSES)}"
                )
            if status in hint_statuses:
                fail(f"{path}.expectedHintDiagnostics must not duplicate statuses")
            hint_statuses.append(status)
            count = diagnostic.get("count")
            if not isinstance(count, int) or not 0 <= count <= 512:
                fail(f"{diagnostic_path}.count must be between 0 and 512")
            hint_diagnostic_count += count
        if hint_diagnostic_count > 512:
            fail(f"{path}.expectedHintDiagnostics exceeds the 512-diagnostic baseline bound")

        risk_hints = fixture.get("riskHints")
        if not isinstance(risk_hints, list) or not risk_hints:
            fail(f"{path}.riskHints must be a non-empty array")
        if any(value not in ALLOWED_RISKS for value in risk_hints):
            fail(f"{path}.riskHints contains an unknown risk")

        resolved = (ROOT / fixture_path).resolve()
        try:
            resolved.relative_to(ROOT / "tests/fixtures/web-ir")
        except ValueError:
            fail(f"{path}.path must stay under tests/fixtures/web-ir")
        if resolved.suffix != ".html":
            fail(f"{path}.path must point to an HTML fixture")
        if not resolved.is_file():
            fail(f"{path}.path does not exist: {fixture_path}")
        content = resolved.read_bytes()
        if not content.strip():
            fail(f"{path}.path is empty")
        if len(content) > max_fixture_bytes:
            fail(f"{path}.path exceeds maxFixtureBytes")
        if REMOTE_RESOURCE.search(content):
            fail(f"{path}.path contains an unbounded remote script, stylesheet, or image")

        reports.append(
            {
                "id": fixture_id,
                "path": fixture_path,
                "categories": categories,
                "fixtureBytes": len(content),
                "htmlElementCount": len(HTML_OPENING_TAG.findall(content)),
                "declaredEntityCount": len(expected_entities),
                "declaredRelationshipCount": len(expected_relationships),
                "declaredHintDiagnosticCount": hint_diagnostic_count,
                "opaqueRegionCount": opaque_regions,
                "coverage": coverage,
            }
        )

    require_unique(fixture_ids, "fixtures")
    missing_categories = set(required_categories) - category_coverage
    if missing_categories:
        fail(f"required categories are missing: {sorted(missing_categories)}")

    scenarios = load_json(SCENARIO_PATH)
    if scenarios.get("schemaVersion") != 1 or scenarios.get("corpus") != corpus["corpus"]:
        fail("scenario manifest must use corpus schema version 1")
    if scenarios.get("baselineType") != "fixture-inventory":
        fail("scenario manifest baselineType must be fixture-inventory")
    scenario_values = scenarios.get("scenarios")
    if not isinstance(scenario_values, list) or not scenario_values:
        fail("scenario manifest scenarios must be a non-empty array")
    scenario_ids = []
    for index, scenario in enumerate(scenario_values):
        path = f"scenarios[{index}]"
        if not isinstance(scenario, dict):
            fail(f"{path} must be an object")
        scenario_id = require_string(scenario.get("id"), f"{path}.id")
        scenario_ids.append(scenario_id)
        fixture_id = require_string(scenario.get("fixtureId"), f"{path}.fixtureId")
        if fixture_id not in fixture_ids:
            fail(f"{path}.fixtureId does not reference a fixture")
        require_string(scenario.get("operation"), f"{path}.operation")
        require_string(scenario.get("level"), f"{path}.level")
        expected_categories = scenario.get("expectedCategories")
        if not isinstance(expected_categories, list) or not expected_categories:
            fail(f"{path}.expectedCategories must be a non-empty array")
        require_string(scenario.get("expectedEntities"), f"{path}.expectedEntities") if isinstance(scenario.get("expectedEntities"), str) else None
        if not isinstance(scenario.get("expectedEntities"), list) or not scenario["expectedEntities"]:
            fail(f"{path}.expectedEntities must be a non-empty array")
    require_unique(scenario_ids, "scenarios")

    reports.sort(key=lambda report: report["id"])
    totals = {
        "fixtureBytes": sum(report["fixtureBytes"] for report in reports),
        "htmlElementCount": sum(report["htmlElementCount"] for report in reports),
        "declaredEntityCount": sum(report["declaredEntityCount"] for report in reports),
        "declaredRelationshipCount": sum(report["declaredRelationshipCount"] for report in reports),
        "declaredHintDiagnosticCount": sum(
            report["declaredHintDiagnosticCount"] for report in reports
        ),
        "opaqueRegionCount": sum(report["opaqueRegionCount"] for report in reports),
    }
    baseline = {
        "schemaVersion": 1,
        "corpus": corpus["corpus"],
        "baselineType": "fixture-inventory",
        "runtimeClaims": False,
        "fixtureCount": len(reports),
        "scenarioCount": len(scenario_values),
        "categoryCount": len(category_coverage),
        "categories": sorted(category_coverage),
        "totals": totals,
        "fixtures": reports,
    }
    serialized = json.dumps(baseline, indent=2, sort_keys=False) + "\n"
    if args.output:
        output = (ROOT / args.output).resolve() if not args.output.is_absolute() else args.output
        try:
            output.relative_to(ROOT)
        except ValueError:
            fail("--output must stay inside the repository")
        output.parent.mkdir(parents=True, exist_ok=True)
        output.write_text(serialized, encoding="utf-8")
    else:
        print(serialized, end="")
    print(
        f"web-ir corpus validated: {len(reports)} fixtures, {len(scenario_values)} scenarios, "
        f"{len(category_coverage)} categories; baseline is inventory-only"
    )


if __name__ == "__main__":
    main()
