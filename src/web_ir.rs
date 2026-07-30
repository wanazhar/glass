//! Experimental draft Glass Web IR reconciliation.
//!
//! This module intentionally remains a sidecar to the existing semantic
//! observation and intent paths. It turns bounded evidence into canonical draft
//! entities without dispatching browser actions or inventing unsupported links.

use crate::extraction::{
    EvidenceFact, EvidenceQuality, EvidenceSource, ExtractionEvidence, ExtractionEvidenceLimits,
};
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt::{Display, Formatter};

/// Version of the experimental draft Web IR contract.
pub const WEB_IR_DRAFT_SCHEMA_VERSION: u32 = 1;
const MAX_DRAFT_ENTITIES: usize = 4_096;
const MAX_DRAFT_RELATIONSHIPS: usize = 8_192;

/// Canonical entity kinds used by the draft graph.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum DraftEntityKind {
    Page,
    Region,
    Form,
    Field,
    Action,
    Link,
    Table,
    Row,
    Cell,
    Collection,
    CollectionItem,
    Dialog,
    PaginationControl,
    Text,
    UnknownInteractive,
    OpaqueRegion,
}

/// Canonical relationship kinds used by the draft graph.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum DraftRelationshipKind {
    Contains,
    Labels,
    Owns,
    Controls,
    NavigatesTo,
    Opens,
    Confirms,
    Cancels,
    Continues,
}

/// One canonical entity reconciled from one or more evidence sources.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DraftEntity {
    pub id: String,
    pub kind: DraftEntityKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    pub quality: EvidenceQuality,
    pub evidence_sources: Vec<EvidenceSource>,
}

/// One relationship between canonical draft entities.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DraftRelationship {
    pub from: String,
    pub to: String,
    pub kind: DraftRelationshipKind,
}

/// A bounded, experimental Web IR draft.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GlassWebIrDraft {
    pub schema_version: u32,
    pub revision: u64,
    pub entities: Vec<DraftEntity>,
    pub relationships: Vec<DraftRelationship>,
    pub coverage: crate::extraction::EvidenceCoverage,
    pub limits: ExtractionEvidenceLimits,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub diagnostics: Vec<String>,
}

impl GlassWebIrDraft {
    /// Validate graph invariants before exposing a draft to another layer.
    pub fn validate(&self) -> Result<(), WebIrValidationError> {
        if self.schema_version != WEB_IR_DRAFT_SCHEMA_VERSION {
            return Err(WebIrValidationError::new(
                "schemaVersion",
                "unsupported draft Web IR schema version",
            ));
        }
        if self.entities.is_empty() || self.entities.len() > MAX_DRAFT_ENTITIES {
            return Err(WebIrValidationError::new(
                "entities",
                "entity count must be between 1 and the draft bound",
            ));
        }
        let page_count = self
            .entities
            .iter()
            .filter(|entity| entity.kind == DraftEntityKind::Page)
            .count();
        if page_count != 1 {
            return Err(WebIrValidationError::new(
                "entities",
                "drafts must contain exactly one page entity",
            ));
        }
        let mut ids = BTreeSet::new();
        for entity in &self.entities {
            if entity.id.is_empty() || entity.id.len() > 128 || !ids.insert(entity.id.as_str()) {
                return Err(WebIrValidationError::new(
                    "entities.id",
                    "entity IDs must be unique and bounded",
                ));
            }
            if entity.evidence_sources.is_empty()
                && entity.kind != DraftEntityKind::Page
                && entity.kind != DraftEntityKind::OpaqueRegion
            {
                return Err(WebIrValidationError::new(
                    "entities.evidenceSources",
                    "non-page, non-opaque entities require source provenance",
                ));
            }
        }
        if self.relationships.len() > MAX_DRAFT_RELATIONSHIPS {
            return Err(WebIrValidationError::new(
                "relationships",
                "relationship count exceeds the draft bound",
            ));
        }
        for relationship in &self.relationships {
            if relationship.from == relationship.to
                || !ids.contains(relationship.from.as_str())
                || !ids.contains(relationship.to.as_str())
            {
                return Err(WebIrValidationError::new(
                    "relationships",
                    "relationships must reference two distinct known entities",
                ));
            }
        }
        Ok(())
    }

