//! Validated extension metadata and least-privilege permissions.
//!
//! This module intentionally does not load native code or execute extension
//! entrypoints. It provides the versioned manifest and permission boundary
//! that a future extension host must enforce before adding execution.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
use std::fmt;
use std::path::{Component, Path, PathBuf};
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::Command;

const MAX_EXTENSION_MANIFESTS: usize = 128;
const MAX_EXTENSION_MESSAGE_BYTES: usize = 256 * 1024;
const EXTENSION_TIMEOUT: Duration = Duration::from_secs(5);

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

/// One bounded invocation sent to an extension process.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExtensionInvocation {
    pub protocol_version: u32,
    pub extension_id: String,
    pub capability: ExtensionCapability,
    pub host: String,
    pub action: String,
    pub payload: Value,
}

/// One extension process response.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ExtensionResponse {
    protocol_version: u32,
    ok: bool,
    #[serde(default)]
    result: Option<Value>,
    #[serde(default)]
    error: Option<String>,
}

/// A validated, process-isolated extension host boundary.
#[derive(Debug)]
pub struct ExtensionHost {
    root: PathBuf,
    registry: ExtensionRegistry,
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

    /// Load JSON manifests from one directory without executing entrypoints.
    pub fn load_dir(directory: impl AsRef<Path>) -> Result<Self, ExtensionError> {
        let directory = directory.as_ref();
        let metadata = std::fs::symlink_metadata(directory).map_err(|error| {
            ExtensionError(format!("extension directory is unavailable: {error}"))
        })?;
        if !metadata.is_dir() {
            return Err(ExtensionError("extension path must be a directory".into()));
        }
        let mut entries = std::fs::read_dir(directory)
            .map_err(|error| ExtensionError(format!("cannot read extension directory: {error}")))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| ExtensionError(format!("cannot enumerate extensions: {error}")))?;
        entries.sort_by_key(|entry| entry.file_name());
        if entries.len() > MAX_EXTENSION_MANIFESTS {
            return Err(ExtensionError(format!(
                "extension directory exceeds {MAX_EXTENSION_MANIFESTS} manifests"
            )));
        }
        let mut registry = Self::default();
        for entry in entries {
            let path = entry.path();
            if path.extension().and_then(|value| value.to_str()) != Some("json") {
                continue;
            }
            let metadata = std::fs::symlink_metadata(&path)
                .map_err(|error| ExtensionError(format!("cannot inspect manifest: {error}")))?;
            if !metadata.is_file() {
                return Err(ExtensionError(format!(
                    "extension manifest is not a regular file: {}",
                    path.display()
                )));
            }
            let manifest: ExtensionManifest = serde_json::from_slice(
                &std::fs::read(&path)
                    .map_err(|error| ExtensionError(format!("cannot read manifest: {error}")))?,
            )
            .map_err(|error| ExtensionError(format!("invalid extension manifest: {error}")))?;
            registry.register(manifest)?;
        }
        Ok(registry)
    }
}

impl ExtensionHost {
    /// Create a host whose entrypoints must remain below `root`.
    pub fn new(
        root: impl AsRef<Path>,
        registry: ExtensionRegistry,
    ) -> Result<Self, ExtensionError> {
        let root = std::fs::canonicalize(root.as_ref())
            .map_err(|error| ExtensionError(format!("extension root is unavailable: {error}")))?;
        if !root.is_dir() {
            return Err(ExtensionError("extension root must be a directory".into()));
        }
        Ok(Self { root, registry })
    }

