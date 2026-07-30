//! Experimental bounded evidence-extraction contracts.
//!
//! This module defines the request boundary for the native extraction engine
//! planned by issue #30. It does not perform browser work. Inputs are strict
//! authored contracts; observed evidence will use a separate tolerant model.

use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::fmt::{Display, Formatter};

/// Version of the extraction request contract.
pub const EXTRACTION_CONTRACT_SCHEMA_VERSION: u32 = 1;

/// Maximum number of browser nodes one extraction may inspect.
pub const MAX_EXTRACTION_NODES: u32 = 8_192;
/// Maximum text bytes retained by one extraction.
pub const MAX_EXTRACTION_TEXT_BYTES: u32 = 128 * 1024;
/// Maximum DOM/accessibility depth one extraction may traverse.
pub const MAX_EXTRACTION_DEPTH: u16 = 64;
/// Maximum wall-clock duration permitted for one extraction.
pub const MAX_EXTRACTION_DURATION_MS: u64 = 15_000;
/// Maximum serialized evidence bytes returned by one extraction.
pub const MAX_EXTRACTION_OUTPUT_BYTES: u32 = 256 * 1024;

/// The page scope from which evidence may be gathered.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub enum ExtractionScope {
    /// Extract from the active document.
    Document,
    /// Extract from one previously identified semantic region.
    Region { region_id: String },
    /// Extract from one bounded browsing-context scope.
    Frame { frame_id: String },
}

impl ExtractionScope {
    fn validate(&self) -> Result<(), ExtractionContractError> {
        match self {
            Self::Document => Ok(()),
            Self::Region { region_id } => validate_identifier("scope.regionId", region_id),
            Self::Frame { frame_id } => validate_identifier("scope.frameId", frame_id),
        }
    }
}

/// Browser evidence categories an extraction may request.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub enum EvidenceSource {
    Dom,
    Accessibility,
    Layout,
    Forms,
    Navigation,
    Tables,
    Collections,
    Dialogs,
    Frames,
    ShadowDom,
    BoundedProbe,
}

/// Hard resource limits for one extraction request.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ExtractionBudgets {
    pub max_nodes: u32,
    pub max_text_bytes: u32,
    pub max_depth: u16,
    pub max_duration_ms: u64,
    pub max_output_bytes: u32,
}

impl Default for ExtractionBudgets {
    fn default() -> Self {
        Self {
            max_nodes: 2_048,
            max_text_bytes: 32 * 1024,
            max_depth: 32,
            max_duration_ms: 5_000,
            max_output_bytes: 64 * 1024,
        }
    }
}

impl ExtractionBudgets {
    /// Validate all limits against the extraction contract maxima.
    pub fn validate(&self) -> Result<(), ExtractionContractError> {
        validate_positive_bounded("budgets.maxNodes", self.max_nodes, MAX_EXTRACTION_NODES)?;
        validate_positive_bounded(
            "budgets.maxTextBytes",
            self.max_text_bytes,
            MAX_EXTRACTION_TEXT_BYTES,
        )?;
        validate_positive_bounded("budgets.maxDepth", self.max_depth, MAX_EXTRACTION_DEPTH)?;
        validate_positive_bounded(
            "budgets.maxDurationMs",
            self.max_duration_ms,
            MAX_EXTRACTION_DURATION_MS,
        )?;
        validate_positive_bounded(
            "budgets.maxOutputBytes",
            self.max_output_bytes,
            MAX_EXTRACTION_OUTPUT_BYTES,
        )?;
        Ok(())
    }
}

/// A strict, non-mutating evidence extraction request.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ExtractionRequest {
    pub schema_version: u32,
    pub scope: ExtractionScope,
    pub sources: Vec<EvidenceSource>,
    pub budgets: ExtractionBudgets,
}

