//! Bounded task-oriented operations built on the guarded session runtime.

use super::*;
use crate::protocol::{RetryClassification, RetryGuidance};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use sha2::{Digest, Sha256};
use std::time::Duration;

const MAX_EXTRACTION_FIELDS: usize = 32;
const MAX_EXTRACTION_ITEMS: usize = 256;
const MAX_EXTRACTION_BYTES: usize = 256 * 1024;

/// Compact page inspection result for an agent turn.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InspectPageResult {
    pub page: SemanticPage,
    pub revision: u64,
    pub regions: Vec<SemanticRegion>,
    pub limits: SemanticObservationLimits,
    pub focused_target: Option<SemanticTarget>,
    pub alerts: Vec<String>,
}

/// Candidate-only target lookup result. It never dispatches an action.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FindTargetResult {
    pub normalized_intent: String,
    pub revision: Option<u64>,
    pub candidates: Vec<SemanticIntentCandidate>,
    pub ambiguity: String,
    pub suggested_constraints: Vec<IntentConstraintSuggestion>,
}

/// Bounded action plus optional postcondition verification result.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ActAndVerifyResult {
    pub status: String,
    pub phase: String,
    pub mutation_possible: bool,
    pub execution: SemanticIntentExecutionResult,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub verification: Option<VerificationOutcome>,
    pub retry: RetryGuidance,
}

/// Structured extraction field declaration.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExtractionField {
    pub name: String,
    pub path: String,
    pub kind: ExtractionKind,
}

/// Supported bounded extraction shapes.
///
/// The legacy variants remain available for compatibility. The explicit
/// variants are preferred for new requests because they let callers state the
/// expected scalar contract without relying on a broad `Scalar` value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ExtractionKind {
    Scalar,
    OptionalScalar,
    String,
    OptionalString,
    Number,
    Currency,
    Date,
    DateTime,
    Boolean,
    Url,
    Enum,
    List,
    Record,
    Object,
    Table,
    RepeatedItems,
}

/// Request for typed extraction from one fresh semantic region.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StructuredExtractionRequest {
    pub fields: Vec<ExtractionField>,
    #[serde(default)]
    pub region_id: Option<String>,
    #[serde(default)]
    pub start_index: usize,
    #[serde(default)]
    pub continuation: Option<StructuredExtractionContinuation>,
    #[serde(default = "default_extraction_items")]
    pub max_items: usize,
    #[serde(default = "default_extraction_bytes")]
    pub max_bytes: usize,
}

/// One bounded item from a table or repeated collection extraction.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StructuredExtractionRecord {
    pub field: String,
    pub index: usize,
    pub value: Value,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub entity_ids: Vec<String>,
}

/// Bounded typed extraction output with revision and field provenance.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StructuredExtractionResult {
    pub source_revision: u64,
    pub source_route: SemanticRouteIdentity,
    pub records: Vec<Value>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub record_items: Vec<StructuredExtractionRecord>,
    pub truncated: bool,
    pub provenance: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub field_provenance: Vec<StructuredExtractionProvenance>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub continuation: Option<StructuredExtractionContinuation>,
    pub limits: StructuredExtractionLimits,
}

/// Revision-bound continuation for a bounded item extraction.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StructuredExtractionContinuation {
    pub next_index: usize,
    pub source_revision: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub region_id: Option<String>,
    pub contract_hash: String,
    pub source_route: SemanticRouteIdentity,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_route_fingerprint: Option<String>,
}

/// Evidence supporting one extracted field.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StructuredExtractionProvenance {
    pub field: String,
    pub path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub region_id: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub entity_ids: Vec<String>,
}

/// Explicit output bounds and observed extraction size.
#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StructuredExtractionLimits {
    pub max_items: usize,
    pub max_bytes: usize,
    pub observed_items: usize,
    pub serialized_bytes: usize,
    pub truncated: bool,
}

/// Recovery information for a run identifier that may outlive a session.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecoverRunResult {
    pub execution_id: String,
    pub known: bool,
    pub phase: String,
    pub dispatch_happened: bool,
    pub mutation_possible: bool,
    pub reconciliation: String,
    pub retry: RetryGuidance,
}

impl BrowserSession {
    /// Capture one bounded semantic observation without changing browser state.
    pub async fn inspect_page(&self) -> BrowserResult<InspectPageResult> {
        let observation = self
            .semantic_observe(SemanticObservationLevel::Structured)
            .await?;
        Ok(InspectPageResult {
            page: observation.page,
            revision: observation.revision,
            regions: observation.regions,
            limits: observation.limits,
            focused_target: None,
            alerts: Vec::new(),
        })
    }

