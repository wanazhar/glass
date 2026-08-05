//! Security policy engine for browser operations.
//!
//! Defines a policy system with presets (development, hardened, custom)
//! and capability-based gating. Every session operation is checked against
//! the active policy before execution. Includes network filtering, file
//! system sandboxing, and per-capability allow/deny controls.

use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
use std::path::{Path, PathBuf};
use url::Url;

/// Version of the structured policy decision/error contract.
pub const POLICY_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, clap::ValueEnum)]
#[serde(rename_all = "snake_case")]
pub enum PolicyCapability {
    Attach,
    PersistentProfile,
    Evaluate,
    Upload,
    Download,
    Screenshot,
    RawCdp,
    ReadFormValues,
    ReadSensitiveFormValues,
    ReadSensitiveExtraction,
    CoordinateClick,
    ConsentDismissal,
    DeclaredAgentIdentity,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, clap::ValueEnum)]
#[serde(rename_all = "snake_case")]
pub enum PolicyPreset {
    Development,
    #[clap(name = "ci")]
    Ci,
    Polite,
    Hardened,
    #[clap(name = "untrusted-mcp")]
    UntrustedMcp,
}

impl std::str::FromStr for PolicyPreset {
    type Err = PolicyError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "development" | "dev" => Ok(Self::Development),
            "ci" => Ok(Self::Ci),
            "polite" => Ok(Self::Polite),
            "hardened" => Ok(Self::Hardened),
            "untrusted-mcp" | "untrusted_mcp" => Ok(Self::UntrustedMcp),
            _ => Err(PolicyError::InvalidConfiguration {
                reason: format!(
                    "policy preset must be dev, ci, polite, hardened, or untrusted-mcp, got: {value}"
                ),
            }),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "decision", rename_all = "snake_case")]
pub enum PolicyDecision {
    Allow,
    Deny { reason: String },
    RequireConfirmation { reason: String },
}
/// Browser-free, read-only navigation policy result.
///
/// This deliberately reports only URL policy evidence. It does not resolve
/// hosts, mutate confirmation tokens, or start a browser session.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NavigationPreflight {
    pub schema_version: u32,
    pub normalized_url: Option<String>,
    pub host: Option<String>,
    pub decision: NavigationPreflightDecision,
    pub reason: String,
    pub confirmation_required: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum NavigationPreflightDecision {
    Allow,
    Deny,
    ConfirmationRequired,
}

#[derive(Debug, Clone)]
pub struct BrowserPolicy {
    preset: PolicyPreset,
    allowed_capabilities: BTreeSet<PolicyCapability>,
    confirmed_capabilities: BTreeSet<PolicyCapability>,
    allowed_hosts: BTreeSet<String>,
    denied_hosts: BTreeSet<String>,
    confirmation_tokens: std::sync::Arc<std::sync::Mutex<BTreeMap<PolicyCapability, u32>>>,
    pinned_hosts: BTreeMap<String, IpAddr>,
    workspace_root: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PolicyError {
    Denied { operation: String, reason: String },
    ConfirmationRequired { operation: String, reason: String },
    InvalidConfiguration { reason: String },
}

/// Stable, actionable policy failure metadata for protocol consumers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PolicyErrorContract {
    pub schema_version: u32,
    pub policy_version: u32,
    pub kind: String,
    pub operation: Option<String>,
    pub rule_id: String,
    pub phase: String,
    pub reason: String,
    pub remediation: Option<String>,
    pub override_possible: bool,
}

impl PolicyError {
    /// Convert the internal error into the versioned public contract.
    pub fn contract(&self) -> PolicyErrorContract {
        match self {
            Self::Denied { operation, reason } => PolicyErrorContract {
                schema_version: POLICY_SCHEMA_VERSION,
                policy_version: POLICY_SCHEMA_VERSION,
                kind: "denied".into(),
                operation: Some(operation.clone()),
                rule_id: format!("policy.{operation}.denied"),
                phase: "preflight".into(),
                reason: reason.clone(),
                remediation: Some(
                    "change the policy or use an explicitly allowed capability".into(),
                ),
                override_possible: true,
            },
            Self::ConfirmationRequired { operation, reason } => PolicyErrorContract {
                schema_version: POLICY_SCHEMA_VERSION,
                policy_version: POLICY_SCHEMA_VERSION,
                kind: "confirmation_required".into(),
                operation: Some(operation.clone()),
                rule_id: format!("policy.{operation}.confirmation"),
                phase: "preflight".into(),
                reason: reason.clone(),
                remediation: Some("provide one explicit confirmation token".into()),
                override_possible: true,
            },
            Self::InvalidConfiguration { reason } => PolicyErrorContract {
                schema_version: POLICY_SCHEMA_VERSION,
                policy_version: POLICY_SCHEMA_VERSION,
                kind: "invalid_configuration".into(),
                operation: None,
                rule_id: "policy.configuration.invalid".into(),
                phase: "configuration".into(),
                reason: reason.clone(),
                remediation: Some(
                    "correct the policy configuration before starting a session".into(),
                ),
                override_possible: false,
            },
        }
    }
}

