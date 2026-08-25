//! One-shot adapters for installed external coding agents.
//!
//! External agents are deliberately temporary: Glass resolves a fixed executable,
//! starts one bounded child process, captures bounded output, and never registers
//! the child as a resident Glass Agent. The default sandbox is read-only.

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
    pub allow_mutation: bool,
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
pub fn delegate(request: ExternalAgentRequest) -> Result<ExternalAgentResult, String> {
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
    let started = Instant::now();
    let mut child = command
        .spawn()
        .map_err(|error| format!("could not start {}: {error}", harness.id()))?;
    let stdout = match child.stdout.take() {
        Some(stdout) => stdout,
        None => {
            let _ = child.kill();
            let _ = child.wait();
            return Err(format!("{} did not expose stdout", harness.id()));
        }
    };
    let stderr = match child.stderr.take() {
        Some(stderr) => stderr,
        None => {
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
        if let Some(status) = child
            .try_wait()
            .map_err(|error| format!("could not poll {}: {error}", harness.id()))?
        {
            break Some(status);
        }
        if started.elapsed() >= request.timeout {
            timed_out = true;
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
    if request.sandbox == ExternalSandbox::WorkspaceWrite && !request.allow_mutation {
        return Err(
            "workspace-write delegation requires explicit mutation authority and confirmation"
                .into(),
        );
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
                prompt,
                "--add-dir".into(),
                root,
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
            args.extend(["--dir".into(), root, prompt]);
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
    }

    #[test]
    fn claude_prompt_precedes_greedy_add_dir_arguments() {
        let invocation = build_invocation(
            ExternalHarness::Claude,
            PathBuf::from("/bin/claude"),
            Path::new("/tmp/project with spaces"),
            "inspect && do not mutate",
            ExternalSandbox::ReadOnly,
        );
        assert!(
            invocation
                .args
                .windows(2)
                .any(|pair| pair[0] == "inspect && do not mutate" && pair[1] == "--add-dir")
        );
        assert_eq!(
            invocation.args.last(),
            Some(&OsString::from("/tmp/project with spaces"))
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
    #[test]
    fn timeout_must_be_at_least_one_second() {
        let request = ExternalAgentRequest {
            harness: "codex".into(),
            root: PathBuf::from("."),
            prompt: "inspect the project".into(),
            sandbox: ExternalSandbox::ReadOnly,
            timeout: Duration::from_millis(999),
            allow_mutation: false,
        };
        assert!(
            validate_request(&request)
                .expect_err("subsecond timeouts must fail")
                .contains("between 1 second and 1 hour")
        );
    }

    #[test]
    fn workspace_write_requires_authority() {
        let request = ExternalAgentRequest {
            harness: "codex".into(),
            root: PathBuf::from("."),
            prompt: "edit the failing test".into(),
            sandbox: ExternalSandbox::WorkspaceWrite,
            timeout: Duration::from_secs(30),
            allow_mutation: false,
        };
        assert!(
            validate_request(&request)
                .expect_err("workspace writes without authority must fail")
                .contains("explicit mutation authority")
        );
    }
}
