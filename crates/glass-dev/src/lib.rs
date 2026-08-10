//! Glass Development Environment runtime.
//!
//! This crate owns the resident software-development workspace used by the
//! `glass` product. Browser intelligence remains provided by the one-way
//! `glass-browser` dependency.

pub mod agents;
pub mod browser;
pub mod daemon;
pub mod debugger;
pub mod experiments;
pub mod git;
pub mod intelligence;
pub mod kernels;
pub mod lsp;
pub mod testing;
pub mod tools;
pub mod tui;
pub mod workspace;

use glass_browser::cli::args::Cli;

pub use agents::{
    AgentEvent, AgentId, AgentRegistry, AgentSnapshot, AgentSpec, AgentStatus, ResidentAgentBroker,
};
pub use browser::{BrowserRuntimeState, BrowserService, BrowserStartConfig};
pub use experiments::{
    ExperimentComparison, ExperimentEvidence, ExperimentManager, ExperimentRanking,
    ExperimentSnapshot, ExperimentState,
};
pub use intelligence::{
    CausalPath, DevelopmentEdge, DevelopmentIntelligence, DevelopmentNode, DevelopmentNodeKind,
    ObservableDevelopmentEvent, ObservableEventInput, ReplayDiff,
};
pub use lsp::{LanguageServerConfig, LanguageService, LanguageServiceEvent};
pub use tools::{DevelopmentToolContext, DevelopmentToolRouter};
pub use workspace::{DevelopmentWorkspace, SharedDevelopmentWorkspace};

/// Dispatch the full Glass Development Environment.
///
/// Command-surface extraction from the browser crate is incremental during
/// the 0.3.4 ownership migration. The product entry point lives here so each
/// resident service can move without changing the installed `glass` binary.
pub async fn dispatch(cli: Cli) -> Result<(), Box<dyn std::error::Error>> {
    if let Some(glass_browser::cli::args::Commands::Agent {
        action:
            glass_browser::cli::args::AgentCommand::ToolFile {
                path,
                root,
                allow_mutation,
                yes,
            },
    }) = &cli.command
        && std::env::var_os("GLASS_DEV_DAEMON_SOCKET").is_some()
    {
        let result = daemon::forward_resident_tool_file(
            path,
            root,
            *allow_mutation || cli.yolo,
            *yes || cli.yolo,
        )
        .await?;
        println!("{}", serde_json::to_string_pretty(&result)?);
        return Ok(());
    }
    if let Some(glass_browser::cli::args::Commands::Daemon { action }) = &cli.command {
        return daemon::dispatch(action).await;
    }
    if cli.command.is_none() && cli.prompt.is_none() && !cli.mcp {
        return tui::run(std::env::current_dir()?, cli.tui_layout);
    }
    glass_browser::cli::runner::dispatch(cli).await
}
