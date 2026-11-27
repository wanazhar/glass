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
use crate::threads::{GROUP_CONVERSATION, GroupAction, GroupThread, MemberSuggestion, Thread};
use crate::tools::{
    AgentTools, MARBLE_ASK, ToolOutcome, ToolRouter, WORKSPACE_LIST, WORKSPACE_READ,
    WORKSPACE_SEARCH, WORKSPACE_WRITE,
};
use crate::worker::{CancelRegistry, now_millis};
use crate::{
    ApprovalInbox, Conversation, HostPolicy, MarbleError, SteerRequest, TurnOptions, run_turn,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::{HashMap, HashSet};
use std::path::{Component, Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// Marbles one host keeps. A roster is meant to be scanned, not paged.
pub const PRISM_LIMIT: usize = 32;
/// Longest accepted marble name.
pub const NAME_LIMIT: usize = 40;
/// Longest accepted role line.
pub const ROLE_LIMIT: usize = 80;
/// Longest accepted personality text.
pub const PERSONA_LIMIT: usize = 2000;
/// Glyphs the roster offers. Simple shapes read at sidebar size; the eyes and
/// the state motion are what make a marble recognisable rather than the outline.
pub const GLYPHS: [&str; 6] = ["circle", "hex", "shield", "drop", "star", "square"];

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum WhenToSpeak {
    Mentioned,
    #[default]
    Relevant,
    Always,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct AgentCard {
    pub voice: String,
    pub model: String,
    pub when_to_speak: WhenToSpeak,
    pub tools: AgentTools,
    pub in_everyone: bool,
    pub tool_call_limit: Option<usize>,
    pub turn_timeout_secs: Option<u64>,
}

fn role_prompt(name: &str, focus: &str) -> String {
    format!(
        "You are {name}. {focus} Stay in your role and answer briefly by default. Reply to small talk without tools."
    )
}

fn template_card(
    when_to_speak: WhenToSpeak,
    native: bool,
    tools: &[&str],
    engineer: bool,
) -> AgentCard {
    AgentCard {
        when_to_speak,
        tools: AgentTools {
            native,
            allowed: tools.iter().map(|name| (*name).into()).collect(),
        },
        in_everyone: true,
        tool_call_limit: Some(if engineer { 25 } else { 8 }),
        turn_timeout_secs: Some(if engineer { 600 } else { 120 }),
        ..Default::default()
    }
}

impl AgentCard {
    pub(crate) fn validate(&mut self) -> Result<(), String> {
        self.tools.validate()?;
        self.model = self.model.trim().to_owned();
        self.voice = self.voice.trim().to_owned();
        if self.model.chars().count() > 180 || self.voice.chars().count() > PERSONA_LIMIT {
            return Err("agent model or voice is too long".into());
        }
        if self.tool_call_limit == Some(0) || self.turn_timeout_secs == Some(0) {
            return Err("turn budgets must be positive".into());
        }
        Ok(())
    }
}

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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_call_limit: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub turn_timeout_secs: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub card: Option<AgentCard>,
}

fn default_glyph() -> String {
    GLYPHS[0].to_string()
}

fn idle() -> String {
    "idle".to_string()
}

impl PrismMarble {
    pub(crate) fn turn_budget(&self) -> (usize, Duration) {
        let engineer = self.role.eq_ignore_ascii_case("engineer")
            || self.name.eq_ignore_ascii_case("engineer");
        let tools = self
            .card
            .as_ref()
            .and_then(|card| card.tool_call_limit)
            .or(self.tool_call_limit)
            .filter(|limit| *limit > 0)
            .unwrap_or(if engineer {
                25
            } else {
                crate::turn::DEFAULT_TOOL_LIMIT
            });
        let seconds = self
            .card
            .as_ref()
            .and_then(|card| card.turn_timeout_secs)
            .or(self.turn_timeout_secs)
            .filter(|seconds| *seconds > 0)
            .unwrap_or(if engineer {
                600
            } else {
                crate::DEFAULT_TURN_TIMEOUT.as_secs()
            });
        (tools, Duration::from_secs(seconds))
    }

    /// Whether this marble is running a turn right now.
    pub fn is_working(&self) -> bool {
        self.state == "working"
    }

    pub(crate) fn in_everyone(&self) -> bool {
        self.card.as_ref().is_none_or(|card| card.in_everyone)
    }

    pub(crate) fn model(&self, fallback: &str) -> Option<String> {
        let model = self
            .card
            .as_ref()
            .map(|card| card.model.as_str())
            .filter(|model| !model.is_empty())
            .unwrap_or(fallback)
            .trim();
        (!model.is_empty()).then(|| model.to_owned())
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_card: Option<AgentCard>,
    #[serde(default)]
    template_installed: bool,
}

impl PrismState {
    pub fn install_template(&mut self) -> bool {
        if self.template_installed && self.get("lead").is_some() {
            return false;
        }
        for agent in &mut self.marbles {
            if agent.card.is_none() {
                agent.card = Some(AgentCard {
                    tools: AgentTools {
                        native: true,
                        allowed: ToolRouter::catalog()
                            .into_iter()
                            .map(|tool| tool.name)
                            .collect(),
                    },
                    in_everyone: true,
                    ..Default::default()
                });
            }
        }
        let reads = &[WORKSPACE_LIST, WORKSPACE_READ, WORKSPACE_SEARCH];
        let write = &[
            WORKSPACE_LIST,
            WORKSPACE_READ,
            WORKSPACE_SEARCH,
            WORKSPACE_WRITE,
        ];
        if self.default_persona.is_empty() {
            self.default_persona = role_prompt(
                "the generalist companion",
                "Answer everyday questions and handle light tasks. Ask one clarifying question when it would prevent a wrong answer.",
            );
        }
        self.default_card
            .get_or_insert_with(|| template_card(WhenToSpeak::Relevant, true, write, false));
        for (id, name, role, glyph, hue, focus, speak, native, tools) in [
            (
                "lead",
                "Lead",
                "Plans work and closes group threads",
                "hex",
                42,
                "Set the order, give each specialist a distinct assignment, and close with the decision and what is done. Suggest rather than add members.",
                WhenToSpeak::Always,
                false,
                &[MARBLE_ASK][..],
            ),
            (
                "engineer",
                "Engineer",
                "Writes code and fixes defects",
                "square",
                145,
                "Read the relevant code before changing it, make focused fixes, and report which tests actually ran.",
                WhenToSpeak::Relevant,
                true,
                write,
            ),
            (
                "designer",
                "Designer",
                "Designs and reviews interfaces",
                "drop",
                265,
                "Review hierarchy, accessibility and interaction. Offer concrete visual improvements and state which screens you actually checked.",
                WhenToSpeak::Relevant,
                true,
                reads,
            ),
            (
                "researcher",
                "Researcher",
                "Finds and checks sources",
                "star",
                32,
                "Prefer primary sources, verify claims across sources and distinguish evidence from inference. Cite sources you actually checked.",
                WhenToSpeak::Relevant,
                true,
                reads,
            ),
            (
                "reviewer",
                "Reviewer",
                "Reviews work and reports what was verified",
                "shield",
                152,
                "Look for concrete regressions and missing tests. Do not edit the work; state what you verified and what remains uncertain.",
                WhenToSpeak::Relevant,
                false,
                reads,
            ),
            (
                "finance",
                "Finance",
                "Reviews costs, budgets and spending",
                "hex",
                44,
                "Show units and assumptions, distinguish estimates from actual costs, and do not make purchases or financial commitments.",
                WhenToSpeak::Mentioned,
                false,
                reads,
            ),
            (
                "shopper",
                "Shopper",
                "Finds and compares products",
                "drop",
                277,
                "Compare a small number of products against the user's constraints. State price, currency and uncertainty. Never place an order.",
                WhenToSpeak::Mentioned,
                true,
                reads,
            ),
        ] {
            if self.get(id).is_some() || (self.template_installed && id != "lead") {
                continue;
            }
            let taken = |name: &str| {
                self.marbles
                    .iter()
                    .any(|agent| agent.name.eq_ignore_ascii_case(name))
            };
            let name = if !taken(name) {
                name.to_string()
            } else if id != "lead" {
                continue;
            } else {
                std::iter::once("Lead (built-in)".to_string())
                    .chain((2..).map(|number| format!("Lead (built-in {number})")))
                    .find(|name| !taken(name))
                    .expect("the roster is finite")
            };
            if self
                .add(&name, role, glyph, hue, "", &role_prompt(&name, focus))
                .is_ok()
            {
                let agent = self.marbles.last_mut().expect("just added");
                agent.id = id.into();
                agent.card = Some(template_card(speak, native, tools, id == "engineer"));
            }
        }
        self.template_installed = true;
        true
    }

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
            tool_call_limit: None,
            turn_timeout_secs: None,
            card: None,
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
            role: "The default assistant".to_string(),
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
            tool_call_limit: None,
            turn_timeout_secs: None,
            card: self.default_card.clone(),
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
            let changed = persona
                .as_ref()
                .is_some_and(|persona| self.default_persona != *persona);
            if let Some(persona) = persona {
                self.default_persona = persona;
            }
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
        if id == "lead" {
            return Err("Lead is required in group chats".into());
        }
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
            if !conversation_id.is_empty() && !crate::everyone::is_session(conversation_id) {
                marble.conversation_id = conversation_id.to_string();
            }
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

#[derive(Default)]
struct GroupLane {
    running: Mutex<()>,
    latest: Mutex<Option<Arc<AtomicBool>>>,
}

struct GroupTurn<'a> {
    context: &'a str,
    cancel: &'a Arc<AtomicBool>,
    everyone_entry: Option<&'a str>,
    retry_task_id: Option<&'a str>,
}

fn reserve_round_reply(
    state: &mut ProductState,
    entry_id: &str,
    task_id: &str,
    agent: &PrismMarble,
    thread: Option<&str>,
) {
    if state
        .everyone_pending
        .contains_key(&format!("task:{task_id}"))
    {
        return;
    }
    let source = format!("round:{task_id}");
    crate::everyone::reserve(state, entry_id, &source, &agent.id, &agent.name, thread);
    crate::everyone::claim(state, &source, task_id);
}

#[derive(Clone, PartialEq, Eq, Hash)]
enum RoundScope {
    Group(String),
    Everyone(String),
}

enum RoundSource<'a> {
    New,
    Retry(Box<TaskRecord>),
    Held(&'a str),
}

pub(crate) struct HeldRound {
    pub id: String,
    pub position: usize,
}

impl RoundScope {
    fn thread(&self) -> &str {
        match self {
            Self::Group(id) | Self::Everyone(id) => id,
        }
    }

    fn conversation(&self) -> &'static str {
        match self {
            Self::Group(_) => GROUP_CONVERSATION,
            Self::Everyone(_) => crate::everyone::EVERYONE_CONVERSATION,
        }
    }

    fn record<'a>(&self, state: &'a ProductState) -> Result<&'a Thread, (u16, &'static str)> {
        let record = state
            .threads
            .iter()
            .find(|item| item.id == self.thread() && item.conversation_id == self.conversation())
            .ok_or((404, "unknown_thread"))?;
        if record.archived {
            return Err((
                409,
                if matches!(self, Self::Everyone(_)) {
                    "thread_archived"
                } else {
                    "group_thread_closed"
                },
            ));
        }
        if matches!(self, Self::Group(_)) {
            let group = record.group.as_ref().ok_or((409, "not_a_group_thread"))?;
            if group.closed {
                return Err((409, "group_thread_closed"));
            }
        }
        Ok(record)
    }
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
    engines: Mutex<HashMap<String, PrismEngine>>,
    groups: Mutex<HashMap<RoundScope, Arc<GroupLane>>>,
    /// Marbles waiting on an ask: marble id → the channels answering that ask.
    ask_waiters: Mutex<HashMap<String, Vec<AskWaiter>>>,
    /// Depth of the turn each task is part of, so asks cannot chain forever.
    task_depths: Mutex<HashMap<String, u8>>,
    /// Depth waiting for a dispatched task to be accepted, keyed by marble id.
    pending_depths: Mutex<HashMap<String, u8>>,
    /// How many asks one turn has made, capped so a turn cannot fan out forever.
    ask_counts: Mutex<HashMap<String, usize>>,
}

