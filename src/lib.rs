//! Glass — lightweight local browser control for Chrome and Chromium.
//!
//! Drives Chrome or Chromium directly through the Chrome DevTools Protocol
//! (CDP), without Playwright, WebDriver, or an embedded browser runtime.
//!
//! # Quick start
//!
//! Build a session with [`SessionOptions`], start Chrome, navigate, and close
//! the session when the work is complete:
//!
//! ```rust,no_run
//! use glass::{BrowserSession, SessionOptions};
//!
//! # async fn run() -> glass::BrowserResult<()> {
//! let options = SessionOptions::builder().build()?;
//! let session = BrowserSession::start(&options).await?;
//! let page = session.navigate("https://example.com").await?;
//! println!("{}", page.url);
//! session.close().await?;
//! # Ok(())
//! # }
//! ```
//!
//! The [`browser`] module contains the reusable Rust API. The [`cli`] module
//! backs the `glass` binary, [`mcp`] exposes the MCP stdio server, and [`tui`]
//! provides the terminal interface. User-facing guides are available in the
//! repository's [`docs`](https://github.com/wanazhar/glass/tree/main/docs).
//!
//! # Cargo features
//!
//! - `visual-compare` enables PNG comparison helpers for screenshot checks.
//! - `fuzzing` enables test-oriented fuzzing hooks and is not needed by normal
//!   applications.
//!
//! # Modules
//!
//! - [`browser`] — Chrome lifecycle, CDP client, DOM/accessibility parsing,
//!   mouse movement, security policy, profiles, and the central
//!   [`BrowserSession`].
//! - [`cli`] — Clap argument definitions and command dispatch.
//! - [`mcp`] — JSON-RPC/MCP stdio server for MCP-compatible clients.
//! - [`tui`] — Ratatui terminal interface.

/// Browser control modules and the reusable [`browser::BrowserSession`] API.
pub mod browser;
/// Command-line argument definitions and dispatch helpers.
pub mod cli;
/// MCP stdio server, prompts, resources, and tool dispatch.
pub mod mcp;
/// Ratatui terminal interface.
pub mod tui;

// Keep the most common embedding types on the crate root. Lower-level and
// capability-specific APIs remain organized under `glass::browser`.
pub use browser::{
    AccessibilityDiffSummary, ActionContractError, ActionFailureKind, ActionKind, ActionOutcome,
    ActionStatus, ActionVerificationEvidence, BrowserResult, BrowserSession, NavigationOutcome,
    PageInfo, SessionOptions, SessionOptionsBuilder, WORKFLOW_SCHEMA_VERSION, WorkflowBudgets,
    WorkflowCheckpoint, WorkflowCheckpointPage, WorkflowCheckpointStep, WorkflowDefinition,
    WorkflowInput, WorkflowOutput, WorkflowOutputDeclaration, WorkflowOutputSource,
    WorkflowResumeError, WorkflowResumePlan, WorkflowRunResult, WorkflowRunStatus, WorkflowStep,
    WorkflowStepRecord, WorkflowStepState, WorkflowTerminalProof, WorkflowTrace,
    WorkflowTraceEvent, WorkflowTransactionClass, WorkflowValidationError, WorkflowValueType,
};
