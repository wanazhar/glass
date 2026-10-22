//! Read-only planning surface: filesystem reads, no shell or extension dispatch.
use crate::tools::Outcome;
use glass_mod_api::ToolCall;
use std::path::{Path, PathBuf};

pub const PROMPT: &str = "You are planning a coding task. Inspect the project with the inspect tool. You cannot execute commands, write files, use extensions, or delegate work. Produce a concrete numbered plan with files to change, verification steps, and risks. Do not claim implementation or tests are complete. A user must explicitly choose execution afterwards.";
pub fn schema() -> Vec<(String, String, String)> {
    vec![("inspect".into(), "Read-only project inspection. action=list, read, or search; path is relative to the workspace; query is literal text.".into(), serde_json::json!({"type":"object","properties":{"action":{"type":"string","enum":["list","read","search"]},"path":{"type":"string"},"query":{"type":"string"}},"required":["action"]}).to_string())]
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
}
