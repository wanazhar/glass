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
const MAX_RETRIEVAL_SIGNALS: usize = 16;
const MAX_BACKEND_CAPABILITIES: usize = 32;
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
    pub current_revision: u64,
    /// Current source dimensions are optional for callers that only need
    /// scope assessment; portability checks fail closed when absent.
    pub surface_kind: Option<KnowledgeSurfaceKind>,
    pub backend_kind: Option<KnowledgeBackendKind>,
    pub backend_capabilities: Vec<KnowledgeBackendCapability>,
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
    pub current_revision: Option<u64>,
    pub surface_kind: Option<KnowledgeSurfaceKind>,
    pub backend_kind: Option<KnowledgeBackendKind>,
    pub backend_capabilities: Vec<KnowledgeBackendCapability>,
}

/// Inputs for creating one page-family record from fresh semantic evidence.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KnowledgeRecordBuildOptions {
    pub record_id: String,
    pub scope: KnowledgeScope,
    pub glass_version: String,
    pub observed_at: String,
    pub surface: KnowledgeSurfaceProvenance,
    pub backend: KnowledgeBackendProvenance,
    pub portability: KnowledgePortability,
    pub current_validation: KnowledgeCurrentValidation,
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
        let current_revision = options.current_revision.unwrap_or(observation.revision);
        if current_revision == 0 {
            return Err(KnowledgeValidationError::new(
                "currentRevision",
                "must be positive",
            ));
        }
        validate_backend_capabilities("backendCapabilities", &options.backend_capabilities)?;
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
            surface_kind: options.surface_kind,
            backend_kind: options.backend_kind,
            backend_capabilities: options.backend_capabilities,
            current_revision,
        })
    }
    /// Build a lookup context with the live surface/backend evidence used by
    /// the observation. Callers must provide these dimensions explicitly.
    pub fn from_observation_with_portability(
        observation: &SemanticObservation,
        options: KnowledgeLookupOptions,
        surface_kind: KnowledgeSurfaceKind,
        backend_kind: KnowledgeBackendKind,
        backend_capabilities: Vec<KnowledgeBackendCapability>,
    ) -> Result<Self, KnowledgeValidationError> {
        let mut context = Self::from_observation(observation, options)?;
        context.surface_kind = Some(surface_kind);
        context.backend_kind = Some(backend_kind);
        context.backend_capabilities = backend_capabilities;
        Ok(context)
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
#[serde(rename_all = "camelCase")]
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
#[serde(rename_all = "camelCase")]
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
#[serde(rename_all = "camelCase")]
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
#[serde(rename_all = "camelCase")]
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

/// Bounded provenance for the surface that produced a knowledge record.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct KnowledgeSurfaceProvenance {
    pub kind: KnowledgeSurfaceKind,
    pub understanding: KnowledgeUnderstandingLevel,
    pub coverage: KnowledgeSurfaceCoverage,
}

/// Surface kinds are deliberately transport-neutral and closed over the
/// known foundation vocabulary. Unknown serialized values fail closed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum KnowledgeSurfaceKind {
    Document,
    Accessibility,
    ShadowDocument,
    FrameDocument,
    Svg,
    Canvas2d,
    Webgl,
    Webgpu,
    EmbeddedPdf,
    Media,
    Terminal,
    RemoteApplication,
    BrowserNative,
    ExtensionDefined,
    Unknown,
    Opaque,
}

/// How much of a source surface Glass understood when it learned a record.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum KnowledgeUnderstandingLevel {
    Opaque,
    Inferred,
    Partial,
    Strong,
    TaskCompilable,
}

/// Bounded coverage dimensions retained for explainability.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum KnowledgeSurfaceCoverage {
    None,
    Structural,
    Semantic,
    Interaction,
    Complete,
}

impl Default for KnowledgeSurfaceProvenance {
    fn default() -> Self {
        Self {
            kind: KnowledgeSurfaceKind::Opaque,
            understanding: KnowledgeUnderstandingLevel::Opaque,
            coverage: KnowledgeSurfaceCoverage::None,
        }
    }
}

/// Backend identity and capability provenance for a learned record.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct KnowledgeBackendProvenance {
    pub backend: KnowledgeBackendKind,
    pub profile: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub capabilities: Vec<KnowledgeBackendCapability>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum KnowledgeBackendKind {
    Cdp,
    WebdriverBidi,
    BrowserExtension,
    Visual,
    Terminal,
    Unknown,
}

/// Capabilities are evidence provenance, not permission to perform an action.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum KnowledgeBackendCapability {
    Navigation,
    SemanticExtraction,
    CoordinateInput,
    Script,
    Capture,
    Verification,
    Storage,
    Prompt,
}

