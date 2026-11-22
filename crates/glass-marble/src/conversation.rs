//! Durable conversation lifecycle over the Glass host runtime client.

use crate::{
    Approval, MarbleError, TURN_TIMEOUT, control::Verdict, events::EventBus, host::HostPolicy,
    state::PendingApproval, state::ProductState, tools::ToolRouter, tools::ToolScope,
};
use glass_runtime_client::{
    Incoming, RuntimeClient, RuntimeEvent, SessionOptions, TurnInput, json, protocol::ToolRequest,
};
use serde_json::{Value, json as value};
use std::collections::{BTreeMap, HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, AtomicU64, Ordering},
    mpsc::{self, Receiver},
};
use std::time::{Duration, Instant};

/// How often a running turn drains the next runtime frame so cancel, text
/// deltas, and tool activity reach the UI promptly.
pub const PUMP_INTERVAL: Duration = Duration::from_millis(200);
/// Bound on auxiliary responses (for example a cancel acknowledgement) a turn
/// may observe before its own response arrives.
const MAX_AUXILIARY_RESPONSES: usize = 8;
/// Bound on request ids remembered until their response arrives, so a runtime
/// that never answers a steer cannot grow this table without limit.
const MAX_OUTSTANDING_RESPONSES: usize = 64;
/// Refusal recorded for a governed call whose approval wait a stop request
/// ended, so the model learns the call never ran.
const CANCELLED_APPROVAL: &str = "cancelled before this tool call ran";

/// Product instructions for a marble conversation.
pub const SESSION_INSTRUCTIONS: &str = "You are a durable collaborator working inside the \
     user's workspace. Core executes your file and shell tools; the `glass.workspace.*` tools are \
     governed integrations that Marble's own control layer classifies before they run, and the human \
     answers anything that layer leaves open. A refusal names the rule behind it, so adapt instead of \
     retrying the same call. Keep changes legible and reversible, say what you changed, and surface \
     attribution when you act on the human's behalf.";

const TOOL_GUIDANCE: &str = "Use your tools (web_search/web_fetch) whenever the user asks for current or factual information you cannot be sure of. Never say you searched or checked without actually calling the tool.";
const CONVERSATIONAL_GUIDANCE: &str = "Match the length of the message: greetings and small talk get one or two sentences, with no unsolicited speeches or monologues. Expand only when asked or when the task needs it.";
pub(crate) const REPLY_PRIVACY_GUIDANCE: &str =
    "Keep reasoning and planning private; put only the final answer in your reply.";

/// Session instructions for one conversation: the product posture, plus what the
/// workspace is made of so the marble does not infer crate ownership from a file
/// path.
fn session_instructions(cwd: &Path) -> String {
    let mut instructions = SESSION_INSTRUCTIONS.to_string();
    instructions.push('\n');
    instructions.push_str(TOOL_GUIDANCE);
    instructions.push('\n');
    instructions.push_str(CONVERSATIONAL_GUIDANCE);
    instructions.push('\n');
    instructions.push_str(REPLY_PRIVACY_GUIDANCE);
    if let Some(briefing) = crate::crates::CrateMap::load(cwd).briefing() {
        instructions.push('\n');
        instructions.push_str(&briefing);
    }
    instructions
}

/// Session instructions for the default assistant: its own personality when
/// the person gave it one, else the plain product posture.
pub fn default_assistant_instructions(persona: &str, cwd: &Path) -> String {
    let persona = persona.trim();
    format!(
        "You are Marble. Speak only as Marble. Never @mention yourself or call yourself 'a Marble'. Personality: {persona}.
{}",
        session_instructions(cwd)
    )
}

/// Session instructions for one named marble: who it is and who it can ask,
/// then the same product posture and workspace briefing the default assistant
/// gets. Instructions are fixed when the engine session is created, so a
/// persona edit only reaches the model through a fresh conversation.
pub fn marble_session_instructions(
    marble: &crate::prism::PrismMarble,
    roster: &[crate::prism::PrismMarble],
    cwd: &Path,
) -> String {
    let persona = if marble.persona.trim().is_empty() {
        marble.role.trim()
    } else {
        marble.persona.trim()
    };
    let others: Vec<String> = roster
        .iter()
        .filter(|other| other.id != marble.id)
        .map(|other| format!("{} — {}", other.name, other.role))
        .collect();
    let others = if others.is_empty() {
        "none yet".to_string()
    } else {
        others.join("; ")
    };
    let mut instructions = format!(
        "You are {name}, one of the user's Marbles. Speak only as {name}. Never @mention yourself or call yourself 'a Marble'. Personality: {persona}. Other Marbles you can ask for help: {others}.",
        name = marble.name
    );
    instructions.push('\n');
    instructions.push_str(&session_instructions(cwd));
    instructions
}

fn strip_self_mention<'a>(text: &'a str, name: &str) -> &'a str {
    let Some(rest) = text.trim_start().strip_prefix('@') else {
        return text;
    };
    let Some(prefix) = rest.get(..name.len()) else {
        return text;
    };
    if name.is_empty() || prefix.to_lowercase() != name.to_lowercase() {
        return text;
    }
    let tail = &rest[name.len()..];
    if tail
        .chars()
        .next()
        .is_some_and(|c| !c.is_whitespace() && !matches!(c, ',' | ':' | '.' | '!' | '?' | '—'))
    {
        return text;
    }
    tail.trim_start_matches(|c: char| {
        c.is_whitespace() || matches!(c, ',' | ':' | '.' | '!' | '?' | '—')
    })
}

#[derive(Default)]
struct ToolRows {
    rows: Vec<(String, String, String)>,
}

impl ToolRows {
    fn normalize(&self, event: &mut RuntimeEvent) {
        if !matches!(event.kind.as_str(), "tool_start" | "tool_end") {
            return;
        }
        let name = event.data["tool"]
            .as_str()
            .or_else(|| event.data["name"].as_str())
            .or_else(|| {
                let call = event.data["callId"].as_str()?;
                self.rows
                    .iter()
                    .find(|row| row.0 == call)
                    .map(|row| row.2.split(" · ").next().unwrap_or("tool"))
            })
            .unwrap_or("tool")
            .to_string();
        event.data["tool"] = value!(name);
    }

    fn observe(&mut self, event: &RuntimeEvent) -> bool {
        if !matches!(event.kind.as_str(), "tool_start" | "tool_end") {
            return false;
        }
        let data = &event.data;
        let id = data["callId"].as_str().unwrap_or_default();
        if id.is_empty() {
            return false;
        }
        let existing = self.rows.iter().position(|row| row.0 == id);
        let status = if event.kind == "tool_start" {
            "running"
        } else if data["isError"].as_bool().unwrap_or(false) {
            "failed"
        } else {
            "ok"
        };
        let name = data["name"]
            .as_str()
            .or_else(|| data["tool"].as_str())
            .unwrap_or("tool");
        let summary = ["query", "url", "path", "command"]
            .iter()
            .find_map(|key| data["arguments"][*key].as_str())
            .or_else(|| data["title"].as_str())
            .unwrap_or_default();
        let summary: String = summary
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
            .chars()
            .take(120)
            .collect();
        let label = if summary.is_empty() {
            name.to_string()
        } else {
            format!("{name} · {summary}")
        };
        if let Some(index) = existing {
            if event.kind == "tool_start" && summary.is_empty() {
                return true;
            }
            let label = if event.kind == "tool_end" {
                self.rows[index].2.clone()
            } else {
                label
            };
            self.rows[index] = (id.into(), format!("{label} · {status}"), label);
        } else {
            self.rows
                .push((id.into(), format!("{label} · {status}"), label));
        }
        true
    }
}

