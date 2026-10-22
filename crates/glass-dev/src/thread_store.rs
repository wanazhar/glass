//! Private, versioned project-local identities for resident conversations.

use crate::development::{DevelopmentError, DevelopmentResult};
use serde::{Deserialize, Serialize};
use std::fs::{self, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

const VERSION: u32 = 1;
const MAX_BYTES: u64 = 256 * 1024;
const MAX_THREADS: usize = 256;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct ThreadRecord {
    pub thread_id: String,
    pub title: String,
    pub role: String,
    pub created_at_ms: u128,
    pub runs: Vec<RunRecord>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct RunRecord {
    pub run_id: String,
    pub engine: String,
    pub core_session_id: Option<String>,
    pub classification: RunClassification,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum RunClassification {
    Queued,
    Starting,
    RecoveryPending,
    Completed,
    Failed,
    Cancelled,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Document {
    version: u32,
    project_identity: ProjectIdentity,
    threads: Vec<ThreadRecord>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ProjectIdentity {
    canonical_root: PathBuf,
    #[cfg(unix)]
    device: u64,
    #[cfg(unix)]
    inode: u64,
}

#[derive(Debug, Clone)]
pub(crate) struct ThreadStore {
    root: PathBuf,
    path: PathBuf,
    identity: ProjectIdentity,
}

impl ThreadStore {
    pub(crate) fn open(root: &Path) -> DevelopmentResult<Self> {
        let root = fs::canonicalize(root)?;
        let metadata = fs::metadata(&root)?;
        if !metadata.is_dir() {
            return Err(config("thread store project root is not a directory"));
        }
        #[cfg(unix)]
        let identity = {
            use std::os::unix::fs::MetadataExt;
            ProjectIdentity {
                canonical_root: root.clone(),
                device: metadata.dev(),
                inode: metadata.ino(),
            }
        };
        #[cfg(not(unix))]
        let identity = ProjectIdentity {
            canonical_root: root.clone(),
        };
        let path = root.join(".glass/threads/registry.json");
        let store = Self {
            root,
            path,
            identity,
        };
        let _ = store.load()?;
        Ok(store)
    }

    pub(crate) fn list(&self) -> DevelopmentResult<Vec<ThreadRecord>> {
        Ok(self.load()?.threads)
    }

    pub(crate) fn create(&self, thread: ThreadRecord) -> DevelopmentResult<()> {
        validate_thread(&thread)?;
        self.mutate(|doc| {
            let mut candidate_runs = std::collections::BTreeSet::new();
            if thread
                .runs
                .iter()
                .any(|run| !candidate_runs.insert(run.run_id.as_str()))
            {
                return Err(config("duplicate run identifier in new thread"));
            }
            if doc.threads.len() >= MAX_THREADS {
                return Err(config("thread registry reached its limit"));
            }
            if doc
                .threads
                .iter()
                .any(|existing| existing.thread_id == thread.thread_id)
                || thread.runs.iter().any(|candidate| {
                    doc.threads
                        .iter()
                        .flat_map(|t| &t.runs)
                        .any(|run| run.run_id == candidate.run_id)
                })
            {
                return Err(config("duplicate thread or run identifier"));
            }
            doc.threads.push(thread);
            Ok(())
        })
    }

    pub(crate) fn append_run(&self, thread_id: &str, run: RunRecord) -> DevelopmentResult<()> {
        validate_id(&run.run_id, "run")?;
        validate_run(&run)?;
        self.mutate(|doc| {
            if doc
                .threads
                .iter()
                .flat_map(|t| &t.runs)
                .any(|existing| existing.run_id == run.run_id)
            {
                return Err(config("duplicate run identifier"));
            }
            let thread = doc
                .threads
                .iter_mut()
                .find(|thread| thread.thread_id == thread_id)
                .ok_or_else(|| config("thread disappeared before run persistence"))?;
            if thread.runs.len() >= 32 {
                return Err(config("thread run history reached its limit"));
            }
            thread.runs.push(run);
            Ok(())
        })
    }

    pub(crate) fn set_session(
        &self,
        thread_id: &str,
        run_id: &str,
        session_id: &str,
    ) -> DevelopmentResult<()> {
        if session_id.is_empty()
            || session_id.len() > 256
            || session_id.chars().any(char::is_control)
        {
            return Err(config("Core returned an invalid session reference"));
        }
        self.mutate(|doc| {
            let run = doc
                .threads
                .iter_mut()
                .find(|t| t.thread_id == thread_id)
                .and_then(|t| t.runs.iter_mut().find(|r| r.run_id == run_id))
                .ok_or_else(|| config("thread run disappeared before session persistence"))?;
            if run
                .core_session_id
                .as_deref()
                .is_some_and(|existing| existing != session_id)
            {
                return Err(config(
                    "Core session reference cannot be replaced for an existing run",
                ));
            }
            run.core_session_id = Some(session_id.to_owned());
            run.classification = RunClassification::RecoveryPending;
            Ok(())
        })
    }

    pub(crate) fn classify_run(
        &self,
        thread_id: &str,
        run_id: &str,
        classification: RunClassification,
    ) -> DevelopmentResult<()> {
        self.mutate(|doc| {
            let run = doc
                .threads
                .iter_mut()
                .find(|t| t.thread_id == thread_id)
                .and_then(|t| t.runs.iter_mut().find(|r| r.run_id == run_id))
                .ok_or_else(|| config("thread run disappeared before lifecycle persistence"))?;
            run.classification = classification;
            Ok(())
        })
    }

    fn mutate(
        &self,
        f: impl FnOnce(&mut Document) -> DevelopmentResult<()>,
    ) -> DevelopmentResult<()> {
        ensure_store_directories(&self.root)?;
        let lock_path = self.path.with_extension("lock");
        let mut lock_options = OpenOptions::new();
        lock_options
            .create(true)
            .truncate(false)
            .read(true)
            .write(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            lock_options.mode(0o600);
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            lock_options.custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC);
        }
        let lock = lock_options.open(lock_path)?;
        if !lock.metadata()?.is_file() {
            return Err(config("thread registry lock is not a regular file"));
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            if lock.metadata()?.nlink() != 1 {
                return Err(config(
                    "thread registry lock has an unexpected hard-link count",
                ));
            }
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            lock.set_permissions(fs::Permissions::from_mode(0o600))?;
        }
        fs2::FileExt::lock_exclusive(&lock)?;
        let result = (|| {
            let mut doc = self.load()?;
            f(&mut doc)?;
            self.save(&doc)
        })();
        let _ = fs2::FileExt::unlock(&lock);
        result
    }

    fn load(&self) -> DevelopmentResult<Document> {
        validate_store_directories(&self.root)?;
        let metadata = match fs::symlink_metadata(&self.path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(Document {
                    version: VERSION,
                    project_identity: self.identity.clone(),
                    threads: Vec::new(),
                });
            }
            Err(error) => return Err(error.into()),
        };
        if !metadata.file_type().is_file() || metadata.len() > MAX_BYTES {
            return Err(config("thread registry is not a bounded regular file"));
        }
        let mut bytes = Vec::with_capacity(metadata.len() as usize);
        let mut open_options = OpenOptions::new();
        open_options.read(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            open_options.custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC);
        }
        open_options
            .open(&self.path)?
            .take(MAX_BYTES + 1)
            .read_to_end(&mut bytes)?;
        if bytes.len() as u64 > MAX_BYTES {
            return Err(config("thread registry exceeds its size limit"));
        }
        let doc: Document = serde_json::from_slice(&bytes)
            .map_err(|e| config(format!("invalid thread registry: {e}")))?;
        if doc.version != VERSION {
            return Err(config(format!(
                "unsupported thread registry version {}",
                doc.version
            )));
        }
        if doc.project_identity != self.identity {
            return Err(config(
                "thread registry belongs to a different project identity",
            ));
        }
        if doc.threads.len() > MAX_THREADS {
            return Err(config("thread registry exceeds its record limit"));
        }
        let mut thread_ids = std::collections::BTreeSet::new();
        let mut run_ids = std::collections::BTreeSet::new();
        for thread in &doc.threads {
            validate_thread(thread)?;
            if !thread_ids.insert(thread.thread_id.as_str()) {
                return Err(config("duplicate thread identifier in registry"));
            }
            for run in &thread.runs {
                if !run_ids.insert(run.run_id.as_str()) {
                    return Err(config("duplicate run identifier in registry"));
                }
            }
        }
        Ok(doc)
    }

    fn save(&self, doc: &Document) -> DevelopmentResult<()> {
        let parent = self.path.parent().unwrap_or(&self.root);
        let bytes = serde_json::to_vec_pretty(doc)?;
        if bytes.len() as u64 > MAX_BYTES {
            return Err(config("thread registry exceeds its size limit"));
        }
        let (temporary, mut file) = create_temporary(&self.path)?;
        let _cleanup = TemporaryFile(temporary.clone());
        file.write_all(&bytes)?;
        file.sync_all()?;
        replace_file(&temporary, &self.path)?;
        #[cfg(unix)]
        {
            OpenOptions::new().read(true).open(parent)?.sync_all()?;
        }
        Ok(())
    }
}

fn create_temporary(path: &Path) -> DevelopmentResult<(PathBuf, fs::File)> {
    for _ in 0..8 {
        let suffix = random_id("tmp")?;
        let temporary = path.with_extension(format!("tmp-{}-{suffix}", std::process::id()));
        let mut options = OpenOptions::new();
        options.create_new(true).write(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options
                .mode(0o600)
                .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC);
        }
        match options.open(&temporary) {
            Ok(file) => return Ok((temporary, file)),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error.into()),
        }
    }
    Err(config(
        "could not allocate a unique temporary thread-registry file",
    ))
}

