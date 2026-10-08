//! Spawning and talking to external extensions.
//!
//! One child process per extension, newline-delimited JSON in both directions.
//! Every exchange is bounded by a timeout and a byte cap, so a wedged or
//! hostile extension degrades into a warning rather than a hung session.

use std::io::{BufReader, Read, Write};
use std::os::unix::process::CommandExt;
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

use super::protocol::parse_manifest;
use super::protocol::{
    self, MAX_REPLY_BYTES, Manifest, PROTOCOL_VERSION, REQUEST_TIMEOUT_MS, Request, SubagentTask,
    is_nested_call, is_subagent_call, is_subagents_call, parse_subagent_tasks,
};

/// Callback the host uses to run a tool an extension asked for.
pub type HostToolBridge<'a> = dyn FnMut(&str, &str) -> String + 'a;

/// Callback the host uses to run delegated tasks in fresh contexts.
///
/// The subagent loop needs the provider and the workspace, which belong
/// to the REPL. Rather than hand the extension a borrow of them — the
/// extension outlives the turn that triggered the delegation — the REPL
/// installs a factory that builds what it needs on demand. The factory
/// owns only an `Arc` provider factory and a `PathBuf`, so it is
/// `'static` and the extension needs no lifetime parameter.
///
/// The callback takes a batch: one extension is one serial pipe, so
/// fan-out cannot come from overlapping tool calls. A batch arrives as a
/// single request, the host runs the tasks concurrently, and the reports
/// come back in request order.
pub type SubagentFn = Box<dyn FnMut(&[SubagentTask]) -> Vec<String> + Send + Sync>;

/// Legacy whitespace commands or JSON argument vectors; never evaluated by a shell.
pub fn arguments(spec: &str) -> Result<Vec<String>, String> {
    let mut args: Vec<String> = if spec.trim_start().starts_with('[') {
        serde_json::from_str(spec)
            .map_err(|_| "Extension argument vector must contain only strings")?
    } else {
        spec.split_whitespace().map(str::to_string).collect()
    };
    if args.is_empty() || args[0].is_empty() {
        return Err("Extension executable is empty".into());
    }
    for arg in &mut args {
        if let Some(rest) = arg.strip_prefix("~/")
            && let Ok(home) = std::env::var("HOME")
        {
            *arg = format!("{home}/{rest}");
        }
    }
    Ok(args)
}

/// A running extension process.
pub struct Extension {
    pub manifest: Manifest,
    child: Child,
    stopped: bool,
    cancel: std::sync::Arc<std::sync::atomic::AtomicBool>,
    stdin: Option<ChildStdin>,
    stdout: Option<BufReader<ChildStdout>>,
    /// Raw stdout descriptor, used only for poll(2) readiness checks. `BufReader`
    /// buffers bytes internally, so polling the fd is still correct: readable
    /// means either the fd or the buffer has something.
    raw_fd: i32,
    next_id: u64,
    /// Runs a delegated task. Set by the REPL so the host, not the
    /// extension process, owns the nested agent loop.
    subagent: Option<SubagentFn>,
}

