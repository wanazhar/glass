use glass::reliability::{
    RELIABILITY_SCENARIO_SCHEMA_VERSION, ReliabilityFixtureManifest, ReliabilityForbiddenOutcome,
    ReliabilityPlatform, ReliabilityScenario,
};
use serde_json::Value;

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

#[test]
fn checked_in_capability_suite_has_unique_valid_scenarios() {
    let source = include_str!("fixtures/reliability-capability-suite-v1.json");
    let values: Vec<Value> = serde_json::from_str(source).unwrap();
    let scenarios: Vec<ReliabilityScenario> = values
        .into_iter()
        .map(ReliabilityScenario::from_value)
        .collect::<Result<_, _>>()
        .unwrap();
    let manifest =
        ReliabilityFixtureManifest::from_json(include_str!("fixtures/reliability-fixture-v1.json"))
            .unwrap();
    for scenario in &scenarios {
        manifest.validate_scenario(scenario).unwrap();
    }
    let ids: std::collections::BTreeSet<_> =
        scenarios.iter().map(|scenario| &scenario.id).collect();
    assert_eq!(scenarios.len(), 6);
    assert_eq!(ids.len(), scenarios.len());
    assert!(scenarios.iter().any(|scenario| {
        scenario.steps.iter().any(|step| {
            step.apply_control
                == Some(glass::reliability::ReliabilityFixtureControl::DuplicateTarget)
        })
    }));
    let plan = scenarios[0].execution_plan(&manifest).unwrap();
    assert_eq!(plan.operations.len(), 2);
}

#[test]
fn capability_suite_covers_the_supported_release_platform_matrix() {
    let values: Vec<Value> = serde_json::from_str(include_str!(
        "fixtures/reliability-capability-suite-v1.json"
    ))
    .unwrap();
    let scenarios: Vec<ReliabilityScenario> = values
        .into_iter()
        .map(ReliabilityScenario::from_value)
        .collect::<Result<_, _>>()
        .unwrap();
    let expected = std::collections::BTreeSet::from([
        ReliabilityPlatform::LinuxX86_64,
        ReliabilityPlatform::MacosX86_64,
        ReliabilityPlatform::MacosArm64,
    ]);

    for scenario in scenarios {
        let platforms: std::collections::BTreeSet<_> = scenario.platforms.iter().copied().collect();
        assert_eq!(
            platforms, expected,
            "scenario {} has an incomplete matrix",
            scenario.id
        );
    }
}
