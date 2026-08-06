//! Stable, bounded, transport-neutral browser surface contracts.
//!
//! A surface is an evidenced boundary in a browser-hosted experience.  Surface
//! detection is not an action grant: callers must declare capabilities backed by
//! current evidence and validate the complete contract before using it.

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::net::Ipv6Addr;
use std::fmt::{Display, Formatter};

/// Schema version for the multi-surface contract.
pub const SURFACE_SCHEMA_VERSION: u32 = 1;
/// Maximum number of surfaces in one bounded surface set.
pub const MAX_SURFACES: usize = 256;
/// Maximum nesting depth, including the root at depth zero.
pub const MAX_SURFACE_NESTING_DEPTH: u16 = 32;
/// Maximum bytes in a surface or extension identifier component.
pub const MAX_SURFACE_IDENTIFIER_BYTES: usize = 128;
/// Maximum capabilities declared by one surface.
pub const MAX_SURFACE_CAPABILITIES: usize = 16;
/// Maximum evidence records declared by one surface.
pub const MAX_SURFACE_EVIDENCE: usize = 32;
/// Maximum diagnostics declared by one surface.
pub const MAX_SURFACE_DIAGNOSTICS: usize = 64;
/// Maximum diagnostic code bytes.
pub const MAX_SURFACE_DIAGNOSTIC_CODE_BYTES: usize = 64;
/// Maximum diagnostic message bytes.
pub const MAX_SURFACE_DIAGNOSTIC_MESSAGE_BYTES: usize = 512;
/// Maximum optional evidence detail bytes.
pub const MAX_SURFACE_EVIDENCE_DETAIL_BYTES: usize = 256;
/// Maximum canonical serialized surface payload.
pub const MAX_SURFACE_PAYLOAD_BYTES: usize = 64 * 1024;
/// Maximum bytes in a provenance source identifier.
pub const MAX_SURFACE_PROVENANCE_ID_BYTES: usize = 128;
/// Maximum bytes in an adapter/backend/bridge version.
pub const MAX_SURFACE_PROVENANCE_VERSION_BYTES: usize = 64;
/// Maximum bytes in an observation timestamp.
pub const MAX_SURFACE_PROVENANCE_TIMESTAMP_BYTES: usize = 64;
/// Maximum bridge grants held by one trusted registry.
pub const MAX_BRIDGE_GRANTS: usize = 128;
/// Maximum serialized trusted bridge grant registry.
pub const MAX_BRIDGE_GRANT_PAYLOAD_BYTES: usize = 32 * 1024;

/// A stable identity for one surface boundary.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SurfaceId(String);

impl SurfaceId {
    /// Construct and validate a surface identifier.
    pub fn new(value: impl Into<String>) -> Result<Self, SurfaceContractError> {
        let value = value.into();
        validate_namespaced_component("surfaceId", &value)?;
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl From<SurfaceId> for String {
    fn from(value: SurfaceId) -> Self {
        value.0
    }
}

/// A namespaced, versioned extension identifier.  The namespace is part of
/// identity, so an extension cannot collide with a core or another vendor's
/// concept.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ExtensionIdentifier {
    pub schema_version: u32,
    pub namespace: String,
    pub name: String,
    pub version: String,
}

impl ExtensionIdentifier {
    pub fn new(
        namespace: impl Into<String>,
        name: impl Into<String>,
    ) -> Result<Self, SurfaceContractError> {
        Self::new_versioned(namespace, name, "1")
    }

    pub fn new_versioned(
        namespace: impl Into<String>,
        name: impl Into<String>,
        version: impl Into<String>,
    ) -> Result<Self, SurfaceContractError> {
        let identifier = Self {
            schema_version: SURFACE_SCHEMA_VERSION,
            namespace: namespace.into(),
            name: name.into(),
            version: version.into(),
        };
        identifier.validate()?;
        Ok(identifier)
    }

    pub fn validate(&self) -> Result<(), SurfaceContractError> {
        if self.schema_version != SURFACE_SCHEMA_VERSION {
            return Err(SurfaceContractError::new(
                "extension.schemaVersion",
                "unsupported extension identifier schema version",
            ));
        }
        validate_extension_component("extension.namespace", &self.namespace)?;
        validate_extension_component("extension.name", &self.name)?;
        validate_bounded_text(
            "extension.version",
            &self.version,
            MAX_SURFACE_PROVENANCE_VERSION_BYTES,
        )
    }

    pub fn qualified_name(&self) -> String {
        format!("{}:{}@{}", self.namespace, self.name, self.version)
    }
}

/// Initial bounded surface kinds.  `Unknown` is an honest classification, not
/// an escape hatch for arbitrary unvalidated kind strings.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub enum SurfaceKind {
    Document,
    Accessibility,
    ShadowDocument,
    FrameDocument,
    Svg,
    Canvas2d,
    Webgl,
    Webgpu,
    EmbeddedDocument,
    Pdf,
    Media,
    Terminal,
    RemoteApplication,
    BrowserNative,
    ExtensionDefined { extension: ExtensionIdentifier },
    Unknown,
    Opaque,
}

/// Capability classes are declarations backed by evidence, never inferred
/// solely from [`SurfaceKind`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub enum SurfaceCapability {
    ReadStructure,
    ReadText,
    ReadRelations,
    ReadState,
    SemanticAction,
    CoordinateAction,
    Input,
    KeyboardInput,
    PointerInput,
    Capture,
    Extraction,
    Bridge,
    RevisionObservation,
    Verification,
}

/// Explicit progressive-understanding level, from detection to compilation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[repr(u8)]
pub enum UnderstandingLevel {
    Opaque = 0,
    CoordinateOnly = 1,
    Structural = 2,
    Semantic = 3,
    TaskCompilable = 4,
}

impl UnderstandingLevel {
    pub const fn value(self) -> u8 {
        self as u8
    }
}

/// Evidence quality for one coverage dimension.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CoverageLevel {
    Opaque,
    Partial,
    Strong,
    Complete,
}

/// Whether interaction semantics are available for a surface.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum InteractionCoverage {
    Unavailable,
    CoordinateOnly,
    Semantic,
    TaskCompilable,
}

/// Coverage is independent from kind and must be supported by evidence.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SurfaceCoverage {
    pub structural: CoverageLevel,
    pub semantic: CoverageLevel,
    pub interaction: InteractionCoverage,
}

impl SurfaceCoverage {
    pub const OPAQUE: Self = Self {
        structural: CoverageLevel::Opaque,
        semantic: CoverageLevel::Opaque,
        interaction: InteractionCoverage::Unavailable,
    };

    fn validate(&self, level: UnderstandingLevel) -> Result<(), SurfaceContractError> {
        if level >= UnderstandingLevel::Structural
            && self.structural == CoverageLevel::Opaque
        {
            return Err(SurfaceContractError::new(
                "coverage.structural",
                "structural understanding requires structural coverage",
            ));
        }
        if level >= UnderstandingLevel::Semantic && self.semantic == CoverageLevel::Opaque {
            return Err(SurfaceContractError::new(
                "coverage.semantic",
                "semantic understanding requires semantic coverage",
            ));
        }
        if level <= UnderstandingLevel::CoordinateOnly && self.semantic > CoverageLevel::Partial {
            return Err(SurfaceContractError::new(
                "coverage.semantic",
                "opaque or coordinate-only understanding cannot claim strong semantic coverage",
            ));
        }
        if level == UnderstandingLevel::Opaque
            && self.interaction != InteractionCoverage::Unavailable
        {
            return Err(SurfaceContractError::new(
                "coverage.interaction",
                "opaque understanding cannot claim interaction coverage",
            ));
        }
        if level == UnderstandingLevel::CoordinateOnly
            && self.interaction < InteractionCoverage::CoordinateOnly
        {
            return Err(SurfaceContractError::new(
                "coverage.interaction",
                "coordinate-only understanding requires coordinate interaction coverage",
            ));
        }
        if self.interaction == InteractionCoverage::TaskCompilable
            && level < UnderstandingLevel::Semantic
        {
            return Err(SurfaceContractError::new(
                "coverage.interaction",
                "task-compilable interaction requires semantic understanding",
            ));
        }
        if level == UnderstandingLevel::TaskCompilable
            && self.interaction != InteractionCoverage::TaskCompilable
        {
            return Err(SurfaceContractError::new(
                "coverage.interaction",
                "task-compilable understanding requires task-compilable interaction coverage",
            ));
        }
        Ok(())
    }
}

/// Class of the component that supplied one evidence record.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub enum ProvenanceSourceClass {
    LiveWebIr,
    Backend,
    Bridge,
    Memory,
    Visual,
}
/// Trust established for a page-provided semantic bridge.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum BridgeTrustLevel {
    Untrusted,
    OriginValidated,
    CapabilityGranted,
}
/// An independently issued bridge capability grant.  A surface payload may
/// carry only the opaque token; the registry is supplied by the trusted host.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BridgeCapabilityGrant {
    pub token: String,
    pub origin: String,
    pub capabilities: Vec<SurfaceCapability>,
}

/// Host-side registry of independently validated bridge grants.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BridgeGrantRegistry {
    pub grants: BTreeMap<String, BridgeCapabilityGrant>,
}