impl fmt::Display for PolicyError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Denied { operation, reason } => {
                write!(formatter, "policy denied {operation}: {reason}")
            }
            Self::ConfirmationRequired { operation, reason } => {
                write!(
                    formatter,
                    "policy confirmation required for {operation}: {reason}"
                )
            }
            Self::InvalidConfiguration { reason } => {
                write!(formatter, "invalid browser policy: {reason}")
            }
        }
    }
}

impl std::error::Error for PolicyError {}

impl BrowserPolicy {
    /// Create a policy from a preset and workspace root.
    pub fn from_preset(
        preset: PolicyPreset,
        workspace_root: impl AsRef<Path>,
    ) -> Result<Self, PolicyError> {
        match preset {
            PolicyPreset::Development => Self::development(workspace_root),
            PolicyPreset::Ci => Self::ci(workspace_root),
            PolicyPreset::Polite => Self::polite(workspace_root),
            PolicyPreset::Hardened => Self::hardened(workspace_root),
            PolicyPreset::UntrustedMcp => Self::untrusted_mcp(workspace_root),
        }
    }

    pub fn development(workspace_root: impl AsRef<Path>) -> Result<Self, PolicyError> {
        Self::new(PolicyPreset::Development, workspace_root, [], [])
    }

    pub fn ci(workspace_root: impl AsRef<Path>) -> Result<Self, PolicyError> {
        Self::new(PolicyPreset::Ci, workspace_root, [], [])
    }

    pub fn polite(workspace_root: impl AsRef<Path>) -> Result<Self, PolicyError> {
        Self::new(PolicyPreset::Polite, workspace_root, [], [])
    }

    pub fn hardened(workspace_root: impl AsRef<Path>) -> Result<Self, PolicyError> {
        Self::new(PolicyPreset::Hardened, workspace_root, [], [])
    }

    pub fn untrusted_mcp(workspace_root: impl AsRef<Path>) -> Result<Self, PolicyError> {
        Self::new(PolicyPreset::UntrustedMcp, workspace_root, [], [])
    }

    pub fn new(
        preset: PolicyPreset,
        workspace_root: impl AsRef<Path>,
        allowed_capabilities: impl IntoIterator<Item = PolicyCapability>,
        confirmed_capabilities: impl IntoIterator<Item = PolicyCapability>,
    ) -> Result<Self, PolicyError> {
        let workspace_root = std::fs::canonicalize(workspace_root.as_ref()).map_err(|error| {
            PolicyError::InvalidConfiguration {
                reason: format!("workspace root must exist and be canonicalizable: {error}"),
            }
        })?;
        if !workspace_root.is_dir() {
            return Err(PolicyError::InvalidConfiguration {
                reason: "workspace root must be a directory".to_string(),
            });
        }
        let allowed_capabilities: BTreeSet<_> = allowed_capabilities.into_iter().collect();
        let confirmed_capabilities: BTreeSet<_> = confirmed_capabilities.into_iter().collect();
        if allowed_capabilities
            .iter()
            .any(|capability| confirmed_capabilities.contains(capability))
        {
            return Err(PolicyError::InvalidConfiguration {
                reason: "a capability cannot be both allowed and confirmation-required".to_string(),
            });
        }
        Ok(Self {
            preset,
            allowed_capabilities,
            confirmed_capabilities,
            allowed_hosts: BTreeSet::new(),
            denied_hosts: BTreeSet::new(),
            confirmation_tokens: Default::default(),
            pinned_hosts: BTreeMap::new(),
            workspace_root,
        })
    }

    pub fn with_host_rules(
        mut self,
        allowed_hosts: impl IntoIterator<Item = String>,
        denied_hosts: impl IntoIterator<Item = String>,
    ) -> Result<Self, PolicyError> {
        self.allowed_hosts = normalize_host_rules(allowed_hosts)?;
        self.denied_hosts = normalize_host_rules(denied_hosts)?;
        if self
            .allowed_hosts
            .iter()
            .any(|host| self.denied_hosts.contains(host))
        {
            return Err(PolicyError::InvalidConfiguration {
                reason: "a host cannot be both allowed and denied".to_string(),
            });
        }
        Ok(self)
    }

