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

#[tokio::test]
async fn versioned_native_journey_corpus_runs_local_navigation_and_scroll_journey() {
    let corpus: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/native-journeys-v1.json"))
            .expect("native journey corpus must be valid JSON");
    assert_eq!(corpus["schemaVersion"], 1);
    let journey = corpus["journeys"]
        .as_array()
        .unwrap()
        .iter()
        .find(|journey| journey["id"] == "local-navigation-and-nested-scroll")
        .expect("local native journey is required");
    assert_eq!(journey["backend"], "native");

    let config = NativeEngineConfig::default()
        .with_initial_url("fixture://journey.test/")
        .with_fixture(
            "fixture://journey.test/",
            "<title>Journey landing</title><a id='detail-link' href='fixture://journey.test/detail'>Detail</a><div id='feed' style='height:32px;overflow-y:auto'><div style='height:160px'>Feed</div></div>",
        )
        .unwrap()
        .with_fixture(
            "fixture://journey.test/detail",
            "<title>Journey detail</title><main id='detail'>Detail content</main>",
        )
        .unwrap();
    let mut engine = NativeEngine::new(config).unwrap();
    engine.initialize_async().await.unwrap();

    // Resolve the action from the current DOM before acting, then use its
    // revision-bound reference so the corpus exercises the same stale-target
    // protection available to agents.
    let link = engine.resolve_target("id=detail-link").unwrap();
    let link_ref = format!("ref=r{}:n{}", engine.revision(), link.index());
    engine
        .action_async(NativeAction::Click { target: link_ref })
        .await
        .unwrap();
    assert_eq!(
        engine.evaluate_async("document.title").await.unwrap(),
        serde_json::json!("Journey detail")
    );
    engine.go_back_async().await.unwrap();
    assert_eq!(engine.snapshot().unwrap().url, "fixture://journey.test/");
    engine.go_forward_async().await.unwrap();
    assert_eq!(
        engine
            .evaluate_async("document.querySelector('#detail').textContent")
            .await
            .unwrap(),
        serde_json::json!("Detail content")
    );

    engine
        .navigate_async("fixture://journey.test/")
        .await
        .unwrap();
    let feed = engine.resolve_target("id=feed").unwrap();
    let feed_ref = format!("ref=r{}:n{}", engine.revision(), feed.index());
    engine
        .action_async(NativeAction::ScrollTarget {
            target: feed_ref,
            delta_x: 0,
            delta_y: 28,
        })
        .await
        .unwrap();
    assert_eq!(
        engine
            .evaluate_async("document.querySelector('#feed').scrollTop")
            .await
            .unwrap(),
        serde_json::json!(28)
    );
    engine.close_async().await.unwrap();
}

#[tokio::test]
async fn versioned_native_image_journey_opens_and_paints_full_image() {
    let corpus: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/native-journeys-v1.json"))
            .expect("native journey corpus must be valid JSON");
    let journey = corpus["journeys"]
        .as_array()
        .unwrap()
        .iter()
        .find(|journey| journey["id"] == "local-image-gallery-full-view")
        .expect("local image journey is required");
    assert_eq!(journey["backend"], "native");

    let image = native_test_png_data_url();
    let gallery = format!(
        "<title>Journey gallery</title><body style='margin:0'><a id='full-image-link' href='fixture://journey-images.test/full'><img id='thumbnail' src='{image}' style='display:block;width:2px;height:2px'></a></body>"
    );
    let full = format!(
        "<title>Full image</title><body style='margin:0'><img id='full-image' src='{image}' style='display:block;width:2px;height:2px'></body>"
    );
    let config = NativeEngineConfig::default()
        .with_initial_url("fixture://journey-images.test/gallery")
        .with_fixture("fixture://journey-images.test/gallery", gallery)
        .unwrap()
        .with_fixture("fixture://journey-images.test/full", full)
        .unwrap();
    let mut engine = NativeEngine::new(config).unwrap();
    engine.initialize_async().await.unwrap();
    assert_eq!(
        engine
            .evaluate_async("document.title === 'Journey gallery' && document.querySelector('#thumbnail').complete && document.querySelector('#thumbnail').naturalWidth === 2")
            .await
            .unwrap(),
        serde_json::json!(true),
    );

    let observed_revision = engine.revision();
    let link = engine.resolve_target("id=full-image-link").unwrap();
    let target = format!("ref=r{observed_revision}:n{}", link.index());
    engine
        .action_async(NativeAction::Click { target })
        .await
        .unwrap();
    assert_eq!(
        engine
            .evaluate_async("document.title === 'Full image' && document.querySelector('#full-image').complete && document.querySelector('#full-image').naturalWidth === 2 && document.querySelector('#full-image').naturalHeight === 2")
            .await
            .unwrap(),
        serde_json::json!(true),
    );

    let surface = engine.rasterize().unwrap();
    let has_decoded_pixel =
        (0..16).any(|y| (0..16).any(|x| surface.pixel(x, y) == Some([255, 0, 0, 255])));
    assert!(
        has_decoded_pixel,
        "full-image capture must contain a decoded red source pixel"
    );
    engine.close_async().await.unwrap();
}

