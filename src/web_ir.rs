//! Stable, bounded Glass Web IR v1 reconciliation and validation.
//!
//! The module turns deterministic browser evidence into canonical semantic
//! entities. It never dispatches browser actions, contains no CDP identifiers,
//! and preserves explicit uncertainty, coverage, and resource limits.

use crate::extraction::{
    EvidenceFact, EvidenceQuality, EvidenceRelationshipHint, EvidenceSource, ExtractionEvidence,
    ExtractionEvidenceLimits, ExtractionScope,
};
use crate::surfaces::SurfaceSet;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt::{Display, Formatter};

/// Version of the stable Glass Web IR v1 contract.
pub const WEB_IR_SCHEMA_VERSION: u32 = 1;
const MAX_WEB_IR_ENTITIES: usize = 4_096;
const MAX_WEB_IR_RELATIONSHIPS: usize = 8_192;
const MAX_WEB_IR_BYTES: usize = 256 * 1024;
const MAX_WEB_IR_DIAGNOSTICS: usize = 128;
const MAX_WEB_IR_DIAGNOSTIC_BYTES: usize = 512;
const MAX_WEB_IR_ENTITY_TEXT_BYTES: usize = 256;
const MAX_WEB_IR_ACTIONS: usize = 16;

/// Canonical entity kinds used by Glass Web IR v1.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum WebIrEntityKind {
    Page,
    Region,
    Form,
    Field,
    Action,
    Link,
    NavigationItem,
    Tab,
    Table,
    Row,
    Cell,
    Collection,
    CollectionItem,
    Dialog,
    PaginationControl,
    Frame,
    ShadowRoot,
    Probe,
    Text,
    UnknownInteractive,
    OpaqueRegion,
}

/// Canonical relationship kinds used by Glass Web IR v1.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum WebIrRelationshipKind {
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

/// One deterministic action supported by a semantic entity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum WebIrAction {
    Read,
    Click,
    Type,
    Select,
    Check,
    Uncheck,
    Submit,
    Open,
    Close,
    Confirm,
    Cancel,
    Navigate,
    Extract,
    Paginate,
}

/// Semantic sensitivity carried independently from page values.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum WebIrSensitivity {
    Public,
    Personal,
    Secret,
    Financial,
    #[default]
    Unknown,
}

/// Browser scope containing an entity.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum WebIrScopeKind {
    #[default]
    Document,
    Region,
    Frame,
    ShadowRoot,
}

/// State known for one semantic entity. `None` means no confirming evidence.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WebIrEntityState {
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub visible: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hit_testable: Option<bool>,
}

/// Bounded semantic metadata keyed by a revision-local entity ID.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WebIrEntityDetails {
    #[serde(default)]
    pub state: WebIrEntityState,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub supported_actions: Vec<WebIrAction>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub region_id: Option<String>,
    #[serde(default)]
    pub scope: WebIrScopeKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scope_id: Option<String>,
    #[serde(default)]
    pub sensitivity: WebIrSensitivity,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub semantic_stability_key: Option<String>,
    #[serde(default)]
    pub truncated: bool,
}

/// Bounded document metadata associated with one IR revision.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WebIrDocument {
    pub revision: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ready_state: Option<String>,
}

/// One canonical entity reconciled from one or more evidence sources.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WebIrEntity {
    pub id: String,
    pub kind: WebIrEntityKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    pub quality: EvidenceQuality,
    pub evidence_sources: Vec<EvidenceSource>,
}

/// One relationship between canonical Web IR entities.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WebIrRelationship {
    pub from: String,
    pub to: String,
    pub kind: WebIrRelationshipKind,
}

/// Kind of change represented by a Web IR revision diff.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum WebIrChangeKind {
    Added,
    Removed,
    Changed,
}

/// One entity change between two validated Web IR revisions.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WebIrEntityChange {
    pub id: String,
    pub kind: WebIrChangeKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub before: Option<WebIrEntity>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub after: Option<WebIrEntity>,
}

/// One relationship addition or removal between two validated Web IR revisions.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WebIrRelationshipChange {
    pub relationship: WebIrRelationship,
    pub kind: WebIrChangeKind,
}

/// Deterministic changes between two validated Web IR revisions.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GlassWebIrDiff {
    pub schema_version: u32,
    pub from_revision: u64,
    pub to_revision: u64,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub entity_changes: Vec<WebIrEntityChange>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub relationship_changes: Vec<WebIrRelationshipChange>,
    pub coverage_changed: bool,
    pub limits_changed: bool,
    pub diagnostics_changed: bool,
    pub relationship_hint_diagnostics_changed: bool,
}

/// Continuity classification for one entity across Web IR revisions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum WebIrEntityContinuityStatus {
    Unchanged,
    Changed,
    Rebound,
    Removed,
    Ambiguous,
}

/// Explain whether a revision-local entity remains safe to use.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WebIrEntityContinuity {
    pub requested_id: String,
    pub status: WebIrEntityContinuityStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current_id: Option<String>,
    pub reason: String,
}

impl WebIrEntity {
    /// Return a bounded semantic key suitable only for revision comparison.
    pub fn semantic_identity_key(&self) -> Option<String> {
        if self.role.is_none() && self.name.is_none() {
            return None;
        }
        Some(format!(
            "{}|{}|{}",
            kind_name(self.kind),
            self.role
                .as_deref()
                .unwrap_or_default()
                .to_ascii_lowercase(),
            self.name
                .as_deref()
                .unwrap_or_default()
                .to_ascii_lowercase()
        ))
    }
}

/// Outcome of source-level relationship-hint validation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum RelationshipHintDiagnosticStatus {
    Validated,
    Emitted,
    UnmatchedParent,
}

/// A redacted diagnostic for one validated relationship hint.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WebIrRelationshipHintDiagnostic {
    pub fact_index: usize,
    pub source: EvidenceSource,
    pub hint: EvidenceRelationshipHint,
    pub parent_role: String,
    pub status: RelationshipHintDiagnosticStatus,
}

/// The stable, bounded Glass Web IR v1 observed contract.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GlassWebIrV1 {
    pub schema_version: u32,
    pub revision: u64,
    #[serde(default)]
    pub document: WebIrDocument,
    pub entities: Vec<WebIrEntity>,
    pub relationships: Vec<WebIrRelationship>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub entity_details: BTreeMap<String, WebIrEntityDetails>,
    pub coverage: crate::extraction::EvidenceCoverage,
    pub limits: ExtractionEvidenceLimits,
    /// Validated surface provenance retained alongside semantic entities.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub surface_set: Option<SurfaceSet>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub diagnostics: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub relationship_hint_diagnostics: Vec<WebIrRelationshipHintDiagnostic>,
}

/// Minimum graph shape expected from one representative fixture.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct WebIrFixtureExpectation {
    pub required_entity_counts: BTreeMap<WebIrEntityKind, u32>,
    pub required_relationship_counts: BTreeMap<WebIrRelationshipKind, u32>,
    pub opaque_regions: u32,
}

impl WebIrEntityKind {
    /// Parse the stable fixture vocabulary into a Web IR entity kind.
    pub fn from_contract_name(name: &str) -> Option<Self> {
        Some(match name {
            "page" => Self::Page,
            "region" => Self::Region,
            "form" => Self::Form,
            "field" => Self::Field,
            "action" => Self::Action,
            "link" => Self::Link,
            "navigationItem" => Self::NavigationItem,
            "tab" => Self::Tab,
            "table" => Self::Table,
            "row" => Self::Row,
            "cell" => Self::Cell,
            "collection" => Self::Collection,
            "collectionItem" => Self::CollectionItem,
            "dialog" => Self::Dialog,
            "paginationControl" => Self::PaginationControl,
            "frame" => Self::Frame,
            "shadowRoot" => Self::ShadowRoot,
            "probe" => Self::Probe,
            "text" => Self::Text,
            "unknownInteractive" => Self::UnknownInteractive,
            "opaqueRegion" => Self::OpaqueRegion,
            _ => return None,
        })
    }
}

impl WebIrRelationshipKind {
    /// Parse the stable fixture vocabulary into a Web IR relationship kind.
    pub fn from_contract_name(name: &str) -> Option<Self> {
        Some(match name {
            "contains" => Self::Contains,
            "labels" => Self::Labels,
            "owns" => Self::Owns,
            "controls" => Self::Controls,
            "navigatesTo" => Self::NavigatesTo,
            "opens" => Self::Opens,
            "confirms" => Self::Confirms,
            "cancels" => Self::Cancels,
            "continues" => Self::Continues,
            "submits" => Self::Submits,
            "headerFor" => Self::HeaderFor,
            "cellOf" => Self::CellOf,
            "selects" => Self::Selects,
            "repeatsAs" => Self::RepeatsAs,
            "scopedTo" => Self::ScopedTo,
            _ => return None,
        })
    }
}

