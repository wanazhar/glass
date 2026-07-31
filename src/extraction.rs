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

/// Explainable quality class attached to an evidence fact.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum EvidenceQuality {
    Confirmed,
    Strong,
    Partial,
    Inferred,
    Conflicted,
    Opaque,
}

/// Explicit semantic relationship hint carried by bounded evidence.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum EvidenceRelationshipHint {
    Contains,
    Labels,
    Owns,
    Controls,
    NavigatesTo,
    Opens,
    Confirms,
    Cancels,
    Continues,
    Submits,
    HeaderFor,
    CellOf,
    Selects,
    RepeatsAs,
    ScopedTo,
}

/// Coverage summary for one bounded extraction.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EvidenceCoverage {
    pub structural: EvidenceQuality,
    pub semantic: EvidenceQuality,
    pub interactive_entities_observed: u32,
    pub opaque_regions: u32,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub reasons: Vec<String>,
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
    pub coverage: EvidenceCoverage,
}

/// One redacted, source-labelled fact from browser evidence.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EvidenceFact {
    pub source: EvidenceSource,
    pub kind: String,
    pub quality: EvidenceQuality,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Bounded observed accessibility region role for relationship reconciliation.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_role: Option<String>,
    /// Explicit semantic relationship, when a source can prove one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub relationship_hint: Option<EvidenceRelationshipHint>,
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

impl ExtractionEvidence {
    /// Validate every explicit relationship hint against its source contract.
    pub fn validate_relationship_hints(&self) -> Result<(), ExtractionContractError> {
        for (index, fact) in self.facts.iter().enumerate() {
            let path = format!("facts[{index}].relationshipHint");
            fact.validate_relationship_hint(&path)?;
        }
        Ok(())
    }
}

impl EvidenceFact {
    /// Validate one explicit relationship hint and its required parent role.
    pub fn validate_relationship_hint(&self, path: &str) -> Result<(), ExtractionContractError> {
        let Some(hint) = self.relationship_hint else {
            return Ok(());
        };
        if self.parent_role.as_deref().is_none_or(str::is_empty) {
            return Err(ExtractionContractError::new(
                path,
                "relationship hints require a bounded parent role",
            ));
        }
        if !relationship_hint_allowed(self.source, hint) {
            return Err(ExtractionContractError::new(
                path,
                format!(
                    "relationship hint {hint:?} is not supported by source {:?}",
                    self.source
                ),
            ));
        }
        Ok(())
    }
}

