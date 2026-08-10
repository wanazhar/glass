//! Isolated competing implementations ranked from observable evidence.

use crate::DevelopmentWorkspace;
use crate::agents::{AgentId, AgentRegistry, AgentSpec};
use crate::git::{GitError, GitService};
use crate::testing::TestRun;
use glass_browser::development::{DevelopmentError, DevelopmentResult};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::time::Duration;

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
    pub notes: String,
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
}

impl ExperimentManager {
    pub fn new(
        repository_root: impl AsRef<Path>,
        worktree_root: impl AsRef<Path>,
    ) -> DevelopmentResult<Self> {
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
            Ok(workspace) => workspace,
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

    pub fn record_evidence(
        &mut self,
        id: &str,
        evidence: ExperimentEvidence,
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
            .map(|experiment| rank(&experiment.snapshot))
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

fn rank(snapshot: &ExperimentSnapshot) -> ExperimentRanking {
    let evidence = &snapshot.evidence;
    let mut score = 0_i64;
    let mut reasons = Vec::new();
    score = score.saturating_add((evidence.tests_passed.min(10_000) as i64) * 10);
    score = score.saturating_sub((evidence.tests_failed.min(10_000) as i64) * 1_000);
    match evidence.workflow_passed {
        Some(true) => {
            score = score.saturating_add(5_000);
            reasons.push("workflow passed".into());
        }
        Some(false) => {
            score = score.saturating_sub(20_000);
            reasons.push("workflow failed".into());
        }
        None => reasons.push("workflow not measured".into()),
    }
    score = score.saturating_sub((evidence.semantic_regressions.min(10_000) as i64) * 2_000);
    score = score.saturating_sub((evidence.diagnostics.min(10_000) as i64) * 200);
    score = score.saturating_sub((evidence.changed_files.min(10_000) as i64) * 5);
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
        for arguments in [
            vec!["init"],
            vec!["config", "user.name", "Glass Test"],
            vec!["config", "user.email", "glass@example.invalid"],
            vec!["add", "Cargo.toml"],
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
        let mut manager = ExperimentManager::new(&root, &worktrees).unwrap();
        let first = manager
            .create("approach-a", "experiment-a", Some(3101))
            .unwrap();
        let second = manager
            .create("approach-b", "experiment-b", Some(3102))
            .unwrap();
        assert!(first.worktree.exists());
        assert!(second.worktree.exists());
        std::fs::write(first.worktree.join("a.rs"), "fn a() {}\n").unwrap();
        std::fs::write(second.worktree.join("b.rs"), "fn b() {}\n").unwrap();
        assert_eq!(manager.refresh_changed_files("approach-a").unwrap(), 1);
        assert_eq!(manager.refresh_changed_files("approach-b").unwrap(), 1);
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
        let comparison = manager.compare();
        assert_eq!(comparison.recommended.as_deref(), Some("approach-a"));
        assert_eq!(
            manager.select("approach-a").unwrap().state,
            ExperimentState::Selected
        );
        assert!(manager.select("approach-b").is_err());
        manager.remove("approach-a", true).unwrap();
        manager.remove("approach-b", true).unwrap();
        drop(manager);
        std::fs::remove_dir_all(base).unwrap();
    }
}
