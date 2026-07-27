//! Versioned, declarative workflow definitions.
//!
//! This module contains the validated workflow contract, execution evidence,
//! bounded checkpointing, and resume reconciliation.

use super::types::{BatchMode, BatchStep, BrowserResult, VerificationPredicate};
use super::{
    INTENT_RESOLUTION_SCHEMA_VERSION, IntentConfidence, IntentConstraints, IntentPolicyDecision,
    IntentScope, SemanticIntentAction, SemanticIntentExecutionRequest, SemanticIntentRequest,
    SemanticIntentResult, SemanticResolution, SemanticResolutionPolicy, SemanticRouteIdentity,
    target_fingerprint_digest,
};
use serde::de::Error as DeError;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::time::{Duration, Instant};
use url::Url;

/// The workflow definition schema understood by this crate.
pub const WORKFLOW_SCHEMA_VERSION: u32 = 1;
const MAX_NAME_BYTES: usize = 128;
const MAX_DESCRIPTION_BYTES: usize = 4 * 1024;
const MAX_INPUTS: usize = 64;
const MAX_STEPS: usize = 64;
const MAX_DURATION_MS: u64 = 15 * 60 * 1_000;
const MAX_RETRIES: u32 = 8;
const MAX_EXTRACTED_BYTES: usize = 4 * 1024 * 1024;
const MAX_TEXT_BYTES: usize = 64 * 1024;
const MAX_TARGET_BYTES: usize = 1_024;
const MAX_WAIT_CONDITION_BYTES: usize = 4 * 1024;
const MAX_STEP_REPETITIONS: u32 = 8;
const MAX_WORKFLOW_TRACE_EVENTS: usize = 2_048;
const WORKFLOW_TRACE_SCHEMA_VERSION: u8 = 1;
const WORKFLOW_CHECKPOINT_SCHEMA_VERSION: u8 = 1;
const MAX_WORKFLOW_CHECKPOINT_BYTES: usize = 8 * 1024;
const MAX_CHECKPOINT_HISTORY_STATES: usize = 64;
const MAX_CHECKPOINT_EXECUTION_IDS: usize = 8;
const MAX_INTENT_PURPOSE_BYTES: usize = 256;

/// A complete declarative workflow.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowDefinition {
    /// Schema version, independent of the workflow's business version.
    pub schema_version: u32,
    /// Stable human-readable workflow name.
    pub name: String,
    /// Caller-owned version of this workflow definition.
    pub workflow_version: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub inputs: BTreeMap<String, WorkflowInput>,
    pub budgets: WorkflowBudgets,
    #[serde(default)]
    pub preconditions: Vec<VerificationPredicate>,
    pub steps: Vec<WorkflowStep>,
    pub terminal_condition: VerificationPredicate,
    pub outputs: BTreeMap<String, WorkflowOutputDeclaration>,
}

impl WorkflowDefinition {
    /// Parse and validate a JSON workflow definition.
    pub fn from_json(input: &str) -> Result<Self, WorkflowValidationError> {
        let value: Value = serde_json::from_str(input)
            .map_err(|error| WorkflowValidationError::new("$", format!("invalid JSON: {error}")))?;
        Self::from_value(value)
    }

    /// Deserialize and validate a workflow definition from JSON data.
    pub fn from_value(value: Value) -> Result<Self, WorkflowValidationError> {
        require_object_fields(
            &value,
            &[
                "schemaVersion",
                "name",
                "workflowVersion",
                "inputs",
                "budgets",
                "steps",
                "terminalCondition",
                "outputs",
            ],
        )?;
        reject_unknown_fields(
            &value,
            "$",
            &[
                "schemaVersion",
                "name",
                "workflowVersion",
                "description",
                "inputs",
                "budgets",
                "preconditions",
                "steps",
                "terminalCondition",
                "outputs",
            ],
        )?;
        if let Some(inputs) = value.get("inputs").and_then(Value::as_object) {
            for (name, input) in inputs {
                reject_unknown_fields(
                    input,
                    &format!("inputs.{name}"),
                    &["valueType", "type", "required", "maxLength", "sensitive"],
                )?;
            }
        }
        if let Some(budgets) = value.get("budgets") {
            reject_unknown_fields(
                budgets,
                "budgets",
                &[
                    "maxSteps",
                    "maxDurationMs",
                    "maxRetries",
                    "maxExtractedBytes",
                ],
            )?;
        }
        if let Some(outputs) = value.get("outputs").and_then(Value::as_object) {
            for (name, output) in outputs {
                reject_unknown_fields(
                    output,
                    &format!("outputs.{name}"),
                    &["valueType", "type", "source", "required", "sensitive"],
                )?;
            }
        }
        if let Some(preconditions) = value.get("preconditions").and_then(Value::as_array) {
            for (index, predicate) in preconditions.iter().enumerate() {
                reject_predicate_fields(predicate, &format!("preconditions[{index}]"))?;
            }
        }
        if let Some(predicate) = value.get("terminalCondition") {
            reject_predicate_fields(predicate, "terminalCondition")?;
        }
        if let Some(steps) = value.get("steps").and_then(Value::as_array) {
            for (index, step) in steps.iter().enumerate() {
                reject_unknown_fields(
                    step,
                    &format!("steps[{index}]"),
                    &[
                        "id",
                        "action",
                        "intent",
                        "when",
                        "expect",
                        "beforeRetry",
                        "transaction",
                        "idempotencyKey",
                        "maxRetries",
                        "repeat",
                        "url",
                        "timeoutMs",
                        "target",
                        "text",
                        "value",
                        "condition",
                        "dx",
                        "dy",
                        "includeDom",
                        "includeScreenshot",
                        "includeFormValues",
                    ],
                )?;
                if let Some(intent) = step.get("intent") {
                    if step.get("action").is_some() {
                        return Err(WorkflowValidationError::new(
                            format!("steps[{index}].action"),
                            "semantic intent steps cannot also declare a batch action",
                        ));
                    }
                    reject_unknown_fields(
                        intent,
                        &format!("steps[{index}].intent"),
                        &[
                            "action",
                            "purpose",
                            "intent",
                            "scope",
                            "constraints",
                            "resolutionPolicy",
                            "value",
                        ],
                    )?;
                }
                for field in ["when", "expect", "beforeRetry"] {
                    if let Some(predicate) = step.get(field) {
                        reject_predicate_fields(predicate, &format!("steps[{index}].{field}"))?;
                    }
                }
            }
        }
        let definition: Self = serde_json::from_value(value).map_err(|error| {
            WorkflowValidationError::new("$", format!("invalid workflow shape: {error}"))
        })?;
        definition.validate()?;
        Ok(definition)
    }

    /// Validate all structural and resource-boundary constraints.
    pub fn validate(&self) -> Result<(), WorkflowValidationError> {
        if self.schema_version != WORKFLOW_SCHEMA_VERSION {
            return Err(WorkflowValidationError::new(
                "schemaVersion",
                format!(
                    "unsupported schema version {}; expected {}",
                    self.schema_version, WORKFLOW_SCHEMA_VERSION
                ),
            ));
        }
        validate_name("name", &self.name)?;
        validate_name("workflowVersion", &self.workflow_version)?;
        if let Some(description) = &self.description {
            validate_bytes("description", description, 1, MAX_DESCRIPTION_BYTES)?;
        }
        if self.inputs.len() > MAX_INPUTS {
            return Err(WorkflowValidationError::new(
                "inputs",
                format!("must contain at most {MAX_INPUTS} entries"),
            ));
        }
        for (name, input) in &self.inputs {
            validate_map_key("inputs", name)?;
            input.validate(&format!("inputs.{name}"))?;
        }
        self.budgets.validate("budgets")?;
        if self.steps.is_empty() {
            return Err(WorkflowValidationError::new(
                "steps",
                "must contain at least one step",
            ));
        }
        if self.steps.len() > self.budgets.max_steps as usize {
            return Err(WorkflowValidationError::new(
                "steps",
                "step count exceeds budgets.maxSteps",
            ));
        }
        let expanded_steps: usize = self.steps.iter().map(|step| step.repeat as usize).sum();
        if expanded_steps > self.budgets.max_steps as usize {
            return Err(WorkflowValidationError::new(
                "steps",
                "expanded repetition count exceeds budgets.maxSteps",
            ));
        }

        let mut ids = BTreeSet::new();
        let mut idempotency_keys = BTreeSet::new();
        for (index, step) in self.steps.iter().enumerate() {
            let path = format!("steps[{index}]");
            step.validate(&path, self.budgets.max_retries)?;
            if !ids.insert(step.id.as_str()) {
                return Err(WorkflowValidationError::new(
                    format!("{path}.id"),
                    format!("duplicate step ID {:?}", step.id),
                ));
            }
            if let Some(key) = &step.idempotency_key
                && !idempotency_keys.insert(key.as_str())
            {
                return Err(WorkflowValidationError::new(
                    format!("{path}.idempotencyKey"),
                    format!("duplicate idempotency key {:?}", key),
                ));
            }
        }
        for (index, predicate) in self.preconditions.iter().enumerate() {
            validate_predicate(predicate, &format!("preconditions[{index}]"))?;
        }
        validate_predicate(&self.terminal_condition, "terminalCondition")?;
        for (name, output) in &self.outputs {
            validate_map_key("outputs", name)?;
            output.validate(&format!("outputs.{name}"))?;
        }
        Ok(())
    }

    /// Return stable JSON suitable for hashing, caching, or audit records.
    pub fn to_canonical_json(&self) -> Result<String, WorkflowValidationError> {
        self.validate()?;
        serde_json::to_string(self).map_err(|error| {
            WorkflowValidationError::new("$", format!("cannot serialize workflow: {error}"))
        })
    }

    /// Validate caller-provided input values before execution starts.
    pub fn validate_inputs(
        &self,
        values: &BTreeMap<String, Value>,
    ) -> Result<(), WorkflowValidationError> {
        for name in values.keys() {
            if !self.inputs.contains_key(name) {
                return Err(WorkflowValidationError::new(
                    format!("inputs.{name}"),
                    "value has no declared input",
                ));
            }
        }
        for (name, declaration) in &self.inputs {
            match values.get(name) {
                Some(value) => declaration.validate_value(&format!("inputs.{name}"), value)?,
                None if declaration.required => {
                    return Err(WorkflowValidationError::new(
                        format!("inputs.{name}"),
                        "required input is missing",
                    ));
                }
                None => {}
            }
        }
        Ok(())
    }

    /// Resolve bounded `${inputs.name}` placeholders in declared actions.
    /// Resolution happens before browser startup or dispatch and never
    /// evaluates arbitrary expressions.
    pub fn resolve_actions(
        &self,
        values: &BTreeMap<String, Value>,
    ) -> Result<Vec<WorkflowStep>, WorkflowValidationError> {
        self.validate_inputs(values)?;
        self.steps
            .iter()
            .enumerate()
            .map(|(index, step)| {
                let mut resolved = step.clone();
                if let Some(intent) = &step.intent {
                    resolved.intent = Some(resolve_workflow_intent(
                        intent,
                        values,
                        &format!("steps[{index}].intent"),
                    )?);
                } else {
                    resolved.action = resolve_batch_step(
                        &step.action,
                        values,
                        &format!("steps[{index}].action"),
                    )?;
                }
                resolved.validate(&format!("steps[{index}]"), self.budgets.max_retries)?;
                Ok(resolved)
            })
            .collect()
    }
}

/// A declared workflow input and its accepted JSON type.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowInput {
    #[serde(alias = "type")]
    pub value_type: WorkflowValueType,
    #[serde(default = "default_true")]
    pub required: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_length: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sensitive: Option<bool>,
}

impl WorkflowInput {
    fn validate(&self, path: &str) -> Result<(), WorkflowValidationError> {
        if self.max_length == Some(0) || self.max_length.is_some_and(|value| value > MAX_TEXT_BYTES)
        {
            return Err(WorkflowValidationError::new(
                format!("{path}.maxLength"),
                format!("must be 1..={MAX_TEXT_BYTES}"),
            ));
        }
        Ok(())
    }

    fn validate_value(&self, path: &str, value: &Value) -> Result<(), WorkflowValidationError> {
        let valid = match self.value_type {
            WorkflowValueType::String => value.is_string(),
            WorkflowValueType::Integer => value.as_i64().is_some() || value.as_u64().is_some(),
            WorkflowValueType::Number => value.is_number(),
            WorkflowValueType::Boolean => value.is_boolean(),
            WorkflowValueType::Url => value
                .as_str()
                .is_some_and(|value| Url::parse(value).is_ok()),
        };
        if !valid {
            return Err(WorkflowValidationError::new(
                path,
                format!("expected {}", self.value_type),
            ));
        }
        if let Some(max_length) = self.max_length {
            let length = value
                .as_str()
                .map_or_else(|| value.to_string().len(), str::len);
            if length > max_length {
                return Err(WorkflowValidationError::new(
                    path,
                    format!("value exceeds maxLength {max_length}"),
                ));
            }
        }
        Ok(())
    }
}

fn default_true() -> bool {
    true
}

/// Runtime resource limits for a workflow.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowBudgets {
    pub max_steps: u32,
    pub max_duration_ms: u64,
    #[serde(default)]
    pub max_retries: u32,
    pub max_extracted_bytes: usize,
}

impl WorkflowBudgets {
    fn validate(&self, path: &str) -> Result<(), WorkflowValidationError> {
        if self.max_steps == 0 || self.max_steps as usize > MAX_STEPS {
            return Err(WorkflowValidationError::new(
                format!("{path}.maxSteps"),
                format!("must be 1..={MAX_STEPS}"),
            ));
        }
        if self.max_duration_ms == 0 || self.max_duration_ms > MAX_DURATION_MS {
            return Err(WorkflowValidationError::new(
                format!("{path}.maxDurationMs"),
                format!("must be 1..={MAX_DURATION_MS}"),
            ));
        }
        if self.max_retries > MAX_RETRIES {
            return Err(WorkflowValidationError::new(
                format!("{path}.maxRetries"),
                format!("must be <= {MAX_RETRIES}"),
            ));
        }
        if self.max_extracted_bytes == 0 || self.max_extracted_bytes > MAX_EXTRACTED_BYTES {
            return Err(WorkflowValidationError::new(
                format!("{path}.maxExtractedBytes"),
                format!("must be 1..={MAX_EXTRACTED_BYTES}"),
            ));
        }
        Ok(())
    }
}

/// JSON value types supported by workflow inputs and outputs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkflowValueType {
    String,
    Integer,
    Number,
    Boolean,
    Url,
}

impl fmt::Display for WorkflowValueType {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::String => "string",
            Self::Integer => "integer",
            Self::Number => "number",
            Self::Boolean => "boolean",
            Self::Url => "url",
        })
    }
}

/// A named action with an optional postcondition.
#[derive(Debug, Clone)]
pub struct WorkflowStep {
    pub id: String,
    pub action: BatchStep,
    pub intent: Option<WorkflowIntentStep>,
    pub when: Option<VerificationPredicate>,
    pub expect: Option<VerificationPredicate>,
    pub before_retry: Option<VerificationPredicate>,
    pub transaction: WorkflowTransactionClass,
    pub idempotency_key: Option<String>,
    pub max_retries: u32,
    pub repeat: u32,
}

/// A semantic workflow action resolved against fresh page evidence at runtime.
/// Existing locator-based workflow actions remain unchanged.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkflowIntentStep {
    pub action: SemanticIntentAction,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub purpose: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub intent: Option<String>,
    #[serde(default)]
    pub scope: IntentScope,
    #[serde(default)]
    pub constraints: IntentConstraints,
    #[serde(default = "default_workflow_resolution_policy")]
    pub resolution_policy: SemanticResolutionPolicy,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,
}

impl WorkflowIntentStep {
    fn validate(&self, path: &str) -> Result<(), WorkflowValidationError> {
        let supplied = self.purpose.is_some() as u8 + self.intent.is_some() as u8;
        if supplied != 1 {
            return Err(WorkflowValidationError::new(
                format!("{path}.purpose"),
                "provide exactly one of purpose or intent",
            ));
        }
        if let Some(purpose) = &self.purpose {
            validate_bytes(
                &format!("{path}.purpose"),
                purpose,
                1,
                MAX_INTENT_PURPOSE_BYTES,
            )?;
        }
        if let Some(intent) = &self.intent {
            validate_bytes(
                &format!("{path}.intent"),
                intent,
                1,
                MAX_INTENT_PURPOSE_BYTES * 2,
            )?;
        }
        if let Some(value) = &self.value {
            validate_bytes(&format!("{path}.value"), value, 0, 4_096)?;
        }
        self.execution_request(path).map(|_| ())
    }

