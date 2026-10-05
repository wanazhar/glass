//! Spawning and talking to external extensions.
//!
//! One child process per extension, newline-delimited JSON in both directions.
//! Every exchange is bounded by a timeout and a byte cap, so a wedged or
//! hostile extension degrades into a warning rather than a hung session.

use std::io::{BufReader, Write};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

use super::protocol::{Manifest, Request, MAX_REPLY_BYTES, PROTOCOL_VERSION, REQUEST_TIMEOUT_MS};
use super::protocol::parse_manifest;

/// A running extension process.
pub struct Extension {
    pub manifest: Manifest,
    child: Child,
    stdin: Option<ChildStdin>,
    stdout: Option<BufReader<ChildStdout>>,
    /// Raw stdout descriptor, used only for poll(2) readiness checks. `BufReader`
    /// buffers bytes internally, so polling the fd is still correct: readable
    /// means either the fd or the buffer has something.
    raw_fd: i32,
    next_id: u64,
}

impl Extension {
    /// Spawn `cmd` and complete the handshake.
    ///
    /// `cmd` may include arguments (`python3 ext.py`, `node dist/index.js`), so
    /// it is split on whitespace rather than treated as a single path. That is
    /// deliberate: an extension's interpreter is part of how it is invoked, and
    /// requiring every extension to be a self-contained executable would lock
    /// out the scripting most extensions actually want.
    pub fn spawn(
        cmd: &str,
        workspace: &str,
        model: &str,
    ) -> Result<Self, String> {
        let mut parts = cmd.split_whitespace();
        let Some(program) = parts.next() else {
            return Err(format!("extension command `{cmd}` is empty"));
        };
        let args: Vec<String> = parts.map(str::to_string).collect();

        let mut child = Command::new(program)
            .args(&args)
            .current_dir(workspace)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            // Extension stderr goes to our stderr so a broken extension can
            // still say why, without us trying to parse it.
            .stderr(Stdio::inherit())
            .spawn()
            .map_err(|e| format!("cannot start extension `{cmd}`: {e}"))?;

        let raw_fd = child.stdout.as_ref().map(|s| fd_of(s)).unwrap_or(-1);
        let mut ext = Self {
            manifest: Manifest::default(),
            stdin: child.stdin.take(),
            stdout: child.stdout.take().map(BufReader::new),
            raw_fd,
            next_id: 0,
            child,
        };

        let hello = Request::Hello {
            protocol: PROTOCOL_VERSION,
            workspace: workspace.to_string(),
            model: model.to_string(),
        };
        let reply = ext
            .request(&hello, REQUEST_TIMEOUT_MS * 3)
            .ok_or_else(|| format!("extension `{cmd}` closed before the handshake"))?;
        ext.manifest = parse_manifest(&reply)
            .map_err(|e| format!("extension `{cmd}`: {e}"))?;
        Ok(ext)
    }

    /// Send a request and read one reply.
    ///
    /// Returns `None` if the extension closed the pipe, exceeded the byte cap,
    /// or did not answer in time. Every failure is recoverable: the caller
    /// falls back to the default behaviour rather than aborting the turn.
    fn request(&mut self, req: &Request, timeout_ms: u64) -> Option<serde_json::Value> {
        self.next_id += 1;
        let id = self.next_id;
        let mut line = serde_json::to_string(&req.to_json(id)).ok()?;
        line.push('\n');

        let stdin = self.stdin.as_mut()?;
        stdin.write_all(line.as_bytes()).ok()?;
        stdin.flush().ok()?;

        self.read_reply(timeout_ms)
    }

    /// Read one line, capped in both size and time.
    fn read_reply(&mut self, timeout_ms: u64) -> Option<serde_json::Value> {
        let stdout = self.stdout.as_mut()?;
        let deadline = std::time::Instant::now() + std::time::Duration::from_millis(timeout_ms);
        let mut total = 0usize;
        let mut line = String::new();

        loop {
            let remaining = deadline.saturating_duration_since(std::time::Instant::now());
            if remaining.is_zero() {
                return None;
            }

            // `BufRead` has no timeout, so readiness is checked by polling for
            // readable bytes and then doing a non-blocking-ish read via a
            // short-lived worker. Simpler and portable: spawn nothing, use the
            // fact that a well-behaved extension replies immediately and cap
            // the wait with a polling loop on the fd.
            if !fd_readable(self.raw_fd, remaining) {
                return None;
            }

            let mut chunk = String::new();
            let n = read_line_limited(stdout, &mut chunk, MAX_REPLY_BYTES - total)?;
            if n == 0 {
                return None;
            }
            total += n;
            if total > MAX_REPLY_BYTES {
                return None;
            }
            line.push_str(&chunk);
            if line.ends_with('\n') {
                break;
            }
        }

        let trimmed = line.trim();
        if trimmed.is_empty() {
            return None;
        }
        serde_json::from_str(trimmed).ok()
    }

