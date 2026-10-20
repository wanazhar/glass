#![cfg(feature = "native-engine")]

use base64::Engine as _;
use fs2::FileExt;
use futures_util::{SinkExt, StreamExt};
use gif::{Encoder, Frame, Repeat};
use glass_browser::browser::native_backend::NATIVE_ENGINE_BACKEND_ID;
use glass_browser::browser::native_engine::{
    MAX_NATIVE_DIAGNOSTIC_DETAIL_BYTES, MAX_NATIVE_DIAGNOSTICS, NativeAction, NativeBorderRadius,
    NativeBorderStyle, NativeColor, NativeDiagnosticCode, NativeDiagnosticSource,
    NativeDisplayCommand, NativeDocument, NativeEngine, NativeEngineConfig, NativeEngineError,
    NativeEngineLimits, NativeEventKind, NativeFile, NativeLifecycleState, NativeNodeId,
    NativeOrigin, NativePoint, NativePreflightAction, NativeRect, NativeRuntimeState,
    NativeRuntimeTraceKind, NativeSurface, NativeSvgStrokeShape, NativeTextDecorationSkipInk,
    NativeTextDecorationSkipSpaces, NativeTextDecorationStyle, NativeWorkerFailureKind, Viewport,
};
use glass_browser::browser::session::{
    Cookie, ExtractionField, ExtractionKind, IntentConfidence, IntentConstraints,
    SemanticIntentAction, SemanticIntentExecutionRequest, SemanticIntentRequest,
    SemanticObservationLevel, SemanticResolutionPolicy, StructuredExtractionRequest,
    VerificationPredicate, VisualCaptureOptions, VisualClip, VisualFormat, WaitCondition,
};
use glass_browser::browser::{NativeDialogController, NativeDialogResolution, NativePendingDialog};
use glass_browser::browser_backend::{
    ActionRequest, BROWSER_BACKEND_SCHEMA_VERSION, BackendSelectionRequest,
    BrowserBackendDispatcher, BrowserBackendError, BrowserCapability, CaptureFormat,
    CaptureRequest, CertificationLevel, ClickModifiers, EffectsRequest, EvidenceLevel,
    EvidenceRequest, NavigationRequest, PromptDecision, ScriptRequest, SemanticAction,
    StorageOperation, StorageRequest, StorageScope, SupportLevel,
};
use glass_browser::{BackendFactory, BrowserRuntime, BrowserRuntimeSession, NativeEngineBackend};
use glass_browser::{
    BrowserSession, EvidenceSource, ExtractionRequest, GlassTask, TaskAmbiguityPolicy, TaskKind,
    TaskLimits, TaskRevisionPolicy, TaskRiskClass, TaskScope, WebIrAction, WebIrEntityKind,
};
use sha2::{Digest, Sha256, Sha384};
use std::borrow::Cow;
use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, OpenOptions};
use std::io::Cursor;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, OnceLock};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{Mutex, oneshot};
use tokio_tungstenite::{
    accept_async, accept_hdr_async,
    tungstenite::{
        Message,
        handshake::server::{Request as WebSocketRequest, Response as WebSocketResponse},
        http::{
            HeaderValue,
            header::{COOKIE, SET_COOKIE},
        },
    },
};
use url::Url;

fn native_content_process_test_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

/// Keep small viewport fixtures independent of the live document's default
/// `body` margin when their expected geometry is expressed from the viewport
/// origin.
fn native_html_without_body_margin(markup: &str) -> String {
    format!("<style>body {{ margin: 0px; }}</style>{markup}")
}

