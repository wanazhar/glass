//! Validate deterministic intent-resolution classifications and policy outcomes.

use glass::browser::session::{
    IntentPolicyDecision, SemanticIntentRequest, SemanticIntentResult, SemanticObservation,
    SemanticResolution, resolve_intent,
};
use serde::Deserialize;
use serde_json::Value;
use std::collections::BTreeSet;

#[derive(Debug, Deserialize)]
struct Corpus {
    #[serde(rename = "schemaVersion")]
    schema_version: u32,
    fixtures: Vec<Fixture>,
}

#[derive(Debug, Deserialize)]
struct Fixture {
    name: String,
    observation: Value,
    request: Value,
    expected: Expected,
}

#[derive(Debug, Deserialize)]
struct Expected {
    resolution: SemanticResolution,
    #[serde(rename = "policyDecision")]
    policy_decision: IntentPolicyDecision,
    #[serde(rename = "candidateCount")]
    candidate_count: usize,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let corpus: Corpus = serde_json::from_str(include_str!(
        "../benchmarks/scenarios/intent-resolution-v1.json"
    ))?;
    assert_eq!(corpus.schema_version, 1);
    let mut names = BTreeSet::new();
    for fixture in corpus.fixtures {
        assert!(names.insert(fixture.name.clone()), "duplicate fixture name");
        let observation =
            SemanticObservation::from_json(&serde_json::to_string(&fixture.observation)?)?;
        let request = SemanticIntentRequest::from_json(&serde_json::to_string(&fixture.request)?)?;
        let result: SemanticIntentResult = resolve_intent(&request, &observation);
        assert_eq!(
            result.resolution, fixture.expected.resolution,
            "{}",
            fixture.name
        );
        assert_eq!(
            result.policy_decision, fixture.expected.policy_decision,
            "{}",
            fixture.name
        );
        assert_eq!(
            result.candidates.len(),
            fixture.expected.candidate_count,
            "{}",
            fixture.name
        );
        result.validate()?;
    }
    println!("validated {} intent fixtures", names.len());
    Ok(())
}
