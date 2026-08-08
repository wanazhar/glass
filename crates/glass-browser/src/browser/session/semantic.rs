//! Versioned semantic observation contracts.
//!
//! This module defines the bounded external shape used by the semantic
//! observation engine. It is intentionally separate from [`PageContext`], so
//! existing detailed and raw observation callers remain compatible while the
//! semantic surface is built incrementally.

use super::types::PageContext;
use crate::browser::dom::CompactAxNode;
use serde::{Deserialize, Serialize};
#[cfg(test)]
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

pub const SEMANTIC_OBSERVATION_SCHEMA_VERSION: u32 = 1;
const MAX_REGIONS: usize = 64;
const MAX_EVIDENCE_ITEMS: usize = 8;
const MAX_EVIDENCE_BYTES: usize = 128;
const MAX_ID_BYTES: usize = 128;
const MAX_LABEL_BYTES: usize = 256;
const MAX_TITLE_BYTES: usize = 1_024;
const MAX_URL_BYTES: usize = 2_048;
const MAX_TARGETS: usize = 32;
const MAX_CHANGE_ITEMS: usize = 128;
const MAX_STRUCTURED_RECORDS: usize = 256;
const MAX_STRUCTURED_FIELDS: usize = 32;
const MAX_STRUCTURED_BYTES: usize = 64 * 1024;
const MAX_ROLE_BYTES: usize = 64;
const MAX_VISIBLE_TEXT_BYTES: usize = 8 * 1024;

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

impl SemanticObservationLevel {
    fn includes_targets(self) -> bool {
        matches!(
            self,
            Self::Interactive | Self::Structured | Self::Detailed | Self::Raw
        )
    }

    fn includes_text(self) -> bool {
        matches!(self, Self::Structured | Self::Detailed | Self::Raw)
    }

    fn includes_accessibility(self) -> bool {
        matches!(self, Self::Detailed | Self::Raw)
    }

    fn includes_raw_accessibility(self) -> bool {
        matches!(self, Self::Raw)
    }
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
#[serde(rename_all = "camelCase")]
pub struct SemanticRouteIdentity {
    pub target_id: String,
    pub frame_id: String,
    pub url: String,
}

/// Page-level semantic summary.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
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
#[serde(rename_all = "camelCase")]
pub struct SemanticExpansionHandle {
    pub region_id: String,
    pub revision: u64,
    pub route: SemanticRouteIdentity,
}

/// A bounded, revision-scoped action reference in an interactive observation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SemanticTarget {
    pub reference: String,
    pub role: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input_type: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub disabled: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub read_only: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub required: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub checked: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub empty: Option<bool>,
}

/// One bounded semantic table row or repeated collection item.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SemanticStructuredRecord {
    pub fields: BTreeMap<String, String>,
}

/// A bounded accessibility node included only by detailed semantic levels.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SemanticAccessibilityNode {
    pub role: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub name: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub children: Vec<SemanticAccessibilityNode>,
    #[serde(default, skip_serializing_if = "is_false")]
    pub interactive: bool,
}
/// Kind of bounded semantic entity change between two compatible revisions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SemanticChangeKind {
    Added,
    Removed,
    Updated,
}

/// Region-level change identified by stable semantic IDs where possible.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SemanticRegionChange {
    pub id: String,
    pub kind: SemanticChangeKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub previous_id: Option<String>,
}

/// Target-level change scoped to a semantic region.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SemanticTargetChange {
    pub region_id: String,
    pub target_id: String,
    pub kind: SemanticChangeKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub previous_target_id: Option<String>,
}

/// Conservative advisory mapping between revision-scoped target references.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SemanticContinuity {
    pub previous_reference: String,
    pub current_reference: String,
    pub confidence: SemanticConfidence,
    pub evidence: String,
}

/// Bounded changes between two observations on the same route.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SemanticChangeSet {
    pub from_revision: u64,
    pub to_revision: u64,
    pub route: SemanticRouteIdentity,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub regions: Vec<SemanticRegionChange>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub targets: Vec<SemanticTargetChange>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub continuity: Vec<SemanticContinuity>,
}

/// A bounded semantic region summary.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SemanticRegion {
    pub id: String,
    pub kind: SemanticRegionKind,
    pub label: String,
    pub interactive_count: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub item_count: Option<usize>,
    pub confidence: SemanticConfidence,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub structured_records: Vec<SemanticStructuredRecord>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub evidence: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub targets: Vec<SemanticTarget>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expansion: Option<SemanticExpansionHandle>,
}

/// Explicit bounds and omission metadata for one semantic observation.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SemanticObservationLimits {
    pub truncated: bool,
    pub omitted_regions: usize,
    #[serde(default)]
    pub omitted_targets: usize,
    #[serde(default)]
    pub omitted_structured_records: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub structured_bytes: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub omitted_bytes: Option<usize>,
    /// Number of bytes included in the optional bounded text projection.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text_bytes: Option<usize>,
    /// Whether callers should expand a region instead of relying on text alone.
    #[serde(default, skip_serializing_if = "is_false")]
    pub text_truncated: bool,
    /// Viewport geometry captured alongside the document-level projection.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub viewport: Option<SemanticViewport>,
}

/// Bounded viewport geometry associated with one observation.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SemanticViewport {
    pub scroll_x: f64,
    pub scroll_y: f64,
    pub width: f64,
    pub height: f64,
    pub document_width: f64,
    pub document_height: f64,
}

