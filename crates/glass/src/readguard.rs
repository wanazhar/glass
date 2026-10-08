//! Read-before-write guard.
//!
//! Every high-scoring harness has some version of this, and it is the one
//! guardrail that needs no extra tool: refuse to let the model overwrite a file
//! it has never looked at.
//!
//! The failure it prevents is specific and expensive. `sed -i 's/x/y/' f.rs`
//! against a file the model has never read will happily succeed, and the model
//! will only discover the damage when the build fails several turns later — or
//! not at all. Requiring a read first costs nothing when the model was going to
//! read anyway, which is nearly always, and turns a silent corruption into an
//! immediate, legible error.
//!
//! Two rules keep it from becoming an obstacle:
//!
//! 1. **Only files that already exist.** Creating a new file is not a blind
//!    overwrite, so `cat > new.rs` is never blocked.
//! 2. **Any read counts.** Not `cat` specifically — a `rg` that matched inside
//!    the file, a `git diff` that showed it, a build that compiled it. The
//!    model has seen the contents; the mechanism does not matter.

use std::collections::HashSet;
use std::path::Path;

/// Files the model has demonstrably seen the contents of.
#[derive(Default)]
pub struct ReadSet {
    seen: HashSet<String>,
}

impl ReadSet {
    pub fn new() -> Self {
        Self::default()
    }

    /// Record a file the model has read.
    pub fn mark(&mut self, path: &Path) {
        self.seen.insert(super::concurrency::canonical_key(path));
    }

    /// Whether the model has read this file.
    pub fn contains(&self, path: &Path) -> bool {
        self.seen.contains(&super::concurrency::canonical_key(path))
    }

    /// Paths recorded as read, for tests and diagnostics.
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn marked(&self) -> &HashSet<String> {
        &self.seen
    }

    /// Inspect a bash command for reads and record them.
    ///
    /// Deliberately generous about what counts as a read. A false positive
    /// costs nothing; a false negative lets the overwrite through, which is the
    /// whole thing this guards against.
    pub fn observe(&mut self, command: &str, output: &str, workspace: &Path) {
        let cmd = command.trim();
        if cmd.is_empty() {
            return;
        }

        // The overwhelmingly common idiom: `cat path`, `head -n5 path`, `less`.
        for (i, tok) in cmd.split_whitespace().enumerate() {
            let t = tok.trim_matches(|c: char| {
                !c.is_alphanumeric() && c != '.' && c != '/' && c != '_' && c != '-'
            });
            if t.is_empty() || !looks_like_path(t) {
                continue;
            }
            let prev = if i > 0 {
                cmd.split_whitespace().nth(i - 1).unwrap_or("")
            } else {
                ""
            };
            let next = cmd.split_whitespace().nth(i + 1).unwrap_or("");
            let reader = [
                "cat", "head", "tail", "less", "bat", "wc", "nl", "source", "stat", "file", "wc",
            ]
            .contains(&prev.trim_matches(|c: char| !c.is_alphanumeric()));
            let _ = next;
            if reader {
                self.mark(&abs(t, workspace));
            }
        }

        // `rg` / `grep` counts as reading only when the path was named
        // explicitly. A bare `rg foo .` searched the tree and says nothing about
        // any one file's contents.
        for tool in ["rg", "grep"] {
            if let Some(rest) = strip_tool(cmd, tool) {
                for tok in rest.split_whitespace().skip(1) {
                    let t = tok.trim_matches(|c: char| {
                        !c.is_alphanumeric()
                            && c != '.'
                            && c != '/'
                            && c != '_'
                            && c != '-'
                            && c != '*'
                    });
                    if t.is_empty() || t.starts_with('-') || t == "." || t == ".." {
                        continue;
                    }
                    if looks_like_path(t) {
                        self.mark(&abs(t, workspace));
                    }
                }
            }
        }

        // A compiler or test run read and validated every file it touched, and
        // the failure text names them, so anything mentioned in a non-empty
        // result from a build tool counts as read.
        let builds = [
            "cargo", "go", "npm", "make", "tsc", "pytest", "gcc", "clang",
        ];
        if builds.iter().any(|b| cmd.starts_with(b)) && !output.trim().is_empty() {
            for line in output.lines() {
                for tok in line.split_whitespace() {
                    let t = tok.trim_matches(|c: char| {
                        !c.is_alphanumeric() && c != '.' && c != '/' && c != '_' && c != '-'
                    });
                    if looks_like_path(t) {
                        self.mark(&abs(t, workspace));
                    }
                }
            }
        }
    }

    /// Check a set of write targets, returning those that must not be written.
    ///
    /// Nonexistent files are always allowed: creating is not overwriting.
    pub fn blocked<'a>(&self, targets: &'a [std::path::PathBuf]) -> Vec<&'a std::path::PathBuf> {
        targets
            .iter()
            .filter(|p| p.is_file() && !self.contains(p))
            .collect()
    }
}