    /// Resolve candidates from a fresh observation without acting.
    pub async fn find_target(
        &self,
        request: &SemanticIntentRequest,
    ) -> BrowserResult<FindTargetResult> {
        let result = self.resolve_intent(request).await?;
        let ambiguity = match result.resolution {
            SemanticResolution::Exact
            | SemanticResolution::UniqueHighConfidence
            | SemanticResolution::UniqueLowConfidence => "none",
            SemanticResolution::Ambiguous => "ambiguous",
            SemanticResolution::NotFound => "not_found",
            SemanticResolution::StaleRevision => "stale_revision",
            SemanticResolution::PolicyRejected => "policy_rejected",
            SemanticResolution::UnsupportedIntent => "unsupported_intent",
        };
        Ok(FindTargetResult {
            normalized_intent: result.normalized_intent,
            revision: result.revision,
            candidates: result.candidates,
            ambiguity: ambiguity.into(),
            suggested_constraints: result.suggested_constraints,
        })
    }

    /// Execute one explicit intent through the guarded boundary and verify it.
    pub async fn act_and_verify(
        &self,
        execution: &SemanticIntentExecutionRequest,
        predicate: Option<VerificationPredicate>,
        timeout: Duration,
    ) -> BrowserResult<ActAndVerifyResult> {
        let execution_result = self.execute_intent(execution).await?;
        let Some(_) = execution_result.action.as_ref() else {
            return Ok(ActAndVerifyResult {
                status: "not_executed".into(),
                phase: "preflight".into(),
                mutation_possible: false,
                execution: execution_result,
                verification: None,
                retry: RetryGuidance {
                    classification: RetryClassification::SafeAfterReobserve,
                    recommended_operation: "find_target".into(),
                },
            });
        };
        let Some(predicate) = predicate else {
            return Ok(ActAndVerifyResult {
                status: "dispatched_unverified".into(),
                phase: "post_dispatch".into(),
                mutation_possible: true,
                execution: execution_result,
                verification: None,
                retry: RetryGuidance {
                    classification: RetryClassification::RequiresUserDecision,
                    recommended_operation: "inspect_page".into(),
                },
            });
        };
        let verification = self.verify(predicate, timeout).await;
        match verification {
            Ok(verification) => Ok(ActAndVerifyResult {
                status: "verified".into(),
                phase: "verification".into(),
                mutation_possible: false,
                execution: execution_result,
                verification: Some(verification),
                retry: RetryGuidance {
                    classification: RetryClassification::SafeImmediate,
                    recommended_operation: "inspect_page".into(),
                },
            }),
            Err(_error) => Ok(ActAndVerifyResult {
                status: "indeterminate".into(),
                phase: "verification".into(),
                mutation_possible: true,
                execution: execution_result,
                verification: None,
                retry: RetryGuidance {
                    classification: RetryClassification::UnsafeUntilReconciled,
                    recommended_operation: "recover_run".into(),
                },
            }),
        }
    }