impl GlassWebIrV1 {
    /// Validate graph invariants before exposing Web IR to another layer.
    pub fn validate(&self) -> Result<(), WebIrValidationError> {
        if self.schema_version != WEB_IR_SCHEMA_VERSION {
            return Err(WebIrValidationError::new(
                "schemaVersion",
                "unsupported Glass Web IR schema version",
            ));
        }
        if self.document.revision != self.revision {
            return Err(WebIrValidationError::new(
                "document.revision",
                "document revision must match the Web IR revision",
            ));
        }
        validate_optional_text("document.url", self.document.url.as_deref(), 2_048)?;
        validate_optional_text("document.title", self.document.title.as_deref(), 512)?;
        validate_optional_text("document.kind", self.document.kind.as_deref(), 128)?;
        validate_optional_text(
            "document.readyState",
            self.document.ready_state.as_deref(),
            32,
        )?;
        if self
            .document
            .url
            .as_deref()
            .is_some_and(|url| url.contains('?') || url.contains('#'))
        {
            return Err(WebIrValidationError::new(
                "document.url",
                "document URL must omit query strings and fragments",
            ));
        }
        if self.entities.is_empty() || self.entities.len() > MAX_WEB_IR_ENTITIES {
            return Err(WebIrValidationError::new(
                "entities",
                "entity count must be within the Glass Web IR bound",
            ));
        }
        let page_count = self
            .entities
            .iter()
            .filter(|entity| entity.kind == WebIrEntityKind::Page)
            .count();
        if page_count != 1 {
            return Err(WebIrValidationError::new(
                "entities",
                "Glass Web IR must contain exactly one page entity",
            ));
        }
        let mut ids = BTreeSet::new();
        let mut opaque_regions = 0_u32;
        for (index, entity) in self.entities.iter().enumerate() {
            validate_identifier(&format!("entities[{index}].id"), &entity.id)?;
            if !ids.insert(entity.id.as_str()) {
                return Err(WebIrValidationError::new(
                    format!("entities[{index}].id"),
                    "entity IDs must be unique within one revision",
                ));
            }
            validate_optional_text(
                &format!("entities[{index}].role"),
                entity.role.as_deref(),
                MAX_WEB_IR_ENTITY_TEXT_BYTES,
            )?;
            validate_optional_text(
                &format!("entities[{index}].name"),
                entity.name.as_deref(),
                MAX_WEB_IR_ENTITY_TEXT_BYTES,
            )?;
            let unique_sources = entity
                .evidence_sources
                .iter()
                .copied()
                .collect::<BTreeSet<_>>();
            if entity.evidence_sources.len() > 12
                || unique_sources.len() != entity.evidence_sources.len()
            {
                return Err(WebIrValidationError::new(
                    format!("entities[{index}].evidenceSources"),
                    "evidence sources must be unique and bounded",
                ));
            }
            if entity.kind == WebIrEntityKind::OpaqueRegion {
                opaque_regions = opaque_regions.saturating_add(1);
                if entity.quality != EvidenceQuality::Opaque || !entity.evidence_sources.is_empty()
                {
                    return Err(WebIrValidationError::new(
                        format!("entities[{index}]"),
                        "opaque regions cannot claim positive evidence or quality",
                    ));
                }
            } else if entity.quality == EvidenceQuality::Opaque {
                return Err(WebIrValidationError::new(
                    format!("entities[{index}].quality"),
                    "opaque quality is reserved for opaque regions",
                ));
            }
            if entity.evidence_sources.is_empty()
                && entity.kind != WebIrEntityKind::Page
                && entity.kind != WebIrEntityKind::OpaqueRegion
            {
                return Err(WebIrValidationError::new(
                    format!("entities[{index}].evidenceSources"),
                    "positive entities require source provenance",
                ));
            }
        }
        if opaque_regions != self.coverage.opaque_regions {
            return Err(WebIrValidationError::new(
                "coverage.opaqueRegions",
                "coverage count must match opaque region entities",
            ));
        }
        if self.entity_details.len() > self.entities.len() {
            return Err(WebIrValidationError::new(
                "entityDetails",
                "entity details exceed the entity count",
            ));
        }
        for (entity_id, details) in &self.entity_details {
            if !ids.contains(entity_id.as_str()) {
                return Err(WebIrValidationError::new(
                    "entityDetails",
                    "entity details must reference a known entity",
                ));
            }
            validate_entity_details(entity_id, details, &ids)?;
        }
        if let Some(surface_set) = &self.surface_set {
            surface_set
                .validate()
                .map_err(|error| WebIrValidationError::new("surfaceSet", error.to_string()))?;
        }
        if self.relationships.len() > MAX_WEB_IR_RELATIONSHIPS {
            return Err(WebIrValidationError::new(
                "relationships",
                "relationship count exceeds the Glass Web IR bound",
            ));
        }
        let mut relationship_keys = BTreeSet::new();
        for (index, relationship) in self.relationships.iter().enumerate() {
            if relationship.from == relationship.to
                || !ids.contains(relationship.from.as_str())
                || !ids.contains(relationship.to.as_str())
            {
                return Err(WebIrValidationError::new(
                    format!("relationships[{index}]"),
                    "relationships must reference two distinct known entities",
                ));
            }
            if !relationship_keys.insert(relationship_key(relationship)) {
                return Err(WebIrValidationError::new(
                    format!("relationships[{index}]"),
                    "relationships must be unique",
                ));
            }
        }
        validate_diagnostics("coverage.reasons", &self.coverage.reasons, 16, 128)?;
        validate_diagnostics(
            "diagnostics",
            &self.diagnostics,
            MAX_WEB_IR_DIAGNOSTICS,
            MAX_WEB_IR_DIAGNOSTIC_BYTES,
        )?;
        if self.relationship_hint_diagnostics.len() > MAX_WEB_IR_ENTITIES {
            return Err(WebIrValidationError::new(
                "relationshipHintDiagnostics",
                "relationship hint diagnostic count exceeds its bound",
            ));
        }
        for (index, diagnostic) in self.relationship_hint_diagnostics.iter().enumerate() {
            validate_bounded_text(
                &format!("relationshipHintDiagnostics[{index}].parentRole"),
                &diagnostic.parent_role,
                64,
            )?;
        }
        let unique_missing_sources = self
            .limits
            .missing_sources
            .iter()
            .copied()
            .collect::<BTreeSet<_>>();
        if self.limits.missing_sources.len() > 12
            || unique_missing_sources.len() != self.limits.missing_sources.len()
        {
            return Err(WebIrValidationError::new(
                "limits.missingSources",
                "missing sources must be unique and bounded",
            ));
        }
        let serialized = serde_json::to_vec(self)
            .map_err(|error| WebIrValidationError::new("$", error.to_string()))?;
        if serialized.len() > MAX_WEB_IR_BYTES {
            return Err(WebIrValidationError::new(
                "$",
                "serialized Glass Web IR exceeds the 256 KiB bound",
            ));
        }
        Ok(())
    }

    /// Validate that the Web IR satisfies a representative fixture minimum.
    pub fn validate_against(
        &self,
        expectation: &WebIrFixtureExpectation,
    ) -> Result<(), WebIrValidationError> {
        self.validate()?;
        for (kind, minimum) in &expectation.required_entity_counts {
            let actual = self
                .entities
                .iter()
                .filter(|entity| entity.kind == *kind)
                .count() as u32;
            if actual < *minimum {
                return Err(WebIrValidationError::new(
                    format!("expectation.entities.{}", kind_name(*kind)),
                    format!("expected at least {minimum}, observed {actual}"),
                ));
            }
        }
        let opaque_regions = self
            .entities
            .iter()
            .filter(|entity| entity.kind == WebIrEntityKind::OpaqueRegion)
            .count() as u32;
        if opaque_regions != expectation.opaque_regions {
            return Err(WebIrValidationError::new(
                "expectation.opaqueRegions",
                format!(
                    "expected {expected}, observed {opaque_regions}",
                    expected = expectation.opaque_regions
                ),
            ));
        }
        for (kind, minimum) in &expectation.required_relationship_counts {
            let actual = self
                .relationships
                .iter()
                .filter(|relationship| relationship.kind == *kind)
                .count() as u32;
            if actual < *minimum {
                return Err(WebIrValidationError::new(
                    format!("expectation.relationships.{}", relationship_name(*kind)),
                    format!("expected at least {minimum}, observed {actual}"),
                ));
            }
        }
        Ok(())
    }

    /// Validate minimum counts for relationship-hint diagnostic outcomes.
    pub fn validate_hint_diagnostics_against(
        &self,
        expected_status_counts: &BTreeMap<RelationshipHintDiagnosticStatus, u32>,
    ) -> Result<(), WebIrValidationError> {
        self.validate()?;
        for (status, minimum) in expected_status_counts {
            let actual = self
                .relationship_hint_diagnostics
                .iter()
                .filter(|diagnostic| diagnostic.status == *status)
                .count() as u32;
            if actual < *minimum {
                return Err(WebIrValidationError::new(
                    format!(
                        "expectation.relationshipHintDiagnostics.{}",
                        hint_status_name(*status)
                    ),
                    format!("expected at least {minimum}, observed {actual}"),
                ));
            }
        }
        Ok(())
    }

    /// Validate that `next` is a compatible revision for transition analysis.
    ///
    /// Forward revisions are accepted. An exact same-revision document is also
    /// accepted for deterministic self-comparisons; same-revision content drift
    /// and revision regressions fail closed.
    pub fn validate_revision_transition(&self, next: &Self) -> Result<(), WebIrValidationError> {
        self.validate()?;
        next.validate()?;
        if next.revision < self.revision {
            return Err(WebIrValidationError::new(
                "revision",
                "target revision is older than the source revision",
            ));
        }
        if next.revision == self.revision && self != next {
            return Err(WebIrValidationError::new(
                "revision",
                "same-revision Web IR documents must have identical content",
            ));
        }
        Ok(())
    }

