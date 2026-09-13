use super::browsing_context::NATIVE_CONTEXT_ID;
use super::config::{
    MAX_NATIVE_NODES, NativeEngineLimits, Viewport, is_network_url, validate_context_id,
    validate_url_text, validate_window_name, without_fragment,
};
use super::dom::{
    NativeDocument, NativeDocumentWire, NativeNodeId, NativePageScriptSource,
    NativePageScriptTiming,
};
use super::error::{NativeEngineError, NativeWorkerFailureKind};
use super::interaction::{
    MAX_NATIVE_EFFECTS, MAX_NATIVE_FORM_BODY_BYTES, NativeEventKind, NativeFile,
    validate_native_edit_key, validate_native_key,
};
use super::javascript::{
    MAX_NATIVE_DIALOG_TEXT_BYTES, MAX_NATIVE_DIALOGS, MAX_NATIVE_EVENTSOURCE_FIELD_BYTES,
    MAX_NATIVE_EVENTSOURCE_MESSAGE_BYTES, MAX_NATIVE_FETCH_STREAM_CHUNK_BYTES,
    MAX_NATIVE_HISTORY_STATE_BYTES, MAX_NATIVE_INDEXED_DB_CHANGES, MAX_NATIVE_MODULE_IMPORTS,
    MAX_NATIVE_SCRIPT_BYTES, MAX_NATIVE_WEBSOCKET_CLOSE_REASON_BYTES,
    MAX_NATIVE_WEBSOCKET_MESSAGE_BYTES, MAX_NATIVE_WEBSOCKET_PROTOCOL_BYTES,
    MAX_NATIVE_WEBSOCKET_PROTOCOLS, MAX_NATIVE_XHR_TIMEOUT_MS, NativeCookieChange,
    NativeCookieProfileEntry, NativeDialog, NativeFrameScriptBinding, NativeFrameScriptContext,
    NativeFrameScriptRequest, NativeFrameScriptWindow, NativeIndexedDbChange, NativeIndexedDbState,
    NativeJavaScriptRuntime, NativePageScript, NativePageScriptResult, NativePopupRequest,
    NativePostMessageRequest, NativeScriptCommand, NativeScriptEvaluation, NativeStorageEvent,
    NativeWebStorageState, NativeWindowCloseRequest, NativeWindowNavigationRequest,
    NativeWindowProxyUpdate, NativeWorkerEventSourceCommand, NativeWorkerMessage,
    NativeWorkerRegistry, NativeWorkerWebSocketCommand, diff_indexed_db_changes,
    execute_dynamic_page_scripts, execute_page_scripts, host_event_script,
    host_hash_change_event_script, host_key_event_script, host_key_event_script_with_modifiers,
    host_submit_event_script, literal_dynamic_module_specifiers, load_indexed_db_profile,
    load_web_storage_profile, order_page_scripts, page_script_sources_to_scripts,
    save_web_storage_profile, static_module_specifiers, storage_key, worker_message_script,
};
use super::layout::NativePoint;
use super::origin::NativeOrigin;
use super::resource_loader::{
    MAX_NATIVE_RESPONSE_HEADER_BYTES, MAX_NATIVE_RESPONSE_HEADER_NAME_BYTES,
    MAX_NATIVE_RESPONSE_HEADER_VALUE_BYTES, MAX_NATIVE_RESPONSE_HEADERS, NativeCorsMode,
    NativeFetchRedirectMode, NativeFetchRequest, NativeFetchResponse, NativeFetchResponseStream,
    NativeNavigationMethod, NativeNavigationRequest, NativeRequestBody, NativeResourceLoader,
    NativeWebSocketTarget,
};
#[cfg(windows)]
use super::sandbox::NativeContentSandbox;
use super::sandbox::prepare_worker_command;
use base64::Engine as _;
use futures_util::{SinkExt, StreamExt};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::process::{Child, ChildStdin, ChildStdout};
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
const MAX_CONTENT_DOCUMENT_WIRE_BYTES: usize = 16 * 1024 * 1024;
const CONTENT_WORKER_PROTOCOL_VERSION: u64 = 8;
const CONTENT_PROCESS_LOAD_TIMEOUT: Duration = Duration::from_secs(30);
const CONTENT_PROCESS_MUTATION_TIMEOUT: Duration = Duration::from_secs(5);
const CONTENT_PROCESS_SCRIPT_TIMEOUT: Duration = Duration::from_secs(5);
const MAX_CONTENT_EVENT_LOOP_TURNS: usize = MAX_NATIVE_EFFECTS;
const NATIVE_WEBSOCKET_CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
const NATIVE_WEBSOCKET_POLL_INTERVAL: Duration = Duration::from_millis(10);
const MAX_NATIVE_WEBSOCKET_EVENTS: usize = MAX_NATIVE_EFFECTS;
const NATIVE_EVENTSOURCE_INITIAL_RETRY: Duration = Duration::from_secs(3);
const NATIVE_EVENTSOURCE_MAX_RETRY: Duration = Duration::from_secs(30);
const MAX_NATIVE_EVENTSOURCE_RECONNECTS: usize = MAX_NATIVE_EFFECTS;
const MAX_NATIVE_EVENTSOURCE_CONNECTIONS: usize = MAX_NATIVE_EFFECTS;
const MAX_NATIVE_FETCH_STREAM_CONNECTIONS: usize = MAX_NATIVE_EFFECTS;
const MAX_CONTENT_STYLESHEETS: usize = 16;
const MAX_CONTENT_STYLESHEET_BYTES: usize = 512 * 1024;
const MAX_CONTENT_IMAGES: usize = 64;
const MAX_CONTENT_FRAME_SOURCES: usize = 64;