/// Versioned semantic page model.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SemanticObservation {
    pub schema_version: u32,
    pub revision: u64,
    pub level: SemanticObservationLevel,
    pub route: SemanticRouteIdentity,
    pub page: SemanticPage,
    pub regions: Vec<SemanticRegion>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub accessibility: Option<Vec<SemanticAccessibilityNode>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub raw_accessibility: Option<Vec<SemanticAccessibilityNode>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub changes: Option<SemanticChangeSet>,
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
        let mut anchors = Vec::new();
        for root in &context.accessibility.roots {
            collect_regions(
                root,
                context.accessibility.revision,
                &route,
                &mut regions,
                &mut anchors,
            );
        }
        let mut omitted_structured_records = 0usize;
        if level.includes_text() {
            for region in &mut regions {
                if matches!(
                    region.kind,
                    SemanticRegionKind::Collection | SemanticRegionKind::Table
                ) && let Some(node) = find_region_node(context, region.id.as_str())
                {
                    let available = structured_record_count(node, region.kind);
                    region.structured_records = structured_records_for_region(node, region.kind);
                    omitted_structured_records = omitted_structured_records
                        .saturating_add(available.saturating_sub(region.structured_records.len()));
                }
            }
        }
        let mut retained_records = 0usize;
        let mut structured_bytes = 0usize;
        for region in &mut regions {
            while retained_records.saturating_add(region.structured_records.len())
                > MAX_STRUCTURED_RECORDS
                || structured_bytes.saturating_add(
                    region
                        .structured_records
                        .iter()
                        .filter_map(|record| {
                            serde_json::to_vec(record).ok().map(|bytes| bytes.len())
                        })
                        .sum::<usize>(),
                ) > MAX_STRUCTURED_BYTES
            {
                if region.structured_records.is_empty() {
                    break;
                }
                region.structured_records.pop();
                omitted_structured_records = omitted_structured_records.saturating_add(1);
            }
            retained_records = retained_records.saturating_add(region.structured_records.len());
            structured_bytes = structured_bytes.saturating_add(
                region
                    .structured_records
                    .iter()
                    .filter_map(|record| serde_json::to_vec(record).ok().map(|bytes| bytes.len()))
                    .sum::<usize>(),
            );
        }
        if regions.is_empty() {
            regions.push(SemanticRegion {
                id: "region_main".into(),
                kind: SemanticRegionKind::Unknown,
                label: "Unclassified page content".into(),
                interactive_count: context.accessibility.interactive.len(),
                item_count: None,
                structured_records: Vec::new(),
                confidence: SemanticConfidence::Unknown,
                evidence: vec!["no recognized landmark role".into()],
                targets: Vec::new(),
                expansion: Some(SemanticExpansionHandle {
                    region_id: "region_main".into(),
                    revision: context.accessibility.revision,
                    route: route.clone(),
                }),
            });
            anchors.push(("region_main".into(), None));
        }
        populate_level(
            &mut regions,
            &context.accessibility.interactive,
            &anchors,
            level,
        );
        let (kind, confidence, evidence) = classify_page(context, &regions);
        let bounded_text = level
            .includes_text()
            .then(|| bounded_semantic_text(&context.text, MAX_VISIBLE_TEXT_BYTES));
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
            text: bounded_text.clone(),
            accessibility: level.includes_accessibility().then(|| {
                context
                    .accessibility
                    .roots
                    .iter()
                    .map(to_semantic_node)
                    .collect()
            }),
            raw_accessibility: level.includes_raw_accessibility().then(|| {
                context
                    .accessibility
                    .roots
                    .iter()
                    .map(to_semantic_node)
                    .collect()
            }),
            changes: None,
            limits: SemanticObservationLimits {
                truncated: context.accessibility.truncated
                    || context.accessibility.omitted_count > 0
                    || !context.incomplete.is_empty()
                    || omitted_structured_records > 0
                    || bounded_text
                        .as_deref()
                        .is_some_and(|text| text.ends_with("[truncated]")),
                omitted_regions: 0,
                omitted_targets: context.accessibility.omitted_count,
                omitted_structured_records,
                structured_bytes: level.includes_text().then_some(structured_bytes),
                omitted_bytes: None,
                text_bytes: bounded_text.as_ref().map(String::len),
                text_truncated: bounded_text
                    .as_deref()
                    .is_some_and(|text| text.ends_with("[truncated]")),
                viewport: context
                    .boundaries
                    .viewport
                    .map(|viewport| SemanticViewport {
                        scroll_x: viewport.scroll_x,
                        scroll_y: viewport.scroll_y,
                        width: viewport.width,
                        height: viewport.height,
                        document_width: viewport.document_width,
                        document_height: viewport.document_height,
                    }),
            },
        };
        observation.validate()?;
        Ok(observation)
    }

    /// Build a revision-scoped observation containing one previously named
    /// region. The source context is still classified in full so the region
    /// ID and its target grouping follow the same deterministic rules as the
    /// page-level observation.
    /// Build a revision-scoped observation containing one previously named
    /// region. Large page-level text and accessibility payloads are narrowed
    /// to the selected region before serialization.
    pub fn scoped_region_from_page_context(
        context: &PageContext,
        level: SemanticObservationLevel,
        region_id: &str,
    ) -> Result<Self, SemanticObservationError> {
        let region_node = find_region_node(context, region_id);
        let mut observation = Self::from_page_context(context, level)?;
        let region_index = observation
            .regions
            .iter()
            .position(|region| region.id == region_id)
            .ok_or_else(|| {
                SemanticObservationError::new(
                    "regionId",
                    format!(
                        "region {region_id:?} is not present at revision {}",
                        observation.revision
                    ),
                )
            })?;
        let omitted_regions = observation.regions.len().saturating_sub(1);
        let selected = observation.regions.remove(region_index);
        observation.regions = vec![selected];
        observation.limits.omitted_regions = omitted_regions;
        observation.limits.truncated |= omitted_regions > 0;
        observation.limits.omitted_targets = observation
            .regions
            .first()
            .map(|region| {
                region
                    .interactive_count
                    .saturating_sub(region.targets.len())
            })
            .unwrap_or_default();

        let selected_text = region_node.map(region_node_text);
        observation.text = level
            .includes_text()
            .then(|| selected_text.clone().unwrap_or_default())
            .filter(|text| !text.is_empty());
        observation.limits.text_bytes = observation.text.as_ref().map(String::len);
        observation.limits.text_truncated = observation
            .text
            .as_deref()
            .is_some_and(|text| text.ends_with("[truncated]"));
        observation.limits.omitted_bytes =
            selected_text.map(|text| context.text.len().saturating_sub(text.len()));
        let scoped_structured_bytes = observation
            .regions
            .iter()
            .flat_map(|region| region.structured_records.iter())
            .filter_map(|record| serde_json::to_vec(record).ok())
            .map(|bytes| bytes.len())
            .sum();
        observation.limits.structured_bytes =
            level.includes_text().then_some(scoped_structured_bytes);
        observation.accessibility = level.includes_accessibility().then(|| {
            region_node
                .map(|node| vec![to_semantic_node(node)])
                .unwrap_or_default()
        });
        observation.raw_accessibility = level.includes_raw_accessibility().then(|| {
            region_node
                .map(|node| vec![to_semantic_node(node)])
                .unwrap_or_default()
        });
        observation.validate()?;
        Ok(observation)
    }

    /// Compute bounded changes from an earlier compatible observation.
    pub fn diff_from(
        &self,
        previous: &SemanticObservation,
    ) -> Result<SemanticChangeSet, SemanticObservationError> {
        self.validate()?;
        previous.validate()?;
        if self.route != previous.route {
            return Err(SemanticObservationError::new(
                "route",
                "semantic diff requires the same target, frame, and URL",
            ));
        }
        if self.level != previous.level {
            return Err(SemanticObservationError::new(
                "level",
                "semantic diff requires matching observation levels",
            ));
        }
        if self.revision < previous.revision {
            return Err(SemanticObservationError::new(
                "revision",
                "semantic diff cannot move backwards in revision",
            ));
        }

        let mut regions = Vec::new();
        let mut matched_current = BTreeSet::new();
        for previous_region in &previous.regions {
            let exact = self
                .regions
                .iter()
                .position(|region| region.id == previous_region.id);
            let fallback = exact.or_else(|| {
                let candidates: Vec<usize> = self
                    .regions
                    .iter()
                    .enumerate()
                    .filter(|(index, region)| {
                        !matched_current.contains(index)
                            && region.kind == previous_region.kind
                            && region.label == previous_region.label
                    })
                    .map(|(index, _)| index)
                    .collect();
                (candidates.len() == 1).then_some(candidates[0])
            });
            if let Some(current_index) = fallback {
                matched_current.insert(current_index);
                let current_region = &self.regions[current_index];
                if current_region != previous_region {
                    regions.push(SemanticRegionChange {
                        id: current_region.id.clone(),
                        kind: SemanticChangeKind::Updated,
                        previous_id: (current_region.id != previous_region.id)
                            .then(|| previous_region.id.clone()),
                    });
                }
            } else {
                regions.push(SemanticRegionChange {
                    id: previous_region.id.clone(),
                    kind: SemanticChangeKind::Removed,
                    previous_id: None,
                });
            }
        }
        for (index, current_region) in self.regions.iter().enumerate() {
            if !matched_current.contains(&index)
                && !previous
                    .regions
                    .iter()
                    .any(|region| region.id == current_region.id)
            {
                regions.push(SemanticRegionChange {
                    id: current_region.id.clone(),
                    kind: SemanticChangeKind::Added,
                    previous_id: None,
                });
            }
        }

        let (targets, continuity) = diff_targets(previous, self);
        let changes = SemanticChangeSet {
            from_revision: previous.revision,
            to_revision: self.revision,
            route: self.route.clone(),
            regions,
            targets,
            continuity,
        };
        validate_change_set(&changes)?;
        Ok(changes)
    }

    /// Attach a revision-aware diff to this observation.
    pub fn with_changes_from(
        mut self,
        previous: &SemanticObservation,
    ) -> Result<Self, SemanticObservationError> {
        self.changes = Some(self.diff_from(previous)?);
        self.validate()?;
        Ok(self)
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
        validate_level_payload(self)?;
        if let Some(changes) = &self.changes {
            validate_change_set(changes)?;
            if changes.to_revision != self.revision || changes.route != self.route {
                return Err(SemanticObservationError::new(
                    "changes",
                    "change set does not belong to this observation",
                ));
            }
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
        let mut total_structured_records = 0usize;
        let mut total_structured_bytes = 0usize;
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
            if region.targets.len() > MAX_TARGETS {
                return Err(SemanticObservationError::new(
                    format!("{path}.targets"),
                    format!("contains more than {MAX_TARGETS} targets"),
                ));
            }
            for (target_index, target) in region.targets.iter().enumerate() {
                let target_path = format!("{path}.targets[{target_index}]");
                validate_text(
                    &format!("{target_path}.reference"),
                    &target.reference,
                    MAX_ID_BYTES,
                    false,
                )?;
                validate_text(
                    &format!("{target_path}.role"),
                    &target.role,
                    MAX_ROLE_BYTES,
                    false,
                )?;
                validate_text(
                    &format!("{target_path}.name"),
                    &target.name,
                    MAX_LABEL_BYTES,
                    true,
                )?;
                if let Some(input_type) = &target.input_type {
                    validate_text(
                        &format!("{target_path}.inputType"),
                        input_type,
                        MAX_ROLE_BYTES,
                        false,
                    )?;
                }
            }
            let record_path = format!("{path}.structuredRecords");
            if region.structured_records.len() > MAX_STRUCTURED_RECORDS {
                return Err(SemanticObservationError::new(
                    &record_path,
                    format!("contains more than {MAX_STRUCTURED_RECORDS} records"),
                ));
            }
            for (record_index, record) in region.structured_records.iter().enumerate() {
                let fields_path = format!("{record_path}[{record_index}].fields");
                if record.fields.len() > MAX_STRUCTURED_FIELDS {
                    return Err(SemanticObservationError::new(
                        &fields_path,
                        format!("contains more than {MAX_STRUCTURED_FIELDS} fields"),
                    ));
                }
                for (field_name, field_value) in &record.fields {
                    validate_text(
                        &format!("{fields_path}.{field_name}"),
                        field_name,
                        MAX_LABEL_BYTES,
                        false,
                    )?;
                    validate_text(
                        &format!("{fields_path}.{field_name}"),
                        field_value,
                        MAX_LABEL_BYTES,
                        true,
                    )?;
                }
            }
            total_structured_records =
                total_structured_records.saturating_add(region.structured_records.len());
            total_structured_bytes = total_structured_bytes.saturating_add(
                region
                    .structured_records
                    .iter()
                    .filter_map(|record| serde_json::to_vec(record).ok().map(|bytes| bytes.len()))
                    .sum::<usize>(),
            );
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
        if total_structured_records > MAX_STRUCTURED_RECORDS {
            return Err(SemanticObservationError::new(
                "regions.structuredRecords",
                format!("contains more than {MAX_STRUCTURED_RECORDS} aggregate records"),
            ));
        }
        if total_structured_bytes > MAX_STRUCTURED_BYTES {
            return Err(SemanticObservationError::new(
                "regions.structuredRecords",
                format!("contains more than {MAX_STRUCTURED_BYTES} aggregate bytes"),
            ));
        }
        if self.limits.omitted_structured_records > 0 && !self.limits.truncated {
            return Err(SemanticObservationError::new(
                "limits.truncated",
                "structured record omissions require truncated=true",
            ));
        }
        if let Some(reported) = self.limits.structured_bytes
            && reported != total_structured_bytes
        {
            return Err(SemanticObservationError::new(
                "limits.structuredBytes",
                "structured byte count does not match the serialized records",
            ));
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

fn validate_level_payload(
    observation: &SemanticObservation,
) -> Result<(), SemanticObservationError> {
    let has_targets = observation
        .regions
        .iter()
        .any(|region| !region.targets.is_empty());
    if has_targets && !observation.level.includes_targets() {
        return Err(SemanticObservationError::new(
            "regions.targets",
            "target payload is not available at the summary observation level",
        ));
    }
    if observation.text.is_some() != observation.level.includes_text() {
        return Err(SemanticObservationError::new(
            "text",
            "visible text payload does not match observation level",
        ));
    }
    let has_structured_records = observation
        .regions
        .iter()
        .any(|region| !region.structured_records.is_empty());
    if has_structured_records && !observation.level.includes_text() {
        return Err(SemanticObservationError::new(
            "regions.structuredRecords",
            "structured records are not available at summary or interactive observation levels",
        ));
    }
    if observation.accessibility.is_some() != observation.level.includes_accessibility() {
        return Err(SemanticObservationError::new(
            "accessibility",
            "accessibility payload does not match observation level",
        ));
    }
    if observation.raw_accessibility.is_some() != observation.level.includes_raw_accessibility() {
        return Err(SemanticObservationError::new(
            "rawAccessibility",
            "raw accessibility payload does not match observation level",
        ));
    }
    Ok(())
}

fn validate_change_set(changes: &SemanticChangeSet) -> Result<(), SemanticObservationError> {
    validate_route("changes.route", &changes.route)?;
    if changes.from_revision > changes.to_revision {
        return Err(SemanticObservationError::new(
            "changes.fromRevision",
            "cannot be newer than toRevision",
        ));
    }
    if changes.regions.len() > MAX_CHANGE_ITEMS {
        return Err(SemanticObservationError::new(
            "changes.regions",
            format!("contains more than {MAX_CHANGE_ITEMS} changes"),
        ));
    }
    if changes.targets.len() > MAX_CHANGE_ITEMS {
        return Err(SemanticObservationError::new(
            "changes.targets",
            format!("contains more than {MAX_CHANGE_ITEMS} changes"),
        ));
    }
    if changes.continuity.len() > MAX_CHANGE_ITEMS {
        return Err(SemanticObservationError::new(
            "changes.continuity",
            format!("contains more than {MAX_CHANGE_ITEMS} mappings"),
        ));
    }
    for (index, change) in changes.regions.iter().enumerate() {
        validate_text(
            &format!("changes.regions[{index}].id"),
            &change.id,
            MAX_ID_BYTES,
            false,
        )?;
        if let Some(previous_id) = &change.previous_id {
            validate_text(
                &format!("changes.regions[{index}].previousId"),
                previous_id,
                MAX_ID_BYTES,
                false,
            )?;
        }
    }
    for (index, change) in changes.targets.iter().enumerate() {
        validate_text(
            &format!("changes.targets[{index}].regionId"),
            &change.region_id,
            MAX_ID_BYTES,
            false,
        )?;
        validate_text(
            &format!("changes.targets[{index}].targetId"),
            &change.target_id,
            MAX_ID_BYTES,
            false,
        )?;
        if let Some(previous_target_id) = &change.previous_target_id {
            validate_text(
                &format!("changes.targets[{index}].previousTargetId"),
                previous_target_id,
                MAX_ID_BYTES,
                false,
            )?;
        }
    }
    for (index, continuity) in changes.continuity.iter().enumerate() {
        validate_text(
            &format!("changes.continuity[{index}].previousReference"),
            &continuity.previous_reference,
            MAX_ID_BYTES,
            false,
        )?;
        validate_text(
            &format!("changes.continuity[{index}].currentReference"),
            &continuity.current_reference,
            MAX_ID_BYTES,
            false,
        )?;
        validate_text(
            &format!("changes.continuity[{index}].evidence"),
            &continuity.evidence,
            MAX_EVIDENCE_BYTES,
            false,
        )?;
    }
    Ok(())
}

fn diff_targets(
    previous: &SemanticObservation,
    current: &SemanticObservation,
) -> (Vec<SemanticTargetChange>, Vec<SemanticContinuity>) {
    let mut changes = Vec::new();
    let mut continuity = Vec::new();
    let mut matched_current = BTreeSet::new();
    for previous_region in &previous.regions {
        let Some(current_region) = current
            .regions
            .iter()
            .find(|region| region.id == previous_region.id)
        else {
            for target in &previous_region.targets {
                changes.push(SemanticTargetChange {
                    region_id: previous_region.id.clone(),
                    target_id: target.reference.clone(),
                    kind: SemanticChangeKind::Removed,
                    previous_target_id: None,
                });
            }
            continue;
        };
        for previous_target in &previous_region.targets {
            let exact = current_region
                .targets
                .iter()
                .position(|target| target.reference == previous_target.reference);
            let fallback = exact.or_else(|| {
                let candidates: Vec<usize> = current_region
                    .targets
                    .iter()
                    .enumerate()
                    .filter(|(index, target)| {
                        !matched_current.contains(&(*index, current_region.id.clone()))
                            && target.role == previous_target.role
                            && target.name == previous_target.name
                            && target.input_type == previous_target.input_type
                    })
                    .map(|(index, _)| index)
                    .collect();
                (candidates.len() == 1).then_some(candidates[0])
            });
            if let Some(current_index) = fallback {
                matched_current.insert((current_index, current_region.id.clone()));
                let current_target = &current_region.targets[current_index];
                if current_target != previous_target {
                    changes.push(SemanticTargetChange {
                        region_id: current_region.id.clone(),
                        target_id: current_target.reference.clone(),
                        kind: SemanticChangeKind::Updated,
                        previous_target_id: (current_target.reference != previous_target.reference)
                            .then(|| previous_target.reference.clone()),
                    });
                }
                if current_target.reference != previous_target.reference {
                    continuity.push(SemanticContinuity {
                        previous_reference: previous_target.reference.clone(),
                        current_reference: current_target.reference.clone(),
                        confidence: SemanticConfidence::Medium,
                        evidence: "unique role/name/inputType match".into(),
                    });
                }
            } else {
                changes.push(SemanticTargetChange {
                    region_id: previous_region.id.clone(),
                    target_id: previous_target.reference.clone(),
                    kind: SemanticChangeKind::Removed,
                    previous_target_id: None,
                });
            }
        }
        for (index, current_target) in current_region.targets.iter().enumerate() {
            if !matched_current.contains(&(index, current_region.id.clone()))
                && !previous_region
                    .targets
                    .iter()
                    .any(|target| target.reference == current_target.reference)
            {
                changes.push(SemanticTargetChange {
                    region_id: current_region.id.clone(),
                    target_id: current_target.reference.clone(),
                    kind: SemanticChangeKind::Added,
                    previous_target_id: None,
                });
            }
        }
    }
    for current_region in &current.regions {
        if !previous
            .regions
            .iter()
            .any(|region| region.id == current_region.id)
        {
            for target in &current_region.targets {
                changes.push(SemanticTargetChange {
                    region_id: current_region.id.clone(),
                    target_id: target.reference.clone(),
                    kind: SemanticChangeKind::Added,
                    previous_target_id: None,
                });
            }
        }
    }
    changes.truncate(MAX_CHANGE_ITEMS);
    continuity.truncate(MAX_CHANGE_ITEMS);
    (changes, continuity)
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
fn structured_record_count(node: &CompactAxNode, kind: SemanticRegionKind) -> usize {
    let role = match kind {
        SemanticRegionKind::Table => "row",
        SemanticRegionKind::Collection => "listitem",
        _ => return 0,
    };
    let mut nodes = Vec::new();
    collect_role_nodes(node, role, &mut nodes);
    nodes
        .into_iter()
        .filter(|node| kind != SemanticRegionKind::Table || !contains_role(node, "columnheader"))
        .count()
}

fn structured_records_for_region(
    node: &CompactAxNode,
    kind: SemanticRegionKind,
) -> Vec<SemanticStructuredRecord> {
    match kind {
        SemanticRegionKind::Table => table_records(node),
        SemanticRegionKind::Collection => collection_records(node),
        _ => Vec::new(),
    }
}

fn table_records(node: &CompactAxNode) -> Vec<SemanticStructuredRecord> {
    let mut header_nodes = Vec::new();
    collect_role_nodes(node, "columnheader", &mut header_nodes);
    let headers: Vec<String> = header_nodes
        .into_iter()
        .map(|header| bounded_semantic_text(&header.name, MAX_LABEL_BYTES))
        .collect();
    let mut rows = Vec::new();
    collect_role_nodes(node, "row", &mut rows);
    rows.into_iter()
        .take(MAX_STRUCTURED_RECORDS)
        .filter(|row| !contains_role(row, "columnheader"))
        .filter_map(|row| {
            let mut cells = Vec::new();
            collect_row_cells(row, &mut cells);
            if cells.is_empty() {
                return None;
            }
            let mut fields = BTreeMap::new();
            for (index, cell) in cells.into_iter().take(MAX_STRUCTURED_FIELDS).enumerate() {
                let header = headers
                    .get(index)
                    .filter(|header| !header.is_empty())
                    .cloned()
                    .unwrap_or_else(|| format!("column_{}", index + 1));
                insert_record_field(&mut fields, header, cell);
            }
            Some(SemanticStructuredRecord { fields })
        })
        .collect()
}

fn collection_records(node: &CompactAxNode) -> Vec<SemanticStructuredRecord> {
    let mut items = Vec::new();
    collect_role_nodes(node, "listitem", &mut items);
    items
        .into_iter()
        .take(MAX_STRUCTURED_RECORDS)
        .filter_map(|item| {
            let mut fields = BTreeMap::new();
            if !item.name.is_empty() {
                insert_record_field(
                    &mut fields,
                    "name".into(),
                    bounded_semantic_text(&item.name, MAX_LABEL_BYTES),
                );
            }
            collect_collection_fields(item, &mut fields);
            (!fields.is_empty()).then_some(SemanticStructuredRecord { fields })
        })
        .collect()
}

fn collect_role_nodes<'a>(
    node: &'a CompactAxNode,
    role: &str,
    output: &mut Vec<&'a CompactAxNode>,
) {
    if node.role == role {
        output.push(node);
    }
    for child in &node.children {
        collect_role_nodes(child, role, output);
    }
}

fn collect_row_cells(node: &CompactAxNode, output: &mut Vec<String>) {
    for child in &node.children {
        if matches!(child.role.as_str(), "cell" | "gridcell" | "columnheader") {
            output.push(bounded_semantic_text(&child.name, MAX_LABEL_BYTES));
        } else if child.role != "row" {
            collect_row_cells(child, output);
        }
    }
}

fn collect_collection_fields(node: &CompactAxNode, fields: &mut BTreeMap<String, String>) {
    for child in &node.children {
        let key = match child.role.as_str() {
            "heading" => Some("heading"),
            "paragraph" => Some("description"),
            "link" => Some("link"),
            _ => None,
        };
        if let Some(key) = key {
            if !child.name.is_empty() {
                insert_record_field(
                    fields,
                    key.into(),
                    bounded_semantic_text(&child.name, MAX_LABEL_BYTES),
                );
            }
        } else if child.role != "listitem" {
            collect_collection_fields(child, fields);
        }
        if fields.len() >= MAX_STRUCTURED_FIELDS {
            return;
        }
    }
}

fn contains_role(node: &CompactAxNode, role: &str) -> bool {
    node.role == role || node.children.iter().any(|child| contains_role(child, role))
}

fn insert_record_field(fields: &mut BTreeMap<String, String>, key: String, value: String) {
    if fields.len() >= MAX_STRUCTURED_FIELDS {
        return;
    }
    let base = if key.is_empty() {
        "field".to_string()
    } else {
        key
    };
    let mut candidate = base.clone();
    let mut suffix = 2;
    while fields.contains_key(&candidate) {
        candidate = format!("{base}_{suffix}");
        suffix += 1;
    }
    fields.insert(candidate, value);
}

fn collect_regions(
    node: &CompactAxNode,
    revision: u64,
    route: &SemanticRouteIdentity,
    regions: &mut Vec<SemanticRegion>,
    anchors: &mut Vec<(String, Option<String>)>,
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
        let structured_records = Vec::new();
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
            structured_records,
            confidence: SemanticConfidence::Exact,
            evidence: vec![format!("aria-role={}", node.role)],
            targets: Vec::new(),
            expansion: Some(SemanticExpansionHandle {
                region_id: id.clone(),
                revision,
                route: route.clone(),
            }),
        });
        anchors.push((id, Some(format!("{}:{}", node.role, node.name))));
    }
    for child in &node.children {
        collect_regions(child, revision, route, regions, anchors);
    }
}
fn populate_level(
    regions: &mut [SemanticRegion],
    controls: &[crate::browser::dom::CompactInteractiveElement],
    anchors: &[(String, Option<String>)],
    level: SemanticObservationLevel,
) {
    if !level.includes_targets() || regions.is_empty() {
        return;
    }
    let mut grouped: Vec<Vec<&crate::browser::dom::CompactInteractiveElement>> =
        (0..regions.len()).map(|_| Vec::new()).collect();
    for control in controls {
        let region_index = anchors
            .iter()
            .enumerate()
            .filter(|(_, (_, anchor))| {
                anchor
                    .as_ref()
                    .is_some_and(|anchor| control.ancestor_path.iter().any(|item| item == anchor))
            })
            .map(|(index, _)| index)
            .next_back()
            .unwrap_or_else(|| regions.len().saturating_sub(1));
        if let Some(group) = grouped.get_mut(region_index) {
            group.push(control);
        }
    }

    let active_regions: Vec<usize> = grouped
        .iter()
        .enumerate()
        .filter_map(|(index, controls)| (!controls.is_empty()).then_some(index))
        .collect();
    let reserved = active_regions.len().min(MAX_TARGETS);
    let mut selected = vec![false; controls.len()];
    let mut selected_count = 0;
    // Reserve one document-order target for every region before spending the
    // remaining budget on high-value primary/content regions.
    for (region_index, group) in grouped.iter().enumerate() {
        if selected_count >= reserved || group.is_empty() {
            continue;
        }
        let control = group[0];
        if let Some(global_index) = controls
            .iter()
            .position(|candidate| std::ptr::eq(candidate, control))
        {
            selected[global_index] = true;
            selected_count += 1;
            append_semantic_target(&mut regions[region_index], control);
        }
    }
    let mut priority: Vec<usize> = active_regions
        .into_iter()
        .filter(|index| grouped[*index].len() > 1)
        .collect();
    priority.sort_by_key(|index| (region_target_priority(regions[*index].kind), *index));
    for region_index in priority {
        for control in grouped[region_index].iter().skip(1) {
            if selected_count >= MAX_TARGETS {
                break;
            }
            let Some(global_index) = controls
                .iter()
                .position(|candidate| std::ptr::eq(candidate, *control))
            else {
                continue;
            };
            if selected[global_index] {
                continue;
            }
            selected[global_index] = true;
            selected_count += 1;
            append_semantic_target(&mut regions[region_index], control);
        }
    }
}

