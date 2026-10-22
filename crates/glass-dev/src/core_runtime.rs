//! Glass Core resident harness adapter.
//!
//! Glass Dev drives the `glass-core` engine over its negotiated mixed-tool
//! JSON-RPC runtime (`glass-core runtime`). Core owns provider and session
//! state plus its native coding tools; Glass routes only Glass integrations.
//!
//! Glass Core is the default resident harness. The Pi SDK runtime remains
//! available as an explicit legacy backend; see [`crate::resident_harness`].

use crate::agents::ResidentAgentBroker;
use crate::development::{DevelopmentError, DevelopmentResult, ToolCall};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::VecDeque;
use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::thread;
use std::time::{Duration, Instant};

/// Protocol version Glass Dev speaks to `glass-core runtime`.
pub const GLASS_CORE_PROTOCOL_VERSION: u64 = 2;
/// `glass-core` rejects frames larger than one mebibyte in either direction.
const MAX_FRAME_BYTES: usize = 1024 * 1024;
/// Bounded result text returned to the engine for one host tool call.
const TOOL_RESULT_BYTES: usize = 512 * 1024;
const EVENT_CAPACITY: usize = 256;
const STARTUP_TIMEOUT: Duration = Duration::from_secs(15);
const SHUTDOWN_GRACE: Duration = Duration::from_millis(500);
/// Environment override for the `glass-core` executable.
pub const GLASS_CORE_BINARY_ENV: &str = "GLASS_CORE_BINARY";
/// Executable basenames searched on `PATH`, in order.
pub const GLASS_CORE_BINARY_CANDIDATES: [&str; 2] = ["glass-core", "gc"];
/// Glass-authored collaboration brief handed to every resident session.
const SYSTEM_INSTRUCTIONS: &str = include_str!("../assets/glass-core-system.md");

/// Readiness state of one Glass Core component.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CoreReadinessState {
    /// Component is present and usable.
    Ready,
    /// Component was not found.
    Missing,
    /// Component exists but does not satisfy the negotiated surface.
    Incompatible,
    /// Component exists but has no usable provider configuration.
    Unauthenticated,
    /// Component exists but could not be inspected.
    Unknown,
}

/// One probed Glass Core component.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CoreReadinessComponent {
    /// Probe outcome.
    pub state: CoreReadinessState,
    /// Reported version, when the component exposes one.
    pub version: Option<String>,
    /// Resolved executable path, when found.
    pub path: Option<PathBuf>,
    /// Human-readable explanation of the outcome.
    pub detail: String,
}

/// Resident harness readiness for the Glass Core engine.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CoreReadiness {
    /// True when the engine can start a resident session.
    pub ready: bool,
    /// `glass-core` executable probe.
    pub binary: CoreReadinessComponent,
    /// Runtime protocol probe.
    pub protocol: CoreReadinessComponent,
    /// Provider configuration probe.
    pub authentication: CoreReadinessComponent,
    /// Selected provider, when reported.
    pub provider: Option<String>,
    /// Selected model, when reported.
    pub model: Option<String>,
    /// Newest stored session id for this workspace, when any.
    pub session: Option<String>,
    /// Directory holding Glass Core session records.
    pub storage_dir: PathBuf,
    /// Ordered repair steps for the current state.
    pub remediation: Vec<String>,
}

/// Options for one resident Glass Core session.
#[derive(Clone, Default)]
pub struct CoreRuntimeOptions {
    /// Execute mutations without pausing for an approval frame.
    pub unrestricted: bool,
    /// Directory holding Glass Core session records.
    pub session_dir: PathBuf,
    /// Display name stored with the session.
    pub name: Option<String>,
    /// Model id selected for the session.
    pub model: Option<String>,
    /// Thinking level requested by the caller. Glass Core selects reasoning
    /// budgets per model, so the value is reported rather than forced.
    pub thinking: Option<String>,
    /// Authoritative resident daemon broker used when no local executor exists.
    pub broker: Option<ResidentAgentBroker>,
    /// Executes Glass integration calls inside the resident workspace.
    pub local_tool_executor: Option<crate::resident_harness::AgentToolExecutor>,
    /// Extra instructions appended to the Glass collaboration brief.
    pub additional_system_prompt: Option<String>,
    /// Resume the newest stored session for this workspace.
    pub resume: bool,
    /// Session id to fork from, when the caller wants a branch.
    pub fork_from: Option<PathBuf>,
    /// Explicit `glass-core` executable. `GLASS_CORE_BINARY` and `PATH` are
    /// used when this is unset.
    pub binary: Option<PathBuf>,
    /// Extra argv passed before the `runtime` subcommand.
    pub binary_args: Vec<String>,
}

impl std::fmt::Debug for CoreRuntimeOptions {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("CoreRuntimeOptions")
            .field("unrestricted", &self.unrestricted)
            .field("session_dir", &self.session_dir)
            .field("name", &self.name)
            .field("model", &self.model)
            .field("thinking", &self.thinking)
            .field("broker", &self.broker)
            .field(
                "local_tool_executor",
                &self.local_tool_executor.as_ref().map(|_| "configured"),
            )
            .field("additional_system_prompt", &self.additional_system_prompt)
            .field("resume", &self.resume)
            .field("fork_from", &self.fork_from)
            .field("binary", &self.binary)
            .field("binary_args", &self.binary_args)
            .finish()
    }
}

struct PendingCoreToolApproval {
    frame_id: String,
    call: ToolCall,
}

/// One resident Glass Core engine process.
pub(crate) struct GlassCoreRuntime {
    child: Child,
    input: ChildStdin,
    output: Receiver<Result<Value, String>>,
    root: PathBuf,
    storage_dir: PathBuf,
    session_id: String,
    broker: Option<ResidentAgentBroker>,
    local_tool_executor: Option<crate::resident_harness::AgentToolExecutor>,
    unrestricted: bool,
    next_id: u64,
    host_events: VecDeque<Value>,
    pending_tool_approval: Option<PendingCoreToolApproval>,
    active_turn: Option<Value>,
}

impl std::fmt::Debug for GlassCoreRuntime {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("GlassCoreRuntime")
            .field("pid", &self.child.id())
            .field("root", &self.root)
            .field("session", &self.session_id)
            .finish_non_exhaustive()
    }
}

impl GlassCoreRuntime {
    /// Start one `glass-core runtime` process and open a mixed-tool session.
    pub(crate) fn spawn(root: &Path, options: CoreRuntimeOptions) -> DevelopmentResult<Self> {
        let root = fs::canonicalize(root)?;
        fs::create_dir_all(&options.session_dir)?;
        let storage_dir = fs::canonicalize(&options.session_dir)?;
        let binary = resolve_core_binary(options.binary.as_deref())?;

        let mut command = Command::new(&binary);
        command.args(&options.binary_args).arg("runtime");
        command
            .current_dir(&root)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit());
        let mut child = command.spawn().map_err(|error| {
            DevelopmentError::Process(format!(
                "failed to start the Glass Core runtime at {}: {error}",
                binary.display()
            ))
        })?;
        let input = child.stdin.take().ok_or_else(|| {
            DevelopmentError::Process("Glass Core runtime stdin is unavailable".into())
        })?;
        let output_stream = child.stdout.take().ok_or_else(|| {
            DevelopmentError::Process("Glass Core runtime stdout is unavailable".into())
        })?;
        let (sender, output) = mpsc::sync_channel(EVENT_CAPACITY);
        thread::Builder::new()
            .name("glass-core-runtime-output".into())
            .spawn(move || read_frames(output_stream, sender))?;

