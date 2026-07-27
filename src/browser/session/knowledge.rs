//! Versioned, bounded browser knowledge records.
//!
//! Knowledge is an optimization and an explanation surface. A record may help
//! recognize a recurring page or reduce inspection work, but it never contains
//! an executable target reference and never authorizes a browser mutation.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::fmt;

pub const KNOWLEDGE_SCHEMA_VERSION: u32 = 1;
pub const MAX_KNOWLEDGE_RECORDS: usize = 256;
const MAX_RECORD_ID_BYTES: usize = 128;
const MAX_SCOPE_VALUE_BYTES: usize = 256;
const MAX_TIMESTAMP_BYTES: usize = 64;
const MAX_LANDMARKS: usize = 32;
const MAX_HISTORY: usize = 32;
const MAX_DATA_BYTES: usize = 16 * 1024;
const MAX_RECORD_BYTES: usize = 64 * 1024;
const MAX_JSON_DEPTH: usize = 8;
const MAX_JSON_OBJECT_ENTRIES: usize = 64;
const MAX_JSON_ARRAY_ENTRIES: usize = 64;
const MAX_JSON_STRING_BYTES: usize = 4096;

/// Knowledge record categories supported by the versioned store contract.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum KnowledgeRecordKind {
    PageFamily,
    RegionModel,
    TargetFingerprint,
    RouteTransition,
    WorkflowEntryPoint,
    VerifiedPostcondition,
    ExtractionShape,
    InvalidationRule,
}

/// Confidence and lifecycle state of one knowledge record.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum KnowledgeConfidence {
    Candidate,
    Observed,
    Verified,
    Stale,
    Contradicted,
    Quarantined,
}

/// Scope dimensions that prevent knowledge from crossing incompatible sessions.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct KnowledgeScope {
    pub origin: String,
    pub path_pattern: String,
    pub profile_scope: KnowledgeProfileScope,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub profile_key: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub locale: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tenant_key: Option<String>,
    pub browser_family: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub browser_version_range: Option<String>,
    pub glass_schema_version: u32,
    pub policy_preset: String,
}

/// Whether a record is anonymous, generally authenticated, or bound to one
/// caller-selected profile identity class.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum KnowledgeProfileScope {
    Anonymous,
    Authenticated,
    ProfileBound,
}

/// Provenance and verification counters for a knowledge record.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct KnowledgeSource {
    pub first_seen_at: String,
    pub last_verified_at: String,
    pub glass_version: String,
    pub verification_count: u32,
}

/// Conditions that make remembered knowledge stale or unusable.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct KnowledgeInvalidation {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_age_seconds: Option<u64>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub required_landmarks: Vec<String>,
}

/// One bounded, auditable lifecycle transition.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct KnowledgeLifecycleEvent {
    pub from: KnowledgeConfidence,
    pub to: KnowledgeConfidence,
    pub reason: String,
    pub observed_at: String,
}

/// A versioned knowledge record. `data` is deliberately opaque to this layer,
/// but its shape, size, and sensitive field names are strictly bounded.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct KnowledgeRecord {
    pub schema_version: u32,
    pub record_id: String,
    pub kind: KnowledgeRecordKind,
    pub scope: KnowledgeScope,
    pub source: KnowledgeSource,
    pub confidence: KnowledgeConfidence,
    pub invalidation: KnowledgeInvalidation,
    pub data: Value,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub history: Vec<KnowledgeLifecycleEvent>,
}

impl KnowledgeRecord {
    /// Validate one record against the stable contract and all payload bounds.
    pub fn validate(&self) -> Result<(), KnowledgeValidationError> {
        if self.schema_version != KNOWLEDGE_SCHEMA_VERSION {
            return Err(KnowledgeValidationError::new(
                "schemaVersion",
                format!(
                    "unsupported schema version {}; expected {}",
                    self.schema_version, KNOWLEDGE_SCHEMA_VERSION
                ),
            ));
        }
        validate_text("recordId", &self.record_id, MAX_RECORD_ID_BYTES, false)?;
        validate_scope(&self.scope)?;
        validate_source(&self.source)?;
        validate_invalidation(&self.invalidation)?;
        if self.history.len() > MAX_HISTORY {
            return Err(KnowledgeValidationError::new(
                "history",
                format!("contains more than {MAX_HISTORY} events"),
            ));
        }
        for (index, event) in self.history.iter().enumerate() {
            validate_text(
                &format!("history[{index}].reason"),
                &event.reason,
                MAX_SCOPE_VALUE_BYTES,
                false,
            )?;
            validate_text(
                &format!("history[{index}].observedAt"),
                &event.observed_at,
                MAX_TIMESTAMP_BYTES,
                false,
            )?;
        }
        validate_json_value("data", &self.data, 0)?;
        let data_bytes = serde_json::to_vec(&self.data).map_err(|error| {
            KnowledgeValidationError::new("data", format!("cannot serialize data: {error}"))
        })?;
        if data_bytes.len() > MAX_DATA_BYTES {
            return Err(KnowledgeValidationError::new(
                "data",
                format!("exceeds the {MAX_DATA_BYTES}-byte limit"),
            ));
        }
        let record_bytes = serde_json::to_vec(self).map_err(|error| {
            KnowledgeValidationError::new("$", format!("cannot serialize record: {error}"))
        })?;
        if record_bytes.len() > MAX_RECORD_BYTES {
            return Err(KnowledgeValidationError::new(
                "$",
                format!("record exceeds the {MAX_RECORD_BYTES}-byte limit"),
            ));
        }
        Ok(())
    }

