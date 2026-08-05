//! Bounded task-oriented operations built on the guarded session runtime.

use super::*;
use crate::protocol::{RetryClassification, RetryGuidance};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
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
    pub limits: StructuredExtractionLimits,
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
        let observation = self
            .semantic_observe(SemanticObservationLevel::Structured)
            .await?;
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
        let mut truncated = false;
        let mut observed_items = 0usize;
        let mut record_items = Vec::new();
        let mut field_provenance = Vec::with_capacity(request.fields.len());
        for field in &request.fields {
            let value = extraction_source_value(&source, field);
            let mut value = validate_extracted_value(value, field)?;
            let entity_ids = provenance_entity_ids(&value);
            if let Value::Array(items) = &mut value {
                observed_items = observed_items.saturating_add(items.len());
                if items.len() > request.max_items {
                    items.truncate(request.max_items);
                    truncated = true;
                }
                record_items.extend(extraction_record_items(field, &value));
            }
            field_provenance.push(StructuredExtractionProvenance {
                field: field.name.clone(),
                path: field.path.clone(),
                region_id: request.region_id.clone(),
                entity_ids,
            });
            record.insert(field.name.clone(), value);
        }
        let serialized = serde_json::to_vec(&record)?;
        let serialized_bytes = serialized.len();
        if serialized_bytes > request.max_bytes {
            return Err(format!(
                "extraction exceeds maxBytes ({} > {})",
                serialized_bytes, request.max_bytes
            )
            .into());
        }
        Ok(StructuredExtractionResult {
            source_revision: observation.revision,
            source_route: observation.route,
            records: vec![record.into()],
            record_items,
            truncated,
            provenance: request
                .fields
                .iter()
                .map(|field| field.path.clone())
                .collect(),
            field_provenance,
            limits: StructuredExtractionLimits {
                max_items: request.max_items,
                max_bytes: request.max_bytes,
                observed_items,
                serialized_bytes,
                truncated,
            },
        })
    }

    /// Reconcile a run ID conservatively when the original session is gone.
    pub fn recover_run(&self, execution_id: &str) -> BrowserResult<RecoverRunResult> {
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
}

fn validate_extraction_request(request: &StructuredExtractionRequest) -> BrowserResult<()> {
    if request.fields.is_empty() || request.fields.len() > MAX_EXTRACTION_FIELDS {
        return Err(format!("fields must contain 1..={MAX_EXTRACTION_FIELDS} entries").into());
    }
    if !(1..=MAX_EXTRACTION_ITEMS).contains(&request.max_items) {
        return Err(format!("maxItems must be 1..={MAX_EXTRACTION_ITEMS}").into());
    }
    if !(1..=MAX_EXTRACTION_BYTES).contains(&request.max_bytes) {
        return Err(format!("maxBytes must be 1..={MAX_EXTRACTION_BYTES}").into());
    }
    for field in &request.fields {
        if field.name.is_empty() || field.name.len() > 128 || field.path.len() > 512 {
            return Err("extraction field names and paths must be bounded".into());
        }
    }
    Ok(())
}

fn extraction_source_value(source: &Value, field: &ExtractionField) -> Option<Value> {
    let value = value_at_path(source, &field.path).cloned();
    if field.path == "$.structuredRecords"
        && value
            .as_ref()
            .is_some_and(|value| value.as_array().is_some_and(Vec::is_empty))
    {
        value_at_path(source, "$.targets").cloned().or(value)
    } else {
        value
    }
}

fn value_at_path<'a>(value: &'a Value, path: &str) -> Option<&'a Value> {
    if path.is_empty() || path == "$" {
        return Some(value);
    }
    path.trim_start_matches("$.")
        .split('.')
        .try_fold(value, |current, segment| current.get(segment))
}

fn validate_extracted_value(value: Option<Value>, field: &ExtractionField) -> BrowserResult<Value> {
    let value = value.unwrap_or(Value::Null);
    let valid = match field.kind {
        ExtractionKind::Scalar => value.is_string() || value.is_number() || value.is_boolean(),
        ExtractionKind::OptionalScalar => {
            value.is_null() || value.is_string() || value.is_number() || value.is_boolean()
        }
        ExtractionKind::String | ExtractionKind::Enum => value.is_string(),
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
) -> Vec<StructuredExtractionRecord> {
    if !matches!(
        field.kind,
        ExtractionKind::Table | ExtractionKind::RepeatedItems
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
            index,
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
                path: "page.title".into(),
                kind: ExtractionKind::Scalar,
            }],
            region_id: None,
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
        let items = extraction_record_items(&table, &value);
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].field, "rows");
        assert_eq!(items[0].index, 0);
        assert_eq!(items[0].entity_ids, vec!["r7:b1"]);
        assert!(
            extraction_record_items(
                &ExtractionField {
                    kind: ExtractionKind::String,
                    ..table
                },
                &value
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
        let result = StructuredExtractionResult {
            source_revision: 7,
            source_route: SemanticRouteIdentity {
                target_id: "target".into(),
                frame_id: "frame".into(),
                url: "https://example.test".into(),
            },
            records: vec![value],
            record_items: vec![StructuredExtractionRecord {
                field: "items".into(),
                index: 0,
                value: serde_json::json!({"reference": "r7:b1"}),
                entity_ids: vec!["r7:b1".into()],
            }],
            truncated: true,
            provenance: vec!["$.targets".into()],
            field_provenance: vec![StructuredExtractionProvenance {
                field: "items".into(),
                path: "$.targets".into(),
                region_id: Some("region_results".into()),
                entity_ids: vec!["r7:b1".into()],
            }],
            limits: StructuredExtractionLimits {
                max_items: 2,
                max_bytes: 1024,
                observed_items: 3,
                serialized_bytes: 128,
                truncated: true,
            },
        };
        let serialized = serde_json::to_value(result).unwrap();
        assert_eq!(serialized["sourceRevision"], 7);
        assert_eq!(serialized["recordItems"][0]["field"], "items");
        assert_eq!(serialized["recordItems"][0]["index"], 0);
        assert_eq!(serialized["recordItems"][0]["entityIds"][0], "r7:b1");
        assert_eq!(
            serialized["fieldProvenance"][0]["regionId"],
            "region_results"
        );
        assert_eq!(serialized["fieldProvenance"][0]["entityIds"][0], "r7:b1");
        assert_eq!(serialized["limits"]["observedItems"], 3);
        assert_eq!(serialized["limits"]["truncated"], true);
    }
}
