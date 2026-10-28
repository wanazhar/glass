//! Glass Development Environment runtime.
//!
//! This crate owns the resident software-development workspace used by the
//! `glass` product. Browser intelligence remains provided by the one-way
//! `glass-browser` dependency.
//! Resident agents use the standalone `glass-core` engine. Glass owns the
//! workspace interface and its browser, Git, and review integrations; Core
//! owns provider connections and native coding tools.
//!
//! The interactive TUI separates terminal rendering from workspace work. A
//! snapshot worker hydrates resident projections and executes governed browser,
//! Git, agent, and project operations off the input loop. The UI applies
//! versioned snapshots, keeps drafts and modal state local, and restores the
//! terminal without waiting for an active bounded job.
//!
//! Desktop destinations are Agent, Code, App, Terminal, Tasks, Git, Debug, and
//! More. The shared composer dock is available on every surface.
//! [`SessionTodoList`] is the workspace-local Agent checklist persisted at
//! `.glass/todos/session.json`; it is not the overnight task DAG.
//!
//! ## Public API
//!
//! [`DevelopmentWorkspace`] owns the Glass integration workspace and its
//! project-configuration boundary. [`SharedDevelopmentWorkspace`] is the thread-safe handle used by
//! the TUI, daemon, MCP, and agent services. [`CoreReadiness`] and
//! [`CoreRuntimeOptions`] describe the resident Glass Core runtime.
//!
//! Raw process and resident-service controls are implementation details. The
//! supported public integration mutation route is [`DevelopmentWorkspace::execute_tool`]
//! with a [`tools::DevelopmentToolContext`] carrying operation-specific
//! confirmation. Glass Core owns native coding tools and their permissions.
//!
//! ```compile_fail
//! let _ = glass_dev::agents::AgentRegistry::new(".");
//! ```
//!
//! ```compile_fail
//! let _ = glass_dev::development::AgentToolGateway::default();
//! ```
//!
//! ```compile_fail
//! let _ = glass_dev::DevelopmentToolRouter::default();
//! ```
//!
//! ```compile_fail
//! let _ = glass_dev::browser::BrowserService::new(".");
//! ```
//!
//! ```compile_fail
//! let _ = glass_dev::github::ship;
//! ```
//!
//! ```compile_fail
//! let _ = glass_dev::development::ProcessManager::new(".");
//! ```
//!
//! ```compile_fail
//! let _ = glass_dev::tasks::TaskScheduler::new(".");
//! ```
//!
//! ```compile_fail
//! let _ = glass_dev::mcp::DevelopmentMcpBackend::open(".", true);
//! ```
//!
//! ```compile_fail
//! use glass_browser::browser::policy::PolicyPreset;
//! use glass_browser::cli::args::TuiLayout;
//! let _ = glass_dev::tui::DevTuiState::open_for_tui_with_policy(
//!     ".",
//!     TuiLayout::Desktop,
//!     true,
//!     PolicyPreset::Development,
//! );
//! ```
//!
//! ```compile_fail
//! let workspace = glass_dev::DevelopmentWorkspace::open(".").unwrap();
//! let _agents = workspace.agents();
//! ```
//!
//! ```compile_fail
//! let workspace = glass_dev::DevelopmentWorkspace::open(".").unwrap();
//! let _browser = workspace.browser();
//! ```
//!
//! ```compile_fail
//! let mut workspace = glass_dev::DevelopmentWorkspace::open(".").unwrap();
//! let _ = workspace.intelligence_mut();
//! ```
//!
//! The focused browser-only API lives in
//! [`glass_browser`](https://docs.rs/glass-browser).
//!
//! The docs.rs page documents the Rust library API. Published pages match the
//! crate version they were built from. Installed command behavior is specified
//! in the [CLI reference](https://github.com/wanazhar/glass/blob/main/docs/cli.md).
//! Current-checkout TUI behavior is specified in the
//! [Development TUI architecture](https://github.com/wanazhar/glass/blob/main/docs/architecture/development-tui.md)
//! guide.
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
//! [`resident_harness`] adapts the sole resident engine, [`core_runtime`], the
//! `glass-core` host runtime. [`agents`] owns resident
//! agent scheduling and evidence. [`browser`] provides the development browser
//! service. [`development`] contains files, editors, processes, language
//! services, and project execution. [`tools`] route Glass integrations, while
//! [`tui`] and [`workspace`] connect those services to the interactive product.

