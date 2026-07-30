//! Experimental bounded evidence-extraction contracts.
//!
//! This module defines the request boundary for the native extraction engine
//! planned by issue #30. It does not perform browser work. Inputs are strict
//! authored contracts; observed evidence will use a separate tolerant model.
use crate::browser::dom::{CompactAxNode, DomNode};
use crate::browser::session::PageContext;
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

/// Bounded evidence produced from an existing page observation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExtractionEvidence {
    pub schema_version: u32,
    pub revision: u64,
    pub scope: ExtractionScope,
    pub sources: Vec<EvidenceSource>,
    pub facts: Vec<EvidenceFact>,
    pub limits: ExtractionEvidenceLimits,
}

/// One redacted, source-labelled fact from browser evidence.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EvidenceFact {
    pub source: EvidenceSource,
    pub kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub required: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub read_only: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub empty: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub geometry_present: Option<bool>,
}

/// Explicit omissions and truncation from one evidence extraction.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExtractionEvidenceLimits {
    pub truncated: bool,
    pub omitted_facts: u32,
    pub text_bytes: u32,
    pub missing_sources: Vec<EvidenceSource>,
}

/// Extract bounded, redacted facts from an existing page observation.
///
/// This adapter is deliberately side-effect free. It consumes only the
/// already-collected `PageContext`; browser acquisition remains owned by the
/// session observation layer.
pub fn extract_page_context(
    context: &PageContext,
    request: &ExtractionRequest,
) -> Result<ExtractionEvidence, ExtractionContractError> {
    request.validate()?;
    if !matches!(request.scope, ExtractionScope::Document) {
        return Err(ExtractionContractError::new(
            "scope",
            "region and frame extraction require a scoped page observation",
        ));
    }

    let mut collector = EvidenceCollector::new(request.budgets);
    let mut missing_sources = BTreeSet::new();
    let incomplete_accessibility = context.incomplete.iter().any(|reason| {
        matches!(
            reason,
            crate::browser::session::ObservationIncompleteReason::AccessibilityNode
                | crate::browser::session::ObservationIncompleteReason::AccessibilityLabel
        )
    });

    for source in &request.sources {
        match source {
            EvidenceSource::Accessibility => {
                if incomplete_accessibility {
                    missing_sources.insert(*source);
                } else {
                    for root in &context.accessibility.roots {
                        collect_accessibility(root, &mut collector, 0);
                    }
                }
            }
            EvidenceSource::Dom => {
                if let Some(root) = context.dom.as_ref() {
                    collect_dom(root, &mut collector, 0);
                } else {
                    missing_sources.insert(*source);
                }
            }
            EvidenceSource::Forms => {
                for control in &context.accessibility.interactive {
                    if control.input_type.is_some()
                        || matches!(
                            control.role.as_str(),
                            "checkbox" | "combobox" | "radio" | "textbox"
                        )
                    {
                        collector.push(EvidenceFact {
                            source: *source,
                            kind: "control".into(),
                            role: Some(control.role.clone()),
                            name: Some(control.name.clone()),
                            input_type: control.input_type.clone(),
                            required: Some(control.required),
                            read_only: Some(control.read_only),
                            empty: Some(control.empty),
                            geometry_present: None,
                        });
                    }
                }
            }
            EvidenceSource::Layout => {
                if let Some(root) = context.dom.as_ref() {
                    collect_layout(root, &mut collector, *source, 0);
                } else {
                    missing_sources.insert(*source);
                }
            }
            _ => {
                missing_sources.insert(*source);
            }
        }
    }

    collector.truncated |= context.accessibility.truncated || context.boundaries.truncated;
    let mut evidence = ExtractionEvidence {
        schema_version: EXTRACTION_CONTRACT_SCHEMA_VERSION,
        revision: context.accessibility.revision,
        scope: request.scope.clone(),
        sources: request.sources.clone(),
        facts: collector.facts,
        limits: ExtractionEvidenceLimits {
            truncated: collector.truncated,
            omitted_facts: collector.omitted_facts,
            text_bytes: collector.text_bytes,
            missing_sources: missing_sources.into_iter().collect(),
        },
    };
    trim_to_output_budget(&mut evidence, request.budgets.max_output_bytes)?;
    Ok(evidence)
}

