//! Versioned intent-resolution contracts.
//!
//! This module describes how a caller asks Glass to resolve bounded browser
//! intent into inspectable candidates. It does not perform resolution or
//! authorize an action; those concerns remain in the later resolver and the
//! existing guarded action executor.

use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

#[cfg(test)]
use super::{
    SemanticExpansionHandle, SemanticObservation, SemanticPage, SemanticRegion, SemanticTarget,
};
use super::{SemanticPageKind, SemanticRegionKind, SemanticRouteIdentity};

pub const INTENT_RESOLUTION_SCHEMA_VERSION: u32 = 1;
const MAX_INTENT_BYTES: usize = 512;
const MAX_ACTION_BYTES: usize = 64;
const MAX_ID_BYTES: usize = 128;
const MAX_LABEL_BYTES: usize = 256;
const MAX_EVIDENCE_ITEMS: usize = 8;
const MAX_EVIDENCE_BYTES: usize = 160;
const MAX_EXCLUDE_TEXT: usize = 8;
const MAX_CANDIDATES: usize = 32;
const MAX_SUGGESTIONS: usize = 8;

/// Action requested after intent resolution succeeds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SemanticIntentAction {
    Click,
    Type,
    Clear,
    Check,
    Uncheck,
    Select,
    Submit,
    Open,
    Close,
    Search,
    Filter,
    Sort,
    Paginate,
    Toggle,
    Expand,
    Collapse,
    Download,
    Upload,
    Inspect,
    Extract,
}

/// Deterministic purpose produced from a bounded intent phrase.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SemanticIntentPurpose {
    Activate,
    Open,
    Continue,
    Submit,
    Cancel,
    Close,
    Search,
    Filter,
    Sort,
    PaginationNext,
    PaginationPrevious,
    Select,
    Check,
    Uncheck,
    Toggle,
    Enter,
    Clear,
    Replace,
    Expand,
    Collapse,
    Download,
    Upload,
    Choose,
    Inspect,
    Extract,
}

/// Normalized intent used by deterministic candidate generation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NormalizedSemanticIntent {
    pub canonical: String,
    pub purpose: SemanticIntentPurpose,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub terms: Vec<String>,
}

/// Normalize one request without consulting a browser or external service.
pub fn normalize_intent(
    request: &SemanticIntentRequest,
) -> Result<NormalizedSemanticIntent, IntentResolutionError> {
    request.validate()?;
    let normalized = request
        .intent
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_ascii_lowercase();
    if normalized.is_empty()
        || matches!(
            normalized.as_str(),
            "do it" | "do that" | "handle this" | "click it" | "use it"
        )
    {
        return Err(IntentResolutionError::new(
            "intent",
            "unsupportedIntent: provide a concrete purpose or target constraint",
        ));
    }

    let (purpose, canonical, terms) =
        if normalized == "next" || normalized == "next page" || normalized == "go to the next page"
        {
            (
                SemanticIntentPurpose::PaginationNext,
                "paginate next".into(),
                Vec::new(),
            )
        } else if normalized == "previous"
            || normalized == "previous page"
            || normalized == "go to the previous page"
        {
            (
                SemanticIntentPurpose::PaginationPrevious,
                "paginate previous".into(),
                Vec::new(),
            )
        } else if let Some(value) = normalized.strip_prefix("search for ") {
            concrete_phrase(
                request.action,
                SemanticIntentPurpose::Search,
                "search",
                value,
            )?
        } else if let Some(value) = normalized.strip_prefix("filter by ") {
            concrete_phrase(
                request.action,
                SemanticIntentPurpose::Filter,
                "filter",
                value,
            )?
        } else if let Some(value) = normalized.strip_prefix("sort by ") {
            concrete_phrase(request.action, SemanticIntentPurpose::Sort, "sort", value)?
        } else if normalized.starts_with("continue ") {
            compatible_phrase(
                request.action,
                SemanticIntentPurpose::Continue,
                normalized.clone(),
            )?
        } else if normalized.starts_with("open ") {
            compatible_phrase(
                request.action,
                SemanticIntentPurpose::Open,
                normalized.clone(),
            )?
        } else if normalized == "submit" || normalized.starts_with("submit ") {
            compatible_phrase(
                request.action,
                SemanticIntentPurpose::Submit,
                normalized.clone(),
            )?
        } else if normalized == "cancel" || normalized.starts_with("cancel ") {
            compatible_phrase(
                request.action,
                SemanticIntentPurpose::Cancel,
                normalized.clone(),
            )?
        } else if normalized == "close" || normalized.starts_with("close ") {
            compatible_phrase(
                request.action,
                SemanticIntentPurpose::Close,
                normalized.clone(),
            )?
        } else {
            let purpose = purpose_for_action(request.action);
            let canonical = normalized.clone();
            let terms = terms_from(&normalized);
            (purpose, canonical, terms)
        };

    validate_text("normalizedIntent", &canonical, MAX_INTENT_BYTES, false)?;
    Ok(NormalizedSemanticIntent {
        canonical,
        purpose,
        terms,
    })
}

