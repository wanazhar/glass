//! Governed host tools the marble process owns and executes itself.
//!
//! Core executes its native coding tools in a mixed session; only these
//! `glass.*` integrations come back over `tool/execute`. Every path is resolved
//! against one workspace root, so an approved tool cannot reach outside it.

use crate::MarbleError;
use crate::crates::{CRATES_TOOL, CrateMap};
use glass_runtime_client::ToolDefinition;
use serde_json::{Value, json};
use std::collections::HashMap;
use std::fs;
use std::hash::{Hash, Hasher};
use std::path::{Component, Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

pub const WORKSPACE_LIST: &str = "glass.workspace.list";
pub const WORKSPACE_READ: &str = "glass.workspace.read";
pub const WORKSPACE_WRITE: &str = "glass.workspace.write";
pub const WORKSPACE_SEARCH: &str = "glass.workspace.search";

/// Whether one governed tool changes the workspace.
pub fn is_write_tool(name: &str) -> bool {
    name == WORKSPACE_WRITE
}

/// Who is asking for one governed call: the chat and the turn it belongs to.
///
/// Coordination is per conversation, because that is the unit that read a file
/// and later wants to write it. An empty scope is a one-shot call outside a
/// chat, and it takes no part in coordination.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ToolScope {
    pub conversation_id: String,
    pub task_id: String,
}

impl ToolScope {
    pub fn new(conversation_id: impl Into<String>, task_id: impl Into<String>) -> Self {
        Self {
            conversation_id: conversation_id.into(),
            task_id: task_id.into(),
        }
    }

    pub fn is_bound(&self) -> bool {
        !self.conversation_id.is_empty()
    }
}

/// Another chat's work that a write is about to replace.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PathTension {
    /// The chat whose version would be lost, when the host knows it.
    pub conversation_id: String,
    /// True when this chat read an older version of the same file.
    pub stale: bool,
}

/// Another chat's work that an approval card or a refused write must mention.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PathConflict {
    /// Workspace-relative path both chats are working in.
    pub path: String,
    /// What the person should know before approving.
    pub note: String,
    /// True when the write itself is refused so the other change is not lost.
    pub stale: bool,
}

/// A write that could not run because another chat's version is in the way.
#[derive(Debug)]
pub enum PathWriteError {
    Tension(PathTension),
    Io(std::io::Error),
}

/// How long one chat's write keeps a path flagged for the other chats.
const PATH_CONFLICT_WINDOW_MS: u64 = 10 * 60 * 1000;
/// Bounds the remembered paths a long-running host keeps.
const PATH_LEASE_LIMIT: usize = 4_096;

static PATH_LEASES: OnceLock<PathLeases> = OnceLock::new();

fn unix_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_millis() as u64)
        .unwrap_or_default()
}

fn fingerprint(bytes: &[u8]) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    bytes.hash(&mut hasher);
    hasher.finish()
}

fn disk_fingerprint(path: &Path) -> Option<u64> {
    fs::read(path).ok().map(|bytes| fingerprint(&bytes))
}

#[derive(Debug, Clone, Copy)]
struct SeenPath {
    digest: u64,
    at: u64,
}

#[derive(Debug, Clone)]
struct LastWrite {
    conversation_id: String,
    at: u64,
}

#[derive(Debug, Default)]
struct PathRegistry {
    /// What each chat last read at a path, so a stale base is recognisable.
    seen: HashMap<(PathBuf, String), SeenPath>,
    /// The chat that last wrote a path through this host.
    last_write: HashMap<PathBuf, LastWrite>,
    /// One lock per path, so two lanes cannot write one file at the same time.
    gates: HashMap<PathBuf, Arc<Mutex<()>>>,
}

