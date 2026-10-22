//! Glass-owned scheduling for independent resident agent sessions.
//!
//! The engine is selected by `GLASS_AGENT_HARNESS`; Glass Core is the default
//! and the Pi SDK runtime is the legacy backend. Glass owns tool execution,
//! approvals, and evidence for both.

use crate::development::{DevelopmentError, DevelopmentResult};
use crate::pi_runtime::PiSessionRequest;
use crate::resident_harness::{AgentToolExecutor, ResidentHarness, ResidentHarnessOptions};
use crate::thread_store::{
    RunClassification, RunRecord, SendAttemptMode, SendAttemptRecord, SendAttemptStatus,
    ThreadRecord, ThreadStore,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, SyncSender, TryRecvError, TrySendError};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const COMMAND_CAPACITY: usize = 32;
const EVENT_CAPACITY: usize = 256;
const HISTORY_CAPACITY: usize = 512;
const AGENT_EVIDENCE_CAPACITY: usize = 64;
const MAX_AGENTS: usize = 32;
const MAX_PROMPT_BYTES: usize = 64 * 1024;

/// Connection metadata for an optional resident-agent broker.
#[derive(Debug, Clone)]
pub struct ResidentAgentBroker {
    /// Unix socket used to reach the broker.
    pub socket: PathBuf,
    /// Authentication token sent to the broker.
    pub token: String,
    /// Workspace identity asserted by the broker.
    pub workspace_id: String,
}

/// Stable identifier for one resident agent; values start with `agent-`.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct AgentId(String);

impl AgentId {
    /// Borrow the stable identifier as a string slice.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Parse an identifier, requiring the `agent-` prefix and a 64-byte bound.
    pub fn parse(value: impl Into<String>) -> DevelopmentResult<Self> {
        let value = value.into();
        if !value.starts_with("agent-")
            || value.len() > 64
            || value.chars().any(|character| character.is_control())
        {
            return Err(DevelopmentError::InvalidInput("invalid agent id".into()));
        }
        Ok(Self(value))
    }
}

/// Lifecycle state of a resident agent.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum AgentStatus {
    /// Accepted but waiting for dependencies or capacity.
    Queued,
    /// Worker process is being started.
    Starting,
    /// A prior Glass run exists but its exact Core session is not yet resumed.
    RecoveryPending,
    /// Prior run has no initialized engine session reference and cannot resume.
    Unavailable,
    /// Worker is ready for a request.
    Idle,
    /// Worker is processing a request.
    Working,
    /// Worker is waiting on external/user input.
    Waiting,
    /// Agent completed successfully.
    Completed,
    /// Agent terminated with an error.
    Failed,
    /// Agent was explicitly cancelled.
    Cancelled,
}

impl AgentStatus {
    fn terminal(self) -> bool {
        matches!(
            self,
            Self::Completed | Self::Failed | Self::Cancelled | Self::Unavailable
        )
    }

    /// Human-readable label for terminal and TUI surfaces.
    pub fn label(self) -> &'static str {
        match self {
            Self::Queued => "queued",
            Self::Starting => "starting",
            Self::RecoveryPending => "recovery pending",
            Self::Unavailable => "unavailable",
            Self::Idle => "idle",
            Self::Working => "working",
            Self::Waiting => "waiting",
            Self::Completed => "completed",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
        }
    }
}

/// Declarative request for one resident agent session.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentSpec {
    /// Human-readable role assigned to the agent.
    pub role: String,
    /// Initial task prompt.
    pub task: String,
    /// Agents that must complete before this agent starts.
    #[serde(default)]
    pub dependencies: Vec<AgentId>,
    /// Optional engine model override.
    pub model: Option<String>,
    /// Optional engine thinking-level override.
    pub thinking: Option<String>,
    /// Optional existing worktree path.
    pub worktree: Option<PathBuf>,
    /// Optional architect session to fork as a subagent.
    pub fork_from: Option<PathBuf>,
    /// Whether governed tool restrictions are disabled.
    #[serde(default)]
    pub unrestricted: bool,
    /// Optional worker runtime limit in seconds.
    pub max_runtime_seconds: Option<u64>,
    /// Optional retained-event limit for this agent.
    pub max_events: Option<u64>,
}

impl AgentSpec {
    /// Build a spec with governed defaults of 3,600 seconds and 10,000 events.
    pub fn new(role: impl Into<String>, task: impl Into<String>) -> Self {
        Self {
            role: role.into(),
            task: task.into(),
            dependencies: Vec::new(),
            model: None,
            thinking: None,
            worktree: None,
            fork_from: None,
            unrestricted: false,
            max_runtime_seconds: Some(3_600),
            max_events: Some(10_000),
        }
    }
}

/// Current serialized projection of an agent and its bounded evidence.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentSnapshot {
    /// Stable agent identifier.
    pub id: AgentId,
    /// Configured role.
    pub role: String,
    /// Configured task prompt.
    pub task: String,
    /// Current lifecycle status.
    pub status: AgentStatus,
    /// Dependency identifiers.
    pub dependencies: Vec<AgentId>,
    /// Effective model, if configured.
    pub model: Option<String>,
    /// Effective thinking level, if configured.
    pub thinking: Option<String>,
    /// Canonical worktree used by the worker.
    pub worktree: PathBuf,
    /// Persisted engine session file, when the runtime has one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_file: Option<String>,
    /// Whether unrestricted tools were enabled.
    pub unrestricted: bool,
    /// Creation timestamp in Unix milliseconds.
    pub created_at_ms: u128,
    /// Start timestamp in Unix milliseconds.
    pub started_at_ms: Option<u128>,
    /// Last projection update timestamp in Unix milliseconds.
    pub updated_at_ms: u128,
    /// Number of events received for this agent.
    pub event_count: u64,
    /// Number of lossy events dropped by the bounded queue.
    pub dropped_event_count: u64,
    /// Last worker error, if any.
    pub last_error: Option<String>,
    /// Last engine response identifier, if any.
    pub last_response_id: Option<String>,
    /// Bounded tool/runtime evidence values.
    pub evidence: Vec<Value>,
}

/// One ordered event emitted by an agent worker.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentEvent {
    /// Monotonic event sequence number.
    pub sequence: u64,
    /// Agent that emitted the event.
    pub agent_id: AgentId,
    /// Event timestamp in Unix milliseconds.
    pub timestamp_ms: u128,
    /// Event kind label.
    pub kind: String,
    /// Structured event payload.
    pub payload: Value,
}

#[derive(Debug)]
enum WorkerCommand {
    Request {
        attempt_id: Option<String>,
        request: PiSessionRequest,
        started: Option<SyncSender<Result<String, String>>>,
    },
    ApproveTool {
        frame_id: String,
        approved: bool,
    },
    Shutdown,
}

#[derive(Debug)]
enum WorkerEvent {
    Ready(AgentId),
    RequestStarted(AgentId, String, bool, Option<String>),
    RequestFailed(AgentId, String, String),
    Engine(AgentId, Value),
    Failed(AgentId, String),
    Stopped(AgentId),
}

struct AgentRecord {
    snapshot: AgentSnapshot,
    spec: AgentSpec,
    command: Option<SyncSender<WorkerCommand>>,
    worker: Option<thread::JoinHandle<()>>,
    awaiting_agent_settle: bool,
    dropped_events: Arc<AtomicU64>,
    thread_id: String,
    run_id: String,
    resume_session_id: Option<String>,
    recovering: bool,
}

struct WorkerRuntime {
    worktree: PathBuf,
    fork_from: Option<PathBuf>,
    local_tool_executor: Option<AgentToolExecutor>,
    sessions_dir: PathBuf,
    broker: Option<ResidentAgentBroker>,
    additional_system_prompt: Option<String>,
    thread_id: String,
    run_id: String,
    thread_store: ThreadStore,
    resume_session_id: Option<String>,
    core_binary: Option<PathBuf>,
    core_binary_args: Vec<String>,
}

/// Registry and scheduler for independent resident agent processes.
///
/// Each running agent owns a distinct engine process and persistent session.
/// The registry only starts dependency-ready work, bounds all queues and
/// retained evidence, and never silently promotes a failed dependency to
/// success. `GLASS_AGENT_HARNESS` selects the engine; Glass Core is the
/// default.
pub(crate) struct AgentRegistry {
    root: PathBuf,
    sessions_dir: PathBuf,
    records: BTreeMap<AgentId, AgentRecord>,
    events_tx: SyncSender<WorkerEvent>,
    events_rx: Receiver<WorkerEvent>,
    history: VecDeque<AgentEvent>,
    local_tool_executor: Option<AgentToolExecutor>,
    next_agent: u64,
    next_event: u64,
    broker: Option<ResidentAgentBroker>,
    additional_system_prompt: Option<String>,
    default_model: Option<String>,
    default_thinking: Option<String>,
    default_unrestricted: bool,
    thread_store: ThreadStore,
    #[cfg(test)]
    core_binary: Option<PathBuf>,
    #[cfg(test)]
    core_binary_args: Vec<String>,
}

impl AgentRegistry {
    /// Create a registry rooted at the canonical project directory.
    pub(crate) fn new(root: impl AsRef<Path>) -> DevelopmentResult<Self> {
        let root = std::fs::canonicalize(root)?;
        let thread_store = ThreadStore::open(&root)?;
        let sessions_dir = root.join(".glass").join("pi-sessions");
        let (events_tx, events_rx) = mpsc::sync_channel(EVENT_CAPACITY);
        let mut registry = Self {
            root: root.clone(),
            sessions_dir,
            records: BTreeMap::new(),
            events_tx,
            events_rx,
            history: VecDeque::new(),
            local_tool_executor: None,
            next_agent: 1,
            next_event: 1,
            broker: None,
            additional_system_prompt: None,
            default_model: None,
            default_thinking: None,
            default_unrestricted: false,
            thread_store,
            #[cfg(test)]
            core_binary: None,
            #[cfg(test)]
            core_binary_args: Vec::new(),
        };
        for thread in registry.thread_store.list()? {
            let Some(run) = thread.runs.last() else {
                continue;
            };
            let id = AgentId(format!("agent-{:04}", registry.next_agent));
            registry.next_agent = registry.next_agent.saturating_add(1);
            let now = now_ms();
            let spec = AgentSpec::new(thread.role.clone(), "");
            let status = if cfg!(windows) {
                AgentStatus::Unavailable
            } else {
                match run.classification {
                    RunClassification::Queued => AgentStatus::Unavailable,
                    RunClassification::Starting if run.core_session_id.is_none() => {
                        AgentStatus::Unavailable
                    }
                    RunClassification::RecoveryPending => AgentStatus::RecoveryPending,
                    RunClassification::Starting if run.core_session_id.is_some() => {
                        AgentStatus::RecoveryPending
                    }
                    RunClassification::Idle
                    | RunClassification::Working
                    | RunClassification::Waiting
                        if run.core_session_id.is_some() =>
                    {
                        AgentStatus::RecoveryPending
                    }
                    RunClassification::Starting
                    | RunClassification::Idle
                    | RunClassification::Working
                    | RunClassification::Waiting => AgentStatus::Unavailable,
                    RunClassification::Completed => AgentStatus::Completed,
                    RunClassification::Failed => AgentStatus::Failed,
                    RunClassification::Cancelled => AgentStatus::Cancelled,
                }
            };
            let last_error = if cfg!(windows) {
                Some("Durable thread recovery is unavailable on Windows until protected registry ACL support is available; registry files inherit workspace ACLs.".into())
            } else {
                match status {
                AgentStatus::RecoveryPending => Some(format!(
                    "This resident session was last known to be {} and needs explicit Core recovery before it can accept messages.",
                    run.classification.label()
                )),
                AgentStatus::Unavailable if run.classification == RunClassification::Queued => Some("This thread was queued but never initialized; it will not resume or replay automatically.".into()),
                AgentStatus::Unavailable => Some("The prior run stopped before an engine session reference was initialized and cannot be resumed.".into()),
                _ => None,
                }
            };
            registry.records.insert(
                id.clone(),
                AgentRecord {
                    snapshot: AgentSnapshot {
                        id: id.clone(),
                        role: thread.role,
                        task: format!("{} · {}", thread.title, status.label()),
                        status,
                        dependencies: Vec::new(),
                        model: None,
                        thinking: None,
                        worktree: root.clone(),
                        session_file: None,
                        unrestricted: false,
                        created_at_ms: thread.created_at_ms,
                        started_at_ms: None,
                        updated_at_ms: now,
                        event_count: 0,
                        dropped_event_count: 0,
                        last_error,
                        last_response_id: None,
                        evidence: Vec::new(),
                    },
                    spec,
                    command: None,
                    worker: None,
                    awaiting_agent_settle: false,
                    dropped_events: Arc::new(AtomicU64::new(0)),
                    thread_id: thread.thread_id,
                    run_id: run.run_id.clone(),
                    resume_session_id: None,
                    recovering: false,
                },
            );
        }
        Ok(registry)
    }

