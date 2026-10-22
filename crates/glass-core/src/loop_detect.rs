//! Loop detection over repeated tool calls.
//!
//! A SHA-256 ring window over the last [`WINDOW`] tool calls (name plus
//! canonical arguments). When the same call appears [`TRIGGER`] times in the
//! window, the caller is told once and the window resets — the model decides
//! what to do with the reminder, and one reminder per window prevents reminder
//! storms.
//!
//! Ported from crush's `loop_detection.go` (window 10, max repeats 5), with
//! the same window/repeat constants. Differences, stated: detection is per
//! single call, not whole-batch shape, so an interleaved useful call does not
//! mask a bad repeat; and the reminder is advisory — nothing is cancelled
//! automatically, because a completed tool result is never silently discarded.

use std::collections::VecDeque;

/// Ring size: how many recent calls the window remembers.
pub const WINDOW: usize = 10;
/// Repeats of the same call within the window that raise the reminder.
pub const TRIGGER: usize = 5;

/// Bounded ring of call hashes with repeat detection.
#[derive(Debug, Default)]
pub struct LoopWindow {
    hashes: VecDeque<[u8; 32]>,
    /// Fired for the current streak; reset when the window clears.
    fired: bool,
}

impl LoopWindow {
    pub fn new() -> Self {
        Self::default()
    }

    fn hash(name: &str, arguments: &str) -> [u8; 32] {
        use ring::digest::{SHA256, digest};
        let mut input = Vec::with_capacity(name.len() + arguments.len() + 1);
        input.extend_from_slice(name.as_bytes());
        input.push(0);
        input.extend_from_slice(arguments.as_bytes());
        let d = digest(&SHA256, &input);
        let mut out = [0u8; 32];
        out.copy_from_slice(d.as_ref());
        out
    }

    /// Record one tool call. Returns `true` exactly once per streak when the
    /// repeat threshold is crossed, so the caller can emit a single reminder.
    pub fn record(&mut self, name: &str, arguments: &str) -> bool {
        // A different call breaks the repeat streak for "fired" purposes but
        // stays in the window: crush counts repeats inside the sliding window
        // and our reminder must not re-fire every further repeat.
        let h = Self::hash(name, arguments);
        self.hashes.push_back(h);
        while self.hashes.len() > WINDOW {
            self.hashes.pop_front();
        }
        let repeats = self.hashes.iter().filter(|x| **x == h).count();
        if repeats >= TRIGGER && !self.fired {
            self.fired = true;
            return true;
        }
        false
    }

    /// Clear at a turn boundary, so a reminder from the previous turn's
    /// genuine work does not color the next one.
    pub fn clear(&mut self) {
        self.hashes.clear();
        self.fired = false;
    }
}

/// The reminder body the model receives. Names the repeated call so the
/// reminder means something without needing transcript context.
pub fn reminder(name: &str) -> String {
    format!(
        "Loop guard: `{name}` repeated {TRIGGER} times in the last {WINDOW} tool calls. \
         Stop repeating it; state what you are trying to achieve and take a different approach, \
         or report the blocker."
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fires_at_trigger_repeats_only_once() {
        let mut w = LoopWindow::new();
        for _ in 0..TRIGGER - 1 {
            assert!(!w.record("bash", "{\"command\":\"ls\"}"));
        }
        assert!(w.record("bash", "{\"command\":\"ls\"}"));
        // Further repeats do not re-fire within the same streak.
        assert!(!w.record("bash", "{\"command\":\"ls\"}"));
    }

    #[test]
    fn different_call_does_not_fire() {
        let mut w = LoopWindow::new();
        for i in 0..(TRIGGER * 2) {
            let args = format!("{{\"command\":\"echo {i}\"}}");
            assert!(!w.record("bash", &args));
        }
    }

    #[test]
    fn interleaved_distinct_calls_still_detect_counts() {
        let mut w = LoopWindow::new();
        // 4 ls plus interleaved distinct calls; the window holds 10, so the
        // interleaved ones do not push the ls entries out.
        for i in 0..(TRIGGER - 1) {
            assert!(!w.record("bash", "{\"command\":\"ls\"}"));
            assert!(!w.record("state", &format!("{{\"i\":{i}}}")));
        }
        // 4 ls in the window; the 5th crosses the threshold. Interleaved
        // distinct calls do not mask the count.
        assert!(w.record("bash", "{\"command\":\"ls\"}"));
        // And the reminder does not re-fire while `fired` is set.
        assert!(!w.record("bash", "{\"command\":\"ls\"}"));
    }

    #[test]
    fn clear_resets_everything() {
        let mut w = LoopWindow::new();
        for _ in 0..TRIGGER {
            w.record("bash", "{\"command\":\"ls\"}");
        }
        w.clear();
        for _ in 0..TRIGGER - 1 {
            assert!(!w.record("bash", "{\"command\":\"ls\"}"));
        }
        assert!(w.record("bash", "{\"command\":\"ls\"}"));
    }

    #[test]
    fn same_name_different_args_differ() {
        let mut w = LoopWindow::new();
        for _ in 0..TRIGGER - 1 {
            assert!(!w.record("bash", "{\"command\":\"a\"}"));
        }
        // A different call interrupts the streak but stays in the window.
        assert!(!w.record("bash", "{\"command\":\"b\"}"));
        // With only one distinct call inserted, the count of `a` is at 5 for
        // the first time — that is a genuine threshold crossing and DOES fire,
        // even though an unrelated call sits in the window.
        assert!(w.record("bash", "{\"command\":\"a\"}"));
        // And once fired, further repeats stay silent until clear().
        assert!(!w.record("bash", "{\"command\":\"a\"}"));
    }

    #[test]
    fn window_is_bounded() {
        let mut w = LoopWindow::new();
        for i in 0..(WINDOW * 3) {
            w.record("state", &format!("{{\"i\":{i}}}"));
        }
        // Nothing from long ago can still count toward a trigger.
        for _ in 0..TRIGGER - 1 {
            assert!(!w.record("bash", "{\"command\":\"stale-check\"}"));
        }
        assert!(w.record("bash", "{\"command\":\"stale-check\"}"));
    }

    #[test]
    fn reminder_names_the_call() {
        let r = reminder("grep");
        assert!(r.contains("grep"), "{r}");
        assert!(r.contains("different approach"), "{r}");
    }
}