    /// Extract typed, bounded records from a fresh semantic region.
    pub async fn extract_structured(
        &self,
        request: &StructuredExtractionRequest,
    ) -> BrowserResult<StructuredExtractionResult> {
        validate_extraction_request(request)?;
        if request.fields.iter().any(extraction_field_is_sensitive) {
            self.policy.require_sensitive_extraction()?;
        }
        let contract_hash = extraction_contract_hash(request);
        let observation = self
            .semantic_observe(SemanticObservationLevel::Structured)
            .await?;
        if let Some(continuation) = &request.continuation
            && !continuation_matches_source(
                continuation,
                observation.revision,
                &observation.route,
                request.region_id.as_deref(),
                &contract_hash,
            )
        {
            return Err(
                "continuation does not match the current semantic source, region, or extraction contract"
                    .into(),
            );
        }
        let start_index = request
            .continuation
            .as_ref()
            .map_or(request.start_index, |continuation| continuation.next_index);
        let scoped_region = request.region_id.as_deref().and_then(|region_id| {
            observation
                .regions
                .iter()
                .find(|region| region.id == region_id)
        });
        if request.region_id.is_some() && scoped_region.is_none() {
            let region_id = request.region_id.as_deref().unwrap_or_default();
            return Err(format!("region not found: {region_id}").into());
        }
        let source = if let Some(region) = scoped_region {
            serde_json::to_value(region)?
        } else {
            serde_json::to_value(&observation)?
        };
        let mut record = Map::new();
        let mut truncated = observation.limits.truncated
            || observation.limits.omitted_regions > 0
            || observation.limits.omitted_targets > 0
            || observation.limits.omitted_structured_records > 0;
        let mut next_index: Option<usize> = None;
        let mut observed_items = observation.limits.omitted_structured_records;
        let mut emitted_items = 0usize;
        let mut record_items = Vec::new();
        let mut field_provenance = Vec::with_capacity(request.fields.len());
        for field in &request.fields {
            let mut value = extraction_source_value(&source, field)
                .ok_or_else(|| format!("field path is missing: {}", field.path))?;
            if value_contains_sensitive(&value) {
                if field.path == "$.targets"
                    || (field.path == "$"
                        && field.name == "region"
                        && field.kind == ExtractionKind::Object)
                {
                    redact_sensitive_value(&mut value);
                } else {
                    self.policy.require_sensitive_extraction()?;
                }
            }
            let mut value = validate_extracted_value(Some(value), field)?;
            if let Value::Array(items) = &mut value {
                let item_count = items.len();
                observed_items = observed_items.saturating_add(item_count);
                let start = start_index.min(item_count);
                let remaining = request.max_items.saturating_sub(emitted_items);
                let end = start.saturating_add(remaining).min(item_count);
                emitted_items = emitted_items.saturating_add(end.saturating_sub(start));
                truncated |= start > 0 || end < item_count;
                if end < item_count {
                    next_index = Some(next_index.map_or(end, |current| current.max(end)));
                }
                if start > 0 || end < item_count {
                    *items = items[start..end].to_vec();
                }
                record_items.extend(extraction_record_items(field, &value, start));
            }
            let entity_ids = provenance_entity_ids(&value);
            field_provenance.push(StructuredExtractionProvenance {
                field: field.name.clone(),
                path: field.path.clone(),
                region_id: request.region_id.clone(),
                entity_ids,
            });
            record.insert(field.name.clone(), value);
        }
        let source_route = sanitized_route(&observation.route);
        let continuation = next_index.map(|next_index| StructuredExtractionContinuation {
            next_index,
            source_revision: observation.revision,
            source_route: source_route.clone(),
            region_id: request.region_id.clone(),
            contract_hash: contract_hash.clone(),
            source_route_fingerprint: Some(route_fingerprint(&observation.route)),
        });
        let result = StructuredExtractionResult {
            source_revision: observation.revision,
            source_route,
            records: vec![record.into()],
            record_items,
            truncated,
            provenance: request
                .fields
                .iter()
                .map(|field| field.path.clone())
                .collect(),
            field_provenance,
            continuation,
            limits: StructuredExtractionLimits {
                max_items: request.max_items,
                max_bytes: request.max_bytes,
                observed_items,
                serialized_bytes: 0,
                truncated,
            },
        };
        finalize_extraction_result_with_context(
            result,
            request.max_bytes,
            Some(&contract_hash),
            request.region_id.as_deref(),
            Some(&route_fingerprint(&observation.route)),
        )
    }

    /// Reconcile a run ID conservatively when the original session is gone.
    pub fn recover_run(&self, execution_id: &str) -> BrowserResult<RecoverRunResult> {
        recover_run(execution_id)
    }
}

/// Reconcile a run ID without requiring a live browser session.
pub fn recover_run(execution_id: &str) -> BrowserResult<RecoverRunResult> {
    if execution_id.is_empty() || execution_id.len() > 128 {
        return Err("execution ID must be 1..=128 bytes".into());
    }
    Ok(RecoverRunResult {
        execution_id: execution_id.into(),
        known: false,
        phase: "reconciliation".into(),
        dispatch_happened: false,
        mutation_possible: true,
        reconciliation: "session-local execution evidence is unavailable; observe before retry"
            .into(),
        retry: RetryGuidance {
            classification: RetryClassification::UnsafeUntilReconciled,
            recommended_operation: "inspect_page".into(),
        },
    })
}

fn serialized_extraction_result_bytes(
    result: &mut StructuredExtractionResult,
) -> BrowserResult<usize> {
    let mut observed = 0usize;
    for _ in 0..8 {
        let serialized = serde_json::to_vec(result)
            .map_err(|error| format!("failed to serialize extraction result: {error}"))?;
        let bytes = serialized.len();
        result.limits.serialized_bytes = bytes;
        if bytes == observed {
            return Ok(bytes);
        }
        observed = bytes;
    }
    Ok(observed)
}

#[cfg(test)]
fn finalize_extraction_result(
    result: StructuredExtractionResult,
    max_bytes: usize,
) -> BrowserResult<StructuredExtractionResult> {
    finalize_extraction_result_with_context(result, max_bytes, None, None, None)
}

fn finalize_extraction_result_with_context(
    mut result: StructuredExtractionResult,
    max_bytes: usize,
    contract_hash: Option<&str>,
    region_id: Option<&str>,
    route_fingerprint: Option<&str>,
) -> BrowserResult<StructuredExtractionResult> {
    loop {
        let serialized_bytes = serialized_extraction_result_bytes(&mut result)?;
        if serialized_bytes <= max_bytes {
            return Ok(result);
        }
        let Some(removed_index) = trim_one_extraction_item(&mut result) else {
            return Err(format!(
                "extraction exceeds maxBytes ({} > {})",
                serialized_bytes, max_bytes
            )
            .into());
        };
        if result.continuation.is_none()
            && let Some(contract_hash) = contract_hash
        {
            result.continuation = Some(StructuredExtractionContinuation {
                next_index: removed_index.saturating_add(1),
                source_revision: result.source_revision,
                region_id: region_id.map(str::to_string),
                contract_hash: contract_hash.to_string(),
                source_route: result.source_route.clone(),
                source_route_fingerprint: route_fingerprint.map(str::to_string),
            });
        }
        result.truncated = true;
        result.limits.truncated = true;
    }
}