        let mut runtime = Self {
            child,
            input,
            output,
            root: root.clone(),
            storage_dir,
            session_id: String::new(),
            broker: options.broker.clone(),
            local_tool_executor: options.local_tool_executor.clone(),
            unrestricted: options.unrestricted,
            next_id: 1,
            host_events: VecDeque::new(),
            pending_tool_approval: None,
            active_turn: None,
        };
        runtime.initialize()?;
        runtime.open_session(&root, &options)?;
        Ok(runtime)
    }

    /// Session id assigned by the engine.
    ///
    /// Resident session state reaches the terminal through conversation
    /// events; this accessor exists for the adapter contract tests.
    #[cfg(test)]
    pub(crate) fn session_id(&self) -> &str {
        &self.session_id
    }

    fn initialize(&mut self) -> DevelopmentResult<()> {
        let id = self.send_request(
            "initialize",
            json!({
                "protocolVersion": GLASS_CORE_PROTOCOL_VERSION,
                "requiredCapabilities": ["mixedTools"],
                "storageDir": self.storage_dir,
            }),
        )?;
        let result = self.wait_response(&id, STARTUP_TIMEOUT)?;
        let protocol = result
            .get("protocolVersion")
            .and_then(Value::as_u64)
            .unwrap_or_default();
        if protocol != GLASS_CORE_PROTOCOL_VERSION {
            return Err(DevelopmentError::Process(format!(
                "Glass Core runtime protocol {protocol} is not supported"
            )));
        }
        if !result
            .pointer("/capabilities/mixedTools")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        {
            return Err(DevelopmentError::Process(
                "Glass Core runtime does not advertise mixed Core/Glass tool ownership (protocol v2 required)".into(),
            ));
        }
        Ok(())
    }

    fn open_session(&mut self, root: &Path, options: &CoreRuntimeOptions) -> DevelopmentResult<()> {
        if options.resume
            && let Some(session) = self.newest_workspace_session(root)?
        {
            let id = self.send_request(
                "session/resume",
                json!({"sessionId": session, "expectedToolMode": "mixed"}),
            )?;
            let result = self.wait_response(&id, STARTUP_TIMEOUT)?;
            self.session_id = result
                .get("sessionId")
                .and_then(Value::as_str)
                .unwrap_or(&session)
                .to_string();
            return Ok(());
        }
        let id = self.send_request("session/new", self.session_params(root, options))?;
        let result = self.wait_response(&id, STARTUP_TIMEOUT)?;
        self.session_id = result
            .get("sessionId")
            .and_then(Value::as_str)
            .ok_or_else(|| {
                DevelopmentError::Process("Glass Core session/new returned no sessionId".into())
            })?
            .to_string();
        if let Some(name) = &options.name {
            let id = self.send_request(
                "session/name",
                json!({"sessionId": self.session_id, "name": name}),
            )?;
            self.wait_response(&id, STARTUP_TIMEOUT)?;
        }
        if let Some(fork) = &options.fork_from {
            let parent = session_id_from_path(fork);
            if parent.is_empty() {
                self.host_events.push_back(json!({
                    "type": "notice",
                    "text": "Glass Core · no session reference to fork; started a fresh session.",
                }));
            } else {
                let id = self.send_request(
                    "session/fork",
                    json!({"sessionId": parent, "throughTurn": Value::Null}),
                );
                match id {
                    Ok(id) => {
                        if let Ok(result) = self.wait_response(&id, STARTUP_TIMEOUT) {
                            let forked = result
                                .get("sessionId")
                                .and_then(Value::as_str)
                                .map(str::to_string);
                            self.host_events.push_back(json!({
                                "type": "notice",
                                "text": format!(
                                    "Glass Core · branched session {} from {}",
                                    forked.as_deref().unwrap_or("(unnamed)"),
                                    parent
                                ),
                                "forkedSessionId": forked,
                            }));
                        }
                    }
                    Err(error) => self.host_events.push_back(json!({
                        "type": "notice",
                        "text": format!("Glass Core · could not branch the requested session: {error}"),
                    })),
                }
            }
        }
        Ok(())
    }

    fn session_params(&self, root: &Path, options: &CoreRuntimeOptions) -> Value {
        let tools = crate::tools::integration_tool_catalog()
            .into_iter()
            .map(|tool| {
                json!({
                    "name": tool.name,
                    "description": tool.description,
                    "inputSchema": tool.input_schema,
                })
            })
            .collect::<Vec<_>>();
        let mut params = json!({
            "cwd": root,
            "toolMode": "mixed",
            "tools": tools,
            "instructions": instructions(options),
        });
        if let Some(model) = &options.model {
            params["model"] = json!(model);
        }
        params
    }

    /// Newest stored session whose recorded `cwd` is this workspace.
    ///
    /// Session ids are creation-ordered, and `session/list` pages ascending
    /// with `nextAfterSessionId`, so the last match in the last page is the
    /// newest session. Paging is bounded so a large store cannot stall startup.
    fn newest_workspace_session(&mut self, root: &Path) -> DevelopmentResult<Option<String>> {
        const MAX_PAGES: usize = 8;
        const PAGE_LIMIT: u64 = 128;
        let mut after: Option<String> = None;
        let mut newest: Option<String> = None;
        for _ in 0..MAX_PAGES {
            let mut params = json!({"limit": PAGE_LIMIT});
            if let Some(after) = &after {
                params["afterSessionId"] = json!(after);
            }
            let id = self.send_request("session/list", params)?;
            let result = self.wait_response(&id, STARTUP_TIMEOUT)?;
            let sessions = result
                .get("sessions")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default();
            for session in &sessions {
                let cwd = session.get("cwd").and_then(Value::as_str);
                if cwd.is_some_and(|cwd| Path::new(cwd) == root)
                    && session.get("toolMode").and_then(Value::as_str) == Some("mixed")
                    && let Some(session) = session.get("sessionId").and_then(Value::as_str)
                {
                    newest = Some(session.to_string());
                }
            }
            match result
                .get("nextAfterSessionId")
                .and_then(Value::as_str)
                .map(str::to_string)
            {
                Some(next) => after = Some(next),
                None => break,
            }
        }
        Ok(newest)
    }

    /// Queue one operation for the engine and return the request id.
    pub(crate) fn start_request(
        &mut self,
        request: crate::pi_runtime::PiSessionRequest,
    ) -> DevelopmentResult<String> {
        let id = self.next_request_id();
        match request_parts(&self.session_id, &request) {
            Ok((method, params)) => {
                self.send_frame(&json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "method": method,
                    "params": params,
                }))?;
            }
            Err(reason) => {
                self.host_events.push_back(json!({
                    "type": "response",
                    "id": id,
                    "operation": request_operation(&request),
                    "ok": false,
                    "error": reason,
                }));
            }
        }
        Ok(id)
    }

    /// Poll for the next Glass event, servicing engine tool requests inline.
    pub(crate) fn recv_event_timeout(
        &mut self,
        timeout: Duration,
    ) -> DevelopmentResult<Option<Value>> {
        if let Some(event) = self.host_events.pop_front() {
            return Ok(Some(event));
        }
        let deadline = Instant::now() + timeout;
        loop {
            if let Some(event) = self.host_events.pop_front() {
                return Ok(Some(event));
            }
            let Some(value) =
                self.recv_frame(deadline.saturating_duration_since(Instant::now()))?
            else {
                return Ok(None);
            };
            self.handle_frame(value)?;
        }
    }

    /// Answer one parked host tool approval.
    pub(crate) fn resolve_tool_approval(
        &mut self,
        frame_id: &str,
        approved: bool,
    ) -> DevelopmentResult<()> {
        let pending = self.pending_tool_approval.as_ref().ok_or_else(|| {
            DevelopmentError::Conflict("no Glass Core tool call is awaiting approval".into())
        })?;
        if pending.frame_id != frame_id {
            return Err(DevelopmentError::Conflict(format!(
                "Glass Core approval frame {frame_id} is not the pending frame"
            )));
        }
        let pending = self.pending_tool_approval.take().ok_or_else(|| {
            DevelopmentError::Conflict("Glass Core approval request vanished".into())
        })?;
        let result = if approved {
            self.execute_tool_call(&pending.call, true, true)
        } else {
            Err(DevelopmentError::Conflict(
                "Glass denied this Glass Core tool call".into(),
            ))
        };
        self.respond_tool_result(&pending.frame_id, &result)?;
        self.host_events.push_back(json!({
            "type": "glass_tool_approval_resolved",
            "frameId": pending.frame_id,
            "toolName": pending.call.name,
            "approved": approved,
            "ok": result.is_ok(),
        }));
        if approved {
            self.host_events
                .push_back(tool_evidence(&pending.call, &result));
        }
        Ok(())
    }

    fn handle_frame(&mut self, value: Value) -> DevelopmentResult<()> {
        match value.get("method").and_then(Value::as_str) {
            Some("tool/execute") => self.handle_tool_request(&value),
            Some("runtime/event") => {
                let params = value.get("params").cloned().unwrap_or(Value::Null);
                for event in translate_event(&params) {
                    self.host_events.push_back(event);
                }
                Ok(())
            }
            Some(other) => {
                self.host_events.push_back(json!({
                    "type": "notice",
                    "text": format!("Glass Core · runtime sent unsupported method {other}"),
                }));
                Ok(())
            }
            None => {
                let Some(id) = value.get("id").cloned() else {
                    return Ok(());
                };
                let mut event = json!({
                    "type": "response",
                    "id": id,
                    "ok": value.get("error").is_none(),
                });
                if let Some(result) = value.get("result") {
                    event["result"] = result.clone();
                }
                if let Some(error) = value.get("error") {
                    event["error"] = error.clone();
                }
                self.host_events.push_back(event);
                Ok(())
            }
        }
    }

    fn handle_tool_request(&mut self, value: &Value) -> DevelopmentResult<()> {
        let frame_id = value
            .get("id")
            .cloned()
            .ok_or_else(|| DevelopmentError::Serialization("tool frame has no id".into()))?;
        let params = value.get("params").cloned().unwrap_or(Value::Null);
        let call_id = params
            .get("callId")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        let name = params
            .get("name")
            .and_then(Value::as_str)
            .ok_or_else(|| DevelopmentError::Serialization("tool frame has no name".into()))?;
        let call = ToolCall {
            id: call_id,
            name: name.to_string(),
            arguments: params.get("arguments").cloned().unwrap_or(Value::Null),
        };
        let frame_key = frame_id_key(&frame_id);
        if !crate::tools::is_integration_tool(&call.name) {
            let result = Err(DevelopmentError::NotFound(format!(
                "Glass integration {}",
                call.name
            )));
            self.respond_tool_result(&frame_key, &result)?;
            self.host_events.push_back(tool_evidence(&call, &result));
            return Ok(());
        }
        if !self.unrestricted && crate::tools::tool_requires_mutation(&call.name) {
            if self.pending_tool_approval.is_some() {
                return Err(DevelopmentError::Conflict(
                    "Glass Core requested a second tool while approval was pending".into(),
                ));
            }
            self.pending_tool_approval = Some(PendingCoreToolApproval {
                frame_id: frame_key.clone(),
                call: call.clone(),
            });
            self.host_events.push_back(json!({
                "type": "glass_tool_approval_request",
                "frameId": frame_key,
                "toolName": call.name,
                "arguments": redact_tool_arguments(&call.arguments),
            }));
            return Ok(());
        }
        let result = self.execute_tool_call(&call, self.unrestricted, self.unrestricted);
        self.respond_tool_result(&frame_key, &result)?;
        self.host_events.push_back(tool_evidence(&call, &result));
        Ok(())
    }

    fn execute_tool_call(
        &self,
        call: &ToolCall,
        allow_mutation: bool,
        confirmed: bool,
    ) -> DevelopmentResult<Value> {
        if let Some(executor) = &self.local_tool_executor {
            return executor(call, allow_mutation, confirmed);
        }
        match &self.broker {
            Some(broker) => {
                let runtime = tokio::runtime::Builder::new_current_thread()
                    .enable_io()
                    .build()
                    .map_err(DevelopmentError::Io)?;
                runtime
                    .block_on(crate::daemon::forward_resident_tool_call_with_context(
                        broker,
                        call,
                        &self.root,
                        allow_mutation,
                        confirmed,
                    ))
                    .map_err(|error| DevelopmentError::Process(error.to_string()))
            }
            None => Err(DevelopmentError::Process(
                "resident Glass Core has no authoritative Glass daemon broker".into(),
            )),
        }
    }

    fn respond_tool_result(
        &mut self,
        frame_id: &str,
        result: &DevelopmentResult<Value>,
    ) -> DevelopmentResult<()> {
        let (text, is_error) = match result {
            Ok(value) => (bounded_result_text(value), false),
            Err(error) => (format!("error: {error}"), true),
        };
        let id = frame_id_value(frame_id)?;
        self.send_frame(&json!({
            "jsonrpc": "2.0",
            "id": id,
            "result": {"text": text, "isError": is_error},
        }))
    }

    fn send_request(&mut self, method: &str, params: Value) -> DevelopmentResult<String> {
        let id = self.next_request_id();
        self.send_frame(&json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params,
        }))?;
        Ok(id)
    }

    fn next_request_id(&mut self) -> String {
        let id = format!("glass-{}", self.next_id);
        self.next_id = self.next_id.saturating_add(1);
        id
    }

    fn send_frame(&mut self, value: &Value) -> DevelopmentResult<()> {
        let mut encoded = serde_json::to_vec(value)
            .map_err(|error| DevelopmentError::Serialization(error.to_string()))?;
        if encoded.len() + 1 > MAX_FRAME_BYTES {
            return Err(DevelopmentError::InvalidInput(format!(
                "Glass Core request exceeds the {} byte runtime frame limit",
                MAX_FRAME_BYTES
            )));
        }
        encoded.push(b'\n');
        self.input
            .write_all(&encoded)
            .and_then(|()| self.input.flush())
            .map_err(|error| {
                DevelopmentError::Process(format!("Glass Core runtime write failed: {error}"))
            })
    }

    fn recv_frame(&mut self, timeout: Duration) -> DevelopmentResult<Option<Value>> {
        match self.output.recv_timeout(timeout) {
            Ok(Ok(value)) => Ok(Some(value)),
            Ok(Err(error)) => Err(DevelopmentError::Process(error)),
            Err(RecvTimeoutError::Timeout) => Ok(None),
            Err(RecvTimeoutError::Disconnected) => Err(DevelopmentError::Process(
                "Glass Core runtime event stream closed".into(),
            )),
        }
    }

    fn wait_response(&mut self, id: &str, timeout: Duration) -> DevelopmentResult<Value> {
        let deadline = Instant::now() + timeout;
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            let Some(value) = self.recv_frame(remaining)? else {
                return Err(DevelopmentError::Process(format!(
                    "Glass Core runtime did not answer {id} within {timeout:?}"
                )));
            };
            if value.get("method").is_some() {
                self.handle_frame(value)?;
                continue;
            }
            if value.get("id").and_then(Value::as_str) != Some(id) {
                self.handle_frame(value)?;
                continue;
            }
            if let Some(error) = value.get("error") {
                return Err(DevelopmentError::Process(format!(
                    "Glass Core runtime rejected {id}: {error}"
                )));
            }
            return Ok(value.get("result").cloned().unwrap_or(Value::Null));
        }
    }

    fn cancel_active_turn(&mut self) {
        if self.session_id.is_empty() {
            return;
        }
        let mut params = json!({"sessionId": self.session_id});
        if let Some(turn) = self.active_turn.take() {
            params["turnId"] = turn;
        }
        let _ = self.send_request("turn/cancel", params);
    }
}

