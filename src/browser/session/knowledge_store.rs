//! Crash-safe local persistence for bounded [`super::KnowledgeRecord`] values.
//!
//! The store is intentionally an explicit library component. Opening it does
//! not make a browser session consult remembered data; callers must opt into
//! each later knowledge-assisted operation.

use super::{
    KNOWLEDGE_SCHEMA_VERSION, KnowledgeAssessment, KnowledgeConfidence, KnowledgeLookupContext,
    KnowledgeRecord, KnowledgeStoreSnapshot, KnowledgeValidationError, MAX_KNOWLEDGE_RECORDS,
};
use fs2::FileExt;
use serde::Serialize;
use std::cmp::Ordering;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering as AtomicOrdering};

pub const DEFAULT_KNOWLEDGE_STORE_BYTES: usize = 4 * 1024 * 1024;
const STORE_LOCK_SUFFIX: &str = ".lock";
static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(1);

/// Default per-profile knowledge file used by local management surfaces.
pub fn default_knowledge_store_path(profile: &str) -> PathBuf {
    std::env::var_os("GLASS_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(dirs::config_dir)
        .unwrap_or_else(|| PathBuf::from("."))
        .join("glass")
        .join("knowledge")
        .join(format!("{profile}.json"))
}

/// Resource bounds for one local knowledge store.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KnowledgeStoreLimits {
    pub max_records: usize,
    pub max_bytes: usize,
}

impl Default for KnowledgeStoreLimits {
    fn default() -> Self {
        Self {
            max_records: MAX_KNOWLEDGE_RECORDS,
            max_bytes: DEFAULT_KNOWLEDGE_STORE_BYTES,
        }
    }
}

impl KnowledgeStoreLimits {
    fn validate(self) -> Result<(), KnowledgeStoreError> {
        if self.max_records == 0 || self.max_records > MAX_KNOWLEDGE_RECORDS {
            return Err(KnowledgeStoreError::InvalidConfiguration(format!(
                "maxRecords must be between 1 and {MAX_KNOWLEDGE_RECORDS}"
            )));
        }
        if self.max_bytes == 0 {
            return Err(KnowledgeStoreError::InvalidConfiguration(
                "maxBytes must be positive".into(),
            ));
        }
        Ok(())
    }
}

/// Result of an upsert or removal, including deterministic pruning evidence.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgeStoreChange {
    pub record_id: String,
    pub retained: bool,
    pub removed: bool,
    pub pruned_record_ids: Vec<String>,
}

/// Bounded store statistics suitable for CLI/TUI display.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgeStoreStats {
    pub record_count: usize,
    pub serialized_bytes: usize,
    pub candidate_count: usize,
    pub observed_count: usize,
    pub verified_count: usize,
    pub stale_count: usize,
    pub contradicted_count: usize,
    pub quarantined_count: usize,
}

/// A validated local knowledge store backed by one JSON snapshot file.
#[derive(Debug)]
pub struct KnowledgeStore {
    path: PathBuf,
    limits: KnowledgeStoreLimits,
    snapshot: KnowledgeStoreSnapshot,
    last_pruned: Option<Vec<String>>,
}

impl KnowledgeStore {
    /// Open an existing store or an empty store at `path`.
    pub fn open(path: impl Into<PathBuf>) -> Result<Self, KnowledgeStoreError> {
        Self::open_with_limits(path, KnowledgeStoreLimits::default())
    }

    /// Open a store with explicit record and serialized-byte limits.
    pub fn open_with_limits(
        path: impl Into<PathBuf>,
        limits: KnowledgeStoreLimits,
    ) -> Result<Self, KnowledgeStoreError> {
        limits.validate()?;
        let path = path.into();
        let snapshot = read_snapshot(&path)?;
        validate_snapshot_size(&snapshot, limits.max_bytes)?;
        Ok(Self {
            path,
            limits,
            snapshot,
            last_pruned: None,
        })
    }

    /// Return the path used for the snapshot file.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Return the active store limits.
    pub fn limits(&self) -> KnowledgeStoreLimits {
        self.limits
    }

    /// Return the last validated in-memory snapshot.
    pub fn snapshot(&self) -> &KnowledgeStoreSnapshot {
        &self.snapshot
    }

