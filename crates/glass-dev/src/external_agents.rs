//! One-shot adapters for installed external coding agents.
//!
//! External agents are deliberately temporary: Glass resolves a fixed executable,
//! starts one bounded child process, captures bounded output, and never registers
//! the child as a resident Glass Agent. The default sandbox is read-only.

use crate::development::ToolAuthorization;
use crate::harness;
use serde::Serialize;
use std::ffi::OsString;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

pub const DEFAULT_TIMEOUT_SECS: u64 = 600;
const MIN_TIMEOUT: Duration = Duration::from_secs(1);
const MAX_PROMPT_BYTES: usize = 64 * 1024;
const MAX_STDOUT_BYTES: usize = 256 * 1024;
const MAX_STDERR_BYTES: usize = 64 * 1024;
const MAX_TIMEOUT: Duration = Duration::from_secs(60 * 60);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExternalHarness {
    Codex,
    Claude,
    OpenCode,
}

impl ExternalHarness {
    pub fn parse(name: &str) -> Result<Self, String> {
        match name.trim().to_ascii_lowercase().as_str() {
            "codex" | "codex-cli" => Ok(Self::Codex),
            "claude" | "claude-code" => Ok(Self::Claude),
            "opencode" | "open-code" => Ok(Self::OpenCode),
            _ => Err(format!(
                "unsupported temporary agent `{name}`; choose codex, claude, or opencode"
            )),
        }
    }

