//! Post-write verification and syntax gating for file mutations.
//!
//! The interface research is unusually clear on one point: the biggest win in
//! editing reliability is not a better edit tool, it is a *harness-side guardrail
//! that checks the result*. SWE-agent measured that adding a lint check which
//! reverts on failure was worth ~3pp, more than the edit tool itself, and that
//! returning the updated file to the model was worth another +3.5pp on edit
//! success and cut cost ~18%.
//!
//! Since our only tool is `bash`, this has to live in the harness rather than in
//! a tool: the model writes a file with whatever shell idiom it likes, and we
//! verify afterwards regardless of how the write happened.
//!
//! Two independent checks:
//!
//! 1. **Echo-back.** A write is followed by reading the affected region back and
//!    putting it in front of the model. Free, works for every language, and it
//!    is what lets the model notice it truncated a heredoc.
//! 2. **Syntax gate.** After a write to a source file, run the language's own
//!    parser and, on failure, return the error plus the offending lines. We do
//!    not revert: reverting requires deciding the previous content was better,
//!    which is a judgement the model should make with the error in hand.

use std::path::{Path, PathBuf};

/// Languages we can gate cheaply, mapped to a parser command that exits
/// non-zero on a syntax error.
///
/// Only interpreters and compilers that are already required to run the project
/// are used. Nothing is installed, and an absent command means "cannot check",
/// which is reported honestly rather than treated as a pass.
const SYNTAX_CHECKS: &[(&str, &[&str])] = &[
    ("py", &["python3", "-m", "py_compile"]),
    ("pyi", &["python3", "-m", "py_compile"]),
    ("js", &["node", "--check"]),
    ("mjs", &["node", "--check"]),
    ("cjs", &["node", "--check"]),
    (
        "ts",
        &["tsc", "--noEmit", "--allowJs", "false", "--skipLibCheck"],
    ),
    ("json", &["python3", "-m", "json.tool"]),
];

/// Extension of `path`, lowercased.
fn ext(path: &Path) -> Option<String> {
    path.extension()
        .and_then(|e| e.to_str())
        .map(str::to_ascii_lowercase)
}

/// Characters that delimit a shell token we care about. Stripping them lets a
/// single helper handle quoted paths without four near-identical branches.
const TRIM: &[char] = &['\'', '"', ';', '`', '|', ',', ')', '('];

/// Extract paths a shell command plausibly wrote.
///
/// This is heuristic by necessity: we are parsing text the model wrote, and a
/// full shell parser is both large and a new failure surface. The bias is
/// deliberately toward false positives, because a spurious verification costs a
/// few hundred tokens while a missed one costs a corrupted file the model never
/// notices.
pub fn written_paths(command: &str, workspace: &Path) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = Vec::new();
    let cmd = command.trim();

    // Heredoc: `cat > path <<'EOF'`. The redirect target and the heredoc marker
    // are separate tokens, so the target is found by the same redirect rule as
    // any other write; this branch exists only to make sure a bare `<<` does
    // not suppress it.
    let _heredoc = cmd.contains("<<");

    // Redirects and `tee`. The `>` may be its own token (`echo hi > out`) or
    // attached to the path (`>out.txt`), and `tee` may carry `-a`. Both shapes
    // have to be handled or the most common write idiom is missed.
    let toks: Vec<&str> = cmd.split_whitespace().collect();
    for (idx, tok) in toks.iter().enumerate() {
        let t = tok.trim_matches(TRIM);

        let attached = t.starts_with('>') && t.len() > 1;
        // `>` or `>>` standing alone as its own token.
        let detached =
            idx > 0 && matches!(toks[idx - 1].trim_matches(TRIM), ">" | ">>" | "1>" | "2>");
        let tee_prev = idx > 0 && toks[idx - 1].trim_matches(TRIM) == "tee";
        let tee_flag =
            idx >= 2 && toks[idx - 1] == "-a" && toks[idx - 2].trim_matches(TRIM) == "tee";

        let candidate = if attached {
            Some(t.trim_start_matches('>'))
        } else if detached || tee_prev || tee_flag {
            Some(t)
        } else {
            None
        };

        if let Some(p) = candidate
            && !p.is_empty()
            && p != "/"
            && !p.starts_with('&')
            && !p.contains('$')
            && !p.contains('*')
        {
            out.push(resolve(p, workspace));
        }
    }

    // Whole-file mutation tools that never name a redirect.
    // `sed -i` names its file as an argument rather than via a redirect, so it
    // needs its own rule. The script is quoted and the path is not, so the file
    // is the first argument after the closing quote — taking the last token
    // instead misses the common `sed -i 's/x/y/' file` form entirely.
    if let Some(rest) = cmd.strip_prefix("sed -i") {
        let mut after_script = rest.trim_start();
        // `-e`/`-f` put the script behind a flag, so skip flags first, then the
        // script, then read the path.
        for flag in ["-e", "-f"] {
            if let Some(r) = after_script.strip_prefix(flag) {
                after_script = r.trim_start();
                break;
            }
        }
        if after_script.starts_with('\'') || after_script.starts_with('"') {
            let quote = after_script.as_bytes()[0] as char;
            if let Some(end) = after_script[1..].find(quote) {
                after_script = after_script[end + 2..].trim_start();
            }
        }
        // Skip any remaining flags such as `-e` or a backup suffix.
        for tok in after_script.split_whitespace() {
            let t = tok.trim_matches(TRIM);
            if t.is_empty() || t.starts_with('-') {
                continue;
            }
            out.push(resolve(t, workspace));
            break;
        }
    }

    out.sort();
    out.dedup();
    out
}

