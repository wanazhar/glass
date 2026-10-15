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
