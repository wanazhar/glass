//! Versioned, bounded browser knowledge records.
//!
//! Knowledge is an optimization and an explanation surface. A record may help
//! recognize a recurring page or reduce inspection work, but it never contains
//! an executable target reference and never authorizes a browser mutation.

use super::{SemanticIntentCandidate, SemanticObservation, target_fingerprint_digest};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::fmt;
use url::Url;

pub const KNOWLEDGE_SCHEMA_VERSION: u32 = 1;
pub const MAX_KNOWLEDGE_RECORDS: usize = 256;
const MAX_RECORD_ID_BYTES: usize = 128;
const MAX_SCOPE_VALUE_BYTES: usize = 256;
const MAX_ROLE_BYTES: usize = 64;
const MAX_TIMESTAMP_BYTES: usize = 64;
const MAX_LANDMARKS: usize = 32;
const MAX_HISTORY: usize = 32;
const MAX_DATA_BYTES: usize = 16 * 1024;
const MAX_RECORD_BYTES: usize = 64 * 1024;
const MAX_JSON_DEPTH: usize = 8;
const MAX_JSON_OBJECT_ENTRIES: usize = 64;
const MAX_JSON_ARRAY_ENTRIES: usize = 64;
const MAX_JSON_STRING_BYTES: usize = 4096;

/// Current browser/session dimensions used to assess one stored record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KnowledgeLookupContext {
    pub origin: String,
    pub path: String,
    pub profile_scope: KnowledgeProfileScope,
    pub profile_key: Option<String>,
    pub locale: Option<String>,
    pub tenant_key: Option<String>,
    pub browser_family: String,
    pub browser_version: Option<String>,
    pub glass_schema_version: u32,
    pub policy_preset: String,
    pub landmarks: Vec<String>,
    pub now_epoch_seconds: i64,
}

/// Explicit session inputs used to construct a lookup context from an
/// observation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KnowledgeLookupOptions {
    pub profile_scope: KnowledgeProfileScope,
    pub profile_key: Option<String>,
    pub locale: Option<String>,
    pub tenant_key: Option<String>,
    pub browser_family: String,
    pub browser_version: Option<String>,
    pub glass_schema_version: u32,
    pub policy_preset: String,
    pub now_epoch_seconds: i64,
}

/// Inputs for creating one page-family record from fresh semantic evidence.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KnowledgeRecordBuildOptions {
    pub record_id: String,
    pub scope: KnowledgeScope,
    pub glass_version: String,
    pub observed_at: String,
}

impl KnowledgeLookupContext {
    /// Build matching dimensions from a semantic observation and explicit
    /// session scope. Current element references are never retained here.
    pub fn from_observation(
        observation: &SemanticObservation,
        options: KnowledgeLookupOptions,
    ) -> Result<Self, KnowledgeValidationError> {
        let url = Url::parse(&observation.route.url).map_err(|error| {
            KnowledgeValidationError::new("observation.route.url", format!("invalid URL: {error}"))
        })?;
        let origin = url.origin().ascii_serialization();
        let path = if url.path().is_empty() {
            "/".to_string()
        } else {
            url.path().to_string()
        };
        let mut landmarks = BTreeSet::new();
        landmarks.insert(
            serde_json::to_string(&observation.page.kind).map_err(|error| {
                KnowledgeValidationError::new("observation.page.kind", error.to_string())
            })?,
        );
        for region in &observation.regions {
            landmarks.insert(serde_json::to_string(&region.kind).map_err(|error| {
                KnowledgeValidationError::new("observation.regions.kind", error.to_string())
            })?);
        }
        Ok(Self {
            origin,
            path,
            profile_scope: options.profile_scope,
            profile_key: options.profile_key,
            locale: options.locale,
            tenant_key: options.tenant_key,
            browser_family: options.browser_family,
            browser_version: options.browser_version,
            glass_schema_version: options.glass_schema_version,
            policy_preset: options.policy_preset,
            landmarks: landmarks
                .into_iter()
                .map(|landmark| landmark.trim_matches('"').to_string())
                .collect(),
            now_epoch_seconds: options.now_epoch_seconds,
        })
    }
}

