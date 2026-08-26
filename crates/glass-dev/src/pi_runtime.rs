//! Glass-owned host for Pi's native `AgentSession` SDK runtime.

use crate::agents::ResidentAgentBroker;
use crate::development::{DevelopmentError, DevelopmentResult, ToolCall};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::VecDeque;
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::Arc;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const MAX_FRAME_BYTES: usize = 16 * 1024 * 1024;
const EVENT_CAPACITY: usize = 256;
const RUNTIME_SOURCE: &str = include_str!("../assets/pi-runtime.mjs");
pub const PINNED_PI_SDK_VERSION: &str = "0.84.3";
const MINIMUM_NODE_VERSION: (u64, u64, u64) = (22, 19, 0);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PiReadinessState {
    Ready,
    Missing,
    Incompatible,
    Expired,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PiReadinessComponent {
    pub state: PiReadinessState,
    pub version: Option<String>,
    pub path: Option<PathBuf>,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PiReadiness {
    pub ready: bool,
    pub node: PiReadinessComponent,
    pub sdk: PiReadinessComponent,
    pub authentication: PiReadinessComponent,
    pub provider: Option<String>,
    pub model: Option<String>,
    pub session: Option<String>,
    pub agent_dir: PathBuf,
    pub managed_root: PathBuf,
    pub remediation: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SelectedPiRuntime {
    sdk_entry: PathBuf,
    agent_dir: PathBuf,
}

#[derive(Debug, Clone)]
pub enum PiSessionRequest {
    Hello,
    Prompt {
        text: String,
        context: Option<Value>,
    },
    Steer {
        text: String,
        context: Option<Value>,
    },
    FollowUp {
        text: String,
        context: Option<Value>,
    },
    Abort,
    State,
    Models,
    SetModel {
        provider: String,
        model_id: String,
    },
    SetThinking {
        level: String,
    },
    NewSession,
    Compact {
        instructions: Option<String>,
    },
    CloneSession,
    Rewind {
        entry_id: String,
    },
    Fork {
        entry_id: String,
    },
    SwitchSession {
        path: String,
    },
    ListSessions,
    Entries {
        since: Option<String>,
    },
    Tree,
    Messages,
    SessionStats,
    SetSessionName {
        name: String,
    },
}

pub type PiToolExecutor =
    Arc<dyn Fn(&ToolCall, bool, bool) -> DevelopmentResult<Value> + Send + Sync>;

#[derive(Clone, Default)]
pub struct PiRuntimeOptions {
    pub unrestricted: bool,
    pub session_dir: PathBuf,
    pub name: Option<String>,
    pub model: Option<String>,
    pub thinking: Option<String>,
    pub broker: Option<ResidentAgentBroker>,
    pub local_tool_executor: Option<PiToolExecutor>,
    pub additional_system_prompt: Option<String>,
    pub resume: bool,
}

impl std::fmt::Debug for PiRuntimeOptions {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("PiRuntimeOptions")
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
            .finish()
    }
}

struct PendingPiToolApproval {
    frame_id: String,
    call: ToolCall,
}

pub struct GlassPiRuntime {
    child: Child,
    input: ChildStdin,
    output: Receiver<Result<Value, String>>,
    root: PathBuf,
    broker: Option<ResidentAgentBroker>,
    local_tool_executor: Option<PiToolExecutor>,
    unrestricted: bool,
    next_id: u64,
    host_events: VecDeque<Value>,
    pending_tool_approval: Option<PendingPiToolApproval>,
    aborting_turn: bool,
    unknown_tool_failures: u8,
}

impl std::fmt::Debug for GlassPiRuntime {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("GlassPiRuntime")
            .field("pid", &self.child.id())
            .field("root", &self.root)
            .finish_non_exhaustive()
    }
}

impl GlassPiRuntime {
    pub fn spawn(root: &Path, options: PiRuntimeOptions) -> DevelopmentResult<Self> {
        validate_options(&options)?;
        let root = fs::canonicalize(root)?;
        fs::create_dir_all(&options.session_dir)?;
        let session_dir = fs::canonicalize(&options.session_dir)?;
        let runtime_path = materialize_runtime()?;
        let sdk_entry = locate_sdk_entry()?;
        let agent_dir = active_agent_dir(&sdk_entry)?;
        fs::create_dir_all(&agent_dir)?;

        let mut command = Command::new("node");
        command
            .arg(&runtime_path)
            .env("GLASS_PI_SDK_ENTRY", sdk_entry)
            .env("GLASS_PI_CWD", &root)
            .env("GLASS_PI_SESSION_DIR", &session_dir)
            .env("GLASS_PI_AGENT_DIR", &agent_dir)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit());
        for (key, value) in [
            ("GLASS_PI_SESSION_NAME", options.name.as_deref()),
            ("GLASS_PI_MODEL", options.model.as_deref()),
            ("GLASS_PI_THINKING", options.thinking.as_deref()),
            (
                "GLASS_PI_SYSTEM_PROMPT",
                options.additional_system_prompt.as_deref(),
            ),
        ] {
            if let Some(value) = value {
                command.env(key, value);
            }
        }
        if options.resume {
            command.env("GLASS_PI_RESUME", "1");
        }

        let mut child = command.spawn().map_err(|error| {
            DevelopmentError::Process(format!("failed to start Glass Pi SDK runtime: {error}"))
        })?;
        let input = child
            .stdin
            .take()
            .ok_or_else(|| DevelopmentError::Process("Pi runtime stdin is unavailable".into()))?;
        let output_stream = child
            .stdout
            .take()
            .ok_or_else(|| DevelopmentError::Process("Pi runtime stdout is unavailable".into()))?;
        let (sender, output) = mpsc::sync_channel(EVENT_CAPACITY);
        thread::Builder::new()
            .name("glass-pi-sdk-output".into())
            .spawn(move || read_frames(output_stream, sender))?;

        let mut runtime = Self {
            child,
            input,
            output,
            root,
            local_tool_executor: options.local_tool_executor,
            broker: options.broker,
            unrestricted: options.unrestricted,
            next_id: 1,
            host_events: VecDeque::new(),
            pending_tool_approval: None,
            aborting_turn: false,
            unknown_tool_failures: 0,
        };
        match runtime.recv_raw(Duration::from_secs(15))? {
            Some(value) if value.get("type").and_then(Value::as_str) == Some("ready") => {
                Ok(runtime)
            }
            Some(value) => Err(DevelopmentError::Process(format!(
                "Pi SDK runtime emitted an unexpected startup frame: {value}"
            ))),
            None => Err(DevelopmentError::Process(
                "Pi SDK runtime did not become ready within 15 seconds".into(),
            )),
        }
    }

    pub fn start_request(&mut self, request: PiSessionRequest) -> DevelopmentResult<String> {
        let (operation, params) = request_parts(request);
        let id = format!("glass-{}", self.next_id);
        self.next_id = self.next_id.saturating_add(1);
        self.send(&json!({"id": id, "operation": operation, "params": params}))?;
        Ok(id)
    }

    pub fn recv_event_timeout(&mut self, timeout: Duration) -> DevelopmentResult<Option<Value>> {
        if let Some(event) = self.host_events.pop_front() {
            return Ok(Some(event));
        }
        let deadline = Instant::now() + timeout;
        loop {
            if let Some(event) = self.host_events.pop_front() {
                return Ok(Some(event));
            }
            let Some(value) = self.recv_raw(deadline.saturating_duration_since(Instant::now()))?
            else {
                return Ok(None);
            };
            if value.get("type").and_then(Value::as_str) == Some("toolCall") {
                if let Some(event) = self.handle_tool_call(&value)? {
                    return Ok(Some(event));
                }
                continue;
            }
            if value.get("type").and_then(Value::as_str) == Some("agent_settled") {
                self.aborting_turn = false;
                self.unknown_tool_failures = 0;
            }
            return Ok(Some(value));
        }
    }

    pub fn resolve_tool_approval(
        &mut self,
        frame_id: &str,
        approved: bool,
    ) -> DevelopmentResult<()> {
        let pending = self.pending_tool_approval.as_ref().ok_or_else(|| {
            DevelopmentError::Conflict("no Pi tool call is awaiting approval".into())
        })?;
        if pending.frame_id != frame_id {
            return Err(DevelopmentError::Conflict(format!(
                "Pi approval frame {} is not the pending frame",
                frame_id
            )));
        }
        let pending = self
            .pending_tool_approval
            .take()
            .ok_or_else(|| DevelopmentError::Conflict("Pi approval request disappeared".into()))?;
        let result = if approved {
            self.execute_tool_call(&pending.call, true, true)
        } else {
            Err(DevelopmentError::Conflict(
                "Glass denied this Pi tool call".into(),
            ))
        };
        self.send_tool_result(&pending.frame_id, &result)?;
        self.host_events.push_back(json!({
            "type": "glass_tool_approval_resolved",
            "frameId": pending.frame_id,
            "toolName": pending.call.name,
            "approved": approved,
            "ok": result.is_ok(),
        }));
        if approved {
            self.host_events
                .push_back(browser_evidence(&pending.call, &result));
        }
        Ok(())
    }

    fn recv_raw(&mut self, timeout: Duration) -> DevelopmentResult<Option<Value>> {
        match self.output.recv_timeout(timeout) {
            Ok(Ok(value)) => Ok(Some(value)),
            Ok(Err(error)) => Err(DevelopmentError::Process(error)),
            Err(RecvTimeoutError::Timeout) => Ok(None),
            Err(RecvTimeoutError::Disconnected) => Err(DevelopmentError::Process(
                "Pi SDK runtime event stream closed".into(),
            )),
        }
    }

    fn handle_tool_call(&mut self, value: &Value) -> DevelopmentResult<Option<Value>> {
        let frame_id = value
            .get("id")
            .and_then(Value::as_str)
            .ok_or_else(|| DevelopmentError::Serialization("Pi tool frame has no id".into()))?;
        let call: ToolCall = normalize_tool_call(serde_json::from_value(
            value.get("call").cloned().ok_or_else(|| {
                DevelopmentError::Serialization("Pi tool frame has no call".into())
            })?,
        )?);
        if !self.unrestricted && crate::tools::tool_requires_mutation(&call.name) {
            if self.pending_tool_approval.is_some() {
                return Err(DevelopmentError::Conflict(
                    "Pi emitted a second tool call while approval was pending".into(),
                ));
            }
            self.pending_tool_approval = Some(PendingPiToolApproval {
                frame_id: frame_id.into(),
                call: call.clone(),
            });
            return Ok(Some(json!({
                "type": "glass_tool_approval_request",
                "frameId": frame_id,
                "toolName": call.name,
                "arguments": redact_tool_arguments(&call.arguments),
            })));
        }
        let result = self.execute_tool_call(&call, self.unrestricted, self.unrestricted);
        let unknown_tool = is_unknown_tool_error(&result);
        let result = if unknown_tool {
            result.map_err(|error| {
                DevelopmentError::InvalidInput(format!(
                    "{error}; use a registered Glass capability such as `read`, `edit`, `bash`, `grep`, or `glass_tool`"
                ))
            })
        } else {
            result
        };
        self.send_tool_result(frame_id, &result)?;
        self.host_events.push_back(browser_evidence(&call, &result));
        if unknown_tool && !self.aborting_turn {
            self.unknown_tool_failures = self.unknown_tool_failures.saturating_add(1);
            if self.unknown_tool_failures >= 3 {
                self.aborting_turn = true;
                let abort_id = self.start_request(PiSessionRequest::Abort)?;
                self.host_events.push_back(json!({
                    "type": "glass_tool_rejected",
                    "toolName": call.name,
                    "reason": "Repeated unknown Glass tool calls; the turn was aborted after three recoverable failures",
                    "recoverable": false,
                    "attempt": self.unknown_tool_failures,
                    "abortRequestId": abort_id,
                }));
            } else {
                self.host_events.push_back(json!({
                    "type": "glass_tool_rejected",
                    "toolName": call.name,
                    "reason": "Glass returned an unknown tool error; the tool result was returned so Pi can retry with a registered tool",
                    "recoverable": true,
                    "attempt": self.unknown_tool_failures,
                }));
            }
        }
        Ok(None)
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
                "resident Pi has no authoritative Glass daemon broker".into(),
            )),
        }
    }

    fn send_tool_result(
        &mut self,
        frame_id: &str,
        result: &DevelopmentResult<Value>,
    ) -> DevelopmentResult<()> {
        let response = match result {
            Ok(result) => json!({
                "operation": "toolResult", "id": frame_id, "ok": true, "result": result,
            }),
            Err(error) => json!({
                "operation": "toolResult", "id": frame_id, "ok": false,
                "error": error.to_string(),
            }),
        };
        self.send(&response)
    }

    fn send(&mut self, value: &Value) -> DevelopmentResult<()> {
        let encoded = serde_json::to_vec(value)?;
        if encoded.is_empty() || encoded.len() > MAX_FRAME_BYTES {
            return Err(DevelopmentError::InvalidInput(
                "Pi SDK command exceeds the framed protocol limit".into(),
            ));
        }
        self.input
            .write_all(&(encoded.len() as u32).to_be_bytes())?;
        self.input.write_all(&encoded)?;
        self.input.flush()?;
        Ok(())
    }
}

