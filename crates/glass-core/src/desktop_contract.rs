//! Per-workspace desktop config and credential allowlist contract.
//!
//! Forward-fit slice for Glass Core: bounded contract types that bind one
//! workspace to its managed desktop session configuration and to the credential
//! allowlist that workspace may use. Runtime wiring lands in later slices; a
//! workspace without an allowlist permits no credential.

use serde::{Deserialize, Serialize};

/// Maximum identifier length accepted in workspace-scoped desktop and
/// credential allowlist entries.
pub const MAX_ID_BYTES: usize = 256;

/// Managed desktop session configuration for one workspace.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct DesktopConfig {
    /// Workspace the desktop session is configured for.
    pub workspace_id: String,
    /// Managed desktop environment: "gnome" or "xfce".
    pub environment: String,
}

impl DesktopConfig {
    pub fn new(
        workspace_id: impl Into<String>,
        environment: impl Into<String>,
    ) -> Result<Self, String> {
        let workspace_id = workspace_id.into();
        if workspace_id.trim().is_empty() || workspace_id.len() > MAX_ID_BYTES {
            return Err("desktop workspace id is invalid".into());
        }
        let environment = environment.into();
        if !matches!(environment.as_str(), "gnome" | "xfce") {
            return Err(format!(
                "desktop environment must be gnome or xfce (got `{environment}`)"
            ));
        }
        Ok(Self { workspace_id, environment })
    }

    pub fn environment(&self) -> Option<&'static str> {
        match self.environment.as_str() {
            "gnome" => Some("gnome"),
            "xfce" => Some("xfce"),
            _ => None,
        }
    }
}

/// Credential allowlist scoped to one workspace; an empty allowlist permits no
/// credential.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct CredentialAllowlist {
    /// Workspace the allowlist applies to.
    pub workspace_id: String,
    /// Credential identifiers the workspace may use.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub credentials: Vec<String>,
}

impl CredentialAllowlist {
    pub fn new(
        workspace_id: impl Into<String>,
        credentials: Vec<String>,
    ) -> Result<Self, String> {
        let workspace_id = workspace_id.into();
        if workspace_id.trim().is_empty() || workspace_id.len() > MAX_ID_BYTES {
            return Err("credential allowlist workspace id is invalid".into());
        }
        for credential in &credentials {
            if credential.trim().is_empty() || credential.len() > MAX_ID_BYTES {
                return Err("credential allowlist entry is invalid".into());
            }
        }
        Ok(Self { workspace_id, credentials })
    }

    /// Whether the workspace may use the credential. Fails closed: an empty
    /// allowlist, an unknown workspace, or an unknown credential grants nothing.
    pub fn allows(&self, workspace_id: &str, credential_id: &str) -> bool {
        if self.workspace_id.as_str() != workspace_id {
            return false;
        }
        for credential in &self.credentials {
            if credential.as_str() == credential_id {
                return true;
            }
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn workspace_desktop_config_and_credential_allowlist_fail_closed() {
        let desktop = DesktopConfig::new("workspace-a", "gnome").unwrap();
        assert_eq!(desktop.environment(), Some("gnome"));
        assert!(DesktopConfig::new("workspace-a", "permissive").is_err());

        let credentials = vec!["credential-a".to_string()];
        let allowlist = CredentialAllowlist::new("workspace-a", credentials).unwrap();
        assert!(allowlist.allows("workspace-a", "credential-a"));
        assert!(!allowlist.allows("workspace-a", "credential-b"));
        assert!(!allowlist.allows("workspace-b", "credential-a"));
    }
}
