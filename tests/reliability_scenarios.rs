use glass::reliability::{
    RELIABILITY_SCENARIO_SCHEMA_VERSION, ReliabilityForbiddenOutcome, ReliabilityScenario,
};

#[test]
fn checked_in_reliability_scenario_is_valid_and_canonical() {
    let source = include_str!("fixtures/reliability-scenario-v1.json");
    let scenario = ReliabilityScenario::from_json(source).unwrap();
    assert_eq!(scenario.schema_version, RELIABILITY_SCENARIO_SCHEMA_VERSION);
    assert_eq!(scenario.steps.len(), 3);
    assert!(
        scenario
            .forbid
            .contains(&ReliabilityForbiddenOutcome::SecretLeaked)
    );
    let canonical = scenario.to_canonical_json().unwrap();
    assert!(canonical.contains("nonIdempotentMutationDuplicated"));
}