fn normalize_tool_call(mut call: ToolCall) -> ToolCall {
    let requested_name = call.name.clone();
    let canonical = match requested_name.as_str() {
        "read" | "glass.fs.read" => Some("glass.file.read"),
        "ls" | "glass.fs.list" => Some("glass.file.list"),
        "grep" | "glass.fs.grep" => Some("glass.file.grep"),
        "find" | "glass.fs.find" => Some("glass.file.find"),
        "write" | "glass.fs.write" => Some("glass.file.write"),
        "edit" | "glass.fs.edit" => Some("glass.file.edit"),
        "glass.fs.patch" => Some("glass.file.patch"),
        "glass.fs.mkdir" => Some("glass.file.mkdir"),
        "glass.fs.rename" => Some("glass.file.rename"),
        "glass.fs.delete" | "glass.fs.remove" => Some("glass.file.delete"),
        "bash" | "shell" | "terminal" | "glass.fs.shell" => Some("glass.command.run"),
        _ => None,
    };
    let Some(canonical) = canonical else {
        return call;
    };
    call.name = canonical.into();
    if canonical == "glass.command.run" {
        let mut arguments = match call.arguments {
            Value::Object(arguments) => arguments,
            _ => serde_json::Map::new(),
        };
        if !arguments.contains_key("name") {
            let id = call
                .id
                .chars()
                .map(|character| {
                    if character.is_ascii_alphanumeric() || matches!(character, '-' | '_') {
                        character
                    } else {
                        '-'
                    }
                })
                .take(48)
                .collect::<String>();
            arguments.insert("name".into(), Value::String(format!("pi-{id}")));
        }
        if let Some(timeout) = arguments.remove("timeout") {
            arguments.insert("timeoutSeconds".into(), timeout);
        }
        call.arguments = Value::Object(arguments);
    }
    call
}