impl Extension {
    /// Spawn `cmd` and complete the handshake.
    ///
    /// `cmd` may include arguments (`python3 ext.py`, `node dist/index.js`), so
    /// it is split on whitespace rather than treated as a single path. That is
    /// deliberate: an extension's interpreter is part of how it is invoked, and
    /// requiring every extension to be a self-contained executable would lock
    /// out the scripting most extensions actually want.
    pub fn spawn(cmd: &str, workspace: &str, model: &str) -> Result<Self, String> {
        Self::spawn_cancellable(
            cmd,
            workspace,
            model,
            std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
        )
    }
    pub fn spawn_cancellable(
        cmd: &str,
        workspace: &str,
        model: &str,
        cancel: std::sync::Arc<std::sync::atomic::AtomicBool>,
    ) -> Result<Self, String> {
        let parts = arguments(cmd)?;
        let program = &parts[0];
        let args = &parts[1..];

        let mut command = Command::new(program);
        // Extensions are local programs, but need not inherit provider secrets.
        for (name, _) in std::env::vars_os() {
            let upper = name.to_string_lossy().to_ascii_uppercase();
            if upper.contains("TOKEN")
                || upper.contains("SECRET")
                || upper.contains("PASSWORD")
                || upper.contains("API_KEY")
                || upper == "AWS_ACCESS_KEY_ID"
                || upper == "SSH_AUTH_SOCK"
            {
                command.env_remove(name);
            }
        }
        let mut child = command
            .process_group(0)
            .args(args)
            .current_dir(workspace)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            // Extension stderr goes to our stderr so a broken extension can
            // still say why, without us trying to parse it.
            .stderr(Stdio::inherit())
            .spawn()
            .map_err(|e| format!("cannot start extension `{cmd}`: {e}"))?;

        let raw_fd = child.stdout.as_ref().map(fd_of).unwrap_or(-1);
        let mut ext = Self {
            manifest: Manifest::default(),
            stdin: child.stdin.take(),
            stdout: child.stdout.take().map(BufReader::new),
            raw_fd,
            next_id: 0,
            subagent: None,
            child,
            stopped: false,
            cancel,
        };

        let hello = Request::Hello {
            protocol: PROTOCOL_VERSION,
            workspace: workspace.to_string(),
            model: model.to_string(),
        };
        let reply = ext
            .request(&hello, REQUEST_TIMEOUT_MS * 3, None)
            .ok_or_else(|| format!("extension `{cmd}` closed before the handshake"))?;
        ext.manifest = parse_manifest(&reply).map_err(|e| format!("extension `{cmd}`: {e}"))?;
        Ok(ext)
    }

    pub fn finish_startup(&mut self) {
        self.cancel = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    }

    /// Install the callback that runs delegated tasks.
    ///
    /// The REPL owns the provider and the workspace, so it supplies
    /// the runner; the extension process only asks for a task to be
    /// delegated. Called once after spawn, before any request.
    pub fn set_subagent(&mut self, runner: SubagentFn) {
        self.subagent = Some(runner);
    }