/// Why one remembered record matched or failed to match current state.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum KnowledgeSignalKind {
    OriginMatch,
    PathMatch,
    ProfileScopeMatch,
    LocaleMatch,
    TenantMatch,
    BrowserMatch,
    SchemaMatch,
    PolicyMatch,
    LandmarkMatch,
    FreshnessMatch,
}

/// One bounded positive or negative assessment explanation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct KnowledgeAssessmentSignal {
    pub kind: KnowledgeSignalKind,
    pub detail: String,
}

/// Eligibility state after current scope, freshness, and landmark checks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum KnowledgeAssessmentStatus {
    Eligible,
    OutOfScope,
    Stale,
    Contradicted,
    Quarantined,
}

/// Fresh-state assessment of one stored record. This result contains no target
/// reference and cannot authorize a browser mutation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct KnowledgeAssessment {
    pub record_id: String,
    pub status: KnowledgeAssessmentStatus,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub signals: Vec<KnowledgeAssessmentSignal>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub conflicts: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub missing_landmarks: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub age_seconds: Option<i64>,
}

/// Whether a semantic observation consulted the local knowledge store.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum KnowledgeObservationMode {
    FreshOnly,
    Assessed,
}

/// Fresh semantic observation plus explicit, non-authorizing knowledge
/// assessment evidence.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct KnowledgeObservationReport {
    pub observation: SemanticObservation,
    pub mode: KnowledgeObservationMode,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub assessments: Vec<KnowledgeAssessment>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub eligible_record_ids: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub stale_record_ids: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub out_of_scope_record_ids: Vec<String>,
}

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
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
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
    /// Build an observed page-family record from fresh semantic structure.
    ///
    /// Only page/region kinds are retained. Current revisioned target
    /// references, labels, text, accessibility trees, and form values are not
    /// copied into persistent knowledge.
    pub fn from_page_observation(
        observation: &SemanticObservation,
        options: KnowledgeRecordBuildOptions,
    ) -> Result<Self, KnowledgeValidationError> {
        observation.validate().map_err(|error| {
            KnowledgeValidationError::new("observation", format!("invalid observation: {error}"))
        })?;
        validate_text("recordId", &options.record_id, MAX_RECORD_ID_BYTES, false)?;
        validate_text(
            "source.glassVersion",
            &options.glass_version,
            MAX_SCOPE_VALUE_BYTES,
            false,
        )?;
        validate_timestamp("source.observedAt", &options.observed_at)?;
        validate_scope(&options.scope)?;
        let page_kind = serde_json::to_value(observation.page.kind)
            .map_err(|error| KnowledgeValidationError::new("data.pageKind", error.to_string()))?;
        let region_kinds = observation
            .regions
            .iter()
            .map(|region| serde_json::to_value(region.kind))
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| {
                KnowledgeValidationError::new("data.regionKinds", error.to_string())
            })?;
        let mut required_landmarks = BTreeSet::new();
        required_landmarks.insert(page_kind.as_str().unwrap_or_default().to_owned());
        for kind in &region_kinds {
            if let Some(kind) = kind.as_str() {
                required_landmarks.insert(kind.to_owned());
            }
        }
        let record = Self {
            schema_version: KNOWLEDGE_SCHEMA_VERSION,
            record_id: options.record_id,
            kind: KnowledgeRecordKind::PageFamily,
            scope: options.scope,
            source: KnowledgeSource {
                first_seen_at: options.observed_at.clone(),
                last_verified_at: options.observed_at,
                glass_version: options.glass_version,
                verification_count: 0,
            },
            confidence: KnowledgeConfidence::Observed,
            invalidation: KnowledgeInvalidation {
                max_age_seconds: Some(604_800),
                required_landmarks: required_landmarks.into_iter().collect(),
            },
            data: json!({
                "pageKind": page_kind,
                "regionKinds": region_kinds,
            }),
            history: Vec::new(),
        };
        record.validate()?;
        Ok(record)
    }

    /// Build an observed target-fingerprint record from one fresh intent
    /// candidate. Only a digest and non-sensitive semantic dimensions are
    /// retained; the candidate's current reference and accessible name are
    /// never persisted.
    pub fn from_intent_candidate(
        candidate: &SemanticIntentCandidate,
        options: KnowledgeRecordBuildOptions,
    ) -> Result<Self, KnowledgeValidationError> {
        let fingerprint = candidate.fingerprint.as_ref().ok_or_else(|| {
            KnowledgeValidationError::new(
                "candidate.fingerprint",
                "fresh intent candidates require a target fingerprint",
            )
        })?;
        validate_text("recordId", &options.record_id, MAX_RECORD_ID_BYTES, false)?;
        validate_text(
            "source.glassVersion",
            &options.glass_version,
            MAX_SCOPE_VALUE_BYTES,
            false,
        )?;
        validate_timestamp("source.observedAt", &options.observed_at)?;
        validate_scope(&options.scope)?;
        validate_text("candidate.role", &candidate.role, MAX_ROLE_BYTES, false)?;
        let digest = target_fingerprint_digest(
            &candidate.role,
            &candidate.name,
            candidate.input_type.as_deref(),
            candidate.region_kind,
            fingerprint.purpose,
        );
        let required_landmarks = candidate
            .region_kind
            .map(|kind| serde_json::to_string(&kind).unwrap_or_default())
            .into_iter()
            .map(|landmark| landmark.trim_matches('"').to_string())
            .collect();
        let record = Self {
            schema_version: KNOWLEDGE_SCHEMA_VERSION,
            record_id: options.record_id,
            kind: KnowledgeRecordKind::TargetFingerprint,
            scope: options.scope,
            source: KnowledgeSource {
                first_seen_at: options.observed_at.clone(),
                last_verified_at: options.observed_at,
                glass_version: options.glass_version,
                verification_count: 0,
            },
            confidence: KnowledgeConfidence::Observed,
            invalidation: KnowledgeInvalidation {
                max_age_seconds: Some(604_800),
                required_landmarks,
            },
            data: json!({
                "fingerprint": digest,
                "role": candidate.role,
                "regionKind": candidate.region_kind,
                "purpose": fingerprint.purpose,
            }),
            history: Vec::new(),
        };
        record.validate()?;
        Ok(record)
    }

    /// Build a candidate workflow-entry record from a validated definition.
    /// Only hashed workflow identity, hashed step/output IDs, and bounded
    /// shape counts are retained; inputs, locators, predicates, and values are
    /// deliberately excluded.
    pub fn from_workflow_definition(
        workflow: &super::WorkflowDefinition,
        options: KnowledgeRecordBuildOptions,
    ) -> Result<Self, KnowledgeValidationError> {
        workflow.validate().map_err(|error| {
            KnowledgeValidationError::new("workflow", format!("invalid workflow: {error}"))
        })?;
        validate_text("recordId", &options.record_id, MAX_RECORD_ID_BYTES, false)?;
        validate_text(
            "source.glassVersion",
            &options.glass_version,
            MAX_SCOPE_VALUE_BYTES,
            false,
        )?;
        validate_timestamp("source.observedAt", &options.observed_at)?;
        validate_scope(&options.scope)?;
        let workflow_hash = hash_knowledge_identity(&[&workflow.name, &workflow.workflow_version]);
        let step_hashes: Vec<String> = workflow
            .steps
            .iter()
            .map(|step| hash_knowledge_identity(&[&step.id]))
            .collect();
        let output_hashes: Vec<String> = workflow
            .outputs
            .keys()
            .map(|key| hash_knowledge_identity(&[key]))
            .collect();
        let record = Self {
            schema_version: KNOWLEDGE_SCHEMA_VERSION,
            record_id: options.record_id,
            kind: KnowledgeRecordKind::WorkflowEntryPoint,
            scope: options.scope,
            source: KnowledgeSource {
                first_seen_at: options.observed_at.clone(),
                last_verified_at: options.observed_at,
                glass_version: options.glass_version,
                verification_count: 0,
            },
            confidence: KnowledgeConfidence::Candidate,
            invalidation: KnowledgeInvalidation {
                max_age_seconds: Some(604_800),
                required_landmarks: Vec::new(),
            },
            data: json!({
                "workflowHash": workflow_hash,
                "stepHashes": step_hashes,
                "outputHashes": output_hashes,
                "stepCount": workflow.steps.len(),
                "intentStepCount": workflow.steps.iter().filter(|step| step.intent.is_some()).count(),
                "postconditionCount": workflow.steps.iter().filter(|step| step.expect.is_some()).count() + 1,
            }),
            history: Vec::new(),
        };
        record.validate()?;
        Ok(record)
    }

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
            validate_timestamp(&format!("history[{index}].observedAt"), &event.observed_at)?;
        }
        if !matches!(self.data, Value::Object(_) | Value::Array(_)) {
            return Err(KnowledgeValidationError::new(
                "data",
                "must be a JSON object or array",
            ));
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

    /// Assess this record against current scope and fresh observation signals.
    /// A positive assessment never includes an executable target reference.
    pub fn assess(&self, context: &KnowledgeLookupContext) -> KnowledgeAssessment {
        let mut signals = Vec::new();
        let mut conflicts = Vec::new();
        let mut missing_landmarks = Vec::new();

        if self.scope.origin == context.origin {
            signals.push(signal(KnowledgeSignalKind::OriginMatch, "origin matches"));
        } else {
            conflicts.push("origin does not match".into());
        }
        if path_matches(&self.scope.path_pattern, &context.path) {
            signals.push(signal(
                KnowledgeSignalKind::PathMatch,
                "path pattern matches",
            ));
        } else {
            conflicts.push("path is outside the record pattern".into());
        }
        if self.scope.profile_scope == context.profile_scope
            && self.scope.profile_key == context.profile_key
        {
            signals.push(signal(
                KnowledgeSignalKind::ProfileScopeMatch,
                "profile scope matches",
            ));
        } else {
            conflicts.push("profile scope does not match".into());
        }
        compare_optional_scope(
            &mut signals,
            &mut conflicts,
            KnowledgeSignalKind::LocaleMatch,
            "locale",
            &self.scope.locale,
            &context.locale,
        );
        compare_optional_scope(
            &mut signals,
            &mut conflicts,
            KnowledgeSignalKind::TenantMatch,
            "tenant",
            &self.scope.tenant_key,
            &context.tenant_key,
        );
        if self.scope.browser_family == context.browser_family
            && browser_version_matches(
                self.scope.browser_version_range.as_deref(),
                context.browser_version.as_deref(),
            )
        {
            signals.push(signal(
                KnowledgeSignalKind::BrowserMatch,
                "browser scope matches",
            ));
        } else {
            conflicts.push("browser family or version does not match".into());
        }
        if self.scope.glass_schema_version == context.glass_schema_version {
            signals.push(signal(
                KnowledgeSignalKind::SchemaMatch,
                "schema scope matches",
            ));
        } else {
            conflicts.push("Glass schema scope does not match".into());
        }
        if self.scope.policy_preset == context.policy_preset {
            signals.push(signal(
                KnowledgeSignalKind::PolicyMatch,
                "policy scope matches",
            ));
        } else {
            conflicts.push("policy scope does not match".into());
        }
        for required in &self.invalidation.required_landmarks {
            if context.landmarks.iter().any(|current| current == required) {
                signals.push(signal(
                    KnowledgeSignalKind::LandmarkMatch,
                    format!("landmark {required} is present"),
                ));
            } else {
                missing_landmarks.push(required.clone());
            }
        }

        let age_seconds =
            parse_age_seconds(&self.source.last_verified_at, context.now_epoch_seconds);
        if let Some(age) = age_seconds {
            if self
                .invalidation
                .max_age_seconds
                .is_none_or(|maximum| age <= maximum as i64)
            {
                signals.push(signal(
                    KnowledgeSignalKind::FreshnessMatch,
                    format!("record age is {age} seconds"),
                ));
            } else {
                conflicts.push("record exceeded its maximum age".into());
            }
        } else {
            conflicts.push("lastVerifiedAt is not a valid RFC3339 timestamp".into());
        }

        let scope_conflict = conflicts.iter().any(|conflict| {
            matches!(
                conflict.as_str(),
                "origin does not match"
                    | "path is outside the record pattern"
                    | "profile scope does not match"
                    | "locale does not match"
                    | "tenant does not match"
                    | "browser family or version does not match"
                    | "Glass schema scope does not match"
                    | "policy scope does not match"
            )
        });
        let status = if self.confidence == KnowledgeConfidence::Contradicted {
            KnowledgeAssessmentStatus::Contradicted
        } else if self.confidence == KnowledgeConfidence::Quarantined {
            KnowledgeAssessmentStatus::Quarantined
        } else if scope_conflict {
            KnowledgeAssessmentStatus::OutOfScope
        } else if !missing_landmarks.is_empty()
            || age_seconds.is_none()
            || self
                .invalidation
                .max_age_seconds
                .is_some_and(|maximum| age_seconds.is_some_and(|age| age > maximum as i64))
        {
            KnowledgeAssessmentStatus::Stale
        } else {
            KnowledgeAssessmentStatus::Eligible
        };
        KnowledgeAssessment {
            record_id: self.record_id.clone(),
            status,
            signals,
            conflicts,
            missing_landmarks,
            age_seconds,
        }
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
        validate_timestamp("observedAt", &observed_at)?;
        if self.confidence == next {
            if fresh_verification {
                self.source.last_verified_at = observed_at;
                self.source.verification_count = self.source.verification_count.saturating_add(1);
                self.validate()?;
            }
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
            observed_at: observed_at.clone(),
        };
        self.confidence = next;
        if fresh_verification {
            self.source.last_verified_at = observed_at;
            self.source.verification_count = self.source.verification_count.saturating_add(1);
        }
        self.history.push(event);
        self.validate()
    }
}