fn trim_one_extraction_item(result: &mut StructuredExtractionResult) -> Option<usize> {
    let field_name = {
        let record = result.records.first_mut().and_then(Value::as_object_mut)?;
        let (field, items) = record
            .iter_mut()
            .rev()
            .find_map(|(field, value)| value.as_array_mut().map(|items| (field, items)))?;
        items.pop()?;
        field.clone()
    };
    result
        .record_items
        .iter()
        .rposition(|item| item.field == field_name)
        .map(|position| result.record_items.remove(position).index)
        .or(Some(0))
}

fn validate_extraction_request(request: &StructuredExtractionRequest) -> BrowserResult<()> {
    if request.fields.is_empty() || request.fields.len() > MAX_EXTRACTION_FIELDS {
        return Err(format!("fields must contain 1..={MAX_EXTRACTION_FIELDS} entries").into());
    }
    if request.start_index > MAX_EXTRACTION_ITEMS {
        return Err(format!("startIndex must be <= {MAX_EXTRACTION_ITEMS}").into());
    }
    if let Some(region_id) = request.region_id.as_deref()
        && (region_id.is_empty() || region_id.len() > 128)
    {
        return Err("regionId must be 1..=128 bytes".into());
    }
    if let Some(continuation) = &request.continuation {
        if continuation.next_index > MAX_EXTRACTION_ITEMS {
            return Err(format!("continuation nextIndex must be <= {MAX_EXTRACTION_ITEMS}").into());
        }
        if request.start_index != 0 && request.start_index != continuation.next_index {
            return Err(
                "startIndex must match continuation nextIndex when both are provided".into(),
            );
        }
    }
    if !(1..=MAX_EXTRACTION_ITEMS).contains(&request.max_items) {
        return Err(format!("maxItems must be 1..={MAX_EXTRACTION_ITEMS}").into());
    }
    if !(1..=MAX_EXTRACTION_BYTES).contains(&request.max_bytes) {
        return Err(format!("maxBytes must be 1..={MAX_EXTRACTION_BYTES}").into());
    }
    let mut names = std::collections::BTreeSet::new();
    for field in &request.fields {
        if field.name.is_empty()
            || field.name.len() > 128
            || field.path.len() > 512
            || !is_identifier(&field.name)
        {
            return Err("extraction field names and paths must be bounded identifiers".into());
        }
        if !names.insert(field.name.as_str()) {
            return Err(format!("duplicate extraction field name: {}", field.name).into());
        }
        if !(is_semantic_path(&field.path)
            || (field.path == "$"
                && field.name == "region"
                && field.kind == ExtractionKind::Object
                && request.region_id.is_some()))
        {
            return Err(format!("invalid semantic extraction path: {}", field.path).into());
        }
    }
    Ok(())
}

fn is_identifier(value: &str) -> bool {
    !value.is_empty()
        && value.bytes().enumerate().all(|(index, byte)| {
            if index == 0 {
                byte.is_ascii_alphabetic() || byte == b'_'
            } else {
                byte.is_ascii_alphanumeric() || byte == b'_'
            }
        })
}

fn is_semantic_path(path: &str) -> bool {
    let Some(path) = path.strip_prefix("$.") else {
        return false;
    };
    !path.is_empty()
        && !path.contains(['[', ']', '/', ':', '(', ')', '#', '@'])
        && path.split('.').all(|segment| {
            !segment.is_empty()
                && segment.bytes().enumerate().all(|(index, byte)| {
                    if index == 0 {
                        byte.is_ascii_alphabetic() || byte == b'_'
                    } else {
                        byte.is_ascii_alphanumeric() || byte == b'_'
                    }
                })
        })
}

fn extraction_field_is_sensitive(field: &ExtractionField) -> bool {
    is_sensitive_extraction_text(&field.name) || is_sensitive_extraction_text(&field.path)
}

fn is_sensitive_extraction_text(value: &str) -> bool {
    let normalized: String = value
        .to_ascii_lowercase()
        .chars()
        .filter(|character| character.is_ascii_alphanumeric())
        .collect();
    [
        "password",
        "passwd",
        "secret",
        "token",
        "apikey",
        "cookie",
        "authorization",
        "creditcard",
        "cardnumber",
        "cvv",
        "ssn",
        "socialsecurity",
    ]
    .iter()
    .any(|term| normalized.contains(term))
}

