//! Native semantic observation to Glass Web IR extraction.
//!
//! The native engine does not expose a browser-vendor DOM protocol. This
//! adapter therefore converts its revisioned semantic observation into the
//! same source-labelled evidence contract used by the Chromium session. The
//! conversion is deliberately bounded and keeps unsupported evidence classes
//! explicit instead of manufacturing DOM-level certainty.

use super::session::{
    BrowserResult, SemanticObservation, SemanticRegion, SemanticRegionKind, SemanticTarget,
};
use crate::extraction::{
    EvidenceCoverage, EvidenceFact, EvidenceQuality, EvidenceSource, ExtractionContractError,
    ExtractionEvidence, ExtractionEvidenceLimits, ExtractionRequest, ExtractionScope,
};
use crate::web_ir::{GlassWebIrV1, WebIrDocument, reconcile_evidence};
use std::collections::BTreeSet;

const MAX_NATIVE_FACT_TEXT_BYTES: usize = 256;
const MAX_NATIVE_ROLE_BYTES: usize = 64;
const MAX_NATIVE_TEXT_BYTES: usize = 8 * 1024;

/// Convert one already-captured native semantic observation into validated
/// Glass Web IR. No browser operation is dispatched by this function.
pub(crate) fn extract(
    observation: &SemanticObservation,
    request: &ExtractionRequest,
) -> BrowserResult<GlassWebIrV1> {
    request.validate()?;
    observation.validate()?;
    let regions = scoped_regions(observation, &request.scope)?;
    let mut builder = NativeEvidenceBuilder::new(request);
    let include_targets = request.budgets.max_depth > 1;

    if !include_targets
        && request.sources.iter().any(|source| {
            matches!(
                source,
                EvidenceSource::Accessibility | EvidenceSource::Forms | EvidenceSource::Navigation
            )
        })
    {
        let omitted_targets = regions
            .iter()
            .map(|region| region.targets.len())
            .sum::<usize>();
        builder.omit(omitted_targets);
    }

    for source in request.sources.iter().copied() {
        match source {
            EvidenceSource::Accessibility => {
                for region in &regions {
                    builder.push(region_fact(region));
                    if include_targets {
                        for target in &region.targets {
                            builder.push_target(source, target);
                        }
                    }
                }
            }
            EvidenceSource::Forms => {
                for region in &regions {
                    if include_targets {
                        for target in &region.targets {
                            if is_form_target(target) {
                                builder.push_target(source, target);
                            }
                        }
                    }
                }
            }
            EvidenceSource::Navigation => {
                for region in &regions {
                    if include_targets {
                        for target in &region.targets {
                            if is_navigation_target(target) {
                                builder.push_target(source, target);
                            }
                        }
                    }
                }
            }
            EvidenceSource::Frames => {
                let mut frame_ids = BTreeSet::from([observation.route.frame_id.clone()]);
                for region in &regions {
                    for target in &region.targets {
                        if let Some(frame_id) = &target.frame_id {
                            frame_ids.insert(frame_id.clone());
                        }
                    }
                }
                for frame_id in frame_ids {
                    builder.push(EvidenceFact {
                        source,
                        kind: "boundary".into(),
                        quality: EvidenceQuality::Strong,
                        role: Some("frame".into()),
                        name: Some(frame_id),
                        parent_role: None,
                        relationship_hint: None,
                        input_type: None,
                        autocomplete: None,
                        required: None,
                        read_only: None,
                        empty: None,
                        checked: None,
                        disabled: None,
                        geometry_present: None,
                    });
                }
            }
            EvidenceSource::BrowserNative => {
                if let Some(viewport) = observation.limits.viewport {
                    builder.push(EvidenceFact {
                        source,
                        kind: "boundary".into(),
                        quality: EvidenceQuality::Strong,
                        role: Some("viewport".into()),
                        name: Some(format!(
                            "viewport {:.0}x{:.0} scroll {:.0},{:.0}",
                            viewport.width, viewport.height, viewport.scroll_x, viewport.scroll_y
                        )),
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
            }
            EvidenceSource::BoundedProbe => {
                if let Some(text) = scoped_text(observation, &regions, &request.scope)
                    .filter(|text| !text.is_empty())
                {
                    builder.push(EvidenceFact {
                        source,
                        kind: "text".into(),
                        quality: EvidenceQuality::Strong,
                        role: Some("text".into()),
                        name: Some(text),
                        parent_role: None,
                        relationship_hint: None,
                        input_type: None,
                        autocomplete: None,
                        required: None,
                        read_only: None,
                        empty: None,
                        checked: None,
                        disabled: None,
                        geometry_present: None,
                    });
                }
            }
            unsupported => {
                builder.missing_sources.insert(unsupported);
            }
        }
    }

    let NativeEvidenceBuilder {
        facts,
        omitted_facts,
        text_bytes,
        truncated,
        missing_sources,
        interactive_entities_observed,
        ..
    } = builder;
    let mut evidence = ExtractionEvidence {
        schema_version: crate::extraction::EXTRACTION_CONTRACT_SCHEMA_VERSION,
        revision: observation.revision,
        scope: request.scope.clone(),
        sources: request.sources.clone(),
        facts,
        limits: ExtractionEvidenceLimits {
            truncated: truncated || observation.limits.truncated,
            omitted_facts,
            text_bytes,
            missing_sources: missing_sources.into_iter().collect(),
        },
        coverage: EvidenceCoverage {
            structural: EvidenceQuality::Strong,
            semantic: EvidenceQuality::Strong,
            interactive_entities_observed,
            opaque_regions: 0,
            reasons: Vec::new(),
        },
        surface_set: None,
    };
    if evidence.limits.truncated {
        evidence.coverage.structural = EvidenceQuality::Partial;
        evidence.coverage.semantic = EvidenceQuality::Partial;
        evidence
            .coverage
            .reasons
            .push("native semantic observation was bounded".into());
    }
    if !evidence.limits.missing_sources.is_empty() {
        evidence.coverage.structural = EvidenceQuality::Partial;
        evidence.coverage.semantic = EvidenceQuality::Partial;
        evidence
            .coverage
            .reasons
            .push("requested evidence sources are unavailable in the native projection".into());
    }
    trim_to_output_budget(&mut evidence, request.budgets.max_output_bytes)?;

    let mut ir = reconcile_evidence(&evidence)?;
    ir.document = WebIrDocument {
        revision: ir.revision,
        url: Some(strip_query_and_fragment(&observation.page.url)),
        title: (!observation.page.title.is_empty())
            .then(|| truncate_utf8(&observation.page.title, 512)),
        kind: Some(
            serde_json::to_string(&observation.page.kind)?
                .trim_matches('"')
                .into(),
        ),
        ready_state: Some("complete".into()),
    };
    ir.validate()?;
    Ok(ir)
}

struct NativeEvidenceBuilder {
    max_nodes: usize,
    max_text_bytes: usize,
    facts: Vec<EvidenceFact>,
    omitted_facts: u32,
    text_bytes: u32,
    truncated: bool,
    missing_sources: BTreeSet<EvidenceSource>,
    target_keys: BTreeSet<String>,
    interactive_entities_observed: u32,
}

impl NativeEvidenceBuilder {
    fn new(request: &ExtractionRequest) -> Self {
        Self {
            max_nodes: request.budgets.max_nodes as usize,
            max_text_bytes: request.budgets.max_text_bytes as usize,
            facts: Vec::new(),
            omitted_facts: 0,
            text_bytes: 0,
            truncated: false,
            missing_sources: BTreeSet::new(),
            target_keys: BTreeSet::new(),
            interactive_entities_observed: 0,
        }
    }

    fn push_target(&mut self, source: EvidenceSource, target: &SemanticTarget) {
        let key = format!(
            "{}:{}",
            target.frame_id.as_deref().unwrap_or_default(),
            target.reference
        );
        if self.target_keys.insert(key) {
            self.interactive_entities_observed =
                self.interactive_entities_observed.saturating_add(1);
        }
        self.push(target_fact(source, target));
    }

    fn push(&mut self, mut fact: EvidenceFact) {
        if self.facts.len() >= self.max_nodes {
            self.omit(1);
            return;
        }
        fact.role = self.bound_text(fact.role.take(), MAX_NATIVE_ROLE_BYTES);
        fact.name = self.bound_text(fact.name.take(), MAX_NATIVE_FACT_TEXT_BYTES);
        fact.parent_role = self.bound_text(fact.parent_role.take(), MAX_NATIVE_ROLE_BYTES);
        fact.input_type = self.bound_text(fact.input_type.take(), MAX_NATIVE_ROLE_BYTES);
        self.facts.push(fact);
    }

    fn bound_text(&mut self, value: Option<String>, max_bytes: usize) -> Option<String> {
        let value = value?;
        let remaining = self.max_text_bytes.saturating_sub(self.text_bytes as usize);
        if remaining == 0 {
            self.truncated |= !value.is_empty();
            return None;
        }
        let bounded = truncate_utf8(&value, remaining.min(max_bytes));
        self.text_bytes = self.text_bytes.saturating_add(bounded.len() as u32);
        self.truncated |= bounded.len() < value.len();
        (!bounded.is_empty()).then_some(bounded)
    }

    fn omit(&mut self, count: usize) {
        self.omitted_facts = self
            .omitted_facts
            .saturating_add(count.min(u32::MAX as usize) as u32);
        self.truncated |= count > 0;
    }
}

fn scoped_regions<'a>(
    observation: &'a SemanticObservation,
    scope: &ExtractionScope,
) -> Result<Vec<&'a SemanticRegion>, ExtractionContractError> {
    match scope {
        ExtractionScope::Document | ExtractionScope::Frame { .. } => {
            Ok(observation.regions.iter().collect())
        }
        ExtractionScope::Region { region_id } => observation
            .regions
            .iter()
            .find(|region| region.id == *region_id)
            .map(|region| vec![region])
            .ok_or_else(|| {
                ExtractionContractError::new(
                    "scope.regionId",
                    format!(
                        "region {region_id:?} is not present at revision {}",
                        observation.revision
                    ),
                )
            }),
    }
}

fn region_fact(region: &SemanticRegion) -> EvidenceFact {
    EvidenceFact {
        source: EvidenceSource::Accessibility,
        kind: "semantic".into(),
        quality: EvidenceQuality::Strong,
        role: Some(region_role(region.kind).into()),
        name: Some(region.label.clone()),
        parent_role: None,
        relationship_hint: None,
        input_type: None,
        autocomplete: None,
        required: None,
        read_only: None,
        empty: None,
        checked: None,
        disabled: None,
        geometry_present: None,
    }
}

fn target_fact(source: EvidenceSource, target: &SemanticTarget) -> EvidenceFact {
    EvidenceFact {
        source,
        kind: if source == EvidenceSource::Navigation {
            "semantic".into()
        } else {
            "control".into()
        },
        quality: if source == EvidenceSource::Forms {
            EvidenceQuality::Strong
        } else {
            EvidenceQuality::Confirmed
        },
        role: Some(target.role.clone()),
        name: Some(target.name.clone()),
        parent_role: None,
        relationship_hint: None,
        input_type: target.input_type.clone(),
        autocomplete: None,
        required: target.required,
        read_only: target.read_only,
        empty: target.empty,
        checked: target.checked,
        disabled: target.disabled,
        geometry_present: None,
    }
}

fn is_form_target(target: &SemanticTarget) -> bool {
    matches!(
        target.role.as_str(),
        "checkbox"
            | "combobox"
            | "file"
            | "listbox"
            | "radio"
            | "slider"
            | "spinbutton"
            | "switch"
            | "textbox"
    )
}

fn is_navigation_target(target: &SemanticTarget) -> bool {
    matches!(target.role.as_str(), "link" | "menuitem" | "tab")
}

fn region_role(kind: SemanticRegionKind) -> &'static str {
    match kind {
        SemanticRegionKind::Navigation => "navigation",
        SemanticRegionKind::Main => "main",
        SemanticRegionKind::Search => "search",
        SemanticRegionKind::Form => "form",
        SemanticRegionKind::Dialog => "dialog",
        SemanticRegionKind::Alert => "alert",
        SemanticRegionKind::Status => "status",
        SemanticRegionKind::Toolbar => "toolbar",
        SemanticRegionKind::FilterPanel
        | SemanticRegionKind::Results
        | SemanticRegionKind::Pagination
        | SemanticRegionKind::CheckoutSummary
        | SemanticRegionKind::Authentication
        | SemanticRegionKind::Footer
        | SemanticRegionKind::Unknown => "region",
        SemanticRegionKind::Collection => "list",
        SemanticRegionKind::Table => "table",
        SemanticRegionKind::Article => "article",
        SemanticRegionKind::Sidebar => "complementary",
    }
}

