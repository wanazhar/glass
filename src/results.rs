//! Bounded, redacted result contracts and local diagnostic artifacts.
//!
//! Agent-facing responses default to [`crate::results::ResponseMode::Minimal`]. Detailed
//! evidence stays in a local result artifact and is referenced by ID.

use crate::protocol::{ErrorPhase, RetryGuidance};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::SystemTime;

pub const RESULT_SCHEMA_VERSION: u32 = 1;
const MAX_RESULT_ID_BYTES: usize = 96;
const MAX_RESULT_BYTES: usize = 512 * 1024;
const MAX_DETAIL_BYTES: usize = 256 * 1024;
static RESULT_SEQUENCE: AtomicU64 = AtomicU64::new(1);

/// Agent-facing result detail level.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, clap::ValueEnum, Default)]
#[serde(rename_all = "lowercase")]
pub enum ResponseMode {
    #[default]
    Minimal,
    Normal,
    Diagnostic,
}

/// Whether detailed evidence can be retrieved locally.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DetailAvailability {
    pub result_id: String,
    pub details_available: bool,
    pub details_truncated: bool,
}

/// Canonical operation result before a response mode projection.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OperationResult {
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub execution_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub revision: Option<u64>,
    pub phase: ErrorPhase,
    pub mutation_possible: bool,
    pub retry: RetryGuidance,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub effect: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub verification: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub policy: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<Value>,
    pub detail_availability: DetailAvailability,
}

impl OperationResult {
    /// Project the same result into the requested bounded response mode.
    pub fn project(&self, mode: ResponseMode) -> Value {
        let mut output = Map::new();
        for key in [
            "status",
            "executionId",
            "revision",
            "phase",
            "mutationPossible",
            "retry",
            "effect",
            "verification",
        ] {
            if let Some(value) = serde_json::to_value(self)
                .ok()
                .and_then(|value| value.get(key).cloned())
            {
                output.insert(key.into(), value);
            }
        }
        if matches!(mode, ResponseMode::Normal | ResponseMode::Diagnostic) {
            for key in ["target", "policy", "detailAvailability"] {
                if let Some(value) = serde_json::to_value(self)
                    .ok()
                    .and_then(|value| value.get(key).cloned())
                {
                    output.insert(key.into(), value);
                }
            }
        }
        if mode == ResponseMode::Diagnostic
            && let Some(details) = &self.details
        {
            output.insert("details".into(), details.clone());
        }
        Value::Object(output)
    }
}

/// Project one serialized result into the bounded agent-facing response modes.
///
/// The complete value is retained by [`ResultStore`]; minimal and normal
/// projections omit only diagnostic-heavy fields and advertise the artifact
/// through `detailAvailability`.
pub fn project_value(mut value: Value, mode: ResponseMode) -> Value {
    if mode != ResponseMode::Diagnostic
        && let Value::Object(object) = &mut value
    {
        object.remove("details");
        if mode == ResponseMode::Minimal {
            for key in ["trace", "diagnostics", "screenshot", "dom", "formValues"] {
                object.remove(key);
            }
        }
    }
    value
}

/// Persist complete details and return the selected bounded projection.
pub fn project_and_store(
    value: Value,
    mode: ResponseMode,
    prefix: &str,
    root: impl Into<PathBuf>,
) -> Result<Value, ResultStoreError> {
    let store = ResultStore::new(root);
    let result_id = ResultStore::next_id(prefix)?;
    let availability = store.save(&result_id, &value)?;
    let mut projected = project_value(value, mode);
    if let Value::Object(object) = &mut projected {
        object.insert(
            "detailAvailability".into(),
            serde_json::to_value(availability)?,
        );
    } else {
        projected = serde_json::json!({
            "result": projected,
            "detailAvailability": availability,
        });
    }
    Ok(projected)
}

/// Stored diagnostic result with a versioned, redacted payload.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResultArtifact {
    pub schema_version: u32,
    pub result_id: String,
    pub created_at: String,
    pub details_truncated: bool,
    pub details: Value,
}