    pub fn preset(&self) -> PolicyPreset {
        self.preset
    }

    pub fn with_confirmation_tokens(
        self,
        capabilities: impl IntoIterator<Item = PolicyCapability>,
    ) -> Result<Self, PolicyError> {
        let mut tokens =
            self.confirmation_tokens
                .lock()
                .map_err(|_| PolicyError::InvalidConfiguration {
                    reason: "confirmation token state is unavailable".to_string(),
                })?;
        for capability in capabilities {
            if capability == PolicyCapability::RawCdp {
                return Err(PolicyError::InvalidConfiguration {
                    reason: "raw CDP is an unlimited escape hatch and supports explicit allow only"
                        .to_string(),
                });
            }
            if !self.confirmed_capabilities.contains(&capability) {
                return Err(PolicyError::InvalidConfiguration {
                    reason: format!(
                        "{capability:?} needs --policy-confirm before a one-operation token"
                    ),
                });
            }
            *tokens.entry(capability).or_default() += 1;
        }
        drop(tokens);
        Ok(self)
    }

    pub fn workspace_root(&self) -> &Path {
        &self.workspace_root
    }

    pub async fn prepare_hardened_session(
        &mut self,
        attached: bool,
    ) -> Result<Option<String>, PolicyError> {
        // Development and CI do not require host allowlisting
        if matches!(
            self.preset,
            PolicyPreset::Development | PolicyPreset::Ci | PolicyPreset::Polite
        ) {
            return Ok(None);
        }
        if self.allowed_hosts.is_empty() {
            return Err(PolicyError::InvalidConfiguration {
                reason: "hardened and untrusted-mcp modes require at least one exact --policy-allow-host"
                    .to_string(),
            });
        }
        let mut resolver_rules = Vec::with_capacity(self.allowed_hosts.len());
        for host in &self.allowed_hosts {
            let address = if let Ok(address) = host.parse::<IpAddr>() {
                address
            } else {
                if attached {
                    return Err(PolicyError::InvalidConfiguration {
                        reason: "hardened attach requires public IP-literal allow rules to avoid DNS rebinding"
                            .to_string(),
                    });
                }
                let addresses: Vec<IpAddr> = tokio::net::lookup_host((host.as_str(), 443))
                    .await
                    .map_err(|error| PolicyError::InvalidConfiguration {
                        reason: format!("could not resolve allowed host {host}: {error}"),
                    })?
                    .map(|address| address.ip())
                    .collect();
                if addresses.is_empty() || addresses.iter().copied().any(is_non_public_ip) {
                    return Err(PolicyError::InvalidConfiguration {
                        reason: format!(
                            "allowed host {host} did not resolve only to public addresses"
                        ),
                    });
                }
                addresses
                    .iter()
                    .copied()
                    .find(IpAddr::is_ipv4)
                    .ok_or_else(|| PolicyError::InvalidConfiguration {
                        reason: format!(
                            "allowed host {host} needs a public IPv4 address for resolver pinning"
                        ),
                    })?
            };
            if is_non_public_ip(address) {
                return Err(PolicyError::InvalidConfiguration {
                    reason: format!("allowed host {host} is not a public address"),
                });
            }
            self.pinned_hosts.insert(host.clone(), address);
            if !attached {
                resolver_rules.push(format!("MAP {host} {address}"));
            }
        }
        Ok((!resolver_rules.is_empty()).then(|| resolver_rules.join(",")))
    }