pub(crate) struct NativeContentLoad {
    pub(crate) url: String,
    pub(crate) origin: NativeOrigin,
    pub(crate) document: NativeDocumentWire,
    pub(crate) frame_sources: Option<Vec<String>>,
    pub(crate) events: Vec<NativeContentEvent>,
    pub(crate) scroll_commands: Vec<NativeScriptCommand>,
    pub(crate) navigation: Option<NativeContentNavigation>,
    pub(crate) storage_events: Vec<NativeStorageEvent>,
    pub(crate) indexed_db_changes: Vec<NativeIndexedDbChange>,
    pub(crate) dialogs: Vec<NativeDialog>,
    pub(crate) popups: Vec<NativePopupRequest>,
    pub(crate) post_messages: Vec<NativePostMessageRequest>,
    pub(crate) window_closes: Vec<NativeWindowCloseRequest>,
    pub(crate) window_navigations: Vec<NativeWindowNavigationRequest>,
    pub(crate) window_name: String,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct NativeContentEvent {
    pub(crate) node_index: u32,
    pub(crate) kind: NativeEventKind,
}

enum NativeWebSocketCommand {
    Send(Message),
    Close { code: u16, reason: String },
}

enum NativeWebSocketEvent {
    Open {
        protocol: String,
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
    },
    Close {
        code: u16,
        reason: String,
        was_clean: bool,
    },
}

struct NativeWebSocketConnection {
    commands: mpsc::Sender<NativeWebSocketCommand>,
    events: mpsc::Receiver<NativeWebSocketEvent>,
}

enum NativeFetchStreamCommand {
    Read,
    Cancel,
}

enum NativeFetchStreamEvent {
    Chunk { data: Vec<u8> },
    End,
    Error { message: String },
}

struct NativeFetchStreamConnection {
    commands: mpsc::Sender<NativeFetchStreamCommand>,
    events: mpsc::Receiver<NativeFetchStreamEvent>,
    read_pending: bool,
}

enum NativeEventSourceCommand {
    Close,
}

enum NativeEventSourceEvent {
    Open {
        origin: String,
        cookie_changes: Vec<NativeCookieChange>,
    },
    Message {
        event: String,
        data: String,
        last_event_id: String,
        origin: String,
    },
    Error {
        message: String,
    },
    Close,
}

struct NativeEventSourceConnection {
    commands: mpsc::Sender<NativeEventSourceCommand>,
    events: mpsc::Receiver<NativeEventSourceEvent>,
}

#[derive(Default)]
struct NativeEventSourceParser {
    buffer: Vec<u8>,
    data: String,
    event: String,
    last_event_id: String,
    retry: Duration,
    skip_lf_after_cr: bool,
}

struct NativeEventSourceMessage {
    event: String,
    data: String,
    last_event_id: String,
}

fn parse_event_source_chunk(
    parser: &mut NativeEventSourceParser,
    chunk: &[u8],
) -> Result<Vec<NativeEventSourceMessage>, NativeEngineError> {
    let next_len = parser.buffer.len().saturating_add(chunk.len());
    if next_len > MAX_NATIVE_EVENTSOURCE_MESSAGE_BYTES + MAX_NATIVE_EVENTSOURCE_FIELD_BYTES {
        return Err(NativeEngineError::limit(
            "native EventSource line buffer",
            MAX_NATIVE_EVENTSOURCE_MESSAGE_BYTES + MAX_NATIVE_EVENTSOURCE_FIELD_BYTES,
            next_len,
        ));
    }
    parser.buffer.extend_from_slice(chunk);
    let mut messages = Vec::new();
    loop {
        if parser.skip_lf_after_cr {
            match parser.buffer.first() {
                Some(b'\n') => {
                    parser.buffer.remove(0);
                    parser.skip_lf_after_cr = false;
                }
                Some(_) => parser.skip_lf_after_cr = false,
                None => break,
            }
        }
        let Some(line_end) = parser
            .buffer
            .iter()
            .position(|byte| matches!(*byte, b'\r' | b'\n'))
        else {
            break;
        };
        let is_cr = parser.buffer[line_end] == b'\r';
        let terminator_len = if is_cr && parser.buffer.get(line_end + 1) == Some(&b'\n') {
            2
        } else {
            if is_cr {
                parser.skip_lf_after_cr = true;
            }
            1
        };
        let line = parser.buffer.drain(..line_end).collect::<Vec<_>>();
        parser.buffer.drain(..terminator_len);
        if line.len() > MAX_NATIVE_EVENTSOURCE_MESSAGE_BYTES {
            return Err(NativeEngineError::limit(
                "native EventSource field",
                MAX_NATIVE_EVENTSOURCE_MESSAGE_BYTES,
                line.len(),
            ));
        }
        let line = String::from_utf8(line).map_err(|_| NativeEngineError::Network {
            operation: "EventSource stream".into(),
            reason: "EventSource stream was not valid UTF-8".into(),
        })?;
        if line.is_empty() {
            if !parser.data.is_empty() {
                let data = parser
                    .data
                    .strip_suffix('\n')
                    .unwrap_or(&parser.data)
                    .to_owned();
                messages.push(NativeEventSourceMessage {
                    event: if parser.event.is_empty() {
                        "message".into()
                    } else {
                        parser.event.clone()
                    },
                    data,
                    last_event_id: parser.last_event_id.clone(),
                });
            }
            parser.data.clear();
            parser.event.clear();
            continue;
        }
        if line.starts_with(':') {
            continue;
        }
        let (field, value) = line
            .split_once(':')
            .map_or((line.as_str(), ""), |(field, value)| {
                (field, value.strip_prefix(' ').unwrap_or(value))
            });
        match field {
            "data" => {
                let next_data = parser
                    .data
                    .len()
                    .saturating_add(value.len())
                    .saturating_add(1);
                if next_data > MAX_NATIVE_EVENTSOURCE_MESSAGE_BYTES {
                    return Err(NativeEngineError::limit(
                        "native EventSource message",
                        MAX_NATIVE_EVENTSOURCE_MESSAGE_BYTES,
                        next_data,
                    ));
                }
                parser.data.push_str(value);
                parser.data.push('\n');
            }
            "event" => {
                if value.len() > MAX_NATIVE_EVENTSOURCE_FIELD_BYTES {
                    return Err(NativeEngineError::limit(
                        "native EventSource event name",
                        MAX_NATIVE_EVENTSOURCE_FIELD_BYTES,
                        value.len(),
                    ));
                }
                parser.event = value.to_owned();
            }
            "id" if !value.contains('\0') => {
                if value.len() > MAX_NATIVE_EVENTSOURCE_FIELD_BYTES {
                    return Err(NativeEngineError::limit(
                        "native EventSource last event ID",
                        MAX_NATIVE_EVENTSOURCE_FIELD_BYTES,
                        value.len(),
                    ));
                }
                parser.last_event_id = value.to_owned();
            }
            "retry" if !value.is_empty() && value.bytes().all(|byte| byte.is_ascii_digit()) => {
                if let Ok(milliseconds) = value.parse::<u64>() {
                    parser.retry = Duration::from_millis(
                        milliseconds.min(NATIVE_EVENTSOURCE_MAX_RETRY.as_millis() as u64),
                    );
                }
            }
            _ => {}
        }
    }
    Ok(messages)
}

fn bounded_websocket_text(value: impl AsRef<str>, limit: usize) -> String {
    let value = value.as_ref();
    let mut end = value.len().min(limit);
    while end > 0 && !value.is_char_boundary(end) {
        end -= 1;
    }
    value[..end].to_owned()
}

fn validate_websocket_protocols(protocols: &[String]) -> Result<(), NativeEngineError> {
    if protocols.len() > MAX_NATIVE_WEBSOCKET_PROTOCOLS {
        return Err(NativeEngineError::limit(
            "native WebSocket protocols",
            MAX_NATIVE_WEBSOCKET_PROTOCOLS,
            protocols.len(),
        ));
    }
    let mut seen = BTreeSet::new();
    for protocol in protocols {
        if protocol.is_empty()
            || protocol.len() > MAX_NATIVE_WEBSOCKET_PROTOCOL_BYTES
            || !protocol.bytes().all(|byte| {
                byte.is_ascii_alphanumeric()
                    || matches!(
                        byte,
                        b'!' | b'#'
                            | b'$'
                            | b'%'
                            | b'&'
                            | b'\''
                            | b'*'
                            | b'+'
                            | b'-'
                            | b'.'
                            | b'^'
                            | b'_'
                            | b'`'
                            | b'|'
                            | b'~'
                    )
            })
            || !seen.insert(protocol)
        {
            return Err(NativeEngineError::invalid(
                "native WebSocket protocols",
                "must contain unique bounded WebSocket tokens",
            ));
        }
    }
    Ok(())
}

fn native_websocket_request(
    target: NativeWebSocketTarget,
    origin: &NativeOrigin,
    protocols: &[String],
) -> Result<tokio_tungstenite::tungstenite::http::Request<()>, NativeEngineError> {
    validate_websocket_protocols(protocols)?;
    let mut request =
        target
            .url
            .as_str()
            .into_client_request()
            .map_err(|error| NativeEngineError::Network {
                operation: "WebSocket handshake".into(),
                reason: format!("could not build request: {error}"),
            })?;
    let headers = request.headers_mut();
    headers.insert(
        tokio_tungstenite::tungstenite::http::header::ORIGIN,
        tokio_tungstenite::tungstenite::http::HeaderValue::from_str(&origin.serialized()).map_err(
            |_| NativeEngineError::Network {
                operation: "WebSocket handshake".into(),
                reason: "document origin is not a valid request header".into(),
            },
        )?,
    );
    if let Some(cookie) = target.cookie {
        headers.insert(
            tokio_tungstenite::tungstenite::http::header::COOKIE,
            tokio_tungstenite::tungstenite::http::HeaderValue::from_str(&cookie).map_err(|_| {
                NativeEngineError::Network {
                    operation: "WebSocket handshake".into(),
                    reason: "native cookie state is not a valid request header".into(),
                }
            })?,
        );
    }
    if !protocols.is_empty() {
        let value = protocols.join(", ");
        headers.insert(
            tokio_tungstenite::tungstenite::http::header::SEC_WEBSOCKET_PROTOCOL,
            tokio_tungstenite::tungstenite::http::HeaderValue::from_str(&value).map_err(|_| {
                NativeEngineError::Network {
                    operation: "WebSocket handshake".into(),
                    reason: "WebSocket protocols are not valid request headers".into(),
                }
            })?,
        );
    }
    Ok(request)
}

async fn queue_websocket_event(
    events: &mpsc::Sender<NativeWebSocketEvent>,
    event: NativeWebSocketEvent,
) -> bool {
    events.send(event).await.is_ok()
}

async fn run_native_websocket(
    request: tokio_tungstenite::tungstenite::http::Request<()>,
    origin: String,
    mut commands: mpsc::Receiver<NativeWebSocketCommand>,
    events: mpsc::Sender<NativeWebSocketEvent>,
) {
    let connection = timeout(NATIVE_WEBSOCKET_CONNECT_TIMEOUT, connect_async(request)).await;
    let (socket, response) = match connection {
        Ok(Ok(connection)) => connection,
        Ok(Err(error)) => {
            let message = bounded_websocket_text(error.to_string(), MAX_NATIVE_SCRIPT_BYTES);
            if !queue_websocket_event(&events, NativeWebSocketEvent::Error { message }).await {
                return;
            }
            let _ = queue_websocket_event(
                &events,
                NativeWebSocketEvent::Close {
                    code: 1006,
                    reason: String::new(),
                    was_clean: false,
                },
            )
            .await;
            return;
        }
        Err(_) => {
            if !queue_websocket_event(
                &events,
                NativeWebSocketEvent::Error {
                    message: "native WebSocket handshake timed out".into(),
                },
            )
            .await
            {
                return;
            }
            let _ = queue_websocket_event(
                &events,
                NativeWebSocketEvent::Close {
                    code: 1006,
                    reason: String::new(),
                    was_clean: false,
                },
            )
            .await;
            return;
        }
    };
    let protocol = response
        .headers()
        .get(tokio_tungstenite::tungstenite::http::header::SEC_WEBSOCKET_PROTOCOL)
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default()
        .to_owned();
    if !queue_websocket_event(&events, NativeWebSocketEvent::Open { protocol }).await {
        return;
    }
    let (mut sink, mut stream) = socket.split();
    loop {
        tokio::select! {
            command = commands.recv() => {
                match command {
                    Some(NativeWebSocketCommand::Send(message)) => {
                        if let Err(error) = sink.send(message).await {
                            let message = bounded_websocket_text(error.to_string(), MAX_NATIVE_SCRIPT_BYTES);
                            if !queue_websocket_event(&events, NativeWebSocketEvent::Error { message }).await {
                                return;
                            }
                            let _ = queue_websocket_event(&events, NativeWebSocketEvent::Close {
                                code: 1006,
                                reason: String::new(),
                                was_clean: false,
                            }).await;
                            return;
                        }
                    }
                    Some(NativeWebSocketCommand::Close { code, reason }) => {
                        let close = Message::Close(Some(tokio_tungstenite::tungstenite::protocol::CloseFrame {
                            code: tokio_tungstenite::tungstenite::protocol::frame::coding::CloseCode::from(code),
                            reason: reason.clone().into(),
                        }));
                        let clean = sink.send(close).await.is_ok();
                        let _ = sink.close().await;
                        let _ = queue_websocket_event(&events, NativeWebSocketEvent::Close {
                            code: if clean { code } else { 1006 },
                            reason: if clean { reason } else { String::new() },
                            was_clean: clean,
                        }).await;
                        return;
                    }
                    None => {
                        let _ = sink.close().await;
                        let _ = queue_websocket_event(&events, NativeWebSocketEvent::Close {
                            code: 1006,
                            reason: String::new(),
                            was_clean: false,
                        }).await;
                        return;
                    }
                }
            }
            incoming = stream.next() => {
                match incoming {
                    Some(Ok(Message::Text(data))) => {
                        let data = data.to_string();
                        if data.len() > MAX_NATIVE_WEBSOCKET_MESSAGE_BYTES {
                            let _ = queue_websocket_event(&events, NativeWebSocketEvent::Error {
                                message: "native WebSocket message exceeds its limit".into(),
                            }).await;
                            let _ = queue_websocket_event(&events, NativeWebSocketEvent::Close {
                                code: 1009,
                                reason: String::new(),
                                was_clean: false,
                            }).await;
                            return;
                        }
                        if !queue_websocket_event(&events, NativeWebSocketEvent::MessageText { data, origin: origin.clone() }).await {
                            return;
                        }
                    }
                    Some(Ok(Message::Binary(data))) => {
                        let data = data.to_vec();
                        if data.len() > MAX_NATIVE_WEBSOCKET_MESSAGE_BYTES {
                            let _ = queue_websocket_event(&events, NativeWebSocketEvent::Error {
                                message: "native WebSocket message exceeds its limit".into(),
                            }).await;
                            let _ = queue_websocket_event(&events, NativeWebSocketEvent::Close {
                                code: 1009,
                                reason: String::new(),
                                was_clean: false,
                            }).await;
                            return;
                        }
                        if !queue_websocket_event(&events, NativeWebSocketEvent::MessageBinary { data, origin: origin.clone() }).await {
                            return;
                        }
                    }
                    Some(Ok(Message::Close(frame))) => {
                        let (code, reason) = frame
                            .map(|frame| (u16::from(frame.code), bounded_websocket_text(frame.reason, MAX_NATIVE_WEBSOCKET_CLOSE_REASON_BYTES)))
                            .unwrap_or((1005, String::new()));
                        let _ = queue_websocket_event(&events, NativeWebSocketEvent::Close {
                            code,
                            reason,
                            was_clean: true,
                        }).await;
                        return;
                    }
                    Some(Ok(Message::Ping(data))) => {
                        if let Err(error) = sink.send(Message::Pong(data)).await {
                            let message = bounded_websocket_text(error.to_string(), MAX_NATIVE_SCRIPT_BYTES);
                            if !queue_websocket_event(&events, NativeWebSocketEvent::Error { message }).await {
                                return;
                            }
                            let _ = queue_websocket_event(&events, NativeWebSocketEvent::Close {
                                code: 1006,
                                reason: String::new(),
                                was_clean: false,
                            }).await;
                            return;
                        }
                    }
                    Some(Ok(Message::Pong(_))) => {}
                    Some(Ok(_)) => {}
                    Some(Err(error)) => {
                        let message = bounded_websocket_text(error.to_string(), MAX_NATIVE_SCRIPT_BYTES);
                        if !queue_websocket_event(&events, NativeWebSocketEvent::Error { message }).await {
                            return;
                        }
                        let _ = queue_websocket_event(&events, NativeWebSocketEvent::Close {
                            code: 1006,
                            reason: String::new(),
                            was_clean: false,
                        }).await;
                        return;
                    }
                    None => {
                        let _ = queue_websocket_event(&events, NativeWebSocketEvent::Close {
                            code: 1006,
                            reason: String::new(),
                            was_clean: false,
                        }).await;
                        return;
                    }
                }
            }
        }
    }
}

fn spawn_native_websocket(
    target: NativeWebSocketTarget,
    origin: &NativeOrigin,
    protocols: &[String],
) -> Result<NativeWebSocketConnection, NativeEngineError> {
    let request = native_websocket_request(target, origin, protocols)?;
    let (command_sender, command_receiver) = mpsc::channel(MAX_NATIVE_WEBSOCKET_EVENTS);
    let (event_sender, event_receiver) = mpsc::channel(MAX_NATIVE_WEBSOCKET_EVENTS);
    tokio::spawn(run_native_websocket(
        request,
        origin.serialized(),
        command_receiver,
        event_sender,
    ));
    Ok(NativeWebSocketConnection {
        commands: command_sender,
        events: event_receiver,
    })
}

async fn run_native_fetch_stream(
    response: reqwest::Response,
    max_response_bytes: usize,
    mut commands: mpsc::Receiver<NativeFetchStreamCommand>,
    events: mpsc::Sender<NativeFetchStreamEvent>,
) {
    let mut stream = response.bytes_stream();
    let mut total_bytes = 0usize;
    let mut pending_parts = VecDeque::new();
    let mut read_requested = false;
    loop {
        if !read_requested {
            match commands.recv().await {
                Some(NativeFetchStreamCommand::Read) => read_requested = true,
                Some(NativeFetchStreamCommand::Cancel) | None => return,
            }
        }
        if let Some(part) = pending_parts.pop_front() {
            read_requested = false;
            if events
                .send(NativeFetchStreamEvent::Chunk { data: part })
                .await
                .is_err()
            {
                return;
            }
            continue;
        }
        tokio::select! {
            command = commands.recv() => {
                match command {
                    Some(NativeFetchStreamCommand::Read) => {}
                    Some(NativeFetchStreamCommand::Cancel) | None => return,
                }
            }
            chunk = stream.next() => {
                match chunk {
                    Some(Ok(chunk)) => {
                        let next_total = total_bytes.saturating_add(chunk.len());
                        if next_total > max_response_bytes {
                            let _ = events.send(NativeFetchStreamEvent::Error {
                                message: "native fetch response stream exceeds its limit".into(),
                            }).await;
                            return;
                        }
                        total_bytes = next_total;
                        pending_parts.extend(
                            chunk
                                .chunks(MAX_NATIVE_FETCH_STREAM_CHUNK_BYTES)
                                .map(|part| part.to_vec()),
                        );
                    }
                    Some(Err(error)) => {
                        if events
                            .send(NativeFetchStreamEvent::Error {
                                message: bounded_websocket_text(
                                    error.to_string(),
                                    MAX_NATIVE_SCRIPT_BYTES,
                                ),
                            })
                            .await
                            .is_err()
                        {
                            return;
                        }
                        while let Some(command) = commands.recv().await {
                            if matches!(command, NativeFetchStreamCommand::Cancel) {
                                return;
                            }
                        }
                        return;
                    }
                    None => {
                        if events.send(NativeFetchStreamEvent::End).await.is_err() {
                            return;
                        }
                        while let Some(command) = commands.recv().await {
                            if matches!(command, NativeFetchStreamCommand::Cancel) {
                                return;
                            }
                        }
                        return;
                    }
                }
            }
        }
    }
}

fn spawn_native_fetch_stream(
    response: reqwest::Response,
    max_response_bytes: usize,
) -> NativeFetchStreamConnection {
    let (command_sender, command_receiver) = mpsc::channel(MAX_NATIVE_FETCH_STREAM_CONNECTIONS);
    let (event_sender, event_receiver) = mpsc::channel(MAX_NATIVE_FETCH_STREAM_CONNECTIONS);
    tokio::spawn(run_native_fetch_stream(
        response,
        max_response_bytes,
        command_receiver,
        event_sender,
    ));
    NativeFetchStreamConnection {
        commands: command_sender,
        events: event_receiver,
        read_pending: false,
    }
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

async fn run_native_event_source(
    mut loader: NativeResourceLoader,
    document_url: String,
    href: String,
    with_credentials: bool,
    mut commands: mpsc::Receiver<NativeEventSourceCommand>,
    events: mpsc::Sender<NativeEventSourceEvent>,
) {
    let mut last_event_id = String::new();
    let mut retry = NATIVE_EVENTSOURCE_INITIAL_RETRY;
    let mut reconnects = 0usize;
    loop {
        let response = timeout(
            NATIVE_WEBSOCKET_CONNECT_TIMEOUT,
            loader.open_event_source_async(&document_url, &href, with_credentials, &last_event_id),
        )
        .await;
        let (url, response) = match response {
            Ok(Ok(response)) => response,
            Ok(Err(error)) => {
                if !queue_event_source_event(
                    &events,
                    NativeEventSourceEvent::Error {
                        message: bounded_websocket_text(error.to_string(), MAX_NATIVE_SCRIPT_BYTES),
                    },
                )
                .await
                {
                    return;
                }
                reconnects = reconnects.saturating_add(1);
                if reconnects > MAX_NATIVE_EVENTSOURCE_RECONNECTS
                    || !wait_event_source_retry(&mut commands, retry).await
                {
                    let _ = queue_event_source_event(&events, NativeEventSourceEvent::Close).await;
                    return;
                }
                continue;
            }
            Err(_) => {
                if !queue_event_source_event(
                    &events,
                    NativeEventSourceEvent::Error {
                        message: "native EventSource connection timed out".into(),
                    },
                )
                .await
                {
                    return;
                }
                reconnects = reconnects.saturating_add(1);
                if reconnects > MAX_NATIVE_EVENTSOURCE_RECONNECTS
                    || !wait_event_source_retry(&mut commands, retry).await
                {
                    let _ = queue_event_source_event(&events, NativeEventSourceEvent::Close).await;
                    return;
                }
                continue;
            }
        };
        let cookie_changes = loader.take_cookie_changes();
        reconnects = 0;
        if !queue_event_source_event(
            &events,
            NativeEventSourceEvent::Open {
                origin: url.origin().ascii_serialization(),
                cookie_changes,
            },
        )
        .await
        {
            return;
        }
        let mut parser = NativeEventSourceParser {
            last_event_id: last_event_id.clone(),
            retry,
            ..NativeEventSourceParser::default()
        };
        let mut stream = response.bytes_stream();
        loop {
            tokio::select! {
                command = commands.recv() => {
                    if !matches!(command, Some(NativeEventSourceCommand::Close)) {
                        let _ = queue_event_source_event(&events, NativeEventSourceEvent::Close).await;
                    }
                    return;
                }
                chunk = stream.next() => {
                    match chunk {
                        Some(Ok(chunk)) => {
                            let messages = match parse_event_source_chunk(&mut parser, &chunk) {
                                Ok(messages) => messages,
                                Err(error) => {
                                    let _ = queue_event_source_event(&events, NativeEventSourceEvent::Error {
                                        message: bounded_websocket_text(error.to_string(), MAX_NATIVE_SCRIPT_BYTES),
                                    }).await;
                                    break;
                                }
                            };
                            last_event_id = parser.last_event_id.clone();
                            for message in messages {
                                last_event_id = message.last_event_id.clone();
                                if !queue_event_source_event(&events, NativeEventSourceEvent::Message {
                                    event: message.event,
                                    data: message.data,
                                    last_event_id: message.last_event_id,
                                    origin: url.origin().ascii_serialization(),
                                }).await {
                                    return;
                                }
                            }
                        }
                        Some(Err(error)) => {
                            let _ = queue_event_source_event(&events, NativeEventSourceEvent::Error {
                                message: bounded_websocket_text(error.to_string(), MAX_NATIVE_SCRIPT_BYTES),
                            }).await;
                            break;
                        }
                        None => break,
                    }
                }
            }
        }
        retry = parser.retry.min(NATIVE_EVENTSOURCE_MAX_RETRY);
        reconnects = reconnects.saturating_add(1);
        if reconnects > MAX_NATIVE_EVENTSOURCE_RECONNECTS
            || !wait_event_source_retry(&mut commands, retry).await
        {
            let _ = queue_event_source_event(&events, NativeEventSourceEvent::Close).await;
            return;
        }
    }
}

async fn queue_event_source_event(
    events: &mpsc::Sender<NativeEventSourceEvent>,
    event: NativeEventSourceEvent,
) -> bool {
    events.send(event).await.is_ok()
}

fn spawn_native_event_source(
    loader: &NativeResourceLoader,
    document_url: &str,
    href: &str,
    with_credentials: bool,
) -> NativeEventSourceConnection {
    let (command_sender, command_receiver) = mpsc::channel(MAX_NATIVE_EVENTSOURCE_CONNECTIONS);
    let (event_sender, event_receiver) = mpsc::channel(MAX_NATIVE_EVENTSOURCE_CONNECTIONS);
    tokio::spawn(run_native_event_source(
        loader.clone(),
        document_url.to_owned(),
        href.to_owned(),
        with_credentials,
        command_receiver,
        event_sender,
    ));
    NativeEventSourceConnection {
        commands: command_sender,
        events: event_receiver,
    }
}

#[derive(Debug, Clone)]
pub(crate) struct NativeContentNavigation {
    pub(crate) node_index: u32,
    pub(crate) href: String,
    pub(crate) submitter_node_index: Option<u32>,
    pub(crate) location: bool,
    pub(crate) replace_history: bool,
}

pub(crate) struct NativeContentMutation {
    pub(crate) document: NativeDocumentWire,
    pub(crate) events: Vec<NativeContentEvent>,
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
    pub(crate) window_closes: Vec<NativeWindowCloseRequest>,
    pub(crate) window_navigations: Vec<NativeWindowNavigationRequest>,
    pub(crate) window_name: String,
}

/// Process-backed lifecycle and bounded document-transfer channel for one
/// native content runtime.
pub(crate) struct NativeContentProcess {
    child: Child,
    stdin: ChildStdin,
    stdout: ChildStdout,
    next_request_id: u64,
    healthy: bool,
    failure_kind: Option<NativeWorkerFailureKind>,
    scroll_offset: NativePoint,
    nested_scroll_offsets: BTreeMap<u32, NativePoint>,
    #[cfg(windows)]
    sandbox: NativeContentSandbox,
}

impl NativeContentProcess {
    pub(crate) async fn spawn(storage_path: Option<&Path>) -> Result<Self, NativeEngineError> {
        let path = worker_binary_path()?;
        if let Some(storage_path) = storage_path
            && !storage_path.exists()
        {
            save_web_storage_profile(
                Some(storage_path),
                &NativeWebStorageState::default(),
                &[],
                &[],
                &[],
                &NativeIndexedDbState::default(),
                &[],
            )?;
        }
        let (mut command, mut sandbox) = prepare_worker_command(&path, storage_path)?;
        let mut child = command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .kill_on_drop(true)
            .spawn()
            .map_err(|_| {
                NativeEngineError::worker_failure(
                    "spawn content process",
                    NativeWorkerFailureKind::Spawn,
                    "native content worker could not be started",
                )
            })?;
        if let Err(error) = sandbox.attach(&child) {
            let _ = child.start_kill();
            return Err(error);
        }
        let stdin = child.stdin.take().ok_or_else(|| {
            NativeEngineError::worker_failure(
                "spawn content process",
                NativeWorkerFailureKind::Transport,
                "native content worker stdin is unavailable",
            )
        })?;
        let stdout = child.stdout.take().ok_or_else(|| {
            NativeEngineError::worker_failure(
                "spawn content process",
                NativeWorkerFailureKind::Transport,
                "native content worker stdout is unavailable",
            )
        })?;
        let mut process = Self {
            child,
            stdin,
            stdout,
            next_request_id: 1,
            healthy: true,
            failure_kind: None,
            scroll_offset: NativePoint { x: 0, y: 0 },
            nested_scroll_offsets: BTreeMap::new(),
            #[cfg(windows)]
            sandbox,
        };
        let id = process.next_id();
        let response = process
            .exchange(json!({
                "kind": "ping",
                "id": id,
                "protocol": CONTENT_WORKER_PROTOCOL_VERSION,
            }))
            .await?;
        require_response_kind(&response, "pong", id, "content process ping")?;
        if response.get("protocol").and_then(Value::as_u64) != Some(CONTENT_WORKER_PROTOCOL_VERSION)
        {
            return Err(NativeEngineError::worker_failure(
                "content process ping",
                NativeWorkerFailureKind::Protocol,
                "content process protocol version is unsupported",
            ));
        }
        Ok(process)
    }

    pub(crate) async fn start(
        &mut self,
        storage_path: Option<&Path>,
        context_id: &str,
        frame_id: &str,
        window_name: &str,
        opener_context_id: Option<&str>,
        opener_window_name: &str,
        opener_url: &str,
        frame_context: Option<&NativeFrameScriptContext>,
    ) -> Result<(), NativeEngineError> {
        let id = self.next_id();
        let response = self
            .exchange(json!({
                "kind": "start",
                "id": id,
                "protocol": CONTENT_WORKER_PROTOCOL_VERSION,
                "storage_path": storage_path.map(|path| path.to_string_lossy().into_owned()),
                "context_id": context_id,
                "frame_id": frame_id,
                "window_name": window_name,
                "opener_context_id": opener_context_id,
                "opener_window_name": opener_window_name,
                "opener_url": opener_url,
                "frame_context": frame_context,
            }))
            .await?;
        let result = require_response_kind(&response, "started", id, "content process start");
        if result.is_ok() {
            self.scroll_offset = NativePoint { x: 0, y: 0 };
            self.nested_scroll_offsets.clear();
        }
        if result.is_err() {
            self.mark_failed(NativeWorkerFailureKind::Protocol);
            let _ = self.child.start_kill();
        }
        result
    }

    pub(crate) async fn commit(&mut self) -> Result<(), NativeEngineError> {
        let id = self.next_id();
        let response = self
            .exchange(json!({
                "kind": "commit",
                "id": id,
                "protocol": CONTENT_WORKER_PROTOCOL_VERSION,
            }))
            .await?;
        let result = require_response_kind(&response, "committed", id, "content process commit");
        if result.is_err() {
            self.mark_failed(NativeWorkerFailureKind::Protocol);
            let _ = self.child.start_kill();
        }
        result
    }

    pub(crate) async fn sync_frame_script_context(
        &mut self,
        context: Option<&NativeFrameScriptContext>,
    ) -> Result<(), NativeEngineError> {
        let id = self.next_id();
        let response = match timeout(
            CONTENT_PROCESS_SCRIPT_TIMEOUT,
            self.exchange(json!({
                "kind": "frame_script_context_sync",
                "id": id,
                "protocol": CONTENT_WORKER_PROTOCOL_VERSION,
                "context": context,
            })),
        )
        .await
        {
            Ok(response) => response?,
            Err(_) => {
                self.mark_failed(NativeWorkerFailureKind::Timeout);
                let _ = self.child.start_kill();
                return Err(NativeEngineError::worker_failure(
                    "content process frame script context synchronization",
                    NativeWorkerFailureKind::Timeout,
                    "content process frame script context synchronization exceeded its deadline",
                ));
            }
        };
        let result = require_response_kind(
            &response,
            "frame_script_context_synced",
            id,
            "content process frame script context synchronization",
        );
        if result.is_err() {
            self.mark_failed(NativeWorkerFailureKind::InvalidTransfer);
            let _ = self.child.start_kill();
        }
        result
    }

    pub(crate) async fn load(
        &mut self,
        navigation: &NativeNavigationRequest,
        limits: &NativeEngineLimits,
        viewport: Viewport,
        referrer: Option<&str>,
    ) -> Result<NativeContentLoad, NativeEngineError> {
        let id = self.next_id();
        let response = match timeout(
            CONTENT_PROCESS_LOAD_TIMEOUT,
            self.exchange(json!({
                "kind": "load",
                "id": id,
                "protocol": CONTENT_WORKER_PROTOCOL_VERSION,
                "url": navigation.url,
                "method": navigation.method.as_str(),
                "body": navigation.body.as_ref().and_then(|body| match body {
                    NativeRequestBody::Text(body) => Some(body),
                    NativeRequestBody::Bytes(_) => None,
                }),
                "body_base64": navigation.body.as_ref().and_then(|body| match body {
                    NativeRequestBody::Text(_) => None,
                    NativeRequestBody::Bytes(body) => Some(
                        base64::engine::general_purpose::STANDARD.encode(body),
                    ),
                }),
                "content_type": navigation.body_content_type,
                "referrer": referrer,
                "max_document_bytes": limits.max_document_bytes,
                "max_nodes": limits.max_nodes,
                "max_dom_depth": limits.max_dom_depth,
                "max_text_bytes": limits.max_text_bytes,
                "viewport_width": viewport.width,
                "viewport_height": viewport.height,
                "viewport_device_scale_factor_milli": viewport.device_scale_factor_milli,
            })),
        )
        .await
        {
            Ok(response) => response?,
            Err(_) => {
                self.mark_failed(NativeWorkerFailureKind::Timeout);
                let _ = self.child.start_kill();
                return Err(NativeEngineError::worker_failure(
                    "content process load",
                    NativeWorkerFailureKind::Timeout,
                    "content process load exceeded its deadline",
                ));
            }
        };
        if response.get("kind").and_then(Value::as_str) == Some("error") {
            return Err(NativeEngineError::Worker {
                operation: "content process load".into(),
                reason: response
                    .get("reason")
                    .and_then(Value::as_str)
                    .unwrap_or("content process rejected the document load")
                    .into(),
            });
        }
        let result = decode_loaded_response(&response, id);
        if result.is_err() {
            self.mark_failed(NativeWorkerFailureKind::InvalidTransfer);
            let _ = self.child.start_kill();
        }
        result
    }

    pub(crate) async fn mutate_click_with_event_preflight(
        &mut self,
        node_index: u32,
    ) -> Result<NativeContentMutation, NativeEngineError> {
        let id = self.next_id();
        self.mutate_with_request_kind(
            id,
            "mutate_click_preflight",
            json!({"node_index": node_index}),
        )
        .await
    }

    pub(crate) async fn mutate_type_with_event_bridge(
        &mut self,
        node_index: u32,
        text: String,
    ) -> Result<NativeContentMutation, NativeEngineError> {
        let id = self.next_id();
        self.mutate_with_request_kind(
            id,
            "mutate_type_events",
            json!({"node_index": node_index, "text": text}),
        )
        .await
    }

    pub(crate) async fn mutate_form_action_with_event_bridge(
        &mut self,
        action: Value,
    ) -> Result<NativeContentMutation, NativeEngineError> {
        let id = self.next_id();
        self.mutate_with_request_kind(id, "mutate_form_events", action)
            .await
    }

    pub(crate) async fn mutate_key_with_event_bridge(
        &mut self,
        node_index: u32,
        key: String,
    ) -> Result<NativeContentMutation, NativeEngineError> {
        let id = self.next_id();
        self.mutate_with_request_kind(
            id,
            "mutate_key_events",
            json!({"node_index": node_index, "key": key}),
        )
        .await
    }

    pub(crate) async fn mutate_key_event_with_event_bridge(
        &mut self,
        node_index: u32,
        key: String,
        kind: NativeEventKind,
        modifiers: i64,
    ) -> Result<NativeContentMutation, NativeEngineError> {
        let id = self.next_id();
        self.mutate_with_request_kind(
            id,
            "mutate_key_event",
            json!({
                "node_index": node_index,
                "key": key,
                "kind": event_kind_text(kind),
                "modifiers": modifiers,
            }),
        )
        .await
    }

    pub(crate) async fn mutate_key_shortcut_with_event_bridge(
        &mut self,
        node_index: u32,
        key: String,
        modifiers: i64,
        apply_default: bool,
    ) -> Result<NativeContentMutation, NativeEngineError> {
        let id = self.next_id();
        self.mutate_with_request_kind(
            id,
            "mutate_key_shortcut",
            json!({
                "node_index": node_index,
                "key": key,
                "kind": "shortcut",
                "modifiers": modifiers,
                "apply_default": apply_default,
            }),
        )
        .await
    }

    pub(crate) async fn dispatch_lifecycle_events(
        &mut self,
        events: &[NativeEventKind],
    ) -> Result<NativeContentMutation, NativeEngineError> {
        let event_names = events
            .iter()
            .map(|kind| event_kind_text(*kind))
            .collect::<Vec<_>>();
        let id = self.next_id();
        self.mutate_with_request_kind(
            id,
            "mutate_lifecycle_events",
            json!({"events": event_names}),
        )
        .await
    }

    pub(crate) async fn dispatch_before_unload(
        &mut self,
    ) -> Result<NativeContentMutation, NativeEngineError> {
        let id = self.next_id();
        self.mutate_with_request_kind(id, "mutate_before_unload", Value::Null)
            .await
    }

    pub(crate) async fn dispatch_hash_change(
        &mut self,
        old_url: &str,
        new_url: &str,
    ) -> Result<NativeContentMutation, NativeEngineError> {
        let id = self.next_id();
        self.mutate_with_request_kind(
            id,
            "mutate_hash_change",
            json!({"old_url": old_url, "new_url": new_url}),
        )
        .await
    }

    pub(crate) async fn fetch(
        &mut self,
        document_url: &str,
        href: &str,
        credentials: bool,
    ) -> Result<NativeFetchResponse, NativeEngineError> {
        let id = self.next_id();
        let response = match timeout(
            CONTENT_PROCESS_LOAD_TIMEOUT,
            self.exchange(json!({
                "kind": "fetch",
                "id": id,
                "protocol": CONTENT_WORKER_PROTOCOL_VERSION,
                "document_url": document_url,
                "href": href,
                "credentials": credentials,
            })),
        )
        .await
        {
            Ok(response) => response?,
            Err(_) => {
                self.mark_failed(NativeWorkerFailureKind::Timeout);
                let _ = self.child.start_kill();
                return Err(NativeEngineError::worker_failure(
                    "content process fetch",
                    NativeWorkerFailureKind::Timeout,
                    "content process fetch exceeded its deadline",
                ));
            }
        };
        if response.get("kind").and_then(Value::as_str) == Some("error") {
            return Err(NativeEngineError::Worker {
                operation: "content process fetch".into(),
                reason: response
                    .get("reason")
                    .and_then(Value::as_str)
                    .unwrap_or("content process rejected the fetch")
                    .into(),
            });
        }
        let result = decode_fetch_response(&response, id);
        if result.is_err() {
            self.mark_failed(NativeWorkerFailureKind::InvalidTransfer);
            let _ = self.child.start_kill();
        }
        result
    }

    pub(crate) async fn evaluate(
        &mut self,
        source: &str,
    ) -> Result<NativeContentScriptResult, NativeEngineError> {
        let id = self.next_id();
        let response = match timeout(
            CONTENT_PROCESS_SCRIPT_TIMEOUT,
            self.exchange(json!({
                "kind": "script",
                "id": id,
                "protocol": CONTENT_WORKER_PROTOCOL_VERSION,
                "source": source,
            })),
        )
        .await
        {
            Ok(response) => response?,
            Err(_) => {
                self.mark_failed(NativeWorkerFailureKind::Timeout);
                let _ = self.child.start_kill();
                return Err(NativeEngineError::worker_failure(
                    "content process script",
                    NativeWorkerFailureKind::Timeout,
                    "content process script exceeded its deadline",
                ));
            }
        };
        if response.get("kind").and_then(Value::as_str) == Some("error") {
            return Err(NativeEngineError::Worker {
                operation: "content process script".into(),
                reason: response
                    .get("reason")
                    .and_then(Value::as_str)
                    .unwrap_or("content process rejected the script")
                    .into(),
            });
        }
        decode_script_response(&response, id)
    }

    pub(crate) async fn sync_scroll_offset(
        &mut self,
        scroll_offset: NativePoint,
    ) -> Result<(), NativeEngineError> {
        if self.scroll_offset == scroll_offset {
            return Ok(());
        }
        let id = self.next_id();
        let response = match timeout(
            CONTENT_PROCESS_SCRIPT_TIMEOUT,
            self.exchange(json!({
                "kind": "scroll_sync",
                "id": id,
                "protocol": CONTENT_WORKER_PROTOCOL_VERSION,
                "x": scroll_offset.x,
                "y": scroll_offset.y,
            })),
        )
        .await
        {
            Ok(response) => response?,
            Err(_) => {
                self.mark_failed(NativeWorkerFailureKind::Timeout);
                let _ = self.child.start_kill();
                return Err(NativeEngineError::worker_failure(
                    "content process scroll synchronization",
                    NativeWorkerFailureKind::Timeout,
                    "content process scroll synchronization exceeded its deadline",
                ));
            }
        };
        let result = require_response_kind(
            &response,
            "scroll_synced",
            id,
            "content process scroll synchronization",
        );
        if result.is_err() {
            self.mark_failed(NativeWorkerFailureKind::InvalidTransfer);
            let _ = self.child.start_kill();
        } else {
            self.scroll_offset = scroll_offset;
        }
        result
    }

    pub(crate) async fn sync_nested_scroll_offsets(
        &mut self,
        offsets: &BTreeMap<u32, NativePoint>,
    ) -> Result<(), NativeEngineError> {
        if &self.nested_scroll_offsets == offsets {
            return Ok(());
        }
        let id = self.next_id();
        let nested = offsets
            .iter()
            .map(|(node_index, offset)| {
                json!({
                    "node_index": node_index,
                    "x": offset.x,
                    "y": offset.y,
                })
            })
            .collect::<Vec<_>>();
        let response = match timeout(
            CONTENT_PROCESS_SCRIPT_TIMEOUT,
            self.exchange(json!({
                "kind": "scroll_sync",
                "id": id,
                "protocol": CONTENT_WORKER_PROTOCOL_VERSION,
                "nested": nested,
            })),
        )
        .await
        {
            Ok(response) => response?,
            Err(_) => {
                self.mark_failed(NativeWorkerFailureKind::Timeout);
                let _ = self.child.start_kill();
                return Err(NativeEngineError::worker_failure(
                    "content process nested scroll synchronization",
                    NativeWorkerFailureKind::Timeout,
                    "content process nested scroll synchronization exceeded its deadline",
                ));
            }
        };
        let result = require_response_kind(
            &response,
            "scroll_synced",
            id,
            "content process nested scroll synchronization",
        );
        if result.is_err() {
            self.mark_failed(NativeWorkerFailureKind::InvalidTransfer);
            let _ = self.child.start_kill();
        } else {
            self.nested_scroll_offsets = offsets.clone();
        }
        result
    }

    pub(crate) async fn sync_history(
        &mut self,
        url: &str,
        state: &Value,
        length: usize,
    ) -> Result<(), NativeEngineError> {
        validate_url_text("content process history URL", url)?;
        let encoded = serde_json::to_vec(state).map_err(|_| NativeEngineError::Worker {
            operation: "serialize content process history state".into(),
            reason: "history state could not be serialized".into(),
        })?;
        if encoded.len() > MAX_NATIVE_HISTORY_STATE_BYTES {
            return Err(NativeEngineError::limit(
                "content process history state",
                MAX_NATIVE_HISTORY_STATE_BYTES,
                encoded.len(),
            ));
        }
        let id = self.next_id();
        let response = match timeout(
            CONTENT_PROCESS_SCRIPT_TIMEOUT,
            self.exchange(json!({
                "kind": "history_sync",
                "id": id,
                "protocol": CONTENT_WORKER_PROTOCOL_VERSION,
                "url": url,
                "state": state,
                "length": length.max(1),
            })),
        )
        .await
        {
            Ok(response) => response?,
            Err(_) => {
                self.mark_failed(NativeWorkerFailureKind::Timeout);
                let _ = self.child.start_kill();
                return Err(NativeEngineError::worker_failure(
                    "content process history synchronization",
                    NativeWorkerFailureKind::Timeout,
                    "content process history synchronization exceeded its deadline",
                ));
            }
        };
        let result = require_response_kind(
            &response,
            "history_synced",
            id,
            "content process history synchronization",
        );
        if result.is_err() {
            self.mark_failed(NativeWorkerFailureKind::InvalidTransfer);
            let _ = self.child.start_kill();
        }
        result
    }

    pub(crate) async fn sync_window_proxies(
        &mut self,
        updates: &[NativeWindowProxyUpdate],
    ) -> Result<(), NativeEngineError> {
        if updates.is_empty() {
            return Ok(());
        }
        if updates.len() > MAX_NATIVE_EFFECTS {
            return Err(NativeEngineError::limit(
                "native WindowProxy updates",
                MAX_NATIVE_EFFECTS,
                updates.len(),
            ));
        }
        let id = self.next_id();
        let response = match timeout(
            CONTENT_PROCESS_SCRIPT_TIMEOUT,
            self.exchange(json!({
                "kind": "window_proxy_sync",
                "id": id,
                "protocol": CONTENT_WORKER_PROTOCOL_VERSION,
                "updates": updates,
            })),
        )
        .await
        {
            Ok(response) => response?,
            Err(_) => {
                self.mark_failed(NativeWorkerFailureKind::Timeout);
                let _ = self.child.start_kill();
                return Err(NativeEngineError::worker_failure(
                    "content process WindowProxy synchronization",
                    NativeWorkerFailureKind::Timeout,
                    "content process WindowProxy synchronization exceeded its deadline",
                ));
            }
        };
        let result = require_response_kind(
            &response,
            "window_proxy_synced",
            id,
            "content process WindowProxy synchronization",
        );
        if result.is_err() {
            self.mark_failed(NativeWorkerFailureKind::InvalidTransfer);
            let _ = self.child.start_kill();
        }
        result
    }

    pub(crate) async fn sync_frame_script_bindings(
        &mut self,
        bindings: &[NativeFrameScriptBinding],
    ) -> Result<(), NativeEngineError> {
        if bindings.len() > MAX_CONTENT_FRAME_SOURCES {
            return Err(NativeEngineError::limit(
                "native frame script bindings",
                MAX_CONTENT_FRAME_SOURCES,
                bindings.len(),
            ));
        }
        let id = self.next_id();
        let response = match timeout(
            CONTENT_PROCESS_SCRIPT_TIMEOUT,
            self.exchange(json!({
                "kind": "frame_script_sync",
                "id": id,
                "protocol": CONTENT_WORKER_PROTOCOL_VERSION,
                "bindings": bindings,
            })),
        )
        .await
        {
            Ok(response) => response?,
            Err(_) => {
                self.mark_failed(NativeWorkerFailureKind::Timeout);
                let _ = self.child.start_kill();
                return Err(NativeEngineError::worker_failure(
                    "content process frame script synchronization",
                    NativeWorkerFailureKind::Timeout,
                    "content process frame script synchronization exceeded its deadline",
                ));
            }
        };
        let result = require_response_kind(
            &response,
            "frame_script_synced",
            id,
            "content process frame script synchronization",
        );
        if result.is_err() {
            self.mark_failed(NativeWorkerFailureKind::InvalidTransfer);
            let _ = self.child.start_kill();
        }
        result
    }

    pub(crate) async fn sync_storage_events(
        &mut self,
        events: &[NativeStorageEvent],
    ) -> Result<(), NativeEngineError> {
        if events.is_empty() {
            return Ok(());
        }
        let id = self.next_id();
        let response = match timeout(
            CONTENT_PROCESS_SCRIPT_TIMEOUT,
            self.exchange(json!({
                "kind": "storage_events",
                "id": id,
                "protocol": CONTENT_WORKER_PROTOCOL_VERSION,
                "events": events,
            })),
        )
        .await
        {
            Ok(response) => response?,
            Err(_) => {
                self.mark_failed(NativeWorkerFailureKind::Timeout);
                let _ = self.child.start_kill();
                return Err(NativeEngineError::worker_failure(
                    "content process storage events",
                    NativeWorkerFailureKind::Timeout,
                    "content process storage-event synchronization exceeded its deadline",
                ));
            }
        };
        let result = require_response_kind(
            &response,
            "storage_events_synced",
            id,
            "content process storage events",
        );
        if result.is_err() {
            self.mark_failed(NativeWorkerFailureKind::Protocol);
            let _ = self.child.start_kill();
        }
        result
    }

    pub(crate) async fn sync_storage_state(
        &mut self,
        state: &NativeWebStorageState,
        indexed_db_state: &NativeIndexedDbState,
    ) -> Result<(), NativeEngineError> {
        let id = self.next_id();
        let response = match timeout(
            CONTENT_PROCESS_SCRIPT_TIMEOUT,
            self.exchange(json!({
                "kind": "storage_state",
                "id": id,
                "protocol": CONTENT_WORKER_PROTOCOL_VERSION,
                "state": state,
                "indexed_db_state": indexed_db_state,
            })),
        )
        .await
        {
            Ok(response) => response?,
            Err(_) => {
                self.mark_failed(NativeWorkerFailureKind::Timeout);
                let _ = self.child.start_kill();
                return Err(NativeEngineError::worker_failure(
                    "content process storage state",
                    NativeWorkerFailureKind::Timeout,
                    "content process storage-state synchronization exceeded its deadline",
                ));
            }
        };
        let result = require_response_kind(
            &response,
            "storage_state_synced",
            id,
            "content process storage state",
        );
        if result.is_err() {
            self.mark_failed(NativeWorkerFailureKind::Protocol);
            let _ = self.child.start_kill();
        }
        result
    }

    pub(crate) async fn cookies(
        &mut self,
        document_url: &str,
    ) -> Result<Vec<NativeCookieProfileEntry>, NativeEngineError> {
        let id = self.next_id();
        let response = self
            .exchange_with_timeout(
                json!({
                "kind": "cookies",
                "id": id,
                "protocol": CONTENT_WORKER_PROTOCOL_VERSION,
                "document_url": document_url,
                }),
                "content process cookies",
            )
            .await?;
        decode_cookie_profiles(&response, id, "content process cookies")
    }

    pub(crate) async fn set_cookies(
        &mut self,
        cookies: &[NativeCookieProfileEntry],
    ) -> Result<(), NativeEngineError> {
        let id = self.next_id();
        let response = self
            .exchange_with_timeout(
                json!({
                "kind": "set_cookies",
                "id": id,
                "protocol": CONTENT_WORKER_PROTOCOL_VERSION,
                "cookies": cookies,
                }),
                "content process set cookies",
            )
            .await?;
        require_response_kind(&response, "cookies_set", id, "content process set cookies")
    }

    pub(crate) async fn clear_cookies(&mut self) -> Result<(), NativeEngineError> {
        let id = self.next_id();
        let response = self
            .exchange_with_timeout(
                json!({
                "kind": "clear_cookies",
                "id": id,
                "protocol": CONTENT_WORKER_PROTOCOL_VERSION,
                }),
                "content process clear cookies",
            )
            .await?;
        require_response_kind(
            &response,
            "cookies_cleared",
            id,
            "content process clear cookies",
        )
    }

    async fn mutate_with_request_kind(
        &mut self,
        id: u64,
        request_kind: &str,
        action: Value,
    ) -> Result<NativeContentMutation, NativeEngineError> {
        let response = match timeout(
            CONTENT_PROCESS_MUTATION_TIMEOUT,
            self.exchange(json!({
                "kind": request_kind,
                "id": id,
                "protocol": CONTENT_WORKER_PROTOCOL_VERSION,
                "action": action,
            })),
        )
        .await
        {
            Ok(response) => response?,
            Err(_) => {
                self.mark_failed(NativeWorkerFailureKind::Timeout);
                let _ = self.child.start_kill();
                return Err(NativeEngineError::worker_failure(
                    "content process mutation",
                    NativeWorkerFailureKind::Timeout,
                    "content process mutation exceeded its deadline",
                ));
            }
        };
        if response.get("kind").and_then(Value::as_str) == Some("error") {
            self.mark_failed(NativeWorkerFailureKind::Rejected);
            let _ = self.child.start_kill();
            return Err(NativeEngineError::worker_failure(
                "content process mutation",
                NativeWorkerFailureKind::Rejected,
                response
                    .get("reason")
                    .and_then(Value::as_str)
                    .unwrap_or("content process rejected the mutation"),
            ));
        }
        let result = decode_mutated_response(&response, id);
        if result.is_err() {
            self.mark_failed(NativeWorkerFailureKind::InvalidTransfer);
            let _ = self.child.start_kill();
        }
        result
    }

    pub(crate) fn is_healthy(&self) -> bool {
        self.healthy
    }

    pub(crate) fn failure_kind(&self) -> Option<NativeWorkerFailureKind> {
        self.failure_kind
    }

    pub(crate) async fn close(mut self) -> Result<(), NativeEngineError> {
        if let Ok(Some(_)) = self.child.try_wait() {
            self.mark_failed(NativeWorkerFailureKind::Exited);
            return Ok(());
        }
        let id = self.next_id();
        let response = self
            .exchange(json!({
                "kind": "close",
                "id": id,
                "protocol": CONTENT_WORKER_PROTOCOL_VERSION,
            }))
            .await?;
        require_response_kind(&response, "closed", id, "content process close")?;
        match timeout(Duration::from_secs(1), self.child.wait()).await {
            Ok(Ok(_)) => Ok(()),
            Ok(Err(_)) => Err(NativeEngineError::Worker {
                operation: "wait for content process".into(),
                reason: "content process did not report a valid exit".into(),
            }),
            Err(_) => {
                let _ = self.child.start_kill();
                Ok(())
            }
        }
    }

    async fn exchange(&mut self, request: Value) -> Result<Value, NativeEngineError> {
        let result = self.exchange_inner(request).await;
        match result {
            Ok(response) => Ok(response),
            Err(error) => {
                let kind = match self.child.try_wait() {
                    Ok(Some(_)) => NativeWorkerFailureKind::Exited,
                    Ok(None) | Err(_) => NativeWorkerFailureKind::Transport,
                };
                self.mark_failed(kind);
                Err(NativeEngineError::worker_failure(
                    "content process IPC",
                    kind,
                    error.to_string(),
                ))
            }
        }
    }

    async fn exchange_with_timeout(
        &mut self,
        request: Value,
        operation: &str,
    ) -> Result<Value, NativeEngineError> {
        match timeout(CONTENT_PROCESS_SCRIPT_TIMEOUT, self.exchange(request)).await {
            Ok(response) => response,
            Err(_) => {
                self.mark_failed(NativeWorkerFailureKind::Timeout);
                let _ = self.child.start_kill();
                Err(NativeEngineError::worker_failure(
                    operation,
                    NativeWorkerFailureKind::Timeout,
                    "content process cookie operation exceeded its deadline",
                ))
            }
        }
    }

    fn mark_failed(&mut self, kind: NativeWorkerFailureKind) {
        self.healthy = false;
        self.failure_kind = Some(kind);
    }

    async fn exchange_inner(&mut self, request: Value) -> Result<Value, NativeEngineError> {
        let payload = serde_json::to_vec(&request).map_err(|_| NativeEngineError::Worker {
            operation: "encode content IPC".into(),
            reason: "content process request could not be encoded".into(),
        })?;
        write_frame(&mut self.stdin, &payload).await?;
        let response = read_frame(&mut self.stdout).await?;
        serde_json::from_slice(&response).map_err(|_| NativeEngineError::Worker {
            operation: "decode content IPC".into(),
            reason: "content process returned an invalid response".into(),
        })
    }

    fn next_id(&mut self) -> u64 {
        let id = self.next_request_id;
        self.next_request_id = self.next_request_id.saturating_add(1);
        id
    }
}

impl Drop for NativeContentProcess {
    fn drop(&mut self) {
        let _ = self.child.start_kill();
    }
}

async fn write_frame(
    writer: &mut (impl AsyncWrite + Unpin),
    payload: &[u8],
) -> Result<(), NativeEngineError> {
    if payload.len() > MAX_CONTENT_IPC_FRAME_BYTES {
        return Err(NativeEngineError::limit(
            "content IPC frame",
            MAX_CONTENT_IPC_FRAME_BYTES,
            payload.len(),
        ));
    }
    let length = u32::try_from(payload.len()).map_err(|_| {
        NativeEngineError::limit("content IPC frame", u32::MAX as usize, payload.len())
    })?;
    writer
        .write_all(&length.to_be_bytes())
        .await
        .map_err(|_| NativeEngineError::Worker {
            operation: "write content IPC".into(),
            reason: "content process pipe is unavailable".into(),
        })?;
    writer
        .write_all(payload)
        .await
        .map_err(|_| NativeEngineError::Worker {
            operation: "write content IPC".into(),
            reason: "content process pipe is unavailable".into(),
        })?;
    writer.flush().await.map_err(|_| NativeEngineError::Worker {
        operation: "write content IPC".into(),
        reason: "content process pipe is unavailable".into(),
    })
}

async fn read_frame(reader: &mut (impl AsyncRead + Unpin)) -> Result<Vec<u8>, NativeEngineError> {
    let mut length = [0_u8; 4];
    reader
        .read_exact(&mut length)
        .await
        .map_err(|_| NativeEngineError::Worker {
            operation: "read content IPC".into(),
            reason: "content process exited or closed its pipe".into(),
        })?;
    let length = u32::from_be_bytes(length) as usize;
    if length > MAX_CONTENT_IPC_FRAME_BYTES {
        return Err(NativeEngineError::limit(
            "content IPC frame",
            MAX_CONTENT_IPC_FRAME_BYTES,
            length,
        ));
    }
    let mut payload = vec![0_u8; length];
    reader
        .read_exact(&mut payload)
        .await
        .map_err(|_| NativeEngineError::Worker {
            operation: "read content IPC".into(),
            reason: "content process returned a truncated frame".into(),
        })?;
    Ok(payload)
}

fn require_response_kind(
    response: &Value,
    expected: &str,
    expected_id: u64,
    operation: &str,
) -> Result<(), NativeEngineError> {
    if response.get("kind").and_then(Value::as_str) == Some(expected)
        && response.get("id").and_then(Value::as_u64) == Some(expected_id)
    {
        return Ok(());
    }
    Err(NativeEngineError::worker_failure(
        operation,
        NativeWorkerFailureKind::Protocol,
        "content process rejected the typed command",
    ))
}

fn decode_loaded_response(
    response: &Value,
    id: u64,
) -> Result<NativeContentLoad, NativeEngineError> {
    require_response_kind(response, "loaded", id, "content process load")?;
    let url =
        response
            .get("url")
            .and_then(Value::as_str)
            .ok_or_else(|| NativeEngineError::Worker {
                operation: "decode content process load".into(),
                reason: "content process omitted the final URL".into(),
            })?;
    validate_url_text("content process final URL", url)?;
    if !is_network_url(without_fragment(url)) {
        return Err(NativeEngineError::Worker {
            operation: "decode content process load".into(),
            reason: "content process returned a non-HTTP(S) final URL".into(),
        });
    }
    let encoded_document = response
        .get("document_base64")
        .and_then(Value::as_str)
        .ok_or_else(|| NativeEngineError::Worker {
            operation: "decode content process load".into(),
            reason: "content process omitted the document snapshot".into(),
        })?;
    let document_bytes = base64::engine::general_purpose::STANDARD
        .decode(encoded_document)
        .map_err(|_| NativeEngineError::Worker {
            operation: "decode content process load".into(),
            reason: "content process returned an invalid document snapshot".into(),
        })?;
    if document_bytes.len() > MAX_CONTENT_DOCUMENT_WIRE_BYTES {
        return Err(NativeEngineError::limit(
            "content-process document snapshot",
            MAX_CONTENT_DOCUMENT_WIRE_BYTES,
            document_bytes.len(),
        ));
    }
    let document = decode_document_wire(&document_bytes, "decode content process load")?;
    let events = decode_event_payload(response, "decode content process load")?;
    let scroll_commands = decode_scroll_commands(response, "decode content process load")?;
    let origin_url =
        url::Url::parse(without_fragment(url)).map_err(|_| NativeEngineError::Worker {
            operation: "decode content process load".into(),
            reason: "content process returned invalid final URL syntax".into(),
        })?;
    let origin = NativeOrigin::from_url(&origin_url)?;
    let navigation = response
        .get("navigation")
        .filter(|value| !value.is_null())
        .map(|value| decode_content_navigation(value, "decode content process load"))
        .transpose()?;
    let storage_events = decode_storage_events(response, "decode content process load")?;
    let indexed_db_changes = decode_indexed_db_changes(response, "decode content process load")?;
    let dialogs = decode_dialogs(response, "decode content process load")?;
    let popups = decode_popup_requests(response, "decode content process load")?;
    let post_messages = decode_post_message_requests(response, "decode content process load")?;
    let window_closes = decode_window_close_requests(response, "decode content process load")?;
    let window_navigations =
        decode_window_navigation_requests(response, "decode content process load")?;
    let window_name = decode_window_name(response, "decode content process load")?;
    let frame_sources = decode_frame_sources(response, "decode content process load")?;
    Ok(NativeContentLoad {
        url: url.into(),
        origin,
        document,
        frame_sources,
        events,
        scroll_commands,
        navigation,
        storage_events,
        indexed_db_changes,
        dialogs,
        popups,
        post_messages,
        window_closes,
        window_navigations,
        window_name,
    })
}

fn decode_frame_sources(
    response: &Value,
    operation: &str,
) -> Result<Option<Vec<String>>, NativeEngineError> {
    let Some(value) = response.get("frame_sources") else {
        return Ok(None);
    };
    if value.is_null() {
        return Ok(None);
    }
    let sources = value.as_array().ok_or_else(|| NativeEngineError::Worker {
        operation: operation.into(),
        reason: "content process returned invalid frame policy sources".into(),
    })?;
    if sources.len() > MAX_CONTENT_FRAME_SOURCES {
        return Err(NativeEngineError::limit(
            "content-process frame policy sources",
            MAX_CONTENT_FRAME_SOURCES,
            sources.len(),
        ));
    }
    let mut decoded = Vec::with_capacity(sources.len());
    let mut total_bytes = 0usize;
    for source in sources {
        let source = source.as_str().ok_or_else(|| NativeEngineError::Worker {
            operation: operation.into(),
            reason: "content process returned a non-text frame policy source".into(),
        })?;
        if source.is_empty() || source.bytes().any(|byte| byte.is_ascii_control()) {
            return Err(NativeEngineError::Worker {
                operation: operation.into(),
                reason: "content process returned an invalid frame policy source".into(),
            });
        }
        if source.len() > MAX_NATIVE_RESPONSE_HEADER_VALUE_BYTES {
            return Err(NativeEngineError::limit(
                "content-process frame policy source",
                MAX_NATIVE_RESPONSE_HEADER_VALUE_BYTES,
                source.len(),
            ));
        }
        let next_bytes = total_bytes.saturating_add(source.len());
        if next_bytes > MAX_NATIVE_RESPONSE_HEADER_BYTES {
            return Err(NativeEngineError::limit(
                "content-process frame policy sources",
                MAX_NATIVE_RESPONSE_HEADER_BYTES,
                next_bytes,
            ));
        }
        total_bytes = next_bytes;
        decoded.push(source.to_owned());
    }
    Ok(Some(decoded))
}

fn decode_frame_script_bindings(
    value: Option<&Value>,
    operation: &str,
) -> Result<Vec<NativeFrameScriptBinding>, NativeEngineError> {
    let Some(value) = value else {
        return Ok(Vec::new());
    };
    let encoded = serde_json::to_vec(value).map_err(|_| NativeEngineError::Worker {
        operation: operation.into(),
        reason: "content process frame script bindings were not serializable".into(),
    })?;
    if encoded.len() > MAX_CONTENT_DOCUMENT_WIRE_BYTES {
        return Err(NativeEngineError::limit(
            "content-process frame script bindings",
            MAX_CONTENT_DOCUMENT_WIRE_BYTES,
            encoded.len(),
        ));
    }
    let bindings: Vec<NativeFrameScriptBinding> =
        serde_json::from_value(value.clone()).map_err(|_| NativeEngineError::Worker {
            operation: operation.into(),
            reason: "content process returned invalid frame script bindings".into(),
        })?;
    if bindings.len() > MAX_CONTENT_FRAME_SOURCES {
        return Err(NativeEngineError::limit(
            "content-process frame script bindings",
            MAX_CONTENT_FRAME_SOURCES,
            bindings.len(),
        ));
    }
    for binding in &bindings {
        if binding.frame_id.is_empty() {
            return Err(NativeEngineError::Worker {
                operation: operation.into(),
                reason: "content process returned an empty frame script ID".into(),
            });
        }
        validate_url_text("content-process frame script URL", &binding.url)?;
        if binding.origin.is_empty() {
            return Err(NativeEngineError::Worker {
                operation: operation.into(),
                reason: "content process returned an empty frame script origin".into(),
            });
        }
    }
    Ok(bindings)
}

fn decode_frame_script_context(
    value: Option<&Value>,
    operation: &str,
) -> Result<Option<NativeFrameScriptContext>, NativeEngineError> {
    let Some(value) = value else {
        return Ok(None);
    };
    if value.is_null() {
        return Ok(None);
    }
    let encoded = serde_json::to_vec(value).map_err(|_| NativeEngineError::Worker {
        operation: operation.into(),
        reason: "content process frame script context was not serializable".into(),
    })?;
    if encoded.len() > MAX_CONTENT_DOCUMENT_WIRE_BYTES {
        return Err(NativeEngineError::limit(
            "content-process frame script context",
            MAX_CONTENT_DOCUMENT_WIRE_BYTES,
            encoded.len(),
        ));
    }
    let context: NativeFrameScriptContext =
        serde_json::from_value(value.clone()).map_err(|_| NativeEngineError::Worker {
            operation: operation.into(),
            reason: "content process returned invalid frame script context".into(),
        })?;
    validate_context_id(&context.current_frame_id)?;
    if let Some(parent) = context.parent.as_ref() {
        validate_frame_script_window(parent, operation)?;
    }
    if let Some(top) = context.top.as_ref() {
        validate_frame_script_window(top, operation)?;
    }
    Ok(Some(context))
}

fn validate_frame_script_window(
    window: &NativeFrameScriptWindow,
    operation: &str,
) -> Result<(), NativeEngineError> {
    validate_context_id(&window.context_id)?;
    validate_url_text("content-process frame script window URL", &window.url)?;
    if window.origin.is_empty() {
        return Err(NativeEngineError::Worker {
            operation: operation.into(),
            reason: "content process returned an empty frame script window origin".into(),
        });
    }
    if window.children.len() > MAX_CONTENT_FRAME_SOURCES {
        return Err(NativeEngineError::limit(
            "content-process frame script window children",
            MAX_CONTENT_FRAME_SOURCES,
            window.children.len(),
        ));
    }
    for child in &window.children {
        validate_context_id(&child.frame_id)?;
        validate_url_text("content-process frame script child URL", &child.url)?;
        if child.origin.is_empty() {
            return Err(NativeEngineError::Worker {
                operation: operation.into(),
                reason: "content process returned an empty frame script child origin".into(),
            });
        }
        if child.children.len() > MAX_CONTENT_FRAME_SOURCES {
            return Err(NativeEngineError::limit(
                "content-process frame script descendants",
                MAX_CONTENT_FRAME_SOURCES,
                child.children.len(),
            ));
        }
    }
    Ok(())
}

fn decode_cookie_profiles(
    response: &Value,
    id: u64,
    operation: &str,
) -> Result<Vec<NativeCookieProfileEntry>, NativeEngineError> {
    require_response_kind(response, "cookies", id, operation)?;
    let value = response
        .get("cookies")
        .ok_or_else(|| NativeEngineError::Worker {
            operation: operation.into(),
            reason: "content process omitted cookie profiles".into(),
        })?;
    let cookies: Vec<NativeCookieProfileEntry> =
        serde_json::from_value(value.clone()).map_err(|_| NativeEngineError::Worker {
            operation: operation.into(),
            reason: "content process returned invalid cookie profiles".into(),
        })?;
    if cookies.len() > super::javascript::MAX_NATIVE_COOKIE_PROFILE_ENTRIES {
        return Err(NativeEngineError::limit(
            "content-process cookie profiles",
            super::javascript::MAX_NATIVE_COOKIE_PROFILE_ENTRIES,
            cookies.len(),
        ));
    }
    Ok(cookies)
}

fn decode_mutated_response(
    response: &Value,
    id: u64,
) -> Result<NativeContentMutation, NativeEngineError> {
    require_response_kind(response, "mutated", id, "content process mutation")?;
    decode_mutation_payload(response, "decode content process mutation")
}

fn decode_mutation_payload(
    response: &Value,
    operation: &str,
) -> Result<NativeContentMutation, NativeEngineError> {
    let encoded_document = response
        .get("document_base64")
        .and_then(Value::as_str)
        .ok_or_else(|| NativeEngineError::Worker {
            operation: operation.into(),
            reason: "content process omitted the mutated document snapshot".into(),
        })?;
    let document_bytes = base64::engine::general_purpose::STANDARD
        .decode(encoded_document)
        .map_err(|_| NativeEngineError::Worker {
            operation: operation.into(),
            reason: "content process returned an invalid document snapshot".into(),
        })?;
    let document = decode_document_wire(&document_bytes, operation)?;
    let events = decode_event_payload(response, operation)?;
    let navigation = response
        .get("navigation")
        .filter(|value| !value.is_null())
        .map(|value| decode_content_navigation(value, operation))
        .transpose()?;
    let storage_events = decode_storage_events(response, operation)?;
    let indexed_db_changes = decode_indexed_db_changes(response, operation)?;
    let dialogs = decode_dialogs(response, operation)?;
    let popups = decode_popup_requests(response, operation)?;
    let post_messages = decode_post_message_requests(response, operation)?;
    let window_closes = decode_window_close_requests(response, operation)?;
    let window_navigations = decode_window_navigation_requests(response, operation)?;
    let window_name = decode_window_name(response, operation)?;
    let history = decode_mutation_history(response, operation)?;
    let scroll_commands = decode_scroll_commands(response, operation)?;
    Ok(NativeContentMutation {
        document,
        events,
        navigation,
        allowed: response
            .get("allowed")
            .and_then(Value::as_bool)
            .unwrap_or(true),
        history,
        scroll_commands,
        storage_events,
        indexed_db_changes,
        dialogs,
        popups,
        post_messages,
        window_closes,
        window_navigations,
        window_name,
    })
}

fn decode_scroll_commands(
    response: &Value,
    operation: &str,
) -> Result<Vec<NativeScriptCommand>, NativeEngineError> {
    let Some(value) = response.get("scroll_commands") else {
        return Ok(Vec::new());
    };
    let values = value.as_array().ok_or_else(|| NativeEngineError::Worker {
        operation: operation.into(),
        reason: "content process returned invalid scroll commands".into(),
    })?;
    if values.len() > MAX_NATIVE_EFFECTS {
        return Err(NativeEngineError::limit(
            "content-process scroll commands",
            MAX_NATIVE_EFFECTS,
            values.len(),
        ));
    }
    let commands =
        serde_json::from_value::<Vec<NativeScriptCommand>>(value.clone()).map_err(|_| {
            NativeEngineError::Worker {
                operation: operation.into(),
                reason: "content process returned malformed scroll commands".into(),
            }
        })?;
    for command in &commands {
        let NativeScriptCommand::ScrollTo {
            node_index,
            left,
            top,
        } = command
        else {
            return Err(NativeEngineError::Worker {
                operation: operation.into(),
                reason: "content process returned a non-scroll command in scroll_commands".into(),
            });
        };
        if *node_index == u32::MAX || *left < 0 || *top < 0 {
            return Err(NativeEngineError::Worker {
                operation: operation.into(),
                reason: "content process returned an invalid scroll target or offset".into(),
            });
        }
    }
    Ok(commands)
}

fn decode_history_commands(
    response: &Value,
    operation: &str,
) -> Result<Vec<NativeScriptCommand>, NativeEngineError> {
    let Some(value) = response.get("history") else {
        return Ok(Vec::new());
    };
    decode_history_command_value(value, operation)
}

fn decode_mutation_history(
    response: &Value,
    operation: &str,
) -> Result<Vec<NativeScriptCommand>, NativeEngineError> {
    let Some(value) = response.get("mutation_history") else {
        return Ok(Vec::new());
    };
    decode_history_command_value(value, operation)
}

fn decode_history_command_value(
    value: &Value,
    operation: &str,
) -> Result<Vec<NativeScriptCommand>, NativeEngineError> {
    let values = value.as_array().ok_or_else(|| NativeEngineError::Worker {
        operation: operation.into(),
        reason: "content process returned invalid history commands".into(),
    })?;
    if values.len() > MAX_NATIVE_EFFECTS {
        return Err(NativeEngineError::limit(
            "content-process history commands",
            MAX_NATIVE_EFFECTS,
            values.len(),
        ));
    }
    let commands =
        serde_json::from_value::<Vec<NativeScriptCommand>>(value.clone()).map_err(|_| {
            NativeEngineError::Worker {
                operation: operation.into(),
                reason: "content process returned malformed history commands".into(),
            }
        })?;
    for command in &commands {
        match command {
            NativeScriptCommand::HistoryPushState { state, .. }
            | NativeScriptCommand::HistoryReplaceState { state, .. } => {
                let encoded = serde_json::to_vec(state).map_err(|_| NativeEngineError::Worker {
                    operation: operation.into(),
                    reason: "content process returned unserializable history state".into(),
                })?;
                if encoded.len() > MAX_NATIVE_HISTORY_STATE_BYTES {
                    return Err(NativeEngineError::limit(
                        "content-process history state",
                        MAX_NATIVE_HISTORY_STATE_BYTES,
                        encoded.len(),
                    ));
                }
            }
            NativeScriptCommand::HistoryGo { delta } => {
                if delta.unsigned_abs() > 1024 {
                    return Err(NativeEngineError::limit(
                        "content-process history traversal delta",
                        1024,
                        delta.unsigned_abs() as usize,
                    ));
                }
            }
            _ => {
                return Err(NativeEngineError::Worker {
                    operation: operation.into(),
                    reason: "content process returned a non-history command in history output"
                        .into(),
                });
            }
        }
    }
    Ok(commands)
}

fn decode_event_payload(
    response: &Value,
    operation: &str,
) -> Result<Vec<NativeContentEvent>, NativeEngineError> {
    let event_values = response
        .get("events")
        .and_then(Value::as_array)
        .ok_or_else(|| NativeEngineError::Worker {
            operation: operation.into(),
            reason: "content process omitted event effects".into(),
        })?;
    if event_values.len() > MAX_NATIVE_EFFECTS {
        return Err(NativeEngineError::limit(
            "content-process mutation effects",
            MAX_NATIVE_EFFECTS,
            event_values.len(),
        ));
    }
    let mut events = Vec::with_capacity(event_values.len());
    for value in event_values {
        let node_index = value
            .get("node_index")
            .and_then(Value::as_u64)
            .and_then(|value| u32::try_from(value).ok())
            .ok_or_else(|| NativeEngineError::Worker {
                operation: operation.into(),
                reason: "content process returned an invalid mutation node".into(),
            })?;
        let kind = value
            .get("kind")
            .and_then(Value::as_str)
            .and_then(parse_event_kind)
            .ok_or_else(|| NativeEngineError::Worker {
                operation: operation.into(),
                reason: "content process returned an invalid mutation effect".into(),
            })?;
        events.push(NativeContentEvent { node_index, kind });
    }
    Ok(events)
}

fn decode_popup_requests(
    response: &Value,
    operation: &str,
) -> Result<Vec<NativePopupRequest>, NativeEngineError> {
    let Some(value) = response.get("popups") else {
        return Ok(Vec::new());
    };
    let values = value.as_array().ok_or_else(|| NativeEngineError::Worker {
        operation: operation.into(),
        reason: "content process returned invalid popup requests".into(),
    })?;
    if values.len() > MAX_NATIVE_EFFECTS {
        return Err(NativeEngineError::limit(
            "content-process popup requests",
            MAX_NATIVE_EFFECTS,
            values.len(),
        ));
    }
    let mut popups =
        serde_json::from_value::<Vec<NativePopupRequest>>(value.clone()).map_err(|_| {
            NativeEngineError::Worker {
                operation: operation.into(),
                reason: "content process returned malformed popup requests".into(),
            }
        })?;
    for popup in &mut popups {
        validate_url_text("content-process popup URL", &popup.url)?;
        if popup.target.is_empty() {
            popup.target = "_blank".into();
        } else {
            validate_url_text("content-process popup target", &popup.target)?;
        }
    }
    Ok(popups)
}

fn decode_post_message_requests(
    response: &Value,
    operation: &str,
) -> Result<Vec<NativePostMessageRequest>, NativeEngineError> {
    let Some(value) = response.get("post_messages") else {
        return Ok(Vec::new());
    };
    let values = value.as_array().ok_or_else(|| NativeEngineError::Worker {
        operation: operation.into(),
        reason: "content process returned invalid postMessage requests".into(),
    })?;
    if values.len() > MAX_NATIVE_EFFECTS {
        return Err(NativeEngineError::limit(
            "content-process postMessage requests",
            MAX_NATIVE_EFFECTS,
            values.len(),
        ));
    }
    let messages =
        serde_json::from_value::<Vec<NativePostMessageRequest>>(value.clone()).map_err(|_| {
            NativeEngineError::Worker {
                operation: operation.into(),
                reason: "content process returned malformed postMessage requests".into(),
            }
        })?;
    for message in &messages {
        if message.target.is_empty() && message.target_context_id.is_none() {
            return Err(NativeEngineError::invalid(
                "content-process postMessage target",
                "must not be empty without a direct target context",
            ));
        }
        if !message.target.is_empty() {
            validate_url_text("content-process postMessage target", &message.target)?;
        }
        validate_url_text(
            "content-process postMessage target origin",
            &message.target_origin,
        )?;
        if let Some(target_context_id) = message.target_context_id.as_deref() {
            validate_context_id(target_context_id)?;
        }
        let encoded = serde_json::to_vec(&message.data).map_err(|_| NativeEngineError::Worker {
            operation: operation.into(),
            reason: "content process returned unserializable postMessage data".into(),
        })?;
        if encoded.len() > super::javascript::MAX_NATIVE_POST_MESSAGE_BYTES {
            return Err(NativeEngineError::limit(
                "content-process postMessage data",
                super::javascript::MAX_NATIVE_POST_MESSAGE_BYTES,
                encoded.len(),
            ));
        }
    }
    Ok(messages)
}

fn decode_content_navigation(
    value: &Value,
    operation: &str,
) -> Result<NativeContentNavigation, NativeEngineError> {
    let node_index = value
        .get("node_index")
        .and_then(Value::as_u64)
        .and_then(|value| u32::try_from(value).ok())
        .ok_or_else(|| NativeEngineError::Worker {
            operation: operation.into(),
            reason: "content process returned an invalid navigation node".into(),
        })?;
    let href =
        value
            .get("href")
            .and_then(Value::as_str)
            .ok_or_else(|| NativeEngineError::Worker {
                operation: operation.into(),
                reason: "content process returned an invalid navigation href".into(),
            })?;
    validate_url_text("content process navigation href", href)?;
    let submitter_node_index = match value.get("submitter_node_index") {
        None | Some(Value::Null) => None,
        Some(value) => Some(
            value
                .as_u64()
                .and_then(|value| u32::try_from(value).ok())
                .ok_or_else(|| NativeEngineError::Worker {
                    operation: operation.into(),
                    reason: "content process returned an invalid submitter node".into(),
                })?,
        ),
    };
    let location = match value.get("location") {
        None => false,
        Some(value) => value.as_bool().ok_or_else(|| NativeEngineError::Worker {
            operation: operation.into(),
            reason: "content process returned an invalid location navigation flag".into(),
        })?,
    };
    let replace_history = match value.get("replace_history") {
        None => false,
        Some(value) => value.as_bool().ok_or_else(|| NativeEngineError::Worker {
            operation: operation.into(),
            reason: "content process returned an invalid history replacement flag".into(),
        })?,
    };
    if location && (node_index != 0 || submitter_node_index.is_some()) {
        return Err(NativeEngineError::Worker {
            operation: operation.into(),
            reason: "location navigation carried a DOM target".into(),
        });
    }
    if replace_history && !location {
        return Err(NativeEngineError::Worker {
            operation: operation.into(),
            reason: "history replacement was returned without location navigation".into(),
        });
    }
    Ok(NativeContentNavigation {
        node_index,
        href: href.to_owned(),
        submitter_node_index,
        location,
        replace_history,
    })
}

fn decode_fetch_headers(response: &Value) -> Result<Vec<(String, String)>, NativeEngineError> {
    let Some(raw_headers) = response.get("headers") else {
        return Ok(Vec::new());
    };
    let Some(raw_headers) = raw_headers.as_array() else {
        return Err(NativeEngineError::Worker {
            operation: "decode content process fetch".into(),
            reason: "content process returned invalid response headers".into(),
        });
    };
    if raw_headers.len() > MAX_NATIVE_RESPONSE_HEADERS {
        return Err(NativeEngineError::limit(
            "content-process response headers",
            MAX_NATIVE_RESPONSE_HEADERS,
            raw_headers.len(),
        ));
    }
    let mut headers = Vec::with_capacity(raw_headers.len());
    let mut total_bytes = 0usize;
    for raw_header in raw_headers {
        let Some(raw_header) = raw_header.as_array() else {
            return Err(NativeEngineError::Worker {
                operation: "decode content process fetch".into(),
                reason: "content process returned an invalid response header entry".into(),
            });
        };
        if raw_header.len() != 2 {
            return Err(NativeEngineError::Worker {
                operation: "decode content process fetch".into(),
                reason: "content process returned an invalid response header pair".into(),
            });
        }
        let name = raw_header[0]
            .as_str()
            .ok_or_else(|| NativeEngineError::Worker {
                operation: "decode content process fetch".into(),
                reason: "content process returned a non-text response header name".into(),
            })?;
        let value = raw_header[1]
            .as_str()
            .ok_or_else(|| NativeEngineError::Worker {
                operation: "decode content process fetch".into(),
                reason: "content process returned a non-text response header value".into(),
            })?;
        let normalized_name = name.to_ascii_lowercase();
        if normalized_name.len() > MAX_NATIVE_RESPONSE_HEADER_NAME_BYTES {
            return Err(NativeEngineError::limit(
                "content-process response header name",
                MAX_NATIVE_RESPONSE_HEADER_NAME_BYTES,
                normalized_name.len(),
            ));
        }
        reqwest::header::HeaderName::from_bytes(normalized_name.as_bytes()).map_err(|_| {
            NativeEngineError::Worker {
                operation: "decode content process fetch".into(),
                reason: "content process returned an invalid response header name".into(),
            }
        })?;
        if value.len() > MAX_NATIVE_RESPONSE_HEADER_VALUE_BYTES {
            return Err(NativeEngineError::limit(
                "content-process response header value",
                MAX_NATIVE_RESPONSE_HEADER_VALUE_BYTES,
                value.len(),
            ));
        }
        let next_bytes = total_bytes
            .saturating_add(normalized_name.len())
            .saturating_add(value.len());
        if next_bytes > MAX_NATIVE_RESPONSE_HEADER_BYTES {
            return Err(NativeEngineError::limit(
                "content-process response headers",
                MAX_NATIVE_RESPONSE_HEADER_BYTES,
                next_bytes,
            ));
        }
        total_bytes = next_bytes;
        headers.push((normalized_name, value.to_owned()));
    }
    Ok(headers)
}

fn decode_fetch_response(
    response: &Value,
    id: u64,
) -> Result<NativeFetchResponse, NativeEngineError> {
    require_response_kind(response, "fetched", id, "content process fetch")?;
    let url =
        response
            .get("url")
            .and_then(Value::as_str)
            .ok_or_else(|| NativeEngineError::Worker {
                operation: "decode content process fetch".into(),
                reason: "content process omitted the fetch URL".into(),
            })?;
    validate_url_text("content process fetch URL", url)?;
    if !is_network_url(without_fragment(url)) {
        return Err(NativeEngineError::Worker {
            operation: "decode content process fetch".into(),
            reason: "content process returned a non-HTTP(S) fetch URL".into(),
        });
    }
    let status = response
        .get("status")
        .and_then(Value::as_u64)
        .and_then(|value| u16::try_from(value).ok())
        .ok_or_else(|| NativeEngineError::Worker {
            operation: "decode content process fetch".into(),
            reason: "content process returned an invalid HTTP status".into(),
        })?;
    let content_type = response
        .get("content_type")
        .filter(|value| !value.is_null())
        .map(|value| {
            value
                .as_str()
                .map(str::to_owned)
                .ok_or_else(|| NativeEngineError::Worker {
                    operation: "decode content process fetch".into(),
                    reason: "content process returned an invalid content type".into(),
                })
        })
        .transpose()?;
    let opaque = response
        .get("opaque")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let redirected = response
        .get("redirected")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let opaque_redirect = response
        .get("opaque_redirect")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let headers = decode_fetch_headers(response)?;
    let encoded_body = response
        .get("body_base64")
        .and_then(Value::as_str)
        .ok_or_else(|| NativeEngineError::Worker {
            operation: "decode content process fetch".into(),
            reason: "content process omitted the fetch body".into(),
        })?;
    let body = base64::engine::general_purpose::STANDARD
        .decode(encoded_body)
        .map_err(|_| NativeEngineError::Worker {
            operation: "decode content process fetch".into(),
            reason: "content process returned an invalid fetch body".into(),
        })?;
    if body.len() > MAX_CONTENT_DOCUMENT_WIRE_BYTES {
        return Err(NativeEngineError::limit(
            "content-process fetch response",
            MAX_CONTENT_DOCUMENT_WIRE_BYTES,
            body.len(),
        ));
    }
    Ok(NativeFetchResponse {
        url: url.to_owned(),
        status,
        content_type,
        headers,
        body,
        redirected,
        opaque,
        opaque_redirect,
    })
}

fn decode_script_response(
    response: &Value,
    id: u64,
) -> Result<NativeContentScriptResult, NativeEngineError> {
    require_response_kind(response, "evaluated", id, "content process script")?;
    let value = response
        .get("value")
        .cloned()
        .ok_or_else(|| NativeEngineError::Worker {
            operation: "decode content process script".into(),
            reason: "content process omitted the script result".into(),
        })?;
    let encoded = serde_json::to_vec(&value).map_err(|_| NativeEngineError::Worker {
        operation: "decode content process script".into(),
        reason: "content process returned an unserializable script result".into(),
    })?;
    if encoded.len() > super::javascript::MAX_NATIVE_SCRIPT_RESULT_BYTES {
        return Err(NativeEngineError::limit(
            "content-process script result",
            super::javascript::MAX_NATIVE_SCRIPT_RESULT_BYTES,
            encoded.len(),
        ));
    }
    let storage_events = decode_storage_events(response, "decode content process script")?;
    let indexed_db_changes = decode_indexed_db_changes(response, "decode content process script")?;
    let dialogs = decode_dialogs(response, "decode content process script")?;
    let has_document = response.get("document_base64").is_some();
    let has_events = response.get("events").is_some();
    let mutation = if has_document || has_events {
        Some(decode_mutation_payload(
            response,
            "decode content process script",
        )?)
    } else {
        None
    };
    let has_mutation = mutation.is_some();
    let post_messages = decode_post_message_requests(&response, "decode content process script")?;
    let window_closes = decode_window_close_requests(&response, "decode content process script")?;
    let window_navigations =
        decode_window_navigation_requests(&response, "decode content process script")?;
    let frame_scripts = decode_frame_script_requests(&response, "decode content process script")?;
    let window_name = decode_window_name(&response, "decode content process script")?;
    let history = decode_history_commands(&response, "decode content process script")?;
    Ok(NativeContentScriptResult {
        value,
        storage_events: if has_mutation {
            Vec::new()
        } else {
            storage_events
        },
        mutation,
        history,
        frame_scripts,
        indexed_db_changes,
        dialogs: if has_mutation { Vec::new() } else { dialogs },
        popups: if has_mutation {
            Vec::new()
        } else {
            decode_popup_requests(&response, "decode content process script")?
        },
        post_messages: if has_mutation {
            Vec::new()
        } else {
            post_messages
        },
        window_closes: if has_mutation {
            Vec::new()
        } else {
            window_closes
        },
        window_navigations: if has_mutation {
            Vec::new()
        } else {
            window_navigations
        },
        window_name,
    })
}

fn decode_frame_script_requests(
    response: &Value,
    operation: &str,
) -> Result<Vec<NativeFrameScriptRequest>, NativeEngineError> {
    let Some(value) = response.get("frame_scripts") else {
        return Ok(Vec::new());
    };
    let values = value.as_array().ok_or_else(|| NativeEngineError::Worker {
        operation: operation.into(),
        reason: "content process returned invalid frame script requests".into(),
    })?;
    if values.len() > MAX_NATIVE_EFFECTS {
        return Err(NativeEngineError::limit(
            "content-process frame script requests",
            MAX_NATIVE_EFFECTS,
            values.len(),
        ));
    }
    let requests =
        serde_json::from_value::<Vec<NativeFrameScriptRequest>>(value.clone()).map_err(|_| {
            NativeEngineError::Worker {
                operation: operation.into(),
                reason: "content process returned malformed frame script requests".into(),
            }
        })?;
    for request in &requests {
        validate_context_id(&request.frame_id)?;
        validate_context_id(&request.source_frame_id)?;
        if matches!(
            request.command.as_ref(),
            NativeScriptCommand::FrameScript { .. }
        ) {
            return Err(NativeEngineError::Worker {
                operation: operation.into(),
                reason: "content process returned a nested frame script request".into(),
            });
        }
        let encoded = serde_json::to_vec(request.command.as_ref()).map_err(|_| {
            NativeEngineError::Worker {
                operation: operation.into(),
                reason: "content process returned an unserializable frame script command".into(),
            }
        })?;
        if encoded.len() > super::javascript::MAX_NATIVE_SCRIPT_RESULT_BYTES {
            return Err(NativeEngineError::limit(
                "content-process frame script command",
                super::javascript::MAX_NATIVE_SCRIPT_RESULT_BYTES,
                encoded.len(),
            ));
        }
    }
    Ok(requests)
}

fn decode_window_name(response: &Value, _operation: &str) -> Result<String, NativeEngineError> {
    let value = response
        .get("window_name")
        .and_then(Value::as_str)
        .unwrap_or_default();
    validate_window_name(value)?;
    Ok(value.to_owned())
}

fn decode_window_close_requests(
    response: &Value,
    operation: &str,
) -> Result<Vec<NativeWindowCloseRequest>, NativeEngineError> {
    let Some(value) = response.get("window_closes") else {
        return Ok(Vec::new());
    };
    let values = value.as_array().ok_or_else(|| NativeEngineError::Worker {
        operation: operation.into(),
        reason: "content process returned invalid window close requests".into(),
    })?;
    if values.len() > MAX_NATIVE_EFFECTS {
        return Err(NativeEngineError::limit(
            "content-process window close requests",
            MAX_NATIVE_EFFECTS,
            values.len(),
        ));
    }
    let requests =
        serde_json::from_value::<Vec<NativeWindowCloseRequest>>(value.clone()).map_err(|_| {
            NativeEngineError::Worker {
                operation: operation.into(),
                reason: "content process returned malformed window close requests".into(),
            }
        })?;
    for request in &requests {
        validate_url_text("content-process window close target", &request.target)?;
        if let Some(target_context_id) = request.target_context_id.as_deref() {
            validate_context_id(target_context_id)?;
        }
    }
    Ok(requests)
}

fn decode_window_navigation_requests(
    response: &Value,
    operation: &str,
) -> Result<Vec<NativeWindowNavigationRequest>, NativeEngineError> {
    let Some(value) = response.get("window_navigations") else {
        return Ok(Vec::new());
    };
    let values = value.as_array().ok_or_else(|| NativeEngineError::Worker {
        operation: operation.into(),
        reason: "content process returned invalid window navigation requests".into(),
    })?;
    if values.len() > MAX_NATIVE_EFFECTS {
        return Err(NativeEngineError::limit(
            "content-process window navigation requests",
            MAX_NATIVE_EFFECTS,
            values.len(),
        ));
    }
    let requests = serde_json::from_value::<Vec<NativeWindowNavigationRequest>>(value.clone())
        .map_err(|_| NativeEngineError::Worker {
            operation: operation.into(),
            reason: "content process returned malformed window navigation requests".into(),
        })?;
    for request in &requests {
        if request.target.is_empty() && request.target_context_id.is_none() {
            return Err(NativeEngineError::invalid(
                "content-process window navigation target",
                "must not be empty without a direct target context",
            ));
        }
        if !request.target.is_empty() {
            validate_url_text("content-process window navigation target", &request.target)?;
        }
        validate_url_text("content-process window navigation href", &request.href)?;
        if let Some(target_context_id) = request.target_context_id.as_deref() {
            validate_context_id(target_context_id)?;
        }
    }
    Ok(requests)
}

fn decode_window_proxy_updates(
    value: Option<&Value>,
    operation: &str,
) -> Result<Vec<NativeWindowProxyUpdate>, NativeEngineError> {
    let Some(value) = value else {
        return Ok(Vec::new());
    };
    let values = value.as_array().ok_or_else(|| NativeEngineError::Worker {
        operation: operation.into(),
        reason: "content process received invalid WindowProxy updates".into(),
    })?;
    if values.len() > MAX_NATIVE_EFFECTS {
        return Err(NativeEngineError::limit(
            "content-process WindowProxy updates",
            MAX_NATIVE_EFFECTS,
            values.len(),
        ));
    }
    let updates =
        serde_json::from_value::<Vec<NativeWindowProxyUpdate>>(value.clone()).map_err(|_| {
            NativeEngineError::Worker {
                operation: operation.into(),
                reason: "content process received malformed WindowProxy updates".into(),
            }
        })?;
    for update in &updates {
        if update.cache_key.len() > super::javascript::MAX_NATIVE_SCRIPT_RESULT_BYTES {
            return Err(NativeEngineError::limit(
                "content-process WindowProxy cache key",
                super::javascript::MAX_NATIVE_SCRIPT_RESULT_BYTES,
                update.cache_key.len(),
            ));
        }
        validate_context_id(&update.target_context_id)?;
        validate_url_text("content-process WindowProxy URL", &update.href)?;
        validate_window_name(&update.name)?;
    }
    Ok(updates)
}

fn decode_dialogs(
    response: &Value,
    operation: &str,
) -> Result<Vec<NativeDialog>, NativeEngineError> {
    let Some(value) = response.get("dialogs") else {
        return Ok(Vec::new());
    };
    let values = value.as_array().ok_or_else(|| NativeEngineError::Worker {
        operation: operation.into(),
        reason: "content process returned invalid dialogs".into(),
    })?;
    if values.len() > MAX_NATIVE_DIALOGS {
        return Err(NativeEngineError::limit(
            "content-process dialogs",
            MAX_NATIVE_DIALOGS,
            values.len(),
        ));
    }
    let dialogs = serde_json::from_value::<Vec<NativeDialog>>(value.clone()).map_err(|_| {
        NativeEngineError::Worker {
            operation: operation.into(),
            reason: "content process returned malformed dialogs".into(),
        }
    })?;
    for dialog in &dialogs {
        if !matches!(dialog.dialog_type.as_str(), "alert" | "confirm" | "prompt") {
            return Err(NativeEngineError::invalid(
                "content-process dialog type",
                "must be alert, confirm, or prompt",
            ));
        }
        if dialog.message.len() > MAX_NATIVE_DIALOG_TEXT_BYTES
            || dialog
                .default_value
                .as_ref()
                .is_some_and(|value| value.len() > MAX_NATIVE_DIALOG_TEXT_BYTES)
        {
            return Err(NativeEngineError::limit(
                "content-process dialog text",
                MAX_NATIVE_DIALOG_TEXT_BYTES,
                dialog
                    .default_value
                    .as_ref()
                    .map_or(dialog.message.len(), String::len),
            ));
        }
    }
    Ok(dialogs)
}

fn decode_storage_events(
    response: &Value,
    operation: &str,
) -> Result<Vec<NativeStorageEvent>, NativeEngineError> {
    let Some(value) = response.get("storage_events") else {
        return Ok(Vec::new());
    };
    decode_storage_event_value(value, operation)
}

fn decode_indexed_db_changes(
    response: &Value,
    operation: &str,
) -> Result<Vec<NativeIndexedDbChange>, NativeEngineError> {
    let Some(value) = response.get("indexed_db_changes") else {
        return Ok(Vec::new());
    };
    let changes: Vec<NativeIndexedDbChange> =
        serde_json::from_value(value.clone()).map_err(|_| NativeEngineError::Worker {
            operation: operation.into(),
            reason: "content process returned malformed IndexedDB changes".into(),
        })?;
    if changes.len() > MAX_NATIVE_INDEXED_DB_CHANGES {
        return Err(NativeEngineError::limit(
            "content-process IndexedDB changes",
            MAX_NATIVE_INDEXED_DB_CHANGES,
            changes.len(),
        ));
    }
    Ok(changes)
}

fn decode_storage_event_value(
    value: &Value,
    operation: &str,
) -> Result<Vec<NativeStorageEvent>, NativeEngineError> {
    let values = value.as_array().ok_or_else(|| NativeEngineError::Worker {
        operation: operation.into(),
        reason: "content process returned invalid storage events".into(),
    })?;
    if values.len() > MAX_NATIVE_EFFECTS {
        return Err(NativeEngineError::limit(
            "content-process storage events",
            MAX_NATIVE_EFFECTS,
            values.len(),
        ));
    }
    let events =
        serde_json::from_value::<Vec<NativeStorageEvent>>(value.clone()).map_err(|_| {
            NativeEngineError::Worker {
                operation: operation.into(),
                reason: "content process returned malformed storage events".into(),
            }
        })?;
    for event in &events {
        if !matches!(event.scope.as_str(), "local" | "session") {
            return Err(NativeEngineError::invalid(
                "content-process storage event scope",
                "must be local or session",
            ));
        }
        if event.storage_key.is_empty() || event.storage_key.len() > MAX_NATIVE_SCRIPT_BYTES {
            return Err(NativeEngineError::limit(
                "content-process storage event key",
                MAX_NATIVE_SCRIPT_BYTES,
                event.storage_key.len(),
            ));
        }
        validate_context_id(&event.source_context_id)?;
        if event.url.len() > MAX_NATIVE_SCRIPT_BYTES {
            return Err(NativeEngineError::limit(
                "content-process storage event URL",
                MAX_NATIVE_SCRIPT_BYTES,
                event.url.len(),
            ));
        }
        for (field, value) in [
            ("key", event.key.as_deref()),
            ("old value", event.old_value.as_deref()),
            ("new value", event.new_value.as_deref()),
        ] {
            if value.is_some_and(|value| value.len() > MAX_NATIVE_SCRIPT_BYTES) {
                return Err(NativeEngineError::limit(
                    format!("content-process storage event {field}"),
                    MAX_NATIVE_SCRIPT_BYTES,
                    value.map_or(0, str::len),
                ));
            }
        }
        if event.key.is_none() && event.new_value.is_some() {
            return Err(NativeEngineError::invalid(
                "content-process storage event",
                "a clear event must not contain a new value",
            ));
        }
    }
    Ok(events)
}

fn decode_document_wire(
    document_bytes: &[u8],
    operation: &str,
) -> Result<NativeDocumentWire, NativeEngineError> {
    if document_bytes.len() > MAX_CONTENT_DOCUMENT_WIRE_BYTES {
        return Err(NativeEngineError::limit(
            "content-process document snapshot",
            MAX_CONTENT_DOCUMENT_WIRE_BYTES,
            document_bytes.len(),
        ));
    }
    serde_json::from_slice(document_bytes).map_err(|_| NativeEngineError::Worker {
        operation: operation.into(),
        reason: "content process returned an invalid document snapshot".into(),
    })
}

#[doc(hidden)]
pub async fn run_native_content_worker() -> Result<(), NativeEngineError> {
    let mut stdin = tokio::io::stdin();
    let mut stdout = tokio::io::stdout();
    let mut running = false;
    let mut document = None;
    let mut document_url = None;
    let mut document_origin = None;
    let mut viewport = Viewport::default();
    let mut scroll_offset = NativePoint { x: 0, y: 0 };
    let mut nested_scroll_offsets = BTreeMap::new();
    let mut resource_loader = None;
    let mut javascript_runtime: Option<NativeJavaScriptRuntime> = None;
    let mut workers = NativeWorkerRegistry::new();
    let mut pending_worker_messages: VecDeque<NativeWorkerMessage> = VecDeque::new();
    let mut websocket_connections = BTreeMap::new();
    let mut worker_websocket_connections = BTreeMap::new();
    let mut fetch_stream_connections = BTreeMap::new();
    let mut event_source_connections = BTreeMap::new();
    let mut worker_event_source_connections = BTreeMap::new();
    let mut storage_state = NativeWebStorageState::default();
    let mut indexed_db_state = NativeIndexedDbState::default();
    let mut storage_profile_path: Option<PathBuf> = None;
    let mut storage_context_id = NATIVE_CONTEXT_ID.to_owned();
    let mut frame_id = NATIVE_CONTEXT_ID.to_owned();
    let mut window_name = String::new();
    let mut opener_context_id: Option<String> = None;
    let mut opener_window_name = String::new();
    let mut opener_url = String::new();
    let mut frame_script_context: Option<NativeFrameScriptContext> = None;
    let mut frame_script_bindings = Vec::new();
    loop {
        let payload = read_frame(&mut stdin).await?;
        let request: Value =
            serde_json::from_slice(&payload).map_err(|_| NativeEngineError::Worker {
                operation: "decode content IPC".into(),
                reason: "content process received an invalid request".into(),
            })?;
        let id = request.get("id").cloned().unwrap_or(Value::Null);
        let kind = request
            .get("kind")
            .and_then(Value::as_str)
            .unwrap_or_default();
        refresh_content_runtime_cookie(
            javascript_runtime.as_ref(),
            resource_loader.as_ref(),
            document_url.as_deref(),
        )?;
        if let (Some(runtime), Some(document_url), Some(document_origin)) = (
            javascript_runtime.as_ref(),
            document_url.as_deref(),
            document_origin.as_ref(),
        ) {
            runtime.set_indexed_db_state(
                indexed_db_state.origin(&storage_key(document_url, document_origin)),
            );
        }
        let mut response = match kind {
            "ping" if protocol_matches(&request) => {
                json!({"kind":"pong","id":id,"protocol":CONTENT_WORKER_PROTOCOL_VERSION})
            }
            "start" if protocol_matches(&request) && !running => {
                let requested_context_id = request
                    .get("context_id")
                    .and_then(Value::as_str)
                    .ok_or_else(|| {
                        NativeEngineError::invalid("content-process context id", "must be text")
                    })?;
                validate_context_id(requested_context_id)?;
                let requested_frame_id = request
                    .get("frame_id")
                    .and_then(Value::as_str)
                    .ok_or_else(|| {
                        NativeEngineError::invalid("content-process frame id", "must be text")
                    })?;
                validate_context_id(requested_frame_id)?;
                let requested_window_name = request
                    .get("window_name")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                validate_window_name(requested_window_name)?;
                let requested_opener_context_id =
                    request.get("opener_context_id").and_then(Value::as_str);
                if let Some(opener_context_id) = requested_opener_context_id {
                    validate_context_id(opener_context_id)?;
                }
                let requested_opener_window_name = request
                    .get("opener_window_name")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                validate_window_name(requested_opener_window_name)?;
                let requested_opener_url = request
                    .get("opener_url")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                if !requested_opener_url.is_empty() {
                    validate_url_text("content-process opener URL", requested_opener_url)?;
                }
                let requested_frame_context = decode_frame_script_context(
                    request.get("frame_context"),
                    "decode content process frame script context",
                )?;
                let requested_path = request
                    .get("storage_path")
                    .and_then(Value::as_str)
                    .map(PathBuf::from);
                let loaded_web_storage = requested_path
                    .as_deref()
                    .map(|path| load_web_storage_profile(Some(path)))
                    .transpose();
                let loaded_indexed_db = requested_path
                    .as_deref()
                    .map(|path| load_indexed_db_profile(Some(path)))
                    .transpose();
                match (loaded_web_storage, loaded_indexed_db) {
                    (Ok(loaded), Ok(loaded_indexed_db)) => {
                        if let Some(loaded) = loaded {
                            storage_state = loaded;
                        }
                        if let Some(loaded) = loaded_indexed_db {
                            indexed_db_state = loaded;
                        }
                        storage_profile_path = requested_path;
                        storage_context_id = requested_context_id.to_owned();
                        frame_id = requested_frame_id.to_owned();
                        window_name = requested_window_name.to_owned();
                        opener_context_id = requested_opener_context_id.map(str::to_owned);
                        opener_window_name = requested_opener_window_name.to_owned();
                        opener_url = requested_opener_url.to_owned();
                        frame_script_context = requested_frame_context;
                        scroll_offset = NativePoint { x: 0, y: 0 };
                        nested_scroll_offsets.clear();
                        running = true;
                        json!({"kind":"started","id":id})
                    }
                    (Err(error), _) | (_, Err(error)) => content_error_response(id, error),
                }
            }
            "commit" if protocol_matches(&request) && running => {
                json!({"kind":"committed","id":id})
            }
            "scroll_sync" if protocol_matches(&request) && running => {
                if request.get("x").is_some() || request.get("y").is_some() {
                    let x = request
                        .get("x")
                        .and_then(Value::as_u64)
                        .and_then(|value| u32::try_from(value).ok())
                        .ok_or_else(|| {
                            NativeEngineError::invalid(
                                "content-process scroll x",
                                "must be a non-negative integer",
                            )
                        })?;
                    let y = request
                        .get("y")
                        .and_then(Value::as_u64)
                        .and_then(|value| u32::try_from(value).ok())
                        .ok_or_else(|| {
                            NativeEngineError::invalid(
                                "content-process scroll y",
                                "must be a non-negative integer",
                            )
                        })?;
                    scroll_offset = NativePoint { x, y };
                }
                if let Some(value) = request.get("nested") {
                    let entries = value.as_array().ok_or_else(|| {
                        NativeEngineError::invalid(
                            "content-process nested scroll offsets",
                            "must be an array",
                        )
                    })?;
                    if entries.len() > MAX_NATIVE_NODES {
                        return Err(NativeEngineError::limit(
                            "content-process nested scroll offsets",
                            MAX_NATIVE_NODES,
                            entries.len(),
                        ));
                    }
                    let mut next_offsets = BTreeMap::new();
                    for entry in entries {
                        let node_index = entry
                            .get("node_index")
                            .and_then(Value::as_u64)
                            .and_then(|value| u32::try_from(value).ok())
                            .ok_or_else(|| {
                                NativeEngineError::invalid(
                                    "content-process nested scroll node",
                                    "must be a non-negative integer",
                                )
                            })?;
                        let x = entry
                            .get("x")
                            .and_then(Value::as_u64)
                            .and_then(|value| u32::try_from(value).ok())
                            .ok_or_else(|| {
                                NativeEngineError::invalid(
                                    "content-process nested scroll x",
                                    "must be a non-negative integer",
                                )
                            })?;
                        let y = entry
                            .get("y")
                            .and_then(Value::as_u64)
                            .and_then(|value| u32::try_from(value).ok())
                            .ok_or_else(|| {
                                NativeEngineError::invalid(
                                    "content-process nested scroll y",
                                    "must be a non-negative integer",
                                )
                            })?;
                        next_offsets.insert(node_index, NativePoint { x, y });
                    }
                    nested_scroll_offsets = next_offsets;
                }
                if request.get("x").is_none()
                    && request.get("y").is_none()
                    && request.get("nested").is_none()
                {
                    return Err(NativeEngineError::invalid(
                        "content-process scroll synchronization",
                        "must contain a root or nested scroll state",
                    ));
                }
                if let Some(runtime) = javascript_runtime.as_ref() {
                    runtime.set_scroll_offset(scroll_offset);
                    runtime.set_nested_scroll_offsets(nested_scroll_offsets.clone());
                }
                json!({"kind":"scroll_synced","id":id})
            }
            "history_sync" if protocol_matches(&request) && running => {
                let requested_url =
                    request.get("url").and_then(Value::as_str).ok_or_else(|| {
                        NativeEngineError::invalid("content-process history URL", "must be text")
                    })?;
                validate_url_text("content-process history URL", requested_url)?;
                let requested_state = request.get("state").ok_or_else(|| {
                    NativeEngineError::invalid("content-process history state", "must be present")
                })?;
                validate_content_history_state(requested_state)?;
                let length = request
                    .get("length")
                    .and_then(Value::as_u64)
                    .and_then(|value| usize::try_from(value).ok())
                    .ok_or_else(|| {
                        NativeEngineError::invalid(
                            "content-process history length",
                            "must be a positive integer",
                        )
                    })?;
                if length == 0 {
                    return Err(NativeEngineError::invalid(
                        "content-process history length",
                        "must be a positive integer",
                    ));
                }
                document_url = Some(requested_url.to_owned());
                if let Some(runtime) = javascript_runtime.as_ref() {
                    runtime.set_history_state(requested_state.clone());
                    runtime.set_history_length(length);
                }
                json!({"kind":"history_synced","id":id})
            }
            "cookies" if protocol_matches(&request) && running => {
                let requested_url = request
                    .get("document_url")
                    .and_then(Value::as_str)
                    .ok_or_else(|| {
                        NativeEngineError::invalid("content-process cookie URL", "must be text")
                    })?;
                let cookies = match resource_loader.as_ref() {
                    Some(loader) => loader.cookies_for_document(requested_url),
                    None => Err(NativeEngineError::Worker {
                        operation: "content process cookies".into(),
                        reason: "content process has no resource loader".into(),
                    }),
                }?;
                json!({"kind":"cookies","id":id,"cookies":cookies})
            }
            "set_cookies" if protocol_matches(&request) && running => {
                let values = request.get("cookies").ok_or_else(|| {
                    NativeEngineError::invalid("content-process cookies", "must be an array")
                })?;
                let cookies: Vec<NativeCookieProfileEntry> = serde_json::from_value(values.clone())
                    .map_err(|_| {
                        NativeEngineError::invalid(
                            "content-process cookies",
                            "must be valid native cookie profiles",
                        )
                    })?;
                let Some(loader) = resource_loader.as_mut() else {
                    return Err(NativeEngineError::Worker {
                        operation: "content process set cookies".into(),
                        reason: "content process has no resource loader".into(),
                    });
                };
                loader.set_cookie_profiles(&cookies)?;
                refresh_content_runtime_cookie(
                    javascript_runtime.as_ref(),
                    resource_loader.as_ref(),
                    document_url.as_deref(),
                )?;
                json!({"kind":"cookies_set","id":id})
            }
            "clear_cookies" if protocol_matches(&request) && running => {
                let Some(loader) = resource_loader.as_mut() else {
                    return Err(NativeEngineError::Worker {
                        operation: "content process clear cookies".into(),
                        reason: "content process has no resource loader".into(),
                    });
                };
                loader.clear_cookies();
                refresh_content_runtime_cookie(
                    javascript_runtime.as_ref(),
                    resource_loader.as_ref(),
                    document_url.as_deref(),
                )?;
                json!({"kind":"cookies_cleared","id":id})
            }
            "storage_events" if protocol_matches(&request) && running => {
                let values = request.get("events").ok_or_else(|| {
                    NativeEngineError::invalid("content-process storage events", "must be an array")
                })?;
                let events =
                    decode_storage_event_value(values, "decode content process storage events")?;
                for event in &events {
                    storage_state.apply_storage_event(event)?;
                }
                if let Some(runtime) = javascript_runtime.as_ref() {
                    runtime.set_storage_state(storage_state.clone());
                    runtime.set_storage_events(events)?;
                }
                json!({"kind":"storage_events_synced","id":id})
            }
            "window_proxy_sync" if protocol_matches(&request) && running => {
                let updates = decode_window_proxy_updates(
                    request.get("updates"),
                    "decode content process WindowProxy updates",
                )?;
                if let Some(runtime) = javascript_runtime.as_ref() {
                    runtime.sync_window_proxies(&updates)?;
                }
                json!({"kind":"window_proxy_synced","id":id})
            }
            "frame_script_sync" if protocol_matches(&request) && running => {
                let bindings = decode_frame_script_bindings(
                    request.get("bindings"),
                    "decode content process frame script bindings",
                )?;
                frame_script_bindings = bindings;
                if let Some(runtime) = javascript_runtime.as_ref() {
                    runtime.set_frame_script_bindings(frame_script_bindings.clone());
                }
                json!({"kind":"frame_script_synced","id":id})
            }
            "frame_script_context_sync" if protocol_matches(&request) && running => {
                frame_script_context = decode_frame_script_context(
                    request.get("context"),
                    "decode content process frame script context",
                )?;
                if let Some(runtime) = javascript_runtime.as_ref() {
                    runtime.set_frame_script_context(frame_script_context.clone());
                }
                json!({"kind":"frame_script_context_synced","id":id})
            }
            "storage_state" if protocol_matches(&request) && running => {
                let value = request.get("state").ok_or_else(|| {
                    NativeEngineError::invalid("content-process storage state", "must be an object")
                })?;
                let next_state: NativeWebStorageState = serde_json::from_value(value.clone())
                    .map_err(|_| {
                        NativeEngineError::invalid(
                            "content-process storage state",
                            "must be a valid native Web Storage state",
                        )
                    })?;
                next_state.validate()?;
                storage_state = next_state;
                if let Some(value) = request.get("indexed_db_state") {
                    let next_indexed_db_state: NativeIndexedDbState =
                        serde_json::from_value(value.clone()).map_err(|_| {
                            NativeEngineError::invalid(
                                "content-process IndexedDB state",
                                "must be a valid native IndexedDB state",
                            )
                        })?;
                    next_indexed_db_state.validate()?;
                    indexed_db_state = next_indexed_db_state;
                }
                if let Some(runtime) = javascript_runtime.as_ref() {
                    runtime.replace_storage_state(storage_state.clone());
                    if let (Some(document_url), Some(document_origin)) =
                        (document_url.as_deref(), document_origin.as_ref())
                    {
                        runtime.set_indexed_db_state(
                            indexed_db_state.origin(&storage_key(document_url, document_origin)),
                        );
                    }
                }
                json!({"kind":"storage_state_synced","id":id})
            }
            "load" if protocol_matches(&request) && running => {
                frame_script_bindings.clear();
                websocket_connections.clear();
                worker_websocket_connections.clear();
                fetch_stream_connections.clear();
                event_source_connections.clear();
                worker_event_source_connections.clear();
                workers.clear();
                pending_worker_messages.clear();
                if let Some(runtime) = javascript_runtime.as_ref() {
                    storage_state = runtime.storage_state();
                }
                match load_content_resource(
                    &request,
                    &mut resource_loader,
                    storage_profile_path.as_deref(),
                )
                .await
                {
                    Ok((
                        resource,
                        mut parsed,
                        loaded_viewport,
                        script_sources,
                        resource_events,
                    )) => {
                        let mut script_runtime =
                            match NativeJavaScriptRuntime::new_with_context_metadata(
                                &storage_context_id,
                                &window_name,
                                opener_context_id.as_deref(),
                                &opener_window_name,
                                &opener_url,
                            ) {
                                Ok(runtime) => Some(runtime),
                                Err(error) => {
                                    let response = content_error_response(id, error);
                                    write_value_frame(&mut stdout, &response).await?;
                                    continue;
                                }
                            };
                        if let Some(runtime) = script_runtime.as_ref() {
                            runtime.set_frame_id(frame_id.clone());
                            runtime.set_frame_script_context(frame_script_context.clone());
                            runtime.set_frame_script_bindings(frame_script_bindings.clone());
                            runtime.set_scroll_offset(scroll_offset);
                            runtime.set_nested_scroll_offsets(nested_scroll_offsets.clone());
                        }
                        let document_cookie = resource_loader
                            .as_ref()
                            .map(|loader| loader.document_cookie(&resource.url))
                            .transpose()?
                            .unwrap_or_default();
                        let page_scripts = execute_page_scripts(
                            &mut parsed,
                            &mut script_runtime,
                            &storage_context_id,
                            &script_sources,
                            &resource.url,
                            &resource.origin,
                            loaded_viewport,
                            &storage_state,
                            &indexed_db_state,
                            &document_cookie,
                            &resource_events,
                        );
                        let prepared = match page_scripts {
                            Ok(page_scripts) if page_scripts.navigation.is_some() => {
                                let dialogs = page_scripts.dialogs;
                                let events = page_scripts.events;
                                let scroll_commands = page_scripts.scroll_commands;
                                let navigation = page_scripts.navigation.map(|navigation| {
                                    NativeContentNavigation {
                                        node_index: 0,
                                        href: navigation.href,
                                        submitter_node_index: None,
                                        location: true,
                                        replace_history: navigation.replace_history,
                                    }
                                });
                                Ok((parsed, navigation, dialogs, events, scroll_commands))
                            }
                            Ok(page_scripts)
                                if page_scripts.pending_fetches.is_empty()
                                    && page_scripts.websocket_commands.is_empty()
                                    && page_scripts.event_source_commands.is_empty() =>
                            {
                                Ok((
                                    parsed,
                                    None,
                                    page_scripts.dialogs,
                                    page_scripts.events,
                                    page_scripts.scroll_commands,
                                ))
                            }
                            Ok(page_scripts) => {
                                let page_dialogs = page_scripts.dialogs;
                                let mut page_events = page_scripts.events;
                                let mut page_scroll_commands = page_scripts.scroll_commands;
                                match (script_runtime.as_ref(), resource_loader.as_mut()) {
                                    (Some(runtime), Some(loader)) => {
                                        match resolve_script_fetches(
                                            &parsed,
                                            runtime,
                                            Some(loader),
                                            &mut websocket_connections,
                                            &mut fetch_stream_connections,
                                            &mut event_source_connections,
                                            &resource.url,
                                            &resource.origin,
                                            loaded_viewport,
                                            false,
                                            NativeScriptEvaluation {
                                                value: Value::Null,
                                                commands: page_scripts
                                                    .pending_fetches
                                                    .into_iter()
                                                    .chain(page_scripts.websocket_commands)
                                                    .chain(page_scripts.event_source_commands)
                                                    .collect(),
                                                top_level_await_pending: false,
                                            },
                                        )
                                        .await
                                        {
                                            Ok((next, mutation, _resolved_value)) => {
                                                page_events.extend(
                                                    mutation.events.into_iter().map(|event| {
                                                        (event.node_index, event.kind)
                                                    }),
                                                );
                                                page_scroll_commands
                                                    .extend(mutation.scroll_commands);
                                                Ok((
                                                    next,
                                                    mutation.navigation,
                                                    page_dialogs,
                                                    page_events,
                                                    page_scroll_commands,
                                                ))
                                            }
                                            Err(error) => Err(error),
                                        }
                                    }
                                    _ => Err(NativeEngineError::Worker {
                                        operation: "page-load fetch scheduling".into(),
                                        reason: "content process fetch state is unavailable".into(),
                                    }),
                                }
                            }
                            Err(error) => Err(error),
                        };
                        match prepared {
                            Ok((parsed, navigation, dialogs, events, scroll_commands)) => {
                                if let Some(runtime) = script_runtime.as_ref() {
                                    storage_state = runtime.storage_state();
                                    indexed_db_state.replace_origin(
                                        storage_key(&resource.url, &resource.origin),
                                        runtime.indexed_db_state(),
                                    )?;
                                    let worker_commands = runtime.take_worker_commands();
                                    let Some(loader) = resource_loader.as_mut() else {
                                        return Err(NativeEngineError::Worker {
                                            operation: "page-load Worker scheduling".into(),
                                            reason:
                                                "content process resource loader is unavailable"
                                                    .into(),
                                        });
                                    };
                                    workers
                                        .apply_commands(worker_commands, loader, &resource.url)
                                        .await?;
                                    process_worker_websocket_commands(
                                        workers.take_websocket_commands(),
                                        &mut worker_websocket_connections,
                                        Some(&*loader),
                                    )?;
                                    process_worker_event_source_commands(
                                        workers.take_event_source_commands(),
                                        &mut worker_event_source_connections,
                                        Some(&*loader),
                                    )?;
                                    pending_worker_messages.extend(workers.take_messages());
                                }
                                let document_wire = parsed.to_content_wire();
                                document = Some(parsed);
                                document_url = Some(resource.url.clone());
                                document_origin = Some(resource.origin.clone());
                                viewport = loaded_viewport;
                                javascript_runtime = script_runtime;
                                json!({
                                    "kind": "loaded",
                                    "id": id,
                                    "url": resource.url,
                                    "navigation": navigation.as_ref().map(|navigation| json!({
                                        "node_index": navigation.node_index,
                                        "href": navigation.href,
                                        "submitter_node_index": navigation.submitter_node_index,
                                        "location": navigation.location,
                                        "replace_history": navigation.replace_history,
                                    })),
                                    "dialogs": dialogs,
                                    "events": events.iter().map(|(node_index, kind)| json!({
                                        "node_index": node_index,
                                        "kind": event_kind_text(*kind),
                                    })).collect::<Vec<_>>(),
                                    "scroll_commands": scroll_commands,
                                    "frame_sources": resource.frame_sources,
                                    "document_base64": base64::engine::general_purpose::STANDARD
                                        .encode(serde_json::to_vec(&document_wire).unwrap_or_default()),
                                })
                            }
                            Err(error) => content_error_response(id, error),
                        }
                    }
                    Err(error) => content_error_response(id, error),
                }
            }
            "fetch" if protocol_matches(&request) && running => {
                let Some(_current) = document.as_ref() else {
                    let response = content_error_response(
                        id,
                        NativeEngineError::Worker {
                            operation: "content process fetch".into(),
                            reason: "content process has no committed document".into(),
                        },
                    );
                    write_value_frame(&mut stdout, &response).await?;
                    continue;
                };
                let document_url = request
                    .get("document_url")
                    .and_then(Value::as_str)
                    .ok_or_else(|| {
                        NativeEngineError::invalid(
                            "content-process fetch owner URL",
                            "must be text",
                        )
                    })?;
                let href = request.get("href").and_then(Value::as_str).ok_or_else(|| {
                    NativeEngineError::invalid("content-process fetch URL", "must be text")
                })?;
                let credentials = request
                    .get("credentials")
                    .and_then(Value::as_bool)
                    .unwrap_or(false);
                let Some(loader) = resource_loader.as_mut() else {
                    let response = content_error_response(
                        id,
                        NativeEngineError::Worker {
                            operation: "content process fetch".into(),
                            reason: "content process has no resource loader".into(),
                        },
                    );
                    write_value_frame(&mut stdout, &response).await?;
                    continue;
                };
                match loader.fetch_async(document_url, href, credentials).await {
                    Ok(fetch) => {
                        json!({
                            "kind": "fetched",
                            "id": id,
                            "url": fetch.url,
                            "status": fetch.status,
                            "content_type": fetch.content_type,
                            "headers": fetch.headers,
                            "redirected": fetch.redirected,
                            "opaque": fetch.opaque,
                            "opaque_redirect": fetch.opaque_redirect,
                            "body_base64": base64::engine::general_purpose::STANDARD
                                .encode(fetch.body),
                        })
                    }
                    Err(error) => content_error_response(id, error),
                }
            }
            "script" if protocol_matches(&request) && running => {
                let Some(current) = document.as_ref() else {
                    let response = content_error_response(
                        id,
                        NativeEngineError::Worker {
                            operation: "content process script".into(),
                            reason: "content process has no committed document".into(),
                        },
                    );
                    write_value_frame(&mut stdout, &response).await?;
                    continue;
                };
                let source = request
                    .get("source")
                    .and_then(Value::as_str)
                    .ok_or_else(|| {
                        NativeEngineError::invalid("content-process script source", "must be text")
                    })?;
                if javascript_runtime.is_none() {
                    match NativeJavaScriptRuntime::new_with_context_metadata(
                        &storage_context_id,
                        &window_name,
                        opener_context_id.as_deref(),
                        &opener_window_name,
                        &opener_url,
                    ) {
                        Ok(runtime) => {
                            runtime.set_storage_state(storage_state.clone());
                            runtime.set_frame_id(frame_id.clone());
                            runtime.set_frame_script_context(frame_script_context.clone());
                            runtime.set_frame_script_bindings(frame_script_bindings.clone());
                            runtime.set_scroll_offset(scroll_offset);
                            runtime.set_nested_scroll_offsets(nested_scroll_offsets.clone());
                            javascript_runtime = Some(runtime);
                        }
                        Err(error) => {
                            let response = content_error_response(id, error);
                            write_value_frame(&mut stdout, &response).await?;
                            continue;
                        }
                    }
                }
                let runtime = javascript_runtime.as_ref().expect("runtime initialized");
                runtime.set_scroll_offset(scroll_offset);
                runtime.set_nested_scroll_offsets(nested_scroll_offsets.clone());
                let Some(committed_url) = document_url.clone() else {
                    let response = content_error_response(
                        id,
                        NativeEngineError::Worker {
                            operation: "content process script".into(),
                            reason: "content process has no committed URL".into(),
                        },
                    );
                    write_value_frame(&mut stdout, &response).await?;
                    continue;
                };
                let Some(document_origin) = document_origin.as_ref() else {
                    let response = content_error_response(
                        id,
                        NativeEngineError::Worker {
                            operation: "content process script".into(),
                            reason: "content process has no committed origin".into(),
                        },
                    );
                    write_value_frame(&mut stdout, &response).await?;
                    continue;
                };
                runtime.set_indexed_db_state(
                    indexed_db_state.origin(&storage_key(&committed_url, document_origin)),
                );
                let Some(loader) = resource_loader.as_mut() else {
                    return Err(NativeEngineError::Worker {
                        operation: "content process Worker scheduling".into(),
                        reason: "content process resource loader is unavailable".into(),
                    });
                };
                workers.run_due_timers(loader).await?;
                process_worker_websocket_commands(
                    workers.take_websocket_commands(),
                    &mut worker_websocket_connections,
                    Some(&*loader),
                )?;
                process_worker_event_source_commands(
                    workers.take_event_source_commands(),
                    &mut worker_event_source_connections,
                    Some(&*loader),
                )?;
                pump_worker_websocket_event(
                    &mut workers,
                    &mut worker_websocket_connections,
                    loader,
                )
                .await?;
                pump_worker_event_source_event(
                    &mut workers,
                    &mut worker_event_source_connections,
                    loader,
                )
                .await?;
                pending_worker_messages.extend(workers.take_messages());
                let worker_messages = std::mem::take(&mut pending_worker_messages)
                    .into_iter()
                    .collect::<Vec<_>>();
                let source = match worker_message_script(&worker_messages)? {
                    Some(prefix) => format!("{prefix}{source}"),
                    None => source.to_owned(),
                };
                let evaluation =
                    runtime.evaluate(&source, current, &committed_url, document_origin, viewport);
                let Some(loader) = resource_loader.as_mut() else {
                    return Err(NativeEngineError::Worker {
                        operation: "content process Worker scheduling".into(),
                        reason: "content process resource loader is unavailable".into(),
                    });
                };
                let worker_commands = runtime.take_worker_commands();
                workers
                    .apply_commands(worker_commands, loader, &committed_url)
                    .await?;
                process_worker_websocket_commands(
                    workers.take_websocket_commands(),
                    &mut worker_websocket_connections,
                    Some(&*loader),
                )?;
                process_worker_event_source_commands(
                    workers.take_event_source_commands(),
                    &mut worker_event_source_connections,
                    Some(&*loader),
                )?;
                pending_worker_messages.extend(workers.take_messages());
                match evaluation {
                    Ok(NativeScriptEvaluation {
                        value,
                        commands,
                        top_level_await_pending,
                    }) if commands.is_empty()
                        && !top_level_await_pending
                        && websocket_connections.is_empty()
                        && event_source_connections.is_empty() =>
                    {
                        let frame_scripts = runtime.take_frame_script_events();
                        json!({
                            "kind":"evaluated",
                            "id":id,
                            "value":value,
                            "history": [],
                            "frame_scripts":frame_scripts,
                        })
                    }
                    Ok(NativeScriptEvaluation {
                        value,
                        commands,
                        top_level_await_pending,
                    }) => {
                        let history = extract_history_commands(&commands);
                        if let Err(error) = apply_content_runtime_history(
                            &history,
                            &mut document_url,
                            document_origin,
                            runtime,
                        ) {
                            let response = content_error_response(id, error);
                            write_value_frame(&mut stdout, &response).await?;
                            continue;
                        }
                        let script_url = document_url.clone().unwrap_or(committed_url.clone());
                        let has_websocket = commands.iter().any(|command| {
                            matches!(
                                command,
                                NativeScriptCommand::WebSocketOpen { .. }
                                    | NativeScriptCommand::WebSocketSend { .. }
                                    | NativeScriptCommand::WebSocketClose { .. }
                            )
                        });
                        let has_event_source = commands.iter().any(|command| {
                            matches!(
                                command,
                                NativeScriptCommand::EventSourceOpen { .. }
                                    | NativeScriptCommand::EventSourceClose { .. }
                            )
                        });
                        let result = resolve_script_fetches(
                            current,
                            runtime,
                            resource_loader.as_mut(),
                            &mut websocket_connections,
                            &mut fetch_stream_connections,
                            &mut event_source_connections,
                            &script_url,
                            document_origin,
                            viewport,
                            top_level_await_pending || has_websocket || has_event_source,
                            NativeScriptEvaluation {
                                value: value.clone(),
                                commands,
                                top_level_await_pending,
                            },
                        )
                        .await;
                        match result {
                            Ok((next, mutation, resolved_value)) => {
                                let value = resolved_value.unwrap_or(value);
                                let Some(loader) = resource_loader.as_mut() else {
                                    let response = content_error_response(
                                        id,
                                        NativeEngineError::Worker {
                                            operation: "dynamic Worker scheduling".into(),
                                            reason:
                                                "content process resource loader is unavailable"
                                                    .into(),
                                        },
                                    );
                                    write_value_frame(&mut stdout, &response).await?;
                                    continue;
                                };
                                let dynamic_worker_commands = runtime.take_worker_commands();
                                workers
                                    .apply_commands(
                                        dynamic_worker_commands,
                                        loader,
                                        &document_url.clone().unwrap_or(committed_url.clone()),
                                    )
                                    .await?;
                                process_worker_websocket_commands(
                                    workers.take_websocket_commands(),
                                    &mut worker_websocket_connections,
                                    Some(&*loader),
                                )?;
                                process_worker_event_source_commands(
                                    workers.take_event_source_commands(),
                                    &mut worker_event_source_connections,
                                    Some(&*loader),
                                )?;
                                pending_worker_messages.extend(workers.take_messages());
                                if !mutation.history.is_empty() {
                                    let Some(base_url) = document_url.as_deref() else {
                                        let response = content_error_response(
                                            id,
                                            NativeEngineError::Worker {
                                                operation: "content process script history".into(),
                                                reason: "history mutation lost its document URL"
                                                    .into(),
                                            },
                                        );
                                        write_value_frame(&mut stdout, &response).await?;
                                        continue;
                                    };
                                    match resolve_content_history_document_url(
                                        &mutation.history,
                                        base_url,
                                        document_origin,
                                    ) {
                                        Ok(url) => document_url = Some(url),
                                        Err(error) => {
                                            let response = content_error_response(id, error);
                                            write_value_frame(&mut stdout, &response).await?;
                                            continue;
                                        }
                                    }
                                }
                                document = Some(next);
                                let frame_scripts = runtime.take_frame_script_events();
                                json!({
                                    "kind": "evaluated",
                                    "id": id,
                                    "value": value,
                                    "history": history,
                                    "mutation_history": mutation.history,
                                    "scroll_commands": mutation.scroll_commands,
                                    "frame_scripts": frame_scripts,
                                    "document_base64": base64::engine::general_purpose::STANDARD
                                        .encode(serde_json::to_vec(&mutation.document).unwrap_or_default()),
                                    "events": mutation.events.iter().map(|event| json!({
                                        "node_index": event.node_index,
                                        "kind": event_kind_text(event.kind),
                                    })).collect::<Vec<_>>(),
                                    "dialogs": mutation.dialogs,
                                    "navigation": mutation.navigation.as_ref().map(|navigation| json!({
                                        "node_index": navigation.node_index,
                                        "href": navigation.href,
                                        "submitter_node_index": navigation.submitter_node_index,
                                        "location": navigation.location,
                                        "replace_history": navigation.replace_history,
                                    })),
                                })
                            }
                            Err(error) => content_error_response(id, error),
                        }
                    }
                    Err(error) => content_error_response(id, error),
                }
            }
            "mutate_click_preflight" if protocol_matches(&request) && running => {
                let Some(current) = document.as_ref() else {
                    let response = content_error_response(
                        id,
                        NativeEngineError::Worker {
                            operation: "content process click preflight".into(),
                            reason: "content process has no committed document".into(),
                        },
                    );
                    write_value_frame(&mut stdout, &response).await?;
                    continue;
                };
                let node_index = request
                    .get("action")
                    .and_then(|action| action.get("node_index"))
                    .and_then(Value::as_u64)
                    .and_then(|value| u32::try_from(value).ok())
                    .ok_or_else(|| {
                        NativeEngineError::invalid(
                            "content-process click target",
                            "must be a uint32",
                        )
                    })?;
                let Some(mut committed_url) = document_url.clone() else {
                    let response = content_error_response(
                        id,
                        NativeEngineError::Worker {
                            operation: "content process click preflight".into(),
                            reason: "content process has no committed URL".into(),
                        },
                    );
                    write_value_frame(&mut stdout, &response).await?;
                    continue;
                };
                let Some(document_origin) = document_origin.as_ref() else {
                    let response = content_error_response(
                        id,
                        NativeEngineError::Worker {
                            operation: "content process click preflight".into(),
                            reason: "content process has no committed origin".into(),
                        },
                    );
                    write_value_frame(&mut stdout, &response).await?;
                    continue;
                };
                if javascript_runtime.is_none() {
                    match NativeJavaScriptRuntime::new_with_context_metadata(
                        &storage_context_id,
                        &window_name,
                        opener_context_id.as_deref(),
                        &opener_window_name,
                        &opener_url,
                    ) {
                        Ok(runtime) => {
                            runtime.set_storage_state(storage_state.clone());
                            runtime.set_frame_id(frame_id.clone());
                            runtime.set_frame_script_context(frame_script_context.clone());
                            javascript_runtime = Some(runtime);
                        }
                        Err(error) => {
                            let response = content_error_response(id, error);
                            write_value_frame(&mut stdout, &response).await?;
                            continue;
                        }
                    }
                }
                let runtime = javascript_runtime.as_ref().expect("runtime initialized");
                runtime.set_indexed_db_state(
                    indexed_db_state.origin(&storage_key(&committed_url, document_origin)),
                );
                match mutate_click_with_event_preflight(
                    current,
                    runtime,
                    &mut committed_url,
                    document_origin,
                    viewport,
                    node_index,
                ) {
                    Ok((next, mutation)) => {
                        document = Some(next);
                        document_url = Some(committed_url);
                        json!({
                            "kind": "mutated",
                            "id": id,
                            "allowed": mutation.allowed,
                            "mutation_history": mutation.history,
                            "scroll_commands": mutation.scroll_commands,
                            "document_base64": base64::engine::general_purpose::STANDARD
                                .encode(serde_json::to_vec(&mutation.document).unwrap_or_default()),
                            "events": mutation.events.iter().map(|event| json!({
                                "node_index": event.node_index,
                                "kind": event_kind_text(event.kind),
                            })).collect::<Vec<_>>(),
                            "navigation": mutation.navigation.as_ref().map(|navigation| json!({
                            "node_index": navigation.node_index,
                            "href": navigation.href,
                            "submitter_node_index": navigation.submitter_node_index,
                            "location": navigation.location,
                            "replace_history": navigation.replace_history,
                        })),
                        })
                    }
                    Err(error) => content_error_response(id, error),
                }
            }
            "mutate_type_events" if protocol_matches(&request) && running => {
                let Some(current) = document.as_ref() else {
                    let response = content_error_response(
                        id,
                        NativeEngineError::Worker {
                            operation: "content process type event bridge".into(),
                            reason: "content process has no committed document".into(),
                        },
                    );
                    write_value_frame(&mut stdout, &response).await?;
                    continue;
                };
                let action = request.get("action").ok_or_else(|| {
                    NativeEngineError::invalid("content-process type action", "is required")
                })?;
                let node_index = action
                    .get("node_index")
                    .and_then(Value::as_u64)
                    .and_then(|value| u32::try_from(value).ok())
                    .ok_or_else(|| {
                        NativeEngineError::invalid(
                            "content-process type target",
                            "must be a uint32",
                        )
                    })?;
                let text = action.get("text").and_then(Value::as_str).ok_or_else(|| {
                    NativeEngineError::invalid("content-process type text", "must be text")
                })?;
                let Some(mut committed_url) = document_url.clone() else {
                    let response = content_error_response(
                        id,
                        NativeEngineError::Worker {
                            operation: "content process type event bridge".into(),
                            reason: "content process has no committed URL".into(),
                        },
                    );
                    write_value_frame(&mut stdout, &response).await?;
                    continue;
                };
                let Some(document_origin) = document_origin.as_ref() else {
                    let response = content_error_response(
                        id,
                        NativeEngineError::Worker {
                            operation: "content process type event bridge".into(),
                            reason: "content process has no committed origin".into(),
                        },
                    );
                    write_value_frame(&mut stdout, &response).await?;
                    continue;
                };
                if javascript_runtime.is_none() {
                    match NativeJavaScriptRuntime::new_with_context_metadata(
                        &storage_context_id,
                        &window_name,
                        opener_context_id.as_deref(),
                        &opener_window_name,
                        &opener_url,
                    ) {
                        Ok(runtime) => {
                            runtime.set_storage_state(storage_state.clone());
                            runtime.set_frame_id(frame_id.clone());
                            runtime.set_frame_script_context(frame_script_context.clone());
                            javascript_runtime = Some(runtime);
                        }
                        Err(error) => {
                            let response = content_error_response(id, error);
                            write_value_frame(&mut stdout, &response).await?;
                            continue;
                        }
                    }
                }
                let runtime = javascript_runtime.as_ref().expect("runtime initialized");
                runtime.set_indexed_db_state(
                    indexed_db_state.origin(&storage_key(&committed_url, document_origin)),
                );
                match mutate_type_with_event_bridge(
                    current,
                    runtime,
                    &mut committed_url,
                    document_origin,
                    viewport,
                    node_index,
                    text,
                ) {
                    Ok((next, mutation)) => {
                        document = Some(next);
                        document_url = Some(committed_url);
                        json!({
                            "kind": "mutated",
                            "id": id,
                            "mutation_history": mutation.history,
                            "scroll_commands": mutation.scroll_commands,
                            "document_base64": base64::engine::general_purpose::STANDARD
                                .encode(serde_json::to_vec(&mutation.document).unwrap_or_default()),
                            "events": mutation.events.iter().map(|event| json!({
                                "node_index": event.node_index,
                                "kind": event_kind_text(event.kind),
                            })).collect::<Vec<_>>(),
                        })
                    }
                    Err(error) => content_error_response(id, error),
                }
            }
            "mutate_form_events" if protocol_matches(&request) && running => {
                let Some(current) = document.as_ref() else {
                    let response = content_error_response(
                        id,
                        NativeEngineError::Worker {
                            operation: "content process form action".into(),
                            reason: "content process has no committed document".into(),
                        },
                    );
                    write_value_frame(&mut stdout, &response).await?;
                    continue;
                };
                let action = request.get("action").ok_or_else(|| {
                    NativeEngineError::invalid("content-process form action", "is required")
                })?;
                let node_index = action
                    .get("node_index")
                    .and_then(Value::as_u64)
                    .and_then(|value| u32::try_from(value).ok())
                    .ok_or_else(|| {
                        NativeEngineError::invalid(
                            "content-process form target",
                            "must be a uint32",
                        )
                    })?;
                let action_kind = action.get("kind").and_then(Value::as_str);
                let destination_node_index = action
                    .get("destination_node_index")
                    .and_then(Value::as_u64)
                    .and_then(|value| u32::try_from(value).ok());
                let form_action = match action_kind {
                    Some("clear") => NativeFormAction::Clear,
                    Some("select") => NativeFormAction::Select(
                        action
                            .get("value")
                            .and_then(Value::as_str)
                            .ok_or_else(|| {
                                NativeEngineError::invalid(
                                    "content-process select value",
                                    "must be text",
                                )
                            })?
                            .to_owned(),
                    ),
                    Some("hover") => NativeFormAction::Hover,
                    Some("drag") => NativeFormAction::Drag {
                        destination_node_index: destination_node_index.ok_or_else(|| {
                            NativeEngineError::invalid(
                                "content-process drag destination",
                                "must be a uint32",
                            )
                        })?,
                    },
                    Some("upload") => {
                        let files = serde_json::from_value::<Vec<NativeFile>>(
                            action.get("files").cloned().ok_or_else(|| {
                                NativeEngineError::invalid(
                                    "content-process upload files",
                                    "must be present",
                                )
                            })?,
                        )
                        .map_err(|_| {
                            NativeEngineError::invalid(
                                "content-process upload files",
                                "must be a valid native file list",
                            )
                        })?;
                        NativeFile::validate_many(&files)?;
                        NativeFormAction::Upload(files)
                    }
                    _ => {
                        let response = content_error_response(
                            id,
                            NativeEngineError::invalid(
                                "content-process form action",
                                "kind must be clear, select, hover, drag, or upload",
                            ),
                        );
                        write_value_frame(&mut stdout, &response).await?;
                        continue;
                    }
                };
                let Some(mut committed_url) = document_url.clone() else {
                    let response = content_error_response(
                        id,
                        NativeEngineError::Worker {
                            operation: "content process form action".into(),
                            reason: "content process has no committed URL".into(),
                        },
                    );
                    write_value_frame(&mut stdout, &response).await?;
                    continue;
                };
                let Some(document_origin) = document_origin.as_ref() else {
                    let response = content_error_response(
                        id,
                        NativeEngineError::Worker {
                            operation: "content process form action".into(),
                            reason: "content process has no committed origin".into(),
                        },
                    );
                    write_value_frame(&mut stdout, &response).await?;
                    continue;
                };
                if javascript_runtime.is_none() {
                    match NativeJavaScriptRuntime::new_with_context_metadata(
                        &storage_context_id,
                        &window_name,
                        opener_context_id.as_deref(),
                        &opener_window_name,
                        &opener_url,
                    ) {
                        Ok(runtime) => {
                            runtime.set_storage_state(storage_state.clone());
                            runtime.set_frame_id(frame_id.clone());
                            runtime.set_frame_script_context(frame_script_context.clone());
                            javascript_runtime = Some(runtime);
                        }
                        Err(error) => {
                            let response = content_error_response(id, error);
                            write_value_frame(&mut stdout, &response).await?;
                            continue;
                        }
                    }
                }
                let runtime = javascript_runtime.as_ref().expect("runtime initialized");
                runtime.set_indexed_db_state(
                    indexed_db_state.origin(&storage_key(&committed_url, document_origin)),
                );
                match mutate_form_action_with_event_bridge(
                    current,
                    runtime,
                    &mut committed_url,
                    document_origin,
                    viewport,
                    node_index,
                    form_action,
                ) {
                    Ok((next, mutation)) => {
                        document = Some(next);
                        document_url = Some(committed_url);
                        json!({
                            "kind": "mutated",
                            "id": id,
                            "mutation_history": mutation.history,
                            "scroll_commands": mutation.scroll_commands,
                            "document_base64": base64::engine::general_purpose::STANDARD
                                .encode(serde_json::to_vec(&mutation.document).unwrap_or_default()),
                            "events": mutation.events.iter().map(|event| json!({
                                "node_index": event.node_index,
                                "kind": event_kind_text(event.kind),
                            })).collect::<Vec<_>>(),
                        })
                    }
                    Err(error) => content_error_response(id, error),
                }
            }
            "mutate_key_events" | "mutate_key_event" | "mutate_key_shortcut"
                if protocol_matches(&request) && running =>
            {
                let Some(current) = document.as_ref() else {
                    let response = content_error_response(
                        id,
                        NativeEngineError::Worker {
                            operation: "content process key event bridge".into(),
                            reason: "content process has no committed document".into(),
                        },
                    );
                    write_value_frame(&mut stdout, &response).await?;
                    continue;
                };
                let action = request.get("action").ok_or_else(|| {
                    NativeEngineError::invalid("content-process key action", "is required")
                })?;
                let node_index = action
                    .get("node_index")
                    .and_then(Value::as_u64)
                    .and_then(|value| u32::try_from(value).ok())
                    .ok_or_else(|| {
                        NativeEngineError::invalid("content-process key target", "must be a uint32")
                    })?;
                let key = action.get("key").and_then(Value::as_str).ok_or_else(|| {
                    NativeEngineError::invalid("content-process key", "must be text")
                })?;
                let request_kind = request.get("kind").and_then(Value::as_str).unwrap_or("");
                let action_kind = action.get("kind").and_then(Value::as_str).unwrap_or("");
                let is_legacy_key_press = request_kind == "mutate_key_events";
                let key_validation = if is_legacy_key_press {
                    validate_native_edit_key(key)
                } else {
                    validate_native_key(key)
                };
                if let Err(error) = key_validation {
                    let response = content_error_response(id, error);
                    write_value_frame(&mut stdout, &response).await?;
                    continue;
                }
                let Some(mut committed_url) = document_url.clone() else {
                    let response = content_error_response(
                        id,
                        NativeEngineError::Worker {
                            operation: "content process key event bridge".into(),
                            reason: "content process has no committed URL".into(),
                        },
                    );
                    write_value_frame(&mut stdout, &response).await?;
                    continue;
                };
                let Some(document_origin) = document_origin.as_ref() else {
                    let response = content_error_response(
                        id,
                        NativeEngineError::Worker {
                            operation: "content process key event bridge".into(),
                            reason: "content process has no committed origin".into(),
                        },
                    );
                    write_value_frame(&mut stdout, &response).await?;
                    continue;
                };
                if javascript_runtime.is_none() {
                    match NativeJavaScriptRuntime::new_with_context_metadata(
                        &storage_context_id,
                        &window_name,
                        opener_context_id.as_deref(),
                        &opener_window_name,
                        &opener_url,
                    ) {
                        Ok(runtime) => {
                            runtime.set_storage_state(storage_state.clone());
                            runtime.set_frame_id(frame_id.clone());
                            runtime.set_frame_script_context(frame_script_context.clone());
                            javascript_runtime = Some(runtime);
                        }
                        Err(error) => {
                            let response = content_error_response(id, error);
                            write_value_frame(&mut stdout, &response).await?;
                            continue;
                        }
                    }
                }
                let runtime = javascript_runtime.as_ref().expect("runtime initialized");
                runtime.set_indexed_db_state(
                    indexed_db_state.origin(&storage_key(&committed_url, document_origin)),
                );
                let modifiers = action.get("modifiers").and_then(Value::as_i64).unwrap_or(0);
                if !(0..=15).contains(&modifiers) {
                    let response = content_error_response(
                        id,
                        NativeEngineError::invalid(
                            "content-process key modifiers",
                            "must be an integer mask from 0 through 15",
                        ),
                    );
                    write_value_frame(&mut stdout, &response).await?;
                    continue;
                }
                let result = match (request_kind, action_kind) {
                    ("mutate_key_event", "keydown" | "keyup") => {
                        let kind = if action_kind == "keydown" {
                            NativeEventKind::KeyDown
                        } else {
                            NativeEventKind::KeyUp
                        };
                        mutate_key_event_with_event_bridge(
                            current,
                            runtime,
                            &mut committed_url,
                            document_origin,
                            viewport,
                            node_index,
                            key,
                            kind,
                            modifiers,
                        )
                    }
                    ("mutate_key_shortcut", "shortcut") => mutate_key_shortcut_with_event_bridge(
                        current,
                        runtime,
                        &mut committed_url,
                        document_origin,
                        viewport,
                        node_index,
                        key,
                        modifiers,
                        action
                            .get("apply_default")
                            .and_then(Value::as_bool)
                            .unwrap_or(false),
                    ),
                    ("mutate_key_events", "") => mutate_key_with_event_bridge(
                        current,
                        runtime,
                        &mut committed_url,
                        document_origin,
                        viewport,
                        node_index,
                        key,
                    ),
                    _ => Err(NativeEngineError::invalid(
                        "content-process key action",
                        "request kind must match keydown, keyup, or shortcut action",
                    )),
                };
                match result {
                    Ok((next, mutation)) => {
                        document = Some(next);
                        document_url = Some(committed_url);
                        json!({
                            "kind": "mutated",
                            "id": id,
                            "mutation_history": mutation.history,
                            "scroll_commands": mutation.scroll_commands,
                            "document_base64": base64::engine::general_purpose::STANDARD
                                .encode(serde_json::to_vec(&mutation.document).unwrap_or_default()),
                            "events": mutation.events.iter().map(|event| json!({
                                "node_index": event.node_index,
                                "kind": event_kind_text(event.kind),
                            })).collect::<Vec<_>>(),
                        })
                    }
                    Err(error) => content_error_response(id, error),
                }
            }
            "mutate_before_unload" if protocol_matches(&request) && running => {
                let Some(current) = document.as_ref() else {
                    let response = content_error_response(
                        id,
                        NativeEngineError::Worker {
                            operation: "content process beforeunload".into(),
                            reason: "content process has no committed document".into(),
                        },
                    );
                    write_value_frame(&mut stdout, &response).await?;
                    continue;
                };
                let Some(mut committed_url) = document_url.clone() else {
                    let response = content_error_response(
                        id,
                        NativeEngineError::Worker {
                            operation: "content process beforeunload".into(),
                            reason: "content process has no committed URL".into(),
                        },
                    );
                    write_value_frame(&mut stdout, &response).await?;
                    continue;
                };
                let Some(document_origin) = document_origin.as_ref() else {
                    let response = content_error_response(
                        id,
                        NativeEngineError::Worker {
                            operation: "content process beforeunload".into(),
                            reason: "content process has no committed origin".into(),
                        },
                    );
                    write_value_frame(&mut stdout, &response).await?;
                    continue;
                };
                let Some(runtime) = javascript_runtime.as_ref() else {
                    let response = content_error_response(
                        id,
                        NativeEngineError::Worker {
                            operation: "content process beforeunload".into(),
                            reason: "content process has no JavaScript runtime".into(),
                        },
                    );
                    write_value_frame(&mut stdout, &response).await?;
                    continue;
                };
                runtime.set_indexed_db_state(
                    indexed_db_state.origin(&storage_key(&committed_url, document_origin)),
                );
                match mutate_before_unload(
                    current,
                    runtime,
                    &mut committed_url,
                    document_origin,
                    viewport,
                ) {
                    Ok((next, mutation)) => {
                        let allowed = mutation.allowed;
                        document = Some(next);
                        document_url = Some(committed_url);
                        json!({
                            "kind": "mutated",
                            "id": id,
                            "allowed": allowed,
                            "mutation_history": mutation.history,
                            "scroll_commands": mutation.scroll_commands,
                            "document_base64": base64::engine::general_purpose::STANDARD
                                .encode(serde_json::to_vec(&mutation.document).unwrap_or_default()),
                            "events": mutation.events.iter().map(|event| json!({
                                "node_index": event.node_index,
                                "kind": event_kind_text(event.kind),
                            })).collect::<Vec<_>>(),
                            "navigation": mutation.navigation.as_ref().map(|navigation| json!({
                                "node_index": navigation.node_index,
                                "href": navigation.href,
                                "submitter_node_index": navigation.submitter_node_index,
                                "location": navigation.location,
                                "replace_history": navigation.replace_history,
                            })),
                        })
                    }
                    Err(error) => content_error_response(id, error),
                }
            }
            "mutate_lifecycle_events" if protocol_matches(&request) && running => {
                let Some(current) = document.as_ref() else {
                    let response = content_error_response(
                        id,
                        NativeEngineError::Worker {
                            operation: "content process lifecycle events".into(),
                            reason: "content process has no committed document".into(),
                        },
                    );
                    write_value_frame(&mut stdout, &response).await?;
                    continue;
                };
                let event_values = request
                    .get("action")
                    .and_then(|action| action.get("events"))
                    .and_then(Value::as_array)
                    .ok_or_else(|| {
                        NativeEngineError::invalid(
                            "content-process lifecycle events",
                            "must be an array",
                        )
                    })?;
                let mut events = Vec::with_capacity(event_values.len());
                for value in event_values {
                    let event = value.as_str().and_then(parse_event_kind).ok_or_else(|| {
                        NativeEngineError::invalid(
                            "content-process lifecycle event",
                            "must be a supported lifecycle event",
                        )
                    })?;
                    if !matches!(
                        event,
                        NativeEventKind::PageHide
                            | NativeEventKind::Unload
                            | NativeEventKind::PageShow
                            | NativeEventKind::PopState
                    ) {
                        return Err(NativeEngineError::invalid(
                            "content-process lifecycle event",
                            "must be pagehide, unload, pageshow, or popstate",
                        ));
                    }
                    events.push(event);
                }
                let Some(mut committed_url) = document_url.clone() else {
                    let response = content_error_response(
                        id,
                        NativeEngineError::Worker {
                            operation: "content process lifecycle events".into(),
                            reason: "content process has no committed URL".into(),
                        },
                    );
                    write_value_frame(&mut stdout, &response).await?;
                    continue;
                };
                let Some(document_origin) = document_origin.as_ref() else {
                    let response = content_error_response(
                        id,
                        NativeEngineError::Worker {
                            operation: "content process lifecycle events".into(),
                            reason: "content process has no committed origin".into(),
                        },
                    );
                    write_value_frame(&mut stdout, &response).await?;
                    continue;
                };
                let Some(runtime) = javascript_runtime.as_ref() else {
                    let response = content_error_response(
                        id,
                        NativeEngineError::Worker {
                            operation: "content process lifecycle events".into(),
                            reason: "content process has no JavaScript runtime".into(),
                        },
                    );
                    write_value_frame(&mut stdout, &response).await?;
                    continue;
                };
                runtime.set_indexed_db_state(
                    indexed_db_state.origin(&storage_key(&committed_url, document_origin)),
                );
                match mutate_lifecycle_events(
                    current,
                    runtime,
                    &mut committed_url,
                    document_origin,
                    viewport,
                    &events,
                ) {
                    Ok((next, mutation)) => {
                        document = Some(next);
                        document_url = Some(committed_url);
                        json!({
                            "kind": "mutated",
                            "id": id,
                            "mutation_history": mutation.history,
                            "scroll_commands": mutation.scroll_commands,
                            "document_base64": base64::engine::general_purpose::STANDARD
                                .encode(serde_json::to_vec(&mutation.document).unwrap_or_default()),
                            "events": mutation.events.iter().map(|event| json!({
                                "node_index": event.node_index,
                                "kind": event_kind_text(event.kind),
                            })).collect::<Vec<_>>(),
                            "navigation": mutation.navigation.as_ref().map(|navigation| json!({
                                "node_index": navigation.node_index,
                                "href": navigation.href,
                                "submitter_node_index": navigation.submitter_node_index,
                                "location": navigation.location,
                                "replace_history": navigation.replace_history,
                            })),
                        })
                    }
                    Err(error) => content_error_response(id, error),
                }
            }
            "mutate_hash_change" if protocol_matches(&request) && running => {
                let Some(current) = document.as_ref() else {
                    let response = content_error_response(
                        id,
                        NativeEngineError::Worker {
                            operation: "content process hashchange".into(),
                            reason: "content process has no committed document".into(),
                        },
                    );
                    write_value_frame(&mut stdout, &response).await?;
                    continue;
                };
                let old_url = request
                    .get("action")
                    .and_then(|action| action.get("old_url"))
                    .and_then(Value::as_str)
                    .ok_or_else(|| {
                        NativeEngineError::invalid("hashchange old URL", "must be text")
                    })?;
                let new_url = request
                    .get("action")
                    .and_then(|action| action.get("new_url"))
                    .and_then(Value::as_str)
                    .ok_or_else(|| {
                        NativeEngineError::invalid("hashchange new URL", "must be text")
                    })?;
                let mut committed_url = new_url.to_owned();
                validate_url_text("hashchange old URL", old_url)?;
                validate_url_text("hashchange new URL", new_url)?;
                if without_fragment(old_url) != without_fragment(new_url) || old_url == new_url {
                    return Err(NativeEngineError::invalid(
                        "hashchange URLs",
                        "must differ only by a non-empty fragment",
                    ));
                }
                let Some(document_origin) = document_origin.as_ref() else {
                    let response = content_error_response(
                        id,
                        NativeEngineError::Worker {
                            operation: "content process hashchange".into(),
                            reason: "content process has no committed origin".into(),
                        },
                    );
                    write_value_frame(&mut stdout, &response).await?;
                    continue;
                };
                let Some(runtime) = javascript_runtime.as_ref() else {
                    let response = content_error_response(
                        id,
                        NativeEngineError::Worker {
                            operation: "content process hashchange".into(),
                            reason: "content process has no JavaScript runtime".into(),
                        },
                    );
                    write_value_frame(&mut stdout, &response).await?;
                    continue;
                };
                runtime.set_indexed_db_state(
                    indexed_db_state.origin(&storage_key(new_url, document_origin)),
                );
                match mutate_hash_change(
                    current,
                    runtime,
                    old_url,
                    &mut committed_url,
                    document_origin,
                    viewport,
                ) {
                    Ok((next, mutation)) => {
                        document = Some(next);
                        document_url = Some(committed_url);
                        json!({
                            "kind": "mutated",
                            "id": id,
                            "mutation_history": mutation.history,
                            "scroll_commands": mutation.scroll_commands,
                            "document_base64": base64::engine::general_purpose::STANDARD
                                .encode(serde_json::to_vec(&mutation.document).unwrap_or_default()),
                            "events": mutation.events.iter().map(|event| json!({
                                "node_index": event.node_index,
                                "kind": event_kind_text(event.kind),
                            })).collect::<Vec<_>>(),
                            "navigation": mutation.navigation.as_ref().map(|navigation| json!({
                                "node_index": navigation.node_index,
                                "href": navigation.href,
                                "submitter_node_index": navigation.submitter_node_index,
                                "location": navigation.location,
                                "replace_history": navigation.replace_history,
                            })),
                        })
                    }
                    Err(error) => content_error_response(id, error),
                }
            }
            "close" if protocol_matches(&request) => {
                let (
                    storage_events,
                    indexed_db_changes,
                    _dialogs,
                    _popups,
                    _post_messages,
                    _window_closes,
                    _window_navigations,
                    _window_name,
                ) = sync_content_runtime_state(
                    javascript_runtime.as_ref(),
                    &mut storage_state,
                    &mut indexed_db_state,
                    &mut resource_loader,
                    document_url.as_deref(),
                    document_origin.as_ref(),
                )?;
                persist_content_profile(
                    storage_profile_path.as_deref(),
                    &storage_state,
                    &indexed_db_state,
                    &storage_events,
                    &indexed_db_changes,
                    &mut resource_loader,
                )?;
                write_value_frame(&mut stdout, &json!({"kind":"closed","id":id})).await?;
                return Ok(());
            }
            _ => content_error_response(
                id,
                NativeEngineError::Worker {
                    operation: "content process command".into(),
                    reason: "content process rejected the typed command".into(),
                },
            ),
        };
        let (
            storage_events,
            indexed_db_changes,
            dialogs,
            popups,
            post_messages,
            window_closes,
            window_navigations,
            response_window_name,
        ) = sync_content_runtime_state(
            javascript_runtime.as_ref(),
            &mut storage_state,
            &mut indexed_db_state,
            &mut resource_loader,
            document_url.as_deref(),
            document_origin.as_ref(),
        )?;
        if javascript_runtime.is_some() {
            window_name = response_window_name.clone();
        }
        persist_content_profile(
            storage_profile_path.as_deref(),
            &storage_state,
            &indexed_db_state,
            &storage_events,
            &indexed_db_changes,
            &mut resource_loader,
        )?;
        let mut response_dialogs = decode_dialogs(&response, "merge content process dialogs")?;
        response_dialogs.extend(dialogs);
        if response_dialogs.len() > MAX_NATIVE_DIALOGS {
            return Err(NativeEngineError::limit(
                "content-process dialogs",
                MAX_NATIVE_DIALOGS,
                response_dialogs.len(),
            ));
        }
        let mut response_post_messages =
            decode_post_message_requests(&response, "merge content process postMessage")?;
        response_post_messages.extend(post_messages);
        if response_post_messages.len() > MAX_NATIVE_EFFECTS {
            return Err(NativeEngineError::limit(
                "content-process postMessage requests",
                MAX_NATIVE_EFFECTS,
                response_post_messages.len(),
            ));
        }
        let mut response_window_closes =
            decode_window_close_requests(&response, "merge content process window close")?;
        response_window_closes.extend(window_closes);
        if response_window_closes.len() > MAX_NATIVE_EFFECTS {
            return Err(NativeEngineError::limit(
                "content-process window close requests",
                MAX_NATIVE_EFFECTS,
                response_window_closes.len(),
            ));
        }
        let mut response_window_navigations = decode_window_navigation_requests(
            &response,
            "merge content process window navigation",
        )?;
        response_window_navigations.extend(window_navigations);
        if response_window_navigations.len() > MAX_NATIVE_EFFECTS {
            return Err(NativeEngineError::limit(
                "content-process window navigation requests",
                MAX_NATIVE_EFFECTS,
                response_window_navigations.len(),
            ));
        }
        if let Some(object) = response.as_object_mut() {
            object.insert(
                "storage_events".into(),
                serde_json::to_value(storage_events).map_err(|_| NativeEngineError::Worker {
                    operation: "encode content process storage events".into(),
                    reason: "content process storage events could not be encoded".into(),
                })?,
            );
            object.insert(
                "indexed_db_changes".into(),
                serde_json::to_value(&indexed_db_changes).map_err(|_| {
                    NativeEngineError::Worker {
                        operation: "encode content process IndexedDB changes".into(),
                        reason: "content process IndexedDB changes could not be encoded".into(),
                    }
                })?,
            );
            object.insert(
                "dialogs".into(),
                serde_json::to_value(response_dialogs).map_err(|_| NativeEngineError::Worker {
                    operation: "encode content process dialogs".into(),
                    reason: "content process dialogs could not be encoded".into(),
                })?,
            );
            object.insert(
                "popups".into(),
                serde_json::to_value(popups).map_err(|_| NativeEngineError::Worker {
                    operation: "encode content process popups".into(),
                    reason: "content process popups could not be encoded".into(),
                })?,
            );
            object.insert(
                "post_messages".into(),
                serde_json::to_value(response_post_messages).map_err(|_| {
                    NativeEngineError::Worker {
                        operation: "encode content process postMessage requests".into(),
                        reason: "content process postMessage requests could not be encoded".into(),
                    }
                })?,
            );
            object.insert(
                "window_closes".into(),
                serde_json::to_value(response_window_closes).map_err(|_| {
                    NativeEngineError::Worker {
                        operation: "encode content process window close requests".into(),
                        reason: "content process window close requests could not be encoded".into(),
                    }
                })?,
            );
            object.insert(
                "window_navigations".into(),
                serde_json::to_value(response_window_navigations).map_err(|_| {
                    NativeEngineError::Worker {
                        operation: "encode content process window navigation requests".into(),
                        reason: "content process window navigation requests could not be encoded"
                            .into(),
                    }
                })?,
            );
            object.insert("window_name".into(), Value::String(response_window_name));
        }
        write_value_frame(&mut stdout, &response).await?;
    }
}

fn sync_content_runtime_state(
    runtime: Option<&NativeJavaScriptRuntime>,
    storage_state: &mut NativeWebStorageState,
    indexed_db_state: &mut NativeIndexedDbState,
    resource_loader: &mut Option<NativeResourceLoader>,
    document_url: Option<&str>,
    document_origin: Option<&NativeOrigin>,
) -> Result<
    (
        Vec<NativeStorageEvent>,
        Vec<NativeIndexedDbChange>,
        Vec<NativeDialog>,
        Vec<NativePopupRequest>,
        Vec<NativePostMessageRequest>,
        Vec<NativeWindowCloseRequest>,
        Vec<NativeWindowNavigationRequest>,
        String,
    ),
    NativeEngineError,
> {
    let Some(runtime) = runtime else {
        return Ok((
            Vec::new(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
            String::new(),
        ));
    };
    let before_indexed_db = indexed_db_state.clone();
    *storage_state = runtime.storage_state();
    let indexed_db_changes =
        if let (Some(document_url), Some(document_origin)) = (document_url, document_origin) {
            let key = storage_key(document_url, document_origin);
            let before = before_indexed_db.origin(&key);
            let after = runtime.indexed_db_state();
            indexed_db_state.replace_origin(key.clone(), after.clone())?;
            diff_indexed_db_changes(&key, &before, &after)?
        } else {
            Vec::new()
        };
    if let (Some(loader), Some(document_url)) = (resource_loader.as_mut(), document_url) {
        for value in runtime.take_cookie_updates() {
            loader.set_document_cookie(document_url, &value)?;
        }
    }
    Ok((
        runtime.take_storage_changes(),
        indexed_db_changes,
        runtime.take_dialog_events(),
        runtime.take_popup_events(),
        runtime.take_post_message_events(),
        runtime.take_window_close_events(),
        runtime.take_window_navigation_events(),
        runtime.window_name(),
    ))
}

fn persist_content_profile(
    storage_path: Option<&Path>,
    storage_state: &NativeWebStorageState,
    indexed_db_state: &NativeIndexedDbState,
    storage_events: &[NativeStorageEvent],
    indexed_db_changes: &[NativeIndexedDbChange],
    resource_loader: &mut Option<NativeResourceLoader>,
) -> Result<(), NativeEngineError> {
    let cookie_state = resource_loader
        .as_ref()
        .map(NativeResourceLoader::cookie_profile)
        .unwrap_or_default();
    let cookie_changes = resource_loader
        .as_mut()
        .map(NativeResourceLoader::take_cookie_changes)
        .unwrap_or_default();
    save_web_storage_profile(
        storage_path,
        storage_state,
        storage_events,
        &cookie_state,
        &cookie_changes,
        indexed_db_state,
        indexed_db_changes,
    )
}

fn refresh_content_runtime_cookie(
    runtime: Option<&NativeJavaScriptRuntime>,
    resource_loader: Option<&NativeResourceLoader>,
    document_url: Option<&str>,
) -> Result<(), NativeEngineError> {
    if let (Some(runtime), Some(loader), Some(document_url)) =
        (runtime, resource_loader, document_url)
    {
        runtime.set_cookie_state(loader.document_cookie(document_url)?);
    }
    Ok(())
}

async fn load_content_resource(
    request: &Value,
    resource_loader: &mut Option<NativeResourceLoader>,
    storage_path: Option<&Path>,
) -> Result<
    (
        NativeContentLoad,
        NativeDocument,
        Viewport,
        Vec<NativePageScript>,
        Vec<(u32, NativeEventKind)>,
    ),
    NativeEngineError,
> {
    let url = request
        .get("url")
        .and_then(Value::as_str)
        .ok_or_else(|| NativeEngineError::invalid("content-process URL", "must be text"))?;
    validate_url_text("content-process URL", url)?;
    if !is_network_url(without_fragment(url)) {
        return Err(NativeEngineError::UnsupportedUrl {
            reason: "content process accepts only HTTP(S) document URLs".into(),
        });
    }
    let method = match request
        .get("method")
        .and_then(Value::as_str)
        .unwrap_or("GET")
    {
        "GET" => NativeNavigationMethod::Get,
        "POST" => NativeNavigationMethod::Post,
        _ => {
            return Err(NativeEngineError::UnsupportedUrl {
                reason: "content process supports only GET and POST document navigation".into(),
            });
        }
    };
    let text_body = request
        .get("body")
        .and_then(|value| (!value.is_null()).then_some(value))
        .map(|value| {
            value.as_str().ok_or_else(|| {
                NativeEngineError::invalid(
                    "content-process navigation body",
                    "must be text or null",
                )
            })
        })
        .transpose()?;
    let body_base64 = request
        .get("body_base64")
        .and_then(|value| (!value.is_null()).then_some(value))
        .map(|value| {
            value.as_str().ok_or_else(|| {
                NativeEngineError::invalid(
                    "content-process navigation binary body",
                    "must be base64 text or null",
                )
            })
        })
        .transpose()?;
    if text_body.is_some() && body_base64.is_some() {
        return Err(NativeEngineError::invalid(
            "content-process navigation body",
            "must use either text or base64 bytes",
        ));
    }
    let body = match body_base64 {
        Some(encoded) => Some(NativeRequestBody::Bytes(
            base64::engine::general_purpose::STANDARD
                .decode(encoded)
                .map_err(|_| {
                    NativeEngineError::invalid(
                        "content-process navigation binary body",
                        "must be valid base64",
                    )
                })?,
        )),
        None => text_body.map(str::to_owned).map(NativeRequestBody::Text),
    };
    let content_type = request
        .get("content_type")
        .and_then(|value| (!value.is_null()).then_some(value))
        .map(|value| {
            value.as_str().ok_or_else(|| {
                NativeEngineError::invalid(
                    "content-process navigation content type",
                    "must be text or null",
                )
            })
        })
        .transpose()?;
    let navigation = match method {
        NativeNavigationMethod::Get => {
            if body.is_some() {
                return Err(NativeEngineError::invalid(
                    "content-process GET body",
                    "must be null",
                ));
            }
            if content_type.is_some() {
                return Err(NativeEngineError::invalid(
                    "content-process GET content type",
                    "must be null",
                ));
            }
            NativeNavigationRequest::get(url)
        }
        NativeNavigationMethod::Post => NativeNavigationRequest::post_with_body(
            url,
            body.ok_or_else(|| {
                NativeEngineError::invalid("content-process POST body", "must be present")
            })?,
            content_type
                .unwrap_or_else(|| "application/x-www-form-urlencoded".into())
                .to_owned(),
        )?,
        _ => {
            return Err(NativeEngineError::UnsupportedUrl {
                reason: "content process supports only GET and POST document navigation".into(),
            });
        }
    };
    let max_document_bytes = request
        .get("max_document_bytes")
        .and_then(Value::as_u64)
        .and_then(|value| usize::try_from(value).ok())
        .ok_or_else(|| {
            NativeEngineError::invalid(
                "content-process document limit",
                "must be a positive integer",
            )
        })?;
    let limits = NativeEngineLimits {
        max_document_bytes,
        max_nodes: request
            .get("max_nodes")
            .and_then(Value::as_u64)
            .and_then(|value| usize::try_from(value).ok())
            .unwrap_or(NativeEngineLimits::default().max_nodes),
        max_dom_depth: request
            .get("max_dom_depth")
            .and_then(Value::as_u64)
            .and_then(|value| usize::try_from(value).ok())
            .unwrap_or(NativeEngineLimits::default().max_dom_depth),
        max_text_bytes: request
            .get("max_text_bytes")
            .and_then(Value::as_u64)
            .and_then(|value| usize::try_from(value).ok())
            .unwrap_or(NativeEngineLimits::default().max_text_bytes),
        ..NativeEngineLimits::default()
    };
    let viewport = Viewport {
        width: request
            .get("viewport_width")
            .and_then(Value::as_u64)
            .and_then(|value| u32::try_from(value).ok())
            .unwrap_or_else(|| Viewport::default().width),
        height: request
            .get("viewport_height")
            .and_then(Value::as_u64)
            .and_then(|value| u32::try_from(value).ok())
            .unwrap_or_else(|| Viewport::default().height),
        device_scale_factor_milli: request
            .get("viewport_device_scale_factor_milli")
            .and_then(Value::as_u64)
            .and_then(|value| u32::try_from(value).ok())
            .unwrap_or_else(|| Viewport::default().device_scale_factor_milli),
    };
    viewport.validate()?;
    let referrer = request
        .get("referrer")
        .and_then(|value| (!value.is_null()).then_some(value))
        .map(|value| {
            value.as_str().ok_or_else(|| {
                NativeEngineError::invalid("content-process referrer", "must be text or null")
            })
        })
        .transpose()?;
    let loader = match resource_loader {
        Some(loader) if loader.max_document_bytes() == max_document_bytes => loader,
        Some(_) => {
            return Err(NativeEngineError::invalid(
                "content-process document limit",
                "cannot change the document limit after the content process starts",
            ));
        }
        None => {
            *resource_loader = Some(NativeResourceLoader::for_content_process(
                max_document_bytes,
                storage_path,
            )?);
            resource_loader
                .as_mut()
                .expect("content-process resource loader was just initialized")
        }
    };
    let resource = loader
        .load_async_request_with_referrer(&navigation, referrer)
        .await?;
    let frame_sources = loader.frame_sources_for_document(&resource.url)?;
    let discovery = NativeDocument::parse(&resource.body, &limits)?;
    let mut external_stylesheets = Vec::new();
    let mut resource_events = Vec::new();
    for href in discovery
        .external_stylesheet_links()
        .into_iter()
        .take(MAX_CONTENT_STYLESHEETS)
    {
        let (node_index, href) = href;
        let event_kind = match loader.load_stylesheet_async(&resource.url, &href).await {
            Ok(Some(stylesheet)) => {
                let next_len = external_stylesheets
                    .iter()
                    .map(String::len)
                    .sum::<usize>()
                    .saturating_add(stylesheet.len());
                if next_len <= MAX_CONTENT_STYLESHEET_BYTES {
                    external_stylesheets.push(stylesheet);
                    NativeEventKind::Load
                } else {
                    NativeEventKind::Error
                }
            }
            Ok(None) | Err(_) => NativeEventKind::Error,
        };
        resource_events.push((node_index, event_kind));
    }
    let mut document =
        NativeDocument::parse_with_stylesheets(&resource.body, &limits, &external_stylesheets, 1)?;
    resource_events
        .extend(load_external_images(&mut document, loader, &resource.url, viewport).await?);
    let (script_sources, script_resource_events) =
        load_page_script_sources(&document, loader, &resource.url).await?;
    resource_events.extend(script_resource_events);
    resource_events.sort_unstable_by_key(|(node_index, _)| *node_index);
    resource_events.dedup();
    let wire = document.to_content_wire();
    Ok((
        NativeContentLoad {
            url: resource.url,
            origin: resource.origin,
            document: wire,
            frame_sources,
            events: resource_events
                .iter()
                .map(|(node_index, kind)| NativeContentEvent {
                    node_index: *node_index,
                    kind: *kind,
                })
                .collect(),
            scroll_commands: Vec::new(),
            navigation: None,
            storage_events: Vec::new(),
            indexed_db_changes: Vec::new(),
            dialogs: Vec::new(),
            popups: Vec::new(),
            post_messages: Vec::new(),
            window_closes: Vec::new(),
            window_navigations: Vec::new(),
            window_name: String::new(),
        },
        document,
        viewport,
        script_sources,
        resource_events,
    ))
}

async fn load_external_images(
    document: &mut NativeDocument,
    loader: &mut NativeResourceLoader,
    document_url: &str,
    viewport: Viewport,
) -> Result<Vec<(u32, NativeEventKind)>, NativeEngineError> {
    let mut image_events = Vec::new();
    for (node_index, source) in document
        .external_image_links(viewport)
        .into_iter()
        .take(MAX_CONTENT_IMAGES)
    {
        let node_id = NativeNodeId::from_parts(document.generation(), node_index);
        if document.image_resource_for_node(node_id).is_some() {
            continue;
        }
        document.mark_image_load(node_index, source.clone(), viewport)?;
        let event_kind = match loader.load_image_async(document_url, &source).await {
            Ok(Some(image)) => {
                document.set_image_resource(node_index, source, image)?;
                NativeEventKind::Load
            }
            Ok(None) | Err(_) => NativeEventKind::Error,
        };
        image_events.push((node_index, event_kind));
    }
    for (node_index, source) in document
        .external_background_image_links()
        .into_iter()
        .take(MAX_CONTENT_IMAGES)
    {
        let node_id = NativeNodeId::from_parts(document.generation(), node_index);
        if document
            .background_image_resource_for_node(node_id)
            .is_some()
        {
            continue;
        }
        match loader.load_image_async(document_url, &source).await {
            Ok(Some(image)) => {
                document.set_background_image_resource(node_index, source, image)?;
            }
            Ok(None) | Err(_) => {}
        }
    }
    Ok(image_events)
}

async fn load_page_script_sources(
    document: &NativeDocument,
    loader: &mut NativeResourceLoader,
    document_url: &str,
) -> Result<(Vec<NativePageScript>, Vec<(u32, NativeEventKind)>), NativeEngineError> {
    load_page_script_source_list(
        document.page_script_sources(
            super::javascript::MAX_NATIVE_INLINE_SCRIPTS,
            MAX_NATIVE_SCRIPT_BYTES,
        ),
        loader,
        document_url,
        "glass-inline-module",
    )
    .await
}

async fn load_dynamic_page_script_sources(
    sources: Vec<NativePageScriptSource>,
    loader: &mut NativeResourceLoader,
    document_url: &str,
) -> Result<(Vec<NativePageScript>, Vec<(u32, NativeEventKind)>), NativeEngineError> {
    load_page_script_source_list(sources, loader, document_url, "glass-dynamic-module").await
}

async fn load_page_script_source_list(
    page_sources: Vec<NativePageScriptSource>,
    loader: &mut NativeResourceLoader,
    document_url: &str,
    module_name_prefix: &str,
) -> Result<(Vec<NativePageScript>, Vec<(u32, NativeEventKind)>), NativeEngineError> {
    let mut sources = Vec::new();
    let mut resource_events = Vec::new();
    for (index, script) in page_sources.into_iter().enumerate() {
        match script {
            NativePageScriptSource::Inline {
                source,
                timing,
                node_index,
            } => {
                sources.push((
                    timing,
                    NativePageScript::Classic {
                        source,
                        node_index: Some(node_index),
                    },
                ));
            }
            NativePageScriptSource::ModuleInline {
                source,
                timing,
                node_index,
            } => {
                let name = format!("{document_url}#{module_name_prefix}-{index}");
                let mut seen = BTreeSet::new();
                seen.insert(name.clone());
                let mut total_bytes = source.len();
                let script_start = sources.len();
                sources.push((
                    timing,
                    NativePageScript::Module {
                        name: name.clone(),
                        source: source.clone(),
                        node_index: Some(node_index),
                    },
                ));
                let dependency_result = load_module_dependencies(
                    document_url,
                    &name,
                    &source,
                    timing,
                    loader,
                    &mut sources,
                    &mut seen,
                    &mut total_bytes,
                )
                .await;
                if dependency_result.is_err() {
                    sources.truncate(script_start);
                }
            }
            NativePageScriptSource::External {
                href,
                timing,
                node_index,
            } => {
                match loader
                    .load_script_async(document_url, &href, MAX_NATIVE_SCRIPT_BYTES)
                    .await
                {
                    Ok(resource) => match resource {
                        Some(resource) => {
                            resource_events.push((node_index, NativeEventKind::Load));
                            sources.push((
                                timing,
                                NativePageScript::Classic {
                                    source: resource.body,
                                    node_index: Some(node_index),
                                },
                            ));
                        }
                        None => resource_events.push((node_index, NativeEventKind::Error)),
                    },
                    Err(_) => resource_events.push((node_index, NativeEventKind::Error)),
                }
            }
            NativePageScriptSource::ModuleExternal {
                href,
                timing,
                node_index,
            } => {
                match loader
                    .load_script_async(document_url, &href, MAX_NATIVE_SCRIPT_BYTES)
                    .await
                {
                    Ok(resource) => match resource {
                        Some(resource) => {
                            let script_start = sources.len();
                            let name = resource.url;
                            let source = resource.body;
                            let mut seen = BTreeSet::new();
                            seen.insert(name.clone());
                            let mut total_bytes = source.len();
                            sources.push((
                                timing,
                                NativePageScript::Module {
                                    name: name.clone(),
                                    source: source.clone(),
                                    node_index: Some(node_index),
                                },
                            ));
                            let dependency_result = load_module_dependencies(
                                document_url,
                                &name,
                                &source,
                                timing,
                                loader,
                                &mut sources,
                                &mut seen,
                                &mut total_bytes,
                            )
                            .await;
                            if dependency_result.is_ok() {
                                resource_events.push((node_index, NativeEventKind::Load));
                            } else {
                                sources.truncate(script_start);
                                resource_events.push((node_index, NativeEventKind::Error));
                            }
                        }
                        None => resource_events.push((node_index, NativeEventKind::Error)),
                    },
                    Err(_) => resource_events.push((node_index, NativeEventKind::Error)),
                }
            }
        }
    }
    Ok((order_page_scripts(sources), resource_events))
}

/// Load and execute dynamic external/module sources discovered by a dynamic
/// script. Inline descendants are handled synchronously by the shared
/// scheduler; only sources that require the content-process loader are
/// returned by each scheduler turn.
async fn execute_dynamic_page_scripts_with_loader(
    document: &mut NativeDocument,
    runtime: &NativeJavaScriptRuntime,
    mut sources: Vec<NativePageScriptSource>,
    loader: &mut NativeResourceLoader,
    document_url: &str,
    document_origin: &NativeOrigin,
    viewport: Viewport,
) -> Result<NativePageScriptResult, NativeEngineError> {
    let mut aggregate = NativePageScriptResult {
        pending_fetches: Vec::new(),
        websocket_commands: Vec::new(),
        event_source_commands: Vec::new(),
        pending_script_sources: Vec::new(),
        scroll_commands: Vec::new(),
        navigation: None,
        dialogs: Vec::new(),
        events: Vec::new(),
    };
    let mut batches = 0usize;
    loop {
        batches = batches.saturating_add(1);
        if batches > super::javascript::MAX_NATIVE_INLINE_SCRIPTS {
            return Err(NativeEngineError::limit(
                "native dynamic script loader turns",
                super::javascript::MAX_NATIVE_INLINE_SCRIPTS,
                batches,
            ));
        }
        let (scripts, resource_events) =
            load_dynamic_page_script_sources(sources, loader, document_url).await?;
        let mut result = execute_dynamic_page_scripts(
            document,
            runtime,
            scripts,
            document_url,
            document_origin,
            viewport,
            &resource_events,
        )?;
        let next_sources = std::mem::take(&mut result.pending_script_sources);
        merge_dynamic_page_script_result(&mut aggregate, result)?;
        if next_sources.is_empty() {
            return Ok(aggregate);
        }
        sources = next_sources;
    }
}

fn merge_dynamic_page_script_result(
    aggregate: &mut NativePageScriptResult,
    result: NativePageScriptResult,
) -> Result<(), NativeEngineError> {
    aggregate.pending_fetches.extend(result.pending_fetches);
    aggregate
        .websocket_commands
        .extend(result.websocket_commands);
    aggregate
        .event_source_commands
        .extend(result.event_source_commands);
    aggregate.scroll_commands.extend(result.scroll_commands);
    aggregate.dialogs.extend(result.dialogs);
    aggregate.events.extend(result.events);
    if let Some(navigation) = result.navigation {
        if aggregate.navigation.is_some() {
            return Err(NativeEngineError::TargetNotActionable {
                reason: "one dynamic page-script batch cannot activate multiple navigations".into(),
            });
        }
        aggregate.navigation = Some(navigation);
    }
    Ok(())
}

async fn load_module_dependencies(
    owner_url: &str,
    module_url: &str,
    source: &str,
    timing: NativePageScriptTiming,
    loader: &mut NativeResourceLoader,
    scripts: &mut Vec<(NativePageScriptTiming, NativePageScript)>,
    seen: &mut BTreeSet<String>,
    total_bytes: &mut usize,
) -> Result<(), NativeEngineError> {
    let mut pending = vec![(module_url.to_owned(), source.to_owned())];
    while let Some((current_url, current_source)) = pending.pop() {
        let mut specifiers = static_module_specifiers(&current_source)?;
        specifiers.extend(literal_dynamic_module_specifiers(&current_source));
        for specifier in specifiers {
            let Some(target) = resolve_module_specifier(&current_url, &specifier)? else {
                continue;
            };
            let Some(resource) = loader
                .load_script_async(owner_url, &target, MAX_NATIVE_SCRIPT_BYTES)
                .await?
            else {
                return Err(NativeEngineError::Network {
                    operation: "module dependency".into(),
                    reason: "module dependency could not be loaded".into(),
                });
            };
            let name = resource.url;
            if !seen.insert(name.clone()) {
                continue;
            }
            if seen.len() > MAX_NATIVE_MODULE_IMPORTS {
                return Err(NativeEngineError::limit(
                    "module graph entries",
                    MAX_NATIVE_MODULE_IMPORTS,
                    seen.len(),
                ));
            }
            let source = resource.body;
            *total_bytes = total_bytes.saturating_add(source.len());
            if *total_bytes > MAX_NATIVE_SCRIPT_BYTES.saturating_mul(MAX_NATIVE_MODULE_IMPORTS) {
                return Err(NativeEngineError::limit(
                    "module graph bytes",
                    MAX_NATIVE_SCRIPT_BYTES.saturating_mul(MAX_NATIVE_MODULE_IMPORTS),
                    *total_bytes,
                ));
            }
            scripts.push((
                timing,
                NativePageScript::ModuleDependency {
                    name: name.clone(),
                    source: source.clone(),
                },
            ));
            pending.push((name, source));
        }
    }
    Ok(())
}

fn resolve_module_specifier(
    module_url: &str,
    specifier: &str,
) -> Result<Option<String>, NativeEngineError> {
    let is_absolute = specifier.starts_with("http://") || specifier.starts_with("https://");
    if !is_absolute
        && !specifier.starts_with("./")
        && !specifier.starts_with("../")
        && !specifier.starts_with('/')
    {
        return Err(NativeEngineError::UnsupportedUrl {
            reason: "bare module specifiers require an import map".into(),
        });
    }
    let base = Url::parse(without_fragment(module_url)).map_err(|_| {
        NativeEngineError::UnsupportedUrl {
            reason: "module owner URL is not valid URL syntax".into(),
        }
    })?;
    let mut target = if is_absolute {
        Url::parse(specifier)
    } else {
        base.join(specifier)
    }
    .map_err(|_| NativeEngineError::UnsupportedUrl {
        reason: "module specifier could not be resolved against its owner".into(),
    })?;
    target.set_fragment(None);
    if !is_network_url(target.as_str()) {
        return Ok(None);
    }
    Ok(Some(target.to_string()))
}

fn mutate_click_with_event_preflight(
    current: &NativeDocument,
    runtime: &NativeJavaScriptRuntime,
    document_url: &mut String,
    document_origin: &NativeOrigin,
    viewport: Viewport,
    node_index: u32,
) -> Result<(NativeDocument, NativeContentMutation), NativeEngineError> {
    let node_id = NativeNodeId::from_parts(current.generation(), node_index);
    let mut next = current.clone();
    let mut history = Vec::new();
    let mut scroll_commands = Vec::new();
    let mut events = next.apply_script_focus(node_id)?;
    let focus_metadata = events
        .iter()
        .map(|(node, kind)| (node.index(), *kind))
        .collect::<Vec<_>>();
    if let Some(source) = host_event_script(&focus_metadata)? {
        let evaluation =
            runtime.evaluate(&source, &next, document_url, document_origin, viewport)?;
        apply_content_event_history(
            &evaluation.commands,
            document_url,
            document_origin,
            runtime,
            &mut history,
        )?;
        scroll_commands.extend(extract_scroll_commands(&evaluation.commands));
        events.extend(next.apply_script_commands(&evaluation.commands)?);
    }

    let click_source =
        host_event_script(&[(node_index, NativeEventKind::Click)])?.ok_or_else(|| {
            NativeEngineError::Worker {
                operation: "content process click preflight".into(),
                reason: "native click event source was empty".into(),
            }
        })?;
    let click_evaluation = runtime.evaluate(
        &click_source,
        &next,
        document_url,
        document_origin,
        viewport,
    )?;
    let click_allowed = click_evaluation
        .value
        .as_array()
        .and_then(|values| values.first())
        .and_then(Value::as_bool)
        .ok_or_else(|| NativeEngineError::Worker {
            operation: "content process click preflight".into(),
            reason: "native click event result was invalid".into(),
        })?;
    apply_content_event_history(
        &click_evaluation.commands,
        document_url,
        document_origin,
        runtime,
        &mut history,
    )?;
    scroll_commands.extend(extract_scroll_commands(&click_evaluation.commands));
    events.extend(next.apply_script_commands(&click_evaluation.commands)?);
    let mut navigation = None;
    if click_allowed {
        events.extend(next.apply_click(node_id)?);
        if let Some(form_id) = next.submit_control_form(node_id) {
            let invalid = next.invalid_form_controls(form_id, Some(node_id))?;
            if invalid.is_empty() {
                if dispatch_submit_event(
                    &mut next,
                    runtime,
                    document_url,
                    document_origin,
                    viewport,
                    form_id,
                    Some(node_id),
                    &mut events,
                    &mut history,
                    &mut scroll_commands,
                )? {
                    navigation = Some(NativeContentNavigation {
                        node_index: form_id.index(),
                        href: next
                            .form_submission_request_with_submitter(
                                form_id,
                                document_url,
                                Some(node_id),
                            )?
                            .url,
                        submitter_node_index: Some(node_id.index()),
                        location: false,
                        replace_history: false,
                    });
                }
            } else {
                dispatch_invalid_events(
                    &mut next,
                    runtime,
                    document_url,
                    document_origin,
                    viewport,
                    &invalid,
                    &mut events,
                    &mut history,
                    &mut scroll_commands,
                )?;
            }
        }
    } else {
        events.push((node_id, NativeEventKind::Click));
    }
    dispatch_scroll_events(
        &mut next,
        runtime,
        document_url,
        document_origin,
        viewport,
        &mut scroll_commands,
        &mut events,
        &mut history,
    )?;
    if events.len() > MAX_NATIVE_EFFECTS {
        return Err(NativeEngineError::limit(
            "content-process click event effects",
            MAX_NATIVE_EFFECTS,
            events.len(),
        ));
    }
    let mutation = NativeContentMutation {
        document: next.to_content_wire(),
        events: events
            .into_iter()
            .map(|(node, kind)| NativeContentEvent {
                node_index: node.index(),
                kind,
            })
            .collect(),
        navigation,
        allowed: click_allowed,
        history,
        scroll_commands,
        storage_events: Vec::new(),
        indexed_db_changes: Vec::new(),
        dialogs: Vec::new(),
        popups: Vec::new(),
        post_messages: Vec::new(),
        window_closes: Vec::new(),
        window_navigations: Vec::new(),
        window_name: String::new(),
    };
    Ok((next, mutation))
}

fn dispatch_submit_event(
    document: &mut NativeDocument,
    runtime: &NativeJavaScriptRuntime,
    document_url: &mut String,
    document_origin: &NativeOrigin,
    viewport: Viewport,
    form_id: NativeNodeId,
    submitter: Option<NativeNodeId>,
    events: &mut Vec<(NativeNodeId, NativeEventKind)>,
    history: &mut Vec<NativeScriptCommand>,
    scroll_commands: &mut Vec<NativeScriptCommand>,
) -> Result<bool, NativeEngineError> {
    let source = host_submit_event_script(form_id.index(), submitter.map(NativeNodeId::index))?
        .ok_or_else(|| NativeEngineError::Worker {
            operation: "content process submit event".into(),
            reason: "native submit event source was empty".into(),
        })?;
    let evaluation =
        runtime.evaluate(&source, document, document_url, document_origin, viewport)?;
    apply_content_event_history(
        &evaluation.commands,
        document_url,
        document_origin,
        runtime,
        history,
    )?;
    scroll_commands.extend(extract_scroll_commands(&evaluation.commands));
    let allowed = evaluation
        .value
        .as_array()
        .and_then(|values| values.first())
        .and_then(Value::as_bool)
        .ok_or_else(|| NativeEngineError::Worker {
            operation: "content process submit event".into(),
            reason: "native submit event result was invalid".into(),
        })?;
    events.push((form_id, NativeEventKind::Submit));
    events.extend(document.apply_script_commands(&evaluation.commands)?);
    Ok(allowed)
}

fn dispatch_invalid_events(
    document: &mut NativeDocument,
    runtime: &NativeJavaScriptRuntime,
    document_url: &mut String,
    document_origin: &NativeOrigin,
    viewport: Viewport,
    invalid: &[NativeNodeId],
    events: &mut Vec<(NativeNodeId, NativeEventKind)>,
    history: &mut Vec<NativeScriptCommand>,
    scroll_commands: &mut Vec<NativeScriptCommand>,
) -> Result<(), NativeEngineError> {
    let metadata = invalid
        .iter()
        .map(|id| (id.index(), NativeEventKind::Invalid))
        .collect::<Vec<_>>();
    let Some(source) = host_event_script(&metadata)? else {
        return Ok(());
    };
    let evaluation =
        runtime.evaluate(&source, document, document_url, document_origin, viewport)?;
    apply_content_event_history(
        &evaluation.commands,
        document_url,
        document_origin,
        runtime,
        history,
    )?;
    scroll_commands.extend(extract_scroll_commands(&evaluation.commands));
    events.extend(
        invalid
            .iter()
            .copied()
            .map(|id| (id, NativeEventKind::Invalid)),
    );
    events.extend(document.apply_script_commands(&evaluation.commands)?);
    Ok(())
}

fn mutate_type_with_event_bridge(
    current: &NativeDocument,
    runtime: &NativeJavaScriptRuntime,
    document_url: &mut String,
    document_origin: &NativeOrigin,
    viewport: Viewport,
    node_index: u32,
    text: &str,
) -> Result<(NativeDocument, NativeContentMutation), NativeEngineError> {
    let node_id = NativeNodeId::from_parts(current.generation(), node_index);
    let mut next = current.clone();
    let mut history = Vec::new();
    let mut scroll_commands = Vec::new();
    let mut events = next.apply_type(node_id, text)?;
    let default_events = events.clone();
    for (event_node, event_kind) in default_events {
        let source = host_event_script(&[(event_node.index(), event_kind)])?;
        let Some(source) = source else {
            continue;
        };
        let evaluation =
            runtime.evaluate(&source, &next, document_url, document_origin, viewport)?;
        apply_content_event_history(
            &evaluation.commands,
            document_url,
            document_origin,
            runtime,
            &mut history,
        )?;
        scroll_commands.extend(extract_scroll_commands(&evaluation.commands));
        events.extend(next.apply_script_commands(&evaluation.commands)?);
        if events.len() > MAX_NATIVE_EFFECTS {
            return Err(NativeEngineError::limit(
                "content-process type event effects",
                MAX_NATIVE_EFFECTS,
                events.len(),
            ));
        }
    }
    dispatch_scroll_events(
        &mut next,
        runtime,
        document_url,
        document_origin,
        viewport,
        &mut scroll_commands,
        &mut events,
        &mut history,
    )?;
    let mutation = NativeContentMutation {
        document: next.to_content_wire(),
        events: events
            .into_iter()
            .map(|(node, kind)| NativeContentEvent {
                node_index: node.index(),
                kind,
            })
            .collect(),
        navigation: None,
        allowed: true,
        history,
        scroll_commands,
        storage_events: Vec::new(),
        indexed_db_changes: Vec::new(),
        dialogs: Vec::new(),
        popups: Vec::new(),
        post_messages: Vec::new(),
        window_closes: Vec::new(),
        window_navigations: Vec::new(),
        window_name: String::new(),
    };
    Ok((next, mutation))
}

enum NativeFormAction {
    Clear,
    Select(String),
    Hover,
    Drag { destination_node_index: u32 },
    Upload(Vec<NativeFile>),
}

fn mutate_form_action_with_event_bridge(
    current: &NativeDocument,
    runtime: &NativeJavaScriptRuntime,
    document_url: &mut String,
    document_origin: &NativeOrigin,
    viewport: Viewport,
    node_index: u32,
    action: NativeFormAction,
) -> Result<(NativeDocument, NativeContentMutation), NativeEngineError> {
    let node_id = NativeNodeId::from_parts(current.generation(), node_index);
    let mut next = current.clone();
    let mut history = Vec::new();
    let mut scroll_commands = Vec::new();
    let mut events = match action {
        NativeFormAction::Clear => next.apply_clear(node_id)?,
        NativeFormAction::Select(value) => next.apply_select(node_id, &value)?,
        NativeFormAction::Hover => next.apply_hover(node_id)?,
        NativeFormAction::Drag {
            destination_node_index,
        } => next.apply_drag(
            node_id,
            NativeNodeId::from_parts(current.generation(), destination_node_index),
        )?,
        NativeFormAction::Upload(files) => next.apply_upload(node_id, &files)?,
    };
    let default_events = events.clone();
    for (event_node, event_kind) in default_events {
        let source = host_event_script(&[(event_node.index(), event_kind)])?;
        let Some(source) = source else {
            continue;
        };
        let evaluation =
            runtime.evaluate(&source, &next, document_url, document_origin, viewport)?;
        apply_content_event_history(
            &evaluation.commands,
            document_url,
            document_origin,
            runtime,
            &mut history,
        )?;
        scroll_commands.extend(extract_scroll_commands(&evaluation.commands));
        events.extend(next.apply_script_commands(&evaluation.commands)?);
        if events.len() > MAX_NATIVE_EFFECTS {
            return Err(NativeEngineError::limit(
                "content-process form event effects",
                MAX_NATIVE_EFFECTS,
                events.len(),
            ));
        }
    }
    dispatch_scroll_events(
        &mut next,
        runtime,
        document_url,
        document_origin,
        viewport,
        &mut scroll_commands,
        &mut events,
        &mut history,
    )?;
    let mutation = NativeContentMutation {
        document: next.to_content_wire(),
        events: events
            .into_iter()
            .map(|(node, kind)| NativeContentEvent {
                node_index: node.index(),
                kind,
            })
            .collect(),
        navigation: None,
        allowed: true,
        history,
        scroll_commands,
        storage_events: Vec::new(),
        indexed_db_changes: Vec::new(),
        dialogs: Vec::new(),
        popups: Vec::new(),
        post_messages: Vec::new(),
        window_closes: Vec::new(),
        window_navigations: Vec::new(),
        window_name: String::new(),
    };
    Ok((next, mutation))
}

fn mutate_key_with_event_bridge(
    current: &NativeDocument,
    runtime: &NativeJavaScriptRuntime,
    document_url: &mut String,
    document_origin: &NativeOrigin,
    viewport: Viewport,
    node_index: u32,
    key: &str,
) -> Result<(NativeDocument, NativeContentMutation), NativeEngineError> {
    validate_native_edit_key(key)?;
    let node_id = NativeNodeId::from_parts(current.generation(), node_index);
    if current.focused_text_control()? != node_id {
        return Err(NativeEngineError::TargetNotActionable {
            reason: "key press target is not the focused text control".into(),
        });
    }
    let mut next = current.clone();
    let mut history = Vec::new();
    let mut scroll_commands = Vec::new();
    let mut events = vec![(node_id, NativeEventKind::KeyDown)];
    let keydown_source = host_key_event_script(node_index, NativeEventKind::KeyDown, key)?
        .ok_or_else(|| NativeEngineError::Worker {
            operation: "content process keydown event bridge".into(),
            reason: "native keydown event source was empty".into(),
        })?;
    let keydown = runtime.evaluate(
        &keydown_source,
        &next,
        document_url,
        document_origin,
        viewport,
    )?;
    apply_content_event_history(
        &keydown.commands,
        document_url,
        document_origin,
        runtime,
        &mut history,
    )?;
    scroll_commands.extend(extract_scroll_commands(&keydown.commands));
    let keydown_allowed = keydown
        .value
        .as_array()
        .and_then(|values| values.first())
        .and_then(serde_json::Value::as_bool)
        .ok_or_else(|| NativeEngineError::Worker {
            operation: "content process keydown event bridge".into(),
            reason: "native keydown event result was invalid".into(),
        })?;
    events.extend(next.apply_script_commands(&keydown.commands)?);

    if keydown_allowed
        && next
            .focused_text_control()
            .is_ok_and(|focused| focused == node_id)
    {
        let input_events = next.apply_key_press(node_id, key)?;
        events.extend(input_events.clone());
        for (event_node, event_kind) in input_events {
            let source = host_event_script(&[(event_node.index(), event_kind)])?;
            let Some(source) = source else {
                continue;
            };
            let evaluation =
                runtime.evaluate(&source, &next, document_url, document_origin, viewport)?;
            apply_content_event_history(
                &evaluation.commands,
                document_url,
                document_origin,
                runtime,
                &mut history,
            )?;
            scroll_commands.extend(extract_scroll_commands(&evaluation.commands));
            events.extend(next.apply_script_commands(&evaluation.commands)?);
        }
    }

    events.push((node_id, NativeEventKind::KeyUp));
    let keyup_source =
        host_key_event_script(node_index, NativeEventKind::KeyUp, key)?.ok_or_else(|| {
            NativeEngineError::Worker {
                operation: "content process keyup event bridge".into(),
                reason: "native keyup event source was empty".into(),
            }
        })?;
    let keyup = runtime.evaluate(
        &keyup_source,
        &next,
        document_url,
        document_origin,
        viewport,
    )?;
    apply_content_event_history(
        &keyup.commands,
        document_url,
        document_origin,
        runtime,
        &mut history,
    )?;
    scroll_commands.extend(extract_scroll_commands(&keyup.commands));
    events.extend(next.apply_script_commands(&keyup.commands)?);
    dispatch_scroll_events(
        &mut next,
        runtime,
        document_url,
        document_origin,
        viewport,
        &mut scroll_commands,
        &mut events,
        &mut history,
    )?;
    if events.len() > MAX_NATIVE_EFFECTS {
        return Err(NativeEngineError::limit(
            "content-process key press effects",
            MAX_NATIVE_EFFECTS,
            events.len(),
        ));
    }
    let mutation = NativeContentMutation {
        document: next.to_content_wire(),
        events: events
            .into_iter()
            .map(|(node, kind)| NativeContentEvent {
                node_index: node.index(),
                kind,
            })
            .collect(),
        navigation: None,
        allowed: true,
        history,
        scroll_commands,
        storage_events: Vec::new(),
        indexed_db_changes: Vec::new(),
        dialogs: Vec::new(),
        popups: Vec::new(),
        post_messages: Vec::new(),
        window_closes: Vec::new(),
        window_navigations: Vec::new(),
        window_name: String::new(),
    };
    Ok((next, mutation))
}

fn mutate_key_event_with_event_bridge(
    current: &NativeDocument,
    runtime: &NativeJavaScriptRuntime,
    document_url: &mut String,
    document_origin: &NativeOrigin,
    viewport: Viewport,
    node_index: u32,
    key: &str,
    kind: NativeEventKind,
    modifiers: i64,
) -> Result<(NativeDocument, NativeContentMutation), NativeEngineError> {
    validate_native_key(key)?;
    if !matches!(kind, NativeEventKind::KeyDown | NativeEventKind::KeyUp) {
        return Err(NativeEngineError::invalid(
            "content-process key event",
            "kind must be keydown or keyup",
        ));
    }
    let node_id = NativeNodeId::from_parts(current.generation(), node_index);
    if current.focused_node() != node_id {
        return Err(NativeEngineError::TargetNotActionable {
            reason: "key event target is not the focused page target".into(),
        });
    }
    let mut next = current.clone();
    let mut history = Vec::new();
    let mut scroll_commands = Vec::new();
    let source = host_key_event_script_with_modifiers(node_index, kind, key, modifiers)?
        .ok_or_else(|| NativeEngineError::Worker {
            operation: "content process key event bridge".into(),
            reason: "native key event source was empty".into(),
        })?;
    let evaluation = runtime.evaluate(&source, &next, document_url, document_origin, viewport)?;
    apply_content_event_history(
        &evaluation.commands,
        document_url,
        document_origin,
        runtime,
        &mut history,
    )?;
    scroll_commands.extend(extract_scroll_commands(&evaluation.commands));
    let mut events = vec![(node_id, kind)];
    events.extend(next.apply_script_commands(&evaluation.commands)?);
    dispatch_scroll_events(
        &mut next,
        runtime,
        document_url,
        document_origin,
        viewport,
        &mut scroll_commands,
        &mut events,
        &mut history,
    )?;
    if events.len() > MAX_NATIVE_EFFECTS {
        return Err(NativeEngineError::limit(
            "content-process key event effects",
            MAX_NATIVE_EFFECTS,
            events.len(),
        ));
    }
    Ok((
        next.clone(),
        NativeContentMutation {
            document: next.to_content_wire(),
            events: events
                .into_iter()
                .map(|(node, kind)| NativeContentEvent {
                    node_index: node.index(),
                    kind,
                })
                .collect(),
            navigation: None,
            allowed: true,
            history,
            scroll_commands,
            storage_events: Vec::new(),
            indexed_db_changes: Vec::new(),
            dialogs: Vec::new(),
            popups: Vec::new(),
            post_messages: Vec::new(),
            window_closes: Vec::new(),
            window_navigations: Vec::new(),
            window_name: String::new(),
        },
    ))
}

fn mutate_key_shortcut_with_event_bridge(
    current: &NativeDocument,
    runtime: &NativeJavaScriptRuntime,
    document_url: &mut String,
    document_origin: &NativeOrigin,
    viewport: Viewport,
    node_index: u32,
    key: &str,
    modifiers: i64,
    apply_default: bool,
) -> Result<(NativeDocument, NativeContentMutation), NativeEngineError> {
    validate_native_key(key)?;
    if !(0..=15).contains(&modifiers) {
        return Err(NativeEngineError::invalid(
            "content-process shortcut modifiers",
            "must be an integer mask from 0 through 15",
        ));
    }
    if apply_default && !should_apply_native_key_default(key, modifiers) {
        return Err(NativeEngineError::invalid(
            "content-process shortcut default",
            "default behavior is only valid for Ctrl/Meta+A, bounded text editing, or Tab focus traversal",
        ));
    }
    let node_id = NativeNodeId::from_parts(current.generation(), node_index);
    if current.focused_node() != node_id {
        return Err(NativeEngineError::TargetNotActionable {
            reason: "shortcut target is not the focused page target".into(),
        });
    }
    let mut next = current.clone();
    let mut history = Vec::new();
    let mut scroll_commands = Vec::new();
    let keydown_source =
        host_key_event_script_with_modifiers(node_index, NativeEventKind::KeyDown, key, modifiers)?
            .ok_or_else(|| NativeEngineError::Worker {
                operation: "content process shortcut event bridge".into(),
                reason: "native shortcut keydown source was empty".into(),
            })?;
    let keydown = runtime.evaluate(
        &keydown_source,
        &next,
        document_url,
        document_origin,
        viewport,
    )?;
    apply_content_event_history(
        &keydown.commands,
        document_url,
        document_origin,
        runtime,
        &mut history,
    )?;
    scroll_commands.extend(extract_scroll_commands(&keydown.commands));
    let keydown_allowed = keydown
        .value
        .as_array()
        .and_then(|values| values.first())
        .and_then(serde_json::Value::as_bool)
        .ok_or_else(|| NativeEngineError::Worker {
            operation: "content process shortcut keydown".into(),
            reason: "native shortcut keydown result was invalid".into(),
        })?;
    let mut events = vec![(node_id, NativeEventKind::KeyDown)];
    events.extend(next.apply_script_commands(&keydown.commands)?);
    if keydown_allowed && apply_default && next.focused_node() == node_id {
        let default_events = if key == "Tab" {
            next.apply_tab_focus(modifiers & 8 != 0)?
        } else {
            next.apply_key_default(node_id, key, modifiers)?
        };
        events.extend(default_events.clone());
        for (event_node, event_kind) in default_events {
            let source = host_event_script(&[(event_node.index(), event_kind)])?;
            let Some(source) = source else {
                continue;
            };
            let evaluation =
                runtime.evaluate(&source, &next, document_url, document_origin, viewport)?;
            apply_content_event_history(
                &evaluation.commands,
                document_url,
                document_origin,
                runtime,
                &mut history,
            )?;
            scroll_commands.extend(extract_scroll_commands(&evaluation.commands));
            events.extend(next.apply_script_commands(&evaluation.commands)?);
        }
    }
    let keyup_source =
        host_key_event_script_with_modifiers(node_index, NativeEventKind::KeyUp, key, modifiers)?
            .ok_or_else(|| NativeEngineError::Worker {
            operation: "content process shortcut event bridge".into(),
            reason: "native shortcut keyup source was empty".into(),
        })?;
    let keyup = runtime.evaluate(
        &keyup_source,
        &next,
        document_url,
        document_origin,
        viewport,
    )?;
    apply_content_event_history(
        &keyup.commands,
        document_url,
        document_origin,
        runtime,
        &mut history,
    )?;
    scroll_commands.extend(extract_scroll_commands(&keyup.commands));
    events.push((node_id, NativeEventKind::KeyUp));
    events.extend(next.apply_script_commands(&keyup.commands)?);
    dispatch_scroll_events(
        &mut next,
        runtime,
        document_url,
        document_origin,
        viewport,
        &mut scroll_commands,
        &mut events,
        &mut history,
    )?;
    if events.len() > MAX_NATIVE_EFFECTS {
        return Err(NativeEngineError::limit(
            "content-process shortcut effects",
            MAX_NATIVE_EFFECTS,
            events.len(),
        ));
    }
    Ok((
        next.clone(),
        NativeContentMutation {
            document: next.to_content_wire(),
            events: events
                .into_iter()
                .map(|(node, kind)| NativeContentEvent {
                    node_index: node.index(),
                    kind,
                })
                .collect(),
            navigation: None,
            allowed: true,
            history,
            scroll_commands,
            storage_events: Vec::new(),
            indexed_db_changes: Vec::new(),
            dialogs: Vec::new(),
            popups: Vec::new(),
            post_messages: Vec::new(),
            window_closes: Vec::new(),
            window_navigations: Vec::new(),
            window_name: String::new(),
        },
    ))
}

fn should_apply_native_key_default(key: &str, modifiers: i64) -> bool {
    let primary_modifier = modifiers & (2 | 4) != 0;
    if primary_modifier {
        return key.eq_ignore_ascii_case("a") && modifiers & 1 == 0;
    }
    modifiers & 1 == 0
        && (key.chars().count() == 1
            || matches!(
                key,
                "Backspace" | "Delete" | "ArrowLeft" | "ArrowRight" | "Home" | "End" | "Tab"
            ))
}

fn split_location_navigation(
    commands: Vec<NativeScriptCommand>,
) -> Result<(Vec<NativeScriptCommand>, Option<NativeContentNavigation>), NativeEngineError> {
    let mut retained = Vec::new();
    let mut navigation = None;
    for command in commands {
        match command {
            NativeScriptCommand::Navigate { href, replace } => {
                validate_url_text("script location href", &href)?;
                if navigation.is_some() {
                    return Err(NativeEngineError::TargetNotActionable {
                        reason: "one lifecycle event cannot activate multiple navigations".into(),
                    });
                }
                navigation = Some(NativeContentNavigation {
                    node_index: 0,
                    href,
                    submitter_node_index: None,
                    location: true,
                    replace_history: replace,
                });
            }
            command => retained.push(command),
        }
    }
    Ok((retained, navigation))
}

fn extract_history_commands(commands: &[NativeScriptCommand]) -> Vec<NativeScriptCommand> {
    commands
        .iter()
        .filter(|command| {
            matches!(
                command,
                NativeScriptCommand::HistoryPushState { .. }
                    | NativeScriptCommand::HistoryReplaceState { .. }
                    | NativeScriptCommand::HistoryGo { .. }
            )
        })
        .cloned()
        .collect()
}

fn extract_scroll_commands(commands: &[NativeScriptCommand]) -> Vec<NativeScriptCommand> {
    commands
        .iter()
        .filter(|command| matches!(command, NativeScriptCommand::ScrollTo { .. }))
        .cloned()
        .collect()
}

/// Deliver scroll events for commands emitted by the page realm and apply any
/// synchronous mutations produced by those listeners. The content process is
/// the owner of page JavaScript, so scroll listeners must run here before the
/// resulting document snapshot crosses back to the parent coordinator.
fn dispatch_scroll_events(
    document: &mut NativeDocument,
    runtime: &NativeJavaScriptRuntime,
    document_url: &mut String,
    document_origin: &NativeOrigin,
    viewport: Viewport,
    pending_commands: &mut Vec<NativeScriptCommand>,
    events: &mut Vec<(NativeNodeId, NativeEventKind)>,
    history: &mut Vec<NativeScriptCommand>,
) -> Result<(), NativeEngineError> {
    let mut pending = std::mem::take(pending_commands);
    let mut cursor = 0;
    while cursor < pending.len() {
        let NativeScriptCommand::ScrollTo { node_index, .. } = pending[cursor] else {
            cursor += 1;
            continue;
        };
        let event_node_index = (node_index != 0).then_some(node_index).unwrap_or(u32::MAX);
        let source = host_event_script(&[(event_node_index, NativeEventKind::Scroll)])?
            .ok_or_else(|| NativeEngineError::Worker {
                operation: "content process scroll event".into(),
                reason: "native scroll event source was empty".into(),
            })?;
        let evaluation =
            runtime.evaluate(&source, document, document_url, document_origin, viewport)?;
        apply_content_event_history(
            &evaluation.commands,
            document_url,
            document_origin,
            runtime,
            history,
        )?;
        let new_scroll_commands = extract_scroll_commands(&evaluation.commands);
        if pending.len().saturating_add(new_scroll_commands.len()) > MAX_NATIVE_EFFECTS {
            return Err(NativeEngineError::limit(
                "content-process scroll event commands",
                MAX_NATIVE_EFFECTS,
                pending.len().saturating_add(new_scroll_commands.len()),
            ));
        }
        events.push((
            NativeNodeId::from_parts(document.generation(), event_node_index),
            NativeEventKind::Scroll,
        ));
        events.extend(document.apply_script_commands(&evaluation.commands)?);
        pending.extend(new_scroll_commands);
        if events.len() > MAX_NATIVE_EFFECTS {
            return Err(NativeEngineError::limit(
                "content-process scroll event effects",
                MAX_NATIVE_EFFECTS,
                events.len(),
            ));
        }
        cursor += 1;
    }
    *pending_commands = pending;
    Ok(())
}

fn apply_content_event_history(
    commands: &[NativeScriptCommand],
    document_url: &mut String,
    document_origin: &NativeOrigin,
    runtime: &NativeJavaScriptRuntime,
    history: &mut Vec<NativeScriptCommand>,
) -> Result<(), NativeEngineError> {
    let commands = extract_history_commands(commands);
    if commands.is_empty() {
        return Ok(());
    }
    let mut url = Some(document_url.clone());
    apply_content_runtime_history(&commands, &mut url, document_origin, runtime)?;
    *document_url = url.ok_or_else(|| NativeEngineError::Worker {
        operation: "content process history".into(),
        reason: "history did not preserve the committed document URL".into(),
    })?;
    history.extend(commands);
    Ok(())
}

fn resolve_content_history_document_url(
    commands: &[NativeScriptCommand],
    base_url: &str,
    document_origin: &NativeOrigin,
) -> Result<String, NativeEngineError> {
    let mut url = base_url.to_owned();
    for command in commands {
        match command {
            NativeScriptCommand::HistoryPushState { href, state }
            | NativeScriptCommand::HistoryReplaceState { href, state } => {
                validate_content_history_state(state)?;
                url = resolve_content_history_href(&url, document_origin, href)?;
            }
            NativeScriptCommand::HistoryGo { .. } => {}
            _ => {
                return Err(NativeEngineError::Worker {
                    operation: "content process history".into(),
                    reason: "non-history command reached history URL resolution".into(),
                });
            }
        }
    }
    Ok(url)
}

fn apply_content_runtime_history(
    commands: &[NativeScriptCommand],
    document_url: &mut Option<String>,
    document_origin: &NativeOrigin,
    runtime: &NativeJavaScriptRuntime,
) -> Result<(), NativeEngineError> {
    let Some(current_url) = document_url.as_deref() else {
        return Err(NativeEngineError::Worker {
            operation: "content process history".into(),
            reason: "history requires a committed document URL".into(),
        });
    };
    let mut base_url = current_url.to_owned();
    let mut traversal_seen = false;
    for command in commands {
        match command {
            NativeScriptCommand::HistoryPushState { href, state } => {
                if traversal_seen {
                    return Err(NativeEngineError::TargetNotActionable {
                        reason:
                            "history state mutation cannot follow traversal in one script batch"
                                .into(),
                    });
                }
                let url = resolve_content_history_href(&base_url, document_origin, href)?;
                validate_content_history_state(state)?;
                let length = runtime.history_length().saturating_add(1);
                runtime.set_history_state(state.clone());
                runtime.set_history_length(length);
                *document_url = Some(url.clone());
                base_url = url;
            }
            NativeScriptCommand::HistoryReplaceState { href, state } => {
                if traversal_seen {
                    return Err(NativeEngineError::TargetNotActionable {
                        reason:
                            "history state mutation cannot follow traversal in one script batch"
                                .into(),
                    });
                }
                let url = resolve_content_history_href(&base_url, document_origin, href)?;
                validate_content_history_state(state)?;
                runtime.set_history_state(state.clone());
                *document_url = Some(url.clone());
                base_url = url;
            }
            NativeScriptCommand::HistoryGo { delta } => {
                if traversal_seen {
                    return Err(NativeEngineError::TargetNotActionable {
                        reason: "a script batch cannot request multiple history traversals".into(),
                    });
                }
                if delta.unsigned_abs() > 1024 {
                    return Err(NativeEngineError::limit(
                        "content-process history traversal delta",
                        1024,
                        delta.unsigned_abs() as usize,
                    ));
                }
                traversal_seen = true;
            }
            _ => unreachable!("history extraction only returns history commands"),
        }
    }
    Ok(())
}

fn resolve_content_history_href(
    base_url: &str,
    document_origin: &NativeOrigin,
    href: &str,
) -> Result<String, NativeEngineError> {
    validate_url_text("content-process history URL", href)?;
    let base = Url::parse(base_url).map_err(|_| NativeEngineError::UnsupportedUrl {
        reason: "content-process history base URL is malformed".into(),
    })?;
    let target = if let Ok(absolute) = Url::parse(href) {
        absolute
    } else {
        base.join(href)
            .map_err(|_| NativeEngineError::UnsupportedUrl {
                reason: "content-process history URL is malformed".into(),
            })?
    };
    if NativeOrigin::from_url(&target)? != *document_origin {
        return Err(NativeEngineError::UnsupportedUrl {
            reason: "content-process history URL must be same-origin".into(),
        });
    }
    let target = target.to_string();
    validate_url_text("content-process history URL", &target)?;
    Ok(target)
}

fn validate_content_history_state(state: &Value) -> Result<(), NativeEngineError> {
    let encoded = serde_json::to_vec(state).map_err(|_| NativeEngineError::Worker {
        operation: "serialize content-process history state".into(),
        reason: "history state could not be serialized".into(),
    })?;
    if encoded.len() > MAX_NATIVE_HISTORY_STATE_BYTES {
        return Err(NativeEngineError::limit(
            "content-process history state",
            MAX_NATIVE_HISTORY_STATE_BYTES,
            encoded.len(),
        ));
    }
    Ok(())
}

fn mutate_before_unload(
    current: &NativeDocument,
    runtime: &NativeJavaScriptRuntime,
    document_url: &mut String,
    document_origin: &NativeOrigin,
    viewport: Viewport,
) -> Result<(NativeDocument, NativeContentMutation), NativeEngineError> {
    let source =
        host_event_script(&[(u32::MAX, NativeEventKind::BeforeUnload)])?.ok_or_else(|| {
            NativeEngineError::Worker {
                operation: "content process beforeunload".into(),
                reason: "beforeunload event source was empty".into(),
            }
        })?;
    let evaluation = runtime.evaluate(&source, current, document_url, document_origin, viewport)?;
    let allowed = evaluation
        .value
        .as_array()
        .and_then(|values| values.first())
        .and_then(Value::as_bool)
        .ok_or_else(|| NativeEngineError::Worker {
            operation: "content process beforeunload".into(),
            reason: "beforeunload event result was invalid".into(),
        })?;
    let mut next = current.clone();
    let mut history = Vec::new();
    let mut events = vec![(
        NativeNodeId::from_parts(current.generation(), u32::MAX),
        NativeEventKind::BeforeUnload,
    )];
    let mut scroll_commands = extract_scroll_commands(&evaluation.commands);
    let (commands, navigation) = split_location_navigation(evaluation.commands)?;
    apply_content_event_history(
        &commands,
        document_url,
        document_origin,
        runtime,
        &mut history,
    )?;
    events.extend(next.apply_script_commands(&commands)?);
    dispatch_scroll_events(
        &mut next,
        runtime,
        document_url,
        document_origin,
        viewport,
        &mut scroll_commands,
        &mut events,
        &mut history,
    )?;
    if events.len() > MAX_NATIVE_EFFECTS {
        return Err(NativeEngineError::limit(
            "native beforeunload effects",
            MAX_NATIVE_EFFECTS,
            events.len(),
        ));
    }
    Ok((
        next.clone(),
        NativeContentMutation {
            document: next.to_content_wire(),
            events: events
                .into_iter()
                .map(|(node, kind)| NativeContentEvent {
                    node_index: node.index(),
                    kind,
                })
                .collect(),
            navigation,
            allowed,
            history,
            scroll_commands,
            storage_events: Vec::new(),
            indexed_db_changes: Vec::new(),
            dialogs: Vec::new(),
            popups: Vec::new(),
            post_messages: Vec::new(),
            window_closes: Vec::new(),
            window_navigations: Vec::new(),
            window_name: String::new(),
        },
    ))
}

fn mutate_lifecycle_events(
    current: &NativeDocument,
    runtime: &NativeJavaScriptRuntime,
    document_url: &mut String,
    document_origin: &NativeOrigin,
    viewport: Viewport,
    kinds: &[NativeEventKind],
) -> Result<(NativeDocument, NativeContentMutation), NativeEngineError> {
    if kinds.is_empty() {
        return Ok((
            current.clone(),
            NativeContentMutation {
                document: current.to_content_wire(),
                events: Vec::new(),
                navigation: None,
                allowed: true,
                history: Vec::new(),
                scroll_commands: Vec::new(),
                storage_events: Vec::new(),
                indexed_db_changes: Vec::new(),
                dialogs: Vec::new(),
                popups: Vec::new(),
                post_messages: Vec::new(),
                window_closes: Vec::new(),
                window_navigations: Vec::new(),
                window_name: String::new(),
            },
        ));
    }
    let metadata = kinds
        .iter()
        .map(|kind| (u32::MAX, *kind))
        .collect::<Vec<_>>();
    let source = host_event_script(&metadata)?.ok_or_else(|| NativeEngineError::Worker {
        operation: "content process lifecycle events".into(),
        reason: "lifecycle event metadata was empty".into(),
    })?;
    let evaluation = runtime.evaluate(&source, current, document_url, document_origin, viewport)?;
    let mut next = current.clone();
    let mut history = Vec::new();
    let mut scroll_commands = extract_scroll_commands(&evaluation.commands);
    let (commands, navigation) = split_location_navigation(evaluation.commands)?;
    apply_content_event_history(
        &commands,
        document_url,
        document_origin,
        runtime,
        &mut history,
    )?;
    let effects = next.apply_script_commands(&commands)?;
    let mut events = kinds
        .iter()
        .copied()
        .map(|kind| NativeContentEvent {
            node_index: u32::MAX,
            kind,
        })
        .collect::<Vec<_>>();
    events.extend(effects.into_iter().map(|(node, kind)| NativeContentEvent {
        node_index: node.index(),
        kind,
    }));
    let mut event_effects = events
        .iter()
        .map(|event| {
            (
                NativeNodeId::from_parts(current.generation(), event.node_index),
                event.kind,
            )
        })
        .collect::<Vec<_>>();
    dispatch_scroll_events(
        &mut next,
        runtime,
        document_url,
        document_origin,
        viewport,
        &mut scroll_commands,
        &mut event_effects,
        &mut history,
    )?;
    events = event_effects
        .into_iter()
        .map(|(node, kind)| NativeContentEvent {
            node_index: node.index(),
            kind,
        })
        .collect();
    if events.len() > MAX_NATIVE_EFFECTS {
        return Err(NativeEngineError::limit(
            "native lifecycle effects",
            MAX_NATIVE_EFFECTS,
            events.len(),
        ));
    }
    Ok((
        next.clone(),
        NativeContentMutation {
            document: next.to_content_wire(),
            events,
            navigation,
            allowed: true,
            history,
            scroll_commands,
            storage_events: Vec::new(),
            indexed_db_changes: Vec::new(),
            dialogs: Vec::new(),
            popups: Vec::new(),
            post_messages: Vec::new(),
            window_closes: Vec::new(),
            window_navigations: Vec::new(),
            window_name: String::new(),
        },
    ))
}

fn mutate_hash_change(
    current: &NativeDocument,
    runtime: &NativeJavaScriptRuntime,
    old_url: &str,
    new_url: &mut String,
    document_origin: &NativeOrigin,
    viewport: Viewport,
) -> Result<(NativeDocument, NativeContentMutation), NativeEngineError> {
    let source = host_hash_change_event_script(old_url, new_url)?.ok_or_else(|| {
        NativeEngineError::Worker {
            operation: "content process hashchange".into(),
            reason: "hashchange event source was empty".into(),
        }
    })?;
    let evaluation = runtime.evaluate(&source, current, new_url, document_origin, viewport)?;
    let mut next = current.clone();
    let mut history = Vec::new();
    let mut scroll_commands = extract_scroll_commands(&evaluation.commands);
    let (commands, navigation) = split_location_navigation(evaluation.commands)?;
    apply_content_event_history(&commands, new_url, document_origin, runtime, &mut history)?;
    let mut events = vec![NativeContentEvent {
        node_index: u32::MAX,
        kind: NativeEventKind::HashChange,
    }];
    events.extend(
        next.apply_script_commands(&commands)?
            .into_iter()
            .map(|(node, kind)| NativeContentEvent {
                node_index: node.index(),
                kind,
            }),
    );
    let mut event_effects = events
        .iter()
        .map(|event| {
            (
                NativeNodeId::from_parts(current.generation(), event.node_index),
                event.kind,
            )
        })
        .collect::<Vec<_>>();
    dispatch_scroll_events(
        &mut next,
        runtime,
        new_url,
        document_origin,
        viewport,
        &mut scroll_commands,
        &mut event_effects,
        &mut history,
    )?;
    events = event_effects
        .into_iter()
        .map(|(node, kind)| NativeContentEvent {
            node_index: node.index(),
            kind,
        })
        .collect();
    if events.len() > MAX_NATIVE_EFFECTS {
        return Err(NativeEngineError::limit(
            "native hashchange effects",
            MAX_NATIVE_EFFECTS,
            events.len(),
        ));
    }
    Ok((
        next.clone(),
        NativeContentMutation {
            document: next.to_content_wire(),
            events,
            navigation,
            allowed: true,
            history,
            scroll_commands,
            storage_events: Vec::new(),
            indexed_db_changes: Vec::new(),
            dialogs: Vec::new(),
            popups: Vec::new(),
            post_messages: Vec::new(),
            window_closes: Vec::new(),
            window_navigations: Vec::new(),
            window_name: String::new(),
        },
    ))
}

async fn mutate_script_document(
    current: &NativeDocument,
    runtime: &NativeJavaScriptRuntime,
    document_url: &str,
    document_origin: &NativeOrigin,
    viewport: Viewport,
    commands: &[NativeScriptCommand],
    mut loader: Option<&mut NativeResourceLoader>,
) -> Result<
    (
        NativeDocument,
        NativeContentMutation,
        NativePageScriptResult,
    ),
    NativeEngineError,
> {
    let mut document_url = document_url.to_owned();
    let mut history = Vec::new();
    let mut scroll_commands = extract_scroll_commands(commands);
    let mut dialogs = Vec::new();
    let mut dynamic_navigation = None;
    let mut dynamic_result = NativePageScriptResult::default();
    let mut next = current.clone();
    let mut events = next.apply_script_commands_allowing_links(commands)?;
    next.refresh_image_loads(viewport);
    next.refresh_background_image_sources();
    let dynamic_sources = next.take_newly_attached_page_script_sources(
        commands,
        super::javascript::MAX_NATIVE_INLINE_SCRIPTS,
        MAX_NATIVE_SCRIPT_BYTES,
    );
    if !dynamic_sources.is_empty() {
        dynamic_result = if let Some(loader) = loader.as_deref_mut() {
            execute_dynamic_page_scripts_with_loader(
                &mut next,
                runtime,
                dynamic_sources,
                loader,
                &document_url,
                document_origin,
                viewport,
            )
            .await?
        } else {
            if dynamic_sources.iter().any(|source| {
                matches!(
                    source,
                    NativePageScriptSource::External { .. }
                        | NativePageScriptSource::ModuleExternal { .. }
                )
            }) {
                return Err(NativeEngineError::UnsupportedUrl {
                    reason:
                        "dynamic external/module scripts require a process-backed HTTP(S) document"
                            .into(),
                });
            }
            execute_dynamic_page_scripts(
                &mut next,
                runtime,
                page_script_sources_to_scripts(
                    dynamic_sources,
                    &document_url,
                    "glass-dynamic-module",
                ),
                &document_url,
                document_origin,
                viewport,
                &[],
            )?
        };
        if !dynamic_result.pending_script_sources.is_empty() {
            return Err(NativeEngineError::Worker {
                operation: "dynamic page script".into(),
                reason: "dynamic external/module script loader handoff was unavailable".into(),
            });
        }
        events.extend(dynamic_result.events.drain(..).map(|(node_index, kind)| {
            (
                NativeNodeId::from_parts(next.generation(), node_index),
                kind,
            )
        }));
        scroll_commands.extend(std::mem::take(&mut dynamic_result.scroll_commands));
        dialogs.extend(std::mem::take(&mut dynamic_result.dialogs));
        if let Some(page_navigation) = dynamic_result.navigation.take() {
            dynamic_navigation = Some(ScriptNavigationTarget::Location {
                href: page_navigation.href,
                replace_history: page_navigation.replace_history,
            });
        }
    }
    let validation_ids = events
        .iter()
        .filter(|(_, kind)| *kind == NativeEventKind::Invalid)
        .map(|(id, _)| *id)
        .collect::<Vec<_>>();
    if !validation_ids.is_empty()
        && let Some(source) = host_event_script(
            &validation_ids
                .iter()
                .map(|id| (id.index(), NativeEventKind::Invalid))
                .collect::<Vec<_>>(),
        )?
    {
        let evaluation =
            runtime.evaluate(&source, &next, &document_url, document_origin, viewport)?;
        apply_content_event_history(
            &evaluation.commands,
            &mut document_url,
            document_origin,
            runtime,
            &mut history,
        )?;
        scroll_commands.extend(extract_scroll_commands(&evaluation.commands));
        events.extend(next.apply_script_commands(&evaluation.commands)?);
    }
    let image_events = if let Some(loader) = loader.as_deref_mut() {
        load_external_images(&mut next, loader, &document_url, viewport).await?
    } else {
        Vec::new()
    };
    for (node_index, event_kind) in image_events {
        let Some(source) = host_event_script(&[(node_index, event_kind)])? else {
            continue;
        };
        let evaluation =
            runtime.evaluate(&source, &next, &document_url, document_origin, viewport)?;
        apply_content_event_history(
            &evaluation.commands,
            &mut document_url,
            document_origin,
            runtime,
            &mut history,
        )?;
        scroll_commands.extend(extract_scroll_commands(&evaluation.commands));
        events.extend(next.apply_script_commands(&evaluation.commands)?);
        events.push((
            NativeNodeId::from_parts(next.generation(), node_index),
            event_kind,
        ));
    }
    next.refresh_image_loads(viewport);
    next.refresh_background_image_sources();
    let mut navigation = script_navigation_target(&next, &document_url, commands)?;
    if let Some(dynamic_navigation) = dynamic_navigation {
        if navigation.is_some() {
            return Err(NativeEngineError::TargetNotActionable {
                reason: "one script batch cannot activate multiple navigations".into(),
            });
        }
        navigation = Some(dynamic_navigation);
    }
    if let Some(ScriptNavigationTarget::Form {
        form_id,
        dispatch_submit: true,
        submitter,
        ..
    }) = navigation.as_ref()
    {
        let invalid = next.invalid_form_controls(*form_id, *submitter)?;
        if invalid.is_empty() {
            if !dispatch_submit_event(
                &mut next,
                runtime,
                &mut document_url,
                document_origin,
                viewport,
                *form_id,
                *submitter,
                &mut events,
                &mut history,
                &mut scroll_commands,
            )? {
                navigation = None;
            }
        } else {
            dispatch_invalid_events(
                &mut next,
                runtime,
                &mut document_url,
                document_origin,
                viewport,
                &invalid,
                &mut events,
                &mut history,
                &mut scroll_commands,
            )?;
            navigation = None;
        }
    }
    dispatch_scroll_events(
        &mut next,
        runtime,
        &mut document_url,
        document_origin,
        viewport,
        &mut scroll_commands,
        &mut events,
        &mut history,
    )?;
    if events.len() > MAX_NATIVE_EFFECTS {
        return Err(NativeEngineError::limit(
            "content-process script mutation effects",
            MAX_NATIVE_EFFECTS,
            events.len(),
        ));
    }
    let mutation = NativeContentMutation {
        document: next.to_content_wire(),
        events: events
            .into_iter()
            .map(|(node, kind)| NativeContentEvent {
                node_index: node.index(),
                kind,
            })
            .collect(),
        navigation: navigation
            .map(|navigation| match navigation {
                ScriptNavigationTarget::Link { node_index, href } => Ok(NativeContentNavigation {
                    node_index,
                    href,
                    submitter_node_index: None,
                    location: false,
                    replace_history: false,
                }),
                ScriptNavigationTarget::Form {
                    form_id,
                    node_index,
                    submitter,
                    ..
                } => Ok(NativeContentNavigation {
                    node_index,
                    href: next
                        .form_submission_request_with_submitter(form_id, &document_url, submitter)?
                        .url,
                    submitter_node_index: submitter.map(NativeNodeId::index),
                    location: false,
                    replace_history: false,
                }),
                ScriptNavigationTarget::Location {
                    href,
                    replace_history,
                } => Ok(NativeContentNavigation {
                    node_index: 0,
                    href,
                    submitter_node_index: None,
                    location: true,
                    replace_history,
                }),
            })
            .transpose()?,
        allowed: true,
        history,
        scroll_commands,
        storage_events: Vec::new(),
        indexed_db_changes: Vec::new(),
        popups: Vec::new(),
        post_messages: Vec::new(),
        window_closes: Vec::new(),
        window_navigations: Vec::new(),
        dialogs,
        window_name: String::new(),
    };
    Ok((next, mutation, dynamic_result))
}

fn process_websocket_commands(
    commands: Vec<NativeScriptCommand>,
    connections: &mut BTreeMap<u32, NativeWebSocketConnection>,
    loader: Option<&NativeResourceLoader>,
    document_url: &str,
    document_origin: &NativeOrigin,
) -> Result<Vec<NativeScriptCommand>, NativeEngineError> {
    let mut retained = Vec::with_capacity(commands.len());
    for command in commands {
        match command {
            NativeScriptCommand::WebSocketOpen {
                socket_id,
                href,
                protocols,
                worker_id: None,
            } => {
                if connections.contains_key(&socket_id) {
                    return Err(NativeEngineError::Network {
                        operation: "WebSocket open".into(),
                        reason: "WebSocket identifier is already active".into(),
                    });
                }
                if connections.len() >= MAX_NATIVE_WEBSOCKET_EVENTS {
                    return Err(NativeEngineError::limit(
                        "native WebSocket connections",
                        MAX_NATIVE_WEBSOCKET_EVENTS,
                        connections.len().saturating_add(1),
                    ));
                }
                let loader = loader.ok_or_else(|| NativeEngineError::Worker {
                    operation: "WebSocket open".into(),
                    reason: "content process has no resource loader".into(),
                })?;
                let target = loader.websocket_target(document_url, &href)?;
                let connection = spawn_native_websocket(target, document_origin, &protocols)?;
                connections.insert(socket_id, connection);
            }
            NativeScriptCommand::WebSocketSend {
                socket_id,
                data,
                data_base64,
                worker_id: None,
            } => {
                let message = match (data, data_base64) {
                    (Some(data), None) => {
                        if data.len() > MAX_NATIVE_WEBSOCKET_MESSAGE_BYTES {
                            return Err(NativeEngineError::limit(
                                "native WebSocket message",
                                MAX_NATIVE_WEBSOCKET_MESSAGE_BYTES,
                                data.len(),
                            ));
                        }
                        Message::Text(data.into())
                    }
                    (None, Some(data_base64)) => {
                        let data = base64::engine::general_purpose::STANDARD
                            .decode(data_base64)
                            .map_err(|_| {
                                NativeEngineError::invalid(
                                    "native WebSocket binary message",
                                    "must be valid base64",
                                )
                            })?;
                        if data.len() > MAX_NATIVE_WEBSOCKET_MESSAGE_BYTES {
                            return Err(NativeEngineError::limit(
                                "native WebSocket message",
                                MAX_NATIVE_WEBSOCKET_MESSAGE_BYTES,
                                data.len(),
                            ));
                        }
                        Message::Binary(data.into())
                    }
                    _ => {
                        return Err(NativeEngineError::invalid(
                            "native WebSocket message",
                            "must contain exactly one text or binary payload",
                        ));
                    }
                };
                let connection =
                    connections
                        .get(&socket_id)
                        .ok_or_else(|| NativeEngineError::Network {
                            operation: "WebSocket send".into(),
                            reason: "WebSocket identifier is not active".into(),
                        })?;
                connection
                    .commands
                    .try_send(NativeWebSocketCommand::Send(message))
                    .map_err(|_| NativeEngineError::Network {
                        operation: "WebSocket send".into(),
                        reason: "WebSocket command queue is full or closed".into(),
                    })?;
            }
            NativeScriptCommand::WebSocketClose {
                socket_id,
                code,
                reason,
                worker_id: None,
            } => {
                if code != 1000 && !(3000..=4999).contains(&code) {
                    return Err(NativeEngineError::invalid(
                        "native WebSocket close code",
                        "must be 1000 or in the 3000-4999 range",
                    ));
                }
                if reason.len() > MAX_NATIVE_WEBSOCKET_CLOSE_REASON_BYTES {
                    return Err(NativeEngineError::limit(
                        "native WebSocket close reason",
                        MAX_NATIVE_WEBSOCKET_CLOSE_REASON_BYTES,
                        reason.len(),
                    ));
                }
                let connection =
                    connections
                        .get(&socket_id)
                        .ok_or_else(|| NativeEngineError::Network {
                            operation: "WebSocket close".into(),
                            reason: "WebSocket identifier is not active".into(),
                        })?;
                connection
                    .commands
                    .try_send(NativeWebSocketCommand::Close { code, reason })
                    .map_err(|_| NativeEngineError::Network {
                        operation: "WebSocket close".into(),
                        reason: "WebSocket command queue is full or closed".into(),
                    })?;
            }
            command => retained.push(command),
        }
    }
    Ok(retained)
}

fn process_worker_websocket_commands(
    commands: Vec<NativeWorkerWebSocketCommand>,
    connections: &mut BTreeMap<(u32, u32), NativeWebSocketConnection>,
    loader: Option<&NativeResourceLoader>,
) -> Result<(), NativeEngineError> {
    for request in commands {
        let NativeWorkerWebSocketCommand {
            worker_id,
            worker_url,
            command,
        } = request;
        match command {
            NativeScriptCommand::WebSocketOpen {
                socket_id,
                href,
                protocols,
                worker_id: Some(command_worker_id),
            } => {
                if command_worker_id != worker_id {
                    return Err(NativeEngineError::invalid(
                        "native Worker WebSocket command",
                        "WebSocket command worker id does not match its owner",
                    ));
                }
                let key = (worker_id, socket_id);
                if connections.contains_key(&key) {
                    return Err(NativeEngineError::Network {
                        operation: "Worker WebSocket open".into(),
                        reason: "WebSocket identifier is already active".into(),
                    });
                }
                if connections.len() >= MAX_NATIVE_WEBSOCKET_EVENTS {
                    return Err(NativeEngineError::limit(
                        "native Worker WebSocket connections",
                        MAX_NATIVE_WEBSOCKET_EVENTS,
                        connections.len().saturating_add(1),
                    ));
                }
                let loader = loader.ok_or_else(|| NativeEngineError::Worker {
                    operation: "Worker WebSocket open".into(),
                    reason: "content process has no resource loader".into(),
                })?;
                let target = loader.websocket_target(&worker_url, &href)?;
                let worker_url = Url::parse(without_fragment(&worker_url)).map_err(|_| {
                    NativeEngineError::UnsupportedUrl {
                        reason: "Worker WebSocket owner URL is not valid HTTP(S) syntax".into(),
                    }
                })?;
                let worker_origin = NativeOrigin::from_url(&worker_url)?;
                let connection = spawn_native_websocket(target, &worker_origin, &protocols)?;
                connections.insert(key, connection);
            }
            NativeScriptCommand::WebSocketSend {
                socket_id,
                data,
                data_base64,
                worker_id: Some(command_worker_id),
            } => {
                if command_worker_id != worker_id {
                    return Err(NativeEngineError::invalid(
                        "native Worker WebSocket command",
                        "WebSocket command worker id does not match its owner",
                    ));
                }
                let message = match (data, data_base64) {
                    (Some(data), None) => {
                        if data.len() > MAX_NATIVE_WEBSOCKET_MESSAGE_BYTES {
                            return Err(NativeEngineError::limit(
                                "native Worker WebSocket message",
                                MAX_NATIVE_WEBSOCKET_MESSAGE_BYTES,
                                data.len(),
                            ));
                        }
                        Message::Text(data.into())
                    }
                    (None, Some(data_base64)) => {
                        let data = base64::engine::general_purpose::STANDARD
                            .decode(data_base64)
                            .map_err(|_| {
                                NativeEngineError::invalid(
                                    "native Worker WebSocket binary message",
                                    "must be valid base64",
                                )
                            })?;
                        if data.len() > MAX_NATIVE_WEBSOCKET_MESSAGE_BYTES {
                            return Err(NativeEngineError::limit(
                                "native Worker WebSocket message",
                                MAX_NATIVE_WEBSOCKET_MESSAGE_BYTES,
                                data.len(),
                            ));
                        }
                        Message::Binary(data.into())
                    }
                    _ => {
                        return Err(NativeEngineError::invalid(
                            "native Worker WebSocket message",
                            "must contain exactly one text or binary payload",
                        ));
                    }
                };
                let connection = connections.get(&(worker_id, socket_id)).ok_or_else(|| {
                    NativeEngineError::Network {
                        operation: "Worker WebSocket send".into(),
                        reason: "WebSocket identifier is not active".into(),
                    }
                })?;
                connection
                    .commands
                    .try_send(NativeWebSocketCommand::Send(message))
                    .map_err(|_| NativeEngineError::Network {
                        operation: "Worker WebSocket send".into(),
                        reason: "WebSocket command queue is full or closed".into(),
                    })?;
            }
            NativeScriptCommand::WebSocketClose {
                socket_id,
                code,
                reason,
                worker_id: Some(command_worker_id),
            } => {
                if command_worker_id != worker_id {
                    return Err(NativeEngineError::invalid(
                        "native Worker WebSocket command",
                        "WebSocket command worker id does not match its owner",
                    ));
                }
                if code != 1000 && !(3000..=4999).contains(&code) {
                    return Err(NativeEngineError::invalid(
                        "native Worker WebSocket close code",
                        "must be 1000 or in the 3000-4999 range",
                    ));
                }
                if reason.len() > MAX_NATIVE_WEBSOCKET_CLOSE_REASON_BYTES {
                    return Err(NativeEngineError::limit(
                        "native Worker WebSocket close reason",
                        MAX_NATIVE_WEBSOCKET_CLOSE_REASON_BYTES,
                        reason.len(),
                    ));
                }
                let connection = connections.get(&(worker_id, socket_id)).ok_or_else(|| {
                    NativeEngineError::Network {
                        operation: "Worker WebSocket close".into(),
                        reason: "WebSocket identifier is not active".into(),
                    }
                })?;
                connection
                    .commands
                    .try_send(NativeWebSocketCommand::Close { code, reason })
                    .map_err(|_| NativeEngineError::Network {
                        operation: "Worker WebSocket close".into(),
                        reason: "WebSocket command queue is full or closed".into(),
                    })?;
            }
            _ => {
                return Err(NativeEngineError::invalid(
                    "native Worker WebSocket command",
                    "command is not a worker-owned WebSocket operation",
                ));
            }
        }
    }
    Ok(())
}

fn process_worker_event_source_commands(
    commands: Vec<NativeWorkerEventSourceCommand>,
    connections: &mut BTreeMap<(u32, u32), NativeEventSourceConnection>,
    loader: Option<&NativeResourceLoader>,
) -> Result<(), NativeEngineError> {
    for request in commands {
        let NativeWorkerEventSourceCommand {
            worker_id,
            worker_url,
            command,
        } = request;
        match command {
            NativeScriptCommand::EventSourceOpen {
                source_id,
                href,
                with_credentials,
                worker_id: Some(command_worker_id),
            } => {
                if command_worker_id != worker_id {
                    return Err(NativeEngineError::invalid(
                        "native Worker EventSource command",
                        "EventSource command worker id does not match its owner",
                    ));
                }
                let key = (worker_id, source_id);
                if connections.contains_key(&key) {
                    return Err(NativeEngineError::Network {
                        operation: "Worker EventSource open".into(),
                        reason: "EventSource identifier is already active".into(),
                    });
                }
                if connections.len() >= MAX_NATIVE_EVENTSOURCE_CONNECTIONS {
                    return Err(NativeEngineError::limit(
                        "native Worker EventSource connections",
                        MAX_NATIVE_EVENTSOURCE_CONNECTIONS,
                        connections.len().saturating_add(1),
                    ));
                }
                let loader = loader.ok_or_else(|| NativeEngineError::Worker {
                    operation: "Worker EventSource open".into(),
                    reason: "content process has no resource loader".into(),
                })?;
                validate_url_text("native Worker EventSource URL", &href)?;
                connections.insert(
                    key,
                    spawn_native_event_source(loader, &worker_url, &href, with_credentials),
                );
            }
            NativeScriptCommand::EventSourceClose {
                source_id,
                worker_id: Some(command_worker_id),
            } => {
                if command_worker_id != worker_id {
                    return Err(NativeEngineError::invalid(
                        "native Worker EventSource command",
                        "EventSource command worker id does not match its owner",
                    ));
                }
                if let Some(connection) = connections.remove(&(worker_id, source_id)) {
                    let _ = connection
                        .commands
                        .try_send(NativeEventSourceCommand::Close);
                }
            }
            _ => {
                return Err(NativeEngineError::invalid(
                    "native Worker EventSource command",
                    "command is not a worker-owned EventSource operation",
                ));
            }
        }
    }
    Ok(())
}

fn process_fetch_stream_commands(
    commands: Vec<NativeScriptCommand>,
    connections: &mut BTreeMap<u32, NativeFetchStreamConnection>,
) -> Result<Vec<NativeScriptCommand>, NativeEngineError> {
    let mut retained = Vec::with_capacity(commands.len());
    for command in commands {
        match command {
            NativeScriptCommand::FetchStreamRead { stream_id } => {
                let Some(connection) = connections.get_mut(&stream_id) else {
                    return Err(NativeEngineError::Network {
                        operation: "fetch response stream read".into(),
                        reason: "fetch response stream identifier is not active".into(),
                    });
                };
                connection.read_pending = true;
                connection
                    .commands
                    .try_send(NativeFetchStreamCommand::Read)
                    .map_err(|_| NativeEngineError::Network {
                        operation: "fetch response stream read".into(),
                        reason: "fetch response stream task is unavailable".into(),
                    })?;
            }
            NativeScriptCommand::FetchStreamCancel { stream_id } => {
                if let Some(connection) = connections.remove(&stream_id) {
                    let _ = connection
                        .commands
                        .try_send(NativeFetchStreamCommand::Cancel);
                }
            }
            command => retained.push(command),
        }
    }
    Ok(retained)
}

fn process_event_source_commands(
    commands: Vec<NativeScriptCommand>,
    connections: &mut BTreeMap<u32, NativeEventSourceConnection>,
    loader: Option<&NativeResourceLoader>,
    document_url: &str,
) -> Result<Vec<NativeScriptCommand>, NativeEngineError> {
    let mut retained = Vec::with_capacity(commands.len());
    for command in commands {
        match command {
            NativeScriptCommand::EventSourceOpen {
                source_id,
                href,
                with_credentials,
                worker_id: None,
            } => {
                if connections.contains_key(&source_id) {
                    return Err(NativeEngineError::Network {
                        operation: "EventSource open".into(),
                        reason: "EventSource identifier is already active".into(),
                    });
                }
                if connections.len() >= MAX_NATIVE_EVENTSOURCE_CONNECTIONS {
                    return Err(NativeEngineError::limit(
                        "native EventSource connections",
                        MAX_NATIVE_EVENTSOURCE_CONNECTIONS,
                        connections.len().saturating_add(1),
                    ));
                }
                let loader = loader.ok_or_else(|| NativeEngineError::Worker {
                    operation: "EventSource open".into(),
                    reason: "content process has no resource loader".into(),
                })?;
                validate_url_text("EventSource URL", &href)?;
                connections.insert(
                    source_id,
                    spawn_native_event_source(loader, document_url, &href, with_credentials),
                );
            }
            NativeScriptCommand::EventSourceClose {
                source_id,
                worker_id: None,
            } => {
                if let Some(connection) = connections.remove(&source_id) {
                    let _ = connection
                        .commands
                        .try_send(NativeEventSourceCommand::Close);
                }
            }
            command => retained.push(command),
        }
    }
    Ok(retained)
}

fn take_websocket_event(
    connections: &mut BTreeMap<u32, NativeWebSocketConnection>,
) -> Option<(u32, NativeWebSocketEvent)> {
    let socket_ids = connections.keys().copied().collect::<Vec<_>>();
    for socket_id in socket_ids {
        let Some(connection) = connections.get_mut(&socket_id) else {
            continue;
        };
        match connection.events.try_recv() {
            Ok(event) => return Some((socket_id, event)),
            Err(mpsc::error::TryRecvError::Empty) => {}
            Err(mpsc::error::TryRecvError::Disconnected) => {
                connections.remove(&socket_id);
            }
        }
    }
    None
}

fn take_worker_websocket_event(
    connections: &mut BTreeMap<(u32, u32), NativeWebSocketConnection>,
) -> Option<((u32, u32), NativeWebSocketEvent)> {
    let socket_ids = connections.keys().copied().collect::<Vec<_>>();
    for key in socket_ids {
        let Some(connection) = connections.get_mut(&key) else {
            continue;
        };
        match connection.events.try_recv() {
            Ok(event) => return Some((key, event)),
            Err(mpsc::error::TryRecvError::Empty) => {}
            Err(mpsc::error::TryRecvError::Disconnected) => {
                connections.remove(&key);
            }
        }
    }
    None
}

fn take_worker_event_source_event(
    connections: &mut BTreeMap<(u32, u32), NativeEventSourceConnection>,
) -> Option<((u32, u32), NativeEventSourceEvent)> {
    let source_ids = connections.keys().copied().collect::<Vec<_>>();
    for key in source_ids {
        let Some(connection) = connections.get_mut(&key) else {
            continue;
        };
        match connection.events.try_recv() {
            Ok(event) => return Some((key, event)),
            Err(mpsc::error::TryRecvError::Empty) => {}
            Err(mpsc::error::TryRecvError::Disconnected) => {
                connections.remove(&key);
            }
        }
    }
    None
}

async fn pump_worker_websocket_event(
    workers: &mut NativeWorkerRegistry,
    connections: &mut BTreeMap<(u32, u32), NativeWebSocketConnection>,
    loader: &mut NativeResourceLoader,
) -> Result<bool, NativeEngineError> {
    let Some(((worker_id, socket_id), event)) = take_worker_websocket_event(connections) else {
        return Ok(false);
    };
    let remove_after_dispatch = matches!(&event, NativeWebSocketEvent::Close { .. });
    workers
        .dispatch_websocket_event(
            worker_id,
            socket_id,
            &websocket_event_payload(&event),
            loader,
        )
        .await?;
    process_worker_websocket_commands(
        workers.take_websocket_commands(),
        connections,
        Some(&*loader),
    )?;
    if remove_after_dispatch {
        connections.remove(&(worker_id, socket_id));
    }
    Ok(true)
}

async fn pump_worker_event_source_event(
    workers: &mut NativeWorkerRegistry,
    connections: &mut BTreeMap<(u32, u32), NativeEventSourceConnection>,
    loader: &mut NativeResourceLoader,
) -> Result<bool, NativeEngineError> {
    let Some(((worker_id, source_id), event)) = take_worker_event_source_event(connections) else {
        return Ok(false);
    };
    if let NativeEventSourceEvent::Open { cookie_changes, .. } = &event
        && !cookie_changes.is_empty()
    {
        loader.apply_cookie_changes(cookie_changes)?;
    }
    let remove_after_dispatch = matches!(&event, NativeEventSourceEvent::Close);
    workers
        .dispatch_event_source_event(
            worker_id,
            source_id,
            &event_source_event_payload(&event),
            loader,
        )
        .await?;
    process_worker_event_source_commands(
        workers.take_event_source_commands(),
        connections,
        Some(&*loader),
    )?;
    if remove_after_dispatch {
        connections.remove(&(worker_id, source_id));
    }
    Ok(true)
}

fn take_event_source_event(
    connections: &mut BTreeMap<u32, NativeEventSourceConnection>,
) -> Option<(u32, NativeEventSourceEvent)> {
    let source_ids = connections.keys().copied().collect::<Vec<_>>();
    for source_id in source_ids {
        let Some(connection) = connections.get_mut(&source_id) else {
            continue;
        };
        match connection.events.try_recv() {
            Ok(event) => return Some((source_id, event)),
            Err(mpsc::error::TryRecvError::Empty) => {}
            Err(mpsc::error::TryRecvError::Disconnected) => {
                connections.remove(&source_id);
            }
        }
    }
    None
}

fn take_fetch_stream_event(
    connections: &mut BTreeMap<u32, NativeFetchStreamConnection>,
) -> Option<(u32, NativeFetchStreamEvent)> {
    let stream_ids = connections.keys().copied().collect::<Vec<_>>();
    for stream_id in stream_ids {
        let Some(connection) = connections.get_mut(&stream_id) else {
            continue;
        };
        match connection.events.try_recv() {
            Ok(event) => {
                connection.read_pending = false;
                return Some((stream_id, event));
            }
            Err(mpsc::error::TryRecvError::Empty) => {}
            Err(mpsc::error::TryRecvError::Disconnected) => {
                connections.remove(&stream_id);
            }
        }
    }
    None
}

fn websocket_event_payload(event: &NativeWebSocketEvent) -> Value {
    match event {
        NativeWebSocketEvent::Open { protocol } => {
            json!({"type": "open", "protocol": protocol})
        }
        NativeWebSocketEvent::MessageText { data, origin } => {
            json!({"type": "message", "data": data, "binary": false, "origin": origin})
        }
        NativeWebSocketEvent::MessageBinary { data, origin } => json!({
            "type": "message",
            "data": "",
            "dataBase64": base64::engine::general_purpose::STANDARD.encode(data),
            "binary": true,
            "origin": origin,
        }),
        NativeWebSocketEvent::Error { message } => json!({"type": "error", "message": message}),
        NativeWebSocketEvent::Close {
            code,
            reason,
            was_clean,
        } => json!({
            "type": "close",
            "code": code,
            "reason": reason,
            "wasClean": was_clean,
        }),
    }
}

fn event_source_event_payload(event: &NativeEventSourceEvent) -> Value {
    match event {
        NativeEventSourceEvent::Open { origin, .. } => {
            json!({"type": "open", "origin": origin})
        }
        NativeEventSourceEvent::Message {
            event,
            data,
            last_event_id,
            origin,
        } => json!({
            "type": "message",
            "event": event,
            "data": data,
            "lastEventId": last_event_id,
            "origin": origin,
        }),
        NativeEventSourceEvent::Error { message } => {
            json!({"type": "error", "message": message})
        }
        NativeEventSourceEvent::Close => json!({"type": "close"}),
    }
}

fn fetch_stream_event_payload(event: &NativeFetchStreamEvent) -> Value {
    match event {
        NativeFetchStreamEvent::Chunk { data } => json!({
            "type": "chunk",
            "dataBase64": base64::engine::general_purpose::STANDARD.encode(data),
        }),
        NativeFetchStreamEvent::End => json!({"type": "end"}),
        NativeFetchStreamEvent::Error { message } => {
            json!({"type": "error", "message": message})
        }
    }
}

fn fetch_commands(
    commands: &[NativeScriptCommand],
) -> Result<
    Vec<(
        u32,
        String,
        NativeNavigationMethod,
        BTreeMap<String, String>,
        Option<NativeRequestBody>,
        Option<String>,
        NativeCorsMode,
        NativeFetchRedirectMode,
        Option<Duration>,
        bool,
    )>,
    NativeEngineError,
> {
    commands
        .iter()
        .filter_map(|command| match command {
            NativeScriptCommand::Fetch {
                request_id,
                href,
                credentials,
                method,
                headers,
                body,
                body_base64,
                content_type,
                mode,
                redirect,
                timeout_ms,
                ..
            } => Some((
                *request_id,
                href.clone(),
                method,
                headers.clone(),
                body.clone(),
                body_base64.clone(),
                content_type.clone(),
                mode.clone(),
                redirect.clone(),
                *timeout_ms,
                *credentials,
            )),
            _ => None,
        })
        .map(
            |(
                request_id,
                href,
                method,
                headers,
                body,
                body_base64,
                content_type,
                mode,
                redirect,
                timeout_ms,
                credentials,
            )| {
                if timeout_ms.is_some_and(|value| value > MAX_NATIVE_XHR_TIMEOUT_MS) {
                    return Err(NativeEngineError::invalid(
                        "script fetch timeout",
                        "must not exceed the native XHR timeout limit",
                    ));
                }
                let timeout = timeout_ms.map(|value| Duration::from_millis(u64::from(value)));
                let cors_mode = match mode.as_deref().unwrap_or("cors") {
                    "cors" => NativeCorsMode::Cors,
                    "no-cors" => NativeCorsMode::NoCors,
                    "same-origin" => NativeCorsMode::SameOrigin,
                    _ => {
                        return Err(NativeEngineError::invalid(
                            "script fetch mode",
                            "must be cors, no-cors, or same-origin",
                        ));
                    }
                };
                let redirect_mode = match redirect.as_deref().unwrap_or("follow") {
                    "follow" => NativeFetchRedirectMode::Follow,
                    "error" => NativeFetchRedirectMode::Error,
                    "manual" => NativeFetchRedirectMode::Manual,
                    _ => {
                        return Err(NativeEngineError::invalid(
                            "script fetch redirect mode",
                            "must be follow, error, or manual",
                        ));
                    }
                };
                let method =
                    NativeNavigationMethod::from_fetch_method(method.as_str()).map_err(|_| {
                        NativeEngineError::invalid(
                            "script fetch method",
                            "must be GET, HEAD, POST, PUT, PATCH, DELETE, or OPTIONS",
                        )
                    })?;
                let body = match body_base64 {
                    Some(encoded) => {
                        let decoded = base64::engine::general_purpose::STANDARD
                            .decode(encoded)
                            .map_err(|_| {
                                NativeEngineError::invalid(
                                    "script fetch binary body",
                                    "must be valid base64",
                                )
                            })?;
                        if decoded.len() > MAX_NATIVE_FORM_BODY_BYTES {
                            return Err(NativeEngineError::limit(
                                "script fetch binary body",
                                MAX_NATIVE_FORM_BODY_BYTES,
                                decoded.len(),
                            ));
                        }
                        Some(NativeRequestBody::Bytes(decoded))
                    }
                    None => body.map(NativeRequestBody::Text),
                };
                Ok((
                    request_id,
                    href,
                    method,
                    headers,
                    body,
                    content_type,
                    cors_mode,
                    redirect_mode,
                    timeout,
                    credentials,
                ))
            },
        )
        .collect()
}

fn fetch_response_payload(result: Result<NativeFetchResponse, NativeEngineError>) -> Value {
    match result {
        Ok(response) => json!({
            "url": response.url,
            "status": response.status,
            "contentType": response.content_type,
            "headers": response.headers,
            "body": String::from_utf8_lossy(&response.body),
            "bodyBase64": base64::engine::general_purpose::STANDARD.encode(&response.body),
            "redirected": response.redirected,
            "opaque": response.opaque,
            "opaqueRedirect": response.opaque_redirect,
        }),
        Err(error) => json!({
            "error": error.to_string(),
            "timeout": matches!(
                error,
                NativeEngineError::Network { reason, .. } if reason == "request timed out"
            ),
        }),
    }
}

fn fetch_stream_response_payload(response: NativeFetchResponse, stream_id: u32) -> Value {
    let mut payload = fetch_response_payload(Ok(response));
    if let Some(object) = payload.as_object_mut() {
        object.insert("bodyStreamId".into(), Value::from(stream_id));
    }
    payload
}

async fn collect_native_fetch_response(
    opened: NativeFetchResponseStream,
) -> Result<NativeFetchResponse, NativeEngineError> {
    let mut stream = opened.body.bytes_stream();
    let mut body = Vec::new();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|error| NativeEngineError::Network {
            operation: "fetch response body".into(),
            reason: error.to_string(),
        })?;
        let next_len = body.len().saturating_add(chunk.len());
        if next_len > opened.max_response_bytes {
            return Err(NativeEngineError::limit(
                "fetch response",
                opened.max_response_bytes,
                next_len,
            ));
        }
        body.extend_from_slice(&chunk);
    }
    let mut response = opened.response;
    response.body = body;
    Ok(response)
}

/// Activate network commands emitted by a dynamically attached script. The
/// DOM/event effects are already committed by `mutate_script_document`; this
/// handoff only installs persistent transports and returns fetches to the
/// existing resolver queue.
fn activate_dynamic_page_script_network(
    result: NativePageScriptResult,
    websocket_connections: &mut BTreeMap<u32, NativeWebSocketConnection>,
    event_source_connections: &mut BTreeMap<u32, NativeEventSourceConnection>,
    loader: Option<&NativeResourceLoader>,
    document_url: &str,
    document_origin: &NativeOrigin,
) -> Result<(Vec<NativeScriptCommand>, bool), NativeEngineError> {
    if !result.pending_script_sources.is_empty() {
        return Err(NativeEngineError::Worker {
            operation: "dynamic page script".into(),
            reason: "dynamic external/module script sources were not fully loaded".into(),
        });
    }
    let has_background_transport =
        !result.websocket_commands.is_empty() || !result.event_source_commands.is_empty();
    let retained_websocket = process_websocket_commands(
        result.websocket_commands,
        websocket_connections,
        loader,
        document_url,
        document_origin,
    )?;
    if !retained_websocket.is_empty() {
        return Err(NativeEngineError::Worker {
            operation: "dynamic page script WebSocket commands".into(),
            reason: "dynamic WebSocket command handoff retained an unexpected command".into(),
        });
    }
    let retained_event_source = process_event_source_commands(
        result.event_source_commands,
        event_source_connections,
        loader,
        document_url,
    )?;
    if !retained_event_source.is_empty() {
        return Err(NativeEngineError::Worker {
            operation: "dynamic page script EventSource commands".into(),
            reason: "dynamic EventSource command handoff retained an unexpected command".into(),
        });
    }
    Ok((result.pending_fetches, has_background_transport))
}

async fn resolve_script_fetches(
    current: &NativeDocument,
    runtime: &NativeJavaScriptRuntime,
    mut loader: Option<&mut NativeResourceLoader>,
    websocket_connections: &mut BTreeMap<u32, NativeWebSocketConnection>,
    fetch_stream_connections: &mut BTreeMap<u32, NativeFetchStreamConnection>,
    event_source_connections: &mut BTreeMap<u32, NativeEventSourceConnection>,
    document_url: &str,
    document_origin: &NativeOrigin,
    viewport: Viewport,
    mut pump_background_events: bool,
    evaluation: NativeScriptEvaluation,
) -> Result<(NativeDocument, NativeContentMutation, Option<Value>), NativeEngineError> {
    let NativeScriptEvaluation {
        commands: initial_commands,
        top_level_await_pending,
        ..
    } = evaluation;
    let mut current_url = document_url.to_owned();
    let initial_commands = process_websocket_commands(
        initial_commands,
        websocket_connections,
        loader.as_deref(),
        &current_url,
        document_origin,
    )?;
    let initial_commands = process_event_source_commands(
        initial_commands,
        event_source_connections,
        loader.as_deref(),
        &current_url,
    )?;
    let initial_commands =
        process_fetch_stream_commands(initial_commands, fetch_stream_connections)?;
    let (mut next, mut mutation, dynamic_result) = mutate_script_document(
        current,
        runtime,
        &current_url,
        document_origin,
        viewport,
        &initial_commands,
        loader.as_deref_mut(),
    )
    .await?;
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
        document_origin,
    )?;
    pending.extend(fetch_commands(&dynamic_fetches)?);
    pump_background_events |= dynamic_background;
    let mut resolved_count = 0usize;
    let mut resolved_value = None;
    let mut event_loop_turns = 0usize;
    loop {
        while let Some((
            request_id,
            href,
            method,
            headers,
            body,
            content_type,
            cors_mode,
            redirect_mode,
            timeout,
            credentials,
        )) = pending.pop()
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
            let opened = loader
                .open_fetch_response_stream_async(NativeFetchRequest {
                    document_url: &current_url,
                    href: &href,
                    method,
                    body,
                    content_type,
                    request_headers: headers,
                    cors_mode,
                    redirect_mode,
                    timeout,
                    credentials,
                    max_response_bytes: None,
                })
                .await;
            let payload = match opened {
                Ok(opened) if !opened.response.opaque && !opened.response.opaque_redirect => {
                    if fetch_stream_connections.contains_key(&request_id) {
                        fetch_response_payload(Err(NativeEngineError::Network {
                            operation: "fetch response stream".into(),
                            reason: "fetch response stream identifier is already active".into(),
                        }))
                    } else {
                        let response = opened.response;
                        let stream =
                            spawn_native_fetch_stream(opened.body, opened.max_response_bytes);
                        fetch_stream_connections.insert(request_id, stream);
                        fetch_stream_response_payload(response, request_id)
                    }
                }
                Ok(opened) => fetch_response_payload(collect_native_fetch_response(opened).await),
                Err(error) => fetch_response_payload(Err(error)),
            };
            let resolved = runtime.resolve_fetch(
                request_id,
                &payload,
                &next,
                &current_url,
                document_origin,
                viewport,
            )?;
            if top_level_await_pending && let Some(value) = runtime.take_top_level_await_result()? {
                resolved_value = Some(value);
            }
            let resolved_commands = process_websocket_commands(
                resolved.commands,
                websocket_connections,
                Some(&*loader),
                &current_url,
                document_origin,
            )?;
            let resolved_commands = process_event_source_commands(
                resolved_commands,
                event_source_connections,
                Some(&*loader),
                &current_url,
            )?;
            let resolved_commands =
                process_fetch_stream_commands(resolved_commands, fetch_stream_connections)?;
            let resolved_history = extract_history_commands(&resolved_commands);
            if !resolved_history.is_empty() {
                let mut url = Some(current_url.clone());
                apply_content_runtime_history(
                    &resolved_history,
                    &mut url,
                    document_origin,
                    runtime,
                )?;
                current_url = url.ok_or_else(|| NativeEngineError::Worker {
                    operation: "content process fetch history".into(),
                    reason: "fetch callback history lost its document URL".into(),
                })?;
            }
            let (resolved_next, resolved_mutation, dynamic_result) = mutate_script_document(
                &next,
                runtime,
                &current_url,
                document_origin,
                viewport,
                &resolved_commands,
                Some(&mut *loader),
            )
            .await?;
            next = resolved_next;
            mutation.events.extend(resolved_mutation.events);
            mutation.history.extend(resolved_history);
            mutation
                .scroll_commands
                .extend(resolved_mutation.scroll_commands);
            if !resolved_mutation.history.is_empty() {
                current_url = resolve_content_history_document_url(
                    &resolved_mutation.history,
                    &current_url,
                    document_origin,
                )?;
                mutation.history.extend(resolved_mutation.history);
            }
            if mutation.navigation.is_none() {
                mutation.navigation = resolved_mutation.navigation;
            } else if resolved_mutation.navigation.is_some() {
                return Err(NativeEngineError::TargetNotActionable {
                    reason:
                        "one script event-loop fetch batch cannot activate multiple navigations"
                            .into(),
                });
            }
            let (dynamic_fetches, dynamic_background) = activate_dynamic_page_script_network(
                dynamic_result,
                websocket_connections,
                event_source_connections,
                Some(&*loader),
                &current_url,
                document_origin,
            )?;
            pending.extend(fetch_commands(&resolved_commands)?);
            pending.extend(fetch_commands(&dynamic_fetches)?);
            pump_background_events |= dynamic_background;
        }
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
        if pump_background_events
            && let Some((socket_id, event)) = take_websocket_event(websocket_connections)
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
                &next,
                &current_url,
                document_origin,
                viewport,
            )?;
            let event_commands = process_websocket_commands(
                event_evaluation.commands,
                websocket_connections,
                loader.as_deref(),
                &current_url,
                document_origin,
            )?;
            let event_commands =
                process_fetch_stream_commands(event_commands, fetch_stream_connections)?;
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
                document_origin,
            )?;
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
        if pump_fetch_streams
            && let Some((stream_id, event)) = take_fetch_stream_event(fetch_stream_connections)
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
                loader.as_deref(),
                &current_url,
                document_origin,
            )?;
            let event_commands = process_event_source_commands(
                event_commands,
                event_source_connections,
                loader.as_deref(),
                &current_url,
            )?;
            let event_commands =
                process_fetch_stream_commands(event_commands, fetch_stream_connections)?;
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
                document_origin,
            )?;
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
        if fetch_stream_connections
            .values()
            .any(|connection| connection.read_pending)
        {
            sleep(NATIVE_WEBSOCKET_POLL_INTERVAL).await;
            continue;
        }
        if pump_background_events
            && let Some((source_id, event)) = take_event_source_event(event_source_connections)
        {
            event_loop_turns = event_loop_turns.saturating_add(1);
            if event_loop_turns > MAX_CONTENT_EVENT_LOOP_TURNS {
                return Err(NativeEngineError::limit(
                    "content-process event-loop turns",
                    MAX_CONTENT_EVENT_LOOP_TURNS,
                    event_loop_turns,
                ));
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
                &next,
                &current_url,
                document_origin,
                viewport,
            )?;
            let event_commands = process_websocket_commands(
                event_evaluation.commands,
                websocket_connections,
                loader.as_deref(),
                &current_url,
                document_origin,
            )?;
            let event_commands = process_event_source_commands(
                event_commands,
                event_source_connections,
                loader.as_deref(),
                &current_url,
            )?;
            let event_commands =
                process_fetch_stream_commands(event_commands, fetch_stream_connections)?;
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
                document_origin,
            )?;
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
            loader.as_deref(),
            &current_url,
            document_origin,
        )?;
        let timer_commands = process_event_source_commands(
            timer_commands,
            event_source_connections,
            loader.as_deref(),
            &current_url,
        )?;
        let timer_commands =
            process_fetch_stream_commands(timer_commands, fetch_stream_connections)?;
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
            document_origin,
        )?;
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
                    })
                } else {
                    None
                }
            }
            NativeScriptCommand::SubmitForm { node_index } => {
                let node_id = NativeNodeId::from_parts(document.generation(), *node_index);
                document.form_submission_request(node_id, document_url)?;
                Some(ScriptNavigationTarget::Form {
                    node_index: *node_index,
                    form_id: node_id,
                    dispatch_submit: false,
                    submitter: None,
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
        NativeEventKind::Focus => "focus",
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
        "focus" => Some(NativeEventKind::Focus),
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
    json!({
        "kind": "error",
        "id": id,
        "reason": error.to_string(),
    })
}

fn protocol_matches(request: &Value) -> bool {
    request.get("protocol").and_then(Value::as_u64) == Some(CONTENT_WORKER_PROTOCOL_VERSION)
}

fn worker_binary_path() -> Result<PathBuf, NativeEngineError> {
    if let Ok(path) = std::env::var("GLASS_NATIVE_CONTENT_WORKER") {
        if !path.is_empty() {
            return Ok(PathBuf::from(path));
        }
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
