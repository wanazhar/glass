//! Validate persistent-knowledge scope, freshness, and quarantine outcomes.

use glass::browser::session::{
    KnowledgeAssessmentStatus, KnowledgeBackendCapability, KnowledgeBackendKind,
    KnowledgeLookupContext, KnowledgeProfileScope, KnowledgeRecord, KnowledgeSurfaceKind,
};
use serde::Deserialize;
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
    record: KnowledgeRecord,
    context: Context,
    #[serde(rename = "expectedStatus")]
    expected_status: KnowledgeAssessmentStatus,
}

#[derive(Debug, Deserialize)]
struct Context {
    origin: String,
    path: String,
    #[serde(rename = "profileScope")]
    profile_scope: KnowledgeProfileScope,
    #[serde(rename = "profileKey")]
    profile_key: Option<String>,
    locale: Option<String>,
    #[serde(rename = "tenantKey")]
    tenant_key: Option<String>,
    #[serde(rename = "browserFamily")]
    browser_family: String,
    #[serde(rename = "browserVersion")]
    browser_version: Option<String>,
    #[serde(rename = "glassSchemaVersion")]
    glass_schema_version: u32,
    #[serde(rename = "policyPreset")]
    policy_preset: String,
    #[serde(rename = "surfaceKind")]
    surface_kind: KnowledgeSurfaceKind,
    #[serde(rename = "backendKind")]
    backend_kind: KnowledgeBackendKind,
    #[serde(rename = "backendCapabilities")]
    backend_capabilities: Vec<KnowledgeBackendCapability>,
    landmarks: Vec<String>,
    now: String,
}

impl Context {
    fn into_lookup(
        self,
        observation_revision: u64,
    ) -> Result<KnowledgeLookupContext, chrono::ParseError> {
        Ok(KnowledgeLookupContext {
            origin: self.origin,
            path: self.path,
            profile_scope: self.profile_scope,
            profile_key: self.profile_key,
            locale: self.locale,
            tenant_key: self.tenant_key,
            browser_family: self.browser_family,
            browser_version: self.browser_version,
            glass_schema_version: self.glass_schema_version,
            policy_preset: self.policy_preset,
            landmarks: self.landmarks,
            now_epoch_seconds: chrono::DateTime::parse_from_rfc3339(&self.now)?.timestamp(),
            current_revision: observation_revision,
            surface_kind: Some(self.surface_kind),
            backend_kind: Some(self.backend_kind),
            backend_capabilities: self.backend_capabilities,
        })
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let corpus: Corpus =
        serde_json::from_str(include_str!("../benchmarks/scenarios/knowledge-scorecard-v2.json"))?;
    assert_eq!(corpus.schema_version, 1);
    let mut names = BTreeSet::new();
    let mut ids = BTreeSet::new();

    for fixture in corpus.fixtures {
        assert!(names.insert(fixture.name.clone()), "duplicate fixture name");
        assert!(
            ids.insert(fixture.record.record_id.clone()),
            "duplicate record ID"
        );
        fixture.record.validate()?;
        let data = serde_json::to_string(&fixture.record.data)?;
        for forbidden in ["axr-", "rawAccessibility", "password", "cookie", "secret"] {
            assert!(
                !data.contains(forbidden),
                "{} contains {forbidden}",
                fixture.name
            );
        }
        let current_revision = fixture
            .record
            .retrieval
            .current_validation
            .current_revision
            .unwrap_or(1);
        let assessment = fixture
            .record
            .assess(&fixture.context.into_lookup(current_revision)?);
        assert_eq!(
            assessment.status, fixture.expected_status,
            "{}",
            fixture.name
        );
    }

    println!("validated {} knowledge fixtures", names.len());
    Ok(())
}
