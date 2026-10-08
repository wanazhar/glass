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

use std::path::Path;

/// How many prompts are remembered. A cap keeps the file
/// bounded; recall only ever needs the recent end.
pub const MAX: usize = 200;

fn path(dir: &Path) -> std::path::PathBuf {
    dir.join("history.jsonl")
}

/// The most recent [`MAX`] prompts, oldest first.
pub fn load(dir: &Path) -> Vec<String> {
    use std::io::{BufRead, BufReader};
    let Ok(file) = std::fs::File::open(path(dir)) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for line in BufReader::new(file).lines().map_while(Result::ok) {
        // A torn final line is expected after a hard kill;
        // skipping it beats refusing to load the rest.
        if let Ok(s) = serde_json::from_str::<String>(&line) {
            out.push(s);
        }
    }
    if out.len() > MAX {
        out.drain(..out.len() - MAX);
    }
    out
}

/// Append one prompt, trimming the file back to [`MAX`].
///
/// Failures are silent: history is a convenience, and a
/// read-only data directory must not break the prompt.
pub fn record(dir: &Path, text: &str) {
    if text.trim().is_empty() {
        return;
    }
    let Ok(line) = serde_json::to_string(text) else {
        return;
    };
    use std::io::Write;
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path(dir))
    {
        let _ = writeln!(f, "{line}");
    }
    trim(dir);
}

/// Drop the oldest entries once the file exceeds [`MAX`].
fn trim(dir: &Path) {
    let p = path(dir);
    let Ok(text) = std::fs::read_to_string(&p) else {
        return;
    };
    let lines: Vec<&str> = text.lines().collect();
    if lines.len() <= MAX {
        return;
    }
    let keep = lines[lines.len() - MAX..].join("\n");
    let _ = std::fs::write(&p, format!("{keep}\n"));
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