impl ExtractionRequest {
    /// Validate the authored request before any browser work starts.
    pub fn validate(&self) -> Result<(), ExtractionContractError> {
        if self.schema_version != EXTRACTION_CONTRACT_SCHEMA_VERSION {
            return Err(ExtractionContractError::new(
                "schemaVersion",
                "unsupported extraction contract schema version",
            ));
        }
        self.scope.validate()?;
        if self.sources.is_empty() {
            return Err(ExtractionContractError::new(
                "sources",
                "at least one evidence source is required",
            ));
        }
        let mut unique_sources = BTreeSet::new();
        for source in &self.sources {
            if !unique_sources.insert(*source) {
                return Err(ExtractionContractError::new(
                    "sources",
                    "evidence sources must not be duplicated",
                ));
            }
        }
        self.budgets.validate()
    }

    /// Parse and validate a strict authored request.
    pub fn from_json(input: &str) -> Result<Self, ExtractionContractError> {
        let request: Self = serde_json::from_str(input).map_err(|error| {
            ExtractionContractError::new("$", format!("invalid extraction request: {error}"))
        })?;
        request.validate()?;
        Ok(request)
    }

    /// Serialize a validated request with stable field ordering.
    pub fn to_canonical_json(&self) -> Result<String, ExtractionContractError> {
        self.validate()?;
        serde_json::to_string(self).map_err(|error| {
            ExtractionContractError::new(
                "$",
                format!("failed to serialize extraction request: {error}"),
            )
        })
    }
}

/// A machine-readable extraction contract error.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExtractionContractError {
    pub path: String,
    pub reason: String,
}

impl ExtractionContractError {
    fn new(path: impl Into<String>, reason: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            reason: reason.into(),
        }
    }
}

impl Display for ExtractionContractError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{}: {}", self.path, self.reason)
    }
}

impl std::error::Error for ExtractionContractError {}

fn validate_identifier(path: &str, value: &str) -> Result<(), ExtractionContractError> {
    if value.is_empty() || value.len() > 256 || value.chars().any(char::is_control) {
        return Err(ExtractionContractError::new(
            path,
            "identifier must contain 1-256 non-control characters",
        ));
    }
    Ok(())
}

fn validate_positive_bounded<T>(
    path: &str,
    value: T,
    maximum: T,
) -> Result<(), ExtractionContractError>
where
    T: Copy + PartialOrd + From<u8>,
{
    if value < T::from(1) || value > maximum {
        return Err(ExtractionContractError::new(
            path,
            "value must be positive and within the extraction contract bound",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn request() -> ExtractionRequest {
        ExtractionRequest {
            schema_version: EXTRACTION_CONTRACT_SCHEMA_VERSION,
            scope: ExtractionScope::Document,
            sources: vec![EvidenceSource::Accessibility, EvidenceSource::Forms],
            budgets: ExtractionBudgets::default(),
        }
    }

    #[test]
    fn request_accepts_bounded_non_mutating_sources() {
        let request = request();
        assert!(request.validate().is_ok());
        assert_eq!(
            request.to_canonical_json().unwrap(),
            request.to_canonical_json().unwrap()
        );
    }

    #[test]
    fn request_rejects_duplicate_sources() {
        let mut request = request();
        request.sources.push(EvidenceSource::Forms);
        let error = request.validate().unwrap_err();
        assert_eq!(error.path, "sources");
    }

    #[test]
    fn request_rejects_budget_overflow_and_zero() {
        let mut request = request();
        request.budgets.max_nodes = 0;
        assert_eq!(request.validate().unwrap_err().path, "budgets.maxNodes");
        request.budgets.max_nodes = MAX_EXTRACTION_NODES + 1;
        assert_eq!(request.validate().unwrap_err().path, "budgets.maxNodes");
    }

    #[test]
    fn request_rejects_unknown_authored_fields() {
        let value = json!({
            "schemaVersion": 1,
            "scope": "document",
            "sources": ["accessibility"],
            "budgets": ExtractionBudgets::default(),
            "futureField": true
        });
        assert!(ExtractionRequest::from_json(&value.to_string()).is_err());
    }

    #[test]
    fn scoped_requests_require_bounded_identifiers() {
        let mut request = request();
        request.scope = ExtractionScope::Region {
            region_id: "".into(),
        };
        assert_eq!(request.validate().unwrap_err().path, "scope.regionId");
    }
}
