//! Transport-neutral browser capability contracts.
//!
//! This module deliberately speaks only in Glass semantic terms.  A backend
//! adapter may translate these requests to CDP, WebDriver BiDi, or another
//! transport, but transport identifiers and command/domain types must not cross
//! this boundary.

use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::future::Future;
use std::pin::Pin;

pub const BROWSER_BACKEND_SCHEMA_VERSION: u32 = 1;
pub const MAX_BACKEND_ID_BYTES: usize = 128;
pub const MAX_VERSION_BYTES: usize = 128;
pub const MAX_BROWSER_FAMILY_BYTES: usize = 64;
pub const MAX_CAPABILITIES: usize = 32;
pub const MAX_DEPENDENCIES: usize = 16;
pub const MAX_LIMITATIONS: usize = 16;
pub const MAX_LIMITATION_BYTES: usize = 256;
pub const MAX_SELECTION_REQUIREMENTS: usize = 32;
pub const MAX_CONTEXTS: usize = 64;
pub const MAX_TEXT_BYTES: usize = 16 * 1024;

fn validate_text(field: &str, value: &str, max: usize) -> Result<(), BrowserBackendError> {
    if value.is_empty() {
        return Err(BrowserBackendError::InvalidConfiguration {
            field: field.into(),
            reason: "must not be empty".into(),
        });
    }
    if value.len() > max {
        return Err(BrowserBackendError::InvalidConfiguration {
            field: field.into(),
            reason: format!("must be at most {max} UTF-8 bytes"),
        });
    }
    if !value.is_char_boundary(value.len()) {
        return Err(BrowserBackendError::InvalidConfiguration {
            field: field.into(),
            reason: "must be valid UTF-8".into(),
        });
    }
    Ok(())
}

/// Semantic operations that a backend may expose.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum BrowserCapability {
    Navigation,
    Contexts,
    Evidence,
    Action,
    Effects,
    Script,
    Capture,
    Storage,
    Prompts,
    Downloads,
}

impl BrowserCapability {
    pub const ALL: [Self; 10] = [
        Self::Navigation,
        Self::Contexts,
        Self::Evidence,
        Self::Action,
        Self::Effects,
        Self::Script,
        Self::Capture,
        Self::Storage,
        Self::Prompts,
        Self::Downloads,
    ];
}

/// Support is intentionally bounded.  No unknown level is accepted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SupportLevel {
    Available,
    Partial,
    Restricted,
    Unavailable,
}

impl SupportLevel {
    pub fn satisfies(self, required: Self) -> bool {
        match required {
            Self::Available => self == Self::Available,
            Self::Partial => matches!(self, Self::Available | Self::Partial),
            Self::Restricted => matches!(self, Self::Available | Self::Partial | Self::Restricted),
            Self::Unavailable => true,
        }
    }
}

/// Whether a capability can be carried to another backend.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Portability {
    SemanticPortable,
    SurfacePortable,
    BackendCapabilityDependent,
    BackendSpecific,
    BrowserSpecific,
    NonPortable,
}

/// A capability needed by a semantic operation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CapabilityDependency {
    pub capability: BrowserCapability,
    pub minimum: SupportLevel,
    pub reason: String,
}

impl CapabilityDependency {
    pub fn validate(&self) -> Result<(), BrowserBackendError> {
        validate_text("capability dependency reason", &self.reason, MAX_LIMITATION_BYTES)
    }
}

/// One explicit capability declaration.  A missing map entry is not treated as
/// an available default; [`BackendProfile::capability`] reports it as omitted.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CapabilityDescriptor {
    pub level: SupportLevel,
    pub portability: Portability,
    #[serde(default)]
    pub dependencies: Vec<CapabilityDependency>,
    #[serde(default)]
    pub limitations: Vec<String>,
}

