//! Turn-completion notifications.
//!
//! The point is unattended work: a long turn ends while you are in another
//! window, and a desktop notification or a terminal bell is what brings you
//! back. Short turns stay silent — notifying for every one-line answer is
//! noise — but a failed turn always notifies, however fast it failed.
//!
//! `auto` picks OSC 9 on terminals known to surface it and falls back to the
//! bell everywhere else. tmux is deliberately not treated as OSC-capable:
//! the sequence is swallowed unless passthrough is configured, so the bell,
//! which sets the window flag, is the honest choice.

use std::io::Write;
use std::time::Duration;

/// How a finished turn gets attention.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Mode {
    /// OSC 9 where the terminal supports it, bell otherwise.
    #[default]
    Auto,
    /// OSC 9 unconditionally.
    Osc,
    /// The terminal bell.
    Bell,
    /// Never notify.
    Off,
}

impl Mode {
    /// Parse a config value. Unknown values are rejected rather than guessed.
    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "auto" => Some(Mode::Auto),
            "osc" => Some(Mode::Osc),
            "bell" => Some(Mode::Bell),
            "off" | "none" | "disabled" => Some(Mode::Off),
            _ => None,
        }
    }
}

/// Turns shorter than this are answered while the user is still there.
pub const NOTIFY_AFTER_S: u64 = 5;

/// Whether a finished turn deserves attention.
///
/// `finished` excludes user-cancelled and steered turns: the user is
/// already at the keyboard in those cases, and an alert would be wrong.
pub fn wanted(failed: bool, finished: bool, elapsed: Duration, after_s: u64) -> bool {
    failed || (finished && elapsed.as_secs() >= after_s)
}

/// The escape sequence for a notification, if this mode and terminal
/// produce one.
///
/// Split from emission so the selection is testable without a terminal.
/// The message is sanitized first: provider or tool text must never be
/// able to smuggle terminal controls into the sequence.
pub fn sequence(
    mode: Mode,
    term_program: Option<&str>,
    term: Option<&str>,
    message: &str,
) -> Option<String> {
    let message = sanitize(message);
    if message.is_empty() {
        return None;
    }
    let osc = format!("\x1b]9;{message}\x07");
    match mode {
        Mode::Off => None,
        Mode::Bell => Some("\x07".into()),
        Mode::Osc => Some(osc),
        Mode::Auto => Some(if osc_capable(term_program, term) {
            osc
        } else {
            "\x07".into()
        }),
    }
}

/// Emit the notification for a finished turn. Best effort, never blocking.
pub fn emit(mode: Mode, message: &str) {
    let Some(seq) = sequence(
        mode,
        std::env::var("TERM_PROGRAM").ok().as_deref(),
        std::env::var("TERM").ok().as_deref(),
        message,
    ) else {
        return;
    };
    let mut out = std::io::stdout();
    let _ = out.write_all(seq.as_bytes());
    let _ = out.flush();
}

/// Terminals that surface OSC 9 as a desktop notification.
///
/// Deliberately conservative: a terminal that ignores the sequence shows
/// nothing at all, which is worse than a bell.
fn osc_capable(program: Option<&str>, term: Option<&str>) -> bool {
    const PROGRAMS: &[&str] = &["iTerm.app", "WezTerm", "vscode", "mintty"];
    if let Some(p) = program
        && PROGRAMS.iter().any(|k| p.eq_ignore_ascii_case(k))
    {
        return true;
    }
    matches!(term, Some(t) if t.contains("kitty") || t.contains("ghostty"))
}

/// Printable text only, bounded. Escape, bell, and every other control
/// character is dropped.
fn sanitize(message: &str) -> String {
    message
        .chars()
        .filter(|c| !c.is_control())
        .take(200)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mode_parses_and_rejects_unknown_values() {
        assert_eq!(Mode::parse("auto"), Some(Mode::Auto));
        assert_eq!(Mode::parse(" OSC "), Some(Mode::Osc));
        assert_eq!(Mode::parse("Bell"), Some(Mode::Bell));
        assert_eq!(Mode::parse("off"), Some(Mode::Off));
        assert_eq!(Mode::parse("sometimes"), None);
    }

    #[test]
    fn off_emits_nothing() {
        assert_eq!(sequence(Mode::Off, None, None, "done"), None);
    }

    #[test]
    fn fixed_modes_emit_their_sequence() {
        let osc = sequence(Mode::Osc, None, Some("dumb"), "done").unwrap();
        assert!(osc.starts_with("\x1b]9;"));
        assert!(osc.ends_with('\x07'));
        assert_eq!(sequence(Mode::Bell, None, Some("xterm-kitty"), "done").unwrap(), "\x07");
    }

    #[test]
    fn auto_prefers_osc_on_capable_terminals_and_bells_elsewhere() {
        let seq = sequence(Mode::Auto, Some("iTerm.app"), Some("xterm"), "done").unwrap();
        assert!(seq.starts_with("\x1b]9;"), "iTerm must get OSC 9");
        let seq = sequence(Mode::Auto, None, Some("xterm-kitty"), "done").unwrap();
        assert!(seq.starts_with("\x1b]9;"), "kitty must get OSC 9");
        let seq = sequence(Mode::Auto, None, Some("xterm-256color"), "done").unwrap();
        assert_eq!(seq, "\x07", "an unknown terminal gets the bell");
    }

    #[test]
    fn message_cannot_smuggle_control_characters() {
        let seq = sequence(Mode::Osc, None, None, "done\x1b]9;spoof\x07\n").unwrap();
        assert_eq!(seq, "\x1b]9;done]9;spoof\x07");
        assert!(!seq[4..].contains('\n'));
    }

    #[test]
    fn an_empty_message_after_sanitize_emits_nothing() {
        assert_eq!(sequence(Mode::Bell, None, None, "\x07\x1b"), None);
    }

    #[test]
    fn only_slow_finished_or_failed_turns_notify() {
        let fast = Duration::from_secs(1);
        let slow = Duration::from_secs(30);
        assert!(!wanted(false, true, fast, NOTIFY_AFTER_S));
        assert!(wanted(false, true, slow, NOTIFY_AFTER_S));
        assert!(wanted(true, false, fast, NOTIFY_AFTER_S), "failures always notify");
        assert!(
            !wanted(false, false, slow, NOTIFY_AFTER_S),
            "cancelled or steered turns must stay silent"
        );
    }

    #[test]
    fn threshold_is_inclusive() {
        assert!(wanted(false, true, Duration::from_secs(5), 5));
        assert!(!wanted(false, true, Duration::from_secs(4), 5));
    }
}
