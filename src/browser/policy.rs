use serde::Serialize;
use std::collections::BTreeSet;
use std::fmt;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
use std::path::{Path, PathBuf};
use url::Url;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, clap::ValueEnum)]
#[serde(rename_all = "snake_case")]
pub enum PolicyCapability {
    Attach,
    PersistentProfile,
    Evaluate,
    Upload,
    Download,
    Screenshot,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, clap::ValueEnum)]
#[serde(rename_all = "snake_case")]
pub enum PolicyPreset {
    Development,
    Hardened,
}

impl std::str::FromStr for PolicyPreset {
    type Err = PolicyError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "development" => Ok(Self::Development),
            "hardened" => Ok(Self::Hardened),
            _ => Err(PolicyError::InvalidConfiguration {
                reason: "policy preset must be development or hardened".to_string(),
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

#[derive(Debug, Clone)]
pub struct BrowserPolicy {
    preset: PolicyPreset,
    allowed_capabilities: BTreeSet<PolicyCapability>,
    confirmed_capabilities: BTreeSet<PolicyCapability>,
    allowed_hosts: BTreeSet<String>,
    denied_hosts: BTreeSet<String>,
    workspace_root: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PolicyError {
    Denied { operation: String, reason: String },
    ConfirmationRequired { operation: String, reason: String },
    InvalidConfiguration { reason: String },
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
    pub fn development(workspace_root: impl AsRef<Path>) -> Result<Self, PolicyError> {
        Self::new(PolicyPreset::Development, workspace_root, [], [])
    }

    pub fn hardened(workspace_root: impl AsRef<Path>) -> Result<Self, PolicyError> {
        Self::new(PolicyPreset::Hardened, workspace_root, [], [])
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
        Ok(Self {
            preset,
            allowed_capabilities: allowed_capabilities.into_iter().collect(),
            confirmed_capabilities: confirmed_capabilities.into_iter().collect(),
            allowed_hosts: BTreeSet::new(),
            denied_hosts: BTreeSet::new(),
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

    pub fn workspace_root(&self) -> &Path {
        &self.workspace_root
    }

    pub fn decide(&self, capability: PolicyCapability) -> PolicyDecision {
        if self.preset == PolicyPreset::Development
            || self.allowed_capabilities.contains(&capability)
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
        let operation = format!("{capability:?}").to_lowercase();
        match self.decide(capability) {
            PolicyDecision::Allow => Ok(()),
            PolicyDecision::Deny { reason } => Err(PolicyError::Denied { operation, reason }),
            PolicyDecision::RequireConfirmation { reason } => {
                Err(PolicyError::ConfirmationRequired { operation, reason })
            }
        }
    }

    pub async fn require_url(&self, value: &str) -> Result<Url, PolicyError> {
        let url = Url::parse(value).map_err(|error| PolicyError::Denied {
            operation: "navigate".to_string(),
            reason: format!("URL is invalid: {error}"),
        })?;
        if self.preset == PolicyPreset::Development {
            return Ok(url);
        }
        if !matches!(url.scheme(), "http" | "https") {
            return Err(url_denied(
                "hardened navigation permits only http and https",
            ));
        }
        if !url.username().is_empty() || url.password().is_some() {
            return Err(url_denied("URLs containing credentials are not permitted"));
        }
        let host = url
            .host_str()
            .ok_or_else(|| url_denied("URL must contain a host"))?;
        if self.denied_hosts.contains(host) {
            return Err(url_denied("host is explicitly denied"));
        }
        if !self.allowed_hosts.is_empty() && !self.allowed_hosts.contains(host) {
            return Err(url_denied("host is not in the explicit allow list"));
        }
        if host.eq_ignore_ascii_case("localhost") || host.ends_with(".localhost") {
            return Err(url_denied("localhost destinations are not permitted"));
        }
        let port = url.port_or_known_default().unwrap_or(0);
        let addresses: Vec<IpAddr> = tokio::net::lookup_host((host, port))
            .await
            .map_err(|error| url_denied(&format!("host resolution failed: {error}")))?
            .map(|address| address.ip())
            .collect();
        if addresses.is_empty() || addresses.iter().copied().any(is_non_public_ip) {
            return Err(url_denied(
                "host resolves to a non-public or reserved network destination",
            ));
        }
        Ok(url)
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
        if self.preset == PolicyPreset::Development || canonical.starts_with(&self.workspace_root) {
            Ok(())
        } else {
            Err(PolicyError::Denied {
                operation: operation.to_string(),
                reason: "path escapes the configured workspace root".to_string(),
            })
        }
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

fn url_denied(reason: &str) -> PolicyError {
    PolicyError::Denied {
        operation: "navigate".to_string(),
        reason: reason.to_string(),
    }
}

fn is_non_public_ip(address: IpAddr) -> bool {
    match address {
        IpAddr::V4(address) => {
            address.is_private()
                || address.is_loopback()
                || address.is_link_local()
                || address.is_multicast()
                || address.is_unspecified()
                || address.is_broadcast()
                || address.is_documentation()
                || address.octets()[0] == 0
                || address.octets()[0] >= 240
                || is_shared_v4(address)
        }
        IpAddr::V6(address) => {
            address.is_loopback()
                || address.is_multicast()
                || address.is_unspecified()
                || is_unique_local_v6(address)
                || is_link_local_v6(address)
                || is_documentation_v6(address)
                || address
                    .to_ipv4_mapped()
                    .is_some_and(|address| is_non_public_ip(IpAddr::V4(address)))
        }
    }
}

fn is_shared_v4(address: Ipv4Addr) -> bool {
    let octets = address.octets();
    octets[0] == 100 && (64..=127).contains(&octets[1])
}

fn is_unique_local_v6(address: Ipv6Addr) -> bool {
    address.segments()[0] & 0xfe00 == 0xfc00
}

fn is_link_local_v6(address: Ipv6Addr) -> bool {
    address.segments()[0] & 0xffc0 == 0xfe80
}

fn is_documentation_v6(address: Ipv6Addr) -> bool {
    address.segments()[..2] == [0x2001, 0x0db8]
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
}
