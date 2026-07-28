//! Validated extension metadata and least-privilege permissions.
//!
//! This module intentionally does not load native code or execute extension
//! entrypoints. It provides the versioned manifest and permission boundary
//! that a future extension host must enforce before adding execution.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fmt;

/// Version of the extension manifest contract.
pub const EXTENSION_SCHEMA_VERSION: u32 = 1;

/// Extension points that can be negotiated without granting raw browser access.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Ord, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ExtensionCapability {
    CustomVerification,
    SemanticRegionClassifier,
    IntentEvidencePack,
    ExtractionTransform,
    SiteAdapter,
    WorkflowTemplate,
    TraceExporter,
    KnowledgeStore,
    StrictPolicy,
}

/// Least-privilege host and action permissions.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ExtensionPermissions {
    pub hosts: Vec<String>,
    pub actions: Vec<String>,
}

/// Versioned extension metadata. It is data, not an execution grant.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ExtensionManifest {
    pub schema_version: u32,
    pub id: String,
    pub version: String,
    pub api_version: u32,
    pub capabilities: Vec<ExtensionCapability>,
    pub permissions: ExtensionPermissions,
    pub entrypoint: Option<String>,
}

/// Bounded extension registry for metadata and negotiation.
#[derive(Debug, Default)]
pub struct ExtensionRegistry {
    manifests: BTreeMap<String, ExtensionManifest>,
}

/// Extension validation or registry failure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExtensionError(pub String);

impl fmt::Display for ExtensionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl std::error::Error for ExtensionError {}

impl ExtensionManifest {
    /// Validate the manifest without loading or trusting its entrypoint.
    pub fn validate(&self) -> Result<(), ExtensionError> {
        const MAX_TEXT_BYTES: usize = 256;
        const MAX_HOSTS: usize = 32;
        const MAX_ACTIONS: usize = 32;
        if self.schema_version != EXTENSION_SCHEMA_VERSION {
            return Err(ExtensionError(format!(
                "unsupported extension schema {}; expected {}",
                self.schema_version, EXTENSION_SCHEMA_VERSION
            )));
        }
        for (name, value) in [("id", self.id.as_str()), ("version", self.version.as_str())] {
            if value.is_empty() || value.len() > MAX_TEXT_BYTES {
                return Err(ExtensionError(format!(
                    "extension {name} must be 1..={MAX_TEXT_BYTES} bytes"
                )));
            }
        }
        if self.api_version == 0 {
            return Err(ExtensionError(
                "extension apiVersion must be positive".into(),
            ));
        }
        if self.capabilities.is_empty() {
            return Err(ExtensionError(
                "extension must declare at least one capability".into(),
            ));
        }
        if self.permissions.hosts.is_empty() || self.permissions.hosts.len() > MAX_HOSTS {
            return Err(ExtensionError(format!(
                "extension hosts must contain 1..={MAX_HOSTS} entries"
            )));
        }
        if self.permissions.actions.is_empty() || self.permissions.actions.len() > MAX_ACTIONS {
            return Err(ExtensionError(format!(
                "extension actions must contain 1..={MAX_ACTIONS} entries"
            )));
        }
        for host in &self.permissions.hosts {
            if host.is_empty() || host.len() > MAX_TEXT_BYTES || host == "*" || host.contains('/') {
                return Err(ExtensionError(format!(
                    "extension host is not an exact host: {host:?}"
                )));
            }
        }
        for action in &self.permissions.actions {
            if !allowed_action(action) {
                return Err(ExtensionError(format!(
                    "extension action is not allowed: {action:?}"
                )));
            }
        }
        if let Some(entrypoint) = &self.entrypoint
            && (entrypoint.is_empty() || entrypoint.len() > MAX_TEXT_BYTES)
        {
            return Err(ExtensionError(
                "extension entrypoint is out of bounds".into(),
            ));
        }
        Ok(())
    }
}

impl ExtensionRegistry {
    /// Register validated metadata, rejecting duplicate IDs.
    pub fn register(&mut self, manifest: ExtensionManifest) -> Result<(), ExtensionError> {
        manifest.validate()?;
        if self.manifests.contains_key(&manifest.id) {
            return Err(ExtensionError(format!(
                "extension ID is already registered: {}",
                manifest.id
            )));
        }
        self.manifests.insert(manifest.id.clone(), manifest);
        Ok(())
    }

    /// Return registered metadata without exposing executable state.
    pub fn get(&self, id: &str) -> Option<&ExtensionManifest> {
        self.manifests.get(id)
    }

    /// Return deterministic registered metadata.
    pub fn manifests(&self) -> impl Iterator<Item = &ExtensionManifest> {
        self.manifests.values()
    }
}

fn allowed_action(action: &str) -> bool {
    matches!(
        action,
        "navigate"
            | "observe"
            | "verify"
            | "click"
            | "type"
            | "clear"
            | "check"
            | "uncheck"
            | "select"
            | "scroll"
            | "extract"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn manifest() -> ExtensionManifest {
        ExtensionManifest {
            schema_version: EXTENSION_SCHEMA_VERSION,
            id: "example.docs-adapter".into(),
            version: "1.0.0".into(),
            api_version: 1,
            capabilities: vec![ExtensionCapability::SiteAdapter],
            permissions: ExtensionPermissions {
                hosts: vec!["docs.example.com".into()],
                actions: vec!["navigate".into(), "observe".into(), "extract".into()],
            },
            entrypoint: Some("metadata-only".into()),
        }
    }

    #[test]
    fn manifest_validation_is_strict_and_does_not_grant_execution() {
        let mut registry = ExtensionRegistry::default();
        registry.register(manifest()).unwrap();
        assert_eq!(registry.manifests().count(), 1);
        assert!(registry.register(manifest()).is_err());

        let mut wildcard = manifest();
        wildcard.permissions.hosts = vec!["*".into()];
        assert!(wildcard.validate().is_err());
    }

    #[test]
    fn mutation_permissions_are_explicitly_bounded() {
        let mut extension = manifest();
        extension.permissions.actions = vec!["evaluate".into()];
        assert!(extension.validate().is_err());
    }
}