/// Resolve an intent against one current semantic observation.
pub fn resolve_intent(
    request: &SemanticIntentRequest,
    observation: &super::SemanticObservation,
) -> SemanticIntentResult {
    let normalized = match normalize_intent(request) {
        Ok(normalized) => normalized,
        Err(error) => {
            return result_for(
                request,
                observation,
                IntentResultParts {
                    normalized_intent: String::new(),
                    resolution: SemanticResolution::UnsupportedIntent,
                    policy_decision: IntentPolicyDecision::Rejected,
                    candidates: Vec::new(),
                    excluded_candidates: Vec::new(),
                    selected_candidate: None,
                    reason: Some(error.reason),
                },
            );
        }
    };
    if request
        .expected_revision
        .is_some_and(|revision| revision != observation.revision)
    {
        return result_for(
            request,
            observation,
            IntentResultParts {
                normalized_intent: normalized.canonical,
                resolution: SemanticResolution::StaleRevision,
                policy_decision: IntentPolicyDecision::Rejected,
                candidates: Vec::new(),
                excluded_candidates: Vec::new(),
                selected_candidate: None,
                reason: Some("expected revision is no longer current".into()),
            },
        );
    }

    let mut candidates = Vec::new();
    let mut excluded = Vec::new();
    let mut candidate_index = 0usize;
    for region in &observation.regions {
        for target in &region.targets {
            let id = format!("candidate_{}", candidate_index + 1);
            candidate_index += 1;
            let (include, evidence, excluded_reason) =
                score_candidate(request, &normalized, observation, region, target);
            if !include {
                if excluded.len() < request.constraints.max_candidates {
                    excluded.push(ExcludedIntentCandidate {
                        id,
                        reason: excluded_reason.unwrap_or(IntentEvidence {
                            category: IntentEvidenceCategory::NegativeConflict,
                            detail: "candidate did not satisfy the declared constraints".into(),
                        }),
                    });
                }
                continue;
            }
            if candidates.len() >= request.constraints.max_candidates {
                continue;
            }
            candidates.push(SemanticIntentCandidate {
                id,
                reference: target.reference.clone(),
                role: target.role.clone(),
                name: target.name.clone(),
                region_id: Some(region.id.clone()),
                region_kind: Some(region.kind),
                confidence: confidence_for(&evidence),
                evidence,
            });
        }
    }

    let classification = classify_resolution(&candidates);
    let (resolution, policy_decision, selected_candidate, reason) =
        apply_resolution_policy(request.resolution_policy, classification, &candidates);
    result_for(
        request,
        observation,
        IntentResultParts {
            normalized_intent: normalized.canonical,
            resolution,
            policy_decision,
            candidates,
            excluded_candidates: excluded,
            selected_candidate,
            reason,
        },
    )
}

fn score_candidate(
    request: &SemanticIntentRequest,
    normalized: &NormalizedSemanticIntent,
    observation: &super::SemanticObservation,
    region: &super::SemanticRegion,
    target: &super::SemanticTarget,
) -> (bool, Vec<IntentEvidence>, Option<IntentEvidence>) {
    if request.constraints.must_be_visible || request.constraints.must_be_enabled {
        return (
            false,
            Vec::new(),
            Some(IntentEvidence {
                category: IntentEvidenceCategory::PolicyExclusion,
                detail: "visibility or enabled state requires actionability validation".into(),
            }),
        );
    }
    if request
        .scope
        .page_kind
        .is_some_and(|kind| kind != observation.page.kind)
    {
        return (
            false,
            Vec::new(),
            Some(IntentEvidence {
                category: IntentEvidenceCategory::NegativeConflict,
                detail: "page kind is outside the requested scope".into(),
            }),
        );
    }
    if request
        .scope
        .region_kind
        .is_some_and(|kind| kind != region.kind)
        || request
            .scope
            .region_id
            .as_deref()
            .is_some_and(|id| id != region.id)
    {
        return (
            false,
            Vec::new(),
            Some(IntentEvidence {
                category: IntentEvidenceCategory::NegativeConflict,
                detail: "region is outside the requested scope".into(),
            }),
        );
    }
    let name_lower = target.name.to_ascii_lowercase();
    if request
        .constraints
        .exclude_text
        .iter()
        .any(|value| name_lower.contains(&value.to_ascii_lowercase()))
    {
        return (
            false,
            Vec::new(),
            Some(IntentEvidence {
                category: IntentEvidenceCategory::NegativeConflict,
                detail: "accessible name contains excluded text".into(),
            }),
        );
    }
    if request
        .constraints
        .role
        .as_deref()
        .is_some_and(|role| role != target.role)
    {
        return (
            false,
            Vec::new(),
            Some(IntentEvidence {
                category: IntentEvidenceCategory::NegativeConflict,
                detail: "role does not match the declared constraint".into(),
            }),
        );
    }
    if request
        .constraints
        .name
        .as_deref()
        .is_some_and(|name| name != target.name)
    {
        return (
            false,
            Vec::new(),
            Some(IntentEvidence {
                category: IntentEvidenceCategory::NegativeConflict,
                detail: "accessible name does not match the declared constraint".into(),
            }),
        );
    }
    if request
        .constraints
        .name_contains
        .as_deref()
        .is_some_and(|value| !name_lower.contains(&value.to_ascii_lowercase()))
    {
        return (
            false,
            Vec::new(),
            Some(IntentEvidence {
                category: IntentEvidenceCategory::NegativeConflict,
                detail: "accessible name does not contain the declared constraint".into(),
            }),
        );
    }

    let mut evidence = vec![IntentEvidence {
        category: IntentEvidenceCategory::RouteMatch,
        detail: "candidate belongs to the observed target and frame route".into(),
    }];
    if request.constraints.role.is_some() {
        evidence.push(IntentEvidence {
            category: IntentEvidenceCategory::ExactRole,
            detail: "role matches the declared constraint".into(),
        });
    }
    if request.constraints.name.is_some() {
        evidence.push(IntentEvidence {
            category: IntentEvidenceCategory::ExactName,
            detail: "accessible name matches the declared constraint".into(),
        });
    }
    if request.constraints.name_contains.is_some()
        || normalized
            .terms
            .iter()
            .any(|term| name_lower.contains(term))
        || synonyms_match(normalized, &name_lower)
    {
        evidence.push(IntentEvidence {
            category: if request.constraints.name_contains.is_some() {
                IntentEvidenceCategory::ExactName
            } else {
                IntentEvidenceCategory::SemanticName
            },
            detail: "bounded intent terms match the accessible name".into(),
        });
    }
    if request.scope.region_kind.is_some() || request.scope.region_id.is_some() {
        evidence.push(IntentEvidence {
            category: IntentEvidenceCategory::RegionMatch,
            detail: "candidate belongs to the requested semantic region".into(),
        });
    }
    (true, evidence, None)
}

