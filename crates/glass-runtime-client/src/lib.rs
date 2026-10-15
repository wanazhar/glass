//! Host-neutral client for `glass-core runtime`.
//!
//! The host owns tools, approval UI, child lifetime, and event rendering. Calls
//! are asynchronous: send a request, then poll messages while the turn runs.
//! The runtime is local trusted code, not a sandbox or an authenticated server.
pub use serde_json::{Value, json};
use std::io::{self, BufRead, BufReader, Read, Write};
use std::path::Path;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::time::Duration;

pub const PROTOCOL_VERSION: u64 = 1;
pub const MAX_FRAME_BYTES: usize = 1024 * 1024;

/// A single runtime child. Drop terminates and reaps the owned process.
/// Use `shutdown` for graceful closure after turns settle.
pub struct RuntimeClient {
    child: Child,
    input: ChildStdin,
    output: Receiver<io::Result<Value>>,
    next_id: u64,
}
impl RuntimeClient {
    /// Start without a shell. Provider configuration is inherited from the
    /// normal GC environment; initialization chooses isolated session storage.
    pub fn spawn(binary: impl AsRef<Path>, cwd: impl AsRef<Path>) -> io::Result<Self> {
        let mut command = Command::new(binary.as_ref());
        command
            .arg("runtime")
            .current_dir(cwd)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit());
        Self::spawn_command(command)
    }
    /// Supply environment overrides explicitly when a host needs a separate
    /// GC configuration or credential directory.
    pub fn spawn_command(mut command: Command) -> io::Result<Self> {
        command.stdin(Stdio::piped()).stdout(Stdio::piped());
        let mut child = command.spawn()?;
        let input = child
            .stdin
            .take()
            .ok_or_else(|| io::Error::other("runtime stdin unavailable"))?;
        let stream = child
            .stdout
            .take()
            .ok_or_else(|| io::Error::other("runtime stdout unavailable"))?;
        let (tx, output) = mpsc::sync_channel(64);
        std::thread::spawn(move || {
            let mut reader = BufReader::new(stream);
            loop {
                let result = read_frame(&mut reader);
                let finished = result.is_err();
                if tx.send(result).is_err() || finished {
                    break;
                }
            }
        });
        Ok(Self {
            child,
            input,
            output,
            next_id: 1,
        })
    }
    /// Send a request without waiting for its response. The returned ID matches
    /// the eventual response; events may arrive first.
    pub fn request(&mut self, method: &str, params: Value) -> io::Result<String> {
        let id = format!("host-{}", self.next_id);
        self.next_id = self
            .next_id
            .checked_add(1)
            .ok_or_else(|| io::Error::other("request ID exhausted"))?;
        self.send(json!({"jsonrpc":"2.0","id":id,"method":method,"params":params}))?;
        Ok(id)
    }
    /// Answer an exact runtime tool request after applying host policy.
    /// A denied approval is an error result, not permission to execute.
    pub fn tool_result(&mut self, id: Value, text: &str, is_error: bool) -> io::Result<()> {
        self.send(json!({"jsonrpc":"2.0","id":id,"result":{"text":text,"isError":is_error}}))
    }
    /// Receive events, tool requests, or correlated responses. None means the
    /// timeout elapsed; EOF and malformed frames are errors.
    pub fn recv_timeout(&self, timeout: Duration) -> io::Result<Option<Value>> {
        match self.output.recv_timeout(timeout) {
            Ok(v) => v.map(Some),
            Err(RecvTimeoutError::Timeout) => Ok(None),
            Err(RecvTimeoutError::Disconnected) => Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "runtime disconnected",
            )),
        }
    }
    pub fn send(&mut self, value: Value) -> io::Result<()> {
        let bytes = serde_json::to_vec(&value)?;
        if bytes.len() + 1 > MAX_FRAME_BYTES {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "runtime frame exceeds 1 MiB",
            ));
        }
        self.input.write_all(&bytes)?;
        self.input.write_all(b"\n")?;
        self.input.flush()
    }
    pub fn pid(&self) -> u32 {
        self.child.id()
    }
}
impl Drop for RuntimeClient {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
fn read_frame(reader: &mut impl BufRead) -> io::Result<Value> {
    let mut line = Vec::new();
    let n = reader
        .by_ref()
        .take((MAX_FRAME_BYTES + 1) as u64)
        .read_until(b'\n', &mut line)?;
    if n == 0 {
        return Err(io::Error::new(
            io::ErrorKind::UnexpectedEof,
            "runtime stdout closed",
        ));
    }
    if n > MAX_FRAME_BYTES {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "runtime frame exceeds 1 MiB",
        ));
    }
    let value: Value = serde_json::from_slice(&line)?;
    if !value.is_object() || value["jsonrpc"] != "2.0" {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "invalid JSON-RPC envelope",
        ));
    }
    Ok(value)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn bounded_frames() {
        assert!(read_frame(&mut &b"{\"jsonrpc\":\"2.0\",\"id\":1,\"result\":{}}\n"[..]).is_ok());
        assert!(read_frame(&mut &b"[]\n"[..]).is_err());
        assert!(read_frame(&mut &b""[..]).is_err());
        assert!(read_frame(&mut &vec![b' '; MAX_FRAME_BYTES + 1][..]).is_err());
    }
}

