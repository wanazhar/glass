//! Resident Debug Adapter Protocol runtime.
//!
//! The runtime owns one adapter process and speaks bounded DAP framing over
//! stdio. Higher-level debugger methods retain adapter-neutral JSON bodies so
//! LLDB, debugpy, Delve and JavaScript adapters can share one implementation.

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::VecDeque;
use std::fmt;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, TrySendError};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

const MAX_DAP_HEADER_BYTES: usize = 8 * 1024;
const MAX_DAP_MESSAGE_BYTES: usize = 4 * 1024 * 1024;
const MAX_PENDING_MESSAGES: usize = 512;

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
}

impl DebugAdapterConfig {
    pub fn new(command: impl Into<PathBuf>, arguments: impl IntoIterator<Item = String>) -> Self {
        Self {
            command: command.into(),
            arguments: arguments.into_iter().collect(),
        }
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
    stdin: ChildStdin,
    messages: Receiver<DebugResult<Value>>,
    reader: Option<JoinHandle<()>>,
    stderr_drain: Option<JoinHandle<()>>,
    pending: VecDeque<Value>,
    next_sequence: u64,
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
        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| DebugError::Protocol("adapter stdin was unavailable".into()))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| DebugError::Protocol("adapter stdout was unavailable".into()))?;
        let stderr = child
            .stderr
            .take()
            .ok_or_else(|| DebugError::Protocol("adapter stderr was unavailable".into()))?;
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
        let stderr_drain = std::thread::spawn(move || {
            let mut stderr = stderr;
            let mut sink = std::io::sink();
            let _ = std::io::copy(&mut stderr, &mut sink);
        });
        Ok(Self {
            child,
            stdin,
            messages,
            reader: Some(reader),
            stderr_drain: Some(stderr_drain),
            pending: VecDeque::new(),
            next_sequence: 1,
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
        let sequence = self.next_sequence;
        self.next_sequence = self
            .next_sequence
            .checked_add(1)
            .ok_or_else(|| DebugError::Protocol("DAP sequence overflowed".into()))?;
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
            push_pending(&mut self.pending, message?)?;
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
        if let Some(stderr_drain) = self.stderr_drain.take() {
            let _ = stderr_drain.join();
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
        if let Some(stderr_drain) = self.stderr_drain.take() {
            let _ = stderr_drain.join();
        }
    }
}

/// Adapter-neutral debugger session state and typed operations.
pub struct DebuggerSession {
    client: DapClient,
    state: DebugSessionState,
    capabilities: Value,
    timeout: Duration,
    pending_start: Option<(u64, &'static str)>,
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
                "supportsRunInTerminalRequest": false,
            }),
            timeout,
        )?;
        Ok(Self {
            client,
            state: DebugSessionState::Initialized,
            capabilities,
            timeout,
            pending_start: None,
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
        self.client.request(
            "setBreakpoints",
            json!({
                "source": {"path": path},
                "breakpoints": breakpoints,
                "sourceModified": false,
            }),
            self.timeout,
        )
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
        self.client.request("evaluate", arguments, self.timeout)
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
        }
        Ok(events)
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
        let stack = debugger.stack_trace(thread_id).unwrap();
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
        wait_for_debug_event(&mut debugger, "terminated", Duration::from_secs(15));
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
}