fn synonyms_match(normalized: &NormalizedSemanticIntent, name: &str) -> bool {
    let synonyms: &[(&str, &[&str])] = &[
        ("settings", &["preferences", "configuration", "options"]),
        ("continue", &["proceed", "next"]),
        ("close", &["dismiss", "exit"]),
        ("open", &["view", "show"]),
        ("search", &["find", "lookup"]),
    ];
    normalized.terms.iter().any(|term| {
        synonyms
            .iter()
            .find(|(source, _)| source == term)
            .is_some_and(|(_, alternatives)| alternatives.iter().any(|value| name.contains(value)))
    })
}

fn confidence_for(evidence: &[IntentEvidence]) -> IntentConfidence {
    let has = |category| evidence.iter().any(|item| item.category == category);
    if has(IntentEvidenceCategory::ExactName) && has(IntentEvidenceCategory::ExactRole) {
        IntentConfidence::Exact
    } else if has(IntentEvidenceCategory::ExactName)
        || (has(IntentEvidenceCategory::SemanticName) && has(IntentEvidenceCategory::RegionMatch))
    {
        IntentConfidence::High
    } else if has(IntentEvidenceCategory::SemanticName) || has(IntentEvidenceCategory::RegionMatch)
    {
        IntentConfidence::Medium
    } else {
        IntentConfidence::Low
    }
}

fn classify_resolution(candidates: &[SemanticIntentCandidate]) -> SemanticResolution {
    match candidates {
        [] => SemanticResolution::NotFound,
        [candidate] => match candidate.confidence {
            IntentConfidence::Exact => SemanticResolution::Exact,
            IntentConfidence::High => SemanticResolution::UniqueHighConfidence,
            IntentConfidence::Medium | IntentConfidence::Low | IntentConfidence::Insufficient => {
                SemanticResolution::UniqueLowConfidence
            }
        },
        _ => SemanticResolution::Ambiguous,
    }
}

fn apply_resolution_policy(
    policy: SemanticResolutionPolicy,
    resolution: SemanticResolution,
    candidates: &[SemanticIntentCandidate],
) -> (
    SemanticResolution,
    IntentPolicyDecision,
    Option<String>,
    Option<String>,
) {
    if matches!(policy, SemanticResolutionPolicy::ReportOnly) {
        return (
            resolution,
            IntentPolicyDecision::ReportOnly,
            None,
            Some("reportOnly never dispatches an action".into()),
        );
    }
    if matches!(policy, SemanticResolutionPolicy::InteractiveConfirmation) {
        return (
            resolution,
            IntentPolicyDecision::ConfirmationRequired,
            None,
            Some("caller confirmation is required before dispatch".into()),
        );
    }
    let allowed = match policy {
        SemanticResolutionPolicy::RequireExact => resolution == SemanticResolution::Exact,
        SemanticResolutionPolicy::RequireUniqueHighConfidence => matches!(
            resolution,
            SemanticResolution::Exact | SemanticResolution::UniqueHighConfidence
        ),
        SemanticResolutionPolicy::AllowUniqueMediumConfidence => {
            matches!(
                resolution,
                SemanticResolution::Exact
                    | SemanticResolution::UniqueHighConfidence
                    | SemanticResolution::UniqueLowConfidence
            ) && candidates.first().is_some_and(|candidate| {
                !matches!(
                    candidate.confidence,
                    IntentConfidence::Low | IntentConfidence::Insufficient
                )
            })
        }
        SemanticResolutionPolicy::ReportOnly
        | SemanticResolutionPolicy::InteractiveConfirmation => false,
    };
    if allowed {
        (
            resolution,
            IntentPolicyDecision::Allowed,
            candidates.first().map(|candidate| candidate.id.clone()),
            None,
        )
    } else if matches!(resolution, SemanticResolution::UniqueLowConfidence) {
        (
            SemanticResolution::PolicyRejected,
            IntentPolicyDecision::Rejected,
            None,
            Some("candidate did not meet the declared confidence policy".into()),
        )
    } else {
        (
            resolution,
            IntentPolicyDecision::Rejected,
            None,
            Some("resolution did not produce one policy-eligible candidate".into()),
        )
    }
}