fn is_unknown_tool_error(result: &DevelopmentResult<Value>) -> bool {
    matches!(
        result,
        Err(DevelopmentError::NotFound(message)) if message.starts_with("tool ")
    )
}
fn browser_evidence(call: &ToolCall, result: &DevelopmentResult<Value>) -> Value {
    let browser = call.name.starts_with("glass.browser.");
    let mut evidence = serde_json::Map::new();
    evidence.insert(
        "type".into(),
        Value::String(
            if browser {
                "glass_browser_evidence"
            } else {
                "glass_tool_evidence"
            }
            .into(),
        ),
    );
    evidence.insert("toolName".into(), Value::String(call.name.clone()));
    evidence.insert("ok".into(), Value::Bool(result.is_ok()));
    if let Err(error) = result {
        evidence.insert(
            "error".into(),
            Value::String(redact_text(&error.to_string())),
        );
    }
    if let Ok(value) = result {
        for key in [
            "browserRevision",
            "currentRevision",
            "finalRevision",
            "targetId",
            "url",
            "title",
            "workflowState",
        ] {
            if let Some(value) = value
                .get(key)
                .filter(|value| value.is_string() || value.is_number() || value.is_boolean())
            {
                let value = if key == "url" {
                    value
                        .as_str()
                        .map(redact_url)
                        .map(Value::String)
                        .unwrap_or_else(|| value.clone())
                } else {
                    value.clone()
                };
                evidence.insert(key.into(), value);
            }
        }
        if let Some(revision) = value
            .pointer("/accessibility/revision")
            .and_then(Value::as_u64)
        {
            evidence.insert("browserRevision".into(), Value::from(revision));
        }
        if let Some(url) = value.pointer("/page/url").and_then(Value::as_str) {
            evidence.insert("url".into(), Value::String(redact_url(url)));
        }
        if let Some(title) = value.pointer("/page/title").and_then(Value::as_str) {
            evidence.insert("title".into(), Value::String(title.into()));
        }
        if let Some(count) = value
            .pointer("/accessibility/interactive")
            .and_then(Value::as_array)
            .map(Vec::len)
        {
            evidence.insert("semanticEntityCount".into(), Value::from(count));
        }
    }
    Value::Object(evidence)
}
fn redact_url(url: &str) -> String {
    url.split(['?', '#'])
        .next()
        .unwrap_or(url)
        .chars()
        .take(2_048)
        .collect()
}

