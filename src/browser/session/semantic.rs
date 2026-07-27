//! Versioned semantic observation contracts.
//!
//! This module defines the bounded external shape used by the semantic
//! observation engine. It is intentionally separate from [`PageContext`], so
//! existing detailed and raw observation callers remain compatible while the
//! semantic surface is built incrementally.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeSet;

pub const SEMANTIC_OBSERVATION_SCHEMA_VERSION: u32 = 1;
const MAX_REGIONS: usize = 64;
const MAX_EVIDENCE_ITEMS: usize = 8;
const MAX_EVIDENCE_BYTES: usize = 128;
const MAX_ID_BYTES: usize = 128;
const MAX_LABEL_BYTES: usize = 256;
const MAX_TITLE_BYTES: usize = 1_024;
const MAX_URL_BYTES: usize = 2_048;

/// Amount of semantic structure requested by a caller.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SemanticObservationLevel {
    Summary,
    Interactive,
    Structured,
    Detailed,
    Raw,
}

/// Bounded confidence attached to an evidence-backed classification.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SemanticConfidence {
    Exact,
    High,
    Medium,
    Low,
    Unknown,
}

/// Advisory page classification. `Unknown` and `Generic` are valid outcomes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SemanticPageKind {
    Generic,
    Home,
    Search,
    SearchResults,
    Article,
    Documentation,
    Listing,
    Detail,
    Form,
    Authentication,
    Checkout,
    Confirmation,
    Dashboard,
    Settings,
    Error,
    AccessDenied,
    Unknown,
}

/// Initial semantic region taxonomy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SemanticRegionKind {
    Navigation,
    Main,
    Search,
    Form,
    Dialog,
    Alert,
    Status,
    Toolbar,
    FilterPanel,
    Results,
    Collection,
    Table,
    Pagination,
    Article,
    Sidebar,
    CheckoutSummary,
    Authentication,
    Footer,
    Unknown,
}

/// Route identity required to keep semantic handles scoped to one page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SemanticRouteIdentity {
    pub target_id: String,
    pub frame_id: String,
    pub url: String,
}

/// Page-level semantic summary.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SemanticPage {
    pub kind: SemanticPageKind,
    pub title: String,
    pub url: String,
    pub target_id: String,
    pub frame_id: String,
    pub confidence: SemanticConfidence,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub evidence: Vec<String>,
}

/// Revision-scoped handle for requesting one region's details.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SemanticExpansionHandle {
    pub region_id: String,
    pub revision: u64,
    pub route: SemanticRouteIdentity,
}

/// A bounded semantic region summary.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SemanticRegion {
    pub id: String,
    pub kind: SemanticRegionKind,
    pub label: String,
    pub interactive_count: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub item_count: Option<usize>,
    pub confidence: SemanticConfidence,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub evidence: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expansion: Option<SemanticExpansionHandle>,
}

/// Explicit bounds and omission metadata for one semantic observation.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SemanticObservationLimits {
    pub truncated: bool,
    pub omitted_regions: usize,
    #[serde(default)]
    pub omitted_targets: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub omitted_bytes: Option<usize>,
}

/// Versioned semantic page model.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SemanticObservation {
    pub schema_version: u32,
    pub revision: u64,
    pub level: SemanticObservationLevel,
    pub route: SemanticRouteIdentity,
    pub page: SemanticPage,
    pub regions: Vec<SemanticRegion>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub changes: Option<Value>,
    pub limits: SemanticObservationLimits,
}

impl SemanticObservation {
    /// Validate bounds and cross-field identity invariants.
    pub fn validate(&self) -> Result<(), SemanticObservationError> {
        if self.schema_version != SEMANTIC_OBSERVATION_SCHEMA_VERSION {
            return Err(SemanticObservationError::new(
                "schemaVersion",
                format!(
                    "unsupported schema version {}; expected {}",
                    self.schema_version, SEMANTIC_OBSERVATION_SCHEMA_VERSION
                ),
            ));
        }
        validate_route("route", &self.route)?;
        validate_route_fields(
            "page",
            &self.page.target_id,
            &self.page.frame_id,
            &self.page.url,
        )?;
        validate_text("page.title", &self.page.title, MAX_TITLE_BYTES, true)?;
        validate_evidence("page.evidence", &self.page.evidence)?;
        if self.regions.len() > MAX_REGIONS {
            return Err(SemanticObservationError::new(
                "regions",
                format!("contains more than {MAX_REGIONS} regions"),
            ));
        }
        if self.page.target_id != self.route.target_id
            || self.page.frame_id != self.route.frame_id
            || self.page.url != self.route.url
        {
            return Err(SemanticObservationError::new(
                "page",
                "page route identity does not match observation route",
            ));
        }
        let mut region_ids = BTreeSet::new();
        for (index, region) in self.regions.iter().enumerate() {
            let path = format!("regions[{index}]");
            validate_text(&format!("{path}.id"), &region.id, MAX_ID_BYTES, false)?;
            if !region_ids.insert(region.id.as_str()) {
                return Err(SemanticObservationError::new(
                    format!("{path}.id"),
                    "duplicate semantic region ID",
                ));
            }
            validate_text(
                &format!("{path}.label"),
                &region.label,
                MAX_LABEL_BYTES,
                false,
            )?;
            validate_evidence(&format!("{path}.evidence"), &region.evidence)?;
            if let Some(expansion) = &region.expansion {
                if expansion.region_id != region.id || expansion.revision != self.revision {
                    return Err(SemanticObservationError::new(
                        format!("{path}.expansion"),
                        "expansion handle does not belong to this region revision",
                    ));
                }
                if expansion.route != self.route {
                    return Err(SemanticObservationError::new(
                        format!("{path}.expansion.route"),
                        "expansion handle route does not match observation route",
                    ));
                }
            }
        }
        Ok(())
    }

