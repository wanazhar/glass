//! Prism protocol: several marbles working at once.
//!
//! One marble is one assistant, and one engine process runs one turn at a time.
//! That is the right shape for a person talking to an assistant, and the wrong
//! shape for handing the same question to four specialists at once. Prism is the
//! second shape: a roster of named marbles, each with its own role, glyph, colour,
//! workspace, conversation, and engine process, dispatched together like a hive
//! leaving the hive.
//!
//! What stays shared: the product document, the event bus, the approval inbox,
//! the cancel table, and the control layer. What is per marble: identity,
//! conversation, workspace root, and the engine process, because Core's runtime
//! protocol serialises turns inside one process and tells hosts to use separate
//! processes for concurrent agents.
//!
//! A marble is dispatched one turn at a time. Asking a marble that is already
//! working is refused rather than silently queued behind an engine nobody can
//! see, so presence never claims two things are happening in one place.

use crate::events::EventBus;
use crate::live::LiveStore;
use crate::state::{ProductState, TaskRecord, TranscriptEntry};
use crate::store::{self, StateStore};
use crate::tools::ToolRouter;
use crate::worker::{CancelRegistry, now_millis};
use crate::{
    ApprovalInbox, Conversation, HostPolicy, InboundMessage, MarbleError, TurnOptions, run_turn,
    to_turn_input,
};
use glass_runtime_client::protocol::ContextBlock;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::path::{Component, Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// Marbles one host keeps. A roster is meant to be scanned, not paged.
pub const PRISM_LIMIT: usize = 8;
/// Longest accepted marble name.
pub const NAME_LIMIT: usize = 40;
/// Longest accepted role line.
pub const ROLE_LIMIT: usize = 80;
/// Glyphs the roster offers. Simple shapes read at sidebar size; the eyes and
/// the state motion are what make a marble recognisable rather than the outline.
pub const GLYPHS: [&str; 6] = ["circle", "hex", "shield", "drop", "star", "square"];

/// One named marble in the roster.
///
/// Identity is deliberately small: a name, a role, a shape, and a hue. State
/// lives on the same object because a person watching a roster reads "who is
/// this" and "what are they doing" in one glance.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PrismMarble {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub role: String,
    /// One of [`GLYPHS`]; the surface falls back to `circle`.
    #[serde(default = "default_glyph")]
    pub glyph: String,
    /// Hue in degrees, so several marbles stay distinguishable at a glance.
    #[serde(default)]
    pub hue: u16,
    /// Workspace subdirectory this marble works in; empty means the host root.
    #[serde(default)]
    pub workspace: String,
    /// `idle`, `working`, `done`, or `blocked`.
    #[serde(default = "idle")]
    pub state: String,
    /// Durable conversation this marble keeps, so tomorrow is the same marble.
    #[serde(default)]
    pub conversation_id: String,
    /// Task of the turn it is running now, empty when it is not working.
    #[serde(default)]
    pub task_id: String,
    #[serde(default)]
    pub last_at: u64,
    #[serde(default)]
    pub runs: u64,
    /// Last refusal or failure, kept so a blocked marble explains itself.
    #[serde(default)]
    pub last_error: String,
}

fn default_glyph() -> String {
    GLYPHS[0].to_string()
}

fn idle() -> String {
    "idle".to_string()
}

impl PrismMarble {
    /// Whether this marble is running a turn right now.
    pub fn is_working(&self) -> bool {
        self.state == "working"
    }
}

/// Durable roster plus what the swarm has done so far.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PrismState {
    #[serde(default)]
    pub marbles: Vec<PrismMarble>,
    /// Dispatches that fanned out to more than one marble.
    #[serde(default)]
    pub swarms: u64,
    /// Individual marble turns admitted through Prism.
    #[serde(default)]
    pub dispatched: u64,
}

impl PrismState {
    pub fn get(&self, id: &str) -> Option<&PrismMarble> {
        self.marbles.iter().find(|marble| marble.id == id)
    }

    pub fn find_mut(&mut self, id: &str) -> Option<&mut PrismMarble> {
        self.marbles.iter_mut().find(|marble| marble.id == id)
    }

    /// The marble that keeps a conversation, if the roster owns it.
    ///
    /// This is what lets any surface continue a marble's chat: the engine that
    /// created a session is the only one that can resume it, so a message is
    /// routed to the owner rather than submitted to whichever engine happens to
    /// be serving the composer.
    pub fn owner_of(&self, conversation_id: &str) -> Option<&PrismMarble> {
        if conversation_id.is_empty() {
            return None;
        }
        self.marbles
            .iter()
            .find(|marble| marble.conversation_id == conversation_id)
    }

