use super::command;
use super::editor::{
    self as native, EditorEngine, EditorMode, GhostText, Motion, Operator, TextObject,
    apply_motion, compile_prove_it, complete_mention, evidence_card, expand_mentions,
    inferred_app_path, join_app_url, line_hunks, local_fim, multi_delete, multi_insert,
    next_edit_after_accept, pair_apply_caret, pair_apply_step, parse_inlay_hints, same_word_ranges,
    split_ghost_word, textobject_from_key, textobject_selection,
};
use super::parse::IncrementalSyntax;
use crate::browser::{BrowserService, BrowserStartConfig};
use crate::development::TextSelection;
use crate::{ExperimentComparison, SharedDevelopmentWorkspace};
use glass_browser::browser::policy::BrowserPolicy;
use glass_browser::browser::session::VerificationPredicate;
use glass_browser::browser::{
    NATIVE_DIALOG_TEXT_LIMIT_BYTES, NativeDialogResolution, NativePendingDialog, WorkflowRecorder,
};
use glass_browser::browser_workspace::{
    BrowserConnectionPhase, BrowserWorkspaceAction, BrowserWorkspaceAdapterKind,
    BrowserWorkspaceController, BrowserWorkspaceEntity, BrowserWorkspaceIntent,
    BrowserWorkspaceLayout, BrowserWorkspaceTarget,
};
use glass_browser::cli::args::TuiLayout;
use glass_browser::terminal_graphics::AnsiCanvas;
use glass_browser::tui::live_view::AnsiPane;
use serde::{Deserialize, Serialize};
use std::borrow::Cow;
use std::cell::RefCell;
use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_BROWSER_TOOL: AtomicU64 = AtomicU64::new(1);
static NEXT_CHAT_MESSAGE: AtomicU64 = AtomicU64::new(1);
const MAX_CODE_PROMPT_CONTEXT_BYTES: usize = 64 * 1024;
const MAX_CODE_CONTEXT_FILES: usize = 8;
const MAX_LSP_WORKSPACE_EDIT_FILES: usize = 16;
const MAX_LSP_WORKSPACE_EDIT_EDITS: usize = 16_384;
const MAX_NATIVE_GC_UI_OUTPUT_BYTES: usize = 64 * 1024;
pub(super) const BROWSER_RECOVERY_STATUS: &str =
    "Browser recovery available · current surface preserved";

fn relative_workspace_path(root: &Path, path: &Path) -> Option<PathBuf> {
    #[cfg(windows)]
    {
        let mut path_components = path.components();
        for root_component in root.components() {
            let path_component = path_components.next()?;
            if !root_component
                .as_os_str()
                .to_string_lossy()
                .eq_ignore_ascii_case(&path_component.as_os_str().to_string_lossy())
            {
                return None;
            }
        }
        Some(path_components.collect())
    }
    #[cfg(not(windows))]
    {
        path.strip_prefix(root).ok().map(Path::to_path_buf)
    }
}

fn bounded_native_gc_ui_output(text: &str) -> String {
    if text.len() <= MAX_NATIVE_GC_UI_OUTPUT_BYTES {
        return text.to_string();
    }
    let mut end = MAX_NATIVE_GC_UI_OUTPUT_BYTES;
    while !text.is_char_boundary(end) {
        end = end.saturating_sub(1);
    }
    format!(
        "{}\n\n[Core UI output truncated at {} bytes]",
        &text[..end],
        MAX_NATIVE_GC_UI_OUTPUT_BYTES
    )
}

fn apply_lsp_text_edits(original: &str, edits: &[serde_json::Value]) -> Result<String, String> {
    if edits.len() > MAX_LSP_WORKSPACE_EDIT_EDITS {
        return Err("LSP formatter returned too many edits".into());
    }
    let mut resolved = Vec::with_capacity(edits.len());
    for edit in edits {
        let range = edit
            .get("range")
            .ok_or_else(|| "LSP TextEdit omitted its range".to_string())?;
        let start = lsp_position_byte_offset(original, &range["start"])?;
        let end = lsp_position_byte_offset(original, &range["end"])?;
        if start > end {
            return Err("LSP TextEdit range ends before it starts".into());
        }
        let new_text = edit
            .get("newText")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| "LSP TextEdit omitted newText".to_string())?;
        resolved.push((start, end, new_text.to_string()));
    }
    resolved.sort_by(|left, right| right.0.cmp(&left.0).then_with(|| right.1.cmp(&left.1)));
    for pair in resolved.windows(2) {
        if pair[1].1 > pair[0].0 || pair[1].0 == pair[0].0 {
            return Err("LSP formatter returned overlapping edits".into());
        }
    }
    let mut proposed = original.to_string();
    for (start, end, new_text) in resolved {
        proposed.replace_range(start..end, &new_text);
    }
    Ok(proposed)
}

fn lsp_workspace_edit_entries(
    value: &serde_json::Value,
) -> Result<Vec<(String, Vec<serde_json::Value>)>, String> {
    let edit = value.get("result").unwrap_or(value);
    if edit.is_null() {
        return Ok(Vec::new());
    }
    let changes = edit.get("changes").and_then(serde_json::Value::as_object);
    let document_changes = edit
        .get("documentChanges")
        .and_then(serde_json::Value::as_array);
    if changes.is_some() && document_changes.is_some() {
        return Err("LSP workspace edit ambiguously contains changes and documentChanges".into());
    }
    let mut entries = Vec::new();
    if let Some(changes) = changes {
        entries.extend(
            changes
                .iter()
                .map(|(uri, edits)| {
                    Ok((
                        uri.clone(),
                        edits.as_array().cloned().ok_or_else(|| {
                            "LSP workspace edit changes must be TextEdit arrays".to_string()
                        })?,
                    ))
                })
                .collect::<Result<Vec<_>, String>>()?,
        );
    } else if let Some(document_changes) = document_changes {
        for change in document_changes {
            if change.get("kind").is_some() {
                return Err("LSP file create, rename, and delete operations need explicit Code review support".into());
            }
            let text_document = change
                .get("textDocument")
                .ok_or_else(|| "LSP document change omitted textDocument".to_string())?;
            let uri = text_document
                .get("uri")
                .and_then(serde_json::Value::as_str)
                .ok_or_else(|| "LSP document change omitted its URI".to_string())?;
            let edits = change
                .get("edits")
                .and_then(serde_json::Value::as_array)
                .cloned()
                .ok_or_else(|| "LSP document change omitted its TextEdit array".to_string())?;
            entries.push((uri.to_string(), edits));
        }
    } else {
        return Err("LSP result did not contain a workspace edit".into());
    }
    if entries.len() > MAX_LSP_WORKSPACE_EDIT_FILES {
        return Err(format!(
            "LSP workspace edit exceeds the {MAX_LSP_WORKSPACE_EDIT_FILES} file preview limit"
        ));
    }
    let mut total_edits = 0usize;
    for (_, edits) in &entries {
        total_edits = total_edits.saturating_add(edits.len());
        if total_edits > MAX_LSP_WORKSPACE_EDIT_EDITS {
            return Err(format!(
                "LSP workspace edit exceeds the {MAX_LSP_WORKSPACE_EDIT_EDITS} edit limit"
            ));
        }
    }
    Ok(entries)
}

fn lsp_position_byte_offset(text: &str, position: &serde_json::Value) -> Result<usize, String> {
    let line = position
        .get("line")
        .and_then(serde_json::Value::as_u64)
        .ok_or_else(|| "LSP position has no numeric line".to_string())?;
    let character = position
        .get("character")
        .and_then(serde_json::Value::as_u64)
        .ok_or_else(|| "LSP position has no numeric character".to_string())?;
    let line = usize::try_from(line).map_err(|_| "LSP line exceeds platform limits".to_string())?;
    let character = usize::try_from(character)
        .map_err(|_| "LSP character exceeds platform limits".to_string())?;
    let mut line_start = 0usize;
    for _ in 0..line {
        let newline = text[line_start..]
            .find('\n')
            .ok_or_else(|| "LSP position line is outside the document".to_string())?;
        line_start += newline + 1;
    }
    let line_end = text[line_start..]
        .find('\n')
        .map(|offset| line_start + offset)
        .unwrap_or(text.len());
    let line_text = &text[line_start..line_end];
    let mut utf16_offset = 0usize;
    for (byte_offset, character_at) in line_text.char_indices() {
        if utf16_offset == character {
            return Ok(line_start + byte_offset);
        }
        utf16_offset += character_at.len_utf16();
        if utf16_offset > character {
            return Err("LSP position splits a UTF-16 surrogate pair".into());
        }
    }
    if utf16_offset == character {
        Ok(line_end)
    } else {
        Err("LSP position character is outside the line".into())
    }
}

pub struct PendingConfirmation {
    pub call: crate::development::ToolCall,
    pub context: crate::tools::DevelopmentToolContext,
    pub summary: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NativeGcUiPanel {
    pub owner: crate::AgentId,
    pub command: String,
    pub scope: String,
    pub title: String,
    pub kind: String,
    pub content: String,
    pub query: String,
    pub scroll: u16,
    pub searching: bool,
    pub pending: bool,
    requested_after_sequence: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodeContextAttachment {
    pub path: String,
    pub selection: Option<crate::development::TextSelection>,
    content: String,
    dirty: bool,
}

#[derive(Debug, Clone)]
struct LspEditBaseline {
    path: String,
    content: String,
}

impl NativeGcUiPanel {
    pub fn visible_lines(&self) -> Vec<String> {
        let query = self.query.trim().to_lowercase();
        let title_matches = query.is_empty() || self.title.to_lowercase().contains(&query);
        let mut lines = Vec::new();
        if !self.pending && title_matches && !self.title.is_empty() {
            lines.push(self.title.clone());
        }
        lines.extend(
            self.content
                .lines()
                .filter(|line| query.is_empty() || line.to_lowercase().contains(&query))
                .map(str::to_owned),
        );
        lines
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum SelectionPreviewTarget {
    GitDiff {
        path: Option<String>,
        staged: bool,
    },
    ProcessLogs {
        name: String,
    },
    DebugThreads {
        session: String,
    },
    DebugStack {
        session: String,
        thread_id: i64,
    },
    DebugScopes {
        session: String,
        frame_id: i64,
        frame_path: Option<String>,
        source_path: Option<String>,
    },
    DebugVariables {
        session: String,
        frame_id: i64,
        frame_path: Option<String>,
        source_path: Option<String>,
        scope_index: usize,
        scope_name: String,
        reference: i64,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct SelectionPreviewKey {
    surface: DevSurface,
    expected_debug_pane: Option<DebugPane>,
    target: SelectionPreviewTarget,
}

struct PendingSelectionPreview {
    key: SelectionPreviewKey,
    call: crate::development::ToolCall,
    context: crate::tools::DevelopmentToolContext,
    label: String,
}

#[derive(Debug, Clone)]
struct ActiveSelectionPreview {
    id: u64,
    key: SelectionPreviewKey,
    label: String,
}

impl SelectionPreviewKey {
    fn tool_name(&self) -> &'static str {
        match &self.target {
            SelectionPreviewTarget::GitDiff { .. } => "glass.git.diff",
            SelectionPreviewTarget::ProcessLogs { .. } => "glass.process.logs",
            SelectionPreviewTarget::DebugThreads { .. } => "glass.debug.threads",
            SelectionPreviewTarget::DebugStack { .. } => "glass.debug.stack",
            SelectionPreviewTarget::DebugScopes { .. } => "glass.debug.scopes",
            SelectionPreviewTarget::DebugVariables { .. } => "glass.debug.variables",
        }
    }

    fn publishes_status_on(&self, surface: DevSurface) -> bool {
        match &self.target {
            SelectionPreviewTarget::DebugScopes { .. }
            | SelectionPreviewTarget::DebugVariables { .. } => {
                surface == DevSurface::Debug && self.surface == DevSurface::Debug
            }
            _ => true,
        }
    }
}

/// Focused TUI editor for one exact native JavaScript dialog identity.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BrowserDialogPrompt {
    /// Pending page dialog, including its target/frame owner and stable ID.
    pub pending: NativePendingDialog,
    /// Editable text for a `prompt()` call.
    pub prompt_input: String,
    /// Character index of the prompt cursor.
    pub prompt_cursor: usize,
    /// A stale/invalid response error shown without discarding the pending ID.
    pub error: Option<String>,
}

#[derive(Debug, Clone)]
pub struct TuiWorkflowRecording {
    pub name: String,
    pub recorder: WorkflowRecorder,
    step: usize,
}

impl TuiWorkflowRecording {
    fn next_id(&mut self) -> String {
        self.step += 1;
        format!("step-{}", self.step)
    }
}

enum PendingFim {
    Thread {
        path: String,
        offset: usize,
        revision: u64,
        rx: std::sync::mpsc::Receiver<Result<String, String>>,
    },
    Resident {
        path: String,
        offset: usize,
        revision: u64,
        agent_id: crate::AgentId,
        since: u64,
        request_id: String,
    },
}

impl PendingFim {
    fn path(&self) -> &str {
        match self {
            Self::Thread { path, .. } | Self::Resident { path, .. } => path,
        }
    }

    fn offset(&self) -> usize {
        match self {
            Self::Thread { offset, .. } | Self::Resident { offset, .. } => *offset,
        }
    }

    fn revision(&self) -> u64 {
        match self {
            Self::Thread { revision, .. } | Self::Resident { revision, .. } => *revision,
        }
    }
}

fn matching_resident_fim_response(
    events: &[crate::AgentEvent],
    agent_id: &crate::AgentId,
    request_id: &str,
) -> Option<Result<Option<String>, String>> {
    events.iter().rev().find_map(|event| {
        if &event.agent_id != agent_id
            || event
                .payload
                .get("type")
                .and_then(serde_json::Value::as_str)
                != Some("response")
            || event.payload.get("id").and_then(serde_json::Value::as_str) != Some(request_id)
            || event
                .payload
                .get("operation")
                .and_then(serde_json::Value::as_str)
                != Some("complete")
        {
            return None;
        }
        Some(crate::fim::parse_fim_text_checked(&event.payload))
    })
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum AttentionItemKey {
    EditorProposal {
        proposal_id: String,
        path: String,
    },
    FollowUp {
        thread_id: String,
        entry_id: String,
    },
    SendAttempt {
        thread_id: String,
        attempt_id: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AttentionItem {
    pub key: AttentionItemKey,
    pub title: String,
    pub detail: String,
    pub owner_agent_id: Option<crate::AgentId>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChatMessageState {
    Sending,
    Sent,
    Failed,
}

fn is_default_composer_agent(status: crate::AgentStatus) -> bool {
    matches!(
        status,
        crate::AgentStatus::Starting
            | crate::AgentStatus::Idle
            | crate::AgentStatus::Working
            | crate::AgentStatus::Waiting
    )
}

fn is_agent_lifecycle_notice(status: &str) -> bool {
    status.contains("Glass Agent is thinking")
        || status.contains("Glass Core is thinking")
        || status == "Glass Agent waiting for approval"
        || status == "Glass Agent waiting"
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct WorkspacePlan {
    pub id: String,
    pub goal: String,
    pub body: String,
    pub accepted: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionPickerItem {
    pub path: String,
    pub label: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ThreadPickerItem {
    pub agent_id: crate::AgentId,
    pub title: String,
    pub status: String,
}

struct TaskThreadLabel {
    assigned_agent: Option<crate::AgentId>,
    id: String,
    title: String,
}

fn project_thread_picker_title(
    role: &str,
    agent_id: &crate::AgentId,
    tasks: &[TaskThreadLabel],
) -> String {
    tasks
        .iter()
        .find(|task| task.assigned_agent.as_ref() == Some(agent_id))
        .map_or_else(
            || role.to_owned(),
            |task| format!("{role} · {} · {}", task.id, task.title),
        )
}

/// Build conversation labels from the task-to-agent associations recorded by
/// the scheduler, including retained associations for terminal tasks.
/// Agents without a matching assigned task retain their role-only label.
pub(super) fn project_thread_picker_items(
    agents: &[crate::AgentSnapshot],
    tasks: &[crate::TaskSnapshot],
) -> Vec<ThreadPickerItem> {
    let task_labels = tasks
        .iter()
        .map(|task| TaskThreadLabel {
            assigned_agent: task.assigned_agent.clone(),
            id: task.id.as_str().to_owned(),
            title: task.title.clone(),
        })
        .collect::<Vec<_>>();
    agents
        .iter()
        .map(|agent| ThreadPickerItem {
            agent_id: agent.id.clone(),
            title: project_thread_picker_title(&agent.role, &agent.id, &task_labels),
            status: agent.status.label().to_owned(),
        })
        .collect()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcessRow {
    pub name: String,
    pub command: String,
    pub pid: Option<u32>,
    pub health: crate::development::ProcessHealth,
    pub url: Option<String>,
}

impl ProcessRow {
    pub fn from_snapshot(item: &crate::development::ProcessSnapshot) -> Self {
        Self {
            name: item.name.clone(),
            command: item.command.clone(),
            pid: item.pid,
            health: item.health.clone(),
            url: item.detected_urls.first().cloned(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DebugSessionRow {
    pub name: String,
    pub state: crate::debugger::DebugSessionState,
    pub pid: u32,
    pub breakpoints: usize,
    pub watches: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DebugScopeRow {
    pub name: String,
    pub variables_reference: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DebugVariableRow {
    pub name: String,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DebugThreadRow {
    pub id: i64,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DebugFrameRow {
    pub id: i64,
    pub name: String,
    pub path: Option<String>,
    pub line: Option<u64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DebugPane {
    #[default]
    Sessions,
    Threads,
    Frames,
}

impl DebugPane {
    pub fn label(self) -> &'static str {
        match self {
            Self::Sessions => "sessions",
            Self::Threads => "threads",
            Self::Frames => "frames",
        }
    }

    fn next(self) -> Self {
        match self {
            Self::Sessions => Self::Threads,
            Self::Threads => Self::Frames,
            Self::Frames => Self::Sessions,
        }
    }

    fn previous(self) -> Self {
        match self {
            Self::Sessions => Self::Frames,
            Self::Threads => Self::Sessions,
            Self::Frames => Self::Threads,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingChatMessage {
    /// Process-local owner captured when the send was submitted.
    pub agent_id: Option<crate::AgentId>,
    pub message_id: String,
    pub text: String,
    pub state: ChatMessageState,
    pub job_id: Option<u64>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct ComposerDraft {
    text: String,
    cursor: usize,
}

#[cfg(unix)]
type PendingComposerDraftLoad = (
    u64,
    std::sync::mpsc::Receiver<Result<Option<ComposerDraft>, String>>,
);

#[cfg(unix)]
enum ComposerDraftWrite {
    Save {
        key: String,
        target: ComposerDraftTarget,
        draft: ComposerDraft,
        generation: u64,
    },
    Load {
        workspace: crate::workspace::SharedDevelopmentWorkspace,
        agent_id: crate::AgentId,
        reply: std::sync::mpsc::Sender<Result<Option<ComposerDraft>, String>>,
    },
    Flush(std::sync::mpsc::Sender<Result<(), String>>),
    DiscardOwners {
        workspace: crate::workspace::SharedDevelopmentWorkspace,
        owners: Vec<(crate::AgentId, u64)>,
    },
    #[cfg(test)]
    DiscardThread {
        store: crate::thread_store::ThreadStore,
        thread_id: String,
    },
    Shutdown,
    #[cfg(test)]
    Delay(std::time::Duration),
}

#[cfg(unix)]
type PendingFollowUpQueueLoad = (
    u64,
    std::sync::mpsc::Receiver<Result<FollowUpQueueSnapshot, String>>,
);

#[cfg(unix)]
type AttentionQueueSnapshots = Vec<(crate::AgentId, FollowUpQueueSnapshot)>;

#[cfg(unix)]
type AttentionQueueLoadResult = Result<AttentionQueueSnapshots, String>;

#[cfg(unix)]
type AttentionQueueLoadReceiver = std::sync::mpsc::Receiver<AttentionQueueLoadResult>;

#[cfg(unix)]
type PendingAttentionQueueLoad = (
    std::collections::BTreeMap<crate::AgentId, u64>,
    AttentionQueueLoadReceiver,
);

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct FollowUpQueueSnapshot {
    thread_id: String,
    current_run_id: String,
    entries: Vec<crate::thread_store::FollowUpQueueRecord>,
    send_attempts: Vec<crate::thread_store::SendAttemptRecord>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct SendAttemptRetryTarget {
    pub(super) agent_id: crate::AgentId,
    pub(super) thread_id: String,
    pub(super) attempt_id: String,
    pub(super) status: crate::thread_store::SendAttemptStatus,
    client_message_id: String,
    mode: crate::thread_store::SendAttemptMode,
    text: String,
    pub(super) preview: String,
}

fn retryable_send_attempt_status(attempt: &crate::thread_store::SendAttemptRecord) -> bool {
    use crate::thread_store::SendAttemptStatus::{Failed, Uncertain};
    attempt.status == Uncertain || (attempt.status == Failed && attempt.engine_request_id.is_some())
}

fn send_attempt_attention_label(status: crate::thread_store::SendAttemptStatus) -> &'static str {
    match status {
        crate::thread_store::SendAttemptStatus::Failed => "Core turn failed",
        _ => "Message delivery uncertain",
    }
}

#[cfg(unix)]
enum FollowUpQueueCommand {
    Save {
        workspace: crate::workspace::SharedDevelopmentWorkspace,
        agent_id: crate::AgentId,
        entries: Vec<crate::thread_store::FollowUpQueueRecord>,
        generation: u64,
    },
    Load {
        workspace: crate::workspace::SharedDevelopmentWorkspace,
        agent_id: crate::AgentId,
        reply: std::sync::mpsc::Sender<Result<FollowUpQueueSnapshot, String>>,
    },
    LoadInbox {
        workspace: crate::workspace::SharedDevelopmentWorkspace,
        reply: std::sync::mpsc::Sender<AttentionQueueLoadResult>,
    },
    Shutdown,
}

#[cfg(unix)]
fn load_all_follow_up_queues(
    workspace: crate::workspace::SharedDevelopmentWorkspace,
) -> AttentionQueueLoadResult {
    workspace
        .lock()
        .and_then(|mut workspace| {
            let agents = workspace.agents().list()?;
            let mut queues = Vec::new();
            for agent in agents {
                let (store, thread_id, current_run_id) =
                    workspace.agents().follow_up_queue_target(&agent.id)?;
                let mut entries = store.follow_up_queue(&thread_id)?;
                let send_attempts = store.send_attempts(&thread_id)?;
                for entry in &mut entries {
                    if entry.state == crate::thread_store::FollowUpQueueState::Dispatching {
                        entry.state = crate::thread_store::FollowUpQueueState::NeedsReview;
                    }
                }
                queues.push((
                    agent.id,
                    FollowUpQueueSnapshot {
                        thread_id,
                        current_run_id,
                        entries,
                        send_attempts,
                    },
                ));
            }
            Ok(queues)
        })
        .map_err(|error| error.to_string())
}

#[cfg(unix)]
enum FollowUpQueueEvent {
    Saved(crate::AgentId, u64),
    SaveFailed(crate::AgentId, u64, String),
}

#[cfg(unix)]
fn clear_pending_queue_dispatch_if_matches(
    pending: &mut Option<(crate::AgentId, String, u64)>,
    agent_id: &crate::AgentId,
    generation: u64,
) -> bool {
    if pending
        .as_ref()
        .is_some_and(|(owner, _, pending_generation)| {
            owner == agent_id && *pending_generation == generation
        })
    {
        *pending = None;
        true
    } else {
        false
    }
}

#[cfg(unix)]
struct FollowUpQueueWriter {
    tx: std::sync::mpsc::Sender<FollowUpQueueCommand>,
    events: std::sync::mpsc::Receiver<FollowUpQueueEvent>,
    worker: Option<std::thread::JoinHandle<()>>,
}

#[cfg(unix)]
impl FollowUpQueueWriter {
    fn spawn() -> Self {
        use std::collections::BTreeMap;
        use std::sync::mpsc::{self, RecvTimeoutError, TryRecvError};
        use std::time::Duration;

        type PendingQueueWrites = BTreeMap<
            crate::AgentId,
            (
                crate::workspace::SharedDevelopmentWorkspace,
                Vec<crate::thread_store::FollowUpQueueRecord>,
                u64,
            ),
        >;

        let (tx, rx) = mpsc::channel();
        let (events_tx, events) = mpsc::channel();
        let worker = std::thread::spawn(move || {
            let mut pending = PendingQueueWrites::new();
            let flush = |pending: &mut PendingQueueWrites,
                         events: &mpsc::Sender<FollowUpQueueEvent>| {
                let owners = pending.keys().cloned().collect::<Vec<_>>();
                for owner in owners {
                    let Some((workspace, entries, generation)) = pending.remove(&owner) else {
                        continue;
                    };
                    let result = workspace
                        .lock()
                        .and_then(|mut workspace| {
                            let (store, thread_id, _) =
                                workspace.agents().follow_up_queue_target(&owner)?;
                            store.set_follow_up_queue(&thread_id, entries)
                        })
                        .map_err(|error| error.to_string());
                    let event = match result {
                        Ok(()) => FollowUpQueueEvent::Saved(owner, generation),
                        Err(error) => FollowUpQueueEvent::SaveFailed(owner, generation, error),
                    };
                    let _ = events.send(event);
                }
            };
            loop {
                match rx.recv_timeout(Duration::from_millis(100)) {
                    Ok(FollowUpQueueCommand::Save {
                        workspace,
                        agent_id,
                        entries,
                        generation,
                    }) => {
                        pending.insert(agent_id, (workspace, entries, generation));
                        loop {
                            match rx.try_recv() {
                                Ok(FollowUpQueueCommand::Save {
                                    workspace,
                                    agent_id,
                                    entries,
                                    generation,
                                }) => {
                                    pending.insert(agent_id, (workspace, entries, generation));
                                }
                                Ok(FollowUpQueueCommand::Load {
                                    workspace,
                                    agent_id,
                                    reply,
                                }) => {
                                    flush(&mut pending, &events_tx);
                                    let result = workspace
                                        .lock()
                                        .and_then(|mut workspace| {
                                            let (store, thread_id, run_id) = workspace
                                                .agents()
                                                .follow_up_queue_target(&agent_id)?;
                                            let mut entries = store.follow_up_queue(&thread_id)?;
                                            let send_attempts = store.send_attempts(&thread_id)?;
                                            for entry in &mut entries {
                                                if entry.state
                                                    == crate::thread_store::FollowUpQueueState::Dispatching
                                                {
                                                    entry.state = crate::thread_store::FollowUpQueueState::NeedsReview;
                                                }
                                            }
                                            Ok(FollowUpQueueSnapshot {
                                                thread_id,
                                                current_run_id: run_id,
                                                entries,
                                                send_attempts,
                                            })
                                        })
                                        .map_err(|error| error.to_string());
                                    let _ = reply.send(result);
                                }
                                Ok(FollowUpQueueCommand::LoadInbox { workspace, reply }) => {
                                    flush(&mut pending, &events_tx);
                                    let _ = reply.send(load_all_follow_up_queues(workspace));
                                }
                                Ok(FollowUpQueueCommand::Shutdown) => {
                                    flush(&mut pending, &events_tx);
                                    return;
                                }
                                Err(TryRecvError::Empty | TryRecvError::Disconnected) => break,
                            }
                        }
                    }
                    Ok(FollowUpQueueCommand::Load {
                        workspace,
                        agent_id,
                        reply,
                    }) => {
                        flush(&mut pending, &events_tx);
                        let result = workspace
                            .lock()
                            .and_then(|mut workspace| {
                                let (store, thread_id, run_id) =
                                    workspace.agents().follow_up_queue_target(&agent_id)?;
                                let mut entries = store.follow_up_queue(&thread_id)?;
                                let send_attempts = store.send_attempts(&thread_id)?;
                                for entry in &mut entries {
                                    if entry.state
                                        == crate::thread_store::FollowUpQueueState::Dispatching
                                    {
                                        entry.state =
                                            crate::thread_store::FollowUpQueueState::NeedsReview;
                                    }
                                }
                                Ok(FollowUpQueueSnapshot {
                                    thread_id,
                                    current_run_id: run_id,
                                    entries,
                                    send_attempts,
                                })
                            })
                            .map_err(|error| error.to_string());
                        let _ = reply.send(result);
                    }
                    Ok(FollowUpQueueCommand::LoadInbox { workspace, reply }) => {
                        flush(&mut pending, &events_tx);
                        let _ = reply.send(load_all_follow_up_queues(workspace));
                    }
                    Ok(FollowUpQueueCommand::Shutdown) => {
                        flush(&mut pending, &events_tx);
                        return;
                    }
                    Err(RecvTimeoutError::Timeout) => flush(&mut pending, &events_tx),
                    Err(RecvTimeoutError::Disconnected) => {
                        flush(&mut pending, &events_tx);
                        return;
                    }
                }
            }
        });
        Self {
            tx,
            events,
            worker: Some(worker),
        }
    }

    fn load(
        &self,
        workspace: crate::workspace::SharedDevelopmentWorkspace,
        agent_id: crate::AgentId,
    ) -> Result<std::sync::mpsc::Receiver<Result<FollowUpQueueSnapshot, String>>, String> {
        let (reply, result) = std::sync::mpsc::channel();
        self.tx
            .send(FollowUpQueueCommand::Load {
                workspace,
                agent_id,
                reply,
            })
            .map_err(|error| error.to_string())?;
        Ok(result)
    }

    fn load_inbox(
        &self,
        workspace: crate::workspace::SharedDevelopmentWorkspace,
    ) -> Result<AttentionQueueLoadReceiver, String> {
        let (reply, result) = std::sync::mpsc::channel();
        self.tx
            .send(FollowUpQueueCommand::LoadInbox { workspace, reply })
            .map_err(|error| error.to_string())?;
        Ok(result)
    }

    fn save(
        &self,
        workspace: crate::workspace::SharedDevelopmentWorkspace,
        agent_id: crate::AgentId,
        entries: Vec<crate::thread_store::FollowUpQueueRecord>,
        generation: u64,
    ) -> Result<(), String> {
        self.tx
            .send(FollowUpQueueCommand::Save {
                workspace,
                agent_id,
                entries,
                generation,
            })
            .map_err(|error| error.to_string())
    }

    fn try_event(&self) -> Option<FollowUpQueueEvent> {
        self.events.try_recv().ok()
    }
}

#[cfg(unix)]
impl Drop for FollowUpQueueWriter {
    fn drop(&mut self) {
        let _ = self.tx.send(FollowUpQueueCommand::Shutdown);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

#[cfg(unix)]
enum ComposerDraftTarget {
    Agent {
        workspace: crate::workspace::SharedDevelopmentWorkspace,
        agent_id: crate::AgentId,
    },
    #[cfg(test)]
    Thread {
        store: crate::thread_store::ThreadStore,
        thread_id: String,
    },
}

#[cfg(unix)]
enum ComposerDraftWriterEvent {
    SaveFailed(crate::AgentId, u64, String),
    SaveSucceeded(crate::AgentId, u64),
    DiscardComplete(Result<(), String>),
}

#[cfg(unix)]
struct ComposerDraftWriter {
    tx: std::sync::mpsc::Sender<ComposerDraftWrite>,
    events: std::sync::mpsc::Receiver<ComposerDraftWriterEvent>,
    worker: Option<std::thread::JoinHandle<()>>,
}

#[cfg(unix)]
impl ComposerDraftWriter {
    fn spawn() -> Self {
        use std::collections::BTreeMap;
        use std::sync::mpsc::{self, RecvTimeoutError, TryRecvError};
        use std::time::Duration;

        let (tx, rx) = mpsc::channel();
        let (events_tx, events) = mpsc::channel();
        let worker = std::thread::spawn(move || {
            let mut pending = BTreeMap::<String, (ComposerDraftTarget, ComposerDraft, u64)>::new();
            let mut unsaved_agents = std::collections::BTreeMap::new();
            let mut wait_for_change_after_failure = false;
            loop {
                let received = if wait_for_change_after_failure {
                    rx.recv().map_err(|_| RecvTimeoutError::Disconnected)
                } else {
                    rx.recv_timeout(Duration::from_millis(150))
                };
                match received {
                    Ok(ComposerDraftWrite::Save {
                        key,
                        target,
                        draft,
                        generation,
                    }) => {
                        wait_for_change_after_failure = false;
                        match &target {
                            ComposerDraftTarget::Agent { agent_id, .. } => {
                                unsaved_agents.insert(agent_id.clone(), generation);
                            }
                            #[cfg(test)]
                            ComposerDraftTarget::Thread { .. } => {}
                        }
                        pending.insert(key, (target, draft, generation));
                        loop {
                            match rx.try_recv() {
                                Ok(ComposerDraftWrite::Save {
                                    key,
                                    target,
                                    draft,
                                    generation,
                                }) => {
                                    wait_for_change_after_failure = false;
                                    match &target {
                                        ComposerDraftTarget::Agent { agent_id, .. } => {
                                            unsaved_agents.insert(agent_id.clone(), generation);
                                        }
                                        #[cfg(test)]
                                        ComposerDraftTarget::Thread { .. } => {}
                                    }
                                    pending.insert(key, (target, draft, generation));
                                }
                                Ok(ComposerDraftWrite::Load {
                                    workspace,
                                    agent_id,
                                    reply,
                                }) => {
                                    let flushed = Self::flush_pending(
                                        &mut pending,
                                        &mut unsaved_agents,
                                        &events_tx,
                                    );
                                    let result = flushed.and_then(|()| {
                                        let (store, thread_id) = workspace
                                            .lock()
                                            .and_then(|mut workspace| {
                                                workspace.agents().composer_draft_target(&agent_id)
                                            })
                                            .map_err(|error| error.to_string())?;
                                        store
                                            .composer_draft(&thread_id)
                                            .map(|draft| {
                                                draft.map(|draft| ComposerDraft {
                                                    text: draft.text,
                                                    cursor: draft.cursor,
                                                })
                                            })
                                            .map_err(|error| error.to_string())
                                    });
                                    let _ = reply.send(result);
                                    wait_for_change_after_failure = false;
                                }
                                Ok(ComposerDraftWrite::Flush(reply)) => {
                                    let result = Self::flush_pending(
                                        &mut pending,
                                        &mut unsaved_agents,
                                        &events_tx,
                                    );
                                    wait_for_change_after_failure = result.is_err();
                                    let _ = reply.send(result);
                                }
                                Ok(ComposerDraftWrite::Shutdown) => {
                                    return;
                                }
                                Ok(ComposerDraftWrite::DiscardOwners { workspace, owners }) => {
                                    let result = Self::discard_owners(
                                        &mut pending,
                                        &mut unsaved_agents,
                                        &workspace,
                                        &owners,
                                    );
                                    let _ = events_tx
                                        .send(ComposerDraftWriterEvent::DiscardComplete(result));
                                }
                                #[cfg(test)]
                                Ok(ComposerDraftWrite::DiscardThread { store, thread_id }) => {
                                    pending.remove(&format!("thread:{thread_id}"));
                                    let result = store
                                        .clear_composer_draft(&thread_id)
                                        .map_err(|error| error.to_string());
                                    let _ = events_tx
                                        .send(ComposerDraftWriterEvent::DiscardComplete(result));
                                }
                                #[cfg(test)]
                                Ok(ComposerDraftWrite::Delay(duration)) => {
                                    std::thread::sleep(duration);
                                }
                                Err(TryRecvError::Empty | TryRecvError::Disconnected) => break,
                            }
                        }
                    }
                    Ok(ComposerDraftWrite::Load {
                        workspace,
                        agent_id,
                        reply,
                    }) => {
                        let flushed =
                            Self::flush_pending(&mut pending, &mut unsaved_agents, &events_tx);
                        let result = flushed.and_then(|()| {
                            let (store, thread_id) = workspace
                                .lock()
                                .and_then(|mut workspace| {
                                    workspace.agents().composer_draft_target(&agent_id)
                                })
                                .map_err(|error| error.to_string())?;
                            store
                                .composer_draft(&thread_id)
                                .map(|draft| {
                                    draft.map(|draft| ComposerDraft {
                                        text: draft.text,
                                        cursor: draft.cursor,
                                    })
                                })
                                .map_err(|error| error.to_string())
                        });
                        let _ = reply.send(result);
                        wait_for_change_after_failure = false;
                    }
                    Ok(ComposerDraftWrite::Flush(reply)) => {
                        let result =
                            Self::flush_pending(&mut pending, &mut unsaved_agents, &events_tx);
                        wait_for_change_after_failure = result.is_err();
                        let _ = reply.send(result);
                    }
                    Ok(ComposerDraftWrite::Shutdown) => {
                        return;
                    }
                    Ok(ComposerDraftWrite::DiscardOwners { workspace, owners }) => {
                        let result = Self::discard_owners(
                            &mut pending,
                            &mut unsaved_agents,
                            &workspace,
                            &owners,
                        );
                        let _ = events_tx.send(ComposerDraftWriterEvent::DiscardComplete(result));
                    }
                    #[cfg(test)]
                    Ok(ComposerDraftWrite::DiscardThread { store, thread_id }) => {
                        pending.remove(&format!("thread:{thread_id}"));
                        let result = store
                            .clear_composer_draft(&thread_id)
                            .map_err(|error| error.to_string());
                        let _ = events_tx.send(ComposerDraftWriterEvent::DiscardComplete(result));
                    }
                    #[cfg(test)]
                    Ok(ComposerDraftWrite::Delay(duration)) => {
                        std::thread::sleep(duration);
                    }
                    Err(RecvTimeoutError::Timeout) => {
                        let result =
                            Self::flush_pending(&mut pending, &mut unsaved_agents, &events_tx);
                        wait_for_change_after_failure = result.is_err();
                    }
                    Err(RecvTimeoutError::Disconnected) => {
                        let _ = Self::flush_pending(&mut pending, &mut unsaved_agents, &events_tx);
                        return;
                    }
                }
            }
        });
        Self {
            tx,
            events,
            worker: Some(worker),
        }
    }

    #[cfg(test)]
    fn save(
        &self,
        store: crate::thread_store::ThreadStore,
        thread_id: String,
        draft: ComposerDraft,
    ) -> Result<(), String> {
        self.tx
            .send(ComposerDraftWrite::Save {
                key: format!("thread:{thread_id}"),
                target: ComposerDraftTarget::Thread { store, thread_id },
                draft,
                generation: 0,
            })
            .map_err(|error| error.to_string())
    }

    fn save_agent(
        &self,
        workspace: crate::workspace::SharedDevelopmentWorkspace,
        agent_id: crate::AgentId,
        draft: ComposerDraft,
        generation: u64,
    ) -> Result<(), String> {
        self.tx
            .send(ComposerDraftWrite::Save {
                key: format!("agent:{}", agent_id.as_str()),
                target: ComposerDraftTarget::Agent {
                    workspace,
                    agent_id,
                },
                draft,
                generation,
            })
            .map_err(|error| error.to_string())
    }

    fn load_agent(
        &self,
        workspace: crate::workspace::SharedDevelopmentWorkspace,
        agent_id: crate::AgentId,
    ) -> Result<std::sync::mpsc::Receiver<Result<Option<ComposerDraft>, String>>, String> {
        let (reply, result) = std::sync::mpsc::channel();
        self.tx
            .send(ComposerDraftWrite::Load {
                workspace,
                agent_id,
                reply,
            })
            .map_err(|error| error.to_string())?;
        Ok(result)
    }

    fn flush(&self) -> Result<(), (String, bool)> {
        let (tx, rx) = std::sync::mpsc::channel();
        self.tx
            .send(ComposerDraftWrite::Flush(tx))
            .map_err(|error| (error.to_string(), true))?;
        match rx.recv_timeout(std::time::Duration::from_secs(2)) {
            Ok(Ok(())) => Ok(()),
            Ok(Err(error)) => Err((error, false)),
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                Err(("timed out waiting for composer draft storage".into(), true))
            }
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                Err(("composer draft writer disconnected".into(), true))
            }
        }
    }

    fn try_event(&self) -> Option<ComposerDraftWriterEvent> {
        self.events.try_recv().ok()
    }

    fn enqueue_discard_owners(
        &self,
        workspace: crate::workspace::SharedDevelopmentWorkspace,
        owners: Vec<(crate::AgentId, u64)>,
    ) -> Result<(), String> {
        self.tx
            .send(ComposerDraftWrite::DiscardOwners { workspace, owners })
            .map_err(|error| error.to_string())
    }

    #[cfg(test)]
    fn block_for_test(&self, duration: std::time::Duration) -> Result<(), String> {
        self.tx
            .send(ComposerDraftWrite::Delay(duration))
            .map_err(|error| error.to_string())
    }

    #[cfg(test)]
    fn disconnected_for_test() -> Self {
        let (tx, rx) = std::sync::mpsc::channel();
        drop(rx);
        let (event_tx, events) = std::sync::mpsc::channel();
        drop(event_tx);
        Self {
            tx,
            events,
            worker: None,
        }
    }

    fn flush_pending(
        pending: &mut std::collections::BTreeMap<String, (ComposerDraftTarget, ComposerDraft, u64)>,
        unsaved_agents: &mut std::collections::BTreeMap<crate::AgentId, u64>,
        events: &std::sync::mpsc::Sender<ComposerDraftWriterEvent>,
    ) -> Result<(), String> {
        let mut failed = Vec::new();
        let thread_ids = pending.keys().cloned().collect::<Vec<_>>();
        for thread_id in thread_ids {
            let Some((target, draft, generation)) = pending.remove(&thread_id) else {
                continue;
            };
            let result = match &target {
                #[cfg(test)]
                ComposerDraftTarget::Thread { store, thread_id } => store
                    .set_composer_draft(
                        thread_id,
                        crate::thread_store::ComposerDraftRecord {
                            text: draft.text.clone(),
                            cursor: draft.cursor,
                        },
                    )
                    .map_err(|error| error.to_string()),
                ComposerDraftTarget::Agent {
                    workspace,
                    agent_id,
                } => workspace
                    .lock()
                    .and_then(|mut workspace| {
                        let (store, thread_id) =
                            workspace.agents().composer_draft_target(agent_id)?;
                        store.set_composer_draft(
                            &thread_id,
                            crate::thread_store::ComposerDraftRecord {
                                text: draft.text.clone(),
                                cursor: draft.cursor,
                            },
                        )
                    })
                    .map_err(|error| error.to_string()),
            };
            let agent_id = match &target {
                ComposerDraftTarget::Agent { agent_id, .. } => Some(agent_id.clone()),
                #[cfg(test)]
                ComposerDraftTarget::Thread { .. } => None,
            };
            match result {
                Ok(()) => {
                    if let Some(agent_id) = agent_id {
                        if unsaved_agents.get(&agent_id) == Some(&generation) {
                            unsaved_agents.remove(&agent_id);
                        }
                        let _ = events.send(ComposerDraftWriterEvent::SaveSucceeded(
                            agent_id, generation,
                        ));
                    }
                }
                Err(error) => {
                    if let Some(agent_id) = agent_id {
                        unsaved_agents.insert(agent_id.clone(), generation);
                        let _ = events.send(ComposerDraftWriterEvent::SaveFailed(
                            agent_id,
                            generation,
                            error.clone(),
                        ));
                    }
                    failed.push(error);
                    pending.insert(thread_id, (target, draft, generation));
                }
            }
        }
        failed.into_iter().next().map_or(Ok(()), Err)
    }

    fn discard_owners(
        pending: &mut std::collections::BTreeMap<String, (ComposerDraftTarget, ComposerDraft, u64)>,
        unsaved_agents: &mut std::collections::BTreeMap<crate::AgentId, u64>,
        workspace: &crate::workspace::SharedDevelopmentWorkspace,
        owners: &[(crate::AgentId, u64)],
    ) -> Result<(), String> {
        let mut failed = Vec::new();
        for (agent_id, generation) in owners {
            if unsaved_agents
                .get(agent_id)
                .is_some_and(|current| current > generation)
            {
                continue;
            }
            pending.remove(&format!("agent:{}", agent_id.as_str()));
            let result = workspace
                .lock()
                .and_then(|mut workspace| {
                    let (store, thread_id) = workspace.agents().composer_draft_target(agent_id)?;
                    store.clear_composer_draft(&thread_id)
                })
                .map_err(|error| error.to_string());
            match result {
                Ok(()) => {
                    if unsaved_agents
                        .get(agent_id)
                        .is_some_and(|current| current <= generation)
                    {
                        unsaved_agents.remove(agent_id);
                    }
                }
                Err(error) => failed.push(error),
            }
        }
        failed.into_iter().next().map_or(Ok(()), Err)
    }

    #[cfg(test)]
    fn discard_thread(
        &self,
        store: crate::thread_store::ThreadStore,
        thread_id: String,
    ) -> Result<(), String> {
        self.tx
            .send(ComposerDraftWrite::DiscardThread { store, thread_id })
            .map_err(|error| error.to_string())
    }
}

#[cfg(unix)]
impl Drop for ComposerDraftWriter {
    fn drop(&mut self) {
        let _ = self.tx.send(ComposerDraftWrite::Shutdown);
        // Registry fsync runs on this worker. Detach it during teardown so a
        // slow filesystem cannot trap the terminal after an explicit bounded
        // quit flush has already reported its outcome.
        drop(self.worker.take());
    }
}

/// In-TUI recovery choices offered when a browser start collides or fails.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BrowserRecoveryOffer {
    pub reason: String,
    pub port: u16,
    /// Native browser recovery must never direct a user to an external
    /// Chromium binary or DevTools port.
    pub native_backend: bool,
    /// A healthy Chrome CDP endpoint answered on the preferred port.
    pub compatible_endpoint: bool,
}

impl BrowserRecoveryOffer {
    pub fn from_error(error: &str, port: u16) -> Self {
        Self::from_error_for_backend(error, port, false)
    }

    pub fn from_error_for_backend(error: &str, port: u16, native_backend: bool) -> Self {
        let lower = error.to_ascii_lowercase();
        Self {
            reason: error.to_string(),
            port,
            native_backend,
            compatible_endpoint: ["attach", "devtools", "page target", "multiple page targets"]
                .iter()
                .any(|marker| lower.contains(marker)),
        }
    }

    pub fn actions(&self) -> &'static [(&'static str, &'static str)] {
        if self.native_backend {
            &[
                ("1", "restart the native browser session"),
                ("Esc", "dismiss"),
            ]
        } else if self.compatible_endpoint {
            &[
                ("1", "attach after checking the running browser"),
                ("2", "launch an isolated browser on an automatic free port"),
                ("3", "retry the preferred port"),
                ("Esc", "dismiss"),
            ]
        } else {
            &[
                ("1", "launch an isolated browser on an automatic free port"),
                ("2", "retry the preferred port"),
                ("Esc", "dismiss"),
            ]
        }
    }

    pub fn guidance(&self) -> &'static str {
        let lower = self.reason.to_ascii_lowercase();
        if self.native_backend {
            "The Glass native browser stopped responding. The agent and workspace remain available; dismiss this notice, then start a new native browser session from the composer."
        } else if self.compatible_endpoint {
            "A DevTools endpoint answered. Attach only when its page is the intended browser; otherwise launch an isolated session."
        } else if lower.contains("occup") || lower.contains("address") || lower.contains("port") {
            "The preferred endpoint is busy or unavailable. Glass keeps the project and agent alive while you retry or move to an automatic free port."
        } else {
            "The browser did not become usable. Check the selected browser backend, then retry; the workspace remains intact."
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum DevSurface {
    Agent,
    Code,
    App,
    Terminal,
    Tasks,
    Git,
    Debug,
    More,
}

impl DevSurface {
    pub const ALL: [Self; 8] = [
        Self::Agent,
        Self::Code,
        Self::App,
        Self::Terminal,
        Self::Tasks,
        Self::Git,
        Self::Debug,
        Self::More,
    ];

    pub const PRIMARY: [Self; 5] = [
        Self::Agent,
        Self::Code,
        Self::App,
        Self::Terminal,
        Self::Git,
    ];

    pub const PHONE: [Self; 5] = Self::PRIMARY;

    pub fn label(self) -> &'static str {
        match self {
            Self::Agent => "Agent",
            Self::Code => "Code",
            Self::App => "Browser",
            Self::Terminal => "Terminal",
            Self::Tasks => "Tasks",
            Self::Git => "Git",
            Self::Debug => "Debug",
            Self::More => "More",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResponsiveClass {
    Desktop,
    Compact,
    Phone,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditorExitPrompt {
    Clean,
    Unsaved,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProductMode {
    Build,
    Agent,
    RunApp,
    Debug,
}

impl ProductMode {
    pub fn label(self) -> &'static str {
        match self {
            Self::Build => "build",
            Self::Agent => "agent",
            Self::RunApp => "run",
            Self::Debug => "debug",
        }
    }
}

const PALETTE_INPUT_LIMIT: usize = 8 * 1024;
const COMPOSER_INPUT_LIMIT: usize = 16 * 1024;

pub struct DevTuiState {
    pub workspace: SharedDevelopmentWorkspace,
    /// Cloned directly so modal status/control never needs the workspace lock.
    pub(crate) browser_service: BrowserService,
    /// Defaults captured from global CLI browser options for later resident starts.
    pub(crate) browser_start_defaults: BrowserStartConfig,
    pub surface: DevSurface,
    pub layout: TuiLayout,
    pub quit_confirmation: bool,
    pub quit: bool,
    pub command_mode: bool,
    pub command_input: String,
    pub command_cursor: usize,
    pub command_history: Vec<String>,
    pub command_history_index: Option<usize>,
    pub palette_error: Option<String>,
    pub palette_scroll: u16,
    /// Index in the filtered, arrow-selectable palette action list.
    pub palette_selection: usize,
    pub menu_open: bool,
    pub menu_selection: usize,
    pub help_open: bool,
    pub help_scroll: u16,
    /// Explicit Core-owned human UI output, kept out of the chat transcript.
    pub native_gc_ui_panel: Option<NativeGcUiPanel>,
    native_gc_ui_last_sequence: std::collections::BTreeMap<crate::AgentId, u64>,
    pub composer_mode: bool,
    pub pending_plan: Option<WorkspacePlan>,
    pub composer_input: String,
    pub composer_cursor: usize,
    pub composer_steer: bool,
    /// Attach the explicitly viewed Code buffer to sends from the Code dock.
    pub composer_code_context_enabled: bool,
    /// Additional open buffers explicitly attached to the next Code prompt.
    pub code_context_attachments: Vec<CodeContextAttachment>,
    pending_lsp_format_baselines: VecDeque<LspEditBaseline>,
    pending_lsp_preview_results: VecDeque<(String, serde_json::Value)>,
    pending_editor_acceptances: VecDeque<String>,
    pub last_app_comment: Option<String>,
    pub session_todos: crate::SessionTodoList,
    pub composer_history: Vec<String>,
    pub composer_history_index: Option<usize>,
    pub composer_history_draft: String,
    pub file_picker_open: bool,
    pub file_picker_query: String,
    pub file_picker_cursor: usize,
    pub file_picker_selection: usize,
    pub transcript_selection: usize,
    pub transcript_expanded: bool,
    /// Shows provider reasoning in isolation after an explicit Ctrl-T toggle.
    pub reasoning_view_open: bool,
    pub session_picker_open: bool,
    pub session_picker_items: Vec<SessionPickerItem>,
    pub session_picker_selection: usize,
    pub conversation_items: Vec<super::projection::ConversationEntry>,
    pub(super) core_transcript: RefCell<super::core_conversation::InlineTranscript>,
    pub agent_conversations:
        std::collections::BTreeMap<crate::AgentId, super::snapshot::AgentConversation>,
    pub agent_model: String,
    pub agent_thinking: String,
    pub agent_session_name: String,
    pub agent_token_summary: String,
    pub git_branch: String,
    pub git_dirty: bool,
    pub git_ahead: u64,
    pub git_behind: u64,
    pub git_conflicts: Vec<String>,
    pub github_review_requested: bool,
    pub pending_open_file: Option<String>,
    pub pending_chat_messages: Vec<PendingChatMessage>,
    composer_retry_message_id: Option<String>,
    pub agent_send_job: Option<u64>,
    pub selected_agent: Option<crate::AgentId>,
    agent_status_by_id: std::collections::BTreeMap<crate::AgentId, crate::AgentStatus>,
    pub thread_picker_open: bool,
    pub thread_picker_items: Vec<ThreadPickerItem>,
    pub thread_picker_selection: usize,
    thread_picker_previous_status: Option<String>,
    pub attention_inbox_open: bool,
    pub attention_selection: usize,
    pub(crate) attention_items: Vec<AttentionItem>,
    send_attempt_retry_candidate: Option<SendAttemptRetryTarget>,
    pub(super) send_attempt_retry_confirmation: Option<SendAttemptRetryTarget>,
    pub follow_up_queue_open: bool,
    pub follow_up_queue_selection: usize,
    pub follow_up_queue_edit_text: String,
    pub follow_up_queue_edit_cursor: usize,
    pub(crate) follow_up_queue_edit_mode: Option<crate::thread_store::FollowUpQueueMode>,
    pub follow_up_queue_edit_index: Option<usize>,
    follow_up_queue_edit_target_run_id: Option<String>,
    pub follow_up_queue_retry_confirmation: Option<String>,
    follow_up_queue_snapshots: std::collections::BTreeMap<crate::AgentId, FollowUpQueueSnapshot>,
    follow_up_queue_generations: std::collections::BTreeMap<crate::AgentId, u64>,
    follow_up_queue_pending_dispatch: Option<(crate::AgentId, String, u64)>,
    follow_up_queue_dispatch_jobs: std::collections::BTreeMap<u64, (crate::AgentId, String)>,
    #[cfg(unix)]
    follow_up_queue_writer: Option<FollowUpQueueWriter>,
    #[cfg(unix)]
    pending_follow_up_queue_loads:
        std::collections::BTreeMap<crate::AgentId, PendingFollowUpQueueLoad>,
    #[cfg(unix)]
    attention_queue_load: Option<PendingAttentionQueueLoad>,
    #[cfg(unix)]
    attention_queue_refresh_at: Option<std::time::Instant>,
    /// Drafts keyed by process-local resident agent identity; `None` is the
    /// composer before an agent thread is selected.
    thread_drafts: std::collections::BTreeMap<Option<crate::AgentId>, ComposerDraft>,
    #[cfg(unix)]
    persisted_composer_draft: Option<(crate::AgentId, ComposerDraft)>,
    #[cfg(unix)]
    composer_draft_writer: Option<ComposerDraftWriter>,
    #[cfg(unix)]
    composer_draft_save_failed: bool,
    #[cfg(unix)]
    composer_draft_failure: Option<String>,
    #[cfg(unix)]
    composer_draft_discard_pending: bool,
    #[cfg(unix)]
    composer_draft_discard_targets: std::collections::BTreeMap<crate::AgentId, u64>,
    #[cfg(unix)]
    composer_draft_unsaved_agents: std::collections::BTreeMap<crate::AgentId, u64>,
    #[cfg(unix)]
    composer_draft_failed_agents: std::collections::BTreeMap<crate::AgentId, u64>,
    #[cfg(unix)]
    composer_draft_edit_generations: std::collections::BTreeMap<crate::AgentId, u64>,
    #[cfg(unix)]
    pending_composer_draft_loads:
        std::collections::BTreeMap<crate::AgentId, PendingComposerDraftLoad>,
    pub pending_confirmation: Option<PendingConfirmation>,
    /// URL retained while the TUI launches a detached browser before navigation.
    pub pending_browser_navigation: Option<String>,
    /// Native page dialog currently presented above all App/surface content.
    pub browser_dialog: Option<BrowserDialogPrompt>,
    pub pending_page_entity: Option<String>,
    pub process_urls: Vec<String>,
    selected_editor_proposal_id: Option<String>,
    pub editor_exit_prompt: Option<EditorExitPrompt>,
    pub queued_tool_request: Option<(
        crate::development::ToolCall,
        crate::tools::DevelopmentToolContext,
    )>,
    pub running_tool_job: Option<u64>,
    active_selection_preview: Option<ActiveSelectionPreview>,
    pending_selection_preview: Option<PendingSelectionPreview>,
    pub surface_scroll: std::collections::BTreeMap<DevSurface, usize>,
    pub terminal_width: u16,
    pub terminal_height: u16,
    pub status: String,
    pub snapshot_root: String,
    pub snapshot_project_revision: u64,
    pub snapshot_generation: u64,
    pub snapshot_skills_count: usize,
    pub snapshot_tools_count: usize,
    /// Whether the asynchronous workspace projection has delivered at least
    /// one complete snapshot. The interactive TUI must not present empty
    /// collections as authoritative before this becomes true.
    pub snapshot_ready: bool,
    /// Wall-clock cost of the last background refresh pass.
    pub refresh_latency_ms: u64,
    /// Highest agent event sequence folded into the live conversation.
    pub conversation_cursor: u64,
    /// Index of the editor buffer focused by the Code surface.
    pub editor_buffer_index: usize,
    pending_editor_input: std::collections::VecDeque<PendingEditorInput>,
    /// Last editor buffer projection, available while a background task owns
    /// the workspace mutex. Writes still require the live workspace.
    pub(crate) focused_editor_buffer: Option<crate::development::EditorBuffer>,
    pub focused_editor_path: String,
    /// Cached editor content used while the shared workspace is busy.
    pub focused_editor_content: String,
    pub focused_editor_dirty: bool,
    pub focused_editor_line: u32,
    pub focused_editor_column: u32,
    pub focused_editor_selection: Option<crate::development::TextSelection>,
    pub editor_comments: Vec<crate::development::EditorComment>,
    pub editor_proposals: Vec<crate::development::EditorProposal>,
    pub editor_checkpoints: Vec<crate::development::EditorCheckpoint>,
    pub editor_diagnostics: Vec<crate::development::LanguageDiagnostic>,
    pub editor_inlays: Vec<(u32, String)>,
    pub last_crew_wake: Option<String>,
    pub agents: String,
    pub agent_readiness: String,
    pub harnesses: String,
    /// Set by an in-TUI login action; the event loop temporarily hands the
    /// terminal to the selected runtime's interactive login flow.
    pub agent_login_requested: bool,
    /// Set when the user explicitly hands the terminal to GC's native command UI.
    pub agent_command_ui_requested: bool,
    /// Dynamically discovered commands from the selected Glass Core session.
    pub gc_command_menu_open: bool,
    pub gc_command_query: String,
    pub gc_command_items: Vec<serde_json::Value>,
    pub gc_command_selected: usize,
    /// Model selection is a Core-backed subview of the command palette.
    pub gc_model_menu_open: bool,
    pub gc_model_items: Vec<serde_json::Value>,
    pub gc_model_provider: Option<String>,
    pub gc_model_current: Option<String>,
    pub gc_model_error: Option<String>,
    pub gc_session_settings: Option<super::snapshot::CoreSessionSettings>,
    pub gc_settings_view: bool,
    pub gc_command_loading: bool,
    pub gc_command_discovery_requested: bool,
    gc_command_discovery_pending: bool,
    /// Tool job IDs whose command-list responses belong only to the slash
    /// palette. Late results remain cache updates after the menu closes.
    gc_command_discovery_jobs: std::collections::HashSet<u64>,
    gc_model_discovery_jobs: std::collections::HashSet<u64>,
    gc_settings_jobs: std::collections::HashSet<u64>,
    composer_gc_command_completion: bool,
    composer_glass_action_completion: Option<String>,
    /// Set by a palette action; the event loop hands the terminal to the
    /// selected external harness and resumes Glass after it exits.
    pub harness_launch_requested: Option<String>,
    pub agent_conversation: String,
    pub tasks: String,
    pub editor: String,
    pub files: Vec<String>,
    pub selected_file: usize,
    pub code_edit_mode: bool,
    pub editor_soft_wrap: bool,
    pub editor_scroll_line: usize,
    pub editor_scroll_column: usize,
    pub editor_engine: EditorEngine,
    pub syntax: IncrementalSyntax,
    pub last_verify: Option<String>,
    pub last_proof_ok: Option<bool>,
    pub pending_verify: Option<serde_json::Value>,
    pub factory_split: bool,
    pub lsp: String,
    pub processes: String,
    pub process_entries: Vec<ProcessRow>,
    pub selected_process: usize,
    selected_process_identity: Option<String>,
    pub process_logs: String,
    pub process_logs_requested: bool,
    pub git: String,
    pub git_entries: Vec<crate::git::GitStatusEntry>,
    pub github: crate::github::GitHubStatus,
    pub github_review: String,
    pub selected_git_file: usize,
    selected_git_identity: Option<String>,
    pub git_diff_path: Option<String>,
    pub git_diff: String,
    pub git_diff_open: bool,
    pub git_diff_requested: bool,
    pub tests: String,
    pub kernels: String,
    pub debugger: String,
    pub debug_sessions: Vec<DebugSessionRow>,
    pub selected_debug_session: usize,
    selected_debug_identity: Option<String>,
    pub debug_threads: Vec<DebugThreadRow>,
    pub selected_debug_thread: usize,
    pub debug_frames: Vec<DebugFrameRow>,
    pub selected_debug_frame: usize,
    pub debug_pane: DebugPane,
    pub debug_threads_requested: bool,
    pub debug_stack_requested: bool,
    pub debug_scopes: Vec<DebugScopeRow>,
    pub selected_debug_scope: usize,
    pub debug_variables: Vec<DebugVariableRow>,
    pub debug_variables_notice: Option<String>,
    pub debug_scopes_requested: bool,
    pub debug_variables_requested: bool,
    pub selected_todo: usize,
    pub selected_more: usize,
    pub more_result: String,
    pub replay: String,
    pub browser: String,
    pub browser_detail: String,
    pub browser_workspace: BrowserWorkspaceController,
    pub browser_recovery: Option<BrowserRecoveryOffer>,
    pub browser_target_picker: bool,
    pub browser_target_picker_requested: bool,
    pub browser_target_query: String,
    pub browser_target_selection: usize,
    pub browser_visual_live: bool,
    pending_browser_visual_request: Option<BrowserVisualRequest>,
    pub browser_observe_pending: bool,
    browser_observe_retry_after: Option<std::time::Instant>,
    browser_observe_retry_count: u8,
    pub browser_ansi: AnsiCanvas,
    pub browser_pane: Option<AnsiPane>,
    pub workflow: String,
    pub workspace_status: String,
    pub experiment_comparison: Option<ExperimentComparison>,
    pub experiments: String,
    pub cockpit_url: String,
    pub private_cockpit: Option<crate::development::LocalCockpit>,
    pub workflow_recording: Option<TuiWorkflowRecording>,
    pending_fim: Option<PendingFim>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct BrowserVisualRequest {
    pub(crate) live: bool,
    pub(crate) status: Option<String>,
    pub(crate) failure: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum PendingEditorInput {
    Key(crossterm::event::KeyCode, crossterm::event::KeyModifiers),
    Paste(String),
}

impl DevTuiState {
    /// Lock the shared workspace, mapping errors to strings for the palette.
    pub fn ws(&self) -> Result<std::sync::MutexGuard<'_, crate::DevelopmentWorkspace>, String> {
        self.workspace.try_lock().map_err(|error| error.to_string())
    }

    /// Lock the shared workspace for mutation, mapping errors to strings.
    pub fn ws_mut(&self) -> Result<std::sync::MutexGuard<'_, crate::DevelopmentWorkspace>, String> {
        self.workspace.try_lock().map_err(|error| error.to_string())
    }

    /// Lock the workspace for mutation inside UI callbacks; reports lock
    /// failures through the status line instead of `?`.
    fn locked<F, T>(&mut self, operation: F) -> Option<T>
    where
        F: FnOnce(&mut crate::DevelopmentWorkspace) -> T,
    {
        match self.workspace.try_lock() {
            Ok(mut workspace) => Some(operation(&mut workspace)),
            Err(error) => {
                self.status = format!("Workspace lock failed: {error}");
                None
            }
        }
    }

    pub fn product_mode(&self) -> ProductMode {
        match self.surface {
            DevSurface::Agent => ProductMode::Agent,
            DevSurface::App => ProductMode::RunApp,
            DevSurface::Debug => ProductMode::Debug,
            _ => ProductMode::Build,
        }
    }

    pub(crate) fn set_browser_start_defaults(&mut self, defaults: BrowserStartConfig) {
        self.browser_start_defaults = defaults;
    }

    #[cfg(test)]
    pub(crate) fn open(
        root: impl AsRef<Path>,
        layout: TuiLayout,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let browser_policy = BrowserPolicy::development(root.as_ref())?;
        Self::open_internal(root, layout, true, browser_policy)
    }

    /// Construct the interactive TUI without doing a full synchronous
    /// projection pass. The snapshot worker fills resident projections after
    /// the first frame, so a large repository can show the cockpit immediately.
    #[cfg(test)]
    pub(crate) fn open_for_tui(
        root: impl AsRef<Path>,
        layout: TuiLayout,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let browser_policy = BrowserPolicy::development(root.as_ref())?;
        Self::open_internal(root, layout, false, browser_policy)
    }

    /// Construct the TUI with the complete browser authorization policy.
    pub(crate) fn open_for_tui_with_browser_policy(
        root: impl AsRef<Path>,
        layout: TuiLayout,
        browser_policy: BrowserPolicy,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        Self::open_internal(root, layout, false, browser_policy)
    }

    fn open_internal(
        root: impl AsRef<Path>,
        layout: TuiLayout,
        initial_refresh: bool,
        browser_policy: BrowserPolicy,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let workspace = SharedDevelopmentWorkspace::open_with_browser_policy(root, browser_policy)?;
        let locked = workspace.lock()?;
        let browser_service = locked.browser().clone();
        let snapshot_root = locked.root().display().to_string();
        let snapshot_project_revision = locked.project().revision();
        let snapshot_generation = locked.generation();
        let snapshot_skills_count = locked.customization().skills().count();
        let snapshot_tools_count = locked.customization().config().tools.len();
        drop(locked);
        let agent_readiness = crate::resident_harness::resident_readiness()
            .map(|readiness| format_resident_readiness(&readiness))
            .unwrap_or_else(|error| format!("Agent unavailable · {error}"));
        let initial_status = if !initial_refresh {
            "Loading workspace snapshot · first refresh pending · ? help".to_string()
        } else if agent_readiness.starts_with("✓ Ready") {
            "Core available · type to chat · / commands · Ctrl-P files · ? help".to_string()
        } else {
            "Core setup required · type / for Core commands · Ctrl-K for Glass actions".to_string()
        };
        let mut state = Self {
            workspace,
            browser_service,
            browser_start_defaults: BrowserStartConfig {
                modal_dialogs: true,
                ..BrowserStartConfig::default()
            },
            surface: DevSurface::Agent,
            layout,
            quit: false,
            quit_confirmation: false,
            command_mode: false,
            command_input: String::new(),
            command_cursor: 0,
            command_history: Vec::new(),
            command_history_index: None,
            palette_error: None,
            palette_scroll: 0,
            palette_selection: 0,
            menu_open: false,
            menu_selection: 0,
            help_open: false,
            help_scroll: 0,
            native_gc_ui_panel: None,
            native_gc_ui_last_sequence: std::collections::BTreeMap::new(),
            // Agent is the landing workspace and its composer is immediately
            // ready for text, including digits and slash commands.
            composer_mode: true,
            pending_plan: None,
            composer_input: String::new(),
            composer_cursor: 0,
            composer_steer: false,
            composer_code_context_enabled: false,
            code_context_attachments: Vec::new(),
            pending_lsp_format_baselines: VecDeque::new(),
            pending_lsp_preview_results: VecDeque::new(),
            pending_editor_acceptances: VecDeque::new(),
            last_app_comment: None,
            session_todos: crate::SessionTodoList::default(),
            composer_history: Vec::new(),
            composer_history_index: None,
            composer_history_draft: String::new(),
            file_picker_open: false,
            file_picker_query: String::new(),
            file_picker_cursor: 0,
            file_picker_selection: 0,
            transcript_selection: 0,
            transcript_expanded: false,
            reasoning_view_open: false,
            core_transcript: RefCell::new(super::core_conversation::InlineTranscript::default()),
            session_picker_open: false,
            session_picker_items: Vec::new(),
            session_picker_selection: 0,
            conversation_items: Vec::new(),
            agent_conversations: std::collections::BTreeMap::new(),
            agent_model: String::new(),
            agent_thinking: String::new(),
            agent_session_name: String::new(),
            agent_token_summary: String::new(),
            git_branch: String::new(),
            git_dirty: false,
            git_ahead: 0,
            git_behind: 0,
            git_conflicts: Vec::new(),
            github_review_requested: false,
            pending_open_file: None,
            pending_chat_messages: Vec::new(),
            composer_retry_message_id: None,
            agent_send_job: None,
            selected_agent: None,
            agent_status_by_id: std::collections::BTreeMap::new(),
            thread_picker_open: false,
            thread_picker_items: Vec::new(),
            thread_picker_selection: 0,
            thread_picker_previous_status: None,
            attention_inbox_open: false,
            attention_selection: 0,
            attention_items: Vec::new(),
            send_attempt_retry_candidate: None,
            send_attempt_retry_confirmation: None,
            follow_up_queue_open: false,
            follow_up_queue_selection: 0,
            follow_up_queue_edit_text: String::new(),
            follow_up_queue_edit_cursor: 0,
            follow_up_queue_edit_mode: None,
            follow_up_queue_edit_index: None,
            follow_up_queue_edit_target_run_id: None,
            follow_up_queue_retry_confirmation: None,
            follow_up_queue_snapshots: std::collections::BTreeMap::new(),
            follow_up_queue_generations: std::collections::BTreeMap::new(),
            follow_up_queue_pending_dispatch: None,
            follow_up_queue_dispatch_jobs: std::collections::BTreeMap::new(),
            #[cfg(unix)]
            follow_up_queue_writer: None,
            #[cfg(unix)]
            pending_follow_up_queue_loads: std::collections::BTreeMap::new(),
            #[cfg(unix)]
            attention_queue_load: None,
            #[cfg(unix)]
            attention_queue_refresh_at: None,
            thread_drafts: std::collections::BTreeMap::new(),
            #[cfg(unix)]
            persisted_composer_draft: None,
            #[cfg(unix)]
            composer_draft_writer: None,
            #[cfg(unix)]
            composer_draft_save_failed: false,
            #[cfg(unix)]
            composer_draft_failure: None,
            #[cfg(unix)]
            composer_draft_discard_pending: false,
            #[cfg(unix)]
            composer_draft_discard_targets: std::collections::BTreeMap::new(),
            #[cfg(unix)]
            composer_draft_unsaved_agents: std::collections::BTreeMap::new(),
            #[cfg(unix)]
            composer_draft_failed_agents: std::collections::BTreeMap::new(),
            #[cfg(unix)]
            composer_draft_edit_generations: std::collections::BTreeMap::new(),
            #[cfg(unix)]
            pending_composer_draft_loads: std::collections::BTreeMap::new(),
            pending_confirmation: None,
            editor_exit_prompt: None,
            selected_editor_proposal_id: None,
            pending_browser_navigation: None,
            browser_dialog: None,
            pending_page_entity: None,
            process_urls: Vec::new(),
            queued_tool_request: None,
            running_tool_job: None,
            active_selection_preview: None,
            pending_selection_preview: None,
            surface_scroll: std::collections::BTreeMap::new(),
            terminal_width: 80,
            terminal_height: 24,
            snapshot_root,
            snapshot_project_revision,
            snapshot_generation,
            snapshot_skills_count,
            snapshot_tools_count,
            snapshot_ready: initial_refresh,
            refresh_latency_ms: 0,
            conversation_cursor: 0,
            editor_buffer_index: 0,
            pending_editor_input: std::collections::VecDeque::new(),
            focused_editor_buffer: None,
            focused_editor_path: String::new(),
            focused_editor_content: String::new(),
            focused_editor_dirty: false,
            focused_editor_line: 0,
            focused_editor_column: 0,
            focused_editor_selection: None,
            editor_comments: Vec::new(),
            editor_proposals: Vec::new(),
            editor_checkpoints: Vec::new(),
            editor_diagnostics: Vec::new(),
            editor_inlays: Vec::new(),
            last_crew_wake: None,
            status: initial_status,
            agents: String::new(),
            agent_readiness,
            harnesses: crate::harness::summary(),
            agent_login_requested: false,
            agent_command_ui_requested: false,
            gc_command_menu_open: false,
            gc_command_query: String::new(),
            gc_command_items: Vec::new(),
            gc_command_selected: 0,
            gc_model_menu_open: false,
            gc_model_items: Vec::new(),
            gc_model_provider: None,
            gc_model_current: None,
            gc_model_error: None,
            gc_session_settings: None,
            gc_settings_view: false,
            gc_command_loading: false,
            gc_command_discovery_requested: false,
            gc_command_discovery_pending: false,
            gc_command_discovery_jobs: std::collections::HashSet::new(),
            gc_model_discovery_jobs: std::collections::HashSet::new(),
            gc_settings_jobs: std::collections::HashSet::new(),
            composer_gc_command_completion: false,
            composer_glass_action_completion: None,
            harness_launch_requested: None,
            agent_conversation:
                "No conversation yet. Press Enter or start typing to compose a message.".into(),
            tasks: String::new(),
            editor: String::new(),
            files: Vec::new(),
            selected_file: 0,
            code_edit_mode: false,
            editor_soft_wrap: false,
            editor_scroll_line: 0,
            editor_scroll_column: 0,
            editor_engine: EditorEngine {
                mode: EditorMode::Insert,
                ..EditorEngine::default()
            },
            syntax: IncrementalSyntax::new(),
            last_verify: None,
            last_proof_ok: None,
            pending_verify: None,
            // Keep the Agent workspace focused on the conversation. An open
            // editor buffer can outlive a surface change; showing the split
            // merely because the composer opened makes that old file look
            // like an implicit context attachment.
            factory_split: false,
            lsp: String::new(),
            processes: String::new(),
            process_entries: Vec::new(),
            selected_process: 0,
            selected_process_identity: None,
            process_logs: String::new(),
            process_logs_requested: false,
            git: String::new(),
            git_entries: Vec::new(),
            github: crate::github::GitHubStatus::default(),
            github_review: "No GitHub review yet".into(),
            selected_git_file: 0,
            selected_git_identity: None,
            git_diff_path: None,
            git_diff: String::new(),
            git_diff_open: false,
            git_diff_requested: false,
            tests: String::new(),
            kernels: String::new(),
            debugger: String::new(),
            debug_sessions: Vec::new(),
            selected_debug_session: 0,
            selected_debug_identity: None,
            debug_threads: Vec::new(),
            selected_debug_thread: 0,
            debug_frames: Vec::new(),
            selected_debug_frame: 0,
            debug_pane: DebugPane::Sessions,
            debug_threads_requested: false,
            debug_stack_requested: false,
            debug_scopes: Vec::new(),
            selected_debug_scope: 0,
            debug_variables: Vec::new(),
            debug_variables_notice: None,
            debug_scopes_requested: false,
            debug_variables_requested: false,
            selected_todo: 0,
            selected_more: 0,
            more_result: String::new(),
            replay: String::new(),
            browser: String::new(),
            browser_detail: "No browser observation yet".into(),
            browser_workspace: BrowserWorkspaceController::for_adapter(
                match layout {
                    TuiLayout::Mobile => BrowserWorkspaceLayout::Phone,
                    TuiLayout::Compact => BrowserWorkspaceLayout::Compact,
                    TuiLayout::Auto | TuiLayout::Desktop => BrowserWorkspaceLayout::Desktop,
                },
                BrowserWorkspaceAdapterKind::EmbeddedDevelopment,
            ),
            browser_recovery: None,
            browser_target_picker: false,
            browser_target_picker_requested: false,
            browser_target_query: String::new(),
            browser_target_selection: 0,
            browser_visual_live: false,
            pending_browser_visual_request: None,
            browser_observe_pending: false,
            browser_observe_retry_after: None,
            browser_observe_retry_count: 0,
            browser_ansi: AnsiCanvas::default(),
            browser_pane: None,
            workflow: "No workflow evidence yet".into(),
            workspace_status: String::new(),
            experiment_comparison: None,
            experiments: "No experiments. :experiment create ID BRANCH [PORT]".into(),
            cockpit_url: String::new(),
            private_cockpit: None,
            workflow_recording: None,
            pending_fim: None,
        };
        if initial_refresh {
            state.refresh();
        }
        Ok(state)
    }

    pub fn responsive_class(&self, width: u16, height: u16) -> ResponsiveClass {
        match self.layout {
            TuiLayout::Desktop => ResponsiveClass::Desktop,
            TuiLayout::Compact => ResponsiveClass::Compact,
            TuiLayout::Mobile => ResponsiveClass::Phone,
            TuiLayout::Auto if width < 72 || height < 22 => ResponsiveClass::Phone,
            TuiLayout::Auto if width < 118 || height < 32 => ResponsiveClass::Compact,
            TuiLayout::Auto => ResponsiveClass::Desktop,
        }
    }

    pub fn request_quit(&mut self) {
        if self.quit {
            return;
        }
        #[cfg(unix)]
        {
            if !self.persist_composer_draft() || self.composer_draft_save_failed {
                for (owner, generation) in self
                    .composer_draft_unsaved_agents
                    .iter()
                    .chain(self.composer_draft_failed_agents.iter())
                {
                    self.composer_draft_discard_targets
                        .entry(owner.clone())
                        .and_modify(|target| *target = (*target).max(*generation))
                        .or_insert(*generation);
                }
                let save_error = self
                    .composer_draft_failure
                    .clone()
                    .unwrap_or_else(|| "composer draft is not saved".into());
                self.quit_confirmation = true;
                self.status = format!(
                    "Could not save composer draft: {save_error} · Enter discards the unsaved draft and quits · Esc keeps editing"
                );
                return;
            }
            if !self.flush_composer_drafts() {
                for (owner, generation) in self
                    .composer_draft_unsaved_agents
                    .iter()
                    .chain(self.composer_draft_failed_agents.iter())
                {
                    self.composer_draft_discard_targets
                        .entry(owner.clone())
                        .and_modify(|target| *target = (*target).max(*generation))
                        .or_insert(*generation);
                }
                let save_error = self
                    .composer_draft_failure
                    .clone()
                    .unwrap_or_else(|| "persistence did not complete".into());
                self.quit_confirmation = true;
                self.status = format!(
                    "Could not save composer draft: {save_error} · Enter discards the unsaved draft and quits · Esc keeps editing"
                );
                return;
            }
        }
        if self.editor_exit_prompt.is_none() {
            let dirty_buffer = match self.workspace.try_lock() {
                Ok(workspace) => workspace
                    .project()
                    .buffers()
                    .enumerate()
                    .find(|(_, buffer)| buffer.dirty)
                    .map(|(index, buffer)| (index, buffer.path.clone())),
                Err(_) if self.focused_editor_dirty => {
                    Some((self.editor_buffer_index, self.focused_editor_path.clone()))
                }
                Err(_) => {
                    self.status =
                        "Workspace busy · retry quit after editor buffers can be checked".into();
                    return;
                }
            }
            .or_else(|| {
                self.focused_editor_dirty
                    .then(|| (self.editor_buffer_index, self.focused_editor_path.clone()))
            });
            if let Some((index, path)) = dirty_buffer {
                self.editor_buffer_index = index;
                if !path.is_empty() {
                    self.focused_editor_path = path;
                }
                self.surface = DevSurface::Code;
                self.code_edit_mode = true;
                self.refresh_editor_projection();
                self.request_editor_exit();
                return;
            }
        }
        self.quit_confirmation = true;
        self.status = if self.editor_exit_prompt == Some(EditorExitPrompt::Unsaved) {
            "Unsaved changes · confirm quit to discard or Esc to keep editing".into()
        } else {
            "Quit confirmation · Enter exits · Esc stays".into()
        };
    }

    pub fn confirm_quit(&mut self) {
        if let Err(error) = self.dismiss_native_browser_dialog_for_quit() {
            self.quit_confirmation = false;
            if let Some(dialog) = self.browser_dialog.as_mut() {
                dialog.error = Some(error.clone());
            }
            self.status = format!("Could not resolve the page dialog before quitting · {error}");
            return;
        }
        #[cfg(unix)]
        if self.composer_draft_discard_pending {
            self.quit_confirmation = true;
            self.status = "Discarding composer draft · waiting for storage".into();
            return;
        }
        #[cfg(unix)]
        if self.composer_draft_save_failed || !self.composer_draft_discard_targets.is_empty() {
            let owners = self
                .composer_draft_discard_targets
                .iter()
                .map(|(owner, generation)| (owner.clone(), *generation))
                .collect::<Vec<_>>();
            let mut result = self.composer_draft_writer.as_ref().map_or_else(
                || Err("draft writer is unavailable".into()),
                |writer| writer.enqueue_discard_owners(self.workspace.clone(), owners.clone()),
            );
            if result.is_err() {
                let replacement = ComposerDraftWriter::spawn();
                result = replacement.enqueue_discard_owners(self.workspace.clone(), owners);
                self.composer_draft_writer = Some(replacement);
            }
            match result {
                Ok(()) => {
                    self.composer_draft_discard_pending = true;
                    self.quit_confirmation = true;
                    self.status = "Discarding composer draft · waiting for storage".into();
                }
                Err(error) => {
                    self.quit_confirmation = true;
                    self.composer_draft_failure = Some(error.clone());
                    self.status = format!(
                        "Could not queue composer draft discard: {error} · retry or Esc to keep editing"
                    );
                }
            }
            return;
        }
        self.quit_confirmation = false;
        self.quit = true;
        #[cfg(unix)]
        {
            self.status = "Closing Glass Dev".into();
        }
        #[cfg(not(unix))]
        {
            self.status = "Closing Glass Dev".into();
        }
    }

    pub fn cancel_quit(&mut self) {
        self.quit_confirmation = false;
        #[cfg(unix)]
        self.composer_draft_discard_targets.clear();
        #[cfg(unix)]
        if self.composer_draft_save_failed {
            self.status =
                "Composer draft remains unsaved · keep editing and retry persistence".into();
            return;
        }
        self.status = "Quit dismissed · workspace remains open".into();
    }

    pub fn quit_menu_index(&self) -> usize {
        self.surface_actions().len() + 1
    }

    /// Guided launchers for the current surface. `Ctrl-K` opens the command center.
    pub fn surface_actions(&self) -> &'static [command::SurfaceAction] {
        command::surface_actions_for(self.surface, crate::ResidentHarnessKind::default_kind())
    }

    /// Explain why a guided launcher cannot run with the currently visible
    /// state. The action menu uses this both for presentation and for the
    /// reducer guard, so a stale or incomplete snapshot cannot look runnable.
    pub fn surface_action_unavailable_reason(
        &self,
        action: command::SurfaceAction,
    ) -> Option<&'static str> {
        let snapshot_pending =
            || (!self.snapshot_ready).then_some("workspace snapshot is still loading");
        let browser = self.browser_workspace.state();
        match action.command {
            "Enter" | "i" if self.surface == DevSurface::Code => {
                if self.files.is_empty() && self.focused_editor_path.is_empty() {
                    Some("no project file is selected")
                } else {
                    None
                }
            }
            "editor replace-selection" | "editor comment-selection" => self
                .focused_editor_selection
                .as_ref()
                .filter(|selection| !selection.is_empty())
                .is_none()
                .then_some("no editor selection is active"),
            "Ctrl-S" => {
                if self.focused_editor_path.is_empty() {
                    Some("no editor buffer is open")
                } else if !self.focused_editor_dirty {
                    Some("the focused buffer has no unsaved changes")
                } else {
                    None
                }
            }
            "app source" => browser
                .selected()
                .is_none()
                .then_some("no browser entity is selected"),
            "browser observe" => (browser.connection != BrowserConnectionPhase::Connected)
                .then_some("browser is not connected · start browser first"),
            "browser type TARGET TEXT" => {
                if browser.connection != BrowserConnectionPhase::Connected {
                    Some("browser is not connected · start browser first")
                } else if browser.selected().is_none() {
                    Some("no browser entity is selected")
                } else if !browser.selected().is_some_and(|entity| entity.actionable) {
                    Some("the selected browser entity is not actionable")
                } else {
                    None
                }
            }
            "app open" => {
                if snapshot_pending().is_some() {
                    snapshot_pending()
                } else if self.process_urls.is_empty() {
                    Some("no detected app URL is available")
                } else {
                    None
                }
            }
            "process start dev" => {
                if let Some(reason) = snapshot_pending() {
                    Some(reason)
                } else if self
                    .ws()
                    .ok()
                    .and_then(|workspace| workspace.project().detection().dev_command.clone())
                    .is_none()
                {
                    Some("no project development command was detected")
                } else {
                    None
                }
            }
            "process logs NAME" | "process restart NAME" | "process stop NAME" => {
                if let Some(reason) = snapshot_pending() {
                    Some(reason)
                } else {
                    self.selected_process_entry()
                        .is_none()
                        .then_some("no managed process is selected")
                }
            }
            "task cancel TASK_ID" => {
                if let Some(reason) = snapshot_pending() {
                    Some(reason)
                } else if self
                    .tasks
                    .lines()
                    .any(|line| line.starts_with("●") || line.starts_with("○"))
                {
                    None
                } else {
                    Some("no queued or running task is available")
                }
            }
            "task retry TASK_ID" => {
                if let Some(reason) = snapshot_pending() {
                    Some(reason)
                } else if self.tasks.lines().any(|line| line.starts_with('×')) {
                    None
                } else {
                    Some("no failed task is available")
                }
            }
            "git stage ." | "git commit MESSAGE" => {
                if let Some(reason) = snapshot_pending() {
                    Some(reason)
                } else {
                    self.git_entries
                        .is_empty()
                        .then_some("the working tree has no changes")
                }
            }
            "git diff selected" => {
                if let Some(reason) = snapshot_pending() {
                    Some(reason)
                } else {
                    self.selected_git_entry()
                        .is_none()
                        .then_some("no changed file is selected")
                }
            }
            "debug threads SESSION"
            | "debug continue SESSION THREAD_ID"
            | "debug step SESSION THREAD_ID over"
            | "debug pause SESSION THREAD_ID" => {
                if let Some(reason) = snapshot_pending() {
                    Some(reason)
                } else if self.selected_debug_session().is_none() {
                    Some("no debugger session is selected")
                } else if self.selected_debug_thread().is_none() {
                    Some("no debugger thread is selected")
                } else {
                    None
                }
            }
            "debug break SESSION PATH LINES" => {
                if let Some(reason) = snapshot_pending() {
                    Some(reason)
                } else if self.selected_debug_session().is_none() {
                    Some("no debugger session is selected")
                } else if self.focused_editor_path.is_empty() {
                    Some("open a source file before setting a breakpoint")
                } else {
                    None
                }
            }
            "test results" => {
                if let Some(reason) = snapshot_pending() {
                    Some(reason)
                } else {
                    (self.tests.trim().is_empty() || self.tests.trim() == "No test runs")
                        .then_some("no test result is available")
                }
            }
            _ => None,
        }
    }

    pub fn surface_action_available(&self, action: command::SurfaceAction) -> bool {
        self.surface_action_unavailable_reason(action).is_none()
    }

    pub fn toggle_help(&mut self) {
        self.help_open = !self.help_open;
        self.help_scroll = 0;
        self.status = if self.help_open {
            "Help · j/k scroll · ? or Esc closes".into()
        } else {
            "Help closed".into()
        };
    }

    pub fn scroll_help(&mut self, delta: i32) {
        let visible_lines = self.terminal_height.saturating_sub(4) as usize;
        let max_scroll = super::bindings::curriculum_help(self.surface)
            .lines()
            .count()
            .saturating_sub(visible_lines)
            .min(u16::MAX as usize) as i32;
        self.help_scroll = i32::from(self.help_scroll)
            .saturating_add(delta)
            .clamp(0, max_scroll) as u16;
    }

    pub fn open_native_gc_ui_pending(&mut self, owner: crate::AgentId, command: &str) {
        let command = command.trim_start_matches('/');
        if command.is_empty() {
            return;
        }
        let requested_after_sequence = self
            .native_gc_ui_last_sequence
            .get(&owner)
            .copied()
            .unwrap_or_default();
        self.native_gc_ui_panel = Some(NativeGcUiPanel {
            owner,
            command: command.to_string(),
            scope: "Core".into(),
            title: format!("Glass Core /{command}"),
            kind: "result".into(),
            content: format!("Waiting for Glass Core /{command}…"),
            query: String::new(),
            scroll: 0,
            searching: false,
            pending: true,
            requested_after_sequence,
        });
        self.status = format!("Glass Core /{command} · Esc closes · / searches");
    }

    pub fn open_native_gc_ui_notice(
        &mut self,
        owner: crate::AgentId,
        command: &str,
        title: &str,
        content: &str,
        kind: &str,
    ) {
        let command = command.trim_start_matches('/');
        if command.is_empty() {
            return;
        }
        let requested_after_sequence = self
            .native_gc_ui_last_sequence
            .get(&owner)
            .copied()
            .unwrap_or_default();
        self.native_gc_ui_panel = Some(NativeGcUiPanel {
            owner,
            command: command.to_string(),
            scope: "Core".into(),
            title: title.to_string(),
            kind: kind.to_string(),
            content: bounded_native_gc_ui_output(content),
            query: String::new(),
            scroll: 0,
            searching: false,
            pending: false,
            requested_after_sequence,
        });
        self.status = format!("Glass Core /{command} · Esc closes · / searches");
    }

    pub fn close_native_gc_ui_panel(&mut self) {
        if self.native_gc_ui_panel.take().is_some() {
            self.status = "Glass Core panel closed · conversation and draft preserved".into();
        }
    }

    pub fn native_gc_ui_search_start(&mut self) {
        if let Some(panel) = self.native_gc_ui_panel.as_mut() {
            panel.searching = true;
            panel.query.clear();
            panel.scroll = 0;
        }
    }

    pub fn native_gc_ui_search_char(&mut self, character: char) {
        if !character.is_control()
            && let Some(panel) = self.native_gc_ui_panel.as_mut()
            && panel.searching
            && panel.query.len().saturating_add(character.len_utf8()) <= 128
        {
            panel.query.push(character);
            panel.scroll = 0;
        }
    }

    pub fn native_gc_ui_search_backspace(&mut self) {
        if let Some(panel) = self.native_gc_ui_panel.as_mut()
            && panel.searching
        {
            panel.query.pop();
            panel.scroll = 0;
        }
    }

    pub fn native_gc_ui_search_clear(&mut self) {
        if let Some(panel) = self.native_gc_ui_panel.as_mut() {
            panel.query.clear();
            panel.searching = false;
            panel.scroll = 0;
        }
    }

    pub fn native_gc_ui_search_finish(&mut self) {
        if let Some(panel) = self.native_gc_ui_panel.as_mut() {
            panel.searching = false;
        }
    }

    pub fn scroll_native_gc_ui(&mut self, delta: i32) {
        let Some(panel) = self.native_gc_ui_panel.as_mut() else {
            return;
        };
        let visible_lines = panel
            .visible_lines()
            .len()
            .saturating_sub(self.terminal_height.saturating_sub(6) as usize)
            .min(u16::MAX as usize) as i32;
        panel.scroll = i32::from(panel.scroll)
            .saturating_add(delta)
            .clamp(0, visible_lines) as u16;
    }

    fn project_native_gc_ui_outputs(
        &mut self,
        snapshot: &super::snapshot::DisplaySnapshot,
        first_snapshot: bool,
    ) {
        let mut outputs = Vec::new();
        let mut failures = Vec::new();
        for (owner, conversation) in &snapshot.agent_conversations {
            for entry in &conversation.items {
                let Some(metadata) = super::projection::native_gc_ui_entry(entry) else {
                    continue;
                };
                if metadata.error {
                    failures.push((
                        owner.clone(),
                        metadata.sequence,
                        metadata.command,
                        metadata.title,
                        entry.text.clone(),
                    ));
                    continue;
                }
                outputs.push((
                    owner.clone(),
                    metadata.sequence,
                    metadata.command,
                    metadata.scope,
                    metadata.title,
                    metadata.kind,
                    entry.text.clone(),
                ));
            }
        }
        outputs.sort_by_key(|(owner, sequence, _, _, _, _, _)| (owner.clone(), *sequence));

        failures.sort_by_key(|(owner, sequence, _, _, _)| (owner.clone(), *sequence));
        for (owner, sequence, command, title, content) in failures {
            let seen = self
                .native_gc_ui_last_sequence
                .get(&owner)
                .copied()
                .unwrap_or_default();
            if sequence > seen
                && self.native_gc_ui_panel.as_ref().is_some_and(|panel| {
                    panel.owner == owner
                        && panel.pending
                        && sequence > panel.requested_after_sequence
                })
            {
                if let Some(panel) = self.native_gc_ui_panel.as_mut() {
                    if command != "command" {
                        panel.command = command;
                        panel.title = title;
                    } else {
                        panel.title = format!("Glass Core /{} failed", panel.command);
                    }
                    panel.kind = "error".into();
                    panel.content = bounded_native_gc_ui_output(&content);
                    panel.pending = false;
                }
                self.status = "Glass Core command failed · Esc closes · / searches".into();
            }
            self.native_gc_ui_last_sequence
                .insert(owner, sequence.max(seen));
        }

        for (owner, sequence, command, scope, title, kind, content) in outputs {
            let seen = self
                .native_gc_ui_last_sequence
                .get(&owner)
                .copied()
                .unwrap_or_default();
            if sequence <= seen {
                continue;
            }
            let matches_pending_panel = self.native_gc_ui_panel.as_ref().is_some_and(|panel| {
                panel.owner == owner
                    && panel.command == command
                    && panel.pending
                    && sequence > panel.requested_after_sequence
            });
            if matches_pending_panel {
                let content = bounded_native_gc_ui_output(&content);
                if let Some(panel) = self.native_gc_ui_panel.as_mut() {
                    panel.scope = scope;
                    panel.title = title;
                    panel.kind = kind;
                    panel.content = content;
                    panel.scroll = 0;
                    panel.pending = false;
                }
            }
            self.native_gc_ui_last_sequence
                .insert(owner.clone(), sequence);
        }

        // Ordinary command results stay in the conversation. Their explicit
        // completion marker only dismisses the pending panel created when the
        // command was submitted; it never opens a panel on its own.
        let mut ordinary_completions = snapshot
            .agent_conversations
            .iter()
            .flat_map(|(owner, conversation)| {
                conversation
                    .items
                    .iter()
                    .filter(|entry| super::projection::native_gc_ui_entry(entry).is_none())
                    .filter_map(move |entry| match entry.kind {
                        super::projection::ConversationKind::System => {
                            let command =
                                entry.tool_name.as_deref()?.strip_prefix("coreCommand/")?;
                            let sequence = entry
                                .entry_id
                                .as_deref()?
                                .strip_prefix("core-command:")?
                                .parse::<u64>()
                                .ok()?;
                            Some((owner.clone(), sequence, Some(command.to_string())))
                        }
                        super::projection::ConversationKind::Error
                            if entry.tool_name.as_deref() == Some("coreCommand") =>
                        {
                            let sequence = entry
                                .entry_id
                                .as_deref()?
                                .strip_prefix("core-ui-error:")?
                                .parse::<u64>()
                                .ok()?;
                            Some((owner.clone(), sequence, None))
                        }
                        _ => None,
                    })
            })
            .collect::<Vec<_>>();
        ordinary_completions.sort_by_key(|(owner, sequence, _)| (owner.clone(), *sequence));
        for (owner, sequence, command) in ordinary_completions {
            let seen = self
                .native_gc_ui_last_sequence
                .get(&owner)
                .copied()
                .unwrap_or_default();
            let should_close = sequence > seen
                && self.native_gc_ui_panel.as_ref().is_some_and(|panel| {
                    panel.owner == owner
                        && panel.pending
                        && sequence > panel.requested_after_sequence
                        && command
                            .as_ref()
                            .is_none_or(|command| panel.command == *command)
                });
            if should_close {
                self.close_native_gc_ui_panel();
            }
            self.native_gc_ui_last_sequence
                .insert(owner, sequence.max(seen));
        }

        if first_snapshot {
            for (owner, conversation) in &snapshot.agent_conversations {
                let sequence = conversation
                    .items
                    .iter()
                    .filter_map(|entry| {
                        super::projection::native_gc_ui_entry(entry)
                            .map(|metadata| metadata.sequence)
                            .or_else(|| {
                                entry
                                    .entry_id
                                    .as_deref()
                                    .and_then(|id| id.strip_prefix("core-ui-error:"))
                                    .or_else(|| {
                                        entry
                                            .entry_id
                                            .as_deref()
                                            .and_then(|id| id.strip_prefix("core-command:"))
                                    })
                                    .and_then(|sequence| sequence.parse::<u64>().ok())
                            })
                    })
                    .max()
                    .unwrap_or_default();
                self.native_gc_ui_last_sequence
                    .entry(owner.clone())
                    .and_modify(|seen| *seen = (*seen).max(sequence))
                    .or_insert(sequence);
            }
        }
    }

    pub fn open_menu(&mut self) {
        self.menu_open = true;
        self.menu_selection = 0;
        self.move_menu_selection(0);
        self.status = format!("Command center · {}", self.surface.label());
    }

    pub fn close_menu(&mut self) {
        self.menu_open = false;
    }

    pub fn start_private_cockpit(&mut self) -> Result<String, String> {
        if let Some(cockpit) = self.private_cockpit.as_ref() {
            return Ok(cockpit.local_url());
        }
        let cockpit = crate::development::LocalCockpit::start(self.workspace.clone())
            .map_err(|error| error.to_string())?;
        let url = cockpit.local_url();
        self.cockpit_url = url.clone();
        self.private_cockpit = Some(cockpit);
        self.status = format!("Private cockpit ready · {url}");
        Ok(url)
    }

    pub fn stop_private_cockpit(&mut self) {
        if self.private_cockpit.take().is_some() {
            self.cockpit_url.clear();
            self.status = "Private cockpit stopped".into();
        } else {
            self.status = "Private cockpit is not running".into();
        }
    }

    pub fn private_cockpit_status(&self) -> String {
        self.private_cockpit
            .as_ref()
            .map(|cockpit| format!("running · {}", cockpit.local_url()))
            .unwrap_or_else(|| "not running · use `cockpit start`".into())
    }

    pub fn start_workflow_recording(&mut self, name: &str) -> Result<String, String> {
        if let Some(recording) = &self.workflow_recording {
            return Err(format!(
                "already recording {} · :workflow record stop first",
                recording.name
            ));
        }
        let name = workflow_slug(name);
        self.workflow_recording = Some(TuiWorkflowRecording {
            name: name.clone(),
            recorder: WorkflowRecorder::new(name.clone(), "1.0.0"),
            step: 0,
        });
        self.surface = DevSurface::App;
        self.refresh_workflow_recording_status();
        self.status = format!("Recording {name} · Enter activates and records locators");
        Ok(self.status.clone())
    }

    pub fn workflow_recording_status(&self) -> String {
        match &self.workflow_recording {
            Some(recording) => format!(
                "recording {} · {} step(s)",
                recording.name,
                recording.recorder.draft().steps.len()
            ),
            None => "not recording · :workflow record start NAME".into(),
        }
    }

    pub fn capture_workflow_click(&mut self) -> Result<bool, String> {
        if self.workflow_recording.is_none() {
            return Ok(false);
        }
        let selected = self
            .browser_workspace
            .state()
            .selected()
            .cloned()
            .filter(|entity| entity.actionable)
            .ok_or("no actionable semantic entity is selected")?;
        let Some(recording) = self.workflow_recording.as_mut() else {
            return Ok(false);
        };
        let id = recording.next_id();
        recording
            .recorder
            .record_click(id, selected.role, selected.name, None)
            .map_err(|error| error.to_string())?;
        self.refresh_workflow_recording_status();
        Ok(true)
    }

    pub fn record_workflow_type(&mut self, input_name: &str) -> Result<String, String> {
        let selected = self
            .browser_workspace
            .state()
            .selected()
            .cloned()
            .ok_or("no semantic entity is selected")?;
        let recording = self
            .workflow_recording
            .as_mut()
            .ok_or("start a recording first")?;
        let id = recording.next_id();
        let input = workflow_slug(input_name);
        recording
            .recorder
            .record_type_input(id, selected.role, selected.name, input.clone())
            .map_err(|error| error.to_string())?;
        let steps = recording.recorder.draft().steps.len();
        self.refresh_workflow_recording_status();
        self.status = format!("REC {steps} · typed ${{inputs.{input}}}");
        Ok(self.status.clone())
    }

    pub fn record_workflow_verify(&mut self) -> Result<String, String> {
        let value = self
            .pending_verify
            .clone()
            .ok_or("no pending prove-it predicate to attach")?;
        let expect: VerificationPredicate = serde_json::from_value(value)
            .map_err(|error| format!("pending verify is not a workflow predicate: {error}"))?;
        let recording = self
            .workflow_recording
            .as_mut()
            .ok_or("start a recording first")?;
        recording
            .recorder
            .attach_expect_to_last(expect)
            .map_err(|error| error.to_string())?;
        let steps = recording.recorder.draft().steps.len();
        self.refresh_workflow_recording_status();
        self.status = format!("REC {steps} · attached prove-it to the last step");
        Ok(self.status.clone())
    }

    pub fn stop_workflow_recording(&mut self) -> Result<String, String> {
        let recording = self
            .workflow_recording
            .take()
            .ok_or("no click-path recording is active")?;
        let draft = recording.recorder.draft();
        if draft.steps.is_empty() {
            self.workflow = "No workflow evidence yet".into();
            return Err("recording had no steps · activate entities before stop".into());
        }
        let dir = Path::new(&self.snapshot_root)
            .join(".glass")
            .join("workflows");
        std::fs::create_dir_all(&dir).map_err(|error| error.to_string())?;
        let path = dir.join(format!("{}.draft.json", recording.name));
        let encoded = serde_json::to_vec_pretty(draft).map_err(|error| error.to_string())?;
        std::fs::write(&path, &encoded).map_err(|error| error.to_string())?;
        self.workflow = format!(
            "Draft {} · {} step(s) · {}",
            recording.name,
            draft.steps.len(),
            path.display()
        );
        self.status = format!(
            "Recorded {} step(s) · {}",
            draft.steps.len(),
            path.display()
        );
        Ok(self.status.clone())
    }

    fn refresh_workflow_recording_status(&mut self) {
        if let Some(recording) = &self.workflow_recording {
            self.workflow = format!(
                "REC {} · {} step(s) · :workflow record stop",
                recording.name,
                recording.recorder.draft().steps.len()
            );
        }
    }

    fn capture_workflow_type_from_selection(&mut self) {
        if self.workflow_recording.is_none() {
            return;
        }
        let input = self
            .browser_workspace
            .state()
            .selected()
            .map(|entity| slug_input_name(&entity.name))
            .unwrap_or_else(|| "value".into());
        let _ = self.record_workflow_type(&input);
    }

    /// Run the selected command-center launcher. Keyboard hints and complete
    /// commands apply directly; commands with placeholders open the palette
    /// with the editable prefix prefilled.
    pub fn run_menu_action(&mut self) {
        let search_index = self.surface_actions().len();
        let quit_index = self.quit_menu_index();
        if self.menu_selection == search_index {
            self.menu_open = false;
            self.open_palette();
            return;
        }
        if self.menu_selection == quit_index {
            self.menu_open = false;
            self.request_quit();
            return;
        }
        let Some(action) = self.surface_actions().get(self.menu_selection).copied() else {
            return;
        };
        if let Some(input) = self.resolve_surface_action(action) {
            self.dispatch_command(&input);
        }
    }

    /// Run the first contextual action bound to a surface key. This is used
    /// for explicit, visible one-key actions such as `/` refreshing the Core
    /// task graph while the Tasks surface is active.
    pub fn run_surface_key_action(
        &mut self,
        key: &str,
        worker: &mut super::snapshot::SnapshotWorker,
    ) {
        let Some(action) = self
            .surface_actions()
            .iter()
            .find(|action| action.key == key)
            .copied()
        else {
            return;
        };
        if let Some(input) = self.resolve_surface_action(action) {
            self.dispatch_command(&input);
            self.submit_queued_tool(worker);
            worker.request_refresh();
        }
    }

    pub fn move_menu_selection(&mut self, delta: i32) {
        let count = self.surface_actions().len() + 2;
        if count == 0 {
            return;
        }
        let is_enabled = |index: usize| {
            index >= self.surface_actions().len()
                || self.surface_action_available(self.surface_actions()[index])
        };
        if delta == 0 {
            if !is_enabled(self.menu_selection) {
                self.menu_selection = (0..count).find(|index| is_enabled(*index)).unwrap_or(0);
            }
            return;
        }
        let direction = if delta < 0 { -1_i32 } else { 1_i32 };
        let steps = delta.unsigned_abs();
        let mut candidate = self.menu_selection as i32;
        for _ in 0..steps {
            for _ in 0..count {
                candidate = (candidate + direction).rem_euclid(count as i32);
                if is_enabled(candidate as usize) {
                    break;
                }
            }
        }
        self.menu_selection = candidate as usize;
    }

    pub fn open_palette(&mut self) {
        self.close_file_picker();
        self.composer_mode = false;
        self.composer_code_context_enabled = false;
        self.command_mode = true;
        self.command_input.clear();
        self.command_cursor = 0;
        self.command_history_index = None;
        self.palette_error = None;
        self.palette_scroll = 0;
        self.palette_selection = 0;
        self.status = format!(
            "Command search · {} actions · ↑↓ select · Enter run · Ctrl-P files",
            self.surface.label()
        );
    }

    pub fn close_palette(&mut self) {
        self.command_mode = false;
        if self.surface == DevSurface::Agent {
            self.composer_mode = true;
        }
        self.command_input.clear();
        self.command_cursor = 0;
        self.command_history_index = None;
        self.palette_error = None;
        self.palette_scroll = 0;
        self.palette_selection = 0;
        self.status = "Command palette closed · Ctrl-K opens Glass workspace actions".into();
    }

    pub fn palette_action_indices(&self) -> Vec<usize> {
        let query = self.command_input.trim();
        self.surface_actions()
            .iter()
            .enumerate()
            .filter_map(|(index, action)| {
                let haystack =
                    format!("{} {} {}", action.label, action.command, action.description);
                fuzzy_contains(&haystack, query).then_some(index)
            })
            .collect()
    }

    pub fn selected_palette_action(&self) -> Option<command::SurfaceAction> {
        let indices = self.palette_action_indices();
        indices
            .get(self.palette_selection)
            .and_then(|index| self.surface_actions().get(*index))
            .copied()
    }

    pub fn move_palette_selection(&mut self, delta: i32) {
        let count = self.palette_action_indices().len();
        if count == 0 {
            return;
        }
        self.palette_selection =
            (self.palette_selection as i32 + delta).rem_euclid(count as i32) as usize;
        self.palette_error = None;
        self.palette_scroll = 0;
        if let Some(action) = self.selected_palette_action() {
            self.status = format!("{} · Enter runs", action.label);
        }
    }

    pub fn submit_palette(&mut self, worker: &mut super::snapshot::SnapshotWorker) {
        let typed = self.command_input.trim().to_string();
        let resolution =
            command::resolve_palette_submission(&typed, self.selected_palette_action());
        let input = match resolution {
            command::PaletteResolution::TypedCommand(input) => {
                self.reset_palette_for_dispatch();
                Some(input)
            }
            command::PaletteResolution::SelectedAction(action) => {
                self.resolve_surface_action(action)
            }
            command::PaletteResolution::NoMatch => {
                self.status = "No matching palette action · Esc closes".into();
                return;
            }
        };

        let Some(input) = input else {
            if !self.command_mode {
                self.submit_queued_tool(worker);
                worker.request_refresh();
            }
            return;
        };

        if self.command_history.last() != Some(&input) {
            self.command_history.push(input.clone());
            if self.command_history.len() > 32 {
                self.command_history.remove(0);
            }
        }
        self.dispatch_command(&input);
        self.submit_queued_tool(worker);
        worker.request_refresh();
    }

    fn reset_palette_for_dispatch(&mut self) {
        self.command_mode = false;
        self.command_input.clear();
        self.command_cursor = 0;
        self.command_history_index = None;
        self.palette_scroll = 0;
        self.palette_selection = 0;
    }

    fn select_palette_label_match(&mut self) {
        let query = self.command_input.trim().to_ascii_lowercase();
        if query.is_empty() {
            return;
        }
        let Some(position) = self
            .palette_action_indices()
            .iter()
            .position(|action_index| {
                let label = self.surface_actions()[*action_index]
                    .label
                    .to_ascii_lowercase();
                label == query
                    || label
                        .strip_prefix(&query)
                        .is_some_and(|rest| rest.starts_with(' '))
            })
        else {
            return;
        };
        self.palette_selection = position;
    }

    fn dispatch_command(&mut self, input: &str) {
        match command::execute(self, input) {
            Ok(message) => {
                self.palette_error = None;
                self.status = message;
            }
            Err(error) => {
                self.palette_error = Some(error.clone());
                self.status = format!("Error: {error}");
            }
        }
    }

    fn resolve_surface_action(&mut self, action: command::SurfaceAction) -> Option<String> {
        if let Some(reason) = self.surface_action_unavailable_reason(action) {
            self.palette_error = Some(reason.into());
            self.status = format!("{} unavailable · {reason}", action.label);
            return None;
        }
        self.palette_error = None;
        self.menu_open = false;
        match action.command {
            "i" => {
                self.command_mode = false;
                if self.surface == DevSurface::Agent {
                    self.open_composer();
                } else {
                    self.enter_code_edit();
                }
                None
            }
            "Enter" => {
                self.command_mode = false;
                if self.surface == DevSurface::Code {
                    self.open_selected_file();
                }
                None
            }
            "browser navigate URL" => {
                self.surface = DevSurface::App;
                self.open_palette_with("browser navigate ");
                self.status =
                    "Navigate · type a URL or domain (https:// optional), then press Enter".into();
                None
            }
            "browser type TARGET TEXT" => {
                self.surface = DevSurface::App;
                self.open_palette_with("browser type ");
                self.status = "Type · enter the target and text, then press Enter".into();
                None
            }
            "debug threads SESSION" | "debug continue SESSION THREAD_ID" => {
                if let Some(session) = self.selected_debug_session().map(|row| row.name.clone()) {
                    let action = action
                        .command
                        .split_whitespace()
                        .nth(1)
                        .unwrap_or("threads");
                    if action == "continue" {
                        if let Some(thread) = self.selected_debug_thread() {
                            self.open_palette_with(&format!(
                                "debug continue {session} {}",
                                thread.id
                            ));
                        } else {
                            self.open_palette_with(&format!("debug continue {session} "));
                        }
                    } else {
                        self.open_palette_with(&format!("debug threads {session}"));
                    }
                    None
                } else {
                    self.open_palette_with("debug threads ");
                    None
                }
            }
            "process logs NAME" | "process stop NAME" | "process restart NAME" => {
                if let Some(name) = self
                    .selected_process_entry()
                    .map(|entry| entry.name.clone())
                {
                    let action = action.command.split_whitespace().nth(1).unwrap_or("logs");
                    self.open_palette_with(&format!("process {action} {name}"));
                    None
                } else {
                    self.open_palette_with(&format!(
                        "{} ",
                        action
                            .command
                            .split_whitespace()
                            .take(2)
                            .collect::<Vec<_>>()
                            .join(" ")
                    ));
                    None
                }
            }
            "1" | "T" | "I" | "O" => {
                self.command_mode = false;
                self.handle_printable(action.command.chars().next().unwrap());
                None
            }
            "Ctrl-S" if self.surface == DevSurface::Code => {
                self.command_mode = false;
                self.edit_code_key(
                    crossterm::event::KeyCode::Char('s'),
                    crossterm::event::KeyModifiers::CONTROL,
                );
                None
            }
            command => {
                let prefill = action_command_prefix(command);
                if prefill != command {
                    self.open_palette_with(&format!("{prefill} "));
                    self.status = format!(
                        "{} · enter only the required value(s), then press Enter",
                        action.label
                    );
                    None
                } else {
                    self.command_mode = false;
                    self.command_input.clear();
                    self.command_cursor = 0;
                    self.command_history_index = None;
                    self.palette_scroll = 0;
                    self.palette_selection = 0;
                    Some(command.to_string())
                }
            }
        }
    }

    pub fn insert_palette_char(&mut self, character: char) {
        self.command_input.insert(self.command_cursor, character);
        self.command_cursor += character.len_utf8();
        self.palette_error = None;
        self.command_history_index = None;
        self.palette_scroll = 0;
        self.palette_selection = 0;
        self.select_palette_label_match();
    }

    pub fn insert_palette_text(&mut self, text: &str) {
        let normalized = text.replace(['\r', '\n'], " ");
        let remaining = PALETTE_INPUT_LIMIT.saturating_sub(self.command_input.len());
        let mut accepted = String::new();
        let mut truncated = false;
        for character in normalized.chars() {
            if accepted.len() + character.len_utf8() > remaining {
                truncated = true;
                break;
            }
            accepted.push(character);
        }
        for character in accepted.chars() {
            self.insert_palette_char(character);
        }
        if truncated {
            self.status = "Palette input limit reached (8 KiB)".into();
        }
    }

    pub fn palette_backspace(&mut self) {
        if self.command_cursor == 0 {
            return;
        }
        let previous = self.command_input[..self.command_cursor]
            .char_indices()
            .next_back()
            .map(|(index, _)| index)
            .unwrap_or(0);
        self.command_input.drain(previous..self.command_cursor);
        self.command_cursor = previous;
        self.command_history_index = None;
        self.palette_scroll = 0;
        self.palette_selection = 0;
        self.select_palette_label_match();
    }

    fn palette_history_indices(&self) -> Vec<usize> {
        let visible = command::palette_order(self.surface);
        self.command_history
            .iter()
            .enumerate()
            .filter_map(|(index, input)| {
                let root = input.split_whitespace().next()?;
                let root = match root {
                    "?" => "help",
                    "q" => "quit",
                    "gh" => "github",
                    "tasks" => "task",
                    "tests" => "test",
                    "experiments" => "experiment",
                    "knowledge" => "memory",
                    "surfaces" | "backend" => "surface",
                    "tools" => "tool",
                    _ => root,
                };
                visible.contains(&root).then_some(index)
            })
            .collect()
    }

    pub fn navigate_palette_history(&mut self, older: bool) {
        let history = self.palette_history_indices();
        if history.is_empty() {
            return;
        }
        let position = self
            .command_history_index
            .and_then(|index| history.iter().position(|candidate| *candidate == index));
        let next_position = match (position, older) {
            (None, true) => history.len() - 1,
            (None, false) => return,
            (Some(position), true) => position.saturating_sub(1),
            (Some(position), false) if position + 1 < history.len() => position + 1,
            (Some(_), false) => {
                self.command_history_index = None;
                self.command_input.clear();
                self.command_cursor = 0;
                self.palette_scroll = 0;
                self.palette_selection = 0;
                return;
            }
        };
        let index = history[next_position];
        self.command_history_index = Some(index);
        self.command_input.clone_from(&self.command_history[index]);
        self.command_cursor = self.command_input.len();
        self.palette_scroll = 0;
        self.palette_selection = 0;
    }

    pub fn complete_palette(&mut self) {
        let Some(completion) = self.palette_matches().first().copied() else {
            return;
        };
        let suffix = self
            .command_input
            .find(char::is_whitespace)
            .map(|index| self.command_input[index..].to_string())
            .unwrap_or_default();
        self.command_input = format!("{completion}{suffix}");
        self.command_cursor = self.command_input.len();
        self.command_history_index = None;
        self.palette_scroll = 0;
        self.palette_selection = 0;
        self.status = format!("Completed `{completion}` · Enter runs · Ctrl-P/N history");
    }

    pub fn open_palette_with(&mut self, prefix: &str) {
        self.open_palette();
        self.command_input = prefix.into();
        self.command_cursor = self.command_input.len();
    }

    pub fn move_palette_cursor(&mut self, right: bool) {
        if right {
            self.command_cursor = self.command_input[self.command_cursor..]
                .char_indices()
                .nth(1)
                .map(|(offset, _)| self.command_cursor + offset)
                .unwrap_or(self.command_input.len());
        } else if self.command_cursor > 0 {
            self.command_cursor = self.command_input[..self.command_cursor]
                .char_indices()
                .next_back()
                .map(|(index, _)| index)
                .unwrap_or(0);
        }
    }

    pub fn scroll_palette(&mut self, delta: i32) {
        self.move_palette_selection(delta.saturating_mul(5));
    }

    pub fn palette_matches(&self) -> Vec<&'static str> {
        let query = self
            .command_input
            .split_whitespace()
            .next()
            .unwrap_or_default();
        command::palette_order(self.surface)
            .into_iter()
            .filter(|candidate| fuzzy_contains(candidate, query))
            .take(6)
            .collect()
    }

    pub fn background_action_running(&self) -> bool {
        self.pending_confirmation.is_some()
            || self.running_tool_job.is_some()
            || self.queued_tool_request.is_some()
            || self.agent_send_job.is_some()
    }

    fn blocking_background_tool_running(&self) -> bool {
        self.pending_confirmation.is_some()
            || self.running_tool_job.is_some()
            || self.queued_tool_request.is_some()
    }

    fn debug_frame_preview_view_is_current(
        &self,
        key: &SelectionPreviewKey,
        source_path: Option<&str>,
    ) -> bool {
        if key.expected_debug_pane != Some(DebugPane::Frames)
            || self.debug_pane != DebugPane::Frames
        {
            return false;
        }
        match key.surface {
            DevSurface::Debug => self.surface == DevSurface::Debug,
            DevSurface::Code => {
                self.surface == DevSurface::Code
                    && source_path.is_some_and(|path| {
                        !self.focused_editor_path.is_empty()
                            && path == self.focused_editor_path.as_str()
                    })
            }
            _ => false,
        }
    }

    fn selection_preview_key_is_current(&self, key: &SelectionPreviewKey) -> bool {
        match &key.target {
            SelectionPreviewTarget::GitDiff { path, staged } => {
                if key.surface != DevSurface::Git
                    || key.expected_debug_pane.is_some()
                    || self.surface != key.surface
                    || !self.git_diff_open
                {
                    return false;
                }
                let selected = self.selected_git_entry();
                if selected.is_some_and(|entry| entry.untracked) {
                    return false;
                }
                let current_path = selected.map(|entry| entry.path.clone());
                let current_staged = selected
                    .is_some_and(|entry| entry.index_status != ' ' && entry.worktree_status == ' ');
                &current_path == path && current_staged == *staged
            }
            SelectionPreviewTarget::ProcessLogs { name } => {
                key.surface == DevSurface::Terminal
                    && key.expected_debug_pane.is_none()
                    && self.surface == DevSurface::Terminal
                    && self
                        .selected_process_entry()
                        .is_some_and(|entry| entry.name == *name)
            }
            SelectionPreviewTarget::DebugThreads { session } => {
                key.surface == DevSurface::Debug
                    && key.expected_debug_pane == Some(DebugPane::Sessions)
                    && self.surface == DevSurface::Debug
                    && self.debug_pane == DebugPane::Sessions
                    && self
                        .selected_debug_session()
                        .is_some_and(|selected| selected.name == *session)
            }
            SelectionPreviewTarget::DebugStack { session, thread_id } => {
                key.surface == DevSurface::Debug
                    && key.expected_debug_pane == Some(DebugPane::Threads)
                    && self.surface == DevSurface::Debug
                    && self.debug_pane == DebugPane::Threads
                    && self
                        .selected_debug_session()
                        .is_some_and(|selected| selected.name == *session)
                    && self
                        .selected_debug_thread()
                        .is_some_and(|selected| selected.id == *thread_id)
            }
            SelectionPreviewTarget::DebugScopes {
                session,
                frame_id,
                frame_path,
                source_path,
            } => {
                self.debug_frame_preview_view_is_current(key, source_path.as_deref())
                    && self
                        .selected_debug_session()
                        .is_some_and(|selected| selected.name == *session)
                    && self.selected_debug_frame().is_some_and(|selected| {
                        selected.id == *frame_id
                            && selected.path.as_deref() == frame_path.as_deref()
                    })
            }
            SelectionPreviewTarget::DebugVariables {
                session,
                frame_id,
                frame_path,
                source_path,
                scope_index,
                scope_name,
                reference,
            } => {
                self.debug_frame_preview_view_is_current(key, source_path.as_deref())
                    && self
                        .selected_debug_session()
                        .is_some_and(|selected| selected.name == *session)
                    && self.selected_debug_frame().is_some_and(|selected| {
                        selected.id == *frame_id
                            && selected.path.as_deref() == frame_path.as_deref()
                    })
                    && self.selected_debug_scope == *scope_index
                    && self
                        .debug_scopes
                        .get(self.selected_debug_scope)
                        .is_some_and(|scope| {
                            scope.variables_reference == *reference && scope.name == *scope_name
                        })
            }
        }
    }

    fn finish_stale_debug_preview(&mut self, key: &SelectionPreviewKey, label: &str, queued: bool) {
        let message = match &key.target {
            SelectionPreviewTarget::DebugScopes { .. } => {
                "Debug scopes preview stale · select a frame to retry"
            }
            SelectionPreviewTarget::DebugVariables { .. } => {
                "Debug variables preview stale · select a scope to retry"
            }
            _ => return,
        };
        if !key.publishes_status_on(self.surface) {
            return;
        }
        let expected = if queued {
            format!("Queued {label} · latest selection will load when the worker is free")
        } else {
            format!("Loading {label}…")
        };
        // Status is shared with direct user actions. Only resolve the message
        // while it still belongs to this preview; a newer action owns any
        // different status string.
        if self.status == expected {
            self.debug_variables_notice = Some(message.into());
            self.status = message.into();
        }
    }

    fn set_debug_variables_notice(&mut self, message: impl Into<String>) {
        let message = message.into();
        self.debug_variables_notice = Some(message.clone());
        if self.surface == DevSurface::Debug {
            self.status = message;
        }
    }

    fn reconcile_stale_debug_previews(&mut self) {
        let stale_pending = self.pending_selection_preview.as_ref().and_then(|pending| {
            (matches!(
                &pending.key.target,
                SelectionPreviewTarget::DebugScopes { .. }
                    | SelectionPreviewTarget::DebugVariables { .. }
            ) && !self.selection_preview_key_is_current(&pending.key))
            .then(|| (pending.key.clone(), pending.label.clone()))
        });
        if let Some((key, label)) = stale_pending {
            self.pending_selection_preview = None;
            self.finish_stale_debug_preview(&key, &label, true);
        }

        let stale_active = self.active_selection_preview.as_ref().and_then(|active| {
            (matches!(
                &active.key.target,
                SelectionPreviewTarget::DebugScopes { .. }
                    | SelectionPreviewTarget::DebugVariables { .. }
            ) && !self.selection_preview_key_is_current(&active.key))
            .then(|| (active.key.clone(), active.label.clone()))
        });
        if let Some((key, label)) = stale_active {
            self.finish_stale_debug_preview(&key, &label, false);
        }
    }

    fn queue_selection_preview(
        &mut self,
        worker: &mut super::snapshot::SnapshotWorker,
        key: SelectionPreviewKey,
        call: crate::development::ToolCall,
        context: crate::tools::DevelopmentToolContext,
        label: String,
    ) {
        let publish_status = key.publishes_status_on(self.surface);
        if self
            .active_selection_preview
            .as_ref()
            .is_some_and(|active| active.key == key && self.running_tool_job == Some(active.id))
        {
            // Returning to the active selection makes an older pending target
            // obsolete; the in-flight result will again match the UI.
            self.pending_selection_preview = None;
            if publish_status && let Some(active) = &self.active_selection_preview {
                self.status = format!("Loading {}…", active.label);
            }
            return;
        }

        self.pending_selection_preview = Some(PendingSelectionPreview {
            key,
            call,
            context,
            label: label.clone(),
        });
        self.flush_pending_selection_preview(worker);
        if publish_status && self.pending_selection_preview.is_some() {
            self.status =
                format!("Queued {label} · latest selection will load when the worker is free");
        }
    }

    pub(super) fn flush_pending_selection_preview(
        &mut self,
        worker: &mut super::snapshot::SnapshotWorker,
    ) {
        if self.background_action_running() {
            return;
        }
        let Some(pending) = self.pending_selection_preview.take() else {
            return;
        };
        if !self.selection_preview_key_is_current(&pending.key) {
            self.finish_stale_debug_preview(&pending.key, &pending.label, true);
            return;
        }
        let publish_status = pending.key.publishes_status_on(self.surface);
        match worker.submit_tool(pending.call, pending.context) {
            Ok(id) => {
                self.running_tool_job = Some(id);
                self.active_selection_preview = Some(ActiveSelectionPreview {
                    id,
                    key: pending.key,
                    label: pending.label.clone(),
                });
                if publish_status {
                    self.status = format!("Loading {}…", pending.label);
                }
            }
            Err(error) => {
                self.active_selection_preview = None;
                if publish_status {
                    self.status = format!("{} unavailable: {error}", pending.label);
                }
            }
        }
    }

    pub fn browser_chat_header(&self) -> String {
        let browser = self.browser_workspace.state();
        let page = if browser.url.is_empty() {
            "no page".to_string()
        } else {
            safe_browser_url(&browser.url).unwrap_or_else(|| "no page".into())
        };
        let title = browser.title.trim();
        let include_title = !title.is_empty() && !title.eq_ignore_ascii_case(&page);
        let revision = browser
            .browser_revision
            .map_or_else(|| "—".to_string(), |revision| revision.to_string());
        let bootstrap = if browser.connection
            == glass_browser::browser_workspace::BrowserConnectionPhase::Connected
        {
            "observe before acting"
        } else {
            "ask `open <url>` to attach"
        };
        let mut header = format!("BROWSER · {}", browser.connection_label());
        if include_title {
            header.push_str(" · ");
            header.push_str(title);
        }
        header.push_str(&format!(" · {} · rev {} · {}", page, revision, bootstrap));
        if let Some(entity) = browser.selected() {
            header.push_str(" · selected ");
            header.push_str(&entity.name);
        }
        header
    }

    pub fn conversation_view(&self) -> String {
        let mut conversation = if self.agent_conversation.starts_with("No conversation yet.")
            && !self.pending_chat_messages.is_empty()
        {
            String::new()
        } else {
            self.agent_conversation.clone()
        };
        let observed = self
            .conversation_items
            .iter()
            .filter_map(|entry| entry.client_message_id.as_deref())
            .collect::<std::collections::HashSet<_>>();
        for message in &self.pending_chat_messages {
            if message.agent_id != self.selected_agent {
                continue;
            }
            if message.state != ChatMessageState::Failed
                && observed.contains(message.message_id.as_str())
            {
                continue;
            }
            if !conversation.is_empty() {
                conversation.push_str("\n\n");
            }
            conversation.push_str("YOU\n");
            conversation.push_str(&message.text);
            conversation.push('\n');
            match message.state {
                ChatMessageState::Sending => conversation.push_str("· sending…"),
                ChatMessageState::Sent => {
                    conversation.push_str("· sent · Glass Agent is thinking…")
                }
                ChatMessageState::Failed => {
                    conversation.push_str("× send failed");
                    if let Some(error) = message.error.as_deref() {
                        conversation.push_str(": ");
                        conversation.push_str(error);
                    }
                    conversation.push_str(" · press Enter to retry");
                }
            }
        }
        conversation
    }

    pub fn conversation_entries_cow(&self) -> Cow<'_, [super::projection::ConversationEntry]> {
        let has_selected_pending = self
            .pending_chat_messages
            .iter()
            .any(|message| message.agent_id == self.selected_agent);

        let cached = if !self.conversation_items.is_empty() {
            Some(self.conversation_items.as_slice())
        } else {
            self.selected_agent
                .as_ref()
                .and_then(|agent| self.agent_conversations.get(agent))
                .map(|conversation| conversation.items.as_slice())
                .filter(|entries| !entries.is_empty())
        };
        if !has_selected_pending {
            if let Some(entries) = cached {
                return Cow::Borrowed(entries);
            }
            if self.agent_conversation.starts_with("No conversation yet.") {
                return Cow::Borrowed(&[]);
            }
            return Cow::Owned(parse_conversation_view(&self.agent_conversation));
        }
        let mut entries = cached.map_or_else(Vec::new, |entries| entries.to_vec());
        if entries.is_empty() && !self.agent_conversation.starts_with("No conversation yet.") {
            entries = parse_conversation_view(&self.agent_conversation);
        }
        let observed = entries
            .iter()
            .filter_map(|entry| entry.client_message_id.as_deref())
            .map(str::to_owned)
            .collect::<std::collections::HashSet<_>>();
        let visible_pending = self
            .pending_chat_messages
            .iter()
            .filter(|message| message.agent_id == self.selected_agent)
            .filter(|message| {
                message.state == ChatMessageState::Failed || !observed.contains(&message.message_id)
            })
            .cloned()
            .collect::<Vec<_>>();
        for message in visible_pending {
            let error_suffix = match message.state {
                ChatMessageState::Sending => "· sending…".to_string(),
                ChatMessageState::Sent => "· sent · Glass Agent is thinking…".to_string(),
                ChatMessageState::Failed => format!(
                    "× send failed: {}\nEnter retries this message · edit to send a new one",
                    message.error.as_deref().unwrap_or("unknown send error")
                ),
            };
            if let Some(entry) = entries.iter_mut().find(|entry| {
                entry.client_message_id.as_deref() == Some(message.message_id.as_str())
            }) {
                if message.state == ChatMessageState::Failed
                    && !entry.text.contains("× send failed:")
                {
                    entry.text.push('\n');
                    entry.text.push_str(&error_suffix);
                }
                continue;
            }
            entries.push(super::projection::ConversationEntry {
                kind: super::projection::ConversationKind::User,
                text: format!("{}\n{error_suffix}", message.text),
                streaming: false,
                entry_id: None,
                client_message_id: Some(message.message_id.clone()),
                tool_name: None,
                tool_card: None,
                core_ui: None,
            });
        }
        Cow::Owned(entries)
    }

    pub fn conversation_entries_view(&self) -> Vec<super::projection::ConversationEntry> {
        self.conversation_entries_cow().into_owned()
    }

    pub fn composer_context_chips(&self) -> String {
        let mut chips = Vec::new();
        if self.surface == DevSurface::Code
            && self.composer_code_context_enabled
            && !self.focused_editor_path.is_empty()
        {
            chips.push(format!(
                "file {}{} · Ctrl-Shift-X remove · Ctrl-Shift-A attach",
                self.focused_editor_path,
                if self.focused_editor_dirty {
                    " (unsaved)"
                } else {
                    ""
                }
            ));
        }
        if self.surface == DevSurface::Code
            && self.composer_code_context_enabled
            && self.focused_editor_selection.is_some()
        {
            chips.push("selection".into());
        }
        if !self.code_context_attachments.is_empty() {
            let shown = self.code_context_attachments.len().min(3);
            let paths = self.code_context_attachments[..shown]
                .iter()
                .map(|attachment| {
                    format!(
                        "{}{}",
                        attachment.path,
                        if attachment.selection.is_some() {
                            " [range]"
                        } else {
                            ""
                        }
                    )
                })
                .collect::<Vec<_>>()
                .join(", ");
            let more = self.code_context_attachments.len().saturating_sub(shown);
            chips.push(format!(
                "context {paths}{} · Ctrl-Shift-X removes focused file",
                if more > 0 {
                    format!(", +{more} more")
                } else {
                    String::new()
                }
            ));
        }
        if let Some(entity) = self.browser_workspace.state().selected() {
            chips.push(format!("app {}", entity.name));
        }
        if self.pending_verify.is_some() {
            chips.push("prove-it".into());
        }
        if self.composer_steer {
            chips.push("steer".into());
        }
        chips.join(" · ")
    }

    pub fn toggle_composer_code_context(&mut self) {
        if self.surface != DevSurface::Code || self.focused_editor_path.is_empty() {
            self.composer_code_context_enabled = false;
            self.status = "Code context is available while asking from an open Code buffer".into();
            return;
        }
        if let Some(index) = self
            .code_context_attachments
            .iter()
            .position(|attachment| attachment.path == self.focused_editor_path)
        {
            self.code_context_attachments.remove(index);
            self.composer_code_context_enabled = false;
            self.status = format!(
                "Code context removed · {} will not be sent",
                self.focused_editor_path
            );
            return;
        }
        self.composer_code_context_enabled = !self.composer_code_context_enabled;
        self.status = if self.composer_code_context_enabled {
            format!(
                "Code context attached on send · {}",
                self.focused_editor_path
            )
        } else {
            "Code context removed · your message will be sent without the buffer".into()
        };
    }

    pub fn attach_focused_code_context(&mut self) {
        if self.surface != DevSurface::Code || self.focused_editor_path.is_empty() {
            self.status = "Open a Code buffer before attaching it to the composer".into();
            return;
        }
        if self
            .code_context_attachments
            .iter()
            .any(|attachment| attachment.path == self.focused_editor_path)
        {
            self.status = format!(
                "{} is already attached · Ctrl-Shift-X removes it",
                self.focused_editor_path
            );
            return;
        }
        if self.code_context_attachments.len() >= MAX_CODE_CONTEXT_FILES {
            self.status =
                format!("Code context holds up to {MAX_CODE_CONTEXT_FILES} additional files");
            return;
        }
        self.code_context_attachments.push(CodeContextAttachment {
            path: self.focused_editor_path.clone(),
            selection: self.focused_editor_selection.clone(),
            content: self.focused_editor_content.clone(),
            dirty: self.focused_editor_dirty,
        });
        self.status = format!(
            "Attached {}{} to the composer · context is bounded to 64 KiB",
            self.focused_editor_path,
            if self.focused_editor_selection.is_some() {
                " selection"
            } else {
                ""
            }
        );
    }

    pub(crate) fn remember_lsp_format_baseline(&mut self, path: &str) -> Result<(), String> {
        if !self.pending_lsp_format_baselines.is_empty() {
            return Err("an LSP formatting preview is already in flight".into());
        }
        let workspace = self.ws()?;
        let path = workspace
            .project()
            .normalize_existing_file_path(path)
            .map_err(|error| error.to_string())?;
        let content = workspace
            .project()
            .buffer_or_file_content(&path)
            .map_err(|error| error.to_string())?;
        drop(workspace);
        self.pending_lsp_format_baselines
            .push_back(LspEditBaseline { path, content });
        Ok(())
    }

    pub(crate) fn forget_lsp_format_baseline(&mut self) {
        self.pending_lsp_format_baselines.pop_back();
    }

    fn preview_lsp_formatting(&mut self, value: &serde_json::Value) -> Result<bool, String> {
        let Some(baseline) = self.pending_lsp_format_baselines.front().cloned() else {
            return Ok(false);
        };
        let edits = value
            .get("result")
            .and_then(serde_json::Value::as_array)
            .ok_or_else(|| "LSP formatter returned no TextEdit array".to_string())?;
        let proposed = apply_lsp_text_edits(&baseline.content, edits)?;
        if proposed == baseline.content {
            self.pending_lsp_format_baselines.pop_front();
            self.status = format!("LSP formatting made no changes · {}", baseline.path);
            return Ok(true);
        }
        let proposal = self
            .ws_mut()?
            .project_mut()
            .propose_editor_change(
                &baseline.path,
                baseline.content,
                proposed,
                "LSP formatting preview".into(),
                crate::development::Actor::local(),
            )
            .map_err(|error| error.to_string())?;
        self.pending_lsp_format_baselines.pop_front();
        self.selected_editor_proposal_id = Some(proposal.id.clone());
        if !self.files.iter().any(|path| path == &proposal.path) {
            self.files.push(proposal.path.clone());
        }
        if let Some(index) = self.files.iter().position(|path| path == &proposal.path) {
            self.selected_file = index;
        }
        if let Some(index) = self.ws().ok().and_then(|workspace| {
            workspace
                .project()
                .buffers()
                .position(|buffer| buffer.path == proposal.path)
        }) {
            self.editor_buffer_index = index;
        }
        self.surface = DevSurface::Code;
        self.code_edit_mode = true;
        self.refresh_editor_projection();
        self.refresh_editor_hunks();
        self.status = format!(
            "LSP formatting preview · {} · inspect diff, then Ctrl-K editor accept {} or editor reject {}",
            proposal.path, proposal.id, proposal.id
        );
        Ok(true)
    }

    fn preview_lsp_workspace_edit(&mut self, value: &serde_json::Value) -> Result<bool, String> {
        let entries = lsp_workspace_edit_entries(value)?;
        if entries.is_empty() {
            self.status = "LSP workspace edit made no changes".into();
            return Ok(true);
        }

        let plans = {
            let workspace = self.ws()?;
            let root = workspace.project().root().to_path_buf();
            let mut plans = Vec::with_capacity(entries.len());
            let mut seen_paths = std::collections::BTreeSet::new();
            for (uri, edits) in entries {
                let parsed = url::Url::parse(&uri).map_err(|_| {
                    "LSP workspace edit contains an invalid document URI".to_string()
                })?;
                if parsed.scheme() != "file" {
                    return Err("LSP workspace edit may only target local file URIs".into());
                }
                let absolute_path = parsed
                    .to_file_path()
                    .map_err(|()| "LSP workspace edit contains an invalid file URI".to_string())?;
                let root = std::fs::canonicalize(&root)
                    .map_err(|_| "LSP workspace root is unavailable".to_string())?;
                let absolute_path = std::fs::canonicalize(&absolute_path)
                    .map_err(|_| "LSP workspace edit targets an unavailable file".to_string())?;
                let relative_path =
                    relative_workspace_path(&root, &absolute_path).ok_or_else(|| {
                        "LSP workspace edit targets a file outside this project".to_string()
                    })?;
                let relative_path = relative_path
                    .to_str()
                    .ok_or_else(|| "LSP workspace edit path is not valid UTF-8".to_string())?
                    .replace(std::path::MAIN_SEPARATOR, "/");
                let path = workspace
                    .project()
                    .normalize_existing_file_path(&relative_path)
                    .map_err(|error| error.to_string())?;
                if !seen_paths.insert(path.clone()) {
                    return Err(format!(
                        "LSP workspace edit addresses {path} more than once"
                    ));
                }
                let original = workspace
                    .project()
                    .buffer_or_file_content(&path)
                    .map_err(|error| error.to_string())?;
                let proposed = apply_lsp_text_edits(&original, &edits)?;
                if proposed != original {
                    plans.push((path, original, proposed));
                }
            }
            plans
        };
        if plans.is_empty() {
            self.status = "LSP workspace edit made no changes".into();
            return Ok(true);
        }

        let first_path = plans[0].0.clone();
        let mut workspace = self.ws_mut()?;
        let mut proposals = Vec::with_capacity(plans.len());
        for (path, original, proposed) in plans {
            let proposal = workspace
                .project_mut()
                .propose_editor_change(
                    &path,
                    original,
                    proposed,
                    "LSP workspace edit preview".into(),
                    crate::development::Actor::local(),
                )
                .map_err(|error| error.to_string())?;
            proposals.push(proposal);
        }
        drop(workspace);

        self.selected_editor_proposal_id = proposals.first().map(|proposal| proposal.id.clone());
        for proposal in &proposals {
            if !self.files.iter().any(|path| path == &proposal.path) {
                self.files.push(proposal.path.clone());
            }
        }
        if let Some(index) = self.files.iter().position(|path| path == &first_path) {
            self.selected_file = index;
        }
        if let Some(index) = self.ws().ok().and_then(|workspace| {
            workspace
                .project()
                .buffers()
                .position(|buffer| buffer.path == first_path)
        }) {
            self.editor_buffer_index = index;
        }
        self.surface = DevSurface::Code;
        self.code_edit_mode = true;
        self.refresh_editor_projection();
        self.refresh_editor_hunks();
        self.status = format!(
            "LSP workspace edit preview · {} files · review each diff, then accept or reject its proposal",
            proposals.len()
        );
        Ok(true)
    }

    fn handle_lsp_preview_result(&mut self, tool: &str, value: &serde_json::Value) {
        let result = match tool {
            "glass.lsp.formatting" | "glass.lsp.range_formatting" => {
                self.preview_lsp_formatting(value)
            }
            "glass.lsp.rename" => self.preview_lsp_workspace_edit(value),
            _ => return,
        };
        match result {
            Ok(true) => {}
            Ok(false) => {
                self.status = format!("{tool} result received");
            }
            Err(error) if Self::is_workspace_busy(&error) => {
                if self.pending_lsp_preview_results.len() >= 8 {
                    if matches!(tool, "glass.lsp.formatting" | "glass.lsp.range_formatting") {
                        self.pending_lsp_format_baselines.pop_front();
                    }
                    self.status =
                        "LSP preview queue is full · wait for the workspace and retry".into();
                    return;
                }
                self.pending_lsp_preview_results
                    .push_back((tool.to_string(), value.clone()));
                self.status = "LSP preview queued · waiting for the workspace".into();
            }
            Err(error) => {
                if matches!(tool, "glass.lsp.formatting" | "glass.lsp.range_formatting") {
                    self.pending_lsp_format_baselines.pop_front();
                    self.status = format!("LSP formatting preview failed · {error}");
                } else {
                    self.status = format!("LSP workspace edit preview failed · {error}");
                }
            }
        }
    }

    fn flush_pending_lsp_preview_result(&mut self) -> bool {
        let Some((tool, value)) = self.pending_lsp_preview_results.pop_front() else {
            return false;
        };
        let result = match tool.as_str() {
            "glass.lsp.formatting" | "glass.lsp.range_formatting" => {
                self.preview_lsp_formatting(&value)
            }
            "glass.lsp.rename" => self.preview_lsp_workspace_edit(&value),
            _ => return true,
        };
        match result {
            Ok(true) => true,
            Ok(false) => {
                self.status = format!("{tool} result received");
                true
            }
            Err(error) if Self::is_workspace_busy(&error) => {
                self.pending_lsp_preview_results.push_front((tool, value));
                false
            }
            Err(error) => {
                if matches!(
                    tool.as_str(),
                    "glass.lsp.formatting" | "glass.lsp.range_formatting"
                ) {
                    self.pending_lsp_format_baselines.pop_front();
                    self.status = format!("LSP formatting preview failed · {error}");
                } else {
                    self.status = format!("LSP workspace edit preview failed · {error}");
                }
                true
            }
        }
    }

    pub fn agent_chrome_line(&self) -> String {
        let mut parts = Vec::new();
        if !self.agent_model.is_empty() {
            parts.push(self.agent_model.clone());
        }
        if !self.agent_thinking.is_empty() {
            parts.push(format!("think {}", self.agent_thinking));
        }
        if !self.agent_session_name.is_empty() {
            parts.push(self.agent_session_name.clone());
        }
        if !self.agent_token_summary.is_empty() {
            parts.push(self.agent_token_summary.clone());
        }
        let queued = self
            .pending_chat_messages
            .iter()
            .filter(|message| matches!(message.state, ChatMessageState::Sending))
            .count();
        if queued > 0 {
            parts.push(format!("queued {queued}"));
        }
        parts.join(" · ")
    }

    pub fn move_transcript_selection(&mut self, delta: i32) {
        let len = self.conversation_entries_view().len();
        if len == 0 {
            self.scroll_surface(delta);
            return;
        }
        let next = self.transcript_selection as i32 + delta;
        self.transcript_selection = next.clamp(0, len as i32 - 1) as usize;
        self.transcript_expanded = false;
        if self.surface == DevSurface::Agent {
            let (_, height) = super::render::agent_conversation_viewport_size(self);
            self.core_transcript
                .borrow_mut()
                .focus_entry(self.transcript_selection, height);
        }
        self.status = format!(
            "Bubble {}/{} · f fork · r rewind · e edit last · o expand",
            self.transcript_selection + 1,
            len
        );
    }

    pub fn toggle_transcript_expand(&mut self) {
        self.transcript_expanded = !self.transcript_expanded;
        self.status = if self.transcript_expanded {
            "Expanded tool card · o collapse".into()
        } else {
            "Collapsed · o expand".into()
        };
    }

    pub fn toggle_reasoning_view(&mut self) {
        self.reasoning_view_open = !self.reasoning_view_open;
        self.status = if self.reasoning_view_open {
            "Core reasoning · Ctrl-T or Esc to return".into()
        } else {
            "Conversation transcript".into()
        };
    }

    pub fn fork_selected_transcript(&mut self, worker: &mut super::snapshot::SnapshotWorker) {
        self.branch_selected_transcript("glass.agent.fork", worker);
    }

    pub fn rewind_selected_transcript(&mut self, worker: &mut super::snapshot::SnapshotWorker) {
        self.branch_selected_transcript("glass.agent.rewind", worker);
    }

    fn branch_selected_transcript(
        &mut self,
        tool: &str,
        worker: &mut super::snapshot::SnapshotWorker,
    ) {
        let Some(entry) = self
            .conversation_entries_view()
            .get(self.transcript_selection)
            .cloned()
        else {
            self.status = "No transcript bubble to branch".into();
            return;
        };
        let Some(entry_id) = entry.entry_id.clone() else {
            self.status = "Selected bubble has no conversation entry · send a turn first".into();
            return;
        };
        let mut arguments = serde_json::json!({"entryId": entry_id});
        if let Some(agent) = self.selected_agent.as_ref() {
            arguments["agentId"] = serde_json::Value::String(agent.as_str().to_string());
        }
        match self.tool_request(tool, arguments, true) {
            Ok((call, context)) => match worker.submit_tool(call, context) {
                Ok(_) => {
                    self.status = format!("{tool} · branching at {entry_id}");
                    worker.request_conversation();
                }
                Err(error) => self.status = format!("Could not {tool}: {error}"),
            },
            Err(error) => self.status = format!("Could not {tool}: {error}"),
        }
    }

    pub fn edit_last_user_message(&mut self) {
        let entries = self.conversation_entries_view();
        let Some(entry) = entries
            .iter()
            .rev()
            .find(|entry| entry.kind == super::projection::ConversationKind::User)
        else {
            self.status = "No user message to edit".into();
            return;
        };
        let text = entry
            .text
            .lines()
            .next()
            .unwrap_or(entry.text.as_str())
            .to_string();
        self.composer_input = text;
        self.composer_cursor = self.composer_input.len();
        self.composer_steer = false;
        self.open_composer();
        if let Some(entry_id) = entry.entry_id.clone() {
            self.status = format!("Editing last user message · rewind {entry_id} on send");
        } else {
            self.status = "Editing last user message · Enter resends".into();
        }
    }

    pub fn open_session_picker(&mut self, value: &serde_json::Value) {
        self.session_picker_items = parse_session_picker_items(value);
        self.session_picker_selection = 0;
        self.session_picker_open = !self.session_picker_items.is_empty();
        let collection = "conversation(s)";
        let none = "No persisted conversations";
        self.status = if self.session_picker_open {
            format!(
                "{} {collection} · Enter switch · Esc close",
                self.session_picker_items.len(),
            )
        } else {
            none.into()
        };
    }

    pub fn close_session_picker(&mut self) {
        self.session_picker_open = false;
    }

    pub fn move_session_picker_selection(&mut self, delta: i32) {
        if self.session_picker_items.is_empty() {
            return;
        }
        let next = self.session_picker_selection as i32 + delta;
        self.session_picker_selection =
            next.clamp(0, self.session_picker_items.len() as i32 - 1) as usize;
    }

    pub fn submit_session_picker(&mut self, worker: &mut super::snapshot::SnapshotWorker) {
        let Some(item) = self.session_picker_items.get(self.session_picker_selection) else {
            self.status = "No session selected".into();
            return;
        };
        let mut arguments = serde_json::json!({"path": item.path});
        if let Some(agent) = self.selected_agent.as_ref() {
            arguments["agentId"] = serde_json::Value::String(agent.as_str().to_string());
        }
        match self.tool_request("glass.agent.switch-session", arguments, true) {
            Ok((call, context)) => match worker.submit_tool(call, context) {
                Ok(_) => {
                    self.agent_session_name = item.label.clone();
                    self.session_picker_open = false;
                    self.status = format!("Switching to {}", item.label);
                    worker.request_conversation();
                }
                Err(error) => self.status = format!("Could not switch session: {error}"),
            },
            Err(error) => self.status = format!("Could not switch session: {error}"),
        }
    }

    fn composer_send_blocked(&self) -> bool {
        self.pending_confirmation.is_some()
            || self.queued_tool_request.is_some()
            || self.running_tool_job.is_some()
    }

    fn reconcile_pending_chat(&mut self) {
        let observed = self
            .conversation_items
            .iter()
            .filter_map(|entry| entry.client_message_id.clone())
            .collect::<std::collections::HashSet<_>>();
        self.pending_chat_messages.retain(|message| {
            message.state == ChatMessageState::Failed || !observed.contains(&message.message_id)
        });
    }
    /// Open the composer immediately; Core readiness may lag the prompt queue.
    pub fn start_agent_interaction(&mut self) {
        self.open_composer();
    }

    /// Select a resident agent without forcing the user through the command
    /// palette. The selected agent becomes the target for follow-up and
    /// session-control actions.
    pub fn cycle_agent_selection(&mut self, delta: i32) {
        let agents = match self.workspace.try_lock() {
            Ok(mut workspace) => match workspace.agents().list() {
                Ok(agents) => agents,
                Err(error) => {
                    self.status = format!("Agent list unavailable: {error}");
                    return;
                }
            },
            Err(error) => {
                self.status = format!("Agent list unavailable: {error}");
                return;
            }
        };
        if agents.is_empty() {
            if !self.select_agent(None) {
                return;
            }
            self.status = "No resident agents · Enter starts the Glass Agent".into();
            return;
        }
        let current = self
            .selected_agent
            .as_ref()
            .and_then(|selected| agents.iter().position(|agent| &agent.id == selected))
            .unwrap_or(0);
        let next = if delta.is_negative() {
            (current + agents.len() - (delta.unsigned_abs() as usize % agents.len())) % agents.len()
        } else {
            (current + (delta as usize % agents.len())) % agents.len()
        };
        let selected = agents[next].id.clone();
        if !self.select_agent(Some(selected.clone())) {
            return;
        }
        self.status = format!(
            "Selected {} · type to chat · ]/[ switches agent",
            selected.as_str()
        );
    }

    pub fn open_thread_picker(&mut self) {
        self.thread_picker_previous_status = Some(self.status.clone());
        self.thread_picker_selection = self
            .thread_picker_items
            .iter()
            .position(|item| Some(&item.agent_id) == self.selected_agent.as_ref())
            .unwrap_or(0);
        self.thread_picker_open = true;
        self.status = if self.thread_picker_items.is_empty() {
            if self.snapshot_ready {
                "No project conversations yet".into()
            } else {
                "Loading project conversations".into()
            }
        } else {
            "Project conversations · Enter opens · Esc returns".into()
        };
    }

    pub fn close_thread_picker(&mut self) {
        self.thread_picker_open = false;
        if let Some(status) = self.thread_picker_previous_status.take() {
            self.status = status;
        }
    }

    pub fn move_thread_picker_selection(&mut self, delta: i32) {
        if self.thread_picker_items.is_empty() {
            return;
        }
        let next = self.thread_picker_selection as i32 + delta;
        self.thread_picker_selection =
            next.clamp(0, self.thread_picker_items.len() as i32 - 1) as usize;
    }

    pub fn activate_thread_picker_selection(&mut self) {
        let Some(item) = self.thread_picker_items.get(self.thread_picker_selection) else {
            self.status = "No project conversation selected".into();
            return;
        };
        let item = item.clone();
        if !self.select_agent(Some(item.agent_id.clone())) {
            return;
        }
        self.surface = DevSurface::Agent;
        self.thread_picker_open = false;
        self.thread_picker_previous_status = None;
        self.status = format!("{} · {}", item.title, item.status);
    }

    pub fn open_follow_up_queue(&mut self) {
        let Some(agent_id) = self.selected_agent.clone() else {
            self.status = "Select a resident thread before opening its follow-up queue".into();
            return;
        };
        self.follow_up_queue_open = true;
        self.follow_up_queue_selection = 0;
        if self.follow_up_queue_snapshots.contains_key(&agent_id) {
            let _ = self.refresh_follow_up_queue_run(&agent_id);
            self.status =
                "Thread queue · a add · e edit · J/K reorder · d remove · Enter send".into();
            return;
        }
        #[cfg(unix)]
        {
            let writer = self
                .follow_up_queue_writer
                .get_or_insert_with(FollowUpQueueWriter::spawn);
            match writer.load(self.workspace.clone(), agent_id.clone()) {
                Ok(reply) => {
                    let generation = self
                        .follow_up_queue_generations
                        .get(&agent_id)
                        .copied()
                        .unwrap_or_default();
                    self.pending_follow_up_queue_loads
                        .insert(agent_id, (generation, reply));
                    self.status = "Loading this thread’s follow-up queue…".into();
                }
                Err(error) => {
                    self.status = format!("Could not load follow-up queue: {error}");
                }
            }
        }
        #[cfg(windows)]
        {
            let target = self.workspace.try_lock().and_then(|mut workspace| {
                workspace
                    .agents()
                    .follow_up_queue_target(&agent_id)
                    .map(|(_, thread_id, run_id)| (thread_id, run_id))
            });
            match target {
                Ok((thread_id, current_run_id)) => {
                    self.follow_up_queue_snapshots.insert(
                        agent_id,
                        FollowUpQueueSnapshot {
                            thread_id,
                            current_run_id,
                            entries: Vec::new(),
                            send_attempts: Vec::new(),
                        },
                    );
                    self.status = "Queue is available for this session only on Windows".into();
                }
                Err(error) => self.status = format!("Could not open follow-up queue: {error}"),
            }
        }
    }

    pub fn close_follow_up_queue(&mut self) {
        self.follow_up_queue_open = false;
        self.cancel_follow_up_queue_edit();
    }

    pub fn open_attention_inbox(&mut self) {
        self.attention_inbox_open = true;
        self.refresh_attention_items();
        #[cfg(unix)]
        if self.attention_queue_load.is_none() {
            let writer = self
                .follow_up_queue_writer
                .get_or_insert_with(FollowUpQueueWriter::spawn);
            match writer.load_inbox(self.workspace.clone()) {
                Ok(reply) => {
                    self.attention_queue_load =
                        Some((self.follow_up_queue_generations.clone(), reply));
                    self.attention_queue_refresh_at = None;
                    self.status = "Loading reviewable follow-ups…".into();
                }
                Err(error) => {
                    self.attention_queue_refresh_at =
                        Some(std::time::Instant::now() + std::time::Duration::from_secs(2));
                    self.status = format!("Could not refresh follow-up review items: {error}");
                }
            }
        }
    }

    pub fn close_attention_inbox(&mut self) {
        self.attention_inbox_open = false;
    }

    pub fn request_uncertain_send_retry(&mut self) {
        let Some(target) = self.send_attempt_retry_candidate.clone() else {
            self.status = "Select a failed or uncertain message from Attention Inbox first".into();
            return;
        };
        if self.selected_agent.as_ref() != Some(&target.agent_id) {
            self.status = "Open the conversation that owns this message first".into();
            return;
        }
        if !self.refresh_follow_up_queue_run(&target.agent_id) {
            self.status =
                "Could not verify the selected conversation · inspect it before retrying".into();
            return;
        }
        let attempt = self
            .follow_up_queue_snapshots
            .get(&target.agent_id)
            .filter(|snapshot| snapshot.thread_id == target.thread_id)
            .and_then(|snapshot| {
                snapshot.send_attempts.iter().find(|attempt| {
                    attempt.attempt_id == target.attempt_id
                        && retryable_send_attempt_status(attempt)
                        && attempt.status == target.status
                        && attempt.client_message_id == target.client_message_id
                        && attempt.mode == target.mode
                        && attempt.text == target.text
                })
            });
        if attempt.is_none() {
            self.send_attempt_retry_candidate = None;
            self.status = "This message is no longer retryable · inbox refreshed".into();
            self.refresh_attention_items();
            return;
        }
        let Some(agent_status) = self
            .workspace
            .try_lock()
            .ok()
            .and_then(|mut workspace| workspace.agents().snapshot(&target.agent_id).ok())
            .map(|snapshot| snapshot.status)
        else {
            self.status =
                "Could not verify the current conversation · resume it before retrying".into();
            return;
        };
        if !matches!(
            agent_status,
            crate::AgentStatus::Idle | crate::AgentStatus::Working | crate::AgentStatus::Waiting
        ) {
            self.status =
                "Resume this recovery-pending conversation before retrying its message".into();
            return;
        }
        if attempt.is_some_and(|attempt| {
            attempt.mode == crate::thread_store::SendAttemptMode::Steer
                && agent_status != crate::AgentStatus::Working
        }) {
            self.status =
                "This steer targets a run that is no longer active · review it instead".into();
            return;
        }
        self.send_attempt_retry_confirmation = Some(target);
        self.status = "Retry may repeat work Core already performed · confirm explicitly".into();
    }

    pub fn cancel_uncertain_send_retry(&mut self) {
        self.send_attempt_retry_confirmation = None;
    }

    pub fn confirm_uncertain_send_retry(&mut self, worker: &mut super::snapshot::SnapshotWorker) {
        let Some(target) = self.send_attempt_retry_confirmation.take() else {
            return;
        };
        if self.selected_agent.as_ref() != Some(&target.agent_id) {
            self.status = "Conversation changed · inspect the message before retrying".into();
            return;
        }
        let current = self.workspace.try_lock().and_then(|mut workspace| {
            workspace
                .agents()
                .follow_up_queue_target(&target.agent_id)
                .map(|(_, thread_id, run_id)| (thread_id, run_id))
        });
        let Ok((thread_id, run_id)) = current else {
            self.status = "Could not verify the current conversation · retry was not sent".into();
            return;
        };
        let Some(snapshot) = self.follow_up_queue_snapshots.get_mut(&target.agent_id) else {
            self.status = "Conversation review is stale · reopen Attention before retrying".into();
            return;
        };
        if snapshot.thread_id != thread_id {
            self.status = "Conversation changed · reopen Attention before retrying".into();
            return;
        }
        snapshot.current_run_id = run_id;
        let retry = self
            .follow_up_queue_snapshots
            .get(&target.agent_id)
            .filter(|snapshot| snapshot.thread_id == target.thread_id)
            .and_then(|snapshot| {
                let attempt = snapshot.send_attempts.iter().find(|attempt| {
                    attempt.attempt_id == target.attempt_id
                        && retryable_send_attempt_status(attempt)
                        && attempt.status == target.status
                        && attempt.client_message_id == target.client_message_id
                        && attempt.mode == target.mode
                        && attempt.text == target.text
                })?;
                Some((attempt.clone(), snapshot.current_run_id.clone()))
            });
        let Some((attempt, current_run_id)) = retry else {
            self.send_attempt_retry_candidate = None;
            self.status = "This delivery changed before retry · inbox refreshed".into();
            self.refresh_attention_items();
            return;
        };
        let Some(agent_status) = self
            .workspace
            .try_lock()
            .ok()
            .and_then(|mut workspace| workspace.agents().snapshot(&target.agent_id).ok())
            .map(|snapshot| snapshot.status)
        else {
            self.status = "Could not verify the current conversation · retry was not sent".into();
            return;
        };
        if !matches!(
            agent_status,
            crate::AgentStatus::Idle | crate::AgentStatus::Working | crate::AgentStatus::Waiting
        ) || (attempt.mode == crate::thread_store::SendAttemptMode::Steer
            && agent_status != crate::AgentStatus::Working)
        {
            self.status =
                "Conversation state changed · retry was not sent; inspect it again".into();
            return;
        }
        if current_run_id.is_empty() {
            self.status =
                "No current run is available · resume this conversation before retrying".into();
            return;
        }
        let mode = match attempt.mode {
            crate::thread_store::SendAttemptMode::Prompt => "prompt",
            crate::thread_store::SendAttemptMode::FollowUp => "follow-up",
            crate::thread_store::SendAttemptMode::Steer => "steer",
        };
        let arguments = serde_json::json!({
            "agentId": target.agent_id.as_str(),
            "text": attempt.text.clone(),
            "mode": mode,
            "clientMessageId": attempt.client_message_id.clone(),
            "expectedRunId": current_run_id,
            "context": {
            "source": "send-attempt-retry",
                "previousAttemptId": attempt.attempt_id.clone(),
            "previousStatus": format!("{:?}", attempt.status).to_ascii_lowercase(),
            },
        });
        let (call, context) = match self.tool_request("glass.agent.send", arguments, true) {
            Ok(request) => request,
            Err(error) => {
                self.status =
                    format!("Retry not sent · previous attempt kept for review · {error}");
                return;
            }
        };
        match worker.submit_tool(call, context) {
            Ok(job_id) => {
                self.pending_chat_messages.push(PendingChatMessage {
                    agent_id: Some(target.agent_id.clone()),
                    message_id: attempt.client_message_id,
                    text: attempt.text,
                    state: ChatMessageState::Sending,
                    job_id: Some(job_id),
                    error: None,
                });
                self.agent_send_job = Some(job_id);
                self.send_attempt_retry_candidate = None;
                self.surface = DevSurface::Agent;
                self.status = "Retry submitted with the same message ID · a new delivery attempt is being tracked".into();
            }
            Err(error) => {
                self.status =
                    format!("Retry not sent · previous attempt kept for review · {error}");
            }
        }
    }

    pub fn move_attention_selection(&mut self, delta: i32) {
        if self.attention_items.is_empty() {
            self.attention_selection = 0;
            return;
        }
        let next = self.attention_selection as i32 + delta;
        self.attention_selection = next.clamp(0, self.attention_items.len() as i32 - 1) as usize;
    }

    pub fn activate_attention_selection(&mut self) {
        let Some(item) = self.attention_items.get(self.attention_selection).cloned() else {
            self.status = "No attention items need review".into();
            return;
        };
        match item.key {
            AttentionItemKey::EditorProposal { proposal_id, path } => {
                let pending = self.editor_proposals.iter().any(|proposal| {
                    proposal.id == proposal_id
                        && proposal.path == path
                        && proposal.state == crate::development::EditorProposalState::Pending
                });
                let workspace = match self.workspace.try_lock() {
                    Ok(workspace) => workspace,
                    Err(error) => {
                        self.status = format!(
                            "Cannot verify this review right now · {error} · press Enter to retry"
                        );
                        return;
                    }
                };
                let normalized_result = workspace.project().normalize_existing_file_path(&path);
                let normalized_path = match normalized_result {
                    Ok(path) => path,
                    Err(_) => {
                        drop(workspace);
                        self.attention_status_stale();
                        return;
                    }
                };
                drop(workspace);
                if !pending {
                    self.attention_status_stale();
                    return;
                }
                self.attention_inbox_open = false;
                self.selected_editor_proposal_id = Some(proposal_id.clone());
                let selected_file = self
                    .files
                    .iter()
                    .position(|file| file == &normalized_path)
                    .unwrap_or_else(|| {
                        self.files.push(normalized_path.clone());
                        self.files.len() - 1
                    });
                self.selected_file = selected_file;
                self.surface = DevSurface::Code;
                self.open_selected_file();
                self.refresh_editor_hunks();
                if self
                    .workspace
                    .try_lock()
                    .ok()
                    .is_some_and(|workspace| workspace.project().buffer(&normalized_path).is_some())
                {
                    self.status = format!(
                        "Review proposal {proposal_id} · {normalized_path} · accept or reject explicitly"
                    );
                }
            }
            AttentionItemKey::FollowUp {
                thread_id,
                entry_id,
            } => {
                let Some(owner) = item.owner_agent_id.clone() else {
                    self.attention_status_stale();
                    return;
                };
                let exists = self
                    .follow_up_queue_snapshots
                    .get(&owner)
                    .is_some_and(|snapshot| {
                        snapshot.thread_id == thread_id
                            && snapshot.entries.iter().any(|entry| {
                                entry.entry_id == entry_id
                                    && entry.state
                                        == crate::thread_store::FollowUpQueueState::NeedsReview
                            })
                    });
                if !exists || !self.refresh_follow_up_queue_run(&owner) {
                    self.attention_status_stale();
                    return;
                }
                if !self.select_agent(Some(owner.clone())) {
                    return;
                }
                let Some(snapshot) = self.follow_up_queue_snapshots.get(&owner) else {
                    self.attention_status_stale();
                    return;
                };
                if snapshot.thread_id != thread_id {
                    self.attention_status_stale();
                    return;
                }
                let Some(index) = snapshot.entries.iter().position(|entry| {
                    entry.entry_id == entry_id
                        && entry.state == crate::thread_store::FollowUpQueueState::NeedsReview
                }) else {
                    self.attention_status_stale();
                    return;
                };
                self.attention_inbox_open = false;
                self.follow_up_queue_open = true;
                self.follow_up_queue_retry_confirmation = None;
                self.follow_up_queue_selection = index;
                self.status = "Unresolved follow-up selected · review before retry".into();
            }
            AttentionItemKey::SendAttempt {
                thread_id,
                attempt_id,
            } => {
                let Some(owner) = item.owner_agent_id.clone() else {
                    self.attention_status_stale();
                    return;
                };
                let attempt = self
                    .follow_up_queue_snapshots
                    .get(&owner)
                    .filter(|snapshot| snapshot.thread_id == thread_id)
                    .and_then(|snapshot| {
                        snapshot.send_attempts.iter().find(|attempt| {
                            attempt.attempt_id == attempt_id
                                && retryable_send_attempt_status(attempt)
                        })
                    });
                let Some(attempt) = attempt else {
                    self.attention_status_stale();
                    return;
                };
                let attempt_status = attempt.status;
                let retry_target = SendAttemptRetryTarget {
                    agent_id: owner.clone(),
                    thread_id: thread_id.clone(),
                    attempt_id: attempt.attempt_id.clone(),
                    status: attempt.status,
                    client_message_id: attempt.client_message_id.clone(),
                    mode: attempt.mode,
                    text: attempt.text.clone(),
                    preview: attempt
                        .text
                        .chars()
                        .rev()
                        .take(120)
                        .collect::<Vec<_>>()
                        .into_iter()
                        .rev()
                        .collect(),
                };
                let detail = retry_target.preview.chars().take(96).collect::<String>();
                if !self.select_agent(Some(owner)) {
                    return;
                }
                self.send_attempt_retry_candidate = Some(retry_target);
                self.send_attempt_retry_confirmation = None;
                self.surface = DevSurface::Agent;
                self.attention_inbox_open = false;
                self.status = format!(
                    "{} · inspect it, then use Ctrl-K and choose Retry send · {detail}",
                    send_attempt_attention_label(attempt_status)
                );
            }
        }
    }

    fn attention_status_stale(&mut self) {
        self.refresh_attention_items();
        self.status = "That attention item is no longer active · inbox refreshed".into();
    }

    fn refresh_attention_items(&mut self) {
        let selected_key = self
            .attention_items
            .get(self.attention_selection)
            .map(|item| item.key.clone());
        let mut items = std::collections::BTreeMap::<AttentionItemKey, AttentionItem>::new();
        for proposal in &self.editor_proposals {
            if proposal.state != crate::development::EditorProposalState::Pending {
                continue;
            }
            let key = AttentionItemKey::EditorProposal {
                proposal_id: proposal.id.clone(),
                path: proposal.path.clone(),
            };
            items.entry(key.clone()).or_insert_with(|| AttentionItem {
                key,
                title: format!("Review · {}", proposal.summary),
                detail: format!("{} · {}", proposal.actor.name, proposal.path),
                owner_agent_id: None,
            });
        }
        for (agent_id, snapshot) in &self.follow_up_queue_snapshots {
            for entry in &snapshot.entries {
                if entry.state != crate::thread_store::FollowUpQueueState::NeedsReview {
                    continue;
                }
                let key = AttentionItemKey::FollowUp {
                    thread_id: snapshot.thread_id.clone(),
                    entry_id: entry.entry_id.clone(),
                };
                let preview = entry.text.lines().next().unwrap_or_default();
                items.entry(key.clone()).or_insert_with(|| AttentionItem {
                    key,
                    title: "Follow-up delivery needs review".into(),
                    detail: format!(
                        "{} · {}",
                        agent_id.as_str(),
                        preview.chars().take(72).collect::<String>()
                    ),
                    owner_agent_id: Some(agent_id.clone()),
                });
            }
            for attempt in &snapshot.send_attempts {
                if !retryable_send_attempt_status(attempt) {
                    continue;
                }
                let key = AttentionItemKey::SendAttempt {
                    thread_id: snapshot.thread_id.clone(),
                    attempt_id: attempt.attempt_id.clone(),
                };
                let preview = attempt.text.lines().next().unwrap_or_default();
                items.entry(key.clone()).or_insert_with(|| AttentionItem {
                    key,
                    title: send_attempt_attention_label(attempt.status).into(),
                    detail: format!(
                        "{} · {}",
                        agent_id.as_str(),
                        preview.chars().take(72).collect::<String>()
                    ),
                    owner_agent_id: Some(agent_id.clone()),
                });
            }
        }
        self.attention_items = items.into_values().collect();
        self.attention_selection = selected_key
            .and_then(|key| self.attention_items.iter().position(|item| item.key == key))
            .unwrap_or_else(|| {
                self.attention_selection
                    .min(self.attention_items.len().saturating_sub(1))
            });
    }

    pub(crate) fn follow_up_queue_view(
        &self,
    ) -> Option<(&str, &[crate::thread_store::FollowUpQueueRecord])> {
        self.selected_agent
            .as_ref()
            .and_then(|agent_id| self.follow_up_queue_snapshots.get(agent_id))
            .map(|snapshot| {
                (
                    snapshot.current_run_id.as_str(),
                    snapshot.entries.as_slice(),
                )
            })
    }

    pub fn follow_up_queue_is_editing(&self) -> bool {
        self.follow_up_queue_edit_mode.is_some()
    }

    pub fn poll_follow_up_queue_io(
        &mut self,
        _worker: &mut super::snapshot::SnapshotWorker,
    ) -> bool {
        #[cfg(unix)]
        let mut changed = false;
        #[cfg(not(unix))]
        let changed = false;
        #[cfg(unix)]
        {
            if let Some((generations, reply)) = self.attention_queue_load.take() {
                match reply.try_recv() {
                    Ok(Ok(queues)) => {
                        for (agent_id, snapshot) in queues {
                            let expected = generations.get(&agent_id).copied().unwrap_or_default();
                            let current = self
                                .follow_up_queue_generations
                                .get(&agent_id)
                                .copied()
                                .unwrap_or_default();
                            if expected == current {
                                self.follow_up_queue_snapshots.insert(agent_id, snapshot);
                            }
                        }
                        self.attention_queue_refresh_at =
                            Some(std::time::Instant::now() + std::time::Duration::from_secs(2));
                        changed = true;
                    }
                    Ok(Err(error)) => {
                        if self.attention_inbox_open {
                            self.status =
                                format!("Could not refresh follow-up review items: {error}");
                            changed = true;
                        }
                        self.attention_queue_refresh_at =
                            Some(std::time::Instant::now() + std::time::Duration::from_secs(2));
                    }
                    Err(std::sync::mpsc::TryRecvError::Empty) => {
                        self.attention_queue_load = Some((generations, reply));
                    }
                    Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                        if self.attention_inbox_open {
                            self.status = "Follow-up review worker disconnected".into();
                            changed = true;
                        }
                        self.attention_queue_refresh_at =
                            Some(std::time::Instant::now() + std::time::Duration::from_secs(2));
                    }
                }
            }
            if self.attention_inbox_open
                && self.attention_queue_load.is_none()
                && self
                    .attention_queue_refresh_at
                    .is_some_and(|deadline| deadline <= std::time::Instant::now())
            {
                self.attention_queue_refresh_at = None;
                let writer = self
                    .follow_up_queue_writer
                    .get_or_insert_with(FollowUpQueueWriter::spawn);
                match writer.load_inbox(self.workspace.clone()) {
                    Ok(reply) => {
                        self.attention_queue_load =
                            Some((self.follow_up_queue_generations.clone(), reply));
                    }
                    Err(error) => {
                        self.attention_queue_refresh_at =
                            Some(std::time::Instant::now() + std::time::Duration::from_secs(2));
                        self.status = format!("Could not refresh follow-up review items: {error}");
                        changed = true;
                    }
                }
            }
            let events = self
                .follow_up_queue_writer
                .as_ref()
                .map(|writer| std::iter::from_fn(|| writer.try_event()).collect::<Vec<_>>())
                .unwrap_or_default();
            for event in events {
                match event {
                    FollowUpQueueEvent::Saved(agent_id, generation) => {
                        if self.follow_up_queue_generations.get(&agent_id) == Some(&generation) {
                            changed = true;
                            if self.follow_up_queue_pending_dispatch.as_ref().is_some_and(
                                |(owner, _, pending_generation)| {
                                    owner == &agent_id && *pending_generation == generation
                                },
                            ) {
                                self.submit_follow_up_queue_dispatch(_worker);
                            }
                        }
                    }
                    FollowUpQueueEvent::SaveFailed(agent_id, generation, error) => {
                        if self.follow_up_queue_generations.get(&agent_id) == Some(&generation) {
                            clear_pending_queue_dispatch_if_matches(
                                &mut self.follow_up_queue_pending_dispatch,
                                &agent_id,
                                generation,
                            );
                            if let Some(snapshot) =
                                self.follow_up_queue_snapshots.get_mut(&agent_id)
                                && let Some(entry) = snapshot.entries.iter_mut().find(|entry| {
                                    entry.state
                                        == crate::thread_store::FollowUpQueueState::Dispatching
                                })
                            {
                                entry.state = crate::thread_store::FollowUpQueueState::Ready;
                            }
                            self.status = format!("Could not save follow-up queue: {error}");
                            changed = true;
                        }
                    }
                }
            }
            let owners = self
                .pending_follow_up_queue_loads
                .keys()
                .cloned()
                .collect::<Vec<_>>();
            for agent_id in owners {
                let Some((generation, reply)) =
                    self.pending_follow_up_queue_loads.remove(&agent_id)
                else {
                    continue;
                };
                match reply.try_recv() {
                    Ok(Ok(snapshot)) => {
                        let unchanged = self
                            .follow_up_queue_generations
                            .get(&agent_id)
                            .copied()
                            .unwrap_or_default()
                            == generation
                            && !self.follow_up_queue_snapshots.contains_key(&agent_id);
                        if unchanged {
                            self.follow_up_queue_snapshots
                                .insert(agent_id.clone(), snapshot);
                            if self.follow_up_queue_open
                                && self.selected_agent.as_ref() == Some(&agent_id)
                            {
                                self.status = "Thread queue loaded · persisted rows are never sent automatically".into();
                            }
                            changed = true;
                        }
                    }
                    Ok(Err(error)) => {
                        if self.follow_up_queue_open
                            && self.selected_agent.as_ref() == Some(&agent_id)
                        {
                            self.status = format!("Could not load follow-up queue: {error}");
                            changed = true;
                        }
                    }
                    Err(std::sync::mpsc::TryRecvError::Empty) => {
                        self.pending_follow_up_queue_loads
                            .insert(agent_id, (generation, reply));
                    }
                    Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                        if self.follow_up_queue_open
                            && self.selected_agent.as_ref() == Some(&agent_id)
                        {
                            self.status = "Follow-up queue worker disconnected".into();
                            changed = true;
                        }
                    }
                }
            }
        }
        if changed {
            self.refresh_attention_items();
        }
        changed
    }

    fn save_follow_up_queue(&mut self, agent_id: crate::AgentId) -> Option<u64> {
        let Some(snapshot) = self.follow_up_queue_snapshots.get(&agent_id).cloned() else {
            self.status = "Follow-up queue is still loading".into();
            return None;
        };
        let generation = self
            .follow_up_queue_generations
            .entry(agent_id.clone())
            .or_default();
        *generation = generation.wrapping_add(1);
        let generation = *generation;
        #[cfg(unix)]
        {
            let writer = self
                .follow_up_queue_writer
                .get_or_insert_with(FollowUpQueueWriter::spawn);
            if let Err(error) = writer.save(
                self.workspace.clone(),
                agent_id,
                snapshot.entries,
                generation,
            ) {
                self.status = format!("Could not queue follow-up queue save: {error}");
                return None;
            }
        }
        #[cfg(windows)]
        {
            let _ = snapshot;
            self.status = "Queue changes are process-local on Windows".into();
        }
        Some(generation)
    }

    pub fn move_follow_up_queue_selection(&mut self, delta: i32) {
        let Some(agent_id) = self.selected_agent.as_ref() else {
            return;
        };
        let count = self
            .follow_up_queue_snapshots
            .get(agent_id)
            .map_or(0, |snapshot| snapshot.entries.len());
        if count == 0 {
            return;
        }
        let next = self.follow_up_queue_selection as i32 + delta;
        self.follow_up_queue_selection = next.clamp(0, count as i32 - 1) as usize;
    }

    pub(crate) fn begin_follow_up_queue_add(
        &mut self,
        mode: crate::thread_store::FollowUpQueueMode,
    ) {
        let Some(agent_id) = self.selected_agent.clone() else {
            return;
        };
        if !self.refresh_follow_up_queue_run(&agent_id) {
            self.status = "Could not verify the current run destination".into();
            return;
        }
        let Some(snapshot) = self.follow_up_queue_snapshots.get(&agent_id) else {
            self.status = "Wait for the thread queue to load".into();
            return;
        };
        if snapshot.entries.len() >= 32 {
            self.status = "Thread queue reached its 32 item limit".into();
            return;
        }
        if mode == crate::thread_store::FollowUpQueueMode::Steer
            && !self.selected_agent_is_working(&agent_id)
        {
            self.status = "Steering needs an active run; choose a follow-up instead".into();
            return;
        }
        self.follow_up_queue_edit_text = self.composer_input.clone();
        self.follow_up_queue_edit_cursor = self.follow_up_queue_edit_text.len();
        self.follow_up_queue_edit_mode = Some(mode);
        self.follow_up_queue_edit_index = None;
        self.follow_up_queue_edit_target_run_id = Some(snapshot.current_run_id.clone());
    }

    pub fn begin_follow_up_queue_edit(&mut self) {
        let Some(agent_id) = self.selected_agent.as_ref() else {
            return;
        };
        let Some(entry) = self
            .follow_up_queue_snapshots
            .get(agent_id)
            .and_then(|snapshot| snapshot.entries.get(self.follow_up_queue_selection))
            .cloned()
        else {
            return;
        };
        self.follow_up_queue_edit_text = entry.text;
        self.follow_up_queue_edit_cursor = self.follow_up_queue_edit_text.len();
        self.follow_up_queue_edit_mode = Some(entry.mode);
        self.follow_up_queue_edit_target_run_id = Some(entry.target_run_id);
        self.follow_up_queue_edit_index = Some(self.follow_up_queue_selection);
    }

    pub fn follow_up_queue_edit_char(&mut self, character: char) {
        if self.follow_up_queue_edit_text.len() + character.len_utf8() > 16 * 1024
            || character == '\0'
        {
            self.status = "Follow-up text limit reached".into();
            return;
        }
        self.follow_up_queue_edit_text
            .insert(self.follow_up_queue_edit_cursor, character);
        self.follow_up_queue_edit_cursor += character.len_utf8();
    }

    pub fn follow_up_queue_edit_backspace(&mut self) {
        if self.follow_up_queue_edit_cursor == 0 {
            return;
        }
        let previous = self.follow_up_queue_edit_text[..self.follow_up_queue_edit_cursor]
            .char_indices()
            .next_back()
            .map(|(index, _)| index)
            .unwrap_or(0);
        self.follow_up_queue_edit_text
            .drain(previous..self.follow_up_queue_edit_cursor);
        self.follow_up_queue_edit_cursor = previous;
    }

    pub fn cancel_follow_up_queue_edit(&mut self) {
        self.follow_up_queue_edit_text.clear();
        self.follow_up_queue_edit_cursor = 0;
        self.follow_up_queue_edit_mode = None;
        self.follow_up_queue_edit_index = None;
        self.follow_up_queue_edit_target_run_id = None;
    }

    pub fn commit_follow_up_queue_edit(&mut self) {
        let Some(agent_id) = self.selected_agent.clone() else {
            return;
        };
        if !self.refresh_follow_up_queue_run(&agent_id) {
            self.status = "Could not verify the current run destination".into();
            return;
        }
        let text = self.follow_up_queue_edit_text.clone();
        let Some(mode) = self.follow_up_queue_edit_mode else {
            return;
        };
        let Some(target_run_id) = self.follow_up_queue_edit_target_run_id.clone() else {
            return;
        };
        if text.trim().is_empty() || text.contains('\0') || text.len() > 16 * 1024 {
            self.status = "Follow-up text must contain 1 to 16 KiB of text".into();
            return;
        }
        if mode == crate::thread_store::FollowUpQueueMode::Steer
            && (!self.selected_agent_is_working(&agent_id)
                || self
                    .follow_up_queue_snapshots
                    .get(&agent_id)
                    .is_none_or(|snapshot| snapshot.current_run_id != target_run_id))
        {
            self.status = "Steering destination must be this thread’s active run".into();
            return;
        }
        let Some(snapshot) = self.follow_up_queue_snapshots.get_mut(&agent_id) else {
            return;
        };
        if let Some(index) = self.follow_up_queue_edit_index {
            let Some(entry) = snapshot.entries.get_mut(index) else {
                return;
            };
            entry.text = text;
            entry.mode = mode;
            entry.target_run_id = target_run_id;
            if entry.state != crate::thread_store::FollowUpQueueState::Accepted {
                entry.state = crate::thread_store::FollowUpQueueState::Ready;
            }
        } else {
            snapshot
                .entries
                .push(crate::thread_store::FollowUpQueueRecord {
                    entry_id: match crate::thread_store::random_id("queue") {
                        Ok(id) => id,
                        Err(error) => {
                            self.status = format!("Could not create queue item: {error}");
                            return;
                        }
                    },
                    target_run_id,
                    mode,
                    text,
                    state: crate::thread_store::FollowUpQueueState::Ready,
                });
            self.follow_up_queue_selection = snapshot.entries.len().saturating_sub(1);
        }
        self.cancel_follow_up_queue_edit();
        if self.save_follow_up_queue(agent_id).is_some() {
            self.status = "Follow-up saved to this thread’s queue".into();
        }
    }

    pub fn remove_selected_follow_up_queue_entry(&mut self) {
        let Some(agent_id) = self.selected_agent.clone() else {
            return;
        };
        let Some(snapshot) = self.follow_up_queue_snapshots.get_mut(&agent_id) else {
            return;
        };
        if self.follow_up_queue_selection >= snapshot.entries.len() {
            return;
        }
        snapshot.entries.remove(self.follow_up_queue_selection);
        self.follow_up_queue_selection = self
            .follow_up_queue_selection
            .min(snapshot.entries.len().saturating_sub(1));
        if self.save_follow_up_queue(agent_id).is_some() {
            self.status = "Removed follow-up from this thread’s queue".into();
        }
    }

    pub fn reorder_selected_follow_up_queue_entry(&mut self, delta: i32) {
        let Some(agent_id) = self.selected_agent.clone() else {
            return;
        };
        let Some(snapshot) = self.follow_up_queue_snapshots.get_mut(&agent_id) else {
            return;
        };
        let next = self.follow_up_queue_selection as i32 + delta;
        if next < 0 || next as usize >= snapshot.entries.len() {
            return;
        }
        snapshot
            .entries
            .swap(self.follow_up_queue_selection, next as usize);
        self.follow_up_queue_selection = next as usize;
        let _ = self.save_follow_up_queue(agent_id);
    }

    pub fn retarget_selected_follow_up_queue_entry(&mut self) {
        let Some(agent_id) = self.selected_agent.clone() else {
            return;
        };
        if !self.refresh_follow_up_queue_run(&agent_id) {
            self.status = "Could not verify the current run destination".into();
            return;
        }
        let Some((run_id, mode)) =
            self.follow_up_queue_snapshots
                .get(&agent_id)
                .and_then(|snapshot| {
                    snapshot
                        .entries
                        .get(self.follow_up_queue_selection)
                        .map(|entry| (snapshot.current_run_id.clone(), entry.mode))
                })
        else {
            return;
        };
        if mode == crate::thread_store::FollowUpQueueMode::Steer
            && !self.selected_agent_is_working(&agent_id)
        {
            self.status = "Steering destination must be an active run".into();
            return;
        }
        let Some(entry) = self
            .follow_up_queue_snapshots
            .get_mut(&agent_id)
            .and_then(|snapshot| snapshot.entries.get_mut(self.follow_up_queue_selection))
        else {
            return;
        };
        entry.target_run_id = run_id;
        if entry.state != crate::thread_store::FollowUpQueueState::Accepted {
            entry.state = crate::thread_store::FollowUpQueueState::Ready;
        }
        let _ = self.save_follow_up_queue(agent_id);
    }

    pub fn dispatch_selected_follow_up_queue_entry(
        &mut self,
        _worker: &mut super::snapshot::SnapshotWorker,
    ) {
        let Some(agent_id) = self.selected_agent.clone() else {
            return;
        };
        if let Some(entry) = self
            .follow_up_queue_snapshots
            .get(&agent_id)
            .and_then(|snapshot| snapshot.entries.get(self.follow_up_queue_selection))
            .filter(|entry| entry.state == crate::thread_store::FollowUpQueueState::NeedsReview)
        {
            self.follow_up_queue_retry_confirmation = Some(entry.entry_id.clone());
            self.status = "Runtime acceptance is unknown; retry may duplicate this message. Press Enter again to confirm".into();
            return;
        }
        if !self.refresh_follow_up_queue_run(&agent_id) {
            self.status = "Could not verify the current run destination".into();
            return;
        }
        let Some((entry_id, target_run_id, current_run_id, mode, state)) = self
            .follow_up_queue_snapshots
            .get(&agent_id)
            .and_then(|snapshot| {
                snapshot
                    .entries
                    .get(self.follow_up_queue_selection)
                    .map(|entry| {
                        (
                            entry.entry_id.clone(),
                            entry.target_run_id.clone(),
                            snapshot.current_run_id.clone(),
                            entry.mode,
                            entry.state,
                        )
                    })
            })
        else {
            return;
        };
        if state != crate::thread_store::FollowUpQueueState::Ready {
            self.status = "Only ready or review-required rows can be sent".into();
            return;
        }
        if target_run_id != current_run_id {
            self.status = "This item targets an older run · press t to retarget explicitly".into();
            return;
        }
        if mode == crate::thread_store::FollowUpQueueMode::Steer
            && !self.selected_agent_is_working(&agent_id)
        {
            self.status = "Steering needs the explicitly targeted active run".into();
            return;
        }
        self.start_follow_up_queue_dispatch(agent_id, entry_id);
    }

    pub fn confirm_follow_up_queue_retry(&mut self) {
        let Some(entry_id) = self.follow_up_queue_retry_confirmation.take() else {
            return;
        };
        let Some(agent_id) = self.selected_agent.clone() else {
            return;
        };
        if !self.refresh_follow_up_queue_run(&agent_id) {
            self.status = "Could not verify the current run destination".into();
            return;
        }
        let target = self
            .follow_up_queue_snapshots
            .get(&agent_id)
            .and_then(|snapshot| {
                snapshot
                    .entries
                    .get(self.follow_up_queue_selection)
                    .filter(|entry| {
                        entry.entry_id == entry_id
                            && entry.state == crate::thread_store::FollowUpQueueState::NeedsReview
                    })
                    .map(|entry| {
                        (
                            entry.target_run_id.clone(),
                            entry.mode,
                            snapshot.current_run_id.clone(),
                        )
                    })
            });
        let Some((target_run_id, mode, current_run_id)) = target else {
            self.status = "Queue item changed; review it again before retrying".into();
            return;
        };
        if target_run_id != current_run_id {
            self.status = "This item targets an older run · retarget it before retrying".into();
            return;
        }
        if mode == crate::thread_store::FollowUpQueueMode::Steer
            && !self.selected_agent_is_working(&agent_id)
        {
            self.status = "Steering destination is no longer active".into();
            return;
        }
        self.start_follow_up_queue_dispatch(agent_id, entry_id);
    }

    pub fn cancel_follow_up_queue_retry(&mut self) {
        self.follow_up_queue_retry_confirmation = None;
    }

    fn start_follow_up_queue_dispatch(&mut self, agent_id: crate::AgentId, entry_id: String) {
        let Some(entry) = self
            .follow_up_queue_snapshots
            .get_mut(&agent_id)
            .and_then(|snapshot| {
                snapshot
                    .entries
                    .iter_mut()
                    .find(|entry| entry.entry_id == entry_id)
            })
        else {
            return;
        };
        entry.state = crate::thread_store::FollowUpQueueState::Dispatching;
        let Some(generation) = self.save_follow_up_queue(agent_id.clone()) else {
            return;
        };
        self.follow_up_queue_pending_dispatch = Some((agent_id, entry_id, generation));
        self.status = "Saving queue state before sending…".into();
    }

    #[cfg(unix)]
    fn submit_follow_up_queue_dispatch(&mut self, worker: &mut super::snapshot::SnapshotWorker) {
        let Some((agent_id, entry_id, _)) = self.follow_up_queue_pending_dispatch.take() else {
            return;
        };
        let Some(entry) = self
            .follow_up_queue_snapshots
            .get(&agent_id)
            .and_then(|snapshot| {
                snapshot
                    .entries
                    .iter()
                    .find(|entry| entry.entry_id == entry_id)
            })
            .cloned()
        else {
            return;
        };
        let mode = match entry.mode {
            crate::thread_store::FollowUpQueueMode::Steer => "steer",
            crate::thread_store::FollowUpQueueMode::FollowUp => "follow-up",
        };
        let arguments = serde_json::json!({
            "agentId": agent_id.as_str(),
            "text": entry.text,
            "mode": mode,
            "clientMessageId": entry.entry_id,
            "expectedRunId": entry.target_run_id,
            "context": {"source": "thread-follow-up-queue", "targetRunId": entry.target_run_id},
        });
        let (call, context) = match self.tool_request("glass.agent.send", arguments, true) {
            Ok(request) => request,
            Err(error) => {
                self.mark_queue_dispatch_failed(&agent_id, &entry_id, &error);
                return;
            }
        };
        match worker.submit_tool(call, context) {
            Ok(job_id) => {
                self.follow_up_queue_dispatch_jobs
                    .insert(job_id, (agent_id.clone(), entry_id.clone()));
                self.agent_send_job = Some(job_id);
                self.pending_chat_messages.push(PendingChatMessage {
                    agent_id: Some(agent_id),
                    message_id: entry_id,
                    text: entry.text,
                    state: ChatMessageState::Sending,
                    job_id: Some(job_id),
                    error: None,
                });
                self.status =
                    "Queue item sent to the resident runtime · awaiting acknowledgement".into();
            }
            Err(error) => self.mark_queue_dispatch_failed(&agent_id, &entry_id, &error),
        }
    }

    #[cfg(unix)]
    fn mark_queue_dispatch_failed(
        &mut self,
        agent_id: &crate::AgentId,
        entry_id: &str,
        error: &str,
    ) {
        if let Some(entry) = self
            .follow_up_queue_snapshots
            .get_mut(agent_id)
            .and_then(|snapshot| {
                snapshot
                    .entries
                    .iter_mut()
                    .find(|entry| entry.entry_id == entry_id)
            })
        {
            entry.state = crate::thread_store::FollowUpQueueState::NeedsReview;
        }
        self.status = format!("Queue delivery needs review · {error}");
        let _ = self.save_follow_up_queue(agent_id.clone());
    }

    fn selected_agent_is_working(&self, agent_id: &crate::AgentId) -> bool {
        match self.workspace.try_lock() {
            Ok(mut workspace) => workspace
                .agents()
                .snapshot(agent_id)
                .is_ok_and(|snapshot| snapshot.status == crate::AgentStatus::Working),
            Err(error) if Self::is_workspace_busy(&error) => self
                .follow_up_queue_snapshots
                .get(agent_id)
                .is_some_and(|snapshot| {
                    snapshot.send_attempts.iter().any(|attempt| {
                        attempt.target_run_id == snapshot.current_run_id
                            && attempt.status == crate::thread_store::SendAttemptStatus::Working
                    })
                }),
            Err(_) => false,
        }
    }

    fn refresh_follow_up_queue_run(&mut self, agent_id: &crate::AgentId) -> bool {
        let target = match self.workspace.try_lock() {
            Ok(mut workspace) => workspace
                .agents()
                .follow_up_queue_target(agent_id)
                .map(|(_, thread_id, run_id)| (thread_id, run_id)),
            Err(error) if Self::is_workspace_busy(&error) => {
                // Queue snapshots carry the last observed destination. It is
                // safe to use while the workspace is busy because every
                // eventual send includes this expected RunId and the tool
                // router revalidates it against the live agent before dispatch.
                return self
                    .follow_up_queue_snapshots
                    .get(agent_id)
                    .is_some_and(|snapshot| !snapshot.current_run_id.is_empty());
            }
            Err(_) => return false,
        };
        let Ok((thread_id, run_id)) = target else {
            return false;
        };
        let Some(snapshot) = self.follow_up_queue_snapshots.get_mut(agent_id) else {
            return false;
        };
        if snapshot.thread_id != thread_id {
            return false;
        }
        snapshot.current_run_id = run_id;
        true
    }

    /// Last complete snapshot of the selected Core worker state.
    pub fn selected_agent_status(&self) -> Option<crate::AgentStatus> {
        let agent_id = self.selected_agent.as_ref()?;
        self.agent_status_by_id.get(agent_id).copied()
    }

    fn select_agent(&mut self, selected: Option<crate::AgentId>) -> bool {
        if self.selected_agent == selected {
            return true;
        }
        self.gc_command_items.clear();
        self.gc_model_items.clear();
        self.gc_model_provider = None;
        self.gc_model_current = None;
        self.gc_model_error = None;
        self.gc_session_settings = None;
        self.gc_settings_view = false;
        self.gc_model_menu_open = false;
        self.gc_command_discovery_pending = false;
        if self.gc_command_menu_open {
            self.gc_command_loading = true;
            self.gc_command_discovery_requested = true;
        }
        self.composer_retry_message_id = None;
        let current = self.selected_agent.clone();
        #[cfg(unix)]
        {
            let current_load_pending = current.as_ref().is_some_and(|agent_id| {
                self.pending_composer_draft_loads.contains_key(agent_id)
                    && !self.thread_drafts.contains_key(&Some(agent_id.clone()))
            });
            if !current_load_pending {
                self.thread_drafts.insert(
                    current.clone(),
                    ComposerDraft {
                        text: self.composer_input.clone(),
                        cursor: self.composer_cursor,
                    },
                );
            }
            if (!current_load_pending && !self.persist_composer_draft())
                || self.composer_draft_save_failed
            {
                return false;
            }
        }
        #[cfg(not(unix))]
        self.thread_drafts.insert(
            current,
            ComposerDraft {
                text: self.composer_input.clone(),
                cursor: self.composer_cursor,
            },
        );
        self.selected_agent = selected.clone();
        self.reasoning_view_open = false;
        if let Some(conversation) = selected
            .as_ref()
            .and_then(|id| self.agent_conversations.get(id))
        {
            self.agent_conversation = conversation.text.clone();
            self.conversation_items = conversation.items.clone();
        } else {
            self.agent_conversation =
                "No conversation yet. Press Enter or start typing to compose a message.".into();
            self.conversation_items.clear();
        }
        let process_local_draft = self.thread_drafts.get(&selected).cloned();
        let draft = process_local_draft.clone().unwrap_or_default();
        self.composer_input = draft.text.clone();
        self.composer_cursor = draft.cursor.min(self.composer_input.len());
        #[cfg(unix)]
        {
            self.persisted_composer_draft =
                selected.clone().map(|agent_id| (agent_id, draft.clone()));
            if process_local_draft.is_none()
                && let Some(agent_id) = selected
                && !self.pending_composer_draft_loads.contains_key(&agent_id)
            {
                let writer = self
                    .composer_draft_writer
                    .get_or_insert_with(ComposerDraftWriter::spawn);
                match writer.load_agent(self.workspace.clone(), agent_id.clone()) {
                    Ok(reply) => {
                        let generation = self
                            .composer_draft_edit_generations
                            .get(&agent_id)
                            .copied()
                            .unwrap_or_default();
                        self.pending_composer_draft_loads
                            .insert(agent_id, (generation, reply));
                    }
                    Err(error) => {
                        self.composer_draft_save_failed = true;
                        self.composer_draft_failure = Some(error.clone());
                        self.status = format!("Could not queue composer draft load: {error}");
                    }
                }
            }
        }
        true
    }

    #[cfg(unix)]
    pub(super) fn persist_composer_draft(&mut self) -> bool {
        let Some(agent_id) = self.selected_agent.clone() else {
            return true;
        };
        let draft = ComposerDraft {
            text: self.composer_input.clone(),
            cursor: self.composer_cursor.min(self.composer_input.len()),
        };
        if self.persisted_composer_draft.as_ref() == Some(&(agent_id.clone(), draft.clone())) {
            if !self.pending_composer_draft_loads.contains_key(&agent_id) {
                self.thread_drafts.insert(Some(agent_id), draft);
            }
            return true;
        }
        let generation = self
            .composer_draft_edit_generations
            .entry(agent_id.clone())
            .or_default();
        *generation = generation.wrapping_add(1);
        let generation = *generation;
        self.thread_drafts
            .insert(Some(agent_id.clone()), draft.clone());
        let writer = self
            .composer_draft_writer
            .get_or_insert_with(ComposerDraftWriter::spawn);
        match writer.save_agent(
            self.workspace.clone(),
            agent_id.clone(),
            draft.clone(),
            generation,
        ) {
            Ok(()) => {
                self.composer_draft_unsaved_agents
                    .insert(agent_id.clone(), generation);
                self.persisted_composer_draft = Some((agent_id, draft));
                true
            }
            Err(error) => {
                self.composer_draft_unsaved_agents
                    .insert(agent_id.clone(), generation);
                self.composer_draft_failed_agents
                    .insert(agent_id.clone(), generation);
                self.composer_draft_discard_targets
                    .insert(agent_id, generation);
                self.composer_draft_save_failed = true;
                self.composer_draft_failure = Some(error.clone());
                self.status = format!("Could not queue composer draft: {error}");
                false
            }
        }
    }

    #[cfg(unix)]
    pub(crate) fn poll_composer_draft_io(&mut self) -> bool {
        let mut changed = false;
        if let Some(writer) = self.composer_draft_writer.as_ref() {
            while let Some(event) = writer.try_event() {
                match event {
                    ComposerDraftWriterEvent::SaveFailed(agent_id, generation, error) => {
                        if self.composer_draft_edit_generations.get(&agent_id) == Some(&generation)
                        {
                            self.composer_draft_unsaved_agents
                                .insert(agent_id.clone(), generation);
                            self.composer_draft_failed_agents
                                .insert(agent_id.clone(), generation);
                            self.composer_draft_save_failed = true;
                            self.composer_draft_failure = Some(error.clone());
                            self.status = format!("Could not save composer draft: {error}");
                            changed = true;
                        }
                    }
                    ComposerDraftWriterEvent::SaveSucceeded(agent_id, generation) => {
                        if self.composer_draft_edit_generations.get(&agent_id) == Some(&generation)
                        {
                            self.composer_draft_unsaved_agents.remove(&agent_id);
                            self.composer_draft_failed_agents.remove(&agent_id);
                        }
                        if !self.composer_draft_discard_pending {
                            self.composer_draft_save_failed =
                                !self.composer_draft_failed_agents.is_empty();
                            if !self.composer_draft_save_failed {
                                self.composer_draft_failure = None;
                            }
                        }
                    }
                    ComposerDraftWriterEvent::DiscardComplete(Ok(())) => {
                        self.composer_draft_discard_pending = false;
                        let discarded = std::mem::take(&mut self.composer_draft_discard_targets);
                        self.composer_draft_save_failed = false;
                        self.composer_draft_failure = None;
                        for (agent_id, generation) in &discarded {
                            if self
                                .composer_draft_failed_agents
                                .get(agent_id)
                                .is_some_and(|current| current <= generation)
                            {
                                self.composer_draft_failed_agents.remove(agent_id);
                            }
                        }
                        for (agent_id, generation) in &discarded {
                            if self
                                .composer_draft_unsaved_agents
                                .get(agent_id)
                                .is_some_and(|current| current <= generation)
                            {
                                self.composer_draft_unsaved_agents.remove(agent_id);
                            }
                        }
                        for (agent_id, _) in discarded {
                            self.thread_drafts.remove(&Some(agent_id));
                        }
                        self.composer_input.clear();
                        self.composer_cursor = 0;
                        self.quit_confirmation = false;
                        self.quit = true;
                        self.status = "Closing Glass Dev · unsaved composer draft discarded".into();
                        changed = true;
                    }
                    ComposerDraftWriterEvent::DiscardComplete(Err(error)) => {
                        self.composer_draft_discard_pending = false;
                        self.composer_draft_save_failed = true;
                        self.composer_draft_failure = Some(error.clone());
                        self.quit_confirmation = true;
                        self.status = format!(
                            "Could not clear composer draft: {error} · Enter retries · Esc keeps editing"
                        );
                        changed = true;
                    }
                }
            }
        }
        let pending_ids = self
            .pending_composer_draft_loads
            .keys()
            .cloned()
            .collect::<Vec<_>>();
        for agent_id in pending_ids {
            let Some((generation, reply)) = self.pending_composer_draft_loads.remove(&agent_id)
            else {
                continue;
            };
            match reply.try_recv() {
                Ok(Ok(draft)) => {
                    let current_generation = self
                        .composer_draft_edit_generations
                        .get(&agent_id)
                        .copied()
                        .unwrap_or_default();
                    if current_generation == generation
                        && !self.thread_drafts.contains_key(&Some(agent_id.clone()))
                    {
                        let draft = draft.unwrap_or_default();
                        self.thread_drafts
                            .insert(Some(agent_id.clone()), draft.clone());
                        if self.selected_agent.as_ref() == Some(&agent_id) {
                            self.composer_input = draft.text.clone();
                            self.composer_cursor = draft.cursor.min(self.composer_input.len());
                            self.persisted_composer_draft = Some((agent_id, draft));
                            changed = true;
                        }
                    }
                }
                Ok(Err(error)) => {
                    if self.selected_agent.as_ref() == Some(&agent_id) {
                        self.status = format!("Could not load composer draft: {error}");
                        changed = true;
                    }
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => {
                    self.pending_composer_draft_loads
                        .insert(agent_id, (generation, reply));
                }
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    if self.selected_agent.as_ref() == Some(&agent_id) {
                        self.status = "Could not load composer draft: worker disconnected".into();
                        changed = true;
                    }
                }
            }
        }
        changed
    }

    #[cfg(unix)]
    fn flush_composer_drafts(&mut self) -> bool {
        self.poll_composer_draft_io();
        let mut owners_before_flush = self.composer_draft_unsaved_agents.clone();
        for (owner, generation) in &self.composer_draft_failed_agents {
            owners_before_flush
                .entry(owner.clone())
                .and_modify(|current| *current = (*current).max(*generation))
                .or_insert(*generation);
        }
        let Some(writer) = self.composer_draft_writer.as_ref() else {
            if self.composer_draft_unsaved_agents.is_empty() {
                self.composer_draft_save_failed = false;
                return true;
            }
            self.composer_draft_save_failed = true;
            self.composer_draft_failure = Some("draft writer is unavailable".into());
            return false;
        };
        match writer.flush() {
            Ok(()) => {
                self.poll_composer_draft_io();
                self.composer_draft_save_failed = !self.composer_draft_failed_agents.is_empty();
                if !self.composer_draft_save_failed {
                    self.composer_draft_failure = None;
                }
                !self.composer_draft_save_failed
            }
            Err((error, unresolved)) => {
                self.poll_composer_draft_io();
                let owners = if unresolved {
                    owners_before_flush
                } else {
                    let mut owners = self.composer_draft_unsaved_agents.clone();
                    for (owner, generation) in &self.composer_draft_failed_agents {
                        owners
                            .entry(owner.clone())
                            .and_modify(|current| *current = (*current).max(*generation))
                            .or_insert(*generation);
                    }
                    owners
                };
                for (owner, generation) in owners {
                    self.composer_draft_discard_targets
                        .entry(owner)
                        .and_modify(|current| *current = (*current).max(generation))
                        .or_insert(generation);
                }
                self.composer_draft_save_failed = true;
                self.composer_draft_failure = Some(error.clone());
                self.status = format!("Could not save composer draft: {error}");
                false
            }
        }
    }

    pub fn open_composer(&mut self) {
        self.close_file_picker();
        self.composer_mode = true;
        self.composer_code_context_enabled =
            self.surface == DevSurface::Code && !self.focused_editor_path.is_empty();
        self.status = format!(
            "Message on {} · Esc back · Enter sends · Tab mention · Shift-Enter newline",
            self.surface.label(),
        );
    }

    /// Focus the shared chat dock without changing the active surface.
    pub fn focus_composer_dock(&mut self) {
        self.open_composer();
    }

    pub fn toggle_selected_git_stage(&mut self) {
        let Some(entry) = self.selected_git_entry().cloned() else {
            self.status = "Select a changed file · Space stages or unstages".into();
            return;
        };
        let staged = entry.index_status != ' ' && !entry.untracked;
        let tool = if staged {
            "glass.git.unstage"
        } else {
            "glass.git.stage"
        };
        let arguments = serde_json::json!({"paths": [entry.path]});
        match self.tool_request(tool, arguments, true) {
            Ok((call, context)) => {
                let summary = format!(
                    "{} {}",
                    if staged { "Unstage" } else { "Stage" },
                    self.selected_git_entry()
                        .map(|entry| entry.path.as_str())
                        .unwrap_or("path")
                );
                let _ = self.queue_or_confirm(call, context, summary);
            }
            Err(error) => self.status = format!("Git stage unavailable: {error}"),
        }
    }

    pub fn jump_to_app_keep_dock(&mut self) {
        self.surface = DevSurface::App;
        self.status = if self.composer_mode {
            "Browser · dock stays open · watch the agent or type a follow-up".into()
        } else {
            "Browser selected · Ctrl-L talks about this page".into()
        };
    }

    pub(crate) fn request_browser_visual_live(&mut self, live: bool, status: Option<String>) {
        self.browser_visual_live = live;
        self.pending_browser_visual_request = Some(BrowserVisualRequest {
            live,
            status,
            failure: None,
        });
    }

    pub(crate) fn request_browser_visual_failure(&mut self, reason: impl Into<String>) {
        let reason = reason.into();
        self.browser_visual_live = false;
        self.pending_browser_visual_request = Some(BrowserVisualRequest {
            live: false,
            status: None,
            failure: Some(reason),
        });
    }

    pub(crate) fn take_browser_visual_request(&mut self) -> Option<BrowserVisualRequest> {
        self.pending_browser_visual_request.take()
    }

    pub fn watch_agent_on_app(&mut self, tool: &str, value: &serde_json::Value) {
        self.browser_workspace.state_mut().input_owner =
            glass_browser::browser_workspace::BrowserInputOwner::Agent;
        self.request_browser_visual_live(true, None);
        let target = value
            .get("target")
            .or_else(|| value.get("name"))
            .and_then(serde_json::Value::as_str)
            .unwrap_or("page");
        let action = tool.rsplit('.').next().unwrap_or("act");
        self.status = format!("Agent {action} {target} · watching · Ctrl-L to steer");
    }

    pub fn comment_selected_app_entity(&mut self) {
        let Some(entity) = self.browser_workspace.state().selected().cloned() else {
            self.status = "No App entity selected · ↑/↓ then C comments".into();
            return;
        };
        self.last_app_comment = Some(format!("{} ({})", entity.name, entity.role));
        self.composer_input = format!(
            "About the selected Browser control [{}] {}: ",
            entity.role, entity.name
        );
        self.composer_cursor = self.composer_input.len();
        self.focus_composer_dock();
        self.status = format!(
            "App comment on {} · finish the note, then Enter",
            entity.name
        );
    }

    pub fn capture_plan_from_goal(&mut self, goal: &str) {
        self.pending_plan = Some(WorkspacePlan {
            id: format!(
                "plan-{}",
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|duration| duration.as_millis())
                    .unwrap_or(0)
            ),
            goal: goal.chars().take(240).collect(),
            body: String::new(),
            accepted: false,
        });
    }

    pub fn accept_pending_plan(&mut self, worker: &mut super::snapshot::SnapshotWorker) {
        let Some(mut plan) = self.pending_plan.clone() else {
            self.status = "No plan to accept · create a plan first".into();
            return;
        };
        if plan.body.is_empty()
            && let Some(entry) = self
                .conversation_entries_view()
                .into_iter()
                .rev()
                .find(|entry| entry.kind == super::projection::ConversationKind::Assistant)
        {
            plan.body = entry.text;
        }
        plan.accepted = true;
        self.persist_plan(&plan);
        if let Ok(list) = self.ws_mut().and_then(|mut workspace| {
            workspace
                .seed_todos_from_plan(&plan.goal, &plan.body)
                .map_err(|error| error.to_string())
        }) {
            self.session_todos = list;
        }
        self.pending_plan = Some(plan.clone());
        self.composer_input = format!(
            "Implement this accepted plan. Stay in proposals unless I say otherwise.\n\nGoal: {}\n\n{}",
            plan.goal, plan.body
        );
        self.composer_cursor = self.composer_input.len();
        self.open_composer();
        self.submit_composer(worker);
        self.status = format!("Plan {} accepted · implementation requested", plan.id);
    }

    pub fn reject_pending_plan(&mut self) {
        if let Some(plan) = self.pending_plan.as_mut() {
            plan.accepted = false;
            self.status = format!("Plan {} rejected · stay in Plan and revise", plan.id);
        } else {
            self.status = "No plan to reject".into();
        }
    }

    fn persist_plan(&self, plan: &WorkspacePlan) {
        let Ok(workspace) = self.ws() else {
            return;
        };
        let dir = workspace.root().join(".glass/plans");
        let _ = std::fs::create_dir_all(&dir);
        let _ = std::fs::write(
            dir.join("latest.json"),
            serde_json::to_vec_pretty(plan).unwrap_or_default(),
        );
    }

    /// Move from the native editor into the shared agent conversation with
    /// the focused buffer attached as unsaved, bounded context.
    pub fn prepare_editor_agent_prompt(&mut self) {
        let Some(buffer) = self.focused_buffer() else {
            self.status = "No open buffer · open a file before asking Glass Core".into();
            return;
        };
        let line = buffer
            .content
            .lines()
            .nth(buffer.cursor_line.saturating_sub(1) as usize)
            .unwrap_or_default()
            .trim();
        let line_preview = line.chars().take(240).collect::<String>();
        self.composer_input = format!(
            "Help me with {}:{}:{}.\nInspect the attached editor buffer and explain the safest minimal change. Current line: `{line_preview}`. Do not edit files until I approve a concrete proposal.",
            buffer.path, buffer.cursor_line, buffer.cursor_column
        );
        self.composer_cursor = self.composer_input.len();
        self.composer_steer = false;
        self.code_edit_mode = false;
        self.editor_exit_prompt = None;
        self.open_composer();
        self.status = format!(
            "Editor context attached · stay on Code · {}:{} · Enter sends",
            buffer.path, buffer.cursor_line
        );
    }

    pub fn review_object_text(&self) -> String {
        let proposals = self
            .editor_proposals
            .iter()
            .map(|item| {
                (
                    item.id.clone(),
                    item.path.clone(),
                    format!("{:?}", item.state),
                )
            })
            .collect::<Vec<_>>();
        let checkpoint = self
            .editor_checkpoints
            .last()
            .map(|item| item.name.as_str());
        let diff = if self.git_diff.starts_with("REVIEW") {
            None
        } else {
            Some(self.git_diff.as_str()).filter(|diff| !diff.trim().is_empty())
        };
        let wake = self.last_crew_wake.as_deref();
        let packed = wake.is_some_and(|wake| wake.starts_with("WAKE"));
        super::editor::review_object(
            &self.github.summary(),
            &self.github_review,
            &proposals,
            super::editor::ReviewEvidence {
                last_verify: if packed {
                    None
                } else {
                    self.last_verify.as_deref()
                },
                git_diff: if packed { None } else { diff },
                tasks: if packed {
                    None
                } else {
                    Some(self.tasks.as_str()).filter(|tasks| !tasks.trim().is_empty())
                },
                checkpoint: if packed { None } else { checkpoint },
                wake,
            },
        )
    }

    pub fn refresh_review_object(&mut self) {
        self.refresh_crew_wake_from_live();
        let text = self.review_object_text();
        self.editor = text.clone();
        self.git_diff = text;
        self.git_diff_path = Some("REVIEW".into());
        self.git_diff_open = true;
        self.status = "REVIEW · :review accept applies the pack · ship TITLE · ask".into();
    }

    pub fn accept_review_pack(&mut self) -> Result<String, String> {
        if !self.auto_checkpoint("before-review-accept") {
            return Err(self.status.clone());
        }
        match self.locked(|workspace| {
            workspace
                .project_mut()
                .accept_pending_editor_proposals(crate::development::Actor::local())
        }) {
            Some(Ok(buffers)) => {
                self.refresh_editor_hunks();
                self.refresh_editor_projection();
                self.refresh_review_object();
                Ok(format!(
                    "Accepted {} proposal(s) from the wake pack",
                    buffers.len()
                ))
            }
            Some(Err(crate::development::DevelopmentError::NotFound(_))) => {
                Err("No pending proposals in the wake pack".into())
            }
            Some(Err(error)) => Err(error.to_string()),
            None => Err("Accept failed · workspace busy".into()),
        }
    }

    fn queue_editor_acceptance(&mut self, id: String) -> Result<String, String> {
        if self.pending_editor_acceptances.len() >= 8 {
            return Err("Accept failed · proposal queue is full".into());
        }
        if !self.pending_editor_acceptances.contains(&id) {
            self.pending_editor_acceptances.push_back(id.clone());
        }
        self.status = "Proposal acceptance queued · waiting for the workspace".into();
        Ok(format!(
            "Queued acceptance for proposal {id} · workspace busy"
        ))
    }

    fn flush_pending_editor_acceptance(&mut self) -> bool {
        let Some(id) = self.pending_editor_acceptances.pop_front() else {
            return false;
        };
        match self.accept_review_proposal(Some(&id)) {
            Ok(message) if message.starts_with("Queued acceptance for proposal ") => false,
            Ok(message) => {
                self.status = message;
                true
            }
            Err(error) => {
                self.status = format!("Proposal acceptance failed · {error}");
                true
            }
        }
    }

    pub fn accept_review_proposal(&mut self, id: Option<&str>) -> Result<String, String> {
        let id = self.resolve_review_proposal_id(id)?;
        if !self.auto_checkpoint("before-review-accept") {
            return Err(self.status.clone());
        }
        match self.locked(|workspace| {
            workspace
                .project_mut()
                .accept_editor_proposal(&id, crate::development::Actor::local())
                .map(|_| ())
        }) {
            Some(Ok(())) => {
                self.refresh_editor_hunks();
                self.refresh_editor_projection();
                self.refresh_review_object();
                Ok(format!("Accepted proposal {id}"))
            }
            Some(Err(error)) if Self::is_workspace_busy(&error) => self.queue_editor_acceptance(id),
            Some(Err(error)) => Err(error.to_string()),
            None => self.queue_editor_acceptance(id),
        }
    }

    pub fn reject_review_proposal(&mut self, id: Option<&str>) -> Result<String, String> {
        let id = self.resolve_review_proposal_id(id)?;
        match self.locked(|workspace| {
            workspace
                .project_mut()
                .reject_editor_proposal(&id, crate::development::Actor::local())
                .map(|_| ())
        }) {
            Some(Ok(())) => {
                self.refresh_editor_hunks();
                self.refresh_review_object();
                Ok(format!("Rejected proposal {id}"))
            }
            Some(Err(error)) => Err(error.to_string()),
            None => Err("Reject failed · workspace busy".into()),
        }
    }

    fn resolve_review_proposal_id(&self, id: Option<&str>) -> Result<String, String> {
        if let Some(id) = id {
            return Ok(id.to_string());
        }
        self.editor_proposals
            .iter()
            .find(|item| item.state == crate::development::EditorProposalState::Pending)
            .map(|item| item.id.clone())
            .ok_or_else(|| "No pending editor proposal".into())
    }

    fn refresh_crew_wake_from_live(&mut self) {
        let accept = self
            .editor_proposals
            .iter()
            .find(|proposal| proposal.state == crate::development::EditorProposalState::Pending)
            .map(|proposal| proposal.id.clone());
        let browser = self.browser_workspace.state();
        let page = if browser.url.is_empty() && browser.selected().is_none() {
            None
        } else {
            Some(self.page_evidence())
        };
        let live = crate::CrewWakeLiveEvidence {
            verify: self.last_verify.clone(),
            page,
            accept,
        };
        if let Some(Ok(Some(wake))) = self.locked(|workspace| workspace.refresh_crew_wake(live)) {
            self.last_crew_wake = Some(wake.render());
        }
    }

    fn page_evidence(&self) -> String {
        let browser = self.browser_workspace.state();
        let url = safe_browser_url(&browser.url).unwrap_or_else(|| "—".into());
        let entity = browser
            .selected()
            .map(|entity| entity.reference.as_str())
            .unwrap_or("—");
        format!(
            "url {url}\n  entity {entity}\n  revision {}",
            browser.browser_revision.unwrap_or(0)
        )
    }

    /// Prepare an editable review prompt using the current workspace evidence.
    pub fn prepare_review_prompt(&mut self) {
        self.composer_input = "Review the current workspace changes. Inspect the Git diff, changed files, diagnostics, and latest test results. Report concrete correctness, security, regression, and missing-test risks. Do not edit files until I approve a fix.".into();
        self.composer_cursor = self.composer_input.len();
        self.composer_steer = false;
        self.open_composer();
        self.status =
            "Review prompt ready · edit it, then press Enter to ask the Glass Agent".into();
    }

    pub fn toggle_composer_steer(&mut self) {
        self.composer_steer = !self.composer_steer;
        self.status = if self.composer_steer {
            "Steer mode · Enter interrupts the running agent".into()
        } else {
            "Follow-up mode · Enter sends during or after the current turn".into()
        };
    }

    /// Refresh the readiness of the sole resident engine.
    pub fn request_agent_setup(&mut self) {
        self.surface = DevSurface::Agent;
        match self.refresh_agent_readiness() {
            Ok(true) => self.status = "Glass Core is ready · start a conversation".into(),
            Ok(false) => {
                let detail = self.agent_readiness.clone();
                self.status = format!("Glass Core needs setup · {detail}");
            }
            Err(error) => self.status = format!("Glass Core readiness check failed: {error}"),
        }
    }

    pub fn request_agent_update(&mut self) {
        self.request_agent_setup();
    }

    pub fn request_agent_login(&mut self) -> Result<(), String> {
        if self.background_action_running() {
            let error = "Finish the current background action before signing in".to_string();
            self.status = error.clone();
            return Err(error);
        }
        self.agent_login_requested = true;
        self.surface = DevSurface::Agent;
        self.status = "Core login will open in this terminal · complete the provider sign-in, then exit to return".into();
        Ok(())
    }

    pub fn request_agent_command_ui(&mut self) -> Result<(), String> {
        if self.background_action_running() {
            let error =
                "Finish the current background action before opening Glass Core".to_string();
            self.status = error.clone();
            return Err(error);
        }
        self.agent_command_ui_requested = true;
        self.surface = DevSurface::Agent;
        self.status =
            "Glass Core will open in this terminal · press : in GC to open its command menu".into();
        Ok(())
    }

    /// Open the composer-integrated GC command catalogue. If the project has
    /// no resident session yet, create an idle Core session before discovery;
    /// this initializes the session protocol but does not send provider input.
    pub fn open_gc_command_menu(&mut self, worker: &mut super::snapshot::SnapshotWorker) {
        self.surface = DevSurface::Agent;
        self.composer_mode = true;
        self.gc_command_menu_open = true;
        self.gc_command_query.clear();
        self.gc_command_selected = 0;
        let catalogue_cached = !self.gc_command_items.is_empty();
        self.gc_command_loading = !catalogue_cached;
        self.gc_command_discovery_requested = false;
        self.composer_gc_command_completion = false;
        self.composer_glass_action_completion = None;
        self.status = "Loading commands from Glass Core…".into();
        if catalogue_cached
            || self.gc_command_discovery_pending
            || !self.gc_command_discovery_jobs.is_empty()
        {
            return;
        }
        let mut arguments = serde_json::json!({});
        if let Some(agent) = self.selected_agent.as_ref() {
            arguments["agentId"] = serde_json::Value::String(agent.as_str().to_string());
        }
        self.queue_gc_command_tool(worker, "glass.agent.commands", arguments, true);
    }

    fn queue_gc_command_tool(
        &mut self,
        worker: &mut super::snapshot::SnapshotWorker,
        tool: &str,
        arguments: serde_json::Value,
        palette_discovery: bool,
    ) -> bool {
        // Selecting a provider model in this menu is a direct human control
        // action. It changes Core-owned session configuration; the Core API
        // validates and applies it without a second Glass approval prompt.
        let user_initiated_core_config = tool == "glass.agent.model";
        match self.tool_request(tool, arguments, user_initiated_core_config) {
            Ok((call, context)) => match worker.submit_tool(call, context) {
                Ok(job) => {
                    self.running_tool_job = Some(job);
                    if palette_discovery && tool == "glass.agent.models" {
                        self.gc_model_discovery_jobs.insert(job);
                    } else if palette_discovery {
                        self.gc_command_discovery_jobs.insert(job);
                        self.gc_command_discovery_pending = true;
                        if self.gc_command_menu_open {
                            self.gc_command_loading = true;
                        }
                    }
                    true
                }
                Err(error) => {
                    self.gc_command_loading = false;
                    self.status = format!("Could not load GC commands: {error}");
                    false
                }
            },
            Err(error) => {
                self.gc_command_loading = false;
                self.status = format!("Could not load GC commands: {error}");
                false
            }
        }
    }

    /// Finish deferred command discovery once the idle Core worker is ready.
    pub fn tick_gc_command_discovery(&mut self, worker: &mut super::snapshot::SnapshotWorker) {
        if !self.gc_command_menu_open || !self.gc_command_discovery_requested {
            return;
        }
        let Some(agent) = self.selected_agent.clone() else {
            return;
        };
        let Some(status) = self.agent_status_by_id.get(&agent).copied() else {
            return;
        };
        if matches!(
            status,
            crate::AgentStatus::Queued
                | crate::AgentStatus::Starting
                | crate::AgentStatus::RecoveryPending
                | crate::AgentStatus::Unavailable
        ) {
            return;
        }
        self.gc_command_discovery_requested = false;
        self.queue_gc_command_tool(
            worker,
            "glass.agent.commands",
            serde_json::json!({"agentId": agent.as_str()}),
            true,
        );
    }

    pub fn insert_gc_command_query(&mut self, character: char) {
        self.gc_command_query.push(character);
        self.gc_command_selected = 0;
        self.composer_gc_command_completion = false;
        self.composer_glass_action_completion = None;
    }

    pub fn backspace_gc_command_query(&mut self) {
        if let Some((index, _)) = self.gc_command_query.char_indices().next_back() {
            self.gc_command_query.truncate(index);
        }
        self.gc_command_selected = 0;
    }

    /// Remove the opening slash and return to the unchanged composer draft.
    pub fn dismiss_gc_command_menu_trigger(&mut self) {
        self.gc_command_menu_open = false;
        self.gc_command_loading = false;
        self.gc_command_discovery_requested = false;
        self.composer_gc_command_completion = false;
        self.composer_glass_action_completion = None;
        self.gc_command_query.clear();
        self.gc_command_selected = 0;
        self.status = "Command menu closed · draft unchanged".into();
    }

    pub fn select_gc_command_first(&mut self) {
        self.gc_command_selected = 0;
    }

    pub fn select_gc_command_last(&mut self) {
        let count = self.gc_palette_item_count();
        self.gc_command_selected = count.saturating_sub(1);
    }

    pub fn move_gc_command_selection(&mut self, delta: i32) {
        let total = self.gc_palette_item_count();
        if total == 0 {
            self.gc_command_selected = 0;
            return;
        }
        self.gc_command_selected =
            (self.gc_command_selected as i32 + delta).rem_euclid(total as i32) as usize;
    }

    pub fn gc_command_glass_actions(&self) -> Vec<command::SurfaceAction> {
        let raw_query = self.gc_command_query.trim().to_ascii_lowercase();
        let query = raw_query
            .strip_prefix("glass")
            .map(str::trim)
            .unwrap_or(raw_query.as_str());
        let mut actions = Vec::new();
        for surface in DevSurface::ALL {
            for action in command::surface_actions(surface) {
                if actions
                    .iter()
                    .any(|known: &command::SurfaceAction| known.command == action.command)
                {
                    continue;
                }
                let searchable =
                    format!("{} {} {}", action.label, action.command, action.description)
                        .to_ascii_lowercase();
                if query.is_empty()
                    && [
                        "view Agent",
                        "view Code",
                        "view Browser",
                        "view Git",
                        "agent status",
                        "agent settings",
                        "agent models [AGENT_ID]",
                        "agent sessions",
                        "agent new",
                        "task list",
                    ]
                    .contains(&action.command)
                    || !query.is_empty()
                        && query
                            .split_whitespace()
                            .all(|term| searchable.contains(term))
                {
                    actions.push(*action);
                }
            }
        }
        actions
    }

    /// Keep the empty-query palette focused on common controls. Typing a query
    /// searches the full GC catalogue; this is a view preference only and does
    /// not hide or alter Core capabilities.
    pub fn gc_palette_core_commands(&self) -> Vec<&serde_json::Value> {
        let matches =
            super::command::filter_gc_commands(&self.gc_command_items, &self.gc_command_query);
        if !self.gc_command_query.trim().is_empty() {
            return matches;
        }
        // Keep the first palette view limited to controls Glass can execute
        // in-place today. The full GC-owned catalogue remains searchable;
        // interactive-only commands are labelled unsupported on selection
        // until the runtime exposes the corresponding structured API.
        const PRIMARY: &[&str] = &["status", "model", "models", "help", "compact"];
        matches
            .into_iter()
            .filter(|item| {
                item.get("name")
                    .and_then(serde_json::Value::as_str)
                    .is_some_and(|name| PRIMARY.contains(&name))
            })
            .collect()
    }

    fn gc_palette_item_count(&self) -> usize {
        let core_count = if self.gc_model_menu_open {
            self.gc_model_items.len()
        } else {
            self.gc_palette_core_commands().len()
        };
        core_count
            + if self.gc_model_menu_open {
                0
            } else {
                self.gc_command_glass_actions().len()
            }
    }

    pub fn gc_command_selection_is_glass(&self) -> bool {
        if self.gc_model_menu_open {
            return false;
        }
        let core_count = self.gc_palette_core_commands().len();
        self.gc_command_selected >= core_count
    }

    pub fn complete_gc_command_selection(&mut self) {
        if self.gc_model_menu_open {
            if let Some(model) = self.gc_model_items.get(self.gc_command_selected)
                && let (Some(provider), Some(id)) = (
                    model
                        .get("provider")
                        .and_then(serde_json::Value::as_str)
                        .or(self.gc_model_provider.as_deref()),
                    model.get("id").and_then(serde_json::Value::as_str),
                )
            {
                self.composer_input = format!("/model {provider} {id}");
                self.composer_cursor = self.composer_input.len();
                self.gc_command_menu_open = false;
                self.gc_model_menu_open = false;
                self.gc_command_query.clear();
                self.composer_gc_command_completion = true;
            }
            return;
        }
        let glass_actions = self.gc_command_glass_actions();
        let core_matches = self.gc_palette_core_commands();
        if let Some(action) = glass_actions
            .get(self.gc_command_selected.saturating_sub(core_matches.len()))
            .filter(|_| self.gc_command_selected >= core_matches.len())
        {
            self.composer_input = format!("/glass {}", action.command);
            self.composer_cursor = self.composer_input.len();
            self.composer_gc_command_completion = false;
            self.composer_glass_action_completion = Some(action.command.to_string());
            self.gc_command_menu_open = false;
            self.gc_model_menu_open = false;
            self.gc_command_query.clear();
            return;
        }
        let Some(item) = core_matches.get(self.gc_command_selected).cloned() else {
            self.status = "No command to complete · keep typing or Esc to treat it as text".into();
            return;
        };
        let Some(name) = item.get("name").and_then(serde_json::Value::as_str) else {
            return;
        };
        let name = name.trim_start_matches('/');
        let placeholder = item
            .get("argumentsHint")
            .or_else(|| item.get("arguments"))
            .and_then(serde_json::Value::as_str)
            .map(|hint| format!(" {hint}"))
            .unwrap_or_else(|| " ".into());
        self.composer_input = format!("/{name}{placeholder}");
        self.composer_cursor = self.composer_input.len();
        self.composer_gc_command_completion = true;
        self.gc_command_menu_open = false;
        self.gc_model_menu_open = false;
        self.gc_command_query.clear();
    }

    pub fn cancel_gc_command_menu(&mut self) {
        if self.gc_model_menu_open {
            self.gc_model_menu_open = false;
            self.gc_settings_view = false;
            self.gc_command_selected = 0;
            self.gc_command_loading = false;
            self.status = "Glass Core commands · choose a control or Esc to close".into();
            return;
        }
        self.composer_input = format!("/{}", self.gc_command_query);
        self.composer_cursor = self.composer_input.len();
        self.gc_command_menu_open = false;
        self.gc_model_menu_open = false;
        self.gc_command_loading = false;
        self.gc_command_discovery_requested = false;
        self.composer_gc_command_completion = false;
        self.composer_glass_action_completion = None;
        self.gc_command_query.clear();
        self.gc_command_selected = 0;
        self.status = "Command menu closed · draft unchanged".into();
    }

    pub fn submit_gc_command_selection(&mut self, worker: &mut super::snapshot::SnapshotWorker) {
        if self.gc_command_loading {
            self.status = "Glass Core command catalogue is still loading".into();
            return;
        }
        let glass_actions = self.gc_command_glass_actions();
        if self.gc_model_menu_open {
            let Some(model) = self.gc_model_items.get(self.gc_command_selected) else {
                self.status = self
                    .gc_model_error
                    .clone()
                    .unwrap_or_else(|| "No models returned by Glass Core".into());
                return;
            };
            let Some(agent) = self.selected_agent.as_ref() else {
                self.status = "Select a Glass Core session before changing its model".into();
                return;
            };
            let provider = model
                .get("provider")
                .and_then(serde_json::Value::as_str)
                .or(self.gc_model_provider.as_deref())
                .map(str::to_owned);
            let Some(provider) = provider else {
                self.status = "Glass Core model catalogue omitted its provider".into();
                return;
            };
            let Some(model_id) = model
                .get("id")
                .and_then(serde_json::Value::as_str)
                .map(str::to_owned)
            else {
                self.status = "Glass Core returned a model without an ID".into();
                return;
            };
            self.gc_model_menu_open = false;
            self.gc_command_menu_open = false;
            self.gc_settings_view = false;
            self.gc_command_query.clear();
            self.composer_gc_command_completion = false;
            self.queue_gc_command_tool(worker, "glass.agent.model", serde_json::json!({"agentId":agent.as_str(),"provider":provider,"modelId":model_id}), false);
            self.status =
                format!("Asking Glass Core to use {provider}/{model_id} for this session");
            return;
        }
        let core_command_count = self.gc_palette_core_commands().len();
        if let Some(action) = glass_actions
            .get(self.gc_command_selected.saturating_sub(core_command_count))
            .copied()
            .filter(|_| self.gc_command_selected >= core_command_count)
        {
            self.gc_command_menu_open = false;
            self.gc_command_query.clear();
            self.composer_gc_command_completion = false;
            if action.command == "agent settings" {
                self.open_gc_model_settings(worker);
                return;
            }
            self.composer_mode = false;
            self.status = match command::execute(self, action.command) {
                Ok(message) => message,
                Err(error) => error,
            };
            if self.surface == DevSurface::Agent {
                self.composer_mode = true;
            }
            return;
        }
        let Some(item) = self
            .gc_palette_core_commands()
            .get(self.gc_command_selected)
            .map(|value| (*value).clone())
        else {
            self.status = "No matching Glass Core command".into();
            return;
        };
        let Some(name) = item
            .get("name")
            .and_then(serde_json::Value::as_str)
            .map(str::to_owned)
        else {
            self.status = "Glass Core returned a command without a name".into();
            return;
        };
        let name = name.trim_start_matches('/').to_owned();
        let query = self.gc_command_query.trim().trim_start_matches('/');
        let arguments = query.strip_prefix(&name).unwrap_or("").trim().to_string();
        let interactive =
            item.get("runtimeMode").and_then(serde_json::Value::as_str) == Some("interactive");
        let disposition = item
            .get("presentationKind")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("unsupported");
        self.gc_command_menu_open = false;
        self.composer_gc_command_completion = false;
        if interactive {
            if disposition == "picker" && name == "model" {
                self.gc_model_menu_open = true;
                self.gc_settings_view = false;
                self.gc_command_menu_open = true;
                self.gc_command_selected = 0;
                self.gc_model_error = None;
                if self.gc_model_items.is_empty() {
                    self.gc_command_loading = true;
                    self.status = "Loading provider models from Glass Core…".into();
                    let mut args = serde_json::json!({});
                    if let Some(agent) = self.selected_agent.as_ref() {
                        args["agentId"] = serde_json::Value::String(agent.as_str().into());
                    }
                    self.queue_gc_command_tool(worker, "glass.agent.models", args, true);
                } else {
                    self.gc_command_loading = false;
                    self.status =
                        "Choose a provider model · Enter applies it to this Core session".into();
                }
            } else if disposition == "picker" && name == "settings" {
                self.open_gc_model_settings(worker);
            } else {
                self.gc_command_query.clear();
                self.gc_command_menu_open = false;
                self.gc_model_menu_open = false;
                if let Some(owner) = self.selected_agent.clone() {
                    let description = item
                        .get("description")
                        .and_then(serde_json::Value::as_str)
                        .unwrap_or("Core command");
                    self.open_native_gc_ui_notice(
                        owner,
                        &name,
                        "Not available in this Glass workspace",
                        &format!("/{name} · {description}\n\nNo action was run. This Core command does not have an in-workspace control yet."),
                        disposition,
                    );
                } else {
                    self.status = format!(
                        "/{name} is not available in this Glass workspace · no action was run"
                    );
                }
            }
            return;
        }
        let Some(agent) = self.selected_agent.clone() else {
            self.status = "Select a Glass Core session before running this command".into();
            return;
        };
        let queued = self.queue_gc_command_tool(
            worker,
            "glass.agent.command",
            serde_json::json!({"agentId":agent.as_str(),"name":name,"arguments":arguments}),
            false,
        );
        if queued {
            self.open_native_gc_ui_pending(agent, &name);
        }
        self.gc_command_query.clear();
    }

    fn open_gc_model_settings(&mut self, worker: &mut super::snapshot::SnapshotWorker) {
        self.gc_model_menu_open = true;
        self.gc_command_menu_open = true;
        self.gc_settings_view = true;
        self.gc_command_selected = 0;
        self.gc_model_error = None;
        self.gc_command_query.clear();
        if let Some(agent) = self.selected_agent.as_ref() {
            match self.tool_request(
                "glass.agent.hello",
                serde_json::json!({"agentId":agent.as_str()}),
                false,
            ) {
                Ok((call, context)) => match worker.submit_tool(call, context) {
                    Ok(job) => {
                        self.gc_settings_jobs.insert(job);
                        self.running_tool_job = Some(job);
                    }
                    Err(error) => self.status = format!("Could not request Core settings: {error}"),
                },
                Err(error) => self.status = format!("Could not request Core settings: {error}"),
            }
        }
        if self.gc_model_items.is_empty() {
            self.gc_command_loading = true;
            self.status = "Loading provider settings from Glass Core…".into();
            let mut args = serde_json::json!({});
            if let Some(agent) = self.selected_agent.as_ref() {
                args["agentId"] = serde_json::Value::String(agent.as_str().into());
            }
            self.queue_gc_command_tool(worker, "glass.agent.models", args, true);
        } else {
            self.gc_command_loading = false;
            self.status = "Core provider settings · Enter applies a selected model".into();
        }
    }

    pub fn close_composer(&mut self) {
        if self.surface == DevSurface::Agent {
            self.composer_mode = true;
            self.composer_gc_command_completion = false;
            self.status = "Conversation ready · draft preserved".into();
        } else {
            self.composer_mode = false;
            self.composer_steer = false;
            self.status = "Composer closed · surface focus restored".into();
        }
    }

    pub fn move_composer_cursor(&mut self, right: bool) {
        if right {
            self.composer_cursor = self.composer_input[self.composer_cursor..]
                .char_indices()
                .nth(1)
                .map(|(offset, _)| self.composer_cursor + offset)
                .unwrap_or(self.composer_input.len());
        } else if self.composer_cursor > 0 {
            self.composer_cursor = self.composer_input[..self.composer_cursor]
                .char_indices()
                .next_back()
                .map(|(index, _)| index)
                .unwrap_or(0);
        }
    }

    pub fn delete_composer_word(&mut self) {
        let before = self.composer_input[..self.composer_cursor].trim_end();
        let start = before.len();
        let target = before[..start]
            .trim_end_matches(|c: char| c.is_whitespace())
            .char_indices()
            .rev()
            .find_map(|(index, character)| character.is_whitespace().then_some(index))
            .map(|index| before[..index].trim_end().len())
            .unwrap_or(0);
        self.composer_input.drain(target..self.composer_cursor);
        self.composer_cursor = target;
    }

    pub fn abort_selected_agent(&mut self, worker: &mut super::snapshot::SnapshotWorker) {
        let Some(agent) = self.selected_agent.clone() else {
            self.status = "No active agent to abort".into();
            return;
        };
        let (call, context) = match self.tool_request(
            "glass.agent.abort",
            serde_json::json!({"agentId": agent.as_str()}),
            true,
        ) {
            Ok(request) => request,
            Err(error) => {
                self.status = format!("Abort unavailable: {error}");
                return;
            }
        };
        match worker.submit_tool(call, context) {
            Ok(id) => {
                self.running_tool_job = Some(id);
                self.status = format!("Stopping {} in background…", agent.as_str());
            }
            Err(error) => self.status = format!("Abort unavailable: {error}"),
        }
    }

    pub fn insert_composer_newline(&mut self) {
        self.insert_composer_text("\n");
        self.status = "Shift-Enter newline · Enter send · ↑ history".into();
    }

    pub fn navigate_composer_history(&mut self, previous: bool) {
        if previous && !self.composer_on_first_line() {
            self.move_composer_line(-1);
            return;
        }
        if !previous && !self.composer_on_last_line() {
            self.move_composer_line(1);
            return;
        }
        if self.composer_history.is_empty() {
            return;
        }
        if previous {
            let index = match self.composer_history_index {
                None => {
                    self.composer_history_draft = self.composer_input.clone();
                    self.composer_history.len().saturating_sub(1)
                }
                Some(0) => 0,
                Some(index) => index - 1,
            };
            self.composer_history_index = Some(index);
            self.composer_input = self.composer_history[index].clone();
            self.composer_cursor = self.composer_input.len();
            return;
        }
        match self.composer_history_index {
            None => {}
            Some(index) if index + 1 < self.composer_history.len() => {
                self.composer_history_index = Some(index + 1);
                self.composer_input = self.composer_history[index + 1].clone();
                self.composer_cursor = self.composer_input.len();
            }
            Some(_) => {
                self.composer_history_index = None;
                self.composer_input = std::mem::take(&mut self.composer_history_draft);
                self.composer_cursor = self.composer_input.len();
            }
        }
    }

    fn composer_on_first_line(&self) -> bool {
        !self.composer_input[..self.composer_cursor.min(self.composer_input.len())].contains('\n')
    }

    fn composer_on_last_line(&self) -> bool {
        !self.composer_input[self.composer_cursor.min(self.composer_input.len())..].contains('\n')
    }

    fn move_composer_line(&mut self, delta: i32) {
        let cursor = self.composer_cursor.min(self.composer_input.len());
        if delta < 0 {
            if let Some(offset) = self.composer_input[..cursor].rfind('\n') {
                self.composer_cursor = offset;
            }
            return;
        }
        if let Some(relative) = self.composer_input[cursor..].find('\n') {
            self.composer_cursor = (cursor + relative + 1).min(self.composer_input.len());
        }
    }

    pub fn remember_composer_history(&mut self, text: &str) {
        let trimmed = text.trim();
        if trimmed.is_empty() {
            return;
        }
        if self.composer_history.last().map(String::as_str) != Some(trimmed) {
            self.composer_history.push(text.to_string());
            if self.composer_history.len() > 64 {
                self.composer_history.remove(0);
            }
        }
        self.composer_history_index = None;
        self.composer_history_draft.clear();
    }

    pub fn open_file_picker(&mut self) {
        self.command_mode = false;
        self.file_picker_open = true;
        self.file_picker_query.clear();
        self.file_picker_cursor = 0;
        self.file_picker_selection = 0;
        self.status = if self.files.is_empty() {
            "Open file · no files yet · wait for refresh or Esc close".into()
        } else {
            format!(
                "Open file · {} paths · type to filter · Enter open · Esc close",
                self.files.len()
            )
        };
    }

    pub fn close_file_picker(&mut self) {
        if !self.file_picker_open {
            return;
        }
        self.file_picker_open = false;
        self.file_picker_query.clear();
        self.file_picker_cursor = 0;
        self.file_picker_selection = 0;
        self.status = "File picker closed · Ctrl-P to reopen".into();
    }

    pub fn file_picker_matches(&self) -> Vec<usize> {
        self.files
            .iter()
            .enumerate()
            .filter_map(|(index, path)| {
                fuzzy_contains(path, self.file_picker_query.trim()).then_some(index)
            })
            .collect()
    }

    pub fn insert_file_picker_char(&mut self, character: char) {
        if character.is_control() {
            return;
        }
        self.file_picker_query
            .insert(self.file_picker_cursor, character);
        self.file_picker_cursor += character.len_utf8();
        self.file_picker_selection = 0;
    }

    pub fn file_picker_backspace(&mut self) {
        if self.file_picker_cursor == 0 {
            return;
        }
        let previous = self.file_picker_query[..self.file_picker_cursor]
            .char_indices()
            .next_back()
            .map(|(index, _)| index)
            .unwrap_or(0);
        self.file_picker_query
            .drain(previous..self.file_picker_cursor);
        self.file_picker_cursor = previous;
        self.file_picker_selection = 0;
    }

    pub fn move_file_picker_selection(&mut self, delta: i32) {
        let count = self.file_picker_matches().len();
        if count == 0 {
            self.file_picker_selection = 0;
            return;
        }
        self.file_picker_selection =
            (self.file_picker_selection as i32 + delta).rem_euclid(count as i32) as usize;
    }

    pub fn submit_file_picker(&mut self) {
        let matches = self.file_picker_matches();
        let Some(&index) = matches.get(self.file_picker_selection) else {
            self.status = "No matching file".into();
            return;
        };
        self.selected_file = index;
        self.close_file_picker();
        self.open_selected_file_for_edit();
    }

    pub fn open_path(&mut self, path: &str) -> Result<String, String> {
        if path.trim().is_empty() {
            return Err("open requires PATH".into());
        }
        if let Some(index) = self.files.iter().position(|item| {
            item == path || item.ends_with(path) || item.rsplit('/').next() == Some(path)
        }) {
            self.selected_file = index;
        } else {
            self.files.insert(0, path.to_string());
            self.selected_file = 0;
        }
        self.open_selected_file_for_edit();
        Ok(format!("Opened {path}"))
    }

    pub fn insert_composer_text(&mut self, text: &str) {
        let remaining = COMPOSER_INPUT_LIMIT.saturating_sub(self.composer_input.len());
        let mut accepted = String::new();
        let mut truncated = false;
        for character in text.chars() {
            if accepted.len() + character.len_utf8() > remaining {
                truncated = true;
                break;
            }
            accepted.push(character);
        }
        self.composer_input
            .insert_str(self.composer_cursor, &accepted);
        self.composer_cursor += accepted.len();
        if !accepted.is_empty() {
            self.composer_retry_message_id = None;
        }
        if truncated {
            self.status = "Composer input limit reached (16 KiB)".into();
        }
    }

    pub fn complete_composer_mention(&mut self) {
        match complete_mention(&self.composer_input, self.composer_cursor, &self.files) {
            Some((text, cursor)) => {
                if text != self.composer_input {
                    self.composer_retry_message_id = None;
                }
                self.composer_input = text;
                self.composer_cursor = cursor;
                self.status = "Mention completed · Tab cycles @file".into();
            }
            None => {
                self.status = "Mentions: @file @page @entity @workflow @workspace".into();
            }
        }
    }

    pub fn composer_backspace(&mut self) {
        if self.composer_cursor == 0 {
            return;
        }
        let previous = self.composer_input[..self.composer_cursor]
            .char_indices()
            .next_back()
            .map(|(index, _)| index)
            .unwrap_or(0);
        self.composer_input.drain(previous..self.composer_cursor);
        self.composer_cursor = previous;
        self.composer_retry_message_id = None;
    }

    /// Apply a composer edit and clear the restored-message retry identity.
    /// Editing turns the draft into a new logical message even if the user
    /// later types the same text again.
    pub fn set_composer_edit(&mut self, text: String, cursor: usize) {
        if text != self.composer_input {
            self.composer_retry_message_id = None;
        }
        self.composer_input = text;
        self.composer_cursor = cursor;
    }

    pub fn submit_composer(&mut self, worker: &mut super::snapshot::SnapshotWorker) {
        if let Some(default_command) = self.composer_glass_action_completion.take() {
            let command = self
                .composer_input
                .strip_prefix("/glass ")
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .unwrap_or(default_command.as_str())
                .to_string();
            if command == "agent settings" {
                self.composer_input.clear();
                self.composer_cursor = 0;
                self.open_gc_model_settings(worker);
                return;
            }
            self.composer_mode = false;
            self.status = match command::execute(self, &command) {
                Ok(message) => message,
                Err(error) => error,
            };
            if self.surface == DevSurface::Agent {
                self.composer_mode = true;
            }
            self.composer_input.clear();
            self.composer_cursor = 0;
            return;
        }
        if self.composer_gc_command_completion {
            let command_exists =
                !super::command::filter_gc_commands(&self.gc_command_items, &self.composer_input)
                    .is_empty();
            if command_exists {
                self.gc_command_query = self.composer_input.clone();
                self.gc_command_menu_open = true;
                self.submit_gc_command_selection(worker);
                self.composer_input.clear();
                self.composer_cursor = 0;
            } else {
                self.status =
                    "No matching Core command · draft kept; Esc then Enter sends it as text".into();
            }
            return;
        }
        let prompt = self.composer_input.clone();
        self.submit_composer_prompt(&prompt, true, worker);
    }

    fn matching_composer_retry_message_id(&self, text: &str) -> Option<String> {
        let message_id = self.composer_retry_message_id.as_ref()?;
        self.pending_chat_messages
            .iter()
            .find(|message| {
                message.message_id == *message_id
                    && message.agent_id == self.selected_agent
                    && message.state == ChatMessageState::Failed
                    && message.text == text
            })
            .map(|message| message.message_id.clone())
    }

    fn submit_composer_prompt(
        &mut self,
        prompt: &str,
        consume_draft: bool,
        worker: &mut super::snapshot::SnapshotWorker,
    ) -> bool {
        self.submit_composer_prompt_for_harness(
            prompt,
            consume_draft,
            worker,
            crate::ResidentHarnessKind::default_kind(),
        )
    }

    fn submit_composer_prompt_for_harness(
        &mut self,
        prompt: &str,
        consume_draft: bool,
        worker: &mut super::snapshot::SnapshotWorker,
        _harness: crate::ResidentHarnessKind,
    ) -> bool {
        if self.composer_send_blocked() {
            self.status = "Background operation running · message kept in composer".into();
            return false;
        }
        if matches!(
            self.selected_agent_status(),
            Some(crate::AgentStatus::RecoveryPending | crate::AgentStatus::Unavailable)
        ) {
            self.status = "This thread needs recovery · message kept in composer · resume it or choose another thread".into();
            return false;
        }
        // Agent sends are accepted while Core is starting; the resident worker
        // queues the first prompt until the session is ready. Readiness text is
        // a snapshot and can lag behind the command channel during startup.
        if prompt.trim().is_empty() {
            self.status = "Message is empty · type a prompt, then press Enter".into();
            return false;
        }
        let file = if !self.focused_editor_path.is_empty() {
            Some(self.focused_editor_path.as_str())
        } else {
            self.files.get(self.selected_file).map(String::as_str)
        };
        let display_text = expand_mentions(prompt, file);
        let text = display_text.clone();
        if consume_draft {
            self.composer_input.clear();
            self.composer_cursor = 0;
        }
        self.remember_composer_history(&text);
        let prove = compile_prove_it(&text);
        let steer = self.composer_steer;
        let queued_follow_up = self.pending_chat_messages.iter().any(|message| {
            message.state == ChatMessageState::Sending && message.agent_id == self.selected_agent
        });
        self.composer_steer = false;
        self.composer_mode = true;
        // A normal message is just the text the user typed. Attach browser
        // state only for an explicit prove-it request; never serialize the
        // selected editor buffer, path, or unrelated project metadata into a
        // greeting or ordinary conversation turn.
        let mut context = if prove.is_some() {
            self.agent_prompt_context()
        } else {
            serde_json::Value::Null
        };
        if self.surface == DevSurface::Code
            && let Some(code) = self.code_prompt_context()
        {
            if context.is_null() {
                context = serde_json::json!({"schemaVersion":"glass.agent-context.v1"});
            }
            context["code"] = code;
        }
        let owner = self.selected_agent.clone();
        let retry_message_id = self.matching_composer_retry_message_id(&display_text);
        if self.composer_retry_message_id.is_some() && retry_message_id.is_none() {
            self.composer_retry_message_id = None;
        }
        let message_id = retry_message_id.clone().unwrap_or_else(|| {
            format!(
                "chat-{}-{}",
                std::process::id(),
                NEXT_CHAT_MESSAGE.fetch_add(1, Ordering::Relaxed)
            )
        });
        let mut arguments = serde_json::json!({
            "text": text,
            "mode": if steer { "steer" } else { "follow-up" },
            "clientMessageId": message_id.clone(),
        });
        if !context.is_null() {
            arguments["context"] = context;
        }
        if let Some(prove) = prove {
            if !self.auto_checkpoint("before-prove-it") {
                if consume_draft {
                    self.composer_input = display_text;
                    self.composer_cursor = self.composer_input.len();
                }
                return false;
            }
            arguments["verify"] = prove.verify.clone();
            arguments["intent"] = serde_json::Value::String(prove.intent);
            self.pending_verify = Some(prove.verify);
            self.last_proof_ok = None;
            self.last_verify = Some(evidence_card(
                arguments["context"]["browser"]["url"]
                    .as_str()
                    .unwrap_or("about:blank"),
                arguments["context"]["browser"]["revision"]
                    .as_u64()
                    .unwrap_or(0),
                arguments["context"]["browser"]["selectedEntity"].as_str(),
                "composer prove-it queued",
                false,
            ));
        }
        if let Some(agent) = self.selected_agent.as_ref() {
            arguments["agentId"] = serde_json::Value::String(agent.as_str().to_string());
        }
        let (call, context) = match self.tool_request("glass.agent.send", arguments, true) {
            Ok(request) => request,
            Err(error) => {
                if consume_draft {
                    self.composer_input = display_text;
                    self.composer_cursor = self.composer_input.len();
                }
                self.status = format!("Message unavailable · edit and retry: {error}");
                return false;
            }
        };
        match worker.submit_tool(call, context) {
            Ok(id) => {
                self.agent_send_job = Some(id);
                if let Some(retry_message_id) = retry_message_id.as_deref() {
                    if let Some(message) = self
                        .pending_chat_messages
                        .iter_mut()
                        .find(|message| message.message_id == retry_message_id)
                    {
                        message.agent_id = owner;
                        message.text = display_text;
                        message.state = ChatMessageState::Sending;
                        message.job_id = Some(id);
                        message.error = None;
                    }
                } else {
                    self.pending_chat_messages.push(PendingChatMessage {
                        agent_id: owner,
                        message_id,
                        text: display_text,
                        state: ChatMessageState::Sending,
                        job_id: Some(id),
                        error: None,
                    });
                }
                self.composer_retry_message_id = None;
                self.status = if steer {
                    "Sent · steering Glass Agent…".into()
                } else if queued_follow_up {
                    "Queued follow-up · Glass Agent will continue".into()
                } else {
                    "Sent · Glass Core is thinking…".into()
                };
                worker.request_conversation();
                true
            }
            Err(error) => {
                if consume_draft {
                    self.composer_input = display_text;
                    self.composer_cursor = self.composer_input.len();
                }
                self.status = format!("Message unavailable · edit and retry: {error}");
                false
            }
        }
    }

    fn agent_prompt_context(&mut self) -> serde_json::Value {
        let state = self.browser_workspace.state();
        let selected_entity = state.selected().map(|entity| {
            serde_json::json!({
                "reference": entity.reference,
                "role": entity.role,
                "name": entity.name,
                "actionable": entity.actionable,
                "revision": entity.revision,
            })
        });
        let selected_target = state
            .targets
            .iter()
            .find(|target| target.selected)
            .map(|target| {
                serde_json::json!({
                    "id": target.id,
                    "title": target.title,
                    "url": safe_browser_url(&target.url),
                })
            });
        let context = serde_json::json!({
            "schemaVersion": "glass.agent-context.v1",
            "browser": {
                "attached": state.connection == glass_browser::browser_workspace::BrowserConnectionPhase::Connected,
                "connection": serde_json::to_value(state.connection).unwrap_or(serde_json::Value::Null),
                "targetId": selected_target.as_ref().and_then(|target| target.get("id")).cloned(),
                "origin": page_origin(&state.url),
                "url": safe_browser_url(&state.url),
                "title": state.title,
                "browserRevision": state.browser_revision,
                "semanticSummary": self.browser_workspace_summary(),
                "selectedEntity": selected_entity,
                "workflowState": state.workflow,
                "inputOwner": serde_json::to_value(state.input_owner).unwrap_or(serde_json::Value::Null),
                "freshness": if state.semantic_invalidated { "stale" } else { "current" },
                "target": selected_target,
                "bootstrap": if state.connection == glass_browser::browser_workspace::BrowserConnectionPhase::Connected {
                    "attached; observe before acting"
                } else {
                    "detached; call glass.browser.start, then navigate and observe the requested URL"
                },
            },
            "authority": {
                "source": "Glass browser workspace",
                "mutationLeaseRequired": selected_entity.as_ref().is_some_and(|entity| {
                    entity.get("actionable").and_then(serde_json::Value::as_bool).unwrap_or(false)
                }),
            }
        });
        if context["browser"]["selectedEntity"].is_object() {
            self.browser_workspace.state_mut().input_owner =
                glass_browser::browser_workspace::BrowserInputOwner::Agent;
        }
        context
    }

    fn code_prompt_context(&self) -> Option<serde_json::Value> {
        let mut sources = self.code_context_attachments.clone();
        if self.composer_code_context_enabled
            && !self.focused_editor_path.is_empty()
            && !sources
                .iter()
                .any(|attachment| attachment.path == self.focused_editor_path)
        {
            sources.push(CodeContextAttachment {
                path: self.focused_editor_path.clone(),
                selection: self.focused_editor_selection.clone(),
                content: self.focused_editor_content.clone(),
                dirty: self.focused_editor_dirty,
            });
        }
        if sources.is_empty() {
            return None;
        }

        // Leave room for the typed metadata and path/range descriptions within
        // the resident agent's hard 64 KiB context-attachment limit.
        let mut remaining = MAX_CODE_PROMPT_CONTEXT_BYTES.saturating_sub(16 * 1024);
        let mut files = Vec::with_capacity(sources.len());
        for source_file in sources {
            let full_content = source_file.content.as_str();
            let selection = source_file.selection.as_ref().and_then(|selection| {
                crate::development::editor::selection_offsets(full_content, selection)
            });
            let (source, selected_only) = selection
                .filter(|(start, end)| start < end)
                .map(|(start, end)| (&full_content[start..end], true))
                .unwrap_or((full_content, false));
            let mut content_end = source.len().min(remaining);
            while !source.is_char_boundary(content_end) {
                content_end = content_end.saturating_sub(1);
            }
            let truncated = content_end < source.len();
            let content = &source[..content_end];
            remaining = remaining.saturating_sub(content.len());
            let language = super::file_view::classify(&source_file.path).label();
            files.push(serde_json::json!({
                "path": source_file.path.clone(),
                "language": language,
                "unsaved": source_file.dirty,
                "selectedOnly": selected_only,
                "selection": source_file.selection,
                "truncated": truncated,
                "content": content,
            }));
        }
        if files.len() == 1 {
            files.into_iter().next()
        } else {
            Some(serde_json::json!({
                "schemaVersion": "glass.code-context.v1",
                "files": files,
            }))
        }
    }

    pub fn approve_confirmation_async(&mut self, worker: &mut super::snapshot::SnapshotWorker) {
        let Some(pending) = self.pending_confirmation.take() else {
            return;
        };
        match worker.submit_tool(pending.call, pending.context) {
            Ok(id) => {
                self.running_tool_job = Some(id);
                self.status = format!(
                    "Running {} · Esc keeps the workspace responsive",
                    pending.summary
                );
            }
            Err(error) => {
                self.status = format!("Could not queue mutation: {error}");
            }
        }
    }

    pub fn apply_visual_job_result(&mut self, result: super::snapshot::VisualJobResult) {
        self.apply_visual_job_result_with_fit(
            result,
            glass_browser::terminal_graphics::FrameFit::Contain,
        );
    }

    pub fn apply_visual_job_result_with_fit(
        &mut self,
        result: super::snapshot::VisualJobResult,
        fit: glass_browser::terminal_graphics::FrameFit,
    ) {
        let png = match result.result {
            Ok(value) => {
                let Some(encoded) = value.get("base64").and_then(serde_json::Value::as_str) else {
                    self.note_browser_visual_failure(
                        "screenshot payload did not contain base64 PNG data",
                        "Live view unavailable · screenshot payload was empty",
                    );
                    return;
                };
                use base64::Engine as _;
                match base64::engine::general_purpose::STANDARD.decode(encoded) {
                    Ok(png) => png,
                    Err(error) => {
                        self.note_browser_visual_failure(
                            format!("screenshot payload was not valid base64: {error}"),
                            "Live view unavailable · invalid screenshot payload",
                        );
                        return;
                    }
                }
            }
            Err(error) => {
                self.note_browser_visual_failure(
                    format!("browser screenshot failed: {error}"),
                    format!("Live view unavailable · {error}"),
                );
                return;
            }
        };
        match AnsiPane::from_png(
            &mut self.browser_ansi,
            &png,
            result.columns.clamp(8, 80),
            result.rows.clamp(4, 40),
            fit,
        ) {
            Ok(pane) => {
                self.browser_pane = Some(pane);
                let browser = self.browser_workspace.state_mut();
                browser.presentation =
                    glass_browser::browser_workspace::BrowserPresentationPath::Ansi;
                browser.frame_revision = browser.browser_revision;
                browser.presentation_reason = Some(
                    "bounded ANSI half-block frame · semantic controls remain authoritative".into(),
                );
                self.status = "Live view updated · ANSI half-block".into();
            }
            Err(error) => {
                self.note_browser_visual_failure(
                    error.to_string(),
                    format!("Live view unavailable: {error}"),
                );
            }
        }
    }

    fn note_browser_visual_failure(
        &mut self,
        reason: impl Into<String>,
        status: impl Into<String>,
    ) {
        let reason = reason.into();
        self.request_browser_visual_failure(reason.clone());
        self.browser_workspace.state_mut().presentation_reason = Some(reason);
        self.status = status.into();
    }

    pub fn queue_tool_request(
        &mut self,
        call: crate::development::ToolCall,
        context: crate::tools::DevelopmentToolContext,
    ) -> Result<(), String> {
        if self.background_action_running() {
            return Err("another background tool is already running".into());
        }
        self.queued_tool_request = Some((call, context));
        Ok(())
    }
    pub(super) fn queue_or_confirm(
        &mut self,
        call: crate::development::ToolCall,
        context: crate::tools::DevelopmentToolContext,
        summary: String,
    ) -> Result<bool, String> {
        self.pending_confirmation = Some(PendingConfirmation {
            call,
            context,
            summary,
        });
        Ok(false)
    }

    pub fn submit_pending_verify(&mut self, worker: &mut super::snapshot::SnapshotWorker) {
        let Some(predicate) = self.pending_verify.take() else {
            return;
        };
        let arguments = serde_json::json!({ "predicate": predicate, "timeoutSeconds": 8 });
        let (call, context) = match self.tool_request("glass.browser.verify", arguments, false) {
            Ok(request) => request,
            Err(error) => {
                self.last_proof_ok = Some(false);
                self.status = format!("Prove-it unavailable: {error}");
                return;
            }
        };
        match worker.submit_tool(call, context) {
            Ok(id) => {
                self.running_tool_job = Some(id);
                self.status = "Prove-it running against the live page".into();
            }
            Err(error) => {
                self.last_proof_ok = Some(false);
                self.status = format!("Prove-it failed to start: {error}");
            }
        }
    }

    pub fn submit_queued_tool(&mut self, worker: &mut super::snapshot::SnapshotWorker) {
        let Some((call, context)) = self.queued_tool_request.take() else {
            return;
        };
        match worker.submit_tool(call, context) {
            Ok(id) => {
                self.running_tool_job = Some(id);
                self.status.push_str(" · running in background");
            }
            Err(error) => {
                self.status = format!("Could not queue tool: {error}");
            }
        }
    }

    pub fn apply_tool_job_result(&mut self, result: super::snapshot::ToolJobResult) {
        if self.gc_settings_jobs.remove(&result.id) {
            if self.running_tool_job == Some(result.id) {
                self.running_tool_job = None;
            }
            if let Err(error) = result.result {
                self.status = format!("Could not read Core session settings: {error}");
            }
            return;
        }
        if result.tool == "glass.agent.send"
            && let Some((agent_id, entry_id)) =
                self.follow_up_queue_dispatch_jobs.remove(&result.id)
        {
            let accepted = matches!(
                &result.result,
                Ok(value)
                    if value.get("queued").and_then(serde_json::Value::as_bool) == Some(true)
            );
            let error = result.result.as_ref().err().map(ToString::to_string);
            if let Some(entry) =
                self.follow_up_queue_snapshots
                    .get_mut(&agent_id)
                    .and_then(|snapshot| {
                        snapshot
                            .entries
                            .iter_mut()
                            .find(|entry| entry.entry_id == entry_id)
                    })
            {
                entry.state = if accepted {
                    crate::thread_store::FollowUpQueueState::Accepted
                } else {
                    crate::thread_store::FollowUpQueueState::NeedsReview
                };
            }
            if let Some(message) = self
                .pending_chat_messages
                .iter_mut()
                .find(|message| message.job_id == Some(result.id))
            {
                message.state = if accepted {
                    ChatMessageState::Sent
                } else {
                    ChatMessageState::Failed
                };
                message.job_id = None;
                message.error.clone_from(&error);
            }
            self.agent_send_job = self
                .pending_chat_messages
                .iter()
                .rev()
                .find(|message| message.state == ChatMessageState::Sending)
                .and_then(|message| message.job_id);
            self.status = if accepted {
                "Runtime acknowledged the queued follow-up".into()
            } else {
                format!(
                    "Follow-up delivery is unresolved · {} · review before retry",
                    error.unwrap_or_else(|| "runtime did not confirm acceptance".into())
                )
            };
            let _ = self.save_follow_up_queue(agent_id);
            self.refresh_attention_items();
            return;
        }
        let palette_discovery = self.gc_command_discovery_jobs.remove(&result.id);
        if palette_discovery {
            if self.running_tool_job == Some(result.id) {
                self.running_tool_job = None;
            }
            match result.result {
                Ok(value)
                    if value
                        .get("initializing")
                        .and_then(serde_json::Value::as_bool)
                        == Some(true) =>
                {
                    self.gc_command_discovery_pending = false;
                    if self.gc_command_menu_open {
                        if let Some(agent_id) = value
                            .get("agentId")
                            .and_then(serde_json::Value::as_str)
                            .and_then(|value| crate::AgentId::parse(value).ok())
                        {
                            self.select_agent(Some(agent_id));
                        }
                        self.gc_command_discovery_requested = true;
                        self.gc_command_loading = true;
                    }
                }
                Ok(value)
                    if value.get("queued").and_then(serde_json::Value::as_bool) == Some(true) => {}
                Ok(value) => {
                    self.gc_command_discovery_pending = false;
                    if let Some(commands) =
                        value.get("commands").and_then(serde_json::Value::as_array)
                    {
                        self.gc_command_items.clone_from(commands);
                    }
                    if self.gc_command_menu_open {
                        self.gc_command_loading = false;
                    }
                }
                Err(error) if self.gc_command_menu_open => {
                    self.gc_command_discovery_pending = false;
                    self.gc_command_loading = false;
                    self.gc_command_discovery_requested = false;
                    self.status = format!("Could not load Glass Core commands: {error}");
                }
                Err(_) => {
                    self.gc_command_discovery_pending = false;
                }
            }
            // Catalogue discovery is palette state, even if its response
            // arrives after Escape closed the menu. It is never chat output.
            return;
        }
        if self.gc_model_discovery_jobs.remove(&result.id) {
            if self.running_tool_job == Some(result.id) {
                self.running_tool_job = None;
            }
            match result.result {
                Ok(value)
                    if value.get("queued").and_then(serde_json::Value::as_bool) == Some(true) =>
                {
                    self.status = "Glass Core is preparing provider models…".into();
                }
                Ok(_) => {
                    self.gc_command_loading = false;
                    self.gc_model_error = Some("Glass Core returned no provider models".into());
                }
                Err(error) => {
                    self.gc_command_loading = false;
                    self.gc_model_error = Some(error.clone());
                    self.status = format!("Could not request provider models: {error}");
                }
            }
            return;
        }
        let mut publish_preview_status = true;
        if result.tool == "glass.agent.send"
            && self
                .pending_chat_messages
                .iter()
                .any(|message| message.job_id == Some(result.id))
        {
            match result.result {
                Ok(value) => {
                    let restarted = value
                        .get("restarted")
                        .and_then(serde_json::Value::as_bool)
                        .unwrap_or(false);
                    let returned_agent = value
                        .get("agentId")
                        .and_then(serde_json::Value::as_str)
                        .and_then(|value| crate::AgentId::parse(value).ok());
                    let owner = self
                        .pending_chat_messages
                        .iter_mut()
                        .find(|message| message.job_id == Some(result.id))
                        .and_then(|message| {
                            if message.agent_id.is_none() {
                                message.agent_id = returned_agent.clone();
                            }
                            message.agent_id.clone()
                        });
                    if self.selected_agent.is_none() && owner.is_some() {
                        self.select_agent(owner.clone());
                    }
                    if owner == self.selected_agent {
                        self.status = if restarted {
                            format!(
                                "Restarted {} · Glass Agent is thinking…",
                                owner.as_ref().map_or("Glass Agent", crate::AgentId::as_str)
                            )
                        } else {
                            format!(
                                "Sent to {} · Glass Agent is thinking…",
                                owner.as_ref().map_or("Glass Agent", crate::AgentId::as_str)
                            )
                        };
                    }
                    if let Some(message) = self
                        .pending_chat_messages
                        .iter_mut()
                        .find(|message| message.job_id == Some(result.id))
                    {
                        message.state = ChatMessageState::Sent;
                        message.job_id = None;
                    }
                }
                Err(error) => {
                    let failed = self
                        .pending_chat_messages
                        .iter_mut()
                        .find(|message| message.job_id == Some(result.id))
                        .map(|message| {
                            message.state = ChatMessageState::Failed;
                            message.job_id = None;
                            message.error = Some(error.clone());
                            (
                                message.agent_id.clone(),
                                message.text.clone(),
                                message.message_id.clone(),
                            )
                        });
                    if let Some((owner, text, message_id)) = failed {
                        if owner == self.selected_agent {
                            if self.composer_input.is_empty() {
                                self.composer_input = text.clone();
                                self.composer_cursor = self.composer_input.len();
                                self.composer_retry_message_id = Some(message_id);
                            } else if self.composer_input == text {
                                self.composer_retry_message_id = Some(message_id);
                            } else {
                                self.composer_retry_message_id = None;
                            }
                            self.composer_mode = true;
                            self.status =
                                "Message failed · full error shown in conversation · edit or retry"
                                    .into();
                        } else {
                            let draft = self.thread_drafts.entry(owner).or_default();
                            if draft.text.is_empty() {
                                draft.cursor = text.len();
                                draft.text = text;
                            }
                        }
                    }
                }
            }
            self.agent_send_job = self
                .pending_chat_messages
                .iter()
                .rev()
                .find(|message| message.state == ChatMessageState::Sending)
                .and_then(|message| message.job_id);
            return;
        }

        if self
            .active_selection_preview
            .as_ref()
            .is_some_and(|active| active.id == result.id)
        {
            let active = self
                .active_selection_preview
                .take()
                .expect("matching active selection preview");
            if self.running_tool_job != Some(result.id) {
                return;
            }
            self.running_tool_job = None;
            if active.key.tool_name() != result.tool {
                return;
            }
            if !self.selection_preview_key_is_current(&active.key) {
                self.finish_stale_debug_preview(&active.key, &active.label, false);
                return;
            }
            publish_preview_status = active.key.publishes_status_on(self.surface);
        } else {
            if self.running_tool_job != Some(result.id) {
                return;
            }
            self.running_tool_job = None;
        }
        match result.result {
            Ok(value) => {
                if result.tool == "glass.agent.sessions" {
                    self.open_session_picker(&value);
                    return;
                }
                if result.tool == "glass.agent.stats" {
                    self.agent_token_summary = stats_summary(&value);
                    self.status = format!("Session stats · {}", self.agent_token_summary);
                    return;
                }
                if result.tool == "glass.browser.verify" {
                    let ok = value.get("status").and_then(serde_json::Value::as_str)
                        == Some("satisfied");
                    self.last_proof_ok = Some(ok);
                    self.last_verify = Some(evidence_card(
                        value
                            .pointer("/predicate/urlEquals")
                            .and_then(serde_json::Value::as_str)
                            .or_else(|| value.get("state").and_then(serde_json::Value::as_str))
                            .unwrap_or("live page"),
                        value
                            .get("elapsedMs")
                            .and_then(serde_json::Value::as_u64)
                            .unwrap_or(0),
                        None,
                        value
                            .get("state")
                            .and_then(serde_json::Value::as_str)
                            .unwrap_or("verify"),
                        ok,
                    ));
                    self.status = if ok {
                        "Prove-it passed · gutter ✓".into()
                    } else {
                        "Prove-it failed · inspect last verify".into()
                    };
                } else if result.tool.starts_with("glass.browser") {
                    self.apply_browser_result(&result.tool, &value);
                    self.browser_detail = super::projection::browser_result(&result.tool, &value);
                    if result.tool == "glass.browser.observe" {
                        self.browser_observe_retry_count = 0;
                        self.browser_observe_retry_after = None;
                    }
                    if result.tool == "glass.browser.act" || result.tool == "glass.browser.navigate"
                    {
                        self.browser_observe_pending = true;
                        self.browser_observe_retry_after = None;
                        self.watch_agent_on_app(&result.tool, &value);
                    }
                } else if result.tool == "glass.process.logs" {
                    self.process_logs = value
                        .get("output")
                        .and_then(serde_json::Value::as_str)
                        .unwrap_or_default()
                        .chars()
                        .rev()
                        .take(8 * 1024)
                        .collect::<String>()
                        .chars()
                        .rev()
                        .collect();
                    self.surface = DevSurface::Terminal;
                    self.status = format!(
                        "Logs · {}",
                        value
                            .get("name")
                            .and_then(serde_json::Value::as_str)
                            .unwrap_or("process")
                    );
                } else if result.tool == "glass.debug.threads" {
                    self.debug_threads = parse_debug_threads(&value);
                    self.selected_debug_thread = self
                        .selected_debug_thread
                        .min(self.debug_threads.len().saturating_sub(1));
                    self.debug_frames.clear();
                    if !self.debug_threads.is_empty() {
                        self.debug_pane = DebugPane::Threads;
                        self.debug_stack_requested = true;
                    }
                    self.status = format!(
                        "{} thread(s) · Enter stack · Space continue",
                        self.debug_threads.len()
                    );
                } else if result.tool == "glass.debug.stack" {
                    self.debug_frames = parse_debug_frames(&value);
                    self.selected_debug_frame = 0;
                    if !self.debug_frames.is_empty() {
                        self.debug_pane = DebugPane::Frames;
                        self.debug_scopes_requested = true;
                    }
                    self.status = format!(
                        "{} frame(s) · Enter jumps to source",
                        self.debug_frames.len()
                    );
                } else if result.tool == "glass.debug.scopes" {
                    self.debug_scopes = parse_debug_scopes(&value);
                    self.selected_debug_scope = 0;
                    self.debug_variables.clear();
                    self.debug_variables_notice = None;
                    if !self.debug_scopes.is_empty() {
                        self.debug_variables_requested = true;
                    }
                    if publish_preview_status {
                        self.status = format!("{} scope(s)", self.debug_scopes.len());
                    }
                } else if result.tool == "glass.debug.variables" {
                    self.debug_variables = parse_debug_variables(&value);
                    self.debug_variables_notice = None;
                    if publish_preview_status {
                        self.status = format!("{} variable(s)", self.debug_variables.len());
                    }
                } else if result.tool == "glass.git.diff" {
                    let empty = value.as_str().is_none_or(|diff| diff.trim().is_empty());
                    self.git_diff = if empty {
                        self.git_diff_path
                            .as_deref()
                            .map(|path| format!("{path}\n\nNo tracked diff for this file."))
                            .unwrap_or_else(|| "Working tree clean · no diff to show".into())
                    } else {
                        value.as_str().unwrap_or_default().to_string()
                    };
                    self.git_diff_open = true;
                    self.status = self
                        .git_diff_path
                        .as_deref()
                        .map(|path| format!("Git diff · {path} · PgUp/PgDn scroll · Esc closes"))
                        .unwrap_or_else(|| "Git diff · PgUp/PgDn scroll · Esc closes".into());
                } else if result.tool == "glass.agent.send" {
                    if let Some(agent_id) = value
                        .get("agentId")
                        .and_then(serde_json::Value::as_str)
                        .and_then(|value| crate::AgentId::parse(value).ok())
                    {
                        self.status =
                            format!("Sent to {} · Glass Agent is thinking…", agent_id.as_str());
                    } else {
                        self.status = "Sent · Glass Agent is thinking…".into();
                    }
                } else if result.tool == "glass.agent.delegate" {
                    self.surface = DevSurface::Agent;
                    let harness = value
                        .get("harness")
                        .and_then(serde_json::Value::as_str)
                        .unwrap_or("external harness");
                    let outcome = if value.get("success").and_then(serde_json::Value::as_bool)
                        == Some(true)
                    {
                        "completed"
                    } else if value.get("timedOut").and_then(serde_json::Value::as_bool)
                        == Some(true)
                    {
                        "timed out"
                    } else {
                        "failed"
                    };
                    let detail = super::projection::external_agent_output(&value);
                    self.status = format!("Temporary {harness} {outcome} · {detail}");
                } else if result.tool == "glass.github.review" {
                    match serde_json::from_value::<crate::github::GitHubReview>(value) {
                        Ok(review) => {
                            self.github = crate::github::GitHubStatus {
                                repository: review.repository.clone(),
                                availability: review.availability,
                            };
                            self.github_review = review.display();
                            self.surface = DevSurface::Git;
                            self.status = "GitHub review refreshed".into();
                        }
                        Err(error) => {
                            self.status = format!("GitHub review response invalid: {error}");
                        }
                    }
                } else if result.tool == "glass.github.ship" {
                    self.surface = DevSurface::Git;
                    self.status = value
                        .get("url")
                        .and_then(serde_json::Value::as_str)
                        .map(|url| format!("Pull request created · {url}"))
                        .unwrap_or_else(|| "Pull request created · review GitHub status".into());
                } else if result.tool == "glass.task.wake" {
                    self.last_crew_wake = serde_json::from_value::<crate::CrewWake>(value.clone())
                        .ok()
                        .map(|wake| wake.render());
                    self.surface = DevSurface::Tasks;
                    self.status = "Crew wake loaded · :review".into();
                } else if result.tool == "glass.task.crew" {
                    let wake = value.get("wake").cloned().unwrap_or_else(|| value.clone());
                    self.last_crew_wake = serde_json::from_value::<crate::CrewWake>(wake)
                        .ok()
                        .map(|wake| wake.render())
                        .or_else(|| {
                            value
                                .get("goal")
                                .and_then(serde_json::Value::as_str)
                                .map(|goal| format!("WAKE\n  goal {goal}"))
                        });
                    self.surface = DevSurface::Tasks;
                    self.status = format!(
                        "Crew wake {} · :review",
                        value
                            .pointer("/wake/id")
                            .and_then(serde_json::Value::as_str)
                            .unwrap_or("queued")
                    );
                } else if result.tool.starts_with("glass.lsp") {
                    if result.tool == "glass.lsp.inlay_hints" {
                        self.editor_inlays =
                            parse_inlay_hints(value.get("result").unwrap_or(&value));
                    }
                    if result.tool == "glass.lsp.diagnostics" {
                        self.editor_diagnostics =
                            parse_editor_diagnostics(value.get("result").unwrap_or(&value));
                    }
                    self.editor = if result.tool == "glass.lsp.diagnostics" {
                        super::projection::lsp(Some(&value))
                    } else {
                        super::projection::first_meaningful(&value)
                    };
                    if matches!(
                        result.tool.as_str(),
                        "glass.lsp.formatting" | "glass.lsp.range_formatting" | "glass.lsp.rename"
                    ) {
                        self.handle_lsp_preview_result(&result.tool, &value);
                    }
                }
                if !matches!(
                    result.tool.as_str(),
                    "glass.agent.send"
                        | "glass.agent.delegate"
                        | "glass.git.diff"
                        | "glass.github.review"
                        | "glass.github.ship"
                        | "glass.browser.verify"
                        | "glass.task.crew"
                        | "glass.task.wake"
                        | "glass.process.logs"
                        | "glass.debug.threads"
                        | "glass.debug.stack"
                        | "glass.debug.scopes"
                        | "glass.debug.variables"
                ) && !matches!(
                    result.tool.as_str(),
                    "glass.lsp.formatting" | "glass.lsp.range_formatting" | "glass.lsp.rename"
                ) {
                    self.status = format!("Completed {} · workspace refreshed", result.tool);
                }
            }
            Err(error) => {
                if matches!(
                    result.tool.as_str(),
                    "glass.lsp.formatting" | "glass.lsp.range_formatting"
                ) {
                    self.pending_lsp_format_baselines.pop_front();
                }
                if matches!(
                    result.tool.as_str(),
                    "glass.agent.commands" | "glass.agent.command"
                ) && self.gc_command_menu_open
                {
                    self.gc_command_loading = false;
                    self.gc_command_discovery_requested = false;
                    self.status = format!("Glass Core command request failed: {error}");
                    return;
                }
                if result.tool == "glass.browser.verify" {
                    self.last_proof_ok = Some(false);
                    self.last_verify = Some(evidence_card("live page", 0, None, &error, false));
                }
                if result.tool == "glass.browser.observe" {
                    self.browser_observe_retry_count =
                        self.browser_observe_retry_count.saturating_add(1);
                    if self.browser_observe_retry_count <= 3 {
                        self.browser_observe_pending = true;
                        self.browser_observe_retry_after =
                            Some(std::time::Instant::now() + std::time::Duration::from_secs(2));
                        self.status = format!(
                            "Browser evidence refresh failed ({}/3); retrying: {error}",
                            self.browser_observe_retry_count
                        );
                    } else {
                        self.browser_observe_pending = false;
                        self.browser_observe_retry_after = None;
                        self.browser_observe_retry_count = 0;
                        self.status = format!(
                            "Browser evidence may be stale · refresh failed after 3 attempts: {error}"
                        );
                    }
                }
                let mut recovery_offered = result.tool.starts_with("glass.browser")
                    && self.note_browser_failure(&result.tool, &error);
                if result.tool == "glass.browser.observe" {
                    recovery_offered = true;
                }
                if result.tool == "glass.git.diff" {
                    self.git_diff = format!("Git diff unavailable: {error}");
                    self.git_diff_open = true;
                }
                if publish_preview_status && !recovery_offered {
                    self.status = format!("{} failed: {error}", result.tool);
                }
            }
        }
    }

    pub fn deny_confirmation(&mut self) {
        self.pending_browser_navigation = None;
        self.pending_page_entity = None;
        if let Some(pending) = self.pending_confirmation.take() {
            self.status = format!("Denied · {}", pending.summary);
        }
    }

    /// Queue browser startup or a fresh observation as part of a requested navigation.
    pub fn prepare_browser_navigation(&mut self, url: &str) -> Result<String, String> {
        if self.background_action_running() {
            return Err("another browser action is already awaiting or running".into());
        }
        if self.browser_workspace.state().connection == BrowserConnectionPhase::Connected {
            let (call, context) =
                self.tool_request("glass.browser.observe", serde_json::json!({}), false)?;
            self.pending_browser_navigation = Some(url.to_string());
            self.queued_tool_request = Some((call, context));
            self.surface = DevSurface::App;
            return Ok("Browser connected · refreshing page before navigation".into());
        }
        let (call, context) = self.tool_request(
            "glass.browser.start",
            serde_json::json!({"modalDialogs": true}),
            true,
        )?;
        self.pending_browser_navigation = Some(url.to_string());
        let queued = self.queue_or_confirm(
            call,
            context,
            format!("Start browser before navigating to {url}"),
        )?;
        self.surface = DevSurface::App;
        Ok(if queued {
            "Browser detached · launching; navigation continues automatically".into()
        } else {
            "Browser detached · Enter approves launch; navigation continues automatically".into()
        })
    }

    pub fn handle_printable(&mut self, character: char) {
        if character == ':' && self.surface != DevSurface::Agent {
            self.open_palette();
            return;
        }
        if character == 'a' && self.surface != DevSurface::Agent {
            self.open_menu();
            return;
        }
        // Agent text always wins over workspace navigation. `/` is routed by
        // the event loop only when it opens the explicit command menu.
        if self.surface == DevSurface::Agent {
            // Always open the composer and retain the first typed character;
            // Core readiness can lag its prompt queue during startup.
            self.open_composer();
            self.insert_composer_text(&character.to_string());
            return;
        }

        if self.surface == DevSurface::Code && character == 'i' {
            self.enter_code_edit();
            return;
        }
        let surface = match character {
            'c' => Some(DevSurface::Code),
            'v' => Some(DevSurface::App),
            'g' => Some(DevSurface::Git),
            _ => None,
        };
        if let Some(surface) = surface {
            self.show_surface(surface);
        } else if self.surface == DevSurface::Agent {
            self.open_composer();
            self.insert_composer_text(&character.to_string());
        }
    }
    /// Queue the project-detected development command without making users
    /// copy it from project inspection into the command palette.
    pub fn request_detected_dev(&mut self) {
        if self.background_action_running() {
            self.status = "Another background action is still running".into();
            return;
        }
        let command_result = {
            match self.ws() {
                Ok(workspace) => workspace
                    .project()
                    .detection()
                    .dev_command
                    .clone()
                    .ok_or_else(|| "no detected dev command".to_string()),
                Err(error) => Err(error),
            }
        };
        let command = match command_result {
            Ok(command) => command,
            Err(error) if error == "no detected dev command" => {
                self.status =
                    "No development command detected · configure a project command, then start it with Ctrl-K".into();
                return;
            }
            Err(error) => {
                self.status = if error.to_string().contains("workspace busy") {
                    "Development suite unavailable: workspace refresh is still running · use :s again"
                        .into()
                } else {
                    format!("Development command unavailable: {error}")
                };
                return;
            }
        };
        let (call, context) = match self.tool_request(
            "glass.process.start",
            serde_json::json!({"name": "dev", "command": command}),
            true,
        ) {
            Ok(request) => request,
            Err(error) => {
                self.status = format!("Development suite unavailable: {error}");
                return;
            }
        };
        let queued = match self.queue_or_confirm(
            call,
            context,
            "Start the detected development suite".into(),
        ) {
            Ok(queued) => queued,
            Err(error) => {
                self.status = format!("Development suite unavailable: {error}");
                return;
            }
        };
        self.surface = DevSurface::Terminal;
        if !queued {
            self.status = "Development suite ready · Enter approves once · Esc cancels".into();
        }
    }

    pub fn show_surface(&mut self, surface: DevSurface) {
        self.surface = surface;
        self.composer_mode = surface == DevSurface::Agent;
        self.composer_code_context_enabled =
            surface == DevSurface::Code && !self.focused_editor_path.is_empty();
        self.status = format!("{} selected", surface.label());
        if surface != DevSurface::Code {
            self.pending_open_file = None;
        }
        if surface == DevSurface::Code && !self.code_edit_mode {
            self.queue_selected_file_preview();
        }
        if surface == DevSurface::Git {
            self.request_selected_git_diff();
        }
    }

    fn queue_selected_file_preview(&mut self) {
        if self.files.is_empty() {
            self.pending_open_file = Some(String::new());
            return;
        }
        self.open_selected_file();
    }

    pub fn flush_pending_open_file(&mut self) {
        if self.pending_open_file.is_none() {
            return;
        }
        if self.code_edit_mode {
            return;
        }
        if self.files.is_empty() {
            return;
        }
        let queued = self.pending_open_file.take().unwrap_or_default();
        if !queued.is_empty()
            && let Some(index) = self.files.iter().position(|path| path == &queued)
        {
            self.selected_file = index;
        }
        self.open_selected_file();
    }

    fn request_selected_git_diff(&mut self) {
        if self.git_entries.is_empty() {
            return;
        }
        self.git_diff_requested = true;
        if let Some(path) = self.selected_git_entry().map(|entry| entry.path.clone()) {
            self.git_diff_path = Some(path);
        }
        if !self.git_diff_open {
            self.git_diff_open = true;
            self.git_diff = "Loading Git diff…".into();
        }
    }

    fn is_workspace_busy(error: &impl std::fmt::Display) -> bool {
        let error = error.to_string().to_ascii_lowercase();
        error.contains("workspace busy") || error.contains("wouldblock")
    }

    pub fn next_surface(&mut self) {
        let surfaces: &[DevSurface] = if self
            .responsive_class(self.terminal_width, self.terminal_height)
            == ResponsiveClass::Phone
        {
            &DevSurface::PHONE
        } else {
            &DevSurface::PRIMARY
        };
        let index = surfaces
            .iter()
            .position(|surface| *surface == self.surface)
            .unwrap_or(0);
        self.show_surface(surfaces[(index + 1) % surfaces.len()]);
    }

    pub fn scroll_surface(&mut self, delta: i32) {
        if self.surface == DevSurface::Agent {
            let (_, height) = super::render::agent_conversation_viewport_size(self);
            self.core_transcript
                .borrow_mut()
                .step(delta as isize, height);
            return;
        }
        let scroll = self.surface_scroll.entry(self.surface).or_default();
        *scroll = (*scroll as i64 + i64::from(delta)).max(0) as usize;
    }

    pub fn scroll_home(&mut self) {
        if self.surface == DevSurface::Agent {
            self.core_transcript.borrow_mut().top();
            return;
        }
        self.surface_scroll.insert(self.surface, 0);
    }

    pub fn scroll_end(&mut self) {
        if self.surface == DevSurface::Agent {
            let _ = super::render::agent_conversation_scroll_end(self);
            let (_, height) = super::render::agent_conversation_viewport_size(self);
            self.core_transcript.borrow_mut().follow_tail(height);
        } else {
            self.surface_scroll.insert(
                self.surface,
                self.surface_content_height().saturating_sub(1),
            );
        }
    }

    /// Bounded logical height of the current surface's content, used for End.
    fn surface_content_height(&self) -> usize {
        if self.surface == DevSurface::Agent {
            return self.core_transcript.borrow().total_lines().max(1);
        }
        let content = match self.surface {
            DevSurface::Code => &self.editor,
            DevSurface::App => &self.browser,
            DevSurface::Terminal => &self.processes,
            DevSurface::Tasks => &self.tasks,
            DevSurface::Git => &self.git,
            DevSurface::Debug => &self.debugger,
            DevSurface::More => &self.workspace_status,
            DevSurface::Agent => unreachable!("agent content handled above"),
        };
        content.lines().count().max(1)
    }

    pub fn current_scroll(&self) -> u16 {
        if self.surface == DevSurface::Agent {
            return self
                .core_transcript
                .borrow()
                .scroll()
                .min(u16::MAX as usize) as u16;
        }
        self.surface_scroll
            .get(&self.surface)
            .copied()
            .unwrap_or(0)
            .min(u16::MAX as usize) as u16
    }

    pub fn move_file_selection(&mut self, delta: i32) {
        if self.files.is_empty() {
            self.selected_file = 0;
            return;
        }
        self.selected_file = (self.selected_file as i32 + delta)
            .clamp(0, self.files.len().saturating_sub(1) as i32)
            as usize;
        if !self.code_edit_mode {
            self.open_selected_file();
        }
    }
    pub fn move_git_selection(&mut self, delta: i32) {
        if self.git_entries.is_empty() {
            self.selected_git_file = 0;
            self.status = "No changed files selected".into();
            return;
        }
        self.selected_git_file = (self.selected_git_file as i32 + delta)
            .clamp(0, self.git_entries.len().saturating_sub(1) as i32)
            as usize;
        if let Some(entry) = self.git_entries.get(self.selected_git_file) {
            self.status = format!(
                "{} · Enter diff · Space stage · c commit · o open",
                entry.path
            );
            self.git_diff_requested = true;
        }
    }

    pub fn selected_git_entry(&self) -> Option<&crate::git::GitStatusEntry> {
        self.git_entries.get(self.selected_git_file)
    }

    pub fn open_selected_git_file(&mut self) {
        let Some(path) = self.selected_git_entry().map(|entry| entry.path.clone()) else {
            self.status = "Select a changed file · o opens it in Code".into();
            return;
        };
        match self.open_path(&path) {
            Ok(_) => self.status = format!("Opened {path} in Code"),
            Err(error) => self.status = format!("Could not open {path}: {error}"),
        }
    }

    pub fn compose_git_commit(&mut self) {
        self.open_palette_with("git commit ");
        self.status = "Commit · type a message, then Enter".into();
    }

    pub fn discard_selected_git_file(&mut self) {
        let Some(path) = self.selected_git_entry().map(|entry| entry.path.clone()) else {
            self.status = "Select a changed file · x discards working-tree changes".into();
            return;
        };
        match self.tool_request(
            "glass.git.discard",
            serde_json::json!({"paths": [path.clone()]}),
            true,
        ) {
            Ok((call, context)) => {
                let _ = self.queue_or_confirm(call, context, format!("Discard {path}"));
            }
            Err(error) => self.status = format!("Git discard unavailable: {error}"),
        }
    }

    pub fn queue_github_review(&mut self, worker: &mut super::snapshot::SnapshotWorker) {
        if self.background_action_running() {
            self.status = "GitHub review waits for the current background operation".into();
            return;
        }
        let (call, context) =
            match self.tool_request("glass.github.review", serde_json::json!({}), false) {
                Ok(request) => request,
                Err(error) => {
                    self.status = format!("GitHub review unavailable: {error}");
                    return;
                }
            };
        match worker.submit_tool(call, context) {
            Ok(id) => {
                self.running_tool_job = Some(id);
                self.status = "Loading pull-request review…".into();
            }
            Err(error) => self.status = format!("GitHub review unavailable: {error}"),
        }
    }

    pub fn move_process_selection(&mut self, delta: i32) {
        if self.process_entries.is_empty() {
            self.selected_process = 0;
            self.status = "No managed process selected".into();
            return;
        }
        self.selected_process = (self.selected_process as i32 + delta)
            .clamp(0, self.process_entries.len().saturating_sub(1) as i32)
            as usize;
        if let Some(entry) = self.selected_process_entry() {
            self.status = format!(
                "{} · {} · Enter logs · Space restart · u Browser · x stop",
                entry.name,
                entry.health.label()
            );
            self.process_logs_requested = true;
        }
    }

    pub fn selected_process_entry(&self) -> Option<&ProcessRow> {
        self.process_entries.get(self.selected_process)
    }

    pub fn queue_selected_process_logs(&mut self, worker: &mut super::snapshot::SnapshotWorker) {
        let Some(name) = self
            .selected_process_entry()
            .map(|entry| entry.name.clone())
        else {
            self.status = "Select a process · j/k then Enter logs".into();
            return;
        };
        let (call, context) = match self.tool_request(
            "glass.process.logs",
            serde_json::json!({"name": name}),
            false,
        ) {
            Ok(request) => request,
            Err(error) => {
                self.status = format!("Process logs unavailable: {error}");
                return;
            }
        };
        self.queue_selection_preview(
            worker,
            SelectionPreviewKey {
                surface: self.surface,
                expected_debug_pane: None,
                target: SelectionPreviewTarget::ProcessLogs { name: name.clone() },
            },
            call,
            context,
            format!("logs for {name}"),
        );
    }

    pub fn restart_selected_process(&mut self) {
        let Some(name) = self
            .selected_process_entry()
            .map(|entry| entry.name.clone())
        else {
            self.status = "Select a process · j/k then Space restarts".into();
            return;
        };
        match self.tool_request(
            "glass.process.restart",
            serde_json::json!({"name": name}),
            true,
        ) {
            Ok((call, context)) => {
                let _ = self.queue_or_confirm(call, context, format!("Restart {name}"));
            }
            Err(error) => self.status = format!("Process restart unavailable: {error}"),
        }
    }

    pub fn stop_selected_process(&mut self) {
        let Some(name) = self
            .selected_process_entry()
            .map(|entry| entry.name.clone())
        else {
            self.status = "Select a process · x stops it".into();
            return;
        };
        match self.tool_request(
            "glass.process.stop",
            serde_json::json!({"name": name}),
            true,
        ) {
            Ok((call, context)) => {
                let _ = self.queue_or_confirm(call, context, format!("Stop {name}"));
            }
            Err(error) => self.status = format!("Process stop unavailable: {error}"),
        }
    }

    pub fn attach_selected_process_url(&mut self) -> Result<String, String> {
        let url = self
            .selected_process_entry()
            .and_then(|entry| entry.url.clone())
            .or_else(|| self.resolved_app_url())
            .ok_or_else(|| {
                "No URL on the selected process · start a suite that prints one".to_string()
            })?;
        if !self.auto_checkpoint("before-app-attach") {
            return Err(self.status.clone());
        }
        self.prepare_browser_navigation(&url)
    }

    pub fn cycle_debug_pane(&mut self, delta: i32) {
        self.debug_pane = if delta < 0 {
            self.debug_pane.previous()
        } else {
            self.debug_pane.next()
        };
        self.status = format!(
            "Debug {} · j/k select · Enter · Space continue",
            self.debug_pane.label()
        );
    }

    pub fn move_debug_selection(&mut self, delta: i32) {
        match self.debug_pane {
            DebugPane::Sessions => {
                if self.debug_sessions.is_empty() {
                    self.selected_debug_session = 0;
                    self.status =
                        "No debugger session · start a configured debugger from Ctrl-K actions"
                            .into();
                    return;
                }
                self.selected_debug_session = (self.selected_debug_session as i32 + delta)
                    .clamp(0, self.debug_sessions.len().saturating_sub(1) as i32)
                    as usize;
                if let Some(session) = self.selected_debug_session() {
                    self.status = format!(
                        "{} · {} · Enter threads",
                        session.name,
                        session.state.label()
                    );
                    self.debug_threads_requested = true;
                }
            }
            DebugPane::Threads => {
                if self.debug_threads.is_empty() {
                    self.status = "No threads · Enter on a session to refresh".into();
                    return;
                }
                self.selected_debug_thread = (self.selected_debug_thread as i32 + delta)
                    .clamp(0, self.debug_threads.len().saturating_sub(1) as i32)
                    as usize;
                if let Some(thread) = self.selected_debug_thread() {
                    self.status = format!("{} · Enter stack · Space continue", thread.name);
                    self.debug_stack_requested = true;
                }
            }
            DebugPane::Frames => {
                if self.debug_frames.is_empty() {
                    self.status = "No frames · Enter on a thread to load the stack".into();
                    return;
                }
                self.selected_debug_frame = (self.selected_debug_frame as i32 + delta)
                    .clamp(0, self.debug_frames.len().saturating_sub(1) as i32)
                    as usize;
                if let Some(frame) = self.selected_debug_frame() {
                    self.status = format!(
                        "{} · Enter jumps to {}",
                        frame.name,
                        frame.path.as_deref().unwrap_or("source")
                    );
                    self.debug_scopes_requested = true;
                }
            }
        }
    }

    pub fn selected_debug_session(&self) -> Option<&DebugSessionRow> {
        self.debug_sessions.get(self.selected_debug_session)
    }

    pub fn selected_debug_thread(&self) -> Option<&DebugThreadRow> {
        self.debug_threads.get(self.selected_debug_thread)
    }

    pub fn selected_debug_frame(&self) -> Option<&DebugFrameRow> {
        self.debug_frames.get(self.selected_debug_frame)
    }

    pub fn activate_debug_selection(&mut self, worker: &mut super::snapshot::SnapshotWorker) {
        match self.debug_pane {
            DebugPane::Sessions => self.queue_debug_threads(worker),
            DebugPane::Threads => self.queue_debug_stack(worker),
            DebugPane::Frames => self.jump_selected_debug_frame(),
        }
    }

    pub fn queue_debug_threads(&mut self, worker: &mut super::snapshot::SnapshotWorker) {
        let Some(session) = self.selected_debug_session().map(|row| row.name.clone()) else {
            self.status = "Start a configured debugger from Ctrl-K actions".into();
            return;
        };
        let (call, context) = match self.tool_request(
            "glass.debug.threads",
            serde_json::json!({"session": session}),
            false,
        ) {
            Ok(request) => request,
            Err(error) => {
                self.status = format!("Debug threads unavailable: {error}");
                return;
            }
        };
        self.queue_selection_preview(
            worker,
            SelectionPreviewKey {
                surface: self.surface,
                expected_debug_pane: Some(DebugPane::Sessions),
                target: SelectionPreviewTarget::DebugThreads {
                    session: session.clone(),
                },
            },
            call,
            context,
            format!("threads for {session}"),
        );
    }

    pub fn queue_debug_stack(&mut self, worker: &mut super::snapshot::SnapshotWorker) {
        let Some(session) = self.selected_debug_session().map(|row| row.name.clone()) else {
            self.status = "Select a debugger session first".into();
            return;
        };
        let Some(thread_id) = self.selected_debug_thread().map(|thread| thread.id) else {
            self.status = "Select a thread · Enter on a session first".into();
            return;
        };
        let (call, context) = match self.tool_request(
            "glass.debug.stack",
            serde_json::json!({"session": session, "threadId": thread_id}),
            false,
        ) {
            Ok(request) => request,
            Err(error) => {
                self.status = format!("Debug stack unavailable: {error}");
                return;
            }
        };
        self.queue_selection_preview(
            worker,
            SelectionPreviewKey {
                surface: self.surface,
                expected_debug_pane: Some(DebugPane::Threads),
                target: SelectionPreviewTarget::DebugStack {
                    session: session.clone(),
                    thread_id,
                },
            },
            call,
            context,
            format!("stack for thread {thread_id}"),
        );
    }

    pub fn continue_selected_debug(&mut self) {
        let Some(session) = self.selected_debug_session().map(|row| row.name.clone()) else {
            self.status = "Select a debugger session · Space continues".into();
            return;
        };
        let Some(thread_id) = self.selected_debug_thread().map(|thread| thread.id) else {
            self.status = "Refresh threads before continue".into();
            return;
        };
        match self.tool_request(
            "glass.debug.continue",
            serde_json::json!({"session": session, "threadId": thread_id}),
            true,
        ) {
            Ok((call, context)) => {
                let _ = self.queue_or_confirm(
                    call,
                    context,
                    format!("Continue {session} thread {thread_id}"),
                );
            }
            Err(error) => self.status = format!("Debug continue unavailable: {error}"),
        }
    }

    pub fn step_selected_debug(&mut self, kind: &str) {
        let Some(session) = self.selected_debug_session().map(|row| row.name.clone()) else {
            self.status = "Select a debugger session · n over · i in · o out".into();
            return;
        };
        let Some(thread_id) = self.selected_debug_thread().map(|thread| thread.id) else {
            self.status = "Refresh threads before stepping".into();
            return;
        };
        match self.tool_request(
            "glass.debug.step",
            serde_json::json!({"session": session, "threadId": thread_id, "kind": kind}),
            true,
        ) {
            Ok((call, context)) => {
                let _ = self.queue_or_confirm(
                    call,
                    context,
                    format!("Step {kind} {session} thread {thread_id}"),
                );
            }
            Err(error) => self.status = format!("Debug step unavailable: {error}"),
        }
    }

    pub fn pause_selected_debug(&mut self) {
        let Some(session) = self.selected_debug_session().map(|row| row.name.clone()) else {
            self.status = "Select a debugger session · p pauses".into();
            return;
        };
        let thread_id = self
            .selected_debug_thread()
            .map(|thread| thread.id)
            .unwrap_or(1);
        match self.tool_request(
            "glass.debug.pause",
            serde_json::json!({"session": session, "threadId": thread_id}),
            true,
        ) {
            Ok((call, context)) => {
                let _ = self.queue_or_confirm(call, context, format!("Pause {session}"));
            }
            Err(error) => self.status = format!("Debug pause unavailable: {error}"),
        }
    }

    pub fn breakpoint_focused_line(&mut self) {
        let Some(session) = self.selected_debug_session().map(|row| row.name.clone()) else {
            self.status = "Start a debugger before setting breakpoints".into();
            return;
        };
        if self.focused_editor_path.is_empty() {
            self.status = "Open a file in Code · b sets a breakpoint on the cursor line".into();
            return;
        }
        let line = u64::from(self.focused_editor_line.max(1));
        match self.tool_request(
            "glass.debug.breakpoint.set",
            serde_json::json!({
                "session": session,
                "path": self.focused_editor_path,
                "lines": [line],
            }),
            true,
        ) {
            Ok((call, context)) => {
                let _ = self.queue_or_confirm(
                    call,
                    context,
                    format!("Breakpoint {}:{}", self.focused_editor_path, line),
                );
            }
            Err(error) => self.status = format!("Breakpoint unavailable: {error}"),
        }
    }

    pub fn queue_debug_scopes(&mut self, worker: &mut super::snapshot::SnapshotWorker) {
        let Some(session) = self.selected_debug_session().map(|row| row.name.clone()) else {
            self.set_debug_variables_notice("Debug scopes unavailable · select a debugger session");
            return;
        };
        let Some((frame_id, frame_path)) = self
            .selected_debug_frame()
            .map(|frame| (frame.id, frame.path.clone()))
        else {
            self.set_debug_variables_notice("Debug scopes unavailable · load a stack frame first");
            return;
        };
        let source_path = (self.surface == DevSurface::Code
            && !self.focused_editor_path.is_empty())
        .then(|| self.focused_editor_path.clone());
        let Ok((call, context)) = self.tool_request(
            "glass.debug.scopes",
            serde_json::json!({"session": session, "frameId": frame_id}),
            false,
        ) else {
            self.set_debug_variables_notice("Debug scopes unavailable · could not build request");
            return;
        };
        self.debug_variables_notice = None;
        self.queue_selection_preview(
            worker,
            SelectionPreviewKey {
                surface: self.surface,
                expected_debug_pane: Some(DebugPane::Frames),
                target: SelectionPreviewTarget::DebugScopes {
                    session,
                    frame_id,
                    frame_path,
                    source_path,
                },
            },
            call,
            context,
            "debug scopes".into(),
        );
    }

    pub fn queue_debug_variables(&mut self, worker: &mut super::snapshot::SnapshotWorker) {
        let Some(session) = self.selected_debug_session().map(|row| row.name.clone()) else {
            self.set_debug_variables_notice(
                "Debug variables unavailable · select a debugger session",
            );
            return;
        };
        let Some((frame_id, frame_path)) = self
            .selected_debug_frame()
            .map(|frame| (frame.id, frame.path.clone()))
        else {
            self.set_debug_variables_notice(
                "Debug variables unavailable · load a stack frame first",
            );
            return;
        };
        let Some(scope) = self.debug_scopes.get(self.selected_debug_scope) else {
            self.set_debug_variables_notice("Debug variables unavailable · load scopes first");
            return;
        };
        let scope_index = self.selected_debug_scope;
        let scope_name = scope.name.clone();
        let reference = scope.variables_reference;
        if reference == 0 {
            self.set_debug_variables_notice(
                "No expandable variables · selected scope has no variable reference",
            );
            return;
        }
        if reference < 0 {
            self.set_debug_variables_notice("Invalid variables reference · select another scope");
            return;
        }
        let source_path = (self.surface == DevSurface::Code
            && !self.focused_editor_path.is_empty())
        .then(|| self.focused_editor_path.clone());
        let Ok((call, context)) = self.tool_request(
            "glass.debug.variables",
            serde_json::json!({"session": session, "variablesReference": reference}),
            false,
        ) else {
            self.set_debug_variables_notice(
                "Debug variables unavailable · could not build request",
            );
            return;
        };
        self.debug_variables_notice = None;
        self.queue_selection_preview(
            worker,
            SelectionPreviewKey {
                surface: self.surface,
                expected_debug_pane: Some(DebugPane::Frames),
                target: SelectionPreviewTarget::DebugVariables {
                    session,
                    frame_id,
                    frame_path,
                    source_path,
                    scope_index,
                    scope_name,
                    reference,
                },
            },
            call,
            context,
            "debug variables".into(),
        );
    }

    pub fn jump_selected_debug_frame(&mut self) {
        let Some(frame) = self.selected_debug_frame().cloned() else {
            self.status = "Select a stack frame · Enter jumps to source".into();
            return;
        };
        let Some(path) = frame.path.clone() else {
            self.status = format!("{} has no source path", frame.name);
            return;
        };
        let source_path = match self.debug_frame_source_path(&path) {
            Ok(path) => path,
            Err(error) => {
                self.status = format!("Could not open {path}: {error}");
                return;
            }
        };
        match self.open_path(&source_path) {
            Ok(_) => {
                if let Some(line) = frame.line {
                    let _ = self.set_editor_cursor(
                        &self.focused_editor_path.clone(),
                        crate::development::TextPosition {
                            line: u32::try_from(line).unwrap_or(u32::MAX).max(1),
                            column: 1,
                        },
                        false,
                    );
                    self.refresh_editor_projection();
                }
                self.status = format!(
                    "Debug {} · {}{}",
                    frame.name,
                    source_path,
                    frame
                        .line
                        .map(|line| format!(":{line}"))
                        .unwrap_or_default()
                );
                self.debug_scopes_requested = true;
            }
            Err(error) => self.status = format!("Could not open {path}: {error}"),
        }
    }

    fn debug_frame_source_path(&self, path: &str) -> Result<String, String> {
        let path = Path::new(path);
        if !path.is_absolute() {
            return Ok(path.to_string_lossy().into_owned());
        }
        let root = std::fs::canonicalize(&self.snapshot_root)
            .map_err(|error| format!("workspace root unavailable: {error}"))?;
        let source = std::fs::canonicalize(path)
            .map_err(|error| format!("source path unavailable: {error}"))?;
        let relative = relative_workspace_path(&root, &source)
            .ok_or_else(|| "source path is outside this workspace".to_string())?;
        Ok(relative.to_string_lossy().into_owned())
    }

    pub fn move_todo_selection(&mut self, delta: i32) {
        if self.session_todos.items.is_empty() {
            self.selected_todo = 0;
            self.status = "No session todos · Plan accept or glass.todo.write".into();
            return;
        }
        self.selected_todo = (self.selected_todo as i32 + delta)
            .clamp(0, self.session_todos.items.len().saturating_sub(1) as i32)
            as usize;
        if let Some(item) = self.session_todos.items.get(self.selected_todo) {
            self.status = format!(
                "{} · {} · Enter complete · Space activate",
                item.id,
                item.status.label()
            );
        }
    }

    pub fn complete_selected_todo(&mut self) {
        let Some(id) = self
            .session_todos
            .items
            .get(self.selected_todo)
            .map(|item| item.id.clone())
        else {
            self.status = "Select a session todo · Enter marks it done".into();
            return;
        };
        match self.ws_mut().and_then(|mut workspace| {
            workspace
                .complete_todo(&id)
                .map_err(|error| error.to_string())
        }) {
            Ok(item) => {
                if let Ok(list) = self.ws().map(|workspace| workspace.todos()) {
                    self.session_todos = list;
                    self.selected_todo = self
                        .selected_todo
                        .min(self.session_todos.items.len().saturating_sub(1));
                }
                self.status = format!("Todo {} done · {}", item.id, item.title);
            }
            Err(error) => self.status = format!("Todo complete unavailable: {error}"),
        }
    }

    pub fn activate_selected_todo(&mut self) {
        let Some(id) = self
            .session_todos
            .items
            .get(self.selected_todo)
            .map(|item| item.id.clone())
        else {
            self.status = "Select a session todo · Space makes it active".into();
            return;
        };
        match self.ws_mut().and_then(|mut workspace| {
            workspace
                .activate_todo(&id)
                .map_err(|error| error.to_string())
        }) {
            Ok(item) => {
                if let Ok(list) = self.ws().map(|workspace| workspace.todos()) {
                    self.session_todos = list;
                }
                self.status = format!("Todo {} active · {}", item.id, item.title);
            }
            Err(error) => self.status = format!("Todo activate unavailable: {error}"),
        }
    }

    pub fn more_route_actions() -> &'static [command::SurfaceAction] {
        command::surface_actions(DevSurface::More)
    }

    pub fn move_more_selection(&mut self, delta: i32) {
        let actions = Self::more_route_actions();
        let last = actions.len().saturating_sub(1) as i32;
        self.selected_more = (self.selected_more as i32 + delta).clamp(0, last) as usize;
        self.status = format!(
            "Route {}/{} · {} · Enter runs",
            self.selected_more + 1,
            actions.len(),
            actions[self.selected_more].label
        );
    }

    pub fn select_more(&mut self, index: usize) {
        let actions = Self::more_route_actions();
        self.selected_more = index.min(actions.len().saturating_sub(1));
        self.status = format!(
            "Route {}/{} · {} · Enter runs",
            self.selected_more + 1,
            actions.len(),
            actions[self.selected_more].label
        );
    }

    pub fn activate_more_selection(&mut self) {
        let action = Self::more_route_actions()
            [self.selected_more.min(Self::more_route_actions().len() - 1)];
        let command = action.command;
        if let Some((_, placeholder)) = command.split_once(' ')
            && placeholder
                .split_whitespace()
                .any(|word| word.chars().all(|character| character.is_ascii_uppercase()))
        {
            let prefill = command
                .split_whitespace()
                .take_while(|word| !word.chars().all(|character| character.is_ascii_uppercase()))
                .collect::<Vec<_>>()
                .join(" ");
            self.open_palette_with(&format!("{prefill} "));
            self.status = format!("{} · complete the highlighted arguments", action.label);
            return;
        }
        match super::command::execute(self, command) {
            Ok(message) => {
                self.surface = DevSurface::More;
                self.more_result = message.clone();
                self.status = more_route_status(command, &message);
            }
            Err(error) => {
                self.surface = DevSurface::More;
                self.open_palette_with(command);
                self.status = format!("{command} · {error}");
            }
        }
    }

    pub fn refresh_experiment_projection(&mut self) -> Result<usize, String> {
        let (projection, comparison, count) = {
            let mut workspace = self.ws_mut()?;
            let experiments = workspace.experiments().map_err(|error| error.to_string())?;
            let snapshots = experiments.snapshots();
            let count = snapshots.len();
            (
                format_experiment_snapshots(&snapshots),
                experiments.compare(),
                count,
            )
        };
        self.experiments = projection;
        self.experiment_comparison = Some(comparison);
        Ok(count)
    }

    pub fn open_selected_file(&mut self) {
        let Some(requested_path) = self.files.get(self.selected_file).cloned() else {
            self.status = "No project file selected".into();
            return;
        };
        let path = self
            .workspace
            .try_lock()
            .ok()
            .and_then(|workspace| {
                workspace
                    .project()
                    .normalize_existing_file_path(&requested_path)
                    .ok()
            })
            .unwrap_or(requested_path);
        let already_open = self
            .workspace
            .try_lock()
            .map(|workspace| workspace.project().buffer(&path).is_some())
            .unwrap_or(false);
        let result = if already_open {
            Ok(())
        } else {
            self.workspace.try_lock().and_then(|mut workspace| {
                workspace
                    .project_mut()
                    .open_buffer(&path, crate::development::Actor::local())
                    .map(|_| ())
            })
        };
        match result {
            Ok(()) => {
                self.pending_open_file = None;
                self.editor_buffer_index = self
                    .workspace
                    .try_lock()
                    .ok()
                    .and_then(|workspace| {
                        workspace
                            .project()
                            .buffers()
                            .position(|buffer| buffer.path == path)
                    })
                    .unwrap_or(0);
                self.refresh_editor_projection();
                self.status = if already_open {
                    format!("Selected {path} · Enter opens the full-screen editor")
                } else {
                    format!("Opened {path} · Enter opens the full-screen editor")
                };
            }
            Err(error) if Self::is_workspace_busy(&error) => {
                self.pending_open_file = Some(path.clone());
                self.status = format!("Opening {path} when the workspace is free");
            }
            Err(error) => self.status = format!("Open failed: {error}"),
        }
    }

    pub fn open_selected_file_for_edit(&mut self) {
        let selected_path = self.files.get(self.selected_file).cloned();
        self.open_selected_file();
        if selected_path.as_deref() == Some(self.focused_editor_path.as_str()) {
            self.surface = DevSurface::Code;
            self.enter_code_edit_normal();
        }
    }

    /// Load the current Git diff without blocking key handling.
    pub fn queue_git_diff(&mut self, worker: &mut super::snapshot::SnapshotWorker) {
        let selected = self.selected_git_entry().map(|entry| {
            (
                entry.path.clone(),
                entry.untracked,
                entry.index_status,
                entry.worktree_status,
            )
        });
        if let Some((path, true, _, _)) = selected.as_ref() {
            // An untracked selection is a local placeholder, not a Git diff
            // request. Invalidate any queued preview so an older tracked diff
            // cannot replace this state when it completes.
            self.pending_selection_preview = None;
            self.git_diff_path = Some(path.clone());
            self.git_diff = format!(
                "{path}\n\nUntracked file · Git has no tracked diff yet.\nUse Enter on Code to inspect the file."
            );
            self.git_diff_open = true;
            self.surface_scroll.insert(DevSurface::Git, 0);
            self.status = format!("{path} is untracked · no Git diff to show");
            return;
        }
        let staged = selected
            .as_ref()
            .is_some_and(|(_, _, index, worktree)| *index != ' ' && *worktree == ' ');
        let mut arguments = serde_json::json!({"staged": staged});
        if let Some((path, _, _, _)) = selected.as_ref() {
            arguments["path"] = serde_json::Value::String(path.clone());
        }
        let (call, context) = match self.tool_request("glass.git.diff", arguments, false) {
            Ok(request) => request,
            Err(error) => {
                self.status = format!("Git diff unavailable: {error}");
                return;
            }
        };
        self.git_diff_path = selected.map(|(path, _, _, _)| path);
        self.git_diff_open = true;
        self.surface_scroll.insert(DevSurface::Git, 0);
        self.git_diff = "Loading Git diff…".into();
        let label = self
            .git_diff_path
            .as_deref()
            .map(|path| format!("diff for {path}"))
            .unwrap_or_else(|| "full worktree diff".into());
        self.queue_selection_preview(
            worker,
            SelectionPreviewKey {
                surface: self.surface,
                expected_debug_pane: None,
                target: SelectionPreviewTarget::GitDiff {
                    path: self.git_diff_path.clone(),
                    staged,
                },
            },
            call,
            context,
            label,
        );
    }

    /// Synchronous helper retained for callers outside the terminal event loop.
    pub fn open_git_diff(&mut self) {
        let diff = self.workspace.try_lock().and_then(|workspace| {
            workspace
                .git()
                .ok_or_else(|| {
                    crate::development::DevelopmentError::NotFound("not a Git repository".into())
                })?
                .diff(false, None)
                .map_err(|error| crate::development::DevelopmentError::Process(error.to_string()))
        });
        self.git_diff_path = None;
        match diff {
            Ok(diff) if diff.trim().is_empty() => {
                self.git_diff = "Working tree clean · no diff to show".into();
                self.git_diff_open = true;
                self.surface_scroll.insert(DevSurface::Git, 0);
                self.status = "Git diff · no changes".into();
            }
            Ok(diff) => {
                self.git_diff = diff;
                self.git_diff_open = true;
                self.surface_scroll.insert(DevSurface::Git, 0);
                self.status = "Git diff · PgUp/PgDn scroll · Esc closes".into();
            }
            Err(error) => self.status = format!("Git diff unavailable: {error}"),
        }
    }

    pub fn close_git_diff(&mut self) {
        self.git_diff_open = false;
        self.git_diff_requested = false;
        self.git_diff_path = None;
        self.surface_scroll.insert(DevSurface::Git, 0);
        self.status = "Git status · select a file with ↑/↓".into();
    }

    pub fn focused_buffer(&self) -> Option<crate::development::EditorBuffer> {
        match self.workspace.try_lock() {
            Ok(workspace) => workspace
                .project()
                .buffers()
                .nth(self.editor_buffer_index)
                .cloned(),
            Err(error) if error.to_string().contains("workspace busy") => {
                self.focused_editor_buffer.clone()
            }
            Err(_) => None,
        }
    }

    /// Bounded disk reconciliation for the open editor buffers. Returns true
    /// when a visible projection should be repainted.
    pub fn tick_editor_reconcile(&mut self) -> bool {
        let changed = match self.workspace.try_lock() {
            Ok(mut workspace) => {
                let project = workspace.project_mut();
                if project.buffers().next().is_none() {
                    false
                } else {
                    project.reconcile_open_buffers().unwrap_or(false)
                }
            }
            Err(_) => false,
        };
        if changed {
            self.refresh_editor_projection();
        }
        changed
    }

    pub fn resolve_editor_external(&mut self, action: &str) -> Result<String, String> {
        let path = self.focused_editor_path.clone();
        if path.is_empty() {
            return Err("no focused editor buffer".into());
        }
        let mut result = self
            .workspace
            .try_lock()
            .map_err(|_| "workspace busy".to_string())?;
        let project = result.project_mut();
        let message = match action {
            "compare" => {
                let snapshot = project.compare_buffer(&path).map_err(|e| e.to_string())?;
                if snapshot.exists {
                    let local = project
                        .buffer(&path)
                        .map(|buffer| buffer.content.as_str())
                        .unwrap_or_default();
                    let external = snapshot.content.as_deref().unwrap_or_default();
                    let diff = bounded_line_diff(local, external);
                    self.git_diff = diff.clone();
                    self.git_diff_open = true;
                    self.git_diff_path = Some(format!("{path} · local vs disk"));
                    self.surface_scroll.insert(DevSurface::Git, 0);
                    self.surface = DevSurface::Git;
                    self.status =
                        "External editor compare · Esc closes · local buffer preserved".into();
                    "External compare opened · local buffer preserved".into()
                } else {
                    "External file is deleted".into()
                }
            }
            "reload" => {
                let buffer = project
                    .reload_buffer_from_disk(&path, crate::development::Actor::local())
                    .map_err(|e| e.to_string())?;
                format!(
                    "Reloaded {} · {}",
                    path,
                    if buffer.dirty { "unsaved" } else { "clean" }
                )
            }
            "keep-local" => {
                project
                    .keep_local_buffer(&path)
                    .map_err(|e| e.to_string())?;
                format!("Kept local text for {} · save rebased", path)
            }
            _ => return Err("editor external actions: compare, reload, keep-local".into()),
        };
        drop(result);
        self.refresh_editor_projection();
        Ok(message)
    }

    pub fn refresh_editor_projection(&mut self) {
        let (buffers, comments, proposals, checkpoints, diagnostics) = self
            .workspace
            .lock()
            .map(|workspace| {
                (
                    workspace.project().buffers().cloned().collect::<Vec<_>>(),
                    workspace.project().editor_comments(None),
                    workspace.project().editor_proposals(),
                    workspace.project().editor_checkpoints(),
                    workspace.project().diagnostics().clone(),
                )
            })
            .unwrap_or_default();
        for attachment in &mut self.code_context_attachments {
            if let Some(buffer) = buffers.iter().find(|buffer| buffer.path == attachment.path) {
                attachment.content.clone_from(&buffer.content);
                attachment.dirty = buffer.dirty;
            }
        }
        self.editor_comments = comments;
        self.editor_proposals = proposals;
        self.reconcile_selected_editor_proposal();
        self.editor_checkpoints = checkpoints;
        if let Some(buffer) = buffers.get(self.editor_buffer_index) {
            self.focused_editor_buffer = Some(buffer.clone());
            self.focused_editor_path = buffer.path.clone();
            self.focused_editor_content = buffer.content.clone();
            self.focused_editor_dirty = buffer.dirty;
            self.focused_editor_line = buffer.cursor_line;
            self.focused_editor_column = buffer.cursor_column;
            self.focused_editor_selection = buffer.selection.clone();
            self.editor_diagnostics = diagnostics.get(&buffer.path).cloned().unwrap_or_default();
            self.ensure_editor_cursor_visible();
            let path = self.focused_editor_path.clone();
            let content = self.focused_editor_content.clone();
            self.syntax.sync(&path, &content);
            if let Some(proposal) = self.selected_editor_proposal(&path) {
                let line = native::line_hunks(&proposal.original, &proposal.proposed)
                    .first()
                    .map(|hunk| hunk.start_line)
                    .unwrap_or(1);
                self.editor_engine.agent_caret = Some(native::Jump {
                    path: path.clone(),
                    line,
                    column: 1,
                });
            } else {
                self.editor_engine.agent_caret = None;
            }
            self.maybe_begin_pair_apply();
        } else {
            self.focused_editor_buffer = None;
            self.focused_editor_path.clear();
            self.focused_editor_content.clear();
            self.focused_editor_dirty = false;
            self.focused_editor_line = 0;
            self.focused_editor_column = 0;
            self.focused_editor_selection = None;
            self.editor_scroll_line = 0;
            self.editor_scroll_column = 0;
            self.editor_inlays.clear();
            self.editor_diagnostics.clear();
        }
        self.editor = format_editor_buffers(&buffers);
    }

    fn refresh_editor_inlays(&mut self) {
        let path = self.focused_editor_path.clone();
        if path.is_empty() {
            self.editor_inlays.clear();
            return;
        }
        let response = self.workspace.try_lock().ok().and_then(|mut workspace| {
            let name = workspace.language().names().next()?.to_string();
            workspace.language().inlay_hints(&name, "local", &path).ok()
        });
        self.editor_inlays = response
            .map(|response| parse_inlay_hints(&response.result))
            .unwrap_or_default();
    }

    pub fn editor_collaboration_summary(&self) -> String {
        let open_comments = self
            .editor_comments
            .iter()
            .filter(|comment| comment.state == crate::development::EditorCommentState::Open)
            .count();
        let pending = self
            .editor_proposals
            .iter()
            .filter(|proposal| proposal.state == crate::development::EditorProposalState::Pending)
            .count();
        format!(
            "comments {} open · proposals {} pending · checkpoints {}",
            open_comments,
            pending,
            self.editor_checkpoints.len()
        )
    }

    pub fn cycle_editor_buffer(&mut self, delta: i32) {
        let count = self
            .workspace
            .lock()
            .map(|workspace| workspace.project().buffers().count())
            .unwrap_or(0);
        if count == 0 {
            self.status = "No open buffers · open a file from the list".into();
            return;
        }
        let index = self.editor_buffer_index as i32 + delta;
        self.editor_buffer_index = index.rem_euclid(count as i32) as usize;
        let name = self
            .workspace
            .lock()
            .ok()
            .and_then(|workspace| {
                workspace
                    .project()
                    .buffers()
                    .nth(self.editor_buffer_index)
                    .map(|buffer| buffer.path.clone())
            })
            .unwrap_or_default();
        self.refresh_editor_projection();
        self.status = format!("Buffer {}/{} · {name}", self.editor_buffer_index + 1, count);
    }

    pub fn enter_code_edit(&mut self) {
        self.enter_code_edit_with_mode(EditorMode::Insert);
    }

    fn enter_code_edit_normal(&mut self) {
        self.enter_code_edit_with_mode(EditorMode::Normal);
    }

    fn enter_code_edit_with_mode(&mut self, mode: EditorMode) {
        let has_buffer = self.focused_buffer().is_some();
        if !has_buffer {
            self.open_selected_file();
        }
        if self.focused_buffer().is_some() || !self.focused_editor_path.is_empty() {
            self.surface = DevSurface::Code;
            self.composer_mode = false;
            self.composer_code_context_enabled = false;
            self.code_edit_mode = true;
            self.editor_exit_prompt = None;
            self.ensure_editor_cursor_visible();
            match mode {
                EditorMode::Normal => self.editor_engine.enter_normal(),
                _ => self.editor_engine.enter_insert(),
            }
            self.refresh_editor_hunks();
            self.refresh_editor_inlays();
            self.status = match mode {
                EditorMode::Normal => {
                    "NORMAL · arrows/hjkl move · i edit · Ctrl-S save · Esc back".into()
                }
                _ => "INSERT · type to edit · Esc normal · Ctrl-S save".into(),
            };
        }
    }

    pub fn handle_editor_escape(&mut self) {
        if self.editor_engine.overlay.take().is_some()
            || !self.editor_engine.symbols.is_empty()
            || self.editor_engine.ghost.take().is_some()
        {
            self.editor_engine.symbols.clear();
            self.editor_engine.clear_pending();
            self.status = format!("{} · overlay closed", self.editor_engine.mode.label());
            return;
        }
        match self.editor_engine.mode {
            EditorMode::Agent => {
                self.editor_engine.stop_pair_apply();
                self.status =
                    "NORMAL · agent yielded · arrows/hjkl move · i edit · Esc back".into();
            }
            EditorMode::Insert | EditorMode::Select => {
                self.editor_engine.enter_normal();
                self.status = self.editor_normal_status();
            }
            EditorMode::Normal => {
                if self.editor_engine.clear_extra_cursors() {
                    self.status = "NORMAL · extra carets cleared".into();
                    self.refresh_editor_projection();
                    return;
                }
                if !self.focused_editor_dirty {
                    self.close_code_edit();
                    return;
                }
                self.request_editor_exit();
            }
        }
    }

    fn editor_normal_status(&self) -> String {
        let extras = self.editor_engine.extra_caret_count();
        if extras > 0 {
            format!(
                "NORMAL · {} carets · i edit · Ctrl-S save · Esc clears carets",
                extras + 1
            )
        } else {
            "NORMAL · arrows/hjkl move · i edit · Ctrl-S save · Esc back".into()
        }
    }

    pub fn close_code_edit(&mut self) {
        self.code_edit_mode = false;
        self.editor_exit_prompt = None;
        self.surface = DevSurface::Code;
        self.status = "Code navigation · select a file and press Enter to edit".into();
    }

    pub fn request_editor_exit(&mut self) {
        if !self.code_edit_mode {
            return;
        }
        self.refresh_editor_projection();
        let prompt = if self.focused_editor_dirty {
            EditorExitPrompt::Unsaved
        } else {
            EditorExitPrompt::Clean
        };
        self.editor_exit_prompt = Some(prompt);
        self.status = match prompt {
            EditorExitPrompt::Clean => "Exit editor? Enter leaves · Esc stays".into(),
            EditorExitPrompt::Unsaved => {
                "Unsaved changes · S save · D discard · Q discard and quit · Esc stays".into()
            }
        };
    }

    pub fn cancel_editor_exit(&mut self) {
        self.editor_exit_prompt = None;
        self.status = "Still editing · Esc opens exit choices".into();
    }

    pub fn handle_editor_exit_key(&mut self, code: crossterm::event::KeyCode) {
        let Some(prompt) = self.editor_exit_prompt else {
            return;
        };
        match prompt {
            EditorExitPrompt::Clean => match code {
                crossterm::event::KeyCode::Enter
                | crossterm::event::KeyCode::Char('q' | 'Q' | 'y' | 'Y') => {
                    self.close_code_edit();
                }
                crossterm::event::KeyCode::Esc | crossterm::event::KeyCode::Char('n' | 'N') => {
                    self.cancel_editor_exit()
                }
                _ => {}
            },
            EditorExitPrompt::Unsaved => match code {
                crossterm::event::KeyCode::Char('s' | 'S') => match self.save_editor_buffer() {
                    Ok(()) => {
                        let path = self.focused_editor_path.clone();
                        self.refresh_editor_projection();
                        self.close_code_edit();
                        self.status = format!("Saved {path} · editor closed");
                    }
                    Err(error) => {
                        self.status =
                            format!("Save failed: {error} · S retry · D discard · Q quit");
                    }
                },
                crossterm::event::KeyCode::Char('d' | 'D') => match self.discard_editor_buffer() {
                    Ok(()) => {
                        let path = self.focused_editor_path.clone();
                        self.refresh_editor_projection();
                        self.close_code_edit();
                        self.status = format!("Discarded changes in {path} · editor closed");
                    }
                    Err(error) => {
                        self.status =
                            format!("Discard failed: {error} · S save · D retry · Q quit");
                    }
                },
                crossterm::event::KeyCode::Char('q' | 'Q') => {
                    self.quit_confirmation = true;
                    self.status =
                        "Unsaved changes · confirm quit to discard or Esc to keep editing".into();
                }
                crossterm::event::KeyCode::Esc => self.cancel_editor_exit(),
                _ => {}
            },
        }
    }

    fn save_editor_buffer(&mut self) -> Result<(), String> {
        let path = self.focused_editor_path.clone();
        self.workspace
            .try_lock()
            .map_err(|error| error.to_string())?
            .project_mut()
            .save_buffer(&path)
            .map(|_| ())
            .map_err(|error| error.to_string())
    }

    fn discard_editor_buffer(&mut self) -> Result<(), String> {
        let path = self.focused_editor_path.clone();
        self.workspace
            .try_lock()
            .map_err(|error| error.to_string())?
            .project_mut()
            .reload_buffer_from_disk(&path, crate::development::Actor::local())
            .map(|_| ())
            .map_err(|error| error.to_string())
    }

    fn ensure_editor_cursor_visible(&mut self) {
        if self.focused_editor_path.is_empty() {
            return;
        }
        let area = ratatui::layout::Rect::new(0, 0, self.terminal_width, self.terminal_height);
        let rows = super::render::fullscreen_editor_rows(self, area);
        let viewport_height = usize::from(rows[1].height.saturating_sub(2).max(1));
        let viewport_width = usize::from(rows[1].width.saturating_sub(2).max(1));
        if self.editor_soft_wrap {
            let marks = super::file_view::coalesce_editor_marks(self.editor_gutter_marks());
            let notes = self.editor_source_notes();
            let decorations = super::file_view::EditorDecorations {
                marks: &marks,
                inlays: &notes,
                extra_selections: &self.editor_engine.extra_selections,
            };
            let wrapped = super::file_view::render_editable_source_wrapped(
                &self.focused_editor_path,
                &self.focused_editor_content,
                self.focused_editor_line,
                self.focused_editor_column,
                self.focused_editor_selection.as_ref(),
                rows[1].width.saturating_sub(2).max(1),
                &decorations,
            );
            let Some(cursor) = wrapped.cursor else {
                return;
            };
            self.editor_scroll_column = 0;
            if cursor.row < self.editor_scroll_line {
                self.editor_scroll_line = cursor.row;
            } else if cursor.row >= self.editor_scroll_line + viewport_height {
                self.editor_scroll_line = cursor.row + 1 - viewport_height;
            }
            return;
        }
        let lines = self.focused_editor_content.split('\n').collect::<Vec<_>>();
        let cursor_line = self.focused_editor_line.saturating_sub(1) as usize;
        if cursor_line < self.editor_scroll_line {
            self.editor_scroll_line = cursor_line;
        } else if cursor_line >= self.editor_scroll_line + viewport_height {
            self.editor_scroll_line = cursor_line + 1 - viewport_height;
        }
        let marks = super::file_view::coalesce_editor_marks(self.editor_gutter_marks());
        let gutter_width = super::file_view::editor_gutter_prefix_width(lines.len(), &marks);
        let cursor_index = self.focused_editor_column.saturating_sub(1) as usize;
        let source_line = lines.get(cursor_line).copied().unwrap_or_default();
        let prefix = source_line.chars().take(cursor_index).collect::<String>();
        let cursor_column = gutter_width + ratatui::text::Span::raw(prefix).width();
        if cursor_column < self.editor_scroll_column {
            self.editor_scroll_column = cursor_column;
        } else if cursor_column >= self.editor_scroll_column + viewport_width {
            self.editor_scroll_column = cursor_column + 1 - viewport_width;
        }
    }

    pub fn toggle_editor_soft_wrap(&mut self) {
        self.editor_soft_wrap = !self.editor_soft_wrap;
        self.editor_scroll_line = 0;
        self.editor_scroll_column = 0;
        self.ensure_editor_cursor_visible();
        self.status = if self.editor_soft_wrap {
            "Soft wrap ON · source lines reflow to the editor width".into()
        } else {
            "Soft wrap OFF · horizontal scrolling follows source columns".into()
        };
    }

    pub fn edit_code_key(
        &mut self,
        code: crossterm::event::KeyCode,
        modifiers: crossterm::event::KeyModifiers,
    ) {
        if !self.pending_editor_input.is_empty() {
            self.queue_editor_input(PendingEditorInput::Key(code, modifiers));
            return;
        }
        if self.workspace_is_temporarily_busy() {
            self.queue_editor_input(PendingEditorInput::Key(code, modifiers));
            return;
        }
        self.apply_editor_key(code, modifiers);
    }

    fn queue_editor_input(&mut self, input: PendingEditorInput) {
        const MAX_PENDING_EDITOR_KEYS: usize = 4096;
        if self.pending_editor_input.len() < MAX_PENDING_EDITOR_KEYS {
            self.pending_editor_input.push_back(input);
            self.status = format!(
                "Editor input queued · {} event{} waiting for the workspace",
                self.pending_editor_input.len(),
                if self.pending_editor_input.len() == 1 {
                    ""
                } else {
                    "s"
                }
            );
        } else {
            self.status = "Editor input queue is full · wait for the workspace to resume".into();
        }
    }

    pub fn paste_editor_text(&mut self, text: &str) {
        if text.is_empty() {
            return;
        }
        let input = PendingEditorInput::Paste(text.to_string());
        if !self.pending_editor_input.is_empty() {
            self.queue_editor_input(input);
            return;
        }
        if self.workspace_is_temporarily_busy() {
            self.queue_editor_input(input);
            return;
        }
        self.apply_editor_paste(text);
    }

    fn workspace_is_temporarily_busy(&self) -> bool {
        self.workspace
            .try_lock()
            .err()
            .is_some_and(|error| error.to_string().contains("workspace busy"))
    }

    /// Retry queued editor input after a background operation releases the
    /// workspace. Events stay in order, including save and exit keys.
    pub fn flush_pending_editor_input(&mut self) -> bool {
        let preview_processed = self.flush_pending_lsp_preview_result();
        let acceptance_processed = self.flush_pending_editor_acceptance();
        if self.pending_editor_input.is_empty() {
            return preview_processed || acceptance_processed;
        }
        if self.workspace.try_lock().is_err() {
            return preview_processed || acceptance_processed;
        }
        let mut processed = preview_processed || acceptance_processed;
        while let Some(input) = self.pending_editor_input.pop_front() {
            let remaining_before = self.pending_editor_input.len();
            match input {
                PendingEditorInput::Key(code, modifiers) => self.apply_editor_key(code, modifiers),
                PendingEditorInput::Paste(text) => self.apply_editor_paste(&text),
            }
            processed = true;
            if self.pending_editor_input.len() > remaining_before {
                break;
            }
        }
        processed
    }

    fn apply_editor_paste(&mut self, text: &str) {
        let Some(buffer) = self.focused_buffer() else {
            self.status = "No open buffer".into();
            return;
        };
        match self.insert_editor_text(&buffer.path, text) {
            Ok(()) => self.refresh_editor_projection(),
            Err(error) if error.to_string().contains("workspace busy") => {
                self.queue_editor_input(PendingEditorInput::Paste(text.to_string()));
            }
            Err(error) => self.status = format!("Editor paste failed: {error}"),
        }
    }

    fn apply_editor_key(
        &mut self,
        code: crossterm::event::KeyCode,
        modifiers: crossterm::event::KeyModifiers,
    ) {
        if code == crossterm::event::KeyCode::Char('w')
            && modifiers.contains(crossterm::event::KeyModifiers::ALT)
        {
            self.toggle_editor_soft_wrap();
            return;
        }
        if self.editor_engine.mode == EditorMode::Agent {
            self.edit_agent_key(code, modifiers);
            return;
        }
        if self.editor_engine.mode == EditorMode::Insert
            || modifiers.contains(crossterm::event::KeyModifiers::CONTROL)
            || modifiers.contains(crossterm::event::KeyModifiers::ALT)
        {
            self.edit_insert_or_chords(code, modifiers);
            return;
        }
        self.edit_normal_key(code, modifiers);
    }

    fn edit_agent_key(
        &mut self,
        code: crossterm::event::KeyCode,
        modifiers: crossterm::event::KeyModifiers,
    ) {
        match code {
            crossterm::event::KeyCode::Enter => {
                let id = self
                    .editor_engine
                    .pair_apply
                    .as_ref()
                    .map(|apply| apply.proposal_id.clone());
                self.editor_engine.stop_pair_apply();
                if let Some(id) = id {
                    match self.accept_review_proposal(Some(&id)) {
                        Ok(message) => self.status = message,
                        Err(error) => self.status = error,
                    }
                }
            }
            crossterm::event::KeyCode::Char('n') => {
                let id = self
                    .editor_engine
                    .pair_apply
                    .as_ref()
                    .map(|apply| apply.proposal_id.clone());
                self.editor_engine.stop_pair_apply();
                if let Some(id) = id {
                    match self.reject_review_proposal(Some(&id)) {
                        Ok(message) => self.status = message,
                        Err(error) => self.status = error,
                    }
                }
            }
            crossterm::event::KeyCode::Esc => self.handle_editor_escape(),
            _ => {
                self.editor_engine.stop_pair_apply();
                self.editor_engine.enter_insert();
                self.edit_insert_or_chords(code, modifiers);
            }
        }
        self.refresh_editor_projection();
    }

    fn edit_insert_or_chords(
        &mut self,
        code: crossterm::event::KeyCode,
        modifiers: crossterm::event::KeyModifiers,
    ) {
        let Some(buffer) = self.focused_buffer() else {
            self.status = "No open buffer".into();
            return;
        };
        let path = buffer.path.clone();
        if code == crossterm::event::KeyCode::Tab
            && let Some(ghost) = self.editor_engine.ghost.take()
        {
            let text = ghost.text.clone();
            self.accept_ghost_text(ghost, &text);
            return;
        }
        if matches!(
            code,
            crossterm::event::KeyCode::Right | crossterm::event::KeyCode::Char('f')
        ) && modifiers.contains(crossterm::event::KeyModifiers::CONTROL)
            && !modifiers.contains(crossterm::event::KeyModifiers::SHIFT)
            && self.editor_engine.ghost.is_some()
        {
            self.accept_ghost_word();
            return;
        }
        let result = match (code, modifiers) {
            (crossterm::event::KeyCode::Char('a'), value)
                if value.contains(crossterm::event::KeyModifiers::ALT) =>
            {
                self.prepare_editor_agent_prompt();
                Ok(())
            }
            (crossterm::event::KeyCode::Char('s'), value)
                if value.contains(crossterm::event::KeyModifiers::CONTROL) =>
            {
                self.locked(|workspace| workspace.project_mut().save_buffer(&path).map(|_| ()))
                    .unwrap_or_else(|| {
                        Err(crate::development::DevelopmentError::Conflict(
                            "workspace lock failed".into(),
                        ))
                    })
            }
            (crossterm::event::KeyCode::Char('z'), value)
                if value.contains(crossterm::event::KeyModifiers::CONTROL) =>
            {
                self.locked(|workspace| workspace.project_mut().undo_buffer(&path).map(|_| ()))
                    .unwrap_or_else(|| {
                        Err(crate::development::DevelopmentError::Conflict(
                            "workspace lock failed".into(),
                        ))
                    })
            }
            (crossterm::event::KeyCode::Char('y'), value)
                if value.contains(crossterm::event::KeyModifiers::CONTROL) =>
            {
                self.locked(|workspace| workspace.project_mut().redo_buffer(&path).map(|_| ()))
                    .unwrap_or_else(|| {
                        Err(crate::development::DevelopmentError::Conflict(
                            "workspace lock failed".into(),
                        ))
                    })
            }
            (crossterm::event::KeyCode::Char('o'), value)
                if value.contains(crossterm::event::KeyModifiers::CONTROL) =>
            {
                self.open_symbol_picker();
                Ok(())
            }
            (crossterm::event::KeyCode::Left, value) => self.move_editor_cursor(
                &path,
                0,
                -1,
                value.contains(crossterm::event::KeyModifiers::SHIFT),
            ),
            (crossterm::event::KeyCode::Right, value) => self.move_editor_cursor(
                &path,
                0,
                1,
                value.contains(crossterm::event::KeyModifiers::SHIFT),
            ),
            (crossterm::event::KeyCode::Up, value) => self.move_editor_cursor(
                &path,
                -1,
                0,
                value.contains(crossterm::event::KeyModifiers::SHIFT),
            ),
            (crossterm::event::KeyCode::Down, value) => self.move_editor_cursor(
                &path,
                1,
                0,
                value.contains(crossterm::event::KeyModifiers::SHIFT),
            ),
            (crossterm::event::KeyCode::Enter, _) => self.insert_editor_text(&path, "\n"),
            (crossterm::event::KeyCode::Backspace, _) => self.backspace_editor(&path),
            (crossterm::event::KeyCode::Char(character), _)
                if self.editor_engine.mode == EditorMode::Insert =>
            {
                let result = self.insert_editor_text(&path, &character.to_string());
                self.request_ghost_from_line();
                result
            }
            _ => Ok(()),
        };
        if let Err(error) = result {
            if error.to_string().contains("workspace busy") {
                self.queue_editor_input(PendingEditorInput::Key(code, modifiers));
            } else {
                self.status = format!("Editor action failed: {error}");
            }
        }
        self.refresh_editor_projection();
    }

    fn edit_normal_key(
        &mut self,
        code: crossterm::event::KeyCode,
        modifiers: crossterm::event::KeyModifiers,
    ) {
        let Some(buffer) = self.focused_buffer() else {
            self.status = "No open buffer".into();
            return;
        };
        let path = buffer.path.clone();
        let content = buffer.content.clone();
        let cursor = crate::development::TextPosition {
            line: buffer.cursor_line,
            column: buffer.cursor_column,
        };
        if let crossterm::event::KeyCode::Char(digit) = code
            && digit.is_ascii_digit()
            && !(digit == '0' && self.editor_engine.pending_count == 0)
        {
            self.editor_engine
                .push_digit(digit.to_digit(10).unwrap_or(0));
            return;
        }
        let count = self.editor_engine.count();
        if self.editor_engine.pending_find.is_some() {
            if let crossterm::event::KeyCode::Char(needle) = code {
                let jumping_mark = self.editor_engine.pending_find == Some('\'');
                self.editor_engine.pending_find = None;
                if jumping_mark {
                    if let Some(mark) = self.editor_engine.marks.get(&needle).cloned() {
                        self.editor_engine
                            .record_jump(&path, cursor.line, cursor.column);
                        if mark.path == path {
                            let _ = self.set_editor_cursor(
                                &path,
                                crate::development::TextPosition {
                                    line: mark.line,
                                    column: mark.column,
                                },
                                false,
                            );
                        } else {
                            self.status = format!("Mark '{needle}' is {}", mark.path);
                        }
                    } else {
                        self.status = format!("Mark '{needle}' is empty");
                    }
                } else {
                    let motion = Motion::Find {
                        needle,
                        till: self.editor_engine.pending_find_till,
                        reverse: self.editor_engine.pending_find_reverse,
                    };
                    if let Some(operator) = self.editor_engine.pending_operator.take() {
                        let _ = self.apply_operator(&path, &content, cursor, operator, motion);
                    } else {
                        let next = apply_motion(&content, cursor, motion);
                        let _ = self.set_editor_cursor(&path, next, false);
                    }
                }
                self.refresh_editor_projection();
            } else {
                self.editor_engine.clear_pending();
            }
            return;
        }
        if self.editor_engine.pending_mark {
            if let crossterm::event::KeyCode::Char(name) = code {
                self.editor_engine.marks.insert(
                    name,
                    native::Jump {
                        path: path.clone(),
                        line: cursor.line,
                        column: cursor.column,
                    },
                );
                self.status = format!("Mark '{name}' set");
            }
            self.editor_engine.pending_mark = false;
            return;
        }
        if let crossterm::event::KeyCode::Char(character) = code
            && let Some(around) = self.editor_engine.pending_around
        {
            if let Some(object) = textobject_from_key(character, around) {
                self.apply_textobject(&path, &content, cursor, object);
            } else {
                self.editor_engine.clear_pending();
                self.status = format!("Unknown textobject '{character}'");
            }
            return;
        }
        if self.editor_engine.pending_operator.is_some() {
            match code {
                crossterm::event::KeyCode::Char('i') => {
                    self.editor_engine.pending_around = Some(false);
                    self.status =
                        "inner textobject · w word · f fn · a arg · c comment · s string".into();
                    return;
                }
                crossterm::event::KeyCode::Char('a') => {
                    self.editor_engine.pending_around = Some(true);
                    self.status =
                        "around textobject · w word · f fn · a arg · c comment · s string".into();
                    return;
                }
                crossterm::event::KeyCode::Char('f') => {
                    self.apply_textobject(
                        &path,
                        &content,
                        cursor,
                        TextObject::Function { around: true },
                    );
                    return;
                }
                _ => {}
            }
        }
        let motion = match code {
            crossterm::event::KeyCode::Char('h') | crossterm::event::KeyCode::Left => {
                Some(Motion::Left)
            }
            crossterm::event::KeyCode::Char('l') | crossterm::event::KeyCode::Right => {
                Some(Motion::Right)
            }
            crossterm::event::KeyCode::Char('j') | crossterm::event::KeyCode::Down => {
                Some(Motion::Down)
            }
            crossterm::event::KeyCode::Char('k') | crossterm::event::KeyCode::Up => {
                Some(Motion::Up)
            }
            crossterm::event::KeyCode::Char('w') => Some(Motion::WordForward),
            crossterm::event::KeyCode::Char('b') => Some(Motion::WordBackward),
            crossterm::event::KeyCode::Char('e') => Some(Motion::WordEnd),
            crossterm::event::KeyCode::Char('0') => Some(Motion::LineStart),
            crossterm::event::KeyCode::Char('$') => Some(Motion::LineEnd),
            crossterm::event::KeyCode::Char('%') => Some(Motion::MatchPair),
            crossterm::event::KeyCode::Char('G') => Some(Motion::FileEnd),
            _ => None,
        };
        if code == crossterm::event::KeyCode::Char('g') {
            if self.editor_engine.pending_g {
                self.editor_engine.pending_g = false;
                self.apply_editor_motion(&path, &content, cursor, Motion::FileStart, false);
            } else {
                self.editor_engine.pending_g = true;
            }
            return;
        }
        if let Some(motion) = motion {
            if let Some(operator) = self.editor_engine.pending_operator.take() {
                let _ = self.apply_operator(&path, &content, cursor, operator, motion);
            } else {
                let mut position = cursor;
                for _ in 0..count {
                    position = apply_motion(&content, position, motion);
                }
                self.editor_engine.clear_pending();
                let extend = self.editor_engine.mode == EditorMode::Select
                    || modifiers.contains(crossterm::event::KeyModifiers::SHIFT);
                let _ = self.set_editor_cursor(&path, position, extend);
            }
            self.refresh_editor_projection();
            return;
        }
        match code {
            crossterm::event::KeyCode::Char('i') => self.editor_engine.enter_insert(),
            crossterm::event::KeyCode::Char('a') => {
                let next = apply_motion(&content, cursor, Motion::Right);
                let _ = self.set_editor_cursor(&path, next, false);
                self.editor_engine.enter_insert();
            }
            crossterm::event::KeyCode::Char('v') => self.editor_engine.enter_select(),
            crossterm::event::KeyCode::Char('W') => {
                self.apply_textobject(&path, &content, cursor, TextObject::Word { around: true });
            }
            crossterm::event::KeyCode::Char('d') if self.editor_engine.pending_g => {
                self.editor_engine.pending_g = false;
                self.editor_go_to_definition();
            }
            crossterm::event::KeyCode::Char('r') if self.editor_engine.pending_g => {
                self.editor_engine.pending_g = false;
                self.editor_references();
            }
            crossterm::event::KeyCode::Char('p') if self.editor_engine.pending_g => {
                self.editor_engine.pending_g = false;
                self.jump_page_from_source();
            }
            crossterm::event::KeyCode::Char('c') if self.editor_engine.pending_g => {
                self.editor_engine.pending_g = false;
                self.show_comment_thread();
            }
            crossterm::event::KeyCode::Char('m') if self.editor_engine.pending_g => {
                self.editor_engine.pending_g = false;
                self.match_same_objects(&path, &content, cursor, true);
            }
            crossterm::event::KeyCode::Char('n') if self.editor_engine.pending_g => {
                self.editor_engine.pending_g = false;
                self.match_same_objects(&path, &content, cursor, false);
            }
            crossterm::event::KeyCode::Char('d') => {
                self.editor_engine.pending_operator = Some(Operator::Delete);
            }
            crossterm::event::KeyCode::Char('c') => {
                self.editor_engine.pending_operator = Some(Operator::Change);
            }
            crossterm::event::KeyCode::Char('y')
                if !modifiers.contains(crossterm::event::KeyModifiers::CONTROL) =>
            {
                self.editor_engine.pending_operator = Some(Operator::Yank);
            }
            crossterm::event::KeyCode::Char('x') => {
                let _ =
                    self.apply_operator(&path, &content, cursor, Operator::Delete, Motion::Right);
            }
            crossterm::event::KeyCode::Char('u') => {
                let _ =
                    self.locked(|workspace| workspace.project_mut().undo_buffer(&path).map(|_| ()));
            }
            crossterm::event::KeyCode::Char('p') => {
                if !self.editor_engine.yank.is_empty() {
                    let _ = self.insert_editor_text(&path, &self.editor_engine.yank.clone());
                }
            }
            crossterm::event::KeyCode::Char('m') => {
                self.editor_engine.pending_mark = true;
                self.status = "Mark · next character names the mark".into();
            }
            crossterm::event::KeyCode::Char('\'') => {
                self.status = "Jump to mark · next character".into();
                self.editor_engine.pending_find = Some('\'');
            }
            crossterm::event::KeyCode::Char('K') => self.editor_hover(),
            crossterm::event::KeyCode::Char(']') => {
                if let Some(hunk) = self.editor_engine.step_hunk(1).cloned() {
                    let _ = self.set_editor_cursor(
                        &path,
                        crate::development::TextPosition {
                            line: hunk.start_line,
                            column: 1,
                        },
                        false,
                    );
                    self.status = format!(
                        "Hunk {}/{}",
                        self.editor_engine.hunk_index + 1,
                        self.editor_engine.hunks.len()
                    );
                }
            }
            crossterm::event::KeyCode::Char('[') => {
                if let Some(hunk) = self.editor_engine.step_hunk(-1).cloned() {
                    let _ = self.set_editor_cursor(
                        &path,
                        crate::development::TextPosition {
                            line: hunk.start_line,
                            column: 1,
                        },
                        false,
                    );
                }
            }
            crossterm::event::KeyCode::Enter => self.accept_current_hunk(),
            crossterm::event::KeyCode::Char('n') => self.reject_current_hunk(),
            crossterm::event::KeyCode::Char('o') => self.open_symbol_picker(),
            crossterm::event::KeyCode::Char('f') => {
                self.editor_engine.pending_find = Some('f');
                self.editor_engine.pending_find_till = false;
                self.editor_engine.pending_find_reverse = false;
                self.status = "find · next character".into();
            }
            crossterm::event::KeyCode::Char('t') => {
                self.editor_engine.pending_find = Some('t');
                self.editor_engine.pending_find_till = true;
                self.editor_engine.pending_find_reverse = false;
                self.status = "till · next character".into();
            }
            crossterm::event::KeyCode::Char('F') => {
                self.editor_engine.pending_find = Some('F');
                self.editor_engine.pending_find_till = false;
                self.editor_engine.pending_find_reverse = true;
                self.status = "find back · next character".into();
            }
            crossterm::event::KeyCode::Char('T') => {
                self.editor_engine.pending_find = Some('T');
                self.editor_engine.pending_find_till = true;
                self.editor_engine.pending_find_reverse = true;
                self.status = "till back · next character".into();
            }
            _ => {}
        }
        self.refresh_editor_projection();
    }

    fn set_editor_cursor(
        &mut self,
        path: &str,
        position: crate::development::TextPosition,
        extend: bool,
    ) -> crate::development::DevelopmentResult<()> {
        let buffer = self
            .workspace
            .try_lock()?
            .project()
            .buffer(path)
            .cloned()
            .ok_or_else(|| {
                crate::development::DevelopmentError::NotFound(format!("buffer {path}"))
            })?;
        let position = native::clamp_position(&buffer.content, position);
        let anchor = if extend {
            buffer
                .selection
                .map(|selection| selection.anchor)
                .unwrap_or(crate::development::TextPosition {
                    line: buffer.cursor_line,
                    column: buffer.cursor_column,
                })
        } else {
            position
        };
        let mut workspace = self.workspace.try_lock()?;
        workspace
            .project_mut()
            .set_buffer_cursor(path, position.line, position.column)?;
        workspace.project_mut().set_buffer_selection(
            path,
            extend.then_some(crate::development::TextSelection {
                anchor,
                active: position,
            }),
            crate::development::Actor::local(),
        )?;
        Ok(())
    }

    fn apply_editor_motion(
        &mut self,
        path: &str,
        content: &str,
        cursor: crate::development::TextPosition,
        motion: Motion,
        extend: bool,
    ) {
        let next = apply_motion(content, cursor, motion);
        let _ = self.set_editor_cursor(path, next, extend);
    }

    fn apply_textobject(
        &mut self,
        path: &str,
        content: &str,
        cursor: crate::development::TextPosition,
        object: TextObject,
    ) {
        let selection = self
            .syntax
            .textobject(path, content, cursor, object)
            .or_else(|| textobject_selection(content, cursor, object));
        let Some(selection) = selection else {
            self.editor_engine.clear_pending();
            self.status = "No textobject under cursor".into();
            return;
        };
        self.editor_engine.last_textobject = Some(object);
        if let Some(operator) = self.editor_engine.pending_operator.take() {
            let mut ranges = vec![selection];
            for extra in self.editor_engine.extra_selections.clone() {
                let origin = extra.active;
                if let Some(range) = self
                    .syntax
                    .textobject(path, content, origin, object)
                    .or_else(|| textobject_selection(content, origin, object))
                {
                    ranges.push(range);
                }
            }
            if let Err(error) = self.apply_operator_ranges(path, content, operator, ranges) {
                self.status = format!("Operator failed: {error}");
            }
        } else {
            self.editor_engine.enter_select();
            let _ = self.set_editor_cursor(path, selection.anchor, false);
            let _ = self.set_editor_cursor(path, selection.active, true);
            self.status = format!("{} · textobject selected", self.editor_engine.mode.label());
        }
        self.refresh_editor_projection();
    }

    fn match_same_objects(
        &mut self,
        path: &str,
        content: &str,
        cursor: crate::development::TextPosition,
        all: bool,
    ) {
        let object = self
            .editor_engine
            .last_textobject
            .unwrap_or(TextObject::Word { around: false });
        let mut ranges = if matches!(object, TextObject::Word { .. }) {
            same_word_ranges(
                content,
                cursor,
                matches!(object, TextObject::Word { around: true }),
            )
        } else {
            let mut found = self.syntax.same_textobjects(path, content, cursor, object);
            if found.is_empty() {
                found = textobject_selection(content, cursor, object)
                    .into_iter()
                    .collect();
            }
            found
        };
        if ranges.is_empty() {
            self.status = "No matching objects under the caret".into();
            return;
        }
        ranges.sort_by_key(|selection| (selection.anchor.line, selection.anchor.column));
        let primary_index = ranges
            .iter()
            .position(|selection| selection_covers(content, selection, cursor))
            .unwrap_or(0);
        if all {
            let primary = ranges.remove(primary_index.min(ranges.len().saturating_sub(1)));
            self.editor_engine.extra_selections = ranges;
            let _ = self.set_editor_cursor(path, primary.anchor, false);
            let _ = self.set_editor_cursor(path, primary.active, true);
            self.editor_engine.last_textobject = Some(object);
            self.status = self.editor_normal_status();
        } else {
            let extras = &self.editor_engine.extra_selections;
            let next = ranges
                .iter()
                .cycle()
                .skip(primary_index + 1)
                .take(ranges.len().saturating_sub(1))
                .find(|candidate| {
                    !extras.iter().any(|existing| {
                        existing.anchor == candidate.anchor && existing.active == candidate.active
                    })
                })
                .cloned();
            if let Some(next) = next {
                self.editor_engine.extra_selections.push(next);
                self.editor_engine.last_textobject = Some(object);
                self.status = self.editor_normal_status();
            } else {
                self.status = "No further matching objects".into();
            }
        }
        self.refresh_editor_projection();
    }

    fn apply_operator(
        &mut self,
        path: &str,
        content: &str,
        cursor: crate::development::TextPosition,
        operator: Operator,
        motion: Motion,
    ) -> crate::development::DevelopmentResult<()> {
        let mut ranges = vec![TextSelection {
            anchor: cursor,
            active: apply_motion(content, cursor, motion),
        }];
        for extra in &self.editor_engine.extra_selections {
            let origin = extra.active;
            ranges.push(TextSelection {
                anchor: origin,
                active: apply_motion(content, origin, motion),
            });
        }
        self.apply_operator_ranges(path, content, operator, ranges)
    }

    fn apply_operator_ranges(
        &mut self,
        path: &str,
        content: &str,
        operator: Operator,
        ranges: Vec<TextSelection>,
    ) -> crate::development::DevelopmentResult<()> {
        let mut offsets = ranges
            .iter()
            .filter_map(|selection| {
                crate::development::editor::selection_offsets(content, selection)
            })
            .collect::<Vec<_>>();
        offsets.sort_by_key(|(start, _)| *start);
        offsets.dedup();
        if offsets.is_empty() {
            self.editor_engine.clear_pending();
            return Ok(());
        }
        if !self.auto_checkpoint("before-operator") {
            return Err(crate::development::DevelopmentError::Conflict(
                self.status.clone(),
            ));
        }
        match operator {
            Operator::Yank => {
                self.editor_engine.yank = offsets
                    .iter()
                    .map(|(start, stop)| content[*start..*stop].to_string())
                    .collect::<Vec<_>>()
                    .join("\n");
                self.status = format!("Yanked {} bytes", self.editor_engine.yank.len());
            }
            Operator::Delete | Operator::Change => {
                let (next, carets) = multi_delete(content, &offsets);
                let mut extras = Vec::new();
                let mut primary = crate::development::TextPosition { line: 1, column: 1 };
                for (index, offset) in carets.into_iter().enumerate() {
                    let position =
                        crate::development::editor::text_position_at_offset(&next, offset)
                            .unwrap_or(primary);
                    if index == 0 {
                        primary = position;
                    } else {
                        extras.push(TextSelection::collapsed(position));
                    }
                }
                let actor = crate::development::Actor::local();
                let mut workspace = self.workspace.try_lock()?;
                workspace
                    .project_mut()
                    .edit_buffer(path, next, actor.clone())?;
                workspace
                    .project_mut()
                    .set_buffer_cursor(path, primary.line, primary.column)?;
                workspace
                    .project_mut()
                    .set_buffer_selection(path, None, actor)?;
                drop(workspace);
                self.editor_engine.extra_selections = extras;
                if operator == Operator::Change {
                    self.editor_engine.enter_insert();
                }
            }
        }
        self.editor_engine.clear_pending();
        Ok(())
    }

    fn maybe_begin_pair_apply(&mut self) {
        if self.editor_engine.pair_apply.is_some() {
            return;
        }
        let path = self.focused_editor_path.clone();
        if path.is_empty() {
            return;
        }
        let Some(proposal) = self
            .editor_proposals
            .iter()
            .find(|item| {
                item.path == path
                    && item.state == crate::development::EditorProposalState::Pending
                    && item.original == self.focused_editor_content
                    && item.original != item.proposed
            })
            .cloned()
        else {
            return;
        };
        self.editor_engine.begin_pair_apply(
            proposal.id,
            proposal.actor.name.clone(),
            proposal.path,
            proposal.original,
            proposal.proposed,
        );
        self.status = format!(
            "AGENT · {} pair-apply · Enter accept · n reject · type to yield",
            proposal.actor.name
        );
    }

    pub fn tick_pair_apply(&mut self) -> bool {
        let Some(apply) = self.editor_engine.pair_apply.clone() else {
            return false;
        };
        if self.focused_editor_content == apply.proposed {
            return false;
        }
        let (content, revealed, _done) =
            pair_apply_step(&apply.original, &apply.proposed, apply.revealed, 24);
        let path = apply.path.clone();
        let actor = apply.actor.clone();
        let _ = self.locked(|workspace| {
            workspace
                .project_mut()
                .edit_buffer(
                    &path,
                    content.clone(),
                    crate::development::Actor::embedded(),
                )
                .map(|_| ())
        });
        if let Some(live) = self.editor_engine.pair_apply.as_mut() {
            live.revealed = revealed;
        }
        self.focused_editor_content = content.clone();
        self.focused_editor_dirty = true;
        let caret_offset = pair_apply_caret(&apply.original, &apply.proposed, revealed);
        if let Some(position) =
            crate::development::editor::text_position_at_offset(&content, caret_offset)
        {
            self.focused_editor_line = position.line;
            self.focused_editor_column = position.column;
            self.editor_engine.agent_caret = Some(native::Jump {
                path,
                line: position.line,
                column: position.column,
            });
            let _ = self.set_editor_cursor(&self.focused_editor_path.clone(), position, false);
        }
        self.editor_engine.mode = EditorMode::Agent;
        self.status = format!("AGENT · {actor} pair-apply · Enter accept · n reject");
        self.ensure_editor_cursor_visible();
        true
    }

    pub fn editor_gutter_marks(&self) -> Vec<(u32, native::GutterMark)> {
        let mut marks = Vec::new();
        let path = &self.focused_editor_path;
        for diagnostic in &self.editor_diagnostics {
            if diagnostic_matches_path(diagnostic, path) {
                marks.push((
                    diagnostic.start.line.saturating_add(1),
                    native::GutterMark::Lsp,
                ));
            }
        }
        let page_lines = self.page_source_lines(path);
        match self.last_proof_ok {
            Some(true) => {
                for line in &page_lines {
                    marks.push((*line, native::GutterMark::Proof));
                }
            }
            Some(false) => {
                for line in &page_lines {
                    marks.push((*line, native::GutterMark::Lsp));
                }
            }
            None => {}
        }
        for hunk in &self.editor_engine.hunks {
            marks.push((hunk.start_line, native::GutterMark::Git));
        }
        if let Some(caret) = &self.editor_engine.agent_caret
            && caret.path == *path
        {
            marks.push((caret.line, native::GutterMark::Agent));
        }
        for line in page_lines {
            marks.push((line, native::GutterMark::Page));
        }
        for comment in &self.editor_comments {
            if comment.state == crate::development::EditorCommentState::Open
                && (path.is_empty() || comment.path == *path)
            {
                marks.push((comment.start_line, native::GutterMark::Comment));
            }
        }
        marks
    }

    pub fn editor_source_notes(&self) -> Vec<(u32, String)> {
        let mut by_line = std::collections::BTreeMap::<u32, Vec<String>>::new();
        for (line, hint) in &self.editor_inlays {
            by_line.entry(*line).or_default().push(hint.clone());
        }
        let path = &self.focused_editor_path;
        for diagnostic in &self.editor_diagnostics {
            if diagnostic_matches_path(diagnostic, path) {
                by_line
                    .entry(diagnostic.start.line.saturating_add(1))
                    .or_default()
                    .push(format!("! {}", truncate_note(&diagnostic.message)));
            }
        }
        for comment in &self.editor_comments {
            if comment.state == crate::development::EditorCommentState::Open
                && (path.is_empty() || comment.path == *path)
            {
                by_line
                    .entry(comment.start_line)
                    .or_default()
                    .push(format!("# {}", truncate_note(&comment.text)));
            }
        }
        by_line
            .into_iter()
            .map(|(line, parts)| (line, parts.join("  ")))
            .collect()
    }

    fn page_source_lines(&self, path: &str) -> Vec<u32> {
        self.workspace
            .try_lock()
            .ok()
            .map(|workspace| {
                workspace
                    .project()
                    .graph()
                    .entities_for_source(path, None)
                    .into_iter()
                    .map(|link| link.source.start_line)
                    .collect()
            })
            .unwrap_or_default()
    }

    fn show_comment_thread(&mut self) {
        let path = self.focused_editor_path.clone();
        let line = self.focused_editor_line.max(1);
        let thread = self
            .editor_comments
            .iter()
            .filter(|comment| {
                comment.state == crate::development::EditorCommentState::Open
                    && (path.is_empty() || comment.path == path)
                    && line >= comment.start_line
                    && line <= comment.end_line
            })
            .map(|comment| {
                format!(
                    "{} · {}:{}\n  {}",
                    comment.actor.name, comment.path, comment.start_line, comment.text
                )
            })
            .collect::<Vec<_>>();
        if thread.is_empty() {
            self.status = "No open comment on this line · :editor comment-selection".into();
            return;
        }
        self.editor_engine.overlay = Some(thread.join("\n\n"));
        self.status = "Comment thread · Esc closes".into();
    }

    fn refresh_editor_hunks(&mut self) {
        let path = self.focused_editor_path.clone();
        if path.is_empty() {
            return;
        }
        let proposal = self.selected_editor_proposal(&path).cloned();
        if let Some(proposal) = proposal {
            self.editor_engine
                .set_hunks(line_hunks(&proposal.original, &proposal.proposed));
        } else {
            self.editor_engine.set_hunks(Vec::new());
        }
    }

    fn selected_editor_proposal(&self, path: &str) -> Option<&crate::development::EditorProposal> {
        if let Some(proposal_id) = &self.selected_editor_proposal_id {
            self.editor_proposals.iter().find(|proposal| {
                proposal.id == *proposal_id
                    && proposal.path == path
                    && proposal.state == crate::development::EditorProposalState::Pending
            })
        } else {
            self.editor_proposals.iter().find(|proposal| {
                proposal.path == path
                    && proposal.state == crate::development::EditorProposalState::Pending
            })
        }
    }

    fn reconcile_selected_editor_proposal(&mut self) {
        let selected_is_pending = self.selected_editor_proposal_id.as_ref().is_none_or(|id| {
            self.editor_proposals.iter().any(|proposal| {
                proposal.id == *id
                    && proposal.state == crate::development::EditorProposalState::Pending
            })
        });
        if !selected_is_pending {
            self.selected_editor_proposal_id = None;
            self.editor_engine.set_hunks(Vec::new());
            self.editor_engine.agent_caret = None;
        }
    }

    fn auto_checkpoint(&mut self, name: &str) -> bool {
        let _ = self.locked(|workspace| {
            workspace
                .project_mut()
                .create_editor_checkpoint(name.to_string(), crate::development::Actor::local())
                .map(|_| ())
        });
        true
    }

    fn editor_hover(&mut self) {
        let path = self.focused_editor_path.clone();
        let line = self.focused_editor_line.saturating_sub(1);
        let character = self.focused_editor_column.saturating_sub(1);
        match self.locked(|workspace| {
            let server = workspace
                .language()
                .names()
                .next()
                .ok_or_else(|| {
                    crate::development::DevelopmentError::NotFound("language server".into())
                })?
                .to_string();
            workspace
                .language()
                .hover(&server, "local", &path, line, character)
        }) {
            Some(Ok(response)) => {
                self.editor_engine.overlay = Some(response.result.to_string());
                self.status = "Hover · Esc closes".into();
            }
            Some(Err(error)) => self.status = format!("Hover unavailable: {error}"),
            None => self.status = "Hover unavailable · workspace busy".into(),
        }
    }

    fn editor_references(&mut self) {
        let path = self.focused_editor_path.clone();
        let line = self.focused_editor_line.saturating_sub(1);
        let character = self.focused_editor_column.saturating_sub(1);
        match self.locked(|workspace| {
            let server = workspace
                .language()
                .names()
                .next()
                .ok_or_else(|| {
                    crate::development::DevelopmentError::NotFound("language server".into())
                })?
                .to_string();
            workspace
                .language()
                .references(&server, "local", &path, line, character)
        }) {
            Some(Ok(response)) => {
                self.editor_engine.overlay = Some(format_lsp_locations(&response.result));
                self.status = "References · Esc closes".into();
            }
            Some(Err(error)) => self.status = format!("References unavailable: {error}"),
            None => self.status = "References unavailable · workspace busy".into(),
        }
    }

    fn editor_go_to_definition(&mut self) {
        let path = self.focused_editor_path.clone();
        let line = self.focused_editor_line.saturating_sub(1);
        let character = self.focused_editor_column.saturating_sub(1);
        self.editor_engine
            .record_jump(&path, self.focused_editor_line, self.focused_editor_column);
        match self.locked(|workspace| {
            let server = workspace
                .language()
                .names()
                .next()
                .ok_or_else(|| {
                    crate::development::DevelopmentError::NotFound("language server".into())
                })?
                .to_string();
            workspace
                .language()
                .definition(&server, "local", &path, line, character)
        }) {
            Some(Ok(response)) => {
                self.editor_engine.overlay = Some(format!("definition {}", response.result));
                if let Some(target) = parse_lsp_location(&response.result) {
                    let _ = self.open_path(&target.path);
                    let _ = self.set_editor_cursor(
                        &self.focused_editor_path.clone(),
                        crate::development::TextPosition {
                            line: target.line.max(1),
                            column: target.column.max(1),
                        },
                        false,
                    );
                }
                self.status = "Definition · Ctrl-O symbols · Enter jump".into();
            }
            Some(Err(error)) => self.status = format!("Definition unavailable: {error}"),
            None => self.status = "Definition unavailable · workspace busy".into(),
        }
    }

    fn open_symbol_picker(&mut self) {
        let path = self.focused_editor_path.clone();
        match self.locked(|workspace| {
            let server = workspace
                .language()
                .names()
                .next()
                .ok_or_else(|| {
                    crate::development::DevelopmentError::NotFound("language server".into())
                })?
                .to_string();
            workspace
                .language()
                .document_symbols(&server, "local", &path)
        }) {
            Some(Ok(response)) => {
                self.editor_engine.symbols = parse_lsp_symbols(&response.result);
                self.editor_engine.symbol_selection = 0;
                self.status = format!(
                    "Symbols · {} · Enter jumps",
                    self.editor_engine.symbols.len()
                );
            }
            _ => {
                self.editor_engine.symbols = self
                    .focused_editor_content
                    .lines()
                    .enumerate()
                    .filter(|(_, line)| {
                        let trimmed = line.trim_start();
                        trimmed.starts_with("fn ")
                            || trimmed.starts_with("struct ")
                            || trimmed.starts_with("impl ")
                            || trimmed.starts_with("pub ")
                    })
                    .map(|(index, line)| (line.trim().to_string(), index as u32 + 1))
                    .collect();
                self.status = format!("Symbols (lexical) · {}", self.editor_engine.symbols.len());
            }
        }
    }

    fn accept_current_hunk(&mut self) {
        let path = self.focused_editor_path.clone();
        let Some(proposal) = self.selected_editor_proposal(&path).cloned() else {
            self.status = "No proposal hunk on this buffer".into();
            return;
        };
        if !self.auto_checkpoint("before-hunk-accept") {
            return;
        }
        match self.locked(|workspace| {
            workspace
                .project_mut()
                .accept_editor_proposal(&proposal.id, crate::development::Actor::local())
                .map(|_| ())
        }) {
            Some(Ok(())) => {
                self.status = format!("Accepted proposal {}", proposal.id);
                self.refresh_editor_hunks();
            }
            Some(Err(error)) => self.status = format!("Accept failed: {error}"),
            None => self.status = "Accept failed · workspace busy".into(),
        }
    }

    fn reject_current_hunk(&mut self) {
        let path = self.focused_editor_path.clone();
        let Some(proposal) = self.selected_editor_proposal(&path).cloned() else {
            return;
        };
        let _ = self.locked(|workspace| {
            workspace
                .project_mut()
                .reject_editor_proposal(&proposal.id, crate::development::Actor::local())
                .map(|_| ())
        });
        self.status = format!("Rejected proposal {}", proposal.id);
        self.refresh_editor_hunks();
    }

    pub fn jump_page_from_source(&mut self) {
        let path = self.focused_editor_path.clone();
        if path.is_empty() {
            self.status = "Open a buffer, then gp jumps to App".into();
            return;
        }
        let line = self.focused_editor_line.max(1);
        let entity = self
            .locked(|workspace| {
                let _ = workspace.project_mut().discover_runtime_links();
                workspace
                    .project()
                    .graph()
                    .entities_for_source(&path, Some(line))
                    .first()
                    .map(|link| link.entity_id.clone())
            })
            .flatten();
        if let Some(entity) = entity.clone() {
            self.pending_page_entity = Some(entity.clone());
            if self.select_page_entity(&entity) {
                self.surface = DevSurface::App;
                self.code_edit_mode = false;
                self.status = format!("Page bound · entity {entity}");
            }
        }
        let route = inferred_app_path(&path);
        let Some(base) = self.resolved_app_url() else {
            if entity.is_none() {
                self.status = "No App URL · start the detected suite, then gp".into();
            }
            return;
        };
        let url = join_app_url(&base, route.as_deref());
        let current = self.browser_workspace.state().url.clone();
        let current_origin = current.trim_end_matches('/');
        if entity.is_some()
            && !current.is_empty()
            && (current == url || current.starts_with(&url) || url.starts_with(current_origin))
        {
            self.surface = DevSurface::App;
            self.code_edit_mode = false;
            return;
        }
        if !self.auto_checkpoint("before-app-jump") {
            return;
        }
        match self.prepare_browser_navigation(&url) {
            Ok(message) => {
                self.status = if entity.is_some() {
                    format!("{message} · will bind page entity")
                } else {
                    message
                };
            }
            Err(error) => self.status = error,
        }
    }

    pub fn jump_source_from_page(&mut self) {
        let Some(entity) = self.browser_workspace.state().selected().cloned() else {
            self.status = "Select an App entity, then g jumps to source".into();
            return;
        };
        let reference = entity.reference.clone();
        let name = entity.name.clone();
        let link = self
            .locked(|workspace| {
                let _ = workspace.project_mut().discover_runtime_links();
                let graph = workspace.project().graph();
                graph
                    .best_link(&reference)
                    .cloned()
                    .or_else(|| graph.best_link(&name).cloned())
                    .or_else(|| {
                        graph
                            .links
                            .values()
                            .flatten()
                            .find(|link| {
                                reference.ends_with(&link.entity_id)
                                    || name.contains(&link.entity_id)
                            })
                            .cloned()
                    })
            })
            .flatten();
        let Some(link) = link else {
            self.status = format!("No source link for {reference} · :project graph discover");
            return;
        };
        self.open_source_at(&link.source.path, link.source.start_line);
    }

    pub fn attach_detected_app(&mut self) -> Result<String, String> {
        let url = self
            .resolved_app_url()
            .ok_or_else(|| "No App URL · start the detected suite".to_string())?;
        if !self.auto_checkpoint("before-app-attach") {
            return Err(self.status.clone());
        }
        self.prepare_browser_navigation(&url)
    }

    pub fn resolved_app_url(&self) -> Option<String> {
        self.process_urls
            .iter()
            .find(|url| !url.is_empty())
            .cloned()
            .or_else(|| {
                self.workspace.try_lock().ok().and_then(|workspace| {
                    let detection = workspace.project().detection();
                    detection.browser_url.clone().or_else(|| {
                        detection
                            .local_development_urls
                            .iter()
                            .find(|url| !url.is_empty())
                            .cloned()
                    })
                })
            })
            .map(|url| join_app_url(&url, None))
    }

    fn select_page_entity(&mut self, entity_id: &str) -> bool {
        let index = self
            .browser_workspace
            .state()
            .entities
            .iter()
            .position(|entity| {
                entity.reference == entity_id
                    || entity.reference.ends_with(entity_id)
                    || entity.name == entity_id
                    || (!entity_id.is_empty() && entity.name.contains(entity_id))
            });
        let Some(index) = index else {
            return false;
        };
        self.browser_workspace.state_mut().selected_entity = Some(index);
        self.browser = self.browser_workspace_summary();
        true
    }

    fn open_source_at(&mut self, path: &str, line: u32) {
        match self.open_path(path) {
            Ok(_) => {
                let focused = self.focused_editor_path.clone();
                let _ = self.set_editor_cursor(
                    &focused,
                    crate::development::TextPosition {
                        line: line.max(1),
                        column: 1,
                    },
                    false,
                );
                self.refresh_editor_projection();
                self.ensure_editor_cursor_visible();
                self.status = format!("Source · {path}:{line}");
            }
            Err(error) => self.status = format!("Source jump failed: {error}"),
        }
    }

    fn accept_ghost_text(&mut self, ghost: GhostText, text: &str) {
        if !self.ghost_matches_current_buffer(&ghost) {
            self.editor_engine.ghost = None;
            self.status = "Stale suggestion discarded · request again at the current cursor".into();
            return;
        }
        let path = ghost.path;
        self.editor_engine.ghost = None;
        let _ = self.insert_editor_text(&path, text);
        self.refresh_editor_projection();
        self.advance_next_edit();
    }

    fn accept_ghost_word(&mut self) {
        let Some(ghost) = self.editor_engine.ghost.take() else {
            return;
        };
        if !self.ghost_matches_current_buffer(&ghost) {
            self.status = "Stale suggestion discarded · request again at the current cursor".into();
            return;
        }
        let parts =
            split_ghost_word(&ghost.text).map(|(word, rest)| (word.to_string(), rest.to_string()));
        match parts {
            Some((word, rest)) if !rest.is_empty() => {
                let _ = self.insert_editor_text(&ghost.path, &word);
                self.refresh_editor_projection();
                self.install_current_ghost(rest);
                self.status = "Ghost word accepted · Tab rest · Ctrl-Right another word".into();
            }
            Some((word, _)) => self.accept_ghost_text(ghost, &word),
            None => {
                let text = ghost.text.clone();
                self.accept_ghost_text(ghost, &text);
            }
        }
    }

    fn advance_next_edit(&mut self) {
        let content = self.focused_editor_content.clone();
        let caret = crate::development::editor::text_position_offset(
            &content,
            crate::development::TextPosition {
                line: self.focused_editor_line.max(1),
                column: self.focused_editor_column.max(1),
            },
        )
        .unwrap_or(content.len());
        let Some(offset) = next_edit_after_accept(&content, caret) else {
            self.request_ghost_from_line();
            self.status = if self.editor_engine.ghost.is_some() {
                "Ghost accepted · next edit here".into()
            } else {
                "Ghost accepted".into()
            };
            return;
        };
        let Some(position) = crate::development::editor::text_position_at_offset(&content, offset)
        else {
            self.request_ghost_from_line();
            self.status = "Ghost accepted".into();
            return;
        };
        let path = self.focused_editor_path.clone();
        self.editor_engine
            .record_jump(&path, position.line, position.column);
        let _ = self.set_editor_cursor(&path, position, false);
        self.refresh_editor_projection();
        self.request_ghost_from_line();
        self.status = format!(
            "Ghost accepted · next edit {}:{}",
            position.line, position.column
        );
    }

    fn current_editor_revision(&self) -> Option<u64> {
        let workspace = self.workspace.try_lock().ok()?;
        let project = workspace.project();
        let buffer = project.buffer(&self.focused_editor_path)?;
        if buffer.content != self.focused_editor_content
            || buffer.cursor_line != self.focused_editor_line
            || buffer.cursor_column != self.focused_editor_column
        {
            return None;
        }
        Some(project.revision())
    }

    fn ghost_matches_current_buffer(&self, ghost: &GhostText) -> bool {
        if ghost.path != self.focused_editor_path {
            return false;
        }
        let Some(offset) = crate::development::editor::text_position_offset(
            &self.focused_editor_content,
            crate::development::TextPosition {
                line: self.focused_editor_line.max(1),
                column: self.focused_editor_column.max(1),
            },
        ) else {
            return false;
        };
        offset == ghost.offset && self.current_editor_revision() == Some(ghost.revision)
    }

    pub fn visible_ghost_text(&self) -> Option<&str> {
        let ghost = self.editor_engine.ghost.as_ref()?;
        self.ghost_matches_current_buffer(ghost)
            .then_some(ghost.text.as_str())
    }

    fn install_ghost_text(
        &mut self,
        path: &str,
        offset: usize,
        revision: u64,
        text: String,
    ) -> bool {
        match crate::fim::validate_ghost_text(&text) {
            Ok(Some(text)) => {
                self.editor_engine.ghost = Some(GhostText {
                    text,
                    path: path.to_string(),
                    offset,
                    revision,
                });
                true
            }
            Ok(None) => false,
            Err(error) => {
                self.editor_engine.ghost = None;
                self.status = format!("Suggestion rejected · {error}");
                false
            }
        }
    }

    fn install_current_ghost(&mut self, text: String) -> bool {
        let path = self.focused_editor_path.clone();
        let Some(offset) = crate::development::editor::text_position_offset(
            &self.focused_editor_content,
            crate::development::TextPosition {
                line: self.focused_editor_line.max(1),
                column: self.focused_editor_column.max(1),
            },
        ) else {
            return false;
        };
        let Some(revision) = self.current_editor_revision() else {
            return false;
        };
        self.install_ghost_text(&path, offset, revision, text)
    }

    pub fn request_ghost_from_line(&mut self) {
        let path = self.focused_editor_path.clone();
        let Some(offset) = crate::development::editor::text_position_offset(
            &self.focused_editor_content,
            crate::development::TextPosition {
                line: self.focused_editor_line.max(1),
                column: self.focused_editor_column.max(1),
            },
        ) else {
            return;
        };
        let Some(revision) = self.current_editor_revision() else {
            self.editor_engine.ghost = None;
            return;
        };
        if self.editor_engine.ghost.as_ref().is_some_and(|ghost| {
            ghost.path != path || ghost.offset != offset || ghost.revision != revision
        }) {
            self.editor_engine.ghost = None;
        }
        if let Some(text) = local_fim(&self.focused_editor_content, offset) {
            self.install_ghost_text(&path, offset, revision, text);
            return;
        }
        if self.take_ready_fim(&path, offset, revision) {
            return;
        }
        self.spawn_hosted_fim(&path, offset, revision);
        let line = self.focused_editor_line.saturating_sub(1);
        let character = self.focused_editor_column.saturating_sub(1);
        if let Some(text) = self.lsp_ghost_insert(&path, line, character) {
            self.install_ghost_text(&path, offset, revision, text);
            return;
        }
        let line = self
            .focused_editor_content
            .lines()
            .nth(self.focused_editor_line.saturating_sub(1) as usize)
            .unwrap_or("")
            .trim()
            .to_string();
        if line.is_empty() {
            return;
        }
        let ghost = if line.ends_with('{') {
            "\n    \n}"
        } else if line.contains("todo") || line.contains("TODO") {
            " // implemented"
        } else {
            return;
        };
        self.install_ghost_text(&path, offset, revision, ghost.into());
    }

    pub fn tick_fim(&mut self) -> bool {
        let path = self.focused_editor_path.clone();
        let Some(offset) = crate::development::editor::text_position_offset(
            &self.focused_editor_content,
            crate::development::TextPosition {
                line: self.focused_editor_line.max(1),
                column: self.focused_editor_column.max(1),
            },
        ) else {
            return false;
        };
        let Some(revision) = self.current_editor_revision() else {
            return false;
        };
        self.take_ready_fim(&path, offset, revision)
    }

    fn take_ready_fim(&mut self, path: &str, offset: usize, revision: u64) -> bool {
        let stale = self.pending_fim.as_ref().is_some_and(|pending| {
            pending.path() != path || pending.offset() != offset || pending.revision() != revision
        });
        if stale {
            self.pending_fim = None;
            self.status = "Stale suggestion discarded · request again at the current cursor".into();
            return false;
        }
        let resident = match &self.pending_fim {
            Some(PendingFim::Resident {
                agent_id,
                since,
                path,
                offset,
                revision,
                request_id,
            }) => Some((
                agent_id.clone(),
                *since,
                request_id.clone(),
                path.clone(),
                *offset,
                *revision,
            )),
            _ => None,
        };
        if let Some((agent_id, since, request_id, path, offset, revision)) = resident {
            let response = self
                .locked(|workspace| {
                    let events = workspace.agents().history(since).ok()?;
                    matching_resident_fim_response(&events, &agent_id, &request_id)
                })
                .flatten();
            return match response {
                Some(Ok(Some(text))) => {
                    self.pending_fim = None;
                    self.install_ghost_text(&path, offset, revision, text)
                }
                Some(Err(error)) => {
                    self.pending_fim = None;
                    self.status = format!("Suggestion rejected · {error}");
                    false
                }
                Some(Ok(None)) => {
                    self.pending_fim = None;
                    false
                }
                None => false,
            };
        }
        let Some(PendingFim::Thread {
            path,
            offset,
            revision,
            rx,
        }) = self.pending_fim.as_mut()
        else {
            return false;
        };
        let (path, offset, revision) = (path.clone(), *offset, *revision);
        match rx.try_recv() {
            Ok(Ok(text)) if !text.is_empty() => {
                self.pending_fim = None;
                self.install_ghost_text(&path, offset, revision, text)
            }
            Ok(Err(error)) => {
                self.pending_fim = None;
                self.status = format!("Suggestion rejected · {error}");
                false
            }
            Ok(Ok(_)) | Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                self.pending_fim = None;
                false
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => false,
        }
    }

    fn spawn_hosted_fim(&mut self, path: &str, offset: usize, revision: u64) {
        if self.pending_fim.as_ref().is_some_and(|pending| {
            pending.path() == path && pending.offset() == offset && pending.revision() == revision
        }) {
            return;
        }
        let Some(provider) = self.workspace.try_lock().ok().and_then(|workspace| {
            crate::fim::FimProvider::from_editor(&workspace.customization().config().editor)
        }) else {
            return;
        };
        let prefix = self.focused_editor_content[..offset].to_string();
        let suffix = self.focused_editor_content[offset..].to_string();
        match provider.backend {
            crate::fim::FimBackend::Stub => {
                let (tx, rx) = std::sync::mpsc::channel();
                let _ = std::thread::Builder::new()
                    .name("glass-fim".into())
                    .spawn(move || {
                        let _ = tx.send(provider.complete(&prefix, &suffix));
                    });
                self.pending_fim = Some(PendingFim::Thread {
                    path: path.to_string(),
                    offset,
                    revision,
                    rx,
                });
            }
            crate::fim::FimBackend::Resident => {
                let selected = self.selected_agent.clone();
                let queued = self
                    .locked(|workspace| {
                        let since = workspace
                            .agents()
                            .history(0)
                            .ok()?
                            .last()
                            .map(|event| event.sequence)
                            .unwrap_or(0);
                        let snapshots = workspace.agents().list().ok()?;
                        let id = selected
                            .as_ref()
                            .filter(|id| {
                                snapshots.iter().any(|item| {
                                    &item.id == *id && item.status == crate::AgentStatus::Idle
                                })
                            })
                            .cloned()
                            .or_else(|| {
                                snapshots.into_iter().find_map(|item| {
                                    (item.status == crate::AgentStatus::Idle).then_some(item.id)
                                })
                            })?;
                        let request_id = workspace
                            .agents()
                            .request_with_id(
                                &id,
                                crate::resident_harness::ResidentSessionRequest::Complete {
                                    prefix,
                                    suffix,
                                },
                            )
                            .ok()?;
                        Some((id, since, request_id))
                    })
                    .flatten();
                if let Some((agent_id, since, request_id)) = queued {
                    self.pending_fim = Some(PendingFim::Resident {
                        path: path.to_string(),
                        offset,
                        revision,
                        agent_id,
                        since,
                        request_id,
                    });
                }
            }
        }
    }

    fn lsp_ghost_insert(&mut self, path: &str, line: u32, character: u32) -> Option<String> {
        let response = self.locked(|workspace| {
            let server = workspace
                .language()
                .names()
                .next()
                .ok_or_else(|| {
                    crate::development::DevelopmentError::NotFound("language server".into())
                })?
                .to_string();
            workspace
                .language()
                .completion(&server, "local", path, line, character)
        })?;
        parse_completion_insert(&response.ok()?.result)
    }

    fn move_editor_cursor(
        &mut self,
        path: &str,
        line_delta: i32,
        column_delta: i32,
        extend_selection: bool,
    ) -> crate::development::DevelopmentResult<()> {
        let buffer = self
            .workspace
            .try_lock()?
            .project()
            .buffer(path)
            .cloned()
            .ok_or_else(|| {
                crate::development::DevelopmentError::NotFound(format!("buffer {path}"))
            })?;
        let lines = buffer.content.split('\n').collect::<Vec<_>>();
        let line =
            (buffer.cursor_line as i32 + line_delta).clamp(1, lines.len().max(1) as i32) as u32;
        let max_column = lines
            .get(line.saturating_sub(1) as usize)
            .map(|line| line.chars().count() + 1)
            .unwrap_or(1) as u32;
        let column = if line_delta != 0 {
            buffer.cursor_column.min(max_column)
        } else {
            (buffer.cursor_column as i32 + column_delta).clamp(1, max_column as i32) as u32
        };
        let anchor = if extend_selection {
            buffer
                .selection
                .map(|selection| selection.anchor)
                .unwrap_or(crate::development::TextPosition {
                    line: buffer.cursor_line,
                    column: buffer.cursor_column,
                })
        } else {
            crate::development::TextPosition { line, column }
        };
        let mut workspace = self.workspace.try_lock()?;
        workspace
            .project_mut()
            .set_buffer_cursor(path, line, column)?;
        workspace.project_mut().set_buffer_selection(
            path,
            extend_selection.then_some(crate::development::TextSelection {
                anchor,
                active: crate::development::TextPosition { line, column },
            }),
            crate::development::Actor::local(),
        )?;
        Ok(())
    }

    fn insert_editor_text(
        &mut self,
        path: &str,
        text: &str,
    ) -> crate::development::DevelopmentResult<()> {
        let extras = self.editor_engine.extra_selections.clone();
        let mut workspace = self.workspace.try_lock()?;
        let project = workspace.project_mut();
        let buffer = project.buffer(path).cloned().ok_or_else(|| {
            crate::development::DevelopmentError::NotFound(format!("buffer {path}"))
        })?;
        if buffer
            .selection
            .as_ref()
            .is_some_and(|selection| !selection.is_empty())
            && extras.is_empty()
        {
            project.replace_buffer_selection(
                path,
                text.to_string(),
                crate::development::Actor::local(),
            )?;
            return Ok(());
        }
        let primary = editor_offset(&buffer.content, buffer.cursor_line, buffer.cursor_column);
        if extras.is_empty() {
            let mut content = buffer.content;
            content.insert_str(primary, text);
            let cursor =
                crate::development::editor::text_position_at_offset(&content, primary + text.len())
                    .ok_or_else(|| {
                        crate::development::DevelopmentError::InvalidInput(
                            "inserted text ended at an invalid UTF-8 boundary".into(),
                        )
                    })?;
            let actor = crate::development::Actor::local();
            project.edit_buffer(path, content, actor.clone())?;
            project.set_buffer_cursor(path, cursor.line, cursor.column)?;
            project.set_buffer_selection(path, None, actor)?;
            return Ok(());
        }
        let mut sites = vec![primary];
        for extra in &extras {
            if let Some(offset) =
                crate::development::editor::text_position_offset(&buffer.content, extra.active)
            {
                sites.push(offset);
            }
        }
        let (content, carets) = multi_insert(&buffer.content, &sites, text);
        let mut next_extras = Vec::new();
        let mut primary_pos = crate::development::TextPosition { line: 1, column: 1 };
        for (index, offset) in carets.into_iter().enumerate() {
            let position = crate::development::editor::text_position_at_offset(&content, offset)
                .ok_or_else(|| {
                    crate::development::DevelopmentError::InvalidInput(
                        "inserted text ended at an invalid UTF-8 boundary".into(),
                    )
                })?;
            if index == 0 {
                primary_pos = position;
            } else {
                next_extras.push(TextSelection::collapsed(position));
            }
        }
        let actor = crate::development::Actor::local();
        project.edit_buffer(path, content, actor.clone())?;
        project.set_buffer_cursor(path, primary_pos.line, primary_pos.column)?;
        project.set_buffer_selection(path, None, actor)?;
        drop(workspace);
        self.editor_engine.extra_selections = next_extras;
        Ok(())
    }

    fn backspace_editor(&mut self, path: &str) -> crate::development::DevelopmentResult<()> {
        let extras = self.editor_engine.extra_selections.clone();
        let mut workspace = self.workspace.try_lock()?;
        let project = workspace.project_mut();
        let buffer = project.buffer(path).cloned().ok_or_else(|| {
            crate::development::DevelopmentError::NotFound(format!("buffer {path}"))
        })?;
        if buffer
            .selection
            .as_ref()
            .is_some_and(|selection| !selection.is_empty())
            && extras.is_empty()
        {
            project.replace_buffer_selection(
                path,
                String::new(),
                crate::development::Actor::local(),
            )?;
            return Ok(());
        }
        let mut sites = vec![editor_offset(
            &buffer.content,
            buffer.cursor_line,
            buffer.cursor_column,
        )];
        for extra in &extras {
            if let Some(offset) =
                crate::development::editor::text_position_offset(&buffer.content, extra.active)
            {
                sites.push(offset);
            }
        }
        let mut ranges = Vec::new();
        for offset in sites {
            if offset == 0 {
                continue;
            }
            let previous = buffer.content[..offset]
                .char_indices()
                .next_back()
                .map(|(index, _)| index)
                .unwrap_or(0);
            ranges.push((previous, offset));
        }
        if ranges.is_empty() {
            return Ok(());
        }
        if extras.is_empty() && ranges.len() == 1 {
            let (previous, offset) = ranges[0];
            let mut content = buffer.content;
            content.drain(previous..offset);
            let cursor = crate::development::editor::text_position_at_offset(&content, previous)
                .ok_or_else(|| {
                    crate::development::DevelopmentError::InvalidInput(
                        "backspace ended at an invalid UTF-8 boundary".into(),
                    )
                })?;
            let actor = crate::development::Actor::local();
            project.edit_buffer(path, content, actor.clone())?;
            project.set_buffer_cursor(path, cursor.line, cursor.column)?;
            project.set_buffer_selection(path, None, actor)?;
            return Ok(());
        }
        let (content, carets) = multi_delete(&buffer.content, &ranges);
        let mut next_extras = Vec::new();
        let mut primary_pos = crate::development::TextPosition { line: 1, column: 1 };
        for (index, offset) in carets.into_iter().enumerate() {
            let position = crate::development::editor::text_position_at_offset(&content, offset)
                .ok_or_else(|| {
                    crate::development::DevelopmentError::InvalidInput(
                        "backspace ended at an invalid UTF-8 boundary".into(),
                    )
                })?;
            if index == 0 {
                primary_pos = position;
            } else {
                next_extras.push(TextSelection::collapsed(position));
            }
        }
        let actor = crate::development::Actor::local();
        project.edit_buffer(path, content, actor.clone())?;
        project.set_buffer_cursor(path, primary_pos.line, primary_pos.column)?;
        project.set_buffer_selection(path, None, actor)?;
        drop(workspace);
        self.editor_engine.extra_selections = next_extras;
        Ok(())
    }

    pub fn previous_surface(&mut self) {
        let surfaces: &[DevSurface] = if self
            .responsive_class(self.terminal_width, self.terminal_height)
            == ResponsiveClass::Phone
        {
            &DevSurface::PHONE
        } else {
            &DevSurface::PRIMARY
        };
        let index = surfaces
            .iter()
            .position(|surface| *surface == self.surface)
            .unwrap_or(0);
        self.show_surface(surfaces[(index + surfaces.len() - 1) % surfaces.len()]);
    }

    /// Apply one background snapshot without touching UI-only fields.
    fn update_gc_command_catalogue(&mut self, snapshot: &super::snapshot::DisplaySnapshot) {
        let Some(catalogue) = self
            .selected_agent
            .as_ref()
            .and_then(|agent_id| snapshot.core_command_catalogues.get(agent_id))
        else {
            return;
        };
        if let Some(error) = catalogue.error.as_deref() {
            self.gc_command_discovery_pending = false;
            if self.gc_command_menu_open {
                self.gc_command_loading = false;
                self.gc_command_discovery_requested = false;
                self.status = format!("Glass Core command discovery failed: {error}");
            }
            return;
        }
        self.gc_command_items.clone_from(&catalogue.commands);
        self.gc_command_discovery_pending = false;
        if self.gc_command_menu_open {
            self.gc_command_loading = false;
            self.gc_command_discovery_requested = false;
        }
    }

    fn update_gc_model_catalogue(&mut self, snapshot: &super::snapshot::DisplaySnapshot) {
        let Some(catalogue) = self
            .selected_agent
            .as_ref()
            .and_then(|agent_id| snapshot.core_model_catalogues.get(agent_id))
        else {
            return;
        };
        self.gc_model_error.clone_from(&catalogue.error);
        if catalogue.error.is_none() {
            self.gc_model_provider.clone_from(&catalogue.provider);
            self.gc_model_current.clone_from(&catalogue.current_model);
            self.gc_model_items.clone_from(&catalogue.models);
        }
        if self.gc_model_menu_open {
            self.gc_command_loading = false;
            self.gc_command_selected = self
                .gc_command_selected
                .min(self.gc_model_items.len().saturating_sub(1));
            self.status = if let Some(error) = catalogue.error.as_deref() {
                format!("Could not load provider models: {error}")
            } else {
                format!(
                    "Choose a provider model · current: {} · Enter applies to this Core session",
                    self.gc_model_current.as_deref().unwrap_or("unknown")
                )
            };
        }
    }

    fn update_gc_session_settings(&mut self, snapshot: &super::snapshot::DisplaySnapshot) {
        self.gc_session_settings = self
            .selected_agent
            .as_ref()
            .and_then(|agent_id| snapshot.core_session_settings.get(agent_id))
            .cloned();
    }

    pub fn apply_snapshot(&mut self, snapshot: &super::snapshot::DisplaySnapshot) {
        self.remember_row_selection_identity();
        let was_waiting_for_snapshot = !self.snapshot_ready;
        self.project_native_gc_ui_outputs(snapshot, was_waiting_for_snapshot);
        let had_agent_lifecycle_notice = is_agent_lifecycle_notice(&self.status);
        let mut browser_recovery_status = None;
        self.snapshot_ready = true;
        self.conversation_cursor = snapshot.conversation_cursor;
        self.refresh_latency_ms = snapshot.duration.as_millis() as u64;
        if !snapshot.agents.starts_with("Agent registry failed:") {
            self.agent_status_by_id = snapshot.agent_states.iter().cloned().collect();
        }
        if let super::snapshot::BrowserHealth::Crashed {
            last_process_id,
            last_revision,
            last_backend,
        } = &snapshot.browser_health
            && self.browser_recovery.is_none()
        {
            let pid_label = last_process_id
                .map(|pid| pid.to_string())
                .unwrap_or_else(|| "unknown".into());
            let revision_label = last_revision
                .map(|revision| revision.to_string())
                .unwrap_or_else(|| "none".into());
            let reason = format!(
                "browser endpoint crashed unexpectedly (pid {pid_label}, last revision {revision_label})"
            );
            self.offer_browser_recovery_for_backend(
                &reason,
                9222,
                last_backend.as_deref() == Some("native"),
            );
            browser_recovery_status = Some(self.status.clone());
        }
        self.harnesses = snapshot.harnesses.clone();
        if let Ok(list) = self.ws().map(|workspace| workspace.todos()) {
            self.session_todos = list;
            self.selected_todo = self
                .selected_todo
                .min(self.session_todos.items.len().saturating_sub(1));
        }
        let highlighted_thread = self
            .thread_picker_items
            .get(self.thread_picker_selection)
            .map(|item| item.agent_id.clone());
        self.thread_picker_items = snapshot.thread_picker_items.clone();
        self.thread_picker_selection = highlighted_thread
            .as_ref()
            .and_then(|id| {
                self.thread_picker_items
                    .iter()
                    .position(|item| &item.agent_id == id)
            })
            .or_else(|| {
                self.thread_picker_items
                    .iter()
                    .position(|item| Some(&item.agent_id) == self.selected_agent.as_ref())
            })
            .unwrap_or_else(|| {
                self.thread_picker_selection
                    .min(self.thread_picker_items.len().saturating_sub(1))
            });
        self.agents = snapshot.agents.clone();
        self.editor_proposals = snapshot.editor_proposals.clone();
        self.reconcile_selected_editor_proposal();
        self.agent_conversations = snapshot.agent_conversations.clone();
        for conversation in self.agent_conversations.values_mut() {
            let has_core_ui_output = conversation
                .items
                .iter()
                .any(|entry| super::projection::native_gc_ui_entry(entry).is_some());
            conversation
                .items
                .retain(|entry| super::projection::native_gc_ui_entry(entry).is_none());
            if has_core_ui_output {
                conversation.text = conversation
                    .items
                    .iter()
                    .map(super::projection::ConversationEntry::render)
                    .collect::<Vec<_>>()
                    .join("\n\n");
            }
        }
        if self.selected_agent.is_none()
            && let Some(chrome) = snapshot
                .agent_chrome
                .iter()
                .find(|chrome| is_default_composer_agent(chrome.status))
        {
            self.select_agent(Some(chrome.id.clone()));
        }
        if let Some(conversation) = self
            .selected_agent
            .as_ref()
            .and_then(|id| self.agent_conversations.get(id))
        {
            self.agent_conversation = conversation.text.clone();
            self.conversation_items = conversation.items.clone();
        } else {
            self.agent_conversation =
                "No conversation yet. Press Enter or start typing to compose a message.".into();
            self.conversation_items.clear();
        }
        if let Some(chrome) = self
            .selected_agent
            .as_ref()
            .and_then(|selected| {
                snapshot
                    .agent_chrome
                    .iter()
                    .find(|item| item.id == *selected)
            })
            .or_else(|| snapshot.agent_chrome.first())
        {
            self.agent_model = chrome.model.clone().unwrap_or_default();
            self.agent_thinking = chrome.thinking.clone().unwrap_or_default();
            if let Some(path) = chrome.session_file.as_deref() {
                self.agent_session_name = std::path::Path::new(path)
                    .file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or(path)
                    .to_string();
            }
        }
        self.update_gc_command_catalogue(snapshot);
        self.update_gc_model_catalogue(snapshot);
        self.update_gc_session_settings(snapshot);
        let entry_count = self.conversation_entries_view().len();
        if entry_count == 0 {
            self.transcript_selection = 0;
        } else {
            self.transcript_selection = self.transcript_selection.min(entry_count - 1);
        }
        self.reconcile_pending_chat();
        let selected_agent_status = self.selected_agent_status();
        if self.agent_send_job.is_none()
            && !self
                .pending_chat_messages
                .iter()
                .any(|message| message.state == ChatMessageState::Sending)
            && is_agent_lifecycle_notice(&self.status)
        {
            match selected_agent_status {
                Some(crate::AgentStatus::Idle) => {
                    // Core's idle state means no turn is currently running;
                    // it does not prove that the previous prompt succeeded.
                    self.status = "Glass Agent idle".into();
                }
                Some(crate::AgentStatus::Failed) => {
                    self.status = "Glass Agent failed · press Enter to retry".into();
                }
                Some(crate::AgentStatus::Cancelled) => {
                    self.status = "Glass Agent stopped · press Enter to retry".into();
                }
                Some(crate::AgentStatus::Waiting) => {
                    self.status = "Glass Agent waiting".into();
                }
                _ => {}
            }
        }
        self.tasks = snapshot.tasks.clone();
        if !snapshot.editor.is_empty() {
            self.editor = snapshot.editor.clone();
        }
        self.lsp = snapshot.lsp.clone();
        self.processes = snapshot.processes.clone();
        self.process_entries = snapshot.process_entries.clone();
        if self.process_entries.is_empty() {
            self.process_urls.clear();
        } else {
            self.selected_process = self
                .selected_process_identity
                .as_deref()
                .and_then(|name| {
                    self.process_entries
                        .iter()
                        .position(|entry| entry.name == name)
                })
                .unwrap_or_else(|| self.selected_process.min(self.process_entries.len() - 1));
            self.selected_process_identity = self
                .process_entries
                .get(self.selected_process)
                .map(|entry| entry.name.clone());
            self.process_urls = self
                .process_entries
                .iter()
                .filter_map(|entry| entry.url.clone())
                .collect();
            self.process_urls.sort();
            self.process_urls.dedup();
        }
        self.git = snapshot.git.clone();
        self.git_entries = snapshot.git_entries.clone();
        if !self.git_entries.is_empty() {
            self.selected_git_file = self
                .selected_git_identity
                .as_deref()
                .and_then(|path| self.git_entries.iter().position(|entry| entry.path == path))
                .unwrap_or_else(|| self.selected_git_file.min(self.git_entries.len() - 1));
            self.selected_git_identity = self
                .git_entries
                .get(self.selected_git_file)
                .map(|entry| entry.path.clone());
        }
        self.git_branch = snapshot.git_branch.clone();
        self.git_dirty = snapshot.git_dirty;
        self.git_ahead = snapshot.git_ahead;
        self.git_behind = snapshot.git_behind;
        self.git_conflicts = snapshot.git_conflicts.clone();
        self.github = snapshot.github.clone();
        self.github_review = snapshot.github_review.clone();
        if !self.git_entries.is_empty() {
            self.selected_git_file = self
                .selected_git_file
                .min(self.git_entries.len().saturating_sub(1));
            if self.surface == DevSurface::Git && !self.git_diff_requested {
                let selected_path = self.selected_git_entry().map(|entry| entry.path.as_str());
                if !self.git_diff_open || selected_path != self.git_diff_path.as_deref() {
                    self.request_selected_git_diff();
                }
            }
        }
        self.tests = snapshot.tests.clone();
        self.kernels = snapshot.kernels.clone();
        self.debugger = snapshot.debugger.clone();
        let previous_debug = self
            .selected_debug_session()
            .map(|session| session.name.clone());
        self.debug_sessions = snapshot.debug_sessions.clone();
        if self.debug_sessions.is_empty() {
            self.debug_threads.clear();
            self.debug_frames.clear();
            self.debug_scopes.clear();
            self.debug_variables.clear();
            self.selected_debug_thread = 0;
            self.selected_debug_frame = 0;
            self.selected_debug_scope = 0;
            self.debug_scopes_requested = false;
            self.debug_variables_requested = false;
            self.debug_variables_notice = None;
        } else {
            self.selected_debug_session = self
                .selected_debug_identity
                .as_deref()
                .and_then(|name| {
                    self.debug_sessions
                        .iter()
                        .position(|entry| entry.name == name)
                })
                .unwrap_or_else(|| {
                    self.selected_debug_session
                        .min(self.debug_sessions.len() - 1)
                });
            self.selected_debug_identity = self
                .debug_sessions
                .get(self.selected_debug_session)
                .map(|entry| entry.name.clone());
            let current = self
                .selected_debug_session()
                .map(|session| session.name.clone());
            if previous_debug != current {
                self.debug_threads.clear();
                self.debug_frames.clear();
                self.debug_scopes.clear();
                self.debug_variables.clear();
                self.selected_debug_thread = 0;
                self.selected_debug_frame = 0;
                self.selected_debug_scope = 0;
                self.debug_scopes_requested = false;
                self.debug_variables_requested = false;
                self.debug_variables_notice = None;
            }
        }
        self.reconcile_stale_debug_previews();
        self.replay = snapshot.replay.clone();
        self.workflow = snapshot.workflow.clone();
        self.refresh_workflow_recording_status();
        self.workspace_status = snapshot.workspace_status.clone();
        self.browser = snapshot.browser.clone();
        self.snapshot_root = snapshot.root.clone();
        self.snapshot_project_revision = snapshot.project_revision;
        self.snapshot_generation = snapshot.generation;
        self.snapshot_skills_count = snapshot.skills_count;
        self.snapshot_tools_count = snapshot.tools_count;
        self.experiments = snapshot.experiments.clone();
        if !snapshot.files.is_empty() {
            self.files = snapshot.files.clone();
            if !self.files.is_empty() {
                self.selected_file = self.selected_file.min(self.files.len() - 1);
            }
        }
        if !self.code_edit_mode && self.pending_open_file.is_some() && !self.files.is_empty() {
            self.flush_pending_open_file();
        }
        if was_waiting_for_snapshot
            && self.browser_recovery.is_none()
            && !had_agent_lifecycle_notice
        {
            self.status = "Workspace ready · type to chat · / commands · Ctrl-P files".into();
        }
        if had_agent_lifecycle_notice
            && self.agent_send_job.is_none()
            && !self
                .pending_chat_messages
                .iter()
                .any(|message| message.state == ChatMessageState::Sending)
        {
            match self.selected_agent_status() {
                Some(crate::AgentStatus::Idle) => {
                    // Idle is a lifecycle observation, not a successful
                    // response event. Keep failures visible in the transcript.
                    self.status = "Glass Agent idle".into();
                }
                Some(crate::AgentStatus::Failed) => {
                    self.status = "Glass Agent failed · press Enter to retry".into();
                }
                Some(crate::AgentStatus::Cancelled) => {
                    self.status = "Glass Agent stopped · press Enter to retry".into();
                }
                Some(crate::AgentStatus::Waiting) => {
                    self.status = "Glass Agent waiting".into();
                }
                _ => {}
            }
        }
        if let Some(status) = browser_recovery_status {
            self.status = status;
        }
        self.refresh_attention_items();
    }

    fn remember_row_selection_identity(&mut self) {
        if let Some(entry) = self.process_entries.get(self.selected_process) {
            self.selected_process_identity = Some(entry.name.clone());
        }
        if let Some(entry) = self.git_entries.get(self.selected_git_file) {
            self.selected_git_identity = Some(entry.path.clone());
        }
        if let Some(entry) = self.debug_sessions.get(self.selected_debug_session) {
            self.selected_debug_identity = Some(entry.name.clone());
        }
    }

    pub fn set_terminal_size(&mut self, width: u16, height: u16) {
        self.terminal_width = width;
        self.terminal_height = height;
        self.ensure_editor_cursor_visible();
    }

    pub fn refresh(&mut self) {
        self.remember_row_selection_identity();
        self.harnesses = crate::harness::summary();
        let Ok(mut workspace) = self.workspace.try_lock() else {
            self.status = "Workspace lock failed".into();
            return;
        };
        self.files = workspace
            .project()
            .list_files()
            .map(|entries| {
                entries
                    .into_iter()
                    .filter(|entry| matches!(entry.kind, crate::development::FileKind::File))
                    .map(|entry| entry.path)
                    .take(512)
                    .collect()
            })
            .unwrap_or_default();
        if let Some(wake) = workspace.latest_crew_wake() {
            self.last_crew_wake = Some(wake.render());
        }
        if !self.files.is_empty() {
            self.selected_file = self.selected_file.min(self.files.len() - 1);
        }
        let task_snapshots = workspace.tasks().unwrap_or_default();
        let agent_snapshots = workspace.agents().list();
        if let Ok(agents) = &agent_snapshots {
            self.agent_status_by_id = agents
                .iter()
                .map(|agent| (agent.id.clone(), agent.status))
                .collect();
        }
        if let Ok(agents) = &agent_snapshots {
            self.thread_picker_items = project_thread_picker_items(agents, &task_snapshots);
        }
        self.agents = match agent_snapshots {
            Ok(agents) if agents.is_empty() => "No Glass workspace agents. Core conversations appear in the Agent workspace.".into(),
            Ok(agents) => agents
                .iter()
                .map(|agent| {
                    format!(
                        "{}  {}  {} · {}\n  target {} · model {} · thinking {} · events {} · dropped {}{}\n  evidence {}",
                        agent.id.as_str(),
                        agent.status.label(),
                        agent.role,
                        agent.task,
                        agent.worktree.display(),
                        agent.model.as_deref().unwrap_or("default"),
                        agent.thinking.as_deref().unwrap_or("default"),
                        agent.event_count,
                        agent.dropped_event_count,
                        agent
                            .last_error
                            .as_deref()
                            .map(|error| format!(" · {error}"))
                            .unwrap_or_default(),
                        agent
                            .evidence
                            .iter()
                            .rev()
                            .take(3)
                            .map(|evidence| evidence.to_string())
                            .collect::<Vec<_>>()
                            .join(" · ")
                    )
                })
                .collect::<Vec<_>>()
                .join("\n\n"),
            Err(error) => format!("Agent registry failed: {error}"),
        };
        self.agent_conversation = match workspace.agents().history(0) {
            Ok(events) if events.is_empty() => {
                "No conversation yet. Type a message to start one.".into()
            }
            Ok(events) => super::projection::conversation(
                &events
                    .iter()
                    .rev()
                    .take(40)
                    .rev()
                    .cloned()
                    .collect::<Vec<_>>(),
            ),
            Err(error) => format!("Conversation unavailable · {error}"),
        };
        let core_threads = self
            .thread_picker_items
            .iter()
            .map(|thread| {
                let title = thread
                    .title
                    .chars()
                    .filter(|character| !character.is_control())
                    .take(120)
                    .collect::<String>();
                format!(
                    "CORE · {} · {} · {}",
                    thread.status,
                    title,
                    thread.agent_id.as_str()
                )
            })
            .collect::<Vec<_>>();
        let workspace_tasks = match workspace.tasks() {
            Ok(tasks) => tasks
                .iter()
                .map(|task| {
                    let glyph = match task.state {
                        crate::TaskState::Succeeded
                            if task.verification == crate::VerificationRequirement::Settled => "◇",
                        crate::TaskState::Succeeded => "✓",
                        crate::TaskState::Failed | crate::TaskState::Cancelled => "×",
                        crate::TaskState::Blocked => "!",
                        crate::TaskState::Running | crate::TaskState::Verifying => "●",
                        _ => "○",
                    };
                    format!(
                        "{} {}  {}  {}\n  goal {}\n  agent {} · attempt {} · model {} · thinking {}\n  depends {}\n  verification {}{}\n  evidence {}",
                        glyph,
                        task.id.as_str(),
                        task.state.label(),
                        task.title,
                        task.goal,
                        task.assigned_agent
                            .as_ref()
                            .map(|agent| agent.as_str())
                            .unwrap_or("unassigned"),
                        task.attempt,
                        task.model.as_deref().unwrap_or("default"),
                        task.thinking.as_deref().unwrap_or("default"),
                        task.dependencies
                            .iter()
                            .map(|dependency| dependency.as_str())
                            .collect::<Vec<_>>()
                            .join(", "),
                        serde_json::to_string(&task.verification).unwrap_or_default(),
                        task.last_error
                            .as_deref()
                            .map(|error| format!(" · {error}"))
                            .unwrap_or_default(),
                        task.evidence
                            .iter()
                            .rev()
                            .take(3)
                            .map(|evidence| {
                                format!(
                                    "{}={}",
                                    evidence.kind,
                                    evidence.passed.map_or("?".into(), |passed| passed.to_string())
                                )
                            })
                            .collect::<Vec<_>>()
                            .join(" · ")
                    )
                })
                .collect::<Vec<_>>()
                .join("\n\n"),
            Err(error) => format!("Workspace task flows unavailable · {error}"),
        };
        let mut task_sections = Vec::new();
        if !core_threads.is_empty() {
            task_sections.push(format!("CORE CONVERSATIONS\n{}", core_threads.join("\n")));
        }
        if !workspace_tasks.is_empty() {
            task_sections.push(format!("GLASS WORKSPACE FLOWS\n{workspace_tasks}"));
        }
        self.tasks = if task_sections.is_empty() {
            "No Core conversations yet. Type a message in Agent to start one.".into()
        } else {
            task_sections.join("\n\n")
        };
        match workspace.project_mut().processes().list_checked() {
            Ok(items) => {
                self.process_urls = items
                    .iter()
                    .flat_map(|item| item.detected_urls.iter().cloned())
                    .collect();
                self.process_urls.sort();
                self.process_urls.dedup();
                self.process_entries = items.iter().map(ProcessRow::from_snapshot).collect();
                self.selected_process = if self.process_entries.is_empty() {
                    0
                } else {
                    self.selected_process_identity
                        .as_deref()
                        .and_then(|name| {
                            self.process_entries
                                .iter()
                                .position(|entry| entry.name == name)
                        })
                        .unwrap_or_else(|| {
                            self.selected_process.min(self.process_entries.len() - 1)
                        })
                };
                if let Some(entry) = self.process_entries.get(self.selected_process) {
                    self.selected_process_identity = Some(entry.name.clone());
                }
                self.processes = if items.is_empty() {
                    "No managed terminals. Start the detected development command from More.".into()
                } else {
                    items
                        .into_iter()
                        .map(|item| {
                            format!(
                                "{} {} · health {} · pid {} · {}\n  {}",
                                if matches!(item.health, crate::development::ProcessHealth::Healthy)
                                {
                                    "●"
                                } else {
                                    "○"
                                },
                                item.name,
                                item.health.label(),
                                item.pid.map_or_else(|| "—".into(), |pid| pid.to_string()),
                                if item.pty { "PTY" } else { "pipes" },
                                item.detected_urls
                                    .first()
                                    .map_or(item.command.as_str(), String::as_str),
                            )
                        })
                        .collect::<Vec<_>>()
                        .join("\n\n")
                };
            }
            Err(error) => {
                self.process_urls.clear();
                self.process_entries.clear();
                self.selected_process = 0;
                self.processes = format!("Process state failed: {error}");
            }
        }
        let buffers = workspace.project().buffers().cloned().collect::<Vec<_>>();
        self.editor = if buffers.is_empty() {
            "No file open. Select a file below and press Enter to open the full-screen editor."
                .into()
        } else {
            buffers
                .iter()
                .map(|buffer| {
                    // Viewport follows the cursor; long files stay editable.
                    let lines: Vec<&str> = buffer.content.lines().collect();
                    let cursor = buffer.cursor_line as usize;
                    let viewport_rows = 16;
                    let start = cursor
                        .saturating_sub(viewport_rows / 2)
                        .min(lines.len().saturating_sub(viewport_rows.min(lines.len())));
                    let end = (start + viewport_rows).min(lines.len());
                    let gutter_width = lines.len().to_string().len().max(3);
                    let viewport = lines[start..end]
                        .iter()
                        .enumerate()
                        .map(|(index, line)| {
                            let number = start + index + 1;
                            let marker = if number == cursor { "▶" } else { " " };
                            format!(
                                "{marker}{:>gutter_width$} │ {line}",
                                number,
                                gutter_width = gutter_width
                            )
                        })
                        .collect::<Vec<_>>()
                        .join("\n");
                    format!(
                        "{}{} · cursor {}:{} · actor {} · {} lines\n{}",
                        if buffer.dirty { "● " } else { "○ " },
                        buffer.path,
                        buffer.cursor_line,
                        buffer.cursor_column,
                        buffer.actor.id,
                        lines.len(),
                        viewport
                    )
                })
                .collect::<Vec<_>>()
                .join("\n\n")
        };
        self.editor_comments = workspace.project().editor_comments(None);
        self.editor_proposals = workspace.project().editor_proposals();
        self.editor_checkpoints = workspace.project().editor_checkpoints();
        if let Some(buffer) = buffers.get(self.editor_buffer_index) {
            self.focused_editor_path = buffer.path.clone();
            self.focused_editor_dirty = buffer.dirty;
            self.focused_editor_line = buffer.cursor_line;
            self.focused_editor_column = buffer.cursor_column;
            self.focused_editor_selection = buffer.selection.clone();
        }
        self.lsp = {
            let language = workspace.language();
            let servers = language.names().collect::<Vec<_>>();
            let event_count = language.events(0).len();
            if servers.is_empty() {
                "No language server active · diagnostics unavailable".into()
            } else {
                format!("● {} · {} recent events", servers.join(" · "), event_count)
            }
        };
        let git_status = workspace.git().map(|git| git.status());
        self.git = match git_status {
            Some(Ok(status)) => {
                self.git_entries = status.sorted_entries();
                self.git_branch = status.branch.clone().unwrap_or_else(|| "detached".into());
                self.git_ahead = status.ahead;
                self.git_behind = status.behind;
                self.git_conflicts = status.conflicts.clone();
                self.git_dirty = !status.conflicts.is_empty()
                    || status.ahead > 0
                    || status.behind > 0
                    || status.entries.iter().any(|entry| {
                        entry.untracked || entry.index_status != ' ' || entry.worktree_status != ' '
                    });
                let header = format!(
                    "branch {} · ↑{} ↓{} · upstream {}",
                    status.branch.as_deref().unwrap_or("detached"),
                    status.ahead,
                    status.behind,
                    status.upstream.as_deref().unwrap_or("none")
                );
                let entries = status
                    .entries
                    .iter()
                    .map(|entry| {
                        format!(
                            "{}{} {}{}",
                            if entry.untracked { "?" } else { "●" },
                            if status.conflicts.contains(&entry.path) {
                                "!"
                            } else {
                                " "
                            },
                            entry.index_status,
                            entry.path
                        )
                    })
                    .collect::<Vec<_>>();
                if entries.is_empty() {
                    format!("{header}\n✓ working tree clean")
                } else {
                    format!("{header}\n{}", entries.join("\n"))
                }
            }
            Some(Err(error)) => {
                self.git_entries.clear();
                self.git_branch.clear();
                self.git_ahead = 0;
                self.git_behind = 0;
                self.git_conflicts.clear();
                self.git_dirty = false;
                format!("Git state failed: {error}")
            }
            None => {
                self.git_entries.clear();
                self.git_branch.clear();
                self.git_ahead = 0;
                self.git_behind = 0;
                self.git_conflicts.clear();
                self.git_dirty = false;
                "Not a Git repository".into()
            }
        };
        if self.git_entries.is_empty() {
            self.selected_git_file = 0;
        } else {
            self.selected_git_file = self
                .selected_git_identity
                .as_deref()
                .and_then(|path| self.git_entries.iter().position(|entry| entry.path == path))
                .unwrap_or_else(|| self.selected_git_file.min(self.git_entries.len() - 1));
            self.selected_git_identity = self
                .git_entries
                .get(self.selected_git_file)
                .map(|entry| entry.path.clone());
        }
        let _ = workspace.tests_mut().poll();
        let test_runs = workspace
            .tests()
            .results()
            .rev()
            .take(32)
            .collect::<Vec<_>>();
        self.tests = if test_runs.is_empty() {
            "No test runs".into()
        } else {
            test_runs
                .iter()
                .map(|run| {
                    format!(
                        "{} {} · {} · {} ms · {} cases",
                        if run.exit_code == Some(0) {
                            "✓"
                        } else {
                            "×"
                        },
                        run.suite_id,
                        run.state.label(),
                        run.duration_ms.unwrap_or_default(),
                        run.cases.len()
                    )
                })
                .collect::<Vec<_>>()
                .join("\n")
        };
        let kernels = workspace.kernels().snapshots().collect::<Vec<_>>();
        self.kernels = if kernels.is_empty() {
            "No persistent kernels".into()
        } else {
            kernels
                .iter()
                .map(|kernel| {
                    format!(
                        "{} {} · {} · {} executions · rev {}",
                        if matches!(kernel.state, crate::kernels::KernelState::Ready) {
                            "●"
                        } else {
                            "○"
                        },
                        kernel.name,
                        kernel.kind.label(),
                        kernel.executions,
                        kernel.workspace_revision
                    )
                })
                .collect::<Vec<_>>()
                .join("\n")
        };
        let debugger_names = workspace
            .debugger_names()
            .map(str::to_string)
            .collect::<Vec<_>>();
        self.debugger = if debugger_names.is_empty() {
            self.debug_sessions.clear();
            self.selected_debug_session = 0;
            "No debugger sessions. Ctrl-K opens Debug actions.".into()
        } else {
            let snapshots: Result<Vec<_>, _> = debugger_names
                .iter()
                .map(|name| {
                    workspace
                        .debugger_mut(name)
                        .and_then(|debugger| debugger.snapshot())
                        .map(|snapshot| (name.to_string(), snapshot))
                })
                .collect();
            match snapshots {
                Ok(snapshots) => {
                    self.debug_sessions = snapshots
                        .iter()
                        .map(|(name, snapshot)| DebugSessionRow {
                            name: name.clone(),
                            state: snapshot.state,
                            pid: snapshot.adapter_process_id,
                            breakpoints: snapshot.breakpoints.values().map(Vec::len).sum::<usize>(),
                            watches: snapshot.watches.len(),
                        })
                        .collect();
                    self.selected_debug_session = self
                        .selected_debug_identity
                        .as_deref()
                        .and_then(|name| {
                            self.debug_sessions
                                .iter()
                                .position(|entry| entry.name == name)
                        })
                        .unwrap_or_else(|| {
                            self.selected_debug_session
                                .min(self.debug_sessions.len() - 1)
                        });
                    self.selected_debug_identity = self
                        .debug_sessions
                        .get(self.selected_debug_session)
                        .map(|entry| entry.name.clone());
                    snapshots
                    .iter()
                    .map(|(name, snapshot)| {
                        format!(
                            "● {} · {} · pid {} · {} breakpoints · {} watches · {} threads/processes",
                            name,
                            snapshot.state.label(),
                            snapshot.adapter_process_id,
                            snapshot.breakpoints.values().map(Vec::len).sum::<usize>(),
                            snapshot.watches.len(),
                            snapshot.debuggee_processes.len()
                        )
                    })
                    .collect::<Vec<_>>()
                    .join("\n")
                }
                Err(error) => format!("Debugger state failed: {error}"),
            }
        };
        self.replay = workspace
            .intelligence()
            .replay(0, 128)
            .map(|events| {
                if events.is_empty() {
                    "No observable replay events".into()
                } else {
                    events
                        .iter()
                        .rev()
                        .take(24)
                        .rev()
                        .map(|event| {
                            format!(
                                "{} {} · {} · {} · rev {}",
                                event.sequence,
                                event.actor,
                                event.subsystem,
                                event.kind,
                                event.workspace_revision
                            )
                        })
                        .collect::<Vec<_>>()
                        .join("\n")
                }
            })
            .unwrap_or_else(|error| format!("Replay failed: {error}"));
        self.browser = match workspace.browser().state() {
            Ok(state) => {
                let connected = state
                    .get("connected")
                    .and_then(serde_json::Value::as_bool)
                    .unwrap_or(false);
                let revision = state
                    .get("browserRevision")
                    .and_then(serde_json::Value::as_u64);
                if connected {
                    if self.browser_workspace.state().connection
                        != BrowserConnectionPhase::Connected
                    {
                        self.browser_workspace.connected(true, None, revision);
                    } else if let Some(revision) = revision {
                        self.browser_workspace.state_mut().browser_revision = Some(revision);
                    }
                } else {
                    self.browser_workspace.state_mut().connection =
                        BrowserConnectionPhase::Detached;
                }
                self.browser_workspace_summary()
            }
            Err(error) => format!("Browser state failed: {error}"),
        };
        self.workflow = workspace
            .browser()
            .list_workflows()
            .map(|state| super::projection::workflow(Some(&state)))
            .unwrap_or_else(|error| format!("Workflow state failed: {error}"));
        let root = workspace.root().display().to_string();
        let generation = workspace.generation();
        let project_revision = workspace.project().revision();
        let agent_count = workspace
            .agents()
            .list()
            .map(|items| items.len())
            .unwrap_or(0);
        let task_count = workspace.tasks().map(|items| items.len()).unwrap_or(0);
        let kernel_count = workspace.kernels().snapshots().count();
        let debugger_count = workspace.debugger_names().count();
        let detection = workspace.project().detection().clone();
        let dev_hint = detection
            .dev_command
            .as_deref()
            .map(|command| format!("● run `{command}`"))
            .unwrap_or_else(|| "○ configure a dev command".into());
        let app_hint = detection
            .browser_url
            .as_deref()
            .map(|url| format!("● open Browser {url}"))
            .unwrap_or_else(|| "○ start a preview server to detect a Browser URL".into());
        let project_line = if detection.languages.is_empty() {
            "○ project type unknown".to_string()
        } else {
            format!("✓ {} project", detection.languages.join("/"))
        };
        let workspace_line = "✓ workspace ready";
        let agent_hint = if self.agent_readiness.starts_with("✓ Ready") {
            "Core ready · type a message to chat"
        } else {
            "○ Core setup · type / to browse Core setup commands"
        };
        self.workspace_status = format!(
            "WELCOME · {}\n{}\n\n{}\n{}\n\nNEXT ACTIONS\n◆ Ctrl-K · guided Glass workspace actions\n◆ / · discover Core commands and Glass workspace actions\n◆ type a message to talk to Glass Core\n◆ start the detected preview server to see its Browser URL\n◆ Enter · open the selected file in Code\n{}\n{}\n\nSTATE\nroot {}\ngeneration {} · project revision {}\nresident: {} workspace agents · {} workspace tasks · {} kernels · {} debuggers",
            detection.git_branch.as_deref().unwrap_or("no branch"),
            project_line,
            agent_hint,
            workspace_line,
            dev_hint,
            app_hint,
            root,
            generation,
            project_revision,
            agent_count,
            task_count,
            kernel_count,
            debugger_count,
        );
        if let Ok(experiments) = workspace.experiments() {
            let snapshots = experiments.snapshots();
            self.experiments = format_experiment_snapshots(&snapshots);
            self.experiment_comparison = Some(experiments.compare());
        }
        drop(workspace);
        self.refresh_attention_items();
        self.snapshot_ready = true;
        self.reconcile_pending_chat();
    }

    pub fn apply_browser_result(&mut self, tool: &str, result: &serde_json::Value) {
        if let Some(backend) = result.get("backend").and_then(serde_json::Value::as_str) {
            self.browser_workspace.state_mut().backend = Some(backend.to_owned());
        }
        match tool {
            "glass.browser.observe" => {
                let revision = result
                    .get("browserRevision")
                    .and_then(serde_json::Value::as_u64)
                    .or_else(|| {
                        result
                            .pointer("/accessibility/revision")
                            .and_then(serde_json::Value::as_u64)
                    });
                let title = result
                    .pointer("/page/title")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or("Untitled");
                let url = result
                    .pointer("/page/url")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or_default();
                let loading = result
                    .pointer("/page/loading")
                    .and_then(serde_json::Value::as_bool)
                    .unwrap_or(false);
                if let Some(revision) = revision {
                    self.browser_workspace
                        .update_page(title, url, loading, Some(revision));
                    let entities = result
                        .pointer("/accessibility/interactive")
                        .and_then(serde_json::Value::as_array)
                        .into_iter()
                        .flatten()
                        .filter_map(|entity| {
                            Some(BrowserWorkspaceEntity {
                                reference: entity.get("reference")?.as_str()?.into(),
                                role: entity
                                    .get("role")
                                    .and_then(serde_json::Value::as_str)
                                    .unwrap_or("unknown")
                                    .into(),
                                name: entity
                                    .get("name")
                                    .and_then(serde_json::Value::as_str)
                                    .unwrap_or("unnamed")
                                    .into(),
                                actionable: entity
                                    .get("actionable")
                                    .and_then(serde_json::Value::as_bool)
                                    .unwrap_or(true),
                                revision,
                            })
                        })
                        .collect();
                    self.browser_workspace.replace_entities(revision, entities);
                    if let Some(entity) = self.pending_page_entity.clone()
                        && self.select_page_entity(&entity)
                    {
                        self.pending_page_entity = None;
                        self.status = format!("Page bound · {entity}");
                    }
                }
            }
            "glass.browser.targets" => {
                let values = result
                    .get("targets")
                    .and_then(serde_json::Value::as_array)
                    .or_else(|| result.as_array());
                let targets = values
                    .into_iter()
                    .flatten()
                    .filter_map(|target| {
                        Some(BrowserWorkspaceTarget {
                            id: target.get("id")?.as_str()?.into(),
                            title: target
                                .get("title")
                                .and_then(serde_json::Value::as_str)
                                .unwrap_or("Untitled")
                                .into(),
                            url: target
                                .get("url")
                                .and_then(serde_json::Value::as_str)
                                .unwrap_or_default()
                                .into(),
                            selected: target
                                .get("active")
                                .and_then(serde_json::Value::as_bool)
                                .unwrap_or(false),
                        })
                    })
                    .collect();
                self.browser_workspace.replace_targets(targets);
                if self.browser_target_picker_requested {
                    self.browser_target_picker_requested = false;
                    self.browser_target_picker = true;
                    self.browser_target_selection = 0;
                    self.status = if self.browser_workspace.state().targets.is_empty() {
                        "No page targets found · Esc closes".into()
                    } else {
                        "Target picker · type to filter · j/k select · Enter choose · Esc close"
                            .into()
                    };
                }
            }
            "glass.browser.target.select" => {
                if let Some(target_id) = result
                    .pointer("/target/id")
                    .and_then(serde_json::Value::as_str)
                {
                    let mut targets = self.browser_workspace.state().targets.clone();
                    for target in &mut targets {
                        target.selected = target.id == target_id;
                    }
                    self.browser_workspace.replace_targets(targets);
                }
                if let Some(observation) = result.get("observation") {
                    self.apply_browser_result("glass.browser.observe", observation);
                }
            }
            "glass.browser.start" => {
                let connected = result
                    .get("connected")
                    .and_then(serde_json::Value::as_bool)
                    .unwrap_or(true);
                let revision = result
                    .get("browserRevision")
                    .and_then(serde_json::Value::as_u64);
                if connected {
                    self.browser_workspace.connected(true, None, revision);
                    self.browser_recovery = None;
                }
            }
            "glass.browser.stop" => {
                self.browser_workspace.state_mut().connection = BrowserConnectionPhase::Detached;
                self.browser_workspace.state_mut().backend = None;
            }
            _ => {}
        }
        self.browser = self.browser_workspace_summary();
    }

    /// Poll the native dialog control plane without taking the shared
    /// workspace lock. Returns true when the visible pending dialog identity
    /// changes and the overlay should be redrawn.
    pub fn poll_native_browser_dialog(&mut self) -> bool {
        let previous_id = self
            .browser_dialog
            .as_ref()
            .map(|dialog| dialog.pending.id.clone());
        let enabled = match self.browser_service.native_dialog_control_enabled() {
            Ok(enabled) => enabled,
            Err(error) => {
                if let Some(dialog) = self.browser_dialog.as_mut() {
                    dialog.error = Some(error.to_string());
                }
                return false;
            }
        };
        if !enabled {
            self.browser_dialog = None;
            return previous_id.is_some();
        }

        match self.browser_service.pending_native_dialog() {
            Ok(Some(pending)) => {
                if let Some(dialog) = self
                    .browser_dialog
                    .as_mut()
                    .filter(|dialog| dialog.pending.id == pending.id)
                {
                    dialog.pending = pending;
                } else {
                    let prompt_input = if pending.dialog.dialog_type == "prompt" {
                        pending.dialog.default_value.clone().unwrap_or_default()
                    } else {
                        String::new()
                    };
                    let prompt_cursor = prompt_input.chars().count();
                    self.status = if pending.dialog.dialog_type == "beforeunload" {
                        "Page navigation confirmation · page operation paused".into()
                    } else {
                        format!(
                            "JavaScript {} dialog · page operation paused",
                            pending.dialog.dialog_type
                        )
                    };
                    self.browser_dialog = Some(BrowserDialogPrompt {
                        pending,
                        prompt_input,
                        prompt_cursor,
                        error: None,
                    });
                }
            }
            Ok(None) => {
                if let Some(dialog) = self.browser_dialog.take() {
                    self.status = if dialog.pending.dialog.dialog_type == "beforeunload" {
                        "Page navigation decision resolved · page resuming".into()
                    } else {
                        "JavaScript dialog resolved · page resuming".into()
                    };
                }
            }
            Err(error) => {
                let message = error.to_string();
                if let Some(dialog) = self.browser_dialog.as_mut() {
                    dialog.error = Some(message.clone());
                }
                self.status = format!("Native browser dialog control failed · {message}");
            }
        }

        previous_id
            != self
                .browser_dialog
                .as_ref()
                .map(|dialog| dialog.pending.id.clone())
    }

    /// Accept the current native dialog and continue its original page call.
    pub fn accept_native_browser_dialog(&mut self) {
        self.resolve_native_browser_dialog(true);
    }

    /// Dismiss confirm/prompt dialogs. An alert has no cancel result, so Esc
    /// acknowledges it just like Enter.
    pub fn dismiss_native_browser_dialog(&mut self) {
        let is_alert = self
            .browser_dialog
            .as_ref()
            .is_some_and(|dialog| dialog.pending.dialog.dialog_type == "alert");
        self.resolve_native_browser_dialog(is_alert);
    }

    fn resolve_native_browser_dialog(&mut self, accepted: bool) {
        let Some(dialog) = self.browser_dialog.as_ref() else {
            return;
        };
        let id = dialog.pending.id.clone();
        let dialog_type = dialog.pending.dialog.dialog_type.clone();
        let prompt_value =
            (accepted && dialog_type == "prompt").then(|| dialog.prompt_input.clone());
        let resolution = NativeDialogResolution {
            accepted,
            prompt_value,
        };
        match self.browser_service.resolve_native_dialog(&id, resolution) {
            Ok(()) => {
                self.browser_dialog = None;
                self.status = native_browser_dialog_resolution_status(&dialog_type, accepted);
            }
            Err(error) => {
                let message = error.to_string();
                if let Some(dialog) = self.browser_dialog.as_mut() {
                    dialog.error = Some(message.clone());
                }
                self.status = format!("Dialog remains open · {message}");
            }
        }
    }

    pub fn insert_native_dialog_char(&mut self, character: char) {
        if character.is_control() {
            return;
        }
        let Some(dialog) = self.browser_dialog.as_mut() else {
            return;
        };
        if dialog.pending.dialog.dialog_type != "prompt" {
            return;
        }
        if dialog
            .prompt_input
            .len()
            .saturating_add(character.len_utf8())
            > NATIVE_DIALOG_TEXT_LIMIT_BYTES
        {
            dialog.error = Some(format!(
                "Prompt response is limited to {NATIVE_DIALOG_TEXT_LIMIT_BYTES} UTF-8 bytes"
            ));
            return;
        }
        let byte_offset = text_char_index_to_byte(&dialog.prompt_input, dialog.prompt_cursor);
        dialog.prompt_input.insert(byte_offset, character);
        dialog.prompt_cursor = dialog.prompt_cursor.saturating_add(1);
        dialog.error = None;
    }

    pub fn backspace_native_dialog_char(&mut self) {
        let Some(dialog) = self.browser_dialog.as_mut() else {
            return;
        };
        if dialog.pending.dialog.dialog_type != "prompt" || dialog.prompt_cursor == 0 {
            return;
        }
        let start = text_char_index_to_byte(&dialog.prompt_input, dialog.prompt_cursor - 1);
        let end = text_char_index_to_byte(&dialog.prompt_input, dialog.prompt_cursor);
        dialog.prompt_input.replace_range(start..end, "");
        dialog.prompt_cursor -= 1;
        dialog.error = None;
    }

    pub fn delete_native_dialog_char(&mut self) {
        let Some(dialog) = self.browser_dialog.as_mut() else {
            return;
        };
        if dialog.pending.dialog.dialog_type != "prompt"
            || dialog.prompt_cursor >= dialog.prompt_input.chars().count()
        {
            return;
        }
        let start = text_char_index_to_byte(&dialog.prompt_input, dialog.prompt_cursor);
        let end = text_char_index_to_byte(&dialog.prompt_input, dialog.prompt_cursor + 1);
        dialog.prompt_input.replace_range(start..end, "");
        dialog.error = None;
    }

    pub fn move_native_dialog_cursor_left(&mut self) {
        if let Some(dialog) = self.browser_dialog.as_mut()
            && dialog.pending.dialog.dialog_type == "prompt"
        {
            dialog.prompt_cursor = dialog.prompt_cursor.saturating_sub(1);
        }
    }

    pub fn move_native_dialog_cursor_right(&mut self) {
        if let Some(dialog) = self.browser_dialog.as_mut()
            && dialog.pending.dialog.dialog_type == "prompt"
        {
            dialog.prompt_cursor =
                (dialog.prompt_cursor + 1).min(dialog.prompt_input.chars().count());
        }
    }

    pub fn move_native_dialog_cursor_home(&mut self) {
        if let Some(dialog) = self.browser_dialog.as_mut()
            && dialog.pending.dialog.dialog_type == "prompt"
        {
            dialog.prompt_cursor = 0;
        }
    }

    pub fn move_native_dialog_cursor_end(&mut self) {
        if let Some(dialog) = self.browser_dialog.as_mut()
            && dialog.pending.dialog.dialog_type == "prompt"
        {
            dialog.prompt_cursor = dialog.prompt_input.chars().count();
        }
    }

    /// Route one key exclusively to the current native page dialog.
    pub fn handle_native_browser_dialog_key(
        &mut self,
        code: crossterm::event::KeyCode,
        modifiers: crossterm::event::KeyModifiers,
    ) {
        let Some(dialog_type) = self
            .browser_dialog
            .as_ref()
            .map(|dialog| dialog.pending.dialog.dialog_type.clone())
        else {
            return;
        };
        use crossterm::event::KeyCode;

        match code {
            KeyCode::Esc => self.dismiss_native_browser_dialog(),
            KeyCode::Enter => self.accept_native_browser_dialog(),
            KeyCode::Char('y' | 'Y')
                if matches!(dialog_type.as_str(), "confirm" | "beforeunload") =>
            {
                self.accept_native_browser_dialog()
            }
            KeyCode::Char('n' | 'N')
                if matches!(dialog_type.as_str(), "confirm" | "beforeunload") =>
            {
                self.dismiss_native_browser_dialog()
            }
            KeyCode::Backspace => self.backspace_native_dialog_char(),
            KeyCode::Delete => self.delete_native_dialog_char(),
            KeyCode::Left => self.move_native_dialog_cursor_left(),
            KeyCode::Right => self.move_native_dialog_cursor_right(),
            KeyCode::Home => self.move_native_dialog_cursor_home(),
            KeyCode::End => self.move_native_dialog_cursor_end(),
            KeyCode::Char(character)
                if dialog_type == "prompt"
                    && !modifiers.contains(crossterm::event::KeyModifiers::CONTROL)
                    && !modifiers.contains(crossterm::event::KeyModifiers::ALT) =>
            {
                self.insert_native_dialog_char(character)
            }
            _ => {}
        }
    }

    /// Resolve any open native page prompt before the TUI tears down its
    /// browser worker after confirmed quit.
    fn dismiss_native_browser_dialog_for_quit(&mut self) -> Result<(), String> {
        let enabled = self
            .browser_service
            .native_dialog_control_enabled()
            .map_err(|error| error.to_string())?;
        if !enabled {
            self.browser_dialog = None;
            return Ok(());
        }
        let pending = self
            .browser_service
            .pending_native_dialog()
            .map_err(|error| error.to_string())?;
        if let Some(pending) = pending {
            let accepted = matches!(
                pending.dialog.dialog_type.as_str(),
                "alert" | "beforeunload"
            );
            self.browser_service
                .resolve_native_dialog(
                    &pending.id,
                    NativeDialogResolution {
                        accepted,
                        prompt_value: None,
                    },
                )
                .map_err(|error| error.to_string())?;
        }
        self.browser_dialog = None;
        Ok(())
    }

    pub fn browser_workspace_summary(&self) -> String {
        let browser = self.browser_workspace.state();
        let entities = if browser.entities.is_empty() {
            "No current semantic entities".into()
        } else {
            browser
                .entities
                .iter()
                .enumerate()
                .skip(browser.semantic_scroll)
                .take(24)
                .map(|(index, entity)| {
                    format!(
                        "{} {} · {} · {}",
                        if Some(index) == browser.selected_entity {
                            "◆"
                        } else {
                            "○"
                        },
                        entity.name,
                        entity.role,
                        entity.reference
                    )
                })
                .collect::<Vec<_>>()
                .join("\n")
        };
        let targets = if browser.targets.is_empty() {
            "No page targets loaded · use `browser targets`".into()
        } else {
            browser
                .targets
                .iter()
                .take(8)
                .map(|target| {
                    format!(
                        "{} {} · {}",
                        if target.selected { "◆" } else { "○" },
                        target.title,
                        safe_browser_url(&target.url).unwrap_or_else(|| "no URL".into())
                    )
                })
                .collect::<Vec<_>>()
                .join("\n")
        };
        format!(
            "{} · rev {} · {} · owner {}\n{}\n{}\n\nTARGETS\n{}\n\nUNDERSTANDING\n{}\n\nWORKFLOW\n{}",
            browser.connection_label(),
            browser
                .browser_revision
                .map_or_else(|| "—".into(), |revision| revision.to_string()),
            browser.presentation_label(),
            browser.input_owner_label(),
            browser.title,
            safe_browser_url(&browser.url).unwrap_or_else(|| "no page".into()),
            targets,
            entities,
            browser.workflow
        )
    }

    pub fn queue_browser_targets(&mut self, worker: &mut super::snapshot::SnapshotWorker) {
        if self.background_action_running() {
            self.status = "Target picker waits for the current background operation".into();
            return;
        }
        if let Err(error) = self.request_browser_target_picker("") {
            self.status = format!("Could not load browser targets: {error}");
            return;
        }
        let Some((call, context)) = self.queued_tool_request.take() else {
            return;
        };
        match worker.submit_tool(call, context) {
            Ok(id) => {
                self.running_tool_job = Some(id);
                self.status = "Loading browser targets…".into();
            }
            Err(error) => {
                self.browser_target_picker_requested = false;
                self.status = format!("Could not load browser targets: {error}");
            }
        }
    }

    pub fn request_browser_target_picker(
        &mut self,
        query: impl Into<String>,
    ) -> Result<(), String> {
        if self.background_action_running() {
            return Err("Target picker waits for the current background operation".into());
        }
        self.browser_target_query = query.into();
        self.browser_target_selection = 0;
        self.browser_target_picker = false;
        self.browser_target_picker_requested = true;
        let (call, context) =
            match self.tool_request("glass.browser.targets", serde_json::json!({}), false) {
                Ok(request) => request,
                Err(error) => {
                    self.browser_target_picker_requested = false;
                    return Err(error);
                }
            };
        self.queued_tool_request = Some((call, context));
        self.status = "Loading browser targets…".into();
        Ok(())
    }

    pub fn close_browser_target_picker(&mut self) {
        self.browser_target_picker = false;
        self.browser_target_picker_requested = false;
        self.status = "Target picker closed".into();
    }

    pub fn browser_target_matches(&self) -> Vec<usize> {
        let query = self.browser_target_query.to_ascii_lowercase();
        let terms = query.split_whitespace().collect::<Vec<_>>();
        self.browser_workspace
            .state()
            .targets
            .iter()
            .enumerate()
            .filter(|(_, target)| {
                let safe_url = safe_browser_url(&target.url)
                    .unwrap_or_default()
                    .to_ascii_lowercase();
                terms.iter().all(|term| {
                    target.id.to_ascii_lowercase().contains(term)
                        || target.title.to_ascii_lowercase().contains(term)
                        || safe_url.contains(term)
                })
            })
            .map(|(index, _)| index)
            .collect()
    }

    pub fn insert_browser_target_query(&mut self, character: char) {
        let selected_id = self.selected_browser_target_id();
        if self.browser_target_query.len() < 512 {
            self.browser_target_query.push(character);
            self.preserve_browser_target_selection(selected_id.as_deref());
        }
    }

    pub fn browser_target_backspace(&mut self) {
        let selected_id = self.selected_browser_target_id();
        self.browser_target_query.pop();
        self.preserve_browser_target_selection(selected_id.as_deref());
    }

    pub fn clear_browser_target_query(&mut self) {
        let selected_id = self.selected_browser_target_id();
        self.browser_target_query.clear();
        self.preserve_browser_target_selection(selected_id.as_deref());
    }

    fn selected_browser_target_id(&self) -> Option<String> {
        let target_index = *self
            .browser_target_matches()
            .get(self.browser_target_selection)?;
        self.browser_workspace
            .state()
            .targets
            .get(target_index)
            .map(|target| target.id.clone())
    }

    fn preserve_browser_target_selection(&mut self, selected_id: Option<&str>) {
        let matches = self.browser_target_matches();
        if let Some(selected_id) = selected_id
            && let Some(selection) = matches.iter().position(|target_index| {
                self.browser_workspace.state().targets[*target_index].id == selected_id
            })
        {
            self.browser_target_selection = selection;
        } else {
            self.browser_target_selection = self
                .browser_target_selection
                .min(matches.len().saturating_sub(1));
        }
    }

    pub fn move_browser_target_selection(&mut self, delta: i32) {
        let count = self.browser_target_matches().len();
        if count == 0 {
            self.browser_target_selection = 0;
            return;
        }
        self.browser_target_selection =
            (self.browser_target_selection as i32 + delta).rem_euclid(count as i32) as usize;
    }

    pub fn select_browser_target(&mut self) {
        if self.background_action_running() {
            self.status = "Finish the current browser operation before selecting a target".into();
            return;
        }
        let matches = self.browser_target_matches();
        let Some(index) = matches.get(self.browser_target_selection).copied() else {
            self.status = "No matching browser target".into();
            return;
        };
        let Some(target) = self.browser_workspace.state().targets.get(index).cloned() else {
            self.status = "Selected browser target disappeared".into();
            return;
        };
        let (call, context) = match self.tool_request(
            "glass.browser.target.select",
            serde_json::json!({"targetId": target.id}),
            true,
        ) {
            Ok(request) => request,
            Err(error) => {
                self.status = format!("Target selection unavailable: {error}");
                return;
            }
        };
        self.browser_target_picker = false;
        let queued = match self.queue_or_confirm(
            call,
            context,
            format!("Select browser target · {}", target.title),
        ) {
            Ok(queued) => queued,
            Err(error) => {
                self.status = format!("Target selection unavailable: {error}");
                return;
            }
        };
        if !queued {
            self.status = "Target selection ready · Enter approves once · Esc cancels".into();
        }
    }

    pub fn browser_target_picker_view(&self) -> String {
        let matches = self.browser_target_matches();
        let mut lines = vec![
            format!(
                "FILTER  {}",
                if self.browser_target_query.is_empty() {
                    "all pages"
                } else {
                    self.browser_target_query.as_str()
                }
            ),
            String::new(),
        ];
        if matches.is_empty() {
            lines.push("No matching page targets.".into());
        } else {
            for (position, index) in matches.iter().take(16).enumerate() {
                let Some(target) = self.browser_workspace.state().targets.get(*index) else {
                    continue;
                };
                let marker = if position == self.browser_target_selection {
                    "›"
                } else {
                    " "
                };
                let url = safe_browser_url(&target.url).unwrap_or_else(|| "no URL".into());
                lines.push(format!(
                    "{marker} {} · {} · {}",
                    target.title, url, target.id
                ));
            }
        }
        lines.push(String::new());
        lines.push("Type filters · Backspace edits · Ctrl-U clears".into());
        lines.push("j/k or ↑/↓ selects · Enter chooses · Esc closes".into());
        lines.join("\n")
    }

    pub fn queue_browser_observe(&mut self, worker: &mut super::snapshot::SnapshotWorker) {
        if !self.browser_observe_pending || self.blocking_background_tool_running() {
            return;
        }
        if self
            .browser_observe_retry_after
            .is_some_and(|retry_after| std::time::Instant::now() < retry_after)
        {
            return;
        }
        let (call, context) = match self.tool_request(
            "glass.browser.observe",
            serde_json::json!({}),
            false,
        ) {
            Ok(request) => request,
            Err(error) => {
                self.browser_observe_retry_count =
                    self.browser_observe_retry_count.saturating_add(1);
                self.browser_observe_pending = self.browser_observe_retry_count <= 3;
                self.browser_observe_retry_after = self
                    .browser_observe_pending
                    .then(|| std::time::Instant::now() + std::time::Duration::from_secs(2));
                self.status = if self.browser_observe_pending {
                    format!(
                        "Browser evidence refresh failed ({}/3); retrying: {error}",
                        self.browser_observe_retry_count
                    )
                } else {
                    self.browser_observe_retry_count = 0;
                    format!(
                        "Browser evidence may be stale · refresh failed after 3 attempts: {error}"
                    )
                };
                return;
            }
        };
        match worker.submit_tool(call, context) {
            Ok(id) => {
                self.browser_observe_pending = false;
                self.browser_observe_retry_after = None;
                self.running_tool_job = Some(id);
                self.status = "Refreshing semantic browser evidence…".into();
            }
            Err(error) => {
                self.browser_observe_retry_count =
                    self.browser_observe_retry_count.saturating_add(1);
                self.browser_observe_pending = self.browser_observe_retry_count <= 3;
                self.browser_observe_retry_after = self
                    .browser_observe_pending
                    .then(|| std::time::Instant::now() + std::time::Duration::from_secs(2));
                self.status = if self.browser_observe_pending {
                    format!(
                        "Browser evidence refresh failed ({}/3); retrying: {error}",
                        self.browser_observe_retry_count
                    )
                } else {
                    self.browser_observe_retry_count = 0;
                    format!(
                        "Browser evidence may be stale · refresh failed after 3 attempts: {error}"
                    )
                };
            }
        }
    }

    /// Refresh the newly started page before issuing a revision-bound navigation.
    pub fn continue_pending_browser_navigation(
        &mut self,
        worker: &mut super::snapshot::SnapshotWorker,
    ) {
        let Some(url) = self.pending_browser_navigation.as_deref() else {
            return;
        };
        let (call, context) =
            match self.tool_request("glass.browser.observe", serde_json::json!({}), false) {
                Ok(request) => request,
                Err(error) => {
                    let url = self.pending_browser_navigation.take().unwrap_or_default();
                    self.open_palette_with(&format!("browser navigate {url}"));
                    self.status = format!("Browser ready · navigation retry required: {error}");
                    return;
                }
            };
        match worker.submit_tool(call, context) {
            Ok(id) => {
                self.running_tool_job = Some(id);
                self.status = format!("Browser ready · preparing navigation to {url}…");
            }
            Err(error) => {
                let url = self.pending_browser_navigation.take().unwrap_or_default();
                self.open_palette_with(&format!("browser navigate {url}"));
                self.status = format!("Browser ready · navigation retry required: {error}");
            }
        }
    }

    /// Submit the retained navigation after its fresh page revision arrives.
    pub fn submit_pending_browser_navigation(
        &mut self,
        worker: &mut super::snapshot::SnapshotWorker,
    ) {
        let Some(url) = self.pending_browser_navigation.take() else {
            return;
        };
        let Some(revision) = self.browser_workspace.state().browser_revision else {
            self.open_palette_with(&format!("browser navigate {url}"));
            self.status = "Browser launch completed without a usable revision".into();
            return;
        };
        let (call, context) = match self.tool_request(
            "glass.browser.navigate",
            serde_json::json!({"url": url, "browserRevision": revision}),
            true,
        ) {
            Ok(request) => request,
            Err(error) => {
                self.open_palette_with(&format!("browser navigate {url}"));
                self.status = format!("Browser ready · navigation retry required: {error}");
                return;
            }
        };
        match worker.submit_tool(call, context) {
            Ok(id) => {
                self.running_tool_job = Some(id);
                self.status = format!("Browser ready · navigating to {url}…");
            }
            Err(error) => {
                self.open_palette_with(&format!("browser navigate {url}"));
                self.status = format!("Browser ready · navigation retry required: {error}");
            }
        }
    }

    pub fn queue_browser_intent(&mut self, intent: BrowserWorkspaceIntent) {
        let moved_selection = matches!(&intent, BrowserWorkspaceIntent::MoveSelection { .. });
        if self.background_action_running() {
            self.status = "Browser action waits for the current background operation".into();
            return;
        }
        let action = match self.browser_workspace.reduce(intent) {
            Ok(Some(action)) => action,
            Ok(None) => {
                self.browser = self.browser_workspace_summary();
                if moved_selection {
                    self.status = "Selection moved · Enter activates the selected entity".into();
                }
                return;
            }
            Err(error) => {
                self.status = format!("App action unavailable: {error}");
                return;
            }
        };
        let (tool, arguments) = match action {
            BrowserWorkspaceAction::Navigate {
                url,
                expected_revision,
            } => (
                "glass.browser.navigate",
                serde_json::json!({"url": url, "browserRevision": expected_revision}),
            ),
            BrowserWorkspaceAction::Back { expected_revision }
            | BrowserWorkspaceAction::Forward { expected_revision }
            | BrowserWorkspaceAction::Reload { expected_revision }
            | BrowserWorkspaceAction::StopLoading { expected_revision } => {
                let action = match action {
                    BrowserWorkspaceAction::Back { .. } => "back",
                    BrowserWorkspaceAction::Forward { .. } => "forward",
                    BrowserWorkspaceAction::Reload { .. } => "reload",
                    BrowserWorkspaceAction::StopLoading { .. } => "stopLoading",
                    _ => unreachable!(),
                };
                (
                    "glass.browser.act",
                    serde_json::json!({"action": action, "browserRevision": expected_revision}),
                )
            }
            BrowserWorkspaceAction::Click {
                target,
                expected_revision,
            } => {
                let _ = self.capture_workflow_click();
                (
                    "glass.browser.act",
                    serde_json::json!({"action":"click", "target": target, "browserRevision": expected_revision}),
                )
            }
            BrowserWorkspaceAction::Type {
                target,
                text,
                expected_revision,
            } => {
                self.capture_workflow_type_from_selection();
                (
                    "glass.browser.act",
                    serde_json::json!({"action":"type", "target": target, "text": text, "browserRevision": expected_revision}),
                )
            }
            BrowserWorkspaceAction::Scroll {
                dx,
                dy,
                expected_revision,
            } => (
                "glass.browser.act",
                serde_json::json!({"action":"scroll", "dx": dx, "dy": dy, "browserRevision": expected_revision}),
            ),
            _ => return,
        };
        let (call, context) = match self.tool_request(tool, arguments, true) {
            Ok(request) => request,
            Err(error) => {
                self.status = format!("App action unavailable: {error}");
                return;
            }
        };
        let queued = match self.queue_or_confirm(
            call,
            context,
            format!("{tool} · browser revision guarded"),
        ) {
            Ok(queued) => queued,
            Err(error) => {
                self.status = format!("App action unavailable: {error}");
                return;
            }
        };
        if !queued {
            self.status = "Browser mutation ready · Enter approves once · Esc cancels".into();
        }
    }

    pub(super) fn tool_request(
        &self,
        name: &str,
        arguments: serde_json::Value,
        mutating: bool,
    ) -> Result<
        (
            crate::development::ToolCall,
            crate::tools::DevelopmentToolContext,
        ),
        String,
    > {
        // Snapshot refresh owns the workspace lock while it performs filesystem
        // and process inspection. Cached revisions keep chat and actions
        // responsive; the executor still rejects stale guards before mutation.
        let expected_generation = self.snapshot_generation;
        let expected_project_revision = self.snapshot_project_revision;
        Ok((
            crate::development::ToolCall {
                id: format!(
                    "tui-browser-{}",
                    NEXT_BROWSER_TOOL.fetch_add(1, Ordering::Relaxed)
                ),
                name: name.into(),
                arguments,
            },
            crate::tools::DevelopmentToolContext {
                authorization: crate::development::ToolAuthorization::from_user_confirmation(
                    crate::development::Actor::local(),
                    mutating,
                    mutating,
                ),
                initiator: None,
                expected_generation,
                expected_project_revision,
            },
        ))
    }

    /// Offer in-TUI recovery whenever a browser tool fails with a launch or
    /// connection error, so a port collision never strands the session.
    pub fn note_browser_failure(&mut self, tool: &str, error: &str) -> bool {
        let lower = error.to_ascii_lowercase();
        if tool.contains("browser")
            && (lower.contains("occupied")
                || lower.contains("address already in use")
                || lower.contains("connection refused")
                || lower.contains("endpoint crashed")
                || lower.contains("launch failure")
                || lower.contains("timed out")
                || lower.contains("devtools"))
        {
            let native_backend =
                self.browser_workspace.state().backend.as_deref() == Some("native");
            let port = self
                .browser_recovery
                .as_ref()
                .map_or(9222, |offer| offer.port);
            self.offer_browser_recovery_for_backend(error, port, native_backend);
            true
        } else {
            false
        }
    }

    fn offer_browser_recovery_for_backend(&mut self, reason: &str, port: u16, native: bool) {
        self.browser_recovery = Some(BrowserRecoveryOffer::from_error_for_backend(
            reason, port, native,
        ));
        self.browser_workspace
            .disconnected(reason.to_string(), true);
        self.browser_workspace.state_mut().frame_revision = None;
        self.browser_pane = None;
        self.status = BROWSER_RECOVERY_STATUS.into();
    }

    pub fn dismiss_browser_recovery(&mut self) {
        self.browser_recovery = None;
        self.pending_browser_navigation = None;
        self.status = format!(
            "Browser recovery dismissed · {} remains active",
            self.surface.label()
        );
    }

    /// Queue a recovery choice without blocking the terminal on browser launch.
    pub fn accept_browser_recovery(
        &mut self,
        choice: usize,
        worker: &mut super::snapshot::SnapshotWorker,
    ) {
        let Some(offer) = self.browser_recovery.take() else {
            return;
        };
        if offer.native_backend {
            if choice != 0 {
                self.status = "Recovery dismissed · project and agent remain available".into();
                return;
            }
            let request = self.tool_request(
                "glass.browser.start",
                serde_json::json!({"backend":"native"}),
                true,
            );
            match request.and_then(|(call, context)| {
                worker
                    .submit_tool(call, context)
                    .map_err(|error| error.to_string())
            }) {
                Ok(id) => {
                    self.running_tool_job = Some(id);
                    self.status = "Recovering the native browser in background…".into();
                }
                Err(error) => self.status = format!("Could not queue native recovery: {error}"),
            }
            return;
        }
        let (port, attach) = match (offer.compatible_endpoint, choice) {
            (true, 0) => (offer.port, true),
            (true, 1) | (false, 0) => (free_local_port().unwrap_or(0), false),
            (true, 2) | (false, 1) => (offer.port, false),
            _ => {
                self.status = "Recovery dismissed · project and agent remain available".into();
                return;
            }
        };
        if port == 0 {
            self.status =
                "No free local port was available · retry or use an existing browser".into();
            return;
        }
        let (call, context) = match self.tool_request(
            "glass.browser.start",
            browser_recovery_arguments(port, attach),
            true,
        ) {
            Ok(request) => request,
            Err(error) => {
                self.status = format!("Could not queue recovery: {error}");
                return;
            }
        };
        match worker.submit_tool(call, context) {
            Ok(id) => {
                self.running_tool_job = Some(id);
                self.status = if attach {
                    format!("Attaching to browser on port {port} in background…")
                } else {
                    format!("Recovering browser on port {port} in background…")
                };
            }
            Err(error) => self.status = format!("Could not queue recovery: {error}"),
        }
    }

    pub fn refresh_agent_readiness(&mut self) -> Result<bool, String> {
        let readiness =
            crate::resident_harness::resident_readiness().map_err(|error| error.to_string())?;
        let ready = readiness.ready;
        self.agent_readiness = format_resident_readiness(&readiness);
        Ok(ready)
    }
}

fn bounded_preview(value: &str) -> String {
    const LIMIT: usize = 220;
    let mut result = value.chars().take(LIMIT).collect::<String>();
    if value.chars().count() > LIMIT {
        result.push('…');
    }
    result
}

fn bounded_line_diff(local: &str, external: &str) -> String {
    const MAX_CHANGED_LINES: usize = 8;
    const MAX_LINE_CHARS: usize = 100;
    let left = local.lines().collect::<Vec<_>>();
    let right = external.lines().collect::<Vec<_>>();
    let operations = pair_adjacent_changes(aligned_line_operations(&left, &right));
    let changed_count = operations
        .iter()
        .map(|operation| match operation {
            LineOperation::Keep => 0,
            LineOperation::Replace(_, _) => 2,
            LineOperation::Delete(_) | LineOperation::Insert(_) => 1,
        })
        .sum::<usize>();
    if changed_count == 0 {
        return "no line differences".into();
    }
    let mut diff = vec![
        "--- local buffer".to_string(),
        "+++ external disk snapshot".to_string(),
    ];
    let (mut old_line, mut new_line, mut shown) = (1usize, 1usize, 0usize);
    for operation in operations {
        match operation {
            LineOperation::Keep => {
                old_line += 1;
                new_line += 1;
            }
            LineOperation::Delete(line) => {
                if shown == MAX_CHANGED_LINES {
                    break;
                }
                diff.push(format!("@@ -{old_line} +{new_line} @@"));
                diff.push(format!(
                    "-{}",
                    bounded_preview(&line.chars().take(MAX_LINE_CHARS).collect::<String>())
                ));
                old_line += 1;
                shown += 1;
            }
            LineOperation::Insert(line) => {
                if shown == MAX_CHANGED_LINES {
                    break;
                }
                diff.push(format!("@@ -{old_line} +{new_line} @@"));
                diff.push(format!(
                    "+{}",
                    bounded_preview(&line.chars().take(MAX_LINE_CHARS).collect::<String>())
                ));
                new_line += 1;
                shown += 1;
            }
            LineOperation::Replace(old, new) => {
                if shown + 2 > MAX_CHANGED_LINES {
                    break;
                }
                diff.push(format!("@@ -{old_line} +{new_line} @@"));
                diff.push(format!(
                    "-{}",
                    bounded_preview(&old.chars().take(MAX_LINE_CHARS).collect::<String>())
                ));
                diff.push(format!(
                    "+{}",
                    bounded_preview(&new.chars().take(MAX_LINE_CHARS).collect::<String>())
                ));
                old_line += 1;
                new_line += 1;
                shown += 2;
            }
        }
    }
    if changed_count > MAX_CHANGED_LINES {
        diff.push("… diff truncated".into());
    }
    diff.join("\n")
}

enum LineOperation<'a> {
    Keep,
    Delete(&'a str),
    Insert(&'a str),
    Replace(&'a str, &'a str),
}

fn pair_adjacent_changes<'a>(operations: Vec<LineOperation<'a>>) -> Vec<LineOperation<'a>> {
    let mut paired = Vec::with_capacity(operations.len());
    let mut index = 0;
    while index < operations.len() {
        if matches!(operations[index], LineOperation::Keep) {
            paired.push(LineOperation::Keep);
            index += 1;
            continue;
        }
        let end = operations[index..]
            .iter()
            .position(|operation| matches!(operation, LineOperation::Keep))
            .map_or(operations.len(), |offset| index + offset);
        let mut deleted = Vec::new();
        let mut inserted = Vec::new();
        for operation in &operations[index..end] {
            match operation {
                LineOperation::Delete(line) => deleted.push(*line),
                LineOperation::Insert(line) => inserted.push(*line),
                LineOperation::Keep | LineOperation::Replace(_, _) => unreachable!(),
            }
        }
        paired.extend(
            deleted
                .iter()
                .zip(&inserted)
                .map(|(old, new)| LineOperation::Replace(old, new)),
        );
        paired.extend(
            deleted[inserted.len().min(deleted.len())..]
                .iter()
                .copied()
                .map(LineOperation::Delete),
        );
        paired.extend(
            inserted[deleted.len().min(inserted.len())..]
                .iter()
                .copied()
                .map(LineOperation::Insert),
        );
        index = end;
    }
    paired
}

/// Align common lines before applying the display bound. The dynamic-programming
/// table is capped; unusually large inputs fall back to a common-prefix/suffix
/// replacement block so the work and memory remain bounded.
fn aligned_line_operations<'a>(left: &[&'a str], right: &[&'a str]) -> Vec<LineOperation<'a>> {
    const MAX_LCS_CELLS: usize = 1_000_000;
    let cells = left.len().saturating_mul(right.len());
    if cells > MAX_LCS_CELLS {
        let prefix = left
            .iter()
            .zip(right)
            .take_while(|(old, new)| old == new)
            .count();
        let suffix = left[prefix..]
            .iter()
            .rev()
            .zip(right[prefix..].iter().rev())
            .take_while(|(old, new)| old == new)
            .count();
        let mut operations = Vec::new();
        operations.extend((0..prefix).map(|_| LineOperation::Keep));
        operations.extend(
            left[prefix..left.len() - suffix]
                .iter()
                .copied()
                .map(LineOperation::Delete),
        );
        operations.extend(
            right[prefix..right.len() - suffix]
                .iter()
                .copied()
                .map(LineOperation::Insert),
        );
        operations.extend((0..suffix).map(|_| LineOperation::Keep));
        return operations;
    }

    let columns = right.len() + 1;
    let mut lcs = vec![0_u32; (left.len() + 1).saturating_mul(columns)];
    for old in (0..left.len()).rev() {
        for new in (0..right.len()).rev() {
            let index = old * columns + new;
            lcs[index] = if left[old] == right[new] {
                lcs[(old + 1) * columns + new + 1] + 1
            } else {
                lcs[(old + 1) * columns + new].max(lcs[old * columns + new + 1])
            };
        }
    }
    let (mut old, mut new) = (0, 0);
    let mut operations = Vec::with_capacity(left.len() + right.len());
    while old < left.len() && new < right.len() {
        if left[old] == right[new] {
            operations.push(LineOperation::Keep);
            old += 1;
            new += 1;
        } else if lcs[(old + 1) * columns + new] >= lcs[old * columns + new + 1] {
            operations.push(LineOperation::Delete(left[old]));
            old += 1;
        } else {
            operations.push(LineOperation::Insert(right[new]));
            new += 1;
        }
    }
    operations.extend(left[old..].iter().copied().map(LineOperation::Delete));
    operations.extend(right[new..].iter().copied().map(LineOperation::Insert));
    operations
}

fn diagnostic_matches_path(
    diagnostic: &crate::development::LanguageDiagnostic,
    path: &str,
) -> bool {
    path.is_empty()
        || diagnostic.path == path
        || diagnostic.path.ends_with(path)
        || path.ends_with(&diagnostic.path)
}

fn truncate_note(text: &str) -> String {
    let text = text.trim().replace('\n', " ");
    if text.chars().count() <= 80 {
        text
    } else {
        format!("{}…", text.chars().take(79).collect::<String>())
    }
}

fn parse_editor_diagnostics(
    value: &serde_json::Value,
) -> Vec<crate::development::LanguageDiagnostic> {
    if let Ok(list) =
        serde_json::from_value::<Vec<crate::development::LanguageDiagnostic>>(value.clone())
    {
        return list;
    }
    value
        .get("diagnostics")
        .and_then(|item| {
            serde_json::from_value::<Vec<crate::development::LanguageDiagnostic>>(item.clone()).ok()
        })
        .unwrap_or_default()
}

fn more_route_status(command: &str, message: &str) -> String {
    match command {
        "doctor" if message.contains("is ready") => {
            "Doctor · Agent is ready · Core panel updated · 1 opens chat".into()
        }
        "doctor" => format!("Doctor · {message}"),
        "harness list" => {
            let installed = message
                .lines()
                .filter(|line| line.trim_start().starts_with('●'))
                .count();
            format!("Harness catalog refreshed · {installed} installed")
        }
        _ => message.to_string(),
    }
}

fn format_experiment_snapshots(snapshots: &[crate::experiments::ExperimentSnapshot]) -> String {
    if snapshots.is_empty() {
        return "No experiments".into();
    }
    snapshots
        .iter()
        .map(|experiment| {
            format!(
                "{} {} · {} · port {} · agent {}",
                if experiment.evidence.tests_failed == 0 && experiment.evidence.tests_passed > 0 {
                    "✓"
                } else {
                    "○"
                },
                experiment.id,
                experiment.state.label(),
                experiment
                    .port
                    .map_or_else(|| "—".into(), |port| port.to_string()),
                experiment
                    .agent_id
                    .as_ref()
                    .map(|id| id.as_str())
                    .unwrap_or("none")
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn format_editor_buffers(buffers: &[crate::development::EditorBuffer]) -> String {
    if buffers.is_empty() {
        return "No file open. Select a file below and press Enter to open the full-screen editor."
            .into();
    }
    buffers
        .iter()
        .map(|buffer| {
            let lines: Vec<&str> = buffer.content.lines().collect();
            let cursor = buffer.cursor_line as usize;
            let viewport_rows = 16;
            let start = cursor
                .saturating_sub(viewport_rows / 2)
                .min(lines.len().saturating_sub(viewport_rows.min(lines.len())));
            let end = (start + viewport_rows).min(lines.len());
            let gutter_width = lines.len().to_string().len().max(3);
            let viewport = lines[start..end]
                .iter()
                .enumerate()
                .map(|(index, line)| {
                    let number = start + index + 1;
                    let marker = if number == cursor { "▶" } else { " " };
                    format!(
                        "{marker}{:>gutter_width$} │ {line}",
                        number,
                        gutter_width = gutter_width
                    )
                })
                .collect::<Vec<_>>()
                .join("\n");
            format!(
                "{}{} · cursor {}:{} · actor {} · {} lines\n{}",
                if buffer.dirty { "● " } else { "○ " },
                buffer.path,
                buffer.cursor_line,
                buffer.cursor_column,
                buffer.actor.id,
                lines.len(),
                viewport
            )
        })
        .collect::<Vec<_>>()
        .join("\n\n")
}

fn parse_conversation_view(conversation: &str) -> Vec<super::projection::ConversationEntry> {
    let mut entries = Vec::new();
    let mut current: Option<(super::projection::ConversationKind, Vec<&str>)> = None;
    for line in conversation.lines() {
        let kind = match line.trim() {
            "YOU" => Some(super::projection::ConversationKind::User),
            "GLASS AGENT" => Some(super::projection::ConversationKind::Assistant),
            "ALERT" => Some(super::projection::ConversationKind::Alert),
            "ERROR" => Some(super::projection::ConversationKind::Error),
            "SYSTEM" => Some(super::projection::ConversationKind::System),
            _ => None,
        };
        if let Some(kind) = kind {
            if let Some((previous_kind, lines)) = current.take() {
                entries.push(conversation_entry_from_lines(previous_kind, lines));
            }
            current = Some((kind, Vec::new()));
        } else if let Some((_, lines)) = &mut current {
            lines.push(line);
        }
    }
    if let Some((kind, lines)) = current {
        entries.push(conversation_entry_from_lines(kind, lines));
    }
    entries
}

fn conversation_entry_from_lines(
    kind: super::projection::ConversationKind,
    lines: Vec<&str>,
) -> super::projection::ConversationEntry {
    super::projection::ConversationEntry {
        kind,
        text: lines.join("\n").trim_matches('\n').to_string(),
        streaming: false,
        entry_id: None,
        client_message_id: None,
        tool_name: None,
        tool_card: None,
        core_ui: None,
    }
}

fn parse_debug_threads(value: &serde_json::Value) -> Vec<DebugThreadRow> {
    value
        .get("threads")
        .and_then(serde_json::Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|thread| {
            Some(DebugThreadRow {
                id: thread.get("id").and_then(serde_json::Value::as_i64)?,
                name: thread
                    .get("name")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or("thread")
                    .to_string(),
            })
        })
        .collect()
}

fn parse_debug_scopes(value: &serde_json::Value) -> Vec<DebugScopeRow> {
    value
        .get("scopes")
        .and_then(serde_json::Value::as_array)
        .into_iter()
        .flatten()
        .map(|scope| DebugScopeRow {
            name: scope
                .get("name")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("scope")
                .to_string(),
            variables_reference: scope
                .get("variablesReference")
                .and_then(serde_json::Value::as_i64)
                .unwrap_or(0),
        })
        .collect()
}

fn parse_debug_variables(value: &serde_json::Value) -> Vec<DebugVariableRow> {
    value
        .get("variables")
        .and_then(serde_json::Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|variable| {
            Some(DebugVariableRow {
                name: variable
                    .get("name")
                    .and_then(serde_json::Value::as_str)?
                    .to_string(),
                value: variable
                    .get("value")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or("")
                    .chars()
                    .take(120)
                    .collect(),
            })
        })
        .take(64)
        .collect()
}

fn parse_debug_frames(value: &serde_json::Value) -> Vec<DebugFrameRow> {
    value
        .get("stackFrames")
        .and_then(serde_json::Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|frame| {
            Some(DebugFrameRow {
                id: frame.get("id").and_then(serde_json::Value::as_i64)?,
                name: frame
                    .get("name")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or("frame")
                    .to_string(),
                path: frame
                    .pointer("/source/path")
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_string),
                line: frame.get("line").and_then(serde_json::Value::as_u64),
            })
        })
        .collect()
}

fn parse_session_picker_items(value: &serde_json::Value) -> Vec<SessionPickerItem> {
    let items = value
        .as_array()
        .or_else(|| value.get("sessions").and_then(serde_json::Value::as_array))
        .cloned()
        .unwrap_or_default();
    items
        .into_iter()
        .filter_map(|item| {
            if let Some(path) = item.as_str() {
                return Some(SessionPickerItem {
                    label: std::path::Path::new(path)
                        .file_name()
                        .and_then(|name| name.to_str())
                        .unwrap_or(path)
                        .to_string(),
                    path: path.to_string(),
                });
            }
            let path = item
                .get("path")
                .or_else(|| item.get("file"))
                .or_else(|| item.get("sessionFile"))
                .and_then(serde_json::Value::as_str)?;
            let label = item
                .get("name")
                .or_else(|| item.get("sessionName"))
                .and_then(serde_json::Value::as_str)
                .map(str::to_string)
                .unwrap_or_else(|| {
                    std::path::Path::new(path)
                        .file_name()
                        .and_then(|name| name.to_str())
                        .unwrap_or(path)
                        .to_string()
                });
            Some(SessionPickerItem {
                path: path.to_string(),
                label,
            })
        })
        .collect()
}

fn stats_summary(value: &serde_json::Value) -> String {
    let input = value
        .pointer("/inputTokens")
        .or_else(|| value.pointer("/tokens/input"))
        .or_else(|| value.get("input"))
        .and_then(serde_json::Value::as_u64);
    let output = value
        .pointer("/outputTokens")
        .or_else(|| value.pointer("/tokens/output"))
        .or_else(|| value.get("output"))
        .and_then(serde_json::Value::as_u64);
    match (input, output) {
        (Some(input), Some(output)) => format!("{input} in · {output} out"),
        (Some(input), None) => format!("{input} tokens"),
        _ => super::projection::first_meaningful(value)
            .lines()
            .next()
            .unwrap_or("stats")
            .to_string(),
    }
}

/// Status line for the sole resident Glass Core engine.
fn format_resident_readiness(readiness: &crate::ResidentReadiness) -> String {
    let core = &readiness.glass_core;
    let engine = core
        .binary
        .version
        .as_deref()
        .map(|version| format!("glass-core {version}"))
        .unwrap_or_else(|| "engine missing".into());
    let state_line = format!(
        "{} · {} · runtime {} · provider {}",
        if readiness.ready {
            "✓ Ready"
        } else {
            "○ Needs setup"
        },
        engine,
        core_component_label(&core.protocol),
        core_component_label(&core.authentication),
    );
    if readiness.ready {
        let provider = core.provider.as_deref().unwrap_or("not selected");
        format!("{state_line}\nprovider {provider} · Glass Core tools and permissions")
    } else {
        let remediation = readiness
            .remediation
            .first()
            .map(|item| format!(" · next: {item}"))
            .unwrap_or_default();
        format!("{state_line}{remediation}\n\nInstall `glass-core` or set `GLASS_CORE_BINARY`.")
    }
}

fn core_component_label(component: &crate::CoreReadinessComponent) -> String {
    match component.state {
        crate::CoreReadinessState::Ready => "✓".into(),
        crate::CoreReadinessState::Missing => "× missing".into(),
        crate::CoreReadinessState::Incompatible => "! incompatible".into(),
        crate::CoreReadinessState::Unauthenticated => "! no provider".into(),
        crate::CoreReadinessState::Unknown => "? unknown".into(),
    }
}

pub(super) fn safe_browser_url(url: &str) -> Option<String> {
    let url = url.trim();
    if url.is_empty() {
        return None;
    }
    let sanitized = url
        .split(['?', '#'])
        .next()
        .unwrap_or(url)
        .chars()
        .take(2_048)
        .collect::<String>();
    (!sanitized.is_empty()).then_some(sanitized)
}

fn native_browser_dialog_resolution_status(dialog_type: &str, accepted: bool) -> String {
    if dialog_type == "beforeunload" {
        if accepted {
            "Page navigation confirmed · page resuming".into()
        } else {
            "Navigation dismissed · staying on page".into()
        }
    } else {
        format!("JavaScript {dialog_type} resolved · page resuming")
    }
}

fn page_origin(url: &str) -> Option<String> {
    let url = safe_browser_url(url)?;
    let scheme_end = url.find("://")?;
    let authority_start = scheme_end + 3;
    let authority_end = url[authority_start..]
        .find('/')
        .map(|offset| authority_start + offset)
        .unwrap_or(url.len());
    Some(url[..authority_end].to_string())
}
fn action_command_prefix(command: &str) -> String {
    command
        .split_whitespace()
        .take_while(|token| {
            !token
                .chars()
                .all(|character| character.is_ascii_uppercase() || character == '_')
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn fuzzy_contains(candidate: &str, query: &str) -> bool {
    if query.is_empty() {
        return true;
    }
    let mut query = query
        .chars()
        .map(|character| character.to_ascii_lowercase());
    let mut expected = query.next();
    for character in candidate
        .chars()
        .map(|character| character.to_ascii_lowercase())
    {
        if Some(character) == expected {
            expected = query.next();
            if expected.is_none() {
                return true;
            }
        }
    }
    false
}

fn browser_recovery_arguments(port: u16, attach: bool) -> serde_json::Value {
    serde_json::json!({
        "port": port,
        "attach": attach,
        "incognito": !attach,
        "modalDialogs": !attach,
    })
}

fn text_char_index_to_byte(text: &str, character_index: usize) -> usize {
    text.char_indices()
        .nth(character_index)
        .map_or(text.len(), |(byte_index, _)| byte_index)
}

/// Bind an ephemeral localhost port to discover a free one.
fn free_local_port() -> Option<u16> {
    std::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))
        .ok()
        .and_then(|listener| listener.local_addr().ok())
        .map(|address| address.port())
}

struct LspLocation {
    path: String,
    line: u32,
    column: u32,
}

fn parse_completion_insert(value: &serde_json::Value) -> Option<String> {
    let items = value
        .get("items")
        .and_then(serde_json::Value::as_array)
        .or_else(|| value.as_array())?;
    let item = items.first()?;
    item.pointer("/textEdit/newText")
        .or_else(|| item.pointer("/textEdit/insert/newText"))
        .or_else(|| item.get("insertText"))
        .or_else(|| item.get("label"))
        .and_then(serde_json::Value::as_str)
        .map(str::to_string)
        .filter(|text| !text.is_empty())
}

fn format_lsp_locations(value: &serde_json::Value) -> String {
    let items = value
        .as_array()
        .map(|items| items.as_slice())
        .unwrap_or_else(|| std::slice::from_ref(value));
    let mut lines = vec!["REFERENCES".to_string()];
    for item in items.iter().take(32) {
        if let Some(location) = parse_lsp_location(item) {
            lines.push(format!(
                "  {}:{}:{}",
                location.path, location.line, location.column
            ));
        }
    }
    if lines.len() == 1 {
        lines.push("  none".into());
    }
    lines.join("\n")
}

fn parse_lsp_location(value: &serde_json::Value) -> Option<LspLocation> {
    let target = value
        .as_array()
        .and_then(|items| items.first())
        .unwrap_or(value);
    let uri = target
        .get("uri")
        .or_else(|| target.get("targetUri"))
        .and_then(serde_json::Value::as_str)?;
    let range = target
        .get("range")
        .or_else(|| target.get("targetRange"))
        .or_else(|| target.get("targetSelectionRange"))?;
    let line = range
        .pointer("/start/line")
        .and_then(serde_json::Value::as_u64)? as u32
        + 1;
    let column = range
        .pointer("/start/character")
        .and_then(serde_json::Value::as_u64)? as u32
        + 1;
    let path = uri
        .rsplit("://")
        .next()
        .unwrap_or(uri)
        .rsplit('/')
        .take(3)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect::<Vec<_>>()
        .join("/");
    Some(LspLocation {
        path: path.trim_start_matches('/').to_string(),
        line,
        column,
    })
}

fn parse_lsp_symbols(value: &serde_json::Value) -> Vec<(String, u32)> {
    let mut symbols = Vec::new();
    let items = value
        .get("result")
        .or(Some(value))
        .and_then(serde_json::Value::as_array)
        .cloned()
        .unwrap_or_default();
    for item in items {
        let name = item
            .get("name")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("symbol")
            .to_string();
        let line = item
            .pointer("/range/start/line")
            .or_else(|| item.pointer("/location/range/start/line"))
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(0) as u32
            + 1;
        symbols.push((name, line));
    }
    symbols
}

fn workflow_slug(name: &str) -> String {
    let mut slug = String::new();
    for character in name.chars() {
        if character.is_ascii_alphanumeric() {
            slug.push(character.to_ascii_lowercase());
        } else if matches!(character, '-' | '_' | ' ' | '.')
            && !slug.is_empty()
            && !slug.ends_with('-')
        {
            slug.push('-');
        }
    }
    let slug = slug.trim_matches('-').to_string();
    if slug.is_empty() {
        "click-path".into()
    } else {
        slug
    }
}

fn slug_input_name(name: &str) -> String {
    workflow_slug(name).replace('-', "_")
}

fn selection_covers(
    content: &str,
    selection: &crate::development::TextSelection,
    cursor: crate::development::TextPosition,
) -> bool {
    let Some(offset) = crate::development::editor::text_position_offset(content, cursor) else {
        return false;
    };
    let Some((start, end)) = crate::development::editor::selection_offsets(content, selection)
    else {
        return false;
    };
    offset >= start && offset <= end
}

fn editor_offset(content: &str, line: u32, column: u32) -> usize {
    let mut offset = 0;
    for (index, value) in content.split_inclusive('\n').enumerate() {
        if index + 1 == line as usize {
            let column_bytes = value
                .trim_end_matches('\n')
                .char_indices()
                .nth(column.saturating_sub(1) as usize)
                .map(|(offset, _)| offset)
                .unwrap_or_else(|| value.trim_end_matches('\n').len());
            return offset + column_bytes;
        }
        offset += value.len();
    }
    content.len()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lsp_text_edits_use_utf16_positions_and_descending_application() {
        let original = "a😀b\nlast\n";
        let edits = vec![
            serde_json::json!({
                "range": {"start": {"line": 1, "character": 0}, "end": {"line": 1, "character": 4}},
                "newText": "done"
            }),
            serde_json::json!({
                "range": {"start": {"line": 0, "character": 3}, "end": {"line": 0, "character": 4}},
                "newText": "B"
            }),
        ];
        assert_eq!(
            apply_lsp_text_edits(original, &edits).unwrap(),
            "a😀B\ndone\n"
        );
    }

    #[test]
    fn lsp_text_edits_reject_surrogate_splits_and_overlaps() {
        let split_surrogate = vec![serde_json::json!({
            "range": {"start": {"line": 0, "character": 2}, "end": {"line": 0, "character": 3}},
            "newText": "x"
        })];
        assert!(
            apply_lsp_text_edits("a😀b", &split_surrogate)
                .unwrap_err()
                .contains("surrogate")
        );

        let overlap = vec![
            serde_json::json!({
                "range": {"start": {"line": 0, "character": 1}, "end": {"line": 0, "character": 4}},
                "newText": "x"
            }),
            serde_json::json!({
                "range": {"start": {"line": 0, "character": 3}, "end": {"line": 0, "character": 5}},
                "newText": "y"
            }),
        ];
        assert!(
            apply_lsp_text_edits("abcdef", &overlap)
                .unwrap_err()
                .contains("overlapping")
        );
    }

    #[test]
    fn lsp_formatting_creates_a_preview_that_requires_explicit_acceptance() {
        let root = std::env::temp_dir().join(format!("glass-lsp-preview-{}", std::process::id()));
        std::fs::create_dir_all(root.join("src")).expect("create source fixture");
        std::fs::write(root.join("src/main.rs"), "let x=1;\n").expect("write source fixture");
        let mut state =
            DevTuiState::open_for_tui(&root, TuiLayout::Desktop).expect("open fixture workspace");
        state.surface = DevSurface::Code;
        state.files.push("src/main.rs".into());
        state.selected_file = state.files.len() - 1;
        state
            .ws_mut()
            .unwrap()
            .project_mut()
            .open_buffer("src/main.rs", crate::development::Actor::local())
            .unwrap();
        state.refresh_editor_projection();
        state
            .remember_lsp_format_baseline("src/main.rs")
            .expect("capture exact request document");

        let result = serde_json::json!({
            "method": "textDocument/formatting",
            "result": [{
                "range": {"start": {"line": 0, "character": 0}, "end": {"line": 0, "character": 8}},
                "newText": "let x = 1;"
            }]
        });
        let shared_workspace = state.workspace.clone();
        let workspace_guard = shared_workspace
            .try_lock()
            .expect("hold workspace while formatting result arrives");
        state.handle_lsp_preview_result("glass.lsp.formatting", &result);
        assert_eq!(state.pending_lsp_preview_results.len(), 1);
        assert_eq!(state.pending_lsp_format_baselines.len(), 1);
        drop(workspace_guard);
        assert!(state.flush_pending_editor_input());
        assert!(state.status.contains("LSP formatting preview"));
        assert_eq!(
            state
                .ws()
                .unwrap()
                .project()
                .buffer("src/main.rs")
                .unwrap()
                .content,
            "let x=1;\n",
            "preview must leave the buffer unchanged"
        );
        assert_eq!(state.editor_engine.hunks.len(), 1);
        let proposal_id = state
            .editor_proposals
            .iter()
            .find(|proposal| proposal.state == crate::development::EditorProposalState::Pending)
            .expect("pending formatting proposal")
            .id
            .clone();
        state
            .accept_review_proposal(Some(&proposal_id))
            .expect("accept the preview explicitly");
        assert_eq!(
            state
                .ws()
                .unwrap()
                .project()
                .buffer("src/main.rs")
                .unwrap()
                .content,
            "let x = 1;\n"
        );

        state
            .remember_lsp_format_baseline("src/main.rs")
            .expect("capture range-format request buffer");
        let range_result = serde_json::json!({
            "method": "textDocument/rangeFormatting",
            "result": [{
                "range": {"start": {"line": 0, "character": 4}, "end": {"line": 0, "character": 5}},
                "newText": "y"
            }]
        });
        assert!(state.preview_lsp_formatting(&range_result).unwrap());
        assert_eq!(
            state
                .ws()
                .unwrap()
                .project()
                .buffer("src/main.rs")
                .unwrap()
                .content,
            "let x = 1;\n",
            "range formatting must leave the buffer unchanged until accepted"
        );
        let range_proposal_id = state
            .editor_proposals
            .iter()
            .find(|proposal| proposal.state == crate::development::EditorProposalState::Pending)
            .expect("pending range-format proposal")
            .id
            .clone();
        state
            .accept_review_proposal(Some(&range_proposal_id))
            .expect("accept the range-format preview explicitly");
        assert_eq!(
            state
                .ws()
                .unwrap()
                .project()
                .buffer("src/main.rs")
                .unwrap()
                .content,
            "let y = 1;\n"
        );
        drop(state);
        std::fs::remove_dir_all(root).expect("remove fixture workspace");
    }

    #[test]
    fn lsp_workspace_rename_previews_each_file_before_explicit_acceptance() {
        let root =
            std::env::temp_dir().join(format!("glass-lsp-workspace-edit-{}", std::process::id()));
        std::fs::create_dir_all(root.join("src")).expect("create source fixture");
        std::fs::write(root.join("src/first.rs"), "fn old_name() {}\n")
            .expect("write first source fixture");
        std::fs::write(root.join("src/second.rs"), "old_name();\n")
            .expect("write second source fixture");
        let first_uri = url::Url::from_file_path(root.join("src/first.rs"))
            .expect("make first file URI")
            .to_string();
        let second_uri = url::Url::from_file_path(root.join("src/second.rs"))
            .expect("make second file URI")
            .to_string();
        let mut state =
            DevTuiState::open_for_tui(&root, TuiLayout::Desktop).expect("open fixture workspace");
        state.surface = DevSurface::Code;
        for path in ["src/first.rs", "src/second.rs"] {
            state
                .ws_mut()
                .unwrap()
                .project_mut()
                .open_buffer(path, crate::development::Actor::local())
                .unwrap();
        }
        state.refresh_editor_projection();

        let result = serde_json::json!({
            "method": "textDocument/rename",
            "result": {
                "changes": {
                    (first_uri): [{
                        "range": {"start":{"line":0,"character":3},"end":{"line":0,"character":11}},
                        "newText": "new_name"
                    }],
                    (second_uri): [{
                        "range": {"start":{"line":0,"character":0},"end":{"line":0,"character":8}},
                        "newText": "new_name"
                    }]
                }
            }
        });
        let shared_workspace = state.workspace.clone();
        let workspace_guard = shared_workspace
            .try_lock()
            .expect("hold workspace for fixture");
        state.handle_lsp_preview_result("glass.lsp.rename", &result);
        assert_eq!(state.pending_lsp_preview_results.len(), 1);
        assert!(state.status.contains("waiting for the workspace"));
        drop(workspace_guard);
        assert!(state.flush_pending_editor_input());
        assert!(state.status.contains("2 files"));
        assert_eq!(state.editor_engine.hunks.len(), 1);
        assert_eq!(
            state
                .ws()
                .unwrap()
                .project()
                .buffer("src/first.rs")
                .unwrap()
                .content,
            "fn old_name() {}\n",
            "workspace edit preview must leave the first buffer unchanged"
        );
        assert_eq!(
            state
                .ws()
                .unwrap()
                .project()
                .buffer("src/second.rs")
                .unwrap()
                .content,
            "old_name();\n",
            "workspace edit preview must leave the second buffer unchanged"
        );
        let proposal_ids = state
            .editor_proposals
            .iter()
            .filter(|proposal| proposal.state == crate::development::EditorProposalState::Pending)
            .map(|proposal| proposal.id.clone())
            .collect::<Vec<_>>();
        assert_eq!(proposal_ids.len(), 2);
        for (index, id) in proposal_ids.iter().enumerate() {
            if index == 0 {
                let shared_workspace = state.workspace.clone();
                let workspace_guard = shared_workspace
                    .try_lock()
                    .expect("hold workspace while accepting fixture proposal");
                let queued = state
                    .accept_review_proposal(Some(id))
                    .expect("busy acceptance is retained for retry");
                assert!(queued.contains("Queued acceptance"));
                assert!(!state.flush_pending_editor_input());
                assert_eq!(state.pending_editor_acceptances.len(), 1);
                drop(workspace_guard);
                assert!(state.flush_pending_editor_input());
            } else {
                state
                    .accept_review_proposal(Some(id))
                    .expect("accept each reviewed file proposal explicitly");
            }
        }
        assert_eq!(
            state
                .ws()
                .unwrap()
                .project()
                .buffer("src/first.rs")
                .unwrap()
                .content,
            "fn new_name() {}\n"
        );
        assert_eq!(
            state
                .ws()
                .unwrap()
                .project()
                .buffer("src/second.rs")
                .unwrap()
                .content,
            "new_name();\n"
        );
        drop(state);
        std::fs::remove_dir_all(root).expect("remove temporary workspace");
    }

    #[test]
    fn lsp_workspace_edit_rejects_outside_project_before_proposing_changes() {
        let root = std::env::temp_dir().join(format!(
            "glass-lsp-workspace-boundary-{}",
            std::process::id()
        ));
        let outside = std::env::temp_dir().join(format!(
            "glass-lsp-workspace-outside-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&root).expect("create project fixture");
        std::fs::write(&outside, "private\n").expect("write outside fixture");
        let outside_uri = url::Url::from_file_path(&outside)
            .expect("make outside file URI")
            .to_string();
        let mut state =
            DevTuiState::open_for_tui(&root, TuiLayout::Desktop).expect("open fixture workspace");
        let result = serde_json::json!({
            "result": {"changes": {(outside_uri): [{
                "range": {"start":{"line":0,"character":0},"end":{"line":0,"character":7}},
                "newText": "changed"
            }]}}
        });
        let error = state.preview_lsp_workspace_edit(&result).unwrap_err();
        assert!(error.contains("outside this project"));
        assert!(state.editor_proposals.is_empty());
        assert_eq!(std::fs::read_to_string(&outside).unwrap(), "private\n");
        drop(state);
        std::fs::remove_file(outside).expect("remove outside fixture");
        std::fs::remove_dir_all(root).expect("remove project fixture");
    }

    #[cfg(unix)]
    #[test]
    fn lsp_rename_transport_previews_and_accepts_multifile_workspace_edits() {
        let python = std::env::split_paths(&std::env::var_os("PATH").unwrap())
            .map(|directory| directory.join("python3"))
            .find(|candidate| candidate.is_file());
        let Some(python) = python else {
            return;
        };
        let root =
            std::env::temp_dir().join(format!("glass-lsp-rename-transport-{}", std::process::id()));
        std::fs::create_dir_all(root.join("src")).expect("create LSP workspace fixture");
        std::fs::write(root.join("src/first.rs"), "fn old_name() {}\n")
            .expect("write first source fixture");
        std::fs::write(root.join("src/second.rs"), "old_name();\n")
            .expect("write second source fixture");
        let fixture =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/fake_lsp_workspace_edit.py");
        let mut state =
            DevTuiState::open_for_tui(&root, TuiLayout::Desktop).expect("open LSP workspace");
        state.surface = DevSurface::Code;
        state
            .ws_mut()
            .unwrap()
            .language()
            .start(
                "fixture",
                &crate::lsp::LanguageServerConfig {
                    command: python.display().to_string(),
                    arguments: vec![fixture.display().to_string()],
                },
            )
            .expect("start local fake LSP transport");
        for (path, unsaved) in [
            ("src/first.rs", "// unsaved first\nfn old_name() {}\n"),
            ("src/second.rs", "// unsaved second\nold_name();\n"),
        ] {
            state
                .ws_mut()
                .unwrap()
                .project_mut()
                .open_buffer(path, crate::development::Actor::local())
                .unwrap();
            state
                .ws_mut()
                .unwrap()
                .project_mut()
                .edit_buffer(path, unsaved.into(), crate::development::Actor::local())
                .unwrap();
        }
        let open_documents = state
            .ws()
            .unwrap()
            .project()
            .buffers()
            .map(|buffer| (buffer.path.clone(), buffer.content.clone()))
            .collect::<Vec<_>>();
        for (path, content) in &open_documents {
            state
                .ws_mut()
                .unwrap()
                .language()
                .sync_document_content("fixture", path, content)
                .expect("sync unsaved editor buffer to LSP");
        }
        let first_content = open_documents
            .iter()
            .find(|(path, _)| path == "src/first.rs")
            .unwrap()
            .1
            .clone();
        let response = state
            .ws_mut()
            .unwrap()
            .language()
            .rename_with_content(
                "fixture",
                "human",
                "src/first.rs",
                crate::development::DiagnosticPosition {
                    line: 2,
                    character: 4,
                },
                "new_name",
                &first_content,
            )
            .expect("read workspace rename edit over LSP stdio");
        let response = serde_json::to_value(response).expect("serialize LSP response");
        assert!(state.preview_lsp_workspace_edit(&response).unwrap());
        assert!(state.status.contains("2 files"));
        assert_eq!(
            state
                .ws()
                .unwrap()
                .project()
                .buffer_or_file_content("src/first.rs")
                .unwrap(),
            "// unsaved first\nfn old_name() {}\n",
            "receiving a workspace edit must not mutate the unsaved source"
        );
        let pending = state
            .editor_proposals
            .iter()
            .filter(|proposal| proposal.state == crate::development::EditorProposalState::Pending)
            .map(|proposal| proposal.id.clone())
            .collect::<Vec<_>>();
        assert_eq!(pending.len(), 2);
        for proposal in pending {
            state
                .accept_review_proposal(Some(&proposal))
                .expect("accept the reviewed file proposal");
        }
        for path in ["src/first.rs", "src/second.rs"] {
            assert!(
                state
                    .ws()
                    .unwrap()
                    .project()
                    .buffer_or_file_content(path)
                    .unwrap()
                    .contains("new_name")
            );
        }
        state.ws_mut().unwrap().language().stop("fixture").unwrap();
        drop(state);
        std::fs::remove_dir_all(root).expect("remove LSP workspace fixture");
    }

    fn native_gc_ui_snapshot(
        owner: crate::AgentId,
        sequence: u64,
        content: &str,
    ) -> super::super::snapshot::DisplaySnapshot {
        let mut snapshot = super::super::snapshot::DisplaySnapshot::default();
        let marker = super::super::projection::ConversationEntry {
            kind: super::super::projection::ConversationKind::System,
            text: content.into(),
            streaming: false,
            entry_id: Some(format!("core-ui:{sequence}")),
            client_message_id: None,
            tool_name: Some("glass-core.native-ui/help/Core".into()),
            tool_card: None,
            core_ui: None,
        };
        let assistant = super::super::projection::ConversationEntry {
            kind: super::super::projection::ConversationKind::Assistant,
            text: "A normal assistant reply".into(),
            streaming: false,
            entry_id: Some("assistant-1".into()),
            client_message_id: None,
            tool_name: None,
            tool_card: None,
            core_ui: None,
        };
        let items = vec![assistant, marker.clone()];
        snapshot.agent_conversations.insert(
            owner,
            super::super::snapshot::AgentConversation {
                text: "A normal assistant reply".into(),
                items,
                ..Default::default()
            },
        );
        snapshot
    }

    #[test]
    fn native_gc_help_panel_is_owner_scoped_and_never_reopens_after_close() {
        let (mut state, root) = routed_state("native-gc-help-panel");
        let owner = crate::AgentId::parse("agent-0001").unwrap();
        state.selected_agent = Some(owner.clone());
        state.composer_mode = true;
        state.composer_input = "keep this draft".into();
        state.composer_cursor = 7;
        state.apply_snapshot(&super::super::snapshot::DisplaySnapshot::default());
        state.open_native_gc_ui_pending(owner.clone(), "help");
        let snapshot = native_gc_ui_snapshot(
            owner.clone(),
            7,
            "/status  Show status [native GC UI]\n/help  Show commands [native GC UI]",
        );
        state.apply_snapshot(&snapshot);

        let panel = state.native_gc_ui_panel.as_ref().expect("help panel open");
        assert_eq!(panel.owner, owner);
        assert_eq!(panel.scope, "Core");
        assert!(!panel.pending);
        assert!(panel.content.contains("/status"));
        assert_eq!(state.composer_input, "keep this draft");
        assert_eq!(state.composer_cursor, 7);
        assert_eq!(state.conversation_items.len(), 1);
        assert!(!state.conversation_view().contains("Show commands"));
        assert!(
            state
                .conversation_view()
                .contains("A normal assistant reply")
        );

        state.native_gc_ui_search_start();
        state.native_gc_ui_search_char('s');
        state.native_gc_ui_search_char('t');
        assert!(
            state
                .native_gc_ui_panel
                .as_ref()
                .unwrap()
                .visible_lines()
                .iter()
                .all(|line| line.to_lowercase().contains("st"))
        );
        state.close_native_gc_ui_panel();
        state.apply_snapshot(&snapshot);
        assert!(state.native_gc_ui_panel.is_none());
        assert_eq!(state.conversation_items.len(), 1);
        assert_eq!(state.composer_input, "keep this draft");
        assert_eq!(state.composer_cursor, 7);
        drop(state);
        std::fs::remove_dir_all(root).expect("remove temporary workspace");
    }

    #[test]
    fn ordinary_core_command_result_fills_its_panel_and_stays_out_of_chat() {
        let (mut state, root) = routed_state("native-gc-ordinary-command");
        let owner = crate::AgentId::parse("agent-0001").unwrap();
        state.selected_agent = Some(owner.clone());
        state.apply_snapshot(&super::super::snapshot::DisplaySnapshot::default());
        state.open_native_gc_ui_pending(owner.clone(), "status");
        let mut snapshot = super::super::snapshot::DisplaySnapshot::default();
        let entry = super::super::projection::ConversationEntry {
            kind: super::super::projection::ConversationKind::System,
            text: "Core session is ready".into(),
            streaming: false,
            entry_id: Some("core-command:3".into()),
            client_message_id: None,
            tool_name: Some("coreCommand/status".into()),
            tool_card: None,
            core_ui: Some(super::super::projection::CoreUiPresentation {
                version: 1,
                surface: "panel".into(),
                kind: "status".into(),
                title: "Glass Core status".into(),
                command: "status".into(),
                content: "Core session is ready".into(),
                sequence: 3,
                error: false,
            }),
        };
        snapshot.agent_conversations.insert(
            owner,
            super::super::snapshot::AgentConversation {
                text: "SYSTEM\nCore session is ready".into(),
                items: vec![entry],
                ..Default::default()
            },
        );
        state.apply_snapshot(&snapshot);
        let panel = state
            .native_gc_ui_panel
            .as_ref()
            .expect("command panel remains open");
        assert!(!panel.pending);
        assert_eq!(panel.title, "Glass Core status");
        assert_eq!(panel.content, "Core session is ready");
        assert!(!state.conversation_view().contains("Core session is ready"));
        drop(state);
        std::fs::remove_dir_all(root).expect("remove temporary workspace");
    }

    #[test]
    fn failed_core_command_keeps_error_in_its_owner_scoped_panel() {
        let (mut state, root) = routed_state("native-gc-command-error");
        let owner = crate::AgentId::parse("agent-0001").unwrap();
        state.selected_agent = Some(owner.clone());
        state.apply_snapshot(&super::super::snapshot::DisplaySnapshot::default());
        state.open_native_gc_ui_pending(owner.clone(), "help");
        let mut snapshot = super::super::snapshot::DisplaySnapshot::default();
        let entry = super::super::projection::ConversationEntry {
            kind: super::super::projection::ConversationKind::Error,
            text: "provider unavailable".into(),
            streaming: false,
            entry_id: Some("core-ui-error:9".into()),
            client_message_id: None,
            tool_name: Some("coreCommand".into()),
            tool_card: None,
            core_ui: Some(super::super::projection::CoreUiPresentation {
                version: 1,
                surface: "panel".into(),
                kind: "error".into(),
                title: "Glass Core /help failed".into(),
                command: "help".into(),
                content: "provider unavailable".into(),
                sequence: 9,
                error: true,
            }),
        };
        snapshot.agent_conversations.insert(
            owner,
            super::super::snapshot::AgentConversation {
                text: "ERROR\nprovider unavailable".into(),
                items: vec![entry],
                ..Default::default()
            },
        );
        state.apply_snapshot(&snapshot);
        let panel = state
            .native_gc_ui_panel
            .as_ref()
            .expect("error stays in panel");
        assert!(!panel.pending);
        assert_eq!(panel.kind, "error");
        assert_eq!(panel.content, "provider unavailable");
        assert!(!state.conversation_view().contains("provider unavailable"));
        drop(state);
        std::fs::remove_dir_all(root).expect("remove temporary workspace");
    }

    #[test]
    fn unsupported_core_palette_command_has_a_truthful_panel_and_runs_nothing() {
        let (mut state, root) = routed_state("native-gc-unsupported-command");
        let owner = crate::AgentId::parse("agent-0001").unwrap();
        state.selected_agent = Some(owner.clone());
        state.open_native_gc_ui_notice(
            owner,
            "history",
            "Not available in this Glass workspace",
            "/history · Review compaction receipts\n\nNo action was run.",
            "unsupported",
        );
        let panel = state
            .native_gc_ui_panel
            .as_ref()
            .expect("unsupported command panel");
        assert_eq!(panel.command, "history");
        assert_eq!(panel.kind, "unsupported");
        assert!(!panel.pending);
        assert!(panel.content.contains("No action was run"));
        assert!(!state.conversation_view().contains("No action was run"));
        drop(state);
        std::fs::remove_dir_all(root).expect("remove temporary workspace");
    }

    #[test]
    fn slash_palette_starts_focused_and_searches_the_full_core_catalogue() {
        let (mut state, root) = routed_state("slash-focused-core");
        state.gc_command_items = vec![
            serde_json::json!({"name":"status","description":"Current session","runtimeMode":"runtime"}),
            serde_json::json!({"name":"model","description":"Choose model","runtimeMode":"interactive"}),
            serde_json::json!({"name":"reconcile","description":"Resolve interrupted tool calls","runtimeMode":"interactive"}),
        ];
        let initial = state.gc_palette_core_commands();
        assert_eq!(
            initial
                .iter()
                .filter_map(|item| item["name"].as_str())
                .collect::<Vec<_>>(),
            ["status", "model"]
        );
        state.gc_command_query = "reconcile".into();
        let searched = state.gc_palette_core_commands();
        assert_eq!(searched.len(), 1);
        assert_eq!(searched[0]["name"], "reconcile");
        std::fs::remove_dir_all(root).expect("remove test workspace");
    }

    #[test]
    fn slash_palette_exposes_core_settings_without_leaving_glass() {
        let (state, root) = routed_state("slash-core-settings");
        let actions = state.gc_command_glass_actions();
        let settings = actions
            .iter()
            .find(|action| action.command == "agent settings")
            .expect("in-Glass Core settings action");
        assert!(settings.label.contains("provider and model"));
        assert!(settings.description.contains("selected Core session"));
        std::fs::remove_dir_all(root).expect("remove test workspace");
    }

    #[test]
    fn core_settings_opens_in_glass_and_model_selection_uses_core_configure() {
        let (mut state, root) = routed_state("core-settings-route");
        let agent = crate::AgentId::parse("agent-0001").unwrap();
        state.selected_agent = Some(agent);
        let mut worker = super::super::snapshot::SnapshotWorker::spawn(&state);

        state.composer_input = "/glass agent settings".into();
        state.composer_glass_action_completion = Some("agent settings".into());
        state.submit_composer(&mut worker);
        assert!(state.gc_settings_view);
        assert!(state.gc_model_menu_open);
        assert!(state.gc_command_menu_open);
        assert!(!state.gc_settings_jobs.is_empty());

        state.gc_model_items = vec![serde_json::json!({
            "provider":"openai", "id":"gpt-next", "label":"Next"
        })];
        state.gc_command_loading = false;
        state.submit_gc_command_selection(&mut worker);
        assert!(!state.gc_settings_view);
        assert!(!state.gc_model_menu_open);
        assert!(state.status.contains("openai/gpt-next"));
        drop(worker);
        std::fs::remove_dir_all(root).expect("remove test workspace");
    }

    #[test]
    fn typed_core_settings_picker_uses_the_existing_in_glass_settings_panel() {
        let (mut state, root) = routed_state("typed-core-settings-picker");
        let agent = crate::AgentId::parse("agent-0001").unwrap();
        state.selected_agent = Some(agent);
        state.gc_command_items = vec![serde_json::json!({
            "name":"settings",
            "description":"Effective settings",
            "runtimeMode":"interactive",
            "presentationKind":"picker"
        })];
        state.gc_command_query = "settings".into();
        state.gc_command_menu_open = true;
        let mut worker = super::super::snapshot::SnapshotWorker::spawn(&state);

        state.submit_gc_command_selection(&mut worker);

        assert!(state.gc_settings_view);
        assert!(state.gc_model_menu_open);
        assert!(state.gc_command_menu_open);
        assert!(state.gc_command_loading);
        assert!(!state.gc_settings_jobs.is_empty());
        assert!(state.native_gc_ui_panel.is_none());
        drop(worker);
        std::fs::remove_dir_all(root).expect("remove test workspace");
    }

    #[test]
    fn model_picker_uses_core_catalogue_and_escape_returns_to_palette() {
        let (mut state, root) = routed_state("slash-model-picker");
        let agent = crate::AgentId::parse("agent-0001").unwrap();
        state.selected_agent = Some(agent.clone());
        state.gc_model_menu_open = true;
        state.gc_command_menu_open = true;
        state.gc_command_loading = true;
        let mut catalogues = std::collections::BTreeMap::new();
        catalogues.insert(
            agent,
            super::super::snapshot::CoreModelCatalogue {
                provider: Some("openai".into()),
                current_model: Some("gpt-current".into()),
                models: vec![
                    serde_json::json!({"id":"gpt-current","label":"Current"}),
                    serde_json::json!({"id":"gpt-next","label":"Next"}),
                ],
                error: None,
            },
        );
        let snapshot = super::super::snapshot::DisplaySnapshot {
            core_model_catalogues: catalogues,
            ..Default::default()
        };
        state.update_gc_model_catalogue(&snapshot);
        assert_eq!(state.gc_model_provider.as_deref(), Some("openai"));
        assert_eq!(state.gc_model_current.as_deref(), Some("gpt-current"));
        assert_eq!(state.gc_model_items.len(), 2);
        assert!(!state.gc_command_loading);
        state.move_gc_command_selection(1);
        assert_eq!(state.gc_command_selected, 1);
        state.cancel_gc_command_menu();
        assert!(!state.gc_model_menu_open);
        assert!(state.gc_command_menu_open);
        std::fs::remove_dir_all(root).expect("remove test workspace");
    }

    #[test]
    fn task_assigned_threads_show_task_identity_and_other_threads_keep_role() {
        let first = crate::AgentId::parse("agent-0001").unwrap();
        let second = crate::AgentId::parse("agent-0002").unwrap();
        let manual = crate::AgentId::parse("agent-0003").unwrap();
        let tasks = [
            TaskThreadLabel {
                assigned_agent: Some(first.clone()),
                id: "task-alpha".into(),
                title: "Review parser".into(),
            },
            TaskThreadLabel {
                assigned_agent: Some(second.clone()),
                id: "task-beta".into(),
                title: "Update renderer".into(),
            },
            TaskThreadLabel {
                assigned_agent: None,
                id: "task-unassigned".into(),
                title: "Unassigned work".into(),
            },
        ];

        assert_eq!(
            project_thread_picker_title("developer", &first, &tasks),
            "developer · task-alpha · Review parser"
        );
        assert_eq!(
            project_thread_picker_title("developer", &second, &tasks),
            "developer · task-beta · Update renderer"
        );
        assert_eq!(
            project_thread_picker_title("developer", &manual, &tasks),
            "developer"
        );
    }

    #[cfg(unix)]
    #[test]
    fn attention_inbox_is_stable_deduplicated_and_selection_preserving() {
        let (mut state, root) = routed_state("attention-order");
        let agent_id = crate::AgentId::parse("agent-0001").unwrap();
        let proposal = crate::development::EditorProposal {
            id: "proposal-1".into(),
            path: "README.md".into(),
            summary: "Update documentation".into(),
            actor: crate::development::Actor::local(),
            base_hash: String::new(),
            base_revision: 0,
            original: String::new(),
            proposed: String::new(),
            state: crate::development::EditorProposalState::Pending,
            created_revision: 1,
            updated_revision: 1,
        };
        let mut second_proposal = proposal.clone();
        second_proposal.id = "proposal-2".into();
        second_proposal.summary = "Second proposal".into();
        state.editor_proposals = vec![proposal.clone(), proposal, second_proposal];
        state.follow_up_queue_snapshots.insert(
            agent_id.clone(),
            FollowUpQueueSnapshot {
                thread_id: "thread-0001".into(),
                current_run_id: "run-0001".into(),
                send_attempts: vec![
                    crate::thread_store::SendAttemptRecord {
                        attempt_id: "attempt-1".into(),
                        client_message_id: "message-1".into(),
                        target_run_id: "run-old".into(),
                        mode: crate::thread_store::SendAttemptMode::Prompt,
                        text: "Inspect the failed run and summarize the blocker".into(),
                        status: crate::thread_store::SendAttemptStatus::Uncertain,
                        engine_request_id: None,
                        created_at_ms: 1,
                    },
                    crate::thread_store::SendAttemptRecord {
                        attempt_id: "attempt-2".into(),
                        client_message_id: "message-2".into(),
                        target_run_id: "run-old".into(),
                        mode: crate::thread_store::SendAttemptMode::Prompt,
                        text: "Try the same task again".into(),
                        status: crate::thread_store::SendAttemptStatus::Failed,
                        engine_request_id: Some("request-2".into()),
                        created_at_ms: 2,
                    },
                    crate::thread_store::SendAttemptRecord {
                        attempt_id: "attempt-3".into(),
                        client_message_id: "message-3".into(),
                        target_run_id: "run-old".into(),
                        mode: crate::thread_store::SendAttemptMode::Prompt,
                        text: "Local dispatch failed before Core accepted it".into(),
                        status: crate::thread_store::SendAttemptStatus::Failed,
                        engine_request_id: None,
                        created_at_ms: 3,
                    },
                ],
                entries: vec![crate::thread_store::FollowUpQueueRecord {
                    entry_id: "entry-1".into(),
                    target_run_id: "run-old".into(),
                    mode: crate::thread_store::FollowUpQueueMode::Steer,
                    text: "Continue review".into(),
                    state: crate::thread_store::FollowUpQueueState::NeedsReview,
                }],
            },
        );
        state.refresh_attention_items();
        assert_eq!(state.attention_items.len(), 5);
        assert!(state.attention_items.iter().any(|item| {
            item.key
                == (AttentionItemKey::SendAttempt {
                    thread_id: "thread-0001".into(),
                    attempt_id: "attempt-1".into(),
                })
                && item.title == "Message delivery uncertain"
        }));
        assert!(state.attention_items.iter().any(|item| {
            item.key
                == (AttentionItemKey::SendAttempt {
                    thread_id: "thread-0001".into(),
                    attempt_id: "attempt-2".into(),
                })
                && item.title == "Core turn failed"
        }));
        assert!(
            state
                .attention_items
                .windows(2)
                .all(|pair| pair[0].key < pair[1].key)
        );
        let selected = state.attention_items[1].key.clone();
        state.attention_selection = 1;
        state.refresh_attention_items();
        assert_eq!(
            state.attention_items[state.attention_selection].key,
            selected
        );
        state.selected_editor_proposal_id = Some("proposal-2".into());
        assert_eq!(
            state.selected_editor_proposal("README.md").unwrap().id,
            "proposal-2"
        );
        state
            .follow_up_queue_snapshots
            .get_mut(&agent_id)
            .unwrap()
            .entries[0]
            .state = crate::thread_store::FollowUpQueueState::Accepted;
        state.refresh_attention_items();
        assert_eq!(state.attention_items.len(), 4);
        assert!(
            !state
                .attention_items
                .iter()
                .any(|item| matches!(item.key, AttentionItemKey::FollowUp { .. }))
        );
        std::fs::write(root.join("README.md"), "source\n").unwrap();
        let (workspace_proposals, proposal_id) = {
            let mut workspace = state.ws_mut().expect("lock test workspace");
            workspace
                .project_mut()
                .open_buffer("README.md", crate::development::Actor::local())
                .unwrap();
            workspace
                .project_mut()
                .propose_editor_change(
                    "README.md",
                    "source\n".into(),
                    "first\n".into(),
                    "First proposal".into(),
                    crate::development::Actor::local(),
                )
                .unwrap();
            let second = workspace
                .project_mut()
                .propose_editor_change(
                    "README.md",
                    "source\n".into(),
                    "second\n".into(),
                    "Second proposal".into(),
                    crate::development::Actor::local(),
                )
                .unwrap();
            (workspace.project().editor_proposals(), second.id)
        };
        state.editor_proposals = workspace_proposals;
        state.refresh_attention_items();
        state.files = vec!["README.md".into()];
        state.attention_inbox_open = true;
        state.attention_selection = state
            .attention_items
            .iter()
            .position(|item| {
                item.key
                    == (AttentionItemKey::EditorProposal {
                        proposal_id: proposal_id.clone(),
                        path: "README.md".into(),
                    })
            })
            .unwrap();
        state.activate_attention_selection();
        assert_eq!(
            state.selected_editor_proposal_id.as_deref(),
            Some(proposal_id.as_str())
        );
        assert!(
            state
                .status
                .contains(&format!("Review proposal {proposal_id}"))
        );
        state
            .editor_engine
            .set_hunks(vec![super::super::editor::EditorHunk {
                start_line: 1,
                end_line: 1,
                kind: super::super::editor::HunkKind::Change,
            }]);
        state
            .editor_proposals
            .retain(|proposal| proposal.id != proposal_id);
        state.reconcile_selected_editor_proposal();
        assert!(state.selected_editor_proposal_id.is_none());
        assert!(state.editor_engine.hunks.is_empty());
        assert!(state.editor_engine.agent_caret.is_none());
        state.close_attention_inbox();
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn pasted_palette_and_composer_text_report_byte_limit_truncation() {
        let (mut state, root) = routed_state("paste-limits");
        let palette_paste = "é".repeat(PALETTE_INPUT_LIMIT / 2 + 1);
        state.insert_palette_text(&palette_paste);
        assert_eq!(state.command_input.len(), PALETTE_INPUT_LIMIT);
        assert_eq!(state.command_cursor, PALETTE_INPUT_LIMIT);
        assert_eq!(state.status, "Palette input limit reached (8 KiB)");

        state.composer_input.clear();
        state.composer_cursor = 0;
        let composer_paste = "λ".repeat(COMPOSER_INPUT_LIMIT / 2 + 1);
        state.insert_composer_text(&composer_paste);
        assert_eq!(state.composer_input.len(), COMPOSER_INPUT_LIMIT);
        assert_eq!(state.composer_cursor, COMPOSER_INPUT_LIMIT);
        assert_eq!(state.status, "Composer input limit reached (16 KiB)");
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn help_scroll_is_clamped_to_renderable_content() {
        let (mut state, root) = routed_state("help-scroll-bound");
        state.terminal_height = 24;
        let visible = state.terminal_height.saturating_sub(4) as usize;
        let max_scroll = super::super::bindings::curriculum_help(state.surface)
            .lines()
            .count()
            .saturating_sub(visible) as u16;
        state.scroll_help(i32::MAX);
        assert_eq!(state.help_scroll, max_scroll);
        state.scroll_help(i32::MIN);
        assert_eq!(state.help_scroll, 0);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn external_editor_resolution_keeps_dirty_projection_and_exposes_bounded_diff() {
        let (mut state, root) = routed_state("editor-external-resolution");
        std::fs::create_dir_all(root.join("src")).unwrap();
        std::fs::write(root.join("src/main.rs"), "fn base() {}\n").unwrap();
        state
            .ws_mut()
            .unwrap()
            .project_mut()
            .open_buffer("src/main.rs", crate::development::Actor::local())
            .unwrap();
        state.editor_buffer_index = 0;
        state.refresh_editor_projection();
        state
            .ws_mut()
            .unwrap()
            .project_mut()
            .edit_buffer(
                "src/main.rs",
                "fn local() {}\n".into(),
                crate::development::Actor::local(),
            )
            .unwrap();
        std::fs::write(root.join("src/main.rs"), "fn outside() {}\n").unwrap();

        assert!(state.tick_editor_reconcile());
        assert_eq!(state.focused_editor_content, "fn local() {}\n");
        let comparison = state.resolve_editor_external("compare").unwrap();
        assert!(comparison.contains("opened"));
        assert!(state.git_diff.contains("-fn local() {}"));
        assert!(state.git_diff.contains("+fn outside() {}"));
        assert_eq!(state.surface, DevSurface::Git);
        assert_eq!(state.focused_editor_content, "fn local() {}\n");
        state.surface = DevSurface::Code;
        state.resolve_editor_external("reload").unwrap();
        assert_eq!(state.focused_editor_content, "fn outside() {}\n");
        assert_eq!(
            state.focused_buffer().unwrap().actor.kind,
            crate::development::ActorKind::Unknown
        );
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn external_compare_diff_is_bounded_and_marks_truncation() {
        let local = (0..20)
            .map(|line| format!("local {line}\n"))
            .collect::<String>();
        let external = (0..20)
            .map(|line| format!("disk {line}\n"))
            .collect::<String>();
        let diff = bounded_line_diff(&local, &external);
        assert!(diff.contains("-local 0"));
        assert!(diff.contains("+disk 0"));
        assert!(diff.contains("diff truncated"));
        assert!(!diff.contains("-local 9"));
    }

    #[test]
    fn external_compare_aligns_insertions_before_bounding_changed_lines() {
        let local = (0..12)
            .map(|line| format!("line {line}\n"))
            .collect::<String>();
        let mut external_lines = (0..12)
            .map(|line| format!("line {line}"))
            .collect::<Vec<_>>();
        external_lines.insert(0, "inserted header".into());
        external_lines[11] = "changed line 10".into();
        let external = format!("{}\n", external_lines.join("\n"));

        let diff = bounded_line_diff(&local, &external);

        assert!(diff.contains("+inserted header"));
        assert!(diff.contains("-line 10"));
        assert!(diff.contains("+changed line 10"));
        assert!(!diff.contains("diff truncated"));

        let many_changes = bounded_line_diff(
            &(0..20)
                .map(|line| format!("old {line}\n"))
                .collect::<String>(),
            &format!(
                "inserted\n{}",
                (0..20)
                    .map(|line| format!("new {line}\n"))
                    .collect::<String>()
            ),
        );
        assert!(many_changes.contains("+inserted"));
        assert!(many_changes.contains("diff truncated"));
    }

    #[test]
    fn scroll_end_uses_wrapped_agent_conversation_height() {
        let (mut state, root) = routed_state("agent-scroll-end-wrap");
        state.surface = DevSurface::Agent;
        state.terminal_width = 40;
        state.terminal_height = 18;
        state.agent_conversation = format!(
            "YOU\n{}\n\nGLASS AGENT\n{}",
            "user message ".repeat(12),
            "agent response ".repeat(16)
        );
        let raw_height = state.conversation_view().lines().count();
        let expected = super::super::render::agent_conversation_scroll_end(&state);

        state.scroll_end();

        assert_eq!(state.current_scroll() as usize, expected);
        assert!(expected > raw_height.saturating_sub(1));
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn empty_snapshot_refresh_preserves_selection_identity_across_reordering() {
        let (mut state, root) = routed_state("empty-refresh-selection");
        let process = |name: &str| ProcessRow {
            name: name.into(),
            command: "dev".into(),
            pid: None,
            health: crate::development::ProcessHealth::Healthy,
            url: None,
        };
        let git = |path: &str| crate::git::GitStatusEntry {
            path: path.into(),
            original_path: None,
            index_status: ' ',
            worktree_status: 'M',
            untracked: false,
        };
        let debug = |name: &str, pid| DebugSessionRow {
            name: name.into(),
            state: crate::debugger::DebugSessionState::Stopped,
            pid,
            breakpoints: 0,
            watches: 0,
        };
        state.process_entries = vec![process("api"), process("web")];
        state.selected_process = 1;
        state.git_entries = vec![git("a.rs"), git("b.rs")];
        state.selected_git_file = 1;
        state.debug_sessions = vec![debug("alpha", 1), debug("beta", 2)];
        state.selected_debug_session = 1;

        state.apply_snapshot(&super::super::snapshot::DisplaySnapshot::default());
        let refreshed = super::super::snapshot::DisplaySnapshot {
            process_entries: vec![process("web"), process("api")],
            git_entries: vec![git("b.rs"), git("a.rs")],
            debug_sessions: vec![debug("beta", 2), debug("alpha", 1)],
            ..Default::default()
        };
        state.apply_snapshot(&refreshed);

        assert_eq!(state.selected_process_entry().unwrap().name, "web");
        assert_eq!(state.selected_git_entry().unwrap().path, "b.rs");
        assert_eq!(state.selected_debug_session().unwrap().name, "beta");
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn pending_file_open_flushes_after_workspace_recovers_off_code_surface() {
        let (mut state, root) = routed_state("pending-open-off-code");
        std::fs::create_dir_all(root.join("src")).unwrap();
        std::fs::write(root.join("src/main.rs"), "fn main() {}\n").unwrap();
        state.files = vec!["src/main.rs".into()];
        state.pending_open_file = Some("src/main.rs".into());
        state.surface = DevSurface::App;

        state.flush_pending_open_file();

        assert!(state.pending_open_file.is_none());
        assert_eq!(state.focused_editor_path, "src/main.rs");
        assert_eq!(state.surface, DevSurface::App);
        assert!(
            state
                .ws()
                .unwrap()
                .project()
                .buffer("src/main.rs")
                .is_some()
        );
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn browser_selection_intent_updates_projection_through_shared_gateway() {
        let (mut state, root) = routed_state("browser-selection-gateway");
        state.browser_workspace.replace_entities(
            7,
            vec![
                BrowserWorkspaceEntity {
                    reference: "r7:b1".into(),
                    role: "button".into(),
                    name: "One".into(),
                    actionable: true,
                    revision: 7,
                },
                BrowserWorkspaceEntity {
                    reference: "r7:b2".into(),
                    role: "button".into(),
                    name: "Two".into(),
                    actionable: true,
                    revision: 7,
                },
            ],
        );
        state.queue_browser_intent(BrowserWorkspaceIntent::MoveSelection { delta: 1 });
        assert_eq!(state.browser_workspace.state().selected_entity, Some(1));
        assert!(state.browser.contains("Two"));
        assert!(state.status.contains("Selection moved"));
        let _ = std::fs::remove_dir_all(root);
    }

    fn routed_state(label: &str) -> (DevTuiState, std::path::PathBuf) {
        let suffix = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system clock")
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "glass-tui-routing-{label}-{}-{suffix}",
            std::process::id()
        ));
        std::fs::create_dir_all(&root).expect("create temporary workspace");
        let state =
            DevTuiState::open_for_tui(&root, TuiLayout::Desktop).expect("open temporary workspace");
        (state, root)
    }

    #[cfg(unix)]
    #[test]
    fn background_draft_writer_coalesces_without_losing_other_threads() {
        let root = std::env::temp_dir().join(format!(
            "glass-tui-draft-writer-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let store = crate::thread_store::ThreadStore::open(&root).unwrap();
        let make_thread = |role: &str| crate::thread_store::ThreadRecord {
            thread_id: crate::thread_store::random_id("thread").unwrap(),
            title: role.into(),
            role: role.into(),
            created_at_ms: 1,
            composer_draft: None,
            follow_up_queue: None,
            send_attempts: Vec::new(),
            #[cfg(windows)]
            send_attempts_opaque: None,
            runs: vec![crate::thread_store::RunRecord {
                run_id: crate::thread_store::random_id("run").unwrap(),
                engine: "glass-core".into(),
                core_session_id: None,
                classification: crate::thread_store::RunClassification::Starting,
            }],
        };
        let thread_a = make_thread("A");
        let thread_b = make_thread("B");
        let (id_a, id_b) = (thread_a.thread_id.clone(), thread_b.thread_id.clone());
        store.create(thread_a).unwrap();
        store.create(thread_b).unwrap();
        let writer = ComposerDraftWriter::spawn();
        writer
            .save(
                store.clone(),
                id_a.clone(),
                ComposerDraft {
                    text: "old A".into(),
                    cursor: 5,
                },
            )
            .unwrap();
        writer
            .save(
                store.clone(),
                id_b.clone(),
                ComposerDraft {
                    text: "draft B".into(),
                    cursor: 3,
                },
            )
            .unwrap();
        writer
            .save(
                store.clone(),
                id_a.clone(),
                ComposerDraft {
                    text: "latest A".into(),
                    cursor: 6,
                },
            )
            .unwrap();
        writer.flush().unwrap();
        assert_eq!(
            store.composer_draft(&id_a).unwrap().unwrap(),
            crate::thread_store::ComposerDraftRecord {
                text: "latest A".into(),
                cursor: 6,
            }
        );
        assert_eq!(
            store.composer_draft(&id_b).unwrap().unwrap(),
            crate::thread_store::ComposerDraftRecord {
                text: "draft B".into(),
                cursor: 3,
            }
        );
        drop(writer);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn timed_out_flush_discards_after_in_flight_write_completes() {
        let root = std::env::temp_dir().join(format!(
            "glass-tui-draft-discard-timeout-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let store = crate::thread_store::ThreadStore::open(&root).unwrap();
        let thread_id = crate::thread_store::random_id("thread").unwrap();
        store
            .create(crate::thread_store::ThreadRecord {
                thread_id: thread_id.clone(),
                title: "discard target".into(),
                role: "assistant".into(),
                created_at_ms: 1,
                runs: vec![crate::thread_store::RunRecord {
                    run_id: crate::thread_store::random_id("run").unwrap(),
                    engine: "glass-core".into(),
                    core_session_id: None,
                    classification: crate::thread_store::RunClassification::Starting,
                }],
                composer_draft: None,
                follow_up_queue: None,
                send_attempts: Vec::new(),
                #[cfg(windows)]
                send_attempts_opaque: None,
            })
            .unwrap();
        store
            .set_composer_draft(
                &thread_id,
                crate::thread_store::ComposerDraftRecord {
                    text: "previously committed".into(),
                    cursor: 10,
                },
            )
            .unwrap();

        let writer = ComposerDraftWriter::spawn();
        writer
            .block_for_test(std::time::Duration::from_millis(2_100))
            .unwrap();
        writer
            .save(
                store.clone(),
                thread_id.clone(),
                ComposerDraft {
                    text: "write still in flight".into(),
                    cursor: 5,
                },
            )
            .unwrap();
        assert!(writer.flush().is_err(), "the bounded flush must time out");

        writer
            .discard_thread(store.clone(), thread_id.clone())
            .unwrap();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        let mut discarded = false;
        while std::time::Instant::now() < deadline {
            if let Some(ComposerDraftWriterEvent::DiscardComplete(result)) = writer.try_event() {
                result.unwrap();
                discarded = true;
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert!(discarded, "discard should be acknowledged after the write");
        assert_eq!(store.composer_draft(&thread_id).unwrap(), None);
        drop(writer);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn quit_discard_snapshot_clears_save_that_succeeds_after_flush_timeout() {
        let root = std::env::temp_dir().join(format!(
            "glass-tui-draft-quit-success-timeout-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let store = crate::thread_store::ThreadStore::open(&root).unwrap();
        let thread_id = crate::thread_store::random_id("thread").unwrap();
        store
            .create(crate::thread_store::ThreadRecord {
                thread_id: thread_id.clone(),
                title: "quit timeout owner".into(),
                role: "assistant".into(),
                created_at_ms: 1,
                runs: vec![crate::thread_store::RunRecord {
                    run_id: crate::thread_store::random_id("run").unwrap(),
                    engine: "glass-core".into(),
                    core_session_id: None,
                    classification: crate::thread_store::RunClassification::Completed,
                }],
                composer_draft: None,
                follow_up_queue: None,
                send_attempts: Vec::new(),
                #[cfg(windows)]
                send_attempts_opaque: None,
            })
            .unwrap();
        let mut state = DevTuiState::open_for_tui(&root, TuiLayout::Desktop).unwrap();
        let agent = crate::AgentId::parse("agent-0001").unwrap();
        assert!(state.select_agent(Some(agent.clone())));
        for _ in 0..100 {
            state.poll_composer_draft_io();
            if !state.pending_composer_draft_loads.contains_key(&agent) {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        let writer = state
            .composer_draft_writer
            .as_ref()
            .expect("agent selection starts the draft worker");
        writer
            .block_for_test(std::time::Duration::from_millis(2_200))
            .unwrap();
        state.composer_input = "saved after timeout".into();
        state.composer_cursor = state.composer_input.len();
        assert!(state.persist_composer_draft());

        state.request_quit();
        assert!(state.quit_confirmation);
        assert!(!state.quit);
        assert!(state.composer_draft_discard_targets.contains_key(&agent));
        let save_deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
        while std::time::Instant::now() < save_deadline {
            state.poll_composer_draft_io();
            if !state.composer_draft_save_failed
                && store
                    .composer_draft(&thread_id)
                    .unwrap()
                    .is_some_and(|draft| draft.text == "saved after timeout")
            {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert!(!state.composer_draft_save_failed);
        assert!(
            state.composer_draft_discard_targets.contains_key(&agent),
            "a later success must not erase the explicit discard snapshot"
        );
        assert_eq!(
            store.composer_draft(&thread_id).unwrap().unwrap().text,
            "saved after timeout"
        );

        state.confirm_quit();
        assert!(
            !state.quit,
            "quit waits for the durable clear acknowledgement"
        );
        for _ in 0..100 {
            state.poll_composer_draft_io();
            if state.quit {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert!(state.quit);
        assert_eq!(store.composer_draft(&thread_id).unwrap(), None);
        drop(state);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn quit_discard_snapshot_survives_stale_success_for_newer_save_generation() {
        let root = std::env::temp_dir().join(format!(
            "glass-tui-draft-generation-discard-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let store = crate::thread_store::ThreadStore::open(&root).unwrap();
        let thread_id = crate::thread_store::random_id("thread").unwrap();
        store
            .create(crate::thread_store::ThreadRecord {
                thread_id: thread_id.clone(),
                title: "generation race owner".into(),
                role: "assistant".into(),
                created_at_ms: 1,
                runs: vec![crate::thread_store::RunRecord {
                    run_id: crate::thread_store::random_id("run").unwrap(),
                    engine: "glass-core".into(),
                    core_session_id: None,
                    classification: crate::thread_store::RunClassification::Completed,
                }],
                composer_draft: None,
                follow_up_queue: None,
                send_attempts: Vec::new(),
                #[cfg(windows)]
                send_attempts_opaque: None,
            })
            .unwrap();

        let mut state = DevTuiState::open_for_tui(&root, TuiLayout::Desktop).unwrap();
        let agent = crate::AgentId::parse("agent-0001").unwrap();
        assert!(state.select_agent(Some(agent.clone())));
        for _ in 0..100 {
            state.poll_composer_draft_io();
            if !state.pending_composer_draft_loads.contains_key(&agent) {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }

        state.composer_input = "save generation one".into();
        state.composer_cursor = state.composer_input.len();
        assert!(state.persist_composer_draft());
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        while std::time::Instant::now() < deadline {
            if store
                .composer_draft(&thread_id)
                .unwrap()
                .is_some_and(|draft| draft.text == "save generation one")
            {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert_eq!(
            store.composer_draft(&thread_id).unwrap().unwrap().text,
            "save generation one"
        );
        let first_generation = state.composer_draft_edit_generations[&agent];

        // SaveSucceeded for generation one remains queued in the event channel.
        let writer = state.composer_draft_writer.as_ref().unwrap();
        writer
            .block_for_test(std::time::Duration::from_millis(2_200))
            .unwrap();
        state.composer_input = "save generation two".into();
        state.composer_cursor = state.composer_input.len();
        assert!(state.persist_composer_draft());
        let second_generation = state.composer_draft_edit_generations[&agent];
        assert!(second_generation > first_generation);

        state.request_quit();
        assert!(state.quit_confirmation);
        assert!(!state.quit);
        assert_eq!(
            state.composer_draft_discard_targets.get(&agent),
            Some(&second_generation)
        );

        // The worker completes generation two before Enter is handled. Polling
        // both success events must not let generation one's stale event clear
        // the captured owner/generation discard intent.
        let save_deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
        while std::time::Instant::now() < save_deadline {
            if store
                .composer_draft(&thread_id)
                .unwrap()
                .is_some_and(|draft| draft.text == "save generation two")
            {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        state.poll_composer_draft_io();
        assert_eq!(
            state.composer_draft_discard_targets.get(&agent),
            Some(&second_generation)
        );
        assert_eq!(
            store.composer_draft(&thread_id).unwrap().unwrap().text,
            "save generation two"
        );

        state.confirm_quit();
        assert!(
            !state.quit,
            "quit waits for the durable clear acknowledgement"
        );
        for _ in 0..100 {
            state.poll_composer_draft_io();
            if state.quit {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert!(state.quit);
        assert_eq!(store.composer_draft(&thread_id).unwrap(), None);
        drop(state);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn quit_failure_snapshot_keeps_newest_unsaved_generation_over_stale_failure() {
        let (mut state, root) = routed_state("composer-draft-quit-generation-snapshot");
        let agent = crate::AgentId::parse("agent-0001").unwrap();
        let draft = ComposerDraft {
            text: "newer queued edit".into(),
            cursor: 6,
        };
        state.selected_agent = Some(agent.clone());
        state.composer_input = draft.text.clone();
        state.composer_cursor = draft.cursor;
        state.persisted_composer_draft = Some((agent.clone(), draft));
        state
            .composer_draft_edit_generations
            .insert(agent.clone(), 2);
        state.composer_draft_unsaved_agents.insert(agent.clone(), 2);
        state.composer_draft_failed_agents.insert(agent.clone(), 1);
        state.composer_draft_save_failed = true;
        state.composer_draft_failure = Some("older write failed".into());

        state.request_quit();

        assert!(state.quit_confirmation);
        assert!(!state.quit);
        assert_eq!(state.composer_draft_discard_targets.get(&agent), Some(&2));
        drop(state);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn discard_clears_failed_background_owner_and_preserves_successful_other_thread() {
        let root = std::env::temp_dir().join(format!(
            "glass-tui-draft-discard-owner-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let store = crate::thread_store::ThreadStore::open(&root).unwrap();
        let make_thread = |title: &str| crate::thread_store::ThreadRecord {
            thread_id: crate::thread_store::random_id("thread").unwrap(),
            title: title.into(),
            role: title.into(),
            created_at_ms: 1,
            runs: vec![crate::thread_store::RunRecord {
                run_id: crate::thread_store::random_id("run").unwrap(),
                engine: "glass-core".into(),
                core_session_id: None,
                classification: crate::thread_store::RunClassification::Completed,
            }],
            composer_draft: None,
            follow_up_queue: None,
            send_attempts: Vec::new(),
            #[cfg(windows)]
            send_attempts_opaque: None,
        };
        let agent_thread = make_thread("agent A");
        let other_thread = make_thread("agent B");
        store.create(agent_thread.clone()).unwrap();
        store.create(other_thread.clone()).unwrap();
        for index in 0..3 {
            let filler = make_thread(&format!("filler {index}"));
            store.create(filler.clone()).unwrap();
            store
                .set_composer_draft(
                    &filler.thread_id,
                    crate::thread_store::ComposerDraftRecord {
                        text: "f".repeat(16 * 1024),
                        cursor: 0,
                    },
                )
                .unwrap();
        }
        let other_text = "b".repeat(6_000);
        store
            .set_composer_draft(
                &other_thread.thread_id,
                crate::thread_store::ComposerDraftRecord {
                    text: other_text.clone(),
                    cursor: 300,
                },
            )
            .unwrap();

        let mut state = DevTuiState::open_for_tui(&root, TuiLayout::Desktop).unwrap();
        let agent_a = crate::AgentId::parse("agent-0001").unwrap();
        let agent_b = crate::AgentId::parse("agent-0002").unwrap();
        assert!(state.select_agent(Some(agent_a.clone())));
        state.composer_input = "a".repeat(11_000);
        state.composer_cursor = state.composer_input.len();
        assert!(state.persist_composer_draft());
        assert!(state.select_agent(Some(agent_b.clone())));

        for _ in 0..100 {
            state.poll_composer_draft_io();
            if state.composer_draft_failed_agents.contains_key(&agent_a) {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert_eq!(state.selected_agent, Some(agent_b.clone()));
        assert!(state.composer_draft_failed_agents.contains_key(&agent_a));
        assert!(!state.composer_draft_failed_agents.contains_key(&agent_b));

        state.request_quit();
        assert!(state.quit_confirmation);
        assert!(!state.quit);
        state.confirm_quit();
        assert!(!state.quit, "quit waits for worker clear acknowledgement");
        for _ in 0..100 {
            state.poll_composer_draft_io();
            if state.quit {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert!(state.quit);
        assert_eq!(store.composer_draft(&agent_thread.thread_id).unwrap(), None);
        assert_eq!(
            store
                .composer_draft(&other_thread.thread_id)
                .unwrap()
                .unwrap()
                .text,
            other_text
        );
        drop(state);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn project_thread_picker_lists_status_and_selects_without_losing_draft() {
        let root = std::env::temp_dir().join(format!(
            "glass-tui-thread-picker-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let store = crate::thread_store::ThreadStore::open(&root).unwrap();
        for (index, title) in ["First thread", "Second thread", "Third thread"]
            .into_iter()
            .enumerate()
        {
            store
                .create(crate::thread_store::ThreadRecord {
                    thread_id: format!("thread-{:032x}", index + 1),
                    title: title.into(),
                    role: title.into(),
                    created_at_ms: index as u128 + 1,
                    runs: vec![crate::thread_store::RunRecord {
                        run_id: format!("run-{:032x}", index + 1),
                        engine: "glass-core".into(),
                        core_session_id: Some(format!("session-{index}")),
                        classification: crate::thread_store::RunClassification::RecoveryPending,
                    }],
                    composer_draft: None,
                    follow_up_queue: None,
                    send_attempts: Vec::new(),
                    #[cfg(windows)]
                    send_attempts_opaque: None,
                })
                .unwrap();
        }

        let mut state = DevTuiState::open_for_tui(&root, TuiLayout::Desktop).unwrap();
        let first = crate::AgentId::parse("agent-0001").unwrap();
        let second = crate::AgentId::parse("agent-0002").unwrap();
        let third = crate::AgentId::parse("agent-0003").unwrap();
        for (agent, transcript) in [
            (first.clone(), "first transcript"),
            (second.clone(), "second transcript"),
            (third.clone(), "third transcript"),
        ] {
            state.agent_conversations.insert(
                agent,
                super::super::snapshot::AgentConversation {
                    text: transcript.into(),
                    items: Vec::new(),
                    reasoning: String::new(),
                    ..Default::default()
                },
            );
        }
        state.selected_agent = Some(first.clone());
        state.agent_conversation = "first transcript".into();
        state.composer_input = "draft on first".into();
        state.composer_cursor = 5;
        state.thread_drafts.insert(
            Some(second.clone()),
            ComposerDraft {
                text: "draft on second".into(),
                cursor: 10,
            },
        );

        state.refresh();
        state.open_thread_picker();
        assert!(state.thread_picker_open);
        assert_eq!(state.thread_picker_items.len(), 3);
        assert_eq!(state.thread_picker_items[0].title, "First thread");
        let expected_status = if cfg!(windows) {
            "unavailable"
        } else {
            "recovery pending"
        };
        assert_eq!(state.thread_picker_items[0].status, expected_status);
        assert_eq!(state.selected_agent, Some(first.clone()));
        state.move_thread_picker_selection(1);
        state.activate_thread_picker_selection();

        assert!(!state.thread_picker_open);
        assert_eq!(state.selected_agent, Some(second.clone()));
        assert_eq!(state.composer_input, "draft on second");
        assert_eq!(state.composer_cursor, 10);

        state.thread_drafts.insert(
            Some(third.clone()),
            ComposerDraft {
                text: "draft on third".into(),
                cursor: 8,
            },
        );
        state.open_thread_picker();
        state.move_thread_picker_selection(1);
        state.activate_thread_picker_selection();
        assert_eq!(state.selected_agent, Some(third.clone()));
        assert_eq!(state.composer_input, "draft on third");
        assert_eq!(state.composer_cursor, 8);

        state.composer_mode = true;
        state.surface = DevSurface::Agent;
        state.status = "Third thread stays focused".into();
        state.pending_chat_messages.push(PendingChatMessage {
            agent_id: Some(first.clone()),
            message_id: "picker-late-first".into(),
            text: "completed in first".into(),
            state: ChatMessageState::Sending,
            job_id: Some(81),
            error: None,
        });
        state.apply_tool_job_result(super::super::snapshot::ToolJobResult {
            id: 81,
            tool: "glass.agent.send".into(),
            result: Ok(serde_json::json!({"agentId": first.as_str()})),
        });
        assert_eq!(state.selected_agent, Some(third.clone()));
        assert_eq!(state.composer_input, "draft on third");
        assert_eq!(state.composer_cursor, 8);
        assert_eq!(state.status, "Third thread stays focused");
        assert!(state.composer_mode);

        state.open_thread_picker();
        state.move_thread_picker_selection(-2);
        state.activate_thread_picker_selection();
        assert_eq!(state.selected_agent, Some(first));
        assert_eq!(state.composer_input, "draft on first");
        assert_eq!(state.composer_cursor, 5);
        state.status = "Return to this conversation".into();
        state.open_thread_picker();
        state.close_thread_picker();
        assert_eq!(state.status, "Return to this conversation");
        assert_eq!(state.composer_input, "draft on first");
        drop(state);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn composer_draft_reopens_by_thread_id_after_agent_id_changes() {
        let root = std::env::temp_dir().join(format!(
            "glass-tui-durable-draft-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let store = crate::thread_store::ThreadStore::open(&root).unwrap();
        let thread_id = crate::thread_store::random_id("thread").unwrap();
        let thread = |thread_id: String, role: &str| crate::thread_store::ThreadRecord {
            thread_id,
            title: role.into(),
            role: role.into(),
            created_at_ms: 1,
            composer_draft: None,
            follow_up_queue: None,
            send_attempts: Vec::new(),
            #[cfg(windows)]
            send_attempts_opaque: None,
            runs: vec![crate::thread_store::RunRecord {
                run_id: crate::thread_store::random_id("run").unwrap(),
                engine: "glass-core".into(),
                core_session_id: None,
                classification: crate::thread_store::RunClassification::Starting,
            }],
        };
        store.create(thread(thread_id.clone(), "original")).unwrap();

        let mut first = DevTuiState::open_for_tui(&root, TuiLayout::Desktop).unwrap();
        let old_agent_id = crate::AgentId::parse("agent-0001").unwrap();
        first.select_agent(Some(old_agent_id));
        first.composer_input = "resume this λ".into();
        first.composer_cursor = "resume ".len();
        assert!(first.persist_composer_draft());
        assert!(first.flush_composer_drafts());
        drop(first);

        store
            .create(thread(
                crate::thread_store::random_id("thread").unwrap(),
                "earlier row",
            ))
            .unwrap();
        let registry_path = root.join(".glass/threads/registry.json");
        let mut registry: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&registry_path).unwrap()).unwrap();
        registry["threads"].as_array_mut().unwrap().swap(0, 1);
        std::fs::write(
            &registry_path,
            serde_json::to_vec_pretty(&registry).unwrap(),
        )
        .unwrap();

        let mut reopened = DevTuiState::open_for_tui(&root, TuiLayout::Desktop).unwrap();
        let new_agent_id = crate::AgentId::parse("agent-0002").unwrap();
        reopened.select_agent(Some(new_agent_id));
        for _ in 0..100 {
            if reopened.poll_composer_draft_io() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert_eq!(reopened.composer_input, "resume this λ");
        assert_eq!(reopened.composer_cursor, "resume ".len());
        reopened.open_composer();
        assert_eq!(reopened.composer_cursor, "resume ".len());
        drop(reopened);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn failed_composer_draft_enqueue_keeps_outgoing_agent_selected() {
        let (mut state, root) = routed_state("composer-draft-save-failure-switch");
        let agent_a = crate::AgentId::parse("agent-0001").unwrap();
        let agent_b = crate::AgentId::parse("agent-0002").unwrap();
        state.selected_agent = Some(agent_a.clone());
        state.composer_input = "unsaved outgoing draft".into();
        state.composer_cursor = 8;
        state.composer_draft_writer = Some(ComposerDraftWriter::disconnected_for_test());

        assert!(!state.select_agent(Some(agent_b)));
        assert_eq!(state.selected_agent, Some(agent_a.clone()));
        assert_eq!(state.composer_input, "unsaved outgoing draft");
        assert_eq!(state.composer_cursor, 8);
        assert_eq!(
            state.thread_drafts.get(&Some(agent_a)),
            Some(&ComposerDraft {
                text: "unsaved outgoing draft".into(),
                cursor: 8,
            })
        );
        assert!(state.status.contains("Could not queue composer draft"));
        drop(state);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn failed_composer_draft_enqueue_requires_explicit_quit_discard() {
        let (mut state, root) = routed_state("composer-draft-save-failure-quit");
        let agent = crate::AgentId::parse("agent-0001").unwrap();
        state.selected_agent = Some(agent.clone());
        state.composer_input = "unsaved composer text".into();
        state.composer_cursor = 8;
        state.composer_draft_writer = Some(ComposerDraftWriter::disconnected_for_test());

        state.request_quit();
        assert!(state.quit_confirmation);
        assert!(!state.quit);
        assert!(state.status.contains("Could not save composer draft"));
        assert!(
            state
                .status
                .contains("Enter discards the unsaved draft and quits")
        );

        state.cancel_quit();
        assert!(!state.quit);
        assert!(!state.quit_confirmation);
        assert!(state.status.contains("Composer draft remains unsaved"));
        assert_eq!(state.composer_input, "unsaved composer text");

        state.request_quit();
        assert!(state.quit_confirmation);
        state.confirm_quit();
        assert!(!state.quit);
        assert!(state.quit_confirmation);
        assert!(state.status.contains("Discarding composer draft"));
        for _ in 0..100 {
            state.poll_composer_draft_io();
            if state.status.contains("Could not clear composer draft") {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert!(!state.quit);
        assert!(state.quit_confirmation);
        assert!(state.status.contains("Could not clear composer draft"));
        drop(state);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn asynchronous_composer_draft_failure_blocks_switch_and_requires_discard() {
        let (mut state, root) = routed_state("composer-draft-async-save-failure");
        let agent_a = crate::AgentId::parse("agent-0001").unwrap();
        let agent_b = crate::AgentId::parse("agent-0002").unwrap();
        state.selected_agent = Some(agent_a.clone());
        state.composer_input = "draft with failed backing store".into();
        state.composer_cursor = 6;
        assert!(state.persist_composer_draft());
        std::thread::sleep(std::time::Duration::from_millis(250));
        assert!(state.poll_composer_draft_io());
        assert!(state.composer_draft_save_failed);

        assert!(!state.select_agent(Some(agent_b)));
        assert_eq!(state.selected_agent, Some(agent_a));
        state.request_quit();
        assert!(state.quit_confirmation);
        assert!(!state.quit);
        assert!(
            state
                .status
                .contains("Enter discards the unsaved draft and quits")
        );
        state.cancel_quit();
        assert!(state.status.contains("Composer draft remains unsaved"));

        let _ = state.flush_composer_drafts();
        drop(state);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn switching_agents_does_not_wait_for_composer_writer() {
        let (mut state, root) = routed_state("composer-draft-switch-responsive");
        let agent_a = crate::AgentId::parse("agent-0001").unwrap();
        let agent_b = crate::AgentId::parse("agent-0002").unwrap();
        state.selected_agent = Some(agent_a.clone());
        state.composer_input = "keep this draft".into();
        state.composer_cursor = 5;
        let writer = ComposerDraftWriter::spawn();
        writer
            .block_for_test(std::time::Duration::from_millis(250))
            .unwrap();
        state.composer_draft_writer = Some(writer);

        let started = std::time::Instant::now();
        assert!(state.select_agent(Some(agent_b.clone())));
        assert!(started.elapsed() < std::time::Duration::from_millis(100));
        assert_eq!(state.selected_agent, Some(agent_b));
        assert_eq!(
            state.thread_drafts.get(&Some(agent_a)),
            Some(&ComposerDraft {
                text: "keep this draft".into(),
                cursor: 5,
            })
        );

        std::thread::sleep(std::time::Duration::from_millis(300));
        state.poll_composer_draft_io();
        let _ = state.flush_composer_drafts();
        drop(state);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn pending_durable_load_survives_switch_away_and_back() {
        let (mut state, root) = routed_state("composer-draft-late-load-switch");
        let agent_a = crate::AgentId::parse("agent-0001").unwrap();
        let agent_b = crate::AgentId::parse("agent-0002").unwrap();
        let store = crate::thread_store::ThreadStore::open(&root).unwrap();
        let thread_id = crate::thread_store::random_id("thread").unwrap();
        store
            .create(crate::thread_store::ThreadRecord {
                thread_id: thread_id.clone(),
                title: "draft owner".into(),
                role: "assistant".into(),
                created_at_ms: 1,
                runs: vec![crate::thread_store::RunRecord {
                    run_id: crate::thread_store::random_id("run").unwrap(),
                    engine: "glass-core".into(),
                    core_session_id: None,
                    classification: crate::thread_store::RunClassification::Starting,
                }],
                composer_draft: None,
                follow_up_queue: None,
                send_attempts: Vec::new(),
                #[cfg(windows)]
                send_attempts_opaque: None,
            })
            .unwrap();
        store
            .set_composer_draft(
                &thread_id,
                crate::thread_store::ComposerDraftRecord {
                    text: "durable draft λ".into(),
                    cursor: "durable ".len(),
                },
            )
            .unwrap();
        let durable = store.composer_draft(&thread_id).unwrap().unwrap();
        let (load_a_tx, load_a_rx) = std::sync::mpsc::channel();
        let (_load_b_tx, load_b_rx) = std::sync::mpsc::channel();
        state.selected_agent = Some(agent_a.clone());
        state
            .pending_composer_draft_loads
            .insert(agent_a.clone(), (0, load_a_rx));
        state
            .pending_composer_draft_loads
            .insert(agent_b.clone(), (0, load_b_rx));

        assert!(state.select_agent(Some(agent_b)));
        assert!(state.select_agent(Some(agent_a.clone())));
        assert!(state.composer_input.is_empty());
        load_a_tx
            .send(Ok(Some(ComposerDraft {
                text: durable.text,
                cursor: durable.cursor,
            })))
            .unwrap();
        assert!(state.poll_composer_draft_io());
        assert_eq!(state.selected_agent, Some(agent_a));
        assert_eq!(state.composer_input, "durable draft λ");
        assert_eq!(state.composer_cursor, "durable ".len());

        drop(state);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn late_durable_load_does_not_replace_a_new_user_edit() {
        let (mut state, root) = routed_state("composer-draft-late-load-edit");
        let agent = crate::AgentId::parse("agent-0001").unwrap();
        let (load_tx, load_rx) = std::sync::mpsc::channel();
        state.selected_agent = Some(agent.clone());
        state
            .pending_composer_draft_loads
            .insert(agent.clone(), (0, load_rx));
        state.composer_input = "new user edit".into();
        state.composer_cursor = 4;
        assert!(state.persist_composer_draft());
        load_tx
            .send(Ok(Some(ComposerDraft {
                text: "old durable text".into(),
                cursor: 12,
            })))
            .unwrap();

        state.poll_composer_draft_io();
        assert_eq!(state.composer_input, "new user edit");
        assert_eq!(state.composer_cursor, 4);
        assert_eq!(
            state.thread_drafts.get(&Some(agent)),
            Some(&ComposerDraft {
                text: "new user edit".into(),
                cursor: 4,
            })
        );

        let _ = state.flush_composer_drafts();
        drop(state);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn tui_state_forwards_complete_browser_policy_to_resident_workspace() {
        let root =
            std::env::temp_dir().join(format!("glass-tui-browser-policy-{}", std::process::id()));
        std::fs::create_dir_all(&root).expect("create temporary workspace");
        let policy = BrowserPolicy::development(&root)
            .unwrap()
            .with_host_rules(
                ["allowed.example.test".to_string()],
                ["denied.example.test".to_string()],
            )
            .unwrap();

        let state =
            DevTuiState::open_for_tui_with_browser_policy(&root, TuiLayout::Desktop, policy)
                .expect("open TUI state with complete browser policy");
        let workspace = state.ws().expect("lock development workspace");
        let browser = workspace.browser();
        assert_eq!(
            browser
                .navigation_preflight("https://allowed.example.test/path")
                .decision,
            glass_browser::browser::policy::NavigationPreflightDecision::Allow
        );
        assert_eq!(
            browser
                .navigation_preflight("https://denied.example.test/path")
                .decision,
            glass_browser::browser::policy::NavigationPreflightDecision::Deny
        );

        drop(workspace);
        drop(state);
        std::fs::remove_dir_all(root).expect("remove temporary workspace");
    }

    #[test]
    fn debug_thread_and_frame_packets_parse_source_locations() {
        let threads = parse_debug_threads(&serde_json::json!({
            "threads": [{"id": 1, "name": "main"}, {"id": 2, "name": "worker"}]
        }));
        assert_eq!(threads.len(), 2);
        assert_eq!(threads[1].name, "worker");
        let frames = parse_debug_frames(&serde_json::json!({
            "stackFrames": [{
                "id": 12,
                "name": "checkout",
                "line": 40,
                "source": {"path": "src/main.rs"}
            }]
        }));
        assert_eq!(frames[0].path.as_deref(), Some("src/main.rs"));
        assert_eq!(frames[0].line, Some(40));
        let scopes = parse_debug_scopes(&serde_json::json!({
            "scopes": [{"name": "Locals", "variablesReference": 7}]
        }));
        assert_eq!(scopes[0].variables_reference, 7);
        let variables = parse_debug_variables(&serde_json::json!({
            "variables": [{"name": "x", "value": "1"}]
        }));
        assert_eq!(variables[0].name, "x");
    }

    #[test]
    fn quit_confirmation_requires_explicit_follow_through() {
        let root = std::env::temp_dir().join(format!("glass-tui-quit-{}", std::process::id()));
        std::fs::create_dir_all(&root).expect("create temporary workspace");
        let mut state =
            DevTuiState::open_for_tui(&root, TuiLayout::Desktop).expect("open temporary workspace");

        state.request_quit();
        assert!(state.quit_confirmation);
        assert!(!state.quit);
        assert_eq!(state.status, "Quit confirmation · Enter exits · Esc stays");

        state.cancel_quit();
        assert!(!state.quit_confirmation);
        assert!(!state.quit);

        state.request_quit();
        state.confirm_quit();
        assert!(!state.quit_confirmation);
        assert!(state.quit);
        assert_eq!(state.status, "Closing Glass Dev");

        std::fs::remove_dir_all(root).expect("remove temporary workspace");
    }

    #[test]
    fn composer_message_blocked_by_background_restores_exact_input() {
        let (mut state, root) = routed_state("composer-slash-background");
        state.surface = DevSurface::Agent;
        state.open_composer();
        state.running_tool_job = Some(42);
        let original = "Please retry when idle";
        let cursor = original.find("when").expect("cursor anchor");
        state.composer_input = original.into();
        state.composer_cursor = cursor;
        let mut worker = super::super::snapshot::SnapshotWorker::spawn(&state);

        state.submit_composer(&mut worker);

        assert!(state.pending_confirmation.is_none());
        assert!(state.composer_mode);
        assert_eq!(state.composer_input, original);
        assert_eq!(state.composer_cursor, cursor);
        assert!(state.status.contains("kept in composer"));

        drop(worker);
        std::fs::remove_dir_all(root).expect("remove temporary workspace");
    }

    #[test]
    fn agent_printable_digits_colons_and_transcript_letters_go_to_composer() {
        let (mut state, root) = routed_state("agent-printable-focus");
        state.surface = DevSurface::Agent;
        for character in ['1', 'f', ':'] {
            state.handle_printable(character);
        }
        assert!(state.composer_mode);
        assert_eq!(state.composer_input, "1f:");
        assert!(!state.command_mode, "Agent punctuation stays message text");
        std::fs::remove_dir_all(root).expect("remove temporary workspace");
    }

    #[test]
    fn beforeunload_status_reflects_the_user_decision() {
        assert_eq!(
            native_browser_dialog_resolution_status("beforeunload", true),
            "Page navigation confirmed · page resuming"
        );
        assert_eq!(
            native_browser_dialog_resolution_status("beforeunload", false),
            "Navigation dismissed · staying on page"
        );
    }

    fn test_native_dialog(dialog_type: &str, value: &str) -> BrowserDialogPrompt {
        BrowserDialogPrompt {
            pending: NativePendingDialog {
                id: "native-dialog-test-1".into(),
                context_id: "native-context-test".into(),
                frame_id: "native-frame-test".into(),
                dialog: glass_browser::browser::session::PendingDialog {
                    dialog_type: dialog_type.into(),
                    message: "Continue with this page operation?".into(),
                    default_value: (dialog_type == "prompt").then(|| value.to_string()),
                    url: "https://example.test/checkout".into(),
                },
            },
            prompt_input: value.into(),
            prompt_cursor: value.chars().count(),
            error: None,
        }
    }

    #[test]
    fn native_dialog_prompt_keys_edit_unicode_and_enforce_utf8_limit() {
        let root = std::env::temp_dir().join(format!(
            "glass-native-dialog-prompt-{}-{}",
            std::process::id(),
            NEXT_BROWSER_TOOL.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&root).expect("create temporary workspace");
        let mut state =
            DevTuiState::open_for_tui(&root, TuiLayout::Desktop).expect("open TUI state");
        state.browser_dialog = Some(test_native_dialog("prompt", "A🙂B"));
        state.browser_dialog.as_mut().unwrap().prompt_cursor = 2;

        state.handle_native_browser_dialog_key(
            crossterm::event::KeyCode::Left,
            crossterm::event::KeyModifiers::NONE,
        );
        state.handle_native_browser_dialog_key(
            crossterm::event::KeyCode::Char('é'),
            crossterm::event::KeyModifiers::NONE,
        );
        assert_eq!(state.browser_dialog.as_ref().unwrap().prompt_input, "Aé🙂B");
        state.handle_native_browser_dialog_key(
            crossterm::event::KeyCode::Backspace,
            crossterm::event::KeyModifiers::NONE,
        );
        assert_eq!(state.browser_dialog.as_ref().unwrap().prompt_input, "A🙂B");
        state.handle_native_browser_dialog_key(
            crossterm::event::KeyCode::Delete,
            crossterm::event::KeyModifiers::NONE,
        );
        assert_eq!(state.browser_dialog.as_ref().unwrap().prompt_input, "AB");

        let dialog = state.browser_dialog.as_mut().unwrap();
        dialog.prompt_input = "x".repeat(NATIVE_DIALOG_TEXT_LIMIT_BYTES);
        dialog.prompt_cursor = dialog.prompt_input.chars().count();
        state.handle_native_browser_dialog_key(
            crossterm::event::KeyCode::Char('é'),
            crossterm::event::KeyModifiers::NONE,
        );
        let dialog = state.browser_dialog.as_ref().unwrap();
        assert_eq!(dialog.prompt_input.len(), NATIVE_DIALOG_TEXT_LIMIT_BYTES);
        assert!(dialog.error.as_deref().unwrap().contains("UTF-8 bytes"));

        std::fs::remove_dir_all(root).expect("remove temporary workspace");
    }

    #[test]
    fn native_dialog_resolution_failure_keeps_the_exact_overlay_open() {
        let root = std::env::temp_dir().join(format!(
            "glass-native-dialog-stale-{}-{}",
            std::process::id(),
            NEXT_BROWSER_TOOL.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&root).expect("create temporary workspace");
        let mut state =
            DevTuiState::open_for_tui(&root, TuiLayout::Desktop).expect("open TUI state");
        state.browser_dialog = Some(test_native_dialog("confirm", ""));

        state.handle_native_browser_dialog_key(
            crossterm::event::KeyCode::Char('n'),
            crossterm::event::KeyModifiers::NONE,
        );
        let dialog = state.browser_dialog.as_ref().expect("dialog remains open");
        assert_eq!(dialog.pending.id, "native-dialog-test-1");
        assert!(
            dialog
                .error
                .as_deref()
                .is_some_and(|error| !error.is_empty())
        );

        std::fs::remove_dir_all(root).expect("remove temporary workspace");
    }

    #[test]
    fn project_navigation_and_editor_access_are_available_on_first_open() {
        let root = std::env::temp_dir().join(format!(
            "glass-workspace-routing-{}-{}",
            std::process::id(),
            NEXT_BROWSER_TOOL.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&root).expect("create temporary workspace");
        std::fs::write(
            root.join("glass.toml"),
            "[tools.probe]\ndescription='probe'\ncommand='echo unsafe'\n",
        )
        .expect("write executable project configuration");
        std::fs::write(root.join("README.md"), "workspace fixture\n")
            .expect("write file for editor access");

        for layout in [TuiLayout::Desktop, TuiLayout::Mobile] {
            let mut state = DevTuiState::open(&root, layout).expect("open TUI workspace");
            assert_eq!(state.surface, DevSurface::Agent);
            state.show_surface(DevSurface::App);
            assert_eq!(state.surface, DevSurface::App);
            command::execute(&mut state, "view agent").expect("navigate by command");
            assert_eq!(state.surface, DevSurface::Agent);
            state.files = vec!["README.md".into()];
            state.open_file_picker();
            assert!(state.file_picker_open);
            assert!(state.open_path("README.md").is_ok());
            state.open_selected_file_for_edit();
            assert_eq!(state.surface, DevSurface::Code);
            state.open_palette();
            assert!(state.command_mode);
            drop(state);
        }

        std::fs::remove_dir_all(root).expect("remove temporary workspace");
    }

    #[test]
    fn moving_code_selection_refreshes_preview_and_preserves_dirty_buffers() {
        let (mut state, root) = routed_state("file-selection-preview");
        for (path, content) in [
            ("a.txt", "ALPHA_PREVIEW_1\n"),
            ("b.txt", "BETA_PREVIEW_2\n"),
            ("c.txt", "GAMMA_PREVIEW_3\n"),
        ] {
            std::fs::write(root.join(path), content).expect("write preview fixture");
        }
        state.files = vec!["a.txt".into(), "b.txt".into(), "c.txt".into()];
        state.show_surface(DevSurface::Code);
        assert_eq!(state.focused_editor_path, "a.txt");
        assert_eq!(state.focused_editor_content, "ALPHA_PREVIEW_1\n");

        state
            .ws_mut()
            .expect("workspace lock")
            .project_mut()
            .edit_buffer(
                "a.txt",
                "ALPHA_DIRTY_BUFFER\n".into(),
                crate::development::Actor::local(),
            )
            .expect("edit first buffer");
        state.refresh_editor_projection();
        state.move_file_selection(1);
        assert_eq!(state.focused_editor_path, "b.txt");
        assert_eq!(state.focused_editor_content, "BETA_PREVIEW_2\n");
        state.move_file_selection(1);
        assert_eq!(state.focused_editor_path, "c.txt");
        assert_eq!(state.focused_editor_content, "GAMMA_PREVIEW_3\n");
        let workspace = state.ws().expect("workspace lock");
        let first = workspace
            .project()
            .buffer("a.txt")
            .expect("dirty first buffer retained");
        assert!(first.dirty);
        assert_eq!(first.content, "ALPHA_DIRTY_BUFFER\n");
        drop(workspace);

        drop(state);
        std::fs::remove_dir_all(root).expect("remove preview workspace");
    }

    #[test]
    fn editor_writes_and_preserves_dirty_buffers_on_first_open() {
        let root = std::env::temp_dir().join(format!("glass-editor-review-{}", std::process::id()));
        std::fs::create_dir_all(root.join("src")).expect("create temporary workspace");
        std::fs::write(
            root.join("glass.toml"),
            "[tools.probe]\ndescription='probe'\ncommand='echo unsafe'\n",
        )
        .expect("write executable project configuration");
        let original = "fn main() {}\n";
        std::fs::write(root.join("src/README.md"), original).expect("write test file");

        let mut state = DevTuiState::open(&root, TuiLayout::Desktop).expect("open TUI workspace");
        state.files = vec!["src/README.md".into()];
        assert_eq!(state.surface, DevSurface::Agent);

        state.show_surface(DevSurface::Code);
        assert_eq!(state.surface, DevSurface::Code);
        state.selected_file = 0;
        state.open_selected_file();
        assert_eq!(state.focused_editor_content, original);
        let unsaved = "unsaved review buffer\n";
        state
            .ws_mut()
            .expect("lock temporary workspace")
            .project_mut()
            .edit_buffer(
                "src/README.md",
                unsaved.into(),
                crate::development::Actor::local(),
            )
            .expect("seed dirty buffer for preview preservation");
        state.refresh_editor_projection();
        assert_eq!(state.focused_editor_content, unsaved);
        assert!(
            state
                .open_path("src/./README.md")
                .expect("open source file")
                .contains("Opened")
        );
        assert_eq!(state.focused_editor_content, unsaved);

        state.handle_printable('i');
        assert!(state.code_edit_mode);
        state.open_selected_file_for_edit();
        assert!(state.code_edit_mode);
        state.open_file_picker();
        assert!(state.file_picker_open);
        state.submit_file_picker();
        assert!(!state.file_picker_open);
        assert!(state.code_edit_mode);
        assert_eq!(state.surface, DevSurface::Code);
        assert!(state.open_path("src/./README.md").is_ok());
        assert!(state.code_edit_mode);
        assert_eq!(state.focused_editor_content, unsaved);
        assert_eq!(
            std::fs::read_to_string(root.join("src/README.md")).unwrap(),
            original
        );
        assert!(
            state
                .ws()
                .expect("lock workspace")
                .project()
                .buffer("src/README.md")
                .is_some_and(|buffer| buffer.dirty && buffer.content == unsaved)
        );

        state
            .ws_mut()
            .expect("lock temporary workspace")
            .project_mut()
            .propose_editor_change(
                "src/README.md",
                unsaved.into(),
                "proposal must not apply\n".into(),
                "test editor proposal review".into(),
                crate::development::Actor::local(),
            )
            .expect("seed pending proposal");
        state.refresh_editor_projection();
        assert!(state.accept_review_pack().is_ok());
        assert_eq!(
            state
                .ws()
                .expect("lock temporary workspace")
                .project()
                .buffer("src/README.md")
                .expect("preview buffer")
                .content,
            "proposal must not apply\n"
        );
        let edited = "edited in the project workspace\n";
        state
            .ws_mut()
            .expect("lock temporary workspace")
            .project_mut()
            .edit_buffer(
                "src/README.md",
                edited.into(),
                crate::development::Actor::local(),
            )
            .expect("edit buffer as the local human");
        state.refresh_editor_projection();
        assert_eq!(state.focused_editor_content, edited);
        state.save_editor_buffer().expect("save editor buffer");
        assert_eq!(
            std::fs::read_to_string(root.join("src/README.md")).expect("read file"),
            edited
        );

        std::fs::remove_dir_all(root).expect("remove temporary workspace");
    }

    #[test]
    fn browser_context_url_redaction_preserves_path_only() {
        assert_eq!(
            safe_browser_url("https://example.test/orders/7?token=secret#receipt"),
            Some("https://example.test/orders/7".into())
        );
        assert_eq!(
            page_origin("https://example.test/orders/7?token=secret"),
            Some("https://example.test".into())
        );
        assert_eq!(safe_browser_url(""), None);
    }

    #[test]
    fn browser_recovery_offer_explains_port_collision_choices() {
        let offer = BrowserRecoveryOffer::from_error("address already in use on port 9222", 9222);
        assert!(!offer.compatible_endpoint);
        assert_eq!(offer.actions().len(), 3);
        assert!(offer.guidance().contains("preferred endpoint"));

        let attach =
            BrowserRecoveryOffer::from_error("DevTools page target is available for attach", 9222);
        assert!(attach.compatible_endpoint);
        assert_eq!(attach.actions().len(), 4);
        assert!(attach.guidance().contains("Attach"));

        let native = BrowserRecoveryOffer::from_error_for_backend(
            "native browser process stopped",
            9222,
            true,
        );
        assert_eq!(native.actions().len(), 2);
        assert!(native.guidance().contains("native browser session"));
        assert!(!native.guidance().contains("Chrome"));
        assert!(!native.guidance().contains("9222"));
    }

    #[test]
    fn navigation_decoding_error_does_not_offer_process_recovery() {
        let (mut state, root) = routed_state("browser-decode-failure");
        assert!(!state.note_browser_failure(
            "glass.browser.navigate",
            "connection failure during content process load: HTTP document decoding"
        ));
        assert!(state.browser_recovery.is_none());
        std::fs::remove_dir_all(root).expect("remove test workspace");
    }

    #[test]
    fn browser_failure_recovery_uses_the_backend_from_tool_evidence() {
        let (mut state, root) = routed_state("browser-recovery-backend-evidence");
        state.apply_browser_result(
            "glass.browser.observe",
            &serde_json::json!({"backend":"native"}),
        );
        assert!(state.note_browser_failure("glass.browser.start", "native browser launch failure"));
        assert!(
            state
                .browser_recovery
                .as_ref()
                .expect("native recovery")
                .native_backend
        );

        state.dismiss_browser_recovery();
        state.apply_browser_result(
            "glass.browser.observe",
            &serde_json::json!({"backend":"chromium"}),
        );
        assert!(state.note_browser_failure(
            "glass.browser.start",
            "connection refused by browser endpoint"
        ));
        assert!(
            !state
                .browser_recovery
                .as_ref()
                .expect("Chromium recovery")
                .native_backend
        );
        std::fs::remove_dir_all(root).expect("remove test workspace");
    }

    fn crashed_browser_snapshot(state: &DevTuiState) -> super::super::snapshot::DisplaySnapshot {
        super::super::snapshot::DisplaySnapshot {
            root: state.snapshot_root.clone(),
            browser_health: super::super::snapshot::BrowserHealth::Crashed {
                last_process_id: Some(314),
                last_revision: Some(9),
                last_backend: Some("chromium".into()),
            },
            ..Default::default()
        }
    }

    #[test]
    fn native_crash_recovery_does_not_suggest_chromium_or_devtools() {
        let (mut state, root) = routed_state("native-browser-recovery-guidance");
        let snapshot = super::super::snapshot::DisplaySnapshot {
            root: state.snapshot_root.clone(),
            browser_health: super::super::snapshot::BrowserHealth::Crashed {
                last_process_id: None,
                last_revision: Some(9),
                last_backend: Some("native".into()),
            },
            ..Default::default()
        };
        state.apply_snapshot(&snapshot);
        let offer = state
            .browser_recovery
            .as_ref()
            .expect("native recovery offer");
        assert!(offer.native_backend);
        assert!(!offer.guidance().contains("Chrome"));
        assert!(!offer.guidance().contains("9222"));
        assert!(offer.actions()[0].1.contains("native"));
        std::fs::remove_dir_all(root).expect("remove test workspace");
    }

    #[test]
    fn crashed_snapshot_preserves_surfaces_and_defers_recovery_behind_active_ui() {
        use super::super::overlay::{ActiveOverlay, active_overlay};

        for (label, surface, overlay) in [
            ("code", DevSurface::Code, "code"),
            ("help", DevSurface::More, "help"),
            ("quit", DevSurface::Git, "quit"),
            ("regular", DevSurface::Terminal, "regular"),
        ] {
            let (mut state, root) = routed_state(label);
            state.surface = surface;
            state.composer_mode = false;
            state.pending_browser_navigation = Some("https://example.test/after-recovery".into());
            match overlay {
                "code" => state.code_edit_mode = true,
                "help" => state.help_open = true,
                "quit" => state.quit_confirmation = true,
                _ => {}
            }

            let snapshot = crashed_browser_snapshot(&state);
            state.apply_snapshot(&snapshot);

            assert_eq!(state.surface, surface, "{label} surface changed");
            assert!(state.browser_recovery.is_some(), "{label} recovery missing");
            assert!(state.status.contains("Browser recovery available"));
            assert_eq!(
                state.pending_browser_navigation.as_deref(),
                Some("https://example.test/after-recovery")
            );
            let expected = match overlay {
                "code" => Some(ActiveOverlay::FullscreenEditor),
                "help" => Some(ActiveOverlay::Help),
                "quit" => Some(ActiveOverlay::QuitConfirmation),
                _ => Some(ActiveOverlay::BrowserRecovery),
            };
            assert_eq!(active_overlay(&state), expected, "{label} overlay priority");

            match overlay {
                "code" => state.code_edit_mode = false,
                "help" => state.help_open = false,
                "quit" => state.quit_confirmation = false,
                _ => {}
            }
            if overlay != "regular" {
                assert_eq!(active_overlay(&state), Some(ActiveOverlay::BrowserRecovery));
            }
            state.dismiss_browser_recovery();
            assert_eq!(state.surface, surface, "{label} dismissal changed surface");
            assert!(state.browser_recovery.is_none());
            assert!(state.pending_browser_navigation.is_none());
            assert!(state.status.contains(surface.label()));
            drop(state);
            std::fs::remove_dir_all(root).expect("remove temporary workspace");
        }
    }

    #[test]
    fn browser_tool_failure_preserves_picker_and_overlay_state() {
        use super::super::overlay::{ActiveOverlay, active_overlay};

        let (mut state, root) = routed_state("browser-failure-preserves-ui");
        state.surface = DevSurface::Git;
        state.composer_mode = false;
        state.browser_target_picker = true;
        state.browser_target_picker_requested = true;
        state.browser_target_query = "docs".into();
        state.help_open = true;
        state.running_tool_job = Some(41);
        state.apply_tool_job_result(super::super::snapshot::ToolJobResult {
            id: 41,
            tool: "glass.browser.start".into(),
            result: Err("DevTools connection refused".into()),
        });

        assert_eq!(state.surface, DevSurface::Git);
        assert!(state.browser_target_picker);
        assert!(state.browser_target_picker_requested);
        assert_eq!(state.browser_target_query, "docs");
        assert!(state.browser_recovery.is_some());
        assert_eq!(active_overlay(&state), Some(ActiveOverlay::Help));
        assert_eq!(
            state.status,
            "Browser recovery available · current surface preserved"
        );

        state.help_open = false;
        assert_eq!(
            active_overlay(&state),
            Some(ActiveOverlay::BrowserTargetPicker)
        );
        state.close_browser_target_picker();
        assert_eq!(active_overlay(&state), Some(ActiveOverlay::BrowserRecovery));
        state.dismiss_browser_recovery();
        assert_eq!(state.surface, DevSurface::Git);
        assert!(state.browser_recovery.is_none());
        assert!(state.status.contains("Git remains active"));
        drop(state);
        std::fs::remove_dir_all(root).expect("remove temporary workspace");
    }

    #[test]
    fn failed_browser_observe_keeps_evidence_pending_for_bounded_retries() {
        let (mut state, root) = routed_state("browser-observe-retry");
        for attempt in 1..=3 {
            let id = 200 + attempt;
            state.running_tool_job = Some(id);
            state.apply_tool_job_result(super::super::snapshot::ToolJobResult {
                id,
                tool: "glass.browser.observe".into(),
                result: Err("temporary busy result".into()),
            });
            assert!(state.browser_observe_pending);
            assert!(state.status.contains(&format!("{attempt}/3")));
        }
        let id = 204;
        state.running_tool_job = Some(id);
        state.apply_tool_job_result(super::super::snapshot::ToolJobResult {
            id,
            tool: "glass.browser.observe".into(),
            result: Err("still unavailable".into()),
        });
        assert!(!state.browser_observe_pending);
        assert!(state.status.contains("evidence may be stale"));
        drop(state);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn browser_recovery_attach_disables_incognito() {
        assert_eq!(
            browser_recovery_arguments(9222, true),
            serde_json::json!({
                "port": 9222,
                "attach": true,
                "incognito": false,
                "modalDialogs": false,
            })
        );
        assert_eq!(
            browser_recovery_arguments(42123, false),
            serde_json::json!({
                "port": 42123,
                "attach": false,
                "incognito": true,
                "modalDialogs": true,
            })
        );
    }

    #[test]
    fn browser_target_picker_filters_and_redacts_urls() {
        let root = std::env::temp_dir().join(format!("glass-target-picker-{}", std::process::id()));
        std::fs::create_dir_all(&root).expect("create temporary workspace");
        let mut state =
            DevTuiState::open_for_tui(&root, TuiLayout::Desktop).expect("open temporary workspace");
        state.browser_workspace.replace_targets(vec![
            BrowserWorkspaceTarget {
                id: "page-docs".into(),
                title: "Project docs".into(),
                url: "https://example.test/docs?token=secret#receipt".into(),
                selected: false,
            },
            BrowserWorkspaceTarget {
                id: "page-private".into(),
                title: "Private docs".into(),
                url: "https://example.test/docs?session=private#fragment-secret".into(),
                selected: false,
            },
            BrowserWorkspaceTarget {
                id: "page-app".into(),
                title: "Application".into(),
                url: "http://localhost:3000".into(),
                selected: false,
            },
        ]);
        state
            .request_browser_target_picker("docs")
            .expect("queue target picker");
        state.queued_tool_request = None;
        let matches = state.browser_target_matches();
        assert_eq!(matches, vec![0, 1]);

        for sensitive_term in ["token", "secret", "receipt", "session", "fragment-secret"] {
            state.browser_target_query = sensitive_term.into();
            assert_eq!(
                state.browser_target_matches(),
                Vec::<usize>::new(),
                "URL-only term {sensitive_term:?} matched a redacted URL"
            );
        }
        state.browser_target_query = "docs".into();
        let view = state.browser_target_picker_view();
        assert!(view.contains("Project docs"));
        assert!(view.contains("Private docs"));
        assert!(view.contains("https://example.test/docs"));
        assert!(!view.contains("token=secret"));
        assert!(!view.contains("fragment-secret"));
        std::fs::remove_dir_all(root).expect("remove temporary workspace");
    }

    #[test]
    fn browser_target_query_keeps_selection_when_target_still_matches() {
        let root = std::env::temp_dir().join(format!(
            "glass-target-query-selection-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&root).expect("create temporary workspace");
        let mut state =
            DevTuiState::open_for_tui(&root, TuiLayout::Desktop).expect("open temporary workspace");
        state.browser_workspace.replace_targets(vec![
            BrowserWorkspaceTarget {
                id: "page-one".into(),
                title: "Docs one".into(),
                url: "https://example.test/one".into(),
                selected: false,
            },
            BrowserWorkspaceTarget {
                id: "page-two".into(),
                title: "Docs two".into(),
                url: "https://example.test/two".into(),
                selected: false,
            },
        ]);
        state.browser_target_query = "doc".into();
        state.browser_target_selection = 1;

        state.insert_browser_target_query('s');

        assert_eq!(state.browser_target_matches(), vec![0, 1]);
        assert_eq!(state.browser_target_selection, 1);
        std::fs::remove_dir_all(root).expect("remove temporary workspace");
    }

    #[test]
    fn typed_open_in_palette_runs_instead_of_fuzzy_action() {
        let root = std::env::temp_dir().join(format!("glass-typed-open-{}", std::process::id()));
        std::fs::create_dir_all(&root).expect("create temporary workspace");
        let mut state =
            DevTuiState::open_for_tui(&root, TuiLayout::Desktop).expect("open temporary workspace");
        state.files = vec!["src/lib.rs".into(), "Cargo.toml".into()];
        state.open_palette();
        state.command_input = "open".into();
        state.command_cursor = 4;
        let mut worker = super::super::snapshot::SnapshotWorker::spawn(&state);
        state.submit_palette(&mut worker);
        drop(worker);
        assert!(
            state.file_picker_open,
            "typed :open must open the file picker, not a fuzzy action"
        );
        std::fs::remove_dir_all(root).expect("remove temporary workspace");
    }

    #[test]
    fn palette_selects_label_match_after_query_narrows_fuzzy_results() {
        let (mut state, root) = routed_state("palette-label-selection");
        state.surface = DevSurface::Agent;
        state.open_palette();
        state.insert_palette_text("open task tracking");
        assert_eq!(
            state.selected_palette_action().map(|action| action.label),
            Some("Open task tracking")
        );
        drop(state);
        std::fs::remove_dir_all(root).expect("remove palette workspace");
    }

    #[test]
    fn gc_command_menu_dismissal_preserves_composer_and_escape_restores_query() {
        let (mut state, root) = routed_state("gc-command-dismissal");
        state.composer_input = "keep this draft".into();
        state.composer_cursor = 4;
        state.gc_command_menu_open = true;
        state.gc_command_query.clear();
        state.dismiss_gc_command_menu_trigger();
        assert!(!state.gc_command_menu_open);
        assert_eq!(state.composer_input, "keep this draft");
        assert_eq!(state.composer_cursor, 4);

        state.gc_command_menu_open = true;
        state.gc_command_query = "help".into();
        state.cancel_gc_command_menu();
        assert!(!state.gc_command_menu_open);
        assert_eq!(state.composer_input, "/help");
        assert_eq!(state.composer_cursor, "/help".len());

        state.gc_command_menu_open = false;
        state.status = "Conversation ready · draft preserved".into();
        state.gc_command_discovery_jobs.insert(17);
        state.running_tool_job = Some(17);
        state.apply_tool_job_result(super::super::snapshot::ToolJobResult {
            id: 17,
            tool: "glass.agent.commands".into(),
            result: Ok(serde_json::json!({"queued":true,"agentId":"agent-0002"})),
        });
        assert_eq!(state.running_tool_job, None);
        assert_eq!(state.status, "Conversation ready · draft preserved");
        assert!(!state.gc_command_menu_open);
        drop(state);
        std::fs::remove_dir_all(root).expect("remove palette workspace");
    }

    #[test]
    fn late_core_command_catalogue_updates_only_the_closed_palette_cache() {
        let (mut state, root) = routed_state("late-core-command-catalogue");
        let agent_id = crate::AgentId::parse("agent-0002").unwrap();
        state.selected_agent = Some(agent_id.clone());
        state.gc_command_menu_open = false;
        state.gc_command_loading = false;
        state.status = "Conversation ready · draft preserved".into();
        let mut snapshot = super::super::snapshot::DisplaySnapshot::default();
        snapshot.core_command_catalogues.insert(
            agent_id.clone(),
            super::super::snapshot::CoreCommandCatalogue {
                commands: vec![serde_json::json!({"name":"help","description":"Show help"})],
                presentation_version: 1,
                error: None,
            },
        );

        state.update_gc_command_catalogue(&snapshot);
        assert!(!state.gc_command_menu_open);
        assert!(!state.gc_command_loading);
        assert!(!state.gc_command_discovery_pending);
        assert_eq!(state.gc_command_items[0]["name"], "help");
        assert_eq!(state.status, "Conversation ready · draft preserved");

        snapshot.core_command_catalogues.insert(
            agent_id,
            super::super::snapshot::CoreCommandCatalogue {
                commands: Vec::new(),
                presentation_version: 0,
                error: Some("late discovery failure".into()),
            },
        );
        state.update_gc_command_catalogue(&snapshot);
        assert_eq!(state.status, "Conversation ready · draft preserved");

        drop(state);
        std::fs::remove_dir_all(root).expect("remove palette workspace");
    }

    #[test]
    fn reopening_gc_menu_waits_for_the_existing_catalogue_request() {
        let (mut state, root) = routed_state("gc-command-pending-reopen");
        state.gc_command_discovery_pending = true;
        let mut worker = super::super::snapshot::SnapshotWorker::spawn(&state);
        state.open_gc_command_menu(&mut worker);
        assert!(state.gc_command_menu_open);
        assert!(state.gc_command_loading);
        assert!(state.gc_command_discovery_pending);
        assert_eq!(state.running_tool_job, None);
        state.dismiss_gc_command_menu_trigger();
        assert!(state.gc_command_discovery_pending);
        drop(worker);
        drop(state);
        std::fs::remove_dir_all(root).expect("remove palette workspace");
    }

    #[test]
    fn typed_palette_selected_action_and_menu_share_live_view_dispatch() {
        let (mut typed, typed_root) = routed_state("typed-browser-view");
        typed.surface = DevSurface::App;
        typed.open_palette();
        typed.insert_palette_text("browser view");
        let mut typed_worker = super::super::snapshot::SnapshotWorker::spawn(&typed);
        typed.submit_palette(&mut typed_worker);
        drop(typed_worker);
        let typed_request = typed
            .take_browser_visual_request()
            .expect("typed browser view request");

        let (mut palette, palette_root) = routed_state("selected-browser-view");
        palette.surface = DevSurface::App;
        palette.open_palette();
        palette.insert_palette_text("Live browser");
        assert_eq!(
            palette.selected_palette_action().map(|action| action.label),
            Some("Live browser view")
        );
        let mut palette_worker = super::super::snapshot::SnapshotWorker::spawn(&palette);
        palette.submit_palette(&mut palette_worker);
        drop(palette_worker);
        let palette_request = palette
            .take_browser_visual_request()
            .expect("selected browser view request");

        let (mut menu, menu_root) = routed_state("menu-browser-view");
        menu.surface = DevSurface::App;
        let live_view_index = menu
            .surface_actions()
            .iter()
            .position(|action| action.label == "Live browser view")
            .expect("live browser view menu action");
        menu.open_menu();
        menu.menu_selection = live_view_index;
        menu.run_menu_action();
        let menu_request = menu
            .take_browser_visual_request()
            .expect("menu browser view request");

        assert!(typed_request.live);
        assert_eq!(typed_request, palette_request);
        assert_eq!(palette_request, menu_request);
        assert_eq!(typed.status, palette.status);
        assert_eq!(palette.status, menu.status);
        assert_eq!(typed.surface, DevSurface::App);
        assert_eq!(palette.surface, DevSurface::App);
        assert_eq!(menu.surface, DevSurface::App);

        drop((typed, palette, menu));
        std::fs::remove_dir_all(typed_root).expect("remove typed workspace");
        std::fs::remove_dir_all(palette_root).expect("remove palette workspace");
        std::fs::remove_dir_all(menu_root).expect("remove menu workspace");
    }

    #[test]
    fn menu_and_selected_palette_action_share_placeholder_prefill() {
        let (mut menu, menu_root) = routed_state("menu-navigate-prefill");
        menu.surface = DevSurface::App;
        let navigate_index = menu
            .surface_actions()
            .iter()
            .position(|action| action.label == "Navigate")
            .expect("navigate menu action");
        menu.open_menu();
        menu.menu_selection = navigate_index;
        menu.run_menu_action();

        let (mut palette, palette_root) = routed_state("palette-navigate-prefill");
        palette.surface = DevSurface::App;
        palette.open_palette();
        let navigate_index = palette
            .surface_actions()
            .iter()
            .position(|action| action.label == "Navigate")
            .expect("navigate palette action");
        palette.palette_selection = navigate_index;
        let mut worker = super::super::snapshot::SnapshotWorker::spawn(&palette);
        palette.submit_palette(&mut worker);
        drop(worker);

        assert_eq!(menu.command_input, "browser navigate ");
        assert_eq!(palette.command_input, menu.command_input);
        assert_eq!(palette.status, menu.status);
        assert!(menu.command_mode);
        assert!(palette.command_mode);

        drop((menu, palette));
        std::fs::remove_dir_all(menu_root).expect("remove menu workspace");
        std::fs::remove_dir_all(palette_root).expect("remove palette workspace");
    }

    #[test]
    fn menu_and_selected_palette_action_share_unavailable_reason() {
        let (mut menu, menu_root) = routed_state("menu-unavailable-observe");
        menu.surface = DevSurface::App;
        let observe_index = menu
            .surface_actions()
            .iter()
            .position(|action| action.label == "Observe page")
            .expect("observe menu action");
        menu.open_menu();
        menu.menu_selection = observe_index;
        menu.run_menu_action();

        let (mut palette, palette_root) = routed_state("palette-unavailable-observe");
        palette.surface = DevSurface::App;
        palette.open_palette();
        let observe_index = palette
            .surface_actions()
            .iter()
            .position(|action| action.label == "Observe page")
            .expect("observe palette action");
        palette.palette_selection = observe_index;
        let mut worker = super::super::snapshot::SnapshotWorker::spawn(&palette);
        palette.submit_palette(&mut worker);
        drop(worker);

        assert_eq!(palette.status, menu.status);
        assert_eq!(palette.palette_error, menu.palette_error);
        assert!(menu.menu_open, "unavailable menu action stays visible");
        assert!(
            palette.command_mode,
            "unavailable palette action stays visible"
        );
        assert!(menu.status.contains("browser is not connected"));

        drop((menu, palette));
        std::fs::remove_dir_all(menu_root).expect("remove menu workspace");
        std::fs::remove_dir_all(palette_root).expect("remove palette workspace");
    }

    #[test]
    fn colon_is_literal_text_in_the_always_ready_agent_composer() {
        let root =
            std::env::temp_dir().join(format!("glass-composer-colon-{}", std::process::id()));
        std::fs::create_dir_all(&root).expect("create temporary workspace");
        let mut state =
            DevTuiState::open_for_tui(&root, TuiLayout::Desktop).expect("open temporary workspace");

        state.handle_printable(':');

        assert!(state.composer_mode);
        assert_eq!(state.composer_input, ":");
        assert!(!state.command_mode);
        std::fs::remove_dir_all(root).expect("remove temporary workspace");
    }

    #[test]
    fn file_picker_filters_and_opens_a_matching_path() {
        let root = std::env::temp_dir().join(format!("glass-file-picker-{}", std::process::id()));
        std::fs::create_dir_all(root.join("src")).expect("create temporary workspace");
        std::fs::write(root.join("src/lib.rs"), "fn lib() {}\n").unwrap();
        std::fs::write(root.join("src/main.rs"), "fn main() {}\n").unwrap();
        let mut state =
            DevTuiState::open_for_tui(&root, TuiLayout::Desktop).expect("open temporary workspace");
        state.files = vec![
            "src/lib.rs".into(),
            "src/main.rs".into(),
            "README.md".into(),
        ];
        state.open_file_picker();
        assert!(state.file_picker_open);
        for character in "main".chars() {
            state.insert_file_picker_char(character);
        }
        let matches = state.file_picker_matches();
        assert_eq!(matches.len(), 1);
        assert_eq!(state.files[matches[0]], "src/main.rs");
        state.submit_file_picker();
        assert!(!state.file_picker_open);
        assert_eq!(state.focused_editor_path, "src/main.rs");
        assert!(state.code_edit_mode);
        std::fs::remove_dir_all(root).expect("remove temporary workspace");
    }

    #[test]
    fn composer_shift_enter_and_history_are_local_edits() {
        let root =
            std::env::temp_dir().join(format!("glass-composer-history-{}", std::process::id()));
        std::fs::create_dir_all(&root).expect("create temporary workspace");
        let mut state =
            DevTuiState::open_for_tui(&root, TuiLayout::Desktop).expect("open temporary workspace");
        state.open_composer();
        state.insert_composer_text("first");
        state.remember_composer_history("first");
        state.composer_input.clear();
        state.composer_cursor = 0;
        state.insert_composer_text("second");
        state.navigate_composer_history(true);
        assert_eq!(state.composer_input, "first");
        state.navigate_composer_history(false);
        assert_eq!(state.composer_input, "second");
        state.insert_composer_newline();
        assert_eq!(state.composer_input, "second\n");
        state.navigate_composer_history(true);
        assert!(
            state.composer_cursor < state.composer_input.len(),
            "Up on a wrapped draft should move to the previous line before history"
        );
        std::fs::remove_dir_all(root).expect("remove temporary workspace");
    }

    #[test]
    fn agent_composer_does_not_reveal_an_existing_editor_buffer() {
        let root = std::env::temp_dir().join(format!(
            "glass-agent-composer-editor-split-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(root.join(".github/workflows"))
            .expect("create temporary workspace");
        std::fs::write(
            root.join(".github/workflows/ci.yml"),
            "name: harmless fixture\n",
        )
        .expect("write harmless source fixture");
        let mut state =
            DevTuiState::open_for_tui(&root, TuiLayout::Desktop).expect("open temporary workspace");
        state
            .ws_mut()
            .expect("workspace lock")
            .project_mut()
            .open_buffer(
                ".github/workflows/ci.yml",
                crate::development::Actor::local(),
            )
            .expect("open source fixture");
        state.refresh_editor_projection();
        assert_eq!(state.focused_editor_path, ".github/workflows/ci.yml");

        state.surface = DevSurface::Agent;
        state.open_composer();

        assert!(state.composer_mode);
        assert_eq!(state.focused_editor_path, ".github/workflows/ci.yml");
        assert!(
            !state.factory_split,
            "composer must not reveal a previously opened editor buffer by default"
        );
        std::fs::remove_dir_all(root).expect("remove temporary workspace");
    }

    #[test]
    fn external_delegate_result_is_visible_in_tui_status() {
        let root =
            std::env::temp_dir().join(format!("glass-delegate-status-{}", std::process::id()));
        std::fs::create_dir_all(&root).expect("create temporary workspace");
        let mut state =
            DevTuiState::open_for_tui(&root, TuiLayout::Desktop).expect("open temporary workspace");
        state.running_tool_job = Some(7);
        state.apply_tool_job_result(super::super::snapshot::ToolJobResult {
            id: 7,
            tool: "glass.agent.delegate".into(),
            result: Ok(serde_json::json!({
                "harness": "codex",
                "success": true,
                "timedOut": false,
                "output": "REAL_RESULT\nsecond line\n",
                "stderr": "",
            })),
        });
        assert!(state.status.contains("Temporary codex completed"));
        assert!(state.status.contains("REAL_RESULT"));
        assert_eq!(state.surface, DevSurface::Agent);
        assert!(state.running_tool_job.is_none());
        std::fs::remove_dir_all(root).expect("remove temporary workspace");
    }

    #[test]
    fn composer_tab_completes_file_mentions_and_submit_pins_the_path() {
        let root =
            std::env::temp_dir().join(format!("glass-composer-mention-{}", std::process::id()));
        std::fs::create_dir_all(&root).expect("create temporary workspace");
        let mut state =
            DevTuiState::open_for_tui(&root, TuiLayout::Desktop).expect("open temporary workspace");
        state.files = vec!["src/main.rs".into(), "src/lib.rs".into()];
        state.focused_editor_path = "src/main.rs".into();
        state.open_composer();
        state.insert_composer_text("inspect @fi");
        state.complete_composer_mention();
        assert_eq!(state.composer_input, "inspect @file");
        state.complete_composer_mention();
        assert_eq!(state.composer_input, "inspect @file:src/main.rs");
        std::fs::remove_dir_all(root).expect("remove temporary workspace");
    }

    #[test]
    fn editor_gutter_marks_open_comments_on_the_focused_buffer() {
        let root = std::env::temp_dir().join(format!(
            "glass-editor-comment-gutter-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&root).expect("create temporary workspace");
        let mut state =
            DevTuiState::open_for_tui(&root, TuiLayout::Desktop).expect("open temporary workspace");
        state.focused_editor_path = "src/main.rs".into();
        state
            .editor_comments
            .push(crate::development::EditorComment {
                id: "comment-1".into(),
                path: "src/main.rs".into(),
                start_line: 4,
                end_line: 4,
                text: "simplify this".into(),
                actor: crate::development::Actor::local(),
                state: crate::development::EditorCommentState::Open,
                created_revision: 1,
                updated_revision: 1,
            });
        let marks = state.editor_gutter_marks();
        assert!(marks.contains(&(4, native::GutterMark::Comment)));
        let notes = state.editor_source_notes();
        assert!(
            notes
                .iter()
                .any(|(line, note)| *line == 4 && note.contains("simplify this"))
        );
        state.focused_editor_line = 4;
        state.show_comment_thread();
        assert!(
            state
                .editor_engine
                .overlay
                .as_deref()
                .is_some_and(|overlay| overlay.contains("simplify this"))
        );
        std::fs::remove_dir_all(root).expect("remove temporary workspace");
    }

    #[test]
    fn gutter_marks_diagnostics_and_proof_on_source_lines() {
        let root = std::env::temp_dir().join(format!("glass-gutter-proof-{}", std::process::id()));
        std::fs::create_dir_all(root.join("src")).expect("create source directory");
        std::fs::write(root.join("src/main.rs"), "fn main() {}\nfn extra() {}\n")
            .expect("write source");
        let mut state =
            DevTuiState::open_for_tui(&root, TuiLayout::Desktop).expect("open temporary workspace");
        state.focused_editor_path = "src/main.rs".into();
        state.focused_editor_line = 2;
        state
            .editor_diagnostics
            .push(crate::development::LanguageDiagnostic {
                path: "src/main.rs".into(),
                start: crate::development::DiagnosticPosition {
                    line: 0,
                    character: 3,
                },
                end: crate::development::DiagnosticPosition {
                    line: 0,
                    character: 7,
                },
                severity: Some(1),
                code: None,
                source: Some("rustc".into()),
                message: "unused variable".into(),
            });
        state
            .ws_mut()
            .expect("workspace lock")
            .project_mut()
            .link_runtime_source(
                "action.main",
                "src/main.rs",
                1,
                1,
                crate::development::LinkProvenance::ExplicitMarker,
                "handler",
                1.0,
                crate::development::Actor::local(),
            )
            .expect("link");
        state.last_proof_ok = Some(true);
        let marks = state.editor_gutter_marks();
        assert!(
            marks.contains(&(1, native::GutterMark::Lsp)),
            "diagnostic is on LSP line 0 → editor line 1: {marks:?}"
        );
        assert!(
            marks.contains(&(1, native::GutterMark::Proof)),
            "proof sits on the handler, not the cursor: {marks:?}"
        );
        assert!(!marks.contains(&(2, native::GutterMark::Proof)));
        let notes = state.editor_source_notes();
        assert!(
            notes
                .iter()
                .any(|(line, note)| *line == 1 && note.contains("unused variable"))
        );
        std::fs::remove_dir_all(root).expect("remove temporary workspace");
    }

    #[test]
    fn page_gutter_marks_graph_linked_handlers() {
        let root = std::env::temp_dir().join(format!("glass-page-gutter-{}", std::process::id()));
        std::fs::create_dir_all(root.join("src")).expect("create source directory");
        std::fs::write(root.join("src/button.tsx"), "export function Pay() {}\n")
            .expect("write source");
        let mut state =
            DevTuiState::open_for_tui(&root, TuiLayout::Desktop).expect("open temporary workspace");
        state
            .ws_mut()
            .expect("workspace lock")
            .project_mut()
            .link_runtime_source(
                "action.checkout.submit",
                "src/button.tsx",
                1,
                1,
                crate::development::LinkProvenance::ExplicitMarker,
                "test link",
                1.0,
                crate::development::Actor::local(),
            )
            .expect("link source");
        state.focused_editor_path = "src/button.tsx".into();
        let marks = state.editor_gutter_marks();
        assert!(marks.contains(&(1, native::GutterMark::Page)));
        std::fs::remove_dir_all(root).expect("remove temporary workspace");
    }

    #[test]
    fn gp_navigates_to_the_inferred_app_route() {
        let root = std::env::temp_dir().join(format!("glass-gp-app-{}", std::process::id()));
        std::fs::create_dir_all(&root).expect("create temporary workspace");
        let mut state =
            DevTuiState::open_for_tui(&root, TuiLayout::Desktop).expect("open temporary workspace");
        state.focused_editor_path = "app/settings/page.tsx".into();
        state.focused_editor_line = 1;
        state.process_urls = vec!["http://localhost:3000/".into()];
        state.jump_page_from_source();
        assert_eq!(
            state.pending_browser_navigation.as_deref(),
            Some("http://localhost:3000/settings")
        );
        assert_eq!(state.surface, DevSurface::App);
        std::fs::remove_dir_all(root).expect("remove temporary workspace");
    }

    #[test]
    fn quitting_after_gp_still_guards_a_dirty_editor_buffer() {
        let root = std::env::temp_dir().join(format!("glass-gp-dirty-{}", std::process::id()));
        std::fs::create_dir_all(root.join("src")).expect("create source directory");
        std::fs::write(
            root.join("src/button.tsx"),
            "<button data-glass-entity=\"action.checkout.submit\">Pay</button>\n",
        )
        .expect("write source");
        let mut state =
            DevTuiState::open_for_tui(&root, TuiLayout::Desktop).expect("open workspace");
        {
            let mut workspace = state.ws_mut().expect("workspace lock");
            workspace
                .project_mut()
                .open_buffer("src/button.tsx", crate::development::Actor::local())
                .expect("open source buffer");
            workspace
                .project_mut()
                .link_runtime_source(
                    "action.checkout.submit",
                    "src/button.tsx",
                    1,
                    1,
                    crate::development::LinkProvenance::ExplicitMarker,
                    "test link",
                    1.0,
                    crate::development::Actor::local(),
                )
                .expect("link source to page entity");
        }
        state.browser_workspace.replace_entities(
            1,
            vec![BrowserWorkspaceEntity {
                reference: "action.checkout.submit".into(),
                role: "button".into(),
                name: "Pay".into(),
                actionable: true,
                revision: 1,
            }],
        );
        state.focused_editor_path = "src/button.tsx".into();
        state.refresh_editor_projection();
        state.enter_code_edit();
        state.editor_engine.enter_insert();
        state.edit_code_key(
            crossterm::event::KeyCode::Char('!'),
            crossterm::event::KeyModifiers::NONE,
        );
        state.editor_engine.ghost = None;
        state.handle_editor_escape();
        assert_eq!(state.editor_engine.mode, EditorMode::Normal);
        state.edit_code_key(
            crossterm::event::KeyCode::Char('g'),
            crossterm::event::KeyModifiers::NONE,
        );
        state.edit_code_key(
            crossterm::event::KeyCode::Char('p'),
            crossterm::event::KeyModifiers::NONE,
        );
        assert_eq!(state.surface, DevSurface::App);
        assert!(!state.code_edit_mode);
        assert!(state.focused_editor_dirty);

        state.request_quit();
        assert_eq!(state.editor_exit_prompt, Some(EditorExitPrompt::Unsaved));
        assert!(!state.quit_confirmation);
        assert!(!state.quit);
        assert!(state.code_edit_mode);
        assert!(state.focused_editor_content.contains('!'));
        std::fs::remove_dir_all(root).expect("remove temporary workspace");
    }

    #[test]
    fn quitting_selects_the_dirty_buffer_when_another_buffer_is_focused() {
        let root = std::env::temp_dir().join(format!(
            "glass-quit-dirty-buffer-selection-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(root.join("src")).expect("create source directory");
        std::fs::write(root.join("src/dirty.rs"), "fn dirty() {}\n").expect("write dirty source");
        std::fs::write(root.join("src/clean.rs"), "fn clean() {}\n").expect("write clean source");
        let mut state =
            DevTuiState::open_for_tui(&root, TuiLayout::Desktop).expect("open workspace");
        {
            let mut workspace = state.ws_mut().expect("workspace lock");
            let project = workspace.project_mut();
            project
                .open_buffer("src/dirty.rs", crate::development::Actor::local())
                .expect("open dirty buffer");
            project
                .open_buffer("src/clean.rs", crate::development::Actor::local())
                .expect("open clean buffer");
            project
                .edit_buffer(
                    "src/dirty.rs",
                    "fn dirty() { /* unsaved */ }\n".into(),
                    crate::development::Actor::local(),
                )
                .expect("make dirty buffer unsaved");
        }
        state.editor_buffer_index = 0;
        state.refresh_editor_projection();
        assert_eq!(state.focused_editor_path, "src/clean.rs");
        assert!(!state.focused_editor_dirty);

        state.request_quit();

        assert_eq!(state.editor_buffer_index, 1);
        assert_eq!(state.focused_editor_path, "src/dirty.rs");
        assert!(state.focused_editor_dirty);
        assert_eq!(state.editor_exit_prompt, Some(EditorExitPrompt::Unsaved));
        assert!(!state.quit_confirmation);
        assert!(!state.quit);
        std::fs::remove_dir_all(root).expect("remove temporary workspace");
    }

    #[test]
    fn g_on_app_opens_the_linked_source() {
        let root = std::env::temp_dir().join(format!("glass-app-source-{}", std::process::id()));
        std::fs::create_dir_all(root.join("src")).expect("create source directory");
        std::fs::write(
            root.join("src/button.tsx"),
            "<button data-glass-entity=\"action.checkout.submit\">Pay</button>\n",
        )
        .expect("write source");
        let mut state =
            DevTuiState::open_for_tui(&root, TuiLayout::Desktop).expect("open temporary workspace");
        state.files = vec!["src/button.tsx".into()];
        state.browser_workspace.replace_entities(
            1,
            vec![BrowserWorkspaceEntity {
                reference: "action.checkout.submit".into(),
                role: "button".into(),
                name: "Pay".into(),
                actionable: true,
                revision: 1,
            }],
        );
        state.browser_workspace.state_mut().selected_entity = Some(0);
        state.jump_source_from_page();
        assert_eq!(state.focused_editor_path, "src/button.tsx");
        assert_eq!(state.focused_editor_line, 1);
        assert!(state.code_edit_mode);
        std::fs::remove_dir_all(root).expect("remove temporary workspace");
    }

    #[test]
    fn hosted_fim_stub_fills_an_empty_block() {
        let root = std::env::temp_dir().join(format!("glass-fim-stub-{}", std::process::id()));
        std::fs::create_dir_all(root.join("src")).expect("create source directory");
        std::fs::write(
            root.join("glass.toml"),
            "[editor.fim]\nendpoint = \"stub://test\"\nmodel = \"stub\"\n",
        )
        .expect("write fim config");
        std::fs::write(root.join("src/main.rs"), "fn main() {\n    \n}\n").expect("write source");
        let mut state =
            DevTuiState::open_for_tui(&root, TuiLayout::Desktop).expect("open temporary workspace");
        state
            .ws_mut()
            .expect("lock")
            .project_mut()
            .open_buffer("src/main.rs", crate::development::Actor::local())
            .expect("open");
        state.refresh_editor_projection();
        state
            .ws_mut()
            .expect("lock")
            .project_mut()
            .set_buffer_cursor("src/main.rs", 2, 5)
            .expect("cursor");
        state.refresh_editor_projection();
        state.request_ghost_from_line();
        let mut attempts = 0;
        while state.editor_engine.ghost.is_none() && attempts < 50 {
            let _ = state.tick_fim();
            std::thread::sleep(std::time::Duration::from_millis(5));
            attempts += 1;
        }
        assert_eq!(
            state
                .editor_engine
                .ghost
                .as_ref()
                .map(|ghost| ghost.text.as_str()),
            Some("todo!()"),
            "configured stub FIM should fill the empty block"
        );
        std::fs::remove_dir_all(root).expect("remove temporary workspace");
    }

    #[test]
    fn resident_fim_response_requires_the_exact_request_identifier() {
        let agent_id = crate::AgentId::parse("agent-fim-test").unwrap();
        let response = |sequence, request_id: &str, text: &str| crate::AgentEvent {
            sequence,
            agent_id: agent_id.clone(),
            timestamp_ms: 0,
            kind: "response".into(),
            payload: serde_json::json!({
                "type": "response",
                "operation": "complete",
                "id": request_id,
                "ok": true,
                "result": {"text": text}
            }),
        };
        let old_request = response(1, "glass-old", "completion for cursor A");
        let current_request = response(2, "glass-current", "completion for cursor B");
        assert_eq!(
            matching_resident_fim_response(
                &[old_request.clone(), current_request],
                &agent_id,
                "glass-current"
            ),
            Some(Ok(Some("completion for cursor B".into())))
        );
        assert!(
            matching_resident_fim_response(&[old_request], &agent_id, "glass-current").is_none()
        );
    }

    #[test]
    fn fim_results_and_ghost_acceptance_require_the_originating_buffer_revision() {
        let root = std::env::temp_dir().join(format!("glass-fim-owner-{}", std::process::id()));
        std::fs::create_dir_all(root.join("src")).expect("create source directory");
        std::fs::write(root.join("src/main.rs"), "fn main() {}\n").expect("write main source");
        std::fs::write(root.join("src/other.rs"), "fn other() {}\n").expect("write other source");
        let mut state =
            DevTuiState::open_for_tui(&root, TuiLayout::Desktop).expect("open workspace");
        {
            let mut workspace = state.ws_mut().expect("workspace lock");
            workspace
                .project_mut()
                .open_buffer("src/main.rs", crate::development::Actor::local())
                .expect("open main buffer");
            workspace
                .project_mut()
                .open_buffer("src/other.rs", crate::development::Actor::local())
                .expect("open other buffer");
        }
        state.editor_buffer_index = 0;
        state.refresh_editor_projection();
        state.enter_code_edit();
        let origin_revision = state.current_editor_revision().expect("main revision");
        let (sender, receiver) = std::sync::mpsc::channel();
        sender
            .send(Ok("main-only".into()))
            .expect("send test result");
        state.pending_fim = Some(PendingFim::Thread {
            path: "src/main.rs".into(),
            offset: 0,
            revision: origin_revision,
            rx: receiver,
        });

        state.editor_buffer_index = 1;
        state.refresh_editor_projection();
        assert!(!state.tick_fim());
        assert!(state.editor_engine.ghost.is_none());
        assert!(state.status.contains("Stale suggestion discarded"));

        let other_revision = state.current_editor_revision().expect("other revision");
        assert!(state.install_ghost_text(
            "src/other.rs",
            0,
            other_revision,
            "wrong-revision".into(),
        ));
        state
            .ws_mut()
            .expect("workspace lock")
            .project_mut()
            .edit_buffer(
                "src/other.rs",
                "fn other() { /* changed */ }\n".into(),
                crate::development::Actor::local(),
            )
            .expect("edit other buffer");
        state.refresh_editor_projection();
        let changed_content = state.focused_editor_content.clone();
        state.edit_code_key(
            crossterm::event::KeyCode::Tab,
            crossterm::event::KeyModifiers::NONE,
        );
        assert_eq!(state.focused_editor_content, changed_content);
        assert!(state.editor_engine.ghost.is_none());
        assert!(state.status.contains("Stale suggestion discarded"));

        let current_revision = state.current_editor_revision().expect("current revision");
        assert!(state.install_ghost_text(
            "src/other.rs",
            0,
            current_revision,
            "wrong-buffer".into(),
        ));
        state.editor_buffer_index = 0;
        state.refresh_editor_projection();
        let main_content = state.focused_editor_content.clone();
        state.edit_code_key(
            crossterm::event::KeyCode::Tab,
            crossterm::event::KeyModifiers::NONE,
        );
        assert_eq!(state.focused_editor_content, main_content);
        assert!(!state.focused_editor_content.contains("wrong-buffer"));
        assert!(state.status.contains("Stale suggestion discarded"));
        std::fs::remove_dir_all(root).expect("remove temporary workspace");
    }

    #[test]
    fn ghost_tab_jumps_to_the_next_incomplete_ident() {
        let root = std::env::temp_dir().join(format!("glass-ghost-next-{}", std::process::id()));
        std::fs::create_dir_all(root.join("src")).expect("create source directory");
        let source = "fn hello_world() {}\nfn hello_\nfn greet_user() {}\nfn greet_\n";
        std::fs::write(root.join("src/main.rs"), source).expect("write source");
        let mut state =
            DevTuiState::open_for_tui(&root, TuiLayout::Desktop).expect("open temporary workspace");
        state
            .ws_mut()
            .expect("workspace lock")
            .project_mut()
            .open_buffer("src/main.rs", crate::development::Actor::local())
            .expect("open buffer");
        state.refresh_editor_projection();
        state.enter_code_edit();
        let _ = state.set_editor_cursor(
            "src/main.rs",
            crate::development::TextPosition {
                line: 2,
                column: 10,
            },
            false,
        );
        state.refresh_editor_projection();
        let offset = crate::development::editor::text_position_offset(
            &state.focused_editor_content,
            crate::development::TextPosition {
                line: state.focused_editor_line,
                column: state.focused_editor_column,
            },
        )
        .expect("ghost cursor offset");
        let revision = state.current_editor_revision().expect("buffer revision");
        assert!(state.install_ghost_text("src/main.rs", offset, revision, "world".into(),));
        state.edit_code_key(
            crossterm::event::KeyCode::Tab,
            crossterm::event::KeyModifiers::NONE,
        );
        assert!(
            state.focused_editor_content.contains("fn hello_world\n"),
            "tab should apply the ghost: {}",
            state.focused_editor_content
        );
        assert_eq!(state.focused_editor_line, 4);
        assert_eq!(
            state
                .editor_engine
                .ghost
                .as_ref()
                .map(|ghost| ghost.text.as_str()),
            Some("user")
        );
        assert!(state.status.contains("next edit"));
        std::fs::remove_dir_all(root).expect("remove temporary workspace");
    }

    #[test]
    fn crew_tool_result_stores_the_wake_artifact() {
        let root = std::env::temp_dir().join(format!("glass-crew-wake-tui-{}", std::process::id()));
        std::fs::create_dir_all(&root).expect("create temporary workspace");
        let mut state =
            DevTuiState::open_for_tui(&root, TuiLayout::Desktop).expect("open temporary workspace");
        state.running_tool_job = Some(3);
        state.apply_tool_job_result(super::super::snapshot::ToolJobResult {
            id: 3,
            tool: "glass.task.crew".into(),
            result: Ok(serde_json::json!({
                "goal": "add settings toggle",
                "wake": {
                    "id": "add-settings-toggle",
                    "goal": "add settings toggle",
                    "worktree": "/tmp/worktree",
                    "checkpoint": "before-crew:add settings toggle",
                    "createdAtMs": 1,
                    "tasks": [{"id":"task-0001","role":"architect","title":"architect: add settings toggle","state":"queued"}]
                }
            })),
        });
        let wake = state.last_crew_wake.expect("wake");
        assert!(wake.contains("WAKE add-settings-toggle"));
        assert!(wake.contains("architect task-0001 queued"));
        assert_eq!(state.surface, DevSurface::Tasks);
        std::fs::remove_dir_all(root).expect("remove temporary workspace");
    }

    #[test]
    fn review_accept_applies_the_wake_proposal_pack() {
        let root = std::env::temp_dir().join(format!("glass-review-pack-{}", std::process::id()));
        std::fs::create_dir_all(root.join("src")).expect("create source directory");
        std::fs::write(root.join("src/main.rs"), "fn main() {}\n").expect("write source");
        let mut state =
            DevTuiState::open_for_tui(&root, TuiLayout::Desktop).expect("open temporary workspace");
        state
            .ws_mut()
            .expect("workspace lock")
            .project_mut()
            .open_buffer("src/main.rs", crate::development::Actor::local())
            .expect("open buffer");
        state
            .ws_mut()
            .expect("workspace lock")
            .project_mut()
            .propose_editor_change(
                "src/main.rs",
                "fn main() {}\n".into(),
                "fn main() { 1 }\n".into(),
                "add one".into(),
                crate::development::Actor::local(),
            )
            .expect("propose");
        state.refresh_editor_projection();
        let accepted = state.accept_review_pack().expect("accept pack");
        assert!(accepted.contains("1 proposal"));
        assert_eq!(
            state.focused_editor_content, "fn main() { 1 }\n",
            "pack accept must write the proposed buffer"
        );
        std::fs::remove_dir_all(root).expect("remove temporary workspace");
    }

    #[test]
    fn gm_adds_structural_carets_and_inserts_at_each() {
        let root = std::env::temp_dir().join(format!("glass-multi-cursor-{}", std::process::id()));
        std::fs::create_dir_all(root.join("src")).expect("create source directory");
        std::fs::write(root.join("src/main.rs"), "fn foo() { foo(); foo }\n").expect("write");
        let mut state =
            DevTuiState::open_for_tui(&root, TuiLayout::Desktop).expect("open temporary workspace");
        state
            .ws_mut()
            .expect("lock")
            .project_mut()
            .open_buffer("src/main.rs", crate::development::Actor::local())
            .expect("open");
        state.refresh_editor_projection();
        state.enter_code_edit();
        state.handle_editor_escape();
        state
            .ws_mut()
            .expect("lock")
            .project_mut()
            .set_buffer_cursor("src/main.rs", 1, 4)
            .expect("cursor");
        state.refresh_editor_projection();
        state.edit_code_key(
            crossterm::event::KeyCode::Char('g'),
            crossterm::event::KeyModifiers::empty(),
        );
        state.edit_code_key(
            crossterm::event::KeyCode::Char('m'),
            crossterm::event::KeyModifiers::empty(),
        );
        assert_eq!(
            state.editor_engine.extra_selections.len(),
            2,
            "gm should add a caret on every other foo"
        );
        state.edit_code_key(
            crossterm::event::KeyCode::Char('i'),
            crossterm::event::KeyModifiers::empty(),
        );
        state.edit_code_key(
            crossterm::event::KeyCode::Char('X'),
            crossterm::event::KeyModifiers::empty(),
        );
        let content = state.focused_editor_content.clone();
        assert_eq!(
            content.matches('X').count(),
            3,
            "insert should hit every caret: {content}"
        );
        std::fs::remove_dir_all(root).expect("remove temporary workspace");
    }

    #[test]
    fn app_enter_records_role_name_locators_into_a_workflow_draft() {
        let root =
            std::env::temp_dir().join(format!("glass-workflow-record-{}", std::process::id()));
        std::fs::create_dir_all(&root).expect("create temporary workspace");
        let mut state =
            DevTuiState::open_for_tui(&root, TuiLayout::Desktop).expect("open temporary workspace");
        state.browser_workspace.replace_entities(
            7,
            vec![BrowserWorkspaceEntity {
                reference: "r7:save".into(),
                role: "button".into(),
                name: "Save settings".into(),
                actionable: true,
                revision: 7,
            }],
        );
        state
            .start_workflow_recording("Save Settings")
            .expect("start recording");
        state.queue_browser_intent(BrowserWorkspaceIntent::ActivateSelected);
        let draft = serde_json::to_string(
            state
                .workflow_recording
                .as_ref()
                .expect("recording")
                .recorder
                .draft(),
        )
        .expect("serialize draft");
        assert!(draft.contains("role=button;name=Save settings"));
        let stopped = state.stop_workflow_recording().expect("stop recording");
        assert!(stopped.contains("1 step"));
        let path = root.join(".glass/workflows/save-settings.draft.json");
        let persisted = std::fs::read_to_string(&path).expect("read draft");
        assert!(persisted.contains("role=button;name=Save settings"));
        assert!(persisted.contains("\"reviewRequired\": true"));
        std::fs::remove_dir_all(root).expect("remove temporary workspace");
    }

    #[test]
    fn snapshot_refresh_keeps_local_workflow_recording_status_authoritative() {
        let root = std::env::temp_dir().join(format!(
            "glass-workflow-record-status-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&root).expect("create temporary workspace");
        let mut state =
            DevTuiState::open_for_tui(&root, TuiLayout::Desktop).expect("open temporary workspace");
        state
            .start_workflow_recording("locators")
            .expect("start recording");
        state.apply_snapshot(&super::super::snapshot::DisplaySnapshot {
            workflow: "not recording".into(),
            ..Default::default()
        });

        assert!(state.workflow.starts_with("REC locators"));
        std::fs::remove_dir_all(root).expect("remove temporary workspace");
    }

    #[test]
    fn agent_prompt_context_does_not_attach_unselected_hidden_or_editor_files() {
        let root =
            std::env::temp_dir().join(format!("glass-agent-prompt-context-{}", std::process::id()));
        std::fs::create_dir_all(&root).expect("create temporary workspace");
        let hidden_path = root.join(".harmless-context-fixture");
        std::fs::write(&hidden_path, "fixture content must not enter a greeting").unwrap();
        let mut state =
            DevTuiState::open_for_tui(&root, TuiLayout::Desktop).expect("open workspace");
        state.surface = DevSurface::Git;
        state.focused_editor_path = ".harmless-context-fixture".into();

        let context = state.agent_prompt_context();
        let serialized = serde_json::to_string(&context).expect("serialize context");
        assert!(context.get("browser").is_some());
        assert!(context.get("authority").is_some());
        assert!(context.get("editor").is_none());
        assert!(context.get("project").is_none());
        assert!(context.get("playbook").is_none());
        assert!(context.get("playbookText").is_none());
        assert!(!serialized.contains(".harmless-context-fixture"));
        assert!(!serialized.contains("fixture content must not enter a greeting"));
        assert!(!serialized.contains("Git playbook:"));
        assert!(!serialized.contains("Agent playbook:"));

        std::fs::remove_dir_all(root).expect("remove temporary workspace");
    }

    #[test]
    fn code_context_is_visible_removable_and_selection_first() {
        let root =
            std::env::temp_dir().join(format!("glass-code-prompt-context-{}", std::process::id()));
        std::fs::create_dir_all(&root).expect("create temporary workspace");
        let mut state =
            DevTuiState::open_for_tui(&root, TuiLayout::Desktop).expect("open temporary workspace");
        state.show_surface(DevSurface::Code);
        state.focused_editor_path = "src/main.rs".into();
        state.focused_editor_content = "fn selected() {}\nfn hidden() {}\n".into();
        state.focused_editor_dirty = true;
        state.composer_code_context_enabled = true;
        state.focused_editor_selection = Some(TextSelection {
            anchor: crate::development::TextPosition { line: 1, column: 4 },
            active: crate::development::TextPosition {
                line: 1,
                column: 10,
            },
        });
        state.open_composer();

        assert!(
            state
                .composer_context_chips()
                .contains("src/main.rs (unsaved)")
        );
        let context = state.code_prompt_context().expect("Code context available");
        assert_eq!(context["path"], "src/main.rs");
        assert_eq!(context["language"], "Rust");
        assert_eq!(context["selectedOnly"], true);
        assert_eq!(context["content"], "select");

        state.toggle_composer_code_context();
        assert!(state.composer_context_chips().is_empty());
        state.show_surface(DevSurface::Agent);
        assert!(!state.composer_code_context_enabled);
        let agent_context = state.agent_prompt_context();
        let serialized = serde_json::to_string(&agent_context).expect("serialize context");
        assert!(!serialized.contains("src/main.rs"));
        assert!(!serialized.contains("fn hidden"));

        std::fs::remove_dir_all(root).expect("remove temporary workspace");
    }

    #[test]
    fn code_context_attaches_multiple_unsaved_buffers_and_selected_ranges() {
        let root = std::env::temp_dir().join(format!(
            "glass-multifile-code-context-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&root).expect("create temporary workspace");
        let mut state =
            DevTuiState::open_for_tui(&root, TuiLayout::Desktop).expect("open workspace");
        state.show_surface(DevSurface::Code);
        state.focused_editor_path = "src/api.rs".into();
        state.focused_editor_content = "fn public_api() {}\nfn hidden_api() {}\n".into();
        state.focused_editor_dirty = true;
        state.focused_editor_selection = Some(TextSelection {
            anchor: crate::development::TextPosition { line: 1, column: 4 },
            active: crate::development::TextPosition {
                line: 1,
                column: 14,
            },
        });
        state.attach_focused_code_context();

        state.focused_editor_path = "src/model.rs".into();
        state.focused_editor_content = "struct Selected {}\nstruct Hidden {}\n".into();
        state.focused_editor_dirty = true;
        state.focused_editor_selection = Some(TextSelection {
            anchor: crate::development::TextPosition { line: 1, column: 8 },
            active: crate::development::TextPosition {
                line: 1,
                column: 16,
            },
        });
        state.attach_focused_code_context();
        state.composer_code_context_enabled = false;

        let context = state.code_prompt_context().expect("attached Code context");
        let files = context["files"].as_array().expect("multifile context");
        assert_eq!(files.len(), 2);
        assert_eq!(files[0]["path"], "src/api.rs");
        assert_eq!(files[0]["content"], "public_api");
        assert_eq!(files[0]["selectedOnly"], true);
        assert_eq!(files[0]["unsaved"], true);
        assert_eq!(files[1]["path"], "src/model.rs");
        assert_eq!(files[1]["content"], "Selected");
        assert_eq!(files[1]["selectedOnly"], true);
        assert!(
            state
                .composer_context_chips()
                .contains("src/api.rs [range]")
        );
        assert!(
            state
                .composer_context_chips()
                .contains("src/model.rs [range]")
        );

        state.toggle_composer_code_context();
        assert_eq!(state.code_context_attachments.len(), 1);
        assert_eq!(state.code_context_attachments[0].path, "src/api.rs");
        let remaining = state.code_prompt_context().expect("remaining attachment");
        assert_eq!(remaining["path"], "src/api.rs");
        std::fs::remove_dir_all(root).expect("remove temporary workspace");
    }

    #[test]
    fn workflow_recording_is_available_on_first_open() {
        let root =
            std::env::temp_dir().join(format!("glass-workflow-first-open-{}", std::process::id()));
        std::fs::create_dir_all(&root).expect("create temporary workspace");
        let mut state =
            DevTuiState::open_for_tui(&root, TuiLayout::Desktop).expect("open workspace");
        state.browser_workspace.replace_entities(
            1,
            vec![BrowserWorkspaceEntity {
                reference: "r1:save".into(),
                role: "button".into(),
                name: "Save settings".into(),
                actionable: true,
                revision: 1,
            }],
        );
        state.browser_workspace.state_mut().selected_entity = Some(0);
        state
            .start_workflow_recording("retained")
            .expect("start recording");
        assert!(state.capture_workflow_click().expect("capture click"));
        assert!(state.stop_workflow_recording().is_ok());
        assert!(root.join(".glass/workflows/retained.draft.json").exists());
        std::fs::remove_dir_all(root).expect("remove temporary workspace");
    }

    #[test]
    fn editor_soft_wrap_toggle_scrolls_visual_rows_and_resets_horizontal_scroll() {
        let root =
            std::env::temp_dir().join(format!("glass-editor-soft-wrap-{}", std::process::id()));
        std::fs::create_dir_all(root.join("src")).expect("create source directory");
        let content = format!("{}\n", "word ".repeat(80));
        std::fs::write(root.join("src/main.rs"), content).expect("write source");
        let mut state =
            DevTuiState::open_for_tui(&root, TuiLayout::Mobile).expect("open temporary workspace");
        state
            .ws_mut()
            .expect("workspace lock")
            .project_mut()
            .open_buffer("src/main.rs", crate::development::Actor::local())
            .expect("open editor buffer");
        state
            .ws_mut()
            .expect("workspace lock")
            .project_mut()
            .set_buffer_cursor("src/main.rs", 1, 180)
            .expect("set editor cursor");
        state.refresh_editor_projection();
        state.set_terminal_size(32, 16);
        state.enter_code_edit();
        state.edit_code_key(
            crossterm::event::KeyCode::Char('w'),
            crossterm::event::KeyModifiers::ALT,
        );

        assert!(state.editor_soft_wrap);
        assert!(state.editor_scroll_line > 0);
        assert_eq!(state.editor_scroll_column, 0);
        assert!(state.status.contains("Soft wrap ON"));

        state.edit_code_key(
            crossterm::event::KeyCode::Char('w'),
            crossterm::event::KeyModifiers::ALT,
        );
        assert!(!state.editor_soft_wrap);
        assert!(state.editor_scroll_column > 0);
        assert!(state.status.contains("Soft wrap OFF"));
        std::fs::remove_dir_all(root).expect("remove temporary workspace");
    }

    #[test]
    fn editor_cursor_scroll_matches_fullscreen_source_height() {
        let root =
            std::env::temp_dir().join(format!("glass-editor-viewport-{}", std::process::id()));
        std::fs::create_dir_all(root.join("src")).expect("create source directory");
        let content = (1..=30)
            .map(|line| format!("line {line}\n"))
            .collect::<String>();
        std::fs::write(root.join("src/main.rs"), content).expect("write source");
        let mut state =
            DevTuiState::open_for_tui(&root, TuiLayout::Desktop).expect("open temporary workspace");
        state
            .ws_mut()
            .expect("workspace lock")
            .project_mut()
            .open_buffer("src/main.rs", crate::development::Actor::local())
            .expect("open editor buffer");
        state
            .ws_mut()
            .expect("workspace lock")
            .project_mut()
            .set_buffer_cursor("src/main.rs", 23, 1)
            .expect("set editor cursor");
        state.refresh_editor_projection();
        state.editor_scroll_line = 0;
        state.set_terminal_size(100, 30);

        assert_eq!(state.focused_editor_line, 23);
        assert_eq!(
            state.editor_scroll_line, 1,
            "line 23 must scroll into the 22-row source viewport"
        );
        std::fs::remove_dir_all(root).expect("remove temporary workspace");
    }

    #[test]
    fn editor_shift_selection_replaces_text_and_clears_anchor() {
        let root =
            std::env::temp_dir().join(format!("glass-editor-selection-{}", std::process::id()));
        std::fs::create_dir_all(root.join("src")).expect("create source directory");
        std::fs::write(root.join("src/main.rs"), "alpha\nbeta\n").expect("write source");
        let mut state =
            DevTuiState::open_for_tui(&root, TuiLayout::Desktop).expect("open temporary workspace");
        state
            .ws_mut()
            .expect("workspace lock")
            .project_mut()
            .open_buffer("src/main.rs", crate::development::Actor::local())
            .expect("open editor buffer");
        state.refresh_editor_projection();
        state.enter_code_edit();
        for _ in 0..5 {
            state.edit_code_key(
                crossterm::event::KeyCode::Right,
                crossterm::event::KeyModifiers::SHIFT,
            );
        }
        assert!(
            state
                .focused_editor_selection
                .as_ref()
                .is_some_and(|selection| !selection.is_empty())
        );
        state.edit_code_key(
            crossterm::event::KeyCode::Char('X'),
            crossterm::event::KeyModifiers::empty(),
        );
        let buffer = state.focused_buffer().expect("focused editor buffer");
        assert_eq!(buffer.content, "X\nbeta\n");
        assert_eq!((buffer.cursor_line, buffer.cursor_column), (1, 2));
        assert!(buffer.selection.is_none());
        std::fs::remove_dir_all(root).expect("remove temporary workspace");
    }

    #[test]
    fn focused_editor_projection_survives_temporary_workspace_lock_contention() {
        let root = std::env::temp_dir().join(format!(
            "glass-editor-busy-projection-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(root.join("src")).expect("create source directory");
        std::fs::write(root.join("src/main.rs"), "fn main() {}\n").expect("write source");
        let mut state =
            DevTuiState::open_for_tui(&root, TuiLayout::Desktop).expect("open temporary workspace");
        state
            .ws_mut()
            .expect("workspace lock")
            .project_mut()
            .open_buffer("src/main.rs", crate::development::Actor::local())
            .expect("open editor buffer");
        state.refresh_editor_projection();

        let workspace = state.ws().expect("hold workspace lock");
        let buffer = state
            .focused_buffer()
            .expect("cached editor projection while workspace is busy");
        assert_eq!(buffer.path, "src/main.rs");
        assert_eq!(buffer.content, "fn main() {}\n");
        drop(workspace);
        std::fs::remove_dir_all(root).expect("remove temporary workspace");
    }

    #[test]
    fn editor_keys_and_paste_wait_for_workspace_and_apply_in_order() {
        let root =
            std::env::temp_dir().join(format!("glass-editor-queued-keys-{}", std::process::id()));
        std::fs::create_dir_all(root.join("src")).expect("create source directory");
        std::fs::write(root.join("src/main.rs"), "main\n").expect("write source");
        let mut state =
            DevTuiState::open_for_tui(&root, TuiLayout::Desktop).expect("open temporary workspace");
        state
            .ws_mut()
            .expect("workspace lock")
            .project_mut()
            .open_buffer("src/main.rs", crate::development::Actor::local())
            .expect("open editor buffer");
        state.refresh_editor_projection();
        state.enter_code_edit();

        let workspace_handle = state.workspace.clone();
        let workspace_guard = workspace_handle.try_lock().expect("hold workspace lock");
        state.edit_code_key(
            crossterm::event::KeyCode::Char('x'),
            crossterm::event::KeyModifiers::empty(),
        );
        state.edit_code_key(
            crossterm::event::KeyCode::Char('y'),
            crossterm::event::KeyModifiers::empty(),
        );
        state.paste_editor_text("z\n");
        assert_eq!(state.focused_editor_content, "main\n");
        assert!(state.status.contains("3 events waiting"));

        drop(workspace_guard);
        assert!(state.flush_pending_editor_input());
        assert_eq!(state.focused_editor_content, "xyz\nmain\n");
        assert!(state.pending_editor_input.is_empty());
        assert_eq!(
            state.focused_buffer().expect("updated buffer").content,
            "xyz\nmain\n"
        );
        std::fs::remove_dir_all(root).expect("remove temporary workspace");
    }

    #[test]
    fn editor_exit_prompts_save_discard_and_discard_quit() {
        let root = std::env::temp_dir().join(format!("glass-editor-exit-{}", std::process::id()));
        std::fs::create_dir_all(root.join("src")).expect("create source directory");
        let original = "fn main() {}\n";
        std::fs::write(root.join("src/main.rs"), original).expect("write source");
        let mut state =
            DevTuiState::open_for_tui(&root, TuiLayout::Desktop).expect("open temporary workspace");
        state
            .ws_mut()
            .expect("workspace lock")
            .project_mut()
            .open_buffer("src/main.rs", crate::development::Actor::local())
            .expect("open editor buffer");
        state.refresh_editor_projection();
        state.enter_code_edit();
        state.edit_code_key(
            crossterm::event::KeyCode::Char('#'),
            crossterm::event::KeyModifiers::NONE,
        );
        state.request_editor_exit();
        assert_eq!(state.editor_exit_prompt, Some(EditorExitPrompt::Unsaved));
        state.handle_editor_exit_key(crossterm::event::KeyCode::Char('d'));
        assert!(!state.code_edit_mode);
        assert_eq!(state.focused_editor_content, original);
        assert!(!state.focused_editor_dirty);
        assert_eq!(
            std::fs::read_to_string(root.join("src/main.rs")).expect("read source"),
            original
        );
        let reopened = state
            .ws_mut()
            .expect("workspace lock")
            .project_mut()
            .open_buffer("src/main.rs", crate::development::Actor::local())
            .expect("reopen discarded editor buffer");
        assert_eq!(reopened.content, original);
        assert!(!reopened.dirty);
        state.refresh_editor_projection();
        assert_eq!(state.focused_editor_content, original);

        state.enter_code_edit();
        state.edit_code_key(
            crossterm::event::KeyCode::Char('#'),
            crossterm::event::KeyModifiers::NONE,
        );
        state.request_editor_exit();
        state.handle_editor_exit_key(crossterm::event::KeyCode::Char('s'));
        assert!(!state.code_edit_mode);
        assert!(
            std::fs::read_to_string(root.join("src/main.rs"))
                .expect("read saved source")
                .starts_with('#')
        );
        let saved = std::fs::read_to_string(root.join("src/main.rs")).expect("read saved source");

        state.enter_code_edit();
        state.edit_code_key(
            crossterm::event::KeyCode::Char('!'),
            crossterm::event::KeyModifiers::NONE,
        );
        let unsaved = state.focused_editor_content.clone();
        state.request_quit();
        assert_eq!(state.editor_exit_prompt, Some(EditorExitPrompt::Unsaved));
        assert!(!state.quit_confirmation);
        state.handle_editor_exit_key(crossterm::event::KeyCode::Char('q'));
        assert!(state.quit_confirmation);
        assert!(!state.quit);
        assert!(state.code_edit_mode);
        assert_eq!(state.editor_exit_prompt, Some(EditorExitPrompt::Unsaved));
        assert_eq!(
            std::fs::read_to_string(root.join("src/main.rs")).expect("read discarded source"),
            saved
        );
        assert_eq!(
            state
                .ws()
                .expect("workspace lock")
                .project()
                .buffer("src/main.rs")
                .expect("preserved unsaved buffer")
                .content,
            unsaved
        );
        state.cancel_quit();
        assert!(!state.quit_confirmation);
        assert_eq!(state.editor_exit_prompt, Some(EditorExitPrompt::Unsaved));
        assert!(state.focused_editor_dirty);
        state.handle_editor_exit_key(crossterm::event::KeyCode::Char('q'));
        state.confirm_quit();
        assert!(state.quit);
        std::fs::remove_dir_all(root).expect("remove temporary workspace");
    }

    #[test]
    fn late_chat_result_stays_with_originating_agent_thread() {
        let (mut state, root) = routed_state("thread-isolation-late-result");
        let agent_a = crate::AgentId::parse("agent-0001").unwrap();
        let agent_b = crate::AgentId::parse("agent-0002").unwrap();
        let entry = |id: &str, text: &str| super::super::projection::ConversationEntry {
            kind: super::super::projection::ConversationKind::User,
            text: text.into(),
            streaming: false,
            entry_id: Some(format!("entry-{id}")),
            client_message_id: Some(format!("chat-{id}")),
            tool_name: None,
            tool_card: None,
            core_ui: None,
        };
        state.agent_conversations.insert(
            agent_a.clone(),
            super::super::snapshot::AgentConversation {
                text: "transcript A".into(),
                items: vec![entry("a", "message A")],
                reasoning: String::new(),
                ..Default::default()
            },
        );
        state.agent_conversations.insert(
            agent_b.clone(),
            super::super::snapshot::AgentConversation {
                text: "transcript B".into(),
                items: vec![entry("b", "message B")],
                reasoning: String::new(),
                ..Default::default()
            },
        );
        assert!(state.select_agent(Some(agent_a.clone())));
        assert_eq!(state.agent_conversation, "transcript A");
        state.composer_input = "draft alpha".into();
        state.composer_cursor = 5;
        #[cfg(unix)]
        assert!(state.persist_composer_draft());
        assert!(state.select_agent(Some(agent_b.clone())));
        assert_eq!(state.agent_conversation, "transcript B");
        assert!(state.composer_input.is_empty());
        state.composer_input = "draft bravo".into();
        state.composer_cursor = 6;
        #[cfg(unix)]
        assert!(state.persist_composer_draft());
        state.composer_mode = true;
        state.surface = DevSurface::Agent;
        state.status = "B remains focused".into();
        state.pending_chat_messages.push(PendingChatMessage {
            agent_id: Some(agent_a.clone()),
            message_id: "chat-late-a".into(),
            text: "message sent in A".into(),
            state: ChatMessageState::Sending,
            job_id: Some(71),
            error: None,
        });

        state.apply_tool_job_result(super::super::snapshot::ToolJobResult {
            id: 71,
            tool: "glass.agent.send".into(),
            result: Ok(serde_json::json!({"agentId": agent_a.as_str()})),
        });

        assert_eq!(state.selected_agent, Some(agent_b.clone()));
        assert_eq!(state.agent_conversation, "transcript B");
        assert_eq!(state.composer_input, "draft bravo");
        assert_eq!(state.composer_cursor, 6);
        assert_eq!(state.status, "B remains focused");
        assert!(state.composer_mode);
        assert_eq!(state.surface, DevSurface::Agent);
        assert_eq!(
            state.pending_chat_messages[0].agent_id,
            Some(agent_a.clone())
        );
        assert_eq!(state.pending_chat_messages[0].state, ChatMessageState::Sent);
        state.select_agent(Some(agent_a));
        assert_eq!(state.agent_conversation, "transcript A");
        assert_eq!(state.composer_input, "draft alpha");
        assert_eq!(state.composer_cursor, 5);
        assert!(state.conversation_view().contains("message sent in A"));

        std::fs::remove_dir_all(root).expect("remove temporary workspace");
    }

    #[test]
    fn late_chat_failure_is_recoverable_only_in_its_originating_thread() {
        let (mut state, root) = routed_state("thread-isolation-late-failure");
        let agent_a = crate::AgentId::parse("agent-0001").unwrap();
        let agent_b = crate::AgentId::parse("agent-0002").unwrap();
        state.agent_conversations.insert(
            agent_a.clone(),
            super::super::snapshot::AgentConversation {
                text: "transcript A".into(),
                items: Vec::new(),
                reasoning: String::new(),
                ..Default::default()
            },
        );
        state.agent_conversations.insert(
            agent_b.clone(),
            super::super::snapshot::AgentConversation {
                text: "transcript B".into(),
                items: Vec::new(),
                reasoning: String::new(),
                ..Default::default()
            },
        );
        state.select_agent(Some(agent_a.clone()));
        state.select_agent(Some(agent_b.clone()));
        state.composer_input = "B draft".into();
        state.composer_cursor = 2;
        state.composer_mode = true;
        state.surface = DevSurface::Agent;
        state.status = "B stays focused".into();
        state.pending_chat_messages.push(PendingChatMessage {
            agent_id: Some(agent_a.clone()),
            message_id: "chat-failed-a".into(),
            text: "recover this A message".into(),
            state: ChatMessageState::Sending,
            job_id: Some(74),
            error: None,
        });
        state.apply_tool_job_result(super::super::snapshot::ToolJobResult {
            id: 74,
            tool: "glass.agent.send".into(),
            result: Err("transport closed".into()),
        });
        assert_eq!(state.selected_agent, Some(agent_b));
        assert_eq!(state.composer_input, "B draft");
        assert_eq!(state.composer_cursor, 2);
        assert_eq!(state.status, "B stays focused");
        assert!(state.composer_mode);
        assert_eq!(state.surface, DevSurface::Agent);
        state.select_agent(Some(agent_a));
        assert_eq!(state.composer_input, "recover this A message");
        assert_eq!(state.composer_cursor, "recover this A message".len());
        assert!(state.conversation_view().contains("send failed"));
        std::fs::remove_dir_all(root).expect("remove temporary workspace");
    }

    #[test]
    fn failed_send_error_is_visible_on_its_existing_transcript_row() {
        let (mut state, root) = routed_state("failed-send-visible-error");
        let agent_id = crate::AgentId::parse("agent-0001").unwrap();
        let message_id = "chat-failed-visible";
        let error =
            "development conflict: selected thread is recovery pending and has no command channel";
        state.selected_agent = Some(agent_id);
        state.conversation_items = vec![super::super::projection::ConversationEntry {
            kind: super::super::projection::ConversationKind::User,
            text: "hello".into(),
            streaming: false,
            entry_id: Some("entry-user".into()),
            client_message_id: Some(message_id.into()),
            tool_name: None,
            tool_card: None,
            core_ui: None,
        }];
        state.pending_chat_messages.push(PendingChatMessage {
            agent_id: state.selected_agent.clone(),
            message_id: message_id.into(),
            text: "hello".into(),
            state: ChatMessageState::Failed,
            job_id: None,
            error: Some(error.into()),
        });

        let entries = state.conversation_entries_view();
        assert_eq!(
            entries.len(),
            1,
            "the same client message must not duplicate"
        );
        assert!(entries[0].text.contains(error));
        assert!(entries[0].text.contains("Enter retries this message"));
        std::fs::remove_dir_all(root).expect("remove temporary workspace");
    }

    #[test]
    fn default_composer_target_skips_threads_that_need_recovery() {
        assert!(is_default_composer_agent(crate::AgentStatus::Starting));
        assert!(is_default_composer_agent(crate::AgentStatus::Idle));
        assert!(is_default_composer_agent(crate::AgentStatus::Working));
        assert!(is_default_composer_agent(crate::AgentStatus::Waiting));
        assert!(!is_default_composer_agent(
            crate::AgentStatus::RecoveryPending
        ));
        assert!(!is_default_composer_agent(crate::AgentStatus::Unavailable));
        assert!(!is_default_composer_agent(crate::AgentStatus::Completed));
    }

    #[test]
    fn snapshot_does_not_implicitly_select_a_recovery_pending_thread() {
        let (mut state, root) = routed_state("snapshot-does-not-select-recovered-thread");
        let recovered_id = crate::AgentId::parse("agent-0001").unwrap();
        let mut snapshot = super::super::snapshot::DisplaySnapshot::default();
        snapshot
            .agent_states
            .push((recovered_id.clone(), crate::AgentStatus::RecoveryPending));
        snapshot
            .agent_chrome
            .push(super::super::snapshot::AgentChrome {
                id: recovered_id,
                status: crate::AgentStatus::RecoveryPending,
                model: None,
                thinking: None,
                session_file: None,
            });

        state.apply_snapshot(&snapshot);

        assert_eq!(state.selected_agent, None);
        assert_eq!(state.selected_agent_status(), None);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn lifecycle_status_does_not_claim_idle_means_response() {
        let (mut state, root) = routed_state("agent-lifecycle-status-cache");
        let agent_id = crate::AgentId::parse("agent-0001").unwrap();
        state.selected_agent = Some(agent_id.clone());

        let mut snapshot = super::super::snapshot::DisplaySnapshot {
            agents: "agent snapshot".into(),
            ..Default::default()
        };
        snapshot.agent_conversations.insert(
            agent_id.clone(),
            super::super::snapshot::AgentConversation {
                text: "Core prompt failed: -32000".into(),
                ..Default::default()
            },
        );
        snapshot
            .agent_states
            .push((agent_id.clone(), crate::AgentStatus::Idle));
        snapshot
            .agent_chrome
            .push(super::super::snapshot::AgentChrome {
                id: agent_id.clone(),
                status: crate::AgentStatus::Idle,
                model: None,
                thinking: None,
                session_file: None,
            });
        state.status = "Sent · Glass Core is thinking…".into();
        state.apply_snapshot(&snapshot);
        assert_eq!(state.status, "Glass Agent idle");
        assert!(state.agent_conversation.contains("-32000"));

        // Rendering must keep using the last published status during workspace
        // lock contention instead of alternating to global Core availability.
        let _workspace_guard = state.workspace.lock().unwrap();
        assert_eq!(
            state.selected_agent_status(),
            Some(crate::AgentStatus::Idle)
        );
        drop(_workspace_guard);

        snapshot.agent_states[0].1 = crate::AgentStatus::Waiting;
        snapshot.agent_chrome[0].status = crate::AgentStatus::Waiting;
        state.status = "Sent to agent · Glass Agent is thinking…".into();
        state.apply_snapshot(&snapshot);
        assert_eq!(state.status, "Glass Agent waiting");
        assert!(!state.status.contains("approval"));

        snapshot.agent_states[0].1 = crate::AgentStatus::Idle;
        snapshot.agent_chrome[0].status = crate::AgentStatus::Idle;
        state.status = "Glass Agent waiting for approval".into();
        state.apply_snapshot(&snapshot);
        assert_eq!(state.status, "Glass Agent idle");

        state.status = "GitHub review refreshed".into();
        state.apply_snapshot(&snapshot);
        assert_eq!(state.status, "GitHub review refreshed");
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn restored_failed_draft_reuses_its_identity_until_edited() {
        let (mut state, root) = routed_state("failed-send-retry-identity");
        let agent_id = crate::AgentId::parse("agent-0001").unwrap();
        let message_id = "chat-failed-retry";
        state.selected_agent = Some(agent_id.clone());
        state.pending_chat_messages.push(PendingChatMessage {
            agent_id: Some(agent_id),
            message_id: message_id.into(),
            text: "hello".into(),
            state: ChatMessageState::Failed,
            job_id: None,
            error: Some("development conflict".into()),
        });
        state.composer_retry_message_id = Some(message_id.into());
        state.composer_input = "hello".into();
        state.composer_cursor = state.composer_input.len();

        assert_eq!(
            state.matching_composer_retry_message_id("hello").as_deref(),
            Some(message_id)
        );
        state.set_composer_edit("hello again".into(), 11);
        assert!(state.composer_retry_message_id.is_none());
        assert!(
            state
                .matching_composer_retry_message_id("hello again")
                .is_none()
        );
        std::fs::remove_dir_all(root).expect("remove temporary workspace");
    }

    #[test]
    fn bootstrap_chat_result_attaches_to_created_agent_without_stealing_selection() {
        let (mut state, root) = routed_state("thread-isolation-bootstrap");
        let agent_b = crate::AgentId::parse("agent-0002").unwrap();
        let agent_a = crate::AgentId::parse("agent-0001").unwrap();
        state.select_agent(Some(agent_b.clone()));
        state.composer_input = "B draft".into();
        state.composer_cursor = 2;
        state.status = "B focused".into();
        state.pending_chat_messages.push(PendingChatMessage {
            agent_id: None,
            message_id: "chat-bootstrap".into(),
            text: "first message".into(),
            state: ChatMessageState::Sending,
            job_id: Some(72),
            error: None,
        });
        state.apply_tool_job_result(super::super::snapshot::ToolJobResult {
            id: 72,
            tool: "glass.agent.send".into(),
            result: Ok(serde_json::json!({"agentId": agent_a.as_str()})),
        });
        assert_eq!(
            state.pending_chat_messages[0].agent_id,
            Some(agent_a.clone())
        );
        assert_eq!(state.selected_agent, Some(agent_b));
        assert_eq!(state.composer_input, "B draft");
        assert_eq!(state.composer_cursor, 2);
        assert_eq!(state.status, "B focused");
        state.select_agent(Some(agent_a));
        assert!(state.conversation_view().contains("first message"));
        std::fs::remove_dir_all(root).expect("remove temporary workspace");
    }

    #[test]
    fn conversation_view_does_not_echo_pending_message_after_snapshot() {
        let root = std::env::temp_dir().join(format!("glass-chat-merge-{}", std::process::id()));
        std::fs::create_dir_all(&root).expect("create temporary workspace");
        let mut state =
            DevTuiState::open_for_tui(&root, TuiLayout::Desktop).expect("open temporary workspace");
        state.agent_conversation = "YOU\ninspect the failing test\n\nGLASS AGENT\nworking".into();
        state.pending_chat_messages.push(PendingChatMessage {
            agent_id: None,
            message_id: "chat-inspect-1".into(),
            text: "inspect the failing test".into(),
            state: ChatMessageState::Sent,
            job_id: None,
            error: None,
        });
        state.conversation_items = vec![super::super::projection::ConversationEntry {
            kind: super::super::projection::ConversationKind::User,
            text: "inspect the failing test".into(),
            streaming: false,
            entry_id: Some("entry-inspect-1".into()),
            client_message_id: Some("chat-inspect-1".into()),
            tool_name: None,
            tool_card: None,
            core_ui: None,
        }];

        let view = state.conversation_view();
        assert_eq!(view.matches("YOU\ninspect the failing test").count(), 1);
        assert!(!view.contains("Glass Agent is thinking"));

        std::fs::remove_dir_all(root).expect("remove temporary workspace");
    }

    #[test]
    fn conversation_entries_borrow_cached_history_without_pending_messages() {
        let root = std::env::temp_dir().join(format!("glass-chat-cow-{}", std::process::id()));
        std::fs::create_dir_all(&root).expect("create temporary workspace");
        let mut state =
            DevTuiState::open_for_tui(&root, TuiLayout::Desktop).expect("open temporary workspace");
        state.conversation_items = vec![super::super::projection::ConversationEntry {
            kind: super::super::projection::ConversationKind::Assistant,
            text: "cached answer".into(),
            streaming: false,
            entry_id: Some("entry-cow-1".into()),
            client_message_id: None,
            tool_name: None,
            tool_card: None,
            core_ui: None,
        }];

        let view = state.conversation_entries_cow();
        assert!(matches!(view, Cow::Borrowed(_)));
        assert_eq!(view[0].text, "cached answer");

        std::fs::remove_dir_all(root).expect("remove temporary workspace");
    }

    #[test]
    fn pending_chat_reconciliation_uses_message_identity_for_duplicate_text() {
        let root = std::env::temp_dir().join(format!("glass-chat-ids-{}", std::process::id()));
        std::fs::create_dir_all(&root).expect("create temporary workspace");
        let mut state =
            DevTuiState::open_for_tui(&root, TuiLayout::Desktop).expect("open workspace");
        state.agent_conversation = "YOU\nfix the same test".into();
        let pending = |message_id: &str| PendingChatMessage {
            agent_id: None,
            message_id: message_id.into(),
            text: "fix the same test".into(),
            state: ChatMessageState::Sent,
            job_id: None,
            error: None,
        };
        state.pending_chat_messages = vec![pending("chat-1"), pending("chat-2")];
        let user_entry = |message_id: &str| super::super::projection::ConversationEntry {
            kind: super::super::projection::ConversationKind::User,
            text: "fix the same test".into(),
            streaming: false,
            entry_id: Some(format!("entry-{message_id}")),
            client_message_id: Some(message_id.into()),
            tool_name: None,
            tool_card: None,
            core_ui: None,
        };
        state.conversation_items = vec![user_entry("chat-1")];

        state.reconcile_pending_chat();
        assert_eq!(state.pending_chat_messages.len(), 1);
        assert_eq!(state.pending_chat_messages[0].message_id, "chat-2");
        assert_eq!(
            state
                .conversation_view()
                .matches("YOU\nfix the same test")
                .count(),
            2,
            "the confirmed first message and distinct pending duplicate both remain visible"
        );

        state.conversation_items.push(user_entry("chat-2"));
        state.agent_conversation = "YOU\nfix the same test\n\nYOU\nfix the same test".into();
        state.reconcile_pending_chat();
        assert!(state.pending_chat_messages.is_empty());
        assert_eq!(
            state
                .conversation_view()
                .matches("YOU\nfix the same test")
                .count(),
            2,
            "two confirmed messages with the same text remain visible"
        );
        assert_eq!(
            state
                .conversation_entries_view()
                .iter()
                .filter(|entry| entry.kind == super::super::projection::ConversationKind::User)
                .count(),
            2
        );
        std::fs::remove_dir_all(root).expect("remove temporary workspace");
    }

    #[test]
    fn arrow_surface_cycle_matches_layout_order() {
        let root =
            std::env::temp_dir().join(format!("glass-surface-arrows-{}", std::process::id()));
        std::fs::create_dir_all(&root).expect("create temporary workspace");
        let mut desktop =
            DevTuiState::open_for_tui(&root, TuiLayout::Desktop).expect("open desktop workspace");
        desktop.surface = DevSurface::Agent;
        desktop.next_surface();
        assert_eq!(desktop.surface, DevSurface::Code);
        assert_eq!(desktop.status, "Code selected");
        desktop.previous_surface();
        assert_eq!(desktop.surface, DevSurface::Agent);
        desktop.surface = DevSurface::Git;
        desktop.next_surface();
        assert_eq!(desktop.surface, DevSurface::Agent);
        desktop.next_surface();
        assert_eq!(desktop.surface, DevSurface::Code);

        let mut phone =
            DevTuiState::open_for_tui(&root, TuiLayout::Mobile).expect("open phone workspace");
        phone.surface = DevSurface::Agent;
        phone.previous_surface();
        assert_eq!(phone.surface, DevSurface::Git);
        assert_eq!(phone.status, "Git selected");
        phone.next_surface();
        assert_eq!(phone.surface, DevSurface::Agent);

        std::fs::remove_dir_all(root).expect("remove temporary workspace");
    }

    #[test]
    #[cfg(unix)]
    fn delayed_queue_save_failure_does_not_clear_another_owner_dispatch() {
        let owner_a = crate::AgentId::parse("agent-0001").unwrap();
        let owner_b = crate::AgentId::parse("agent-0002").unwrap();
        let mut pending = Some((owner_b.clone(), "queue-b".to_owned(), 7));

        assert!(!clear_pending_queue_dispatch_if_matches(
            &mut pending,
            &owner_a,
            3
        ));
        assert_eq!(pending, Some((owner_b.clone(), "queue-b".to_owned(), 7)));
        assert!(clear_pending_queue_dispatch_if_matches(
            &mut pending,
            &owner_b,
            7
        ));
        assert!(pending.is_none());
    }

    #[test]
    fn unresolved_queue_row_requires_duplicate_risk_confirmation_before_dispatch() {
        let (mut state, root) = routed_state("queue-needs-review-confirmation");
        let agent_id = crate::AgentId::parse("agent-0001").unwrap();
        let run_id = crate::thread_store::random_id("run").unwrap();
        let entry_id = crate::thread_store::random_id("queue").unwrap();
        state.selected_agent = Some(agent_id.clone());
        state.follow_up_queue_snapshots.insert(
            agent_id,
            FollowUpQueueSnapshot {
                thread_id: "thread-0001".into(),
                current_run_id: run_id.clone(),
                send_attempts: Vec::new(),
                entries: vec![crate::thread_store::FollowUpQueueRecord {
                    entry_id: entry_id.clone(),
                    target_run_id: run_id,
                    mode: crate::thread_store::FollowUpQueueMode::FollowUp,
                    text: "Check this result".into(),
                    state: crate::thread_store::FollowUpQueueState::NeedsReview,
                }],
            },
        );
        let mut worker = super::super::snapshot::SnapshotWorker::spawn(&state);

        state.dispatch_selected_follow_up_queue_entry(&mut worker);

        assert_eq!(state.follow_up_queue_retry_confirmation, Some(entry_id));
        assert!(state.follow_up_queue_pending_dispatch.is_none());
        assert!(state.follow_up_queue_dispatch_jobs.is_empty());
        assert!(state.status.contains("may duplicate"));
        drop(worker);
        drop(state);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn agent_thinking_does_not_block_independent_browser_observation() {
        let (mut state, root) = routed_state("observe-during-agent-send");
        state.agent_send_job = Some(42);
        assert!(state.background_action_running());
        assert!(!state.blocking_background_tool_running());
        assert!(!state.composer_send_blocked());

        state.running_tool_job = Some(43);
        assert!(state.blocking_background_tool_running());
        assert!(state.composer_send_blocked());
        std::fs::remove_dir_all(root).expect("remove temporary workspace");
    }

    #[test]
    fn core_login_waits_for_background_agent_action() {
        let (mut state, root) = routed_state("core-login-background-action");
        state.agent_send_job = Some(42);
        let error = state
            .request_agent_login()
            .expect_err("Core handoff cannot suspend a running background action");
        assert!(error.contains("Finish the current background action"));
        assert!(!state.agent_login_requested);
        std::fs::remove_dir_all(root).expect("remove temporary workspace");
    }

    #[test]
    fn follow_up_results_reconcile_by_each_job_id_out_of_order() {
        let (mut state, root) = routed_state("follow-up-result-order");
        state.agent_send_job = Some(12);
        state.pending_chat_messages = vec![
            PendingChatMessage {
                agent_id: None,
                message_id: "first".into(),
                text: "first message".into(),
                state: ChatMessageState::Sending,
                job_id: Some(11),
                error: None,
            },
            PendingChatMessage {
                agent_id: None,
                message_id: "second".into(),
                text: "second message".into(),
                state: ChatMessageState::Sending,
                job_id: Some(12),
                error: None,
            },
        ];

        state.apply_tool_job_result(super::super::snapshot::ToolJobResult {
            id: 11,
            tool: "glass.agent.send".into(),
            result: Ok(serde_json::json!({"agentId":"agent:first"})),
        });

        assert_eq!(state.pending_chat_messages[0].state, ChatMessageState::Sent);
        assert_eq!(state.pending_chat_messages[0].job_id, None);
        assert_eq!(state.agent_send_job, Some(12));
        state.apply_tool_job_result(super::super::snapshot::ToolJobResult {
            id: 12,
            tool: "glass.agent.send".into(),
            result: Err("provider unavailable".into()),
        });
        assert_eq!(
            state.pending_chat_messages[1].state,
            ChatMessageState::Failed
        );
        assert_eq!(
            state.pending_chat_messages[1].error.as_deref(),
            Some("provider unavailable")
        );
        assert_eq!(state.agent_send_job, None);
        std::fs::remove_dir_all(root).expect("remove temporary workspace");
    }

    #[test]
    fn core_coding_tools_are_exposed_without_a_glass_permission_mode() {
        let root =
            std::env::temp_dir().join(format!("glass-core-permissions-{}", std::process::id()));
        std::fs::create_dir_all(&root).expect("create temporary workspace");
        let state =
            DevTuiState::open_for_tui(&root, TuiLayout::Desktop).expect("open temporary workspace");
        assert!(
            state
                .ws()
                .expect("workspace lock")
                .tool_descriptors()
                .iter()
                .any(|descriptor| descriptor.name == "glass.file.write" && descriptor.available)
        );
        std::fs::remove_dir_all(root).expect("remove temporary workspace");
    }

    #[test]
    fn transcript_fork_uses_selected_entry_id() {
        let root = std::env::temp_dir().join(format!("glass-fork-{}", std::process::id()));
        std::fs::create_dir_all(&root).expect("create temporary workspace");
        let mut state =
            DevTuiState::open_for_tui(&root, TuiLayout::Desktop).expect("open temporary workspace");
        state.conversation_items = vec![super::super::projection::ConversationEntry {
            kind: super::super::projection::ConversationKind::User,
            text: "ship the review".into(),
            streaming: false,
            entry_id: Some("entry-7".into()),
            client_message_id: None,
            tool_name: None,
            tool_card: None,
            core_ui: None,
        }];
        state.transcript_selection = 0;
        let mut worker = super::super::snapshot::SnapshotWorker::spawn(&state);
        state.fork_selected_transcript(&mut worker);
        assert!(
            state.status.contains("entry-7")
                || state.status.contains("glass.agent.fork")
                || state.status.contains("branch")
        );
        drop(worker);
        std::fs::remove_dir_all(root).expect("remove temporary workspace");
    }

    #[test]
    fn watching_agent_browser_act_enables_live_view() {
        let root = std::env::temp_dir().join(format!("glass-watch-{}", std::process::id()));
        std::fs::create_dir_all(&root).expect("create temporary workspace");
        let mut state =
            DevTuiState::open_for_tui(&root, TuiLayout::Desktop).expect("open temporary workspace");
        state.watch_agent_on_app(
            "glass.browser.act",
            &serde_json::json!({"target": "Sign in"}),
        );
        assert!(state.browser_visual_live);
        assert!(state.status.contains("Sign in"));
        assert!(state.status.contains("watching"));
        std::fs::remove_dir_all(root).expect("remove temporary workspace");
    }

    #[test]
    fn code_preview_queues_when_workspace_is_busy() {
        let root = std::env::temp_dir().join(format!("glass-open-busy-{}", std::process::id()));
        std::fs::create_dir_all(&root).expect("create temporary workspace");
        std::fs::write(root.join("notes.txt"), "hello\n").expect("write preview file");
        let mut state =
            DevTuiState::open_for_tui(&root, TuiLayout::Desktop).expect("open temporary workspace");
        state.files = vec!["notes.txt".into()];
        let workspace = state.workspace.clone();
        let _guard = workspace.lock().expect("hold workspace lock");

        state.show_surface(DevSurface::Code);
        assert_eq!(state.pending_open_file.as_deref(), Some("notes.txt"));
        assert!(state.status.contains("when the workspace is free"));
        assert!(
            !state.status.contains("Open failed"),
            "busy preview must queue instead of failing: {}",
            state.status
        );

        drop(_guard);
        state.flush_pending_open_file();
        assert!(state.pending_open_file.is_none());
        assert_eq!(state.focused_editor_path, "notes.txt");
        std::fs::remove_dir_all(root).expect("remove temporary workspace");
    }

    #[test]
    fn more_doctor_stays_on_more_and_git_loads_the_selected_diff() {
        let root = std::env::temp_dir().join(format!("glass-more-git-{}", std::process::id()));
        std::fs::create_dir_all(&root).expect("create temporary workspace");
        let mut state =
            DevTuiState::open_for_tui(&root, TuiLayout::Desktop).expect("open temporary workspace");

        state.show_surface(DevSurface::More);
        state.selected_more = DevTuiState::more_route_actions()
            .iter()
            .position(|action| action.command == "doctor")
            .expect("the More action catalog exposes doctor");
        state.activate_more_selection();
        assert_eq!(state.surface, DevSurface::More);
        assert!(
            state.status.to_ascii_lowercase().contains("doctor")
                || state.status.contains("Core panel"),
            "doctor should stay on More: {}",
            state.status
        );

        state.git_entries = vec![crate::git::GitStatusEntry {
            path: "notes.txt".into(),
            original_path: None,
            index_status: ' ',
            worktree_status: 'M',
            untracked: false,
        }];
        state.show_surface(DevSurface::Git);
        assert!(state.git_diff_requested);
        assert!(state.git_diff_open);
        assert!(state.git_diff.contains("Loading"));
        assert_eq!(state.git_diff_path.as_deref(), Some("notes.txt"));
        std::fs::remove_dir_all(root).expect("remove temporary workspace");
    }

    #[test]
    fn selection_preview_bursts_keep_one_latest_git_process_or_debug_target() {
        let (mut state, root) = routed_state("selection-preview-burst");
        let mut worker = super::super::snapshot::SnapshotWorker::spawn(&state);
        state.surface = DevSurface::Git;
        state.git_entries = vec![
            crate::git::GitStatusEntry {
                path: "first.rs".into(),
                original_path: None,
                index_status: ' ',
                worktree_status: 'M',
                untracked: false,
            },
            crate::git::GitStatusEntry {
                path: "latest.rs".into(),
                original_path: None,
                index_status: ' ',
                worktree_status: 'M',
                untracked: false,
            },
        ];
        state.git_diff_open = true;
        state.git_diff_path = Some("first.rs".into());
        state.git_diff = "Loading first diff".into();
        state.active_selection_preview = Some(ActiveSelectionPreview {
            id: 100,
            key: SelectionPreviewKey {
                surface: DevSurface::Git,
                expected_debug_pane: None,
                target: SelectionPreviewTarget::GitDiff {
                    path: Some("first.rs".into()),
                    staged: false,
                },
            },
            label: "diff for first.rs".into(),
        });
        state.running_tool_job = Some(100);

        state.selected_git_file = 1;
        state.queue_git_diff(&mut worker);
        assert!(matches!(
            state.pending_selection_preview.as_ref().map(|request| &request.key.target),
            Some(SelectionPreviewTarget::GitDiff { path: Some(path), .. }) if path == "latest.rs"
        ));

        // Returning to the active selection cancels the pending target and
        // restores the active preview status instead of leaving a stale queue
        // message behind.
        state.selected_git_file = 0;
        state.queue_git_diff(&mut worker);
        assert!(state.pending_selection_preview.is_none());
        assert_eq!(state.status, "Loading diff for first.rs…");

        // A burst that crosses surfaces continually replaces the same pending
        // slot; the latest debugger session wins over Git and process previews.
        state.selected_git_file = 1;
        state.queue_git_diff(&mut worker);
        state.surface = DevSurface::Terminal;
        state.process_entries = vec![
            ProcessRow {
                name: "api".into(),
                command: "api".into(),
                pid: Some(1),
                health: crate::development::ProcessHealth::Healthy,
                url: None,
            },
            ProcessRow {
                name: "web".into(),
                command: "web".into(),
                pid: Some(2),
                health: crate::development::ProcessHealth::Healthy,
                url: None,
            },
        ];
        state.move_process_selection(1);
        state.queue_selected_process_logs(&mut worker);
        assert!(matches!(
            state.pending_selection_preview.as_ref().map(|request| &request.key.target),
            Some(SelectionPreviewTarget::ProcessLogs { name }) if name == "web"
        ));

        state.surface = DevSurface::Debug;
        state.debug_pane = DebugPane::Sessions;
        state.debug_sessions = vec![
            DebugSessionRow {
                name: "debug-a".into(),
                state: crate::debugger::DebugSessionState::Stopped,
                pid: 3,
                breakpoints: 0,
                watches: 0,
            },
            DebugSessionRow {
                name: "debug-b".into(),
                state: crate::debugger::DebugSessionState::Stopped,
                pid: 4,
                breakpoints: 0,
                watches: 0,
            },
        ];
        state.move_debug_selection(1);
        state.queue_debug_threads(&mut worker);
        assert!(matches!(
            state.pending_selection_preview.as_ref().map(|request| &request.key.target),
            Some(SelectionPreviewTarget::DebugThreads { session }) if session == "debug-b"
        ));

        state.apply_tool_job_result(super::super::snapshot::ToolJobResult {
            id: 99,
            tool: "glass.git.diff".into(),
            result: Ok(serde_json::json!("UNOWNED DIFF")),
        });
        assert_eq!(state.running_tool_job, Some(100));
        assert_eq!(
            state
                .active_selection_preview
                .as_ref()
                .map(|active| active.id),
            Some(100)
        );
        assert!(!state.git_diff.contains("UNOWNED DIFF"));

        state.apply_tool_job_result(super::super::snapshot::ToolJobResult {
            id: 100,
            tool: "glass.git.diff".into(),
            result: Ok(serde_json::json!("STALE FIRST DIFF")),
        });
        assert!(!state.git_diff.contains("STALE FIRST DIFF"));
        assert!(state.running_tool_job.is_none());
        assert!(state.active_selection_preview.is_none());
        assert!(matches!(
            state.pending_selection_preview.as_ref().map(|request| &request.key.target),
            Some(SelectionPreviewTarget::DebugThreads { session }) if session == "debug-b"
        ));

        state.flush_pending_selection_preview(&mut worker);
        assert!(state.pending_selection_preview.is_none());
        assert!(matches!(
            state.active_selection_preview.as_ref().map(|active| &active.key.target),
            Some(SelectionPreviewTarget::DebugThreads { session }) if session == "debug-b"
        ));
        let queued_id = state.running_tool_job.expect("latest preview submitted");
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        let mut completed = false;
        while std::time::Instant::now() < deadline {
            if let Ok(Some(result)) = worker.try_job_result() {
                assert_eq!(result.id, queued_id);
                state.apply_tool_job_result(result);
                completed = true;
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert!(
            completed,
            "latest preview should eventually return a result"
        );
        drop(worker);
        std::fs::remove_dir_all(root).expect("remove temporary workspace");
    }

    #[test]
    fn stale_git_diff_cannot_replace_untracked_placeholder() {
        let (mut state, root) = routed_state("selection-preview-untracked");
        let mut worker = super::super::snapshot::SnapshotWorker::spawn(&state);
        state.surface = DevSurface::Git;
        state.git_entries = vec![
            crate::git::GitStatusEntry {
                path: "tracked.rs".into(),
                original_path: None,
                index_status: ' ',
                worktree_status: 'M',
                untracked: false,
            },
            crate::git::GitStatusEntry {
                path: "new.rs".into(),
                original_path: None,
                index_status: '?',
                worktree_status: '?',
                untracked: true,
            },
        ];
        state.git_diff_open = true;
        state.git_diff_path = Some("tracked.rs".into());
        state.git_diff = "Loading Git diff…".into();
        state.running_tool_job = Some(55);
        state.active_selection_preview = Some(ActiveSelectionPreview {
            id: 55,
            key: SelectionPreviewKey {
                surface: DevSurface::Git,
                expected_debug_pane: None,
                target: SelectionPreviewTarget::GitDiff {
                    path: Some("tracked.rs".into()),
                    staged: false,
                },
            },
            label: "diff for tracked.rs".into(),
        });

        state.selected_git_file = 1;
        state.queue_git_diff(&mut worker);
        let placeholder = state.git_diff.clone();
        let placeholder_status = state.status.clone();
        assert!(placeholder.contains("Untracked file"));
        assert!(placeholder_status.contains("new.rs is untracked"));
        assert!(state.pending_selection_preview.is_none());

        state.apply_tool_job_result(super::super::snapshot::ToolJobResult {
            id: 55,
            tool: "glass.git.diff".into(),
            result: Ok(serde_json::json!("STALE TRACKED DIFF")),
        });
        assert_eq!(state.git_diff, placeholder);
        assert_eq!(state.status, placeholder_status);
        assert!(state.git_diff.contains("Untracked file"));
        assert!(state.running_tool_job.is_none());
        assert!(state.active_selection_preview.is_none());
        drop(worker);
        std::fs::remove_dir_all(root).expect("remove temporary workspace");
    }

    #[test]
    fn stale_debug_thread_and_stack_previews_do_not_take_over_new_pane() {
        let (mut state, root) = routed_state("selection-preview-debug-pane");
        state.surface = DevSurface::Debug;
        state.debug_sessions = vec![DebugSessionRow {
            name: "debug-a".into(),
            state: crate::debugger::DebugSessionState::Stopped,
            pid: 7,
            breakpoints: 0,
            watches: 0,
        }];
        state.debug_pane = DebugPane::Sessions;
        state.active_selection_preview = Some(ActiveSelectionPreview {
            id: 701,
            key: SelectionPreviewKey {
                surface: DevSurface::Debug,
                expected_debug_pane: Some(DebugPane::Sessions),
                target: SelectionPreviewTarget::DebugThreads {
                    session: "debug-a".into(),
                },
            },
            label: "threads for debug-a".into(),
        });
        state.running_tool_job = Some(701);

        state.debug_pane = DebugPane::Frames;
        state.status = "Frames pane selected by user".into();
        state.apply_tool_job_result(super::super::snapshot::ToolJobResult {
            id: 701,
            tool: "glass.debug.threads".into(),
            result: Ok(serde_json::json!({"threads": [{"id": 1, "name": "main"}]})),
        });
        assert_eq!(state.debug_pane, DebugPane::Frames);
        assert_eq!(state.status, "Frames pane selected by user");
        assert!(state.debug_threads.is_empty());
        assert!(!state.debug_stack_requested);

        state.debug_threads = vec![DebugThreadRow {
            id: 1,
            name: "main".into(),
        }];
        state.debug_pane = DebugPane::Threads;
        state.active_selection_preview = Some(ActiveSelectionPreview {
            id: 702,
            key: SelectionPreviewKey {
                surface: DevSurface::Debug,
                expected_debug_pane: Some(DebugPane::Threads),
                target: SelectionPreviewTarget::DebugStack {
                    session: "debug-a".into(),
                    thread_id: 1,
                },
            },
            label: "stack for thread 1".into(),
        });
        state.running_tool_job = Some(702);

        state.debug_pane = DebugPane::Sessions;
        state.status = "Sessions pane selected by user".into();
        state.apply_tool_job_result(super::super::snapshot::ToolJobResult {
            id: 702,
            tool: "glass.debug.stack".into(),
            result: Ok(serde_json::json!({"frames": [{"id": 3, "name": "main"}]})),
        });
        assert_eq!(state.debug_pane, DebugPane::Sessions);
        assert_eq!(state.status, "Sessions pane selected by user");
        assert!(state.debug_frames.is_empty());
        assert!(!state.debug_scopes_requested);
        std::fs::remove_dir_all(root).expect("remove temporary workspace");
    }

    #[test]
    fn stale_debug_scope_preview_resolves_loading_status_after_snapshot_removes_session() {
        let (mut state, root) = routed_state("selection-preview-stale-scopes");
        state.snapshot_ready = true;
        state.surface = DevSurface::Debug;
        state.debug_pane = DebugPane::Frames;
        state.debug_sessions = vec![DebugSessionRow {
            name: "debug-a".into(),
            state: crate::debugger::DebugSessionState::Stopped,
            pid: 10,
            breakpoints: 0,
            watches: 0,
        }];
        state.debug_frames = vec![DebugFrameRow {
            id: 44,
            name: "main".into(),
            path: Some("src/main.rs".into()),
            line: Some(1),
        }];
        state.status = "Loading debug scopes…".into();
        state.running_tool_job = Some(744);
        state.active_selection_preview = Some(ActiveSelectionPreview {
            id: 744,
            key: SelectionPreviewKey {
                surface: DevSurface::Debug,
                expected_debug_pane: Some(DebugPane::Frames),
                target: SelectionPreviewTarget::DebugScopes {
                    session: "debug-a".into(),
                    frame_id: 44,
                    frame_path: Some("src/main.rs".into()),
                    source_path: None,
                },
            },
            label: "debug scopes".into(),
        });

        state.apply_snapshot(&super::super::snapshot::DisplaySnapshot::default());

        assert!(state.debug_sessions.is_empty());
        assert!(state.debug_frames.is_empty());
        assert_eq!(
            state.status,
            "Debug scopes preview stale · select a frame to retry"
        );
        assert_eq!(
            state.debug_variables_notice.as_deref(),
            Some("Debug scopes preview stale · select a frame to retry")
        );
        state.apply_tool_job_result(super::super::snapshot::ToolJobResult {
            id: 744,
            tool: "glass.debug.scopes".into(),
            result: Ok(serde_json::json!({
                "scopes": [{"name": "Locals", "variablesReference": 1}]
            })),
        });
        assert!(state.debug_scopes.is_empty());
        assert_eq!(
            state.status,
            "Debug scopes preview stale · select a frame to retry"
        );
        std::fs::remove_dir_all(root).expect("remove temporary workspace");
    }

    #[test]
    fn stale_debug_variable_preview_resolves_loading_status_and_preserves_newer_status() {
        let (mut state, root) = routed_state("selection-preview-stale-variables");
        state.snapshot_ready = true;
        state.surface = DevSurface::Debug;
        state.debug_pane = DebugPane::Frames;
        state.debug_sessions = vec![DebugSessionRow {
            name: "debug-a".into(),
            state: crate::debugger::DebugSessionState::Stopped,
            pid: 11,
            breakpoints: 0,
            watches: 0,
        }];
        state.debug_frames = vec![DebugFrameRow {
            id: 45,
            name: "main".into(),
            path: Some("src/main.rs".into()),
            line: Some(1),
        }];
        state.debug_scopes = vec![DebugScopeRow {
            name: "Locals".into(),
            variables_reference: 19,
        }];
        state.status = "Loading debug variables…".into();
        state.running_tool_job = Some(745);
        state.active_selection_preview = Some(ActiveSelectionPreview {
            id: 745,
            key: SelectionPreviewKey {
                surface: DevSurface::Debug,
                expected_debug_pane: Some(DebugPane::Frames),
                target: SelectionPreviewTarget::DebugVariables {
                    session: "debug-a".into(),
                    frame_id: 45,
                    frame_path: Some("src/main.rs".into()),
                    source_path: None,
                    scope_index: 0,
                    scope_name: "Locals".into(),
                    reference: 19,
                },
            },
            label: "debug variables".into(),
        });

        state.apply_snapshot(&super::super::snapshot::DisplaySnapshot::default());

        assert!(state.debug_sessions.is_empty());
        assert!(state.debug_frames.is_empty());
        assert!(state.debug_scopes.is_empty());
        assert_eq!(
            state.status,
            "Debug variables preview stale · select a scope to retry"
        );
        assert_eq!(
            state.debug_variables_notice.as_deref(),
            Some("Debug variables preview stale · select a scope to retry")
        );
        state.status = "Newer user action owns this status".into();
        state.apply_tool_job_result(super::super::snapshot::ToolJobResult {
            id: 745,
            tool: "glass.debug.variables".into(),
            result: Ok(serde_json::json!({
                "variables": [{"name": "answer", "value": "42"}]
            })),
        });
        assert!(state.debug_variables.is_empty());
        assert_eq!(state.status, "Newer user action owns this status");
        std::fs::remove_dir_all(root).expect("remove temporary workspace");
    }

    #[test]
    fn stale_queued_debug_variable_preview_resolves_queued_status_without_submission() {
        let (mut state, root) = routed_state("selection-preview-queued-stale-variables");
        state.surface = DevSurface::Debug;
        state.debug_pane = DebugPane::Frames;
        state.debug_sessions = vec![DebugSessionRow {
            name: "debug-a".into(),
            state: crate::debugger::DebugSessionState::Stopped,
            pid: 12,
            breakpoints: 0,
            watches: 0,
        }];
        state.debug_frames = vec![DebugFrameRow {
            id: 46,
            name: "main".into(),
            path: Some("src/main.rs".into()),
            line: Some(1),
        }];
        state.debug_scopes = vec![DebugScopeRow {
            name: "Locals".into(),
            variables_reference: 20,
        }];
        state.running_tool_job = Some(746);
        let mut worker = super::super::snapshot::SnapshotWorker::spawn(&state);
        state.queue_debug_variables(&mut worker);
        assert!(matches!(
            state
                .pending_selection_preview
                .as_ref()
                .map(|pending| &pending.key.target),
            Some(SelectionPreviewTarget::DebugVariables { reference: 20, .. })
        ));
        assert!(state.status.starts_with("Queued debug variables"));

        state.running_tool_job = None;
        state.debug_sessions.clear();
        state.debug_frames.clear();
        state.debug_scopes.clear();
        state.flush_pending_selection_preview(&mut worker);

        assert!(state.pending_selection_preview.is_none());
        assert!(state.active_selection_preview.is_none());
        assert!(state.running_tool_job.is_none());
        assert_eq!(
            state.status,
            "Debug variables preview stale · select a scope to retry"
        );
        drop(worker);
        std::fs::remove_dir_all(root).expect("remove temporary workspace");
    }

    #[test]
    fn invalid_debug_variable_references_are_terminal_without_overwriting_code_status() {
        let (mut state, root) = routed_state("selection-preview-invalid-variable-reference");
        state.surface = DevSurface::Debug;
        state.debug_pane = DebugPane::Frames;
        state.debug_sessions = vec![DebugSessionRow {
            name: "debug-a".into(),
            state: crate::debugger::DebugSessionState::Stopped,
            pid: 13,
            breakpoints: 0,
            watches: 0,
        }];
        state.debug_frames = vec![DebugFrameRow {
            id: 47,
            name: "main".into(),
            path: Some("src/main.rs".into()),
            line: Some(1),
        }];
        state.debug_scopes = vec![DebugScopeRow {
            name: "Empty".into(),
            variables_reference: 0,
        }];
        let mut worker = super::super::snapshot::SnapshotWorker::spawn(&state);

        state.queue_debug_variables(&mut worker);

        assert!(state.pending_selection_preview.is_none());
        assert!(state.active_selection_preview.is_none());
        assert!(state.running_tool_job.is_none());
        assert_eq!(
            state.status,
            "No expandable variables · selected scope has no variable reference"
        );

        state.debug_scopes[0].variables_reference = -1;
        state.queue_debug_variables(&mut worker);
        assert_eq!(
            state.status,
            "Invalid variables reference · select another scope"
        );
        assert!(state.active_selection_preview.is_none());

        state.surface = DevSurface::Code;
        state.status = "Code source location · preserved".into();
        state.debug_scopes[0].variables_reference = 0;
        state.queue_debug_variables(&mut worker);
        assert_eq!(state.status, "Code source location · preserved");
        assert_eq!(
            state.debug_variables_notice.as_deref(),
            Some("No expandable variables · selected scope has no variable reference")
        );
        assert!(state.active_selection_preview.is_none());
        assert!(state.running_tool_job.is_none());
        drop(worker);
        std::fs::remove_dir_all(root).expect("remove temporary workspace");
    }

    #[test]
    fn debug_scopes_and_variables_finish_after_source_jump_without_replacing_code_status() {
        let (mut state, root) = routed_state("selection-preview-debug-source");
        std::fs::create_dir_all(root.join("src")).expect("create source directory");
        std::fs::write(root.join("src/main.rs"), "fn main() {}\n").expect("write source file");
        let dap_frame_path = root.join("src/main.rs").display().to_string();
        state.surface = DevSurface::Debug;
        state.debug_pane = DebugPane::Frames;
        state.debug_sessions = vec![DebugSessionRow {
            name: "debug-a".into(),
            state: crate::debugger::DebugSessionState::Stopped,
            pid: 8,
            breakpoints: 0,
            watches: 0,
        }];
        state.debug_frames = vec![DebugFrameRow {
            id: 42,
            name: "main".into(),
            path: Some(dap_frame_path.clone()),
            line: Some(1),
        }];

        state.jump_selected_debug_frame();
        assert_eq!(state.surface, DevSurface::Code);
        assert_eq!(state.focused_editor_path, "src/main.rs");
        assert_eq!(state.debug_pane, DebugPane::Frames);
        let code_status = state.status.clone();

        let mut worker = super::super::snapshot::SnapshotWorker::spawn(&state);
        state.queue_debug_scopes(&mut worker);
        let scopes_id = state.running_tool_job.expect("scope preview submitted");
        assert!(matches!(
            state.active_selection_preview.as_ref().map(|active| &active.key),
            Some(SelectionPreviewKey {
                surface: DevSurface::Code,
                expected_debug_pane: Some(DebugPane::Frames),
                target: SelectionPreviewTarget::DebugScopes {
                    session,
                    frame_id: 42,
                    frame_path: Some(frame_path),
                    source_path: Some(source_path),
                },
            }) if session == "debug-a"
                && frame_path == &dap_frame_path
                && source_path == "src/main.rs"
        ));
        assert_eq!(state.status, code_status);

        state.apply_tool_job_result(super::super::snapshot::ToolJobResult {
            id: scopes_id,
            tool: "glass.debug.scopes".into(),
            result: Ok(serde_json::json!({
                "scopes": [{"name": "Locals", "variablesReference": 17}]
            })),
        });
        assert_eq!(state.surface, DevSurface::Code);
        assert_eq!(state.debug_pane, DebugPane::Frames);
        assert_eq!(state.status, code_status);
        assert_eq!(state.debug_scopes.len(), 1);
        assert!(state.debug_variables_requested);

        state.debug_variables_requested = false;
        state.queue_debug_variables(&mut worker);
        let variables_id = state
            .running_tool_job
            .expect("automatic variable preview submitted");
        assert!(matches!(
            state.active_selection_preview.as_ref().map(|active| &active.key),
            Some(SelectionPreviewKey {
                surface: DevSurface::Code,
                expected_debug_pane: Some(DebugPane::Frames),
                target: SelectionPreviewTarget::DebugVariables {
                    session,
                    frame_id: 42,
                    frame_path: Some(frame_path),
                    source_path: Some(source_path),
                    scope_index: 0,
                    scope_name,
                    reference: 17,
                },
            }) if session == "debug-a"
                && frame_path == &dap_frame_path
                && source_path == "src/main.rs"
                && scope_name == "Locals"
        ));
        state.apply_tool_job_result(super::super::snapshot::ToolJobResult {
            id: variables_id,
            tool: "glass.debug.variables".into(),
            result: Ok(serde_json::json!({
                "variables": [{"name": "answer", "value": "42"}]
            })),
        });
        assert_eq!(state.surface, DevSurface::Code);
        assert_eq!(state.debug_pane, DebugPane::Frames);
        assert_eq!(state.status, code_status);
        assert_eq!(state.debug_variables.len(), 1);
        assert_eq!(state.debug_variables[0].name, "answer");

        drop(worker);
        std::fs::remove_dir_all(root).expect("remove temporary workspace");
    }

    #[cfg(windows)]
    #[test]
    fn windows_workspace_relative_paths_ignore_component_case() {
        assert_eq!(
            relative_workspace_path(
                Path::new(r"C:\Users\runneradmin\Project"),
                Path::new(r"c:\users\RUNNERADMIN\project\src\main.rs"),
            ),
            Some(PathBuf::from(r"src\main.rs"))
        );
        assert_eq!(
            relative_workspace_path(
                Path::new(r"C:\Users\runneradmin\Project"),
                Path::new(r"C:\Users\runneradmin\Other\main.rs"),
            ),
            None
        );
    }

    #[test]
    fn debug_jump_rejects_absolute_source_outside_workspace() {
        let (mut state, root) = routed_state("selection-preview-debug-outside-source");
        let outside_path = root.with_file_name(format!(
            "outside-{}.rs",
            root.file_name()
                .expect("temporary workspace name")
                .to_string_lossy()
        ));
        std::fs::write(&outside_path, "fn outside() {}\n").expect("write outside source file");
        state.surface = DevSurface::Debug;
        state.debug_pane = DebugPane::Frames;
        state.debug_sessions = vec![DebugSessionRow {
            name: "debug-a".into(),
            state: crate::debugger::DebugSessionState::Stopped,
            pid: 9,
            breakpoints: 0,
            watches: 0,
        }];
        state.debug_frames = vec![DebugFrameRow {
            id: 43,
            name: "outside".into(),
            path: Some(outside_path.display().to_string()),
            line: Some(1),
        }];
        let files_before = state.files.clone();

        state.jump_selected_debug_frame();

        assert_eq!(state.surface, DevSurface::Debug);
        assert!(state.focused_editor_path.is_empty());
        assert_eq!(state.files, files_before);
        assert!(
            state
                .status
                .contains("source path is outside this workspace"),
            "unexpected status: {}",
            state.status
        );
        assert!(
            state
                .ws()
                .expect("workspace lock")
                .project()
                .buffers()
                .next()
                .is_none(),
            "outside source must not be opened into a project buffer"
        );

        std::fs::remove_file(outside_path).expect("remove outside source file");
        std::fs::remove_dir_all(root).expect("remove temporary workspace");
    }
}