fn redact_tool_arguments(value: &Value) -> Value {
    match value {
        Value::Object(object) => Value::Object(
            object
                .iter()
                .map(|(key, value)| {
                    let redacted = key.to_ascii_lowercase();
                    let value = if [
                        "password",
                        "token",
                        "secret",
                        "cookie",
                        "authorization",
                        "api_key",
                        "apikey",
                    ]
                    .iter()
                    .any(|needle| redacted.contains(needle))
                    {
                        Value::String("[redacted]".into())
                    } else {
                        redact_tool_arguments(value)
                    };
                    (key.clone(), value)
                })
                .collect(),
        ),
        Value::Array(values) => Value::Array(values.iter().map(redact_tool_arguments).collect()),
        _ => value.clone(),
    }
}

fn redact_text(text: &str) -> String {
    text.chars().take(2_048).collect()
}

impl Drop for GlassPiRuntime {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn request_parts(request: PiSessionRequest) -> (&'static str, Value) {
    match request {
        PiSessionRequest::Hello => ("hello", Value::Null),
        PiSessionRequest::Prompt { text, context } => {
            ("prompt", json!({"text": text, "context": context}))
        }
        PiSessionRequest::Steer { text, context } => {
            ("steer", json!({"text": text, "context": context}))
        }
        PiSessionRequest::FollowUp { text, context } => {
            ("followUp", json!({"text": text, "context": context}))
        }
        PiSessionRequest::Abort => ("abort", Value::Null),
        PiSessionRequest::State => ("state", Value::Null),
        PiSessionRequest::Models => ("models", Value::Null),
        PiSessionRequest::SetModel { provider, model_id } => (
            "setModel",
            json!({"provider": provider, "modelId": model_id}),
        ),
        PiSessionRequest::SetThinking { level } => ("setThinking", json!({"level": level})),
        PiSessionRequest::NewSession => ("newSession", Value::Null),
        PiSessionRequest::Compact { instructions } => {
            ("compact", json!({"instructions": instructions}))
        }
        PiSessionRequest::CloneSession => ("cloneSession", Value::Null),
        PiSessionRequest::Rewind { entry_id } => ("rewind", json!({"entryId": entry_id})),
        PiSessionRequest::Fork { entry_id } => ("fork", json!({"entryId": entry_id})),
        PiSessionRequest::SwitchSession { path } => ("switchSession", json!({"path": path})),
        PiSessionRequest::ListSessions => ("listSessions", Value::Null),
        PiSessionRequest::Entries { since } => ("entries", json!({"since": since})),
        PiSessionRequest::Tree => ("tree", Value::Null),
        PiSessionRequest::Messages => ("messages", Value::Null),
        PiSessionRequest::SessionStats => ("stats", Value::Null),
        PiSessionRequest::SetSessionName { name } => ("setName", json!({"name": name})),
    }
}

fn read_frames(mut stream: impl Read, sender: mpsc::SyncSender<Result<Value, String>>) {
    loop {
        let mut header = [0_u8; 4];
        if let Err(error) = stream.read_exact(&mut header) {
            let _ = sender.send(Err(format!("Pi SDK runtime frame header failed: {error}")));
            return;
        }
        let length = u32::from_be_bytes(header) as usize;
        if length == 0 || length > MAX_FRAME_BYTES {
            let _ = sender.send(Err("Pi SDK runtime emitted an invalid frame length".into()));
            return;
        }
        let mut encoded = vec![0_u8; length];
        if let Err(error) = stream.read_exact(&mut encoded) {
            let _ = sender.send(Err(format!("Pi SDK runtime frame body failed: {error}")));
            return;
        }
        let value = serde_json::from_slice(&encoded).map_err(|error| error.to_string());
        if sender.send(value).is_err() {
            return;
        }
    }
}

pub fn pi_readiness() -> DevelopmentResult<PiReadiness> {
    let managed_root = managed_pi_root()?;
    let node = node_readiness();
    let sdk_candidate = locate_sdk_candidate();
    let (sdk, agent_dir) = match sdk_candidate {
        Ok((path, source)) => {
            let version = sdk_version(&path);
            let compatible = version
                .as_deref()
                .and_then(parse_version)
                .is_some_and(|version| version >= (0, 84, 1));
            let state = if compatible {
                PiReadinessState::Ready
            } else {
                PiReadinessState::Incompatible
            };
            let agent_dir = active_agent_dir(&path)?;
            (
                PiReadinessComponent {
                    state,
                    version,
                    path: Some(path),
                    detail: if compatible {
                        format!("compatible {source} Pi SDK")
                    } else {
                        format!("{source} Pi SDK version is missing or older than 0.84.3")
                    },
                },
                agent_dir,
            )
        }
        Err(error) => (
            PiReadinessComponent {
                state: PiReadinessState::Missing,
                version: None,
                path: None,
                detail: error.to_string(),
            },
            managed_agent_dir()?,
        ),
    };
    let (authentication, provider) = authentication_readiness(&agent_dir);
    let ready = node.state == PiReadinessState::Ready
        && sdk.state == PiReadinessState::Ready
        && authentication.state == PiReadinessState::Ready;
    let mut remediation = Vec::new();
    if node.state != PiReadinessState::Ready {
        remediation.push(format!(
            "Install Node {}.{}.{} or newer, then run `glass agent setup`.",
            MINIMUM_NODE_VERSION.0, MINIMUM_NODE_VERSION.1, MINIMUM_NODE_VERSION.2
        ));
    }
    if sdk.state != PiReadinessState::Ready {
        remediation.push("Run `glass agent setup` to install the pinned managed Pi SDK, or select an existing SDK with `--sdk-entry`.".into());
    }
    if authentication.state != PiReadinessState::Ready {
        remediation.push("Run `glass agent setup --login`, use Pi `/login`, or configure a supported provider API-key environment variable.".into());
    }
    Ok(PiReadiness {
        ready,
        node,
        sdk,
        authentication,
        provider,
        model: None,
        session: newest_session(&agent_dir),
        agent_dir,
        managed_root,
        remediation,
    })
}

pub fn setup_pi_runtime(
    sdk_entry: Option<&Path>,
    agent_dir: Option<&Path>,
    update: bool,
    login: bool,
) -> DevelopmentResult<PiReadiness> {
    let node = node_readiness();
    if node.state != PiReadinessState::Ready {
        return Err(DevelopmentError::Process(node.detail));
    }
    let managed_root = managed_pi_root()?;
    fs::create_dir_all(&managed_root)?;
    let selected = if let Some(entry) = sdk_entry {
        let entry = canonical_file(entry.to_path_buf(), "selected Pi SDK")?;
        let agent_dir = match agent_dir {
            Some(path) => canonical_directory(path, "selected Pi agent directory")?,
            None => default_pi_agent_dir()?,
        };
        SelectedPiRuntime {
            sdk_entry: entry,
            agent_dir,
        }
    } else {
        let entry = managed_sdk_entry()?;
        let installed = sdk_version(&entry).as_deref() == Some(PINNED_PI_SDK_VERSION);
        if update || !installed {
            install_managed_sdk(&managed_root)?;
        }
        SelectedPiRuntime {
            sdk_entry: canonical_file(managed_sdk_entry()?, "managed Pi SDK")?,
            agent_dir: managed_agent_dir()?,
        }
    };
    fs::create_dir_all(&selected.agent_dir)?;
    write_selected_runtime(&selected)?;
    if login {
        run_pi_login(&selected)?;
    }
    pi_readiness()
}

fn node_readiness() -> PiReadinessComponent {
    let path = find_on_path(if cfg!(windows) { "node.exe" } else { "node" });
    let Some(path) = path else {
        return PiReadinessComponent {
            state: PiReadinessState::Missing,
            version: None,
            path: None,
            detail: "Node is not available on PATH".into(),
        };
    };
    match Command::new(&path).arg("--version").output() {
        Ok(output) if output.status.success() => {
            let version = String::from_utf8_lossy(&output.stdout)
                .trim()
                .trim_start_matches('v')
                .to_string();
            let compatible =
                parse_version(&version).is_some_and(|value| value >= MINIMUM_NODE_VERSION);
            PiReadinessComponent {
                state: if compatible {
                    PiReadinessState::Ready
                } else {
                    PiReadinessState::Incompatible
                },
                version: Some(version.clone()),
                path: Some(path),
                detail: if compatible {
                    "compatible Node runtime".into()
                } else {
                    format!("Node {version} is older than 22.19.0")
                },
            }
        }
        Ok(output) => PiReadinessComponent {
            state: PiReadinessState::Unknown,
            version: None,
            path: Some(path),
            detail: format!("Node version check exited {}", output.status),
        },
        Err(error) => PiReadinessComponent {
            state: PiReadinessState::Unknown,
            version: None,
            path: Some(path),
            detail: format!("Node version check failed: {error}"),
        },
    }
}

fn authentication_readiness(agent_dir: &Path) -> (PiReadinessComponent, Option<String>) {
    const PROVIDER_KEYS: [(&str, &str); 10] = [
        ("ANTHROPIC_API_KEY", "anthropic"),
        ("OPENAI_API_KEY", "openai"),
        ("GEMINI_API_KEY", "google"),
        ("GOOGLE_API_KEY", "google"),
        ("MISTRAL_API_KEY", "mistral"),
        ("GROQ_API_KEY", "groq"),
        ("XAI_API_KEY", "xai"),
        ("OPENROUTER_API_KEY", "openrouter"),
        ("ZAI_API_KEY", "zai"),
        ("AWS_BEARER_TOKEN_BEDROCK", "amazon-bedrock"),
    ];
    if let Some((_, provider)) = PROVIDER_KEYS
        .iter()
        .find(|(key, _)| std::env::var_os(key).is_some())
    {
        return (
            PiReadinessComponent {
                state: PiReadinessState::Ready,
                version: None,
                path: None,
                detail: format!("{provider} authentication is available from the environment"),
            },
            Some((*provider).into()),
        );
    }
    let path = agent_dir.join("auth.json");
    let encoded = match fs::read(&path) {
        Ok(encoded) => encoded,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return (
                PiReadinessComponent {
                    state: PiReadinessState::Missing,
                    version: None,
                    path: Some(path),
                    detail: "no Pi credentials are configured".into(),
                },
                None,
            );
        }
        Err(error) => {
            return (
                PiReadinessComponent {
                    state: PiReadinessState::Unknown,
                    version: None,
                    path: Some(path),
                    detail: format!("Pi credential metadata is unreadable: {error}"),
                },
                None,
            );
        }
    };
    let value: Value = match serde_json::from_slice(&encoded) {
        Ok(value) => value,
        Err(error) => {
            return (
                PiReadinessComponent {
                    state: PiReadinessState::Unknown,
                    version: None,
                    path: Some(path),
                    detail: format!("Pi credential metadata is invalid JSON: {error}"),
                },
                None,
            );
        }
    };
    let providers = value
        .as_object()
        .map(|items| items.keys().take(8).cloned().collect::<Vec<_>>())
        .unwrap_or_default();
    if providers.is_empty() {
        return (
            PiReadinessComponent {
                state: PiReadinessState::Missing,
                version: None,
                path: Some(path),
                detail: "Pi credential file contains no providers".into(),
            },
            None,
        );
    }
    let expired = value
        .as_object()
        .is_some_and(|items| items.values().all(credential_expired));
    let state = if expired {
        PiReadinessState::Expired
    } else {
        PiReadinessState::Ready
    };
    (
        PiReadinessComponent {
            state,
            version: None,
            path: Some(path),
            detail: if expired {
                "all stored Pi credentials appear expired".into()
            } else {
                format!("stored Pi credentials for {}", providers.join(", "))
            },
        },
        providers.first().cloned(),
    )
}

