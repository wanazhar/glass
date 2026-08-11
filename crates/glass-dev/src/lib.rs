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
pub mod pi_runtime;
pub mod tasks;
pub mod testing;
pub mod tools;
pub mod trust;
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
    ExperimentSnapshot, ExperimentState, ExperimentTrustPolicy,
};
pub use intelligence::{
    CausalPath, DevelopmentEdge, DevelopmentIntelligence, DevelopmentNode, DevelopmentNodeKind,
    ObservableDevelopmentEvent, ObservableEventInput, ReplayDiff,
};
pub use lsp::{LanguageServerConfig, LanguageService, LanguageServiceEvent};
pub use pi_runtime::PiSessionRequest;
pub use tasks::{
    RetryPolicy, TaskBudget, TaskEvidence, TaskId, TaskScheduler, TaskSnapshot, TaskSpec,
    TaskState, VerificationRequirement,
};
pub use tools::{DevelopmentToolContext, DevelopmentToolRouter};
pub use trust::{LocalTrustDecision, WorkspaceIdentity, WorkspaceTrust, WorkspaceTrustStore};
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
    enforce_legacy_development_trust(&cli)?;
    glass_browser::cli::runner::dispatch(cli).await
}

fn enforce_legacy_development_trust(cli: &Cli) -> Result<(), Box<dyn std::error::Error>> {
    use glass_browser::cli::args::{AgentCommand, AgentHarness, Commands, ProjectCommand};

    let (root, safe_static) = match cli.command.as_ref() {
        Some(Commands::Project { action }) => {
            let root = match action {
                ProjectCommand::Inspect { root }
                | ProjectCommand::Files { root }
                | ProjectCommand::Search { root, .. }
                | ProjectCommand::Read { root, .. }
                | ProjectCommand::Edit { root, .. }
                | ProjectCommand::Mkdir { root, .. }
                | ProjectCommand::Rename { root, .. }
                | ProjectCommand::Delete { root, .. }
                | ProjectCommand::Diagnostics { root, .. }
                | ProjectCommand::Run { root, .. }
                | ProjectCommand::Test { root }
                | ProjectCommand::Lint { root }
                | ProjectCommand::Process { root, .. }
                | ProjectCommand::Diff { root }
                | ProjectCommand::Link { root, .. }
                | ProjectCommand::Graph { root, .. }
                | ProjectCommand::Breakpoint { root, .. }
                | ProjectCommand::Timeline { root }
                | ProjectCommand::Replay { root, .. }
                | ProjectCommand::Neovim { root, .. }
                | ProjectCommand::Experiment { root, .. }
                | ProjectCommand::Attach { root, .. } => root,
            };
            let safe = matches!(
                action,
                ProjectCommand::Inspect { .. }
                    | ProjectCommand::Files { .. }
                    | ProjectCommand::Search { .. }
                    | ProjectCommand::Read { .. }
                    | ProjectCommand::Diff { .. }
                    | ProjectCommand::Graph { .. }
                    | ProjectCommand::Breakpoint { .. }
                    | ProjectCommand::Timeline { .. }
                    | ProjectCommand::Replay { .. }
            );
            (root, safe)
        }
        Some(Commands::Agent { action }) => {
            let (root, safe) = match action {
                AgentCommand::Hello { root, harness } => {
                    (root, matches!(harness, AgentHarness::Local))
                }
                AgentCommand::Prompt { root, harness, .. } => {
                    (root, matches!(harness, AgentHarness::Local))
                }
                AgentCommand::Tool { .. } | AgentCommand::ToolFile { .. } => return Ok(()),
                AgentCommand::Steer { root, .. }
                | AgentCommand::FollowUp { root, .. }
                | AgentCommand::Models { root }
                | AgentCommand::SetModel { root, .. }
                | AgentCommand::Thinking { root, .. }
                | AgentCommand::Abort { root }
                | AgentCommand::NewSession { root } => (root, false),
            };
            (root, safe)
        }
        _ => return Ok(()),
    };
    let workspace = DevelopmentWorkspace::open(root)?;
    if workspace.trust() == WorkspaceTrust::Untrusted && !safe_static {
        return Err(format!(
            "repository-controlled development execution is blocked for {}; open the TUI to inspect and trust this workspace",
            workspace.root().display()
        )
        .into());
    }
    Ok(())
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
