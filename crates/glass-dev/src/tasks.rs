//! Task DAG scheduling and verification above Glass's resident agent engines.

use crate::agents::{AgentEvent, AgentId, AgentRegistry, AgentSnapshot, AgentSpec, AgentStatus};
use crate::development::{DevelopmentError, DevelopmentResult};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const MAX_TASKS: usize = 128;
const MAX_DEPENDENCIES: usize = 32;
const MAX_TASK_EVIDENCE: usize = 128;
const MAX_PERSISTED_TASK_ERROR_BYTES: usize = 160;
const MAX_TASK_TEXT_BYTES: usize = 128 * 1024;
const MAX_VERIFIER_COMMAND_BYTES: usize = 16 * 1024;
const TASK_CHECKPOINT_VERSION: u32 = 1;
const MAX_TASK_CHECKPOINT_BYTES: u64 = 2 * 1024 * 1024;
const TASK_CHECKPOINT_MUTATION_RESERVE: usize = 256;
const TASK_CHECKPOINT_RELATIVE_PATH: &str = ".glass/tasks/scheduler.json";
static NEXT_CHECKPOINT_TEMP: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

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
    /// Human-readable label for terminal and TUI surfaces.
    pub fn label(&self) -> &'static str {
        match self {
            Self::Queued => "queued",
            Self::Ready => "ready",
            Self::Running => "running",
            Self::Waiting => "waiting",
            Self::Verifying => "verifying",
            Self::Succeeded => "succeeded",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
            Self::Paused => "paused",
            Self::Blocked => "blocked",
        }
    }
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
    /// Resolve a deterministic project verifier when the task is created.
    #[default]
    Inferred,
    /// Deliberately accept agent settlement without deterministic proof.
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
    pub budget: TaskBudget,
    #[serde(default)]
    pub verification: VerificationRequirement,
    #[serde(default)]
    pub retry: RetryPolicy,
}

fn default_role() -> String {
    "developer".into()
}

fn crew_spec(role: &str, title: impl Into<String>, goal: &str, prompt: String) -> TaskSpec {
    let mut spec = TaskSpec::new(title, prompt);
    spec.role = role.to_string();
    spec.goal = goal.to_string();
    spec
}

fn crew_prompt(role: &str, goal: &str) -> String {
    match role {
        "architect" => format!(
            "You are the architect for: {goal}\nWrite a bounded plan, file-level proposals, and UI verify predicates. Do not implement production code."
        ),
        "implementer" => format!(
            "You are the implementer for: {goal}\nFollow the architect plan and implement the approved work in your isolated worktree. Keep the change minimal."
        ),
        "tester" => format!(
            "You are the tester for: {goal}\nRun the detected project suite. Do not edit production code. Record failing tests as evidence."
        ),
        "reviewer" => format!(
            "You are the reviewer for: {goal}\nYou cannot write, patch, or save files. Inspect each implementer's proposals in their isolated worktrees and return accept or reject with file-level reasons."
        ),
        "browser" => format!(
            "You are the browser operator for: {goal}\nDrive the running app. Do not touch src/. Prove the UI with causal verify predicates and attach evidence."
        ),
        other => format!("You are {other} for: {goal}"),
    }
}

fn implementer_prompt(goal: &str, index: usize, total: usize, worktree: &Path) -> String {
    let slice = if index == 1 {
        "production source from the architect plan"
    } else {
        "tests, types, and remaining files from the architect plan"
    };
    format!(
        "You are implementer {index} of {total} for: {goal}\nYou work only in this isolated worktree: {}\nYou continue from a forked resident-agent session of the architect when one exists. Do not edit other implementers' trees. Own {slice}. Follow the architect plan and implement the approved work. Keep the change minimal.",
        worktree.display()
    )
}

fn architect_session_file<B: TaskAgentBackend>(
    scheduler: &TaskScheduler,
    agents: &mut B,
    snapshot: &TaskSnapshot,
) -> Option<PathBuf> {
    if snapshot.role != "implementer" {
        return None;
    }
    let architect = snapshot.dependencies.iter().find_map(|dep| {
        let task = scheduler.tasks.get(dep)?;
        (task.snapshot.role == "architect")
            .then(|| task.snapshot.assigned_agent.clone())
            .flatten()
    })?;
    agents
        .agent_snapshots()
        .ok()?
        .into_iter()
        .find(|item| item.id == architect)?
        .session_file
        .map(PathBuf::from)
}

fn tester_prompt(goal: &str, worktree: &Path) -> String {
    format!(
        "You are the tester for: {goal}\nRun the detected project suite in this isolated worktree: {}\nDo not edit production code. Record failing tests as evidence.",
        worktree.display()
    )
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
    /// True when an assigned task was active when Glass last shut down. The
    /// assignment is retained for inspection, but the task is never replayed
    /// without an explicit operator action.
    #[serde(default)]
    pub recovery_pending: bool,
    /// True when bounded evidence history has been pruned or compressed.
    #[serde(default)]
    pub evidence_truncated: bool,
}

/// Durable overnight-crew artifact written under `{root}/.glass/crew`.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CrewWake {
    pub id: String,
    pub goal: String,
    pub worktree: Option<String>,
    pub checkpoint: String,
    pub created_at_ms: u128,
    pub tasks: Vec<CrewWakeMember>,
    #[serde(default)]
    pub diff: String,
    #[serde(default)]
    pub tests: String,
    #[serde(default)]
    pub verify: String,
    #[serde(default)]
    pub page: String,
    #[serde(default)]
    pub accept: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CrewWakeMember {
    pub id: String,
    pub role: String,
    pub title: String,
    pub state: String,
    #[serde(default)]
    pub worktree: Option<String>,
}

/// Live TUI evidence folded into a crew wake on review.
#[derive(Debug, Clone, Default)]
pub struct CrewWakeLiveEvidence {
    pub verify: Option<String>,
    pub page: Option<String>,
    pub accept: Option<String>,
}

impl CrewWake {
    pub fn render(&self) -> String {
        let mut lines = vec![
            format!("WAKE {}", self.id),
            format!("  goal {}", self.goal),
            format!("  worktree {}", self.worktree.as_deref().unwrap_or("—")),
            format!("  checkpoint {}", self.checkpoint),
            format!(
                "  accept {}",
                if self.accept.is_empty() {
                    "none"
                } else {
                    self.accept.as_str()
                }
            ),
        ];
        append_wake_section(&mut lines, "VERIFY", &self.verify, 12);
        append_wake_section(&mut lines, "TESTS", &self.tests, 16);
        append_wake_section(&mut lines, "PAGE", &self.page, 8);
        append_wake_section(&mut lines, "DIFF", &self.diff, 40);
        lines.push(String::new());
        lines.push("CREW".into());
        if self.tasks.is_empty() {
            lines.push("  none queued".into());
        } else {
            for task in &self.tasks {
                match task.worktree.as_deref() {
                    Some(worktree) if !worktree.is_empty() => {
                        lines.push(format!(
                            "  {} {} {} · {worktree}",
                            task.role, task.id, task.state
                        ));
                    }
                    _ => lines.push(format!("  {} {} {}", task.role, task.id, task.state)),
                }
            }
        }
        lines.join("\n")
    }
}

fn append_wake_section(lines: &mut Vec<String>, title: &str, body: &str, limit: usize) {
    if body.trim().is_empty() {
        return;
    }
    lines.push(String::new());
    lines.push(title.into());
    for line in body.lines().take(limit) {
        lines.push(format!("  {line}"));
    }
}

/// Persist a crew wake object and replace `{root}/.glass/crew/latest.json`.
pub(crate) fn persist_crew_wake(root: &Path, wake: &CrewWake) -> DevelopmentResult<()> {
    let dir = root.join(".glass").join("crew");
    std::fs::create_dir_all(&dir)?;
    let encoded = serde_json::to_vec_pretty(wake)?;
    std::fs::write(dir.join(format!("{}.json", wake.id)), &encoded)?;
    std::fs::write(dir.join("latest.json"), encoded)?;
    Ok(())
}