fn append_semantic_target(
    region: &mut SemanticRegion,
    control: &crate::browser::dom::CompactInteractiveElement,
) {
    region.targets.push(SemanticTarget {
        reference: control.reference.clone(),
        role: bounded_semantic_text(&control.role, MAX_ROLE_BYTES),
        name: bounded_semantic_text(&control.name, MAX_LABEL_BYTES),
        input_type: control
            .input_type
            .as_deref()
            .map(|value| bounded_semantic_text(value, MAX_ROLE_BYTES)),
        disabled: control.disabled,
        read_only: Some(control.read_only),
        required: Some(control.required),
        checked: control.checked,
        empty: Some(control.empty),
    });
}

fn region_target_priority(kind: SemanticRegionKind) -> u8 {
    match kind {
        SemanticRegionKind::Main
        | SemanticRegionKind::Collection
        | SemanticRegionKind::Results
        | SemanticRegionKind::Article => 0,
        SemanticRegionKind::Navigation
        | SemanticRegionKind::Search
        | SemanticRegionKind::Toolbar => 1,
        _ => 2,
    }
}

fn is_false(value: &bool) -> bool {
    !*value
}

fn to_semantic_node(node: &CompactAxNode) -> SemanticAccessibilityNode {
    SemanticAccessibilityNode {
        role: bounded_semantic_text(&node.role, MAX_ROLE_BYTES),
        name: bounded_semantic_text(&node.name, MAX_LABEL_BYTES),
        children: node.children.iter().map(to_semantic_node).collect(),
        interactive: node.interactive,
    }
}

