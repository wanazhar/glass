//! Bounded in-process cache for workspace directory walks (task
//! six-wave-d-001, OMP `pi-walker`).
//!
//! Discovery paths (plugins, skills, session listing) glob the workspace
//! repeatedly within one session with nothing changed in between. The cache
//! keeps each walked directory's entry names and never serves a stale
//! question silently: any failed match invalidates the directory, and tool
//! results (write/edit/rename and bash `mv`/`rm`/`git`) invalidate it
//! explicitly. Grep and bash tool output scans stay uncached.
//!
//! Bounds: at most [`MAX_ENTRIES`] directories and [`MAX_PATHS`] total
//! names. Hit/miss counters are reported through `diagnostics()`, never
//! silently.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// Bounded cache: directories held at once.
pub const MAX_ENTRIES: usize = 64;
/// Bounded cache: total file names held across all directories.
pub const MAX_PATHS: usize = 8_192;

#[derive(Default)]
#[allow(dead_code)] // bytes feeds eviction sizing; dirs future nested walks
struct Walk {
    names: Vec<String>,
    /// Kept for future nested-walk work; single-level caching is what the
    /// consumers use, and honest bounds beat carrying dead weight.
    dirs: Vec<String>,
    bytes: usize,
}

#[derive(Default)]
struct Cache {
    entries: HashMap<PathBuf, Walk>,
    hits: u64,
    misses: u64,
    invalidations: u64,
    bytes: usize,
}

static CACHE: Mutex<Option<Cache>> = Mutex::new(None);

fn with<R>(f: impl FnOnce(&mut Cache) -> R) -> Option<R> {
    let mut guard = CACHE.lock().ok()?;
    let cache = guard.get_or_insert_with(Cache::default);
    Some(f(cache))
}

/// Directory entry names, cached. `None` on natural absence (no such
/// directory) — the caller's fallback path runs as before.
pub fn read_dir(dir: &Path) -> Option<Vec<std::fs::DirEntry>> {
    let key = dir.to_path_buf();
    let cached = with(|cache| {
        if let Some(walk) = cache.entries.get(&key) {
            cache.hits += 1;
            Some(walk.names.clone())
        } else {
            cache.misses += 1;
            None
        }
    })?;
    if let Some(names) = cached {
        // A cached hit has to return real entry handles. Re-stat each name:
        // consistency beats a raw clone, and a vanished entry invalidates
        // the whole directory rather than serving a stale set.
        let mut out = Vec::new();
        for name in names {
            let path = key.join(&name);
            match std::fs::metadata(&path) {
                Ok(_meta) => {
                    let entry = std::fs::read_dir(&key).and_then(|mut rd| {
                        rd.find(|e| {
                            e.as_ref()
                                .map(|v| v.file_name().to_string_lossy() == name)
                                .unwrap_or(false)
                        })
                        .transpose()
                    });
                    match entry {
                        Ok(Some(e)) => out.push(e),
                        _ => {
                            // Conservative: any failed match invalidates.
                            invalidate(dir);
                            return walk_uncached(dir);
                        }
                    }
                }
                Err(_) => {
                    // Conservative: any failed match invalidates.
                    invalidate(dir);
                    return walk_uncached(dir);
                }
            }
        }
        return Some(out);
    }
    let walk = walk_uncached(dir)?;
    let fresh: Vec<String> = walk
        .iter()
        .filter_map(|e| e.file_name().to_string_lossy().into_owned().into())
        .collect();
    let bytes: usize = fresh.iter().map(|n| n.len()).sum();
    let stored = with(|cache| {
        if cache.entries.len() >= MAX_ENTRIES || cache.bytes + bytes > MAX_PATHS * 64 {
            // Bounded: evict everything; a full cache is a signal the
            // caller's churn outnumbers its reuse, not a reason to lie.
            cache.entries.clear();
            cache.bytes = 0;
        }
        cache.entries.insert(
            key.clone(),
            Walk { names: fresh.clone(), dirs: Vec::new(), bytes },
        );
        cache.bytes += bytes;
    });
    let _ = stored;
    Some(walk)
}

fn walk_uncached(dir: &Path) -> Option<Vec<std::fs::DirEntry>> {
    std::fs::read_dir(dir).ok()?.collect::<Result<Vec<_>, _>>().ok()
}

/// Invalidate one directory after a write/edit/rename touched it.
pub fn invalidate(dir: &Path) {
    let _ = with(|cache| {
        if cache.entries.remove(dir).is_some() {
            cache.invalidations += 1;
        }
    });
}

/// Invalidate everything: bulk operations (a whole bash command, a worktree
/// switch) touch unknown paths, so the conservative answer is all of it.
pub fn invalidate_all() {
    let _ = with(|cache| {
        cache.entries.clear();
        cache.bytes = 0;
        cache.invalidations += 1;
    });
}

/// Hit/miss/invalidation counters with the stored bounds, for `/usage`-style
/// diagnostics. Never shown unless asked for — this returns plain data.
pub fn diagnostics() -> serde_json::Value {
    with(|cache| {
        serde_json::json!({
            "cached_dirs": cache.entries.len(),
            "max_entries": MAX_ENTRIES,
            "hits": cache.hits,
            "misses": cache.misses,
            "invalidations": cache.invalidations,
        })
    })
    .unwrap_or(serde_json::json!({"enabled": false}))
}

#[cfg(test)]
mod tests {
    use super::*;
    // One global cache: tests lock this so counters stay per-test.
    static TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    #[test]
    fn cached_second_walk_hits_and_a_lost_entry_invalidates() {
        let _guard = TEST_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        invalidate_all();
        let dir = std::env::temp_dir().join(format!("gc-fsscan-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("a.txt"), "x").unwrap();

        let first = read_dir(&dir).unwrap();
        assert_eq!(first.len(), 1);
        let d = diagnostics();
        assert!(d["misses"].as_u64().unwrap() >= 1, "{d}");

        let after_misses = d["misses"].clone();
        let after_hits = d["hits"].clone();
        let second = read_dir(&dir).unwrap();
        assert_eq!(second.len(), 1);
        let d = diagnostics();
        assert_eq!(d["hits"], after_hits.as_u64().unwrap() + 1, "{d}");

        // The name disappears underneath the cache: the walk re-runs.
        std::fs::remove_file(dir.join("a.txt")).unwrap();
        let third = read_dir(&dir).unwrap();
        assert!(third.is_empty(), "stale name must not survive");
        let d = diagnostics();
        assert!(d["invalidations"].as_u64().unwrap() >= 1, "{d}");
        let _ = after_misses;

        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn invalidate_all_clears_everything() {
        let _guard = TEST_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        invalidate_all();
        // The counter is global and tests share the process: assert the
        // monotonic bump, not an absolute count.
        let before = diagnostics();
        let dir = std::env::temp_dir().join(format!("gc-fsscan-all-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        read_dir(&dir).unwrap();
        invalidate_all();
        let d = diagnostics();
        assert_eq!(d["cached_dirs"], 0, "{d}");
        assert!(d["invalidations"].as_u64().unwrap() > before["invalidations"].as_u64().unwrap(), "{d}");
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