    /// Send a request and read one reply.
    ///
    /// Returns `None` if the extension closed the pipe, exceeded the byte cap,
    /// or did not answer in time. Every failure is recoverable: the caller
    /// falls back to the default behaviour rather than aborting the turn.
    fn request(
        &mut self,
        req: &Request,
        timeout_ms: u64,
        mut host_tools: Option<&mut HostToolBridge<'_>>,
    ) -> Option<serde_json::Value> {
        self.next_id += 1;
        let id = self.next_id;
        let mut line = serde_json::to_string(&req.to_json(id)).ok()?;
        line.push('\n');

        let stdin = self.stdin.as_mut()?;
        stdin.write_all(line.as_bytes()).ok()?;
        stdin.flush().ok()?;

        // While a tool runs, the extension may ask the host to run a tool on
        // its behalf. Those requests are interleaved with the reply, so the
        // read loop has to demultiplex: anything tagged `call_tool` is served
        // and its result written back, and only a non-nested message is the
        // answer we were waiting for.
        //
        // The deadline bounds one *waiting* stretch, not the whole request.
        // Servicing nested work resets it: a delegated task or a nested tool
        // chain can legitimately run for minutes, and expiring the wait while
        // that work was making progress would turn a long subagent into
        // "unavailable".
        let mut deadline = std::time::Instant::now() + std::time::Duration::from_millis(timeout_ms);
        loop {
            let remaining = deadline.saturating_duration_since(std::time::Instant::now());
            if remaining.is_zero() {
                return None;
            }
            let v = self.read_reply(remaining.as_millis().max(1) as u64)?;
            if is_subagent_call(&v) || is_subagents_call(&v) {
                // Delegated work. The host owns the nested agent
                // loops; the extension process only asked for them.
                // An extension with no runner installed is refused,
                // not hung on, because a silent gap is worse than an
                // error.
                let (tasks, batched) = if is_subagents_call(&v) {
                    match parse_subagent_tasks(&v) {
                        Ok(tasks) => (tasks, true),
                        Err(e) => {
                            let call_id = v.get("call_id").cloned().unwrap_or(serde_json::Value::Null);
                            self.reply(protocol::subagents_result_json(&call_id, &[], Some(&e)))?;
                            deadline = std::time::Instant::now()
                                + std::time::Duration::from_millis(timeout_ms);
                            continue;
                        }
                    }
                } else {
                    let task = v
                        .get("task")
                        .and_then(|t| t.as_str())
                        .unwrap_or("")
                        .to_string();
                    (
                        vec![SubagentTask {
                            task,
                            ..SubagentTask::default()
                        }],
                        false,
                    )
                };
                let runner = self.subagent.as_mut()?;
                let outputs = runner(&tasks);
                let call_id = v.get("call_id").cloned().unwrap_or(serde_json::Value::Null);
                if batched {
                    self.reply(protocol::subagents_result_json(&call_id, &outputs, None))?;
                } else {
                    let out = outputs.first().cloned().unwrap_or_default();
                    self.reply(protocol::subagent_result_json(&call_id, &out))?;
                }
                deadline =
                    std::time::Instant::now() + std::time::Duration::from_millis(timeout_ms);
                continue;
            }
            if is_nested_call(&v) {
                // A refusal is an answer: when the caller did not grant a
                // bridge, reply with an error body instead of dropping the
                // request — silence would just stall the extension until its
                // deadline expires, with no way to tell why.
                let name = v
                    .get("name")
                    .and_then(|n| n.as_str())
                    .unwrap_or("")
                    .to_string();
                let args = v
                    .get("arguments")
                    .and_then(|a| a.as_str())
                    .unwrap_or("{}")
                    .to_string();
                let out = match host_tools.as_deref_mut() {
                    Some(bridge) => bridge(&name, &args),
                    None => format!(
                        "error: nested tool call `{name}` refused: this hook has no host-tool bridge"
                    ),
                };

                let call_id = v.get("call_id").cloned().unwrap_or(serde_json::Value::Null);
                self.reply(protocol::nested_call_result_json(&call_id, &out))?;
                deadline =
                    std::time::Instant::now() + std::time::Duration::from_millis(timeout_ms);
                continue;
            }
            // A reply to an earlier fire-and-forget event. It is not the answer
            // we asked for, so hold it and keep reading rather than returning
            // it as if it were.
            if let Some(want) = v.get("id").and_then(serde_json::Value::as_u64)
                && want != id
            {
                continue;
            }
            return Some(v);
        }
    }

    /// Write one reply line, assigning it the next id.
    ///
    /// Returns `None` when the pipe is gone; callers are already on a
    /// best-effort path, and a closed pipe must not panic or hang.
    fn reply(&mut self, mut v: serde_json::Value) -> Option<()> {
        self.next_id += 1;
        v["id"] = serde_json::json!(self.next_id);
        let mut line = serde_json::to_string(&v).ok()?;
        line.push('\n');
        let stdin = self.stdin.as_mut()?;
        stdin.write_all(line.as_bytes()).ok()?;
        stdin.flush().ok()?;
        Some(())
    }

    /// Read one line, capped in both size and time.
    fn read_reply(&mut self, timeout_ms: u64) -> Option<serde_json::Value> {
        let stdout = self.stdout.as_mut()?;
        let deadline = std::time::Instant::now() + std::time::Duration::from_millis(timeout_ms);
        let mut bytes = Vec::new();
        loop {
            let remaining = deadline.saturating_duration_since(std::time::Instant::now());
            if remaining.is_zero() || self.cancel.load(std::sync::atomic::Ordering::Acquire) {
                return None;
            }
            if stdout.buffer().is_empty()
                && !fd_readable(
                    self.raw_fd,
                    remaining.min(std::time::Duration::from_millis(50)),
                )
            {
                continue;
            }
            let mut byte = [0u8; 1];
            stdout.read_exact(&mut byte).ok()?;
            if byte[0] == b'\n' {
                break;
            }
            if bytes.len() >= MAX_REPLY_BYTES {
                return None;
            }
            bytes.push(byte[0]);
        }
        serde_json::from_slice(&bytes).ok()
    }