fn count_interactive(node: &CompactAxNode) -> usize {
    usize::from(node.interactive) + node.children.iter().map(count_interactive).sum::<usize>()
}

pub(crate) fn find_region_node<'a>(
    context: &'a PageContext,
    region_id: &str,
) -> Option<&'a CompactAxNode> {
    if region_id == "region_main" {
        return context.accessibility.roots.first();
    }
    let body = region_id.strip_prefix("region_")?;
    let (kind_name, ordinal) = body.rsplit_once('_')?;
    let ordinal = ordinal.parse::<usize>().ok()?;
    let kind = [
        SemanticRegionKind::Navigation,
        SemanticRegionKind::Main,
        SemanticRegionKind::Search,
        SemanticRegionKind::Form,
        SemanticRegionKind::Dialog,
        SemanticRegionKind::Alert,
        SemanticRegionKind::Status,
        SemanticRegionKind::Toolbar,
        SemanticRegionKind::FilterPanel,
        SemanticRegionKind::Results,
        SemanticRegionKind::Collection,
        SemanticRegionKind::Table,
        SemanticRegionKind::Pagination,
        SemanticRegionKind::Article,
        SemanticRegionKind::Sidebar,
        SemanticRegionKind::CheckoutSummary,
        SemanticRegionKind::Authentication,
        SemanticRegionKind::Footer,
    ]
    .into_iter()
    .find(|kind| region_kind_name(*kind) == kind_name)?;
    let mut seen = 0;
    context
        .accessibility
        .roots
        .iter()
        .find_map(|node| find_region_node_in(node, kind, ordinal, &mut seen))
}

