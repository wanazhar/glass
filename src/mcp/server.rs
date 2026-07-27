//! MCP JSON-RPC 2.0 stdio server.
//!
//! Implements the Model Context Protocol (2024-11-05) over stdin/stdout,
//! providing browser automation tools with policy-gated execution, bounded
//! response sizes, and concurrent request handling.

use serde::{Deserialize, Deserializer, Serialize};
use serde_json::{Value, json};
use std::{
    borrow::Cow,
    collections::HashMap,
    io,
    sync::{Arc, Mutex as StdMutex},
    time::Duration,
};
use tokio::io::{
    AsyncBufRead, AsyncBufReadExt, AsyncReadExt, AsyncWrite, AsyncWriteExt, BufReader,
};
use tokio::sync::{Mutex, Semaphore, mpsc, oneshot};
use tracing::{debug, info};

use crate::browser::cdp::CdpError;
use crate::browser::policy::{BrowserPolicy, PolicyError};
use crate::browser::session::{
    ActionContractError, ActionKind, ActionOutcome, ActionVerificationError, BatchStep,
    BrowserResult, BrowserSession, CheckpointV1, DownloadError, Locator, PopupClickError,
    PreflightAction, ReconciliationOptions, SessionOptions, TargetError, VisualCaptureOptions,
    VisualClip, VisualFormat, WaitCondition, WaitTimeout,
};
use crate::cli::args::Cli;
use crate::mcp::prompts;
use crate::mcp::resources;

const MCP_PROTOCOL_VERSION: &str = "2024-11-05";
const MAX_HEADER_BYTES: usize = 8 * 1024;
const MAX_MESSAGE_BYTES: usize = 4 * 1024 * 1024;
const MAX_RESPONSE_BYTES: usize = 32 * 1024 * 1024;
const MAX_CONCURRENT_REQUESTS: usize = 8;
const MAX_QUEUED_RESPONSES: usize = 16;
const FRAME_BODY_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Debug, Deserialize)]
struct JsonRpcRequest {
    jsonrpc: String,
    method: String,
    #[serde(default)]
    params: Value,
    #[serde(default)]
    id: RequestId,
}

#[derive(Debug, Default)]
enum RequestId {
    #[default]
    Missing,
    Present(Value),
}

impl<'de> Deserialize<'de> for RequestId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        Ok(Self::Present(Value::deserialize(deserializer)?))
    }
}

impl RequestId {
    fn is_notification(&self) -> bool {
        matches!(self, Self::Missing)
    }

    fn response_value(&self) -> Option<Value> {
        match self {
            Self::Missing => None,
            Self::Present(value) => Some(value.clone()),
        }
    }

    fn cancellation_key(&self) -> Option<String> {
        match self {
            Self::Present(value @ (Value::String(_) | Value::Number(_))) => Some(value.to_string()),
            Self::Missing | Self::Present(_) => None,
        }
    }

    fn is_valid(&self) -> bool {
        matches!(
            self,
            Self::Missing | Self::Present(Value::Null | Value::String(_) | Value::Number(_))
        )
    }
}

#[derive(Debug, Serialize)]
struct JsonRpcResponse {
    jsonrpc: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<JsonRpcError>,
    #[serde(skip_serializing_if = "Option::is_none")]
    id: Option<Value>,
}

#[derive(Debug, Serialize)]
struct JsonRpcError {
    code: i32,
    message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    data: Option<Value>,
}

#[derive(Debug, Serialize)]
struct Tool {
    name: &'static str,
    description: &'static str,
    #[serde(rename = "inputSchema")]
    input_schema: Value,
}

#[derive(Debug, PartialEq, Eq)]
enum FrameFormat {
    ContentLength,
    Newline,
}

#[derive(Debug, PartialEq, Eq)]
struct RequestLogMetadata<'a> {
    method: &'a str,
    request_id_kind: &'static str,
    request_id_present: bool,
    body_bytes: usize,
}

enum ToolInvocation<'a> {
    Navigate {
        url: &'a str,
        timeout_ms: u64,
        expected_revision: Option<u64>,
    },
    Click {
        target: Cow<'a, str>,
        expected_revision: Option<u64>,
    },
    Preflight {
        target: Cow<'a, str>,
        action: PreflightAction,
    },
    ClickAt {
        x: f64,
        y: f64,
    },
    ClickExpectPopup {
        target: Cow<'a, str>,
    },
    DoubleClick {
        target: Cow<'a, str>,
    },
    Hover {
        target: Cow<'a, str>,
    },
    Drag {
        source: Cow<'a, str>,
        destination: Cow<'a, str>,
    },
    Type {
        text: &'a str,
        target: Option<&'a str>,
        expected_revision: Option<u64>,
    },
    Key {
        key: &'a str,
    },
    KeyDown {
        key: &'a str,
    },
    KeyUp {
        key: &'a str,
    },
    Shortcut {
        shortcut: &'a str,
    },
    Clear {
        target: Cow<'a, str>,
    },
    Check {
        target: Cow<'a, str>,
    },
    Uncheck {
        target: Cow<'a, str>,
    },
    Select {
        target: Cow<'a, str>,
        value: &'a str,
    },
    Upload {
        target: Cow<'a, str>,
        files: Vec<std::path::PathBuf>,
    },
    Screenshot {
        format: VisualFormat,
        quality: Option<u8>,
        scale: f64,
        full_page: bool,
        clip: Option<VisualClip>,
        target: Option<String>,
    },
    Observe {
        include_dom: bool,
        include_screenshot: bool,
        include_form_values: bool,
    },
    GetDom,
    GetText,
    Evaluate {
        expression: &'a str,
    },
    Batch {
        steps: Value,
        atomic: bool,
    },
    ReconcileReferences {
        from_revision: u64,
        refs: Vec<String>,
        hints: Vec<String>,
        scope_ref: Option<String>,
    },
    ObserveDelta,
    SetNetworkConditions {
        preset: Option<String>,
        offline: bool,
        latency_ms: f64,
        download_throughput: f64,
        upload_throughput: f64,
    },
    ClearNetworkConditions,
    SetCpuThrottling {
        rate: f64,
    },
    ClearCpuThrottling,
    SetUserAgent {
        user_agent: String,
        accept_language: Option<String>,
        platform: Option<String>,
    },
    ClearUserAgent,
    ExportCheckpoint,
    ImportCheckpoint {
        checkpoint: Value,
    },
    Scroll {
        dx: f64,
        dy: f64,
    },
    Wait {
        condition: &'a str,
        timeout_ms: u64,
    },
    Diagnostics {
        duration_ms: u64,
    },
    AcceptDialog,
    DismissDialog,
    DismissConsent,
    Download {
        destination: std::path::PathBuf,
        timeout_ms: u64,
    },
    ListTargets,
    CreateTarget {
        url: &'a str,
    },
    SelectTarget {
        id: &'a str,
    },
    CloseTarget {
        id: &'a str,
    },
    ListFrames,
    SelectFrame {
        id: &'a str,
    },
    Cookies,
    SetCookies {
        cookies: Value,
    },
    ClearCookies,
    LocalStorage,
    SessionStorage,
    PrintToPdf {
        options: serde_json::Value,
    },
    FillForm {
        fields: Vec<(String, String)>,
        expected_revision: Option<u64>,
    },
    ClipboardRead,
    ClipboardWrite {
        text: String,
    },
    SetGeolocation {
        latitude: f64,
        longitude: f64,
    },
    ClearGeolocation,
    SetTimezone {
        timezone_id: String,
    },
}

struct Outbound {
    response: JsonRpcResponse,
    format: FrameFormat,
}

type CancellationMap = Arc<StdMutex<HashMap<String, oneshot::Sender<()>>>>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Lifecycle {
    Uninitialized,
    Negotiated,
    Ready,
}

pub async fn run_mcp_server(cli: &Cli) -> BrowserResult<()> {
    let local = tokio::task::LocalSet::new();
    local.run_until(run_mcp_server_local(cli)).await
}

async fn run_mcp_server_local(cli: &Cli) -> BrowserResult<()> {
    info!("MCP server starting on stdio");
    let options = SessionOptions {
        port: cli.port,
        chrome_path: cli.chrome_path.clone(),
        profile: cli.profile.clone(),
        incognito: cli.incognito,
        attach: cli.attach,
        target_id: cli.target_id.clone(),
        frame_id: cli.frame_id.clone(),
        headed: cli.headed,
        interaction_mode: cli.interaction,
        audit: cli.audit,
        policy: None,
    };
    let policy = crate::cli::runner::policy_from_cli(cli)?;
    let stdin = tokio::io::stdin();
    let mut reader = BufReader::new(stdin);
    let stdout = tokio::io::stdout();
    let session = Arc::new(Mutex::new(None));
    let cancellations: CancellationMap = Arc::new(StdMutex::new(HashMap::new()));
    let permits = Arc::new(Semaphore::new(MAX_CONCURRENT_REQUESTS));
    let (outbound_tx, mut outbound_rx) = mpsc::channel::<Outbound>(MAX_QUEUED_RESPONSES);
    let writer = tokio::spawn(async move {
        let mut stdout = stdout;
        while let Some(outbound) = outbound_rx.recv().await {
            write_response(&mut stdout, &outbound.response, outbound.format).await?;
        }
        Ok::<(), io::Error>(())
    });
    let mut lifecycle = Lifecycle::Uninitialized;

    while let Some((body, format)) = read_message(&mut reader).await? {
        let body_bytes = body.len();
        let request: JsonRpcRequest = match serde_json::from_str(&body) {
            Ok(request) => request,
            Err(error) => {
                debug!(body_bytes, "MCP request rejected: invalid JSON");
                let response =
                    error_response(Some(Value::Null), -32700, format!("parse error: {error}"));
                send_response(&outbound_tx, response, format).await?;
                continue;
            }
        };
        let log = request_log_metadata(&request, body_bytes);
        debug!(
            method = log.method,
            request_id_kind = log.request_id_kind,
            request_id_present = log.request_id_present,
            body_bytes = log.body_bytes,
            "MCP request received"
        );

        if !request.id.is_valid() {
            send_response(
                &outbound_tx,
                error_response(Some(Value::Null), -32600, "invalid JSON-RPC request id"),
                format,
            )
            .await?;
            continue;
        }
        if request.jsonrpc != "2.0" {
            if !request.id.is_notification() {
                send_response(
                    &outbound_tx,
                    error_response(request.id.response_value(), -32600, "jsonrpc must be 2.0"),
                    format,
                )
                .await?;
            }
            continue;
        }
        if request.method == "notifications/cancelled" && request.id.is_notification() {
            cancel_request(&request, &cancellations);
            continue;
        }
        if request.method == "initialize" {
            if request.id.is_notification() {
                continue;
            }
            if lifecycle != Lifecycle::Uninitialized {
                send_response(
                    &outbound_tx,
                    error_response(
                        request.id.response_value(),
                        -32600,
                        "initialize may only be requested once",
                    ),
                    format,
                )
                .await?;
                continue;
            }
            let response = initialize_response(&request);
            if response.error.is_none() {
                lifecycle = Lifecycle::Negotiated;
            }
            send_response(&outbound_tx, response, format).await?;
            continue;
        }
        if request.id.is_notification() && request.method == "notifications/initialized" {
            if lifecycle == Lifecycle::Negotiated {
                lifecycle = Lifecycle::Ready;
            }
            continue;
        }
        if lifecycle != Lifecycle::Ready {
            if !request.id.is_notification() {
                send_response(
                    &outbound_tx,
                    error_response(
                        request.id.response_value(),
                        -32002,
                        "server is not initialized",
                    ),
                    format,
                )
                .await?;
            }
            continue;
        }

        let permit = match Arc::clone(&permits).try_acquire_owned() {
            Ok(permit) => permit,
            Err(_) => {
                if !request.id.is_notification() {
                    send_response(
                        &outbound_tx,
                        error_response(
                            request.id.response_value(),
                            -32000,
                            "too many concurrent requests",
                        ),
                        format,
                    )
                    .await?;
                }
                continue;
            }
        };
        let cancellation_key = request.id.cancellation_key();
        let (cancel_tx, cancel_rx) = oneshot::channel();
        let mut cancel_guard = Some(cancel_tx);
        if let Some(key) = cancellation_key.as_ref() {
            let duplicate = {
                let mut active = cancellations.lock().expect("cancellation map poisoned");
                if active.contains_key(key) {
                    true
                } else {
                    active.insert(key.clone(), cancel_guard.take().expect("sender available"));
                    false
                }
            };
            if duplicate {
                send_response(
                    &outbound_tx,
                    error_response(
                        request.id.response_value(),
                        -32600,
                        "duplicate active request id",
                    ),
                    format,
                )
                .await?;
                continue;
            }
        }
        let task_session = Arc::clone(&session);
        let task_options = options.clone();
        let task_policy = policy.clone();
        let task_outbound = outbound_tx.clone();
        let task_cancellations = Arc::clone(&cancellations);
        tokio::task::spawn_local(async move {
            let _permit = permit;
            let _cancel_guard = cancel_guard;
            let is_notification = request.id.is_notification();
            let id = request.id.response_value();
            let operation = async {
                let mut session = task_session.lock().await;
                handle_request(&request, &mut session, &task_options, &task_policy).await
            };
            let mut response = tokio::select! {
                response = operation => response,
                _ = cancel_rx => Some(error_response(id, -32800, "request cancelled")),
            };
            if is_notification {
                response = None;
            }
            if let Some(key) = cancellation_key {
                task_cancellations
                    .lock()
                    .expect("cancellation map poisoned")
                    .remove(&key);
            }
            if let Some(response) = response {
                let _ = send_response(&task_outbound, response, format).await;
            }
        });
    }

    // EOF is a graceful client shutdown: allow already accepted requests to
    // finish and flush their responses before closing the owned browser.
    drop(outbound_tx);
    writer.await??;
    let mut session = session.lock().await;
    if let Some(session) = session.take() {
        session.close().await?;
    }
    Ok(())
}

