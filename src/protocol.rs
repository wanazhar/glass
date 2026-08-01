//! Transport-neutral Glass request and response envelopes.
//!
//! MCP keeps its JSON-RPC framing, but daemon clients and embedded callers use
//! these envelopes for the operation payload. The envelope is intentionally
//! small: transport-specific framing, streaming, and authentication remain
//! outside this contract.

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Version of the canonical Glass operation envelope.
pub const GLASS_PROTOCOL_VERSION: u32 = 1;
const MAX_ID_BYTES: usize = 128;
const MAX_OPERATION_BYTES: usize = 96;
const MAX_ERROR_CODE_BYTES: usize = 64;
const MAX_MESSAGE_BYTES: usize = 512;
const MAX_DEADLINE_MS: u64 = 15 * 60 * 1_000;

/// Canonical transport operation for browser-free Web IR validation.
pub const WEB_IR_VALIDATE_OPERATION: &str = "webIr.validate";
/// Canonical transport operation for browser-free Web IR inspection.
pub const WEB_IR_INSPECT_OPERATION: &str = "webIr.inspect";
/// Canonical transport operation for browser-free Web IR revision diffs.
pub const WEB_IR_DIFF_OPERATION: &str = "webIr.diff";
/// Canonical transport operation for browser-free Web IR continuity checks.
pub const WEB_IR_CONTINUITY_OPERATION: &str = "webIr.continuity";

/// Canonical transport operation for browser-free Task Protocol compilation.
pub const TASK_COMPILE_OPERATION: &str = "task.compile";
/// Canonical transport operation for browser-free Task Protocol validation.
pub const TASK_VALIDATE_OPERATION: &str = "task.validate";

/// Typed payload carried by a `task.compile` request.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TaskCompilePayload {
    pub task: crate::task_protocol::GlassTask,
}

impl TaskCompilePayload {
    /// Validate the authored task before compiler dispatch.
    pub fn validate(&self) -> Result<(), ProtocolError> {
        self.task
            .validate()
            .map_err(|error| ProtocolError::TaskCompilation(error.into()))
    }
}

/// Typed payload carried by a `task.validate` request.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TaskValidationPayload {
    pub task: crate::task_protocol::GlassTask,
}

impl TaskValidationPayload {
    /// Validate the authored task before validation dispatch.
    pub fn validate(&self) -> Result<(), ProtocolError> {
        self.task.validate().map_err(ProtocolError::TaskValidation)
    }
}

/// Typed payload carried by a `webIr.validate` or `webIr.inspect` request.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WebIrDraftPayload {
    pub draft: crate::web_ir::GlassWebIrDraft,
}

impl WebIrDraftPayload {
    /// Validate the draft graph before browser-free dispatch.
    pub fn validate(&self) -> Result<(), ProtocolError> {
        self.draft
            .validate()
            .map_err(ProtocolError::WebIrValidation)
    }
}

/// Typed payload carried by a `webIr.diff` request.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WebIrDiffPayload {
    pub before: crate::web_ir::GlassWebIrDraft,
    pub after: crate::web_ir::GlassWebIrDraft,
}

impl WebIrDiffPayload {
    /// Validate both draft graphs before diff dispatch.
    pub fn validate(&self) -> Result<(), ProtocolError> {
        self.before
            .validate()
            .map_err(ProtocolError::WebIrValidation)?;
        self.after
            .validate()
            .map_err(ProtocolError::WebIrValidation)?;
        Ok(())
    }
}

/// Typed payload carried by a `webIr.continuity` request.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WebIrContinuityPayload {
    pub before: crate::web_ir::GlassWebIrDraft,
    pub after: crate::web_ir::GlassWebIrDraft,
    pub entity_id: String,
}

impl WebIrContinuityPayload {
    /// Validate both draft graphs and the bounded source entity ID.
    pub fn validate(&self) -> Result<(), ProtocolError> {
        self.before
            .validate()
            .map_err(ProtocolError::WebIrValidation)?;
        self.after
            .validate()
            .map_err(ProtocolError::WebIrValidation)?;
        validate_identifier(&self.entity_id, "entityId")
    }
}

/// Typed successful result for a `task.compile` operation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TaskCompileResult {
    pub plan: crate::task_compiler::TaskExecutionPlan,
}

/// Typed successful result for browser-free Task Protocol validation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TaskValidationResult {
    pub valid: bool,
    pub schema_version: u32,
    pub task: crate::task_protocol::TaskKind,
}

/// Bounded successful result for browser-free Web IR validation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WebIrValidationResult {
    pub valid: bool,
    pub schema_version: u32,
    pub revision: u64,
}