impl CapabilityDescriptor {
    pub fn validate(&self) -> Result<(), BrowserBackendError> {
        if self.dependencies.len() > MAX_DEPENDENCIES {
            return Err(invalid("capability dependencies", "too many entries"));
        }
        if self.limitations.len() > MAX_LIMITATIONS {
            return Err(invalid("capability limitations", "too many entries"));
        }
        for dependency in &self.dependencies {
            dependency.validate()?;
        }
        for limitation in &self.limitations {
            validate_text("capability limitation", limitation, MAX_LIMITATION_BYTES)?;
        }
        if self.level == SupportLevel::Unavailable && !self.dependencies.is_empty() {
            return Err(invalid(
                "capability dependencies",
                "unavailable capabilities cannot declare executable dependencies",
            ));
        }
        Ok(())
    }
}

fn unavailable_descriptor() -> CapabilityDescriptor {
    CapabilityDescriptor {
        level: SupportLevel::Unavailable,
        portability: Portability::NonPortable,
        dependencies: Vec::new(),
        limitations: vec!["capability was not declared by this backend".into()],
    }
}

/// Backend certification maturity.  The ordering is selection precedence only;
/// it does not make an experimental backend production-safe.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CertificationLevel {
    ProductionCertified,
    Experimental,
    Partial,
    Unsupported,
}

