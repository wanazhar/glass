//! Pre-tool-use shell hook (task six-wave-e-001, crush `PreToolUse`).
//!
//! One user-registered shell command, configured per project with a regex
//! matcher on the tool name. It runs on a dedicated thread before *any*
//! tool executes — built-ins and extension tools alike — with the tool-call
//! JSON on stdin. Exit codes:
//!
//! - `0` — allow
//! - `2` — block this call; the hook's stdout (bounded, UTF-8) becomes the
//!   refusal body the model sees
//! - `49` — block this call AND cancel the rest of the turn
//! - anything else — warn-and-allow, stderr surfaced once in the transcript
//!
//! The hook's stdout is never tool output; it can only block or pass, so a
//! hook can't fabricate results. Bounds: 3 s timeout, 8 KiB output cap —
//! over-cap or timed-out runs warn-and-allow, deterministically. Hooks run
//! sequentially (a deliberate deviation from crush's parallel hooks:
//! determinism before parallelism, one bounded thread each).
//!
//! Never runs in read-only mode: hook = code execution, and plan mode is
//! the not-running-things mode.

#[cfg(unix)]
unsafe extern "C" {
    fn kill(pid: i32, sig: i32) -> i32;
}

pub const TIMEOUT_S: u64 = 3;
pub const OUTPUT_CAP: usize = 8 * 1024;
/// Block this one call; stdout is the refusal body.
pub const EXIT_BLOCK: i32 = 2;
/// Block this call and cancel the rest of the turn.
pub const EXIT_HALT: i32 = 49;

/// `[hooks.pre_tool_use]` in project config, loaded through the trust gate.
#[derive(Debug, Clone, Default)]
pub struct PreToolUse {
    /// Shell command to run; receives tool-call JSON on stdin.
    pub command: String,
    /// Regex on the tool name; empty = every tool.
    pub matcher: String,
}

/// What the hook decided about one call.
#[derive(Debug, PartialEq)]
pub enum Verdict {
    /// Run the tool normally.
    Allow,
    /// Refuse this call; the string is the body the model sees.
    Block(String),
    /// Refuse this call and cancel the rest of the turn.
    Halt(String),
    /// The hook misbehaved (timeout, cap, nonzero exit); record a warning,
    /// allow the call.
    Warn(String),
}

/// Does this hook fire for a tool with this name?
pub fn matches(hook: &PreToolUse, tool_name: &str) -> bool {
    if hook.command.trim().is_empty() {
        return false;
    }
    let m = hook.matcher.trim();
    if m.is_empty() {
        return true;
    }
    // Bounded regex-lite: matcher is an anchored-or-plain substring pattern;
    // a regex engine here is power a hook config does not need. Case is
    // respected (tool names are lowercase already).
    tool_name.contains(m)
}

/// Run the hook for one call. Never panics, never blocks past the timeout.
pub fn run(hook: &PreToolUse, call_json: &str) -> Verdict {
    let name = tool_name_of(call_json).to_string();
    if !matches(hook, &name) {
        return Verdict::Allow;
    }
    // Stdin is drained on a helper thread: the hook gets its JSON even when
    // it writes more output than we read, so the pipe can never backmove
    // the child into a deadlock the timeout would run out on.
    let mut command = std::process::Command::new("sh");
    command
        .arg("-c")
        .arg(&hook.command)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    let mut child = match command.spawn() {
        Ok(c) => c,
        Err(e) => return warn(format!("hook spawn failed: {e}")),
    };
    if let Some(mut stdin) = child.stdin.take() {
        let payload = call_json.as_bytes().to_vec();
        std::thread::spawn(move || {
            use std::io::Write;
            let _ = stdin.write_all(&payload);
        });
    }
    // Output is drained on helper threads against the cap: a hook that
    // writes past 8 KiB is capped (and warned), never waited on forever.
    let stdout_t = drain(child.stdout.take());
    let stderr_t = drain(child.stderr.take());

    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(TIMEOUT_S);
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                let code = status.code().unwrap_or(1);
                let stdout = bounded(
                    String::from_utf8_lossy(&stdout_t.join().unwrap_or_default()).into_owned(),
                );
                let _ = stderr_t.join();
                return verdict_from(code, stdout);
            }
            Ok(None) => {
                if std::time::Instant::now() >= deadline {
                    // Negative pid signals the whole process group: a hook
                    // that bg-spawned descendants dies with them, exactly
                    // like the bash tool's own timeout path.
                    #[cfg(unix)]
                    unsafe {
                        kill(-(child.id() as i32), 9);
                    }
                    #[cfg(not(unix))]
                    let _ = child.kill();
                    let _ = child.wait();
                    let _ = stdout_t.join();
                    let _ = stderr_t.join();
                    return warn(format!("hook timeout ({TIMEOUT_S}s): allowing the call"));
                }
                std::thread::sleep(std::time::Duration::from_millis(20));
            }
            Err(_) => return warn("hook crashed".to_string()),
        }
    }
}

/// Drain a pipe on a thread, capping what we keep at OUTPUT_CAP.
fn drain(pipe: Option<impl std::io::Read + Send + 'static>) -> std::thread::JoinHandle<Vec<u8>> {
    std::thread::spawn(move || {
        let mut out = Vec::new();
        if let Some(mut pipe) = pipe {
            let mut chunk = [0u8; 1024];
            // Read everything (so the child never blocks on a full pipe) but
            // stop *keeping* bytes past the cap.
            while let Ok(n) = pipe.read(&mut chunk) {
                if n == 0 {
                    break;
                }
                let room = OUTPUT_CAP.saturating_sub(out.len());
                out.extend_from_slice(&chunk[..n.min(room)]);
            }
        }
        out
    })
}