fn find_region_node_in<'a>(
    node: &'a CompactAxNode,
    target_kind: SemanticRegionKind,
    target_ordinal: usize,
    seen: &mut usize,
) -> Option<&'a CompactAxNode> {
    if region_kind(&node.role) == Some(target_kind) {
        *seen += 1;
        if *seen == target_ordinal {
            return Some(node);
        }
    }
    node.children
        .iter()
        .find_map(|child| find_region_node_in(child, target_kind, target_ordinal, seen))
}

fn region_node_text(node: &CompactAxNode) -> String {
    let mut text = String::new();
    append_region_node_text(node, &mut text);
    bounded_semantic_text(&text, MAX_VISIBLE_TEXT_BYTES)
}

fn append_region_node_text(node: &CompactAxNode, output: &mut String) {
    if output.len() >= MAX_VISIBLE_TEXT_BYTES {
        return;
    }
    if !node.name.is_empty() {
        if !output.is_empty() {
            output.push('\n');
        }
        output.push_str(&bounded_semantic_text(
            &node.name,
            MAX_LABEL_BYTES.min(MAX_VISIBLE_TEXT_BYTES.saturating_sub(output.len())),
        ));
    }
    for child in &node.children {
        append_region_node_text(child, output);
    }
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
    let path = url.split(['?', '#']).next().unwrap_or_default();
    let segments: Vec<&str> = path
        .split('/')
        .filter(|segment| !segment.is_empty())
        .collect();
    let collection_segment = segments.iter().enumerate().find(|(_, segment)| {
        matches!(
            **segment,
            "issues" | "pulls" | "discussions" | "search" | "results" | "items"
        )
    });
    let numeric_detail = collection_segment.is_some_and(|(index, _)| {
        segments
            .get(index + 1)
            .is_some_and(|segment| segment.chars().all(|character| character.is_ascii_digit()))
    });
    let repeated_items = regions.iter().any(|region| {
        matches!(
            region.kind,
            SemanticRegionKind::Results
                | SemanticRegionKind::Collection
                | SemanticRegionKind::Table
        ) && region.item_count.is_some_and(|count| count >= 2)
    });
    let lower_text = context.text.to_ascii_lowercase();
    let detail_metadata = ["status", "opened", "assignee", "labels", "comments"]
        .iter()
        .filter(|term| lower_text.contains(**term))
        .count();
    if numeric_detail
        || (context.page.title.trim().len() >= 8
            && (has(SemanticRegionKind::Article) || detail_metadata >= 2))
    {
        return (
            SemanticPageKind::Detail,
            SemanticConfidence::Medium,
            vec!["route-or-detail-metadata-signature".into()],
        );
    }
    if repeated_items || collection_segment.is_some() {
        return (
            SemanticPageKind::Listing,
            SemanticConfidence::Medium,
            vec!["repeated-items-or-collection-route".into()],
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
    let document_signature = url.contains("/doc/html/rfc")
        || title.starts_with("rfc ")
        || title.contains("request for comments")
        || context.text.contains("Status of this Memo")
        || context.text.contains("Table of Contents");
    if document_signature {
        return (
            SemanticPageKind::Documentation,
            SemanticConfidence::High,
            vec!["document-signature=rfc-or-formatted-document".into()],
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

    /// Collect a fresh semantic observation and, unless `fresh_only` is set,
    /// assess stored knowledge against its current route and landmarks.
    ///
    /// The store is read-only for this operation. Assessment never supplies a
    /// target reference or authorizes an action; callers must still use the
    /// current observation and guarded action APIs.
    pub async fn semantic_observe_with_knowledge(
        &self,
        level: SemanticObservationLevel,
        store: &super::KnowledgeStore,
        options: super::KnowledgeLookupOptions,
        fresh_only: bool,
    ) -> super::types::BrowserResult<super::KnowledgeObservationReport> {
        let observation = self.semantic_observe(level).await?;
        if fresh_only {
            return Ok(super::KnowledgeObservationReport {
                observation,
                mode: super::KnowledgeObservationMode::FreshOnly,
                assessments: Vec::new(),
                eligible_record_ids: Vec::new(),
                stale_record_ids: Vec::new(),
                out_of_scope_record_ids: Vec::new(),
            });
        }
        let context = super::KnowledgeLookupContext::from_observation(&observation, options)?;
        let assessments = store.assess(&context);
        let mut eligible_record_ids = Vec::new();
        let mut stale_record_ids = Vec::new();
        let mut out_of_scope_record_ids = Vec::new();
        for assessment in &assessments {
            match assessment.status {
                super::KnowledgeAssessmentStatus::Eligible => {
                    eligible_record_ids.push(assessment.record_id.clone())
                }
                super::KnowledgeAssessmentStatus::Stale => {
                    stale_record_ids.push(assessment.record_id.clone())
                }
                super::KnowledgeAssessmentStatus::OutOfScope => {
                    out_of_scope_record_ids.push(assessment.record_id.clone())
                }
                super::KnowledgeAssessmentStatus::Contradicted
                | super::KnowledgeAssessmentStatus::Quarantined => {}
            }
        }
        Ok(super::KnowledgeObservationReport {
            observation,
            mode: super::KnowledgeObservationMode::Assessed,
            assessments,
            eligible_record_ids,
            stale_record_ids,
            out_of_scope_record_ids,
        })
    }

    /// Expand one semantic region only when its page revision is still current.
    pub async fn semantic_expand_region(
        &self,
        region_id: &str,
        revision: u64,
        level: SemanticObservationLevel,
    ) -> super::types::BrowserResult<SemanticObservation> {
        let context = self.observe_fresh().await?;
        if context.accessibility.revision != revision {
            return Err(SemanticObservationError::new(
                "revision",
                format!(
                    "semantic region revision {revision} is stale; current revision is {}",
                    context.accessibility.revision
                ),
            )
            .into());
        }
        SemanticObservation::scoped_region_from_page_context(&context, level, region_id)
            .map_err(Into::into)
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
                structured_records: Vec::new(),
                confidence: SemanticConfidence::High,
                evidence: vec!["repeated item structure".into()],
                targets: Vec::new(),
                expansion: Some(SemanticExpansionHandle {
                    region_id: "region_results".into(),
                    revision: 42,
                    route: route.clone(),
                }),
            }],
            text: None,
            accessibility: None,
            raw_accessibility: None,
            changes: None,
            limits: SemanticObservationLimits::default(),
            route,
        }
    }

    #[test]
    fn extracts_bounded_table_and_collection_records_from_accessibility_nodes() {
        let table = CompactAxNode {
            role: "table".into(),
            name: "Orders".into(),
            children: vec![
                CompactAxNode {
                    role: "row".into(),
                    name: String::new(),
                    children: vec![
                        CompactAxNode {
                            role: "columnheader".into(),
                            name: "Item".into(),
                            children: Vec::new(),
                            interactive: false,
                        },
                        CompactAxNode {
                            role: "columnheader".into(),
                            name: "Status".into(),
                            children: Vec::new(),
                            interactive: false,
                        },
                    ],
                    interactive: false,
                },
                CompactAxNode {
                    role: "row".into(),
                    name: String::new(),
                    children: vec![
                        CompactAxNode {
                            role: "cell".into(),
                            name: "First order".into(),
                            children: Vec::new(),
                            interactive: false,
                        },
                        CompactAxNode {
                            role: "cell".into(),
                            name: "Ready".into(),
                            children: Vec::new(),
                            interactive: false,
                        },
                    ],
                    interactive: false,
                },
            ],
            interactive: false,
        };
        let rows = table_records(&table);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].fields["Item"], "First order");
        assert_eq!(rows[0].fields["Status"], "Ready");

        let collection = CompactAxNode {
            role: "list".into(),
            name: "Results".into(),
            children: vec![CompactAxNode {
                role: "listitem".into(),
                name: "Glass".into(),
                children: vec![
                    CompactAxNode {
                        role: "heading".into(),
                        name: "Glass".into(),
                        children: Vec::new(),
                        interactive: false,
                    },
                    CompactAxNode {
                        role: "link".into(),
                        name: "Open result".into(),
                        children: Vec::new(),
                        interactive: true,
                    },
                ],
                interactive: false,
            }],
            interactive: false,
        };
        let items = collection_records(&collection);
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].fields["name"], "Glass");
        assert_eq!(items[0].fields["heading"], "Glass");
        assert_eq!(items[0].fields["link"], "Open result");
    }

    #[test]
    fn rejects_unbounded_structured_records_and_fields() {
        let mut too_many = observation();
        too_many.regions[0].structured_records = (0..=MAX_STRUCTURED_RECORDS)
            .map(|_| SemanticStructuredRecord {
                fields: BTreeMap::from([("name".into(), "value".into())]),
            })
            .collect();
        let error = too_many.validate().unwrap_err();
        assert_eq!(error.path, "regions.structuredRecords");

        let mut too_many_fields = observation();
        too_many_fields.regions[0].structured_records = vec![SemanticStructuredRecord {
            fields: (0..=MAX_STRUCTURED_FIELDS)
                .map(|index| (format!("field_{index}"), "value".into()))
                .collect(),
        }];
        let error = too_many_fields.validate().unwrap_err();
        assert_eq!(error.path, "regions.structuredRecords");

        let mut oversized_value = observation();
        oversized_value.regions[0].structured_records = vec![SemanticStructuredRecord {
            fields: BTreeMap::from([("name".into(), "x".repeat(MAX_LABEL_BYTES + 1))]),
        }];
        let error = oversized_value.validate().unwrap_err();
        assert_eq!(error.path, "regions.structuredRecords");
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
    fn accepts_additive_unknown_response_fields() {
        let mut value: Value = serde_json::to_value(observation()).unwrap();
        value["futureField"] = Value::Bool(true);
        let decoded = SemanticObservation::from_json(&value.to_string()).unwrap();
        decoded.validate().unwrap();
    }

    #[test]
    fn additive_response_fixture_remains_compatible() {
        let observation = SemanticObservation::from_json(include_str!(
            "../../../tests/fixtures/semantic-additive-v1.json"
        ))
        .unwrap();
        observation.validate().unwrap();
        assert_eq!(observation.revision, 7);
    }

    #[test]
    fn computes_revision_changes_and_conservative_target_continuity() {
        let mut previous = observation();
        previous.level = SemanticObservationLevel::Interactive;
        previous.regions[0].targets.push(SemanticTarget {
            reference: "axr-42-9".into(),
            role: "button".into(),
            name: "Continue".into(),
            input_type: None,
            disabled: None,
            read_only: None,
            required: None,
            checked: None,
            empty: None,
        });

        let mut current = previous.clone();
        current.revision = 43;
        current.regions[0].expansion.as_mut().unwrap().revision = 43;
        current.regions[0].targets[0].reference = "axr-43-9".into();
        current.changes = None;

        let changes = current.diff_from(&previous).unwrap();
        assert_eq!(changes.from_revision, 42);
        assert_eq!(changes.to_revision, 43);
        assert_eq!(changes.targets.len(), 1);
        assert_eq!(changes.targets[0].kind, SemanticChangeKind::Updated);
        assert_eq!(changes.continuity.len(), 1);
        assert_eq!(changes.continuity[0].confidence, SemanticConfidence::Medium);

        let enriched = current.with_changes_from(&previous).unwrap();
        assert!(enriched.changes.is_some());
        assert!(SemanticObservation::from_json(&enriched.to_canonical_json().unwrap()).is_ok());
    }

    #[test]
    fn rejects_backwards_or_cross_route_semantic_diffs() {
        let previous = observation();
        let mut current = observation();
        current.revision = 41;
        current.regions[0].expansion.as_mut().unwrap().revision = 41;
        let error = current.diff_from(&previous).unwrap_err();
        assert_eq!(error.path, "revision");

        current.revision = 42;
        current.regions[0].expansion.as_mut().unwrap().revision = 42;
        current.route.url = "https://other.test".into();
        current.page.url = "https://other.test".into();
        current.regions[0].expansion.as_mut().unwrap().route = current.route.clone();
        let error = current.diff_from(&previous).unwrap_err();
        assert_eq!(error.path, "route");
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
        let mut context = PageContext {
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
        context
            .accessibility
            .interactive
            .push(crate::browser::dom::CompactInteractiveElement {
                reference: "axr-7-9".into(),
                role: "textbox".into(),
                name: "Query".into(),
                backend_dom_node_id: 9,
                ancestor_path: vec!["search:Site search".into()],
                shadow_host_path: None,
                input_type: Some("search".into()),
                autocomplete: None,
                value: None,
                checked: None,
                disabled: None,
                selected_option: None,
                empty: false,
                read_only: false,
                required: false,
            });
        let semantic =
            SemanticObservation::from_page_context(&context, SemanticObservationLevel::Summary)
                .unwrap();
        assert_eq!(semantic.page.kind, SemanticPageKind::SearchResults);
        assert_eq!(semantic.regions[0].id, "region_search_1");
        assert_eq!(semantic.regions[0].interactive_count, 1);
        assert!(semantic.regions[0].targets.is_empty());
        assert!(!semantic.limits.truncated);
        let mut listing_context = context.clone();
        listing_context.page.url = "https://github.example/issues".into();
        listing_context.page.title = "Issues".into();
        listing_context.text = "Issue one\nIssue two".into();
        listing_context.accessibility.roots = vec![CompactAxNode {
            role: "list".into(),
            name: "Issues".into(),
            children: vec![
                CompactAxNode {
                    role: "listitem".into(),
                    name: "Issue one".into(),
                    children: Vec::new(),
                    interactive: false,
                },
                CompactAxNode {
                    role: "listitem".into(),
                    name: "Issue two".into(),
                    children: Vec::new(),
                    interactive: false,
                },
            ],
            interactive: false,
        }];
        let listing = SemanticObservation::from_page_context(
            &listing_context,
            SemanticObservationLevel::Structured,
        )
        .unwrap();
        assert_eq!(listing.page.kind, SemanticPageKind::Listing);
        assert_eq!(listing.page.confidence, SemanticConfidence::Medium);
        assert!(
            listing
                .page
                .evidence
                .iter()
                .any(|item| item.contains("collection"))
        );

        listing_context.page.url = "https://github.example/issues/42109".into();
        listing_context.page.title = "Issue 42109".into();
        listing_context.text = "Status Open Labels Comments".into();
        let detail = SemanticObservation::from_page_context(
            &listing_context,
            SemanticObservationLevel::Structured,
        )
        .unwrap();
        assert_eq!(detail.page.kind, SemanticPageKind::Detail);
        assert_eq!(detail.page.confidence, SemanticConfidence::Medium);

        let interactive =
            SemanticObservation::from_page_context(&context, SemanticObservationLevel::Interactive)
                .unwrap();
        assert_eq!(interactive.regions[0].targets[0].reference, "axr-7-9");
        assert!(interactive.text.is_none());

        let detailed =
            SemanticObservation::from_page_context(&context, SemanticObservationLevel::Detailed)
                .unwrap();
        assert_eq!(detailed.text.as_deref(), Some("results"));
        assert!(detailed.accessibility.is_some());
        assert!(detailed.raw_accessibility.is_none());

        let scoped = SemanticObservation::scoped_region_from_page_context(
            &context,
            SemanticObservationLevel::Detailed,
            "region_search_1",
        )
        .unwrap();
        assert_eq!(scoped.regions.len(), 1);
        assert_eq!(scoped.limits.omitted_regions, 0);
        assert_eq!(scoped.text.as_deref(), Some("Site search\nQuery"));
        assert_eq!(
            scoped.accessibility.as_ref().unwrap()[0].name,
            "Site search"
        );
        let mut document_context = context.clone();
        document_context.page.url = "https://datatracker.ietf.org/doc/html/rfc2606".into();
        document_context.page.title = "RFC 2606 - Reserved Top Level DNS Names".into();
        document_context.text = format!("Status of this Memo\n{}", "x".repeat(9_000));
        document_context.accessibility.roots.clear();
        document_context.accessibility.interactive.clear();
        document_context.boundaries.viewport = Some(super::super::types::ViewportState {
            scroll_x: 0.0,
            scroll_y: 500.0,
            width: 780.0,
            height: 437.0,
            document_width: 780.0,
            document_height: 4_125.0,
        });
        let document = SemanticObservation::from_page_context(
            &document_context,
            SemanticObservationLevel::Structured,
        )
        .unwrap();
        assert_eq!(document.page.kind, SemanticPageKind::Documentation);
        assert_eq!(document.page.confidence, SemanticConfidence::High);
        assert!(document.limits.text_truncated);
        assert_eq!(document.limits.viewport.unwrap().scroll_y, 500.0);
    }
    #[test]
    fn summary_observations_reject_structured_record_payloads() {
        let mut summary = observation();
        summary.regions[0]
            .structured_records
            .push(SemanticStructuredRecord {
                fields: BTreeMap::from([("name".into(), "secret".into())]),
            });
        let error = summary.validate().unwrap_err();
        assert_eq!(error.path, "regions.structuredRecords");
    }
}
