//! Duplicate-request detection and durable receipts for the `state` tool.
//!
//! `state` is a full replacement guarded by an optimistic revision, and that
//! guard only answers one question: *did someone else write since I read?* It
//! says nothing about the other failure mode — the same write arriving twice.
//! Without an idempotency key a retry either fails as stale or, once the
//! revision has moved on, is indistinguishable from a request that was never
//! accepted. The caller cannot tell "already applied" from "never applied", so
//! a retry loop has to guess.
//!
//! This module supplies the missing half:
//!
//! - a **request id** names one intent, and a repeat of a known id returns the
//!   receipt recorded for it instead of applying anything a second time;
//! - the **receipt** — what was applied, under which id, against which base
//!   revision, by whom, when — is written into the session log beside the state
//!   record it produced, so a replay shows the effect, not just the payload;
//! - the **ledger** that answers "have I seen this id?" is bounded, and both
//!   the bound and the eviction rule are reported to the caller rather than
//!   silently dropping ids.
//!
//! It reuses the existing effect sandwich: `State::apply` prepares a receipt,
//! and the session appends it in the *same* write as the `Record::State` it
//! belongs to. One write means the receipt can never describe work the log did
//! not accept, and a failed append rolls both back together.
//!
//! The module is pulled in from `session.rs` rather than declared in the crate
//! root because `main.rs` owns the module list for this change set.

use std::cell::RefCell;
use std::collections::VecDeque;

use serde_json::{Value, json};

use crate::session::Record;

/// Receipts the in-memory ledger keeps before it starts evicting.
///
/// Bounded because the ledger lives for the length of a session and the model
/// can name as many requests as it likes. Eviction is oldest-first and is
/// always reported, so a caller whose id fell out is told rather than left to
/// discover a silently lost guarantee.
pub const LEDGER_CAPACITY: usize = 256;

/// Namespaced `Record::Custom` kind that carries a receipt in the log.
///
/// A namespaced custom entry rather than a new record kind: the durable
/// envelope (`append`/`append_checked`, repair, replay, export) already
/// handles it, older readers skip it as an unknown kind, and it stays
/// display-only instead of entering model context.
pub const RECEIPT_KIND: &str = "state/receipt";

/// Content revision of a state payload. Also the fingerprint of a request's
/// content: two requests that normalize to the same items are the same write.
pub fn revision_of(items: &[String]) -> String {
    crate::workspace::hash(serde_json::to_string(items).unwrap_or_default().as_bytes())
}

/// What one accepted request did, recorded next to the state it produced.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Receipt {
    /// The caller-supplied request id.
    pub request_id: String,
    /// Revision after the write.
    pub revision: String,
    /// Revision the caller presented (the empty-state revision when it
    /// presented none).
    pub base_revision: String,
    pub items: usize,
    pub bytes: usize,
    /// The session that accepted the write, filled in by the writer itself
    /// rather than asserted by the caller.
    pub actor: String,
    /// Unix seconds the write was applied.
    pub applied_at: u64,
}

impl Receipt {
    pub fn to_json(&self) -> Value {
        json!({
            "v": 1,
            "request": self.request_id,
            "revision": self.revision,
            "base": self.base_revision,
            "items": self.items,
            "bytes": self.bytes,
            "actor": self.actor,
            "at": self.applied_at,
        })
    }

    /// Tolerates a partial record: an id and a revision are enough to answer a
    /// replay, and everything else is reporting detail.
    pub fn from_json(value: &Value) -> Option<Receipt> {
        let text = |key: &str| value.get(key).and_then(Value::as_str).map(str::to_string);
        Some(Receipt {
            request_id: text("request")?,
            revision: text("revision").unwrap_or_default(),
            base_revision: text("base").unwrap_or_default(),
            items: value.get("items").and_then(Value::as_u64).unwrap_or(0) as usize,
            bytes: value.get("bytes").and_then(Value::as_u64).unwrap_or(0) as usize,
            actor: text("actor").unwrap_or_default(),
            applied_at: value.get("at").and_then(Value::as_u64).unwrap_or(0),
        })
    }