async fn send_response(
    sender: &mpsc::Sender<Outbound>,
    response: JsonRpcResponse,
    format: FrameFormat,
) -> io::Result<()> {
    sender
        .send(Outbound { response, format })
        .await
        .map_err(|_| io::Error::new(io::ErrorKind::BrokenPipe, "MCP output task stopped"))
}

fn request_log_metadata(request: &JsonRpcRequest, body_bytes: usize) -> RequestLogMetadata<'_> {
    let request_id_kind = match &request.id {
        RequestId::Missing => "absent",
        RequestId::Present(Value::Null) => "null",
        RequestId::Present(Value::String(_)) => "string",
        RequestId::Present(Value::Number(_)) => "number",
        RequestId::Present(_) => "invalid",
    };
    RequestLogMetadata {
        method: &request.method,
        request_id_kind,
        request_id_present: !request.id.is_notification(),
        body_bytes,
    }
}

fn request_id_key(id: &Value) -> Option<String> {
    matches!(id, Value::String(_) | Value::Number(_)).then(|| id.to_string())
}

fn cancel_request(request: &JsonRpcRequest, cancellations: &CancellationMap) {
    let Some(key) = request.params.get("requestId").and_then(request_id_key) else {
        return;
    };
    if let Some(cancellation) = cancellations
        .lock()
        .expect("cancellation map poisoned")
        .remove(&key)
    {
        let _ = cancellation.send(());
    }
}

fn initialize_response(request: &JsonRpcRequest) -> JsonRpcResponse {
    if request.jsonrpc != "2.0" {
        return error_response(request.id.response_value(), -32600, "jsonrpc must be 2.0");
    }
    let Some(version) = request
        .params
        .get("protocolVersion")
        .and_then(Value::as_str)
    else {
        return error_response(
            request.id.response_value(),
            -32602,
            "protocolVersion must be a supported string",
        );
    };
    if version != MCP_PROTOCOL_VERSION {
        return error_response(
            request.id.response_value(),
            -32602,
            "unsupported MCP protocol version",
        );
    }
    success_response(
        request.id.response_value(),
        json!({
            "protocolVersion": MCP_PROTOCOL_VERSION,
            "capabilities": {
                "tools": {"listChanged": false},
                "prompts": {"listChanged": false},
                "resources": {"subscribe": false, "listChanged": false}
            },
            "serverInfo": {"name": "glass", "version": env!("CARGO_PKG_VERSION")}
        }),
    )
}

async fn handle_request(
    request: &JsonRpcRequest,
    session: &mut Option<BrowserSession>,
    options: &SessionOptions,
    policy: &BrowserPolicy,
) -> Option<JsonRpcResponse> {
    if request.id.is_notification() && request.method == "notifications/initialized" {
        return None;
    }
    if request.jsonrpc != "2.0" {
        return Some(error_response(
            request.id.response_value(),
            -32600,
            "jsonrpc must be 2.0",
        ));
    }

    let response = match request.method.as_str() {
        "initialize" => initialize_response(request),
        "ping" => success_response(request.id.response_value(), json!({})),
        "tools/list" => success_response(request.id.response_value(), json!({"tools": tools()})),
        "prompts/list" => match prompts::list_prompts() {
            Ok(result) => success_response(request.id.response_value(), result),
            Err(error) => error_response(
                request.id.response_value(),
                -32603,
                format!("prompts/list failed: {error}"),
            ),
        },
        "prompts/get" => match request.params.get("name").and_then(Value::as_str) {
            Some(name) => match prompts::get_prompt(name) {
                Ok(result) => success_response(request.id.response_value(), result),
                Err(error) => error_response(
                    request.id.response_value(),
                    -32602,
                    format!("prompts/get failed: {error}"),
                ),
            },
            None => error_response(
                request.id.response_value(),
                -32602,
                "prompts/get requires a string `name` parameter",
            ),
        },
        "resources/list" => match resources::list_resources() {
            Ok(result) => success_response(request.id.response_value(), result),
            Err(error) => error_response(
                request.id.response_value(),
                -32603,
                format!("resources/list failed: {error}"),
            ),
        },
        "resources/read" => match request.params.get("uri").and_then(Value::as_str) {
            Some(uri) => match resources::read_resource(uri) {
                Ok(result) => success_response(request.id.response_value(), result),
                Err(error) => error_response(
                    request.id.response_value(),
                    -32602,
                    format!("resources/read failed: {error}"),
                ),
            },
            None => error_response(
                request.id.response_value(),
                -32602,
                "resources/read requires a string `uri` parameter",
            ),
        },
        "tools/call" => match call_tool(request, session, options, policy).await {
            Ok(result) => success_response(request.id.response_value(), result),
            Err(error) => {
                let text = typed_browser_error(error.as_ref())
                    .unwrap_or_else(|| "browser tool failed".to_string());
                let mut content = vec![json!({"type": "text", "text": text})];
                if requested_failure_trace(request) {
                    let trace = match session.as_ref() {
                        Some(browser_session) => serde_json::to_value(
                            browser_session
                                .failure_trace_for(
                                    mcp_trace_action(
                                        request.params.get("name").and_then(Value::as_str),
                                    ),
                                    error.to_string(),
                                )
                                .await,
                        )
                        .unwrap_or_else(|_| json!({"error": "failure trace serialization failed"})),
                        None => json!({
                            "error": error.to_string(),
                            "session": "not_started"
                        }),
                    };
                    content.push(json!({
                        "type": "text",
                        "text": serde_json::to_string(&trace).unwrap_or_else(|_| "{}".to_string())
                    }));
                }
                let mut response = success_response(
                    request.id.response_value(),
                    json!({
                        "content": content,
                        "isError": true
                    }),
                );
                response.error = None;
                response
            }
        },
        _ => error_response(
            request.id.response_value(),
            -32601,
            format!("method not found: {}", request.method),
        ),
    };
    Some(response)
}

fn requested_failure_trace(request: &JsonRpcRequest) -> bool {
    request
        .params
        .get("arguments")
        .and_then(|arguments| arguments.get("includeTrace"))
        .and_then(Value::as_bool)
        .unwrap_or(false)
}

fn mcp_trace_action(tool: Option<&str>) -> ActionKind {
    match tool {
        Some("clickExpectPopup") => ActionKind::ClickExpectPopup,
        Some("doubleClick") => ActionKind::DoubleClick,
        Some("hover") => ActionKind::Hover,
        Some("drag") => ActionKind::Drag,
        Some("type") => ActionKind::Type,
        Some("key") => ActionKind::KeyPress,
        Some("keyDown") => ActionKind::KeyDown,
        Some("keyUp") => ActionKind::KeyUp,
        Some("shortcut") => ActionKind::Shortcut,
        Some("clear") => ActionKind::Clear,
        Some("check") => ActionKind::Check,
        Some("uncheck") => ActionKind::Uncheck,
        Some("select") => ActionKind::Select,
        Some("upload") => ActionKind::Upload,
        Some("scroll") => ActionKind::Scroll,
        _ => ActionKind::Click,
    }
}