impl Drop for GlassCoreRuntime {
    fn drop(&mut self) {
        self.cancel_active_turn();
        let _ = self.send_request("shutdown", json!({}));
        let deadline = Instant::now() + SHUTDOWN_GRACE;
        while Instant::now() < deadline {
            if let Ok(Some(_)) = self.child.try_wait() {
                return;
            }
            thread::sleep(Duration::from_millis(20));
        }
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// Map one Glass session request onto the Glass Core runtime surface.
fn request_parts(
    session: &str,
    request: &crate::pi_runtime::PiSessionRequest,
) -> Result<(&'static str, Value), String> {
    use crate::pi_runtime::PiSessionRequest as Request;
    let turn = |text: &String, context: &Option<Value>| {
        let mut params = json!({"sessionId": session, "text": text});
        if let Some(context) = context {
            params["context"] = context.clone();
        }
        params
    };
    Ok(match request {
        Request::Hello => ("session/describe", json!({"sessionId": session})),
        Request::Prompt { text, context } => ("turn/prompt", turn(text, context)),
        Request::Steer { text, context } => ("turn/steer", turn(text, context)),
        Request::FollowUp { text, context } => ("turn/followUp", turn(text, context)),
        Request::Complete { prefix, suffix } => (
            "completion/request",
            json!({
                "sessionId": session,
                "prefix": prefix,
                "suffix": suffix,
                "maxTokens": 256,
            }),
        ),
        Request::Abort => ("turn/cancel", json!({"sessionId": session})),
        Request::State => ("session/describe", json!({"sessionId": session})),
        Request::Models => ("models/list", json!({"sessionId": session, "cursor": 0})),
        Request::SetModel { provider, model_id } => {
            let mut params = json!({"sessionId": session, "model": model_id});
            if !provider.is_empty() {
                params["provider"] = json!(provider);
            }
            ("session/configure", params)
        }
        Request::SetThinking { .. } => {
            return Err(
                "Glass Core selects the reasoning budget from the session model; use /model or session/configure"
                    .into(),
            );
        }
        Request::NewSession => ("session/new", json!({})),
        Request::Compact { instructions } => {
            let mut params = json!({"sessionId": session});
            if let Some(instructions) = instructions {
                params["instructions"] = json!(instructions);
            }
            ("session/compact", params)
        }
        Request::CloneSession => ("session/fork", json!({"sessionId": session})),
        Request::Rewind { entry_id } => (
            "session/rewind",
            json!({"sessionId": session, "throughTurn": turn_number(entry_id)?}),
        ),
        Request::Fork { entry_id } => (
            "session/fork",
            json!({"sessionId": session, "throughTurn": turn_number(entry_id)?}),
        ),
        Request::SwitchSession { path } => {
            let id = session_id_from_path(Path::new(path));
            if id.is_empty() {
                return Err("Glass Core needs a session id to switch sessions".into());
            }
            (
                "session/resume",
                json!({"sessionId": id, "expectedToolMode": "host"}),
            )
        }
        Request::ListSessions => ("session/list", json!({})),
        Request::Entries { since } => (
            "session/entries",
            json!({"sessionId": session, "cursor": cursor_number(since.as_deref())}),
        ),
        Request::Tree => ("session/tree", json!({})),
        Request::Messages => ("session/inspect", json!({"sessionId": session})),
        Request::SessionStats => ("session/stats", json!({"sessionId": session})),
        Request::SetSessionName { name } => {
            ("session/name", json!({"sessionId": session, "name": name}))
        }
        Request::SlashCommands => {
            return Err(
                "Glass Core runtime sessions expose no slash-command surface; use Glass actions"
                    .into(),
            );
        }
        Request::SlashCommand { name, .. } => {
            return Err(format!(
                "Glass Core runtime sessions do not execute slash command /{name}"
            ));
        }
    })
}

fn request_operation(request: &crate::pi_runtime::PiSessionRequest) -> &'static str {
    use crate::pi_runtime::PiSessionRequest as Request;
    match request {
        Request::Hello => "hello",
        Request::Prompt { .. } => "prompt",
        Request::Steer { .. } => "steer",
        Request::FollowUp { .. } => "followUp",
        Request::Complete { .. } => "complete",
        Request::Abort => "abort",
        Request::State => "state",
        Request::Models => "models",
        Request::SetModel { .. } => "setModel",
        Request::SetThinking { .. } => "setThinking",
        Request::NewSession => "newSession",
        Request::Compact { .. } => "compact",
        Request::CloneSession => "cloneSession",
        Request::Rewind { .. } => "rewind",
        Request::Fork { .. } => "fork",
        Request::SwitchSession { .. } => "switchSession",
        Request::ListSessions => "listSessions",
        Request::Entries { .. } => "entries",
        Request::Tree => "tree",
        Request::Messages => "messages",
        Request::SessionStats => "stats",
        Request::SetSessionName { .. } => "setName",
        Request::SlashCommands => "slashCommands",
        Request::SlashCommand { .. } => "slashCommand",
    }
}

fn turn_number(entry_id: &str) -> Result<u64, String> {
    entry_id
        .trim()
        .parse::<u64>()
        .map_err(|_| format!("Glass Core identifies turns by number, not entry {entry_id}"))
}

fn cursor_number(cursor: Option<&str>) -> u64 {
    cursor
        .and_then(|cursor| cursor.trim().parse::<u64>().ok())
        .unwrap_or(0)
}

fn session_id_from_path(path: &Path) -> String {
    let raw = path.to_string_lossy();
    if raw.is_empty() {
        return String::new();
    }
    let looks_like_file = raw.contains(std::path::MAIN_SEPARATOR)
        || raw.ends_with(".json")
        || raw.ends_with(".jsonl");
    if !looks_like_file {
        return raw.to_string();
    }
    path.file_stem()
        .map(|stem| stem.to_string_lossy().trim_end_matches(".host").to_string())
        .unwrap_or_default()
}

/// Translate one `runtime/event` notification into Glass conversation events.
fn translate_event(params: &Value) -> Vec<Value> {
    let kind = params
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let data = params.get("data").cloned().unwrap_or(Value::Null);
    let mut events = Vec::new();
    match kind {
        "turn_start" => {
            events.push(json!({
                "type": "turn_start",
                "turnId": params.get("turnId").cloned().unwrap_or(Value::Null),
            }));
        }
        "text" => {
            let text = data.get("text").and_then(Value::as_str).unwrap_or_default();
            if !text.is_empty() {
                events.push(json!({
                    "type": "message_update",
                    "assistantMessageEvent": {"type": "text_delta", "delta": text},
                }));
            }
        }
        "reasoning" => {
            events.push(json!({
                "type": "reasoning",
                "text": data.get("text").cloned().unwrap_or(Value::Null),
            }));
        }
        "tool_start" => {
            events.push(json!({
                "type": "tool_execution_start",
                "toolName": data.get("name").cloned().unwrap_or(Value::Null),
                "callId": data.get("callId").cloned().unwrap_or(Value::Null),
            }));
        }
        "tool_end" => {
            events.push(json!({
                "type": "tool_execution_end",
                "toolName": data.get("name").cloned().unwrap_or(Value::Null),
                "callId": data.get("callId").cloned().unwrap_or(Value::Null),
                "isError": data.get("isError").cloned().unwrap_or(Value::Bool(false)),
            }));
        }
        "tool_cancel" => {
            events.push(json!({
                "type": "tool_cancel",
                "callId": data.get("callId").cloned().unwrap_or(Value::Null),
                "acknowledgementRequired": data
                    .get("acknowledgementRequired")
                    .cloned()
                    .unwrap_or(Value::Bool(false)),
            }));
        }
        "tool_progress" => {
            events.push(json!({"type": "tool_progress", "text": data}));
        }
        "usage" => {
            events.push(json!({"type": "usage", "usage": data}));
        }
        "waiting" => events.push(json!({"type": "waiting", "detail": data})),
        "compacted" => events.push(json!({"type": "compacted", "detail": data})),
        "steered" => events.push(json!({"type": "steered", "detail": data})),
        "queued" => events.push(json!({"type": "queued", "detail": data})),
        "notice" => {
            events.push(json!({
                "type": "notice",
                "text": data
                    .get("text")
                    .cloned()
                    .unwrap_or_else(|| data.clone()),
            }));
        }
        "error" => {
            events.push(json!({
                "type": "error",
                "message": data
                    .get("message")
                    .cloned()
                    .unwrap_or_else(|| data.clone()),
            }));
        }
        "settled" => {
            events.push(json!({"type": "agent_end", "detail": data.clone()}));
            events.push(json!({"type": "agent_settled", "detail": data}));
        }
        other => events.push(json!({
            "type": other,
            "data": data,
        })),
    }
    events
}

fn tool_evidence(call: &ToolCall, result: &DevelopmentResult<Value>) -> Value {
    let browser = call.name.starts_with("glass.browser.");
    json!({
        "type": if browser { "glass_browser_evidence" } else { "glass_tool_evidence" },
        "toolName": call.name,
        "ok": result.is_ok(),
        "error": result.as_ref().err().map(|error| redact_text(&error.to_string())),
    })
}

fn redact_tool_arguments(value: &Value) -> Value {
    match value {
        Value::Object(map) => Value::Object(
            map.iter()
                .map(|(key, value)| {
                    let value = if sensitive_name(key) {
                        Value::String("[redacted]".into())
                    } else {
                        redact_tool_arguments(value)
                    };
                    (key.clone(), value)
                })
                .collect(),
        ),
        Value::Array(items) => Value::Array(items.iter().map(redact_tool_arguments).collect()),
        other => other.clone(),
    }
}

/// True when a field name, flag, or environment variable carries a credential.
fn sensitive_name(name: &str) -> bool {
    let normalized = name
        .trim_start_matches('-')
        .to_ascii_lowercase()
        .replace('-', "_");
    [
        "token",
        "secret",
        "password",
        "passwd",
        "api_key",
        "apikey",
        "authorization",
        "cookie",
        "credential",
        "private_key",
    ]
    .iter()
    .any(|needle| normalized.contains(needle))
}

/// Bound and redact free text before it reaches a conversation event.
///
/// `NAME=value` assignments and `--flag value` pairs are rewritten to
/// `<redacted>` when the name looks like a credential, so a failed command does
/// not copy a secret into the transcript.
fn redact_text(text: &str) -> String {
    let bounded: String = text.chars().take(2_048).collect();
    let mut output = String::with_capacity(bounded.len());
    let mut index = 0;
    while index < bounded.len() {
        let character = bounded[index..]
            .chars()
            .next()
            .expect("index stays on a character boundary");
        if character.is_ascii_alphanumeric() || character == '_' || character == '-' {
            let start = index;
            while index < bounded.len() {
                let candidate = bounded[index..]
                    .chars()
                    .next()
                    .expect("index stays on a character boundary");
                if candidate.is_ascii_alphanumeric() || candidate == '_' || candidate == '-' {
                    index += candidate.len_utf8();
                } else {
                    break;
                }
            }
            let name = &bounded[start..index];
            if sensitive_name(name) {
                let bytes = bounded.as_bytes();
                if bytes.get(index) == Some(&b'=') {
                    output.push_str(name);
                    output.push_str("=<redacted>");
                    index += 1;
                    skip_value(bytes, &mut index);
                    continue;
                }
                if name.starts_with("--") && bytes.get(index) == Some(&b' ') {
                    output.push_str(name);
                    output.push_str(" <redacted>");
                    index += 1;
                    skip_value(bytes, &mut index);
                    continue;
                }
            }
            output.push_str(name);
            continue;
        }
        output.push(character);
        index += character.len_utf8();
    }
    output
}

fn skip_value(bytes: &[u8], index: &mut usize) {
    while *index < bytes.len() && !bytes[*index].is_ascii_whitespace() {
        *index += 1;
    }
}

fn bounded_result_text(value: &Value) -> String {
    let text = match value {
        Value::String(text) => text.clone(),
        other => serde_json::to_string(other).unwrap_or_else(|_| "{}".into()),
    };
    if text.len() <= TOOL_RESULT_BYTES {
        return text;
    }
    let mut end = TOOL_RESULT_BYTES;
    while end > 0 && !text.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}…[truncated by Glass]", &text[..end])
}