    pub fn decide(&self, capability: PolicyCapability) -> PolicyDecision {
        // Untrusted MCP: everything requires confirmation (or is denied)
        if self.preset == PolicyPreset::UntrustedMcp {
            if self.allowed_capabilities.contains(&capability) {
                return PolicyDecision::Allow;
            }
            // Always-deny capabilities in untrusted MCP
            if matches!(
                capability,
                PolicyCapability::RawCdp | PolicyCapability::PersistentProfile
            ) {
                return PolicyDecision::Deny {
                    reason: format!("{capability:?} is disabled in untrusted-mcp mode"),
                };
            }
            if self.confirmed_capabilities.contains(&capability) {
                return PolicyDecision::RequireConfirmation {
                    reason: format!("{capability:?} requires confirmation in untrusted-mcp mode"),
                };
            }
            return PolicyDecision::Deny {
                reason: format!("{capability:?} is disabled by the untrusted-mcp preset"),
            };
        }

        // CI: allow most capabilities but deny raw CDP and persistent profiles
        if self.preset == PolicyPreset::Ci {
            if matches!(
                capability,
                PolicyCapability::RawCdp | PolicyCapability::PersistentProfile
            ) {
                return PolicyDecision::Deny {
                    reason: format!("{capability:?} is disabled in CI mode"),
                };
            }
            if self.allowed_capabilities.contains(&capability) {
                return PolicyDecision::Allow;
            }
            return PolicyDecision::Allow;
        }

        // Development: allow everything
        if matches!(
            self.preset,
            PolicyPreset::Development | PolicyPreset::Polite
        ) || self.allowed_capabilities.contains(&capability)
        {
            PolicyDecision::Allow
        } else if self.confirmed_capabilities.contains(&capability) {
            PolicyDecision::RequireConfirmation {
                reason: format!("{capability:?} is privileged in hardened mode"),
            }
        } else {
            PolicyDecision::Deny {
                reason: format!("{capability:?} is disabled by the hardened preset"),
            }
        }
    }

    pub fn require(&self, capability: PolicyCapability) -> Result<(), PolicyError> {
        match self.decide(capability) {
            PolicyDecision::Allow => Ok(()),
            PolicyDecision::Deny { reason } => Err(PolicyError::Denied {
                operation: capability_name(capability).to_string(),
                reason,
            }),
            PolicyDecision::RequireConfirmation { reason } => {
                let mut tokens = self.confirmation_tokens.lock().map_err(|_| {
                    PolicyError::InvalidConfiguration {
                        reason: "confirmation token state is unavailable".to_string(),
                    }
                })?;
                let remaining = tokens.entry(capability).or_default();
                if *remaining > 0 {
                    *remaining -= 1;
                    Ok(())
                } else {
                    Err(PolicyError::ConfirmationRequired {
                        operation: capability_name(capability).to_string(),
                        reason,
                    })
                }
            }
        }
    }

    /// Check a capability without consuming a confirmation token. Batch
    /// preflight uses this so a token cannot be spent before the batch starts
    /// or be smuggled into a multi-step request.
    pub fn require_for_batch(&self, capability: PolicyCapability) -> Result<(), PolicyError> {
        match self.decide(capability) {
            PolicyDecision::Allow => Ok(()),
            PolicyDecision::Deny { reason } => Err(PolicyError::Denied {
                operation: capability_name(capability).to_string(),
                reason,
            }),
            PolicyDecision::RequireConfirmation { reason } => {
                Err(PolicyError::ConfirmationRequired {
                    operation: capability_name(capability).to_string(),
                    reason,
                })
            }
        }
    }

    /// Whether sensitive form values (passwords, CC numbers) may be read.
    /// This capability is denied in ALL presets by default and must be
    /// explicitly added to `allowed_capabilities`.
    pub fn allow_sensitive_form_values(&self) -> bool {
        self.allowed_capabilities
            .contains(&PolicyCapability::ReadSensitiveFormValues)
    }

    /// Whether secret-like structured extraction fields may be read.
    /// This capability is denied by default and must be explicitly allowed.
    pub fn allow_sensitive_extraction(&self) -> bool {
        self.allowed_capabilities
            .contains(&PolicyCapability::ReadSensitiveExtraction)
    }

    /// Require the dedicated sensitive structured-extraction capability.
    pub fn require_sensitive_extraction(&self) -> Result<(), PolicyError> {
        if self.allow_sensitive_extraction() {
            Ok(())
        } else {
            Err(PolicyError::Denied {
                operation: capability_name(PolicyCapability::ReadSensitiveExtraction).to_string(),
                reason: "sensitive structured extraction requires explicit capability access"
                    .to_string(),
            })
        }
    }

    pub async fn require_url(&self, value: &str) -> Result<Url, PolicyError> {
        let url = Url::parse(value).map_err(|error| PolicyError::Denied {
            operation: "navigate".to_string(),
            reason: format!("URL is invalid: {error}"),
        })?;
        self.check_url(&url)?;
        Ok(url)
    }

