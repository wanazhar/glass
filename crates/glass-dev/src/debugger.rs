//! Resident Debug Adapter Protocol runtime.
//!
//! The runtime owns one adapter process and speaks bounded DAP framing over
//! stdio. Higher-level debugger methods retain adapter-neutral JSON bodies so
//! LLDB, debugpy, Delve and JavaScript adapters can share one implementation.

use crate::development::{ProcessManager, ProcessSnapshot};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::{BTreeMap, VecDeque};
use std::fmt;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, TrySendError};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

const MAX_DAP_HEADER_BYTES: usize = 8 * 1024;
const MAX_DAP_MESSAGE_BYTES: usize = 4 * 1024 * 1024;
const MAX_PENDING_MESSAGES: usize = 512;
type DapWriter = Box<dyn Write + Send>;
type DapReader = Box<dyn Read + Send>;
type AdapterStreams = (DapWriter, DapReader, Vec<JoinHandle<()>>);

pub type DebugResult<T> = Result<T, DebugError>;

#[derive(Debug)]
pub enum DebugError {
    Io(std::io::Error),
    InvalidInput(String),
    Protocol(String),
    Adapter(String),
    Timeout(String),
}

impl fmt::Display for DebugError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => write!(formatter, "debug adapter I/O error: {error}"),
            Self::InvalidInput(message) => write!(formatter, "invalid debugger input: {message}"),
            Self::Protocol(message) => write!(formatter, "debug adapter protocol error: {message}"),
            Self::Adapter(message) => {
                write!(formatter, "debug adapter rejected request: {message}")
            }
            Self::Timeout(message) => write!(formatter, "debug adapter timeout: {message}"),
        }
    }
}

impl std::error::Error for DebugError {}

impl From<std::io::Error> for DebugError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DebugAdapterConfig {
    pub command: PathBuf,
    #[serde(default)]
    pub arguments: Vec<String>,
    #[serde(default)]
    pub transport: DebugAdapterTransport,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum DebugAdapterTransport {
    #[default]
    Stdio,
    Tcp {
        address: SocketAddr,
        #[serde(default = "default_connect_timeout_ms")]
        connect_timeout_ms: u64,
    },
}

const fn default_connect_timeout_ms() -> u64 {
    5_000
}

impl DebugAdapterConfig {
    pub fn new(command: impl Into<PathBuf>, arguments: impl IntoIterator<Item = String>) -> Self {
        Self {
            command: command.into(),
            arguments: arguments.into_iter().collect(),
            transport: DebugAdapterTransport::Stdio,
        }
    }

