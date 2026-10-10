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

use crate::session::{Record, Session};
use std::collections::VecDeque;
use std::path::PathBuf;

/// How a message typed during a turn is treated.
#[derive(Debug, PartialEq, Eq)]
pub enum Intent {
    Steer(String),
    FollowUp(String),
}

#[derive(Default, Clone, serde::Serialize, serde::Deserialize)]
pub struct Queue {
    /// Ordered follow-ups, delivered when the current turn finishes.
    pending: VecDeque<String>,
    #[serde(skip)]
    path: Option<PathBuf>,
    #[serde(default)]
    delivered: u64,
    /// The next steer, applied at the next safe point.
    steer: Option<String>,
}

impl Queue {
    #[cfg(test)]
    pub fn new() -> Self {
        Self::default()
    }

    /// Load a session-owned queue. Delivery receipts prevent replay after a crash.
    pub fn open(session: &Session) -> Result<Self, String> {
        let path = session.path.with_extension("queue.json");
        let mut queue: Self = match std::fs::read(&path) {
            Ok(bytes) => {
                serde_json::from_slice(&bytes).map_err(|e| format!("Invalid queue: {e}"))?
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                return Ok(Self { path: Some(path), ..Self::default() });
            }
            Err(e) => return Err(e.to_string()),
        };
        queue.path = Some(path);
        if queue.is_empty() { return Ok(queue); }
        let records = Session::raw_records(&session.path).map_err(|e| e.to_string())?;
        for pair in records.windows(2) {
            if !matches!(&pair[1], Record::User(_)) { continue; }
            if let Record::Custom { kind, data: Some(data) } = &pair[0]
                && kind == "queue/delivery"
                && let Ok(sequence) = data.parse::<u64>()
                && sequence > queue.delivered
            {
                queue.take_next();
                queue.delivered = sequence;
            }
        }
        queue.save()?;
        Ok(queue)
    }

    fn save(&self) -> Result<(), String> {
        if let Some(path) = &self.path {
            crate::auth::secure_write(path, &serde_json::to_vec(self).map_err(|e| e.to_string())?)?;
        }
        Ok(())
    }

    pub fn push_checked(&mut self, intent: Intent) -> Result<(), String> {
        let previous = self.clone();
        self.push(intent);
        if let Err(error) = self.save() {
            *self = previous;
            return Err(error);
        }
        Ok(())
    }

    /// Commit the prompt and its receipt before removing it from durable storage.
    pub fn deliver(&mut self, session: &mut Session) -> Result<Option<String>, String> {
        let mut updated = self.clone();
        let Some(text) = updated.take_next() else {
            return Ok(None);
        };
        updated.delivered = updated
            .delivered
            .checked_add(1)
            .ok_or("Queue sequence exhausted")?;
        session
            .append_checked(&[
                Record::Custom {
                    kind: "queue/delivery".into(),
                    data: Some(updated.delivered.to_string()),
                },
                Record::User(text.clone()),
            ])
            .map_err(|e| e.to_string())?;
        *self = updated;
        // A failed sidecar update is recoverable from the committed receipt.
        if let Err(error) = self.save() {
            eprintln!("Queue receipt saved; queue checkpoint pending: {error}");
        }
        Ok(Some(text))
    }

    pub fn correction(&self) -> Option<&str> { self.steer.as_deref() }