fn continuation_matches_source(
    continuation: &StructuredExtractionContinuation,
    revision: u64,
    route: &SemanticRouteIdentity,
    region_id: Option<&str>,
    contract_hash: &str,
) -> bool {
    continuation.source_revision == revision
        && match continuation.source_route_fingerprint.as_deref() {
            Some(fingerprint) => fingerprint == route_fingerprint(route),
            None => continuation.source_route == sanitized_route(route),
        }
        && continuation.region_id.as_deref() == region_id
        && continuation.contract_hash == contract_hash
}

fn route_fingerprint(route: &SemanticRouteIdentity) -> String {
    let canonical = serde_json::to_vec(route).expect("route identity is serializable");
    let digest = Sha256::digest(canonical);
    format!("sha256:{digest:x}")
}

fn sanitized_route(route: &SemanticRouteIdentity) -> SemanticRouteIdentity {
    let mut sanitized = route.clone();
    if let Ok(mut url) = url::Url::parse(&sanitized.url) {
        let _ = url.set_username("");
        let _ = url.set_password(None);
        url.set_query(None);
        url.set_fragment(None);
        sanitized.url = url.to_string();
    } else {
        sanitized.url = sanitized
            .url
            .split(['?', '#'])
            .next()
            .unwrap_or_default()
            .to_string();
    }
    sanitized
}

fn extraction_contract_hash(request: &StructuredExtractionRequest) -> String {
    let canonical = serde_json::to_vec(&(&request.fields, &request.region_id))
        .expect("extraction contract is serializable");
    let digest = Sha256::digest(canonical);
    format!("sha256:{digest:x}")
}

fn extraction_source_value(source: &Value, field: &ExtractionField) -> Option<Value> {
    if field.path == "$" && field.name == "region" && field.kind == ExtractionKind::Object {
        return Some(source.clone());
    }
    if let Some(value) = value_at_path(source, &field.path) {
        return Some(value.clone());
    }
    if field.path == "$.structuredRecords"
        && matches!(
            field.kind,
            ExtractionKind::Table | ExtractionKind::RepeatedItems
        )
    {
        return source.get("targets").cloned();
    }
    None
}

fn value_contains_sensitive(value: &Value) -> bool {
    match value {
        Value::String(value) => is_sensitive_extraction_text(value),
        Value::Array(values) => values.iter().any(value_contains_sensitive),
        Value::Object(values) => values.iter().any(|(key, value)| {
            is_sensitive_extraction_text(key) || value_contains_sensitive(value)
        }),
        Value::Null | Value::Bool(_) | Value::Number(_) => false,
    }
}
fn redact_sensitive_value(value: &mut Value) {
    match value {
        Value::String(text) if is_sensitive_extraction_text(text) => {
            *text = "<redacted>".into();
        }
        Value::Array(values) => {
            for value in values {
                redact_sensitive_value(value);
            }
        }
        Value::Object(values) => {
            for (key, value) in values {
                if is_sensitive_extraction_text(key) {
                    *value = Value::String("<redacted>".into());
                } else {
                    redact_sensitive_value(value);
                }
            }
        }
        Value::String(_) | Value::Null | Value::Bool(_) | Value::Number(_) => {}
    }
}

fn value_at_path<'a>(value: &'a Value, path: &str) -> Option<&'a Value> {
    path.strip_prefix("$.")?
        .split('.')
        .try_fold(value, |current, segment| current.get(segment))
}

fn validate_extracted_value(value: Option<Value>, field: &ExtractionField) -> BrowserResult<Value> {
    let Some(value) = value else {
        return Err(format!("field path is missing: {}", field.path).into());
    };
    let valid = match field.kind {
        ExtractionKind::Scalar => value.is_string() || value.is_number() || value.is_boolean(),
        ExtractionKind::OptionalScalar => {
            value.is_null() || value.is_string() || value.is_number() || value.is_boolean()
        }
        ExtractionKind::String | ExtractionKind::Enum => value
            .as_str()
            .is_some_and(|value| !value.is_empty() && value.len() <= 256),
        ExtractionKind::OptionalString => value.is_null() || value.is_string(),
        ExtractionKind::Number => value.is_number(),
        ExtractionKind::Currency => {
            value.is_number() || value.as_str().is_some_and(is_currency_string)
        }
        ExtractionKind::Date => value.as_str().is_some_and(is_iso_date),
        ExtractionKind::DateTime => value
            .as_str()
            .is_some_and(|value| chrono::DateTime::parse_from_rfc3339(value).is_ok()),
        ExtractionKind::Boolean => value.is_boolean(),
        ExtractionKind::Url => value
            .as_str()
            .and_then(|value| url::Url::parse(value).ok())
            .is_some_and(|url| !url.scheme().is_empty()),
        ExtractionKind::List | ExtractionKind::Table | ExtractionKind::RepeatedItems => {
            value.is_array()
        }
        ExtractionKind::Record | ExtractionKind::Object => value.is_object(),
    };
    if !valid {
        return Err(format!("field {} does not match its declared type", field.name).into());
    }
    Ok(value)
}
fn provenance_entity_ids(value: &Value) -> Vec<String> {
    let Value::Array(items) = value else {
        return Vec::new();
    };
    items
        .iter()
        .filter_map(|item| item.get("reference").and_then(Value::as_str))
        .filter(|reference| !reference.is_empty() && reference.len() <= 128)
        .map(str::to_owned)
        .collect()
}