    /// The durable record written beside the `Record::State` it describes.
    pub fn record(&self) -> Record {
        Record::Custom {
            kind: RECEIPT_KIND.to_string(),
            data: Some(self.to_json().to_string()),
        }
    }

    /// One transcript line. Present in history so a replay shows the effect
    /// and its ownership, not just the notes it left behind.
    pub fn render(&self) -> String {
        format!(
            "state receipt · request {} · revision {} (from {}) · {} items, {} B · by {} · at {}",
            self.request_id,
            short(&self.revision),
            short(&self.base_revision),
            self.items,
            self.bytes,
            if self.actor.is_empty() {
                "unbound writer"
            } else {
                self.actor.as_str()
            },
            crate::format_time(
                std::time::UNIX_EPOCH + std::time::Duration::from_secs(self.applied_at)
            ),
        )
    }
}

fn short(revision: &str) -> &str {
    revision.get(..8).unwrap_or(revision)
}

/// The receipts a session log currently remembers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ledger {
    /// Oldest first.
    entries: VecDeque<Receipt>,
    bound: usize,
    evicted: usize,
}

impl Default for Ledger {
    fn default() -> Self {
        Self::new()
    }
}

impl Ledger {
    pub fn new() -> Self {
        Self::with_bound(LEDGER_CAPACITY)
    }

    pub fn with_bound(bound: usize) -> Self {
        Self {
            entries: VecDeque::new(),
            bound: bound.max(1),
            evicted: 0,
        }
    }

    pub fn get(&self, request_id: &str) -> Option<&Receipt> {
        self.entries
            .iter()
            .find(|receipt| receipt.request_id == request_id)
    }

    /// Record a receipt, returning the one it evicted when the bound is hit.
    pub fn record(&mut self, receipt: Receipt) -> Option<Receipt> {
        self.entries
            .retain(|entry| entry.request_id != receipt.request_id);
        self.entries.push_back(receipt);
        self.trim()
    }

    /// Seed from a log. Same bound and same eviction rule as a live ledger, so
    /// a restored ledger cannot be larger than a running one.
    pub fn restore<'a>(receipts: impl IntoIterator<Item = &'a Receipt>) -> Self {
        let mut ledger = Self::new();
        for receipt in receipts {
            ledger.record(receipt.clone());
        }
        ledger
    }

    fn trim(&mut self) -> Option<Receipt> {
        if self.entries.len() <= self.bound {
            return None;
        }
        let evicted = self.entries.pop_front();
        self.evicted += 1;
        evicted
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn bound(&self) -> usize {
        self.bound
    }

    pub fn evicted(&self) -> usize {
        self.evicted
    }

    /// Bound, current size and eviction rule, reported in every write result
    /// and revision read-back so a caller can see what it is relying on.
    pub fn report(&self) -> Value {
        json!({
            "retained": self.entries.len(),
            "bound": self.bound,
            "evicted": self.evicted(),
            "eviction": "oldest request id first",
        })
    }
}

/// Identity of the session a `State` belongs to, published when a session is
/// opened and adopted when the state is created.
///
/// The restore sites rebuild `State` by replaying the log, so this is how a
/// restored state learns which session it belongs to and which receipts that
/// session already durably accepted. A `State` with no adopted context is
/// reported as unbound rather than asserting ownership it cannot verify.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionContext {
    pub session: String,
    /// Whether this process holds the session's exclusive log writer lock,
    /// i.e. whether appends to that log actually happen here.
    pub durable_writer: bool,
    /// Revision of the last state record in the log.
    pub revision: String,
    pub receipts: Vec<Receipt>,
}

thread_local! {
    static CURRENT: RefCell<Option<SessionContext>> = const { RefCell::new(None) };
}

pub fn publish(context: SessionContext) {
    CURRENT.with(|slot| *slot.borrow_mut() = Some(context));
}

pub fn adopt() -> Option<SessionContext> {
    CURRENT.with(|slot| slot.borrow().clone())
}

/// A receipt an applied state change is waiting to be durably accepted for.
#[derive(Debug, Clone)]
pub struct Staged {
    /// Revision the state held before this change.
    pub base_revision: String,
    /// The items this change produced.
    pub items: Vec<String>,
    pub receipt: Receipt,
}