/// Cancel intent for one running durable task.
#[derive(Debug, Clone, Copy)]
pub struct CancelToken<'a> {
    pub requested: &'a AtomicBool,
    pub task_id: &'a str,
}

/// One correction aimed at a running turn.
///
/// Core's `turn/steer` interrupts the current request, keeps the context, and
/// continues the same turn with the correction. A person watching Marble work
/// should be able to say "use the other file" without stopping and restarting.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SteerRequest {
    /// Empty targets whatever is running.
    pub conversation_id: String,
    /// Empty accepts any task in the targeted conversation.
    pub task_id: String,
    pub text: String,
}

/// Awaiting tool request plus the explicit host decision channel.
#[derive(Debug, Clone, Default)]
pub struct ApprovalInbox {
    pending: Arc<Mutex<BTreeMap<String, PendingApproval>>>,
    senders: Arc<Mutex<HashMap<String, mpsc::Sender<Approval>>>>,
}

impl ApprovalInbox {
    fn pending_map(&self) -> std::sync::MutexGuard<'_, BTreeMap<String, PendingApproval>> {
        self.pending
            .lock()
            .unwrap_or_else(|error| error.into_inner())
    }

    fn senders_map(&self) -> std::sync::MutexGuard<'_, HashMap<String, mpsc::Sender<Approval>>> {
        self.senders
            .lock()
            .unwrap_or_else(|error| error.into_inner())
    }

    /// Register one inbound tool request and park for the explicit decision.
    pub fn register(
        &self,
        request_id: &str,
        call_id: &str,
        tool: &str,
        arguments: Value,
        conflict: Option<String>,
    ) -> Receiver<Approval> {
        let (sender, receiver) = mpsc::channel();
        self.pending_map().insert(
            request_id.to_string(),
            PendingApproval {
                request_id: request_id.to_string(),
                call_id: call_id.to_string(),
                tool: tool.to_string(),
                arguments,
                conflict,
            },
        );
        self.senders_map().insert(request_id.to_string(), sender);
        receiver
    }

    /// Resolve one pending tool request with the host decision.
    ///
    /// A delivered decision counts as resolved even when the turn already
    /// cleared its own pending entry while waking up, so the product surface
    /// does not report a successful approval as a conflict.
    pub fn resolve(&self, request_id: &str, approval: Approval) -> bool {
        let mut delivered = false;
        if let Some(sender) = self.senders_map().remove(request_id) {
            delivered = sender.send(approval).is_ok();
        }
        let pending = self.pending_map().remove(request_id).is_some();
        delivered || pending
    }

    /// Snapshot of tool requests still awaiting an explicit decision.
    pub fn pending(&self) -> Vec<PendingApproval> {
        self.pending_map().values().cloned().collect()
    }
}

/// Whether a correction may be applied to the turn that is active now.
fn steer_allowed(
    conversation_id: &str,
    task_id: &str,
    active_conversation: &str,
    active_task: &str,
) -> Result<(), &'static str> {
    if active_conversation.is_empty() || active_task.is_empty() {
        return Err("no turn is running");
    }
    let wrong_conversation = !conversation_id.is_empty() && conversation_id != active_conversation;
    let wrong_task = !task_id.is_empty() && task_id != active_task;
    if wrong_conversation || wrong_task {
        return Err("that turn is not the one running");
    }
    Ok(())
}

/// Park for one explicit decision, but stop waiting as soon as the turn is
/// cancelled.
///
/// A pending approval is the longest pause in a turn, and a person pressing
/// stop is usually looking at exactly that pause, so a stop request has to end
/// the wait instead of leaving the turn parked until the approval timeout
/// expires. `None` means the turn was cancelled; the call must not run.
fn wait_for_approval(
    receiver: &Receiver<Approval>,
    timeout: Duration,
    cancel: Option<CancelToken<'_>>,
) -> Option<Approval> {
    let deadline = Instant::now() + timeout;
    loop {
        if cancel.is_some_and(|token| token.requested.load(Ordering::Relaxed)) {
            return None;
        }
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Some(Approval::Deny);
        }
        match receiver.recv_timeout(remaining.min(PUMP_INTERVAL)) {
            Ok(decision) => return Some(decision),
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => return Some(Approval::Deny),
        }
    }
}

