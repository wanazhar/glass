//! Versioned, declarative workflow definitions.
//!
//! This module contains the data contract only. Execution, persistence, and
//! resume reconciliation build on these validated definitions in later
//! workflow phases.

use super::types::{BatchStep, VerificationPredicate};
use serde::de::Error as DeError;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
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

        let mut ids = BTreeSet::new();
        for (index, step) in self.steps.iter().enumerate() {
            let path = format!("steps[{index}]");
            step.validate(&path)?;
            if !ids.insert(step.id.as_str()) {
                return Err(WorkflowValidationError::new(
                    format!("{path}.id"),
                    format!("duplicate step ID {:?}", step.id),
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
}

/// A declared workflow input and its accepted JSON type.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowInput {
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
    pub expect: Option<VerificationPredicate>,
}

impl Serialize for WorkflowStep {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut action = serde_json::to_value(&self.action).map_err(serde::ser::Error::custom)?;
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

        let mut workflow = serde_json::Map::new();
        workflow.insert("id".into(), Value::String(self.id.clone()));
        if let Value::Object(action) = action {
            workflow.extend(action);
        }
        if let Some(expect) = &self.expect {
            workflow.insert(
                "expect".into(),
                serde_json::to_value(expect).map_err(serde::ser::Error::custom)?,
            );
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
        let expect = workflow
            .remove("expect")
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
        let action = serde_json::from_value(Value::Object(workflow)).map_err(D::Error::custom)?;
        Ok(Self { id, action, expect })
    }
}

impl WorkflowStep {
    fn validate(&self, path: &str) -> Result<(), WorkflowValidationError> {
        validate_name(&format!("{path}.id"), &self.id)?;
        validate_batch_step(&self.action, &format!("{path}.action"))?;
        if let Some(predicate) = &self.expect {
            validate_predicate(predicate, &format!("{path}.expect"))?;
        }
        Ok(())
    }
}

/// A declared output captured by a future workflow executor.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowOutputDeclaration {
    pub value_type: WorkflowValueType,
    #[serde(default)]
    pub required: bool,
}

impl WorkflowOutputDeclaration {
    fn validate(&self, _path: &str) -> Result<(), WorkflowValidationError> {
        Ok(())
    }
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
                expect: Some(VerificationPredicate::TitleContains {
                    value: "Example".into(),
                }),
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
    fn from_json_reports_missing_top_level_path() {
        let mut value = serde_json::to_value(definition()).unwrap();
        value.as_object_mut().unwrap().remove("steps");
        let error = WorkflowDefinition::from_value(value).unwrap_err();
        assert_eq!(error.path, "$.steps");
    }

    #[test]
    fn duplicate_step_ids_are_rejected() {
        let mut workflow = definition();
        workflow.steps.push(WorkflowStep {
            id: "open".into(),
            action: BatchStep::Screenshot,
            expect: None,
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
    fn workflow_step_flattens_batch_action() {
        let value = serde_json::to_value(&definition().steps[0]).unwrap();
        assert_eq!(value["id"], "open");
        assert_eq!(value["action"], "navigate");
        assert_eq!(value["timeoutMs"], 20_000);
    }
}