    fn execution_request(
        &self,
        path: &str,
    ) -> Result<SemanticIntentExecutionRequest, WorkflowValidationError> {
        let intent = self
            .intent
            .clone()
            .or_else(|| self.purpose.as_deref().map(purpose_to_intent))
            .ok_or_else(|| {
                WorkflowValidationError::new(format!("{path}.purpose"), "intent phrase is required")
            })?;
        let request = SemanticIntentRequest {
            schema_version: INTENT_RESOLUTION_SCHEMA_VERSION,
            intent,
            action: self.action,
            scope: self.scope.clone(),
            constraints: self.constraints.clone(),
            resolution_policy: self.resolution_policy,
            expected_revision: None,
        };
        let execution = SemanticIntentExecutionRequest {
            request,
            candidate_id: "workflow-selected-candidate".into(),
            value: self.value.clone(),
        };
        execution.validate().map_err(|error| {
            WorkflowValidationError::new(format!("{path}.{}", error.path), error.reason)
        })?;
        Ok(execution)
    }
}

fn default_workflow_resolution_policy() -> SemanticResolutionPolicy {
    SemanticResolutionPolicy::RequireUniqueHighConfidence
}

fn purpose_to_intent(purpose: &str) -> String {
    let mut result = String::with_capacity(purpose.len() + 8);
    for (index, character) in purpose.chars().enumerate() {
        if character.is_ascii_uppercase() && index > 0 {
            result.push(' ');
        }
        result.push(character.to_ascii_lowercase());
    }
    result
}

/// Semantic target captured by the workflow recorder.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowRecordedTarget {
    pub role: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub region_kind: Option<super::SemanticRegionKind>,
}

/// Bounded route evidence captured by a semantic recorder.
///
/// Browser target and frame handles are hashed because they are useful for
/// comparing a recording with later evidence but are not valid replay
/// selectors. Query strings and fragments are removed from the retained URL.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowRecordedRoute {
    pub target_digest: String,
    pub frame_digest: String,
    pub url: String,
}

/// Resolution evidence retained with one semantic draft step.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowRecordedSemantic {
    pub intent: String,
    pub normalized_intent: String,
    pub action: SemanticIntentAction,
    pub resolution: SemanticResolution,
    pub policy_decision: IntentPolicyDecision,
    pub candidate_count: usize,
    pub excluded_count: usize,
    pub ambiguous: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub revision: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub route: Option<WorkflowRecordedRoute>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_fingerprint: Option<String>,
}

/// Confidence attached to a recorded draft, never to a runtime guarantee.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkflowRecordingConfidence {
    High,
    Medium,
    Low,
}

/// One reviewable semantic recorder draft step.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowDraftStep {
    pub id: String,
    pub action: BatchStep,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub intent: Option<WorkflowIntentStep>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target: Option<WorkflowRecordedTarget>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub semantic: Option<WorkflowRecordedSemantic>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expect: Option<VerificationPredicate>,
    pub transaction: WorkflowTransactionClass,
    pub confidence: WorkflowRecordingConfidence,
    pub review_required: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input_name: Option<String>,
    #[serde(default)]
    pub sensitive_input: bool,
}

/// A bounded recorder output that remains a draft until explicitly reviewed.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowDraft {
    pub schema_version: u32,
    pub name: String,
    pub workflow_version: String,
    pub steps: Vec<WorkflowDraftStep>,
}

/// In-memory recorder for semantic workflow drafts.
#[derive(Debug, Clone)]
pub struct WorkflowRecorder {
    draft: WorkflowDraft,
}

impl WorkflowRecorder {
    /// Start a bounded recorder draft. Recording is local and does not attach
    /// to Chrome or intercept browser traffic.
    pub fn new(name: impl Into<String>, workflow_version: impl Into<String>) -> Self {
        Self {
            draft: WorkflowDraft {
                schema_version: WORKFLOW_SCHEMA_VERSION,
                name: name.into(),
                workflow_version: workflow_version.into(),
                steps: Vec::new(),
            },
        }
    }

    /// Record a semantic click draft using an explicit role and accessible name.
    pub fn record_click(
        &mut self,
        id: impl Into<String>,
        role: impl Into<String>,
        name: impl Into<String>,
        expect: Option<VerificationPredicate>,
    ) -> Result<(), WorkflowValidationError> {
        let target = recorded_target(role.into(), name.into(), None, None)?;
        let locator = format!("role={};name={}", target.role, target.name);
        self.push(WorkflowDraftStep {
            id: id.into(),
            action: BatchStep::Click { target: locator },
            intent: None,
            target: Some(target),
            semantic: None,
            expect,
            transaction: WorkflowTransactionClass::Unknown,
            confidence: WorkflowRecordingConfidence::High,
            review_required: true,
            input_name: None,
            sensitive_input: false,
        })
    }

    /// Record text as a typed input placeholder, never as a literal value.
    pub fn record_type_input(
        &mut self,
        id: impl Into<String>,
        role: impl Into<String>,
        name: impl Into<String>,
        input_name: impl Into<String>,
    ) -> Result<(), WorkflowValidationError> {
        let target = recorded_target(role.into(), name.into(), None, None)?;
        let input_name = input_name.into();
        validate_name("inputName", &input_name)?;
        let sensitive_input = looks_sensitive_input_name(&input_name);
        let locator = format!("role={};name={}", target.role, target.name);
        self.push(WorkflowDraftStep {
            id: id.into(),
            action: BatchStep::Type {
                text: format!("${{inputs.{input_name}}}"),
                target: Some(locator),
            },
            intent: None,
            target: Some(target),
            semantic: None,
            expect: None,
            transaction: WorkflowTransactionClass::Unknown,
            confidence: WorkflowRecordingConfidence::High,
            review_required: true,
            input_name: Some(input_name),
            sensitive_input,
        })
    }

    /// Record a read-only observation draft.
    pub fn record_observe(&mut self, id: impl Into<String>) -> Result<(), WorkflowValidationError> {
        self.push(WorkflowDraftStep {
            id: id.into(),
            action: BatchStep::Observe {
                include_dom: false,
                include_screenshot: false,
                include_form_values: false,
            },
            intent: None,
            target: None,
            semantic: None,
            expect: None,
            transaction: WorkflowTransactionClass::ReadOnly,
            confidence: WorkflowRecordingConfidence::High,
            review_required: true,
            input_name: None,
            sensitive_input: false,
        })
    }

    /// Record a semantic resolution as a reviewable workflow intent step.
    ///
    /// The result may be ambiguous, rejected, or lack a selected candidate;
    /// those states are retained as evidence and never turned into a replay
    /// target. Value-bearing actions receive an input placeholder only.
    pub fn record_semantic_intent(
        &mut self,
        id: impl Into<String>,
        request: &SemanticIntentRequest,
        result: &SemanticIntentResult,
        input_name: Option<impl Into<String>>,
        transaction: WorkflowTransactionClass,
        expect: Option<VerificationPredicate>,
    ) -> Result<(), WorkflowValidationError> {
        request
            .validate()
            .map_err(|error| WorkflowValidationError::new("semantic.request", error.to_string()))?;
        result
            .validate()
            .map_err(|error| WorkflowValidationError::new("semantic.result", error.to_string()))?;
        if request.action != result.action || request.intent != result.intent {
            return Err(WorkflowValidationError::new(
                "semantic",
                "request and result action/intent do not match",
            ));
        }

        let input_name = input_name.map(Into::into);
        let value = match request.action {
            SemanticIntentAction::Type | SemanticIntentAction::Select => {
                let input_name = input_name.as_deref().ok_or_else(|| {
                    WorkflowValidationError::new(
                        "inputName",
                        "type and select recordings require an input name",
                    )
                })?;
                validate_name("inputName", input_name)?;
                Some(format!("${{inputs.{input_name}}}"))
            }
            _ if input_name.is_some() => {
                return Err(WorkflowValidationError::new(
                    "inputName",
                    "only type and select recordings accept an input name",
                ));
            }
            _ => None,
        };

        let selected = result.selected_candidate.as_deref().and_then(|id| {
            result
                .candidates
                .iter()
                .find(|candidate| candidate.id == id)
        });
        let target = selected
            .map(|candidate| {
                recorded_target(
                    candidate.role.clone(),
                    candidate.name.clone(),
                    candidate.region_kind.map(|kind| format!("{kind:?}")),
                    candidate.region_kind,
                )
            })
            .transpose()?;
        let target_fingerprint = selected.and_then(|candidate| {
            candidate.fingerprint.as_ref().map(|fingerprint| {
                target_fingerprint_digest(
                    &candidate.role,
                    &candidate.name,
                    candidate.input_type.as_deref(),
                    candidate.region_kind,
                    fingerprint.purpose,
                )
            })
        });
        let confidence = selected
            .map(|candidate| recording_confidence(candidate.confidence))
            .unwrap_or(WorkflowRecordingConfidence::Low);
        let semantic = WorkflowRecordedSemantic {
            intent: result.intent.clone(),
            normalized_intent: result.normalized_intent.clone(),
            action: result.action,
            resolution: result.resolution,
            policy_decision: result.policy_decision,
            candidate_count: result.candidates.len(),
            excluded_count: result.excluded_count,
            ambiguous: matches!(result.resolution, SemanticResolution::Ambiguous),
            revision: result.revision,
            route: result.route.as_ref().map(recorded_route),
            target_fingerprint,
        };
        let intent = WorkflowIntentStep {
            action: request.action,
            purpose: None,
            intent: Some(request.intent.clone()),
            scope: request.scope.clone(),
            constraints: request.constraints.clone(),
            resolution_policy: request.resolution_policy,
            value,
        };
        self.push(WorkflowDraftStep {
            id: id.into(),
            action: BatchStep::Observe {
                include_dom: false,
                include_screenshot: false,
                include_form_values: false,
            },
            intent: Some(intent),
            target,
            semantic: Some(semantic),
            expect,
            transaction,
            confidence,
            review_required: true,
            sensitive_input: input_name
                .as_deref()
                .is_some_and(looks_sensitive_input_name),
            input_name,
        })
    }

    pub fn draft(&self) -> &WorkflowDraft {
        &self.draft
    }

    /// Convert a reviewed draft into the normal validated workflow contract.
    pub fn into_definition(
        self,
        inputs: BTreeMap<String, WorkflowInput>,
        budgets: WorkflowBudgets,
        terminal_condition: VerificationPredicate,
        outputs: BTreeMap<String, WorkflowOutputDeclaration>,
    ) -> Result<WorkflowDefinition, WorkflowValidationError> {
        let definition = WorkflowDefinition {
            schema_version: self.draft.schema_version,
            name: self.draft.name,
            workflow_version: self.draft.workflow_version,
            description: Some("Recorded draft; review before execution.".into()),
            inputs,
            budgets,
            preconditions: Vec::new(),
            steps: self
                .draft
                .steps
                .into_iter()
                .map(|step| WorkflowStep {
                    id: step.id,
                    action: step.action,
                    intent: step.intent,
                    when: None,
                    expect: step.expect,
                    before_retry: None,
                    transaction: step.transaction,
                    idempotency_key: None,
                    max_retries: 0,
                    repeat: 1,
                })
                .collect(),
            terminal_condition,
            outputs,
        };
        definition.validate()?;
        Ok(definition)
    }

    fn push(&mut self, step: WorkflowDraftStep) -> Result<(), WorkflowValidationError> {
        if self.draft.steps.len() >= MAX_STEPS {
            return Err(WorkflowValidationError::new(
                "steps",
                format!("must contain at most {MAX_STEPS} entries"),
            ));
        }
        validate_name("steps.id", &step.id)?;
        if self.draft.steps.iter().any(|item| item.id == step.id) {
            return Err(WorkflowValidationError::new(
                "steps.id",
                format!("duplicate step ID {:?}", step.id),
            ));
        }
        self.draft.steps.push(step);
        Ok(())
    }
}

fn recorded_target(
    role: String,
    name: String,
    context: Option<String>,
    region_kind: Option<super::SemanticRegionKind>,
) -> Result<WorkflowRecordedTarget, WorkflowValidationError> {
    validate_bytes("target.role", &role, 1, 128)?;
    validate_bytes("target.name", &name, 1, MAX_TARGET_BYTES)?;
    if role.contains([';', '\n', '\r']) || name.contains([';', '\n', '\r']) {
        return Err(WorkflowValidationError::new(
            "target",
            "semantic target fields cannot contain locator separators or newlines",
        ));
    }
    if let Some(context) = &context {
        validate_bytes("target.context", context, 1, 256)?;
    }
    Ok(WorkflowRecordedTarget {
        role,
        name,
        context,
        region_kind,
    })
}

fn recording_confidence(confidence: IntentConfidence) -> WorkflowRecordingConfidence {
    match confidence {
        IntentConfidence::Exact | IntentConfidence::High => WorkflowRecordingConfidence::High,
        IntentConfidence::Medium => WorkflowRecordingConfidence::Medium,
        IntentConfidence::Low | IntentConfidence::Insufficient => WorkflowRecordingConfidence::Low,
    }
}

fn looks_sensitive_input_name(name: &str) -> bool {
    let normalized = name.to_ascii_lowercase();
    [
        "password", "passwd", "secret", "token", "api_key", "apikey", "cookie",
    ]
    .iter()
    .any(|term| normalized.contains(term))
}

fn recorded_route(route: &SemanticRouteIdentity) -> WorkflowRecordedRoute {
    let url = Url::parse(&route.url)
        .map(|mut parsed| {
            let _ = parsed.set_username("");
            let _ = parsed.set_password(None);
            parsed.set_query(None);
            parsed.set_fragment(None);
            parsed.to_string()
        })
        .unwrap_or_else(|_| bound_workflow_text(&route.url, 2_048));
    WorkflowRecordedRoute {
        target_digest: hash_recorded_identifier(&route.target_id),
        frame_digest: hash_recorded_identifier(&route.frame_id),
        url,
    }
}

fn hash_recorded_identifier(value: &str) -> String {
    let digest = Sha256::digest(value.as_bytes());
    format!("sha256:{digest:x}")
}

/// Effect classification used to decide whether a failed attempt may be
/// replayed before dispatch.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkflowTransactionClass {
    ReadOnly,
    Idempotent,
    ConditionallyIdempotent,
    NonIdempotent,
    #[default]
    Unknown,
}

impl WorkflowTransactionClass {
    /// Whether this class permits a retry known to have happened before
    /// dispatch.
    pub fn permits_pre_dispatch_retry(self) -> bool {
        matches!(
            self,
            Self::ReadOnly | Self::Idempotent | Self::ConditionallyIdempotent
        )
    }
}

impl Serialize for WorkflowStep {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut action = if let Some(intent) = &self.intent {
            serde_json::json!({ "intent": intent })
        } else {
            serde_json::to_value(&self.action).map_err(serde::ser::Error::custom)?
        };
        if self.intent.is_none() {
            let object = action
                .as_object_mut()
                .ok_or_else(|| serde::ser::Error::custom("workflow action must be an object"))?;
            for (internal, public) in [
                ("timeout_ms", "timeoutMs"),
                ("include_dom", "includeDom"),
                ("include_screenshot", "includeScreenshot"),
                ("include_form_values", "includeFormValues"),
            ] {
                if let Some(value) = object.remove(internal) {
                    object.insert(public.to_string(), value);
                }
            }
        }