fn typed_browser_error(error: &(dyn std::error::Error + 'static)) -> Option<String> {
    error
        .downcast_ref::<TargetError>()
        .and_then(|error| serde_json::to_string(error).ok())
        .or_else(|| {
            error
                .downcast_ref::<WaitTimeout>()
                .and_then(|error| serde_json::to_string(error).ok())
        })
        .or_else(|| {
            error
                .downcast_ref::<ActionContractError>()
                .and_then(|error| serde_json::to_string(error).ok())
        })
        .or_else(|| {
            error
                .downcast_ref::<ActionVerificationError>()
                .and_then(|error| serde_json::to_string(error).ok())
        })
        .or_else(|| {
            error.downcast_ref::<CdpError>().and_then(|error| {
                serde_json::to_string(&json!({
                    "kind": "transport",
                    "code": error.code,
                    "message": error.message,
                    "data": error.data,
                }))
                .ok()
            })
        })
        .or_else(|| {
            error
                .downcast_ref::<PolicyError>()
                .and_then(|error| serde_json::to_string(error).ok())
        })
        .or_else(|| {
            error
                .downcast_ref::<PopupClickError>()
                .and_then(|error| serde_json::to_string(error).ok())
        })
        .or_else(|| {
            error
                .downcast_ref::<DownloadError>()
                .and_then(|error| serde_json::to_string(error).ok())
        })
}

async fn call_tool(
    request: &JsonRpcRequest,
    session: &mut Option<BrowserSession>,
    options: &SessionOptions,
    policy: &BrowserPolicy,
) -> BrowserResult<Value> {
    let invocation = parse_tool_invocation(&request.params)?;
    let session = ensure_session(session, options, policy).await?;

    match invocation {
        ToolInvocation::Navigate {
            url,
            timeout_ms,
            expected_revision,
        } => {
            if let Some(expected_revision) = expected_revision {
                serialized_result(
                    &session
                        .navigate_with_revision(
                            url,
                            Duration::from_millis(timeout_ms),
                            expected_revision,
                        )
                        .await?,
                )
            } else {
                let page = session
                    .navigate_with_deadline(url, Duration::from_millis(timeout_ms))
                    .await?;
                serialized_result(&page)
            }
        }
        ToolInvocation::Click {
            target,
            expected_revision,
        } => {
            if let Some(expected_revision) = expected_revision {
                action_result(
                    session
                        .click_with_revision(target.as_ref(), expected_revision)
                        .await?,
                )
            } else {
                action_result(session.click(target.as_ref()).await?)
            }
        }
        ToolInvocation::Preflight { target, action } => {
            serialized_result(&session.preflight_with_action(target.as_ref(), action).await)
        }
        ToolInvocation::ClickAt { x, y } => serialized_result(&session.click_at(x, y).await?),
        ToolInvocation::ClickExpectPopup { target } => {
            serialized_result(&session.click_expect_popup(target.as_ref()).await?)
        }
        ToolInvocation::DoubleClick { target } => {
            action_result(session.double_click(target.as_ref()).await?)
        }
        ToolInvocation::Hover { target } => action_result(session.hover(target.as_ref()).await?),
        ToolInvocation::Drag {
            source,
            destination,
        } => action_result(session.drag(source.as_ref(), destination.as_ref()).await?),
        ToolInvocation::Type {
            text,
            target,
            expected_revision,
        } => action_result(
            session
                .type_text_with_expected_revision(text, target, expected_revision)
                .await?,
        ),
        ToolInvocation::Key { key } => action_result(session.key_press(key).await?),
        ToolInvocation::KeyDown { key } => action_result(session.key_down(key).await?),
        ToolInvocation::KeyUp { key } => action_result(session.key_up(key).await?),
        ToolInvocation::Shortcut { shortcut } => action_result(session.shortcut(shortcut).await?),
        ToolInvocation::Clear { target } => action_result(session.clear(target.as_ref()).await?),
        ToolInvocation::Check { target } => action_result(session.check(target.as_ref()).await?),
        ToolInvocation::Uncheck { target } => {
            action_result(session.uncheck(target.as_ref()).await?)
        }
        ToolInvocation::Select { target, value } => {
            action_result(session.select_option(target.as_ref(), value).await?)
        }
        ToolInvocation::Upload { target, files } => {
            action_result(session.upload_files(target.as_ref(), &files).await?)
        }
        ToolInvocation::Screenshot {
            format,
            quality,
            scale,
            full_page,
            clip,
            target,
        } => {
            let capture = session
                .capture_visual(&VisualCaptureOptions {
                    format,
                    quality,
                    scale,
                    clip,
                    full_page,
                    target,
                })
                .await?;
            Ok(json!({
                "content": [{"type":"text", "text": serde_json::to_string(&capture.metadata)?}, {
                    "type": "image",
                    "data": capture.data,
                    "mimeType": format!("image/{}", format.as_cdp())
                }]
            }))
        }
        ToolInvocation::Observe {
            include_dom,
            include_screenshot,
            include_form_values,
        } => {
            let mut context = match (include_dom, include_screenshot, include_form_values) {
                (false, false, false) => session.observe().await?,
                (true, false, false) => session.observe_with_dom().await?,
                (false, true, false) => session.observe_with_screenshot().await?,
                (true, true, false) => session.observe_with_dom_and_screenshot().await?,
                (false, false, true) => session.observe_with_form_values().await?,
                _ => {
                    return Err(
                        "form values may only be combined with default compact observe".into(),
                    );
                }
            };
            let screenshot = context.screenshot.take();
            let context_json = serde_json::to_string(&context)?;
            let context_bytes = context_json.len();
            let mut content = vec![json!({"type": "text", "text": context_json})];
            if let Some(data) = screenshot {
                content.push(json!({
                    "type": "image",
                    "data": data,
                    "mimeType": "image/png"
                }));
            }
            Ok(json!({
                "content": content,
                "_meta": {"contextCost": {
                    "payloadBytes": context_bytes,
                    "estimatedTokens": context_bytes.div_ceil(4)
                }}
            }))
        }
        ToolInvocation::GetDom => serialized_result(&session.deep_dom().await?),
        ToolInvocation::GetText => Ok(text_result(session.text().await?)),
        ToolInvocation::Evaluate { expression } => {
            serialized_result(&session.evaluate(expression).await?)
        }
        ToolInvocation::Batch { steps, atomic } => {
            let parsed: Vec<BatchStep> = serde_json::from_value(steps.clone())
                .map_err(|e| format!("invalid batch steps: {e}"))?;
            serialized_result(&session.run_batch_with_options(&parsed, atomic).await?)
        }
        ToolInvocation::ReconcileReferences {
            from_revision,
            refs,
            hints,
            scope_ref,
        } => {
            let options = ReconciliationOptions {
                hints: hints
                    .iter()
                    .map(|hint| Locator::parse(hint))
                    .collect::<BrowserResult<Vec<_>>>()?,
                scope_ref,
            };
            serialized_result(
                &session
                    .reconcile_references_with_options(from_revision, &refs, &options)
                    .await?,
            )
        }
        ToolInvocation::ObserveDelta => serialized_result(&session.observe_delta().await?),
        ToolInvocation::SetNetworkConditions {
            preset,
            offline,
            latency_ms,
            download_throughput,
            upload_throughput,
        } => {
            let conditions = if let Some(preset) = preset {
                crate::browser::session::NetworkConditions::preset(&preset)?
            } else {
                crate::browser::session::NetworkConditions {
                    offline,
                    latency_ms,
                    download_throughput_bytes: download_throughput,
                    upload_throughput_bytes: upload_throughput,
                    connection_type: None,
                }
            };
            session.set_network_conditions(Some(&conditions)).await?;
            serialized_result(&json!({"ok": true}))
        }
        ToolInvocation::ClearNetworkConditions => {
            session.set_network_conditions(None).await?;
            serialized_result(&json!({"ok": true}))
        }
        ToolInvocation::SetCpuThrottling { rate } => {
            session.set_cpu_throttling(Some(rate)).await?;
            serialized_result(&json!({"ok": true}))
        }
        ToolInvocation::ClearCpuThrottling => {
            session.set_cpu_throttling(None).await?;
            serialized_result(&json!({"ok": true}))
        }
        ToolInvocation::SetUserAgent {
            user_agent,
            accept_language,
            platform,
        } => {
            session
                .set_user_agent(
                    Some(&user_agent),
                    accept_language.as_deref(),
                    platform.as_deref(),
                )
                .await?;
            serialized_result(&json!({"ok": true}))
        }
        ToolInvocation::ClearUserAgent => {
            session.set_user_agent(None, None, None).await?;
            serialized_result(&json!({"ok": true}))
        }
        ToolInvocation::ExportCheckpoint => serialized_result(&session.export_checkpoint().await?),
        ToolInvocation::ImportCheckpoint { checkpoint } => {
            let ckpt: CheckpointV1 = serde_json::from_value(checkpoint.clone())
                .map_err(|e| format!("invalid checkpoint: {e}"))?;
            session.import_checkpoint(&ckpt).await?;
            serialized_result(&json!({"status": "checkpoint_imported"}))
        }
        ToolInvocation::Scroll { dx, dy } => action_result(session.scroll(dx, dy).await?),
        ToolInvocation::Wait {
            condition,
            timeout_ms,
        } => Ok(text_result(serde_json::to_string(
            &session
                .wait(
                    WaitCondition::parse(condition)?,
                    Duration::from_millis(timeout_ms),
                )
                .await?,
        )?)),
        ToolInvocation::Diagnostics { duration_ms } => serialized_result(
            &session
                .diagnostics(Duration::from_millis(duration_ms))
                .await?,
        ),
        ToolInvocation::AcceptDialog => {
            session.accept_dialog().await?;
            serialized_result(&json!({"dialog": "accepted"}))
        }
        ToolInvocation::DismissDialog => {
            session.dismiss_dialog().await?;
            serialized_result(&json!({"dialog": "dismissed"}))
        }
        ToolInvocation::DismissConsent => serialized_result(&session.dismiss_consent().await?),
        ToolInvocation::Download {
            destination,
            timeout_ms,
        } => serialized_result(
            &session
                .wait_for_download(&destination, Duration::from_millis(timeout_ms))
                .await?,
        ),
        ToolInvocation::ListTargets => serialized_result(&session.list_targets().await?),
        ToolInvocation::CreateTarget { url } => {
            serialized_result(&session.create_target(url).await?)
        }
        ToolInvocation::SelectTarget { id } => serialized_result(&session.select_target(id).await?),
        ToolInvocation::CloseTarget { id } => {
            session.close_target(id).await?;
            serialized_result(&json!({"closed": id}))
        }
        ToolInvocation::ListFrames => serialized_result(&session.list_frames().await?),
        ToolInvocation::SelectFrame { id } => serialized_result(&session.select_frame(id).await?),
        ToolInvocation::Cookies => serialized_result(&session.cookies().await?),
        ToolInvocation::SetCookies { cookies } => {
            let parsed: Vec<crate::browser::session::storage::Cookie> =
                serde_json::from_value(cookies.clone())
                    .map_err(|e| format!("invalid cookies: {e}"))?;
            session.set_cookies(&parsed).await?;
            serialized_result(&json!({"ok": true}))
        }
        ToolInvocation::ClearCookies => {
            session.clear_cookies().await?;
            serialized_result(&json!({"ok": true}))
        }
        ToolInvocation::LocalStorage => serialized_result(&session.local_storage().await?),
        ToolInvocation::SessionStorage => serialized_result(&session.session_storage().await?),
        ToolInvocation::PrintToPdf { options } => {
            let opts: crate::browser::session::PdfOptions =
                serde_json::from_value(options).unwrap_or_default();
            serialized_result(&session.print_to_pdf(&opts).await?)
        }
        ToolInvocation::FillForm {
            fields,
            expected_revision,
        } => {
            let refs: Vec<(&str, &str)> = fields
                .iter()
                .map(|(t, v)| (t.as_str(), v.as_str()))
                .collect();
            serialized_result(
                &session
                    .fill_form_with_expected_revision(&refs, expected_revision)
                    .await?,
            )
        }
        ToolInvocation::ClipboardRead => {
            let text = session.clipboard_read().await?;
            serialized_result(&serde_json::json!({"text": text}))
        }
        ToolInvocation::ClipboardWrite { text } => {
            session.clipboard_write(&text).await?;
            serialized_result(&serde_json::json!({"ok": true}))
        }
        ToolInvocation::SetGeolocation {
            latitude,
            longitude,
        } => {
            let loc = crate::browser::session::GeoLocation {
                latitude,
                longitude,
                accuracy: None,
            };
            session.set_geolocation(Some(&loc)).await?;
            serialized_result(&serde_json::json!({"ok": true}))
        }
        ToolInvocation::ClearGeolocation => {
            session.set_geolocation(None).await?;
            serialized_result(&serde_json::json!({"ok": true}))
        }
        ToolInvocation::SetTimezone { timezone_id } => {
            session.set_timezone(Some(&timezone_id)).await?;
            serialized_result(&serde_json::json!({"ok": true}))
        }
    }
}

fn parse_tool_invocation(params: &Value) -> BrowserResult<ToolInvocation<'_>> {
    let tool_name = required_string(params, "name")?;
    let arguments = &params["arguments"];
    if !arguments.is_null() && !arguments.is_object() {
        return Err("tools/call arguments must be an object".into());
    }

    match tool_name {
        "navigate" => Ok(ToolInvocation::Navigate {
            url: required_string(arguments, "url")?,
            timeout_ms: optional_u64(arguments, "timeoutMs", 20_000)?,
            expected_revision: optional_u64_value(arguments, "expectedRevision")?,
        }),
        "click" => Ok(ToolInvocation::Click {
            target: required_target(arguments)?,
            expected_revision: optional_u64_value(arguments, "expectedRevision")?,
        }),
        "preflight" => Ok(ToolInvocation::Preflight {
            target: required_target(arguments)?,
            action: match optional_string(arguments, "action")?.unwrap_or("click") {
                "click" => PreflightAction::Click,
                "hover" => PreflightAction::Hover,
                "type" => PreflightAction::Type,
                "check" => PreflightAction::Check,
                "select" => PreflightAction::Select,
                _ => return Err("action must be click, hover, type, check, or select".into()),
            },
        }),
        "clickAt" => Ok(ToolInvocation::ClickAt {
            x: required_number(arguments, "x")?,
            y: required_number(arguments, "y")?,
        }),
        "clickExpectPopup" => Ok(ToolInvocation::ClickExpectPopup {
            target: required_target(arguments)?,
        }),
        "doubleClick" => Ok(ToolInvocation::DoubleClick {
            target: required_target(arguments)?,
        }),
        "hover" => Ok(ToolInvocation::Hover {
            target: required_target(arguments)?,
        }),
        "drag" => Ok(ToolInvocation::Drag {
            source: Cow::Borrowed(required_string(arguments, "source")?),
            destination: Cow::Borrowed(required_string(arguments, "destination")?),
        }),
        "type" => Ok(ToolInvocation::Type {
            text: required_string(arguments, "text")?,
            target: optional_string(arguments, "target")?,
            expected_revision: optional_u64_value(arguments, "expectedRevision")?,
        }),
        "key" => Ok(ToolInvocation::Key {
            key: required_string(arguments, "key")?,
        }),
        "keyDown" => Ok(ToolInvocation::KeyDown {
            key: required_string(arguments, "key")?,
        }),
        "keyUp" => Ok(ToolInvocation::KeyUp {
            key: required_string(arguments, "key")?,
        }),
        "shortcut" => Ok(ToolInvocation::Shortcut {
            shortcut: required_string(arguments, "shortcut")?,
        }),
        "clear" => Ok(ToolInvocation::Clear {
            target: required_target(arguments)?,
        }),
        "check" => Ok(ToolInvocation::Check {
            target: required_target(arguments)?,
        }),
        "uncheck" => Ok(ToolInvocation::Uncheck {
            target: required_target(arguments)?,
        }),
        "select" => Ok(ToolInvocation::Select {
            target: required_target(arguments)?,
            value: required_string(arguments, "value")?,
        }),
        "upload" => Ok(ToolInvocation::Upload {
            target: required_target(arguments)?,
            files: required_path_array(arguments, "files")?,
        }),
        "screenshot" => Ok(ToolInvocation::Screenshot {
            format: parse_visual_format(optional_string(arguments, "format")?.unwrap_or("png"))?,
            quality: optional_u64_value(arguments, "quality")?
                .map(|value| u8::try_from(value).map_err(|_| "quality must be 0..=100"))
                .transpose()?,
            scale: optional_number(arguments, "scale", 1.0)?,
            full_page: optional_bool(arguments, "fullPage")?,
            clip: optional_visual_clip(arguments)?,
            target: optional_string(arguments, "target")?.map(str::to_string),
        }),
        "observe" => Ok(ToolInvocation::Observe {
            include_dom: optional_bool(arguments, "includeDom")?,
            include_screenshot: optional_bool(arguments, "includeScreenshot")?,
            include_form_values: optional_bool(arguments, "includeFormValues")?,
        }),
        "getDOM" | "dom" => Ok(ToolInvocation::GetDom),
        "getText" | "text" => Ok(ToolInvocation::GetText),
        "evaluate" => Ok(ToolInvocation::Evaluate {
            expression: required_string(arguments, "expression")?,
        }),
        "batch" => Ok(ToolInvocation::Batch {
            steps: arguments["steps"].clone(),
            atomic: optional_bool(arguments, "atomic")?,
        }),
        "reconcileReferences" => Ok(ToolInvocation::ReconcileReferences {
            from_revision: required_u64(arguments, "fromRevision")?,
            refs: required_string_array(arguments, "refs")?
                .into_iter()
                .map(String::from)
                .collect(),
            hints: optional_string_array(arguments, "hints", 8)?,
            scope_ref: arguments
                .get("scopeRef")
                .and_then(Value::as_str)
                .map(String::from),
        }),
        "observeDelta" => Ok(ToolInvocation::ObserveDelta),
        "setNetworkConditions" => Ok(ToolInvocation::SetNetworkConditions {
            preset: optional_string(arguments, "preset")?.map(str::to_string),
            offline: optional_bool(arguments, "offline")?,
            latency_ms: optional_number(arguments, "latencyMs", 0.0)?,
            download_throughput: optional_number(arguments, "downloadThroughput", -1.0)?,
            upload_throughput: optional_number(arguments, "uploadThroughput", -1.0)?,
        }),
        "clearNetworkConditions" => Ok(ToolInvocation::ClearNetworkConditions),
        "setCpuThrottling" => Ok(ToolInvocation::SetCpuThrottling {
            rate: required_number(arguments, "rate")?,
        }),
        "clearCpuThrottling" => Ok(ToolInvocation::ClearCpuThrottling),
        "setUserAgent" => Ok(ToolInvocation::SetUserAgent {
            user_agent: required_string(arguments, "userAgent")?.to_string(),
            accept_language: optional_string(arguments, "acceptLanguage")?.map(str::to_string),
            platform: optional_string(arguments, "platform")?.map(str::to_string),
        }),
        "clearUserAgent" => Ok(ToolInvocation::ClearUserAgent),
        "exportCheckpoint" => Ok(ToolInvocation::ExportCheckpoint),
        "importCheckpoint" => Ok(ToolInvocation::ImportCheckpoint {
            checkpoint: arguments.clone(),
        }),
        "scroll" => Ok(ToolInvocation::Scroll {
            dx: optional_number(arguments, "dx", 0.0)?,
            dy: optional_number(arguments, "dy", 600.0)?,
        }),
        "wait" => Ok(ToolInvocation::Wait {
            condition: required_string(arguments, "condition")?,
            timeout_ms: optional_u64(arguments, "timeoutMs", 10_000)?,
        }),
        "diagnostics" => Ok(ToolInvocation::Diagnostics {
            duration_ms: optional_u64(arguments, "durationMs", 1_000)?,
        }),
        "acceptDialog" => Ok(ToolInvocation::AcceptDialog),
        "dismissDialog" => Ok(ToolInvocation::DismissDialog),
        "dismissConsent" => Ok(ToolInvocation::DismissConsent),
        "download" => Ok(ToolInvocation::Download {
            destination: std::path::PathBuf::from(required_string(arguments, "destination")?),
            timeout_ms: optional_u64(arguments, "timeoutMs", 30_000)?,
        }),
        "listTargets" => Ok(ToolInvocation::ListTargets),
        "createTarget" => Ok(ToolInvocation::CreateTarget {
            url: required_string(arguments, "url")?,
        }),
        "selectTarget" => Ok(ToolInvocation::SelectTarget {
            id: required_string(arguments, "id")?,
        }),
        "closeTarget" => Ok(ToolInvocation::CloseTarget {
            id: required_string(arguments, "id")?,
        }),
        "listFrames" => Ok(ToolInvocation::ListFrames),
        "selectFrame" => Ok(ToolInvocation::SelectFrame {
            id: required_string(arguments, "id")?,
        }),
        "cookies" => Ok(ToolInvocation::Cookies),
        "setCookies" => Ok(ToolInvocation::SetCookies {
            cookies: arguments["cookies"].clone(),
        }),
        "clearCookies" => Ok(ToolInvocation::ClearCookies),
        "localStorage" => Ok(ToolInvocation::LocalStorage),
        "sessionStorage" => Ok(ToolInvocation::SessionStorage),
        "printToPdf" => Ok(ToolInvocation::PrintToPdf {
            options: arguments.clone(),
        }),
        "fillForm" => {
            let arr = arguments["fields"]
                .as_array()
                .ok_or("fields must be an array")?;
            let mut fields = Vec::new();
            for entry in arr {
                let target = entry["target"]
                    .as_str()
                    .ok_or("field target required")?
                    .to_string();
                let value = entry["value"].as_str().unwrap_or("").to_string();
                fields.push((target, value));
            }
            Ok(ToolInvocation::FillForm {
                fields,
                expected_revision: optional_u64_value(arguments, "expectedRevision")?,
            })
        }
        "clipboardRead" => Ok(ToolInvocation::ClipboardRead),
        "clipboardWrite" => Ok(ToolInvocation::ClipboardWrite {
            text: required_string(arguments, "text")?.to_string(),
        }),
        "setGeolocation" => Ok(ToolInvocation::SetGeolocation {
            latitude: arguments["latitude"].as_f64().ok_or("latitude required")?,
            longitude: arguments["longitude"]
                .as_f64()
                .ok_or("longitude required")?,
        }),
        "clearGeolocation" => Ok(ToolInvocation::ClearGeolocation),
        "setTimezone" => Ok(ToolInvocation::SetTimezone {
            timezone_id: required_string(arguments, "timezoneId")?.to_string(),
        }),
        _ => Err(format!("unknown tool: {tool_name}").into()),
    }
}