    /// Clear turns a previous process left looking busy.
    ///
    /// A dispatched marble is marked `working` before its engine starts, and the
    /// document is durable, so a crash mid-swarm used to leave that marble
    /// `working` forever — every later dispatch refused with "already working"
    /// and no way to clear it from a surface. On start each one becomes blocked
    /// with the reason, keeps its conversation, and is dispatchable again.
    pub fn recover_interrupted(&mut self) -> usize {
        let mut recovered = 0usize;
        for marble in &mut self.marbles {
            if !marble.is_working() {
                continue;
            }
            marble.state = "blocked".to_string();
            marble.task_id.clear();
            marble.last_error = format!(
                "{} stopped when the host restarted. Its conversation is kept; dispatch again to \
                 continue.",
                marble.name
            );
            marble.last_at = now_millis();
            recovered += 1;
        }
        recovered
    }

    /// Add one marble, refusing anything a roster cannot show legibly.
    pub fn add(
        &mut self,
        name: &str,
        role: &str,
        glyph: &str,
        hue: u16,
        workspace: &str,
    ) -> Result<&PrismMarble, String> {
        if self.marbles.len() >= PRISM_LIMIT {
            return Err(format!(
                "the roster holds at most {PRISM_LIMIT} marbles; remove one first"
            ));
        }
        let name = valid_name(name)?;
        let role = valid_role(role)?;
        if self
            .marbles
            .iter()
            .any(|marble| marble.name.eq_ignore_ascii_case(&name))
        {
            return Err(format!("a marble is already named {name}"));
        }
        let glyph = valid_glyph(glyph)?;
        let workspace = valid_workspace(workspace)?;
        let marble = PrismMarble {
            id: next_marble_id(),
            name,
            role,
            glyph,
            hue: hue.min(359),
            workspace,
            state: idle(),
            conversation_id: String::new(),
            task_id: String::new(),
            last_at: now_millis(),
            runs: 0,
            last_error: String::new(),
        };
        self.marbles.push(marble);
        Ok(self
            .marbles
            .last()
            .expect("the marble was just pushed onto the roster"))
    }

    /// Change who a marble is without losing its conversation.
    pub fn update(
        &mut self,
        id: &str,
        name: Option<&str>,
        role: Option<&str>,
        glyph: Option<&str>,
        hue: Option<u16>,
        workspace: Option<&str>,
    ) -> Result<(), String> {
        // Validate against the whole roster before mutating, so a bad edit
        // cannot leave the marble half-renamed.
        let name = match name {
            Some(name) => Some(valid_name(name)?),
            None => None,
        };
        if let Some(name) = &name
            && self
                .marbles
                .iter()
                .any(|marble| marble.id != id && marble.name.eq_ignore_ascii_case(name))
        {
            return Err(format!("a marble is already named {name}"));
        }
        let role = role.map(valid_role).transpose()?;
        let glyph = glyph.map(valid_glyph).transpose()?;
        let workspace = workspace.map(valid_workspace).transpose()?;
        let marble = self
            .find_mut(id)
            .ok_or_else(|| "unknown marble".to_string())?;
        if let Some(name) = name {
            marble.name = name;
        }
        if let Some(role) = role {
            marble.role = role;
        }
        if let Some(glyph) = glyph {
            marble.glyph = glyph;
        }
        if let Some(hue) = hue {
            marble.hue = hue.min(359);
        }
        if let Some(workspace) = workspace {
            // A different workspace is a different job, so the conversation does
            // not follow it: the next turn starts where the marble now works.
            if marble.workspace != workspace {
                marble.conversation_id.clear();
                marble.task_id.clear();
                marble.state = idle();
            }
            marble.workspace = workspace;
        }
        marble.last_at = now_millis();
        Ok(())
    }

    /// Remove one marble; returns the conversation it owned, if any.
    pub fn remove(&mut self, id: &str) -> Result<String, String> {
        let index = self
            .marbles
            .iter()
            .position(|marble| marble.id == id)
            .ok_or_else(|| "unknown marble".to_string())?;
        Ok(self.marbles.remove(index).conversation_id)
    }

    /// Mark one marble as running a turn. Callers validate first.
    fn mark_dispatched(&mut self, id: &str, conversation_id: &str) {
        if let Some(marble) = self.find_mut(id) {
            marble.state = "working".to_string();
            marble.task_id.clear();
            marble.last_at = now_millis();
            marble.runs = marble.runs.saturating_add(1);
            marble.last_error.clear();
            if !conversation_id.is_empty() {
                marble.conversation_id = conversation_id.to_string();
            }
            self.dispatched = self.dispatched.saturating_add(1);
        }
    }