/// Load the most recently queued overnight crew, if one exists.
pub fn load_latest_crew_wake(root: &Path) -> Option<CrewWake> {
    let path = root.join(".glass").join("crew").join("latest.json");
    serde_json::from_slice(&std::fs::read(path).ok()?).ok()
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

fn cancel_task_agent_if_live<B: TaskAgentBackend>(
    agents: &mut B,
    id: &AgentId,
) -> DevelopmentResult<()> {
    let snapshots = agents.agent_snapshots()?;
    let Some(snapshot) = snapshots.iter().find(|snapshot| snapshot.id == *id) else {
        return Ok(());
    };
    if matches!(
        snapshot.status,
        AgentStatus::RecoveryPending
            | AgentStatus::Unavailable
            | AgentStatus::Completed
            | AgentStatus::Failed
            | AgentStatus::Cancelled
    ) {
        return Ok(());
    }
    agents.cancel_agent(id)
}

pub(crate) struct TaskScheduler {
    root: PathBuf,
    tasks: BTreeMap<TaskId, TaskRecord>,
    next_task: u64,
    agent_event_cursor: u64,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct TaskSchedulerCheckpoint {
    format_version: u32,
    project_root: PathBuf,
    next_task: u64,
    tasks: Vec<TaskRecordCheckpoint>,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct TaskRecordCheckpoint {
    snapshot: TaskSnapshot,
    next_retry_at_ms: Option<u128>,
}

impl TaskScheduler {
    pub(crate) fn new(root: impl AsRef<Path>) -> DevelopmentResult<Self> {
        let root = std::fs::canonicalize(root)?;
        validate_task_checkpoint_paths(&root)?;
        let checkpoint_path = root.join(TASK_CHECKPOINT_RELATIVE_PATH);
        let (tasks, next_task) = if checkpoint_path.exists() {
            load_task_checkpoint(&checkpoint_path, &root)?
        } else {
            (BTreeMap::new(), 1)
        };
        let mut scheduler = Self {
            root,
            tasks,
            next_task,
            agent_event_cursor: 0,
        };
        for record in scheduler.tasks.values() {
            scheduler.validate_snapshot(&record.snapshot)?;
        }
        let mut recovered = false;
        let mut recovered_ids = Vec::new();
        for record in scheduler.tasks.values_mut() {
            if matches!(
                record.snapshot.state,
                TaskState::Ready | TaskState::Running | TaskState::Waiting | TaskState::Verifying
            ) && record.snapshot.assigned_agent.is_some()
            {
                record.snapshot.state = TaskState::Paused;
                record.snapshot.recovery_pending = true;
                record.snapshot.last_error = Some(
                    "recovery pending: prior agent outcome is unknown; inspect before resuming"
                        .into(),
                );
                record.snapshot.updated_at_ms = now_ms();
                recovered = true;
                recovered_ids.push(record.snapshot.id.clone());
            }
        }
        if recovered {
            scheduler.persist_recovery_checkpoint(&recovered_ids)?;
        }
        Ok(scheduler)
    }

    pub fn create<B: TaskAgentBackend>(
        &mut self,
        agents: &mut B,
        mut spec: TaskSpec,
    ) -> DevelopmentResult<TaskId> {
        self.refresh(agents)?;
        if spec.verification == VerificationRequirement::Inferred {
            let worktree = spec.worktree.as_deref().unwrap_or(&self.root);
            spec.verification = infer_verification(worktree);
        }
        self.validate_spec(&spec)?;
        if self.tasks.len() >= MAX_TASKS {
            return Err(DevelopmentError::Conflict(format!(
                "task scheduler reached its {MAX_TASKS} task limit"
            )));
        }
        let id = TaskId(format!("task-{:04}", self.next_task));
        let next_task = self
            .next_task
            .checked_add(1)
            .ok_or_else(|| DevelopmentError::Conflict("task identifier space exhausted".into()))?;
        let now = now_ms();
        let mut projected = TaskSnapshot {
            id: id.clone(),
            title: spec.title.clone(),
            goal: spec.goal.clone(),
            prompt: spec.prompt.clone(),
            role: spec.role.clone(),
            dependencies: spec.dependencies.clone(),
            assigned_agent: Some(maximum_length_agent_id()?),
            worktree: spec.worktree.clone().unwrap_or_else(|| self.root.clone()),
            model: spec.model.clone(),
            thinking: spec.thinking.clone(),
            budget: spec.budget.clone(),
            verification: spec.verification.clone(),
            retry: spec.retry.clone(),
            state: TaskState::Ready,
            attempt: 1,
            observed_tokens: 0,
            created_at_ms: now,
            started_at_ms: None,
            completed_at_ms: None,
            updated_at_ms: now,
            last_error: None,
            evidence: Vec::new(),
            blocked_override: false,
            recovery_pending: false,
            evidence_truncated: false,
        };
        let checkpoint = self.checkpoint_bytes(None, Some(&projected), next_task)?;
        self.ensure_checkpoint_mutation_capacity(checkpoint.len())?;
        self.next_task = next_task;
        projected.assigned_agent = None;
        projected.state = TaskState::Queued;
        projected.attempt = 0;
        self.tasks.insert(
            id.clone(),
            TaskRecord {
                snapshot: projected,
                next_retry_at_ms: None,
            },
        );
        if let Err(error) = self.ensure_current_checkpoint_fits() {
            self.tasks.remove(&id);
            self.next_task = self.next_task.saturating_sub(1);
            return Err(error);
        }
        self.schedule_ready(agents)?;
        self.persist_checkpoint()?;
        Ok(id)
    }

    /// Queue the overnight factory crew with isolated implementer worktrees.
    pub fn create_crew<B: TaskAgentBackend>(
        &mut self,
        agents: &mut B,
        goal: &str,
        shared: Option<PathBuf>,
        implementer_trees: Vec<PathBuf>,
    ) -> DevelopmentResult<Vec<TaskId>> {
        validate_text("crew goal", goal)?;
        let trees = if implementer_trees.is_empty() {
            vec![shared.clone().unwrap_or_else(|| self.root.clone())]
        } else {
            implementer_trees
        };
        let mut architect = crew_spec(
            "architect",
            format!("architect: {goal}"),
            goal,
            crew_prompt("architect", goal),
        );
        architect.worktree = shared.clone();
        architect.verification = VerificationRequirement::Settled;
        let architect = self.create(agents, architect)?;
        let mut implementers = Vec::new();
        let mut testers = Vec::new();
        let total = trees.len();
        for (index, tree) in trees.into_iter().enumerate() {
            let label = if total == 1 {
                "implementer".into()
            } else {
                format!("implementer-{}", (b'a' + index as u8) as char)
            };
            let mut implementer = crew_spec(
                "implementer",
                format!("{label}: {goal}"),
                goal,
                implementer_prompt(goal, index + 1, total, &tree),
            );
            implementer.worktree = Some(tree.clone());
            implementer.verification = VerificationRequirement::Inferred;
            implementer.dependencies = vec![architect.clone()];
            let implementer = self.create(agents, implementer)?;
            let tester_label = if total == 1 {
                "tester".into()
            } else {
                format!("tester-{}", (b'a' + index as u8) as char)
            };
            let mut tester = crew_spec(
                "tester",
                format!("{tester_label}: {goal}"),
                goal,
                tester_prompt(goal, &tree),
            );
            tester.worktree = Some(tree);
            tester.verification = VerificationRequirement::Inferred;
            tester.dependencies = vec![implementer.clone()];
            let tester = self.create(agents, tester)?;
            implementers.push(implementer);
            testers.push(tester);
        }
        let mut reviewer = crew_spec(
            "reviewer",
            format!("reviewer: {goal}"),
            goal,
            crew_prompt("reviewer", goal),
        );
        reviewer.worktree = shared;
        reviewer.verification = VerificationRequirement::Settled;
        reviewer.dependencies = implementers.clone();
        let reviewer = self.create(agents, reviewer)?;
        let mut browser = crew_spec(
            "browser",
            format!("browser: {goal}"),
            goal,
            crew_prompt("browser", goal),
        );
        browser.verification = VerificationRequirement::BrowserWorkflow {
            assertion: goal.to_string(),
        };
        browser.dependencies = testers.clone();
        let browser = self.create(agents, browser)?;
        let mut ids = vec![architect];
        ids.extend(implementers);
        ids.extend(testers);
        ids.push(reviewer);
        ids.push(browser);
        Ok(ids)
    }

    pub fn refresh<B: TaskAgentBackend>(&mut self, agents: &mut B) -> DevelopmentResult<()> {
        agents.refresh_agents()?;
        let snapshots = agents.agent_snapshots()?;
        let events = agents.agent_events(self.agent_event_cursor)?;
        for event in events {
            self.apply_agent_event(agents, &event)?;
            self.agent_event_cursor = self.agent_event_cursor.max(event.sequence);
        }
        self.apply_agent_failures(&snapshots)?;
        self.enforce_task_budgets(agents)?;
        self.verify_pending(agents)?;
        self.propagate_blocked()?;
        self.schedule_ready(agents)?;
        self.persist_checkpoint()
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
        let evidence = TaskEvidence {
            kind: kind.into(),
            actor: actor.into(),
            source: source.into(),
            passed: Some(passed),
            observed_at_ms: now_ms(),
            details,
        };
        validate_text("task evidence kind", &evidence.kind)?;
        validate_text("task evidence actor", &evidence.actor)?;
        validate_text("task evidence source", &evidence.source)?;
        let mut projected = self.record(id)?.snapshot.clone();
        let mut projected_record = TaskRecord {
            snapshot: projected.clone(),
            next_retry_at_ms: self.record(id)?.next_retry_at_ms,
        };
        push_evidence(&mut projected_record, evidence.clone());
        projected = projected_record.snapshot;
        if projected.state == TaskState::Waiting {
            projected.state = TaskState::Verifying;
        }
        self.ensure_snapshot_checkpoint_fits(id, &projected)?;
        let record = self.record_mut(id)?;
        push_evidence(record, evidence);
        if record.snapshot.state == TaskState::Waiting {
            record.snapshot.state = TaskState::Verifying;
        }
        self.persist_checkpoint()
    }

    pub fn pause<B: TaskAgentBackend>(
        &mut self,
        agents: &mut B,
        id: &TaskId,
    ) -> DevelopmentResult<()> {
        self.refresh(agents)?;
        let mut projected = self.record(id)?.snapshot.clone();
        if projected.state.terminal() {
            return Err(DevelopmentError::Conflict(
                "terminal task cannot be paused".into(),
            ));
        }
        projected.assigned_agent = None;
        projected.state = TaskState::Paused;
        projected.recovery_pending = false;
        projected.last_error = None;
        projected.updated_at_ms = now_ms();
        self.ensure_snapshot_checkpoint_fits(id, &projected)?;
        let agent = self.record(id)?.snapshot.assigned_agent.clone();
        if let Some(agent) = agent {
            cancel_task_agent_if_live(agents, &agent)?;
        }
        let record = self.record_mut(id)?;
        record.snapshot.assigned_agent = None;
        record.snapshot.state = TaskState::Paused;
        record.snapshot.recovery_pending = false;
        record.snapshot.last_error = None;
        record.snapshot.updated_at_ms = now_ms();
        self.persist_checkpoint()
    }

    pub fn resume<B: TaskAgentBackend>(
        &mut self,
        agents: &mut B,
        id: &TaskId,
    ) -> DevelopmentResult<()> {
        let mut projected = self.record(id)?.snapshot.clone();
        if projected.state != TaskState::Paused {
            return Err(DevelopmentError::Conflict("task is not paused".into()));
        }
        projected.state = TaskState::Queued;
        projected.assigned_agent = None;
        projected.recovery_pending = false;
        projected.last_error = None;
        projected.updated_at_ms = now_ms();
        self.ensure_snapshot_checkpoint_fits(id, &projected)?;
        self.ensure_schedule_checkpoint_fits(id, &projected)?;
        let record = self.record_mut(id)?;

        record.snapshot.state = TaskState::Queued;
        record.snapshot.assigned_agent = None;
        record.snapshot.recovery_pending = false;
        record.snapshot.last_error = None;
        record.snapshot.updated_at_ms = now_ms();
        self.schedule_ready(agents)?;
        self.persist_checkpoint()
    }

    pub fn cancel<B: TaskAgentBackend>(
        &mut self,
        agents: &mut B,
        id: &TaskId,
    ) -> DevelopmentResult<()> {
        self.refresh(agents)?;
        let current = self.record(id)?.snapshot.clone();
        let mut projected = current.clone();
        if !projected.state.terminal() {
            projected.state = TaskState::Cancelled;
            projected.recovery_pending = false;
            projected.completed_at_ms = Some(now_ms());
            projected.updated_at_ms = now_ms();
        }
        self.ensure_snapshot_checkpoint_fits(id, &projected)?;
        let agent = current.assigned_agent.clone();
        if let Some(agent) = agent {
            cancel_task_agent_if_live(agents, &agent)?;
        }
        let record = self.record_mut(id)?;
        if !record.snapshot.state.terminal() {
            record.snapshot.state = TaskState::Cancelled;
            record.snapshot.recovery_pending = false;
            record.snapshot.completed_at_ms = Some(now_ms());
            record.snapshot.updated_at_ms = now_ms();
        }
        self.propagate_blocked()?;
        self.persist_checkpoint()
    }

    pub fn retry<B: TaskAgentBackend>(
        &mut self,
        agents: &mut B,
        id: &TaskId,
    ) -> DevelopmentResult<()> {
        let current = self.record(id)?.snapshot.clone();
        if !matches!(current.state, TaskState::Failed | TaskState::Blocked) {
            return Err(DevelopmentError::Conflict(
                "only failed or blocked tasks can be retried".into(),
            ));
        }
        let mut projected = current;
        projected.state = TaskState::Queued;
        projected.assigned_agent = None;
        projected.recovery_pending = false;
        projected.completed_at_ms = None;
        projected.last_error = None;
        projected.updated_at_ms = now_ms();
        self.ensure_snapshot_checkpoint_fits(id, &projected)?;
        self.ensure_schedule_checkpoint_fits(id, &projected)?;
        let record = self.record_mut(id)?;
        record.snapshot.state = TaskState::Queued;
        record.snapshot.assigned_agent = None;
        record.snapshot.recovery_pending = false;
        record.snapshot.completed_at_ms = None;
        record.snapshot.last_error = None;
        record.snapshot.updated_at_ms = now_ms();
        record.next_retry_at_ms = None;
        self.schedule_ready(agents)?;
        self.persist_checkpoint()
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
        if let Some(value) = &model {
            validate_text("task model", value)?;
        }
        if let Some(value) = &thinking {
            validate_text("task thinking", value)?;
        }
        let current = self.record(id)?.snapshot.clone();
        if current.state.terminal() {
            return Err(DevelopmentError::Conflict(
                "terminal task cannot be reassigned".into(),
            ));
        }
        let mut projected = current.clone();
        projected.role.clone_from(&role);
        projected.model.clone_from(&model);
        projected.thinking.clone_from(&thinking);
        projected.assigned_agent = None;
        projected.state = TaskState::Queued;
        projected.recovery_pending = false;
        projected.last_error = None;
        projected.updated_at_ms = now_ms();
        self.ensure_snapshot_checkpoint_fits(id, &projected)?;
        self.ensure_schedule_checkpoint_fits(id, &projected)?;
        let agent = current.assigned_agent.clone();
        if let Some(agent) = agent {
            cancel_task_agent_if_live(agents, &agent)?;
        }
        let record = self.record_mut(id)?;
        record.snapshot.role = role;
        record.snapshot.model = model;
        record.snapshot.thinking = thinking;
        record.snapshot.assigned_agent = None;
        record.snapshot.state = TaskState::Queued;
        record.snapshot.recovery_pending = false;
        record.snapshot.last_error = None;
        record.snapshot.updated_at_ms = now_ms();
        self.schedule_ready(agents)?;
        self.persist_checkpoint()
    }

    pub fn override_blocked<B: TaskAgentBackend>(
        &mut self,
        agents: &mut B,
        id: &TaskId,
    ) -> DevelopmentResult<()> {
        let mut projected = self.record(id)?.snapshot.clone();
        if projected.state != TaskState::Blocked {
            return Err(DevelopmentError::Conflict("task is not blocked".into()));
        }
        projected.blocked_override = true;
        projected.state = TaskState::Queued;
        projected.recovery_pending = false;
        projected.last_error = None;
        projected.updated_at_ms = now_ms();
        self.ensure_snapshot_checkpoint_fits(id, &projected)?;
        self.ensure_schedule_checkpoint_fits(id, &projected)?;
        let record = self.record_mut(id)?;
        record.snapshot.blocked_override = true;
        record.snapshot.state = TaskState::Queued;
        record.snapshot.recovery_pending = false;
        record.snapshot.last_error = None;
        record.snapshot.updated_at_ms = now_ms();
        self.schedule_ready(agents)?;
        self.persist_checkpoint()
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
            let snapshot = self.record(&id)?.snapshot.clone();
            let mut projected = snapshot.clone();
            projected.assigned_agent = Some(maximum_length_agent_id()?);
            projected.state = TaskState::Ready;
            projected.attempt = projected.attempt.saturating_add(1);
            projected.updated_at_ms = now_ms();
            self.ensure_snapshot_checkpoint_fits(&id, &projected)?;
            let mut spec = AgentSpec::new(
                snapshot.role.clone(),
                format!("task {}: {}", id.as_str(), snapshot.title),
            );
            spec.model.clone_from(&snapshot.model);
            spec.thinking.clone_from(&snapshot.thinking);
            spec.worktree = Some(snapshot.worktree.clone());
            spec.fork_from = architect_session_file(self, agents, &snapshot);
            spec.max_runtime_seconds = Some(snapshot.budget.max_runtime_seconds);
            spec.max_events = Some(snapshot.budget.max_events);
            let agent = agents.create_agent(spec)?;
            let record = self.record_mut(&id)?;
            record.snapshot.assigned_agent = Some(agent);
            record.snapshot.state = TaskState::Ready;
            record.snapshot.attempt = record.snapshot.attempt.saturating_add(1);
            record.snapshot.updated_at_ms = now_ms();
            record.next_retry_at_ms = None;
            self.persist_checkpoint()?;
        }
        Ok(())
    }

    fn apply_agent_event<B: TaskAgentBackend>(
        &mut self,
        agents: &mut B,
        event: &AgentEvent,
    ) -> DevelopmentResult<()> {
        let Some(task_id) = self.task_for_agent(&event.agent_id) else {
            return self.persist_checkpoint();
        };
        let state = self.record(&task_id)?.snapshot.state;
        match event.kind.as_str() {
            "ready" if state == TaskState::Ready => {
                let prompt = self.record(&task_id)?.snapshot.prompt.clone();
                let mut projected = self.record(&task_id)?.snapshot.clone();
                projected.state = TaskState::Running;
                projected.started_at_ms.get_or_insert_with(now_ms);
                projected.updated_at_ms = now_ms();
                self.ensure_snapshot_checkpoint_fits(&task_id, &projected)?;
                let record = self.record_mut(&task_id)?;
                record.snapshot.state = TaskState::Running;
                record.snapshot.started_at_ms.get_or_insert_with(now_ms);
                record.snapshot.updated_at_ms = now_ms();
                self.persist_checkpoint()?;
                agents.prompt_agent(&event.agent_id, prompt)?;
            }
            "agent_settled" if state == TaskState::Running => {
                let evidence = TaskEvidence {
                    kind: "agent.settled".into(),
                    actor: event.agent_id.as_str().into(),
                    source: "resident-agent-session".into(),
                    passed: Some(true),
                    observed_at_ms: event.timestamp_ms,
                    details: bounded_event_details(&event.payload),
                };
                let mut projected = self.record(&task_id)?.snapshot.clone();
                projected.state = TaskState::Verifying;
                projected.updated_at_ms = now_ms();
                push_snapshot_evidence(&mut projected, evidence.clone());
                self.fit_event_snapshot_checkpoint(&task_id, &mut projected)?;
            }
            kind => {
                let mut projected = self.record(&task_id)?.snapshot.clone();
                if kind == "message_end" {
                    projected.observed_tokens = projected
                        .observed_tokens
                        .saturating_add(event_tokens(&event.payload));
                }
                push_snapshot_evidence(
                    &mut projected,
                    TaskEvidence {
                        kind: format!("agent.{kind}"),
                        actor: event.agent_id.as_str().into(),
                        source: "resident-agent-session".into(),
                        passed: None,
                        observed_at_ms: event.timestamp_ms,
                        details: bounded_event_details(&event.payload),
                    },
                );
                self.fit_event_snapshot_checkpoint(&task_id, &mut projected)?;
            }
        }
        self.persist_checkpoint()?;
        Ok(())
    }

    fn apply_agent_failures(&mut self, snapshots: &[AgentSnapshot]) -> DevelopmentResult<()> {
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
            let mut projected = self.record(&id)?.snapshot.clone();
            if !projected.state.terminal() {
                projected.state = TaskState::Failed;
                projected.last_error = snapshot
                    .last_error
                    .clone()
                    .or_else(|| Some("assigned agent stopped before verification".into()));
                projected.last_error = projected.last_error.as_deref().map(bounded_task_error);
                validate_text(
                    "task error",
                    projected.last_error.as_deref().unwrap_or_default(),
                )?;
                projected.completed_at_ms = Some(now_ms());
                projected.updated_at_ms = now_ms();
                let replacements = self.fit_snapshot_checkpoint(&id, &mut projected, false)?;
                self.apply_event_snapshot_replacements(replacements)?;
            }
        }
        Ok(())
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
                    let mut projected = self.record(&id)?.snapshot.clone();
                    for item in &new_evidence {
                        push_snapshot_evidence(&mut projected, item.clone());
                    }
                    projected.state = TaskState::Succeeded;
                    projected.completed_at_ms = Some(now_ms());
                    projected.updated_at_ms = now_ms();
                    projected.last_error = None;
                    let replacements = self.fit_snapshot_checkpoint(
                        &id,
                        &mut projected,
                        !new_evidence.is_empty(),
                    )?;
                    let agent = self.record(&id)?.snapshot.assigned_agent.clone();
                    if let Some(agent) = agent {
                        agents.complete_agent(&agent)?;
                    }
                    self.apply_event_snapshot_replacements(replacements)?;
                    self.record_mut(&id)?.next_retry_at_ms = None;
                }
                VerificationOutcome::Waiting => {
                    let mut projected = self.record(&id)?.snapshot.clone();
                    projected.state = TaskState::Waiting;
                    projected.updated_at_ms = now_ms();
                    let replacements = self.fit_snapshot_checkpoint(&id, &mut projected, false)?;
                    self.apply_event_snapshot_replacements(replacements)?;
                }
                VerificationOutcome::Failed(error, new_evidence) => {
                    validate_text("task verification error", &error)?;
                    let mut projected = self.record(&id)?.snapshot.clone();
                    for item in &new_evidence {
                        push_snapshot_evidence(&mut projected, item.clone());
                    }
                    projected.assigned_agent = None;
                    projected.last_error = Some(bounded_task_error(&error));
                    projected.updated_at_ms = now_ms();
                    let (agent, retry, attempt) = {
                        let record = self.record(&id)?;
                        (
                            record.snapshot.assigned_agent.clone(),
                            record.snapshot.retry.clone(),
                            record.snapshot.attempt,
                        )
                    };
                    if attempt <= retry.max_retries {
                        projected.state = TaskState::Queued;
                    } else {
                        projected.state = TaskState::Failed;
                        projected.completed_at_ms = Some(now_ms());
                    }
                    let replacements = self.fit_snapshot_checkpoint(
                        &id,
                        &mut projected,
                        !new_evidence.is_empty(),
                    )?;
                    if let Some(agent) = agent {
                        cancel_task_agent_if_live(agents, &agent)?;
                    }
                    self.apply_event_snapshot_replacements(replacements)?;
                    let record = self.record_mut(&id)?;
                    if attempt <= retry.max_retries {
                        record.next_retry_at_ms = Some(
                            now_ms().saturating_add(u128::from(retry.backoff_seconds) * 1_000),
                        );
                    } else {
                        record.next_retry_at_ms = None;
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
            let mut projected = self.record(&id)?.snapshot.clone();
            projected.state = TaskState::Failed;
            projected.last_error = Some("task runtime or token budget exceeded".into());
            projected.completed_at_ms = Some(now);
            projected.updated_at_ms = now;
            let replacements = self.fit_snapshot_checkpoint(&id, &mut projected, false)?;
            if let Some(agent) = self.record(&id)?.snapshot.assigned_agent.clone() {
                cancel_task_agent_if_live(agents, &agent)?;
            }
            self.apply_event_snapshot_replacements(replacements)?;
        }
        Ok(())
    }

    fn propagate_blocked(&mut self) -> DevelopmentResult<()> {
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
                let mut projected = self.record(&id)?.snapshot.clone();
                projected.state = TaskState::Blocked;
                projected.last_error = Some("a prerequisite task did not succeed".into());
                projected.updated_at_ms = now_ms();
                let replacements = self.fit_snapshot_checkpoint(&id, &mut projected, false)?;
                self.apply_event_snapshot_replacements(replacements)?;
            }
        }
        Ok(())
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
        for (label, value) in [
            ("task model", spec.model.as_deref()),
            ("task thinking", spec.thinking.as_deref()),
        ] {
            if let Some(value) = value {
                validate_text(label, value)?;
            }
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

    fn validate_snapshot(&self, snapshot: &TaskSnapshot) -> DevelopmentResult<()> {
        if snapshot
            .id
            .as_str()
            .strip_prefix("task-")
            .and_then(|suffix| suffix.parse::<u64>().ok())
            == Some(0)
        {
            return Err(DevelopmentError::InvalidInput(
                "task checkpoint contains reserved task id".into(),
            ));
        }
        let mut dependencies = BTreeSet::new();
        if snapshot
            .dependencies
            .iter()
            .any(|dependency| dependency == &snapshot.id || !dependencies.insert(dependency))
        {
            return Err(DevelopmentError::InvalidInput(
                "task checkpoint has a duplicate or self dependency".into(),
            ));
        }
        if snapshot.recovery_pending
            && (snapshot.state != TaskState::Paused || snapshot.assigned_agent.is_none())
        {
            return Err(DevelopmentError::InvalidInput(
                "task checkpoint has inconsistent recovery state".into(),
            ));
        }
        if matches!(
            snapshot.state,
            TaskState::Ready | TaskState::Running | TaskState::Waiting | TaskState::Verifying
        ) && snapshot.assigned_agent.is_none()
        {
            return Err(DevelopmentError::InvalidInput(
                "active task checkpoint has no assigned agent".into(),
            ));
        }
        if matches!(snapshot.state, TaskState::Queued | TaskState::Blocked)
            && snapshot.assigned_agent.is_some()
        {
            return Err(DevelopmentError::InvalidInput(
                "unassigned task state has an assigned agent".into(),
            ));
        }
        if snapshot.state == TaskState::Paused
            && !snapshot.recovery_pending
            && snapshot.assigned_agent.is_some()
        {
            return Err(DevelopmentError::InvalidInput(
                "manually paused task has an unexpected agent assignment".into(),
            ));
        }
        if snapshot.verification == VerificationRequirement::Inferred {
            return Err(DevelopmentError::InvalidInput(
                "task checkpoint contains unresolved inferred verification".into(),
            ));
        }
        if let Some(value) = snapshot.last_error.as_deref()
            && (value.len() > MAX_TASK_TEXT_BYTES || value.contains('\0'))
        {
            return Err(DevelopmentError::InvalidInput(
                "task checkpoint error text exceeds bounds".into(),
            ));
        }
        for (label, value) in [
            ("task model", snapshot.model.as_deref()),
            ("task thinking", snapshot.thinking.as_deref()),
        ] {
            if let Some(value) = value
                && (value.len() > MAX_TASK_TEXT_BYTES || value.contains('\0'))
            {
                return Err(DevelopmentError::InvalidInput(format!(
                    "{label} exceeds task checkpoint bounds"
                )));
            }
        }
        for evidence in &snapshot.evidence {
            validate_text("task evidence kind", &evidence.kind)?;
            validate_text("task evidence actor", &evidence.actor)?;
            validate_text("task evidence source", &evidence.source)?;
        }
        let spec = TaskSpec {
            title: snapshot.title.clone(),
            goal: snapshot.goal.clone(),
            prompt: snapshot.prompt.clone(),
            role: snapshot.role.clone(),
            dependencies: snapshot.dependencies.clone(),
            worktree: Some(snapshot.worktree.clone()),
            model: snapshot.model.clone(),
            thinking: snapshot.thinking.clone(),
            budget: snapshot.budget.clone(),
            verification: snapshot.verification.clone(),
            retry: snapshot.retry.clone(),
        };
        self.validate_spec(&spec)
    }

    fn task_for_agent(&self, agent: &AgentId) -> Option<TaskId> {
        self.tasks
            .iter()
            .find(|(_, record)| {
                !record.snapshot.recovery_pending
                    && record.snapshot.assigned_agent.as_ref() == Some(agent)
            })
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

    fn checkpoint_bytes(
        &self,
        replacement: Option<(&TaskId, &TaskSnapshot)>,
        appended: Option<&TaskSnapshot>,
        next_task: u64,
    ) -> DevelopmentResult<Vec<u8>> {
        let replacements = replacement
            .map(|(id, snapshot)| BTreeMap::from([(id.clone(), snapshot.clone())]))
            .unwrap_or_default();
        self.checkpoint_bytes_with_replacements(&replacements, appended, next_task)
    }

    fn checkpoint_bytes_with_replacements(
        &self,
        replacements: &BTreeMap<TaskId, TaskSnapshot>,
        appended: Option<&TaskSnapshot>,
        next_task: u64,
    ) -> DevelopmentResult<Vec<u8>> {
        let mut tasks = self
            .tasks
            .iter()
            .map(|(id, record)| TaskRecordCheckpoint {
                snapshot: replacements
                    .get(id)
                    .cloned()
                    .unwrap_or_else(|| record.snapshot.clone()),
                next_retry_at_ms: record.next_retry_at_ms,
            })
            .collect::<Vec<_>>();
        if let Some(snapshot) = appended {
            tasks.push(TaskRecordCheckpoint {
                snapshot: snapshot.clone(),
                next_retry_at_ms: None,
            });
        }
        serde_json::to_vec_pretty(&TaskSchedulerCheckpoint {
            format_version: TASK_CHECKPOINT_VERSION,
            project_root: self.root.clone(),
            next_task,
            tasks,
        })
        .map_err(Into::into)
    }

    fn ensure_checkpoint_size(&self, encoded_len: usize) -> DevelopmentResult<()> {
        if encoded_len as u64 > MAX_TASK_CHECKPOINT_BYTES {
            Err(DevelopmentError::Conflict(format!(
                "task checkpoint exceeds {} bytes",
                MAX_TASK_CHECKPOINT_BYTES
            )))
        } else {
            Ok(())
        }
    }

    fn ensure_checkpoint_mutation_capacity(&self, encoded_len: usize) -> DevelopmentResult<()> {
        self.ensure_checkpoint_size(encoded_len.saturating_add(TASK_CHECKPOINT_MUTATION_RESERVE))
    }

    fn ensure_current_checkpoint_fits(&self) -> DevelopmentResult<()> {
        let encoded = self.checkpoint_bytes(None, None, self.next_task)?;
        self.ensure_checkpoint_mutation_capacity(encoded.len())
    }

    fn ensure_snapshot_checkpoint_fits(
        &self,
        id: &TaskId,
        snapshot: &TaskSnapshot,
    ) -> DevelopmentResult<()> {
        let encoded = self.checkpoint_bytes(Some((id, snapshot)), None, self.next_task)?;
        self.ensure_checkpoint_mutation_capacity(encoded.len())
    }

    fn fit_event_snapshot_checkpoint(
        &mut self,
        id: &TaskId,
        snapshot: &mut TaskSnapshot,
    ) -> DevelopmentResult<()> {
        let replacements = self.fit_snapshot_checkpoint(id, snapshot, true)?;
        self.apply_event_snapshot_replacements(replacements)
    }

    fn fit_snapshot_checkpoint(
        &self,
        id: &TaskId,
        snapshot: &mut TaskSnapshot,
        has_incoming_evidence: bool,
    ) -> DevelopmentResult<BTreeMap<TaskId, TaskSnapshot>> {
        let mut replacements = BTreeMap::from([(id.clone(), snapshot.clone())]);
        if self.event_checkpoint_replacements_fit(&replacements)? {
            return Ok(replacements);
        }

        while let Some((task_id, evidence_index)) = self.oldest_duplicate_evidence(&replacements) {
            let mut pruned = replacements
                .get(&task_id)
                .cloned()
                .unwrap_or_else(|| self.tasks[&task_id].snapshot.clone());
            pruned.evidence.remove(evidence_index);
            pruned.evidence_truncated = true;
            replacements.insert(task_id.clone(), pruned.clone());
            if task_id == *id {
                *snapshot = pruned;
            }
            if self.event_checkpoint_replacements_fit(&replacements)? {
                return Ok(replacements);
            }
        }

        // Duplicate pruning preserves each task's newest evidence per kind.
        // If that is not enough, compress the just-arrived event, then drop
        // only that event's evidence as a last resort while retaining state.
        if !has_incoming_evidence {
            if let Some(error) = snapshot.last_error.as_deref() {
                let compact = bounded_task_error(error);
                if compact != error {
                    snapshot.last_error = Some(compact);
                    replacements.insert(id.clone(), snapshot.clone());
                    if self.event_checkpoint_replacements_fit(&replacements)? {
                        return Ok(replacements);
                    }
                }
                // The state transition itself is authoritative. If the old
                // checkpoint has no room for even the compact explanation,
                // persist the state and expose the failure without details.
                snapshot.last_error = None;
                replacements.insert(id.clone(), snapshot.clone());
                if self.event_checkpoint_replacements_fit(&replacements)? {
                    return Ok(replacements);
                }
            }
            return Err(DevelopmentError::Conflict(
                "task checkpoint capacity exhausted during state transition".into(),
            ));
        }
        if let Some(event_evidence) = snapshot.evidence.last_mut() {
            event_evidence.details = checkpoint_pressure_details(&event_evidence.details);
            snapshot.evidence_truncated = true;
        }
        replacements.insert(id.clone(), snapshot.clone());
        if self.event_checkpoint_replacements_fit(&replacements)? {
            return Ok(replacements);
        }
        snapshot.evidence.pop();
        snapshot.evidence_truncated = true;
        replacements.insert(id.clone(), snapshot.clone());
        if self.event_checkpoint_replacements_fit(&replacements)? {
            Ok(replacements)
        } else {
            self.ensure_snapshot_checkpoint_fits(id, snapshot)
                .map(|()| replacements)
        }
    }

    fn oldest_duplicate_evidence(
        &self,
        replacements: &BTreeMap<TaskId, TaskSnapshot>,
    ) -> Option<(TaskId, usize)> {
        let mut oldest: Option<(u128, TaskId, usize)> = None;
        for (id, record) in &self.tasks {
            let snapshot = replacements.get(id).unwrap_or(&record.snapshot);
            let mut latest_by_kind = BTreeMap::<&str, (u128, usize)>::new();
            for (index, evidence) in snapshot.evidence.iter().enumerate() {
                let current = (evidence.observed_at_ms, index);
                if latest_by_kind
                    .get(evidence.kind.as_str())
                    .is_none_or(|prior| current > *prior)
                {
                    latest_by_kind.insert(&evidence.kind, current);
                }
            }
            for (index, evidence) in snapshot.evidence.iter().enumerate() {
                if latest_by_kind.get(evidence.kind.as_str())
                    == Some(&(evidence.observed_at_ms, index))
                {
                    continue;
                }
                let candidate = (evidence.observed_at_ms, id.clone(), index);
                if oldest.as_ref().is_none_or(|current| candidate < *current) {
                    oldest = Some(candidate);
                }
            }
        }
        oldest.map(|(_, id, index)| (id, index))
    }

    fn event_checkpoint_replacements_fit(
        &self,
        replacements: &BTreeMap<TaskId, TaskSnapshot>,
    ) -> DevelopmentResult<bool> {
        let encoded =
            self.checkpoint_bytes_with_replacements(replacements, None, self.next_task)?;
        Ok(self
            .ensure_checkpoint_mutation_capacity(encoded.len())
            .is_ok())
    }

    fn apply_event_snapshot_replacements(
        &mut self,
        replacements: BTreeMap<TaskId, TaskSnapshot>,
    ) -> DevelopmentResult<()> {
        for (id, snapshot) in replacements {
            self.record_mut(&id)?.snapshot = snapshot;
        }
        Ok(())
    }

    fn ensure_schedule_checkpoint_fits(
        &self,
        id: &TaskId,
        queued: &TaskSnapshot,
    ) -> DevelopmentResult<()> {
        let mut scheduled = queued.clone();
        scheduled.assigned_agent = Some(maximum_length_agent_id()?);
        scheduled.state = TaskState::Ready;
        scheduled.attempt = scheduled.attempt.saturating_add(1);
        scheduled.updated_at_ms = now_ms();
        self.ensure_snapshot_checkpoint_fits(id, &scheduled)
    }

    fn persist_checkpoint(&self) -> DevelopmentResult<()> {
        let encoded = self.checkpoint_bytes(None, None, self.next_task)?;
        self.ensure_checkpoint_size(encoded.len())?;
        let path = self.root.join(TASK_CHECKPOINT_RELATIVE_PATH);
        validate_task_checkpoint_paths(&self.root)?;
        let parent = path
            .parent()
            .ok_or_else(|| DevelopmentError::InvalidInput("invalid task checkpoint path".into()))?;
        fs::create_dir_all(parent)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(parent, fs::Permissions::from_mode(0o700))?;
        }

        let (temporary_path, mut file) = loop {
            let sequence = NEXT_CHECKPOINT_TEMP.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            let temporary_path =
                parent.join(format!(".scheduler-{}-{sequence}.tmp", std::process::id()));
            let mut options = OpenOptions::new();
            options.write(true).create_new(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt;
                options.mode(0o600);
            }
            match options.open(&temporary_path) {
                Ok(file) => break (temporary_path, file),
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(error) => return Err(error.into()),
            }
        };
        let write_result = (|| -> DevelopmentResult<()> {
            file.write_all(&encoded)?;
            file.sync_all()?;
            drop(file);
            fs::rename(&temporary_path, &path)?;
            #[cfg(unix)]
            OpenOptions::new().read(true).open(parent)?.sync_all()?;
            Ok(())
        })();
        if write_result.is_err() {
            let _ = fs::remove_file(temporary_path);
        }
        write_result
    }

    fn persist_recovery_checkpoint(&mut self, recovered_ids: &[TaskId]) -> DevelopmentResult<()> {
        match self.persist_checkpoint() {
            Ok(()) => return Ok(()),
            Err(DevelopmentError::Conflict(message))
                if message.contains("task checkpoint exceeds") => {}
            Err(error) => return Err(error),
        }

        // Recovery flags carry the unknown-outcome state even when there is
        // not room for a repeated explanatory last_error on every task.
        for id in recovered_ids {
            self.record_mut(id)?.snapshot.last_error = None;
        }
        match self.persist_checkpoint() {
            Ok(()) => return Ok(()),
            Err(DevelopmentError::Conflict(message))
                if message.contains("task checkpoint exceeds") => {}
            Err(error) => return Err(error),
        }

        // If the checkpoint was written by an older build at its full byte
        // limit, reclaim only older duplicate evidence while retaining the
        // newest evidence kind on each task.
        while let Some((id, index)) = self.oldest_duplicate_evidence(&BTreeMap::new()) {
            let snapshot = &mut self.record_mut(&id)?.snapshot;
            snapshot.evidence.remove(index);
            snapshot.evidence_truncated = true;
            match self.persist_checkpoint() {
                Ok(()) => return Ok(()),
                Err(DevelopmentError::Conflict(message))
                    if message.contains("task checkpoint exceeds") => {}
                Err(error) => return Err(error),
            }
        }
        self.persist_checkpoint()
    }
}

fn validate_task_checkpoint_paths(root: &Path) -> DevelopmentResult<()> {
    for path in [root.join(".glass"), root.join(".glass/tasks")] {
        match fs::symlink_metadata(&path) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                return Err(DevelopmentError::InvalidInput(format!(
                    "task checkpoint path may not be a symbolic link: {}",
                    path.display()
                )));
            }
            Ok(metadata) if !metadata.is_dir() => {
                return Err(DevelopmentError::InvalidInput(format!(
                    "task checkpoint parent is not a directory: {}",
                    path.display()
                )));
            }
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
    }
    let checkpoint = root.join(TASK_CHECKPOINT_RELATIVE_PATH);
    match fs::symlink_metadata(&checkpoint) {
        Ok(metadata) if metadata.file_type().is_symlink() => Err(DevelopmentError::InvalidInput(
            "task checkpoint may not be a symbolic link".into(),
        )),
        Ok(metadata) if !metadata.is_file() => Err(DevelopmentError::InvalidInput(
            "task checkpoint is not a regular file".into(),
        )),
        Ok(_) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.into()),
    }
}