struct IntentResultParts {
    normalized_intent: String,
    resolution: SemanticResolution,
    policy_decision: IntentPolicyDecision,
    candidates: Vec<SemanticIntentCandidate>,
    excluded_candidates: Vec<ExcludedIntentCandidate>,
    selected_candidate: Option<String>,
    reason: Option<String>,
}

fn result_for(
    request: &SemanticIntentRequest,
    observation: &super::SemanticObservation,
    parts: IntentResultParts,
) -> SemanticIntentResult {
    let result = SemanticIntentResult {
        schema_version: INTENT_RESOLUTION_SCHEMA_VERSION,
        intent: request.intent.clone(),
        action: request.action,
        normalized_intent: parts.normalized_intent,
        resolution: parts.resolution,
        policy_decision: parts.policy_decision,
        route: Some(observation.route.clone()),
        revision: Some(observation.revision),
        excluded_count: parts.excluded_candidates.len(),
        candidates: parts.candidates,
        excluded_candidates: parts.excluded_candidates,
        selected_candidate: parts.selected_candidate,
        suggested_constraints: Vec::new(),
        reason: parts.reason,
    };
    debug_assert!(result.validate().is_ok());
    result
}

fn concrete_phrase(
    action: SemanticIntentAction,
    purpose: SemanticIntentPurpose,
    verb: &str,
    value: &str,
) -> Result<(SemanticIntentPurpose, String, Vec<String>), IntentResolutionError> {
    if value.trim().is_empty() {
        return Err(IntentResolutionError::new(
            "intent",
            "unsupportedIntent: the phrase needs a bounded value",
        ));
    }
    let compatible = match purpose {
        SemanticIntentPurpose::Search => {
            matches!(
                action,
                SemanticIntentAction::Search | SemanticIntentAction::Click
            )
        }
        SemanticIntentPurpose::Filter => {
            matches!(
                action,
                SemanticIntentAction::Filter | SemanticIntentAction::Click
            )
        }
        SemanticIntentPurpose::Sort => {
            matches!(
                action,
                SemanticIntentAction::Sort | SemanticIntentAction::Click
            )
        }
        _ => false,
    };
    if !compatible {
        return Err(IntentResolutionError::new(
            "action",
            "action is incompatible with the normalized intent purpose",
        ));
    }
    Ok((
        purpose,
        format!("{verb} {}", value.trim()),
        terms_from(value),
    ))
}

fn compatible_phrase(
    action: SemanticIntentAction,
    purpose: SemanticIntentPurpose,
    canonical: String,
) -> Result<(SemanticIntentPurpose, String, Vec<String>), IntentResolutionError> {
    let compatible = match purpose {
        SemanticIntentPurpose::Open | SemanticIntentPurpose::Continue => {
            matches!(
                action,
                SemanticIntentAction::Click | SemanticIntentAction::Open
            )
        }
        SemanticIntentPurpose::Submit => {
            matches!(
                action,
                SemanticIntentAction::Click | SemanticIntentAction::Submit
            )
        }
        SemanticIntentPurpose::Cancel | SemanticIntentPurpose::Close => {
            matches!(
                action,
                SemanticIntentAction::Click | SemanticIntentAction::Close
            )
        }
        _ => false,
    };
    if !compatible {
        return Err(IntentResolutionError::new(
            "action",
            "action is incompatible with the normalized intent purpose",
        ));
    }
    Ok((purpose, canonical.clone(), terms_from(&canonical)))
}

fn purpose_for_action(action: SemanticIntentAction) -> SemanticIntentPurpose {
    match action {
        SemanticIntentAction::Click => SemanticIntentPurpose::Activate,
        SemanticIntentAction::Type => SemanticIntentPurpose::Enter,
        SemanticIntentAction::Clear => SemanticIntentPurpose::Clear,
        SemanticIntentAction::Check => SemanticIntentPurpose::Check,
        SemanticIntentAction::Uncheck => SemanticIntentPurpose::Uncheck,
        SemanticIntentAction::Select => SemanticIntentPurpose::Select,
        SemanticIntentAction::Submit => SemanticIntentPurpose::Submit,
        SemanticIntentAction::Open => SemanticIntentPurpose::Open,
        SemanticIntentAction::Close => SemanticIntentPurpose::Close,
        SemanticIntentAction::Search => SemanticIntentPurpose::Search,
        SemanticIntentAction::Filter => SemanticIntentPurpose::Filter,
        SemanticIntentAction::Sort => SemanticIntentPurpose::Sort,
        SemanticIntentAction::Paginate => SemanticIntentPurpose::PaginationNext,
        SemanticIntentAction::Toggle => SemanticIntentPurpose::Toggle,
        SemanticIntentAction::Expand => SemanticIntentPurpose::Expand,
        SemanticIntentAction::Collapse => SemanticIntentPurpose::Collapse,
        SemanticIntentAction::Download => SemanticIntentPurpose::Download,
        SemanticIntentAction::Upload => SemanticIntentPurpose::Upload,
        SemanticIntentAction::Inspect => SemanticIntentPurpose::Inspect,
        SemanticIntentAction::Extract => SemanticIntentPurpose::Extract,
    }
}

fn terms_from(value: &str) -> Vec<String> {
    value
        .split_whitespace()
        .filter(|term| !matches!(*term, "the" | "a" | "an" | "to" | "for" | "by"))
        .take(8)
        .map(|term| term.to_string())
        .collect()
}