fn frame_id_key(frame_id: &Value) -> String {
    match frame_id {
        Value::String(text) => text.clone(),
        other => other.to_string(),
    }
}

fn frame_id_value(frame_id: &str) -> DevelopmentResult<Value> {
    Ok(serde_json::from_str::<Value>(frame_id).unwrap_or_else(|_| Value::String(frame_id.into())))
}

fn instructions(options: &CoreRuntimeOptions) -> String {
    let mut text = SYSTEM_INSTRUCTIONS.trim().to_string();
    if let Some(extra) = options
        .additional_system_prompt
        .as_deref()
        .map(str::trim)
        .filter(|extra| !extra.is_empty())
    {
        text.push_str("\n\n## Session brief\n\n");
        text.push_str(extra);
    }
    text
}

fn read_frames(mut stream: impl std::io::Read, sender: mpsc::SyncSender<Result<Value, String>>) {
    let mut reader = BufReader::new(&mut stream);
    let mut line = String::new();
    loop {
        line.clear();
        match reader.read_line(&mut line) {
            Ok(0) => {
                let _ = sender.send(Err("Glass Core runtime closed its output stream".into()));
                return;
            }
            Ok(_) => {}
            Err(error) => {
                let _ = sender.send(Err(format!("Glass Core runtime read failed: {error}")));
                return;
            }
        }
        let trimmed = line.trim_end_matches(['\n', '\r']);
        if trimmed.is_empty() {
            continue;
        }
        if trimmed.len() > MAX_FRAME_BYTES {
            let _ = sender.send(Err("Glass Core runtime emitted an oversized frame".into()));
            return;
        }
        let value = serde_json::from_str::<Value>(trimmed).map_err(|error| error.to_string());
        if sender.send(value).is_err() {
            return;
        }
    }
}