    /// Compute deterministic changes between two validated Web IR revisions.
    pub fn diff(&self, next: &Self) -> Result<GlassWebIrDiff, WebIrValidationError> {
        self.validate_revision_transition(next)?;

        let before_entities = self
            .entities
            .iter()
            .map(|entity| (entity.id.clone(), entity))
            .collect::<BTreeMap<_, _>>();
        let after_entities = next
            .entities
            .iter()
            .map(|entity| (entity.id.clone(), entity))
            .collect::<BTreeMap<_, _>>();
        let entity_ids = before_entities
            .keys()
            .chain(after_entities.keys())
            .cloned()
            .collect::<BTreeSet<_>>();
        let mut entity_changes = Vec::new();
        for id in entity_ids {
            match (before_entities.get(&id), after_entities.get(&id)) {
                (None, Some(after)) => entity_changes.push(WebIrEntityChange {
                    id,
                    kind: WebIrChangeKind::Added,
                    before: None,
                    after: Some((*after).clone()),
                }),
                (Some(before), None) => entity_changes.push(WebIrEntityChange {
                    id,
                    kind: WebIrChangeKind::Removed,
                    before: Some((*before).clone()),
                    after: None,
                }),
                (Some(before), Some(after)) if before != after => {
                    entity_changes.push(WebIrEntityChange {
                        id,
                        kind: WebIrChangeKind::Changed,
                        before: Some((*before).clone()),
                        after: Some((*after).clone()),
                    });
                }
                (Some(_), Some(_)) | (None, None) => {}
            }
        }

        let before_relationships = self
            .relationships
            .iter()
            .map(|relationship| (relationship_key(relationship), relationship))
            .collect::<BTreeMap<_, _>>();
        let after_relationships = next
            .relationships
            .iter()
            .map(|relationship| (relationship_key(relationship), relationship))
            .collect::<BTreeMap<_, _>>();
        let relationship_keys = before_relationships
            .keys()
            .chain(after_relationships.keys())
            .cloned()
            .collect::<BTreeSet<_>>();
        let mut relationship_changes = Vec::new();
        for key in relationship_keys {
            match (
                before_relationships.get(&key),
                after_relationships.get(&key),
            ) {
                (None, Some(relationship)) => {
                    relationship_changes.push(WebIrRelationshipChange {
                        relationship: (*relationship).clone(),
                        kind: WebIrChangeKind::Added,
                    });
                }
                (Some(relationship), None) => {
                    relationship_changes.push(WebIrRelationshipChange {
                        relationship: (*relationship).clone(),
                        kind: WebIrChangeKind::Removed,
                    });
                }
                (Some(_), Some(_)) | (None, None) => {}
            }
        }

        Ok(GlassWebIrDiff {
            schema_version: WEB_IR_SCHEMA_VERSION,
            from_revision: self.revision,
            to_revision: next.revision,
            entity_changes,
            relationship_changes,
            coverage_changed: self.coverage != next.coverage,
            limits_changed: self.limits != next.limits,
            diagnostics_changed: self.diagnostics != next.diagnostics,
            relationship_hint_diagnostics_changed: self.relationship_hint_diagnostics
                != next.relationship_hint_diagnostics,
        })
    }

    /// Classify one source entity against a later validated revision.
    pub fn classify_entity_continuity(
        &self,
        next: &Self,
        entity_id: &str,
    ) -> Result<WebIrEntityContinuity, WebIrValidationError> {
        self.validate_revision_transition(next)?;
        let requested_id = entity_id.to_owned();
        let Some(source) = self.entities.iter().find(|entity| entity.id == entity_id) else {
            return Ok(WebIrEntityContinuity {
                requested_id,
                status: WebIrEntityContinuityStatus::Removed,
                current_id: None,
                reason: "entity was not present in the source revision".into(),
            });
        };

        if let Some(current) = next.entities.iter().find(|entity| entity.id == entity_id) {
            let status = if source.semantic_identity_key() == current.semantic_identity_key() {
                WebIrEntityContinuityStatus::Unchanged
            } else {
                WebIrEntityContinuityStatus::Changed
            };
            return Ok(WebIrEntityContinuity {
                requested_id,
                status,
                current_id: Some(current.id.clone()),
                reason: match status {
                    WebIrEntityContinuityStatus::Unchanged => {
                        "semantic identity remains compatible".into()
                    }
                    WebIrEntityContinuityStatus::Changed => {
                        "same revision-local ID has changed semantic identity".into()
                    }
                    _ => unreachable!("status is selected above"),
                },
            });
        }

        let Some(identity_key) = source.semantic_identity_key() else {
            return Ok(WebIrEntityContinuity {
                requested_id,
                status: WebIrEntityContinuityStatus::Removed,
                current_id: None,
                reason: "entity has no semantic identity for bounded rebinding".into(),
            });
        };
        let candidates = next
            .entities
            .iter()
            .filter(|entity| entity.semantic_identity_key().as_deref() == Some(&identity_key))
            .collect::<Vec<_>>();
        match candidates.as_slice() {
            [] => Ok(WebIrEntityContinuity {
                requested_id,
                status: WebIrEntityContinuityStatus::Removed,
                current_id: None,
                reason: "no compatible semantic identity was observed".into(),
            }),
            [candidate] => Ok(WebIrEntityContinuity {
                requested_id,
                status: WebIrEntityContinuityStatus::Rebound,
                current_id: Some(candidate.id.clone()),
                reason: "revision-local ID changed but semantic identity remained unique".into(),
            }),
            _ => Ok(WebIrEntityContinuity {
                requested_id,
                status: WebIrEntityContinuityStatus::Ambiguous,
                current_id: None,
                reason: "multiple compatible semantic identities were observed".into(),
            }),
        }
    }

    /// Serialize validated Web IR deterministically, independent of vector
    /// ordering supplied by callers.
    pub fn to_canonical_json(&self) -> Result<String, WebIrValidationError> {
        self.validate()?;
        let mut canonical = self.clone();
        canonical.entities.sort_by(|left, right| {
            (
                left.id != "page",
                left.id.as_str(),
                kind_name(left.kind),
                left.role.as_deref().unwrap_or_default(),
                left.name.as_deref().unwrap_or_default(),
            )
                .cmp(&(
                    right.id != "page",
                    right.id.as_str(),
                    kind_name(right.kind),
                    right.role.as_deref().unwrap_or_default(),
                    right.name.as_deref().unwrap_or_default(),
                ))
        });
        if let Some(surface_set) = &mut canonical.surface_set {
            surface_set
                .surfaces
                .sort_by(|left, right| left.surface_id.cmp(&right.surface_id));
        }
        for entity in &mut canonical.entities {
            entity.evidence_sources.sort();
            entity.evidence_sources.dedup();
        }
        canonical.relationships.sort_by_key(relationship_key);
        canonical.relationships.dedup();
        canonical.diagnostics.sort();
        canonical.coverage.reasons.sort();
        canonical
            .relationship_hint_diagnostics
            .sort_by_key(|diagnostic| {
                (
                    diagnostic.fact_index,
                    diagnostic.source,
                    diagnostic.parent_role.to_ascii_lowercase(),
                    diagnostic.hint,
                    diagnostic.status,
                )
            });
        serde_json::to_string(&canonical)
            .map_err(|error| WebIrValidationError::new("$", error.to_string()))
    }
}