struct PrismEngine {
    conversation: Arc<Mutex<Conversation>>,
    steer: mpsc::Sender<SteerRequest>,
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
        parts
            .events
            .attach_replies(&mut parts.state.lock().unwrap_or_else(|e| e.into_inner()));
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
            groups: Mutex::new(HashMap::new()),
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

    pub(crate) fn start_group_round(
        self: &Arc<Self>,
        thread: String,
        text: String,
        client: Option<crate::state::ClientMessage>,
        zone: Option<String>,
    ) -> Result<(), (u16, &'static str)> {
        self.start_round(
            RoundScope::Group(thread),
            text,
            client,
            zone,
            RoundSource::New,
        )
        .map(|_| ())
    }

    pub(crate) fn start_everyone_round(
        self: &Arc<Self>,
        thread: String,
        text: String,
        client: Option<crate::state::ClientMessage>,
        zone: Option<String>,
    ) -> Result<Option<HeldRound>, (u16, &'static str)> {
        self.start_round(
            RoundScope::Everyone(thread),
            text,
            client,
            zone,
            RoundSource::New,
        )
    }

    pub(crate) fn retry_everyone_round(
        self: &Arc<Self>,
        thread: String,
        task: TaskRecord,
        zone: Option<String>,
    ) -> Result<(), (u16, &'static str)> {
        self.start_round(
            RoundScope::Everyone(thread),
            task.prompt.clone(),
            None,
            zone,
            RoundSource::Retry(Box::new(task)),
        )
        .map(|_| ())
    }

