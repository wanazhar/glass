//! Glass Development Environment runtime.
//!
//! This crate owns the resident software-development workspace used by the
//! `glass` product. Browser intelligence remains provided by the one-way
//! `glass-browser` dependency.
//!
//! The interactive TUI separates terminal rendering from workspace work. A
//! snapshot worker hydrates resident projections and executes governed browser,
//! Git, agent, and project operations off the input loop. The UI applies
//! versioned snapshots, keeps drafts and modal state local, and restores the
//! terminal without waiting for an active bounded job.
//!
//! ## Public API
//!
//! [`DevelopmentWorkspace`] owns the resident project runtime and its trust
//! boundary. [`SharedDevelopmentWorkspace`] is the thread-safe handle used by
//! the TUI, daemon, MCP, and agent services. [`DevelopmentToolRouter`] exposes
//! governed tool execution, while [`PiReadiness`] and [`PiSessionRequest`]
//! describe the managed Pi runtime surface.
//!
//! The focused browser-only API lives in
//! [`glass_browser`](https://docs.rs/glass-browser).
//!
//! The docs.rs page documents the Rust library API; installed command behavior
//! is specified in the [CLI reference](https://github.com/wanazhar/glass/blob/main/docs/cli.md).
//!
//! ## Quick start
//!
//! ```rust,no_run
//! use glass_dev::DevelopmentWorkspace;
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let workspace = DevelopmentWorkspace::open(".")?;
//! println!("{}", workspace.root().display());
//! # Ok(())
//! # }
//! ```
//!
//! ## Modules
//!
//! [`agents`] owns resident Pi scheduling and evidence. [`browser`] provides
//! the development browser service. [`development`] contains files, editors,
//! processes, language services, and project execution. [`tools`] and
//! [`trust`] enforce governed operations, while [`tui`] and [`workspace`]
//! connect those services to the interactive product.

/// Resident Pi agent scheduling, evidence, and lifecycle control.
pub mod agents;
/// Development-browser service owned by the resident workspace worker.
pub mod browser;
/// Command-line argument and dispatch support for Glass Dev.
pub mod cli;
/// User and project customization loading.
pub mod customization;
/// Resident development daemon integration.
pub mod daemon;
/// Debugging and semantic breakpoint support.
pub mod debugger;
/// Files, editors, processes, language services, and project execution.
pub mod development;
/// Experiment orchestration and comparison.
pub mod experiments;
/// One-shot adapters for installed external coding agents.
pub mod external_agents;
/// Governed Git workspace operations.
pub mod git;
/// GitHub status, review, and pull-request shipping operations.
pub mod github;
/// Discovery and safe handoff of installed coding harnesses.
pub mod harness;
/// Development graph and causal intelligence projections.
pub mod intelligence;
/// Kernel process and runtime integration.
pub mod kernels;
/// LSP-facing language service integration.
pub mod lsp;
/// MCP server and tool integration.
pub mod mcp;
/// Managed Pi runtime readiness and sessions.
pub mod pi_runtime;
/// Task scheduling, evidence, and verification requirements.
pub mod tasks;
/// Test execution and result collection.
pub mod testing;
/// Governed development-tool routing.
pub mod tools;
/// Workspace trust decisions and persistence.
pub mod trust;
/// Interactive Glass Dev terminal application.
pub mod tui;
/// Workspace ownership and shared handles.
pub mod workspace;

use glass_browser::cli::args::Cli;

