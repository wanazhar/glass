//! Rendering surface for glass.
//!
//! Two renderers, one log:
//! - [`term`] + [`editor`]: the default view. Writes into the terminal's own
//!   scrollback. No alt-screen, no resident history, ~300 lines of state.
//! - [`transcript`]: opt-in fullscreen viewer on ctrl+g. This is where the
//!   scrollbar and jump-to-turn live, and it is the only code path that
//!   touches a long history.

pub mod editor;
pub mod term;
pub mod transcript;

// Re-exported so the harness does not need its own crossterm/ratatui
// dependencies. The terminal backend is an implementation detail of this
// crate, not something the agent layer should reach around it to use.
pub use crossterm;
pub use ratatui;

pub use editor::{Edit, Editor};
pub use transcript::{Entry, Role, Transcript};
