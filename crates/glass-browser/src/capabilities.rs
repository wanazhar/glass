//! Versioned Glass protocol and capability negotiation.
//!
//! MCP remains the transport envelope, while this manifest describes the
//! Glass contracts carried by that envelope. Clients may omit the request for
//! backward compatibility; newer clients can request exact schema versions
//! and receive a typed negotiation error before using optional features.

use crate::browser::policy::{
    BrowserPolicy, POLICY_SCHEMA_VERSION, PolicyCapability, PolicyDecision, PolicyPreset,
};
use crate::browser::session::{
    INTENT_RESOLUTION_SCHEMA_VERSION, KNOWLEDGE_SCHEMA_VERSION,
    SEMANTIC_OBSERVATION_SCHEMA_VERSION, WORKFLOW_AUTHORING_SCHEMA_VERSION,
    WORKFLOW_SCHEMA_VERSION,
};
use crate::extensions::{ExtensionSandbox, experimental_extension_target_supported};
use crate::reliability::{
    RELIABILITY_FIXTURE_SCHEMA_VERSION, RELIABILITY_REPLAY_SCHEMA_VERSION,
    RELIABILITY_SCENARIO_SCHEMA_VERSION,
};
use crate::task_protocol::TASK_PROTOCOL_SCHEMA_VERSION;
use crate::web_ir::WEB_IR_SCHEMA_VERSION;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

/// Stable Glass protocol version negotiated independently from MCP.
pub use crate::protocol::GLASS_PROTOCOL_VERSION;

/// A bounded, machine-readable description of one Glass runtime.
///
/// This is discovery output. It intentionally describes the complete runtime
/// inventory and is kept separate from [`NegotiatedCapabilities`], which is the
/// contract a client may rely on after initialization.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GlassCapabilityManifest {
    pub protocol_version: u32,
    pub glass_version: String,
    pub schemas: BTreeMap<String, Vec<u32>>,
    pub capabilities: BTreeMap<String, bool>,
    #[serde(default)]
    pub capability_statuses: BTreeMap<String, GlassCapabilityStatus>,
    pub constraints: GlassCapabilityConstraints,
}

/// The immutable capability contract selected for one connection.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NegotiatedCapabilities {
    pub protocol_version: u32,
    pub agreed_schemas: BTreeMap<String, u32>,
    pub capabilities: BTreeMap<String, NegotiatedCapability>,
    pub constraints: GlassCapabilityConstraints,
}

/// Effective status for one capability in a negotiated agreement.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NegotiatedCapability {
    pub status: GlassCapabilityStatus,
}

impl NegotiatedCapability {
    fn from_status(status: GlassCapabilityStatus) -> Self {
        Self { status }
    }
}

/// Why a capability is or is not available in the current runtime.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum GlassCapabilityStatus {
    Available,
    AvailableUncertified,
    Experimental,
    DisabledByPolicy,
    UnavailableOnPlatform,
    MissingRuntimeDependency,
    BlockedBySecurityGate,
    #[serde(other)]
    Unknown,
}

/// Client requirements used to form an effective Glass agreement.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CapabilityNegotiationRequest {
    #[serde(default)]
    pub protocol_versions: Vec<u32>,
    #[serde(default)]
    pub protocol_version: Option<u32>,
    #[serde(default)]
    pub schemas: BTreeMap<String, Vec<u32>>,
    #[serde(default)]
    pub requires: Vec<String>,
    #[serde(default)]
    pub optional: Vec<String>,
    #[serde(default)]
    pub accepts_experimental: bool,
}

/// Runtime and policy constraints that affect optional operations.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GlassCapabilityConstraints {
    pub platform: String,
    pub browser_family: String,
    pub policy: String,
    pub max_sessions: u32,
}

impl GlassCapabilityManifest {
    /// Reject impossible capability boolean/status combinations.
    pub fn validate(&self) -> Result<(), CapabilityNegotiationError> {
        for (name, enabled) in &self.capabilities {
            let Some(status) = self.capability_statuses.get(name) else {
                continue;
            };
            let blocking = matches!(
                status,
                GlassCapabilityStatus::DisabledByPolicy
                    | GlassCapabilityStatus::UnavailableOnPlatform
                    | GlassCapabilityStatus::MissingRuntimeDependency
                    | GlassCapabilityStatus::BlockedBySecurityGate
            );
            if *enabled && blocking {
                return Err(CapabilityNegotiationError {
                    field: format!("glass.capabilityStatuses.{name}"),
                    detail: format!("enabled capability cannot have blocking status {status:?}"),
                });
            }
            if matches!(status, GlassCapabilityStatus::Unknown) {
                return Err(CapabilityNegotiationError {
                    field: format!("glass.capabilityStatuses.{name}"),
                    detail: "unknown capability status is not negotiable".into(),
                });
            }
        }
        Ok(())
    }
}

