//! Rendering surface for glass.
//!
//! Streaming and replay share Markdown, code, math, and table presentation.
//! The application owns the terminal surface; this crate provides its editor,
//! bounded viewport layout, and transcript controller. Inline and fullscreen
//! readers share anchors, search, paging, and follow state without a scrollbar.

pub mod cli;
pub mod conversation;
pub mod editor;
pub mod layout;
pub mod markdown;
pub mod math;
pub mod palette;
pub mod syntax;
pub mod term;
pub mod text;
pub mod theme;
pub mod tool_output;
pub mod transcript;

// Re-exported so the harness does not need its own crossterm/ratatui
// dependencies. The terminal backend is an implementation detail of this
// crate, not something the agent layer should reach around it to use.
pub use crossterm;
pub use ratatui;

pub use conversation::{Conversation, ConversationEvent, ConversationItem};
pub use editor::{Edit, Editor};
pub use transcript::Transcript;
pub use transcript::wrap_width;

/// One renderable entry in the transcript.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    pub role: Role,
    pub text: String,
    /// Conversation turn, used by jump-to-turn.
    pub turn: usize,
    /// Exact durable tool identity for expandable cards. `None` keeps legacy
    /// entries and non-tool transcript rows on the same rendering path.
    pub tool_card: Option<ToolCard>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ToolCard {
    pub expanded: bool,
    pub call_id: String,
    pub command: String,
    pub output: String,
    pub status: String,
    pub elapsed_ms: Option<u64>,
    pub spill: Option<String>,
    pub truncated: bool,
}

/// Which role produced a log line, used only for colour.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Role {
    User,
    Assistant,
    Bash,
    Tool,
    System,
    Meta,
}

impl Role {
    pub fn label(self) -> &'static str {
        match self {
            Role::User => "you",
            Role::Assistant => "glass",
            Role::Bash => "bash",
            Role::Tool => "tool",
            Role::System => "system",
            Role::Meta => "",
        }
    }
}

mod table;