    /// Evaluate a navigation URL without starting a browser or consuming a
    /// confirmation token. This intentionally performs no DNS lookup; hardened
    /// policies report an unpinned host instead of resolving it here.
    pub fn preflight_navigation(&self, value: &str) -> NavigationPreflight {
        let normalized = normalize_navigation_url(value);
        let parsed = Url::parse(&normalized);
        let (safe_url, host) = match parsed {
            Ok(url) => {
                let host = url
                    .host_str()
                    .map(|host| host.trim_end_matches('.').to_string());
                (Some(redacted_url(&url)), host)
            }
            Err(error) => {
                return NavigationPreflight {
                    schema_version: POLICY_SCHEMA_VERSION,
                    normalized_url: None,
                    host: None,
                    decision: NavigationPreflightDecision::Deny,
                    reason: format!("URL is invalid: {error}"),
                    confirmation_required: false,
                };
            }
        };
        let url = Url::parse(&normalized).expect("navigation URL parsed above");
        match self.check_url(&url) {
            Ok(()) => NavigationPreflight {
                schema_version: POLICY_SCHEMA_VERSION,
                normalized_url: safe_url,
                host,
                decision: NavigationPreflightDecision::Allow,
                reason: "URL passes the active navigation policy".to_string(),
                confirmation_required: false,
            },
            Err(PolicyError::Denied { reason, .. }) => NavigationPreflight {
                schema_version: POLICY_SCHEMA_VERSION,
                normalized_url: safe_url,
                host,
                decision: NavigationPreflightDecision::Deny,
                reason,
                confirmation_required: false,
            },
            Err(PolicyError::ConfirmationRequired { reason, .. }) => NavigationPreflight {
                schema_version: POLICY_SCHEMA_VERSION,
                normalized_url: safe_url,
                host,
                decision: NavigationPreflightDecision::ConfirmationRequired,
                reason,
                confirmation_required: true,
            },
            Err(PolicyError::InvalidConfiguration { reason }) => NavigationPreflight {
                schema_version: POLICY_SCHEMA_VERSION,
                normalized_url: safe_url,
                host,
                decision: NavigationPreflightDecision::Deny,
                reason,
                confirmation_required: false,
            },
        }
    }

    fn check_url(&self, url: &Url) -> Result<(), PolicyError> {
        let host = url.host_str().map(|host| host.trim_end_matches('.'));
        if host.is_some_and(|host| self.denied_hosts.contains(host)) {
            return Err(url_denied("host is explicitly denied"));
        }
        if !self.allowed_hosts.is_empty()
            && !host.is_some_and(|host| self.allowed_hosts.contains(host))
        {
            return Err(url_denied("host is not in the explicit allow list"));
        }
        if matches!(
            self.preset,
            PolicyPreset::Development | PolicyPreset::Ci | PolicyPreset::Polite
        ) {
            return Ok(());
        }
        if url.host_str().is_some_and(|host| host.ends_with('.')) {
            return Err(url_denied(
                "hardened URLs must use a canonical host without a trailing dot",
            ));
        }
        if !matches!(url.scheme(), "http" | "https") {
            return Err(url_denied(
                "hardened navigation permits only http and https",
            ));
        }
        if !url.username().is_empty() || url.password().is_some() {
            return Err(url_denied("URLs containing credentials are not permitted"));
        }
        let host = host.ok_or_else(|| url_denied("URL must contain a host"))?;
        if host.eq_ignore_ascii_case("localhost") || host.ends_with(".localhost") {
            return Err(url_denied("localhost destinations are not permitted"));
        }
        let Some(address) = self.pinned_hosts.get(host).copied() else {
            return Err(url_denied(
                "hardened host was not pinned at session startup",
            ));
        };
        if is_non_public_ip(address) {
            return Err(url_denied(
                "host resolves to a non-public or reserved network destination",
            ));
        }
        Ok(())
    }

    pub fn require_existing_path(&self, value: &Path) -> Result<PathBuf, PolicyError> {
        let canonical = std::fs::canonicalize(value).map_err(|error| PolicyError::Denied {
            operation: "filesystem_read".to_string(),
            reason: format!("path must exist and be canonicalizable: {error}"),
        })?;
        self.require_within_workspace(&canonical, "filesystem_read")?;
        Ok(canonical)
    }