    fn start_round(
        self: &Arc<Self>,
        scope: RoundScope,
        text: String,
        client: Option<crate::state::ClientMessage>,
        zone: Option<String>,
        source: RoundSource<'_>,
    ) -> Result<Option<HeldRound>, (u16, &'static str)> {
        let lane = self
            .groups
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .entry(scope.clone())
            .or_default()
            .clone();
        let cancel = Arc::new(AtomicBool::new(false));
        let round_id = format!("group-round-{}", crate::next_request_id());
        let entry_id = client.as_ref().map_or_else(
            || match source {
                RoundSource::Held(id) => id.to_owned(),
                _ => format!("entry-{}", crate::next_request_id()),
            },
            |message| message.entry_id.clone(),
        );
        let mut latest = lane.latest.lock().unwrap_or_else(|e| e.into_inner());
        let snapshot = {
            let mut state = self.lock_state();
            scope.record(&state)?;
            if state.control.paused {
                if !matches!(source, RoundSource::New) || !matches!(scope, RoundScope::Everyone(_))
                {
                    return Err((409, "admission_paused"));
                }
                let queued = state
                    .control
                    .enqueue(Some(scope.conversation().into()), &text)
                    .map_err(|_| (409, "queue_full"))?;
                let position = state.control.queue.len();
                let entry = state.control.queue.last_mut().expect("just queued");
                entry.everyone_round = true;
                entry.thread_id = Some(scope.thread().into());
                entry.client_message = client.clone();
                entry.device_time_zone = zone.clone();
                state.bump();
                let snapshot = state.clone();
                drop(state);
                self.persist(&snapshot);
                return Ok(Some(HeldRound {
                    id: queued.id,
                    position,
                }));
            }
            if let RoundSource::Retry(task) = &source
                && task.marble_id != "marble"
                && !state
                    .prism
                    .get(&task.marble_id)
                    .is_some_and(PrismMarble::in_everyone)
            {
                return Err((409, "agent_not_in_everyone"));
            }
            if let RoundSource::Held(id) = source {
                if latest.is_some() {
                    return Err((409, "round_busy"));
                }
                let index = state
                    .control
                    .queue
                    .iter()
                    .position(|entry| {
                        entry.everyone_round && entry.thread_id.as_deref() == Some(scope.thread())
                    })
                    .ok_or((404, "unknown_queue_entry"))?;
                if state.control.queue[index].id != id {
                    return Err((409, "queue_entry_not_head"));
                }
                state.control.queue.remove(index);
            }
            if matches!(scope, RoundScope::Everyone(_)) {
                crate::everyone::ensure_conversation(&mut state);
            }
            if let Some(previous) = latest.replace(cancel.clone()) {
                previous.store(true, Ordering::Relaxed);
            }
            self.cancels.register(&round_id, cancel.clone());
            Self::record_round_message(&mut state, &scope, entry_id.clone(), &text, client);
            state.bump();
            state.clone()
        };
        self.persist(&snapshot);
        if matches!(source, RoundSource::Held(_))
            && let Some(entry) = snapshot
                .transcript
                .iter()
                .find(|entry| entry.id == entry_id)
        {
            self.events.publish(
                scope.conversation(),
                "",
                "entry",
                serde_json::to_value(entry).unwrap(),
            );
        }
        drop(latest);
        let retry = match source {
            RoundSource::Retry(task) => Some(task),
            _ => None,
        };
        let pool = Arc::clone(self);
        std::thread::spawn(move || {
            let running = lane.running.lock().unwrap_or_else(|e| e.into_inner());
            pool.run_group_round(
                &scope,
                &entry_id,
                &text,
                zone.as_deref(),
                &cancel,
                retry.as_deref(),
            );
            pool.cancels.forget(&round_id);
            let mut latest = lane.latest.lock().unwrap_or_else(|e| e.into_inner());
            if latest
                .as_ref()
                .is_some_and(|current| Arc::ptr_eq(current, &cancel))
            {
                *latest = None;
            }
            drop(latest);
            drop(running);
            if let RoundScope::Everyone(thread) = scope {
                pool.resume_held_everyone(&thread);
            }
        });
        Ok(None)
    }

    fn record_round_message(
        state: &mut ProductState,
        scope: &RoundScope,
        entry_id: String,
        text: &str,
        client: Option<crate::state::ClientMessage>,
    ) {
        state.push_transcript(TranscriptEntry {
            id: entry_id,
            client_message_id: client.map(|message| message.id),
            conversation_id: scope.conversation().into(),
            kind: "user".into(),
            text: text.into(),
            thread_id: Some(scope.thread().into()),
            speaker: crate::everyone::USER_SPEAKER.into(),
            speaker_name: "You".into(),
            at: now_millis(),
            ..Default::default()
        });
        state.refresh_thread(scope.thread());
    }