impl PathRegistry {
    /// Keep the table bounded on a host that runs for months.
    fn prune(&mut self) {
        let total = self.seen.len() + self.last_write.len() + self.gates.len();
        if total <= PATH_LEASE_LIMIT {
            return;
        }
        let ceiling = PATH_LEASE_LIMIT / 2;
        let mut seen: Vec<((PathBuf, String), u64)> = self
            .seen
            .iter()
            .map(|(key, seen)| (key.clone(), seen.at))
            .collect();
        seen.sort_by_key(|(_, at)| *at);
        for (key, _) in seen
            .into_iter()
            .take(self.seen.len().saturating_sub(ceiling))
        {
            self.seen.remove(&key);
        }
        let mut writes: Vec<(PathBuf, u64)> = self
            .last_write
            .iter()
            .map(|(path, last)| (path.clone(), last.at))
            .collect();
        writes.sort_by_key(|(_, at)| *at);
        for (path, _) in writes
            .into_iter()
            .take(self.last_write.len().saturating_sub(ceiling))
        {
            self.last_write.remove(&path);
        }
        let mut gates: Vec<PathBuf> = self.gates.keys().cloned().collect();
        gates.sort();
        for path in gates
            .into_iter()
            .take(self.gates.len().saturating_sub(ceiling))
        {
            self.gates.remove(&path);
        }
    }
}

/// One process-wide table of which chat is working in which workspace path.
///
/// Each lane builds its own [`ToolRouter`], so this cannot live on a router:
/// two chats only see each other because every router shares this table.
#[derive(Debug, Clone, Default)]
pub struct PathLeases {
    inner: Arc<Mutex<PathRegistry>>,
}

impl PathLeases {
    /// The table every router in this process shares.
    pub fn shared() -> Self {
        PATH_LEASES.get_or_init(Self::default).clone()
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, PathRegistry> {
        self.inner.lock().unwrap_or_else(|error| error.into_inner())
    }

    fn gate(&self, path: &Path) -> Arc<Mutex<()>> {
        self.lock()
            .gates
            .entry(path.to_path_buf())
            .or_insert_with(|| Arc::new(Mutex::new(())))
            .clone()
    }

    /// Remember what one chat last saw at a path.
    pub fn note_read(&self, path: &Path, conversation_id: &str, bytes: &[u8]) {
        let mut registry = self.lock();
        registry.seen.insert(
            (path.to_path_buf(), conversation_id.to_string()),
            SeenPath {
                digest: fingerprint(bytes),
                at: unix_millis(),
            },
        );
        registry.prune();
    }

    /// Another chat whose work this write would replace, if any.
    pub fn tension(&self, path: &Path, conversation_id: &str) -> Option<PathTension> {
        let registry = self.lock();
        tension(&registry, path, conversation_id, disk_fingerprint(path))
    }

    /// Write one path for one chat, refusing to replace a version that chat has
    /// not seen. The check, the write, and the record all hold the path's lock,
    /// so two lanes cannot interleave inside one file.
    pub fn write(
        &self,
        path: &Path,
        conversation_id: &str,
        content: &[u8],
    ) -> Result<(), PathWriteError> {
        let gate = self.gate(path);
        let _held = gate.lock().unwrap_or_else(|error| error.into_inner());
        {
            let registry = self.lock();
            if let Some(tension) = tension(&registry, path, conversation_id, disk_fingerprint(path))
            {
                return Err(PathWriteError::Tension(tension));
            }
        }
        fs::write(path, content).map_err(PathWriteError::Io)?;
        let mut registry = self.lock();
        let at = unix_millis();
        registry.seen.insert(
            (path.to_path_buf(), conversation_id.to_string()),
            SeenPath {
                digest: fingerprint(content),
                at,
            },
        );
        registry.last_write.insert(
            path.to_path_buf(),
            LastWrite {
                conversation_id: conversation_id.to_string(),
                at,
            },
        );
        registry.prune();
        Ok(())
    }
}

/// Whether writing `path` now would drop work another chat did.
///
/// Two cases matter. A chat that read an older version has a stale base and must
/// re-read. A chat that never read the file may still be about to replace a
/// write another chat just made, so it has to read first as well.
fn tension(
    registry: &PathRegistry,
    path: &Path,
    conversation_id: &str,
    current: Option<u64>,
) -> Option<PathTension> {
    let seen = registry
        .seen
        .get(&(path.to_path_buf(), conversation_id.to_string()))
        .map(|seen| seen.digest);
    if seen == current {
        return None;
    }
    let other = registry
        .last_write
        .get(path)
        .filter(|last| last.conversation_id != conversation_id);
    if seen.is_some() {
        return Some(PathTension {
            conversation_id: other
                .map(|last| last.conversation_id.clone())
                .unwrap_or_default(),
            stale: true,
        });
    }
    let last = other?;
    if unix_millis().saturating_sub(last.at) > PATH_CONFLICT_WINDOW_MS {
        return None;
    }
    Some(PathTension {
        conversation_id: last.conversation_id.clone(),
        stale: false,
    })
}

/// The governed catalog as a surface document.
///
/// Core owns its native coding tools; these are the integrations Marble executes
/// itself, so they are the ones a person approves. Each entry says what it
/// touches, because "list" and "write" deserve different scrutiny.
pub fn document() -> Value {
    Value::Array(
        ToolRouter::catalog()
            .into_iter()
            .map(|tool| {
                json!({
                    "name": tool.name,
                    "description": tool.description,
                    "owner": "marble",
                    "effect": if is_write_tool(&tool.name) { "write" } else { "read" },
                    "schema": tool.input_schema,
                })
            })
            .collect(),
    )
}

/// Largest file the read tool returns in one call.
pub const MAX_READ_BYTES: usize = 256 * 1024;
/// Largest payload the write tool accepts in one call.
pub const MAX_WRITE_BYTES: usize = 256 * 1024;
/// Bounded directory listing.
pub const MAX_LIST_ENTRIES: usize = 500;
/// Bounded search: matches returned, files opened, and bytes read.
pub const MAX_SEARCH_MATCHES: usize = 100;
const MAX_SEARCH_FILES: usize = 2_000;
const MAX_SEARCH_BYTES: usize = 4 * 1024 * 1024;
const MAX_SEARCH_LINE: usize = 200;

/// Directories and files a search never reads: build output, dependencies, and
/// revision or secret metadata that the control layer also refuses.
fn search_skips(name: &str) -> bool {
    matches!(
        name,
        ".git"
            | ".ssh"
            | "node_modules"
            | "target"
            | ".next"
            | "dist"
            | "build"
            | ".venv"
            | "venv"
            | "__pycache__"
    ) || name == ".env"
        || name.starts_with(".env.")
}

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
    /// Paths other chats are working in, shared by every lane in this process.
    leases: PathLeases,
}