    /// Record which durable task now carries one marble's turn.
    fn note_task(&mut self, id: &str, conversation_id: &str, task_id: &str) {
        if let Some(marble) = self.find_mut(id) {
            marble.conversation_id = conversation_id.to_string();
            marble.task_id = task_id.to_string();
            marble.state = "working".to_string();
            marble.last_at = now_millis();
        }
    }

    /// Settle one marble's turn.
    fn note_settled(&mut self, id: &str, status: &str, error: &str) {
        if let Some(marble) = self.find_mut(id) {
            marble.task_id.clear();
            marble.state = if status == "failed" {
                "blocked".to_string()
            } else {
                "done".to_string()
            };
            marble.last_at = now_millis();
            if !error.is_empty() {
                marble.last_error = error.chars().take(280).collect();
            }
        }
    }

    /// Roster document served by `/api/prism` and embedded in `/api/state`.
    pub fn document(&self) -> Value {
        json!({
            "marbles": self.marbles,
            "limit": PRISM_LIMIT,
            "glyphs": GLYPHS,
            "swarms": self.swarms,
            "dispatched": self.dispatched,
            "working": self.marbles.iter().filter(|m| m.is_working()).count(),
            "protocol": "prism",
        })
    }

    /// Small block for the presence document.
    pub fn summary(&self) -> Value {
        json!({
            "marbles": self.marbles.len(),
            "working": self.marbles.iter().filter(|m| m.is_working()).count(),
            "idle": self
                .marbles
                .iter()
                .filter(|m| !m.is_working() && m.state != "blocked")
                .count(),
            "blocked": self.marbles.iter().filter(|m| m.state == "blocked").count(),
            "swarms": self.swarms,
            "dispatched": self.dispatched,
        })
    }
}

fn next_marble_id() -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    static SEQUENCE: AtomicU64 = AtomicU64::new(0);
    let sequence = SEQUENCE.fetch_add(1, Ordering::Relaxed);
    format!("marble-{:x}-{sequence:x}", now_millis())
}

fn valid_name(name: &str) -> Result<String, String> {
    let name = name.trim();
    if name.is_empty() {
        return Err("a marble needs a name".into());
    }
    if name.chars().count() > NAME_LIMIT {
        return Err(format!("a name is at most {NAME_LIMIT} characters"));
    }
    if name.chars().any(char::is_control) {
        return Err("a name cannot contain control characters".into());
    }
    Ok(name.to_string())
}

fn valid_role(role: &str) -> Result<String, String> {
    let role = role.trim();
    if role.chars().count() > ROLE_LIMIT {
        return Err(format!("a role is at most {ROLE_LIMIT} characters"));
    }
    Ok(role.to_string())
}

fn valid_glyph(glyph: &str) -> Result<String, String> {
    let glyph = glyph.trim();
    if glyph.is_empty() {
        return Ok(default_glyph());
    }
    if !GLYPHS.contains(&glyph) {
        return Err(format!("unknown glyph {glyph}: use {}", GLYPHS.join(", ")));
    }
    Ok(glyph.to_string())
}

/// A workspace is a relative path inside the host workspace, never a way out.
fn valid_workspace(workspace: &str) -> Result<String, String> {
    let raw = workspace.trim();
    if raw.is_empty() {
        return Ok(String::new());
    }
    // Checked before trimming: an absolute path must be refused, not quietly
    // rewritten into a relative one that points somewhere else.
    if Path::new(raw).is_absolute() {
        return Err("a marble workspace is relative to the host workspace".into());
    }
    let workspace = raw.trim_end_matches('/');
    if workspace.is_empty() {
        return Ok(String::new());
    }
    let path = Path::new(workspace);
    for component in path.components() {
        match component {
            Component::Normal(_) | Component::CurDir => {}
            _ => return Err("a marble workspace may not leave the host workspace".into()),
        }
    }
    Ok(workspace.to_string())
}

/// Why a dispatch was refused, in the shape the HTTP surface reports.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DispatchRefusal {
    pub status: u16,
    pub code: &'static str,
    pub message: String,
}

fn refuse(status: u16, code: &'static str, message: impl Into<String>) -> DispatchRefusal {
    DispatchRefusal {
        status,
        code,
        message: message.into(),
    }
}