async fn wait_for_modal_dialog(
    controller: &NativeDialogController,
    expected_type: &str,
) -> NativePendingDialog {
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            if let Some(dialog) = controller
                .pending_dialog()
                .expect("dialog control state must remain available")
                && dialog.dialog.dialog_type == expected_type
            {
                break dialog;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .unwrap_or_else(|_| panic!("native modal controller did not expose {expected_type}"))
}

async fn wait_for_shared_worker_messages(
    session: &BrowserSession,
    expression: &str,
) -> serde_json::Value {
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            let value = session.script(expression).await.unwrap().value;
            if value.as_array().is_some_and(|events| !events.is_empty()) {
                return value;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap_or_else(|_| panic!("SharedWorker page event did not arrive: {expression}"))
}

async fn read_http_request_bytes(stream: &mut TcpStream) -> Vec<u8> {
    let mut request = Vec::new();
    let mut chunk = [0_u8; 4096];
    loop {
        let read = stream.read(&mut chunk).await.unwrap();
        if read == 0 {
            break;
        }
        request.extend_from_slice(&chunk[..read]);
        let Some(header_end) = request.windows(4).position(|window| window == b"\r\n\r\n") else {
            continue;
        };
        let header_end = header_end + 4;
        let content_length = String::from_utf8_lossy(&request[..header_end])
            .lines()
            .find_map(|line| {
                line.split_once(':').and_then(|(name, value)| {
                    name.eq_ignore_ascii_case("content-length")
                        .then(|| value.trim().parse::<usize>().unwrap())
                })
            })
            .unwrap_or_default();
        let chunked = String::from_utf8_lossy(&request[..header_end])
            .lines()
            .find_map(|line| {
                line.split_once(':').and_then(|(name, value)| {
                    name.eq_ignore_ascii_case("transfer-encoding").then(|| {
                        value
                            .split(',')
                            .any(|part| part.trim().eq_ignore_ascii_case("chunked"))
                    })
                })
            })
            .unwrap_or(false);
        if chunked {
            let mut offset = header_end;
            let mut decoded = request[..header_end].to_vec();
            let mut complete = false;
            while let Some(line_end) = request[offset..]
                .windows(2)
                .position(|window| window == b"\r\n")
            {
                let line_end = offset + line_end;
                let size = usize::from_str_radix(
                    String::from_utf8_lossy(&request[offset..line_end])
                        .split(';')
                        .next()
                        .unwrap()
                        .trim(),
                    16,
                )
                .unwrap();
                let body_start = line_end + 2;
                let body_end = body_start.saturating_add(size);
                if request.len() < body_end.saturating_add(2) {
                    break;
                }
                decoded.extend_from_slice(&request[body_start..body_end]);
                offset = body_end + 2;
                if size == 0 {
                    complete = true;
                    break;
                }
            }
            if complete {
                return decoded;
            }
        } else if request.len() >= header_end.saturating_add(content_length) {
            break;
        }
    }
    request
}

async fn read_http_request(stream: &mut TcpStream) -> String {
    String::from_utf8_lossy(&read_http_request_bytes(stream).await).into_owned()
}

fn find_element_with_attribute(
    document: &NativeDocument,
    element_name: &str,
    attribute_name: &str,
    attribute_value: &str,
) -> NativeNodeId {
    let mut pending = vec![document.root()];
    while let Some(id) = pending.pop() {
        let node = document.node(id).unwrap();
        if node.element_name() == Some(element_name)
            && node.attribute(attribute_name) == Some(attribute_value)
        {
            return id;
        }
        pending.extend(node.children().iter().copied());
    }
    panic!("missing {element_name} element with {attribute_name}={attribute_value:?}");
}

/// Match the 8 MiB thread-stack contract used by Glass's resident browser worker.
// Native layout, font rasterization, and script evaluation can recurse well
// beyond the small stack Cargo gives a test thread. Keep those tests on this
// explicit worker stack instead of hiding a production deadline workaround.
fn run_native_browser_worker_test(test: impl FnOnce(tokio::runtime::Runtime) + Send + 'static) {
    std::thread::Builder::new()
        .name("native-browser-worker-test".into())
        .stack_size(8 * 1024 * 1024)
        .spawn(move || {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .thread_stack_size(4 * 1024 * 1024)
                .enable_all()
                .build()
                .expect("native browser test runtime should build");
            test(runtime);
        })
        .expect("native browser test thread should start")
        .join()
        .expect("native browser test thread should not panic");
}

#[path = "native_engine_tests/support/media_fixtures.rs"]
mod media_fixtures;
use media_fixtures::*;
#[path = "native_engine_tests/support/runtime_lifecycle.rs"]
mod runtime_lifecycle;
use runtime_lifecycle::*;
#[path = "native_engine_tests/support/service_worker_scenarios.rs"]
mod service_worker_scenarios;
use service_worker_scenarios::*;
#[path = "native_engine_tests/support/session_and_navigation_scenarios.rs"]
mod session_and_navigation_scenarios;
use session_and_navigation_scenarios::*;
#[path = "native_engine_tests/support/dom_and_event_fixtures.rs"]
mod dom_and_event_fixtures;
use dom_and_event_fixtures::*;

#[path = "native_engine_tests/browser_behavior.rs"]
mod browser_behavior;
#[path = "native_engine_tests/content_process.rs"]
mod content_process;
#[path = "native_engine_tests/events_and_interaction.rs"]
mod events_and_interaction;
#[path = "native_engine_tests/file_documents.rs"]
mod file_documents;
#[path = "native_engine_tests/local_documents_and_scripts.rs"]
mod local_documents_and_scripts;
#[path = "native_engine_tests/network_and_workers.rs"]
mod network_and_workers;
#[path = "native_engine_tests/runtime_and_backend.rs"]
mod runtime_and_backend;
#[path = "native_engine_tests/style_layout_rendering.rs"]
mod style_layout_rendering;
