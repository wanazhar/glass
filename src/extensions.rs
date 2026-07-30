//! Validated extension metadata and least-privilege permissions.
//!
//! The host executes one declared entrypoint request in a bounded subprocess.
//! Native-code sandboxing and integration with guarded browser operations remain
//! separate release gates; the `extensions` capability stays disabled until
//! those guarantees are present.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
use std::fmt;
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::Command;
use tokio::sync::Semaphore;

const MAX_EXTENSION_MANIFESTS: usize = 128;
const MAX_EXTENSION_MESSAGE_BYTES: usize = 256 * 1024;
const MAX_EXTENSION_CONCURRENT_INVOCATIONS: usize = 4;
const EXTENSION_TIMEOUT: Duration = Duration::from_secs(5);
const MAX_EXTENSION_VALUE_DEPTH: usize = 8;
const MAX_EXTENSION_VALUE_NODES: usize = 256;
const MAX_EXTENSION_STRING_BYTES: usize = 4_096;

/// Version of the extension manifest contract.
pub const EXTENSION_SCHEMA_VERSION: u32 = 1;

/// Native process sandbox available for an extension invocation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExtensionSandbox {
    LinuxBubblewrap,
    MacSandboxExec,
    Unavailable,
}

impl ExtensionSandbox {
    /// Detect a supported native sandbox without claiming that one exists.
    pub fn detect() -> Self {
        #[cfg(target_os = "linux")]
        {
            if std::process::Command::new("bwrap")
                .arg("--version")
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .status()
                .is_ok_and(|status| status.success())
            {
                return Self::LinuxBubblewrap;
            }
        }
        #[cfg(target_os = "macos")]
        {
            if Path::new("/usr/bin/sandbox-exec").is_file() {
                return Self::MacSandboxExec;
            }
        }
        Self::Unavailable
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::LinuxBubblewrap => "linux-bubblewrap",
            Self::MacSandboxExec => "macos-sandbox-exec",
            Self::Unavailable => "unavailable",
        }
    }
}

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

/// A bounded action request returned by an extension for core dispatch.
///
/// Extensions do not receive browser handles and cannot call CDP. The only
/// way for an extension result to cause a browser mutation is to return this
/// shape to [`ExtensionHost::invoke_guarded`], which requires a current
/// observation revision and routes the operation through `BrowserSession`.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ExtensionGuardedAction {
    pub action: String,
    #[serde(default)]
    pub target: Option<String>,
    #[serde(default)]
    pub value: Option<String>,
    pub expected_revision: u64,
}

