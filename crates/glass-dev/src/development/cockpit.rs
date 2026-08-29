use super::{
    Actor, DEVELOPMENT_SCHEMA_VERSION, DevelopmentError, DevelopmentEvent, DevelopmentEventKind,
    DevelopmentResult, LocalHarness, ProjectDiff, ProjectWorkspace, Timeline, ToolAuthorization,
    ToolCall,
};
use base64::Engine as _;
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs::{self, OpenOptions},
    io::{Read, Write},
    net::{SocketAddr, TcpListener, TcpStream},
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread::{self, JoinHandle},
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mobile_scroll: Option<u16>,
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
            mobile_scroll: None,
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
        if self.mobile_view.as_deref().is_some_and(|value| {
            !matches!(
                value,
                "home"
                    | "overview"
                    | "agent"
                    | "app"
                    | "browser"
                    | "diff"
                    | "project"
                    | "process"
                    | "logs"
            )
        }) {
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
const MAX_COCKPIT_REQUEST_BYTES: usize = 128 * 1024;
const MAX_COCKPIT_RESPONSE_BYTES: usize = 512 * 1024;

/// A loopback-only HTTP cockpit for private local inspection and commands.
///
/// The token is part of the URL, the listener binds only to `127.0.0.1`, and
/// every request is bounded before it reaches the resident workspace. This is
/// intentionally not a public remote-view transport; use SSH/Mosh port
/// forwarding when the operator is remote.
#[derive(Debug)]
pub struct LocalCockpit {
    address: SocketAddr,
    token: String,
    stop: Arc<AtomicBool>,
    join: Option<JoinHandle<()>>,
}

impl LocalCockpit {
    pub fn start(workspace: crate::SharedDevelopmentWorkspace) -> DevelopmentResult<Self> {
        let listener = TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))?;
        listener.set_nonblocking(true)?;
        let address = listener.local_addr()?;
        let token = cockpit_token()?;
        let stop = Arc::new(AtomicBool::new(false));
        let thread_stop = Arc::clone(&stop);
        let thread_token = token.clone();
        let join = thread::Builder::new()
            .name("glass-private-cockpit".into())
            .spawn(move || cockpit_loop(listener, workspace, thread_token, thread_stop))
            .map_err(|error| {
                DevelopmentError::Process(format!("failed to start private cockpit: {error}"))
            })?;
        Ok(Self {
            address,
            token,
            stop,
            join: Some(join),
        })
    }

    pub fn address(&self) -> SocketAddr {
        self.address
    }

    pub fn token(&self) -> &str {
        &self.token
    }

    pub fn local_url(&self) -> String {
        format!("http://{}/{}/", self.address, self.token)
    }

    pub fn shutdown(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
    }
}

