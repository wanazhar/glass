//! Human-readable workflow authoring and deterministic compilation.
//!
//! YAML is an authoring format only. The runtime consumes the existing
//! [`super::WorkflowDefinition`] contract after this module parses, validates,
//! and analyzes the source.

use super::{BatchStep, SemanticIntentAction, WorkflowDefinition, WorkflowTransactionClass};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::fmt;

pub const WORKFLOW_AUTHORING_SCHEMA_VERSION: u32 = 1;
const MAX_SOURCE_BYTES: usize = 256 * 1024;
const MAX_DIAGNOSTIC_MESSAGE_BYTES: usize = 512;

/// Source syntax accepted by the authoring compiler.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum WorkflowAuthoringFormat {
    Yaml,
    Json,
}

/// Stable diagnostic severity used by static analysis and compilation tools.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum WorkflowDiagnosticSeverity {
    Advisory,
    Warning,
    Error,
}

/// One source-linked authoring or analysis diagnostic.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkflowDiagnostic {
    pub code: String,
    pub severity: WorkflowDiagnosticSeverity,
    pub message: String,
    pub path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub column: Option<usize>,
    pub remediation: String,
}

/// Compiled authoring output. `canonical_json` is the only artifact consumed
/// by the workflow runtime; source text is retained by the caller if needed.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkflowAuthoringDocument {
    pub schema_version: u32,
    pub source_format: WorkflowAuthoringFormat,
    pub source_hash: String,
    pub definition: WorkflowDefinition,
    pub canonical_json: String,
    pub diagnostics: Vec<WorkflowDiagnostic>,
}

/// Bounded failure returned when source cannot be converted into the runtime
/// workflow contract.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkflowCompileError {
    pub format: WorkflowAuthoringFormat,
    pub diagnostics: Vec<WorkflowDiagnostic>,
}

impl fmt::Display for WorkflowCompileError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(diagnostic) = self.diagnostics.first() {
            write!(formatter, "{}: {}", diagnostic.code, diagnostic.message)
        } else {
            formatter.write_str("workflow source compilation failed")
        }
    }
}

impl std::error::Error for WorkflowCompileError {}

/// Compile YAML or canonical JSON into the validated workflow contract.
pub fn compile_workflow(
    source: &str,
    format: WorkflowAuthoringFormat,
) -> Result<WorkflowAuthoringDocument, WorkflowCompileError> {
    if source.is_empty() || source.len() > MAX_SOURCE_BYTES {
        return Err(compile_error(
            format,
            diagnostic(
                "authoring.source_size",
                WorkflowDiagnosticSeverity::Error,
                "source is empty or exceeds the 256 KiB authoring limit",
                "$",
                None,
                None,
                "Provide a non-empty workflow smaller than 256 KiB.",
            ),
        ));
    }
    let value = match format {
        WorkflowAuthoringFormat::Yaml => serde_yaml::from_str::<serde_yaml::Value>(source)
            .map_err(|error| {
                let location = error.location();
                compile_error(
                    format,
                    diagnostic(
                        "authoring.parse",
                        WorkflowDiagnosticSeverity::Error,
                        "invalid YAML workflow source",
                        "$",
                        location.as_ref().map(|location| location.line()),
                        location.as_ref().map(|location| location.column()),
                        "Fix the YAML syntax at the reported location.",
                    ),
                )
            })
            .and_then(|value| {
                serde_json::to_value(value).map_err(|_| {
                    compile_error(
                        format,
                        diagnostic(
                            "authoring.parse",
                            WorkflowDiagnosticSeverity::Error,
                            "YAML value could not be represented as JSON",
                            "$",
                            None,
                            None,
                            "Use JSON-compatible scalar, array, and object values.",
                        ),
                    )
                })
            })?,
        WorkflowAuthoringFormat::Json => {
            serde_json::from_str::<Value>(source).map_err(|error| {
                compile_error(
                    format,
                    diagnostic(
                        "authoring.parse",
                        WorkflowDiagnosticSeverity::Error,
                        "invalid JSON workflow source",
                        "$",
                        Some(error.line()),
                        Some(error.column()),
                        "Fix the JSON syntax at the reported location.",
                    ),
                )
            })?
        }
    };
    let mut definition = WorkflowDefinition::from_value(value).map_err(|error| {
        compile_error(
            format,
            diagnostic(
                "workflow.validation",
                WorkflowDiagnosticSeverity::Error,
                error.reason,
                error.path,
                None,
                None,
                "Correct the workflow field named by the diagnostic path.",
            ),
        )
    })?;
    let mut diagnostics = infer_sensitive_inputs(&mut definition);
    let canonical_json = definition.to_canonical_json().map_err(|error| {
        compile_error(
            format,
            diagnostic(
                "workflow.canonicalization",
                WorkflowDiagnosticSeverity::Error,
                error.to_string(),
                "$",
                None,
                None,
                "Correct the workflow definition before compiling it.",
            ),
        )
    })?;
    diagnostics.extend(analyze_workflow(&definition));
    Ok(WorkflowAuthoringDocument {
        schema_version: WORKFLOW_AUTHORING_SCHEMA_VERSION,
        source_format: format,
        source_hash: source_hash(source),
        definition,
        canonical_json,
        diagnostics,
    })
}

