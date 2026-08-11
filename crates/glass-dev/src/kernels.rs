//! Named persistent execution kernels for Glass Dev.

use rusqlite::types::ValueRef;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, TrySendError};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

const MAX_KERNEL_OUTPUT_BYTES: usize = 1024 * 1024;
const MAX_KERNEL_CODE_BYTES: usize = 512 * 1024;
const MAX_KERNEL_SESSIONS: usize = 32;
const MAX_KERNEL_MESSAGES: usize = 64;
const MAX_KERNEL_TOOL_CALLS: usize = 32;
const DEFAULT_KERNEL_TIMEOUT: Duration = Duration::from_secs(30);

const PYTHON_WORKER: &str = r#"
import ast, contextlib, io, json, sys, traceback
class GlassBindings:
    def __init__(self): self.sequence = 0
    def call(self, tool, arguments=None):
        self.sequence += 1
        request_id = "python-%d" % self.sequence
        sys.__stdout__.write(json.dumps({"kind":"toolCall", "id":request_id, "tool":tool, "arguments":arguments or {}}, ensure_ascii=True) + "\n")
        sys.__stdout__.flush()
        response = json.loads(sys.__stdin__.readline())
        if response.get("kind") != "toolResult" or response.get("id") != request_id:
            raise RuntimeError("invalid Glass tool response")
        if not response.get("ok"):
            raise RuntimeError(response.get("error", "Glass tool call failed"))
        return response.get("result")
state = {"__name__": "__glass_kernel__"}
state["glass"] = GlassBindings()
for line in sys.stdin:
    try:
        request = json.loads(line)
        source = request["code"]
        output = io.StringIO()
        result = None
        with contextlib.redirect_stdout(output), contextlib.redirect_stderr(output):
            tree = ast.parse(source, mode="exec")
            if tree.body and isinstance(tree.body[-1], ast.Expr):
                prefix = ast.Module(body=tree.body[:-1], type_ignores=[])
                exec(compile(prefix, "<glass>", "exec"), state, state)
                result = eval(compile(ast.Expression(tree.body[-1].value), "<glass>", "eval"), state, state)
            else:
                exec(compile(tree, "<glass>", "exec"), state, state)
        response = {"kind":"executionResult", "ok": True, "output": output.getvalue(), "value": repr(result) if result is not None else None}
    except Exception:
        response = {"kind":"executionResult", "ok": False, "output": "", "error": traceback.format_exc(limit=16)}
    print(json.dumps(response, ensure_ascii=True), flush=True)
"#;

const JAVASCRIPT_WORKER: &str = r#"
const readline = require('readline');
const vm = require('vm');
const context = vm.createContext({});
let toolSequence = 0;
const pendingTools = new Map();
context.glass = {
  call: (tool, arguments = {}) => new Promise((resolve, reject) => {
    const id = `javascript-${++toolSequence}`;
    pendingTools.set(id, { resolve, reject });
    process.stdout.write(JSON.stringify({ kind: 'toolCall', id, tool, arguments }) + '\n');
  }),
};
const input = readline.createInterface({ input: process.stdin, crlfDelay: Infinity });
input.on('line', async (line) => {
  let request;
  try {
    request = JSON.parse(line);
    if (request.kind === 'toolResult') {
      const pending = pendingTools.get(request.id);
      if (!pending) throw new Error('unknown Glass tool response');
      pendingTools.delete(request.id);
      if (request.ok) pending.resolve(request.result);
      else pending.reject(new Error(request.error || 'Glass tool call failed'));
      return;
    }
    const output = [];
    context.console = {
      log: (...values) => output.push(values.map(String).join(' ')),
      error: (...values) => output.push(values.map(String).join(' ')),
    };
    let value = new vm.Script(request.code, { filename: '<glass>' })
      .runInContext(context, { timeout: request.timeoutMs });
    if (value && typeof value.then === 'function') value = await value;
    process.stdout.write(JSON.stringify({ kind: 'executionResult', ok: true, output: output.join('\n') + (output.length ? '\n' : ''), value: value === undefined ? null : String(value) }) + '\n');
  } catch (error) {
    process.stdout.write(JSON.stringify({ kind: 'executionResult', ok: false, output: '', error: String(error && error.stack || error) }) + '\n');
  }
});
"#;

pub type KernelResult<T> = Result<T, KernelError>;

#[derive(Debug)]
pub enum KernelError {
    Io(std::io::Error),
    Sql(rusqlite::Error),
    InvalidInput(String),
    Unavailable(String),
    Protocol(String),
    Execution(String),
    Timeout(String),
}

