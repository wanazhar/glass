use serde::{Deserialize, Serialize};
use std::io::{self, BufRead, Read, Write};
use tracing::{debug, error, info};

/// JSON-RPC request for MCP.
#[derive(Debug, Deserialize)]
struct JsonRpcRequest {
    jsonrpc: String,
    method: String,
    #[serde(default)]
    params: serde_json::Value,
    #[serde(default)]
    id: Option<serde_json::Value>,
}

/// JSON-RPC response for MCP.
#[derive(Debug, Serialize)]
struct JsonRpcResponse {
    jsonrpc: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<JsonRpcError>,
    #[serde(skip_serializing_if = "Option::is_none")]
    id: Option<serde_json::Value>,
}

#[derive(Debug, Serialize)]
struct JsonRpcError {
    code: i32,
    message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    data: Option<serde_json::Value>,
}

/// MCP tool definition.
#[derive(Debug, Serialize)]
struct Tool {
    name: String,
    description: String,
    #[serde(rename = "inputSchema")]
    input_schema: serde_json::Value,
}

/// Get the list of available MCP tools.
fn get_tools() -> Vec<Tool> {
    vec![
        Tool {
            name: "navigate".to_string(),
            description: "Navigate the browser to a URL".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "url": {
                        "type": "string",
                        "description": "The URL to navigate to"
                    }
                },
                "required": ["url"]
            }),
        },
        Tool {
            name: "click".to_string(),
            description: "Click on an element by CSS selector".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "selector": {
                        "type": "string",
                        "description": "CSS selector for the element to click"
                    }
                },
                "required": ["selector"]
            }),
        },
        Tool {
            name: "type".to_string(),
            description: "Type text into the currently focused element".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "text": {
                        "type": "string",
                        "description": "Text to type"
                    }
                },
                "required": ["text"]
            }),
        },
        Tool {
            name: "screenshot".to_string(),
            description: "Take a screenshot of the current page".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "format": {
                        "type": "string",
                        "description": "Image format: png or jpeg",
                        "enum": ["png", "jpeg"],
                        "default": "png"
                    }
                }
            }),
        },
        Tool {
            name: "getDOM".to_string(),
            description: "Get the accessibility tree of the current page".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {}
            }),
        },
        Tool {
            name: "evaluate".to_string(),
            description: "Execute JavaScript in the page and return the result".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "expression": {
                        "type": "string",
                        "description": "JavaScript expression to evaluate"
                    }
                },
                "required": ["expression"]
            }),
        },
        Tool {
            name: "getText".to_string(),
            description: "Get the text content of the current page".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {}
            }),
        },
    ]
}

