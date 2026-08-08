use serde_json::Value;
use std::collections::BTreeSet;
use std::fs;
use std::path::PathBuf;

fn repository_path(relative: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(relative)
}

fn manifest(path: &str) -> Value {
    let content = fs::read_to_string(repository_path(path)).expect("manifest should be readable");
    serde_json::from_str(&content).expect("manifest should contain valid JSON")
}

#[test]
fn web_ir_corpus_covers_the_pillar_zero_categories() {
    let corpus = manifest("tests/fixtures/web-ir/corpus-v1.json");
    assert_eq!(corpus["schemaVersion"], 1);
    assert_eq!(corpus["corpus"], "glass-semantic-execution-v1");

    let required = corpus["requiredCategories"]
        .as_array()
        .expect("required categories should be an array")
        .iter()
        .map(|value| value.as_str().expect("category should be a string"))
        .collect::<BTreeSet<_>>();
    let covered = corpus["fixtures"]
        .as_array()
        .expect("fixtures should be an array")
        .iter()
        .flat_map(|fixture| {
            fixture["categories"]
                .as_array()
                .expect("fixture categories should be an array")
                .iter()
                .map(|value| value.as_str().expect("fixture category should be a string"))
        })
        .collect::<BTreeSet<_>>();

    assert!(required.is_subset(&covered));
    assert_eq!(corpus["fixtures"].as_array().unwrap().len(), 8);
}

#[test]
fn web_ir_corpus_paths_and_bounds_are_deterministic() {
    let corpus = manifest("tests/fixtures/web-ir/corpus-v1.json");
    let max_bytes = corpus["maxFixtureBytes"]
        .as_u64()
        .expect("fixture byte bound should be numeric");
    let mut fixture_ids = BTreeSet::new();

    for fixture in corpus["fixtures"]
        .as_array()
        .expect("fixtures should be an array")
    {
        let id = fixture["id"]
            .as_str()
            .expect("fixture ID should be a string");
        assert!(fixture_ids.insert(id), "fixture IDs must be unique");
        let relative_path = fixture["path"]
            .as_str()
            .expect("fixture path should be a string");
        assert!(relative_path.starts_with("tests/fixtures/web-ir/"));
        let bytes = fs::read(repository_path(relative_path)).expect("fixture should exist");
        assert!(!bytes.is_empty());
        assert!(bytes.len() as u64 <= max_bytes);
        assert!(fixture["expectedEntities"].as_array().is_some());
        assert!(fixture["expectedRelationships"].as_array().is_some());
        assert!(fixture["opaqueRegions"].as_u64().is_some());
        assert!(fixture["runtimeExpectedEntities"].as_object().is_some());
        assert!(fixture["runtimeExpectedRelationships"].as_array().is_some());
        assert!(fixture["runtimeExpectedOpaqueRegions"].as_u64().is_some());
    }
}

#[test]
fn web_ir_corpus_declares_bounded_hint_diagnostic_expectations() {
    let corpus = manifest("tests/fixtures/web-ir/corpus-v1.json");
    let fixture = corpus["fixtures"]
        .as_array()
        .unwrap()
        .iter()
        .find(|fixture| fixture["id"] == "form-custom")
        .expect("form-custom fixture should exist");
    let diagnostics = fixture["expectedHintDiagnostics"]
        .as_array()
        .expect("hint diagnostics should be an array");
    assert_eq!(diagnostics.len(), 1);
    assert_eq!(diagnostics[0]["status"], "emitted");
    assert_eq!(diagnostics[0]["count"], 2);
}

#[test]
fn web_ir_scenario_manifest_references_known_fixtures() {
    let corpus = manifest("tests/fixtures/web-ir/corpus-v1.json");
    let scenarios = manifest("benchmarks/scenarios/web-ir-v1.json");
    assert_eq!(scenarios["schemaVersion"], 1);
    assert_eq!(scenarios["corpus"], corpus["corpus"]);
    assert_eq!(scenarios["baselineType"], "live-extraction-corpus");

    let fixture_ids = corpus["fixtures"]
        .as_array()
        .expect("fixtures should be an array")
        .iter()
        .map(|fixture| {
            fixture["id"]
                .as_str()
                .expect("fixture ID should be a string")
        })
        .collect::<BTreeSet<_>>();
    let scenario_values = scenarios["scenarios"]
        .as_array()
        .expect("scenarios should be an array");
    let mut scenario_ids = BTreeSet::new();
    for scenario in scenario_values {
        let id = scenario["id"]
            .as_str()
            .expect("scenario ID should be a string");
        assert!(scenario_ids.insert(id), "scenario IDs must be unique");
        let fixture_id = scenario["fixtureId"]
            .as_str()
            .expect("scenario fixture ID should be a string");
        assert!(fixture_ids.contains(fixture_id));
        assert!(scenario["expectedCategories"].as_array().is_some());
        assert!(scenario["expectedEntities"].as_array().is_some());
    }
    assert_eq!(scenario_values.len(), fixture_ids.len());
}

#[test]
fn web_ir_revision_fixture_covers_compatible_stale_and_ambiguous_cases() {
    let fixture = manifest("tests/fixtures/web-ir/revision-cases-v1.json");
    assert_eq!(fixture["schemaVersion"], 1);
    let cases = fixture["cases"]
        .as_array()
        .expect("revision cases should be an array");
    assert_eq!(cases.len(), 5);
    assert!(cases.iter().any(|case| {
        case["id"] == "stale-revision"
            && case["expected"] == "rejected"
            && case["errorPath"] == "revision"
    }));
    assert!(cases.iter().any(|case| {
        case["id"] == "ambiguous-continuity"
            && case["expected"] == "ambiguous"
            && case["status"] == "ambiguous"
    }));
}

#[test]
fn adversarial_corpus_declares_semantic_and_metamorphic_outcomes() {
    let suite = manifest("tests/fixtures/web-ir/adversarial-v1.json");
    assert_eq!(suite["schemaVersion"], 1);
    assert_eq!(suite["suite"], "glass-web-ir-adversarial-v1");
    let cases = suite["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 9);
    let ids = cases
        .iter()
        .map(|case| case["id"].as_str().unwrap())
        .collect::<BTreeSet<_>>();
    assert_eq!(ids.len(), cases.len());
    assert!(cases.iter().all(|case| {
        case["mutation"]
            .as_str()
            .is_some_and(|value| !value.is_empty())
            && case["expected"]
                .as_str()
                .is_some_and(|value| !value.is_empty())
    }));
}