fn extraction_record_items(
    field: &ExtractionField,
    value: &Value,
    start_index: usize,
) -> Vec<StructuredExtractionRecord> {
    if !matches!(
        field.kind,
        ExtractionKind::List | ExtractionKind::Table | ExtractionKind::RepeatedItems
    ) {
        return Vec::new();
    }
    let Value::Array(items) = value else {
        return Vec::new();
    };
    items
        .iter()
        .enumerate()
        .map(|(index, item)| StructuredExtractionRecord {
            field: field.name.clone(),
            index: start_index + index,
            value: item.clone(),
            entity_ids: item
                .get("reference")
                .and_then(Value::as_str)
                .filter(|reference| !reference.is_empty() && reference.len() <= 128)
                .map(|reference| vec![reference.to_owned()])
                .unwrap_or_default(),
        })
        .collect()
}

fn is_iso_date(value: &str) -> bool {
    value.len() == 10
        && value.as_bytes()[4] == b'-'
        && value.as_bytes()[7] == b'-'
        && value
            .bytes()
            .enumerate()
            .all(|(index, byte)| matches!(index, 4 | 7) || byte.is_ascii_digit())
        && chrono::NaiveDate::parse_from_str(value, "%Y-%m-%d").is_ok()
}

fn is_currency_string(value: &str) -> bool {
    let value = value.trim();
    !value.is_empty()
        && value.len() <= 64
        && value.chars().any(|character| character.is_ascii_digit())
        && value.chars().all(|character| {
            character.is_ascii_digit()
                || matches!(character, '.' | ',' | '-' | ' ' | '$' | '€' | '£' | '¥')
        })
}

fn default_extraction_items() -> usize {
    64
}