    /// Serialize a validated record with deterministic object-key ordering.
    pub fn to_canonical_json(&self) -> Result<String, KnowledgeValidationError> {
        self.validate()?;
        serde_json::to_string(self)
            .map_err(|error| KnowledgeValidationError::new("$", error.to_string()))
    }

    /// Hash the canonical record for integrity checks and stable comparisons.
    pub fn content_hash(&self) -> Result<String, KnowledgeValidationError> {
        let canonical = self.to_canonical_json()?;
        let digest = Sha256::digest(canonical.as_bytes());
        Ok(format!("sha256:{digest:x}"))
    }

    /// Apply a lifecycle transition. Promotion to `verified` and recovery from
    /// contradiction/quarantine require fresh verification evidence.
    pub fn transition(
        &mut self,
        next: KnowledgeConfidence,
        reason: String,
        observed_at: String,
        fresh_verification: bool,
    ) -> Result<(), KnowledgeValidationError> {
        validate_text("reason", &reason, MAX_SCOPE_VALUE_BYTES, false)?;
        validate_text("observedAt", &observed_at, MAX_TIMESTAMP_BYTES, false)?;
        if self.confidence == next {
            return Ok(());
        }
        let requires_fresh = matches!(
            next,
            KnowledgeConfidence::Verified | KnowledgeConfidence::Observed
        ) || matches!(
            self.confidence,
            KnowledgeConfidence::Contradicted | KnowledgeConfidence::Quarantined
        );
        if requires_fresh && !fresh_verification {
            return Err(KnowledgeValidationError::new(
                "freshVerification",
                "this lifecycle transition requires fresh browser verification",
            ));
        }
        if self.history.len() >= MAX_HISTORY {
            return Err(KnowledgeValidationError::new(
                "history",
                format!("cannot append beyond {MAX_HISTORY} events"),
            ));
        }
        let event = KnowledgeLifecycleEvent {
            from: self.confidence,
            to: next,
            reason,
            observed_at,
        };
        self.confidence = next;
        self.history.push(event);
        self.validate()
    }
}

/// The persisted top-level store document.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct KnowledgeStoreSnapshot {
    pub schema_version: u32,
    pub records: Vec<KnowledgeRecord>,
}

impl KnowledgeStoreSnapshot {
    pub fn validate(&self) -> Result<(), KnowledgeValidationError> {
        if self.schema_version != KNOWLEDGE_SCHEMA_VERSION {
            return Err(KnowledgeValidationError::new(
                "schemaVersion",
                format!(
                    "unsupported schema version {}; expected {}",
                    self.schema_version, KNOWLEDGE_SCHEMA_VERSION
                ),
            ));
        }
        if self.records.len() > MAX_KNOWLEDGE_RECORDS {
            return Err(KnowledgeValidationError::new(
                "records",
                format!("contains more than {MAX_KNOWLEDGE_RECORDS} records"),
            ));
        }
        let mut record_ids = BTreeSet::new();
        for (index, record) in self.records.iter().enumerate() {
            record.validate().map_err(|error| error.at(index))?;
            if !record_ids.insert(record.record_id.as_str()) {
                return Err(KnowledgeValidationError::new(
                    format!("records[{index}].recordId"),
                    "record ID is duplicated",
                ));
            }
        }
        Ok(())
    }

    pub fn to_canonical_json(&self) -> Result<String, KnowledgeValidationError> {
        self.validate()?;
        serde_json::to_string(self)
            .map_err(|error| KnowledgeValidationError::new("$", error.to_string()))
    }
}

