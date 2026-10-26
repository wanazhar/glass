//! Background snapshot actor for the TUI.
//!
//! The research consensus (Pleiades, Mastra Code, zerostack) is that model
//! streams, tools, and workspace queries must never occupy the terminal
//! task. This module applies that rule to Glass: a worker thread owns the
//! expensive refresh pass — file listing, git status, agent history,
//! process table, test runs — and publishes immutable display snapshots
//! through a `watch` channel. The render loop reads the latest snapshot
//! without ever blocking on the workspace lock, and a latency budget keeps
//! slow git repositories from freezing key handling.

use crate::tui::state::{DebugSessionRow, DevTuiState, ProcessRow};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, Sender, TryRecvError, channel};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

pub struct ToolJob {
    pub id: u64,
    pub call: crate::development::ToolCall,
    pub context: crate::tools::DevelopmentToolContext,
}

pub struct ToolJobResult {
    pub id: u64,
    pub tool: String,
    pub result: Result<serde_json::Value, String>,
}

pub struct VisualJobResult {
    pub id: u64,
    pub columns: u16,
    pub rows: u16,
    pub result: Result<serde_json::Value, String>,
}

/// Commands the UI sends to the worker.
pub enum ActorRequest {
    /// Recompute every snapshot field immediately.
    Refresh,
    /// Recompute only conversation tail fields (cheap, high frequency).
    RefreshConversation,
    Tool(Box<ToolJob>),
    Screenshot {
        id: u64,
        columns: u16,
        rows: u16,
    },
    ShutDown,
}

/// One refresh pass result. Cheap to clone; the UI keeps the latest.
/// Browser supervision outcome for one pass.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum BrowserHealth {
    #[default]
    Unknown,
    /// Connected and healthy.
    Connected,
    /// Was connected earlier; the endpoint stopped responding.
    Crashed {
        last_process_id: Option<u32>,
        last_revision: Option<u64>,
        last_backend: Option<String>,
    },
}

fn browser_backend_from_runtime_state(state: &serde_json::Value) -> Option<String> {
    state
        .pointer("/browserBackend")
        .and_then(serde_json::Value::as_str)
        .map(str::to_owned)
}

#[derive(Debug, Clone)]
pub struct AgentChrome {
    pub id: crate::AgentId,
    pub status: crate::AgentStatus,
    pub model: Option<String>,
    pub thinking: Option<String>,
    pub session_file: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct DisplaySnapshot {
    pub version: u64,
    /// Highest agent event sequence folded into the conversation projection.
    pub conversation_cursor: u64,
    pub agents: String,
    /// Typed lifecycle state for each resident agent; the TUI uses this to
    /// replace optimistic "thinking" copy when a turn actually settles.
    pub agent_states: Vec<(crate::AgentId, crate::AgentStatus)>,
    pub thread_picker_items: Vec<super::state::ThreadPickerItem>,
    pub editor_proposals: Vec<crate::development::EditorProposal>,
    pub harnesses: String,
    pub agent_conversation: String,
    pub conversation_items: Vec<super::projection::ConversationEntry>,
    /// Conversation projections partitioned by their process-local resident agent.
    pub agent_conversations: std::collections::BTreeMap<crate::AgentId, AgentConversation>,
    /// Core-owned slash command catalogues returned by each resident session.
    pub core_command_catalogues: std::collections::BTreeMap<crate::AgentId, CoreCommandCatalogue>,
    /// Core-owned provider/model catalogues returned by each resident session.
    pub core_model_catalogues: std::collections::BTreeMap<crate::AgentId, CoreModelCatalogue>,
    /// Safe display-only fields from Core's session/describe response.
    pub core_session_settings: std::collections::BTreeMap<crate::AgentId, CoreSessionSettings>,
    pub agent_chrome: Vec<AgentChrome>,
    pub tasks: String,
    pub editor: String,
    pub lsp: String,
    pub processes: String,
    pub process_entries: Vec<ProcessRow>,
    pub git: String,
    pub git_entries: Vec<crate::git::GitStatusEntry>,
    pub git_branch: String,
    pub git_dirty: bool,
    pub git_ahead: u64,
    pub git_behind: u64,
    pub git_conflicts: Vec<String>,
    pub github: crate::github::GitHubStatus,
    pub github_review: String,
    pub tests: String,
    pub kernels: String,
    pub debugger: String,
    pub debug_sessions: Vec<DebugSessionRow>,
    pub replay: String,
    pub workflow: String,
    pub workspace_status: String,
    pub experiments: String,
    pub files: Vec<String>,
    pub browser: String,
    pub root: String,
    pub project_revision: u64,
    pub generation: u64,
    pub skills_count: usize,
    pub tools_count: usize,
    /// Wall-clock duration of the pass, for the latency meter.
    pub duration: Duration,
    /// Browser supervision verdict for this pass.
    pub browser_health: BrowserHealth,
}

#[derive(Debug, Clone, Default)]
pub struct CoreCommandCatalogue {
    pub commands: Vec<serde_json::Value>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct CoreModelCatalogue {
    pub provider: Option<String>,
    pub current_model: Option<String>,
    pub models: Vec<serde_json::Value>,
    pub error: Option<String>,
}

/// Safe, display-only subset of the selected Core session manifest.
/// Credentials, endpoints, instructions, and workspace paths are deliberately
/// excluded from the UI projection.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CoreSessionSettings {
    pub provider: Option<String>,
    pub model: Option<String>,
    pub max_tokens: Option<u64>,
    pub image_input: Option<bool>,
    pub tool_mode: Option<String>,
    pub reasoning_budget: Option<u64>,
    pub error: Option<String>,
}

fn core_command_catalogues(
    events: &[crate::AgentEvent],
) -> std::collections::BTreeMap<crate::AgentId, CoreCommandCatalogue> {
    let mut catalogues = std::collections::BTreeMap::<crate::AgentId, CoreCommandCatalogue>::new();
    for event in events {
        if event.kind != "response"
            || event
                .payload
                .get("operation")
                .and_then(serde_json::Value::as_str)
                != Some("coreCommands")
        {
            continue;
        }
        let catalogue = catalogues.entry(event.agent_id.clone()).or_default();
        if event.payload.get("ok").and_then(serde_json::Value::as_bool) == Some(true) {
            catalogue.commands = event
                .payload
                .pointer("/result/commands")
                .and_then(serde_json::Value::as_array)
                .cloned()
                .unwrap_or_default();
            catalogue.error = None;
        } else {
            catalogue.error = event
                .payload
                .pointer("/error/message")
                .and_then(serde_json::Value::as_str)
                .map(str::to_owned)
                .or_else(|| Some("Glass Core did not return its command catalogue".into()));
        }
    }
    catalogues
}

fn core_model_catalogues(
    events: &[crate::AgentEvent],
) -> std::collections::BTreeMap<crate::AgentId, CoreModelCatalogue> {
    let mut catalogues = std::collections::BTreeMap::<crate::AgentId, CoreModelCatalogue>::new();
    for event in events {
        if event.kind != "response"
            || event
                .payload
                .get("operation")
                .and_then(serde_json::Value::as_str)
                != Some("models")
        {
            continue;
        }
        let catalogue = catalogues.entry(event.agent_id.clone()).or_default();
        if event.payload.get("ok").and_then(serde_json::Value::as_bool) == Some(true) {
            let result = &event.payload["result"];
            catalogue.provider = result
                .get("provider")
                .and_then(serde_json::Value::as_str)
                .map(str::to_owned);
            catalogue.current_model = result
                .get("currentModel")
                .and_then(serde_json::Value::as_str)
                .map(str::to_owned);
            catalogue.models = result
                .get("models")
                .and_then(serde_json::Value::as_array)
                .cloned()
                .unwrap_or_default();
            catalogue.error = None;
        } else {
            catalogue.current_model = None;
            catalogue.error = event
                .payload
                .pointer("/error/message")
                .and_then(serde_json::Value::as_str)
                .map(str::to_owned)
                .or_else(|| Some("Glass Core did not return its model catalogue".into()));
        }
    }
    catalogues
}

fn core_session_settings(
    events: &[crate::AgentEvent],
) -> std::collections::BTreeMap<crate::AgentId, CoreSessionSettings> {
    let mut settings = std::collections::BTreeMap::new();
    for event in events {
        if event.kind != "response"
            || event
                .payload
                .get("operation")
                .and_then(serde_json::Value::as_str)
                != Some("hello")
        {
            continue;
        }
        let setting = if event.payload.get("ok").and_then(serde_json::Value::as_bool) == Some(true)
        {
            let value = &event.payload["result"];
            CoreSessionSettings {
                provider: value
                    .get("provider")
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_owned),
                model: value
                    .get("model")
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_owned),
                max_tokens: value.get("maxTokens").and_then(serde_json::Value::as_u64),
                image_input: value.get("imageInput").and_then(serde_json::Value::as_bool),
                tool_mode: value
                    .get("toolMode")
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_owned),
                reasoning_budget: value
                    .get("reasoningBudget")
                    .and_then(serde_json::Value::as_u64),
                error: None,
            }
        } else {
            CoreSessionSettings {
                error: event
                    .payload
                    .pointer("/error/message")
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_owned)
                    .or_else(|| Some("Glass Core session details are unavailable".into())),
                ..Default::default()
            }
        };
        settings.insert(event.agent_id.clone(), setting);
    }
    settings
}

