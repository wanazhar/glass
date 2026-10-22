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
    self, CallLineage, MAX_REPLY_BYTES, Manifest, PROTOCOL_VERSION, REQUEST_TIMEOUT_MS, Request,
    SubagentTask, envelope_report, input_result_json, is_custom_append, is_custom_read,
    is_input_request, is_nested_call, is_subagent_call, is_subagents_call, parse_subagent_tasks,
};

/// How long a cancelled exchange waits for a reply the extension may already
/// have finished writing, before it stops listening.
///
/// One poll interval, not a grace period: long enough that a completed result
/// sitting in the pipe is not thrown away, short enough that cancelling still
/// feels immediate. Anything slower than this is work genuinely still in
/// flight, and is reported as such.
const CANCEL_DRAIN_MS: u64 = 50;

/// Callback the host uses to run a tool an extension asked for.
pub type HostToolBridge<'a> = dyn FnMut(&str, &str) -> String + 'a;
/// Interactive MCP input callback. The returned JSON is private callback data.
pub type HostInputFn<'a> = dyn FnMut(&str, &serde_json::Value) -> serde_json::Value + 'a;

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

#[cfg(test)]
mod security_tests {
    use super::*;

    #[test]
    fn extension_environment_is_rebuilt_without_explicit_secret_overrides() {
        let mut command = Command::new("/bin/sh");
        command.arg("-c").arg("test -z \"${GLASS_TEST_API_KEY:-}\"");
        command.env("GLASS_TEST_API_KEY", "must-not-reach-extension");
        crate::sandbox::scrub_sensitive_env(&mut command);
        assert!(command.status().expect("shell starts").success());
    }
}

/// A shared cancellation flag an extension call watches while it waits.
///
/// One flag outlives the startup handshake: the same token that cancels a
/// slow handshake is still the token a running tool call watches, so a cancel
/// cannot be swallowed at the seam between the two. Cloning is a refcount
/// bump, which is what lets a caller hold the token while the call runs.
#[derive(Clone, Debug)]
pub struct Cancel {
    flag: std::sync::Arc<std::sync::atomic::AtomicBool>,
}

impl Cancel {
    /// A fresh, un-set token.
    pub fn new() -> Self {
        Self {
            flag: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
        }
    }

    /// Adopt an existing flag, e.g. the one a startup modal owns.
    pub fn from_shared(flag: std::sync::Arc<std::sync::atomic::AtomicBool>) -> Self {
        Self { flag }
    }

    /// Reuse a flag without taking ownership of the decision to set it.
    pub fn is_cancelled(&self) -> bool {
        self.flag.load(std::sync::atomic::Ordering::Acquire)
    }

    /// Ask whatever is waiting on this token to stop waiting.
    ///
    /// Set by the agent loop's cancel path. The extension host itself only
    /// reads the flag, so the setter has no caller here yet.
    pub fn cancel(&self) {
        self.flag.store(true, std::sync::atomic::Ordering::Release);
    }

    /// Share cancellation with owned nested shell processes.
    pub(crate) fn shared_flag(&self) -> std::sync::Arc<std::sync::atomic::AtomicBool> {
        self.flag.clone()
    }

    /// Clear the flag. Only meaningful at a lifecycle boundary, where the
    /// previous holder has already given up the right to set it.
    pub fn reset(&self) {
        self.flag.store(false, std::sync::atomic::Ordering::Release);
    }
}

impl Default for Cancel {
    fn default() -> Self {
        Self::new()
    }
}

/// How a cancelled call actually stopped.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CancelPath {
    /// The host stopped waiting; the extension process is still running and
    /// may finish, or may not. Its answer is not reported, because the host
    /// is no longer listening for it.
    Cooperative,
    /// The extension did not stop, so its process group was killed. Any work
    /// it had in flight died with it.
    Killed,
}

/// Why a call into an extension ended without an answer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CallStop {
    /// No loaded extension offers this tool or command.
    Unknown,
    /// A cancel flag was observed while waiting for the answer.
    Cancelled(CancelPath),
    /// The extension closed the pipe, exceeded the byte cap, or ran out of
    /// time. The extension is not necessarily at fault.
    Failed(String),
}

/// What one extension call produced.
///
/// A completed call is a completed call: a cancelled call can never carry an
/// output, because the host stops listening as soon as it cancels. That is
/// what keeps the transcript honest — `Some("")` and `Cancelled` are
/// distinguishable rather than both being "no result".
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CallOutcome {
    pub output: Option<String>,
    pub stop: CallStop,
}

impl CallOutcome {
    fn completed(output: String) -> Self {
        Self {
            output: Some(output),
            stop: CallStop::Unknown,
        }
    }

    fn stopped(stop: CallStop) -> Self {
        Self { output: None, stop }
    }

    // `is_completed` and `is_cancelled` are how the outcome's state is
    // inspected; the agent loop reports through `describe`. Both are also
    // the assertions the registry tests need, and no live caller needs a
    // boolean query that `describe` already answers in words, so they are
    // gated to tests to keep the public surface exactly what is consumed.
    #[cfg(test)]
    pub fn is_completed(&self) -> bool {
        self.output.is_some()
    }

    /// Whether this call was cancelled rather than completed or failed.
    #[cfg(test)]
    pub fn is_cancelled(&self) -> bool {
        matches!(self.stop, CallStop::Cancelled(_))
    }

    /// The output, for callers that only care whether anything came back.
    pub fn into_output(self) -> Option<String> {
        self.output
    }

