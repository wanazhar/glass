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
    #[serde(default = "default_session")]
    pub session: String,
    #[serde(default)]
    pub capabilities: Vec<String>,
    #[serde(default)]
    pub authority: ActorAuthority,
    #[serde(default)]
    pub connection: ActorConnection,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum ActorAuthority {
    Owner,
    Mutate,
    #[default]
    ReadOnly,
    System,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum ActorConnection {
    Local,
    Embedded,
    Cli,
    Mcp,
    Daemon,
    #[default]
    Disconnected,
}

fn default_session() -> String {
    "legacy".into()
}

impl Actor {
    pub fn local() -> Self {
        Self {
            id: "human:local".into(),
            kind: ActorKind::Human,
            name: "Human".into(),
            session: "local".into(),
            capabilities: vec!["read".into(), "mutate".into(), "approve".into()],
            authority: ActorAuthority::Owner,
            connection: ActorConnection::Local,
        }
    }

    pub fn embedded() -> Self {
        Self {
            id: "embedded:glass-agent".into(),
            kind: ActorKind::EmbeddedAgent,
            name: "Glass Agent".into(),
            session: "embedded".into(),
            capabilities: vec!["read".into(), "tool.call".into()],
            authority: ActorAuthority::Mutate,
            connection: ActorConnection::Embedded,
        }
    }

    pub fn external(name: &str) -> Self {
        let normalized = name
            .chars()
            .filter(|character| character.is_ascii_alphanumeric() || "-_.".contains(*character))
            .take(64)
            .collect::<String>();
        let name = if normalized.is_empty() {
            "external".to_string()
        } else {
            normalized
        };
        Self {
            id: format!("external:{name}"),
            kind: ActorKind::ExternalAgent,
            name,
            session: "external".into(),
            capabilities: vec!["read".into(), "structured".into()],
            authority: ActorAuthority::ReadOnly,
            connection: ActorConnection::Cli,
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
    DiagnosticsPublished,
    SemanticBreakpointHit,
    TestStarted,
    TestCompleted,
    HmrObserved,
    ActorJoined,
    ActorLeft,
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DevelopmentEventPage {
    pub schema_version: String,
    pub events: Vec<DevelopmentEvent>,
    pub cursor: Option<String>,
    pub oldest_id: Option<String>,
    pub newest_id: Option<String>,
    pub has_more: bool,
    pub cursor_expired: bool,
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
            for line in BufReader::new(file).lines() {
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

    pub fn events(&self) -> impl DoubleEndedIterator<Item = &DevelopmentEvent> + ExactSizeIterator {
        self.events.iter()
    }

    /// Return one bounded page after an opaque event ID. If compaction removed
    /// the requested cursor, return the oldest retained page and mark the
    /// cursor expired so a subscriber can report the gap instead of silently
    /// claiming continuity.
    pub fn events_after(&self, after_id: Option<&str>, limit: usize) -> DevelopmentEventPage {
        let limit = limit.clamp(1, 256);
        let start = match after_id {
            Some(after_id) => self
                .events
                .iter()
                .position(|event| event.id == after_id)
                .map(|index| index.saturating_add(1)),
            None => Some(0),
        };
        let cursor_expired = after_id.is_some() && start.is_none();
        let start = start.unwrap_or(0);
        let events = self
            .events
            .iter()
            .skip(start)
            .take(limit)
            .cloned()
            .collect::<Vec<_>>();
        let has_more = self.events.len().saturating_sub(start) > events.len();
        let cursor = events.last().map(|event| event.id.clone()).or_else(|| {
            (!cursor_expired)
                .then(|| after_id.map(str::to_string))
                .flatten()
        });
        DevelopmentEventPage {
            schema_version: DEVELOPMENT_SCHEMA_VERSION.into(),
            events,
            cursor,
            oldest_id: self.events.front().map(|event| event.id.clone()),
            newest_id: self.events.back().map(|event| event.id.clone()),
            has_more,
            cursor_expired,
        }
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
        if self.events.len() == MAX_TIMELINE_EVENTS {
            self.events.pop_front();
        }
        self.events.push_back(event.clone());
        self.persist_bounded()?;
        Ok(event)
    }

    fn persist_bounded(&self) -> DevelopmentResult<()> {
        let temporary = self.path.with_extension(format!(
            "jsonl.tmp-{}-{}",
            std::process::id(),
            self.next_ordinal
        ));
        let mut file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&temporary)?;
        for event in &self.events {
            serde_json::to_writer(&mut file, event)?;
            file.write_all(b"\n")?;
        }
        file.sync_all()?;
        if let Err(error) = fs::rename(&temporary, &self.path) {
            let _ = fs::remove_file(&temporary);
            return Err(error.into());
        }
        Ok(())
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

    #[test]
    fn timeline_persists_only_the_newest_bounded_events() {
        let root = std::env::temp_dir().join(format!("glass-timeline-cap-{}", std::process::id()));
        let path = root.join("timeline.jsonl");
        let _ = fs::remove_dir_all(&root);
        let mut timeline = Timeline::open(&path).unwrap();
        for ordinal in 0..MAX_TIMELINE_EVENTS + 3 {
            timeline
                .record(
                    Actor::local(),
                    DevelopmentEventKind::WorkspaceOpened,
                    "/tmp/project",
                    serde_json::json!({"ordinal": ordinal}),
                )
                .unwrap();
        }
        let reopened = Timeline::open(&path).unwrap();
        assert_eq!(reopened.events().count(), MAX_TIMELINE_EVENTS);
        assert_eq!(reopened.events().next().unwrap().payload["ordinal"], 3);
        assert_eq!(
            fs::read_to_string(path).unwrap().lines().count(),
            MAX_TIMELINE_EVENTS
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn event_pages_are_cursor_bounded_and_report_compaction_gaps() {
        let root = std::env::temp_dir().join(format!("glass-event-page-{}", std::process::id()));
        let path = root.join("timeline.jsonl");
        let _ = fs::remove_dir_all(&root);
        let mut timeline = Timeline::open(&path).unwrap();
        for ordinal in 0..3 {
            timeline
                .record(
                    Actor::local(),
                    DevelopmentEventKind::WorkspaceOpened,
                    "/tmp/project",
                    serde_json::json!({"ordinal": ordinal}),
                )
                .unwrap();
        }
        let first = timeline.events_after(None, 2);
        assert_eq!(first.events.len(), 2);
        assert!(first.has_more);
        assert!(!first.cursor_expired);
        let second = timeline.events_after(first.cursor.as_deref(), 2);
        assert_eq!(second.events.len(), 1);
        assert_eq!(second.events[0].payload["ordinal"], 2);
        assert!(!second.has_more);
        let expired = timeline.events_after(Some("dev-expired"), 1);
        assert!(expired.cursor_expired);
        assert_eq!(expired.events[0].payload["ordinal"], 0);
        let empty = Timeline::open(root.join("empty.jsonl")).unwrap();
        let empty_expired = empty.events_after(Some("dev-expired"), 1);
        assert!(empty_expired.cursor_expired);
        assert_eq!(empty_expired.cursor, None);
        let _ = fs::remove_dir_all(root);
    }
}
