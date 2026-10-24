// This module passes one explicit, bounded content-process turn through a
// set of ownership queues. The argument count and tuple-shaped response
// helpers preserve that protocol boundary; aliases or a mutable context
// object would obscure which state crosses the process boundary.
#![allow(clippy::too_many_arguments, clippy::type_complexity)]

use super::browsing_context::NATIVE_CONTEXT_ID;
use super::cancellation::NativeNavigationCancellation;
use super::config::{
    MAX_NATIVE_NODES, NativeEngineLimits, Viewport, is_file_url, is_network_url,
    validate_context_id, validate_url_text, validate_window_name, without_fragment,
};
use super::css::{
    FontWeightValue, NativeFontFaceSource, absolutize_stylesheet_urls, css_import_matches,
    decode_css_url_value, static_css_imports,
};
use super::dialog::{NativeDialogControlPlane, NativeDialogResolution, NativeDialogWait};
use super::dom::{
    MAX_NATIVE_VIDEO_POSTERS, NativeCheckableKind, NativeDocument, NativeDocumentWire,
    NativeImageFrameWire, NativeImageResourceWire, NativeNodeId, NativePageImportMapSource,
    NativePageScriptSource, NativePageScriptTiming, image_from_wire,
};
use super::environment::NativeEnvironmentOverrides;
use super::error::{NativeEngineError, NativeWorkerFailureKind};
use super::fetch_stream::{
    MAX_NATIVE_FETCH_UPLOAD_CHUNKS, NativeFetchStreamCommand, NativeFetchStreamConnection,
    NativeFetchStreamEvent, NativeFetchUploadCommand, NativeFetchUploadConnection,
    NativeFetchUploadEvent, spawn_native_fetch_bytes_stream, spawn_native_fetch_stream,
    spawn_native_fetch_upload_source, spawn_native_fetch_upload_stream,
};
use super::font::{
    MAX_NATIVE_FONT_BYTES, MAX_NATIVE_FONT_FACES, NativeFontBook, NativeFontFaceResource,
};
use super::image::{MAX_NATIVE_IMAGE_TRANSFER_BYTES, NativeImage};
use super::interaction::{
    MAX_NATIVE_EFFECTS, MAX_NATIVE_FORM_BODY_BYTES, NativeEventKind, NativeFile,
    validate_native_edit_key, validate_native_key,
};
use super::javascript::{
    MAX_NATIVE_COOKIE_PROFILE_BYTES, MAX_NATIVE_COOKIE_PROFILE_ENTRIES,
    MAX_NATIVE_DIALOG_TEXT_BYTES, MAX_NATIVE_DIALOGS, MAX_NATIVE_EVENTSOURCE_FIELD_BYTES,
    MAX_NATIVE_EVENTSOURCE_MESSAGE_BYTES, MAX_NATIVE_HISTORY_STATE_BYTES,
    MAX_NATIVE_INDEXED_DB_CHANGES, MAX_NATIVE_MODULE_IMPORTS, MAX_NATIVE_POST_MESSAGE_BYTES,
    MAX_NATIVE_SCRIPT_BYTES, MAX_NATIVE_WEBSOCKET_CLOSE_REASON_BYTES,
    MAX_NATIVE_WEBSOCKET_MESSAGE_BYTES, MAX_NATIVE_WEBSOCKET_PROTOCOL_BYTES,
    MAX_NATIVE_WEBSOCKET_PROTOCOLS, MAX_NATIVE_WORKER_MESSAGES, MAX_NATIVE_XHR_TIMEOUT_MS,
    NATIVE_JSON_MODULE_NAME_PREFIX, NativeCookieChange, NativeDialog, NativeDialogHandler,
    NativeDynamicPageScriptRequest, NativeFrameScriptBinding, NativeFrameScriptContext,
    NativeFrameScriptRequest, NativeFrameScriptWindow, NativeHashChangeEvent,
    NativeIndexedDbChange, NativeIndexedDbState, NativeJavaScriptRuntime,
    NativeMessagePortPageMessage, NativePageEventBatch, NativePageEventContext,
    NativePageMessagePortCommand, NativePageScript, NativePageScriptCommandQueues,
    NativePageScriptContext, NativePageScriptResult, NativePopupRequest, NativePostMessageRequest,
    NativeScriptCommand, NativeScriptEvaluation, NativeServiceWorkerClientMessage,
    NativeServiceWorkerClientState, NativeServiceWorkerOpenWindowRequest,
    NativeSharedWorkerStorageKey, NativeStorageEvent, NativeSyncXhrParentBroker,
    NativeWebStorageState, NativeWindowCloseRequest, NativeWindowNavigationRequest,
    NativeWindowProxyUpdate, NativeWorkerEventSourceCommand, NativeWorkerMessage,
    NativeWorkerRegistry, NativeWorkerWebSocketCommand, apply_document_commands_with_font_face_ack,
    apply_page_script_evaluation, diff_indexed_db_changes, execute_dynamic_page_scripts,
    execute_page_scripts, host_click_event_batch_with_modifiers, host_event_batch,
    host_event_batch_at, host_key_event_batch, host_key_event_batch_with_modifiers,
    host_submit_event_batch, install_native_sync_xhr_parent_broker, load_indexed_db_profile,
    load_service_worker_cache_profile, load_service_worker_registration_profiles,
    load_web_storage_profile, migrate_cookie_profile_from_web_storage, native_module_loader_name,
    order_page_scripts, page_script_sources_to_scripts, resolve_module_request_url,
    save_content_web_storage_profile, save_service_worker_cache_profile, save_web_storage_profile,
    static_module_requests, storage_key, validate_message_port_transfers,
    validate_native_message_payload, validate_native_object_url_transfers,
};
use super::layout::NativePoint;
use super::module_import_map::NativeModuleImportMap;
use super::origin::NativeOrigin;
use super::resource_loader::{
    MAX_NATIVE_CSP_POLICIES, MAX_NATIVE_CSP_VIOLATIONS, MAX_NATIVE_MEDIA_BYTES,
    MAX_NATIVE_RESPONSE_HEADER_BYTES, MAX_NATIVE_RESPONSE_HEADER_NAME_BYTES,
    MAX_NATIVE_RESPONSE_HEADER_VALUE_BYTES, MAX_NATIVE_RESPONSE_HEADERS, NativeCorsMode,
    NativeCspViolation, NativeFetchCacheMode, NativeFetchCredentialsMode, NativeFetchMethod,
    NativeFetchRedirectMode, NativeFetchReferrerPolicy, NativeFetchRequest, NativeFetchResponse,
    NativeFetchResponseStream, NativeMediaMetadata, NativeModuleResourceType,
    NativeNavigationMethod, NativeNavigationPolicyKind, NativeNavigationRequest,
    NativeObjectUrlResource, NativeRequestBody, NativeResource, NativeResourceLoader,
    NativeScriptLoadRequest, NativeScriptResource, NativeStylesheetResource, NativeSubresourceKind,
    NativeWebSocketTarget, resolve_subresource_url, schedule_native_csp_report_deliveries,
    validate_target_navigation_payload,
};
#[cfg(windows)]
use super::sandbox::NativeContentSandbox;
use super::sandbox::prepare_worker_command;
use super::service_worker::{
    NativeServiceWorkerFetchCompletion, NativeServiceWorkerFetchOutcome,
    NativeServiceWorkerFetchTaskResult, NativeServiceWorkerNavigationOutcome,
    NativeServiceWorkerNavigationPreloadResult, NativeServiceWorkerRegistrationRequest,
    NativeServiceWorkerRegistry,
};
use base64::Engine as _;
use futures_util::{SinkExt, StreamExt};
use serde::Serialize;
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::future::Future;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::process::{Child, ChildStdin};
use tokio::sync::mpsc;
use tokio::time::{sleep, timeout};
use tokio_tungstenite::{
    connect_async,
    tungstenite::{Message, client::IntoClientRequest},
};
use url::Url;

// File-input mutations carry bounded in-memory file objects through the same
// document snapshot channel. Keep the channel finite while leaving room for
// the base64 envelope and the rest of the document state.
const MAX_CONTENT_IPC_FRAME_BYTES: usize = 16 * 1024 * 1024;
const MAX_CONTENT_BROKERED_SCRIPT_URLS: usize = 4096;
// One in-flight frame is enough for an event notification or a request reply;
// limiting this to one avoids buffering many maximum-sized IPC responses.
const MAX_CONTENT_PROCESS_OUTPUT_FRAMES: usize = 1;
const MAX_CONTENT_ASYNC_EFFECT_NOTIFICATIONS: usize = MAX_CONTENT_PROCESS_OUTPUT_FRAMES;
const MAX_CONTENT_DOCUMENT_WIRE_BYTES: usize = 16 * 1024 * 1024;
const MAX_CONTENT_DOCUMENT_COOKIE_BYTES: usize =
    MAX_NATIVE_COOKIE_PROFILE_BYTES * MAX_NATIVE_COOKIE_PROFILE_ENTRIES;
const CONTENT_WORKER_PROTOCOL_VERSION: u64 = 33;
// Match the browser integration's bounded 60-second navigation budget. The
// outer caller still owns its shorter operation deadline and cancellation;
// this inner guard must not silently cut an explicitly longer navigation in
// half before that caller's deadline.
const CONTENT_PROCESS_LOAD_TIMEOUT: Duration = Duration::from_secs(60);
// Async owner turns can await a parent-brokered Fetch. Keep their independent
// network/effect budget at 30 seconds rather than extending it with document
// navigation.
pub(crate) const CONTENT_PROCESS_ASYNC_EFFECT_TIMEOUT: Duration = Duration::from_secs(30);
const CONTENT_PROCESS_MUTATION_TIMEOUT: Duration = Duration::from_secs(5);
pub(crate) const CONTENT_PROCESS_SCRIPT_TIMEOUT: Duration = Duration::from_secs(5);
const MAX_CONTENT_EVENT_LOOP_TURNS: usize = MAX_NATIVE_EFFECTS;
const MAX_NATIVE_COOKIE_CHANGE_BATCH: usize = MAX_NATIVE_EFFECTS;
pub const NATIVE_WEBSOCKET_CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
const NATIVE_WEBSOCKET_POLL_INTERVAL: Duration = Duration::from_millis(10);
pub const MAX_NATIVE_WEBSOCKET_EVENTS: usize = MAX_NATIVE_EFFECTS;
pub const NATIVE_EVENTSOURCE_INITIAL_RETRY: Duration = Duration::from_secs(3);
pub const NATIVE_EVENTSOURCE_MAX_RETRY: Duration = Duration::from_secs(30);
pub const MAX_NATIVE_EVENTSOURCE_RECONNECTS: usize = MAX_NATIVE_EFFECTS;
pub const MAX_NATIVE_EVENTSOURCE_CONNECTIONS: usize = MAX_NATIVE_EFFECTS;
const MAX_CONTENT_STYLESHEETS: usize = 16;
const MAX_CONTENT_STYLESHEET_BYTES: usize = 512 * 1024;
const MAX_CONTENT_IMAGES: usize = MAX_NATIVE_VIDEO_POSTERS;
const MAX_CONTENT_MEDIA: usize = 64;
const MAX_CONTENT_FRAME_SOURCES: usize = 64;
const MAX_CONTENT_NAVIGATION_SOURCES: usize = 64;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum NativeContentTaskSource {
    Networking,
    WebSocket,
    FetchStream,
    FetchUpload,
    EventSource,
    Timer,
}

enum NativeContentProcessInput {
    Request(Result<Vec<u8>, NativeEngineError>),
    ServiceWorkerLifetimeFetch(Option<Box<NativeServiceWorkerFetchTaskResult>>),
    WorkerTimer,
}

pub(crate) struct NativeContentFetchBroker<'a> {
    request_id: u64,
    captured_load_fetches: bool,
    owner: NativeContentCookieOwner,
    runtime: Option<&'a NativeJavaScriptRuntime>,
    stdout: &'a mut tokio::io::Stdout,
    ipc_requests: &'a mut mpsc::Receiver<Result<Vec<u8>, NativeEngineError>>,
    cancelled_parent_fetches: &'a mut BTreeSet<(u64, u32)>,
    document_cookie_projection: &'a mut Option<String>,
    next_content_resource_fetch_id: u32,
    page_meta_content_security_policies: Vec<String>,
}