    fn run_group_round(
        self: &Arc<Self>,
        scope: &RoundScope,
        entry_id: &str,
        message: &str,
        zone: Option<&str>,
        cancel: &Arc<AtomicBool>,
        retry: Option<&TaskRecord>,
    ) {
        let thread = scope.thread();
        let everyone = matches!(scope, RoundScope::Everyone(_));
        let mut visited = Vec::new();
        let mut replies = Vec::<(String, String)>::new();
        let mut handoff = String::new();
        for _ in 0..if retry.is_some() { 1 } else { 4 } {
            if cancel.load(Ordering::Relaxed) {
                return;
            }
            let (agent, context) = {
                let state = self.lock_state();
                let Ok(record) = scope.record(&state) else {
                    return;
                };
                let everyone_group;
                let group = if everyone {
                    everyone_group = GroupThread {
                        members: std::iter::once("marble".to_owned())
                            .chain(
                                state
                                    .prism
                                    .marbles
                                    .iter()
                                    .filter(|agent| agent.in_everyone())
                                    .map(|agent| agent.id.clone()),
                            )
                            .collect(),
                        ..GroupThread::default()
                    };
                    &everyone_group
                } else {
                    record.group.as_ref().expect("validated group")
                };
                let roster = |id: &str| {
                    if id == "marble" {
                        Some(state.prism.default_marble())
                    } else {
                        state.prism.get(id).cloned()
                    }
                };
                let mentioned = |text: &str, agent: &PrismMarble| {
                    let text = text.to_lowercase();
                    crate::everyone::mention_position(&text, &agent.id).or_else(|| {
                        crate::everyone::mention_position(&text, &agent.name.to_lowercase())
                    })
                };
                let mut agents: Vec<_> = group.members.iter().filter_map(|id| roster(id)).collect();
                let directed = everyone
                    && crate::everyone::mention_position(&message.to_lowercase(), "everyone")
                        .is_none()
                    && agents
                        .iter()
                        .any(|agent| mentioned(message, agent).is_some());
                agents.retain(|agent| !visited.contains(&agent.id));
                let first_mentioned = |text: &str| {
                    agents
                        .iter()
                        .filter_map(|agent| {
                            mentioned(text, agent).map(|position| (position, agent))
                        })
                        .min_by_key(|(position, _)| *position)
                        .map(|(_, agent)| agent)
                };
                let lead = || agents.iter().find(|agent| agent.id == "lead");
                let agent = if let Some(task) = retry {
                    agents.iter().find(|agent| agent.id == task.marble_id)
                } else if !everyone && visited.is_empty() {
                    lead()
                } else {
                    first_mentioned(&handoff)
                        .or_else(|| first_mentioned(message))
                        .or_else(|| {
                            if directed {
                                return None;
                            }
                            visited
                                .is_empty()
                                .then(lead)
                                .flatten()
                                .or_else(|| {
                                    agents.iter().find(|agent| {
                                        agent.card.as_ref().is_some_and(|card| {
                                            card.when_to_speak == WhenToSpeak::Always
                                        })
                                    })
                                })
                                .or_else(|| {
                                    agents.iter().find(|agent| {
                                        agent.card.as_ref().is_none_or(|card| {
                                            card.when_to_speak == WhenToSpeak::Relevant
                                        })
                                    })
                                })
                        })
                };
                let Some(agent) = agent.cloned() else { return };
                let available = state
                    .prism
                    .marbles
                    .iter()
                    .filter(|agent| !group.members.contains(&agent.id))
                    .map(|agent| agent.id.as_str())
                    .collect::<Vec<_>>()
                    .join(", ");
                let history = state
                    .thread_entry_refs(record)
                    .into_iter()
                    .rev()
                    .filter(|entry| matches!(entry.kind.as_str(), "user" | "assistant"))
                    .take(16)
                    .collect::<Vec<_>>()
                    .into_iter()
                    .rev()
                    .map(|entry| {
                        (
                            state.entry_sender(entry),
                            entry.text.chars().take(1800).collect::<String>(),
                        )
                    })
                    .collect::<Vec<_>>();
                let explicit_handoff = mentioned(&handoff, &agent).is_some();
                let mut context = format!(
                    "You're {} in the group chat {}. Speak only for yourself; be brief and do not repeat earlier contributions. If you have nothing new to add, reply with exactly [[PASS]]. Use @agent-id to hand off to an existing member. If an outside specialist would help, start your reply with [[SUGGEST: agent-id]] on its own line; do not claim they joined. Only Lead may close a task after its Done when is met: start with [[CLOSE]] on its own line, then give a short summary.\nMembers: {}\nAvailable specialists: {}\nGoal: {}\nDone when: {}\nRecent thread messages (quoted): {}\nEarlier replies this round (quoted, in order): {}",
                    agent.name,
                    record.title,
                    group.members.join(", "),
                    available,
                    group.goal,
                    group.done_when,
                    json!(history),
                    json!(replies),
                );
                if explicit_handoff {
                    context.push_str("\nThe previous speaker asked you to take over. Address their request when you can add something; pass if you have nothing new to contribute.");
                }
                if let Some(parent) = state.thread_preamble(thread) {
                    context.push('\n');
                    context.push_str(&parent);
                }
                (agent, context)
            };
            visited.push(agent.id.clone());
            let root = match self.workspace_root(&agent) {
                Ok(root) => root,
                Err(error) => {
                    self.fail_before_acceptance(
                        &agent,
                        message,
                        &error,
                        Some(thread),
                        None,
                        everyone.then_some(entry_id),
                    );
                    continue;
                }
            };
            let Some(reply) = self.run_marble(
                &agent,
                root,
                message,
                Some(thread.into()),
                None,
                zone.map(str::to_owned),
                Some(GroupTurn {
                    context: &context,
                    cancel,
                    everyone_entry: everyone.then_some(entry_id),
                    retry_task_id: retry.map(|task| task.id.as_str()),
                }),
            ) else {
                continue;
            };
            if reply.cancelled || cancel.load(Ordering::Relaxed) {
                return;
            }
            handoff.clone_from(&reply.text);
            if !reply.text.is_empty() {
                replies.push((agent.name.clone(), reply.text));
            }
            let RoundScope::Group(thread) = scope else {
                continue;
            };
            let snapshot = {
                let mut state = self.lock_state();
                if cancel.load(Ordering::Relaxed) {
                    return;
                }
                let valid_suggestion = match &reply.group_action {
                    Some(GroupAction::Suggest(id)) => {
                        id == "marble" || state.prism.get(id).is_some()
                    }
                    _ => false,
                };
                let Some(group) = state
                    .threads
                    .iter_mut()
                    .find(|item| item.id == *thread)
                    .and_then(|item| item.group.as_mut())
                else {
                    return;
                };
                match reply.group_action {
                    Some(GroupAction::Close)
                        if agent.id == "lead" && !group.done_when.is_empty() =>
                    {
                        group.closed = true
                    }
                    Some(GroupAction::Suggest(id))
                        if valid_suggestion
                            && !group.members.contains(&id)
                            && !group.suggestions.iter().any(|item| item.agent_id == id) =>
                    {
                        group.suggestions.push(MemberSuggestion {
                            agent_id: id,
                            suggested_by: agent.id,
                        });
                    }
                    _ => continue,
                }
                state.refresh_thread(thread);
                state.bump();
                state.clone()
            };
            self.persist(&snapshot);
        }
    }

    /// Admit a dispatch and start each marble on its own thread.
    pub fn dispatch(
        self: &Arc<Self>,
        text: &str,
        requested: Option<&[String]>,
    ) -> Result<PrismDispatch, DispatchRefusal> {
        self.dispatch_inner(text, requested, None, None, None, None, None)
    }

    pub fn dispatch_in_thread(
        self: &Arc<Self>,
        text: &str,
        requested: Option<&[String]>,
        thread: Option<&str>,
    ) -> Result<PrismDispatch, DispatchRefusal> {
        self.dispatch_inner(text, requested, None, thread, None, None, None)
    }

    pub(crate) fn dispatch_client_in_thread(
        self: &Arc<Self>,
        text: &str,
        requested: Option<&[String]>,
        thread: Option<&str>,
        client: Option<crate::state::ClientMessage>,
        device_time_zone: Option<&str>,
    ) -> Result<PrismDispatch, DispatchRefusal> {
        self.dispatch_inner(
            text,
            requested,
            None,
            thread,
            client,
            device_time_zone,
            None,
        )
    }