impl BridgeCapabilityGrant {
    fn validate(&self, path: &str) -> Result<(), SurfaceContractError> {
        validate_identifier_with_max(
            &format!("{path}.token"),
            &self.token,
            MAX_SURFACE_PROVENANCE_ID_BYTES,
        )?;
        validate_origin(&format!("{path}.origin"), &self.origin)?;
        let mut capabilities = BTreeSet::new();
        for capability in &self.capabilities {
            if !capabilities.insert(*capability) {
                return Err(SurfaceContractError::new(
                    format!("{path}.capabilities"),
                    "grant capabilities must not be duplicated",
                ));
            }
        }
        Ok(())
    }
}

impl BridgeGrantRegistry {
    pub fn validate(&self) -> Result<(), SurfaceContractError> {
        if self.grants.len() > MAX_BRIDGE_GRANTS {
            return Err(SurfaceContractError::new(
                "grants",
                "bridge grants exceed the registry bound",
            ));
        }
        for (token, grant) in &self.grants {
            if token != &grant.token {
                return Err(SurfaceContractError::new(
                    "bridgeGrant.token",
                    "grant registry key must match grant token",
                ));
            }
            grant.validate("bridgeGrant")?;
        }
        let payload = serde_json::to_vec(self).map_err(|error| {
            SurfaceContractError::new(
                "grants",
                format!("failed to serialize bridge grant registry: {error}"),
            )
        })?;
        if payload.len() > MAX_BRIDGE_GRANT_PAYLOAD_BYTES {
            return Err(SurfaceContractError::new(
                "grants",
                "bridge grant registry payload exceeds the contract bound",
            ));
        }
        Ok(())
    }

    pub fn from_json(input: &str) -> Result<Self, SurfaceContractError> {
        if input.len() > MAX_BRIDGE_GRANT_PAYLOAD_BYTES {
            return Err(SurfaceContractError::new(
                "$",
                "bridge grant registry payload exceeds the contract bound",
            ));
        }
        let registry: Self = serde_json::from_str(input).map_err(|error| {
            SurfaceContractError::new("$", format!("invalid bridge grant registry: {error}"))
        })?;
        registry.validate()?;
        Ok(registry)
    }

    pub fn insert(&mut self, grant: BridgeCapabilityGrant) -> Result<(), SurfaceContractError> {
        grant.validate("bridgeGrant")?;
        if self.grants.contains_key(&grant.token) {
            return Err(SurfaceContractError::new(
                "bridgeGrant.token",
                "bridge grant token is already registered",
            ));
        }
        if self.grants.len() >= MAX_BRIDGE_GRANTS {
            return Err(SurfaceContractError::new(
                "grants",
                "bridge grants exceed the registry bound",
            ));
        }
        let mut candidate = self.clone();
        candidate.grants.insert(grant.token.clone(), grant.clone());
        candidate.validate()?;
        let token = grant.token.clone();
        self.grants.insert(token, grant);
        Ok(())
    }
}

/// Revision-bound, machine-readable evidence provenance.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SurfaceProvenance {
    pub schema_version: u32,
    pub source_class: ProvenanceSourceClass,
    pub source_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub backend: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub backend_version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub adapter_version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bridge_version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bridge_origin: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bridge_trust: Option<BridgeTrustLevel>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub grant_token: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub bridge_capabilities: Vec<SurfaceCapability>,
    pub source_revision: SurfaceRevision,
    pub observed_at: String,
    pub validated_at: String,
}
impl SurfaceProvenance {
    fn validate(
        &self,
        path: &str,
        surface_revision: SurfaceRevision,
        source: SurfaceEvidenceSource,
    ) -> Result<(), SurfaceContractError> {
        if self.schema_version != SURFACE_SCHEMA_VERSION {
            return Err(SurfaceContractError::new(
                format!("{path}.schemaVersion"),
                "unsupported provenance schema version",
            ));
        }
        validate_identifier_with_max(
            &format!("{path}.sourceId"),
            &self.source_id,
            MAX_SURFACE_PROVENANCE_ID_BYTES,
        )?;
        if self.source_revision != surface_revision {
            return Err(SurfaceContractError::new(
                format!("{path}.sourceRevision"),
                "evidence provenance revision must match the surface revision",
            ));
        }
        validate_canonical_timestamp(
            &format!("{path}.observedAt"),
            &self.observed_at,
        )?;
        validate_canonical_timestamp(
            &format!("{path}.validatedAt"),
            &self.validated_at,
        )?;
        if self.validated_at < self.observed_at {
            return Err(SurfaceContractError::new(
                format!("{path}.validatedAt"),
                "validatedAt must not precede observedAt",
            ));
        }
        for (field, value) in [
            ("backend", self.backend.as_deref()),
            ("backendVersion", self.backend_version.as_deref()),
            ("adapterVersion", self.adapter_version.as_deref()),
            ("bridgeVersion", self.bridge_version.as_deref()),
        ] {
            if let Some(value) = value {
                validate_bounded_text(
                    &format!("{path}.{field}"),
                    value,
                    MAX_SURFACE_PROVENANCE_VERSION_BYTES,
                )?;
            }
        }
        match source {
            SurfaceEvidenceSource::Bridge | SurfaceEvidenceSource::Extension => {
                if self.source_class != ProvenanceSourceClass::Bridge
                    || self.bridge_version.is_none()
                    || self.bridge_origin.is_none()
                    || self.bridge_trust < Some(BridgeTrustLevel::OriginValidated)
                {
                    return Err(SurfaceContractError::new(
                        path,
                        "bridge evidence requires validated origin, trust, and bridgeVersion",
                    ));
                }
                if self.bridge_trust == Some(BridgeTrustLevel::CapabilityGranted)
                    && self.grant_token.is_none()
                {
                    return Err(SurfaceContractError::new(
                        path,
                        "capability-granted bridge evidence requires an independent grant token",
                    ));
                }
                if let Some(token) = &self.grant_token {
                    validate_identifier_with_max(
                        &format!("{path}.grantToken"),
                        token,
                        MAX_SURFACE_PROVENANCE_ID_BYTES,
                    )?;
                }
                validate_origin(
                    &format!("{path}.bridgeOrigin"),
                    self.bridge_origin.as_deref().unwrap(),
                )?;
            }
            SurfaceEvidenceSource::Visual => {
                if self.source_class != ProvenanceSourceClass::Visual {
                    return Err(SurfaceContractError::new(
                        path,
                        "visual evidence requires visual provenance",
                    ));
                }
            }
            SurfaceEvidenceSource::Memory => {
                if self.source_class != ProvenanceSourceClass::Memory {
                    return Err(SurfaceContractError::new(
                        path,
                        "memory evidence requires memory provenance",
                    ));
                }
            }
            _ => {
                if !matches!(
                    self.source_class,
                    ProvenanceSourceClass::LiveWebIr | ProvenanceSourceClass::Backend
                ) {
                    return Err(SurfaceContractError::new(
                        path,
                        "provenance source class is incompatible with evidence source",
                    ));
                }
            }
        }
        if self.source_class == ProvenanceSourceClass::Backend
            && (self.backend.is_none() || self.backend_version.is_none())
        {
            return Err(SurfaceContractError::new(
                path,
                "backend provenance requires backend and backendVersion",
            ));
        }
        let mut granted = BTreeSet::new();
        for capability in &self.bridge_capabilities {
            if !granted.insert(*capability) {
                return Err(SurfaceContractError::new(
                    format!("{path}.bridgeCapabilities"),
                    "bridge capabilities must not be duplicated",
                ));
            }
        }
        Ok(())
    }
}

/// Source class for one bounded surface observation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub enum SurfaceEvidenceSource {
    Dom,
    Accessibility,
    Layout,
    CanvasDetection,
    Svg,
    Frame,
    ShadowDom,
    EmbeddedDocument,
    Pdf,
    MediaMetadata,
    TerminalProtocol,
    BrowserNative,
    RemoteStream,
    Bridge,
    Extension,
    Memory,
    Visual,
}

/// One provenance record.  It describes evidence and does not authorize an
/// action or import a transport-specific identifier.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SurfaceEvidence {
    pub source: SurfaceEvidenceSource,
    pub quality: CoverageLevel,
    pub provenance: SurfaceProvenance,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

impl SurfaceEvidence {
    fn validate(
        &self,
        path: &str,
        surface_revision: SurfaceRevision,
    ) -> Result<(), SurfaceContractError> {
        self.provenance
            .validate(&format!("{path}.provenance"), surface_revision, self.source)?;
        if let Some(detail) = &self.detail {
            validate_bounded_text(
                &format!("{path}.detail"),
                detail,
                MAX_SURFACE_EVIDENCE_DETAIL_BYTES,
            )?;
        }
        Ok(())
    }
}

/// Revision supplied by the adapter that produced the evidence.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SurfaceRevision(pub u64);

impl SurfaceRevision {
    pub fn new(value: u64) -> Result<Self, SurfaceContractError> {
        if value == 0 {
            return Err(SurfaceContractError::new(
                "revision",
                "revision must be positive",
            ));
        }
        Ok(Self(value))
    }
}

/// Explainable bounded diagnostic attached to a surface.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SurfaceDiagnostic {
    pub severity: DiagnosticSeverity,
    pub code: String,
    pub message: String,
}

