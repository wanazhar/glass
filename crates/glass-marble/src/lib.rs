//! Glass Marble: the smallest durable Glass-backed conversational harness.

pub mod conversation;
pub mod host;
pub mod normalize;
pub mod server;
pub mod state;
pub mod turn;

pub use conversation::{ApprovalInbox, Conversation};
pub use host::{Approval, HostPolicy};
pub use normalize::{InboundMessage, to_turn_input};
pub use server::{MarbleServer, TurnCommand};
pub use state::{ConversationRecord, PendingApproval, ProductState, TaskRecord, TranscriptEntry};
pub use turn::{MarbleReply, MarbleTurn, run_turn};

use std::error::Error;
use std::fmt;
use std::time::Duration;

/// Bounded wait for one runtime response, event, or tool exchange.
pub const TURN_TIMEOUT: Duration = Duration::from_secs(30);

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