    /// Re-read the snapshot under the store lock.
    pub fn refresh(&mut self) -> Result<(), KnowledgeStoreError> {
        let _lock = StoreLock::acquire(&self.path)?;
        let snapshot = read_snapshot(&self.path)?;
        validate_snapshot_size(&snapshot, self.limits.max_bytes)?;
        self.snapshot = snapshot;
        self.last_pruned = None;
        Ok(())
    }

    /// Find one record by its stable ID.
    pub fn get(&self, record_id: &str) -> Option<&KnowledgeRecord> {
        self.snapshot
            .records
            .iter()
            .find(|record| record.record_id == record_id)
    }

    /// Assess all records against fresh session dimensions without changing
    /// the store or returning any executable browser reference.
    pub fn assess(&self, context: &KnowledgeLookupContext) -> Vec<KnowledgeAssessment> {
        self.snapshot
            .records
            .iter()
            .map(|record| record.assess(context))
            .collect()
    }

    /// Return the validated records for inspection or export.
    pub fn records(&self) -> &[KnowledgeRecord] {
        &self.snapshot.records
    }

    /// Return bounded serialized-size and lifecycle counts.
    pub fn stats(&self) -> Result<KnowledgeStoreStats, KnowledgeStoreError> {
        let mut stats = KnowledgeStoreStats {
            record_count: self.snapshot.records.len(),
            serialized_bytes: serialized_size(&self.snapshot)?,
            candidate_count: 0,
            observed_count: 0,
            verified_count: 0,
            stale_count: 0,
            contradicted_count: 0,
            quarantined_count: 0,
        };
        for record in &self.snapshot.records {
            match record.confidence {
                KnowledgeConfidence::Candidate => stats.candidate_count += 1,
                KnowledgeConfidence::Observed => stats.observed_count += 1,
                KnowledgeConfidence::Verified => stats.verified_count += 1,
                KnowledgeConfidence::Stale => stats.stale_count += 1,
                KnowledgeConfidence::Contradicted => stats.contradicted_count += 1,
                KnowledgeConfidence::Quarantined => stats.quarantined_count += 1,
            }
        }
        Ok(stats)
    }

    /// Replace the complete store only after validating its schema and bounds.
    pub fn replace_snapshot(
        &mut self,
        snapshot: KnowledgeStoreSnapshot,
    ) -> Result<(), KnowledgeStoreError> {
        snapshot
            .validate()
            .map_err(KnowledgeStoreError::InvalidContract)?;
        if snapshot.records.len() > self.limits.max_records {
            return Err(KnowledgeStoreError::Capacity(
                "snapshot exceeds the configured record limit".into(),
            ));
        }
        validate_snapshot_size(&snapshot, self.limits.max_bytes)?;
        let _lock = StoreLock::acquire(&self.path)?;
        write_snapshot(&self.path, &snapshot, self.limits.max_bytes)?;
        self.snapshot = snapshot;
        self.last_pruned = None;
        Ok(())
    }

    /// Transition one record and retain the lifecycle evidence in its history.
    pub fn transition(
        &mut self,
        record_id: &str,
        next: KnowledgeConfidence,
        reason: String,
        observed_at: String,
        fresh_verification: bool,
    ) -> Result<KnowledgeStoreChange, KnowledgeStoreError> {
        let record_id = record_id.to_owned();
        self.mutate(|snapshot| {
            let record = snapshot
                .records
                .iter_mut()
                .find(|record| record.record_id == record_id)
                .ok_or_else(|| KnowledgeStoreError::NotFound(record_id.clone()))?;
            record.transition(next, reason, observed_at, fresh_verification)?;
            Ok(())
        })?;
        Ok(KnowledgeStoreChange {
            retained: true,
            removed: false,
            record_id,
            pruned_record_ids: self.last_pruned.take().unwrap_or_default(),
        })
    }

    /// Remove every record scoped to one exact origin.
    pub fn purge_origin(
        &mut self,
        origin: &str,
    ) -> Result<KnowledgePurgeResult, KnowledgeStoreError> {
        let mut removed_record_ids = Vec::new();
        self.mutate(|snapshot| {
            snapshot.records.retain(|record| {
                if record.scope.origin == origin {
                    removed_record_ids.push(record.record_id.clone());
                    false
                } else {
                    true
                }
            });
            Ok(())
        })?;
        Ok(KnowledgePurgeResult {
            removed_record_ids,
            pruned_record_ids: self.last_pruned.take().unwrap_or_default(),
        })
    }

