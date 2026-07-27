//! Validate the checked-in semantic observation canaries.

use glass::browser::session::SemanticObservation;
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
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let corpus: Corpus = serde_json::from_str(include_str!(
        "../benchmarks/scenarios/semantic-observation-v1.json"
    ))?;
    assert_eq!(corpus.schema_version, 1);
    let mut names = BTreeSet::new();
    let mut levels = BTreeSet::new();
    for fixture in corpus.fixtures {
        assert!(names.insert(fixture.name.clone()), "duplicate fixture name");
        let input = serde_json::to_string(&fixture.observation)?;
        let observation = SemanticObservation::from_json(&input)?;
        let canonical = observation.to_canonical_json()?;
        let round_trip = SemanticObservation::from_json(&canonical)?;
        assert_eq!(round_trip.to_canonical_json()?, canonical);
        levels.insert(format!("{:?}", observation.level));
        assert!(
            canonical.len() <= 128 * 1024,
            "fixture exceeds payload budget"
        );
        assert!(
            !canonical.contains("password"),
            "fixture exposes a secret field"
        );
    }
    assert_eq!(levels.len(), 4);
    println!("validated {} semantic fixtures", names.len());
    Ok(())
}
