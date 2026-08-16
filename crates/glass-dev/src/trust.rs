//! Identity-bound workspace trust stored outside repository control.

use crate::development::{DevelopmentError, DevelopmentResult};
use serde::{Deserialize, Serialize};
use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

const TRUST_STORE_VERSION: u32 = 1;
const MAX_TRUST_STORE_BYTES: u64 = 1024 * 1024;

/// Effective authority of the currently open workspace.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum WorkspaceTrust {
    /// Static inspection only; repository-controlled execution is blocked.
    Untrusted,
    /// Executable project configuration is enabled for this process lifetime.
    TrustedOnce,
    /// Identity was matched in the Glass-owned trust store.
    TrustedProject,
}

impl WorkspaceTrust {
    pub fn permits_project_execution(self) -> bool {
        matches!(self, Self::TrustedOnce | Self::TrustedProject)
    }

    /// Human-facing label for TUI surfaces.
    pub fn label(self) -> &'static str {
        match self {
            Self::Untrusted => "untrusted",
            Self::TrustedOnce => "trusted once",
            Self::TrustedProject => "trusted project",
        }
    }
}

/// Explicit local-user decision. This is intentionally absent from MCP/tools.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LocalTrustDecision {
    OpenUntrusted,
    TrustOnce,
    TrustProject,
}

/// Stable evidence used to reject a moved or replaced project.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceIdentity {
    pub canonical_root: PathBuf,
    pub git_remote: Option<String>,
    pub created_unix_nanos: Option<u128>,
    #[cfg(unix)]
    pub device: u64,
    #[cfg(unix)]
    pub inode: u64,
}

impl WorkspaceIdentity {
    pub fn inspect(root: impl AsRef<Path>) -> DevelopmentResult<Self> {
        let canonical_root = std::fs::canonicalize(root)?;
        let metadata = std::fs::metadata(&canonical_root)?;
        let created_unix_nanos = metadata
            .created()
            .ok()
            .and_then(|created| created.duration_since(UNIX_EPOCH).ok())
            .map(|duration| duration.as_nanos());
        #[cfg(unix)]
        let (device, inode) = {
            use std::os::unix::fs::MetadataExt;
            (metadata.dev(), metadata.ino())
        };
        Ok(Self {
            git_remote: read_git_remote(&canonical_root),
            canonical_root,
            created_unix_nanos,
            #[cfg(unix)]
            device,
            #[cfg(unix)]
            inode,
        })
    }

