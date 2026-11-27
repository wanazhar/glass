//! Durable product state on disk.
//!
//! Marble is self-hosted and single-process, so a revisioned JSON document with
//! atomic replacement is enough: a crash leaves either the previous revision or
//! the complete new one, never a half-written mix.

use crate::MarbleError;
use crate::state::ProductState;
use serde_json::{Value, json};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

/// File name of the durable product document.
pub const STATE_FILE: &str = "product-state.json";
/// Temporary file name used while replacing the document.
pub const STATE_TEMP_FILE: &str = "product-state.json.tmp";
/// Directory beside the state file where daily backups are written.
pub const BACKUP_DIR: &str = "backups";
/// Daily backups kept before the oldest is removed.
pub const BACKUP_KEEP: usize = 7;

/// Whether the last durable write succeeded.
///
/// A failed write is the difference between an assistant that looks healthy and
/// one that is silently forgetting everything since the last successful one, so
/// `/healthz` reports it instead of leaving it in a log.
static LAST_PERSIST_ERROR: OnceLock<Mutex<Option<String>>> = OnceLock::new();

fn persist_slot() -> &'static Mutex<Option<String>> {
    LAST_PERSIST_ERROR.get_or_init(|| Mutex::new(None))
}

/// The last write failure, or `None` when the document is being written.
pub fn last_persist_error() -> Option<String> {
    persist_slot()
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .clone()
}

fn record_persist_outcome(error: Option<&MarbleError>) {
    let mut slot = persist_slot()
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    *slot = error.map(|error| error.to_string());
}

/// Resolve the self-hosted data directory.
///
/// Order: `GLASS_MARBLE_DATA_DIR`, `XDG_DATA_HOME/glass-marble`,
/// `~/.local/share/glass-marble`, then a stable temp fallback.
pub fn default_data_dir() -> PathBuf {
    if let Some(configured) = std::env::var_os("GLASS_MARBLE_DATA_DIR")
        && !configured.is_empty()
    {
        return PathBuf::from(configured);
    }
    if let Some(xdg) = std::env::var_os("XDG_DATA_HOME")
        && !xdg.is_empty()
    {
        return PathBuf::from(xdg).join("glass-marble");
    }
    if let Some(home) = std::env::var_os("HOME")
        && !home.is_empty()
    {
        return PathBuf::from(home).join(".local/share/glass-marble");
    }
    std::env::temp_dir().join("glass-marble")
}

/// Wall clock in milliseconds, used only to name kept-aside documents.
fn unix_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_millis() as u64)
        .unwrap_or_default()
}

/// Durable runtime session storage under the product data directory.
pub fn sessions_dir(data_dir: &Path) -> PathBuf {
    data_dir.join("sessions")
}

/// In-flight turn journal beside the product document.
pub fn live_dir(data_dir: &Path) -> PathBuf {
    data_dir.join("live")
}

/// Replace one small file so a crash leaves the old bytes or the new ones.
///
/// `sync_all` is what makes this survive a power loss rather than only a
/// process crash: without it the rename can be visible while the contents it
/// points at are still in the page cache. The parent directory is synced too,
/// because a rename is itself a directory change.
pub fn write_durably(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let parent = path.parent().ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "durable write path has no parent directory",
        )
    })?;
    fs::create_dir_all(parent)?;
    let name = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| "document".into());
    // Several threads persist the same document; a shared temporary name let
    // one writer rename another's half-written file, or find it already gone.
    static SEQUENCE: AtomicU64 = AtomicU64::new(0);
    let sequence = SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let temp = parent.join(format!("{name}.{}-{sequence}.tmp", std::process::id()));
    {
        let mut file = fs::File::create(&temp)?;
        std::io::Write::write_all(&mut file, bytes)?;
        file.sync_all()?;
    }
    fs::rename(&temp, path)?;
    // Best effort: not every platform lets a directory be opened for sync, and
    // the document itself is already durable when this fails.
    if let Ok(directory) = fs::File::open(parent) {
        let _ = directory.sync_all();
    }
    Ok(())
}

/// Revisioned product state document.
#[derive(Debug, Clone)]
pub struct StateStore {
    path: PathBuf,
}