struct EvidenceCollector {
    budgets: ExtractionBudgets,
    facts: Vec<EvidenceFact>,
    omitted_facts: u32,
    text_bytes: u32,
    truncated: bool,
}

impl EvidenceCollector {
    fn new(budgets: ExtractionBudgets) -> Self {
        Self {
            budgets,
            facts: Vec::new(),
            omitted_facts: 0,
            text_bytes: 0,
            truncated: false,
        }
    }

    fn allow_depth(&mut self, depth: u16) -> bool {
        if depth >= self.budgets.max_depth {
            self.omitted_facts = self.omitted_facts.saturating_add(1);
            self.truncated = true;
            false
        } else {
            true
        }
    }

    fn push(&mut self, mut fact: EvidenceFact) {
        if self.facts.len() as u32 >= self.budgets.max_nodes {
            self.omitted_facts = self.omitted_facts.saturating_add(1);
            self.truncated = true;
            return;
        }
        fact.role = self.bound_text(fact.role.take());
        fact.name = self.bound_text(fact.name.take());
        fact.input_type = self.bound_text(fact.input_type.take());
        self.facts.push(fact);
    }

    fn bound_text(&mut self, value: Option<String>) -> Option<String> {
        let value = value?;
        let remaining = self.budgets.max_text_bytes.saturating_sub(self.text_bytes) as usize;
        if remaining == 0 {
            self.truncated = true;
            return None;
        }
        let bounded = truncate_utf8(&value, remaining);
        self.text_bytes = self.text_bytes.saturating_add(bounded.len() as u32);
        if bounded.len() < value.len() {
            self.truncated = true;
        }
        Some(bounded)
    }
}

fn collect_accessibility(node: &CompactAxNode, collector: &mut EvidenceCollector, depth: u16) {
    if !collector.allow_depth(depth) {
        return;
    }
    collector.push(EvidenceFact {
        source: EvidenceSource::Accessibility,
        kind: "node".into(),
        role: Some(node.role.clone()),
        name: Some(node.name.clone()),
        input_type: None,
        required: None,
        read_only: None,
        empty: None,
        geometry_present: None,
    });
    for child in &node.children {
        collect_accessibility(child, collector, depth.saturating_add(1));
    }
}

fn collect_dom(node: &DomNode, collector: &mut EvidenceCollector, depth: u16) {
    if !collector.allow_depth(depth) {
        return;
    }
    collector.push(EvidenceFact {
        source: EvidenceSource::Dom,
        kind: "element".into(),
        role: None,
        name: Some(node.node_name.clone()),
        input_type: None,
        required: None,
        read_only: None,
        empty: None,
        geometry_present: None,
    });
    for child in &node.children {
        collect_dom(child, collector, depth.saturating_add(1));
    }
}

fn collect_layout(
    node: &DomNode,
    collector: &mut EvidenceCollector,
    source: EvidenceSource,
    depth: u16,
) {
    if !collector.allow_depth(depth) {
        return;
    }
    collector.push(EvidenceFact {
        source,
        kind: "geometry".into(),
        role: None,
        name: Some(node.node_name.clone()),
        input_type: None,
        required: None,
        read_only: None,
        empty: None,
        geometry_present: Some(node.bounding_box.is_some()),
    });
    for child in &node.children {
        collect_layout(child, collector, source, depth.saturating_add(1));
    }
}

fn trim_to_output_budget(
    evidence: &mut ExtractionEvidence,
    max_output_bytes: u32,
) -> Result<(), ExtractionContractError> {
    while serde_json::to_vec(evidence)
        .map_err(|error| ExtractionContractError::new("$", error.to_string()))?
        .len()
        > max_output_bytes as usize
    {
        if evidence.facts.pop().is_none() {
            return Err(ExtractionContractError::new(
                "budgets.maxOutputBytes",
                "output budget is too small for extraction metadata",
            ));
        }
        evidence.limits.omitted_facts = evidence.limits.omitted_facts.saturating_add(1);
        evidence.limits.truncated = true;
    }
    Ok(())
}