    /// Ask for the extension's system-prompt contribution.
    pub fn system_prompt(&mut self) -> Option<String> {
        self.request(&Request::SystemPrompt, REQUEST_TIMEOUT_MS, None)
            .and_then(|v| v.get("text").and_then(|t| t.as_str()).map(str::to_string))
            .filter(|text| !text.trim().is_empty())
            .or_else(|| {
                (!self.manifest.system_prompt.trim().is_empty())
                    .then(|| self.manifest.system_prompt.clone())
            })
    }

    /// Offer a tool result for rewriting. Returns `Some` only if the extension
    /// actually changed something.
    ///
    /// `host_tools` is the bridge the extension may use while it rewrites —
    /// the auto-verify extension runs the project's verifier from inside this
    /// hook, so the bridge is not optional plumbing here.
    pub fn rewrite_tool_result(
        &mut self,
        call_id: &str,
        name: &str,
        body: &str,
        host_tools: &mut HostToolBridge<'_>,
    ) -> Option<String> {
        let req = Request::ToolResult {
            call_id: call_id.to_string(),
            name: name.to_string(),
            body: body.to_string(),
        };
        let v = self.request(&req, REQUEST_TIMEOUT_MS, Some(host_tools))?;
        let new = v.get("body")?.as_str()?;
        (!new.is_empty() && new != body).then(|| new.to_string())
    }

    /// Ask whether a tool call may run.
    pub fn authorize(&mut self, call_id: &str, name: &str, arguments: &str) -> Result<(), String> {
        let req = Request::Authorize {
            call_id: call_id.to_string(),
            name: name.to_string(),
            arguments: arguments.to_string(),
        };
        match self.request(&req, REQUEST_TIMEOUT_MS, None) {
            // A configured authorization hook must explicitly allow execution.
            None if !self.manifest.authorization_required => Ok(()),
            None => Err(format!(
                "{}: authorization unavailable; tool blocked",
                self.manifest.name
            )),
            Some(v) => {
                if v.get("allow")
                    .and_then(|a| a.as_bool())
                    .unwrap_or(!self.manifest.authorization_required)
                {
                    Ok(())
                } else {
                    Err(v
                        .get("reason")
                        .and_then(|r| r.as_str())
                        .unwrap_or("blocked by extension")
                        .to_string())
                }
            }
        }
    }

    /// Tell the extension a turn finished. Best-effort.
    ///
    /// `cacheable_prefix_bytes` is the size of the prefix a provider would
    /// cache, which is what lets a warmer decide if a refresh is worth a write.
    /// Fire-and-forget: send the event and do not wait for a reply.
    ///
    /// The protocol permits a reply, but an extension that stays silent must
    /// not be able to stall the loop. Blocking here for the timeout meant every
    /// turn paid a fixed penalty against whichever extension declined to
    /// answer, which is exactly the bug that made the agent appear to stop
    /// after one tool call.
    pub fn turn_end(&mut self, text: &str, cacheable_prefix_bytes: usize) {
        self.send_only(&Request::TurnEnd {
            text: text.to_string(),
            cacheable_prefix_bytes,
        });
    }

    /// Write a request without reading a reply, so a silent extension costs
    /// nothing. Any late reply is drained by the next request that does wait.
    fn send_only(&mut self, req: &Request) {
        self.next_id += 1;
        let id = self.next_id;
        let Ok(mut line) = serde_json::to_string(&req.to_json(id)) else {
            return;
        };
        line.push('\n');
        let Some(stdin) = self.stdin.as_mut() else {
            return;
        };
        let _ = stdin.write_all(line.as_bytes());
        let _ = stdin.flush();
    }

