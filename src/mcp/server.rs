use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::io;
use tokio::io::{
    AsyncBufRead, AsyncBufReadExt, AsyncReadExt, AsyncWrite, AsyncWriteExt, BufReader,
};
use tracing::{debug, info};

use crate::browser::session::{ActionOutcome, BrowserResult, BrowserSession, SessionOptions};
use crate::cli::args::Cli;

#[derive(Debug, Deserialize)]
struct JsonRpcRequest {
    jsonrpc: String,
    method: String,
    #[serde(default)]
    params: Value,
    #[serde(default)]
    id: Option<Value>,
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
        target: &'a str,
    },
    DoubleClick {
        target: &'a str,
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

pub async fn run_mcp_server(cli: &Cli) -> BrowserResult<()> {
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
    let mut stdout = tokio::io::stdout();
    let mut session = None;

    while let Some((body, format)) = read_message(&mut reader).await? {
        let body_bytes = body.len();
        let request: JsonRpcRequest = match serde_json::from_str(&body) {
            Ok(request) => request,
            Err(error) => {
                debug!(body_bytes, "MCP request rejected: invalid JSON");
                let response = error_response(None, -32700, format!("parse error: {error}"));
                write_response(&mut stdout, &response, format).await?;
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

        let response = handle_request(&request, &mut session, &options).await;
        if let Some(response) = response {
            write_response(&mut stdout, &response, format).await?;
        }
    }

    if let Some(session) = session {
        session.close().await?;
    }
    Ok(())
}

fn request_log_metadata(request: &JsonRpcRequest, body_bytes: usize) -> RequestLogMetadata<'_> {
    let request_id_kind = match request.id.as_ref() {
        None => "absent",
        Some(Value::Null) => "null",
        Some(Value::String(_)) => "string",
        Some(Value::Number(_)) => "number",
        Some(_) => "invalid",
    };
    RequestLogMetadata {
        method: &request.method,
        request_id_kind,
        request_id_present: request.id.is_some(),
        body_bytes,
    }
}

async fn handle_request(
    request: &JsonRpcRequest,
    session: &mut Option<BrowserSession>,
    options: &SessionOptions,
) -> Option<JsonRpcResponse> {
    if request.id.is_none() && request.method == "notifications/initialized" {
        return None;
    }
    if request.jsonrpc != "2.0" {
        return Some(error_response(
            request.id.clone(),
            -32600,
            "jsonrpc must be 2.0",
        ));
    }

    let response = match request.method.as_str() {
        "initialize" => success_response(
            request.id.clone(),
            json!({
                "protocolVersion": "2024-11-05",
                "capabilities": {"tools": {"listChanged": false}},
                "serverInfo": {"name": "glass", "version": env!("CARGO_PKG_VERSION")}
            }),
        ),
        "ping" => success_response(request.id.clone(), json!({})),
        "tools/list" => success_response(request.id.clone(), json!({"tools": tools()})),
        "tools/call" => match call_tool(request, session, options).await {
            Ok(result) => success_response(request.id.clone(), result),
            Err(error) => {
                let mut response = success_response(
                    request.id.clone(),
                    json!({
                        "content": [{"type": "text", "text": error.to_string()}],
                        "isError": true
                    }),
                );
                response.error = None;
                response
            }
        },
        _ => error_response(
            request.id.clone(),
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
        ToolInvocation::Click { target } => action_result(session.click(target).await?),
        ToolInvocation::DoubleClick { target } => {
            action_result(session.double_click(target).await?)
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
            description: "Click an accessibility reference, accessible name, or CSS selector.",
            input_schema: json!({
                "type": "object",
                "properties": {"target": {"type": "string"}, "selector": {"type": "string"}},
                "anyOf": [{"required": ["target"]}, {"required": ["selector"]}]
            }),
        },
        Tool {
            name: "doubleClick",
            description: "Double-click an accessibility reference, accessible name, or CSS selector.",
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

fn required_target(arguments: &Value) -> BrowserResult<&str> {
    optional_string(arguments, "target")?.map_or_else(|| required_string(arguments, "selector"), Ok)
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
    let mut first_line = String::new();
    loop {
        first_line.clear();
        if reader.read_line(&mut first_line).await? == 0 {
            return Ok(None);
        }
        if !first_line.trim().is_empty() {
            break;
        }
    }

    if let Some(length) = first_line
        .trim()
        .strip_prefix("Content-Length:")
        .and_then(|value| value.trim().parse::<usize>().ok())
    {
        let mut separator = String::new();
        reader.read_line(&mut separator).await?;
        let mut body = vec![0_u8; length];
        reader.read_exact(&mut body).await?;
        return Ok(Some((
            String::from_utf8_lossy(&body).into_owned(),
            FrameFormat::ContentLength,
        )));
    }

    Ok(Some((first_line.trim().to_string(), FrameFormat::Newline)))
}

async fn write_response<W: AsyncWrite + Unpin>(
    writer: &mut W,
    response: &JsonRpcResponse,
    format: FrameFormat,
) -> io::Result<()> {
    let body = serde_json::to_string(response).map_err(io::Error::other)?;
    match format {
        FrameFormat::ContentLength => {
            writer
                .write_all(format!("Content-Length: {}\r\n\r\n{}", body.len(), body).as_bytes())
                .await?;
        }
        FrameFormat::Newline => {
            writer.write_all(body.as_bytes()).await?;
            writer.write_all(b"\n").await?;
        }
    }
    writer.flush().await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::browser::session::ActionKind;

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
        assert!(
            result["content"][0]["text"]
                .as_str()
                .is_some_and(|message| message.contains("includeScreenshot must be a boolean"))
        );
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
}