/// Resident agent scheduling, evidence, and lifecycle control.
pub mod agents;
/// Development-browser service owned by the resident workspace worker.
pub mod browser;
/// Command-line argument and dispatch support for Glass Dev.
pub mod cli;
/// Managed Glass Core runtime readiness and resident sessions.
pub mod core_runtime;
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
/// Configured editor completion providers for fill-in-the-middle ghosts.
pub mod fim;
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
/// Resident agent engine selection, options, and readiness.
pub mod resident_harness;
/// Task scheduling, evidence, and verification requirements.
pub mod tasks;
/// Test execution and result collection.
pub mod testing;
/// Durable Glass-owned identities for resident project conversations.
pub(crate) mod thread_store;
/// Workspace-local Agent todos persisted at `.glass/todos/session.json` (not the overnight task DAG).
pub mod todos;
/// Governed development-tool routing.
pub mod tools;
/// Interactive Glass Dev terminal application.
pub mod tui;
/// Workspace ownership and shared handles.
pub mod workspace;

use glass_browser::cli::args::Cli;

/// Resident agent event and identity types.
pub use agents::{AgentEvent, AgentId, AgentSnapshot, AgentSpec, AgentStatus, ResidentAgentBroker};
/// Development browser configuration, state, and service handle.
pub use browser::{BrowserRuntimeState, BrowserStartBackend, BrowserStartConfig};
/// Resident Glass Core readiness, options, and harness selection.
pub use core_runtime::{
    CoreReadiness, CoreReadinessComponent, CoreReadinessState, CoreRuntimeOptions,
    GLASS_CORE_BINARY_ENV, GLASS_CORE_PROTOCOL_VERSION, core_readiness, resolve_core_binary,
};
/// User-facing customization and skill configuration.
pub use customization::{Customization, GlassConfig, Skill};
/// Experiment management and comparison types.
pub use experiments::{
    ExperimentComparison, ExperimentEvidence, ExperimentRanking, ExperimentSnapshot,
    ExperimentState, ExperimentWeights,
};
/// Development intelligence graph and replay types.
pub use intelligence::{
    CausalPath, DevelopmentEdge, DevelopmentIntelligence, DevelopmentNode, DevelopmentNodeKind,
    ObservableDevelopmentEvent, ObservableEventInput, ReplayDiff,
};
/// Language-service configuration and event types.
pub use lsp::{LanguageServerConfig, LanguageServiceEvent};
/// Resident harness identity, options, and readiness.
pub use resident_harness::{
    AgentToolExecutor, ResidentHarnessKind, ResidentHarnessOptions, ResidentReadiness,
    ResidentSessionRequest, harness_kind_from_id, resident_readiness,
};
/// Task scheduling, retry, evidence, and verification types.
pub use tasks::{
    CrewWake, CrewWakeLiveEvidence, CrewWakeMember, RetryPolicy, TaskBudget, TaskEvidence, TaskId,
    TaskSnapshot, TaskSpec, TaskState, VerificationRequirement, load_latest_crew_wake,
};
/// Workspace-local Agent todos persisted at `.glass/todos/session.json` (not the overnight task DAG).
pub use todos::{SessionTodo, SessionTodoList, TodoStatus};
/// Context carrying host-granted authority and confirmation for tool execution.
pub use tools::DevelopmentToolContext;
/// Shared workspace handle.
pub use workspace::{DevelopmentWorkspace, SharedDevelopmentWorkspace};