/// Resolve the `glass-core` executable for a resident session.
pub fn resolve_core_binary(explicit: Option<&Path>) -> DevelopmentResult<PathBuf> {
    if let Some(path) = explicit {
        if path.is_file() {
            return Ok(path.to_path_buf());
        }
        return Err(DevelopmentError::NotFound(format!(
            "Glass Core executable {} does not exist",
            path.display()
        )));
    }
    if let Some(configured) = std::env::var_os(GLASS_CORE_BINARY_ENV) {
        let path = PathBuf::from(configured);
        if path.is_file() {
            return Ok(path);
        }
        return Err(DevelopmentError::NotFound(format!(
            "{GLASS_CORE_BINARY_ENV} points at {}, which does not exist",
            path.display()
        )));
    }
    for candidate in GLASS_CORE_BINARY_CANDIDATES {
        if let Some(path) = search_path(candidate) {
            return Ok(path);
        }
    }
    Err(DevelopmentError::NotFound(format!(
        "no {} executable on PATH; install glass-core or set {GLASS_CORE_BINARY_ENV}",
        GLASS_CORE_BINARY_CANDIDATES.join(" or ")
    )))
}

fn search_path(binary: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|directory| directory.join(binary))
        .find(|candidate| candidate.is_file())
}

fn core_version(binary: &Path, args: &[String]) -> Option<String> {
    let output = Command::new(binary)
        .args(args)
        .arg("--version")
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&output.stdout);
    let version = text.split_whitespace().last()?.trim().to_string();
    (!version.is_empty()).then_some(version)
}

