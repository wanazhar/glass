//! Rendering surface for glass.
//!
//! Two renderers, one log:
//! - [`term`] + [`editor`]: the default view. Writes into the terminal's own
//!   scrollback. No alt-screen, no resident history, ~300 lines of state.
//! - [`transcript`]: opt-in fullscreen viewer on ctrl+g. This is where the
//!   scrollbar and jump-to-turn live, and it renders through [`layout`] so
//!   frame cost tracks the viewport rather than the length of history.

pub mod editor;
pub mod layout;
pub mod markdown;
pub mod palette;
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

pub use editor::{Edit, Editor};
pub use transcript::Transcript;
pub use transcript::wrap_width;

/// One renderable entry in the transcript.
#[derive(Clone, Debug)]
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
