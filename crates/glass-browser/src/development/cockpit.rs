use super::{
    DEVELOPMENT_SCHEMA_VERSION, DevelopmentError, DevelopmentEvent, DevelopmentEventKind,
    DevelopmentResult, LocalHarness, ProjectDiff, ProjectWorkspace, Timeline,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

pub const MAX_RESIDENT_SESSIONS: usize = 8;
pub const DEFAULT_SESSION_IDLE_TIMEOUT: Duration = Duration::from_secs(30 * 60);
pub const MAX_ATTENTION_ITEMS: usize = 32;
pub const MAX_VERIFICATION_CHECKS: usize = 16;

#[derive(Debug)]
struct ResidentEntry {
    workspace: ProjectWorkspace,
    harness: LocalHarness,
    last_used: Instant,
}

#[derive(Debug)]
pub struct ResidentDevelopmentSessions {
    entries: BTreeMap<PathBuf, ResidentEntry>,
    capacity: usize,
    idle_timeout: Duration,
}

impl Default for ResidentDevelopmentSessions {
    fn default() -> Self {
        Self::new(MAX_RESIDENT_SESSIONS, DEFAULT_SESSION_IDLE_TIMEOUT)
    }
}

impl ResidentDevelopmentSessions {
    pub fn new(capacity: usize, idle_timeout: Duration) -> Self {
        Self {
            entries: BTreeMap::new(),
            capacity: capacity.clamp(1, MAX_RESIDENT_SESSIONS),
            idle_timeout,
        }
    }

    pub fn with_workspace<T>(
        &mut self,
        root: impl AsRef<Path>,
        operation: impl FnOnce(&mut ProjectWorkspace) -> DevelopmentResult<T>,
    ) -> DevelopmentResult<T> {
        self.with_runtime(root, |workspace, _| operation(workspace))
    }

    pub fn with_runtime<T>(
        &mut self,
        root: impl AsRef<Path>,
        operation: impl FnOnce(&mut ProjectWorkspace, &mut LocalHarness) -> DevelopmentResult<T>,
    ) -> DevelopmentResult<T> {
        let root = super::project::canonical_root(root.as_ref())?;
        self.prune_idle();
        if !self.entries.contains_key(&root) {
            self.evict_lru_if_full();
            self.entries.insert(
                root.clone(),
                ResidentEntry {
                    workspace: ProjectWorkspace::open(&root)?,
                    harness: LocalHarness::default(),
                    last_used: Instant::now(),
                },
            );
        }
        let entry = self
            .entries
            .get_mut(&root)
            .expect("resident entry inserted");
        entry.last_used = Instant::now();
        operation(&mut entry.workspace, &mut entry.harness)
    }

    pub fn contains(&mut self, root: impl AsRef<Path>) -> bool {
        self.prune_idle();
        super::project::canonical_root(root.as_ref())
            .ok()
            .is_some_and(|root| self.entries.contains_key(&root))
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn capacity(&self) -> usize {
        self.capacity
    }

    pub fn detach(&mut self, root: impl AsRef<Path>) -> DevelopmentResult<bool> {
        let root = super::project::canonical_root(root.as_ref())?;
        Ok(self.entries.remove(&root).is_some())
    }

    pub fn roots(&self) -> Vec<PathBuf> {
        self.entries.keys().cloned().collect()
    }

    fn prune_idle(&mut self) {
        let now = Instant::now();
        self.entries.retain(|_, entry| {
            let has_running_process = entry
                .workspace
                .processes()
                .list()
                .iter()
                .any(|process| matches!(process.state, super::ProcessState::Running));
            has_running_process || now.duration_since(entry.last_used) < self.idle_timeout
        });
    }

    fn evict_lru_if_full(&mut self) {
        if self.entries.len() < self.capacity {
            return;
        }
        if let Some(root) = self
            .entries
            .iter()
            .min_by_key(|(_, entry)| entry.last_used)
            .map(|(root, _)| root.clone())
        {
            self.entries.remove(&root);
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ReconnectCapsule {
    pub schema_version: String,
    pub project_root: String,
    pub event_cursor: Option<String>,
    pub mobile_view: Option<String>,
    pub browser_target_id: Option<String>,
    pub browser_revision: Option<u64>,
    pub pending_attention: Option<String>,
    pub live_mode: Option<String>,
    pub live_quality: Option<String>,
    pub saved_at_ms: u64,
}

impl ReconnectCapsule {
    pub fn new(project_root: impl AsRef<Path>) -> DevelopmentResult<Self> {
        let root = super::project::canonical_root(project_root.as_ref())?;
        Ok(Self {
            schema_version: DEVELOPMENT_SCHEMA_VERSION.into(),
            project_root: root.display().to_string(),
            event_cursor: None,
            mobile_view: None,
            browser_target_id: None,
            browser_revision: None,
            pending_attention: None,
            live_mode: None,
            live_quality: None,
            saved_at_ms: now_ms(),
        })
    }

    pub fn validate(&self) -> DevelopmentResult<()> {
        if self.schema_version != DEVELOPMENT_SCHEMA_VERSION {
            return Err(DevelopmentError::InvalidInput(
                "unsupported reconnect capsule schema".into(),
            ));
        }
        for (name, value, limit) in [
            ("projectRoot", Some(self.project_root.as_str()), 4096),
            ("eventCursor", self.event_cursor.as_deref(), 128),
            ("mobileView", self.mobile_view.as_deref(), 32),
            ("browserTargetId", self.browser_target_id.as_deref(), 128),
            ("pendingAttention", self.pending_attention.as_deref(), 256),
            ("liveMode", self.live_mode.as_deref(), 32),
            ("liveQuality", self.live_quality.as_deref(), 32),
        ] {
            if value.is_some_and(|value| value.len() > limit) {
                return Err(DevelopmentError::InvalidInput(format!(
                    "{name} exceeds the {limit} byte reconnect capsule limit"
                )));
            }
        }
        if self
            .mobile_view
            .as_deref()
            .is_some_and(|value| !matches!(value, "home" | "agent" | "app" | "diff" | "project"))
        {
            return Err(DevelopmentError::InvalidInput(
                "reconnect capsule mobileView is not recognized".into(),
            ));
        }
        if self
            .live_mode
            .as_deref()
            .is_some_and(|value| !matches!(value, "off" | "auto" | "on"))
        {
            return Err(DevelopmentError::InvalidInput(
                "reconnect capsule liveMode is not recognized".into(),
            ));
        }
        if self
            .live_quality
            .as_deref()
            .is_some_and(|value| !matches!(value, "auto" | "data" | "balanced" | "smooth"))
        {
            return Err(DevelopmentError::InvalidInput(
                "reconnect capsule liveQuality is not recognized".into(),
            ));
        }
        let root = super::project::canonical_root(Path::new(&self.project_root))?;
        if root != Path::new(&self.project_root) {
            return Err(DevelopmentError::InvalidInput(
                "reconnect capsule project root is not canonical".into(),
            ));
        }
        Ok(())
    }
}

pub struct ReconnectCapsuleStore;

impl ReconnectCapsuleStore {
    pub fn save(capsule: &ReconnectCapsule) -> DevelopmentResult<PathBuf> {
        capsule.validate()?;
        let path = capsule_path(Path::new(&capsule.project_root))?;
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let temporary = path.with_extension(format!("json.tmp-{}", std::process::id()));
        let mut file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&temporary)?;
        serde_json::to_writer_pretty(&mut file, capsule)?;
        file.write_all(b"\n")?;
        file.sync_all()?;
        if let Err(error) = fs::rename(&temporary, &path) {
            let _ = fs::remove_file(&temporary);
            return Err(error.into());
        }
        Ok(path)
    }

    pub fn load(root: impl AsRef<Path>) -> DevelopmentResult<Option<ReconnectCapsule>> {
        let path = capsule_path(root.as_ref())?;
        if !path.is_file() {
            return Ok(None);
        }
        let metadata = fs::metadata(&path)?;
        if metadata.len() > 16 * 1024 {
            return Err(DevelopmentError::InvalidInput(
                "reconnect capsule exceeds 16384 bytes".into(),
            ));
        }
        let capsule = serde_json::from_slice::<ReconnectCapsule>(&fs::read(path)?)?;
        capsule.validate()?;
        Ok(Some(capsule))
    }

    pub fn clear(root: impl AsRef<Path>) -> DevelopmentResult<bool> {
        let path = capsule_path(root.as_ref())?;
        if !path.exists() {
            return Ok(false);
        }
        fs::remove_file(path)?;
        Ok(true)
    }
}

fn capsule_path(root: &Path) -> DevelopmentResult<PathBuf> {
    let root = super::project::canonical_root(root)?;
    Ok(Timeline::for_project(&root)?
        .path()
        .with_file_name("reconnect-capsule.json"))
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum AttentionState {
    NeedsAttention,
    Running,
    Recent,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AttentionItem {
    pub id: String,
    pub state: AttentionState,
    pub title: String,
    pub detail: String,
    pub occurred_at_ms: u64,
    pub event_id: String,
}

pub fn attention_inbox(
    events: impl DoubleEndedIterator<Item = DevelopmentEvent>,
) -> Vec<AttentionItem> {
    let mut items = events
        .rev()
        .filter_map(|event| attention_item(&event))
        .take(MAX_ATTENTION_ITEMS)
        .collect::<Vec<_>>();
    items.sort_by_key(|item| std::cmp::Reverse(item.occurred_at_ms));
    items
}

fn attention_item(event: &DevelopmentEvent) -> Option<AttentionItem> {
    let (state, title, detail) = match event.kind {
        DevelopmentEventKind::ProcessStarted => (
            AttentionState::Running,
            "Process running",
            payload_label(&event.payload, "name", "managed process"),
        ),
        DevelopmentEventKind::TestStarted => (
            AttentionState::Running,
            "Tests running",
            payload_label(&event.payload, "name", "test run"),
        ),
        DevelopmentEventKind::AgentPrompt => (
            AttentionState::Running,
            "Agent working",
            "bounded local prompt accepted".into(),
        ),
        DevelopmentEventKind::DiagnosticsPublished
            if event
                .payload
                .get("count")
                .and_then(serde_json::Value::as_u64)
                .unwrap_or(0)
                > 0 =>
        {
            (
                AttentionState::NeedsAttention,
                "Diagnostics need attention",
                payload_label(&event.payload, "path", "project diagnostics"),
            )
        }
        DevelopmentEventKind::SemanticBreakpointHit => (
            AttentionState::NeedsAttention,
            "Semantic breakpoint hit",
            payload_label(&event.payload, "entity", "browser entity changed"),
        ),
        DevelopmentEventKind::ProcessExited => {
            let failed = event_failed(&event.payload);
            (
                if failed {
                    AttentionState::NeedsAttention
                } else {
                    AttentionState::Recent
                },
                if failed {
                    "Process failed"
                } else {
                    "Process completed"
                },
                payload_label(&event.payload, "name", "managed process"),
            )
        }
        DevelopmentEventKind::TestCompleted => {
            let failed = event_failed(&event.payload);
            (
                if failed {
                    AttentionState::NeedsAttention
                } else {
                    AttentionState::Recent
                },
                if failed {
                    "Tests failed"
                } else {
                    "Tests passed"
                },
                payload_label(&event.payload, "name", "test run"),
            )
        }
        DevelopmentEventKind::VerificationCompleted => (
            AttentionState::Recent,
            "Verification completed",
            payload_label(&event.payload, "status", "verification evidence updated"),
        ),
        DevelopmentEventKind::FileSaved => (
            AttentionState::Recent,
            "File saved",
            payload_label(&event.payload, "path", "project file"),
        ),
        _ => return None,
    };
    Some(AttentionItem {
        id: format!("attention:{}", event.id),
        state,
        title: title.into(),
        detail,
        occurred_at_ms: event.occurred_at_ms,
        event_id: event.id.clone(),
    })
}

fn event_failed(payload: &serde_json::Value) -> bool {
    if payload.get("success").and_then(serde_json::Value::as_bool) == Some(false)
        || payload
            .get("code")
            .and_then(serde_json::Value::as_i64)
            .is_some_and(|code| code != 0)
        || payload
            .get("status")
            .and_then(serde_json::Value::as_str)
            .is_some_and(|status| matches!(status, "failed" | "timeout" | "cancelled"))
    {
        return true;
    }
    let Some(state) = payload.get("state") else {
        return false;
    };
    state.as_str() == Some("failed")
        || state
            .get("exited")
            .and_then(|exited| exited.get("code"))
            .and_then(serde_json::Value::as_i64)
            .is_some_and(|code| code != 0)
}

fn payload_label(payload: &serde_json::Value, field: &str, fallback: &str) -> String {
    payload
        .get(field)
        .and_then(serde_json::Value::as_str)
        .filter(|value| value.len() <= 256)
        .unwrap_or(fallback)
        .to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct VerificationCheck {
    pub label: String,
    pub status: String,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct VerificationCard {
    pub schema_version: String,
    pub title: String,
    pub outcome: String,
    pub checks: Vec<VerificationCheck>,
    pub changed_files: usize,
    pub semantic_revision: Option<u64>,
    pub visual_status: String,
    pub generated_at_ms: u64,
}

impl VerificationCard {
    pub fn from_diff(
        title: &str,
        diff: &ProjectDiff,
        semantic_revision: Option<u64>,
    ) -> DevelopmentResult<Self> {
        if title.trim().is_empty() || title.len() > 128 {
            return Err(DevelopmentError::InvalidInput(
                "verification card title must be 1-128 bytes".into(),
            ));
        }
        let visual_status = diff
            .visual
            .get("status")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("not-captured")
            .to_string();
        let process_count = diff
            .runtime
            .get("processCount")
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(0);
        let link_count = diff
            .semantic
            .get("linkCount")
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(0);
        Ok(Self {
            schema_version: DEVELOPMENT_SCHEMA_VERSION.into(),
            title: title.into(),
            outcome: "reviewRequired".into(),
            checks: vec![
                VerificationCheck {
                    label: "Code changes".into(),
                    status: if diff.files.is_empty() {
                        "clean"
                    } else {
                        "changed"
                    }
                    .into(),
                    detail: format!("{} changed files", diff.files.len()),
                },
                VerificationCheck {
                    label: "Runtime".into(),
                    status: "observed".into(),
                    detail: format!("{process_count} managed processes"),
                },
                VerificationCheck {
                    label: "Semantic links".into(),
                    status: "observed".into(),
                    detail: format!("{link_count} source/runtime links"),
                },
                VerificationCheck {
                    label: "Visual evidence".into(),
                    status: visual_status.clone(),
                    detail: if visual_status == "not-captured" {
                        "request an explicit screenshot or comparison".into()
                    } else {
                        "explicit visual evidence attached".into()
                    },
                },
            ],
            changed_files: diff.files.len(),
            semantic_revision,
            visual_status,
            generated_at_ms: now_ms(),
        })
    }

    pub fn add_check(&mut self, check: VerificationCheck) -> DevelopmentResult<()> {
        if self.checks.len() == MAX_VERIFICATION_CHECKS {
            return Err(DevelopmentError::InvalidInput(
                "verification card check limit reached".into(),
            ));
        }
        if check.label.is_empty() || check.label.len() > 128 || check.detail.len() > 512 {
            return Err(DevelopmentError::InvalidInput(
                "verification check exceeds bounded text limits".into(),
            ));
        }
        self.checks.push(check);
        Ok(())
    }
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_millis() as u64)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(name: &str) -> PathBuf {
        let root =
            std::env::temp_dir().join(format!("glass-cockpit-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        fs::write(
            root.join("Cargo.toml"),
            "[package]\nname='demo'\nversion='0.1.0'\n",
        )
        .unwrap();
        root
    }

    #[test]
    fn resident_registry_reuses_and_evicts_bounded_workspaces() {
        let first = fixture("first");
        let second = fixture("second");
        let mut sessions = ResidentDevelopmentSessions::new(1, Duration::from_secs(60));
        sessions
            .with_workspace(&first, |workspace| {
                workspace.attach_actor(super::super::Actor::external("one"))?;
                Ok(())
            })
            .unwrap();
        sessions
            .with_workspace(&first, |workspace| {
                assert!(workspace.actors().any(|actor| actor.id == "external:one"));
                Ok(())
            })
            .unwrap();
        sessions.with_workspace(&second, |_| Ok(())).unwrap();
        assert_eq!(sessions.len(), 1);
        assert!(!sessions.contains(&first));
        assert!(sessions.contains(&second));
        let _ = fs::remove_dir_all(first);
        let _ = fs::remove_dir_all(second);
    }

    #[test]
    fn reconnect_capsule_round_trips_without_payload_fields() {
        let root = fixture("capsule");
        let mut capsule = ReconnectCapsule::new(&root).unwrap();
        capsule.event_cursor = Some("dev-1".into());
        capsule.mobile_view = Some("app".into());
        ReconnectCapsuleStore::save(&capsule).unwrap();
        assert_eq!(ReconnectCapsuleStore::load(&root).unwrap(), Some(capsule));
        assert!(ReconnectCapsuleStore::clear(&root).unwrap());
        assert_eq!(ReconnectCapsuleStore::load(&root).unwrap(), None);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn resident_registry_expires_idle_state_before_reopening() {
        let root = fixture("expiry");
        fs::write(root.join("src.rs"), "fn main() {}\n").unwrap();
        let mut sessions = ResidentDevelopmentSessions::new(2, Duration::ZERO);
        sessions
            .with_workspace(&root, |workspace| {
                workspace.open_buffer("src.rs", super::super::Actor::local())?;
                Ok(())
            })
            .unwrap();
        let retained = sessions
            .with_workspace(&root, |workspace| Ok(workspace.buffer("src.rs").is_some()))
            .unwrap();
        assert!(!retained);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn attention_inbox_classifies_only_actionable_runtime_events() {
        let events = vec![
            DevelopmentEvent::new(
                super::super::Actor::local(),
                DevelopmentEventKind::WorkspaceOpened,
                "/tmp",
                serde_json::json!({}),
                1,
            ),
            DevelopmentEvent::new(
                super::super::Actor::local(),
                DevelopmentEventKind::DiagnosticsPublished,
                "/tmp",
                serde_json::json!({"count":2,"path":"src/lib.rs"}),
                2,
            ),
            DevelopmentEvent::new(
                super::super::Actor::local(),
                DevelopmentEventKind::ProcessStarted,
                "/tmp",
                serde_json::json!({"name":"dev"}),
                3,
            ),
            DevelopmentEvent::new(
                super::super::Actor::local(),
                DevelopmentEventKind::TestCompleted,
                "/tmp",
                serde_json::json!({"name":"unit","state":{"exited":{"code":1}}}),
                4,
            ),
        ];
        let inbox = attention_inbox(events.into_iter());
        assert_eq!(inbox.len(), 3);
        assert!(
            inbox
                .iter()
                .any(|item| item.state == AttentionState::Running)
        );
        assert!(inbox.iter().any(|item| item.title == "Tests failed"));
    }

    #[test]
    fn verification_card_keeps_visual_evidence_explicit() {
        let diff = ProjectDiff {
            schema_version: DEVELOPMENT_SCHEMA_VERSION.into(),
            files: vec![],
            runtime: BTreeMap::from([("processCount".into(), serde_json::json!(1))]),
            semantic: BTreeMap::from([("linkCount".into(), serde_json::json!(2))]),
            visual: BTreeMap::from([("status".into(), serde_json::json!("not-captured"))]),
            workflow: BTreeMap::new(),
            test_impact: BTreeMap::new(),
        };
        let card = VerificationCard::from_diff("Checkout fix", &diff, Some(9)).unwrap();
        assert_eq!(card.visual_status, "not-captured");
        assert_eq!(card.checks.len(), 4);
        assert!(card.checks[3].detail.contains("explicit screenshot"));
    }
}