fn default_extraction_bytes() -> usize {
    64 * 1024
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extraction_bounds_and_types_are_checked() {
        let request = StructuredExtractionRequest {
            fields: vec![ExtractionField {
                name: "title".into(),
                path: "$.page.title".into(),
                kind: ExtractionKind::Scalar,
            }],
            region_id: None,
            start_index: 0,
            continuation: None,
            max_items: 1,
            max_bytes: 1024,
        };
        validate_extraction_request(&request).unwrap();
        assert_eq!(
            validate_extracted_value(Some(Value::String("Glass".into())), &request.fields[0])
                .unwrap(),
            Value::String("Glass".into())
        );
    }

    #[test]
    fn extraction_paths_must_be_non_empty() {
        let request = StructuredExtractionRequest {
            fields: vec![ExtractionField {
                name: "title".into(),
                path: String::new(),
                kind: ExtractionKind::String,
            }],
            region_id: None,
            start_index: 0,
            continuation: None,
            max_items: 1,
            max_bytes: 1024,
        };
        assert!(validate_extraction_request(&request).is_err());
    }

    #[test]
    fn continuation_requests_require_a_bounded_consistent_index() {
        let route = SemanticRouteIdentity {
            target_id: "target".into(),
            frame_id: "frame".into(),
            url: "https://example.test/".into(),
        };
        let mut request = StructuredExtractionRequest {
            fields: vec![ExtractionField {
                name: "items".into(),
                path: "$.items".into(),
                kind: ExtractionKind::RepeatedItems,
            }],
            region_id: None,
            start_index: 0,
            continuation: Some(StructuredExtractionContinuation {
                next_index: 4,
                source_revision: 7,
                contract_hash: "sha256:test".into(),
                region_id: None,
                source_route: route.clone(),
                source_route_fingerprint: None,
            }),
            max_items: 2,
            max_bytes: 1024,
        };
        let contract_hash = extraction_contract_hash(&request);
        request.continuation.as_mut().unwrap().contract_hash = contract_hash.clone();
        validate_extraction_request(&request).unwrap();
        let continuation = request.continuation.as_ref().unwrap();
        assert!(continuation_matches_source(
            continuation,
            7,
            &route,
            None,
            &contract_hash
        ));
        assert!(!continuation_matches_source(
            continuation,
            7,
            &route,
            Some("other-region"),
            &contract_hash,
        ));
        request.start_index = 3;
        assert!(validate_extraction_request(&request).is_err());
        request.start_index = 0;
        request.continuation.as_mut().unwrap().next_index = 257;
        assert!(validate_extraction_request(&request).is_err());
    }

    #[test]
    fn sensitive_extraction_fields_are_detected_conservatively() {
        assert!(extraction_field_is_sensitive(&ExtractionField {
            name: "authToken".into(),
            path: "$.record.value".into(),
            kind: ExtractionKind::String,
        }));
        assert!(extraction_field_is_sensitive(&ExtractionField {
            name: "value".into(),
            path: "$.password".into(),
            kind: ExtractionKind::String,
        }));
        assert!(extraction_field_is_sensitive(&ExtractionField {
            name: "creditCard".into(),
            path: "$.billing.card_number".into(),
            kind: ExtractionKind::String,
        }));
        assert!(!extraction_field_is_sensitive(&ExtractionField {
            name: "title".into(),
            path: "$.page.title".into(),
            kind: ExtractionKind::String,
        }));
    }

    #[test]
    fn explicit_extraction_kinds_validate_typed_values() {
        let cases = [
            (ExtractionKind::String, Value::String("Glass".into())),
            (ExtractionKind::OptionalString, Value::Null),
            (ExtractionKind::Number, serde_json::json!(42)),
            (ExtractionKind::Currency, Value::String("$42.00".into())),
            (ExtractionKind::Date, Value::String("2026-08-01".into())),
            (
                ExtractionKind::DateTime,
                Value::String("2026-08-01T12:00:00Z".into()),
            ),
            (ExtractionKind::Boolean, Value::Bool(true)),
            (
                ExtractionKind::Url,
                Value::String("https://example.test/orders".into()),
            ),
            (ExtractionKind::Enum, Value::String("submitted".into())),
            (ExtractionKind::List, serde_json::json!([1, 2])),
            (ExtractionKind::Record, serde_json::json!({"id": 1})),
            (ExtractionKind::Table, serde_json::json!([])),
            (ExtractionKind::RepeatedItems, serde_json::json!([])),
        ];
        for (kind, value) in cases {
            let field = ExtractionField {
                name: "value".into(),
                path: "$.value".into(),
                kind,
            };
            assert!(
                validate_extracted_value(Some(value), &field).is_ok(),
                "expected {kind:?} to accept its typed value"
            );
        }
    }

    #[test]
    fn explicit_extraction_kinds_reject_incompatible_values() {
        for (kind, value) in [
            (ExtractionKind::Number, Value::String("42".into())),
            (ExtractionKind::Date, Value::String("not-a-date".into())),
            (ExtractionKind::Date, Value::String("2026-02-30".into())),
            (ExtractionKind::DateTime, Value::String("2026-08-01".into())),
            (ExtractionKind::Boolean, Value::String("true".into())),
            (ExtractionKind::Url, Value::String("not a url".into())),
            (ExtractionKind::Table, Value::Object(Map::new())),
        ] {
            let field = ExtractionField {
                name: "value".into(),
                path: "$.value".into(),
                kind,
            };
            assert!(
                validate_extracted_value(Some(value), &field).is_err(),
                "expected {kind:?} to reject the incompatible value"
            );
        }
    }

    #[test]
    fn table_and_collection_items_preserve_bounded_record_provenance() {
        let table = ExtractionField {
            name: "rows".into(),
            path: "$.targets".into(),
            kind: ExtractionKind::Table,
        };
        let value = serde_json::json!([
            {"reference": "r7:b1", "name": "Hosting"},
            {"name": "unreferenced"}
        ]);
        let items = extraction_record_items(&table, &value, 3);
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].index, 3);
        assert_eq!(items[0].entity_ids, vec!["r7:b1"]);
        assert!(
            extraction_record_items(
                &ExtractionField {
                    kind: ExtractionKind::String,
                    ..table
                },
                &value,
                0,
            )
            .is_empty()
        );
    }

    #[test]
    fn extraction_provenance_ids_and_limits_are_bounded_and_serialized() {
        let value = serde_json::json!([
            {"reference": "r7:b1"},
            {"reference": "r7:b2"},
            {"label": "no-reference"}
        ]);
        assert_eq!(
            provenance_entity_ids(&value),
            vec!["r7:b1".to_string(), "r7:b2".to_string()]
        );
        let mut result = StructuredExtractionResult {
            source_revision: 7,
            source_route: SemanticRouteIdentity {
                target_id: "target".into(),
                frame_id: "frame".into(),
                url: "https://example.test".into(),
            },
            records: vec![value],
            record_items: vec![
                StructuredExtractionRecord {
                    field: "items".into(),
                    index: 0,
                    value: serde_json::json!({"reference": "r7:b1"}),
                    entity_ids: vec!["r7:b1".into()],
                },
                StructuredExtractionRecord {
                    field: "rows".into(),
                    index: 1,
                    value: serde_json::json!({"reference": "r7:b2"}),
                    entity_ids: vec!["r7:b2".into()],
                },
            ],
            truncated: true,
            provenance: vec!["$.targets".into()],
            field_provenance: vec![StructuredExtractionProvenance {
                field: "items".into(),
                path: "$.targets".into(),
                region_id: Some("region_results".into()),
                entity_ids: vec!["r7:b1".into()],
            }],
            continuation: Some(StructuredExtractionContinuation {
                next_index: 2,
                source_revision: 7,
                contract_hash: "sha256:test".into(),
                region_id: Some("region_results".into()),
                source_route: SemanticRouteIdentity {
                    target_id: "target".into(),
                    frame_id: "frame".into(),
                    url: "https://example.test".into(),
                },
                source_route_fingerprint: Some(route_fingerprint(&SemanticRouteIdentity {
                    target_id: "target".into(),
                    frame_id: "frame".into(),
                    url: "https://example.test".into(),
                })),
            }),
            limits: StructuredExtractionLimits {
                max_items: 2,
                max_bytes: 1024,
                observed_items: 3,
                serialized_bytes: 0,
                truncated: true,
            },
        };
        let serialized_bytes = serialized_extraction_result_bytes(&mut result).unwrap();
        assert_eq!(
            serde_json::to_vec(&result).unwrap().len(),
            serialized_bytes,
            "serializedBytes must equal the complete extraction response size"
        );
        assert!(
            serialized_bytes > serde_json::to_vec(&result.records).unwrap().len(),
            "collection metadata must be included in serializedBytes"
        );
        let mut at_boundary = result.clone();
        at_boundary.limits.serialized_bytes = 0;
        let at_boundary = finalize_extraction_result(at_boundary, serialized_bytes).unwrap();
        assert_eq!(
            serde_json::to_vec(&at_boundary).unwrap().len(),
            serialized_bytes
        );
        let mut below_boundary = result.clone();
        below_boundary.limits.serialized_bytes = 0;
        let error = finalize_extraction_result(below_boundary, serialized_bytes - 1)
            .unwrap_err()
            .to_string();
        assert!(error.contains("maxBytes"));
        let serialized = serde_json::to_value(result).unwrap();
        assert_eq!(serialized["continuation"]["nextIndex"], 2);
        assert_eq!(serialized["sourceRevision"], 7);
        assert_eq!(serialized["recordItems"][0]["field"], "items");
        assert_eq!(serialized["recordItems"][0]["index"], 0);
        assert_eq!(serialized["recordItems"][0]["entityIds"][0], "r7:b1");
        assert_eq!(serialized["recordItems"][1]["field"], "rows");
        assert_eq!(serialized["recordItems"][1]["index"], 1);
        assert_eq!(
            serialized["fieldProvenance"][0]["regionId"],
            "region_results"
        );
        assert_eq!(serialized["fieldProvenance"][0]["entityIds"][0], "r7:b1");
        assert_eq!(serialized["limits"]["observedItems"], 3);
        assert_eq!(serialized["limits"]["truncated"], true);
    }
    #[test]
    fn extraction_contract_rejects_ambiguous_or_unsafe_paths() {
        let mut request = StructuredExtractionRequest {
            fields: vec![
                ExtractionField {
                    name: "title".into(),
                    path: "$.page.title".into(),
                    kind: ExtractionKind::String,
                },
                ExtractionField {
                    name: "title".into(),
                    path: "$.page.url".into(),
                    kind: ExtractionKind::Url,
                },
            ],
            region_id: Some("r".into()),
            start_index: 0,
            continuation: None,
            max_items: 1,
            max_bytes: 1024,
        };
        assert!(validate_extraction_request(&request).is_err());
        request.fields[1].name = "url".into();
        request.fields[1].path = "$.page[href]".into();
        assert!(validate_extraction_request(&request).is_err());
        request.fields[1].path = "$".into();
        assert!(validate_extraction_request(&request).is_err());
    }

    #[test]
    fn missing_and_explicit_null_are_distinct() {
        let field = ExtractionField {
            name: "value".into(),
            path: "$.value".into(),
            kind: ExtractionKind::OptionalString,
        };
        assert!(validate_extracted_value(None, &field).is_err());
        assert_eq!(
            validate_extracted_value(Some(Value::Null), &field).unwrap(),
            Value::Null
        );
    }

    #[test]
    fn extraction_route_redacts_credentials_and_query_identity() {
        let route = SemanticRouteIdentity {
            target_id: "target".into(),
            frame_id: "frame".into(),
            url: "https://user:pass@example.test/path?token=secret#fragment".into(),
        };
        let sanitized = sanitized_route(&route);
        assert_eq!(sanitized.target_id, route.target_id);
        assert_eq!(sanitized.frame_id, route.frame_id);
        assert_eq!(sanitized.url, "https://example.test/path");
    }
}
