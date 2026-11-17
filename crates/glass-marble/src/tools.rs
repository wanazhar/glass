//! Governed host tools the marble process owns and executes itself.
//!
//! Core executes its native coding tools in a mixed session; only these
//! `glass.*` integrations come back over `tool/execute`. Every path is resolved
//! against one workspace root, so an approved tool cannot reach outside it.

use crate::MarbleError;
use glass_runtime_client::ToolDefinition;
use serde_json::{Value, json};
use std::fs;
use std::path::{Component, Path, PathBuf};

pub const WORKSPACE_LIST: &str = "glass.workspace.list";
pub const WORKSPACE_READ: &str = "glass.workspace.read";
pub const WORKSPACE_WRITE: &str = "glass.workspace.write";

/// Whether one governed tool changes the workspace.
pub fn is_write_tool(name: &str) -> bool {
    name == WORKSPACE_WRITE
}

/// Largest file the read tool returns in one call.
pub const MAX_READ_BYTES: usize = 256 * 1024;
/// Largest payload the write tool accepts in one call.
pub const MAX_WRITE_BYTES: usize = 256 * 1024;
/// Bounded directory listing.
pub const MAX_LIST_ENTRIES: usize = 500;

/// Result of executing one governed host tool.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolOutcome {
    pub text: String,
    pub is_error: bool,
}

impl ToolOutcome {
    pub fn ok(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            is_error: false,
        }
    }

    pub fn error(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            is_error: true,
        }
    }
}

/// Workspace-scoped tool executor owned by the marble host.
#[derive(Debug, Clone)]
pub struct ToolRouter {
    root: PathBuf,
}

