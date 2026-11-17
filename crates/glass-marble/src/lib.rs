//! Glass Marble: a self-hosted durable Glass collaborator.
//!
//! Marble owns the product surface (conversations, transcript, approvals,
//! events) while Glass Core owns the model loop, native coding tools, and the
//! durable journal. Mixed tool ownership keeps both responsibilities honest:
//! Core executes its own tools, and only `glass.*` integrations come back to
//! this process for an explicit human decision.

pub mod conversation;
pub mod events;
pub mod host;
pub mod normalize;
pub mod server;
pub mod state;
pub mod store;
pub mod tools;
pub mod turn;
pub mod worker;

pub use conversation::{ApprovalInbox, CancelToken, Conversation};
pub use events::{EventBus, TurnEvent};
pub use host::{Approval, HostPolicy};
pub use normalize::{InboundMessage, to_turn_input};
pub use server::{
    AccessPolicy, MarbleServer, MarbleServerParts, RequestHead, Response, STATIC_HTML,
};
pub use state::{ConversationRecord, PendingApproval, ProductState, TaskRecord, TranscriptEntry};
pub use store::{StateStore, default_data_dir, sessions_dir};
pub use tools::{ToolOutcome, ToolRouter};
pub use turn::{DEFAULT_TURN_TIMEOUT, MarbleReply, MarbleTurn, TurnOptions, run_turn};
pub use worker::{AcceptedTurn, TurnCommand, TurnRequest, spawn_turn_worker};

use std::error::Error;
use std::fmt;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// Bounded wait for one runtime response, event, or tool exchange.
pub const TURN_TIMEOUT: Duration = Duration::from_secs(30);

static REQUEST_SEQUENCE: AtomicU64 = AtomicU64::new(0);

/// Unique durable request id for one inbound message.
///
/// The runtime deduplicates submissions by `(conversationId, requestId)`, so a
/// constant or process-wide id would silently drop every turn after the first.
pub fn next_request_id() -> String {
    let sequence = REQUEST_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_nanos())
        .unwrap_or_default();
    format!("{nanos:x}-{sequence:x}")
}

#[derive(Debug)]
pub enum MarbleError {
    Io(std::io::Error),
    Protocol(String),
}

impl fmt::Display for MarbleError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => write!(formatter, "glass-marble runtime error: {error}"),
            Self::Protocol(message) => write!(formatter, "glass-marble protocol error: {message}"),
        }
    }
}

impl Error for MarbleError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn durable_request_ids_are_distinct() {
        let first = next_request_id();
        let second = next_request_id();
        assert_ne!(first, second);
        assert!(!first.is_empty());
    }
}
