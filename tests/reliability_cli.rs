use serde_json::Value;
use std::process::Command;

#[test]
fn release_certification_blocks_a_suite_without_observations() {
    let output = Command::new(env!("CARGO_BIN_EXE_glass"))
        .args([
            "certify",
            "release",
            "--version",
            "0.2.0",
            "--scenarios",
            "tests/fixtures/reliability-capability-suite-v1.json",
            "--observations",
            "tests/fixtures/reliability-observations-empty.json",
        ])
        .output()
        .unwrap();

    assert!(!output.status.success());
    let report: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["status"], "blocked");
    assert_eq!(report["gate"]["scenarioCount"], 6);
    assert_eq!(report["gate"]["certified"], false);
    assert_eq!(report["gate"]["failures"][0]["code"], "missing_evidence");
}

#[test]
fn execution_plan_command_is_browser_free_and_manifest_bound() {
    let output = Command::new(env!("CARGO_BIN_EXE_glass"))
        .args([
            "certify",
            "plan",
            "--scenario",
            "tests/fixtures/reliability-scenario-v1.json",
            "--fixture",
            "tests/fixtures/reliability-fixture-v1.json",
        ])
        .output()
        .unwrap();

    assert!(output.status.success());
    let report: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["status"], "valid");
    assert_eq!(
        report["plan"]["scenarioId"],
        "duplicate-submit-after-timeout"
    );
    assert_eq!(report["plan"]["fixtureId"], "checkout-submit");
    assert_eq!(report["plan"]["operations"].as_array().unwrap().len(), 3);
}