impl SurfaceDiagnostic {
    pub fn validate(&self, path: &str) -> Result<(), SurfaceContractError> {
        validate_identifier_with_max(
            &format!("{path}.code"),
            &self.code,
            MAX_SURFACE_DIAGNOSTIC_CODE_BYTES,
        )?;
        validate_bounded_text(
            &format!("{path}.message"),
            &self.message,
            MAX_SURFACE_DIAGNOSTIC_MESSAGE_BYTES,
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DiagnosticSeverity {
    Info,
    Warning,
    Error,
}

/// Identity, nesting, evidence, and declared understanding for one surface.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Surface {
    pub schema_version: u32,
    pub surface_id: SurfaceId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_surface_id: Option<SurfaceId>,
    pub nesting_depth: u16,
    pub kind: SurfaceKind,
    pub capabilities: Vec<SurfaceCapability>,
    pub understanding: UnderstandingLevel,
    pub coverage: SurfaceCoverage,
    pub evidence: Vec<SurfaceEvidence>,
    pub revision: SurfaceRevision,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub diagnostics: Vec<SurfaceDiagnostic>,
}

impl Surface {
    pub fn validate(&self) -> Result<(), SurfaceContractError> {
        self.validate_with_grants(&BridgeGrantRegistry::default())
    }

    pub fn validate_with_grants(
        &self,
        grants: &BridgeGrantRegistry,
    ) -> Result<(), SurfaceContractError> {
        grants.validate()?;
        if self.schema_version != SURFACE_SCHEMA_VERSION {
            return Err(SurfaceContractError::new(
                "schemaVersion",
                "unsupported surface contract schema version",
            ));
        }
        validate_surface_id("surfaceId", &self.surface_id)?;
        if let Some(parent) = &self.parent_surface_id {
            validate_surface_id("parentSurfaceId", parent)?;
            if parent == &self.surface_id {
                return Err(SurfaceContractError::new(
                    "parentSurfaceId",
                    "a surface cannot be its own parent",
                ));
            }
        }
        validate_nesting(self.parent_surface_id.is_some(), self.nesting_depth)?;
        self.validate_kind()?;
        self.validate_capabilities()?;
        self.coverage.validate(self.understanding)?;
        let revision = SurfaceRevision::new(self.revision.0)?;
        if self.evidence.is_empty() {
            return Err(SurfaceContractError::new(
                "evidence",
                "at least one evidence source is required",
            ));
        }
        if self.evidence.len() > MAX_SURFACE_EVIDENCE {
            return Err(SurfaceContractError::new(
                "evidence",
                "evidence exceeds the surface contract bound",
            ));
        }
        for (index, evidence) in self.evidence.iter().enumerate() {
            evidence.validate(&format!("evidence[{index}]"), revision)?;
        }
        self.validate_evidence_requirements(grants)?;
        if self.diagnostics.len() > MAX_SURFACE_DIAGNOSTICS {
            return Err(SurfaceContractError::new(
                "diagnostics",
                "diagnostics exceed the surface contract bound",
            ));
        }
        for (index, diagnostic) in self.diagnostics.iter().enumerate() {
            diagnostic.validate(&format!("diagnostics[{index}]"))?;
        }
        Ok(())
    }

    fn validate_evidence_requirements(
        &self,
        grants: &BridgeGrantRegistry,
    ) -> Result<(), SurfaceContractError> {
        let executable_requested = self.capabilities.iter().any(|capability| {
            matches!(
                capability,
                SurfaceCapability::SemanticAction
                    | SurfaceCapability::CoordinateAction
                    | SurfaceCapability::Input
                    | SurfaceCapability::KeyboardInput
                    | SurfaceCapability::PointerInput
                    | SurfaceCapability::Bridge
            )
        });
        if executable_requested
            && self
                .evidence
                .iter()
                .all(|evidence| evidence.source == SurfaceEvidenceSource::Memory)
        {
            return Err(SurfaceContractError::new(
                "evidence",
                "memory evidence is advisory and cannot authorize executable actions",
            ));
        }
        let has_explicit_action_evidence = self.evidence.iter().any(|evidence| {
            matches!(
                evidence.provenance.source_class,
                ProvenanceSourceClass::LiveWebIr
                    | ProvenanceSourceClass::Backend
                    | ProvenanceSourceClass::Bridge
            ) && !matches!(
                evidence.source,
                SurfaceEvidenceSource::Visual | SurfaceEvidenceSource::Memory
            )
        });
        if executable_requested && !has_explicit_action_evidence {
            return Err(SurfaceContractError::new(
                "evidence",
                "executable actions require explicit live Web IR or trusted bridge evidence",
            ));
        }
        let pointer_input_requested = self.capabilities.iter().any(|capability| {
            matches!(
                capability,
                SurfaceCapability::CoordinateAction | SurfaceCapability::PointerInput
            )
        });
        let keyboard_input_requested = self.capabilities.iter().any(|capability| {
            matches!(
                capability,
                SurfaceCapability::Input | SurfaceCapability::KeyboardInput
            )
        });
        if pointer_input_requested
            && !self.evidence.iter().any(|evidence| {
                evidence.quality >= CoverageLevel::Strong
                    && action_evidence_source(&self.kind, evidence.source, true)
                    && trusted_action_provenance(evidence)
            })
        {
            return Err(SurfaceContractError::new(
                "evidence",
                "pointer input requires strong compatible geometry or trusted bridge evidence",
            ));
        }
        if keyboard_input_requested
            && !self.evidence.iter().any(|evidence| {
                evidence.quality >= CoverageLevel::Strong
                    && keyboard_evidence_source(&self.kind, evidence.source)
                    && trusted_action_provenance(evidence)
            })
        {
            return Err(SurfaceContractError::new(
                "evidence",
                "keyboard input requires strong compatible DOM, accessibility, native, or trusted bridge evidence",
            ));
        }
        let structural_evidence = self
            .evidence
            .iter()
            .filter(|evidence| source_supports_structure(&self.kind, evidence.source));
        let has_structural_evidence = structural_evidence
            .clone()
            .any(|evidence| evidence.quality >= self.coverage.structural);
        if self.understanding >= UnderstandingLevel::Structural && !has_structural_evidence {
            return Err(SurfaceContractError::new(
                "evidence",
                "structural coverage requires compatible DOM, accessibility, or validated adapter evidence",
            ));
        }
        let mut semantic_evidence = self
            .evidence
            .iter()
            .filter(|evidence| source_supports_semantics(&self.kind, evidence.source));
        let has_strong_semantic_evidence = semantic_evidence
            .clone()
            .any(|evidence| evidence.quality >= CoverageLevel::Strong);
        let has_any_semantic_evidence = semantic_evidence.next().is_some();
        let needs_semantics = self.understanding >= UnderstandingLevel::Semantic
            || self.capabilities.contains(&SurfaceCapability::SemanticAction);
        if needs_semantics
            && (self.coverage.semantic < CoverageLevel::Strong
                || !has_strong_semantic_evidence)
        {
            return Err(SurfaceContractError::new(
                "evidence",
                "semantic understanding and action require strong compatible evidence",
            ));
        }
        if needs_semantics && !has_any_semantic_evidence {
            return Err(SurfaceContractError::new(
                "evidence",
                "semantic understanding cannot be established from detection, memory, or geometry alone",
            ));
        }
        let has_bridge_evidence = self.evidence.iter().any(|evidence| {
            matches!(
                evidence.source,
                SurfaceEvidenceSource::Bridge | SurfaceEvidenceSource::Extension
            )
        });
        if self.understanding == UnderstandingLevel::TaskCompilable
            && (self.coverage.structural < CoverageLevel::Strong
                || self.coverage.semantic < CoverageLevel::Strong
                || !has_strong_semantic_evidence)
        {
            return Err(SurfaceContractError::new(
                "evidence",
                "task-compilable understanding requires strong structural and semantic evidence",
            ));
        }
        let bridge_capability_requested = self.capabilities.contains(&SurfaceCapability::Bridge);
        let actionability_requested = self.understanding == UnderstandingLevel::TaskCompilable
            || self.capabilities.iter().any(|capability| {
                matches!(
                    capability,
                    SurfaceCapability::SemanticAction
                        | SurfaceCapability::CoordinateAction
                        | SurfaceCapability::Input
                        | SurfaceCapability::KeyboardInput
                        | SurfaceCapability::PointerInput
                        | SurfaceCapability::Bridge
                )
            });
        if bridge_capability_requested && !has_bridge_evidence {
            return Err(SurfaceContractError::new(
                "evidence",
                "bridge invocation requires validated bridge evidence and an independent registry grant",
            ));
        }
        if has_bridge_evidence {
            for evidence in self.evidence.iter().filter(|evidence| {
                matches!(
                    evidence.source,
                    SurfaceEvidenceSource::Bridge | SurfaceEvidenceSource::Extension
                )
            }) {
                let Some(token) = evidence.provenance.grant_token.as_deref() else {
                    return Err(SurfaceContractError::new(
                        "evidence",
                        "bridge evidence requires an independent registry grant",
                    ));
                };
                let Some(grant) = grants.grants.get(token) else {
                    return Err(SurfaceContractError::new(
                        "evidence",
                        "bridge grant token is not present in the trusted registry",
                    ));
                };
                if evidence.provenance.bridge_origin.as_deref() != Some(grant.origin.as_str())
                    || !self
                        .capabilities
                        .iter()
                        .all(|capability| grant.capabilities.contains(capability))
                    || !evidence
                        .provenance
                        .bridge_capabilities
                        .iter()
                        .all(|capability| grant.capabilities.contains(capability))
                {
                    return Err(SurfaceContractError::new(
                        "evidence",
                        "bridge evidence capabilities and origin do not match its registry grant",
                    ));
                }
            }
        }
        if actionability_requested && has_bridge_evidence {
            let required: &[SurfaceCapability] = self.capabilities.as_slice();
            if !self.evidence.iter().any(|evidence| {
                matches!(
                    evidence.source,
                    SurfaceEvidenceSource::Bridge | SurfaceEvidenceSource::Extension
                ) && evidence.provenance.bridge_trust == Some(BridgeTrustLevel::CapabilityGranted)
                    && evidence.provenance.grant_token.as_ref().is_some_and(|token| {
                        grants.grants.get(token).is_some_and(|grant| {
                            required
                                .iter()
                                .all(|capability| grant.capabilities.contains(capability))
                        })
                    })
            }) {
                return Err(SurfaceContractError::new(
                    "evidence",
                    "bridge-derived semantic action requires a validated registry capability grant",
                ));
            }
        }
        Ok(())
    }

    fn validate_kind(&self) -> Result<(), SurfaceContractError> {
        match &self.kind {
            SurfaceKind::ExtensionDefined { extension } => extension.validate(),
            SurfaceKind::Unknown => {
                if self.understanding != UnderstandingLevel::Opaque {
                    return Err(SurfaceContractError::new(
                        "understanding",
                        "unknown surfaces must remain opaque until an extension is validated",
                    ));
                }
                Ok(())
            }
            SurfaceKind::Opaque => {
                if self.understanding != UnderstandingLevel::Opaque {
                    return Err(SurfaceContractError::new(
                        "understanding",
                        "opaque surfaces cannot claim a higher understanding level",
                    ));
                }
                Ok(())
            }
            _ => Ok(()),
        }
    }

    fn validate_capabilities(&self) -> Result<(), SurfaceContractError> {
        if self.capabilities.len() > MAX_SURFACE_CAPABILITIES {
            return Err(SurfaceContractError::new(
                "capabilities",
                "capabilities exceed the surface contract bound",
            ));
        }
        let mut unique = BTreeSet::new();
        for capability in &self.capabilities {
            if !unique.insert(*capability) {
                return Err(SurfaceContractError::new(
                    "capabilities",
                    "capabilities must not be duplicated",
                ));
            }
            let minimum = match capability {
                SurfaceCapability::ReadStructure
                | SurfaceCapability::ReadText
                | SurfaceCapability::ReadRelations
                | SurfaceCapability::ReadState
                | SurfaceCapability::Extraction => UnderstandingLevel::Structural,
                SurfaceCapability::SemanticAction | SurfaceCapability::Verification => {
                    UnderstandingLevel::Semantic
                }
                SurfaceCapability::Input
                | SurfaceCapability::KeyboardInput
                | SurfaceCapability::PointerInput
                | SurfaceCapability::CoordinateAction => {
                    UnderstandingLevel::CoordinateOnly
                }
                SurfaceCapability::Capture
                | SurfaceCapability::Bridge
                | SurfaceCapability::RevisionObservation => UnderstandingLevel::Opaque,
            };
            if self.understanding < minimum {
                return Err(SurfaceContractError::new(
                    "capabilities",
                    "capability is incompatible with the declared understanding level",
                ));
            }
            if matches!(
                capability,
                SurfaceCapability::CoordinateAction
                    | SurfaceCapability::Input
                    | SurfaceCapability::KeyboardInput
                    | SurfaceCapability::PointerInput
            ) && self.coverage.interaction < InteractionCoverage::CoordinateOnly
            {
                return Err(SurfaceContractError::new(
                    "coverage.interaction",
                    "coordinate input requires coordinate interaction coverage",
                ));
            }
            if matches!(capability, SurfaceCapability::SemanticAction)
                && self.coverage.interaction < InteractionCoverage::Semantic
            {
                return Err(SurfaceContractError::new(
                    "coverage.interaction",
                    "semantic action requires semantic interaction coverage",
                ));
            }
        }
        if self.understanding == UnderstandingLevel::TaskCompilable {
            for required in [
                SurfaceCapability::ReadStructure,
                SurfaceCapability::ReadState,
                SurfaceCapability::SemanticAction,
                SurfaceCapability::RevisionObservation,
                SurfaceCapability::Verification,
            ] {
                if !unique.contains(&required) {
                    return Err(SurfaceContractError::new(
                        "capabilities",
                        "task-compilable understanding requires structure, state, semantic action, revision observation, and verification capabilities",
                    ));
                }
            }
        }
        if matches!(self.understanding, UnderstandingLevel::Opaque)
            && self.capabilities.iter().any(|capability| {
                matches!(
                    capability,
                    SurfaceCapability::CoordinateAction
                        | SurfaceCapability::SemanticAction
                        | SurfaceCapability::Input
                        | SurfaceCapability::KeyboardInput
                        | SurfaceCapability::PointerInput
                        | SurfaceCapability::Verification
                )
            })
        {
            return Err(SurfaceContractError::new(
                "capabilities",
                "opaque understanding cannot claim action or verification capabilities",
            ));
        }
        Ok(())
    }

    /// Parse and validate one surface from strict JSON.
    pub fn from_json(input: &str) -> Result<Self, SurfaceContractError> {
        Self::from_json_with_grants(input, &BridgeGrantRegistry::default())
    }

    pub fn from_json_with_grants(
        input: &str,
        grants: &BridgeGrantRegistry,
    ) -> Result<Self, SurfaceContractError> {
        if input.len() > MAX_SURFACE_PAYLOAD_BYTES {
            return Err(SurfaceContractError::new(
                "$",
                "surface payload exceeds the contract bound",
            ));
        }
        let surface: Self = serde_json::from_str(input).map_err(|error| {
            SurfaceContractError::new("$", format!("invalid surface: {error}"))
        })?;
        surface.validate_with_grants(grants)?;
        Ok(surface)
    }

    /// Serialize a validated surface using the stable serde representation.
    pub fn to_canonical_json(&self) -> Result<String, SurfaceContractError> {
        self.to_canonical_json_with_grants(&BridgeGrantRegistry::default())
    }

    pub fn to_canonical_json_with_grants(
        &self,
        grants: &BridgeGrantRegistry,
    ) -> Result<String, SurfaceContractError> {
        self.validate_with_grants(grants)?;
        let output = serde_json::to_string(self).map_err(|error| {
            SurfaceContractError::new("$", format!("failed to serialize surface: {error}"))
        })?;
        if output.len() > MAX_SURFACE_PAYLOAD_BYTES {
            return Err(SurfaceContractError::new(
                "$",
                "surface payload exceeds the contract bound",
            ));
        }
        Ok(output)
    }
}

/// A validated set of surfaces.  The set validates parent references and
/// depth, preventing cycles and orphaned nested surfaces.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SurfaceSet {
    pub schema_version: u32,
    pub surfaces: Vec<Surface>,
}

impl SurfaceSet {
    pub fn validate(&self) -> Result<(), SurfaceContractError> {
        self.validate_with_grants(&BridgeGrantRegistry::default())
    }