        let mut workflow = serde_json::Map::new();
        workflow.insert("id".into(), Value::String(self.id.clone()));
        if let Value::Object(action) = action {
            workflow.extend(action);
        }
        if let Some(when) = &self.when {
            workflow.insert(
                "when".into(),
                serde_json::to_value(when).map_err(serde::ser::Error::custom)?,
            );
        }
        if let Some(expect) = &self.expect {
            workflow.insert(
                "expect".into(),
                serde_json::to_value(expect).map_err(serde::ser::Error::custom)?,
            );
        }
        if let Some(before_retry) = &self.before_retry {
            workflow.insert(
                "beforeRetry".into(),
                serde_json::to_value(before_retry).map_err(serde::ser::Error::custom)?,
            );
        }
        workflow.insert(
            "transaction".into(),
            serde_json::to_value(self.transaction).map_err(serde::ser::Error::custom)?,
        );
        if let Some(key) = &self.idempotency_key {
            workflow.insert("idempotencyKey".into(), Value::String(key.clone()));
        }
        if self.max_retries > 0 {
            workflow.insert("maxRetries".into(), Value::from(self.max_retries));
        }
        if self.repeat > 1 {
            workflow.insert("repeat".into(), Value::from(self.repeat));
        }
        Value::Object(workflow).serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for WorkflowStep {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let mut workflow = serde_json::Map::<String, Value>::deserialize(deserializer)?;
        let id = workflow
            .remove("id")
            .ok_or_else(|| D::Error::custom("workflow step is missing id"))?;
        let id = serde_json::from_value(id).map_err(D::Error::custom)?;
        let when = workflow
            .remove("when")
            .map(serde_json::from_value)
            .transpose()
            .map_err(D::Error::custom)?;
        let expect = workflow
            .remove("expect")
            .map(serde_json::from_value)
            .transpose()
            .map_err(D::Error::custom)?;
        let before_retry = workflow
            .remove("beforeRetry")
            .map(serde_json::from_value)
            .transpose()
            .map_err(D::Error::custom)?;
        let transaction = workflow
            .remove("transaction")
            .map(serde_json::from_value)
            .transpose()
            .map_err(D::Error::custom)?
            .unwrap_or_default();
        let idempotency_key = workflow
            .remove("idempotencyKey")
            .map(serde_json::from_value)
            .transpose()
            .map_err(D::Error::custom)?;
        let max_retries = workflow
            .remove("maxRetries")
            .map(serde_json::from_value)
            .transpose()
            .map_err(D::Error::custom)?
            .unwrap_or(0);
        let repeat = workflow
            .remove("repeat")
            .map(serde_json::from_value)
            .transpose()
            .map_err(D::Error::custom)?
            .unwrap_or(1);
        let intent = workflow
            .remove("intent")
            .map(serde_json::from_value)
            .transpose()
            .map_err(D::Error::custom)?;
        for (public, internal) in [
            ("timeoutMs", "timeout_ms"),
            ("includeDom", "include_dom"),
            ("includeScreenshot", "include_screenshot"),
            ("includeFormValues", "include_form_values"),
        ] {
            if let Some(value) = workflow.remove(public) {
                workflow.insert(internal.into(), value);
            }
        }
        let action = if intent.is_some() {
            BatchStep::Observe {
                include_dom: false,
                include_screenshot: false,
                include_form_values: false,
            }
        } else {
            serde_json::from_value(Value::Object(workflow)).map_err(D::Error::custom)?
        };
        Ok(Self {
            id,
            action,
            intent,
            when,
            expect,
            before_retry,
            transaction,
            idempotency_key,
            max_retries,
            repeat,
        })
    }
}

impl WorkflowStep {
    fn validate(
        &self,
        path: &str,
        workflow_max_retries: u32,
    ) -> Result<(), WorkflowValidationError> {
        validate_name(&format!("{path}.id"), &self.id)?;
        if let Some(intent) = &self.intent {
            intent.validate(&format!("{path}.intent"))?;
        } else {
            validate_batch_step(&self.action, &format!("{path}.action"))?;
        }
        if let Some(predicate) = &self.when {
            validate_predicate(predicate, &format!("{path}.when"))?;
        }
        if let Some(predicate) = &self.expect {
            validate_predicate(predicate, &format!("{path}.expect"))?;
        }
        if let Some(predicate) = &self.before_retry {
            validate_predicate(predicate, &format!("{path}.beforeRetry"))?;
        }
        if let Some(key) = &self.idempotency_key {
            validate_bytes(&format!("{path}.idempotencyKey"), key, 1, 256)?;
        }
        if self.repeat == 0 || self.repeat > MAX_STEP_REPETITIONS {
            return Err(WorkflowValidationError::new(
                format!("{path}.repeat"),
                format!("must be 1..={MAX_STEP_REPETITIONS}"),
            ));
        }
        if self.repeat > 1 && self.when.is_some() {
            return Err(WorkflowValidationError::new(
                format!("{path}.repeat"),
                "conditional steps cannot repeat automatically",
            ));
        }
        if self.repeat > 1 && !self.transaction.permits_pre_dispatch_retry() {
            return Err(WorkflowValidationError::new(
                format!("{path}.repeat"),
                "unknown or non-idempotent steps cannot repeat automatically",
            ));
        }
        if self.max_retries > workflow_max_retries {
            return Err(WorkflowValidationError::new(
                format!("{path}.maxRetries"),
                "step retry count exceeds budgets.maxRetries",
            ));
        }
        match self.transaction {
            WorkflowTransactionClass::ConditionallyIdempotent if self.idempotency_key.is_none() => {
                return Err(WorkflowValidationError::new(
                    format!("{path}.idempotencyKey"),
                    "conditionally idempotent steps require an idempotency key",
                ));
            }
            WorkflowTransactionClass::NonIdempotent | WorkflowTransactionClass::Unknown
                if self.max_retries > 0 =>
            {
                return Err(WorkflowValidationError::new(
                    format!("{path}.maxRetries"),
                    "non-idempotent or unknown steps cannot be retried automatically",
                ));
            }
            _ => {}
        }
        Ok(())
    }
}

/// Durable state of one workflow step.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkflowStepState {
    Pending,
    Ready,
    Preflight,
    Resolving,
    NotDispatched,
    Dispatched,
    EffectObserved,
    Verified,
    OutputsExtracted,
    Committed,
    FailedBeforeDispatch,
    FailedAfterDispatch,
    Indeterminate,
    Skipped,
}

impl WorkflowStepState {
    /// Return whether a state transition is valid for the linear runner.
    pub fn can_transition_to(self, next: Self) -> bool {
        matches!(
            (self, next),
            (Self::Pending, Self::Ready | Self::Skipped)
                | (Self::Ready, Self::Preflight | Self::Skipped)
                | (
                    Self::Preflight,
                    Self::Resolving | Self::EffectObserved | Self::FailedBeforeDispatch
                )
                | (Self::Resolving, Self::NotDispatched | Self::Dispatched)
                | (Self::NotDispatched, Self::FailedBeforeDispatch)
                | (
                    Self::Dispatched,
                    Self::EffectObserved | Self::FailedAfterDispatch | Self::Indeterminate
                )
                | (
                    Self::EffectObserved,
                    Self::Verified | Self::FailedAfterDispatch | Self::Indeterminate
                )
                | (Self::Verified, Self::OutputsExtracted)
                | (Self::OutputsExtracted, Self::Committed)
                | (Self::FailedBeforeDispatch, Self::Ready)
                | (Self::Committed, Self::Ready)
        )
    }
}

/// The retained state and transition history for one workflow step.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowStepRecord {
    pub id: String,
    pub state: WorkflowStepState,
    pub history: Vec<WorkflowStepState>,
    pub attempts: u32,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub execution_ids: Vec<String>,
    #[serde(default)]
    pub dispatch_acknowledged: bool,
    #[serde(default)]
    pub effect_observed: bool,
    #[serde(default)]
    pub postcondition_verified: bool,
    #[serde(default)]
    pub retry_safe: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub previous_revision: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub current_revision: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub branch_decision: Option<WorkflowBranchDecision>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub intent_evidence: Option<WorkflowIntentEvidence>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Bounded evidence linking a semantic workflow step to its accepted target.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowIntentEvidence {
    pub resolution_id: String,
    pub candidate_id: String,
    pub revision: u64,
    pub resolution: super::SemanticResolution,
    pub policy_decision: super::IntentPolicyDecision,
    pub confidence: super::IntentConfidence,
    pub fingerprint: Option<super::SemanticTargetFingerprint>,
}

impl WorkflowStepRecord {
    fn new(id: &str) -> Self {
        Self {
            id: id.to_string(),
            state: WorkflowStepState::Pending,
            history: vec![WorkflowStepState::Pending],
            attempts: 0,
            execution_ids: Vec::new(),
            dispatch_acknowledged: false,
            effect_observed: false,
            postcondition_verified: false,
            retry_safe: false,
            previous_revision: None,
            current_revision: None,
            branch_decision: None,
            intent_evidence: None,
            error: None,
        }
    }

    fn transition(&mut self, next: WorkflowStepState) -> Result<(), String> {
        if !self.state.can_transition_to(next) {
            return Err(format!(
                "invalid workflow step transition {} -> {}",
                state_name(self.state),
                state_name(next)
            ));
        }
        self.state = next;
        self.history.push(next);
        Ok(())
    }

    fn fail(&mut self, state: WorkflowStepState, error: &str) {
        let _ = self.transition(state);
        self.error = Some(bound_workflow_text(error, 512));
    }
}

/// Overall outcome of a workflow run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkflowRunStatus {
    Completed,
    Failed,
    BudgetExhausted,
    ResumeRequired,
}

/// Evidence that the workflow's terminal condition was satisfied.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowTerminalProof {
    pub predicate: VerificationPredicate,
    pub revision: u64,
    pub state: String,
}

/// Result of a linear workflow run.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowRunResult {
    pub run_id: String,
    pub name: String,
    pub workflow_version: String,
    pub status: WorkflowRunStatus,
    pub steps: Vec<WorkflowStepRecord>,
    pub trace: WorkflowTrace,
    pub outputs: BTreeMap<String, WorkflowOutput>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub terminal_proof: Option<WorkflowTerminalProof>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub failed_step: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub failure: Option<String>,
    pub initial_revision: u64,
    pub final_revision: u64,
}

/// Evidence for a declarative step condition evaluated before dispatch.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowBranchDecision {
    pub step_id: String,
    pub predicate: VerificationPredicate,
    pub matched: bool,
}

/// Deterministic state-transition trace for replay and debugging.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowTrace {
    #[serde(default = "default_workflow_trace_schema_version")]
    pub schema_version: u8,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run_id: Option<String>,
    pub events: Vec<WorkflowTraceEvent>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub branch_decisions: Vec<WorkflowBranchDecision>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub intent_resolutions: Vec<WorkflowIntentEvidence>,
}

/// One ordered state transition in a workflow trace.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowTraceEvent {
    pub sequence: u64,
    pub step_id: String,
    pub state: WorkflowStepState,
    pub attempt: u32,
}

impl WorkflowTrace {
    /// Build a stable trace from retained step histories.
    pub fn from_steps(steps: &[WorkflowStepRecord]) -> Self {
        let mut events = Vec::new();
        for step in steps {
            let mut attempt = 0u32;
            for state in &step.history {
                if events.len() == MAX_WORKFLOW_TRACE_EVENTS {
                    break;
                }
                if *state == WorkflowStepState::Preflight {
                    attempt = attempt.saturating_add(1);
                }
                events.push(WorkflowTraceEvent {
                    sequence: events.len() as u64,
                    step_id: step.id.clone(),
                    state: *state,
                    attempt,
                });
            }
        }
        let branch_decisions = steps
            .iter()
            .filter_map(|step| step.branch_decision.clone())
            .collect();
        let intent_resolutions = steps
            .iter()
            .filter_map(|step| step.intent_evidence.clone())
            .collect();
        Self {
            schema_version: WORKFLOW_TRACE_SCHEMA_VERSION,
            run_id: None,
            events,
            branch_decisions,
            intent_resolutions,
        }
    }

    /// Validate sequence ordering and the trace event budget.
    pub fn validate(&self) -> Result<(), WorkflowValidationError> {
        if self.schema_version != WORKFLOW_TRACE_SCHEMA_VERSION {
            return Err(WorkflowValidationError::new(
                "trace.schemaVersion",
                format!(
                    "unsupported trace schema version {}; expected {}",
                    self.schema_version, WORKFLOW_TRACE_SCHEMA_VERSION
                ),
            ));
        }
        if self
            .run_id
            .as_ref()
            .is_some_and(|run_id| run_id.is_empty() || run_id.len() > 128)
        {
            return Err(WorkflowValidationError::new(
                "trace.runId",
                "run ID must contain 1 to 128 bytes",
            ));
        }
        if self.events.len() > MAX_WORKFLOW_TRACE_EVENTS {
            return Err(WorkflowValidationError::new(
                "trace.events",
                format!("must contain at most {MAX_WORKFLOW_TRACE_EVENTS} events"),
            ));
        }
        for (index, event) in self.events.iter().enumerate() {
            if event.sequence != index as u64 {
                return Err(WorkflowValidationError::new(
                    format!("trace.events[{index}].sequence"),
                    "sequence must be contiguous from zero",
                ));
            }
        }
        for (index, decision) in self.branch_decisions.iter().enumerate() {
            if decision.step_id.is_empty() {
                return Err(WorkflowValidationError::new(
                    format!("trace.branchDecisions[{index}].stepId"),
                    "step ID must not be empty",
                ));
            }
            validate_predicate(
                &decision.predicate,
                &format!("trace.branchDecisions[{index}].predicate"),
            )?;
        }
        Ok(())
    }

    /// Replay the trace into step records without dispatching browser work.
    ///
    /// Replay is intentionally an inspection operation: it checks that the
    /// trace references the declared steps in order, follows the workflow
    /// state machine, and carries monotonic attempt numbers. A truncated
    /// trace is accepted as a valid prefix when it ends at the event budget.
    pub fn replay(
        &self,
        workflow: &WorkflowDefinition,
    ) -> Result<Vec<WorkflowStepRecord>, WorkflowValidationError> {
        workflow.validate()?;
        self.validate()?;
        let mut records: Vec<_> = workflow
            .steps
            .iter()
            .map(|step| WorkflowStepRecord::new(&step.id))
            .collect();
        let mut seen = vec![false; records.len()];
        let mut highest_step = 0usize;
        let mut started = false;

        for (event_index, event) in self.events.iter().enumerate() {
            let step_index = workflow
                .steps
                .iter()
                .position(|step| step.id == event.step_id)
                .ok_or_else(|| {
                    WorkflowValidationError::new(
                        format!("trace.events[{event_index}].stepId"),
                        "step ID is not declared by the workflow",
                    )
                })?;
            if !started {
                if step_index != 0 {
                    return Err(WorkflowValidationError::new(
                        format!("trace.events[{event_index}].stepId"),
                        "trace must begin with the first declared step",
                    ));
                }
                started = true;
            }
            if step_index > highest_step.saturating_add(1) {
                return Err(WorkflowValidationError::new(
                    format!("trace.events[{event_index}].stepId"),
                    "trace skips a declared step",
                ));
            }
            if step_index < highest_step {
                return Err(WorkflowValidationError::new(
                    format!("trace.events[{event_index}].stepId"),
                    "trace returns to an earlier step",
                ));
            }
            highest_step = highest_step.max(step_index);

            let record = &mut records[step_index];
            if !seen[step_index] {
                if event.state != WorkflowStepState::Pending || event.attempt != 0 {
                    return Err(WorkflowValidationError::new(
                        format!("trace.events[{event_index}]"),
                        "each step must begin with pending at attempt zero",
                    ));
                }
                seen[step_index] = true;
                continue;
            }
            if event.state == WorkflowStepState::Preflight {
                if event.attempt != record.attempts.saturating_add(1) {
                    return Err(WorkflowValidationError::new(
                        format!("trace.events[{event_index}].attempt"),
                        "preflight attempt must increment by one",
                    ));
                }
                record.attempts = event.attempt;
            } else if event.attempt != record.attempts {
                return Err(WorkflowValidationError::new(
                    format!("trace.events[{event_index}].attempt"),
                    "event attempt does not match the current step attempt",
                ));
            }
            record.transition(event.state).map_err(|reason| {
                WorkflowValidationError::new(format!("trace.events[{event_index}].state"), reason)
            })?;
        }
        for decision in &self.branch_decisions {
            let index = workflow
                .steps
                .iter()
                .position(|step| step.id == decision.step_id)
                .ok_or_else(|| {
                    WorkflowValidationError::new(
                        "trace.branchDecisions",
                        "branch decision references an undeclared step",
                    )
                })?;
            if workflow.steps[index].when.as_ref() != Some(&decision.predicate) {
                return Err(WorkflowValidationError::new(
                    "trace.branchDecisions",
                    "branch decision predicate does not match the workflow",
                ));
            }
            records[index].branch_decision = Some(decision.clone());
        }
        Ok(records)
    }
}

fn default_workflow_trace_schema_version() -> u8 {
    WORKFLOW_TRACE_SCHEMA_VERSION
}

