//! Glass-owned native browser engine kernel.
//!
//! The current implementation combines a deterministic, headless
//! fixture/data-URL kernel with a bounded asynchronous HTTP(S) document loader
//! and a bounded semantic interaction and form-control slice. Product builds
//! enable this kernel by default; its remaining conformance and isolation
//! gates are tracked by issue #40.

mod browsing_context;
mod cancellation;
mod config;
mod content_process;
mod css;
mod diagnostics;
mod dialog;
mod dom;
mod engine;
mod environment;
mod error;
mod fetch_stream;
mod font;
mod history;
mod html_encoding;
mod html_parser;
mod image;
mod interaction;
mod javascript;
mod layout;
mod lifecycle;
mod module_import_map;
mod origin;
mod paint;
mod raster;
mod resource_loader;
mod runtime;
mod sandbox;
mod scheduler;
mod service_worker;
mod worker;

use std::sync::atomic::{AtomicU64, Ordering};

// Keep this primitive compatible with the workspace MSRV (Rust 1.88).
// `AtomicU64::fetch_update` is renamed in newer stable compilers, while the
// checked increment semantics remain the same.
#[allow(deprecated)]
fn fetch_next_id(counter: &AtomicU64, success: Ordering, failure: Ordering) -> Result<u64, u64> {
    counter.fetch_update(success, failure, |current| current.checked_add(1))
}

