//! Inbound text becomes the minimal typed turn input.

use glass_runtime_client::TurnInput;

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