/// Result classification exposed to every interface.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SemanticResolution {
    Exact,
    UniqueHighConfidence,
    UniqueLowConfidence,
    Ambiguous,
    NotFound,
    StaleRevision,
    PolicyRejected,
    UnsupportedIntent,
}

/// Confidence attached to one candidate after evidence is considered.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum IntentConfidence {
    Exact,
    High,
    Medium,
    Low,
    Insufficient,
}

/// Public evidence categories; numeric scores are intentionally not part of
/// the contract.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum IntentEvidenceCategory {
    ExactRole,
    ExactName,
    SemanticName,
    RegionMatch,
    FormRelationship,
    HeadingContext,
    StateMatch,
    RouteMatch,
    WorkflowContext,
    HistoricalMatch,
    NegativeConflict,
    PolicyExclusion,
}

/// One bounded explanation for including or excluding a candidate.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct IntentEvidence {
    pub category: IntentEvidenceCategory,
    pub detail: String,
}

/// Optional page and region scope for candidate generation.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct IntentScope {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub page_kind: Option<SemanticPageKind>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub region_kind: Option<SemanticRegionKind>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub region_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub form_label: Option<String>,
}

/// Explicit candidate constraints. No field is an implicit selector.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct IntentConstraints {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name_contains: Option<String>,
    #[serde(default)]
    pub must_be_visible: bool,
    #[serde(default)]
    pub must_be_enabled: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub exclude_text: Vec<String>,
    #[serde(default = "default_max_candidates")]
    pub max_candidates: usize,
}

impl Default for IntentConstraints {
    fn default() -> Self {
        Self {
            role: None,
            name: None,
            name_contains: None,
            must_be_visible: false,
            must_be_enabled: false,
            exclude_text: Vec::new(),
            max_candidates: default_max_candidates(),
        }
    }
}

fn default_max_candidates() -> usize {
    MAX_CANDIDATES
}

/// Caller-selected certainty and execution behavior.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SemanticResolutionPolicy {
    ReportOnly,
    RequireExact,
    RequireUniqueHighConfidence,
    AllowUniqueMediumConfidence,
    InteractiveConfirmation,
}

/// Versioned request to resolve one bounded intent.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SemanticIntentRequest {
    pub schema_version: u32,
    pub intent: String,
    pub action: SemanticIntentAction,
    #[serde(default)]
    pub scope: IntentScope,
    #[serde(default)]
    pub constraints: IntentConstraints,
    pub resolution_policy: SemanticResolutionPolicy,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_revision: Option<u64>,
}

/// A current, revision-scoped target candidate.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SemanticIntentCandidate {
    pub id: String,
    pub reference: String,
    pub role: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub region_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub region_kind: Option<SemanticRegionKind>,
    pub confidence: IntentConfidence,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub evidence: Vec<IntentEvidence>,
}

/// Candidate excluded from consideration for an inspectable reason.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ExcludedIntentCandidate {
    pub id: String,
    pub reason: IntentEvidence,
}

/// A bounded suggestion that a caller can add to disambiguate.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct IntentConstraintSuggestion {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub region_kind: Option<SemanticRegionKind>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name_contains: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
}

/// Explicit policy outcome for the resolution attempt.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum IntentPolicyDecision {
    Allowed,
    ReportOnly,
    ConfirmationRequired,
    Rejected,
}

/// Resolution result returned before any optional guarded action dispatch.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SemanticIntentResult {
    pub schema_version: u32,
    pub intent: String,
    pub action: SemanticIntentAction,
    pub normalized_intent: String,
    pub resolution: SemanticResolution,
    pub policy_decision: IntentPolicyDecision,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub route: Option<SemanticRouteIdentity>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub revision: Option<u64>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub candidates: Vec<SemanticIntentCandidate>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub excluded_candidates: Vec<ExcludedIntentCandidate>,
    pub excluded_count: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub selected_candidate: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub suggested_constraints: Vec<IntentConstraintSuggestion>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

impl SemanticIntentRequest {
    pub fn validate(&self) -> Result<(), IntentResolutionError> {
        if self.schema_version != INTENT_RESOLUTION_SCHEMA_VERSION {
            return Err(IntentResolutionError::new(
                "schemaVersion",
                format!(
                    "unsupported schema version {}; expected {}",
                    self.schema_version, INTENT_RESOLUTION_SCHEMA_VERSION
                ),
            ));
        }
        validate_text("intent", &self.intent, MAX_INTENT_BYTES, false)?;
        validate_scope(&self.scope)?;
        validate_constraints(&self.constraints)?;
        if self.scope.region_id.is_some() && self.scope.region_kind.is_none() {
            return Err(IntentResolutionError::new(
                "scope.regionId",
                "regionId requires regionKind or a later concrete region handle",
            ));
        }
        Ok(())
    }

    pub fn from_json(input: &str) -> Result<Self, IntentResolutionError> {
        let request: Self = serde_json::from_str(input).map_err(|error| {
            IntentResolutionError::new("$", format!("invalid intent request shape: {error}"))
        })?;
        request.validate()?;
        Ok(request)
    }

    pub fn to_canonical_json(&self) -> Result<String, IntentResolutionError> {
        self.validate()?;
        serde_json::to_string(self)
            .map_err(|error| IntentResolutionError::new("$", error.to_string()))
    }
}