impl WebIrValidationResult {
    /// Build the validation result after a draft has passed graph validation.
    pub fn from_draft(draft: &crate::web_ir::GlassWebIrDraft) -> Self {
        Self {
            valid: true,
            schema_version: draft.schema_version,
            revision: draft.revision,
        }
    }
}

/// Bounded summary result for browser-free Web IR inspection.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WebIrInspectionResult {
    pub schema_version: u32,
    pub revision: u64,
    pub entity_count: usize,
    pub relationship_count: usize,
    pub coverage: crate::extraction::EvidenceCoverage,
    pub truncated: bool,
    pub opaque_regions: u32,
    pub diagnostic_count: usize,
    pub relationship_hint_diagnostic_count: usize,
}

impl WebIrInspectionResult {
    /// Build a bounded summary after a draft has passed graph validation.
    pub fn from_draft(draft: &crate::web_ir::GlassWebIrDraft) -> Self {
        Self {
            schema_version: draft.schema_version,
            revision: draft.revision,
            entity_count: draft.entities.len(),
            relationship_count: draft.relationships.len(),
            coverage: draft.coverage.clone(),
            truncated: draft.limits.truncated,
            opaque_regions: draft.coverage.opaque_regions,
            diagnostic_count: draft.diagnostics.len(),
            relationship_hint_diagnostic_count: draft.relationship_hint_diagnostics.len(),
        }
    }
}

/// Bounded summary result for a browser-free Web IR revision diff.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WebIrDiffResult {
    pub schema_version: u32,
    pub from_revision: u64,
    pub to_revision: u64,
    pub entity_added_count: usize,
    pub entity_removed_count: usize,
    pub entity_changed_count: usize,
    pub relationship_added_count: usize,
    pub relationship_removed_count: usize,
    pub coverage_changed: bool,
    pub limits_changed: bool,
    pub diagnostics_changed: bool,
    pub relationship_hint_diagnostics_changed: bool,
}

impl WebIrDiffResult {
    /// Build a bounded summary without exposing entity or page content.
    pub fn from_diff(diff: &crate::web_ir::GlassWebIrDiff) -> Self {
        Self {
            schema_version: diff.schema_version,
            from_revision: diff.from_revision,
            to_revision: diff.to_revision,
            entity_added_count: diff
                .entity_changes
                .iter()
                .filter(|change| change.kind == crate::web_ir::DraftChangeKind::Added)
                .count(),
            entity_removed_count: diff
                .entity_changes
                .iter()
                .filter(|change| change.kind == crate::web_ir::DraftChangeKind::Removed)
                .count(),
            entity_changed_count: diff
                .entity_changes
                .iter()
                .filter(|change| change.kind == crate::web_ir::DraftChangeKind::Changed)
                .count(),
            relationship_added_count: diff
                .relationship_changes
                .iter()
                .filter(|change| change.kind == crate::web_ir::DraftChangeKind::Added)
                .count(),
            relationship_removed_count: diff
                .relationship_changes
                .iter()
                .filter(|change| change.kind == crate::web_ir::DraftChangeKind::Removed)
                .count(),
            coverage_changed: diff.coverage_changed,
            limits_changed: diff.limits_changed,
            diagnostics_changed: diff.diagnostics_changed,
            relationship_hint_diagnostics_changed: diff.relationship_hint_diagnostics_changed,
        }
    }
}

/// Browser-free Web IR continuity classification for one entity.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WebIrContinuityResult {
    pub requested_id: String,
    pub status: crate::web_ir::DraftEntityContinuityStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current_id: Option<String>,
    pub reason: String,
}

impl From<crate::web_ir::DraftEntityContinuity> for WebIrContinuityResult {
    fn from(continuity: crate::web_ir::DraftEntityContinuity) -> Self {
        Self {
            requested_id: continuity.requested_id,
            status: continuity.status,
            current_id: continuity.current_id,
            reason: continuity.reason,
        }
    }
}

impl TaskCompileResult {
    /// Validate the embedded deterministic execution plan.
    pub fn validate(&self) -> Result<(), ProtocolError> {
        self.plan.validate().map_err(ProtocolError::TaskCompilation)
    }
}

/// A request-independent mutation lease reference.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MutationLeaseRef {
    pub session_id: String,
    pub token: String,
}

/// Canonical operation request shared by supported transports.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GlassRequest {
    pub protocol_version: u32,
    pub request_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub correlation_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mutation_lease: Option<MutationLeaseRef>,
    pub operation: String,
    pub payload: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deadline_ms: Option<u64>,
}

