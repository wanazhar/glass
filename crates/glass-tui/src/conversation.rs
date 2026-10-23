//! Shared typed conversation projection for Core's live view and host adapters.
//!
//! The reducer owns role aggregation and tool-call association. Hosts retain
//! their own session, message, and policy metadata alongside the returned
//! item indices.

use crate::{Entry, Role, ToolCard};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConversationItem {
    pub entry: Entry,
    pub streaming: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ConversationEvent {
    TurnStart {
        turn: usize,
    },
    User(String),
    AssistantDelta(String),
    AssistantFinal(String),
    ReasoningDelta(String),
    ToolStart {
        call_id: String,
        name: String,
        command: String,
    },
    ToolProgress {
        call_id: String,
        text: String,
    },
    ToolEnd {
        call_id: String,
        output: String,
        status: String,
        elapsed_ms: Option<u64>,
    },
    Status {
        role: Role,
        text: String,
    },
    Finish,
}

#[derive(Clone, Debug, Default)]
pub struct Conversation {
    items: Vec<ConversationItem>,
    reasoning: String,
    reasoning_stream_open: bool,
    turn: usize,
    active_assistant: Option<usize>,
}

impl Conversation {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn seed(&mut self, entries: Vec<Entry>) {
        self.turn = entries.last().map(|entry| entry.turn).unwrap_or_default();
        self.items = entries
            .into_iter()
            .map(|entry| ConversationItem {
                entry,
                streaming: false,
            })
            .collect();
        self.reasoning.clear();
        self.reasoning_stream_open = false;
        self.active_assistant = None;
    }

    pub fn seed_reasoning(&mut self, reasoning: String) {
        self.reasoning = reasoning;
        self.reasoning_stream_open = false;
    }

    pub fn apply(&mut self, event: ConversationEvent) -> Option<usize> {
        match event {
            ConversationEvent::TurnStart { turn } => {
                self.finish_streaming();
                self.turn = turn;
                self.reasoning_stream_open = false;
                None
            }
            ConversationEvent::User(text) => {
                self.finish_streaming();
                Some(self.push(Role::User, text, None))
            }
            ConversationEvent::AssistantDelta(delta) => {
                self.reasoning_stream_open = false;
                if let Some(index) = self
                    .active_assistant
                    .filter(|index| self.items.get(*index).is_some_and(|item| item.streaming))
                {
                    self.items[index].entry.text.push_str(&delta);
                    Some(index)
                } else {
                    let index = self.push(Role::Assistant, delta, None);
                    self.items[index].streaming = true;
                    self.active_assistant = Some(index);
                    Some(index)
                }
            }
            ConversationEvent::AssistantFinal(text) => {
                self.reasoning_stream_open = false;
                if let Some(index) = self
                    .active_assistant
                    .take()
                    .filter(|index| self.items.get(*index).is_some_and(|item| item.streaming))
                {
                    self.items[index].entry.text = text;
                    self.items[index].streaming = false;
                    Some(index)
                } else if self.items.last().is_some_and(|item| {
                    item.entry.role == Role::Assistant
                        && (item.entry.text.is_empty() || item.entry.text == text)
                }) {
                    let index = self.items.len() - 1;
                    self.items[index].entry.text = text;
                    Some(index)
                } else {
                    Some(self.push(Role::Assistant, text, None))
                }
            }
            ConversationEvent::ReasoningDelta(delta) => {
                if !self.reasoning_stream_open && !self.reasoning.is_empty() {
                    self.reasoning.push_str("\n\n");
                }
                self.reasoning.push_str(&delta);
                self.reasoning_stream_open = true;
                None
            }
            ConversationEvent::ToolStart {
                call_id,
                name,
                command,
            } => {
                self.reasoning_stream_open = false;
                self.finish_streaming();
                let index = self.items.len();
                self.items.push(ConversationItem {
                    entry: Entry {
                        role: if name == "bash" {
                            Role::Bash
                        } else {
                            Role::Tool
                        },
                        text: format!("{name}\n{command}"),
                        turn: self.turn,
                        tool_card: Some(ToolCard {
                            expanded: false,
                            call_id,
                            command: if command.is_empty() { name } else { command },
                            output: String::new(),
                            status: "running".into(),
                            elapsed_ms: None,
                            spill: None,
                            truncated: false,
                        }),
                    },
                    streaming: false,
                });
                Some(index)
            }
            ConversationEvent::ToolProgress { call_id, text } => {
                self.reasoning_stream_open = false;
                let index = self.tool_index(&call_id)?;
                let card = self.items[index].entry.tool_card.as_mut()?;
                if !card.output.is_empty() {
                    card.output.push('\n');
                }
                card.output.push_str(&text);
                Some(index)
            }
            ConversationEvent::ToolEnd {
                call_id,
                output,
                status,
                elapsed_ms,
            } => {
                self.reasoning_stream_open = false;
                let Some(index) = self.tool_index(&call_id) else {
                    return Some(self.push(Role::Tool, output, None));
                };
                let card = self.items[index].entry.tool_card.as_mut()?;
                if !output.is_empty() {
                    card.output = output;
                }
                card.status = status;
                card.elapsed_ms = elapsed_ms;
                self.items[index].entry.text = format!("{}\n{}", card.command, card.output);
                Some(index)
            }
            ConversationEvent::Status { role, text } => {
                self.reasoning_stream_open = false;
                self.finish_streaming();
                Some(self.push(role, text, None))
            }
            ConversationEvent::Finish => {
                self.finish_streaming();
                self.reasoning_stream_open = false;
                None
            }
        }
    }

    pub fn items(&self) -> &[ConversationItem] {
        &self.items
    }

    pub fn reasoning(&self) -> &str {
        &self.reasoning
    }

    pub fn finish_streaming(&mut self) {
        if let Some(index) = self.active_assistant.take()
            && let Some(item) = self.items.get_mut(index)
        {
            item.streaming = false;
        }
    }

    fn push(&mut self, role: Role, text: String, tool_card: Option<ToolCard>) -> usize {
        let index = self.items.len();
        self.items.push(ConversationItem {
            entry: Entry {
                role,
                text,
                turn: self.turn,
                tool_card,
            },
            streaming: false,
        });
        index
    }

    fn tool_index(&self, call_id: &str) -> Option<usize> {
        self.items.iter().rposition(|item| {
            item.entry
                .tool_card
                .as_ref()
                .is_some_and(|card| card.call_id == call_id)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn assistant_deltas_final_and_reasoning_keep_typed_turn_state() {
        let mut conversation = Conversation::new();
        conversation.apply(ConversationEvent::TurnStart { turn: 3 });
        conversation.apply(ConversationEvent::ReasoningDelta("Reasoning".into()));
        conversation.apply(ConversationEvent::AssistantDelta("Hello ".into()));
        conversation.apply(ConversationEvent::AssistantDelta("there".into()));
        conversation.apply(ConversationEvent::AssistantFinal("Hello there!".into()));
        assert_eq!(conversation.reasoning(), "Reasoning");
        assert_eq!(conversation.items().len(), 1);
        assert_eq!(conversation.items()[0].entry.text, "Hello there!");
        assert_eq!(conversation.items()[0].entry.turn, 3);
        assert!(!conversation.items()[0].streaming);
    }

    #[test]
    fn tool_progress_and_end_match_exact_call_ids() {
        let mut conversation = Conversation::new();
        for call_id in ["call-a", "call-b"] {
            conversation.apply(ConversationEvent::ToolStart {
                call_id: call_id.into(),
                name: "glass.file.read".into(),
                command: "src/main.rs".into(),
            });
        }
        conversation.apply(ConversationEvent::ToolProgress {
            call_id: "call-a".into(),
            text: "first output".into(),
        });
        conversation.apply(ConversationEvent::ToolEnd {
            call_id: "call-b".into(),
            output: "second output".into(),
            status: "exit 0".into(),
            elapsed_ms: None,
        });
        let a = conversation.items()[0].entry.tool_card.as_ref().unwrap();
        let b = conversation.items()[1].entry.tool_card.as_ref().unwrap();
        assert_eq!(a.output, "first output");
        assert_eq!(a.status, "running");
        assert_eq!(b.output, "second output");
        assert_eq!(b.status, "exit 0");
    }

    #[test]
    fn empty_tool_end_keeps_accumulated_progress_output() {
        let mut conversation = Conversation::new();
        conversation.apply(ConversationEvent::ToolStart {
            call_id: "call-1".into(),
            name: "glass.file.read".into(),
            command: "src/main.rs".into(),
        });
        conversation.apply(ConversationEvent::ToolProgress {
            call_id: "call-1".into(),
            text: "streamed output".into(),
        });
        conversation.apply(ConversationEvent::ToolEnd {
            call_id: "call-1".into(),
            output: String::new(),
            status: "completed".into(),
            elapsed_ms: None,
        });

        let item = &conversation.items()[0];
        let card = item.entry.tool_card.as_ref().unwrap();
        assert_eq!(card.output, "streamed output");
        assert!(item.entry.text.ends_with("streamed output"));
        assert_eq!(card.status, "completed");
    }

    #[test]
    fn assistant_stream_keeps_chronology_around_tool_events() {
        let mut conversation = Conversation::new();
        conversation.apply(ConversationEvent::TurnStart { turn: 4 });
        conversation.apply(ConversationEvent::AssistantDelta("before ".into()));
        conversation.apply(ConversationEvent::ToolStart {
            call_id: "call-1".into(),
            name: "glass.file.read".into(),
            command: "src/lib.rs".into(),
        });
        conversation.apply(ConversationEvent::ToolEnd {
            call_id: "call-1".into(),
            output: "contents".into(),
            status: "completed".into(),
            elapsed_ms: Some(12),
        });
        conversation.apply(ConversationEvent::AssistantDelta("after".into()));

        assert_eq!(conversation.items().len(), 3);
        assert_eq!(conversation.items()[0].entry.text, "before ");
        assert_eq!(conversation.items()[0].entry.turn, 4);
        let tool = conversation.items()[1].entry.tool_card.as_ref().unwrap();
        assert_eq!(tool.call_id, "call-1");
        assert_eq!(tool.elapsed_ms, Some(12));
        assert_eq!(conversation.items()[2].entry.text, "after");
    }

    #[test]
    fn turn_start_finishes_the_previous_assistant_stream() {
        let mut conversation = Conversation::new();
        conversation.apply(ConversationEvent::TurnStart { turn: 1 });
        conversation.apply(ConversationEvent::AssistantDelta("partial".into()));
        conversation.apply(ConversationEvent::TurnStart { turn: 2 });

        assert!(!conversation.items()[0].streaming);
        conversation.apply(ConversationEvent::AssistantDelta("next".into()));
        assert_eq!(conversation.items().len(), 2);
        assert_eq!(conversation.items()[1].entry.turn, 2);
    }
}