    /// Configure the authenticated broker used for resident-agent requests.
    pub(crate) fn set_resident_broker(
        &mut self,
        broker: ResidentAgentBroker,
    ) -> DevelopmentResult<()> {
        if !broker.socket.is_absolute()
            || broker.socket == Path::new("/")
            || broker.token.is_empty()
            || broker.token.len() > 256
            || broker.workspace_id.is_empty()
            || broker.workspace_id.len() > 128
            || broker
                .workspace_id
                .chars()
                .any(|character| character.is_control())
        {
            return Err(DevelopmentError::InvalidInput(
                "invalid resident agent broker context".into(),
            ));
        }
        self.broker = Some(broker);
        Ok(())
    }
    pub(crate) fn set_local_tool_executor(&mut self, executor: AgentToolExecutor) {
        self.local_tool_executor = Some(executor);
    }

    pub(crate) fn set_additional_system_prompt(
        &mut self,
        prompt: Option<String>,
    ) -> DevelopmentResult<()> {
        if prompt
            .as_ref()
            .is_some_and(|prompt| prompt.len() > 128 * 1024 || prompt.contains('\0'))
        {
            return Err(DevelopmentError::InvalidInput(
                "agent project instructions exceed 128 KiB or contain NUL".into(),
            ));
        }
        self.additional_system_prompt = prompt;
        Ok(())
    }

    /// Active user-global and trusted-project instructions supplied to the engine.
    #[cfg(test)]
    pub fn additional_system_prompt(&self) -> Option<&str> {
        self.additional_system_prompt.as_deref()
    }

    pub(crate) fn set_defaults(
        &mut self,
        model: Option<String>,
        thinking: Option<String>,
    ) -> DevelopmentResult<()> {
        for (label, value) in [("model", &model), ("thinking", &thinking)] {
            if value
                .as_ref()
                .is_some_and(|value| value.is_empty() || value.len() > 256 || value.contains('\0'))
            {
                return Err(DevelopmentError::InvalidInput(format!(
                    "default agent {label} must contain 1..=256 bytes without NUL"
                )));
            }
        }
        self.default_model = model;
        self.default_thinking = thinking;
        Ok(())
    }
    /// Make newly created resident agents unrestricted for this process.
    pub(crate) fn set_default_unrestricted(&mut self, unrestricted: bool) {
        self.default_unrestricted = unrestricted;
    }

    pub fn default_unrestricted(&self) -> bool {
        self.default_unrestricted
    }

    /// Validate and enqueue an agent; dependencies must be known and successful.
    pub(crate) fn create(&mut self, mut spec: AgentSpec) -> DevelopmentResult<AgentId> {
        self.refresh()?;
        if self.default_unrestricted {
            spec.unrestricted = true;
        }
        if spec.model.is_none() {
            spec.model.clone_from(&self.default_model);
        }
        if spec.thinking.is_none() {
            spec.thinking.clone_from(&self.default_thinking);
        }
        validate_spec(&self.root, &self.records, &spec)?;
        if self.records.len() >= MAX_AGENTS {
            return Err(DevelopmentError::Conflict(format!(
                "agent registry reached its {MAX_AGENTS} session limit"
            )));
        }
        let id = AgentId(format!("agent-{:04}", self.next_agent));
        self.next_agent = self
            .next_agent
            .checked_add(1)
            .ok_or_else(|| DevelopmentError::Conflict("agent identifier space exhausted".into()))?;
        let now = now_ms();
        let thread_id = crate::thread_store::random_id("thread")?;
        let run_id = crate::thread_store::random_id("run")?;
        let harness = crate::resident_harness::ResidentHarnessKind::from_env();
        self.thread_store.create(ThreadRecord {
            thread_id: thread_id.clone(),
            title: spec.role.clone(),
            role: spec.role.clone(),
            created_at_ms: now,
            composer_draft: None,
            follow_up_queue: None,
            send_attempts: Vec::new(),
            #[cfg(windows)]
            send_attempts_opaque: None,
            runs: vec![RunRecord {
                run_id: run_id.clone(),
                engine: harness.id().into(),
                core_session_id: None,
                classification: if spec.dependencies.is_empty() {
                    RunClassification::Starting
                } else {
                    RunClassification::Queued
                },
            }],
        })?;
        let worktree = spec.worktree.clone().unwrap_or_else(|| self.root.clone());
        let status = if spec.dependencies.is_empty() {
            AgentStatus::Starting
        } else {
            AgentStatus::Queued
        };
        self.records.insert(
            id.clone(),
            AgentRecord {
                snapshot: AgentSnapshot {
                    id: id.clone(),
                    role: spec.role.clone(),
                    task: spec.task.clone(),
                    status,
                    dependencies: spec.dependencies.clone(),
                    model: spec.model.clone(),
                    thinking: spec.thinking.clone(),
                    worktree,
                    session_file: None,
                    unrestricted: spec.unrestricted,
                    created_at_ms: now,
                    started_at_ms: None,
                    updated_at_ms: now,
                    event_count: 0,
                    dropped_event_count: 0,
                    last_error: None,
                    last_response_id: None,
                    evidence: Vec::new(),
                },
                spec,
                command: None,
                worker: None,
                awaiting_agent_settle: false,
                dropped_events: Arc::new(AtomicU64::new(0)),
                thread_id,
                run_id,
                resume_session_id: None,
                recovering: false,
            },
        );
        if status == AgentStatus::Starting {
            self.spawn(&id)?;
        }
        Ok(id)
    }