impl GlassRequest {
    /// Validate protocol version, identifiers, operation bounds, and deadline.
    pub fn validate(&self) -> Result<(), ProtocolError> {
        if self.protocol_version != GLASS_PROTOCOL_VERSION {
            return Err(ProtocolError::UnsupportedVersion(self.protocol_version));
        }
        validate_identifier(&self.request_id, "requestId")?;
        if let Some(correlation_id) = &self.correlation_id {
            validate_identifier(correlation_id, "correlationId")?;
        }
        if let Some(session_id) = &self.session_id {
            validate_identifier(session_id, "sessionId")?;
        }
        if let Some(lease) = &self.mutation_lease {
            validate_identifier(&lease.session_id, "mutationLease.sessionId")?;
            validate_identifier(&lease.token, "mutationLease.token")?;
        }
        if self.operation.is_empty() || self.operation.len() > MAX_OPERATION_BYTES {
            return Err(ProtocolError::InvalidField(
                "operation must be a bounded non-empty string".into(),
            ));
        }
        if self.operation.chars().any(char::is_whitespace) {
            return Err(ProtocolError::InvalidField(
                "operation must not contain whitespace".into(),
            ));
        }
        if let Some(deadline_ms) = self.deadline_ms
            && !(1..=MAX_DEADLINE_MS).contains(&deadline_ms)
        {
            return Err(ProtocolError::InvalidField(format!(
                "deadlineMs must be 1..={MAX_DEADLINE_MS}"
            )));
        }
        Ok(())
    }

    /// Decode and validate a typed `task.compile` payload.
    pub fn decode_task_compile(&self) -> Result<TaskCompilePayload, ProtocolError> {
        self.validate()?;
        if self.operation != TASK_COMPILE_OPERATION {
            return Err(ProtocolError::InvalidField(format!(
                "expected operation {TASK_COMPILE_OPERATION}"
            )));
        }
        let payload: TaskCompilePayload =
            serde_json::from_value(self.payload.clone()).map_err(|error| {
                ProtocolError::InvalidField(format!("task.compile payload: {error}"))
            })?;
        payload.validate()?;
        Ok(payload)
    }

    /// Decode and validate a typed `task.validate` payload.
    pub fn decode_task_validate(&self) -> Result<TaskValidationPayload, ProtocolError> {
        self.validate()?;
        if self.operation != TASK_VALIDATE_OPERATION {
            return Err(ProtocolError::InvalidField(format!(
                "expected operation {TASK_VALIDATE_OPERATION}"
            )));
        }
        let payload: TaskValidationPayload =
            serde_json::from_value(self.payload.clone()).map_err(|error| {
                ProtocolError::InvalidField(format!("task.validate payload: {error}"))
            })?;
        payload.validate()?;
        Ok(payload)
    }

    /// Decode and validate a typed `webIr.validate` payload.
    pub fn decode_web_ir_validate(&self) -> Result<WebIrDraftPayload, ProtocolError> {
        self.decode_web_ir_draft(WEB_IR_VALIDATE_OPERATION)
    }

    /// Decode and validate a typed `webIr.inspect` payload.
    pub fn decode_web_ir_inspect(&self) -> Result<WebIrDraftPayload, ProtocolError> {
        self.decode_web_ir_draft(WEB_IR_INSPECT_OPERATION)
    }

    fn decode_web_ir_draft(&self, operation: &str) -> Result<WebIrDraftPayload, ProtocolError> {
        self.validate()?;
        if self.operation != operation {
            return Err(ProtocolError::InvalidField(format!(
                "expected operation {operation}"
            )));
        }
        let payload: WebIrDraftPayload =
            serde_json::from_value(self.payload.clone()).map_err(|error| {
                ProtocolError::InvalidField(format!("{operation} payload: {error}"))
            })?;
        payload.validate()?;
        Ok(payload)
    }

    /// Decode and validate a typed `webIr.diff` payload.
    pub fn decode_web_ir_diff(&self) -> Result<WebIrDiffPayload, ProtocolError> {
        self.validate()?;
        if self.operation != WEB_IR_DIFF_OPERATION {
            return Err(ProtocolError::InvalidField(format!(
                "expected operation {WEB_IR_DIFF_OPERATION}"
            )));
        }
        let payload: WebIrDiffPayload = serde_json::from_value(self.payload.clone())
            .map_err(|error| ProtocolError::InvalidField(format!("webIr.diff payload: {error}")))?;
        payload.validate()?;
        Ok(payload)
    }

