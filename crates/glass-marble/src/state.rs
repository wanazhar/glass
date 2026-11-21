//! Server-owned product state for the local Glass marble UI.

use crate::MarbleError;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// One durable conversation the local product knows about.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConversationRecord {
    pub id: String,
    pub task_ids: Vec<String>,
}

/// One durable generation task admitted for a conversation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskRecord {
    pub id: String,
    pub conversation_id: String,
    pub status: String,
    pub owner: String,
}

/// One inbound runtime tool request awaiting an explicit host decision.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PendingApproval {
    pub request_id: String,
    pub call_id: String,
    pub tool: String,
    pub arguments: Value,
}

/// One durable transcript entry for a conversation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TranscriptEntry {
    pub conversation_id: String,
    pub kind: String,
    pub text: String,
}

/// Whole local product state served to the UI and encoded for the client.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProductState {
    pub conversations: Vec<ConversationRecord>,
    pub tasks: Vec<TaskRecord>,
    pub approvals: Vec<PendingApproval>,
    pub transcript: Vec<TranscriptEntry>,
}

impl ProductState {
    /// Encode state into the JSON shape served to and accepted from the UI.
    pub fn encode(&self) -> Value {
        serde_json::to_value(self).unwrap_or(Value::Null)
    }

    /// Decode state from the JSON shape served to and accepted from the UI.
    pub fn decode(value: &Value) -> Result<Self, MarbleError> {
        serde_json::from_value(value.clone())
            .map_err(|error| MarbleError::Protocol(format!("invalid product state: {error}")))
    }
}
