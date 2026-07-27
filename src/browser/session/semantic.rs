//! Versioned semantic observation contracts.
//!
//! This module defines the bounded external shape used by the semantic
//! observation engine. It is intentionally separate from [`PageContext`], so
//! existing detailed and raw observation callers remain compatible while the
//! semantic surface is built incrementally.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeSet;

use super::types::PageContext;
use crate::browser::dom::CompactAxNode;

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
    /// Build a deterministic semantic summary from an existing fresh page
    /// observation. Classification is advisory and never replaces revisioned
    /// interactive references from the source observation.
    pub fn from_page_context(
        context: &PageContext,
        level: SemanticObservationLevel,
    ) -> Result<Self, SemanticObservationError> {
        let route = SemanticRouteIdentity {
            target_id: context.page.target_id.clone(),
            frame_id: context.page.frame_id.clone(),
            url: context.page.url.clone(),
        };
        let mut regions = Vec::new();
        for root in &context.accessibility.roots {
            collect_regions(root, context.accessibility.revision, &route, &mut regions);
        }
        if regions.is_empty() {
            regions.push(SemanticRegion {
                id: "region_main".into(),
                kind: SemanticRegionKind::Unknown,
                label: "Unclassified page content".into(),
                interactive_count: context.accessibility.interactive.len(),
                item_count: None,
                confidence: SemanticConfidence::Unknown,
                evidence: vec!["no recognized landmark role".into()],
                expansion: Some(SemanticExpansionHandle {
                    region_id: "region_main".into(),
                    revision: context.accessibility.revision,
                    route: route.clone(),
                }),
            });
        }
        let (kind, confidence, evidence) = classify_page(context, &regions);
        let observation = Self {
            schema_version: SEMANTIC_OBSERVATION_SCHEMA_VERSION,
            revision: context.accessibility.revision,
            level,
            route: route.clone(),
            page: SemanticPage {
                kind,
                title: context.page.title.clone(),
                url: context.page.url.clone(),
                target_id: context.page.target_id.clone(),
                frame_id: context.page.frame_id.clone(),
                confidence,
                evidence,
            },
            regions,
            changes: None,
            limits: SemanticObservationLimits {
                truncated: context.accessibility.truncated
                    || context.accessibility.omitted_count > 0
                    || !context.incomplete.is_empty(),
                omitted_regions: 0,
                omitted_targets: context.accessibility.omitted_count,
                omitted_bytes: None,
            },
        };
        observation.validate()?;
        Ok(observation)
    }

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
        let requirement = if allow_empty {
            format!("at most {maximum} bytes")
        } else {
            format!("non-empty and at most {maximum} bytes")
        };
        return Err(SemanticObservationError::new(
            path,
            format!("must be {requirement}"),
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

fn collect_regions(
    node: &CompactAxNode,
    revision: u64,
    route: &SemanticRouteIdentity,
    regions: &mut Vec<SemanticRegion>,
) {
    if let Some(kind) = region_kind(&node.role) {
        let ordinal = regions.iter().filter(|region| region.kind == kind).count() + 1;
        let id = format!("region_{}_{}", region_kind_name(kind), ordinal);
        let item_count = match kind {
            SemanticRegionKind::Results
            | SemanticRegionKind::Collection
            | SemanticRegionKind::Table => Some(
                node.children
                    .iter()
                    .filter(|child| matches!(child.role.as_str(), "listitem" | "row" | "option"))
                    .count(),
            ),
            _ => None,
        };
        regions.push(SemanticRegion {
            id: id.clone(),
            kind,
            label: if node.name.is_empty() {
                region_kind_label(kind).into()
            } else {
                bounded_semantic_text(&node.name, MAX_LABEL_BYTES)
            },
            interactive_count: count_interactive(node),
            item_count,
            confidence: SemanticConfidence::Exact,
            evidence: vec![format!("aria-role={}", node.role)],
            expansion: Some(SemanticExpansionHandle {
                region_id: id,
                revision,
                route: route.clone(),
            }),
        });
    }
    for child in &node.children {
        collect_regions(child, revision, route, regions);
    }
}

fn count_interactive(node: &CompactAxNode) -> usize {
    usize::from(node.interactive) + node.children.iter().map(count_interactive).sum::<usize>()
}

fn region_kind(role: &str) -> Option<SemanticRegionKind> {
    Some(match role {
        "navigation" => SemanticRegionKind::Navigation,
        "main" => SemanticRegionKind::Main,
        "search" => SemanticRegionKind::Search,
        "form" => SemanticRegionKind::Form,
        "dialog" => SemanticRegionKind::Dialog,
        "alert" => SemanticRegionKind::Alert,
        "status" => SemanticRegionKind::Status,
        "toolbar" => SemanticRegionKind::Toolbar,
        "complementary" => SemanticRegionKind::Sidebar,
        "article" => SemanticRegionKind::Article,
        "contentinfo" => SemanticRegionKind::Footer,
        "table" => SemanticRegionKind::Table,
        "list" => SemanticRegionKind::Collection,
        _ => return None,
    })
}

fn region_kind_name(kind: SemanticRegionKind) -> &'static str {
    match kind {
        SemanticRegionKind::Navigation => "navigation",
        SemanticRegionKind::Main => "main",
        SemanticRegionKind::Search => "search",
        SemanticRegionKind::Form => "form",
        SemanticRegionKind::Dialog => "dialog",
        SemanticRegionKind::Alert => "alert",
        SemanticRegionKind::Status => "status",
        SemanticRegionKind::Toolbar => "toolbar",
        SemanticRegionKind::FilterPanel => "filter_panel",
        SemanticRegionKind::Results => "results",
        SemanticRegionKind::Collection => "collection",
        SemanticRegionKind::Table => "table",
        SemanticRegionKind::Pagination => "pagination",
        SemanticRegionKind::Article => "article",
        SemanticRegionKind::Sidebar => "sidebar",
        SemanticRegionKind::CheckoutSummary => "checkout_summary",
        SemanticRegionKind::Authentication => "authentication",
        SemanticRegionKind::Footer => "footer",
        SemanticRegionKind::Unknown => "unknown",
    }
}

fn region_kind_label(kind: SemanticRegionKind) -> &'static str {
    match kind {
        SemanticRegionKind::Navigation => "Navigation",
        SemanticRegionKind::Main => "Main content",
        SemanticRegionKind::Search => "Search",
        SemanticRegionKind::Form => "Form",
        SemanticRegionKind::Dialog => "Dialog",
        SemanticRegionKind::Alert => "Alert",
        SemanticRegionKind::Status => "Status",
        SemanticRegionKind::Toolbar => "Toolbar",
        SemanticRegionKind::Sidebar => "Sidebar",
        SemanticRegionKind::Article => "Article",
        SemanticRegionKind::Footer => "Footer",
        SemanticRegionKind::Table => "Table",
        SemanticRegionKind::Collection => "Collection",
        _ => "Page region",
    }
}