    pub fn manage(&mut self, keys: &crate::input::KeyReader) {
        loop {
            let entries = self.describe();
            if entries.is_empty() {
                crate::modal::info(
                    keys,
                    "Follow-up queue",
                    "No queued messages. Alt+Enter queues a follow-up during a turn.",
                );
                return;
            }
            let Some(index) =
                crate::modal::choose(keys, "Follow-up queue", "Saved with this session", &entries)
            else {
                return;
            };
            let actions = [
                "Edit",
                "Move earlier",
                "Move later",
                "Promote to correction",
                "Delete",
            ]
            .map(String::from);
            let Some(action) =
                crate::modal::choose(keys, "Queued message", &entries[index], &actions)
            else {
                continue;
            };
            let previous = self.clone();
            let steer_selected = self.steer.is_some() && index == 0;
            let offset = usize::from(self.steer.is_some());
            let pending_index = index.saturating_sub(offset);
            match action {
                0 => {
                    let current = if steer_selected {
                        self.steer.as_ref().unwrap()
                    } else {
                        &self.pending[pending_index]
                    };
                    if let Some(text) =
                        crate::modal::field(keys, "Edit queued message", "Message", current, false)
                        && !text.trim().is_empty()
                    {
                            if steer_selected {
                                self.steer = Some(text);
                            } else {
                                self.pending[pending_index] = text;
                            }
                    }
                }
                1 if !steer_selected && pending_index > 0 => {
                    self.pending.swap(pending_index, pending_index - 1)
                }
                2 if !steer_selected && pending_index + 1 < self.pending.len() => {
                    self.pending.swap(pending_index, pending_index + 1)
                }
                3 if !steer_selected => {
                    let text = self.pending.remove(pending_index).unwrap();
                    if let Some(old) = self.steer.replace(text) {
                        self.pending.push_front(old);
                    }
                }
                4 => {
                    if steer_selected {
                        self.steer = None;
                    } else {
                        self.pending.remove(pending_index);
                    }
                }
                _ => {}
            }
            if let Err(error) = self.save() {
                *self = previous;
                crate::modal::info(keys, "Queue was not changed", &error);
            }
        }
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

    /// Deliver a pending correction before later queued work. The active
    /// Agent owns cancellation; the REPL owns durable delivery on continuation.
    pub fn take_next(&mut self) -> Option<String> {
        self.steer.take().or_else(|| self.pending.pop_front())
    }

    /// Take the next follow-up, if any.
    #[cfg_attr(not(test), allow(dead_code))]
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
    fn queue_survives_restart_and_committed_delivery_is_not_replayed() {
        let dir = std::env::temp_dir().join(format!(
            "gc-queue-{}-{}",
            std::process::id(),
            crate::auth::random(8).unwrap()
        ));
        let mut session = Session::create(&dir).unwrap();
        let mut queue = Queue::open(&session).unwrap();
        queue
            .push_checked(Intent::FollowUp("first".into()))
            .unwrap();
        queue
            .push_checked(Intent::FollowUp("second".into()))
            .unwrap();
        let old_checkpoint = std::fs::read(queue.path.as_ref().unwrap()).unwrap();
        assert_eq!(
            queue.deliver(&mut session).unwrap().as_deref(),
            Some("first")
        );
        // Simulate a crash after the durable session receipt but before sidecar rename.
        std::fs::write(queue.path.as_ref().unwrap(), old_checkpoint).unwrap();
        let mut recovered = Queue::open(&session).unwrap();
        assert_eq!(recovered.describe(), ["queued: second"]);
        assert_eq!(
            recovered.deliver(&mut session).unwrap().as_deref(),
            Some("second")
        );
        assert!(Queue::open(&session).unwrap().is_empty());
        assert_eq!(
            session
                .records
                .iter()
                .filter(|r| matches!(r, Record::User(_)))
                .count(),
            2
        );
        drop(session);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn failed_queue_save_preserves_previous_contents() {
        let mut queue = Queue::new();
        queue.push(Intent::FollowUp("keep".into()));
        queue.path = Some(PathBuf::from("/dev/null/impossible"));
        assert!(
            queue
                .push_checked(Intent::FollowUp("not acknowledged".into()))
                .is_err()
        );
        assert_eq!(queue.describe(), ["queued: keep"]);
    }

    #[test]
    fn pending_correction_is_delivered_before_queued_work_once() {
        let mut queue = Queue::new();
        queue.push(Intent::FollowUp("later".into()));
        queue.push(Intent::Steer("first correction".into()));
        queue.push(Intent::Steer("latest correction".into()));
        assert_eq!(queue.take_next().as_deref(), Some("latest correction"));
        assert_eq!(queue.take_next().as_deref(), Some("later"));
        assert!(queue.take_next().is_none());
    }

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
        assert_eq!(
            classify("fix", false, true),
            Some(Intent::Steer("fix".into()))
        );
        assert_eq!(
            classify("fix", false, false),
            Some(Intent::FollowUp("fix".into()))
        );
    }

    #[test]
    fn alt_enter_always_queues() {
        assert_eq!(
            classify("then docs", true, true),
            Some(Intent::FollowUp("then docs".into()))
        );
        assert_eq!(
            classify("then docs", true, false),
            Some(Intent::FollowUp("then docs".into()))
        );
    }

    #[test]
    fn blank_submissions_are_dropped() {
        assert_eq!(classify("", false, true), None);
        assert_eq!(classify("   \n ", true, false), None);
    }

    #[test]
    fn text_is_trimmed() {
        assert_eq!(
            classify("  hi  ", false, true),
            Some(Intent::Steer("hi".into()))
        );
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
