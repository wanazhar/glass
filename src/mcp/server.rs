use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::io;
use tokio::io::{
    AsyncBufRead, AsyncBufReadExt, AsyncReadExt, AsyncWrite, AsyncWriteExt, BufReader,
};
use tracing::{debug, info};

use crate::browser::session::{BrowserResult, BrowserSession, SessionOptions};
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

enum FrameFormat {
    ContentLength,
    Newline,
}

pub async fn run_mcp_server(cli: &Cli) -> BrowserResult<()> {
    info!("MCP server starting on stdio");
    let options = SessionOptions {
        port: cli.port,
        chrome_path: cli.chrome_path.clone(),
        profile: cli.profile.clone(),
        incognito: cli.incognito,
        headed: cli.headed,
        interaction_mode: cli.interaction,
    };
    let stdin = tokio::io::stdin();
    let mut reader = BufReader::new(stdin);
    let mut stdout = tokio::io::stdout();
    let mut session = None;

    while let Some((body, format)) = read_message(&mut reader).await? {
        debug!(%body, "MCP request received");
        let request: JsonRpcRequest = match serde_json::from_str(&body) {
            Ok(request) => request,
            Err(error) => {
                let response = error_response(None, -32700, format!("parse error: {error}"));
                write_response(&mut stdout, &response, format).await?;
                continue;
            }
        };

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
    let tool_name = request.params["name"]
        .as_str()
        .ok_or("tools/call requires a string name")?;
    let arguments = &request.params["arguments"];
    let session = ensure_session(session, options).await?;

    match tool_name {
        "navigate" => {
            let url = required_string(arguments, "url")?;
            let page = session.navigate(url).await?;
            Ok(text_result(format!(
                "navigated to {} — {}",
                page.title, page.url
            )))
        }
        "click" => {
            let target = required_string(arguments, "target")
                .or_else(|_| required_string(arguments, "selector"))?;
            Ok(text_result(format!(
                "clicked {}",
                session.click(target).await?
            )))
        }
        "type" => {
            let text = required_string(arguments, "text")?;
            let target = arguments["target"].as_str();
            session.type_text(text, target).await?;
            Ok(text_result(format!(
                "typed {} characters",
                text.chars().count()
            )))
        }
        "screenshot" => {
            let image = session.screenshot_base64().await?;
            Ok(json!({
                "content": [{
                    "type": "image",
                    "data": image,
                    "mimeType": "image/png"
                }]
            }))
        }
        "observe" => {
            let include_screenshot = arguments["includeScreenshot"].as_bool().unwrap_or(false);
            let mut context = if include_screenshot {
                session.observe_with_screenshot().await?
            } else {
                session.observe().await?
            };
            let screenshot = context.screenshot.take();
            let context_json = serde_json::to_string_pretty(&context)?;
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
        "getDOM" | "dom" => Ok(text_result(session.snapshot().await?.format())),
        "getText" | "text" => Ok(text_result(session.text().await?)),
        "evaluate" => {
            let expression = required_string(arguments, "expression")?;
            Ok(text_result(serde_json::to_string_pretty(
                &session.evaluate(expression).await?,
            )?))
        }
        "scroll" => {
            let dx = arguments["dx"].as_f64().unwrap_or(0.0);
            let dy = arguments["dy"].as_f64().unwrap_or(600.0);
            session.scroll(dx, dy).await?;
            Ok(text_result(format!("scrolled by ({dx}, {dy})")))
        }
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
            description: "Return DOM, accessibility, and visible text context; screenshots are opt-in.",
            input_schema: json!({
                "type": "object",
                "properties": {"includeScreenshot": {"type": "boolean", "default": false}}
            }),
        },
        Tool {
            name: "getDOM",
            description: "Return the current accessibility snapshot.",
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
                "properties": {"dx": {"type": "number"}, "dy": {"type": "number"}}
            }),
        },
    ]
}

fn required_string<'a>(arguments: &'a Value, name: &str) -> BrowserResult<&'a str> {
    arguments[name]
        .as_str()
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| format!("missing string argument: {name}").into())
}

fn text_result(text: impl Into<String>) -> Value {
    json!({"content": [{"type": "text", "text": text.into()}]})
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

    #[tokio::test]
    async fn advertises_real_browser_tools_without_starting_chrome() {
        let request: JsonRpcRequest = serde_json::from_value(json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "tools/list"
        }))
        .unwrap();
        let mut session = None;
        let result = handle_request(&request, &mut session, &SessionOptions::default())
            .await
            .unwrap();
        let result = result.result.unwrap();
        let tools = result["tools"].as_array().unwrap();
        assert_eq!(tools.len(), 9);
        let observe = tools.iter().find(|tool| tool["name"] == "observe").unwrap();
        assert_eq!(
            observe["inputSchema"]["properties"]["includeScreenshot"]["default"],
            false
        );
        assert!(tools.iter().any(|tool| tool["name"] == "screenshot"));
        assert!(session.is_none());
    }
}