    /// Serialize a validated draft deterministically.
    pub fn to_canonical_json(&self) -> Result<String, WebIrValidationError> {
        self.validate()?;
        serde_json::to_string(self)
            .map_err(|error| WebIrValidationError::new("$", error.to_string()))
    }
}

/// Reconcile bounded extraction facts into a canonical draft graph.
pub fn reconcile_evidence(
    evidence: &ExtractionEvidence,
) -> Result<GlassWebIrDraft, WebIrValidationError> {
    let mut facts = evidence.facts.clone();
    facts.sort_by_key(fact_sort_key);

    let mut entities = vec![DraftEntity {
        id: "page".into(),
        kind: DraftEntityKind::Page,
        role: None,
        name: None,
        quality: EvidenceQuality::Confirmed,
        evidence_sources: Vec::new(),
    }];
    let mut indexes: BTreeMap<String, Vec<usize>> = BTreeMap::new();
    let mut suffixes: BTreeMap<String, usize> = BTreeMap::new();
    let mut diagnostics = BTreeSet::new();

    for fact in facts {
        let Some(kind) = canonical_kind(&fact) else {
            diagnostics.insert(format!("unsupportedFact:{}", fact.kind));
            continue;
        };
        let key = canonical_key(kind, fact.role.as_deref(), fact.name.as_deref());
        let existing = indexes.get(&key).and_then(|candidates| {
            candidates
                .iter()
                .copied()
                .find(|index| !entities[*index].evidence_sources.contains(&fact.source))
        });
        if let Some(index) = existing {
            let entity = &mut entities[index];
            entity.quality = stronger_quality(entity.quality, fact.quality);
            entity.evidence_sources.push(fact.source);
            entity.evidence_sources.sort();
            entity.evidence_sources.dedup();
            continue;
        }

        let base_id = format!("entity_{}_{}", kind_name(kind), slug(fact.name.as_deref()));
        let suffix = suffixes.entry(base_id.clone()).or_insert(0);
        let id = if *suffix == 0 {
            base_id.clone()
        } else {
            format!("{base_id}_{}", *suffix)
        };
        *suffix = suffix.saturating_add(1);
        let index = entities.len();
        entities.push(DraftEntity {
            id,
            kind,
            role: fact.role,
            name: fact.name,
            quality: fact.quality,
            evidence_sources: vec![fact.source],
        });
        indexes.entry(key).or_default().push(index);
    }

    for index in 0..evidence.coverage.opaque_regions {
        entities.push(DraftEntity {
            id: format!("opaque_region_{index}"),
            kind: DraftEntityKind::OpaqueRegion,
            role: None,
            name: None,
            quality: EvidenceQuality::Opaque,
            evidence_sources: Vec::new(),
        });
    }

    let relationships = entities
        .iter()
        .filter(|entity| entity.kind != DraftEntityKind::Page)
        .map(|entity| DraftRelationship {
            from: "page".into(),
            to: entity.id.clone(),
            kind: DraftRelationshipKind::Contains,
        })
        .collect();
    let draft = GlassWebIrDraft {
        schema_version: WEB_IR_DRAFT_SCHEMA_VERSION,
        revision: evidence.revision,
        entities,
        relationships,
        coverage: evidence.coverage.clone(),
        limits: evidence.limits.clone(),
        diagnostics: diagnostics.into_iter().collect(),
    };
    draft.validate()?;
    Ok(draft)
}

/// A machine-readable draft Web IR validation failure.
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

