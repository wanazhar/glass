//! Isolated competing implementations ranked from observable evidence.

use crate::agents::{AgentId, AgentRegistry, AgentSpec};
use crate::development::{DevelopmentError, DevelopmentResult, ProcessHealth};
use crate::git::{GitError, GitService};
use crate::testing::TestRun;
use crate::{
    DevelopmentWorkspace, LocalTrustDecision, WorkspaceIdentity, WorkspaceTrust,
    WorkspaceTrustStore,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const MAX_EXPERIMENTS: usize = 8;
const MAX_NOTES_BYTES: usize = 16 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ExperimentState {
    Ready,
    Running,
    Completed,
    Failed,
    Selected,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ExperimentTrustPolicy {
    /// Child worktrees start untrusted and cannot execute project code.
    Reevaluate,
    /// A trusted parent grants process-lifetime trust to child worktrees only.
    InheritOnce,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExperimentEvidence {
    pub tests_passed: u64,
    pub tests_failed: u64,
    pub workflow_passed: Option<bool>,
    pub semantic_regressions: u64,
    pub visual_difference: Option<f64>,
    pub lcp_ms: Option<f64>,
    pub diagnostics: u64,
    pub debugger_stops: u64,
    pub changed_files: u64,
    pub build_passed: Option<bool>,
    pub startup_healthy: Option<bool>,
    pub process_crashes: u64,
    pub notes: String,
    #[serde(default)]
    pub provenance: BTreeMap<String, EvidenceProvenance>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct EvidenceProvenance {
    pub producer: String,
    pub timestamp_ms: u128,
    pub workspace_revision: u64,
    pub browser_revision: Option<u64>,
    pub run_id: Option<String>,
    pub measured: bool,
    pub available: bool,
    pub details: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExperimentSnapshot {
    pub id: String,
    pub branch: String,
    pub worktree: PathBuf,
    pub port: Option<u16>,
    pub state: ExperimentState,
    pub agent_id: Option<AgentId>,
    pub evidence: ExperimentEvidence,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExperimentRanking {
    pub id: String,
    pub score: i64,
    pub reasons: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExperimentComparison {
    pub rankings: Vec<ExperimentRanking>,
    pub recommended: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ExperimentWeights {
    pub test_passed: i64,
    pub test_failed: i64,
    pub workflow_passed: i64,
    pub workflow_failed: i64,
    pub semantic_regression: i64,
    pub diagnostic: i64,
    pub changed_file: i64,
    pub process_crash: i64,
}

impl Default for ExperimentWeights {
    fn default() -> Self {
        Self {
            test_passed: 10,
            test_failed: -1_000,
            workflow_passed: 5_000,
            workflow_failed: -20_000,
            semantic_regression: -2_000,
            diagnostic: -200,
            changed_file: -5,
            process_crash: -5_000,
        }
    }
}

struct Experiment {
    snapshot: ExperimentSnapshot,
    workspace: DevelopmentWorkspace,
}

/// Owns worktrees and resident runtime state for competing implementations.
pub struct ExperimentManager {
    repository: GitService,
    worktree_root: PathBuf,
    experiments: BTreeMap<String, Experiment>,
    ports: BTreeSet<u16>,
    trust_policy: ExperimentTrustPolicy,
    weights: ExperimentWeights,
}

impl ExperimentManager {
    pub fn new(
        repository_root: impl AsRef<Path>,
        worktree_root: impl AsRef<Path>,
    ) -> DevelopmentResult<Self> {
        let identity = WorkspaceIdentity::inspect(repository_root.as_ref())?;
        let trust = WorkspaceTrustStore::platform_default()?.status(&identity)?;
        Self::new_governed(repository_root, worktree_root, trust)
    }

    pub fn new_governed(
        repository_root: impl AsRef<Path>,
        worktree_root: impl AsRef<Path>,
        parent_trust: WorkspaceTrust,
    ) -> DevelopmentResult<Self> {
        if !parent_trust.permits_project_execution() {
            return Err(DevelopmentError::Conflict(
                "experiments cannot create executable worktrees before workspace trust".into(),
            ));
        }
        let repository = GitService::open(repository_root)
            .map_err(|error| DevelopmentError::Process(error.to_string()))?;
        let worktree_root = absolutize(worktree_root.as_ref())?;
        if worktree_root == Path::new("/") || worktree_root == repository.root() {
            return Err(DevelopmentError::InvalidInput(
                "experiment worktree root must be an explicit directory outside the repository"
                    .into(),
            ));
        }
        std::fs::create_dir_all(&worktree_root)?;
        Ok(Self {
            repository,
            worktree_root,
            experiments: BTreeMap::new(),
            ports: BTreeSet::new(),
            trust_policy: ExperimentTrustPolicy::InheritOnce,
            weights: ExperimentWeights::default(),
        })
    }

    pub fn create(
        &mut self,
        id: &str,
        branch: &str,
        port: Option<u16>,
    ) -> DevelopmentResult<ExperimentSnapshot> {
        validate_name(id, "experiment")?;
        validate_name(branch, "experiment branch")?;
        if self.experiments.contains_key(id) {
            return Err(DevelopmentError::Conflict(format!(
                "experiment {id} already exists"
            )));
        }
        if self.experiments.len() >= MAX_EXPERIMENTS {
            return Err(DevelopmentError::Conflict(format!(
                "experiment limit is {MAX_EXPERIMENTS}"
            )));
        }
        if let Some(port) = port
            && (port == 0 || !self.ports.insert(port))
        {
            return Err(DevelopmentError::Conflict(
                "experiment ports must be non-zero and unique".into(),
            ));
        }
        let worktree = self.worktree_root.join(id);
        if worktree.exists() {
            if let Some(port) = port {
                self.ports.remove(&port);
            }
            return Err(DevelopmentError::Conflict(format!(
                "experiment path already exists: {}",
                worktree.display()
            )));
        }
        if let Err(error) = self.repository.create_worktree(&worktree, branch, true) {
            if let Some(port) = port {
                self.ports.remove(&port);
            }
            return Err(DevelopmentError::Process(error.to_string()));
        }
        let workspace = match DevelopmentWorkspace::open(&worktree) {
            Ok(mut workspace) => {
                if self.trust_policy == ExperimentTrustPolicy::InheritOnce
                    && let Err(error) =
                        workspace.apply_local_trust_decision(LocalTrustDecision::TrustOnce)
                {
                    drop(workspace);
                    let _ = self.repository.remove_worktree(&worktree, true);
                    if let Some(port) = port {
                        self.ports.remove(&port);
                    }
                    return Err(error);
                }
                workspace
            }
            Err(error) => {
                let _ = self.repository.remove_worktree(&worktree, true);
                if let Some(port) = port {
                    self.ports.remove(&port);
                }
                return Err(error);
            }
        };
        let snapshot = ExperimentSnapshot {
            id: id.into(),
            branch: branch.into(),
            worktree,
            port,
            state: ExperimentState::Ready,
            agent_id: None,
            evidence: ExperimentEvidence::default(),
        };
        self.experiments.insert(
            id.into(),
            Experiment {
                snapshot: snapshot.clone(),
                workspace,
            },
        );
        Ok(snapshot)
    }

    pub fn assign_agent(
        &mut self,
        id: &str,
        agents: &mut AgentRegistry,
        mut spec: AgentSpec,
    ) -> DevelopmentResult<AgentId> {
        let experiment = self.experiment_mut(id)?;
        if experiment.snapshot.agent_id.is_some() {
            return Err(DevelopmentError::Conflict(format!(
                "experiment {id} already has an agent"
            )));
        }
        spec.worktree = Some(experiment.snapshot.worktree.clone());
        let agent = agents.create(spec)?;
        experiment.snapshot.agent_id = Some(agent.clone());
        experiment.snapshot.state = ExperimentState::Running;
        Ok(agent)
    }

    pub fn start_process(
        &mut self,
        id: &str,
        name: &str,
        command: &str,
    ) -> DevelopmentResult<serde_json::Value> {
        let experiment = self.experiment_mut(id)?;
        experiment.snapshot.state = ExperimentState::Running;
        Ok(serde_json::to_value(
            experiment
                .workspace
                .project_mut()
                .start_process(name, command)?,
        )?)
    }

    pub fn run_test(
        &mut self,
        id: &str,
        run_id: &str,
        suite_id: &str,
        actor_id: &str,
        timeout: Option<Duration>,
    ) -> DevelopmentResult<TestRun> {
        let experiment = self.experiment_mut(id)?;
        let revision = experiment.workspace.project().revision();
        experiment.snapshot.state = ExperimentState::Running;
        experiment
            .workspace
            .tests_mut()
            .start(run_id, suite_id, actor_id, revision, timeout)
            .map_err(|error| DevelopmentError::Process(error.to_string()))
    }

    pub fn poll_tests(&mut self, id: &str) -> DevelopmentResult<Vec<TestRun>> {
        let experiment = self.experiment_mut(id)?;
        let finished = experiment
            .workspace
            .tests_mut()
            .poll()
            .map_err(|error| DevelopmentError::Process(error.to_string()))?;
        for run in &finished {
            let failed = run.exit_code != Some(0);
            let count = u64::try_from(run.cases.len().max(1)).unwrap_or(u64::MAX);
            if failed {
                experiment.snapshot.evidence.tests_failed = experiment
                    .snapshot
                    .evidence
                    .tests_failed
                    .saturating_add(count);
            } else {
                experiment.snapshot.evidence.tests_passed = experiment
                    .snapshot
                    .evidence
                    .tests_passed
                    .saturating_add(count);
            }
        }
        Ok(finished)
    }

    /// Collect every currently available evidence family from resident or
    /// bounded project services. Missing providers are recorded as unavailable
    /// rather than silently converted into favorable zeroes.
    pub fn collect_automatic(&mut self, id: &str) -> DevelopmentResult<ExperimentEvidence> {
        let experiment = self.experiment_mut(id)?;
        experiment.snapshot.state = ExperimentState::Running;
        let revision = experiment.workspace.project().revision();
        let browser_state = experiment.workspace.browser().state().ok();
        let browser_revision = browser_state
            .as_ref()
            .and_then(|state| state.get("browserRevision"))
            .and_then(Value::as_u64);
        let mut evidence = ExperimentEvidence::default();

        let detection = experiment.workspace.project().detection().clone();
        if let Some(command) = detection.build_command.as_deref() {
            let measurement = run_measured_command(&experiment.snapshot.worktree, command, 600)?;
            evidence.build_passed = Some(measurement.passed);
            evidence.provenance.insert(
                "buildPassed".into(),
                measured_provenance(
                    "project-build",
                    revision,
                    browser_revision,
                    Some(measurement.run_id),
                    serde_json::json!({
                        "command":command,
                        "exitCode":measurement.exit_code,
                        "durationMs":measurement.duration_ms
                    }),
                ),
            );
        } else {
            evidence.provenance.insert(
                "buildPassed".into(),
                unavailable_provenance("project-build", revision, browser_revision),
            );
        }

        if let Some(command) = detection.test_command.as_deref() {
            let measurement = run_measured_command(&experiment.snapshot.worktree, command, 600)?;
            if measurement.passed {
                evidence.tests_passed = 1;
            } else {
                evidence.tests_failed = 1;
            }
            evidence.provenance.insert(
                "tests".into(),
                measured_provenance(
                    "project-test",
                    revision,
                    browser_revision,
                    Some(measurement.run_id),
                    serde_json::json!({
                        "command":command,
                        "exitCode":measurement.exit_code,
                        "durationMs":measurement.duration_ms,
                        "aggregateCommandResult":true
                    }),
                ),
            );
        } else {
            evidence.provenance.insert(
                "tests".into(),
                unavailable_provenance("project-test", revision, browser_revision),
            );
        }

        if let Some(git) = experiment.workspace.git() {
            let status = git
                .status()
                .map_err(|error| DevelopmentError::Process(error.to_string()))?;
            evidence.changed_files = u64::try_from(status.entries.len()).unwrap_or(u64::MAX);
            evidence.provenance.insert(
                "changedFiles".into(),
                measured_provenance(
                    "resident-git",
                    revision,
                    browser_revision,
                    None,
                    serde_json::json!({
                        "entries":status.entries,
                        "conflicts":status.conflicts
                    }),
                ),
            );
        }

        let processes = experiment
            .workspace
            .project_mut()
            .processes()
            .list_checked()?;
        evidence.process_crashes = processes
            .iter()
            .filter(|process| process.health == ProcessHealth::Failed)
            .count() as u64;
        evidence.startup_healthy = (!processes.is_empty()).then(|| {
            evidence.process_crashes == 0
                && processes.iter().all(|process| {
                    matches!(
                        process.health,
                        ProcessHealth::Healthy | ProcessHealth::Starting
                    )
                })
        });
        evidence.provenance.insert(
            "startupHealth".into(),
            EvidenceProvenance {
                producer: "resident-process-service".into(),
                timestamp_ms: now_ms(),
                workspace_revision: revision,
                browser_revision,
                run_id: None,
                measured: true,
                available: !processes.is_empty(),
                details: serde_json::to_value(&processes)?,
            },
        );

        let diagnostics = experiment
            .workspace
            .language()
            .events(0)
            .into_iter()
            .filter(|event| event.operation == "diagnostics")
            .filter_map(|event| event.result_count)
            .sum::<usize>();
        evidence.diagnostics = diagnostics as u64;
        evidence.provenance.insert(
            "diagnostics".into(),
            EvidenceProvenance {
                producer: "resident-lsp".into(),
                timestamp_ms: now_ms(),
                workspace_revision: revision,
                browser_revision,
                run_id: None,
                measured: true,
                available: experiment.workspace.language().names().next().is_some(),
                details: serde_json::json!({"observedDiagnostics":diagnostics}),
            },
        );

        let debugger_names = experiment
            .workspace
            .debugger_names()
            .map(str::to_string)
            .collect::<Vec<_>>();
        let mut debugger_stops = 0_u64;
        for debugger in &debugger_names {
            debugger_stops = debugger_stops.saturating_add(
                experiment
                    .workspace
                    .debugger_mut(debugger)
                    .map_err(|error| DevelopmentError::Process(error.to_string()))?
                    .poll_events()
                    .map_err(|error| DevelopmentError::Process(error.to_string()))?
                    .iter()
                    .filter(|event| event.event == "stopped")
                    .count() as u64,
            );
        }
        evidence.debugger_stops = debugger_stops;
        evidence.provenance.insert(
            "debuggerStops".into(),
            EvidenceProvenance {
                producer: "resident-dap".into(),
                timestamp_ms: now_ms(),
                workspace_revision: revision,
                browser_revision,
                run_id: None,
                measured: true,
                available: !debugger_names.is_empty(),
                details: serde_json::json!({"stops":debugger_stops,"sessions":debugger_names}),
            },
        );

        if browser_state
            .as_ref()
            .and_then(|state| state.get("connected"))
            .and_then(Value::as_bool)
            == Some(true)
        {
            match experiment.workspace.browser().verify_workflow() {
                Ok(result) => {
                    evidence.workflow_passed = result.get("verified").and_then(Value::as_bool);
                    evidence.provenance.insert(
                        "workflowPassed".into(),
                        measured_provenance(
                            "resident-browser-workflow",
                            revision,
                            browser_revision,
                            result
                                .pointer("/result/runId")
                                .and_then(Value::as_str)
                                .map(str::to_string),
                            result,
                        ),
                    );
                }
                Err(error) => {
                    evidence.provenance.insert(
                        "workflowPassed".into(),
                        EvidenceProvenance {
                            details: serde_json::json!({"error":error.to_string()}),
                            ..unavailable_provenance(
                                "resident-browser-workflow",
                                revision,
                                browser_revision,
                            )
                        },
                    );
                }
            }
            match experiment.workspace.browser().diff() {
                Ok(result) => {
                    evidence.semantic_regressions = result
                        .get("changes")
                        .and_then(Value::as_array)
                        .map_or(0, |changes| changes.len() as u64);
                    evidence.provenance.insert(
                        "semanticRegressions".into(),
                        measured_provenance(
                            "resident-browser-semantic-diff",
                            revision,
                            browser_revision,
                            None,
                            result,
                        ),
                    );
                }
                Err(error) => {
                    evidence.provenance.insert(
                        "semanticRegressions".into(),
                        EvidenceProvenance {
                            details: serde_json::json!({"error":error.to_string()}),
                            ..unavailable_provenance(
                                "resident-browser-semantic-diff",
                                revision,
                                browser_revision,
                            )
                        },
                    );
                }
            }
        } else {
            for (metric, producer) in [
                ("workflowPassed", "resident-browser-workflow"),
                ("semanticRegressions", "resident-browser-semantic-diff"),
                ("visualDifference", "resident-browser-visual-compare"),
                ("lcpMs", "resident-browser-performance"),
            ] {
                evidence.provenance.insert(
                    metric.into(),
                    unavailable_provenance(producer, revision, browser_revision),
                );
            }
        }
        experiment.snapshot.evidence = evidence.clone();
        experiment.snapshot.state = ExperimentState::Completed;
        Ok(evidence)
    }

    pub fn record_evidence(
        &mut self,
        id: &str,
        mut evidence: ExperimentEvidence,
    ) -> DevelopmentResult<()> {
        if evidence.notes.len() > MAX_NOTES_BYTES || evidence.notes.contains('\0') {
            return Err(DevelopmentError::InvalidInput(
                "experiment notes exceed their bound or contain NUL".into(),
            ));
        }
        if evidence
            .visual_difference
            .is_some_and(|value| !value.is_finite() || value < 0.0)
            || evidence
                .lcp_ms
                .is_some_and(|value| !value.is_finite() || value < 0.0)
        {
            return Err(DevelopmentError::InvalidInput(
                "experiment metrics must be finite and non-negative".into(),
            ));
        }
        let experiment = self.experiment_mut(id)?;
        let revision = experiment.workspace.project().revision();
        let browser_revision = experiment
            .workspace
            .browser()
            .state()
            .ok()
            .and_then(|state| state.get("browserRevision").and_then(Value::as_u64));
        for metric in [
            "tests",
            "workflowPassed",
            "semanticRegressions",
            "visualDifference",
            "lcpMs",
            "diagnostics",
            "debuggerStops",
            "changedFiles",
            "buildPassed",
            "startupHealth",
            "processCrashes",
        ] {
            evidence.provenance.insert(
                metric.into(),
                EvidenceProvenance {
                    producer: "manual-external".into(),
                    timestamp_ms: now_ms(),
                    workspace_revision: revision,
                    browser_revision,
                    run_id: None,
                    measured: false,
                    available: true,
                    details: Value::Null,
                },
            );
        }
        experiment.snapshot.evidence = evidence;
        experiment.snapshot.state = ExperimentState::Completed;
        Ok(())
    }

    pub fn refresh_changed_files(&mut self, id: &str) -> DevelopmentResult<u64> {
        let experiment = self.experiment_mut(id)?;
        let status = experiment
            .workspace
            .git()
            .ok_or_else(|| DevelopmentError::NotFound("experiment Git repository".into()))?
            .status()
            .map_err(|error| DevelopmentError::Process(error.to_string()))?;
        let changed = u64::try_from(status.entries.len()).unwrap_or(u64::MAX);
        experiment.snapshot.evidence.changed_files = changed;
        Ok(changed)
    }

    pub fn compare(&self) -> ExperimentComparison {
        let mut rankings = self
            .experiments
            .values()
            .map(|experiment| rank(&experiment.snapshot, &self.weights))
            .collect::<Vec<_>>();
        rankings.sort_by(|left, right| {
            right
                .score
                .cmp(&left.score)
                .then_with(|| left.id.cmp(&right.id))
        });
        let recommended = rankings
            .first()
            .filter(|ranking| {
                self.experiments.get(&ranking.id).is_some_and(|experiment| {
                    experiment.snapshot.state == ExperimentState::Completed
                })
            })
            .map(|ranking| ranking.id.clone());
        ExperimentComparison {
            rankings,
            recommended,
        }
    }

    pub fn set_weights(
        &mut self,
        weights: ExperimentWeights,
        trust: WorkspaceTrust,
    ) -> DevelopmentResult<()> {
        if !trust.permits_project_execution() {
            return Err(DevelopmentError::Conflict(
                "experiment ranking weights require a trusted workspace".into(),
            ));
        }
        if [
            weights.test_passed,
            weights.test_failed,
            weights.workflow_passed,
            weights.workflow_failed,
            weights.semantic_regression,
            weights.diagnostic,
            weights.changed_file,
            weights.process_crash,
        ]
        .iter()
        .any(|weight| weight.unsigned_abs() > 1_000_000)
        {
            return Err(DevelopmentError::InvalidInput(
                "experiment weight magnitude exceeds 1000000".into(),
            ));
        }
        self.weights = weights;
        Ok(())
    }

    pub fn select(&mut self, id: &str) -> DevelopmentResult<ExperimentSnapshot> {
        let comparison = self.compare();
        if comparison.recommended.as_deref() != Some(id) {
            return Err(DevelopmentError::Conflict(
                "selected experiment is not the current evidence-derived recommendation".into(),
            ));
        }
        for experiment in self.experiments.values_mut() {
            if experiment.snapshot.state == ExperimentState::Selected {
                experiment.snapshot.state = ExperimentState::Completed;
            }
        }
        let experiment = self.experiment_mut(id)?;
        experiment.snapshot.state = ExperimentState::Selected;
        Ok(experiment.snapshot.clone())
    }

    pub fn snapshots(&self) -> Vec<ExperimentSnapshot> {
        self.experiments
            .values()
            .map(|experiment| experiment.snapshot.clone())
            .collect()
    }

    pub fn remove(&mut self, id: &str, force: bool) -> DevelopmentResult<()> {
        let experiment = self
            .experiments
            .remove(id)
            .ok_or_else(|| DevelopmentError::NotFound(format!("experiment {id}")))?;
        if let Some(port) = experiment.snapshot.port {
            self.ports.remove(&port);
        }
        drop(experiment.workspace);
        self.repository
            .remove_worktree(&experiment.snapshot.worktree, force)
            .map_err(|error| DevelopmentError::Process(error.to_string()))
    }

    fn experiment_mut(&mut self, id: &str) -> DevelopmentResult<&mut Experiment> {
        self.experiments
            .get_mut(id)
            .ok_or_else(|| DevelopmentError::NotFound(format!("experiment {id}")))
    }
}

struct CommandMeasurement {
    passed: bool,
    exit_code: i32,
    duration_ms: u128,
    run_id: String,
}

fn run_measured_command(
    worktree: &Path,
    command: &str,
    timeout_seconds: u64,
) -> DevelopmentResult<CommandMeasurement> {
    if command.is_empty() || command.len() > 16 * 1024 || command.contains('\0') {
        return Err(DevelopmentError::InvalidInput(
            "experiment command is empty, oversized, or contains NUL".into(),
        ));
    }
    let started = Instant::now();
    let mut process = if cfg!(windows) {
        let mut process = Command::new("cmd.exe");
        process.args(["/d", "/s", "/c", command]);
        process
    } else {
        let mut process = Command::new("sh");
        process.args(["-lc", command]);
        process
    };
    let mut child = process
        .current_dir(worktree)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;
    let deadline = Instant::now() + Duration::from_secs(timeout_seconds);
    let exit_code = loop {
        if let Some(status) = child.try_wait()? {
            break status.code().unwrap_or(-1);
        }
        if Instant::now() >= deadline {
            child.kill()?;
            child.wait()?;
            break -1;
        }
        thread::sleep(Duration::from_millis(10));
    };
    let timestamp = now_ms();
    Ok(CommandMeasurement {
        passed: exit_code == 0,
        exit_code,
        duration_ms: started.elapsed().as_millis(),
        run_id: format!("experiment-command-{timestamp}"),
    })
}

fn measured_provenance(
    producer: &str,
    workspace_revision: u64,
    browser_revision: Option<u64>,
    run_id: Option<String>,
    details: Value,
) -> EvidenceProvenance {
    EvidenceProvenance {
        producer: producer.into(),
        timestamp_ms: now_ms(),
        workspace_revision,
        browser_revision,
        run_id,
        measured: true,
        available: true,
        details,
    }
}

fn unavailable_provenance(
    producer: &str,
    workspace_revision: u64,
    browser_revision: Option<u64>,
) -> EvidenceProvenance {
    EvidenceProvenance {
        producer: producer.into(),
        timestamp_ms: now_ms(),
        workspace_revision,
        browser_revision,
        run_id: None,
        measured: true,
        available: false,
        details: Value::Null,
    }
}

fn now_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
}

fn rank(snapshot: &ExperimentSnapshot, weights: &ExperimentWeights) -> ExperimentRanking {
    let evidence = &snapshot.evidence;
    let mut score = 0_i64;
    let mut reasons = Vec::new();
    score = score.saturating_add(
        (evidence.tests_passed.min(10_000) as i64).saturating_mul(weights.test_passed),
    );
    score = score.saturating_add(
        (evidence.tests_failed.min(10_000) as i64).saturating_mul(weights.test_failed),
    );
    match evidence.workflow_passed {
        Some(true) => {
            score = score.saturating_add(weights.workflow_passed);
            reasons.push("workflow passed".into());
        }
        Some(false) => {
            score = score.saturating_add(weights.workflow_failed);
            reasons.push("workflow failed".into());
        }
        None => reasons.push("workflow not measured".into()),
    }
    score = score.saturating_add(
        (evidence.semantic_regressions.min(10_000) as i64)
            .saturating_mul(weights.semantic_regression),
    );
    score = score.saturating_add(
        (evidence.diagnostics.min(10_000) as i64).saturating_mul(weights.diagnostic),
    );
    score = score.saturating_add(
        (evidence.changed_files.min(10_000) as i64).saturating_mul(weights.changed_file),
    );
    score = score.saturating_add(
        (evidence.process_crashes.min(10_000) as i64).saturating_mul(weights.process_crash),
    );
    match evidence.build_passed {
        Some(true) => score = score.saturating_add(2_000),
        Some(false) => score = score.saturating_sub(10_000),
        None => {}
    }
    match evidence.startup_healthy {
        Some(true) => score = score.saturating_add(1_000),
        Some(false) => score = score.saturating_sub(5_000),
        None => {}
    }
    if let Some(difference) = evidence.visual_difference {
        score = score.saturating_sub((difference.clamp(0.0, 1.0) * 1_000.0) as i64);
    }
    if let Some(lcp) = evidence.lcp_ms {
        score = score.saturating_sub(lcp.min(i64::MAX as f64) as i64);
        reasons.push(format!("LCP {lcp:.2} ms"));
    }
    if evidence.tests_failed == 0 {
        reasons.push(format!("{} tests passed", evidence.tests_passed));
    } else {
        reasons.push(format!("{} tests failed", evidence.tests_failed));
    }
    if evidence.semantic_regressions > 0 {
        reasons.push(format!(
            "{} semantic regressions",
            evidence.semantic_regressions
        ));
    }
    ExperimentRanking {
        id: snapshot.id.clone(),
        score,
        reasons,
    }
}

fn validate_name(value: &str, description: &str) -> DevelopmentResult<()> {
    if value.is_empty()
        || value.len() > 128
        || !value
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || "-_.".contains(character))
    {
        return Err(DevelopmentError::InvalidInput(format!(
            "{description} must be 1..=128 ASCII letters, digits, '-', '_' or '.'"
        )));
    }
    Ok(())
}

fn absolutize(path: &Path) -> DevelopmentResult<PathBuf> {
    if path.is_absolute() {
        Ok(path.to_path_buf())
    } else {
        Ok(std::env::current_dir()?.join(path))
    }
}

impl From<GitError> for DevelopmentError {
    fn from(error: GitError) -> Self {
        Self::Process(error.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT_ROOT: AtomicU64 = AtomicU64::new(0);

    fn repository() -> (PathBuf, PathBuf) {
        let sequence = NEXT_ROOT.fetch_add(1, Ordering::Relaxed);
        let base = std::env::temp_dir().join(format!(
            "glass-experiments-{}-{sequence}",
            std::process::id()
        ));
        let root = base.join("repository");
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(
            root.join("Cargo.toml"),
            "[package]\nname='fixture'\nversion='0.1.0'\n",
        )
        .unwrap();
        std::fs::write(
            root.join("glass.toml"),
            "[commands]\nbuild='rustc --version'\ntest='rustc --version'\n",
        )
        .unwrap();
        for arguments in [
            vec!["init"],
            vec!["config", "user.name", "Glass Test"],
            vec!["config", "user.email", "glass@example.invalid"],
            vec!["add", "."],
            vec!["commit", "-m", "initial"],
        ] {
            assert!(
                Command::new("git")
                    .args(arguments)
                    .current_dir(&root)
                    .status()
                    .unwrap()
                    .success()
            );
        }
        (base.clone(), root)
    }

    #[test]
    fn isolated_worktrees_are_ranked_and_selected_from_evidence() {
        let (base, root) = repository();
        let worktrees = base.join("worktrees");
        assert!(
            ExperimentManager::new_governed(&root, &worktrees, WorkspaceTrust::Untrusted).is_err()
        );
        let mut manager =
            ExperimentManager::new_governed(&root, &worktrees, WorkspaceTrust::TrustedOnce)
                .unwrap();
        let first = manager
            .create("approach-a", "experiment-a", Some(3101))
            .unwrap();
        let second = manager
            .create("approach-b", "experiment-b", Some(3102))
            .unwrap();
        let third = manager
            .create("approach-c", "experiment-c", Some(3103))
            .unwrap();
        assert!(first.worktree.exists());
        assert!(second.worktree.exists());
        assert!(third.worktree.exists());
        std::fs::write(first.worktree.join("a.rs"), "fn a() {}\n").unwrap();
        std::fs::write(second.worktree.join("b.rs"), "fn b() {}\n").unwrap();
        std::fs::write(third.worktree.join("c.rs"), "fn c() {}\n").unwrap();
        assert_eq!(manager.refresh_changed_files("approach-a").unwrap(), 1);
        assert_eq!(manager.refresh_changed_files("approach-b").unwrap(), 1);
        let automatic = manager.collect_automatic("approach-a").unwrap();
        assert_eq!(automatic.build_passed, Some(true));
        assert_eq!(automatic.tests_passed, 1);
        assert_eq!(automatic.changed_files, 1);
        assert!(automatic.provenance["buildPassed"].measured);
        assert!(automatic.provenance["buildPassed"].available);
        assert!(!automatic.provenance["workflowPassed"].available);
        manager
            .record_evidence(
                "approach-a",
                ExperimentEvidence {
                    tests_passed: 142,
                    workflow_passed: Some(true),
                    lcp_ms: Some(1_220.0),
                    ..ExperimentEvidence::default()
                },
            )
            .unwrap();
        assert!(!manager.snapshots()[0].evidence.provenance["tests"].measured);
        assert!(
            manager
                .set_weights(ExperimentWeights::default(), WorkspaceTrust::Untrusted)
                .is_err()
        );
        manager
            .record_evidence(
                "approach-b",
                ExperimentEvidence {
                    tests_passed: 141,
                    tests_failed: 1,
                    workflow_passed: Some(false),
                    semantic_regressions: 1,
                    lcp_ms: Some(1_080.0),
                    ..ExperimentEvidence::default()
                },
            )
            .unwrap();
        manager
            .record_evidence(
                "approach-c",
                ExperimentEvidence {
                    tests_passed: 120,
                    tests_failed: 4,
                    workflow_passed: Some(false),
                    semantic_regressions: 2,
                    lcp_ms: Some(1_450.0),
                    ..ExperimentEvidence::default()
                },
            )
            .unwrap();
        let comparison = manager.compare();
        assert_eq!(comparison.rankings.len(), 3);
        assert_eq!(comparison.recommended.as_deref(), Some("approach-a"));
        assert_eq!(
            manager.select("approach-a").unwrap().state,
            ExperimentState::Selected
        );
        assert!(manager.select("approach-b").is_err());
        manager.remove("approach-a", true).unwrap();
        manager.remove("approach-b", true).unwrap();
        manager.remove("approach-c", true).unwrap();
        drop(manager);
        std::fs::remove_dir_all(base).unwrap();
    }
}