fn maximum_length_agent_id() -> DevelopmentResult<AgentId> {
    AgentId::parse(format!("agent-{}", "\\".repeat(58)))
}

fn load_task_checkpoint(
    path: &Path,
    project_root: &Path,
) -> DevelopmentResult<(BTreeMap<TaskId, TaskRecord>, u64)> {
    let metadata = fs::symlink_metadata(path)?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(DevelopmentError::InvalidInput(
            "task checkpoint must be a regular non-symlink file".into(),
        ));
    }
    if metadata.len() > MAX_TASK_CHECKPOINT_BYTES {
        return Err(DevelopmentError::InvalidInput(format!(
            "task checkpoint exceeds {} bytes",
            MAX_TASK_CHECKPOINT_BYTES
        )));
    }
    let bytes = fs::read(path)?;
    let checkpoint: TaskSchedulerCheckpoint = serde_json::from_slice(&bytes).map_err(|error| {
        DevelopmentError::InvalidInput(format!("invalid task checkpoint: {error}"))
    })?;
    if checkpoint.format_version != TASK_CHECKPOINT_VERSION {
        return Err(DevelopmentError::InvalidInput(format!(
            "unsupported task checkpoint version {}",
            checkpoint.format_version
        )));
    }
    if checkpoint.project_root != project_root {
        return Err(DevelopmentError::InvalidInput(
            "task checkpoint belongs to a different project root".into(),
        ));
    }
    if checkpoint.next_task == 0 || checkpoint.tasks.len() > MAX_TASKS {
        return Err(DevelopmentError::InvalidInput(
            "task checkpoint exceeds supported bounds".into(),
        ));
    }
    let mut tasks = BTreeMap::new();
    let mut maximum_id = 0_u64;
    for saved in checkpoint.tasks {
        let snapshot = saved.snapshot;
        let id = TaskId::parse(snapshot.id.as_str().to_owned())?;
        for (label, value) in [
            ("task title", snapshot.title.as_str()),
            ("task goal", snapshot.goal.as_str()),
            ("task prompt", snapshot.prompt.as_str()),
            ("task role", snapshot.role.as_str()),
        ] {
            validate_text(label, value)?;
        }
        if snapshot.dependencies.len() > MAX_DEPENDENCIES
            || snapshot.evidence.len() > MAX_TASK_EVIDENCE
        {
            return Err(DevelopmentError::InvalidInput(
                "task checkpoint record exceeds supported bounds".into(),
            ));
        }
        let numeric_id = id
            .as_str()
            .strip_prefix("task-")
            .unwrap_or_default()
            .parse::<u64>()
            .map_err(|_| DevelopmentError::InvalidInput("invalid task checkpoint id".into()))?;
        maximum_id = maximum_id.max(numeric_id);
        if tasks
            .insert(
                id,
                TaskRecord {
                    snapshot,
                    next_retry_at_ms: saved.next_retry_at_ms,
                },
            )
            .is_some()
        {
            return Err(DevelopmentError::InvalidInput(
                "duplicate task id in checkpoint".into(),
            ));
        }
    }
    if checkpoint.next_task <= maximum_id {
        return Err(DevelopmentError::InvalidInput(
            "task checkpoint next id is not beyond saved task ids".into(),
        ));
    }
    for record in tasks.values() {
        if record
            .snapshot
            .dependencies
            .iter()
            .any(|dependency| !tasks.contains_key(dependency))
        {
            return Err(DevelopmentError::InvalidInput(
                "task checkpoint has a missing dependency".into(),
            ));
        }
        if let Some(agent) = &record.snapshot.assigned_agent {
            AgentId::parse(agent.as_str().to_owned())?;
        }
    }
    Ok((tasks, checkpoint.next_task))
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
        VerificationRequirement::Inferred => Err(DevelopmentError::Conflict(
            "inferred task verification was not resolved at creation".into(),
        )),
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