    /// Decode and validate a typed `webIr.continuity` payload.
    pub fn decode_web_ir_continuity(&self) -> Result<WebIrContinuityPayload, ProtocolError> {
        self.validate()?;
        if self.operation != WEB_IR_CONTINUITY_OPERATION {
            return Err(ProtocolError::InvalidField(format!(
                "expected operation {WEB_IR_CONTINUITY_OPERATION}"
            )));
        }
        let payload: WebIrContinuityPayload = serde_json::from_value(self.payload.clone())
            .map_err(|error| {
                ProtocolError::InvalidField(format!("webIr.continuity payload: {error}"))
            })?;
        payload.validate()?;
        Ok(payload)
    }
}

/// Decode and compile a `task.compile` request without browser access.
pub fn compile_task_request(
    request: &GlassRequest,
) -> Result<crate::task_compiler::TaskExecutionPlan, ProtocolError> {
    let payload = request.decode_task_compile()?;
    crate::task_compiler::compile_task(&payload.task).map_err(ProtocolError::TaskCompilation)
}

/// Decode and compile a `task.compile` request into a typed response payload.
pub fn compile_task_result(request: &GlassRequest) -> Result<TaskCompileResult, ProtocolError> {
    Ok(TaskCompileResult {
        plan: compile_task_request(request)?,
    })
}

/// Validate a `task.validate` request into a typed response payload.
pub fn validate_task_result(request: &GlassRequest) -> Result<TaskValidationResult, ProtocolError> {
    let payload = request.decode_task_validate()?;
    Ok(TaskValidationResult {
        valid: true,
        schema_version: payload.task.schema_version,
        task: payload.task.task,
    })
}

/// Validate a Web IR draft from a canonical request.
pub fn web_ir_validate_result(
    request: &GlassRequest,
) -> Result<WebIrValidationResult, ProtocolError> {
    let payload = request.decode_web_ir_validate()?;
    Ok(WebIrValidationResult::from_draft(&payload.draft))
}

/// Inspect a Web IR draft from a canonical request.
pub fn web_ir_inspect_result(
    request: &GlassRequest,
) -> Result<WebIrInspectionResult, ProtocolError> {
    let payload = request.decode_web_ir_inspect()?;
    Ok(WebIrInspectionResult::from_draft(&payload.draft))
}

/// Compute a bounded Web IR diff from a canonical request.
pub fn web_ir_diff_result(request: &GlassRequest) -> Result<WebIrDiffResult, ProtocolError> {
    let payload = request.decode_web_ir_diff()?;
    let diff = payload
        .before
        .diff(&payload.after)
        .map_err(ProtocolError::WebIrValidation)?;
    Ok(WebIrDiffResult::from_diff(&diff))
}

/// Classify one Web IR entity from a canonical request.
pub fn web_ir_continuity_result(
    request: &GlassRequest,
) -> Result<WebIrContinuityResult, ProtocolError> {
    let payload = request.decode_web_ir_continuity()?;
    let continuity = payload
        .before
        .classify_entity_continuity(&payload.after, &payload.entity_id)
        .map_err(ProtocolError::WebIrValidation)?;
    Ok(WebIrContinuityResult::from(continuity))
}

/// Canonical operation response shared by supported transports.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GlassResponse {
    pub protocol_version: u32,
    pub request_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub correlation_id: Option<String>,
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<GlassError>,
}

impl GlassResponse {
    /// Validate envelope identity and the mutually exclusive result/error form.
    pub fn validate(&self) -> Result<(), ProtocolError> {
        if self.protocol_version != GLASS_PROTOCOL_VERSION {
            return Err(ProtocolError::UnsupportedVersion(self.protocol_version));
        }
        validate_identifier(&self.request_id, "requestId")?;
        if let Some(correlation_id) = &self.correlation_id {
            validate_identifier(correlation_id, "correlationId")?;
        }
        match (self.ok, self.result.is_some(), self.error.is_some()) {
            (true, true, false) | (false, false, true) => Ok(()),
            _ => Err(ProtocolError::InvalidField(
                "ok responses require result and error responses require error".into(),
            )),
        }
    }

    /// Decode and validate a successful typed `task.compile` result.
    pub fn decode_task_compile_result(&self) -> Result<TaskCompileResult, ProtocolError> {
        self.validate()?;
        if !self.ok {
            return Err(ProtocolError::InvalidField(
                "task.compile result requires a successful response".into(),
            ));
        }
        let value = self
            .result
            .clone()
            .ok_or_else(|| ProtocolError::InvalidField("task.compile result is missing".into()))?;
        let result: TaskCompileResult = serde_json::from_value(value).map_err(|error| {
            ProtocolError::InvalidField(format!("task.compile result: {error}"))
        })?;
        result.validate()?;
        Ok(result)
    }