    /// Insert or replace a record, then prune the least useful records first.
    pub fn upsert(
        &mut self,
        record: KnowledgeRecord,
    ) -> Result<KnowledgeStoreChange, KnowledgeStoreError> {
        record.validate()?;
        if record.to_canonical_json()?.len() > self.limits.max_bytes {
            return Err(KnowledgeStoreError::Capacity(
                "record exceeds the configured store byte limit".into(),
            ));
        }
        let record_id = record.record_id.clone();
        self.mutate(|snapshot| {
            if let Some(existing) = snapshot
                .records
                .iter_mut()
                .find(|existing| existing.record_id == record_id)
            {
                *existing = record;
            } else {
                snapshot.records.push(record);
            }
            Ok(())
        })?;
        let retained = self.get(&record_id).is_some();
        if !retained {
            return Err(KnowledgeStoreError::Capacity(
                "record was pruned by the configured store limits".into(),
            ));
        }
        Ok(KnowledgeStoreChange {
            record_id,
            retained,
            removed: false,
            pruned_record_ids: self.last_pruned.take().unwrap_or_default(),
        })
    }

    /// Remove one record and persist the resulting snapshot.
    pub fn remove(&mut self, record_id: &str) -> Result<KnowledgeStoreChange, KnowledgeStoreError> {
        let record_id = record_id.to_owned();
        let existed = self.mutate(|snapshot| {
            let before = snapshot.records.len();
            snapshot
                .records
                .retain(|record| record.record_id != record_id);
            Ok(before != snapshot.records.len())
        })?;
        Ok(KnowledgeStoreChange {
            record_id,
            retained: false,
            removed: existed,
            pruned_record_ids: if existed {
                self.last_pruned.take().unwrap_or_default()
            } else {
                Vec::new()
            },
        })
    }
}

impl KnowledgeStore {
    fn mutate<T, F>(&mut self, operation: F) -> Result<T, KnowledgeStoreError>
    where
        F: FnOnce(&mut KnowledgeStoreSnapshot) -> Result<T, KnowledgeStoreError>,
    {
        let _lock = StoreLock::acquire(&self.path)?;
        let mut snapshot = read_snapshot(&self.path)?;
        let result = operation(&mut snapshot)?;
        let pruned = prune_snapshot(&mut snapshot, self.limits)?;
        write_snapshot(&self.path, &snapshot, self.limits.max_bytes)?;
        self.snapshot = snapshot;
        self.last_pruned = Some(pruned);
        Ok(result)
    }
}

fn read_snapshot(path: &Path) -> Result<KnowledgeStoreSnapshot, KnowledgeStoreError> {
    let bytes = match fs::read(path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            return Ok(KnowledgeStoreSnapshot {
                schema_version: KNOWLEDGE_SCHEMA_VERSION,
                records: Vec::new(),
            });
        }
        Err(error) => return Err(KnowledgeStoreError::Io(error)),
    };
    let snapshot = serde_json::from_slice::<KnowledgeStoreSnapshot>(&bytes)
        .map_err(|error| KnowledgeStoreError::Corrupt(format!("invalid JSON snapshot: {error}")))?;
    snapshot
        .validate()
        .map_err(KnowledgeStoreError::InvalidContract)?;
    Ok(snapshot)
}