    pub(crate) fn dispatch_private_chat(
        self: &Arc<Self>,
        text: &str,
        marble: &str,
        conversation: &str,
        thread: Option<&str>,
        client: Option<crate::state::ClientMessage>,
        device_time_zone: Option<&str>,
    ) -> Result<PrismDispatch, DispatchRefusal> {
        self.dispatch_inner(
            text,
            Some(&[marble.to_owned()]),
            None,
            thread,
            client,
            device_time_zone,
            Some(conversation),
        )
    }

    pub(crate) fn resume_held_everyone(self: &Arc<Self>, thread: &str) -> bool {
        let entry = self
            .lock_state()
            .control
            .queue
            .iter()
            .find(|entry| entry.everyone_round && entry.thread_id.as_deref() == Some(thread))
            .cloned();
        entry.is_some_and(|entry| {
            self.start_round(
                RoundScope::Everyone(thread.into()),
                entry.text,
                entry.client_message,
                entry.device_time_zone,
                RoundSource::Held(&entry.id),
            )
            .is_ok()
        })
    }

    pub fn resume_held(self: &Arc<Self>) -> usize {
        let queued = self.lock_state().control.queue.clone();
        let mut seen = HashSet::new();
        let mut seen_threads = HashSet::new();
        let mut dispatched = 0;
        for entry in queued {
            if entry.everyone_round {
                if let Some(thread) = entry.thread_id
                    && seen_threads.insert(thread.clone())
                    && self.resume_held_everyone(&thread)
                {
                    dispatched += 1;
                }
                continue;
            }
            let Some(id) = entry.marble_id else { continue };
            if !seen.insert(id.clone()) {
                continue;
            }
            match self.dispatch_inner(
                &entry.text,
                Some(std::slice::from_ref(&id)),
                Some(&entry.id),
                entry.thread_id.as_deref(),
                entry.client_message.clone(),
                entry.device_time_zone.as_deref(),
                entry.conversation_id.as_deref(),
            ) {
                Ok(_) => dispatched += 1,
                Err(error)
                    if !matches!(
                        error.code,
                        "marble_busy" | "admission_paused" | "unknown_queue_entry"
                    ) =>
                {
                    let marble = {
                        let mut state = self.lock_state();
                        state.control.queue.retain(|queued| queued.id != entry.id);
                        crate::everyone::rekey(&mut state, &entry.id, &format!("marble:{id}"));
                        state.prism.get(&id).cloned()
                    };
                    if let Some(marble) = marble {
                        self.fail_before_acceptance(
                            &marble,
                            &entry.text,
                            &error.message,
                            entry.thread_id.as_deref(),
                            entry.client_message.as_ref(),
                            None,
                        );
                    }
                }
                Err(_) => {}
            }
        }
        dispatched
    }

    #[allow(clippy::too_many_arguments)]
    fn dispatch_inner(
        self: &Arc<Self>,
        text: &str,
        requested: Option<&[String]>,
        queue_id: Option<&str>,
        thread_id: Option<&str>,
        client: Option<crate::state::ClientMessage>,
        device_time_zone: Option<&str>,
        conversation: Option<&str>,
    ) -> Result<PrismDispatch, DispatchRefusal> {
        let targets = {
            let state = self.lock_state();
            let mut targets = plan_dispatch(&state, text, requested)?;
            if let Some(id) = conversation.filter(|id| !crate::everyone::is_session(id)) {
                let owner = state.conversation_agent(id).ok_or_else(|| {
                    refuse(
                        404,
                        "unknown_conversation",
                        "This agent chat no longer exists",
                    )
                })?;
                if targets.len() != 1 || targets[0].id != owner.id {
                    return Err(refuse(
                        409,
                        "conversation_agent_mismatch",
                        "That chat belongs to another agent",
                    ));
                }
                targets[0].conversation_id = id.to_owned();
            }
            targets
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
                let explicit = state.conversations.iter().any(|record| {
                    record.id == marble.conversation_id && record.marble_id.is_some()
                });
                state.prism.mark_dispatched(
                    &marble.id,
                    if explicit {
                        ""
                    } else {
                        &marble.conversation_id
                    },
                );
                ids.push(marble.id.clone());
                names.push(marble.name.clone());
            }
            if swarm {
                state.prism.swarms = state.prism.swarms.saturating_add(1);
            }
            if let Some(id) = queue_id {
                state.control.queue.retain(|entry| entry.id != id);
                for marble in &targets {
                    crate::everyone::rekey(&mut state, id, &format!("marble:{}", marble.id));
                }
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
        for (index, (marble, root)) in planned.into_iter().enumerate() {
            let pool = Arc::clone(self);
            let prompt = prompt.clone();
            let marble_id = marble.id.clone();
            let marble_name = marble.name.clone();
            let target = marble.clone();
            let thread = thread_id.map(str::to_string);
            let mut identity = client.clone();
            if index > 0
                && let Some(identity) = &mut identity
            {
                identity.entry_id = format!("{}-{}", identity.entry_id, marble.id);
            }
            let turn_identity = identity.clone();
            let device_time_zone = device_time_zone.map(str::to_owned);
            let spawned = std::thread::Builder::new()
                .name(format!("glass-marble-prism-{marble_id}"))
                .spawn(move || {
                    pool.run_marble(
                        &target,
                        root,
                        &prompt,
                        thread,
                        turn_identity,
                        device_time_zone,
                        None,
                    );
                });
            if let Err(error) = spawned {
                tracing::error!(%error, marble = %marble_name, "could not start a prism turn");
                self.fail_before_acceptance(
                    &marble,
                    text,
                    &format!("could not start the turn: {error}"),
                    thread_id,
                    identity.as_ref(),
                    None,
                );
            }
        }
        Ok(PrismDispatch { ids, names, swarm })
    }

    fn lock_state(&self) -> std::sync::MutexGuard<'_, ProductState> {
        self.state.lock().unwrap_or_else(|error| error.into_inner())
    }

    fn persist(&self, snapshot: &ProductState) {
        store::persist_optional(self.store.as_deref(), snapshot);
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
            .map(|engine| Arc::clone(&engine.conversation))
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
        let (steer, steer_rx) = mpsc::channel();
        let instructions_marble = marble.clone();
        let factory = Arc::clone(&self.factory);
        let conversation = conversation
            .with_steering(steer_rx)
            .with_product(Arc::clone(&self.state))
            .with_speaker(&marble.name)
            .with_model({
                let state = self.lock_state();
                (!state.model.trim().is_empty()).then(|| state.model.trim().to_string())
            })
            .with_instructions_fn(move |cwd| {
                crate::conversation::marble_session_instructions(&instructions_marble, &roster, cwd)
            })
            .with_respawn(move || factory());
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
            .insert(
                marble.id.clone(),
                PrismEngine {
                    conversation: Arc::clone(&engine),
                    steer,
                },
            );
        tracing::info!(marble = %marble.name, workspace = %root.display(), "started a prism engine");
        Ok(engine)
    }

