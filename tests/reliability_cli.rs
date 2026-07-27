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
    assert_eq!(report["gate"]["scenarioCount"], 5);
    assert_eq!(report["gate"]["certified"], false);
    assert_eq!(report["gate"]["failures"][0]["code"], "missing_evidence");
}