impl ExtensionGuardedAction {
    fn validate(&self) -> Result<(), ExtensionError> {
        if self.action.is_empty() || self.action.len() > 32 {
            return Err(ExtensionError(
                "extension guarded action name is out of bounds".into(),
            ));
        }
        if self.expected_revision == 0 {
            return Err(ExtensionError(
                "extension guarded action requires a positive expectedRevision".into(),
            ));
        }
        for (name, value, max) in [
            ("target", self.target.as_deref(), 512),
            ("value", self.value.as_deref(), 4_096),
        ] {
            if let Some(value) = value
                && (value.is_empty() || value.len() > max)
            {
                return Err(ExtensionError(format!(
                    "extension guarded action {name} is out of bounds"
                )));
            }
        }
        Ok(())
    }
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
    invocations: Arc<Semaphore>,
    experimental_enabled: bool,
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
        Ok(Self {
            root,
            registry,
            invocations: Arc::new(Semaphore::new(MAX_EXTENSION_CONCURRENT_INVOCATIONS)),
            experimental_enabled: false,
        })
    }

    /// Opt into the experimental extension capability for this host.
    ///
    /// Invocations still require a detected native sandbox. This opt-in does
    /// not authorize the unsandboxed subprocess helper.
    pub fn with_experimental_extensions(mut self) -> Self {
        self.experimental_enabled = true;
        self
    }

    /// Report the sandbox that an explicit guarded invocation would use.
    pub fn sandbox(&self) -> ExtensionSandbox {
        ExtensionSandbox::detect()
    }

    /// Invoke one declared extension capability through a bounded subprocess.
    #[cfg(test)]
    pub(crate) async fn invoke(
        &self,
        extension_id: &str,
        capability: ExtensionCapability,
        host: &str,
        action: &str,
        payload: Value,
    ) -> Result<Value, ExtensionError> {
        self.invoke_internal(extension_id, capability, host, action, payload, None)
            .await
    }

    /// Invoke one extension only inside the detected native sandbox.
    pub async fn invoke_sandboxed(
        &self,
        extension_id: &str,
        capability: ExtensionCapability,
        host: &str,
        action: &str,
        payload: Value,
    ) -> Result<Value, ExtensionError> {
        if !self.experimental_enabled {
            return Err(ExtensionError(
                "experimental extensions are disabled; opt in explicitly".into(),
            ));
        }
        let sandbox = self.sandbox();
        if sandbox == ExtensionSandbox::Unavailable {
            return Err(ExtensionError(
                "no supported native extension sandbox is available".into(),
            ));
        }
        self.invoke_internal(
            extension_id,
            capability,
            host,
            action,
            payload,
            Some(sandbox),
        )
        .await
    }

    /// Invoke an extension and dispatch its returned action through the core
    /// revision-guarded browser methods.
    ///
    /// The extension may suggest only `click`, `type`, `clear`, `check`,
    /// `uncheck`, or `select`. The returned request must include the current
    /// observation revision. Policy checks, target resolution, verification,
    /// and effect recording remain owned by `BrowserSession`.
    pub async fn invoke_guarded(
        &self,
        session: &crate::browser::BrowserSession,
        extension_id: &str,
        capability: ExtensionCapability,
        host: &str,
        action: &str,
        payload: Value,
    ) -> Result<crate::browser::ActionOutcome, ExtensionError> {
        let result = self
            .invoke_sandboxed(extension_id, capability, host, action, payload)
            .await?;
        let guarded: ExtensionGuardedAction = serde_json::from_value(result).map_err(|error| {
            ExtensionError(format!("invalid guarded extension action: {error}"))
        })?;
        guarded.validate()?;
        let manifest = self
            .registry
            .get(extension_id)
            .ok_or_else(|| ExtensionError("extension is not registered".into()))?;
        if !manifest
            .permissions
            .actions
            .iter()
            .any(|allowed| allowed == &guarded.action)
        {
            return Err(ExtensionError(
                "extension returned an action outside its declared permissions".into(),
            ));
        }
        let target = || {
            guarded
                .target
                .as_deref()
                .ok_or_else(|| ExtensionError("guarded extension action requires a target".into()))
        };
        let revision = guarded.expected_revision;
        match guarded.action.as_str() {
            "click" => session
                .click_with_revision(target()?, revision)
                .await
                .map_err(extension_browser_error),
            "type" => session
                .type_text_with_expected_revision(
                    guarded.value.as_deref().unwrap_or_default(),
                    Some(target()?),
                    Some(revision),
                )
                .await
                .map_err(extension_browser_error),
            "clear" => session
                .clear_with_revision(target()?, Some(revision))
                .await
                .map_err(extension_browser_error),
            "check" => session
                .check_with_revision(target()?, Some(revision))
                .await
                .map_err(extension_browser_error),
            "uncheck" => session
                .uncheck_with_revision(target()?, Some(revision))
                .await
                .map_err(extension_browser_error),
            "select" => session
                .select_option_with_revision(
                    target()?,
                    guarded.value.as_deref().unwrap_or_default(),
                    Some(revision),
                )
                .await
                .map_err(extension_browser_error),
            other => Err(ExtensionError(format!(
                "guarded extension action is not supported: {other}"
            ))),
        }
    }

    async fn invoke_internal(
        &self,
        extension_id: &str,
        capability: ExtensionCapability,
        host: &str,
        action: &str,
        payload: Value,
        sandbox: Option<ExtensionSandbox>,
    ) -> Result<Value, ExtensionError> {
        let _permit = tokio::time::timeout(
            EXTENSION_TIMEOUT,
            Arc::clone(&self.invocations).acquire_owned(),
        )
        .await
        .map_err(|_| ExtensionError("extension invocation queue timed out".into()))?
        .map_err(|_| ExtensionError("extension invocation queue is closed".into()))?;
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
        let mut command = match sandbox {
            None => Command::new(entrypoint),
            Some(ExtensionSandbox::LinuxBubblewrap) => {
                let mut command = Command::new("bwrap");
                command.args([
                    "--die-with-parent",
                    "--new-session",
                    "--unshare-all",
                    "--proc",
                    "/proc",
                    "--dev",
                    "/dev",
                    "--tmpfs",
                    "/tmp",
                    "--ro-bind",
                ]);
                command.arg(&self.root).arg(&self.root);
                for path in ["/bin", "/usr", "/lib", "/lib64"] {
                    if Path::new(path).is_dir() {
                        command.arg("--ro-bind").arg(path).arg(path);
                    }
                }
                command.arg("--chdir").arg(&self.root).arg(entrypoint);
                command
            }
            Some(ExtensionSandbox::MacSandboxExec) => {
                let root = self
                    .root
                    .to_str()
                    .ok_or_else(|| ExtensionError("extension root is not valid UTF-8".into()))?
                    .replace('\\', "\\\\")
                    .replace('"', "\\\"");
                let profile = format!(
                    "(version 1)(deny default)(allow process-exec)(allow file-read* (subpath \"{root}\"))(allow file-read* (subpath \"/bin\"))(allow file-read* (subpath \"/usr/bin\"))(allow file-read* (subpath \"/usr/lib\"))(allow file-write* (subpath \"/tmp\"))"
                );
                let mut command = Command::new("/usr/bin/sandbox-exec");
                command.arg("-p").arg(profile).arg(entrypoint);
                command
            }
            Some(ExtensionSandbox::Unavailable) => {
                return Err(ExtensionError(
                    "no supported native extension sandbox is available".into(),
                ));
            }
        };
        let mut child = command
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
                (true, Some(result), None) => {
                    validate_extension_value(&result)?;
                    Ok(result)
                }
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

fn extension_browser_error(error: impl fmt::Display) -> ExtensionError {
    ExtensionError(format!("guarded extension action failed: {error}"))
}

fn validate_extension_value(value: &Value) -> Result<(), ExtensionError> {
    fn visit(value: &Value, depth: usize, nodes: &mut usize) -> Result<(), ExtensionError> {
        *nodes = nodes.saturating_add(1);
        if *nodes > MAX_EXTENSION_VALUE_NODES {
            return Err(ExtensionError(
                "extension response contains too many values".into(),
            ));
        }
        if depth > MAX_EXTENSION_VALUE_DEPTH {
            return Err(ExtensionError(
                "extension response nesting exceeds the limit".into(),
            ));
        }
        match value {
            Value::String(value) if value.len() > MAX_EXTENSION_STRING_BYTES => Err(
                ExtensionError("extension response string is oversized".into()),
            ),
            Value::Array(values) => {
                if values.len() > MAX_EXTENSION_VALUE_NODES {
                    return Err(ExtensionError(
                        "extension response array is oversized".into(),
                    ));
                }
                for value in values {
                    visit(value, depth + 1, nodes)?;
                }
                Ok(())
            }
            Value::Object(values) => {
                for (key, value) in values {
                    let normalized = key.to_ascii_lowercase();
                    if ["authorization", "cookie", "password", "secret", "token"]
                        .iter()
                        .any(|blocked| normalized.contains(blocked))
                    {
                        return Err(ExtensionError(format!(
                            "extension response contains a sensitive field: {key}"
                        )));
                    }
                    visit(value, depth + 1, nodes)?;
                }
                Ok(())
            }
            _ => Ok(()),
        }
    }

    let mut nodes = 0;
    visit(value, 0, &mut nodes)
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

    #[cfg(unix)]
    #[tokio::test]
    async fn first_party_reference_extensions_load_and_run_through_the_host() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("extensions/first-party");
        let registry = ExtensionRegistry::load_dir(&root).unwrap();
        assert_eq!(registry.manifests().count(), 2);
        let host = ExtensionHost::new(&root, registry).unwrap();

        let title = host
            .invoke(
                "glass.first-party.title-extractor",
                ExtensionCapability::ExtractionTransform,
                "example.com",
                "extract",
                serde_json::json!({"title": "Example Domain"}),
            )
            .await
            .unwrap();
        assert_eq!(title["extension"], "title-extractor");

        let evidence = host
            .invoke(
                "glass.first-party.intent-evidence",
                ExtensionCapability::IntentEvidencePack,
                "example.com",
                "verify",
                serde_json::json!({"role": "button"}),
            )
            .await
            .unwrap();
        assert_eq!(evidence["extension"], "intent-evidence");
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn first_party_extensions_pass_cold_start_exit_and_restart_lifecycle() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("extensions/first-party");
        let registry = ExtensionRegistry::load_dir(&root).unwrap();
        let host = ExtensionHost::new(&root, registry).unwrap();

        for manifest in host.registry.manifests() {
            let capability = manifest.capabilities[0];
            let host_name = &manifest.permissions.hosts[0];
            let action = &manifest.permissions.actions[0];
            let first = host
                .invoke(
                    &manifest.id,
                    capability,
                    host_name,
                    action,
                    serde_json::json!({"lifecycle": "cold-start"}),
                )
                .await
                .unwrap();
            let second = host
                .invoke(
                    &manifest.id,
                    capability,
                    host_name,
                    action,
                    serde_json::json!({"lifecycle": "restart"}),
                )
                .await
                .unwrap();
            assert_eq!(first["extension"], second["extension"]);
        }
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn sandboxed_reference_extensions_pass_native_gate() {
        if std::env::var("GLASS_EXTENSION_SANDBOX_E2E").as_deref() != Ok("1") {
            eprintln!("skipping native extension sandbox gate; set GLASS_EXTENSION_SANDBOX_E2E=1");
            return;
        }
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("extensions/first-party");
        let host = ExtensionHost::new(&root, ExtensionRegistry::load_dir(&root).unwrap())
            .unwrap()
            .with_experimental_extensions();
        assert_ne!(host.sandbox(), ExtensionSandbox::Unavailable);
        let result = host
            .invoke_sandboxed(
                "glass.first-party.title-extractor",
                ExtensionCapability::ExtractionTransform,
                "example.com",
                "extract",
                serde_json::json!({"title": "Example Domain"}),
            )
            .await
            .unwrap();
        assert_eq!(result["extension"], "title-extractor");
    }

    #[tokio::test]
    async fn sandboxed_extensions_require_explicit_experimental_opt_in() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("extensions/first-party");
        let host = ExtensionHost::new(&root, ExtensionRegistry::load_dir(&root).unwrap()).unwrap();
        let error = host
            .invoke_sandboxed(
                "glass.first-party.title-extractor",
                ExtensionCapability::ExtractionTransform,
                "example.com",
                "extract",
                serde_json::json!({"title": "Example Domain"}),
            )
            .await
            .unwrap_err();
        assert!(error.0.contains("experimental extensions are disabled"));
    }

    #[test]
    fn sandbox_detection_is_explicit_and_never_falls_back_silently() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("extensions/first-party");
        let host = ExtensionHost::new(&root, ExtensionRegistry::load_dir(&root).unwrap()).unwrap();
        assert!(!host.sandbox().label().is_empty());
        assert_eq!(
            host.invocations.available_permits(),
            MAX_EXTENSION_CONCURRENT_INVOCATIONS
        );
    }

    #[tokio::test]
    async fn unavailable_sandbox_never_falls_back_to_an_unsandboxed_process() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("extensions/first-party");
        let host = ExtensionHost::new(&root, ExtensionRegistry::load_dir(&root).unwrap()).unwrap();
        let error = host
            .invoke_internal(
                "glass.first-party.title-extractor",
                ExtensionCapability::ExtractionTransform,
                "example.com",
                "extract",
                serde_json::json!({"title": "Example Domain"}),
                Some(ExtensionSandbox::Unavailable),
            )
            .await
            .unwrap_err();
        assert!(error.0.contains("no supported native extension sandbox"));
    }

    #[test]
    fn guarded_extension_actions_require_revision_and_bounded_fields() {
        let action: ExtensionGuardedAction = serde_json::from_value(serde_json::json!({
            "action": "click",
            "target": "ref=r7:b42",
            "expectedRevision": 7
        }))
        .unwrap();
        action.validate().unwrap();

        let mut missing_revision = action.clone();
        missing_revision.expected_revision = 0;
        assert!(missing_revision.validate().is_err());

        let mut oversized = action;
        oversized.target = Some("x".repeat(513));
        assert!(oversized.validate().is_err());
    }

    #[test]
    fn guarded_extension_actions_reject_unknown_fields() {
        assert!(
            serde_json::from_value::<ExtensionGuardedAction>(serde_json::json!({
                "action": "click",
                "target": "ref=r7:b42",
                "expectedRevision": 7,
                "dispatchDirectly": true
            }))
            .is_err()
        );
    }

    #[test]
    fn extension_outputs_fail_closed_on_sensitive_fields_and_unbounded_shapes() {
        assert!(
            validate_extension_value(&serde_json::json!({
                "evidence": {"authorization": "redacted"}
            }))
            .is_err()
        );
        assert!(
            validate_extension_value(&serde_json::json!({
                "text": "x".repeat(MAX_EXTENSION_STRING_BYTES + 1)
            }))
            .is_err()
        );
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