/// Reconcile bounded extraction facts into the stable Glass Web IR v1 graph.
pub fn reconcile_evidence(
    evidence: &ExtractionEvidence,
) -> Result<GlassWebIrV1, WebIrValidationError> {
    evidence
        .validate_relationship_hints()
        .map_err(|error| WebIrValidationError::new(error.path, error.reason))?;
    let mut relationship_hint_diagnostics = evidence
        .facts
        .iter()
        .enumerate()
        .filter_map(|(fact_index, fact)| {
            Some(WebIrRelationshipHintDiagnostic {
                fact_index,
                source: fact.source,
                hint: fact.relationship_hint?,
                parent_role: fact.parent_role.clone()?,
                status: RelationshipHintDiagnosticStatus::Validated,
            })
        })
        .collect::<Vec<_>>();
    let mut facts = evidence
        .facts
        .iter()
        .cloned()
        .enumerate()
        .collect::<Vec<_>>();
    facts.sort_by_key(|(_, fact)| fact_sort_key(fact));

    let observed_sources = evidence
        .sources
        .iter()
        .copied()
        .filter(|source| !evidence.limits.missing_sources.contains(source))
        .collect::<Vec<_>>();
    let mut entities = vec![WebIrEntity {
        id: "page".into(),
        kind: WebIrEntityKind::Page,
        role: None,
        name: None,
        quality: EvidenceQuality::Confirmed,
        evidence_sources: observed_sources,
    }];
    let mut entity_details = BTreeMap::from([(
        "page".to_string(),
        WebIrEntityDetails {
            supported_actions: vec![WebIrAction::Read, WebIrAction::Navigate],
            semantic_stability_key: Some("page".into()),
            ..WebIrEntityDetails::default()
        },
    )]);
    let mut indexes: BTreeMap<String, Vec<usize>> = BTreeMap::new();
    let mut suffixes: BTreeMap<String, usize> = BTreeMap::new();
    let mut diagnostics = BTreeSet::new();

    let mut parent_links = BTreeSet::new();
    for (fact_index, fact) in facts {
        let Some(kind) = canonical_kind(&fact) else {
            if fact.relationship_hint.is_some()
                && let Some(diagnostic) = relationship_hint_diagnostics
                    .iter_mut()
                    .find(|diagnostic| diagnostic.fact_index == fact_index)
            {
                diagnostic.status = RelationshipHintDiagnosticStatus::UnmatchedParent;
            }
            diagnostics.insert(format!("unsupportedFact:{}", fact.kind));
            continue;
        };
        let mut fact_details = entity_details_for_fact(kind, &fact);
        match &evidence.scope {
            ExtractionScope::Document => {}
            ExtractionScope::Region { region_id } => {
                fact_details.scope = WebIrScopeKind::Region;
                fact_details.scope_id = Some(region_id.clone());
            }
            ExtractionScope::Frame { frame_id } => {
                fact_details.scope = WebIrScopeKind::Frame;
                fact_details.scope_id = Some(frame_id.clone());
            }
        }
        let key = canonical_key(kind, fact.role.as_deref(), fact.name.as_deref());
        let existing = indexes.get(&key).and_then(|candidates| {
            candidates
                .iter()
                .copied()
                .find(|index| !entities[*index].evidence_sources.contains(&fact.source))
        });
        let entity_id = if let Some(index) = existing {
            let entity = &mut entities[index];
            entity.quality = stronger_quality(entity.quality, fact.quality);
            entity.evidence_sources.push(fact.source);
            entity.evidence_sources.sort();
            entity.evidence_sources.dedup();
            entity.id.clone()
        } else {
            let base_id = format!("entity_{}_{}", kind_name(kind), slug(fact.name.as_deref()));
            let suffix = suffixes.entry(base_id.clone()).or_insert(0);
            let id = if *suffix == 0 {
                base_id.clone()
            } else {
                format!("{base_id}_{}", *suffix)
            };
            *suffix = suffix.saturating_add(1);
            let index = entities.len();
            entities.push(WebIrEntity {
                id: id.clone(),
                kind,
                role: fact.role,
                name: fact.name,
                quality: fact.quality,
                evidence_sources: vec![fact.source],
            });
            indexes.entry(key).or_default().push(index);
            id
        };
        entity_details
            .entry(entity_id.clone())
            .and_modify(|details| merge_entity_details(details, &fact_details))
            .or_insert(fact_details);
        if let Some(parent_role) = fact.parent_role {
            parent_links.insert((
                fact_index,
                parent_role.to_ascii_lowercase(),
                entity_id,
                kind,
                fact.relationship_hint,
            ));
        }
    }

    for index in 0..evidence.coverage.opaque_regions {
        let id = format!("opaque_region_{index}");
        entities.push(WebIrEntity {
            id: id.clone(),
            kind: WebIrEntityKind::OpaqueRegion,
            role: None,
            name: None,
            quality: EvidenceQuality::Opaque,
            evidence_sources: Vec::new(),
        });
        let mut details = WebIrEntityDetails {
            truncated: true,
            ..WebIrEntityDetails::default()
        };
        match &evidence.scope {
            ExtractionScope::Document => {}
            ExtractionScope::Region { region_id } => {
                details.scope = WebIrScopeKind::Region;
                details.scope_id = Some(region_id.clone());
            }
            ExtractionScope::Frame { frame_id } => {
                details.scope = WebIrScopeKind::Frame;
                details.scope_id = Some(frame_id.clone());
            }
        }
        entity_details.insert(id, details);
    }

    let mut relationships = entities
        .iter()
        .filter(|entity| entity.kind != WebIrEntityKind::Page)
        .map(|entity| WebIrRelationship {
            from: "page".into(),
            to: entity.id.clone(),
            kind: WebIrRelationshipKind::Contains,
        })
        .collect::<Vec<_>>();
    for (fact_index, parent_role, child_id, child_kind, relationship_hint) in parent_links {
        let Some(parent_kind) = parent_entity_kind(&parent_role) else {
            if relationship_hint.is_some() {
                set_hint_status(
                    &mut relationship_hint_diagnostics,
                    fact_index,
                    RelationshipHintDiagnosticStatus::UnmatchedParent,
                );
            }
            continue;
        };
        let Some(parent_id) = entities
            .iter()
            .find(|entity| {
                entity.kind == parent_kind
                    && entity
                        .role
                        .as_deref()
                        .is_some_and(|role| role.eq_ignore_ascii_case(&parent_role))
            })
            .map(|entity| entity.id.clone())
        else {
            if relationship_hint.is_some() {
                set_hint_status(
                    &mut relationship_hint_diagnostics,
                    fact_index,
                    RelationshipHintDiagnosticStatus::UnmatchedParent,
                );
            }
            continue;
        };
        if parent_id == child_id {
            if relationship_hint.is_some() {
                set_hint_status(
                    &mut relationship_hint_diagnostics,
                    fact_index,
                    RelationshipHintDiagnosticStatus::UnmatchedParent,
                );
            }
            continue;
        }
        relationships.push(WebIrRelationship {
            from: parent_id,
            to: child_id,
            kind: relationship_kind(parent_kind, child_kind, relationship_hint),
        });
        if relationship_hint.is_some() {
            set_hint_status(
                &mut relationship_hint_diagnostics,
                fact_index,
                RelationshipHintDiagnosticStatus::Emitted,
            );
        }
    }
    relationships.sort_by_key(|relationship| {
        (
            relationship.from.clone(),
            relationship.to.clone(),
            relationship.kind,
        )
    });
    relationships.dedup();
    let entity_kinds = entities
        .iter()
        .map(|entity| (entity.id.as_str(), entity.kind))
        .collect::<BTreeMap<_, _>>();
    for relationship in &relationships {
        if matches!(
            entity_kinds.get(relationship.from.as_str()),
            Some(
                WebIrEntityKind::Region
                    | WebIrEntityKind::Form
                    | WebIrEntityKind::Dialog
                    | WebIrEntityKind::Table
                    | WebIrEntityKind::Collection
            )
        ) && let Some(details) = entity_details.get_mut(&relationship.to)
        {
            details.region_id = Some(relationship.from.clone());
        }
    }
    let draft = GlassWebIrV1 {
        schema_version: WEB_IR_SCHEMA_VERSION,
        revision: evidence.revision,
        document: WebIrDocument {
            revision: evidence.revision,
            ..WebIrDocument::default()
        },
        entities,
        relationships,
        entity_details,
        coverage: evidence.coverage.clone(),
        limits: evidence.limits.clone(),
        surface_set: evidence.surface_set.clone(),
        diagnostics: diagnostics.into_iter().collect(),
        relationship_hint_diagnostics,
    };
    draft.validate()?;
    Ok(draft)
}

/// A machine-readable Glass Web IR validation failure.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WebIrValidationError {
    pub path: String,
    pub reason: String,
}

impl WebIrValidationError {
    fn new(path: impl Into<String>, reason: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            reason: reason.into(),
        }
    }
}

impl Display for WebIrValidationError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{}: {}", self.path, self.reason)
    }
}

impl std::error::Error for WebIrValidationError {}

fn validate_identifier(path: &str, value: &str) -> Result<(), WebIrValidationError> {
    if value.is_empty()
        || value.len() > 128
        || value.chars().any(char::is_whitespace)
        || value.chars().any(char::is_control)
    {
        return Err(WebIrValidationError::new(
            path,
            "identifier must be non-empty, at most 128 bytes, and contain no whitespace",
        ));
    }
    Ok(())
}

fn validate_bounded_text(
    path: &str,
    value: &str,
    max_bytes: usize,
) -> Result<(), WebIrValidationError> {
    if value.is_empty() || value.len() > max_bytes || value.chars().any(char::is_control) {
        return Err(WebIrValidationError::new(
            path,
            format!("value must be non-empty, at most {max_bytes} bytes, and contain no controls"),
        ));
    }
    Ok(())
}

fn validate_optional_text(
    path: &str,
    value: Option<&str>,
    max_bytes: usize,
) -> Result<(), WebIrValidationError> {
    if let Some(value) = value {
        validate_bounded_text(path, value, max_bytes)?;
    }
    Ok(())
}

fn validate_diagnostics(
    path: &str,
    values: &[String],
    max_count: usize,
    max_bytes: usize,
) -> Result<(), WebIrValidationError> {
    if values.len() > max_count {
        return Err(WebIrValidationError::new(
            path,
            format!("diagnostic count exceeds {max_count}"),
        ));
    }
    for (index, value) in values.iter().enumerate() {
        validate_bounded_text(&format!("{path}[{index}]"), value, max_bytes)?;
    }
    Ok(())
}

