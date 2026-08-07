use super::{DEVELOPMENT_SCHEMA_VERSION, DevelopmentResult, MAX_TIMELINE_EVENTS};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::VecDeque,
    fs::{self, OpenOptions},
    io::{BufRead, BufReader, Write},
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum ActorKind {
    Human,
    EmbeddedAgent,
    ExternalAgent,
    System,
    Observer,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Actor {
    pub id: String,
    pub kind: ActorKind,
    pub name: String,
}

impl Actor {
    pub fn local() -> Self {
        Self {
            id: "human:local".into(),
            kind: ActorKind::Human,
            name: "Human".into(),
        }
    }

    pub fn embedded() -> Self {
        Self {
            id: "embedded:glass-agent".into(),
            kind: ActorKind::EmbeddedAgent,
            name: "Glass Agent".into(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum DevelopmentEventKind {
    WorkspaceOpened,
    FileOpened,
    FileSaved,
    ProcessStarted,
    ProcessOutput,
    ProcessExited,
    AgentPrompt,
    AgentSteered,
    AgentToolCalled,
    AgentToolResult,
    SourceRuntimeLinked,
    VerificationCompleted,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DevelopmentEvent {
    pub schema_version: String,
    pub id: String,
    pub occurred_at_ms: u64,
    pub actor: Actor,
    pub kind: DevelopmentEventKind,
    pub workspace: String,
    #[serde(default)]
    pub payload: Value,
}

impl DevelopmentEvent {
    pub fn new(
        actor: Actor,
        kind: DevelopmentEventKind,
        workspace: impl Into<String>,
        payload: Value,
        ordinal: u64,
    ) -> Self {
        let occurred_at_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |duration| duration.as_millis() as u64);
        Self {
            schema_version: DEVELOPMENT_SCHEMA_VERSION.into(),
            id: format!("dev-{occurred_at_ms}-{ordinal}"),
            occurred_at_ms,
            actor,
            kind,
            workspace: workspace.into(),
            payload,
        }
    }
}

#[derive(Debug)]
pub struct Timeline {
    path: PathBuf,
    events: VecDeque<DevelopmentEvent>,
    next_ordinal: u64,
}

impl Timeline {
    pub fn for_project(root: &Path) -> DevelopmentResult<Self> {
        use sha2::{Digest, Sha256};
        let state_root = dirs::data_local_dir()
            .or_else(dirs::cache_dir)
            .unwrap_or_else(std::env::temp_dir)
            .join("glass")
            .join("development");
        let digest = Sha256::digest(root.to_string_lossy().as_bytes());
        let project_id = digest
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        Self::open(state_root.join(project_id).join("timeline.jsonl"))
    }

    pub fn open(path: impl Into<PathBuf>) -> DevelopmentResult<Self> {
        let path = path.into();
        let mut events = VecDeque::new();
        if path.is_file() {
            let file = fs::File::open(&path)?;
            for line in BufReader::new(file).lines().take(MAX_TIMELINE_EVENTS) {
                let line = line?;
                if line.len() > 64 * 1024 {
                    continue;
                }
                if let Ok(event) = serde_json::from_str::<DevelopmentEvent>(&line) {
                    if events.len() == MAX_TIMELINE_EVENTS {
                        events.pop_front();
                    }
                    events.push_back(event);
                }
            }
        }
        Ok(Self {
            path,
            next_ordinal: events.len() as u64 + 1,
            events,
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn events(&self) -> impl Iterator<Item = &DevelopmentEvent> {
        self.events.iter()
    }

    pub fn record(
        &mut self,
        actor: Actor,
        kind: DevelopmentEventKind,
        workspace: impl Into<String>,
        payload: Value,
    ) -> DevelopmentResult<DevelopmentEvent> {
        let event = DevelopmentEvent::new(actor, kind, workspace, payload, self.next_ordinal);
        self.next_ordinal = self.next_ordinal.saturating_add(1);
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)?;
        }
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)?;
        let serialized = serde_json::to_string(&event)?;
        file.write_all(serialized.as_bytes())?;
        file.write_all(b"\n")?;
        file.flush()?;
        if self.events.len() == MAX_TIMELINE_EVENTS {
            self.events.pop_front();
        }
        self.events.push_back(event.clone());
        Ok(event)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn timeline_round_trips_bounded_events() {
        let root = std::env::temp_dir().join(format!("glass-timeline-{}", std::process::id()));
        let path = root.join("timeline.jsonl");
        let _ = fs::remove_dir_all(&root);
        let mut timeline = Timeline::open(&path).unwrap();
        timeline
            .record(
                Actor::local(),
                DevelopmentEventKind::WorkspaceOpened,
                "/tmp/project",
                serde_json::json!({"status":"ready"}),
            )
            .unwrap();
        let reopened = Timeline::open(&path).unwrap();
        assert_eq!(reopened.events().count(), 1);
        assert_eq!(
            reopened.events().next().unwrap().kind,
            DevelopmentEventKind::WorkspaceOpened
        );
        let _ = fs::remove_dir_all(root);
    }
}