    pub fn validate_with_grants(
        &self,
        grants: &BridgeGrantRegistry,
    ) -> Result<(), SurfaceContractError> {
        grants.validate()?;
        if self.schema_version != SURFACE_SCHEMA_VERSION {
            return Err(SurfaceContractError::new(
                "schemaVersion",
                "unsupported surface contract schema version",
            ));
        }
        if self.surfaces.is_empty() {
            return Err(SurfaceContractError::new(
                "surfaces",
                "at least one surface is required",
            ));
        }
        if self.surfaces.len() > MAX_SURFACES {
            return Err(SurfaceContractError::new(
                "surfaces",
                "surfaces exceed the contract bound",
            ));
        }
        let mut ids = BTreeSet::new();
        for (index, surface) in self.surfaces.iter().enumerate() {
            surface
                .validate_with_grants(grants)
                .map_err(|error| error.at(&format!("surfaces[{index}]")))?;
            if !ids.insert(surface.surface_id.clone()) {
                return Err(SurfaceContractError::new(
                    format!("surfaces[{index}].surfaceId"),
                    "surface IDs must be unique",
                ));
            }
        }
        for (index, surface) in self.surfaces.iter().enumerate() {
            match (&surface.parent_surface_id, surface.nesting_depth) {
                (None, 0) => {}
                (None, _) => {
                    return Err(SurfaceContractError::new(
                        format!("surfaces[{index}].nestingDepth"),
                        "root surfaces must have nesting depth zero",
                    ));
                }
                (Some(parent), depth) => {
                    let Some(parent_surface) = self
                        .surfaces
                        .iter()
                        .find(|candidate| candidate.surface_id == *parent)
                    else {
                        return Err(SurfaceContractError::new(
                            format!("surfaces[{index}].parentSurfaceId"),
                            "parent surface does not exist in the surface set",
                        ));
                    };
                    if depth != parent_surface.nesting_depth.saturating_add(1) {
                        return Err(SurfaceContractError::new(
                            format!("surfaces[{index}].nestingDepth"),
                            "nested surface depth must be exactly one greater than its parent",
                        ));
                    }
                }
            }
        }
        Ok(())
    }

    pub fn from_json(input: &str) -> Result<Self, SurfaceContractError> {
        Self::from_json_with_grants(input, &BridgeGrantRegistry::default())
    }