fn signal(kind: KnowledgeSignalKind, detail: impl Into<String>) -> KnowledgeAssessmentSignal {
    KnowledgeAssessmentSignal {
        kind,
        detail: detail.into(),
    }
}

fn hash_knowledge_identity(parts: &[&str]) -> String {
    let canonical = serde_json::to_vec(parts).expect("JSON string arrays are serializable");
    let digest = Sha256::digest(canonical);
    format!("sha256:{digest:x}")
}

fn compare_optional_scope(
    signals: &mut Vec<KnowledgeAssessmentSignal>,
    conflicts: &mut Vec<String>,
    kind: KnowledgeSignalKind,
    label: &str,
    expected: &Option<String>,
    actual: &Option<String>,
) {
    if expected
        .as_ref()
        .is_none_or(|expected| Some(expected) == actual.as_ref())
    {
        signals.push(signal(kind, format!("{label} scope matches")));
    } else {
        conflicts.push(format!("{label} does not match"));
    }
}

fn browser_version_matches(expected_range: Option<&str>, actual: Option<&str>) -> bool {
    match (expected_range, actual) {
        (None, _) => true,
        (Some(expected), Some(actual)) => {
            if expected == actual || expected == ">=current" {
                return true;
            }
            let (operator, expected_version) = [">=", "<=", ">", "<", "="]
                .iter()
                .find_map(|operator| {
                    expected
                        .strip_prefix(operator)
                        .map(|version| (*operator, version))
                })
                .unwrap_or(("=", expected));
            let Some(expected_major) = version_major(expected_version) else {
                return false;
            };
            let Some(actual_major) = version_major(actual) else {
                return false;
            };
            match operator {
                ">=" => actual_major >= expected_major,
                "<=" => actual_major <= expected_major,
                ">" => actual_major > expected_major,
                "<" => actual_major < expected_major,
                "=" => actual_major == expected_major,
                _ => false,
            }
        }
        (Some(_), None) => false,
    }
}

