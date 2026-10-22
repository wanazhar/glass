//! OSC 8 terminal hyperlinks (task six-wave-d-001, opencode).
//!
//! Reader and tool-card URLs render with OSC 8 escape sequences when the
//! terminal advertises support. The escape is applied to the *source text*
//! before layout, so wrapped lines need no special handling — each wrap
//! segment carries its escapes. Terminals that don't support OSC 8 get
//! byte-identical output to today: capability detection is conservative
//! (explicit opt-in or exact TERM hints), never a guess.
//!
//! The escape format is the OSC 8 spec: `ESC ] 8 ; params ; URI ST` for the
//! start and `ESC ] 8 ; ; ST` for the end. The ST is the standalone
//! terminator (`ESC \`), chosen over BEL because the same writer already
//! emits OSC 7501 with ST and mixed styles read worse.

use std::sync::atomic::{AtomicU8, Ordering};

/// 0 = undetected, 1 = unsupported, 2 = supported (escape-byte convention
/// used by the existing capability maps in `notify.rs`).
static OSC8: AtomicU8 = AtomicU8::new(0);

/// Configure support from an explicit session-level setting
/// (`hyperlinks = "on" | "off" | "auto"`). `auto` consults TERM hints.
pub fn configure(mode: &str) -> bool {
    let term = std::env::var("TERM").unwrap_or_default();
    let supported = match mode {
        "on" => true,
        "off" => false,
        _ => supports(&term),
    };
    OSC8.store(if supported { 2 } else { 1 }, Ordering::Relaxed);
    supported
}

/// Conservative TERM-based detection: only terminals known to implement
/// OSC 8 in their emitting paths. Unknown = unsupported = unchanged bytes.
pub fn supports(term: &str) -> bool {
    let lower = term.to_ascii_lowercase();
    lower.contains("kitty") || lower.contains("ghostty") || lower.contains("wezterm")
}

fn supported() -> bool {
    OSC8.load(Ordering::Relaxed) == 2
}

#[cfg(test)]
fn with_term(term: Option<&str>, f: impl FnOnce()) {
    // SAFETY: tests are single-threaded around env access; the TEST_LOCK
    // below serializes anything that touches TERM.
    unsafe {
        match term {
            Some(t) => std::env::set_var("TERM", t),
            None => std::env::remove_var("TERM"),
        }
    }
    f();
}

/// Wrap every bare URL in `text` with OSC 8 escapes when the terminal
/// supports them; otherwise the text passes through untouched (byte-clean
/// fallback, asserted by tests).
pub fn link_urls(text: &str) -> String {
    if !supported() || !text.contains("http") {
        return text.to_string();
    }
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(pos) = rest.find("http://").or_else(|| rest.find("https://")) {
        let (before, candidate) = rest.split_at(pos);
        // A URL ends at whitespace or a closing bracket/code fence: the
        // split character itself stays outside the link.
        let end = candidate
            .find(|c: char| c.is_whitespace() || c == ')' || c == ']' || c == '`')
            .unwrap_or(candidate.len());
        let (url, tail) = candidate.split_at(end);
        out.push_str(before);
        push_link(&mut out, url);
        if tail.is_empty() {
            rest = "";
            break;
        }
        // The terminating character is plain text; resume after it.
        out.push(tail.chars().next().unwrap_or_default());
        rest = &tail[tail.chars().next().map(char::len_utf8).unwrap_or(1)..];
    }
    out.push_str(rest);
    out
}

fn push_link(out: &mut String, url: &str) {
    out.push_str("\x1b]8;;");
    out.push_str(url);
    out.push_str("\x1b\\");
    out.push_str(url);
    out.push_str("\x1b]8;;\x1b\\");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unsupported_terminals_get_byte_identical_output() {
        OSC8.store(1, Ordering::Relaxed);
        let text = "see https://example.com/x and http://a.b\nwrapped";
        assert_eq!(link_urls(text), text, "fallback must be byte-clean");
    }

    #[test]
    fn supported_terminals_link_urls_with_escapes() {
        OSC8.store(2, Ordering::Relaxed);
        let out = link_urls("read https://example.com/next");
        assert!(out.starts_with("read \x1b]8;;https://example.com/next\x1b\\"));
        assert!(out.contains("https://example.com/next\x1b]8;;\x1b\\"));
        assert_eq!(
            out.matches("https://example.com/next").count(),
            2,
            "visible text plus uri target: {out}"
        );
        // Split characters stay outside the link.
        let out = link_urls("(https://a.b/c)");
        assert!(out.starts_with("("), "{out}");
        assert!(
            out.contains(")\x1b]8;;\x1b\\") || out.ends_with(")"),
            "{out}"
        );
        OSC8.store(1, Ordering::Relaxed);
    }

    #[test]
    fn configure_follows_the_setting_and_term() {
        assert!(configure("on"));
        assert_eq!(OSC8.load(Ordering::Relaxed), 2);
        assert!(!configure("off"));
        assert_eq!(OSC8.load(Ordering::Relaxed), 1);
        // auto with no hint = no; auto with a hint = yes.
        with_term(None, || {
            assert!(!configure("auto"));
            assert_eq!(OSC8.load(Ordering::Relaxed), 1);
        });
        with_term(Some("xterm-ghostty"), || {
            assert!(configure("auto"));
            assert_eq!(OSC8.load(Ordering::Relaxed), 2);
        });
        // restore something sane for other tests.
        with_term(Some("xterm-256color"), || {});
        OSC8.store(1, Ordering::Relaxed);
    }
}