#[tokio::test]
async fn versioned_native_disclosure_search_form_and_history_journey() {
    let corpus: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/native-journeys-v1.json"))
            .expect("native journey corpus must be valid JSON");
    let journey = corpus["journeys"]
        .as_array()
        .unwrap()
        .iter()
        .find(|journey| journey["id"] == "local-disclosure-search-and-results")
        .expect("local search journey is required");
    assert_eq!(journey["backend"], "native");

    let search = "<title>Search</title><button id='help-toggle' aria-expanded='false'>Help</button><p id='help-answer' hidden>Enter a search query to find results.</p><form id='search-form' action='/results' method='get'><input id='query' name='q' type='search' required><button id='search-submit' type='submit'>Search</button></form><script>document.querySelector('#help-toggle').addEventListener('click', () => { const button = document.querySelector('#help-toggle'); const answer = document.querySelector('#help-answer'); const open = button.getAttribute('aria-expanded') !== 'true'; button.setAttribute('aria-expanded', String(open)); answer.hidden = !open; });</script>";
    let results = "<title>Search results</title><main id='result'>Rust language results</main>";
    let config = NativeEngineConfig::default()
        .with_initial_url("fixture://journey-search.test/")
        .with_fixture("fixture://journey-search.test/", search)
        .unwrap()
        .with_fixture("fixture://journey-search.test/results?q=rust", results)
        .unwrap();
    let mut engine = NativeEngine::new(config).unwrap();
    engine.initialize_async().await.unwrap();

    let observed_revision = engine.revision();
    let toggle = engine.resolve_target("id=help-toggle").unwrap();
    engine
        .action_async(NativeAction::Click {
            target: format!("ref=r{observed_revision}:n{}", toggle.index()),
        })
        .await
        .unwrap();
    assert_eq!(
        engine
            .evaluate_async("document.querySelector('#help-toggle').getAttribute('aria-expanded') === 'true' && !document.querySelector('#help-answer').hidden && document.querySelector('#help-answer').textContent.includes('search')")
            .await
            .unwrap(),
        serde_json::json!(true),
    );

    let observed_revision = engine.revision();
    let query = engine.resolve_target("id=query").unwrap();
    engine
        .action_async(NativeAction::Type {
            target: format!("ref=r{observed_revision}:n{}", query.index()),
            text: "rust".into(),
        })
        .await
        .unwrap();
    let observed_revision = engine.revision();
    let submit = engine.resolve_target("id=search-submit").unwrap();
    engine
        .action_async(NativeAction::Click {
            target: format!("ref=r{observed_revision}:n{}", submit.index()),
        })
        .await
        .unwrap();
    assert_eq!(
        engine.snapshot().unwrap().url,
        "fixture://journey-search.test/results?q=rust"
    );
    assert_eq!(
        engine
            .evaluate_async("document.title === 'Search results' && document.querySelector('#result').textContent.includes('Rust') && location.search === '?q=rust'")
            .await
            .unwrap(),
        serde_json::json!(true),
    );

    engine.go_back_async().await.unwrap();
    assert_eq!(
        engine.snapshot().unwrap().url,
        "fixture://journey-search.test/"
    );
    assert_eq!(
        engine
            .evaluate_async("document.title === 'Search' && location.search === ''")
            .await
            .unwrap(),
        serde_json::json!(true),
    );
    engine.close_async().await.unwrap();
}

#[tokio::test]
async fn native_host_view_handles_large_local_anchor_collections_within_budget() {
    let anchors = (0..2_000)
        .map(|index| format!("<a href='/item/{index}' title='item {index}'>item</a>"))
        .collect::<String>();
    let html = format!(
        "<title>Large anchor fixture</title><input id='empty-upload' type='file'><button id='inline-event' onclick=\"window.inlineEventCount = (window.inlineEventCount || 0) + 1\">Run</button><main>{anchors}</main>"
    );
    let config = NativeEngineConfig::default()
        .with_initial_url("fixture://large-host-view.test/")
        .with_fixture("fixture://large-host-view.test/", &html)
        .unwrap();
    let mut engine = NativeEngine::new(config).unwrap();
    engine.initialize_async().await.unwrap();

    assert_eq!(
        engine
            .evaluate_async("document.links.length")
            .await
            .unwrap(),
        serde_json::json!(2_000),
    );
    assert_eq!(
        engine
            .evaluate_async("document.links[1999].getAttribute('href')")
            .await
            .unwrap(),
        serde_json::json!("/item/1999"),
    );
    assert_eq!(
        engine
            .evaluate_async("(() => { const files = document.querySelector('#empty-upload').files; return [files === document.querySelector('#empty-upload').files, files.length]; })()")
            .await
            .unwrap(),
        serde_json::json!([true, 0]),
    );
    assert_eq!(
        engine
            .evaluate_async("(() => { document.querySelector('#inline-event').click(); document.querySelector('#inline-event').setAttribute('onclick', 'window.inlineEventCount += 2'); document.querySelector('#inline-event').click(); return window.inlineEventCount; })()")
            .await
            .unwrap(),
        serde_json::json!(3),
    );
    engine.close_async().await.unwrap();
}