fn credential_expired(value: &Value) -> bool {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    ["expires", "expiresAt", "expires_at", "expiration"]
        .iter()
        .find_map(|key| value.get(key))
        .and_then(|value| value.as_u64().or_else(|| value.as_str()?.parse().ok()))
        .is_some_and(|expires| {
            let seconds = if expires > 10_000_000_000 {
                expires / 1_000
            } else {
                expires
            };
            seconds <= now
        })
}

fn newest_session(agent_dir: &Path) -> Option<String> {
    let sessions = agent_dir.join("sessions");
    let mut newest = None;
    for entry in fs::read_dir(sessions).ok()? {
        let entry = entry.ok()?;
        let modified = entry.metadata().ok()?.modified().ok()?;
        if newest
            .as_ref()
            .is_none_or(|(current, _)| modified > *current)
        {
            newest = Some((modified, entry.file_name().to_string_lossy().into_owned()));
        }
    }
    newest.map(|(_, name)| name)
}

fn materialize_runtime() -> DevelopmentResult<PathBuf> {
    let base = dirs::cache_dir()
        .ok_or_else(|| DevelopmentError::Process("user cache directory is unavailable".into()))?
        .join("glass")
        .join("pi-sdk-v1");
    fs::create_dir_all(&base)?;
    let path = base.join("runtime.mjs");
    if fs::read_to_string(&path).ok().as_deref() != Some(RUNTIME_SOURCE) {
        let temporary = base.join(format!("runtime-{}.tmp", std::process::id()));
        fs::write(&temporary, RUNTIME_SOURCE)?;
        fs::rename(temporary, &path)?;
    }
    Ok(path)
}

