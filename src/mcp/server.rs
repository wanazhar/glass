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

use crate::browser::session::{
    ActionOutcome, BrowserResult, BrowserSession, SessionOptions, TargetError,
};
use crate::cli::args::Cli;

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
    },
    Click {
        target: Cow<'a, str>,
    },
    DoubleClick {
        target: Cow<'a, str>,
    },
    Type {
        text: &'a str,
        target: Option<&'a str>,
    },
    Screenshot,
    Observe {
        include_dom: bool,
        include_screenshot: bool,
    },
    GetDom,
    GetText,
    Evaluate {
        expression: &'a str,
    },
    Scroll {
        dx: f64,
        dy: f64,
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
        headed: cli.headed,
        interaction_mode: cli.interaction,
    };
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
        let task_outbound = outbound_tx.clone();
        let task_cancellations = Arc::clone(&cancellations);
        tokio::task::spawn_local(async move {
            let _permit = permit;
            let _cancel_guard = cancel_guard;
            let is_notification = request.id.is_notification();
            let id = request.id.response_value();
            let operation = async {
                let mut session = task_session.lock().await;
                handle_request(&request, &mut session, &task_options).await
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
            "capabilities": {"tools": {"listChanged": false}},
            "serverInfo": {"name": "glass", "version": env!("CARGO_PKG_VERSION")}
        }),
    )
}

async fn handle_request(
    request: &JsonRpcRequest,
    session: &mut Option<BrowserSession>,
    options: &SessionOptions,
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
        "tools/call" => match call_tool(request, session, options).await {
            Ok(result) => success_response(request.id.response_value(), result),
            Err(error) => {
                let text = error
                    .downcast_ref::<TargetError>()
                    .and_then(|error| serde_json::to_string(error).ok())
                    .unwrap_or_else(|| "browser tool failed".to_string());
                let mut response = success_response(
                    request.id.response_value(),
                    json!({
                        "content": [{"type": "text", "text": text}],
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

async fn call_tool(
    request: &JsonRpcRequest,
    session: &mut Option<BrowserSession>,
    options: &SessionOptions,
) -> BrowserResult<Value> {
    let invocation = parse_tool_invocation(&request.params)?;
    let session = ensure_session(session, options).await?;

    match invocation {
        ToolInvocation::Navigate { url } => {
            let page = session.navigate(url).await?;
            serialized_result(&page)
        }
        ToolInvocation::Click { target } => action_result(session.click(target.as_ref()).await?),
        ToolInvocation::DoubleClick { target } => {
            action_result(session.double_click(target.as_ref()).await?)
        }
        ToolInvocation::Type { text, target } => {
            action_result(session.type_text(text, target).await?)
        }
        ToolInvocation::Screenshot => {
            let image = session.screenshot_base64().await?;
            Ok(json!({
                "content": [{
                    "type": "image",
                    "data": image,
                    "mimeType": "image/png"
                }]
            }))
        }
        ToolInvocation::Observe {
            include_dom,
            include_screenshot,
        } => {
            let mut context = match (include_dom, include_screenshot) {
                (false, false) => session.observe().await?,
                (true, false) => session.observe_with_dom().await?,
                (false, true) => session.observe_with_screenshot().await?,
                (true, true) => session.observe_with_dom_and_screenshot().await?,
            };
            let screenshot = context.screenshot.take();
            let context_json = serde_json::to_string(&context)?;
            let mut content = vec![json!({"type": "text", "text": context_json})];
            if let Some(data) = screenshot {
                content.push(json!({
                    "type": "image",
                    "data": data,
                    "mimeType": "image/png"
                }));
            }
            Ok(json!({"content": content}))
        }
        ToolInvocation::GetDom => serialized_result(&session.deep_dom().await?),
        ToolInvocation::GetText => Ok(text_result(session.text().await?)),
        ToolInvocation::Evaluate { expression } => {
            serialized_result(&session.evaluate(expression).await?)
        }
        ToolInvocation::Scroll { dx, dy } => action_result(session.scroll(dx, dy).await?),
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
        }),
        "click" => Ok(ToolInvocation::Click {
            target: required_target(arguments)?,
        }),
        "doubleClick" => Ok(ToolInvocation::DoubleClick {
            target: required_target(arguments)?,
        }),
        "type" => Ok(ToolInvocation::Type {
            text: required_string(arguments, "text")?,
            target: optional_string(arguments, "target")?,
        }),
        "screenshot" => Ok(ToolInvocation::Screenshot),
        "observe" => Ok(ToolInvocation::Observe {
            include_dom: optional_bool(arguments, "includeDom")?,
            include_screenshot: optional_bool(arguments, "includeScreenshot")?,
        }),
        "getDOM" | "dom" => Ok(ToolInvocation::GetDom),
        "getText" | "text" => Ok(ToolInvocation::GetText),
        "evaluate" => Ok(ToolInvocation::Evaluate {
            expression: required_string(arguments, "expression")?,
        }),
        "scroll" => Ok(ToolInvocation::Scroll {
            dx: optional_number(arguments, "dx", 0.0)?,
            dy: optional_number(arguments, "dy", 600.0)?,
        }),
        _ => Err(format!("unknown tool: {tool_name}").into()),
    }
}

async fn ensure_session<'a>(
    session: &'a mut Option<BrowserSession>,
    options: &SessionOptions,
) -> BrowserResult<&'a mut BrowserSession> {
    if session.is_none() {
        *session = Some(BrowserSession::start(options).await?);
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
                "properties": {"url": {"type": "string"}},
                "required": ["url"]
            }),
        },
        Tool {
            name: "click",
            description: "Click one uniquely resolved ref/name/role+name/text/CSS/ordinal locator.",
            input_schema: json!({
                "type": "object",
                "properties": {"target": {"type": "string"}, "selector": {"type": "string"}},
                "anyOf": [{"required": ["target"]}, {"required": ["selector"]}]
            }),
        },
        Tool {
            name: "doubleClick",
            description: "Double-click one uniquely resolved ref/name/role+name/text/CSS/ordinal locator.",
            input_schema: json!({
                "type": "object",
                "properties": {"target": {"type": "string"}, "selector": {"type": "string"}},
                "anyOf": [{"required": ["target"]}, {"required": ["selector"]}]
            }),
        },
        Tool {
            name: "type",
            description: "Insert text into the focused element, optionally clicking a target.",
            input_schema: json!({
                "type": "object",
                "properties": {"text": {"type": "string"}, "target": {"type": "string"}},
                "required": ["text"]
            }),
        },
        Tool {
            name: "screenshot",
            description: "Capture the current page as a PNG image.",
            input_schema: json!({"type": "object", "properties": {}}),
        },
        Tool {
            name: "observe",
            description: "Return compact accessibility and visible-text context; full DOM and screenshots are opt-in.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "includeDom": {"type": "boolean", "default": false},
                    "includeScreenshot": {"type": "boolean", "default": false}
                }
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
            name: "evaluate",
            description: "Evaluate JavaScript in the current page.",
            input_schema: json!({
                "type": "object",
                "properties": {"expression": {"type": "string"}},
                "required": ["expression"]
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
    ]
}

fn required_string<'a>(arguments: &'a Value, name: &str) -> BrowserResult<&'a str> {
    arguments
        .get(name)
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| format!("{name} must be a non-empty string").into())
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

