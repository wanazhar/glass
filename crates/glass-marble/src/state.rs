//! Server-owned product state for the local Glass marble UI.

use crate::MarbleError;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// One durable conversation the local product knows about.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConversationRecord {
    pub id: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub task_ids: Vec<String>,
    /// Unix milliseconds of the last update; zero for records without one.
    #[serde(default)]
    pub updated_at: u64,
}

/// One durable generation task admitted for a conversation.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskRecord {
    pub id: String,
    pub conversation_id: String,
    /// `queued`, `running`, `completed`, `cancelled`, or `failed`.
    pub status: String,
    /// Who owns the running turn. Marble tasks are owned by the agent.
    pub owner: String,
    #[serde(default)]
    pub prompt: String,
    #[serde(default)]
    pub at: u64,
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
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TranscriptEntry {
    pub conversation_id: String,
    /// `user`, `assistant`, `reasoning`, `tool`, or `system`.
    pub kind: String,
    pub text: String,
    #[serde(default)]
    pub task_id: String,
    #[serde(default)]
    pub at: u64,
}

/// Whole local product state served to the UI and encoded for the client.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProductState {
    /// Monotonic document revision; durable commits use it for reconciliation.
    #[serde(default)]
    pub revision: u64,
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

    /// Advance the document revision after a mutation.
    pub fn bump(&mut self) -> u64 {
        self.revision += 1;
        self.revision
    }

    /// Find or create the conversation record for `id`.
    pub fn conversation_mut(&mut self, id: &str) -> &mut ConversationRecord {
        if let Some(index) = self
            .conversations
            .iter()
            .position(|conversation| conversation.id == id)
        {
            return &mut self.conversations[index];
        }
        self.conversations.push(ConversationRecord {
            id: id.to_string(),
            ..ConversationRecord::default()
        });
        self.conversations
            .last_mut()
            .expect("conversation record was just pushed")
    }

    /// Replace the record for one task id.
    pub fn upsert_task(&mut self, task: TaskRecord) {
        self.tasks.retain(|existing| existing.id != task.id);
        self.tasks.push(task);
    }

    /// Update one task's status; returns whether the task was known.
    pub fn set_task_status(&mut self, task_id: &str, status: &str) -> bool {
        if let Some(task) = self.tasks.iter_mut().find(|task| task.id == task_id) {
            task.status = status.to_string();
            return true;
        }
        false
    }

    /// Append one transcript entry.
    pub fn push_transcript(&mut self, entry: TranscriptEntry) {
        self.transcript.push(entry);
    }

    /// Transcript entries for one conversation, newest last.
    pub fn transcript_for(&self, conversation_id: &str) -> Vec<TranscriptEntry> {
        self.transcript
            .iter()
            .filter(|entry| entry.conversation_id == conversation_id)
            .cloned()
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn conversation_records_are_created_once_and_reused() {
        let mut state = ProductState::default();
        state.conversation_mut("c1").title = "first".into();
        state.conversation_mut("c1").title = "second".into();
        assert_eq!(state.conversations.len(), 1);
        assert_eq!(state.conversations[0].title, "second");
        assert_eq!(state.bump(), 1);
        assert_eq!(state.bump(), 2);
    }

    #[test]
    fn legacy_state_documents_decode_with_defaulted_fields() {
        let legacy = serde_json::json!({
            "conversations": [{"id": "c1", "taskIds": ["t1"]}],
            "tasks": [{"id": "t1", "conversationId": "c1", "status": "completed", "owner": "human"}],
            "approvals": [],
            "transcript": [{"conversationId": "c1", "kind": "assistant", "text": "hi"}]
        });
        let state = ProductState::decode(&legacy).expect("legacy state should decode");
        assert_eq!(state.revision, 0);
        assert_eq!(state.conversations[0].title, "");
        assert_eq!(state.tasks[0].at, 0);
        assert_eq!(state.transcript[0].task_id, "");
    }
}
