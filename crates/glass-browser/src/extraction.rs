//! Stable bounded evidence-extraction contract and browser-observation adapter.
//!
//! Authored requests are strict and resource-bounded. Observed evidence is
//! source-labelled, explicit about omissions, and safe to reconcile into
//! Glass Web IR without dispatching browser actions.
use crate::browser::dom::{CompactAxNode, CompactInteractiveElement, DomNode};
use crate::browser::session::{PageContext, find_region_node};
use crate::surfaces::{
    BridgeGrantRegistry, BridgeTrustLevel, CoverageLevel, DiagnosticSeverity, InteractionCoverage,
    ProvenanceSourceClass, SURFACE_SCHEMA_VERSION, SemanticBridge, Surface, SurfaceCapability,
    SurfaceCoverage, SurfaceDiagnostic, SurfaceEvidence, SurfaceEvidenceSource, SurfaceId,
    SurfaceKind, SurfaceProvenance, SurfaceRevision, SurfaceSet, UnderstandingLevel,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::fmt::{Display, Formatter};
use std::time::{Duration, Instant};

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
    Svg,
    CanvasDetection,
    MediaMetadata,
    EmbeddedDocument,
    Pdf,
    BrowserNative,
    Bridge,
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
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
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EvidenceCoverage {
    pub structural: EvidenceQuality,
    pub semantic: EvidenceQuality,
    pub interactive_entities_observed: u32,
    pub opaque_regions: u32,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub reasons: Vec<String>,
}

/// Bounded evidence produced from an existing page observation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExtractionEvidence {
    pub schema_version: u32,
    pub revision: u64,
    pub scope: ExtractionScope,
    pub sources: Vec<EvidenceSource>,
    pub facts: Vec<EvidenceFact>,
    pub limits: ExtractionEvidenceLimits,
    pub coverage: EvidenceCoverage,
    /// Validated transport-neutral surface evidence for this observation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub surface_set: Option<SurfaceSet>,
}

/// One redacted, source-labelled fact from browser evidence.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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
    pub autocomplete: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub required: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub read_only: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub empty: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub checked: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub disabled: Option<bool>,
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
        | EvidenceSource::Svg
        | EvidenceSource::CanvasDetection
        | EvidenceSource::MediaMetadata
        | EvidenceSource::EmbeddedDocument
        | EvidenceSource::Pdf
        | EvidenceSource::BrowserNative
        | EvidenceSource::Bridge
        | EvidenceSource::BoundedProbe => false,
    }
}

/// Explicit omissions and truncation from one evidence extraction.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExtractionEvidenceLimits {
    pub truncated: bool,
    pub omitted_facts: u32,
    pub text_bytes: u32,
    pub missing_sources: Vec<EvidenceSource>,
}