impl ToolRouter {
    /// Canonicalize the workspace root once so later path checks compare like
    /// with like.
    pub fn new(root: impl AsRef<Path>) -> Result<Self, MarbleError> {
        let root = root.as_ref().canonicalize().map_err(MarbleError::Io)?;
        Ok(Self {
            root,
            leases: PathLeases::shared(),
        })
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
                name: WORKSPACE_SEARCH.into(),
                description: "Search workspace files for a case-insensitive text match and \
                     return file, line number, and the matching line. Skips `.git`, `.ssh`, \
                     `.env*`, dependencies, and build output."
                    .into(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "query": {"type": "string", "description": "Text to find, case-insensitive."},
                        "path": {"type": "string", "description": "File or directory relative to the workspace root; defaults to the root."},
                        "maxResults": {"type": "integer", "description": "Matches to return, 1 to 100."}
                    },
                    "required": ["query"],
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
            ToolDefinition {
                name: CRATES_TOOL.into(),
                description: "List the Rust crates in this workspace with their packages, the \
                     binaries they ship, the siblings they depend on, and how this host can reach \
                     them. Pass `name` for one crate and what depends on it."
                    .into(),
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "name": {"type": "string", "description": "Crate or binary name; omit for the whole workspace."}
                    },
                    "additionalProperties": false
                }),
            },
        ]
    }

    /// Execute one tool call outside a chat. Unknown tools fail instead of
    /// silently succeeding.
    pub fn execute(&self, name: &str, arguments: &Value) -> ToolOutcome {
        self.execute_scoped(&ToolScope::default(), name, arguments)
    }

    /// Execute one tool call for a chat, so a write knows whose version of a
    /// path it is based on.
    pub fn execute_scoped(&self, scope: &ToolScope, name: &str, arguments: &Value) -> ToolOutcome {
        match name {
            WORKSPACE_LIST => self.list(arguments),
            WORKSPACE_READ => self.read(scope, arguments),
            WORKSPACE_SEARCH => self.search(arguments),
            WORKSPACE_WRITE => self.write(scope, arguments),
            CRATES_TOOL => self.crates(arguments),
            other => ToolOutcome::error(format!("host has no tool named {other}")),
        }
    }

    /// Another chat's work in the path this call names, for an approval card.
    ///
    /// Reads name what this chat has seen; only a write can disturb another
    /// chat, so every other tool reports no conflict.
    pub fn conflict(
        &self,
        scope: &ToolScope,
        name: &str,
        arguments: &Value,
    ) -> Option<PathConflict> {
        if !scope.is_bound() || !is_write_tool(name) {
            return None;
        }
        let path = self.resolve(arguments).ok()?;
        let tension = self.leases.tension(&path, &scope.conversation_id)?;
        let display = self.display(&path);
        let note = if tension.stale {
            format!(
                "Another chat or an editor changed {display} after this conversation read it. \
                 Approving will not overwrite it: Marble is asked to re-read and redo the change."
            )
        } else {
            format!(
                "Another chat wrote {display} recently. Marble must re-read it before replacing it."
            )
        };
        Some(PathConflict {
            path: display,
            note,
            stale: tension.stale,
        })
    }

    /// What the model is told when its write would drop another chat's change.
    fn tension_note(&self, path: &Path, tension: &PathTension) -> String {
        let display = self.display(path);
        if tension.stale {
            format!(
                "{display} changed since this conversation last read it (another chat or an \
                 editor); re-read the file and redo the change"
            )
        } else {
            format!("another chat wrote {display} recently; re-read the file before replacing it")
        }
    }

    /// Report the workspace's crates and how this host can reach each one.
    ///
    /// The manifests are read on every call so a crate added in the middle of a
    /// conversation shows up without a restart; the map is small and read-only.
    fn crates(&self, arguments: &Value) -> ToolOutcome {
        let map = CrateMap::load(&self.root);
        if map.is_empty() {
            return ToolOutcome::ok(format!(
                "{} has no Cargo.toml workspace, so there are no crates to report",
                self.display(&self.root)
            ));
        }
        match arguments
            .get("name")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|name| !name.is_empty())
        {
            None => ToolOutcome::ok(map.document().to_string()),
            Some(name) => match map.find(name) {
                Some(entry) => ToolOutcome::ok(map.crate_document(entry).to_string()),
                None => {
                    ToolOutcome::error(format!("no crate or binary named {name} in this workspace"))
                }
            },
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

    fn read(&self, scope: &ToolScope, arguments: &Value) -> ToolOutcome {
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
        if scope.is_bound() {
            self.leases.note_read(&path, &scope.conversation_id, &bytes);
        }
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

    fn search(&self, arguments: &Value) -> ToolOutcome {
        let query = arguments
            .get("query")
            .and_then(Value::as_str)
            .map(str::trim)
            .unwrap_or_default();
        if query.is_empty() {
            return ToolOutcome::error("query must be a nonempty string");
        }
        if query.chars().count() > 200 {
            return ToolOutcome::error("query is longer than 200 characters");
        }
        let root = match self.resolve(arguments) {
            Ok(path) => path,
            Err(outcome) => return outcome,
        };
        let limit = arguments
            .get("maxResults")
            .and_then(Value::as_u64)
            .unwrap_or(40)
            .clamp(1, MAX_SEARCH_MATCHES as u64) as usize;
        let needle = query.to_ascii_lowercase();
        let mut matches = Vec::new();
        let mut files = 0usize;
        let mut bytes_read = 0usize;
        let mut queue = std::collections::VecDeque::from([root.clone()]);
        'walk: while let Some(current) = queue.pop_front() {
            if matches.len() >= limit || files >= MAX_SEARCH_FILES || bytes_read >= MAX_SEARCH_BYTES
            {
                break;
            }
            let Ok(metadata) = fs::metadata(&current) else {
                continue;
            };
            if metadata.is_dir() {
                let Ok(entries) = fs::read_dir(&current) else {
                    continue;
                };
                let mut children: Vec<PathBuf> = entries
                    .filter_map(Result::ok)
                    .map(|entry| entry.path())
                    .filter(|path| {
                        path.file_name()
                            .map(|name| !search_skips(&name.to_string_lossy()))
                            .unwrap_or(false)
                    })
                    .collect();
                children.sort();
                queue.extend(children);
                continue;
            }
            files += 1;
            if metadata.len() > 512 * 1024 {
                continue;
            }
            let Ok(bytes) = fs::read(&current) else {
                continue;
            };
            if bytes.iter().take(1024).any(|byte| *byte == 0) {
                continue;
            }
            bytes_read += bytes.len();
            let text = String::from_utf8_lossy(&bytes);
            for (index, line) in text.lines().enumerate() {
                if !line.to_ascii_lowercase().contains(&needle) {
                    continue;
                }
                let trimmed = line.trim();
                let trimmed = if trimmed.chars().count() > MAX_SEARCH_LINE {
                    let short: String = trimmed.chars().take(MAX_SEARCH_LINE).collect();
                    format!("{short}…")
                } else {
                    trimmed.to_string()
                };
                matches.push(format!(
                    "{}:{}: {trimmed}",
                    self.display(&current),
                    index + 1
                ));
                if matches.len() >= limit {
                    break 'walk;
                }
            }
        }
        if matches.is_empty() {
            return ToolOutcome::ok(format!(
                "no matches for {query} under {}",
                self.display(&root)
            ));
        }
        let mut text = format!(
            "{} match{} for {query} under {}:\n{}",
            matches.len(),
            if matches.len() == 1 { "" } else { "es" },
            self.display(&root),
            matches.join("\n")
        );
        if matches.len() >= limit {
            text.push_str(&format!("\n(stopped at {limit} matches)"));
        }
        ToolOutcome::ok(text)
    }

    fn write(&self, scope: &ToolScope, arguments: &Value) -> ToolOutcome {
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
        let written = if scope.is_bound() {
            self.leases
                .write(&path, &scope.conversation_id, content.as_bytes())
        } else {
            fs::write(&path, content).map_err(PathWriteError::Io)
        };
        match written {
            Ok(()) => ToolOutcome::ok(format!(
                "wrote {} bytes to {}",
                content.len(),
                self.display(&path)
            )),
            Err(PathWriteError::Tension(tension)) => {
                ToolOutcome::error(self.tension_note(&path, &tension))
            }
            Err(PathWriteError::Io(error)) => {
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
    fn search_finds_matches_with_line_numbers_and_skips_secrets() {
        let root = workspace("search");
        fs::create_dir_all(root.join("docs")).unwrap();
        fs::write(
            root.join("docs/notes.md"),
            "first\nGlass Marble lives here\n",
        )
        .unwrap();
        fs::write(root.join("other.txt"), "nothing to see\n").unwrap();
        fs::write(root.join(".env"), "MARBLE=glass\n").unwrap();
        fs::create_dir_all(root.join("node_modules/pkg")).unwrap();
        fs::write(root.join("node_modules/pkg/index.js"), "glass marble\n").unwrap();
        let router = ToolRouter::new(&root).unwrap();

        let outcome = router.execute(WORKSPACE_SEARCH, &json!({"query": "glass marble"}));
        assert!(!outcome.is_error, "{}", outcome.text);
        assert!(
            outcome.text.contains("docs/notes.md:2:"),
            "{}",
            outcome.text
        );
        assert!(!outcome.text.contains("node_modules"), "{}", outcome.text);
        assert!(!outcome.text.contains("MARBLE=glass"), "{}", outcome.text);

        let empty = router.execute(WORKSPACE_SEARCH, &json!({"query": "missing"}));
        assert!(empty.text.starts_with("no matches"));
        assert!(
            router
                .execute(WORKSPACE_SEARCH, &json!({"query": ""}))
                .is_error
        );
        assert!(
            router
                .execute(WORKSPACE_SEARCH, &json!({"query": "x", "path": "../"}))
                .is_error
        );
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn a_chat_cannot_replace_a_version_it_has_not_seen() {
        let root = workspace("lost-write");
        fs::write(root.join("notes.md"), "first").unwrap();
        let lane_a = ToolRouter::new(&root).unwrap();
        let lane_b = ToolRouter::new(&root).unwrap();
        let chat_a = ToolScope::new("chat-a", "task-a");
        let chat_b = ToolScope::new("chat-b", "task-b");

        assert!(
            !lane_a
                .execute_scoped(&chat_a, WORKSPACE_READ, &json!({"path": "notes.md"}))
                .is_error
        );
        assert!(
            !lane_b
                .execute_scoped(
                    &chat_b,
                    WORKSPACE_WRITE,
                    &json!({"path": "notes.md", "content": "second"})
                )
                .is_error
        );

        // The first chat read the old version; writing now would drop the
        // second chat's change, so the router refuses instead of losing it.
        let refused = lane_a.execute_scoped(
            &chat_a,
            WORKSPACE_WRITE,
            &json!({"path": "notes.md", "content": "third"}),
        );
        assert!(refused.is_error, "{}", refused.text);
        assert!(refused.text.contains("re-read"), "{}", refused.text);
        assert_eq!(fs::read_to_string(root.join("notes.md")).unwrap(), "second");

        // Re-reading makes the same write safe.
        assert!(
            !lane_a
                .execute_scoped(&chat_a, WORKSPACE_READ, &json!({"path": "notes.md"}))
                .is_error
        );
        assert!(
            !lane_a
                .execute_scoped(
                    &chat_a,
                    WORKSPACE_WRITE,
                    &json!({"path": "notes.md", "content": "third"})
                )
                .is_error
        );
        assert_eq!(fs::read_to_string(root.join("notes.md")).unwrap(), "third");

        // A chat that never read the file cannot replace the latest write
        // either: it has to read first.
        let blind = lane_b.execute_scoped(
            &chat_b,
            WORKSPACE_WRITE,
            &json!({"path": "notes.md", "content": "blind"}),
        );
        assert!(blind.is_error, "{blind:?}");
        assert_eq!(fs::read_to_string(root.join("notes.md")).unwrap(), "third");

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn the_approval_reports_the_other_chat_before_the_write_runs() {
        let root = workspace("conflict-note");
        let lane_a = ToolRouter::new(&root).unwrap();
        let lane_b = ToolRouter::new(&root).unwrap();
        let chat_a = ToolScope::new("chat-a", "task-a");
        let chat_b = ToolScope::new("chat-b", "task-b");
        let write = json!({"path": "docs/notes.md", "content": "from a"});

        assert!(
            !lane_a
                .execute_scoped(&chat_a, WORKSPACE_WRITE, &write)
                .is_error
        );
        let conflict = lane_b
            .conflict(&chat_b, WORKSPACE_WRITE, &write)
            .expect("the second chat sees the first chat's write");
        assert_eq!(conflict.path, "docs/notes.md");
        assert!(
            conflict.note.contains("Another chat wrote") && conflict.note.contains("re-read"),
            "{}",
            conflict.note
        );
        assert!(!conflict.stale);

        // Reads never show a conflict, and neither does a one-shot call.
        assert!(
            lane_b
                .conflict(&chat_b, WORKSPACE_READ, &json!({"path": "docs/notes.md"}))
                .is_none()
        );
        assert!(
            lane_b
                .conflict(&ToolScope::default(), WORKSPACE_WRITE, &write)
                .is_none()
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
    fn the_crate_tool_reads_the_workspace_and_refuses_unknown_names() {
        let root = workspace("crates");
        fs::write(
            root.join("Cargo.toml"),
            "[workspace]\nmembers = [\"crates/one\"]\n",
        )
        .unwrap();
        fs::create_dir_all(root.join("crates/one")).unwrap();
        fs::write(
            root.join("crates/one/Cargo.toml"),
            "[package]\nname = \"one\"\nversion = \"0.1.0\"\n",
        )
        .unwrap();
        let router = ToolRouter::new(&root).unwrap();

        let outcome = router.execute(CRATES_TOOL, &json!({}));
        assert!(!outcome.is_error, "{}", outcome.text);
        assert!(outcome.text.contains("\"one\""), "{}", outcome.text);

        let one = router.execute(CRATES_TOOL, &json!({"name": "one"}));
        assert!(!one.is_error, "{}", one.text);
        assert!(one.text.contains("dependedOnBy"), "{}", one.text);

        let missing = router.execute(CRATES_TOOL, &json!({"name": "nope"}));
        assert!(missing.is_error, "{}", missing.text);

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