fn truncate_utf8(value: &str, max_bytes: usize) -> String {
    if value.len() <= max_bytes {
        return value.to_owned();
    }
    value
        .char_indices()
        .take_while(|(index, _)| *index < max_bytes)
        .map(|(_, character)| character)
        .collect()
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
    fn page_context() -> PageContext {
        use crate::browser::dom::{CompactAxNode, CompactInteractiveElement, DomNode};
        use crate::browser::session::{
            CompactAccessibilitySnapshot, ObservationBoundarySummary, ObservationConsistency,
            PageInfo,
        };

        let page = PageInfo {
            url: "https://example.test/form".into(),
            title: "Example form".into(),
            ready_state: "interactive".into(),
            target_id: "target-1".into(),
            frame_id: "frame-1".into(),
        };
        let accessibility = CompactAccessibilitySnapshot {
            page: page.clone(),
            revision: 7,
            roots: vec![CompactAxNode {
                role: "form".into(),
                name: "Example form".into(),
                children: vec![CompactAxNode {
                    role: "textbox".into(),
                    name: "Full name".into(),
                    children: Vec::new(),
                    interactive: true,
                }],
                interactive: false,
            }],
            interactive: vec![CompactInteractiveElement {
                reference: "axr-1-1".into(),
                role: "textbox".into(),
                name: "Full name".into(),
                backend_dom_node_id: 11,
                ancestor_path: Vec::new(),
                shadow_host_path: None,
                input_type: Some("text".into()),
                value: Some("secret-value".into()),
                checked: None,
                selected_option: None,
                empty: true,
                read_only: false,
                required: true,
            }],
            truncated: false,
            omitted_count: 0,
            ranking_applied: false,
            completeness: None,
        };
        PageContext {
            page,
            text: "secret-value".into(),
            dom: Some(DomNode {
                node_id: 1,
                node_name: "HTML".into(),
                node_value: String::new(),
                children: vec![DomNode {
                    node_id: 2,
                    node_name: "INPUT".into(),
                    node_value: String::new(),
                    children: Vec::new(),
                    attributes: vec!["type".into(), "text".into()],
                    bounding_box: Some([0.0, 0.0, 120.0, 24.0]),
                }],
                attributes: Vec::new(),
                bounding_box: Some([0.0, 0.0, 800.0, 600.0]),
            }),
            accessibility,
            consistency: ObservationConsistency {
                consistent: true,
                attempts: 1,
                start_revision: 7,
                end_revision: 7,
                start_mutation_revision: 0,
                end_mutation_revision: 0,
            },
            boundaries: ObservationBoundarySummary::default(),
            incomplete: Vec::new(),
            screenshot: None,
        }
    }

    #[test]
    fn extraction_collects_supported_sources_and_reports_missing_sources() {
        let mut request = request();
        request.sources = vec![
            EvidenceSource::Accessibility,
            EvidenceSource::Dom,
            EvidenceSource::Forms,
            EvidenceSource::Layout,
            EvidenceSource::Navigation,
        ];
        let evidence = extract_page_context(&page_context(), &request).unwrap();
        assert_eq!(evidence.revision, 7);
        assert!(!evidence.facts.is_empty());
        assert_eq!(
            evidence.limits.missing_sources,
            vec![EvidenceSource::Navigation]
        );
        assert!(
            !serde_json::to_string(&evidence)
                .unwrap()
                .contains("secret-value")
        );
    }

    #[test]
    fn extraction_enforces_node_and_depth_budgets() {
        let mut request = request();
        request.sources = vec![EvidenceSource::Dom];
        request.budgets.max_nodes = 1;
        request.budgets.max_depth = 1;
        let evidence = extract_page_context(&page_context(), &request).unwrap();
        assert_eq!(evidence.facts.len(), 1);
        assert!(evidence.limits.truncated);
        assert!(evidence.limits.omitted_facts > 0);
    }

    #[test]
    fn extraction_rejects_unbound_scopes_until_scoped_observation_exists() {
        let mut request = request();
        request.scope = ExtractionScope::Region {
            region_id: "region-main".into(),
        };
        let error = extract_page_context(&page_context(), &request).unwrap_err();
        assert_eq!(error.path, "scope");
    }
}
