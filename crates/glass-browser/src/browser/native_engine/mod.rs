//! Feature-gated Glass-owned native browser engine kernel.
//!
//! The current implementation combines a deterministic, headless
//! fixture/data-URL kernel with a narrow semantic interaction slice. It is not
//! a browser-parity implementation, network client, or security boundary.

mod browsing_context;
mod config;
mod css;
mod dom;
mod engine;
mod error;
mod history;
mod interaction;
mod layout;
mod lifecycle;
mod origin;
mod paint;
mod raster;
mod resource_loader;
mod scheduler;

pub use browsing_context::{NATIVE_CONTEXT_ID, NativeBrowsingContext};
pub use config::{
    MAX_NATIVE_DOCUMENT_BYTES, MAX_NATIVE_DOM_DEPTH, MAX_NATIVE_FIXTURES,
    MAX_NATIVE_HISTORY_ENTRIES, MAX_NATIVE_NODES, MAX_NATIVE_SCHEDULER_TASKS,
    MAX_NATIVE_VIEWPORT_DIMENSION, NativeEngineConfig, NativeEngineLimits, NativeFixture, Viewport,
};
pub use css::NativeColor;
pub use css::{NativeBorderRadius, NativeBorderStyle};
pub use dom::{NativeDocument, NativeNode, NativeNodeId, NativeNodeKind, NativeSemanticNode};
pub use engine::{NativeActionResult, NativeEffectsSnapshot, NativeEngine, NativeEngineSnapshot};
pub use error::NativeEngineError;
pub use history::{NativeHistory, NativeHistoryEntry};
pub use interaction::{MAX_NATIVE_EFFECTS, NativeAction, NativeEffect, NativeEventKind};
pub use layout::{NativeLayoutBox, NativeLayoutSnapshot, NativePoint, NativeRect};
pub use lifecycle::NativeLifecycleState;
pub use origin::NativeOrigin;
pub use paint::{
    MAX_NATIVE_DISPLAY_COMMANDS, NativeBorderPaint, NativeBorderPaintSide, NativeDisplayCommand,
    NativeDisplayList,
};
pub use raster::{MAX_NATIVE_SURFACE_PIXELS, NativeSurface};
pub use resource_loader::{NativeResource, NativeResourceLoader};
pub use scheduler::{DeterministicClock, DeterministicScheduler, NativeTask, ScheduledTask};