impl fmt::Display for KernelError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => write!(formatter, "kernel I/O error: {error}"),
            Self::Sql(error) => write!(formatter, "SQL kernel error: {error}"),
            Self::InvalidInput(message) => write!(formatter, "invalid kernel input: {message}"),
            Self::Unavailable(message) => write!(formatter, "kernel unavailable: {message}"),
            Self::Protocol(message) => write!(formatter, "kernel protocol error: {message}"),
            Self::Execution(message) => write!(formatter, "kernel execution failed: {message}"),
            Self::Timeout(message) => write!(formatter, "kernel execution timed out: {message}"),
        }
    }
}

impl std::error::Error for KernelError {}

impl From<std::io::Error> for KernelError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
    }
}

impl From<rusqlite::Error> for KernelError {
    fn from(error: rusqlite::Error) -> Self {
        Self::Sql(error)
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum KernelKind {
    Python,
    JavaScript,
    Shell,
    Sql,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum KernelState {
    Ready,
    Failed,
    Stopped,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct KernelSnapshot {
    pub name: String,
    pub kind: KernelKind,
    pub state: KernelState,
    pub executions: u64,
    pub actor_id: String,
    pub workspace_revision: u64,
    #[serde(default)]
    pub capabilities: Vec<String>,
    #[serde(default)]
    pub mutation_authority: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct KernelExecution {
    pub name: String,
    pub kind: KernelKind,
    pub sequence: u64,
    pub actor_id: String,
    pub initiator_id: String,
    pub executor_id: String,
    pub workspace_revision: u64,
    pub duration_ms: u64,
    pub output: String,
    pub value: Value,
    pub truncated: bool,
    pub tool_calls: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct KernelToolCall {
    pub id: String,
    pub tool: String,
    #[serde(default)]
    pub arguments: Value,
}

enum KernelBackend {
    Json(JsonProcessKernel),
    Shell(ShellKernel),
    Sql(SqlKernel),
}

struct KernelSession {
    snapshot: KernelSnapshot,
    backend: KernelBackend,
    capabilities: BTreeSet<String>,
}

pub struct KernelManager {
    root: PathBuf,
    sessions: BTreeMap<String, KernelSession>,
}

impl KernelManager {
    pub fn new(root: impl AsRef<Path>) -> KernelResult<Self> {
        Ok(Self {
            root: root.as_ref().canonicalize()?,
            sessions: BTreeMap::new(),
        })
    }

    pub fn start(&mut self, name: &str, kind: KernelKind, actor_id: &str) -> KernelResult<()> {
        self.start_governed(name, kind, actor_id, &[], false)
    }

    pub fn start_governed(
        &mut self,
        name: &str,
        kind: KernelKind,
        actor_id: &str,
        capabilities: &[String],
        mutation_authority: bool,
    ) -> KernelResult<()> {
        validate_name(name, "kernel")?;
        validate_actor_id(actor_id, "kernel actor")?;
        let capabilities = validate_capabilities(capabilities)?;
        if self.sessions.contains_key(name) {
            return Err(KernelError::InvalidInput(format!(
                "kernel {name} already exists"
            )));
        }
        if self.sessions.len() == MAX_KERNEL_SESSIONS {
            return Err(KernelError::InvalidInput(format!(
                "kernel session limit is {MAX_KERNEL_SESSIONS}"
            )));
        }
        let backend = match kind {
            KernelKind::Python => KernelBackend::Json(JsonProcessKernel::spawn(
                &self.root,
                if cfg!(windows) { "python" } else { "python3" },
                &["-u", "-c", PYTHON_WORKER],
            )?),
            KernelKind::JavaScript => KernelBackend::Json(JsonProcessKernel::spawn(
                &self.root,
                "node",
                &["-e", JAVASCRIPT_WORKER],
            )?),
            KernelKind::Shell => KernelBackend::Shell(ShellKernel::spawn(&self.root)?),
            KernelKind::Sql => KernelBackend::Sql(SqlKernel::new()?),
        };
        self.sessions.insert(
            name.to_string(),
            KernelSession {
                snapshot: KernelSnapshot {
                    name: name.to_string(),
                    kind,
                    state: KernelState::Ready,
                    executions: 0,
                    actor_id: actor_id.to_string(),
                    workspace_revision: 0,
                    capabilities: capabilities.iter().cloned().collect(),
                    mutation_authority,
                },
                backend,
                capabilities,
            },
        );
        Ok(())
    }

    pub fn execute(
        &mut self,
        name: &str,
        code: &str,
        actor_id: &str,
        workspace_revision: u64,
        timeout: Option<Duration>,
    ) -> KernelResult<KernelExecution> {
        self.execute_with_tools(name, code, actor_id, workspace_revision, timeout, |_| {
            Err(KernelError::Execution(
                "kernel has no governed Glass tool dispatcher".into(),
            ))
        })
    }

    pub fn execute_with_tools(
        &mut self,
        name: &str,
        code: &str,
        initiator_id: &str,
        workspace_revision: u64,
        timeout: Option<Duration>,
        mut dispatch: impl FnMut(&KernelToolCall) -> KernelResult<Value>,
    ) -> KernelResult<KernelExecution> {
        validate_code(code)?;
        validate_actor_id(initiator_id, "kernel initiator")?;
        let timeout = timeout.unwrap_or(DEFAULT_KERNEL_TIMEOUT);
        if timeout.is_zero() || timeout > Duration::from_secs(600) {
            return Err(KernelError::InvalidInput(
                "kernel timeout must be between 1 ms and 600 seconds".into(),
            ));
        }
        let session = self
            .sessions
            .get_mut(name)
            .ok_or_else(|| KernelError::InvalidInput(format!("unknown kernel {name}")))?;
        if session.snapshot.state != KernelState::Ready {
            return Err(KernelError::Execution(format!(
                "kernel {name} is not ready"
            )));
        }
        let started = Instant::now();
        let executor_id = format!("kernel:{name}");
        let capabilities = session.capabilities.clone();
        let mut tool_calls = 0_u64;
        let mut governed_dispatch = |call: &KernelToolCall| {
            validate_name(&call.id, "kernel tool call ID")?;
            tool_calls = tool_calls.saturating_add(1);
            if tool_calls > MAX_KERNEL_TOOL_CALLS as u64 {
                return Err(KernelError::Execution(format!(
                    "kernel execution exceeded {MAX_KERNEL_TOOL_CALLS} Glass tool calls"
                )));
            }
            if call.tool.starts_with("glass.eval.") {
                return Err(KernelError::Execution(
                    "recursive glass.eval tool calls are forbidden".into(),
                ));
            }
            if !capabilities.contains(&call.tool) {
                return Err(KernelError::Execution(format!(
                    "kernel capability {} was not granted",
                    call.tool
                )));
            }
            dispatch(call)
        };
        let result = match &mut session.backend {
            KernelBackend::Json(kernel) => kernel.execute(code, timeout, &mut governed_dispatch),
            KernelBackend::Shell(kernel) => kernel.execute(code, timeout, &mut governed_dispatch),
            KernelBackend::Sql(kernel) => kernel.execute(code, &mut governed_dispatch),
        };
        let (output, value, truncated) = match result {
            Ok(result) => result,
            Err(error) => {
                if matches!(error, KernelError::Protocol(_) | KernelError::Timeout(_)) {
                    session.snapshot.state = KernelState::Failed;
                }
                return Err(error);
            }
        };
        session.snapshot.executions = session
            .snapshot
            .executions
            .checked_add(1)
            .ok_or_else(|| KernelError::Protocol("kernel sequence overflowed".into()))?;
        session.snapshot.actor_id = executor_id.clone();
        session.snapshot.workspace_revision = workspace_revision;
        Ok(KernelExecution {
            name: name.to_string(),
            kind: session.snapshot.kind,
            sequence: session.snapshot.executions,
            actor_id: executor_id.clone(),
            initiator_id: initiator_id.to_string(),
            executor_id,
            workspace_revision,
            duration_ms: started.elapsed().as_millis() as u64,
            output,
            value,
            truncated,
            tool_calls,
        })
    }

    pub fn snapshots(&self) -> impl Iterator<Item = &KernelSnapshot> {
        self.sessions.values().map(|session| &session.snapshot)
    }

    pub fn snapshot(&self, name: &str) -> Option<&KernelSnapshot> {
        self.sessions.get(name).map(|session| &session.snapshot)
    }

    pub fn stop(&mut self, name: &str) -> KernelResult<KernelSnapshot> {
        let mut session = self
            .sessions
            .remove(name)
            .ok_or_else(|| KernelError::InvalidInput(format!("unknown kernel {name}")))?;
        session.stop();
        Ok(session.snapshot.clone())
    }

    pub fn cancel(&mut self, name: &str) -> KernelResult<KernelSnapshot> {
        let session = self
            .sessions
            .get_mut(name)
            .ok_or_else(|| KernelError::InvalidInput(format!("unknown kernel {name}")))?;
        session.stop();
        session.snapshot.state = KernelState::Failed;
        Ok(session.snapshot.clone())
    }

    pub fn reset(&mut self, name: &str, actor_id: &str) -> KernelResult<()> {
        let session = self
            .sessions
            .get(name)
            .ok_or_else(|| KernelError::InvalidInput(format!("unknown kernel {name}")))?;
        let kind = session.snapshot.kind;
        let capabilities = session.snapshot.capabilities.clone();
        let mutation_authority = session.snapshot.mutation_authority;
        self.stop(name)?;
        self.start_governed(name, kind, actor_id, &capabilities, mutation_authority)
    }
}

impl KernelSession {
    fn stop(&mut self) {
        match &mut self.backend {
            KernelBackend::Json(kernel) => kernel.stop(),
            KernelBackend::Shell(kernel) => kernel.stop(),
            KernelBackend::Sql(_) => {}
        }
        self.snapshot.state = KernelState::Stopped;
    }
}

impl Drop for KernelSession {
    fn drop(&mut self) {
        self.stop();
    }
}

struct JsonProcessKernel {
    child: Child,
    stdin: ChildStdin,
    responses: Receiver<KernelResult<Value>>,
    reader: Option<JoinHandle<()>>,
    stderr_drain: Option<JoinHandle<()>>,
}

impl JsonProcessKernel {
    fn spawn(root: &Path, program: &str, arguments: &[&str]) -> KernelResult<Self> {
        let mut child = Command::new(program)
            .args(arguments)
            .current_dir(root)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|error| {
                if error.kind() == std::io::ErrorKind::NotFound {
                    KernelError::Unavailable(format!("{program} was not found"))
                } else {
                    KernelError::Io(error)
                }
            })?;
        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| KernelError::Protocol("kernel stdin was unavailable".into()))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| KernelError::Protocol("kernel stdout was unavailable".into()))?;
        let stderr = child
            .stderr
            .take()
            .ok_or_else(|| KernelError::Protocol("kernel stderr was unavailable".into()))?;
        let (sender, responses) = mpsc::sync_channel(MAX_KERNEL_MESSAGES);
        let reader = std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines() {
                let response = line.map_err(KernelError::Io).and_then(|line| {
                    if line.len() > MAX_KERNEL_OUTPUT_BYTES {
                        return Err(KernelError::Protocol(
                            "kernel response exceeded the output limit".into(),
                        ));
                    }
                    serde_json::from_str(&line).map_err(|error| {
                        KernelError::Protocol(format!("invalid kernel response: {error}"))
                    })
                });
                match sender.try_send(response) {
                    Ok(()) => {}
                    Err(TrySendError::Full(_) | TrySendError::Disconnected(_)) => break,
                }
            }
        });
        let stderr_drain = std::thread::spawn(move || {
            let mut stderr = stderr;
            let _ = std::io::copy(&mut stderr, &mut std::io::sink());
        });
        Ok(Self {
            child,
            stdin,
            responses,
            reader: Some(reader),
            stderr_drain: Some(stderr_drain),
        })
    }

    fn execute(
        &mut self,
        code: &str,
        timeout: Duration,
        dispatch: &mut impl FnMut(&KernelToolCall) -> KernelResult<Value>,
    ) -> KernelResult<KernelOutput> {
        serde_json::to_writer(
            &mut self.stdin,
            &json!({"code": code, "timeoutMs": timeout.as_millis()}),
        )
        .map_err(|error| {
            KernelError::Protocol(format!("could not encode kernel input: {error}"))
        })?;
        self.stdin.write_all(b"\n")?;
        self.stdin.flush()?;
        let deadline = Instant::now() + timeout;
        let response = loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            let response = match self.responses.recv_timeout(remaining) {
                Ok(response) => response?,
                Err(RecvTimeoutError::Timeout) => {
                    self.stop();
                    return Err(KernelError::Timeout(format!(
                        "execution exceeded {} ms",
                        timeout.as_millis()
                    )));
                }
                Err(RecvTimeoutError::Disconnected) => {
                    return Err(KernelError::Protocol(
                        "kernel response stream closed".into(),
                    ));
                }
            };
            if response.get("kind").and_then(Value::as_str) == Some("toolCall") {
                let call: KernelToolCall = serde_json::from_value(response).map_err(|error| {
                    KernelError::Protocol(format!("invalid kernel tool call: {error}"))
                })?;
                let dispatched = dispatch(&call);
                let response = match dispatched {
                    Ok(result) => {
                        json!({"kind":"toolResult","id":call.id,"ok":true,"result":result})
                    }
                    Err(error) => {
                        json!({"kind":"toolResult","id":call.id,"ok":false,"error":error.to_string()})
                    }
                };
                serde_json::to_writer(&mut self.stdin, &response).map_err(|error| {
                    KernelError::Protocol(format!("could not encode kernel tool result: {error}"))
                })?;
                self.stdin.write_all(b"\n")?;
                self.stdin.flush()?;
                continue;
            }
            if response.get("kind").and_then(Value::as_str) != Some("executionResult") {
                return Err(KernelError::Protocol(
                    "kernel response has an unknown message kind".into(),
                ));
            }
            break response;
        };
        if response.get("ok").and_then(Value::as_bool) != Some(true) {
            return Err(KernelError::Execution(
                response
                    .get("error")
                    .and_then(Value::as_str)
                    .unwrap_or("kernel returned an unsuccessful response")
                    .to_string(),
            ));
        }
        bounded_output(
            response
                .get("output")
                .and_then(Value::as_str)
                .unwrap_or_default(),
            response.get("value").cloned().unwrap_or(Value::Null),
        )
    }