fn validate_entity_details(
    entity_id: &str,
    details: &WebIrEntityDetails,
    ids: &BTreeSet<&str>,
) -> Result<(), WebIrValidationError> {
    if details.supported_actions.len() > MAX_WEB_IR_ACTIONS
        || details
            .supported_actions
            .windows(2)
            .any(|pair| pair[0] >= pair[1])
    {
        return Err(WebIrValidationError::new(
            format!("entityDetails.{entity_id}.supportedActions"),
            "supported actions must be unique, sorted, and bounded",
        ));
    }
    if details.state.disabled == Some(true)
        && details
            .supported_actions
            .iter()
            .any(|action| !matches!(action, WebIrAction::Read | WebIrAction::Extract))
    {
        return Err(WebIrValidationError::new(
            format!("entityDetails.{entity_id}.supportedActions"),
            "disabled entities cannot advertise executable actions",
        ));
    }
    if details.state.read_only == Some(true)
        && details.supported_actions.iter().any(|action| {
            matches!(
                action,
                WebIrAction::Type | WebIrAction::Select | WebIrAction::Check | WebIrAction::Uncheck
            )
        })
    {
        return Err(WebIrValidationError::new(
            format!("entityDetails.{entity_id}.supportedActions"),
            "read-only entities cannot advertise value mutation",
        ));
    }
    if let Some(region_id) = details.region_id.as_deref()
        && !ids.contains(region_id)
    {
        return Err(WebIrValidationError::new(
            format!("entityDetails.{entity_id}.regionId"),
            "region membership must reference a known entity",
        ));
    }
    if details.scope == WebIrScopeKind::Document && details.scope_id.is_some() {
        return Err(WebIrValidationError::new(
            format!("entityDetails.{entity_id}.scopeId"),
            "document-scoped entities cannot carry a frame or shadow scope ID",
        ));
    }
    validate_optional_text(
        &format!("entityDetails.{entity_id}.scopeId"),
        details.scope_id.as_deref(),
        128,
    )?;
    validate_optional_text(
        &format!("entityDetails.{entity_id}.semanticStabilityKey"),
        details.semantic_stability_key.as_deref(),
        512,
    )
}

fn entity_details_for_fact(kind: WebIrEntityKind, fact: &EvidenceFact) -> WebIrEntityDetails {
    let role = fact
        .role
        .as_deref()
        .unwrap_or_default()
        .to_ascii_lowercase();
    let mut supported_actions = match kind {
        WebIrEntityKind::Page => vec![WebIrAction::Read, WebIrAction::Navigate],
        WebIrEntityKind::Text | WebIrEntityKind::Row | WebIrEntityKind::Cell => {
            vec![WebIrAction::Read]
        }
        WebIrEntityKind::Region => vec![WebIrAction::Read, WebIrAction::Extract],
        WebIrEntityKind::Form => vec![WebIrAction::Read, WebIrAction::Extract, WebIrAction::Submit],
        WebIrEntityKind::Field => {
            let mut actions = vec![WebIrAction::Read];
            if fact.read_only != Some(true) {
                match role.as_str() {
                    "checkbox" => actions.extend([WebIrAction::Check, WebIrAction::Uncheck]),
                    "radio" => actions.push(WebIrAction::Check),
                    "combobox" | "listbox" => actions.push(WebIrAction::Select),
                    _ => actions.push(WebIrAction::Type),
                }
            }
            actions
        }
        WebIrEntityKind::Action
        | WebIrEntityKind::NavigationItem
        | WebIrEntityKind::UnknownInteractive => vec![WebIrAction::Click],
        WebIrEntityKind::Tab => vec![WebIrAction::Click, WebIrAction::Select],
        WebIrEntityKind::Link => vec![WebIrAction::Click, WebIrAction::Navigate],
        WebIrEntityKind::Table
        | WebIrEntityKind::Collection
        | WebIrEntityKind::CollectionItem
        | WebIrEntityKind::Frame
        | WebIrEntityKind::ShadowRoot
        | WebIrEntityKind::Probe => vec![WebIrAction::Read, WebIrAction::Extract],
        WebIrEntityKind::Dialog => vec![
            WebIrAction::Read,
            WebIrAction::Close,
            WebIrAction::Confirm,
            WebIrAction::Cancel,
        ],
        WebIrEntityKind::PaginationControl => vec![
            WebIrAction::Click,
            WebIrAction::Extract,
            WebIrAction::Paginate,
        ],
        WebIrEntityKind::OpaqueRegion => Vec::new(),
    };
    supported_actions.sort();
    supported_actions.dedup();
    let input_type = fact.input_type.as_deref().unwrap_or_default();
    let name = fact
        .name
        .as_deref()
        .unwrap_or_default()
        .to_ascii_lowercase();
    let sensitivity = if input_type.eq_ignore_ascii_case("password") {
        WebIrSensitivity::Secret
    } else if input_type.eq_ignore_ascii_case("file") {
        WebIrSensitivity::Personal
    } else if name.contains("card") || name.contains("payment") {
        WebIrSensitivity::Financial
    } else {
        WebIrSensitivity::Public
    };
    WebIrEntityDetails {
        state: WebIrEntityState {
            disabled: None,
            read_only: fact.read_only,
            required: fact.required,
            checked: None,
            empty: fact.empty,
            visible: fact.geometry_present,
            hit_testable: None,
        },
        supported_actions,
        region_id: None,
        scope: WebIrScopeKind::Document,
        scope_id: None,
        sensitivity,
        semantic_stability_key: Some(canonical_key(
            kind,
            fact.role.as_deref(),
            fact.name.as_deref(),
        )),
        truncated: false,
    }
}

fn merge_entity_details(current: &mut WebIrEntityDetails, incoming: &WebIrEntityDetails) {
    current.state.read_only = current.state.read_only.or(incoming.state.read_only);
    current.state.required = current.state.required.or(incoming.state.required);
    current.state.empty = current.state.empty.or(incoming.state.empty);
    current.state.visible = current.state.visible.or(incoming.state.visible);
    current
        .supported_actions
        .extend(incoming.supported_actions.iter().copied());
    current.supported_actions.sort();
    current.supported_actions.dedup();
    current.sensitivity = match (current.sensitivity, incoming.sensitivity) {
        (WebIrSensitivity::Secret, _) | (_, WebIrSensitivity::Secret) => WebIrSensitivity::Secret,
        (WebIrSensitivity::Financial, _) | (_, WebIrSensitivity::Financial) => {
            WebIrSensitivity::Financial
        }
        (WebIrSensitivity::Personal, _) | (_, WebIrSensitivity::Personal) => {
            WebIrSensitivity::Personal
        }
        (WebIrSensitivity::Public, WebIrSensitivity::Public) => WebIrSensitivity::Public,
        _ => WebIrSensitivity::Unknown,
    };
    current.truncated |= incoming.truncated;
}

fn canonical_kind(fact: &EvidenceFact) -> Option<WebIrEntityKind> {
    let role = fact
        .role
        .as_deref()
        .unwrap_or_default()
        .to_ascii_lowercase();
    let name = fact
        .name
        .as_deref()
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase();
    if matches!(role.as_str(), "button" | "link")
        && matches!(
            name.as_str(),
            "next" | "next page" | "previous" | "previous page"
        )
    {
        return Some(WebIrEntityKind::PaginationControl);
    }
    if fact.source == EvidenceSource::Dom {
        return match fact
            .name
            .as_deref()
            .unwrap_or_default()
            .to_ascii_uppercase()
            .as_str()
        {
            "FORM" => Some(WebIrEntityKind::Form),
            "INPUT" | "TEXTAREA" | "SELECT" => Some(WebIrEntityKind::Field),
            "BUTTON" => Some(WebIrEntityKind::Action),
            "A" => Some(WebIrEntityKind::Link),
            "TABLE" => Some(WebIrEntityKind::Table),
            "TR" => Some(WebIrEntityKind::Row),
            "TD" | "TH" => Some(WebIrEntityKind::Cell),
            "DIALOG" => Some(WebIrEntityKind::Dialog),
            "NAV" => Some(WebIrEntityKind::Region),
            "ARTICLE" => Some(WebIrEntityKind::CollectionItem),
            "IFRAME" | "FRAME" => Some(WebIrEntityKind::Frame),
            _ => None,
        };
    }
    match role.as_str() {
        "form" => Some(WebIrEntityKind::Form),
        "navigation" | "main" | "search" | "complementary" | "article" | "toolbar" => {
            Some(WebIrEntityKind::Region)
        }
        "dialog" | "alertdialog" => Some(WebIrEntityKind::Dialog),
        "button" => Some(WebIrEntityKind::Action),
        "menuitem" => Some(WebIrEntityKind::NavigationItem),
        "tab" => Some(WebIrEntityKind::Tab),
        "textbox" | "combobox" | "checkbox" | "radio" | "spinbutton" | "listbox" => {
            Some(WebIrEntityKind::Field)
        }
        "link" => Some(WebIrEntityKind::Link),
        "table" => Some(WebIrEntityKind::Table),
        "row" => Some(WebIrEntityKind::Row),
        "cell" | "gridcell" => Some(WebIrEntityKind::Cell),
        "list" => Some(WebIrEntityKind::Collection),
        "listitem" => Some(WebIrEntityKind::CollectionItem),
        "heading" | "text" => Some(WebIrEntityKind::Text),
        "iframe" | "frame" => Some(WebIrEntityKind::Frame),
        "shadowroot" => Some(WebIrEntityKind::ShadowRoot),
        "viewport" => Some(WebIrEntityKind::Probe),
        _ if fact.kind == "control" => Some(WebIrEntityKind::UnknownInteractive),
        _ => None,
    }
}

fn canonical_key(kind: WebIrEntityKind, role: Option<&str>, name: Option<&str>) -> String {
    format!(
        "{}|{}|{}",
        kind_name(kind),
        role.unwrap_or_default().to_ascii_lowercase(),
        name.unwrap_or_default().to_ascii_lowercase()
    )
}