    /// Decode and validate a successful typed `task.validate` result.
    pub fn decode_task_validation_result(&self) -> Result<TaskValidationResult, ProtocolError> {
        self.validate()?;
        if !self.ok {
            return Err(ProtocolError::InvalidField(
                "task.validate result requires a successful response".into(),
            ));
        }
        let value = self
            .result
            .clone()
            .ok_or_else(|| ProtocolError::InvalidField("task.validate result is missing".into()))?;
        serde_json::from_value(value)
            .map_err(|error| ProtocolError::InvalidField(format!("task.validate result: {error}")))
    }

    /// Decode and validate a successful bounded `webIr.validate` result.
    pub fn decode_web_ir_validation_result(&self) -> Result<WebIrValidationResult, ProtocolError> {
        self.decode_web_ir_result("webIr.validate")
    }

    /// Decode and validate a successful bounded `webIr.inspect` result.
    pub fn decode_web_ir_inspection_result(&self) -> Result<WebIrInspectionResult, ProtocolError> {
        self.decode_web_ir_result("webIr.inspect")
    }

    fn decode_web_ir_result<T>(&self, operation: &str) -> Result<T, ProtocolError>
    where
        T: for<'de> Deserialize<'de>,
    {
        self.validate()?;
        if !self.ok {
            return Err(ProtocolError::InvalidField(format!(
                "{operation} result requires a successful response"
            )));
        }
        let value = self
            .result
            .clone()
            .ok_or_else(|| ProtocolError::InvalidField(format!("{operation} result is missing")))?;
        serde_json::from_value(value)
            .map_err(|error| ProtocolError::InvalidField(format!("{operation} result: {error}")))
    }

    /// Decode and validate a successful bounded `webIr.diff` result.
    pub fn decode_web_ir_diff_result(&self) -> Result<WebIrDiffResult, ProtocolError> {
        self.validate()?;
        if !self.ok {
            return Err(ProtocolError::InvalidField(
                "webIr.diff result requires a successful response".into(),
            ));
        }
        let value = self
            .result
            .clone()
            .ok_or_else(|| ProtocolError::InvalidField("webIr.diff result is missing".into()))?;
        serde_json::from_value(value)
            .map_err(|error| ProtocolError::InvalidField(format!("webIr.diff result: {error}")))
    }

    /// Decode and validate a successful Web IR continuity result.
    pub fn decode_web_ir_continuity_result(&self) -> Result<WebIrContinuityResult, ProtocolError> {
        self.validate()?;
        if !self.ok {
            return Err(ProtocolError::InvalidField(
                "webIr.continuity result requires a successful response".into(),
            ));
        }
        let value = self.result.clone().ok_or_else(|| {
            ProtocolError::InvalidField("webIr.continuity result is missing".into())
        })?;
        serde_json::from_value(value).map_err(|error| {
            ProtocolError::InvalidField(format!("webIr.continuity result: {error}"))
        })
    }
}

/// Phase in which a public operation stopped.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum ErrorPhase {
    #[default]
    Preflight,
    Dispatch,
    PostDispatch,
    Verification,
    Reconciliation,
}

/// Stable retry classification for agent recovery.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum RetryClassification {
    SafeImmediate,
    #[default]
    SafeAfterReobserve,
    SafeAfterReconcile,
    UnsafeUntilReconciled,
    RequiresUserDecision,
    NotRetryable,
    Unknown,
}

/// Bounded recovery guidance attached to every canonical failure.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RetryGuidance {
    pub classification: RetryClassification,
    pub recommended_operation: String,
}

impl Default for RetryGuidance {
    fn default() -> Self {
        Self {
            classification: RetryClassification::SafeAfterReobserve,
            recommended_operation: "inspect_page".into(),
        }
    }
}

/// Structured failure that can be carried across transports without parsing text.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GlassError {
    pub code: String,
    #[serde(default)]
    pub phase: ErrorPhase,
    pub message: String,
    #[serde(default)]
    pub mutation_possible: bool,
    #[serde(default)]
    pub retry: RetryGuidance,
    /// Kept as a tolerated compatibility field for pre-0.2.2 clients.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retryable: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<Value>,
}