fn relationship_hint_allowed(source: EvidenceSource, hint: EvidenceRelationshipHint) -> bool {
    match source {
        EvidenceSource::Accessibility => matches!(
            hint,
            EvidenceRelationshipHint::Contains
                | EvidenceRelationshipHint::Labels
                | EvidenceRelationshipHint::Owns
                | EvidenceRelationshipHint::Controls
                | EvidenceRelationshipHint::Opens
                | EvidenceRelationshipHint::Confirms
                | EvidenceRelationshipHint::Cancels
                | EvidenceRelationshipHint::Continues
                | EvidenceRelationshipHint::Selects
                | EvidenceRelationshipHint::ScopedTo
        ),
        EvidenceSource::Dom => matches!(
            hint,
            EvidenceRelationshipHint::Contains
                | EvidenceRelationshipHint::Labels
                | EvidenceRelationshipHint::Owns
                | EvidenceRelationshipHint::Controls
                | EvidenceRelationshipHint::NavigatesTo
                | EvidenceRelationshipHint::Opens
                | EvidenceRelationshipHint::Confirms
                | EvidenceRelationshipHint::Cancels
                | EvidenceRelationshipHint::Continues
                | EvidenceRelationshipHint::Submits
                | EvidenceRelationshipHint::HeaderFor
                | EvidenceRelationshipHint::CellOf
                | EvidenceRelationshipHint::Selects
                | EvidenceRelationshipHint::RepeatsAs
                | EvidenceRelationshipHint::ScopedTo
        ),
        EvidenceSource::Forms => matches!(
            hint,
            EvidenceRelationshipHint::Contains
                | EvidenceRelationshipHint::Labels
                | EvidenceRelationshipHint::Owns
                | EvidenceRelationshipHint::Controls
                | EvidenceRelationshipHint::Submits
        ),
        EvidenceSource::Layout => matches!(
            hint,
            EvidenceRelationshipHint::Contains | EvidenceRelationshipHint::ScopedTo
        ),
        EvidenceSource::Navigation
        | EvidenceSource::Tables
        | EvidenceSource::Collections
        | EvidenceSource::Dialogs
        | EvidenceSource::Frames
        | EvidenceSource::ShadowDom
        | EvidenceSource::BoundedProbe => false,
    }
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
                        collect_accessibility(root, &mut collector, 0, None);
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
                    if control.input_type.is_some() || is_form_control_role(&control.role) {
                        collector.push(EvidenceFact {
                            source: *source,
                            kind: "control".into(),
                            quality: EvidenceQuality::Strong,
                            role: Some(control.role.clone()),
                            name: Some(control.name.clone()),
                            input_type: control.input_type.clone(),
                            required: Some(control.required),
                            read_only: Some(control.read_only),
                            empty: Some(control.empty),
                            geometry_present: None,
                            parent_role: control
                                .ancestor_path
                                .last()
                                .and_then(|value| value.split(':').next())
                                .filter(|value| !value.is_empty())
                                .map(str::to_owned),
                            relationship_hint: custom_form_control_hint(
                                &control.role,
                                control
                                    .ancestor_path
                                    .last()
                                    .and_then(|value| value.split(':').next()),
                            ),
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
    let missing_source_values: Vec<_> = missing_sources.iter().copied().collect();
    let coverage = evidence_coverage(
        context,
        &request.sources,
        &missing_sources,
        collector.truncated,
    );
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
            missing_sources: missing_source_values,
        },
        coverage,
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
fn evidence_coverage(
    context: &PageContext,
    sources: &[EvidenceSource],
    missing_sources: &BTreeSet<EvidenceSource>,
    truncated: bool,
) -> EvidenceCoverage {
    let requested = |source| sources.contains(&source);
    let available = |source| requested(source) && !missing_sources.contains(&source);
    let structural = if available(EvidenceSource::Dom) && !truncated {
        EvidenceQuality::Strong
    } else if available(EvidenceSource::Dom) || available(EvidenceSource::Accessibility) {
        EvidenceQuality::Partial
    } else {
        EvidenceQuality::Opaque
    };
    let semantic = if available(EvidenceSource::Accessibility) && !truncated {
        EvidenceQuality::Strong
    } else if requested(EvidenceSource::Accessibility) {
        EvidenceQuality::Partial
    } else {
        EvidenceQuality::Opaque
    };
    let opaque_regions = (context.boundaries.child_frames
        + context.boundaries.shadow_roots
        + context.boundaries.canvases) as u32;
    let mut reasons = missing_sources
        .iter()
        .map(|source| format!("missingSource:{source:?}"))
        .collect::<Vec<_>>();
    if truncated {
        reasons.push("budgetTruncated".into());
    }
    if context.boundaries.child_frames > 0 {
        reasons.push("frameBoundary".into());
    }
    if context.boundaries.shadow_roots > 0 {
        reasons.push("shadowBoundary".into());
    }
    if context.boundaries.canvases > 0 {
        reasons.push("canvasBoundary".into());
    }
    reasons.truncate(16);
    EvidenceCoverage {
        structural,
        semantic,
        interactive_entities_observed: context.accessibility.interactive.len() as u32,
        opaque_regions,
        reasons,
    }
}

fn is_form_control_role(role: &str) -> bool {
    matches!(
        role,
        "checkbox"
            | "combobox"
            | "listbox"
            | "radio"
            | "slider"
            | "spinbutton"
            | "switch"
            | "textbox"
    )
}

fn custom_form_control_hint(
    control_role: &str,
    parent_role: Option<&str>,
) -> Option<EvidenceRelationshipHint> {
    if parent_role.is_some_and(|role| role.eq_ignore_ascii_case("form"))
        && matches!(
            control_role,
            "checkbox" | "combobox" | "listbox" | "radio" | "slider" | "spinbutton" | "switch"
        )
    {
        Some(EvidenceRelationshipHint::Controls)
    } else {
        None
    }
}
fn is_region_role(role: &str) -> bool {
    matches!(
        role,
        "article" | "complementary" | "main" | "navigation" | "region" | "search" | "toolbar"
    )
}

fn collect_accessibility(
    node: &CompactAxNode,
    collector: &mut EvidenceCollector,
    depth: u16,
    parent_role: Option<&str>,
) {
    if !collector.allow_depth(depth) {
        return;
    }
    collector.push(EvidenceFact {
        source: EvidenceSource::Accessibility,
        kind: "node".into(),
        quality: EvidenceQuality::Confirmed,
        role: Some(node.role.clone()),
        name: Some(node.name.clone()),
        input_type: None,
        required: None,
        read_only: None,
        empty: None,
        geometry_present: None,
        parent_role: parent_role.map(str::to_owned),
        relationship_hint: None,
    });
    let next_parent_role = if is_region_role(&node.role) {
        Some(node.role.as_str())
    } else {
        parent_role
    };
    for child in &node.children {
        collect_accessibility(child, collector, depth.saturating_add(1), next_parent_role);
    }
}

fn collect_dom(node: &DomNode, collector: &mut EvidenceCollector, depth: u16) {
    if !collector.allow_depth(depth) {
        return;
    }
    collector.push(EvidenceFact {
        source: EvidenceSource::Dom,
        kind: "element".into(),
        quality: EvidenceQuality::Confirmed,
        role: None,
        name: Some(node.node_name.clone()),
        input_type: None,
        required: None,
        read_only: None,
        empty: None,
        geometry_present: None,
        parent_role: None,
        relationship_hint: None,
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
        quality: EvidenceQuality::Strong,
        role: None,
        name: Some(node.node_name.clone()),
        input_type: None,
        required: None,
        read_only: None,
        empty: None,
        geometry_present: Some(node.bounding_box.is_some()),
        parent_role: None,
        relationship_hint: None,
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
        assert_eq!(evidence.coverage.structural, EvidenceQuality::Strong);
        assert_eq!(evidence.coverage.semantic, EvidenceQuality::Strong);
        assert_eq!(
            evidence.facts.first().map(|fact| fact.quality),
            Some(EvidenceQuality::Confirmed)
        );
        assert!(
            evidence
                .coverage
                .reasons
                .iter()
                .any(|reason| reason == "missingSource:Navigation")
        );
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
    fn extraction_emits_controls_hint_for_custom_form_roles() {
        let mut plain_request = request();
        plain_request.sources = vec![EvidenceSource::Forms];
        let plain_evidence = extract_page_context(&page_context(), &plain_request).unwrap();
        assert!(
            plain_evidence
                .facts
                .iter()
                .all(|fact| fact.relationship_hint.is_none())
        );

        let mut context = page_context();
        context.accessibility.interactive[0].role = "combobox".into();
        context.accessibility.interactive[0].ancestor_path = vec!["form:Example form".into()];
        let mut request = request();
        request.sources = vec![EvidenceSource::Forms];
        let evidence = extract_page_context(&context, &request).unwrap();
        let fact = evidence
            .facts
            .iter()
            .find(|fact| fact.source == EvidenceSource::Forms)
            .expect("custom form control should produce a form fact");
        assert_eq!(fact.parent_role.as_deref(), Some("form"));
        assert_eq!(
            fact.relationship_hint,
            Some(EvidenceRelationshipHint::Controls)
        );
    }

    #[test]
    fn extraction_emits_controls_hints_for_extended_custom_form_roles() {
        for role in ["listbox", "slider", "spinbutton", "switch"] {
            let mut context = page_context();
            context.accessibility.interactive[0].role = role.into();
            context.accessibility.interactive[0].ancestor_path = vec!["form:Example form".into()];
            let mut request = request();
            request.sources = vec![EvidenceSource::Forms];
            let evidence = extract_page_context(&context, &request).unwrap();
            let fact = evidence
                .facts
                .iter()
                .find(|fact| fact.source == EvidenceSource::Forms)
                .expect("extended custom form control should produce a form fact");
            assert_eq!(
                fact.relationship_hint,
                Some(EvidenceRelationshipHint::Controls)
            );
        }
    }

    #[test]
    fn extracted_custom_control_hint_reconciles_to_controls_edge() {
        let mut context = page_context();
        context.accessibility.interactive[0].role = "combobox".into();
        context.accessibility.interactive[0].ancestor_path = vec!["form:Example form".into()];
        let evidence = extract_page_context(&context, &request()).unwrap();
        let draft = crate::web_ir::reconcile_evidence(&evidence).unwrap();
        assert_eq!(draft.relationship_hint_diagnostics.len(), 1);
        assert_eq!(
            draft.relationship_hint_diagnostics[0].status,
            crate::web_ir::RelationshipHintDiagnosticStatus::Emitted
        );
        assert!(draft.relationships.iter().any(
            |relationship| relationship.kind == crate::web_ir::DraftRelationshipKind::Controls
        ));
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
    #[test]
    fn extraction_reports_opaque_boundaries_without_claiming_completeness() {
        let mut context = page_context();
        context.boundaries.child_frames = 1;
        let evidence = extract_page_context(&context, &request()).unwrap();
        assert_eq!(evidence.coverage.opaque_regions, 1);
        assert!(
            evidence
                .coverage
                .reasons
                .iter()
                .any(|reason| reason == "frameBoundary")
        );
        assert_ne!(evidence.coverage.semantic, EvidenceQuality::Opaque);
    }
}
