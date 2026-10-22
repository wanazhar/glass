//! Project instruction files: `AGENTS.md` and `CLAUDE.md`.
//!
//! Every serious harness reads these — pi, Codex, Aider, Goose, Claude Code —
//! so a harness that ignores them feels broken to anyone arriving from another
//! tool. They are also the cheapest possible feature: pure prompt text, no
//! new tools, no new code paths.
//!
//! The cost is tokens, and tokens are our scarcest resource. Three rules keep
//! it cheap:
//!
//! 1. **One file per directory, first match wins.** The candidate order is
//!    fixed, so a directory that has both files pays for one.
//! 2. **Hard size cap.** A file larger than the cap is truncated with a
//!    visible marker rather than silently filling the prompt.
//! 3. **No ancestor walking by default.** Only the workspace root and the
//!    cwd are read. Walking up to `/` is what makes this feature unpredictable.
//!
//! A file that is too big is a sign it belongs in a skill, which is loaded on
//! demand for exactly this reason.

use std::path::{Path, PathBuf};

/// Candidate names, in precedence order. `override` shadows within one
/// directory only, which is the convention the ecosystem settled on.
pub const CANDIDATES: &[&str] = &[
    "AGENTS.override.md",
    "AGENTS.md",
    "AGENTS.MD",
    "CLAUDE.md",
    "CLAUDE.MD",
];

/// Bytes of a single instruction file we are willing to inline. Roughly 250
/// tokens. Past this the file is a document, not instructions.
pub const MAX_FILE_BYTES: usize = 8_000;

/// Total budget across every file, so a repo with instructions in several
/// directories cannot crowd out the conversation.
pub const MAX_TOTAL_BYTES: usize = 12_000;

/// Compile-time guard on the budget. Raising `MAX_TOTAL_BYTES` past this should
/// be a deliberate act with a deliberate justification, not an accident that
/// only shows up as a slower first request.
const _: () = assert!(
    MAX_TOTAL_BYTES <= 16_000,
    "project instructions are now costing more than the tool schemas"
);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Instruction {
    pub path: PathBuf,
    pub text: String,
    /// True when the file was cut at [`MAX_FILE_BYTES`].
    pub truncated: bool,
}

impl Instruction {
    /// Rendered for the system prompt.
    pub fn render(&self) -> String {
        let body = if self.truncated {
            format!(
                "{}\n[truncated at {} bytes — read the rest with `cat` if you need it]",
                self.text, MAX_FILE_BYTES
            )
        } else {
            self.text.clone()
        };
        format!(
            "<project_instructions path=\"{}\">\n{body}\n</project_instructions>",
            self.path.display()
        )
    }
}

/// Find the instruction file for a single directory.
pub fn find_in(dir: &Path) -> Option<PathBuf> {
    CANDIDATES.iter().map(|c| dir.join(c)).find(|p| p.is_file())
}

/// Load instructions for a workspace.
///
/// Returns empty when the project has no instruction file, which is the common
/// case and must stay free.
pub fn load(workspace: &Path) -> Vec<Instruction> {
    let mut out = Vec::new();
    let mut budget = MAX_TOTAL_BYTES;

    // Root first, then the cwd, so a deeper file refines the root's.
    let mut dirs: Vec<&Path> = vec![workspace];
    let cwd = std::env::current_dir().ok();
    if let Some(c) = cwd.as_deref()
        && c != workspace
        && c.starts_with(workspace)
    {
        dirs.push(c);
    }

    for dir in dirs {
        let Some(path) = find_in(dir) else { continue };
        if out.iter().any(|i: &Instruction| i.path == path) {
            continue;
        }
        let Ok(raw) = std::fs::read(&path) else {
            continue;
        };

        // Two independent limits: the per-file cap keeps one file from filling
        // the prompt, the running budget keeps several files from doing it
        // together. Either one alone is insufficient.
        let take = raw.len().min(MAX_FILE_BYTES).min(budget);
        let truncated = raw.len() > take;
        let text = String::from_utf8_lossy(&raw[..take]).trim().to_string();
        if text.is_empty() {
            continue;
        }
        budget -= take.min(budget);

        out.push(Instruction {
            path,
            text,
            truncated,
        });
        if budget == 0 {
            break;
        }
    }
    out
}