fn infer_verification(worktree: &Path) -> VerificationRequirement {
    let command = if worktree.join("Cargo.lock").is_file() {
        Some("cargo test --workspace --all-targets --locked")
    } else if worktree.join("Cargo.toml").is_file() {
        Some("cargo test --workspace --all-targets")
    } else if worktree.join("package.json").is_file() {
        Some("npm test")
    } else if worktree.join("pyproject.toml").is_file() || worktree.join("pytest.ini").is_file() {
        Some("python -m pytest")
    } else if worktree.join("go.mod").is_file() {
        Some("go test ./...")
    } else {
        None
    };
    command.map_or(
        VerificationRequirement::GitChange {
            require_changes: true,
            require_clean: false,
        },
        |command| VerificationRequirement::Command {
            command: command.into(),
            expected_exit: 0,
            timeout_seconds: 600,
        },
    )
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
    push_snapshot_evidence(&mut record.snapshot, evidence);
}

fn push_snapshot_evidence(snapshot: &mut TaskSnapshot, evidence: TaskEvidence) {
    if snapshot.evidence.len() == MAX_TASK_EVIDENCE {
        snapshot.evidence.remove(0);
        snapshot.evidence_truncated = true;
    }
    snapshot.evidence.push(evidence);
    snapshot.updated_at_ms = now_ms();
}