    /// One transcript line that says exactly what happened.
    ///
    /// Cancellation is never reported as a failure and a failure is never
    /// reported as a cancellation: the model is told which one it was, and
    /// whether the extension may still be running.
    #[allow(dead_code)]
    pub fn describe(&self, subject: &str) -> String {
        match (&self.output, &self.stop) {
            (Some(output), _) => output.clone(),
            (None, CallStop::Unknown) => {
                format!("error: `{subject}` is not provided by any loaded extension")
            }
            (None, CallStop::Cancelled(CancelPath::Cooperative)) => format!(
                "cancelled: `{subject}` stopped waiting after cancellation; \
                 the extension was left running and its result is not reported"
            ),
            (None, CallStop::Cancelled(CancelPath::Killed)) => format!(
                "cancelled: `{subject}` did not stop, so its extension process group was killed"
            ),
            (None, CallStop::Failed(reason)) => format!("error: `{subject}` failed: {reason}"),
        }
    }
}

/// Everything one extension call needs beyond its arguments: which model call
/// it belongs to, and whether it may still be waiting for an answer.
#[derive(Clone, Debug)]
pub struct CallScope {
    /// The chain of calls this work descends from.
    pub lineage: super::protocol::CallLineage,
    /// The token the call watches while waiting.
    pub cancel: Cancel,
}

impl CallScope {
    /// Work with no originating model call — a slash command, or a hook the
    /// harness runs on its own initiative. Still cancellable, never traced.
    pub fn unscoped() -> Self {
        Self {
            lineage: super::protocol::CallLineage::root(""),
            cancel: Cancel::new(),
        }
    }

    /// Work caused by the model tool call `call_id`, bounded by `cancel`.
    ///
    /// The agent loop builds one of these per model tool call; nothing inside
    /// the extension host has a model tool call to build one from.
    pub fn from_call(call_id: &str, cancel: Cancel) -> Self {
        Self {
            lineage: super::protocol::CallLineage::root(call_id),
            cancel,
        }
    }

    /// Refuse to start work that is already at the nesting bound.
    ///
    /// Checking before dispatch matters as much as checking between links: a
    /// chain that reached the bound by a legitimate route must not be able to
    /// buy one more level by re-entering the registry.
    pub fn check(&self) -> Result<(), String> {
        if self.lineage.depth() >= super::protocol::MAX_NESTED_CALL_DEPTH {
            return Err(format!(
                "refused: `{}` is already {} nested calls deep, the bound is {}",
                self.lineage.origin(),
                self.lineage.depth(),
                super::protocol::MAX_NESTED_CALL_DEPTH
            ));
        }
        Ok(())
    }
}

/// How one extension ended when the registry was shut down.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ShutdownOutcome {
    /// The extension's announced name.
    pub name: String,
    /// The extension answered `goodbye` and exited on its own.
    pub graceful: bool,
    /// Its process group had to be killed, including TERM-ignoring
    /// descendants. This is the existing last resort and must keep firing.
    pub killed: bool,
    /// It had already been stopped by an earlier shutdown. Shutdown is
    /// idempotent: the second call does no work and says so.
    pub already_stopped: bool,
    /// Anything that went wrong while stopping it.
    pub error: Option<String>,
}

impl ShutdownOutcome {
    /// One line naming the extension and the path that was taken.
    pub fn describe(&self) -> String {
        if self.error.is_some() {
            return format!("extension `{}` did not shut down cleanly", self.name);
        }
        if self.already_stopped {
            return format!("extension `{}` was already stopped", self.name);
        }
        if self.killed {
            return format!(
                "extension `{}` ignored the shutdown request; its process group was killed",
                self.name
            );
        }
        format!("extension `{}` stopped on request", self.name)
    }
}

/// Why one attempt to read a reply line did not produce one.
#[derive(Clone, Debug, PartialEq, Eq)]
enum ReadFailure {
    /// A cancel was observed while waiting, and no finished reply was
    /// available to report instead.
    Cancelled,
    /// The pipe closed, the reply was malformed, or the deadline passed.
    Unavailable(String),
}

/// Read one line into `bytes`, bounded by `wait`.
///
/// Split out so the post-cancel drain can reuse the exact same read
/// behaviour as the main wait: a drained reply must be read the same way a
/// waited-for one is, or the drain would be measuring something else.
#[cfg(unix)]
fn read_line(
    stdout: &mut BufReader<ChildStdout>,
    raw_fd: i32,
    wait: std::time::Duration,
    bytes: &mut Vec<u8>,
) -> Result<serde_json::Value, ReadFailure> {
    let deadline = std::time::Instant::now() + wait;
    loop {
        let remaining = deadline.saturating_duration_since(std::time::Instant::now());
        if remaining.is_zero() {
            return Err(ReadFailure::Cancelled);
        }
        if stdout.buffer().is_empty()
            && !fd_readable(raw_fd, remaining.min(std::time::Duration::from_millis(10)))
        {
            continue;
        }
        let mut byte = [0u8; 1];
        if stdout.read_exact(&mut byte).is_err() {
            return Err(ReadFailure::Unavailable(
                "the extension closed its output pipe".into(),
            ));
        }
        if byte[0] == b'\n' {
            break;
        }
        if bytes.len() >= MAX_REPLY_BYTES {
            return Err(ReadFailure::Unavailable(format!(
                "reply exceeded {MAX_REPLY_BYTES} bytes"
            )));
        }
        bytes.push(byte[0]);
    }
    serde_json::from_slice(bytes)
        .map_err(|e| ReadFailure::Unavailable(format!("malformed reply: {e}")))
}