fn locate_sdk_entry() -> DevelopmentResult<PathBuf> {
    locate_sdk_candidate().map(|(path, _)| path)
}

fn locate_sdk_candidate() -> DevelopmentResult<(PathBuf, &'static str)> {
    if let Some(path) = std::env::var_os("GLASS_PI_SDK_ENTRY") {
        return canonical_file(PathBuf::from(path), "GLASS_PI_SDK_ENTRY")
            .map(|path| (path, "environment-selected"));
    }
    if let Some(selected) = read_selected_runtime()
        && let Ok(path) = canonical_file(selected.sdk_entry, "selected Pi SDK")
    {
        return Ok((path, "Glass-selected"));
    }
    let managed = managed_sdk_entry()?;
    if managed.is_file() {
        return fs::canonicalize(managed)
            .map(|path| (path, "Glass-managed"))
            .map_err(DevelopmentError::Io);
    }
    let local = Path::new(env!("CARGO_MANIFEST_DIR")).join(
        "../../packages/pi-runtime/node_modules/@earendil-works/pi-coding-agent/dist/index.js",
    );
    if local.is_file() {
        return fs::canonicalize(local)
            .map(|path| (path, "repository"))
            .map_err(DevelopmentError::Io);
    }
    let pi = find_on_path("pi").ok_or_else(|| {
        DevelopmentError::NotFound(
            "Pi SDK is not ready; run `glass agent setup` or select an existing installation"
                .into(),
        )
    })?;
    let cli = fs::canonicalize(pi)?;
    canonical_file(cli.with_file_name("index.js"), "installed Pi SDK").map(|path| (path, "system"))
}

fn active_agent_dir(sdk_entry: &Path) -> DevelopmentResult<PathBuf> {
    if let Some(path) = std::env::var_os("GLASS_PI_AGENT_DIR") {
        let path = PathBuf::from(path);
        fs::create_dir_all(&path)?;
        return fs::canonicalize(path).map_err(DevelopmentError::Io);
    }
    if let Some(selected) = read_selected_runtime()
        && canonical_file(selected.sdk_entry, "selected Pi SDK")
            .is_ok_and(|selected| selected == sdk_entry)
    {
        fs::create_dir_all(&selected.agent_dir)?;
        return fs::canonicalize(selected.agent_dir).map_err(DevelopmentError::Io);
    }
    managed_agent_dir()
}

fn managed_pi_root() -> DevelopmentResult<PathBuf> {
    dirs::data_local_dir()
        .ok_or_else(|| DevelopmentError::Process("user local-data directory is unavailable".into()))
        .map(|root| root.join("glass").join("runtime").join("pi"))
}

fn managed_agent_dir() -> DevelopmentResult<PathBuf> {
    Ok(managed_pi_root()?.join("agent"))
}

fn default_pi_agent_dir() -> DevelopmentResult<PathBuf> {
    dirs::home_dir()
        .ok_or_else(|| DevelopmentError::Process("user home directory is unavailable".into()))
        .map(|home| home.join(".pi").join("agent"))
}

fn managed_sdk_entry() -> DevelopmentResult<PathBuf> {
    Ok(managed_pi_root()?
        .join("node_modules")
        .join("@earendil-works")
        .join("pi-coding-agent")
        .join("dist")
        .join("index.js"))
}

fn selected_runtime_path() -> DevelopmentResult<PathBuf> {
    Ok(managed_pi_root()?.join("selected-runtime.json"))
}

fn read_selected_runtime() -> Option<SelectedPiRuntime> {
    let path = selected_runtime_path().ok()?;
    serde_json::from_slice(&fs::read(path).ok()?).ok()
}

fn write_selected_runtime(selected: &SelectedPiRuntime) -> DevelopmentResult<()> {
    let path = selected_runtime_path()?;
    let parent = path.parent().ok_or_else(|| {
        DevelopmentError::Process("selected Pi runtime path has no parent".into())
    })?;
    fs::create_dir_all(parent)?;
    let temporary = parent.join(format!("selected-runtime-{}.tmp", std::process::id()));
    fs::write(&temporary, serde_json::to_vec_pretty(selected)?)?;
    fs::rename(temporary, path)?;
    Ok(())
}