/// Workspace path a governed call names, for the control ledger.
fn call_path(call: &ToolRequest) -> String {
    call.arguments
        .get("path")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

/// Durable conversation handle over the local runtime.
pub struct Conversation {
    text_block_boundaries: bool,
    speaker: String,
    engine_id: u64,
    client: RuntimeClient,
    cwd: PathBuf,
    host: HostPolicy,
    approvals: ApprovalInbox,
    router: ToolRouter,
    events: EventBus,
    model: Option<String>,
    approval_timeout: Duration,
    active_conversation: String,
    active_task: String,
    /// Durable product document whose control layer classifies every call.
    product: Option<Arc<Mutex<ProductState>>>,
    store: Option<crate::store::StateStore>,
    /// Corrections arriving while a turn runs, drained between runtime frames.
    steering: Option<mpsc::Receiver<SteerRequest>>,
    /// How to start a fresh engine after the transport dies. `None` means this
    /// conversation drives a runtime it cannot replace (one-shot runs, tests).
    respawn: Option<Box<dyn Fn() -> Command + Send>>,
    /// Where the engine keeps its durable session journal.
    storage_dir: PathBuf,
    /// Set when the engine stopped answering; the next turn restarts it first.
    transport_failed: bool,
    stopped: bool,
    restarts: u64,
    /// Request ids this conversation issued and has not consumed yet.
    ///
    /// A turn can end before every reply it asked for arrives: cancellation
    /// aborts a running task while that task's `durable/run` reply is still in
    /// flight, and the two replies cross on the wire. Those replies belong to
    /// the turn that asked for them, so the next turn drops them instead of
    /// failing on a response id it never used.
    outstanding: Vec<String>,
    buffered: VecDeque<Incoming>,
    /// Instructions a durable conversation opens with; `None` uses the default
    /// product posture. Set for a named marble's engine.
    instructions: Option<String>,
    /// Instructions computed per conversation, so an edited persona reaches
    /// the next conversation on a shared lane.
    instructions_fn: Option<std::sync::Arc<dyn Fn() -> String + Send + Sync>>,
}

impl Drop for Conversation {
    fn drop(&mut self) {
        self.clear_runtime_failure();
    }
}

impl Conversation {
    pub fn spawn(
        command: Command,
        cwd: &Path,
        storage_dir: &Path,
        approvals: ApprovalInbox,
    ) -> Result<Self, MarbleError> {
        let router = ToolRouter::new(cwd)?;
        Self::spawn_with_router(
            command,
            cwd,
            storage_dir,
            HostPolicy::default(),
            approvals,
            router,
            EventBus::default(),
        )
    }

    /// Spawn with an explicit host policy so deny paths stay testable.
    pub fn spawn_with_policy(
        command: Command,
        cwd: &Path,
        storage_dir: &Path,
        host: HostPolicy,
        approvals: ApprovalInbox,
    ) -> Result<Self, MarbleError> {
        let router = ToolRouter::new(cwd)?;
        Self::spawn_with_router(
            command,
            cwd,
            storage_dir,
            host,
            approvals,
            router,
            EventBus::default(),
        )
    }

    /// Full wiring: policy, workspace tool router, and the product event bus.
    pub fn spawn_with_router(
        mut command: Command,
        cwd: &Path,
        storage_dir: &Path,
        host: HostPolicy,
        approvals: ApprovalInbox,
        router: ToolRouter,
        events: EventBus,
    ) -> Result<Self, MarbleError> {
        command.current_dir(cwd);
        let mut client = RuntimeClient::spawn_command(command).map_err(MarbleError::Io)?;
        let init_id = client
            .request("initialize", value!({"protocolVersion":2,"requiredCapabilities":["mixedTools"],"optionalCapabilities":["textBlockBoundaries"],"storageDir":storage_dir}))
            .map_err(MarbleError::Io)?;
        let init = Self::wait_response(&mut client, &router, &events, &init_id)?;
        Self::require_capabilities(&init)?;
        Ok(Self {
            text_block_boundaries: init["capabilities"]["textBlockBoundaries"] == true,
            speaker: "Marble".into(),
            client,
            cwd: cwd.to_path_buf(),
            host,
            approvals,
            router,
            events,
            model: None,
            approval_timeout: Duration::from_secs(300),
            active_conversation: String::new(),
            active_task: String::new(),
            product: None,
            store: None,
            steering: None,
            respawn: None,
            storage_dir: storage_dir.to_path_buf(),
            transport_failed: false,
            stopped: false,
            restarts: 0,
            instructions: None,
            instructions_fn: None,
            outstanding: Vec::new(),
            buffered: VecDeque::new(),
            engine_id: {
                static NEXT_ENGINE_ID: AtomicU64 = AtomicU64::new(1);
                NEXT_ENGINE_ID.fetch_add(1, Ordering::Relaxed)
            },
        })
    }

    /// Fail closed unless the runtime negotiated protocol v2 mixed ownership
    /// together with the durable conversation contract.
    fn require_capabilities(init: &Value) -> Result<(), MarbleError> {
        let protocol = init
            .get("protocolVersion")
            .and_then(Value::as_u64)
            .unwrap_or_default();
        if protocol < glass_runtime_client::MIXED_PROTOCOL_VERSION {
            return Err(MarbleError::Protocol(format!(
                "glass-core runtime protocol {protocol} does not support mixed tool ownership"
            )));
        }
        let capabilities = init.get("capabilities").ok_or_else(|| {
            MarbleError::Protocol("glass-core runtime returned no capability set".into())
        })?;
        for capability in ["mixedTools", "durable", "resume"] {
            if capabilities.get(capability).and_then(Value::as_bool) != Some(true) {
                return Err(MarbleError::Protocol(format!(
                    "glass-core runtime capability {capability} is unavailable"
                )));
            }
        }
        Ok(())
    }

    /// Give the conversation the product document whose control layer decides
    /// what governed tools may run. Without it the host policy answers alone.
    pub fn with_product(mut self, product: Arc<Mutex<ProductState>>) -> Self {
        self.product = Some(product);
        self
    }

    pub(crate) fn product_state(&self) -> Option<Arc<Mutex<ProductState>>> {
        self.product.clone()
    }

    pub(crate) fn set_store(&mut self, store: Option<&crate::store::StateStore>) {
        self.store = store.cloned();
    }

    /// Supply the channel the HTTP surface uses to correct a running turn.
    pub fn with_steering(mut self, steering: mpsc::Receiver<SteerRequest>) -> Self {
        self.steering = Some(steering);
        self
    }

    /// Supply the recipe for starting the engine again.
    ///
    /// Marble is meant to stay up for months, and the engine is a separate
    /// process that can crash, be killed, or lose its pipe. Without a recipe the
    /// only recovery is the service manager restarting the whole assistant,
    /// which also loses the loaded document; with one, the next turn gets a
    /// fresh engine and the product never stops serving.
    pub fn with_respawn(mut self, factory: impl Fn() -> Command + Send + 'static) -> Self {
        self.respawn = Some(Box::new(factory));
        self
    }

    /// Whether the engine transport has failed since the last restart.
    pub fn transport_failed(&self) -> bool {
        self.transport_failed
    }

    /// How many times this conversation restarted its engine.
    pub fn restarts(&self) -> u64 {
        self.restarts
    }

    /// Replace a dead engine before the next turn, and report whether it did.
    ///
    /// Replacing is done ahead of a turn rather than in the middle of one: a
    /// turn that lost its transport is already unanswerable, and pretending
    /// otherwise would hand a person a reply that was never delivered.
    pub fn ensure_runtime(&mut self) -> Result<bool, MarbleError> {
        if !self.transport_failed {
            return Ok(false);
        }
        self.restart()?;
        Ok(true)
    }

    pub fn streams_text_blocks(&self) -> bool {
        self.text_block_boundaries
    }

    /// Spawn a fresh engine, negotiate capabilities, and adopt it.
    pub fn restart(&mut self) -> Result<(), MarbleError> {
        let Some(factory) = self.respawn.as_ref() else {
            return Err(MarbleError::Protocol(
                "the engine transport failed and this conversation has no way to restart it".into(),
            ));
        };
        let mut command = factory();
        command.current_dir(&self.cwd);
        let mut client = RuntimeClient::spawn_command(command).map_err(MarbleError::Io)?;
        let init_id = client
            .request("initialize", value!({"protocolVersion":2,"requiredCapabilities":["mixedTools"],"optionalCapabilities":["textBlockBoundaries"],"storageDir":self.storage_dir}))
            .map_err(MarbleError::Io)?;
        let init = Self::wait_response(&mut client, &self.router, &self.events, &init_id)?;
        Self::require_capabilities(&init)?;
        self.text_block_boundaries = init["capabilities"]["textBlockBoundaries"] == true;
        let previous = std::mem::replace(&mut self.client, client);
        drop(previous);
        self.set_active("", "");
        self.transport_failed = false;
        self.stopped = false;
        self.clear_runtime_failure();
        // The ids belonged to the engine that just went away.
        self.outstanding.clear();
        self.buffered.clear();
        self.restarts = self.restarts.saturating_add(1);
        tracing::warn!(
            restarts = self.restarts,
            "restarted the glass-core runtime after a transport failure"
        );
        Ok(())
    }

    /// Mark the engine as gone and describe why the turn cannot continue.
    fn transport_error(&mut self, error: MarbleError) -> MarbleError {
        self.transport_failed = true;
        MarbleError::Protocol(format!("the engine stopped answering: {error}"))
    }

    fn note_io<T>(&mut self, outcome: std::io::Result<T>) -> Result<T, MarbleError> {
        match outcome {
            Ok(value) => Ok(value),
            Err(error) => {
                self.transport_failed = true;
                Err(MarbleError::Io(error))
            }
        }
    }

    /// Remember one response this conversation still has to consume.
    fn note_issued(&mut self, id: String) {
        if self.outstanding.len() >= MAX_OUTSTANDING_RESPONSES {
            self.outstanding.remove(0);
        }
        self.outstanding.push(id);
    }

    /// Consume one response id; true when this conversation issued it.
    fn note_answered(&mut self, id: &str) -> bool {
        match self.outstanding.iter().position(|known| known == id) {
            Some(index) => {
                self.outstanding.remove(index);
                true
            }
            None => false,
        }
    }

    /// Send one request and remember the response id it will answer with.
    fn issue(&mut self, outcome: std::io::Result<String>) -> Result<String, MarbleError> {
        let id = self.note_io(outcome)?;
        self.note_issued(id.clone());
        Ok(id)
    }

    /// Select a model for new durable conversations.
    pub fn with_model(mut self, model: Option<String>) -> Self {
        self.model = model;
        self
    }

    /// Change the model used the next time a conversation is created. A live
    /// session keeps the model it started with.
    pub fn set_model(&mut self, model: Option<String>) {
        self.model = model;
    }

    /// Fix the instructions this session's conversations open with — a named
    /// marble's identity instead of the default product posture.
    pub fn with_instructions(mut self, instructions: String) -> Self {
        self.instructions = Some(instructions);
        self
    }

    pub fn with_speaker(mut self, name: &str) -> Self {
        self.speaker = name.to_string();
        self
    }

    pub(crate) fn clean_reply(&self, text: &str) -> String {
        strip_self_mention(text, &self.speaker).to_string()
    }

    /// Ask for the instructions each time a conversation is created, so an
    /// edited persona reaches the next conversation without dropping the lane.
    pub fn with_instructions_fn(
        mut self,
        instructions: impl Fn() -> String + Send + Sync + 'static,
    ) -> Self {
        self.instructions_fn = Some(std::sync::Arc::new(instructions));
        self
    }

    /// Override how long an unanswered approval waits before it is denied.
    pub fn with_approval_timeout(mut self, timeout: Duration) -> Self {
        self.approval_timeout = timeout;
        self
    }

    /// Create a durable conversation and return its id.
    pub fn create(&mut self) -> Result<String, MarbleError> {
        let mut options = SessionOptions::mixed(self.cwd.clone(), ToolRouter::catalog());
        options.instructions = Some(
            self.instructions_fn
                .as_ref()
                .map(|instructions| instructions())
                .or_else(|| self.instructions.clone())
                .unwrap_or_else(|| default_assistant_instructions("", &self.cwd)),
        );
        options.model = self.model.clone();
        let created = self.client.durable_create(&options);
        let create_id = self.issue(created)?;
        let result = self.pump(&create_id, Instant::now() + TURN_TIMEOUT, None, &mut |_| {})?;
        let conversation_id = result["conversationId"]
            .as_str()
            .unwrap_or_default()
            .to_string();
        if conversation_id.is_empty() {
            return Err(MarbleError::Protocol(
                "durable/create returned no conversation id".into(),
            ));
        }
        self.set_active(&conversation_id, "");
        Ok(conversation_id)
    }

    /// Admit one durable request and return its task id.
    pub fn submit(
        &mut self,
        conversation_id: &str,
        request_id: &str,
        input: &TurnInput,
    ) -> Result<String, MarbleError> {
        self.set_active(conversation_id, "");
        let record = self.product.as_ref().and_then(|p| {
            p.lock()
                .unwrap_or_else(|e| e.into_inner())
                .conversations
                .iter()
                .find(|c| c.id == conversation_id)
                .cloned()
        });
        let mut engine_id = record
            .as_ref()
            .filter(|c| !c.engine_conversation_id.is_empty())
            .map(|c| c.engine_conversation_id.clone())
            .unwrap_or_else(|| conversation_id.to_string());
        let mut input = input.clone();
        let retrying = record.as_ref().is_some_and(|record| {
            self.product.as_ref().is_some_and(|p| {
                p.lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .tasks
                    .iter()
                    .find(|t| t.id == record.retry_task_id)
                    .is_some_and(|t| {
                        input
                            .text
                            .ends_with(&format!("[User message]\n{}", t.prompt))
                    })
            })
        });
        if let Some(record) = &record
            && retrying
        {
            let latest = self.product.as_ref().is_some_and(|p| {
                p.lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .tasks
                    .iter()
                    .rev()
                    .find(|t| {
                        t.conversation_id == conversation_id
                            && t.status != "queued"
                            && !t.id.starts_with("queued-")
                    })
                    .is_some_and(|t| t.id == record.retry_task_id)
            });
            if let Some(sequence) = record
                .checkpoints
                .get(&record.retry_task_id)
                .filter(|_| latest)
            {
                let request = self.client.request(
                    "durable/fork",
                    value!({"conversationId":engine_id,"asOf":sequence}),
                );
                let id = self.issue(request)?;
                let result = self.pump(&id, Instant::now() + TURN_TIMEOUT, None, &mut |_| {})?;
                engine_id = result["conversationId"]
                    .as_str()
                    .filter(|id| !id.is_empty())
                    .ok_or_else(|| {
                        MarbleError::Protocol("durable/fork returned no conversation id".into())
                    })?
                    .to_string();
            } else {
                input.text = input.text.replacen("[Marble host context]\n", "[Marble host context]\nThis is a retry of an earlier failed message; later turns remain in the conversation history.\n", 1);
            }
        }
        let inspect = self.client.request(
            "durable/inspect",
            value!({"conversationId":engine_id,"limit":1}),
        );
        let inspect_id = self.issue(inspect)?;
        let snapshot = self.pump(
            &inspect_id,
            Instant::now() + TURN_TIMEOUT,
            None,
            &mut |_| {},
        )?;
        let checkpoint = snapshot["sequence"]
            .as_u64()
            .ok_or_else(|| MarbleError::Protocol("durable/inspect returned no sequence".into()))?;
        let submitted = self.client.durable_submit(&engine_id, request_id, &input);
        let submit_id = self.issue(submitted)?;
        let result = self.pump(&submit_id, Instant::now() + TURN_TIMEOUT, None, &mut |_| {})?;
        let task_id = result["submission"]["id"]
            .as_str()
            .unwrap_or_default()
            .to_string();
        if task_id.is_empty() {
            return Err(MarbleError::Protocol(
                "durable/submit returned no task id".into(),
            ));
        }
        if let Some(product) = &self.product {
            let mut state = product.lock().unwrap_or_else(|e| e.into_inner());
            let record = state.conversation_mut(conversation_id);
            record.engine_conversation_id = engine_id;
            record.checkpoints.insert(task_id.clone(), checkpoint);
            if retrying {
                record.retry_task_id.clear();
            }
        }
        self.set_active(conversation_id, &task_id);
        Ok(task_id)
    }

    /// Record one Core-native tool call in the product's tool inventory.
    ///
    /// Core executes its own coding tools, so they never reach the control
    /// layer. They do arrive as runtime events, and a person deciding how much
    /// to trust a marble should be able to see what it actually reached for
    /// rather than only the integrations that asked permission first.
    fn note_engine_tool(&self, event: &RuntimeEvent) {
        let Some(product) = &self.product else {
            return;
        };
        let name = event
            .data
            .get("name")
            .or_else(|| event.data.get("tool"))
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|name| !name.is_empty());
        let Some(name) = name else {
            return;
        };
        let failed = event
            .data
            .get("isError")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let mut state = product.lock().unwrap_or_else(|error| error.into_inner());
        match event.kind.as_str() {
            "tool_start" => state.note_engine_tool(name),
            "tool_end" if failed => state.note_engine_tool_error(name),
            _ => return,
        }
        state.bump();
    }

    /// Publish one product event for the active conversation and task.
    pub fn publish(&self, kind: &str, mut data: Value) {
        if kind == "error" {
            let failure =
                crate::failure::FailureInfo::of(data["message"].as_str().unwrap_or_default());
            data["message"] = value!(crate::failure::MESSAGE);
            data["failure"] = value!(failure);
        }
        let mut everyone = None;
        if matches!(kind, "tool_start" | "tool_end")
            && let Some(product) = &self.product
        {
            let state = product.lock().unwrap_or_else(|error| error.into_inner());
            let call = data["callId"].as_str().unwrap_or_default();
            let id = format!("entry-tool-{}-{call}", self.active_task);
            if let Some(row) = state.transcript.iter().find(|e| e.id == id) {
                data["entry_id"] = value!(row.id);
                data["sender"] = value!(row.speaker_name);
            }
            if let Some(row) = state
                .transcript
                .iter()
                .find(|e| e.id == format!("{id}-everyone"))
            {
                let mut mirrored = data.clone();
                mirrored["entry_id"] = value!(row.id);
                mirrored["sender"] = value!(row.speaker_name);
                everyone = Some(mirrored);
            }
        }
        self.events
            .publish(&self.active_conversation, &self.active_task, kind, data);
        if let Some(data) = everyone {
            self.events.publish(
                crate::everyone::EVERYONE_CONVERSATION,
                &self.active_task,
                kind,
                data,
            );
        }
    }

    pub fn set_active(&mut self, conversation_id: &str, task_id: &str) {
        self.active_conversation = conversation_id.to_string();
        self.active_task = task_id.to_string();
    }

    pub fn recv_typed_timeout(
        &mut self,
        timeout: Duration,
    ) -> Result<Option<Incoming>, MarbleError> {
        if let Some(incoming) = self.buffered.pop_front() {
            return Ok(Some(incoming));
        }
        self.client
            .recv_typed_timeout(timeout)
            .map_err(MarbleError::Io)
    }

    pub(crate) fn check_idle_transport(&mut self, state: &Mutex<ProductState>) {
        if self.stopped || self.transport_failed || self.buffered.len() >= MAX_OUTSTANDING_RESPONSES
        {
            return;
        }
        match self.client.recv_typed_timeout(Duration::ZERO) {
            Ok(Some(incoming)) => self.buffered.push_back(incoming),
            Ok(None) => {}
            Err(error) => {
                let error = self.transport_error(MarbleError::Io(error));
                let mut guard = state.lock().unwrap_or_else(|error| error.into_inner());
                guard
                    .presence
                    .runtime
                    .note_engine_failure(self.engine_id, error.to_string());
                guard.bump();
                self.publish(
                    "runtime",
                    value!({"error": error.to_string(), "revision": guard.revision}),
                );
            }
        }
    }

    pub(crate) fn engine_id(&self) -> u64 {
        self.engine_id
    }

    /// Start one durable task; returns the runtime response id to await.
    pub fn start(&mut self, task_id: &str) -> Result<String, MarbleError> {
        let started = self.client.durable_run(Some(task_id));
        self.issue(started)
    }

    /// Request cancellation of one durable task.
    pub fn abort(&mut self, task_id: &str) -> Result<String, MarbleError> {
        let aborted = self.client.durable_abort(task_id, false);
        self.issue(aborted)
    }

    /// Drain events and tool requests until the correlated response arrives.
    pub fn pump(
        &mut self,
        expected: &str,
        deadline: Instant,
        cancel: Option<CancelToken<'_>>,
        sink: &mut dyn FnMut(&RuntimeEvent),
    ) -> Result<Value, MarbleError> {
        let mut auxiliary: Vec<String> = Vec::new();
        let mut cancelled = false;
        let mut tools = ToolRows::default();
        loop {
            self.drain_steering(&mut auxiliary);
            if let Some(token) = cancel
                && token.requested.load(Ordering::Relaxed)
                && !cancelled
            {
                cancelled = true;
                self.publish(
                    "cancel_requested",
                    value!({"taskId": token.task_id, "requestId": expected}),
                );
                match self.abort(token.task_id) {
                    Ok(id) if auxiliary.len() < MAX_AUXILIARY_RESPONSES => auxiliary.push(id),
                    Ok(_) => {}

                    Err(error) => {
                        self.publish("error", value!({"message": error.to_string()}));
                    }
                }
            }
            let incoming = self.recv_typed_timeout(PUMP_INTERVAL);
            let incoming = match incoming {
                Ok(incoming) => incoming,
                Err(error) => return Err(self.transport_error(error)),
            };
            let Some(incoming) = incoming else {
                if Instant::now() >= deadline {
                    self.publish(
                        "error",
                        value!({"message": format!("timed out waiting for runtime response {expected}")}),
                    );
                    return Err(MarbleError::Protocol(format!(
                        "timed out waiting for runtime response {expected}"
                    )));
                }
                continue;
            };
            match incoming {
                Incoming::Response { id, result } => {
                    if id.as_str() == Some(expected) {
                        self.note_answered(expected);
                        return result.map_err(|error| {
                            MarbleError::Protocol(format!(
                                "runtime error {}: {}",
                                error.code, error.message
                            ))
                        });
                    }
                    let text = id
                        .as_str()
                        .map(str::to_owned)
                        .unwrap_or_else(|| id.to_string());
                    if auxiliary.contains(&text) || self.note_answered(&text) {
                        // A reply from a turn that already ended — a cancelled
                        // run settling, or the abort that raced it — is not an
                        // answer to this turn, and must not fail it.
                        tracing::debug!(
                            response = %text,
                            "dropped a runtime response from an earlier turn"
                        );
                        continue;
                    }
                    return Err(MarbleError::Protocol(format!(
                        "unexpected runtime response id {id:?}"
                    )));
                }
                Incoming::Event(mut event) => {
                    tools.normalize(&mut event);
                    if matches!(event.kind.as_str(), "tool_start" | "tool_end")
                        && ToolRouter::catalog()
                            .iter()
                            .any(|tool| event.data["tool"] == tool.name)
                    {
                        continue;
                    }
                    if tools.observe(&event) {
                        self.record_tool_rows(&tools);
                    }
                    self.note_engine_tool(&event);
                    if !matches!(
                        event.kind.as_str(),
                        "text" | "reasoning" | "text_block_start"
                    ) {
                        self.publish(&event.kind, event.data.clone());
                    }
                    sink(&event);
                }
                Incoming::Tool { request_id, call } => {
                    let start = RuntimeEvent {
                        session_id: call.session_id.clone(),
                        turn_id: call.turn_id.clone(),
                        kind: "tool_start".into(),
                        data: value!({"requestId":request_id.as_str().map(str::to_owned).unwrap_or_else(|| request_id.to_string()), "callId": call.call_id, "tool": call.name, "arguments": call.arguments}),
                    };
                    tools.observe(&start);
                    self.record_tool_rows(&tools);
                    self.publish(&start.kind, start.data.clone());
                    sink(&start);
                    let data = self.handle_tool(request_id, &call, cancel)?;
                    let end = RuntimeEvent {
                        kind: "tool_end".into(),
                        data,
                        ..start
                    };
                    tools.observe(&end);
                    self.record_tool_rows(&tools);
                    self.publish(&end.kind, end.data.clone());
                    sink(&end);
                }
                Incoming::Unknown(_) => {}
            }
        }
    }

    fn record_tool_rows(&self, tools: &ToolRows) {
        let Some(product) = &self.product else { return };
        if self.active_task.is_empty() {
            return;
        }
        let mut state = product.lock().unwrap_or_else(|error| error.into_inner());
        let shared = state
            .everyone_pending
            .contains_key(&format!("task:{}", self.active_task));
        let existing: Vec<_> = state
            .transcript
            .iter()
            .filter(|entry| {
                (entry.conversation_id == self.active_conversation
                    || (shared && entry.conversation_id == crate::everyone::EVERYONE_CONVERSATION))
                    && entry.task_id == self.active_task
                    && entry.kind == "tool"
            })
            .cloned()
            .collect();
        state
            .transcript
            .retain(|entry| !existing.iter().any(|previous| previous.id == entry.id));
        for (call, text, _) in &tools.rows {
            let id = format!("entry-tool-{}-{call}", self.active_task);
            let previous = existing
                .iter()
                .find(|e| e.id == id)
                .cloned()
                .unwrap_or_default();
            state.push_transcript(crate::TranscriptEntry {
                id,
                conversation_id: self.active_conversation.clone(),
                task_id: self.active_task.clone(),
                kind: "tool".into(),
                text: text.clone(),
                at: if previous.at == 0 {
                    crate::worker::now_millis()
                } else {
                    previous.at
                },
                ..previous
            });
            if shared {
                let mirror_id = format!("entry-tool-{}-{call}-everyone", self.active_task);
                let previous = existing
                    .iter()
                    .find(|e| e.id == mirror_id)
                    .cloned()
                    .unwrap_or_default();
                let speaker = state
                    .everyone_pending
                    .get(&format!("task:{}", self.active_task))
                    .cloned()
                    .unwrap_or_default();
                state.push_transcript(crate::TranscriptEntry {
                    id: mirror_id,
                    conversation_id: crate::everyone::EVERYONE_CONVERSATION.into(),
                    task_id: self.active_task.clone(),
                    kind: "tool".into(),
                    text: text.clone(),
                    at: if previous.at == 0 {
                        crate::worker::now_millis()
                    } else {
                        previous.at
                    },
                    speaker,
                    ..previous
                });
            }
        }
        state.bump();
        let snapshot = state.clone();
        drop(state);
        crate::store::persist_optional(self.store.as_ref(), &snapshot);
    }

    /// Apply every queued correction to the turn that is running now. A request
    /// aimed at a different turn is rejected in a product event rather than
    /// silently steered into whatever happens to be active.
    fn drain_steering(&mut self, auxiliary: &mut Vec<String>) {
        while let Some(request) = self
            .steering
            .as_ref()
            .and_then(|channel| channel.try_recv().ok())
        {
            if let Err(reason) = steer_allowed(
                &request.conversation_id,
                &request.task_id,
                &self.active_conversation,
                &self.active_task,
            ) {
                self.publish(
                    "steer_rejected",
                    value!({
                        "reason": reason,
                        "conversationId": request.conversation_id,
                        "taskId": request.task_id,
                    }),
                );
                continue;
            }
            let input = crate::to_turn_input(&crate::InboundMessage::new(&request.text));
            match self.client.steer(&self.active_conversation, &input) {
                Ok(id) => {
                    self.note_issued(id.clone());
                    if auxiliary.len() < MAX_AUXILIARY_RESPONSES {
                        auxiliary.push(id);
                    }
                    self.publish(
                        "steered",
                        value!({
                            "text": request.text,
                            "conversationId": self.active_conversation,
                            "taskId": self.active_task,
                        }),
                    );
                }
                Err(error) => self.publish(
                    "steer_rejected",
                    value!({"reason": error.to_string(), "text": request.text}),
                ),
            }
        }
    }

    fn handle_tool(
        &mut self,
        request_id: Value,
        call: &ToolRequest,
        cancel: Option<CancelToken<'_>>,
    ) -> Result<Value, MarbleError> {
        let key = request_id
            .as_str()
            .map(str::to_owned)
            .unwrap_or_else(|| request_id.to_string());
        let scope = ToolScope::new(self.active_conversation.clone(), self.active_task.clone());
        let (approval, reason) = self.decide(&key, call, &scope, cancel);
        let answer = crate::host::answer_tool(
            &mut self.client,
            request_id,
            call,
            approval,
            &self.router,
            &scope,
            (!reason.is_empty()).then_some(reason.as_str()),
            cancel,
        )?;
        let (approved, outcome) = match answer {
            crate::host::ToolAnswer::Answered(outcome) => (approval == Approval::Approve, outcome),
            crate::host::ToolAnswer::Stopped(outcome) => {
                // A stop request ended this call while it ran, so the ledger says
                // it was refused and the surface shows an errored call.
                self.record(
                    &call.name,
                    &call_path(call),
                    "denied",
                    crate::host::STOPPED_TOOL,
                );
                (false, outcome)
            }
        };
        let data = value!({
            "requestId": key,
            "callId": call.call_id,
            "tool": call.name,
            "approved": approved,
            "isError": outcome.is_error,
            "text": outcome.text,
        });
        Ok(data)
    }

    /// Classify one governed call through marble's control layer, falling back
    /// to the host posture when no product document is attached (one-shot runs).
    fn decide(
        &mut self,
        key: &str,
        call: &ToolRequest,
        scope: &ToolScope,
        cancel: Option<CancelToken<'_>>,
    ) -> (Approval, String) {
        let path = call_path(call);
        let verdict = self.verdict(&call.name, &path);
        let (approval, decision, reason) = match verdict {
            Verdict::Allow { reason } => {
                self.record(&call.name, &path, "allowed", &reason);
                (Approval::Approve, "allowed", reason)
            }
            Verdict::Deny { reason } => {
                self.record(&call.name, &path, "denied", &reason);
                self.publish(
                    "control_denied",
                    value!({
                        "requestId": key,
                        "callId": call.call_id,
                        "tool": call.name,
                        "reason": reason,
                    }),
                );
                (Approval::Deny, "denied", reason)
            }
            Verdict::Ask => {
                if !self.host.prompts() {
                    let decision = self.host.decision();
                    let (decision_label, reason) = match decision {
                        Approval::Approve => ("allowed", "host policy approves governed tools"),
                        Approval::Deny => ("denied", "host policy declines governed tools"),
                    };
                    self.record(&call.name, &path, decision_label, reason);
                    (decision, decision_label, reason.to_string())
                } else {
                    // Another chat may have read or written this path while
                    // this turn was getting here. The card says so before the
                    // person decides, and the write itself refuses a stale
                    // base, so one chat's change is never silently lost.
                    let conflict = self.router.conflict(scope, &call.name, &call.arguments);
                    let conflict_note = conflict.as_ref().map(|conflict| conflict.note.clone());
                    self.publish(
                        "approval_pending",
                        value!({
                            "requestId": key,
                            "callId": call.call_id,
                            "tool": call.name,
                            "arguments": call.arguments,
                            "conflict": conflict_note.clone(),
                        }),
                    );
                    let receiver = self.approvals.register(
                        key,
                        &call.call_id,
                        &call.name,
                        call.arguments.clone(),
                        conflict_note,
                    );
                    let decision = match wait_for_approval(&receiver, self.approval_timeout, cancel)
                    {
                        Some(decision) => decision,
                        None => {
                            // The turn was stopped while this call was parked:
                            // clear the pending entry, answer the runtime so it
                            // can settle, and let the caller abort the task.
                            self.approvals.resolve(key, Approval::Deny);
                            self.publish(
                                "approval_resolved",
                                value!({"requestId": key, "approved": false}),
                            );
                            self.record(&call.name, &path, "denied", CANCELLED_APPROVAL);
                            return (Approval::Deny, CANCELLED_APPROVAL.to_string());
                        }
                    };
                    self.approvals.resolve(key, decision);
                    self.publish(
                        "approval_resolved",
                        value!({"requestId": key, "approved": decision == Approval::Approve}),
                    );
                    let (decision_label, reason) = match decision {
                        Approval::Approve => ("approved", "approved on this surface"),
                        Approval::Deny => ("denied", "declined on this surface"),
                    };
                    self.record(&call.name, &path, decision_label, reason);
                    (decision, decision_label, reason.to_string())
                }
            }
        };
        if decision == "allowed" {
            self.note_grant_use(&call.name, &path);
        }
        (approval, reason)
    }

    fn verdict(&self, tool: &str, path: &str) -> Verdict {
        match &self.product {
            Some(product) => {
                let state = product.lock().unwrap_or_else(|error| error.into_inner());
                state.control.classify(tool, path)
            }
            None => Verdict::Ask,
        }
    }

    fn record(&self, tool: &str, path: &str, decision: &str, reason: &str) {
        let Some(product) = &self.product else {
            return;
        };
        let mut state = product.lock().unwrap_or_else(|error| error.into_inner());
        state.control.note(
            &self.active_conversation,
            &self.active_task,
            tool,
            path,
            decision,
            reason,
        );
        state.bump();
    }

    fn note_grant_use(&self, tool: &str, path: &str) {
        let Some(product) = &self.product else {
            return;
        };
        let mut state = product.lock().unwrap_or_else(|error| error.into_inner());
        state.control.use_grants(tool, path);
    }

    pub fn shutdown(&mut self) -> Result<(), MarbleError> {
        self.stopped = true;
        self.clear_runtime_failure();
        let shutdown_id = self
            .client
            .request("shutdown", json!({}))
            .map_err(MarbleError::Io)?;
        let _ = Self::wait_response_on(&mut self.client, &self.router, &self.events, &shutdown_id);
        Ok(())
    }

    fn clear_runtime_failure(&self) {
        if let Some(state) = &self.product {
            let mut guard = state.lock().unwrap_or_else(|error| error.into_inner());
            if guard.presence.runtime.forget_engine(self.engine_id) {
                guard.bump();
                self.publish("runtime", value!({"revision": guard.revision, "health": guard.presence.runtime.document()}));
            }
        }
    }

    pub fn host(&self) -> &HostPolicy {
        &self.host
    }

    pub fn client_mut(&mut self) -> &mut RuntimeClient {
        &mut self.client
    }

    pub fn approvals(&self) -> &ApprovalInbox {
        &self.approvals
    }

    pub fn router(&self) -> &ToolRouter {
        &self.router
    }

    pub fn events(&self) -> &EventBus {
        &self.events
    }

    fn wait_response(
        client: &mut RuntimeClient,
        router: &ToolRouter,
        events: &EventBus,
        expected: &str,
    ) -> Result<Value, MarbleError> {
        Self::wait_response_on(client, router, events, expected)
    }

    fn wait_response_on(
        client: &mut RuntimeClient,
        router: &ToolRouter,
        events: &EventBus,
        expected: &str,
    ) -> Result<Value, MarbleError> {
        let deadline = Instant::now() + TURN_TIMEOUT;
        loop {
            let incoming = client
                .recv_typed_timeout(PUMP_INTERVAL)
                .map_err(MarbleError::Io)?;
            let Some(incoming) = incoming else {
                if Instant::now() >= deadline {
                    return Err(MarbleError::Protocol(format!(
                        "timed out waiting for runtime response {expected}"
                    )));
                }
                continue;
            };
            match incoming {
                Incoming::Response { id, result } => {
                    if id.as_str() != Some(expected) {
                        return Err(MarbleError::Protocol(format!(
                            "unexpected runtime response id {id:?}"
                        )));
                    }
                    return result.map_err(|error| {
                        MarbleError::Protocol(format!(
                            "runtime error {}: {}",
                            error.code, error.message
                        ))
                    });
                }
                Incoming::Event(event) => {
                    events.publish("", "", &event.kind, event.data);
                }
                Incoming::Tool { request_id, call } => {
                    let key = request_id
                        .as_str()
                        .map(str::to_owned)
                        .unwrap_or_else(|| request_id.to_string());
                    events.publish(
                        "",
                        "",
                        "tool_start",
                        value!({"requestId": key, "tool": call.name}),
                    );
                    let outcome = crate::host::answer_tool_request(
                        client,
                        request_id,
                        &call,
                        Approval::Deny,
                        router,
                    )?;
                    events.publish(
                        "",
                        "",
                        "tool_end",
                        value!({"tool": call.name, "isError": outcome.is_error, "text": outcome.text}),
                    );
                }
                Incoming::Unknown(_) => {}
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_tool_ends_keep_the_start_name_and_failure_status() {
        let mut tools = ToolRows::default();
        let mut event = RuntimeEvent {
            session_id: "s".into(),
            turn_id: "t".into(),
            kind: "tool_start".into(),
            data: value!({"callId":"date", "name":"bash", "title":"date"}),
        };
        tools.normalize(&mut event);
        assert!(tools.observe(&event));
        event.kind = "tool_end".into();
        event.data = value!({"callId":"date", "isError":true, "text":"error: refused"});
        tools.normalize(&mut event);
        assert_eq!(event.data["tool"], "bash");
        assert!(tools.observe(&event));
        assert_eq!(tools.rows.len(), 1);
        assert_eq!(tools.rows[0].1, "bash · date · failed");
        event.data = value!({"callId":"unknown", "isError":false});
        tools.normalize(&mut event);
        assert_eq!(event.data["tool"], "tool");
        event.data = value!({});
        tools.normalize(&mut event);
        assert!(!tools.observe(&event));
        assert_eq!(tools.rows.len(), 1);
    }

    #[test]
    fn identity_and_brevity_are_shared_without_conflicting_names() {
        for persona in ["", "warm and witty"] {
            let prompt = default_assistant_instructions(persona, Path::new("."));
            assert!(prompt.starts_with("You are Marble."));
            assert!(prompt.contains("Keep reasoning and planning private"));
            assert!(prompt.contains(CONVERSATIONAL_GUIDANCE));
            assert!(prompt.contains(REPLY_PRIVACY_GUIDANCE));
            assert!(!prompt.contains("You are Glass Marble"));
        }
        let mut roster = crate::PrismState::default();
        let ada = roster
            .add("Ada", "writer", "star", 35, "", "brief")
            .unwrap()
            .clone();
        let prompt = marble_session_instructions(&ada, &roster.marbles, Path::new("."));
        assert!(prompt.starts_with("You are Ada, one of the user's Marbles. Speak only as Ada."));
        assert!(prompt.contains("Never @mention yourself"));
        assert!(prompt.contains(CONVERSATIONAL_GUIDANCE));
        assert!(!prompt.contains("You are Glass Marble"));
        assert!(!prompt.contains("You are Marble"));
    }

    #[test]
    fn reply_cleanup_only_removes_a_leading_own_name() {
        for (text, expected) in [
            ("@Ada I'll take the lead", "I'll take the lead"),
            ("  @ada: Hello", "Hello"),
            ("@Ada", ""),
            ("", ""),
            ("@Ben hello", "@Ben hello"),
            ("Ask @Ada", "Ask @Ada"),
            ("@Adaline hello", "@Adaline hello"),
            ("@Ada-team hello", "@Ada-team hello"),
            ("  Normal reply", "  Normal reply"),
        ] {
            assert_eq!(strip_self_mention(text, "Ada"), expected);
        }
        assert_eq!(strip_self_mention("@Éva Hello", "Éva"), "Hello");
    }

    #[test]
    fn tool_rows_keep_bounded_inputs_and_native_or_host_outcomes() {
        let mut tools = ToolRows::default();
        let mut event = RuntimeEvent {
            session_id: "s".into(),
            turn_id: value!("t"),
            kind: "text".into(),
            data: value!({}),
        };
        assert!(!tools.observe(&event));
        event.kind = "tool_start".into();
        assert!(!tools.observe(&event));
        event.data =
            value!({"callId":"search", "name":"web_search", "arguments":{"query":"latest\n news"}});
        assert!(tools.observe(&event));
        assert_eq!(tools.rows[0].1, "web_search · latest news · running");
        event.kind = "tool_end".into();
        event.data = value!({"callId":"search", "isError":false});
        tools.observe(&event);
        assert_eq!(tools.rows[0].1, "web_search · latest news · ok");
        event.kind = "tool_start".into();
        event.data = value!({"callId":"read", "tool":"glass.workspace.read", "arguments":{"path":"é".repeat(200)}});
        tools.observe(&event);
        assert!(tools.rows[1].1.chars().count() < 160);
        event.kind = "tool_end".into();
        event.data = value!({"callId":"read", "isError":true});
        tools.observe(&event);
        assert!(tools.rows[1].1.ends_with(" · failed"));
        assert!(default_assistant_instructions("", Path::new(".")).contains(TOOL_GUIDANCE));
    }

    fn capabilities(durable: bool, resume: bool, mixed: bool) -> serde_json::Value {
        serde_json::json!({
            "durable": durable,
            "resume": resume,
            "mixedTools": mixed,
        })
    }

    #[test]
    fn a_correction_only_matches_the_active_turn() {
        assert!(steer_allowed("", "", "c1", "t1").is_ok());
        assert!(steer_allowed("c1", "t1", "c1", "t1").is_ok());
        assert_eq!(
            steer_allowed("c2", "", "c1", "t1"),
            Err("that turn is not the one running")
        );
        assert_eq!(
            steer_allowed("", "t2", "c1", "t1"),
            Err("that turn is not the one running")
        );
        assert_eq!(steer_allowed("", "", "", ""), Err("no turn is running"));
    }

    #[test]
    fn a_decided_approval_is_resolved_exactly_once() {
        let inbox = ApprovalInbox::default();
        let receiver = inbox.register(
            "host:1",
            "call-1",
            "glass.workspace.list",
            Value::Null,
            None,
        );
        assert_eq!(inbox.pending().len(), 1);
        assert!(inbox.resolve("host:1", Approval::Approve));
        assert_eq!(receiver.recv().unwrap(), Approval::Approve);
        assert!(inbox.pending().is_empty());
        assert!(!inbox.resolve("host:1", Approval::Approve));
    }

    #[test]
    fn a_stop_request_ends_an_approval_wait_promptly() {
        let (_sender, receiver) = mpsc::channel::<Approval>();
        let requested = AtomicBool::new(false);
        let cancel = CancelToken {
            requested: &requested,
            task_id: "task-1",
        };
        let started = Instant::now();
        let decision = std::thread::scope(|scope| {
            scope.spawn(|| {
                std::thread::sleep(Duration::from_millis(50));
                requested.store(true, Ordering::Relaxed);
            });
            wait_for_approval(&receiver, Duration::from_secs(60), Some(cancel))
        });
        assert_eq!(decision, None, "a stop request ends the wait undecided");
        assert!(
            started.elapsed() < Duration::from_secs(5),
            "the wait must end on the stop request, not on its timeout: {:?}",
            started.elapsed()
        );
    }

    #[test]
    fn an_approval_wait_still_takes_a_decision_and_still_times_out() {
        let (sender, receiver) = mpsc::channel::<Approval>();
        sender.send(Approval::Approve).unwrap();
        assert_eq!(
            wait_for_approval(&receiver, Duration::from_secs(5), None),
            Some(Approval::Approve)
        );
        let (_sender, receiver) = mpsc::channel::<Approval>();
        assert_eq!(
            wait_for_approval(&receiver, Duration::ZERO, None),
            Some(Approval::Deny)
        );
    }

    #[test]
    fn initialize_capabilities_must_advertise_mixed_and_durable_sessions() {
        assert!(
            Conversation::require_capabilities(&serde_json::json!({
                "protocolVersion": 2,
                "capabilities": capabilities(true, true, true)
            }))
            .is_ok()
        );
        for value in [
            serde_json::json!({"protocolVersion": 2, "capabilities": capabilities(false, true, true)}),
            serde_json::json!({"protocolVersion": 2, "capabilities": capabilities(true, false, true)}),
            serde_json::json!({"protocolVersion": 2, "capabilities": capabilities(true, true, false)}),
            serde_json::json!({"protocolVersion": 1, "capabilities": capabilities(true, true, true)}),
            serde_json::json!({}),
        ] {
            assert!(
                Conversation::require_capabilities(&value).is_err(),
                "{value} should be rejected"
            );
        }
    }
}