/// Bounded, deterministic workflow checkpoint. Input values and page content
/// are intentionally excluded; only route identity and step state are kept.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowCheckpoint {
    pub schema_version: u8,
    #[serde(default)]
    pub run_id: String,
    pub workflow_name: String,
    pub workflow_version: String,
    pub definition_hash: String,
    pub status: WorkflowRunStatus,
    pub next_step_index: usize,
    pub steps: Vec<WorkflowCheckpointStep>,
    pub page: WorkflowCheckpointPage,
}

/// Redacted state for one checkpointed workflow step.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowCheckpointStep {
    pub id: String,
    pub state: WorkflowStepState,
    pub attempts: u32,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub history: Vec<WorkflowStepState>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub execution_ids: Vec<String>,
    #[serde(default)]
    pub dispatch_acknowledged: bool,
    #[serde(default)]
    pub effect_observed: bool,
    #[serde(default)]
    pub postcondition_verified: bool,
    #[serde(default)]
    pub retry_safe: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub previous_revision: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub current_revision: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub branch_decision: Option<WorkflowBranchDecision>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub intent_evidence: Option<WorkflowIntentEvidence>,
}

/// Bounded page identity used to reject unsafe resume attempts.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowCheckpointPage {
    pub target_id: String,
    pub frame_id: String,
    pub url: String,
    pub title: String,
    pub revision: u64,
}

/// Safe next action after a checkpoint has been reconciled with the live page.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowResumePlan {
    pub workflow_name: String,
    pub workflow_version: String,
    pub next_step_index: usize,
    pub current_revision: u64,
    pub reconciled: bool,
}

/// Reason a workflow checkpoint cannot be resumed safely.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum WorkflowResumeError {
    SchemaVersionMismatch {
        expected: u8,
        found: u8,
    },
    DefinitionMismatch,
    RouteChanged,
    InvalidState {
        step_id: String,
        state: WorkflowStepState,
    },
    CheckpointTooLarge,
    CheckpointShape(String),
}

impl fmt::Display for WorkflowResumeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::SchemaVersionMismatch { expected, found } => {
                write!(
                    formatter,
                    "workflow checkpoint schema mismatch: expected {expected}, found {found}"
                )
            }
            Self::DefinitionMismatch => {
                formatter.write_str("workflow definition does not match checkpoint")
            }
            Self::RouteChanged => {
                formatter.write_str("workflow checkpoint route or target changed")
            }
            Self::InvalidState { step_id, state } => {
                write!(
                    formatter,
                    "workflow step {step_id:?} cannot be resumed from {state:?}"
                )
            }
            Self::CheckpointTooLarge => {
                formatter.write_str("workflow checkpoint exceeds the 8 KiB limit")
            }
            Self::CheckpointShape(message) => {
                write!(formatter, "invalid workflow checkpoint: {message}")
            }
        }
    }
}

impl std::error::Error for WorkflowResumeError {}

impl WorkflowRunResult {
    fn failed(
        workflow: &WorkflowDefinition,
        run_id: String,
        steps: Vec<WorkflowStepRecord>,
        failed_step: Option<String>,
        failure: impl Into<String>,
        initial_revision: u64,
        final_revision: u64,
    ) -> Self {
        let mut trace = WorkflowTrace::from_steps(&steps);
        trace.run_id = Some(run_id.clone());
        Self {
            run_id,
            name: workflow.name.clone(),
            workflow_version: workflow.workflow_version.clone(),
            status: WorkflowRunStatus::Failed,
            steps,
            trace,
            outputs: BTreeMap::new(),
            terminal_proof: None,
            failed_step,
            failure: Some(bound_workflow_text(&failure.into(), 512)),
            initial_revision,
            final_revision,
        }
    }

    fn budget_exhausted(
        workflow: &WorkflowDefinition,
        run_id: String,
        steps: Vec<WorkflowStepRecord>,
        failed_step: Option<String>,
        reason: impl Into<String>,
        initial_revision: u64,
        final_revision: u64,
    ) -> Self {
        let mut trace = WorkflowTrace::from_steps(&steps);
        trace.run_id = Some(run_id.clone());
        Self {
            run_id,
            name: workflow.name.clone(),
            workflow_version: workflow.workflow_version.clone(),
            status: WorkflowRunStatus::BudgetExhausted,
            steps,
            trace,
            outputs: BTreeMap::new(),
            terminal_proof: None,
            failed_step,
            failure: Some(bound_workflow_text(&reason.into(), 512)),
            initial_revision,
            final_revision,
        }
    }

    fn resume_required(
        workflow: &WorkflowDefinition,
        run_id: String,
        steps: Vec<WorkflowStepRecord>,
        failed_step: Option<String>,
        reason: impl Into<String>,
        initial_revision: u64,
        final_revision: u64,
    ) -> Self {
        let mut trace = WorkflowTrace::from_steps(&steps);
        trace.run_id = Some(run_id.clone());
        Self {
            run_id,
            name: workflow.name.clone(),
            workflow_version: workflow.workflow_version.clone(),
            status: WorkflowRunStatus::ResumeRequired,
            steps,
            trace,
            outputs: BTreeMap::new(),
            terminal_proof: None,
            failed_step,
            failure: Some(bound_workflow_text(&reason.into(), 512)),
            initial_revision,
            final_revision,
        }
    }
}

impl super::BrowserSession {
    async fn execute_workflow_intent_step(
        &self,
        intent: &WorkflowIntentStep,
        expected_revision: u64,
    ) -> BrowserResult<(super::types::BatchOutcome, WorkflowIntentEvidence)> {
        let mut execution = intent.execution_request("workflow.intent")?;
        execution.request.expected_revision = Some(expected_revision);
        let resolution = self.resolve_intent(&execution.request).await?;
        let candidate_id = resolution.selected_candidate.clone().ok_or_else(|| {
            format!(
                "semantic workflow intent was not uniquely authorized: resolution={}, policy={}",
                serde_json::to_string(&resolution.resolution).unwrap_or_else(|_| "unknown".into()),
                serde_json::to_string(&resolution.policy_decision)
                    .unwrap_or_else(|_| "unknown".into())
            )
        })?;
        execution.candidate_id = candidate_id;
        let result = self.execute_intent(&execution).await?;
        let accepted_candidate = result
            .resolution
            .candidates
            .iter()
            .find(|candidate| candidate.id == result.candidate_id)
            .ok_or("executed intent result omitted its accepted candidate")?;
        let evidence = WorkflowIntentEvidence {
            resolution_id: result.resolution_id.clone(),
            candidate_id: result.candidate_id.clone(),
            revision: result
                .resolution
                .revision
                .ok_or("executed intent result omitted revision")?,
            resolution: result.resolution.resolution,
            policy_decision: result.resolution.policy_decision,
            confidence: accepted_candidate.confidence,
            fingerprint: accepted_candidate.fingerprint.clone(),
        };
        let Some(action) = result.action else {
            return Err(result
                .reason
                .unwrap_or_else(|| "semantic workflow intent was not executed".into())
                .into());
        };
        let execution_id = action.execution_id.clone();
        Ok((
            super::types::BatchOutcome {
                mode: BatchMode::Fixed,
                initial_revision: expected_revision,
                final_revision: current_revision(self),
                steps: vec![super::types::BatchStepOutcome::Success {
                    index: 0,
                    action: "intent".into(),
                    response_bytes: serde_json::to_string(&action).ok().map(|value| value.len()),
                    execution_id: Some(execution_id),
                }],
                completed: 1,
                failed: 0,
                total: 1,
                success: true,
            },
            evidence,
        ))
    }

    /// Execute a validated workflow linearly through the existing batch
    /// policy and action runtime, retaining bounded per-step evidence.
    pub async fn run_workflow(
        &self,
        workflow: &WorkflowDefinition,
        inputs: &BTreeMap<String, Value>,
    ) -> BrowserResult<WorkflowRunResult> {
        workflow.validate()?;
        workflow.validate_inputs(inputs)?;
        let resolved_steps = workflow.resolve_actions(inputs)?;
        let initial_revision = self
            .page_revision
            .load(std::sync::atomic::Ordering::Relaxed);
        let run_id = format!("run_{}", self.next_execution_id());
        let started = Instant::now();
        let duration_budget = Duration::from_millis(workflow.budgets.max_duration_ms);
        let mut executed_steps = 0u32;
        let mut records: Vec<_> = resolved_steps
            .iter()
            .map(|step| WorkflowStepRecord::new(&step.id))
            .collect();

        for predicate in &workflow.preconditions {
            if workflow_budget_expired(started, duration_budget) {
                skip_remaining(&mut records, 0);
                return Ok(WorkflowRunResult::budget_exhausted(
                    workflow,
                    run_id.clone(),
                    records,
                    None,
                    "workflow maxDurationMs budget exhausted before a precondition",
                    initial_revision,
                    current_revision(self),
                ));
            }
            if let Err(error) = self
                .verify(
                    predicate.clone(),
                    workflow_budget_remaining(started, duration_budget),
                )
                .await
            {
                skip_remaining(&mut records, 0);
                if workflow_budget_expired(started, duration_budget) {
                    return Ok(WorkflowRunResult::budget_exhausted(
                        workflow,
                        run_id.clone(),
                        records,
                        None,
                        "workflow maxDurationMs budget exhausted while checking a precondition",
                        initial_revision,
                        current_revision(self),
                    ));
                }
                return Ok(WorkflowRunResult::failed(
                    workflow,
                    run_id.clone(),
                    records,
                    None,
                    format!("workflow precondition failed: {error}"),
                    initial_revision,
                    current_revision(self),
                ));
            }
        }

        for (index, step) in resolved_steps.iter().enumerate() {
            for repetition in 0..step.repeat {
                if executed_steps >= workflow.budgets.max_steps
                    || workflow_budget_expired(started, duration_budget)
                {
                    skip_remaining(&mut records, index);
                    return Ok(WorkflowRunResult::budget_exhausted(
                        workflow,
                        run_id.clone(),
                        records,
                        Some(step.id.clone()),
                        if executed_steps >= workflow.budgets.max_steps {
                            "workflow maxSteps budget exhausted before dispatch"
                        } else {
                            "workflow maxDurationMs budget exhausted before dispatch"
                        },
                        initial_revision,
                        current_revision(self),
                    ));
                }
                executed_steps = executed_steps.saturating_add(1);
                if repetition > 0 {
                    let record = &mut records[index];
                    let _ = record.transition(WorkflowStepState::Ready);
                }
                if let Some(predicate) = &step.when {
                    {
                        let record = &mut records[index];
                        if record.state == WorkflowStepState::Pending {
                            let _ = record.transition(WorkflowStepState::Ready);
                        }
                    }
                    match self.evaluate_predicate_once(predicate).await {
                        Ok((matched, _state)) => {
                            let record = &mut records[index];
                            record.branch_decision = Some(WorkflowBranchDecision {
                                step_id: step.id.clone(),
                                predicate: predicate.clone(),
                                matched,
                            });
                            if !matched {
                                let _ = record.transition(WorkflowStepState::Skipped);
                                break;
                            }
                        }
                        Err(error) => {
                            let message = bound_workflow_text(&error.to_string(), 512);
                            let record = &mut records[index];
                            let _ = record.transition(WorkflowStepState::Preflight);
                            record.fail(WorkflowStepState::FailedBeforeDispatch, &message);
                            skip_remaining(&mut records, index + 1);
                            if workflow_budget_expired(started, duration_budget) {
                                return Ok(WorkflowRunResult::budget_exhausted(
                                    workflow,
                                    run_id.clone(),
                                    records,
                                    Some(step.id.clone()),
                                    "workflow maxDurationMs budget exhausted while evaluating a branch",
                                    initial_revision,
                                    current_revision(self),
                                ));
                            }
                            return Ok(WorkflowRunResult::failed(
                                workflow,
                                run_id.clone(),
                                records,
                                Some(step.id.clone()),
                                message,
                                initial_revision,
                                current_revision(self),
                            ));
                        }
                    }
                }
                let mut attempt_number = 0;
                let mut effect_marker_completed = false;
                let mut intent_evidence = None;
                let outcome = loop {
                    let attempt_revision = current_revision(self);
                    {
                        let record = &mut records[index];
                        if record.previous_revision.is_none() {
                            record.previous_revision = Some(attempt_revision);
                        }
                        record.retry_safe = step.transaction.permits_pre_dispatch_retry();
                        if record.state == WorkflowStepState::Pending {
                            let _ = record.transition(WorkflowStepState::Ready);
                        }
                        let _ = record.transition(WorkflowStepState::Preflight);
                        let _ = record.transition(WorkflowStepState::Resolving);
                        attempt_number += 1;
                        record.attempts = record.attempts.saturating_add(1);
                    }

                    let outcome = if let Some(intent) = &step.intent {
                        match self
                            .execute_workflow_intent_step(intent, attempt_revision)
                            .await
                        {
                            Ok((outcome, evidence)) => {
                                intent_evidence = Some(evidence);
                                Ok(outcome)
                            }
                            Err(error) => Err(error),
                        }
                    } else {
                        match workflow_target(&step.action) {
                            Some(target) => match self.resolve_element(target).await {
                                Ok(_) => {
                                    self.run_batch_with_mode(
                                        std::slice::from_ref(&step.action),
                                        false,
                                        BatchMode::Unguarded,
                                        None,
                                    )
                                    .await
                                }
                                Err(error) => Err(error),
                            },
                            None => {
                                self.run_batch_with_mode(
                                    std::slice::from_ref(&step.action),
                                    false,
                                    BatchMode::Unguarded,
                                    None,
                                )
                                .await
                            }
                        }
                    };
                    match outcome {
                        Ok(outcome) => break outcome,
                        Err(error) => {
                            let message = bound_workflow_text(&error.to_string(), 512);
                            let retry = can_retry_before_dispatch(
                                step.transaction,
                                false,
                                attempt_number,
                                step.max_retries,
                            );
                            {
                                let record = &mut records[index];
                                record.current_revision = Some(current_revision(self));
                                let _ = record.transition(WorkflowStepState::NotDispatched);
                                record.fail(WorkflowStepState::FailedBeforeDispatch, &message);
                                if retry {
                                    let _ = record.transition(WorkflowStepState::Ready);
                                }
                            }
                            if retry {
                                let marker_matches = match &step.before_retry {
                                    Some(predicate) => {
                                        match self.evaluate_predicate_once(predicate).await {
                                            Ok((matched, _)) => Some(matched),
                                            Err(error) => {
                                                let message = bound_workflow_text(
                                                    &format!(
                                                        "effect marker could not be evaluated: {error}"
                                                    ),
                                                    512,
                                                );
                                                let record = &mut records[index];
                                                let _ =
                                                    record.transition(WorkflowStepState::Preflight);
                                                record.fail(
                                                    WorkflowStepState::FailedBeforeDispatch,
                                                    &message,
                                                );
                                                skip_remaining(&mut records, index + 1);
                                                return Ok(WorkflowRunResult::failed(
                                                    workflow,
                                                    run_id.clone(),
                                                    records,
                                                    Some(step.id.clone()),
                                                    message,
                                                    initial_revision,
                                                    current_revision(self),
                                                ));
                                            }
                                        }
                                    }
                                    None => None,
                                };
                                if marker_matches == Some(true) {
                                    effect_marker_completed = true;
                                    break super::types::BatchOutcome {
                                        mode: BatchMode::Unguarded,
                                        initial_revision: current_revision(self),
                                        final_revision: current_revision(self),
                                        steps: Vec::new(),
                                        completed: 0,
                                        failed: 0,
                                        total: 0,
                                        success: true,
                                    };
                                }
                                continue;
                            }
                            let message = records[index]
                                .error
                                .clone()
                                .unwrap_or_else(|| "workflow step failed".into());
                            skip_remaining(&mut records, index + 1);
                            return Ok(WorkflowRunResult::failed(
                                workflow,
                                run_id.clone(),
                                records,
                                Some(step.id.clone()),
                                message,
                                initial_revision,
                                current_revision(self),
                            ));
                        }
                    }
                };

                if effect_marker_completed {
                    commit_workflow_effect_marker(&mut records[index], current_revision(self));
                    continue;
                }

                if !outcome.success {
                    let message = outcome
                        .steps
                        .last()
                        .and_then(|step| match step {
                            super::types::BatchStepOutcome::Error { message, .. } => {
                                Some(message.as_str())
                            }
                            super::types::BatchStepOutcome::Success { .. } => None,
                        })
                        .unwrap_or("workflow action failed after dispatch");
                    let message = {
                        let record = &mut records[index];
                        record.dispatch_acknowledged = true;
                        record.current_revision = Some(current_revision(self));
                        let _ = record.transition(WorkflowStepState::Dispatched);
                        record.fail(WorkflowStepState::Indeterminate, message);
                        record
                            .error
                            .clone()
                            .unwrap_or_else(|| "workflow step failed".into())
                    };
                    skip_remaining(&mut records, index + 1);
                    return Ok(WorkflowRunResult::resume_required(
                        workflow,
                        run_id.clone(),
                        records,
                        Some(step.id.clone()),
                        message,
                        initial_revision,
                        current_revision(self),
                    ));
                }

                {
                    records[index].intent_evidence = intent_evidence;
                    for execution_id in outcome.steps.iter().filter_map(|step| match step {
                        super::types::BatchStepOutcome::Success {
                            execution_id: Some(execution_id),
                            ..
                        } => Some(execution_id.clone()),
                        _ => None,
                    }) {
                        records[index].execution_ids.push(execution_id);
                    }
                    let record = &mut records[index];
                    record.dispatch_acknowledged = true;
                    record.effect_observed = true;
                    record.current_revision = Some(current_revision(self));
                    let _ = record.transition(WorkflowStepState::Dispatched);
                    let _ = record.transition(WorkflowStepState::EffectObserved);
                }
                if let Some(predicate) = &step.expect
                    && let Err(error) = self
                        .verify(
                            predicate.clone(),
                            workflow_budget_remaining(started, duration_budget),
                        )
                        .await
                {
                    let message = {
                        let record = &mut records[index];
                        record.current_revision = Some(current_revision(self));
                        record.fail(WorkflowStepState::FailedAfterDispatch, &error.to_string());
                        record
                            .error
                            .clone()
                            .unwrap_or_else(|| "workflow verification failed".into())
                    };
                    skip_remaining(&mut records, index + 1);
                    if workflow_budget_expired(started, duration_budget) {
                        return Ok(WorkflowRunResult::budget_exhausted(
                            workflow,
                            run_id.clone(),
                            records,
                            Some(step.id.clone()),
                            "workflow maxDurationMs budget exhausted while verifying a step",
                            initial_revision,
                            current_revision(self),
                        ));
                    }
                    return Ok(WorkflowRunResult::resume_required(
                        workflow,
                        run_id.clone(),
                        records,
                        Some(step.id.clone()),
                        message,
                        initial_revision,
                        current_revision(self),
                    ));
                }
                let record = &mut records[index];
                record.postcondition_verified = true;
                let _ = record.transition(WorkflowStepState::Verified);
                let _ = record.transition(WorkflowStepState::OutputsExtracted);
                let _ = record.transition(WorkflowStepState::Committed);
            }
        }

        if workflow_budget_expired(started, duration_budget) {
            return Ok(WorkflowRunResult::budget_exhausted(
                workflow,
                run_id.clone(),
                records,
                None,
                "workflow maxDurationMs budget exhausted before terminal verification",
                initial_revision,
                current_revision(self),
            ));
        }
        let terminal_proof = match self
            .verify(
                workflow.terminal_condition.clone(),
                workflow_budget_remaining(started, duration_budget),
            )
            .await
        {
            Ok(outcome) => WorkflowTerminalProof {
                predicate: outcome.predicate,
                revision: current_revision(self),
                state: outcome.state,
            },
            Err(error) => {
                if workflow_budget_expired(started, duration_budget) {
                    return Ok(WorkflowRunResult::budget_exhausted(
                        workflow,
                        run_id.clone(),
                        records,
                        None,
                        "workflow maxDurationMs budget exhausted while verifying the terminal condition",
                        initial_revision,
                        current_revision(self),
                    ));
                }
                return Ok(WorkflowRunResult::failed(
                    workflow,
                    run_id.clone(),
                    records,
                    None,
                    format!("workflow terminal condition was not proven: {error}"),
                    initial_revision,
                    current_revision(self),
                ));
            }
        };
        if workflow_budget_expired(started, duration_budget) {
            return Ok(WorkflowRunResult::budget_exhausted(
                workflow,
                run_id.clone(),
                records,
                None,
                "workflow maxDurationMs budget exhausted before output extraction",
                initial_revision,
                current_revision(self),
            ));
        }
        let outputs = match extract_workflow_outputs(self, workflow).await {
            Ok(outputs) => outputs,
            Err(error) => {
                if workflow_budget_expired(started, duration_budget) {
                    return Ok(WorkflowRunResult::budget_exhausted(
                        workflow,
                        run_id.clone(),
                        records,
                        None,
                        "workflow maxDurationMs budget exhausted while extracting outputs",
                        initial_revision,
                        current_revision(self),
                    ));
                }
                return Ok(WorkflowRunResult::failed(
                    workflow,
                    run_id.clone(),
                    records,
                    None,
                    format!("workflow output extraction failed: {error}"),
                    initial_revision,
                    current_revision(self),
                ));
            }
        };
        if workflow_budget_expired(started, duration_budget) {
            return Ok(WorkflowRunResult::budget_exhausted(
                workflow,
                run_id.clone(),
                records,
                None,
                "workflow maxDurationMs budget exhausted after output extraction",
                initial_revision,
                current_revision(self),
            ));
        }

        let mut trace = WorkflowTrace::from_steps(&records);
        trace.run_id = Some(run_id.clone());
        Ok(WorkflowRunResult {
            run_id,
            name: workflow.name.clone(),
            workflow_version: workflow.workflow_version.clone(),
            status: WorkflowRunStatus::Completed,
            steps: records,
            trace,
            outputs,
            terminal_proof: Some(terminal_proof),
            failed_step: None,
            failure: None,
            initial_revision,
            final_revision: current_revision(self),
        })
    }
}