fn bounded_event_details(payload: &Value) -> Value {
    const MAX_EVENT_EVIDENCE_BYTES: usize = 4 * 1024;
    let encoded = serde_json::to_vec(payload).unwrap_or_default();
    if encoded.len() <= MAX_EVENT_EVIDENCE_BYTES {
        return payload.clone();
    }
    let mut summary = verifier_scalar_fields(payload);
    summary.insert("truncated".into(), Value::Bool(true));
    summary.insert("originalBytes".into(), Value::from(encoded.len() as u64));
    Value::Object(summary)
}

fn checkpoint_pressure_details(payload: &Value) -> Value {
    let mut summary = verifier_scalar_fields(payload);
    summary.insert("truncated".into(), Value::Bool(true));
    summary.insert("checkpointPressure".into(), Value::Bool(true));
    Value::Object(summary)
}

fn bounded_task_error(value: &str) -> String {
    // Persist a readable, JSON-safe summary rather than allowing provider or
    // process error payloads to consume the aggregate checkpoint budget.
    let mut result = value
        .chars()
        .take(MAX_PERSISTED_TASK_ERROR_BYTES - 3)
        .map(|character| {
            if character.is_ascii_alphanumeric()
                || matches!(character, ' ' | '.' | ',' | ':' | ';' | '_' | '-')
            {
                character
            } else {
                '?'
            }
        })
        .collect::<String>();
    if value.len() > result.len() {
        result.push_str("...");
    }
    if result.is_empty() {
        "agent failed".into()
    } else {
        result
    }
}