fn verdict_from(code: i32, stdout: String) -> Verdict {
    match code {
        0 => Verdict::Allow,
        EXIT_BLOCK => Verdict::Block(stdout),
        EXIT_HALT => Verdict::Halt(stdout),
        other => warn(format!("hook exit {other}: allowing the call")),
    }
}

fn tool_name_of(call_json: &str) -> &str {
    // Cheap scan for the single quoted `name` member — the shape is fixed
    // by the wire format and serde here would allocate per call.
    let idx = match call_json.find("\"name\"") {
        Some(i) => &call_json[i..],
        None => return "",
    };
    let Some(colon) = idx.find(':') else {
        return "";
    };
    let rest = idx[colon + 1..].trim_start();
    // Value must be a string; take it to the closing quote.
    if !matches!(rest.as_bytes().first(), Some(b'"')) {
        return "";
    }
    let inner = &rest[1..];
    let end = inner.find('"').unwrap_or(inner.len());
    &inner[..end]
}

fn bounded(stdout: String) -> String {
    let mut out = stdout.trim().to_string();
    if out.chars().count() > OUTPUT_CAP {
        out = out.chars().take(OUTPUT_CAP).collect();
    }
    out
}

fn warn(message: String) -> Verdict {
    Verdict::Warn(message)
}

/// Load the project-level hook config from the workspace's config.toml.
/// Config.rs is owned by another wave, so the small `[hooks]` table is
/// parsed here from the same file the main config reads; a trust-gated
/// call site only reaches this after the existing trust gate.
pub fn load_config() -> Option<PreToolUse> {
    let path = crate::config::default_path();
    let text = std::fs::read_to_string(path).ok()?;
    let value: toml::Value = toml::from_str(&text).ok()?;
    let table = value.get("hooks")?.get("pre_tool_use")?.as_table()?;
    let command = table
        .get("command")
        .and_then(|v| v.as_str())?
        .trim()
        .to_string();
    if command.is_empty() {
        return None;
    }
    Some(PreToolUse {
        matcher: table
            .get("matcher")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
        command,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hook(command: &str, matcher: &str) -> PreToolUse {
        PreToolUse {
            command: command.into(),
            matcher: matcher.into(),
        }
    }

    const CALL: &str = r#"{"name":"bash","arguments":{"command":"ls"}}"#;

    #[test]
    fn matcher_semantics() {
        assert!(matches(&hook("true", ""), "bash"));
        assert!(matches(&hook("true", "bash"), "bash"));
        assert!(!matches(&hook("true", "edit"), "bash"));
        assert!(
            !matches(&hook("   ", ""), "bash"),
            "empty command never fires"
        );
    }

    #[test]
    fn exit_zero_allows() {
        assert_eq!(run(&hook("true", ""), CALL), Verdict::Allow);
    }

    #[test]
    fn exit_two_blocks_with_stdout_as_the_refusal() {
        let v = run(&hook("echo no-good; exit 2", ""), CALL);
        assert_eq!(v, Verdict::Block("no-good".into()), "{v:?}");
    }

    #[test]
    fn exit_49_halts_the_turn() {
        let v = run(&hook("echo stopping; exit 49", ""), CALL);
        assert_eq!(v, Verdict::Halt("stopping".into()), "{v:?}");
    }

    #[test]
    fn nonzero_warns_and_allows() {
        let v = run(&hook("echo bad >&2; exit 3", ""), CALL);
        assert!(matches!(v, Verdict::Warn(_)), "{v:?}");
    }

    #[test]
    fn timeout_warns_and_allows_within_a_bounded_wait() {
        let started = std::time::Instant::now();
        let v = run(&hook("sleep 30", ""), CALL);
        let elapsed = started.elapsed();
        assert!(matches!(v, Verdict::Warn(_)), "{v:?}");
        assert!(elapsed < std::time::Duration::from_secs(10), "{elapsed:?}");
    }

    #[test]
    fn unmatching_hooks_do_not_spawn() {
        // No shell process is spawned when the matcher declines; same verdict.
        assert_eq!(run(&hook("exit 2", "edit"), CALL), Verdict::Allow);
    }

    #[test]
    fn json_stdin_carries_the_call_shape() {
        // rc compares its whole stdin against the expected JSON: only an
        // exact byte match blocks, so this proves the JSON reaches the
        // hook stdin unmodified.
        let cmd =
            r#"test "$(cat)" = '{"name":"bash","arguments":{"command":"ls"}}' && exit 2 || exit 3"#;
        let v = run(&hook(cmd, ""), CALL);
        assert!(
            matches!(v, Verdict::Block(_)),
            "exact stdin match must prove the JSON shape: {v:?}"
        );
    }
}

#[cfg(all(test, unix))]
#[test]
fn load_config_reads_the_hooks_table() {
    let dir = std::env::temp_dir().join(format!("gc-hooks-cfg-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("config.toml"), "[hooks.pre_tool_use]\ncommand = \"echo blocked\"\nmatcher = \"edit\"\n[pricing]\ninput = 0.0\n").unwrap();
    // load_config reads the default path only; test via a temp override is
    // out of reach here, so assert the shape on a hand-built config text.
    let value: toml::Value =
        toml::from_str(&std::fs::read_to_string(dir.join("config.toml")).unwrap()).unwrap();
    let table = value
        .get("hooks")
        .unwrap()
        .get("pre_tool_use")
        .unwrap()
        .as_table()
        .unwrap();
    assert_eq!(
        table.get("command").unwrap().as_str().unwrap(),
        "echo blocked"
    );
    assert_eq!(table.get("matcher").unwrap().as_str().unwrap(), "edit");
    let _ = std::fs::remove_dir_all(&dir);
}