fn relationship_key(relationship: &WebIrRelationship) -> (String, String, WebIrRelationshipKind) {
    (
        relationship.from.clone(),
        relationship.to.clone(),
        relationship.kind,
    )
}

fn fact_sort_key(fact: &EvidenceFact) -> (EvidenceSource, String, String, String) {
    (
        fact.source,
        fact.kind.clone(),
        fact.role.clone().unwrap_or_default(),
        fact.name.clone().unwrap_or_default(),
    )
}

fn kind_name(kind: WebIrEntityKind) -> &'static str {
    match kind {
        WebIrEntityKind::Page => "page",
        WebIrEntityKind::Region => "region",
        WebIrEntityKind::Form => "form",
        WebIrEntityKind::Field => "field",
        WebIrEntityKind::Action => "action",
        WebIrEntityKind::Link => "link",
        WebIrEntityKind::NavigationItem => "navigationItem",
        WebIrEntityKind::Tab => "tab",
        WebIrEntityKind::Table => "table",
        WebIrEntityKind::Row => "row",
        WebIrEntityKind::Cell => "cell",
        WebIrEntityKind::Collection => "collection",
        WebIrEntityKind::CollectionItem => "collectionItem",
        WebIrEntityKind::Dialog => "dialog",
        WebIrEntityKind::PaginationControl => "paginationControl",
        WebIrEntityKind::Frame => "frame",
        WebIrEntityKind::ShadowRoot => "shadowRoot",
        WebIrEntityKind::Probe => "probe",
        WebIrEntityKind::Text => "text",
        WebIrEntityKind::UnknownInteractive => "unknownInteractive",
        WebIrEntityKind::OpaqueRegion => "opaqueRegion",
    }
}

fn relationship_name(kind: WebIrRelationshipKind) -> &'static str {
    match kind {
        WebIrRelationshipKind::Contains => "contains",
        WebIrRelationshipKind::Labels => "labels",
        WebIrRelationshipKind::Owns => "owns",
        WebIrRelationshipKind::Controls => "controls",
        WebIrRelationshipKind::NavigatesTo => "navigatesTo",
        WebIrRelationshipKind::Opens => "opens",
        WebIrRelationshipKind::Confirms => "confirms",
        WebIrRelationshipKind::Cancels => "cancels",
        WebIrRelationshipKind::Continues => "continues",
        WebIrRelationshipKind::Submits => "submits",
        WebIrRelationshipKind::HeaderFor => "headerFor",
        WebIrRelationshipKind::CellOf => "cellOf",
        WebIrRelationshipKind::Selects => "selects",
        WebIrRelationshipKind::RepeatsAs => "repeatsAs",
        WebIrRelationshipKind::ScopedTo => "scopedTo",
    }
}

fn hint_status_name(status: RelationshipHintDiagnosticStatus) -> &'static str {
    match status {
        RelationshipHintDiagnosticStatus::Validated => "validated",
        RelationshipHintDiagnosticStatus::Emitted => "emitted",
        RelationshipHintDiagnosticStatus::UnmatchedParent => "unmatchedParent",
    }
}

fn set_hint_status(
    diagnostics: &mut [WebIrRelationshipHintDiagnostic],
    fact_index: usize,
    status: RelationshipHintDiagnosticStatus,
) {
    if let Some(diagnostic) = diagnostics
        .iter_mut()
        .find(|diagnostic| diagnostic.fact_index == fact_index)
    {
        diagnostic.status = status;
    }
}

fn parent_entity_kind(role: &str) -> Option<WebIrEntityKind> {
    match role {
        "form" => Some(WebIrEntityKind::Form),
        "dialog" | "alertdialog" => Some(WebIrEntityKind::Dialog),
        "article" | "complementary" | "main" | "navigation" | "region" | "search" | "toolbar" => {
            Some(WebIrEntityKind::Region)
        }
        _ => None,
    }
}

fn relationship_kind(
    parent_kind: WebIrEntityKind,
    child_kind: WebIrEntityKind,
    explicit_hint: Option<EvidenceRelationshipHint>,
) -> WebIrRelationshipKind {
    if let Some(hint) = explicit_hint {
        return match hint {
            EvidenceRelationshipHint::Contains => WebIrRelationshipKind::Contains,
            EvidenceRelationshipHint::Labels => WebIrRelationshipKind::Labels,
            EvidenceRelationshipHint::Owns => WebIrRelationshipKind::Owns,
            EvidenceRelationshipHint::Controls => WebIrRelationshipKind::Controls,
            EvidenceRelationshipHint::NavigatesTo => WebIrRelationshipKind::NavigatesTo,
            EvidenceRelationshipHint::Opens => WebIrRelationshipKind::Opens,
            EvidenceRelationshipHint::Confirms => WebIrRelationshipKind::Confirms,
            EvidenceRelationshipHint::Cancels => WebIrRelationshipKind::Cancels,
            EvidenceRelationshipHint::Continues => WebIrRelationshipKind::Continues,
            EvidenceRelationshipHint::Submits => WebIrRelationshipKind::Submits,
            EvidenceRelationshipHint::HeaderFor => WebIrRelationshipKind::HeaderFor,
            EvidenceRelationshipHint::CellOf => WebIrRelationshipKind::CellOf,
            EvidenceRelationshipHint::Selects => WebIrRelationshipKind::Selects,
            EvidenceRelationshipHint::RepeatsAs => WebIrRelationshipKind::RepeatsAs,
            EvidenceRelationshipHint::ScopedTo => WebIrRelationshipKind::ScopedTo,
        };
    }
    if parent_kind == WebIrEntityKind::Form && child_kind == WebIrEntityKind::Field {
        WebIrRelationshipKind::Owns
    } else {
        WebIrRelationshipKind::Contains
    }
}

fn slug(value: Option<&str>) -> String {
    let mut output = String::new();
    for character in value.unwrap_or("entity").chars() {
        if character.is_ascii_alphanumeric() {
            output.push(character.to_ascii_lowercase());
        } else if !output.ends_with('-') {
            output.push('-');
        }
        if output.len() >= 48 {
            break;
        }
    }
    let trimmed = output.trim_matches('-');
    if trimmed.is_empty() {
        "entity".into()
    } else {
        trimmed.into()
    }
}

fn stronger_quality(left: EvidenceQuality, right: EvidenceQuality) -> EvidenceQuality {
    if quality_rank(right) > quality_rank(left) {
        right
    } else {
        left
    }
}

