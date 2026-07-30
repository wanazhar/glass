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
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
use std::fmt;

/// Stable Glass protocol version negotiated independently from MCP.
pub use crate::protocol::GLASS_PROTOCOL_VERSION;

/// A bounded, machine-readable description of one Glass runtime.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GlassCapabilityManifest {
    pub protocol_version: u32,
    pub glass_version: String,
    pub schemas: BTreeMap<String, Vec<u32>>,
    pub capabilities: BTreeMap<String, bool>,
    #[serde(default)]
    pub capability_statuses: BTreeMap<String, GlassCapabilityStatus>,
    pub constraints: GlassCapabilityConstraints,
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
}

/// Runtime and policy constraints that affect optional operations.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GlassCapabilityConstraints {
    pub platform: String,
    pub browser_family: String,
    pub policy: String,
    pub max_sessions: u32,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct GlassCapabilityRequest {
    #[serde(default = "default_protocol_version")]
    protocol_version: u32,
    #[serde(default)]
    schemas: BTreeMap<String, Vec<u32>>,
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

    /// Negotiate an optional Glass request against this manifest.
    pub fn negotiate(&self, request: Option<&Value>) -> Result<Self, CapabilityNegotiationError> {
        let Some(request) = request else {
            return Ok(self.clone());
        };
        let request: GlassCapabilityRequest =
            serde_json::from_value(request.clone()).map_err(|error| {
                CapabilityNegotiationError {
                    field: "glass".into(),
                    detail: format!("invalid capability request: {error}"),
                }
            })?;
        if request.protocol_version != self.protocol_version {
            return Err(CapabilityNegotiationError {
                field: "glass.protocolVersion".into(),
                detail: format!(
                    "unsupported protocol {}; expected {}",
                    request.protocol_version, self.protocol_version
                ),
            });
        }
        for (name, requested_versions) in request.schemas {
            let Some(supported_versions) = self.schemas.get(&name) else {
                return Err(CapabilityNegotiationError {
                    field: format!("glass.schemas.{name}"),
                    detail: "unknown schema".into(),
                });
            };
            if requested_versions.is_empty()
                || !requested_versions
                    .iter()
                    .any(|version| supported_versions.contains(version))
            {
                return Err(CapabilityNegotiationError {
                    field: format!("glass.schemas.{name}"),
                    detail: format!(
                        "requested versions do not intersect supported versions {supported_versions:?}"
                    ),
                });
            }
        }
        Ok(self.clone())
    }
}

fn supported_schemas() -> BTreeMap<String, Vec<u32>> {
    BTreeMap::from([
        ("protocol".into(), vec![GLASS_PROTOCOL_VERSION]),
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
        assert_eq!(manifest.constraints.max_sessions, 4);
        assert!(manifest.capabilities["workflowResume"]);
        assert!(!manifest.capabilities["localDaemon"]);
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
    fn negotiation_accepts_compatible_schema_requests() {
        let manifest = development_manifest();
        let request = serde_json::json!({
            "protocolVersion": 1,
            "schemas": {"action": [1], "workflow": [1]}
        });

        assert_eq!(manifest.negotiate(Some(&request)).unwrap(), manifest);
    }

    #[test]
    fn negotiation_rejects_unknown_and_incompatible_requests() {
        let manifest = development_manifest();
        let unknown = serde_json::json!({
            "protocolVersion": 1,
            "schemas": {"future": [1]}
        });
        let incompatible = serde_json::json!({
            "protocolVersion": 1,
            "schemas": {"workflow": [99]}
        });

        assert!(manifest.negotiate(Some(&unknown)).is_err());
        assert!(manifest.negotiate(Some(&incompatible)).is_err());
    }
}