/// Dispatch the full Glass Development Environment.
///
/// Resident development commands are handled here. Browser-only commands use
/// the public browser runtime through the one-way crate dependency.
pub async fn dispatch(cli: Cli) -> Result<(), Box<dyn std::error::Error>> {
    cli::configure_json_output(glass_browser::cli::runner::json_output_requested());
    if cli.mcp {
        let policy = glass_browser::cli::runner::policy_from_cli(&cli)?;
        let backend = std::sync::Arc::new(mcp::DevelopmentMcpBackend::open_with_browser_policy(
            std::env::current_dir()?,
            policy,
        )?);
        return glass_browser::mcp::server::run_mcp_server_with_backend(&cli, backend).await;
    }
    if let Some(glass_browser::cli::args::Commands::Agent { action, .. }) = &cli.command
        && matches!(
            action,
            glass_browser::cli::args::AgentCommand::Tool { .. }
                | glass_browser::cli::args::AgentCommand::ToolFile { .. }
        )
    {
        let result = dispatch_external_tool(action).await?;
        glass_browser::cli::runner::print_cli_result(&result)?;
        return Ok(());
    }
    if let Some(glass_browser::cli::args::Commands::Agent { action, json }) = &cli.command {
        return cli::dispatch_agent(
            action,
            *json || glass_browser::cli::runner::json_output_requested(),
        );
    }
    if let Some(glass_browser::cli::args::Commands::Harness { action, json }) = &cli.command {
        return cli::dispatch_harness(
            action,
            *json || glass_browser::cli::runner::json_output_requested(),
        );
    }
    if let Some(glass_browser::cli::args::Commands::Daemon { action }) = &cli.command {
        return daemon::dispatch(action).await;
    }
    if let Some(glass_browser::cli::args::Commands::Project {
        action,
        allow_mutation,
        yes,
        json,
    }) = &cli.command
    {
        return cli::dispatch_project(
            action,
            *allow_mutation,
            *yes,
            *json || glass_browser::cli::runner::json_output_requested(),
        );
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
    if cli.browser_runtime != glass_browser::BrowserRuntime::Chromium
        || cli.browser_endpoint.is_some()
    {
        return glass_browser::cli::runner::dispatch(cli).await;
    }
    glass_browser::cli::runner::dispatch(cli).await
}

fn run_development_tui(cli: &Cli) -> Result<(), Box<dyn std::error::Error>> {
    let visual_options = tui::TuiVisualOptions {
        mode: cli.tui_live,
        backend: cli.tui_live_backend,
        quality: cli.tui_live_quality,
        fit: cli.tui_live_fit,
    };
    let browser_start_defaults = browser_start_defaults_from_cli(cli)?;
    let policy = glass_browser::cli::runner::policy_from_cli(cli)?;
    tui::run_with_browser_policy(
        std::env::current_dir()?,
        cli.tui_layout,
        visual_options,
        browser_start_defaults,
        policy,
    )
}

fn browser_start_defaults_from_cli(
    cli: &Cli,
) -> Result<BrowserStartConfig, Box<dyn std::error::Error>> {
    Ok(BrowserStartConfig {
        backend: match cli.browser_runtime {
            glass_browser::BrowserRuntime::Chromium => {
                crate::browser::BrowserStartBackend::Chromium
            }
            browser if browser.is_native() => crate::browser::BrowserStartBackend::Native,
            _ => {
                return Err(
                    "Glass Dev supports the native and explicit Chromium browser backends".into(),
                );
            }
        },
        port: cli.port,
        // CLI port defaults remain explicit to preserve the existing CLI
        // contract; the runtime tool can distinguish an omitted port.
        port_explicit: true,
        attach: cli.attach,
        incognito: cli.incognito,
        headed: cli.headed,
        profile: cli.profile.clone(),
        chrome_path: cli.chrome_path.clone(),
        modal_dialogs: !cli.attach,
        interaction: cli.interaction,
        audit: cli.audit,
        viewport: cli
            .viewport
            .as_deref()
            .map(parse_tui_viewport)
            .transpose()?,
    })
}

fn parse_tui_viewport(value: &str) -> Result<(u32, u32), Box<dyn std::error::Error>> {
    let (width, height) = value
        .split_once('x')
        .ok_or("viewport must use WIDTHxHEIGHT")?;
    let width = width.parse::<u32>()?;
    let height = height.parse::<u32>()?;
    if !(320..=10_000).contains(&width) || !(240..=10_000).contains(&height) {
        return Err("viewport dimensions must be width 320..10000 and height 240..10000".into());
    }
    Ok((width, height))
}

async fn dispatch_external_tool(
    action: &glass_browser::cli::args::AgentCommand,
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
            *allow_mutation,
            *yes,
        ),
        AgentCommand::ToolFile {
            path,
            root,
            allow_mutation,
            yes,
        } if std::env::var_os("GLASS_DEV_DAEMON_SOCKET").is_some() => {
            return daemon::forward_resident_tool_file(path, root, *allow_mutation, *yes).await;
        }
        AgentCommand::ToolFile {
            path,
            root,
            allow_mutation,
            yes,
        } => (
            daemon::read_private_tool_call(path)?,
            root,
            *allow_mutation,
            *yes,
        ),
        _ => return Err("expected an external agent tool action".into()),
    };
    let mut workspace = DevelopmentWorkspace::open(root)?;
    let context = DevelopmentToolContext {
        authorization: ToolAuthorization::from_user_confirmation(
            Actor::external("cli"),
            allow_mutation,
            confirmed,
        ),
        initiator: None,
        expected_generation: workspace.generation(),
        expected_project_revision: workspace.project().revision(),
    };
    Ok(workspace.execute_tool(&call, &context)?)
}