impl GlassError {
    /// Validate bounded, non-empty diagnostic fields.
    pub fn validate(&self) -> Result<(), ProtocolError> {
        if self.code.is_empty() || self.code.len() > MAX_ERROR_CODE_BYTES {
            return Err(ProtocolError::InvalidField(
                "error code must be a bounded non-empty string".into(),
            ));
        }
        if self.message.is_empty() || self.message.len() > MAX_MESSAGE_BYTES {
            return Err(ProtocolError::InvalidField(
                "error message must be a bounded non-empty string".into(),
            ));
        }
        validate_identifier(
            &self.retry.recommended_operation,
            "retry.recommendedOperation",
        )?;
        Ok(())
    }
}

/// Validation failure for the canonical protocol envelope.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProtocolError {
    UnsupportedVersion(u32),
    InvalidField(String),
    TaskValidation(crate::task_protocol::TaskProtocolError),
    TaskCompilation(crate::task_compiler::TaskCompilationError),
    WebIrValidation(crate::web_ir::WebIrValidationError),
}

impl std::fmt::Display for ProtocolError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnsupportedVersion(version) => {
                write!(formatter, "unsupported Glass protocol version {version}")
            }
            Self::InvalidField(detail) => formatter.write_str(detail),
            Self::TaskValidation(error) => error.fmt(formatter),
            Self::TaskCompilation(error) => error.fmt(formatter),
            Self::WebIrValidation(error) => error.fmt(formatter),
        }
    }
}

impl std::error::Error for ProtocolError {}