pub mod protocol;
pub use protocol::{Incoming, SessionOptions, ToolDefinition, ToolMode, ToolResult, TurnInput};

impl RuntimeClient {
    pub fn initialize(&mut self, storage: impl AsRef<Path>) -> io::Result<String> {
        self.request(
            "initialize",
            json!({"protocolVersion":PROTOCOL_VERSION,"storageDir":storage.as_ref()}),
        )
    }
    pub fn new_session(&mut self, options: &SessionOptions) -> io::Result<String> {
        self.request("session/new", serde_json::to_value(options)?)
    }
    pub fn resume_session(&mut self, session: &str, expected: ToolMode) -> io::Result<String> {
        self.request(
            "session/resume",
            json!({"sessionId":session,"expectedToolMode":expected}),
        )
    }
    pub fn prompt(&mut self, session: &str, input: &TurnInput) -> io::Result<String> {
        let mut params = serde_json::to_value(input)?;
        params["sessionId"] = json!(session);
        self.request("turn/prompt", params)
    }
    pub fn entries(&mut self, session: &str, cursor: u64, limit: u32) -> io::Result<String> {
        self.request(
            "session/entries",
            json!({"sessionId":session,"cursor":cursor,"limit":limit}),
        )
    }
    pub fn structured_result(&mut self, id: Value, result: &ToolResult) -> io::Result<()> {
        self.send(json!({"jsonrpc":"2.0","id":id,"result":result}))
    }
    pub fn progress(&mut self, session: &str, request: Value, text: &str) -> io::Result<()> {
        self.send(json!({"jsonrpc":"2.0","method":"tool/progress","params":{"sessionId":session,"requestId":request,"text":text}}))
    }
    pub fn acknowledge_cancel(
        &mut self,
        session: &str,
        request: Value,
        status: &str,
    ) -> io::Result<String> {
        self.request(
            "tool/cancelled",
            json!({"sessionId":session,"requestId":request,"executionStatus":status}),
        )
    }
    pub fn recv_typed_timeout(&self, timeout: Duration) -> io::Result<Option<Incoming>> {
        self.recv_timeout(timeout)?
            .map(|v| Incoming::parse(v).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e)))
            .transpose()
    }
}

impl RuntimeClient {
    pub fn steer(&mut self, session: &str, input: &TurnInput) -> io::Result<String> {
        let mut p = serde_json::to_value(input)?;
        p["sessionId"] = json!(session);
        self.request("turn/steer", p)
    }
    pub fn follow_up(&mut self, session: &str, input: &TurnInput) -> io::Result<String> {
        let mut p = serde_json::to_value(input)?;
        p["sessionId"] = json!(session);
        self.request("turn/followUp", p)
    }
    pub fn cancel(&mut self, session: &str, turn: Value) -> io::Result<String> {
        self.request("turn/cancel", json!({"sessionId":session,"turnId":turn}))
    }
    pub fn complete(
        &mut self,
        session: &str,
        prefix: &str,
        suffix: &str,
        max_tokens: u32,
    ) -> io::Result<String> {
        self.request(
            "completion/request",
            json!({"sessionId":session,"prefix":prefix,"suffix":suffix,"maxTokens":max_tokens}),
        )
    }
    pub fn models(&mut self, session: &str, cursor: u32) -> io::Result<String> {
        self.request("models/list", json!({"sessionId":session,"cursor":cursor}))
    }
    pub fn reconcile(
        &mut self,
        session: &str,
        request: Value,
        status: &str,
        text: &str,
    ) -> io::Result<String> {
        self.request(
            "tool/reconcile",
            json!({"sessionId":session,"requestId":request,"executionStatus":status,"text":text}),
        )
    }
}