impl SemanticIntentResult {
    pub fn validate(&self) -> Result<(), IntentResolutionError> {
        if self.schema_version != INTENT_RESOLUTION_SCHEMA_VERSION {
            return Err(IntentResolutionError::new(
                "schemaVersion",
                format!(
                    "unsupported schema version {}; expected {}",
                    self.schema_version, INTENT_RESOLUTION_SCHEMA_VERSION
                ),
            ));
        }
        validate_text("intent", &self.intent, MAX_INTENT_BYTES, false)?;
        validate_text(
            "normalizedIntent",
            &self.normalized_intent,
            MAX_INTENT_BYTES,
            false,
        )?;
        if let Some(route) = &self.route {
            validate_text("route.targetId", &route.target_id, MAX_ID_BYTES, false)?;
            validate_text("route.frameId", &route.frame_id, MAX_ID_BYTES, false)?;
            validate_text("route.url", &route.url, 2_048, false)?;
        }
        if self.candidates.len() > MAX_CANDIDATES {
            return Err(IntentResolutionError::new(
                "candidates",
                format!("contains more than {MAX_CANDIDATES} candidates"),
            ));
        }
        if self.excluded_candidates.len() > MAX_CANDIDATES {
            return Err(IntentResolutionError::new(
                "excludedCandidates",
                format!("contains more than {MAX_CANDIDATES} candidates"),
            ));
        }
        if self.suggested_constraints.len() > MAX_SUGGESTIONS {
            return Err(IntentResolutionError::new(
                "suggestedConstraints",
                format!("contains more than {MAX_SUGGESTIONS} suggestions"),
            ));
        }
        for (index, candidate) in self.candidates.iter().enumerate() {
            validate_candidate(&format!("candidates[{index}]"), candidate)?;
        }
        for (index, candidate) in self.excluded_candidates.iter().enumerate() {
            validate_text(
                &format!("excludedCandidates[{index}].id"),
                &candidate.id,
                MAX_ID_BYTES,
                false,
            )?;
            validate_evidence(
                &format!("excludedCandidates[{index}].reason"),
                std::slice::from_ref(&candidate.reason),
            )?;
        }
        if self.excluded_count < self.excluded_candidates.len() {
            return Err(IntentResolutionError::new(
                "excludedCount",
                "cannot be less than the returned excluded candidate count",
            ));
        }
        if let Some(selected) = &self.selected_candidate {
            validate_text("selectedCandidate", selected, MAX_ID_BYTES, false)?;
            if !self
                .candidates
                .iter()
                .any(|candidate| &candidate.id == selected)
            {
                return Err(IntentResolutionError::new(
                    "selectedCandidate",
                    "does not identify a returned candidate",
                ));
            }
        }
        for (index, suggestion) in self.suggested_constraints.iter().enumerate() {
            if let Some(value) = &suggestion.name_contains {
                validate_text(
                    &format!("suggestedConstraints[{index}].nameContains"),
                    value,
                    MAX_LABEL_BYTES,
                    false,
                )?;
            }
            if let Some(value) = &suggestion.role {
                validate_text(
                    &format!("suggestedConstraints[{index}].role"),
                    value,
                    MAX_ACTION_BYTES,
                    false,
                )?;
            }
        }
        if let Some(reason) = &self.reason {
            validate_text("reason", reason, MAX_EVIDENCE_BYTES, false)?;
        }
        Ok(())
    }

    pub fn from_json(input: &str) -> Result<Self, IntentResolutionError> {
        let result: Self = serde_json::from_str(input).map_err(|error| {
            IntentResolutionError::new("$", format!("invalid intent result shape: {error}"))
        })?;
        result.validate()?;
        Ok(result)
    }

    pub fn to_canonical_json(&self) -> Result<String, IntentResolutionError> {
        self.validate()?;
        serde_json::to_string(self)
            .map_err(|error| IntentResolutionError::new("$", error.to_string()))
    }
}

fn validate_scope(scope: &IntentScope) -> Result<(), IntentResolutionError> {
    if let Some(value) = &scope.region_id {
        validate_text("scope.regionId", value, MAX_ID_BYTES, false)?;
    }
    if let Some(value) = &scope.form_label {
        validate_text("scope.formLabel", value, MAX_LABEL_BYTES, false)?;
    }
    Ok(())
}

fn validate_constraints(constraints: &IntentConstraints) -> Result<(), IntentResolutionError> {
    if let Some(value) = &constraints.role {
        validate_text("constraints.role", value, MAX_ACTION_BYTES, false)?;
    }
    for (path, value) in [
        ("constraints.name", constraints.name.as_deref()),
        (
            "constraints.nameContains",
            constraints.name_contains.as_deref(),
        ),
    ] {
        if let Some(value) = value {
            validate_text(path, value, MAX_LABEL_BYTES, false)?;
        }
    }
    if constraints.exclude_text.len() > MAX_EXCLUDE_TEXT {
        return Err(IntentResolutionError::new(
            "constraints.excludeText",
            format!("contains more than {MAX_EXCLUDE_TEXT} values"),
        ));
    }
    for (index, value) in constraints.exclude_text.iter().enumerate() {
        validate_text(
            &format!("constraints.excludeText[{index}]"),
            value,
            MAX_LABEL_BYTES,
            false,
        )?;
    }
    if constraints.max_candidates == 0 || constraints.max_candidates > MAX_CANDIDATES {
        return Err(IntentResolutionError::new(
            "constraints.maxCandidates",
            format!("must be between 1 and {MAX_CANDIDATES}"),
        ));
    }
    Ok(())
}