    pub(crate) fn steer(&self, marble_id: &str, request: SteerRequest) -> bool {
        self.engines
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .get(marble_id)
            .is_some_and(|engine| engine.steer.send(request).is_ok())
    }

    /// Seeds the ask depth for the next dispatch to this marble, then runs it.
    fn run_marble_at_depth(
        self: &Arc<Self>,
        marble: &PrismMarble,
        root: PathBuf,
        text: &str,
        depth: u8,
        thread_id: Option<String>,
        device_time_zone: Option<String>,
    ) {
        if depth > 0 {
            self.pending_depths
                .lock()
                .unwrap_or_else(|error| error.into_inner())
                .insert(marble.id.clone(), depth);
        }
        self.run_marble(marble, root, text, thread_id, None, device_time_zone, None);
    }

    /// Runs an ask from the marble that owns `scope`.
    pub fn ask(
        self: &Arc<Self>,
        scope: &crate::tools::ToolScope,
        arguments: &Value,
    ) -> ToolOutcome {
        if crate::everyone::is_group_session(&scope.conversation_id) {
            return ToolOutcome::error(
                "Use an @mention in your reply to hand off this group round.",
            );
        }
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
        let thread = self.record_ask(&asker_id, &task_id, &scope.conversation_id, entry.clone());
        let prompt = format!("{asker_name} asks: {instruction}");
        let root = self.workspace_root(&target).unwrap_or_default();
        let pool = Arc::clone(self);
        let device_time_zone = scope.device_time_zone.clone();
        let spawned = std::thread::Builder::new()
            .name(format!("glass-marble-ask-{}", target.id))
            .spawn(move || {
                pool.run_marble_at_depth(
                    &target,
                    root,
                    &prompt,
                    depth + 1,
                    thread,
                    device_time_zone,
                )
            });
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
    ) -> Option<String> {
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
            let before = state.transcript.len();
            crate::everyone::record_handoff(&mut state, &conversation, task_id, &entry);
            let parent_conversation = if state
                .everyone_pending
                .contains_key(&format!("task:{task_id}"))
            {
                crate::everyone::EVERYONE_CONVERSATION
            } else {
                conversation.as_str()
            };
            let parent = state.transcript[before..]
                .iter()
                .find(|e| e.conversation_id == parent_conversation && e.kind == "handoff")
                .map(|e| (e.conversation_id.clone(), e.id.clone()));
            let thread = if entry.status == "asked" {
                parent
                    .and_then(|(cv, id)| state.create_thread(&cv, &id))
                    .map(|t| t.id)
            } else {
                None
            };
            state.bump();
            (state.clone(), thread)
        };
        self.persist(&snapshot.0);
        self.events.publish(
            scope_conversation,
            task_id,
            "handoff",
            serde_json::json!({
                "from": entry.from,
                "to": entry.to,
                "status": entry.status,
            }),
        );
        snapshot.1
    }

    /// Run one marble's turn to settlement.
    #[allow(clippy::too_many_arguments)]
    fn run_marble(
        self: &Arc<Self>,
        marble: &PrismMarble,
        root: PathBuf,
        text: &str,
        thread_id: Option<String>,
        client: Option<crate::state::ClientMessage>,
        device_time_zone: Option<String>,
        group: Option<GroupTurn<'_>>,
    ) -> Option<crate::MarbleReply> {
        let engine = match self.engine(marble, &root) {
            Ok(engine) => engine,
            Err(error) => {
                self.fail_before_acceptance(
                    marble,
                    text,
                    &error,
                    thread_id.as_deref(),
                    client.as_ref(),
                    group.as_ref().and_then(|group| group.everyone_entry),
                );
                self.resume_held();
                return None;
            }
        };
        let mut guard = engine.lock().unwrap_or_else(|error| error.into_inner());
        let refreshed;
        let marble = if let Some(group) = &group {
            let state = self.lock_state();
            if group.cancel.load(Ordering::Relaxed) || state.control.paused {
                return None;
            }
            let still_member = if let Some(entry_id) = group.everyone_entry {
                state.transcript.iter().any(|entry| entry.id == entry_id)
                    && (marble.id == "marble"
                        || state
                            .prism
                            .get(&marble.id)
                            .is_some_and(PrismMarble::in_everyone))
                    && thread_id
                        .as_ref()
                        .is_some_and(|id| RoundScope::Everyone(id.clone()).record(&state).is_ok())
            } else {
                state.threads.iter().any(|thread| {
                    Some(thread.id.as_str()) == thread_id.as_deref()
                        && !thread.archived
                        && thread.group.as_ref().is_some_and(|group| {
                            !group.closed && group.members.contains(&marble.id)
                        })
                })
            };
            if !still_member {
                return None;
            }
            refreshed = if marble.id == "marble" {
                state.prism.default_marble()
            } else {
                state.prism.get(&marble.id)?.clone()
            };
            &refreshed
        } else {
            marble
        };
        let conversation = &mut *guard;
        conversation.set_store(self.store.as_deref());
        let (tool_limit, turn_timeout) = marble.turn_budget();
        conversation.set_tool_limit(tool_limit);
        conversation.set_knowledge_owner(&marble.id);
        conversation.set_agent_tools(marble.card.as_ref().map(|card| card.tools.clone()));
        let host_model = self.lock_state().model.trim().to_owned();
        conversation.set_model((!host_model.is_empty()).then_some(host_model));
        conversation.set_card_model(marble.model(""));
        let conversation_id = if group.is_some() {
            let id = crate::everyone::session_id(&marble.id, thread_id.as_deref());
            let mut state = self.lock_state();
            crate::everyone::ensure_session(&mut state, &id);
            if let Some(retry) = group.as_ref().and_then(|group| group.retry_task_id) {
                state.conversation_mut(&id).retry_task_id = retry.into();
            }
            id
        } else {
            self.turn_conversation(marble, thread_id.as_deref())
        };
        let mut accepted: Option<(String, String)> = None;
        let request_id = crate::next_request_id();
        let mut live_turn = self
            .live
            .begin(text, &marble.name, &conversation_id, &request_id);
        let mut input = crate::normalize::engine_input(
            &self.lock_state(),
            Some(&conversation_id),
            &marble.id,
            text,
        );
        if let Some(group) = &group {
            input.text = format!("{}\n\n{}", group.context, input.text);
        } else if let Some(context) = thread_id
            .as_deref()
            .and_then(|id| self.lock_state().thread_preamble(id))
        {
            input.text = format!("{context}\n\n{}", input.text);
        }
        let turn = crate::MarbleTurn {
            device_time_zone,
            conversation_id: (!conversation_id.is_empty()).then_some(conversation_id),
            request_id,
            input,
        };
        let cancel = group.as_ref().map_or_else(
            || Arc::new(AtomicBool::new(false)),
            |group| Arc::clone(group.cancel),
        );
        let mut on_accepted = |conversation_id: &str, task_id: &str| {
            if let Some((_, previous)) =
                accepted.replace((conversation_id.to_string(), task_id.to_string()))
            {
                self.cancels.forget(&previous);
            }
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
                    marble_id: marble.id.clone(),
                    id: task_id.to_string(),
                    conversation_id: conversation_id.to_string(),
                    status: "running".into(),
                    owner: "prism".into(),
                    prompt: text.to_string(),
                    at: now,
                    routine_id: String::new(),
                });
                state.bind_thread(task_id, thread_id.as_deref());
                if group.is_none() {
                    state.push_transcript(TranscriptEntry {
                        id: client
                            .as_ref()
                            .map(|message| message.entry_id.clone())
                            .unwrap_or_default(),
                        client_message_id: client.as_ref().map(|message| message.id.clone()),
                        conversation_id: conversation_id.to_string(),
                        kind: "user".into(),
                        text: text.to_string(),
                        task_id: task_id.to_string(),
                        at: now,
                        ..Default::default()
                    });
                }
                let explicit = state
                    .conversations
                    .iter()
                    .any(|record| record.id == conversation_id && record.marble_id.is_some());
                state.prism.note_task(
                    &marble.id,
                    if explicit { "" } else { conversation_id },
                    task_id,
                );
                if let Some(entry_id) = group.as_ref().and_then(|group| group.everyone_entry) {
                    reserve_round_reply(
                        &mut state,
                        entry_id,
                        task_id,
                        marble,
                        thread_id.as_deref(),
                    );
                } else if group.is_none() {
                    crate::everyone::claim(&mut state, &format!("marble:{}", marble.id), task_id);
                }
                state.bump();
                state.clone()
            };
            self.persist(&snapshot);
            if let Some(client) = &client
                && let Some(entry) = snapshot
                    .transcript
                    .iter()
                    .find(|entry| entry.id == client.entry_id)
            {
                self.events.publish(
                    conversation_id,
                    task_id,
                    "entry",
                    serde_json::to_value(entry).unwrap(),
                );
            }
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
                group: group.is_some(),
                cancel: Some(cancel.as_ref()),
                timeout: self.timeout.min(turn_timeout),
                on_accepted: Some(&mut on_accepted),
                live: live_turn.as_mut(),
            },
        );
        let reply = match result {
            Ok(mut reply) => {
                self.cancels.forget(&reply.task_id);
                self.task_depths
                    .lock()
                    .unwrap_or_else(|error| error.into_inner())
                    .remove(&reply.task_id);
                if group.is_none()
                    && let Some(waiters) = self
                        .ask_waiters
                        .lock()
                        .unwrap_or_else(|error| error.into_inner())
                        .remove(&marble.id)
                {
                    for waiter in waiters {
                        let _ = waiter.send(Ok(reply.text.clone()));
                    }
                }
                let (snapshot, removed) = {
                    let mut state = self.lock_state();
                    if group.is_some() && cancel.load(Ordering::Relaxed) {
                        reply.cancelled = true;
                    }
                    let now = now_millis();
                    let status = if reply.cancelled {
                        "cancelled"
                    } else {
                        "completed"
                    };
                    state.set_task_status(&reply.task_id, status);
                    state.conversation_mut(&reply.conversation_id).updated_at = now;
                    if (group.is_none() || !reply.cancelled) && !reply.reasoning.trim().is_empty() {
                        state.push_transcript(TranscriptEntry {
                            conversation_id: reply.conversation_id.clone(),
                            kind: "reasoning".into(),
                            text: reply.reasoning.clone(),
                            task_id: reply.task_id.clone(),
                            at: now,
                            ..Default::default()
                        });
                    }
                    if (group.is_none() || !reply.cancelled) && !reply.text.trim().is_empty() {
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
                    let mut removed = Vec::new();
                    if group
                        .as_ref()
                        .is_some_and(|group| group.everyone_entry.is_some())
                        && (reply.cancelled || reply.text.trim().is_empty())
                    {
                        crate::everyone::settle_mirror(&mut state, &reply.task_id);
                        state.transcript.retain(|entry| {
                            if entry.conversation_id == crate::everyone::EVERYONE_CONVERSATION
                                && entry.kind == "pending"
                                && entry.task_id == reply.task_id
                            {
                                removed.push((entry.id.clone(), entry.thread_id.clone()));
                                false
                            } else {
                                true
                            }
                        });
                        state.flush_everyone();
                        if !removed.is_empty()
                            && let Some(thread) = &thread_id
                        {
                            state.refresh_thread(thread);
                        }
                    } else {
                        crate::everyone::mirror_settlement(
                            &mut state,
                            &reply.task_id,
                            &reply.text,
                            status,
                        );
                    }
                    state.settle_knowledge(&reply, &marble.id, text);
                    state.bump();
                    (state.clone(), removed)
                };
                self.persist(&snapshot);
                for (id, thread) in removed {
                    self.events.publish(
                        crate::everyone::EVERYONE_CONVERSATION,
                        &reply.task_id,
                        "entry_removed",
                        json!({"id": id, "thread_id": thread}),
                    );
                }
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
                Some(reply)
            }
            Err(error) => {
                if group.is_none()
                    && let Some(waiters) = self
                        .ask_waiters
                        .lock()
                        .unwrap_or_else(|error| error.into_inner())
                        .remove(&marble.id)
                {
                    for waiter in waiters {
                        let _ = waiter.send(Err(crate::failure::MESSAGE.to_string()));
                    }
                }
                if accepted.is_none() {
                    if conversation.transport_failed() {
                        self.lock_state()
                            .presence
                            .runtime
                            .note_engine_failure(conversation.engine_id(), error.to_string());
                    }
                    if !cancel.load(Ordering::Relaxed) {
                        self.fail_before_acceptance(
                            marble,
                            text,
                            &error.to_string(),
                            thread_id.as_deref(),
                            client.as_ref(),
                            group.as_ref().and_then(|group| group.everyone_entry),
                        );
                    }
                    drop(guard);
                    self.resume_held();
                    return None;
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
                        self.cancels.forget(task_id);
                        if group.is_some() && cancel.load(Ordering::Relaxed) {
                            state.set_task_status(task_id, "cancelled");
                        } else {
                            state.set_task_status(task_id, "failed");
                            state.record_failure(conversation_id, task_id, &error.to_string());
                        }
                        crate::everyone::mirror_settlement(&mut state, task_id, "", "failed");
                        state.conversation_mut(conversation_id).updated_at = now;
                    }
                    state
                        .prism
                        .note_settled(&marble.id, "failed", crate::failure::MESSAGE);
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
                        "message": crate::failure::MESSAGE,
                        "revision": snapshot.revision,
                    }),
                );
                None
            }
        };
        drop(guard);
        self.resume_held();
        reply
    }

    fn fail_before_acceptance(
        &self,
        marble: &PrismMarble,
        text: &str,
        error: &str,
        thread_id: Option<&str>,
        client: Option<&crate::state::ClientMessage>,
        everyone_entry: Option<&str>,
    ) {
        let task_id = format!("failed-{}", crate::next_request_id());
        let conversation_id = self.turn_conversation(marble, thread_id);
        let snapshot = {
            let mut state = self.lock_state();
            let conversation_id = if conversation_id.is_empty() {
                format!("recovered-{task_id}")
            } else {
                conversation_id
            };
            let now = now_millis();
            let checkpoint = state
                .conversations
                .iter()
                .find(|record| record.id == conversation_id)
                .and_then(|record| {
                    state
                        .tasks
                        .iter()
                        .rev()
                        .find(|task| {
                            task.conversation_id == conversation_id
                                && task.status != "queued"
                                && !task.id.starts_with("queued-")
                        })
                        .filter(|task| task.id == record.retry_task_id && task.prompt == text)
                        .and_then(|task| record.checkpoints.get(&task.id).copied())
                });
            let conversation = state.conversation_mut(&conversation_id);
            if let Some(checkpoint) = checkpoint {
                conversation.checkpoints.insert(task_id.clone(), checkpoint);
            }
            if conversation.title.is_empty() {
                conversation.title = format!("{} · {}", marble.name, truncate(text, 64));
            }
            conversation.updated_at = now;
            conversation.task_ids.push(task_id.clone());
            state.upsert_task(TaskRecord {
                id: task_id.clone(),
                conversation_id: conversation_id.clone(),
                marble_id: marble.id.clone(),
                status: "failed".into(),
                owner: "prism".into(),
                prompt: text.to_string(),
                at: now,
                ..Default::default()
            });
            state.bind_thread(&task_id, thread_id);
            if let Some(client) = client {
                state.push_transcript(TranscriptEntry {
                    id: client.entry_id.clone(),
                    client_message_id: Some(client.id.clone()),
                    conversation_id: conversation_id.clone(),
                    kind: "user".into(),
                    text: text.into(),
                    task_id: task_id.clone(),
                    at: now,
                    ..Default::default()
                });
            }
            state.record_failure(&conversation_id, &task_id, error);
            if let Some(entry_id) = everyone_entry {
                reserve_round_reply(&mut state, entry_id, &task_id, marble, thread_id);
            } else if !crate::everyone::is_group_session(&conversation_id) {
                crate::everyone::claim(&mut state, &format!("marble:{}", marble.id), &task_id);
            }
            crate::everyone::mirror_settlement(&mut state, &task_id, "", "failed");
            state
                .prism
                .note_settled(&marble.id, "failed", crate::failure::MESSAGE);
            state.bump();
            state.clone()
        };
        self.persist(&snapshot);
        self.events.publish(
            "",
            &task_id,
            "prism",
            json!({ "marble": marble.id, "status": "failed", "message": crate::failure::MESSAGE }),
        );
    }

    fn turn_conversation(&self, marble: &PrismMarble, thread: Option<&str>) -> String {
        let mut state = self.lock_state();
        if state
            .everyone_pending
            .contains_key(&format!("marble:{}", marble.id))
            || thread.is_some_and(|id| {
                state.threads.iter().any(|thread| {
                    thread.id == id
                        && (thread.conversation_id == crate::everyone::EVERYONE_CONVERSATION
                            || thread.group.is_some())
                })
            })
        {
            let id = crate::everyone::session_id(&marble.id, thread);
            crate::everyone::ensure_session(&mut state, &id);
            return id;
        }
        if state
            .conversation_agent(&marble.conversation_id)
            .is_some_and(|agent| agent.id == marble.id)
        {
            return marble.conversation_id.clone();
        }
        let id = state
            .prism
            .get(&marble.id)
            .map(|m| m.conversation_id.clone())
            .unwrap_or_default();
        if id.is_empty() && self.store.is_some() {
            let id = format!("chat-{}", crate::next_request_id());
            if let Some(marble) = state.prism.find_mut(&marble.id) {
                marble.conversation_id.clone_from(&id);
            }
            state.conversation_mut(&id).needs_engine_session = true;
            return id;
        }
        id
    }

    /// Stop every prism engine, used when a marble leaves the roster.
    pub fn shutdown_marble(&self, id: &str) {
        let engine = self
            .engines
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .remove(id);
        if let Some(engine) = engine {
            let mut guard = engine
                .conversation
                .lock()
                .unwrap_or_else(|error| error.into_inner());
            let _ = guard.shutdown();
        }
    }

    /// Stop every marble engine, so a host shutdown does not leave orphans.
    pub fn shutdown(&self) {
        let engines: Vec<_> = {
            let mut map = self
                .engines
                .lock()
                .unwrap_or_else(|error| error.into_inner());
            map.drain().collect()
        };
        for (id, engine) in engines {
            let mut guard = engine
                .conversation
                .lock()
                .unwrap_or_else(|error| error.into_inner());
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

    #[test]
    fn an_idle_everyone_lane_rejects_a_later_held_candidate() {
        let mut state = ProductState::default();
        let thread = state.start_everyone_thread("FIFO").id;
        let scope = RoundScope::Everyone(thread.clone());
        let root = std::env::temp_dir();
        let pool = Arc::new(PrismPool::new(PrismPoolParts {
            factory: Arc::new(|| panic!("a rejected candidate cannot start an engine")),
            cwd: root.clone(),
            sessions: root.clone(),
            state: Arc::new(Mutex::new(state)),
            store: None,
            live: LiveStore::in_data_dir(&root),
            events: EventBus::default(),
            cancels: CancelRegistry::default(),
            approvals: ApprovalInbox::default(),
            timeout: Duration::from_secs(1),
        }));
        pool.lock_state().control.paused = true;
        let hold = |text: &str| {
            pool.start_everyone_round(thread.clone(), text.into(), None, None)
                .unwrap()
                .unwrap()
                .id
        };
        let first = hold("First");
        let second = hold("Second");
        pool.lock_state().control.paused = false;
        let lane = pool.groups.lock().unwrap().get(&scope).unwrap().clone();
        let _running = lane.running.lock().unwrap();
        let stale_dispatch = pool.start_round(
            scope,
            "Second".into(),
            None,
            None,
            RoundSource::Held(&second),
        );
        if let Some(cancel) = lane.latest.lock().unwrap().as_ref() {
            cancel.store(true, Ordering::Relaxed);
        }
        assert!(matches!(stale_dispatch, Err((409, "queue_entry_not_head"))));
        let state = pool.lock_state();
        assert_eq!(
            state
                .control
                .queue
                .iter()
                .map(|entry| &entry.id)
                .collect::<Vec<_>>(),
            [&first, &second]
        );
        assert!(state.transcript.is_empty());
    }

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