fn write_snapshot(
    path: &Path,
    snapshot: &KnowledgeStoreSnapshot,
    max_bytes: usize,
) -> Result<(), KnowledgeStoreError> {
    let canonical = snapshot
        .to_canonical_json()
        .map_err(KnowledgeStoreError::InvalidContract)?;
    if canonical.len() > max_bytes {
        return Err(KnowledgeStoreError::Capacity(
            "snapshot exceeds the configured store byte limit".into(),
        ));
    }
    let parent = parent_dir(path);
    fs::create_dir_all(parent).map_err(KnowledgeStoreError::Io)?;
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| {
            KnowledgeStoreError::InvalidConfiguration("store path has no filename".into())
        })?;
    let sequence = TEMP_SEQUENCE.fetch_add(1, AtomicOrdering::Relaxed);
    let temporary = parent.join(format!(
        ".{file_name}.{}.{}.tmp",
        std::process::id(),
        sequence
    ));
    let result = (|| {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)
            .map_err(KnowledgeStoreError::Io)?;
        file.write_all(canonical.as_bytes())
            .map_err(KnowledgeStoreError::Io)?;
        file.sync_all().map_err(KnowledgeStoreError::Io)?;
        fs::rename(&temporary, path).map_err(KnowledgeStoreError::Io)?;
        #[cfg(unix)]
        File::open(parent)
            .and_then(|directory| directory.sync_all())
            .map_err(KnowledgeStoreError::Io)?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

fn validate_snapshot_size(
    snapshot: &KnowledgeStoreSnapshot,
    max_bytes: usize,
) -> Result<(), KnowledgeStoreError> {
    let bytes = snapshot
        .to_canonical_json()
        .map_err(KnowledgeStoreError::InvalidContract)?
        .len();
    if bytes > max_bytes {
        return Err(KnowledgeStoreError::Capacity(
            "snapshot exceeds the configured store byte limit".into(),
        ));
    }
    Ok(())
}

fn prune_snapshot(
    snapshot: &mut KnowledgeStoreSnapshot,
    limits: KnowledgeStoreLimits,
) -> Result<Vec<String>, KnowledgeStoreError> {
    let mut pruned = Vec::new();
    while snapshot.records.len() > limits.max_records
        || serialized_size(snapshot)? > limits.max_bytes
    {
        let Some(index) = snapshot
            .records
            .iter()
            .enumerate()
            .min_by(|(_, left), (_, right)| prune_order(left, right))
            .map(|(index, _)| index)
        else {
            break;
        };
        pruned.push(snapshot.records.remove(index).record_id);
    }
    if serialized_size(snapshot)? > limits.max_bytes {
        return Err(KnowledgeStoreError::Capacity(
            "cannot fit the snapshot within the configured byte limit".into(),
        ));
    }
    Ok(pruned)
}

fn serialized_size(snapshot: &KnowledgeStoreSnapshot) -> Result<usize, KnowledgeStoreError> {
    snapshot
        .to_canonical_json()
        .map(|json| json.len())
        .map_err(KnowledgeStoreError::InvalidContract)
}

fn prune_order(left: &KnowledgeRecord, right: &KnowledgeRecord) -> Ordering {
    confidence_rank(left.confidence)
        .cmp(&confidence_rank(right.confidence))
        .then_with(|| {
            left.source
                .last_verified_at
                .cmp(&right.source.last_verified_at)
        })
        .then_with(|| left.record_id.cmp(&right.record_id))
}

fn confidence_rank(confidence: KnowledgeConfidence) -> u8 {
    match confidence {
        KnowledgeConfidence::Quarantined => 0,
        KnowledgeConfidence::Contradicted => 1,
        KnowledgeConfidence::Stale => 2,
        KnowledgeConfidence::Candidate => 3,
        KnowledgeConfidence::Observed => 4,
        KnowledgeConfidence::Verified => 5,
    }
}

struct StoreLock {
    file: File,
}

impl StoreLock {
    fn acquire(path: &Path) -> Result<Self, KnowledgeStoreError> {
        let lock_path = PathBuf::from(format!("{}{}", path.display(), STORE_LOCK_SUFFIX));
        fs::create_dir_all(parent_dir(&lock_path)).map_err(KnowledgeStoreError::Io)?;
        let file = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(lock_path)
            .map_err(KnowledgeStoreError::Io)?;
        file.lock_exclusive().map_err(KnowledgeStoreError::Io)?;
        Ok(Self { file })
    }
}

fn parent_dir(path: &Path) -> &Path {
    path.parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."))
}

impl Drop for StoreLock {
    fn drop(&mut self) {
        let _ = FileExt::unlock(&self.file);
    }
}

/// Errors returned by the local knowledge store.
#[derive(Debug)]
pub enum KnowledgeStoreError {
    Io(io::Error),
    Corrupt(String),
    InvalidContract(KnowledgeValidationError),
    InvalidConfiguration(String),
    Capacity(String),
    NotFound(String),
}

impl std::fmt::Display for KnowledgeStoreError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(error) => write!(formatter, "knowledge store I/O error: {error}"),
            Self::Corrupt(reason) => write!(formatter, "knowledge store is corrupt: {reason}"),
            Self::InvalidContract(error) => write!(formatter, "knowledge contract: {error}"),
            Self::InvalidConfiguration(reason) => {
                write!(formatter, "invalid knowledge store configuration: {reason}")
            }
            Self::Capacity(reason) => write!(formatter, "knowledge store capacity: {reason}"),
            Self::NotFound(record_id) => {
                write!(formatter, "knowledge record not found: {record_id}")
            }
        }
    }
}

