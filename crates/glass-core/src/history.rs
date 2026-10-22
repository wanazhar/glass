//! Prompt history, persisted across sessions.
//!
//! The editor's recall is in-memory, so it dies with the
//! process. Prompts are the one thing a user types over and
//! over between sessions — "finish the migration", the same
//! correction phrased better — so they outlive the session
//! that produced them.
//!
//! One JSON string per line: prompts can be multi-line, so a
//! plain-text format cannot store them.

use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
use std::{
    collections::VecDeque,
    fs::{File, OpenOptions},
    io::{self, Read, Seek, SeekFrom, Write},
    path::Path,
};

/// Visible recall entries; the physical journal compacts in small batches.
pub const MAX: usize = 200;
const COMPACT_AT: usize = MAX + 32;
const MAX_BYTES: usize = 1024 * 1024;
const MAX_ENTRY_BYTES: usize = 64 * 1024;

fn path(dir: &Path) -> std::path::PathBuf {
    dir.join("history.jsonl")
}

fn open_regular(path: &Path, append: bool) -> io::Result<File> {
    let mut options = OpenOptions::new();
    options
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
    if append {
        options.append(true).create(true).mode(0o600);
    }
    let file = options.open(path)?;
    let metadata = file.metadata()?;
    if !metadata.is_file() || metadata.nlink() != 1 {
        return Err(io::Error::other(
            "History must be a regular file without hard links",
        ));
    }
    Ok(file)
}

/// Read a bounded tail. Skip a partial first line after seeking into old files.
fn window(dir: &Path) -> io::Result<(Vec<u8>, u64)> {
    let mut file = match open_regular(&path(dir), false) {
        Ok(file) => file,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok((Vec::new(), 0)),
        Err(error) => return Err(error),
    };
    let size = file.metadata()?.len();
    let start = size.saturating_sub(MAX_BYTES as u64);
    file.seek(SeekFrom::Start(start))?;
    let mut bytes = Vec::with_capacity(size.min(MAX_BYTES as u64) as usize);
    file.take(MAX_BYTES as u64).read_to_end(&mut bytes)?;
    if start > 0 {
        let end = bytes
            .iter()
            .position(|byte| *byte == b'\n')
            .map_or(bytes.len(), |at| at + 1);
        bytes.drain(..end);
    }
    Ok((bytes, size))
}
fn entries(bytes: &[u8]) -> VecDeque<(String, &[u8])> {
    let mut out = VecDeque::new();
    for line in bytes.split(|byte| *byte == b'\n') {
        if line.len() > MAX_ENTRY_BYTES {
            continue;
        }
        if let Ok(text) = serde_json::from_slice::<String>(line) {
            out.push_back((text, line));
            if out.len() > MAX {
                out.pop_front();
            }
        }
    }
    out
}

/// Recent bounded recall. Oversized pasted prompts remain in conversation logs.
pub fn load(dir: &Path) -> Vec<String> {
    window(dir)
        .map(|(bytes, _)| entries(&bytes).into_iter().map(|(text, _)| text).collect())
        .unwrap_or_default()
}

