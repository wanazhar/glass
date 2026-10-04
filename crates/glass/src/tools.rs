//! The two tools glass ships: `bash` and `state`.
//!
//! Everything else the model needs is a shell one-liner away, and every tool
//! we do not ship is schema the model pays for on every single request.

use std::io::Read;
use std::os::unix::process::CommandExt;
use std::process::{Child, Command, Stdio};

use glass_mod_api::ToolCall;

// Process signalling, without pulling in a dependency for two constants and
// one call. Only used on the timeout path.
const SIGTERM: i32 = 15;
const SIGKILL: i32 = 9;

unsafe extern "C" {
    fn kill(pid: i32, sig: i32) -> i32;
}

/// Hard ceiling on captured output, per command.
///
/// Tool output dominates context growth. An uncaptured `cat` of a large file
/// can add more tokens than the entire conversation, so truncation is not an
/// optimisation here, it is the difference between a session that survives and
/// one that needs compaction every few turns.
pub const MAX_OUTPUT_BYTES: usize = 60_000;

/// Default and maximum command timeout.
pub const DEFAULT_TIMEOUT_MS: u64 = 120_000;
pub const MAX_TIMEOUT_MS: u64 = 900_000;

pub struct Outcome {
    pub body: String,
    /// Bytes dropped by truncation, reported to the model so it knows the
    /// output it sees is partial rather than complete.
    pub truncated: bool,
    pub exit_code: Option<i32>,
}

impl Outcome {
    fn ok(body: String) -> Self {
        Self { body, truncated: false, exit_code: Some(0) }
    }
}

/// Run `call` and capture its output.
///
/// Deliberately not a permission gate: glass ships no approval layer, and the
/// `Mod::authorize` hook exists for anyone who wants one.
pub fn bash(call: &ToolCall, workspace: &std::path::Path) -> Outcome {
    let Some(command) = call.arg_str("command") else {
        return Outcome {
            body: "error: `command` is required and must be a string".to_string(),
            truncated: false,
            exit_code: None,
        };
    };
    if command.trim().is_empty() {
        return Outcome { body: "error: `command` is empty".to_string(), truncated: false, exit_code: None };
    }

    let cwd = match call.arg_str("cwd") {
        Some(rel) if !rel.trim().is_empty() => {
            let joined = workspace.join(rel.trim());
            if joined.is_dir() { joined } else { workspace.to_path_buf() }
        }
        _ => workspace.to_path_buf(),
    };

    // Models send `timeout_ms` as a JSON number, not a string, so this has to
    // read the raw value. Using `arg_str` here silently returns None and every
    // command would quietly get the default timeout.
    let timeout = call
        .arg_raw("timeout_ms")
        .and_then(|raw| {
            raw.trim_matches('"')
                .parse::<f64>()
                .ok()
                .filter(|v| v.is_finite() && *v > 0.0)
                .map(|v| v as u64)
        })
        .unwrap_or(DEFAULT_TIMEOUT_MS)
        .clamp(100, MAX_TIMEOUT_MS);

    let mut child = match Command::new("/bin/sh")
        .arg("-c")
        .arg(&command)
        .current_dir(&cwd)
        // Put the child in its own process group. Killing only the shell
        // leaves `sleep`, `cargo`, and `npm` running: the shell exits but its
        // children keep the stdout pipe open, so we would block on the drain
        // threads until they finished on their own. Signalling the group
        // terminates the whole tree, which is what a timeout has to mean.
        .process_group(0)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
    {
        Ok(c) => c,
        Err(e) => {
            return Outcome {
                body: format!("error: could not spawn shell: {e}"),
                truncated: false,
                exit_code: None,
            };
        }
    };

    // Drain both pipes concurrently. Reading stdout to completion before
    // stderr will deadlock as soon as the child fills the stderr pipe buffer,
    // which `cargo build` and `git push` both do routinely.
    let mut stdout = child.stdout.take();
    let mut stderr = child.stderr.take();
    let out_buf = std::thread::spawn(move || read_capped(&mut stdout));
    let err_buf = std::thread::spawn(move || read_capped(&mut stderr));

    let deadline = std::time::Instant::now() + std::time::Duration::from_millis(timeout);
    let mut timed_out = false;
    let status = loop {
        match child.try_wait() {
            Ok(Some(s)) => break Some(s),
            Ok(None) => {
                if std::time::Instant::now() >= deadline {
                    kill_group(&mut child);
                    timed_out = true;
                    break child.wait().ok();
                }
                // Poll rather than block so the timeout stays responsive.
                std::thread::sleep(std::time::Duration::from_millis(20));
            }
            Err(_) => break None,
        }
    };

    let (out, out_trunc) = out_buf.join().unwrap_or_else(|_| (String::new(), true));
    let (err, err_trunc) = err_buf.join().unwrap_or_else(|_| (String::new(), true));

    let mut body = String::new();
    if !out.is_empty() {
        body.push_str(&out);
        if !body.ends_with('\n') {
            body.push('\n');
        }
    }
    if !err.is_empty() {
        if !body.is_empty() && !body.ends_with('\n') {
            body.push('\n');
        }
        body.push_str("stderr:\n");
        body.push_str(&err);
    }
    let truncated = out_trunc || err_trunc;
    if truncated {
        body.push_str("\n[output truncated at 60000 bytes]\n");
    }
    if timed_out {
        body.push_str(&format!("\n[command killed after {timeout}ms]\n"));
    }
    if body.is_empty() {
        body.push_str("(no output)");
    }

    Outcome { body, truncated, exit_code: status.and_then(|s| s.code()) }
}