/// Check a dispatch against the roster before anything is started.
///
/// This is the whole admission decision, kept separate from the work so it can
/// be reasoned about — and tested — without an engine process.
pub fn plan_dispatch(
    state: &ProductState,
    text: &str,
    requested: Option<&[String]>,
) -> Result<Vec<PrismMarble>, DispatchRefusal> {
    if text.trim().is_empty() {
        return Err(refuse(400, "empty_prompt", "a dispatch needs a prompt"));
    }
    if state.control.paused {
        return Err(refuse(
            409,
            "admission_paused",
            "admission is paused; resume the queue before dispatching a swarm",
        ));
    }
    if state.prism.marbles.is_empty() {
        return Err(refuse(
            400,
            "no_marbles",
            "add a marble to the roster before dispatching",
        ));
    }
    let chosen: Vec<&PrismMarble> = match requested {
        Some(ids) if !ids.is_empty() => {
            let mut picked = Vec::with_capacity(ids.len());
            for id in ids {
                let marble = state
                    .prism
                    .get(id)
                    .ok_or_else(|| refuse(404, "unknown_marble", format!("unknown marble {id}")))?;
                picked.push(marble);
            }
            picked
        }
        _ => state.prism.marbles.iter().collect(),
    };
    for marble in &chosen {
        if marble.is_working() {
            return Err(refuse(
                409,
                "marble_busy",
                format!("{} is already working; let it finish", marble.name),
            ));
        }
    }
    Ok(chosen.into_iter().cloned().collect())
}

/// One dispatched marble turn, as reported to the surface that asked.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrismDispatch {
    pub ids: Vec<String>,
    pub names: Vec<String>,
    pub swarm: bool,
}

/// Effector for the HTTP layer: the pool only needs to say what it launched.
impl PrismDispatch {
    pub fn document(&self) -> Value {
        json!({
            "dispatched": self.ids,
            "marbles": self.names,
            "swarm": self.swarm,
        })
    }
}

/// Everything the pool needs to start a marble's engine.
pub struct PrismPoolParts {
    /// Recipe for one engine process, built fresh per marble.
    pub factory: Arc<dyn Fn() -> Command + Send + Sync>,
    /// Host workspace; a marble's own workspace is resolved inside it.
    pub cwd: PathBuf,
    /// Root for per-marble engine session directories.
    pub sessions: PathBuf,
    pub state: Arc<Mutex<ProductState>>,
    pub store: Option<Arc<StateStore>>,
    /// In-flight journal, so a swarm that dies mid-turn keeps its partial reply.
    pub live: LiveStore,
    pub events: EventBus,
    /// Shared with the turn worker, so one stop cancels a swarm too.
    pub cancels: CancelRegistry,
    pub approvals: ApprovalInbox,
    pub timeout: Duration,
}

/// Owns one engine process and one conversation per marble.
pub struct PrismPool {
    factory: Arc<dyn Fn() -> Command + Send + Sync>,
    cwd: PathBuf,
    sessions: PathBuf,
    state: Arc<Mutex<ProductState>>,
    store: Option<Arc<StateStore>>,
    live: LiveStore,
    events: EventBus,
    cancels: CancelRegistry,
    approvals: ApprovalInbox,
    timeout: Duration,
    engines: Mutex<HashMap<String, Arc<Mutex<Conversation>>>>,
}

impl std::fmt::Debug for PrismPool {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("PrismPool")
            .field("cwd", &self.cwd)
            .field("sessions", &self.sessions)
            .finish_non_exhaustive()
    }
}

impl PrismPool {
    /// Build the pool. Engines are started lazily on first dispatch, so a host
    /// that never swarms pays nothing for the roster.
    pub fn new(parts: PrismPoolParts) -> Self {
        Self {
            factory: parts.factory,
            cwd: parts.cwd,
            sessions: parts.sessions,
            state: parts.state,
            store: parts.store,
            live: parts.live,
            events: parts.events,
            cancels: parts.cancels,
            approvals: parts.approvals,
            timeout: parts.timeout,
            engines: Mutex::new(HashMap::new()),
        }
    }

    /// Absolute workspace root for one marble, refusing a missing directory.
    fn workspace_root(&self, marble: &PrismMarble) -> Result<PathBuf, String> {
        let root = if marble.workspace.is_empty() {
            self.cwd.clone()
        } else {
            self.cwd.join(&marble.workspace)
        };
        if !root.is_dir() {
            return Err(format!(
                "{} works in {}, which is not a directory",
                marble.name,
                root.display()
            ));
        }
        root.canonicalize()
            .map_err(|error| format!("cannot open {}'s workspace: {error}", marble.name))
    }