async fn ensure_session<'a>(
    session: &'a mut Option<BrowserSession>,
    options: &SessionOptions,
    policy: &BrowserPolicy,
) -> BrowserResult<&'a mut BrowserSession> {
    if session.is_none() {
        *session = Some(BrowserSession::start_with_policy(options, policy.clone()).await?);
    }
    Ok(session.as_mut().expect("session initialized"))
}

fn tools() -> Vec<Tool> {
    vec![
        Tool {
            name: "navigate",
            description: "Navigate the browser to a URL.",
            input_schema: json!({
                "type": "object",
                "properties": {"url": {"type": "string"}, "timeoutMs": {"type":"integer", "minimum":1, "maximum":300000, "default":20000}, "expectedRevision":{"type":"integer","minimum":0}, "includeTrace": {"type":"boolean", "default":false}},
                "required": ["url"]
            }),
        },
        Tool {
            name: "click",
            description: "Click one uniquely resolved ref/name/role+name/text/CSS/ordinal locator.",
            input_schema: json!({
                "type": "object",
                "properties": {"target": {"type": "string"}, "selector": {"type": "string"}, "expectedRevision":{"type":"integer","minimum":0}, "includeTrace": {"type":"boolean", "default":false}},
                "anyOf": [{"required": ["target"]}, {"required": ["selector"]}]
            }),
        },
        Tool {
            name: "clickExpectPopup",
            description: "Click one target and return exactly one causally verified popup without selecting it.",
            input_schema: json!({
                "type": "object",
                "properties": {"target": {"type": "string"}, "selector": {"type": "string"}, "includeTrace": {"type":"boolean", "default":false}},
                "anyOf": [{"required": ["target"]}, {"required": ["selector"]}]
            }),
        },
        Tool {
            name: "doubleClick",
            description: "Double-click one uniquely resolved ref/name/role+name/text/CSS/ordinal locator.",
            input_schema: json!({
                "type": "object",
                "properties": {"target": {"type": "string"}, "selector": {"type": "string"}, "includeTrace": {"type":"boolean", "default":false}},
                "anyOf": [{"required": ["target"]}, {"required": ["selector"]}]
            }),
        },
        Tool {
            name: "hover",
            description: "Move the pointer over one actionable target.",
            input_schema: target_schema(),
        },
        Tool {
            name: "drag",
            description: "Drag one actionable target to another.",
            input_schema: json!({"type":"object","properties":{"source":{"type":"string"},"destination":{"type":"string"},"includeTrace":{"type":"boolean","default":false}},"required":["source","destination"]}),
        },
        Tool {
            name: "key",
            description: "Dispatch a complete browser key press.",
            input_schema: string_schema("key"),
        },
        Tool {
            name: "keyDown",
            description: "Dispatch a browser key-down event.",
            input_schema: string_schema("key"),
        },
        Tool {
            name: "keyUp",
            description: "Dispatch a browser key-up event.",
            input_schema: string_schema("key"),
        },
        Tool {
            name: "shortcut",
            description: "Dispatch one explicit modifier shortcut.",
            input_schema: string_schema("shortcut"),
        },
        Tool {
            name: "clear",
            description: "Clear one actionable editable target.",
            input_schema: target_schema(),
        },
        Tool {
            name: "check",
            description: "Ensure one checkbox or radio is checked.",
            input_schema: target_schema(),
        },
        Tool {
            name: "uncheck",
            description: "Ensure one checkbox is unchecked.",
            input_schema: target_schema(),
        },
        Tool {
            name: "select",
            description: "Select one exact option value.",
            input_schema: json!({"type":"object","properties":{"target":{"type":"string"},"value":{"type":"string"},"includeTrace":{"type":"boolean","default":false}},"required":["target","value"]}),
        },
        Tool {
            name: "upload",
            description: "Set bounded local regular files on one file input; contents are never returned.",
            input_schema: json!({"type":"object","properties":{"target":{"type":"string"},"files":{"type":"array","minItems":1,"maxItems":16,"items":{"type":"string"}},"includeTrace":{"type":"boolean","default":false}},"required":["target","files"]}),
        },
        Tool {
            name: "type",
            description: "Insert text into the focused element, optionally clicking a target.",
            input_schema: json!({
                "type": "object",
                "properties": {"text": {"type": "string"}, "target": {"type": "string"}, "expectedRevision":{"type":"integer","minimum":0}, "includeTrace": {"type":"boolean","default":false}},
                "required": ["text"]
            }),
        },
        Tool {
            name: "screenshot",
            description: "Capture explicit viewport, clip, element, or full-page visual evidence with metadata.",
            input_schema: json!({"type":"object","properties":{
                "format":{"type":"string","enum":["png","jpeg","webp"],"default":"png"},
                "quality":{"type":"integer","minimum":0,"maximum":100},
                "scale":{"type":"number","minimum":0.1,"maximum":4.0,"default":1.0},
                "fullPage":{"type":"boolean","default":false},
                "clip":{"type":"object","properties":{"x":{"type":"number"},"y":{"type":"number"},"width":{"type":"number"},"height":{"type":"number"}},"required":["x","y","width","height"]},
                "target":{"type":"string"}, "includeTrace":{"type":"boolean","default":false}
            }}),
        },
        Tool {
            name: "observe",
            description: "Return compact accessibility and visible-text context; full DOM, screenshots, and form values are opt-in.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "includeDom": {"type": "boolean", "default": false},
                    "includeScreenshot": {"type": "boolean", "default": false},
                    "includeFormValues": {"type": "boolean", "default": false}
                }
            }),
        },
        Tool {
            name: "preflight",
            description: "Dry-run target resolution and clickability without pointer events, focus, scrolling, or revision changes.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "target": {"type": "string"},
                    "action": {"type": "string", "enum": ["click", "hover", "type", "check", "select"], "default": "click"}
                },
                "required": ["target"]
            }),
        },
        Tool {
            name: "clickAt",
            description: "Click exact viewport coordinates for canvas or map surfaces; policy-gated and never retargeted.",
            input_schema: json!({
                "type": "object",
                "properties": {"x": {"type": "number"}, "y": {"type": "number"}},
                "required": ["x", "y"]
            }),
        },
        Tool {
            name: "getDOM",
            description: "Return the full DOM tree. This is an explicit deep-inspection request.",
            input_schema: json!({"type": "object", "properties": {}}),
        },
        Tool {
            name: "getText",
            description: "Return visible text from the current page.",
            input_schema: json!({"type": "object", "properties": {}}),
        },
        Tool {
            name: "reconcileReferences",
            description: "Reconcile prior revisioned refs against the current page to find Preserved/Relocated/Lost mappings.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "fromRevision": {"type": "integer", "minimum": 0},
                    "refs": {"type": "array", "items": {"type": "string"}, "maxItems": 16},
                    "hints": {"type": "array", "items": {"type": "string"}, "maxItems": 8},
                    "scopeRef": {"type": "string"}
                },
                "required": ["fromRevision", "refs"]
            }),
        },
        Tool {
            name: "observeDelta",
            description: "Compare the last compact observation with a fresh same-route observation using bounded added/removed/changed controls.",
            input_schema: json!({"type": "object", "properties": {}}),
        },
        Tool {
            name: "setNetworkConditions",
            description: "Apply bounded session-scoped network conditions using slow-3g, fast-3g, offline, or explicit values.",
            input_schema: json!({"type":"object","properties":{
                "preset":{"type":"string","enum":["slow-3g","fast-3g","offline"]},
                "offline":{"type":"boolean"},"latencyMs":{"type":"number"},
                "downloadThroughput":{"type":"number"},"uploadThroughput":{"type":"number"}
            }}),
        },
        Tool {
            name: "clearNetworkConditions",
            description: "Reset session network conditions.",
            input_schema: json!({"type":"object","properties":{}}),
        },
        Tool {
            name: "setCpuThrottling",
            description: "Set a bounded session CPU throttling multiplier.",
            input_schema: json!({"type":"object","properties":{"rate":{"type":"number","exclusiveMinimum":0,"maximum":20}},"required":["rate"]}),
        },
        Tool {
            name: "clearCpuThrottling",
            description: "Reset CPU throttling to 1x.",
            input_schema: json!({"type":"object","properties":{}}),
        },
        Tool {
            name: "setUserAgent",
            description: "Apply a declared session user-agent and optional Accept-Language/platform override.",
            input_schema: json!({"type":"object","properties":{"userAgent":{"type":"string","maxLength":512},"acceptLanguage":{"type":"string","maxLength":128},"platform":{"type":"string","maxLength":128}},"required":["userAgent"]}),
        },
        Tool {
            name: "clearUserAgent",
            description: "Restore the user agent captured before Glass's override.",
            input_schema: json!({"type":"object","properties":{}}),
        },
        Tool {
            name: "exportCheckpoint",
            description: "Export a session checkpoint (≤ 4 KiB) for cross-process resume.",
            input_schema: json!({"type": "object", "properties": {}}),
        },
        Tool {
            name: "importCheckpoint",
            description: "Import a checkpoint and restore target/frame context.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "checkpoint": {"type": "object"}
                },
                "required": ["checkpoint"]
            }),
        },
        Tool {
            name: "evaluate",
            description: "Evaluate JavaScript in the current page.",
            input_schema: json!({
                "type": "object",
                "properties": {"expression": {"type": "string"}},
                "required": ["expression"]
            }),
        },
        Tool {
            name: "batch",
            description: "Execute an ordered batch of typed operations (max 32 steps). Policy is pre-flighted before any step runs.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "atomic": {"type": "boolean", "default": false},
                    "steps": {
                        "type": "array",
                        "items": {
                            "type": "object",
                            "properties": {
                                "action": {
                                    "type": "string",
                                    "enum": ["navigate", "click", "type", "check", "uncheck", "select", "clear", "scroll", "wait", "observe", "screenshot", "evaluate", "acceptDialog", "dismissDialog"]
                                }
                            },
                            "required": ["action"]
                        }
                    }
                },
                "required": ["steps"]
            }),
        },
        Tool {
            name: "scroll",
            description: "Scroll the page by CSS pixel deltas.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "dx": {"type": "number", "default": 0},
                    "dy": {"type": "number", "default": 600}
                }
            }),
        },
        Tool {
            name: "wait",
            description: "Wait for one typed condition until an explicit deadline.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "condition": {"type": "string"},
                    "timeoutMs": {"type": "integer", "minimum": 1, "default": 10000},
                    "includeTrace": {"type":"boolean","default":false}
                },
                "required": ["condition"]
            }),
        },
        Tool {
            name: "diagnostics",
            description: "Collect bounded, redacted console and network evidence in an explicit scope.",
            input_schema: json!({"type":"object","properties":{"durationMs":{"type":"integer","minimum":1,"maximum":30000,"default":1000}}}),
        },
        Tool {
            name: "acceptDialog",
            description: "Accept the currently open JavaScript dialog.",
            input_schema: json!({"type":"object","properties":{}}),
        },
        Tool {
            name: "dismissDialog",
            description: "Dismiss the currently open JavaScript dialog.",
            input_schema: json!({"type":"object","properties":{}}),
        },
        Tool {
            name: "dismissConsent",
            description: "Dismiss a visible OneTrust or Cookiebot consent control; UX assistance only, never anti-bot bypass.",
            input_schema: json!({"type":"object","properties":{}}),
        },
        Tool {
            name: "download",
            description: "Wait for one download into an authorized existing directory.",
            input_schema: json!({"type":"object","properties":{"destination":{"type":"string"},"timeoutMs":{"type":"integer","minimum":1,"maximum":30000,"default":30000}},"required":["destination"]}),
        },
        Tool {
            name: "listTargets",
            description: "List bounded page targets without changing the active target.",
            input_schema: json!({"type": "object", "properties": {}}),
        },
        Tool {
            name: "createTarget",
            description: "Create a page target without selecting it.",
            input_schema: json!({"type":"object", "properties":{"url":{"type":"string"}}, "required":["url"]}),
        },
        Tool {
            name: "selectTarget",
            description: "Explicitly select the page target used by subsequent tools.",
            input_schema: json!({"type":"object", "properties":{"id":{"type":"string"}}, "required":["id"]}),
        },
        Tool {
            name: "closeTarget",
            description: "Close one page target; closing the active target leaves no implicit selection.",
            input_schema: json!({"type":"object", "properties":{"id":{"type":"string"}}, "required":["id"]}),
        },
        Tool {
            name: "listFrames",
            description: "List bounded frames in the active page target.",
            input_schema: json!({"type": "object", "properties": {}}),
        },
        Tool {
            name: "selectFrame",
            description: "Explicitly select the frame used by subsequent tools.",
            input_schema: json!({"type":"object", "properties":{"id":{"type":"string"}}, "required":["id"]}),
        },
        Tool {
            name: "cookies",
            description: "Read all browser cookies for the current page URL. Requires persistent profile.",
            input_schema: json!({"type":"object","properties":{}}),
        },
        Tool {
            name: "setCookies",
            description: "Set browser cookies. Requires persistent profile.",
            input_schema: json!({"type":"object","properties":{"cookies":{"type":"array"}},"required":["cookies"]}),
        },
        Tool {
            name: "clearCookies",
            description: "Clear all browser cookies. Requires persistent profile.",
            input_schema: json!({"type":"object","properties":{}}),
        },
        Tool {
            name: "localStorage",
            description: "Read localStorage items (bounded to 64 entries, 1 KiB per value). Requires persistent profile.",
            input_schema: json!({"type":"object","properties":{}}),
        },
        Tool {
            name: "sessionStorage",
            description: "Read sessionStorage items (bounded to 64 entries, 1 KiB per value). Requires persistent profile.",
            input_schema: json!({"type":"object","properties":{}}),
        },
        Tool {
            name: "printToPdf",
            description: "Generate a PDF of the current page. Returns base64-encoded data.",
            input_schema: json!({"type":"object","properties":{"paperWidth":{"type":"number"},"paperHeight":{"type":"number"},"printBackground":{"type":"boolean"}}}),
        },
        Tool {
            name: "fillForm",
            description: "Fill multiple form fields atomically (max 16). Resolves all locators first.",
            input_schema: json!({"type":"object","properties":{"fields":{"type":"array","items":{"type":"object","properties":{"target":{"type":"string"},"value":{"type":"string"}},"required":["target"]}},"expectedRevision":{"type":"integer","minimum":0}},"required":["fields"]}),
        },
        Tool {
            name: "clipboardRead",
            description: "Read text from the system clipboard. Returns up to 8 KiB.",
            input_schema: json!({"type":"object","properties":{}}),
        },
        Tool {
            name: "clipboardWrite",
            description: "Write text to the system clipboard. Truncated to 8 KiB.",
            input_schema: json!({"type":"object","properties":{"text":{"type":"string"}},"required":["text"]}),
        },
        Tool {
            name: "setGeolocation",
            description: "Override browser geolocation. Use clearGeolocation to reset.",
            input_schema: json!({"type":"object","properties":{"latitude":{"type":"number"},"longitude":{"type":"number"}},"required":["latitude","longitude"]}),
        },
        Tool {
            name: "clearGeolocation",
            description: "Clear geolocation override.",
            input_schema: json!({"type":"object","properties":{}}),
        },
        Tool {
            name: "setTimezone",
            description: "Override browser timezone (IANA ID like America/New_York).",
            input_schema: json!({"type":"object","properties":{"timezoneId":{"type":"string"}},"required":["timezoneId"]}),
        },
    ]
}