thread_local! {
    static STAGED: RefCell<Option<Staged>> = const { RefCell::new(None) };
}

/// Hand a receipt to the session append that is about to record this change.
pub fn stage(staged: Staged) {
    STAGED.with(|slot| *slot.borrow_mut() = Some(staged));
}

/// Take the staged receipt only if it belongs to the state record being
/// appended.
///
/// The stage is consumed either way. It is written by the apply that
/// immediately precedes the append, so leaving it behind would risk pairing it
/// with an unrelated later write; the base revision plus the exact items make
/// that mismatch detectable instead of guessed at.
pub fn take_receipt(base_revision: &str, items: &[String]) -> Option<Receipt> {
    STAGED.with(|slot| {
        slot.borrow_mut()
            .take()
            .filter(|staged| staged.base_revision == base_revision && staged.items == items)
            .map(|staged| staged.receipt)
    })
}

/// Recover every receipt a log remembers, oldest first.
pub fn receipts_in(records: &[Record]) -> Vec<Receipt> {
    records
        .iter()
        .filter_map(|record| match record {
            Record::Custom { kind, data: Some(data) } if kind == RECEIPT_KIND => {
                serde_json::from_str::<Value>(data).ok().as_ref().and_then(Receipt::from_json)
            }
            _ => None,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn receipt(id: &str, revision: &str) -> Receipt {
        Receipt {
            request_id: id.to_string(),
            revision: revision.to_string(),
            base_revision: "base".to_string(),
            items: 1,
            bytes: 8,
            actor: "session-a".to_string(),
            applied_at: 1_700_000_000,
        }
    }

    #[test]
    fn a_receipt_round_trips_through_the_log_record() {
        let original = receipt("req-1", "abcdef0123456789");
        let record = original.record();
        let Record::Custom { kind, data: Some(_) } = &record else {
            panic!("a receipt is a namespaced custom entry")
        };
        assert_eq!(kind, RECEIPT_KIND);
        assert_eq!(receipts_in(std::slice::from_ref(&record)), vec![original]);
    }

    #[test]
    fn a_receipt_without_optional_fields_still_restores() {
        let value = json!({"request": "req-2"});
        let restored = Receipt::from_json(&value).unwrap();
        assert_eq!(restored.request_id, "req-2");
        assert_eq!(restored.revision, "");
        assert_eq!(restored.actor, "");
    }

    #[test]
    fn the_ledger_reports_its_bound_and_what_it_evicted() {
        let mut ledger = Ledger::with_bound(2);
        ledger.record(receipt("a", "1"));
        ledger.record(receipt("b", "2"));
        assert_eq!(
            ledger.record(receipt("c", "3")).map(|r| r.request_id),
            Some("a".to_string())
        );
        assert_eq!(ledger.len(), 2);
        assert_eq!(ledger.get("a"), None);
        assert_eq!(ledger.get("c").map(|r| r.revision.as_str()), Some("3"));
        assert_eq!(ledger.report()["bound"], 2);
        assert_eq!(ledger.report()["evicted"], 1);
        assert_eq!(ledger.report()["retained"], 2);
    }

    #[test]
    fn a_staged_receipt_is_taken_only_by_the_state_record_it_belongs_to() {
        stage(Staged {
            base_revision: "before".to_string(),
            items: vec!["one".to_string()],
            receipt: receipt("req-3", "after"),
        });
        assert_eq!(take_receipt("before", &["other".to_string()]), None);
        // Consumed even on a mismatch: a stale stage must not wait for a
        // later append that happens to look like it.
        assert_eq!(take_receipt("before", &["one".to_string()]), None);
    }

    #[test]
    fn a_matching_append_takes_the_staged_receipt_once() {
        let items = vec!["one".to_string()];
        stage(Staged {
            base_revision: "before".to_string(),
            items: items.clone(),
            receipt: receipt("req-4", "after"),
        });
        assert_eq!(take_receipt("before", &items).map(|r| r.request_id), Some("req-4".into()));
        assert_eq!(take_receipt("before", &items), None);
    }
}