/// Compile YAML workflow source.
pub fn compile_workflow_yaml(
    source: &str,
) -> Result<WorkflowAuthoringDocument, WorkflowCompileError> {
    compile_workflow(source, WorkflowAuthoringFormat::Yaml)
}

/// Compile JSON workflow source through the same canonical path as YAML.
pub fn compile_workflow_json(
    source: &str,
) -> Result<WorkflowAuthoringDocument, WorkflowCompileError> {
    compile_workflow(source, WorkflowAuthoringFormat::Json)
}

/// Render a validated workflow definition into deterministic YAML.
pub fn format_workflow_yaml(
    definition: &WorkflowDefinition,
) -> Result<String, WorkflowCompileError> {
    definition.validate().map_err(|error| {
        compile_error(
            WorkflowAuthoringFormat::Yaml,
            diagnostic(
                "workflow.validation",
                WorkflowDiagnosticSeverity::Error,
                error.reason,
                error.path,
                None,
                None,
                "Correct the workflow before formatting it.",
            ),
        )
    })?;
    serde_yaml::to_string(definition).map_err(|error| {
        compile_error(
            WorkflowAuthoringFormat::Yaml,
            diagnostic(
                "authoring.format",
                WorkflowDiagnosticSeverity::Error,
                "workflow could not be rendered as YAML",
                "$",
                None,
                None,
                error.to_string(),
            ),
        )
    })
}