/// Best-effort recall only: a busy writer never delays the current conversation.
pub fn record(dir: &Path, text: &str) {
    if text.trim().is_empty() || text.len() > MAX_ENTRY_BYTES {
        return;
    }
    let Ok(line) = serde_json::to_vec(text) else {
        return;
    };
    if line.len() > MAX_ENTRY_BYTES {
        return;
    }
    let lock = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(false)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(dir.join("history.lock"));
    let Ok(lock) = lock else {
        return;
    };
    if !lock
        .metadata()
        .is_ok_and(|meta| meta.is_file() && meta.nlink() == 1)
        || fs2::FileExt::try_lock_exclusive(&lock).is_err()
    {
        return;
    }
    let Ok((bytes, size)) = window(dir) else {
        return;
    };
    let separator = usize::from(!bytes.is_empty() && !bytes.ends_with(b"\n"));
    let line_count = bytes.iter().filter(|byte| **byte == b'\n').count() + separator;
    let duplicate = bytes
        .rsplit(|byte| *byte == b'\n')
        .find(|line| !line.is_empty())
        .filter(|line| line.len() <= MAX_ENTRY_BYTES)
        .and_then(|line| serde_json::from_slice::<String>(line).ok())
        .is_some_and(|previous| previous == text);
    if duplicate && size <= MAX_BYTES as u64 && line_count <= COMPACT_AT {
        return;
    }
    if size <= MAX_BYTES as u64
        && size + line.len() as u64 + (separator as u64) < MAX_BYTES as u64
        && line_count < COMPACT_AT
        && !duplicate
    {
        if let Ok(mut file) = open_regular(&path(dir), true) {
            if separator > 0 && file.write_all(b"\n").is_err() {
                return;
            }
            let mut record = line;
            record.push(b'\n');
            let _ = file.write_all(&record);
        }
        return;
    }
    let mut saved = entries(&bytes);
    if !duplicate {
        saved.push_back((text.to_owned(), &line));
    }
    let mut total: usize = saved.iter().map(|(_, line)| line.len() + 1).sum();
    while saved.len() > MAX || total > MAX_BYTES {
        if let Some((_, line)) = saved.pop_front() {
            total -= line.len() + 1;
        }
    }
    let mut compacted = Vec::with_capacity(total);
    for (_, line) in saved {
        compacted.extend_from_slice(line);
        compacted.push(b'\n');
    }
    let _ = crate::auth::secure_write(&path(dir), &compacted);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(tag: &str) -> std::path::PathBuf {
        static SEQ: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
        let n = SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let d =
            std::env::temp_dir().join(format!("glass-history-{}-{tag}-{n}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn recall_is_byte_bounded_and_large_prompts_are_not_duplicated() {
        let d = scratch("bytes");
        record(&d, &"x".repeat(MAX_ENTRY_BYTES + 1));
        assert!(!path(&d).exists());
        for index in 0..40 {
            record(&d, &format!("{index}:{}", "z".repeat(40_000)));
        }
        assert!(std::fs::metadata(path(&d)).unwrap().len() <= MAX_BYTES as u64);
        assert!(load(&d).last().unwrap().starts_with("39:"));
        // A legacy oversized file is bounded on reads and compacted even when
        // the next eligible prompt is identical to its newest valid entry.
        std::fs::write(
            path(&d),
            format!("{}\n\"last\"\n", "x".repeat(MAX_BYTES * 2)),
        )
        .unwrap();
        assert_eq!(load(&d), vec!["last"]);
        record(&d, "last");
        assert!(std::fs::metadata(path(&d)).unwrap().len() <= MAX_BYTES as u64);
        std::fs::remove_dir_all(d).unwrap();
    }

    #[test]
    fn adjacent_duplicates_and_batching_avoid_full_rewrites() {
        let d = scratch("batching");
        record(&d, "first");
        let inode = std::fs::metadata(path(&d)).unwrap().ino();
        let original = std::fs::read(path(&d)).unwrap();
        record(&d, "first");
        assert_eq!(std::fs::read(path(&d)).unwrap(), original);
        for index in 1..COMPACT_AT {
            record(&d, &format!("prompt {index}"));
        }
        assert_eq!(std::fs::metadata(path(&d)).unwrap().ino(), inode);
        assert_eq!(load(&d).len(), MAX);
        record(&d, "compact now");
        assert_eq!(
            std::fs::read_to_string(path(&d)).unwrap().lines().count(),
            MAX
        );
        assert_eq!(load(&d).last().unwrap(), "compact now");
        std::fs::remove_dir_all(d).unwrap();
    }

    #[test]
    fn recall_refuses_links_and_never_waits_for_a_busy_writer() {
        let d = scratch("links-locks");
        let outside = d.join("outside");
        std::fs::write(&outside, "\"private\"\n").unwrap();
        std::os::unix::fs::symlink(&outside, path(&d)).unwrap();
        record(&d, "do not follow");
        assert!(load(&d).is_empty());
        assert_eq!(std::fs::read_to_string(outside).unwrap(), "\"private\"\n");
        std::fs::remove_file(path(&d)).unwrap();
        let lock = OpenOptions::new()
            .write(true)
            .open(d.join("history.lock"))
            .unwrap();
        fs2::FileExt::try_lock_exclusive(&lock).unwrap();
        record(&d, "busy");
        assert!(!path(&d).exists());
        drop(lock);
        record(&d, "available");
        assert_eq!(load(&d), vec!["available"]);
        assert_eq!(std::fs::metadata(path(&d)).unwrap().mode() & 0o777, 0o600);
        std::fs::remove_dir_all(d).unwrap();
    }

    #[test]
    fn records_roundtrip() {
        let d = scratch("roundtrip");
        assert!(load(&d).is_empty());
        record(&d, "fix the parser");
        record(&d, "now run the tests");
        assert_eq!(
            load(&d),
            vec![
                "fix the parser".to_string(),
                "now run the tests".to_string()
            ]
        );
        std::fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn multi_line_prompts_survive() {
        // A pasted diff is a valid prompt and must come
        // back with its newlines intact.
        let d = scratch("multiline");
        record(&d, "line one\nline two");
        assert_eq!(load(&d), vec!["line one\nline two".to_string()]);
        std::fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn empty_prompts_are_not_recorded() {
        let d = scratch("empty");
        record(&d, "   ");
        assert!(load(&d).is_empty());
        std::fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn the_file_is_capped() {
        let d = scratch("cap");
        for i in 0..(MAX + 50) {
            record(&d, &format!("prompt {i}"));
        }
        let loaded = load(&d);
        assert_eq!(loaded.len(), MAX, "the file must stay bounded");
        assert_eq!(loaded.first().map(String::as_str), Some("prompt 50"));
        let expected = format!("prompt {}", MAX + 49);
        assert_eq!(loaded.last().map(String::as_str), Some(expected.as_str()));
        std::fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn a_torn_tail_does_not_suppress_a_repeated_prompt() {
        let d = scratch("repeated-torn");
        record(&d, "again");
        OpenOptions::new()
            .append(true)
            .open(path(&d))
            .unwrap()
            .write_all(b"{torn")
            .unwrap();
        record(&d, "again");
        assert_eq!(load(&d), vec!["again", "again"]);
        assert!(
            std::fs::read_to_string(path(&d))
                .unwrap()
                .ends_with("{torn\n\"again\"\n")
        );
        std::fs::remove_dir_all(d).unwrap();
    }

    #[test]
    fn a_torn_line_does_not_block_the_rest() {
        let d = scratch("torn");
        record(&d, "good one");
        // Simulate a crash mid-write: a half line, then more.
        use std::io::Write;
        let p = path(&d);
        let mut f = std::fs::OpenOptions::new().append(true).open(&p).unwrap();
        writeln!(f, "{{\"trunc").unwrap();
        drop(f);
        record(&d, "after the crash");
        let loaded = load(&d);
        assert_eq!(loaded.len(), 2, "{loaded:?}");
        assert_eq!(loaded[1], "after the crash");
        std::fs::remove_dir_all(&d).ok();
    }
}
