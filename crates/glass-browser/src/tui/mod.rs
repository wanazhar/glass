//! Terminal user interface (Ratatui).
//!
//! Interactive terminal workspace for browser and project session management.
//!
//! The reducer supports responsive desktop and phone layouts. Structured
//! semantic observation is the default; continuous pixels remain opt-in.
//! Terminal-native live view negotiates Herdr, Kitty, or bounded true-color
//! ANSI rendering through [`crate::terminal_graphics`], while SSH port
//! forwarding to a local Safari session remains the full-fidelity mobile path.
//! Frame retention is bounded by [`crate::presentation`].

pub mod app;
mod herdr_graphics;
