//! Content-addressed blobs (task six-wave-d-001, OMP blob-artifacts).
//!
//! Bash spill files and image payloads are stored under the data dir as
//! `blobs/<sha256 hex>` with a `blob:<sha256>` reference the record keeps.
//! Writes are atomic (`tmp` then rename) and deduplicated: writing the same
//! content twice stores it once and both callers get the same address.
//!
//! Garbage control: `sweep(&data_dir)` scans foreground and managed-worker logs
//! on demand (`gc blobs sweep [--json]`). Recent blobs are retained to protect
//! the gap between storing output and appending its referencing record.

use ring::digest::{Context, SHA256};
#[cfg(test)]
use std::io::Write;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
#[cfg(test)]
use std::sync::atomic::{AtomicU64, Ordering};

/// Blob store root. Same isolation as the rest of the data tree.
pub fn dir() -> PathBuf {
    crate::config::data_dir().join("blobs")
}

/// Reference form kept in records: `blob:<sha256-hex>`.
pub fn reference(hex: &str) -> String {
    format!("blob:{hex}")
}

/// Lowercase hex without a third-party hex crate.
fn encode_hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        out.push(HEX[(b >> 4) as usize] as char);
        out.push(HEX[(b & 0x0f) as usize] as char);
    }
    out
}

/// Extract the hex digest out of a `blob:<sha256>` reference. `None` for
/// anything else — paths stay paths, this never guesses.
pub fn hex_of(reference: &str) -> Option<&str> {
    reference
        .strip_prefix("blob:")
        .filter(|hex| hex.len() == 64 && hex.chars().all(|c| c.is_ascii_hexdigit()))
}

/// Atomically store content, returning its stable reference and path.
#[cfg(test)]
fn put(content: &[u8]) -> std::io::Result<(String, PathBuf)> {
    put_reader(content)
}

/// Consume a completed private spill without copying its bytes a second time.
/// The caller retains the original path if hashing or the same-filesystem move fails.
pub fn adopt_file(path: &Path) -> std::io::Result<(String, PathBuf)> {
    use std::os::unix::fs::OpenOptionsExt;
    let mut file = std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW)
        .open(path)?;
    if !file.metadata()?.is_file() {
        return Err(std::io::Error::other("Spill is not a regular file"));
    }
    let mut digest = Context::new(&SHA256);
    let mut buffer = [0u8; 32 * 1024];
    loop {
        let size = match file.read(&mut buffer) {
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
            result => result?,
        };
        if size == 0 {
            break;
        }
        digest.update(&buffer[..size]);
    }
    let hex = encode_hex(digest.finish().as_ref());
    let dir = dir();
    std::fs::create_dir_all(&dir)?;
    let stored = dir.join(&hex);
    // Refresh the grace window before publishing the new reference. Replacing
    // the destination atomically also repairs a corrupt existing digest path;
    // concurrent identical stores still leave exactly one correct file.
    file.set_modified(std::time::SystemTime::now())?;
    if path != stored {
        std::fs::rename(path, &stored)?;
    }
    Ok((reference(&hex), stored))
}

