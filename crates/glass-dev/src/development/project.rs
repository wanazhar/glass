use super::editor::{selection_offsets, text_position_at_offset, text_position_offset};
use super::{
    Actor, DevelopmentError, DevelopmentEventKind, DevelopmentGraph, DevelopmentResult,
    LanguageDiagnostic, LspClient, MAX_BUFFER_BYTES, MAX_FILE_BYTES, MAX_FILE_ENTRIES,
    ProcessManager, ReplayWindow, SearchHit, SearchKind, SemanticBreakpoint, SemanticBreakpointHit,
    SemanticSnapshot, SourceLocation, Timeline, read_bounded_utf8,
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
    sync::Mutex,
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

/// Explicit result envelope for a bounded project-tree traversal.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ProjectTreeResult {
    pub entries: Vec<FileEntry>,
    pub truncated: bool,
    pub limit: usize,
    pub ignored_directories: Vec<String>,
    pub skipped_symlinks: usize,
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub selection: Option<super::TextSelection>,
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
    language: Option<LspClient>,
    tree_cache: Mutex<Option<(Instant, ProjectTreeResult)>>,
    actors: BTreeMap<String, Actor>,
    actor: Actor,
    revision: u64,
    comments: BTreeMap<String, Vec<super::EditorComment>>,
    proposals: BTreeMap<String, super::EditorProposal>,
    checkpoints: BTreeMap<String, super::EditorCheckpoint>,
    checkpoints_path: PathBuf,
    next_editor_id: u64,
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
    /// Read the persisted event feed without opening a workspace or recording
    /// a synthetic `workspaceOpened` event. This keeps monitoring read-only.
    pub fn event_page(
        root: impl AsRef<Path>,
        after_id: Option<&str>,
        limit: usize,
    ) -> DevelopmentResult<super::DevelopmentEventPage> {
        let root = canonical_root(root.as_ref())?;
        Ok(Timeline::for_project(&root)?.events_after(after_id, limit))
    }

    pub fn timeline_snapshot(
        root: impl AsRef<Path>,
    ) -> DevelopmentResult<Vec<super::DevelopmentEvent>> {
        let root = canonical_root(root.as_ref())?;
        Ok(Timeline::for_project(&root)?.events().cloned().collect())
    }

    pub fn open(root: impl AsRef<Path>) -> DevelopmentResult<Self> {
        let root = canonical_root(root.as_ref())?;
        let detection = detect_project(&root)?;
        let config = load_project_config(detection.config_path.as_deref())?;
        let timeline = Timeline::for_project(&root)?;
        let graph_path = timeline.path().with_file_name("graph.json");
        let checkpoints_path = timeline.path().with_file_name("editor-checkpoints.json");
        let graph = DevelopmentGraph::load(&graph_path)?;
        let checkpoints = load_editor_checkpoints(&checkpoints_path)?;
        let comments = comments_from_timeline(&timeline);
        let proposals = proposals_from_timeline(&timeline);
        let next_editor_id = timeline.events().count() as u64 + 1;
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
            language: None,
            tree_cache: Mutex::new(None),
            actors,
            actor,
            revision: 0,
            comments,
            proposals,
            checkpoints,
            checkpoints_path,
            next_editor_id,
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

    fn invalidate_tree_cache(&self) {
        if let Ok(mut cache) = self.tree_cache.lock() {
            *cache = None;
        }
    }

    pub fn actors(&self) -> impl Iterator<Item = &Actor> {
        self.actors.values()
    }

    pub fn collaboration(&mut self) -> &mut super::CollaborationBus {
        &mut self.collaboration
    }

    pub fn set_buffer_selection(
        &mut self,
        path: &str,
        selection: Option<super::TextSelection>,
        actor: Actor,
    ) -> DevelopmentResult<EditorBuffer> {
        let content = self
            .buffers
            .get(path)
            .map(|buffer| buffer.content.clone())
            .ok_or_else(|| DevelopmentError::NotFound(format!("buffer {path}")))?;
        if let Some(selection) = selection.as_ref()
            && (text_position_offset(&content, selection.anchor).is_none()
                || text_position_offset(&content, selection.active).is_none())
        {
            return Err(DevelopmentError::InvalidInput(
                "editor selection positions must be one-based and within the buffer".into(),
            ));
        }
        let buffer = self
            .buffers
            .get_mut(path)
            .ok_or_else(|| DevelopmentError::NotFound(format!("buffer {path}")))?;
        buffer.selection = selection;
        buffer.actor = actor;
        Ok(buffer.clone())
    }

    /// Return the selected text from a shared buffer, if the selection is non-empty.
    pub fn selected_editor_text(&self, path: &str) -> DevelopmentResult<Option<String>> {
        let buffer = self
            .buffers
            .get(path)
            .ok_or_else(|| DevelopmentError::NotFound(format!("buffer {path}")))?;
        let Some(selection) = buffer.selection.as_ref() else {
            return Ok(None);
        };
        let Some((start, end)) = selection_offsets(&buffer.content, selection) else {
            return Err(DevelopmentError::InvalidInput(
                "editor selection positions must be one-based and within the buffer".into(),
            ));
        };
        if start == end {
            return Ok(None);
        }
        Ok(Some(buffer.content[start..end].to_string()))
    }

    /// Replace the human's current selection and leave the cursor after the replacement.
    pub fn replace_buffer_selection(
        &mut self,
        path: &str,
        replacement: String,
        actor: Actor,
    ) -> DevelopmentResult<EditorBuffer> {
        let buffer = self
            .buffers
            .get(path)
            .cloned()
            .ok_or_else(|| DevelopmentError::NotFound(format!("buffer {path}")))?;
        let selection = buffer.selection.as_ref().ok_or_else(|| {
            DevelopmentError::InvalidInput(format!("editor buffer {path} has no selection"))
        })?;
        let Some((start, end)) = selection_offsets(&buffer.content, selection) else {
            return Err(DevelopmentError::InvalidInput(
                "editor selection positions must be one-based and within the buffer".into(),
            ));
        };
        if start == end {
            return Err(DevelopmentError::InvalidInput(format!(
                "editor buffer {path} has an empty selection"
            )));
        }
        let mut content = buffer.content;
        content.replace_range(start..end, &replacement);
        let cursor =
            text_position_at_offset(&content, start + replacement.len()).ok_or_else(|| {
                DevelopmentError::InvalidInput(
                    "replacement ended at an invalid UTF-8 boundary".into(),
                )
            })?;
        self.edit_buffer(path, content, actor.clone())?;
        self.set_buffer_cursor(path, cursor.line, cursor.column)?;
        self.set_buffer_selection(path, None, actor)?;
        self.buffer(path)
            .cloned()
            .ok_or_else(|| DevelopmentError::NotFound(format!("buffer {path}")))
    }

    pub fn editor_comments(&self, path: Option<&str>) -> Vec<super::EditorComment> {
        self.comments
            .values()
            .flat_map(|comments| comments.iter())
            .filter(|comment| path.is_none_or(|path| path == comment.path))
            .cloned()
            .collect()
    }

    pub fn add_editor_comment(
        &mut self,
        path: &str,
        start_line: u32,
        end_line: u32,
        text: String,
        actor: Actor,
    ) -> DevelopmentResult<super::EditorComment> {
        self.validate_editor_range(path, start_line, end_line)?;
        if text.trim().is_empty() || text.len() > 16 * 1024 {
            return Err(DevelopmentError::InvalidInput(
                "editor comment must contain 1-16384 bytes".into(),
            ));
        }
        let revision = self.bump_editor_revision();
        let comment = super::EditorComment {
            id: self.next_editor_id("comment"),
            path: path.into(),
            start_line,
            end_line,
            text,
            actor: actor.clone(),
            state: super::EditorCommentState::Open,
            created_revision: revision,
            updated_revision: revision,
        };
        self.record_as(
            actor,
            DevelopmentEventKind::EditorCommentAdded,
            serde_json::to_value(&comment)?,
        )?;
        self.comments
            .entry(path.into())
            .or_default()
            .push(comment.clone());
        Ok(comment)
    }

    pub fn resolve_editor_comment(
        &mut self,
        id: &str,
        actor: Actor,
    ) -> DevelopmentResult<super::EditorComment> {
        let (path, index) = self.find_comment(id)?;
        let current = self
            .comments
            .get(&path)
            .and_then(|comments| comments.get(index))
            .cloned()
            .ok_or_else(|| DevelopmentError::NotFound(format!("comment {id}")))?;
        if current.state == super::EditorCommentState::Resolved {
            return Ok(current);
        }
        let revision = self.bump_editor_revision();
        let comment = self
            .comments
            .get_mut(&path)
            .and_then(|comments| comments.get_mut(index))
            .ok_or_else(|| DevelopmentError::NotFound(format!("comment {id}")))?;
        comment.state = super::EditorCommentState::Resolved;
        comment.updated_revision = revision;
        let result = comment.clone();
        self.record_as(
            actor,
            DevelopmentEventKind::EditorCommentResolved,
            serde_json::to_value(&result)?,
        )?;
        Ok(result)
    }

    pub fn editor_proposals(&self) -> Vec<super::EditorProposal> {
        self.proposals.values().cloned().collect()
    }

    pub fn propose_editor_change(
        &mut self,
        path: &str,
        original: String,
        proposed: String,
        summary: String,
        actor: Actor,
    ) -> DevelopmentResult<super::EditorProposal> {
        self.validate_editor_path(path)?;
        if original.len() > MAX_BUFFER_BYTES || proposed.len() > MAX_BUFFER_BYTES {
            return Err(DevelopmentError::InvalidInput(format!(
                "editor proposal exceeds the {} byte buffer limit",
                MAX_BUFFER_BYTES
            )));
        }
        if summary.trim().is_empty() || summary.len() > 1024 {
            return Err(DevelopmentError::InvalidInput(
                "editor proposal summary must contain 1-1024 bytes".into(),
            ));
        }
        let current = self.current_editor_content(path)?;
        if current != original {
            return Err(DevelopmentError::Conflict(format!(
                "editor buffer {path} changed before the proposal was created"
            )));
        }
        let revision = self.bump_editor_revision();
        let proposal = super::EditorProposal {
            id: self.next_editor_id("proposal"),
            path: path.into(),
            summary,
            actor: actor.clone(),
            base_hash: hash(&original),
            base_revision: revision,
            original,
            proposed,
            state: super::EditorProposalState::Pending,
            created_revision: revision,
            updated_revision: revision,
        };
        self.record_as(
            actor,
            DevelopmentEventKind::EditorProposalCreated,
            serde_json::to_value(&proposal)?,
        )?;
        self.proposals.insert(proposal.id.clone(), proposal.clone());
        Ok(proposal)
    }

    pub fn accept_editor_proposal(
        &mut self,
        id: &str,
        actor: Actor,
    ) -> DevelopmentResult<EditorBuffer> {
        let proposal = self
            .proposals
            .get(id)
            .cloned()
            .ok_or_else(|| DevelopmentError::NotFound(format!("proposal {id}")))?;
        if proposal.state != super::EditorProposalState::Pending {
            return Err(DevelopmentError::Conflict(format!(
                "proposal {id} is already {:?}",
                proposal.state
            )));
        }
        if self.current_editor_content(&proposal.path)? != proposal.original
            || hash(&proposal.original) != proposal.base_hash
        {
            let revision = self.bump_editor_revision();
            if let Some(stale) = self.proposals.get_mut(id) {
                stale.state = super::EditorProposalState::Stale;
                stale.updated_revision = revision;
            }
            self.record_as(
                actor,
                DevelopmentEventKind::EditorProposalRejected,
                serde_json::json!({"id": id, "reason": "stale: base content changed"}),
            )?;
            return Err(DevelopmentError::Conflict(format!(
                "proposal {id} is stale because {} changed",
                proposal.path
            )));
        }
        let buffer = self
            .edit_buffer(&proposal.path, proposal.proposed.clone(), actor.clone())
            .map(|_| self.buffer(&proposal.path).cloned())?
            .ok_or_else(|| DevelopmentError::NotFound(format!("buffer {}", proposal.path)))?;
        let revision = self.bump_editor_revision();
        if let Some(accepted) = self.proposals.get_mut(id) {
            accepted.state = super::EditorProposalState::Accepted;
            accepted.updated_revision = revision;
        }
        self.record_as(
            actor,
            DevelopmentEventKind::EditorProposalAccepted,
            serde_json::json!({"id": id, "path": proposal.path}),
        )?;
        Ok(buffer)
    }

    pub fn reject_editor_proposal(
        &mut self,
        id: &str,
        actor: Actor,
    ) -> DevelopmentResult<super::EditorProposal> {
        let current = self
            .proposals
            .get(id)
            .cloned()
            .ok_or_else(|| DevelopmentError::NotFound(format!("proposal {id}")))?;
        if current.state != super::EditorProposalState::Pending {
            return Err(DevelopmentError::Conflict(format!(
                "proposal {id} is already {:?}",
                current.state
            )));
        }
        let revision = self.bump_editor_revision();
        let proposal = self
            .proposals
            .get_mut(id)
            .ok_or_else(|| DevelopmentError::NotFound(format!("proposal {id}")))?;
        proposal.state = super::EditorProposalState::Rejected;
        proposal.updated_revision = revision;
        let result = proposal.clone();
        self.record_as(
            actor,
            DevelopmentEventKind::EditorProposalRejected,
            serde_json::to_value(&result)?,
        )?;
        Ok(result)
    }

    pub fn editor_checkpoints(&self) -> Vec<super::EditorCheckpoint> {
        self.checkpoints.values().cloned().collect()
    }

    pub fn create_editor_checkpoint(
        &mut self,
        name: String,
        actor: Actor,
    ) -> DevelopmentResult<super::EditorCheckpoint> {
        if name.trim().is_empty() || name.len() > 256 {
            return Err(DevelopmentError::InvalidInput(
                "checkpoint name must contain 1-256 bytes".into(),
            ));
        }
        let revision = self.bump_editor_revision();
        let checkpoint = super::EditorCheckpoint {
            id: self.next_editor_id("checkpoint"),
            name,
            actor: actor.clone(),
            revision,
            buffers: self.buffers.values().cloned().collect(),
        };
        self.checkpoints
            .insert(checkpoint.id.clone(), checkpoint.clone());
        self.persist_checkpoints()?;
        self.record_as(
            actor,
            DevelopmentEventKind::EditorCheckpointCreated,
            serde_json::to_value(&checkpoint)?,
        )?;
        Ok(checkpoint)
    }

    pub fn restore_editor_checkpoint(
        &mut self,
        id: &str,
        actor: Actor,
    ) -> DevelopmentResult<Vec<EditorBuffer>> {
        let checkpoint = self
            .checkpoints
            .get(id)
            .cloned()
            .ok_or_else(|| DevelopmentError::NotFound(format!("checkpoint {id}")))?;
        for restored in &checkpoint.buffers {
            if let Some(current) = self.buffers.get(&restored.path) {
                self.undo
                    .entry(restored.path.clone())
                    .or_default()
                    .push(current.content.clone());
            }
            self.buffers.insert(restored.path.clone(), restored.clone());
        }
        let revision = self.bump_editor_revision();
        self.record_as(
            actor,
            DevelopmentEventKind::EditorCheckpointRestored,
            serde_json::json!({"id": id, "revision": revision}),
        )?;
        self.invalidate_tree_cache();
        Ok(checkpoint.buffers)
    }

    fn validate_editor_path(&self, path: &str) -> DevelopmentResult<()> {
        if self.buffers.contains_key(path) {
            return Ok(());
        }
        self.resolve_path(path, false).map(|_| ())
    }

    fn validate_editor_range(
        &self,
        path: &str,
        start_line: u32,
        end_line: u32,
    ) -> DevelopmentResult<()> {
        self.validate_editor_path(path)?;
        if start_line == 0 || end_line < start_line {
            return Err(DevelopmentError::InvalidInput(
                "editor comment range must be one-based and ordered".into(),
            ));
        }
        let content = self
            .buffers
            .get(path)
            .map(|buffer| buffer.content.clone())
            .map(Ok)
            .unwrap_or_else(|| self.read_file_snapshot(path))?;
        let line_count = content.split('\n').count().max(1) as u32;
        if end_line > line_count {
            return Err(DevelopmentError::InvalidInput(format!(
                "editor comment range ends at line {end_line}, but {path} has {line_count} lines"
            )));
        }
        Ok(())
    }

    fn current_editor_content(&self, path: &str) -> DevelopmentResult<String> {
        self.buffer_or_file_content(path)
    }

    pub(crate) fn buffer_or_file_content(&self, path: &str) -> DevelopmentResult<String> {
        if let Some(buffer) = self.buffers.get(path) {
            return Ok(buffer.content.clone());
        }
        match self.read_file_snapshot(path) {
            Ok(content) => Ok(content),
            Err(DevelopmentError::Io(error)) if error.kind() == std::io::ErrorKind::NotFound => {
                Ok(String::new())
            }
            Err(error) => Err(error),
        }
    }

    fn find_comment(&self, id: &str) -> DevelopmentResult<(String, usize)> {
        self.comments
            .iter()
            .find_map(|(path, comments)| {
                comments
                    .iter()
                    .position(|comment| comment.id == id)
                    .map(|index| (path.clone(), index))
            })
            .ok_or_else(|| DevelopmentError::NotFound(format!("comment {id}")))
    }

    fn bump_editor_revision(&mut self) -> u64 {
        self.revision = self.revision.saturating_add(1);
        self.revision
    }

    fn next_editor_id(&mut self, kind: &str) -> String {
        let id = format!("{kind}-{}-{}", self.revision, self.next_editor_id);
        self.next_editor_id = self.next_editor_id.saturating_add(1);
        id
    }

    fn persist_checkpoints(&self) -> DevelopmentResult<()> {
        let data = serde_json::to_vec_pretty(&self.checkpoints)?;
        fs::write(&self.checkpoints_path, data)?;
        Ok(())
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
        Ok(self.list_files_result()?.entries)
    }

    /// Traverse the project with explicit bounds and skip evidence.
    pub fn list_files_result(&self) -> DevelopmentResult<ProjectTreeResult> {
        {
            let cache = self.tree_cache.lock().map_err(|_| {
                DevelopmentError::Conflict("project tree cache is unavailable".into())
            })?;
            if let Some((created, result)) = cache.as_ref()
                && created.elapsed() <= Duration::from_millis(250)
            {
                return Ok(result.clone());
            }
        }
        let mut result = ProjectTreeResult {
            entries: Vec::new(),
            truncated: false,
            limit: MAX_FILE_ENTRIES,
            ignored_directories: Vec::new(),
            skipped_symlinks: 0,
        };
        visit_files(&self.root, &self.root, &mut result)?;
        let git = git_statuses(&self.root);
        for entry in &mut result.entries {
            entry.git_status = git.get(&entry.path).cloned();
            if let Some(buffer) = self.buffers.get(&entry.path) {
                entry.dirty = buffer.dirty;
                entry.actor = Some(buffer.actor.clone());
            }
        }
        result
            .entries
            .sort_by(|left, right| left.path.cmp(&right.path));
        result.ignored_directories.sort();
        result.ignored_directories.dedup();
        *self.tree_cache.lock().map_err(|_| {
            DevelopmentError::Conflict("project tree cache is unavailable".into())
        })? = Some((Instant::now(), result.clone()));
        Ok(result)
    }

    pub fn read_file(&mut self, path: &str) -> DevelopmentResult<String> {
        let (_, relative) = self.resolve_path(path, false)?;
        let content = self.read_file_snapshot(path)?;
        self.record(
            DevelopmentEventKind::FileOpened,
            serde_json::json!({"path": relative}),
        )?;
        Ok(content)
    }

    /// Read a confined file without producing one timeline event per file in a
    /// bounded aggregate operation such as project grep.
    pub(crate) fn read_file_snapshot(&self, path: &str) -> DevelopmentResult<String> {
        let (absolute, _) = self.resolve_path(path, false)?;
        read_bounded_utf8(&absolute, MAX_FILE_BYTES, "project file")
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
            selection: None,
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
                        selection: None,
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
        self.invalidate_tree_cache();
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
        let result = buffer.clone();
        self.invalidate_tree_cache();
        Ok(result)
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
        let result = buffer.clone();
        self.invalidate_tree_cache();
        Ok(result)
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
        self.invalidate_tree_cache();
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
        self.invalidate_tree_cache();
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
        self.invalidate_tree_cache();
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
        self.invalidate_tree_cache();
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
        if self.processes.list_checked()?.iter().any(|process| {
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
            serde_json::json!({
                "name": name,
                "commandBytes": command.len(),
                "commandSha256": hash(command),
                "pid": snapshot.pid
            }),
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
            serde_json::json!({
                "name": name,
                "commandBytes": command.len(),
                "commandSha256": hash(command)
            }),
        )?;
        #[cfg(windows)]
        {
            let snapshot = self.processes.run_bounded(name, command, timeout)?;
            self.record(
                DevelopmentEventKind::TestCompleted,
                serde_json::json!({
                    "name": name,
                    "state": snapshot.state,
                    "outputBytes": snapshot.output.len(),
                    "outputSha256": hash(&snapshot.output)
                }),
            )?;
            return Ok(snapshot);
        }
        #[cfg(not(windows))]
        self.start_process(name, command)?;
        #[cfg(not(windows))]
        self.processes.close_input(name)?;
        #[cfg(not(windows))]
        let deadline = Instant::now() + timeout;
        #[cfg(not(windows))]
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
        #[cfg(not(windows))]
        self.record(
            DevelopmentEventKind::TestCompleted,
            serde_json::json!({
                "name": name,
                "state": snapshot.state,
                "outputBytes": snapshot.output.len(),
                "outputSha256": hash(&snapshot.output)
            }),
        )?;
        #[cfg(not(windows))]
        Ok(snapshot)
    }

    pub fn run_command_to_completion(
        &mut self,
        name: &str,
        command: &str,
        timeout: Duration,
    ) -> DevelopmentResult<super::ProcessSnapshot> {
        #[cfg(windows)]
        return self.processes.run_bounded(name, command, timeout);

        #[cfg(not(windows))]
        self.start_process(name, command)?;
        #[cfg(not(windows))]
        self.processes.close_input(name)?;
        #[cfg(not(windows))]
        let deadline = Instant::now() + timeout;
        #[cfg(not(windows))]
        loop {
            let snapshot = self
                .processes
                .poll()?
                .into_iter()
                .find(|snapshot| snapshot.name == name)
                .ok_or_else(|| DevelopmentError::NotFound(format!("process {name}")))?;
            if !matches!(snapshot.state, super::ProcessState::Running) {
                self.record(
                    DevelopmentEventKind::ProcessExited,
                    serde_json::json!({
                        "name": name,
                        "state": snapshot.state,
                        "outputBytes": snapshot.output.len(),
                        "outputSha256": hash(&snapshot.output)
                    }),
                )?;
                return Ok(snapshot);
            }
            if Instant::now() >= deadline {
                self.stop_process(name)?;
                return Err(DevelopmentError::Process(format!(
                    "command {name} exceeded {} seconds",
                    timeout.as_secs()
                )));
            }
            std::thread::sleep(Duration::from_millis(25));
        }
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
        if self.language.is_none() {
            self.language = Some(LspClient::rust_analyzer(&self.root)?);
        }
        let diagnostics = self
            .language
            .as_mut()
            .expect("language client initialized")
            .diagnostics(path)?;
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
        for process in self.processes.list_checked()? {
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
        Ok((candidate, portable_relative_path(path)))
    }
}

fn portable_relative_path(path: &Path) -> String {
    path.components()
        .map(|component| component.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/")
}

fn load_editor_checkpoints(
    path: &Path,
) -> DevelopmentResult<BTreeMap<String, super::EditorCheckpoint>> {
    if !path.is_file() {
        return Ok(BTreeMap::new());
    }
    let data = fs::read(path)?;
    serde_json::from_slice(&data)
        .map_err(|error| DevelopmentError::Config(format!("invalid editor checkpoints: {error}")))
}

fn comments_from_timeline(timeline: &Timeline) -> BTreeMap<String, Vec<super::EditorComment>> {
    let mut comments: BTreeMap<String, Vec<super::EditorComment>> = BTreeMap::new();
    for event in timeline.events() {
        match event.kind {
            DevelopmentEventKind::EditorCommentAdded => {
                if let Ok(comment) =
                    serde_json::from_value::<super::EditorComment>(event.payload.clone())
                {
                    comments
                        .entry(comment.path.clone())
                        .or_default()
                        .push(comment);
                }
            }
            DevelopmentEventKind::EditorCommentResolved => {
                let Some(id) = event.payload.get("id").and_then(serde_json::Value::as_str) else {
                    continue;
                };
                for comment in comments.values_mut().flat_map(|items| items.iter_mut()) {
                    if comment.id == id {
                        comment.state = super::EditorCommentState::Resolved;
                        if let Some(revision) = event
                            .payload
                            .get("updatedRevision")
                            .and_then(serde_json::Value::as_u64)
                        {
                            comment.updated_revision = revision;
                        }
                    }
                }
            }
            _ => {}
        }
    }
    comments
}

fn proposals_from_timeline(timeline: &Timeline) -> BTreeMap<String, super::EditorProposal> {
    let mut proposals = BTreeMap::new();
    for event in timeline.events() {
        match event.kind {
            DevelopmentEventKind::EditorProposalCreated => {
                if let Ok(proposal) =
                    serde_json::from_value::<super::EditorProposal>(event.payload.clone())
                {
                    proposals.insert(proposal.id.clone(), proposal);
                }
            }
            DevelopmentEventKind::EditorProposalAccepted
            | DevelopmentEventKind::EditorProposalRejected => {
                let Some(id) = event.payload.get("id").and_then(serde_json::Value::as_str) else {
                    continue;
                };
                if let Some(proposal) = proposals.get_mut(id) {
                    proposal.state =
                        if matches!(event.kind, DevelopmentEventKind::EditorProposalAccepted) {
                            super::EditorProposalState::Accepted
                        } else if event
                            .payload
                            .get("reason")
                            .and_then(serde_json::Value::as_str)
                            .is_some_and(|reason| reason.contains("stale"))
                        {
                            super::EditorProposalState::Stale
                        } else {
                            super::EditorProposalState::Rejected
                        };
                }
            }
            _ => {}
        }
    }
    proposals
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

pub(crate) fn canonical_root(root: &Path) -> DevelopmentResult<PathBuf> {
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

fn visit_files(
    root: &Path,
    current: &Path,
    result: &mut ProjectTreeResult,
) -> DevelopmentResult<()> {
    if result.entries.len() >= MAX_FILE_ENTRIES {
        result.truncated = true;
        return Ok(());
    }
    let mut children =
        fs::read_dir(current).map(|entries| entries.collect::<Result<Vec<_>, _>>())??;
    children.sort_by_key(|entry| entry.file_name());
    for entry in children {
        if result.entries.len() >= MAX_FILE_ENTRIES {
            result.truncated = true;
            break;
        }
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().into_owned();
        if name == ".git" || name == "target" || name == "node_modules" || name == ".glass" {
            result.ignored_directories.push(name);
            continue;
        }
        let metadata = fs::symlink_metadata(&path)?;
        if metadata.file_type().is_symlink() {
            result.skipped_symlinks = result.skipped_symlinks.saturating_add(1);
            continue;
        }
        let relative = path
            .strip_prefix(root)
            .map_err(|_| DevelopmentError::PathOutsideWorkspace(path.clone()))?;
        let relative = portable_relative_path(relative);
        if metadata.is_dir() {
            result.entries.push(FileEntry {
                path: relative,
                kind: FileKind::Directory,
                bytes: None,
                git_status: None,
                dirty: false,
                actor: None,
            });
            visit_files(root, &path, result)?;
        } else if metadata.is_file() {
            result.entries.push(FileEntry {
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
    read_bounded_utf8(&absolute, MAX_BUFFER_BYTES, "existing editor file")
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
    fn event_feed_reads_do_not_record_monitoring_activity() {
        let root = fixture();
        drop(ProjectWorkspace::open(&root).unwrap());
        let before = ProjectWorkspace::timeline_snapshot(&root).unwrap();
        let page = ProjectWorkspace::event_page(&root, None, 64).unwrap();
        let after = ProjectWorkspace::timeline_snapshot(&root).unwrap();
        assert_eq!(page.events, before);
        assert_eq!(after, before);
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
    fn editor_selection_replacement_is_atomic_and_unicode_safe() {
        let root = fixture();
        let mut project = ProjectWorkspace::open(&root).unwrap();
        project.open_buffer("src/main.rs", Actor::local()).unwrap();
        project
            .edit_buffer("src/main.rs", "αlpha\nβeta\n".into(), Actor::local())
            .unwrap();
        project
            .set_buffer_selection(
                "src/main.rs",
                Some(crate::development::TextSelection {
                    anchor: crate::development::TextPosition { line: 1, column: 2 },
                    active: crate::development::TextPosition { line: 2, column: 3 },
                }),
                Actor::local(),
            )
            .unwrap();
        assert_eq!(
            project
                .selected_editor_text("src/main.rs")
                .unwrap()
                .as_deref(),
            Some("lpha\nβe")
        );
        let buffer = project
            .replace_buffer_selection("src/main.rs", "X\nY".into(), Actor::local())
            .unwrap();
        assert_eq!(buffer.content, "αX\nYta\n");
        assert_eq!(buffer.cursor_line, 2);
        assert_eq!(buffer.cursor_column, 2);
        assert!(buffer.selection.is_none());
        assert!(matches!(
            project.set_buffer_selection(
                "src/main.rs",
                Some(crate::development::TextSelection {
                    anchor: crate::development::TextPosition { line: 9, column: 1 },
                    active: crate::development::TextPosition { line: 9, column: 1 },
                }),
                Actor::local(),
            ),
            Err(DevelopmentError::InvalidInput(_))
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

    #[test]
    fn project_tree_reports_ignore_and_symlink_semantics() {
        let root = fixture();
        fs::create_dir_all(root.join("target/debug")).unwrap();
        fs::write(root.join("target/debug/ignored"), "ignored").unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink(root.join("src/main.rs"), root.join("linked.rs")).unwrap();
        let project = ProjectWorkspace::open(&root).unwrap();
        let tree = project.list_files_result().unwrap();
        assert_eq!(tree.limit, MAX_FILE_ENTRIES);
        assert!(tree.ignored_directories.contains(&"target".to_string()));
        assert!(
            !tree
                .entries
                .iter()
                .any(|entry| entry.path.contains("ignored"))
        );
        #[cfg(unix)]
        assert_eq!(tree.skipped_symlinks, 1);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn editor_comments_replay_and_proposals_require_approval() {
        let root = fixture();
        let mut project = ProjectWorkspace::open(&root).unwrap();
        project.open_buffer("src/main.rs", Actor::local()).unwrap();
        let comment = project
            .add_editor_comment(
                "src/main.rs",
                1,
                1,
                "Prefer an explicit return here".into(),
                Actor::local(),
            )
            .unwrap();
        assert_eq!(project.editor_comments(None).len(), 1);
        assert_eq!(project.editor_proposals().len(), 0);
        let proposal = project
            .propose_editor_change(
                "src/main.rs",
                "fn main() {}\n".into(),
                "fn main() { println!(\"ok\"); }\n".into(),
                "Add observable output".into(),
                Actor::local(),
            )
            .unwrap();
        assert_eq!(
            project.buffer("src/main.rs").unwrap().content,
            "fn main() {}\n"
        );
        project
            .accept_editor_proposal(&proposal.id, Actor::local())
            .unwrap();
        assert_eq!(
            project.buffer("src/main.rs").unwrap().content,
            "fn main() { println!(\"ok\"); }\n"
        );
        let resolved = project
            .resolve_editor_comment(&comment.id, Actor::local())
            .unwrap();
        let revision = project.revision();
        assert_eq!(
            project
                .resolve_editor_comment(&comment.id, Actor::local())
                .unwrap(),
            resolved
        );
        assert_eq!(project.revision(), revision);
        drop(project);

        let reopened = ProjectWorkspace::open(&root).unwrap();
        let comments = reopened.editor_comments(Some("src/main.rs"));
        assert_eq!(comments.len(), 1);
        assert_eq!(
            comments[0].state,
            crate::development::EditorCommentState::Resolved
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn editor_proposals_become_stale_on_buffer_conflict() {
        let root = fixture();
        let mut project = ProjectWorkspace::open(&root).unwrap();
        project.open_buffer("src/main.rs", Actor::local()).unwrap();
        let proposal = project
            .propose_editor_change(
                "src/main.rs",
                "fn main() {}\n".into(),
                "fn main() { println!(\"proposal\"); }\n".into(),
                "Add output".into(),
                Actor::local(),
            )
            .unwrap();
        project
            .edit_buffer(
                "src/main.rs",
                "fn main() { println!(\"human\"); }\n".into(),
                Actor::local(),
            )
            .unwrap();
        assert!(matches!(
            project.accept_editor_proposal(&proposal.id, Actor::local()),
            Err(DevelopmentError::Conflict(_))
        ));
        assert_eq!(
            project.editor_proposals()[0].state,
            crate::development::EditorProposalState::Stale
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn editor_checkpoints_restore_unsaved_buffers_and_survive_reopen() {
        let root = fixture();
        let mut project = ProjectWorkspace::open(&root).unwrap();
        project
            .edit_buffer("src/main.rs", "one\n".into(), Actor::local())
            .unwrap();
        let checkpoint = project
            .create_editor_checkpoint("before experiment".into(), Actor::local())
            .unwrap();
        project
            .edit_buffer("src/main.rs", "two\n".into(), Actor::local())
            .unwrap();
        project
            .restore_editor_checkpoint(&checkpoint.id, Actor::local())
            .unwrap();
        assert_eq!(project.buffer("src/main.rs").unwrap().content, "one\n");
        drop(project);

        let reopened = ProjectWorkspace::open(&root).unwrap();
        assert_eq!(reopened.editor_checkpoints().len(), 1);
        let _ = fs::remove_dir_all(root);
    }
}