fn install_managed_sdk(root: &Path) -> DevelopmentResult<()> {
    let manifest = serde_json::json!({
        "name":"@glass-dev/managed-pi-runtime",
        "private":true,
        "dependencies":{"@earendil-works/pi-coding-agent":PINNED_PI_SDK_VERSION}
    });
    fs::write(
        root.join("package.json"),
        serde_json::to_vec_pretty(&manifest)?,
    )?;
    let status = Command::new(if cfg!(windows) { "npm.cmd" } else { "npm" })
        .args([
            "install",
            "--ignore-scripts",
            "--no-audit",
            "--no-fund",
            "--save-exact",
            &format!("@earendil-works/pi-coding-agent@{PINNED_PI_SDK_VERSION}"),
        ])
        .current_dir(root)
        .status()
        .map_err(|error| DevelopmentError::Process(format!("failed to start npm: {error}")))?;
    if !status.success() {
        return Err(DevelopmentError::Process(format!(
            "managed Pi SDK installation exited {status}"
        )));
    }
    Ok(())
}

fn run_pi_login(selected: &SelectedPiRuntime) -> DevelopmentResult<()> {
    let managed_cli = managed_pi_root()?
        .join("node_modules")
        .join(".bin")
        .join(if cfg!(windows) { "pi.cmd" } else { "pi" });
    let cli = if managed_cli.is_file() {
        managed_cli
    } else {
        find_on_path(if cfg!(windows) { "pi.cmd" } else { "pi" })
            .ok_or_else(|| DevelopmentError::NotFound("Pi CLI for interactive `/login`".into()))?
    };
    eprintln!(
        "Glass is opening Pi with its selected credential directory. Run `/login`, then exit Pi to return to Glass."
    );
    let status = Command::new(cli)
        .env("PI_CODING_AGENT_DIR", &selected.agent_dir)
        .status()
        .map_err(|error| DevelopmentError::Process(format!("failed to open Pi login: {error}")))?;
    if !status.success() {
        return Err(DevelopmentError::Process(format!(
            "Pi login session exited {status}"
        )));
    }
    Ok(())
}

fn sdk_version(entry: &Path) -> Option<String> {
    let package = entry.parent()?.parent()?.join("package.json");
    let value: Value = serde_json::from_slice(&fs::read(package).ok()?).ok()?;
    value.get("version")?.as_str().map(str::to_string)
}

fn parse_version(value: &str) -> Option<(u64, u64, u64)> {
    let core = value
        .trim()
        .trim_start_matches('v')
        .split(['-', '+'])
        .next()?;
    let mut parts = core.split('.');
    Some((
        parts.next()?.parse().ok()?,
        parts.next()?.parse().ok()?,
        parts.next()?.parse().ok()?,
    ))
}

fn canonical_directory(path: &Path, label: &str) -> DevelopmentResult<PathBuf> {
    let canonical = fs::canonicalize(path).map_err(|error| {
        DevelopmentError::Process(format!(
            "{label} is unavailable at {}: {error}",
            path.display()
        ))
    })?;
    if !canonical.is_dir() {
        return Err(DevelopmentError::Process(format!(
            "{label} is not a directory: {}",
            canonical.display()
        )));
    }
    Ok(canonical)
}

fn canonical_file(path: PathBuf, label: &str) -> DevelopmentResult<PathBuf> {
    let canonical = fs::canonicalize(&path).map_err(|error| {
        DevelopmentError::Process(format!(
            "{label} is unavailable at {}: {error}",
            path.display()
        ))
    })?;
    if !canonical.is_file() {
        return Err(DevelopmentError::Process(format!(
            "{label} is not a file: {}",
            canonical.display()
        )));
    }
    Ok(canonical)
}

fn find_on_path(name: &str) -> Option<PathBuf> {
    std::env::var_os("PATH").and_then(|path| {
        std::env::split_paths(&path)
            .map(|directory| directory.join(name))
            .find(|candidate| candidate.is_file())
    })
}