fn validate_candidate(
    path: &str,
    candidate: &SemanticIntentCandidate,
) -> Result<(), IntentResolutionError> {
    for (suffix, value, maximum) in [
        ("id", candidate.id.as_str(), MAX_ID_BYTES),
        ("reference", candidate.reference.as_str(), MAX_ID_BYTES),
        ("role", candidate.role.as_str(), MAX_ACTION_BYTES),
        ("name", candidate.name.as_str(), MAX_LABEL_BYTES),
    ] {
        validate_text(&format!("{path}.{suffix}"), value, maximum, false)?;
    }
    if let Some(region_id) = &candidate.region_id {
        validate_text(&format!("{path}.regionId"), region_id, MAX_ID_BYTES, false)?;
    }
    validate_evidence(&format!("{path}.evidence"), &candidate.evidence)
}

fn validate_evidence(path: &str, evidence: &[IntentEvidence]) -> Result<(), IntentResolutionError> {
    if evidence.len() > MAX_EVIDENCE_ITEMS {
        return Err(IntentResolutionError::new(
            path,
            format!("contains more than {MAX_EVIDENCE_ITEMS} items"),
        ));
    }
    let mut categories = BTreeSet::new();
    for (index, item) in evidence.iter().enumerate() {
        if !categories.insert(item.category) {
            return Err(IntentResolutionError::new(
                format!("{path}[{index}].category"),
                "duplicate evidence category",
            ));
        }
        validate_text(
            &format!("{path}[{index}].detail"),
            &item.detail,
            MAX_EVIDENCE_BYTES,
            false,
        )?;
    }
    Ok(())
}

fn validate_text(
    path: &str,
    value: &str,
    maximum: usize,
    allow_empty: bool,
) -> Result<(), IntentResolutionError> {
    if (!allow_empty && value.is_empty()) || value.len() > maximum {
        let requirement = if allow_empty {
            format!("at most {maximum} bytes")
        } else {
            format!("non-empty and at most {maximum} bytes")
        };
        return Err(IntentResolutionError::new(
            path,
            format!("must be {requirement}"),
        ));
    }
    Ok(())
}

/// Path-aware intent contract validation error.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct IntentResolutionError {
    pub path: String,
    pub reason: String,
}

impl IntentResolutionError {
    fn new(path: impl Into<String>, reason: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            reason: reason.into(),
        }
    }
}

impl std::fmt::Display for IntentResolutionError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{}: {}", self.path, self.reason)
    }
}