    fn stop(&mut self) {
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

struct ShellKernel {
    child: Child,
    stdin: ChildStdin,
    lines: Receiver<KernelResult<String>>,
    reader: Option<JoinHandle<()>>,
    sequence: u64,
}

impl ShellKernel {
    #[cfg(unix)]
    fn spawn(root: &Path) -> KernelResult<Self> {
        let mut child = Command::new("/bin/sh")
            .current_dir(root)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()?;
        let mut stdin = child
            .stdin
            .take()
            .ok_or_else(|| KernelError::Protocol("shell stdin was unavailable".into()))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| KernelError::Protocol("shell stdout was unavailable".into()))?;
        writeln!(
            stdin,
            "glass_call() {{ glass_tool_arguments=\"$2\"; [ -n \"$glass_tool_arguments\" ] || glass_tool_arguments='{{}}'; printf '__GLASS_TOOL_CALL__:%s\\t%s\\n' \"$1\" \"$glass_tool_arguments\"; IFS= read -r glass_tool_response; printf '%s\\n' \"$glass_tool_response\"; }}"
        )?;
        stdin.flush()?;
        let (sender, lines) = mpsc::sync_channel(MAX_KERNEL_MESSAGES);
        let reader = std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines() {
                let line = line.map_err(KernelError::Io);
                match sender.try_send(line) {
                    Ok(()) => {}
                    Err(TrySendError::Full(_) | TrySendError::Disconnected(_)) => break,
                }
            }
        });
        Ok(Self {
            child,
            stdin,
            lines,
            reader: Some(reader),
            sequence: 0,
        })
    }

    #[cfg(windows)]
    fn spawn(_root: &Path) -> KernelResult<Self> {
        Err(KernelError::Unavailable(
            "persistent shell kernels are not yet available on Windows".into(),
        ))
    }

    #[cfg(unix)]
    fn execute(
        &mut self,
        code: &str,
        timeout: Duration,
        dispatch: &mut impl FnMut(&KernelToolCall) -> KernelResult<Value>,
    ) -> KernelResult<KernelOutput> {
        self.sequence = self
            .sequence
            .checked_add(1)
            .ok_or_else(|| KernelError::Protocol("shell sequence overflowed".into()))?;
        let marker = format!("__GLASS_SHELL_{}__", self.sequence);
        writeln!(
            self.stdin,
            "{{\n{code}\n}} 2>&1\nglass_status=$?\nprintf '{marker}:%s\\n' \"$glass_status\""
        )?;
        self.stdin.flush()?;
        let deadline = Instant::now() + timeout;
        let mut output = String::new();
        let mut truncated = false;
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                self.stop();
                return Err(KernelError::Timeout(format!(
                    "execution exceeded {} ms",
                    timeout.as_millis()
                )));
            }
            let line = match self.lines.recv_timeout(remaining) {
                Ok(line) => line?,
                Err(RecvTimeoutError::Timeout) => {
                    self.stop();
                    return Err(KernelError::Timeout(format!(
                        "execution exceeded {} ms",
                        timeout.as_millis()
                    )));
                }
                Err(RecvTimeoutError::Disconnected) => {
                    return Err(KernelError::Protocol("shell output stream closed".into()));
                }
            };
            let marker_prefix = format!("{marker}:");
            if let Some(marker_index) = line.find(&marker_prefix) {
                append_bounded(&mut output, &line[..marker_index], &mut truncated);
                let status = &line[marker_index + marker_prefix.len()..];
                let status = status.parse::<i32>().map_err(|_| {
                    KernelError::Protocol("shell returned an invalid exit status".into())
                })?;
                if status != 0 {
                    return Err(KernelError::Execution(format!(
                        "shell command exited with status {status}: {output}"
                    )));
                }
                return Ok((output, json!({"exitCode": status}), truncated));
            }
            if let Some(request) = line.strip_prefix("__GLASS_TOOL_CALL__:") {
                let (tool, arguments) = request.split_once('\t').ok_or_else(|| {
                    KernelError::Protocol("shell Glass tool call has no arguments".into())
                })?;
                let call = KernelToolCall {
                    id: format!("shell-{}", self.sequence),
                    tool: tool.to_string(),
                    arguments: serde_json::from_str(arguments).map_err(|error| {
                        KernelError::Protocol(format!(
                            "shell Glass tool arguments are invalid JSON: {error}"
                        ))
                    })?,
                };
                let result = dispatch(&call)?;
                serde_json::to_writer(&mut self.stdin, &result).map_err(|error| {
                    KernelError::Protocol(format!("could not encode shell tool result: {error}"))
                })?;
                self.stdin.write_all(b"\n")?;
                self.stdin.flush()?;
                continue;
            }
            append_bounded(&mut output, &line, &mut truncated);
            append_bounded(&mut output, "\n", &mut truncated);
        }
    }

    #[cfg(windows)]
    fn execute(
        &mut self,
        _code: &str,
        _timeout: Duration,
        _dispatch: &mut impl FnMut(&KernelToolCall) -> KernelResult<Value>,
    ) -> KernelResult<KernelOutput> {
        Err(KernelError::Unavailable(
            "persistent shell kernels are not yet available on Windows".into(),
        ))
    }

    fn stop(&mut self) {
        if self.child.try_wait().ok().flatten().is_none() {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
        if let Some(reader) = self.reader.take() {
            let _ = reader.join();
        }
    }
}