impl ToolRouter {
    /// Canonicalize the workspace root once so later path checks compare like
    /// with like.
    pub fn new(root: impl AsRef<Path>) -> Result<Self, MarbleError> {
        let root = root.as_ref().canonicalize().map_err(MarbleError::Io)?;
        Ok(Self { root })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Tool schemas registered with the runtime for a mixed session.
    pub fn catalog() -> Vec<ToolDefinition> {
        vec![
            ToolDefinition {
                name: WORKSPACE_LIST.into(),
                description: "List one workspace directory. Paths are relative to the \
                     workspace root and may not leave it."
                    .into(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "path": {"type": "string", "description": "Directory relative to the workspace root."}
                    },
                    "additionalProperties": false
                }),
            },
            ToolDefinition {
                name: WORKSPACE_READ.into(),
                description: "Read a UTF-8 workspace file. Large files are truncated.".into(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "path": {"type": "string", "description": "File relative to the workspace root."}
                    },
                    "required": ["path"],
                    "additionalProperties": false
                }),
            },
            ToolDefinition {
                name: WORKSPACE_WRITE.into(),
                description: "Write a UTF-8 workspace file, creating parent directories. \
                     Replaces the whole file."
                    .into(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "path": {"type": "string", "description": "File relative to the workspace root."},
                        "content": {"type": "string", "description": "Complete new file content."}
                    },
                    "required": ["path", "content"],
                    "additionalProperties": false
                }),
            },
        ]
    }

    /// Execute one tool call. Unknown tools fail instead of silently succeeding.
    pub fn execute(&self, name: &str, arguments: &Value) -> ToolOutcome {
        match name {
            WORKSPACE_LIST => self.list(arguments),
            WORKSPACE_READ => self.read(arguments),
            WORKSPACE_WRITE => self.write(arguments),
            other => ToolOutcome::error(format!("host has no tool named {other}")),
        }
    }

    /// Resolve a tool argument inside the workspace root, rejecting absolute
    /// paths, parent traversal, and symlinks that escape the root.
    fn resolve(&self, arguments: &Value) -> Result<PathBuf, ToolOutcome> {
        let raw = match arguments.get("path") {
            None | Some(Value::Null) => ".".to_string(),
            Some(Value::String(path)) if !path.trim().is_empty() => path.clone(),
            _ => return Err(ToolOutcome::error("path must be a nonempty string")),
        };
        let relative = Path::new(&raw);
        if relative.is_absolute() {
            return Err(ToolOutcome::error(format!(
                "path {raw} must be relative to the workspace"
            )));
        }
        for component in relative.components() {
            match component {
                Component::Normal(_) | Component::CurDir => {}
                _ => {
                    return Err(ToolOutcome::error(format!(
                        "path {raw} must not leave the workspace"
                    )));
                }
            }
        }
        let joined = self.root.join(relative);
        // A write target may not exist yet: validate the nearest existing
        // ancestor instead, so symlinked directories cannot escape the root.
        let mut existing = joined.as_path();
        while !existing.exists() {
            existing = existing.parent().ok_or_else(|| {
                ToolOutcome::error(format!("cannot resolve {raw} inside the workspace"))
            })?;
        }
        let canonical = existing
            .canonicalize()
            .map_err(|error| ToolOutcome::error(format!("cannot resolve {raw}: {error}")))?;
        if !canonical.starts_with(&self.root) {
            return Err(ToolOutcome::error(format!(
                "path {raw} resolves outside the workspace"
            )));
        }
        Ok(joined)
    }

    fn display(&self, path: &Path) -> String {
        path.strip_prefix(&self.root)
            .unwrap_or(path)
            .display()
            .to_string()
    }

    fn list(&self, arguments: &Value) -> ToolOutcome {
        let path = match self.resolve(arguments) {
            Ok(path) => path,
            Err(outcome) => return outcome,
        };
        let entries = match fs::read_dir(&path) {
            Ok(entries) => entries,
            Err(error) => {
                return ToolOutcome::error(format!("cannot list {}: {error}", self.display(&path)));
            }
        };
        let base = self.display(&path);
        let mut lines = Vec::new();
        let mut truncated = false;
        let mut names: Vec<(String, bool)> = Vec::new();
        for entry in entries {
            let Ok(entry) = entry else {
                continue;
            };
            let is_dir = entry.file_type().map(|kind| kind.is_dir()).unwrap_or(false);
            let name = entry.file_name().to_string_lossy().to_string();
            names.push((name, is_dir));
        }
        names.sort();
        for (name, is_dir) in names {
            if lines.len() >= MAX_LIST_ENTRIES {
                truncated = true;
                break;
            }
            lines.push(if is_dir { format!("{name}/") } else { name });
        }
        let mut text = if lines.is_empty() {
            format!("{base} is empty")
        } else {
            format!("{base}:\n{}", lines.join("\n"))
        };
        if truncated {
            text.push_str(&format!(
                "\n(listing stopped at {MAX_LIST_ENTRIES} entries)"
            ));
        }
        ToolOutcome::ok(text)
    }

    fn read(&self, arguments: &Value) -> ToolOutcome {
        let path = match self.resolve(arguments) {
            Ok(path) => path,
            Err(outcome) => return outcome,
        };
        let metadata = match fs::metadata(&path) {
            Ok(metadata) => metadata,
            Err(error) => {
                return ToolOutcome::error(format!("cannot read {}: {error}", self.display(&path)));
            }
        };
        if metadata.is_dir() {
            return ToolOutcome::error(format!(
                "{} is a directory; use {WORKSPACE_LIST}",
                self.display(&path)
            ));
        }
        let bytes = match fs::read(&path) {
            Ok(bytes) => bytes,
            Err(error) => {
                return ToolOutcome::error(format!("cannot read {}: {error}", self.display(&path)));
            }
        };
        let truncated = bytes.len() > MAX_READ_BYTES;
        let slice = &bytes[..bytes.len().min(MAX_READ_BYTES)];
        let slice = match std::str::from_utf8(slice) {
            Ok(slice) => slice,
            Err(error) => {
                let boundary = error.valid_up_to();
                std::str::from_utf8(&slice[..boundary]).unwrap_or_default()
            }
        };
        let mut text = format!("{}:\n{slice}", self.display(&path));
        if truncated {
            text.push_str(&format!("\n(truncated at {MAX_READ_BYTES} bytes)"));
        }
        ToolOutcome::ok(text)
    }

    fn write(&self, arguments: &Value) -> ToolOutcome {
        let Some(content) = arguments.get("content").and_then(Value::as_str) else {
            return ToolOutcome::error("content must be a string");
        };
        if content.len() > MAX_WRITE_BYTES {
            return ToolOutcome::error(format!(
                "content exceeds the {MAX_WRITE_BYTES} byte tool limit"
            ));
        }
        let path = match self.resolve(arguments) {
            Ok(path) => path,
            Err(outcome) => return outcome,
        };
        if path.exists() && path.is_dir() {
            return ToolOutcome::error(format!("{} is a directory", self.display(&path)));
        }
        if let Some(parent) = path.parent()
            && let Err(error) = fs::create_dir_all(parent)
        {
            return ToolOutcome::error(format!("cannot create {}: {error}", self.display(parent)));
        }
        if !path.starts_with(&self.root) {
            return ToolOutcome::error("refusing to write outside the workspace");
        }
        match fs::write(&path, content) {
            Ok(()) => ToolOutcome::ok(format!(
                "wrote {} bytes to {}",
                content.len(),
                self.display(&path)
            )),
            Err(error) => {
                ToolOutcome::error(format!("cannot write {}: {error}", self.display(&path)))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn workspace(name: &str) -> PathBuf {
        let root =
            std::env::temp_dir().join(format!("glass-marble-tools-{}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        root
    }

    #[test]
    fn list_and_read_stay_inside_the_workspace() {
        let root = workspace("list");
        fs::create_dir_all(root.join("nested")).unwrap();
        fs::write(root.join("nested/notes.txt"), "hello marble").unwrap();
        let router = ToolRouter::new(&root).unwrap();

        let listing = router.execute(WORKSPACE_LIST, &json!({}));
        assert!(!listing.is_error);
        assert!(listing.text.contains("nested/"));

        let read = router.execute(WORKSPACE_READ, &json!({"path": "nested/notes.txt"}));
        assert!(!read.is_error);
        assert!(read.text.contains("hello marble"));

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn traversal_and_absolute_paths_are_rejected() {
        let root = workspace("traversal");
        let router = ToolRouter::new(&root).unwrap();
        for arguments in [
            json!({"path": "../escape.txt"}),
            json!({"path": "/etc/passwd"}),
            json!({"path": "nested/../../escape.txt"}),
        ] {
            let outcome = router.execute(WORKSPACE_READ, &arguments);
            assert!(outcome.is_error, "{arguments} should be refused");
        }
        assert!(
            router
                .execute(WORKSPACE_WRITE, &json!({"path": "../x", "content": "x"}))
                .is_error
        );
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn write_creates_parents_and_reports_size() {
        let root = workspace("write");
        let router = ToolRouter::new(&root).unwrap();
        let outcome = router.execute(
            WORKSPACE_WRITE,
            &json!({"path": "deep/file.txt", "content": "durable"}),
        );
        assert!(!outcome.is_error, "{}", outcome.text);
        assert_eq!(
            fs::read_to_string(root.join("deep/file.txt")).unwrap(),
            "durable"
        );
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn unknown_tool_is_an_error_not_a_silent_success() {
        let root = workspace("unknown");
        let router = ToolRouter::new(&root).unwrap();
        let outcome = router.execute("glass.not.real", &json!({}));
        assert!(outcome.is_error);
        assert!(outcome.text.contains("no tool named"));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn catalog_is_namespaced_for_mixed_sessions() {
        for tool in ToolRouter::catalog() {
            assert!(tool.name.starts_with("glass."));
            assert_eq!(tool.input_schema["type"], json!("object"));
        }
    }
}