    pub fn from_json_with_grants(
        input: &str,
        grants: &BridgeGrantRegistry,
    ) -> Result<Self, SurfaceContractError> {
        if input.len() > MAX_SURFACE_PAYLOAD_BYTES {
            return Err(SurfaceContractError::new(
                "$",
                "surface payload exceeds the contract bound",
            ));
        }
        let set: Self = serde_json::from_str(input).map_err(|error| {
            SurfaceContractError::new("$", format!("invalid surface set: {error}"))
        })?;
        set.validate_with_grants(grants)?;
        Ok(set)
    }

    pub fn to_canonical_json(&self) -> Result<String, SurfaceContractError> {
        self.to_canonical_json_with_grants(&BridgeGrantRegistry::default())
    }

    pub fn to_canonical_json_with_grants(
        &self,
        grants: &BridgeGrantRegistry,
    ) -> Result<String, SurfaceContractError> {
        self.validate_with_grants(grants)?;
        let output = serde_json::to_string(self).map_err(|error| {
            SurfaceContractError::new("$", format!("failed to serialize surface set: {error}"))
        })?;
        if output.len() > MAX_SURFACE_PAYLOAD_BYTES {
            return Err(SurfaceContractError::new(
                "$",
                "surface payload exceeds the contract bound",
            ));
        }
        Ok(output)
    }
}

/// Alias emphasizing that this set is the transport-neutral contract boundary.
pub type SurfaceContract = SurfaceSet;

/// A machine-readable bounded surface contract error.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SurfaceContractError {
    pub path: String,
    pub reason: String,
}

impl SurfaceContractError {
    fn new(path: impl Into<String>, reason: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            reason: reason.into(),
        }
    }

    fn at(self, prefix: &str) -> Self {
        Self::new(format!("{prefix}.{}", self.path), self.reason)
    }
}

impl Display for SurfaceContractError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{}: {}", self.path, self.reason)
    }
}

impl std::error::Error for SurfaceContractError {}

fn action_evidence_source(
    kind: &SurfaceKind,
    source: SurfaceEvidenceSource,
    coordinate_action: bool,
) -> bool {
    if coordinate_action {
        return matches!(
            source,
            SurfaceEvidenceSource::Layout
                | SurfaceEvidenceSource::CanvasDetection
                | SurfaceEvidenceSource::Bridge
                | SurfaceEvidenceSource::Extension
        ) || (matches!(kind, SurfaceKind::Svg) && source == SurfaceEvidenceSource::Svg)
            || (matches!(kind, SurfaceKind::RemoteApplication)
                && source == SurfaceEvidenceSource::RemoteStream)
            || (matches!(kind, SurfaceKind::Terminal)
                && source == SurfaceEvidenceSource::TerminalProtocol)
            || (matches!(kind, SurfaceKind::BrowserNative)
                && source == SurfaceEvidenceSource::BrowserNative);
    }
    match kind {
        SurfaceKind::Canvas2d | SurfaceKind::Webgl | SurfaceKind::Webgpu => matches!(
            source,
            SurfaceEvidenceSource::Layout
                | SurfaceEvidenceSource::CanvasDetection
                | SurfaceEvidenceSource::Bridge
                | SurfaceEvidenceSource::Extension
        ),
        SurfaceKind::Svg => matches!(
            source,
            SurfaceEvidenceSource::Svg
                | SurfaceEvidenceSource::Dom
                | SurfaceEvidenceSource::Accessibility
                | SurfaceEvidenceSource::Bridge
                | SurfaceEvidenceSource::Extension
        ),
        SurfaceKind::RemoteApplication => matches!(
            source,
            SurfaceEvidenceSource::RemoteStream
                | SurfaceEvidenceSource::Bridge
                | SurfaceEvidenceSource::Extension
        ),
        SurfaceKind::Terminal => matches!(
            source,
            SurfaceEvidenceSource::TerminalProtocol
                | SurfaceEvidenceSource::Bridge
                | SurfaceEvidenceSource::Extension
        ),
        SurfaceKind::BrowserNative => matches!(
            source,
            SurfaceEvidenceSource::BrowserNative
                | SurfaceEvidenceSource::Bridge
                | SurfaceEvidenceSource::Extension
        ),
        _ => matches!(
            source,
            SurfaceEvidenceSource::Dom
                | SurfaceEvidenceSource::Accessibility
                | SurfaceEvidenceSource::Bridge
                | SurfaceEvidenceSource::Extension
        ),
    }
}

fn trusted_action_provenance(evidence: &SurfaceEvidence) -> bool {
    matches!(
        evidence.provenance.source_class,
        ProvenanceSourceClass::LiveWebIr
            | ProvenanceSourceClass::Backend
            | ProvenanceSourceClass::Bridge
    )
}

fn keyboard_evidence_source(kind: &SurfaceKind, source: SurfaceEvidenceSource) -> bool {
    match kind {
        SurfaceKind::Document
        | SurfaceKind::Accessibility
        | SurfaceKind::ShadowDocument
        | SurfaceKind::FrameDocument
        | SurfaceKind::EmbeddedDocument
        | SurfaceKind::Media => matches!(
            source,
            SurfaceEvidenceSource::Dom
                | SurfaceEvidenceSource::Accessibility
                | SurfaceEvidenceSource::Bridge
                | SurfaceEvidenceSource::Extension
        ),
        SurfaceKind::Svg => matches!(
            source,
            SurfaceEvidenceSource::Svg
                | SurfaceEvidenceSource::Dom
                | SurfaceEvidenceSource::Accessibility
                | SurfaceEvidenceSource::Bridge
                | SurfaceEvidenceSource::Extension
        ),
        SurfaceKind::Pdf => matches!(
            source,
            SurfaceEvidenceSource::Pdf
                | SurfaceEvidenceSource::Bridge
                | SurfaceEvidenceSource::Extension
        ),
        SurfaceKind::Canvas2d | SurfaceKind::Webgl | SurfaceKind::Webgpu => {
            matches!(source, SurfaceEvidenceSource::Bridge | SurfaceEvidenceSource::Extension)
        }
        SurfaceKind::RemoteApplication => matches!(
            source,
            SurfaceEvidenceSource::RemoteStream
                | SurfaceEvidenceSource::Bridge
                | SurfaceEvidenceSource::Extension
        ),
        SurfaceKind::Terminal => matches!(
            source,
            SurfaceEvidenceSource::TerminalProtocol
                | SurfaceEvidenceSource::Bridge
                | SurfaceEvidenceSource::Extension
        ),
        SurfaceKind::BrowserNative => matches!(
            source,
            SurfaceEvidenceSource::BrowserNative
                | SurfaceEvidenceSource::Bridge
                | SurfaceEvidenceSource::Extension
        ),
        SurfaceKind::ExtensionDefined { .. } => {
            matches!(source, SurfaceEvidenceSource::Bridge | SurfaceEvidenceSource::Extension)
        }
        SurfaceKind::Unknown | SurfaceKind::Opaque => false,
    }
}

fn source_supports_structure(kind: &SurfaceKind, source: SurfaceEvidenceSource) -> bool {
    if matches!(
        source,
        SurfaceEvidenceSource::Bridge | SurfaceEvidenceSource::Extension
    ) {
        return !matches!(kind, SurfaceKind::Unknown | SurfaceKind::Opaque);
    }
    match kind {
        SurfaceKind::Document => matches!(source, SurfaceEvidenceSource::Dom | SurfaceEvidenceSource::Accessibility),
        SurfaceKind::Accessibility => source == SurfaceEvidenceSource::Accessibility,
        SurfaceKind::ShadowDocument => matches!(
            source,
            SurfaceEvidenceSource::ShadowDom
                | SurfaceEvidenceSource::Dom
                | SurfaceEvidenceSource::Accessibility
        ),
        SurfaceKind::FrameDocument => matches!(
            source,
            SurfaceEvidenceSource::Frame
                | SurfaceEvidenceSource::Dom
                | SurfaceEvidenceSource::Accessibility
        ),
        SurfaceKind::Svg => matches!(
            source,
            SurfaceEvidenceSource::Svg
                | SurfaceEvidenceSource::Dom
                | SurfaceEvidenceSource::Accessibility
        ),
        SurfaceKind::Canvas2d | SurfaceKind::Webgl | SurfaceKind::Webgpu => {
            matches!(source, SurfaceEvidenceSource::Bridge | SurfaceEvidenceSource::Extension)
        }
        SurfaceKind::EmbeddedDocument => matches!(
            source,
            SurfaceEvidenceSource::EmbeddedDocument
                | SurfaceEvidenceSource::Dom
                | SurfaceEvidenceSource::Accessibility
        ),
        SurfaceKind::Pdf => matches!(
            source,
            SurfaceEvidenceSource::Pdf | SurfaceEvidenceSource::EmbeddedDocument
        ),
        SurfaceKind::Media => matches!(
            source,
            SurfaceEvidenceSource::MediaMetadata
                | SurfaceEvidenceSource::Dom
                | SurfaceEvidenceSource::Accessibility
        ),
        SurfaceKind::Terminal => matches!(
            source,
            SurfaceEvidenceSource::TerminalProtocol
                | SurfaceEvidenceSource::Bridge
                | SurfaceEvidenceSource::Extension
        ),
        SurfaceKind::RemoteApplication => {
            matches!(source, SurfaceEvidenceSource::Bridge | SurfaceEvidenceSource::Extension)
        }
        SurfaceKind::BrowserNative => matches!(
            source,
            SurfaceEvidenceSource::BrowserNative
                | SurfaceEvidenceSource::Bridge
                | SurfaceEvidenceSource::Extension
        ),
        SurfaceKind::ExtensionDefined { .. } => {
            matches!(source, SurfaceEvidenceSource::Bridge | SurfaceEvidenceSource::Extension)
        }
        SurfaceKind::Unknown | SurfaceKind::Opaque => false,
    }
}