struct SqlKernel {
    connection: rusqlite::Connection,
}

impl SqlKernel {
    fn new() -> KernelResult<Self> {
        Ok(Self {
            connection: rusqlite::Connection::open_in_memory()?,
        })
    }

    fn execute(
        &mut self,
        code: &str,
        dispatch: &mut impl FnMut(&KernelToolCall) -> KernelResult<Value>,
    ) -> KernelResult<KernelOutput> {
        if let Some(request) = code.trim().strip_prefix("GLASS CALL ") {
            let call: KernelToolCall = serde_json::from_str(request).map_err(|error| {
                KernelError::InvalidInput(format!("invalid SQL Glass tool call: {error}"))
            })?;
            return bounded_output("", dispatch(&call)?);
        }
        let leading = code.trim_start().to_ascii_uppercase();
        if !["SELECT", "WITH", "PRAGMA", "EXPLAIN"]
            .iter()
            .any(|keyword| leading.starts_with(keyword))
        {
            self.connection.execute_batch(code)?;
            return Ok((String::new(), Value::Null, false));
        }
        let mut statement = self.connection.prepare(code)?;
        if statement.column_count() == 0 {
            drop(statement);
            self.connection.execute_batch(code)?;
            return Ok((String::new(), Value::Null, false));
        }
        let names = statement
            .column_names()
            .into_iter()
            .map(str::to_string)
            .collect::<Vec<_>>();
        let column_count = names.len();
        let rows = statement.query_map([], |row| {
            let mut record = serde_json::Map::new();
            for (index, name) in names.iter().enumerate().take(column_count) {
                record.insert(name.clone(), sql_value(row.get_ref(index)?));
            }
            Ok(Value::Object(record))
        })?;
        let mut values = Vec::new();
        for row in rows.take(100_000) {
            values.push(row?);
            if serde_json::to_vec(&values)
                .map_err(|error| KernelError::Protocol(error.to_string()))?
                .len()
                > MAX_KERNEL_OUTPUT_BYTES
            {
                values.pop();
                return Ok((String::new(), Value::Array(values), true));
            }
        }
        Ok((String::new(), Value::Array(values), false))
    }
}