/// Extract bounded, redacted facts from an existing page observation.
///
/// This adapter is side-effect free. It consumes only the already-collected
/// `PageContext`; browser acquisition remains owned by the session layer.
pub fn extract_page_context(
    context: &PageContext,
    request: &ExtractionRequest,
) -> Result<ExtractionEvidence, ExtractionContractError> {
    validate_observation_boundaries(context)?;
    request.validate()?;
    let region_root = match &request.scope {
        ExtractionScope::Document => None,
        ExtractionScope::Region { region_id } if region_id == "region_main" => None,
        ExtractionScope::Region { region_id } => {
            Some(find_region_node(context, region_id).ok_or_else(|| {
                ExtractionContractError::new(
                    "scope.regionId",
                    format!(
                        "region {region_id:?} is not present at revision {}",
                        context.accessibility.revision
                    ),
                )
            })?)
        }
        ExtractionScope::Frame { frame_id } if frame_id == &context.page.frame_id => None,
        ExtractionScope::Frame { frame_id } => {
            return Err(ExtractionContractError::new(
                "scope.frameId",
                format!("frame {frame_id:?} is not the active observed frame"),
            ));
        }
    };

    let mut collector = EvidenceCollector::new(request.budgets);
    let mut missing_sources = BTreeSet::new();
    let inconsistent_observation = observation_has_mutation_race(context);
    let incomplete_accessibility = context.incomplete.iter().any(|reason| {
        matches!(
            reason,
            crate::browser::session::ObservationIncompleteReason::AccessibilityNode
                | crate::browser::session::ObservationIncompleteReason::AccessibilityLabel
                | crate::browser::session::ObservationIncompleteReason::Control
        )
    });

    for source in &request.sources {
        match source {
            EvidenceSource::Accessibility => {
                if incomplete_accessibility {
                    missing_sources.insert(*source);
                } else if let Some(root) = region_root {
                    collect_accessibility(root, &mut collector, 0, None);
                } else {
                    for root in &context.accessibility.roots {
                        if collector.node_budget_exhausted() {
                            collector.mark_omitted();
                            break;
                        }
                        collect_accessibility(root, &mut collector, 0, None);
                    }
                }
                if !incomplete_accessibility {
                    for control in &context.accessibility.interactive {
                        if control_in_region(control, region_root) {
                            collect_accessibility_control(control, &mut collector);
                        }
                    }
                }
            }
            EvidenceSource::Dom | EvidenceSource::Layout if region_root.is_some() => {
                missing_sources.insert(*source);
            }
            EvidenceSource::Dom => {
                if let Some(root) = context.dom.as_ref() {
                    collect_dom(root, &mut collector, 0);
                } else {
                    missing_sources.insert(*source);
                }
            }
            EvidenceSource::Forms => {
                if incomplete_accessibility {
                    missing_sources.insert(*source);
                } else {
                    for control in &context.accessibility.interactive {
                        if collector.node_budget_exhausted() {
                            collector.mark_omitted();
                            break;
                        }
                        if !control_in_region(control, region_root)
                            || (control.input_type.is_none()
                                && !is_form_control_role(&control.role))
                        {
                            continue;
                        }
                        if !collector.allow_node(0) {
                            break;
                        }
                        let parent_role = nearest_safe_parent_role(&control.ancestor_path);
                        collector.push(EvidenceFact {
                            source: *source,
                            kind: "control".into(),
                            quality: EvidenceQuality::Strong,
                            role: Some(control.role.clone()),
                            name: Some(control.name.clone()),
                            input_type: control.input_type.clone(),
                            autocomplete: control.autocomplete.clone(),
                            required: Some(control.required),
                            read_only: Some(control.read_only),
                            empty: Some(control.empty),
                            checked: control.checked,
                            disabled: control.disabled,
                            geometry_present: None,
                            relationship_hint: custom_form_control_hint(
                                &control.role,
                                parent_role.as_deref(),
                            ),
                            parent_role,
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
            EvidenceSource::Navigation
            | EvidenceSource::Tables
            | EvidenceSource::Collections
            | EvidenceSource::Dialogs => {
                if incomplete_accessibility {
                    missing_sources.insert(*source);
                } else if let Some(root) = region_root {
                    collect_semantic_source(root, *source, &mut collector, 0, None);
                } else {
                    for root in &context.accessibility.roots {
                        if collector.node_budget_exhausted() {
                            collector.mark_omitted();
                            break;
                        }
                        collect_semantic_source(root, *source, &mut collector, 0, None);
                    }
                }
            }
            EvidenceSource::Frames => {
                let count = if matches!(request.scope, ExtractionScope::Frame { .. }) {
                    1
                } else {
                    context.boundaries.child_frames
                };
                if count > 0 {
                    push_boundary_fact(
                        &mut collector,
                        *source,
                        "frame",
                        format!("{count} observed frame scope(s)"),
                    );
                }
            }
            EvidenceSource::ShadowDom => {
                if context.boundaries.shadow_roots > 0 {
                    push_boundary_fact(
                        &mut collector,
                        *source,
                        "shadowroot",
                        format!(
                            "{} observed shadow root(s)",
                            context.boundaries.shadow_roots
                        ),
                    );
                }
            }
            EvidenceSource::Svg => {
                if let Some(root) = context.dom.as_ref()
                    && has_svg_node(root)
                {
                    collect_svg_semantics(root, &mut collector, 0, false);
                } else if context.boundaries.svg_elements > 0 {
                    push_boundary_fact(
                        &mut collector,
                        *source,
                        "surface",
                        format!(
                            "{} observed SVG surface(s)",
                            context.boundaries.svg_elements
                        ),
                    );
                } else {
                    missing_sources.insert(*source);
                }
            }
            EvidenceSource::CanvasDetection
            | EvidenceSource::MediaMetadata
            | EvidenceSource::EmbeddedDocument
            | EvidenceSource::Pdf
            | EvidenceSource::BrowserNative
            | EvidenceSource::Bridge => {
                let count = match source {
                    EvidenceSource::CanvasDetection => context.boundaries.canvases,
                    EvidenceSource::MediaMetadata => context.boundaries.media_elements,
                    EvidenceSource::EmbeddedDocument => context.boundaries.embedded_documents,
                    EvidenceSource::Pdf => context.boundaries.pdf_documents,
                    EvidenceSource::BrowserNative => context.boundaries.native_surfaces,
                    EvidenceSource::Bridge => 0,
                    _ => 0,
                };
                if count > 0 {
                    push_boundary_fact(
                        &mut collector,
                        *source,
                        "surface",
                        format!("{count} observed surface(s)"),
                    );
                } else {
                    missing_sources.insert(*source);
                }
            }
            EvidenceSource::BoundedProbe => {
                if let Some(viewport) = context.boundaries.viewport {
                    push_boundary_fact(
                        &mut collector,
                        *source,
                        "viewport",
                        format!(
                            "{}x{} viewport in {}x{} document",
                            viewport.width,
                            viewport.height,
                            viewport.document_width,
                            viewport.document_height
                        ),
                    );
                } else {
                    missing_sources.insert(*source);
                }
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
        collector.timed_out,
        inconsistent_observation,
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
        surface_set: build_surface_set(context, &request.sources, &missing_sources)?,
    };
    trim_to_output_budget(&mut evidence, request.budgets.max_output_bytes)?;
    Ok(evidence)
}
/// Convert a validated page semantic bridge into bounded extraction evidence.
///
/// The bridge is never trusted by itself: origin, revision, capability
/// allowlists, and the independent host grant are checked before facts are
/// materialized.
pub fn extract_semantic_bridge(
    bridge: &SemanticBridge,
    grants: &BridgeGrantRegistry,
    expected_origin: &str,
    expected_revision: SurfaceRevision,
) -> Result<ExtractionEvidence, ExtractionContractError> {
    bridge
        .validate(grants, expected_origin, expected_revision)
        .map_err(|error| ExtractionContractError::new("bridge", error.to_string()))?;
    let timestamp = format!(
        "1970-01-01T00:{:02}:{:02}Z",
        (bridge.revision.0 / 60) % 60,
        bridge.revision.0 % 60
    );
    let provenance = SurfaceProvenance {
        schema_version: SURFACE_SCHEMA_VERSION,
        source_class: ProvenanceSourceClass::Bridge,
        source_id: format!("bridge:{}", bridge.surface_id.as_str()),
        backend: None,
        backend_version: None,
        adapter_version: None,
        bridge_version: Some(bridge.bridge_version.clone()),
        bridge_origin: Some(bridge.origin.clone()),
        bridge_trust: Some(BridgeTrustLevel::CapabilityGranted),
        grant_token: Some(bridge.grant_token.clone()),
        bridge_capabilities: bridge.capabilities.clone(),
        source_revision: bridge.revision,
        observed_at: timestamp.clone(),
        validated_at: timestamp,
    };
    let task_compilable = [
        SurfaceCapability::ReadStructure,
        SurfaceCapability::ReadState,
        SurfaceCapability::SemanticAction,
        SurfaceCapability::RevisionObservation,
        SurfaceCapability::Verification,
    ]
    .iter()
    .all(|capability| bridge.capabilities.contains(capability));
    let surface_understanding = if task_compilable {
        UnderstandingLevel::TaskCompilable
    } else {
        UnderstandingLevel::Semantic
    };
    let interaction = if task_compilable {
        InteractionCoverage::TaskCompilable
    } else {
        InteractionCoverage::Semantic
    };
    let surface = Surface {
        schema_version: SURFACE_SCHEMA_VERSION,
        surface_id: bridge.surface_id.clone(),
        parent_surface_id: None,
        nesting_depth: 0,
        kind: bridge.surface_kind.clone(),
        capabilities: bridge.capabilities.clone(),
        understanding: surface_understanding,
        coverage: SurfaceCoverage {
            structural: CoverageLevel::Strong,
            semantic: CoverageLevel::Strong,
            interaction,
        },
        evidence: vec![SurfaceEvidence {
            source: SurfaceEvidenceSource::Bridge,
            quality: CoverageLevel::Strong,
            provenance,
            detail: Some("validated semantic bridge evidence".into()),
        }],
        revision: bridge.revision,
        diagnostics: Vec::new(),
    };
    let surface_set = SurfaceSet {
        schema_version: SURFACE_SCHEMA_VERSION,
        surfaces: vec![surface],
    };
    surface_set
        .validate_with_grants(grants)
        .map_err(|error| ExtractionContractError::new("bridge.surfaceSet", error.to_string()))?;
    let mut facts = Vec::with_capacity(bridge.entities.len());
    for entity in &bridge.entities {
        let role = entity.role.clone().or_else(|| Some(entity.kind.clone()));
        facts.push(EvidenceFact {
            source: EvidenceSource::Bridge,
            kind: entity.kind.clone(),
            quality: match entity.quality {
                CoverageLevel::Complete | CoverageLevel::Strong => EvidenceQuality::Confirmed,
                CoverageLevel::Partial => EvidenceQuality::Partial,
                CoverageLevel::Opaque => EvidenceQuality::Opaque,
            },
            role,
            name: entity.name.clone(),
            parent_role: None,
            relationship_hint: None,
            input_type: None,
            autocomplete: None,
            required: None,
            read_only: None,
            empty: None,
            checked: None,
            disabled: None,
            geometry_present: Some(true),
        });
    }
    let evidence = ExtractionEvidence {
        schema_version: EXTRACTION_CONTRACT_SCHEMA_VERSION,
        revision: bridge.revision.0,
        scope: ExtractionScope::Document,
        sources: vec![EvidenceSource::Bridge],
        facts,
        limits: ExtractionEvidenceLimits {
            truncated: false,
            omitted_facts: 0,
            text_bytes: 0,
            missing_sources: Vec::new(),
        },
        coverage: EvidenceCoverage {
            structural: EvidenceQuality::Confirmed,
            semantic: EvidenceQuality::Confirmed,
            interactive_entities_observed: bridge.entities.len() as u32,
            opaque_regions: 0,
            reasons: Vec::new(),
        },
        surface_set: Some(surface_set),
    };
    evidence.validate_relationship_hints()?;
    Ok(evidence)
}

fn build_surface_set(
    context: &PageContext,
    requested: &[EvidenceSource],
    missing: &BTreeSet<EvidenceSource>,
) -> Result<Option<SurfaceSet>, ExtractionContractError> {
    let revision = SurfaceRevision::new(context.accessibility.revision.max(1))
        .map_err(|error| ExtractionContractError::new("surfaceSet.revision", error.to_string()))?;
    let quality = if context.boundaries.truncated || context.accessibility.truncated {
        CoverageLevel::Partial
    } else {
        CoverageLevel::Strong
    };
    let timestamp = format!(
        "1970-01-01T00:{:02}:{:02}Z",
        (revision.0 / 60) % 60,
        revision.0 % 60
    );
    let provenance = || SurfaceProvenance {
        schema_version: SURFACE_SCHEMA_VERSION,
        source_class: ProvenanceSourceClass::LiveWebIr,
        source_id: format!("page:{}", revision.0),
        backend: None,
        backend_version: None,
        adapter_version: Some("browser-observation-1".into()),
        bridge_version: None,
        bridge_origin: None,
        bridge_trust: None,
        grant_token: None,
        bridge_capabilities: Vec::new(),
        source_revision: revision,
        observed_at: timestamp.clone(),
        validated_at: timestamp.clone(),
    };
    let evidence = |source: SurfaceEvidenceSource, detail: String, level| SurfaceEvidence {
        source,
        quality: level,
        provenance: provenance(),
        detail: Some(detail),
    };
    let available =
        |source: EvidenceSource| requested.contains(&source) && !missing.contains(&source);
    let mut surfaces = Vec::new();
    let mut surface_bound_omitted = false;
    let add = |surfaces: &mut Vec<Surface>,
               id: String,
               parent: Option<SurfaceId>,
               kind: SurfaceKind,
               evidence_values: Vec<SurfaceEvidence>,
               understanding: UnderstandingLevel,
               coverage: SurfaceCoverage,
               capabilities: Vec<SurfaceCapability>| {
        let surface_id = SurfaceId::new(id).map_err(|error| {
            ExtractionContractError::new("surfaceSet.surfaces", error.to_string())
        })?;
        let nesting_depth = u16::from(parent.is_some());
        let diagnostics = match &kind {
            SurfaceKind::EmbeddedDocument | SurfaceKind::Pdf | SurfaceKind::BrowserNative => {
                vec![SurfaceDiagnostic {
                    severity: DiagnosticSeverity::Warning,
                    code: "semanticUnavailable".into(),
                    message: "surface boundary detected, but semantic content was not observed; semantic compilation is unavailable".into(),
                }]
            }
            _ => Vec::new(),
        };
        surfaces.push(Surface {
            schema_version: SURFACE_SCHEMA_VERSION,
            surface_id,
            parent_surface_id: parent,
            nesting_depth,
            kind,
            capabilities,
            understanding,
            coverage,
            evidence: evidence_values,
            revision,
            diagnostics,
        });
        Ok::<(), ExtractionContractError>(())
    };

    let mut document_evidence = Vec::new();
    if available(EvidenceSource::Dom) {
        document_evidence.push(evidence(
            SurfaceEvidenceSource::Dom,
            "observed document structure".into(),
            quality,
        ));
    }
    if available(EvidenceSource::Accessibility) {
        document_evidence.push(evidence(
            SurfaceEvidenceSource::Accessibility,
            "observed accessibility semantics".into(),
            quality,
        ));
    }
    let has_document = !document_evidence.is_empty();
    let root_id = SurfaceId::new("document")
        .map_err(|error| ExtractionContractError::new("surfaceSet.surfaces", error.to_string()))?;
    if has_document {
        add(
            &mut surfaces,
            "document".into(),
            None,
            SurfaceKind::Document,
            document_evidence,
            UnderstandingLevel::Structural,
            SurfaceCoverage {
                structural: quality,
                semantic: if available(EvidenceSource::Accessibility) {
                    quality
                } else {
                    CoverageLevel::Partial
                },
                interaction: InteractionCoverage::Unavailable,
            },
            vec![
                SurfaceCapability::ReadStructure,
                SurfaceCapability::ReadText,
                SurfaceCapability::ReadRelations,
                SurfaceCapability::ReadState,
                SurfaceCapability::Extraction,
                SurfaceCapability::RevisionObservation,
            ],
        )?;
    }
    let parent_surface_id = has_document.then(|| root_id.clone());
    let mut add_children = |kind: SurfaceKind,
                            source: SurfaceEvidenceSource,
                            request_source: EvidenceSource,
                            count: usize,
                            prefix: &str|
     -> Result<(), ExtractionContractError> {
        if !available(request_source) {
            return Ok(());
        }
        for index in 0..count.min(256) {
            if surfaces.len() >= 256 {
                surface_bound_omitted = true;
                break;
            }
            let structured_svg =
                matches!(&kind, SurfaceKind::Svg) && context.dom.as_ref().is_some_and(has_svg_node);
            let (understanding, semantic, capabilities) = if structured_svg {
                (
                    UnderstandingLevel::Semantic,
                    CoverageLevel::Strong,
                    vec![
                        SurfaceCapability::ReadStructure,
                        SurfaceCapability::ReadText,
                        SurfaceCapability::ReadRelations,
                        SurfaceCapability::Extraction,
                    ],
                )
            } else {
                (
                    UnderstandingLevel::Structural,
                    CoverageLevel::Partial,
                    vec![SurfaceCapability::ReadStructure],
                )
            };
            add(
                &mut surfaces,
                format!("{prefix}_{index}"),
                parent_surface_id.clone(),
                kind.clone(),
                vec![evidence(
                    source,
                    format!("observed {prefix} boundary"),
                    quality,
                )],
                understanding,
                SurfaceCoverage {
                    structural: quality,
                    semantic,
                    interaction: InteractionCoverage::Unavailable,
                },
                capabilities,
            )?;
        }
        Ok(())
    };
    add_children(
        SurfaceKind::FrameDocument,
        SurfaceEvidenceSource::Frame,
        EvidenceSource::Frames,
        context.boundaries.child_frames,
        "frame",
    )?;
    add_children(
        SurfaceKind::ShadowDocument,
        SurfaceEvidenceSource::ShadowDom,
        EvidenceSource::ShadowDom,
        context.boundaries.shadow_roots,
        "shadow",
    )?;
    add_children(
        SurfaceKind::Svg,
        SurfaceEvidenceSource::Svg,
        EvidenceSource::Svg,
        context.boundaries.svg_elements,
        "svg",
    )?;
    add_children(
        SurfaceKind::Media,
        SurfaceEvidenceSource::MediaMetadata,
        EvidenceSource::MediaMetadata,
        context.boundaries.media_elements,
        "media",
    )?;
    add_children(
        SurfaceKind::EmbeddedDocument,
        SurfaceEvidenceSource::EmbeddedDocument,
        EvidenceSource::EmbeddedDocument,
        context.boundaries.embedded_documents,
        "embedded",
    )?;
    add_children(
        SurfaceKind::Pdf,
        SurfaceEvidenceSource::Pdf,
        EvidenceSource::Pdf,
        context.boundaries.pdf_documents,
        "pdf",
    )?;
    add_children(
        SurfaceKind::BrowserNative,
        SurfaceEvidenceSource::BrowserNative,
        EvidenceSource::BrowserNative,
        context.boundaries.native_surfaces,
        "native",
    )?;

    let canvas_2d = if context.boundaries.canvas_2d == 0
        && context.boundaries.webgl_canvases == 0
        && context.boundaries.webgpu_canvases == 0
    {
        context.boundaries.canvases
    } else {
        context.boundaries.canvas_2d
    };
    for (kind, count, prefix) in [
        (SurfaceKind::Canvas2d, canvas_2d, "canvas2d"),
        (
            SurfaceKind::Webgl,
            context.boundaries.webgl_canvases,
            "webgl",
        ),
        (
            SurfaceKind::Webgpu,
            context.boundaries.webgpu_canvases,
            "webgpu",
        ),
    ] {
        if !available(EvidenceSource::CanvasDetection) {
            continue;
        }
        for index in 0..count.min(256) {
            if surfaces.len() >= 256 {
                surface_bound_omitted = true;
                break;
            }
            add(
                &mut surfaces,
                format!("{prefix}_{index}"),
                parent_surface_id.clone(),
                kind.clone(),
                vec![evidence(
                    SurfaceEvidenceSource::CanvasDetection,
                    format!("observed {prefix} graphics boundary"),
                    CoverageLevel::Strong,
                )],
                UnderstandingLevel::CoordinateOnly,
                SurfaceCoverage {
                    structural: CoverageLevel::Partial,
                    semantic: CoverageLevel::Partial,
                    interaction: InteractionCoverage::CoordinateOnly,
                },
                vec![
                    SurfaceCapability::CoordinateAction,
                    SurfaceCapability::Capture,
                ],
            )?;
        }
    }
    if context.boundaries.truncated && surfaces.len() < 256 && available(EvidenceSource::Layout) {
        add(
            &mut surfaces,
            "opaque_boundary".into(),
            parent_surface_id,
            SurfaceKind::Opaque,
            vec![evidence(
                SurfaceEvidenceSource::Layout,
                "bounded observation omitted surface details".into(),
                CoverageLevel::Opaque,
            )],
            UnderstandingLevel::Opaque,
            SurfaceCoverage::OPAQUE,
            Vec::new(),
        )?;
    }
    if surface_bound_omitted && let Some(surface) = surfaces.first_mut() {
        surface.diagnostics.push(SurfaceDiagnostic {
            severity: DiagnosticSeverity::Warning,
            code: "surfaceBound".into(),
            message: "additional observed surfaces omitted at the contract bound".into(),
        });
    }
    if surfaces.is_empty() {
        return Ok(None);
    }
    let set = SurfaceSet {
        schema_version: SURFACE_SCHEMA_VERSION,
        surfaces,
    };
    set.validate()
        .map_err(|error| ExtractionContractError::new("surfaceSet", error.to_string()))?;
    Ok(Some(set))
}

fn validate_observation_boundaries(context: &PageContext) -> Result<(), ExtractionContractError> {
    let boundaries = &context.boundaries;
    if boundaries.scan_limit > 0 && boundaries.scanned_elements > boundaries.scan_limit {
        return Err(ExtractionContractError::new(
            "boundaries.scannedElements",
            "observed element count exceeds the browser boundary scan limit",
        ));
    }
    if boundaries.scanned_elements > 0 {
        let counters = [
            ("shadowRoots", boundaries.shadow_roots),
            ("childFrames", boundaries.child_frames),
            ("canvases", boundaries.canvases),
            ("svgElements", boundaries.svg_elements),
            ("mediaElements", boundaries.media_elements),
            ("embeddedDocuments", boundaries.embedded_documents),
            ("pdfDocuments", boundaries.pdf_documents),
        ];
        if let Some((name, _count)) = counters
            .into_iter()
            .find(|(_, count)| *count > boundaries.scanned_elements)
        {
            return Err(ExtractionContractError::new(
                format!("boundaries.{name}"),
                "surface count exceeds the observed element count",
            ));
        }
    }
    if boundaries.pdf_documents > boundaries.embedded_documents {
        return Err(ExtractionContractError::new(
            "boundaries.pdfDocuments",
            "PDF surface count cannot exceed embedded document count",
        ));
    }
    let classified_canvases = boundaries
        .canvas_2d
        .saturating_add(boundaries.webgl_canvases)
        .saturating_add(boundaries.webgpu_canvases);
    if classified_canvases > boundaries.canvases {
        return Err(ExtractionContractError::new(
            "boundaries.canvasClassification",
            "graphics subtype counts cannot exceed the observed canvas count",
        ));
    }
    Ok(())
}

struct EvidenceCollector {
    budgets: ExtractionBudgets,
    deadline: Instant,
    facts: Vec<EvidenceFact>,
    inspected_nodes: u32,
    omitted_facts: u32,
    text_bytes: u32,
    truncated: bool,
    timed_out: bool,
}

impl EvidenceCollector {
    fn new(budgets: ExtractionBudgets) -> Self {
        Self {
            deadline: Instant::now() + Duration::from_millis(budgets.max_duration_ms),
            budgets,
            facts: Vec::new(),
            inspected_nodes: 0,
            omitted_facts: 0,
            text_bytes: 0,
            truncated: false,
            timed_out: false,
        }
    }

    fn deadline_reached(&self) -> bool {
        Instant::now() >= self.deadline
    }

    fn mark_timeout(&mut self) {
        self.timed_out = true;
        self.mark_omitted();
    }

    fn allow_node(&mut self, depth: u16) -> bool {
        if self.deadline_reached() {
            self.mark_timeout();
            return false;
        }
        if depth >= self.budgets.max_depth || self.node_budget_exhausted() {
            self.mark_omitted();
            return false;
        }
        self.inspected_nodes = self.inspected_nodes.saturating_add(1);
        true
    }

    fn node_budget_exhausted(&self) -> bool {
        self.inspected_nodes >= self.budgets.max_nodes
    }

    fn mark_omitted(&mut self) {
        self.omitted_facts = self.omitted_facts.saturating_add(1);
        self.truncated = true;
    }

    fn push(&mut self, mut fact: EvidenceFact) {
        if self.deadline_reached() {
            self.mark_timeout();
            return;
        }
        fact.role = self.bound_text(fact.role.take());
        fact.name = self.bound_text(fact.name.take());
        fact.parent_role = self.bound_text(fact.parent_role.take());
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
        if bounded.is_empty() {
            self.truncated |= !value.is_empty();
            return None;
        }

        self.text_bytes = self.text_bytes.saturating_add(bounded.len() as u32);
        if bounded.len() < value.len() {
            self.truncated = true;
        }
        Some(bounded)
    }
}
fn observation_has_mutation_race(context: &PageContext) -> bool {
    !context.consistency.consistent
        || context.consistency.start_revision != context.consistency.end_revision
        || context.consistency.start_mutation_revision != context.consistency.end_mutation_revision
}

fn evidence_coverage(
    context: &PageContext,
    sources: &[EvidenceSource],
    missing_sources: &BTreeSet<EvidenceSource>,
    truncated: bool,
    timed_out: bool,
    inconsistent_observation: bool,
) -> EvidenceCoverage {
    let requested = |source| sources.contains(&source);
    let available = |source| requested(source) && !missing_sources.contains(&source);
    let mut structural = if available(EvidenceSource::Dom) && !truncated {
        EvidenceQuality::Strong
    } else if available(EvidenceSource::Dom) || available(EvidenceSource::Accessibility) {
        EvidenceQuality::Partial
    } else {
        EvidenceQuality::Opaque
    };
    let mut semantic = if available(EvidenceSource::Accessibility) && !truncated {
        EvidenceQuality::Strong
    } else if requested(EvidenceSource::Accessibility) {
        EvidenceQuality::Partial
    } else {
        EvidenceQuality::Opaque
    };
    if inconsistent_observation {
        if structural != EvidenceQuality::Opaque {
            structural = EvidenceQuality::Partial;
        }
        if semantic != EvidenceQuality::Opaque {
            semantic = EvidenceQuality::Partial;
        }
    }
    // Frames, shadow roots, and graphics are explicit boundaries with
    // structural or coordinate coverage; only an omitted boundary is opaque.
    let opaque_regions = u32::from(context.boundaries.truncated);
    let mut reasons = missing_sources
        .iter()
        .map(|source| format!("missingSource:{source:?}"))
        .collect::<Vec<_>>();
    if truncated {
        reasons.push("budgetTruncated".into());
    }
    if timed_out {
        reasons.push("timeBudgetExceeded".into());
    }
    if inconsistent_observation {
        reasons.push("mutationRace".into());
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
fn safe_parent_role(value: &str) -> Option<String> {
    let role = value.trim();
    if role.is_empty() || role.len() > 64 {
        return None;
    }
    let known_role = matches!(
        role.to_ascii_lowercase().as_str(),
        "alert"
            | "article"
            | "banner"
            | "button"
            | "cell"
            | "checkbox"
            | "columnheader"
            | "combobox"
            | "complementary"
            | "dialog"
            | "document"
            | "form"
            | "grid"
            | "heading"
            | "img"
            | "link"
            | "list"
            | "listbox"
            | "main"
            | "menu"
            | "menuitem"
            | "navigation"
            | "option"
            | "radio"
            | "region"
            | "row"
            | "rowheader"
            | "search"
            | "slider"
            | "spinbutton"
            | "status"
            | "switch"
            | "tab"
            | "table"
            | "textbox"
            | "toolbar"
            | "tree"
    );
    known_role.then(|| role.to_owned())
}

fn nearest_safe_parent_role(ancestor_path: &[String]) -> Option<String> {
    ancestor_path.iter().rev().find_map(|value| {
        let role = value
            .split_once(':')
            .map_or(value.as_str(), |(role, _)| role);
        safe_parent_role(role)
    })
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

fn control_in_region(
    control: &CompactInteractiveElement,
    region_root: Option<&CompactAxNode>,
) -> bool {
    let Some(region_root) = region_root else {
        return true;
    };
    control.ancestor_path.iter().any(|ancestor| {
        let (role, name) = ancestor.split_once(':').unwrap_or((ancestor, ""));
        role.eq_ignore_ascii_case(&region_root.role)
            && (region_root.name.is_empty() || name.eq_ignore_ascii_case(&region_root.name))
    })
}

fn semantic_source_matches(source: EvidenceSource, role: &str) -> bool {
    match source {
        EvidenceSource::Navigation => {
            matches!(role, "navigation" | "link" | "tab" | "menu" | "menuitem")
        }
        EvidenceSource::Tables => matches!(
            role,
            "table" | "grid" | "row" | "cell" | "gridcell" | "columnheader" | "rowheader"
        ),
        EvidenceSource::Collections => {
            matches!(role, "list" | "listitem" | "feed" | "article")
        }
        EvidenceSource::Dialogs => {
            matches!(role, "dialog" | "alertdialog" | "alert" | "status")
        }
        _ => false,
    }
}

fn collect_semantic_source(
    node: &CompactAxNode,
    source: EvidenceSource,
    collector: &mut EvidenceCollector,
    depth: u16,
    parent_role: Option<&str>,
) {
    if !collector.allow_node(depth) {
        return;
    }
    if semantic_source_matches(source, &node.role) {
        collector.push(EvidenceFact {
            source,
            kind: "semantic".into(),
            quality: EvidenceQuality::Confirmed,
            role: Some(node.role.clone()),
            name: Some(node.name.clone()),
            input_type: None,
            autocomplete: None,
            required: None,
            read_only: None,
            empty: None,
            checked: None,
            disabled: None,
            geometry_present: None,
            parent_role: parent_role.map(str::to_owned),
            relationship_hint: None,
        });
    }
    let next_parent_role = safe_parent_role(&node.role).or_else(|| parent_role.map(str::to_owned));
    for child in &node.children {
        if collector.node_budget_exhausted() {
            collector.mark_omitted();
            break;
        }
        collect_semantic_source(
            child,
            source,
            collector,
            depth.saturating_add(1),
            next_parent_role.as_deref(),
        );
    }
}

fn push_boundary_fact(
    collector: &mut EvidenceCollector,
    source: EvidenceSource,
    role: &str,
    name: String,
) {
    if !collector.allow_node(0) {
        return;
    }
    collector.push(EvidenceFact {
        source,
        kind: "boundary".into(),
        quality: EvidenceQuality::Strong,
        role: Some(role.into()),
        name: Some(name),
        input_type: None,
        autocomplete: None,
        required: None,
        read_only: None,
        empty: None,
        checked: None,
        disabled: None,
        geometry_present: None,
        parent_role: None,
        relationship_hint: None,
    });
}

fn collect_accessibility(
    node: &CompactAxNode,
    collector: &mut EvidenceCollector,
    depth: u16,
    parent_role: Option<&str>,
) {
    if !collector.allow_node(depth) {
        return;
    }
    if !node.interactive {
        collector.push(EvidenceFact {
            source: EvidenceSource::Accessibility,
            kind: "node".into(),
            quality: EvidenceQuality::Confirmed,
            role: Some(node.role.clone()),
            name: Some(node.name.clone()),
            input_type: None,
            autocomplete: None,
            required: None,
            read_only: None,
            empty: None,
            checked: None,
            disabled: None,
            geometry_present: None,
            parent_role: parent_role.map(str::to_owned),
            relationship_hint: None,
        });
    }
    let next_parent_role = if is_region_role(&node.role) {
        Some(node.role.as_str())
    } else {
        parent_role
    };
    for child in &node.children {
        if collector.node_budget_exhausted() {
            collector.mark_omitted();
            break;
        }
        collect_accessibility(child, collector, depth.saturating_add(1), next_parent_role);
    }
}

fn collect_accessibility_control(
    control: &CompactInteractiveElement,
    collector: &mut EvidenceCollector,
) {
    if matches!(
        control.role.to_ascii_lowercase().as_str(),
        "rootwebarea" | "webarea" | "document"
    ) {
        return;
    }
    let parent_role = nearest_safe_parent_role(&control.ancestor_path);
    collector.push(EvidenceFact {
        source: EvidenceSource::Accessibility,
        kind: "control".into(),
        quality: EvidenceQuality::Confirmed,
        role: Some(control.role.clone()),
        name: Some(control.name.clone()),
        input_type: control.input_type.clone(),
        autocomplete: control.autocomplete.clone(),
        required: Some(control.required),
        read_only: Some(control.read_only),
        empty: Some(control.empty),
        checked: control.checked,
        disabled: control.disabled,
        geometry_present: None,
        parent_role,
        relationship_hint: None,
    });
}

fn has_svg_node(node: &DomNode) -> bool {
    if node.node_name.eq_ignore_ascii_case("svg") {
        return true;
    }
    node.children.iter().any(has_svg_node)
}

fn svg_attribute(node: &DomNode, name: &str) -> Option<String> {
    node.attributes
        .as_chunks::<2>()
        .0
        .iter()
        .find(|pair| pair[0].eq_ignore_ascii_case(name))
        .map(|pair| pair[1].clone())
}

/// Extract a conservative semantic projection from structured SVG markup.
/// Geometry and paint are intentionally omitted; only bounded roles/names are
/// promoted to Web IR facts.
fn collect_svg_semantics(
    node: &DomNode,
    collector: &mut EvidenceCollector,
    depth: u16,
    inside_svg: bool,
) {
    if !collector.allow_node(depth) {
        return;
    }
    let tag = node.node_name.to_ascii_lowercase();
    let in_svg = inside_svg || tag == "svg";
    if in_svg {
        let (role, name) = match tag.as_str() {
            "svg" => (
                "region",
                svg_attribute(node, "aria-label")
                    .or_else(|| svg_attribute(node, "title"))
                    .or_else(|| svg_attribute(node, "id")),
            ),
            "a" => (
                "link",
                svg_attribute(node, "aria-label").or_else(|| svg_attribute(node, "href")),
            ),
            "text" | "title" | "desc" => (
                "text",
                (!node.node_value.is_empty()).then(|| node.node_value.clone()),
            ),
            _ => ("", None),
        };
        if !role.is_empty() {
            collector.push(EvidenceFact {
                source: EvidenceSource::Svg,
                kind: tag.clone(),
                quality: EvidenceQuality::Confirmed,
                role: Some(role.into()),
                name,
                parent_role: None,
                relationship_hint: None,
                input_type: None,
                autocomplete: None,
                required: None,
                read_only: None,
                empty: None,
                checked: None,
                disabled: None,
                geometry_present: Some(node.bounding_box.is_some()),
            });
        }
    }
    for child in &node.children {
        if collector.node_budget_exhausted() {
            collector.mark_omitted();
            break;
        }
        collect_svg_semantics(child, collector, depth.saturating_add(1), in_svg);
    }
}

fn collect_dom(node: &DomNode, collector: &mut EvidenceCollector, depth: u16) {
    if !collector.allow_node(depth) {
        return;
    }
    collector.push(EvidenceFact {
        source: EvidenceSource::Dom,
        kind: "element".into(),
        quality: EvidenceQuality::Confirmed,
        role: None,
        name: Some(node.node_name.clone()),
        input_type: None,
        autocomplete: None,
        required: None,
        read_only: None,
        empty: None,
        checked: None,
        disabled: None,
        geometry_present: None,
        parent_role: None,
        relationship_hint: None,
    });
    for child in &node.children {
        if collector.node_budget_exhausted() {
            collector.mark_omitted();
            break;
        }
        collect_dom(child, collector, depth.saturating_add(1));
    }
}

fn collect_layout(
    node: &DomNode,
    collector: &mut EvidenceCollector,
    source: EvidenceSource,
    depth: u16,
) {
    if !collector.allow_node(depth) {
        return;
    }
    collector.push(EvidenceFact {
        source,
        kind: "geometry".into(),
        quality: EvidenceQuality::Strong,
        role: None,
        name: Some(node.node_name.clone()),
        input_type: None,
        autocomplete: None,
        required: None,
        read_only: None,
        empty: None,
        checked: None,
        disabled: None,
        geometry_present: Some(node.bounding_box.is_some()),
        parent_role: None,
        relationship_hint: None,
    });
    for child in &node.children {
        if collector.node_budget_exhausted() {
            collector.mark_omitted();
            break;
        }
        collect_layout(child, collector, source, depth.saturating_add(1));
    }
}

fn trim_to_output_budget(
    evidence: &mut ExtractionEvidence,
    max_output_bytes: u32,
) -> Result<(), ExtractionContractError> {
    let mut removed_fact = false;
    let mut removed_surface_set = false;
    while serde_json::to_vec(evidence)
        .map_err(|error| ExtractionContractError::new("$", error.to_string()))?
        .len()
        > max_output_bytes as usize
    {
        if evidence.facts.pop().is_none() {
            if evidence.surface_set.take().is_some() {
                removed_surface_set = true;
                evidence.limits.truncated = true;
                evidence
                    .coverage
                    .reasons
                    .push("surfaceSetBudgetTruncated".into());
                evidence.coverage.reasons.truncate(16);
                continue;
            }
            return Err(ExtractionContractError::new(
                "budgets.maxOutputBytes",
                "output budget is too small for extraction metadata",
            ));
        }
        if !removed_fact {
            evidence.coverage.structural = downgrade_coverage(evidence.coverage.structural);
            evidence.coverage.semantic = downgrade_coverage(evidence.coverage.semantic);
            if !evidence
                .coverage
                .reasons
                .iter()
                .any(|reason| reason == "budgetTruncated")
            {
                evidence.coverage.reasons.push("budgetTruncated".into());
                evidence.coverage.reasons.truncate(16);
            }
        }
        removed_fact = true;
        evidence.limits.omitted_facts = evidence.limits.omitted_facts.saturating_add(1);
        evidence.limits.truncated = true;
        evidence.limits.text_bytes = evidence.facts.iter().map(fact_text_bytes).sum::<u32>();
    }
    evidence.limits.text_bytes = evidence.facts.iter().map(fact_text_bytes).sum::<u32>();
    if removed_fact || removed_surface_set {
        evidence.coverage.structural = downgrade_coverage(evidence.coverage.structural);
        evidence.coverage.semantic = downgrade_coverage(evidence.coverage.semantic);
    }
    Ok(())
}

fn downgrade_coverage(quality: EvidenceQuality) -> EvidenceQuality {
    if quality == EvidenceQuality::Opaque {
        EvidenceQuality::Opaque
    } else {
        EvidenceQuality::Partial
    }
}

fn fact_text_bytes(fact: &EvidenceFact) -> u32 {
    [
        fact.role.as_deref(),
        fact.name.as_deref(),
        fact.parent_role.as_deref(),
        fact.input_type.as_deref(),
    ]
    .into_iter()
    .flatten()
    .map(|value| value.len() as u32)
    .sum()
}

fn truncate_utf8(value: &str, max_bytes: usize) -> String {
    if value.len() <= max_bytes {
        return value.to_owned();
    }
    let mut end = max_bytes;
    while !value.is_char_boundary(end) {
        end -= 1;
    }
    value[..end].to_owned()
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
    use crate::surfaces::SEMANTIC_BRIDGE_SCHEMA_VERSION;
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
                autocomplete: Some("name".into()),
                value: Some("secret-value".into()),
                checked: None,
                disabled: None,
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
    fn extraction_collects_supported_sources_without_confusing_absence_and_missing_evidence() {
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
        assert!(evidence.limits.missing_sources.is_empty());
        assert!(evidence.facts.iter().any(|fact| {
            fact.source == EvidenceSource::Accessibility
                && fact.role.as_deref() == Some("textbox")
                && fact.name.as_deref() == Some("Full name")
                && fact.input_type.as_deref() == Some("text")
        }));
        assert!(
            !serde_json::to_string(&evidence)
                .unwrap()
                .contains("secret-value")
        );
    }

    #[test]
    fn extraction_reconciles_compatible_multi_surface_observation() {
        let mut context = page_context();
        context.boundaries = crate::browser::session::ObservationBoundarySummary {
            canvases: 3,
            canvas_2d: 1,
            webgl_canvases: 1,
            webgpu_canvases: 1,
            child_frames: 1,
            shadow_roots: 1,
            svg_elements: 1,
            media_elements: 1,
            embedded_documents: 1,
            pdf_documents: 1,
            native_surfaces: 1,
            truncated: true,
            ..Default::default()
        };
        let mut request = request();
        request.sources = vec![
            EvidenceSource::Accessibility,
            EvidenceSource::Dom,
            EvidenceSource::Layout,
            EvidenceSource::Frames,
            EvidenceSource::ShadowDom,
            EvidenceSource::Svg,
            EvidenceSource::CanvasDetection,
            EvidenceSource::MediaMetadata,
            EvidenceSource::EmbeddedDocument,
            EvidenceSource::Pdf,
            EvidenceSource::BrowserNative,
        ];
        let evidence = extract_page_context(&context, &request).unwrap();
        let surfaces = evidence.surface_set.as_ref().expect("surface evidence");
        surfaces.validate().unwrap();
        assert!(
            surfaces
                .surfaces
                .iter()
                .any(|surface| surface.kind == SurfaceKind::Webgl)
        );
        assert!(
            surfaces
                .surfaces
                .iter()
                .any(|surface| surface.kind == SurfaceKind::Webgpu)
        );
        assert!(
            surfaces
                .surfaces
                .iter()
                .any(|surface| surface.kind == SurfaceKind::BrowserNative)
        );
        let embedded = surfaces
            .surfaces
            .iter()
            .find(|surface| surface.kind == SurfaceKind::EmbeddedDocument)
            .expect("embedded boundary");
        assert_eq!(embedded.understanding, UnderstandingLevel::Structural);
        assert!(embedded.diagnostics.iter().any(|diagnostic| {
            diagnostic.code == "semanticUnavailable"
                && diagnostic.severity == DiagnosticSeverity::Warning
        }));
        let opaque = surfaces
            .surfaces
            .iter()
            .find(|surface| surface.kind == SurfaceKind::Opaque)
            .expect("bounded omission should remain opaque");
        assert_eq!(opaque.understanding, UnderstandingLevel::Opaque);
        assert!(opaque.capabilities.is_empty());
        let ir = crate::web_ir::reconcile_evidence(&evidence).unwrap();
        for (surface_id, expected_kind) in [
            ("canvas2d_0", SurfaceKind::Canvas2d),
            ("embedded_0", SurfaceKind::EmbeddedDocument),
            ("pdf_0", SurfaceKind::Pdf),
        ] {
            let entity_id = ir
                .entity_details
                .iter()
                .find(|(_, details)| details.surface_id.as_deref() == Some(surface_id))
                .map(|(id, _)| id)
                .expect("surface entity details");
            assert_eq!(
                ir.entity_details[entity_id].surface_kind.as_ref(),
                Some(&expected_kind)
            );
            assert!(ir.relationships.iter().any(|relationship| {
                relationship.from == "page" && relationship.to == *entity_id
            }));
            assert!(ir.entity_details[entity_id].supported_actions.is_empty());
        }
        assert!(ir.diagnostics.iter().any(|diagnostic| {
            diagnostic.contains("embedded_0") && diagnostic.contains("semanticUnavailable")
        }));
        assert!(ir.surface_set.is_some());
        let mut malformed = evidence.clone();
        malformed.surface_set.as_mut().unwrap().schema_version = 99;
        assert!(crate::web_ir::reconcile_evidence(&malformed).is_err());
        let mut unauthorized = opaque.clone();
        unauthorized
            .capabilities
            .push(SurfaceCapability::SemanticAction);
        assert!(unauthorized.validate().is_err());
        let mut reversed = ir.clone();
        if let Some(surface_set) = &mut reversed.surface_set {
            surface_set.surfaces.reverse();
            for surface in &mut surface_set.surfaces {
                surface.evidence.reverse();
            }
        }
        assert_eq!(
            ir.to_canonical_json().unwrap(),
            reversed.to_canonical_json().unwrap()
        );
        let mut next_context = context.clone();
        next_context.accessibility.revision = 8;
        let next = extract_page_context(&next_context, &request).unwrap();
        let next_ir = crate::web_ir::reconcile_evidence(&next).unwrap();
        assert!(ir.diff(&next_ir).unwrap().surface_set_changed);
        assert!(ir.to_canonical_json().unwrap().contains("surfaceSet"));
    }

    #[test]
    fn extraction_rejects_inconsistent_surface_boundary_counts() {
        let mut context = page_context();
        context.boundaries.canvases = 1;
        context.boundaries.webgl_canvases = 2;
        let error = extract_page_context(&context, &request()).unwrap_err();
        assert_eq!(error.path, "boundaries.canvasClassification");

        context.boundaries.webgl_canvases = 0;
        context.boundaries.pdf_documents = 1;
        let error = extract_page_context(&context, &request()).unwrap_err();
        assert_eq!(error.path, "boundaries.pdfDocuments");
    }

    #[test]
    fn extraction_classifies_specialized_surface_without_document_root() {
        let mut context = page_context();
        context.dom = None;
        context.accessibility.roots.clear();
        context.boundaries.canvases = 1;
        context.boundaries.canvas_2d = 0;
        let mut request = request();
        request.sources = vec![EvidenceSource::CanvasDetection];
        let evidence = extract_page_context(&context, &request).unwrap();
        let surfaces = evidence.surface_set.expect("canvas surface");
        assert_eq!(surfaces.surfaces.len(), 1);
        assert_eq!(surfaces.surfaces[0].kind, SurfaceKind::Canvas2d);
        assert!(surfaces.surfaces[0].parent_surface_id.is_none());
        surfaces.validate().unwrap();
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
        context.accessibility.interactive[0].ancestor_path = vec![
            "form:Example form".into(),
            "label:Nested field label".into(),
        ];
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
            |relationship| relationship.kind == crate::web_ir::WebIrRelationshipKind::Controls
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
    fn extraction_supports_observed_region_and_active_frame_scopes() {
        let mut region_request = request();
        region_request.scope = ExtractionScope::Region {
            region_id: "region_form_1".into(),
        };
        let region = extract_page_context(&page_context(), &region_request).unwrap();
        assert!(!region.facts.is_empty());
        assert_eq!(region.scope, region_request.scope);

        let mut frame_request = request();
        frame_request.scope = ExtractionScope::Frame {
            frame_id: "frame-1".into(),
        };
        assert!(extract_page_context(&page_context(), &frame_request).is_ok());
        frame_request.scope = ExtractionScope::Frame {
            frame_id: "other-frame".into(),
        };
        assert_eq!(
            extract_page_context(&page_context(), &frame_request)
                .unwrap_err()
                .path,
            "scope.frameId"
        );
    }

    #[test]
    fn extraction_covers_semantic_boundary_and_probe_sources() {
        use crate::browser::session::ViewportState;

        let mut context = page_context();
        context.accessibility.roots[0].children.extend([
            CompactAxNode {
                role: "navigation".into(),
                name: "Primary".into(),
                children: vec![CompactAxNode {
                    role: "link".into(),
                    name: "Home".into(),
                    children: Vec::new(),
                    interactive: true,
                }],
                interactive: false,
            },
            CompactAxNode {
                role: "table".into(),
                name: "Orders".into(),
                children: vec![CompactAxNode {
                    role: "row".into(),
                    name: "Order 1".into(),
                    children: Vec::new(),
                    interactive: false,
                }],
                interactive: false,
            },
            CompactAxNode {
                role: "list".into(),
                name: "Results".into(),
                children: vec![CompactAxNode {
                    role: "listitem".into(),
                    name: "Result 1".into(),
                    children: Vec::new(),
                    interactive: false,
                }],
                interactive: false,
            },
            CompactAxNode {
                role: "dialog".into(),
                name: "Confirm".into(),
                children: Vec::new(),
                interactive: false,
            },
        ]);
        context.boundaries.child_frames = 1;
        context.boundaries.shadow_roots = 1;
        context.boundaries.viewport = Some(ViewportState {
            width: 800.0,
            height: 600.0,
            document_width: 1200.0,
            document_height: 1800.0,
            ..ViewportState::default()
        });
        let mut request = request();
        request.sources = vec![
            EvidenceSource::Navigation,
            EvidenceSource::Tables,
            EvidenceSource::Collections,
            EvidenceSource::Dialogs,
            EvidenceSource::Frames,
            EvidenceSource::ShadowDom,
            EvidenceSource::BoundedProbe,
        ];
        let evidence = extract_page_context(&context, &request).unwrap();
        assert!(evidence.limits.missing_sources.is_empty());
        for source in request.sources {
            assert!(
                evidence.facts.iter().any(|fact| fact.source == source),
                "missing facts for {source:?}"
            );
        }
    }
    #[test]
    fn observed_absence_of_frames_and_shadow_roots_is_not_missing_evidence() {
        let context = page_context();
        let mut request = request();
        request.sources = vec![EvidenceSource::Frames, EvidenceSource::ShadowDom];

        let evidence = extract_page_context(&context, &request).unwrap();

        assert!(evidence.facts.is_empty());
        assert!(evidence.limits.missing_sources.is_empty());
    }
    #[test]
    fn extraction_reports_explicit_opaque_boundaries_without_counting_structural_boundaries() {
        let mut context = page_context();
        context.boundaries.child_frames = 1;
        let evidence = extract_page_context(&context, &request()).unwrap();
        assert_eq!(evidence.coverage.opaque_regions, 0);
        assert!(
            evidence
                .coverage
                .reasons
                .iter()
                .any(|reason| reason == "frameBoundary")
        );
        assert_ne!(evidence.coverage.semantic, EvidenceQuality::Opaque);

        context.boundaries.truncated = true;
        let mut opaque_request = request();
        opaque_request.sources.push(EvidenceSource::Layout);
        let opaque = extract_page_context(&context, &opaque_request).unwrap();
        assert_eq!(opaque.coverage.opaque_regions, 1);
        assert!(opaque.surface_set.as_ref().is_some_and(|surface_set| {
            surface_set
                .surfaces
                .iter()
                .any(|surface| surface.kind == SurfaceKind::Opaque)
        }));
    }
    #[test]
    fn extraction_output_budget_truncation_updates_coverage_and_text_bytes() {
        let context = page_context();
        let mut request = request();
        request.sources = vec![EvidenceSource::Accessibility, EvidenceSource::Dom];
        let complete = extract_page_context(&context, &request).unwrap();
        let complete_size = serde_json::to_vec(&complete).unwrap().len();
        request.budgets.max_output_bytes = (complete_size - 1) as u32;

        let bounded = extract_page_context(&context, &request).unwrap();
        assert!(bounded.limits.truncated);
        assert!(bounded.limits.omitted_facts > 0);
        assert_eq!(
            bounded.limits.text_bytes,
            bounded.facts.iter().map(fact_text_bytes).sum::<u32>()
        );
        assert_eq!(bounded.coverage.structural, EvidenceQuality::Partial);
        assert!(
            bounded
                .coverage
                .reasons
                .iter()
                .any(|reason| reason == "budgetTruncated")
        );
    }
    #[test]
    fn extraction_marks_surface_set_omission_when_output_budget_requires_it() {
        let context = page_context();
        let mut request = request();
        request.sources = vec![
            EvidenceSource::Accessibility,
            EvidenceSource::Dom,
            EvidenceSource::Frames,
        ];
        let complete = extract_page_context(&context, &request).unwrap();
        assert!(complete.surface_set.is_some());
        let mut facts_removed = complete.clone();
        facts_removed.facts.clear();
        let minimum_with_surfaces = serde_json::to_vec(&facts_removed).unwrap().len();
        request.budgets.max_output_bytes = (minimum_with_surfaces - 1) as u32;

        let bounded = extract_page_context(&context, &request).unwrap();
        assert!(bounded.surface_set.is_none());
        assert!(bounded.limits.truncated);
        assert!(
            bounded
                .coverage
                .reasons
                .iter()
                .any(|reason| reason == "surfaceSetBudgetTruncated")
        );
    }

    #[test]
    fn extraction_distinguishes_absent_forms_from_missing_accessibility() {
        let mut request = request();
        request.sources = vec![EvidenceSource::Forms];

        let mut absent_context = page_context();
        absent_context.accessibility.interactive.clear();
        let absent = extract_page_context(&absent_context, &request).unwrap();
        assert!(absent.facts.is_empty());
        assert!(absent.limits.missing_sources.is_empty());

        let mut missing_context = absent_context;
        missing_context
            .incomplete
            .push(crate::browser::session::ObservationIncompleteReason::AccessibilityNode);
        let missing = extract_page_context(&missing_context, &request).unwrap();
        assert!(missing.facts.is_empty());
        assert_eq!(missing.limits.missing_sources, vec![EvidenceSource::Forms]);
        assert!(
            missing
                .coverage
                .reasons
                .iter()
                .any(|reason| reason == "missingSource:Forms")
        );
    }

    #[test]
    fn extraction_treats_truncated_controls_as_missing_forms() {
        let mut context = page_context();
        context
            .incomplete
            .push(crate::browser::session::ObservationIncompleteReason::Control);
        let mut request = request();
        request.sources = vec![EvidenceSource::Forms];

        let evidence = extract_page_context(&context, &request).unwrap();
        assert!(evidence.facts.is_empty());
        assert_eq!(evidence.limits.missing_sources, vec![EvidenceSource::Forms]);
    }

    #[test]
    fn extraction_enforces_duration_budget_at_collection_checkpoint() {
        let mut collector = EvidenceCollector::new(ExtractionBudgets::default());
        collector.deadline = Instant::now() - Duration::from_millis(1);

        assert!(!collector.allow_node(0));
        assert!(collector.timed_out);
        assert!(collector.truncated);
        assert_eq!(collector.omitted_facts, 1);
    }

    #[test]
    fn extraction_marks_mutation_races_as_partial_coverage() {
        let mut context = page_context();
        context.consistency.consistent = false;
        let evidence = extract_page_context(&context, &request()).unwrap();
        assert_eq!(evidence.coverage.structural, EvidenceQuality::Partial);
        assert_eq!(evidence.coverage.semantic, EvidenceQuality::Partial);
        assert!(
            evidence
                .coverage
                .reasons
                .iter()
                .any(|reason| reason == "mutationRace")
        );
    }

    #[test]
    fn extraction_is_non_mutating_and_preserves_fact_source_provenance() {
        let context = page_context();
        let before = serde_json::to_vec(&context).unwrap();
        let mut request = request();
        request.sources = vec![
            EvidenceSource::Accessibility,
            EvidenceSource::Dom,
            EvidenceSource::Forms,
            EvidenceSource::Layout,
        ];
        let evidence = extract_page_context(&context, &request).unwrap();
        assert_eq!(serde_json::to_vec(&context).unwrap(), before);
        assert!(
            evidence
                .facts
                .iter()
                .all(|fact| request.sources.contains(&fact.source))
        );
        assert!(
            evidence
                .facts
                .iter()
                .any(|fact| fact.source == EvidenceSource::Dom)
        );
        assert!(
            evidence
                .facts
                .iter()
                .any(|fact| fact.source == EvidenceSource::Forms)
        );
        assert!(
            evidence
                .facts
                .iter()
                .any(|fact| fact.source == EvidenceSource::Layout)
        );
    }
    #[test]
    fn utf8_truncation_never_exceeds_the_byte_budget() {
        assert_eq!(truncate_utf8("é", 1), "");
        assert_eq!(truncate_utf8("aé", 2), "a");
        assert_eq!(truncate_utf8("aé", 3), "aé");
        assert!(truncate_utf8("🙂🙂", 7).len() <= 7);
    }
    #[test]
    fn semantic_bridge_extracts_svg_fact_and_rejects_stale_revision() {
        let mut grants = BridgeGrantRegistry::default();
        grants
            .insert(crate::surfaces::BridgeCapabilityGrant {
                token: "svg-grant".into(),
                origin: "https://example.test".into(),
                capabilities: vec![SurfaceCapability::ReadStructure],
            })
            .unwrap();
        let bridge = SemanticBridge {
            schema_version: SEMANTIC_BRIDGE_SCHEMA_VERSION,
            bridge_version: "1".into(),
            origin: "https://example.test".into(),
            grant_token: "svg-grant".into(),
            revision: SurfaceRevision(11),
            surface_id: SurfaceId::new("svg-bridge").unwrap(),
            surface_kind: SurfaceKind::Svg,
            capabilities: vec![SurfaceCapability::ReadStructure],
            entities: vec![crate::surfaces::SemanticBridgeEntity {
                id: "title".into(),
                kind: "text".into(),
                role: Some("text".into()),
                name: Some("Chart".into()),
                quality: CoverageLevel::Strong,
                allowed_actions: vec![crate::surfaces::BridgeAction::Read],
                parent_id: None,
            }],
        };
        let evidence = extract_semantic_bridge(
            &bridge,
            &grants,
            "https://example.test",
            SurfaceRevision(11),
        )
        .unwrap();
        let ir = crate::web_ir::reconcile_evidence_with_grants(&evidence, &grants).unwrap();
        assert!(
            ir.entities
                .iter()
                .any(|entity| entity.kind == crate::web_ir::WebIrEntityKind::Text)
        );
        assert_eq!(ir.revision, 11);
        assert!(
            extract_semantic_bridge(
                &bridge,
                &grants,
                "https://example.test",
                SurfaceRevision(12),
            )
            .is_err()
        );
    }
}