#[tokio::test]
async fn native_host_view_mixed_dom_with_retained_realm_fits_the_fixed_heap_budget() {
    let mut markup = String::from(
        "<title>Retained realm fixture</title><script>window.__retained = Array.from({length: 16_000}, (_, i) => String(i).padStart(5, '0') + 'x'.repeat(2040)); void 0;</script><main>",
    );
    for index in 0..3_000 {
        let element = match index % 5 {
            0 => format!("<a id='node-{index}' href='/item/{index}'>link</a>"),
            1 => format!("<span id='node-{index}' data-value='{index}'>text</span>"),
            2 => format!("<button id='node-{index}' type='button'>button</button>"),
            3 => format!("<div id='node-{index}' style='height:2px'>block</div>"),
            _ => format!("<input id='node-{index}' value='{index}'>"),
        };
        markup.push_str(&element);
    }
    markup.push_str("</main>");
    let config = NativeEngineConfig::default()
        .with_initial_url("fixture://retained-large-host-view.test/")
        .with_fixture("fixture://retained-large-host-view.test/", &markup)
        .unwrap();
    let mut engine = NativeEngine::new(config).unwrap();
    engine.initialize_async().await.unwrap();

    assert_eq!(
        engine.evaluate_async("[document.querySelectorAll('a').length, window.__retained.length, window.__retained[15999].length]").await.unwrap(),
        serde_json::json!([600, 16_000, 2_045]),
    );
    engine.close_async().await.unwrap();
}

#[tokio::test]
async fn native_host_view_dense_multirelational_links_keep_attributes_and_semantics() {
    let mut markup = String::from(
        "<title>Dense link fixture</title><style>a,span{display:inline;position:relative;color:#123456;font-size:13px;line-height:1.25;font-weight:400;text-align:start;text-decoration:none;white-space:normal;direction:ltr;visibility:visible;opacity:1;pointer-events:auto;margin-left:1px;padding-left:2px;border-width:1px;border-style:solid;border-color:#abc;overflow:visible;box-sizing:border-box}</style><main>",
    );
    let mediawiki_metadata = "x".repeat(259);
    for index in 0..5_500 {
        if index == 4_749 {
            markup.push_str(&format!(
                "<a rel='mw:WikiLink' href='/wiki/Target_{index}' about='#mwt{index}' typeof='mw:Transclusion' class='entry' id='link-{index}' data-mw='{mediawiki_metadata}'>item{index:03}</a>"
            ));
        } else {
            markup.push_str(&format!(
                "<a rel='mw:WikiLink' href='/wiki/Target_{index}' about='#mwt{index}' typeof='mw:Transclusion' class='entry' id='link-{index}'>item{index:03}</a>"
            ));
        }
    }
    for index in 0..1_000 {
        markup.push_str(&format!(
            "<span class='dense' id='aux-{index}'>value</span>"
        ));
    }
    markup.push_str("</main>");
    let config = NativeEngineConfig::default()
        .with_initial_url("fixture://dense-links.test/")
        .with_fixture("fixture://dense-links.test/", &markup)
        .unwrap();
    let mut engine = NativeEngine::new(config).unwrap();
    engine.initialize_async().await.unwrap();
    assert_eq!(
        engine
            .evaluate_async("[document.querySelectorAll('a').length, document.querySelector('#link-5499').href, document.querySelector('#link-5499').textContent, document.querySelector('#link-4749').getAttribute('data-mw').length]")
            .await
            .unwrap(),
        serde_json::json!([5_500, "fixture://dense-links.test/wiki/Target_5499", "item5499", 259]),
    );
    assert_eq!(
        engine
            .evaluate_async("(() => { const link = document.querySelector('#link-4749'); const text = link.firstChild; return [text.nodeType, text.parentNode === link, text.parentElement === link, text.isConnected, text.getRootNode() === document, text.nextSibling, text.previousSibling, text instanceof Text]; })()")
            .await
            .unwrap(),
        serde_json::json!([3, true, true, true, true, null, null, true]),
    );
    engine.close_async().await.unwrap();
}

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