fn scoped_text(
    observation: &SemanticObservation,
    regions: &[&SemanticRegion],
    scope: &ExtractionScope,
) -> Option<String> {
    if !matches!(scope, ExtractionScope::Region { .. }) {
        return observation.text.clone();
    }
    let mut text = String::new();
    for region in regions {
        append_bounded_text(&mut text, &region.label);
        for target in &region.targets {
            if !target.name.is_empty() {
                append_bounded_text(&mut text, &target.name);
            }
        }
    }
    (!text.is_empty()).then_some(text)
}

fn append_bounded_text(output: &mut String, value: &str) {
    if output.len() >= MAX_NATIVE_TEXT_BYTES || value.is_empty() {
        return;
    }
    if !output.is_empty() {
        output.push('\n');
    }
    let available = MAX_NATIVE_TEXT_BYTES.saturating_sub(output.len());
    let value = truncate_utf8(value, available);
    output.push_str(&value);
}

fn trim_to_output_budget(
    evidence: &mut ExtractionEvidence,
    max_output_bytes: u32,
) -> Result<(), ExtractionContractError> {
    let mut removed = false;
    while serde_json::to_vec(evidence)
        .map_err(|error| ExtractionContractError::new("$", error.to_string()))?
        .len()
        > max_output_bytes as usize
    {
        if evidence.facts.pop().is_none() {
            return Err(ExtractionContractError::new(
                "budgets.maxOutputBytes",
                "output budget is too small for native extraction metadata",
            ));
        }
        removed = true;
        evidence.limits.omitted_facts = evidence.limits.omitted_facts.saturating_add(1);
        evidence.limits.truncated = true;
    }
    if removed {
        evidence.coverage.structural = EvidenceQuality::Partial;
        evidence.coverage.semantic = EvidenceQuality::Partial;
        evidence.coverage.reasons.push("budgetTruncated".into());
        evidence.coverage.reasons.truncate(16);
    }
    evidence.limits.text_bytes = evidence.facts.iter().map(fact_text_bytes).sum();
    Ok(())
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

fn strip_query_and_fragment(url: &str) -> String {
    truncate_utf8(url.split(['?', '#']).next().unwrap_or_default(), 2_048)
}

fn truncate_utf8(value: &str, max_bytes: usize) -> String {
    if value.len() <= max_bytes {
        return value.to_owned();
    }
    let mut end = max_bytes;
    while end > 0 && !value.is_char_boundary(end) {
        end -= 1;
    }
    value[..end].to_owned()
}
