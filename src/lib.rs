//! Glass — lightweight, local-first browser automation.
//!
//! Drives Chrome or Chromium directly through the Chrome DevTools Protocol
//! (CDP), without Playwright, WebDriver, or an embedded browser runtime.
//!
//! # Modules
//!
//! - [`browser`] — Chrome lifecycle, CDP client, DOM/accessibility parsing,
//!   mouse movement, security policy, profiles, and the central
//!   [`BrowserSession`](browser::BrowserSession).
//! - [`cli`] — Clap argument definitions and command dispatch.
//! - [`mcp`] — JSON-RPC/MCP stdio server for AI clients.
//! - [`tui`] — Ratatui terminal interface.

pub mod browser;
pub mod cli;
pub mod mcp;
pub mod tui;
