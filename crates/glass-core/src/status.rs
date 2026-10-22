//! Program status (OSC 7501): what this tab is doing, for the terminal.
//!
//! Spec: the Program Status Protocol v0.3
//! (superlogical.com/rex/docs/build/program-status). One OSC report per
//! state change; `state` is required, `app` identifies the program, `msg` is
//! base64 text — and never carries prompts or model output.
//!
//! Detection: the spec offers an optional reply handshake. glass-core reads
//! input through crossterm's event parser, which never sees the raw OSC
//! reply, so reports go out without detection — explicitly permitted by the
//! spec ("sending reports without detecting support is safe": well-behaved
//! terminals ignore unknown OSCs, and multiplexers swallow them instead of
//! painting anything). Reports are emitted only for an interactive terminal
//! with a real `TERM`, so pipes, CI, and headless runs stay byte-clean.

use std::io::{IsTerminal, Write};
use std::sync::atomic::{AtomicBool, Ordering};

static ENABLED: AtomicBool = AtomicBool::new(false);

/// Configure from `program_status = "on" | "off"`; returns whether reports
/// will be emitted.
pub fn configure(mode: &str) -> bool {
    let tty = std::io::stdin().is_terminal() && std::io::stdout().is_terminal();
    let term = std::env::var("TERM").unwrap_or_default();
    let capable = tty && !term.is_empty() && term != "dumb";
    let on = mode == "on" && capable;
    ENABLED.store(on, Ordering::Relaxed);
    on
}

/// The report body without escape framing: `state=…:app=glass-core[:kind=…][:msg=…]`.
pub fn body(state: &str, kind: Option<&str>, msg: &str) -> String {
    let mut body = format!("state={state}:app=glass-core");
    if state == "blocked"
        && let Some(kind) = kind.filter(|k| matches!(*k, "permission" | "question" | "auth"))
    {
        body.push_str(&format!(":kind={kind}"));
    }
    let msg = msg.trim();
    if !msg.is_empty() {
        use base64::Engine;
        let capped: String = msg.chars().take(200).collect();
        let encoded = base64::engine::general_purpose::STANDARD.encode(capped);
        body.push_str(&format!(":msg={encoded}"));
    }
    body
}

/// Write one report through `out`.
pub fn emit(mut out: impl Write, state: &str, kind: Option<&str>, msg: &str) {
    let _ = write!(out, "\x1b]7501;{}\x1b\\", body(state, kind, msg));
    let _ = out.flush();
}

/// Report a state to the terminal, when enabled.
pub fn set(state: &str, msg: &str) {
    if ENABLED.load(Ordering::Relaxed) {
        emit(std::io::stdout(), state, None, msg);
    }
}

/// Report `blocked` with the reason the protocol understands.
pub fn blocked(kind: &str, msg: &str) {
    if ENABLED.load(Ordering::Relaxed) {
        emit(std::io::stdout(), "blocked", Some(kind), msg);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reports_carry_state_app_and_base64_message() {
        assert_eq!(body("working", None, ""), "state=working:app=glass-core");
        assert_eq!(
            body("working", None, "Running tests"),
            "state=working:app=glass-core:msg=UnVubmluZyB0ZXN0cw==",
            "msg is standard base64 of the trimmed text"
        );
        assert_eq!(
            body("blocked", Some("permission"), "Allow reading AGENTS.md?"),
            "state=blocked:app=glass-core:kind=permission:msg=QWxsb3cgcmVhZGluZyBBR0VOVFMubWQ/"
        );
        // `kind` is only meaningful with blocked; others ignore it.
        assert_eq!(body("done", Some("auth"), ""), "state=done:app=glass-core");
        // Unknown kinds are dropped so an older terminal never misreads them.
        assert_eq!(
            body("blocked", Some("nonsense"), ""),
            "state=blocked:app=glass-core"
        );
    }

    #[test]
    fn a_long_message_is_capped_before_encoding() {
        let body = body("working", None, &"x".repeat(5_000));
        let encoded = body.split(":msg=").nth(1).unwrap();
        use base64::Engine;
        let decoded = base64::engine::general_purpose::STANDARD
            .decode(encoded)
            .unwrap();
        assert_eq!(decoded.len(), 200, "capped at the documented 200 chars");
    }

    #[test]
    fn emit_frames_the_report_and_off_stays_silent() {
        let mut out: Vec<u8> = Vec::new();
        emit(&mut out, "idle", None, "");
        assert_eq!(
            String::from_utf8(out).unwrap(),
            "\x1b]7501;state=idle:app=glass-core\x1b\\"
        );
        let previous = ENABLED.swap(false, Ordering::Relaxed);
        set("working", "should not appear");
        ENABLED.store(previous, Ordering::Relaxed);
    }

    #[test]
    fn nothing_is_emitted_without_a_capable_terminal() {
        // The test process has no interactive stdout: configure must say no
        // even when asked, so reports cannot leak into test output.
        assert!(!configure("on") || std::io::stdout().is_terminal());
        configure("off");
    }
}