type FetchBrokerResult = (
    Result<NativeFetchResponse, NativeEngineError>,
    String,
    Vec<NativeCspViolation>,
);

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(crate) struct NativeContentCookieOwner {
    pub(crate) context_id: String,
    pub(crate) frame_id: String,
    pub(crate) generation: u32,
    pub(crate) document_url: String,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(crate) struct NativeContentCookieWrite {
    pub(crate) owner: NativeContentCookieOwner,
    pub(crate) value: String,
}

#[derive(Clone)]
struct NativeParentEventSourceOpen {
    stream_id: u64,
    origin: String,
    csp_violations: Vec<NativeCspViolation>,
}

enum NativeParentEventSourceOpenResult {
    Opened(NativeParentEventSourceOpen),
    Failed {
        message: String,
        csp_violations: Vec<NativeCspViolation>,
    },
}

enum NativeParentEventSourceRead {
    Chunk(Vec<u8>),
    Pending,
    End,
}

struct NativeParentEventSourceFailure {
    message: String,
    csp_violations: Vec<NativeCspViolation>,
}

#[derive(Clone)]
struct NativeParentWebSocketOpen {
    stream_id: u64,
    protocol: String,
    csp_violations: Vec<NativeCspViolation>,
}

enum NativeParentWebSocketOpenResult {
    Opened(NativeParentWebSocketOpen),
    Failed {
        message: String,
        csp_violations: Vec<NativeCspViolation>,
    },
}

enum NativeParentWebSocketRead {
    Pending,
    Event(NativeWebSocketEvent),
}

impl NativeContentTaskSource {
    fn next(self) -> Self {
        match self {
            Self::Networking => Self::WebSocket,
            Self::WebSocket => Self::FetchStream,
            Self::FetchStream => Self::FetchUpload,
            Self::FetchUpload => Self::EventSource,
            Self::EventSource => Self::Timer,
            Self::Timer => Self::Networking,
        }
    }
}

struct NativePendingUploadFetch {
    task: tokio::task::JoinHandle<(
        Result<NativeFetchResponseStream, NativeEngineError>,
        NativeResourceLoader,
    )>,
}

struct NativePendingControlledUpload {
    task: tokio::task::JoinHandle<Result<Vec<u8>, NativeEngineError>>,
    parent_brokered: bool,
    document_url: String,
    href: String,
    method: NativeFetchMethod,
    headers: BTreeMap<String, String>,
    content_type: Option<String>,
    cors_mode: NativeCorsMode,
    redirect_mode: NativeFetchRedirectMode,
    cache_mode: NativeFetchCacheMode,
    timeout: Option<Duration>,
    credentials: bool,
    credentials_mode: Option<NativeFetchCredentialsMode>,
    referrer: Option<String>,
    referrer_url: Option<String>,
    referrer_policy: Option<String>,
}

type NativeScriptFetch = (
    u32,
    String,
    NativeFetchMethod,
    BTreeMap<String, String>,
    Option<NativeRequestBody>,
    Option<String>,
    NativeCorsMode,
    NativeFetchRedirectMode,
    NativeFetchCacheMode,
    Option<Duration>,
    Option<u32>,
    bool,
    Option<NativeFetchCredentialsMode>,
    Option<String>,
    Option<String>,
    Option<String>,
    bool,
    Option<String>,
    bool,
    Option<NativeModuleResourceType>,
);

pub(crate) struct NativeContentLoad {
    pub(crate) url: String,
    pub(crate) generation: u32,
    pub(crate) origin: NativeOrigin,
    pub(crate) document: NativeDocumentWire,
    pub(crate) frame_sources: Option<Vec<Vec<String>>>,
    pub(crate) navigate_to_sources: Option<Vec<Vec<String>>>,
    pub(crate) events: Vec<NativeContentEvent>,
    pub(crate) csp_violations: Vec<NativeCspViolation>,
    pub(crate) scroll_commands: Vec<NativeScriptCommand>,
    pub(crate) navigation: Option<NativeContentNavigation>,
    pub(crate) storage_events: Vec<NativeStorageEvent>,
    pub(crate) indexed_db_changes: Vec<NativeIndexedDbChange>,
    pub(crate) dialogs: Vec<NativeDialog>,
    pub(crate) popups: Vec<NativePopupRequest>,
    pub(crate) post_messages: Vec<NativePostMessageRequest>,
    pub(crate) page_message_port_commands: Vec<NativePageMessagePortCommand>,
    pub(crate) shared_worker_commands: Vec<NativeScriptCommand>,
    pub(crate) window_closes: Vec<NativeWindowCloseRequest>,
    pub(crate) window_navigations: Vec<NativeWindowNavigationRequest>,
    pub(crate) service_worker_client_messages: Vec<NativeServiceWorkerClientMessage>,
    pub(crate) service_worker_open_windows: Vec<NativeServiceWorkerOpenWindowRequest>,
    pub(crate) window_name: String,
}

pub(crate) enum NativeContentLoadResult {
    Loaded(Box<NativeContentLoad>),
    Suspended(Vec<NativeServiceWorkerOpenWindowRequest>),
}

/// The child-owned result of a top-level `navigate-to` policy preflight.
/// Report-only records are returned separately from authorization so the
/// parent can deliver them without allowing them to change the decision.
pub(crate) struct NativeContentNavigationPolicy {
    pub(crate) allowed: bool,
    pub(crate) csp_violations: Vec<NativeCspViolation>,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct NativeContentEvent {
    pub(crate) node_index: u32,
    pub(crate) kind: NativeEventKind,
}

pub enum NativeWebSocketEvent {
    Open {
        protocol: String,
        csp_violations: Vec<NativeCspViolation>,
    },
    MessageText {
        data: String,
        origin: String,
    },
    MessageBinary {
        data: Vec<u8>,
        origin: String,
    },
    Error {
        message: String,
        csp_violations: Vec<NativeCspViolation>,
    },
    Close {
        code: u16,
        reason: String,
        was_clean: bool,
    },
}

struct NativeWebSocketConnection {
    event_sender: mpsc::Sender<NativeWebSocketEvent>,
    events: mpsc::Receiver<NativeWebSocketEvent>,
    parent_stream_id: Option<u64>,
}

enum NativeEventSourceCommand {
    Close,
    Chunk(Vec<u8>),
    End,
    Error(String),
    Reopened {
        origin: String,
        csp_violations: Vec<NativeCspViolation>,
    },
    OpenFailed {
        message: String,
        csp_violations: Vec<NativeCspViolation>,
    },
}

enum NativeEventSourceEvent {
    Open {
        origin: String,
        cookie_changes: Vec<NativeCookieChange>,
        csp_violations: Vec<NativeCspViolation>,
    },
    Message {
        event: String,
        data: String,
        last_event_id: String,
        origin: String,
    },
    Error {
        message: String,
        csp_violations: Vec<NativeCspViolation>,
    },
    Reconnect {
        last_event_id: String,
    },
    Close,
}

struct NativeEventSourceConnection {
    commands: mpsc::Sender<NativeEventSourceCommand>,
    events: mpsc::Receiver<NativeEventSourceEvent>,
    parent_stream_id: Option<u64>,
    parent_initiator_url: String,
    parent_href: String,
    parent_with_credentials: bool,
}

#[path = "content_process/event_streams.rs"]
mod event_streams;
mod fetch_broker;
use event_streams::bounded_websocket_text;
pub use event_streams::{
    NativeEventSourceParser, native_websocket_request, parse_event_source_chunk,
    validate_websocket_protocols,
};

async fn queue_websocket_event(
    events: &mpsc::Sender<NativeWebSocketEvent>,
    event: NativeWebSocketEvent,
) -> bool {
    events.send(event).await.is_ok()
}

async fn wait_event_source_retry(
    commands: &mut mpsc::Receiver<NativeEventSourceCommand>,
    delay: Duration,
) -> bool {
    tokio::select! {
        _ = sleep(delay) => true,
        command = commands.recv() => !matches!(command, Some(NativeEventSourceCommand::Close) | None),
    }
}

async fn queue_event_source_event(
    events: &mpsc::Sender<NativeEventSourceEvent>,
    event: NativeEventSourceEvent,
) -> bool {
    events.send(event).await.is_ok()
}

async fn run_native_parent_event_source(
    initial_open: Result<NativeParentEventSourceOpen, NativeParentEventSourceFailure>,
    initial_last_event_id: String,
    mut commands: mpsc::Receiver<NativeEventSourceCommand>,
    events: mpsc::Sender<NativeEventSourceEvent>,
) {
    let mut last_event_id = initial_last_event_id;
    let mut retry = NATIVE_EVENTSOURCE_INITIAL_RETRY;
    let mut reconnects = 0usize;
    let mut parser = NativeEventSourceParser {
        last_event_id: last_event_id.clone(),
        retry,
        ..NativeEventSourceParser::default()
    };
    let mut origin = String::new();
    match initial_open {
        Ok(open) => {
            origin = open.origin.clone();
            if !queue_event_source_event(
                &events,
                NativeEventSourceEvent::Open {
                    origin: open.origin,
                    cookie_changes: Vec::new(),
                    csp_violations: open.csp_violations,
                },
            )
            .await
            {
                return;
            }
        }
        Err(failure) => {
            if !schedule_parent_event_source_reconnect(
                &mut commands,
                &events,
                &failure.message,
                failure.csp_violations,
                retry,
                &last_event_id,
                &mut reconnects,
            )
            .await
            {
                return;
            }
        }
    }

    loop {
        match commands.recv().await {
            Some(NativeEventSourceCommand::Close) | None => {
                let _ = queue_event_source_event(&events, NativeEventSourceEvent::Close).await;
                return;
            }
            Some(NativeEventSourceCommand::Chunk(chunk)) => {
                let messages = match parse_event_source_chunk(&mut parser, &chunk) {
                    Ok(messages) => messages,
                    Err(error) => {
                        if !schedule_parent_event_source_reconnect(
                            &mut commands,
                            &events,
                            &bounded_websocket_text(error.to_string(), MAX_NATIVE_SCRIPT_BYTES),
                            Vec::new(),
                            retry,
                            &last_event_id,
                            &mut reconnects,
                        )
                        .await
                        {
                            return;
                        }
                        continue;
                    }
                };
                last_event_id = parser.last_event_id.clone();
                retry = parser.retry.min(NATIVE_EVENTSOURCE_MAX_RETRY);
                for message in messages {
                    last_event_id = message.last_event_id.clone();
                    if !queue_event_source_event(
                        &events,
                        NativeEventSourceEvent::Message {
                            event: message.event,
                            data: message.data,
                            last_event_id: message.last_event_id,
                            origin: origin.clone(),
                        },
                    )
                    .await
                    {
                        return;
                    }
                }
            }
            Some(NativeEventSourceCommand::End) => {
                if !schedule_parent_event_source_reconnect(
                    &mut commands,
                    &events,
                    "EventSource connection ended",
                    Vec::new(),
                    retry,
                    &last_event_id,
                    &mut reconnects,
                )
                .await
                {
                    return;
                }
            }
            Some(NativeEventSourceCommand::Error(message)) => {
                if !schedule_parent_event_source_reconnect(
                    &mut commands,
                    &events,
                    &message,
                    Vec::new(),
                    retry,
                    &last_event_id,
                    &mut reconnects,
                )
                .await
                {
                    return;
                }
            }
            Some(NativeEventSourceCommand::Reopened {
                origin: reopened_origin,
                csp_violations,
            }) => {
                origin = reopened_origin.clone();
                parser = NativeEventSourceParser {
                    last_event_id: last_event_id.clone(),
                    retry,
                    ..NativeEventSourceParser::default()
                };
                reconnects = 0;
                if !queue_event_source_event(
                    &events,
                    NativeEventSourceEvent::Open {
                        origin: reopened_origin,
                        cookie_changes: Vec::new(),
                        csp_violations,
                    },
                )
                .await
                {
                    return;
                }
            }
            Some(NativeEventSourceCommand::OpenFailed {
                message,
                csp_violations,
            }) => {
                if !schedule_parent_event_source_reconnect(
                    &mut commands,
                    &events,
                    &message,
                    csp_violations,
                    retry,
                    &last_event_id,
                    &mut reconnects,
                )
                .await
                {
                    return;
                }
            }
        }
    }
}

async fn schedule_parent_event_source_reconnect(
    commands: &mut mpsc::Receiver<NativeEventSourceCommand>,
    events: &mpsc::Sender<NativeEventSourceEvent>,
    message: &str,
    csp_violations: Vec<NativeCspViolation>,
    retry: Duration,
    last_event_id: &str,
    reconnects: &mut usize,
) -> bool {
    if !queue_event_source_event(
        events,
        NativeEventSourceEvent::Error {
            message: bounded_websocket_text(message, MAX_NATIVE_SCRIPT_BYTES),
            csp_violations,
        },
    )
    .await
    {
        return false;
    }
    *reconnects = reconnects.saturating_add(1);
    if *reconnects > MAX_NATIVE_EVENTSOURCE_RECONNECTS
        || !wait_event_source_retry(commands, retry).await
    {
        let _ = queue_event_source_event(events, NativeEventSourceEvent::Close).await;
        return false;
    }
    queue_event_source_event(
        events,
        NativeEventSourceEvent::Reconnect {
            last_event_id: last_event_id.to_owned(),
        },
    )
    .await
}

fn spawn_native_parent_event_source(
    initiator_url: &str,
    href: &str,
    with_credentials: bool,
    initial_last_event_id: &str,
    initial_open: Result<NativeParentEventSourceOpen, NativeParentEventSourceFailure>,
) -> NativeEventSourceConnection {
    let (command_sender, command_receiver) = mpsc::channel(MAX_NATIVE_EVENTSOURCE_CONNECTIONS);
    let (event_sender, event_receiver) = mpsc::channel(MAX_NATIVE_EVENTSOURCE_CONNECTIONS);
    tokio::spawn(run_native_parent_event_source(
        initial_open,
        initial_last_event_id.to_owned(),
        command_receiver,
        event_sender,
    ));
    NativeEventSourceConnection {
        commands: command_sender,
        events: event_receiver,
        parent_stream_id: None,
        parent_initiator_url: initiator_url.to_owned(),
        parent_href: href.to_owned(),
        parent_with_credentials: with_credentials,
    }
}

#[derive(Debug, Clone)]
pub(crate) struct NativeContentNavigation {
    pub(crate) node_index: u32,
    pub(crate) href: String,
    pub(crate) submitter_node_index: Option<u32>,
    pub(crate) target: String,
    pub(crate) method: NativeNavigationMethod,
    pub(crate) body: Option<NativeRequestBody>,
    pub(crate) body_content_type: Option<String>,
    pub(crate) location: bool,
    pub(crate) replace_history: bool,
    pub(crate) object_url: Option<NativeObjectUrlResource>,
}

pub(crate) struct NativeContentMutation {
    pub(crate) document: NativeDocumentWire,
    pub(crate) navigate_to_sources: Option<Vec<Vec<String>>>,
    pub(crate) events: Vec<NativeContentEvent>,
    pub(crate) csp_violations: Vec<NativeCspViolation>,
    pub(crate) navigation: Option<NativeContentNavigation>,
    pub(crate) allowed: bool,
    pub(crate) history: Vec<NativeScriptCommand>,
    pub(crate) scroll_commands: Vec<NativeScriptCommand>,
    pub(crate) storage_events: Vec<NativeStorageEvent>,
    pub(crate) indexed_db_changes: Vec<NativeIndexedDbChange>,
    pub(crate) dialogs: Vec<NativeDialog>,
    pub(crate) popups: Vec<NativePopupRequest>,
    pub(crate) post_messages: Vec<NativePostMessageRequest>,
    pub(crate) window_closes: Vec<NativeWindowCloseRequest>,
    pub(crate) window_navigations: Vec<NativeWindowNavigationRequest>,
    pub(crate) window_name: String,
}

pub(crate) struct NativeContentScriptResult {
    pub(crate) value: Value,
    pub(crate) mutation: Option<NativeContentMutation>,
    pub(crate) history: Vec<NativeScriptCommand>,
    pub(crate) frame_scripts: Vec<NativeFrameScriptRequest>,
    pub(crate) storage_events: Vec<NativeStorageEvent>,
    pub(crate) indexed_db_changes: Vec<NativeIndexedDbChange>,
    pub(crate) dialogs: Vec<NativeDialog>,
    pub(crate) popups: Vec<NativePopupRequest>,
    pub(crate) post_messages: Vec<NativePostMessageRequest>,
    pub(crate) page_message_port_commands: Vec<NativePageMessagePortCommand>,
    pub(crate) shared_worker_commands: Vec<NativeScriptCommand>,
    pub(crate) window_closes: Vec<NativeWindowCloseRequest>,
    pub(crate) window_navigations: Vec<NativeWindowNavigationRequest>,
    pub(crate) service_worker_client_messages: Vec<NativeServiceWorkerClientMessage>,
    pub(crate) service_worker_open_windows: Vec<NativeServiceWorkerOpenWindowRequest>,
    pub(crate) service_worker_fetch_resumed: bool,
    pub(crate) window_name: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NativeContentAsyncEffectNotification {
    pub sequence: u64,
    pub context_id: String,
    pub frame_id: String,
}

mod response;
use response::*;
mod profile_state;
use profile_state::*;

/// Process-backed lifecycle and bounded document-transfer channel for one
/// native content runtime.
struct NativeParentEventSourceStream {
    owner: NativeContentCookieOwner,
    source_id: u32,
    worker_id: Option<u32>,
    response: reqwest::Response,
}

type NativeParentWebSocket =
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;

struct NativeParentWebSocketStream {
    owner: NativeContentCookieOwner,
    source_id: u32,
    worker_id: Option<u32>,
    origin: String,
    socket: NativeParentWebSocket,
}

pub(crate) struct NativeContentProcess {
    child: Child,
    stdin: ChildStdin,
    stdout_frames: mpsc::Receiver<Result<Vec<u8>, NativeEngineError>>,
    stdout_reader: tokio::task::JoinHandle<()>,
    next_request_id: u64,
    next_async_event_sequence: u64,
    pending_async_effect_notifications: VecDeque<NativeContentAsyncEffectNotification>,
    healthy: bool,
    ipc_aborted: Arc<AtomicBool>,
    failure_kind: Option<NativeWorkerFailureKind>,
    scroll_offset: NativePoint,
    nested_scroll_offsets: BTreeMap<u32, NativePoint>,
    current_document_url: Option<String>,
    current_document_generation: Option<u32>,
    pending_navigation_request: Option<Value>,
    parent_brokered_script_owner: Option<NativeContentCookieOwner>,
    parent_brokered_script_urls: BTreeSet<String>,
    parent_brokered_policy_owner: Option<NativeContentCookieOwner>,
    parent_brokered_meta_policies: BTreeSet<String>,
    context_id: Option<String>,
    frame_id: Option<String>,
    dialog_control: NativeDialogControlPlane,
    pending_cookie_changes: Arc<Mutex<Vec<NativeCookieChange>>>,
    pending_cookie_writes: Vec<NativeContentCookieWrite>,
    next_parent_event_source_stream_id: u64,
    parent_event_source_streams: BTreeMap<u64, NativeParentEventSourceStream>,
    next_parent_websocket_stream_id: u64,
    parent_websocket_streams: BTreeMap<u64, NativeParentWebSocketStream>,
    #[cfg(windows)]
    sandbox: NativeContentSandbox,
}

struct NativeWorkerDialogRpc {
    next_dialog_id: AtomicU64,
    active_request_id: AtomicU64,
    enabled: AtomicBool,
    dialog_waiting: Arc<AtomicBool>,
    dialog_decisions: Mutex<std::sync::mpsc::Receiver<Result<NativeWorkerDialogDecision, String>>>,
}

impl NativeWorkerDialogRpc {
    fn new(
        dialog_decisions: std::sync::mpsc::Receiver<Result<NativeWorkerDialogDecision, String>>,
    ) -> Self {
        Self {
            next_dialog_id: AtomicU64::new(1),
            active_request_id: AtomicU64::new(0),
            enabled: AtomicBool::new(false),
            dialog_waiting: Arc::new(AtomicBool::new(false)),
            dialog_decisions: Mutex::new(dialog_decisions),
        }
    }

    fn set_enabled(&self, enabled: bool) {
        self.enabled.store(enabled, Ordering::Release);
    }

    fn begin_request(&self, request_id: u64) {
        self.active_request_id.store(request_id, Ordering::Release);
    }

    fn request_dialog(
        &self,
        dialog: NativeDialog,
        url: String,
    ) -> Result<Option<NativeDialogResolution>, String> {
        if !self.enabled.load(Ordering::Acquire) {
            return Ok(None);
        }
        let request_id = self.active_request_id.load(Ordering::Acquire);
        if request_id == 0 {
            return Err("native dialog has no active content request".into());
        }
        if !matches!(dialog.dialog_type.as_str(), "alert" | "confirm" | "prompt") {
            return Err("native dialog type is invalid".into());
        }
        if dialog.message.len() > MAX_NATIVE_DIALOG_TEXT_BYTES
            || dialog
                .default_value
                .as_ref()
                .is_some_and(|value| value.len() > MAX_NATIVE_DIALOG_TEXT_BYTES)
        {
            return Err("native dialog text exceeds its byte limit".into());
        }
        validate_url_text("native dialog URL", &url).map_err(|error| error.to_string())?;
        let dialog_id = self
            .next_dialog_id
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |next| {
                next.checked_add(1)
            })
            .map_err(|_| "native dialog identifier space is exhausted".to_owned())?;
        let event = json!({
            "kind": "dialog_open",
            "request_id": request_id,
            "dialog_id": dialog_id,
            "url": url,
            "dialog": dialog,
        });
        let encoded = serde_json::to_vec(&event)
            .map_err(|_| "native dialog event could not be encoded".to_owned())?;
        self.dialog_waiting.store(true, Ordering::Release);
        let decision = (|| {
            write_sync_frame(std::io::stdout(), &encoded)
                .map_err(|_| "native dialog event could not be sent to its owner".to_owned())?;
            self.dialog_decisions
                .lock()
                .map_err(|_| "native dialog decision queue was poisoned".to_owned())?
                .recv()
                .map_err(|_| "native dialog decision channel was closed".to_owned())?
        })();
        self.dialog_waiting.store(false, Ordering::Release);
        let decision = decision?;
        if decision.kind != "dialog_decision"
            || decision.request_id != request_id
            || decision.dialog_id != dialog_id
        {
            return Err("native dialog decision did not match the active call".into());
        }
        if decision
            .prompt_value
            .as_ref()
            .is_some_and(|value| value.len() > MAX_NATIVE_DIALOG_TEXT_BYTES)
        {
            return Err("native prompt response exceeds its byte limit".into());
        }
        Ok(Some(NativeDialogResolution {
            accepted: decision.accepted,
            prompt_value: decision.prompt_value,
        }))
    }
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct NativeWorkerDialogDecision {
    kind: String,
    request_id: u64,
    dialog_id: u64,
    accepted: bool,
    #[serde(default)]
    prompt_value: Option<String>,
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct NativeContentAsyncEffectsReady {
    kind: String,
    protocol: u64,
    sequence: u64,
    context_id: String,
    frame_id: String,
}

fn decode_async_effects_ready(
    value: &Value,
    expected_sequence: u64,
    expected_context_id: Option<&str>,
    expected_frame_id: Option<&str>,
) -> Result<Option<NativeContentAsyncEffectsReady>, NativeEngineError> {
    if value.get("kind").and_then(Value::as_str) != Some("async_effects_ready") {
        return Ok(None);
    }
    let event: NativeContentAsyncEffectsReady =
        serde_json::from_value(value.clone()).map_err(|_| {
            NativeEngineError::worker_failure(
                "content process asynchronous event",
                NativeWorkerFailureKind::Protocol,
                "content process emitted a malformed asynchronous event notification",
            )
        })?;
    if event.kind != "async_effects_ready"
        || event.protocol != CONTENT_WORKER_PROTOCOL_VERSION
        || event.sequence != expected_sequence
        || expected_context_id != Some(event.context_id.as_str())
        || expected_frame_id != Some(event.frame_id.as_str())
    {
        return Err(NativeEngineError::worker_failure(
            "content process asynchronous event",
            NativeWorkerFailureKind::Protocol,
            "content process asynchronous event identity or sequence is invalid",
        ));
    }
    Ok(Some(event))
}

fn content_worker_effects_pending(
    pending_worker_messages: &VecDeque<NativeWorkerMessage>,
    pending_worker_websocket_commands: &VecDeque<NativeWorkerWebSocketCommand>,
    pending_message_port_messages: &VecDeque<NativeMessagePortPageMessage>,
    pending_page_message_port_commands: &VecDeque<NativePageMessagePortCommand>,
    pending_shared_worker_commands: &VecDeque<NativeScriptCommand>,
    pending_service_worker_client_messages: &VecDeque<NativeServiceWorkerClientMessage>,
    pending_external_service_worker_client_messages: &VecDeque<NativeServiceWorkerClientMessage>,
    pending_service_worker_open_windows: &VecDeque<NativeServiceWorkerOpenWindowRequest>,
    pending_lifetime_cookie_changes: &[NativeCookieChange],
) -> Result<bool, NativeEngineError> {
    for (name, length, limit) in [
        (
            "content-process pending worker messages",
            pending_worker_messages.len(),
            MAX_NATIVE_WORKER_MESSAGES,
        ),
        (
            "content-process pending worker WebSocket commands",
            pending_worker_websocket_commands.len(),
            MAX_NATIVE_WORKER_MESSAGES,
        ),
        (
            "content-process pending MessagePort messages",
            pending_message_port_messages.len(),
            MAX_NATIVE_WORKER_MESSAGES,
        ),
        (
            "content-process pending page MessagePort commands",
            pending_page_message_port_commands.len(),
            MAX_NATIVE_EFFECTS,
        ),
        (
            "content-process pending SharedWorker commands",
            pending_shared_worker_commands.len(),
            MAX_NATIVE_EFFECTS,
        ),
        (
            "content-process pending ServiceWorker client messages",
            pending_service_worker_client_messages.len(),
            MAX_NATIVE_WORKER_MESSAGES,
        ),
        (
            "content-process pending external ServiceWorker client messages",
            pending_external_service_worker_client_messages.len(),
            MAX_NATIVE_WORKER_MESSAGES,
        ),
        (
            "content-process pending ServiceWorker openWindow requests",
            pending_service_worker_open_windows.len(),
            MAX_NATIVE_EFFECTS,
        ),
        (
            "content-process pending lifetime cookie changes",
            pending_lifetime_cookie_changes.len(),
            MAX_NATIVE_COOKIE_CHANGE_BATCH,
        ),
    ] {
        if length > limit {
            return Err(NativeEngineError::limit(name, limit, length));
        }
    }
    Ok(!pending_worker_messages.is_empty()
        || !pending_worker_websocket_commands.is_empty()
        || !pending_message_port_messages.is_empty()
        || !pending_page_message_port_commands.is_empty()
        || !pending_shared_worker_commands.is_empty()
        || !pending_service_worker_client_messages.is_empty()
        || !pending_external_service_worker_client_messages.is_empty()
        || !pending_service_worker_open_windows.is_empty()
        || !pending_lifetime_cookie_changes.is_empty())
}

async fn write_async_effects_ready(
    stdout: &mut (impl AsyncWrite + Unpin),
    has_pending_effects: bool,
    notification_pending: &mut bool,
    next_sequence: &mut u64,
    context_id: &str,
    frame_id: &str,
) -> Result<(), NativeEngineError> {
    if !has_pending_effects || *notification_pending {
        return Ok(());
    }
    if context_id.is_empty() || frame_id.is_empty() {
        return Err(NativeEngineError::worker_failure(
            "content process asynchronous event",
            NativeWorkerFailureKind::Protocol,
            "content process cannot announce effects without an owner identity",
        ));
    }
    let sequence = *next_sequence;
    let next = sequence.checked_add(1).ok_or_else(|| {
        NativeEngineError::worker_failure(
            "content process asynchronous event",
            NativeWorkerFailureKind::Protocol,
            "content process asynchronous event sequence is exhausted",
        )
    })?;
    write_value_frame(
        stdout,
        &json!({
            "kind": "async_effects_ready",
            "protocol": CONTENT_WORKER_PROTOCOL_VERSION,
            "sequence": sequence,
            "context_id": context_id,
            "frame_id": frame_id,
        }),
    )
    .await?;
    *next_sequence = next;
    *notification_pending = true;
    Ok(())
}

mod event_mutations;
use event_mutations::*;
mod page_lifecycle;
use page_lifecycle::*;
mod network_events;
pub(super) use network_events::*;
pub use network_events::{websocket_event_csp_violations, websocket_event_payload};
mod process;

impl Drop for NativeContentProcess {
    fn drop(&mut self) {
        let _ = self.child.start_kill();
        self.stdout_reader.abort();
    }
}

mod ipc;
mod navigation;
use ipc::*;
use navigation::*;
pub(crate) use navigation::{
    ContentLoadStage, apply_pending_meta_content_security_policies, inline_style_policy_nodes,
    is_parent_owned_network_target, refresh_inline_style_policy,
};

#[doc(hidden)]
pub use worker::run_native_content_worker;
mod worker;

async fn resolve_service_worker_commands(
    commands: Vec<NativeScriptCommand>,
    registry: &mut NativeServiceWorkerRegistry,
    loader: &mut NativeResourceLoader,
    runtime: &NativeJavaScriptRuntime,
    document: &mut NativeDocument,
    document_url: &str,
    document_origin: &NativeOrigin,
    viewport: Viewport,
    mut parent_fetch_broker: Option<&mut NativeContentFetchBroker<'_>>,
) -> Result<Vec<NativeScriptCommand>, NativeEngineError> {
    let mut pending = VecDeque::from(commands);
    let mut document_commands = Vec::new();
    let mut turns = 0usize;
    while let Some(command) = pending.pop_front() {
        turns = turns.saturating_add(1);
        if turns > MAX_CONTENT_EVENT_LOOP_TURNS {
            return Err(NativeEngineError::limit(
                "service worker registration event-loop turns",
                MAX_CONTENT_EVENT_LOOP_TURNS,
                turns,
            ));
        }
        match command {
            NativeScriptCommand::ServiceWorkerRegister {
                request_id,
                script_url,
                scope,
                worker_type,
                referrer_url,
                referrer_policy,
            } => {
                let result = match NativeFetchReferrerPolicy::parse(&referrer_policy) {
                    Ok(referrer_policy) => {
                        registry
                            .register(
                                NativeServiceWorkerRegistrationRequest {
                                    owner_url: document_url,
                                    script_url: Some(&script_url),
                                    scope: &scope,
                                    worker_type: Some(&worker_type),
                                    referrer_url: &referrer_url,
                                    referrer_policy,
                                    request_id,
                                },
                                loader,
                                parent_fetch_broker.as_deref_mut(),
                            )
                            .await
                    }
                    Err(error) => Err(error),
                };
                let payload = match result {
                    Ok(state) => {
                        runtime.set_service_worker_registrations(
                            registry.states_for_document(document_url)?,
                        );
                        json!({"kind":"register","registration":state})
                    }
                    Err(error) => json!({"error":error.to_string()}),
                };
                let evaluation = runtime.resolve_service_worker_registration(
                    request_id,
                    &payload,
                    document,
                    document_url,
                    document_origin,
                    viewport,
                )?;
                if evaluation.top_level_await_pending {
                    return Err(NativeEngineError::Worker {
                        operation: "service worker registration response".into(),
                        reason: "service worker registration response remained pending".into(),
                    });
                }
                document_commands.extend(evaluation.commands);
                pending.extend(runtime.take_service_worker_commands());
            }
            NativeScriptCommand::ServiceWorkerUnregister { request_id, scope } => {
                let unregistered = registry.unregister(&scope);
                runtime
                    .set_service_worker_registrations(registry.states_for_document(document_url)?);
                let evaluation = runtime.resolve_service_worker_registration(
                    request_id,
                    &json!({
                        "kind":"unregister",
                        "scope":scope,
                        "unregistered":unregistered,
                    }),
                    document,
                    document_url,
                    document_origin,
                    viewport,
                )?;
                if evaluation.top_level_await_pending {
                    return Err(NativeEngineError::Worker {
                        operation: "service worker unregister response".into(),
                        reason: "service worker unregister response remained pending".into(),
                    });
                }
                document_commands.extend(evaluation.commands);
                pending.extend(runtime.take_service_worker_commands());
            }
            NativeScriptCommand::ServiceWorkerUpdate {
                request_id,
                scope,
                referrer_url,
                referrer_policy,
            } => {
                let result = match NativeFetchReferrerPolicy::parse(&referrer_policy) {
                    Ok(referrer_policy) => {
                        registry
                            .update(
                                NativeServiceWorkerRegistrationRequest {
                                    owner_url: document_url,
                                    script_url: None,
                                    scope: &scope,
                                    worker_type: None,
                                    referrer_url: &referrer_url,
                                    referrer_policy,
                                    request_id,
                                },
                                loader,
                                parent_fetch_broker.as_deref_mut(),
                            )
                            .await
                    }
                    Err(error) => Err(error),
                };
                let payload = match result {
                    Ok(state) => {
                        runtime.set_service_worker_registrations(
                            registry.states_for_document(document_url)?,
                        );
                        json!({"kind":"update","registration":state})
                    }
                    Err(error) => json!({"error":error.to_string()}),
                };
                let evaluation = runtime.resolve_service_worker_registration(
                    request_id,
                    &payload,
                    document,
                    document_url,
                    document_origin,
                    viewport,
                )?;
                if evaluation.top_level_await_pending {
                    return Err(NativeEngineError::Worker {
                        operation: "service worker update response".into(),
                        reason: "service worker update response remained pending".into(),
                    });
                }
                document_commands.extend(evaluation.commands);
                pending.extend(runtime.take_service_worker_commands());
            }
            NativeScriptCommand::ServiceWorkerNavigationPreload {
                request_id,
                scope,
                operation,
                header_value,
            } => {
                let payload = match registry.apply_navigation_preload_operation(
                    document_url,
                    &scope,
                    &operation,
                    header_value.as_deref(),
                ) {
                    Ok(NativeServiceWorkerNavigationPreloadResult::State {
                        enabled,
                        header_value,
                    }) => json!({
                        "operation": operation,
                        "state": {
                            "enabled": enabled,
                            "headerValue": header_value,
                        },
                    }),
                    Ok(NativeServiceWorkerNavigationPreloadResult::InvalidState) => json!({
                        "error": "ServiceWorker registration has no active worker",
                        "errorName": "InvalidStateError",
                    }),
                    Err(error) => json!({"error":error.to_string()}),
                };
                let evaluation = runtime.resolve_service_worker_registration(
                    request_id,
                    &payload,
                    document,
                    document_url,
                    document_origin,
                    viewport,
                )?;
                if evaluation.top_level_await_pending {
                    return Err(NativeEngineError::Worker {
                        operation: "service worker navigation preload response".into(),
                        reason: "navigation preload response remained pending".into(),
                    });
                }
                document_commands.extend(evaluation.commands);
                pending.extend(runtime.take_service_worker_commands());
            }
            NativeScriptCommand::ServiceWorkerPostMessage {
                scope,
                source_origin,
                data,
                transfer_ports,
            } => {
                registry
                    .post_message(
                        loader,
                        &scope,
                        &source_origin,
                        &data,
                        &transfer_ports,
                        parent_fetch_broker.as_deref_mut(),
                    )
                    .await?;
                pending.extend(runtime.take_service_worker_commands());
            }
            other => document_commands.push(other),
        }
    }
    Ok(document_commands)
}

async fn parent_navigation_broker_response(
    request_id: u64,
    load_request: &Value,
    broker_request: &Value,
    context_id: Option<&str>,
    frame_id: Option<&str>,
    loader: &mut NativeResourceLoader,
) -> Result<Value, NativeEngineError> {
    if broker_request.get("id").and_then(Value::as_u64) != Some(request_id)
        || load_request.get("kind").and_then(Value::as_str) != Some("load")
    {
        return Err(NativeEngineError::worker_failure(
            "content process parent navigation broker",
            NativeWorkerFailureKind::Protocol,
            "parent navigation request belongs to a different content operation",
        ));
    }
    let generation = load_request
        .get("generation")
        .and_then(Value::as_u64)
        .and_then(|value| u32::try_from(value).ok())
        .filter(|generation| *generation != 0)
        .ok_or_else(|| {
            NativeEngineError::worker_failure(
                "content process parent navigation broker",
                NativeWorkerFailureKind::Protocol,
                "load operation has no valid document generation",
            )
        })?;
    if broker_request.get("generation").and_then(Value::as_u64) != Some(u64::from(generation))
        || broker_request.get("url").and_then(Value::as_str)
            != load_request.get("url").and_then(Value::as_str)
        || broker_request.get("context_id").and_then(Value::as_str) != context_id
        || broker_request.get("frame_id").and_then(Value::as_str) != frame_id
    {
        return Err(NativeEngineError::worker_failure(
            "content process parent navigation broker",
            NativeWorkerFailureKind::Protocol,
            "parent navigation request does not match its captured load owner",
        ));
    }
    let result = async {
        let navigation = decode_content_process_load_navigation(load_request)?;
        if !is_network_url(without_fragment(&navigation.url)) {
            return Err(NativeEngineError::worker_failure(
                "content process parent navigation broker",
                NativeWorkerFailureKind::Protocol,
                "only HTTP(S) navigations may use the parent network broker",
            ));
        }
        let referrer = load_request
            .get("referrer")
            .and_then(|value| value.as_str());
        let referrer_policy = load_request
            .get("referrer_policy")
            .and_then(Value::as_str)
            .ok_or_else(|| {
                NativeEngineError::worker_failure(
                    "content process parent navigation broker",
                    NativeWorkerFailureKind::Protocol,
                    "load operation omitted its Referrer-Policy",
                )
            })
            .and_then(NativeFetchReferrerPolicy::parse)?;
        let navigation_stage = ContentLoadStage::begin(request_id, "parent_navigation_fetch");
        let resource = loader
            .load_async_request_with_referrer_policy(&navigation, referrer, referrer_policy)
            .await?;
        navigation_stage.finish(Some(resource.body.len()));
        let document_cookie = loader.document_cookie(&resource.url)?;
        if document_cookie.len() > MAX_CONTENT_DOCUMENT_COOKIE_BYTES {
            return Err(NativeEngineError::limit(
                "parent document.cookie projection",
                MAX_CONTENT_DOCUMENT_COOKIE_BYTES,
                document_cookie.len(),
            ));
        }
        let policy_headers = loader.document_response_policy_headers(&resource.url)?;
        let body = resource.body.into_bytes();
        if body.len() > loader.max_document_bytes() {
            return Err(NativeEngineError::limit(
                "parent-brokered navigation document",
                loader.max_document_bytes(),
                body.len(),
            ));
        }
        Ok(json!({
            "kind": "parent_navigation_response",
            "id": request_id,
            "generation": generation,
            "url": resource.url,
            "body_base64": base64::engine::general_purpose::STANDARD.encode(body),
            "document_cookie": document_cookie,
            "response_policy_headers": policy_headers,
        }))
    }
    .await;
    Ok(match result {
        Ok(response) => response,
        Err(error) => json!({
            "kind": "error",
            "id": request_id,
            "generation": generation,
            "reason": error.to_string(),
        }),
    })
}

#[path = "content_process/dynamic_scripts.rs"]
mod dynamic_scripts;
use dynamic_scripts::{
    activate_dynamic_page_script_network, execute_dynamic_page_scripts_with_loader,
    execute_dynamic_page_scripts_without_loader, fetch_commands, fetch_opened_response_payload,
    fetch_response_payload, fetch_response_payload_with_csp_violations,
    font_fetch_response_payload, form_action_allows, load_dynamic_page_module,
    load_module_dependencies, process_page_fetch_resolution, refresh_parent_broker_meta_csp,
    register_page_import_map_source, retain_font_face_follow_up_commands,
    retain_font_face_network_commands,
};
pub(crate) use dynamic_scripts::{
    merge_dynamic_page_script_result, missing_parent_network_authority,
};

async fn resolve_script_fetches(
    current: &NativeDocument,
    runtime: &NativeJavaScriptRuntime,
    mut loader: Option<&mut NativeResourceLoader>,
    mut parent_fetch_broker: Option<NativeContentFetchBroker<'_>>,
    service_workers: &mut NativeServiceWorkerRegistry,
    websocket_connections: &mut BTreeMap<u32, NativeWebSocketConnection>,
    fetch_stream_connections: &mut BTreeMap<u32, NativeFetchStreamConnection>,
    fetch_upload_connections: &mut BTreeMap<u32, NativeFetchUploadConnection>,
    event_source_connections: &mut BTreeMap<u32, NativeEventSourceConnection>,
    document_url: &str,
    document_origin: &NativeOrigin,
    viewport: Viewport,
    mut pump_background_events: bool,
    evaluation: NativeScriptEvaluation,
) -> Result<(NativeDocument, NativeContentMutation, Option<Value>), NativeEngineError> {
    let _timer_pump = runtime.suspend_timer_pump();
    let NativeScriptEvaluation {
        commands: initial_commands,
        top_level_await_pending,
        ..
    } = evaluation;
    let mut current_url = document_url.to_owned();
    let initial_commands = process_websocket_commands(
        initial_commands,
        websocket_connections,
        &current_url,
        parent_fetch_broker.as_mut(),
    )
    .await?;
    let initial_commands = process_event_source_commands(
        initial_commands,
        event_source_connections,
        loader.as_deref(),
        &current_url,
        parent_fetch_broker.as_mut(),
    )
    .await?;
    let initial_commands =
        process_fetch_stream_commands(initial_commands, fetch_stream_connections)?;
    let initial_commands =
        process_fetch_upload_commands(initial_commands, fetch_upload_connections)?;
    let (mut next, mut mutation, dynamic_result) = mutate_script_document(
        current,
        runtime,
        &current_url,
        document_origin,
        viewport,
        &initial_commands,
        loader.as_deref_mut(),
        parent_fetch_broker.as_mut(),
    )
    .await?;
    if let (Some(loader), Some(broker)) = (loader.as_deref(), parent_fetch_broker.as_mut()) {
        refresh_parent_broker_meta_csp(loader, Some(broker))?;
    }
    if !mutation.history.is_empty() {
        current_url =
            resolve_content_history_document_url(&mutation.history, &current_url, document_origin)?;
    }
    let mut pending = fetch_commands(&initial_commands)?;
    let (dynamic_fetches, dynamic_background) = activate_dynamic_page_script_network(
        dynamic_result,
        websocket_connections,
        event_source_connections,
        loader.as_deref(),
        &current_url,
        parent_fetch_broker.as_mut(),
    )
    .await?;
    pending.extend(fetch_commands(&dynamic_fetches)?);
    pump_background_events |= dynamic_background;
    let mut resolved_count = 0usize;
    let mut resolved_value = None;
    let mut event_loop_turns = 0usize;
    let mut next_task_source = NativeContentTaskSource::Networking;
    let mut websocket_read_cursor = 0u32;
    let mut event_source_read_cursor = 0u32;
    let mut pending_upload_fetches: BTreeMap<u32, NativePendingUploadFetch> = BTreeMap::new();
    let mut pending_controlled_uploads: BTreeMap<u32, NativePendingControlledUpload> =
        BTreeMap::new();
    loop {
        if top_level_await_pending && resolved_value.is_some() {
            break;
        }
        if top_level_await_pending && let Some(value) = runtime.take_top_level_await_result()? {
            resolved_value = Some(value);
            break;
        }
        let pump_fetch_streams = pump_background_events
            || fetch_stream_connections
                .values()
                .any(|connection| connection.read_pending);
        let mut selected_source = None;
        let mut selected_fetch = None;
        let mut selected_controlled_upload_completion = None;
        let mut selected_upload_fetch_completion = None;
        let mut selected_websocket_event = None;
        let mut selected_fetch_stream_event = None;
        let mut selected_fetch_upload_event = None;
        let mut selected_event_source_event = None;
        let mut parent_event_source_progress = false;
        for _ in 0..6 {
            let source = next_task_source;
            next_task_source = source.next();
            match source {
                NativeContentTaskSource::Networking => {
                    if let Some(request_id) = pending_controlled_uploads
                        .iter()
                        .find(|(_, fetch)| fetch.task.is_finished())
                        .map(|(request_id, _)| *request_id)
                    {
                        selected_controlled_upload_completion = Some(request_id);
                        selected_source = Some(source);
                        break;
                    }
                    if let Some(request_id) = pending_upload_fetches
                        .iter()
                        .find(|(_, fetch)| fetch.task.is_finished())
                        .map(|(request_id, _)| *request_id)
                    {
                        selected_upload_fetch_completion = Some(request_id);
                        selected_source = Some(source);
                        break;
                    }
                    if let Some(fetch) = pending.pop_front() {
                        selected_fetch = Some(fetch);
                        selected_source = Some(source);
                        break;
                    }
                }
                NativeContentTaskSource::WebSocket if pump_background_events => {
                    if let Some(event) = take_websocket_event(websocket_connections) {
                        selected_websocket_event = Some(event);
                        selected_source = Some(source);
                        break;
                    }
                    if pump_parent_websocket_stream(
                        websocket_connections,
                        parent_fetch_broker.as_mut(),
                        &mut websocket_read_cursor,
                    )
                    .await?
                        && let Some(event) = take_websocket_event(websocket_connections)
                    {
                        selected_websocket_event = Some(event);
                        selected_source = Some(source);
                        break;
                    }
                }
                NativeContentTaskSource::FetchStream if pump_fetch_streams => {
                    if let Some(event) = take_fetch_stream_event(fetch_stream_connections) {
                        selected_fetch_stream_event = Some(event);
                        selected_source = Some(source);
                        break;
                    }
                }
                NativeContentTaskSource::FetchUpload => {
                    if let Some(stream_id) = take_fetch_upload_event(fetch_upload_connections) {
                        selected_fetch_upload_event = Some(stream_id);
                        selected_source = Some(source);
                        break;
                    }
                }
                NativeContentTaskSource::EventSource if pump_background_events => {
                    if let Some(event) = take_event_source_event(event_source_connections) {
                        selected_event_source_event = Some(event);
                        selected_source = Some(source);
                        break;
                    }
                    parent_event_source_progress = pump_parent_event_source_stream(
                        event_source_connections,
                        parent_fetch_broker.as_mut(),
                        &mut event_source_read_cursor,
                    )
                    .await?;
                    if parent_event_source_progress
                        && let Some(event) = take_event_source_event(event_source_connections)
                    {
                        selected_event_source_event = Some(event);
                        selected_source = Some(source);
                        break;
                    }
                    if parent_event_source_progress {
                        break;
                    }
                }
                NativeContentTaskSource::Timer
                    if top_level_await_pending
                        && runtime
                            .next_timer_delay_ms()?
                            .is_some_and(|delay| delay == 0) =>
                {
                    selected_source = Some(source);
                    break;
                }
                _ => {}
            }
        }
        if selected_source.is_none() && parent_event_source_progress {
            continue;
        }
        if selected_source.is_none()
            && pump_background_events
            && pump_parent_event_source_stream(
                event_source_connections,
                parent_fetch_broker.as_mut(),
                &mut event_source_read_cursor,
            )
            .await?
        {
            continue;
        }
        if let Some((
            request_id,
            href,
            method,
            headers,
            body,
            content_type,
            cors_mode,
            redirect_mode,
            cache_mode,
            timeout,
            upload_stream_id,
            credentials,
            credentials_mode,
            referrer,
            referrer_url,
            referrer_policy,
            font_destination,
            module_referrer,
            module_destination,
            module_type,
        )) = selected_fetch.take()
        {
            resolved_count = resolved_count.saturating_add(1);
            if resolved_count > MAX_NATIVE_EFFECTS {
                return Err(NativeEngineError::limit(
                    "script fetch requests",
                    MAX_NATIVE_EFFECTS,
                    resolved_count,
                ));
            }
            let Some(loader) = loader.as_deref_mut() else {
                return Err(NativeEngineError::Worker {
                    operation: "content process script fetch".into(),
                    reason: "content process has no resource loader".into(),
                });
            };
            if module_destination {
                let module_referrer_policy = referrer_policy
                    .as_deref()
                    .map(NativeFetchReferrerPolicy::parse)
                    .transpose()?;
                let loaded_module = if method.as_str() == "GET"
                    && headers.is_empty()
                    && body.is_none()
                    && content_type.is_none()
                    && cors_mode == NativeCorsMode::Cors
                    && redirect_mode == NativeFetchRedirectMode::Follow
                    && cache_mode == NativeFetchCacheMode::Default
                    && timeout.is_none()
                    && upload_stream_id.is_none()
                    && credentials
                {
                    load_dynamic_page_module(
                        request_id,
                        &current_url,
                        module_referrer.as_deref().unwrap_or_default(),
                        module_referrer_policy,
                        &href,
                        module_type,
                        runtime,
                        loader,
                        parent_fetch_broker.as_mut(),
                    )
                    .await
                } else {
                    Err(NativeEngineError::invalid(
                        "dynamic module request",
                        "must be a credentialed bodyless CORS GET with default cache and follow redirects",
                    ))
                };
                let module_alias = loaded_module.as_ref().ok().cloned();
                let payload = match loaded_module {
                    Ok(module_key) => json!({
                        "dynamicModuleImport": {"moduleKey": module_key},
                    }),
                    Err(_) => json!({
                        "dynamicModuleImport": {"error": "Failed to load dynamically imported module"},
                    }),
                };
                let resolution = process_page_fetch_resolution(
                    request_id,
                    payload,
                    runtime,
                    service_workers,
                    websocket_connections,
                    fetch_stream_connections,
                    fetch_upload_connections,
                    event_source_connections,
                    loader,
                    &mut next,
                    &mut mutation,
                    &mut current_url,
                    document_origin,
                    viewport,
                    top_level_await_pending,
                    &mut resolved_value,
                    &mut pump_background_events,
                    &mut pending,
                    parent_fetch_broker.as_mut(),
                )
                .await;
                if let Some(module_alias) = module_alias {
                    runtime.discard_dynamic_module_alias(&module_alias);
                }
                resolution?;
                continue;
            }
            if font_destination {
                if method.as_str() != "GET"
                    || !headers.is_empty()
                    || body.is_some()
                    || content_type.is_some()
                    || cors_mode != NativeCorsMode::Cors
                    || redirect_mode != NativeFetchRedirectMode::Follow
                    || cache_mode != NativeFetchCacheMode::Default
                    || !credentials
                    || upload_stream_id.is_some()
                {
                    return Err(NativeEngineError::invalid(
                        "font fetch request",
                        "must be a bodyless credentialed CORS GET with follow redirects and default cache",
                    ));
                }
                let object_url = runtime.object_url_resource(&href)?;
                let intercepted = if is_network_url(&current_url) {
                    service_workers
                        .intercept_fetch(
                            loader,
                            &current_url,
                            &href,
                            method.as_str(),
                            headers.clone(),
                            body.clone(),
                            content_type.clone(),
                            cors_mode,
                            redirect_mode,
                            timeout,
                            credentials,
                            credentials_mode,
                            referrer.as_deref(),
                            referrer_url.as_deref(),
                            referrer_policy.as_deref(),
                            "font",
                            None,
                            None,
                            parent_fetch_broker.as_mut(),
                            false,
                        )
                        .await
                } else {
                    Ok(NativeServiceWorkerFetchOutcome::NotHandled)
                };
                let payload = match intercepted {
                    Ok(NativeServiceWorkerFetchOutcome::Handled(response)) => {
                        fetch_response_payload(Ok(response))
                    }
                    Ok(NativeServiceWorkerFetchOutcome::NotHandled) => {
                        let response = if is_parent_owned_network_target(&current_url, &href) {
                            if let Some(broker) = parent_fetch_broker.as_mut() {
                                refresh_parent_broker_meta_csp(loader, Some(broker))?;
                                broker.load_font_response(&current_url, &href).await
                            } else {
                                Err(missing_parent_network_authority("font Fetch"))
                            }
                        } else {
                            loader
                                .load_font_response_async(&current_url, &href, object_url.as_ref())
                                .await
                        };
                        font_fetch_response_payload(response)
                    }
                    Ok(NativeServiceWorkerFetchOutcome::Suspended) => {
                        fetch_response_payload(Err(NativeEngineError::Worker {
                            operation: "content process font fetch".into(),
                            reason: "Service Worker font fetch is awaiting a browser WindowClient"
                                .into(),
                        }))
                    }
                    Err(error) => fetch_response_payload(Err(error)),
                };
                process_page_fetch_resolution(
                    request_id,
                    payload,
                    runtime,
                    service_workers,
                    websocket_connections,
                    fetch_stream_connections,
                    fetch_upload_connections,
                    event_source_connections,
                    loader,
                    &mut next,
                    &mut mutation,
                    &mut current_url,
                    document_origin,
                    viewport,
                    top_level_await_pending,
                    &mut resolved_value,
                    &mut pump_background_events,
                    &mut pending,
                    parent_fetch_broker.as_mut(),
                )
                .await?;
                continue;
            }
            if let Some(upload_stream_id) = upload_stream_id {
                if upload_stream_id != request_id {
                    return Err(NativeEngineError::invalid(
                        "fetch request upload stream",
                        "upload stream identifier must match its fetch request",
                    ));
                }
                if body.is_some() {
                    return Err(NativeEngineError::invalid(
                        "fetch request upload stream",
                        "streaming fetch requests must not also carry a buffered body",
                    ));
                }
                if fetch_upload_connections.contains_key(&upload_stream_id) {
                    return Err(NativeEngineError::Network {
                        operation: "fetch request upload stream".into(),
                        reason: "fetch request upload identifier is already active".into(),
                    });
                }
                if is_network_url(&current_url)
                    && service_workers.fetch_is_controlled(&current_url, &href, "fetch")?
                {
                    let pending_upload_count = pending_upload_fetches
                        .len()
                        .saturating_add(pending_controlled_uploads.len());
                    if pending_upload_count >= MAX_NATIVE_EFFECTS {
                        return Err(NativeEngineError::limit(
                            "native pending fetch uploads",
                            MAX_NATIVE_EFFECTS,
                            pending_upload_count.saturating_add(1),
                        ));
                    }
                    let (upload_connection, upload_source) = spawn_native_fetch_upload_source();
                    fetch_upload_connections.insert(upload_stream_id, upload_connection);
                    let task = tokio::spawn(async move {
                        upload_source
                            .collect(MAX_NATIVE_FORM_BODY_BYTES)
                            .await
                            .map_err(|reason| NativeEngineError::Network {
                                operation: "controlled Service Worker fetch request upload".into(),
                                reason,
                            })
                    });
                    pending_controlled_uploads.insert(
                        request_id,
                        NativePendingControlledUpload {
                            task,
                            parent_brokered: false,
                            document_url: current_url.clone(),
                            href,
                            method,
                            headers,
                            content_type,
                            cors_mode,
                            redirect_mode,
                            cache_mode,
                            timeout,
                            credentials,
                            credentials_mode,
                            referrer,
                            referrer_url,
                            referrer_policy,
                        },
                    );
                    continue;
                }
                if is_parent_owned_network_target(&current_url, &href) {
                    let parent_brokered = parent_fetch_broker.is_some();
                    let pending_upload_count = pending_upload_fetches
                        .len()
                        .saturating_add(pending_controlled_uploads.len());
                    if pending_upload_count >= MAX_NATIVE_EFFECTS {
                        return Err(NativeEngineError::limit(
                            "native pending fetch uploads",
                            MAX_NATIVE_EFFECTS,
                            pending_upload_count.saturating_add(1),
                        ));
                    }
                    let (upload_connection, upload_source) = spawn_native_fetch_upload_source();
                    fetch_upload_connections.insert(upload_stream_id, upload_connection);
                    let task = tokio::spawn(async move {
                        upload_source
                            .collect(MAX_NATIVE_FORM_BODY_BYTES)
                            .await
                            .map_err(|reason| NativeEngineError::Network {
                                operation: "parent-brokered fetch request upload".into(),
                                reason,
                            })
                    });
                    pending_controlled_uploads.insert(
                        request_id,
                        NativePendingControlledUpload {
                            task,
                            parent_brokered,
                            document_url: current_url.clone(),
                            href,
                            method,
                            headers,
                            content_type,
                            cors_mode,
                            redirect_mode,
                            cache_mode,
                            timeout,
                            credentials,
                            credentials_mode,
                            referrer,
                            referrer_url,
                            referrer_policy,
                        },
                    );
                    continue;
                }
                let pending_upload_count = pending_upload_fetches
                    .len()
                    .saturating_add(pending_controlled_uploads.len());
                if pending_upload_count >= MAX_NATIVE_EFFECTS {
                    return Err(NativeEngineError::limit(
                        "native pending fetch uploads",
                        MAX_NATIVE_EFFECTS,
                        pending_upload_count.saturating_add(1),
                    ));
                }
                let (upload_connection, request_body) = spawn_native_fetch_upload_stream();
                fetch_upload_connections.insert(upload_stream_id, upload_connection);
                let task_document_url = current_url.clone();
                let task_href = href.clone();
                let task_loader = loader.clone();
                let task = tokio::spawn(async move {
                    let mut task_loader = task_loader;
                    let result = task_loader
                        .open_fetch_response_stream_with_body_async(
                            NativeFetchRequest {
                                document_url: &task_document_url,
                                href: &task_href,
                                method,
                                body: None,
                                content_type,
                                request_headers: headers,
                                credentials,
                                credentials_mode,
                                referrer_url,
                                referrer_policy,
                                cors_mode,
                                redirect_mode,
                                cache_mode,
                                timeout,
                                max_response_bytes: None,
                            },
                            Some(request_body),
                        )
                        .await;
                    (result, task_loader)
                });
                pending_upload_fetches.insert(request_id, NativePendingUploadFetch { task });
                continue;
            }
            let intercepted = service_workers
                .intercept_fetch(
                    loader,
                    &current_url,
                    &href,
                    method.as_str(),
                    headers.clone(),
                    body.clone(),
                    content_type.clone(),
                    cors_mode,
                    redirect_mode,
                    timeout,
                    credentials,
                    credentials_mode,
                    referrer.as_deref(),
                    referrer_url.as_deref(),
                    referrer_policy.as_deref(),
                    "fetch",
                    None,
                    None,
                    parent_fetch_broker.as_mut(),
                    false,
                )
                .await;
            let payload = match intercepted {
                Ok(NativeServiceWorkerFetchOutcome::Handled(response)) => {
                    fetch_response_payload(Ok(response))
                }
                Ok(NativeServiceWorkerFetchOutcome::NotHandled) => {
                    let fetch_request = NativeFetchRequest {
                        document_url: &current_url,
                        href: &href,
                        method,
                        body,
                        content_type,
                        request_headers: headers,
                        cors_mode,
                        redirect_mode,
                        cache_mode,
                        timeout,
                        credentials,
                        credentials_mode,
                        referrer_url,
                        referrer_policy,
                        max_response_bytes: None,
                    };
                    if is_parent_owned_network_target(&current_url, &href) {
                        if let Some(broker) = parent_fetch_broker.as_mut() {
                            let (fetch, document_cookie, csp_violations) =
                                broker.fetch(request_id, &fetch_request).await?;
                            runtime.set_cookie_state(document_cookie);
                            fetch_response_payload_with_csp_violations(fetch, csp_violations)
                        } else {
                            fetch_response_payload(Err(missing_parent_network_authority(
                                "Fetch request",
                            )))
                        }
                    } else {
                        let opened = loader.open_fetch_response_stream_async(fetch_request).await;
                        fetch_opened_response_payload(opened, request_id, fetch_stream_connections)
                            .await
                    }
                }
                Ok(NativeServiceWorkerFetchOutcome::Suspended) => {
                    fetch_response_payload(Err(NativeEngineError::Worker {
                        operation: "content process script fetch".into(),
                        reason: "Service Worker fetch is awaiting a browser WindowClient".into(),
                    }))
                }
                Err(error) => fetch_response_payload(Err(error)),
            };
            process_page_fetch_resolution(
                request_id,
                payload,
                runtime,
                service_workers,
                websocket_connections,
                fetch_stream_connections,
                fetch_upload_connections,
                event_source_connections,
                loader,
                &mut next,
                &mut mutation,
                &mut current_url,
                document_origin,
                viewport,
                top_level_await_pending,
                &mut resolved_value,
                &mut pump_background_events,
                &mut pending,
                parent_fetch_broker.as_mut(),
            )
            .await?;
            continue;
        }
        if let Some(request_id) = selected_controlled_upload_completion.take() {
            let pending_fetch =
                pending_controlled_uploads
                    .remove(&request_id)
                    .ok_or_else(|| NativeEngineError::Worker {
                        operation: "controlled Service Worker fetch upload completion".into(),
                        reason: "completed controlled fetch upload task was not active".into(),
                    })?;
            let body_result = match pending_fetch.task.await {
                Ok(result) => result,
                Err(_) => Err(NativeEngineError::worker_failure(
                    "controlled Service Worker fetch upload task",
                    NativeWorkerFailureKind::Transport,
                    "native controlled fetch upload task terminated unexpectedly",
                )),
            };
            if let Some(connection) = fetch_upload_connections.remove(&request_id) {
                let _ = connection
                    .commands
                    .try_send(NativeFetchUploadCommand::Cancel);
            }
            let Some(loader) = loader.as_deref_mut() else {
                return Err(NativeEngineError::Worker {
                    operation: "controlled Service Worker fetch upload completion".into(),
                    reason: "content process has no resource loader".into(),
                });
            };
            let payload = match body_result {
                Ok(body) => {
                    let request_body = if pending_fetch.parent_brokered {
                        NativeRequestBody::StreamedBytes(body)
                    } else {
                        NativeRequestBody::Bytes(body)
                    };
                    if pending_fetch.parent_brokered {
                        let parent_fetch_broker =
                            parent_fetch_broker.as_mut().ok_or_else(|| {
                                NativeEngineError::Worker {
                                    operation: "parent-brokered fetch upload".into(),
                                    reason: "content process parent Fetch broker is unavailable"
                                        .into(),
                                }
                            })?;
                        let fetch_request = NativeFetchRequest {
                            document_url: &pending_fetch.document_url,
                            href: &pending_fetch.href,
                            method: pending_fetch.method,
                            body: Some(request_body),
                            content_type: pending_fetch.content_type,
                            request_headers: pending_fetch.headers,
                            credentials: pending_fetch.credentials,
                            credentials_mode: pending_fetch.credentials_mode,
                            referrer_url: pending_fetch.referrer_url,
                            referrer_policy: pending_fetch.referrer_policy,
                            cors_mode: pending_fetch.cors_mode,
                            redirect_mode: pending_fetch.redirect_mode,
                            cache_mode: pending_fetch.cache_mode,
                            timeout: pending_fetch.timeout,
                            max_response_bytes: None,
                        };
                        let (fetch, document_cookie, csp_violations) = parent_fetch_broker
                            .fetch(request_id, &fetch_request)
                            .await?;
                        runtime.set_cookie_state(document_cookie);
                        fetch_response_payload_with_csp_violations(fetch, csp_violations)
                    } else {
                        let intercepted = service_workers
                            .intercept_fetch(
                                loader,
                                &pending_fetch.document_url,
                                &pending_fetch.href,
                                pending_fetch.method.as_str(),
                                pending_fetch.headers.clone(),
                                Some(request_body.clone()),
                                pending_fetch.content_type.clone(),
                                pending_fetch.cors_mode,
                                pending_fetch.redirect_mode,
                                pending_fetch.timeout,
                                pending_fetch.credentials,
                                pending_fetch.credentials_mode,
                                pending_fetch.referrer.as_deref(),
                                pending_fetch.referrer_url.as_deref(),
                                pending_fetch.referrer_policy.as_deref(),
                                "fetch",
                                None,
                                None,
                                parent_fetch_broker.as_mut(),
                                false,
                            )
                            .await;
                        match intercepted {
                            Ok(NativeServiceWorkerFetchOutcome::Handled(response)) => {
                                fetch_response_payload(Ok(response))
                            }
                            Ok(NativeServiceWorkerFetchOutcome::NotHandled) => {
                                let fetch_request = NativeFetchRequest {
                                    document_url: &pending_fetch.document_url,
                                    href: &pending_fetch.href,
                                    method: pending_fetch.method,
                                    body: Some(request_body),
                                    content_type: pending_fetch.content_type,
                                    request_headers: pending_fetch.headers,
                                    credentials: pending_fetch.credentials,
                                    credentials_mode: pending_fetch.credentials_mode,
                                    referrer_url: pending_fetch.referrer_url,
                                    referrer_policy: pending_fetch.referrer_policy,
                                    cors_mode: pending_fetch.cors_mode,
                                    redirect_mode: pending_fetch.redirect_mode,
                                    cache_mode: pending_fetch.cache_mode,
                                    timeout: pending_fetch.timeout,
                                    max_response_bytes: None,
                                };
                                if is_parent_owned_network_target(
                                    &pending_fetch.document_url,
                                    &pending_fetch.href,
                                ) {
                                    if let Some(broker) = parent_fetch_broker.as_mut() {
                                        let (fetch, document_cookie, csp_violations) =
                                            broker.fetch(request_id, &fetch_request).await?;
                                        runtime.set_cookie_state(document_cookie);
                                        fetch_response_payload_with_csp_violations(
                                            fetch,
                                            csp_violations,
                                        )
                                    } else {
                                        fetch_response_payload(Err(NativeEngineError::Worker {
                                            operation:
                                                "parent-brokered Service Worker upload fallback"
                                                    .into(),
                                            reason:
                                                "content process parent Fetch broker is unavailable"
                                                    .into(),
                                        }))
                                    }
                                } else {
                                    let opened = loader
                                        .open_fetch_response_stream_async(fetch_request)
                                        .await;
                                    fetch_opened_response_payload(
                                        opened,
                                        request_id,
                                        fetch_stream_connections,
                                    )
                                    .await
                                }
                            }
                            Ok(NativeServiceWorkerFetchOutcome::Suspended) => {
                                fetch_response_payload(Err(NativeEngineError::Worker {
                                    operation: "content process script fetch".into(),
                                    reason:
                                        "Service Worker fetch is awaiting a browser WindowClient"
                                            .into(),
                                }))
                            }
                            Err(error) => fetch_response_payload(Err(error)),
                        }
                    }
                }
                Err(error) => fetch_response_payload(Err(error)),
            };
            process_page_fetch_resolution(
                request_id,
                payload,
                runtime,
                service_workers,
                websocket_connections,
                fetch_stream_connections,
                fetch_upload_connections,
                event_source_connections,
                loader,
                &mut next,
                &mut mutation,
                &mut current_url,
                document_origin,
                viewport,
                top_level_await_pending,
                &mut resolved_value,
                &mut pump_background_events,
                &mut pending,
                parent_fetch_broker.as_mut(),
            )
            .await?;
            continue;
        }
        if let Some(request_id) = selected_upload_fetch_completion.take() {
            let pending_fetch = pending_upload_fetches.remove(&request_id).ok_or_else(|| {
                NativeEngineError::Worker {
                    operation: "fetch request upload completion".into(),
                    reason: "completed fetch upload task was not active".into(),
                }
            })?;
            let (opened, task_loader) = pending_fetch.task.await.map_err(|_| {
                NativeEngineError::worker_failure(
                    "fetch request upload task",
                    NativeWorkerFailureKind::Transport,
                    "native fetch upload task terminated unexpectedly",
                )
            })?;
            if let Some(connection) = fetch_upload_connections.remove(&request_id) {
                let _ = connection
                    .commands
                    .try_send(NativeFetchUploadCommand::Cancel);
            }
            let Some(loader) = loader.as_deref_mut() else {
                return Err(NativeEngineError::Worker {
                    operation: "content process fetch upload completion".into(),
                    reason: "content process has no resource loader".into(),
                });
            };
            loader.merge_fetch_task_state(task_loader)?;
            let payload =
                fetch_opened_response_payload(opened, request_id, fetch_stream_connections).await;
            process_page_fetch_resolution(
                request_id,
                payload,
                runtime,
                service_workers,
                websocket_connections,
                fetch_stream_connections,
                fetch_upload_connections,
                event_source_connections,
                loader,
                &mut next,
                &mut mutation,
                &mut current_url,
                document_origin,
                viewport,
                top_level_await_pending,
                &mut resolved_value,
                &mut pump_background_events,
                &mut pending,
                parent_fetch_broker.as_mut(),
            )
            .await?;
            continue;
        }
        if selected_source == Some(NativeContentTaskSource::FetchUpload)
            && let Some(stream_id) = selected_fetch_upload_event.take()
        {
            let event_evaluation = runtime.dispatch_fetch_upload_event(
                stream_id,
                &fetch_upload_event_payload(),
                &next,
                &current_url,
                document_origin,
                viewport,
            )?;
            let event_commands = process_websocket_commands(
                event_evaluation.commands,
                websocket_connections,
                &current_url,
                parent_fetch_broker.as_mut(),
            )
            .await?;
            let event_commands = process_event_source_commands(
                event_commands,
                event_source_connections,
                loader.as_deref(),
                &current_url,
                parent_fetch_broker.as_mut(),
            )
            .await?;
            let event_commands =
                process_fetch_stream_commands(event_commands, fetch_stream_connections)?;
            let event_commands =
                process_fetch_upload_commands(event_commands, fetch_upload_connections)?;
            if event_commands.is_empty() {
                if top_level_await_pending
                    && let Some(value) = runtime.take_top_level_await_result()?
                {
                    resolved_value = Some(value);
                }
                continue;
            }
            let event_history = extract_history_commands(&event_commands);
            if !event_history.is_empty() {
                let mut url = Some(current_url.clone());
                apply_content_runtime_history(&event_history, &mut url, document_origin, runtime)?;
                current_url = url.ok_or_else(|| NativeEngineError::Worker {
                    operation: "content process fetch upload history".into(),
                    reason: "fetch upload callback history lost its document URL".into(),
                })?;
                mutation.history.extend(event_history);
            }
            let (event_next, event_mutation, dynamic_result) = mutate_script_document(
                &next,
                runtime,
                &current_url,
                document_origin,
                viewport,
                &event_commands,
                loader.as_deref_mut(),
                parent_fetch_broker.as_mut(),
            )
            .await?;
            next = event_next;
            mutation.events.extend(event_mutation.events);
            mutation
                .scroll_commands
                .extend(event_mutation.scroll_commands);
            if !event_mutation.history.is_empty() {
                current_url = resolve_content_history_document_url(
                    &event_mutation.history,
                    &current_url,
                    document_origin,
                )?;
                mutation.history.extend(event_mutation.history);
            }
            if mutation.navigation.is_none() {
                mutation.navigation = event_mutation.navigation;
            } else if event_mutation.navigation.is_some() {
                return Err(NativeEngineError::TargetNotActionable {
                    reason: "one fetch upload event batch cannot activate multiple navigations"
                        .into(),
                });
            }
            let (dynamic_fetches, dynamic_background) = activate_dynamic_page_script_network(
                dynamic_result,
                websocket_connections,
                event_source_connections,
                loader.as_deref(),
                &current_url,
                parent_fetch_broker.as_mut(),
            )
            .await?;
            pending.extend(fetch_commands(&event_commands)?);
            pending.extend(fetch_commands(&dynamic_fetches)?);
            pump_background_events |= dynamic_background;
            if top_level_await_pending && let Some(value) = runtime.take_top_level_await_result()? {
                resolved_value = Some(value);
            }
            continue;
        }
        if selected_source == Some(NativeContentTaskSource::WebSocket)
            && let Some((socket_id, event)) = selected_websocket_event.take()
        {
            event_loop_turns = event_loop_turns.saturating_add(1);
            if event_loop_turns > MAX_CONTENT_EVENT_LOOP_TURNS {
                return Err(NativeEngineError::limit(
                    "content-process event-loop turns",
                    MAX_CONTENT_EVENT_LOOP_TURNS,
                    event_loop_turns,
                ));
            }
            let remove_after_dispatch = matches!(&event, NativeWebSocketEvent::Close { .. });
            let event_evaluation = runtime.dispatch_websocket_event(
                socket_id,
                &websocket_event_payload(&event),
                NativePageEventContext {
                    document: &next,
                    document_url: &current_url,
                    origin: document_origin,
                    viewport,
                    csp_violations: websocket_event_csp_violations(&event),
                },
            )?;
            let event_commands = process_websocket_commands(
                event_evaluation.commands,
                websocket_connections,
                &current_url,
                parent_fetch_broker.as_mut(),
            )
            .await?;
            let event_commands =
                process_fetch_stream_commands(event_commands, fetch_stream_connections)?;
            let event_commands =
                process_fetch_upload_commands(event_commands, fetch_upload_connections)?;
            let event_history = extract_history_commands(&event_commands);
            if !event_history.is_empty() {
                let mut url = Some(current_url.clone());
                apply_content_runtime_history(&event_history, &mut url, document_origin, runtime)?;
                current_url = url.ok_or_else(|| NativeEngineError::Worker {
                    operation: "content process WebSocket history".into(),
                    reason: "WebSocket callback history lost its document URL".into(),
                })?;
                mutation.history.extend(event_history);
            }
            let (event_next, event_mutation, dynamic_result) = mutate_script_document(
                &next,
                runtime,
                &current_url,
                document_origin,
                viewport,
                &event_commands,
                loader.as_deref_mut(),
                parent_fetch_broker.as_mut(),
            )
            .await?;
            next = event_next;
            mutation.events.extend(event_mutation.events);
            mutation
                .scroll_commands
                .extend(event_mutation.scroll_commands);
            if !event_mutation.history.is_empty() {
                current_url = resolve_content_history_document_url(
                    &event_mutation.history,
                    &current_url,
                    document_origin,
                )?;
                mutation.history.extend(event_mutation.history);
            }
            if mutation.navigation.is_none() {
                mutation.navigation = event_mutation.navigation;
            } else if event_mutation.navigation.is_some() {
                return Err(NativeEngineError::TargetNotActionable {
                    reason: "one WebSocket event batch cannot activate multiple navigations".into(),
                });
            }
            let (dynamic_fetches, dynamic_background) = activate_dynamic_page_script_network(
                dynamic_result,
                websocket_connections,
                event_source_connections,
                loader.as_deref(),
                &current_url,
                parent_fetch_broker.as_mut(),
            )
            .await?;
            pending.extend(fetch_commands(&event_commands)?);
            pending.extend(fetch_commands(&dynamic_fetches)?);
            pump_background_events |= dynamic_background;
            if remove_after_dispatch {
                websocket_connections.remove(&socket_id);
            }
            if top_level_await_pending && let Some(value) = runtime.take_top_level_await_result()? {
                resolved_value = Some(value);
            }
            continue;
        }
        if selected_source == Some(NativeContentTaskSource::FetchStream)
            && let Some((stream_id, event)) = selected_fetch_stream_event.take()
        {
            event_loop_turns = event_loop_turns.saturating_add(1);
            if event_loop_turns > MAX_CONTENT_EVENT_LOOP_TURNS {
                return Err(NativeEngineError::limit(
                    "content-process event-loop turns",
                    MAX_CONTENT_EVENT_LOOP_TURNS,
                    event_loop_turns,
                ));
            }
            let remove_after_dispatch = matches!(
                &event,
                NativeFetchStreamEvent::End | NativeFetchStreamEvent::Error { .. }
            );
            let event_evaluation = runtime.dispatch_fetch_stream_event(
                stream_id,
                &fetch_stream_event_payload(&event),
                &next,
                &current_url,
                document_origin,
                viewport,
            )?;
            let event_commands = process_websocket_commands(
                event_evaluation.commands,
                websocket_connections,
                &current_url,
                parent_fetch_broker.as_mut(),
            )
            .await?;
            let event_commands = process_event_source_commands(
                event_commands,
                event_source_connections,
                loader.as_deref(),
                &current_url,
                parent_fetch_broker.as_mut(),
            )
            .await?;
            let event_commands =
                process_fetch_stream_commands(event_commands, fetch_stream_connections)?;
            let event_commands =
                process_fetch_upload_commands(event_commands, fetch_upload_connections)?;
            let event_history = extract_history_commands(&event_commands);
            if !event_history.is_empty() {
                let mut url = Some(current_url.clone());
                apply_content_runtime_history(&event_history, &mut url, document_origin, runtime)?;
                current_url = url.ok_or_else(|| NativeEngineError::Worker {
                    operation: "content process fetch stream history".into(),
                    reason: "fetch stream callback history lost its document URL".into(),
                })?;
                mutation.history.extend(event_history);
            }
            let (event_next, event_mutation, dynamic_result) = mutate_script_document(
                &next,
                runtime,
                &current_url,
                document_origin,
                viewport,
                &event_commands,
                loader.as_deref_mut(),
                parent_fetch_broker.as_mut(),
            )
            .await?;
            next = event_next;
            mutation.events.extend(event_mutation.events);
            mutation
                .scroll_commands
                .extend(event_mutation.scroll_commands);
            if !event_mutation.history.is_empty() {
                current_url = resolve_content_history_document_url(
                    &event_mutation.history,
                    &current_url,
                    document_origin,
                )?;
                mutation.history.extend(event_mutation.history);
            }
            if mutation.navigation.is_none() {
                mutation.navigation = event_mutation.navigation;
            } else if event_mutation.navigation.is_some() {
                return Err(NativeEngineError::TargetNotActionable {
                    reason: "one fetch stream event batch cannot activate multiple navigations"
                        .into(),
                });
            }
            let (dynamic_fetches, dynamic_background) = activate_dynamic_page_script_network(
                dynamic_result,
                websocket_connections,
                event_source_connections,
                loader.as_deref(),
                &current_url,
                parent_fetch_broker.as_mut(),
            )
            .await?;
            pending.extend(fetch_commands(&event_commands)?);
            pending.extend(fetch_commands(&dynamic_fetches)?);
            pump_background_events |= dynamic_background;
            if remove_after_dispatch {
                fetch_stream_connections.remove(&stream_id);
            }
            if top_level_await_pending && let Some(value) = runtime.take_top_level_await_result()? {
                resolved_value = Some(value);
            }
            continue;
        }
        if selected_source.is_none()
            && fetch_stream_connections
                .values()
                .any(|connection| connection.read_pending)
        {
            sleep(NATIVE_WEBSOCKET_POLL_INTERVAL).await;
            continue;
        }
        if selected_source.is_none()
            && (!pending_upload_fetches.is_empty() || !pending_controlled_uploads.is_empty())
        {
            sleep(NATIVE_WEBSOCKET_POLL_INTERVAL).await;
            continue;
        }
        if selected_source == Some(NativeContentTaskSource::EventSource)
            && let Some((source_id, event)) = selected_event_source_event.take()
        {
            event_loop_turns = event_loop_turns.saturating_add(1);
            if event_loop_turns > MAX_CONTENT_EVENT_LOOP_TURNS {
                return Err(NativeEngineError::limit(
                    "content-process event-loop turns",
                    MAX_CONTENT_EVENT_LOOP_TURNS,
                    event_loop_turns,
                ));
            }
            if let NativeEventSourceEvent::Reconnect { last_event_id } = &event {
                let (initiator_url, href, with_credentials, old_stream_id) =
                    event_source_connections
                        .get_mut(&source_id)
                        .map(|connection| {
                            (
                                connection.parent_initiator_url.clone(),
                                connection.parent_href.clone(),
                                connection.parent_with_credentials,
                                connection.parent_stream_id.take(),
                            )
                        })
                        .ok_or_else(|| {
                            NativeEngineError::worker_failure(
                                "parent EventSource reconnect",
                                NativeWorkerFailureKind::Protocol,
                                "reconnecting EventSource is no longer active",
                            )
                        })?;
                let broker =
                    parent_fetch_broker
                        .as_mut()
                        .ok_or_else(|| NativeEngineError::Worker {
                            operation: "parent EventSource reconnect".into(),
                            reason: "parent cookie and network authority is unavailable".into(),
                        })?;
                if let Some(stream_id) = old_stream_id {
                    broker
                        .close_parent_event_source(source_id, None, stream_id)
                        .await?;
                }
                let reopened = broker
                    .open_event_source(
                        source_id,
                        None,
                        &initiator_url,
                        &href,
                        with_credentials,
                        last_event_id,
                    )
                    .await?;
                let connection = event_source_connections
                    .get_mut(&source_id)
                    .ok_or_else(|| {
                        NativeEngineError::worker_failure(
                            "parent EventSource reconnect",
                            NativeWorkerFailureKind::Protocol,
                            "reconnecting EventSource closed while its request was opening",
                        )
                    })?;
                let command = match reopened {
                    NativeParentEventSourceOpenResult::Opened(open) => {
                        connection.parent_stream_id = Some(open.stream_id);
                        NativeEventSourceCommand::Reopened {
                            origin: open.origin,
                            csp_violations: open.csp_violations,
                        }
                    }
                    NativeParentEventSourceOpenResult::Failed {
                        message,
                        csp_violations,
                    } => NativeEventSourceCommand::OpenFailed {
                        message,
                        csp_violations,
                    },
                };
                connection.commands.send(command).await.map_err(|_| {
                    NativeEngineError::worker_failure(
                        "parent EventSource reconnect",
                        NativeWorkerFailureKind::Transport,
                        "EventSource parser task terminated before receiving its reconnect result",
                    )
                })?;
                continue;
            }
            if matches!(&event, NativeEventSourceEvent::Close)
                && let Some(connection) = event_source_connections.get_mut(&source_id)
                && let Some(stream_id) = connection.parent_stream_id.take()
            {
                let broker =
                    parent_fetch_broker
                        .as_mut()
                        .ok_or_else(|| NativeEngineError::Worker {
                            operation: "parent EventSource close".into(),
                            reason: "parent cookie and network authority is unavailable".into(),
                        })?;
                broker
                    .close_parent_event_source(source_id, None, stream_id)
                    .await?;
            }
            if let NativeEventSourceEvent::Open { cookie_changes, .. } = &event
                && !cookie_changes.is_empty()
            {
                let Some(loader) = loader.as_deref_mut() else {
                    return Err(NativeEngineError::Worker {
                        operation: "content process EventSource cookies".into(),
                        reason: "content process has no resource loader".into(),
                    });
                };
                loader.apply_cookie_changes(cookie_changes)?;
            }
            let remove_after_dispatch = matches!(&event, NativeEventSourceEvent::Close);
            let event_evaluation = runtime.dispatch_event_source_event(
                source_id,
                &event_source_event_payload(&event),
                NativePageEventContext {
                    document: &next,
                    document_url: &current_url,
                    origin: document_origin,
                    viewport,
                    csp_violations: event_source_csp_violations(&event),
                },
            )?;
            let event_commands = process_websocket_commands(
                event_evaluation.commands,
                websocket_connections,
                &current_url,
                parent_fetch_broker.as_mut(),
            )
            .await?;
            let event_commands = process_event_source_commands(
                event_commands,
                event_source_connections,
                loader.as_deref(),
                &current_url,
                parent_fetch_broker.as_mut(),
            )
            .await?;
            let event_commands =
                process_fetch_stream_commands(event_commands, fetch_stream_connections)?;
            let event_commands =
                process_fetch_upload_commands(event_commands, fetch_upload_connections)?;
            let event_history = extract_history_commands(&event_commands);
            if !event_history.is_empty() {
                let mut url = Some(current_url.clone());
                apply_content_runtime_history(&event_history, &mut url, document_origin, runtime)?;
                current_url = url.ok_or_else(|| NativeEngineError::Worker {
                    operation: "content process EventSource history".into(),
                    reason: "EventSource callback history lost its document URL".into(),
                })?;
                mutation.history.extend(event_history);
            }
            let (event_next, event_mutation, dynamic_result) = mutate_script_document(
                &next,
                runtime,
                &current_url,
                document_origin,
                viewport,
                &event_commands,
                loader.as_deref_mut(),
                parent_fetch_broker.as_mut(),
            )
            .await?;
            next = event_next;
            mutation.events.extend(event_mutation.events);
            mutation
                .scroll_commands
                .extend(event_mutation.scroll_commands);
            if !event_mutation.history.is_empty() {
                current_url = resolve_content_history_document_url(
                    &event_mutation.history,
                    &current_url,
                    document_origin,
                )?;
                mutation.history.extend(event_mutation.history);
            }
            if mutation.navigation.is_none() {
                mutation.navigation = event_mutation.navigation;
            } else if event_mutation.navigation.is_some() {
                return Err(NativeEngineError::TargetNotActionable {
                    reason: "one EventSource event batch cannot activate multiple navigations"
                        .into(),
                });
            }
            let (dynamic_fetches, dynamic_background) = activate_dynamic_page_script_network(
                dynamic_result,
                websocket_connections,
                event_source_connections,
                loader.as_deref(),
                &current_url,
                parent_fetch_broker.as_mut(),
            )
            .await?;
            pending.extend(fetch_commands(&event_commands)?);
            pending.extend(fetch_commands(&dynamic_fetches)?);
            pump_background_events |= dynamic_background;
            if remove_after_dispatch {
                event_source_connections.remove(&source_id);
            }
            if top_level_await_pending && let Some(value) = runtime.take_top_level_await_result()? {
                resolved_value = Some(value);
            }
            continue;
        }
        if !top_level_await_pending {
            break;
        }
        let timer_delay = runtime.next_timer_delay_ms()?;
        if let Some(delay_ms) = timer_delay
            && delay_ms > 0
        {
            sleep(Duration::from_millis(
                delay_ms.min(NATIVE_WEBSOCKET_POLL_INTERVAL.as_millis() as u64),
            ))
            .await;
            continue;
        }
        if timer_delay.is_none()
            && (!websocket_connections.is_empty() || !event_source_connections.is_empty())
        {
            sleep(NATIVE_WEBSOCKET_POLL_INTERVAL).await;
            continue;
        }
        let Some(delay_ms) = timer_delay else {
            return Err(NativeEngineError::Worker {
                operation: "content process script event loop".into(),
                reason: "top-level await remained pending without a native host operation".into(),
            });
        };
        event_loop_turns = event_loop_turns.saturating_add(1);
        if event_loop_turns > MAX_CONTENT_EVENT_LOOP_TURNS {
            return Err(NativeEngineError::limit(
                "content-process event-loop turns",
                MAX_CONTENT_EVENT_LOOP_TURNS,
                event_loop_turns,
            ));
        }
        if delay_ms > 0 {
            sleep(Duration::from_millis(delay_ms)).await;
        }
        let timer_evaluation =
            runtime.run_timer_turn(&next, &current_url, document_origin, viewport)?;
        let timer_commands = process_websocket_commands(
            timer_evaluation.commands,
            websocket_connections,
            &current_url,
            parent_fetch_broker.as_mut(),
        )
        .await?;
        let timer_commands = process_event_source_commands(
            timer_commands,
            event_source_connections,
            loader.as_deref(),
            &current_url,
            parent_fetch_broker.as_mut(),
        )
        .await?;
        let timer_commands =
            process_fetch_stream_commands(timer_commands, fetch_stream_connections)?;
        let timer_commands =
            process_fetch_upload_commands(timer_commands, fetch_upload_connections)?;
        let timer_history = extract_history_commands(&timer_commands);
        if !timer_history.is_empty() {
            let mut url = Some(current_url.clone());
            apply_content_runtime_history(&timer_history, &mut url, document_origin, runtime)?;
            current_url = url.ok_or_else(|| NativeEngineError::Worker {
                operation: "content process timer history".into(),
                reason: "timer callback history lost its document URL".into(),
            })?;
            mutation.history.extend(timer_history);
        }
        let (timer_next, timer_mutation, dynamic_result) = mutate_script_document(
            &next,
            runtime,
            &current_url,
            document_origin,
            viewport,
            &timer_commands,
            loader.as_deref_mut(),
            parent_fetch_broker.as_mut(),
        )
        .await?;
        next = timer_next;
        mutation.events.extend(timer_mutation.events);
        mutation
            .scroll_commands
            .extend(timer_mutation.scroll_commands);
        if !timer_mutation.history.is_empty() {
            current_url = resolve_content_history_document_url(
                &timer_mutation.history,
                &current_url,
                document_origin,
            )?;
            mutation.history.extend(timer_mutation.history);
        }
        if mutation.navigation.is_none() {
            mutation.navigation = timer_mutation.navigation;
        } else if timer_mutation.navigation.is_some() {
            return Err(NativeEngineError::TargetNotActionable {
                reason: "one script event-loop batch cannot activate multiple navigations".into(),
            });
        }
        let (dynamic_fetches, dynamic_background) = activate_dynamic_page_script_network(
            dynamic_result,
            websocket_connections,
            event_source_connections,
            loader.as_deref(),
            &current_url,
            parent_fetch_broker.as_mut(),
        )
        .await?;
        pending.extend(fetch_commands(&timer_commands)?);
        pending.extend(fetch_commands(&dynamic_fetches)?);
        pump_background_events |= dynamic_background;
    }
    if mutation.events.len() > MAX_NATIVE_EFFECTS {
        return Err(NativeEngineError::limit(
            "script fetch effects",
            MAX_NATIVE_EFFECTS,
            mutation.events.len(),
        ));
    }
    mutation.document = next.to_content_wire();
    Ok((next, mutation, resolved_value))
}

enum ScriptNavigationTarget {
    Link {
        node_index: u32,
        href: String,
    },
    Form {
        node_index: u32,
        form_id: NativeNodeId,
        dispatch_submit: bool,
        submitter: Option<NativeNodeId>,
        target: String,
    },
    Location {
        href: String,
        replace_history: bool,
    },
}

fn script_navigation_target(
    document: &NativeDocument,
    document_url: &str,
    commands: &[NativeScriptCommand],
) -> Result<Option<ScriptNavigationTarget>, NativeEngineError> {
    let mut navigation = None;
    for command in commands {
        let target = match command {
            NativeScriptCommand::Click { node_index } => {
                let node_id = NativeNodeId::from_parts(document.generation(), *node_index);
                if let Some(href) = document.link_href(node_id).filter(|href| !href.is_empty()) {
                    Some(ScriptNavigationTarget::Link {
                        node_index: *node_index,
                        href: href.to_owned(),
                    })
                } else if let Some(form_id) = document.submit_control_form(node_id) {
                    let target = document.form_submission_target(form_id, Some(node_id))?;
                    document.form_submission_request_with_submitter(
                        form_id,
                        document_url,
                        Some(node_id),
                    )?;
                    Some(ScriptNavigationTarget::Form {
                        node_index: form_id.index(),
                        form_id,
                        dispatch_submit: true,
                        submitter: Some(node_id),
                        target,
                    })
                } else {
                    None
                }
            }
            NativeScriptCommand::SubmitForm { node_index } => {
                let node_id = NativeNodeId::from_parts(document.generation(), *node_index);
                let target = document.form_submission_target(node_id, None)?;
                document.form_submission_request(node_id, document_url)?;
                Some(ScriptNavigationTarget::Form {
                    node_index: *node_index,
                    form_id: node_id,
                    dispatch_submit: false,
                    submitter: None,
                    target,
                })
            }
            NativeScriptCommand::RequestSubmitForm {
                node_index,
                submitter_index,
            } => {
                let node_id = NativeNodeId::from_parts(document.generation(), *node_index);
                let submitter = submitter_index
                    .map(|index| NativeNodeId::from_parts(document.generation(), index));
                if let Some(submitter) = submitter
                    && document.submit_control_form(submitter) != Some(node_id)
                {
                    return Err(NativeEngineError::TargetNotActionable {
                        reason: "requestSubmit submitter must be a submit control for the form"
                            .into(),
                    });
                }
                let target = document.form_submission_target(node_id, submitter)?;
                document.form_submission_request_with_submitter(
                    node_id,
                    document_url,
                    submitter,
                )?;
                Some(ScriptNavigationTarget::Form {
                    node_index: *node_index,
                    form_id: node_id,
                    dispatch_submit: true,
                    submitter,
                    target,
                })
            }
            NativeScriptCommand::Navigate { href, replace } => {
                validate_url_text("script location href", href)?;
                Some(ScriptNavigationTarget::Location {
                    href: href.to_owned(),
                    replace_history: *replace,
                })
            }
            _ => None,
        };
        let Some(target) = target else { continue };
        if navigation.is_some() {
            return Err(NativeEngineError::TargetNotActionable {
                reason: "one script batch cannot activate multiple navigations".into(),
            });
        }
        navigation = Some(target);
    }
    Ok(navigation)
}

fn event_kind_text(kind: NativeEventKind) -> &'static str {
    match kind {
        NativeEventKind::Blur => "blur",
        NativeEventKind::FocusOut => "focusout",
        NativeEventKind::Focus => "focus",
        NativeEventKind::FocusIn => "focusin",
        NativeEventKind::ReadyStateChange => "readystatechange",
        NativeEventKind::DomContentLoaded => "DOMContentLoaded",
        NativeEventKind::Load => "load",
        NativeEventKind::Error => "error",
        NativeEventKind::PageHide => "pagehide",
        NativeEventKind::Unload => "unload",
        NativeEventKind::PageShow => "pageshow",
        NativeEventKind::BeforeUnload => "beforeunload",
        NativeEventKind::HashChange => "hashchange",
        NativeEventKind::PopState => "popstate",
        NativeEventKind::Invalid => "invalid",
        NativeEventKind::KeyDown => "keydown",
        NativeEventKind::KeyUp => "keyup",
        NativeEventKind::Submit => "submit",
        NativeEventKind::Reset => "reset",
        NativeEventKind::Click => "click",
        NativeEventKind::MouseOver => "mouseover",
        NativeEventKind::MouseEnter => "mouseenter",
        NativeEventKind::DragStart => "dragstart",
        NativeEventKind::DragEnter => "dragenter",
        NativeEventKind::DragOver => "dragover",
        NativeEventKind::Drop => "drop",
        NativeEventKind::DragEnd => "dragend",
        NativeEventKind::Input => "input",
        NativeEventKind::Change => "change",
        NativeEventKind::Scroll => "scroll",
    }
}

fn parse_event_kind(value: &str) -> Option<NativeEventKind> {
    match value {
        "blur" => Some(NativeEventKind::Blur),
        "focusout" => Some(NativeEventKind::FocusOut),
        "focus" => Some(NativeEventKind::Focus),
        "focusin" => Some(NativeEventKind::FocusIn),
        "readystatechange" => Some(NativeEventKind::ReadyStateChange),
        "DOMContentLoaded" => Some(NativeEventKind::DomContentLoaded),
        "load" => Some(NativeEventKind::Load),
        "error" => Some(NativeEventKind::Error),
        "pagehide" => Some(NativeEventKind::PageHide),
        "unload" => Some(NativeEventKind::Unload),
        "pageshow" => Some(NativeEventKind::PageShow),
        "beforeunload" => Some(NativeEventKind::BeforeUnload),
        "hashchange" => Some(NativeEventKind::HashChange),
        "popstate" => Some(NativeEventKind::PopState),
        "invalid" => Some(NativeEventKind::Invalid),
        "keydown" => Some(NativeEventKind::KeyDown),
        "keyup" => Some(NativeEventKind::KeyUp),
        "submit" => Some(NativeEventKind::Submit),
        "reset" => Some(NativeEventKind::Reset),
        "click" => Some(NativeEventKind::Click),
        "mouseover" => Some(NativeEventKind::MouseOver),
        "mouseenter" => Some(NativeEventKind::MouseEnter),
        "dragstart" => Some(NativeEventKind::DragStart),
        "dragenter" => Some(NativeEventKind::DragEnter),
        "dragover" => Some(NativeEventKind::DragOver),
        "drop" => Some(NativeEventKind::Drop),
        "dragend" => Some(NativeEventKind::DragEnd),
        "input" => Some(NativeEventKind::Input),
        "change" => Some(NativeEventKind::Change),
        "scroll" => Some(NativeEventKind::Scroll),
        _ => None,
    }
}

async fn write_value_frame(
    writer: &mut (impl AsyncWrite + Unpin),
    value: &Value,
) -> Result<(), NativeEngineError> {
    let payload = serde_json::to_vec(value).map_err(|_| NativeEngineError::Worker {
        operation: "encode content IPC".into(),
        reason: "content process response could not be encoded".into(),
    })?;
    write_frame(writer, &payload).await
}

fn content_error_response(id: Value, error: NativeEngineError) -> Value {
    let mut response = json!({
        "kind": "error",
        "id": id,
        "reason": error.to_string(),
    });
    if let NativeEngineError::LimitExceeded {
        resource,
        limit,
        actual,
    } = error
    {
        response["error_type"] = Value::String("resource_limit".into());
        response["resource"] = Value::String(resource);
        response["limit"] = Value::from(limit as u64);
        response["actual"] = Value::from(actual as u64);
    }
    response
}

fn protocol_matches(request: &Value) -> bool {
    request.get("protocol").and_then(Value::as_u64) == Some(CONTENT_WORKER_PROTOCOL_VERSION)
}

fn worker_binary_path() -> Result<PathBuf, NativeEngineError> {
    if let Ok(path) = std::env::var("GLASS_NATIVE_CONTENT_WORKER")
        && !path.is_empty()
    {
        return Ok(PathBuf::from(path));
    }
    let current = std::env::current_exe().map_err(|_| {
        NativeEngineError::worker_failure(
            "locate content process",
            NativeWorkerFailureKind::Spawn,
            "current executable path is unavailable",
        )
    })?;
    let name = if cfg!(windows) {
        "glass-native-content-worker.exe"
    } else {
        "glass-native-content-worker"
    };
    let mut directory = current.parent();
    for _ in 0..3 {
        let Some(parent) = directory else {
            break;
        };
        let candidate = parent.join(name);
        if candidate.is_file() {
            return Ok(candidate);
        }
        directory = parent.parent();
    }
    Err(NativeEngineError::worker_failure(
        "locate content process",
        NativeWorkerFailureKind::Spawn,
        "native content worker executable was not found; build the glass-native-content-worker binary",
    ))
}

#[cfg(test)]
#[path = "content_process/tests.rs"]
mod tests;