fn target_schema() -> Value {
    json!({"type":"object","properties":{"target":{"type":"string"},"includeTrace":{"type":"boolean","default":false}},"required":["target"]})
}

fn string_schema(name: &str) -> Value {
    json!({"type":"object","properties":{(name):{"type":"string"},"includeTrace":{"type":"boolean","default":false}},"required":[name]})
}

fn required_u64(arguments: &Value, name: &str) -> BrowserResult<u64> {
    arguments
        .get(name)
        .and_then(Value::as_u64)
        .ok_or_else(|| format!("{name} must be a non-negative integer").into())
}

fn required_string_array<'a>(arguments: &'a Value, name: &str) -> BrowserResult<Vec<&'a str>> {
    arguments
        .get(name)
        .and_then(Value::as_array)
        .filter(|arr| !arr.is_empty() && arr.len() <= 32)
        .ok_or_else(|| format!("{name} must be a non-empty array with at most 32 entries"))?
        .iter()
        .map(|v| {
            v.as_str()
                .filter(|s| !s.is_empty())
                .ok_or_else(|| format!("{name} entries must be non-empty strings").into())
        })
        .collect()
}

fn optional_string_array(
    arguments: &Value,
    name: &str,
    max_entries: usize,
) -> BrowserResult<Vec<String>> {
    let Some(value) = arguments.get(name) else {
        return Ok(Vec::new());
    };
    let values = value
        .as_array()
        .filter(|values| values.len() <= max_entries)
        .ok_or_else(|| format!("{name} must be an array with at most {max_entries} entries"))?;
    values
        .iter()
        .map(|value| {
            value
                .as_str()
                .filter(|entry| !entry.trim().is_empty())
                .map(str::to_string)
                .ok_or_else(|| format!("{name} entries must be non-empty strings").into())
        })
        .collect()
}