    pub fn with_tcp(mut self, address: SocketAddr, connect_timeout: Duration) -> Self {
        self.transport = DebugAdapterTransport::Tcp {
            address,
            connect_timeout_ms: u64::try_from(connect_timeout.as_millis()).unwrap_or(u64::MAX),
        };
        self
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum DebugSessionState {
    Starting,
    Initialized,
    Running,
    Stopped,
    Terminated,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DebugEvent {
    pub event: String,
    #[serde(default)]
    pub body: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DebuggerSnapshot {
    pub state: DebugSessionState,
    pub adapter_process_id: u32,
    pub capabilities: Value,
    pub breakpoints: BTreeMap<PathBuf, Vec<SourceBreakpoint>>,
    pub watches: Vec<String>,
    pub events: Vec<DebugEvent>,
    pub debuggee_processes: Vec<ProcessSnapshot>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SourceBreakpoint {
    pub line: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub condition: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hit_condition: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub log_message: Option<String>,
}

impl SourceBreakpoint {
    pub fn line(line: u64) -> Self {
        Self {
            line,
            condition: None,
            hit_condition: None,
            log_message: None,
        }
    }
}

/// One owned DAP adapter process with bounded request and event queues.
pub struct DapClient {
    child: Child,
    stdin: DapWriter,
    messages: Receiver<DebugResult<Value>>,
    reader: Option<JoinHandle<()>>,
    output_drains: Vec<JoinHandle<()>>,
    pending: VecDeque<Value>,
    next_sequence: u64,
    root: PathBuf,
    debuggees: ProcessManager,
}

impl DapClient {
    pub fn spawn(root: &Path, config: &DebugAdapterConfig) -> DebugResult<Self> {
        if !root.is_dir() {
            return Err(DebugError::InvalidInput(format!(
                "debugger project root is not a directory: {}",
                root.display()
            )));
        }
        if config.command.as_os_str().is_empty() {
            return Err(DebugError::InvalidInput(
                "debug adapter command must not be empty".into(),
            ));
        }
        let mut child = Command::new(&config.command)
            .args(&config.arguments)
            .current_dir(root)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;
        let child_stdin = child
            .stdin
            .take()
            .ok_or_else(|| DebugError::Protocol("adapter stdin was unavailable".into()))?;
        let child_stdout = child
            .stdout
            .take()
            .ok_or_else(|| DebugError::Protocol("adapter stdout was unavailable".into()))?;
        let child_stderr = child
            .stderr
            .take()
            .ok_or_else(|| DebugError::Protocol("adapter stderr was unavailable".into()))?;
        let (stdin, stdout, output_drains): AdapterStreams = match config.transport {
            DebugAdapterTransport::Stdio => (
                Box::new(child_stdin),
                Box::new(child_stdout),
                vec![spawn_output_drain(child_stderr)],
            ),
            DebugAdapterTransport::Tcp {
                address,
                connect_timeout_ms,
            } => {
                if !address.ip().is_loopback() || connect_timeout_ms == 0 {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(DebugError::InvalidInput(
                        "DAP TCP transport requires a loopback address and positive timeout".into(),
                    ));
                }
                drop(child_stdin);
                let output_drains = vec![
                    spawn_output_drain(child_stdout),
                    spawn_output_drain(child_stderr),
                ];
                let deadline = Instant::now() + Duration::from_millis(connect_timeout_ms);
                let stream = loop {
                    match TcpStream::connect_timeout(&address, Duration::from_millis(100)) {
                        Ok(stream) => break stream,
                        Err(error) => {
                            if child.try_wait()?.is_some() {
                                return Err(DebugError::Protocol(format!(
                                    "TCP debug adapter exited before accepting {address}: {error}"
                                )));
                            }
                            if Instant::now() >= deadline {
                                let _ = child.kill();
                                let _ = child.wait();
                                return Err(DebugError::Timeout(format!(
                                    "TCP debug adapter did not accept {address} within {connect_timeout_ms} ms"
                                )));
                            }
                        }
                    }
                };
                let reader = stream.try_clone()?;
                (Box::new(stream), Box::new(reader), output_drains)
            }
        };
        let (sender, messages) = mpsc::sync_channel(MAX_PENDING_MESSAGES);
        let reader = std::thread::spawn(move || {
            let mut stdout = BufReader::new(stdout);
            loop {
                match read_dap_message(&mut stdout) {
                    Ok(Some(message)) => match sender.try_send(Ok(message)) {
                        Ok(()) => {}
                        Err(TrySendError::Full(_) | TrySendError::Disconnected(_)) => break,
                    },
                    Ok(None) => break,
                    Err(error) => {
                        let _ = sender.try_send(Err(error));
                        break;
                    }
                }
            }
        });
        Ok(Self {
            child,
            stdin,
            messages,
            reader: Some(reader),
            output_drains,
            pending: VecDeque::new(),
            next_sequence: 1,
            root: root.to_path_buf(),
            debuggees: ProcessManager::new(root),
        })
    }

    pub fn process_id(&self) -> u32 {
        self.child.id()
    }

    pub fn request(
        &mut self,
        command: &str,
        arguments: Value,
        timeout: Duration,
    ) -> DebugResult<Value> {
        let sequence = self.send_request(command, arguments)?;
        self.wait_for_response(sequence, command, timeout)
    }

    fn send_request(&mut self, command: &str, arguments: Value) -> DebugResult<u64> {
        if command.is_empty() || command.len() > 128 {
            return Err(DebugError::InvalidInput(
                "DAP command must contain 1..=128 bytes".into(),
            ));
        }
        let sequence = self.take_sequence()?;
        write_dap_message(
            &mut self.stdin,
            &json!({
                "seq": sequence,
                "type": "request",
                "command": command,
                "arguments": arguments,
            }),
        )?;

        Ok(sequence)
    }

    fn wait_for_response(
        &mut self,
        sequence: u64,
        command: &str,
        timeout: Duration,
    ) -> DebugResult<Value> {
        let deadline = Instant::now() + timeout;
        loop {
            if let Some(response) = take_response(&mut self.pending, sequence) {
                return response_body(response, command);
            }
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return Err(DebugError::Timeout(format!(
                    "request {command} exceeded {} ms",
                    timeout.as_millis()
                )));
            }
            match self.messages.recv_timeout(remaining) {
                Ok(Ok(message)) => {
                    let Some(message) = self.route_inbound(message)? else {
                        continue;
                    };
                    if is_response_for(&message, sequence) {
                        return response_body(message, command);
                    }
                    push_pending(&mut self.pending, message)?;
                }
                Ok(Err(error)) => return Err(error),
                Err(RecvTimeoutError::Timeout) => {
                    return Err(DebugError::Timeout(format!(
                        "request {command} exceeded {} ms",
                        timeout.as_millis()
                    )));
                }
                Err(RecvTimeoutError::Disconnected) => {
                    return Err(DebugError::Protocol(
                        "debug adapter event stream closed".into(),
                    ));
                }
            }
        }
    }

    pub fn poll_events(&mut self) -> DebugResult<Vec<DebugEvent>> {
        while let Ok(message) = self.messages.try_recv() {
            if let Some(message) = self.route_inbound(message?)? {
                push_pending(&mut self.pending, message)?;
            }
        }
        let mut events = Vec::new();
        let mut retained = VecDeque::new();
        while let Some(message) = self.pending.pop_front() {
            if message.get("type").and_then(Value::as_str) == Some("event") {
                let event = message
                    .get("event")
                    .and_then(Value::as_str)
                    .ok_or_else(|| DebugError::Protocol("DAP event has no name".into()))?;
                events.push(DebugEvent {
                    event: event.to_string(),
                    body: message.get("body").cloned().unwrap_or(Value::Null),
                });
            } else {
                retained.push_back(message);
            }
        }
        self.pending = retained;
        Ok(events)
    }

    fn route_inbound(&mut self, message: Value) -> DebugResult<Option<Value>> {
        if message.get("type").and_then(Value::as_str) != Some("request") {
            return Ok(Some(message));
        }
        let request_sequence = message
            .get("seq")
            .and_then(Value::as_u64)
            .ok_or_else(|| DebugError::Protocol("DAP reverse request has no sequence".into()))?;
        let command = message
            .get("command")
            .and_then(Value::as_str)
            .ok_or_else(|| DebugError::Protocol("DAP reverse request has no command".into()))?;
        let result = match command {
            "runInTerminal" => self.run_in_terminal(
                request_sequence,
                message
                    .get("arguments")
                    .cloned()
                    .unwrap_or_else(|| json!({})),
            ),
            _ => Err(DebugError::Adapter(format!(
                "unsupported reverse request {command}"
            ))),
        };
        let (success, body, error) = match result {
            Ok(body) => (true, body, None),
            Err(error) => (false, Value::Null, Some(error.to_string())),
        };
        let response_sequence = self.take_sequence()?;
        let mut response = json!({
            "seq": response_sequence,
            "type": "response",
            "request_seq": request_sequence,
            "success": success,
            "command": command,
        });
        if success {
            response["body"] = body.clone();
        } else if let Some(error) = &error {
            response["message"] = json!(error);
        }
        write_dap_message(&mut self.stdin, &response)?;
        push_pending(
            &mut self.pending,
            json!({
                "type":"event",
                "event":"glass/reverseRequest",
                "body":{
                    "command":command,
                    "requestSequence":request_sequence,
                    "success":success,
                    "response":body,
                    "error":error,
                }
            }),
        )?;
        Ok(None)
    }

    fn run_in_terminal(&mut self, sequence: u64, arguments: Value) -> DebugResult<Value> {
        let argv = arguments
            .get("args")
            .and_then(Value::as_array)
            .ok_or_else(|| DebugError::InvalidInput("runInTerminal requires args".into()))?
            .iter()
            .map(|argument| {
                argument.as_str().map(str::to_string).ok_or_else(|| {
                    DebugError::InvalidInput("runInTerminal args must be strings".into())
                })
            })
            .collect::<DebugResult<Vec<_>>>()?;
        let cwd = arguments
            .get("cwd")
            .and_then(Value::as_str)
            .map(PathBuf::from)
            .unwrap_or_else(|| self.root.clone());
        let mut environment = BTreeMap::new();
        if let Some(values) = arguments.get("env") {
            let values = values.as_object().ok_or_else(|| {
                DebugError::InvalidInput("runInTerminal env must be an object".into())
            })?;
            for (key, value) in values {
                if value.is_null() {
                    continue;
                }
                let value = value.as_str().ok_or_else(|| {
                    DebugError::InvalidInput(
                        "runInTerminal environment values must be strings or null".into(),
                    )
                })?;
                environment.insert(key.clone(), value.to_string());
            }
        }
        let name = format!("dap-{sequence}");
        let snapshot = self
            .debuggees
            .start_argv(&name, &argv, &cwd, &environment)
            .map_err(|error| DebugError::Adapter(error.to_string()))?;
        Ok(json!({
            "processId": snapshot.pid,
            "shellProcessId": snapshot.pid,
            "glassProcess": snapshot,
        }))
    }

    fn take_sequence(&mut self) -> DebugResult<u64> {
        let sequence = self.next_sequence;
        self.next_sequence = self
            .next_sequence
            .checked_add(1)
            .ok_or_else(|| DebugError::Protocol("DAP sequence overflowed".into()))?;
        Ok(sequence)
    }

    pub fn debuggee_processes(&mut self) -> DebugResult<Vec<ProcessSnapshot>> {
        self.debuggees
            .list_checked()
            .map_err(|error| DebugError::Adapter(error.to_string()))
    }

    pub fn shutdown(&mut self) -> DebugResult<()> {
        if self.child.try_wait()?.is_none() {
            let _ = self.request(
                "disconnect",
                json!({"terminateDebuggee": true}),
                Duration::from_secs(2),
            );
            if self.child.try_wait()?.is_none() {
                self.child.kill()?;
            }
            let _ = self.child.wait();
        }
        if let Some(reader) = self.reader.take() {
            let _ = reader.join();
        }
        for drain in self.output_drains.drain(..) {
            let _ = drain.join();
        }
        Ok(())
    }
}

impl Drop for DapClient {
    fn drop(&mut self) {
        if self.child.try_wait().ok().flatten().is_none() {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
        if let Some(reader) = self.reader.take() {
            let _ = reader.join();
        }
        for drain in self.output_drains.drain(..) {
            let _ = drain.join();
        }
    }
}

fn spawn_output_drain(mut output: impl Read + Send + 'static) -> JoinHandle<()> {
    std::thread::spawn(move || {
        let mut sink = std::io::sink();
        let _ = std::io::copy(&mut output, &mut sink);
    })
}

/// Adapter-neutral debugger session state and typed operations.
pub struct DebuggerSession {
    client: DapClient,
    state: DebugSessionState,
    capabilities: Value,
    timeout: Duration,
    pending_start: Option<(u64, &'static str)>,
    breakpoints: BTreeMap<PathBuf, Vec<SourceBreakpoint>>,
    watches: Vec<String>,
    events: VecDeque<DebugEvent>,
}

impl DebuggerSession {
    pub fn start(
        root: &Path,
        config: &DebugAdapterConfig,
        client_name: &str,
        timeout: Duration,
    ) -> DebugResult<Self> {
        let mut client = DapClient::spawn(root, config)?;
        let capabilities = client.request(
            "initialize",
            json!({
                "clientID": "glass-dev",
                "clientName": client_name,
                "adapterID": "glass",
                "pathFormat": "path",
                "linesStartAt1": true,
                "columnsStartAt1": true,
                "supportsVariableType": true,
                "supportsVariablePaging": true,
                "supportsRunInTerminalRequest": true,
            }),
            timeout,
        )?;
        Ok(Self {
            client,
            state: DebugSessionState::Initialized,
            capabilities,
            timeout,
            pending_start: None,
            breakpoints: BTreeMap::new(),
            watches: Vec::new(),
            events: VecDeque::new(),
        })
    }

    pub fn state(&self) -> DebugSessionState {
        self.state
    }

    pub fn capabilities(&self) -> &Value {
        &self.capabilities
    }

    pub fn launch(&mut self, arguments: Value) -> DebugResult<Value> {
        self.begin_start("launch", arguments)
    }

    pub fn attach(&mut self, arguments: Value) -> DebugResult<Value> {
        self.begin_start("attach", arguments)
    }

    pub fn configuration_done(&mut self) -> DebugResult<Value> {
        let configuration = self
            .client
            .request("configurationDone", json!({}), self.timeout)?;
        let startup = if let Some((sequence, command)) = self.pending_start.take() {
            self.client
                .wait_for_response(sequence, command, self.timeout)?
        } else {
            Value::Null
        };
        self.state = DebugSessionState::Running;
        Ok(json!({"configurationDone": configuration, "startup": startup}))
    }

    fn begin_start(&mut self, command: &'static str, arguments: Value) -> DebugResult<Value> {
        if self.pending_start.is_some() {
            return Err(DebugError::InvalidInput(
                "a debugger launch or attach request is already pending".into(),
            ));
        }
        let sequence = self.client.send_request(command, arguments)?;
        self.pending_start = Some((sequence, command));
        self.state = DebugSessionState::Starting;
        Ok(json!({"pending": true, "requestSequence": sequence}))
    }

    pub fn restart(&mut self, arguments: Value) -> DebugResult<Value> {
        let body = self.client.request("restart", arguments, self.timeout)?;
        self.state = DebugSessionState::Running;
        Ok(body)
    }

    pub fn set_breakpoints(&mut self, path: &Path, lines: &[u64]) -> DebugResult<Value> {
        let breakpoints = lines
            .iter()
            .copied()
            .map(SourceBreakpoint::line)
            .collect::<Vec<_>>();
        self.set_source_breakpoints(path, &breakpoints)
    }

    pub fn set_source_breakpoints(
        &mut self,
        path: &Path,
        breakpoints: &[SourceBreakpoint],
    ) -> DebugResult<Value> {
        if breakpoints.len() > 256 || breakpoints.iter().any(|item| item.line == 0) {
            return Err(DebugError::InvalidInput(
                "breakpoints require at most 256 positive line numbers".into(),
            ));
        }
        let result = self.client.request(
            "setBreakpoints",
            json!({
                "source": {"path": path},
                "breakpoints": breakpoints,
                "sourceModified": false,
            }),
            self.timeout,
        )?;
        if breakpoints.is_empty() {
            self.breakpoints.remove(path);
        } else {
            self.breakpoints
                .insert(path.to_path_buf(), breakpoints.to_vec());
        }
        Ok(result)
    }

    pub fn set_exception_breakpoints(&mut self, filters: &[String]) -> DebugResult<Value> {
        self.client.request(
            "setExceptionBreakpoints",
            json!({"filters": filters}),
            self.timeout,
        )
    }

    pub fn continue_thread(&mut self, thread_id: i64) -> DebugResult<Value> {
        let body = self.client.request(
            "continue",
            json!({"threadId": positive_id(thread_id, "thread")?}),
            self.timeout,
        )?;
        self.state = DebugSessionState::Running;
        Ok(body)
    }

    pub fn pause(&mut self, thread_id: i64) -> DebugResult<Value> {
        self.client.request(
            "pause",
            json!({"threadId": positive_id(thread_id, "thread")?}),
            self.timeout,
        )
    }

    pub fn next(&mut self, thread_id: i64) -> DebugResult<Value> {
        self.step("next", thread_id)
    }

    pub fn step_in(&mut self, thread_id: i64) -> DebugResult<Value> {
        self.step("stepIn", thread_id)
    }

    pub fn step_out(&mut self, thread_id: i64) -> DebugResult<Value> {
        self.step("stepOut", thread_id)
    }

    fn step(&mut self, command: &str, thread_id: i64) -> DebugResult<Value> {
        let body = self.client.request(
            command,
            json!({"threadId": positive_id(thread_id, "thread")?}),
            self.timeout,
        )?;
        self.state = DebugSessionState::Running;
        Ok(body)
    }

    pub fn threads(&mut self) -> DebugResult<Value> {
        self.client.request("threads", json!({}), self.timeout)
    }

    pub fn stack_trace(&mut self, thread_id: i64) -> DebugResult<Value> {
        self.client.request(
            "stackTrace",
            json!({"threadId": positive_id(thread_id, "thread")?}),
            self.timeout,
        )
    }

    pub fn scopes(&mut self, frame_id: i64) -> DebugResult<Value> {
        self.client.request(
            "scopes",
            json!({"frameId": positive_id(frame_id, "frame")?}),
            self.timeout,
        )
    }

    pub fn variables(&mut self, variables_reference: i64) -> DebugResult<Value> {
        self.client.request(
            "variables",
            json!({"variablesReference": positive_id(variables_reference, "variables reference")?}),
            self.timeout,
        )
    }

    pub fn evaluate(
        &mut self,
        expression: &str,
        frame_id: Option<i64>,
        context: &str,
    ) -> DebugResult<Value> {
        if expression.is_empty() || expression.len() > 64 * 1024 {
            return Err(DebugError::InvalidInput(
                "debug evaluation expression must contain 1..=65536 bytes".into(),
            ));
        }
        let mut arguments = json!({"expression": expression, "context": context});
        if let Some(frame_id) = frame_id {
            arguments["frameId"] = json!(positive_id(frame_id, "frame")?);
        }
        let result = self.client.request("evaluate", arguments, self.timeout)?;
        if context == "watch" && !self.watches.iter().any(|watch| watch == expression) {
            if self.watches.len() == 128 {
                self.watches.remove(0);
            }
            self.watches.push(expression.to_string());
        }
        Ok(result)
    }

    pub fn poll_events(&mut self) -> DebugResult<Vec<DebugEvent>> {
        let events = self.client.poll_events()?;
        for event in &events {
            match event.event.as_str() {
                "stopped" => self.state = DebugSessionState::Stopped,
                "continued" => self.state = DebugSessionState::Running,
                "terminated" | "exited" => self.state = DebugSessionState::Terminated,
                _ => {}
            }
            if self.events.len() == 256 {
                self.events.pop_front();
            }
            self.events.push_back(event.clone());
        }
        Ok(events)
    }

    pub fn debuggee_processes(&mut self) -> DebugResult<Vec<ProcessSnapshot>> {
        self.client.debuggee_processes()
    }

    pub fn snapshot(&mut self) -> DebugResult<DebuggerSnapshot> {
        Ok(DebuggerSnapshot {
            state: self.state,
            adapter_process_id: self.client.process_id(),
            capabilities: self.capabilities.clone(),
            breakpoints: self.breakpoints.clone(),
            watches: self.watches.clone(),
            events: self.events.iter().cloned().collect(),
            debuggee_processes: self.client.debuggee_processes()?,
        })
    }

    pub fn disconnect(&mut self, terminate_debuggee: bool) -> DebugResult<Value> {
        let result = self.client.request(
            "disconnect",
            json!({"terminateDebuggee": terminate_debuggee}),
            self.timeout,
        );
        self.state = DebugSessionState::Terminated;
        result
    }

    pub fn terminate(&mut self, restart: bool) -> DebugResult<Value> {
        let result = self
            .client
            .request("terminate", json!({"restart": restart}), self.timeout);
        self.state = if restart {
            DebugSessionState::Starting
        } else {
            DebugSessionState::Terminated
        };
        result
    }

    pub fn shutdown(&mut self) -> DebugResult<()> {
        self.state = DebugSessionState::Terminated;
        self.client.shutdown()
    }
}

fn positive_id(value: i64, description: &str) -> DebugResult<i64> {
    (value > 0).then_some(value).ok_or_else(|| {
        DebugError::InvalidInput(format!("{description} ID must be a positive integer"))
    })
}

fn push_pending(pending: &mut VecDeque<Value>, message: Value) -> DebugResult<()> {
    if pending.len() == MAX_PENDING_MESSAGES {
        return Err(DebugError::Protocol(format!(
            "debug adapter exceeded the {MAX_PENDING_MESSAGES} pending message limit"
        )));
    }
    pending.push_back(message);
    Ok(())
}

fn take_response(pending: &mut VecDeque<Value>, sequence: u64) -> Option<Value> {
    let position = pending
        .iter()
        .position(|message| is_response_for(message, sequence))?;
    pending.remove(position)
}

fn is_response_for(message: &Value, sequence: u64) -> bool {
    message.get("type").and_then(Value::as_str) == Some("response")
        && message.get("request_seq").and_then(Value::as_u64) == Some(sequence)
}

fn response_body(message: Value, command: &str) -> DebugResult<Value> {
    if message.get("success").and_then(Value::as_bool) != Some(true) {
        let detail = message
            .get("message")
            .and_then(Value::as_str)
            .unwrap_or("adapter returned an unsuccessful response");
        return Err(DebugError::Adapter(format!("{command}: {detail}")));
    }
    Ok(message.get("body").cloned().unwrap_or(Value::Null))
}

fn read_dap_message(reader: &mut impl BufRead) -> DebugResult<Option<Value>> {
    let mut content_length = None;
    let mut header_bytes = 0usize;
    loop {
        let mut line = String::new();
        let read = reader.read_line(&mut line)?;
        if read == 0 {
            return if header_bytes == 0 {
                Ok(None)
            } else {
                Err(DebugError::Protocol("truncated DAP header".into()))
            };
        }
        header_bytes = header_bytes
            .checked_add(read)
            .ok_or_else(|| DebugError::Protocol("DAP header length overflowed".into()))?;
        if header_bytes > MAX_DAP_HEADER_BYTES {
            return Err(DebugError::Protocol(format!(
                "DAP header exceeds {MAX_DAP_HEADER_BYTES} bytes"
            )));
        }
        if line == "\r\n" || line == "\n" {
            break;
        }
        if let Some((name, value)) = line.trim().split_once(':')
            && name.eq_ignore_ascii_case("Content-Length")
        {
            content_length =
                Some(value.trim().parse::<usize>().map_err(|_| {
                    DebugError::Protocol("invalid DAP Content-Length header".into())
                })?);
        }
    }
    let content_length = content_length
        .ok_or_else(|| DebugError::Protocol("DAP message has no Content-Length header".into()))?;
    if content_length == 0 || content_length > MAX_DAP_MESSAGE_BYTES {
        return Err(DebugError::Protocol(format!(
            "DAP message length must be 1..={MAX_DAP_MESSAGE_BYTES} bytes"
        )));
    }
    let mut body = vec![0; content_length];
    reader.read_exact(&mut body)?;
    serde_json::from_slice(&body)
        .map_err(|error| DebugError::Protocol(format!("invalid DAP JSON: {error}")))
}

fn write_dap_message(writer: &mut impl Write, message: &Value) -> DebugResult<()> {
    let body = serde_json::to_vec(message)
        .map_err(|error| DebugError::Protocol(format!("could not encode DAP JSON: {error}")))?;
    if body.is_empty() || body.len() > MAX_DAP_MESSAGE_BYTES {
        return Err(DebugError::Protocol(format!(
            "DAP message length must be 1..={MAX_DAP_MESSAGE_BYTES} bytes"
        )));
    }
    write!(writer, "Content-Length: {}\r\n\r\n", body.len())?;
    writer.write_all(&body)?;
    writer.flush()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    const REVERSE_ADAPTER: &str = r#"
import json, sys

def read_message():
    length = None
    while True:
        line = sys.stdin.buffer.readline()
        if not line:
            return None
        if line in (b'\r\n', b'\n'):
            break
        name, value = line.decode().strip().split(':', 1)
        if name.lower() == 'content-length':
            length = int(value)
    return json.loads(sys.stdin.buffer.read(length))

def send(message):
    body = json.dumps(message, separators=(',', ':')).encode()
    sys.stdout.buffer.write(('Content-Length: %d\r\n\r\n' % len(body)).encode() + body)
    sys.stdout.buffer.flush()

request = read_message()
send({'seq': 1, 'type': 'response', 'request_seq': request['seq'], 'success': True,
      'command': request['command'], 'body': {}})
request = read_message()
send({'seq': 2, 'type': 'request', 'command': 'runInTerminal', 'arguments': {
    'kind': 'integrated', 'cwd': sys.argv[1],
    'args': [sys.executable, '-c', 'import time; print(\"glass-debuggee-ready\", flush=True); time.sleep(30)'],
    'env': {'GLASS_DAP_TEST': '1'}}})
terminal = read_message()
if terminal.get('success') is not True or not terminal.get('body', {}).get('processId'):
    raise SystemExit(41)
send({'seq': 3, 'type': 'request', 'command': 'startDebugging', 'arguments': {}})
unsupported = read_message()
if unsupported.get('success') is not False:
    raise SystemExit(42)
send({'seq': 4, 'type': 'response', 'request_seq': request['seq'], 'success': True,
      'command': request['command'], 'body': {'reverseRequestsHandled': True}})
for request in iter(read_message, None):
    if request.get('command') == 'disconnect':
        raise SystemExit(0)
    send({'seq': 5, 'type': 'response', 'request_seq': request['seq'], 'success': True,
          'command': request['command'], 'body': {}})
"#;

    const TCP_ADAPTER: &str = r#"
import json, socket, sys
server = socket.socket()
server.bind(('127.0.0.1', int(sys.argv[-1])))
server.listen(1)
stream, _ = server.accept()
reader = stream.makefile('rb')
writer = stream.makefile('wb')
length = None
while True:
    line = reader.readline()
    if line in (b'\r\n', b'\n'):
        break
    name, value = line.decode().strip().split(':', 1)
    if name.lower() == 'content-length':
        length = int(value)
request = json.loads(reader.read(length))
body = json.dumps({'seq': 1, 'type': 'response', 'request_seq': request['seq'],
                   'success': True, 'command': request['command'],
                   'body': {'transport': 'tcp'}}).encode()
writer.write(('Content-Length: %d\r\n\r\n' % len(body)).encode() + body)
writer.flush()
"#;

    #[test]
    fn dap_framing_round_trips_json() {
        let message = json!({"seq": 7, "type": "event", "event": "stopped"});
        let mut encoded = Vec::new();
        write_dap_message(&mut encoded, &message).unwrap();
        assert_eq!(
            read_dap_message(&mut BufReader::new(Cursor::new(encoded))).unwrap(),
            Some(message)
        );
    }

    #[test]
    fn dap_framing_rejects_missing_and_oversized_lengths() {
        let missing = b"X-Test: true\r\n\r\n{}";
        assert!(read_dap_message(&mut BufReader::new(&missing[..])).is_err());
        let oversized = format!("Content-Length: {}\r\n\r\n", MAX_DAP_MESSAGE_BYTES + 1);
        assert!(read_dap_message(&mut BufReader::new(oversized.as_bytes())).is_err());
    }

    #[test]
    fn unsuccessful_adapter_response_is_typed() {
        let error = response_body(
            json!({
                "type": "response",
                "request_seq": 1,
                "success": false,
                "message": "breakpoint refused"
            }),
            "setBreakpoints",
        )
        .unwrap_err();
        assert!(error.to_string().contains("breakpoint refused"));
    }

    #[test]
    fn debugger_ids_and_breakpoint_limits_fail_closed() {
        assert!(positive_id(0, "thread").is_err());
        assert!(positive_id(-1, "frame").is_err());
        assert_eq!(positive_id(1, "thread").unwrap(), 1);

        let breakpoint = SourceBreakpoint {
            line: 42,
            condition: Some("inventory == 0".into()),
            hit_condition: None,
            log_message: Some("inventory={inventory}".into()),
        };
        let encoded = serde_json::to_value(breakpoint).unwrap();
        assert_eq!(encoded["line"], 42);
        assert_eq!(encoded["condition"], "inventory == 0");
        assert!(encoded.get("hitCondition").is_none());
    }

    #[test]
    fn reverse_requests_are_supervised_bounded_and_observable() {
        let Some((root, config)) = python_adapter(REVERSE_ADAPTER) else {
            return;
        };
        let mut client = DapClient::spawn(&root, &config).unwrap();
        client
            .request("initialize", json!({}), Duration::from_secs(5))
            .unwrap();
        let response = client
            .request("exercise", json!({}), Duration::from_secs(5))
            .unwrap();
        assert_eq!(response["reverseRequestsHandled"], true);

        let deadline = Instant::now() + Duration::from_secs(3);
        let processes = loop {
            let processes = client.debuggee_processes().unwrap();
            if processes
                .first()
                .is_some_and(|process| process.output.contains("glass-debuggee-ready"))
            {
                break processes;
            }
            assert!(
                Instant::now() < deadline,
                "debuggee output was not observed"
            );
            std::thread::sleep(Duration::from_millis(10));
        };
        assert!(processes[0].pty);
        let process_id = processes[0].pid.unwrap();
        let events = client.poll_events().unwrap();
        assert_eq!(events.len(), 2);
        assert!(events[0].body["success"].as_bool().unwrap());
        assert!(!events[1].body["success"].as_bool().unwrap());
        drop(client);
        assert_process_exits(process_id);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn adapter_crash_and_timeout_are_typed_failures() {
        let Some((root, crash)) = python_adapter("raise SystemExit(7)") else {
            return;
        };
        let mut client = DapClient::spawn(&root, &crash).unwrap();
        let error = client
            .request("initialize", json!({}), Duration::from_secs(2))
            .unwrap_err();
        assert!(
            matches!(&error, DebugError::Io(_) | DebugError::Protocol(_)),
            "unexpected adapter crash error: {error}"
        );
        drop(client);

        let (timeout_root, timeout) = python_adapter("import time; time.sleep(30)").unwrap();
        let mut client = DapClient::spawn(&timeout_root, &timeout).unwrap();
        let error = client
            .request("initialize", json!({}), Duration::from_millis(50))
            .unwrap_err();
        assert!(matches!(error, DebugError::Timeout(_)));
        drop(client);
        let _ = std::fs::remove_dir_all(root);
        let _ = std::fs::remove_dir_all(timeout_root);
    }

    #[test]
    fn loopback_tcp_adapter_transport_is_owned_and_framed() {
        let Some((root, mut config)) = python_adapter(TCP_ADAPTER) else {
            return;
        };
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        drop(listener);
        config.arguments.push(address.port().to_string());
        config = config.with_tcp(address, Duration::from_secs(3));
        let mut client = DapClient::spawn(&root, &config).unwrap();
        let body = client
            .request("initialize", json!({}), Duration::from_secs(3))
            .unwrap();
        assert_eq!(body["transport"], "tcp");
        drop(client);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn real_lldb_adapter_supports_breakpoint_stack_and_continue() {
        let Some(adapter) = std::env::var_os("GLASS_LLDB_DAP") else {
            return;
        };
        let root = adapter_test_root("lldb");
        std::fs::create_dir_all(&root).unwrap();
        let source = root.join("main.rs");
        let program = root.join(if cfg!(windows) {
            "fixture.exe"
        } else {
            "fixture"
        });
        std::fs::write(
            &source,
            "fn main() {\n    let value = 41;\n    let answer = value + 1;\n    println!(\"{answer}\");\n}\n",
        )
        .unwrap();
        let status = std::process::Command::new("rustc")
            .args(["-C", "debuginfo=2", "-C", "opt-level=0"])
            .arg(&source)
            .arg("-o")
            .arg(&program)
            .status()
            .unwrap();
        assert!(status.success());
        let config = DebugAdapterConfig::new(PathBuf::from(adapter), []);
        let mut debugger =
            DebuggerSession::start(&root, &config, "Glass LLDB E2E", Duration::from_secs(20))
                .unwrap();
        debugger
            .launch(json!({"program":program,"cwd":root,"stopOnEntry":false}))
            .unwrap();
        let breakpoints = debugger.set_breakpoints(&source, &[3]).unwrap();
        assert!(
            breakpoints["breakpoints"]
                .as_array()
                .is_some_and(|items| !items.is_empty())
        );
        debugger.configuration_done().unwrap();
        let stopped = wait_for_debug_event(&mut debugger, "stopped", Duration::from_secs(20));
        let thread_id = stopped["threadId"].as_i64().unwrap();
        let stack = wait_for_stack_trace(&mut debugger, thread_id, Duration::from_secs(20));
        assert!(
            stack["stackFrames"]
                .as_array()
                .is_some_and(|items| !items.is_empty())
        );
        debugger.continue_thread(thread_id).unwrap();
        wait_for_debug_end(&mut debugger, Duration::from_secs(20));
        debugger.shutdown().unwrap();
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn real_delve_adapter_supports_tcp_breakpoint_stack_and_continue() {
        let Some(adapter) = std::env::var_os("GLASS_DELVE") else {
            return;
        };
        let root = adapter_test_root("delve");
        std::fs::create_dir_all(&root).unwrap();
        let source = root.join("main.go");
        std::fs::write(
            &source,
            "package main\n\nimport \"fmt\"\n\nfunc main() {\n\tvalue := 41\n\tfmt.Println(value + 1)\n}\n",
        )
        .unwrap();
        std::fs::write(
            root.join("go.mod"),
            "module glass.test/debugger\n\ngo 1.22\n",
        )
        .unwrap();
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        drop(listener);
        let config = DebugAdapterConfig::new(
            PathBuf::from(adapter),
            ["dap".into(), format!("--listen={address}")],
        )
        .with_tcp(address, Duration::from_secs(10));
        let mut debugger =
            DebuggerSession::start(&root, &config, "Glass Delve E2E", Duration::from_secs(30))
                .unwrap();
        debugger
            .launch(json!({"mode":"debug","program":root,"cwd":root,"stopOnEntry":false}))
            .unwrap();
        let breakpoints = debugger.set_breakpoints(&source, &[7]).unwrap();
        assert!(
            breakpoints["breakpoints"]
                .as_array()
                .is_some_and(|items| !items.is_empty())
        );
        debugger.configuration_done().unwrap();
        let stopped = wait_for_debug_event(&mut debugger, "stopped", Duration::from_secs(30));
        let thread_id = stopped["threadId"].as_i64().unwrap();
        let stack = wait_for_stack_trace(&mut debugger, thread_id, Duration::from_secs(30));
        assert!(
            stack["stackFrames"]
                .as_array()
                .is_some_and(|items| !items.is_empty())
        );
        debugger.continue_thread(thread_id).unwrap();
        wait_for_debug_end(&mut debugger, Duration::from_secs(30));
        debugger.shutdown().unwrap();
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn real_debugpy_adapter_supports_breakpoint_state_and_continue() {
        let Some(python) = std::env::var_os("GLASS_DEBUGPY_PYTHON") else {
            return;
        };
        let root =
            std::env::temp_dir().join(format!("glass-debugpy-fixture-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let program = root.join("program.py");
        std::fs::write(&program, "value = 41\nanswer = value + 1\nprint(answer)\n").unwrap();
        let config = DebugAdapterConfig::new(
            PathBuf::from(python),
            ["-m".to_string(), "debugpy.adapter".to_string()],
        );
        let mut debugger =
            DebuggerSession::start(&root, &config, "Glass DebugPy E2E", Duration::from_secs(15))
                .unwrap();
        debugger
            .launch(json!({
                "name":"Glass debugpy fixture",
                "type":"python",
                "request":"launch",
                "program":program,
                "console":"internalConsole",
                "justMyCode":false
            }))
            .unwrap();
        let breakpoints = debugger.set_breakpoints(&program, &[2]).unwrap();
        assert_eq!(breakpoints["breakpoints"][0]["verified"], true);
        debugger.configuration_done().unwrap();
        let stopped = wait_for_debug_event(&mut debugger, "stopped", Duration::from_secs(15));
        let thread_id = stopped["threadId"].as_i64().unwrap();
        let threads = debugger.threads().unwrap();
        assert!(
            threads["threads"]
                .as_array()
                .is_some_and(|items| !items.is_empty())
        );
        let stack = wait_for_stack_trace(&mut debugger, thread_id, Duration::from_secs(15));
        let frame_id = stack["stackFrames"][0]["id"].as_i64().unwrap();
        let scopes = debugger.scopes(frame_id).unwrap();
        let variables_reference = scopes["scopes"][0]["variablesReference"].as_i64().unwrap();
        let variables = debugger.variables(variables_reference).unwrap();
        assert!(
            variables["variables"]
                .as_array()
                .is_some_and(|items| { items.iter().any(|variable| variable["name"] == "value") })
        );
        let evaluated = debugger
            .evaluate("value + 1", Some(frame_id), "watch")
            .unwrap();
        assert_eq!(evaluated["result"], "42");
        debugger.continue_thread(thread_id).unwrap();
        wait_for_debug_end(&mut debugger, Duration::from_secs(15));
        debugger.shutdown().unwrap();
        let _ = std::fs::remove_dir_all(root);
    }

    fn wait_for_debug_event(
        debugger: &mut DebuggerSession,
        expected: &str,
        timeout: Duration,
    ) -> Value {
        let deadline = Instant::now() + timeout;
        loop {
            for event in debugger.poll_events().unwrap() {
                if event.event == expected {
                    return event.body;
                }
            }
            assert!(
                Instant::now() < deadline,
                "timed out waiting for {expected}"
            );
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    fn wait_for_stack_trace(
        debugger: &mut DebuggerSession,
        preferred_thread_id: i64,
        timeout: Duration,
    ) -> Value {
        let deadline = Instant::now() + timeout;
        loop {
            let mut errors = Vec::new();
            let mut thread_ids = vec![preferred_thread_id];
            if let Ok(threads) = debugger.threads()
                && let Some(threads) = threads["threads"].as_array()
            {
                thread_ids.extend(
                    threads
                        .iter()
                        .filter_map(|thread| thread["id"].as_i64())
                        .filter(|thread_id| *thread_id != preferred_thread_id),
                );
            }
            for thread_id in thread_ids {
                match debugger.stack_trace(thread_id) {
                    Ok(stack)
                        if stack["stackFrames"]
                            .as_array()
                            .is_some_and(|frames| !frames.is_empty()) =>
                    {
                        return stack;
                    }
                    Ok(_) => errors.push(format!("thread {thread_id} returned no stack frames")),
                    Err(error) => errors.push(error.to_string()),
                }
            }
            let last_error = errors
                .last()
                .map(String::as_str)
                .unwrap_or("adapter returned no stack frames");
            assert!(
                Instant::now() < deadline,
                "timed out waiting for stack trace: {last_error}"
            );
            std::thread::sleep(Duration::from_millis(25));
        }
    }

    fn wait_for_debug_end(debugger: &mut DebuggerSession, timeout: Duration) -> Value {
        let deadline = Instant::now() + timeout;
        loop {
            for event in debugger.poll_events().unwrap() {
                if matches!(event.event.as_str(), "exited" | "terminated") {
                    return event.body;
                }
            }
            assert!(Instant::now() < deadline, "timed out waiting for debug end");
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    fn python_adapter(script: &str) -> Option<(PathBuf, DebugAdapterConfig)> {
        if std::process::Command::new("python3")
            .arg("--version")
            .output()
            .is_err()
        {
            return None;
        }
        let root = adapter_test_root("python");
        std::fs::create_dir_all(&root).unwrap();
        let adapter = root.join("adapter.py");
        std::fs::write(&adapter, script).unwrap();
        Some((
            root.clone(),
            DebugAdapterConfig::new(
                PathBuf::from("python3"),
                [
                    "-u".into(),
                    adapter.display().to_string(),
                    root.display().to_string(),
                ],
            ),
        ))
    }

    fn adapter_test_root(label: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "glass-dap-{label}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
    }

    #[cfg(unix)]
    fn assert_process_exits(process_id: u32) {
        let deadline = Instant::now() + Duration::from_secs(3);
        loop {
            let result = unsafe { libc::kill(process_id as i32, 0) };
            if result != 0 && std::io::Error::last_os_error().raw_os_error() == Some(libc::ESRCH) {
                return;
            }
            assert!(
                Instant::now() < deadline,
                "debuggee process {process_id} survived owner drop"
            );
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    #[cfg(not(unix))]
    fn assert_process_exits(_process_id: u32) {}
}
