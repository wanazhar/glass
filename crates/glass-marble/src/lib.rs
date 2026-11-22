//! Glass Marble: the always-on, self-hosted personal assistant.
//!
//! Marble is the assistant itself, not a surface bolted onto one: it keeps its
//! own conversations, transcript, approvals, memory, routines, presence, and
//! notification channels, and it stays reachable over the operator's tailnet.
//! Glass Core owns the model loop, native coding tools, and the durable journal;
//! mixed tool ownership keeps both responsibilities honest. Core executes its
//! own tools, and only `glass.*` integrations come back to this process for an
//! explicit decision.

pub mod control;
pub mod conversation;
pub mod crates;
pub mod events;
pub mod everyone;
pub mod failure;
pub mod host;
pub mod live;
pub mod lock;
pub mod memory;
pub mod normalize;
pub mod notify;
pub mod presence;
pub mod prism;
pub mod routine;
pub mod scheduler;
pub mod server;
pub mod service;
pub mod state;
pub mod store;
pub mod threads;
pub mod tls;
pub mod tools;
pub mod turn;
pub mod voice;
pub mod worker;

pub use control::{ControlEntry, ControlGrant, ControlProfile, ControlState, QueuedTurn, Verdict};
pub use conversation::{
    ApprovalInbox, CancelToken, Conversation, SteerRequest, default_assistant_instructions,
    marble_session_instructions,
};
pub use crates::{BinaryReach, CRATES_TOOL, CrateMap, WorkspaceCrate};
pub use events::{EventBus, TurnEvent};
pub use host::{Approval, HostPolicy};
pub use live::{LiveRecovery, LiveStore, LiveTurn};
pub use lock::DataDirLock;
pub use memory::{MemoryCommand, MemoryFact, MemoryState, MemorySuggestion, parse_command};
pub use normalize::{InboundMessage, to_turn_input};
pub use notify::{
    ApnsSettings, Delivery, DeliveryRecord, DeviceRegistration, NOTIFY_EVENTS, Notification,
    NotificationSettings, QuietHours, recent_deliveries,
};
pub use presence::{PresenceState, RuntimeHealth, Waiting};
pub use prism::{
    PRISM_LIMIT, PrismDispatch, PrismMarble, PrismPool, PrismPoolParts, PrismState, plan_dispatch,
};
pub use routine::{Routine, RoutineState, Schedule, parse_schedule};
pub use scheduler::{TICK, spawn_scheduler};
pub use server::{
    AccessPolicy, MarbleServer, MarbleServerParts, RequestHead, Response, STATIC_HTML,
};
pub use service::{InstallOutcome, ServicePlatform, ServiceSpec};
pub use state::{
    CONVERSATION_LIMIT, ConversationRecord, ENGINE_TOOL_LIMIT, ObservedTool, PendingApproval,
    ProductState, PruneSummary, RetentionState, TASK_LIMIT, TRANSCRIPT_LIMIT,
    TRANSCRIPT_PER_CONVERSATION, TaskRecord, TranscriptEntry,
};
pub use store::{StateStore, default_data_dir, sessions_dir};
pub use tools::{ToolOutcome, ToolRouter};
pub use turn::{DEFAULT_TURN_TIMEOUT, MarbleReply, MarbleTurn, TurnOptions, run_turn};
pub use voice::{VoiceSettings, spoken_form};
pub use worker::{
    AcceptedTurn, CancelRegistry, LaneFactory, SteerSink, TurnCommand, TurnInbox, TurnLane,
    TurnPool, TurnRequest, TurnSender, TurnWorkerGone, spawn_turn_pool,
};

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