fn required_string<'a>(arguments: &'a Value, name: &str) -> BrowserResult<&'a str> {
    arguments
        .get(name)
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| format!("{name} must be a non-empty string").into())
}

fn required_path_array(arguments: &Value, name: &str) -> BrowserResult<Vec<std::path::PathBuf>> {
    let values = arguments
        .get(name)
        .and_then(Value::as_array)
        .filter(|values| !values.is_empty() && values.len() <= 16)
        .ok_or_else(|| format!("{name} must contain 1..=16 paths"))?;
    values
        .iter()
        .map(|value| {
            value
                .as_str()
                .filter(|path| !path.is_empty())
                .map(std::path::PathBuf::from)
                .ok_or_else(|| format!("{name} entries must be non-empty strings").into())
        })
        .collect()
}

fn required_target(arguments: &Value) -> BrowserResult<Cow<'_, str>> {
    if let Some(target) = optional_string(arguments, "target")? {
        return Ok(Cow::Borrowed(target));
    }
    Ok(Cow::Owned(format!(
        "css={}",
        required_string(arguments, "selector")?
    )))
}

fn optional_string<'a>(arguments: &'a Value, name: &str) -> BrowserResult<Option<&'a str>> {
    match arguments.get(name) {
        None => Ok(None),
        Some(Value::String(value)) if !value.trim().is_empty() => Ok(Some(value)),
        Some(_) => Err(format!("{name} must be a non-empty string").into()),
    }
}

fn optional_bool(arguments: &Value, name: &str) -> BrowserResult<bool> {
    match arguments.get(name) {
        None => Ok(false),
        Some(Value::Bool(value)) => Ok(*value),
        Some(_) => Err(format!("{name} must be a boolean").into()),
    }
}

fn optional_number(arguments: &Value, name: &str, default: f64) -> BrowserResult<f64> {
    match arguments.get(name) {
        None => Ok(default),
        Some(value) => value
            .as_f64()
            .ok_or_else(|| format!("{name} must be a number").into()),
    }
}

fn required_number(arguments: &Value, name: &str) -> BrowserResult<f64> {
    arguments
        .get(name)
        .and_then(Value::as_f64)
        .filter(|value| value.is_finite())
        .ok_or_else(|| format!("{name} must be a finite number").into())
}

fn optional_u64(arguments: &Value, name: &str, default: u64) -> BrowserResult<u64> {
    match arguments.get(name) {
        None => Ok(default),
        Some(value) => value
            .as_u64()
            .filter(|value| (1..=300_000).contains(value))
            .ok_or_else(|| format!("{name} must be an integer from 1 to 300000").into()),
    }
}

fn optional_u64_value(arguments: &Value, name: &str) -> BrowserResult<Option<u64>> {
    match arguments.get(name) {
        None | Some(Value::Null) => Ok(None),
        Some(value) => value
            .as_u64()
            .map(Some)
            .ok_or_else(|| format!("{name} must be a non-negative integer").into()),
    }
}

fn parse_visual_format(value: &str) -> BrowserResult<VisualFormat> {
    match value {
        "png" => Ok(VisualFormat::Png),
        "jpeg" => Ok(VisualFormat::Jpeg),
        "webp" => Ok(VisualFormat::Webp),
        _ => Err("format must be png, jpeg, or webp".into()),
    }
}

fn optional_visual_clip(arguments: &Value) -> BrowserResult<Option<VisualClip>> {
    let Some(value) = arguments.get("clip").filter(|value| !value.is_null()) else {
        return Ok(None);
    };
    let object = value.as_object().ok_or("clip must be an object")?;
    let number = |name: &str| {
        object
            .get(name)
            .and_then(Value::as_f64)
            .ok_or_else(|| format!("clip.{name} must be numeric"))
    };
    Ok(Some(VisualClip {
        x: number("x")?,
        y: number("y")?,
        width: number("width")?,
        height: number("height")?,
    }))
}

fn text_result(text: impl Into<String>) -> Value {
    let text = text.into();
    let payload_bytes = text.len();
    json!({
        "content": [{"type": "text", "text": text}],
        "_meta": {"contextCost": {
            "payloadBytes": payload_bytes,
            "estimatedTokens": payload_bytes.div_ceil(4)
        }}
    })
}

fn action_result(outcome: ActionOutcome) -> BrowserResult<Value> {
    serialized_result(&outcome)
}

fn serialized_result<T: Serialize + ?Sized>(value: &T) -> BrowserResult<Value> {
    Ok(text_result(serde_json::to_string(value)?))
}

fn success_response(id: Option<Value>, result: Value) -> JsonRpcResponse {
    JsonRpcResponse {
        jsonrpc: "2.0",
        result: Some(result),
        error: None,
        id,
    }
}

fn error_response(id: Option<Value>, code: i32, message: impl Into<String>) -> JsonRpcResponse {
    JsonRpcResponse {
        jsonrpc: "2.0",
        result: None,
        error: Some(JsonRpcError {
            code,
            message: message.into(),
            data: None,
        }),
        id,
    }
}

async fn read_message<R: AsyncBufRead + Unpin>(
    reader: &mut R,
) -> io::Result<Option<(String, FrameFormat)>> {
    let mut preamble_bytes = 0_usize;
    loop {
        let Some(first_line) = read_initial_line(reader, MAX_HEADER_BYTES).await? else {
            return Ok(None);
        };
        preamble_bytes = preamble_bytes.saturating_add(first_line.len());
        if preamble_bytes > MAX_HEADER_BYTES {
            return Err(invalid_data("MCP preamble exceeds the size limit"));
        }
        let first_line = strip_line_ending(&first_line);
        if first_line.iter().all(u8::is_ascii_whitespace) {
            continue;
        }
        if first_line.starts_with(b"Content-Length:") {
            let length_text = std::str::from_utf8(&first_line[b"Content-Length:".len()..])
                .map_err(invalid_data)?
                .trim();
            let length = length_text
                .parse::<usize>()
                .map_err(|_| invalid_data("invalid Content-Length"))?;
            if length > MAX_MESSAGE_BYTES {
                return Err(invalid_data("MCP message exceeds the size limit"));
            }
            let separator = tokio::time::timeout(
                FRAME_BODY_TIMEOUT,
                read_limited_line(reader, MAX_HEADER_BYTES),
            )
            .await
            .map_err(|_| io::Error::new(io::ErrorKind::TimedOut, "MCP header timed out"))??
            .ok_or_else(|| invalid_data("truncated MCP header"))?;
            if !strip_line_ending(&separator).is_empty() {
                return Err(invalid_data(
                    "Content-Length header must end with a blank line",
                ));
            }
            let mut body = vec![0_u8; length];
            tokio::time::timeout(FRAME_BODY_TIMEOUT, reader.read_exact(&mut body))
                .await
                .map_err(|_| io::Error::new(io::ErrorKind::TimedOut, "MCP body timed out"))??;
            let body = String::from_utf8(body).map_err(invalid_data)?;
            return Ok(Some((body, FrameFormat::ContentLength)));
        }
        if first_line.len() > MAX_MESSAGE_BYTES {
            return Err(invalid_data("MCP newline message exceeds the size limit"));
        }
        let body = String::from_utf8(first_line.to_vec()).map_err(invalid_data)?;
        return Ok(Some((body, FrameFormat::Newline)));
    }
}

/// Exercise the production MCP frame reader without exposing frame contents.
#[cfg(feature = "fuzzing")]
#[doc(hidden)]
pub async fn fuzz_frame(data: &[u8]) {
    let mut reader = BufReader::new(data);
    let _ = read_message(&mut reader).await;
}