impl std::error::Error for IntentResolutionError {}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_request() -> SemanticIntentRequest {
        SemanticIntentRequest {
            schema_version: INTENT_RESOLUTION_SCHEMA_VERSION,
            intent: "open settings".into(),
            action: SemanticIntentAction::Click,
            scope: IntentScope {
                page_kind: Some(SemanticPageKind::Dashboard),
                region_kind: Some(SemanticRegionKind::Navigation),
                ..IntentScope::default()
            },
            constraints: IntentConstraints {
                role: Some("button".into()),
                must_be_visible: true,
                ..IntentConstraints::default()
            },
            resolution_policy: SemanticResolutionPolicy::RequireUniqueHighConfidence,
            expected_revision: Some(42),
        }
    }

    #[test]
    fn request_round_trip_is_canonical_and_bounded() {
        let request = make_request();
        let first = request.to_canonical_json().unwrap();
        assert!(first.contains("requireUniqueHighConfidence"));
        assert!(first.contains("pageKind"));
        assert_eq!(
            SemanticIntentRequest::from_json(&first)
                .unwrap()
                .to_canonical_json()
                .unwrap(),
            first
        );
    }

    #[test]
    fn request_rejects_invalid_scope_and_limits() {
        let mut request = make_request();
        request.scope.region_id = Some("region_navigation".into());
        request.scope.region_kind = None;
        let error = request.validate().unwrap_err();
        assert_eq!(error.path, "scope.regionId");

        let mut request = make_request();
        request.constraints.max_candidates = MAX_CANDIDATES + 1;
        let error = request.validate().unwrap_err();
        assert_eq!(error.path, "constraints.maxCandidates");
    }

    #[test]
    fn normalization_is_deterministic_and_rejects_vague_phrases() {
        let mut request = make_request();
        request.intent = "go to the next page".into();
        request.action = SemanticIntentAction::Paginate;
        let normalized = normalize_intent(&request).unwrap();
        assert_eq!(normalized.canonical, "paginate next");
        assert_eq!(normalized.purpose, SemanticIntentPurpose::PaginationNext);

        request.intent = "search for blue shoes".into();
        request.action = SemanticIntentAction::Search;
        let normalized = normalize_intent(&request).unwrap();
        assert_eq!(normalized.canonical, "search blue shoes");
        assert_eq!(
            normalized.terms,
            vec!["blue".to_string(), "shoes".to_string()]
        );

        request.intent = "do it".into();
        let error = normalize_intent(&request).unwrap_err();
        assert_eq!(error.path, "intent");

        request.intent = "search for blue shoes".into();
        request.action = SemanticIntentAction::Type;
        let error = normalize_intent(&request).unwrap_err();
        assert_eq!(error.path, "action");
    }

    fn semantic_observation() -> SemanticObservation {
        let route = SemanticRouteIdentity {
            target_id: "target".into(),
            frame_id: "frame".into(),
            url: "https://example.test/dashboard".into(),
        };
        let page = SemanticPage {
            kind: SemanticPageKind::Dashboard,
            title: "Dashboard".into(),
            url: route.url.clone(),
            target_id: route.target_id.clone(),
            frame_id: route.frame_id.clone(),
            confidence: super::super::SemanticConfidence::High,
            evidence: vec!["fixture".into()],
        };
        let region = SemanticRegion {
            id: "region_navigation_1".into(),
            kind: SemanticRegionKind::Navigation,
            label: "Navigation".into(),
            interactive_count: 3,
            item_count: None,
            confidence: super::super::SemanticConfidence::Exact,
            evidence: vec!["aria-role=navigation".into()],
            targets: vec![
                SemanticTarget {
                    reference: "r42:b1".into(),
                    role: "button".into(),
                    name: "Settings".into(),
                    input_type: None,
                },
                SemanticTarget {
                    reference: "r42:b2".into(),
                    role: "button".into(),
                    name: "Continue shopping".into(),
                    input_type: None,
                },
                SemanticTarget {
                    reference: "r42:b3".into(),
                    role: "button".into(),
                    name: "Continue to payment".into(),
                    input_type: None,
                },
            ],
            expansion: Some(SemanticExpansionHandle {
                region_id: "region_navigation_1".into(),
                revision: 42,
                route: route.clone(),
            }),
        };
        SemanticObservation {
            schema_version: super::INTENT_RESOLUTION_SCHEMA_VERSION,
            revision: 42,
            level: super::super::SemanticObservationLevel::Interactive,
            route,
            page,
            regions: vec![region],
            text: None,
            accessibility: None,
            raw_accessibility: None,
            changes: None,
            limits: super::super::SemanticObservationLimits::default(),
        }
    }

    #[test]
    fn resolver_returns_selected_exact_and_explicit_ambiguous_results() {
        let observation = semantic_observation();
        let mut request = make_request();
        request.constraints.must_be_visible = false;
        request.constraints.name = Some("Settings".into());
        let result = resolve_intent(&request, &observation);
        assert_eq!(result.resolution, SemanticResolution::Exact);
        assert_eq!(result.policy_decision, IntentPolicyDecision::Allowed);
        assert_eq!(result.selected_candidate.as_deref(), Some("candidate_1"));

        request.intent = "continue checkout".into();
        request.constraints.name = None;
        request.constraints.name_contains = Some("continue".into());
        let result = resolve_intent(&request, &observation);
        assert_eq!(result.resolution, SemanticResolution::Ambiguous);
        assert_eq!(result.policy_decision, IntentPolicyDecision::Rejected);
        assert!(result.selected_candidate.is_none());
        assert_eq!(result.candidates.len(), 2);
    }

    #[test]
    fn resolver_fails_closed_for_stale_revision_and_unproven_state() {
        let observation = semantic_observation();
        let mut request = make_request();
        request.expected_revision = Some(41);
        let result = resolve_intent(&request, &observation);
        assert_eq!(result.resolution, SemanticResolution::StaleRevision);

        request.expected_revision = Some(42);
        request.constraints.must_be_enabled = true;
        let result = resolve_intent(&request, &observation);
        assert_eq!(result.resolution, SemanticResolution::NotFound);
        assert_eq!(result.excluded_count, 3);
        assert!(result.selected_candidate.is_none());
    }

    #[test]
    fn result_rejects_duplicate_evidence_and_unknown_fields() {
        let result = SemanticIntentResult {
            schema_version: INTENT_RESOLUTION_SCHEMA_VERSION,
            intent: "open settings".into(),
            action: SemanticIntentAction::Click,
            normalized_intent: "open settings".into(),
            resolution: SemanticResolution::UniqueHighConfidence,
            policy_decision: IntentPolicyDecision::Allowed,
            route: None,
            revision: Some(42),
            candidates: vec![SemanticIntentCandidate {
                id: "candidate_1".into(),
                reference: "r42:b17".into(),
                role: "button".into(),
                name: "Settings".into(),
                region_id: None,
                region_kind: None,
                confidence: IntentConfidence::High,
                evidence: vec![IntentEvidence {
                    category: IntentEvidenceCategory::ExactName,
                    detail: "accessible name exact match".into(),
                }],
            }],
            excluded_candidates: Vec::new(),
            excluded_count: 0,
            selected_candidate: Some("candidate_1".into()),
            suggested_constraints: Vec::new(),
            reason: None,
        };
        let canonical = result.to_canonical_json().unwrap();
        let mut value: serde_json::Value = serde_json::from_str(&canonical).unwrap();
        value["futureField"] = true.into();
        assert_eq!(
            SemanticIntentResult::from_json(&value.to_string())
                .unwrap_err()
                .path,
            "$"
        );

        let mut duplicate = result;
        duplicate.candidates[0].evidence.push(IntentEvidence {
            category: IntentEvidenceCategory::ExactName,
            detail: "duplicate".into(),
        });
        assert_eq!(
            duplicate.validate().unwrap_err().path,
            "candidates[0].evidence[1].category"
        );
    }
}