impl Default for KnowledgeBackendProvenance {
    fn default() -> Self {
        Self {
            backend: KnowledgeBackendKind::Unknown,
            profile: "legacy".into(),
            capabilities: Vec::new(),
        }
    }
}

/// How safely a remembered fact may travel between surfaces and backends.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum KnowledgePortability {
    SemanticPortable,
    SurfacePortable,
    BackendCapabilityDependent,
    BackendSpecific,
    BrowserSpecific,
    NonPortable,
}

/// The advisory role memory had in producing a compiler input.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum KnowledgeMemoryInfluence {
    None,
    RankingOnly,
    TemplateSuggested,
    VerificationSuggested,
    RecoverySuggested,
    IdentityContinuitySuggested,
}

/// Explainable deterministic and optional semantic retrieval evidence.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum KnowledgeRetrievalSignalKind {
    ExactPageFamilyMatch,
    ExactFingerprintMatch,
    OriginMatch,
    SurfaceMatch,
    BackendMatch,
    GraphDistance,
    SemanticSimilarity,
    Freshness,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct KnowledgeRetrievalSignal {
    pub kind: KnowledgeRetrievalSignalKind,
    pub detail: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub score_millis: Option<u16>,
}

/// Current validation is intentionally separate from historical confidence.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum KnowledgeCurrentValidationStatus {
    NotValidated,
    Validated,
    Rejected,
    Stale,
    Contradicted,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum KnowledgeEvidenceQuality {
    None,
    Weak,
    Partial,
    Strong,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct KnowledgeCurrentValidation {
    pub status: KnowledgeCurrentValidationStatus,
    pub evidence_quality: KnowledgeEvidenceQuality,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub current_revision: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub validated_at: Option<String>,
}

impl Default for KnowledgeCurrentValidation {
    fn default() -> Self {
        Self {
            status: KnowledgeCurrentValidationStatus::NotValidated,
            evidence_quality: KnowledgeEvidenceQuality::None,
            current_revision: None,
            validated_at: None,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct KnowledgeRetrievalExplanation {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub signals: Vec<KnowledgeRetrievalSignal>,
    #[serde(default)]
    pub current_validation: KnowledgeCurrentValidation,
}

/// Provenance and verification counters for a knowledge record.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct KnowledgeSource {
    pub first_seen_at: String,
    pub last_verified_at: String,
    pub glass_version: String,
    pub verification_count: u32,
    #[serde(default, skip_serializing_if = "is_legacy_surface")]
    pub surface: KnowledgeSurfaceProvenance,
    #[serde(default, skip_serializing_if = "is_legacy_backend")]
    pub backend: KnowledgeBackendProvenance,
}

impl Default for KnowledgeSource {
    fn default() -> Self {
        Self {
            first_seen_at: String::new(),
            last_verified_at: String::new(),
            glass_version: String::new(),
            verification_count: 0,
            surface: KnowledgeSurfaceProvenance::default(),
            backend: KnowledgeBackendProvenance::default(),
        }
    }
}
impl Default for KnowledgePortability {
    fn default() -> Self {
        Self::NonPortable
    }
}

impl Default for KnowledgeMemoryInfluence {
    fn default() -> Self {
        Self::None
    }
}
fn is_legacy_surface(value: &KnowledgeSurfaceProvenance) -> bool {
    *value == KnowledgeSurfaceProvenance::default()
}

fn is_legacy_backend(value: &KnowledgeBackendProvenance) -> bool {
    *value == KnowledgeBackendProvenance::default()
}

fn is_nonportable(value: &KnowledgePortability) -> bool {
    *value == KnowledgePortability::NonPortable
}

fn is_no_memory_influence(value: &KnowledgeMemoryInfluence) -> bool {
    *value == KnowledgeMemoryInfluence::None
}

fn is_empty_retrieval(value: &KnowledgeRetrievalExplanation) -> bool {
    value.signals.is_empty()
        && value.current_validation == KnowledgeCurrentValidation::default()
}

/// Conditions that make remembered knowledge stale or unusable.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgeInvalidation {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_age_seconds: Option<u64>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub required_landmarks: Vec<String>,
}

/// One bounded, auditable lifecycle transition.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
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
    #[serde(
        default,
        skip_serializing_if = "is_nonportable"
    )]
    pub portability: KnowledgePortability,
    #[serde(default, skip_serializing_if = "is_no_memory_influence")]
    pub memory_influence: KnowledgeMemoryInfluence,
    #[serde(default, skip_serializing_if = "is_empty_retrieval")]
    pub retrieval: KnowledgeRetrievalExplanation,
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
                surface: options.surface,
                backend: options.backend,
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
            portability: options.portability,
            memory_influence: KnowledgeMemoryInfluence::default(),
            retrieval: KnowledgeRetrievalExplanation {
                current_validation: options.current_validation,
                ..KnowledgeRetrievalExplanation::default()
            },
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
                surface: options.surface,
                backend: options.backend,
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
            portability: options.portability,
            memory_influence: KnowledgeMemoryInfluence::default(),
            retrieval: KnowledgeRetrievalExplanation {
                current_validation: options.current_validation,
                ..KnowledgeRetrievalExplanation::default()
            },
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
                surface: options.surface,
                backend: options.backend,
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
            portability: options.portability,
            memory_influence: KnowledgeMemoryInfluence::default(),
            retrieval: KnowledgeRetrievalExplanation {
                current_validation: options.current_validation,
                ..KnowledgeRetrievalExplanation::default()
            },
            history: Vec::new(),
        };
        record.validate()?;
        Ok(record)
    }
    /// Attach explicit source provenance and portability to a freshly built
    /// record. Legacy defaults remain non-portable and non-authorizing.
    pub fn with_provenance(
        mut self,
        surface: KnowledgeSurfaceProvenance,
        backend: KnowledgeBackendProvenance,
        portability: KnowledgePortability,
    ) -> Result<Self, KnowledgeValidationError> {
        self.source.surface = surface;
        self.source.backend = backend;
        self.portability = portability;
        self.validate()?;
        Ok(self)
    }

    /// Attach a current Web IR evidence witness explicitly. A timestamp or
    /// boolean freshness flag without a positive revision is rejected.
    pub fn with_current_validation(
        mut self,
        current_revision: u64,
        evidence_quality: KnowledgeEvidenceQuality,
        validated_at: String,
    ) -> Result<Self, KnowledgeValidationError> {
        if current_revision == 0 {
            return Err(KnowledgeValidationError::new(
                "currentRevision",
                "must be positive",
            ));
        }
        if evidence_quality == KnowledgeEvidenceQuality::None {
            return Err(KnowledgeValidationError::new(
                "evidenceQuality",
                "must identify a current evidence quality",
            ));
        }
        validate_timestamp("validatedAt", &validated_at)?;
        self.retrieval.current_validation = KnowledgeCurrentValidation {
            status: KnowledgeCurrentValidationStatus::Validated,
            evidence_quality,
            current_revision: Some(current_revision),
            validated_at: Some(validated_at),
        };
        self.validate()?;
        Ok(self)
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
        validate_retrieval(&self.retrieval)?;
        if self.memory_influence != KnowledgeMemoryInfluence::None {
            if self.retrieval.signals.is_empty() {
                return Err(KnowledgeValidationError::new(
                    "memoryInfluence",
                    "memory influence requires retrieval signals",
                ));
            }
            if self.retrieval.current_validation.status
                != KnowledgeCurrentValidationStatus::Validated
            {
                return Err(KnowledgeValidationError::new(
                    "memoryInfluence",
                    "memory influence requires current validation",
                ));
            }
        }
        if (matches!(
            self.source.surface.kind,
            KnowledgeSurfaceKind::Opaque | KnowledgeSurfaceKind::Unknown
        ) || self.source.backend.backend == KnowledgeBackendKind::Unknown)
            && self.portability != KnowledgePortability::NonPortable
        {
            return Err(KnowledgeValidationError::new(
                "portability",
                "opaque or unknown provenance is nonPortable",
            ));
        }
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

        let current_validation_conflict = self.retrieval.current_validation.status
            != KnowledgeCurrentValidationStatus::Validated;
        let current_revision_conflict = self.retrieval.current_validation.current_revision
            != Some(context.current_revision);
        if current_validation_conflict {
            conflicts.push("record lacks current Web IR validation".into());
        } else if current_revision_conflict {
            conflicts.push("current Web IR revision does not match".into());
        }
        let provenance_conflict = matches!(
            self.source.surface.kind,
            KnowledgeSurfaceKind::Opaque | KnowledgeSurfaceKind::Unknown
        ) || self.source.backend.backend == KnowledgeBackendKind::Unknown;
        if provenance_conflict {
            conflicts.push("surface or backend provenance is unknown".into());
        }
        let surface_conflict = self.portability == KnowledgePortability::SurfacePortable
            && context.surface_kind != Some(self.source.surface.kind);
        let backend_conflict = match self.portability {
            KnowledgePortability::BackendCapabilityDependent => {
                context.backend_kind != Some(self.source.backend.backend)
                    || self
                        .source
                        .backend
                        .capabilities
                        .iter()
                        .any(|capability| !context.backend_capabilities.contains(capability))
            }
            KnowledgePortability::BackendSpecific | KnowledgePortability::BrowserSpecific => {
                context.backend_kind != Some(self.source.backend.backend)
            }
            _ => false,
        };
        let portability_conflict = self.portability == KnowledgePortability::NonPortable
            || surface_conflict
            || backend_conflict;
        if portability_conflict {
            conflicts.push(
                if surface_conflict {
                    "record surface provenance is incompatible with this context"
                } else if backend_conflict {
                    "record backend provenance is incompatible with this context"
                } else {
                    "record portability is incompatible with this context"
                }
                .into(),
            );
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
        } else if current_validation_conflict
            || current_revision_conflict
            || provenance_conflict
            || portability_conflict
            || !missing_landmarks.is_empty()
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

    /// Apply a lifecycle transition after a current Web IR witness has been
    /// recorded. A boolean freshness flag alone never creates validation.
    pub fn transition_with_validation(
        &mut self,
        next: KnowledgeConfidence,
        reason: String,
        observed_at: String,
        current_revision: u64,
        evidence_quality: KnowledgeEvidenceQuality,
    ) -> Result<(), KnowledgeValidationError> {
        if current_revision == 0 {
            return Err(KnowledgeValidationError::new(
                "currentRevision",
                "must be positive",
            ));
        }
        if evidence_quality == KnowledgeEvidenceQuality::None {
            return Err(KnowledgeValidationError::new(
                "evidenceQuality",
                "must identify a current evidence quality",
            ));
        }
        let previous = self.retrieval.current_validation.clone();
        self.retrieval.current_validation = KnowledgeCurrentValidation {
            status: KnowledgeCurrentValidationStatus::Validated,
            evidence_quality,
            current_revision: Some(current_revision),
            validated_at: Some(observed_at.clone()),
        };
        let result = self.transition(next, reason, observed_at, true);
        if result.is_err() {
            self.retrieval.current_validation = previous;
        }
        result
    }

    /// Apply a lifecycle transition. Promotion to `verified` and recovery from
    /// contradiction/quarantine require fresh verification evidence and a
    /// separately recorded current validation witness.
    pub fn transition(
        &mut self,
        next: KnowledgeConfidence,
        reason: String,
        observed_at: String,
        fresh_verification: bool,
    ) -> Result<(), KnowledgeValidationError> {
        let mut candidate = self.clone();
        candidate.transition_in_place(next, reason, observed_at, fresh_verification)?;
        *self = candidate;
        Ok(())
    }

    fn transition_in_place(
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
                self.source.last_verified_at = observed_at.clone();
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
        if next == KnowledgeConfidence::Verified
            && self.retrieval.current_validation.status
                != KnowledgeCurrentValidationStatus::Validated
        {
            return Err(KnowledgeValidationError::new(
                "currentValidation",
                "verified promotion requires a current Web IR witness",
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
            self.source.last_verified_at = observed_at.clone();
            self.source.verification_count = self.source.verification_count.saturating_add(1);
        }
        if next == KnowledgeConfidence::Stale {
            self.invalidate_current_validation(KnowledgeCurrentValidationStatus::Stale);
        } else if matches!(
            next,
            KnowledgeConfidence::Contradicted | KnowledgeConfidence::Quarantined
        ) {
            self.invalidate_current_validation(KnowledgeCurrentValidationStatus::Contradicted);
        }
        self.history.push(event);
        self.validate()
    }

    fn invalidate_current_validation(&mut self, status: KnowledgeCurrentValidationStatus) {
        self.retrieval.current_validation = KnowledgeCurrentValidation {
            status,
            evidence_quality: KnowledgeEvidenceQuality::None,
            current_revision: None,
            validated_at: None,
        };
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
#[serde(rename_all = "camelCase")]
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
    )?;
    validate_surface_provenance(&source.surface)?;
    validate_backend_provenance(&source.backend)
}

fn validate_surface_provenance(
    surface: &KnowledgeSurfaceProvenance,
) -> Result<(), KnowledgeValidationError> {
    if surface.coverage == KnowledgeSurfaceCoverage::None
        && surface.understanding != KnowledgeUnderstandingLevel::Opaque
    {
        return Err(KnowledgeValidationError::new(
            "source.surface.coverage",
            "none coverage requires opaque understanding",
        ));
    }
    if matches!(
        surface.kind,
        KnowledgeSurfaceKind::Opaque | KnowledgeSurfaceKind::Unknown
    ) && surface.understanding != KnowledgeUnderstandingLevel::Opaque
    {
        return Err(KnowledgeValidationError::new(
            "source.surface.understanding",
            "opaque or unknown surfaces cannot claim understanding",
        ));
    }
    if surface.understanding == KnowledgeUnderstandingLevel::TaskCompilable
        && surface.coverage != KnowledgeSurfaceCoverage::Complete
    {
        return Err(KnowledgeValidationError::new(
            "source.surface.coverage",
            "task-compilable understanding requires complete coverage",
        ));
    }
    Ok(())
}

fn validate_backend_provenance(
    backend: &KnowledgeBackendProvenance,
) -> Result<(), KnowledgeValidationError> {
    validate_text(
        "source.backend.profile",
        &backend.profile,
        MAX_SCOPE_VALUE_BYTES,
        false,
    )?;
    validate_public_text("source.backend.profile", &backend.profile)?;
    validate_backend_capabilities("source.backend.capabilities", &backend.capabilities)
}

fn validate_backend_capabilities(
    path: &str,
    capabilities: &[KnowledgeBackendCapability],
) -> Result<(), KnowledgeValidationError> {
    if capabilities.len() > MAX_BACKEND_CAPABILITIES {
        return Err(KnowledgeValidationError::new(
            path,
            format!("contains more than {MAX_BACKEND_CAPABILITIES} capabilities"),
        ));
    }
    let mut unique = BTreeSet::new();
    for (index, capability) in capabilities.iter().enumerate() {
        if !unique.insert(*capability) {
            return Err(KnowledgeValidationError::new(
                format!("{path}[{index}]"),
                "capability is duplicated",
            ));
        }
    }
    Ok(())
}

fn validate_retrieval(
    retrieval: &KnowledgeRetrievalExplanation,
) -> Result<(), KnowledgeValidationError> {
    if retrieval.signals.len() > MAX_RETRIEVAL_SIGNALS {
        return Err(KnowledgeValidationError::new(
            "retrieval.signals",
            format!("contains more than {MAX_RETRIEVAL_SIGNALS} signals"),
        ));
    }
    for (index, signal) in retrieval.signals.iter().enumerate() {
        validate_text(
            &format!("retrieval.signals[{index}].detail"),
            &signal.detail,
            MAX_SCOPE_VALUE_BYTES,
            false,
        )?;
        validate_public_text(
            &format!("retrieval.signals[{index}].detail"),
            &signal.detail,
        )?;
        if signal.score_millis.is_some_and(|score| score > 1_000) {
            return Err(KnowledgeValidationError::new(
                format!("retrieval.signals[{index}].scoreMillis"),
                "must be between 0 and 1000",
            ));
        }
    }
    let current = &retrieval.current_validation;
    match current.status {
        KnowledgeCurrentValidationStatus::Validated => {
            if current.evidence_quality == KnowledgeEvidenceQuality::None {
                return Err(KnowledgeValidationError::new(
                    "retrieval.currentValidation.evidenceQuality",
                    "validated evidence requires a quality",
                ));
            }
            if current
                .current_revision
                .is_none_or(|revision| revision == 0)
            {
                return Err(KnowledgeValidationError::new(
                    "retrieval.currentValidation.currentRevision",
                    "validated evidence requires a positive current revision witness",
                ));
            }
            let timestamp = current.validated_at.as_deref().ok_or_else(|| {
                KnowledgeValidationError::new(
                    "retrieval.currentValidation.validatedAt",
                    "is required for validated evidence",
                )
            })?;
            validate_timestamp("retrieval.currentValidation.validatedAt", timestamp)?;
        }
        KnowledgeCurrentValidationStatus::NotValidated => {
            if current.current_revision.is_some() || current.validated_at.is_some() {
                return Err(KnowledgeValidationError::new(
                    "retrieval.currentValidation",
                    "notValidated evidence cannot include revision or timestamp",
                ));
            }
            if current.evidence_quality != KnowledgeEvidenceQuality::None {
                return Err(KnowledgeValidationError::new(
                    "retrieval.currentValidation.evidenceQuality",
                    "notValidated evidence must have none quality",
                ));
            }
        }
        _ => {
            if current.current_revision.is_some() || current.validated_at.is_some() {
                return Err(KnowledgeValidationError::new(
                    "retrieval.currentValidation",
                    "invalidated evidence cannot retain a revision or timestamp",
                ));
            }
            if current.evidence_quality != KnowledgeEvidenceQuality::None {
                return Err(KnowledgeValidationError::new(
                    "retrieval.currentValidation.evidenceQuality",
                    "invalidated evidence must have none quality",
                ));
            }
        }
    }
    Ok(())
}

fn validate_public_text(path: &str, value: &str) -> Result<(), KnowledgeValidationError> {
    let normalized = value
        .chars()
        .filter(char::is_ascii_alphanumeric)
        .collect::<String>()
        .to_ascii_lowercase();
    const FORBIDDEN: &[&str] = &[
        "authorization", "cookie", "credential", "password", "secret", "session", "token",
    ];
    if FORBIDDEN.iter().any(|word| normalized.contains(word)) {
        return Err(KnowledgeValidationError::new(
            path,
            "sensitive value is not permitted in provenance",
        ));
    }
    Ok(())
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
    #[test]
    fn lookup_options_propagate_live_portability_without_defaults() {
        let corpus: Value = serde_json::from_str(include_str!(
            "../../../benchmarks/scenarios/semantic-observation-v1.json"
        ))
        .unwrap();
        let observation =
            SemanticObservation::from_json(&corpus["fixtures"][1]["observation"].to_string())
                .unwrap();
        let options = KnowledgeLookupOptions {
            profile_scope: KnowledgeProfileScope::Anonymous,
            profile_key: None,
            locale: None,
            tenant_key: None,
            browser_family: "chromium".into(),
            browser_version: Some("120.0".into()),
            glass_schema_version: 1,
            policy_preset: "balanced".into(),
            now_epoch_seconds: 0,
            current_revision: None,
            surface_kind: Some(KnowledgeSurfaceKind::Svg),
            backend_kind: Some(KnowledgeBackendKind::Visual),
            backend_capabilities: vec![KnowledgeBackendCapability::Capture],
        };
        let context = KnowledgeLookupContext::from_observation(&observation, options).unwrap();
        assert_eq!(context.surface_kind, Some(KnowledgeSurfaceKind::Svg));
        assert_eq!(context.backend_kind, Some(KnowledgeBackendKind::Visual));
        assert_eq!(
            context.backend_capabilities,
            vec![KnowledgeBackendCapability::Capture]
        );
        let mut absent = context;
        absent.surface_kind = None;
        absent.backend_kind = None;
        absent.backend_capabilities.clear();
        assert!(absent.surface_kind.is_none());
        assert!(absent.backend_kind.is_none());
    }
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
                surface: KnowledgeSurfaceProvenance {
                    kind: KnowledgeSurfaceKind::Document,
                    understanding: KnowledgeUnderstandingLevel::Strong,
                    coverage: KnowledgeSurfaceCoverage::Semantic,
                },
                backend: KnowledgeBackendProvenance {
                    backend: KnowledgeBackendKind::Cdp,
                    profile: "production".into(),
                    capabilities: vec![
                        KnowledgeBackendCapability::SemanticExtraction,
                        KnowledgeBackendCapability::Verification,
                    ],
                },
            },
            confidence: KnowledgeConfidence::Observed,
            invalidation: KnowledgeInvalidation {
                max_age_seconds: Some(604_800),
                required_landmarks: vec!["main".into(), "search".into()],
            },
            data: json!({"pageKind": "documentation", "regions": ["main", "search"]}),
            portability: KnowledgePortability::SurfacePortable,
            memory_influence: KnowledgeMemoryInfluence::None,
            retrieval: KnowledgeRetrievalExplanation {
                signals: Vec::new(),
                current_validation: KnowledgeCurrentValidation {
                    status: KnowledgeCurrentValidationStatus::Validated,
                    evidence_quality: KnowledgeEvidenceQuality::Strong,
                    current_revision: Some(42),
                    validated_at: Some("2026-07-27T00:00:00Z".into()),
                },
            },
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
            surface_kind: Some(KnowledgeSurfaceKind::Document),
            backend_kind: Some(KnowledgeBackendKind::Cdp),
            backend_capabilities: vec![
                KnowledgeBackendCapability::SemanticExtraction,
                KnowledgeBackendCapability::Verification,
            ],
            glass_schema_version: 1,
            policy_preset: "balanced".into(),
            current_revision: 42,
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
    fn assessment_rejects_current_revision_mismatch() {
        let mut context = lookup_context();
        context.current_revision = 43;
        let assessment = record().assess(&context);
        assert_eq!(assessment.status, KnowledgeAssessmentStatus::Stale);
        assert!(
            assessment
                .conflicts
                .contains(&"current Web IR revision does not match".to_string())
        );
    }
    #[test]
    fn lookup_capabilities_are_bounded_and_unique() {
        let oversized = vec![
            KnowledgeBackendCapability::Navigation;
            MAX_BACKEND_CAPABILITIES + 1
        ];
        let error = validate_backend_capabilities("backendCapabilities", &oversized).unwrap_err();
        assert_eq!(error.path, "backendCapabilities");
        let duplicate = vec![
            KnowledgeBackendCapability::Navigation,
            KnowledgeBackendCapability::Navigation,
        ];
        let error = validate_backend_capabilities("backendCapabilities", &duplicate).unwrap_err();
        assert_eq!(error.path, "backendCapabilities[1]");
    }
    #[test]
    fn assessment_fails_closed_without_current_validation() {
        let mut legacy = record();
        legacy.retrieval.current_validation = KnowledgeCurrentValidation::default();
        let assessment = legacy.assess(&lookup_context());
        assert_eq!(assessment.status, KnowledgeAssessmentStatus::Stale);
        assert!(
            assessment
                .conflicts
                .contains(&"record lacks current Web IR validation".to_string())
        );
    }

    #[test]
    fn assessment_fails_closed_for_opaque_backend_and_nonportable_memory() {
        let mut legacy = record();
        legacy.source.surface = KnowledgeSurfaceProvenance::default();
        legacy.source.backend = KnowledgeBackendProvenance::default();
        legacy.portability = KnowledgePortability::NonPortable;
        let assessment = legacy.assess(&lookup_context());
        assert_eq!(assessment.status, KnowledgeAssessmentStatus::Stale);
        assert!(
            assessment
                .conflicts
                .contains(&"surface or backend provenance is unknown".to_string())
        );
    }
    #[test]
    fn surface_portability_requires_current_surface_witness() {
        let mut context = lookup_context();
        context.surface_kind = Some(KnowledgeSurfaceKind::Svg);
        let assessment = record().assess(&context);
        assert_eq!(assessment.status, KnowledgeAssessmentStatus::Stale);
        assert!(
            assessment
                .conflicts
                .contains(&"record surface provenance is incompatible with this context".into())
        );
    }

    #[test]
    fn lifecycle_invalidation_clears_current_validation() {
        let mut value = record();
        value
            .transition(
                KnowledgeConfidence::Stale,
                "drift detected".into(),
                "2026-07-27T00:00:01Z".into(),
                false,
            )
            .unwrap();
        assert_eq!(
            value.retrieval.current_validation.status,
            KnowledgeCurrentValidationStatus::Stale
        );
        assert_eq!(value.assess(&lookup_context()).status, KnowledgeAssessmentStatus::Stale);
    }

    #[test]
    fn validation_requires_revision_witness_and_retrieval_is_redacted() {
        let mut value = record();
        value.retrieval.current_validation.current_revision = None;
        let error = value.validate().unwrap_err();
        assert_eq!(error.path, "retrieval.currentValidation.currentRevision");

        let mut signal_value = record();
        signal_value.retrieval.signals.push(KnowledgeRetrievalSignal {
            kind: KnowledgeRetrievalSignalKind::SemanticSimilarity,
            detail: "token from page".into(),
            score_millis: None,
        });
        let error = signal_value.validate().unwrap_err();
        assert_eq!(error.path, "retrieval.signals[0].detail");
        assert!(
            serde_json::from_value::<KnowledgeRetrievalSignal>(
                json!({"kind":"freshness","detail":"ok","extra":"reject"})
            )
            .is_err()
        );
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
                surface: KnowledgeSurfaceProvenance {
                    kind: KnowledgeSurfaceKind::Document,
                    understanding: KnowledgeUnderstandingLevel::Strong,
                    coverage: KnowledgeSurfaceCoverage::Semantic,
                },
                backend: KnowledgeBackendProvenance {
                    backend: KnowledgeBackendKind::Cdp,
                    profile: "test".into(),
                    capabilities: vec![KnowledgeBackendCapability::SemanticExtraction],
                },
                portability: KnowledgePortability::SemanticPortable,
                current_validation: KnowledgeCurrentValidation::default(),
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
                surface: KnowledgeSurfaceProvenance {
                    kind: KnowledgeSurfaceKind::Document,
                    understanding: KnowledgeUnderstandingLevel::Strong,
                    coverage: KnowledgeSurfaceCoverage::Semantic,
                },
                backend: KnowledgeBackendProvenance {
                    backend: KnowledgeBackendKind::Cdp,
                    profile: "test".into(),
                    capabilities: vec![KnowledgeBackendCapability::SemanticExtraction],
                },
                portability: KnowledgePortability::SemanticPortable,
                current_validation: KnowledgeCurrentValidation::default(),
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
                surface: KnowledgeSurfaceProvenance {
                    kind: KnowledgeSurfaceKind::Document,
                    understanding: KnowledgeUnderstandingLevel::Strong,
                    coverage: KnowledgeSurfaceCoverage::Semantic,
                },
                backend: KnowledgeBackendProvenance {
                    backend: KnowledgeBackendKind::Cdp,
                    profile: "test".into(),
                    capabilities: vec![KnowledgeBackendCapability::SemanticExtraction],
                },
                portability: KnowledgePortability::SemanticPortable,
                current_validation: KnowledgeCurrentValidation::default(),
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
    #[test]
    fn metadata_round_trip_is_serde_stable_and_bounded() {
        let mut record = record();
        record.source.surface = KnowledgeSurfaceProvenance {
            kind: KnowledgeSurfaceKind::Document,
            understanding: KnowledgeUnderstandingLevel::Strong,
            coverage: KnowledgeSurfaceCoverage::Semantic,
        };
        record.source.backend = KnowledgeBackendProvenance {
            backend: KnowledgeBackendKind::Cdp,
            profile: "production".into(),
            capabilities: vec![
                KnowledgeBackendCapability::SemanticExtraction,
                KnowledgeBackendCapability::Verification,
            ],
        };
        record.portability = KnowledgePortability::SurfacePortable;
        record.retrieval.signals = vec![KnowledgeRetrievalSignal {
            kind: KnowledgeRetrievalSignalKind::ExactPageFamilyMatch,
            detail: "same page family".into(),
            score_millis: Some(1_000),
        }];
        record.validate().unwrap();
        let json = record.to_canonical_json().unwrap();
        let parsed: KnowledgeRecord = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, record);
        assert!(json.contains("\"surface\""));
        assert!(json.contains("\"backend\""));
        assert!(json.contains("\"surfacePortable\""));
        assert!(json.contains("\"exactPageFamilyMatch\""));
    }

    #[test]
    fn legacy_snapshot_uses_fail_closed_additive_defaults() {
        let legacy = r#"{
            "schemaVersion":1,
            "records":[{
                "schemaVersion":1,
                "recordId":"legacy",
                "kind":"pageFamily",
                "scope":{
                    "origin":"https://example.test",
                    "pathPattern":"/docs/*",
                    "profileScope":"anonymous",
                    "browserFamily":"chromium",
                    "glassSchemaVersion":1,
                    "policyPreset":"balanced"
                },
                "source":{
                    "firstSeenAt":"2026-07-27T00:00:00Z",
                    "lastVerifiedAt":"2026-07-27T00:00:00Z",
                    "glassVersion":"0.2.0",
                    "verificationCount":1
                },
                "confidence":"observed",
                "invalidation":{},
                "data":{"pageKind":"documentation"}
            }]
        }"#;
        let snapshot: KnowledgeStoreSnapshot = serde_json::from_str(legacy).unwrap();
        snapshot.validate().unwrap();
        let migrated = &snapshot.records[0];
        assert_eq!(migrated.source.surface.kind, KnowledgeSurfaceKind::Opaque);
        assert_eq!(migrated.source.backend.backend, KnowledgeBackendKind::Unknown);
        assert_eq!(migrated.portability, KnowledgePortability::NonPortable);
        assert_eq!(migrated.memory_influence, KnowledgeMemoryInfluence::None);
        assert_eq!(
            migrated.retrieval.current_validation.status,
            KnowledgeCurrentValidationStatus::NotValidated
        );
        let expected: Value = serde_json::from_str(legacy).unwrap();
        assert_eq!(serde_json::to_value(&snapshot).unwrap(), expected);
    }

    #[test]
    fn invalid_metadata_fails_closed_and_sensitive_profile_is_rejected() {
        let unknown = serde_json::from_str::<KnowledgeSurfaceKind>(r#""futureSurface""#);
        assert!(unknown.is_err());
        let mut base = record();
        base.source.backend.profile = "session-token-profile".into();
        let error = base.validate().unwrap_err();
        assert_eq!(error.path, "source.backend.profile");

        let mut influenced = record();
        influenced.source.backend.profile = "production".into();
        influenced.memory_influence = KnowledgeMemoryInfluence::RankingOnly;
        influenced.retrieval.current_validation = KnowledgeCurrentValidation::default();
        let error = influenced.validate().unwrap_err();
        assert_eq!(error.path, "memoryInfluence");
    }
    #[test]
    fn boolean_freshness_cannot_promote_without_witness() {
        let mut value = record();
        value.retrieval.current_validation = KnowledgeCurrentValidation::default();
        let before = value.clone();
        let error = value
            .transition(
                KnowledgeConfidence::Verified,
                "boolean is insufficient".into(),
                "2026-07-27T00:00:01Z".into(),
                true,
            )
            .unwrap_err();
        assert_eq!(error.path, "currentValidation");
        assert_eq!(value, before);
        value
            .transition_with_validation(
                KnowledgeConfidence::Verified,
                "revision witness".into(),
                "2026-07-27T00:00:01Z".into(),
                43,
                KnowledgeEvidenceQuality::Strong,
            )
            .unwrap();
        assert_eq!(value.confidence, KnowledgeConfidence::Verified);
    }

    #[test]
    fn fresh_transition_validates_memory_but_never_authorizes_mutation() {
        let mut record = record();
        assert_eq!(
            record.retrieval.current_validation.status,
            KnowledgeCurrentValidationStatus::Validated
        );
        record
            .transition(
                KnowledgeConfidence::Verified,
                "historical-only".into(),
                "2026-07-27T00:00:01Z".into(),
                false,
            )
            .unwrap_err();
        record
            .transition(
                KnowledgeConfidence::Verified,
                "fresh browser evidence".into(),
                "2026-07-27T00:00:01Z".into(),
                true,
            )
            .unwrap();
        assert_eq!(
            record.retrieval.current_validation.status,
            KnowledgeCurrentValidationStatus::Validated
        );
        assert!(record.data.get("reference").is_none());
        assert!(record.data.get("target").is_none());
    }
}
