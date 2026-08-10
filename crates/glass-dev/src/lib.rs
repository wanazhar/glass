//! Glass Development Environment runtime.
//!
//! This crate owns the resident software-development workspace used by the
//! `glass` product. Browser intelligence remains provided by the one-way
//! `glass-browser` dependency.

pub mod agents;
pub mod daemon;
pub mod debugger;
pub mod git;
pub mod kernels;
pub mod lsp;
pub mod testing;
pub mod tools;
pub mod workspace;

use glass_browser::cli::args::Cli;

pub use agents::{AgentEvent, AgentId, AgentRegistry, AgentSnapshot, AgentSpec, AgentStatus};
pub use lsp::{LanguageServerConfig, LanguageService, LanguageServiceEvent};
pub use tools::{DevelopmentToolContext, DevelopmentToolRouter};
pub use workspace::{DevelopmentWorkspace, SharedDevelopmentWorkspace};

/// Dispatch the full Glass Development Environment.
///
/// Command-surface extraction from the browser crate is incremental during
/// the 0.3.4 ownership migration. The product entry point lives here so each
/// resident service can move without changing the installed `glass` binary.
pub async fn dispatch(cli: Cli) -> Result<(), Box<dyn std::error::Error>> {
    if let Some(glass_browser::cli::args::Commands::Daemon { action }) = &cli.command {
        return daemon::dispatch(action).await;
    }
    glass_browser::cli::runner::dispatch(cli).await
}