fn verifier_scalar_fields(payload: &Value) -> serde_json::Map<String, Value> {
    let mut summary = serde_json::Map::new();
    for field in ["errors", "regressions"] {
        if let Some(value) = payload.get(field).and_then(Value::as_u64) {
            summary.insert(field.to_string(), Value::from(value));
        }
    }
    for field in ["hasChanges", "clean"] {
        if let Some(value) = payload.get(field).and_then(Value::as_bool) {
            summary.insert(field.to_string(), Value::Bool(value));
        }
    }
    summary
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
                session_file: None,
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

    fn settle_only(title: impl Into<String>, prompt: impl Into<String>) -> TaskSpec {
        let mut spec = TaskSpec::new(title, prompt);
        spec.verification = VerificationRequirement::Settled;
        spec
    }

    #[test]
    fn task_checkpoint_restores_identity_evidence_and_requires_explicit_recovery() {
        let (root, mut scheduler, mut agents) = scheduler();
        let task = scheduler
            .create(
                &mut agents,
                settle_only("recover me", "do not replay me automatically"),
            )
            .unwrap();
        let old_agent = scheduler.tasks[&task]
            .snapshot
            .assigned_agent
            .clone()
            .expect("scheduled task retains its assignment");
        scheduler.tasks.get_mut(&task).unwrap().snapshot.state = TaskState::Running;
        scheduler
            .submit_evidence(
                &task,
                "checkpoint-test",
                "operator",
                "task-test",
                true,
                serde_json::json!({"reviewed":true}),
            )
            .unwrap();
        let checkpoint_path = root.join(TASK_CHECKPOINT_RELATIVE_PATH);
        let checkpoint: Value =
            serde_json::from_slice(&fs::read(&checkpoint_path).unwrap()).unwrap();
        assert_eq!(checkpoint["formatVersion"], TASK_CHECKPOINT_VERSION);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                fs::metadata(&checkpoint_path).unwrap().permissions().mode() & 0o777,
                0o600
            );
        }

        drop(scheduler);
        let mut recovered = TaskScheduler::new(&root).unwrap();
        let restored = &recovered.tasks[&task].snapshot;
        assert_eq!(restored.id, task);
        assert_eq!(restored.assigned_agent, Some(old_agent));
        assert_eq!(restored.state, TaskState::Paused);
        assert!(restored.recovery_pending);
        assert!(
            restored
                .last_error
                .as_deref()
                .is_some_and(|message| message.contains("recovery pending"))
        );
        assert!(
            restored
                .evidence
                .iter()
                .any(|item| item.kind == "checkpoint-test")
        );

        let mut restarted_agents = FakeAgents::default();
        recovered.refresh(&mut restarted_agents).unwrap();
        assert!(restarted_agents.prompts.is_empty());
        assert_eq!(recovered.tasks[&task].snapshot.state, TaskState::Paused);

        assert_eq!(recovered.next_task, 2);
        assert!(restarted_agents.prompts.is_empty());

        recovered.resume(&mut restarted_agents, &task).unwrap();
        recovered.refresh(&mut restarted_agents).unwrap();
        assert_eq!(restarted_agents.prompts.len(), 1);
        assert_eq!(
            restarted_agents.prompts[0].1,
            "do not replay me automatically"
        );
        assert!(!recovered.tasks[&task].snapshot.recovery_pending);
        assert!(recovered.tasks[&task].snapshot.assigned_agent.is_some());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn recovery_pending_controls_tolerate_missing_historical_agents() {
        {
            let (root, mut scheduler, mut agents) = scheduler();
            let task = scheduler
                .create(&mut agents, settle_only("reassign me", "recovery prompt"))
                .unwrap();
            let old_agent = scheduler.tasks[&task]
                .snapshot
                .assigned_agent
                .clone()
                .unwrap();
            {
                let snapshot = &mut scheduler.tasks.get_mut(&task).unwrap().snapshot;
                snapshot.state = TaskState::Paused;
                snapshot.recovery_pending = true;
            }
            scheduler.persist_checkpoint().unwrap();
            drop(scheduler);

            let mut recovered = TaskScheduler::new(&root).unwrap();
            recovered
                .reassign(&mut agents, &task, "reviewer".into(), None, None)
                .unwrap();
            assert!(!recovered.tasks[&task].snapshot.recovery_pending);
            assert_eq!(recovered.tasks[&task].snapshot.role, "reviewer");
            assert_eq!(agents.snapshots[&old_agent].status, AgentStatus::Cancelled);
            recovered.refresh(&mut agents).unwrap();
            assert_eq!(agents.prompts.len(), 1);
            assert_eq!(agents.prompts[0].1, "recovery prompt");
            fs::remove_dir_all(root).unwrap();
        }

        let (root, mut scheduler, mut agents) = scheduler();
        let task = scheduler
            .create(&mut agents, settle_only("cancel me", "cancel prompt"))
            .unwrap();
        scheduler.tasks.get_mut(&task).unwrap().snapshot.state = TaskState::Paused;
        scheduler
            .tasks
            .get_mut(&task)
            .unwrap()
            .snapshot
            .recovery_pending = true;
        scheduler.persist_checkpoint().unwrap();
        drop(scheduler);

        let mut recovered = TaskScheduler::new(&root).unwrap();
        let mut restarted_agents = FakeAgents::default();
        recovered.cancel(&mut restarted_agents, &task).unwrap();
        assert_eq!(recovered.tasks[&task].snapshot.state, TaskState::Cancelled);
        assert!(!recovered.tasks[&task].snapshot.recovery_pending);
        assert!(restarted_agents.prompts.is_empty());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn near_capacity_recovery_with_many_active_tasks_drops_repeated_explanations() {
        let (root, mut scheduler, mut agents) = scheduler();
        let mut tasks = Vec::new();
        for index in 0..10 {
            tasks.push(
                scheduler
                    .create(
                        &mut agents,
                        settle_only(format!("active-{index}"), "recovery capacity prompt"),
                    )
                    .unwrap(),
            );
        }
        let filler = &tasks[0];
        for size in [
            128 * 1024,
            64 * 1024,
            32 * 1024,
            16 * 1024,
            8 * 1024,
            4 * 1024,
            2 * 1024,
            1024,
            512,
            256,
            128,
        ] {
            loop {
                if scheduler
                    .submit_evidence(
                        filler,
                        "repeat",
                        "operator",
                        "task-test",
                        true,
                        Value::String("r".repeat(size)),
                    )
                    .is_err()
                {
                    break;
                }
            }
        }
        for id in &tasks {
            scheduler.tasks.get_mut(id).unwrap().snapshot.state = TaskState::Running;
        }
        scheduler.persist_checkpoint().unwrap();
        drop(scheduler);

        let recovered = TaskScheduler::new(&root).unwrap();
        assert_eq!(recovered.tasks.len(), tasks.len());
        assert!(tasks.iter().all(|id| {
            recovered.tasks[id].snapshot.state == TaskState::Paused
                && recovered.tasks[id].snapshot.recovery_pending
        }));
        assert!(recovered.tasks.values().any(|record| {
            record.snapshot.last_error.is_none() && record.snapshot.recovery_pending
        }));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn near_capacity_agent_failure_persists_failed_state_with_bounded_error() {
        let (root, mut scheduler, mut agents) = scheduler();
        let task = scheduler
            .create(&mut agents, settle_only("fail safely", "failure prompt"))
            .unwrap();
        let agent = scheduler.tasks[&task]
            .snapshot
            .assigned_agent
            .clone()
            .unwrap();

        let mut index = 0;
        loop {
            let result = scheduler.submit_evidence(
                &task,
                format!("proof-{index}"),
                "reviewer",
                "task-test",
                true,
                Value::String("e".repeat(16 * 1024)),
            );
            if result.is_err() {
                break;
            }
            index += 1;
        }
        assert!(index > 50);

        let mut failed = agents.snapshots[&agent].clone();
        failed.status = AgentStatus::Failed;
        failed.last_error = Some(format!("engine failure {}", "x".repeat(64 * 1024)));
        scheduler.apply_agent_failures(&[failed]).unwrap();
        scheduler.persist_checkpoint().unwrap();

        let snapshot = &scheduler.tasks[&task].snapshot;
        assert_eq!(snapshot.state, TaskState::Failed);
        let persisted_error = snapshot.last_error.as_deref().unwrap();
        assert!(persisted_error.starts_with("engine failure "));
        assert!(persisted_error.ends_with("..."));
        assert!(persisted_error.len() <= MAX_PERSISTED_TASK_ERROR_BYTES);
        let reopened = TaskScheduler::new(&root).unwrap();
        assert_eq!(reopened.tasks[&task].snapshot.state, TaskState::Failed);
        assert_eq!(
            reopened.tasks[&task].snapshot.last_error.as_deref(),
            Some(persisted_error)
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn task_checkpoint_revalidates_worktree_and_verification_policy() {
        let (root, mut scheduler, mut agents) = scheduler();
        scheduler
            .create(&mut agents, settle_only("validate me", "validation prompt"))
            .unwrap();
        let path = root.join(TASK_CHECKPOINT_RELATIVE_PATH);
        let outside = root.with_extension("outside-worktree");
        fs::create_dir_all(&outside).unwrap();

        let mut checkpoint: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        checkpoint["tasks"][0]["snapshot"]["worktree"] = serde_json::json!(outside);
        fs::write(&path, serde_json::to_vec(&checkpoint).unwrap()).unwrap();
        assert!(TaskScheduler::new(&root).is_err());

        checkpoint["tasks"][0]["snapshot"]["worktree"] = serde_json::json!(root);
        checkpoint["tasks"][0]["snapshot"]["verification"] = serde_json::json!({
            "kind": "command",
            "command": "echo verified",
            "expectedExit": 0,
            "timeoutSeconds": 601
        });
        fs::write(&path, serde_json::to_vec(&checkpoint).unwrap()).unwrap();
        assert!(TaskScheduler::new(&root).is_err());
        fs::remove_dir_all(outside).unwrap();
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn task_checkpoint_rejects_unknown_versions_and_oversized_files() {
        let (root, scheduler, _) = scheduler();
        drop(scheduler);
        let path = root.join(TASK_CHECKPOINT_RELATIVE_PATH);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, r#"{"formatVersion":99,"nextTask":1,"tasks":[]}"#).unwrap();
        assert!(TaskScheduler::new(&root).is_err());

        fs::write(&path, vec![b' '; MAX_TASK_CHECKPOINT_BYTES as usize + 1]).unwrap();
        assert!(TaskScheduler::new(&root).is_err());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn task_checkpoint_is_bound_to_project_and_preserves_the_next_id() {
        let (root, mut scheduler, mut agents) = scheduler();
        let first = scheduler
            .create(&mut agents, settle_only("first", "first prompt"))
            .unwrap();
        assert_eq!(first.as_str(), "task-0001");
        let path = root.join(TASK_CHECKPOINT_RELATIVE_PATH);

        let foreign_root = root.with_extension("foreign-project");
        fs::create_dir_all(foreign_root.join(".glass/tasks")).unwrap();
        fs::copy(&path, foreign_root.join(TASK_CHECKPOINT_RELATIVE_PATH)).unwrap();
        assert!(TaskScheduler::new(&foreign_root).is_err());

        let mut checkpoint: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        checkpoint["nextTask"] = serde_json::json!(1);
        fs::write(&path, serde_json::to_vec(&checkpoint).unwrap()).unwrap();
        assert!(TaskScheduler::new(&root).is_err());

        fs::remove_dir_all(foreign_root).unwrap();
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn checkpoint_capacity_rejects_task_and_evidence_before_mutation_or_dispatch() {
        let (root, mut scheduler, mut agents) = scheduler();
        let large_prompt = "x".repeat(MAX_TASK_TEXT_BYTES);
        let mut first = None;
        for index in 0..15 {
            let task = scheduler
                .create(
                    &mut agents,
                    settle_only(format!("large-{index}"), large_prompt.clone()),
                )
                .unwrap();
            first.get_or_insert(task);
        }
        let before_count = scheduler.tasks.len();
        let before_next = scheduler.next_task;
        let before_agents = agents.next;
        let overflow = scheduler.create(&mut agents, settle_only("overflow", large_prompt.clone()));
        assert!(overflow.is_err());
        assert_eq!(scheduler.tasks.len(), before_count);
        assert_eq!(scheduler.next_task, before_next);
        assert_eq!(agents.next, before_agents);

        let task = first.unwrap();
        let before_evidence = scheduler.tasks[&task].snapshot.evidence.len();
        let evidence = scheduler.submit_evidence(
            &task,
            "largeEvidence",
            "operator",
            "task-test",
            true,
            Value::String("d".repeat(MAX_TASK_TEXT_BYTES)),
        );
        assert!(evidence.is_err());
        assert_eq!(
            scheduler.tasks[&task].snapshot.evidence.len(),
            before_evidence
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn oversized_runtime_event_is_summarized_and_cursor_advances_after_checkpoint() {
        let (root, mut scheduler, mut agents) = scheduler();
        let task = scheduler
            .create(&mut agents, settle_only("event", "event prompt"))
            .unwrap();
        let assigned = scheduler.tasks[&task]
            .snapshot
            .assigned_agent
            .clone()
            .unwrap();
        scheduler.tasks.get_mut(&task).unwrap().snapshot.state = TaskState::Running;
        agents.emit(
            &assigned,
            "message_end",
            serde_json::json!({
                "errors": 1,
                "clean": true,
                "large": "x".repeat(MAX_TASK_CHECKPOINT_BYTES as usize),
            }),
        );

        scheduler.refresh(&mut agents).unwrap();
        assert_eq!(scheduler.agent_event_cursor, 2);
        let evidence = scheduler.tasks[&task]
            .snapshot
            .evidence
            .iter()
            .find(|evidence| evidence.kind == "agent.message_end")
            .unwrap();
        assert_eq!(evidence.details["errors"], 1);
        assert_eq!(evidence.details["clean"], true);
        assert_eq!(evidence.details["truncated"], true);
        assert!(serde_json::to_vec(evidence).unwrap().len() < 1024);
        let mut reopened = TaskScheduler::new(&root).unwrap();
        assert!(reopened.tasks[&task].snapshot.recovery_pending);
        let prompts_before = agents.prompts.len();
        reopened.refresh(&mut agents).unwrap();
        assert_eq!(agents.prompts.len(), prompts_before);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn oversized_event_summary_only_copies_bounded_verifier_scalars() {
        let details = bounded_event_details(&serde_json::json!({
            "errors": "n".repeat(8 * 1024),
            "regressions": 3,
            "hasChanges": {"nested": "x".repeat(8 * 1024)},
            "clean": true,
            "payload": "x".repeat(8 * 1024),
        }));
        assert!(!details.as_object().unwrap().contains_key("errors"));
        assert_eq!(details["regressions"], 3);
        assert!(!details.as_object().unwrap().contains_key("hasChanges"));
        assert_eq!(details["clean"], true);
        assert_eq!(details["truncated"], true);
        assert!(serde_json::to_vec(&details).unwrap().len() < 256);
    }

    #[test]
    fn checkpoint_saturation_drops_old_event_evidence_and_advances_cursor() {
        let (root, mut scheduler, mut agents) = scheduler();
        let task = scheduler
            .create(&mut agents, settle_only("saturate", "saturation prompt"))
            .unwrap();
        let evidence_task = scheduler
            .create(&mut agents, settle_only("other evidence", "other prompt"))
            .unwrap();
        scheduler
            .submit_evidence(
                &evidence_task,
                "required-proof",
                "operator",
                "task-test",
                true,
                serde_json::json!({"clean": true}),
            )
            .unwrap();
        let assigned = scheduler.tasks[&task]
            .snapshot
            .assigned_agent
            .clone()
            .unwrap();
        let mut accepted = 0;
        for size in [
            128 * 1024,
            64 * 1024,
            32 * 1024,
            16 * 1024,
            8 * 1024,
            4 * 1024,
            2 * 1024,
            1024,
            512,
            256,
            128,
            1,
        ] {
            loop {
                let result = scheduler.submit_evidence(
                    &evidence_task,
                    "fill",
                    "operator",
                    "task-test",
                    true,
                    Value::String("f".repeat(size)),
                );
                if result.is_err() {
                    break;
                }
                accepted += 1;
            }
        }
        assert!(accepted >= 2);
        assert!(accepted < MAX_TASK_EVIDENCE);

        scheduler.agent_event_cursor = agents.next_event;
        agents.emit(
            &assigned,
            "message_end",
            Value::String("e".repeat(3 * 1024)),
        );
        let event_sequence = agents.next_event;
        scheduler.refresh(&mut agents).unwrap();
        assert_eq!(scheduler.agent_event_cursor, event_sequence);
        let evidence = &scheduler.tasks[&task].snapshot.evidence;
        assert!(
            evidence
                .iter()
                .any(|evidence| evidence.kind == "agent.message_end")
        );
        let retained = &scheduler.tasks[&evidence_task].snapshot.evidence;
        assert_eq!(
            retained
                .iter()
                .filter(|evidence| evidence.kind == "fill")
                .count(),
            accepted - 1
        );
        assert!(retained.iter().any(|evidence| {
            evidence.kind == "required-proof" && evidence.details["clean"] == true
        }));
        assert!(scheduler.tasks[&evidence_task].snapshot.evidence_truncated);
        assert!(matches!(
            verify(
                &VerificationRequirement::LspDiagnostics { max_errors: 0 },
                &root,
                retained
            )
            .unwrap(),
            VerificationOutcome::Waiting
        ));
        let evidence_count = retained.len();
        scheduler.refresh(&mut agents).unwrap();
        assert_eq!(scheduler.agent_event_cursor, event_sequence);
        assert_eq!(
            scheduler.tasks[&evidence_task].snapshot.evidence.len(),
            evidence_count
        );
        let mut reopened = TaskScheduler::new(&root).unwrap();
        assert!(reopened.tasks[&task].snapshot.recovery_pending);
        let prompts_before = agents.prompts.len();
        reopened.refresh(&mut agents).unwrap();
        assert_eq!(agents.prompts.len(), prompts_before);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn checkpoint_pressure_drops_incoming_event_proof_without_false_verification() {
        let (root, mut scheduler, mut agents) = scheduler();
        let mut receiver = settle_only("proof event", "proof prompt");
        receiver.verification = VerificationRequirement::LspDiagnostics { max_errors: 0 };
        let task = scheduler.create(&mut agents, receiver).unwrap();
        let assigned = scheduler.tasks[&task]
            .snapshot
            .assigned_agent
            .clone()
            .unwrap();
        let filler = scheduler
            .create(&mut agents, settle_only("unique evidence", "filler prompt"))
            .unwrap();
        let mut kind = 0;
        for size in [
            128 * 1024,
            64 * 1024,
            32 * 1024,
            16 * 1024,
            8 * 1024,
            4 * 1024,
            2 * 1024,
            1024,
            512,
            256,
            128,
            1,
        ] {
            loop {
                let result = scheduler.submit_evidence(
                    &filler,
                    format!("unique-{kind}"),
                    "operator",
                    "task-test",
                    true,
                    Value::String("u".repeat(size)),
                );
                if result.is_err() {
                    break;
                }
                kind += 1;
            }
        }
        assert!(kind >= 20);
        assert!(kind < MAX_TASK_EVIDENCE);

        scheduler.agent_event_cursor = agents.next_event;
        agents.emit(
            &assigned,
            "message_end",
            serde_json::json!({
                "usage": {"totalTokens": 7},
                "message": "m".repeat(3 * 1024),
            }),
        );
        let sequence = agents.next_event;
        scheduler.refresh(&mut agents).unwrap();
        assert_eq!(scheduler.agent_event_cursor, sequence);
        let snapshot = &scheduler.tasks[&task].snapshot;
        assert!(snapshot.evidence_truncated);
        assert_eq!(snapshot.observed_tokens, 7);
        assert!(
            !snapshot
                .evidence
                .iter()
                .any(|evidence| evidence.kind == "agent.message_end")
        );
        assert!(matches!(
            verify(
                &VerificationRequirement::LspDiagnostics { max_errors: 0 },
                &root,
                &snapshot.evidence
            )
            .unwrap(),
            VerificationOutcome::Waiting
        ));
        scheduler.refresh(&mut agents).unwrap();
        assert_eq!(scheduler.agent_event_cursor, sequence);
        fs::remove_dir_all(root).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn task_checkpoint_rejects_symlinked_state_paths() {
        use std::os::unix::fs::symlink;

        let (root, scheduler, _) = scheduler();
        drop(scheduler);
        let external = root.with_extension("external-state");
        fs::create_dir_all(&external).unwrap();
        symlink(&external, root.join(".glass")).unwrap();
        assert!(TaskScheduler::new(&root).is_err());
        fs::remove_file(root.join(".glass")).unwrap();
        fs::remove_dir_all(&external).unwrap();
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn tasks_dispatch_prompts_verify_and_wake_dag_dependents() {
        let (root, mut scheduler, mut agents) = scheduler();
        let first = scheduler
            .create(&mut agents, settle_only("investigate", "inspect failure"))
            .unwrap();
        settle(&mut scheduler, &mut agents);
        assert_eq!(
            scheduler.snapshot(&mut agents, &first).unwrap().state,
            TaskState::Succeeded
        );
        assert_eq!(agents.prompts[0].1, "inspect failure");

        let mut dependent = settle_only("repair", "apply repair");
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
                        settle_only(format!("worker-{index}"), format!("inspect shard {index}")),
                    )
                    .unwrap(),
            );
        }
        assert_eq!(scheduler.tasks.len(), 8);

        let mut integration = settle_only("integration", "verify all shards");
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

    #[test]
    fn implementation_tasks_infer_project_verification_and_settle_only_is_explicit() {
        let (root, mut scheduler, mut agents) = scheduler();
        std::fs::write(
            root.join("Cargo.toml"),
            "[package]\nname='verification-fixture'\nversion='0.1.0'\n",
        )
        .unwrap();
        let inferred = scheduler
            .create(
                &mut agents,
                TaskSpec::new("implement repair", "change the implementation"),
            )
            .unwrap();
        assert!(matches!(
            scheduler.tasks[&inferred].snapshot.verification,
            VerificationRequirement::Command { ref command, .. }
                if command == "cargo test --workspace --all-targets"
        ));

        let research = scheduler
            .create(
                &mut agents,
                settle_only("research alternatives", "compare approaches"),
            )
            .unwrap();
        assert_eq!(
            scheduler.tasks[&research].snapshot.verification,
            VerificationRequirement::Settled
        );
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn factory_crew_queues_architect_implementer_tester_reviewer_and_browser() {
        let (root, mut scheduler, mut agents) = scheduler();
        let tree_a = root.join("impl-a");
        let tree_b = root.join("impl-b");
        std::fs::create_dir_all(&tree_a).unwrap();
        std::fs::create_dir_all(&tree_b).unwrap();
        let ids = scheduler
            .create_crew(
                &mut agents,
                "add settings toggle",
                Some(root.clone()),
                vec![tree_a.clone(), tree_b.clone()],
            )
            .unwrap();
        assert_eq!(ids.len(), 7);
        let snapshots = ids
            .iter()
            .map(|id| scheduler.snapshot(&mut agents, id).unwrap())
            .collect::<Vec<_>>();
        let roles = snapshots
            .iter()
            .map(|snapshot| snapshot.role.as_str())
            .collect::<Vec<_>>();
        assert_eq!(
            roles,
            [
                "architect",
                "implementer",
                "implementer",
                "tester",
                "tester",
                "reviewer",
                "browser"
            ]
        );
        assert_eq!(snapshots[1].worktree, tree_a);
        assert_eq!(snapshots[2].worktree, tree_b);
        assert_eq!(snapshots[3].worktree, tree_a);
        assert_eq!(snapshots[4].worktree, tree_b);
        assert!(snapshots[1].dependencies.contains(&ids[0]));
        assert!(snapshots[2].dependencies.contains(&ids[0]));
        assert!(snapshots[3].dependencies.contains(&ids[1]));
        assert!(snapshots[4].dependencies.contains(&ids[2]));
        assert!(snapshots[5].dependencies.contains(&ids[1]));
        assert!(snapshots[5].dependencies.contains(&ids[2]));
        assert!(snapshots[6].dependencies.contains(&ids[3]));
        assert!(snapshots[6].dependencies.contains(&ids[4]));
        assert!(snapshots[1].prompt.contains("isolated worktree"));
        assert!(snapshots[1].prompt.contains("1 of 2"));
        assert!(snapshots[2].prompt.contains("2 of 2"));
        assert!(snapshots[0].prompt.contains("Do not implement"));
        assert!(snapshots[5].prompt.contains("cannot write"));
        assert!(matches!(
            snapshots[6].verification,
            VerificationRequirement::BrowserWorkflow { .. }
        ));
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn crew_wake_persists_and_reloads_from_glass_crew() {
        let root = std::env::temp_dir().join(format!(
            "glass-crew-wake-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let wake = CrewWake {
            id: "add-settings-toggle".into(),
            goal: "add settings toggle".into(),
            worktree: Some(
                root.join(".glass/worktrees/add-settings-toggle")
                    .display()
                    .to_string(),
            ),
            checkpoint: "before-crew:add settings toggle".into(),
            created_at_ms: 1,
            tasks: vec![CrewWakeMember {
                id: "task-0001".into(),
                role: "architect".into(),
                title: "architect: add settings toggle".into(),
                state: "queued".into(),
                worktree: None,
            }],
            verify: "PROOF ✓\n  url /settings".into(),
            tests: "cargo passed · 12 ms · exit 0".into(),
            page: "url http://localhost:3000".into(),
            diff: "diff --git a/src/lib.rs b/src/lib.rs".into(),
            accept: "proposal-1".into(),
        };
        persist_crew_wake(&root, &wake).unwrap();
        let loaded = load_latest_crew_wake(&root).expect("latest wake");
        assert_eq!(loaded, wake);
        let rendered = loaded.render();
        assert!(rendered.contains("WAKE add-settings-toggle"));
        assert!(rendered.contains("VERIFY"));
        assert!(rendered.contains("TESTS"));
        assert!(rendered.contains("PAGE"));
        assert!(rendered.contains("DIFF"));
        assert!(rendered.contains("accept proposal-1"));
        let legacy = serde_json::from_str::<CrewWake>(
            r#"{"id":"legacy","goal":"g","checkpoint":"c","createdAtMs":1,"tasks":[]}"#,
        )
        .unwrap();
        assert!(legacy.diff.is_empty());
        assert_eq!(legacy.accept, "");
        std::fs::remove_dir_all(root).unwrap();
    }
}