impl super::BrowserSession {
    /// Export a deterministic, redacted workflow checkpoint bounded to 8 KiB.
    pub async fn export_workflow_checkpoint(
        &self,
        workflow: &WorkflowDefinition,
        result: &WorkflowRunResult,
    ) -> BrowserResult<WorkflowCheckpoint> {
        workflow.validate()?;
        if result.name != workflow.name
            || result.workflow_version != workflow.workflow_version
            || result.steps.len() != workflow.steps.len()
        {
            return Err(WorkflowResumeError::CheckpointShape(
                "run result does not belong to workflow definition".into(),
            )
            .into());
        }
        let page = self.page_info().await?;
        let next_step_index = result
            .steps
            .iter()
            .position(|step| step.state != WorkflowStepState::Committed)
            .unwrap_or(result.steps.len());
        let checkpoint = WorkflowCheckpoint {
            schema_version: WORKFLOW_CHECKPOINT_SCHEMA_VERSION,
            run_id: result.run_id.clone(),
            workflow_name: workflow.name.clone(),
            workflow_version: workflow.workflow_version.clone(),
            definition_hash: workflow_definition_hash(workflow)?,
            status: result.status,
            next_step_index,
            steps: result
                .steps
                .iter()
                .map(|step| WorkflowCheckpointStep {
                    id: step.id.clone(),
                    state: step.state,
                    attempts: step.attempts,
                    history: step.history.clone(),
                    execution_ids: step.execution_ids.clone(),
                    dispatch_acknowledged: step.dispatch_acknowledged,
                    effect_observed: step.effect_observed,
                    postcondition_verified: step.postcondition_verified,
                    retry_safe: step.retry_safe,
                    previous_revision: step.previous_revision,
                    current_revision: step.current_revision,
                    branch_decision: step.branch_decision.clone(),
                    intent_evidence: step.intent_evidence.clone(),
                })
                .collect(),
            page: WorkflowCheckpointPage {
                target_id: bound_workflow_text(&page.target_id, 256),
                frame_id: bound_workflow_text(&page.frame_id, 256),
                url: bound_workflow_text(&page.url, 1_024),
                title: bound_workflow_text(&page.title, 1_024),
                revision: current_revision(self),
            },
        };
        checkpoint.validate_size()?;
        Ok(checkpoint)
    }

    /// Parse and validate a workflow checkpoint without contacting Chrome.
    pub fn parse_workflow_checkpoint(
        input: &str,
    ) -> Result<WorkflowCheckpoint, WorkflowResumeError> {
        let checkpoint: WorkflowCheckpoint = serde_json::from_str(input)
            .map_err(|error| WorkflowResumeError::CheckpointShape(error.to_string()))?;
        checkpoint.validate_size()?;
        Ok(checkpoint)
    }

    /// Reconcile a checkpoint with the current route and return the only safe
    /// next step. This method never dispatches a browser action.
    pub async fn reconcile_workflow_checkpoint(
        &self,
        workflow: &WorkflowDefinition,
        checkpoint: &WorkflowCheckpoint,
    ) -> BrowserResult<WorkflowResumePlan> {
        workflow.validate()?;
        checkpoint.validate_size()?;
        if checkpoint.schema_version != WORKFLOW_CHECKPOINT_SCHEMA_VERSION {
            return Err(WorkflowResumeError::SchemaVersionMismatch {
                expected: WORKFLOW_CHECKPOINT_SCHEMA_VERSION,
                found: checkpoint.schema_version,
            }
            .into());
        }
        if checkpoint.workflow_name != workflow.name
            || checkpoint.workflow_version != workflow.workflow_version
            || checkpoint.definition_hash != workflow_definition_hash(workflow)?
        {
            return Err(WorkflowResumeError::DefinitionMismatch.into());
        }
        if checkpoint.steps.len() != workflow.steps.len()
            || checkpoint
                .steps
                .iter()
                .zip(&workflow.steps)
                .any(|(checkpoint_step, step)| checkpoint_step.id != step.id)
        {
            return Err(WorkflowResumeError::CheckpointShape(
                "checkpoint steps do not match workflow steps".into(),
            )
            .into());
        }

        let page = self.page_info().await?;
        if checkpoint.page.target_id != page.target_id
            || checkpoint.page.frame_id != page.frame_id
            || checkpoint.page.url != bound_workflow_text(&page.url, 1_024)
            || checkpoint.page.title != bound_workflow_text(&page.title, 1_024)
        {
            return Err(WorkflowResumeError::RouteChanged.into());
        }

        let mut next_step_index = checkpoint.steps.len();
        for (index, checkpoint_step) in checkpoint.steps.iter().enumerate() {
            if checkpoint_step.state != WorkflowStepState::Committed {
                next_step_index = index;
                break;
            }
        }
        if checkpoint.next_step_index != next_step_index {
            return Err(WorkflowResumeError::CheckpointShape(
                "nextStepIndex does not match step states".into(),
            )
            .into());
        }
        if checkpoint.status == WorkflowRunStatus::Completed
            && next_step_index != workflow.steps.len()
        {
            return Err(WorkflowResumeError::CheckpointShape(
                "completed checkpoint has uncommitted steps".into(),
            )
            .into());
        }
        if next_step_index < workflow.steps.len() {
            let step = &workflow.steps[next_step_index];
            let state = checkpoint.steps[next_step_index].state;
            if state == WorkflowStepState::FailedBeforeDispatch
                && !step.transaction.permits_pre_dispatch_retry()
            {
                return Err(WorkflowResumeError::InvalidState {
                    step_id: step.id.clone(),
                    state,
                }
                .into());
            }
            if !matches!(
                state,
                WorkflowStepState::Pending | WorkflowStepState::FailedBeforeDispatch
            ) {
                return Err(WorkflowResumeError::InvalidState {
                    step_id: step.id.clone(),
                    state,
                }
                .into());
            }
            for checkpoint_step in checkpoint.steps.iter().skip(next_step_index + 1) {
                if checkpoint_step.state != WorkflowStepState::Skipped {
                    return Err(WorkflowResumeError::InvalidState {
                        step_id: checkpoint_step.id.clone(),
                        state: checkpoint_step.state,
                    }
                    .into());
                }
            }
        }

        Ok(WorkflowResumePlan {
            workflow_name: workflow.name.clone(),
            workflow_version: workflow.workflow_version.clone(),
            next_step_index,
            current_revision: current_revision(self),
            reconciled: true,
        })
    }

    /// Reconcile a checkpoint and execute only its safe pending suffix.
    /// Previously committed steps are never re-dispatched by this method.
    pub async fn resume_workflow(
        &self,
        workflow: &WorkflowDefinition,
        inputs: &BTreeMap<String, Value>,
        checkpoint: &WorkflowCheckpoint,
    ) -> BrowserResult<WorkflowRunResult> {
        let plan = self
            .reconcile_workflow_checkpoint(workflow, checkpoint)
            .await?;
        if plan.next_step_index >= workflow.steps.len() {
            return Err(WorkflowResumeError::CheckpointShape(
                "workflow checkpoint is already complete".into(),
            )
            .into());
        }
        let mut suffix = workflow.clone();
        suffix.steps = workflow.steps[plan.next_step_index..].to_vec();
        let mut result = self.run_workflow(&suffix, inputs).await?;
        let mut prefix = checkpoint.steps[..plan.next_step_index]
            .iter()
            .map(checkpoint_step_to_record)
            .collect::<Vec<_>>();
        prefix.append(&mut result.steps);
        result.steps = prefix;
        result.trace = WorkflowTrace::from_steps(&result.steps);
        result.trace.run_id = Some(result.run_id.clone());
        Ok(result)
    }
}

fn checkpoint_step_to_record(step: &WorkflowCheckpointStep) -> WorkflowStepRecord {
    let history = if step.history.is_empty() {
        vec![
            WorkflowStepState::Pending,
            WorkflowStepState::Ready,
            WorkflowStepState::Preflight,
            WorkflowStepState::Resolving,
            WorkflowStepState::Dispatched,
            WorkflowStepState::EffectObserved,
            WorkflowStepState::Verified,
            WorkflowStepState::OutputsExtracted,
            WorkflowStepState::Committed,
        ]
    } else {
        step.history.clone()
    };
    WorkflowStepRecord {
        id: step.id.clone(),
        state: step.state,
        history,
        attempts: step.attempts,
        execution_ids: step.execution_ids.clone(),
        dispatch_acknowledged: step.dispatch_acknowledged,
        effect_observed: step.effect_observed,
        postcondition_verified: step.postcondition_verified,
        retry_safe: step.retry_safe,
        previous_revision: step.previous_revision,
        current_revision: step.current_revision,
        branch_decision: step.branch_decision.clone(),
        intent_evidence: step.intent_evidence.clone(),
        error: None,
    }
}

impl WorkflowCheckpoint {
    /// Serialize a checkpoint after enforcing its size and schema bounds.
    pub fn to_canonical_json(&self) -> Result<String, WorkflowResumeError> {
        self.validate_size()?;
        serde_json::to_string(self)
            .map_err(|error| WorkflowResumeError::CheckpointShape(error.to_string()))
    }

    fn validate_size(&self) -> Result<(), WorkflowResumeError> {
        if self.schema_version != WORKFLOW_CHECKPOINT_SCHEMA_VERSION {
            return Err(WorkflowResumeError::SchemaVersionMismatch {
                expected: WORKFLOW_CHECKPOINT_SCHEMA_VERSION,
                found: self.schema_version,
            });
        }
        if self.steps.len() > MAX_STEPS {
            return Err(WorkflowResumeError::CheckpointShape(
                "checkpoint contains too many steps".into(),
            ));
        }
        if self.run_id.len() > 128 {
            return Err(WorkflowResumeError::CheckpointShape(
                "checkpoint runId is too long".into(),
            ));
        }
        for (index, step) in self.steps.iter().enumerate() {
            if step.history.len() > MAX_CHECKPOINT_HISTORY_STATES {
                return Err(WorkflowResumeError::CheckpointShape(format!(
                    "checkpoint step {index} contains too much state history"
                )));
            }
            if !step.history.is_empty() && step.history.last() != Some(&step.state) {
                return Err(WorkflowResumeError::CheckpointShape(format!(
                    "checkpoint step {index} history does not end at its state"
                )));
            }
            if step.execution_ids.len() > MAX_CHECKPOINT_EXECUTION_IDS
                || step.execution_ids.iter().any(|id| id.len() > 128)
            {
                return Err(WorkflowResumeError::CheckpointShape(format!(
                    "checkpoint step {index} contains too many execution IDs"
                )));
            }
        }
        let bytes = serde_json::to_vec(self)
            .map_err(|error| WorkflowResumeError::CheckpointShape(error.to_string()))?;
        if bytes.len() > MAX_WORKFLOW_CHECKPOINT_BYTES {
            return Err(WorkflowResumeError::CheckpointTooLarge);
        }
        Ok(())
    }
}