impl Drop for LocalCockpit {
    fn drop(&mut self) {
        self.shutdown();
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CockpitCommandRequest {
    name: String,
    #[serde(default)]
    arguments: serde_json::Value,
    #[serde(default)]
    allow_mutation: bool,
    #[serde(default)]
    confirmed: bool,
    expected_generation: Option<u64>,
    expected_project_revision: Option<u64>,
}

fn cockpit_loop(
    listener: TcpListener,
    workspace: crate::SharedDevelopmentWorkspace,
    token: String,
    stop: Arc<AtomicBool>,
) {
    let mut github_probe = crate::github::GitHubProbeCache::default();
    while !stop.load(Ordering::Acquire) {
        match listener.accept() {
            Ok((mut stream, _)) => {
                let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
                let response =
                    handle_cockpit_connection(&mut stream, &workspace, &token, &mut github_probe);
                write_http_response(&mut stream, response);
            }
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                thread::sleep(Duration::from_millis(25));
            }
            Err(_) => break,
        }
    }
}

fn handle_cockpit_connection(
    stream: &mut TcpStream,
    workspace: &crate::SharedDevelopmentWorkspace,
    token: &str,
    github_probe: &mut crate::github::GitHubProbeCache,
) -> (u16, &'static str, Vec<u8>) {
    let request = match read_http_request(stream) {
        Ok(request) => request,
        Err(error) => return json_http(400, serde_json::json!({"error":error})),
    };
    let (method, path, body) = request;
    let prefix = format!("/{token}");
    if !path.starts_with(&prefix) || !path[prefix.len()..].strip_prefix('/').is_some_and(|_| true) {
        return json_http(
            401,
            serde_json::json!({"error":"private cockpit token required"}),
        );
    }
    let endpoint = path[prefix.len()..].split('?').next().unwrap_or_default();
    match (method.as_str(), endpoint) {
        ("GET", "/") => (
            200,
            "text/html; charset=utf-8",
            cockpit_html(&format!("/{token}/")).into_bytes(),
        ),
        ("GET", "/v1/health") => json_http(200, serde_json::json!({"ok":true})),
        ("GET", "/v1/state") => match cockpit_state(workspace, github_probe) {
            Ok(value) => json_http(200, value),
            Err(error) => json_http(503, serde_json::json!({"error":error.to_string()})),
        },
        ("POST", "/v1/command") => match cockpit_command(workspace, &body) {
            Ok(value) => json_http(200, value),
            Err(error) => {
                let status = if matches!(error, DevelopmentError::Conflict(_)) {
                    409
                } else {
                    400
                };
                json_http(status, serde_json::json!({"error":error.to_string()}))
            }
        },
        _ => json_http(
            404,
            serde_json::json!({"error":"cockpit endpoint not found"}),
        ),
    }
}

fn read_http_request(stream: &mut TcpStream) -> Result<(String, String, Vec<u8>), String> {
    let mut bytes = Vec::with_capacity(4096);
    let mut chunk = [0_u8; 4096];
    let header_end = loop {
        let read = stream.read(&mut chunk).map_err(|error| error.to_string())?;
        if read == 0 {
            return Err("request ended before headers".into());
        }
        bytes.extend_from_slice(&chunk[..read]);
        if bytes.len() > MAX_COCKPIT_REQUEST_BYTES {
            return Err("request exceeds the private cockpit limit".into());
        }
        if let Some(index) = bytes.windows(4).position(|window| window == b"\r\n\r\n") {
            break index + 4;
        }
    };
    let header = std::str::from_utf8(&bytes[..header_end])
        .map_err(|_| "request headers are not UTF-8".to_string())?;
    let mut lines = header.split("\r\n");
    let request_line = lines.next().ok_or("request line missing")?;
    let mut request_parts = request_line.split_whitespace();
    let method = request_parts
        .next()
        .ok_or("request method missing")?
        .to_string();
    let path = request_parts
        .next()
        .ok_or("request path missing")?
        .to_string();
    if request_parts.next().is_none() {
        return Err("HTTP version missing".into());
    }
    let mut content_length = 0usize;
    for line in lines {
        if let Some((name, value)) = line.split_once(':')
            && name.eq_ignore_ascii_case("content-length")
        {
            content_length = value
                .trim()
                .parse()
                .map_err(|_| "invalid content length".to_string())?;
        }
    }
    if content_length > MAX_COCKPIT_REQUEST_BYTES - header_end {
        return Err("request body exceeds the private cockpit limit".into());
    }
    while bytes.len() < header_end + content_length {
        let read = stream.read(&mut chunk).map_err(|error| error.to_string())?;
        if read == 0 {
            return Err("request ended before body".into());
        }
        bytes.extend_from_slice(&chunk[..read]);
    }
    Ok((
        method,
        path,
        bytes[header_end..header_end + content_length].to_vec(),
    ))
}

fn cockpit_state(
    workspace: &crate::SharedDevelopmentWorkspace,
    github_probe: &mut crate::github::GitHubProbeCache,
) -> DevelopmentResult<serde_json::Value> {
    let (root, generation, revision, trust, agents, tasks, git, proposals) = {
        let mut locked = workspace.lock()?;
        let root = locked.root().display().to_string();
        let generation = locked.generation();
        let revision = locked.project().revision();
        let trust = locked.trust().label().to_string();
        let agents = locked.agents().list().map_err(|error| error.to_string());
        let tasks = locked.task_snapshots().map_err(|error| error.to_string());
        let git = locked
            .git()
            .map(|git| git.status().map_err(|error| error.to_string()));
        let proposals = locked
            .project()
            .editor_proposals()
            .into_iter()
            .take(32)
            .map(|proposal| {
                serde_json::json!({
                    "id": proposal.id,
                    "path": proposal.path,
                    "summary": proposal.summary,
                    "state": proposal.state,
                })
            })
            .collect::<Vec<_>>();
        (
            root, generation, revision, trust, agents, tasks, git, proposals,
        )
    };
    let github = github_probe.probe(Path::new(&root));
    let agents = match agents {
        Ok(value) => serde_json::to_value(value)?,
        Err(error) => serde_json::json!({"error":error}),
    };
    let tasks = match tasks {
        Ok(value) => serde_json::to_value(value)?,
        Err(error) => serde_json::json!({"error":error}),
    };
    let git = match git {
        Some(Ok(value)) => serde_json::to_value(value)?,
        Some(Err(error)) => serde_json::json!({"error":error}),
        None => serde_json::Value::Null,
    };
    Ok(serde_json::json!({
        "schemaVersion": super::DEVELOPMENT_COCKPIT_SCHEMA_VERSION,
        "root": root,
        "generation": generation,
        "projectRevision": revision,
        "trust": trust,
        "agents": agents,
        "tasks": tasks,
        "git": git,
        "github": github,
        "proposals": proposals,
    }))
}
fn cockpit_command(
    workspace: &crate::SharedDevelopmentWorkspace,
    body: &[u8],
) -> DevelopmentResult<serde_json::Value> {
    let request: CockpitCommandRequest = serde_json::from_slice(body)?;
    if request.name.is_empty()
        || request.name.len() > 128
        || !request
            .name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_'))
    {
        return Err(DevelopmentError::InvalidInput(
            "cockpit command names must be bounded tool identifiers".into(),
        ));
    }
    let mut locked = workspace.lock()?;
    let call = ToolCall {
        id: format!("cockpit-{}", now_ms()),
        name: request.name,
        arguments: request.arguments,
    };
    let expected_generation = request.expected_generation.unwrap_or(locked.generation());
    let expected_project_revision = request
        .expected_project_revision
        .unwrap_or_else(|| locked.project().revision());
    let context = crate::tools::DevelopmentToolContext {
        authorization: ToolAuthorization {
            actor: Actor::external("cockpit"),
            allow_mutation: request.allow_mutation && request.confirmed,
            confirmed: request.confirmed,
            unrestricted: false,
        },
        initiator: None,
        expected_generation,
        expected_project_revision,
    };
    let result = locked.execute_tool(&call, &context)?;
    Ok(serde_json::json!({"ok":true,"tool":call.name,"result":result}))
}

fn write_http_response(stream: &mut TcpStream, response: (u16, &'static str, Vec<u8>)) {
    let (status, content_type, body) = response;
    let reason = match status {
        200 => "OK",
        400 => "Bad Request",
        401 => "Unauthorized",
        404 => "Not Found",
        409 => "Conflict",
        503 => "Service Unavailable",
        _ => "Error",
    };
    let header = format!(
        "HTTP/1.1 {status} {reason}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n",
        body.len()
    );
    let _ = stream.write_all(header.as_bytes());
    let _ = stream.write_all(&body);
}

fn json_http(status: u16, value: serde_json::Value) -> (u16, &'static str, Vec<u8>) {
    let body = serde_json::to_vec(&value)
        .unwrap_or_else(|_| b"{\"error\":\"serialization failure\"}".to_vec());
    if body.len() > MAX_COCKPIT_RESPONSE_BYTES {
        return (
            503,
            "application/json",
            b"{\"error\":\"cockpit response exceeded its limit\"}".to_vec(),
        );
    }
    (status, "application/json", body)
}

fn cockpit_token() -> DevelopmentResult<String> {
    let mut bytes = [0_u8; 24];
    getrandom::fill(&mut bytes).map_err(|error| {
        DevelopmentError::Process(format!("private cockpit token generation failed: {error}"))
    })?;
    Ok(base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes))
}

fn cockpit_html(base: &str) -> String {
    format!(
        r##"<!doctype html>
<meta charset="utf-8">
<title>Glass private cockpit</title>
<style>
body{{font:15px/1.45 system-ui,sans-serif;background:#10131a;color:#e8edf5;margin:2rem;max-width:72rem}}
pre{{background:#181d27;padding:1rem;border-radius:.6rem;white-space:pre-wrap}}
button,input{{font:inherit;padding:.45rem .7rem;margin:.2rem 0}}
button{{cursor:pointer}} code{{color:#9ad1ff}}
#proposals div{{display:flex;gap:.6rem;align-items:center;margin:.35rem 0;flex-wrap:wrap}}
</style>
<h1>Glass private cockpit</h1>
<p>Loopback-only workspace state. Mutations remain governed by Glass trust and revision checks.</p>
<p>
<button id="refresh">Refresh</button>
<button id="review">Review GitHub PR</button>
<button id="accept-pack">Accept pack</button>
</p>
<div id="proposals"></div>
<pre id="state">Loading…</pre>
<script>
const base = "{}";
const state = document.querySelector("#state");
const proposalsEl = document.querySelector("#proposals");
let lastState = {{}};
async function load() {{
  const response = await fetch(base + "v1/state", {{cache:"no-store"}});
  lastState = await response.json();
  renderProposals(lastState.proposals || []);
  state.textContent = JSON.stringify(lastState, null, 2);
}}
function renderProposals(proposals) {{
  const pending = proposals.filter((item) => item.state === "pending");
  document.querySelector("#accept-pack").disabled = pending.length === 0;
  proposalsEl.innerHTML = pending.length
    ? pending.map((item) => `<div><code>${{item.path}}</code> ${{item.summary}} <button data-accept="${{item.id}}">Accept</button> <button data-reject="${{item.id}}">Reject</button></div>`).join("")
    : "<p>No pending proposals</p>";
  proposalsEl.querySelectorAll("[data-accept]").forEach((button) => {{
    button.onclick = () => command("glass.editor.proposal.accept", {{id: button.dataset.accept}}, true);
  }});
  proposalsEl.querySelectorAll("[data-reject]").forEach((button) => {{
    button.onclick = () => command("glass.editor.proposal.reject", {{id: button.dataset.reject}}, true);
  }});
}}
async function command(name, argumentsValue={{}}, mutate=false) {{
  const response = await fetch(base + "v1/command", {{
    method:"POST", headers:{{"content-type":"application/json"}},
    body:JSON.stringify({{
      name,
      arguments: argumentsValue,
      allowMutation: mutate,
      confirmed: mutate,
      expectedGeneration: lastState.generation,
      expectedProjectRevision: lastState.projectRevision
    }})
  }});
  state.textContent = JSON.stringify(await response.json(), null, 2);
  await load();
}}
document.querySelector("#refresh").onclick = load;
document.querySelector("#review").onclick = () => command("glass.github.review");
document.querySelector("#accept-pack").onclick = () => command("glass.editor.proposal.accept_pack", {{}}, true);
load();
</script>"##,
        base
    )
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
        capsule.mobile_scroll = Some(20);
        ReconnectCapsuleStore::save(&capsule).unwrap();
        assert_eq!(ReconnectCapsuleStore::load(&root).unwrap(), Some(capsule));
        assert!(ReconnectCapsuleStore::clear(&root).unwrap());
        assert_eq!(ReconnectCapsuleStore::load(&root).unwrap(), None);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn reconnect_capsule_accepts_current_and_legacy_mobile_views() {
        let root = fixture("capsule-mobile-views");
        for view in [
            "home", "overview", "agent", "app", "browser", "diff", "project", "process", "logs",
        ] {
            let mut capsule = ReconnectCapsule::new(&root).unwrap();
            capsule.mobile_view = Some(view.into());
            capsule.validate().unwrap();
        }
        let mut capsule = ReconnectCapsule::new(&root).unwrap();
        capsule.mobile_view = Some("unknown".into());
        assert!(capsule.validate().is_err());
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
    fn local_cockpit_requires_token_and_serves_health() {
        let root = fixture("http");
        let workspace = crate::SharedDevelopmentWorkspace::open(&root).unwrap();
        let cockpit = LocalCockpit::start(workspace).unwrap();

        let mut unauthorized = TcpStream::connect(cockpit.address()).unwrap();
        unauthorized
            .write_all(b"GET /v1/health HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n")
            .unwrap();
        let _ = unauthorized.shutdown(std::net::Shutdown::Write);
        let mut unauthorized_response = Vec::new();
        unauthorized
            .read_to_end(&mut unauthorized_response)
            .unwrap();
        let unauthorized_response = String::from_utf8_lossy(&unauthorized_response);
        assert!(unauthorized_response.starts_with("HTTP/1.1 401 Unauthorized"));

        let request = format!(
            "GET /{}/v1/health HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n",
            cockpit.token()
        );
        let mut authorized = TcpStream::connect(cockpit.address()).unwrap();
        authorized.write_all(request.as_bytes()).unwrap();
        let _ = authorized.shutdown(std::net::Shutdown::Write);
        let mut authorized_response = Vec::new();
        authorized.read_to_end(&mut authorized_response).unwrap();
        let authorized_response = String::from_utf8_lossy(&authorized_response);
        assert!(authorized_response.starts_with("HTTP/1.1 200 OK"));
        assert!(authorized_response.contains("{\"ok\":true}"));
        drop(cockpit);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn local_cockpit_accepts_the_pending_proposal_pack() {
        let root = fixture("accept-pack");
        fs::write(root.join("src.rs"), "fn main() {}\n").unwrap();
        let workspace = crate::SharedDevelopmentWorkspace::open(&root).unwrap();
        {
            let mut locked = workspace.lock().unwrap();
            locked
                .apply_local_trust_decision(crate::LocalTrustDecision::TrustProject)
                .unwrap();
            locked
                .project_mut()
                .open_buffer("src.rs", Actor::local())
                .unwrap();
            locked
                .project_mut()
                .propose_editor_change(
                    "src.rs",
                    "fn main() {}\n".into(),
                    "fn main() { 1 }\n".into(),
                    "add one".into(),
                    Actor::local(),
                )
                .unwrap();
        }
        let cockpit = LocalCockpit::start(workspace.clone()).unwrap();

        let html_request = format!(
            "GET /{}/ HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n",
            cockpit.token()
        );
        let mut html_stream = TcpStream::connect(cockpit.address()).unwrap();
        html_stream.write_all(html_request.as_bytes()).unwrap();
        let _ = html_stream.shutdown(std::net::Shutdown::Write);
        let mut html = Vec::new();
        html_stream.read_to_end(&mut html).unwrap();
        let html = String::from_utf8_lossy(&html);
        assert!(html.contains("Accept pack"));
        assert!(html.contains("glass.editor.proposal.accept_pack"));

        let state_request = format!(
            "GET /{}/v1/state HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n",
            cockpit.token()
        );
        let mut state_stream = TcpStream::connect(cockpit.address()).unwrap();
        state_stream.write_all(state_request.as_bytes()).unwrap();
        let _ = state_stream.shutdown(std::net::Shutdown::Write);
        let mut state_body = Vec::new();
        state_stream.read_to_end(&mut state_body).unwrap();
        let state_body = String::from_utf8_lossy(&state_body);
        assert!(state_body.contains("\"path\":\"src.rs\""));
        assert!(state_body.contains("\"state\":\"pending\""));
        assert!(!state_body.contains("fn main() { 1 }"));

        let denied = serde_json::json!({"name":"glass.editor.proposal.accept_pack"}).to_string();
        let denied_request = format!(
            "POST /{}/v1/command HTTP/1.1\r\nHost: localhost\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{denied}",
            cockpit.token(),
            denied.len()
        );
        let mut denied_stream = TcpStream::connect(cockpit.address()).unwrap();
        denied_stream.write_all(denied_request.as_bytes()).unwrap();
        let _ = denied_stream.shutdown(std::net::Shutdown::Write);
        let mut denied_body = Vec::new();
        denied_stream.read_to_end(&mut denied_body).unwrap();
        let denied_body = String::from_utf8_lossy(&denied_body);
        assert!(denied_body.contains("409") || denied_body.contains("mutation authority"));

        let accepted = serde_json::json!({
            "name":"glass.editor.proposal.accept_pack",
            "allowMutation":true,
            "confirmed":true
        })
        .to_string();
        let accept_request = format!(
            "POST /{}/v1/command HTTP/1.1\r\nHost: localhost\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{accepted}",
            cockpit.token(),
            accepted.len()
        );
        let mut accept_stream = TcpStream::connect(cockpit.address()).unwrap();
        accept_stream.write_all(accept_request.as_bytes()).unwrap();
        let _ = accept_stream.shutdown(std::net::Shutdown::Write);
        let mut accept_body = Vec::new();
        accept_stream.read_to_end(&mut accept_body).unwrap();
        let accept_body = String::from_utf8_lossy(&accept_body);
        assert!(accept_body.starts_with("HTTP/1.1 200 OK"), "{accept_body}");

        let locked = workspace.lock().unwrap();
        assert_eq!(
            locked.project().buffer("src.rs").unwrap().content,
            "fn main() { 1 }\n"
        );
        assert_eq!(
            locked.project().editor_proposals()[0].state,
            crate::development::EditorProposalState::Accepted
        );
        drop(locked);
        drop(cockpit);
        let _ = fs::remove_dir_all(root);
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
