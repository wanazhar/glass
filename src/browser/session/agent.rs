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
#[derive(Debug, Clone, Copy, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ExtractionKind {
    Scalar,
    OptionalScalar,
    List,
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

/// Bounded typed extraction output with revision provenance.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StructuredExtractionResult {
    pub source_revision: u64,
    pub source_route: SemanticRouteIdentity,
    pub records: Vec<Value>,
    pub truncated: bool,
    pub provenance: Vec<String>,
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
        let source = if let Some(region_id) = &request.region_id {
            observation
                .regions
                .iter()
                .find(|region| region.id == *region_id)
                .map(serde_json::to_value)
                .transpose()?
                .ok_or_else(|| format!("region not found: {region_id}"))?
        } else {
            serde_json::to_value(&observation)?
        };
        let mut record = Map::new();
        let mut truncated = false;
        for field in &request.fields {
            let value = value_at_path(&source, &field.path).cloned();
            let mut value = validate_extracted_value(value, field)?;
            if let Value::Array(items) = &mut value
                && items.len() > request.max_items
            {
                items.truncate(request.max_items);
                truncated = true;
            }
            record.insert(field.name.clone(), value);
        }
        let serialized = serde_json::to_vec(&record)?;
        if serialized.len() > request.max_bytes {
            return Err(format!(
                "extraction exceeds maxBytes ({} > {})",
                serialized.len(),
                request.max_bytes
            )
            .into());
        }
        Ok(StructuredExtractionResult {
            source_revision: observation.revision,
            source_route: observation.route,
            records: vec![record.into()],
            truncated,
            provenance: request
                .fields
                .iter()
                .map(|field| field.path.clone())
                .collect(),
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
        ExtractionKind::List | ExtractionKind::RepeatedItems | ExtractionKind::Table => {
            value.is_array()
        }
        ExtractionKind::Object => value.is_object(),
    };
    if !valid {
        return Err(format!("field {} does not match its declared type", field.name).into());
    }
    Ok(value)
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
}