    pub fn require_output_path(&self, value: &Path) -> Result<PathBuf, PolicyError> {
        if std::fs::symlink_metadata(value).is_ok() {
            let canonical = std::fs::canonicalize(value).map_err(|error| PolicyError::Denied {
                operation: "filesystem_write".to_string(),
                reason: format!("existing output must be canonicalizable: {error}"),
            })?;
            self.require_within_workspace(&canonical, "filesystem_write")?;
            return Ok(canonical);
        }
        let name = value.file_name().ok_or_else(|| PolicyError::Denied {
            operation: "filesystem_write".to_string(),
            reason: "output path must name a file or directory".to_string(),
        })?;
        let parent = value.parent().unwrap_or_else(|| Path::new("."));
        let parent = std::fs::canonicalize(parent).map_err(|error| PolicyError::Denied {
            operation: "filesystem_write".to_string(),
            reason: format!("output parent must exist and be canonicalizable: {error}"),
        })?;
        self.require_within_workspace(&parent, "filesystem_write")?;
        Ok(parent.join(name))
    }

    fn require_within_workspace(
        &self,
        canonical: &Path,
        operation: &str,
    ) -> Result<(), PolicyError> {
        if matches!(
            self.preset,
            PolicyPreset::Development | PolicyPreset::Polite
        ) || canonical.starts_with(&self.workspace_root)
        {
            Ok(())
        } else {
            Err(PolicyError::Denied {
                operation: operation.to_string(),
                reason: "path escapes the configured workspace root".to_string(),
            })
        }
    }

    pub fn is_polite(&self) -> bool {
        self.preset == PolicyPreset::Polite
    }
}

fn capability_name(capability: PolicyCapability) -> &'static str {
    match capability {
        PolicyCapability::Attach => "attach",
        PolicyCapability::PersistentProfile => "persistent_profile",
        PolicyCapability::Evaluate => "evaluate",
        PolicyCapability::Upload => "upload",
        PolicyCapability::Download => "download",
        PolicyCapability::Screenshot => "screenshot",
        PolicyCapability::RawCdp => "raw_cdp",
        PolicyCapability::ReadFormValues => "read_form_values",
        PolicyCapability::ReadSensitiveFormValues => "read_sensitive_form_values",
        PolicyCapability::ReadSensitiveExtraction => "read_sensitive_extraction",
        PolicyCapability::CoordinateClick => "coordinate_click",
        PolicyCapability::ConsentDismissal => "consent_dismissal",
        PolicyCapability::DeclaredAgentIdentity => "declared_agent_identity",
    }
}

fn normalize_host_rules(
    hosts: impl IntoIterator<Item = String>,
) -> Result<BTreeSet<String>, PolicyError> {
    hosts
        .into_iter()
        .map(|host| {
            let host = host.trim().trim_end_matches('.').to_ascii_lowercase();
            if host.is_empty()
                || host.contains('/')
                || host.contains(':')
                || host.contains('*')
                || Url::parse(&format!("https://{host}/"))
                    .ok()
                    .and_then(|url| url.host_str().map(str::to_string))
                    .as_deref()
                    != Some(host.as_str())
            {
                return Err(PolicyError::InvalidConfiguration {
                    reason: format!("invalid exact host rule: {host}"),
                });
            }
            Ok(host)
        })
        .collect()
}
fn normalize_navigation_url(value: &str) -> String {
    let value = value.trim();
    if value.starts_with("http://")
        || value.starts_with("https://")
        || value.starts_with("about:")
        || value.starts_with("file:")
        || value.starts_with("data:")
    {
        value.to_string()
    } else {
        format!("https://{value}")
    }
}

fn redacted_url(url: &Url) -> String {
    let mut safe = url.clone();
    if !safe.username().is_empty() {
        let _ = safe.set_username("");
    }
    if safe.password().is_some() {
        let _ = safe.set_password(None);
    }
    safe.to_string()
}

fn url_denied(reason: &str) -> PolicyError {
    PolicyError::Denied {
        operation: "navigate".to_string(),
        reason: reason.to_string(),
    }
}