type KernelOutput = (String, Value, bool);

fn sql_value(value: ValueRef<'_>) -> Value {
    match value {
        ValueRef::Null => Value::Null,
        ValueRef::Integer(value) => json!(value),
        ValueRef::Real(value) => json!(value),
        ValueRef::Text(value) => String::from_utf8_lossy(value).into_owned().into(),
        ValueRef::Blob(value) => json!({"blobBytes": value.len()}),
    }
}

fn bounded_output(output: &str, value: Value) -> KernelResult<KernelOutput> {
    let mut retained = String::new();
    let mut truncated = false;
    append_bounded(&mut retained, output, &mut truncated);
    let encoded_value = serde_json::to_vec(&value).map_err(|error| {
        KernelError::Protocol(format!("could not encode kernel value: {error}"))
    })?;
    if encoded_value.len() > MAX_KERNEL_OUTPUT_BYTES {
        return Ok((retained, Value::Null, true));
    }
    Ok((retained, value, truncated))
}

fn append_bounded(output: &mut String, value: &str, truncated: &mut bool) {
    let remaining = MAX_KERNEL_OUTPUT_BYTES.saturating_sub(output.len());
    if value.len() <= remaining {
        output.push_str(value);
    } else {
        let boundary = value
            .char_indices()
            .take_while(|(index, _)| *index <= remaining)
            .map(|(index, _)| index)
            .last()
            .unwrap_or(0);
        output.push_str(&value[..boundary]);
        *truncated = true;
    }
}