    /// Invoke one declared extension capability through a bounded subprocess.
    pub async fn invoke(
        &self,
        extension_id: &str,
        capability: ExtensionCapability,
        host: &str,
        action: &str,
        payload: Value,
    ) -> Result<Value, ExtensionError> {
        let manifest = self
            .registry
            .get(extension_id)
            .ok_or_else(|| ExtensionError("extension is not registered".into()))?;
        if !manifest.capabilities.contains(&capability) {
            return Err(ExtensionError(
                "extension capability was not declared".into(),
            ));
        }
        if !manifest
            .permissions
            .hosts
            .iter()
            .any(|allowed| allowed == host)
        {
            return Err(ExtensionError(
                "extension host permission was not declared".into(),
            ));
        }
        if !manifest
            .permissions
            .actions
            .iter()
            .any(|allowed| allowed == action)
        {
            return Err(ExtensionError(
                "extension action permission was not declared".into(),
            ));
        }
        let entrypoint = manifest
            .entrypoint
            .as_deref()
            .ok_or_else(|| ExtensionError("extension has no executable entrypoint".into()))?;
        let entrypoint = confined_entrypoint(&self.root, entrypoint)?;
        let invocation = ExtensionInvocation {
            protocol_version: EXTENSION_SCHEMA_VERSION,
            extension_id: extension_id.into(),
            capability,
            host: host.into(),
            action: action.into(),
            payload,
        };
        let encoded = serde_json::to_vec(&invocation)
            .map_err(|error| ExtensionError(format!("cannot encode extension request: {error}")))?;
        if encoded.len() > MAX_EXTENSION_MESSAGE_BYTES {
            return Err(ExtensionError(
                "extension request exceeds the size limit".into(),
            ));
        }
        let mut child = Command::new(entrypoint)
            .current_dir(&self.root)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null())
            .spawn()
            .map_err(|error| {
                ExtensionError(format!("extension process could not start: {error}"))
            })?;
        let mut stdin = child
            .stdin
            .take()
            .ok_or_else(|| ExtensionError("extension stdin is unavailable".into()))?;
        let mut stdout = BufReader::new(
            child
                .stdout
                .take()
                .ok_or_else(|| ExtensionError("extension stdout is unavailable".into()))?,
        );
        stdin
            .write_all(&encoded)
            .await
            .map_err(|error| ExtensionError(format!("extension request failed: {error}")))?;
        stdin
            .write_all(b"\n")
            .await
            .map_err(|error| ExtensionError(format!("extension request failed: {error}")))?;
        stdin
            .shutdown()
            .await
            .map_err(|error| ExtensionError(format!("extension request failed: {error}")))?;
        let result = tokio::time::timeout(EXTENSION_TIMEOUT, async {
            let mut response = Vec::new();
            let bytes = stdout
                .read_until(b'\n', &mut response)
                .await
                .map_err(|error| ExtensionError(format!("extension response failed: {error}")))?;
            if bytes == 0 || response.len() > MAX_EXTENSION_MESSAGE_BYTES {
                return Err(ExtensionError(
                    "extension response is missing or oversized".into(),
                ));
            }
            let response: ExtensionResponse = serde_json::from_slice(&response)
                .map_err(|error| ExtensionError(format!("invalid extension response: {error}")))?;
            if response.protocol_version != EXTENSION_SCHEMA_VERSION {
                return Err(ExtensionError(
                    "unsupported extension response protocol".into(),
                ));
            }
            match (response.ok, response.result, response.error) {
                (true, Some(result), None) => Ok(result),
                (false, None, Some(error)) if error.len() <= 512 => Err(ExtensionError(error)),
                _ => Err(ExtensionError(
                    "extension response outcome is invalid".into(),
                )),
            }
        })
        .await;
        let timed_out = result.is_err();
        if timed_out {
            let _ = child.kill().await;
        }
        let status = child
            .wait()
            .await
            .map_err(|error| ExtensionError(format!("extension process failed: {error}")))?;
        if !status.success() && !timed_out {
            return Err(ExtensionError(
                "extension process exited unsuccessfully".into(),
            ));
        }
        if timed_out {
            return Err(ExtensionError("extension invocation timed out".into()));
        }
        result.expect("extension timeout was handled")
    }
}

fn confined_entrypoint(root: &Path, entrypoint: &str) -> Result<PathBuf, ExtensionError> {
    let path = Path::new(entrypoint);
    if path.is_absolute()
        || path
            .components()
            .any(|component| matches!(component, Component::ParentDir | Component::Prefix(_)))
    {
        return Err(ExtensionError(
            "extension entrypoint escapes its root".into(),
        ));
    }
    let path = std::fs::canonicalize(root.join(path))
        .map_err(|error| ExtensionError(format!("extension entrypoint is unavailable: {error}")))?;
    if !path.starts_with(root) || !path.is_file() {
        return Err(ExtensionError(
            "extension entrypoint is not a file below its root".into(),
        ));
    }
    Ok(path)
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

    #[cfg(unix)]
    #[tokio::test]
    async fn host_enforces_permissions_and_runs_one_bounded_rpc() {
        use std::os::unix::fs::PermissionsExt;

        let root =
            std::env::temp_dir().join(format!("glass-extension-host-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        let entrypoint = root.join("extension.sh");
        std::fs::write(
            &entrypoint,
            "#!/bin/sh\nread request\nprintf '%s\\n' '{\"protocolVersion\":1,\"ok\":true,\"result\":{\"accepted\":true}}'\n",
        )
        .unwrap();
        std::fs::set_permissions(&entrypoint, std::fs::Permissions::from_mode(0o700)).unwrap();

        let mut extension = manifest();
        extension.entrypoint = Some("extension.sh".into());
        let mut registry = ExtensionRegistry::default();
        registry.register(extension).unwrap();
        let host = ExtensionHost::new(&root, registry).unwrap();
        let result = host
            .invoke(
                "example.docs-adapter",
                ExtensionCapability::SiteAdapter,
                "docs.example.com",
                "observe",
                serde_json::json!({"revision": 1}),
            )
            .await
            .unwrap();
        assert_eq!(result["accepted"], true);
        assert!(
            host.invoke(
                "example.docs-adapter",
                ExtensionCapability::SiteAdapter,
                "other.example.com",
                "observe",
                serde_json::json!({}),
            )
            .await
            .is_err()
        );

        std::fs::remove_file(entrypoint).unwrap();
        std::fs::remove_dir(root).unwrap();
    }

    #[test]
    fn host_rejects_entrypoints_that_escape_root() {
        let root = std::env::current_dir().unwrap();
        let mut registry = ExtensionRegistry::default();
        let mut extension = manifest();
        extension.entrypoint = Some("../outside".into());
        registry.register(extension).unwrap();
        let host = ExtensionHost::new(&root, registry).unwrap();
        let runtime = tokio::runtime::Runtime::new().unwrap();
        let error = runtime.block_on(host.invoke(
            "example.docs-adapter",
            ExtensionCapability::SiteAdapter,
            "docs.example.com",
            "observe",
            serde_json::json!({}),
        ));
        assert!(error.unwrap_err().0.contains("escapes"));
    }
}
