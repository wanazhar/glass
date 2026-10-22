//! `gc worktree`: find and reclaim the work fan-out left behind.
//!
//! Delegated tasks with `worktree: true` run in detached checkouts under the
//! data directory. Until now the only way back was manual archaeology; this
//! module is the return path: list what exists, apply its diff onto the
//! workspace (working tree only — committing stays `/git`'s job), and
//! optionally remove the checkout.
//!
//! Diffs are taken with `git add -A` inside the worktree first, so files the
//! subagent created are included; the worktree's own index is disposable.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use crate::config;

/// Where `run_task` creates isolated checkouts.
pub fn worktrees_root() -> PathBuf {
    config::data_dir().join("subagents").join("worktrees")
}

fn git(dir: &Path, args: &[&str]) -> Result<std::process::Output, String> {
    Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .output()
        .map_err(|e| format!("git: {e}"))
}

fn git_ok(dir: &Path, args: &[&str]) -> Result<String, String> {
    let out = git(dir, args)?;
    if !out.status.success() {
        return Err(format!(
            "git {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

/// Pick the checkout to operate on: an explicit name/path, else the newest.
fn resolve(target: Option<&str>) -> Result<PathBuf, String> {
    let root = worktrees_root();
    if let Some(target) = target {
        let direct = PathBuf::from(target);
        let candidate = if direct.is_dir() {
            direct
        } else {
            root.join(target)
        };
        if !candidate.is_dir() {
            return Err(format!("no worktree at {}", candidate.display()));
        }
        return candidate.canonicalize().map_err(|e| e.to_string());
    }
    let newest = std::fs::read_dir(&root)
        .map_err(|_| format!("no worktrees under {}", root.display()))?
        .filter_map(Result::ok)
        .filter(|e| e.file_type().map(|t| t.is_dir()).unwrap_or(false))
        .max_by_key(|e| e.metadata().ok().and_then(|m| m.modified().ok()))
        .map(|e| e.path());
    match newest {
        Some(path) => path.canonicalize().map_err(|e| e.to_string()),
        None => Err(format!("no worktrees under {}", root.display())),
    }
}

/// Apply the checkout's diff onto `workspace` (working tree, no commit).
///
/// Returns a human summary on success. Conflicts from `--3way` are left in
/// place with git's markers and reported as errors — silently choosing sides
/// would be worse than refusing.
pub fn apply_patch(workspace: &Path, checkout: &Path, remove: bool) -> Result<String, String> {
    let common_a = git_ok(checkout, &["rev-parse", "--git-common-dir"])?;
    let common_b = git_ok(workspace, &["rev-parse", "--git-common-dir"])?;
    let resolve_common = |base: &Path, p: &str| -> Result<PathBuf, String> {
        let path = Path::new(p.trim());
        let path = if path.is_absolute() {
            path.to_path_buf()
        } else {
            base.join(path)
        };
        path.canonicalize()
            .map_err(|e| format!("repository identity: {e}"))
    };
    if resolve_common(checkout, &common_a)? != resolve_common(workspace, &common_b)? {
        return Err("the worktree belongs to a different repository".into());
    }

    // Stage everything inside the checkout so new files are part of the diff,
    // then take a binary-safe patch against its HEAD.
    git_ok(checkout, &["add", "-A"])?;
    let out = git(checkout, &["diff", "--cached", "--binary", "HEAD"])?;
    if !out.status.success() {
        return Err(format!(
            "git diff failed: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    let patch = out.stdout;
    if patch.is_empty() {
        return Ok("nothing to apply (the checkout has no changes)".into());
    }
    let files = git_ok(checkout, &["diff", "--cached", "--name-only", "HEAD"])
        .map(|s| s.lines().count())
        .unwrap_or(0);

    let mut child = Command::new("git")
        .arg("-C")
        .arg(workspace)
        .args(["apply", "--3way", "-"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("git apply: {e}"))?;
    {
        use std::io::Write;
        let stdin = child.stdin.as_mut().ok_or("git apply stdin unavailable")?;
        stdin.write_all(&patch).map_err(|e| e.to_string())?;
    }
    let out = child.wait_with_output().map_err(|e| e.to_string())?;
    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr);
        return Err(format!(
            "apply did not land cleanly; resolve the conflicts and retry (git said: {})",
            stderr.trim()
        ));
    }

    let mut summary = format!(
        "applied {files} changed file(s) from {}; review with /git, commit when ready",
        checkout.display()
    );
    if remove {
        let out = Command::new("git")
            .arg("-C")
            .arg(workspace)
            .args(["worktree", "remove", "--force"])
            .arg(checkout)
            .output();
        match out {
            Ok(o) if o.status.success() => {
                let _ = git(workspace, &["worktree", "prune"]);
                summary.push_str("; checkout removed");
            }
            Ok(o) => summary.push_str(&format!(
                "; could not remove the checkout ({}); delete it manually",
                String::from_utf8_lossy(&o.stderr).trim()
            )),
            Err(e) => summary.push_str(&format!("; remove failed: {e}")),
        }
    }
    Ok(summary)
}

/// `gc worktree …` entry point.
pub fn command(args: &[String]) -> Result<(), String> {
    let workspace = std::env::current_dir().map_err(|e| e.to_string())?;
    match args.first().map(String::as_str) {
        Some("new") => {
            let name = args
                .get(1)
                .ok_or("gc worktree new NAME [BASE] [--branch BRANCH]")?;
            if name.is_empty()
                || name.len() > 80
                || !name
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
            {
                return Err(
                    "Use a worktree name of 1–80 letters, digits, hyphens or underscores".into(),
                );
            }
            let mut base = "HEAD";
            let mut branch = None;
            let mut i = 2;
            while i < args.len() {
                if args[i] == "--branch" && branch.is_none() {
                    i += 1;
                    let value = args.get(i).ok_or("--branch needs a name")?;
                    git_ok(&workspace, &["check-ref-format", "--branch", value])?;
                    branch = Some(value.as_str());
                } else if i == 2 && !args[i].starts_with('-') {
                    base = &args[i];
                } else {
                    return Err("gc worktree new NAME [BASE] [--branch BRANCH]".into());
                }
                i += 1;
            }
            let revision = git_ok(
                &workspace,
                &[
                    "rev-parse",
                    "--verify",
                    "--end-of-options",
                    &format!("{base}^{{commit}}"),
                ],
            )?;
            let root = worktrees_root();
            std::fs::create_dir_all(&root).map_err(|e| e.to_string())?;
            let checkout = root.join(name);
            if checkout.exists() {
                return Err(
                    "Worktree already exists; use gc worktree show NAME to inspect and reuse it"
                        .into(),
                );
            }
            let mut command = Command::new("git");
            command.arg("-C").arg(&workspace).args(["worktree", "add"]);
            if let Some(branch) = branch {
                command.args(["-b", branch]);
            } else {
                command.arg("--detach");
            }
            let out = command
                .arg(&checkout)
                .arg(revision.trim())
                .output()
                .map_err(|e| e.to_string())?;
            if !out.status.success() {
                return Err(String::from_utf8_lossy(&out.stderr).trim().into());
            }
            crate::cli::report(
                "Task worktree",
                &serde_json::json!({"path":checkout,"base":revision.trim(),"branch":branch.unwrap_or("detached"),"next":"Run gc from this directory. The parent working tree is unchanged."}),
            );
            Ok(())
        }
        Some("show" | "cleanup-preview") => {
            if args.len() != 2 {
                return Err("gc worktree show|cleanup-preview NAME".into());
            }
            let checkout = resolve(Some(&args[1]))?;
            let status = git_ok(&checkout, &["status", "--porcelain"])?;
            let head = git_ok(&checkout, &["rev-parse", "HEAD"])?;
            let branch = git_ok(&checkout, &["branch", "--show-current"])?;
            crate::cli::report(
                "Worktree preview",
                &serde_json::json!({"path":checkout,"head":head.trim(),"branch":branch.trim(),"changes":status,"clean":status.is_empty(),"removed":false,"cleanup":"Preview only. Check active agents and preserve commits before explicitly removing a worktree with Git."}),
            );
            Ok(())
        }
        Some("list") | None => {
            let json = args.iter().any(|a| a == "--json");
            let root = worktrees_root();
            let mut rows = Vec::new();
            if let Ok(entries) = std::fs::read_dir(&root) {
                for entry in entries.flatten() {
                    if !entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                        continue;
                    }
                    let path = entry.path();
                    let modified = entry
                        .metadata()
                        .ok()
                        .and_then(|m| m.modified().ok())
                        .map(crate::format_time)
                        .unwrap_or_else(|| "?".into());
                    let dirty = git_ok(&path, &["status", "--porcelain"])
                        .map(|s| s.lines().count())
                        .unwrap_or(0);
                    rows.push((path, modified, dirty));
                }
            }
            if json {
                let rows: Vec<serde_json::Value> = rows
                    .iter()
                    .map(|(path, modified, dirty)| {
                        serde_json::json!({
                            "path": path.display().to_string(),
                            "name": path.file_name().map(|n| n.to_string_lossy().into_owned()),
                            "modified": modified,
                            "changed_files": dirty,
                        })
                    })
                    .collect();
                crate::emit_json(serde_json::json!({"worktrees": rows}));
            } else if rows.is_empty() {
                crate::cli_println!("no worktrees under {}", root.display());
            } else {
                rows.sort_by(|a, b| a.1.cmp(&b.1));
                for (path, modified, dirty) in rows {
                    crate::cli_println!(
                        "{} · {dirty} changed file(s) · {modified}\n  {}",
                        path.file_name()
                            .map(|n| n.to_string_lossy().into_owned())
                            .unwrap_or_default(),
                        path.display()
                    );
                }
            }
            Ok(())
        }
        Some("apply") => {
            let remove = args.iter().any(|a| a == "--remove");
            let target = args
                .iter()
                .skip(1)
                .find(|a| !a.starts_with("--"))
                .map(String::as_str);
            let checkout = resolve(target)?;
            let summary = apply_patch(&workspace, &checkout, remove)?;
            crate::cli_println!("{summary}");
            Ok(())
        }
        _ => Err(
            "usage: glass-core worktree list [--json] | worktree apply [<name>] [--remove]".into(),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn git_available() -> bool {
        Command::new("git")
            .arg("--version")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    }

    fn run(dir: &Path, args: &[&str]) {
        let out = Command::new("git")
            .arg("-C")
            .arg(dir)
            .args(args)
            .env("GIT_AUTHOR_NAME", "t")
            .env("GIT_AUTHOR_EMAIL", "t@t")
            .env("GIT_COMMITTER_NAME", "t")
            .env("GIT_COMMITTER_EMAIL", "t@t")
            .output()
            .expect("git");
        assert!(out.status.success(), "git {args:?}: {:?}", out);
    }

    #[test]
    fn apply_brings_back_edits_and_new_files() {
        if !git_available() {
            return;
        }
        let base = std::env::temp_dir().join(format!("gc-worktree-{}", std::process::id()));
        let repo = base.join("repo");
        let wts = base.join("wts");
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(&repo).unwrap();
        run(&repo, &["init", "-q"]);
        std::fs::write(repo.join("a.txt"), "one\n").unwrap();
        run(&repo, &["add", "-A"]);
        run(&repo, &["commit", "-qm", "init"]);

        let checkout = crate::subagent::worktree(&repo, &wts, "t1").expect("worktree");
        std::fs::write(checkout.join("a.txt"), "two\n").unwrap();
        std::fs::write(checkout.join("new.txt"), "fresh\n").unwrap();

        let summary = apply_patch(&repo, &checkout, true).expect("apply");
        assert!(summary.contains("applied"), "{summary}");
        assert_eq!(
            std::fs::read_to_string(repo.join("a.txt")).unwrap(),
            "two\n"
        );
        assert_eq!(
            std::fs::read_to_string(repo.join("new.txt")).unwrap(),
            "fresh\n"
        );
        assert!(!checkout.exists(), "checkout removed on request");
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn a_clean_checkout_reports_nothing_to_apply() {
        if !git_available() {
            return;
        }
        let base = std::env::temp_dir().join(format!("gc-worktree-clean-{}", std::process::id()));
        let repo = base.join("repo");
        let wts = base.join("wts");
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(&repo).unwrap();
        run(&repo, &["init", "-q"]);
        std::fs::write(repo.join("a.txt"), "one\n").unwrap();
        run(&repo, &["add", "-A"]);
        run(&repo, &["commit", "-qm", "init"]);
        let checkout = crate::subagent::worktree(&repo, &wts, "t1").expect("worktree");
        let summary = apply_patch(&repo, &checkout, false).expect("apply");
        assert!(summary.contains("nothing to apply"), "{summary}");
        let _ = std::fs::remove_dir_all(&base);
    }
}