fn canonical_kind(fact: &EvidenceFact) -> Option<DraftEntityKind> {
    let role = fact
        .role
        .as_deref()
        .unwrap_or_default()
        .to_ascii_lowercase();
    if fact.source == EvidenceSource::Dom {
        return match fact
            .name
            .as_deref()
            .unwrap_or_default()
            .to_ascii_uppercase()
            .as_str()
        {
            "FORM" => Some(DraftEntityKind::Form),
            "INPUT" | "TEXTAREA" | "SELECT" => Some(DraftEntityKind::Field),
            "BUTTON" => Some(DraftEntityKind::Action),
            "A" => Some(DraftEntityKind::Link),
            "TABLE" => Some(DraftEntityKind::Table),
            "TR" => Some(DraftEntityKind::Row),
            "TD" | "TH" => Some(DraftEntityKind::Cell),
            "DIALOG" => Some(DraftEntityKind::Dialog),
            "NAV" => Some(DraftEntityKind::Region),
            "ARTICLE" => Some(DraftEntityKind::CollectionItem),
            _ => None,
        };
    }
    match role.as_str() {
        "form" => Some(DraftEntityKind::Form),
        "navigation" | "main" | "search" | "complementary" | "article" | "toolbar" => {
            Some(DraftEntityKind::Region)
        }
        "dialog" | "alertdialog" => Some(DraftEntityKind::Dialog),
        "textbox" | "combobox" | "checkbox" | "radio" | "spinbutton" | "listbox" => {
            Some(DraftEntityKind::Field)
        }
        "button" | "menuitem" | "tab" => Some(DraftEntityKind::Action),
        "link" => Some(DraftEntityKind::Link),
        "table" => Some(DraftEntityKind::Table),
        "row" => Some(DraftEntityKind::Row),
        "cell" | "gridcell" => Some(DraftEntityKind::Cell),
        "list" => Some(DraftEntityKind::Collection),
        "listitem" => Some(DraftEntityKind::CollectionItem),
        "heading" | "text" => Some(DraftEntityKind::Text),
        _ if fact.kind == "control" => Some(DraftEntityKind::UnknownInteractive),
        _ => None,
    }
}

fn canonical_key(kind: DraftEntityKind, role: Option<&str>, name: Option<&str>) -> String {
    format!(
        "{}|{}|{}",
        kind_name(kind),
        role.unwrap_or_default().to_ascii_lowercase(),
        name.unwrap_or_default().to_ascii_lowercase()
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

fn kind_name(kind: DraftEntityKind) -> &'static str {
    match kind {
        DraftEntityKind::Page => "page",
        DraftEntityKind::Region => "region",
        DraftEntityKind::Form => "form",
        DraftEntityKind::Field => "field",
        DraftEntityKind::Action => "action",
        DraftEntityKind::Link => "link",
        DraftEntityKind::Table => "table",
        DraftEntityKind::Row => "row",
        DraftEntityKind::Cell => "cell",
        DraftEntityKind::Collection => "collection",
        DraftEntityKind::CollectionItem => "collectionItem",
        DraftEntityKind::Dialog => "dialog",
        DraftEntityKind::PaginationControl => "paginationControl",
        DraftEntityKind::Text => "text",
        DraftEntityKind::UnknownInteractive => "unknownInteractive",
        DraftEntityKind::OpaqueRegion => "opaqueRegion",
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
    fn reconciliation_materializes_opaque_coverage_without_provenance_claims() {
        let mut evidence = evidence();
        evidence.coverage.opaque_regions = 2;
        let draft = reconcile_evidence(&evidence).unwrap();
        let opaque = draft
            .entities
            .iter()
            .filter(|entity| entity.kind == DraftEntityKind::OpaqueRegion)
            .collect::<Vec<_>>();
        assert_eq!(opaque.len(), 2);
        assert!(opaque.iter().all(|entity| {
            entity.quality == EvidenceQuality::Opaque && entity.evidence_sources.is_empty()
        }));
        assert_eq!(draft.relationships.len(), 4);
        draft.validate().unwrap();
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
    fn validation_rejects_dangling_relationships() {
        let mut draft = reconcile_evidence(&evidence()).unwrap();
        draft.relationships[0].to = "missing".into();
        let error = draft.validate().unwrap_err();
        assert_eq!(error.path, "relationships");
    }
}