impl StateStore {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    /// Store the document inside `data_dir`.
    pub fn in_data_dir(data_dir: &Path) -> Self {
        Self::new(data_dir.join(STATE_FILE))
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Load the document, returning an empty state when it does not exist yet.
    ///
    /// A document that cannot be parsed is never treated as empty: empty is a
    /// real state that the next write would make permanent. The newest daily
    /// backup is restored instead, and the unreadable file is moved aside so it
    /// can still be inspected. With no backup to fall back on, this refuses to
    /// start rather than overwrite what the operator had.
    pub fn load(&self) -> Result<ProductState, MarbleError> {
        match fs::read_to_string(&self.path) {
            Ok(encoded) => match serde_json::from_str::<ProductState>(&encoded) {
                Ok(mut state) => {
                    state.ensure_entry_ids();
                    crate::turn::separate_stored_reasoning(&mut state);
                    state.prune_threads();
                    Ok(state)
                }
                Err(error) => {
                    tracing::warn!(
                        %error,
                        path = %self.path.display(),
                        "product state is unreadable; trying the newest daily backup"
                    );
                    self.recover_from_backup(&error).map(|mut state| {
                        state.ensure_entry_ids();
                        state.prune_threads();
                        state
                    })
                }
            },
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                Ok(ProductState::default())
            }
            Err(error) => Err(MarbleError::Io(error)),
        }
    }

    fn recover_from_backup(
        &self,
        parse_error: &serde_json::Error,
    ) -> Result<ProductState, MarbleError> {
        let Some(backup) = self.newest_backup() else {
            return Err(MarbleError::Protocol(format!(
                "cannot read product state at {}: {parse_error}; no daily backup is available, so \
                 nothing was replaced — run `glass-marble restore --from <file>` or move the file aside",
                self.path.display()
            )));
        };
        let state = Self::read_document(&backup)?;
        let quarantined = self
            .path
            .with_file_name(format!("product-state.corrupt-{}.json", unix_millis()));
        fs::rename(&self.path, &quarantined).map_err(MarbleError::Io)?;
        self.save(&state)?;
        tracing::warn!(
            backup = %backup.display(),
            quarantined = %quarantined.display(),
            "restored the product state from the newest daily backup"
        );
        Ok(state)
    }

    /// Daily backups on disk, oldest first.
    fn backup_paths(&self) -> Vec<PathBuf> {
        let Some(parent) = self.path.parent() else {
            return Vec::new();
        };
        let Ok(entries) = fs::read_dir(parent.join(BACKUP_DIR)) else {
            return Vec::new();
        };
        let mut paths: Vec<PathBuf> = entries
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .filter(|candidate| candidate.extension().is_some_and(|ext| ext == "json"))
            .collect();
        paths.sort();
        paths
    }

    /// The newest daily backup, when one exists.
    pub fn newest_backup(&self) -> Option<PathBuf> {
        self.backup_paths().pop()
    }

    /// Read a daily backup, or an export written by `/api/export`.
    fn read_document(path: &Path) -> Result<ProductState, MarbleError> {
        let encoded = fs::read_to_string(path).map_err(MarbleError::Io)?;
        let value: serde_json::Value = serde_json::from_str(&encoded).map_err(|error| {
            MarbleError::Protocol(format!("cannot read {}: {error}", path.display()))
        })?;
        let document = value.get("state").cloned().unwrap_or(value);
        let mut state: ProductState = serde_json::from_value(document).map_err(|error| {
            MarbleError::Protocol(format!(
                "{} is not a marble product document: {error}",
                path.display()
            ))
        })?;
        state.ensure_entry_ids();
        crate::turn::separate_stored_reasoning(&mut state);
        state.prune_threads();
        Ok(state)
    }

    /// Replace the live document with a daily backup, or with `from` (a backup
    /// or an `/api/export` file). The document being replaced is copied aside
    /// first, so a restore is itself reversible.
    ///
    /// Stop the service before restoring: it holds the document in memory and
    /// would write that copy back on its next change.
    pub fn restore(&self, from: Option<&Path>) -> Result<(ProductState, PathBuf), MarbleError> {
        let source = match from {
            Some(path) => path.to_path_buf(),
            None => self.newest_backup().ok_or_else(|| {
                MarbleError::Protocol(format!(
                    "no backups under {} and no --from file given",
                    self.path
                        .parent()
                        .map(|parent| parent.join(BACKUP_DIR).display().to_string())
                        .unwrap_or_else(|| BACKUP_DIR.into())
                ))
            })?,
        };
        let state = Self::read_document(&source)?;
        if self.path.exists() {
            let replaced = self
                .path
                .with_file_name(format!("product-state.replaced-{}.json", unix_millis()));
            if let Err(error) = fs::copy(&self.path, &replaced) {
                tracing::warn!(%error, "could not keep a copy of the replaced product state");
            }
        }
        self.save(&state)?;
        Ok((state, source))
    }

