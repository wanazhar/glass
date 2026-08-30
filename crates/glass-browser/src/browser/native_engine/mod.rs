//! Feature-gated Glass-owned native browser engine kernel.
//!
//! Phase 1 is a deterministic, headless fixture/data-URL engine. It is not a
//! browser-parity implementation, network client, or security boundary.

mod browsing_context;
mod config;
mod dom;
mod engine;
mod error;
mod history;
mod lifecycle;
mod origin;
mod resource_loader;
mod scheduler;

pub use browsing_context::{NATIVE_CONTEXT_ID, NativeBrowsingContext};
pub use config::{
    MAX_NATIVE_DOCUMENT_BYTES, MAX_NATIVE_DOM_DEPTH, MAX_NATIVE_FIXTURES,
    MAX_NATIVE_HISTORY_ENTRIES, MAX_NATIVE_NODES, MAX_NATIVE_SCHEDULER_TASKS,
    MAX_NATIVE_VIEWPORT_DIMENSION, NativeEngineConfig, NativeEngineLimits, NativeFixture, Viewport,
};
pub use dom::{NativeDocument, NativeNode, NativeNodeId, NativeNodeKind, NativeSemanticNode};
pub use engine::{NativeEngine, NativeEngineSnapshot};
pub use error::NativeEngineError;
pub use history::{NativeHistory, NativeHistoryEntry};
pub use lifecycle::NativeLifecycleState;
pub use origin::NativeOrigin;
pub use resource_loader::{NativeResource, NativeResourceLoader};
pub use scheduler::{DeterministicClock, DeterministicScheduler, NativeTask, ScheduledTask};