#[cfg(test)]
fn put_reader(mut reader: impl Read) -> std::io::Result<(String, PathBuf)> {
    use std::os::unix::fs::OpenOptionsExt;
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let dir = dir();
    std::fs::create_dir_all(&dir)?;
    let (tmp, mut file) = loop {
        let tmp = dir.join(format!(
            ".tmp-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&tmp)
        {
            Ok(file) => break (tmp, file),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error),
        }
    };
    let result = (|| {
        let mut digest = Context::new(&SHA256);
        let mut buffer = [0u8; 32 * 1024];
        loop {
            let size = match reader.read(&mut buffer) {
                Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
                value => value?,
            };
            if size == 0 {
                break;
            }
            digest.update(&buffer[..size]);
            file.write_all(&buffer[..size])?;
        }
        file.flush()?;
        let hex = encode_hex(digest.finish().as_ref());
        let path = dir.join(&hex);
        // Concurrent identical stores have unique temporaries and identical
        // final content, so neither caller loses a valid reference.
        if !path.exists() {
            std::fs::rename(&tmp, &path)?;
        }
        Ok((reference(&hex), path))
    })();
    let _ = std::fs::remove_file(&tmp);
    result
}

/// Read only the requested range; oversized offsets and lengths cannot wrap.
pub fn get(reference: &str, offset: usize, length: Option<usize>) -> std::io::Result<Vec<u8>> {
    let hex = hex_of(reference).ok_or_else(|| {
        std::io::Error::new(std::io::ErrorKind::InvalidInput, "not a blob reference")
    })?;
    let mut file = std::fs::File::open(dir().join(hex))?;
    let size = file.metadata()?.len();
    let start = (offset as u64).min(size);
    file.seek(SeekFrom::Start(start))?;
    let count = length
        .map(|n| n as u64)
        .unwrap_or(u64::MAX)
        .min(size - start);
    let mut data = Vec::new();
    file.take(count).read_to_end(&mut data)?;
    Ok(data)
}

/// One sweep outcome, in count form so `--json` stays introspectable.
#[derive(Debug, Default, serde::Serialize)]
pub struct SweepReport {
    pub scanned: usize,
    pub kept_referenced: usize,
    pub kept_recent: usize,
    pub removed: usize,
    pub errors: usize,
}

/// Mtime guard: a blob written inside this window may belong to a session
/// whose referencing record has not been flushed yet (put → append is not
/// atomic across the log fsync) or to a live running turn. Never delete it.
const RECENT_SECS: u64 = 300;

fn referenced_hexes(data_dir: &Path) -> Result<std::collections::HashSet<String>, String> {
    fn scan(bytes: &[u8], eof: bool, refs: &mut std::collections::HashSet<String>) {
        for (at, prefix) in bytes.windows(5).enumerate() {
            if prefix != b"blob:" {
                continue;
            }
            let end = at + 69;
            if end > bytes.len() || (end == bytes.len() && !eof) {
                continue;
            }
            let hex = &bytes[at + 5..end];
            if hex.iter().all(u8::is_ascii_hexdigit)
                && bytes.get(end).is_none_or(|byte| !byte.is_ascii_hexdigit())
            {
                refs.insert(String::from_utf8_lossy(hex).to_ascii_lowercase());
            }
        }
    }
    let mut refs = std::collections::HashSet::new();
    let mut pending = vec![(data_dir.to_path_buf(), 0usize)];
    let mut visited = 0usize;
    while let Some((directory, depth)) = pending.pop() {
        for entry in std::fs::read_dir(&directory).map_err(|e| e.to_string())? {
            let entry = entry.map_err(|e| e.to_string())?;
            visited += 1;
            if visited > 100_000 {
                return Err("Blob reference scan exceeds 100000 entries; nothing removed".into());
            }
            let path = entry.path();
            let kind = entry.file_type().map_err(|e| e.to_string())?;
            if kind.is_symlink()
                && (depth > 0 || entry.file_name() == "agents" || entry.file_name() == "subagents")
            {
                return Err("Symlink in managed session storage; nothing removed".into());
            }
            if kind.is_dir() {
                let name = entry.file_name();
                if (depth == 0 && name != "agents" && name != "subagents") || name == "worktrees" {
                    continue;
                }
                if depth >= 16 {
                    return Err(
                        "Blob reference scan exceeds 16 directory levels; nothing removed".into(),
                    );
                }
                pending.push((path, depth + 1));
                continue;
            }
            if path.extension().is_none_or(|ext| ext != "jsonl") {
                continue;
            }
            if !kind.is_file() {
                return Err("Non-regular session log; nothing removed".into());
            }
            use std::os::unix::fs::OpenOptionsExt;
            let mut file = std::fs::OpenOptions::new()
                .read(true)
                .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
                .open(&path)
                .map_err(|e| e.to_string())?;
            if !file.metadata().map_err(|e| e.to_string())?.is_file() {
                return Err("Non-regular session log; nothing removed".into());
            }
            let mut buffer = [0u8; 32 * 1024];
            let mut carry = Vec::new();
            loop {
                let size = match file.read(&mut buffer) {
                    Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
                    result => result.map_err(|e| e.to_string())?,
                };
                carry.extend_from_slice(&buffer[..size]);
                scan(&carry, size == 0, &mut refs);
                if size == 0 {
                    break;
                }
                if carry.len() > 69 {
                    carry.drain(..carry.len() - 69);
                }
            }
        }
    }
    Ok(refs)
}

/// Delete blobs under `blobs/` that no session log references. Blobs
/// modified within [`RECENT_SECS`] are skipped (kept_recent), as are
/// referenced ones; removal failures are counted, never fatal. A tmp file
/// (`/.tmp-…`) never matches a hex name and is only removed when old — it
/// precedes rename andrename failure leaves it, so treat age as the signal.
pub fn sweep(data_dir: &Path) -> Result<SweepReport, String> {
    let mut report = SweepReport::default();
    let refs = referenced_hexes(data_dir)?;
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let blob_dir = data_dir.join("blobs");
    let entries =
        std::fs::read_dir(&blob_dir).map_err(|e| format!("{}: {e}", blob_dir.display()))?;
    for entry in entries {
        let entry = entry.map_err(|e| {
            report.errors += 1;
            e.to_string()
        })?;
        let path = entry.path();
        report.scanned += 1;
        let name = match path.file_name().and_then(|n| n.to_str()) {
            Some(n) => n.to_string(),
            None => {
                report.errors += 1;
                continue;
            }
        };
        let mtime_secs = entry
            .metadata()
            .ok()
            .and_then(|m| m.modified().ok())
            .and_then(|m| m.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_secs());
        // Guard window first: any recent file stays, referenced or not.
        if mtime_secs.is_some_and(|t| now.saturating_sub(t) < RECENT_SECS) {
            report.kept_recent += 1;
            continue;
        }
        if hex_valid(&name) {
            if refs.contains(&name) {
                report.kept_referenced += 1;
            } else if std::fs::remove_file(&path).is_ok() {
                report.removed += 1;
            } else {
                report.errors += 1;
            }
        } else if name.starts_with(".tmp-") {
            // Only reachable when older than the guard window.
            let _ = std::fs::remove_file(&path);
            report.removed += 1;
        } else {
            // Unknown layout: leave it alone and surface it.
            report.errors += 1;
        }
    }
    Ok(report)
}

fn hex_valid(name: &str) -> bool {
    hex_of(&format!("blob:{name}")).is_some()
}

#[cfg(test)]
mod sweep_tests {
    use super::*;

    /// Isolate from the shared data dir: sweep takes the dir as an argument,
    /// but `put`/`get` still write to the real store, so tests only exercise
    /// files sweep owns.
    fn sweep_dir(name: &str) -> PathBuf {
        let base = std::env::temp_dir().join(format!("gc-sweep-{}-{}", std::process::id(), name));
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(base.join("blobs")).unwrap();
        base
    }

    fn age(path: &Path, secs: u64) {
        let old = std::time::SystemTime::now() - std::time::Duration::from_secs(secs);
        let f = std::fs::File::options().append(true).open(path).unwrap();
        f.set_modified(old).unwrap();
    }

    #[test]
    fn sweep_preserves_nested_worker_references_across_read_boundaries() {
        let base = sweep_dir("nested");
        let name = "d".repeat(64);
        let blob = base.join("blobs").join(&name);
        std::fs::write(&blob, b"worker output").unwrap();
        age(&blob, 1000);
        let sessions = base.join("agents/owner/sessions");
        std::fs::create_dir_all(&sessions).unwrap();
        let text = format!("{}blob:{name}\"", " ".repeat(32 * 1024 - 20));
        std::fs::write(sessions.join("worker.jsonl"), text).unwrap();
        let report = sweep(&base).unwrap();
        assert_eq!(report.kept_referenced, 1);
        assert_eq!(report.removed, 0);
        assert!(blob.exists());
        let extra = "e".repeat(64);
        std::fs::create_dir_all(base.join("subagents")).unwrap();
        std::fs::write(base.join("subagents/child.jsonl"), format!("blob:{extra}")).unwrap();
        assert!(referenced_hexes(&base).unwrap().contains(&extra));
    }

    #[test]
    fn sweep_refuses_symlinked_logs_before_deleting_blobs() {
        let base = sweep_dir("symlink");
        let blob = base.join("blobs").join("f".repeat(64));
        std::fs::write(&blob, b"protected").unwrap();
        age(&blob, 1000);
        std::os::unix::fs::symlink("missing", base.join("worker.jsonl")).unwrap();
        assert!(sweep(&base).is_err());
        assert!(blob.exists());
    }

    #[test]
    fn sweep_removes_unreferenced_keeps_referenced_and_recent() {
        let base = sweep_dir("basic");
        let dir = base.join("blobs");
        let referenced_name = "a".repeat(64);
        let stale_name = "b".repeat(64);
        let fresh_name = "c".repeat(64);
        for (name, aged) in [
            (&referenced_name, 1000),
            (&stale_name, 1000),
            (&fresh_name, 0),
        ] {
            let p = dir.join(name);
            std::fs::write(&p, b"x").unwrap();
            if aged > 0 {
                age(&p, aged);
            }
        }
        // The referenced one is named by a session log.
        std::fs::write(
            base.join("sess.jsonl"),
            format!("{{\"spill\":\"blob:{}\"}}\n", referenced_name),
        )
        .unwrap();
        // Aged tmp file goes; recent tmp stays (guard window).
        let tmp = dir.join(format!(".tmp-1-{}", stale_name));
        std::fs::write(&tmp, b"partial").unwrap();
        age(&tmp, 1000);
        let tmp2 = dir.join(".tmp-2-recent");
        std::fs::write(&tmp2, b"partial").unwrap();

        let report = sweep(&base).unwrap();
        assert_eq!(report.removed, 2, "stale unreferenced + aged tmp");
        assert_eq!(report.kept_referenced, 1);
        assert_eq!(report.kept_recent, 2, "fresh blob + recent tmp");
        assert_eq!(report.errors, 0);
        assert!(!dir.join(&stale_name).exists());
        assert!(dir.join(&referenced_name).exists());
        assert!(dir.join(&fresh_name).exists(), "recent blob survives");
        assert!(!tmp.exists());
        assert!(tmp2.exists(), "recent tmp survives (in-flight put)");
    }

    #[test]
    fn sweep_counts_unknown_layout_as_errors_without_deleting() {
        let base = sweep_dir("unknown");
        let dir = base.join("blobs");
        let p = dir.join("not-a-blob");
        std::fs::write(&p, b"?").unwrap();
        age(&p, 1000);
        let report = sweep(&base).unwrap();
        assert_eq!(report.errors, 1);
        assert_eq!(report.removed, 0);
        assert!(p.exists(), "unknown layout is never deleted");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adopted_spills_move_once_and_deduplicate() {
        use std::os::unix::fs::MetadataExt;
        let base = std::env::temp_dir().join(format!(
            "gc-adopt-{}-{}",
            std::process::id(),
            crate::partial::now()
        ));
        std::fs::create_dir_all(&base).unwrap();
        let first = base.join("first.log");
        let second = base.join("second.log");
        let body = format!("Unique adoption fixture: {}", base.display());
        std::fs::write(&first, &body).unwrap();
        std::fs::write(&second, &body).unwrap();
        let before = std::fs::metadata(&first).unwrap();
        let (reference, stored) = adopt_file(&first).unwrap();
        assert!(!first.exists());
        assert_eq!(std::fs::metadata(&stored).unwrap().ino(), before.ino());
        assert_eq!(get(&reference, 0, None).unwrap(), body.as_bytes());
        // A preexisting digest path must not silently replace verified output
        // with corrupt bytes merely because its filename looks correct.
        std::fs::write(&stored, b"corrupt existing blob").unwrap();
        assert_eq!(adopt_file(&second).unwrap(), (reference.clone(), stored));
        assert_eq!(get(&reference, 0, None).unwrap(), body.as_bytes());
        assert!(!second.exists());
        std::fs::remove_dir(&base).unwrap();
    }

    #[test]
    fn put_is_deduplicated_and_roundtrips() {
        let content = b"blob body \xf0\x9f\x8c\x8d tail";
        let (r1, p1) = put(content).unwrap();
        let (r2, p2) = put(content).unwrap();
        assert_eq!(r1, r2);
        assert_eq!(p1, p2, "identical content stores once");
        assert!(hex_of(&r1).is_some());
        assert_eq!(get(&r1, 0, None).unwrap(), content);
        // Ranged read.
        assert_eq!(get(&r1, 4, Some(4)).unwrap(), &content[4..8]);
    }

    #[test]
    fn ranged_reads_do_not_wrap_oversized_lengths() {
        let (reference, _) = put(b"range overflow regression").unwrap();
        assert_eq!(
            get(&reference, 6, Some(usize::MAX)).unwrap(),
            b"overflow regression"
        );
        assert!(
            get(&reference, usize::MAX, Some(usize::MAX))
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn concurrent_identical_streams_store_successfully() {
        let workers: Vec<_> = (0..8)
            .map(|_| {
                std::thread::spawn(|| {
                    let content = vec![b'x'; 1024 * 1024];
                    put_reader(content.as_slice()).unwrap()
                })
            })
            .collect();
        let results: Vec<_> = workers.into_iter().map(|w| w.join().unwrap()).collect();
        assert!(results.iter().all(|result| result == &results[0]));
        assert_eq!(get(&results[0].0, 999_999, Some(4)).unwrap(), b"xxxx");
    }

    #[test]
    fn non_blob_references_are_refused_not_guessed() {
        let err = get("output/out-1-2.log", 0, None).unwrap_err();
        assert_eq!(err.kind(), std::io::ErrorKind::InvalidInput);
        assert!(hex_of("0123").is_none());
    }
}