/// Local bounded result artifact store.
#[derive(Debug, Clone)]
pub struct ResultStore {
    root: PathBuf,
    max_bytes: usize,
}

impl ResultStore {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self {
            root: root.into(),
            max_bytes: MAX_RESULT_BYTES,
        }
    }

    pub fn with_max_bytes(mut self, max_bytes: usize) -> Self {
        self.max_bytes = max_bytes.max(1024);
        self
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn next_id(prefix: &str) -> Result<String, ResultStoreError> {
        let id = format!(
            "{}_{}_{}",
            sanitize_id(prefix)?,
            Utc::now().timestamp_millis(),
            RESULT_SEQUENCE.fetch_add(1, Ordering::Relaxed)
        );
        validate_id(&id)?;
        Ok(id)
    }

    pub fn save(
        &self,
        result_id: &str,
        details: &Value,
    ) -> Result<DetailAvailability, ResultStoreError> {
        validate_id(result_id)?;
        std::fs::create_dir_all(&self.root)?;
        let redacted = redact_value(details);
        let raw = serde_json::to_vec(&redacted)?;
        let (details, details_truncated) = if raw.len() > MAX_DETAIL_BYTES {
            (
                Value::String(String::from("diagnostic details truncated")),
                true,
            )
        } else {
            (redacted, false)
        };
        let artifact = ResultArtifact {
            schema_version: RESULT_SCHEMA_VERSION,
            result_id: result_id.to_string(),
            created_at: Utc::now().to_rfc3339(),
            details_truncated,
            details,
        };
        let bytes = serde_json::to_vec(&artifact)?;
        if bytes.len() > self.max_bytes {
            return Err(ResultStoreError::TooLarge(bytes.len(), self.max_bytes));
        }
        let destination = self.path_for(result_id);
        let temporary = destination.with_extension("json.tmp");
        std::fs::write(&temporary, bytes)?;
        std::fs::rename(&temporary, &destination)?;
        Ok(DetailAvailability {
            result_id: result_id.to_string(),
            details_available: true,
            details_truncated,
        })
    }

    pub fn load(&self, result_id: &str) -> Result<ResultArtifact, ResultStoreError> {
        validate_id(result_id)?;
        let artifact: ResultArtifact =
            serde_json::from_slice(&std::fs::read(self.path_for(result_id))?)?;
        if artifact.schema_version != RESULT_SCHEMA_VERSION || artifact.result_id != result_id {
            return Err(ResultStoreError::InvalidArtifact(result_id.to_string()));
        }
        Ok(artifact)
    }

    pub fn purge_older_than(&self, age: std::time::Duration) -> Result<usize, ResultStoreError> {
        if !self.root.exists() {
            return Ok(0);
        }
        let cutoff = SystemTime::now()
            .checked_sub(age)
            .unwrap_or(SystemTime::UNIX_EPOCH);
        let mut removed = 0;
        for entry in std::fs::read_dir(&self.root)? {
            let entry = entry?;
            if entry.path().extension().and_then(|value| value.to_str()) != Some("json") {
                continue;
            }
            if entry.metadata()?.modified().unwrap_or(SystemTime::now()) < cutoff {
                std::fs::remove_file(entry.path())?;
                removed += 1;
            }
        }
        Ok(removed)
    }

    fn path_for(&self, result_id: &str) -> PathBuf {
        self.root.join(format!("{result_id}.json"))
    }
}

/// Default local diagnostic artifact directory.
pub fn default_result_store_path() -> PathBuf {
    dirs::cache_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join("glass")
        .join("results")
}

#[derive(Debug)]
pub enum ResultStoreError {
    InvalidId,
    InvalidArtifact(String),
    TooLarge(usize, usize),
    Io(std::io::Error),
    Json(serde_json::Error),
}

impl std::fmt::Display for ResultStoreError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidId => formatter.write_str("invalid result ID"),
            Self::InvalidArtifact(id) => write!(formatter, "result artifact is invalid: {id}"),
            Self::TooLarge(actual, limit) => write!(
                formatter,
                "result artifact is {actual} bytes, exceeding the {limit}-byte budget"
            ),
            Self::Io(error) => error.fmt(formatter),
            Self::Json(error) => error.fmt(formatter),
        }
    }
}