fn validate_code(code: &str) -> KernelResult<()> {
    if code.is_empty() || code.len() > MAX_KERNEL_CODE_BYTES {
        return Err(KernelError::InvalidInput(format!(
            "kernel code must contain 1..={MAX_KERNEL_CODE_BYTES} bytes"
        )));
    }
    Ok(())
}

fn validate_capabilities(capabilities: &[String]) -> KernelResult<BTreeSet<String>> {
    if capabilities.len() > 64 {
        return Err(KernelError::InvalidInput(
            "kernel capability limit is 64".into(),
        ));
    }
    capabilities
        .iter()
        .map(|capability| {
            if capability.is_empty()
                || capability.len() > 128
                || capability.starts_with("glass.eval.")
                || !capability
                    .chars()
                    .all(|character| character.is_ascii_alphanumeric() || ".-_".contains(character))
            {
                return Err(KernelError::InvalidInput(format!(
                    "invalid or recursive kernel capability {capability}"
                )));
            }
            Ok(capability.clone())
        })
        .collect()
}

fn validate_name(name: &str, description: &str) -> KernelResult<()> {
    if name.is_empty()
        || name.len() > 128
        || !name
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || "-_.".contains(character))
    {
        return Err(KernelError::InvalidInput(format!(
            "{description} must be 1..=128 ASCII letters, digits, '-', '_' or '.'"
        )));
    }
    Ok(())
}