fn validate_options(options: &PiRuntimeOptions) -> DevelopmentResult<()> {
    if options.session_dir.as_os_str().is_empty() {
        return Err(DevelopmentError::InvalidInput(
            "Pi session directory is required".into(),
        ));
    }
    for (label, value, limit) in [
        ("Pi session name", options.name.as_deref(), 1024),
        ("Pi model", options.model.as_deref(), 1024),
        ("Pi thinking level", options.thinking.as_deref(), 128),
        (
            "Pi system prompt",
            options.additional_system_prompt.as_deref(),
            128 * 1024,
        ),
    ] {
        if value.is_some_and(|value| value.len() > limit || value.contains('\0')) {
            return Err(DevelopmentError::InvalidInput(format!("invalid {label}")));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    fn response(runtime: &mut GlassPiRuntime, request: PiSessionRequest) -> Value {
        let id = runtime.start_request(request).unwrap();
        loop {
            let value = runtime
                .recv_event_timeout(Duration::from_secs(10))
                .unwrap()
                .expect("native SDK response");
            if value.get("type").and_then(Value::as_str) == Some("response")
                && value.get("id").and_then(Value::as_str) == Some(&id)
            {
                return value;
            }
        }
    }
    fn unknown_tool_rejection_count(runtime: &GlassPiRuntime) -> usize {
        runtime
            .host_events
            .iter()
            .filter(|event| {
                event.get("type").and_then(Value::as_str) == Some("glass_tool_rejected")
            })
            .count()
    }

    #[test]
    fn request_mapping_uses_native_sdk_operations() {
        assert_eq!(request_parts(PiSessionRequest::Hello).0, "hello");
        assert_eq!(
            request_parts(PiSessionRequest::FollowUp {
                text: "next".into(),
                context: None,
            })
            .0,
            "followUp"
        );
        let (_, params) = request_parts(PiSessionRequest::Prompt {
            text: "inspect".into(),
            context: Some(json!({
                "schemaVersion": "glass.agent-context.v1",
                "browser": {"browserRevision": 7}
            })),
        });
        assert_eq!(params["context"]["browser"]["browserRevision"], 7);
        assert_eq!(request_parts(PiSessionRequest::SessionStats).0, "stats");
    }

    #[test]
    fn runtime_asset_never_invokes_pi_rpc_cli() {
        assert!(!RUNTIME_SOURCE.contains("--mode"));
        assert!(!RUNTIME_SOURCE.contains("--mode rpc"));
        assert!(RUNTIME_SOURCE.contains("createAgentSession"));
        assert!(RUNTIME_SOURCE.contains("SessionManager"));
    }

    #[test]
    fn runtime_asset_registers_governed_custom_tools_without_builtins() {
        assert!(RUNTIME_SOURCE.contains("noTools: \"builtin\""));
        assert!(RUNTIME_SOURCE.contains("tools: [\"glass_tool\", \"delegate\", \"read\""));
        assert!(RUNTIME_SOURCE.contains("\"glass.agent.delegate\""));
        assert!(RUNTIME_SOURCE.contains("customTools: [glassTool, ...nativeTools]"));
    }
    #[test]
    fn unknown_tool_call_recovers_then_aborts_once() {
        if locate_sdk_entry().is_err() {
            return;
        }
        let root =
            std::env::temp_dir().join(format!("glass-pi-unknown-tool-test-{}", std::process::id()));
        let sessions = root.join("sessions");
        fs::create_dir_all(&root).unwrap();
        let executor: PiToolExecutor =
            Arc::new(|_, _, _| Err(DevelopmentError::NotFound("tool glass.fs.list".into())));
        let mut runtime = GlassPiRuntime::spawn(
            &root,
            PiRuntimeOptions {
                session_dir: sessions,
                local_tool_executor: Some(executor),
                ..PiRuntimeOptions::default()
            },
        )
        .unwrap();
        let call = serde_json::json!({
            "type": "toolCall",
            "id": "unknown-tool-frame",
            "call": {"id": "unknown-tool-call", "name": "glass.fs.list", "arguments": {}}
        });

        assert!(runtime.handle_tool_call(&call).unwrap().is_none());
        assert!(!runtime.aborting_turn);
        assert_eq!(unknown_tool_rejection_count(&runtime), 1);
        assert_eq!(
            runtime
                .host_events
                .back()
                .and_then(|event| event.get("attempt")),
            Some(&json!(1))
        );

        assert!(runtime.handle_tool_call(&call).unwrap().is_none());
        assert!(!runtime.aborting_turn);
        assert_eq!(unknown_tool_rejection_count(&runtime), 2);

        assert!(runtime.handle_tool_call(&call).unwrap().is_none());
        assert!(runtime.aborting_turn);
        assert_eq!(unknown_tool_rejection_count(&runtime), 3);
        assert_eq!(
            runtime
                .host_events
                .back()
                .and_then(|event| event.get("recoverable")),
            Some(&json!(false))
        );

        assert!(runtime.handle_tool_call(&call).unwrap().is_none());
        assert_eq!(unknown_tool_rejection_count(&runtime), 3);
        drop(runtime);
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn readiness_version_and_expiry_checks_are_deterministic() {
        assert_eq!(parse_version("v22.19.0"), Some((22, 19, 0)));
        assert_eq!(parse_version("0.84.3-beta.1"), Some((0, 84, 3)));
        assert_eq!(parse_version("invalid"), None);
        assert!(credential_expired(&serde_json::json!({"expires":1})));
        assert!(!credential_expired(&serde_json::json!({
            "expires":9_999_999_999_u64
        })));
        assert!(!credential_expired(&serde_json::json!({"type":"api_key"})));
    }

    #[test]
    fn browser_evidence_redacts_url_query_and_fragment() {
        assert_eq!(
            redact_url("https://example.test/orders/7?token=secret#receipt"),
            "https://example.test/orders/7"
        );
    }

    #[test]
    fn native_sdk_starts_and_reports_capabilities_when_installed() {
        if locate_sdk_entry().is_err() {
            return;
        }
        let root =
            std::env::temp_dir().join(format!("glass-pi-native-sdk-test-{}", std::process::id()));
        let sessions = root.join("sessions");
        fs::create_dir_all(&root).unwrap();
        let mut runtime = GlassPiRuntime::spawn(
            &root,
            PiRuntimeOptions {
                session_dir: sessions,
                name: Some("native SDK contract test".into()),
                ..PiRuntimeOptions::default()
            },
        )
        .unwrap();
        let hello = response(&mut runtime, PiSessionRequest::Hello);
        assert_eq!(hello.get("ok").and_then(Value::as_bool), Some(true));
        assert_eq!(
            hello.pointer("/result/protocol").and_then(Value::as_str),
            Some("glass-pi-sdk-v1")
        );
        let named = response(
            &mut runtime,
            PiSessionRequest::SetSessionName {
                name: "renamed SDK session".into(),
            },
        );
        assert_eq!(named.get("ok").and_then(Value::as_bool), Some(true));
        assert_eq!(
            response(&mut runtime, PiSessionRequest::SessionStats)
                .get("ok")
                .and_then(Value::as_bool),
            Some(true)
        );
        assert!(
            response(&mut runtime, PiSessionRequest::Tree)
                .pointer("/result")
                .is_some()
        );
        assert!(
            response(&mut runtime, PiSessionRequest::Messages)
                .pointer("/result")
                .is_some()
        );
        let cloned = response(&mut runtime, PiSessionRequest::CloneSession);
        assert_eq!(
            cloned.get("ok").and_then(Value::as_bool),
            Some(false),
            "{cloned}"
        );
        assert!(
            cloned
                .get("error")
                .and_then(Value::as_str)
                .is_some_and(|error| error.contains("empty or invalid"))
        );
        assert_eq!(
            response(&mut runtime, PiSessionRequest::NewSession)
                .get("ok")
                .and_then(Value::as_bool),
            Some(true)
        );
        assert!(
            response(&mut runtime, PiSessionRequest::ListSessions)
                .pointer("/result")
                .and_then(Value::as_array)
                .is_some()
        );
        let rejected = response(
            &mut runtime,
            PiSessionRequest::SwitchSession {
                path: root.join("outside.jsonl").display().to_string(),
            },
        );
        assert_eq!(rejected.get("ok").and_then(Value::as_bool), Some(false));
        drop(runtime);
        fs::remove_dir_all(&root).unwrap();
    }
}
