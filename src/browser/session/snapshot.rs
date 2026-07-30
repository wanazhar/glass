//! Versioned, redacted session snapshots for deterministic local inspection.

use super::*;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

pub const SESSION_SNAPSHOT_SCHEMA_VERSION: u32 = 1;
const MAX_SNAPSHOT_BYTES: usize = 128 * 1024;
pub fn default_session_snapshot_path(profile: &str) -> PathBuf {
    std::env::var_os("GLASS_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(dirs::config_dir)
        .unwrap_or_else(|| PathBuf::from("."))
        .join("glass")
        .join("snapshots")
        .join(profile)
}
const MAX_SNAPSHOTS: usize = 64;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SessionSnapshot {
    pub schema_version: u32,
    pub snapshot_id: String,
    pub created_at: u64,
    pub profile: String,
    pub observation: SemanticObservation,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionSnapshotDiff {
    pub from_snapshot: String,
    pub to_snapshot: String,
    pub same_route: bool,
    pub revision_delta: u64,
    pub changed_regions: usize,
    pub changed_targets: usize,
}

#[derive(Debug, Clone)]
pub struct SessionSnapshotStore {
    root: PathBuf,
}

impl SessionSnapshot {
    pub fn from_observation(profile: impl Into<String>, observation: SemanticObservation) -> Self {
        let mut observation = observation;
        redact_observation(&mut observation);
        let created_at = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let bytes = serde_json::to_vec(&observation).unwrap_or_default();
        let digest = Sha256::digest(&bytes);
        let short = digest[..6]
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        Self {
            schema_version: SESSION_SNAPSHOT_SCHEMA_VERSION,
            snapshot_id: format!("snap-{}-{short}", observation.revision),
            created_at,
            profile: profile.into(),
            observation,
        }
    }

    pub fn validate(&self) -> BrowserResult<()> {
        if self.schema_version != SESSION_SNAPSHOT_SCHEMA_VERSION {
            return Err(format!(
                "unsupported session snapshot schema: {}",
                self.schema_version
            )
            .into());
        }
        if self.snapshot_id.is_empty() || self.snapshot_id.len() > 128 {
            return Err("snapshot ID must be 1..=128 bytes".into());
        }
        if self.profile.is_empty() || self.profile.len() > 128 {
            return Err("snapshot profile must be 1..=128 bytes".into());
        }
        let bytes = serde_json::to_vec(self)?;
        if bytes.len() > MAX_SNAPSHOT_BYTES {
            return Err(format!("session snapshot exceeds {MAX_SNAPSHOT_BYTES} bytes").into());
        }
        Ok(())
    }
}

impl SessionSnapshotStore {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    pub fn save(&self, snapshot: &SessionSnapshot) -> BrowserResult<()> {
        snapshot.validate()?;
        std::fs::create_dir_all(&self.root)?;
        let path = self.path_for(&snapshot.snapshot_id)?;
        let temp = path.with_extension("tmp");
        std::fs::write(&temp, serde_json::to_vec_pretty(snapshot)?)?;
        std::fs::rename(temp, path)?;
        let mut files = self.files()?;
        if files.len() > MAX_SNAPSHOTS {
            files.sort_by_key(|path| std::fs::metadata(path).and_then(|m| m.modified()).ok());
            let remove_count = files.len() - MAX_SNAPSHOTS;
            for path in files.into_iter().take(remove_count) {
                let _ = std::fs::remove_file(path);
            }
        }
        Ok(())
    }

    pub fn load(&self, snapshot_id: &str) -> BrowserResult<SessionSnapshot> {
        let path = self.path_for(snapshot_id)?;
        let snapshot: SessionSnapshot = serde_json::from_slice(&std::fs::read(path)?)?;
        snapshot.validate()?;
        Ok(snapshot)
    }

    pub fn list(&self) -> BrowserResult<Vec<SessionSnapshot>> {
        let mut snapshots = Vec::new();
        for path in self.files()? {
            if let Ok(snapshot) = serde_json::from_slice::<SessionSnapshot>(&std::fs::read(path)?)
                && snapshot.validate().is_ok()
            {
                snapshots.push(snapshot);
            }
        }
        snapshots.sort_by_key(|snapshot| (snapshot.created_at, snapshot.snapshot_id.clone()));
        Ok(snapshots)
    }

    pub fn diff(&self, from: &str, to: &str) -> BrowserResult<SessionSnapshotDiff> {
        let from_snapshot = self.load(from)?;
        let to_snapshot = self.load(to)?;
        let same_route = from_snapshot.observation.route == to_snapshot.observation.route;
        let changed_regions = from_snapshot
            .observation
            .regions
            .iter()
            .zip(to_snapshot.observation.regions.iter())
            .filter(|(left, right)| left != right)
            .count()
            + from_snapshot
                .observation
                .regions
                .len()
                .abs_diff(to_snapshot.observation.regions.len());
        let changed_targets = from_snapshot
            .observation
            .regions
            .iter()
            .flat_map(|region| region.targets.iter())
            .zip(
                to_snapshot
                    .observation
                    .regions
                    .iter()
                    .flat_map(|region| region.targets.iter()),
            )
            .filter(|(left, right)| left != right)
            .count();
        Ok(SessionSnapshotDiff {
            from_snapshot: from.into(),
            to_snapshot: to.into(),
            same_route,
            revision_delta: to_snapshot
                .observation
                .revision
                .saturating_sub(from_snapshot.observation.revision),
            changed_regions,
            changed_targets,
        })
    }

    pub fn purge(&self) -> BrowserResult<usize> {
        let mut removed = 0;
        for path in self.files()? {
            if std::fs::remove_file(path).is_ok() {
                removed += 1;
            }
        }
        Ok(removed)
    }

    fn files(&self) -> BrowserResult<Vec<PathBuf>> {
        if !self.root.is_dir() {
            return Ok(Vec::new());
        }
        Ok(std::fs::read_dir(&self.root)?
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .filter(|path| path.extension().is_some_and(|ext| ext == "json"))
            .collect())
    }

    fn path_for(&self, snapshot_id: &str) -> BrowserResult<PathBuf> {
        if snapshot_id.is_empty()
            || snapshot_id.len() > 128
            || !snapshot_id
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
        {
            return Err("snapshot ID contains invalid characters".into());
        }
        Ok(self.root.join(format!("{snapshot_id}.json")))
    }
}

fn redact_observation(observation: &mut SemanticObservation) {
    observation.text = None;
    observation.accessibility = None;
    observation.raw_accessibility = None;
    observation.changes = None;
    observation.page.url = redacted_url(&observation.page.url);
    observation.page.title = redact_text(&observation.page.title);
    observation.route.url = observation.page.url.clone();
    for region in &mut observation.regions {
        region.label = redact_text(&region.label);
        region.evidence = region
            .evidence
            .iter()
            .map(|value| redact_text(value))
            .collect();
        for target in &mut region.targets {
            target.name = redact_text(&target.name);
        }
        if let Some(expansion) = &mut region.expansion {
            expansion.route.url = observation.route.url.clone();
        }
    }
}

fn redacted_url(url: &str) -> String {
    url.split(['?', '#']).next().unwrap_or(url).to_string()
}

fn redact_text(value: &str) -> String {
    let lower = value.to_ascii_lowercase();
    if [
        "password",
        "passwd",
        "token",
        "secret",
        "cookie",
        "authorization",
    ]
    .iter()
    .any(|term| lower.contains(term))
    {
        "[REDACTED]".into()
    } else {
        value.chars().take(512).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snapshot_redacts_urls_and_sensitive_labels() {
        let observation: SemanticObservation = serde_json::from_value(serde_json::json!({
            "schemaVersion": 1, "revision": 2, "level": "structured",
            "route": {"targetId":"t", "frameId":"f", "url":"https://x.test/?token=secret"},
            "page": {"kind":"form", "title":"Password reset", "url":"https://x.test/?token=secret", "targetId":"t", "frameId":"f", "confidence":"high"},
            "regions": [{"id":"main", "kind":"form", "label":"password form", "interactiveCount":1, "confidence":"high", "targets":[{"reference":"r2:b1", "role":"textbox", "name":"password"}]}],
            "limits": {"truncated":false,"omittedRegions":0}
        })).unwrap();
        let snapshot = SessionSnapshot::from_observation("default", observation);
        snapshot.validate().unwrap();
        assert_eq!(snapshot.observation.page.url, "https://x.test/");
        assert_eq!(
            snapshot.observation.regions[0].targets[0].name,
            "[REDACTED]"
        );
        assert!(serde_json::to_string(&snapshot).unwrap().len() < MAX_SNAPSHOT_BYTES);
    }
}