fn classify_page(
    context: &PageContext,
    regions: &[SemanticRegion],
) -> (SemanticPageKind, SemanticConfidence, Vec<String>) {
    let title = context.page.title.to_ascii_lowercase();
    let url = context.page.url.to_ascii_lowercase();
    let has = |kind| regions.iter().any(|region| region.kind == kind);
    if title.contains("sign in")
        || title.contains("log in")
        || url.contains("/login")
        || url.contains("/signin")
        || url.contains("/auth")
    {
        return (
            SemanticPageKind::Authentication,
            SemanticConfidence::High,
            vec!["title-or-url-signature=authentication".into()],
        );
    }
    if has(SemanticRegionKind::Search) {
        let kind = if url.contains("search") || title.contains("search") {
            SemanticPageKind::SearchResults
        } else {
            SemanticPageKind::Search
        };
        return (
            kind,
            SemanticConfidence::High,
            vec!["aria-role=search".into()],
        );
    }
    if has(SemanticRegionKind::Form) {
        return (
            SemanticPageKind::Form,
            SemanticConfidence::High,
            vec!["aria-role=form".into()],
        );
    }
    if has(SemanticRegionKind::Article) {
        return (
            SemanticPageKind::Article,
            SemanticConfidence::High,
            vec!["aria-role=article".into()],
        );
    }
    (
        SemanticPageKind::Generic,
        SemanticConfidence::Unknown,
        vec!["no-high-confidence-page-signature".into()],
    )
}

fn bounded_semantic_text(value: &str, maximum: usize) -> String {
    if value.len() <= maximum {
        return value.to_string();
    }
    let mut end = maximum.saturating_sub(15);
    while end > 0 && !value.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}…[truncated]", &value[..end])
}

impl super::BrowserSession {
    /// Collect fresh, bounded semantic page structure from browser evidence.
    pub async fn semantic_observe(
        &self,
        level: SemanticObservationLevel,
    ) -> super::types::BrowserResult<SemanticObservation> {
        let context = self.observe_fresh().await?;
        SemanticObservation::from_page_context(&context, level).map_err(Into::into)
    }
}

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

    #[test]
    fn classifies_landmarks_without_dropping_interactive_counts() {
        let page = super::super::types::PageInfo {
            url: "https://example.test/search".into(),
            title: "Search".into(),
            ready_state: "complete".into(),
            target_id: "target".into(),
            frame_id: "frame".into(),
        };
        let context = PageContext {
            page,
            text: "results".into(),
            dom: None,
            accessibility: super::super::types::CompactAccessibilitySnapshot {
                page: super::super::types::PageInfo {
                    url: "https://example.test/search".into(),
                    title: "Search".into(),
                    ready_state: "complete".into(),
                    target_id: "target".into(),
                    frame_id: "frame".into(),
                },
                revision: 7,
                roots: vec![CompactAxNode {
                    role: "search".into(),
                    name: "Site search".into(),
                    children: vec![CompactAxNode {
                        role: "textbox".into(),
                        name: "Query".into(),
                        children: Vec::new(),
                        interactive: true,
                    }],
                    interactive: false,
                }],
                interactive: Vec::new(),
                truncated: false,
                omitted_count: 0,
                ranking_applied: false,
                completeness: None,
            },
            consistency: super::super::types::ObservationConsistency {
                consistent: true,
                attempts: 1,
                start_revision: 7,
                end_revision: 7,
                start_mutation_revision: 0,
                end_mutation_revision: 0,
            },
            boundaries: Default::default(),
            incomplete: Vec::new(),
            screenshot: None,
        };
        let semantic =
            SemanticObservation::from_page_context(&context, SemanticObservationLevel::Summary)
                .unwrap();
        assert_eq!(semantic.page.kind, SemanticPageKind::SearchResults);
        assert_eq!(semantic.regions[0].id, "region_search_1");
        assert_eq!(semantic.regions[0].interactive_count, 1);
        assert!(!semantic.limits.truncated);
    }
}