fn version_major(value: &str) -> Option<u64> {
    value
        .trim()
        .split_once('.')
        .map_or(value.trim(), |(major, _)| major)
        .parse()
        .ok()
}

fn parse_age_seconds(last_verified_at: &str, now_epoch_seconds: i64) -> Option<i64> {
    let timestamp = chrono::DateTime::parse_from_rfc3339(last_verified_at).ok()?;
    Some(
        now_epoch_seconds
            .saturating_sub(timestamp.timestamp())
            .max(0),
    )
}

fn path_matches(pattern: &str, path: &str) -> bool {
    if pattern == path {
        return true;
    }
    let Some(prefix) = pattern.strip_suffix('*') else {
        return false;
    };
    path.starts_with(prefix)
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
    validate_timestamp("source.firstSeenAt", &source.first_seen_at)?;
    validate_timestamp("source.lastVerifiedAt", &source.last_verified_at)?;
    validate_text(
        "source.glassVersion",
        &source.glass_version,
        MAX_SCOPE_VALUE_BYTES,
        false,
    )
}

fn validate_timestamp(path: &str, value: &str) -> Result<(), KnowledgeValidationError> {
    validate_text(path, value, MAX_TIMESTAMP_BYTES, false)?;
    chrono::DateTime::parse_from_rfc3339(value).map_err(|error| {
        KnowledgeValidationError::new(path, format!("must be RFC3339: {error}"))
    })?;
    Ok(())
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
    use crate::browser::session::{
        FingerprintInvalidation, IntentConfidence, SemanticIntentPurpose, SemanticRegionKind,
        SemanticRouteIdentity, SemanticTargetFingerprint, WorkflowDefinition,
    };
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
    fn scalar_record_data_is_rejected_by_the_contract() {
        let mut record = record();
        record.data = json!("not-a-knowledge-object");
        let error = record.validate().unwrap_err();
        assert_eq!(error.path, "data");
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
        assert_eq!(record.source.last_verified_at, "2026-07-27T00:00:01Z");
        assert_eq!(record.source.verification_count, 2);
        assert_eq!(record.history.len(), 1);
    }

    #[test]
    fn fresh_verification_refreshes_an_unchanged_state() {
        let mut record = record();
        record
            .transition(
                KnowledgeConfidence::Observed,
                "fresh observation repeated".into(),
                "2026-07-27T00:00:02Z".into(),
                true,
            )
            .unwrap();
        assert_eq!(record.source.last_verified_at, "2026-07-27T00:00:02Z");
        assert_eq!(record.source.verification_count, 2);
        assert!(record.history.is_empty());
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

    fn lookup_context() -> KnowledgeLookupContext {
        KnowledgeLookupContext {
            origin: "https://example.test".into(),
            path: "/docs/getting-started".into(),
            profile_scope: KnowledgeProfileScope::Anonymous,
            profile_key: None,
            locale: Some("en-US".into()),
            tenant_key: None,
            browser_family: "chromium".into(),
            browser_version: Some(">=120".into()),
            glass_schema_version: 1,
            policy_preset: "balanced".into(),
            landmarks: vec!["documentation".into(), "main".into(), "search".into()],
            now_epoch_seconds: chrono::DateTime::parse_from_rfc3339("2026-07-27T00:00:00Z")
                .unwrap()
                .timestamp(),
        }
    }

    #[test]
    fn assessment_accepts_fresh_matching_scope_and_landmarks() {
        let assessment = record().assess(&lookup_context());
        assert_eq!(assessment.status, KnowledgeAssessmentStatus::Eligible);
        assert_eq!(assessment.missing_landmarks, Vec::<String>::new());
        assert!(assessment.conflicts.is_empty());
    }

    #[test]
    fn assessment_matches_browser_version_ranges() {
        assert!(browser_version_matches(Some(">=120"), Some("120.0")));
        assert!(browser_version_matches(Some("<121"), Some("120.0.1")));
        assert!(!browser_version_matches(Some(">=121"), Some("120.0")));
        assert!(!browser_version_matches(Some("120"), None));
    }

    #[test]
    fn assessment_marks_missing_landmarks_stale() {
        let mut context = lookup_context();
        context.landmarks.retain(|landmark| landmark != "search");
        let assessment = record().assess(&context);
        assert_eq!(assessment.status, KnowledgeAssessmentStatus::Stale);
        assert_eq!(assessment.missing_landmarks, vec!["search"]);
    }

    #[test]
    fn assessment_rejects_cross_origin_scope() {
        let mut context = lookup_context();
        context.origin = "https://other.test".into();
        let assessment = record().assess(&context);
        assert_eq!(assessment.status, KnowledgeAssessmentStatus::OutOfScope);
        assert!(
            assessment
                .conflicts
                .contains(&"origin does not match".to_string())
        );
    }

    #[test]
    fn page_record_keeps_shape_but_not_current_targets() {
        let corpus: Value = serde_json::from_str(include_str!(
            "../../../benchmarks/scenarios/semantic-observation-v1.json"
        ))
        .unwrap();
        let observation =
            SemanticObservation::from_json(&corpus["fixtures"][1]["observation"].to_string())
                .unwrap();
        let record = KnowledgeRecord::from_page_observation(
            &observation,
            KnowledgeRecordBuildOptions {
                record_id: "knowledge_search".into(),
                scope: record().scope,
                glass_version: "0.2.0".into(),
                observed_at: "2026-07-27T00:00:00Z".into(),
            },
        )
        .unwrap();
        let data = serde_json::to_string(&record.data).unwrap();
        assert!(data.contains("searchResults"));
        assert!(!data.contains("axr-8-9"));
        assert_eq!(record.confidence, KnowledgeConfidence::Observed);
    }

    #[test]
    fn target_record_keeps_digest_but_not_current_handles_or_names() {
        let candidate = SemanticIntentCandidate {
            id: "candidate_1".into(),
            reference: "axr-42-1".into(),
            role: "button".into(),
            name: "Private Settings Label".into(),
            input_type: None,
            region_id: Some("region_navigation".into()),
            region_kind: Some(SemanticRegionKind::Navigation),
            confidence: IntentConfidence::Exact,
            evidence: Vec::new(),
            fingerprint: Some(SemanticTargetFingerprint {
                revision: 42,
                route: SemanticRouteIdentity {
                    target_id: "target-1".into(),
                    frame_id: "frame-1".into(),
                    url: "https://example.test/settings".into(),
                },
                role: "button".into(),
                name: "Private Settings Label".into(),
                input_type: None,
                region_id: Some("region_navigation".into()),
                region_kind: Some(SemanticRegionKind::Navigation),
                purpose: SemanticIntentPurpose::Open,
                invalidated_by: vec![FingerprintInvalidation::Revision],
            }),
        };
        let record = KnowledgeRecord::from_intent_candidate(
            &candidate,
            KnowledgeRecordBuildOptions {
                record_id: "knowledge-settings-target".into(),
                scope: record().scope,
                glass_version: "0.2.0".into(),
                observed_at: "2026-07-27T00:00:00Z".into(),
            },
        )
        .unwrap();
        let data = serde_json::to_string(&record.data).unwrap();
        assert!(data.contains("sha256:"));
        assert!(!data.contains("axr-42-1"));
        assert!(!data.contains("Private Settings Label"));
        assert!(!data.contains("target-1"));
        assert!(!data.contains("frame-1"));
    }

    #[test]
    fn workflow_record_keeps_shape_without_definition_details() {
        let corpus: Value = serde_json::from_str(include_str!(
            "../../../benchmarks/scenarios/workflow-v1.json"
        ))
        .unwrap();
        let workflow =
            WorkflowDefinition::from_value(corpus["scenarios"][0]["workflow"].clone()).unwrap();
        let record = KnowledgeRecord::from_workflow_definition(
            &workflow,
            KnowledgeRecordBuildOptions {
                record_id: "knowledge-workflow-entry".into(),
                scope: record().scope,
                glass_version: "0.2.0".into(),
                observed_at: "2026-07-27T00:00:00Z".into(),
            },
        )
        .unwrap();
        assert_eq!(record.kind, KnowledgeRecordKind::WorkflowEntryPoint);
        assert_eq!(record.confidence, KnowledgeConfidence::Candidate);
        let data = serde_json::to_string(&record.data).unwrap();
        assert!(data.contains("workflowHash"));
        assert!(!data.contains("linear-typed-output"));
        assert!(!data.contains("Glass Scorecard"));
        assert!(!data.contains("target"));
    }
}