fn managed_storage_dir() -> DevelopmentResult<PathBuf> {
    let base = dirs::data_local_dir()
        .or_else(dirs::home_dir)
        .ok_or_else(|| DevelopmentError::Config("no local data directory is available".into()))?;
    Ok(base.join("glass").join("core-sessions"))
}

/// Probe the resident Glass Core engine without starting a session.
pub fn core_readiness() -> DevelopmentResult<CoreReadiness> {
    let storage_dir = managed_storage_dir()?;
    let binary = resolve_core_binary(None);
    let (binary_component, version) = match &binary {
        Ok(path) => {
            let version = core_version(path, &[]);
            (
                CoreReadinessComponent {
                    state: CoreReadinessState::Ready,
                    version: version.clone(),
                    path: Some(path.clone()),
                    detail: version
                        .as_deref()
                        .map(|version| format!("Glass Core engine available ({version})"))
                        .unwrap_or_else(|| "Glass Core engine available".into()),
                },
                version,
            )
        }
        Err(error) => (
            CoreReadinessComponent {
                state: CoreReadinessState::Missing,
                version: None,
                path: None,
                detail: error.to_string(),
            },
            None,
        ),
    };
    let (protocol, authentication, provider, model) = if binary_component.state
        == CoreReadinessState::Ready
    {
        match probe_runtime(binary.as_ref().ok().map(PathBuf::as_path), &storage_dir) {
            Ok(probe) => (
                CoreReadinessComponent {
                    state: CoreReadinessState::Ready,
                    version: version.clone(),
                    path: binary.as_ref().ok().cloned(),
                    detail: format!(
                        "runtime protocol {GLASS_CORE_PROTOCOL_VERSION} with negotiated mixed tools"
                    ),
                },
                probe.authentication,
                probe.provider,
                probe.model,
            ),
            Err(error) => (
                CoreReadinessComponent {
                    state: if error.to_string().contains("protocol")
                        || error
                            .to_string()
                            .contains("mixed Core/Glass tool ownership")
                    {
                        CoreReadinessState::Incompatible
                    } else {
                        CoreReadinessState::Unknown
                    },
                    version: version.clone(),
                    path: binary.as_ref().ok().cloned(),
                    detail: error.to_string(),
                },
                CoreReadinessComponent {
                    state: CoreReadinessState::Unknown,
                    version: None,
                    path: None,
                    detail: "runtime probe did not complete".into(),
                },
                None,
                None,
            ),
        }
    } else {
        (
            CoreReadinessComponent {
                state: CoreReadinessState::Missing,
                version: None,
                path: None,
                detail: "the runtime cannot start without the engine".into(),
            },
            CoreReadinessComponent {
                state: CoreReadinessState::Unknown,
                version: None,
                path: None,
                detail: "not inspected".into(),
            },
            None,
            None,
        )
    };
    let ready = binary_component.state == CoreReadinessState::Ready
        && protocol.state == CoreReadinessState::Ready
        && authentication.state == CoreReadinessState::Ready;
    let mut remediation = Vec::new();
    if binary_component.state == CoreReadinessState::Missing {
        remediation.push(
            "Install glass-core (`cargo install glass-core --locked`) or set GLASS_CORE_BINARY"
                .to_string(),
        );
    }
    if protocol.state == CoreReadinessState::Incompatible {
        remediation
            .push("Update glass-core to a build with runtime protocol v2 mixed tools".to_string());
    }
    if authentication.state != CoreReadinessState::Ready {
        remediation.push(
            "Connect a provider for glass-core (`glass-core` then /login, or set a provider API key)"
                .to_string(),
        );
    }
    Ok(CoreReadiness {
        ready,
        binary: binary_component,
        protocol,
        authentication,
        provider,
        model,
        session: None,
        storage_dir,
        remediation,
    })
}

struct RuntimeProbe {
    authentication: CoreReadinessComponent,
    provider: Option<String>,
    model: Option<String>,
}