fn workflow_definition_hash(
    workflow: &WorkflowDefinition,
) -> Result<String, WorkflowValidationError> {
    let canonical = workflow.to_canonical_json()?;
    let digest = Sha256::digest(canonical.as_bytes());
    Ok(digest.iter().map(|byte| format!("{byte:02x}")).collect())
}

fn workflow_target(action: &BatchStep) -> Option<&str> {
    match action {
        BatchStep::Click { target }
        | BatchStep::Check { target }
        | BatchStep::Uncheck { target }
        | BatchStep::Select { target, .. }
        | BatchStep::Clear { target } => Some(target),
        BatchStep::Type { target, .. } => target.as_deref(),
        _ => None,
    }
}

fn resolve_workflow_intent(
    intent: &WorkflowIntentStep,
    inputs: &BTreeMap<String, Value>,
    path: &str,
) -> Result<WorkflowIntentStep, WorkflowValidationError> {
    let resolve = |field: &str, value: &str, maximum: usize| {
        resolve_input_template(value, inputs, &format!("{path}.{field}"), maximum)
    };
    let resolve_optional = |field: &str, value: &Option<String>, maximum: usize| {
        value
            .as_deref()
            .map(|value| resolve(field, value, maximum))
            .transpose()
    };
    let mut resolved = intent.clone();
    resolved.purpose = resolve_optional("purpose", &intent.purpose, MAX_INTENT_PURPOSE_BYTES)?;
    resolved.intent = resolve_optional("intent", &intent.intent, MAX_INTENT_PURPOSE_BYTES * 2)?;
    resolved.value = resolve_optional("value", &intent.value, 4_096)?;
    resolved.scope.region_id = resolve_optional("scope.regionId", &intent.scope.region_id, 128)?;
    resolved.scope.form_label = resolve_optional("scope.formLabel", &intent.scope.form_label, 256)?;
    resolved.constraints.role = resolve_optional("constraints.role", &intent.constraints.role, 64)?;
    resolved.constraints.name = resolve_optional(
        "constraints.name",
        &intent.constraints.name,
        MAX_TARGET_BYTES,
    )?;
    resolved.constraints.name_contains = resolve_optional(
        "constraints.nameContains",
        &intent.constraints.name_contains,
        MAX_TARGET_BYTES,
    )?;
    resolved.constraints.exclude_text = intent
        .constraints
        .exclude_text
        .iter()
        .enumerate()
        .map(|(index, value)| {
            resolve_input_template(
                value,
                inputs,
                &format!("{path}.constraints.excludeText[{index}]"),
                MAX_TARGET_BYTES,
            )
        })
        .collect::<Result<_, _>>()?;
    resolved.validate(path)?;
    Ok(resolved)
}

fn resolve_batch_step(
    action: &BatchStep,
    inputs: &BTreeMap<String, Value>,
    path: &str,
) -> Result<BatchStep, WorkflowValidationError> {
    let resolve = |field: &str, value: &str, maximum: usize| {
        resolve_input_template(value, inputs, &format!("{path}.{field}"), maximum)
    };
    Ok(match action {
        BatchStep::Navigate { url, timeout_ms } => BatchStep::Navigate {
            url: resolve("url", url, MAX_TARGET_BYTES)?,
            timeout_ms: *timeout_ms,
        },
        BatchStep::Click { target } => BatchStep::Click {
            target: resolve("target", target, MAX_TARGET_BYTES)?,
        },
        BatchStep::Type { text, target } => BatchStep::Type {
            text: resolve("text", text, MAX_TEXT_BYTES)?,
            target: target
                .as_deref()
                .map(|value| resolve("target", value, MAX_TARGET_BYTES))
                .transpose()?,
        },
        BatchStep::Check { target } => BatchStep::Check {
            target: resolve("target", target, MAX_TARGET_BYTES)?,
        },
        BatchStep::Uncheck { target } => BatchStep::Uncheck {
            target: resolve("target", target, MAX_TARGET_BYTES)?,
        },
        BatchStep::Select { target, value } => BatchStep::Select {
            target: resolve("target", target, MAX_TARGET_BYTES)?,
            value: resolve("value", value, MAX_TEXT_BYTES)?,
        },
        BatchStep::Clear { target } => BatchStep::Clear {
            target: resolve("target", target, MAX_TARGET_BYTES)?,
        },
        BatchStep::Wait {
            condition,
            timeout_ms,
        } => BatchStep::Wait {
            condition: resolve("condition", condition, MAX_WAIT_CONDITION_BYTES)?,
            timeout_ms: *timeout_ms,
        },
        BatchStep::Scroll { dx, dy } => BatchStep::Scroll { dx: *dx, dy: *dy },
        BatchStep::Observe {
            include_dom,
            include_screenshot,
            include_form_values,
        } => BatchStep::Observe {
            include_dom: *include_dom,
            include_screenshot: *include_screenshot,
            include_form_values: *include_form_values,
        },
        BatchStep::Screenshot => BatchStep::Screenshot,
        BatchStep::Evaluate { expression } => BatchStep::Evaluate {
            expression: resolve("expression", expression, MAX_TEXT_BYTES)?,
        },
        BatchStep::AcceptDialog => BatchStep::AcceptDialog,
        BatchStep::DismissDialog => BatchStep::DismissDialog,
    })
}

fn resolve_input_template(
    value: &str,
    inputs: &BTreeMap<String, Value>,
    path: &str,
    maximum: usize,
) -> Result<String, WorkflowValidationError> {
    let marker = "${inputs.";
    let mut resolved = String::with_capacity(value.len());
    let mut remainder = value;
    while let Some(start) = remainder.find(marker) {
        resolved.push_str(&remainder[..start]);
        let placeholder = &remainder[start..];
        let end = placeholder.find('}').ok_or_else(|| {
            WorkflowValidationError::new(path, "input placeholder is missing a closing brace")
        })?;
        let name = &placeholder[marker.len()..end];
        if name.is_empty() || name.contains(['{', '}', '$']) {
            return Err(WorkflowValidationError::new(
                path,
                "input placeholder name is invalid",
            ));
        }
        let input = inputs.get(name).ok_or_else(|| {
            WorkflowValidationError::new(path, format!("input {name:?} is missing or not declared"))
        })?;
        let text = match input {
            Value::String(text) => text.clone(),
            Value::Bool(value) => value.to_string(),
            Value::Number(value) => value.to_string(),
            _ => {
                return Err(WorkflowValidationError::new(
                    path,
                    format!("input {name:?} cannot be inserted into an action string"),
                ));
            }
        };
        resolved.push_str(&text);
        remainder = &placeholder[end + 1..];
    }
    resolved.push_str(remainder);
    validate_bytes(path, &resolved, 0, maximum)?;
    Ok(resolved)
}

async fn extract_workflow_outputs(
    session: &super::BrowserSession,
    workflow: &WorkflowDefinition,
) -> BrowserResult<BTreeMap<String, WorkflowOutput>> {
    let page = if workflow
        .outputs
        .values()
        .any(|output| !matches!(output.source, WorkflowOutputSource::VisibleText))
    {
        Some(session.page_info().await?)
    } else {
        None
    };
    let visible_text = if workflow
        .outputs
        .values()
        .any(|output| matches!(output.source, WorkflowOutputSource::VisibleText))
    {
        Some(session.text().await?)
    } else {
        None
    };
    let mut outputs = BTreeMap::new();
    let mut extracted_bytes = 0usize;
    let revision = current_revision(session);
    for (name, declaration) in &workflow.outputs {
        let text = match declaration.source {
            WorkflowOutputSource::PageUrl => page.as_ref().map(|page| page.url.as_str()),
            WorkflowOutputSource::PageTitle => page.as_ref().map(|page| page.title.as_str()),
            WorkflowOutputSource::VisibleText => visible_text.as_deref(),
        }
        .ok_or_else(|| format!("output {name:?} has no extraction source"))?;
        extracted_bytes = extracted_bytes.saturating_add(text.len());
        if extracted_bytes > workflow.budgets.max_extracted_bytes {
            return Err(format!(
                "outputs exceed maxExtractedBytes {}",
                workflow.budgets.max_extracted_bytes
            )
            .into());
        }
        let value = typed_output_value(name, declaration.value_type, text)?;
        let redacted = declaration.sensitive;
        outputs.insert(
            name.clone(),
            WorkflowOutput {
                value_type: declaration.value_type,
                value: if redacted { Value::Null } else { value },
                redacted,
                evidence: WorkflowOutputEvidence {
                    source: declaration.source,
                    revision,
                },
            },
        );
    }
    Ok(outputs)
}

fn typed_output_value(
    name: &str,
    value_type: WorkflowValueType,
    text: &str,
) -> BrowserResult<Value> {
    let trimmed = text.trim();
    match value_type {
        WorkflowValueType::String => Ok(Value::String(text.to_string())),
        WorkflowValueType::Url => {
            if Url::parse(trimmed).is_err() {
                return Err(format!("output {name:?} is not a valid URL").into());
            }
            Ok(Value::String(text.to_string()))
        }
        WorkflowValueType::Integer => trimmed
            .parse::<i64>()
            .map(Value::from)
            .map_err(|_| format!("output {name:?} cannot be parsed as an integer").into()),
        WorkflowValueType::Number => {
            let number = trimmed
                .parse::<f64>()
                .map_err(|_| format!("output {name:?} cannot be parsed as a number"))?;
            serde_json::Number::from_f64(number)
                .map(Value::Number)
                .ok_or_else(|| format!("output {name:?} is not a finite number").into())
        }
        WorkflowValueType::Boolean => match trimmed {
            "true" => Ok(Value::Bool(true)),
            "false" => Ok(Value::Bool(false)),
            _ => Err(format!("output {name:?} cannot be parsed as a boolean").into()),
        },
    }
}

fn current_revision(session: &super::BrowserSession) -> u64 {
    session
        .page_revision
        .load(std::sync::atomic::Ordering::Relaxed)
}

fn workflow_budget_expired(started: Instant, budget: Duration) -> bool {
    started.elapsed() >= budget
}

fn workflow_budget_remaining(started: Instant, budget: Duration) -> Duration {
    budget.saturating_sub(started.elapsed())
}

fn can_retry_before_dispatch(
    transaction: WorkflowTransactionClass,
    dispatch_observed: bool,
    attempt_number: u32,
    max_retries: u32,
) -> bool {
    !dispatch_observed && attempt_number <= max_retries && transaction.permits_pre_dispatch_retry()
}

fn skip_remaining(records: &mut [WorkflowStepRecord], start: usize) {
    for record in records.iter_mut().skip(start) {
        if record.state == WorkflowStepState::Pending {
            let _ = record.transition(WorkflowStepState::Skipped);
        }
    }
}

fn commit_workflow_effect_marker(record: &mut WorkflowStepRecord, revision: u64) {
    if record.state == WorkflowStepState::Ready {
        let _ = record.transition(WorkflowStepState::Preflight);
    }
    if record.state == WorkflowStepState::Preflight {
        let _ = record.transition(WorkflowStepState::EffectObserved);
    }
    record.effect_observed = true;
    record.postcondition_verified = true;
    record.current_revision = Some(revision);
    let _ = record.transition(WorkflowStepState::Verified);
    let _ = record.transition(WorkflowStepState::OutputsExtracted);
    let _ = record.transition(WorkflowStepState::Committed);
}

fn state_name(state: WorkflowStepState) -> &'static str {
    match state {
        WorkflowStepState::Pending => "pending",
        WorkflowStepState::Ready => "ready",
        WorkflowStepState::Preflight => "preflight",
        WorkflowStepState::Resolving => "resolving",
        WorkflowStepState::NotDispatched => "not_dispatched",
        WorkflowStepState::Dispatched => "dispatched",
        WorkflowStepState::EffectObserved => "effect_observed",
        WorkflowStepState::Verified => "verified",
        WorkflowStepState::OutputsExtracted => "outputs_extracted",
        WorkflowStepState::Committed => "committed",
        WorkflowStepState::FailedBeforeDispatch => "failed_before_dispatch",
        WorkflowStepState::FailedAfterDispatch => "failed_after_dispatch",
        WorkflowStepState::Indeterminate => "indeterminate",
        WorkflowStepState::Skipped => "skipped",
    }
}

fn bound_workflow_text(value: &str, max_bytes: usize) -> String {
    if value.len() <= max_bytes {
        return value.to_string();
    }
    let mut end = max_bytes.saturating_sub(13);
    while end > 0 && !value.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}\n[truncated]", &value[..end])
}

/// A declared output captured after terminal verification.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowOutputDeclaration {
    #[serde(alias = "type")]
    pub value_type: WorkflowValueType,
    pub source: WorkflowOutputSource,
    #[serde(default)]
    pub required: bool,
    #[serde(default)]
    pub sensitive: bool,
}

impl WorkflowOutputDeclaration {
    fn validate(&self, path: &str) -> Result<(), WorkflowValidationError> {
        let compatible = match self.source {
            WorkflowOutputSource::PageUrl => {
                matches!(
                    self.value_type,
                    WorkflowValueType::String | WorkflowValueType::Url
                )
            }
            WorkflowOutputSource::PageTitle | WorkflowOutputSource::VisibleText => true,
        };
        if !compatible {
            return Err(WorkflowValidationError::new(
                format!("{path}.valueType"),
                format!(
                    "{} output source requires a compatible value type",
                    self.source
                ),
            ));
        }
        Ok(())
    }
}

/// Bounded, non-JavaScript sources for workflow outputs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkflowOutputSource {
    PageUrl,
    PageTitle,
    VisibleText,
}

impl fmt::Display for WorkflowOutputSource {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::PageUrl => "page_url",
            Self::PageTitle => "page_title",
            Self::VisibleText => "visible_text",
        })
    }
}

/// A typed output value with its declared extraction source.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowOutput {
    pub value_type: WorkflowValueType,
    pub value: Value,
    #[serde(default, skip_serializing_if = "is_false")]
    pub redacted: bool,
    pub evidence: WorkflowOutputEvidence,
}

fn is_false(value: &bool) -> bool {
    !*value
}

/// Bounded provenance for a typed workflow output.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowOutputEvidence {
    pub source: WorkflowOutputSource,
    pub revision: u64,
}

/// A path-aware validation failure.
#[derive(Debug, Clone, Serialize)]
pub struct WorkflowValidationError {
    pub path: String,
    pub reason: String,
}

impl WorkflowValidationError {
    fn new(path: impl Into<String>, reason: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            reason: reason.into(),
        }
    }
}

impl fmt::Display for WorkflowValidationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}: {}", self.path, self.reason)
    }
}

impl std::error::Error for WorkflowValidationError {}

fn require_object_fields(value: &Value, fields: &[&str]) -> Result<(), WorkflowValidationError> {
    let object = value.as_object().ok_or_else(|| {
        WorkflowValidationError::new("$", "workflow definition must be a JSON object")
    })?;
    for field in fields {
        if !object.contains_key(*field) {
            return Err(WorkflowValidationError::new(
                format!("$.{field}"),
                "required field is missing",
            ));
        }
    }
    Ok(())
}

fn reject_unknown_fields(
    value: &Value,
    path: &str,
    allowed: &[&str],
) -> Result<(), WorkflowValidationError> {
    let object = value
        .as_object()
        .ok_or_else(|| WorkflowValidationError::new(path, "expected a JSON object"))?;
    if let Some(field) = object
        .keys()
        .find(|field| !allowed.contains(&field.as_str()))
    {
        return Err(WorkflowValidationError::new(
            format!("{path}.{field}"),
            "unknown workflow field",
        ));
    }
    Ok(())
}

fn reject_predicate_fields(value: &Value, path: &str) -> Result<(), WorkflowValidationError> {
    let object = value
        .as_object()
        .ok_or_else(|| WorkflowValidationError::new(path, "expected a predicate object"))?;
    let allowed = [
        "urlEquals",
        "titleContains",
        "visible",
        "textContains",
        "popupOpened",
        "dialogOpen",
        "downloadStarted",
        "revisionEquals",
        "all",
        "any",
        "not",
    ];
    if object.len() != 1 {
        return Err(WorkflowValidationError::new(
            path,
            "predicate must contain exactly one recognized field",
        ));
    }
    let Some((field, nested)) = object.iter().next() else {
        return Err(WorkflowValidationError::new(
            path,
            "predicate must not be empty",
        ));
    };
    if !allowed.contains(&field.as_str()) {
        return Err(WorkflowValidationError::new(
            format!("{path}.{field}"),
            "unknown predicate field",
        ));
    }
    match field.as_str() {
        "all" | "any" => {
            let predicates = nested.as_array().ok_or_else(|| {
                WorkflowValidationError::new(format!("{path}.{field}"), "must be an array")
            })?;
            for (index, predicate) in predicates.iter().enumerate() {
                reject_predicate_fields(predicate, &format!("{path}.{field}[{index}]"))?;
            }
        }
        "not" => reject_predicate_fields(nested, &format!("{path}.not"))?,
        _ => {}
    }
    Ok(())
}