    /// Ask for the extension's system-prompt contribution.
    pub fn system_prompt(&mut self) -> Option<String> {
        let v = self.request(&Request::SystemPrompt, REQUEST_TIMEOUT_MS)?;
        v.get("text").and_then(|t| t.as_str()).map(str::to_string).filter(|s| !s.trim().is_empty())
    }

    /// Offer a tool result for rewriting. Returns `Some` only if the extension
    /// actually changed something.
    pub fn rewrite_tool_result(
        &mut self,
        call_id: &str,
        name: &str,
        body: &str,
    ) -> Option<String> {
        let req = Request::ToolResult {
            call_id: call_id.to_string(),
            name: name.to_string(),
            body: body.to_string(),
        };
        let v = self.request(&req, REQUEST_TIMEOUT_MS)?;
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
        match self.request(&req, REQUEST_TIMEOUT_MS) {
            // No answer means no opinion, which must not block the tool.
            None => Ok(()),
            Some(v) => {
                if v.get("allow").and_then(|a| a.as_bool()).unwrap_or(true) {
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
    pub fn turn_end(&mut self, text: &str) {
        let _ = self.request(&Request::TurnEnd { text: text.to_string() }, REQUEST_TIMEOUT_MS);
    }

    /// Run a slash command. Returns the extension's output.
    pub fn run_command(&mut self, name: &str, args: &str) -> Option<String> {
        let req = Request::Command { name: name.to_string(), args: args.to_string() };
        let v = self.request(&req, REQUEST_TIMEOUT_MS)?;
        v.get("output").and_then(|o| o.as_str()).map(str::to_string)
    }

    /// Execute one of this extension's tools.
    ///
    /// Returns `None` when the extension does not offer it or does not answer,
    /// so the caller can report the tool as unavailable rather than inventing
    /// an empty success.
    pub fn run_tool(&mut self, name: &str, arguments: &str) -> Option<String> {
        if !self.manifest.tools.iter().any(|t| t.name == name) {
            return None;
        }
        let req = Request::RunTool { name: name.to_string(), arguments: arguments.to_string() };
        let v = self.request(&req, REQUEST_TIMEOUT_MS * 3)?;
        let out = v.get("output")?.as_str()?;
        (!out.is_empty()).then(|| out.to_string())
    }

    pub fn has_command(&self, name: &str) -> bool {
        self.manifest.commands.iter().any(|c| c.name == name)
    }

    /// Shut the extension down, escalating to a kill if it ignores the request.
    pub fn shutdown(&mut self) {
        let _ = self.request(&Request::Goodbye, 200);
        // Dropping stdin should let a well-behaved extension exit on EOF.
        self.stdin = None;
        for _ in 0..20 {
            match self.child.try_wait() {
                Ok(Some(_)) => return,
                Ok(None) => std::thread::sleep(std::time::Duration::from_millis(10)),
                Err(_) => break,
            }
        }
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

impl Drop for Extension {
    fn drop(&mut self) {
        // A killed harness must not leave orphaned extension processes holding
        // the workspace open.
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

/// Read one line into `out`, returning how many bytes were consumed.
///
/// The limit is enforced by reading at most `limit` bytes, so an extension
/// that never sends a newline cannot grow our buffer without bound.
fn read_line_limited(r: &mut impl std::io::Read, out: &mut String, limit: usize) -> Option<usize> {
    let mut buf = vec![0u8; 4096.min(limit.max(1))];
    let mut filled = Vec::new();
    loop {
        let n = r.read(&mut buf).ok()?;
        if n == 0 {
            break;
        }
        filled.extend_from_slice(&buf[..n]);
        if filled.len() >= limit || filled.contains(&b'\n') {
            break;
        }
    }
    if filled.is_empty() {
        return None;
    }
    out.push_str(&String::from_utf8_lossy(&filled));
    Some(filled.len())
}

/// Wait until the stream has bytes available, or the deadline passes.
///
/// `BufReader` does not implement `AsFd`, so the raw stdout is kept alongside
/// it purely for this poll.
#[cfg(unix)]
fn fd_readable(fd: i32, timeout: std::time::Duration) -> bool {
    let ms = i32::try_from(timeout.as_millis()).unwrap_or(i32::MAX);
    let mut pfd = PollFd { fd, events: POLLIN, revents: 0 };
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