    fn is_persistable(&self) -> bool {
        #[cfg(unix)]
        {
            self.device != 0 && self.inode != 0
        }
        #[cfg(not(unix))]
        {
            self.created_unix_nanos.is_some()
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct TrustRecord {
    identity: WorkspaceIdentity,
    trusted_at_unix_ms: u128,
}

#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct TrustStoreDocument {
    version: u32,
    records: Vec<TrustRecord>,
}

/// Glass-owned persisted trust database.
#[derive(Debug, Clone)]
pub struct WorkspaceTrustStore {
    path: PathBuf,
}

impl WorkspaceTrustStore {
    pub fn platform_default() -> DevelopmentResult<Self> {
        if let Some(path) = std::env::var_os("GLASS_TRUST_STORE_PATH") {
            return Ok(Self::at(path));
        }
        let base = dirs::data_local_dir().ok_or_else(|| {
            DevelopmentError::Config("platform local-data directory is unavailable".into())
        })?;
        Ok(Self::at(base.join("glass/trust/workspaces-v1.json")))
    }

    pub fn at(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn status(&self, identity: &WorkspaceIdentity) -> DevelopmentResult<WorkspaceTrust> {
        let document = self.load()?;
        Ok(
            if document
                .records
                .iter()
                .any(|record| record.identity == *identity)
            {
                WorkspaceTrust::TrustedProject
            } else {
                WorkspaceTrust::Untrusted
            },
        )
    }

    pub fn trust_project(&self, identity: &WorkspaceIdentity) -> DevelopmentResult<()> {
        if !identity.is_persistable() {
            return Err(DevelopmentError::Conflict(
                "project identity cannot be proven safely on this filesystem; use trust-once"
                    .into(),
            ));
        }
        let parent = self.path.parent().ok_or_else(|| {
            DevelopmentError::Config("trust-store path has no parent directory".into())
        })?;
        std::fs::create_dir_all(parent)?;
        let lock_path = self.path.with_extension("lock");
        let lock = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(lock_path)?;
        fs2::FileExt::lock_exclusive(&lock)?;
        let result = (|| {
            let mut document = self.load()?;
            document
                .records
                .retain(|record| record.identity.canonical_root != identity.canonical_root);
            document.records.push(TrustRecord {
                identity: identity.clone(),
                trusted_at_unix_ms: SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_millis(),
            });
            if document.records.len() > 1024 {
                document
                    .records
                    .sort_by_key(|record| record.trusted_at_unix_ms);
                document.records.drain(..document.records.len() - 1024);
            }
            self.save(&document)
        })();
        let _ = fs2::FileExt::unlock(&lock);
        result
    }

    pub fn forget(&self, identity: &WorkspaceIdentity) -> DevelopmentResult<()> {
        let mut document = self.load()?;
        document
            .records
            .retain(|record| record.identity != *identity);
        self.save(&document)
    }

    fn load(&self) -> DevelopmentResult<TrustStoreDocument> {
        if !self.path.exists() {
            return Ok(TrustStoreDocument {
                version: TRUST_STORE_VERSION,
                records: Vec::new(),
            });
        }
        let metadata = std::fs::metadata(&self.path)?;
        if !metadata.is_file() || metadata.len() > MAX_TRUST_STORE_BYTES {
            return Err(DevelopmentError::Config(
                "workspace trust store is not a bounded regular file".into(),
            ));
        }
        let mut bytes = Vec::with_capacity(metadata.len() as usize);
        File::open(&self.path)?.read_to_end(&mut bytes)?;
        let document: TrustStoreDocument = serde_json::from_slice(&bytes)
            .map_err(|error| DevelopmentError::Config(error.to_string()))?;
        if document.version != TRUST_STORE_VERSION {
            return Err(DevelopmentError::Config(format!(
                "unsupported workspace trust-store version {}",
                document.version
            )));
        }
        Ok(document)
    }

    fn save(&self, document: &TrustStoreDocument) -> DevelopmentResult<()> {
        let parent = self.path.parent().ok_or_else(|| {
            DevelopmentError::Config("trust-store path has no parent directory".into())
        })?;
        std::fs::create_dir_all(parent)?;
        let bytes = serde_json::to_vec_pretty(document)?;
        if bytes.len() as u64 > MAX_TRUST_STORE_BYTES {
            return Err(DevelopmentError::Config(
                "workspace trust store exceeds its size limit".into(),
            ));
        }
        let temporary = self
            .path
            .with_extension(format!("tmp-{}", std::process::id()));
        let mut file = OpenOptions::new()
            .create(true)
            .truncate(true)
            .write(true)
            .open(&temporary)?;
        file.write_all(&bytes)?;
        file.sync_all()?;
        std::fs::rename(temporary, &self.path)?;
        Ok(())
    }
}

fn read_git_remote(root: &Path) -> Option<String> {
    let output = std::process::Command::new("git")
        .arg("-C")
        .arg(root)
        .args(["config", "--get", "remote.origin.url"])
        .stdin(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .output()
        .ok()?;
    if !output.status.success() || output.stdout.len() > 4096 {
        return None;
    }
    let remote = String::from_utf8(output.stdout).ok()?;
    let remote = remote.trim();
    (!remote.is_empty()).then(|| remote.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn root(label: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "glass-trust-{label}-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
    }

    #[test]
    fn trusted_project_is_external_and_identity_bound() {
        let root = root("identity");
        let store_path = root.with_extension("store.json");
        std::fs::create_dir_all(&root).unwrap();
        let store = WorkspaceTrustStore::at(&store_path);
        let original = WorkspaceIdentity::inspect(&root).unwrap();
        assert_eq!(store.status(&original).unwrap(), WorkspaceTrust::Untrusted);
        store.trust_project(&original).unwrap();
        assert_eq!(
            store.status(&original).unwrap(),
            WorkspaceTrust::TrustedProject
        );
        assert!(!root.join(".glass-trust").exists());

        std::fs::remove_dir(&root).unwrap();
        std::fs::create_dir(&root).unwrap();
        let replacement = WorkspaceIdentity::inspect(&root).unwrap();
        assert_ne!(replacement, original);
        assert_eq!(
            store.status(&replacement).unwrap(),
            WorkspaceTrust::Untrusted
        );

        std::fs::remove_dir_all(root).unwrap();
        std::fs::remove_file(store_path).unwrap();
    }

    #[test]
    fn trust_once_is_never_written_to_the_store() {
        let root = root("once");
        let store_path = root.with_extension("store.json");
        std::fs::create_dir_all(&root).unwrap();
        let identity = WorkspaceIdentity::inspect(&root).unwrap();
        let store = WorkspaceTrustStore::at(&store_path);
        assert_eq!(WorkspaceTrust::TrustedOnce, WorkspaceTrust::TrustedOnce);
        assert_eq!(store.status(&identity).unwrap(), WorkspaceTrust::Untrusted);
        assert!(!store_path.exists());
        std::fs::remove_dir_all(root).unwrap();
    }
}