async fn read_initial_line<R: AsyncBufRead + Unpin>(
    reader: &mut R,
    limit: usize,
) -> io::Result<Option<Vec<u8>>> {
    let has_data = !reader.fill_buf().await?.is_empty();
    if !has_data {
        return Ok(None);
    }
    tokio::time::timeout(FRAME_BODY_TIMEOUT, read_limited_line(reader, limit))
        .await
        .map_err(|_| io::Error::new(io::ErrorKind::TimedOut, "MCP line timed out"))?
}

async fn read_limited_line<R: AsyncBufRead + Unpin>(
    reader: &mut R,
    limit: usize,
) -> io::Result<Option<Vec<u8>>> {
    let mut line = Vec::new();
    loop {
        let buffer = reader.fill_buf().await?;
        if buffer.is_empty() {
            return Ok((!line.is_empty()).then_some(line));
        }
        let take = buffer
            .iter()
            .position(|byte| *byte == b'\n')
            .map_or(buffer.len(), |index| index + 1);
        if line.len().saturating_add(take) > limit {
            return Err(invalid_data("MCP line exceeds the size limit"));
        }
        line.extend_from_slice(&buffer[..take]);
        reader.consume(take);
        if line.ends_with(b"\n") {
            return Ok(Some(line));
        }
    }
}

fn strip_line_ending(line: &[u8]) -> &[u8] {
    line.strip_suffix(b"\r\n")
        .or_else(|| line.strip_suffix(b"\n"))
        .unwrap_or(line)
}

fn invalid_data(error: impl std::fmt::Display) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, error.to_string())
}

async fn write_response<W: AsyncWrite + Unpin>(
    writer: &mut W,
    response: &JsonRpcResponse,
    format: FrameFormat,
) -> io::Result<()> {
    let body = encode_response(response, MAX_RESPONSE_BYTES)?;
    match format {
        FrameFormat::ContentLength => {
            writer
                .write_all(format!("Content-Length: {}\r\n\r\n", body.len()).as_bytes())
                .await?;
            writer.write_all(body.as_bytes()).await?;
        }
        FrameFormat::Newline => {
            writer.write_all(body.as_bytes()).await?;
            writer.write_all(b"\n").await?;
        }
    }
    writer.flush().await
}