/// Resident Pi agent types and scheduler handles.
pub use agents::{
    AgentEvent, AgentId, AgentRegistry, AgentSnapshot, AgentSpec, AgentStatus, ResidentAgentBroker,
};
/// Development browser configuration, state, and service handle.
pub use browser::{BrowserRuntimeState, BrowserService, BrowserStartConfig};
/// User-facing customization and skill configuration.
pub use customization::{Customization, GlassConfig, Skill};
/// Experiment management and comparison types.
pub use experiments::{
    ExperimentComparison, ExperimentEvidence, ExperimentManager, ExperimentRanking,
    ExperimentSnapshot, ExperimentState, ExperimentTrustPolicy, ExperimentWeights,
};
/// Development intelligence graph and replay types.
pub use intelligence::{
    CausalPath, DevelopmentEdge, DevelopmentIntelligence, DevelopmentNode, DevelopmentNodeKind,
    ObservableDevelopmentEvent, ObservableEventInput, ReplayDiff,
};
/// Language-service configuration and event types.
pub use lsp::{LanguageServerConfig, LanguageService, LanguageServiceEvent};
/// Pi readiness and managed-session request types.
pub use pi_runtime::{
    PINNED_PI_SDK_VERSION, PiReadiness, PiReadinessComponent, PiReadinessState, PiSessionRequest,
};
/// Task scheduling, retry, evidence, and verification types.
pub use tasks::{
    CrewWake, CrewWakeMember, RetryPolicy, TaskBudget, TaskEvidence, TaskId, TaskScheduler,
    TaskSnapshot, TaskSpec, TaskState, VerificationRequirement, load_latest_crew_wake,
    persist_crew_wake,
};
/// Governed tool execution context and router.
pub use tools::{DevelopmentToolContext, DevelopmentToolRouter};
/// Workspace trust identities, decisions, and persistence.
pub use trust::{LocalTrustDecision, WorkspaceIdentity, WorkspaceTrust, WorkspaceTrustStore};
/// Workspace owners and thread-safe handles.
pub use workspace::{DevelopmentWorkspace, SharedDevelopmentWorkspace};

/// Dispatch the full Glass Development Environment.
///
/// Resident development commands are handled here. Browser-only commands use
/// the public browser runtime through the one-way crate dependency.
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
    if let Some(glass_browser::cli::args::Commands::Agent { action }) = &cli.command {
        enforce_legacy_development_trust(&cli)?;
        return cli::dispatch_agent(action, cli.yolo);
    }
    if let Some(glass_browser::cli::args::Commands::Harness { action }) = &cli.command {
        enforce_legacy_development_trust(&cli)?;
        return cli::dispatch_harness(action);
    }
    if let Some(glass_browser::cli::args::Commands::Daemon { action }) = &cli.command {
        return daemon::dispatch(action).await;
    }
    if let Some(glass_browser::cli::args::Commands::Project { action }) = &cli.command {
        enforce_legacy_development_trust(&cli)?;
        return cli::dispatch_project(action);
    }
    if cli.prompt.is_none()
        && matches!(
            cli.command.as_ref(),
            Some(glass_browser::cli::args::Commands::Tui)
        )
    {
        return run_development_tui(&cli);
    }
    if cli.command.is_none() && cli.prompt.is_none() && !cli.mcp {
        return run_development_tui(&cli);
    }
    enforce_legacy_development_trust(&cli)?;
    glass_browser::cli::runner::dispatch(cli).await
}

fn run_development_tui(cli: &Cli) -> Result<(), Box<dyn std::error::Error>> {
    let visual_options = tui::TuiVisualOptions {
        mode: cli.tui_live,
        backend: cli.tui_live_backend,
        quality: cli.tui_live_quality,
        fit: cli.tui_live_fit,
    };
    tui::run(
        std::env::current_dir()?,
        cli.tui_layout,
        visual_options,
        cli.yolo,
        cli.policy,
    )
}

fn enforce_legacy_development_trust(cli: &Cli) -> Result<(), Box<dyn std::error::Error>> {
    use glass_browser::cli::args::{
        AgentCommand, AgentHarness, Commands, HarnessCommand, ProjectCommand,
    };

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
        Some(Commands::Harness { action }) => match action {
            HarnessCommand::List => return Ok(()),
            HarnessCommand::Start { root, .. } => (root, false),
        },
        Some(Commands::Agent { action }) => {
            let (root, safe) = match action {
                AgentCommand::Doctor | AgentCommand::Setup { .. } | AgentCommand::Status => {
                    return Ok(());
                }
                AgentCommand::Hello { root, harness } => {
                    (root, matches!(harness, AgentHarness::Local))
                }
                AgentCommand::Prompt { root, harness, .. } => {
                    (root, matches!(harness, AgentHarness::Local))
                }
                AgentCommand::Delegate { root, sandbox, .. } => (
                    root,
                    matches!(
                        sandbox,
                        glass_browser::cli::args::ExternalAgentSandbox::ReadOnly
                    ),
                ),
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
    use crate::development::{Actor, ToolAuthorization, ToolCall};
    use glass_browser::cli::args::AgentCommand;

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
            unrestricted: allow_mutation && confirmed,
        },
        initiator: None,
        expected_generation: workspace.generation(),
        expected_project_revision: workspace.project().revision(),
    };
    Ok(workspace.execute_tool(&call, &context)?)
}