    /// Write one backup per UTC day, keeping a week of history.
    ///
    /// Trimming bounds the live document, and an export is only useful if the
    /// person remembers to ask for it. A daily copy beside the state file means
    /// a bad edit, a corrupt document, or a mistaken deletion is recoverable
    /// without a service to restore from. Returns the path when a new backup was
    /// written.
    pub fn write_daily_backup(&self, state: &ProductState, now: u64) -> Option<std::path::PathBuf> {
        let parent = self.path.parent()?;
        let directory = parent.join(BACKUP_DIR);
        let path = directory.join(format!("product-state-{}.json", now / 86_400_000));
        if path.exists() {
            return None;
        }
        fs::create_dir_all(&directory).ok()?;
        let encoded = serde_json::to_vec_pretty(state).ok()?;
        fs::write(&path, encoded).ok()?;
        let mut backups: Vec<std::path::PathBuf> = fs::read_dir(&directory)
            .ok()?
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .filter(|candidate| candidate.extension().is_some_and(|ext| ext == "json"))
            .collect();
        backups.sort();
        while backups.len() > BACKUP_KEEP {
            let oldest = backups.remove(0);
            let _ = fs::remove_file(oldest);
        }
        Some(path)
    }

    /// Directory the document and its backups live in.
    pub fn data_dir(&self) -> Option<&Path> {
        self.path.parent()
    }