fn text_result(text: impl Into<String>) -> Value {
    json!({"content": [{"type": "text", "text": text.into()}]})
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
    use crate::browser::session::ActionKind;

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
        let initialized = handle_request(&initialize, &mut session, &SessionOptions::default())
            .await
            .unwrap();
        assert_eq!(initialized.result.unwrap()["serverInfo"]["name"], "glass");
        assert!(session.is_none());

        let result = handle_request(&request, &mut session, &SessionOptions::default())
            .await
            .unwrap();
        let result = result.result.unwrap();
        let tools = result["tools"].as_array().unwrap();
        assert_eq!(tools.len(), 10);
        let observe = tools.iter().find(|tool| tool["name"] == "observe").unwrap();
        assert_eq!(
            observe["inputSchema"]["properties"]["includeScreenshot"]["default"],
            false
        );
        assert_eq!(
            observe["inputSchema"]["properties"]["includeDom"]["default"],
            false
        );
        assert!(tools.iter().any(|tool| tool["name"] == "screenshot"));
        assert!(tools.iter().any(|tool| tool["name"] == "doubleClick"));
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
                include_screenshot: false
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
    fn action_results_are_compact_json_text() {
        let result = action_result(ActionOutcome {
            action: ActionKind::Scroll,
            target: None,
            revision: 9,
        })
        .unwrap();
        let text = result["content"][0]["text"].as_str().unwrap();

        assert!(!text.contains('\n'));
        assert_eq!(
            serde_json::from_str::<Value>(text).unwrap(),
            json!({"action": "scroll", "revision": 9})
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

        let response = handle_request(&request, &mut session, &SessionOptions::default())
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