fn validate_scope(scope: &KnowledgeScope) -> Result<(), KnowledgeValidationError> {
    validate_text("scope.origin", &scope.origin, 2048, false)?;
    validate_text("scope.pathPattern", &scope.path_pattern, 512, false)?;
    validate_text(
        "scope.browserFamily",
        &scope.browser_family,
        MAX_SCOPE_VALUE_BYTES,
        false,
    )?;
    validate_text(
        "scope.policyPreset",
        &scope.policy_preset,
        MAX_SCOPE_VALUE_BYTES,
        false,
    )?;
    if scope.glass_schema_version == 0 {
        return Err(KnowledgeValidationError::new(
            "scope.glassSchemaVersion",
            "must be positive",
        ));
    }
    for (path, value) in [
        ("scope.profileKey", scope.profile_key.as_deref()),
        ("scope.locale", scope.locale.as_deref()),
        ("scope.tenantKey", scope.tenant_key.as_deref()),
        (
            "scope.browserVersionRange",
            scope.browser_version_range.as_deref(),
        ),
    ] {
        if let Some(value) = value {
            validate_text(path, value, MAX_SCOPE_VALUE_BYTES, false)?;
        }
    }
    match scope.profile_scope {
        KnowledgeProfileScope::ProfileBound if scope.profile_key.is_none() => {
            Err(KnowledgeValidationError::new(
                "scope.profileKey",
                "is required for profileBound knowledge",
            ))
        }
        KnowledgeProfileScope::Anonymous if scope.profile_key.is_some() => {
            Err(KnowledgeValidationError::new(
                "scope.profileKey",
                "must be absent for anonymous knowledge",
            ))
        }
        _ => Ok(()),
    }
}

fn validate_source(source: &KnowledgeSource) -> Result<(), KnowledgeValidationError> {
    validate_text(
        "source.firstSeenAt",
        &source.first_seen_at,
        MAX_TIMESTAMP_BYTES,
        false,
    )?;
    validate_text(
        "source.lastVerifiedAt",
        &source.last_verified_at,
        MAX_TIMESTAMP_BYTES,
        false,
    )?;
    validate_text(
        "source.glassVersion",
        &source.glass_version,
        MAX_SCOPE_VALUE_BYTES,
        false,
    )
}

fn validate_invalidation(
    invalidation: &KnowledgeInvalidation,
) -> Result<(), KnowledgeValidationError> {
    if invalidation.required_landmarks.len() > MAX_LANDMARKS {
        return Err(KnowledgeValidationError::new(
            "invalidation.requiredLandmarks",
            format!("contains more than {MAX_LANDMARKS} landmarks"),
        ));
    }
    let mut landmarks = BTreeSet::new();
    for (index, landmark) in invalidation.required_landmarks.iter().enumerate() {
        validate_text(
            &format!("invalidation.requiredLandmarks[{index}]"),
            landmark,
            MAX_SCOPE_VALUE_BYTES,
            false,
        )?;
        if !landmarks.insert(landmark) {
            return Err(KnowledgeValidationError::new(
                format!("invalidation.requiredLandmarks[{index}]"),
                "landmark is duplicated",
            ));
        }
    }
    Ok(())
}

fn validate_json_value(
    path: &str,
    value: &Value,
    depth: usize,
) -> Result<(), KnowledgeValidationError> {
    if depth > MAX_JSON_DEPTH {
        return Err(KnowledgeValidationError::new(
            path,
            format!("exceeds the {MAX_JSON_DEPTH}-level nesting limit"),
        ));
    }
    match value {
        Value::Null | Value::Bool(_) | Value::Number(_) => Ok(()),
        Value::String(value) => validate_text(path, value, MAX_JSON_STRING_BYTES, true),
        Value::Array(values) => {
            if values.len() > MAX_JSON_ARRAY_ENTRIES {
                return Err(KnowledgeValidationError::new(
                    path,
                    format!("contains more than {MAX_JSON_ARRAY_ENTRIES} values"),
                ));
            }
            for (index, value) in values.iter().enumerate() {
                validate_json_value(&format!("{path}[{index}]"), value, depth + 1)?;
            }
            Ok(())
        }
        Value::Object(values) => {
            if values.len() > MAX_JSON_OBJECT_ENTRIES {
                return Err(KnowledgeValidationError::new(
                    path,
                    format!("contains more than {MAX_JSON_OBJECT_ENTRIES} fields"),
                ));
            }
            for (key, value) in values {
                validate_key(&format!("{path}.{key}"), key)?;
                validate_json_value(&format!("{path}.{key}"), value, depth + 1)?;
            }
            Ok(())
        }
    }
}