    pub const fn id(self) -> &'static str {
        match self {
            Self::Codex => "codex",
            Self::Claude => "claude",
            Self::OpenCode => "opencode",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ExternalSandbox {
    ReadOnly,
    WorkspaceWrite,
}

impl ExternalSandbox {
    pub fn parse(value: &str) -> Result<Self, String> {
        match value.trim().to_ascii_lowercase().as_str() {
            "read-only" | "readonly" | "read_only" => Ok(Self::ReadOnly),
            "workspace-write" | "workspacewrite" | "workspace_write" => Ok(Self::WorkspaceWrite),
            _ => Err(format!(
                "unsupported temporary-agent sandbox `{value}`; choose read-only or workspace-write"
            )),
        }
    }

    pub const fn id(self) -> &'static str {
        match self {
            Self::ReadOnly => "read-only",
            Self::WorkspaceWrite => "workspace-write",
        }
    }
}

#[derive(Debug, Clone)]
pub struct ExternalAgentRequest {
    pub harness: String,
    pub root: PathBuf,
    pub prompt: String,
    pub sandbox: ExternalSandbox,
    pub timeout: Duration,
    pub authorization: ToolAuthorization,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExternalAgentResult {
    pub harness: String,
    pub transport: String,
    pub sandbox: ExternalSandbox,
    pub success: bool,
    pub status: Option<i32>,
    pub timed_out: bool,
    pub duration_ms: u64,
    pub output: String,
    pub stderr: String,
    pub output_truncated: bool,
    pub stderr_truncated: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ExternalInvocation {
    pub program: PathBuf,
    pub args: Vec<OsString>,
    pub transport: &'static str,
}

/// Resolve and execute one temporary external-agent request.
pub(crate) fn delegate(request: ExternalAgentRequest) -> Result<ExternalAgentResult, String> {
    validate_request(&request)?;
    let root = fs::canonicalize(&request.root).map_err(|error| {
        format!(
            "could not resolve delegation root {}: {error}",
            request.root.display()
        )
    })?;
    if !root.is_dir() {
        return Err(format!(
            "delegation root is not a directory: {}",
            root.display()
        ));
    }

    let harness = ExternalHarness::parse(&request.harness)?;
    let resolved = harness::resolve(harness.id())?;
    let invocation = build_invocation(
        harness,
        resolved.path,
        &root,
        &request.prompt,
        request.sandbox,
    );

    let mut command = Command::new(&invocation.program);
    command
        .args(&invocation.args)
        .current_dir(&root)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    configure_process_group(&mut command);
    let started = Instant::now();
    let mut child = command
        .spawn()
        .map_err(|error| format!("could not start {}: {error}", harness.id()))?;
    let process_tree = match ProcessTree::assign(child.id()) {
        Ok(process_tree) => process_tree,
        Err(error) => {
            let _ = child.kill();
            let _ = child.wait();
            return Err(format!(
                "could not own {} process tree: {error}",
                harness.id()
            ));
        }
    };
    let stdout = match child.stdout.take() {
        Some(stdout) => stdout,
        None => {
            process_tree.terminate();
            let _ = child.kill();
            let _ = child.wait();
            return Err(format!("{} did not expose stdout", harness.id()));
        }
    };
    let stderr = match child.stderr.take() {
        Some(stderr) => stderr,
        None => {
            process_tree.terminate();
            let _ = child.kill();
            let _ = child.wait();
            return Err(format!("{} did not expose stderr", harness.id()));
        }
    };
    let (sender, receiver) = mpsc::channel();
    let stdout_thread = spawn_reader(stdout, StreamKind::Stdout, sender.clone(), MAX_STDOUT_BYTES);

    let stderr_thread = spawn_reader(stderr, StreamKind::Stderr, sender.clone(), MAX_STDERR_BYTES);
    drop(sender);

    let mut stdout_bytes = Vec::new();
    let mut stderr_bytes = Vec::new();
    let mut stdout_truncated = false;
    let mut stderr_truncated = false;
    let mut timed_out = false;
    let status = loop {
        drain_chunks(
            &receiver,
            &mut stdout_bytes,
            &mut stderr_bytes,
            &mut stdout_truncated,
            &mut stderr_truncated,
        );
        match child.try_wait() {
            Ok(Some(status)) => break Some(status),
            Ok(None) => {}
            Err(error) => {
                process_tree.terminate();
                let _ = child.kill();
                let _ = child.wait();
                let _ = stdout_thread.join();
                let _ = stderr_thread.join();
                return Err(format!("could not poll {}: {error}", harness.id()));
            }
        }
        if started.elapsed() >= request.timeout {
            timed_out = true;
            process_tree.terminate();
            let _ = child.kill();
            break child.wait().ok();
        }
        match receiver.recv_timeout(Duration::from_millis(20)) {
            Ok((kind, chunk)) => append_chunk(
                kind,
                &chunk,
                &mut stdout_bytes,
                &mut stderr_bytes,
                &mut stdout_truncated,
                &mut stderr_truncated,
            ),
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                // The child is still polled above; a closed pipe is not itself
                // proof that the process has exited.
            }
        }
    };

    // A temporary harness owns its process group. Reap descendants that keep
    // inherited output pipes open after the direct child exits.
    process_tree.terminate();
    let _ = stdout_thread.join();
    let _ = stderr_thread.join();
    drain_chunks(
        &receiver,
        &mut stdout_bytes,
        &mut stderr_bytes,
        &mut stdout_truncated,
        &mut stderr_truncated,
    );

    Ok(ExternalAgentResult {
        harness: harness.id().into(),
        transport: invocation.transport.into(),
        sandbox: request.sandbox,
        success: !timed_out && status.is_some_and(|status| status.success()),
        status: status.and_then(|status| status.code()),
        timed_out,
        duration_ms: started.elapsed().as_millis().min(u128::from(u64::MAX)) as u64,
        output: String::from_utf8_lossy(&stdout_bytes).into_owned(),
        stderr: String::from_utf8_lossy(&stderr_bytes).into_owned(),
        output_truncated: stdout_truncated,
        stderr_truncated,
    })
}

#[cfg(unix)]
fn configure_process_group(command: &mut Command) {
    use std::os::unix::process::CommandExt;
    // SAFETY: setpgid is async-signal-safe and assigns this child a fresh group.
    unsafe {
        command.pre_exec(|| {
            if libc::setpgid(0, 0) == -1 {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        });
    }
}

#[cfg(not(unix))]
fn configure_process_group(_command: &mut Command) {}

struct ProcessTree {
    pid: u32,
    #[cfg(windows)]
    job: WindowsJob,
}

impl ProcessTree {
    fn assign(pid: u32) -> std::io::Result<Self> {
        Ok(Self {
            pid,
            #[cfg(windows)]
            job: WindowsJob::assign(pid)?,
        })
    }

    fn terminate(&self) {
        #[cfg(unix)]
        // SAFETY: negative pid targets only the process group created for this run.
        unsafe {
            libc::kill(-(self.pid as i32), libc::SIGKILL);
        }
        #[cfg(windows)]
        // SAFETY: the job handle is valid and uniquely owned by this run.
        unsafe {
            windows_sys::Win32::System::JobObjects::TerminateJobObject(self.job.handle, 1);
        }
    }
}

#[cfg(windows)]
struct WindowsJob {
    handle: windows_sys::Win32::Foundation::HANDLE,
}

#[cfg(windows)]
impl WindowsJob {
    fn assign(pid: u32) -> std::io::Result<Self> {
        use windows_sys::Win32::{
            Foundation::CloseHandle,
            System::{
                JobObjects::{
                    AssignProcessToJobObject, CreateJobObjectW, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
                    JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JobObjectExtendedLimitInformation,
                    SetInformationJobObject,
                },
                Threading::{
                    OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_SET_QUOTA,
                    PROCESS_TERMINATE,
                },
            },
        };
        // SAFETY: null security/name creates an unnamed job owned by this process.
        let handle = unsafe { CreateJobObjectW(std::ptr::null(), std::ptr::null()) };
        if handle.is_null() {
            return Err(std::io::Error::last_os_error());
        }
        let mut limits: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = unsafe { std::mem::zeroed() };
        limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        // SAFETY: the information pointer and size match the requested class.
        let configured = unsafe {
            SetInformationJobObject(
                handle,
                JobObjectExtendedLimitInformation,
                (&raw const limits).cast(),
                std::mem::size_of_val(&limits) as u32,
            )
        };
        // SAFETY: requested rights are limited to job assignment and lifecycle.
        let process = unsafe {
            OpenProcess(
                PROCESS_SET_QUOTA | PROCESS_TERMINATE | PROCESS_QUERY_LIMITED_INFORMATION,
                0,
                pid,
            )
        };
        if configured == 0 || process.is_null() {
            // SAFETY: non-null job handle is owned here.
            unsafe { CloseHandle(handle) };
            return Err(std::io::Error::last_os_error());
        }
        // SAFETY: both handles are valid and owned for this call.
        let assigned = unsafe { AssignProcessToJobObject(handle, process) };
        // SAFETY: the temporary process handle is no longer needed.
        unsafe { CloseHandle(process) };
        if assigned == 0 {
            // SAFETY: assignment failed; closing the job releases its resources.
            unsafe { CloseHandle(handle) };
            return Err(std::io::Error::last_os_error());
        }
        Ok(Self { handle })
    }
}

#[cfg(windows)]
impl Drop for WindowsJob {
    fn drop(&mut self) {
        // SAFETY: this type uniquely owns the job handle.
        unsafe { windows_sys::Win32::Foundation::CloseHandle(self.handle) };
    }
}

fn validate_request(request: &ExternalAgentRequest) -> Result<(), String> {
    if request.prompt.trim().is_empty() {
        return Err("temporary-agent prompt must not be empty".into());
    }
    if request.prompt.contains('\0') {
        return Err("temporary-agent prompt contains a NUL byte".into());
    }
    if request.prompt.len() > MAX_PROMPT_BYTES {
        return Err(format!(
            "temporary-agent prompt exceeds the {MAX_PROMPT_BYTES}-byte limit"
        ));
    }
    if request.timeout < MIN_TIMEOUT || request.timeout > MAX_TIMEOUT {
        return Err("temporary-agent timeout must be between 1 second and 1 hour".into());
    }
    if request.sandbox == ExternalSandbox::WorkspaceWrite
        && !request.authorization.permits_mutation()
    {
        return Err("workspace-write delegation requires mutation authority".into());
    }
    Ok(())
}

pub(crate) fn build_invocation(
    harness: ExternalHarness,
    executable: PathBuf,
    root: &Path,
    prompt: &str,
    sandbox: ExternalSandbox,
) -> ExternalInvocation {
    let root = root.as_os_str().to_os_string();
    let prompt = OsString::from(prompt);
    let (args, transport) = match harness {
        ExternalHarness::Codex => (
            vec![
                "exec".into(),
                "--json".into(),
                "--ephemeral".into(),
                "--color".into(),
                "never".into(),
                "--sandbox".into(),
                sandbox.id().into(),
                "--cd".into(),
                root,
                "--".into(),
                prompt,
            ],
            "codex-jsonl",
        ),
        ExternalHarness::Claude => (
            vec![
                "--print".into(),
                "--output-format".into(),
                "stream-json".into(),
                "--no-session-persistence".into(),
                "--permission-mode".into(),
                match sandbox {
                    ExternalSandbox::ReadOnly => "plan".into(),
                    ExternalSandbox::WorkspaceWrite => "acceptEdits".into(),
                },
                "--add-dir".into(),
                root,
                "--".into(),
                prompt,
            ],
            "claude-stream-json",
        ),
        ExternalHarness::OpenCode => {
            let mut args = vec![
                "run".into(),
                "--format".into(),
                "json".into(),
                "--pure".into(),
            ];
            if sandbox == ExternalSandbox::ReadOnly {
                args.extend(["--agent".into(), "plan".into()]);
            }
            args.extend(["--dir".into(), root, "--".into(), prompt]);
            (args, "opencode-json")
        }
    };
    ExternalInvocation {
        program: executable,
        args,
        transport,
    }
}

#[derive(Debug, Clone, Copy)]
enum StreamKind {
    Stdout,
    Stderr,
}

fn spawn_reader<R: Read + Send + 'static>(
    mut reader: R,
    kind: StreamKind,
    sender: Sender<(StreamKind, Vec<u8>)>,
    limit: usize,
) -> JoinHandle<()> {
    thread::spawn(move || {
        let mut buffer = [0_u8; 8192];
        let mut read = 0;
        loop {
            if read >= limit {
                break;
            }
            let read_limit = (limit - read).min(buffer.len());
            let length = match reader.read(&mut buffer[..read_limit]) {
                Ok(0) => break,
                Ok(length) => length,
                Err(_) => break,
            };
            read += length;
            if sender.send((kind, buffer[..length].to_vec())).is_err() {
                break;
            }
        }
    })
}

fn drain_chunks(
    receiver: &Receiver<(StreamKind, Vec<u8>)>,
    stdout: &mut Vec<u8>,
    stderr: &mut Vec<u8>,
    stdout_truncated: &mut bool,
    stderr_truncated: &mut bool,
) {
    while let Ok((kind, chunk)) = receiver.try_recv() {
        append_chunk(
            kind,
            &chunk,
            stdout,
            stderr,
            stdout_truncated,
            stderr_truncated,
        );
    }
}

fn append_chunk(
    kind: StreamKind,
    chunk: &[u8],
    stdout: &mut Vec<u8>,
    stderr: &mut Vec<u8>,
    stdout_truncated: &mut bool,
    stderr_truncated: &mut bool,
) {
    let (target, truncated, limit) = match kind {
        StreamKind::Stdout => (stdout, stdout_truncated, MAX_STDOUT_BYTES),
        StreamKind::Stderr => (stderr, stderr_truncated, MAX_STDERR_BYTES),
    };
    if target.len() >= limit {
        *truncated = true;
        return;
    }
    let remaining = limit - target.len();
    if chunk.len() > remaining {
        target.extend_from_slice(&chunk[..remaining]);
        *truncated = true;
    } else {
        target.extend_from_slice(chunk);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::development::Actor;

    #[test]
    fn parses_only_supported_temporary_harnesses() {
        assert!(ExternalHarness::parse("Codex CLI").is_err());
        assert_eq!(ExternalHarness::parse("codex-cli").unwrap().id(), "codex");
        assert_eq!(
            ExternalHarness::parse("claude-code").unwrap().id(),
            "claude"
        );
        assert_eq!(
            ExternalHarness::parse("open-code").unwrap().id(),
            "opencode"
        );
        assert!(ExternalHarness::parse("aider").is_err());
    }

    #[test]
    fn invocation_keeps_prompt_and_root_as_single_arguments() {
        let invocation = build_invocation(
            ExternalHarness::Codex,
            PathBuf::from("/bin/codex"),
            Path::new("/tmp/project with spaces"),
            "inspect && do not mutate",
            ExternalSandbox::ReadOnly,
        );
        assert_eq!(invocation.program, PathBuf::from("/bin/codex"));
        assert_eq!(invocation.transport, "codex-jsonl");
        assert!(
            invocation
                .args
                .windows(2)
                .any(|pair| pair[0] == "--cd" && pair[1] == "/tmp/project with spaces")
        );
        assert_eq!(
            invocation.args.last(),
            Some(&OsString::from("inspect && do not mutate"))
        );
        assert!(
            invocation
                .args
                .windows(2)
                .any(|pair| pair[0] == "--" && pair[1] == "inspect && do not mutate")
        );
    }

    #[test]
    fn claude_prompt_is_positional_after_end_of_options() {
        let invocation = build_invocation(
            ExternalHarness::Claude,
            PathBuf::from("/bin/claude"),
            Path::new("/tmp/project with spaces"),
            "--dangerously-skip-permissions",
            ExternalSandbox::ReadOnly,
        );
        assert!(
            invocation
                .args
                .windows(2)
                .any(|pair| pair[0] == "--add-dir" && pair[1] == "/tmp/project with spaces")
        );
        assert!(
            invocation
                .args
                .windows(2)
                .any(|pair| pair[0] == "--" && pair[1] == "--dangerously-skip-permissions")
        );
        assert_eq!(
            invocation.args.last(),
            Some(&OsString::from("--dangerously-skip-permissions"))
        );
    }

    #[test]
    fn readonly_opencode_uses_plan_agent() {
        let read_only = build_invocation(
            ExternalHarness::OpenCode,
            PathBuf::from("/bin/opencode"),
            Path::new("/tmp/project"),
            "inspect the project",
            ExternalSandbox::ReadOnly,
        );
        assert!(
            read_only
                .args
                .windows(2)
                .any(|pair| pair[0] == "--agent" && pair[1] == "plan")
        );

        let workspace_write = build_invocation(
            ExternalHarness::OpenCode,
            PathBuf::from("/bin/opencode"),
            Path::new("/tmp/project"),
            "update the project",
            ExternalSandbox::WorkspaceWrite,
        );
        assert!(
            !workspace_write
                .args
                .windows(2)
                .any(|pair| pair[0] == "--agent" && pair[1] == "plan")
        );
    }

    #[test]
    fn reader_stops_at_stream_limit() {
        let (sender, receiver) = mpsc::channel();
        let reader = spawn_reader(
            std::io::Cursor::new(vec![0_u8; MAX_STDOUT_BYTES + 1]),
            StreamKind::Stdout,
            sender,
            MAX_STDOUT_BYTES,
        );

        reader.join().expect("reader thread should finish");
        let received = receiver
            .into_iter()
            .map(|(_, chunk)| chunk.len())
            .sum::<usize>();
        assert_eq!(received, MAX_STDOUT_BYTES);
    }

    #[cfg(unix)]
    #[test]
    fn completed_harness_group_closes_pipes_inherited_by_descendants() {
        let mut command = Command::new("/bin/sh");
        command
            .args(["-c", "sleep 30 & exit 0"])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        configure_process_group(&mut command);
        let mut child = command.spawn().unwrap();
        let process_tree = ProcessTree::assign(child.id()).unwrap();
        let stdout = child.stdout.take().unwrap();
        let reader = thread::spawn(move || {
            let mut bytes = Vec::new();
            let mut stdout = stdout;
            stdout.read_to_end(&mut bytes).unwrap();
        });

        let status = child.wait().unwrap();
        assert!(status.success());
        process_tree.terminate();
        reader.join().unwrap();
    }

    #[test]
    fn timeout_must_be_at_least_one_second() {
        let request = ExternalAgentRequest {
            harness: "codex".into(),
            root: PathBuf::from("."),
            prompt: "inspect the project".into(),
            sandbox: ExternalSandbox::ReadOnly,
            timeout: Duration::from_millis(999),
            authorization: ToolAuthorization::read_only(Actor::local()),
        };
        assert!(
            validate_request(&request)
                .expect_err("subsecond timeouts must fail")
                .contains("between 1 second and 1 hour")
        );
    }

    #[test]
    fn workspace_write_requires_both_authorization_factors() {
        for (allow_mutation, confirmed) in [(false, false), (false, true), (true, false)] {
            let request = ExternalAgentRequest {
                harness: "unsupported".into(),
                root: PathBuf::from("."),
                prompt: "edit the failing test".into(),
                sandbox: ExternalSandbox::WorkspaceWrite,
                timeout: Duration::from_secs(30),
                authorization: ToolAuthorization::from_user_confirmation(
                    Actor::local(),
                    allow_mutation,
                    confirmed,
                ),
            };
            let error = validate_request(&request)
                .expect_err("incomplete workspace-write authorization must fail");
            assert!(error.contains("mutation authority"));
        }

        let authorized = ExternalAgentRequest {
            harness: "unsupported".into(),
            root: PathBuf::from("."),
            prompt: "edit the failing test".into(),
            sandbox: ExternalSandbox::WorkspaceWrite,
            timeout: Duration::from_secs(30),
            authorization: ToolAuthorization::from_user_confirmation(Actor::local(), true, true),
        };
        assert!(validate_request(&authorized).is_ok());
    }

    #[test]
    fn public_delegate_checks_both_factors_before_resolving_harness() {
        for (allow_mutation, confirmed) in [(false, false), (false, true), (true, false)] {
            let error = delegate(ExternalAgentRequest {
                harness: "unsupported".into(),
                root: std::env::temp_dir(),
                prompt: "edit the failing test".into(),
                sandbox: ExternalSandbox::WorkspaceWrite,
                timeout: Duration::from_secs(30),
                authorization: ToolAuthorization::from_user_confirmation(
                    Actor::local(),
                    allow_mutation,
                    confirmed,
                ),
            })
            .expect_err("direct library delegation must reject incomplete authorization");
            assert!(error.contains("mutation authority"));
        }
        let error = delegate(ExternalAgentRequest {
            harness: "unsupported".into(),
            root: std::env::temp_dir(),
            prompt: "edit the failing test".into(),
            sandbox: ExternalSandbox::WorkspaceWrite,
            timeout: Duration::from_secs(30),
            authorization: ToolAuthorization::from_user_confirmation(Actor::local(), true, true),
        })
        .expect_err("unknown harness should be rejected after authorization");
        assert!(error.contains("unsupported temporary agent"));
    }
}