    pub(crate) fn conversation_root(&self, id: &str) -> Option<PathBuf> {
        (!id.is_empty()
            && id
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_')))
        .then(|| {
            self.data_dir()
                .map(|dir| dir.join("conversation-sessions").join(id))
        })
        .flatten()
    }

    /// Whether a durable document exists at all.
    pub fn document_exists(&self) -> bool {
        self.path.exists()
    }

    /// Daily backups on disk, how old the newest one is, and how many are kept.
    ///
    /// An assistant that has been up for a month without a backup is one bad
    /// write away from starting over, and that is not something an operator
    /// should have to discover from a directory listing.
    pub fn backup_status(&self) -> Value {
        let paths = self.backup_paths();
        let newest = paths.last().cloned();
        let age_seconds = newest
            .as_ref()
            .and_then(|path| fs::metadata(path).ok())
            .and_then(|metadata| metadata.modified().ok())
            .and_then(|modified| modified.duration_since(UNIX_EPOCH).ok())
            .map(|elapsed| unix_millis().saturating_sub(elapsed.as_millis() as u64) / 1000);
        json!({
            "count": paths.len(),
            "keep": BACKUP_KEEP,
            "newest": newest.map(|path| path.display().to_string()),
            "ageSeconds": age_seconds,
        })
    }

    /// Documents moved aside because they could not be parsed.
    ///
    /// They are kept so the operator can inspect them; each one is also a sign
    /// that a write or a disk went wrong, so the count is part of health.
    pub fn quarantined_count(&self) -> usize {
        let Some(parent) = self.path.parent() else {
            return 0;
        };
        let Ok(entries) = fs::read_dir(parent) else {
            return 0;
        };
        entries
            .filter_map(Result::ok)
            .filter(|entry| {
                entry
                    .file_name()
                    .to_string_lossy()
                    .starts_with("product-state.corrupt-")
            })
            .count()
    }

    /// Write the document atomically, replacing the previous revision.
    pub fn save(&self, state: &ProductState) -> Result<(), MarbleError> {
        let parent = self
            .path
            .parent()
            .ok_or_else(|| MarbleError::Protocol("product state path has no parent".into()))?;
        fs::create_dir_all(parent).map_err(MarbleError::Io)?;
        let encoded = serde_json::to_vec_pretty(state).map_err(|error| {
            MarbleError::Protocol(format!("cannot encode product state: {error}"))
        })?;
        // The temporary file is named after the document so a failed write
        // leaves nothing else behind; `write_durably` fsyncs before the rename.
        // The error names the file it could not write — "No such file or
        // directory" alone does not say which directory went missing.
        // Saves are serialised per document, and an older snapshot that loses
        // the race never replaces a newer revision already on disk.
        static SAVED: OnceLock<Mutex<HashMap<PathBuf, u64>>> = OnceLock::new();
        let mut saved = SAVED
            .get_or_init(|| Mutex::new(HashMap::new()))
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        if saved
            .get(&self.path)
            .is_some_and(|revision| *revision > state.revision)
        {
            return Ok(());
        }
        write_durably(&self.path, &encoded).map_err(|error| {
            MarbleError::Protocol(format!("cannot write {}: {error}", self.path.display()))
        })?;
        saved.insert(self.path.clone(), state.revision);
        if let Some(events) = &state.thread_events.0 {
            events.publish_replies(state);
        }
        Ok(())
    }
}

/// Load product state from `data_dir`, falling back to an empty document only
/// when nothing at all can be read.
///
/// Serving does not use this: a surface that would write the document back must
/// report an unreadable file instead of quietly starting over.
pub fn load_or_default(data_dir: &Path) -> ProductState {
    let store = StateStore::in_data_dir(data_dir);
    let mut state = store.load().unwrap_or_else(|error| {
        tracing::warn!(%error, "starting from empty marble product state");
        ProductState::default()
    });
    // Tasks recorded for conversations that no longer exist are orphans —
    // nothing can open them, and a surface would show them as work anyway.
    let orphaned = state.prune_orphaned_tasks();
    let orphaned_everyone = state.prune_everyone_entries();
    if orphaned_everyone > 0 {
        tracing::info!(
            orphaned_everyone,
            "dropped Everyone entries quoting deleted turns"
        );
    }
    if orphaned > 0 {
        tracing::info!(orphaned, "pruned tasks whose conversations are gone");
        state.bump();
        persist(&store, &state);
    }
    state
}

/// Persist one snapshot, keeping the revision the caller already bumped.
pub fn persist(store: &StateStore, state: &ProductState) {
    let outcome = store.save(state);
    record_persist_outcome(outcome.as_ref().err());
    if let Err(error) = outcome {
        tracing::warn!(%error, "could not persist marble product state");
    }
}

pub(crate) fn persist_optional(store: Option<&StateStore>, state: &ProductState) {
    if let Some(store) = store {
        persist(store, state);
    } else if let Some(events) = &state.thread_events.0 {
        events.publish_replies(state);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::TaskRecord;

    fn data_dir(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("glass-marble-store-{}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn missing_document_loads_as_empty_state() {
        // A missing file is a first start, not damage; nothing to recover.
        let dir = data_dir("missing");
        let store = StateStore::in_data_dir(&dir);
        assert_eq!(store.load().unwrap(), ProductState::default());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn load_or_default_prunes_tasks_without_a_conversation() {
        // Test pollution and older documents leave tasks behind when their
        // conversation is gone. A load drops them and persists the fix.
        let dir = data_dir("orphans");
        let store = StateStore::in_data_dir(&dir);
        let mut state = ProductState::default();
        state.conversation_mut("kept").title = "real".into();
        state.tasks.push(TaskRecord {
            marble_id: String::new(),
            id: "t-kept".into(),
            conversation_id: "kept".into(),
            status: "completed".into(),
            owner: "agent".into(),
            prompt: "real work".into(),
            at: 1,
            routine_id: String::new(),
        });
        state.tasks.push(TaskRecord {
            marble_id: String::new(),
            id: "t-orphan".into(),
            conversation_id: "gone".into(),
            status: "interrupted".into(),
            owner: "agent".into(),
            prompt: "leftover".into(),
            at: 2,
            routine_id: String::new(),
        });
        store.save(&state).unwrap();

        let loaded = load_or_default(&dir);
        assert_eq!(loaded.tasks.len(), 1);
        assert_eq!(loaded.tasks[0].id, "t-kept");
        // The document on disk is fixed too, so the next boot sees no orphans.
        let reloaded = store.load().unwrap();
        assert_eq!(reloaded.tasks.len(), 1);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_missing_data_directory_is_recreated_on_save() {
        // Deleting the data directory under a running store used to surface as
        // a bare "No such file or directory". The save recreates whatever the
        // path needs instead of failing the write.
        let dir = data_dir("deleted");
        let store = StateStore::in_data_dir(&dir);
        let mut state = ProductState::default();
        state.bump();
        store.save(&state).unwrap();
        fs::remove_dir_all(&dir).unwrap();
        store.save(&state).unwrap();
        assert_eq!(store.load().unwrap(), state);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn saved_document_round_trips_with_revision() {
        let dir = data_dir("round-trip");
        let store = StateStore::in_data_dir(&dir);
        let mut state = ProductState::default();
        state.bump();
        state.conversation_mut("c1").title = "durable".into();
        store.save(&state).unwrap();
        assert_eq!(store.load().unwrap(), state);
        assert_eq!(store.load().unwrap().revision, 1);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn old_replies_are_loaded_with_their_planning_in_a_separate_row() {
        let dir = data_dir("legacy-reasoning");
        let store = StateStore::in_data_dir(&dir);
        let mut state = ProductState::default();
        let plan = "The user is asking why. I need to explain. I'll output only the final answer.";
        let answer = "My bad — just the answer.";
        for conversation in ["private", "everyone"] {
            state.push_transcript(crate::state::TranscriptEntry {
                conversation_id: conversation.into(),
                task_id: "test-turn".into(),
                kind: "assistant".into(),
                speaker: "dietrich".into(),
                speaker_name: "Dietrich".into(),
                text: format!("{plan}{answer}"),
                ..Default::default()
            });
        }
        store.save(&state).unwrap();
        let migrated = store.load().unwrap();
        for conversation in ["private", "everyone"] {
            let rows = migrated
                .transcript
                .iter()
                .filter(|entry| entry.conversation_id == conversation)
                .collect::<Vec<_>>();
            assert_eq!(rows.len(), 2);
            assert_eq!((&*rows[0].kind, &*rows[0].text), ("reasoning", plan));
            assert_eq!((&*rows[1].kind, &*rows[1].text), ("assistant", answer));
            assert_eq!(rows[0].task_id, rows[1].task_id);
            assert_eq!(rows[0].speaker_name, "Dietrich");
        }
        store.save(&migrated).unwrap();
        assert_eq!(store.load().unwrap(), migrated);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn corrupt_document_reports_instead_of_silently_resetting() {
        let dir = data_dir("corrupt");
        let store = StateStore::in_data_dir(&dir);
        fs::write(store.path(), b"{not json").unwrap();
        assert!(store.load().is_err());
        assert!(
            fs::read(store.path()).unwrap() == b"{not json",
            "an unreadable document with no backup is left untouched"
        );
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_corrupt_document_recovers_from_the_newest_backup() {
        let dir = data_dir("recover");
        let store = StateStore::in_data_dir(&dir);
        let mut older = ProductState::default();
        older.conversation_mut("old").title = "older".into();
        store.write_daily_backup(&older, 86_400_000).unwrap();
        let mut newer = ProductState::default();
        newer.conversation_mut("new").title = "newer".into();
        newer.bump();
        store.write_daily_backup(&newer, 2 * 86_400_000).unwrap();
        fs::write(store.path(), b"{ truncated").unwrap();

        let loaded = store.load().expect("recovered from a backup");
        assert_eq!(loaded, newer);
        let quarantined = fs::read_dir(&dir)
            .unwrap()
            .filter_map(Result::ok)
            .any(|entry| {
                entry
                    .file_name()
                    .to_string_lossy()
                    .starts_with("product-state.corrupt-")
            });
        assert!(quarantined, "the unreadable file is kept for inspection");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn restore_accepts_an_export_and_keeps_the_replaced_document() {
        let dir = data_dir("restore");
        let store = StateStore::in_data_dir(&dir);
        let mut live = ProductState::default();
        live.conversation_mut("live").title = "live".into();
        store.save(&live).unwrap();

        let mut wanted = ProductState::default();
        wanted.conversation_mut("c1").title = "from export".into();
        let export = dir.join("export.json");
        fs::write(
            &export,
            serde_json::to_vec(&serde_json::json!({
                "format": "glass-marble/v1",
                "exportedAt": 1,
                "revision": 2,
                "state": wanted,
            }))
            .unwrap(),
        )
        .unwrap();

        let (restored, source) = store.restore(Some(&export)).unwrap();
        assert_eq!(source, export);
        assert_eq!(restored, wanted);
        assert_eq!(store.load().unwrap(), wanted);
        assert!(
            fs::read_dir(&dir)
                .unwrap()
                .filter_map(Result::ok)
                .any(|entry| entry
                    .file_name()
                    .to_string_lossy()
                    .starts_with("product-state.replaced-")),
            "the replaced document is copied aside"
        );
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn restore_without_a_file_uses_the_newest_backup() {
        let dir = data_dir("restore-newest");
        let store = StateStore::in_data_dir(&dir);
        assert!(store.restore(None).is_err(), "nothing to restore from yet");
        let mut state = ProductState::default();
        state.conversation_mut("c1").title = "backed up".into();
        let path = store.write_daily_backup(&state, 86_400_000).unwrap();
        assert_eq!(store.newest_backup(), Some(path));
        let (restored, _) = store.restore(None).unwrap();
        assert_eq!(restored, state);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn concurrent_saves_never_fail_or_regress_the_revision() {
        let dir = data_dir("concurrent");
        let store = std::sync::Arc::new(StateStore::in_data_dir(&dir));
        let handles: Vec<_> = (1..=32u64)
            .map(|revision| {
                let store = std::sync::Arc::clone(&store);
                std::thread::spawn(move || {
                    let state = ProductState {
                        revision,
                        ..ProductState::default()
                    };
                    store.save(&state)
                })
            })
            .collect();
        for handle in handles {
            handle.join().unwrap().unwrap();
        }
        assert_eq!(store.load().unwrap().revision, 32);
        let leftovers = fs::read_dir(&dir)
            .unwrap()
            .filter_map(Result::ok)
            .filter(|entry| entry.file_name().to_string_lossy().ends_with(".tmp"))
            .count();
        assert_eq!(leftovers, 0);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn save_leaves_no_temporary_file_behind() {
        let dir = data_dir("atomic");
        let store = StateStore::in_data_dir(&dir);
        store.save(&ProductState::default()).unwrap();
        assert!(store.path().exists());
        assert!(!dir.join(STATE_TEMP_FILE).exists());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn daily_backups_are_written_once_and_capped() {
        let dir = data_dir("backups");
        let store = StateStore::in_data_dir(&dir);
        let state = ProductState::default();
        let first = store
            .write_daily_backup(&state, 86_400_000)
            .expect("a backup");
        assert!(first.exists());
        assert!(
            store
                .write_daily_backup(&state, 86_400_000 + 1000)
                .is_none(),
            "the same day is written only once"
        );
        for day in 2..=(BACKUP_KEEP as u64 + 3) {
            store.write_daily_backup(&state, day * 86_400_000);
        }
        let files = fs::read_dir(dir.join(BACKUP_DIR))
            .unwrap()
            .filter_map(Result::ok)
            .count();
        assert_eq!(files, BACKUP_KEEP);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn health_reports_backups_and_quarantined_documents() {
        let dir = data_dir("health");
        let store = StateStore::in_data_dir(&dir);
        assert!(!store.document_exists());
        store.save(&ProductState::default()).unwrap();
        assert!(store.document_exists());
        assert_eq!(store.data_dir(), Some(dir.as_path()));
        assert_eq!(store.backup_status()["count"], json!(0));
        assert!(store.backup_status()["ageSeconds"].is_null());
        store
            .write_daily_backup(&ProductState::default(), 86_400_000)
            .unwrap();
        assert_eq!(store.backup_status()["count"], json!(1));
        assert!(store.backup_status()["ageSeconds"].as_u64().is_some());
        fs::write(dir.join("product-state.corrupt-1.json"), b"{ partial").unwrap();
        assert_eq!(store.quarantined_count(), 1);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_failed_write_is_visible_until_the_next_successful_one() {
        let dir = data_dir("persist-health");
        let store = StateStore::in_data_dir(&dir);
        persist(&store, &ProductState::default());
        assert_eq!(last_persist_error(), None);
        // A directory where the document should be makes the atomic write fail.
        fs::remove_file(store.path()).unwrap();
        fs::create_dir(store.path()).unwrap();
        persist(&store, &ProductState::default());
        assert!(last_persist_error().is_some());
        fs::remove_dir(store.path()).unwrap();
        persist(&store, &ProductState::default());
        assert_eq!(last_persist_error(), None);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn default_data_dir_prefers_explicit_configuration() {
        assert!(default_data_dir().is_absolute());
        assert_eq!(
            sessions_dir(Path::new("/tmp/x")),
            Path::new("/tmp/x/sessions")
        );
    }
}