/// One exchange with an extension, and how it ended.
#[derive(Clone, Debug, PartialEq)]
enum RequestOutcome {
    Answered(serde_json::Value),
    /// A cancel flag was observed while waiting. Never carries an answer:
    /// the host stopped listening, so anything the extension says afterwards
    /// is not attributable to this call and is not reported as its result.
    Cancelled(CancelPath),
    /// The pipe closed, the reply was over the byte cap, or the deadline
    /// passed. Recoverable: the caller falls back rather than aborting.
    Unavailable(String),
}

impl RequestOutcome {
    fn into_value(self) -> Option<serde_json::Value> {
        match self {
            Self::Answered(v) => Some(v),
            Self::Cancelled(_) | Self::Unavailable(_) => None,
        }
    }

    fn stop(self) -> CallStop {
        match self {
            Self::Answered(_) => CallStop::Unknown,
            Self::Cancelled(path) => CallStop::Cancelled(path),
            Self::Unavailable(reason) => CallStop::Failed(reason),
        }
    }
}

// Leave framing overhead below the SDK's 4 MiB transport limit.
const MAX_CUSTOM_STATE_BYTES: usize = 3 * 1024 * 1024;

/// A running extension process.
pub struct Extension {
    pub manifest: Manifest,
    child: Child,
    stopped: bool,
    /// The token every exchange on this extension watches, including the
    /// handshake. It is the same token the caller holds, so a cancel issued
    /// during startup still reaches a tool call that starts later.
    cancel: Cancel,
    /// Set once the handshake is over and this extension is live. Used to
    /// answer "can this extension still be cancelled?" without guessing.
    live: bool,
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
    /// Durable entries this extension owns, seeded at startup and
    /// grown by its own appends. `read_custom` is served from here,
    /// so an extension can reconstruct state after a restart
    /// without the entries entering the model context.
    custom: std::collections::BTreeMap<String, Vec<String>>,
    custom_bytes: usize,
    /// Appends queued by the extension, drained by the agent into
    /// the session log after the tool call or turn that queued them.
    pending_custom: Vec<(String, Option<String>)>,
    session_path: Option<std::path::PathBuf>,
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
        Self::spawn_cancellable(cmd, workspace, model, Cancel::new())
    }
    /// Spawn an extension whose handshake can be cancelled.
    ///
    /// The token is not consumed by the handshake: it becomes the extension's
    /// own cancel token, so the caller keeps the ability to stop this
    /// extension for the rest of the session. Dropping it at the end of
    /// startup is what used to leave a running tool call with a flag nothing
    /// could set.
    pub fn spawn_cancellable(
        cmd: &str,
        workspace: &str,
        model: &str,
        cancel: Cancel,
    ) -> Result<Self, String> {
        let parts = arguments(cmd)?;
        let program = &parts[0];
        let args = &parts[1..];

        let mut command = Command::new(program);
        crate::sandbox::scrub_sensitive_env(&mut command);
        // This reads the sidecar and installs a per-child OS jail before the
        // extension can execute or announce tools in its handshake.
        let notes =
            crate::sandbox::harden_extension(&mut command, cmd, std::path::Path::new(workspace))?;
        for note in notes {
            eprintln!("glass-core: extension `{cmd}`: {note}");
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
        // The pre-exec jail closure owns its ruleset descriptor. Drop the
        // command now so the host does not retain one ruleset FD per extension.
        drop(command);

        let raw_fd = child.stdout.as_ref().map(fd_of).unwrap_or(-1);
        let mut ext = Self {
            manifest: Manifest::default(),
            stdin: child.stdin.take(),
            stdout: child.stdout.take().map(BufReader::new),
            raw_fd,
            next_id: 0,
            subagent: None,
            custom: std::collections::BTreeMap::new(),
            custom_bytes: 64,
            pending_custom: Vec::new(),
            session_path: None,
            child,
            stopped: false,
            live: false,
            cancel,
        };

        let hello = Request::Hello {
            protocol: PROTOCOL_VERSION,
            workspace: workspace.to_string(),
            model: model.to_string(),
        };
        let reply = match ext.request_outcome(&hello, REQUEST_TIMEOUT_MS * 3, None) {
            RequestOutcome::Answered(v) => v,
            // A cancelled startup is not a broken extension. Saying so
            // separately keeps a user who pressed Esc from being told the
            // extension "closed before the handshake".
            RequestOutcome::Cancelled(_) => {
                return Err(format!("extension `{cmd}` startup cancelled"));
            }
            RequestOutcome::Unavailable(reason) => {
                return Err(format!("extension `{cmd}` closed before the handshake: {reason}"));
            }
        };
        ext.manifest = parse_manifest(&reply).map_err(|e| format!("extension `{cmd}`: {e}"))?;
        Ok(ext)
    }

    /// Close the startup phase and hand the caller the live cancel token.
    ///
    /// The previous version replaced the token with a fresh, permanently
    /// clear flag, which meant the token the caller still held stopped
    /// reaching this extension the instant startup finished. Instead the same
    /// token stays wired in, and it is cleared on the way out: the startup
    /// caller is finished asking, and a flag left set by an aborted attempt
    /// must not cancel the extension that replaced it.
    ///
    /// Returns the token so the caller keeps a handle on this extension.
    pub fn finish_startup(&mut self) -> Cancel {
        self.cancel.reset();
        self.live = true;
        self.cancel.clone()
    }

    /// The token that cancels this extension, live or still starting.
    #[allow(dead_code)]
    pub fn cancel_token(&self) -> Cancel {
        self.cancel.clone()
    }

    /// Whether this extension has finished its handshake.
    #[allow(dead_code)]
    pub fn is_live(&self) -> bool {
        self.live
    }

    /// Ask this extension's current exchange to stop waiting.
    #[allow(dead_code)]
    pub fn cancel(&self) {
        self.cancel.cancel();
    }

    /// Whether a cancel has been issued against this extension.
    pub fn is_cancelled(&self) -> bool {
        self.cancel.is_cancelled()
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
        host_tools: Option<&mut HostToolBridge<'_>>,
    ) -> Option<serde_json::Value> {
        self.request_with_input(req, timeout_ms, host_tools, None)
    }

    fn request_with_input(
        &mut self,
        req: &Request,
        timeout_ms: u64,
        host_tools: Option<&mut HostToolBridge<'_>>,
        host_input: Option<&mut HostInputFn<'_>>,
    ) -> Option<serde_json::Value> {
        self.request_scoped(req, timeout_ms, host_tools, host_input, &CallScope::unscoped())
            .into_value()
    }

    /// The full exchange, carrying the call's lineage and cancel token.
    ///
    /// `scope` is what makes a cancel reach work already in flight: every
    /// wait below observes the extension's own token as well as the caller's,
    /// so a cancel issued from either side stops the wait promptly instead of
    /// running out the timeout.
    fn request_scoped(
        &mut self,
        req: &Request,
        timeout_ms: u64,
        host_tools: Option<&mut HostToolBridge<'_>>,
        host_input: Option<&mut HostInputFn<'_>>,
        scope: &CallScope,
    ) -> RequestOutcome {
        if !crate::plugin_dev::enabled() {
            return self.request_inner(req, timeout_ms, host_tools, host_input, scope);
        }
        let started = std::time::Instant::now();
        let result = self.request_inner(req, timeout_ms, host_tools, host_input, scope);
        let kind = match req {
            Request::Hello { .. } => "hello",
            Request::SystemPrompt => "system_prompt",
            Request::ToolResult { .. } => "tool_result",
            Request::Authorize { .. } => "authorize",
            Request::TurnEnd { .. } => "turn_end",
            Request::Command { .. } => "command",
            Request::RunTool { .. } | Request::RunToolInput { .. } => "run_tool",
            Request::Goodbye => "goodbye",
            Request::SeedCustom { .. } => "seed_custom",
        };
        crate::plugin_dev::record(
            &self.manifest.name,
            kind,
            started.elapsed().as_millis(),
            matches!(result, RequestOutcome::Answered(_)),
        );
        result
    }

    fn request_inner(
        &mut self,
        req: &Request,
        timeout_ms: u64,
        host_tools: Option<&mut HostToolBridge<'_>>,
        host_input: Option<&mut HostInputFn<'_>>,
        scope: &CallScope,
    ) -> RequestOutcome {
        // One exchange. It returns a `Result` internally only so the nested
        // reply writes below can `?`: a pipe that has gone away mid-exchange
        // ends the exchange with a reason instead of silently continuing to
        // service requests nobody will read.
        let result = self.request_exchange(req, timeout_ms, host_tools, host_input, scope);
        match result {
            Ok(v) => RequestOutcome::Answered(v),
            Err(stop) => stop,
        }
    }

    fn request_exchange(
        &mut self,
        req: &Request,
        timeout_ms: u64,
        mut host_tools: Option<&mut HostToolBridge<'_>>,
        mut host_input: Option<&mut HostInputFn<'_>>,
        scope: &CallScope,
    ) -> Result<serde_json::Value, RequestOutcome> {
        self.next_id += 1;
        let id = self.next_id;
        let mut line = match serde_json::to_string(&req.to_json(id)) {
            Ok(l) => l,
            Err(e) => return Err(RequestOutcome::Unavailable(format!("cannot encode request: {e}"))),
        };
        line.push('\n');

        // A cancel that arrived before dispatch must not send the request at
        // all. Writing it and then ignoring the answer would make an
        // already-abandoned call look like one that ran.
        if self.cancel.is_cancelled() || scope.cancel.is_cancelled() {
            return Err(RequestOutcome::Cancelled(CancelPath::Cooperative));
        }
        {
            let Some(stdin) = self.stdin.as_mut() else {
                return Err(RequestOutcome::Unavailable("the extension closed its input pipe".into()));
            };
            if stdin.write_all(line.as_bytes()).is_err() || stdin.flush().is_err() {
                return Err(RequestOutcome::Unavailable("the extension closed its input pipe".into()));
            }
        }

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
        //
        // `chain` is the lineage the host actually granted. Every nested
        // dispatch extends it, so a runaway or looping extension is refused
        // by `CallLineage` rather than being allowed to recurse.
        let mut deadline = std::time::Instant::now() + std::time::Duration::from_millis(timeout_ms);
        let mut chain = scope.lineage.clone();
        // Watching both tokens is what makes a cancel reach work already in
        // flight: the extension's own token covers a cancel issued from
        // anywhere, and the caller's covers one scoped to this call.
        let extension_token = self.cancel.clone();
        let call_token = scope.cancel.clone();
        let watch = move || {
            extension_token.is_cancelled() || call_token.is_cancelled()
        };
        loop {
            let remaining = deadline.saturating_duration_since(std::time::Instant::now());
            if remaining.is_zero() {
                return Err(RequestOutcome::Unavailable(format!(
                    "no answer within {timeout_ms}ms"
                )));
            }
            let v = match self.read_reply(remaining.as_millis().max(1) as u64, &watch) {
                Ok(v) => v,
                // A cancel and a dead pipe are different facts and must not
                // be reported as the same thing.
                Err(ReadFailure::Cancelled) => {
                    return Err(RequestOutcome::Cancelled(CancelPath::Cooperative));
                }
                Err(ReadFailure::Unavailable(reason)) => {
                    return Err(RequestOutcome::Unavailable(reason));
                }
            };
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
                            let call_id =
                                v.get("call_id").cloned().unwrap_or(serde_json::Value::Null);
                            self.reply_or_fail(protocol::subagents_result_json(&call_id, &[], Some(&e)))?;
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
                let Some(runner) = self.subagent.as_mut() else {
                    let call_id = v.get("call_id").cloned().unwrap_or(serde_json::Value::Null);
                    self.reply_or_fail(protocol::subagents_result_json(
                        &call_id,
                        &[],
                        Some("no subagent runner is installed for this extension"),
                    ))?;
                    deadline = std::time::Instant::now()
                        + std::time::Duration::from_millis(timeout_ms);
                    continue;
                };
                let outputs = runner(&tasks);
                // Frame every report so its content cannot read as
                // session instructions: the envelope travels with the
                // report into the tool result the model sees.
                let outputs: Vec<String> = outputs
                    .iter()
                    .map(|output| envelope_report(output))
                    .collect();
                let call_id = v.get("call_id").cloned().unwrap_or(serde_json::Value::Null);
                if batched {
                    self.reply_or_fail(protocol::subagents_result_json(&call_id, &outputs, None))?;
                } else {
                    let out = outputs.first().cloned().unwrap_or_default();
                    self.reply_or_fail(protocol::subagent_result_json(&call_id, &out))?;
                }
                deadline = std::time::Instant::now() + std::time::Duration::from_millis(timeout_ms);
                continue;
            }
            if is_custom_append(&v) {
                // A durable entry for the session log. The host owns
                // the log, so success is returned only after the checked
                // append has completed.
                let kind = v
                    .get("kind")
                    .and_then(|k| k.as_str())
                    .unwrap_or("")
                    .to_string();
                let data = v.get("data").and_then(|d| d.as_str()).map(str::to_string);
                let call_id = v.get("call_id").cloned().unwrap_or(serde_json::Value::Null);
                let (ok, error) = match self.append_custom(kind, data) {
                    Ok(()) => (true, None),
                    Err(e) => (false, Some(e)),
                };
                self.reply_or_fail(protocol::custom_result_json(
                    &call_id,
                    ok,
                    &[],
                    error.as_deref(),
                ))?;
                deadline = std::time::Instant::now() + std::time::Duration::from_millis(timeout_ms);
                continue;
            }
            if is_custom_read(&v) {
                let kind = v
                    .get("kind")
                    .and_then(|k| k.as_str())
                    .unwrap_or("")
                    .to_string();
                let call_id = v.get("call_id").cloned().unwrap_or(serde_json::Value::Null);
                let entries = self.read_custom(&kind);
                let reply = protocol::custom_result_json(&call_id, true, &entries, None);
                if serde_json::to_vec(&reply).map_or(true, |v| v.len() > MAX_CUSTOM_STATE_BYTES) {
                    self.reply_or_fail(protocol::custom_result_json(&call_id, false, &[], Some("saved custom entries exceed the replay limit; inspect the session log")))?;
                } else { self.reply_or_fail(reply)?; }
                deadline = std::time::Instant::now() + std::time::Duration::from_millis(timeout_ms);
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
                let nested_id = v
                    .get("call_id")
                    .and_then(|c| c.as_str())
                    .unwrap_or("")
                    .to_string();
                // The host's chain is authoritative. An extension may echo
                // the lineage it believes it is running under, but it cannot
                // re-parent the work onto another model call, and it cannot
                // report a chain that loops or outruns the depth bound.
                let claimed = match v.get("lineage") {
                    Some(raw) => match CallLineage::from_json(raw) {
                        Ok(claimed) => Some(claimed),
                        Err(reason) => {
                            let call_id =
                                v.get("call_id").cloned().unwrap_or(serde_json::Value::Null);
                            self.reply_or_fail(protocol::nested_call_result_json(
                                &call_id,
                                &format!("error: {reason}"),
                                None,
                                &chain,
                            ))?;
                            deadline = std::time::Instant::now()
                                + std::time::Duration::from_millis(timeout_ms);
                            continue;
                        }
                    },
                    None => None,
                };
                if let Some(claimed) = &claimed
                    && let Err(reason) = chain.verify_child(claimed, &nested_id)
                {
                    let call_id = v.get("call_id").cloned().unwrap_or(serde_json::Value::Null);
                    self.reply_or_fail(protocol::nested_call_result_json(
                        &call_id,
                        &format!("error: {reason}"),
                        Some(claimed),
                        &chain,
                    ))?;
                    deadline =
                        std::time::Instant::now() + std::time::Duration::from_millis(timeout_ms);
                    continue;
                }
                // Extending the chain is the depth bound. A refusal here
                // stops the recursion instead of entering it.
                let next = match chain.descend(&nested_id) {
                    Ok(next) => next,
                    Err(reason) => {
                        let call_id = v.get("call_id").cloned().unwrap_or(serde_json::Value::Null);
                        self.reply_or_fail(protocol::nested_call_result_json(
                            &call_id,
                            &format!("error: {reason}"),
                            claimed.as_ref(),
                            &chain,
                        ))?;
                        deadline =
                            std::time::Instant::now() + std::time::Duration::from_millis(timeout_ms);
                        continue;
                    }
                };
                let granted = next.clone();
                let out = match host_tools.as_deref_mut() {
                    Some(bridge) => bridge(&name, &args),
                    None => format!(
                        "error: nested tool call `{name}` refused: this hook has no host-tool bridge"
                    ),
                };
                chain = next;
                let call_id = v.get("call_id").cloned().unwrap_or(serde_json::Value::Null);
                self.reply_or_fail(protocol::nested_call_result_json(
                    &call_id,
                    &out,
                    claimed.as_ref(),
                    &granted,
                ))?;
                deadline = std::time::Instant::now() + std::time::Duration::from_millis(timeout_ms);
                continue;
            }
            if is_input_request(&v) {
                let call_id = v.get("call_id").cloned().unwrap_or(serde_json::Value::Null);
                let server = v.get("server").and_then(|x| x.as_str()).unwrap_or("");
                let params = v.get("params").cloned().unwrap_or(serde_json::Value::Null);
                let response = match host_input.as_deref_mut() {
                    Some(callback) => callback(server, &params),
                    None => serde_json::json!({"action":"decline"}),
                };
                self.reply_or_fail(input_result_json(&call_id, &response))?;
                deadline = std::time::Instant::now() + std::time::Duration::from_millis(timeout_ms);
                continue;
            }
            // Only an unsigned request ID can correlate a reply. Nested host
            // requests have already been dispatched above using their own types.
            let Some(reply_id) = v.get("id").and_then(serde_json::Value::as_u64) else {
                return Err(RequestOutcome::Unavailable("extension reply is missing a valid unsigned request id".into()));
            };
            if reply_id != id {
                continue; // A late reply to an earlier exchange.
            }
            return Ok(v);
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

    /// Write a reply, turning a dead pipe into the call's own failure.
    ///
    /// A nested result the host cannot write back is not a result the
    /// extension will ever see, so the exchange ends as unavailable rather
    /// than continuing to service a request nobody is listening to.
    fn reply_or_fail(&mut self, v: serde_json::Value) -> Result<(), RequestOutcome> {
        self.reply(v).ok_or_else(|| {
            RequestOutcome::Unavailable("the extension closed its input pipe".into())
        })
    }

    /// Read one line, capped in both size and time, watching a cancel as it goes.
    ///
    /// The wait has to observe the cancel *inside* the poll loop, not only
    /// between reads: one read can legitimately block for the whole reply
    /// timeout, and checking between reads would leave a cancelled call
    /// waiting out the full timeout anyway.
    ///
    /// `watch` is called between poll slices. When it reports a cancel, one
    /// last bounded drain runs before giving up: a reply the extension
    /// already finished writing is work that completed, and reporting it as
    /// cancelled would tell the model its work did not happen when it did.
    ///
    /// The reason is returned rather than a bare `None` so that "the pipe
    /// closed", "the reply was too large" and "the deadline passed" stay
    /// distinguishable from a cancel: they mean very different things to
    /// whoever reads the transcript, and collapsing them is how a cancelled
    /// call ends up being reported as a broken extension.
    fn read_reply(
        &mut self,
        timeout_ms: u64,
        watch: &dyn Fn() -> bool,
    ) -> Result<serde_json::Value, ReadFailure> {
        let Some(stdout) = self.stdout.as_mut() else {
            return Err(ReadFailure::Unavailable(
                "the extension closed its output pipe".into(),
            ));
        };
        let deadline = std::time::Instant::now() + std::time::Duration::from_millis(timeout_ms);
        let mut bytes = Vec::new();
        loop {
            let remaining = deadline.saturating_duration_since(std::time::Instant::now());
            if remaining.is_zero() {
                return Err(ReadFailure::Unavailable(format!(
                    "no reply within {timeout_ms}ms"
                )));
            }
            if watch() {
                let drain = remaining.min(std::time::Duration::from_millis(CANCEL_DRAIN_MS));
                return match read_line(stdout, self.raw_fd, drain, &mut bytes) {
                    Ok(v) => Ok(v),
                    Err(ReadFailure::Cancelled) | Err(ReadFailure::Unavailable(_)) => {
                        Err(ReadFailure::Cancelled)
                    }
                };
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
            if stdout.read_exact(&mut byte).is_err() {
                return Err(ReadFailure::Unavailable(
                    "the extension closed its output pipe".into(),
                ));
            }
            if byte[0] == b'\n' {
                break;
            }
            if bytes.len() >= MAX_REPLY_BYTES {
                return Err(ReadFailure::Unavailable(format!(
                    "reply exceeded {MAX_REPLY_BYTES} bytes"
                )));
            }
            bytes.push(byte[0]);
        }
        serde_json::from_slice(&bytes)
            .map_err(|e| ReadFailure::Unavailable(format!("malformed reply: {e}")))
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
        let v = self.request(&req, REQUEST_TIMEOUT_MS * 10, Some(host_tools))?;
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

    /// Send a request and read one reply, keeping the reason it ended.
    ///
    /// The handshake uses this rather than `request` because a cancelled
    /// startup and a failed one need different messages: telling a user who
    /// pressed Esc that their extension "closed before the handshake" is
    /// simply false.
    fn request_outcome(
        &mut self,
        req: &Request,
        timeout_ms: u64,
        host_tools: Option<&mut HostToolBridge<'_>>,
    ) -> RequestOutcome {
        self.request_scoped(req, timeout_ms, host_tools, None, &CallScope::unscoped())
    }

    /// Run a slash command under a call scope. Returns the extension's output.
    ///
    /// A cancelled command is reported as cancelled, not as an extension that
    /// failed: the command may have started work the extension is still doing,
    /// and saying "did not return a result" would imply the same silence as a
    /// command that was never offered.
    pub fn run_command_scoped(
        &mut self,
        name: &str,
        args: &str,
        scope: &CallScope,
    ) -> CallOutcome {
        if !self.has_command(name) {
            return CallOutcome::stopped(CallStop::Unknown);
        }
        let req = Request::Command {
            name: name.to_string(),
            args: args.to_string(),
        };
        match self.request_scoped(&req, REQUEST_TIMEOUT_MS, None, None, scope) {
            RequestOutcome::Answered(v) => match v.get("output").and_then(|o| o.as_str()) {
                Some(out) => CallOutcome::completed(out.to_string()),
                // A reply without an output is a protocol failure, not an
                // empty success: inventing an empty string here is how a
                // broken extension becomes a confident wrong answer.
                None => CallOutcome::stopped(CallStop::Failed(
                    "replied without an output field".into(),
                )),
            },
            RequestOutcome::Cancelled(_) => {
                // A half-finished serial exchange cannot be reused: its late
                // reply could be mistaken for the next request's result.
                let path = self.force_stop();
                CallOutcome::stopped(CallStop::Cancelled(path))
            }
            other => CallOutcome::stopped(other.stop()),
        }
    }

    /// Execute one of this extension's tools, servicing any nested tool calls
    /// it makes while it runs.
    ///
    /// Returns `CallStop::Unknown` when the extension does not offer the tool
    /// and `CallStop::Cancelled` when the caller cancelled, so neither is
    /// mistaken for a tool that ran and returned nothing.
    pub fn run_tool_scoped(
        &mut self,
        name: &str,
        arguments: &str,
        host_tools: &mut HostToolBridge<'_>,
        scope: &CallScope,
    ) -> CallOutcome {
        if !self.manifest.tools.iter().any(|t| t.name == name) {
            return CallOutcome::stopped(CallStop::Unknown);
        }
        if let Err(reason) = scope.check() {
            return CallOutcome::stopped(CallStop::Failed(reason));
        }
        let req = Request::RunTool {
            call_id: scope.lineage.origin().to_string(),
            lineage: scope.lineage.clone(),
            name: name.to_string(),
            arguments: arguments.to_string(),
        };
        self.dispatch_tool(&req, Some(host_tools), None, scope)
    }

    /// Execute a tool with a scoped host input callback. The callback is present only for
    /// the duration of this request and is never retained or logged by the extension host.
    pub fn run_tool_with_input_scoped(
        &mut self,
        name: &str,
        arguments: &str,
        host_tools: &mut HostToolBridge<'_>,
        host_input: &mut HostInputFn<'_>,
        scope: &CallScope,
    ) -> CallOutcome {
        if !self.manifest.tools.iter().any(|t| t.name == name) {
            return CallOutcome::stopped(CallStop::Unknown);
        }
        if let Err(reason) = scope.check() {
            return CallOutcome::stopped(CallStop::Failed(reason));
        }
        let req = Request::RunToolInput {
            call_id: scope.lineage.origin().to_string(),
            lineage: scope.lineage.clone(),
            name: name.to_string(),
            arguments: arguments.to_string(),
        };
        self.dispatch_tool(&req, Some(host_tools), Some(host_input), scope)
    }

    /// Run one tool request and shape the exchange into a [`CallOutcome`].
    fn dispatch_tool(
        &mut self,
        req: &Request,
        host_tools: Option<&mut HostToolBridge<'_>>,
        host_input: Option<&mut HostInputFn<'_>>,
        scope: &CallScope,
    ) -> CallOutcome {
        match self.request_scoped(req, REQUEST_TIMEOUT_MS * 10, host_tools, host_input, scope) {
            RequestOutcome::Answered(v) => match v.get("output").and_then(|o| o.as_str()) {
                Some(out) if !out.is_empty() => CallOutcome::completed(if v.get("is_error").and_then(|x| x.as_bool()) == Some(true) && !out.starts_with("error:") { format!("error: {out}") } else { out.to_string() }),
                Some(_) => CallOutcome::stopped(CallStop::Failed("returned an empty result".into())),
                None => CallOutcome::stopped(CallStop::Failed(
                    "replied without an output field".into(),
                )),
            },
            RequestOutcome::Cancelled(_) => {
                // The pending reply must never become the next call's reply.
                let path = self.force_stop();
                CallOutcome::stopped(CallStop::Cancelled(path))
            }
            other => CallOutcome::stopped(other.stop()),
        }
    }

    pub fn has_command(&self, name: &str) -> bool {
        self.manifest.commands.iter().any(|c| c.name == name)
    }

    /// Append a durable entry for the session log.
    ///
    /// Kinds are namespaced `<extension-name>/…` so one
    /// extension cannot write into another's state. The
    /// acknowledgment follows the checked append.
    pub fn append_custom(&mut self, kind: String, data: Option<String>) -> Result<(), String> {
        let prefix = format!("{}/", self.manifest.name);
        if !kind.starts_with(&prefix) {
            return Err(format!("custom kinds are namespaced: `{prefix}…`"));
        }
        let added = data.as_ref().map_or(0, |text| serde_json::to_vec(text).map_or(usize::MAX, |v| v.len().saturating_add(1)))
            .saturating_add(if self.custom.contains_key(&kind) { 0 } else { serde_json::to_vec(&kind).map_or(usize::MAX, |v| v.len().saturating_add(32)) });
        if self.custom_bytes.saturating_add(added) > MAX_CUSTOM_STATE_BYTES {
            return Err("custom state exceeds the 3 MiB serialized replay limit; entry was not saved. Store large data in files and append a reference".into());
        }
        let path = self.session_path.as_ref().ok_or_else(|| {
            "session log is unavailable; custom entry was not acknowledged".to_string()
        })?;
        crate::session::Session::append_custom_path(path, &kind, data.as_deref())
            .map_err(|error| format!("cannot durably append custom entry: {error}"))?;
        self.custom_bytes = self.custom_bytes.saturating_add(added);
        self.custom
            .entry(kind.clone())
            .or_default()
            .extend(data.clone());
        Ok(())
    }

    pub fn bind_session_path(&mut self, path: std::path::PathBuf) {
        self.session_path = Some(path);
    }

    /// Take the entries queued for the session log.
    pub fn take_custom(&mut self) -> Vec<(String, Option<String>)> {
        std::mem::take(&mut self.pending_custom)
    }

    /// Seed the extension's entries from the session log, and
    /// hand them to the extension process so it can rebuild
    /// state that outlives a restart.
    pub fn seed_custom(&mut self, entries: Vec<(String, Vec<String>)>) {
        let prefix = format!("{}/", self.manifest.name);
        let mine: Vec<(String, Vec<String>)> = entries
            .into_iter()
            .filter(|(kind, _)| kind.starts_with(&prefix))
            .collect();
        let seed_bytes = serde_json::to_vec(&Request::SeedCustom { entries: mine.clone() }.to_json(0)).map_or(usize::MAX, |v| v.len());
        self.custom_bytes = seed_bytes;
        let had_custom = !self.custom.is_empty();
        self.custom.clear();
        self.pending_custom.clear();
        for (kind, data) in &mine {
            self.custom.insert(kind.clone(), data.clone());
        }
        // Best-effort: an extension that does not answer the
        // seed keeps working, it just starts with empty state.
        if seed_bytes > MAX_CUSTOM_STATE_BYTES {
            // Clear the extension-side view on session switches; never leave
            // another session's state active when this seed cannot be replayed.
            let _ = self.request(&Request::SeedCustom { entries: Vec::new() }, REQUEST_TIMEOUT_MS, None);
            eprintln!("extension {}: saved custom state exceeds the 3 MiB replay limit; state remains in the session log and was not sent", self.manifest.name);
            return;
        }
        if !mine.is_empty() || had_custom {
            let req = Request::SeedCustom { entries: mine };
            let _ = self.request(&req, REQUEST_TIMEOUT_MS, None);
        }
    }

    /// The extension's entries for one kind, seeded and
    /// appended, in append order.
    pub fn read_custom(&self, kind: &str) -> Vec<String> {
        self.custom.get(kind).cloned().unwrap_or_default()
    }

    /// Shut the extension down, escalating to a kill if it ignores the request.
    ///
    /// Returns what actually happened rather than nothing, so a caller can
    /// tell "every extension stopped cleanly" from "one had to be killed and
    /// its work was lost". The escalation itself is unchanged: the process
    /// *group* is killed, which is what reaches TERM-ignoring descendants, and
    /// it still fires even after the child itself exited.
    ///
    /// Idempotent: a second call does no signalling and reports
    /// `already_stopped`, so a shutdown followed by a `Drop` — or by a second
    /// shutdown from a different layer — is harmless.
    pub fn shutdown(&mut self) -> ShutdownOutcome {
        let name = self.manifest.name.clone();
        if self.stopped {
            return ShutdownOutcome {
                name,
                graceful: true,
                killed: false,
                already_stopped: true,
                error: None,
            };
        }
        // The goodbye is advisory, and only worth sending while the extension
        // is still supposed to be running: a cancelled extension has already
        // been asked to stop and must not be waited on. Whether it answers is
        // not what decides the outcome — a well-behaved extension exits on the
        // closed pipe without ever replying to `goodbye`. The kill below is
        // what actually guarantees the exit.
        if !self.is_cancelled() {
            let _ = self.request(&Request::Goodbye, 200, None);
        }
        // Dropping stdin should let a well-behaved extension exit on EOF.
        self.stdin = None;
        let mut error = None;
        let mut graceful = false;
        for _ in 0..20 {
            match self.child.try_wait() {
                Ok(Some(_)) => {
                    // The child is gone, but its descendants may not be: keep
                    // killing the group so nothing it started survives it.
                    unsafe {
                        libc::kill(-(self.child.id() as i32), libc::SIGKILL);
                    }
                    self.stopped = true;
                    graceful = true;
                    break;
                }
                Ok(None) => std::thread::sleep(std::time::Duration::from_millis(10)),
                Err(e) => {
                    error = Some(format!("cannot wait for the extension process: {e}"));
                    break;
                }
            }
        }
        let mut killed = false;
        if !graceful {
            unsafe {
                libc::kill(-(self.child.id() as i32), libc::SIGKILL);
            }
            killed = true;
            if let Err(e) = self.child.kill() {
                // The group signal already went out; a failing `kill(2)` on
                // an already-dead child is not a shutdown failure worth
                // escalating into a panic, but it is worth reporting.
                error.get_or_insert_with(|| format!("cannot kill the extension process: {e}"));
            }
            let _ = self.child.wait();
        }
        self.stopped = true;
        ShutdownOutcome {
            name,
            graceful,
            killed,
            already_stopped: false,
            error,
        }
    }

    /// Stop this extension without the grace period, killing the whole
    /// process group so nothing it started outlives it.
    ///
    /// The fallback for an extension that will not stop cooperatively: a
    /// cancel that only stops the host's waiting would otherwise leave a
    /// process burning CPU for the rest of the session. Reports which path
    /// was taken so the caller never has to guess whether the kill fired.
    #[allow(dead_code)]
    pub fn force_stop(&mut self) -> CancelPath {
        if self.stopped {
            return CancelPath::Cooperative;
        }
        self.cancel.cancel();
        self.stdin = None;
        self.kill_group();
        self.stopped = true;
        CancelPath::Killed
    }

    fn kill_group(&mut self) {
        unsafe {
            libc::kill(-(self.child.id() as i32), libc::SIGKILL);
        }
        let _ = self.child.kill();
        let _ = self.child.wait();
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