fn validate_actor_id(name: &str, description: &str) -> KernelResult<()> {
    if name.is_empty()
        || name.len() > 128
        || !name
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || "-_.:".contains(character))
    {
        return Err(KernelError::InvalidInput(format!(
            "{description} must be 1..=128 ASCII letters, digits, '-', '_', '.' or ':'"
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT_ROOT: AtomicU64 = AtomicU64::new(0);

    fn root() -> PathBuf {
        let sequence = NEXT_ROOT.fetch_add(1, Ordering::Relaxed);
        let root = std::env::temp_dir().join(format!(
            "glass-kernel-manager-{}-{sequence}",
            std::process::id()
        ));
        std::fs::create_dir_all(&root).unwrap();
        root
    }

    #[test]
    fn python_kernel_preserves_state_and_attribution() {
        let root = root();
        let mut kernels = KernelManager::new(&root).unwrap();
        kernels
            .start("python", KernelKind::Python, "human")
            .unwrap();
        kernels
            .execute("python", "answer = 40", "human", 1, None)
            .unwrap();
        let result = kernels
            .execute("python", "answer + 2", "agent", 2, None)
            .unwrap();
        assert_eq!(result.value, "42");
        assert_eq!(result.actor_id, "kernel:python");
        assert_eq!(result.initiator_id, "agent");
        assert_eq!(result.executor_id, "kernel:python");
        assert_eq!(result.workspace_revision, 2);
        assert_eq!(kernels.snapshot("python").unwrap().executions, 2);
        kernels.stop("python").unwrap();
        std::fs::remove_dir_all(root).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn shell_kernel_preserves_environment_and_directory_state() {
        let root = root();
        let mut kernels = KernelManager::new(&root).unwrap();
        kernels.start("shell", KernelKind::Shell, "human").unwrap();
        kernels
            .execute("shell", "value=glass", "human", 1, None)
            .unwrap();
        let result = kernels
            .execute("shell", "printf '%s' \"$value\"", "human", 1, None)
            .unwrap();
        assert_eq!(result.output, "glass");
        kernels.stop("shell").unwrap();
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn sql_kernel_preserves_tables_and_returns_structured_rows() {
        let root = root();
        let mut kernels = KernelManager::new(&root).unwrap();
        kernels.start("sql", KernelKind::Sql, "agent").unwrap();
        kernels
            .execute(
                "sql",
                "CREATE TABLE inventory (sku TEXT, count INTEGER); INSERT INTO inventory VALUES ('x', 0);",
                "agent",
                1,
                None,
            )
            .unwrap();
        let result = kernels
            .execute("sql", "SELECT sku, count FROM inventory", "agent", 1, None)
            .unwrap();
        assert_eq!(result.value[0]["sku"], "x");
        assert_eq!(result.value[0]["count"], 0);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn kernel_names_code_and_timeouts_fail_closed() {
        assert!(validate_name("python-1", "kernel").is_ok());
        assert!(validate_name("../escape", "kernel").is_err());
        assert!(validate_code("").is_err());
        assert!(validate_code(&"x".repeat(MAX_KERNEL_CODE_BYTES + 1)).is_err());
    }

    #[test]
    fn python_and_javascript_bindings_dispatch_granted_capabilities() {
        let root = root();
        let capability = vec!["glass.test.results".to_string()];
        for (name, kind, code) in [
            (
                "python-binding",
                KernelKind::Python,
                "glass.call('glass.test.results', {'limit': 1})",
            ),
            (
                "javascript-binding",
                KernelKind::JavaScript,
                "glass.call('glass.test.results', {limit: 1}).then(value => value.count)",
            ),
        ] {
            let mut kernels = KernelManager::new(&root).unwrap();
            if let Err(error) =
                kernels.start_governed(name, kind, &format!("kernel:{name}"), &capability, false)
            {
                if matches!(error, KernelError::Unavailable(_)) {
                    continue;
                }
                panic!("could not start {name}: {error}");
            }
            let mut observed = Vec::new();
            let execution = kernels
                .execute_with_tools(name, code, "agent-0003", 7, None, |call| {
                    observed.push(call.clone());
                    Ok(json!({"count": 3}))
                })
                .unwrap();
            assert_eq!(execution.initiator_id, "agent-0003");
            assert_eq!(execution.executor_id, format!("kernel:{name}"));
            assert_eq!(execution.tool_calls, 1);
            assert_eq!(observed[0].tool, "glass.test.results");
            assert_eq!(observed[0].arguments["limit"], 1);
        }
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn sql_and_shell_bindings_use_the_same_bounded_dispatch_contract() {
        let root = root();
        let capability = vec!["glass.graph.path".to_string()];
        let mut kinds = vec![(
            "sql-binding",
            KernelKind::Sql,
            r#"GLASS CALL {"id":"sql-1","tool":"glass.graph.path","arguments":{"from":"a","to":"b"}}"#,
        )];
        if cfg!(unix) {
            kinds.push((
                "shell-binding",
                KernelKind::Shell,
                r#"glass_call glass.graph.path '{"from":"a","to":"b"}'"#,
            ));
        }
        for (name, kind, code) in kinds {
            let mut kernels = KernelManager::new(&root).unwrap();
            kernels
                .start_governed(name, kind, &format!("kernel:{name}"), &capability, false)
                .unwrap();
            let result = kernels
                .execute_with_tools(name, code, "human:local", 9, None, |call| {
                    assert_eq!(call.tool, "glass.graph.path");
                    Ok(json!({"path":["a","b"]}))
                })
                .unwrap();
            assert_eq!(result.tool_calls, 1);
        }
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn ungranted_and_recursive_kernel_capabilities_fail_closed() {
        assert!(validate_capabilities(&["glass.eval.execute".into()]).is_err());
        let root = root();
        let mut kernels = KernelManager::new(&root).unwrap();
        kernels
            .start_governed(
                "python-denied",
                KernelKind::Python,
                "kernel:python-denied",
                &["glass.test.results".into()],
                false,
            )
            .unwrap();
        let error = kernels
            .execute_with_tools(
                "python-denied",
                "glass.call('glass.file.write', {'path':'x'})",
                "agent-0003",
                1,
                None,
                |_| panic!("ungranted call reached the dispatcher"),
            )
            .unwrap_err();
        assert!(error.to_string().contains("not granted"));
        std::fs::remove_dir_all(root).unwrap();
    }
}