/// Run deterministic safety and maintainability checks without a browser.
pub fn analyze_workflow(definition: &WorkflowDefinition) -> Vec<WorkflowDiagnostic> {
    let mut diagnostics = Vec::new();
    let declared_inputs = definition.inputs.keys().cloned().collect::<BTreeSet<_>>();
    if let Ok(value) = serde_json::to_value(definition) {
        collect_input_references(&value, "$", &declared_inputs, &mut diagnostics);
    }
    for (index, step) in definition.steps.iter().enumerate() {
        let path = format!("steps[{index}]");
        let mutating = step.intent.as_ref().map_or_else(
            || batch_step_is_mutating(&step.action),
            |intent| intent_action_is_mutating(intent.action),
        );
        if mutating && step.expect.is_none() {
            diagnostics.push(diagnostic(
                "workflow.missing_postcondition",
                WorkflowDiagnosticSeverity::Warning,
                "mutating step has no explicit postcondition",
                format!("{path}.expect"),
                None,
                None,
                "Add a bounded expect predicate or review the warning explicitly.",
            ));
        }
        if mutating && step.transaction == WorkflowTransactionClass::Unknown {
            diagnostics.push(diagnostic(
                "workflow.unknown_transaction",
                WorkflowDiagnosticSeverity::Warning,
                "mutating step has unknown retry and duplicate-effect behavior",
                format!("{path}.transaction"),
                None,
                None,
                "Classify the step as readOnly, idempotent, conditionallyIdempotent, or nonIdempotent.",
            ));
        }
        if step.max_retries > 0 && !step.transaction.permits_pre_dispatch_retry() {
            diagnostics.push(diagnostic(
                "workflow.unsafe_retry",
                WorkflowDiagnosticSeverity::Error,
                "step requests retries that its transaction class cannot prove safe",
                format!("{path}.maxRetries"),
                None,
                None,
                "Set maxRetries to zero or choose a retry-safe transaction class with an effect marker.",
            ));
        }
        if step.transaction == WorkflowTransactionClass::NonIdempotent
            && step.idempotency_key.is_none()
        {
            diagnostics.push(diagnostic(
                "workflow.non_idempotent_without_marker",
                WorkflowDiagnosticSeverity::Error,
                "non-idempotent step has no duplicate-effect marker",
                format!("{path}.idempotencyKey"),
                None,
                None,
                "Add an idempotency key or classify the step differently after review.",
            ));
        }
        match &step.action {
            BatchStep::Type { text, .. } if !contains_input_placeholder(text) => {
                diagnostics.push(diagnostic(
                    "workflow.literal_input_value",
                    WorkflowDiagnosticSeverity::Error,
                    "type step contains a literal value instead of an input placeholder",
                    format!("{path}.text"),
                    None,
                    None,
                    "Declare an input and use ${inputs.name}; provide the value at execution time.",
                ));
            }
            BatchStep::Select { value, .. } if !contains_input_placeholder(value) => {
                diagnostics.push(diagnostic(
                    "workflow.literal_input_value",
                    WorkflowDiagnosticSeverity::Error,
                    "select step contains a literal value instead of an input placeholder",
                    format!("{path}.value"),
                    None,
                    None,
                    "Declare an input and use ${inputs.name}; provide the value at execution time.",
                ));
            }
            _ => {}
        }
        if let Some(intent) = &step.intent
            && matches!(
                intent.action,
                SemanticIntentAction::Type | SemanticIntentAction::Select
            )
            && !intent
                .value
                .as_deref()
                .is_some_and(contains_input_placeholder)
        {
            diagnostics.push(diagnostic(
                "workflow.literal_input_value",
                WorkflowDiagnosticSeverity::Error,
                "value-bearing intent contains a literal value instead of an input placeholder",
                format!("{path}.intent.value"),
                None,
                None,
                "Declare an input and use ${inputs.name}; provide the value at execution time.",
            ));
        }
    }
    diagnostics.sort_by(|left, right| {
        left.path
            .cmp(&right.path)
            .then(left.code.cmp(&right.code))
            .then(left.severity.cmp(&right.severity))
    });
    diagnostics
}

fn infer_sensitive_inputs(definition: &mut WorkflowDefinition) -> Vec<WorkflowDiagnostic> {
    let mut diagnostics = Vec::new();
    for (name, input) in &mut definition.inputs {
        if !looks_sensitive_input_name(name) {
            continue;
        }
        match input.sensitive {
            None => {
                input.sensitive = Some(true);
                diagnostics.push(diagnostic(
                    "workflow.sensitive_input_inferred",
                    WorkflowDiagnosticSeverity::Advisory,
                    "input marked sensitive from its name",
                    format!("inputs.{name}.sensitive"),
                    None,
                    None,
                    "Keep the value outside the workflow source and provide it at execution time.",
                ));
            }
            Some(false) => diagnostics.push(diagnostic(
                "workflow.sensitive_input_denied",
                WorkflowDiagnosticSeverity::Error,
                "input name suggests sensitive data but sensitive was explicitly disabled",
                format!("inputs.{name}.sensitive"),
                None,
                None,
                "Set sensitive: true or rename the input after reviewing its data class.",
            )),
            Some(true) => {}
        }
    }
    diagnostics
}