fn source_supports_semantics(kind: &SurfaceKind, source: SurfaceEvidenceSource) -> bool {
    if matches!(
        source,
        SurfaceEvidenceSource::Bridge | SurfaceEvidenceSource::Extension
    ) {
        return !matches!(kind, SurfaceKind::Unknown | SurfaceKind::Opaque);
    }
    match kind {
        SurfaceKind::Document => matches!(source, SurfaceEvidenceSource::Dom | SurfaceEvidenceSource::Accessibility),
        SurfaceKind::Accessibility => source == SurfaceEvidenceSource::Accessibility,
        SurfaceKind::ShadowDocument => matches!(source, SurfaceEvidenceSource::Dom | SurfaceEvidenceSource::Accessibility),
        SurfaceKind::FrameDocument => matches!(source, SurfaceEvidenceSource::Dom | SurfaceEvidenceSource::Accessibility),
        SurfaceKind::Svg => matches!(
            source,
            SurfaceEvidenceSource::Svg
                | SurfaceEvidenceSource::Dom
                | SurfaceEvidenceSource::Accessibility
        ),
        SurfaceKind::Canvas2d | SurfaceKind::Webgl | SurfaceKind::Webgpu => {
            matches!(source, SurfaceEvidenceSource::Bridge | SurfaceEvidenceSource::Extension)
        }
        SurfaceKind::EmbeddedDocument => matches!(
            source,
            SurfaceEvidenceSource::EmbeddedDocument
                | SurfaceEvidenceSource::Dom
                | SurfaceEvidenceSource::Accessibility
        ),
        SurfaceKind::Pdf => source == SurfaceEvidenceSource::Pdf,
        SurfaceKind::Media => matches!(
            source,
            SurfaceEvidenceSource::MediaMetadata
                | SurfaceEvidenceSource::Dom
                | SurfaceEvidenceSource::Accessibility
        ),
        SurfaceKind::Terminal => source == SurfaceEvidenceSource::TerminalProtocol,
        SurfaceKind::BrowserNative => source == SurfaceEvidenceSource::BrowserNative,
        SurfaceKind::RemoteApplication => {
            matches!(source, SurfaceEvidenceSource::Bridge | SurfaceEvidenceSource::Extension)
        }
        SurfaceKind::ExtensionDefined { .. } => {
            matches!(source, SurfaceEvidenceSource::Bridge | SurfaceEvidenceSource::Extension)
        }
        SurfaceKind::Unknown | SurfaceKind::Opaque => false,
    }
}

fn validate_surface_id(path: &str, value: &SurfaceId) -> Result<(), SurfaceContractError> {
    validate_identifier_with_max(path, value.as_str(), MAX_SURFACE_IDENTIFIER_BYTES)
}

fn validate_namespaced_component(path: &str, value: &str) -> Result<(), SurfaceContractError> {
    validate_identifier_with_max(path, value, MAX_SURFACE_IDENTIFIER_BYTES)
}

fn validate_identifier_with_max(
    path: &str,
    value: &str,
    maximum: usize,
) -> Result<(), SurfaceContractError> {
    if value.is_empty() || value.len() > maximum {
        return Err(SurfaceContractError::new(
            path,
            format!("identifier must contain 1-{maximum} bytes"),
        ));
    }
    let valid = value.as_bytes().iter().enumerate().all(|(index, byte)| {
        byte.is_ascii_alphanumeric()
            || (*byte == b'.' && index > 0 && index + 1 < value.len())
            || (*byte == b'_' && index > 0)
            || (*byte == b'-' && index > 0)
            || (*byte == b':' && index > 0 && index + 1 < value.len())
    });
    if !valid
        || !value.as_bytes()[0].is_ascii_alphanumeric()
        || !value.as_bytes()[value.len() - 1].is_ascii_alphanumeric()
    {
        return Err(SurfaceContractError::new(
            path,
            "identifier must use alphanumeric namespaced characters and cannot begin or end with punctuation",
        ));
    }
    Ok(())
}

fn validate_extension_component(
    path: &str,
    value: &str,
) -> Result<(), SurfaceContractError> {
    if value.contains(':') {
        return Err(SurfaceContractError::new(
            path,
            "extension namespace and name must not contain ':'",
        ));
    }
    validate_namespaced_component(path, value)
}

fn validate_bounded_text(
    path: &str,
    value: &str,
    maximum: usize,
) -> Result<(), SurfaceContractError> {
    if value.is_empty() || value.len() > maximum || value.chars().any(char::is_control) {
        return Err(SurfaceContractError::new(
            path,
            format!("text must contain 1-{maximum} non-control bytes"),
        ));
    }
    Ok(())
}
fn validate_canonical_timestamp(
    path: &str,
    value: &str,
) -> Result<(), SurfaceContractError> {
    if value.len() != 20
        || value.as_bytes()[4] != b'-'
        || value.as_bytes()[7] != b'-'
        || value.as_bytes()[10] != b'T'
        || value.as_bytes()[13] != b':'
        || value.as_bytes()[16] != b':'
        || value.as_bytes()[19] != b'Z'
        || value
            .as_bytes()
            .iter()
            .enumerate()
            .any(|(index, byte)| {
                !matches!(index, 4 | 7 | 10 | 13 | 16 | 19) && !byte.is_ascii_digit()
            })
    {
        return Err(SurfaceContractError::new(
            path,
            "timestamp must use canonical UTC RFC3339 form YYYY-MM-DDTHH:MM:SSZ",
        ));
    }
    let number = |start: usize, end: usize| {
        value.as_bytes()[start..end]
            .iter()
            .fold(0u32, |total, byte| total * 10 + u32::from(byte - b'0'))
    };
    let year = number(0, 4);
    let month = number(5, 7);
    let day = number(8, 10);
    let hour = number(11, 13);
    let minute = number(14, 16);
    let second = number(17, 19);
    let leap_year = year % 4 == 0 && (year % 100 != 0 || year % 400 == 0);
    let days_in_month = match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if leap_year => 29,
        2 => 28,
        _ => 0,
    };
    if !(1970..=9999).contains(&year)
        || day == 0
        || day > days_in_month
        || hour > 23
        || minute > 59
        || second > 59
    {
        return Err(SurfaceContractError::new(
            path,
            "timestamp contains an invalid canonical UTC date or time",
        ));
    }
    Ok(())
}

fn validate_origin(path: &str, value: &str) -> Result<(), SurfaceContractError> {
    validate_bounded_text(path, value, MAX_SURFACE_PROVENANCE_ID_BYTES)?;
    let Some(authority) = value
        .strip_prefix("https://")
        .or_else(|| value.strip_prefix("http://"))
    else {
        return Err(SurfaceContractError::new(
            path,
            "bridge origin must use http or https",
        ));
    };
    if authority.is_empty()
        || authority.contains('@')
        || authority.chars().any(|character| {
            character.is_whitespace() || matches!(character, '/' | '?' | '#')
        })
    {
        return Err(SurfaceContractError::new(
            path,
            "bridge origin must contain only an origin authority",
        ));
    }
    let (host, port) = if let Some(rest) = authority.strip_prefix('[') {
        let Some(end) = rest.find(']') else {
            return Err(SurfaceContractError::new(path, "invalid bracketed origin host"));
        };
        let host = &rest[..end];
        if host.parse::<Ipv6Addr>().is_err() {
            return Err(SurfaceContractError::new(
                path,
                "bracketed origin host must be a valid IPv6 address",
            ));
        }
        let suffix = &rest[end + 1..];
        let port = suffix.strip_prefix(':');
        if !suffix.is_empty() && port.is_none() {
            return Err(SurfaceContractError::new(path, "invalid origin authority suffix"));
        }
        (host, port)
    } else {
        let mut parts = authority.split(':');
        let host = parts.next().unwrap_or_default();
        let port = parts.next();
        if parts.next().is_some() {
            return Err(SurfaceContractError::new(path, "origin host must use a valid port"));
        }
        (host, port)
    };
    let valid_host = if host.contains(':') {
        !host.is_empty()
            && host.chars().all(|character| {
                character.is_ascii_hexdigit() || matches!(character, ':' | '.')
            })
    } else {
        !host.is_empty()
            && host.split('.').all(|label| {
                !label.is_empty()
                    && label.chars().all(|character| {
                        character.is_ascii_alphanumeric() || character == '-'
                    })
                    && label.as_bytes()[0].is_ascii_alphanumeric()
                    && label.as_bytes()[label.len() - 1].is_ascii_alphanumeric()
            })
    };
    if !valid_host {
        return Err(SurfaceContractError::new(
            path,
            "origin host contains malformed labels",
        ));
    }
    if let Some(port) = port {
        if port.is_empty()
            || !port.chars().all(|character| character.is_ascii_digit())
            || port.parse::<u16>().is_err()
        {
            return Err(SurfaceContractError::new(path, "origin port must be 1-65535"));
        }
    }
    Ok(())
}


