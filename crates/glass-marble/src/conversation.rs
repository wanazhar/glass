//! Durable conversation lifecycle over the Glass host runtime client.

use crate::{
    Approval, MarbleError, TURN_TIMEOUT, control::Verdict, events::EventBus, host::HostPolicy,
    state::PendingApproval, state::ProductState, tools::ToolRouter,
};
use glass_runtime_client::{
    Incoming, RuntimeClient, RuntimeEvent, SessionOptions, TurnInput, json, protocol::ToolRequest,
};
use serde_json::{Value, json as value};
use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
    mpsc::{self, Receiver},
};
use std::time::{Duration, Instant};

/// How often a running turn drains the next runtime frame so cancel, text
/// deltas, and tool activity reach the UI promptly.
pub const PUMP_INTERVAL: Duration = Duration::from_millis(200);
/// Bound on auxiliary responses (for example a cancel acknowledgement) a turn
/// may observe before its own response arrives.
const MAX_AUXILIARY_RESPONSES: usize = 8;

/// Product instructions for a marble conversation.
pub const SESSION_INSTRUCTIONS: &str = "You are Glass Marble, a durable collaborator working inside the \
     user's workspace. Core executes your file and shell tools; the `glass.workspace.*` tools are \
     governed integrations that Marble's own control layer classifies before they run, and the human \
     answers anything that layer leaves open. A refusal names the rule behind it, so adapt instead of \
     retrying the same call. Keep changes legible and reversible, say what you changed, and surface \
     attribution when you act on the human's behalf.";

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
    ) -> Receiver<Approval> {
        let (sender, receiver) = mpsc::channel();
        self.pending_map().insert(
            request_id.to_string(),
            PendingApproval {
                request_id: request_id.to_string(),
                call_id: call_id.to_string(),
                tool: tool.to_string(),
                arguments,
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

/// Durable conversation handle over the local runtime.
pub struct Conversation {
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
    /// Corrections arriving while a turn runs, drained between runtime frames.
    steering: Option<mpsc::Receiver<SteerRequest>>,
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
            .initialize_mixed(storage_dir)
            .map_err(MarbleError::Io)?;
        let init = Self::wait_response(&mut client, &router, &events, &init_id)?;
        Self::require_capabilities(&init)?;
        Ok(Self {
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
            steering: None,
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

    /// Supply the channel the HTTP surface uses to correct a running turn.
    pub fn with_steering(mut self, steering: mpsc::Receiver<SteerRequest>) -> Self {
        self.steering = Some(steering);
        self
    }

    /// Select a model for new durable conversations.
    pub fn with_model(mut self, model: Option<String>) -> Self {
        self.model = model;
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
        options.instructions = Some(SESSION_INSTRUCTIONS.to_string());
        options.model = self.model.clone();
        let create_id = self
            .client
            .durable_create(&options)
            .map_err(MarbleError::Io)?;
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
        let submit_id = self
            .client
            .durable_submit(conversation_id, request_id, input)
            .map_err(MarbleError::Io)?;
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
        self.set_active(conversation_id, &task_id);
        Ok(task_id)
    }

    /// Start one durable task; returns the runtime response id to await.
    pub fn start(&mut self, task_id: &str) -> Result<String, MarbleError> {
        self.client
            .durable_run(Some(task_id))
            .map_err(MarbleError::Io)
    }

    /// Request cancellation of one durable task.
    pub fn abort(&mut self, task_id: &str) -> Result<String, MarbleError> {
        self.client
            .durable_abort(task_id, false)
            .map_err(MarbleError::Io)
    }

    /// Publish one product event for the active conversation and task.
    pub fn publish(&self, kind: &str, data: Value) {
        self.events
            .publish(&self.active_conversation, &self.active_task, kind, data);
    }

    pub fn set_active(&mut self, conversation_id: &str, task_id: &str) {
        self.active_conversation = conversation_id.to_string();
        self.active_task = task_id.to_string();
    }

    pub fn recv_typed_timeout(
        &mut self,
        timeout: Duration,
    ) -> Result<Option<Incoming>, MarbleError> {
        self.client
            .recv_typed_timeout(timeout)
            .map_err(MarbleError::Io)
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
            let incoming = self.recv_typed_timeout(PUMP_INTERVAL)?;
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
                    if auxiliary.contains(&text) {
                        continue;
                    }
                    return Err(MarbleError::Protocol(format!(
                        "unexpected runtime response id {id:?}"
                    )));
                }
                Incoming::Event(event) => {
                    self.publish(&event.kind, event.data.clone());
                    sink(&event);
                }
                Incoming::Tool { request_id, call } => {
                    self.handle_tool(request_id, &call)?;
                }
                Incoming::Unknown(_) => {}
            }
        }
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

    fn handle_tool(&mut self, request_id: Value, call: &ToolRequest) -> Result<(), MarbleError> {
        let key = request_id
            .as_str()
            .map(str::to_owned)
            .unwrap_or_else(|| request_id.to_string());
        self.publish(
            "tool_start",
            value!({
                "requestId": key,
                "callId": call.call_id,
                "tool": call.name,
                "arguments": call.arguments,
            }),
        );
        let (approval, reason) = self.decide(&key, call);
        let outcome = crate::host::answer_tool_request_with_note(
            &mut self.client,
            request_id,
            call,
            approval,
            &self.router,
            (!reason.is_empty()).then_some(reason.as_str()),
        )?;
        self.publish(
            "tool_end",
            value!({
                "requestId": key,
                "callId": call.call_id,
                "tool": call.name,
                "approved": approval == Approval::Approve,
                "isError": outcome.is_error,
                "text": outcome.text,
            }),
        );
        Ok(())
    }

    /// Classify one governed call through marble's control layer, falling back
    /// to the host posture when no product document is attached (one-shot runs).
    fn decide(&mut self, key: &str, call: &ToolRequest) -> (Approval, String) {
        let path = call
            .arguments
            .get("path")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
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
                    self.publish(
                        "approval_pending",
                        value!({
                            "requestId": key,
                            "callId": call.call_id,
                            "tool": call.name,
                            "arguments": call.arguments,
                        }),
                    );
                    let decision = self
                        .approvals
                        .register(key, &call.call_id, &call.name, call.arguments.clone())
                        .recv_timeout(self.approval_timeout)
                        .unwrap_or(Approval::Deny);
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
        let shutdown_id = self
            .client
            .request("shutdown", json!({}))
            .map_err(MarbleError::Io)?;
        let _ = Self::wait_response_on(&mut self.client, &self.router, &self.events, &shutdown_id);
        Ok(())
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
        let receiver = inbox.register("host:1", "call-1", "glass.workspace.list", Value::Null);
        assert_eq!(inbox.pending().len(), 1);
        assert!(inbox.resolve("host:1", Approval::Approve));
        assert_eq!(receiver.recv().unwrap(), Approval::Approve);
        assert!(inbox.pending().is_empty());
        assert!(!inbox.resolve("host:1", Approval::Approve));
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
