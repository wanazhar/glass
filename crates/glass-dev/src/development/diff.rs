use super::{
    DEVELOPMENT_SCHEMA_VERSION, DevelopmentError, DevelopmentGraph, DevelopmentResult,
    ProcessManager, Timeline,
};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, path::Path, process::Command};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct FileChange {
    pub path: String,
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ProjectDiff {
    pub schema_version: String,
    pub files: Vec<FileChange>,
    pub runtime: BTreeMap<String, serde_json::Value>,
    pub semantic: BTreeMap<String, serde_json::Value>,
    pub visual: BTreeMap<String, serde_json::Value>,
    pub workflow: BTreeMap<String, serde_json::Value>,
    pub test_impact: BTreeMap<String, serde_json::Value>,
}

pub fn build_diff(
    root: &Path,
    timeline: &Timeline,
    graph: &DevelopmentGraph,
    processes: &mut ProcessManager,
) -> DevelopmentResult<ProjectDiff> {
    let output = Command::new("git")
        .args(["status", "--short", "--untracked-files=all"])
        .current_dir(root)
        .output()
        .map_err(DevelopmentError::Io)?;
    let mut files = Vec::new();
    if output.status.success() {
        for line in String::from_utf8_lossy(&output.stdout).lines().take(512) {
            if line.len() < 4 {
                continue;
            }
            let status = line[..2].trim().to_string();
            let path = line[3..].trim().to_string();
            if !path.is_empty() {
                files.push(FileChange { path, status });
            }
        }
    }
    let process_snapshots = processes.list();
    let mut runtime = BTreeMap::new();
    runtime.insert(
        "processCount".into(),
        serde_json::json!(process_snapshots.len()),
    );
    runtime.insert("processes".into(), serde_json::to_value(process_snapshots)?);
    let mut semantic = BTreeMap::new();
    semantic.insert(
        "linkCount".into(),
        serde_json::json!(graph.links.values().map(Vec::len).sum::<usize>()),
    );
    semantic.insert(
        "breakpointHits".into(),
        serde_json::json!(
            timeline
                .events()
                .filter(|event| matches!(
                    event.kind,
                    super::DevelopmentEventKind::SemanticBreakpointHit
                ))
                .count()
        ),
    );
    let mut visual = BTreeMap::new();
    visual.insert("status".into(), serde_json::json!("not-captured"));
    visual.insert(
        "reason".into(),
        serde_json::json!(
            "visual evidence requires explicit capture; screenshots are never implicit"
        ),
    );
    semantic.insert(
        "entities".into(),
        serde_json::to_value(graph.links.keys().collect::<Vec<_>>())?,
    );
    let mut workflow = BTreeMap::new();
    let verification_count = timeline
        .events()
        .filter(|event| {
            matches!(
                event.kind,
                super::DevelopmentEventKind::VerificationCompleted
            )
        })
        .count();
    workflow.insert(
        "verificationCount".into(),
        serde_json::json!(verification_count),
    );
    workflow.insert(
        "recentEventCount".into(),
        serde_json::json!(timeline.events().count()),
    );
    let mut test_impact = BTreeMap::new();
    test_impact.insert(
        "started".into(),
        serde_json::json!(
            timeline
                .events()
                .filter(|event| matches!(event.kind, super::DevelopmentEventKind::TestStarted))
                .count()
        ),
    );
    test_impact.insert(
        "completed".into(),
        serde_json::json!(
            timeline
                .events()
                .filter(|event| matches!(event.kind, super::DevelopmentEventKind::TestCompleted))
                .count()
        ),
    );
    Ok(ProjectDiff {
        schema_version: DEVELOPMENT_SCHEMA_VERSION.into(),
        files,
        runtime,
        semantic,
        visual,
        workflow,
        test_impact,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::development::{Actor, DevelopmentEventKind, Timeline};
    use std::fs;

    #[test]
    fn diff_is_bounded_and_reports_runtime_and_semantic_sections() {
        let root = std::env::temp_dir().join(format!("glass-diff-{}", std::process::id()));
        let _ = fs::create_dir_all(&root);
        let mut timeline = Timeline::open(root.join("timeline.jsonl")).unwrap();
        timeline
            .record(
                Actor::local(),
                DevelopmentEventKind::VerificationCompleted,
                "/tmp",
                serde_json::json!({}),
            )
            .unwrap();
        let mut manager = ProcessManager::new(&root);
        let diff =
            build_diff(&root, &timeline, &DevelopmentGraph::default(), &mut manager).unwrap();
        assert_eq!(diff.schema_version, DEVELOPMENT_SCHEMA_VERSION);
        assert!(diff.runtime.contains_key("processes"));
        assert!(diff.semantic.contains_key("entities"));
        assert_eq!(diff.visual["status"], "not-captured");
        let _ = fs::remove_dir_all(root);
    }
}