fn validate_nesting(has_parent: bool, depth: u16) -> Result<(), SurfaceContractError> {
    if depth > MAX_SURFACE_NESTING_DEPTH {
        return Err(SurfaceContractError::new(
            "nestingDepth",
            "nesting depth exceeds the surface contract bound",
        ));
    }
    if !has_parent && depth != 0 {
        return Err(SurfaceContractError::new(
            "nestingDepth",
            "a root surface must have depth zero",
        ));
    }
    if has_parent && depth == 0 {
        return Err(SurfaceContractError::new(
            "nestingDepth",
            "a nested surface must have a positive depth",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn surface(kind: SurfaceKind, level: UnderstandingLevel, capabilities: Vec<SurfaceCapability>) -> Surface {
        let (source, provenance) = if matches!(&kind, SurfaceKind::ExtensionDefined { .. }) {
            (
                SurfaceEvidenceSource::Bridge,
                SurfaceProvenance {
                    schema_version: SURFACE_SCHEMA_VERSION,
                    source_class: ProvenanceSourceClass::Bridge,
                    source_id: "bridge.chart".into(),
                    backend: None,
                    backend_version: None,
                    adapter_version: None,
                    bridge_version: Some("1".into()),
                    bridge_origin: Some("https://example.test".into()),
                    bridge_trust: Some(BridgeTrustLevel::CapabilityGranted),
                    grant_token: Some("grant-chart".into()),
                    bridge_capabilities: vec![SurfaceCapability::ReadStructure],
                    source_revision: SurfaceRevision(1),
                    observed_at: "2026-08-06T00:00:00Z".into(),
                    validated_at: "2026-08-06T00:00:01Z".into(),
                },
            )
        } else {
            (
                SurfaceEvidenceSource::Accessibility,
                SurfaceProvenance {
                    schema_version: SURFACE_SCHEMA_VERSION,
                    source_class: ProvenanceSourceClass::LiveWebIr,
                    source_id: "browser.session".into(),
                    bridge_version: None,
                    bridge_origin: None,
                    bridge_trust: None,
                    grant_token: None,
                    bridge_capabilities: vec![],
                    backend: Some("cdp".into()),
                    backend_version: Some("1".into()),
                    adapter_version: None,
                    source_revision: SurfaceRevision(1),
                    observed_at: "2026-08-06T00:00:00Z".into(),
                    validated_at: "2026-08-06T00:00:01Z".into(),
                },
            )
        };
        Surface {
            schema_version: SURFACE_SCHEMA_VERSION,
            surface_id: SurfaceId::new("surface.document.main").unwrap(),
            parent_surface_id: None,
            nesting_depth: 0,
            kind,
            capabilities,
            understanding: level,
            coverage: SurfaceCoverage {
                structural: CoverageLevel::Strong,
                semantic: if level >= UnderstandingLevel::Semantic { CoverageLevel::Strong } else { CoverageLevel::Partial },
                interaction: match level {
                    UnderstandingLevel::Opaque => InteractionCoverage::Unavailable,
                    UnderstandingLevel::CoordinateOnly => InteractionCoverage::CoordinateOnly,
                    UnderstandingLevel::Structural => InteractionCoverage::Unavailable,
                    UnderstandingLevel::Semantic => InteractionCoverage::Semantic,
                    UnderstandingLevel::TaskCompilable => InteractionCoverage::TaskCompilable,
                },
            },
            evidence: vec![SurfaceEvidence { source, quality: CoverageLevel::Strong, provenance, detail: None }],
            revision: SurfaceRevision(1),
            diagnostics: vec![],
        }
    }

    #[test]
    fn round_trip_and_extension_namespace() {
        let extension = ExtensionIdentifier::new("vendor.example", "chart.series").unwrap();
        let value = surface(
            SurfaceKind::ExtensionDefined { extension },
            UnderstandingLevel::Semantic,
            vec![SurfaceCapability::ReadStructure],
        );
        let mut grants = BridgeGrantRegistry::default();
        grants
            .insert(BridgeCapabilityGrant {
                token: "grant-chart".into(),
                origin: "https://example.test".into(),
                capabilities: vec![SurfaceCapability::ReadStructure],
            })
            .unwrap();
        let encoded = value.to_canonical_json_with_grants(&grants).unwrap();
        assert_eq!(Surface::from_json_with_grants(&encoded, &grants).unwrap(), value);
        assert!(encoded.contains("extensionDefined"));
    }

    #[test]
    fn unknown_and_invalid_variants_fail_closed() {
        let unknown_kind = json!({"schemaVersion":1,"surfaceId":"surface.main","nestingDepth":0,"kind":"futureThing","capabilities":[],"understanding":"opaque","coverage":{"structural":"opaque","semantic":"opaque","interaction":"unavailable"},"evidence":[{"source":"layout","quality":"partial"}],"revision":1});
        assert!(Surface::from_json(&unknown_kind.to_string()).is_err());
        let invalid_id = json!({"schemaVersion":1,"surfaceId":"../cdp-node","nestingDepth":0,"kind":"opaque","capabilities":[],"understanding":"opaque","coverage":{"structural":"opaque","semantic":"opaque","interaction":"unavailable"},"evidence":[{"source":"layout","quality":"opaque"}],"revision":1});
        assert!(Surface::from_json(&invalid_id.to_string()).is_err());
    }

    #[test]
    fn bounds_duplicates_and_understanding_invariants() {
        let mut value = surface(SurfaceKind::Canvas2d, UnderstandingLevel::CoordinateOnly, vec![SurfaceCapability::CoordinateAction, SurfaceCapability::CoordinateAction]);
        assert!(value.validate().is_err());
        value.capabilities = vec![SurfaceCapability::SemanticAction];
        assert!(value.validate().is_err());
        value = surface(SurfaceKind::Canvas2d, UnderstandingLevel::Opaque, vec![]);
        value.nesting_depth = MAX_SURFACE_NESTING_DEPTH + 1;
        assert!(value.validate().is_err());
        value.nesting_depth = 0;
        value.diagnostics = (0..=MAX_SURFACE_DIAGNOSTICS).map(|index| SurfaceDiagnostic { severity: DiagnosticSeverity::Warning, code: format!("d{index}"), message: "bounded".into() }).collect();
        assert!(value.validate().is_err());
    }

    #[test]
    fn nesting_requires_existing_parent_and_adjacent_depth() {
        let root = surface(SurfaceKind::Document, UnderstandingLevel::Structural, vec![SurfaceCapability::ReadStructure]);
        let mut child = surface(SurfaceKind::Svg, UnderstandingLevel::Structural, vec![SurfaceCapability::ReadStructure]);
        child.surface_id = SurfaceId::new("surface.svg.main").unwrap();
        child.parent_surface_id = Some(root.surface_id.clone());
        child.nesting_depth = 1;
        assert!(SurfaceSet { schema_version: 1, surfaces: vec![root.clone(), child.clone()] }.validate().is_ok());
        child.nesting_depth = 2;
        assert!(SurfaceSet { schema_version: 1, surfaces: vec![root, child] }.validate().is_err());
    }

    #[test]
    fn task_compilable_requires_guard_capabilities() {
        let value = surface(SurfaceKind::Document, UnderstandingLevel::TaskCompilable, vec![SurfaceCapability::ReadStructure]);
        assert!(value.validate().is_err());
    }
    #[test]
    fn task_interaction_requires_semantic_understanding() {
        let mut value = surface(
            SurfaceKind::Document,
            UnderstandingLevel::Structural,
            vec![SurfaceCapability::ReadStructure],
        );
        value.coverage.interaction = InteractionCoverage::TaskCompilable;
        assert!(value.validate().is_err());
    }
    #[test]
    fn semantic_evidence_rejects_detection_only() {
        let mut value = surface(
            SurfaceKind::Canvas2d,
            UnderstandingLevel::Semantic,
            vec![SurfaceCapability::ReadStructure],
        );
        value.evidence[0].source = SurfaceEvidenceSource::CanvasDetection;
        assert!(value.validate().is_err());
    }

    #[test]
    fn task_compilable_requires_strong_revision_bound_evidence() {
        let mut value = surface(
            SurfaceKind::Document,
            UnderstandingLevel::TaskCompilable,
            vec![
                SurfaceCapability::ReadStructure,
                SurfaceCapability::ReadState,
                SurfaceCapability::SemanticAction,
                SurfaceCapability::RevisionObservation,
                SurfaceCapability::Verification,
            ],
        );
        value.evidence[0].quality = CoverageLevel::Partial;
        assert!(value.validate().is_err());
        value.evidence[0].quality = CoverageLevel::Strong;
        value.evidence[0].provenance.source_revision = SurfaceRevision(2);
        assert!(value.validate().is_err());
    }

    #[test]
    fn oversized_json_is_rejected_before_deserialization() {
        let input = format!(r#"{{"padding":"{}"}}"#, "x".repeat(MAX_SURFACE_PAYLOAD_BYTES));
        let error = Surface::from_json(&input).unwrap_err();
        assert_eq!(error.path, "$");
    }

    #[test]
    fn extension_identity_is_versioned_and_colon_free() {
        let extension = ExtensionIdentifier::new_versioned("vendor.example", "chart", "2.1").unwrap();
        assert_eq!(extension.qualified_name(), "vendor.example:chart@2.1");
        assert!(ExtensionIdentifier::new("vendor:example", "chart").is_err());
    }
    #[test]
    fn structural_coverage_rejects_layout_only_evidence() {
        let mut value = surface(
            SurfaceKind::Document,
            UnderstandingLevel::Structural,
            vec![SurfaceCapability::ReadStructure],
        );
        value.evidence[0].source = SurfaceEvidenceSource::Layout;
        assert!(value.validate().is_err());
    }

    #[test]
    fn memory_evidence_is_advisory_only() {
        let mut value = surface(
            SurfaceKind::Canvas2d,
            UnderstandingLevel::CoordinateOnly,
            vec![SurfaceCapability::CoordinateAction, SurfaceCapability::Input],
        );
        value.evidence[0].source = SurfaceEvidenceSource::Memory;
        value.evidence[0].provenance.source_class = ProvenanceSourceClass::Memory;
        assert!(value.validate().is_err());
    }

    #[test]
    fn bridge_action_requires_origin_and_capability_grant() {
        let extension = ExtensionIdentifier::new("vendor.example", "chart").unwrap();
        let mut value = surface(
            SurfaceKind::ExtensionDefined { extension },
            UnderstandingLevel::Semantic,
            vec![SurfaceCapability::ReadStructure, SurfaceCapability::SemanticAction],
        );
        assert!(value.validate().is_err());
        value.evidence[0]
            .provenance
            .bridge_capabilities
            .push(SurfaceCapability::SemanticAction);
        assert!(value.validate().is_err());
        let mut grants = BridgeGrantRegistry::default();
        grants
            .insert(BridgeCapabilityGrant {
                token: "grant-chart".into(),
                origin: "https://example.test".into(),
                capabilities: vec![
                    SurfaceCapability::ReadStructure,
                    SurfaceCapability::SemanticAction,
                ],
            })
            .unwrap();
        assert!(value.validate_with_grants(&grants).is_ok());
        value.evidence[0].provenance.bridge_origin = Some("not-an-origin".into());
        assert!(value.validate_with_grants(&grants).is_err());
        value.evidence[0].provenance.bridge_origin = Some("https://example.test:bad".into());
        assert!(value.validate_with_grants(&grants).is_err());
        value.evidence[0].provenance.bridge_origin = Some("https://user@example.test".into());
        let mut ipv6_grants = BridgeGrantRegistry::default();
        ipv6_grants
            .insert(BridgeCapabilityGrant {
                token: "grant-chart".into(),
                origin: "https://[2001:db8::1]:443".into(),
                capabilities: vec![
                    SurfaceCapability::ReadStructure,
                    SurfaceCapability::SemanticAction,
                ],
            })
            .unwrap();
        value.evidence[0].provenance.bridge_origin = Some("https://[2001:db8::1]:443".into());
        assert!(value.validate_with_grants(&ipv6_grants).is_ok());
        value.evidence[0].provenance.bridge_origin = Some("https://[2001:::1]".into());
        assert!(value.validate_with_grants(&ipv6_grants).is_err());
        assert!(value.validate_with_grants(&grants).is_err());
    }

    #[test]
    fn timestamps_must_be_canonical_utc() {
        let mut value = surface(
            SurfaceKind::Document,
            UnderstandingLevel::Structural,
            vec![SurfaceCapability::ReadStructure],
        );
        value.evidence[0].provenance.observed_at = "yesterday".into();
        assert!(value.validate().is_err());
        value.evidence[0].provenance.observed_at = "2026-02-31T00:00:00Z".into();
        assert!(value.validate().is_err());
    }
    #[test]
    fn coordinate_actions_require_strong_geometry() {
        let mut value = surface(
            SurfaceKind::Canvas2d,
            UnderstandingLevel::CoordinateOnly,
            vec![SurfaceCapability::CoordinateAction],
        );
        value.evidence[0].source = SurfaceEvidenceSource::CanvasDetection;
        value.evidence[0].quality = CoverageLevel::Opaque;
        assert!(value.validate().is_err());
        value.evidence[0].quality = CoverageLevel::Partial;
        assert!(value.validate().is_err());
        value.evidence[0].quality = CoverageLevel::Strong;
        assert!(value.validate().is_ok());
    }

    #[test]
    fn input_only_requires_strong_geometry() {
        let mut value = surface(
            SurfaceKind::Document,
            UnderstandingLevel::CoordinateOnly,
            vec![SurfaceCapability::Input],
        );
        value.evidence[0].quality = CoverageLevel::Opaque;
        assert!(value.validate().is_err());
        value.evidence[0].quality = CoverageLevel::Strong;
        assert!(value.validate().is_ok());
    }
    #[test]
    fn remote_stream_can_supply_strong_coordinate_input() {
        let mut value = surface(
            SurfaceKind::RemoteApplication,
            UnderstandingLevel::CoordinateOnly,
            vec![SurfaceCapability::Input],
        );
        value.evidence[0].source = SurfaceEvidenceSource::RemoteStream;
        value.evidence[0].quality = CoverageLevel::Strong;
        assert!(value.validate().is_ok());
    }
    #[test]
    fn terminal_and_native_input_use_strong_protocol_evidence() {
        let mut terminal = surface(
            SurfaceKind::Terminal,
            UnderstandingLevel::CoordinateOnly,
            vec![SurfaceCapability::Input],
        );
        terminal.evidence[0].source = SurfaceEvidenceSource::TerminalProtocol;
        terminal.evidence[0].quality = CoverageLevel::Strong;
        assert!(terminal.validate().is_ok());
        let mut native = surface(
            SurfaceKind::BrowserNative,
            UnderstandingLevel::CoordinateOnly,
            vec![SurfaceCapability::Input],
        );
        native.evidence[0].source = SurfaceEvidenceSource::BrowserNative;
        native.evidence[0].quality = CoverageLevel::Strong;
        assert!(native.validate().is_ok());
    }

    #[test]
    fn backend_provenance_can_support_strong_document_input() {
        let mut value = surface(
            SurfaceKind::Document,
            UnderstandingLevel::CoordinateOnly,
            vec![SurfaceCapability::Input],
        );
        value.evidence[0].provenance.source_class = ProvenanceSourceClass::Backend;
        assert!(value.validate().is_ok());
    }
    #[test]
    fn visual_only_evidence_cannot_authorize_coordinate_input() {
        let mut value = surface(
            SurfaceKind::Canvas2d,
            UnderstandingLevel::CoordinateOnly,
            vec![SurfaceCapability::CoordinateAction, SurfaceCapability::Input],
        );
        value.evidence[0].source = SurfaceEvidenceSource::Visual;
        value.evidence[0].provenance.source_class = ProvenanceSourceClass::Visual;
        assert!(value.validate().is_err());
    }

    #[test]
    fn bridge_invocation_requires_trusted_grant_evidence() {
        let mut value = surface(
            SurfaceKind::Document,
            UnderstandingLevel::Opaque,
            vec![SurfaceCapability::Bridge],
        );
        assert!(value.validate().is_err());
        value.evidence[0].source = SurfaceEvidenceSource::Bridge;
        assert!(value.validate().is_err());
    }

    #[test]
    fn bridge_registry_bounds_and_duplicate_insert_are_atomic() {
        let mut registry = BridgeGrantRegistry::default();
        let grant = BridgeCapabilityGrant {
            token: "grant-1".into(),
            origin: "https://example.test".into(),
            capabilities: vec![SurfaceCapability::ReadStructure],
        };
        registry.insert(grant.clone()).unwrap();
        assert!(registry.insert(grant).is_err());
        assert_eq!(registry.grants.len(), 1);
        let oversized = format!(r#"{{"grants":{{"x":"{}"}}}}"#, "x".repeat(MAX_BRIDGE_GRANT_PAYLOAD_BYTES));
        assert!(BridgeGrantRegistry::from_json(&oversized).is_err());
    }
    #[test]
    fn evidence_sources_are_rejected_when_bound_to_another_surface_kind() {
        let mut document = surface(
            SurfaceKind::Document,
            UnderstandingLevel::Structural,
            vec![SurfaceCapability::ReadStructure],
        );
        document.evidence[0].source = SurfaceEvidenceSource::Svg;
        assert!(document.validate().is_err());

        document.evidence[0].source = SurfaceEvidenceSource::TerminalProtocol;
        assert!(document.validate().is_err());

        let mut semantic_document = surface(
            SurfaceKind::Document,
            UnderstandingLevel::Semantic,
            vec![SurfaceCapability::ReadStructure],
        );
        semantic_document.evidence[0].source = SurfaceEvidenceSource::MediaMetadata;
        assert!(semantic_document.validate().is_err());
    }

    #[test]
    fn svg_evidence_can_supply_keyboard_input() {
        let mut value = surface(
            SurfaceKind::Svg,
            UnderstandingLevel::CoordinateOnly,
            vec![SurfaceCapability::Input],
        );
        value.evidence[0].source = SurfaceEvidenceSource::Svg;
        value.evidence[0].quality = CoverageLevel::Strong;
        assert!(value.validate().is_ok());
    }

    #[test]
    fn mixed_pointer_and_keyboard_input_requires_both_evidence_modalities() {
        let mut value = surface(
            SurfaceKind::Canvas2d,
            UnderstandingLevel::CoordinateOnly,
            vec![SurfaceCapability::CoordinateAction, SurfaceCapability::Input],
        );
        value.evidence[0].source = SurfaceEvidenceSource::CanvasDetection;
        value.evidence[0].quality = CoverageLevel::Strong;
        assert!(value.validate().is_err());
    }

}
