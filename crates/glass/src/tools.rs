//! The two tools glass ships: `bash` and `state`.
//!
//! Everything else the model needs is a shell one-liner away, and every tool
//! we do not ship is schema the model pays for on every single request.

use std::io::{Read, Write};
use std::os::unix::process::CommandExt;
use std::path::PathBuf;
use std::process::{Child, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};

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
    /// Where the full output was written, when it was too big to inline.
    pub spill: Option<PathBuf>,
}

impl Outcome {
    fn ok(body: String) -> Self {
        Self {
            body,
            truncated: false,
            exit_code: Some(0),
            spill: None,
        }
    }

    /// A tool that did not run, with a message for the model.
    pub fn err(msg: impl Into<String>) -> Self {
        Self {
            body: msg.into(),
            truncated: false,
            exit_code: None,
            spill: None,
        }
    }
}

/// Run `call` and capture its output.
///
/// Deliberately not a permission gate: glass ships no approval layer, and the
/// `Mod::authorize` hook exists for anyone who wants one.
pub fn bash(call: &ToolCall, workspace: &std::path::Path) -> Outcome {
    bash_cancellable(call, workspace, &std::sync::atomic::AtomicBool::new(false))
}
pub fn bash_cancellable(
    call: &ToolCall,
    workspace: &std::path::Path,
    cancel: &std::sync::atomic::AtomicBool,
) -> Outcome {
    let Some(command) = call.arg_str("command") else {
        return Outcome::err("error: `command` is required and must be a string".to_string());
    };
    if command.trim().is_empty() {
        return Outcome::err("error: `command` is empty".to_string());
    }
    if let Some(reason) = crate::sandbox::refusal() {
        return Outcome::err(format!(
            "error: sandbox = strict is not available ({reason}); refusing to run commands unsandboxed"
        ));
    }

    let cwd = match call.arg_str("cwd") {
        Some(rel) if !rel.trim().is_empty() => {
            let joined = workspace.join(rel.trim());
            if joined.is_dir() {
                joined
            } else {
                workspace.to_path_buf()
            }
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

    let mut builder = crate::sandbox::shell(&command);
    builder
        .current_dir(&cwd)
        // Put the child in its own process group. Killing only the shell
        // leaves `sleep`, `cargo`, and `npm` running: the shell exits but its
        // children keep the stdout pipe open, so we would block on the drain
        // threads until they finished on their own. Signalling the group
        // terminates the whole tree, which is what a timeout has to mean.
        .process_group(0)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    // Kernel-level confinement, when a policy is active: applied after fork
    // and before exec so the shell and everything it spawns inherits it.
    crate::sandbox::harden(&mut builder);
    let mut child = match builder.spawn() {
        Ok(c) => c,
        Err(e) => {
            return Outcome {
                body: format!("error: could not spawn shell: {e}"),
                truncated: false,
                exit_code: None,
                spill: None,
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
    let mut cancelled = false;
    let status = loop {
        match child.try_wait() {
            Ok(Some(s)) => break Some(s),
            Ok(None) => {
                if cancel.load(std::sync::atomic::Ordering::Acquire) {
                    kill_group(&mut child);
                    cancelled = true;
                    break child.wait().ok();
                }
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

    let (out, out_spill) = out_buf.join().unwrap_or_else(|_| (String::new(), None));
    let (err, err_spill) = err_buf.join().unwrap_or_else(|_| (String::new(), None));

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

    // A stream that overran the cap left a file behind. Point the model at it
    // rather than discarding the remainder: losing the tail of a build log or a
    // search result is exactly the information the model came for.
    let spill = out_spill.or(err_spill);
    if let Some(p) = &spill {
        body.push_str(&format!(
            "\n[output truncated at {MAX_OUTPUT_BYTES} bytes; full output saved to {} — \
             read it with `head`, `tail`, or `rg`]\n",
            p.display()
        ));
    }
    if cancelled {
        body.push_str("\n[command cancelled; owned process group stopped]\n");
    }
    if timed_out {
        body.push_str(&format!("\n[command killed after {timeout}ms]\n"));
    }
    if body.is_empty() {
        body.push_str("(no output)");
    }

    // Exit code first, always. Interface research is unambiguous that a model
    // without it will re-run commands "to determine the effect of the prior
    // action" — the retry loops show up as duplicated commands in transcripts.
    let code = status.and_then(|s| s.code());
    let mut prefix = match code {
        Some(0) => "[exit 0]\n".to_string(),
        Some(c) => format!("[exit {c}]\n"),
        // Killed by a signal rather than exiting: name it, since the model
        // cannot infer it from output alone.
        None => "[killed by signal]\n".to_string(),
    };
    prefix.push_str(&body);

    Outcome {
        truncated: spill.is_some(),
        body: prefix,
        exit_code: code,
        spill,
    }
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

/// Read a stream, keeping the first [`MAX_OUTPUT_BYTES`] inline and spilling the
/// rest to a temp file.
///
/// Truncating without a spill throws away the part a build log or a search
/// sweep actually ends with, which is the part that says what went wrong. With
/// a spill the model can always recover the tail with `tail` or `rg` instead of
/// re-running the command. Temp files are the OS's to reap, so a crashed session
/// leaves nothing behind that needs cleaning up.
fn read_capped(r: &mut Option<impl Read>) -> (String, Option<PathBuf>) {
    let Some(reader) = r.as_mut() else {
        return (String::new(), None);
    };

    let mut head: Vec<u8> = Vec::new();
    let mut spill: Option<(std::fs::File, PathBuf)> = None;
    let mut chunk = [0u8; 8192];

    loop {
        let n = match reader.read(&mut chunk) {
            Ok(0) | Err(_) => break,
            Ok(n) => n,
        };

        let room = MAX_OUTPUT_BYTES.saturating_sub(head.len());
        if room >= n {
            head.extend_from_slice(&chunk[..n]);
            continue;
        }
        // This chunk straddles the cap: fill the inline budget, spill the rest.
        head.extend_from_slice(&chunk[..room]);
        spill = spill.or_else(open_spill);
        if let Some((f, _)) = spill.as_mut() {
            let _ = f.write_all(&chunk[room..]);
        }
    }

    let path = spill.map(|(mut f, p)| {
        let _ = f.flush();
        p
    });

    (String::from_utf8_lossy(&head).into_owned(), path)
}

fn spill_dir() -> PathBuf {
    crate::config::data_dir().join("output")
}

/// Open a spill file, returning it with the path we will report to the model.
///
/// Returns `None` when the temp dir is unusable, which degrades to plain
/// truncation rather than failing the command.
fn open_spill() -> Option<(std::fs::File, PathBuf)> {
    let dir = spill_dir();
    std::fs::create_dir_all(&dir).ok()?;

    // pid plus a counter keeps concurrent commands from colliding.
    static SEQ: AtomicU64 = AtomicU64::new(0);
    let n = SEQ.fetch_add(1, Ordering::Relaxed);
    let path = dir.join(format!("out-{}-{n}.log", std::process::id()));

    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let f = options.open(&path).ok()?;
    Some((f, path))
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
                let cleaned: Vec<String> = items
                    .into_iter()
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .collect();
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
                spill: None,
            },
        }
    }

    fn render(&self) -> String {
        if self.items.is_empty() {
            return "(no state recorded)".to_string();
        }
        self.items
            .iter()
            .map(|s| format!("- {s}"))
            .collect::<Vec<_>>()
            .join("\n")
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
        ToolCall {
            id: "t1".into(),
            name: "bash".into(),
            arguments: args.to_string(),
        }
    }

    #[test]
    fn runs_a_simple_command() {
        let out = bash(
            &call(r#"{"command":"echo hello"}"#),
            std::path::Path::new("."),
        );
        // The result always leads with the exit code so the model never has to
        // guess whether a command worked.
        assert_eq!(out.body, "[exit 0]\nhello\n");
        assert_eq!(out.exit_code, Some(0));
        assert!(!out.truncated);
    }

    #[test]
    fn nonzero_exit_is_reported_in_the_body() {
        let out = bash(
            &call(r#"{"command":"echo bad >&2; exit 2"}"#),
            std::path::Path::new("."),
        );
        assert!(out.body.starts_with("[exit 2]"), "{}", out.body);
        assert_eq!(out.exit_code, Some(2));
    }

    #[test]
    fn successful_command_with_no_output_is_unambiguous() {
        // A silent success previously read identically to "nothing happened",
        // which is what makes models re-run commands to find out.
        let out = bash(&call(r#"{"command":"true"}"#), std::path::Path::new("."));
        assert_eq!(out.body, "[exit 0]\n(no output)");
    }

    #[test]
    fn captures_stderr() {
        let out = bash(
            &call(r#"{"command":"echo oops >&2"}"#),
            std::path::Path::new("."),
        );
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
        assert!(
            started.elapsed() < std::time::Duration::from_secs(3),
            "took {:?}",
            started.elapsed()
        );
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
        assert_eq!(out.body, "[exit 0]\nok\n");
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
        assert!(
            started.elapsed() < std::time::Duration::from_secs(3),
            "took {:?}",
            started.elapsed()
        );
    }

    #[test]
    fn honours_cwd() {
        let dir = std::env::temp_dir().join("glass-bash-cwd-test");
        std::fs::create_dir_all(&dir).unwrap();
        let out = bash(&call(r#"{"command":"pwd","cwd":"."}"#), &dir);
        // Compare canonically: macOS temp dirs are symlinked via /private.
        let path = out
            .body
            .trim()
            .strip_prefix("[exit 0]")
            .unwrap_or(out.body.trim());
        let got = std::fs::canonicalize(path.trim()).unwrap_or_else(|_| path.trim().into());
        let want = std::fs::canonicalize(&dir).unwrap();
        assert_eq!(got, want);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn large_output_spills_to_disk_and_keeps_the_tail() {
        let out = bash(
            &call(r#"{"command":"seq 1 40000"}"#),
            std::path::Path::new("."),
        );
        assert!(out.truncated, "expected truncation");
        let spill = out.spill.as_ref().expect("overflow must spill to a file");
        let msg = std::fs::read_to_string(spill).expect("spill file must exist");
        assert!(
            msg.contains("40000"),
            "the last line must be recoverable from the spill file"
        );
        // The inline body points the model at the file.
        assert!(out.body.contains("saved to"), "{}", out.body);
        assert!(
            out.body.len() < MAX_OUTPUT_BYTES + 400,
            "body grew to {}",
            out.body.len()
        );
        let _ = std::fs::remove_file(spill);
    }

    #[test]
    fn output_under_the_cap_does_not_spill() {
        let out = bash(
            &call(r#"{"command":"echo small"}"#),
            std::path::Path::new("."),
        );
        assert!(!out.truncated);
        assert!(out.spill.is_none());
    }

    #[test]
    fn spill_contains_exactly_the_overflow() {
        // The inline head and the spill file must partition the stream, with no
        // bytes lost at the boundary.
        let out = bash(
            &call(r#"{"command":"seq 1 40000"}"#),
            std::path::Path::new("."),
        );
        let spill = out.spill.as_ref().unwrap();
        let tail = std::fs::read_to_string(spill).unwrap();
        let head_lines = out
            .body
            .lines()
            .filter(|l| l.chars().all(|c| c.is_ascii_digit()) && !l.is_empty())
            .count();
        assert!(head_lines > 0, "head should carry some lines");
        assert!(tail.contains("40000"));
        // 40000 lines total; head + spill must account for all of them.
        let spill_lines = tail.lines().filter(|l| !l.trim().is_empty()).count();
        assert!(
            head_lines + spill_lines >= 39999,
            "head {head_lines} + spill {spill_lines} lost lines"
        );
        let _ = std::fs::remove_file(spill);
    }

    #[test]
    fn state_records_and_reads_back() {
        let mut s = State::new();
        let c = ToolCall {
            id: "t".into(),
            name: "state".into(),
            arguments: r#"{"items":["a is b","c is d"]}"#.into(),
        };
        s.apply(&c);
        assert_eq!(s.items().len(), 2);
        let read = ToolCall {
            id: "t".into(),
            name: "state".into(),
            arguments: "{}".into(),
        };
        let out = s.apply(&read);
        assert!(out.body.contains("a is b"));
        assert!(out.body.contains("c is d"));
    }

    #[test]
    fn state_rejects_non_array_items() {
        let mut s = State::new();
        let c = ToolCall {
            id: "t".into(),
            name: "state".into(),
            arguments: r#"{"items":"nope"}"#.into(),
        };
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