impl CertificationLevel {
    fn rank(self) -> u8 {
        match self {
            Self::ProductionCertified => 4,
            Self::Experimental => 3,
            Self::Partial => 2,
            Self::Unsupported => 1,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BrowserVersionRange {
    pub family: String,
    #[serde(default)]
    pub minimum: Option<String>,
    #[serde(default)]
    pub maximum: Option<String>,
}

impl BrowserVersionRange {
    pub fn validate(&self) -> Result<(), BrowserBackendError> {
        validate_text("browser family", &self.family, MAX_BROWSER_FAMILY_BYTES)?;
        if let Some(minimum) = &self.minimum {
            validate_text("browser minimum version", minimum, MAX_VERSION_BYTES)?;
        }
        if let Some(maximum) = &self.maximum {
            validate_text("browser maximum version", maximum, MAX_VERSION_BYTES)?;
        }
        if let (Some(minimum), Some(maximum)) = (&self.minimum, &self.maximum)
            && compare_versions(minimum, maximum) == Ordering::Greater
        {
            return Err(invalid("browser version range", "minimum exceeds maximum"));
        }
        Ok(())
    }

    fn contains(&self, family: &str, version: Option<&str>) -> bool {
        if self.family != family {
            return false;
        }
        let Some(version) = version else {
            return true;
        };
        self.minimum
            .as_deref()
            .is_none_or(|minimum| compare_versions(version, minimum) != Ordering::Less)
            && self
                .maximum
                .as_deref()
                .is_none_or(|maximum| compare_versions(version, maximum) != Ordering::Greater)
    }
}

fn compare_versions(left: &str, right: &str) -> Ordering {
    let left_parts = left.split(['.', '-', '+']).collect::<Vec<_>>();
    let right_parts = right.split(['.', '-', '+']).collect::<Vec<_>>();
    for (left_part, right_part) in left_parts.iter().zip(right_parts.iter()) {
        match (left_part.parse::<u64>(), right_part.parse::<u64>()) {
            (Ok(left), Ok(right)) => match left.cmp(&right) {
                Ordering::Equal => continue,
                order => return order,
            },
            _ => match left_part.cmp(right_part) {
                Ordering::Equal => continue,
                order => return order,
            },
        }
    }
    left_parts.len().cmp(&right_parts.len())
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CertificationProfile {
    pub level: CertificationLevel,
    pub glass_version: String,
    #[serde(default)]
    pub tested_capabilities: Vec<BrowserCapability>,
    #[serde(default)]
    pub limitations: Vec<String>,
}

impl CertificationProfile {
    pub fn validate(&self) -> Result<(), BrowserBackendError> {
        validate_text("certification glass version", &self.glass_version, MAX_VERSION_BYTES)?;
        if self.tested_capabilities.len() > MAX_CAPABILITIES {
            return Err(invalid("certification tested capabilities", "too many entries"));
        }
        if self.limitations.len() > MAX_LIMITATIONS {
            return Err(invalid("certification limitations", "too many entries"));
        }
        for limitation in &self.limitations {
            validate_text("certification limitation", limitation, MAX_LIMITATION_BYTES)?;
        }
        if self.level == CertificationLevel::ProductionCertified && self.tested_capabilities.is_empty() {
            return Err(invalid(
                "certification tested capabilities",
                "production certification requires conformance coverage",
            ));
        }
        if self.level == CertificationLevel::Unsupported && self.tested_capabilities.iter().any(|capability| {
            capability != &BrowserCapability::Contexts
        }) {
            return Err(invalid(
                "certification tested capabilities",
                "unsupported backends cannot claim tested semantic capabilities",
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BackendIdentity {
    pub backend_id: String,
    pub version: String,
    pub browser: BrowserVersionRange,
    pub certification: CertificationProfile,
}

impl BackendIdentity {
    pub fn validate(&self) -> Result<(), BrowserBackendError> {
        validate_text("backend id", &self.backend_id, MAX_BACKEND_ID_BYTES)?;
        validate_text("backend version", &self.version, MAX_VERSION_BYTES)?;
        self.browser.validate()?;
        self.certification.validate()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BackendProfile {
    pub schema_version: u32,
    pub identity: BackendIdentity,
    #[serde(default)]
    pub capabilities: BTreeMap<BrowserCapability, CapabilityDescriptor>,
}

impl BackendProfile {
    pub fn validate(&self) -> Result<(), BrowserBackendError> {
        if self.schema_version != BROWSER_BACKEND_SCHEMA_VERSION {
            return Err(invalid("schema version", "unsupported browser backend schema"));
        }
        self.identity.validate()?;
        if self.capabilities.len() > MAX_CAPABILITIES {
            return Err(invalid("capabilities", "too many entries"));
        }
        for descriptor in self.capabilities.values() {
            descriptor.validate()?;
        }
        if self.identity.certification.level == CertificationLevel::Unsupported
            && self.capabilities.values().any(|descriptor| descriptor.level != SupportLevel::Unavailable)
        {
            return Err(invalid(
                "capabilities",
                "unsupported certification cannot advertise available capabilities",
            ));
        }
        Ok(())
    }

    pub fn capability(&self, capability: BrowserCapability) -> CapabilityDescriptor {
        self.capabilities
            .get(&capability)
            .cloned()
            .unwrap_or_else(unavailable_descriptor)
    }

    pub fn require(
        &self,
        capability: BrowserCapability,
        minimum: SupportLevel,
    ) -> Result<(), BrowserBackendError> {
        let descriptor = self.capability(capability);
        if descriptor.level.satisfies(minimum) {
            return Ok(());
        }
        Err(BrowserBackendError::CapabilityUnavailable {
            capability,
            required: minimum,
            actual: descriptor.level,
            declared: self.capabilities.contains_key(&capability),
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CapabilityRequirement {
    pub capability: BrowserCapability,
    pub minimum: SupportLevel,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BackendSelectionRequest {
    pub schema_version: u32,
    #[serde(default)]
    pub preferred_backend_id: Option<String>,
    #[serde(default)]
    pub required_capabilities: Vec<CapabilityRequirement>,
    #[serde(default = "default_minimum_certification")]
    pub minimum_certification: CertificationLevel,
    #[serde(default)]
    pub browser_family: Option<String>,
    #[serde(default)]
    pub browser_version: Option<String>,
}

fn default_minimum_certification() -> CertificationLevel {
    CertificationLevel::Partial
}

impl BackendSelectionRequest {
    pub fn validate(&self) -> Result<(), BrowserBackendError> {
        if self.schema_version != BROWSER_BACKEND_SCHEMA_VERSION {
            return Err(invalid("schema version", "unsupported browser backend schema"));
        }
        if let Some(id) = &self.preferred_backend_id {
            validate_text("preferred backend id", id, MAX_BACKEND_ID_BYTES)?;
        }
        if self.required_capabilities.len() > MAX_SELECTION_REQUIREMENTS {
            return Err(invalid("required capabilities", "too many entries"));
        }
        if let Some(family) = &self.browser_family {
            validate_text("browser family", family, MAX_BROWSER_FAMILY_BYTES)?;
        }
        if let Some(version) = &self.browser_version {
            validate_text("browser version", version, MAX_VERSION_BYTES)?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BackendSelectionResult {
    pub schema_version: u32,
    pub selected: BackendProfile,
    pub reason: SelectionReason,
    pub considered_backend_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SelectionReason {
    ExplicitPreference,
    CertificationThenCapability,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SelectionRejection {
    InvalidProfile,
    CertificationBelowMinimum,
    BrowserMismatch,
    MissingCapability(BrowserCapability),
}

/// Selects without iteration-order dependence.  An explicit preference is
/// strict: an unusable preferred backend is an error, never a silent fallback.
pub fn select_backend(
    request: &BackendSelectionRequest,
    profiles: &[BackendProfile],
) -> Result<BackendSelectionResult, BrowserBackendError> {
    request.validate()?;
    if profiles.is_empty() {
        return Err(BrowserBackendError::SelectionFailed {
            reason: "no backend profiles were provided".into(),
        });
    }
    let mut seen_backend_ids = BTreeSet::new();
    for profile in profiles {
        if !seen_backend_ids.insert(profile.identity.backend_id.as_str()) {
            return Err(invalid("backend profiles", "duplicate backend id"));
        }
    }

    let mut considered_backend_ids = profiles
        .iter()
        .map(|profile| profile.identity.backend_id.clone())
        .collect::<Vec<_>>();
    considered_backend_ids.sort();
    considered_backend_ids.dedup();

    let mut eligible = Vec::new();
    let mut preferred_rejection = None;
    for profile in profiles {
        let id = &profile.identity.backend_id;
        if let Err(error) = profile.validate() {
            if request.preferred_backend_id.as_ref() == Some(id) {
                preferred_rejection = Some(error.to_string());
            }
            continue;
        }
        let rejection = if profile.identity.certification.level.rank()
            < request.minimum_certification.rank()
        {
            Some(SelectionRejection::CertificationBelowMinimum)
        } else if let Some(family) = request.browser_family.as_deref()
            && !profile.identity.browser.contains(family, request.browser_version.as_deref())
        {
            Some(SelectionRejection::BrowserMismatch)
        } else {
            request.required_capabilities.iter().find_map(|requirement| {
                (!profile
                    .capability(requirement.capability)
                    .level
                    .satisfies(requirement.minimum))
                .then_some(SelectionRejection::MissingCapability(requirement.capability))
            })
        };
        if let Some(rejection) = rejection {
            if request.preferred_backend_id.as_ref() == Some(id) {
                preferred_rejection = Some(format_selection_rejection(&rejection));
            }
        } else {
            eligible.push(profile);
        }
    }

    if let Some(preferred) = &request.preferred_backend_id {
        let Some(profile) = eligible.iter().find(|profile| &profile.identity.backend_id == preferred) else {
            return Err(BrowserBackendError::SelectionFailed {
                reason: preferred_rejection
                    .unwrap_or_else(|| format!("preferred backend `{preferred}` is not available")),
            });
        };
        return Ok(BackendSelectionResult {
            schema_version: BROWSER_BACKEND_SCHEMA_VERSION,
            selected: (*profile).clone(),
            reason: SelectionReason::ExplicitPreference,
            considered_backend_ids,
        });
    }

    eligible.sort_by(|left, right| {
        right
            .identity
            .certification
            .level
            .rank()
            .cmp(&left.identity.certification.level.rank())
            .then_with(|| capability_score(right).cmp(&capability_score(left)))
            .then_with(|| left.identity.backend_id.cmp(&right.identity.backend_id))
            .then_with(|| left.identity.version.cmp(&right.identity.version))
    });
    let Some(selected) = eligible.first() else {
        return Err(BrowserBackendError::SelectionFailed {
            reason: "no backend satisfies the requested capabilities and policy".into(),
        });
    };
    Ok(BackendSelectionResult {
        schema_version: BROWSER_BACKEND_SCHEMA_VERSION,
        selected: (*selected).clone(),
        reason: SelectionReason::CertificationThenCapability,
        considered_backend_ids,
    })
}

fn capability_score(profile: &BackendProfile) -> u16 {
    BrowserCapability::ALL
        .iter()
        .map(|capability| match profile.capability(*capability).level {
            SupportLevel::Available => 4,
            SupportLevel::Partial => 2,
            SupportLevel::Restricted => 1,
            SupportLevel::Unavailable => 0,
        })
        .sum()
}

fn format_selection_rejection(rejection: &SelectionRejection) -> String {
    match rejection {
        SelectionRejection::InvalidProfile => "invalid backend profile".into(),
        SelectionRejection::CertificationBelowMinimum => "certification is below minimum".into(),
        SelectionRejection::BrowserMismatch => "browser version range does not match".into(),
        SelectionRejection::MissingCapability(capability) => {
            format!("required capability {capability:?} is unavailable")
        }
    }
}

fn invalid(field: &str, reason: &str) -> BrowserBackendError {
    BrowserBackendError::InvalidConfiguration {
        field: field.into(),
        reason: reason.into(),
    }
}

/// Stable failures shared by every backend adapter.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "details", rename_all = "camelCase")]
pub enum BrowserBackendError {
    CapabilityUnavailable {
        capability: BrowserCapability,
        required: SupportLevel,
        actual: SupportLevel,
        declared: bool,
    },
    InvalidConfiguration {
        field: String,
        reason: String,
    },
    Connection {
        operation: String,
        reason: String,
    },
    Lifecycle {
        operation: String,
        state: String,
        reason: String,
    },
    UnsupportedOperation {
        operation: String,
        reason: String,
    },
    SelectionFailed {
        reason: String,
    },
}

impl fmt::Display for BrowserBackendError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::CapabilityUnavailable { capability, .. } => {
                write!(formatter, "capability unavailable: {capability:?}")
            }
            Self::InvalidConfiguration { field, reason } => write!(formatter, "invalid {field}: {reason}"),
            Self::Connection { operation, reason } => write!(formatter, "connection failure during {operation}: {reason}"),
            Self::Lifecycle { operation, state, reason } => write!(formatter, "lifecycle failure during {operation} ({state}): {reason}"),
            Self::UnsupportedOperation { operation, reason } => write!(formatter, "unsupported operation {operation}: {reason}"),
            Self::SelectionFailed { reason } => write!(formatter, "backend selection failed: {reason}"),
        }
    }
}

impl std::error::Error for BrowserBackendError {}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NavigationRequest {
    pub url: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NavigationResult {
    pub url: String,
    pub revision: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ContextRequest {
    #[serde(default)]
    pub include_background: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BrowsingContext {
    pub context_id: String,
    pub url: String,
    pub active: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EvidenceRequest {
    pub context_id: String,
    pub level: EvidenceLevel,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum EvidenceLevel {
    Compact,
    Deep,
    Screenshot,
    Combined,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EvidenceResult {
    pub context_id: String,
    pub revision: u64,
    pub url: String,
    pub title: String,
    pub visible_text: String,
    pub complete: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ActionRequest {
    pub context_id: String,
    pub action: SemanticAction,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SemanticAction {
    Click { target: String },
    Type { target: String, text: String },
    KeyPress { key: String },
    Scroll { delta_x: i32, delta_y: i32 },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ActionResult {
    pub context_id: String,
    pub revision: u64,
    pub accepted: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EffectsRequest {
    pub context_id: String,
    pub since_revision: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EffectsResult {
    pub context_id: String,
    pub revision: u64,
    pub changed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ScriptRequest {
    pub context_id: String,
    pub source: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ScriptResult {
    pub value: serde_json::Value,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CaptureFormat {
    Png,
    Jpeg,
    Pdf,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CaptureRequest {
    pub context_id: String,
    pub format: CaptureFormat,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CaptureResult {
    pub format: CaptureFormat,
    pub bytes: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum StorageScope {
    Cookies,
    Local,
    Session,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum StorageOperation {
    Read,
    Write { key: String, value: String },
    Clear,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct StorageRequest {
    pub context_id: String,
    pub scope: StorageScope,
    pub operation: StorageOperation,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct StorageResult {
    pub entries: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PromptDecision {
    Accept,
    Dismiss,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PromptRequest {
    pub context_id: String,
    pub decision: PromptDecision,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PromptResult {
    pub handled: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DownloadOperation {
    List,
    Cancel { download_id: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DownloadRequest {
    pub context_id: String,
    pub operation: DownloadOperation,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DownloadResult {
    pub download_ids: Vec<String>,
}

pub type BackendFuture<'a, T> = Pin<Box<dyn Future<Output = Result<T, BrowserBackendError>> + Send + 'a>>;

/// Object-safe asynchronous semantic backend boundary.
///
/// Implementations MUST call [`BackendProfile::require`] before an operation;
/// this prevents a missing optional capability from silently becoming a
/// weaker operation.  Adapters may use any transport internally.
pub trait BrowserBackend: Send + Sync {
    fn profile(&self) -> &BackendProfile;
    fn initialize<'a>(&'a self) -> BackendFuture<'a, ()>;
    fn close<'a>(&'a self) -> BackendFuture<'a, ()>;
    fn navigate<'a>(&'a self, request: NavigationRequest) -> BackendFuture<'a, NavigationResult>;
    fn contexts<'a>(&'a self, request: ContextRequest) -> BackendFuture<'a, Vec<BrowsingContext>>;
    fn evidence<'a>(&'a self, request: EvidenceRequest) -> BackendFuture<'a, EvidenceResult>;
    fn action<'a>(&'a self, request: ActionRequest) -> BackendFuture<'a, ActionResult>;
    fn effects<'a>(&'a self, request: EffectsRequest) -> BackendFuture<'a, EffectsResult>;
    fn script<'a>(&'a self, request: ScriptRequest) -> BackendFuture<'a, ScriptResult>;
    fn capture<'a>(&'a self, request: CaptureRequest) -> BackendFuture<'a, CaptureResult>;
    fn storage<'a>(&'a self, request: StorageRequest) -> BackendFuture<'a, StorageResult>;
    fn prompt<'a>(&'a self, request: PromptRequest) -> BackendFuture<'a, PromptResult>;
    fn download<'a>(&'a self, request: DownloadRequest) -> BackendFuture<'a, DownloadResult>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn profile(id: &str, certification: CertificationLevel) -> BackendProfile {
        let mut capabilities = BTreeMap::new();
        for capability in BrowserCapability::ALL {
            capabilities.insert(
                capability,
                CapabilityDescriptor {
                    level: SupportLevel::Available,
                    portability: Portability::SemanticPortable,
                    dependencies: Vec::new(),
                    limitations: Vec::new(),
                },
            );
        }
        BackendProfile {
            schema_version: BROWSER_BACKEND_SCHEMA_VERSION,
            identity: BackendIdentity {
                backend_id: id.into(),
                version: "1.0.0".into(),
                browser: BrowserVersionRange {
                    family: "chromium".into(),
                    minimum: Some("120.0.0".into()),
                    maximum: Some("160.0.0".into()),
                },
                certification: CertificationProfile {
                    level: certification,
                    glass_version: "0.3.1".into(),
                    tested_capabilities: if certification == CertificationLevel::ProductionCertified {
                        BrowserCapability::ALL.to_vec()
                    } else {
                        Vec::new()
                    },
                    limitations: Vec::new(),
                },
            },
            capabilities,
        }
    }

    #[test]
    fn profile_serialization_is_stable_and_camel_case() {
        let value = serde_json::to_value(profile("cdp", CertificationLevel::ProductionCertified)).unwrap();
        assert_eq!(value["schemaVersion"], json!(1));
        assert_eq!(value["identity"]["backendId"], json!("cdp"));
        assert_eq!(value["identity"]["certification"]["level"], json!("productionCertified"));
        assert_eq!(value["capabilities"]["navigation"]["level"], json!("available"));
        assert!(serde_json::from_value::<BackendProfile>(value).is_ok());
    }

    #[test]
    fn selection_precedence_is_explicit_then_certification_then_capability() {
        let production = profile("cdp", CertificationLevel::ProductionCertified);
        let experimental = profile("bidi", CertificationLevel::Experimental);
        let request = BackendSelectionRequest {
            schema_version: 1,
            preferred_backend_id: Some("bidi".into()),
            required_capabilities: vec![CapabilityRequirement {
                capability: BrowserCapability::Evidence,
                minimum: SupportLevel::Available,
            }],
            minimum_certification: CertificationLevel::Partial,
            browser_family: Some("chromium".into()),
            browser_version: Some("150.0.0".into()),
        };
        let selected = select_backend(&request, &[production.clone(), experimental]).unwrap();
        assert_eq!(selected.selected.identity.backend_id, "bidi");
        assert_eq!(selected.reason, SelectionReason::ExplicitPreference);

        let mut automatic = request;
        automatic.preferred_backend_id = None;
        let selected = select_backend(&automatic, &[profile("bidi", CertificationLevel::Experimental), production]).unwrap();
        assert_eq!(selected.selected.identity.backend_id, "cdp");
        assert_eq!(selected.reason, SelectionReason::CertificationThenCapability);
    }

    #[test]
    fn omitted_capability_is_explicit_and_fails_closed() {
        let mut backend = profile("partial", CertificationLevel::Partial);
        backend.capabilities.remove(&BrowserCapability::Downloads);
        let descriptor = backend.capability(BrowserCapability::Downloads);
        assert_eq!(descriptor.level, SupportLevel::Unavailable);
        assert!(matches!(
            backend.require(BrowserCapability::Downloads, SupportLevel::Partial),
            Err(BrowserBackendError::CapabilityUnavailable { declared: false, .. })
        ));
    }

    #[test]
    fn production_certification_requires_conformance_evidence() {
        let mut backend = profile("bad", CertificationLevel::ProductionCertified);
        backend.identity.certification.tested_capabilities.clear();
        assert!(matches!(backend.validate(), Err(BrowserBackendError::InvalidConfiguration { field, .. }) if field == "certification tested capabilities"));
    }

    #[test]
    fn typed_errors_round_trip_without_losing_kind() {
        let errors = [
            BrowserBackendError::CapabilityUnavailable {
                capability: BrowserCapability::Evidence,
                required: SupportLevel::Available,
                actual: SupportLevel::Unavailable,
                declared: false,
            },
            BrowserBackendError::InvalidConfiguration { field: "url".into(), reason: "empty".into() },
            BrowserBackendError::Connection { operation: "initialize".into(), reason: "refused".into() },
            BrowserBackendError::Lifecycle { operation: "close".into(), state: "closing".into(), reason: "timeout".into() },
            BrowserBackendError::UnsupportedOperation { operation: "capture".into(), reason: "not implemented".into() },
        ];
        for error in errors {
            let encoded = serde_json::to_value(&error).unwrap();
            assert!(encoded.get("kind").is_some());
            assert_eq!(serde_json::from_value::<BrowserBackendError>(encoded).unwrap(), error);
        }
    }
}