    /// Drain worker events, update snapshots, and start newly ready agents.
    pub(crate) fn refresh(&mut self) -> DevelopmentResult<()> {
        loop {
            match self.events_rx.try_recv() {
                Ok(event) => self.apply_worker_event(event)?,
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    return Err(DevelopmentError::Process(
                        "agent scheduler event channel closed".into(),
                    ));
                }
            }
        }
        for record in self.records.values_mut() {
            record.snapshot.dropped_event_count = record.dropped_events.load(Ordering::Relaxed);
        }
        self.reap_finished_workers()?;
        self.enforce_budgets()?;
        self.fail_blocked_dependents()?;
        self.start_ready_agents()?;
        Ok(())
    }

    /// Return one refreshed agent snapshot or a not-found error.
    pub fn snapshot(&mut self, id: &AgentId) -> DevelopmentResult<AgentSnapshot> {
        self.refresh()?;
        let record = self
            .records
            .get(id)
            .ok_or_else(|| DevelopmentError::NotFound(format!("agent {}", id.as_str())))?;
        Ok(record.snapshot.clone())
    }

    #[cfg(unix)]
    pub(crate) fn composer_draft_target(
        &self,
        id: &AgentId,
    ) -> DevelopmentResult<(ThreadStore, String)> {
        let thread_id = self
            .records
            .get(id)
            .map(|record| record.thread_id.clone())
            .ok_or_else(|| DevelopmentError::NotFound(format!("agent {}", id.as_str())))?;
        Ok((self.thread_store.clone(), thread_id))
    }

    pub(crate) fn follow_up_queue_target(
        &self,
        id: &AgentId,
    ) -> DevelopmentResult<(ThreadStore, String, String)> {
        let record = self
            .records
            .get(id)
            .ok_or_else(|| DevelopmentError::NotFound(format!("agent {}", id.as_str())))?;
        Ok((
            self.thread_store.clone(),
            record.thread_id.clone(),
            record.run_id.clone(),
        ))
    }

    /// Return refreshed snapshots for all registered agents.
    pub fn list(&mut self) -> DevelopmentResult<Vec<AgentSnapshot>> {
        self.refresh()?;
        Ok(self
            .records
            .values()
            .map(|record| record.snapshot.clone())
            .collect())
    }

    /// Return retained events at or after the supplied sequence number.
    pub fn history(&mut self, since: u64) -> DevelopmentResult<Vec<AgentEvent>> {
        self.refresh()?;
        Ok(self
            .history
            .iter()
            .filter(|event| event.sequence > since)
            .cloned()
            .collect())
    }

    /// Explicitly reattach a recovery-pending thread to its stored Core session.
    pub(crate) fn resume(&mut self, id: &AgentId) -> DevelopmentResult<()> {
        self.refresh()?;
        let (thread_id, previous_run_id, status) = {
            let record = self.record_mut(id)?;
            (
                record.thread_id.clone(),
                record.run_id.clone(),
                record.snapshot.status,
            )
        };
        if status != AgentStatus::RecoveryPending {
            return Err(DevelopmentError::Conflict(format!(
                "agent {} can only resume while recovery is pending",
                id.as_str()
            )));
        }
        if crate::resident_harness::ResidentHarnessKind::from_env()
            != crate::resident_harness::ResidentHarnessKind::GlassCore
        {
            return Err(DevelopmentError::Conflict(
                "durable thread recovery requires GLASS_AGENT_HARNESS=glass-core".into(),
            ));
        }
        let thread = self
            .thread_store
            .list()?
            .into_iter()
            .find(|thread| thread.thread_id == thread_id)
            .ok_or_else(|| DevelopmentError::NotFound("durable Glass thread".into()))?;
        let prior = thread
            .runs
            .iter()
            .find(|run| run.run_id == previous_run_id)
            .ok_or_else(|| DevelopmentError::NotFound("durable Glass run".into()))?;
        if prior.engine != "glass-core" {
            return Err(DevelopmentError::Conflict(
                "this durable thread was not created by Glass Core".into(),
            ));
        }
        let session_id = prior.core_session_id.clone().ok_or_else(|| {
            DevelopmentError::Conflict("the durable thread has no Core session reference".into())
        })?;
        let new_run_id = crate::thread_store::random_id("run")?;
        self.thread_store.append_run(
            &thread_id,
            RunRecord {
                run_id: new_run_id.clone(),
                engine: "glass-core".into(),
                core_session_id: Some(session_id.clone()),
                classification: RunClassification::RecoveryPending,
            },
        )?;
        {
            let record = self.record_mut(id)?;
            record.run_id = new_run_id.clone();
            record.resume_session_id = Some(session_id);
            record.recovering = true;
            record.snapshot.status = AgentStatus::Starting;
            record.snapshot.last_error = None;
            record.snapshot.started_at_ms = Some(now_ms());
            record.snapshot.updated_at_ms = now_ms();
        }
        if let Err(error) = self.spawn(id) {
            let record = self.record_mut(id)?;
            record.snapshot.status = AgentStatus::RecoveryPending;
            record.snapshot.last_error =
                Some(format!("Core session recovery could not start: {error}"));
            record.recovering = true;
            return Err(error);
        }
        self.record_event(
            id,
            "recovering",
            serde_json::json!({"threadId":thread_id,"runId":new_run_id}),
        );
        Ok(())
    }

    /// Send an initial prompt to an agent, subject to prompt-size bounds.
    pub(crate) fn prompt(
        &mut self,
        id: &AgentId,
        text: impl Into<String>,
    ) -> DevelopmentResult<()> {
        self.prompt_with_context(id, text, None)
    }

    pub(crate) fn prompt_with_context(
        &mut self,
        id: &AgentId,
        text: impl Into<String>,
        context: Option<Value>,
    ) -> DevelopmentResult<()> {
        let text = text.into();
        validate_text("agent prompt", &text, MAX_PROMPT_BYTES)?;
        validate_context_attachment(context.as_ref())?;
        self.send(id, PiSessionRequest::Prompt { text, context }, None)
    }

    pub(crate) fn steer_with_context(
        &mut self,
        id: &AgentId,
        text: impl Into<String>,
        context: Option<Value>,
    ) -> DevelopmentResult<()> {
        let text = text.into();
        validate_text("agent steering", &text, MAX_PROMPT_BYTES)?;
        validate_context_attachment(context.as_ref())?;
        self.send(id, PiSessionRequest::Steer { text, context }, None)
    }

    pub(crate) fn follow_up_with_context(
        &mut self,
        id: &AgentId,
        text: impl Into<String>,
        context: Option<Value>,
    ) -> DevelopmentResult<()> {
        let text = text.into();
        validate_text("agent follow-up", &text, MAX_PROMPT_BYTES)?;
        validate_context_attachment(context.as_ref())?;
        self.send(id, PiSessionRequest::FollowUp { text, context }, None)
    }

    pub(crate) fn send_with_context_and_client_message_id(
        &mut self,
        id: &AgentId,
        mode: &str,
        text: impl Into<String>,
        context: Option<Value>,
        client_message_id: Option<&str>,
    ) -> DevelopmentResult<()> {
        if let Some(client_message_id) = client_message_id
            && (client_message_id.is_empty()
                || client_message_id.len() > 128
                || !client_message_id
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || b"._:-".contains(&byte)))
        {
            return Err(DevelopmentError::InvalidInput(
                "client message ID must be 1-128 ASCII letters, digits, or ._:-".into(),
            ));
        }
        let text = text.into();
        validate_text("agent message", &text, MAX_PROMPT_BYTES)?;
        validate_context_attachment(context.as_ref())?;
        let request = match mode {
            "steer" => PiSessionRequest::Steer { text, context },
            "follow-up" => PiSessionRequest::FollowUp { text, context },
            _ => PiSessionRequest::Prompt { text, context },
        };
        self.send(id, request, client_message_id)
    }

    pub(crate) fn approve_tool(
        &mut self,
        id: &AgentId,
        frame_id: impl Into<String>,
        approved: bool,
    ) -> DevelopmentResult<()> {
        self.refresh()?;
        let frame_id = frame_id.into();
        validate_text("resident approval frame", &frame_id, 256)?;
        let record = self.record_mut(id)?;
        if record.snapshot.status.terminal() {
            return Err(DevelopmentError::Conflict(format!(
                "agent {} is already terminal",
                id.as_str()
            )));
        }
        let sender = record.command.as_ref().ok_or_else(|| {
            DevelopmentError::Conflict(format!("agent {} is still starting", id.as_str()))
        })?;
        sender
            .try_send(WorkerCommand::ApproveTool { frame_id, approved })
            .map_err(|error| match error {
                TrySendError::Full(_) => DevelopmentError::Conflict(format!(
                    "agent {} command queue is full",
                    id.as_str()
                )),
                TrySendError::Disconnected(_) => DevelopmentError::Process(format!(
                    "agent {} command channel closed",
                    id.as_str()
                )),
            })
    }

    pub(crate) fn request(
        &mut self,
        id: &AgentId,
        request: PiSessionRequest,
    ) -> DevelopmentResult<()> {
        self.send(id, request, None)
    }

    pub(crate) fn request_with_id(
        &mut self,
        id: &AgentId,
        request: PiSessionRequest,
    ) -> DevelopmentResult<String> {
        self.refresh()?;
        let (started_tx, started_rx) = mpsc::sync_channel(1);
        {
            let record = self.record_mut(id)?;
            if record.snapshot.status.terminal()
                || matches!(
                    record.snapshot.status,
                    AgentStatus::Queued | AgentStatus::Starting | AgentStatus::RecoveryPending
                )
            {
                return Err(agent_request_blocked(id, record.snapshot.status));
            }
            let sender = record.command.as_ref().ok_or_else(|| {
                DevelopmentError::Conflict(format!("agent {} is still starting", id.as_str()))
            })?;
            sender
                .try_send(WorkerCommand::Request {
                    attempt_id: None,
                    request,
                    started: Some(started_tx),
                })
                .map_err(|error| match error {
                    TrySendError::Full(_) => DevelopmentError::Conflict(format!(
                        "agent {} command queue is full",
                        id.as_str()
                    )),
                    TrySendError::Disconnected(_) => DevelopmentError::Process(format!(
                        "agent {} command channel closed",
                        id.as_str()
                    )),
                })?;
        }
        match started_rx.recv_timeout(Duration::from_secs(15)) {
            Ok(Ok(request_id)) => Ok(request_id),
            Ok(Err(error)) => Err(DevelopmentError::Process(error)),
            Err(mpsc::RecvTimeoutError::Timeout) => Err(DevelopmentError::Conflict(
                "the resident engine did not start the request within 15 seconds".into(),
            )),
            Err(mpsc::RecvTimeoutError::Disconnected) => Err(DevelopmentError::Process(
                "the resident worker stopped before starting the request".into(),
            )),
        }
    }

    /// Mark a non-terminal agent complete after its worker settles.
    pub(crate) fn complete(&mut self, id: &AgentId) -> DevelopmentResult<()> {
        self.refresh()?;
        if self.record_mut(id)?.snapshot.status == AgentStatus::RecoveryPending {
            return Err(DevelopmentError::Conflict(
                "a recovery-pending thread is not a live worker".into(),
            ));
        }
        if self.record_mut(id)?.snapshot.status.terminal() {
            return Err(DevelopmentError::Conflict(format!(
                "agent {} is already terminal",
                id.as_str()
            )));
        }
        if let Some(sender) = self.record_mut(id)?.command.take() {
            let _ = sender.try_send(WorkerCommand::Shutdown);
        }
        self.join_worker(id, AgentStatus::Completed)?;
        self.record_event(id, "completed", Value::Null);
        self.start_ready_agents()?;
        Ok(())
    }

    /// Cancel a non-terminal agent and stop its worker.
    pub(crate) fn cancel(&mut self, id: &AgentId) -> DevelopmentResult<()> {
        self.refresh()?;
        if self.record_mut(id)?.snapshot.status == AgentStatus::RecoveryPending {
            return Err(DevelopmentError::Conflict(
                "a recovery-pending thread is not a live worker".into(),
            ));
        }
        if self.record_mut(id)?.snapshot.status.terminal() {
            return Ok(());
        }
        if let Some(sender) = &self.record_mut(id)?.command {
            let _ = sender.try_send(WorkerCommand::Request {
                attempt_id: None,
                request: PiSessionRequest::Abort,
                started: None,
            });
            let _ = sender.try_send(WorkerCommand::Shutdown);
        }
        self.record_mut(id)?.command = None;
        self.join_worker(id, AgentStatus::Cancelled)?;
        self.record_event(id, "cancelled", Value::Null);
        self.fail_blocked_dependents()?;
        Ok(())
    }
    /// Restart a failed or cancelled session under the same agent identity.
    ///
    /// Interactive callers keep the selected agent id, so retrying after an
    /// aborted or crashed turn should not strand the composer on a terminal
    /// worker. Completed sessions remain terminal by design; callers should
    /// create a new session when they explicitly finished one.
    pub(crate) fn restart(&mut self, id: &AgentId) -> DevelopmentResult<()> {
        self.refresh()?;
        let previous_status = {
            let record = self.record_mut(id)?;
            if !matches!(
                record.snapshot.status,
                AgentStatus::Failed | AgentStatus::Cancelled
            ) {
                return Err(DevelopmentError::Conflict(format!(
                    "agent {} can only restart from failed or cancelled, not {:?}",
                    id.as_str(),
                    record.snapshot.status
                )));
            }
            record.snapshot.status
        };
        // A failure event can be observed just before the worker thread
        // exits. Join it here instead of making the user retry a second time.
        self.join_worker(id, previous_status)?;
        let new_run_id = crate::thread_store::random_id("run")?;
        let engine = crate::resident_harness::ResidentHarnessKind::from_env();
        let thread_id = self.record_mut(id)?.thread_id.clone();
        self.thread_store.append_run(
            &thread_id,
            RunRecord {
                run_id: new_run_id.clone(),
                engine: engine.id().into(),
                core_session_id: None,
                classification: RunClassification::Starting,
            },
        )?;
        {
            let record = self.record_mut(id)?;
            record.run_id = new_run_id;
            record.snapshot.last_error = None;
            record.snapshot.last_response_id = None;
            record.awaiting_agent_settle = false;
        }
        self.spawn(id)?;
        self.record_event(
            id,
            "restarted",
            serde_json::json!({"previousStatus": previous_status.label()}),
        );
        Ok(())
    }

    fn send(
        &mut self,
        id: &AgentId,
        request: PiSessionRequest,
        client_message_id: Option<&str>,
    ) -> DevelopmentResult<()> {
        self.refresh()?;
        let attempt_mode_and_text = if cfg!(unix) {
            match &request {
                PiSessionRequest::Prompt { text, .. } => {
                    Some((SendAttemptMode::Prompt, text.clone()))
                }
                PiSessionRequest::FollowUp { text, .. } => {
                    Some((SendAttemptMode::FollowUp, text.clone()))
                }
                PiSessionRequest::Steer { text, .. } => {
                    Some((SendAttemptMode::Steer, text.clone()))
                }
                _ => None,
            }
        } else {
            None
        };
        let message_id = if matches!(
            &request,
            PiSessionRequest::Prompt { .. }
                | PiSessionRequest::FollowUp { .. }
                | PiSessionRequest::Steer { .. }
        ) {
            Some(match client_message_id {
                Some(message_id) => message_id.to_string(),
                None => crate::thread_store::random_id("message")?,
            })
        } else {
            None
        };
        let mut user_event = match &request {
            PiSessionRequest::Prompt { text, context } => Some(
                serde_json::json!({"text": text, "mode": "prompt", "contextAttached": context.is_some()}),
            ),
            PiSessionRequest::Steer { text, context } => Some(
                serde_json::json!({"text": text, "mode": "steer", "contextAttached": context.is_some()}),
            ),
            PiSessionRequest::FollowUp { text, context } => Some(
                serde_json::json!({"text": text, "mode": "follow-up", "contextAttached": context.is_some()}),
            ),
            _ => None,
        };
        if let (Some(message_id), Some(Value::Object(payload))) =
            (message_id.as_deref(), user_event.as_mut())
        {
            payload.insert(
                "clientMessageId".into(),
                Value::String(message_id.to_string()),
            );
        }
        let (thread_id, run_id, sender) = {
            let record = self.record_mut(id)?;
            if record.snapshot.status.terminal()
                || matches!(
                    record.snapshot.status,
                    AgentStatus::Queued | AgentStatus::Starting | AgentStatus::RecoveryPending
                )
            {
                return Err(agent_request_blocked(id, record.snapshot.status));
            }
            let sender = record.command.as_ref().cloned().ok_or_else(|| {
                DevelopmentError::Conflict(format!("agent {} is still starting", id.as_str()))
            })?;
            (record.thread_id.clone(), record.run_id.clone(), sender)
        };
        let attempt_id = if let (Some((mode, text)), Some(message_id)) =
            (attempt_mode_and_text, message_id.as_ref())
        {
            let attempt_id = crate::thread_store::random_id("attempt")?;
            self.thread_store.create_send_attempt(
                &thread_id,
                SendAttemptRecord {
                    attempt_id: attempt_id.clone(),
                    client_message_id: message_id.clone(),
                    target_run_id: run_id,
                    mode,
                    text,
                    status: SendAttemptStatus::Dispatching,
                    engine_request_id: None,
                    created_at_ms: now_ms(),
                },
            )?;
            Some(attempt_id)
        } else {
            None
        };
        if let Err(error) = sender.try_send(WorkerCommand::Request {
            attempt_id: attempt_id.clone(),
            request,
            started: None,
        }) {
            if let Some(attempt_id) = attempt_id {
                self.thread_store.update_send_attempt(
                    &thread_id,
                    &attempt_id,
                    SendAttemptStatus::Failed,
                    None,
                )?;
            }
            return Err(match error {
                TrySendError::Full(_) => DevelopmentError::Conflict(format!(
                    "agent {} command queue is full",
                    id.as_str()
                )),
                TrySendError::Disconnected(_) => DevelopmentError::Process(format!(
                    "agent {} command channel closed",
                    id.as_str()
                )),
            });
        }
        if let Some(payload) = user_event {
            self.record_event(id, "user", payload);
        }
        Ok(())
    }

    fn spawn(&mut self, id: &AgentId) -> DevelopmentResult<()> {
        let (thread_id, run_id, resume_session_id) = {
            let record = self
                .records
                .get(id)
                .ok_or_else(|| DevelopmentError::NotFound(format!("agent {}", id.as_str())))?;
            (
                record.thread_id.clone(),
                record.run_id.clone(),
                record.resume_session_id.clone(),
            )
        };
        if resume_session_id.is_none() {
            self.thread_store
                .classify_run(&thread_id, &run_id, RunClassification::Starting)?;
        }
        let sessions_dir = self.sessions_dir.clone();
        let events = self.events_tx.clone();
        let broker = self.broker.clone();
        let additional_system_prompt = self.additional_system_prompt.clone();
        let local_tool_executor = self.local_tool_executor.clone();
        let thread_store = self.thread_store.clone();
        #[cfg(test)]
        let core_binary = self.core_binary.clone();
        #[cfg(not(test))]
        let core_binary: Option<PathBuf> = None;
        #[cfg(test)]
        let core_binary_args = self.core_binary_args.clone();
        #[cfg(not(test))]
        let core_binary_args = Vec::new();
        let record = self.record_mut(id)?;
        let dropped_events = Arc::clone(&record.dropped_events);
        let spec = record.spec.clone();
        let worktree = record.snapshot.worktree.clone();
        let fork_from = record.spec.fork_from.clone();
        let thread_id = record.thread_id.clone();
        let run_id = record.run_id.clone();
        let worker_id = id.clone();
        let (commands_tx, commands_rx) = mpsc::sync_channel(COMMAND_CAPACITY);
        record.command = Some(commands_tx);
        record.snapshot.status = AgentStatus::Starting;
        record.snapshot.started_at_ms = Some(now_ms());
        record.snapshot.updated_at_ms = now_ms();
        let worker = thread::Builder::new()
            .name(format!("glass-{}", id.as_str()))
            .spawn(move || {
                run_worker(
                    worker_id,
                    spec,
                    WorkerRuntime {
                        local_tool_executor,
                        worktree,
                        fork_from,
                        sessions_dir,
                        broker,
                        additional_system_prompt,
                        thread_id,
                        run_id,
                        thread_store,
                        resume_session_id,
                        core_binary,
                        core_binary_args,
                    },
                    commands_rx,
                    events,
                    dropped_events,
                )
            })
            .map_err(DevelopmentError::Io)?;
        record.worker = Some(worker);
        self.record_event(id, "starting", Value::Null);
        Ok(())
    }

    fn start_ready_agents(&mut self) -> DevelopmentResult<()> {
        let ready: Vec<_> = self
            .records
            .iter()
            .filter(|(_, record)| record.snapshot.status == AgentStatus::Queued)
            .filter(|(_, record)| {
                record.spec.dependencies.iter().all(|dependency| {
                    self.records.get(dependency).is_some_and(|dependency| {
                        dependency.snapshot.status == AgentStatus::Completed
                    })
                })
            })
            .map(|(id, _)| id.clone())
            .collect();
        for id in ready {
            self.spawn(&id)?;
        }
        Ok(())
    }

    fn fail_blocked_dependents(&mut self) -> DevelopmentResult<()> {
        let terminal_failures: BTreeSet<_> = self
            .records
            .iter()
            .filter(|(_, record)| {
                matches!(
                    record.snapshot.status,
                    AgentStatus::Failed | AgentStatus::Cancelled
                )
            })
            .map(|(id, _)| id.clone())
            .collect();
        let blocked: Vec<_> = self
            .records
            .iter()
            .filter(|(_, record)| record.snapshot.status == AgentStatus::Queued)
            .filter(|(_, record)| {
                record
                    .spec
                    .dependencies
                    .iter()
                    .any(|dependency| terminal_failures.contains(dependency))
            })
            .map(|(id, _)| id.clone())
            .collect();
        for id in blocked {
            let (thread_id, run_id) = {
                let record = self.record_mut(&id)?;
                (record.thread_id.clone(), record.run_id.clone())
            };
            self.thread_store
                .classify_run(&thread_id, &run_id, RunClassification::Failed)?;
            if let Some(record) = self.records.get_mut(&id) {
                record.snapshot.status = AgentStatus::Failed;
                record.snapshot.last_error = Some("a required agent did not complete".into());
                record.snapshot.updated_at_ms = now_ms();
            }
            self.record_event(
                &id,
                "dependencyFailed",
                Value::String("a required agent did not complete".into()),
            );
        }
        Ok(())
    }

    fn enforce_budgets(&mut self) -> DevelopmentResult<()> {
        let now = now_ms();
        let exceeded: Vec<_> = self
            .records
            .iter()
            .filter(|(_, record)| !record.snapshot.status.terminal())
            .filter(|(_, record)| {
                record
                    .spec
                    .max_runtime_seconds
                    .zip(record.snapshot.started_at_ms)
                    .is_some_and(|(seconds, started)| {
                        now.saturating_sub(started) > u128::from(seconds) * 1_000
                    })
                    || record
                        .spec
                        .max_events
                        .is_some_and(|limit| record.snapshot.event_count >= limit)
            })
            .map(|(id, _)| id.clone())
            .collect();
        for id in exceeded {
            let (thread_id, run_id) = {
                let record = self.record_mut(&id)?;
                (record.thread_id.clone(), record.run_id.clone())
            };
            self.thread_store
                .classify_run(&thread_id, &run_id, RunClassification::Failed)?;
            if let Some(record) = self.records.get_mut(&id) {
                if let Some(sender) = record.command.take() {
                    let _ = sender.try_send(WorkerCommand::Request {
                        attempt_id: None,
                        request: PiSessionRequest::Abort,
                        started: None,
                    });
                    let _ = sender.try_send(WorkerCommand::Shutdown);
                }
                record.snapshot.status = AgentStatus::Failed;
                record.snapshot.last_error = Some("agent budget exceeded".into());
                record.snapshot.updated_at_ms = now;
            }
            self.record_event(&id, "budgetExceeded", Value::Null);
        }
        Ok(())
    }

    fn apply_worker_event(&mut self, event: WorkerEvent) -> DevelopmentResult<()> {
        match event {
            WorkerEvent::Ready(id) => {
                self.persist_run_phase(&id, RunClassification::Idle)?;
                if let Some(record) = self.records.get_mut(&id)
                    && !record.snapshot.status.terminal()
                {
                    record.snapshot.status = AgentStatus::Idle;
                    record.snapshot.updated_at_ms = now_ms();
                    record.recovering = false;
                    record.resume_session_id = None;
                }
                self.record_event(&id, "ready", Value::Null);
            }
            WorkerEvent::RequestStarted(id, request_id, waits_for_agent, attempt_id) => {
                if let Some(attempt_id) = attempt_id {
                    self.persist_send_attempt(
                        &id,
                        &attempt_id,
                        if waits_for_agent {
                            SendAttemptStatus::Working
                        } else {
                            SendAttemptStatus::Accepted
                        },
                        Some(request_id.clone()),
                    )?;
                }
                self.persist_run_phase(&id, RunClassification::Working)?;
                if let Some(record) = self.records.get_mut(&id)
                    && !record.snapshot.status.terminal()
                {
                    record.snapshot.last_response_id = Some(request_id.clone());
                    record.snapshot.status = AgentStatus::Working;
                    record.awaiting_agent_settle |= waits_for_agent;
                    record.snapshot.updated_at_ms = now_ms();
                }
                self.record_event(&id, "requestStarted", Value::String(request_id));
            }
            WorkerEvent::RequestFailed(id, attempt_id, error) => {
                self.persist_send_attempt(&id, &attempt_id, SendAttemptStatus::Failed, None)?;
                self.record_event(
                    &id,
                    "requestFailed",
                    serde_json::json!({"attemptId":attempt_id,"error":error}),
                );
            }
            WorkerEvent::Engine(id, value) => {
                let event_type = value.get("type").and_then(Value::as_str);
                let phase = match event_type {
                    Some("waiting") => Some(RunClassification::Waiting),
                    Some("agent_settled") => Some(RunClassification::Idle),
                    Some("response")
                        if self
                            .records
                            .get(&id)
                            .is_some_and(|record| !record.awaiting_agent_settle) =>
                    {
                        Some(RunClassification::Idle)
                    }
                    _ => None,
                };
                if let Some(phase) = phase {
                    self.persist_run_phase(&id, phase)?;
                }
                if event_type == Some("waiting") {
                    self.persist_unambiguous_active_send_attempt(&id, SendAttemptStatus::Waiting)?;
                } else if event_type == Some("response") {
                    self.persist_send_attempt_response(&id, &value)?;
                }
                if let Some(record) = self.records.get_mut(&id) {
                    if let Some(file) = value
                        .pointer("/result/sessionFile")
                        .or_else(|| value.pointer("/state/sessionFile"))
                        .or_else(|| value.get("sessionFile"))
                        .and_then(Value::as_str)
                        .filter(|file| !file.is_empty())
                    {
                        record.snapshot.session_file = Some(file.into());
                    }
                    let event_type = value.get("type").and_then(Value::as_str);
                    if event_type == Some("waiting") && !record.snapshot.status.terminal() {
                        record.awaiting_agent_settle = true;
                        record.snapshot.status = AgentStatus::Waiting;
                    }
                    if event_type == Some("agent_settled") && !record.snapshot.status.terminal() {
                        record.awaiting_agent_settle = false;
                        record.snapshot.status = AgentStatus::Idle;
                    }
                    if event_type == Some("response")
                        && !record.awaiting_agent_settle
                        && !record.snapshot.status.terminal()
                    {
                        record.snapshot.status = AgentStatus::Idle;
                    }
                    record.snapshot.event_count = record.snapshot.event_count.saturating_add(1);
                    record.snapshot.updated_at_ms = now_ms();
                    if record.snapshot.evidence.len() == AGENT_EVIDENCE_CAPACITY {
                        record.snapshot.evidence.remove(0);
                    }
                    record.snapshot.evidence.push(value.clone());
                }
                let kind = value
                    .get("type")
                    .and_then(Value::as_str)
                    .unwrap_or("engineEvent")
                    .to_string();
                self.record_event(&id, &kind, value);
            }
            WorkerEvent::Failed(id, error) => {
                self.mark_inflight_attempts_uncertain(&id)?;
                if let Some(record) = self.records.get(&id) {
                    self.thread_store.classify_run(
                        &record.thread_id,
                        &record.run_id,
                        if record.recovering {
                            RunClassification::RecoveryPending
                        } else {
                            RunClassification::Failed
                        },
                    )?;
                }
                if let Some(record) = self.records.get_mut(&id)
                    && !record.snapshot.status.terminal()
                {
                    record.snapshot.status = if record.recovering {
                        AgentStatus::RecoveryPending
                    } else {
                        AgentStatus::Failed
                    };
                    record.snapshot.last_error = Some(error.clone());
                    record.snapshot.updated_at_ms = now_ms();
                    record.command = None;
                }
                self.record_event(&id, "failed", Value::String(error));
            }
            WorkerEvent::Stopped(id) => {
                if let Some(record) = self.records.get_mut(&id) {
                    record.command = None;
                }
                self.record_event(&id, "stopped", Value::Null);
            }
        }
        Ok(())
    }

    fn join_worker(&mut self, id: &AgentId, terminal: AgentStatus) -> DevelopmentResult<()> {
        let worker = self.record_mut(id)?.worker.take();
        if let Some(worker) = worker
            && worker.join().is_err()
        {
            let error = format!("agent {} worker panicked", id.as_str());
            let (thread_id, run_id) = {
                let record = self.record_mut(id)?;
                (record.thread_id.clone(), record.run_id.clone())
            };
            self.thread_store
                .classify_run(&thread_id, &run_id, RunClassification::Failed)?;
            let record = self.record_mut(id)?;
            record.command = None;
            record.snapshot.status = AgentStatus::Failed;
            record.snapshot.last_error = Some(error.clone());
            record.snapshot.updated_at_ms = now_ms();
            self.record_event(
                id,
                "workerPanicked",
                serde_json::json!({"agentId":id.as_str(),"error":error}),
            );
            return Err(DevelopmentError::Process(error));
        }
        let (thread_id, run_id) = {
            let record = self.record_mut(id)?;
            (record.thread_id.clone(), record.run_id.clone())
        };
        let classification = match terminal {
            AgentStatus::Completed => RunClassification::Completed,
            AgentStatus::Failed => RunClassification::Failed,
            AgentStatus::Cancelled => RunClassification::Cancelled,
            _ => {
                return Err(DevelopmentError::Conflict(
                    "invalid terminal agent status".into(),
                ));
            }
        };
        self.thread_store
            .classify_run(&thread_id, &run_id, classification)?;
        let record = self.record_mut(id)?;
        record.command = None;
        record.snapshot.status = terminal;
        record.snapshot.updated_at_ms = now_ms();
        Ok(())
    }

    fn reap_finished_workers(&mut self) -> DevelopmentResult<()> {
        let finished = self
            .records
            .iter()
            .filter(|(_, record)| {
                record
                    .worker
                    .as_ref()
                    .is_some_and(|worker| worker.is_finished())
            })
            .map(|(id, _)| id.clone())
            .collect::<Vec<_>>();
        for id in finished {
            let worker = self
                .records
                .get_mut(&id)
                .and_then(|record| record.worker.take());
            let panicked = worker.is_some_and(|worker| worker.join().is_err());
            let recovering = self
                .records
                .get(&id)
                .is_some_and(|record| record.recovering);
            let should_fail = panicked
                || self
                    .records
                    .get(&id)
                    .is_some_and(|record| !record.snapshot.status.terminal());
            if should_fail && let Some(record) = self.records.get(&id) {
                self.thread_store.classify_run(
                    &record.thread_id,
                    &record.run_id,
                    if recovering {
                        RunClassification::RecoveryPending
                    } else {
                        RunClassification::Failed
                    },
                )?;
            }
            let mut event = None;
            if let Some(record) = self.records.get_mut(&id) {
                record.command = None;
                if panicked {
                    let error = format!("agent {} worker panicked", id.as_str());
                    record.snapshot.status = if recovering {
                        AgentStatus::RecoveryPending
                    } else {
                        AgentStatus::Failed
                    };
                    record.snapshot.last_error = Some(error.clone());
                    record.snapshot.updated_at_ms = now_ms();
                    event = Some((
                        "workerPanicked",
                        serde_json::json!({
                            "agentId":id.as_str(),
                            "role":record.snapshot.role.clone(),
                            "task":record.snapshot.task.clone(),
                            "error":error,
                        }),
                    ));
                } else if !record.snapshot.status.terminal()
                    && !(recovering && record.snapshot.last_error.is_some())
                {
                    let error = format!(
                        "agent {} worker exited before a terminal outcome",
                        id.as_str()
                    );
                    record.snapshot.status = if recovering {
                        AgentStatus::RecoveryPending
                    } else {
                        AgentStatus::Failed
                    };
                    record.snapshot.last_error = Some(error.clone());
                    record.snapshot.updated_at_ms = now_ms();
                    event = Some(("workerExited", Value::String(error)));
                }
            }
            if let Some((kind, payload)) = event {
                self.record_event(&id, kind, payload);
            }
        }
        Ok(())
    }

    fn persist_run_phase(&self, id: &AgentId, phase: RunClassification) -> DevelopmentResult<()> {
        let record = self
            .records
            .get(id)
            .ok_or_else(|| DevelopmentError::NotFound(format!("agent {}", id.as_str())))?;
        self.thread_store
            .classify_run(&record.thread_id, &record.run_id, phase)
    }

    fn persist_send_attempt(
        &self,
        id: &AgentId,
        attempt_id: &str,
        status: SendAttemptStatus,
        engine_request_id: Option<String>,
    ) -> DevelopmentResult<()> {
        if !cfg!(unix) {
            return Ok(());
        }
        let record = self
            .records
            .get(id)
            .ok_or_else(|| DevelopmentError::NotFound(format!("agent {}", id.as_str())))?;
        self.thread_store.update_send_attempt(
            &record.thread_id,
            attempt_id,
            status,
            engine_request_id,
        )
    }

    fn persist_unambiguous_active_send_attempt(
        &self,
        id: &AgentId,
        status: SendAttemptStatus,
    ) -> DevelopmentResult<()> {
        let record = self
            .records
            .get(id)
            .ok_or_else(|| DevelopmentError::NotFound(format!("agent {}", id.as_str())))?;
        let targets = self
            .thread_store
            .list()?
            .into_iter()
            .find(|thread| thread.thread_id == record.thread_id)
            .map(|thread| {
                thread
                    .send_attempts
                    .into_iter()
                    .filter(|attempt| {
                        attempt.target_run_id == record.run_id
                            && matches!(
                                attempt.status,
                                SendAttemptStatus::Accepted
                                    | SendAttemptStatus::Working
                                    | SendAttemptStatus::Waiting
                            )
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        if let [attempt] = targets.as_slice() {
            self.persist_send_attempt(id, &attempt.attempt_id, status, None)?;
        }
        Ok(())
    }

    fn persist_send_attempt_response(
        &self,
        id: &AgentId,
        response: &Value,
    ) -> DevelopmentResult<()> {
        let Some(request_id) = response.get("id").and_then(Value::as_str) else {
            return Ok(());
        };
        let record = self
            .records
            .get(id)
            .ok_or_else(|| DevelopmentError::NotFound(format!("agent {}", id.as_str())))?;
        let attempt = self
            .thread_store
            .send_attempts(&record.thread_id)?
            .into_iter()
            .find(|attempt| {
                attempt.engine_request_id.as_deref() == Some(request_id)
                    && matches!(
                        attempt.status,
                        SendAttemptStatus::Accepted
                            | SendAttemptStatus::Working
                            | SendAttemptStatus::Waiting
                    )
            });
        if let Some(attempt) = attempt {
            let status = if response.get("ok").and_then(Value::as_bool) == Some(true) {
                SendAttemptStatus::Settled
            } else {
                SendAttemptStatus::Failed
            };
            self.persist_send_attempt(id, &attempt.attempt_id, status, None)?;
        }
        Ok(())
    }

    fn mark_inflight_attempts_uncertain(&self, id: &AgentId) -> DevelopmentResult<()> {
        let Some(record) = self.records.get(id) else {
            return Ok(());
        };
        let thread_id = record.thread_id.clone();
        let run_id = record.run_id.clone();
        let attempts = self
            .thread_store
            .list()?
            .into_iter()
            .find(|thread| thread.thread_id == thread_id)
            .map(|thread| thread.send_attempts)
            .unwrap_or_default();
        for attempt in attempts.into_iter().filter(|attempt| {
            attempt.target_run_id == run_id
                && matches!(
                    attempt.status,
                    SendAttemptStatus::Dispatching
                        | SendAttemptStatus::Accepted
                        | SendAttemptStatus::Working
                        | SendAttemptStatus::Waiting
                )
        }) {
            self.persist_send_attempt(id, &attempt.attempt_id, SendAttemptStatus::Uncertain, None)?;
        }
        Ok(())
    }

    fn record_event(&mut self, id: &AgentId, kind: &str, payload: Value) {
        if self.history.len() == HISTORY_CAPACITY {
            self.history.pop_front();
        }
        self.history.push_back(AgentEvent {
            sequence: self.next_event,
            agent_id: id.clone(),
            timestamp_ms: now_ms(),
            kind: kind.into(),
            payload,
        });
        self.next_event = self.next_event.saturating_add(1);
    }

    fn record_mut(&mut self, id: &AgentId) -> DevelopmentResult<&mut AgentRecord> {
        self.records
            .get_mut(id)
            .ok_or_else(|| DevelopmentError::NotFound(format!("agent {}", id.as_str())))
    }
}

impl Drop for AgentRegistry {
    fn drop(&mut self) {
        let mut workers = Vec::new();
        for (id, record) in &mut self.records {
            if let Some(sender) = record.command.take() {
                let _ = sender.try_send(WorkerCommand::Shutdown);
            }
            if let Some(worker) = record.worker.take() {
                workers.push((id.clone(), worker));
            }
        }
        for (id, worker) in workers {
            if worker.join().is_err() {
                tracing::error!(agent_id = %id.as_str(), "agent worker panicked during registry drop");
            }
        }
    }
}

fn run_worker(
    id: AgentId,
    spec: AgentSpec,
    runtime: WorkerRuntime,
    commands: Receiver<WorkerCommand>,
    events: SyncSender<WorkerEvent>,
    dropped_events: Arc<AtomicU64>,
) {
    let options = ResidentHarnessOptions {
        unrestricted: spec.unrestricted,
        session_dir: runtime.sessions_dir,
        name: Some(format!("{}: {}", spec.role, id.as_str())),
        model: spec.model,
        thinking: spec.thinking,
        broker: runtime.broker,
        local_tool_executor: runtime.local_tool_executor,
        additional_system_prompt: runtime.additional_system_prompt,
        resume: false,
        resume_session_id: runtime.resume_session_id,
        binary: runtime.core_binary,
        binary_args: runtime.core_binary_args,
        fork_from: runtime.fork_from,
    };
    let mut harness = match ResidentHarness::spawn(
        crate::resident_harness::ResidentHarnessKind::from_env(),
        &runtime.worktree,
        options,
    ) {
        Ok(harness) => harness,
        Err(error) => {
            send_critical_worker_event(&events, WorkerEvent::Failed(id, error.to_string()));
            return;
        }
    };
    let session_id = harness.core_session_id().map(str::to_owned);
    for startup_event in harness.take_startup_events() {
        send_critical_worker_event(&events, WorkerEvent::Engine(id.clone(), startup_event));
    }
    if let Err(error) = announce_worker_ready(
        &events,
        id.clone(),
        &runtime.thread_store,
        &runtime.thread_id,
        &runtime.run_id,
        session_id.as_deref(),
    ) {
        send_critical_worker_event(
            &events,
            WorkerEvent::Failed(
                id,
                format!("failed to persist Core session ID before accepting prompts: {error}"),
            ),
        );
        return;
    }
    loop {
        loop {
            match commands.try_recv() {
                Ok(WorkerCommand::Request {
                    attempt_id,
                    request,
                    started,
                }) => {
                    let waits_for_agent = matches!(
                        request,
                        PiSessionRequest::Prompt { .. }
                            | PiSessionRequest::FollowUp { .. }
                            | PiSessionRequest::Complete { .. }
                    );
                    match harness.start_request(request) {
                        Ok(request_id) => {
                            if let Some(started) = started {
                                let _ = started.send(Ok(request_id.clone()));
                            }
                            send_critical_worker_event(
                                &events,
                                WorkerEvent::RequestStarted(
                                    id.clone(),
                                    request_id,
                                    waits_for_agent,
                                    attempt_id,
                                ),
                            );
                        }
                        Err(error) => {
                            if let Some(started) = started {
                                let _ = started.send(Err(error.to_string()));
                            }
                            if let Some(attempt_id) = attempt_id {
                                send_critical_worker_event(
                                    &events,
                                    WorkerEvent::RequestFailed(
                                        id.clone(),
                                        attempt_id,
                                        error.to_string(),
                                    ),
                                );
                            }
                            send_critical_worker_event(
                                &events,
                                WorkerEvent::Failed(id.clone(), error.to_string()),
                            );
                            return;
                        }
                    }
                }
                Ok(WorkerCommand::ApproveTool { frame_id, approved }) => {
                    if let Err(error) = harness.resolve_tool_approval(&frame_id, approved) {
                        send_critical_worker_event(
                            &events,
                            WorkerEvent::Engine(
                                id.clone(),
                                serde_json::json!({
                                    "type":"glass_tool_approval_resolved",
                                    "frameId":frame_id,
                                    "approved":approved,
                                    "ok":false,
                                    "error":error.to_string(),
                                }),
                            ),
                        );
                    }
                }
                Ok(WorkerCommand::Shutdown) | Err(TryRecvError::Disconnected) => {
                    send_critical_worker_event(&events, WorkerEvent::Stopped(id));
                    return;
                }
                Err(TryRecvError::Empty) => break,
            }
        }
        match harness.recv_event_timeout(Duration::from_millis(25)) {
            Ok(Some(value)) => send_lossy_worker_event(
                &events,
                WorkerEvent::Engine(id.clone(), value),
                &dropped_events,
            ),
            Ok(None) => {}
            Err(error) => {
                send_critical_worker_event(&events, WorkerEvent::Failed(id, error.to_string()));
                return;
            }
        }
    }
}

fn announce_worker_ready(
    events: &SyncSender<WorkerEvent>,
    id: AgentId,
    store: &ThreadStore,
    thread_id: &str,
    run_id: &str,
    session_id: Option<&str>,
) -> DevelopmentResult<()> {
    if let Some(session_id) = session_id {
        store.set_session(thread_id, run_id, session_id)?;
    }
    send_critical_worker_event(events, WorkerEvent::Ready(id));
    Ok(())
}

fn send_lossy_worker_event(
    events: &SyncSender<WorkerEvent>,
    event: WorkerEvent,
    dropped_events: &AtomicU64,
) {
    if matches!(events.try_send(event), Err(TrySendError::Full(_))) {
        dropped_events.fetch_add(1, Ordering::Relaxed);
    }
}

fn send_critical_worker_event(events: &SyncSender<WorkerEvent>, event: WorkerEvent) {
    let _ = events.send(event);
}

fn agent_request_blocked(id: &AgentId, status: AgentStatus) -> DevelopmentError {
    let reason = match status {
        AgentStatus::RecoveryPending => "explicit Core session recovery is required",
        AgentStatus::Unavailable => "the prior run was never initialized and will not replay",
        _ => "the agent is not ready to accept requests",
    };
    DevelopmentError::Conflict(format!(
        "agent {} cannot accept requests while {:?}: {reason}",
        id.as_str(),
        status
    ))
}

fn validate_spec(
    root: &Path,
    records: &BTreeMap<AgentId, AgentRecord>,
    spec: &AgentSpec,
) -> DevelopmentResult<()> {
    validate_text("agent role", &spec.role, 128)?;
    validate_text("agent task", &spec.task, MAX_PROMPT_BYTES)?;
    if spec.dependencies.len() > MAX_AGENTS {
        return Err(DevelopmentError::InvalidInput(
            "agent dependency list is too large".into(),
        ));
    }
    let mut unique = BTreeSet::new();
    for dependency in &spec.dependencies {
        if !unique.insert(dependency) {
            return Err(DevelopmentError::InvalidInput(format!(
                "duplicate dependency {}",
                dependency.as_str()
            )));
        }
        if !records.contains_key(dependency) {
            return Err(DevelopmentError::NotFound(format!(
                "dependency agent {}",
                dependency.as_str()
            )));
        }
    }
    if let Some(worktree) = &spec.worktree {
        let worktree = std::fs::canonicalize(worktree)?;
        if worktree == Path::new("/")
            || (!worktree.starts_with(root) && !worktree.join(".git").exists())
        {
            return Err(DevelopmentError::PathOutsideWorkspace(worktree));
        }
    }
    if let Some(path) = &spec.fork_from {
        let path = std::fs::canonicalize(path)?;
        if path == Path::new("/") || !path.starts_with(root) {
            return Err(DevelopmentError::PathOutsideWorkspace(path));
        }
    }
    if spec.max_runtime_seconds == Some(0) || spec.max_events == Some(0) {
        return Err(DevelopmentError::InvalidInput(
            "agent budgets must be greater than zero".into(),
        ));
    }
    Ok(())
}

fn validate_text(description: &str, text: &str, limit: usize) -> DevelopmentResult<()> {
    if text.trim().is_empty() || text.len() > limit || text.contains('\0') {
        return Err(DevelopmentError::InvalidInput(format!(
            "{description} must contain 1..={limit} bytes without NUL"
        )));
    }
    Ok(())
}

fn validate_context_attachment(context: Option<&Value>) -> DevelopmentResult<()> {
    let Some(context) = context else {
        return Ok(());
    };
    if !context.is_object() {
        return Err(DevelopmentError::InvalidInput(
            "agent context attachment must be a JSON object".into(),
        ));
    }
    let size = serde_json::to_vec(context)
        .map_err(|error| DevelopmentError::Serialization(error.to_string()))?
        .len();
    if size > 64 * 1024 {
        return Err(DevelopmentError::InvalidInput(
            "agent context attachment exceeds 64 KiB".into(),
        ));
    }
    Ok(())
}

fn now_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT_ROOT: AtomicU64 = AtomicU64::new(0);

    fn test_root() -> PathBuf {
        let sequence = NEXT_ROOT.fetch_add(1, Ordering::Relaxed);
        let root = std::env::temp_dir().join(format!(
            "glass-agent-registry-{}-{sequence}",
            std::process::id()
        ));
        std::fs::create_dir_all(&root).unwrap();
        root
    }

    #[cfg(unix)]
    fn configure_fake_core(registry: &mut AgentRegistry, mode: &str) {
        registry.core_binary = std::env::split_paths(&std::env::var_os("PATH").unwrap())
            .map(|directory| directory.join("python3"))
            .find(|candidate| candidate.is_file());
        assert!(registry.core_binary.is_some(), "python3 is required");
        registry.core_binary_args = vec![
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("tests/fixtures/fake_glass_core_runtime.py")
                .display()
                .to_string(),
            mode.into(),
        ];
    }

    fn wait_for_agent(
        registry: &mut AgentRegistry,
        id: &AgentId,
        status: AgentStatus,
    ) -> AgentSnapshot {
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        loop {
            registry.refresh().unwrap();
            let snapshot = registry.snapshot(id).unwrap();
            if snapshot.status == status {
                return snapshot;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "agent did not reach {status:?}; got {:?} ({:?})",
                snapshot.status,
                snapshot.last_error
            );
            thread::sleep(Duration::from_millis(10));
        }
    }

    #[test]
    fn lossy_worker_queue_reports_dropped_events() {
        let (sender, _receiver) = mpsc::sync_channel(1);
        let id = AgentId::parse("agent-overflow").unwrap();
        sender.send(WorkerEvent::Ready(id.clone())).unwrap();
        let dropped = AtomicU64::new(0);
        send_lossy_worker_event(
            &sender,
            WorkerEvent::Engine(id, serde_json::json!({"type":"delta"})),
            &dropped,
        );
        assert_eq!(dropped.load(Ordering::Relaxed), 1);
    }

    #[cfg(unix)]
    #[test]
    fn reopened_thread_recovers_exact_session_hydrates_before_ready_and_never_replays() {
        let root = test_root();
        let mut first = AgentRegistry::new(&root).unwrap();
        configure_fake_core(&mut first, "exact-resume");
        let id = first
            .create(AgentSpec::new("builder", "initial session"))
            .unwrap();
        wait_for_agent(&mut first, &id, AgentStatus::Idle);
        first.prompt(&id, "first explicit prompt").unwrap();
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        loop {
            first.refresh().unwrap();
            if first
                .history(0)
                .unwrap()
                .iter()
                .any(|event| event.kind == "agent_settled")
            {
                break;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "initial turn did not settle"
            );
            thread::sleep(Duration::from_millis(10));
        }
        let original = first.thread_store.list().unwrap().remove(0);
        let original_run = original.runs.last().unwrap().run_id.clone();
        let session_id = original
            .runs
            .last()
            .unwrap()
            .core_session_id
            .clone()
            .unwrap();
        let thread_id = original.thread_id.clone();
        let pending_attempt_id = crate::thread_store::random_id("attempt").unwrap();
        first
            .thread_store
            .create_send_attempt(
                &thread_id,
                SendAttemptRecord {
                    attempt_id: pending_attempt_id.clone(),
                    client_message_id: "message-recovery-check".into(),
                    target_run_id: original_run.clone(),
                    mode: SendAttemptMode::FollowUp,
                    text: "This request was sent near process shutdown".into(),
                    status: SendAttemptStatus::Dispatching,
                    engine_request_id: None,
                    created_at_ms: now_ms(),
                },
            )
            .unwrap();
        drop(first);

        let mut recovered = AgentRegistry::new(&root).unwrap();
        configure_fake_core(&mut recovered, "missing-session");
        let recovered_id = recovered.list().unwrap().remove(0).id;
        assert_eq!(
            recovered.snapshot(&recovered_id).unwrap().status,
            AgentStatus::RecoveryPending
        );
        assert_eq!(
            recovered
                .thread_store
                .send_attempts(&thread_id)
                .unwrap()
                .into_iter()
                .find(|attempt| attempt.attempt_id == pending_attempt_id)
                .unwrap()
                .status,
            SendAttemptStatus::Uncertain
        );
        recovered.resume(&recovered_id).unwrap();
        assert!(recovered.prompt(&recovered_id, "must wait").is_err());
        let failed = wait_for_agent(&mut recovered, &recovered_id, AgentStatus::RecoveryPending);
        assert!(
            failed
                .last_error
                .as_deref()
                .unwrap_or_default()
                .contains("session not found")
        );
        let after_failure = recovered.thread_store.list().unwrap().remove(0);
        assert_eq!(after_failure.thread_id, thread_id);
        assert_eq!(
            after_failure
                .runs
                .last()
                .unwrap()
                .core_session_id
                .as_deref(),
            Some(session_id.as_str())
        );
        assert_ne!(after_failure.runs.last().unwrap().run_id, original_run);
        let request_log = root.join(".glass/pi-sessions/fixture-requests.jsonl");
        let first_log = std::fs::read_to_string(&request_log).unwrap();
        assert_eq!(first_log.matches("turn/prompt").count(), 1);
        assert_eq!(first_log.matches("session/new").count(), 1);

        configure_fake_core(&mut recovered, "exact-resume");
        recovered.resume(&recovered_id).unwrap();
        assert!(
            recovered
                .prompt(&recovered_id, "still initializing")
                .is_err()
        );
        wait_for_agent(&mut recovered, &recovered_id, AgentStatus::Idle);
        let resumed = recovered.thread_store.list().unwrap().remove(0);
        assert_eq!(resumed.thread_id, thread_id);
        assert_eq!(
            resumed.runs.last().unwrap().core_session_id.as_deref(),
            Some(session_id.as_str())
        );
        assert_ne!(
            resumed.runs.last().unwrap().run_id,
            after_failure.runs.last().unwrap().run_id
        );
        let events = recovered.history(0).unwrap();
        let user_at = events
            .iter()
            .position(|event| event.kind == "user")
            .unwrap();
        let answer_at = events
            .iter()
            .position(|event| event.kind == "message_end")
            .unwrap();
        let ready_at = events
            .iter()
            .position(|event| event.kind == "ready")
            .unwrap();
        assert!(user_at < answer_at && answer_at < ready_at, "{events:?}");
        assert_eq!(events[user_at].payload["text"], "prior question");
        assert_eq!(events[answer_at].payload["text"], "prior answer");

        let before_send = std::fs::read_to_string(&request_log).unwrap();
        assert_eq!(before_send.matches("turn/prompt").count(), 1);
        assert_eq!(before_send.matches("session/new").count(), 1);
        recovered
            .prompt(&recovered_id, "second explicit prompt")
            .unwrap();
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        loop {
            recovered.refresh().unwrap();
            let events = recovered.history(0).unwrap();
            if events
                .iter()
                .filter(|event| event.kind == "agent_settled")
                .count()
                >= 1
            {
                break;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "post-recovery turn did not settle"
            );
            thread::sleep(Duration::from_millis(10));
        }
        let all_requests = std::fs::read_to_string(request_log).unwrap();
        assert_eq!(all_requests.matches("turn/prompt").count(), 2);
        assert_eq!(all_requests.matches("session/new").count(), 1);
        drop(recovered);
        let _ = std::fs::remove_dir_all(root);
    }

    fn insert_test_worker(
        registry: &mut AgentRegistry,
        id: AgentId,
        command: SyncSender<WorkerCommand>,
        worker: thread::JoinHandle<()>,
    ) {
        let spec = AgentSpec::new("lifecycle-test", "prove worker ownership");
        let now = now_ms();
        let thread_id = crate::thread_store::random_id("thread").unwrap();
        let run_id = crate::thread_store::random_id("run").unwrap();
        registry
            .thread_store
            .create(ThreadRecord {
                thread_id: thread_id.clone(),
                title: spec.role.clone(),
                role: spec.role.clone(),
                created_at_ms: now,
                composer_draft: None,
                follow_up_queue: None,
                send_attempts: Vec::new(),
                #[cfg(windows)]
                send_attempts_opaque: None,
                runs: vec![RunRecord {
                    run_id: run_id.clone(),
                    engine: "glass-core".into(),
                    core_session_id: Some("fixture".into()),
                    classification: RunClassification::RecoveryPending,
                }],
            })
            .unwrap();
        registry.records.insert(
            id.clone(),
            AgentRecord {
                snapshot: AgentSnapshot {
                    id,
                    role: spec.role.clone(),
                    task: spec.task.clone(),
                    status: AgentStatus::Working,
                    dependencies: Vec::new(),
                    model: None,
                    thinking: None,
                    worktree: registry.root.clone(),
                    session_file: None,
                    unrestricted: false,
                    created_at_ms: now,
                    started_at_ms: Some(now),
                    updated_at_ms: now,
                    event_count: 0,
                    dropped_event_count: 0,
                    last_error: None,
                    last_response_id: None,
                    evidence: Vec::new(),
                },
                spec,
                command: Some(command),
                worker: Some(worker),
                awaiting_agent_settle: false,
                dropped_events: Arc::new(AtomicU64::new(0)),
                thread_id,
                run_id,
                resume_session_id: None,
                recovering: false,
            },
        );
    }

    #[test]
    fn request_with_id_returns_the_workers_pi_request_identifier() {
        let root = test_root();
        let mut registry = AgentRegistry::new(&root).unwrap();
        let id = AgentId::parse("agent-request-id-test").unwrap();
        let (command, commands) = mpsc::sync_channel(COMMAND_CAPACITY);
        let worker = thread::spawn(move || {
            while let Ok(command) = commands.recv() {
                match command {
                    WorkerCommand::Request {
                        request: PiSessionRequest::Complete { .. },
                        started: Some(started),
                        ..
                    } => {
                        started.send(Ok("glass-request-17".into())).unwrap();
                    }
                    WorkerCommand::Shutdown => break,
                    _ => {}
                }
            }
        });
        insert_test_worker(&mut registry, id.clone(), command, worker);

        let request_id = registry
            .request_with_id(
                &id,
                PiSessionRequest::Complete {
                    prefix: "fn main() {".into(),
                    suffix: "}".into(),
                },
            )
            .unwrap();
        assert_eq!(request_id, "glass-request-17");
        registry.cancel(&id).unwrap();
        drop(registry);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn reopened_thread_rows_are_truthful_and_never_accept_prompts() {
        let root = test_root();
        let registry = AgentRegistry::new(&root).unwrap();
        let mut rows = Vec::new();
        for (title, classification, session_id) in [
            (
                "prior active",
                RunClassification::RecoveryPending,
                Some("actual-core-session".to_string()),
            ),
            (
                "prior idle",
                RunClassification::Idle,
                Some("idle-core-session".to_string()),
            ),
            (
                "prior working",
                RunClassification::Working,
                Some("working-core-session".to_string()),
            ),
            (
                "prior waiting",
                RunClassification::Waiting,
                Some("waiting-core-session".to_string()),
            ),
            ("not initialized", RunClassification::Starting, None),
            (
                "waiting in dependency queue",
                RunClassification::Queued,
                None,
            ),
            (
                "finished",
                RunClassification::Completed,
                Some("finished-session".to_string()),
            ),
        ] {
            rows.push(ThreadRecord {
                thread_id: crate::thread_store::random_id("thread").unwrap(),
                title: title.into(),
                role: "builder".into(),
                created_at_ms: 1,
                composer_draft: None,
                follow_up_queue: None,
                send_attempts: Vec::new(),
                #[cfg(windows)]
                send_attempts_opaque: None,
                runs: vec![RunRecord {
                    run_id: crate::thread_store::random_id("run").unwrap(),
                    engine: "glass-core".into(),
                    core_session_id: session_id,
                    classification,
                }],
            });
        }
        for row in rows {
            registry.thread_store.create(row).unwrap();
        }
        drop(registry);
        let mut reopened = AgentRegistry::new(&root).unwrap();
        let snapshots = reopened.list().unwrap();
        assert_eq!(snapshots.len(), 7);
        if cfg!(windows) {
            assert!(
                snapshots
                    .iter()
                    .all(|snapshot| snapshot.status == AgentStatus::Unavailable)
            );
            assert!(snapshots.iter().all(|snapshot| {
                snapshot
                    .last_error
                    .as_deref()
                    .is_some_and(|error| error.contains("workspace ACLs"))
            }));
            drop(reopened);
            std::fs::remove_dir_all(root).unwrap();
            return;
        }
        assert_eq!(
            snapshots
                .iter()
                .filter(|s| s.status == AgentStatus::RecoveryPending)
                .count(),
            4
        );
        assert_eq!(
            snapshots
                .iter()
                .filter(|s| s.status == AgentStatus::Unavailable)
                .count(),
            2
        );
        assert_eq!(
            snapshots
                .iter()
                .filter(|s| s.status == AgentStatus::Completed)
                .count(),
            1
        );
        let pending = snapshots
            .iter()
            .find(|s| s.status == AgentStatus::RecoveryPending)
            .unwrap()
            .id
            .clone();
        let err = reopened
            .request(
                &pending,
                PiSessionRequest::Prompt {
                    text: "must not replay".into(),
                    context: None,
                },
            )
            .unwrap_err();
        assert!(err.to_string().contains("explicit Core session recovery"));
        let never_started = snapshots
            .iter()
            .find(|s| s.task.contains("waiting in dependency queue"))
            .unwrap()
            .id
            .clone();
        let err = reopened
            .request(
                &never_started,
                PiSessionRequest::Prompt {
                    text: "do not replay queued task".into(),
                    context: None,
                },
            )
            .unwrap_err();
        assert!(err.to_string().contains("will not replay"));
        assert!(
            reopened
                .history(0)
                .unwrap()
                .iter()
                .all(|event| event.kind != "user")
        );
        drop(reopened);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn worker_lifecycle_phases_are_durable_and_waiting_is_visible() {
        let root = test_root();
        let mut registry = AgentRegistry::new(&root).unwrap();
        let id = AgentId::parse("agent-durable-phases").unwrap();
        let (command, commands) = mpsc::sync_channel(COMMAND_CAPACITY);
        let worker = thread::spawn(move || {
            while let Ok(command) = commands.recv() {
                if matches!(command, WorkerCommand::Shutdown) {
                    break;
                }
            }
        });
        insert_test_worker(&mut registry, id.clone(), command, worker);
        let run = |registry: &AgentRegistry| {
            let record = registry.records.get(&id).unwrap();
            registry
                .thread_store
                .list()
                .unwrap()
                .into_iter()
                .find(|thread| thread.thread_id == record.thread_id)
                .unwrap()
                .runs
                .into_iter()
                .find(|run| run.run_id == record.run_id)
                .unwrap()
        };

        registry
            .apply_worker_event(WorkerEvent::Ready(id.clone()))
            .unwrap();
        assert_eq!(run(&registry).classification, RunClassification::Idle);
        registry
            .apply_worker_event(WorkerEvent::RequestStarted(
                id.clone(),
                "request-1".into(),
                true,
                None,
            ))
            .unwrap();
        assert_eq!(run(&registry).classification, RunClassification::Working);
        registry
            .apply_worker_event(WorkerEvent::Engine(
                id.clone(),
                serde_json::json!({"type":"waiting"}),
            ))
            .unwrap();
        assert_eq!(registry.records[&id].snapshot.status, AgentStatus::Waiting);
        assert_eq!(run(&registry).classification, RunClassification::Waiting);
        registry
            .apply_worker_event(WorkerEvent::Engine(
                id.clone(),
                serde_json::json!({"type":"agent_settled"}),
            ))
            .unwrap();
        assert_eq!(registry.records[&id].snapshot.status, AgentStatus::Idle);
        assert_eq!(run(&registry).classification, RunClassification::Idle);

        registry.cancel(&id).unwrap();
        drop(registry);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn sends_persist_attempt_before_dispatch_and_correlate_lifecycle() {
        let root = test_root();
        let mut registry = AgentRegistry::new(&root).unwrap();
        let id = AgentId::parse("agent-durable-send").unwrap();
        let (command, commands) = mpsc::sync_channel(COMMAND_CAPACITY);
        let worker = thread::spawn(move || {
            while let Ok(command) = commands.recv() {
                if matches!(command, WorkerCommand::Shutdown) {
                    break;
                }
            }
        });
        insert_test_worker(&mut registry, id.clone(), command, worker);
        registry
            .send_with_context_and_client_message_id(
                &id,
                "prompt",
                "Inspect the project and report the next step",
                None,
                Some("message-stable-1"),
            )
            .unwrap();
        let (thread_id, run_id) = {
            let record = registry.records.get(&id).unwrap();
            (record.thread_id.clone(), record.run_id.clone())
        };
        let attempt = registry
            .thread_store
            .list()
            .unwrap()
            .into_iter()
            .find(|thread| thread.thread_id == thread_id)
            .unwrap()
            .send_attempts
            .into_iter()
            .find(|attempt| attempt.client_message_id == "message-stable-1")
            .unwrap();
        assert_eq!(attempt.status, SendAttemptStatus::Dispatching);
        assert_eq!(attempt.target_run_id, run_id);
        let attempt_id = attempt.attempt_id.clone();
        registry
            .send_with_context_and_client_message_id(
                &id,
                "followUp",
                "Also check the existing tests",
                None,
                Some("message-stable-2"),
            )
            .unwrap();
        let second_attempt_id = registry
            .thread_store
            .send_attempts(&thread_id)
            .unwrap()
            .into_iter()
            .find(|attempt| attempt.client_message_id == "message-stable-2")
            .unwrap()
            .attempt_id;
        registry
            .apply_worker_event(WorkerEvent::RequestStarted(
                id.clone(),
                "core-request-1".into(),
                true,
                Some(attempt_id.clone()),
            ))
            .unwrap();
        registry
            .apply_worker_event(WorkerEvent::RequestStarted(
                id.clone(),
                "core-request-2".into(),
                true,
                Some(second_attempt_id.clone()),
            ))
            .unwrap();
        let accepted = registry
            .thread_store
            .list()
            .unwrap()
            .into_iter()
            .find(|thread| thread.thread_id == thread_id)
            .unwrap()
            .send_attempts
            .into_iter()
            .find(|attempt| attempt.attempt_id == attempt_id)
            .unwrap();
        assert_eq!(accepted.status, SendAttemptStatus::Working);
        assert_eq!(
            accepted.engine_request_id.as_deref(),
            Some("core-request-1")
        );
        registry
            .apply_worker_event(WorkerEvent::Engine(
                id.clone(),
                serde_json::json!({"type":"waiting"}),
            ))
            .unwrap();
        let status = registry
            .thread_store
            .send_attempts(&thread_id)
            .unwrap()
            .into_iter()
            .find(|attempt| attempt.attempt_id == attempt_id)
            .unwrap()
            .status;
        assert_eq!(status, SendAttemptStatus::Working);
        registry
            .apply_worker_event(WorkerEvent::Engine(
                id.clone(),
                serde_json::json!({"type":"agent_settled"}),
            ))
            .unwrap();
        let status = registry
            .thread_store
            .send_attempts(&thread_id)
            .unwrap()
            .into_iter()
            .find(|attempt| attempt.attempt_id == attempt_id)
            .unwrap()
            .status;
        assert_eq!(status, SendAttemptStatus::Working);
        registry
            .apply_worker_event(WorkerEvent::Engine(
                id.clone(),
                serde_json::json!({
                    "type":"response",
                    "id":"core-request-1",
                    "ok":true,
                }),
            ))
            .unwrap();
        let status = registry
            .thread_store
            .send_attempts(&thread_id)
            .unwrap()
            .into_iter()
            .find(|attempt| attempt.attempt_id == attempt_id)
            .unwrap()
            .status;
        assert_eq!(status, SendAttemptStatus::Settled);
        assert_eq!(
            registry
                .thread_store
                .send_attempts(&thread_id)
                .unwrap()
                .into_iter()
                .find(|attempt| attempt.attempt_id == second_attempt_id)
                .unwrap()
                .status,
            SendAttemptStatus::Working
        );
        registry
            .apply_worker_event(WorkerEvent::Engine(
                id.clone(),
                serde_json::json!({
                    "type":"response",
                    "id":"core-request-2",
                    "ok":true,
                }),
            ))
            .unwrap();
        assert_eq!(
            registry
                .thread_store
                .send_attempts(&thread_id)
                .unwrap()
                .into_iter()
                .find(|attempt| attempt.attempt_id == second_attempt_id)
                .unwrap()
                .status,
            SendAttemptStatus::Settled
        );
        registry.cancel(&id).unwrap();
        drop(registry);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn worker_persists_core_session_before_ready_notification() {
        let root = test_root();
        let store = ThreadStore::open(&root).unwrap();
        let thread_id = crate::thread_store::random_id("thread").unwrap();
        let run_id = crate::thread_store::random_id("run").unwrap();
        store
            .create(ThreadRecord {
                thread_id: thread_id.clone(),
                title: "test".into(),
                role: "builder".into(),
                created_at_ms: 1,
                composer_draft: None,
                follow_up_queue: None,
                send_attempts: Vec::new(),
                #[cfg(windows)]
                send_attempts_opaque: None,
                runs: vec![RunRecord {
                    run_id: run_id.clone(),
                    engine: "glass-core".into(),
                    core_session_id: None,
                    classification: RunClassification::Starting,
                }],
            })
            .unwrap();
        let (events, received) = mpsc::sync_channel(1);
        let agent_id = AgentId::parse("agent-ready-order").unwrap();
        announce_worker_ready(
            &events,
            agent_id.clone(),
            &store,
            &thread_id,
            &run_id,
            Some("actual-core-session"),
        )
        .unwrap();
        assert!(matches!(received.recv().unwrap(), WorkerEvent::Ready(id) if id == agent_id));
        let record = store.list().unwrap().remove(0);
        assert_eq!(
            record.runs[0].core_session_id.as_deref(),
            Some("actual-core-session")
        );
        assert_eq!(
            record.runs[0].classification,
            RunClassification::RecoveryPending
        );
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn terminal_cancel_classification_survives_registry_reopen() {
        let root = test_root();
        let mut registry = AgentRegistry::new(&root).unwrap();
        let id = AgentId::parse("agent-terminal-reopen").unwrap();
        let (command, commands) = mpsc::sync_channel(COMMAND_CAPACITY);
        let worker = thread::spawn(move || {
            while let Ok(command) = commands.recv() {
                if matches!(command, WorkerCommand::Shutdown) {
                    break;
                }
            }
        });
        insert_test_worker(&mut registry, id.clone(), command, worker);
        let original_thread = registry.records.values().next().unwrap().thread_id.clone();
        registry.cancel(&id).unwrap();
        drop(registry);
        let reopened = AgentRegistry::new(&root).unwrap();
        assert_eq!(
            reopened.records.values().next().unwrap().snapshot.status,
            if cfg!(windows) {
                AgentStatus::Unavailable
            } else {
                AgentStatus::Cancelled
            }
        );
        let record = reopened
            .thread_store
            .list()
            .unwrap()
            .into_iter()
            .find(|t| t.thread_id == original_thread)
            .unwrap();
        assert_eq!(record.runs[0].classification, RunClassification::Cancelled);
        drop(reopened);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn repeated_cancel_joins_every_owned_worker() {
        let root = test_root();
        let mut registry = AgentRegistry::new(&root).unwrap();
        let exited = Arc::new(AtomicU64::new(0));
        for sequence in 0..128 {
            let id = AgentId::parse(format!("agent-test-{sequence}")).unwrap();
            let (command, commands) = mpsc::sync_channel(COMMAND_CAPACITY);
            let exits = Arc::clone(&exited);
            let worker = thread::spawn(move || {
                while let Ok(command) = commands.recv() {
                    if matches!(command, WorkerCommand::Shutdown) {
                        break;
                    }
                }
                exits.fetch_add(1, Ordering::SeqCst);
            });
            insert_test_worker(&mut registry, id.clone(), command, worker);
            registry.cancel(&id).unwrap();
            let record = registry.records.get(&id).unwrap();
            assert_eq!(record.snapshot.status, AgentStatus::Cancelled);
            assert!(record.worker.is_none());
        }
        assert_eq!(exited.load(Ordering::SeqCst), 128);
        drop(registry);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn worker_panic_becomes_attributed_agent_failure() {
        let root = test_root();
        let mut registry = AgentRegistry::new(&root).unwrap();
        let id = AgentId::parse("agent-panic-test").unwrap();
        let (command, _commands) = mpsc::sync_channel(COMMAND_CAPACITY);
        let worker = thread::spawn(|| panic!("intentional worker panic"));
        worker.thread().unpark();
        insert_test_worker(&mut registry, id.clone(), command, worker);
        for _ in 0..100 {
            registry.refresh().unwrap();
            if registry.records[&id].snapshot.status == AgentStatus::Failed {
                break;
            }
            thread::sleep(Duration::from_millis(5));
        }
        let snapshot = registry.snapshot(&id).unwrap();
        assert_eq!(snapshot.status, AgentStatus::Failed);
        assert!(snapshot.last_error.unwrap().contains(id.as_str()));
        assert!(registry.history(0).unwrap().iter().any(|event| {
            event.agent_id == id
                && event.kind == "workerPanicked"
                && event.payload["role"] == "lifecycle-test"
        }));
        drop(registry);
        std::fs::remove_dir_all(root).unwrap();
    }

    fn wait_for_status(
        registry: &mut AgentRegistry,
        id: &AgentId,
        expected: AgentStatus,
    ) -> AgentSnapshot {
        for _ in 0..800 {
            let snapshot = registry.snapshot(id).unwrap();
            if snapshot.status == expected {
                return snapshot;
            }
            if snapshot.status == AgentStatus::Failed {
                panic!("agent failed: {:?}", snapshot.last_error);
            }
            thread::sleep(Duration::from_millis(25));
        }
        panic!("agent did not reach {expected:?}");
    }

    #[test]
    fn independent_resident_sessions_schedule_dependencies_and_stream_state() {
        // The engine is selected by GLASS_AGENT_HARNESS, so gate on the
        // selected harness's readiness instead of on one executable name.
        if !crate::resident_harness::resident_readiness()
            .map(|readiness| readiness.ready)
            .unwrap_or(false)
        {
            return;
        }
        let root = test_root();
        let mut registry = AgentRegistry::new(&root).unwrap();
        let first = registry
            .create(AgentSpec::new("implementer", "inspect state"))
            .unwrap();
        let peer = registry
            .create(AgentSpec::new("researcher", "inspect independent state"))
            .unwrap();
        wait_for_status(&mut registry, &first, AgentStatus::Idle);
        wait_for_status(&mut registry, &peer, AgentStatus::Idle);
        registry.request(&first, PiSessionRequest::State).unwrap();
        registry.request(&peer, PiSessionRequest::State).unwrap();
        for _ in 0..200 {
            let snapshot = registry.snapshot(&first).unwrap();
            if snapshot
                .evidence
                .iter()
                .any(|event| event.get("type").and_then(Value::as_str) == Some("response"))
            {
                break;
            }
            thread::sleep(Duration::from_millis(25));
        }
        assert!(registry.snapshot(&first).unwrap().event_count > 0);
        for _ in 0..200 {
            if registry.snapshot(&peer).unwrap().event_count > 0 {
                break;
            }
            thread::sleep(Duration::from_millis(25));
        }
        assert!(registry.snapshot(&peer).unwrap().event_count > 0);

        let mut dependent = AgentSpec::new("reviewer", "review evidence");
        dependent.dependencies.push(first.clone());
        let second = registry.create(dependent).unwrap();
        assert_eq!(
            registry.snapshot(&second).unwrap().status,
            AgentStatus::Queued
        );
        registry.complete(&first).unwrap();
        wait_for_status(&mut registry, &second, AgentStatus::Idle);
        assert_ne!(first, second);
        registry.cancel(&second).unwrap();
        assert_eq!(
            registry.snapshot(&second).unwrap().status,
            AgentStatus::Cancelled
        );
        registry.restart(&second).unwrap();
        wait_for_status(&mut registry, &second, AgentStatus::Idle);
        registry.cancel(&second).unwrap();
        registry.cancel(&peer).unwrap();
        drop(registry);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn invalid_agent_specs_and_failed_dependencies_fail_closed() {
        let root = test_root();
        let mut registry = AgentRegistry::new(&root).unwrap();
        assert!(registry.create(AgentSpec::new("", "task")).is_err());
        let first = registry.create(AgentSpec::new("lead", "task")).unwrap();
        let mut dependent = AgentSpec::new("review", "task");
        dependent.dependencies.push(first.clone());
        let second = registry.create(dependent).unwrap();
        registry.cancel(&first).unwrap();
        assert_eq!(
            registry.snapshot(&second).unwrap().status,
            AgentStatus::Failed
        );
        drop(registry);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn context_attachments_are_bounded_json_objects() {
        assert!(validate_context_attachment(None).is_ok());
        assert!(validate_context_attachment(Some(&serde_json::json!("text"))).is_err());
        let oversized = serde_json::json!({"text": "x".repeat(64 * 1024)});
        assert!(validate_context_attachment(Some(&oversized)).is_err());
        let bounded = serde_json::json!({
            "schemaVersion": "glass.agent-context.v1",
            "browser": {"browserRevision": 12}
        });
        assert!(validate_context_attachment(Some(&bounded)).is_ok());
    }
}