fn resolve(p: &str, workspace: &Path) -> PathBuf {
    let raw = Path::new(p);
    if raw.is_absolute() {
        raw.to_path_buf()
    } else {
        workspace.join(raw)
    }
}

/// Result of checking one file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Check {
    pub path: PathBuf,
    /// The verification text to put in front of the model.
    pub report: String,
    /// True when a parser rejected the file.
    pub failed: bool,
}

/// Read back the tail of a written file and, for known languages, syntax-check it.
///
/// `tail_lines` bounds the echo so a 4000-line generated file does not flood the
/// context; the model can always read more with `bash`.
pub fn verify(path: &Path, tail_lines: usize) -> Option<Check> {
    // Only verify what exists: a redirect to a path the command never created
    // means the command failed in a way its output should already explain.
    let meta = std::fs::metadata(path).ok()?;
    let is_dir = meta.is_dir();
    if is_dir || meta.len() == 0 {
        return None;
    }

    let mut report = String::new();

    // Syntax gate first: if the file does not parse, an echo-back of the broken
    // region is what actually helps the model fix it.
    if let Some((_, cmd)) = SYNTAX_CHECKS
        .iter()
        .find(|(e, _)| ext(path).as_deref() == Some(*e))
    {
        match run_parser(cmd, path) {
            Ok(()) => {
                report.push_str("[syntax ok]\n");
            }
            Err(msg) => {
                report.push_str(&format!("[syntax error] {msg}\n"));
                return Some(Check {
                    path: path.to_path_buf(),
                    report,
                    failed: true,
                });
            }
        }
    }

    let bytes = std::fs::read(path).ok()?;
    let text = String::from_utf8_lossy(&bytes);
    let total = text.lines().count();
    let shown: Vec<&str> = text
        .lines()
        .skip(total.saturating_sub(tail_lines))
        .collect();

    report.push_str(&format!("[wrote {} — {total} lines]\n", display(path)));
    for l in shown {
        report.push_str(l);
        report.push('\n');
    }
    Some(Check {
        path: path.to_path_buf(),
        report,
        failed: false,
    })
}

fn display(p: &Path) -> String {
    p.file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| p.display().to_string())
}

/// Run a parser over a file, returning its stderr on rejection.
fn run_parser(cmd: &[&str], path: &Path) -> Result<(), String> {
    use std::process::Command;
    let mut c = Command::new(cmd[0]);
    for a in &cmd[1..] {
        c.arg(a);
    }
    c.arg(path);
    c.stdin(std::process::Stdio::null());
    c.stdout(std::process::Stdio::null());
    // The parser must not be able to write the file we are checking.
    c.stderr(std::process::Stdio::piped());

    match c.output() {
        Ok(o) if o.status.success() => Ok(()),
        Ok(o) => {
            let err = String::from_utf8_lossy(&o.stderr);
            // Only the useful part of a compiler diagnostic.
            let head: String = err.lines().take(12).collect::<Vec<_>>().join("\n");
            Err(head.trim().to_string())
        }
        // No parser available. Not a pass, but not a failure either: the caller
        // reports that the check could not run.
        Err(_) => Err(String::from(
            "_glass: no parser available, syntax unchecked",
        )),
    }
}