fn probe_runtime(binary: Option<&Path>, storage_dir: &Path) -> DevelopmentResult<RuntimeProbe> {
    let binary = binary.ok_or_else(|| {
        DevelopmentError::NotFound("no Glass Core executable was resolved".into())
    })?;
    let mut command = Command::new(binary);
    command
        .arg("runtime")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    let mut child = command
        .spawn()
        .map_err(|error| DevelopmentError::Process(error.to_string()))?;
    let mut input = child
        .stdin
        .take()
        .ok_or_else(|| DevelopmentError::Process("runtime stdin unavailable".into()))?;
    let stream = child
        .stdout
        .take()
        .ok_or_else(|| DevelopmentError::Process("runtime stdout unavailable".into()))?;
    let (sender, output) = mpsc::sync_channel(EVENT_CAPACITY);
    let reader = thread::Builder::new()
        .name("glass-core-readiness".into())
        .spawn(move || read_frames(stream, sender))?;
    let deadline = Instant::now() + STARTUP_TIMEOUT;
    let mut request = |method: &str, params: Value| -> DevelopmentResult<Value> {
        let frame = json!({"jsonrpc": "2.0", "id": method, "method": method, "params": params});
        let mut encoded = serde_json::to_vec(&frame)
            .map_err(|error| DevelopmentError::Serialization(error.to_string()))?;
        encoded.push(b'\n');
        input
            .write_all(&encoded)
            .and_then(|()| input.flush())
            .map_err(|error| DevelopmentError::Process(error.to_string()))?;
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            match output.recv_timeout(remaining) {
                Ok(Ok(value)) if value.get("id").and_then(Value::as_str) == Some(method) => {
                    if let Some(error) = value.get("error") {
                        return Err(DevelopmentError::Process(error.to_string()));
                    }
                    return Ok(value.get("result").cloned().unwrap_or(Value::Null));
                }
                Ok(Ok(_)) => continue,
                Ok(Err(error)) => return Err(DevelopmentError::Process(error)),
                Err(RecvTimeoutError::Timeout) => {
                    return Err(DevelopmentError::Process(format!(
                        "Glass Core runtime did not answer {method}"
                    )));
                }
                Err(RecvTimeoutError::Disconnected) => {
                    return Err(DevelopmentError::Process(
                        "Glass Core runtime closed during readiness probing".into(),
                    ));
                }
            }
        }
    };
    let result = (|| {
        fs::create_dir_all(storage_dir)?;
        request(
            "initialize",
            json!({
                "protocolVersion": GLASS_CORE_PROTOCOL_VERSION,
                "requiredCapabilities": ["mixedTools"],
                "storageDir": storage_dir,
            }),
        )?;
        request("auth/status", json!({}))
    })();
    let _ = input.write_all(
        b"{\"jsonrpc\":\"2.0\",\"id\":\"shutdown\",\"method\":\"shutdown\",\"params\":{}}\n",
    );
    let _ = input.flush();
    let _ = child.kill();
    let _ = child.wait();
    let _ = reader.join();
    let status = result?;
    let connected = status
        .get("connected")
        .and_then(Value::as_bool)
        .or_else(|| status.get("ready").and_then(Value::as_bool))
        .unwrap_or(false);
    let provider = status
        .get("provider")
        .and_then(Value::as_str)
        .map(str::to_string);
    let model = status
        .get("model")
        .and_then(Value::as_str)
        .map(str::to_string);
    let authentication = CoreReadinessComponent {
        state: if connected {
            CoreReadinessState::Ready
        } else {
            CoreReadinessState::Unauthenticated
        },
        version: None,
        path: None,
        detail: status
            .get("detail")
            .and_then(Value::as_str)
            .map(str::to_string)
            .unwrap_or_else(|| {
                if connected {
                    "provider configuration accepted".into()
                } else {
                    "no usable provider configuration".into()
                }
            }),
    };
    Ok(RuntimeProbe {
        authentication,
        provider,
        model,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pi_runtime::PiSessionRequest;
    use std::sync::Arc;

    fn python3() -> Option<PathBuf> {
        search_path("python3")
    }

    fn fixture() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests")
            .join("fixtures")
            .join("fake_glass_core_runtime.py")
    }

    fn harness(
        root: &Path,
        unrestricted: bool,
        executor: Option<crate::resident_harness::AgentToolExecutor>,
    ) -> GlassCoreRuntime {
        harness_mode(root, unrestricted, executor, "mixed")
    }

    fn harness_mode(
        root: &Path,
        unrestricted: bool,
        executor: Option<crate::resident_harness::AgentToolExecutor>,
        mode: &str,
    ) -> GlassCoreRuntime {
        spawn_mode(root, unrestricted, executor, mode).unwrap()
    }

    fn spawn_mode(
        root: &Path,
        unrestricted: bool,
        executor: Option<crate::resident_harness::AgentToolExecutor>,
        mode: &str,
    ) -> DevelopmentResult<GlassCoreRuntime> {
        let python = python3().expect("python3 is required for this fixture test");
        GlassCoreRuntime::spawn(
            root,
            CoreRuntimeOptions {
                unrestricted,
                session_dir: root.join("sessions"),
                name: Some("fixture".into()),
                local_tool_executor: executor,
                binary: Some(python),
                binary_args: vec![fixture().display().to_string(), mode.into()],
                ..CoreRuntimeOptions::default()
            },
        )
    }

    fn drain(runtime: &mut GlassCoreRuntime, timeout: Duration) -> Vec<Value> {
        let deadline = Instant::now() + timeout;
        let mut events = Vec::new();
        while Instant::now() < deadline {
            match runtime
                .recv_event_timeout(Duration::from_millis(25))
                .unwrap()
            {
                Some(event) => events.push(event),
                None => break,
            }
        }
        events
    }

    #[test]
    fn turn_requests_map_onto_the_runtime_surface() {
        let (method, params) = request_parts(
            "s-1",
            &PiSessionRequest::Prompt {
                text: "hello".into(),
                context: None,
            },
        )
        .unwrap();
        assert_eq!(method, "turn/prompt");
        assert_eq!(params["sessionId"], "s-1");
        assert_eq!(params["text"], "hello");
        assert!(params.get("context").is_none());

        let (method, params) = request_parts(
            "s-1",
            &PiSessionRequest::SetModel {
                provider: "anthropic".into(),
                model_id: "claude".into(),
            },
        )
        .unwrap();
        assert_eq!(method, "session/configure");
        assert_eq!(params["model"], "claude");
        assert_eq!(params["provider"], "anthropic");
    }

    #[test]
    fn unsupported_operations_explain_the_gap() {
        let error = request_parts(
            "s-1",
            &PiSessionRequest::SetThinking {
                level: "high".into(),
            },
        )
        .unwrap_err();
        assert!(error.contains("reasoning budget"));
        let error = request_parts(
            "s-1",
            &PiSessionRequest::Rewind {
                entry_id: "entry-9".into(),
            },
        )
        .unwrap_err();
        assert!(error.contains("turns by number"));
    }

    #[test]
    fn settled_events_close_streaming_then_idle_the_agent() {
        let events = translate_event(&json!({"type": "settled", "data": {"reason": "completed"}}));
        let kinds = events
            .iter()
            .filter_map(|event| event.get("type").and_then(Value::as_str))
            .collect::<Vec<_>>();
        assert_eq!(kinds, vec!["agent_end", "agent_settled"]);
    }

    #[test]
    fn text_events_stream_as_assistant_deltas() {
        let events = translate_event(&json!({"type": "text", "data": {"text": "hi"}}));
        assert_eq!(
            events[0].pointer("/assistantMessageEvent/delta"),
            Some(&json!("hi"))
        );
        assert_eq!(
            events[0].pointer("/assistantMessageEvent/type"),
            Some(&json!("text_delta"))
        );
    }

    #[test]
    fn evidence_redacts_secret_like_tokens() {
        let evidence = tool_evidence(
            &ToolCall {
                id: "call-1".into(),
                name: "glass.command.run".into(),
                arguments: json!({}),
            },
            &Err(DevelopmentError::Process(
                "GITHUB_TOKEN=abcdef failed".into(),
            )),
        );
        let error = evidence["error"].as_str().unwrap();
        assert!(error.contains("<redacted>"), "{error}");
        assert!(!error.contains("abcdef"), "{error}");
    }

    #[test]
    fn integration_tool_catalog_is_namespaced_object_schemas() {
        let catalog = crate::tools::integration_tool_catalog();
        assert!(!catalog.is_empty());
        for tool in &catalog {
            assert!(tool.name.starts_with("glass."), "{}", tool.name);
            assert_eq!(tool.input_schema["type"], "object", "{}", tool.name);
        }
        assert!(crate::tools::is_integration_tool("glass.workspace.list"));
        assert!(!crate::tools::is_integration_tool("glass.file.read"));
        assert!(!crate::tools::is_integration_tool("glass.not_registered"));
    }

    #[test]
    fn rejects_v1_runtime_without_silent_host_mode_fallback() {
        if python3().is_none() {
            return;
        }
        let root = std::env::temp_dir().join(format!(
            "glass-core-v1-rejected-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        fs::create_dir_all(&root).unwrap();
        let error = spawn_mode(&root, true, None, "legacy-v1").unwrap_err();
        assert!(
            error.to_string().contains("protocolVersion must be 1"),
            "{error}"
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn rejects_v2_runtime_without_mixed_tools_capability() {
        if python3().is_none() {
            return;
        }
        let root = std::env::temp_dir().join(format!(
            "glass-core-no-mixed-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        fs::create_dir_all(&root).unwrap();
        let error = spawn_mode(&root, true, None, "missing-mixed").unwrap_err();
        assert!(
            error
                .to_string()
                .contains("does not advertise mixed Core/Glass"),
            "{error}"
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn protocol_capability_not_package_version_decides_compatibility() {
        if python3().is_none() {
            return;
        }
        let root = std::env::temp_dir().join(format!(
            "glass-core-old-version-v2-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        fs::create_dir_all(&root).unwrap();
        let runtime = harness_mode(&root, true, None, "older-version-v2");
        assert_eq!(runtime.session_id(), "fixture-session");
        drop(runtime);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn resident_session_drives_a_turn_and_answers_host_tools() {
        if python3().is_none() {
            return;
        }
        let root = std::env::temp_dir().join(format!(
            "glass-core-harness-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        let executor: crate::resident_harness::AgentToolExecutor =
            Arc::new(|call: &ToolCall, _allow_mutation: bool, _confirmed: bool| {
                assert_eq!(call.name, "glass.workspace.list");
                Ok(json!({"text": "fixture contents"}))
            });
        let mut runtime = harness(&root, true, Some(executor));
        assert_eq!(runtime.session_id(), "fixture-session");

        runtime
            .start_request(PiSessionRequest::Prompt {
                text: "read the readme".into(),
                context: None,
            })
            .unwrap();
        let events = drain(&mut runtime, Duration::from_secs(5));
        let kinds = events
            .iter()
            .filter_map(|event| event.get("type").and_then(Value::as_str))
            .collect::<Vec<_>>();
        assert!(kinds.contains(&"message_update"), "{kinds:?}");
        assert!(kinds.contains(&"glass_tool_evidence"), "{kinds:?}");
        assert!(kinds.contains(&"agent_settled"), "{kinds:?}");
        let settled = events
            .iter()
            .find(|event| event.get("type").and_then(Value::as_str) == Some("agent_settled"))
            .unwrap();
        assert_eq!(settled.pointer("/detail/reason"), Some(&json!("completed")));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn mutations_park_on_an_approval_frame_until_answered() {
        if python3().is_none() {
            return;
        }
        let root = std::env::temp_dir().join(format!(
            "glass-core-approval-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        let executor: crate::resident_harness::AgentToolExecutor =
            Arc::new(|call: &ToolCall, allow_mutation: bool, confirmed: bool| {
                assert!(allow_mutation && confirmed);
                Ok(json!({"tool": call.name, "written": true}))
            });
        let mut runtime = harness(&root, false, Some(executor));
        runtime
            .start_request(PiSessionRequest::Prompt {
                text: "write the file".into(),
                context: None,
            })
            .unwrap();
        let events = drain(&mut runtime, Duration::from_secs(5));
        let approval = events
            .iter()
            .find(|event| {
                event.get("type").and_then(Value::as_str) == Some("glass_tool_approval_request")
            })
            .expect("mutation parks on an approval frame");
        let frame = approval["frameId"].as_str().unwrap().to_string();
        assert_eq!(approval["toolName"], json!("glass.git.commit"));
        assert!(approval["arguments"].is_object());
        runtime.resolve_tool_approval(&frame, true).unwrap();
        let events = drain(&mut runtime, Duration::from_secs(5));
        let kinds = events
            .iter()
            .filter_map(|event| event.get("type").and_then(Value::as_str))
            .collect::<Vec<_>>();
        assert!(kinds.contains(&"glass_tool_approval_resolved"), "{kinds:?}");
        assert!(kinds.contains(&"glass_tool_evidence"), "{kinds:?}");
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn resume_reuses_the_newest_workspace_session() {
        if python3().is_none() {
            return;
        }
        let root = std::env::temp_dir().join(format!(
            "glass-core-resume-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        let python = python3().unwrap();
        let runtime = GlassCoreRuntime::spawn(
            &root,
            CoreRuntimeOptions {
                session_dir: root.join("sessions"),
                resume: true,
                binary: Some(python),
                binary_args: vec![fixture().display().to_string()],
                ..CoreRuntimeOptions::default()
            },
        )
        .unwrap();
        // The fixture prefixes resumed ids, so this asserts that the stored
        // session for this workspace was listed and resumed.
        assert_eq!(runtime.session_id(), "resumed-fixture-session");
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn unknown_approval_frames_are_rejected() {
        if python3().is_none() {
            return;
        }
        let root = std::env::temp_dir().join(format!(
            "glass-core-frame-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        let mut runtime = harness(&root, false, None);
        let error = runtime
            .resolve_tool_approval("host:fixture:1:call-1", true)
            .unwrap_err();
        assert!(error.to_string().contains("no Glass Core tool call"));
        let _ = fs::remove_dir_all(&root);
    }
}