    /// Run a slash command. Returns the extension's output.
    pub fn run_command(&mut self, name: &str, args: &str) -> Option<String> {
        let req = Request::Command {
            name: name.to_string(),
            args: args.to_string(),
        };
        let v = self.request(&req, REQUEST_TIMEOUT_MS, None)?;
        v.get("output").and_then(|o| o.as_str()).map(str::to_string)
    }

    /// Execute one of this extension's tools, servicing any nested tool calls
    /// it makes while it runs.
    ///
    /// Returns `None` when the extension does not offer the tool or does not
    /// answer, so the caller reports it as unavailable rather than inventing an
    /// empty success.
    pub fn run_tool(
        &mut self,
        name: &str,
        arguments: &str,
        host_tools: &mut HostToolBridge,
    ) -> Option<String> {
        if !self.manifest.tools.iter().any(|t| t.name == name) {
            return None;
        }
        let req = Request::RunTool {
            name: name.to_string(),
            arguments: arguments.to_string(),
        };
        self.request(&req, REQUEST_TIMEOUT_MS * 10, Some(host_tools))
            .and_then(|v| v.get("output").and_then(|o| o.as_str()).map(str::to_string))
            .filter(|s| !s.is_empty())
    }

    pub fn has_command(&self, name: &str) -> bool {
        self.manifest.commands.iter().any(|c| c.name == name)
    }

    /// Shut the extension down, escalating to a kill if it ignores the request.
    pub fn shutdown(&mut self) {
        if self.stopped {
            return;
        }
        let _ = self.request(&Request::Goodbye, 200, None);
        // Dropping stdin should let a well-behaved extension exit on EOF.
        self.stdin = None;
        for _ in 0..20 {
            match self.child.try_wait() {
                Ok(Some(_)) => {
                    unsafe {
                        libc::kill(-(self.child.id() as i32), libc::SIGKILL);
                    }
                    self.stopped = true;
                    return;
                }
                Ok(None) => std::thread::sleep(std::time::Duration::from_millis(10)),
                Err(_) => break,
            }
        }
        unsafe {
            libc::kill(-(self.child.id() as i32), libc::SIGKILL);
        }
        let _ = self.child.kill();
        let _ = self.child.wait();
        self.stopped = true;
    }
}

impl Drop for Extension {
    fn drop(&mut self) {
        if self.stopped {
            return;
        }
        // A killed harness must not leave orphaned extension processes holding
        // the workspace open.
        unsafe {
            libc::kill(-(self.child.id() as i32), libc::SIGKILL);
        }
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// Raw descriptor for a child stream, for poll(2).
#[cfg(unix)]
fn fd_of(s: &ChildStdout) -> i32 {
    use std::os::fd::AsRawFd;
    s.as_raw_fd()
}

#[cfg(not(unix))]
fn fd_of(_s: &ChildStdout) -> i32 {
    -1
}

/// Wait until the stream has bytes available, or the deadline passes.
///
/// `BufReader` does not implement `AsFd`, so the raw stdout is kept alongside
/// it purely for this poll.
#[cfg(unix)]
fn fd_readable(fd: i32, timeout: std::time::Duration) -> bool {
    let ms = i32::try_from(timeout.as_millis()).unwrap_or(i32::MAX);
    let mut pfd = PollFd {
        fd,
        events: POLLIN,
        revents: 0,
    };
    let rc = unsafe { poll(&mut pfd as *mut PollFd, 1, ms) };
    rc > 0 && (pfd.revents & (POLLIN | POLLHUP | POLLERR)) != 0
}

#[cfg(unix)]
const POLLIN: i16 = 0x0001;
#[cfg(unix)]
const POLLERR: i16 = 0x0008;
#[cfg(unix)]
const POLLHUP: i16 = 0x0010;

#[cfg(unix)]
#[repr(C)]
struct PollFd {
    fd: i32,
    events: i16,
    revents: i16,
}

unsafe extern "C" {
    fn poll(fds: *mut PollFd, nfds: u64, timeout: i32) -> i32;
}

#[cfg(not(unix))]
fn fd_readable(_fd: i32, timeout: std::time::Duration) -> bool {
    std::thread::sleep(timeout);
    true
}