/// A bounded failure returned before an unsupported contract is used.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CapabilityNegotiationError {
    pub field: String,
    pub detail: String,
}

impl fmt::Display for CapabilityNegotiationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}: {}", self.field, self.detail)
    }
}

impl std::error::Error for CapabilityNegotiationError {}

fn normalize_request(
    value: &Value,
) -> Result<CapabilityNegotiationRequest, CapabilityNegotiationError> {
    let mut request: CapabilityNegotiationRequest =
        serde_json::from_value(value.clone()).map_err(|error| CapabilityNegotiationError {
            field: "glass".into(),
            detail: format!("invalid capability request: {error}"),
        })?;
    if request.protocol_versions.is_empty() {
        request.protocol_versions = request.protocol_version.into_iter().collect();
    }
    if request.protocol_versions.is_empty() {
        request.protocol_versions.push(default_protocol_version());
    }
    Ok(request)
}

impl GlassCapabilityManifest {
    /// Build the manifest for the current binary and policy.
    pub fn for_policy(policy: &BrowserPolicy) -> Self {
        Self::for_policy_in_mode(policy, false)
    }

    /// Build a manifest for an explicit runtime mode.
    pub fn for_policy_in_mode(policy: &BrowserPolicy, local_daemon: bool) -> Self {
        Self::for_policy_in_mode_with_experimental_extensions(policy, local_daemon, false)
    }

    /// Build a manifest with an explicit experimental extension opt-in.
    pub fn for_policy_with_experimental_extensions(
        policy: &BrowserPolicy,
        experimental_extensions: bool,
    ) -> Self {
        Self::for_policy_in_mode_with_experimental_extensions(
            policy,
            false,
            experimental_extensions,
        )
    }

    /// Build a manifest for an explicit runtime mode and extension opt-in.
    pub fn for_policy_in_mode_with_experimental_extensions(
        policy: &BrowserPolicy,
        local_daemon: bool,
        experimental_extensions: bool,
    ) -> Self {
        let extensions_enabled = experimental_extensions
            && experimental_extension_target_supported()
            && !matches!(ExtensionSandbox::detect(), ExtensionSandbox::Unavailable);
        let raw_cdp = matches!(
            policy.decide(PolicyCapability::RawCdp),
            PolicyDecision::Allow
        );
        let persistent_profile = matches!(
            policy.decide(PolicyCapability::PersistentProfile),
            PolicyDecision::Allow
        );
        let mut capabilities = BTreeMap::from([
            ("action", true),
            ("semanticRegions", true),
            ("taskProtocol", true),
            ("webIr", true),
            ("observationDiffs", true),
            ("intentResolution", true),
            ("workflowRuntime", true),
            ("workflowResume", true),
            ("persistentKnowledge", persistent_profile),
            ("workflowAuthoring", true),
            ("reliabilityCertification", true),
            ("rawCdp", raw_cdp),
            ("localDaemon", local_daemon),
            ("extensions", extensions_enabled),
        ])
        .into_iter()
        .map(|(name, enabled)| (name.to_string(), enabled))
        .collect::<BTreeMap<_, _>>();
        capabilities.insert("mcpStdio".into(), true);
        let capability_statuses = capabilities
            .iter()
            .map(|(name, enabled)| {
                let status = match name.as_str() {
                    "extensions" if *enabled => GlassCapabilityStatus::Experimental,
                    "extensions" => {
                        if experimental_extensions {
                            GlassCapabilityStatus::BlockedBySecurityGate
                        } else {
                            GlassCapabilityStatus::DisabledByPolicy
                        }
                    }
                    "rawCdp" | "persistentKnowledge" if !enabled => {
                        GlassCapabilityStatus::DisabledByPolicy
                    }
                    _ if *enabled => GlassCapabilityStatus::Available,
                    _ => GlassCapabilityStatus::AvailableUncertified,
                };
                (name.clone(), status)
            })
            .collect();
        Self {
            protocol_version: GLASS_PROTOCOL_VERSION,
            glass_version: env!("CARGO_PKG_VERSION").into(),
            schemas: supported_schemas(),
            capabilities,
            capability_statuses,
            constraints: GlassCapabilityConstraints {
                platform: platform_label().into(),
                browser_family: "chromium".into(),
                policy: policy_label(policy.preset()).into(),
                max_sessions: 4,
            },
        }
    }