#[cfg(test)]
mod authorization_boundary_tests {
    use super::*;
    use clap::Parser;
    use glass_browser::cli::args::Cli;
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT_ROOT: AtomicU64 = AtomicU64::new(0);

    #[test]
    fn global_browser_start_options_reach_the_resident_tui_defaults() {
        // Clap's generated command graph needs more stack than the default
        // Rust test worker when parsing the full global option set.
        std::thread::Builder::new()
            .stack_size(16 * 1024 * 1024)
            .spawn(|| {
                let cli = Cli::try_parse_from([
                    "glass",
                    "--port",
                    "9333",
                    "--profile",
                    "work",
                    "--attach",
                    "--headed",
                    "--viewport",
                    "1440x900",
                    "--interaction",
                    "fast",
                    "--audit",
                ])
                .unwrap();
                let config = browser_start_defaults_from_cli(&cli).unwrap();
                assert_eq!(config.port, 9333);
                assert_eq!(config.profile, "work");
                assert!(config.attach && config.headed);
                assert_eq!(config.viewport, Some((1440, 900)));
                assert_eq!(
                    config.interaction,
                    glass_browser::browser::session::InteractionMode::Fast
                );
                assert!(config.audit);
                assert!(!config.modal_dialogs);
            })
            .unwrap()
            .join()
            .unwrap();
    }

    #[test]
    fn cli_agent_tool_writes_after_explicit_authorization() {
        let root = std::env::temp_dir().join(format!(
            "glass-cli-agent-tool-{}-{}",
            std::process::id(),
            NEXT_ROOT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&root).unwrap();
        let call = crate::development::ToolCall {
            id: "cli-boundary-check".into(),
            name: "glass.file.write".into(),
            arguments: serde_json::json!({"path":"blocked.txt","content":"blocked"}),
        };
        let call = serde_json::to_string(&call).unwrap();
        let root_arg = root.to_str().unwrap().to_string();
        std::thread::Builder::new()
            .stack_size(16 * 1024 * 1024)
            .spawn(move || {
                let cli = Cli::try_parse_from([
                    "glass",
                    "agent",
                    "tool",
                    call.as_str(),
                    "--root",
                    root_arg.as_str(),
                    "--allow-mutation",
                    "--yes",
                ])
                .unwrap();
                assert_ne!(
                    cli.browser_runtime,
                    glass_browser::BrowserRuntime::Chromium,
                    "the test must cover Glass Dev's default native runtime route"
                );
                let runtime = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .unwrap();
                runtime
                    .block_on(dispatch(cli))
                    .expect("unfamiliar workspaces do not block Glass tool execution");
                assert_eq!(
                    std::fs::read_to_string(root.join("blocked.txt")).unwrap(),
                    "blocked"
                );
                std::fs::remove_dir_all(root).unwrap();
            })
            .unwrap()
            .join()
            .unwrap();
    }
}
