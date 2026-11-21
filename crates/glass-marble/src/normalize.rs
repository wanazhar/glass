//! Inbound text becomes the minimal typed turn input.

use glass_runtime_client::TurnInput;
use glass_runtime_client::protocol::ContextBlock;

pub(crate) fn engine_input(
    state: &crate::ProductState,
    conversation_id: Option<&str>,
    owner: &str,
    text: &str,
) -> TurnInput {
    let mut input = to_turn_input(&InboundMessage::new(text));
    if let Some(previous) = conversation_id.filter(|id| !id.is_empty()).and_then(|id| {
        state
            .tasks
            .iter()
            .rev()
            .find(|task| task.conversation_id == id && task.status != "queued")
    }) && matches!(
        previous.status.as_str(),
        "cancelled" | "failed" | "interrupted"
    ) && !state.transcript.iter().any(|entry| {
        entry.task_id == previous.id && entry.kind == "assistant" && !entry.text.trim().is_empty()
    }) {
        input.context.push(ContextBlock::Text {
            text: "Host note: the earlier unanswered request was withdrawn. Do not answer it; answer only the newest message.".into(),
        });
    }
    if let Some(text) = state.memory.context_block_for(owner) {
        input.context.push(ContextBlock::Text { text });
    }
    input
}

/// One human-or-agent message entering a Glass marble turn.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InboundMessage {
    pub text: String,
}

impl InboundMessage {
    pub fn new(text: impl Into<String>) -> Self {
        Self { text: text.into() }
    }
}

/// Marble v1 carries text only; context and images stay empty.
pub fn to_turn_input(message: &InboundMessage) -> TurnInput {
    TurnInput {
        text: message.text.clone(),
        context: Vec::new(),
        images: Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ProductState, TaskRecord, TranscriptEntry};

    #[test]
    fn withdrawn_unanswered_turn_is_not_the_next_request() {
        let mut state = ProductState::default();
        assert!(
            engine_input(&state, Some("chat"), "marble", "new")
                .context
                .is_empty()
        );
        for status in ["cancelled", "failed", "interrupted"] {
            state.tasks = vec![TaskRecord {
                id: "old".into(),
                conversation_id: "chat".into(),
                status: status.into(),
                ..Default::default()
            }];
            let input = engine_input(&state, Some("chat"), "marble", "new");
            assert_eq!(input.text, "new");
            assert!(
                matches!(&input.context[0], ContextBlock::Text { text } if text.contains("withdrawn"))
            );
            assert!(
                engine_input(&state, Some("other"), "marble", "new")
                    .context
                    .is_empty()
            );
            assert!(
                engine_input(&state, None, "marble", "new")
                    .context
                    .is_empty()
            );
        }
        state.transcript.push(TranscriptEntry {
            task_id: "old".into(),
            kind: "assistant".into(),
            text: "visible reply".into(),
            ..Default::default()
        });
        assert!(
            engine_input(&state, Some("chat"), "marble", "new")
                .context
                .is_empty()
        );
        state.transcript.clear();
        state.tasks[0].status = "completed".into();
        assert!(
            engine_input(&state, Some("chat"), "marble", "new")
                .context
                .is_empty()
        );
    }
}