fn validate_name(path: &str, value: &str) -> Result<(), WorkflowValidationError> {
    validate_bytes(path, value, 1, MAX_NAME_BYTES)?;
    if value.trim() != value {
        return Err(WorkflowValidationError::new(
            path,
            "must not have leading or trailing whitespace",
        ));
    }
    Ok(())
}

fn validate_map_key(scope: &str, key: &str) -> Result<(), WorkflowValidationError> {
    validate_name(&format!("{scope}.{key}"), key)
}

fn validate_bytes(
    path: &str,
    value: &str,
    minimum: usize,
    maximum: usize,
) -> Result<(), WorkflowValidationError> {
    if value.len() < minimum || value.len() > maximum {
        return Err(WorkflowValidationError::new(
            path,
            format!("must be {minimum}..={maximum} UTF-8 bytes"),
        ));
    }
    Ok(())
}

fn validate_target(path: &str, target: &str) -> Result<(), WorkflowValidationError> {
    validate_bytes(path, target, 1, MAX_TARGET_BYTES)
}

fn validate_predicate(
    predicate: &VerificationPredicate,
    path: &str,
) -> Result<(), WorkflowValidationError> {
    predicate
        .validate(0)
        .map_err(|error| WorkflowValidationError::new(path, error.to_string()))
}