#[derive(Debug, Clone, Default)]
pub struct AgentConversation {
    pub text: String,
    pub items: Vec<super::projection::ConversationEntry>,
    /// Private provider reasoning shown only in the explicit reasoning view.
    pub reasoning: String,
    /// Bounded, display-safe view of Core's latest durable task graph response.
    pub core_task_graph: Option<CoreTaskGraph>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CoreTaskGraph {
    pub sequence: Option<u64>,
    pub tasks: Vec<CoreTaskRow>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoreTaskRow {
    pub id: String,
    pub kind: String,
    pub status: String,
    pub parent: Option<String>,
    pub background: bool,
    pub dependency_count: usize,
}

fn bounded_task_label(value: &serde_json::Value, fallback: &str) -> String {
    value
        .as_str()
        .map(|text| {
            text.chars()
                .filter(|character| !character.is_control())
                .take(128)
                .collect::<String>()
        })
        .filter(|text| !text.is_empty())
        .unwrap_or_else(|| fallback.to_string())
}

fn core_task_graph(events: &[crate::AgentEvent]) -> Option<CoreTaskGraph> {
    let event = events.iter().rev().find(|event| {
        event.kind == "response"
            && event
                .payload
                .get("operation")
                .and_then(serde_json::Value::as_str)
                == Some("coreTaskGraph")
    })?;
    if event.payload.get("ok").and_then(serde_json::Value::as_bool) != Some(true) {
        let message = event
            .payload
            .pointer("/error/message")
            .and_then(serde_json::Value::as_str)
            .map(|message| {
                message
                    .chars()
                    .filter(|character| !character.is_control())
                    .take(192)
                    .collect()
            });
        return Some(CoreTaskGraph {
            error: message.or_else(|| Some("Core task graph request failed".into())),
            ..CoreTaskGraph::default()
        });
    }
    let result = &event.payload["result"];
    let rows = result
        .get("tasks")
        .and_then(serde_json::Value::as_array)
        .into_iter()
        .flatten()
        .take(128)
        .map(|task| CoreTaskRow {
            id: bounded_task_label(&task["id"], "?"),
            kind: bounded_task_label(&task["kind"], "?"),
            status: bounded_task_label(&task["status"], "?"),
            parent: task["owner"].as_str().map(|parent| {
                parent
                    .chars()
                    .filter(|character| !character.is_control())
                    .take(128)
                    .collect()
            }),
            background: task["background"].as_bool().unwrap_or(false),
            dependency_count: task["dependencyCount"].as_u64().unwrap_or(0) as usize,
        })
        .collect();
    Some(CoreTaskGraph {
        sequence: result.get("sequence").and_then(serde_json::Value::as_u64),
        tasks: rows,
        error: None,
    })
}

fn agent_conversations(
    events: &[crate::AgentEvent],
) -> std::collections::BTreeMap<crate::AgentId, AgentConversation> {
    let mut grouped = std::collections::BTreeMap::<crate::AgentId, Vec<crate::AgentEvent>>::new();
    for event in events {
        grouped
            .entry(event.agent_id.clone())
            .or_default()
            .push(event.clone());
    }
    grouped
        .into_iter()
        .map(|(id, events)| {
            let items = crate::tui::projection::conversation_entries(&events);
            let reasoning = crate::tui::projection::conversation_reasoning(&events);
            let text = if items.is_empty() {
                String::new()
            } else {
                crate::tui::projection::conversation(&events)
            };
            (
                id,
                AgentConversation {
                    text,
                    items,
                    reasoning,
                    core_task_graph: core_task_graph(&events),
                },
            )
        })
        .collect()
}

impl DisplaySnapshot {
    /// True when every projection still holds its initial empty value.
    pub fn is_empty(&self) -> bool {
        self.agents.is_empty() && self.git.is_empty() && self.files.is_empty()
    }
}

/// Shared flag set by the UI when a render consumed a stale frame.
pub struct SnapshotWorker {
    handle: Option<JoinHandle<()>>,
    requests: Sender<ActorRequest>,
    snapshots: Arc<std::sync::Mutex<Option<DisplaySnapshot>>>,
    dirty: Arc<AtomicBool>,
    last_applied: Option<DisplaySnapshot>,
    /// Highest agent event sequence already folded into the conversation.
    conversation_cursor: Arc<std::sync::atomic::AtomicU64>,
    job_results: Receiver<ToolJobResult>,
    visual_results: Receiver<VisualJobResult>,
    next_job_id: u64,
    next_visual_id: u64,
    refresh_requested: Arc<AtomicBool>,
    conversation_requested: Arc<AtomicBool>,
    visual_requested: Arc<AtomicBool>,
}

impl SnapshotWorker {
    /// Spawn the worker for one workspace. The state is only used for its
    /// shared workspace handle; the UI retains its own clone.
    pub fn spawn(state: &DevTuiState) -> Self {
        let workspace = state.workspace.clone();
        let (request_tx, request_rx) = channel::<ActorRequest>();
        let (job_result_tx, job_result_rx) = channel::<ToolJobResult>();
        let (visual_result_tx, visual_result_rx) = channel::<VisualJobResult>();
        let refresh_requested = Arc::new(AtomicBool::new(false));
        let conversation_requested = Arc::new(AtomicBool::new(false));
        let worker_refresh_requested = Arc::clone(&refresh_requested);
        let worker_conversation_requested = Arc::clone(&conversation_requested);
        let visual_requested = Arc::new(AtomicBool::new(false));
        let worker_visual_requested = Arc::clone(&visual_requested);
        let snapshots = Arc::new(std::sync::Mutex::new(None::<DisplaySnapshot>));
        let dirty = Arc::new(AtomicBool::new(true));
        let snapshot_sink = Arc::clone(&snapshots);
        let dirty_flag = Arc::clone(&dirty);
        let conversation_cursor = Arc::new(std::sync::atomic::AtomicU64::new(0));
        let worker_cursor = Arc::clone(&conversation_cursor);
        let handle = std::thread::Builder::new()
            .name("glass-snapshot".into())
            .spawn(move || {
                worker_loop(
                    workspace,
                    request_rx,
                    job_result_tx,
                    visual_result_tx,
                    worker_refresh_requested,
                    worker_conversation_requested,
                    worker_visual_requested,
                    snapshot_sink,
                    dirty_flag,
                    worker_cursor,
                );
            })
            .expect("failed to spawn snapshot worker");
        Self {
            handle: Some(handle),
            requests: request_tx,
            snapshots,
            dirty,
            last_applied: None,
            conversation_cursor,
            job_results: job_result_rx,
            visual_results: visual_result_rx,
            next_job_id: 1,
            next_visual_id: 1,
            refresh_requested,
            conversation_requested,
            visual_requested,
        }
    }

    /// Ask for a full refresh pass.
    pub fn request_refresh(&self) {
        if !self.refresh_requested.swap(true, Ordering::AcqRel)
            && self.requests.send(ActorRequest::Refresh).is_err()
        {
            self.refresh_requested.store(false, Ordering::Release);
        }
        self.dirty.store(true, Ordering::Release);
    }

    /// Ask for a cheap conversation-tail pass.
    pub fn request_conversation(&self) {
        if !self.conversation_requested.swap(true, Ordering::AcqRel)
            && self
                .requests
                .send(ActorRequest::RefreshConversation)
                .is_err()
        {
            self.conversation_requested.store(false, Ordering::Release);
        }
    }

    /// Queue a governed tool call without occupying the terminal task.
    pub fn submit_tool(
        &mut self,
        call: crate::development::ToolCall,
        context: crate::tools::DevelopmentToolContext,
    ) -> Result<u64, String> {
        let id = self.next_job_id;
        self.next_job_id = self.next_job_id.saturating_add(1);
        self.requests
            .send(ActorRequest::Tool(Box::new(ToolJob { id, call, context })))
            .map_err(|_| "snapshot worker stopped".to_string())?;
        Ok(id)
    }

    pub fn try_job_result(&self) -> Result<Option<ToolJobResult>, String> {
        match self.job_results.try_recv() {
            Ok(result) => Ok(Some(result)),
            Err(TryRecvError::Empty) => Ok(None),
            Err(TryRecvError::Disconnected) => Err("tool worker stopped".into()),
        }
    }

    pub fn submit_screenshot(&mut self, columns: u16, rows: u16) -> Option<u64> {
        if self.visual_requested.swap(true, Ordering::AcqRel) {
            return None;
        }
        let id = self.next_visual_id;
        self.next_visual_id = self.next_visual_id.saturating_add(1);
        if self
            .requests
            .send(ActorRequest::Screenshot { id, columns, rows })
            .is_err()
        {
            self.visual_requested.store(false, Ordering::Release);
            return None;
        }
        Some(id)
    }

    pub fn try_visual_result(&self) -> Result<Option<VisualJobResult>, String> {
        match self.visual_results.try_recv() {
            Ok(result) => Ok(Some(result)),
            Err(TryRecvError::Empty) => Ok(None),
            Err(TryRecvError::Disconnected) => Err("visual worker stopped".into()),
        }
    }

    /// Take the freshest snapshot when one is newer than the last applied;
    /// `None` when current. Never blocks on the worker.
    pub fn take_pending(&mut self) -> Option<DisplaySnapshot> {
        let latest = self.snapshots.lock().ok().and_then(|slot| slot.clone())?;
        let changed = self
            .last_applied
            .as_ref()
            .is_none_or(|applied| applied.version != latest.version);
        if !changed {
            return None;
        }
        self.last_applied = Some(latest.clone());
        Some(latest)
    }

    /// True while a requested pass has not produced a snapshot yet.
    pub fn is_busy(&self) -> bool {
        self.dirty.load(Ordering::Acquire)
    }

    /// Highest agent event sequence already folded into the conversation.
    pub fn conversation_cursor(&self) -> u64 {
        self.conversation_cursor.load(Ordering::Acquire)
    }
}

impl Drop for SnapshotWorker {
    fn drop(&mut self) {
        let _ = self.requests.send(ActorRequest::ShutDown);
        // Always reap the worker, including an in-flight tool call. The TUI
        // declares its terminal guard after this worker, so raw mode and the
        // alternate screen are restored before this join can wait.
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn worker_loop(
    workspace: crate::SharedDevelopmentWorkspace,
    requests: Receiver<ActorRequest>,
    job_results: Sender<ToolJobResult>,
    visual_results: Sender<VisualJobResult>,
    refresh_requested: Arc<AtomicBool>,
    conversation_requested: Arc<AtomicBool>,
    visual_requested: Arc<AtomicBool>,
    snapshots: Arc<std::sync::Mutex<Option<DisplaySnapshot>>>,
    dirty: Arc<AtomicBool>,
    conversation_cursor: Arc<std::sync::atomic::AtomicU64>,
) {
    // GitHub review probes are cached independently of cheap local snapshots;
    // this keeps the render loop local while still refreshing remote state.
    let mut github_review = crate::github::GitHubReviewCache::default();
    let mut seeded = false;
    let mut conversation_events = Vec::new();
    let mut conversation_tail = String::new();
    let mut github_probe = crate::github::GitHubProbeCache::default();
    let mut last_browser: Option<(Option<u32>, Option<u64>, Option<String>)> = None;
    let mut snapshot_version = 0u64;
    loop {
        let request = if seeded {
            match requests.recv_timeout(Duration::from_millis(250)) {
                Ok(request) => request,
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => ActorRequest::Refresh,
                Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
            }
        } else {
            match requests.recv() {
                Ok(request) => request,
                Err(_) => break,
            }
        };
        match request {
            ActorRequest::ShutDown => break,
            ActorRequest::Screenshot { id, columns, rows } => {
                visual_requested.store(false, Ordering::Release);
                let result = workspace
                    .lock()
                    .and_then(|locked| locked.browser().screenshot())
                    .map_err(|error| error.to_string());
                let _ = visual_results.send(VisualJobResult {
                    id,
                    columns,
                    rows,
                    result,
                });
            }
            ActorRequest::Tool(job) => {
                let tool = job.call.name.clone();
                let result = workspace
                    .lock()
                    .and_then(|mut locked| locked.execute_tool(&job.call, &job.context))
                    .map_err(|error| error.to_string());
                let _ = job_results.send(ToolJobResult {
                    id: job.id,
                    tool,
                    result,
                });
            }
            ActorRequest::RefreshConversation => {
                conversation_requested.store(false, Ordering::Release);
                // Cheap fast path: fold only new events, then re-render the
                // bounded event history so streaming deltas coalesce.
                let Ok(mut locked) = workspace.lock() else {
                    continue;
                };
                let cursor = conversation_cursor.load(Ordering::Acquire);
                if let Ok(events) = locked.agents().history(cursor)
                    && !events.is_empty()
                {
                    let highest = events
                        .iter()
                        .map(|event| event.sequence)
                        .max()
                        .unwrap_or(cursor);
                    conversation_events.extend(events);
                    let items = crate::tui::projection::conversation_entries(&conversation_events);
                    let by_agent = agent_conversations(&conversation_events);
                    conversation_tail = if items.is_empty() {
                        "No conversation yet. Press Enter or start typing to compose a message."
                            .into()
                    } else {
                        crate::tui::projection::conversation(&conversation_events)
                    };
                    conversation_cursor.store(highest, Ordering::Release);
                    if let Ok(mut slot) = snapshots.lock()
                        && let Some(snapshot) = slot.as_mut()
                    {
                        snapshot.conversation_items = items;
                        snapshot.agent_conversations = by_agent;
                        snapshot.core_command_catalogues =
                            core_command_catalogues(&conversation_events);
                        snapshot.core_model_catalogues =
                            core_model_catalogues(&conversation_events);
                        snapshot.core_session_settings =
                            core_session_settings(&conversation_events);
                        snapshot.conversation_cursor = highest;
                    }
                }
                if let Ok(mut slot) = snapshots.lock()
                    && let Some(snapshot) = slot.as_mut()
                {
                    snapshot_version = snapshot_version.saturating_add(1);
                    snapshot.version = snapshot_version;
                    snapshot.agent_conversation = conversation_tail.clone();
                }
                dirty.store(false, Ordering::Release);
            }
            ActorRequest::Refresh => {
                refresh_requested.store(false, Ordering::Release);
                let started = Instant::now();
                let mut snapshot = compute_snapshot(
                    &workspace,
                    &mut seeded,
                    &mut github_probe,
                    &mut github_review,
                );
                snapshot_version = snapshot_version.saturating_add(1);
                snapshot.version = snapshot_version;
                snapshot.duration = started.elapsed();
                // Supervise the browser endpoint between passes.
                let state = workspace
                    .lock()
                    .ok()
                    .and_then(|locked| locked.browser().state().ok());
                match state {
                    Some(state) => {
                        let connected = state
                            .pointer("/connected")
                            .and_then(serde_json::Value::as_bool)
                            .unwrap_or(false);
                        let pid = state
                            .pointer("/browserProcessId")
                            .and_then(serde_json::Value::as_u64)
                            .map(|pid| pid as u32);
                        let revision = state
                            .pointer("/browserRevision")
                            .and_then(serde_json::Value::as_u64);
                        let backend = browser_backend_from_runtime_state(&state);
                        if connected {
                            last_browser = Some((pid, revision, backend));
                            snapshot.browser_health = BrowserHealth::Connected;
                        } else if let Some(previous) = last_browser.take() {
                            snapshot.browser_health = BrowserHealth::Crashed {
                                last_process_id: previous.0,
                                last_revision: previous.1,
                                last_backend: previous.2,
                            };
                        }
                    }
                    None => {
                        if let Some(previous) = last_browser.take() {
                            snapshot.browser_health = BrowserHealth::Crashed {
                                last_process_id: previous.0,
                                last_revision: previous.1,
                                last_backend: previous.2,
                            };
                        }
                    }
                }
                if let Ok(latest) = workspace
                    .lock()
                    .and_then(|mut locked| locked.agents().history(0))
                {
                    conversation_events = latest;
                    let items = crate::tui::projection::conversation_entries(&conversation_events);
                    let by_agent = agent_conversations(&conversation_events);
                    conversation_tail = if items.is_empty() {
                        "No conversation yet. Press Enter or start typing to compose a message."
                            .into()
                    } else {
                        crate::tui::projection::conversation(&conversation_events)
                    };
                    snapshot.conversation_items = items;
                    snapshot.agent_conversations = by_agent;
                    snapshot.core_command_catalogues =
                        core_command_catalogues(&conversation_events);
                    snapshot.core_model_catalogues = core_model_catalogues(&conversation_events);
                    snapshot.core_session_settings = core_session_settings(&conversation_events);
                    let highest = conversation_events
                        .iter()
                        .map(|event| event.sequence)
                        .max()
                        .unwrap_or_default();
                    conversation_cursor.store(highest, Ordering::Release);
                }
                snapshot.conversation_cursor = conversation_cursor.load(Ordering::Acquire);
                snapshot.agent_conversation = conversation_tail.clone();
                if let Ok(mut slot) = snapshots.lock() {
                    *slot = Some(snapshot);
                }
                dirty.store(false, Ordering::Release);
            }
        }
    }
}

/// One full snapshot pass. Mirrors DevTuiState::refresh but never touches
/// UI-only fields (selection, scroll, status).
fn compute_snapshot(
    workspace: &crate::SharedDevelopmentWorkspace,
    seeded: &mut bool,
    github_probe: &mut crate::github::GitHubProbeCache,
    github_review: &mut crate::github::GitHubReviewCache,
) -> DisplaySnapshot {
    let mut snapshot = DisplaySnapshot::default();
    let Ok(mut locked) = workspace.lock() else {
        snapshot.git = "Workspace lock failed".into();
        return snapshot;
    };
    *seeded = true;
    snapshot.files = locked
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
    snapshot.harnesses = crate::harness::summary();
    let agent_snapshots = locked.agents().list();
    let task_snapshots = locked.tasks();
    let picker_tasks = task_snapshots
        .as_ref()
        .map(Vec::as_slice)
        .unwrap_or_default();
    snapshot.editor_proposals = locked.project().editor_proposals();
    snapshot.agent_states = agent_snapshots
        .as_ref()
        .map(|agents| {
            agents
                .iter()
                .map(|agent| (agent.id.clone(), agent.status))
                .collect()
        })
        .unwrap_or_default();
    snapshot.thread_picker_items = agent_snapshots
        .as_ref()
        .map(|agents| super::state::project_thread_picker_items(agents, picker_tasks))
        .unwrap_or_default();
    snapshot.agent_chrome = agent_snapshots
        .as_ref()
        .map(|agents| {
            agents
                .iter()
                .map(|agent| AgentChrome {
                    id: agent.id.clone(),
                    status: agent.status,
                    model: agent.model.clone(),
                    thinking: agent.thinking.clone(),
                    session_file: agent.session_file.clone(),
                })
                .collect()
        })
        .unwrap_or_default();
    snapshot.agents = match agent_snapshots {
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
    snapshot.agent_conversation = match locked.agents().history(0) {
        Ok(events) if events.is_empty() => {
            snapshot.conversation_items.clear();
            snapshot.agent_conversations.clear();
            snapshot.core_command_catalogues.clear();
            snapshot.core_model_catalogues.clear();
            snapshot.core_session_settings.clear();
            "No conversation yet. Press Enter or start typing to compose a message.".into()
        }
        Ok(events) => {
            snapshot.conversation_items = crate::tui::projection::conversation_entries(&events);
            snapshot.agent_conversations = agent_conversations(&events);
            snapshot.core_command_catalogues = core_command_catalogues(&events);
            snapshot.core_model_catalogues = core_model_catalogues(&events);
            snapshot.core_session_settings = core_session_settings(&events);
            crate::tui::projection::conversation(&events)
        }
        Err(error) => format!("Conversation unavailable: {error}"),
    };
    let core_threads = snapshot
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
    let workspace_tasks = match task_snapshots {
        Ok(tasks) if tasks.is_empty() => String::new(),
        Ok(tasks) => tasks
            .iter()
            .map(|task| {
                let glyph = match task.state {
                    crate::TaskState::Succeeded => "✓",
                    crate::TaskState::Failed | crate::TaskState::Cancelled => "×",
                    crate::TaskState::Blocked => "!",
                    crate::TaskState::Running | crate::TaskState::Verifying => "●",
                    _ => "○",
                };
                format!(
                    "{glyph} {}  {}  {}\n  goal {}\n  agent {} · attempt {} · model {} · thinking {}\n  depends {}\n  verification {}\n  evidence {}",
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
                    task.evidence
                        .iter()
                        .rev()
                        .take(3)
                        .map(|evidence| {
                            format!(
                                "{}={}",
                                evidence.kind,
                                evidence
                                    .passed
                                    .map_or("?".to_string(), |passed| passed.to_string())
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
    snapshot.tasks = if task_sections.is_empty() {
        "No Core conversations yet. Type a message in Agent to start one.".into()
    } else {
        task_sections.join("\n\n")
    };
    match locked.project_mut().processes().list_checked() {
        Ok(items) => {
            snapshot.process_entries = items.iter().map(ProcessRow::from_snapshot).collect();
            snapshot.processes = if items.is_empty() {
                "No managed terminals. Start the detected development command from More.".into()
            } else {
                items
                    .into_iter()
                    .map(|item| {
                        format!(
                            "{} {} · health {} · pid {} · {}\n  {}",
                            if matches!(item.health, crate::development::ProcessHealth::Healthy) {
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
            snapshot.process_entries.clear();
            snapshot.processes = format!("Process state failed: {error}");
        }
    }
    snapshot.lsp = {
        let language = locked.language();
        let servers = language.names().collect::<Vec<_>>();
        let event_count = language.events(0).len();
        if servers.is_empty() {
            "No language server active · diagnostics unavailable".into()
        } else {
            format!("● {} · {} recent events", servers.join(" · "), event_count)
        }
    };
    let _ = locked.tests_mut().poll();
    let test_runs = locked.tests().results().rev().take(32).collect::<Vec<_>>();
    snapshot.tests = if test_runs.is_empty() {
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
    let kernels = locked.kernels().snapshots().cloned().collect::<Vec<_>>();
    snapshot.kernels = if kernels.is_empty() {
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
    let debugger_names = locked
        .debugger_names()
        .map(str::to_string)
        .collect::<Vec<_>>();
    let debug_rows = debugger_names
        .iter()
        .filter_map(|name| {
            locked
                .debugger_mut(name)
                .ok()
                .and_then(|debugger| debugger.snapshot().ok())
                .map(|value| (name.clone(), value))
        })
        .collect::<Vec<_>>();
    snapshot.debug_sessions = debug_rows
        .iter()
        .map(|(name, value)| DebugSessionRow {
            name: name.clone(),
            state: value.state,
            pid: value.adapter_process_id,
            breakpoints: value.breakpoints.values().map(Vec::len).sum::<usize>(),
            watches: value.watches.len(),
        })
        .collect();
    snapshot.debugger = if debug_rows.is_empty() {
        "No debugger sessions. Ctrl-K opens Debug actions.".into()
    } else {
        debug_rows
            .iter()
            .map(|(name, value)| {
                format!(
                    "● {} · {} · pid {} · {} breakpoints · {} watches · {} threads/processes",
                    name,
                    value.state.label(),
                    value.adapter_process_id,
                    value.breakpoints.values().map(Vec::len).sum::<usize>(),
                    value.watches.len(),
                    value.debuggee_processes.len()
                )
            })
            .collect::<Vec<_>>()
            .join("\n")
    };
    snapshot.replay = locked
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
    snapshot.workflow = locked
        .browser()
        .list_workflows()
        .map(|value| crate::tui::projection::workflow(Some(&value)))
        .unwrap_or_else(|error| format!("Workflow state failed: {error}"));
    snapshot.browser = locked
        .browser()
        .state()
        .map(|value| {
            let connected = value
                .get("connected")
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(false);
            let policy = value
                .get("policyPreset")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("development");
            let revision = value
                .get("browserRevision")
                .and_then(serde_json::Value::as_u64)
                .map_or_else(|| "—".into(), |value| value.to_string());
            if connected {
                format!("● connected · policy {policy} · revision {revision}")
            } else {
                format!("○ detached · policy {policy} · browser start required")
            }
        })
        .unwrap_or_else(|error| format!("Browser state failed: {error}"));
    let git = git_projection(&mut locked);
    snapshot.git = git.text;
    snapshot.git_entries = git.entries;
    snapshot.git_branch = git.branch;
    snapshot.git_dirty = git.dirty;
    snapshot.git_ahead = git.ahead;
    snapshot.git_behind = git.behind;
    snapshot.git_conflicts = git.conflicts;
    snapshot.workspace_status = workspace_status_projection(&mut locked);
    let root = locked.root().to_path_buf();
    snapshot.root = root.display().to_string();
    snapshot.project_revision = locked.project().revision();
    snapshot.generation = locked.generation();
    snapshot.skills_count = locked.customization().skills().count();
    snapshot.tools_count = locked.customization().config().tools.len();
    drop(locked);
    snapshot.github = github_probe.probe(&root);
    snapshot.github_review = github_review.review(&root).display();
    snapshot
}

struct GitProjection {
    text: String,
    entries: Vec<crate::git::GitStatusEntry>,
    branch: String,
    dirty: bool,
    ahead: u64,
    behind: u64,
    conflicts: Vec<String>,
}

fn git_projection(workspace: &mut crate::DevelopmentWorkspace) -> GitProjection {
    workspace
        .git()
        .map(|git| match git.status() {
            Ok(status) => {
                let entries = status.sorted_entries();
                let header = format!(
                    "branch {} · ↑{} ↓{} · upstream {}",
                    status.branch.as_deref().unwrap_or("detached"),
                    status.ahead,
                    status.behind,
                    status.upstream.as_deref().unwrap_or("none")
                );
                let lines = entries
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
                let text = if lines.is_empty() {
                    format!("{header}\n✓ working tree clean")
                } else {
                    format!("{header}\n{}", lines.join("\n"))
                };
                GitProjection {
                    text,
                    entries,
                    branch: status.branch.unwrap_or_else(|| "detached".into()),
                    dirty: !status.conflicts.is_empty()
                        || status.ahead > 0
                        || status.behind > 0
                        || status.entries.iter().any(|entry| {
                            entry.untracked
                                || entry.index_status != ' '
                                || entry.worktree_status != ' '
                        }),
                    ahead: status.ahead,
                    behind: status.behind,
                    conflicts: status.conflicts,
                }
            }
            Err(error) => GitProjection {
                text: format!("Git state failed: {error}"),
                entries: Vec::new(),
                branch: String::new(),
                dirty: false,
                ahead: 0,
                behind: 0,
                conflicts: Vec::new(),
            },
        })
        .unwrap_or(GitProjection {
            text: "Not a Git repository".into(),
            entries: Vec::new(),
            branch: String::new(),
            dirty: false,
            ahead: 0,
            behind: 0,
            conflicts: Vec::new(),
        })
}

fn workspace_status_projection(workspace: &mut crate::DevelopmentWorkspace) -> String {
    let detection = workspace.project().detection().clone();
    let agent_count = workspace
        .agents()
        .list()
        .map(|items| items.len())
        .unwrap_or(0);
    let task_count = workspace.tasks().map(|items| items.len()).unwrap_or(0);
    let kernel_count = workspace.kernels().snapshots().count();
    let debugger_count = workspace.debugger_names().count();
    let project_line = if detection.languages.is_empty() {
        "○ project type unknown".to_string()
    } else {
        format!("✓ {} project", detection.languages.join("/"))
    };
    let dev_hint = detection
        .dev_command
        .as_deref()
        .map(|command| format!("● run `{command}`"))
        .unwrap_or_else(|| "○ configure a dev command".into());
    let app_hint = detection
        .browser_url
        .as_deref()
        .map(|url| format!("● open Browser {url}"))
        .unwrap_or_else(|| "○ open Browser after a URL is detected".into());
    format!(
        "WELCOME · {}\n{}\n\n✓ workspace ready\n\nNEXT ACTIONS\n◆ Ctrl-K · guided Glass workspace actions\n◆ / · discover Core commands and Glass workspace actions\n◆ start detected preview server to see its Browser URL\n◆ Enter · open the selected file in Code\n{}\n{}\n\nSTATE\nroot {}\ngeneration {} · project revision {}\nresident: {} workspace agents · {} workspace tasks · {} kernels · {} debuggers",
        detection.git_branch.as_deref().unwrap_or("no branch"),
        project_line,
        dev_hint,
        app_hint,
        workspace.root().display(),
        workspace.generation(),
        workspace.project().revision(),
        agent_count,
        task_count,
        kernel_count,
        debugger_count,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn browser_health_reads_backend_from_the_runtime_state_contract() {
        assert_eq!(
            browser_backend_from_runtime_state(&serde_json::json!({
                "browserBackend": "native",
                "backend": "chromium"
            })),
            Some("native".into())
        );
        assert_eq!(
            browser_backend_from_runtime_state(&serde_json::json!({
                "browserBackend": "chromium"
            })),
            Some("chromium".into())
        );
        assert_eq!(
            browser_backend_from_runtime_state(&serde_json::json!({}),),
            None
        );
    }

    #[test]
    fn core_session_settings_keep_only_the_display_allowlist() {
        let agent = crate::AgentId::parse("agent-0001").unwrap();
        let values = core_session_settings(&[crate::AgentEvent {
            sequence: 1,
            agent_id: agent.clone(),
            timestamp_ms: 0,
            kind: "response".into(),
            payload: serde_json::json!({
                "operation":"hello",
                "ok":true,
                "result":{
                    "provider":"openai",
                    "model":"gpt-test",
                    "maxTokens":8192,
                    "imageInput":true,
                    "toolMode":"mixed",
                    "reasoningBudget":2048,
                    "instructions":"must not surface",
                    "baseUrl":"https://private.invalid",
                    "apiKeyEnv":"PRIVATE_KEY",
                    "cwd":"/private/project"
                }
            }),
        }]);
        assert_eq!(
            values[&agent],
            CoreSessionSettings {
                provider: Some("openai".into()),
                model: Some("gpt-test".into()),
                max_tokens: Some(8192),
                image_input: Some(true),
                tool_mode: Some("mixed".into()),
                reasoning_budget: Some(2048),
                error: None,
            }
        );
    }

    #[test]
    fn core_command_catalogues_keep_latest_session_result_and_errors() {
        let agent = crate::AgentId::parse("agent-0001").unwrap();
        let event = |sequence, payload| crate::AgentEvent {
            sequence,
            agent_id: agent.clone(),
            timestamp_ms: 0,
            kind: "response".into(),
            payload,
        };
        let catalogues = core_command_catalogues(&[
            event(
                1,
                serde_json::json!({
                    "operation":"coreCommands",
                    "ok":true,
                    "result":{"commands":[{"name":"status"}]}
                }),
            ),
            event(
                2,
                serde_json::json!({
                    "operation":"coreCommands",
                    "ok":true,
                    "result":{"commands":[{"name":"status"},{"name":"model"}]}
                }),
            ),
        ]);
        assert_eq!(catalogues[&agent].commands.len(), 2);
        assert_eq!(catalogues[&agent].commands[1]["name"], "model");

        let catalogues = core_command_catalogues(&[event(
            3,
            serde_json::json!({
                "operation":"coreCommands",
                "ok":false,
                "error":{"message":"catalogue unavailable"}
            }),
        )]);
        assert_eq!(
            catalogues[&agent].error.as_deref(),
            Some("catalogue unavailable")
        );
    }

    #[test]
    fn agent_conversation_projection_partitions_events_by_owner() {
        let agent_a = crate::AgentId::parse("agent-0001").unwrap();
        let agent_b = crate::AgentId::parse("agent-0002").unwrap();
        let event = |sequence, agent_id: crate::AgentId, text: &str| crate::AgentEvent {
            sequence,
            agent_id,
            timestamp_ms: 0,
            kind: "message_end".into(),
            payload: serde_json::json!({"message":{"role":"assistant","content":[{"type":"text","text":text}]}}),
        };
        let grouped = agent_conversations(&[
            event(1, agent_a.clone(), "private to A"),
            event(2, agent_b.clone(), "private to B"),
        ]);
        assert!(grouped[&agent_a].text.contains("private to A"));
        assert!(!grouped[&agent_a].text.contains("private to B"));
        assert!(grouped[&agent_b].text.contains("private to B"));
        assert!(!grouped[&agent_b].text.contains("private to A"));
    }

    #[test]
    fn durable_task_graph_snapshot_keeps_only_safe_bounded_fields() {
        let agent_id = crate::AgentId::parse("agent-0001").unwrap();
        let conversation = agent_conversations(&[crate::AgentEvent {
            sequence: 1,
            agent_id: agent_id.clone(),
            timestamp_ms: 0,
            kind: "response".into(),
            payload: serde_json::json!({
                "operation":"coreTaskGraph", "ok":true,
                "result":{"sequence":4,"tasks":[{
                    "id":"generation-4", "kind":"gc.generation", "status":"waiting",
                    "owner":"parent-1", "background":true, "dependencyCount":1,
                    "input":{"text":"must not be projected"},
                    "checkpoint":{"private":"must not be projected"},
                    "outcome":{"private":"must not be projected"}
                }]}
            }),
        }])
        .remove(&agent_id)
        .unwrap();
        let graph = conversation.core_task_graph.unwrap();
        assert_eq!(graph.sequence, Some(4));
        assert_eq!(graph.tasks.len(), 1);
        assert_eq!(graph.tasks[0].id, "generation-4");
        assert_eq!(graph.tasks[0].parent.as_deref(), Some("parent-1"));
        assert!(graph.tasks[0].background);
        assert_eq!(graph.tasks[0].dependency_count, 1);
        assert!(!format!("{graph:?}").contains("must not be projected"));
    }

    #[test]
    fn snapshot_worker_publishes_and_applies_without_blocking_ui() {
        let root = std::env::temp_dir().join(format!("glass-snapshot-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(
            root.join("Cargo.toml"),
            "[package]\nname='x'\nversion='0.1.0'\n",
        )
        .unwrap();
        let store = crate::thread_store::ThreadStore::open(&root).unwrap();
        store
            .create(crate::thread_store::ThreadRecord {
                thread_id: crate::thread_store::random_id("thread").unwrap(),
                title: "reopened conversation".into(),
                role: "builder".into(),
                created_at_ms: 1,
                composer_draft: None,
                follow_up_queue: None,
                send_attempts: Vec::new(),
                #[cfg(windows)]
                send_attempts_opaque: None,
                runs: vec![crate::thread_store::RunRecord {
                    run_id: crate::thread_store::random_id("run").unwrap(),
                    engine: "glass-core".into(),
                    core_session_id: Some("fixture-session-reference".into()),
                    classification: crate::thread_store::RunClassification::RecoveryPending,
                }],
            })
            .unwrap();
        let state = crate::tui::state::DevTuiState::open(
            &root,
            glass_browser::cli::args::TuiLayout::Desktop,
        )
        .unwrap();
        let mut worker = SnapshotWorker::spawn(&state);
        worker.request_refresh();
        let deadline = Instant::now() + Duration::from_secs(10);
        let snapshot = loop {
            if let Some(snapshot) = worker.take_pending() {
                break snapshot;
            }
            assert!(Instant::now() < deadline, "worker never published");
            std::thread::sleep(Duration::from_millis(20));
        };
        assert!(!snapshot.files.is_empty());
        assert!(snapshot.git.contains("branch") || snapshot.git.contains("Git"));
        assert!(snapshot.workspace_status.contains("WELCOME"));
        assert!(!snapshot.tasks.is_empty());
        assert!(!snapshot.processes.is_empty());
        assert!(!snapshot.browser.is_empty());
        assert_eq!(snapshot.agent_states.len(), 1);
        assert_eq!(
            snapshot.agent_states[0].1,
            if cfg!(windows) {
                crate::AgentStatus::Unavailable
            } else {
                crate::AgentStatus::RecoveryPending
            }
        );
        assert_eq!(
            snapshot.agent_chrome[0].status,
            if cfg!(windows) {
                crate::AgentStatus::Unavailable
            } else {
                crate::AgentStatus::RecoveryPending
            }
        );
        assert!(snapshot.agents.contains(if cfg!(windows) {
            "unavailable"
        } else {
            "recovery pending"
        }));
        // The conversation cursor starts at zero and only moves when events exist.
        let cursor = worker.conversation_cursor();
        worker.request_conversation();
        let deadline = Instant::now() + Duration::from_secs(5);
        while worker.is_busy() && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(10));
        }
        assert!(worker.conversation_cursor() >= cursor);
        drop(worker);
        drop(state);
        let _ = std::fs::remove_dir_all(root);
    }
}
