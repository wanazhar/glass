//! Autonomous, verified task DAG scheduling above resident Pi agents.

use crate::agents::{AgentEvent, AgentId, AgentRegistry, AgentSnapshot, AgentSpec, AgentStatus};
use glass_browser::development::{DevelopmentError, DevelopmentResult};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const MAX_TASKS: usize = 128;
const MAX_DEPENDENCIES: usize = 32;
const MAX_TASK_EVIDENCE: usize = 128;
const MAX_TASK_TEXT_BYTES: usize = 128 * 1024;
const MAX_VERIFIER_COMMAND_BYTES: usize = 16 * 1024;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct TaskId(String);

impl TaskId {
    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn parse(value: impl Into<String>) -> DevelopmentResult<Self> {
        let value = value.into();
        if !value.starts_with("task-")
            || value.len() > 64
            || value.chars().any(|character| character.is_control())
        {
            return Err(DevelopmentError::InvalidInput("invalid task id".into()));
        }
        Ok(Self(value))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TaskState {
    Queued,
    Ready,
    Running,
    Waiting,
    Verifying,
    Succeeded,
    Failed,
    Cancelled,
    Paused,
    Blocked,
}

impl TaskState {
    fn terminal(self) -> bool {
        matches!(self, Self::Succeeded | Self::Failed | Self::Cancelled)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct TaskBudget {
    pub max_runtime_seconds: u64,
    pub max_events: u64,
    pub max_tokens: Option<u64>,
}

impl Default for TaskBudget {
    fn default() -> Self {
        Self {
            max_runtime_seconds: 3_600,
            max_events: 10_000,
            max_tokens: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RetryPolicy {
    pub max_retries: u32,
    pub backoff_seconds: u64,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            max_retries: 0,
            backoff_seconds: 1,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum VerificationRequirement {
    #[default]
    Settled,
    Command {
        command: String,
        expected_exit: i32,
        timeout_seconds: u64,
    },
    LspDiagnostics {
        max_errors: u64,
    },
    BrowserWorkflow {
        assertion: String,
    },
    SemanticRegression {
        baseline: String,
        maximum_regressions: u64,
    },
    DebuggerAssertion {
        session: String,
        expression: String,
    },
    GitChange {
        require_changes: bool,
        require_clean: bool,
    },
    TrustedCustom {
        name: String,
    },
    All {
        requirements: Vec<VerificationRequirement>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TaskEvidence {
    pub kind: String,
    pub actor: String,
    pub source: String,
    pub passed: Option<bool>,
    pub observed_at_ms: u128,
    pub details: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskSpec {
    pub title: String,
    pub goal: String,
    pub prompt: String,
    #[serde(default = "default_role")]
    pub role: String,
    #[serde(default)]
    pub dependencies: Vec<TaskId>,
    pub worktree: Option<PathBuf>,
    pub model: Option<String>,
    pub thinking: Option<String>,
    #[serde(default)]
    pub unrestricted: bool,
    #[serde(default)]
    pub budget: TaskBudget,
    #[serde(default)]
    pub verification: VerificationRequirement,
    #[serde(default)]
    pub retry: RetryPolicy,
}

fn default_role() -> String {
    "developer".into()
}

impl TaskSpec {
    pub fn new(title: impl Into<String>, prompt: impl Into<String>) -> Self {
        let title = title.into();
        Self {
            goal: title.clone(),
            title,
            prompt: prompt.into(),
            role: default_role(),
            dependencies: Vec::new(),
            worktree: None,
            model: None,
            thinking: None,
            unrestricted: false,
            budget: TaskBudget::default(),
            verification: VerificationRequirement::default(),
            retry: RetryPolicy::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskSnapshot {
    pub id: TaskId,
    pub title: String,
    pub goal: String,
    pub prompt: String,
    pub role: String,
    pub dependencies: Vec<TaskId>,
    pub assigned_agent: Option<AgentId>,
    pub worktree: PathBuf,
    pub model: Option<String>,
    pub thinking: Option<String>,
    pub unrestricted: bool,
    pub budget: TaskBudget,
    pub verification: VerificationRequirement,
    pub retry: RetryPolicy,
    pub state: TaskState,
    pub attempt: u32,
    pub observed_tokens: u64,
    pub created_at_ms: u128,
    pub started_at_ms: Option<u128>,
    pub completed_at_ms: Option<u128>,
    pub updated_at_ms: u128,
    pub last_error: Option<String>,
    pub evidence: Vec<TaskEvidence>,
    pub blocked_override: bool,
}

struct TaskRecord {
    snapshot: TaskSnapshot,
    next_retry_at_ms: Option<u128>,
}

pub trait TaskAgentBackend {
    fn refresh_agents(&mut self) -> DevelopmentResult<()>;
    fn create_agent(&mut self, spec: AgentSpec) -> DevelopmentResult<AgentId>;
    fn agent_snapshots(&mut self) -> DevelopmentResult<Vec<AgentSnapshot>>;
    fn agent_events(&mut self, since: u64) -> DevelopmentResult<Vec<AgentEvent>>;
    fn prompt_agent(&mut self, id: &AgentId, prompt: String) -> DevelopmentResult<()>;
    fn cancel_agent(&mut self, id: &AgentId) -> DevelopmentResult<()>;
    fn complete_agent(&mut self, id: &AgentId) -> DevelopmentResult<()>;
}

impl TaskAgentBackend for AgentRegistry {
    fn refresh_agents(&mut self) -> DevelopmentResult<()> {
        self.refresh()
    }

    fn create_agent(&mut self, spec: AgentSpec) -> DevelopmentResult<AgentId> {
        self.create(spec)
    }

    fn agent_snapshots(&mut self) -> DevelopmentResult<Vec<AgentSnapshot>> {
        self.list()
    }

    fn agent_events(&mut self, since: u64) -> DevelopmentResult<Vec<AgentEvent>> {
        self.history(since)
    }

    fn prompt_agent(&mut self, id: &AgentId, prompt: String) -> DevelopmentResult<()> {
        self.prompt(id, prompt)
    }

    fn cancel_agent(&mut self, id: &AgentId) -> DevelopmentResult<()> {
        self.cancel(id)
    }

    fn complete_agent(&mut self, id: &AgentId) -> DevelopmentResult<()> {
        self.complete(id)
    }
}

pub struct TaskScheduler {
    root: PathBuf,
    tasks: BTreeMap<TaskId, TaskRecord>,
    next_task: u64,
    agent_event_cursor: u64,
}

impl TaskScheduler {
    pub fn new(root: impl AsRef<Path>) -> DevelopmentResult<Self> {
        Ok(Self {
            root: std::fs::canonicalize(root)?,
            tasks: BTreeMap::new(),
            next_task: 1,
            agent_event_cursor: 0,
        })
    }

    pub fn create<B: TaskAgentBackend>(
        &mut self,
        agents: &mut B,
        spec: TaskSpec,
    ) -> DevelopmentResult<TaskId> {
        self.refresh(agents)?;
        self.validate_spec(&spec)?;
        if self.tasks.len() >= MAX_TASKS {
            return Err(DevelopmentError::Conflict(format!(
                "task scheduler reached its {MAX_TASKS} task limit"
            )));
        }
        let id = TaskId(format!("task-{:04}", self.next_task));
        self.next_task = self
            .next_task
            .checked_add(1)
            .ok_or_else(|| DevelopmentError::Conflict("task identifier space exhausted".into()))?;
        let now = now_ms();
        self.tasks.insert(
            id.clone(),
            TaskRecord {
                snapshot: TaskSnapshot {
                    id: id.clone(),
                    title: spec.title,
                    goal: spec.goal,
                    prompt: spec.prompt,
                    role: spec.role,
                    dependencies: spec.dependencies,
                    assigned_agent: None,
                    worktree: spec.worktree.unwrap_or_else(|| self.root.clone()),
                    model: spec.model,
                    thinking: spec.thinking,
                    unrestricted: spec.unrestricted,
                    budget: spec.budget,
                    verification: spec.verification,
                    retry: spec.retry,
                    state: TaskState::Queued,
                    attempt: 0,
                    observed_tokens: 0,
                    created_at_ms: now,
                    started_at_ms: None,
                    completed_at_ms: None,
                    updated_at_ms: now,
                    last_error: None,
                    evidence: Vec::new(),
                    blocked_override: false,
                },
                next_retry_at_ms: None,
            },
        );
        self.schedule_ready(agents)?;
        Ok(id)
    }

    pub fn refresh<B: TaskAgentBackend>(&mut self, agents: &mut B) -> DevelopmentResult<()> {
        agents.refresh_agents()?;
        let snapshots = agents.agent_snapshots()?;
        let events = agents.agent_events(self.agent_event_cursor)?;
        for event in events {
            self.agent_event_cursor = self.agent_event_cursor.max(event.sequence);
            self.apply_agent_event(agents, &event)?;
        }
        self.apply_agent_failures(&snapshots);
        self.enforce_task_budgets(agents)?;
        self.verify_pending(agents)?;
        self.propagate_blocked();
        self.schedule_ready(agents)
    }

    pub fn list<B: TaskAgentBackend>(
        &mut self,
        agents: &mut B,
    ) -> DevelopmentResult<Vec<TaskSnapshot>> {
        self.refresh(agents)?;
        Ok(self
            .tasks
            .values()
            .map(|record| record.snapshot.clone())
            .collect())
    }

    pub fn snapshot<B: TaskAgentBackend>(
        &mut self,
        agents: &mut B,
        id: &TaskId,
    ) -> DevelopmentResult<TaskSnapshot> {
        self.refresh(agents)?;
        self.tasks
            .get(id)
            .map(|record| record.snapshot.clone())
            .ok_or_else(|| DevelopmentError::NotFound(format!("task {}", id.as_str())))
    }

    pub fn submit_evidence(
        &mut self,
        id: &TaskId,
        kind: impl Into<String>,
        actor: impl Into<String>,
        source: impl Into<String>,
        passed: bool,
        details: Value,
    ) -> DevelopmentResult<()> {
        let record = self.record_mut(id)?;
        push_evidence(
            record,
            TaskEvidence {
                kind: kind.into(),
                actor: actor.into(),
                source: source.into(),
                passed: Some(passed),
                observed_at_ms: now_ms(),
                details,
            },
        );
        if record.snapshot.state == TaskState::Waiting {
            record.snapshot.state = TaskState::Verifying;
        }
        Ok(())
    }

    pub fn pause<B: TaskAgentBackend>(
        &mut self,
        agents: &mut B,
        id: &TaskId,
    ) -> DevelopmentResult<()> {
        self.refresh(agents)?;
        let agent = self.record(id)?.snapshot.assigned_agent.clone();
        if let Some(agent) = agent {
            agents.cancel_agent(&agent)?;
        }
        let record = self.record_mut(id)?;
        if record.snapshot.state.terminal() {
            return Err(DevelopmentError::Conflict(
                "terminal task cannot be paused".into(),
            ));
        }
        record.snapshot.assigned_agent = None;
        record.snapshot.state = TaskState::Paused;
        record.snapshot.updated_at_ms = now_ms();
        Ok(())
    }

    pub fn resume<B: TaskAgentBackend>(
        &mut self,
        agents: &mut B,
        id: &TaskId,
    ) -> DevelopmentResult<()> {
        let record = self.record_mut(id)?;
        if record.snapshot.state != TaskState::Paused {
            return Err(DevelopmentError::Conflict("task is not paused".into()));
        }
        record.snapshot.state = TaskState::Queued;
        record.snapshot.updated_at_ms = now_ms();
        self.schedule_ready(agents)
    }

    pub fn cancel<B: TaskAgentBackend>(
        &mut self,
        agents: &mut B,
        id: &TaskId,
    ) -> DevelopmentResult<()> {
        self.refresh(agents)?;
        let agent = self.record(id)?.snapshot.assigned_agent.clone();
        if let Some(agent) = agent {
            agents.cancel_agent(&agent)?;
        }
        let record = self.record_mut(id)?;
        if !record.snapshot.state.terminal() {
            record.snapshot.state = TaskState::Cancelled;
            record.snapshot.completed_at_ms = Some(now_ms());
            record.snapshot.updated_at_ms = now_ms();
        }
        self.propagate_blocked();
        Ok(())
    }

    pub fn retry<B: TaskAgentBackend>(
        &mut self,
        agents: &mut B,
        id: &TaskId,
    ) -> DevelopmentResult<()> {
        let record = self.record_mut(id)?;
        if !matches!(
            record.snapshot.state,
            TaskState::Failed | TaskState::Blocked
        ) {
            return Err(DevelopmentError::Conflict(
                "only failed or blocked tasks can be retried".into(),
            ));
        }
        record.snapshot.state = TaskState::Queued;
        record.snapshot.assigned_agent = None;
        record.snapshot.completed_at_ms = None;
        record.snapshot.last_error = None;
        record.snapshot.updated_at_ms = now_ms();
        record.next_retry_at_ms = None;
        self.schedule_ready(agents)
    }

    pub fn reassign<B: TaskAgentBackend>(
        &mut self,
        agents: &mut B,
        id: &TaskId,
        role: String,
        model: Option<String>,
        thinking: Option<String>,
    ) -> DevelopmentResult<()> {
        validate_text("task role", &role)?;
        let agent = self.record(id)?.snapshot.assigned_agent.clone();
        if let Some(agent) = agent {
            agents.cancel_agent(&agent)?;
        }
        let record = self.record_mut(id)?;
        if record.snapshot.state.terminal() {
            return Err(DevelopmentError::Conflict(
                "terminal task cannot be reassigned".into(),
            ));
        }
        record.snapshot.role = role;
        record.snapshot.model = model;
        record.snapshot.thinking = thinking;
        record.snapshot.assigned_agent = None;
        record.snapshot.state = TaskState::Queued;
        record.snapshot.updated_at_ms = now_ms();
        self.schedule_ready(agents)
    }

    pub fn override_blocked<B: TaskAgentBackend>(
        &mut self,
        agents: &mut B,
        id: &TaskId,
    ) -> DevelopmentResult<()> {
        let record = self.record_mut(id)?;
        if record.snapshot.state != TaskState::Blocked {
            return Err(DevelopmentError::Conflict("task is not blocked".into()));
        }
        record.snapshot.blocked_override = true;
        record.snapshot.state = TaskState::Queued;
        record.snapshot.last_error = None;
        record.snapshot.updated_at_ms = now_ms();
        self.schedule_ready(agents)
    }

    fn schedule_ready<B: TaskAgentBackend>(&mut self, agents: &mut B) -> DevelopmentResult<()> {
        let now = now_ms();
        let ready = self
            .tasks
            .iter()
            .filter(|(_, record)| record.snapshot.state == TaskState::Queued)
            .filter(|(_, record)| record.next_retry_at_ms.is_none_or(|at| at <= now))
            .filter(|(_, record)| {
                record.snapshot.blocked_override
                    || record.snapshot.dependencies.iter().all(|dependency| {
                        self.tasks.get(dependency).is_some_and(|dependency| {
                            dependency.snapshot.state == TaskState::Succeeded
                        })
                    })
            })
            .map(|(id, _)| id.clone())
            .collect::<Vec<_>>();
        for id in ready {
            let record = self.record(&id)?;
            let mut spec = AgentSpec::new(
                record.snapshot.role.clone(),
                format!("task {}: {}", id.as_str(), record.snapshot.title),
            );
            spec.model.clone_from(&record.snapshot.model);
            spec.thinking.clone_from(&record.snapshot.thinking);
            spec.worktree = Some(record.snapshot.worktree.clone());
            spec.unrestricted = record.snapshot.unrestricted;
            spec.max_runtime_seconds = Some(record.snapshot.budget.max_runtime_seconds);
            spec.max_events = Some(record.snapshot.budget.max_events);
            let agent = agents.create_agent(spec)?;
            let record = self.record_mut(&id)?;
            record.snapshot.assigned_agent = Some(agent);
            record.snapshot.state = TaskState::Ready;
            record.snapshot.attempt = record.snapshot.attempt.saturating_add(1);
            record.snapshot.updated_at_ms = now_ms();
            record.next_retry_at_ms = None;
        }
        Ok(())
    }

    fn apply_agent_event<B: TaskAgentBackend>(
        &mut self,
        agents: &mut B,
        event: &AgentEvent,
    ) -> DevelopmentResult<()> {
        let Some(task_id) = self.task_for_agent(&event.agent_id) else {
            return Ok(());
        };
        let state = self.record(&task_id)?.snapshot.state;
        match event.kind.as_str() {
            "ready" if state == TaskState::Ready => {
                let prompt = self.record(&task_id)?.snapshot.prompt.clone();
                agents.prompt_agent(&event.agent_id, prompt)?;
                let record = self.record_mut(&task_id)?;
                record.snapshot.state = TaskState::Running;
                record.snapshot.started_at_ms.get_or_insert_with(now_ms);
                record.snapshot.updated_at_ms = now_ms();
            }
            "agent_settled" if state == TaskState::Running => {
                let record = self.record_mut(&task_id)?;
                record.snapshot.state = TaskState::Verifying;
                record.snapshot.updated_at_ms = now_ms();
                push_evidence(
                    record,
                    TaskEvidence {
                        kind: "agent.settled".into(),
                        actor: event.agent_id.as_str().into(),
                        source: "pi-agent-session".into(),
                        passed: Some(true),
                        observed_at_ms: event.timestamp_ms,
                        details: event.payload.clone(),
                    },
                );
            }
            kind => {
                let record = self.record_mut(&task_id)?;
                if kind == "message_end" {
                    record.snapshot.observed_tokens = record
                        .snapshot
                        .observed_tokens
                        .saturating_add(event_tokens(&event.payload));
                }
                push_evidence(
                    record,
                    TaskEvidence {
                        kind: format!("agent.{kind}"),
                        actor: event.agent_id.as_str().into(),
                        source: "pi-agent-session".into(),
                        passed: None,
                        observed_at_ms: event.timestamp_ms,
                        details: event.payload.clone(),
                    },
                );
            }
        }
        Ok(())
    }

    fn apply_agent_failures(&mut self, snapshots: &[AgentSnapshot]) {
        for snapshot in snapshots {
            if !matches!(
                snapshot.status,
                AgentStatus::Failed | AgentStatus::Cancelled
            ) {
                continue;
            }
            let Some(id) = self.task_for_agent(&snapshot.id) else {
                continue;
            };
            let record = self.tasks.get_mut(&id).expect("task exists");
            if !record.snapshot.state.terminal() {
                record.snapshot.state = TaskState::Failed;
                record.snapshot.last_error = snapshot
                    .last_error
                    .clone()
                    .or_else(|| Some("assigned agent stopped before verification".into()));
                record.snapshot.completed_at_ms = Some(now_ms());
                record.snapshot.updated_at_ms = now_ms();
            }
        }
    }

    fn verify_pending<B: TaskAgentBackend>(&mut self, agents: &mut B) -> DevelopmentResult<()> {
        let pending = self
            .tasks
            .iter()
            .filter(|(_, record)| record.snapshot.state == TaskState::Verifying)
            .map(|(id, _)| id.clone())
            .collect::<Vec<_>>();
        for id in pending {
            let (requirement, worktree, evidence) = {
                let record = self.record(&id)?;
                (
                    record.snapshot.verification.clone(),
                    record.snapshot.worktree.clone(),
                    record.snapshot.evidence.clone(),
                )
            };
            match verify(&requirement, &worktree, &evidence)? {
                VerificationOutcome::Passed(new_evidence) => {
                    let agent = self.record(&id)?.snapshot.assigned_agent.clone();
                    if let Some(agent) = agent {
                        agents.complete_agent(&agent)?;
                    }
                    let record = self.record_mut(&id)?;
                    for evidence in new_evidence {
                        push_evidence(record, evidence);
                    }
                    record.snapshot.state = TaskState::Succeeded;
                    record.snapshot.completed_at_ms = Some(now_ms());
                    record.snapshot.updated_at_ms = now_ms();
                    record.snapshot.last_error = None;
                }
                VerificationOutcome::Waiting => {
                    let record = self.record_mut(&id)?;
                    record.snapshot.state = TaskState::Waiting;
                    record.snapshot.updated_at_ms = now_ms();
                }
                VerificationOutcome::Failed(error, new_evidence) => {
                    let (agent, retry, attempt) = {
                        let record = self.record(&id)?;
                        (
                            record.snapshot.assigned_agent.clone(),
                            record.snapshot.retry.clone(),
                            record.snapshot.attempt,
                        )
                    };
                    if let Some(agent) = agent {
                        agents.cancel_agent(&agent)?;
                    }
                    let record = self.record_mut(&id)?;
                    for evidence in new_evidence {
                        push_evidence(record, evidence);
                    }
                    record.snapshot.assigned_agent = None;
                    record.snapshot.last_error = Some(error);
                    record.snapshot.updated_at_ms = now_ms();
                    if attempt <= retry.max_retries {
                        record.snapshot.state = TaskState::Queued;
                        record.next_retry_at_ms = Some(
                            now_ms().saturating_add(u128::from(retry.backoff_seconds) * 1_000),
                        );
                    } else {
                        record.snapshot.state = TaskState::Failed;
                        record.snapshot.completed_at_ms = Some(now_ms());
                    }
                }
            }
        }
        Ok(())
    }

    fn enforce_task_budgets<B: TaskAgentBackend>(
        &mut self,
        agents: &mut B,
    ) -> DevelopmentResult<()> {
        let now = now_ms();
        let exceeded = self
            .tasks
            .iter()
            .filter(|(_, record)| {
                matches!(
                    record.snapshot.state,
                    TaskState::Running | TaskState::Waiting
                )
            })
            .filter(|(_, record)| {
                record.snapshot.started_at_ms.is_some_and(|started| {
                    now.saturating_sub(started)
                        > u128::from(record.snapshot.budget.max_runtime_seconds) * 1_000
                }) || record
                    .snapshot
                    .budget
                    .max_tokens
                    .is_some_and(|limit| record.snapshot.observed_tokens >= limit)
            })
            .map(|(id, _)| id.clone())
            .collect::<Vec<_>>();
        for id in exceeded {
            if let Some(agent) = self.record(&id)?.snapshot.assigned_agent.clone() {
                agents.cancel_agent(&agent)?;
            }
            let record = self.record_mut(&id)?;
            record.snapshot.state = TaskState::Failed;
            record.snapshot.last_error = Some("task runtime or token budget exceeded".into());
            record.snapshot.completed_at_ms = Some(now);
            record.snapshot.updated_at_ms = now;
        }
        Ok(())
    }

    fn propagate_blocked(&mut self) {
        loop {
            let failed = self
                .tasks
                .iter()
                .filter(|(_, record)| {
                    matches!(
                        record.snapshot.state,
                        TaskState::Failed | TaskState::Cancelled | TaskState::Blocked
                    )
                })
                .map(|(id, _)| id.clone())
                .collect::<BTreeSet<_>>();
            let blocked = self
                .tasks
                .iter()
                .filter(|(_, record)| {
                    record.snapshot.state == TaskState::Queued && !record.snapshot.blocked_override
                })
                .filter(|(_, record)| {
                    record
                        .snapshot
                        .dependencies
                        .iter()
                        .any(|dependency| failed.contains(dependency))
                })
                .map(|(id, _)| id.clone())
                .collect::<Vec<_>>();
            if blocked.is_empty() {
                break;
            }
            for id in blocked {
                let record = self.tasks.get_mut(&id).expect("task exists");
                record.snapshot.state = TaskState::Blocked;
                record.snapshot.last_error = Some("a prerequisite task did not succeed".into());
                record.snapshot.updated_at_ms = now_ms();
            }
        }
    }

    fn validate_spec(&self, spec: &TaskSpec) -> DevelopmentResult<()> {
        for (label, value) in [
            ("task title", spec.title.as_str()),
            ("task goal", spec.goal.as_str()),
            ("task prompt", spec.prompt.as_str()),
            ("task role", spec.role.as_str()),
        ] {
            validate_text(label, value)?;
        }
        if spec.dependencies.len() > MAX_DEPENDENCIES {
            return Err(DevelopmentError::InvalidInput(
                "task has too many dependencies".into(),
            ));
        }
        let mut unique = BTreeSet::new();
        for dependency in &spec.dependencies {
            if !unique.insert(dependency) {
                return Err(DevelopmentError::InvalidInput(format!(
                    "duplicate task dependency {}",
                    dependency.as_str()
                )));
            }
            if !self.tasks.contains_key(dependency) {
                return Err(DevelopmentError::NotFound(format!(
                    "task dependency {}",
                    dependency.as_str()
                )));
            }
        }
        if spec.budget.max_runtime_seconds == 0 || spec.budget.max_events == 0 {
            return Err(DevelopmentError::InvalidInput(
                "task budgets must be positive".into(),
            ));
        }
        validate_verification(&spec.verification, 0)?;
        if let Some(worktree) = &spec.worktree {
            let worktree = std::fs::canonicalize(worktree)?;
            if worktree == Path::new("/")
                || (!worktree.starts_with(&self.root) && !worktree.join(".git").exists())
            {
                return Err(DevelopmentError::PathOutsideWorkspace(worktree));
            }
        }
        Ok(())
    }

    fn task_for_agent(&self, agent: &AgentId) -> Option<TaskId> {
        self.tasks
            .iter()
            .find(|(_, record)| record.snapshot.assigned_agent.as_ref() == Some(agent))
            .map(|(id, _)| id.clone())
    }

    fn record(&self, id: &TaskId) -> DevelopmentResult<&TaskRecord> {
        self.tasks
            .get(id)
            .ok_or_else(|| DevelopmentError::NotFound(format!("task {}", id.as_str())))
    }

    fn record_mut(&mut self, id: &TaskId) -> DevelopmentResult<&mut TaskRecord> {
        self.tasks
            .get_mut(id)
            .ok_or_else(|| DevelopmentError::NotFound(format!("task {}", id.as_str())))
    }
}

enum VerificationOutcome {
    Passed(Vec<TaskEvidence>),
    Waiting,
    Failed(String, Vec<TaskEvidence>),
}

fn verify(
    requirement: &VerificationRequirement,
    worktree: &Path,
    evidence: &[TaskEvidence],
) -> DevelopmentResult<VerificationOutcome> {
    match requirement {
        VerificationRequirement::Settled => Ok(VerificationOutcome::Passed(Vec::new())),
        VerificationRequirement::Command {
            command,
            expected_exit,
            timeout_seconds,
        } => verify_command(worktree, command, *expected_exit, *timeout_seconds),
        VerificationRequirement::All { requirements } => {
            let mut collected = Vec::new();
            for requirement in requirements {
                match verify(requirement, worktree, evidence)? {
                    VerificationOutcome::Passed(mut current) => collected.append(&mut current),
                    VerificationOutcome::Waiting => return Ok(VerificationOutcome::Waiting),
                    VerificationOutcome::Failed(error, mut current) => {
                        collected.append(&mut current);
                        return Ok(VerificationOutcome::Failed(error, collected));
                    }
                }
            }
            Ok(VerificationOutcome::Passed(collected))
        }
        requirement => {
            let kind = verification_kind(requirement);
            match evidence.iter().rev().find(|item| item.kind == kind) {
                Some(item) if evidence_satisfies(requirement, item) => {
                    Ok(VerificationOutcome::Passed(Vec::new()))
                }
                Some(item) if item.passed == Some(false) => Ok(VerificationOutcome::Failed(
                    format!("{kind} verification failed"),
                    Vec::new(),
                )),
                _ => Ok(VerificationOutcome::Waiting),
            }
        }
    }
}

fn evidence_satisfies(requirement: &VerificationRequirement, evidence: &TaskEvidence) -> bool {
    if evidence.passed != Some(true) {
        return false;
    }
    match requirement {
        VerificationRequirement::LspDiagnostics { max_errors } => evidence
            .details
            .get("errors")
            .and_then(Value::as_u64)
            .is_some_and(|errors| errors <= *max_errors),
        VerificationRequirement::SemanticRegression {
            maximum_regressions,
            ..
        } => evidence
            .details
            .get("regressions")
            .and_then(Value::as_u64)
            .is_some_and(|regressions| regressions <= *maximum_regressions),
        VerificationRequirement::GitChange {
            require_changes,
            require_clean,
        } => {
            evidence.details.get("hasChanges").and_then(Value::as_bool) == Some(*require_changes)
                && (!*require_clean
                    || evidence.details.get("clean").and_then(Value::as_bool) == Some(true))
        }
        _ => true,
    }
}

fn verify_command(
    worktree: &Path,
    command: &str,
    expected_exit: i32,
    timeout_seconds: u64,
) -> DevelopmentResult<VerificationOutcome> {
    let mut process = if cfg!(windows) {
        let mut command_process = Command::new("cmd.exe");
        command_process.args(["/d", "/s", "/c", command]);
        command_process
    } else {
        let mut command_process = Command::new("sh");
        command_process.args(["-lc", command]);
        command_process
    };
    let mut child = process
        .current_dir(worktree)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;
    let deadline = Instant::now() + Duration::from_secs(timeout_seconds);
    loop {
        if let Some(status) = child.try_wait()? {
            let actual = status.code().unwrap_or(-1);
            let passed = actual == expected_exit;
            let observed = TaskEvidence {
                kind: "command".into(),
                actor: "glass-task-verifier".into(),
                source: "trusted-command".into(),
                passed: Some(passed),
                observed_at_ms: now_ms(),
                details: serde_json::json!({
                    "command": command,
                    "expectedExit": expected_exit,
                    "actualExit": actual,
                }),
            };
            return Ok(if passed {
                VerificationOutcome::Passed(vec![observed])
            } else {
                VerificationOutcome::Failed(
                    format!("verification command exited {actual}, expected {expected_exit}"),
                    vec![observed],
                )
            });
        }
        if Instant::now() >= deadline {
            child.kill()?;
            child.wait()?;
            return Ok(VerificationOutcome::Failed(
                "verification command timed out".into(),
                vec![TaskEvidence {
                    kind: "command".into(),
                    actor: "glass-task-verifier".into(),
                    source: "trusted-command".into(),
                    passed: Some(false),
                    observed_at_ms: now_ms(),
                    details: serde_json::json!({"command":command,"timedOut":true}),
                }],
            ));
        }
        thread::sleep(Duration::from_millis(10));
    }
}

fn verification_kind(requirement: &VerificationRequirement) -> &'static str {
    match requirement {
        VerificationRequirement::LspDiagnostics { .. } => "lspDiagnostics",
        VerificationRequirement::BrowserWorkflow { .. } => "browserWorkflow",
        VerificationRequirement::SemanticRegression { .. } => "semanticRegression",
        VerificationRequirement::DebuggerAssertion { .. } => "debuggerAssertion",
        VerificationRequirement::GitChange { .. } => "gitChange",
        VerificationRequirement::TrustedCustom { .. } => "trustedCustom",
        _ => "unknown",
    }
}

fn event_tokens(value: &Value) -> u64 {
    value
        .pointer("/message/usage/totalTokens")
        .or_else(|| value.pointer("/usage/totalTokens"))
        .and_then(Value::as_u64)
        .or_else(|| {
            let usage = value
                .pointer("/message/usage")
                .or_else(|| value.get("usage"))?;
            Some(
                ["input", "output", "cacheRead", "cacheWrite"]
                    .iter()
                    .filter_map(|name| usage.get(name).and_then(Value::as_u64))
                    .fold(0_u64, u64::saturating_add),
            )
        })
        .unwrap_or(0)
}

fn validate_verification(
    requirement: &VerificationRequirement,
    depth: usize,
) -> DevelopmentResult<()> {
    if depth > 8 {
        return Err(DevelopmentError::InvalidInput(
            "task verification nesting exceeds 8".into(),
        ));
    }
    match requirement {
        VerificationRequirement::Command {
            command,
            timeout_seconds,
            ..
        } => {
            if command.is_empty()
                || command.len() > MAX_VERIFIER_COMMAND_BYTES
                || command.contains('\0')
                || *timeout_seconds == 0
                || *timeout_seconds > 600
            {
                return Err(DevelopmentError::InvalidInput(
                    "invalid task verification command".into(),
                ));
            }
        }
        VerificationRequirement::All { requirements } => {
            if requirements.is_empty() || requirements.len() > 16 {
                return Err(DevelopmentError::InvalidInput(
                    "task verification group must contain 1..=16 requirements".into(),
                ));
            }
            for requirement in requirements {
                validate_verification(requirement, depth + 1)?;
            }
        }
        _ => {}
    }
    Ok(())
}

fn push_evidence(record: &mut TaskRecord, evidence: TaskEvidence) {
    if record.snapshot.evidence.len() == MAX_TASK_EVIDENCE {
        record.snapshot.evidence.remove(0);
    }
    record.snapshot.evidence.push(evidence);
    record.snapshot.updated_at_ms = now_ms();
}

fn validate_text(label: &str, value: &str) -> DevelopmentResult<()> {
    if value.is_empty() || value.len() > MAX_TASK_TEXT_BYTES || value.contains('\0') {
        return Err(DevelopmentError::InvalidInput(format!(
            "{label} must contain 1..={MAX_TASK_TEXT_BYTES} bytes without NUL"
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

    static NEXT_TEST_ROOT: AtomicU64 = AtomicU64::new(1);

    #[derive(Default)]
    struct FakeAgents {
        next: u64,
        next_event: u64,
        snapshots: BTreeMap<AgentId, AgentSnapshot>,
        events: Vec<AgentEvent>,
        prompts: Vec<(AgentId, String)>,
    }

    impl FakeAgents {
        fn emit(&mut self, id: &AgentId, kind: &str, payload: Value) {
            self.next_event += 1;
            self.events.push(AgentEvent {
                sequence: self.next_event,
                agent_id: id.clone(),
                timestamp_ms: now_ms(),
                kind: kind.into(),
                payload,
            });
        }

        fn snapshot(id: AgentId, spec: AgentSpec) -> AgentSnapshot {
            AgentSnapshot {
                id,
                role: spec.role,
                task: spec.task,
                status: AgentStatus::Idle,
                dependencies: Vec::new(),
                model: spec.model,
                thinking: spec.thinking,
                worktree: spec.worktree.unwrap(),
                unrestricted: spec.unrestricted,
                created_at_ms: now_ms(),
                started_at_ms: Some(now_ms()),
                updated_at_ms: now_ms(),
                event_count: 0,
                dropped_event_count: 0,
                last_error: None,
                last_response_id: None,
                evidence: Vec::new(),
            }
        }
    }

    impl TaskAgentBackend for FakeAgents {
        fn refresh_agents(&mut self) -> DevelopmentResult<()> {
            Ok(())
        }

        fn create_agent(&mut self, spec: AgentSpec) -> DevelopmentResult<AgentId> {
            self.next += 1;
            let id = AgentId::parse(format!("agent-{:04}", self.next))?;
            self.snapshots
                .insert(id.clone(), Self::snapshot(id.clone(), spec));
            self.emit(&id, "ready", Value::Null);
            Ok(id)
        }

        fn agent_snapshots(&mut self) -> DevelopmentResult<Vec<AgentSnapshot>> {
            Ok(self.snapshots.values().cloned().collect())
        }

        fn agent_events(&mut self, since: u64) -> DevelopmentResult<Vec<AgentEvent>> {
            Ok(self
                .events
                .iter()
                .filter(|event| event.sequence > since)
                .cloned()
                .collect())
        }

        fn prompt_agent(&mut self, id: &AgentId, prompt: String) -> DevelopmentResult<()> {
            self.prompts.push((id.clone(), prompt));
            self.emit(id, "requestStarted", Value::String("request-1".into()));
            self.emit(id, "agent_settled", Value::Null);
            Ok(())
        }

        fn cancel_agent(&mut self, id: &AgentId) -> DevelopmentResult<()> {
            self.snapshots.get_mut(id).unwrap().status = AgentStatus::Cancelled;
            Ok(())
        }

        fn complete_agent(&mut self, id: &AgentId) -> DevelopmentResult<()> {
            self.snapshots.get_mut(id).unwrap().status = AgentStatus::Completed;
            Ok(())
        }
    }

    fn scheduler() -> (PathBuf, TaskScheduler, FakeAgents) {
        let root = std::env::temp_dir().join(format!(
            "glass-task-scheduler-test-{}-{}",
            std::process::id(),
            NEXT_TEST_ROOT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&root).unwrap();
        let scheduler = TaskScheduler::new(&root).unwrap();
        (root, scheduler, FakeAgents::default())
    }

    fn settle(scheduler: &mut TaskScheduler, agents: &mut FakeAgents) {
        scheduler.refresh(agents).unwrap();
        scheduler.refresh(agents).unwrap();
    }

    #[test]
    fn tasks_dispatch_prompts_verify_and_wake_dag_dependents() {
        let (root, mut scheduler, mut agents) = scheduler();
        let first = scheduler
            .create(&mut agents, TaskSpec::new("investigate", "inspect failure"))
            .unwrap();
        settle(&mut scheduler, &mut agents);
        assert_eq!(
            scheduler.snapshot(&mut agents, &first).unwrap().state,
            TaskState::Succeeded
        );
        assert_eq!(agents.prompts[0].1, "inspect failure");

        let mut dependent = TaskSpec::new("repair", "apply repair");
        dependent.dependencies.push(first);
        let dependent = scheduler.create(&mut agents, dependent).unwrap();
        settle(&mut scheduler, &mut agents);
        assert_eq!(
            scheduler.snapshot(&mut agents, &dependent).unwrap().state,
            TaskState::Succeeded
        );
        assert_eq!(agents.prompts[1].1, "apply repair");
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn eight_ready_tasks_dispatch_before_integration_dependency_wakes() {
        let (root, mut scheduler, mut agents) = scheduler();
        let mut leaves = Vec::new();
        for index in 0..8 {
            leaves.push(
                scheduler
                    .create(
                        &mut agents,
                        TaskSpec::new(format!("worker-{index}"), format!("inspect shard {index}")),
                    )
                    .unwrap(),
            );
        }
        assert_eq!(scheduler.tasks.len(), 8);

        let mut integration = TaskSpec::new("integration", "verify all shards");
        integration.dependencies = leaves.clone();
        let integration = scheduler.create(&mut agents, integration).unwrap();
        assert_eq!(scheduler.tasks.len(), 9);
        settle(&mut scheduler, &mut agents);
        settle(&mut scheduler, &mut agents);

        for leaf in leaves {
            assert_eq!(
                scheduler.snapshot(&mut agents, &leaf).unwrap().state,
                TaskState::Succeeded
            );
        }
        assert_eq!(
            scheduler.snapshot(&mut agents, &integration).unwrap().state,
            TaskState::Succeeded
        );
        assert_eq!(agents.prompts.len(), 9);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn verification_failure_retries_then_blocks_descendants() {
        let (root, mut scheduler, mut agents) = scheduler();
        let mut failing = TaskSpec::new("verify", "produce candidate");
        failing.verification = VerificationRequirement::Command {
            command: "exit 7".into(),
            expected_exit: 0,
            timeout_seconds: 2,
        };
        failing.retry = RetryPolicy {
            max_retries: 1,
            backoff_seconds: 0,
        };
        let failing = scheduler.create(&mut agents, failing).unwrap();
        let mut child = TaskSpec::new("integration", "integrate candidate");
        child.dependencies.push(failing.clone());
        let child = scheduler.create(&mut agents, child).unwrap();
        for _ in 0..6 {
            scheduler.refresh(&mut agents).unwrap();
        }
        let failed = scheduler.snapshot(&mut agents, &failing).unwrap();
        assert_eq!(failed.state, TaskState::Failed);
        assert_eq!(failed.attempt, 2);
        assert_eq!(
            scheduler.snapshot(&mut agents, &child).unwrap().state,
            TaskState::Blocked
        );
        assert_eq!(agents.prompts.len(), 2);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn external_verification_evidence_is_proof_not_agent_claim() {
        let (root, mut scheduler, mut agents) = scheduler();
        let mut spec = TaskSpec::new("diagnostics", "repair diagnostics");
        spec.verification = VerificationRequirement::LspDiagnostics { max_errors: 0 };
        let id = scheduler.create(&mut agents, spec).unwrap();
        settle(&mut scheduler, &mut agents);
        assert_eq!(
            scheduler.snapshot(&mut agents, &id).unwrap().state,
            TaskState::Waiting
        );
        scheduler
            .submit_evidence(
                &id,
                "lspDiagnostics",
                "resident-lsp",
                "task-test",
                true,
                serde_json::json!({"errors":0,"source":"resident-lsp"}),
            )
            .unwrap();
        scheduler.refresh(&mut agents).unwrap();
        assert_eq!(
            scheduler.snapshot(&mut agents, &id).unwrap().state,
            TaskState::Succeeded
        );
        std::fs::remove_dir_all(root).unwrap();
    }
}