    /// Parse and validate a semantic observation from JSON.
    pub fn from_json(input: &str) -> Result<Self, SemanticObservationError> {
        let observation: Self = serde_json::from_str(input).map_err(|error| {
            SemanticObservationError::new("$", format!("invalid observation shape: {error}"))
        })?;
        observation.validate()?;
        Ok(observation)
    }

    /// Serialize a validated observation deterministically.
    pub fn to_canonical_json(&self) -> Result<String, SemanticObservationError> {
        self.validate()?;
        serde_json::to_string(self)
            .map_err(|error| SemanticObservationError::new("$", error.to_string()))
    }
}

fn validate_route(
    path: &str,
    route: &SemanticRouteIdentity,
) -> Result<(), SemanticObservationError> {
    validate_route_fields(path, &route.target_id, &route.frame_id, &route.url)
}

fn validate_route_fields(
    path: &str,
    target_id: &str,
    frame_id: &str,
    url: &str,
) -> Result<(), SemanticObservationError> {
    validate_text(&format!("{path}.targetId"), target_id, MAX_ID_BYTES, false)?;
    validate_text(&format!("{path}.frameId"), frame_id, MAX_ID_BYTES, false)?;
    validate_text(&format!("{path}.url"), url, MAX_URL_BYTES, false)
}

fn validate_evidence(path: &str, evidence: &[String]) -> Result<(), SemanticObservationError> {
    if evidence.len() > MAX_EVIDENCE_ITEMS {
        return Err(SemanticObservationError::new(
            path,
            format!("contains more than {MAX_EVIDENCE_ITEMS} evidence items"),
        ));
    }
    for (index, item) in evidence.iter().enumerate() {
        validate_text(&format!("{path}[{index}]"), item, MAX_EVIDENCE_BYTES, false)?;
    }
    Ok(())
}

fn validate_text(
    path: &str,
    value: &str,
    maximum: usize,
    allow_empty: bool,
) -> Result<(), SemanticObservationError> {
    if (!allow_empty && value.is_empty()) || value.len() > maximum {
        return Err(SemanticObservationError::new(
            path,
            format!(
                "must be {} and at most {maximum} bytes",
                if allow_empty { "at most" } else { "non-empty" }
            ),
        ));
    }
    Ok(())
}

/// Path-aware semantic contract validation error.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SemanticObservationError {
    pub path: String,
    pub reason: String,
}

impl SemanticObservationError {
    fn new(path: impl Into<String>, reason: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            reason: reason.into(),
        }
    }
}

impl std::fmt::Display for SemanticObservationError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{}: {}", self.path, self.reason)
    }
}

impl std::error::Error for SemanticObservationError {}

#[cfg(test)]
mod tests {
    use super::*;

    fn observation() -> SemanticObservation {
        let route = SemanticRouteIdentity {
            target_id: "target".into(),
            frame_id: "frame".into(),
            url: "https://example.test/search".into(),
        };
        SemanticObservation {
            schema_version: SEMANTIC_OBSERVATION_SCHEMA_VERSION,
            revision: 42,
            level: SemanticObservationLevel::Summary,
            page: SemanticPage {
                kind: SemanticPageKind::SearchResults,
                title: "Search".into(),
                url: route.url.clone(),
                target_id: route.target_id.clone(),
                frame_id: route.frame_id.clone(),
                confidence: SemanticConfidence::High,
                evidence: vec!["role=main".into()],
            },
            regions: vec![SemanticRegion {
                id: "region_results".into(),
                kind: SemanticRegionKind::Results,
                label: "Search results".into(),
                interactive_count: 2,
                item_count: Some(10),
                confidence: SemanticConfidence::High,
                evidence: vec!["repeated item structure".into()],
                expansion: Some(SemanticExpansionHandle {
                    region_id: "region_results".into(),
                    revision: 42,
                    route: route.clone(),
                }),
            }],
            changes: None,
            limits: SemanticObservationLimits::default(),
            route,
        }
    }

    #[test]
    fn canonical_json_is_stable_and_camel_case() {
        let observation = observation();
        let first = observation.to_canonical_json().unwrap();
        let second = observation.to_canonical_json().unwrap();
        assert_eq!(first, second);
        assert!(first.contains("\"schemaVersion\":1"));
        assert!(first.contains("\"searchResults\""));
        assert!(first.contains("\"regionId\":\"region_results\""));
    }

    #[test]
    fn rejects_duplicate_regions_and_route_mismatch() {
        let mut duplicate_regions = observation();
        duplicate_regions
            .regions
            .push(duplicate_regions.regions[0].clone());
        let error = duplicate_regions.validate().unwrap_err();
        assert_eq!(error.path, "regions[1].id");

        let mut route_mismatch = observation();
        route_mismatch.page.url = "https://other.test".into();
        let error = route_mismatch.validate().unwrap_err();
        assert_eq!(error.path, "page");
    }

    #[test]
    fn rejects_unknown_fields_before_accepting_future_contracts() {
        let mut value: Value = serde_json::to_value(observation()).unwrap();
        value["futureField"] = Value::Bool(true);
        let error = SemanticObservation::from_json(&value.to_string()).unwrap_err();
        assert_eq!(error.path, "$");
    }
}