/// Signal the child's entire process group.
///
/// The negative pid is what makes this a group signal rather than a signal to
/// one process. Without it the shell dies on its own but its children keep
/// running and keep the stdout pipe open, so a timed-out `cargo build` would
/// still block the harness until it finished on its own.
///
/// SIGTERM first so a well-behaved process can flush output, then SIGKILL for
/// anything that ignored it. The signal is repeated briefly because a child
/// forked just before the shell died may not have been in the group yet.
fn kill_group(child: &mut Child) {
    // Guard against pid 0 or 1: a negative group id of 0 means "our own
    // group" and -1 means "every process we may signal", neither of which is
    // ever intended here.
    let Some(pid) = (child.id() > 1).then_some(child.id()) else {
        return;
    };
    let pgid = -(pid as i32);

    unsafe {
        kill(pgid, SIGTERM);
    }
    std::thread::sleep(std::time::Duration::from_millis(100));
    for _ in 0..4 {
        if child.try_wait().ok().flatten().is_some() {
            break;
        }
        unsafe {
            kill(pgid, SIGKILL);
        }
        std::thread::sleep(std::time::Duration::from_millis(30));
    }
}

/// Read at most [`MAX_OUTPUT_BYTES`], draining the rest so the child never
/// blocks on a full pipe.
fn read_capped(r: &mut Option<impl Read>) -> (String, bool) {
    let Some(reader) = r.as_mut() else { return (String::new(), false) };
    let mut buf = Vec::new();
    let mut chunk = [0u8; 8192];
    let mut truncated = false;
    loop {
        match reader.read(&mut chunk) {
            Ok(0) | Err(_) => break,
            Ok(n) => {
                if buf.len() < MAX_OUTPUT_BYTES {
                    let room = MAX_OUTPUT_BYTES - buf.len();
                    let take = room.min(n);
                    buf.extend_from_slice(&chunk[..take]);
                    if take < n {
                        truncated = true;
                    }
                } else {
                    truncated = true;
                }
            }
        }
    }
    (String::from_utf8_lossy(&buf).into_owned(), truncated)
}

/// Durable working state, written by the model, read by the compactor.
#[derive(Default)]
pub struct State {
    items: Vec<String>,
}

impl State {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn items(&self) -> &[String] {
        &self.items
    }

