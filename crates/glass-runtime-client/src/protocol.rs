//! Typed protocol values. Unknown event names remain usable through raw data.
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::PathBuf;
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    pub input_schema: Value,
}
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ToolMode {
    #[default]
    Host,
    Local,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionOptions {
    pub cwd: PathBuf,
    #[serde(default)]
    pub tool_mode: ToolMode,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tools: Vec<ToolDefinition>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub instructions: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub connection: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image_input: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reasoning_budget: Option<u32>,
}
impl SessionOptions {
    pub fn host(cwd: impl Into<PathBuf>, tools: Vec<ToolDefinition>) -> Self {
        Self {
            cwd: cwd.into(),
            tool_mode: ToolMode::Host,
            tools,
            instructions: None,
            model: None,
            connection: None,
            provider: None,
            base_url: None,
            image_input: None,
            max_tokens: None,
            reasoning_budget: None,
        }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum ContextBlock {
    Text { text: String },
    Resource { uri: String, text: String },
    Json { value: Value },
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImageInput {
    pub media_type: String,
    pub data: String,
}
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TurnInput {
    pub text: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub context: Vec<ContextBlock>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub images: Vec<ImageInput>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum ContentBlock {
    Text {
        text: String,
    },
    Json {
        value: Value,
    },
    Image {
        #[serde(rename = "mediaType")]
        media_type: String,
        data: String,
    },
}
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolResult {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub content: Vec<ContentBlock>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub structured_content: Option<Value>,
    #[serde(default)]
    pub is_error: bool,
}
#[derive(Debug, Clone, Deserialize)]
pub struct RpcError {
    pub code: i32,
    pub message: String,
    #[serde(default)]
    pub data: Value,
}
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeEvent {
    pub session_id: String,
    pub turn_id: Value,
    #[serde(rename = "type")]
    pub kind: String,
    pub data: Value,
}
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolRequest {
    pub session_id: String,
    pub turn_id: Value,
    pub call_id: String,
    pub name: String,
    pub arguments: Value,
}
#[derive(Debug, Clone)]
pub enum Incoming {
    Event(RuntimeEvent),
    Tool {
        request_id: Value,
        call: ToolRequest,
    },
    Response {
        id: Value,
        result: Result<Value, RpcError>,
    },
    Unknown(Value),
}
impl Incoming {
    pub fn parse(value: Value) -> Result<Self, serde_json::Error> {
        match value["method"].as_str() {
            Some("runtime/event") => {
                serde_json::from_value(value["params"].clone()).map(Self::Event)
            }
            Some("tool/execute") => {
                serde_json::from_value(value["params"].clone()).map(|call| Self::Tool {
                    request_id: value["id"].clone(),
                    call,
                })
            }
            Some(_) => Ok(Self::Unknown(value)),
            _ => {
                if value.get("id").is_none()
                    || value.get("result").is_some() == value.get("error").is_some()
                {
                    return Err(<serde_json::Error as serde::de::Error>::custom(
                        "invalid response envelope",
                    ));
                }
                let result = if let Some(error) = value.get("error") {
                    Err(serde_json::from_value(error.clone())?)
                } else {
                    Ok(value["result"].clone())
                };
                Ok(Self::Response {
                    id: value["id"].clone(),
                    result,
                })
            }
        }
    }
}