fn validate_batch_step(action: &BatchStep, path: &str) -> Result<(), WorkflowValidationError> {
    match action {
        BatchStep::Navigate { url, timeout_ms } => {
            if Url::parse(url).is_err() {
                return Err(WorkflowValidationError::new(
                    format!("{path}.url"),
                    "must be an absolute URL",
                ));
            }
            if *timeout_ms == 0 || *timeout_ms > MAX_DURATION_MS {
                return Err(WorkflowValidationError::new(
                    format!("{path}.timeoutMs"),
                    format!("must be 1..={MAX_DURATION_MS}"),
                ));
            }
        }
        BatchStep::Click { target }
        | BatchStep::Check { target }
        | BatchStep::Uncheck { target }
        | BatchStep::Clear { target } => validate_target(&format!("{path}.target"), target)?,
        BatchStep::Type { text, target } => {
            validate_bytes(&format!("{path}.text"), text, 0, MAX_TEXT_BYTES)?;
            if let Some(target) = target {
                validate_target(&format!("{path}.target"), target)?;
            }
        }
        BatchStep::Select { target, value } => {
            validate_target(&format!("{path}.target"), target)?;
            validate_bytes(&format!("{path}.value"), value, 1, MAX_TEXT_BYTES)?;
        }
        BatchStep::Scroll { dx, dy } => {
            if !dx.is_finite()
                || !dy.is_finite()
                || dx.abs() > 1_000_000.0
                || dy.abs() > 1_000_000.0
            {
                return Err(WorkflowValidationError::new(
                    path,
                    "scroll deltas must be finite and bounded",
                ));
            }
        }
        BatchStep::Wait {
            condition,
            timeout_ms,
        } => {
            validate_bytes(
                &format!("{path}.condition"),
                condition,
                1,
                MAX_WAIT_CONDITION_BYTES,
            )?;
            if *timeout_ms == 0 || *timeout_ms > MAX_DURATION_MS {
                return Err(WorkflowValidationError::new(
                    format!("{path}.timeoutMs"),
                    format!("must be 1..={MAX_DURATION_MS}"),
                ));
            }
        }
        BatchStep::Observe { .. }
        | BatchStep::Screenshot
        | BatchStep::AcceptDialog
        | BatchStep::DismissDialog => {}
        BatchStep::Evaluate { .. } => {
            return Err(WorkflowValidationError::new(
                path,
                "evaluate is not permitted in a declarative workflow",
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::browser::session::{
        FingerprintInvalidation, SemanticIntentCandidate, SemanticIntentPurpose,
        SemanticRegionKind, SemanticTargetFingerprint,
    };
    use serde_json::json;

    fn definition() -> WorkflowDefinition {
        WorkflowDefinition {
            schema_version: WORKFLOW_SCHEMA_VERSION,
            name: "example".into(),
            workflow_version: "1.0.0".into(),
            description: None,
            inputs: BTreeMap::from([(
                "url".into(),
                WorkflowInput {
                    value_type: WorkflowValueType::Url,
                    required: true,
                    max_length: Some(2_048),
                    sensitive: None,
                },
            )]),
            budgets: WorkflowBudgets {
                max_steps: 2,
                max_duration_ms: 30_000,
                max_retries: 1,
                max_extracted_bytes: 4_096,
            },
            preconditions: vec![],
            steps: vec![WorkflowStep {
                id: "open".into(),
                action: BatchStep::Navigate {
                    url: "https://example.com".into(),
                    timeout_ms: 20_000,
                },
                intent: None,
                when: None,
                expect: Some(VerificationPredicate::TitleContains {
                    value: "Example".into(),
                }),
                before_retry: None,
                transaction: WorkflowTransactionClass::ReadOnly,
                idempotency_key: None,
                max_retries: 0,
                repeat: 1,
            }],
            terminal_condition: VerificationPredicate::UrlEquals {
                value: "https://example.com/".into(),
            },
            outputs: BTreeMap::new(),
        }
    }

    #[test]
    fn canonical_json_is_stable_and_uses_camel_case() {
        let workflow = definition();
        let first = workflow.to_canonical_json().unwrap();
        let second = workflow.to_canonical_json().unwrap();
        assert_eq!(first, second);
        assert!(first.contains("\"schemaVersion\":1"));
        assert!(first.contains("\"timeoutMs\":20000"));
        assert!(first.contains("\"workflowVersion\":\"1.0.0\""));
    }

    #[test]
    fn before_retry_marker_is_canonical_and_validated() {
        let mut workflow = definition();
        workflow.steps[0].before_retry = Some(VerificationPredicate::TitleContains {
            value: "Already saved".into(),
        });
        let json = workflow.to_canonical_json().unwrap();
        assert!(json.contains("\"beforeRetry\":{"));
        let parsed = WorkflowDefinition::from_json(&json).unwrap();
        assert!(parsed.steps[0].before_retry.is_some());
    }

    #[test]
    fn type_alias_is_accepted_but_canonical_json_uses_value_type() {
        let mut value = serde_json::to_value(definition()).unwrap();
        let input = value["inputs"]["url"].as_object_mut().unwrap();
        let value_type = input.remove("valueType").unwrap();
        input.insert("type".into(), value_type);

        let parsed = WorkflowDefinition::from_value(value).unwrap();
        let canonical = parsed.to_canonical_json().unwrap();
        assert!(canonical.contains("\"valueType\":\"url\""));
        assert!(!canonical.contains("\"type\":\"url\""));
    }

    #[test]
    fn effect_marker_commit_follows_evidence_states_without_dispatch() {
        let mut record = WorkflowStepRecord::new("save");
        record.transition(WorkflowStepState::Ready).unwrap();
        commit_workflow_effect_marker(&mut record, 9);
        assert_eq!(record.state, WorkflowStepState::Committed);
        assert!(!record.dispatch_acknowledged);
        assert!(record.effect_observed);
        assert!(record.postcondition_verified);
        assert_eq!(record.current_revision, Some(9));
    }

    #[test]
    fn from_json_reports_missing_top_level_path() {
        let mut value = serde_json::to_value(definition()).unwrap();
        value.as_object_mut().unwrap().remove("steps");
        let error = WorkflowDefinition::from_value(value).unwrap_err();
        assert_eq!(error.path, "$.steps");
    }

    #[test]
    fn unknown_definition_fields_fail_before_deserialization() {
        let mut value = serde_json::to_value(definition()).unwrap();
        value
            .as_object_mut()
            .unwrap()
            .insert("futureField".into(), json!(true));
        let error = WorkflowDefinition::from_value(value).unwrap_err();
        assert_eq!(error.path, "$.futureField");
    }

    #[test]
    fn semantic_intent_steps_round_trip_with_bounded_defaults() {
        let mut value = serde_json::to_value(definition()).unwrap();
        value["steps"][0] = json!({
            "id": "continue",
            "intent": {
                "action": "click",
                "purpose": "continueCheckout",
                "scope": {"regionKind": "checkoutSummary"},
                "resolutionPolicy": "requireUniqueHighConfidence"
            },
            "transaction": "idempotent"
        });
        let workflow = WorkflowDefinition::from_value(value).unwrap();
        let intent = workflow.steps[0].intent.as_ref().unwrap();
        assert_eq!(intent.action, SemanticIntentAction::Click);
        assert_eq!(intent.purpose.as_deref(), Some("continueCheckout"));
        assert_eq!(
            intent
                .execution_request("steps[0].intent")
                .unwrap()
                .request
                .intent,
            "continue checkout"
        );
        let canonical = workflow.to_canonical_json().unwrap();
        assert!(canonical.contains("\"resolutionPolicy\":\"requireUniqueHighConfidence\""));
        assert!(!canonical.contains("\"action\":\"navigate\""));
    }

    #[test]
    fn semantic_intent_steps_reject_ambiguous_shape_and_missing_values() {
        let mut value = serde_json::to_value(definition()).unwrap();
        value["steps"][0] = json!({
            "id": "bad",
            "intent": {
                "action": "type",
                "purpose": "enterSearch",
                "intent": "enter search",
                "resolutionPolicy": "requireUniqueHighConfidence"
            },
            "transaction": "idempotent"
        });
        let error = WorkflowDefinition::from_value(value).unwrap_err();
        assert_eq!(error.path, "steps[0].intent.purpose");

        let mut value = serde_json::to_value(definition()).unwrap();
        value["steps"][0] = json!({
            "id": "bad",
            "intent": {
                "action": "type",
                "purpose": "enterSearch",
                "resolutionPolicy": "requireUniqueHighConfidence"
            },
            "transaction": "idempotent"
        });
        let error = WorkflowDefinition::from_value(value).unwrap_err();
        assert_eq!(error.path, "steps[0].intent.value");
    }

    #[test]
    fn unknown_predicate_fields_fail_with_their_json_path() {
        let mut value = serde_json::to_value(definition()).unwrap();
        value["terminalCondition"] = json!({"titleContains": "Example", "future": true});
        let error = WorkflowDefinition::from_value(value).unwrap_err();
        assert_eq!(error.path, "terminalCondition");
    }

    #[test]
    fn duplicate_step_ids_are_rejected() {
        let mut workflow = definition();
        workflow.steps.push(WorkflowStep {
            id: "open".into(),
            action: BatchStep::Screenshot,
            intent: None,
            when: None,
            expect: None,
            before_retry: None,
            transaction: WorkflowTransactionClass::ReadOnly,
            idempotency_key: None,
            max_retries: 0,
            repeat: 1,
        });
        let error = workflow.validate().unwrap_err();
        assert_eq!(error.path, "steps[1].id");
    }

    #[test]
    fn invalid_budget_reports_exact_path() {
        let mut workflow = definition();
        workflow.budgets.max_steps = 0;
        let error = workflow.validate().unwrap_err();
        assert_eq!(error.path, "budgets.maxSteps");
    }

    #[test]
    fn conditional_idempotency_requires_a_key() {
        let mut workflow = definition();
        workflow.steps[0].transaction = WorkflowTransactionClass::ConditionallyIdempotent;
        let error = workflow.validate().unwrap_err();
        assert_eq!(error.path, "steps[0].idempotencyKey");
    }

    #[test]
    fn duplicate_idempotency_keys_are_rejected() {
        let mut workflow = definition();
        workflow.steps[0].transaction = WorkflowTransactionClass::ConditionallyIdempotent;
        workflow.steps[0].idempotency_key = Some("save-once".into());
        workflow.steps.push(WorkflowStep {
            id: "second".into(),
            action: BatchStep::Screenshot,
            intent: None,
            when: None,
            expect: None,
            before_retry: None,
            transaction: WorkflowTransactionClass::ConditionallyIdempotent,
            idempotency_key: Some("save-once".into()),
            max_retries: 0,
            repeat: 1,
        });
        workflow.budgets.max_steps = 2;
        let error = workflow.validate().unwrap_err();
        assert_eq!(error.path, "steps[1].idempotencyKey");
    }

    #[test]
    fn unknown_steps_cannot_request_automatic_retries() {
        let mut workflow = definition();
        workflow.steps[0].transaction = WorkflowTransactionClass::Unknown;
        workflow.budgets.max_retries = 1;
        workflow.steps[0].max_retries = 1;
        let error = workflow.validate().unwrap_err();
        assert_eq!(error.path, "steps[0].maxRetries");
    }

    #[test]
    fn bounded_repetition_requires_retry_safe_class() {
        let mut workflow = definition();
        workflow.budgets.max_steps = 2;
        workflow.steps[0].repeat = 2;
        workflow.steps[0].transaction = WorkflowTransactionClass::Unknown;
        let error = workflow.validate().unwrap_err();
        assert_eq!(error.path, "steps[0].repeat");
    }

    #[test]
    fn retry_policy_never_replays_after_dispatch() {
        assert!(can_retry_before_dispatch(
            WorkflowTransactionClass::Idempotent,
            false,
            1,
            1
        ));
        assert!(!can_retry_before_dispatch(
            WorkflowTransactionClass::NonIdempotent,
            false,
            1,
            1
        ));
        assert!(!can_retry_before_dispatch(
            WorkflowTransactionClass::Idempotent,
            true,
            1,
            1
        ));
    }

    #[test]
    fn workflow_checkpoint_is_deterministic_and_redacted() {
        let checkpoint = WorkflowCheckpoint {
            schema_version: WORKFLOW_CHECKPOINT_SCHEMA_VERSION,
            run_id: "run_test".into(),
            workflow_name: "example".into(),
            workflow_version: "1.0.0".into(),
            definition_hash: "a".repeat(64),
            status: WorkflowRunStatus::Failed,
            next_step_index: 1,
            steps: vec![
                WorkflowCheckpointStep {
                    id: "open".into(),
                    state: WorkflowStepState::Committed,
                    attempts: 1,
                    history: Vec::new(),
                    execution_ids: Vec::new(),
                    dispatch_acknowledged: false,
                    effect_observed: false,
                    postcondition_verified: false,
                    retry_safe: false,
                    previous_revision: None,
                    current_revision: None,
                    branch_decision: None,
                    intent_evidence: None,
                },
                WorkflowCheckpointStep {
                    id: "save".into(),
                    state: WorkflowStepState::FailedBeforeDispatch,
                    attempts: 1,
                    history: Vec::new(),
                    execution_ids: Vec::new(),
                    dispatch_acknowledged: false,
                    effect_observed: false,
                    postcondition_verified: false,
                    retry_safe: false,
                    previous_revision: None,
                    current_revision: None,
                    branch_decision: None,
                    intent_evidence: None,
                },
            ],
            page: WorkflowCheckpointPage {
                target_id: "target".into(),
                frame_id: "frame".into(),
                url: "https://example.com".into(),
                title: "Example".into(),
                revision: 3,
            },
        };
        let first = checkpoint.to_canonical_json().unwrap();
        let second = checkpoint.to_canonical_json().unwrap();
        assert_eq!(first, second);
        assert!(!first.contains("password"));
        assert_eq!(
            crate::BrowserSession::parse_workflow_checkpoint(&first)
                .unwrap()
                .next_step_index,
            1
        );
    }

    #[test]
    fn minimal_workflow_fixture_round_trips() {
        let workflow = WorkflowDefinition::from_json(include_str!(
            "../../../tests/fixtures/workflow-minimal.json"
        ))
        .unwrap();
        assert_eq!(workflow.name, "open-example");
        assert!(workflow.preconditions.is_empty());
        assert!(workflow.to_canonical_json().is_ok());
    }

    #[test]
    fn evaluate_is_rejected_before_execution() {
        let mut workflow = definition();
        workflow.steps[0].action = BatchStep::Evaluate {
            expression: "document.title".into(),
        };
        let error = workflow.validate().unwrap_err();
        assert!(error.reason.contains("not permitted"));
    }

    #[test]
    fn inputs_are_type_checked_and_bounded() {
        let workflow = definition();
        let values = BTreeMap::from([("url".into(), json!("https://example.com"))]);
        workflow.validate_inputs(&values).unwrap();

        let bad_values = BTreeMap::from([("url".into(), json!("not a url"))]);
        let error = workflow.validate_inputs(&bad_values).unwrap_err();
        assert_eq!(error.path, "inputs.url");
    }

    #[test]
    fn resolve_actions_substitutes_bounded_inputs() {
        let mut workflow = definition();
        workflow.steps[0].action = BatchStep::Navigate {
            url: "${inputs.url}".into(),
            timeout_ms: 20_000,
        };
        let values = BTreeMap::from([("url".into(), json!("https://docs.example.com"))]);
        let steps = workflow.resolve_actions(&values).unwrap();
        let BatchStep::Navigate { url, .. } = &steps[0].action else {
            panic!("expected navigate action");
        };
        assert_eq!(url, "https://docs.example.com");
    }

    #[test]
    fn resolve_actions_rejects_unknown_placeholders_before_dispatch() {
        let mut workflow = definition();
        workflow.steps[0].action = BatchStep::Click {
            target: "name=${inputs.missing}".into(),
        };
        let values = BTreeMap::from([("url".into(), json!("https://example.com"))]);
        let error = workflow.resolve_actions(&values).unwrap_err();
        assert_eq!(error.path, "steps[0].action.target");
    }

    #[test]
    fn typed_output_values_require_strict_scalar_conversions() {
        assert_eq!(
            typed_output_value("count", WorkflowValueType::Integer, " 42 ").unwrap(),
            json!(42)
        );
        assert_eq!(
            typed_output_value("ready", WorkflowValueType::Boolean, "false").unwrap(),
            json!(false)
        );
        assert!(typed_output_value("count", WorkflowValueType::Integer, "4.2").is_err());
        assert!(typed_output_value("ready", WorkflowValueType::Boolean, "yes").is_err());
    }

    #[test]
    fn sensitive_output_serialization_contains_no_literal_value() {
        let output = WorkflowOutput {
            value_type: WorkflowValueType::String,
            value: Value::Null,
            redacted: true,
            evidence: WorkflowOutputEvidence {
                source: WorkflowOutputSource::VisibleText,
                revision: 4,
            },
        };
        let serialized = serde_json::to_string(&output).unwrap();
        assert!(serialized.contains("\"redacted\":true"));
        assert!(!serialized.contains("secret"));
    }

    #[test]
    fn recorder_keeps_semantic_targets_and_redacts_typed_values() {
        let mut recorder = WorkflowRecorder::new("checkout", "1.0.0");
        recorder
            .record_click("continue", "button", "Continue", None)
            .unwrap();
        recorder
            .record_type_input("email", "textbox", "Email", "email")
            .unwrap();
        let draft = recorder.draft();
        let BatchStep::Click { target } = &draft.steps[0].action else {
            panic!("expected semantic click draft");
        };
        assert_eq!(target, "role=button;name=Continue");
        let serialized = serde_json::to_string(draft).unwrap();
        assert!(!serialized.contains("password-value"));
        assert!(serialized.contains("${inputs.email}"));
        assert!(draft.steps[1].review_required);
    }

    #[test]
    fn recorder_captures_semantic_evidence_without_replay_handles_or_query_values() {
        let request = SemanticIntentRequest {
            schema_version: INTENT_RESOLUTION_SCHEMA_VERSION,
            intent: "submit order".into(),
            action: SemanticIntentAction::Click,
            scope: IntentScope::default(),
            constraints: IntentConstraints::default(),
            resolution_policy: SemanticResolutionPolicy::RequireUniqueHighConfidence,
            expected_revision: None,
        };
        let route = SemanticRouteIdentity {
            target_id: "target-7".into(),
            frame_id: "frame-2".into(),
            url: "https://shop.example/orders?token=secret#confirmation".into(),
        };
        let result = SemanticIntentResult {
            schema_version: INTENT_RESOLUTION_SCHEMA_VERSION,
            intent: request.intent.clone(),
            action: request.action,
            normalized_intent: "submit order".into(),
            resolution: SemanticResolution::UniqueHighConfidence,
            policy_decision: IntentPolicyDecision::Allowed,
            route: Some(route.clone()),
            revision: Some(7),
            candidates: vec![SemanticIntentCandidate {
                id: "candidate-1".into(),
                reference: "r7:backend-secret".into(),
                role: "button".into(),
                name: "Submit order".into(),
                input_type: None,
                region_id: Some("checkout-region".into()),
                region_kind: Some(SemanticRegionKind::CheckoutSummary),
                confidence: IntentConfidence::High,
                evidence: Vec::new(),
                fingerprint: Some(SemanticTargetFingerprint {
                    revision: 7,
                    route: route.clone(),
                    role: "button".into(),
                    name: "Submit order".into(),
                    input_type: None,
                    region_id: Some("checkout-region".into()),
                    region_kind: Some(SemanticRegionKind::CheckoutSummary),
                    purpose: SemanticIntentPurpose::Submit,
                    invalidated_by: vec![FingerprintInvalidation::Revision],
                }),
            }],
            excluded_candidates: Vec::new(),
            excluded_count: 0,
            selected_candidate: Some("candidate-1".into()),
            suggested_constraints: Vec::new(),
            reason: None,
        };
        let mut recorder = WorkflowRecorder::new("checkout", "1.0.0");
        recorder
            .record_semantic_intent(
                "submit",
                &request,
                &result,
                None::<String>,
                WorkflowTransactionClass::NonIdempotent,
                Some(VerificationPredicate::TextContains {
                    value: "Order submitted".into(),
                }),
            )
            .unwrap();

        let draft = recorder.draft();
        let step = &draft.steps[0];
        assert_eq!(
            step.target.as_ref().unwrap().region_kind,
            Some(SemanticRegionKind::CheckoutSummary)
        );
        assert_eq!(step.semantic.as_ref().unwrap().revision, Some(7));
        assert!(step.semantic.as_ref().unwrap().target_fingerprint.is_some());
        let serialized = serde_json::to_string(draft).unwrap();
        assert!(!serialized.contains("backend-secret"));
        assert!(!serialized.contains("token=secret"));
        assert!(!serialized.contains("target-7"));
        assert!(!serialized.contains("frame-2"));

        let definition = recorder
            .into_definition(
                BTreeMap::new(),
                WorkflowBudgets {
                    max_steps: 1,
                    max_duration_ms: 30_000,
                    max_retries: 0,
                    max_extracted_bytes: 4_096,
                },
                VerificationPredicate::TextContains {
                    value: "Order submitted".into(),
                },
                BTreeMap::new(),
            )
            .unwrap();
        assert!(definition.steps[0].intent.is_some());
        assert!(definition.steps[0].expect.is_some());
    }

    #[test]
    fn recorder_keeps_ambiguous_semantic_results_unselected() {
        let request = SemanticIntentRequest {
            schema_version: INTENT_RESOLUTION_SCHEMA_VERSION,
            intent: "open settings".into(),
            action: SemanticIntentAction::Click,
            scope: IntentScope::default(),
            constraints: IntentConstraints::default(),
            resolution_policy: SemanticResolutionPolicy::RequireUniqueHighConfidence,
            expected_revision: None,
        };
        let candidate = |id: &str| SemanticIntentCandidate {
            id: id.into(),
            reference: format!("{id}:ref"),
            role: "button".into(),
            name: "Settings".into(),
            input_type: None,
            region_id: None,
            region_kind: None,
            confidence: IntentConfidence::High,
            evidence: Vec::new(),
            fingerprint: None,
        };
        let result = SemanticIntentResult {
            schema_version: INTENT_RESOLUTION_SCHEMA_VERSION,
            intent: request.intent.clone(),
            action: request.action,
            normalized_intent: request.intent.clone(),
            resolution: SemanticResolution::Ambiguous,
            policy_decision: IntentPolicyDecision::Rejected,
            route: None,
            revision: Some(3),
            candidates: vec![candidate("one"), candidate("two")],
            excluded_candidates: Vec::new(),
            excluded_count: 0,
            selected_candidate: None,
            suggested_constraints: Vec::new(),
            reason: Some("two candidates remain".into()),
        };
        let mut recorder = WorkflowRecorder::new("settings", "1.0.0");
        recorder
            .record_semantic_intent(
                "open-settings",
                &request,
                &result,
                None::<String>,
                WorkflowTransactionClass::Unknown,
                None,
            )
            .unwrap();
        let step = &recorder.draft().steps[0];
        assert!(step.target.is_none());
        assert_eq!(step.confidence, WorkflowRecordingConfidence::Low);
        assert!(step.semantic.as_ref().unwrap().ambiguous);
        assert!(step.review_required);
    }

    #[test]
    fn workflow_step_flattens_batch_action() {
        let value = serde_json::to_value(&definition().steps[0]).unwrap();
        assert_eq!(value["id"], "open");
        assert_eq!(value["action"], "navigate");
        assert_eq!(value["timeoutMs"], 20_000);
    }

    #[test]
    fn state_machine_accepts_linear_commit_and_rejects_invalid_jump() {
        assert!(WorkflowStepState::Pending.can_transition_to(WorkflowStepState::Ready));
        assert!(WorkflowStepState::Resolving.can_transition_to(WorkflowStepState::Dispatched));
        assert!(!WorkflowStepState::Pending.can_transition_to(WorkflowStepState::Committed));

        let mut record = WorkflowStepRecord::new("open");
        for state in [
            WorkflowStepState::Ready,
            WorkflowStepState::Preflight,
            WorkflowStepState::Resolving,
            WorkflowStepState::Dispatched,
            WorkflowStepState::EffectObserved,
            WorkflowStepState::Verified,
            WorkflowStepState::OutputsExtracted,
            WorkflowStepState::Committed,
        ] {
            record.transition(state).unwrap();
        }
        assert_eq!(record.state, WorkflowStepState::Committed);
        assert_eq!(record.history.len(), 9);
        assert!(record.transition(WorkflowStepState::Preflight).is_err());
        let trace = WorkflowTrace::from_steps(&[record]);
        trace.validate().unwrap();
        assert_eq!(trace.events[0].sequence, 0);
        assert_eq!(
            trace.events.last().unwrap().state,
            WorkflowStepState::Committed
        );
    }

    #[test]
    fn step_record_serializes_bounded_execution_evidence() {
        let mut record = WorkflowStepRecord::new("open");
        record.execution_ids = vec!["act_7".into(), "act_8".into()];
        record.dispatch_acknowledged = true;
        record.effect_observed = true;
        record.postcondition_verified = true;
        record.retry_safe = false;
        record.previous_revision = Some(7);
        record.current_revision = Some(8);
        record.intent_evidence = Some(WorkflowIntentEvidence {
            resolution_id: "res_1".into(),
            candidate_id: "candidate_1".into(),
            revision: 8,
            resolution: crate::browser::session::SemanticResolution::Exact,
            policy_decision: crate::browser::session::IntentPolicyDecision::Allowed,
            confidence: crate::browser::session::IntentConfidence::Exact,
            fingerprint: None,
        });

        let trace = WorkflowTrace::from_steps(std::slice::from_ref(&record));
        assert_eq!(trace.intent_resolutions.len(), 1);
        let value = serde_json::to_value(record).unwrap();
        assert_eq!(value["executionIds"], serde_json::json!(["act_7", "act_8"]));
        assert_eq!(value["dispatchAcknowledged"], true);
        assert_eq!(value["effectObserved"], true);
        assert_eq!(value["postconditionVerified"], true);
        assert_eq!(value["retrySafe"], false);
        assert_eq!(value["previousRevision"], 7);
        assert_eq!(value["currentRevision"], 8);
        assert_eq!(value["intentEvidence"]["candidateId"], "candidate_1");
    }

    #[test]
    fn budget_exhaustion_is_a_typed_run_status() {
        let result = WorkflowRunResult::budget_exhausted(
            &definition(),
            "run_test".into(),
            vec![WorkflowStepRecord::new("open")],
            Some("open".into()),
            "maxSteps exhausted",
            3,
            3,
        );
        assert_eq!(result.status, WorkflowRunStatus::BudgetExhausted);
        assert_eq!(result.trace.run_id.as_deref(), Some("run_test"));
        let value = serde_json::to_value(result).unwrap();
        assert_eq!(value["status"], "budget_exhausted");
    }

    #[test]
    fn trace_schema_version_is_independent_and_validated() {
        let trace = WorkflowTrace::from_steps(&[]);
        assert_eq!(trace.schema_version, WORKFLOW_TRACE_SCHEMA_VERSION);
        trace.validate().unwrap();

        let mut unsupported = trace;
        unsupported.schema_version = WORKFLOW_TRACE_SCHEMA_VERSION + 1;
        let error = unsupported.validate().unwrap_err();
        assert_eq!(error.path, "trace.schemaVersion");
    }

    #[test]
    fn post_dispatch_failures_require_resume_reconciliation() {
        let result = WorkflowRunResult::resume_required(
            &definition(),
            "run_test".into(),
            vec![WorkflowStepRecord::new("open")],
            Some("open".into()),
            "postcondition was not proven",
            3,
            4,
        );
        assert_eq!(result.status, WorkflowRunStatus::ResumeRequired);
        assert_eq!(
            serde_json::to_value(result).unwrap()["status"],
            "resume_required"
        );
    }

    #[test]
    fn trace_replay_preserves_attempt_boundaries() {
        let mut record = WorkflowStepRecord::new("open");
        record.transition(WorkflowStepState::Ready).unwrap();
        record.transition(WorkflowStepState::Preflight).unwrap();
        record.attempts = 1;
        record.transition(WorkflowStepState::Resolving).unwrap();
        record.transition(WorkflowStepState::NotDispatched).unwrap();
        record.fail(WorkflowStepState::FailedBeforeDispatch, "before dispatch");
        record.transition(WorkflowStepState::Ready).unwrap();
        record.transition(WorkflowStepState::Preflight).unwrap();
        record.attempts = 2;
        record.transition(WorkflowStepState::Resolving).unwrap();
        record.transition(WorkflowStepState::Dispatched).unwrap();
        record
            .transition(WorkflowStepState::EffectObserved)
            .unwrap();
        record.transition(WorkflowStepState::Verified).unwrap();
        record
            .transition(WorkflowStepState::OutputsExtracted)
            .unwrap();
        record.transition(WorkflowStepState::Committed).unwrap();

        let workflow = definition();
        let trace = WorkflowTrace::from_steps(&[record]);
        let replayed = trace.replay(&workflow).unwrap();
        assert_eq!(trace.events[2].attempt, 1);
        assert_eq!(trace.events[7].attempt, 2);
        assert_eq!(replayed[0].state, WorkflowStepState::Committed);
        assert_eq!(replayed[0].attempts, 2);
    }

    #[test]
    fn trace_replay_rejects_a_prefix_that_skips_the_first_step() {
        let mut workflow = definition();
        workflow.steps.push(WorkflowStep {
            id: "save".into(),
            action: BatchStep::Screenshot,
            intent: None,
            when: None,
            expect: None,
            before_retry: None,
            transaction: WorkflowTransactionClass::ReadOnly,
            idempotency_key: None,
            max_retries: 0,
            repeat: 1,
        });
        let trace = WorkflowTrace::from_steps(&[WorkflowStepRecord::new("save")]);
        let error = trace.replay(&workflow).unwrap_err();
        assert_eq!(error.path, "trace.events[0].stepId");
    }

    #[test]
    fn conditional_steps_record_and_replay_branch_decisions() {
        let mut workflow = definition();
        let predicate = VerificationPredicate::TitleContains {
            value: "Example".into(),
        };
        workflow.steps[0].when = Some(predicate.clone());
        let mut record = WorkflowStepRecord::new("open");
        record.transition(WorkflowStepState::Ready).unwrap();
        record.branch_decision = Some(WorkflowBranchDecision {
            step_id: "open".into(),
            predicate: predicate.clone(),
            matched: false,
        });
        record.transition(WorkflowStepState::Skipped).unwrap();
        let trace = WorkflowTrace::from_steps(&[record]);
        assert!(!trace.branch_decisions[0].matched);
        let replayed = trace.replay(&workflow).unwrap();
        assert_eq!(replayed[0].state, WorkflowStepState::Skipped);
        assert_eq!(
            replayed[0].branch_decision.as_ref().unwrap().predicate,
            predicate
        );
    }
}