fn validate_store_directories(root: &Path) -> DevelopmentResult<()> {
    let glass = root.join(".glass");
    let threads = glass.join("threads");
    for path in [&glass, &threads] {
        match fs::symlink_metadata(path) {
            Ok(metadata) if metadata.file_type().is_dir() => {}
            Ok(_) => return Err(config("thread registry parent must be a real directory")),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(error) => return Err(error.into()),
        }
    }
    Ok(())
}

fn ensure_store_directories(root: &Path) -> DevelopmentResult<()> {
    let glass = root.join(".glass");
    let threads = glass.join("threads");
    for path in [&glass, &threads] {
        match fs::symlink_metadata(path) {
            Ok(metadata) if metadata.file_type().is_dir() => {}
            Ok(_) => return Err(config("thread registry parent must be a real directory")),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                match fs::create_dir(path) {
                    Ok(()) => {
                        #[cfg(unix)]
                        {
                            use std::os::unix::fs::PermissionsExt;
                            fs::set_permissions(path, fs::Permissions::from_mode(0o700))?;
                        }
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
                    Err(error) => return Err(error.into()),
                }
                let metadata = fs::symlink_metadata(path)?;
                if !metadata.file_type().is_dir() {
                    return Err(config("thread registry parent must be a real directory"));
                }
            }
            Err(error) => return Err(error.into()),
        }
    }
    Ok(())
}

