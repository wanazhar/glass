//! Glass Development Environment runtime.
//!
//! This crate owns the resident software-development workspace used by the
//! `glass` product. Browser intelligence remains provided by the one-way
//! `glass-browser` dependency.

pub mod debugger;
pub mod workspace;

use glass_browser::cli::args::Cli;

pub use workspace::{DevelopmentWorkspace, SharedDevelopmentWorkspace};

/// Dispatch the full Glass Development Environment.
///
/// Command-surface extraction from the browser crate is incremental during
/// the 0.3.4 ownership migration. The product entry point lives here so each
/// resident service can move without changing the installed `glass` binary.
pub async fn dispatch(cli: Cli) -> Result<(), Box<dyn std::error::Error>> {
    glass_browser::cli::runner::dispatch(cli).await
}