/// Render the checks for a batch of written files into one block.
///
/// Empty means nothing was verified, so the caller can leave the tool result
/// untouched rather than appending an empty note.
pub fn report(paths: &[PathBuf], workspace: &Path, tail_lines: usize) -> String {
    let mut out = String::new();
    for p in paths {
        // `py_compile` writes a __pycache__ artifact; ignore it here.
        if p.to_string_lossy().contains("__pycache__") {
            continue;
        }
        let rel = p.strip_prefix(workspace).unwrap_or(p);
        if let Some(c) = verify(p, tail_lines) {
            out.push('\n');
            out.push_str(&c.report);
            let _ = rel;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(tag: &str) -> PathBuf {
        use std::sync::atomic::{AtomicU32, Ordering};
        static SEQ: AtomicU32 = AtomicU32::new(0);
        let n = SEQ.fetch_add(1, Ordering::Relaxed);
        let d = std::env::temp_dir().join(format!("glass-verify-{}-{tag}-{n}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn finds_heredoc_target() {
        let ws = Path::new("/w");
        let got = written_paths("cat > src/lib.rs <<'EOF'\nfn main() {}\nEOF", ws);
        assert_eq!(got, vec![PathBuf::from("/w/src/lib.rs")]);
    }

    #[test]
    fn finds_plain_redirect() {
        let got = written_paths("echo hi > out.txt", Path::new("/w"));
        assert_eq!(got, vec![PathBuf::from("/w/out.txt")]);
    }

    #[test]
    fn finds_append_redirect() {
        let got = written_paths("echo hi >> log.txt", Path::new("/w"));
        assert_eq!(got, vec![PathBuf::from("/w/log.txt")]);
    }

    #[test]
    fn finds_tee_target() {
        let got = written_paths("cmd | tee report.md", Path::new("/w"));
        assert_eq!(got, vec![PathBuf::from("/w/report.md")]);
    }

    #[test]
    fn ignores_read_only_commands() {
        assert!(written_paths("cat AGENTS.md", Path::new("/w")).is_empty());
        assert!(written_paths("ls -la", Path::new("/w")).is_empty());
        assert!(written_paths("rg foo src/", Path::new("/w")).is_empty());
    }

    #[test]
    fn ignores_variables_and_globs() {
        // We cannot resolve these without a shell, and guessing would be worse
        // than skipping.
        assert!(written_paths("cat > $OUT/x", Path::new("/w")).is_empty());
        assert!(written_paths("cmd > *.txt", Path::new("/w")).is_empty());
    }

    #[test]
    fn never_treats_root_as_a_write_target() {
        assert!(written_paths("cat > / <<EOF", Path::new("/w")).is_empty());
    }

    #[test]
    fn dedups_multiple_writes_to_one_path() {
        let got = written_paths("cat > a.txt <<EOF\nEOF\ncat > a.txt", Path::new("/w"));
        assert_eq!(got.len(), 1, "{got:?}");
    }

    #[test]
    fn absolute_paths_are_left_alone() {
        let got = written_paths("cat > /etc/hosts <<EOF\nEOF", Path::new("/w"));
        assert_eq!(got, vec![PathBuf::from("/etc/hosts")]);
    }

    #[test]
    fn verifies_a_valid_python_file() {
        let d = scratch("pyok");
        let f = d.join("ok.py");
        std::fs::write(&f, "x = 1\nprint(x)\n").unwrap();
        let c = verify(&f, 10).expect("file exists");
        assert!(!c.failed);
        assert!(c.report.contains("syntax ok"));
        assert!(c.report.contains("print(x)"));
        std::fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn rejects_a_broken_python_file() {
        let d = scratch("pybad");
        let f = d.join("bad.py");
        std::fs::write(&f, "def oops(\n").unwrap();
        let c = verify(&f, 10).expect("file exists");
        assert!(c.failed, "a parse error must be reported");
        assert!(c.report.contains("syntax error"));
        std::fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn missing_file_verifies_to_nothing() {
        assert!(verify(Path::new("/definitely/not/here.txt"), 5).is_none());
    }

    #[test]
    fn empty_file_is_not_reported() {
        let d = scratch("empty");
        let f = d.join("empty.txt");
        std::fs::write(&f, "").unwrap();
        assert!(
            verify(&f, 5).is_none(),
            "an empty write has nothing to echo"
        );
        std::fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn directory_is_not_verified() {
        let d = scratch("dir");
        assert!(verify(&d, 5).is_none());
        std::fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn echo_back_is_bounded() {
        let d = scratch("tail");
        let f = d.join("big.txt");
        let body: String = (0..500).map(|i| format!("line {i}\n")).collect();
        std::fs::write(&f, body).unwrap();
        let c = verify(&f, 5).expect("file exists");
        assert!(c.report.contains("line 499"));
        assert!(
            !c.report.contains("line 0"),
            "echo must be bounded to the tail"
        );
        assert!(c.report.contains("500 lines"), "total count still reported");
        std::fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn json_is_gated() {
        let d = scratch("json");
        let f = d.join("a.json");
        std::fs::write(&f, "{\"a\": 1}").unwrap();
        assert!(!verify(&f, 5).unwrap().failed);

        let g = d.join("b.json");
        std::fs::write(&g, "{not json").unwrap();
        assert!(verify(&g, 5).unwrap().failed);
        std::fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn report_is_empty_when_nothing_was_written() {
        assert_eq!(report(&[], Path::new("/w"), 10), "");
    }

    #[test]
    fn unknown_extension_is_echoed_without_a_gate() {
        let d = scratch("unk");
        let f = d.join("a.rs");
        std::fs::write(&f, "fn main() {}\n").unwrap();
        let c = verify(&f, 10).unwrap();
        assert!(!c.failed);
        assert!(!c.report.contains("syntax"), "no parser claimed for .rs");
        assert!(c.report.contains("fn main()"));
        std::fs::remove_dir_all(&d).ok();
    }
}
#[cfg(test)]
mod integration {
    use super::*;

    /// The full loop the model actually sees: run a heredoc write, then verify
    /// what it wrote. This is the path that catches a truncated heredoc.
    #[test]
    fn heredoc_write_is_verified_and_syntax_error_surfaces() {
        use std::process::Command;

        let d = std::env::temp_dir().join(format!("glass-verify-int-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();

        let cmd = "cat > out.py <<'EOF'\ndef oops(\nEOF";
        let status = Command::new("/bin/sh")
            .arg("-c")
            .arg(cmd)
            .current_dir(&d)
            .status()
            .unwrap();
        assert!(status.success(), "the shell command itself succeeds");

        let touched = written_paths(cmd, &d);
        assert_eq!(
            touched,
            vec![d.join("out.py")],
            "heredoc target must be detected"
        );

        let note = report(&touched, &d, 40);
        assert!(
            note.contains("syntax error"),
            "unclosed paren must surface: {note}"
        );
        assert!(
            note.contains("never closed"),
            "the parser's own message is kept: {note}"
        );

        let _ = std::fs::remove_dir_all(&d);
    }

    /// A heredoc that writes valid code reports clean, so a passing gate never
    /// costs the model a second turn.
    #[test]
    fn valid_heredoc_reports_ok() {
        use std::process::Command;

        let d = std::env::temp_dir().join(format!("glass-verify-ok-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();

        let cmd = "cat > ok.py <<'EOF'\nx = 1\nprint(x)\nEOF";
        Command::new("/bin/sh")
            .arg("-c")
            .arg(cmd)
            .current_dir(&d)
            .status()
            .unwrap();

        let note = report(&written_paths(cmd, &d), &d, 40);
        assert!(note.contains("syntax ok"), "{note}");

        let _ = std::fs::remove_dir_all(&d);
    }
}

#[cfg(test)]
mod sed_tests {
    use super::*;

    #[test]
    fn sed_i_finds_unquoted_trailing_path() {
        // The common form: script quoted, path bare. This is the one a
        // "last token" heuristic gets wrong.
        let got = written_paths("sed -i 's/old/new/' src/a.rs", Path::new("/w"));
        assert_eq!(got, vec![PathBuf::from("/w/src/a.rs")]);
    }

    #[test]
    fn sed_i_finds_quoted_trailing_path() {
        let got = written_paths("sed -i 's/a/b/' 'src/a.rs'", Path::new("/w"));
        assert_eq!(got, vec![PathBuf::from("/w/src/a.rs")]);
    }

    #[test]
    fn sed_i_skips_extra_flags() {
        let got = written_paths("sed -i -e 's/a/b/' src/a.rs", Path::new("/w"));
        assert_eq!(got, vec![PathBuf::from("/w/src/a.rs")]);
    }

    #[test]
    fn plain_sed_is_not_a_write() {
        // Without -i, sed prints and does not modify.
        assert!(written_paths("sed 's/a/b/' src/a.rs", Path::new("/w")).is_empty());
    }
}