#[cfg(windows)]
fn replace_file(source: &Path, destination: &Path) -> DevelopmentResult<()> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::{
        MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH, MoveFileExW,
    };
    let source: Vec<u16> = source.as_os_str().encode_wide().chain(Some(0)).collect();
    let destination: Vec<u16> = destination
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect();
    // SAFETY: both paths are valid NUL-terminated UTF-16 buffers for the call.
    let result = unsafe {
        MoveFileExW(
            source.as_ptr(),
            destination.as_ptr(),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    };
    if result == 0 {
        return Err(std::io::Error::last_os_error().into());
    }
    Ok(())
}

#[cfg(not(windows))]
fn replace_file(source: &Path, destination: &Path) -> DevelopmentResult<()> {
    fs::rename(source, destination)?;
    Ok(())
}

struct TemporaryFile(PathBuf);
impl Drop for TemporaryFile {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}

pub(crate) fn random_id(prefix: &str) -> DevelopmentResult<String> {
    let mut bytes = [0u8; 16];
    getrandom::fill(&mut bytes)
        .map_err(|e| config(format!("secure random ID generation failed: {e}")))?;
    Ok(format!(
        "{prefix}-{}",
        bytes.iter().map(|b| format!("{b:02x}")).collect::<String>()
    ))
}

