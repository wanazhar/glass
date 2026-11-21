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
use crate::tools::{ToolOutcome, ToolRouter};
use crate::worker::{CancelRegistry, now_millis};
use crate::{ApprovalInbox, Conversation, HostPolicy, MarbleError, TurnOptions, run_turn};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::{HashMap, HashSet};
use std::path::{Component, Path, PathBuf};
use std::process::Command;
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// Marbles one host keeps. A roster is meant to be scanned, not paged.
pub const PRISM_LIMIT: usize = 8;
/// Longest accepted marble name.
pub const NAME_LIMIT: usize = 40;
/// Longest accepted role line.
pub const ROLE_LIMIT: usize = 80;
/// Longest accepted personality text.
pub const PERSONA_LIMIT: usize = 2000;
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
    /// Who this marble is, in its own words: the personality its turns adopt.
    /// Falls back to the one-line `role` when empty.
    #[serde(default)]
    pub persona: String,
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
    /// The default assistant's own personality text, editable like a marble's.
    #[serde(default)]
    pub default_persona: String,
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
    #[allow(clippy::too_many_arguments)]
    pub fn add(
        &mut self,
        name: &str,
        role: &str,
        glyph: &str,
        hue: u16,
        workspace: &str,
        persona: &str,
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
            persona: valid_persona(persona)?,
        };
        self.marbles.push(marble);
        Ok(self
            .marbles
            .last()
            .expect("the marble was just pushed onto the roster"))
    }

    /// The default assistant as a roster entry: fixed id `marble`, fixed
    /// shape, editable personality. Its chats are the ordinary conversation
    /// list, not a marble-owned conversation.
    pub fn default_marble(&self) -> PrismMarble {
        PrismMarble {
            id: "marble".to_string(),
            name: "Marble".to_string(),
            role: "the default assistant".to_string(),
            glyph: GLYPHS[0].to_string(),
            hue: 210,
            workspace: String::new(),
            state: idle(),
            conversation_id: String::new(),
            task_id: String::new(),
            last_at: 0,
            runs: self.dispatched,
            last_error: String::new(),
            persona: self.default_persona.clone(),
        }
    }

    /// Change who a marble is without losing its conversation. Returns whether
    /// the identity the engine was told about changed: a new persona only
    /// reaches the model through a fresh conversation, so the caller drops the
    /// marble's engine when this reports `true`. The default assistant
    /// (`"marble"`) only takes a persona — its shape is fixed.
    #[allow(clippy::too_many_arguments)]
    pub fn update(
        &mut self,
        id: &str,
        name: Option<&str>,
        role: Option<&str>,
        glyph: Option<&str>,
        hue: Option<u16>,
        workspace: Option<&str>,
        persona: Option<&str>,
    ) -> Result<bool, String> {
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
        let persona = persona.map(valid_persona).transpose()?;
        if id == "marble" {
            if name.is_some()
                || role.is_some()
                || glyph.is_some()
                || hue.is_some()
                || workspace.is_some()
            {
                return Err("the default assistant only changes its personality".into());
            }
            let persona = persona.unwrap_or_default();
            let changed = self.default_persona != persona;
            self.default_persona = persona;
            return Ok(changed);
        }
        let mut identity_changed = false;
        let marble = self
            .find_mut(id)
            .ok_or_else(|| "unknown marble".to_string())?;
        if let Some(name) = name {
            identity_changed |= marble.name != name;
            marble.name = name;
        }
        if let Some(role) = role {
            marble.role = role;
        }
        if let Some(persona) = persona {
            identity_changed |= marble.persona != persona;
            marble.persona = persona;
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
        Ok(identity_changed)
    }

    /// Drop the conversation identity an edited marble was known by, keeping
    /// the transcript rows behind it. The next dispatch starts a fresh engine
    /// session that hears the new persona. The default assistant has no
    /// marble-owned conversation to drop.
    pub fn restart_conversation(&mut self, id: &str) {
        if let Some(marble) = self.find_mut(id) {
            marble.conversation_id.clear();
            marble.task_id.clear();
            marble.state = idle();
        }
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
        let mut marbles = vec![{
            let mut entry =
                serde_json::to_value(self.default_marble()).unwrap_or_else(|_| json!({}));
            entry["isDefault"] = json!(true);
            entry
        }];
        marbles.extend(self.marbles.iter().map(|marble| {
            let mut entry = serde_json::to_value(marble).unwrap_or_else(|_| json!({}));
            entry["isDefault"] = json!(false);
            entry
        }));
        json!({
            "marbles": marbles,
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

fn valid_persona(persona: &str) -> Result<String, String> {
    let persona = persona.trim();
    if persona.chars().count() > PERSONA_LIMIT {
        return Err(format!(
            "a personality is at most {PERSONA_LIMIT} characters"
        ));
    }
    Ok(persona.to_string())
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

/// One end of a marble-to-marble ask: the reply text or why there is none.
type AskWaiter = mpsc::Sender<Result<String, String>>;

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
    /// Marbles waiting on an ask: marble id → the channels answering that ask.
    ask_waiters: Mutex<HashMap<String, Vec<AskWaiter>>>,
    /// Depth of the turn each task is part of, so asks cannot chain forever.
    task_depths: Mutex<HashMap<String, u8>>,
    /// Depth waiting for a dispatched task to be accepted, keyed by marble id.
    pending_depths: Mutex<HashMap<String, u8>>,
    /// How many asks one turn has made, capped so a turn cannot fan out forever.
    ask_counts: Mutex<HashMap<String, usize>>,
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
            ask_waiters: Mutex::new(HashMap::new()),
            task_depths: Mutex::new(HashMap::new()),
            pending_depths: Mutex::new(HashMap::new()),
            ask_counts: Mutex::new(HashMap::new()),
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
        self.dispatch_inner(text, requested, None)
    }

    pub fn resume_held(self: &Arc<Self>) -> usize {
        let queued = self.lock_state().control.queue.clone();
        let mut seen = HashSet::new();
        let mut dispatched = 0;
        for entry in queued {
            let Some(id) = entry.marble_id else { continue };
            if seen.insert(id.clone())
                && self
                    .dispatch_inner(&entry.text, Some(&[id]), Some(&entry.id))
                    .is_ok()
            {
                dispatched += 1;
            }
        }
        dispatched
    }

    fn dispatch_inner(
        self: &Arc<Self>,
        text: &str,
        requested: Option<&[String]>,
        queue_id: Option<&str>,
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
            if let Some(id) = queue_id {
                if state.control.paused {
                    return Err(refuse(409, "admission_paused", "admission is paused"));
                }
                if !state.control.queue.iter().any(|entry| entry.id == id) {
                    return Err(refuse(
                        404,
                        "unknown_queue_entry",
                        "that message is no longer queued",
                    ));
                }
            }
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
            if let Some(id) = queue_id {
                state.control.queue.retain(|entry| entry.id != id);
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
        self: &Arc<Self>,
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
            .map_err(|error| format!("cannot open {}'s workspace: {error}", marble.name))?
            .with_ask({
                let pool = Arc::downgrade(self);
                move |scope, arguments| {
                    let Some(pool) = pool.upgrade() else {
                        return ToolOutcome::error("the marble pool is gone");
                    };
                    pool.ask(scope, arguments)
                }
            });
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
        let roster = {
            let state = self.lock_state();
            state.prism.marbles.clone()
        };
        let conversation = conversation
            .with_product(Arc::clone(&self.state))
            .with_instructions(crate::conversation::marble_session_instructions(
                marble, &roster, root,
            ));
        let engine = Arc::new(Mutex::new(conversation));
        let watched = Arc::downgrade(&engine);
        let state = Arc::downgrade(&self.state);
        std::thread::spawn(move || {
            loop {
                std::thread::sleep(Duration::from_secs(1));
                let (Some(engine), Some(state)) = (watched.upgrade(), state.upgrade()) else {
                    break;
                };
                if let Ok(mut conversation) = engine.try_lock() {
                    conversation.check_idle_transport(&state);
                }
            }
        });
        self.engines
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .insert(marble.id.clone(), Arc::clone(&engine));
        tracing::info!(marble = %marble.name, workspace = %root.display(), "started a prism engine");
        Ok(engine)
    }

    /// Seeds the ask depth for the next dispatch to this marble, then runs it.
    fn run_marble_at_depth(
        self: &Arc<Self>,
        marble: &PrismMarble,
        root: PathBuf,
        text: &str,
        depth: u8,
    ) {
        if depth > 0 {
            self.pending_depths
                .lock()
                .unwrap_or_else(|error| error.into_inner())
                .insert(marble.id.clone(), depth);
        }
        self.run_marble(marble, root, text);
    }

    /// Runs an ask from the marble that owns `scope`.
    pub fn ask(
        self: &Arc<Self>,
        scope: &crate::tools::ToolScope,
        arguments: &Value,
    ) -> ToolOutcome {
        const MAX_DEPTH: u8 = 2;
        const MAX_ASKS: usize = 3;
        const ASK_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(600);

        let task_id = scope.task_id.clone();
        let to = arguments.get("to").and_then(Value::as_str).unwrap_or("");
        let instruction = arguments
            .get("instruction")
            .and_then(Value::as_str)
            .unwrap_or("")
            .trim()
            .to_string();
        let (asker_id, asker_name, depth) = {
            let snapshot = self.lock_state();
            let asker = snapshot
                .prism
                .marbles
                .iter()
                .find(|marble| marble.task_id == task_id)
                .cloned();
            let depth = *self
                .task_depths
                .lock()
                .unwrap_or_else(|error| error.into_inner())
                .get(&task_id)
                .unwrap_or(&0);
            match asker {
                Some(asker) => (asker.id.clone(), asker.name.clone(), depth),
                None => (String::new(), "Marble".to_string(), depth),
            }
        };
        let mut entry = crate::state::HandoffEntry {
            from: asker_id.clone(),
            from_name: asker_name.clone(),
            to: String::new(),
            to_name: to.to_string(),
            instruction: instruction.clone(),
            status: "asked".into(),
            summary: String::new(),
        };
        let fail = |pool: &Arc<Self>,
                    reason: String,
                    status: &str,
                    entry: &crate::state::HandoffEntry|
         -> ToolOutcome {
            let mut entry = entry.clone();
            entry.status = status.into();
            entry.summary = reason.clone();
            pool.record_ask(&asker_id, &task_id, &scope.conversation_id, entry);
            ToolOutcome::error(reason)
        };
        if instruction.is_empty() {
            return fail(
                self,
                "an ask needs an instruction".to_string(),
                "failed",
                &entry,
            );
        }
        if depth >= MAX_DEPTH {
            return fail(
                self,
                "the request has already been passed along twice; give the answer directly"
                    .to_string(),
                "refused",
                &entry,
            );
        }
        let target = {
            let snapshot = self.lock_state();
            snapshot
                .prism
                .marbles
                .iter()
                .find(|marble| marble.name.eq_ignore_ascii_case(to))
                .cloned()
        };
        let Some(target) = target else {
            return fail(
                self,
                format!("there is no Marble named {to}"),
                "refused",
                &entry,
            );
        };
        entry.to = target.id.clone();
        entry.to_name = target.name.clone();
        if target.id == asker_id {
            return fail(
                self,
                "a Marble cannot ask itself".to_string(),
                "refused",
                &entry,
            );
        }
        let asks = {
            let mut counts = self
                .ask_counts
                .lock()
                .unwrap_or_else(|error| error.into_inner());
            let asks = counts.entry(task_id.clone()).or_insert(0);
            *asks += 1;
            *asks
        };
        if asks > MAX_ASKS {
            return fail(
                self,
                "this turn already asked three Marbles".to_string(),
                "refused",
                &entry,
            );
        }
        if target.state == "working" {
            return fail(
                self,
                format!("{} is busy with another task", target.name),
                "refused",
                &entry,
            );
        }
        let (sender, receiver) = mpsc::channel();
        self.ask_waiters
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .entry(target.id.clone())
            .or_default()
            .push(sender);
        self.record_ask(&asker_id, &task_id, &scope.conversation_id, entry.clone());
        let prompt = format!("{asker_name} asks: {instruction}");
        let root = self.workspace_root(&target).unwrap_or_default();
        let pool = Arc::clone(self);
        let spawned = std::thread::Builder::new()
            .name(format!("glass-marble-ask-{}", target.id))
            .spawn(move || pool.run_marble_at_depth(&target, root, &prompt, depth + 1));
        if let Err(error) = spawned {
            return fail(
                self,
                format!("could not start the turn: {error}"),
                "failed",
                &entry,
            );
        }
        match receiver.recv_timeout(ASK_TIMEOUT) {
            Ok(Ok(text)) => {
                let mut done = entry.clone();
                done.status = "finished".into();
                done.summary = text.clone();
                self.record_ask(&asker_id, &task_id, &scope.conversation_id, done);
                ToolOutcome::ok(text)
            }
            Ok(Err(reason)) => fail(self, reason, "failed", &entry),
            Err(_) => fail(
                self,
                format!("{} did not answer in ten minutes", entry.to),
                "failed",
                &entry,
            ),
        }
    }

    /// Writes a handoff line into the asker's transcript and into Everyone.
    fn record_ask(
        &self,
        asker_id: &str,
        task_id: &str,
        scope_conversation: &str,
        entry: crate::state::HandoffEntry,
    ) {
        let snapshot = {
            let mut state = self.lock_state();
            let conversation = if scope_conversation.is_empty() {
                state
                    .prism
                    .get(asker_id)
                    .map(|marble| marble.conversation_id.clone())
                    .unwrap_or_default()
            } else {
                scope_conversation.to_string()
            };
            crate::everyone::record_handoff(&mut state, &conversation, task_id, &entry);
            state.bump();
            state.clone()
        };
        self.persist(&snapshot);
        self.events.publish(
            "",
            "",
            "handoff",
            serde_json::json!({
                "from": entry.from,
                "to": entry.to,
                "status": entry.status,
            }),
        );
    }

    /// Run one marble's turn to settlement.
    fn run_marble(self: &Arc<Self>, marble: &PrismMarble, root: PathBuf, text: &str) {
        let engine = match self.engine(marble, &root) {
            Ok(engine) => engine,
            Err(error) => {
                self.block(&marble.id, &error);
                self.resume_held();
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
        let input = crate::normalize::engine_input(
            &self.lock_state(),
            Some(&conversation_id),
            &marble.id,
            text,
        );
        let turn = crate::MarbleTurn {
            conversation_id: (!conversation_id.is_empty()).then_some(conversation_id),
            request_id,
            input,
        };
        let cancel = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let mut on_accepted = |conversation_id: &str, task_id: &str| {
            accepted = Some((conversation_id.to_string(), task_id.to_string()));
            if let Some(depth) = self
                .pending_depths
                .lock()
                .unwrap_or_else(|error| error.into_inner())
                .remove(&marble.id)
            {
                self.task_depths
                    .lock()
                    .unwrap_or_else(|error| error.into_inner())
                    .insert(task_id.to_string(), depth);
            }
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
                    ..Default::default()
                });
                state.prism.note_task(&marble.id, conversation_id, task_id);
                crate::everyone::claim(&mut state, &format!("marble:{}", marble.id), task_id);
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
                self.task_depths
                    .lock()
                    .unwrap_or_else(|error| error.into_inner())
                    .remove(&reply.task_id);
                if let Some(waiters) = self
                    .ask_waiters
                    .lock()
                    .unwrap_or_else(|error| error.into_inner())
                    .remove(&marble.id)
                {
                    for waiter in waiters {
                        let _ = waiter.send(Ok(reply.text.clone()));
                    }
                }
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
                            ..Default::default()
                        });
                    }
                    state.prism.note_settled(&marble.id, status, "");
                    crate::everyone::mirror_settlement(
                        &mut state,
                        &reply.task_id,
                        &reply.text,
                        status,
                    );
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
                if let Some(waiters) = self
                    .ask_waiters
                    .lock()
                    .unwrap_or_else(|error| error.into_inner())
                    .remove(&marble.id)
                {
                    for waiter in waiters {
                        let _ = waiter.send(Err(error.to_string()));
                    }
                }
                let snapshot = {
                    let mut state = self.lock_state();
                    let now = now_millis();
                    if conversation.transport_failed() {
                        state
                            .presence
                            .runtime
                            .note_engine_failure(conversation.engine_id(), error.to_string());
                    }
                    if let Some((conversation_id, task_id)) = &accepted {
                        self.task_depths
                            .lock()
                            .unwrap_or_else(|error| error.into_inner())
                            .remove(task_id);
                        crate::everyone::mirror_settlement(&mut state, task_id, "", "failed");
                        self.cancels.forget(task_id);
                        state.set_task_status(task_id, "failed");
                        state.push_transcript(TranscriptEntry {
                            conversation_id: conversation_id.clone(),
                            kind: "system".into(),
                            text: format!("{} could not finish: {error}", marble.name),
                            task_id: task_id.clone(),
                            at: now,
                            ..Default::default()
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
        drop(guard);
        self.resume_held();
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
            .add("Scout", "reads the repo", "hex", 210, "", "")
            .unwrap();
        state
            .add("Forge", "writes the patch", "shield", 12, "src", "")
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
            state.add("scout", "", "circle", 0, "", "").unwrap_err(),
            "a marble is already named scout",
            "names are compared without case, so a roster cannot show two Scouts"
        );
        assert!(state.add("", "", "circle", 0, "", "").is_err());
        assert!(state.add("Nova", "", "triangle", 0, "", "").is_err());
        assert!(state.add("Nova", "", "star", 0, "../outside", "").is_err());
        assert!(state.add("Nova", "", "star", 0, "/etc", "").is_err());
    }

    #[test]
    fn the_roster_is_bounded_so_it_stays_scannable() {
        let mut state = PrismState::default();
        for index in 0..PRISM_LIMIT {
            state
                .add(&format!("marble {index}"), "", "circle", 0, "", "")
                .unwrap();
        }
        let error = state
            .add("one too many", "", "circle", 0, "", "")
            .unwrap_err();
        assert!(error.contains("at most"));
    }

    #[test]
    fn an_edit_validates_before_mutating_and_a_new_workspace_moves_the_marble() {
        let mut state = roster();
        let id = state.marbles[0].id.clone();
        state.marbles[0].conversation_id = "conv-1".into();
        assert!(
            state
                .update(&id, Some("forge"), None, None, None, None, None)
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
                None,
            )
            .unwrap();
        assert_eq!(state.marbles[0].name, "Lookout");
        assert_eq!(state.marbles[0].glyph, "drop");
        assert_eq!(state.marbles[0].hue, 300);
        assert_eq!(state.marbles[0].conversation_id, "conv-1");

        state
            .update(&id, None, None, None, None, Some("src/glass"), None)
            .unwrap();
        assert_eq!(state.marbles[0].workspace, "src/glass");
        assert!(
            state.marbles[0].conversation_id.is_empty(),
            "a marble that moves starts a conversation where it now works"
        );
        assert_eq!(state.remove(&id).unwrap(), "");
        assert!(
            state
                .update(&id, None, None, None, None, None, None)
                .is_err()
        );
    }

    #[test]
    fn a_persona_reaches_the_instructions_and_restarts_the_chat() {
        let mut state = PrismState::default();
        let ada = state
            .add("Ada", "writer", "star", 40, "", "dry wit, short sentences")
            .unwrap()
            .id
            .clone();
        state.add("Ben", "checker", "hex", 210, "", "").unwrap();
        let roster = state.marbles.clone();
        let marble = state.get(&ada).unwrap();
        let instructions = crate::conversation::marble_session_instructions(
            marble,
            &roster,
            std::path::Path::new("."),
        );
        assert!(instructions.contains("You are Ada"), "{instructions}");
        assert!(
            instructions.contains("dry wit, short sentences"),
            "{instructions}"
        );
        assert!(instructions.contains("Ben — checker"), "{instructions}");
        assert!(
            instructions.contains(crate::conversation::SESSION_INSTRUCTIONS),
            "the product posture still follows: {instructions}"
        );

        // An edited persona marks the identity changed, and restarting keeps
        // the transcript while dropping the conversation id the engine knew.
        state.marbles[0].conversation_id = "conv-1".into();
        assert!(
            state
                .update(&ada, None, None, None, None, None, Some("warmer"))
                .unwrap()
        );
        state.restart_conversation(&ada);
        assert!(state.get(&ada).unwrap().conversation_id.is_empty());
        assert_eq!(state.get(&ada).unwrap().persona, "warmer");
        assert!(
            !state
                .update(&ada, None, None, None, None, None, Some("warmer"))
                .unwrap(),
            "the same persona changes nothing"
        );
    }

    #[test]
    fn the_default_assistant_is_a_roster_entry_with_an_editable_persona() {
        let mut state = PrismState::default();
        let document = state.document();
        assert_eq!(document["marbles"][0]["id"], json!("marble"));
        assert_eq!(document["marbles"][0]["name"], json!("Marble"));
        assert_eq!(document["marbles"][0]["isDefault"], json!(true));

        assert!(
            state
                .update(
                    "marble",
                    None,
                    None,
                    None,
                    None,
                    None,
                    Some("calm and brief")
                )
                .unwrap()
        );
        assert_eq!(state.default_persona, "calm and brief");
        assert_eq!(
            state.document()["marbles"][0]["persona"],
            json!("calm and brief")
        );
        // Its name and shape are fixed.
        assert!(
            state
                .update("marble", Some("Nope"), None, None, None, None, None)
                .is_err()
        );
        // There is no marble-owned conversation to restart.
        state.restart_conversation("marble");
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
        assert_eq!(document["marbles"].as_array().unwrap().len(), 3);
        assert_eq!(document["swarms"], json!(3));
        assert_eq!(state.summary()["idle"], json!(1));
    }
}