    /// Admit a dispatch and start each marble on its own thread.
    pub fn dispatch(
        self: &Arc<Self>,
        text: &str,
        requested: Option<&[String]>,
    ) -> Result<PrismDispatch, DispatchRefusal> {
        let targets = {
            let state = self.lock_state();
            plan_dispatch(&state, text, requested)?
        };
        // Workspaces are checked before the roster is marked busy, so a typo
        // does not leave a marble looking like it is working.
        let mut planned = Vec::with_capacity(targets.len());
        for marble in &targets {
            match self.workspace_root(marble) {
                Ok(root) => planned.push((marble.clone(), root)),
                Err(error) => return Err(refuse(409, "workspace_unavailable", error)),
            }
        }
        let prompt = text.trim().to_string();
        let swarm = planned.len() > 1;
        let mut ids = Vec::with_capacity(planned.len());
        let mut names = Vec::with_capacity(planned.len());
        let snapshot = {
            let mut state = self.lock_state();
            // Checked again under the same lock that marks the roster, so two
            // dispatches arriving together cannot both claim one marble.
            for (marble, _) in &planned {
                if state
                    .prism
                    .get(&marble.id)
                    .is_some_and(|current| current.is_working())
                {
                    return Err(refuse(
                        409,
                        "marble_busy",
                        format!("{} is already working; let it finish", marble.name),
                    ));
                }
            }
            for (marble, _) in &planned {
                state
                    .prism
                    .mark_dispatched(&marble.id, &marble.conversation_id);
                ids.push(marble.id.clone());
                names.push(marble.name.clone());
            }
            if swarm {
                state.prism.swarms = state.prism.swarms.saturating_add(1);
            }
            state.bump();
            state.clone()
        };
        self.persist(&snapshot);
        self.events.publish(
            "",
            "",
            "prism",
            json!({
                "dispatched": ids,
                "swarm": swarm,
                "revision": snapshot.revision,
            }),
        );
        for (marble, root) in planned {
            let pool = Arc::clone(self);
            let prompt = prompt.clone();
            let marble_id = marble.id.clone();
            let marble_name = marble.name.clone();
            let spawned = std::thread::Builder::new()
                .name(format!("glass-marble-prism-{marble_id}"))
                .spawn(move || pool.run_marble(&marble, root, &prompt));
            if let Err(error) = spawned {
                tracing::error!(%error, marble = %marble_name, "could not start a prism turn");
                self.block(&marble_id, &format!("could not start the turn: {error}"));
            }
        }
        Ok(PrismDispatch { ids, names, swarm })
    }