    /// Negotiate a bounded, immutable agreement against this manifest.
    ///
    /// `None` retains backwards-compatible discovery semantics by agreeing to
    /// the server's complete inventory. A request selects exact schema
    /// versions and may require or optionally accept named capabilities.
    pub fn negotiate(
        &self,
        request: Option<&Value>,
    ) -> Result<NegotiatedCapabilities, CapabilityNegotiationError> {
        self.validate()?;
        let request = match request {
            Some(value) => normalize_request(value)?,
            None => CapabilityNegotiationRequest {
                protocol_versions: vec![self.protocol_version],
                protocol_version: None,
                schemas: BTreeMap::new(),
                requires: Vec::new(),
                optional: self.capabilities.keys().cloned().collect(),
                accepts_experimental: true,
            },
        };
        if !request.protocol_versions.contains(&self.protocol_version) {
            return Err(CapabilityNegotiationError {
                field: "glass.protocolVersions".into(),
                detail: format!(
                    "unsupported protocols {:?}; expected {}",
                    request.protocol_versions, self.protocol_version
                ),
            });
        }

        let mut agreed_schemas = BTreeMap::new();
        for (name, requested_versions) in request.schemas {
            let Some(supported_versions) = self.schemas.get(&name) else {
                return Err(CapabilityNegotiationError {
                    field: format!("glass.schemas.{name}"),
                    detail: "unknown schema".into(),
                });
            };
            let Some(version) = requested_versions
                .iter()
                .filter(|version| supported_versions.contains(version))
                .max()
                .copied()
            else {
                return Err(CapabilityNegotiationError {
                    field: format!("glass.schemas.{name}"),
                    detail: format!(
                        "requested versions do not intersect supported versions {supported_versions:?}"
                    ),
                });
            };
            agreed_schemas.insert(name, version);
        }
        if agreed_schemas.is_empty() {
            agreed_schemas = self
                .schemas
                .iter()
                .filter_map(|(name, versions)| {
                    versions
                        .iter()
                        .max()
                        .map(|version| (name.clone(), *version))
                })
                .collect();
        }

        let required = request.requires.into_iter().collect::<BTreeSet<_>>();
        let optional = request.optional.into_iter().collect::<BTreeSet<_>>();
        let requested_capabilities: BTreeSet<String> = if required.is_empty() && optional.is_empty()
        {
            self.capabilities.keys().cloned().collect()
        } else {
            required.union(&optional).cloned().collect()
        };
        let mut capabilities = BTreeMap::new();
        for name in requested_capabilities {
            let Some(enabled) = self.capabilities.get(&name) else {
                if required.contains(&name) {
                    return Err(CapabilityNegotiationError {
                        field: format!("glass.requires.{name}"),
                        detail: "unknown capability".into(),
                    });
                }
                continue;
            };
            let status = self
                .capability_statuses
                .get(&name)
                .copied()
                .unwrap_or(if *enabled {
                    GlassCapabilityStatus::Available
                } else {
                    GlassCapabilityStatus::UnavailableOnPlatform
                });
            if required.contains(&name) {
                if !*enabled {
                    return Err(CapabilityNegotiationError {
                        field: format!("glass.requires.{name}"),
                        detail: format!("capability is not enabled ({status:?})"),
                    });
                }
                if status == GlassCapabilityStatus::Experimental && !request.accepts_experimental {
                    return Err(CapabilityNegotiationError {
                        field: format!("glass.requires.{name}"),
                        detail: "experimental capability requires acceptsExperimental=true".into(),
                    });
                }
            } else if status == GlassCapabilityStatus::Experimental && !request.accepts_experimental
            {
                continue;
            }
            capabilities.insert(name, NegotiatedCapability::from_status(status));
        }

        Ok(NegotiatedCapabilities {
            protocol_version: self.protocol_version,
            agreed_schemas,
            capabilities,
            constraints: self.constraints.clone(),
        })
    }
}