    /// Handle a `state` tool call. No `items` argument means read-back.
    pub fn apply(&mut self, call: &ToolCall) -> Outcome {
        // Absent or null means read-back, which is how the model recovers
        // state after a compaction without having to guess.
        let raw = match call.arg_raw("items") {
            Some(r) if !r.is_empty() && r != "null" => r,
            _ => return Outcome::ok(self.render()),
        };

        match serde_json::from_str::<Vec<String>>(&raw) {
            Ok(items) => {
                let cleaned: Vec<String> =
                    items.into_iter().map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect();
                if cleaned.is_empty() {
                    self.items.clear();
                    Outcome::ok("state cleared".to_string())
                } else {
                    let n = cleaned.len();
                    self.items = cleaned;
                    Outcome::ok(format!("state recorded ({n} items)"))
                }
            }
            Err(e) => Outcome {
                body: format!("error: `items` must be an array of strings: {e}"),
                truncated: false,
                exit_code: None,
            },
        }
    }

    fn render(&self) -> String {
        if self.items.is_empty() {
            return "(no state recorded)".to_string();
        }
        self.items.iter().map(|s| format!("- {s}")).collect::<Vec<_>>().join("\n")
    }

    /// Render as the system-visible block injected after compaction.
    pub fn as_context(&self) -> Option<String> {
        if self.items.is_empty() {
            return None;
        }
        Some(format!(
            "State recorded earlier in this session (your own notes, keep it accurate):\n{}",
            self.render()
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn call(args: &str) -> ToolCall {
        ToolCall { id: "t1".into(), name: "bash".into(), arguments: args.to_string() }
    }

    #[test]
    fn runs_a_simple_command() {
        let out = bash(&call(r#"{"command":"echo hello"}"#), std::path::Path::new("."));
        assert_eq!(out.body.trim(), "hello");
        assert_eq!(out.exit_code, Some(0));
        assert!(!out.truncated);
    }

    #[test]
    fn captures_stderr() {
        let out = bash(&call(r#"{"command":"echo oops >&2"}"#), std::path::Path::new("."));
        assert!(out.body.contains("stderr:"));
        assert!(out.body.contains("oops"));
    }

    #[test]
    fn reports_exit_code_in_nonzero_case() {
        let out = bash(&call(r#"{"command":"exit 3"}"#), std::path::Path::new("."));
        assert_eq!(out.exit_code, Some(3));
    }

    #[test]
    fn empty_command_is_rejected_without_spawning() {
        let out = bash(&call(r#"{"command":"   "}"#), std::path::Path::new("."));
        assert!(out.body.contains("empty"));
        assert_eq!(out.exit_code, None);
    }

    #[test]
    fn missing_command_is_rejected() {
        let out = bash(&call(r#"{"cwd":"/tmp"}"#), std::path::Path::new("."));
        assert!(out.body.contains("required"));
    }

    #[test]
    fn timeout_kills_the_child() {
        let started = std::time::Instant::now();
        let out = bash(
            &call(r#"{"command":"sleep 5","timeout_ms":150}"#),
            std::path::Path::new("."),
        );
        assert!(out.body.contains("killed"), "{}", out.body);
        assert!(out.exit_code.is_none() || out.exit_code != Some(0));
        // Proves the numeric timeout was honoured rather than defaulted to 120s.
        assert!(started.elapsed() < std::time::Duration::from_secs(3), "took {:?}", started.elapsed());
    }

    #[test]
    fn numeric_timeout_is_honoured_not_defaulted() {
        // Regression: reading a JSON number with the string accessor returned
        // None, so every command silently ran with the 120s default.
        let started = std::time::Instant::now();
        let out = bash(
            &call(r#"{"command":"sleep 30","timeout_ms":200}"#),
            std::path::Path::new("."),
        );
        assert!(out.body.contains("killed"), "{}", out.body);
        assert!(started.elapsed() < std::time::Duration::from_secs(3));
    }

    #[test]
    fn string_timeout_is_also_accepted() {
        let started = std::time::Instant::now();
        let out = bash(
            &call(r#"{"command":"sleep 30","timeout_ms":"200"}"#),
            std::path::Path::new("."),
        );
        assert!(out.body.contains("killed"), "{}", out.body);
        assert!(started.elapsed() < std::time::Duration::from_secs(3));
    }

    #[test]
    fn non_numeric_timeout_falls_back_to_default() {
        // Garbage must not become a 0ms timeout that kills everything instantly.
        let out = bash(
            &call(r#"{"command":"echo ok","timeout_ms":"soon"}"#),
            std::path::Path::new("."),
        );
        assert_eq!(out.body.trim(), "ok");
        assert!(!out.body.contains("killed"));
    }

    #[test]
    fn child_processes_are_killed_with_the_group() {
        // The shell dies instantly but `sleep` would survive without a group
        // signal, holding the stdout pipe open past the timeout.
        let started = std::time::Instant::now();
        let out = bash(
            &call(r#"{"command":"sleep 30 & sleep 30","timeout_ms":200}"#),
            std::path::Path::new("."),
        );
        assert!(out.body.contains("killed"), "{}", out.body);
        assert!(started.elapsed() < std::time::Duration::from_secs(3), "took {:?}", started.elapsed());
    }

    #[test]
    fn honours_cwd() {
        let dir = std::env::temp_dir().join("glass-bash-cwd-test");
        std::fs::create_dir_all(&dir).unwrap();
        let out = bash(&call(r#"{"command":"pwd","cwd":"."}"#), &dir);
        // Compare canonically: macOS temp dirs are symlinked via /private.
        let got = std::fs::canonicalize(out.body.trim()).unwrap_or_else(|_| out.body.trim().into());
        let want = std::fs::canonicalize(&dir).unwrap();
        assert_eq!(got, want);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn large_output_is_truncated() {
        let out = bash(
            &call(r#"{"command":"yes abcdefgh | head -c 200000"}"#),
            std::path::Path::new("."),
        );
        assert!(out.truncated, "expected truncation");
        assert!(out.body.contains("truncated"));
        assert!(out.body.len() < MAX_OUTPUT_BYTES + 200, "body grew to {}", out.body.len());
    }

    #[test]
    fn state_records_and_reads_back() {
        let mut s = State::new();
        let c = ToolCall { id: "t".into(), name: "state".into(), arguments: r#"{"items":["a is b","c is d"]}"#.into() };
        s.apply(&c);
        assert_eq!(s.items().len(), 2);
        let read = ToolCall { id: "t".into(), name: "state".into(), arguments: "{}".into() };
        let out = s.apply(&read);
        assert!(out.body.contains("a is b"));
        assert!(out.body.contains("c is d"));
    }

    #[test]
    fn state_rejects_non_array_items() {
        let mut s = State::new();
        let c = ToolCall { id: "t".into(), name: "state".into(), arguments: r#"{"items":"nope"}"#.into() };
        let out = s.apply(&c);
        assert!(out.body.contains("array of strings"));
        assert!(s.items().is_empty());
    }

    #[test]
    fn state_drops_blank_entries() {
        let mut s = State::new();
        let c = ToolCall {
            id: "t".into(),
            name: "state".into(),
            arguments: r#"{"items":["  real  ","","   "]}"#.into(),
        };
        s.apply(&c);
        assert_eq!(s.items(), &["real".to_string()]);
    }

    #[test]
    fn empty_state_has_no_context_block() {
        assert!(State::new().as_context().is_none());
    }

    #[test]
    fn state_context_renders_items() {
        let mut s = State::new();
        s.apply(&ToolCall {
            id: "t".into(),
            name: "state".into(),
            arguments: r#"{"items":["step 1 done"]}"#.into(),
        });
        let ctx = s.as_context().unwrap();
        assert!(ctx.contains("step 1 done"));
    }
}
