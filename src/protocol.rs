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
}

impl std::fmt::Display for ProtocolError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnsupportedVersion(version) => {
                write!(formatter, "unsupported Glass protocol version {version}")
            }
            Self::InvalidField(detail) => formatter.write_str(detail),
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