fn supported_schemas() -> BTreeMap<String, Vec<u32>> {
    BTreeMap::from([
        ("protocol".into(), vec![GLASS_PROTOCOL_VERSION]),
        ("task".into(), vec![TASK_PROTOCOL_SCHEMA_VERSION]),
        ("webIr".into(), vec![WEB_IR_SCHEMA_VERSION]),
        ("action".into(), vec![1]),
        (
            "observation".into(),
            vec![SEMANTIC_OBSERVATION_SCHEMA_VERSION],
        ),
        ("workflow".into(), vec![WORKFLOW_SCHEMA_VERSION]),
        ("checkpoint".into(), vec![1]),
        ("policy".into(), vec![POLICY_SCHEMA_VERSION]),
        ("workflowCheckpoint".into(), vec![1]),
        ("trace".into(), vec![1]),
        ("intent".into(), vec![INTENT_RESOLUTION_SCHEMA_VERSION]),
        ("knowledge".into(), vec![KNOWLEDGE_SCHEMA_VERSION]),
        ("authoring".into(), vec![WORKFLOW_AUTHORING_SCHEMA_VERSION]),
        (
            "reliabilityScenario".into(),
            vec![RELIABILITY_SCENARIO_SCHEMA_VERSION],
        ),
        (
            "reliabilityFixture".into(),
            vec![RELIABILITY_FIXTURE_SCHEMA_VERSION],
        ),
        (
            "reliabilityReplay".into(),
            vec![RELIABILITY_REPLAY_SCHEMA_VERSION],
        ),
    ])
}

fn default_protocol_version() -> u32 {
    GLASS_PROTOCOL_VERSION
}

fn policy_label(policy: PolicyPreset) -> &'static str {
    match policy {
        PolicyPreset::Development => "development",
        PolicyPreset::Ci => "ci",
        PolicyPreset::Polite => "polite",
        PolicyPreset::Hardened => "hardened",
        PolicyPreset::UntrustedMcp => "untrusted-mcp",
    }
}

