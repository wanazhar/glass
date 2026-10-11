//! Content-addressed blobs (task six-wave-d-001, OMP blob-artifacts).
//!
//! Bash spill files and image payloads are stored under the data dir as
//! `blobs/<sha256 hex>` with a `blob:<sha256>` reference the record keeps.
//! Writes are atomic (`tmp` then rename) and deduplicated: writing the same
//! content twice stores it once and both callers get the same address.
//!
//! Garbage control: `sweep(&data_dir)` deletes blobs no session record
//! references (`gc blobs sweep [--json]`). Running sessions are skipped by
//! mtime; in-flight blobs have no reference yet, so they win the race by
//! remaining unreadable rather than being deleted mid-write.

use ring::digest::{digest, SHA256};
use std::path::{Path, PathBuf};

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

/// Store content, returning `(reference, path)`. Dedup: identical content
/// maps to one stored object, and a second write is a no-op rename of    /// renamed — a crashed `put` leaves it, and a failed rename
    /// leaves it too. Age is the only safe signal, so sweep removes old
    /// tmp files and leaves recent ones.
pub fn put(content: &[u8]) -> std::io::Result<(String, PathBuf)> {
    let hex = encode_hex(digest(&SHA256, content).as_ref());
    let dir = dir();
    std::fs::create_dir_all(&dir)?;
    let path = dir.join(&hex);
    if path.exists() {
        return Ok((reference(&hex), path));
    }
    // Atomic tmp-rename; a partial file can never masquerade as a blob
    // because the name is only the content's digest once rename lands.
    let tmp = dir.join(format!(".tmp-{}-{}", std::process::id(), hex));
    {
        use std::io::Write as _;
        use std::os::unix::fs::OpenOptionsExt;
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create_new(true).mode(0o600);
        let mut f = options.open(&tmp)?;
        f.write_all(content)?;
        f.flush()?;
    }
    std::fs::rename(&tmp, &path)?;
    Ok((reference(&hex), path))
}

/// Read a blob back by reference. Offset/length supported for ranged
/// re-reads from a later session or handoff.
pub fn get(reference: &str, offset: usize, length: Option<usize>) -> std::io::Result<Vec<u8>> {
    let hex = hex_of(reference)
        .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::InvalidInput, "not a blob reference"))?;
    let path = dir().join(hex);
    let data = std::fs::read(path)?;
    let start = offset.min(data.len());
    let end = length.map(|l| start + l).unwrap_or(data.len()).min(data.len());
    Ok(data[start..end].to_vec())
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
    let mut refs = std::collections::HashSet::new();
    let entries = std::fs::read_dir(data_dir).map_err(|e| format!("{}: {e}", data_dir.display()))?;
    for entry in entries {
        let entry = entry.map_err(|e| e.to_string())?;
        let path = entry.path();
        if path.extension().is_none_or(|x| x != "jsonl") {
            continue;
        }
        // Read raw bytes and scan for `"blob:<64 hex>"`: record encodings
        // differ (`spill` on bash records, `out` on tool results), and a
        // plain byte scan is the one form that survives every encoding
        // change. A blob name can't collide with ordinary text because it
        // is a full 64-hex digest behind the scheme prefix.
        let bytes = std::fs::read(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        let text = String::from_utf8_lossy(&bytes);
        let mut from = 0usize;
        while let Some(at) = text[from..].find("blob:") {
            let at = from + at;
            // Scan form: the candidate hex is exactly the 64 bytes after
            // the prefix and must end at a non-hex boundary or end of text,
            // so a longer run of hex digits (a concatenation, not a
            // reference) is never accepted. `hex_of` expects the whole
            // remaining string to be the reference and is used by readers
            // that already hold a delimited field value.
            let rest = &text[at..];
            let hexrun = rest.get(5..69).unwrap_or("");
            if hexrun.len() == 64
                && hexrun.chars().all(|c| c.is_ascii_hexdigit())
                && rest[69..].chars().next().is_none_or(|c| !c.is_ascii_hexdigit())
            {
                refs.insert(hexrun.to_ascii_lowercase());
            }
            from = at + 5;
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
    let entries = std::fs::read_dir(&blob_dir).map_err(|e| format!("{}: {e}", blob_dir.display()))?;
    for entry in entries {
        let entry = entry.map_err(|e| { report.errors += 1; e.to_string() })?;
        let path = entry.path();
        report.scanned += 1;
        let name = match path.file_name().and_then(|n| n.to_str()) {
            Some(n) => n.to_string(),
            None => { report.errors += 1; continue; }
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
            if aged > 0 { age(&p, aged); }
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
    fn non_blob_references_are_refused_not_guessed() {
        let err = get("output/out-1-2.log", 0, None).unwrap_err();
        assert_eq!(err.kind(), std::io::ErrorKind::InvalidInput);
        assert!(hex_of("0123").is_none());
    }
}
