//! Feature-gated Glass-owned native browser engine kernel.
//!
//! The current implementation combines a deterministic, headless
//! fixture/data-URL kernel with a bounded asynchronous HTTP(S) document loader
//! and a bounded semantic interaction and form-control slice. It is not a browser-parity
//! implementation or a security boundary.

mod browsing_context;
mod config;
mod content_process;
mod css;
mod diagnostics;
mod dom;
mod engine;
mod error;
mod history;
mod interaction;
mod javascript;
mod layout;
mod lifecycle;
mod origin;
mod paint;
mod raster;
mod resource_loader;
mod runtime;
mod sandbox;
mod scheduler;
mod worker;

pub use browsing_context::{NATIVE_CONTEXT_ID, NativeBrowsingContext};
pub use config::{
    MAX_NATIVE_DOCUMENT_BYTES, MAX_NATIVE_DOM_DEPTH, MAX_NATIVE_FIXTURES,
    MAX_NATIVE_HISTORY_ENTRIES, MAX_NATIVE_NODES, MAX_NATIVE_SCHEDULER_TASKS,
    MAX_NATIVE_VIEWPORT_DIMENSION, NativeEngineConfig, NativeEngineLimits, NativeFixture, Viewport,
};
#[doc(hidden)]
pub use content_process::run_native_content_worker;
pub use css::NativeColor;
pub use css::{
    NativeBorderRadius, NativeBorderStyle, NativeTextDecorationSkipInk,
    NativeTextDecorationSkipSpaces, NativeTextDecorationStyle,
};
pub use diagnostics::{
    MAX_NATIVE_DIAGNOSTIC_DETAIL_BYTES, MAX_NATIVE_DIAGNOSTICS, NativeDiagnostic,
    NativeDiagnosticCode, NativeDiagnosticSource,
};
pub use dom::{NativeDocument, NativeNode, NativeNodeId, NativeNodeKind, NativeSemanticNode};
pub use engine::{
    NativeActionResult, NativeActionabilityReason, NativeDiagnosticsSnapshot,
    NativeEffectsSnapshot, NativeEngine, NativeEngineSnapshot, NativeInspectionSnapshot,
    NativePreflightAction, NativeTargetErrorKind, NativeTargetPreflight,
};
pub use error::{NativeEngineError, NativeWorkerFailureKind};
pub use history::{NativeHistory, NativeHistoryDirection, NativeHistoryEntry};
pub use interaction::{MAX_NATIVE_EFFECTS, NativeAction, NativeEffect, NativeEventKind};
pub(crate) use javascript::{
    NativePopupRequest, NativePostMessageRequest, NativeWindowCloseRequest,
};
pub use layout::{
    NativeLayoutBox, NativeLayoutSnapshot, NativePoint, NativeRect, NativeTextLayout,
};
pub use lifecycle::NativeLifecycleState;
pub use origin::NativeOrigin;
pub use paint::{
    MAX_NATIVE_DISPLAY_COMMANDS, NativeBorderPaint, NativeBorderPaintSide, NativeDisplayCommand,
    NativeDisplayList,
};
pub use raster::{
    MAX_NATIVE_OPACITY_GROUP_DEPTH, MAX_NATIVE_OPACITY_LAYER_PIXELS, MAX_NATIVE_SURFACE_PIXELS,
    NativeSurface,
};
pub use resource_loader::{NativeFetchResponse, NativeResource, NativeResourceLoader};
pub use runtime::{
    MAX_NATIVE_MICROTASKS, MAX_NATIVE_RUNTIME_TRACE, NativeCancellationToken, NativeMicrotask,
    NativeRuntime, NativeRuntimeState, NativeRuntimeTraceEvent, NativeRuntimeTraceKind,
};
pub use scheduler::{DeterministicClock, DeterministicScheduler, NativeTask, ScheduledTask};
pub use worker::{NativeRuntimeWorker, NativeRuntimeWorkerState};