fn platform_label() -> &'static str {
    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    {
        "linux-x86_64"
    }
    #[cfg(all(target_os = "linux", target_arch = "aarch64"))]
    {
        "linux-arm64"
    }
    #[cfg(all(target_os = "macos", target_arch = "x86_64"))]
    {
        "macos-x86_64"
    }
    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    {
        "macos-aarch64"
    }
    #[cfg(not(any(
        all(target_os = "linux", target_arch = "x86_64"),
        all(target_os = "linux", target_arch = "aarch64"),
        all(target_os = "macos", target_arch = "x86_64"),
        all(target_os = "macos", target_arch = "aarch64")
    )))]
    {
        "unsupported"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn development_manifest() -> GlassCapabilityManifest {
        let policy = BrowserPolicy::development(std::env::current_dir().unwrap()).unwrap();
        GlassCapabilityManifest::for_policy(&policy)
    }

    #[test]
    fn manifest_is_stable_and_lists_current_contracts() {
        let manifest = development_manifest();

        assert_eq!(manifest.protocol_version, 1);
        assert_eq!(manifest.schemas["workflow"], vec![1]);
        assert_eq!(manifest.schemas["reliabilityReplay"], vec![1]);
        assert_eq!(manifest.schemas["task"], vec![1]);
        assert_eq!(manifest.schemas["webIr"], vec![1]);
        assert_eq!(manifest.constraints.max_sessions, 4);
        assert!(manifest.capabilities["workflowResume"]);
        assert!(!manifest.capabilities["localDaemon"]);
        assert!(manifest.capabilities["taskProtocol"]);
        assert!(manifest.capabilities["webIr"]);
        assert_eq!(
            manifest.capability_statuses["taskProtocol"],
            GlassCapabilityStatus::Available
        );
        assert_eq!(
            manifest.capability_statuses["webIr"],
            GlassCapabilityStatus::Available
        );
        assert_eq!(
            manifest.capability_statuses["extensions"],
            GlassCapabilityStatus::DisabledByPolicy
        );
        assert_eq!(
            manifest.capability_statuses["rawCdp"],
            GlassCapabilityStatus::Available
        );
    }

    #[test]
    fn experimental_extensions_require_opt_in_and_a_native_sandbox() {
        let policy = BrowserPolicy::development(std::env::current_dir().unwrap()).unwrap();
        let manifest =
            GlassCapabilityManifest::for_policy_with_experimental_extensions(&policy, true);
        if !experimental_extension_target_supported()
            || matches!(ExtensionSandbox::detect(), ExtensionSandbox::Unavailable)
        {
            assert!(!manifest.capabilities["extensions"]);
            assert_eq!(
                manifest.capability_statuses["extensions"],
                GlassCapabilityStatus::BlockedBySecurityGate
            );
        } else {
            assert!(manifest.capabilities["extensions"]);
            assert_eq!(
                manifest.capability_statuses["extensions"],
                GlassCapabilityStatus::Experimental
            );
        }
    }

    #[test]
    fn negotiation_returns_effective_schema_and_capability_agreement() {
        let manifest = development_manifest();
        let request = serde_json::json!({
            "protocolVersions": [99, 1],
            "schemas": {
                "action": [99, 1],
                "workflow": [1],
                "task": [1],
                "webIr": [1]
            },
            "requires": ["workflowResume"],
            "optional": ["extensions", "taskProtocol", "webIr"],
            "acceptsExperimental": false,
            "futureField": true
        });

        let agreement = manifest.negotiate(Some(&request)).unwrap();
        assert_eq!(agreement.protocol_version, 1);
        assert_eq!(agreement.agreed_schemas["action"], 1);
        assert_eq!(agreement.agreed_schemas["workflow"], 1);
        assert_eq!(agreement.agreed_schemas["task"], 1);
        assert_eq!(agreement.agreed_schemas["webIr"], 1);
        assert_eq!(
            agreement.capabilities["workflowResume"].status,
            GlassCapabilityStatus::Available
        );
        assert_eq!(
            agreement.capabilities["extensions"].status,
            GlassCapabilityStatus::DisabledByPolicy
        );
        assert_eq!(
            agreement.capabilities["taskProtocol"].status,
            GlassCapabilityStatus::Available
        );
        assert_eq!(
            agreement.capabilities["webIr"].status,
            GlassCapabilityStatus::Available
        );
        assert_eq!(manifest.schemas["workflow"], vec![1]);
    }

    #[test]
    fn negotiation_rejects_unknown_and_incompatible_requests() {
        let manifest = development_manifest();
        let unknown_schema = serde_json::json!({
            "protocolVersion": 1,
            "schemas": {"future": [1]}
        });
        let incompatible_schema = serde_json::json!({
            "protocolVersion": 1,
            "schemas": {"workflow": [99]}
        });
        let missing_required_capability = serde_json::json!({
            "protocolVersions": [1],
            "requires": ["extensions"]
        });

        assert!(manifest.negotiate(Some(&unknown_schema)).is_err());
        assert!(manifest.negotiate(Some(&incompatible_schema)).is_err());
        assert!(
            manifest
                .negotiate(Some(&missing_required_capability))
                .is_err()
        );
    }

    #[test]
    fn manifest_validation_rejects_enabled_blocked_capabilities() {
        let mut manifest = development_manifest();
        manifest.capabilities.insert("broken".into(), true);
        manifest.capability_statuses.insert(
            "broken".into(),
            GlassCapabilityStatus::BlockedBySecurityGate,
        );

        let error = manifest.validate().unwrap_err();
        assert_eq!(error.field, "glass.capabilityStatuses.broken");
    }

    #[test]
    fn unknown_manifest_fields_and_statuses_are_tolerated_on_decode() {
        let value = serde_json::json!({
            "protocolVersion": 1,
            "glassVersion": "0.2.2",
            "schemas": {},
            "capabilities": {"future": false},
            "capabilityStatuses": {"future": "addedLater"},
            "constraints": {
                "platform": "linux-arm64",
                "browserFamily": "chromium",
                "policy": "development",
                "maxSessions": 4,
                "futureConstraint": true
            },
            "futureManifestField": true
        });
        let manifest: GlassCapabilityManifest = serde_json::from_value(value).unwrap();
        assert_eq!(
            manifest.capability_statuses["future"],
            GlassCapabilityStatus::Unknown
        );
        assert!(manifest.validate().is_err());
    }
}