fn validate_id(id: &str, prefix: &str) -> DevelopmentResult<()> {
    if id.len() != prefix.len() + 33
        || !id.starts_with(&format!("{prefix}-"))
        || !id[prefix.len() + 1..]
            .bytes()
            .all(|b| b.is_ascii_hexdigit())
    {
        return Err(config(format!(
            "invalid {prefix} identifier in thread registry"
        )));
    }
    Ok(())
}

fn validate_thread(thread: &ThreadRecord) -> DevelopmentResult<()> {
    validate_id(&thread.thread_id, "thread")?;
    if thread.title.len() > 256
        || thread.role.len() > 128
        || thread.title.chars().any(char::is_control)
        || thread.role.chars().any(char::is_control)
        || thread.runs.is_empty()
        || thread.runs.len() > 32
    {
        return Err(config("thread registry record exceeds its field limits"));
    }
    for run in &thread.runs {
        validate_id(&run.run_id, "run")?;
        validate_run(run)?;
    }
    Ok(())
}

fn validate_run(run: &RunRecord) -> DevelopmentResult<()> {
    if run.engine.is_empty()
        || run.engine.len() > 64
        || run.engine.chars().any(char::is_control)
        || run
            .core_session_id
            .as_ref()
            .is_some_and(|s| s.is_empty() || s.len() > 256 || s.chars().any(char::is_control))
    {
        return Err(config("thread registry run is invalid"));
    }
    Ok(())
}
fn config(message: impl Into<String>) -> DevelopmentError {
    DevelopmentError::Config(message.into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};
    fn root() -> PathBuf {
        let n = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let p = std::env::temp_dir().join(format!("glass-thread-store-{n}"));
        fs::create_dir_all(&p).unwrap();
        p
    }
    fn record() -> ThreadRecord {
        ThreadRecord {
            thread_id: random_id("thread").unwrap(),
            title: "Build feature".into(),
            role: "builder".into(),
            created_at_ms: 1,
            runs: vec![RunRecord {
                run_id: random_id("run").unwrap(),
                engine: "glass-core".into(),
                core_session_id: None,
                classification: RunClassification::RecoveryPending,
            }],
        }
    }
    #[test]
    fn round_trip_and_reopen_preserves_durable_ids() {
        let root = root();
        let store = ThreadStore::open(&root).unwrap();
        let row = record();
        store.create(row.clone()).unwrap();
        let again = ThreadStore::open(&root).unwrap();
        assert_eq!(again.list().unwrap(), vec![row]);
        let bytes = fs::read(root.join(".glass/threads/registry.json")).unwrap();
        let text = String::from_utf8(bytes).unwrap();
        assert!(!text.contains("agent-"));
        assert!(!text.contains("job"));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn concurrent_mutations_keep_every_thread_record() {
        let root = root();
        let store = ThreadStore::open(&root).unwrap();
        let writers: Vec<_> = (0..8)
            .map(|index| {
                let store = store.clone();
                std::thread::spawn(move || {
                    let mut row = record();
                    row.title = format!("writer-{index}");
                    store.create(row).unwrap();
                })
            })
            .collect();
        for writer in writers {
            writer.join().unwrap();
        }
        assert_eq!(store.list().unwrap().len(), 8);
        let _ = fs::remove_dir_all(root);
    }
    #[test]
    fn rejects_unknown_schema_without_overwriting() {
        let root = root();
        let store = ThreadStore::open(&root).unwrap();
        let row = record();
        store.create(row).unwrap();
        let path = root.join(".glass/threads/registry.json");
        let before = fs::read(&path).unwrap();
        let mut value: serde_json::Value = serde_json::from_slice(&before).unwrap();
        value["version"] = serde_json::json!(99);
        fs::write(&path, serde_json::to_vec(&value).unwrap()).unwrap();
        let corrupt = fs::read(&path).unwrap();
        assert!(ThreadStore::open(&root).is_err());
        assert_eq!(fs::read(path).unwrap(), corrupt);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn rejects_project_identity_mismatch_and_preserves_failed_mutation() {
        let root = root();
        let store = ThreadStore::open(&root).unwrap();
        let first = record();
        store.create(first).unwrap();
        let path = root.join(".glass/threads/registry.json");
        let before = fs::read(&path).unwrap();
        let other = root.join("other");
        fs::create_dir_all(&other).unwrap();
        let other_store = ThreadStore::open(&other).unwrap();
        fs::create_dir_all(other_store.path.parent().unwrap()).unwrap();
        fs::copy(&path, &other_store.path).unwrap();
        assert!(other_store.list().is_err());
        let mut oversized = record();
        oversized.title = "x".repeat(MAX_BYTES as usize);
        assert!(store.create(oversized).is_err());
        assert_eq!(fs::read(&path).unwrap(), before);
        let _ = fs::remove_dir_all(root);
    }

    #[cfg(unix)]
    #[test]
    fn registry_and_lock_are_private() {
        use std::os::unix::fs::PermissionsExt;
        let root = root();
        let store = ThreadStore::open(&root).unwrap();
        store.create(record()).unwrap();
        let registry_mode = fs::metadata(&store.path).unwrap().permissions().mode() & 0o777;
        let lock_mode = fs::metadata(store.path.with_extension("lock"))
            .unwrap()
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(registry_mode, 0o600);
        assert_eq!(lock_mode, 0o600);
        let _ = fs::remove_dir_all(root);
    }

    #[cfg(unix)]
    #[test]
    fn dangling_registry_symlink_is_not_treated_as_an_empty_store() {
        use std::os::unix::fs::symlink;
        let root = root();
        let store = ThreadStore::open(&root).unwrap();
        fs::create_dir_all(store.path.parent().unwrap()).unwrap();
        symlink(root.join("missing-target"), &store.path).unwrap();
        assert!(ThreadStore::open(&root).is_err());
        let _ = fs::remove_dir_all(root);
    }

    #[cfg(unix)]
    #[test]
    fn lock_symlink_is_rejected_without_touching_its_target() {
        use std::os::unix::fs::symlink;
        let root = root();
        let store = ThreadStore::open(&root).unwrap();
        store.create(record()).unwrap();
        let bytes = fs::read(&store.path).unwrap();
        let lock = store.path.with_extension("lock");
        fs::remove_file(&lock).unwrap();
        let target = root.join("outside");
        fs::write(&target, b"leave me alone").unwrap();
        symlink(&target, &lock).unwrap();
        assert!(store.create(record()).is_err());
        assert_eq!(fs::read(target).unwrap(), b"leave me alone");
        assert_eq!(fs::read(&store.path).unwrap(), bytes);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn initialized_session_becomes_recovery_pending() {
        let root = root();
        let store = ThreadStore::open(&root).unwrap();
        let mut row = record();
        row.runs[0].classification = RunClassification::Starting;
        let thread_id = row.thread_id.clone();
        let run_id = row.runs[0].run_id.clone();
        store.create(row).unwrap();
        store
            .set_session(&thread_id, &run_id, "core-session-from-runtime")
            .unwrap();
        let reopened = ThreadStore::open(&root).unwrap();
        let row = reopened.list().unwrap().remove(0);
        assert_eq!(
            row.runs[0].core_session_id.as_deref(),
            Some("core-session-from-runtime")
        );
        assert_eq!(
            row.runs[0].classification,
            RunClassification::RecoveryPending
        );
        let _ = fs::remove_dir_all(root);
    }
}
