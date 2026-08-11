//! Glass-owned host for Pi's native `AgentSession` SDK runtime.

use crate::agents::ResidentAgentBroker;
use glass_browser::development::{DevelopmentError, DevelopmentResult, ToolCall};
use serde_json::{Value, json};
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::thread;
use std::time::{Duration, Instant};

const MAX_FRAME_BYTES: usize = 16 * 1024 * 1024;
const EVENT_CAPACITY: usize = 256;
const RUNTIME_SOURCE: &str = include_str!("../assets/pi-runtime.mjs");

#[derive(Debug, Clone)]
pub enum PiSessionRequest {
    Hello,
    Prompt { text: String },
    Steer { text: String },
    FollowUp { text: String },
    Abort,
    State,
    Models,
    SetModel { provider: String, model_id: String },
    SetThinking { level: String },
    NewSession,
    Compact { instructions: Option<String> },
    CloneSession,
    Fork { entry_id: String },
    SwitchSession { path: String },
    ListSessions,
    Entries { since: Option<String> },
    Tree,
    Messages,
    SessionStats,
    SetSessionName { name: String },
}

#[derive(Debug, Clone, Default)]
pub struct PiRuntimeOptions {
    pub unrestricted: bool,
    pub session_dir: PathBuf,
    pub name: Option<String>,
    pub model: Option<String>,
    pub thinking: Option<String>,
    pub broker: Option<ResidentAgentBroker>,
    pub additional_system_prompt: Option<String>,
    pub resume: bool,
}

pub struct GlassPiRuntime {
    child: Child,
    input: ChildStdin,
    output: Receiver<Result<Value, String>>,
    root: PathBuf,
    broker: Option<ResidentAgentBroker>,
    unrestricted: bool,
    next_id: u64,
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
        let agent_dir = runtime_path
            .parent()
            .ok_or_else(|| DevelopmentError::Process("Pi runtime cache has no parent".into()))?
            .join("agent-home");
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
            broker: options.broker,
            unrestricted: options.unrestricted,
            next_id: 1,
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
        let deadline = Instant::now() + timeout;
        loop {
            let Some(value) = self.recv_raw(deadline.saturating_duration_since(Instant::now()))?
            else {
                return Ok(None);
            };
            if value.get("type").and_then(Value::as_str) == Some("toolCall") {
                self.handle_tool_call(&value)?;
                continue;
            }
            return Ok(Some(value));
        }
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

    fn handle_tool_call(&mut self, value: &Value) -> DevelopmentResult<()> {
        let frame_id = value
            .get("id")
            .and_then(Value::as_str)
            .ok_or_else(|| DevelopmentError::Serialization("Pi tool frame has no id".into()))?;
        let call: ToolCall =
            serde_json::from_value(value.get("call").cloned().ok_or_else(|| {
                DevelopmentError::Serialization("Pi tool frame has no call".into())
            })?)?;
        let result = match &self.broker {
            Some(broker) => {
                let runtime = tokio::runtime::Builder::new_current_thread()
                    .enable_io()
                    .build()
                    .map_err(DevelopmentError::Io)?;
                runtime.block_on(crate::daemon::forward_resident_tool_call_with_context(
                    broker,
                    &call,
                    &self.root,
                    self.unrestricted,
                    self.unrestricted,
                ))
            }
            None => Err("resident Pi has no authoritative Glass daemon broker".into()),
        };
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

impl Drop for GlassPiRuntime {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn request_parts(request: PiSessionRequest) -> (&'static str, Value) {
    match request {
        PiSessionRequest::Hello => ("hello", Value::Null),
        PiSessionRequest::Prompt { text } => ("prompt", json!({"text": text})),
        PiSessionRequest::Steer { text } => ("steer", json!({"text": text})),
        PiSessionRequest::FollowUp { text } => ("followUp", json!({"text": text})),
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
    if let Some(path) = std::env::var_os("GLASS_PI_SDK_ENTRY") {
        return canonical_file(PathBuf::from(path), "GLASS_PI_SDK_ENTRY");
    }
    let local = Path::new(env!("CARGO_MANIFEST_DIR")).join(
        "../../packages/pi-runtime/node_modules/@earendil-works/pi-coding-agent/dist/index.js",
    );
    if local.is_file() {
        return fs::canonicalize(local).map_err(DevelopmentError::Io);
    }
    let pi = find_on_path("pi").ok_or_else(|| {
        DevelopmentError::NotFound(
            "Pi SDK; install @earendil-works/pi-coding-agent or set GLASS_PI_SDK_ENTRY".into(),
        )
    })?;
    let cli = fs::canonicalize(pi)?;
    canonical_file(cli.with_file_name("index.js"), "installed Pi SDK")
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

    #[test]
    fn request_mapping_uses_native_sdk_operations() {
        assert_eq!(request_parts(PiSessionRequest::Hello).0, "hello");
        assert_eq!(
            request_parts(PiSessionRequest::FollowUp {
                text: "next".into()
            })
            .0,
            "followUp"
        );
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
