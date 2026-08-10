//! Glass Development Environment runtime.
//!
//! This crate owns the resident software-development workspace used by the
//! `glass` product. Browser intelligence remains provided by the one-way
//! `glass-browser` dependency.

pub mod agents;
pub mod browser;
pub mod customization;
pub mod daemon;
pub mod debugger;
pub mod experiments;
pub mod git;
pub mod intelligence;
pub mod kernels;
pub mod lsp;
pub mod mcp;
pub mod testing;
pub mod tools;
pub mod tui;
pub mod workspace;

use glass_browser::cli::args::Cli;

pub use agents::{
    AgentEvent, AgentId, AgentRegistry, AgentSnapshot, AgentSpec, AgentStatus, ResidentAgentBroker,
};
pub use browser::{BrowserRuntimeState, BrowserService, BrowserStartConfig};
pub use customization::{Customization, GlassConfig, Skill};
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
    if cli.mcp {
        let backend = std::sync::Arc::new(mcp::DevelopmentMcpBackend::open(
            std::env::current_dir()?,
            cli.yolo,
        )?);
        return glass_browser::mcp::server::run_mcp_server_with_backend(&cli, backend).await;
    }
    if let Some(glass_browser::cli::args::Commands::Agent { action }) = &cli.command
        && matches!(
            action,
            glass_browser::cli::args::AgentCommand::Tool { .. }
                | glass_browser::cli::args::AgentCommand::ToolFile { .. }
        )
    {
        let result = dispatch_external_tool(action, cli.yolo).await?;
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

async fn dispatch_external_tool(
    action: &glass_browser::cli::args::AgentCommand,
    unrestricted: bool,
) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
    use glass_browser::cli::args::AgentCommand;
    use glass_browser::development::{Actor, ToolAuthorization, ToolCall};

    let (call, root, allow_mutation, confirmed) = match action {
        AgentCommand::Tool {
            call,
            root,
            allow_mutation,
            yes,
        } => (
            serde_json::from_str::<ToolCall>(call)?,
            root,
            *allow_mutation || unrestricted,
            *yes || unrestricted,
        ),
        AgentCommand::ToolFile {
            path,
            root,
            allow_mutation,
            yes,
        } if std::env::var_os("GLASS_DEV_DAEMON_SOCKET").is_some() => {
            return daemon::forward_resident_tool_file(
                path,
                root,
                *allow_mutation || unrestricted,
                *yes || unrestricted,
            )
            .await;
        }
        AgentCommand::ToolFile {
            path,
            root,
            allow_mutation,
            yes,
        } => (
            daemon::read_private_tool_call(path)?,
            root,
            *allow_mutation || unrestricted,
            *yes || unrestricted,
        ),
        _ => return Err("expected an external agent tool action".into()),
    };
    let mut workspace = DevelopmentWorkspace::open(root)?;
    let context = DevelopmentToolContext {
        authorization: ToolAuthorization {
            actor: Actor::external("cli"),
            allow_mutation,
            confirmed,
        },
        expected_generation: workspace.generation(),
        expected_project_revision: workspace.project().revision(),
    };
    Ok(workspace.execute_tool(&call, &context)?)
}