fn is_non_public_ip(address: IpAddr) -> bool {
    match address {
        IpAddr::V4(address) => {
            let value = u32::from(address);
            [
                ("0.0.0.0", 8),
                ("10.0.0.0", 8),
                ("100.64.0.0", 10),
                ("127.0.0.0", 8),
                ("169.254.0.0", 16),
                ("172.16.0.0", 12),
                ("192.0.0.0", 24),
                ("192.0.2.0", 24),
                ("192.88.99.0", 24),
                ("192.168.0.0", 16),
                ("198.18.0.0", 15),
                ("198.51.100.0", 24),
                ("203.0.113.0", 24),
                ("224.0.0.0", 4),
                ("240.0.0.0", 4),
            ]
            .into_iter()
            .any(|(network, prefix)| {
                ipv4_in_prefix(
                    value,
                    u32::from(network.parse::<Ipv4Addr>().unwrap()),
                    prefix,
                )
            })
        }
        IpAddr::V6(address) => {
            address.is_loopback()
                || address.is_multicast()
                || address.is_unspecified()
                || [
                    ("64:ff9b::", 96),
                    ("64:ff9b:1::", 48),
                    ("100::", 64),
                    ("2001::", 32),
                    ("2001:db8::", 32),
                    ("2002::", 16),
                    ("fc00::", 7),
                    ("fe80::", 10),
                    ("fec0::", 10),
                ]
                .into_iter()
                .any(|(network, prefix)| {
                    ipv6_in_prefix(
                        u128::from(address),
                        u128::from(network.parse::<Ipv6Addr>().unwrap()),
                        prefix,
                    )
                })
                || address
                    .to_ipv4_mapped()
                    .is_some_and(|address| is_non_public_ip(IpAddr::V4(address)))
        }
    }
}

fn ipv4_in_prefix(value: u32, network: u32, prefix: u32) -> bool {
    let mask = u32::MAX.checked_shl(32 - prefix).unwrap_or(0);
    value & mask == network & mask
}