/// Result of purging records for one origin.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgePurgeResult {
    pub removed_record_ids: Vec<String>,
    pub pruned_record_ids: Vec<String>,
}

impl std::error::Error for KnowledgeStoreError {}

impl From<KnowledgeValidationError> for KnowledgeStoreError {
    fn from(error: KnowledgeValidationError) -> Self {
        Self::InvalidContract(error)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::knowledge::{
        KnowledgeBackendKind, KnowledgeBackendProvenance, KnowledgeBackendCapability,
        KnowledgeCurrentValidation, KnowledgeEvidenceQuality, KnowledgeMemoryInfluence,
        KnowledgePortability, KnowledgeRetrievalExplanation, KnowledgeSurfaceCoverage,
        KnowledgeSurfaceKind, KnowledgeSurfaceProvenance, KnowledgeUnderstandingLevel,
    };
    use crate::browser::session::{
        KnowledgeInvalidation, KnowledgeLookupContext, KnowledgeProfileScope, KnowledgeRecordKind,
        KnowledgeScope, KnowledgeSource,
    };
    use serde_json::json;
    use std::sync::atomic::{AtomicU64, Ordering};

    static TEST_SEQUENCE: AtomicU64 = AtomicU64::new(1);

    fn test_path() -> PathBuf {
        std::env::temp_dir().join(format!(
            "glass-knowledge-store-{}-{}",
            std::process::id(),
            TEST_SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ))
    }

    fn record(id: &str, confidence: KnowledgeConfidence, verified_at: &str) -> KnowledgeRecord {
        KnowledgeRecord {
            schema_version: KNOWLEDGE_SCHEMA_VERSION,
            record_id: id.into(),
            kind: KnowledgeRecordKind::PageFamily,
            scope: KnowledgeScope {
                origin: "https://example.test".into(),
                path_pattern: "/docs/*".into(),
                profile_scope: KnowledgeProfileScope::Anonymous,
                profile_key: None,
                locale: None,
                tenant_key: None,
                browser_family: "chromium".into(),
                browser_version_range: None,
                glass_schema_version: 1,
                policy_preset: "balanced".into(),
            },
            source: KnowledgeSource {
                first_seen_at: format!("2026-07-27T00:00:{verified_at:0>2}Z"),
                last_verified_at: format!("2026-07-27T00:00:{verified_at:0>2}Z"),
                glass_version: "0.2.0".into(),
                verification_count: 1,
                surface: KnowledgeSurfaceProvenance {
                    kind: KnowledgeSurfaceKind::Document,
                    understanding: KnowledgeUnderstandingLevel::Strong,
                    coverage: KnowledgeSurfaceCoverage::Semantic,
                },
                backend: KnowledgeBackendProvenance {
                    backend: KnowledgeBackendKind::Cdp,
                    profile: "production".into(),
                    capabilities: vec![
                        KnowledgeBackendCapability::SemanticExtraction,
                        KnowledgeBackendCapability::Verification,
                    ],
                },
            },
            confidence,
            invalidation: KnowledgeInvalidation::default(),
            data: json!({"pageKind": "documentation"}),
            portability: KnowledgePortability::SurfacePortable,
            memory_influence: KnowledgeMemoryInfluence::None,
            retrieval: KnowledgeRetrievalExplanation {
                signals: Vec::new(),
                current_validation: KnowledgeCurrentValidation {
                    status: super::super::knowledge::KnowledgeCurrentValidationStatus::Validated,
                    evidence_quality: KnowledgeEvidenceQuality::Strong,
                    current_revision: Some(1),
                    validated_at: Some("2026-07-27T00:00:00Z".into()),
                },
            },
            history: Vec::new(),
        }
    }

    #[test]
    fn upsert_reopens_from_an_atomic_snapshot() {
        let path = test_path();
        let mut store = KnowledgeStore::open(&path).unwrap();
        store
            .upsert(record("knowledge_1", KnowledgeConfidence::Verified, "2"))
            .unwrap();
        let reopened = KnowledgeStore::open(&path).unwrap();
        assert!(reopened.get("knowledge_1").is_some());
        assert_eq!(reopened.snapshot().records.len(), 1);
        let _ = fs::remove_file(&path);
        let _ = fs::remove_file(format!("{}{}", path.display(), STORE_LOCK_SUFFIX));
    }

    #[test]
    fn pruning_prefers_low_confidence_and_old_records() {
        let path = test_path();
        let limits = KnowledgeStoreLimits {
            max_records: 2,
            max_bytes: DEFAULT_KNOWLEDGE_STORE_BYTES,
        };
        let mut store = KnowledgeStore::open_with_limits(&path, limits).unwrap();
        store
            .upsert(record("verified", KnowledgeConfidence::Verified, "1"))
            .unwrap();
        store
            .upsert(record("stale", KnowledgeConfidence::Stale, "3"))
            .unwrap();
        let change = store
            .upsert(record("observed", KnowledgeConfidence::Observed, "2"))
            .unwrap();
        assert_eq!(change.pruned_record_ids, vec!["stale"]);
        assert!(store.get("verified").is_some());
        assert!(store.get("observed").is_some());
        let _ = fs::remove_file(&path);
        let _ = fs::remove_file(format!("{}{}", path.display(), STORE_LOCK_SUFFIX));
    }

    #[test]
    fn malformed_snapshot_is_not_silently_replaced() {
        let path = test_path();
        fs::write(&path, b"not-json").unwrap();
        let error = KnowledgeStore::open(&path).unwrap_err();
        assert!(matches!(error, KnowledgeStoreError::Corrupt(_)));
        let _ = fs::remove_file(&path);
    }

    #[test]
    fn assessment_is_read_only_and_returns_current_evidence() {
        let path = test_path();
        let mut store = KnowledgeStore::open(&path).unwrap();
        store
            .upsert(record("knowledge_1", KnowledgeConfidence::Verified, "01"))
            .unwrap();
        let context = KnowledgeLookupContext {
            origin: "https://example.test".into(),
            path: "/docs/start".into(),
            profile_scope: KnowledgeProfileScope::Anonymous,
            profile_key: None,
            locale: None,
            tenant_key: None,
            browser_family: "chromium".into(),
            browser_version: None,
            glass_schema_version: 1,
            policy_preset: "balanced".into(),
            current_revision: 1,
            landmarks: vec!["documentation".into()],
            now_epoch_seconds: chrono::DateTime::parse_from_rfc3339("2026-07-27T00:00:02Z")
                .unwrap()
                .timestamp(),
            surface_kind: None,
            backend_kind: None,
            backend_capabilities: Vec::new(),
        };
        let assessments = store.assess(&context);
        assert_eq!(assessments.len(), 1);
        assert_eq!(assessments[0].record_id, "knowledge_1");
        assert_eq!(store.snapshot().records.len(), 1);
        let _ = fs::remove_file(&path);
        let _ = fs::remove_file(format!("{}{}", path.display(), STORE_LOCK_SUFFIX));
    }

    #[test]
    fn lifecycle_management_and_stats_are_persisted() {
        let path = test_path();
        let mut store = KnowledgeStore::open(&path).unwrap();
        store
            .upsert(record("keep", KnowledgeConfidence::Observed, "01"))
            .unwrap();
        store
            .upsert(record("purge", KnowledgeConfidence::Observed, "02"))
            .unwrap();
        store
            .transition(
                "keep",
                KnowledgeConfidence::Verified,
                "fresh landmark verification".into(),
                "2026-07-27T00:00:03Z".into(),
                true,
            )
            .unwrap();
        let stats = store.stats().unwrap();
        assert_eq!(stats.record_count, 2);
        assert_eq!(stats.verified_count, 1);
        let purge = store.purge_origin("https://example.test").unwrap();
        assert_eq!(purge.removed_record_ids, vec!["keep", "purge"]);
        assert!(store.records().is_empty());
        let _ = fs::remove_file(&path);
        let _ = fs::remove_file(format!("{}{}", path.display(), STORE_LOCK_SUFFIX));
    }
}