fn encode_response(response: &JsonRpcResponse, limit: usize) -> io::Result<String> {
    let body = serde_json::to_string(response).map_err(io::Error::other)?;
    if body.len() > limit {
        return serde_json::to_string(&error_response(
            response.id.clone(),
            -32001,
            "MCP response exceeds the size limit",
        ))
        .map_err(io::Error::other);
    }
    Ok(body)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::browser::session::{ActionKind, ActionStatus, ActionVerificationEvidence};

    #[derive(Deserialize)]
    struct FramingCorpusCase {
        name: String,
        bytes: Vec<u8>,
        valid: bool,
    }

    #[test]
    fn request_log_metadata_excludes_params_and_raw_id() {
        let body = json!({
            "jsonrpc": "2.0",
            "id": "private-request-id",
            "method": "tools/call",
            "params": {
                "name": "type",
                "arguments": {"text": "super-secret-value"}
            }
        })
        .to_string();
        let request: JsonRpcRequest = serde_json::from_str(&body).unwrap();

        let metadata = request_log_metadata(&request, body.len());

        assert_eq!(metadata.method, "tools/call");
        assert_eq!(metadata.request_id_kind, "string");
        assert!(metadata.request_id_present);
        assert_eq!(metadata.body_bytes, body.len());
        let rendered = format!("{metadata:?}");
        assert!(!rendered.contains("private-request-id"));
        assert!(!rendered.contains("super-secret-value"));
    }

    #[tokio::test]
    async fn initializes_and_advertises_browser_tools_without_starting_chrome() {
        let initialize: JsonRpcRequest = serde_json::from_value(json!({
            "jsonrpc": "2.0",
            "id": "initialize",
            "method": "initialize",
            "params": {"protocolVersion": "2024-11-05"}
        }))
        .unwrap();
        let request: JsonRpcRequest = serde_json::from_value(json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "tools/list"
        }))
        .unwrap();
        let mut session = None;
        let policy = BrowserPolicy::development(std::env::current_dir().unwrap()).unwrap();
        let initialized = handle_request(
            &initialize,
            &mut session,
            &SessionOptions::default(),
            &policy,
        )
        .await
        .unwrap();
        let caps = &initialized.result.as_ref().unwrap()["capabilities"];
        assert_eq!(
            initialized.result.as_ref().unwrap()["serverInfo"]["name"],
            "glass"
        );
        assert_eq!(caps["tools"]["listChanged"], false);
        assert_eq!(caps["prompts"]["listChanged"], false);
        assert_eq!(caps["resources"]["subscribe"], false);
        assert_eq!(caps["resources"]["listChanged"], false);
        assert!(session.is_none());

        let result = handle_request(&request, &mut session, &SessionOptions::default(), &policy)
            .await
            .unwrap();
        let result = result.result.unwrap();
        let tools = result["tools"].as_array().unwrap();
        assert_eq!(tools.len(), 59);
        let observe = tools.iter().find(|tool| tool["name"] == "observe").unwrap();
        assert_eq!(
            observe["inputSchema"]["properties"]["includeScreenshot"]["default"],
            false
        );
        for tool_name in ["navigate", "click", "type", "fillForm"] {
            let tool = tools.iter().find(|tool| tool["name"] == tool_name).unwrap();
            assert_eq!(
                tool["inputSchema"]["properties"]["expectedRevision"]["type"],
                "integer"
            );
        }
        assert_eq!(
            observe["inputSchema"]["properties"]["includeDom"]["default"],
            false
        );
        assert!(tools.iter().any(|tool| tool["name"] == "screenshot"));
        assert!(tools.iter().any(|tool| tool["name"] == "doubleClick"));
        let popup_click = tools
            .iter()
            .find(|tool| tool["name"] == "clickExpectPopup")
            .unwrap();
        assert_eq!(
            popup_click["inputSchema"],
            json!({
                "type": "object",
                "properties": {"target": {"type": "string"}, "selector": {"type": "string"}, "includeTrace": {"type":"boolean", "default": false}},
                "anyOf": [{"required": ["target"]}, {"required": ["selector"]}]
            })
        );
        for name in [
            "listTargets",
            "createTarget",
            "selectTarget",
            "closeTarget",
            "listFrames",
            "selectFrame",
        ] {
            assert!(tools.iter().any(|tool| tool["name"] == name));
        }
        for name in [
            "hover", "drag", "key", "shortcut", "clear", "check", "uncheck", "select", "upload",
        ] {
            assert!(tools.iter().any(|tool| tool["name"] == name));
        }
        for name in [
            "diagnostics",
            "acceptDialog",
            "dismissDialog",
            "dismissConsent",
            "download",
        ] {
            assert!(tools.iter().any(|tool| tool["name"] == name));
        }
        assert!(tools.iter().any(|tool| {
            tool["name"] == "getDOM"
                && tool["description"]
                    .as_str()
                    .is_some_and(|description| description.contains("explicit"))
        }));
        assert!(session.is_none());
    }

    #[test]
    fn rejects_unsupported_protocol_versions_without_echoing_them() {
        let request: JsonRpcRequest = serde_json::from_value(json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {"protocolVersion": "private-future-version"}
        }))
        .unwrap();

        let response = initialize_response(&request);
        let error = response.error.unwrap();
        assert_eq!(error.code, -32602);
        assert_eq!(error.message, "unsupported MCP protocol version");
        assert!(!error.message.contains("private-future-version"));
    }

    #[test]
    fn cancellation_matches_string_and_numeric_request_ids() {
        for request_id in [json!(7), json!("task-7")] {
            let cancellations: CancellationMap = Arc::new(StdMutex::new(HashMap::new()));
            let (sender, mut receiver) = oneshot::channel();
            cancellations
                .lock()
                .unwrap()
                .insert(request_id_key(&request_id).unwrap(), sender);
            let request: JsonRpcRequest = serde_json::from_value(json!({
                "jsonrpc": "2.0",
                "method": "notifications/cancelled",
                "params": {"requestId": request_id}
            }))
            .unwrap();

            cancel_request(&request, &cancellations);
            assert!(receiver.try_recv().is_ok());
            assert!(cancellations.lock().unwrap().is_empty());
        }
    }

    #[test]
    fn parses_observation_options_strictly() {
        let params = json!({
            "name": "observe",
            "arguments": {"includeDom": true, "includeScreenshot": false}
        });
        assert!(matches!(
            parse_tool_invocation(&params).unwrap(),
            ToolInvocation::Observe {
                include_dom: true,
                include_screenshot: false,
                ..
            }
        ));

        let invalid = json!({
            "name": "observe",
            "arguments": {"includeDom": "true"}
        });
        let error = parse_tool_invocation(&invalid)
            .err()
            .expect("invalid boolean option should fail");
        assert!(error.to_string().contains("includeDom must be a boolean"));
    }

    #[test]
    fn parses_click_expect_popup_target_and_legacy_selector() {
        for (arguments, expected) in [
            (json!({"target": "css=#popup"}), "css=#popup"),
            (json!({"selector": "#popup"}), "css=#popup"),
        ] {
            let params = json!({"name": "clickExpectPopup", "arguments": arguments});
            assert!(matches!(
                parse_tool_invocation(&params).unwrap(),
                ToolInvocation::ClickExpectPopup { target } if target == expected
            ));
        }
    }

    #[test]
    fn parses_revision_guard_without_changing_legacy_invocations() {
        let guarded_params = json!({
            "name": "click",
            "arguments": {"target": "r7:b42", "expectedRevision": 7}
        });
        let guarded = parse_tool_invocation(&guarded_params).unwrap();
        assert!(matches!(
            guarded,
            ToolInvocation::Click {
                expected_revision: Some(7),
                ..
            }
        ));

        let legacy_params = json!({
            "name": "click",
            "arguments": {"target": "Save"}
        });
        let legacy = parse_tool_invocation(&legacy_params).unwrap();
        assert!(matches!(
            legacy,
            ToolInvocation::Click {
                expected_revision: None,
                ..
            }
        ));
    }

    #[test]
    fn serializes_popup_failures_as_typed_mcp_content() {
        let error = PopupClickError {
            kind: crate::browser::session::PopupClickErrorKind::PopupAmbiguous,
            message: "two opener-matching popups".to_string(),
        };
        let text = typed_browser_error(&error).expect("popup error should remain typed");
        assert_eq!(
            serde_json::from_str::<Value>(&text).unwrap(),
            json!({
                "kind": "popup_ambiguous",
                "message": "two opener-matching popups"
            })
        );
    }

    #[test]
    fn serializes_download_failures_as_typed_mcp_content() {
        for (kind, expected) in [
            (
                crate::browser::session::DownloadErrorKind::AuthorizationFailed,
                "authorization_failed",
            ),
            (
                crate::browser::session::DownloadErrorKind::RestorationFailed,
                "restoration_failed",
            ),
        ] {
            let error = DownloadError {
                kind,
                message: "bounded download failure".to_string(),
            };
            let text = typed_browser_error(&error).expect("download error should remain typed");
            assert_eq!(
                serde_json::from_str::<Value>(&text).unwrap(),
                json!({
                    "kind": expected,
                    "message": "bounded download failure"
                })
            );
        }
    }

    #[test]
    fn action_results_are_compact_json_text() {
        let result = action_result(ActionOutcome {
            status: ActionStatus::Succeeded,
            action: ActionKind::Scroll,
            target: None,
            revision: 9,
            previous_revision: 8,
            current_revision: 9,
            target_id: "target-1".to_string(),
            frame_id: "frame-1".to_string(),
            verification: ActionVerificationEvidence {
                revision_delta: 1,
                ..ActionVerificationEvidence::default()
            },
            evidence: None,
        })
        .unwrap();
        let text = result["content"][0]["text"].as_str().unwrap();

        assert!(!text.contains('\n'));
        assert_eq!(
            serde_json::from_str::<Value>(text).unwrap(),
            json!({"status":"succeeded", "action": "scroll", "revision": 9, "previousRevision":8, "currentRevision":9, "target_id":"target-1", "frame_id":"frame-1", "verification":{"revisionDelta":1,"urlChanged":false,"titleChanged":false,"targetChanged":false,"frameChanged":false}})
        );
    }

    #[tokio::test]
    async fn rejects_invalid_tool_calls_without_starting_chrome() {
        let request: JsonRpcRequest = serde_json::from_value(json!({
            "jsonrpc": "2.0",
            "id": 7,
            "method": "tools/call",
            "params": {
                "name": "observe",
                "arguments": {"includeScreenshot": "yes"}
            }
        }))
        .unwrap();
        let mut session = None;
        let policy = BrowserPolicy::development(std::env::current_dir().unwrap()).unwrap();

        let response = handle_request(&request, &mut session, &SessionOptions::default(), &policy)
            .await
            .unwrap();
        let result = response.result.unwrap();

        assert_eq!(result["isError"], true);
        assert_eq!(result["content"][0]["text"], "browser tool failed");
        assert!(!result.to_string().contains("yes"));
        assert!(session.is_none());
    }

    #[tokio::test]
    async fn preserves_content_length_framing() {
        let body = r#"{"jsonrpc":"2.0","id":1,"method":"ping"}"#;
        let (mut sender, receiver) = tokio::io::duplex(512);
        sender
            .write_all(format!("Content-Length: {}\r\n\r\n{body}", body.len()).as_bytes())
            .await
            .unwrap();
        sender.shutdown().await.unwrap();

        let mut reader = BufReader::new(receiver);
        let (decoded, format) = read_message(&mut reader).await.unwrap().unwrap();
        assert_eq!(decoded, body);
        assert_eq!(format, FrameFormat::ContentLength);

        let response = success_response(Some(json!(1)), json!({"ok": true}));
        let (mut sender, mut receiver) = tokio::io::duplex(512);
        write_response(&mut sender, &response, FrameFormat::ContentLength)
            .await
            .unwrap();
        sender.shutdown().await.unwrap();
        let mut encoded = Vec::new();
        receiver.read_to_end(&mut encoded).await.unwrap();
        let encoded = String::from_utf8(encoded).unwrap();

        assert!(encoded.starts_with("Content-Length: "));
        assert!(encoded.ends_with(r#"{"jsonrpc":"2.0","result":{"ok":true},"id":1}"#));
    }

    #[tokio::test]
    async fn rejects_oversized_and_malformed_frames_before_allocating_bodies() {
        let oversized = format!("Content-Length: {}\r\n\r\n", MAX_MESSAGE_BYTES + 1);
        let mut reader = BufReader::new(oversized.as_bytes());
        let error = read_message(&mut reader).await.unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::InvalidData);

        let malformed = b"Content-Length: 2\r\nnot-blank\r\n{}";
        let mut reader = BufReader::new(&malformed[..]);
        let error = read_message(&mut reader).await.unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::InvalidData);

        let mut long_line = vec![b'x'; MAX_HEADER_BYTES + 1];
        long_line.push(b'\n');
        let mut reader = BufReader::new(long_line.as_slice());
        let error = read_message(&mut reader).await.unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::InvalidData);

        let mut blank_preamble = b"\n".repeat(MAX_HEADER_BYTES + 1);
        blank_preamble.extend_from_slice(b"{}\n");
        let mut reader = BufReader::new(blank_preamble.as_slice());
        let error = read_message(&mut reader).await.unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::InvalidData);
    }

    #[tokio::test]
    async fn rejects_invalid_utf8_and_truncated_content_frames() {
        let invalid_utf8 = [0xff, b'\n'];
        let mut reader = BufReader::new(&invalid_utf8[..]);
        let error = read_message(&mut reader).await.unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::InvalidData);

        let truncated = b"Content-Length: 4\r\n\r\n{}";
        let mut reader = BufReader::new(&truncated[..]);
        let error = read_message(&mut reader).await.unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::UnexpectedEof);
    }

    #[tokio::test]
    async fn framing_regression_corpus_has_stable_outcomes() {
        let cases: Vec<FramingCorpusCase> =
            serde_json::from_str(include_str!("../../tests/fixtures/mcp_framing_corpus.json"))
                .unwrap();
        for case in cases {
            let mut reader = BufReader::new(case.bytes.as_slice());
            let result = read_message(&mut reader).await;
            assert_eq!(result.is_ok(), case.valid, "corpus case {}", case.name);
        }
    }

    #[tokio::test]
    async fn framing_property_sweep_handles_truncation_lengths_and_bytes() {
        let complete = b"Content-Length: 2\r\n\r\n{}";
        for end in 0..complete.len() {
            let mut reader = BufReader::new(&complete[..end]);
            let _ = read_message(&mut reader).await;
        }
        for digits in 1..=128 {
            let frame = format!("Content-Length: {}\r\n\r\n", "9".repeat(digits));
            let mut reader = BufReader::new(frame.as_bytes());
            assert!(read_message(&mut reader).await.is_err());
        }
        for byte in 0_u8..=u8::MAX {
            let line = [byte, b'\n'];
            let mut reader = BufReader::new(&line[..]);
            let _ = read_message(&mut reader).await;
        }
    }

    #[test]
    fn oversized_responses_become_small_protocol_errors() {
        let response = success_response(Some(json!(9)), json!({"value": "x".repeat(128)}));
        let encoded = encode_response(&response, 64).unwrap();
        let value: Value = serde_json::from_str(&encoded).unwrap();
        assert_eq!(value["id"], 9);
        assert_eq!(value["error"]["code"], -32001);
        assert!(!encoded.contains(&"x".repeat(128)));
    }

    #[tokio::test]
    async fn prompts_list_returns_all_agent_prompts() {
        let request: JsonRpcRequest = serde_json::from_value(json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "prompts/list"
        }))
        .unwrap();
        let mut session = None;
        let policy = BrowserPolicy::development(std::env::current_dir().unwrap()).unwrap();

        let response = handle_request(&request, &mut session, &SessionOptions::default(), &policy)
            .await
            .unwrap();
        let result = response.result.unwrap();
        let prompts = result["prompts"].as_array().unwrap();
        assert_eq!(prompts.len(), 4);
        let names: Vec<&str> = prompts
            .iter()
            .map(|p| p["name"].as_str().unwrap())
            .collect();
        assert!(names.contains(&"glass-safe-navigation"));
        assert!(names.contains(&"glass-target-selection"));
        assert!(names.contains(&"glass-topology"));
        assert!(names.contains(&"glass-recovery"));
    }

    #[tokio::test]
    async fn prompts_get_returns_specific_prompt_content() {
        let request: JsonRpcRequest = serde_json::from_value(json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "prompts/get",
            "params": {"name": "glass-safe-navigation"}
        }))
        .unwrap();
        let mut session = None;
        let policy = BrowserPolicy::development(std::env::current_dir().unwrap()).unwrap();

        let response = handle_request(&request, &mut session, &SessionOptions::default(), &policy)
            .await
            .unwrap();
        let result = response.result.unwrap();
        let messages = result["messages"].as_array().unwrap();
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0]["role"], "user");
        let text = messages[0]["content"]["text"].as_str().unwrap();
        assert!(text.contains("Glass Safe Navigation Loop"));
    }

    #[tokio::test]
    async fn prompts_get_rejects_missing_name_param() {
        let request: JsonRpcRequest = serde_json::from_value(json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "prompts/get",
            "params": {}
        }))
        .unwrap();
        let mut session = None;
        let policy = BrowserPolicy::development(std::env::current_dir().unwrap()).unwrap();

        let response = handle_request(&request, &mut session, &SessionOptions::default(), &policy)
            .await
            .unwrap();
        assert!(response.error.is_some());
        assert_eq!(response.error.unwrap().code, -32602);
    }

    #[tokio::test]
    async fn resources_list_returns_all_contract_resources() {
        let request: JsonRpcRequest = serde_json::from_value(json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "resources/list"
        }))
        .unwrap();
        let mut session = None;
        let policy = BrowserPolicy::development(std::env::current_dir().unwrap()).unwrap();

        let response = handle_request(&request, &mut session, &SessionOptions::default(), &policy)
            .await
            .unwrap();
        let result = response.result.unwrap();
        let resources = result["resources"].as_array().unwrap();
        assert_eq!(resources.len(), 4);
        let uris: Vec<&str> = resources
            .iter()
            .map(|r| r["uri"].as_str().unwrap())
            .collect();
        assert!(uris.contains(&"glass://contract/locators"));
        assert!(uris.contains(&"glass://contract/errors"));
        assert!(uris.contains(&"glass://contract/limits"));
        assert!(uris.contains(&"glass://contract/topology"));
    }

    #[tokio::test]
    async fn resources_read_returns_markdown_content() {
        let request: JsonRpcRequest = serde_json::from_value(json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "resources/read",
            "params": {"uri": "glass://contract/locators"}
        }))
        .unwrap();
        let mut session = None;
        let policy = BrowserPolicy::development(std::env::current_dir().unwrap()).unwrap();

        let response = handle_request(&request, &mut session, &SessionOptions::default(), &policy)
            .await
            .unwrap();
        let result = response.result.unwrap();
        let contents = result["contents"].as_array().unwrap();
        assert_eq!(contents.len(), 1);
        assert_eq!(contents[0]["mimeType"], "text/markdown");
        let text = contents[0]["text"].as_str().unwrap();
        assert!(text.contains("Glass Locator Grammar"));
    }

    #[tokio::test]
    async fn resources_read_rejects_unknown_uri_with_32602() {
        let request: JsonRpcRequest = serde_json::from_value(json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "resources/read",
            "params": {"uri": "glass://nonexistent"}
        }))
        .unwrap();
        let mut session = None;
        let policy = BrowserPolicy::development(std::env::current_dir().unwrap()).unwrap();

        let response = handle_request(&request, &mut session, &SessionOptions::default(), &policy)
            .await
            .unwrap();
        assert!(response.error.is_some());
        assert_eq!(response.error.unwrap().code, -32602);
    }

    #[tokio::test]
    async fn resources_read_rejects_missing_uri_param() {
        let request: JsonRpcRequest = serde_json::from_value(json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "resources/read",
            "params": {}
        }))
        .unwrap();
        let mut session = None;
        let policy = BrowserPolicy::development(std::env::current_dir().unwrap()).unwrap();

        let response = handle_request(&request, &mut session, &SessionOptions::default(), &policy)
            .await
            .unwrap();
        assert!(response.error.is_some());
        assert_eq!(response.error.unwrap().code, -32602);
    }

    #[test]
    fn concurrent_request_limit_rejects_the_ninth_permit() {
        let semaphore = Arc::new(Semaphore::new(MAX_CONCURRENT_REQUESTS));
        let permits = (0..MAX_CONCURRENT_REQUESTS)
            .map(|_| Arc::clone(&semaphore).try_acquire_owned().unwrap())
            .collect::<Vec<_>>();
        assert!(Arc::clone(&semaphore).try_acquire_owned().is_err());
        drop(permits);
        assert!(Arc::clone(&semaphore).try_acquire_owned().is_ok());
    }
}