fn looks_sensitive_input_name(name: &str) -> bool {
    let normalized = name.to_ascii_lowercase();
    [
        "password", "passwd", "secret", "token", "api_key", "apikey", "cookie",
    ]
    .iter()
    .any(|term| normalized.contains(term))
}

fn contains_input_placeholder(value: &str) -> bool {
    value.contains("${inputs.")
}

fn collect_input_references(
    value: &Value,
    path: &str,
    declared_inputs: &BTreeSet<String>,
    diagnostics: &mut Vec<WorkflowDiagnostic>,
) {
    match value {
        Value::String(text) => {
            let marker = "${inputs.";
            let mut remainder = text.as_str();
            while let Some(start) = remainder.find(marker) {
                let placeholder = &remainder[start..];
                let Some(end) = placeholder.find('}') else {
                    diagnostics.push(diagnostic(
                        "workflow.invalid_input_reference",
                        WorkflowDiagnosticSeverity::Error,
                        "input placeholder is missing a closing brace",
                        path,
                        None,
                        None,
                        "Use a complete ${inputs.name} placeholder.",
                    ));
                    break;
                };
                let name = &placeholder[marker.len()..end];
                if name.is_empty() || name.contains(['{', '}', '$']) {
                    diagnostics.push(diagnostic(
                        "workflow.invalid_input_reference",
                        WorkflowDiagnosticSeverity::Error,
                        "input placeholder name is invalid",
                        path,
                        None,
                        None,
                        "Use a simple declared input name inside ${inputs.name}.",
                    ));
                } else if !declared_inputs.contains(name) {
                    diagnostics.push(diagnostic(
                        "workflow.undefined_input",
                        WorkflowDiagnosticSeverity::Error,
                        "workflow references an input that is not declared",
                        path,
                        None,
                        None,
                        "Declare the input or remove the placeholder before execution.",
                    ));
                }
                remainder = &placeholder[end + 1..];
            }
        }
        Value::Array(items) => {
            for (index, item) in items.iter().enumerate() {
                collect_input_references(
                    item,
                    &format!("{path}[{index}]"),
                    declared_inputs,
                    diagnostics,
                );
            }
        }
        Value::Object(fields) => {
            for (name, item) in fields {
                collect_input_references(
                    item,
                    &format!("{path}.{name}"),
                    declared_inputs,
                    diagnostics,
                );
            }
        }
        Value::Null | Value::Bool(_) | Value::Number(_) => {}
    }
}

fn batch_step_is_mutating(step: &BatchStep) -> bool {
    matches!(
        step,
        BatchStep::Navigate { .. }
            | BatchStep::Click { .. }
            | BatchStep::Type { .. }
            | BatchStep::Check { .. }
            | BatchStep::Uncheck { .. }
            | BatchStep::Select { .. }
            | BatchStep::Clear { .. }
            | BatchStep::Scroll { .. }
            | BatchStep::Evaluate { .. }
            | BatchStep::AcceptDialog
            | BatchStep::DismissDialog
    )
}

fn intent_action_is_mutating(action: SemanticIntentAction) -> bool {
    !matches!(
        action,
        SemanticIntentAction::Inspect | SemanticIntentAction::Extract
    )
}

fn source_hash(source: &str) -> String {
    let digest = Sha256::digest(source.as_bytes());
    format!("sha256:{digest:x}")
}

fn compile_error(
    format: WorkflowAuthoringFormat,
    diagnostic: WorkflowDiagnostic,
) -> WorkflowCompileError {
    WorkflowCompileError {
        format,
        diagnostics: vec![diagnostic],
    }
}