fn validate_identifier(value: &str, field: &str) -> Result<(), ProtocolError> {
    if value.is_empty() || value.len() > MAX_ID_BYTES || value.chars().any(char::is_whitespace) {
        return Err(ProtocolError::InvalidField(format!(
            "{field} must be a bounded non-whitespace identifier"
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request() -> GlassRequest {
        GlassRequest {
            protocol_version: GLASS_PROTOCOL_VERSION,
            request_id: "request-1".into(),
            correlation_id: Some("run-1".into()),
            session_id: Some("session-1".into()),
            mutation_lease: Some(MutationLeaseRef {
                session_id: "session-1".into(),
                token: "lease-1".into(),
            }),
            operation: "browser.observe".into(),
            payload: serde_json::json!({"level": "interactive"}),
            deadline_ms: Some(5_000),
        }
    }

    fn web_ir_draft(revision: u64, name: &str) -> crate::web_ir::GlassWebIrDraft {
        serde_json::from_value(serde_json::json!({
            "schemaVersion": 1,
            "revision": revision,
            "entities": [
                {
                    "id": "page",
                    "kind": "page",
                    "quality": "confirmed",
                    "evidenceSources": []
                },
                {
                    "id": "field-1",
                    "kind": "field",
                    "role": "textbox",
                    "name": name,
                    "quality": "strong",
                    "evidenceSources": ["dom"]
                }
            ],
            "relationships": [
                {"from": "page", "to": "field-1", "kind": "contains"}
            ],
            "coverage": {
                "structural": "strong",
                "semantic": "strong",
                "interactiveEntitiesObserved": 1,
                "opaqueRegions": 0,
                "reasons": []
            },
            "limits": {
                "truncated": false,
                "omittedFacts": 0,
                "textBytes": 0,
                "missingSources": []
            }
        }))
        .unwrap()
    }

    #[test]
    fn request_round_trips_and_validates() {
        let request = request();
        request.validate().unwrap();
        let value = serde_json::to_value(&request).unwrap();
        assert_eq!(value["protocolVersion"], 1);
        assert_eq!(value["mutationLease"]["sessionId"], "session-1");
        let decoded: GlassRequest = serde_json::from_value(value).unwrap();
        assert_eq!(decoded, request);
    }

    #[test]
    fn response_requires_exactly_one_outcome() {
        let response = GlassResponse {
            protocol_version: GLASS_PROTOCOL_VERSION,
            request_id: "request-1".into(),
            correlation_id: None,
            ok: false,
            result: None,
            error: Some(GlassError {
                code: "target.stale".into(),
                phase: ErrorPhase::Preflight,
                message: "a mutation lease is required".into(),
                mutation_possible: false,
                retry: RetryGuidance {
                    classification: RetryClassification::SafeAfterReobserve,
                    recommended_operation: "inspect_page".into(),
                },
                retryable: Some(true),
                details: None,
            }),
        };
        response.validate().unwrap();
        let mut invalid = response.clone();
        invalid.ok = true;
        assert!(invalid.validate().is_err());
    }

    #[test]
    fn web_ir_revision_operations_round_trip_with_bounded_results() {
        let before = web_ir_draft(7, "Email");
        let after = web_ir_draft(8, "Email address");
        let diff_request = GlassRequest {
            protocol_version: GLASS_PROTOCOL_VERSION,
            request_id: "diff-1".into(),
            correlation_id: None,
            session_id: None,
            mutation_lease: None,
            operation: WEB_IR_DIFF_OPERATION.into(),
            payload: serde_json::to_value(WebIrDiffPayload {
                before: before.clone(),
                after: after.clone(),
            })
            .unwrap(),
            deadline_ms: None,
        };
        let diff = web_ir_diff_result(&diff_request).unwrap();
        assert_eq!(diff.from_revision, 7);
        assert_eq!(diff.to_revision, 8);
        assert_eq!(diff.entity_changed_count, 1);
        assert_eq!(diff_request.decode_web_ir_diff().unwrap().before, before);

        let continuity_request = GlassRequest {
            protocol_version: GLASS_PROTOCOL_VERSION,
            request_id: "continuity-1".into(),
            correlation_id: None,
            session_id: None,
            mutation_lease: None,
            operation: WEB_IR_CONTINUITY_OPERATION.into(),
            payload: serde_json::to_value(WebIrContinuityPayload {
                before,
                after,
                entity_id: "field-1".into(),
            })
            .unwrap(),
            deadline_ms: None,
        };
        let continuity = web_ir_continuity_result(&continuity_request).unwrap();
        assert_eq!(
            continuity.status,
            crate::web_ir::DraftEntityContinuityStatus::Changed
        );
        let response = GlassResponse {
            protocol_version: GLASS_PROTOCOL_VERSION,
            request_id: continuity_request.request_id.clone(),
            correlation_id: None,
            ok: true,
            result: Some(serde_json::to_value(&continuity).unwrap()),
            error: None,
        };
        assert_eq!(
            response.decode_web_ir_continuity_result().unwrap(),
            continuity
        );
    }

    #[test]
    fn web_ir_inspect_and_validate_operations_round_trip() {
        let draft = web_ir_draft(7, "Email");
        let validate_request = GlassRequest {
            protocol_version: GLASS_PROTOCOL_VERSION,
            request_id: "validate-1".into(),
            correlation_id: None,
            session_id: None,
            mutation_lease: None,
            operation: WEB_IR_VALIDATE_OPERATION.into(),
            payload: serde_json::json!({"draft": draft.clone()}),
            deadline_ms: None,
        };
        let validation = web_ir_validate_result(&validate_request).unwrap();
        assert!(validation.valid);
        assert_eq!(
            validate_request.decode_web_ir_validate().unwrap().draft,
            draft
        );
        let validation_response = GlassResponse {
            protocol_version: GLASS_PROTOCOL_VERSION,
            request_id: "validate-1".into(),
            correlation_id: None,
            ok: true,
            result: Some(serde_json::to_value(&validation).unwrap()),
            error: None,
        };
        assert_eq!(
            validation_response
                .decode_web_ir_validation_result()
                .unwrap(),
            validation
        );

        let inspect_request = GlassRequest {
            operation: WEB_IR_INSPECT_OPERATION.into(),
            request_id: "inspect-1".into(),
            payload: serde_json::json!({"draft": draft}),
            ..validate_request
        };
        let inspection = web_ir_inspect_result(&inspect_request).unwrap();
        let inspection_response = GlassResponse {
            protocol_version: GLASS_PROTOCOL_VERSION,
            request_id: "inspect-1".into(),
            correlation_id: None,
            ok: true,
            result: Some(serde_json::to_value(&inspection).unwrap()),
            error: None,
        };
        assert_eq!(
            inspection_response
                .decode_web_ir_inspection_result()
                .unwrap(),
            inspection
        );
    }

    #[test]
    fn bounds_and_unknown_fields_fail_closed() {
        let mut request = request();
        request.operation = "bad operation".into();
        assert!(request.validate().is_err());
        let unknown = serde_json::json!({
            "protocolVersion": 1,
            "requestId": "request-1",
            "operation": "browser.observe",
            "payload": {},
            "future": true
        });
        assert!(serde_json::from_value::<GlassRequest>(unknown).is_err());
    }

    #[test]
    fn task_validate_boundary_decodes_without_compiling() {
        let task = serde_json::json!({
            "schemaVersion": 1,
            "task": "region.extract",
            "scope": {"regionName": "Checkout"},
            "limits": {"maxActions": 4, "timeoutMs": 2000, "maxItems": 16},
            "risk": "readOnly"
        });
        let request = GlassRequest {
            protocol_version: GLASS_PROTOCOL_VERSION,
            request_id: "validate-task-1".into(),
            correlation_id: None,
            session_id: None,
            mutation_lease: None,
            operation: TASK_VALIDATE_OPERATION.into(),
            payload: serde_json::json!({"task": task}),
            deadline_ms: None,
        };
        let result = validate_task_result(&request).unwrap();
        assert_eq!(
            result,
            TaskValidationResult {
                valid: true,
                schema_version: 1,
                task: crate::task_protocol::TaskKind::RegionExtract,
            }
        );
        let response = GlassResponse {
            protocol_version: GLASS_PROTOCOL_VERSION,
            request_id: request.request_id.clone(),
            correlation_id: None,
            ok: true,
            result: Some(serde_json::to_value(&result).unwrap()),
            error: None,
        };
        assert_eq!(response.decode_task_validation_result().unwrap(), result);
        assert_eq!(
            request.decode_task_validate().unwrap().task.task,
            crate::task_protocol::TaskKind::RegionExtract
        );
    }

    #[test]
    fn task_compile_boundary_decodes_and_compiles_without_browser_state() {
        let task = serde_json::json!({
            "schemaVersion": 1,
            "task": "region.extract",
            "scope": {"regionName": "Checkout"},
            "limits": {"maxActions": 8, "timeoutMs": 5000, "maxItems": 32},
            "risk": "readOnly"
        });
        let request = GlassRequest {
            protocol_version: GLASS_PROTOCOL_VERSION,
            request_id: "compile-1".into(),
            correlation_id: None,
            session_id: None,
            mutation_lease: None,
            operation: TASK_COMPILE_OPERATION.into(),
            payload: serde_json::json!({"task": task}),
            deadline_ms: None,
        };
        let plan = compile_task_request(&request).unwrap();
        assert_eq!(plan.task, crate::task_protocol::TaskKind::RegionExtract);
        assert_eq!(plan.scope.region_name.as_deref(), Some("Checkout"));
        assert_eq!(plan.limits.max_actions, 8);
        assert_eq!(
            plan.revision,
            crate::task_protocol::TaskRevisionPolicy::Exact
        );

        let mut wrong_operation = request.clone();
        wrong_operation.operation = "browser.observe".into();
        assert!(wrong_operation.decode_task_compile().is_err());

        let mut unknown = request.clone();
        unknown.payload["futureField"] = true.into();
        assert!(unknown.decode_task_compile().is_err());

        let mut invalid = request;
        invalid.payload["task"]["task"] = "form.fill".into();
        assert!(compile_task_request(&invalid).is_err());
    }

    #[test]
    fn task_compile_result_round_trips_through_success_response() {
        let request = GlassRequest {
            protocol_version: GLASS_PROTOCOL_VERSION,
            request_id: "compile-2".into(),
            correlation_id: None,
            session_id: None,
            mutation_lease: None,
            operation: TASK_COMPILE_OPERATION.into(),
            payload: serde_json::json!({
                "task": {
                    "schemaVersion": 1,
                    "task": "field.read",
                    "scope": {"entityKind": "field", "entityName": "Email"},
                    "limits": {"maxActions": 4, "timeoutMs": 2000, "maxItems": 1},
                    "risk": "readOnly"
                }
            }),
            deadline_ms: None,
        };
        let result = compile_task_result(&request).unwrap();
        let response = GlassResponse {
            protocol_version: GLASS_PROTOCOL_VERSION,
            request_id: request.request_id.clone(),
            correlation_id: None,
            ok: true,
            result: Some(serde_json::to_value(&result).unwrap()),
            error: None,
        };
        assert_eq!(response.decode_task_compile_result().unwrap(), result);

        let mut unknown = response.clone();
        unknown.result.as_mut().unwrap()["futureField"] = true.into();
        assert!(unknown.decode_task_compile_result().is_err());

        let mut failure = response;
        failure.ok = false;
        failure.result = None;
        failure.error = Some(GlassError {
            code: "task.invalid".into(),
            phase: ErrorPhase::Preflight,
            message: "invalid task".into(),
            mutation_possible: false,
            retry: RetryGuidance::default(),
            retryable: None,
            details: None,
        });
        assert!(failure.decode_task_compile_result().is_err());
    }

    #[test]
    fn additive_response_fields_are_tolerated() {
        let response: GlassResponse = serde_json::from_value(serde_json::json!({
            "protocolVersion": 1,
            "requestId": "request-1",
            "ok": false,
            "error": {
                "code": "target.stale",
                "message": "stale",
                "retryable": true,
                "future": "ignored"
            },
            "future": true
        }))
        .unwrap();
        assert_eq!(
            response.error.unwrap().retry.classification,
            RetryClassification::SafeAfterReobserve
        );
    }
}
