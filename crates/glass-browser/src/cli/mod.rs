//! CLI argument parsing and command dispatch.
//!
//! Defines the top-level CLI struct (via clap) and the runner module that
//! orchestrates one-shot commands, TUI mode, and MCP server startup.

pub mod args;
pub mod runner;
pub(crate) mod site_smoke;