fn diagnostic(
    code: impl Into<String>,
    severity: WorkflowDiagnosticSeverity,
    message: impl Into<String>,
    path: impl Into<String>,
    line: Option<usize>,
    column: Option<usize>,
    remediation: impl Into<String>,
) -> WorkflowDiagnostic {
    WorkflowDiagnostic {
        code: code.into(),
        severity,
        message: bound_text(&message.into(), MAX_DIAGNOSTIC_MESSAGE_BYTES),
        path: path.into(),
        line,
        column,
        remediation: bound_text(&remediation.into(), MAX_DIAGNOSTIC_MESSAGE_BYTES),
    }
}

fn bound_text(value: &str, maximum: usize) -> String {
    if value.len() <= maximum {
        return value.to_string();
    }
    let mut end = maximum.saturating_sub(15);
    while end > 0 && !value.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}…[truncated]", &value[..end])
}

#[cfg(test)]
mod tests {
    use super::*;

    const YAML: &str = r#"
schemaVersion: 1
name: docs-search
workflowVersion: 1.0.0
inputs: {}
budgets:
  maxSteps: 1
  maxDurationMs: 30000
  maxRetries: 0
  maxExtractedBytes: 4096
steps:
  - id: search
    action: navigate
    url: https://example.test/docs
    transaction: read_only
    expect:
      urlEquals: https://example.test/docs
terminalCondition:
  urlEquals: https://example.test/docs
outputs: {}
"#;

    #[test]
    fn yaml_compiles_to_runtime_contract_and_stable_hash() {
        let document = compile_workflow_yaml(YAML).unwrap();
        assert_eq!(document.schema_version, 1);
        assert_eq!(document.definition.name, "docs-search");
        assert!(document.canonical_json.contains("schemaVersion"));
        assert_eq!(document.source_hash, source_hash(YAML));
    }

    #[test]
    fn yaml_and_json_share_canonical_semantics() {
        let yaml = compile_workflow_yaml(YAML).unwrap();
        let json = compile_workflow_json(&yaml.canonical_json).unwrap();
        assert_eq!(yaml.canonical_json, json.canonical_json);
    }

    #[test]
    fn parser_reports_location_without_echoing_source() {
        let error = compile_workflow_yaml("schemaVersion: [").unwrap_err();
        let diagnostic = &error.diagnostics[0];
        assert_eq!(diagnostic.code, "authoring.parse");
        assert!(diagnostic.line.is_some());
        assert!(!diagnostic.message.contains("schemaVersion: ["));
    }

    #[test]
    fn analyzer_reports_mutation_and_sensitive_input_findings() {
        let source = YAML
            .replace(
                "inputs: {}",
                "inputs:\n  password:\n    type: string\n    required: true",
            )
            .replace(
                "    expect:\n      urlEquals: https://example.test/docs\n",
                "",
            );
        let document = compile_workflow_yaml(&source).unwrap();
        assert!(
            document
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code == "workflow.missing_postcondition")
        );
        assert!(
            document
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code == "workflow.sensitive_input_inferred")
        );
        assert_eq!(document.definition.inputs["password"].sensitive, Some(true));
    }

    #[test]
    fn analyzer_reports_undefined_inputs_and_literal_values() {
        let source = r#"
schemaVersion: 1
name: input-check
workflowVersion: 1.0.0
inputs: {}
budgets:
  maxSteps: 1
  maxDurationMs: 30000
  maxRetries: 0
  maxExtractedBytes: 4096
steps:
  - id: type
    action: type
    text: literal-secret
    target: "role=textbox;name=Search"
    transaction: non_idempotent
    idempotencyKey: type-once
    expect:
      textContains: "${inputs.missing}"
terminalCondition:
  textContains: done
outputs: {}
"#;
        let document = compile_workflow_yaml(&source).unwrap();
        assert!(
            document
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code == "workflow.undefined_input")
        );
        assert!(
            document
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code == "workflow.literal_input_value")
        );
    }

    #[test]
    fn formatter_rejects_invalid_definition() {
        let mut document = compile_workflow_yaml(YAML).unwrap();
        document.definition.steps.clear();
        let error = format_workflow_yaml(&document.definition).unwrap_err();
        assert_eq!(error.diagnostics[0].code, "workflow.validation");
    }
}