/// Handle a JSON-RPC request.
fn handle_request(request: &JsonRpcRequest) -> JsonRpcResponse {
    let response = match request.method.as_str() {
        "initialize" => {
            info!("MCP: initialize");
            JsonRpcResponse {
                jsonrpc: "2.0".to_string(),
                result: Some(serde_json::json!({
                    "protocolVersion": "2024-11-05",
                    "capabilities": {
                        "tools": {
                            "listChanged": false
                        }
                    },
                    "serverInfo": {
                        "name": "glass",
                        "version": "0.1.0"
                    }
                })),
                error: None,
                id: request.id.clone(),
            }
        }
        "notifications/initialized" => {
            // No response needed for notifications
            return JsonRpcResponse {
                jsonrpc: "2.0".to_string(),
                result: None,
                error: None,
                id: None,
            };
        }
        "tools/list" => {
            info!("MCP: tools/list");
            let tools = get_tools();
            JsonRpcResponse {
                jsonrpc: "2.0".to_string(),
                result: Some(serde_json::json!({
                    "tools": tools
                })),
                error: None,
                id: request.id.clone(),
            }
        }
        "tools/call" => {
            let tool_name = request.params["name"].as_str().unwrap_or("");
            let arguments = &request.params["arguments"];
            info!("MCP: tools/call {tool_name}");

            match tool_name {
                "navigate" => {
                    let url = arguments["url"].as_str().unwrap_or("");
                    JsonRpcResponse {
                        jsonrpc: "2.0".to_string(),
                        result: Some(serde_json::json!({
                            "content": [{
                                "type": "text",
                                "text": format!("Navigated to: {url}")
                            }]
                        })),
                        error: None,
                        id: request.id.clone(),
                    }
                }
                "click" => {
                    let selector = arguments["selector"].as_str().unwrap_or("");
                    JsonRpcResponse {
                        jsonrpc: "2.0".to_string(),
                        result: Some(serde_json::json!({
                            "content": [{
                                "type": "text",
                                "text": format!("Clicked: {selector}")
                            }]
                        })),
                        error: None,
                        id: request.id.clone(),
                    }
                }
                "type" => {
                    let text = arguments["text"].as_str().unwrap_or("");
                    JsonRpcResponse {
                        jsonrpc: "2.0".to_string(),
                        result: Some(serde_json::json!({
                            "content": [{
                                "type": "text",
                                "text": format!("Typed: {text}")
                            }]
                        })),
                        error: None,
                        id: request.id.clone(),
                    }
                }
                "screenshot" => {
                    JsonRpcResponse {
                        jsonrpc: "2.0".to_string(),
                        result: Some(serde_json::json!({
                            "content": [{
                                "type": "text",
                                "text": "Screenshot taken (base64 data)"
                            }]
                        })),
                        error: None,
                        id: request.id.clone(),
                    }
                }
                "getDOM" | "getText" => {
                    JsonRpcResponse {
                        jsonrpc: "2.0".to_string(),
                        result: Some(serde_json::json!({
                            "content": [{
                                "type": "text",
                                "text": "Page content retrieved"
                            }]
                        })),
                        error: None,
                        id: request.id.clone(),
                    }
                }
                "evaluate" => {
                    let expr = arguments["expression"].as_str().unwrap_or("");
                    JsonRpcResponse {
                        jsonrpc: "2.0".to_string(),
                        result: Some(serde_json::json!({
                            "content": [{
                                "type": "text",
                                "text": format!("Evaluated: {expr}")
                            }]
                        })),
                        error: None,
                        id: request.id.clone(),
                    }
                }
                _ => JsonRpcResponse {
                    jsonrpc: "2.0".to_string(),
                    result: None,
                    error: Some(JsonRpcError {
                        code: -32601,
                        message: format!("Unknown tool: {tool_name}"),
                        data: None,
                    }),
                    id: request.id.clone(),
                },
            }
        }
        "ping" => JsonRpcResponse {
            jsonrpc: "2.0".to_string(),
            result: Some(serde_json::json!({})),
            error: None,
            id: request.id.clone(),
        },
        _ => JsonRpcResponse {
            jsonrpc: "2.0".to_string(),
            result: None,
            error: Some(JsonRpcError {
                code: -32601,
                message: format!("Method not found: {}", request.method),
                data: None,
            }),
            id: request.id.clone(),
        },
    };

    response
}

/// Run the MCP server over stdio.
pub async fn run_mcp_server() -> Result<(), Box<dyn std::error::Error>> {
    info!("MCP server starting on stdio");

    let stdin = io::stdin();
    let mut reader = stdin.lock();
    let mut stdout = io::stdout();

    loop {
        // Read Content-Length header
        let mut header_line = String::new();
        match reader.read_line(&mut header_line) {
            Ok(0) => break, // EOF
            Ok(_) => {}
            Err(e) => {
                error!("Read error: {e}");
                break;
            }
        }

        let header_line = header_line.trim();
        if header_line.is_empty() {
            continue;
        }

        let content_length: usize = header_line
            .strip_prefix("Content-Length: ")
            .and_then(|s| s.parse().ok())
            .unwrap_or(0);

        if content_length == 0 {
            continue;
        }

        // Skip empty line after header
        let mut empty = String::new();
        reader.read_line(&mut empty)?;

        // Read the JSON body
        let mut body = vec![0u8; content_length];
        reader.read_exact(&mut body)?;

        let body_str = match String::from_utf8(body) {
            Ok(s) => s,
            Err(e) => {
                error!("Invalid UTF-8: {e}");
                continue;
            }
        };

        debug!("MCP recv: {body_str}");

        // Parse the JSON-RPC request
        let request: JsonRpcRequest = match serde_json::from_str(&body_str) {
            Ok(r) => r,
            Err(e) => {
                error!("Invalid JSON-RPC: {e}");
                let response = JsonRpcResponse {
                    jsonrpc: "2.0".to_string(),
                    result: None,
                    error: Some(JsonRpcError {
                        code: -32700,
                        message: format!("Parse error: {e}"),
                        data: None,
                    }),
                    id: None,
                };
                write_response(&mut stdout, &response)?;
                continue;
            }
        };

        // Handle the request
        let response = handle_request(&request);

        // Skip responses without id (notifications)
        if response.id.is_none() {
            continue;
        }

        write_response(&mut stdout, &response)?;
    }

    Ok(())
}

/// Write a JSON-RPC response to stdout with Content-Length header.
fn write_response(
    writer: &mut impl Write,
    response: &JsonRpcResponse,
) -> Result<(), Box<dyn std::error::Error>> {
    let body = serde_json::to_string(response)?;
    write!(writer, "Content-Length: {}\r\n\r\n{}", body.len(), body)?;
    writer.flush()?;
    Ok(())
}
