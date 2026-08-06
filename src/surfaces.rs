//! Stable, bounded, transport-neutral browser surface contracts.
//!
//! A surface is an evidenced boundary in a browser-hosted experience.  Surface
//! detection is not an action grant: callers must declare capabilities backed by
//! current evidence and validate the complete contract before using it.

use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
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

/// A namespaced extension identifier.  The namespace is part of identity, so
/// an extension cannot collide with a core or another vendor's concept.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ExtensionIdentifier {
    pub namespace: String,
    pub name: String,
}

impl ExtensionIdentifier {
    pub fn new(
        namespace: impl Into<String>,
        name: impl Into<String>,
    ) -> Result<Self, SurfaceContractError> {
        let identifier = Self {
            namespace: namespace.into(),
            name: name.into(),
        };
        identifier.validate()?;
        Ok(identifier)
    }

    pub fn validate(&self) -> Result<(), SurfaceContractError> {
        validate_namespaced_component("extension.namespace", &self.namespace)?;
        validate_namespaced_component("extension.name", &self.name)
    }

    pub fn qualified_name(&self) -> String {
        format!("{}:{}", self.namespace, self.name)
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
    Visual,
    Extension,
}

/// One provenance record.  It describes evidence and does not authorize an
/// action or import a transport-specific identifier.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SurfaceEvidence {
    pub source: SurfaceEvidenceSource,
    pub quality: CoverageLevel,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

impl SurfaceEvidence {
    fn validate(&self, path: &str) -> Result<(), SurfaceContractError> {
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
            evidence.validate(&format!("evidence[{index}]"))?;
        }
        SurfaceRevision::new(self.revision.0)?;
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
                SurfaceCapability::CoordinateAction | SurfaceCapability::Input => {
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
            if matches!(capability, SurfaceCapability::CoordinateAction | SurfaceCapability::Input)
                && self.coverage.interaction < InteractionCoverage::CoordinateOnly
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
        let surface: Self = serde_json::from_str(input).map_err(|error| {
            SurfaceContractError::new("$", format!("invalid surface: {error}"))
        })?;
        surface.validate()?;
        if input.len() > MAX_SURFACE_PAYLOAD_BYTES {
            return Err(SurfaceContractError::new(
                "$",
                "surface payload exceeds the contract bound",
            ));
        }
        Ok(surface)
    }

    /// Serialize a validated surface using the stable serde representation.
    pub fn to_canonical_json(&self) -> Result<String, SurfaceContractError> {
        self.validate()?;
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
            surface.validate().map_err(|error| error.at(&format!("surfaces[{index}]")))?;
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
        let set: Self = serde_json::from_str(input).map_err(|error| {
            SurfaceContractError::new("$", format!("invalid surface set: {error}"))
        })?;
        set.validate()?;
        if input.len() > MAX_SURFACE_PAYLOAD_BYTES {
            return Err(SurfaceContractError::new(
                "$",
                "surface payload exceeds the contract bound",
            ));
        }
        Ok(set)
    }

    pub fn to_canonical_json(&self) -> Result<String, SurfaceContractError> {
        self.validate()?;
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
    if !valid || !value.as_bytes()[0].is_ascii_alphanumeric() || !value.as_bytes()[value.len() - 1].is_ascii_alphanumeric() {
        return Err(SurfaceContractError::new(
            path,
            "identifier must use alphanumeric namespaced characters and cannot begin or end with punctuation",
        ));
    }
    Ok(())
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
            evidence: vec![SurfaceEvidence { source: SurfaceEvidenceSource::Accessibility, quality: CoverageLevel::Strong, detail: None }],
            revision: SurfaceRevision(1),
            diagnostics: vec![],
        }
    }

    #[test]
    fn round_trip_and_extension_namespace() {
        let extension = ExtensionIdentifier::new("vendor.example", "chart.series").unwrap();
        let value = surface(SurfaceKind::ExtensionDefined { extension }, UnderstandingLevel::Semantic, vec![SurfaceCapability::ReadStructure]);
        let encoded = value.to_canonical_json().unwrap();
        assert_eq!(Surface::from_json(&encoded).unwrap(), value);
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
}