fn validate_key(path: &str, key: &str) -> Result<(), KnowledgeValidationError> {
    validate_text(path, key, MAX_SCOPE_VALUE_BYTES, false)?;
    let normalized = key
        .chars()
        .filter(char::is_ascii_alphanumeric)
        .collect::<String>()
        .to_ascii_lowercase();
    const FORBIDDEN: &[&str] = &[
        "authorization",
        "cookie",
        "credential",
        "formvalue",
        "password",
        "rawaccessibility",
        "rawcdp",
        "rawdom",
        "secret",
        "screenshot",
        "token",
    ];
    if FORBIDDEN.iter().any(|word| normalized.contains(word)) {
        return Err(KnowledgeValidationError::new(
            path,
            "field name is not permitted in persistent knowledge",
        ));
    }
    Ok(())
}

fn validate_text(
    path: &str,
    value: &str,
    maximum: usize,
    allow_empty: bool,
) -> Result<(), KnowledgeValidationError> {
    if (!allow_empty && value.is_empty()) || value.len() > maximum {
        let requirement = if allow_empty {
            format!("at most {maximum} bytes")
        } else {
            format!("non-empty and at most {maximum} bytes")
        };
        return Err(KnowledgeValidationError::new(
            path,
            format!("must be {requirement}"),
        ));
    }
    Ok(())
}

/// Path-aware knowledge contract validation failure.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct KnowledgeValidationError {
    pub path: String,
    pub reason: String,
}

impl KnowledgeValidationError {
    fn new(path: impl Into<String>, reason: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            reason: reason.into(),
        }
    }

    fn at(self, index: usize) -> Self {
        Self::new(format!("records[{index}].{}", self.path), self.reason)
    }
}

impl fmt::Display for KnowledgeValidationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}: {}", self.path, self.reason)
    }
}

impl std::error::Error for KnowledgeValidationError {}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn record() -> KnowledgeRecord {
        KnowledgeRecord {
            schema_version: KNOWLEDGE_SCHEMA_VERSION,
            record_id: "knowledge_docs_1".into(),
            kind: KnowledgeRecordKind::PageFamily,
            scope: KnowledgeScope {
                origin: "https://example.test".into(),
                path_pattern: "/docs/*".into(),
                profile_scope: KnowledgeProfileScope::Anonymous,
                profile_key: None,
                locale: Some("en-US".into()),
                tenant_key: None,
                browser_family: "chromium".into(),
                browser_version_range: Some(">=120".into()),
                glass_schema_version: 1,
                policy_preset: "balanced".into(),
            },
            source: KnowledgeSource {
                first_seen_at: "2026-07-27T00:00:00Z".into(),
                last_verified_at: "2026-07-27T00:00:00Z".into(),
                glass_version: "0.2.0".into(),
                verification_count: 1,
            },
            confidence: KnowledgeConfidence::Observed,
            invalidation: KnowledgeInvalidation {
                max_age_seconds: Some(604_800),
                required_landmarks: vec!["main".into(), "search".into()],
            },
            data: json!({"pageKind": "documentation", "regions": ["main", "search"]}),
            history: Vec::new(),
        }
    }

    #[test]
    fn record_round_trip_and_hash_are_deterministic() {
        let record = record();
        let canonical = record.to_canonical_json().unwrap();
        let parsed: KnowledgeRecord = serde_json::from_str(&canonical).unwrap();
        assert_eq!(parsed, record);
        assert_eq!(
            record.content_hash().unwrap(),
            parsed.content_hash().unwrap()
        );
    }

    #[test]
    fn sensitive_data_keys_are_rejected() {
        let mut record = record();
        record.data = json!({"requiredLandmarks": ["main"], "password": "never"});
        let error = record.validate().unwrap_err();
        assert_eq!(error.path, "data.password");
    }

    #[test]
    fn profile_bound_scope_requires_a_profile_key() {
        let mut record = record();
        record.scope.profile_scope = KnowledgeProfileScope::ProfileBound;
        let error = record.validate().unwrap_err();
        assert_eq!(error.path, "scope.profileKey");
    }

    #[test]
    fn verified_promotion_requires_fresh_evidence() {
        let mut record = record();
        let error = record
            .transition(
                KnowledgeConfidence::Verified,
                "imported record".into(),
                "2026-07-27T00:00:01Z".into(),
                false,
            )
            .unwrap_err();
        assert_eq!(error.path, "freshVerification");
        record
            .transition(
                KnowledgeConfidence::Verified,
                "fresh landmark match".into(),
                "2026-07-27T00:00:01Z".into(),
                true,
            )
            .unwrap();
        assert_eq!(record.confidence, KnowledgeConfidence::Verified);
        assert_eq!(record.history.len(), 1);
    }

    #[test]
    fn snapshot_rejects_duplicate_record_ids() {
        let record = record();
        let snapshot = KnowledgeStoreSnapshot {
            schema_version: KNOWLEDGE_SCHEMA_VERSION,
            records: vec![record.clone(), record],
        };
        let error = snapshot.validate().unwrap_err();
        assert_eq!(error.path, "records[1].recordId");
    }
}