impl std::error::Error for ResultStoreError {}

impl From<std::io::Error> for ResultStoreError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
    }
}

impl From<serde_json::Error> for ResultStoreError {
    fn from(error: serde_json::Error) -> Self {
        Self::Json(error)
    }
}

fn sanitize_id(value: &str) -> Result<String, ResultStoreError> {
    if value.is_empty() || value.len() > MAX_RESULT_ID_BYTES {
        return Err(ResultStoreError::InvalidId);
    }
    let sanitized = value
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || character == '-' {
                character
            } else {
                '_'
            }
        })
        .collect::<String>();
    validate_id(&sanitized)?;
    Ok(sanitized)
}

fn validate_id(value: &str) -> Result<(), ResultStoreError> {
    if value.is_empty()
        || value.len() > MAX_RESULT_ID_BYTES
        || value == "."
        || value == ".."
        || value.contains('/')
        || value.contains('\\')
        || value.chars().any(char::is_whitespace)
    {
        return Err(ResultStoreError::InvalidId);
    }
    Ok(())
}

fn redact_value(value: &Value) -> Value {
    match value {
        Value::Object(object) => {
            let mut result = Map::new();
            for (key, value) in object {
                if is_sensitive_key(key) {
                    result.insert(key.clone(), Value::String("[REDACTED]".into()));
                } else {
                    result.insert(key.clone(), redact_value(value));
                }
            }
            Value::Object(result)
        }
        Value::Array(values) => Value::Array(values.iter().map(redact_value).collect()),
        other => other.clone(),
    }
}

fn is_sensitive_key(key: &str) -> bool {
    let key = key.to_ascii_lowercase();
    [
        "cookie",
        "cookies",
        "authorization",
        "headers",
        "password",
        "payment",
        "typedformvalues",
        "formvalues",
        "screenshot",
        "dom",
        "console",
        "network",
    ]
    .iter()
    .any(|sensitive| key.contains(sensitive))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn response_modes_preserve_recovery_fields_and_bound_details() {
        let result = OperationResult {
            status: "indeterminate".into(),
            execution_id: Some("run_1".into()),
            revision: Some(4),
            phase: ErrorPhase::PostDispatch,
            mutation_possible: true,
            retry: RetryGuidance {
                classification: crate::protocol::RetryClassification::UnsafeUntilReconciled,
                recommended_operation: "recover_run".into(),
            },
            effect: Some(serde_json::json!({"observed": false})),
            verification: None,
            target: Some(serde_json::json!({"id": "submit"})),
            policy: Some(serde_json::json!({"allowed": true})),
            details: Some(serde_json::json!({"trace": [1, 2, 3]})),
            detail_availability: DetailAvailability {
                result_id: "run_1".into(),
                details_available: true,
                details_truncated: false,
            },
        };
        assert!(result.project(ResponseMode::Minimal)["retry"].is_object());
        assert!(
            result
                .project(ResponseMode::Minimal)
                .get("details")
                .is_none()
        );
        assert!(result.project(ResponseMode::Normal)["target"].is_object());
        assert!(result.project(ResponseMode::Diagnostic)["details"].is_object());
    }

    #[test]
    fn result_store_redacts_and_round_trips_atomically() {
        let root = std::env::temp_dir().join(format!("glass-results-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let store = ResultStore::new(&root);
        let availability = store
            .save(
                "run_1",
                &serde_json::json!({"cookie": "secret", "nested": {"password": "secret"}, "value": 3}),
            )
            .unwrap();
        assert!(availability.details_available);
        let artifact = store.load("run_1").unwrap();
        assert_eq!(artifact.details["cookie"], "[REDACTED]");
        assert_eq!(artifact.details["nested"]["password"], "[REDACTED]");
        assert_eq!(artifact.details["value"], 3);
        let _ = std::fs::remove_dir_all(root);
    }
}