fn ipv6_in_prefix(value: u128, network: u128, prefix: u32) -> bool {
    let mask = u128::MAX.checked_shl(128 - prefix).unwrap_or(0);
    value & mask == network & mask
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hardened_capabilities_are_typed_denials_or_confirmations() {
        let root = std::env::current_dir().unwrap();
        let denied = BrowserPolicy::hardened(&root).unwrap();
        assert!(matches!(
            denied.decide(PolicyCapability::Evaluate),
            PolicyDecision::Deny { .. }
        ));
        let confirm = BrowserPolicy::new(
            PolicyPreset::Hardened,
            &root,
            [],
            [PolicyCapability::Evaluate],
        )
        .unwrap();
        assert!(matches!(
            confirm.require(PolicyCapability::Evaluate),
            Err(PolicyError::ConfirmationRequired { .. })
        ));
        let approved = confirm
            .with_confirmation_tokens([PolicyCapability::Evaluate])
            .unwrap();
        assert!(approved.require(PolicyCapability::Evaluate).is_ok());
        assert!(matches!(
            approved.require(PolicyCapability::Evaluate),
            Err(PolicyError::ConfirmationRequired { .. })
        ));
        assert!(
            BrowserPolicy::new(
                PolicyPreset::Hardened,
                &root,
                [PolicyCapability::Evaluate],
                [PolicyCapability::Evaluate],
            )
            .is_err()
        );
    }

    #[test]
    fn sensitive_extraction_requires_its_dedicated_capability() {
        let root = std::env::current_dir().unwrap();
        let default = BrowserPolicy::development(&root).unwrap();
        assert!(!default.allow_sensitive_extraction());
        let denied = default.require_sensitive_extraction().unwrap_err();
        assert_eq!(
            denied.contract().operation.as_deref(),
            Some("read_sensitive_extraction")
        );

        let form_only = BrowserPolicy::new(
            PolicyPreset::Development,
            &root,
            [PolicyCapability::ReadSensitiveFormValues],
            [],
        )
        .unwrap();
        assert!(!form_only.allow_sensitive_extraction());

        let extraction = BrowserPolicy::new(
            PolicyPreset::Development,
            &root,
            [PolicyCapability::ReadSensitiveExtraction],
            [],
        )
        .unwrap();
        assert!(extraction.allow_sensitive_extraction());
        assert!(extraction.require_sensitive_extraction().is_ok());
    }

    #[tokio::test]
    async fn hardened_urls_reject_alternate_private_and_local_forms() {
        let policy = BrowserPolicy::hardened(std::env::current_dir().unwrap()).unwrap();
        for value in [
            "file:///etc/passwd",
            "data:text/plain,secret",
            "http://localhost/",
            "http://127.1/",
            "http://2130706433/",
            "http://[::1]/",
            "http://[::ffff:127.0.0.1]/",
            "https://user:secret@example.com/",
        ] {
            assert!(policy.require_url(value).await.is_err(), "accepted {value}");
        }
        for value in [
            "192.0.0.8",
            "198.18.0.1",
            "198.51.100.1",
            "203.0.113.1",
            "64:ff9b::1",
            "2001::1",
            "2002::1",
            "fec0::1",
        ] {
            assert!(is_non_public_ip(value.parse().unwrap()), "accepted {value}");
        }
    }

    #[tokio::test]
    async fn explicit_host_rules_are_canonical_and_pinned() {
        let root = std::env::current_dir().unwrap();
        let denied = BrowserPolicy::development(&root)
            .unwrap()
            .with_host_rules([], ["example.com".to_string()])
            .unwrap();
        assert!(denied.require_url("https://example.com./").await.is_err());

        let mut pinned = BrowserPolicy::hardened(&root)
            .unwrap()
            .with_host_rules(["8.8.8.8".to_string()], [])
            .unwrap();
        let rules = pinned.prepare_hardened_session(false).await.unwrap();
        assert_eq!(rules.as_deref(), Some("MAP 8.8.8.8 8.8.8.8"));
        assert!(pinned.require_url("https://8.8.8.8/").await.is_ok());
    }

    #[tokio::test]
    async fn ci_urls_do_not_require_hardened_host_pinning() {
        let policy = BrowserPolicy::ci(std::env::current_dir().unwrap()).unwrap();
        assert!(policy.require_url("https://example.com/").await.is_ok());
    }

    #[test]
    fn navigation_preflight_is_read_only_and_redacts_credentials() {
        let root = std::env::current_dir().unwrap();
        let policy = BrowserPolicy::development(&root).unwrap();
        let allowed = policy.preflight_navigation(" example.com/path ");
        assert_eq!(allowed.decision, NavigationPreflightDecision::Allow);
        assert_eq!(
            allowed.normalized_url.as_deref(),
            Some("https://example.com/path")
        );
        assert_eq!(allowed.host.as_deref(), Some("example.com"));
        assert!(!allowed.confirmation_required);

        let denied = policy
            .with_host_rules([], ["example.com".to_string()])
            .unwrap()
            .preflight_navigation("https://user:secret@example.com/");
        assert_eq!(denied.decision, NavigationPreflightDecision::Deny);
        assert_eq!(denied.host.as_deref(), Some("example.com"));
        assert!(
            !denied
                .normalized_url
                .as_deref()
                .unwrap_or_default()
                .contains("secret")
        );
    }

    #[test]
    fn navigation_preflight_reports_malformed_urls_without_dns_or_session() {
        let root = std::env::current_dir().unwrap();
        let policy = BrowserPolicy::development(&root).unwrap();
        let result = policy.preflight_navigation("http://[");
        assert_eq!(result.decision, NavigationPreflightDecision::Deny);
        assert!(result.normalized_url.is_none());
        assert!(result.host.is_none());
        assert!(result.reason.contains("URL is invalid"));

        let hardened = BrowserPolicy::hardened(std::env::current_dir().unwrap()).unwrap();
        let result = hardened.preflight_navigation("https://example.com/");

        let confirmed = BrowserPolicy::new(
            PolicyPreset::Hardened,
            &root,
            [],
            [PolicyCapability::Evaluate],
        )
        .unwrap()
        .with_confirmation_tokens([PolicyCapability::Evaluate])
        .unwrap();
        let _ = confirmed.preflight_navigation("https://example.com/");
        assert!(confirmed.require(PolicyCapability::Evaluate).is_ok());
        assert_eq!(result.decision, NavigationPreflightDecision::Deny);
        assert!(result.reason.contains("not pinned"));
    }

    #[test]
    fn canonical_paths_reject_symlink_escape() {
        #[cfg(unix)]
        {
            use std::os::unix::fs::symlink;
            let root = std::env::temp_dir().join(format!("glass-policy-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&root);
            std::fs::create_dir_all(&root).unwrap();
            symlink("/etc", root.join("escape")).unwrap();
            let policy = BrowserPolicy::hardened(&root).unwrap();
            assert!(
                policy
                    .require_existing_path(&root.join("escape/passwd"))
                    .is_err()
            );
            assert!(
                policy
                    .require_output_path(&root.join("escape/passwd"))
                    .is_err()
            );
            let _ = std::fs::remove_dir_all(root);
        }
    }

    #[test]
    fn policy_errors_expose_versioned_remediation_contract() {
        let error = PolicyError::ConfirmationRequired {
            operation: "evaluate".into(),
            reason: "requires explicit approval".into(),
        };
        let value = serde_json::to_value(error.contract()).unwrap();
        assert_eq!(value["schemaVersion"], 1);
        assert_eq!(value["policyVersion"], 1);
        assert_eq!(value["ruleId"], "policy.evaluate.confirmation");
        assert_eq!(value["phase"], "preflight");
        assert_eq!(value["overridePossible"], true);
        assert!(value["remediation"].as_str().is_some());
    }
}
