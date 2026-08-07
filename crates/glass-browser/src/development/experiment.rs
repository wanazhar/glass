use super::{DevelopmentError, DevelopmentResult};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ExperimentWorkspace {
    pub name: String,
    pub branch: String,
    pub worktree: PathBuf,
    pub dev_port: u16,
    pub browser_url: String,
    pub agent_thread: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ExperimentEvidence {
    pub name: String,
    pub files_changed: usize,
    pub insertions: u64,
    pub deletions: u64,
    pub test_status: Option<String>,
    pub semantic_regressions: Option<usize>,
    pub workflow_status: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ExperimentComparison {
    pub left: ExperimentEvidence,
    pub right: ExperimentEvidence,
}

#[derive(Debug, Clone)]
pub struct ExperimentManager {
    repository: PathBuf,
    root: PathBuf,
}

impl ExperimentManager {
    pub fn new(repository: &Path) -> DevelopmentResult<Self> {
        let repository = fs::canonicalize(repository)?;
        let output = Command::new("git")
            .args(["rev-parse", "--show-toplevel"])
            .current_dir(&repository)
            .output()?;
        if !output.status.success() {
            return Err(DevelopmentError::InvalidInput(
                "experiments require a Git repository".into(),
            ));
        }
        let top = String::from_utf8(output.stdout)
            .map_err(|error| DevelopmentError::InvalidInput(error.to_string()))?;
        let repository = fs::canonicalize(top.trim())?;
        let repository_name = repository
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("project");
        let root = repository
            .parent()
            .unwrap_or(&repository)
            .join(".glass-worktrees")
            .join(repository_name);
        Ok(Self { repository, root })
    }

    pub fn create(&self, name: &str, dev_port: u16) -> DevelopmentResult<ExperimentWorkspace> {
        validate_name(name)?;
        if dev_port == 0 {
            return Err(DevelopmentError::InvalidInput(
                "experiment dev port must be non-zero".into(),
            ));
        }
        fs::create_dir_all(&self.root)?;
        let worktree = self.root.join(name);
        if worktree.exists() {
            return Err(DevelopmentError::Conflict(format!(
                "experiment already exists: {name}"
            )));
        }
        let branch = format!("glass/experiment/{name}");
        let status = Command::new("git")
            .args(["worktree", "add", "-b", &branch])
            .arg(&worktree)
            .arg("HEAD")
            .current_dir(&self.repository)
            .status()?;
        if !status.success() {
            return Err(DevelopmentError::Process(format!(
                "git could not create experiment {name}"
            )));
        }
        Ok(ExperimentWorkspace {
            name: name.into(),
            branch,
            worktree,
            dev_port,
            browser_url: format!("http://localhost:{dev_port}"),
            agent_thread: format!("experiment:{name}"),
        })
    }

    pub fn evidence(
        &self,
        experiment: &ExperimentWorkspace,
        test_status: Option<String>,
        semantic_regressions: Option<usize>,
        workflow_status: Option<String>,
    ) -> DevelopmentResult<ExperimentEvidence> {
        let output = Command::new("git")
            .args(["diff", "--numstat", "HEAD"])
            .current_dir(&experiment.worktree)
            .output()?;
        if !output.status.success() {
            return Err(DevelopmentError::Process(format!(
                "git diff failed for experiment {}",
                experiment.name
            )));
        }
        let mut files_changed = 0;
        let mut insertions = 0_u64;
        let mut deletions = 0_u64;
        for line in String::from_utf8_lossy(&output.stdout).lines().take(4096) {
            let mut fields = line.split('\t');
            let added = fields.next().unwrap_or("0");
            let removed = fields.next().unwrap_or("0");
            if fields.next().is_none() {
                continue;
            }
            files_changed += 1;
            insertions = insertions.saturating_add(added.parse().unwrap_or(0));
            deletions = deletions.saturating_add(removed.parse().unwrap_or(0));
        }
        Ok(ExperimentEvidence {
            name: experiment.name.clone(),
            files_changed,
            insertions,
            deletions,
            test_status,
            semantic_regressions,
            workflow_status,
        })
    }

    pub fn compare(
        &self,
        left: ExperimentEvidence,
        right: ExperimentEvidence,
    ) -> ExperimentComparison {
        ExperimentComparison { left, right }
    }
}

fn validate_name(name: &str) -> DevelopmentResult<()> {
    if name.is_empty()
        || name.len() > 48
        || name
            .chars()
            .any(|character| !(character.is_ascii_alphanumeric() || "-_".contains(character)))
    {
        return Err(DevelopmentError::InvalidInput(
            "experiment name must be 1-48 ASCII letters, digits, '-' or '_'".into(),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn worktree_experiment_is_real_and_evidence_is_measured() {
        let root = std::env::temp_dir().join(format!("glass-experiment-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        for args in [
            vec!["init"],
            vec!["config", "user.email", "glass@example.invalid"],
            vec!["config", "user.name", "Glass Test"],
        ] {
            assert!(
                Command::new("git")
                    .args(args)
                    .current_dir(&root)
                    .status()
                    .unwrap()
                    .success()
            );
        }
        fs::write(root.join("app.txt"), "base\n").unwrap();
        assert!(
            Command::new("git")
                .args(["add", "."])
                .current_dir(&root)
                .status()
                .unwrap()
                .success()
        );
        assert!(
            Command::new("git")
                .args(["commit", "-m", "base"])
                .current_dir(&root)
                .status()
                .unwrap()
                .success()
        );
        let manager = ExperimentManager::new(&root).unwrap();
        let experiment = manager.create("candidate_a", 3101).unwrap();
        fs::write(experiment.worktree.join("app.txt"), "base\nchange\n").unwrap();
        let evidence = manager
            .evidence(&experiment, Some("pass".into()), Some(0), None)
            .unwrap();
        assert_eq!(evidence.files_changed, 1);
        assert_eq!(evidence.insertions, 1);
        assert_eq!(experiment.browser_url, "http://localhost:3101");
        let _ = Command::new("git")
            .args(["worktree", "remove", "--force"])
            .arg(&experiment.worktree)
            .current_dir(&root)
            .status();
        let _ = fs::remove_dir_all(root);
    }
}