/// Render every instruction file as one system-prompt section.
pub fn render_all(instructions: &[Instruction]) -> Option<String> {
    if instructions.is_empty() {
        return None;
    }
    let mut out = String::new();
    for i in instructions {
        out.push('\n');
        out.push_str(&i.render());
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(tag: &str) -> PathBuf {
        static SEQ: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
        let n = SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let d = std::env::temp_dir().join(format!("glass-agents-{}-{tag}-{n}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn finds_agents_md() {
        let d = scratch("basic");
        std::fs::write(d.join("AGENTS.md"), "use cargo, not npm").unwrap();
        let found = find_in(&d).expect("should find AGENTS.md");
        assert!(found.ends_with("AGENTS.md"));
        std::fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn override_wins_over_agents() {
        let d = scratch("override");
        std::fs::write(d.join("AGENTS.md"), "base").unwrap();
        std::fs::write(d.join("AGENTS.override.md"), "override").unwrap();
        let found = find_in(&d).unwrap();
        assert!(found.ends_with("AGENTS.override.md"));
        std::fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn claude_md_is_a_fallback() {
        let d = scratch("claude");
        std::fs::write(d.join("CLAUDE.md"), "from claude").unwrap();
        let found = find_in(&d).unwrap();
        assert!(found.ends_with("CLAUDE.md"));
        std::fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn one_file_per_directory_even_if_several_exist() {
        let d = scratch("one");
        std::fs::write(d.join("AGENTS.md"), "a").unwrap();
        std::fs::write(d.join("CLAUDE.md"), "b").unwrap();
        let loaded = load(&d);
        assert_eq!(loaded.len(), 1, "must not read both");
        std::fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn missing_file_yields_nothing() {
        let d = scratch("none");
        assert!(load(&d).is_empty());
        std::fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn oversized_file_is_truncated_visibly() {
        let d = scratch("big");
        std::fs::write(d.join("AGENTS.md"), "x".repeat(MAX_FILE_BYTES + 500)).unwrap();
        let loaded = load(&d);
        assert!(loaded[0].truncated, "must be marked truncated");
        let r = loaded[0].render();
        assert!(r.contains("truncated"), "{r}");
        assert!(r.len() < MAX_FILE_BYTES + 500);
        std::fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn total_budget_is_enforced() {
        let d = scratch("budget");
        std::fs::write(d.join("AGENTS.md"), "x".repeat(MAX_FILE_BYTES)).unwrap();
        // A nested cwd file must not push us past the total budget.
        let nested = d.join("sub");
        std::fs::create_dir_all(&nested).unwrap();
        std::fs::write(nested.join("AGENTS.md"), "y".repeat(MAX_FILE_BYTES)).unwrap();

        let loaded = load(&d);
        let total: usize = loaded.iter().map(|i| i.text.len()).sum();
        assert!(total <= MAX_TOTAL_BYTES, "total {total} exceeded budget");
        std::fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn empty_file_is_skipped() {
        let d = scratch("empty");
        std::fs::write(d.join("AGENTS.md"), "   \n  ").unwrap();
        assert!(
            load(&d).is_empty(),
            "a blank file must not add an empty section"
        );
        std::fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn render_wraps_in_a_tag_with_the_path() {
        let i = Instruction {
            path: PathBuf::from("/w/AGENTS.md"),
            text: "use cargo".into(),
            truncated: false,
        };
        let r = i.render();
        assert!(r.contains("project_instructions"));
        assert!(r.contains("/w/AGENTS.md"));
        assert!(r.contains("use cargo"));
    }

    #[test]
    fn render_all_returns_none_when_empty() {
        assert!(render_all(&[]).is_none());
    }
}