    fn lock_state(&self) -> std::sync::MutexGuard<'_, ProductState> {
        self.state.lock().unwrap_or_else(|error| error.into_inner())
    }

    fn persist(&self, snapshot: &ProductState) {
        if let Some(store) = &self.store {
            store::persist(store, snapshot);
        }
    }

    /// Engine for one marble, started on first use.
    fn engine(
        &self,
        marble: &PrismMarble,
        root: &Path,
    ) -> Result<Arc<Mutex<Conversation>>, String> {
        if let Some(engine) = self
            .engines
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .get(&marble.id)
            .cloned()
        {
            return Ok(engine);
        }
        let router = ToolRouter::new(root)
            .map_err(|error| format!("cannot open {}'s workspace: {error}", marble.name))?;
        let sessions = self.sessions.join(&marble.id);
        std::fs::create_dir_all(&sessions).map_err(|error| {
            format!("cannot create {}'s session directory: {error}", marble.name)
        })?;
        // The command is built per marble so nothing about one engine leaks into
        // another, and `current_dir` is set again by the conversation.
        let mut command = (self.factory)();
        command.current_dir(root);
        let conversation = Conversation::spawn_with_router(
            command,
            root,
            &sessions,
            HostPolicy::asking(),
            self.approvals.clone(),
            router,
            self.events.clone(),
        )
        .map_err(|error| format!("{} could not start its engine: {error}", marble.name))?;
        let conversation = conversation.with_product(Arc::clone(&self.state));
        let engine = Arc::new(Mutex::new(conversation));
        self.engines
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .insert(marble.id.clone(), Arc::clone(&engine));
        tracing::info!(marble = %marble.name, workspace = %root.display(), "started a prism engine");
        Ok(engine)
    }

    /// Run one marble's turn to settlement.
    fn run_marble(&self, marble: &PrismMarble, root: PathBuf, text: &str) {
        let engine = match self.engine(marble, &root) {
            Ok(engine) => engine,
            Err(error) => {
                self.block(&marble.id, &error);
                return;
            }
        };
        let mut guard = engine.lock().unwrap_or_else(|error| error.into_inner());
        let conversation = &mut *guard;
        let conversation_id = {
            let state = self.lock_state();
            state
                .prism
                .get(&marble.id)
                .map(|m| m.conversation_id.clone())
                .unwrap_or_default()
        };
        let mut accepted: Option<(String, String)> = None;
        let request_id = crate::next_request_id();
        let mut live_turn = self
            .live
            .begin(text, &marble.name, &conversation_id, &request_id);
        let input = self.turn_input(text);
        let turn = crate::MarbleTurn {
            conversation_id: (!conversation_id.is_empty()).then_some(conversation_id),
            request_id,
            input,
        };
        let cancel = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let mut on_accepted = |conversation_id: &str, task_id: &str| {
            accepted = Some((conversation_id.to_string(), task_id.to_string()));
            // The stop switch reaches a swarm exactly like a typed turn: the
            // worker's cancel table is the one place that knows every task id.
            self.cancels.register(task_id, Arc::clone(&cancel));
            let snapshot = {
                let mut state = self.lock_state();
                let now = now_millis();
                let title = truncate(text, 64);
                let record = state.conversation_mut(conversation_id);
                if record.title.is_empty() {
                    record.title = format!("{} · {title}", marble.name);
                }
                record.updated_at = now;
                if !record.task_ids.iter().any(|id| id == task_id) {
                    record.task_ids.push(task_id.to_string());
                }
                state.upsert_task(TaskRecord {
                    id: task_id.to_string(),
                    conversation_id: conversation_id.to_string(),
                    status: "running".into(),
                    owner: "prism".into(),
                    prompt: text.to_string(),
                    at: now,
                    routine_id: String::new(),
                });
                state.push_transcript(TranscriptEntry {
                    conversation_id: conversation_id.to_string(),
                    kind: "user".into(),
                    text: text.to_string(),
                    task_id: task_id.to_string(),
                    at: now,
                });
                state.prism.note_task(&marble.id, conversation_id, task_id);
                state.bump();
                state.clone()
            };
            self.persist(&snapshot);
            self.events.publish(
                conversation_id,
                task_id,
                "state",
                json!({ "revision": snapshot.revision, "status": "running", "marble": marble.id }),
            );
        };
        // A dispatched turn is cancellable from the control panel exactly like a
        // typed one, because it lands in the same registry.
        let result = run_turn(
            &turn,
            conversation,
            TurnOptions {
                cancel: Some(cancel.as_ref()),
                timeout: self.timeout,
                on_accepted: Some(&mut on_accepted),
                live: live_turn.as_mut(),
            },
        );
        match result {
            Ok(reply) => {
                self.cancels.forget(&reply.task_id);
                let snapshot = {
                    let mut state = self.lock_state();
                    let now = now_millis();
                    let status = if reply.cancelled {
                        "cancelled"
                    } else {
                        "completed"
                    };
                    state.set_task_status(&reply.task_id, status);
                    state.conversation_mut(&reply.conversation_id).updated_at = now;
                    if !reply.text.trim().is_empty() {
                        state.push_transcript(TranscriptEntry {
                            conversation_id: reply.conversation_id.clone(),
                            kind: "assistant".into(),
                            text: reply.text.clone(),
                            task_id: reply.task_id.clone(),
                            at: now,
                        });
                    }
                    state.prism.note_settled(&marble.id, status, "");
                    state.bump();
                    state.clone()
                };
                self.persist(&snapshot);
                self.events.publish(
                    &reply.conversation_id,
                    &reply.task_id,
                    "state",
                    json!({
                        "revision": snapshot.revision,
                        "status": if reply.cancelled { "cancelled" } else { "completed" },
                        "marble": marble.id,
                    }),
                );
            }
            Err(error) => {
                let snapshot = {
                    let mut state = self.lock_state();
                    let now = now_millis();
                    if let Some((conversation_id, task_id)) = &accepted {
                        self.cancels.forget(task_id);
                        state.set_task_status(task_id, "failed");
                        state.push_transcript(TranscriptEntry {
                            conversation_id: conversation_id.clone(),
                            kind: "system".into(),
                            text: format!("{} could not finish: {error}", marble.name),
                            task_id: task_id.clone(),
                            at: now,
                        });
                        state.conversation_mut(conversation_id).updated_at = now;
                    }
                    state
                        .prism
                        .note_settled(&marble.id, "failed", &error.to_string());
                    state.bump();
                    state.clone()
                };
                self.persist(&snapshot);
                self.events.publish(
                    "",
                    "",
                    "prism",
                    json!({
                        "marble": marble.id,
                        "status": "failed",
                        "message": error.to_string(),
                        "revision": snapshot.revision,
                    }),
                );
            }
        }
    }

    /// Leave a marble visibly blocked, with the reason it could not work.
    fn block(&self, id: &str, error: &str) {
        let snapshot = {
            let mut state = self.lock_state();
            state.prism.note_settled(id, "failed", error);
            state.bump();
            state.clone()
        };
        self.persist(&snapshot);
        self.events.publish(
            "",
            "",
            "prism",
            json!({ "marble": id, "status": "failed", "message": error }),
        );
    }

    /// Stop every prism engine, used when a marble leaves the roster.
    pub fn shutdown_marble(&self, id: &str) {
        let engine = self
            .engines
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .remove(id);
        if let Some(engine) = engine {
            let mut guard = engine.lock().unwrap_or_else(|error| error.into_inner());
            let _ = guard.shutdown();
        }
    }

    /// Stop every marble engine, so a host shutdown does not leave orphans.
    pub fn shutdown(&self) {
        let engines: Vec<(String, Arc<Mutex<Conversation>>)> = {
            let mut map = self
                .engines
                .lock()
                .unwrap_or_else(|error| error.into_inner());
            map.drain().collect()
        };
        for (id, engine) in engines {
            let mut guard = engine.lock().unwrap_or_else(|error| error.into_inner());
            if let Err(error) = guard.shutdown() {
                tracing::warn!(marble = %id, %error, "could not stop a prism engine");
            }
        }
    }

    /// Carried into every prism turn, so a marble uses what was remembered.
    fn turn_input(&self, text: &str) -> glass_runtime_client::TurnInput {
        let mut input = to_turn_input(&InboundMessage::new(text));
        let block = {
            let state = self.lock_state();
            state.memory.context_block()
        };
        if let Some(block) = block {
            input.context.push(ContextBlock::Text { text: block });
        }
        input
    }
}