fn quality_rank(quality: EvidenceQuality) -> u8 {
    match quality {
        EvidenceQuality::Opaque => 0,
        EvidenceQuality::Conflicted => 1,
        EvidenceQuality::Inferred => 2,
        EvidenceQuality::Partial => 3,
        EvidenceQuality::Strong => 4,
        EvidenceQuality::Confirmed => 5,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::extraction::{EvidenceCoverage, ExtractionScope};

    fn evidence() -> ExtractionEvidence {
        ExtractionEvidence {
            schema_version: 1,
            revision: 9,
            scope: ExtractionScope::Document,
            sources: vec![EvidenceSource::Accessibility, EvidenceSource::Forms],
            facts: vec![
                EvidenceFact {
                    source: EvidenceSource::Accessibility,
                    kind: "node".into(),
                    quality: EvidenceQuality::Confirmed,
                    role: Some("textbox".into()),
                    name: Some("Email".into()),
                    input_type: Some("email".into()),
                    required: None,
                    read_only: None,
                    empty: None,
                    geometry_present: None,
                    parent_role: None,
                    relationship_hint: None,
                },
                EvidenceFact {
                    source: EvidenceSource::Accessibility,
                    kind: "node".into(),
                    quality: EvidenceQuality::Confirmed,
                    role: Some("textbox".into()),
                    name: Some("Email".into()),
                    input_type: Some("email".into()),
                    required: None,
                    read_only: None,
                    empty: None,
                    geometry_present: None,
                    parent_role: None,
                    relationship_hint: None,
                },
                EvidenceFact {
                    source: EvidenceSource::Forms,
                    kind: "control".into(),
                    quality: EvidenceQuality::Strong,
                    role: Some("textbox".into()),
                    name: Some("Email".into()),
                    input_type: Some("email".into()),
                    required: Some(true),
                    read_only: Some(false),
                    empty: Some(true),
                    geometry_present: None,
                    parent_role: None,
                    relationship_hint: None,
                },
            ],
            coverage: EvidenceCoverage {
                structural: EvidenceQuality::Partial,
                semantic: EvidenceQuality::Strong,
                interactive_entities_observed: 2,
                opaque_regions: 0,
                reasons: Vec::new(),
            },
            limits: ExtractionEvidenceLimits {
                truncated: false,
                omitted_facts: 0,
                text_bytes: 15,
                missing_sources: Vec::new(),
            },
            surface_set: None,
        }
    }

    #[test]
    fn reconciliation_merges_cross_source_facts_but_preserves_duplicate_candidates() {
        let draft = reconcile_evidence(&evidence()).unwrap();
        assert_eq!(draft.revision, 9);
        assert_eq!(draft.entities.len(), 3);
        assert_eq!(draft.relationships.len(), 2);
        assert_eq!(
            draft.entities[1].evidence_sources,
            vec![EvidenceSource::Accessibility, EvidenceSource::Forms]
        );
        assert_eq!(draft.entities[1].quality, EvidenceQuality::Confirmed);
    }

    #[test]
    fn reconciliation_links_children_to_observed_regions_only() {
        let mut evidence = evidence();
        evidence.facts.push(EvidenceFact {
            source: EvidenceSource::Accessibility,
            kind: "node".into(),
            quality: EvidenceQuality::Confirmed,
            role: Some("search".into()),
            name: Some("Site search".into()),
            input_type: None,
            required: None,
            read_only: None,
            empty: None,
            geometry_present: None,
            parent_role: None,
            relationship_hint: None,
        });
        evidence.facts[0].parent_role = Some("search".into());
        let draft = reconcile_evidence(&evidence).unwrap();
        let region_id = draft
            .entities
            .iter()
            .find(|entity| entity.kind == WebIrEntityKind::Region)
            .map(|entity| entity.id.clone())
            .unwrap();
        assert!(draft.relationships.iter().any(|relationship| {
            relationship.from == region_id && relationship.kind == WebIrRelationshipKind::Contains
        }));
    }

    #[test]
    fn reconciliation_emits_form_ownership_for_observed_form_ancestry() {
        let mut evidence = evidence();
        evidence.facts.push(EvidenceFact {
            source: EvidenceSource::Accessibility,
            kind: "node".into(),
            quality: EvidenceQuality::Confirmed,
            role: Some("form".into()),
            name: Some("Account".into()),
            input_type: None,
            required: None,
            read_only: None,
            empty: None,
            geometry_present: None,
            parent_role: None,
            relationship_hint: None,
        });
        evidence.facts[0].parent_role = Some("form".into());
        let draft = reconcile_evidence(&evidence).unwrap();
        let form_id = draft
            .entities
            .iter()
            .find(|entity| entity.kind == WebIrEntityKind::Form)
            .map(|entity| entity.id.clone())
            .unwrap();
        let field_ids = draft
            .entities
            .iter()
            .filter(|entity| entity.kind == WebIrEntityKind::Field)
            .map(|entity| entity.id.as_str())
            .collect::<BTreeSet<_>>();
        assert!(draft.relationships.iter().any(|relationship| {
            relationship.from == form_id
                && field_ids.contains(relationship.to.as_str())
                && relationship.kind == WebIrRelationshipKind::Owns
        }));
    }

    #[test]
    fn reconciliation_honors_explicit_relationship_hints() {
        let mut evidence = evidence();
        evidence.facts.push(EvidenceFact {
            source: EvidenceSource::Accessibility,
            kind: "node".into(),
            quality: EvidenceQuality::Confirmed,
            role: Some("form".into()),
            name: Some("Account".into()),
            input_type: None,
            required: None,
            read_only: None,
            empty: None,
            geometry_present: None,
            parent_role: None,
            relationship_hint: None,
        });
        evidence.facts[0].parent_role = Some("form".into());
        evidence.facts[0].relationship_hint = Some(EvidenceRelationshipHint::Controls);
        let draft = reconcile_evidence(&evidence).unwrap();
        assert!(
            draft
                .relationships
                .iter()
                .any(|relationship| relationship.kind == WebIrRelationshipKind::Controls)
        );
        assert_eq!(draft.relationship_hint_diagnostics.len(), 1);
        let diagnostic = &draft.relationship_hint_diagnostics[0];
        assert_eq!(diagnostic.fact_index, 0);
        assert_eq!(diagnostic.source, EvidenceSource::Accessibility);
        assert_eq!(diagnostic.hint, EvidenceRelationshipHint::Controls);
        assert_eq!(diagnostic.parent_role, "form");
        assert_eq!(diagnostic.status, RelationshipHintDiagnosticStatus::Emitted);
        let expected = BTreeMap::from([(RelationshipHintDiagnosticStatus::Emitted, 1)]);
        draft.validate_hint_diagnostics_against(&expected).unwrap();
    }

    #[test]
    fn reconciliation_rejects_unbounded_or_unsupported_hints() {
        let mut missing_parent = evidence();
        missing_parent.facts[0].relationship_hint = Some(EvidenceRelationshipHint::Controls);
        let error = reconcile_evidence(&missing_parent).unwrap_err();
        assert_eq!(error.path, "facts[0].relationshipHint");

        let mut wrong_source = evidence();
        wrong_source.facts[0].parent_role = Some("search".into());
        wrong_source.facts[0].relationship_hint = Some(EvidenceRelationshipHint::Controls);
        wrong_source.facts[0].source = EvidenceSource::Layout;
        let error = reconcile_evidence(&wrong_source).unwrap_err();
        assert_eq!(error.path, "facts[0].relationshipHint");
    }

    #[test]
    fn reconciliation_marks_valid_unmatched_hints_without_emitting_edges() {
        let mut evidence = evidence();
        evidence.facts[0].parent_role = Some("dialog".into());
        evidence.facts[0].relationship_hint = Some(EvidenceRelationshipHint::Controls);
        let draft = reconcile_evidence(&evidence).unwrap();
        assert_eq!(
            draft.relationship_hint_diagnostics[0].status,
            RelationshipHintDiagnosticStatus::UnmatchedParent
        );
        assert!(
            !draft
                .relationships
                .iter()
                .any(|relationship| relationship.kind == WebIrRelationshipKind::Controls)
        );
        let expected = BTreeMap::from([(RelationshipHintDiagnosticStatus::UnmatchedParent, 1)]);
        draft.validate_hint_diagnostics_against(&expected).unwrap();
        let missing = BTreeMap::from([(RelationshipHintDiagnosticStatus::Emitted, 1)]);
        let error = draft
            .validate_hint_diagnostics_against(&missing)
            .unwrap_err();
        assert_eq!(
            error.path,
            "expectation.relationshipHintDiagnostics.emitted"
        );
    }

    #[test]
    fn reconciliation_materializes_opaque_coverage_without_provenance_claims() {
        let mut evidence = evidence();
        evidence.coverage.opaque_regions = 2;
        let draft = reconcile_evidence(&evidence).unwrap();
        let opaque = draft
            .entities
            .iter()
            .filter(|entity| entity.kind == WebIrEntityKind::OpaqueRegion)
            .collect::<Vec<_>>();
        assert_eq!(opaque.len(), 2);
        assert!(opaque.iter().all(|entity| {
            entity.quality == EvidenceQuality::Opaque && entity.evidence_sources.is_empty()
        }));
        assert_eq!(draft.relationships.len(), 4);
        draft.validate().unwrap();
    }

    #[test]
    fn fixture_expectation_validates_minimum_graph_shape() {
        let draft = reconcile_evidence(&evidence()).unwrap();
        let mut expectation = WebIrFixtureExpectation::default();
        expectation
            .required_entity_counts
            .insert(WebIrEntityKind::Page, 1);
        expectation
            .required_entity_counts
            .insert(WebIrEntityKind::Field, 2);
        expectation
            .required_relationship_counts
            .insert(WebIrRelationshipKind::Contains, 2);
        draft.validate_against(&expectation).unwrap();

        expectation
            .required_entity_counts
            .insert(WebIrEntityKind::Dialog, 1);
        let error = draft.validate_against(&expectation).unwrap_err();
        assert_eq!(error.path, "expectation.entities.dialog");
    }

    #[test]
    fn fixture_corpus_vocabulary_matches_draft_contract() {
        let corpus: serde_json::Value =
            serde_json::from_str(include_str!("../tests/fixtures/web-ir/corpus-v1.json")).unwrap();
        let fixtures = corpus["fixtures"].as_array().unwrap();
        assert_eq!(fixtures.len(), 8);
        for fixture in fixtures {
            let entities = fixture["expectedEntities"].as_array().unwrap();
            let relationships = fixture["expectedRelationships"].as_array().unwrap();
            assert_eq!(
                entities.first().and_then(serde_json::Value::as_str),
                Some("page")
            );
            assert!(relationships.iter().any(|value| value == "contains"));
            for entity in entities {
                assert!(WebIrEntityKind::from_contract_name(entity.as_str().unwrap()).is_some());
            }
            for relationship in relationships {
                assert!(
                    WebIrRelationshipKind::from_contract_name(relationship.as_str().unwrap())
                        .is_some()
                );
            }
            let opaque_count = entities
                .iter()
                .filter(|entity| entity.as_str() == Some("opaqueRegion"))
                .count() as u64;
            assert_eq!(opaque_count, fixture["opaqueRegions"].as_u64().unwrap());
        }
    }

    #[test]
    fn reconciliation_is_deterministic_and_does_not_emit_raw_values() {
        let draft = reconcile_evidence(&evidence()).unwrap();
        let first = draft.to_canonical_json().unwrap();
        let second = reconcile_evidence(&evidence())
            .unwrap()
            .to_canonical_json()
            .unwrap();
        assert_eq!(first, second);
        assert!(!first.contains("secret"));
    }

    #[test]
    fn revision_diff_reports_deterministic_entity_relationship_and_metadata_changes() {
        let before = reconcile_evidence(&evidence()).unwrap();
        let mut after_evidence = evidence();
        after_evidence.revision = 10;
        after_evidence.coverage.structural = EvidenceQuality::Strong;
        after_evidence.facts[0].parent_role = Some("form".into());

        let mut form_fact = after_evidence.facts[0].clone();
        form_fact.role = Some("form".into());
        form_fact.name = Some("Account".into());
        form_fact.parent_role = None;
        after_evidence.facts.push(form_fact);

        let mut layout_fact = after_evidence.facts[0].clone();
        layout_fact.source = EvidenceSource::Layout;
        layout_fact.quality = EvidenceQuality::Strong;
        layout_fact.geometry_present = Some(true);
        after_evidence.facts.push(layout_fact);

        let after = reconcile_evidence(&after_evidence).unwrap();
        let diff = before.diff(&after).unwrap();
        assert_eq!(diff.from_revision, 9);
        assert_eq!(diff.to_revision, 10);
        assert!(
            diff.entity_changes
                .iter()
                .any(|change| change.kind == WebIrChangeKind::Added)
        );
        assert!(
            diff.entity_changes
                .iter()
                .any(|change| change.kind == WebIrChangeKind::Changed)
        );
        assert!(diff.relationship_changes.iter().any(|change| {
            change.kind == WebIrChangeKind::Added
                && change.relationship.kind == WebIrRelationshipKind::Owns
        }));
        assert!(diff.coverage_changed);
        assert!(!diff.limits_changed);
        assert!(!diff.diagnostics_changed);
        assert!(!diff.relationship_hint_diagnostics_changed);

        let reverse_error = after.diff(&before).unwrap_err();
        assert_eq!(reverse_error.path, "revision");
        assert_eq!(
            reverse_error.reason,
            "target revision is older than the source revision"
        );
        assert_eq!(
            serde_json::to_string(&diff).unwrap(),
            serde_json::to_string(&before.diff(&after).unwrap()).unwrap()
        );
    }

    #[test]
    fn continuity_classification_fails_closed_for_stale_and_ambiguous_targets() {
        let source = reconcile_evidence(&evidence()).unwrap();
        let target_id = source
            .entities
            .iter()
            .find(|entity| entity.kind == WebIrEntityKind::Field)
            .map(|entity| entity.id.clone())
            .unwrap();

        let unchanged = source
            .classify_entity_continuity(&source, &target_id)
            .unwrap();
        assert_eq!(unchanged.status, WebIrEntityContinuityStatus::Unchanged);

        let mut changed_draft = source.clone();
        changed_draft.revision = 10;
        changed_draft.document.revision = 10;
        changed_draft
            .entities
            .iter_mut()
            .find(|entity| entity.id == target_id)
            .unwrap()
            .name = Some("Different field".into());
        let changed = source
            .classify_entity_continuity(&changed_draft, &target_id)
            .unwrap();
        assert_eq!(changed.status, WebIrEntityContinuityStatus::Changed);

        let target_key = source
            .entities
            .iter()
            .find(|entity| entity.id == target_id)
            .and_then(WebIrEntity::semantic_identity_key)
            .unwrap();
        let removed_ids = source
            .entities
            .iter()
            .filter(|entity| entity.semantic_identity_key().as_deref() == Some(target_key.as_str()))
            .map(|entity| entity.id.clone())
            .collect::<BTreeSet<_>>();
        let mut rebound_draft = source.clone();
        rebound_draft.revision = 10;
        rebound_draft.document.revision = 10;
        rebound_draft
            .entities
            .retain(|entity| !removed_ids.contains(&entity.id));
        rebound_draft.relationships.retain(|relationship| {
            !removed_ids.contains(&relationship.from) && !removed_ids.contains(&relationship.to)
        });
        rebound_draft
            .entity_details
            .retain(|entity_id, _| !removed_ids.contains(entity_id));
        let mut rebound_entity = source
            .entities
            .iter()
            .find(|entity| entity.semantic_identity_key().as_deref() == Some(target_key.as_str()))
            .unwrap()
            .clone();
        let rebound_id = "replacement-field".to_owned();
        rebound_entity.id = rebound_id.clone();
        rebound_draft.entities.push(rebound_entity.clone());
        if let Some(details) = source.entity_details.get(&target_id) {
            rebound_draft
                .entity_details
                .insert(rebound_id.clone(), details.clone());
        }
        let rebound = source
            .classify_entity_continuity(&rebound_draft, &target_id)
            .unwrap();
        assert_eq!(rebound.status, WebIrEntityContinuityStatus::Rebound);
        assert_eq!(rebound.current_id.as_deref(), Some(rebound_id.as_str()));

        let mut removed_draft = source.clone();
        removed_draft.revision = 10;
        removed_draft.document.revision = 10;
        removed_draft
            .entities
            .retain(|entity| !removed_ids.contains(&entity.id));
        removed_draft.relationships.retain(|relationship| {
            !removed_ids.contains(&relationship.from) && !removed_ids.contains(&relationship.to)
        });
        removed_draft
            .entity_details
            .retain(|entity_id, _| !removed_ids.contains(entity_id));
        let removed = source
            .classify_entity_continuity(&removed_draft, &target_id)
            .unwrap();
        assert_eq!(removed.status, WebIrEntityContinuityStatus::Removed);

        let mut ambiguous_draft = removed_draft;
        let mut first = rebound_entity;
        first.id = "replacement-a".into();
        let mut second = first.clone();
        second.id = "replacement-b".into();
        ambiguous_draft.entities.extend([first, second]);
        let ambiguous = source
            .classify_entity_continuity(&ambiguous_draft, &target_id)
            .unwrap();
        assert_eq!(ambiguous.status, WebIrEntityContinuityStatus::Ambiguous);
        assert!(ambiguous.current_id.is_none());
    }

    #[test]
    fn same_revision_content_drift_is_rejected() {
        let source = reconcile_evidence(&evidence()).unwrap();
        let mut drifted = source.clone();
        drifted
            .entities
            .iter_mut()
            .find(|entity| entity.kind == WebIrEntityKind::Field)
            .unwrap()
            .name = Some("Changed at the same revision".into());

        assert_eq!(
            source
                .validate_revision_transition(&drifted)
                .unwrap_err()
                .reason,
            "same-revision Web IR documents must have identical content"
        );
    }

    #[test]
    fn validation_rejects_dangling_relationships() {
        let mut draft = reconcile_evidence(&evidence()).unwrap();
        draft.relationships[0].to = "missing".into();
        let error = draft.validate().unwrap_err();
        assert_eq!(error.path, "relationships[0]");
    }
    #[test]
    fn canonical_json_is_independent_of_graph_vector_order() {
        let mut draft = reconcile_evidence(&evidence()).unwrap();
        draft.diagnostics = vec!["unsupported:z".into(), "unsupported:a".into()];
        draft.coverage.reasons = vec!["shadowBoundary".into(), "frameBoundary".into()];
        let expected = draft.to_canonical_json().unwrap();
        let mut shuffled = draft.clone();
        shuffled.entities.reverse();
        shuffled.relationships.reverse();
        shuffled.diagnostics.reverse();
        shuffled.coverage.reasons.reverse();
        for entity in &mut shuffled.entities {
            entity.evidence_sources.reverse();
        }
        assert_eq!(shuffled.to_canonical_json().unwrap(), expected);
    }

    #[test]
    fn validation_rejects_inconsistent_opaque_coverage_and_duplicates() {
        let mut missing_opaque = reconcile_evidence(&evidence()).unwrap();
        missing_opaque.coverage.opaque_regions = 1;
        let error = missing_opaque.validate().unwrap_err();
        assert_eq!(error.path, "coverage.opaqueRegions");

        let mut opaque = evidence();
        opaque.coverage.opaque_regions = 1;
        let mut draft = reconcile_evidence(&opaque).unwrap();
        draft
            .entities
            .iter_mut()
            .find(|entity| entity.kind == WebIrEntityKind::OpaqueRegion)
            .unwrap()
            .quality = EvidenceQuality::Strong;
        let error = draft.validate().unwrap_err();
        assert_eq!(error.path, "entities[3]");

        let mut duplicate = reconcile_evidence(&evidence()).unwrap();
        duplicate
            .relationships
            .push(duplicate.relationships[0].clone());
        let error = duplicate.validate().unwrap_err();
        assert_eq!(error.path, "relationships[2]");
    }
    #[test]
    fn stable_ir_populates_action_state_and_sensitivity_metadata() {
        let draft = reconcile_evidence(&evidence()).unwrap();
        let field = draft
            .entities
            .iter()
            .find(|entity| entity.kind == WebIrEntityKind::Field)
            .unwrap();
        let details = &draft.entity_details[&field.id];
        assert!(details.supported_actions.contains(&WebIrAction::Read));
        assert!(details.supported_actions.contains(&WebIrAction::Type));
        assert_eq!(details.state.required, Some(true));
        assert_eq!(details.sensitivity, WebIrSensitivity::Public);
        assert!(details.semantic_stability_key.is_some());
    }

    #[test]
    fn stable_ir_rejects_unbounded_diagnostics_and_executable_disabled_entities() {
        let mut diagnostic = reconcile_evidence(&evidence()).unwrap();
        diagnostic.diagnostics = vec!["x".repeat(MAX_WEB_IR_DIAGNOSTIC_BYTES + 1)];
        assert_eq!(diagnostic.validate().unwrap_err().path, "diagnostics[0]");

        let mut disabled = reconcile_evidence(&evidence()).unwrap();
        let field_id = disabled
            .entities
            .iter()
            .find(|entity| entity.kind == WebIrEntityKind::Field)
            .unwrap()
            .id
            .clone();
        disabled
            .entity_details
            .get_mut(&field_id)
            .unwrap()
            .state
            .disabled = Some(true);
        assert_eq!(
            disabled.validate().unwrap_err().path,
            format!("entityDetails.{field_id}.supportedActions")
        );
    }
}