pub use browsing_context::{NATIVE_CONTEXT_ID, NativeBrowsingContext};
#[doc(hidden)]
pub use cancellation::NativeNavigationCancellation;
pub use config::{
    MAX_NATIVE_DOCUMENT_BYTES, MAX_NATIVE_DOM_DEPTH, MAX_NATIVE_FILE_ROOT_BYTES,
    MAX_NATIVE_FILE_ROOTS, MAX_NATIVE_FIXTURES, MAX_NATIVE_HISTORY_ENTRIES, MAX_NATIVE_NODES,
    MAX_NATIVE_SCHEDULER_TASKS, MAX_NATIVE_VIEWPORT_DIMENSION, NativeEngineConfig,
    NativeEngineLimits, NativeFixture, Viewport,
};
#[doc(hidden)]
pub use content_process::run_native_content_worker;
#[doc(hidden)]
pub use content_process::{
    MAX_NATIVE_EVENTSOURCE_CONNECTIONS, MAX_NATIVE_EVENTSOURCE_RECONNECTS,
    MAX_NATIVE_WEBSOCKET_EVENTS, NATIVE_EVENTSOURCE_INITIAL_RETRY, NATIVE_EVENTSOURCE_MAX_RETRY,
    NATIVE_WEBSOCKET_CONNECT_TIMEOUT, NativeContentAsyncEffectNotification,
    NativeEventSourceParser, NativeWebSocketEvent, native_websocket_request,
    parse_event_source_chunk, validate_websocket_protocols, websocket_event_csp_violations,
    websocket_event_payload,
};
pub use css::NativeColor;
pub use css::{
    NativeBorderRadius, NativeBorderStyle, NativeTextDecorationSkipInk,
    NativeTextDecorationSkipSpaces, NativeTextDecorationStyle,
};
pub use diagnostics::{
    MAX_NATIVE_DIAGNOSTIC_DETAIL_BYTES, MAX_NATIVE_DIAGNOSTICS, NativeDiagnostic,
    NativeDiagnosticCode, NativeDiagnosticSource,
};
#[doc(hidden)]
pub use dialog::NativeDialogControlPlane;
pub use dialog::{
    NATIVE_BEFOREUNLOAD_MESSAGE, NATIVE_DIALOG_TEXT_LIMIT_BYTES, NativeDialogController,
    NativeDialogResolution, NativePendingDialog,
};
#[doc(hidden)]
pub use dom::NativeNodeSubtreeTransfer;
pub use dom::{NativeDocument, NativeNode, NativeNodeId, NativeNodeKind, NativeSemanticNode};
#[doc(hidden)]
pub use engine::NativeAsyncEffectTurn;
#[doc(hidden)]
pub use engine::parse_point_target;
pub use engine::{
    NativeActionResult, NativeActionabilityReason, NativeDiagnosticsSnapshot,
    NativeEffectsSnapshot, NativeEngine, NativeEngineSnapshot, NativeInspectionSnapshot,
    NativePageMetadata, NativePreflightAction, NativeScrollTargetSnapshot, NativeTargetErrorKind,
    NativeTargetPreflight,
};
pub use error::{NativeEngineError, NativeWorkerFailureKind};
pub use font::{NativeFontRun, NativeGlyph, NativeGlyphComposite};
pub use history::{NativeHistory, NativeHistoryDirection, NativeHistoryEntry};
pub use interaction::{
    MAX_NATIVE_EFFECTS, MAX_NATIVE_FILE_BYTES, MAX_NATIVE_FILE_COUNT, MAX_NATIVE_FILE_TOTAL_BYTES,
    NativeAction, NativeEffect, NativeEventKind, NativeFile,
};
#[doc(hidden)]
pub use javascript::{
    MAX_NATIVE_DIALOG_TEXT_BYTES, MAX_NATIVE_WEBSOCKET_CLOSE_REASON_BYTES,
    MAX_NATIVE_WEBSOCKET_MESSAGE_BYTES, MAX_NATIVE_WORKER_MESSAGES, NativeCookieChange,
    NativeCookieProfileEntry, NativeFrameScriptBinding, NativeFrameScriptContext,
    NativeFrameScriptRequest, NativeFrameScriptWindow, NativeMessagePortTransfer,
    NativePageMessagePortCommand, NativePopupRequest, NativePostMessageRequest,
    NativeScriptCommand, NativeServiceWorkerCacheBatchEntry, NativeServiceWorkerClientMessage,
    NativeServiceWorkerOpenWindowRequest, NativeSharedWorkerCreateRequest,
    NativeSharedWorkerStorageKey, NativeWindowCloseRequest, NativeWindowNavigationRequest,
    NativeWindowProxyUpdate, NativeWorkerEventSourceCommand, NativeWorkerMessage,
    NativeWorkerRegistry, NativeWorkerWebSocketCommand, synchronize_service_worker_client_leases,
    validate_message_port_transfers, validate_page_message_port_command,
};
pub use layout::{
    NativeLayoutBox, NativeLayoutSnapshot, NativePoint, NativeRect, NativeSvgSubpath,
    NativeTextLayout,
};
pub use lifecycle::NativeLifecycleState;
pub use origin::NativeOrigin;
pub use paint::{
    MAX_NATIVE_DISPLAY_COMMANDS, NativeBorderPaint, NativeBorderPaintSide, NativeDisplayCommand,
    NativeDisplayList, NativeSvgStrokeShape,
};
pub use raster::{
    MAX_NATIVE_OPACITY_GROUP_DEPTH, MAX_NATIVE_OPACITY_LAYER_PIXELS, MAX_NATIVE_SURFACE_PIXELS,
    NativeSurface,
};
#[doc(hidden)]
pub use resource_loader::{
    NativeAudioClip, NativeCookieJar, NativeCspViolation, NativeModuleResourceType,
    NativeNavigationMethod, NativeNavigationRequest, NativeRequestBody,
    schedule_native_csp_report_deliveries, validate_target_navigation_payload,
};
pub use resource_loader::{NativeFetchResponse, NativeResource, NativeResourceLoader};
pub use runtime::{
    MAX_NATIVE_MICROTASKS, MAX_NATIVE_RUNTIME_TRACE, NativeCancellationToken, NativeMicrotask,
    NativeRuntime, NativeRuntimeState, NativeRuntimeTraceEvent, NativeRuntimeTraceKind,
};
pub use scheduler::{DeterministicClock, DeterministicScheduler, NativeTask, ScheduledTask};
pub use worker::{NativeRuntimeWorker, NativeRuntimeWorkerState};