fn truncate(text: &str, limit: usize) -> String {
    let trimmed = text.trim();
    if trimmed.chars().count() <= limit {
        return trimmed.to_string();
    }
    let mut title: String = trimmed.chars().take(limit).collect();
    title.push('…');
    title
}

/// Errors from a prism turn are stringly, because they cross a thread boundary
/// into the roster rather than into `Result`-returning product code.
impl From<MarbleError> for DispatchRefusal {
    fn from(error: MarbleError) -> Self {
        refuse(500, "prism_failed", error.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::control::ControlState;

    fn roster() -> PrismState {
        let mut state = PrismState::default();
        state
            .add("Scout", "reads the repo", "hex", 210, "")
            .unwrap();
        state
            .add("Forge", "writes the patch", "shield", 12, "src")
            .unwrap();
        state
    }

    #[test]
    fn a_roster_names_marble_and_refuses_duplicates() {
        let mut state = roster();
        assert_eq!(state.marbles.len(), 2);
        assert_eq!(state.marbles[0].name, "Scout");
        assert_eq!(state.marbles[0].state, "idle");
        assert_eq!(
            state.add("scout", "", "circle", 0, "").unwrap_err(),
            "a marble is already named scout",
            "names are compared without case, so a roster cannot show two Scouts"
        );
        assert!(state.add("", "", "circle", 0, "").is_err());
        assert!(state.add("Nova", "", "triangle", 0, "").is_err());
        assert!(state.add("Nova", "", "star", 0, "../outside").is_err());
        assert!(state.add("Nova", "", "star", 0, "/etc").is_err());
    }

    #[test]
    fn the_roster_is_bounded_so_it_stays_scannable() {
        let mut state = PrismState::default();
        for index in 0..PRISM_LIMIT {
            state
                .add(&format!("marble {index}"), "", "circle", 0, "")
                .unwrap();
        }
        let error = state.add("one too many", "", "circle", 0, "").unwrap_err();
        assert!(error.contains("at most"));
    }

    #[test]
    fn an_edit_validates_before_mutating_and_a_new_workspace_moves_the_marble() {
        let mut state = roster();
        let id = state.marbles[0].id.clone();
        state.marbles[0].conversation_id = "conv-1".into();
        assert!(
            state
                .update(&id, Some("forge"), None, None, None, None)
                .is_err()
        );
        assert_eq!(
            state.marbles[0].name, "Scout",
            "a refused rename changes nothing"
        );
        state
            .update(
                &id,
                Some("Lookout"),
                Some("scans"),
                Some("drop"),
                Some(300),
                None,
            )
            .unwrap();
        assert_eq!(state.marbles[0].name, "Lookout");
        assert_eq!(state.marbles[0].glyph, "drop");
        assert_eq!(state.marbles[0].hue, 300);
        assert_eq!(state.marbles[0].conversation_id, "conv-1");

        state
            .update(&id, None, None, None, None, Some("src/glass"))
            .unwrap();
        assert_eq!(state.marbles[0].workspace, "src/glass");
        assert!(
            state.marbles[0].conversation_id.is_empty(),
            "a marble that moves starts a conversation where it now works"
        );
        assert_eq!(state.remove(&id).unwrap(), "");
        assert!(state.update(&id, None, None, None, None, None).is_err());
    }

    #[test]
    fn a_dispatch_needs_a_prompt_a_roster_and_idle_marbles() {
        let mut product = ProductState::default();
        assert_eq!(
            plan_dispatch(&product, "go", None).unwrap_err().code,
            "no_marbles"
        );
        product = ProductState {
            prism: roster(),
            ..ProductState::default()
        };
        assert_eq!(
            plan_dispatch(&product, "   ", None).unwrap_err().code,
            "empty_prompt"
        );
        let planned = plan_dispatch(&product, "sweep the notes", None).unwrap();
        assert_eq!(planned.len(), 2);

        let only = plan_dispatch(
            &product,
            "sweep",
            Some(&[product.prism.marbles[1].id.clone()]),
        )
        .unwrap();
        assert_eq!(only.len(), 1);
        assert_eq!(only[0].name, "Forge");

        let unknown = plan_dispatch(&product, "sweep", Some(&["marble-nope".to_string()]));
        assert_eq!(unknown.unwrap_err().code, "unknown_marble");

        product.control = ControlState {
            paused: true,
            ..ControlState::default()
        };
        assert_eq!(
            plan_dispatch(&product, "sweep", None).unwrap_err().code,
            "admission_paused"
        );
    }

    #[test]
    fn a_busy_marble_is_refused_instead_of_silently_queued() {
        let mut product = ProductState {
            prism: roster(),
            ..ProductState::default()
        };
        let id = product.prism.marbles[0].id.clone();
        product.prism.mark_dispatched(&id, "");
        let refusal = plan_dispatch(&product, "sweep", None).unwrap_err();
        assert_eq!(refusal.code, "marble_busy");
        assert!(refusal.message.contains("Scout"));
        assert_eq!(product.prism.summary()["working"], json!(1));
    }

    #[test]
    fn a_settled_marble_is_ready_again_and_keeps_its_conversation() {
        let mut state = roster();
        let id = state.marbles[0].id.clone();
        state.mark_dispatched(&id, "conv-1");
        state.note_task(&id, "conv-1", "task-1");
        assert_eq!(state.get(&id).unwrap().task_id, "task-1");
        state.note_settled(&id, "completed", "");
        let marble = state.get(&id).unwrap();
        assert_eq!(marble.state, "done");
        assert!(marble.task_id.is_empty());
        assert_eq!(marble.conversation_id, "conv-1");
        assert_eq!(marble.runs, 1);

        state.note_settled(&id, "failed", "engine went away");
        assert_eq!(state.get(&id).unwrap().state, "blocked");
        assert_eq!(state.get(&id).unwrap().last_error, "engine went away");
    }

    #[test]
    fn a_marble_caught_mid_turn_restarts_blocked_but_ready() {
        let mut state = roster();
        let id = state.marbles[0].id.clone();
        state.mark_dispatched(&id, "conv-1");
        state.note_task(&id, "conv-1", "task-1");
        assert!(state.get(&id).unwrap().is_working());

        assert_eq!(state.recover_interrupted(), 1);
        let marble = state.get(&id).unwrap();
        assert_eq!(marble.state, "blocked");
        assert!(marble.task_id.is_empty());
        assert!(
            !marble.is_working(),
            "a restart must not leave a marble busy"
        );
        assert!(
            marble.last_error.contains("restarted"),
            "{}",
            marble.last_error
        );
        assert_eq!(
            marble.conversation_id, "conv-1",
            "the conversation survives the crash"
        );
        assert_eq!(state.recover_interrupted(), 0, "already recovered");

        // The marble is dispatchable again, so a swarm after a restart works.
        let mut product = ProductState {
            prism: state,
            ..ProductState::default()
        };
        product.prism.marbles[1].state = "idle".into();
        let planned = plan_dispatch(&product, "continue", Some(&[id])).unwrap();
        assert_eq!(planned.len(), 1);
    }

    #[test]
    fn a_conversation_names_the_marble_that_owns_it() {
        let mut state = roster();
        let id = state.marbles[0].id.clone();
        state.note_task(&id, "conv-1", "task-1");
        assert_eq!(
            state.owner_of("conv-1").map(|m| m.name.as_str()),
            Some("Scout")
        );
        assert!(state.owner_of("conv-2").is_none());
        assert!(state.owner_of("").is_none());
    }

    #[test]
    fn the_roster_document_reports_what_a_surface_needs() {
        let mut state = roster();
        let id = state.marbles[0].id.clone();
        state.mark_dispatched(&id, "");
        state.swarms = 3;
        let document = state.document();
        assert_eq!(document["protocol"], json!("prism"));
        assert_eq!(document["limit"], json!(PRISM_LIMIT));
        assert_eq!(document["working"], json!(1));
        assert_eq!(document["marbles"].as_array().unwrap().len(), 2);
        assert_eq!(document["swarms"], json!(3));
        assert_eq!(state.summary()["idle"], json!(1));
    }
}