fn strip_tool<'a>(cmd: &'a str, tool: &str) -> Option<&'a str> {
    cmd.strip_prefix(tool).map(str::trim_start)
}

fn looks_like_path(t: &str) -> bool {
    t.contains('/') || t.contains('.')
}

fn abs(p: &str, workspace: &Path) -> std::path::PathBuf {
    let raw = Path::new(p);
    if raw.is_absolute() {
        raw.to_path_buf()
    } else {
        workspace.join(raw)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(tag: &str) -> std::path::PathBuf {
        use std::sync::atomic::{AtomicU32, Ordering};
        static SEQ: AtomicU32 = AtomicU32::new(0);
        let n = SEQ.fetch_add(1, Ordering::Relaxed);
        let d = std::env::temp_dir().join(format!("glass-rbw-{}-{tag}-{n}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn cat_marks_the_file_read() {
        let d = scratch("cat");
        let ws = d.clone();
        let f = d.join("a.rs");
        std::fs::write(&f, "fn main() {}").unwrap();

        let mut rs = ReadSet::new();
        rs.observe(&format!("cat {}", f.display()), "fn main() {}", &ws);
        assert!(rs.contains(&f), "cat must count as a read");
        std::fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn relative_paths_resolve_against_the_workspace() {
        let d = scratch("rel");
        let f = d.join("src/lib.rs");
        std::fs::create_dir_all(f.parent().unwrap()).unwrap();
        std::fs::write(&f, "x").unwrap();

        let mut rs = ReadSet::new();
        rs.observe("cat src/lib.rs", "x", &d);
        assert!(
            rs.contains(&f),
            "a relative read must resolve to the same key as an absolute one"
        );
        std::fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn bare_recursive_search_is_not_a_read() {
        let d = scratch("rg");
        let mut rs = ReadSet::new();
        // `rg foo .` says nothing about any individual file's contents.
        rs.observe("rg foo .", "src/a.rs:1:foo", &d);
        assert!(rs.marked().is_empty(), "a tree-wide grep is not a read");
        std::fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn grep_naming_a_file_is_a_read() {
        let d = scratch("grepfile");
        let f = d.join("a.rs");
        std::fs::write(&f, "foo").unwrap();
        let mut rs = ReadSet::new();
        rs.observe(&format!("rg foo {}", f.display()), "1:foo", &d);
        assert!(rs.contains(&f));
        std::fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn unwritten_file_is_blocked() {
        let d = scratch("block");
        let f = d.join("a.rs");
        std::fs::write(&f, "old").unwrap();
        let rs = ReadSet::new();
        let targets = [f];
        assert_eq!(
            rs.blocked(&targets).len(),
            1,
            "an unread existing file must be blocked"
        );
        std::fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn read_file_is_allowed() {
        let d = scratch("allow");
        let f = d.join("a.rs");
        std::fs::write(&f, "old").unwrap();
        let mut rs = ReadSet::new();
        rs.mark(&f);
        let targets = [f];
        assert!(rs.blocked(&targets).is_empty());
        std::fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn new_file_is_never_blocked() {
        // Creating is not overwriting.
        let d = scratch("new");
        let f = d.join("brand_new.rs");
        assert!(!f.exists());
        let rs = ReadSet::new();
        let targets = [f];
        assert!(
            rs.blocked(&targets).is_empty(),
            "a nonexistent file must be writable"
        );
        std::fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn different_spellings_of_one_path_are_the_same_key() {
        let d = scratch("spell");
        let f = d.join("a.rs");
        std::fs::write(&f, "x").unwrap();
        let mut rs = ReadSet::new();
        rs.mark(&f);
        assert!(
            rs.contains(&d.join("./a.rs")),
            "the same file must not look unread because of spelling"
        );
        std::fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn build_output_marks_named_files_read() {
        let d = scratch("build");
        let f = d.join("a.rs");
        std::fs::write(&f, "x").unwrap();
        let mut rs = ReadSet::new();
        rs.observe("cargo build", &format!("error[E0425]: {}", f.display()), &d);
        assert!(
            rs.contains(&f),
            "a compiler that named the file clearly read it"
        );
        std::fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn writes_do_not_mark_a_file_read() {
        // Writing is not reading. A command that overwrites a file must not
        // thereby earn permission to overwrite it again.
        let d = scratch("write");
        let f = d.join("a.rs");
        std::fs::write(&f, "old").unwrap();
        let mut rs = ReadSet::new();
        rs.observe(&format!("echo new > {}", f.display()), "", &d);
        assert!(!rs.contains(&f), "writing must not count as reading");
        std::fs::remove_dir_all(&d).ok();
    }
}
