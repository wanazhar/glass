//! Input while a turn is running: steer vs follow-up.
//!
//! Two different intentions, and conflating them is worse than having neither.
//!
//! - **Steer** (Enter): you are changing what the model is doing *right now*.
//!   "Stop, use Postgres not MySQL."
//! - **Follow-up** (Alt+Enter): this is for *after*. "Then update the docs."
//!
//! pi distinguishes these and it is worth copying, because the alternative is
//! either interrupting on every Enter (so you cannot queue) or blocking on
//! input until the turn ends (so you cannot redirect).
//!
//! The queue is a plain FIFO with one steer slot, because a steer must apply
//! before the next model request or it has missed its chance.

use std::collections::VecDeque;

/// How a message typed during a turn is treated.
#[derive(Debug, PartialEq, Eq)]
pub enum Intent {
    Steer(String),
    FollowUp(String),
}

#[derive(Default)]
pub struct Queue {
    /// Ordered follow-ups, delivered when the current turn finishes.
    pending: VecDeque<String>,
    /// The next steer, applied at the next safe point.
    steer: Option<String>,
}

impl Queue {
    pub fn new() -> Self {
        Self::default()
    }

    /// Record input. Empty text is ignored rather than queued as a no-op.
    pub fn push(&mut self, intent: Intent) {
        match intent {
            Intent::Steer(s) => {
                let s = s.trim().to_string();
                if !s.is_empty() {
                    // A second steer before the first is applied overwrites it:
                    // both are corrections, and the later one is the one the
                    // user actually finished typing.
                    self.steer = Some(s);
                }
            }
            Intent::FollowUp(s) => {
                let s = s.trim().to_string();
                if !s.is_empty() {
                    self.pending.push_back(s);
                }
            }
        }
    }

    /// Take the next follow-up, if any.
    pub fn take_follow_up(&mut self) -> Option<String> {
        self.pending.pop_front()
    }

    /// True when nothing is queued or pending.
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn is_empty(&self) -> bool {
        self.pending.is_empty() && self.steer.is_none()
    }

    /// Human-readable summary. Kept for the status line and for tests that
    /// assert on ordering, which is the only way to observe queue state.
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn describe(&self) -> Vec<String> {
        let mut out = Vec::new();
        if let Some(s) = &self.steer {
            out.push(format!("steer: {s}"));
        }
        for s in &self.pending {
            out.push(format!("queued: {s}"));
        }
        out
    }


}

/// Classify a submitted line.
///
/// `alt` distinguishes the two: Enter steers, Alt+Enter queues. When nothing is
/// running both do the same thing, so the distinction only matters mid-turn.
pub fn classify(text: &str, alt: bool, turn_running: bool) -> Option<Intent> {
    let t = text.trim();
    if t.is_empty() {
        return None;
    }
    if alt {
        return Some(Intent::FollowUp(t.to_string()));
    }
    if turn_running {
        return Some(Intent::Steer(t.to_string()));
    }
    Some(Intent::FollowUp(t.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn later_steer_replaces_earlier() {
        let mut q = Queue::new();
        q.push(Intent::Steer("stop".into()));
        q.push(Intent::Steer("actually use postgres".into()));
        // The agent owns the steer slot now, so this asserts on the queue's
        // observable state rather than a getter that no longer exists.
        assert_eq!(q.describe()[0], "steer: actually use postgres");
    }

    #[test]
    fn follow_ups_are_fifo() {
        let mut q = Queue::new();
        q.push(Intent::FollowUp("first".into()));
        q.push(Intent::FollowUp("second".into()));
        assert_eq!(q.take_follow_up().as_deref(), Some("first"));
        assert_eq!(q.take_follow_up().as_deref(), Some("second"));
        assert!(q.take_follow_up().is_none());
    }

    #[test]
    fn empty_text_is_ignored() {
        let mut q = Queue::new();
        q.push(Intent::Steer("   ".into()));
        q.push(Intent::FollowUp("".into()));
        assert!(q.is_empty(), "{:?}", q.describe());
    }

    #[test]
    fn enter_steers_only_while_running() {
        assert_eq!(classify("fix", false, true), Some(Intent::Steer("fix".into())));
        assert_eq!(classify("fix", false, false), Some(Intent::FollowUp("fix".into())));
    }

    #[test]
    fn alt_enter_always_queues() {
        assert_eq!(classify("then docs", true, true), Some(Intent::FollowUp("then docs".into())));
        assert_eq!(classify("then docs", true, false), Some(Intent::FollowUp("then docs".into())));
    }

    #[test]
    fn blank_submissions_are_dropped() {
        assert_eq!(classify("", false, true), None);
        assert_eq!(classify("   \n ", true, false), None);
    }

    #[test]
    fn text_is_trimmed() {
        assert_eq!(classify("  hi  ", false, true), Some(Intent::Steer("hi".into())));
    }

    #[test]
    fn describe_lists_steer_before_queue() {
        let mut q = Queue::new();
        q.push(Intent::FollowUp("later".into()));
        q.push(Intent::Steer("now".into()));
        assert_eq!(q.describe(), vec!["steer: now", "queued: later"]);
    }

    #[test]
    fn queue_reports_emptiness_correctly() {
        let mut q = Queue::new();
        assert!(q.is_empty());
        q.push(Intent::Steer("x".into()));
        assert!(!q.is_empty(), "a steer alone makes the queue non-empty");
        let _ = q.take_follow_up();
        assert!(!q.is_empty(), "the steer is still pending");
    }
}