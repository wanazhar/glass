use super::{
    Actor, DevelopmentError, DevelopmentEventKind, DevelopmentGraph, DevelopmentResult,
    LanguageDiagnostic, LspClient, MAX_BUFFER_BYTES, MAX_FILE_BYTES, MAX_FILE_ENTRIES,
    ProcessManager, ReplayWindow, SearchHit, SearchKind, SemanticBreakpoint, SemanticBreakpointHit,
    SemanticSnapshot, SourceLocation, Timeline,
};
use crate::development::diff::{ProjectDiff, build_diff};
use crate::development::graph::{LinkEvidence, LinkProvenance, RuntimeLink};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs::{self, OpenOptions},
    io::Write,
    path::{Component, Path, PathBuf},
    process::Command,
    time::{Duration, Instant},
};

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct GlassProjectConfig {
    #[serde(default)]
    pub project: ProjectConfig,
    #[serde(default)]
    pub commands: CommandConfig,
    #[serde(default)]
    pub browser: BrowserConfig,
    #[serde(default)]
    pub editor: EditorConfig,
    #[serde(default)]
    pub agent: AgentConfig,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProjectConfig {
    #[serde(default)]
    pub name: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct CommandConfig {
    #[serde(default)]
    pub dev: Option<String>,
    #[serde(default)]
    pub test: Option<String>,
    #[serde(default)]
    pub lint: Option<String>,
    #[serde(default)]
    pub build: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct BrowserConfig {
    #[serde(default)]
    pub url: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct EditorConfig {
    #[serde(default)]
    pub engine: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct AgentConfig {
    #[serde(default)]
    pub harness: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ProjectDetection {
    pub root: PathBuf,
    pub languages: Vec<String>,
    pub package_manager: Option<String>,
    pub framework: Option<String>,
    pub build_system: Option<String>,
    pub formatter: Option<String>,
    pub lsp_servers: Vec<String>,
    pub git_branch: Option<String>,
    pub dev_command: Option<String>,
    pub test_command: Option<String>,
    pub lint_command: Option<String>,
    pub build_command: Option<String>,
    pub browser_url: Option<String>,
    pub local_development_urls: Vec<String>,
    pub editor_engine: Option<String>,
    pub agent_harness: Option<String>,
    pub config_path: Option<PathBuf>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum FileKind {
    File,
    Directory,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct FileEntry {
    pub path: String,
    pub kind: FileKind,
    pub bytes: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub git_status: Option<String>,
    #[serde(default)]
    pub dirty: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub actor: Option<Actor>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct EditorBuffer {
    pub path: String,
    pub content: String,
    pub original_hash: String,
    pub dirty: bool,
    pub cursor_line: u32,
    pub cursor_column: u32,
    pub actor: Actor,
}

pub struct ProjectWorkspace {
    root: PathBuf,
    detection: ProjectDetection,
    config: GlassProjectConfig,
    buffers: BTreeMap<String, EditorBuffer>,
    undo: BTreeMap<String, Vec<String>>,
    redo: BTreeMap<String, Vec<String>>,
    processes: ProcessManager,
    timeline: Timeline,
    graph: DevelopmentGraph,
    graph_path: PathBuf,
    collaboration: super::CollaborationBus,
    diagnostics: BTreeMap<String, Vec<LanguageDiagnostic>>,
    actors: BTreeMap<String, Actor>,
    actor: Actor,
    revision: u64,
}

impl std::fmt::Debug for ProjectWorkspace {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ProjectWorkspace")
            .field("root", &self.root)
            .field("detection", &self.detection)
            .field("buffers", &self.buffers.keys().collect::<Vec<_>>())
            .field("revision", &self.revision)
            .finish()
    }
}

impl ProjectWorkspace {
    pub fn open(root: impl AsRef<Path>) -> DevelopmentResult<Self> {
        let root = canonical_root(root.as_ref())?;
        let detection = detect_project(&root)?;
        let config = load_project_config(detection.config_path.as_deref())?;
        let timeline = Timeline::for_project(&root)?;
        let graph_path = timeline.path().with_file_name("graph.json");
        let graph = DevelopmentGraph::load(&graph_path)?;
        let actor = Actor::local();
        let mut actors = BTreeMap::new();
        actors.insert(actor.id.clone(), actor.clone());
        let embedded = Actor::embedded();
        actors.insert(embedded.id.clone(), embedded);
        for event in timeline.events() {
            if matches!(event.kind, DevelopmentEventKind::ActorJoined) {
                actors.insert(event.actor.id.clone(), event.actor.clone());
            }
        }
        let mut workspace = Self {
            root: root.clone(),
            detection,
            config,
            buffers: BTreeMap::new(),
            undo: BTreeMap::new(),
            redo: BTreeMap::new(),
            processes: ProcessManager::new(&root),
            timeline,
            graph,
            graph_path,
            collaboration: super::CollaborationBus::default(),
            diagnostics: BTreeMap::new(),
            actors,
            actor,
            revision: 0,
        };
        let _ = workspace.record(
            DevelopmentEventKind::WorkspaceOpened,
            serde_json::json!({"root": root}),
        );
        Ok(workspace)
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn detection(&self) -> &ProjectDetection {
        &self.detection
    }

    pub fn config(&self) -> &GlassProjectConfig {
        &self.config
    }

    pub fn revision(&self) -> u64 {
        self.revision
    }

    pub fn timeline(&self) -> &Timeline {
        &self.timeline
    }

    pub fn graph(&self) -> &DevelopmentGraph {
        &self.graph
    }

    pub fn diagnostics(&self) -> &BTreeMap<String, Vec<LanguageDiagnostic>> {
        &self.diagnostics
    }

    pub fn actors(&self) -> impl Iterator<Item = &Actor> {
        self.actors.values()
    }

    pub fn collaboration(&mut self) -> &mut super::CollaborationBus {
        &mut self.collaboration
    }

    pub fn attach_actor(&mut self, actor: Actor) -> DevelopmentResult<()> {
        if self.actors.len() >= 64 && !self.actors.contains_key(&actor.id) {
            return Err(DevelopmentError::InvalidInput(
                "workspace cannot contain more than 64 actors".into(),
            ));
        }
        self.record_as(
            actor.clone(),
            DevelopmentEventKind::ActorJoined,
            serde_json::json!({"actorId": actor.id.clone()}),
        )?;
        self.actors.insert(actor.id.clone(), actor);
        Ok(())
    }

    pub fn processes(&mut self) -> &mut ProcessManager {
        &mut self.processes
    }

    pub fn list_files(&self) -> DevelopmentResult<Vec<FileEntry>> {
        let mut entries = Vec::new();
        visit_files(&self.root, &self.root, &mut entries)?;
        let git = git_statuses(&self.root);
        for entry in &mut entries {
            entry.git_status = git.get(&entry.path).cloned();
            if let Some(buffer) = self.buffers.get(&entry.path) {
                entry.dirty = buffer.dirty;
                entry.actor = Some(buffer.actor.clone());
            }
        }
        entries.sort_by(|left, right| left.path.cmp(&right.path));
        Ok(entries)
    }

    pub fn read_file(&mut self, path: &str) -> DevelopmentResult<String> {
        let (absolute, relative) = self.resolve_path(path, false)?;
        let metadata = fs::metadata(&absolute)?;
        if !metadata.is_file() {
            return Err(DevelopmentError::InvalidInput(format!(
                "not a regular file: {path}"
            )));
        }
        if metadata.len() > MAX_FILE_BYTES as u64 {
            return Err(DevelopmentError::InvalidInput(format!(
                "file exceeds the {} byte read limit: {path}",
                MAX_FILE_BYTES
            )));
        }
        let content = fs::read_to_string(&absolute)?;
        self.record(
            DevelopmentEventKind::FileOpened,
            serde_json::json!({"path": relative}),
        )?;
        Ok(content)
    }

    pub fn open_buffer(&mut self, path: &str, actor: Actor) -> DevelopmentResult<EditorBuffer> {
        let content = self.read_file(path)?;
        let (_, relative) = self.resolve_path(path, false)?;
        let buffer = EditorBuffer {
            path: relative.clone(),
            original_hash: hash(&content),
            content,
            dirty: false,
            cursor_line: 1,
            cursor_column: 1,
            actor,
        };
        self.buffers.insert(relative, buffer.clone());
        Ok(buffer)
    }

    pub fn buffers(&self) -> impl Iterator<Item = &EditorBuffer> {
        self.buffers.values()
    }

    pub fn buffer(&self, path: &str) -> Option<&EditorBuffer> {
        self.buffers.get(path)
    }

    pub fn edit_buffer(
        &mut self,
        path: &str,
        content: String,
        actor: Actor,
    ) -> DevelopmentResult<()> {
        if content.len() > MAX_BUFFER_BYTES {
            return Err(DevelopmentError::InvalidInput(format!(
                "buffer exceeds the {} byte limit",
                MAX_BUFFER_BYTES
            )));
        }
        if !self.buffers.contains_key(path) {
            if self.resolve_path(path, false).is_ok() {
                self.open_buffer(path, actor.clone())?;
            } else {
                let (_, relative) = self.resolve_path(path, true)?;
                self.buffers.insert(
                    relative.clone(),
                    EditorBuffer {
                        path: relative,
                        content: String::new(),
                        original_hash: hash(""),
                        dirty: false,
                        cursor_line: 1,
                        cursor_column: 1,
                        actor: actor.clone(),
                    },
                );
            }
        }
        let buffer = self
            .buffers
            .get_mut(path)
            .ok_or_else(|| DevelopmentError::NotFound(format!("buffer {path}")))?;
        let history = self.undo.entry(path.to_string()).or_default();
        if history.len() == 256 {
            history.remove(0);
        }
        history.push(buffer.content.clone());
        self.redo.remove(path);
        buffer.content = content;
        buffer.dirty = hash(&buffer.content) != buffer.original_hash;
        buffer.actor = actor;
        self.revision = self.revision.saturating_add(1);
        Ok(())
    }

    pub fn save_buffer(&mut self, path: &str) -> DevelopmentResult<EditorBuffer> {
        let buffer = self
            .buffers
            .get(path)
            .cloned()
            .ok_or_else(|| DevelopmentError::NotFound(format!("buffer {path}")))?;
        let disk_hash = match read_existing(&self.root, path) {
            Ok(content) => hash(&content),
            Err(DevelopmentError::Io(error)) if error.kind() == std::io::ErrorKind::NotFound => {
                hash("")
            }
            Err(error) => return Err(error),
        };
        if disk_hash != buffer.original_hash {
            return Err(DevelopmentError::Conflict(format!(
                "{path} changed on disk after the buffer was opened"
            )));
        }
        self.write_file(path, &buffer.content, buffer.actor.clone())?;
        let mut saved = buffer;
        saved.original_hash = hash(&saved.content);
        saved.dirty = false;
        self.buffers.insert(path.into(), saved.clone());
        Ok(saved)
    }

    pub fn undo_buffer(&mut self, path: &str) -> DevelopmentResult<EditorBuffer> {
        let previous = self
            .undo
            .get_mut(path)
            .and_then(Vec::pop)
            .ok_or_else(|| DevelopmentError::NotFound(format!("undo history for {path}")))?;
        let buffer = self
            .buffers
            .get_mut(path)
            .ok_or_else(|| DevelopmentError::NotFound(format!("buffer {path}")))?;
        self.redo
            .entry(path.to_string())
            .or_default()
            .push(std::mem::replace(&mut buffer.content, previous));
        buffer.dirty = hash(&buffer.content) != buffer.original_hash;
        self.revision = self.revision.saturating_add(1);
        Ok(buffer.clone())
    }

    pub fn redo_buffer(&mut self, path: &str) -> DevelopmentResult<EditorBuffer> {
        let next = self
            .redo
            .get_mut(path)
            .and_then(Vec::pop)
            .ok_or_else(|| DevelopmentError::NotFound(format!("redo history for {path}")))?;
        let buffer = self
            .buffers
            .get_mut(path)
            .ok_or_else(|| DevelopmentError::NotFound(format!("buffer {path}")))?;
        self.undo
            .entry(path.to_string())
            .or_default()
            .push(std::mem::replace(&mut buffer.content, next));
        buffer.dirty = hash(&buffer.content) != buffer.original_hash;
        self.revision = self.revision.saturating_add(1);
        Ok(buffer.clone())
    }

    pub fn replace_in_buffer(
        &mut self,
        path: &str,
        needle: &str,
        replacement: &str,
        actor: Actor,
    ) -> DevelopmentResult<usize> {
        if needle.is_empty() {
            return Err(DevelopmentError::InvalidInput(
                "search text must not be empty".into(),
            ));
        }
        if !self.buffers.contains_key(path) {
            self.open_buffer(path, actor.clone())?;
        }
        let current = self
            .buffers
            .get(path)
            .ok_or_else(|| DevelopmentError::NotFound(format!("buffer {path}")))?
            .content
            .clone();
        let matches = current.matches(needle).count();
        if matches > 0 {
            self.edit_buffer(path, current.replace(needle, replacement), actor)?;
        }
        Ok(matches)
    }

    pub fn set_buffer_cursor(
        &mut self,
        path: &str,
        line: u32,
        column: u32,
    ) -> DevelopmentResult<()> {
        if line == 0 || column == 0 {
            return Err(DevelopmentError::InvalidInput(
                "editor cursor positions are one-based".into(),
            ));
        }
        let buffer = self
            .buffers
            .get_mut(path)
            .ok_or_else(|| DevelopmentError::NotFound(format!("buffer {path}")))?;
        buffer.cursor_line = line;
        buffer.cursor_column = column;
        Ok(())
    }

    pub fn rename_path(&mut self, from: &str, to: &str, actor: Actor) -> DevelopmentResult<()> {
        let (source, source_relative) = self.resolve_path(from, false)?;
        let (destination, destination_relative) = self.resolve_path(to, true)?;
        if destination.exists() {
            return Err(DevelopmentError::Conflict(format!(
                "destination already exists: {to}"
            )));
        }
        if let Some(parent) = destination.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::rename(source, destination)?;
        if let Some(mut buffer) = self.buffers.remove(&source_relative) {
            buffer.path = destination_relative.clone();
            self.buffers.insert(destination_relative.clone(), buffer);
        }
        if let Some(history) = self.undo.remove(&source_relative) {
            self.undo.insert(destination_relative.clone(), history);
        }
        if let Some(history) = self.redo.remove(&source_relative) {
            self.redo.insert(destination_relative.clone(), history);
        }
        self.revision = self.revision.saturating_add(1);
        self.record_as(
            actor,
            DevelopmentEventKind::FileSaved,
            serde_json::json!({"operation": "rename", "from": source_relative, "to": destination_relative}),
        )
    }

    pub fn create_directory(&mut self, path: &str, actor: Actor) -> DevelopmentResult<()> {
        let (absolute, relative) = self.resolve_path(path, true)?;
        if absolute.exists() {
            return Err(DevelopmentError::Conflict(format!(
                "path already exists: {path}"
            )));
        }
        fs::create_dir_all(&absolute)?;
        self.revision = self.revision.saturating_add(1);
        self.record_as(
            actor,
            DevelopmentEventKind::FileSaved,
            serde_json::json!({"operation": "mkdir", "path": relative}),
        )
    }

    pub fn delete_path(&mut self, path: &str, actor: Actor) -> DevelopmentResult<()> {
        let (absolute, relative) = self.resolve_path(path, false)?;
        let metadata = fs::symlink_metadata(&absolute)?;
        if metadata.is_dir() {
            if fs::read_dir(&absolute)?.next().is_some() {
                return Err(DevelopmentError::Conflict(format!(
                    "refusing to delete non-empty directory: {path}"
                )));
            }
            fs::remove_dir(&absolute)?;
        } else {
            fs::remove_file(&absolute)?;
        }
        self.buffers.remove(&relative);
        self.undo.remove(&relative);
        self.redo.remove(&relative);
        self.revision = self.revision.saturating_add(1);
        self.record_as(
            actor,
            DevelopmentEventKind::FileSaved,
            serde_json::json!({"operation": "delete", "path": relative}),
        )
    }

    pub fn write_file(&mut self, path: &str, content: &str, actor: Actor) -> DevelopmentResult<()> {
        if content.len() > MAX_FILE_BYTES {
            return Err(DevelopmentError::InvalidInput(format!(
                "file exceeds the {} byte write limit",
                MAX_FILE_BYTES
            )));
        }
        let (absolute, relative) = self.resolve_path(path, true)?;
        let before_hash = fs::read_to_string(&absolute)
            .ok()
            .map(|existing| hash(&existing));
        let parent = absolute
            .parent()
            .ok_or_else(|| DevelopmentError::InvalidInput("file has no parent".into()))?;
        fs::create_dir_all(parent)?;
        let temporary = absolute.with_extension(format!(
            "glass-tmp-{}-{}",
            std::process::id(),
            self.revision.saturating_add(1)
        ));
        {
            let mut file = OpenOptions::new()
                .create_new(true)
                .write(true)
                .open(&temporary)?;
            file.write_all(content.as_bytes())?;
            file.sync_all()?;
        }
        if let Err(error) = fs::rename(&temporary, &absolute) {
            let _ = fs::remove_file(&temporary);
            return Err(error.into());
        }
        self.revision = self.revision.saturating_add(1);
        self.record_as(
            actor,
            DevelopmentEventKind::FileSaved,
            serde_json::json!({
                "path": relative,
                "bytes": content.len(),
                "beforeHash": before_hash,
                "afterHash": hash(content)
            }),
        )?;
        if self.processes.list().iter().any(|process| {
            matches!(process.state, super::ProcessState::Running)
                && (process.name.contains("dev") || process.name.contains("server"))
        }) {
            self.record(
                DevelopmentEventKind::HmrObserved,
                serde_json::json!({
                    "path": relative,
                    "status": "pending-runtime-observation",
                    "browserUrl": self.detection.browser_url
                }),
            )?;
        }
        Ok(())
    }

    pub fn start_process(
        &mut self,
        name: &str,
        command: &str,
    ) -> DevelopmentResult<super::ProcessSnapshot> {
        let snapshot = self.processes.start(name, command)?;
        self.revision = self.revision.saturating_add(1);
        self.record(
            DevelopmentEventKind::ProcessStarted,
            serde_json::json!({"name": name, "command": command, "pid": snapshot.pid}),
        )?;
        Ok(snapshot)
    }

    pub fn stop_process(&mut self, name: &str) -> DevelopmentResult<super::ProcessSnapshot> {
        let snapshot = self.processes.stop(name)?;
        self.revision = self.revision.saturating_add(1);
        self.record(
            DevelopmentEventKind::ProcessExited,
            serde_json::json!({"name": name, "state": snapshot.state}),
        )?;
        Ok(snapshot)
    }

    pub fn run_verification(
        &mut self,
        name: &str,
        command: &str,
        timeout: Duration,
    ) -> DevelopmentResult<super::ProcessSnapshot> {
        self.record(
            DevelopmentEventKind::TestStarted,
            serde_json::json!({"name": name, "command": command}),
        )?;
        self.start_process(name, command)?;
        let deadline = Instant::now() + timeout;
        let snapshot = loop {
            let snapshot = self
                .processes
                .poll()?
                .into_iter()
                .find(|snapshot| snapshot.name == name)
                .ok_or_else(|| DevelopmentError::NotFound(format!("process {name}")))?;
            if !matches!(snapshot.state, super::ProcessState::Running) {
                break snapshot;
            }
            if Instant::now() >= deadline {
                self.stop_process(name)?;
                self.record(
                    DevelopmentEventKind::TestCompleted,
                    serde_json::json!({"name": name, "status": "timeout"}),
                )?;
                return Err(DevelopmentError::Process(format!(
                    "verification {name} exceeded {} seconds",
                    timeout.as_secs()
                )));
            }
            std::thread::sleep(Duration::from_millis(25));
        };
        self.record(
            DevelopmentEventKind::TestCompleted,
            serde_json::json!({
                "name": name,
                "state": snapshot.state,
                "outputBytes": snapshot.output.len(),
                "outputSha256": hash(&snapshot.output)
            }),
        )?;
        Ok(snapshot)
    }

    /// Record a live-update proof only after a browser consumer has observed a
    /// strictly newer semantic revision. This keeps HMR/build claims tied to
    /// caller-supplied browser evidence instead of treating a file save as
    /// proof that the running application changed.
    pub fn confirm_live_update(
        &mut self,
        source_path: &str,
        before_revision: u64,
        after_revision: u64,
        semantic_changes: usize,
        actor: Actor,
    ) -> DevelopmentResult<()> {
        self.resolve_path(source_path, false)?;
        if after_revision <= before_revision {
            return Err(DevelopmentError::InvalidInput(
                "live update proof requires a newer browser revision".into(),
            ));
        }
        self.record_as(
            actor,
            DevelopmentEventKind::HmrObserved,
            serde_json::json!({
                "sourcePath": source_path,
                "status": "confirmed-runtime-observation",
                "beforeBrowserRevision": before_revision,
                "afterBrowserRevision": after_revision,
                "semanticChanges": semantic_changes,
            }),
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn link_runtime_source(
        &mut self,
        entity_id: &str,
        path: &str,
        start_line: u32,
        end_line: u32,
        provenance: LinkProvenance,
        detail: &str,
        confidence: f32,
        actor: Actor,
    ) -> DevelopmentResult<RuntimeLink> {
        let (_, relative) = self.resolve_path(path, false)?;
        let link = RuntimeLink {
            entity_id: entity_id.into(),
            source: SourceLocation::new(relative, start_line, end_line)?,
            evidence: LinkEvidence {
                provenance,
                detail: detail.into(),
                confidence,
            },
        };
        self.graph.link(link.clone())?;
        self.graph.save(&self.graph_path)?;
        self.revision = self.revision.saturating_add(1);
        self.record_as(
            actor,
            DevelopmentEventKind::SourceRuntimeLinked,
            serde_json::to_value(&link)?,
        )?;
        Ok(link)
    }

    pub fn diff(&mut self) -> DevelopmentResult<ProjectDiff> {
        build_diff(&self.root, &self.timeline, &self.graph, &mut self.processes)
    }

    pub fn discover_runtime_links(&mut self) -> DevelopmentResult<Vec<RuntimeLink>> {
        let discovered = self.graph.discover_explicit_markers(&self.root)?;
        self.graph.save(&self.graph_path)?;
        for link in &discovered {
            self.record(
                DevelopmentEventKind::SourceRuntimeLinked,
                serde_json::to_value(link)?,
            )?;
        }
        if !discovered.is_empty() {
            self.revision = self.revision.saturating_add(1);
        }
        Ok(discovered)
    }

    pub fn publish_rust_diagnostics(
        &mut self,
        path: &str,
    ) -> DevelopmentResult<Vec<LanguageDiagnostic>> {
        let mut client = LspClient::rust_analyzer(&self.root)?;
        let diagnostics = client.diagnostics(path)?;
        self.diagnostics.insert(path.into(), diagnostics.clone());
        self.record(
            DevelopmentEventKind::DiagnosticsPublished,
            serde_json::json!({"path": path, "count": diagnostics.len(), "diagnostics": diagnostics}),
        )?;
        Ok(diagnostics)
    }

    pub fn evaluate_semantic_breakpoints(
        &mut self,
        breakpoints: &[SemanticBreakpoint],
        before: &SemanticSnapshot,
        after: &SemanticSnapshot,
    ) -> DevelopmentResult<Vec<SemanticBreakpointHit>> {
        let hits = super::evaluate_breakpoints(breakpoints, before, after, &self.graph);
        for hit in &hits {
            self.record(
                DevelopmentEventKind::SemanticBreakpointHit,
                serde_json::to_value(hit)?,
            )?;
        }
        Ok(hits)
    }

    pub fn replay(&self, start: usize, limit: usize) -> DevelopmentResult<ReplayWindow> {
        super::replay(&self.timeline, start, limit)
    }

    pub fn search(&mut self, query: &str, limit: usize) -> DevelopmentResult<Vec<SearchHit>> {
        if query.trim().is_empty() || query.len() > 256 || limit == 0 || limit > 256 {
            return Err(DevelopmentError::InvalidInput(
                "search requires a 1-256 byte query and a 1-256 result limit".into(),
            ));
        }
        let mut hits = Vec::new();
        for file in self.list_files()? {
            if let Some(score) = super::fuzzy_score(query, &file.path) {
                hits.push(SearchHit {
                    kind: SearchKind::File,
                    label: file.path,
                    detail: file.git_status.unwrap_or_else(|| "project file".into()),
                    score,
                });
            }
        }
        for entity in self.graph.links.keys() {
            if let Some(score) = super::fuzzy_score(query, entity) {
                hits.push(SearchHit {
                    kind: SearchKind::BrowserEntity,
                    label: entity.clone(),
                    detail: "runtime/source graph entity".into(),
                    score,
                });
            }
        }
        for process in self.processes.list() {
            if let Some(score) = super::fuzzy_score(query, &process.name) {
                hits.push(SearchHit {
                    kind: SearchKind::Process,
                    label: process.name,
                    detail: format!("{:?}", process.state),
                    score,
                });
            }
        }
        for event in self.timeline.events() {
            let label = format!("{:?}", event.kind);
            if let Some(score) = super::fuzzy_score(query, &label) {
                hits.push(SearchHit {
                    kind: SearchKind::Event,
                    label,
                    detail: event.id.clone(),
                    score,
                });
            }
        }
        for command in [
            "project files",
            "project open",
            "project diagnostics",
            "project run",
            "project diff",
            "project timeline",
            "project replay",
            "project graph",
            "project experiment",
            "project neovim",
            "agent prompt",
        ] {
            if let Some(score) = super::fuzzy_score(query, command) {
                hits.push(SearchHit {
                    kind: SearchKind::Command,
                    label: command.into(),
                    detail: "command palette".into(),
                    score,
                });
            }
        }
        Ok(super::rank(hits, limit))
    }

    pub fn record(
        &mut self,
        kind: DevelopmentEventKind,
        payload: serde_json::Value,
    ) -> DevelopmentResult<()> {
        self.record_as(self.actor.clone(), kind, payload)
    }

    pub fn record_as(
        &mut self,
        actor: Actor,
        kind: DevelopmentEventKind,
        payload: serde_json::Value,
    ) -> DevelopmentResult<()> {
        self.timeline
            .record(actor, kind, self.root.display().to_string(), payload)?;
        Ok(())
    }

    fn resolve_path(
        &self,
        input: &str,
        allow_missing: bool,
    ) -> DevelopmentResult<(PathBuf, String)> {
        if input.is_empty() || input.len() > 512 {
            return Err(DevelopmentError::InvalidInput(
                "relative path must be 1-512 bytes".into(),
            ));
        }
        let path = Path::new(input);
        if path.is_absolute()
            || path
                .components()
                .any(|component| matches!(component, Component::ParentDir))
        {
            return Err(DevelopmentError::PathOutsideWorkspace(path.to_path_buf()));
        }
        let candidate = self.root.join(path);
        let mut existing_parent = candidate
            .parent()
            .ok_or_else(|| DevelopmentError::InvalidInput("path has no parent".into()))?;
        while !existing_parent.exists() {
            existing_parent = existing_parent
                .parent()
                .ok_or_else(|| DevelopmentError::PathOutsideWorkspace(candidate.clone()))?;
        }
        let canonical_parent = fs::canonicalize(existing_parent)?;
        if !canonical_parent.starts_with(&self.root) {
            return Err(DevelopmentError::PathOutsideWorkspace(candidate));
        }
        if candidate.exists() {
            let canonical = fs::canonicalize(&candidate)?;
            if !canonical.starts_with(&self.root) {
                return Err(DevelopmentError::PathOutsideWorkspace(candidate));
            }
        } else if !allow_missing {
            return Err(DevelopmentError::NotFound(input.into()));
        }
        Ok((candidate, path.to_string_lossy().into_owned()))
    }
}

pub fn detect_project(root: impl AsRef<Path>) -> DevelopmentResult<ProjectDetection> {
    let root = canonical_root(root.as_ref())?;
    let config_path = [root.join("glass.toml"), root.join(".glass.toml")]
        .into_iter()
        .find(|path| path.is_file());
    let mut languages = Vec::new();
    if root.join("Cargo.toml").is_file() {
        languages.push("Rust".into());
    }
    if root.join("package.json").is_file() {
        languages.push("JavaScript/TypeScript".into());
    }
    if root.join("pyproject.toml").is_file() || root.join("requirements.txt").is_file() {
        languages.push("Python".into());
    }
    if root.join("go.mod").is_file() {
        languages.push("Go".into());
    }
    let package_manager = if root.join("pnpm-lock.yaml").is_file() {
        Some("pnpm".into())
    } else if root.join("yarn.lock").is_file() {
        Some("yarn".into())
    } else if root.join("bun.lockb").is_file() || root.join("bun.lock").is_file() {
        Some("bun".into())
    } else if root.join("package-lock.json").is_file() {
        Some("npm".into())
    } else if root.join("Cargo.lock").is_file() {
        Some("cargo".into())
    } else {
        None
    };
    let framework = detect_framework(&root);
    let config = load_project_config(config_path.as_deref())?;
    let defaults = default_commands(&root, package_manager.as_deref());
    let build_system = if root.join("Cargo.toml").is_file() {
        Some("cargo".into())
    } else if root.join("package.json").is_file() {
        Some(package_manager.clone().unwrap_or_else(|| "npm".into()))
    } else if root.join("pyproject.toml").is_file() {
        Some("pyproject".into())
    } else if root.join("go.mod").is_file() {
        Some("go".into())
    } else {
        None
    };
    let formatter = if root.join("Cargo.toml").is_file() {
        Some("rustfmt".into())
    } else if root.join("package.json").is_file() {
        Some("prettier".into())
    } else if root.join("pyproject.toml").is_file() {
        Some("ruff format".into())
    } else if root.join("go.mod").is_file() {
        Some("gofmt".into())
    } else {
        None
    };
    let lsp_servers = [
        ("Rust", "rust-analyzer"),
        ("JavaScript/TypeScript", "typescript-language-server"),
        ("Python", "pyright-langserver"),
        ("Go", "gopls"),
    ]
    .into_iter()
    .filter(|(language, server)| {
        languages.iter().any(|value| value == language) && command_available(server)
    })
    .map(|(_, server)| server.to_string())
    .collect();
    let browser_url = config.browser.url;
    let local_development_urls = browser_url.iter().cloned().collect();
    Ok(ProjectDetection {
        root: root.clone(),
        languages,
        package_manager,
        framework,
        build_system,
        formatter,
        lsp_servers,
        git_branch: git_branch(&root),
        dev_command: config.commands.dev.or(defaults.0),
        test_command: config.commands.test.or(defaults.1),
        lint_command: config.commands.lint.or(defaults.2),
        build_command: config.commands.build.or(defaults.3),
        browser_url,
        local_development_urls,
        editor_engine: config.editor.engine,
        agent_harness: config.agent.harness,
        config_path,
    })
}

#[cfg(feature = "development-runtime")]
fn load_project_config(path: Option<&Path>) -> DevelopmentResult<GlassProjectConfig> {
    path.map(fs::read_to_string)
        .transpose()?
        .map(|text| {
            toml::from_str::<GlassProjectConfig>(&text)
                .map_err(|error| DevelopmentError::Config(error.to_string()))
        })
        .transpose()
        .map(Option::unwrap_or_default)
}

#[cfg(not(feature = "development-runtime"))]
fn load_project_config(path: Option<&Path>) -> DevelopmentResult<GlassProjectConfig> {
    if path.is_some() {
        return Err(DevelopmentError::Config(
            "glass.toml belongs to glass-dev; enable development-runtime".into(),
        ));
    }
    Ok(GlassProjectConfig::default())
}

fn canonical_root(root: &Path) -> DevelopmentResult<PathBuf> {
    let root = if root.is_file() {
        root.parent().unwrap_or(root)
    } else {
        root
    };
    let root = fs::canonicalize(root)?;
    if !root.is_dir() {
        return Err(DevelopmentError::InvalidInput(
            "project root must be a directory".into(),
        ));
    }
    Ok(root)
}

fn visit_files(root: &Path, current: &Path, entries: &mut Vec<FileEntry>) -> DevelopmentResult<()> {
    if entries.len() >= MAX_FILE_ENTRIES {
        return Ok(());
    }
    let mut children =
        fs::read_dir(current).map(|entries| entries.collect::<Result<Vec<_>, _>>())??;
    children.sort_by_key(|entry| entry.file_name());
    for entry in children {
        if entries.len() >= MAX_FILE_ENTRIES {
            break;
        }
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().into_owned();
        if name == ".git" || name == "target" || name == "node_modules" || name == ".glass" {
            continue;
        }
        let metadata = fs::symlink_metadata(&path)?;
        if metadata.file_type().is_symlink() {
            continue;
        }
        let relative = path
            .strip_prefix(root)
            .map_err(|_| DevelopmentError::PathOutsideWorkspace(path.clone()))?
            .to_string_lossy()
            .into_owned();
        if metadata.is_dir() {
            entries.push(FileEntry {
                path: relative,
                kind: FileKind::Directory,
                bytes: None,
                git_status: None,
                dirty: false,
                actor: None,
            });
            visit_files(root, &path, entries)?;
        } else if metadata.is_file() {
            entries.push(FileEntry {
                path: relative,
                kind: FileKind::File,
                bytes: Some(metadata.len()),
                git_status: None,
                dirty: false,
                actor: None,
            });
        }
    }
    Ok(())
}

fn read_existing(root: &Path, path: &str) -> DevelopmentResult<String> {
    let absolute = root.join(path);
    let metadata = fs::metadata(&absolute)?;
    if metadata.len() > MAX_BUFFER_BYTES as u64 {
        return Err(DevelopmentError::InvalidInput(
            "existing file exceeds buffer limit".into(),
        ));
    }
    Ok(fs::read_to_string(absolute)?)
}

fn hash(content: &str) -> String {
    Sha256::digest(content.as_bytes())
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn git_branch(root: &Path) -> Option<String> {
    Command::new("git")
        .args(["rev-parse", "--abbrev-ref", "HEAD"])
        .current_dir(root)
        .output()
        .ok()
        .filter(|output| output.status.success())
        .and_then(|output| String::from_utf8(output.stdout).ok())
        .map(|branch| branch.trim().to_string())
        .filter(|branch| !branch.is_empty())
}

fn git_statuses(root: &Path) -> BTreeMap<String, String> {
    let Some(output) = Command::new("git")
        .args(["status", "--porcelain=v1", "-z", "--untracked-files=all"])
        .current_dir(root)
        .output()
        .ok()
        .filter(|output| output.status.success())
    else {
        return BTreeMap::new();
    };
    let mut statuses = BTreeMap::new();
    for entry in output
        .stdout
        .split(|byte| *byte == 0)
        .filter(|entry| entry.len() >= 4)
    {
        let status = String::from_utf8_lossy(&entry[..2]).trim().to_string();
        let path = String::from_utf8_lossy(&entry[3..]).into_owned();
        if !path.is_empty() {
            statuses.insert(path, status);
        }
    }
    statuses
}

fn detect_framework(root: &Path) -> Option<String> {
    if let Ok(package) = fs::read_to_string(root.join("package.json"))
        && let Ok(value) = serde_json::from_str::<serde_json::Value>(&package)
    {
        let dependencies = value
            .get("dependencies")
            .and_then(serde_json::Value::as_object);
        let dev_dependencies = value
            .get("devDependencies")
            .and_then(serde_json::Value::as_object);
        if let Some(framework) =
            ["next", "vite", "react", "vue", "svelte"]
                .into_iter()
                .find(|framework| {
                    dependencies.is_some_and(|values| values.contains_key(*framework))
                        || dev_dependencies.is_some_and(|values| values.contains_key(*framework))
                })
        {
            return Some(framework.into());
        }
    }
    let python = fs::read_to_string(root.join("pyproject.toml"))
        .or_else(|_| fs::read_to_string(root.join("requirements.txt")))
        .unwrap_or_default()
        .to_ascii_lowercase();
    ["django", "flask", "fastapi"]
        .into_iter()
        .find(|framework| python.contains(framework))
        .map(str::to_string)
}

fn default_commands(
    root: &Path,
    package_manager: Option<&str>,
) -> (
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
) {
    if root.join("Cargo.toml").is_file() {
        return (
            None,
            Some("cargo test".into()),
            Some("cargo clippy --all-targets --all-features -- -D warnings".into()),
            Some("cargo build".into()),
        );
    }
    if root.join("package.json").is_file() {
        let manager = package_manager.unwrap_or("npm");
        let package = fs::read_to_string(root.join("package.json"))
            .ok()
            .and_then(|value| serde_json::from_str::<serde_json::Value>(&value).ok());
        let command = |script: &str| {
            package
                .as_ref()
                .and_then(|value| value.pointer(&format!("/scripts/{script}")))
                .and_then(serde_json::Value::as_str)
                .map(|_| format!("{manager} run {script}"))
        };
        return (
            command("dev"),
            command("test"),
            command("lint"),
            command("build"),
        );
    }
    if root.join("pyproject.toml").is_file() {
        return (
            None,
            Some("python -m pytest".into()),
            Some("ruff check .".into()),
            None,
        );
    }
    if root.join("go.mod").is_file() {
        return (
            None,
            Some("go test ./...".into()),
            Some("go vet ./...".into()),
            Some("go build ./...".into()),
        );
    }
    (None, None, None, None)
}

fn command_available(command: &str) -> bool {
    Command::new(command)
        .arg("--version")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    fn fixture() -> PathBuf {
        static NEXT_FIXTURE: AtomicUsize = AtomicUsize::new(0);
        let root = std::env::temp_dir().join(format!(
            "glass-project-{}-{}",
            std::process::id(),
            NEXT_FIXTURE.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("src")).unwrap();
        fs::write(
            root.join("Cargo.toml"),
            "[package]\nname='fixture'\nversion='0.1.0'\nedition='2024'\n",
        )
        .unwrap();
        fs::write(root.join("src/main.rs"), "fn main() {}\n").unwrap();
        root
    }

    #[test]
    fn detects_cargo_project_and_defaults() {
        let root = fixture();
        let detection = detect_project(&root).unwrap();
        assert_eq!(detection.languages, vec!["Rust"]);
        assert_eq!(detection.test_command.as_deref(), Some("cargo test"));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn file_writes_are_confined_and_atomic() {
        let root = fixture();
        let mut project = ProjectWorkspace::open(&root).unwrap();
        project
            .write_file("src/lib.rs", "pub fn ok() {}\n", Actor::local())
            .unwrap();
        assert!(
            project
                .read_file("src/lib.rs")
                .unwrap()
                .contains("pub fn ok")
        );
        assert!(matches!(
            project.write_file("../escape", "bad", Actor::local()),
            Err(DevelopmentError::PathOutsideWorkspace(_))
        ));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn editor_save_records_actor_and_clears_dirty_state() {
        let root = fixture();
        let mut project = ProjectWorkspace::open(&root).unwrap();
        project.open_buffer("src/main.rs", Actor::local()).unwrap();
        project
            .edit_buffer(
                "src/main.rs",
                "fn main() { println!(\"ok\"); }\n".into(),
                Actor::local(),
            )
            .unwrap();
        assert!(project.buffer("src/main.rs").unwrap().dirty);
        project.save_buffer("src/main.rs").unwrap();
        assert!(!project.buffer("src/main.rs").unwrap().dirty);
        assert!(
            project
                .timeline()
                .events()
                .any(|event| matches!(event.kind, DevelopmentEventKind::FileSaved))
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn editor_undo_redo_and_external_change_conflicts_are_explicit() {
        let root = fixture();
        let mut project = ProjectWorkspace::open(&root).unwrap();
        project.open_buffer("src/main.rs", Actor::local()).unwrap();
        project
            .edit_buffer("src/main.rs", "one".into(), Actor::local())
            .unwrap();
        project
            .edit_buffer("src/main.rs", "two".into(), Actor::local())
            .unwrap();
        assert_eq!(project.undo_buffer("src/main.rs").unwrap().content, "one");
        assert_eq!(project.redo_buffer("src/main.rs").unwrap().content, "two");
        fs::write(root.join("src/main.rs"), "external").unwrap();
        assert!(matches!(
            project.save_buffer("src/main.rs"),
            Err(DevelopmentError::Conflict(_))
        ));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn file_operations_support_nested_create_rename_and_safe_delete() {
        let root = fixture();
        let mut project = ProjectWorkspace::open(&root).unwrap();
        project
            .edit_buffer("nested/new.txt", "hello".into(), Actor::local())
            .unwrap();
        project.save_buffer("nested/new.txt").unwrap();
        project
            .rename_path("nested/new.txt", "nested/renamed.txt", Actor::local())
            .unwrap();
        assert!(root.join("nested/renamed.txt").is_file());
        project
            .delete_path("nested/renamed.txt", Actor::local())
            .unwrap();
        project.delete_path("nested", Actor::local()).unwrap();
        assert!(!root.join("nested").exists());
        let _ = fs::remove_dir_all(root);
    }
}
