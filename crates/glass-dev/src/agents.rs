//! Glass-owned scheduling for independent resident Pi agent sessions.

use glass_browser::development::{
    DevelopmentError, DevelopmentResult, HarnessRequest, PiHarness, PiHarnessOptions,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, SyncSender, TryRecvError, TrySendError};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const COMMAND_CAPACITY: usize = 32;
const EVENT_CAPACITY: usize = 256;
const HISTORY_CAPACITY: usize = 512;
const AGENT_EVIDENCE_CAPACITY: usize = 64;
const MAX_AGENTS: usize = 32;
const MAX_PROMPT_BYTES: usize = 64 * 1024;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct AgentId(String);

impl AgentId {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum AgentStatus {
    Queued,
    Starting,
    Idle,
    Working,
    Waiting,
    Completed,
    Failed,
    Cancelled,
}

impl AgentStatus {
    fn terminal(self) -> bool {
        matches!(self, Self::Completed | Self::Failed | Self::Cancelled)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentSpec {
    pub role: String,
    pub task: String,
    #[serde(default)]
    pub dependencies: Vec<AgentId>,
    pub model: Option<String>,
    pub thinking: Option<String>,
    pub worktree: Option<PathBuf>,
    #[serde(default)]
    pub unrestricted: bool,
    pub max_runtime_seconds: Option<u64>,
    pub max_events: Option<u64>,
}

impl AgentSpec {
    pub fn new(role: impl Into<String>, task: impl Into<String>) -> Self {
        Self {
            role: role.into(),
            task: task.into(),
            dependencies: Vec::new(),
            model: None,
            thinking: None,
            worktree: None,
            unrestricted: false,
            max_runtime_seconds: Some(3_600),
            max_events: Some(10_000),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentSnapshot {
    pub id: AgentId,
    pub role: String,
    pub task: String,
    pub status: AgentStatus,
    pub dependencies: Vec<AgentId>,
    pub model: Option<String>,
    pub thinking: Option<String>,
    pub worktree: PathBuf,
    pub unrestricted: bool,
    pub created_at_ms: u128,
    pub started_at_ms: Option<u128>,
    pub updated_at_ms: u128,
    pub event_count: u64,
    pub last_error: Option<String>,
    pub last_response_id: Option<String>,
    pub evidence: Vec<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentEvent {
    pub sequence: u64,
    pub agent_id: AgentId,
    pub timestamp_ms: u128,
    pub kind: String,
    pub payload: Value,
}

#[derive(Debug)]
enum WorkerCommand {
    Request(HarnessRequest),
    Shutdown,
}

#[derive(Debug)]
enum WorkerEvent {
    Ready(AgentId),
    RequestStarted(AgentId, String, bool),
    Pi(AgentId, Value),
    Failed(AgentId, String),
    Stopped(AgentId),
}

struct AgentRecord {
    snapshot: AgentSnapshot,
    spec: AgentSpec,
    command: Option<SyncSender<WorkerCommand>>,
    awaiting_agent_settle: bool,
}

/// Registry and scheduler for independent Pi subprocesses.
///
/// Each running agent owns a distinct Pi process and persistent session. The
/// registry only starts dependency-ready work, bounds all queues and retained
/// evidence, and never silently promotes a failed dependency to success.
pub struct AgentRegistry {
    root: PathBuf,
    sessions_dir: PathBuf,
    records: BTreeMap<AgentId, AgentRecord>,
    events_tx: SyncSender<WorkerEvent>,
    events_rx: Receiver<WorkerEvent>,
    history: VecDeque<AgentEvent>,
    next_agent: u64,
    next_event: u64,
}

impl AgentRegistry {
    pub fn new(root: impl AsRef<Path>) -> DevelopmentResult<Self> {
        let root = std::fs::canonicalize(root)?;
        let sessions_dir = root.join(".glass").join("pi-sessions");
        std::fs::create_dir_all(&sessions_dir)?;
        let (events_tx, events_rx) = mpsc::sync_channel(EVENT_CAPACITY);
        Ok(Self {
            root,
            sessions_dir,
            records: BTreeMap::new(),
            events_tx,
            events_rx,
            history: VecDeque::new(),
            next_agent: 1,
            next_event: 1,
        })
    }

    pub fn create(&mut self, spec: AgentSpec) -> DevelopmentResult<AgentId> {
        self.refresh()?;
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
                    unrestricted: spec.unrestricted,
                    created_at_ms: now,
                    started_at_ms: None,
                    updated_at_ms: now,
                    event_count: 0,
                    last_error: None,
                    last_response_id: None,
                    evidence: Vec::new(),
                },
                spec,
                command: None,
                awaiting_agent_settle: false,
            },
        );
        if status == AgentStatus::Starting {
            self.spawn(&id)?;
        }
        Ok(id)
    }

    pub fn refresh(&mut self) -> DevelopmentResult<()> {
        loop {
            match self.events_rx.try_recv() {
                Ok(event) => self.apply_worker_event(event),
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    return Err(DevelopmentError::Process(
                        "agent scheduler event channel closed".into(),
                    ));
                }
            }
        }
        self.enforce_budgets();
        self.fail_blocked_dependents();
        self.start_ready_agents()?;
        Ok(())
    }

    pub fn snapshot(&mut self, id: &AgentId) -> DevelopmentResult<AgentSnapshot> {
        self.refresh()?;
        self.records
            .get(id)
            .map(|record| record.snapshot.clone())
            .ok_or_else(|| DevelopmentError::NotFound(format!("agent {}", id.as_str())))
    }

    pub fn list(&mut self) -> DevelopmentResult<Vec<AgentSnapshot>> {
        self.refresh()?;
        Ok(self
            .records
            .values()
            .map(|record| record.snapshot.clone())
            .collect())
    }

    pub fn history(&mut self, since: u64) -> DevelopmentResult<Vec<AgentEvent>> {
        self.refresh()?;
        Ok(self
            .history
            .iter()
            .filter(|event| event.sequence > since)
            .cloned()
            .collect())
    }

    pub fn prompt(&mut self, id: &AgentId, text: impl Into<String>) -> DevelopmentResult<()> {
        let text = text.into();
        validate_text("agent prompt", &text, MAX_PROMPT_BYTES)?;
        self.send(id, HarnessRequest::Prompt { text })
    }

    pub fn steer(&mut self, id: &AgentId, text: impl Into<String>) -> DevelopmentResult<()> {
        let text = text.into();
        validate_text("agent steering", &text, MAX_PROMPT_BYTES)?;
        self.send(id, HarnessRequest::Steer { text })
    }

    pub fn follow_up(&mut self, id: &AgentId, text: impl Into<String>) -> DevelopmentResult<()> {
        let text = text.into();
        validate_text("agent follow-up", &text, MAX_PROMPT_BYTES)?;
        self.send(id, HarnessRequest::FollowUp { text })
    }

    pub fn request(&mut self, id: &AgentId, request: HarnessRequest) -> DevelopmentResult<()> {
        self.send(id, request)
    }

    pub fn complete(&mut self, id: &AgentId) -> DevelopmentResult<()> {
        self.refresh()?;
        let record = self.record_mut(id)?;
        if record.snapshot.status.terminal() {
            return Err(DevelopmentError::Conflict(format!(
                "agent {} is already terminal",
                id.as_str()
            )));
        }
        record.snapshot.status = AgentStatus::Completed;
        record.snapshot.updated_at_ms = now_ms();
        if let Some(sender) = record.command.take() {
            let _ = sender.try_send(WorkerCommand::Shutdown);
        }
        self.record_event(id, "completed", Value::Null);
        self.start_ready_agents()?;
        Ok(())
    }

    pub fn cancel(&mut self, id: &AgentId) -> DevelopmentResult<()> {
        self.refresh()?;
        let record = self.record_mut(id)?;
        if record.snapshot.status.terminal() {
            return Ok(());
        }
        if let Some(sender) = &record.command {
            let _ = sender.try_send(WorkerCommand::Request(HarnessRequest::Abort));
            let _ = sender.try_send(WorkerCommand::Shutdown);
        }
        record.command = None;
        record.snapshot.status = AgentStatus::Cancelled;
        record.snapshot.updated_at_ms = now_ms();
        self.record_event(id, "cancelled", Value::Null);
        self.fail_blocked_dependents();
        Ok(())
    }

    fn send(&mut self, id: &AgentId, request: HarnessRequest) -> DevelopmentResult<()> {
        self.refresh()?;
        let record = self.record_mut(id)?;
        if record.snapshot.status.terminal() || record.snapshot.status == AgentStatus::Queued {
            return Err(DevelopmentError::Conflict(format!(
                "agent {} cannot accept requests while {:?}",
                id.as_str(),
                record.snapshot.status
            )));
        }
        let sender = record.command.as_ref().ok_or_else(|| {
            DevelopmentError::Conflict(format!("agent {} is still starting", id.as_str()))
        })?;
        sender
            .try_send(WorkerCommand::Request(request))
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
        Ok(())
    }

    fn spawn(&mut self, id: &AgentId) -> DevelopmentResult<()> {
        let sessions_dir = self.sessions_dir.clone();
        let events = self.events_tx.clone();
        let record = self.record_mut(id)?;
        let spec = record.spec.clone();
        let worktree = record.snapshot.worktree.clone();
        let worker_id = id.clone();
        let (commands_tx, commands_rx) = mpsc::sync_channel(COMMAND_CAPACITY);
        record.command = Some(commands_tx);
        record.snapshot.status = AgentStatus::Starting;
        record.snapshot.started_at_ms = Some(now_ms());
        record.snapshot.updated_at_ms = now_ms();
        thread::Builder::new()
            .name(format!("glass-{}", id.as_str()))
            .spawn(move || run_worker(worker_id, spec, worktree, sessions_dir, commands_rx, events))
            .map_err(DevelopmentError::Io)?;
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

    fn fail_blocked_dependents(&mut self) {
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
    }

    fn enforce_budgets(&mut self) {
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
            if let Some(record) = self.records.get_mut(&id) {
                if let Some(sender) = record.command.take() {
                    let _ = sender.try_send(WorkerCommand::Request(HarnessRequest::Abort));
                    let _ = sender.try_send(WorkerCommand::Shutdown);
                }
                record.snapshot.status = AgentStatus::Failed;
                record.snapshot.last_error = Some("agent budget exceeded".into());
                record.snapshot.updated_at_ms = now;
            }
            self.record_event(&id, "budgetExceeded", Value::Null);
        }
    }

    fn apply_worker_event(&mut self, event: WorkerEvent) {
        match event {
            WorkerEvent::Ready(id) => {
                if let Some(record) = self.records.get_mut(&id)
                    && !record.snapshot.status.terminal()
                {
                    record.snapshot.status = AgentStatus::Idle;
                    record.snapshot.updated_at_ms = now_ms();
                }
                self.record_event(&id, "ready", Value::Null);
            }
            WorkerEvent::RequestStarted(id, request_id, waits_for_agent) => {
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
            WorkerEvent::Pi(id, value) => {
                if let Some(record) = self.records.get_mut(&id) {
                    let event_type = value.get("type").and_then(Value::as_str);
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
                    .unwrap_or("piEvent")
                    .to_string();
                self.record_event(&id, &kind, value);
            }
            WorkerEvent::Failed(id, error) => {
                if let Some(record) = self.records.get_mut(&id)
                    && !record.snapshot.status.terminal()
                {
                    record.snapshot.status = AgentStatus::Failed;
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
        for record in self.records.values_mut() {
            if let Some(sender) = record.command.take() {
                let _ = sender.try_send(WorkerCommand::Shutdown);
            }
        }
    }
}

fn run_worker(
    id: AgentId,
    spec: AgentSpec,
    worktree: PathBuf,
    sessions_dir: PathBuf,
    commands: Receiver<WorkerCommand>,
    events: SyncSender<WorkerEvent>,
) {
    let options = PiHarnessOptions {
        unrestricted: spec.unrestricted,
        persist_session: true,
        session_dir: Some(sessions_dir),
        name: Some(format!("{}: {}", spec.role, id.as_str())),
        model: spec.model,
        thinking: spec.thinking,
        ..PiHarnessOptions::default()
    };
    let mut harness = match PiHarness::spawn_with_options(&worktree, options) {
        Ok(harness) => harness,
        Err(error) => {
            send_critical_worker_event(&events, WorkerEvent::Failed(id, error.to_string()));
            return;
        }
    };
    send_critical_worker_event(&events, WorkerEvent::Ready(id.clone()));
    loop {
        loop {
            match commands.try_recv() {
                Ok(WorkerCommand::Request(request)) => {
                    let waits_for_agent = matches!(
                        request,
                        HarnessRequest::Prompt { .. } | HarnessRequest::FollowUp { .. }
                    );
                    match harness.start_request(request) {
                        Ok(request_id) => send_critical_worker_event(
                            &events,
                            WorkerEvent::RequestStarted(id.clone(), request_id, waits_for_agent),
                        ),
                        Err(error) => {
                            send_critical_worker_event(
                                &events,
                                WorkerEvent::Failed(id.clone(), error.to_string()),
                            );
                            return;
                        }
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
            Ok(Some(value)) => send_lossy_worker_event(&events, WorkerEvent::Pi(id.clone(), value)),
            Ok(None) => {}
            Err(error) => {
                send_critical_worker_event(&events, WorkerEvent::Failed(id, error.to_string()));
                return;
            }
        }
    }
}

fn send_lossy_worker_event(events: &SyncSender<WorkerEvent>, event: WorkerEvent) {
    let _ = events.try_send(event);
}

fn send_critical_worker_event(events: &SyncSender<WorkerEvent>, event: WorkerEvent) {
    let _ = events.send(event);
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

    fn wait_for_status(
        registry: &mut AgentRegistry,
        id: &AgentId,
        expected: AgentStatus,
    ) -> AgentSnapshot {
        for _ in 0..200 {
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
    fn independent_pi_sessions_schedule_dependencies_and_stream_state() {
        if std::process::Command::new("pi")
            .arg("--version")
            .output()
            .is_err()
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
        registry.request(&first, HarnessRequest::State).unwrap();
        registry.request(&peer, HarnessRequest::State).unwrap();
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
        registry.cancel(&peer).unwrap();
        assert_eq!(
            registry.snapshot(&second).unwrap().status,
            AgentStatus::Cancelled
        );
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
}
