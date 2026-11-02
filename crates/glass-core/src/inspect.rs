//! Read-only planning surface: filesystem reads, no shell or extension dispatch.
use crate::tools::Outcome;
use glass_mod_api::ToolCall;
use std::path::{Path, PathBuf};

pub const PROMPT: &str = "You are planning a coding task. Inspect the project with the inspect tool. You cannot execute commands, write files, use extensions, or delegate writing work. Delegate read-only subtasks only when the teammate tool is advertised. Produce a concrete numbered plan with files to change, verification steps, and risks. Do not claim implementation or tests are complete. A user must explicitly choose execution afterwards.";
pub fn schema() -> Vec<(String, String, String)> {
    vec![("inspect".into(), "Read-only project inspection. action=list/read/search for files, system for OS/architecture/current Unix time, git_status for verified Git status. No arbitrary shell, writes, desktop startup, or permission escalation. Git status is evidence; a .git path alone is not. Paths stay within the workspace; query is literal text.".into(), serde_json::json!({"type":"object","properties":{"action":{"type":"string","enum":["list","read","search","system","git_status"]},"path":{"type":"string"},"query":{"type":"string"}},"required":["action"]}).to_string())]
}
fn scoped(root: &Path, relative: &str) -> Result<PathBuf, String> {
    let root = root.canonicalize().map_err(|e| e.to_string())?;
    let path = root
        .join(relative)
        .canonicalize()
        .map_err(|e| e.to_string())?;
    if !path.starts_with(&root) {
        return Err("Path escapes the workspace".into());
    }
    Ok(path)
}
fn walk(path: &Path, files: &mut Vec<PathBuf>, visited: &mut usize, depth: usize) {
    if *visited >= 5_000 || depth > 12 {
        return;
    }
    let Ok(entries) = std::fs::read_dir(path) else {
        return;
    };
    let mut entries = entries.filter_map(Result::ok).collect::<Vec<_>>();
    entries.sort_by_key(|entry| entry.file_name());
    for entry in entries {
        *visited += 1;
        if *visited > 5_000 {
            break;
        }
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name.starts_with('.')
            || matches!(
                name.as_ref(),
                "target" | "node_modules" | "__pycache__" | "dist"
            )
        {
            continue;
        }
        let Ok(kind) = entry.file_type() else {
            continue;
        };
        if kind.is_file() {
            files.push(entry.path());
        } else if kind.is_dir() {
            walk(&entry.path(), files, visited, depth + 1);
        }
    }
}
pub fn run(call: &ToolCall, workspace: &Path) -> Outcome {
    let result = (|| -> Result<String, String> {
        let path = scoped(
            workspace,
            &call.arg_str("path").unwrap_or_else(|| ".".into()),
        )?;
        let root = workspace.canonicalize().map_err(|e| e.to_string())?;
        match call.arg_str("action").as_deref() {
            Some("system") => Ok(serde_json::json!({"os":std::env::consts::OS,"architecture":std::env::consts::ARCH,"utc_time":httpdate::fmt_http_date(std::time::SystemTime::now()),"unix_time_seconds":std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_err(|e|e.to_string())?.as_secs(),"workspace":root,"evidence":"host system APIs; no shell executed"}).to_string()),
            Some("git_status") => {
                if !path.is_dir() { return Err("Git inspection requires a directory".into()); }
                let args = ["--no-optional-locks", "-c", "core.hooksPath=/dev/null", "rev-parse", "--show-toplevel"];
                let top = crate::workflows::git(&path, &args, None)
                    .map_err(|e| format!("Git repository verification failed; a .git path alone is not evidence: {e}"))?;
                let status = crate::workflows::git(&path, &["--no-optional-locks", "-c", "core.hooksPath=/dev/null", "status", "--porcelain=v1", "--untracked-files=normal", "--", "."], None)?;
                Ok(serde_json::json!({"verified":true,"repository":String::from_utf8_lossy(&top).trim(),"scope":path,"status_porcelain":String::from_utf8_lossy(&status),"evidence":"git rev-parse and git status succeeded; optional index writes, hooks and filesystem monitors disabled"}).to_string())
            }
            Some("read") => {
                if !path.is_file() {
                    return Err("Choose a regular text file".into());
                }
                // A cap is enforced while reading, not after allocating the whole file.
                use std::io::Read;
                let mut bytes = Vec::new();
                std::fs::File::open(&path)
                    .map_err(|e| e.to_string())?
                    .take(32_769)
                    .read_to_end(&mut bytes)
                    .map_err(|e| e.to_string())?;
                let truncated = bytes.len() > 32_768;
                bytes.truncate(32_768);
                let text = String::from_utf8_lossy(&bytes);
                Ok(format!(
                    "{}\n{}{}",
                    path.strip_prefix(&root).unwrap_or(&path).display(),
                    text,
                    if truncated {
                        "\n[read capped at 32 KiB]"
                    } else {
                        ""
                    }
                ))
            }
            Some("list" | "search") => {
                let mut files = Vec::new();
                let mut visited = 0;
                if path.is_file() {
                    files.push(path);
                } else {
                    walk(&path, &mut files, &mut visited, 0);
                }
                let search = call.arg_str("action").as_deref() == Some("search");
                let query = call.arg_str("query").unwrap_or_default();
                if search && query.is_empty() {
                    return Err("Search needs a nonempty literal query".into());
                }
                let mut rows = Vec::new();
                for file in files.into_iter().take(1_000) {
                    let label = file
                        .strip_prefix(&root)
                        .unwrap_or(&file)
                        .display()
                        .to_string();
                    if !search {
                        rows.push(label);
                    } else if std::fs::metadata(&file).is_ok_and(|m| m.len() <= 32_768)
                        && let Ok(text) = std::fs::read_to_string(&file)
                    {
                        for (line, text) in
                            text.lines().enumerate().filter(|(_, t)| t.contains(&query))
                        {
                            rows.push(format!(
                                "{label}:{}: {}",
                                line + 1,
                                glass_tui::text::clip(text, 300)
                            ));
                            if rows.len() >= 100 {
                                break;
                            }
                        }
                    }
                    if rows.len() >= 100 && search {
                        break;
                    }
                }
                Ok(format!(
                    "{}\n[bounded inspection; hidden/build directories omitted]",
                    rows.join("\n")
                ))
            }
            _ => Err("Unknown inspection action".into()),
        }
    })();
    match result {
        Ok(body) => Outcome {
            body,
            truncated: false,
            exit_code: Some(0),
            spill: None,
        },
        Err(error) => Outcome::err(error),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn scoped_paths_cannot_escape() {
        let root = std::env::temp_dir().join(format!("gc-inspect-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        assert!(scoped(&root, "..").is_err());
        std::fs::remove_dir(&root).unwrap();
    }
    #[test]
    fn git_inspection_rejects_stub_and_does_not_refresh_index_or_run_monitor() {
        let root = std::env::temp_dir().join(format!("gc-inspect-git-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join(".git")).unwrap();
        let call = ToolCall {
            id: "git".into(),
            name: "inspect".into(),
            arguments: r#"{"action":"git_status"}"#.into(),
        };
        assert_ne!(run(&call, &root).exit_code, Some(0));
        std::fs::remove_dir_all(root.join(".git")).unwrap();
        assert!(
            std::process::Command::new("git")
                .args(["init", "-q"])
                .current_dir(&root)
                .status()
                .unwrap()
                .success()
        );
        std::fs::write(root.join("evidence.txt"), "verified content").unwrap();
        assert!(
            std::process::Command::new("git")
                .args(["add", "evidence.txt"])
                .current_dir(&root)
                .status()
                .unwrap()
                .success()
        );
        assert!(
            std::process::Command::new("git")
                .args(["config", "core.fsmonitor", "touch monitor-ran"])
                .current_dir(&root)
                .status()
                .unwrap()
                .success()
        );
        let before = std::fs::read(root.join(".git/index")).unwrap();
        let result = run(&call, &root);
        assert_eq!(result.exit_code, Some(0), "{}", result.body);
        let evidence: serde_json::Value = serde_json::from_str(&result.body).unwrap();
        assert_eq!(evidence["verified"], true);
        assert!(
            evidence["status_porcelain"]
                .as_str()
                .unwrap()
                .contains("evidence.txt")
        );
        assert_eq!(before, std::fs::read(root.join(".git/index")).unwrap());
        assert!(!root.join("monitor-ran").exists());
        let system = run(
            &ToolCall {
                id: "sys".into(),
                name: "inspect".into(),
                arguments: r#"{"action":"system"}"#.into(),
            },
            &root,
        );
        assert!(
            serde_json::from_str::<serde_json::Value>(&system.body).unwrap()["unix_time_seconds"]
                .as_u64()
                .unwrap()
                > 0
        );
        std::fs::remove_dir_all(root).unwrap();
    }
}
