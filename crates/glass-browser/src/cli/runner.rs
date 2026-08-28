//! CLI command dispatch and session orchestration.
//!
//! Routes parsed CLI arguments to the appropriate runner: one-shot browser
//! commands, interactive TUI, or the MCP stdio server.

use super::args::{
    BackendCommand, CertifyCommand, CheckpointCommand, Cli, Commands, DaemonCommand, IrCommand,
    KnowledgeCommand, KnowledgeInvalidationState, McpClient, MemoryCommand, ProfileCommand,
    ReplayCommand, ResultCommand, SessionCommand, SnapshotCommand, SurfaceCommand, TaskCommand,
    WorkflowAuthoringCommand, WorkspaceCommand,
};
use crate::browser::policy::{BrowserPolicy, PolicyCapability};
use crate::browser::profile::ProfileManager;
use crate::browser::session::{
    ActionKind, BatchStep, BrowserResult, BrowserSession, CheckpointV1, Cookie,
    KnowledgeConfidence, KnowledgeStore, Locator, PdfOptions, ReconciliationOptions,
    SemanticIntentExecutionRequest, SemanticIntentRequest, SemanticObservationLevel,
    SessionOptions, SessionSnapshotStore, StructuredExtractionRequest, VerificationPredicate,
    VisualCaptureOptions, WaitCondition, WorkflowAuthoringFormat, WorkflowCheckpoint,
    WorkflowDefinition, WorkflowDiagnosticSeverity, WorkflowRecordingSession, compile_workflow,
    default_knowledge_store_path, diff_workflows, format_workflow_yaml, preview_workflow,
    record_semantic_events,
};
use crate::browser::{BackendFactory, CdpSessionBackend};
use crate::browser_backend::{
    ActionRequest, BackendProfile, BrowserBackendDispatcher, BrowserCapability, ContextRequest,
    EffectsRequest, EvidenceLevel, EvidenceRequest, NavigationRequest, SemanticAction,
};
use crate::capabilities::GlassCapabilityManifest;
use crate::protocol::{
    GLASS_PROTOCOL_VERSION, GlassRequest, TASK_COMPILE_OPERATION, TASK_EXECUTE_OPERATION,
    TASK_VALIDATE_OPERATION, WEB_IR_CONTINUITY_OPERATION, WEB_IR_INSPECT_OPERATION,
    WEB_IR_VALIDATE_OPERATION,
};
use crate::reliability::{
    ReliabilityFixtureManifest, ReliabilityReplayBundle, ReliabilityScenario,
    ReliabilityScenarioObservation, build_reliability_scorecard,
};
use crate::reliability_runner::{ReliabilityRunOptions, run_reliability_scenario};
use crate::results::{
    ExperienceProvenance, ExperienceResult, ProvenanceSource, ResponseMode, ResultStore,
    project_and_store,
};
use crate::surfaces::SurfaceSet;
use crate::workspace::{ResourceReference, WorkspaceId, WorkspaceStore};
use base64::Engine;
use clap::ValueEnum;
use serde::Serialize;
use serde_json::Value;
use std::collections::BTreeMap;
use std::io::{IsTerminal, Read};
use std::path::Path;
use std::process::Stdio;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

pub(crate) fn parse_viewport(value: &str) -> BrowserResult<(i64, i64)> {
    let (width, height) = value
        .split_once('x')
        .ok_or("viewport must use WIDTHxHEIGHT, for example 1280x800")?;
    let width = width
        .parse::<i64>()
        .map_err(|_| "viewport width must be an integer")?;
    let height = height
        .parse::<i64>()
        .map_err(|_| "viewport height must be an integer")?;
    if !(320..=10000).contains(&width) || !(240..=10000).contains(&height) {
        return Err("viewport dimensions must be width 320..10000 and height 240..10000".into());
    }
    Ok((width, height))
}
fn should_run_tui(stdin_is_terminal: bool, stdout_is_terminal: bool) -> bool {
    stdin_is_terminal && stdout_is_terminal
}

fn start_here_message() -> &'static str {
    "Glass is ready for a first step; no browser was started.\n\nSTART HERE\n  glass \"navigate to https://example.com\"\n  glass doctor\n  glass --help\n\nFor the interactive terminal UI, run `glass tui` from a terminal."
}

fn print_start_here() {
    println!("{}", start_here_message());
}

/// Top-level command-line entry point: parses CLI arguments and dispatches
/// to the appropriate runner (one-shot, TUI, or MCP server).
pub async fn dispatch(cli: Cli) -> BrowserResult<()> {
    dispatch_product(cli, false).await
}

/// Dispatch the browser-only product. Development commands and the
/// development workspace are intentionally unavailable from `glass-browser`.
pub async fn dispatch_browser(cli: Cli) -> BrowserResult<()> {
    dispatch_product(cli, false).await
}

async fn dispatch_product(mut cli: Cli, _development_enabled: bool) -> BrowserResult<()> {
    let policy = policy_from_cli(&cli)?;
    if cli.experimental_extensions {
        eprintln!(concat!(
            "warning: experimental extensions are enabled; extension code is untrusted, ",
            "sandbox support is required, and behavior may break"
        ));
    }
    if cli.mcp {
        return crate::mcp::server::run_mcp_server(&cli).await;
    }

    match &cli.command {
        Some(Commands::Update {
            dry_run,
            version,
            force,
            registry,
        }) => {
            crate::update::run(crate::update::UpdateOptions {
                dry_run: *dry_run,
                version: version.clone(),
                force: *force,
                registry: registry.clone(),
            })?;
            return Ok(());
        }
        Some(Commands::InstallChromium { update }) => {
            let path = crate::browser::chrome::download_chromium(*update).await?;
            println!("Chrome for Testing installed at {}", path.display());
            return Ok(());
        }
        Some(Commands::Capabilities) => {
            print_json(
                &GlassCapabilityManifest::for_policy_with_experimental_extensions(
                    &policy,
                    cli.experimental_extensions,
                ),
            )?;
            return Ok(());
        }
        Some(Commands::Daemon { action }) => {
            dispatch_daemon(action).await?;
            return Ok(());
        }
        Some(Commands::Doctor { json }) => {
            dispatch_doctor(&cli, &policy, *json).await?;
            return Ok(());
        }
        Some(Commands::McpConfig { client, print }) => {
            dispatch_mcp_config(*client, *print)?;
            return Ok(());
        }
        Some(Commands::Certify { action }) if !matches!(action, CertifyCommand::Run { .. }) => {
            dispatch_certify(action)?;
            return Ok(());
        }
        Some(Commands::Profiles { action }) => {
            policy.require(PolicyCapability::PersistentProfile)?;
            dispatch_profiles(&cli, action.as_ref())?;
            return Ok(());
        }
        Some(Commands::DeleteProfile { name }) => {
            policy.require(PolicyCapability::PersistentProfile)?;
            profile_manager_for_cli(&cli).delete_profile(name)?;
            println!("deleted profile {name}");
            return Ok(());
        }
        Some(Commands::Knowledge { action }) => {
            policy.require(PolicyCapability::PersistentProfile)?;
            dispatch_knowledge(action, cli.knowledge_store.as_deref(), &cli.profile)?;
            return Ok(());
        }
        Some(Commands::Workspace { action }) => {
            dispatch_workspace(action)?;
            return Ok(());
        }
        Some(Commands::Project { action }) => {
            let _ = action;
            return Err("project commands belong to the `glass` development product".into());
        }
        Some(Commands::Agent { action }) => {
            let _ = action;
            return Err("agent harness commands belong to the `glass` development product".into());
        }
        Some(Commands::Harness { action }) => {
            let _ = action;
            return Err("harness commands belong to the `glass` development product".into());
        }
        Some(Commands::Memory { action }) => {
            policy.require(PolicyCapability::PersistentProfile)?;
            dispatch_memory(action, cli.knowledge_store.as_deref(), &cli.profile)?;
            return Ok(());
        }
        Some(Commands::Surfaces { action }) => {
            dispatch_surfaces(action)?;
            return Ok(());
        }
        Some(Commands::Backend { action }) => {
            dispatch_backend(action).await?;
            return Ok(());
        }
        Some(Commands::Replay { action }) => {
            dispatch_replay(action)?;
            return Ok(());
        }
        Some(Commands::Snapshot { action }) if !matches!(action, SnapshotCommand::Create) => {
            dispatch_snapshot(action, &cli.profile)?;
            return Ok(());
        }
        Some(Commands::Result { action }) => {
            dispatch_result(action)?;
            return Ok(());
        }
        Some(Commands::Workflow {
            action: Some(action),
            ..
        }) => {
            dispatch_workflow_authoring(action)?;
            return Ok(());
        }
        Some(Commands::Task {
            action: TaskCommand::Validate { .. } | TaskCommand::Compile { .. },
        }) => {
            if let Some(Commands::Task { action }) = &cli.command {
                dispatch_task(action)?;
            }
            return Ok(());
        }
        Some(Commands::Task {
            action: TaskCommand::Execute { .. },
        }) => {}
        Some(Commands::Ir { action }) => {
            dispatch_ir(action)?;
            return Ok(());
        }
        Some(Commands::Browser { action }) if cli.prompt.is_none() => {
            if let Some(name) = cli.session.clone() {
                cli.port = persistent_session_port(&name)?;
                cli.attach = true;
            }
            let _ = action;
            return crate::tui::app::run_tui_for_product(&cli, false).await;
        }
        Some(Commands::Session { action }) => {
            dispatch_session(&cli, action, &policy).await?;
            return Ok(());
        }
        Some(Commands::Help { topic }) => {
            print_grouped_help(topic.as_deref());
            return Ok(());
        }
        Some(Commands::Tui) | None if cli.prompt.is_none() => {
            if should_run_tui(
                std::io::stdin().is_terminal(),
                std::io::stdout().is_terminal(),
            ) {
                return crate::tui::app::run_tui_for_product(&cli, false).await;
            }
            print_start_here();
            return Ok(());
        }
        _ => {}
    }

    if let Some(Commands::RecoverRun { execution_id }) = &cli.command {
        let result = crate::browser::session::recover_run(execution_id)?;
        print_json_mode(&result, cli.response_mode)?;
        return Ok(());
    }

    if let Some(Commands::SmokeSites {
        input,
        stop_on_error,
    }) = &cli.command
    {
        let viewport = cli.viewport.as_deref().map(parse_viewport).transpose()?;
        return super::site_smoke::run(&cli, policy, input, viewport, *stop_on_error).await;
    }

    let viewport = cli.viewport.as_deref().map(parse_viewport).transpose()?;
    let session_port = cli
        .session
        .as_deref()
        .map(persistent_session_port)
        .transpose()?;
    let options = SessionOptions {
        port: session_port.unwrap_or(cli.port),
        chrome_path: cli.chrome_path.clone(),
        profile: cli.profile.clone(),
        incognito: cli.incognito,
        attach: cli.attach || cli.session.is_some(),
        target_id: cli.target_id.clone(),
        frame_id: cli.frame_id.clone(),
        headed: cli.headed,
        interaction_mode: cli.interaction,
        audit: cli.audit,
        policy: None,
    };
    let session =
        BrowserSession::start_with_policy_and_viewport(&options, policy, viewport).await?;
    let result = if let Some(prompt) = &cli.prompt {
        run_prompt(&session, prompt, cli.response_mode).await
    } else if let Some(command) = &cli.command {
        run_command(&session, command, cli.response_mode).await
    } else {
        Ok(())
    };
    if let Err(error) = &result
        && cli.trace_on_error
    {
        let trace = session
            .failure_trace_for(
                cli_trace_action(cli.command.as_ref(), cli.prompt.as_deref()),
                error.to_string(),
            )
            .await;
        eprintln!("{}", serde_json::to_string(&trace)?);
    }
    let close_result = session.close().await;
    result?;
    close_result
}

async fn dispatch_daemon(action: &DaemonCommand) -> BrowserResult<()> {
    match action {
        DaemonCommand::Start { socket, status } => {
            print_json(&crate::daemon::start(socket.as_deref(), status.as_deref()).await?)?;
        }
        DaemonCommand::Status { socket, status } => {
            print_json(&crate::daemon::status(
                socket.as_deref(),
                status.as_deref(),
            )?)?;
        }
        DaemonCommand::Stop { socket, status } => {
            crate::daemon::stop(socket.as_deref(), status.as_deref())?;
            print_json(&serde_json::json!({"status": "stopped"}))?;
        }
        DaemonCommand::Doctor { socket, status } => {
            print_json(&crate::daemon::doctor(
                socket.as_deref(),
                status.as_deref(),
            )?)?;
        }
        DaemonCommand::Logs { status } => {
            print_json(&crate::daemon::logs(status.as_deref())?)?;
        }
        DaemonCommand::AcknowledgeRecovery {
            status,
            request_ids,
        } => {
            print_json(&crate::daemon::acknowledge_recovery(
                status.as_deref(),
                request_ids,
            )?)?;
        }
        DaemonCommand::Serve { socket, status } => {
            crate::daemon::serve(socket, status).await?;
        }
    }
    Ok(())
}

async fn dispatch_session(
    cli: &Cli,
    action: &SessionCommand,
    policy: &BrowserPolicy,
) -> BrowserResult<()> {
    match action {
        SessionCommand::Start { name } => {
            if cli.attach {
                return Err("session start launches Chrome; remove `--attach`".into());
            }
            let record = crate::browser::persistent::start(
                crate::browser::persistent::PersistentSessionConfig {
                    name: name.clone(),
                    port: cli.port,
                    profile: cli.profile.clone(),
                    headed: cli.headed,
                    chrome_path: cli.chrome_path.clone(),
                    policy_args: policy_forward_args(cli),
                },
            )
            .await?;
            print_json(&serde_json::to_value(record)?)?;
        }
        SessionCommand::Status { name } => {
            print_json(&crate::browser::persistent::status(name)?)?;
        }
        SessionCommand::Stop { name } => {
            print_json(&crate::browser::persistent::stop(name).await?)?;
        }
        SessionCommand::Open { name } => {
            println!("{}", crate::browser::persistent::open_message(name)?);
        }
        SessionCommand::Serve {
            name,
            socket,
            status,
        } => {
            crate::browser::persistent::serve(
                crate::browser::persistent::PersistentSessionServeConfig {
                    name: name.clone(),
                    socket: socket.clone(),
                    status_path: status.clone(),
                    port: cli.port,
                    profile: cli.profile.clone(),
                    headed: cli.headed,
                    chrome_path: cli.chrome_path.clone(),
                    policy: policy.clone(),
                },
            )
            .await?;
        }
    }
    Ok(())
}

fn persistent_session_port(name: &str) -> BrowserResult<u16> {
    let value = crate::browser::persistent::status(name)?;
    if value.get("state").and_then(Value::as_str) != Some("running") {
        return Err(format!(
            "persistent session `{name}` is not running; start it with `glass session start {name}`"
        )
        .into());
    }
    value
        .get("port")
        .and_then(Value::as_u64)
        .and_then(|port| u16::try_from(port).ok())
        .ok_or_else(|| "persistent session has no valid verified port".into())
}

fn policy_forward_args(cli: &Cli) -> Vec<String> {
    let mut args = vec![
        cli.policy
            .to_possible_value()
            .map(|value| value.get_name().to_string())
            .unwrap_or_else(|| "development".into()),
    ];
    if cli.yolo {
        args.push("--yolo".into());
    }
    for (flag, values) in [
        ("--policy-allow", &cli.policy_allow),
        ("--policy-confirm", &cli.policy_confirm),
        ("--policy-confirm-once", &cli.policy_confirm_once),
    ] {
        for value in values {
            if let Some(value) = value.to_possible_value() {
                args.push(flag.into());
                args.push(value.get_name().into());
            }
        }
    }
    for (flag, values) in [
        ("--policy-allow-host", &cli.policy_allow_host),
        ("--policy-deny-host", &cli.policy_deny_host),
    ] {
        for value in values {
            args.push(flag.into());
            args.push(value.clone());
        }
    }
    args
}

fn print_grouped_help(topic: Option<&str>) {
    match topic.unwrap_or("overview") {
        "browser" => println!(
            "BROWSER\n\n  glass browser                 launch the browser-first terminal\n  glass observe --level interactive\n                               inspect the current semantic page\n  glass navigate URL            open one page in a bounded operation\n  glass --session NAME observe  attach to a persistent session\n"
        ),
        "session" => println!(
            "SESSION\n\n  glass session start [NAME]    launch a persistent local browser\n  glass session status [NAME]   inspect owner and browser health\n  glass session open [NAME]     print attach commands\n  glass session stop [NAME]     close the owned browser\n"
        ),
        "skills" => println!(
            "SKILLS\n\n  browser-observe               semantic observation and stable refs\n  browser-actions               guarded navigation, click, type, and scroll\n  workflow                      bounded workflows with checkpoints\n  evidence                      redacted revisions and replayable results\n\nPi receives the complete embedded contract from assets/pi-glass-system.md.\n"
        ),
        _ => println!(
            "GLASS START HERE\n\n  glass browser                 start the browser-first terminal\n  glass session start           keep one browser alive between commands\n  glass help browser            browser commands\n  glass help session            persistent session commands\n  glass help skills             embedded agent workflow skills\n  glass doctor                  check local browser/runtime support\n\nStructured observation is the default. Use `glass <command> --help` for exact flags.\n"
        ),
    }
}

async fn dispatch_doctor(cli: &Cli, policy: &BrowserPolicy, json: bool) -> BrowserResult<()> {
    let executable = std::env::current_exe()
        .ok()
        .map(|path| path.display().to_string());
    let resolved_chrome = crate::browser::chrome::resolve_chrome_path(cli.chrome_path.clone());
    let chrome_path = resolved_chrome
        .as_ref()
        .map(|path| path.display().to_string());
    let profile_manager = ProfileManager::for_browser(resolved_chrome.as_deref());
    let profile_path = profile_manager.profile_dir(&cli.profile);
    let runtime_path = default_result_store_path();
    let profile_writable = path_is_writable(profile_path.parent());
    let runtime_writable = path_is_writable(runtime_path.parent());
    let browser_version = chrome_path.as_deref().and_then(|path| {
        std::process::Command::new(path)
            .arg("--version")
            .output()
            .ok()
            .filter(|output| output.status.success())
            .and_then(|output| String::from_utf8(output.stdout).ok())
            .map(|version| version.trim().to_string())
    });
    let mcp_initialized = probe_mcp_initialization(executable.as_deref()).await;
    let (daemon_socket, daemon_status) = crate::daemon::default_paths();
    let daemon = crate::daemon::doctor(Some(&daemon_socket), Some(&daemon_status))?;
    let profiles = profile_manager.list_profiles().unwrap_or_default();
    let knowledge_path = cli
        .knowledge_store
        .clone()
        .unwrap_or_else(|| default_knowledge_store_path(&cli.profile));
    let knowledge_exists = knowledge_path.is_file();
    let manifest = GlassCapabilityManifest::for_policy_with_experimental_extensions(
        policy,
        cli.experimental_extensions,
    );
    let platform_supported = manifest.constraints.platform != "unsupported";
    let config_root = dirs::config_dir();
    let config_writable = path_is_writable(config_root.as_deref());
    let browser_available = chrome_path.is_some();
    let status = if platform_supported
        && browser_available
        && config_writable
        && profile_writable
        && runtime_writable
        && mcp_initialized
    {
        "ready"
    } else {
        "degraded"
    };
    let findings = serde_json::json!([
        {
            "severity": "info",
            "code": "runtime.executable",
            "message": executable.as_deref().unwrap_or("unable to resolve executable path"),
            "remediation": null
        },
        {
            "severity": if browser_available { "info" } else { "warning" },
            "code": if browser_available { "browser.found" } else { "browser.missing" },
            "message": if browser_available { "Chrome or Chromium was discovered" } else { "Chrome or Chromium was not discovered" },
            "remediation": if browser_available { Value::Null } else { Value::String("install Chromium or pass --chrome-path".into()) }
        },
        {
            "severity": if browser_version.is_some() { "info" } else { "warning" },
            "code": if browser_version.is_some() { "browser.version" } else { "browser.versionUnavailable" },
            "message": browser_version.as_deref().unwrap_or("browser version could not be queried"),
            "remediation": if browser_version.is_some() { Value::Null } else { Value::String("verify the browser executable is runnable".into()) }
        },
        {
            "severity": if profile_writable { "info" } else { "warning" },
            "code": if profile_writable { "runtime.profileWritable" } else { "runtime.profileNotWritable" },
            "message": profile_path.display().to_string(),
            "remediation": if profile_writable { Value::Null } else { Value::String("choose a writable profile parent directory".into()) }
        },
        {
            "severity": if runtime_writable { "info" } else { "warning" },
            "code": if runtime_writable { "runtime.artifactStoreWritable" } else { "runtime.artifactStoreNotWritable" },
            "message": runtime_path.display().to_string(),
            "remediation": if runtime_writable { Value::Null } else { Value::String("choose a writable cache directory".into()) }
        },
        {
            "severity": if config_writable { "info" } else { "warning" },
            "code": if config_writable { "runtime.configWritable" } else { "runtime.configNotWritable" },
            "message": if config_writable { "Glass configuration directory is writable" } else { "Glass configuration directory is unavailable or not writable" },
            "remediation": if config_writable { Value::Null } else { Value::String("choose a writable HOME/XDG config directory".into()) }
        },
        {
            "severity": "info",
            "code": "mcp.stdoutClean",
            "message": "MCP uses stdout for protocol frames and diagnostics use stderr",
            "remediation": null
        },
        {
            "severity": if mcp_initialized { "info" } else { "warning" },
            "code": if mcp_initialized { "mcp.initialized" } else { "mcp.initializationFailed" },
            "message": if mcp_initialized { "MCP initialize completed with clean stdout" } else { "MCP initialize did not return a valid response" },
            "remediation": if mcp_initialized { Value::Null } else { Value::String("run the installed executable with --mcp and inspect stderr".into()) }
        },
        {
            "severity": "info",
            "code": "capability.manifest",
            "message": "capability statuses are included in the report",
            "remediation": null
        }
    ]);
    let report = serde_json::json!({
        "status": status,
        "version": env!("CARGO_PKG_VERSION"),
        "platform": manifest.constraints.platform,
        "executable": executable,
        "browser": {
            "family": manifest.constraints.browser_family,
            "version": browser_version,
            "chromeAvailable": browser_available,
            "chromePath": chrome_path,
            "cdpPort": cli.port,
            "cdpReachable": crate::browser::chrome::check_chrome_health(cli.port).await,
        },
        "profile": {"path": profile_path, "writable": profile_writable},
        "runtime": {"artifactStore": runtime_path, "writable": runtime_writable},
        "daemon": daemon,
        "profiles": {"count": profiles.len(), "names": profiles},
        "policy": {
            "preset": manifest.constraints.policy,
            "capabilities": manifest.capabilities,
            "capabilityStatuses": manifest.capability_statuses,
        },
        "knowledgeStore": {"path": knowledge_path, "exists": knowledge_exists},
        "extensions": {
            "enabled": manifest.capabilities.get("extensions").copied().unwrap_or(false),
            "status": manifest.capability_statuses.get("extensions"),
            "loader": "disabled",
        },
        "findings": findings,
    });
    if json {
        print_json(&report)?;
    } else {
        println!("Glass doctor: {status}");
        for finding in report["findings"].as_array().into_iter().flatten() {
            println!(
                "{} {}: {}",
                finding["severity"].as_str().unwrap_or("info"),
                finding["code"].as_str().unwrap_or("runtime.unknown"),
                finding["message"].as_str().unwrap_or("unknown")
            );
        }
    }
    Ok(())
}

fn path_is_writable(path: Option<&Path>) -> bool {
    let Some(path) = path else {
        return false;
    };
    let candidate = if path.exists() {
        path.to_path_buf()
    } else {
        path.ancestors()
            .find(|ancestor| ancestor.exists())
            .unwrap_or(path)
            .to_path_buf()
    };
    candidate.is_dir()
        && std::fs::metadata(&candidate)
            .map(|metadata| !metadata.permissions().readonly())
            .unwrap_or(false)
}

async fn probe_mcp_initialization(executable: Option<&str>) -> bool {
    let Some(executable) = executable else {
        return false;
    };
    let mut child = match tokio::process::Command::new(executable)
        .arg("--mcp")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
    {
        Ok(child) => child,
        Err(_) => return false,
    };
    let request = br#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"glass-doctor","version":"0.2.2"}}}"#;
    let Some(mut stdin) = child.stdin.take() else {
        let _ = child.kill().await;
        return false;
    };
    if stdin.write_all(request).await.is_err() || stdin.write_all(b"\n").await.is_err() {
        let _ = child.kill().await;
        return false;
    }
    drop(stdin);
    let Some(stdout) = child.stdout.take() else {
        let _ = child.kill().await;
        return false;
    };
    let mut reader = BufReader::new(stdout);
    let mut line = String::new();
    let success = tokio::time::timeout(Duration::from_secs(2), reader.read_line(&mut line))
        .await
        .ok()
        .and_then(Result::ok)
        .is_some_and(|bytes| bytes > 0 && line.contains("\"result\""));
    let _ = child.kill().await;
    success
}

fn dispatch_mcp_config(client: McpClient, _print: bool) -> BrowserResult<()> {
    let executable = std::env::current_exe()?;
    let command = executable.display().to_string();
    let value = match client {
        McpClient::Generic => serde_json::json!({
            "command": command,
            "args": ["--mcp"]
        }),
        McpClient::ClaudeCode => serde_json::json!({
            "mcpServers": {"glass": {"command": command, "args": ["--mcp"]}}
        }),
        McpClient::Codex => serde_json::json!({
            "mcp_servers": {"glass": {"command": command, "args": ["--mcp"]}}
        }),
    };
    print_json(&value)?;
    Ok(())
}

fn dispatch_result(action: &ResultCommand) -> BrowserResult<()> {
    let root = default_result_store_path();
    let store = ResultStore::new(root);
    match action {
        ResultCommand::Show { result_id, section } => {
            let artifact = store.load(result_id)?;
            let value = if let Some(section) = section {
                artifact
                    .details
                    .get(section)
                    .cloned()
                    .ok_or_else(|| format!("result section not found: {section}"))?
            } else {
                serde_json::to_value(artifact)?
            };
            print_json(&value)?;
        }
        ResultCommand::Purge { older_than } => {
            let age = parse_result_age(older_than)?;
            print_json(&serde_json::json!({
                "removed": store.purge_older_than(age)?,
                "olderThan": older_than,
            }))?;
        }
    }
    Ok(())
}

fn default_result_store_path() -> std::path::PathBuf {
    dirs::cache_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join("glass")
        .join("results")
}

fn parse_result_age(value: &str) -> BrowserResult<Duration> {
    let (number, suffix) = value.split_at(
        value
            .find(|character: char| !character.is_ascii_digit())
            .unwrap_or(value.len()),
    );
    let amount: u64 = number
        .parse()
        .map_err(|_| "older-than must be a positive duration such as 7d or 24h")?;
    if amount == 0 {
        return Err("older-than must be positive".into());
    }
    let seconds = match suffix {
        "s" => amount,
        "m" => amount.saturating_mul(60),
        "h" => amount.saturating_mul(60 * 60),
        "d" => amount.saturating_mul(24 * 60 * 60),
        _ => return Err("older-than must end in s, m, h, or d".into()),
    };
    Ok(Duration::from_secs(seconds))
}

fn cli_trace_action(command: Option<&Commands>, prompt: Option<&str>) -> ActionKind {
    if let Some(prompt) = prompt {
        let lower = prompt.trim().to_ascii_lowercase();
        return if lower.starts_with("double click ") {
            ActionKind::DoubleClick
        } else if lower.starts_with("click ") {
            ActionKind::Click
        } else if lower.starts_with("type ") {
            ActionKind::Type
        } else {
            ActionKind::Click
        };
    }
    match command {
        Some(Commands::DoubleClick { .. }) => ActionKind::DoubleClick,
        Some(Commands::ClickExpectPopup { .. }) => ActionKind::ClickExpectPopup,
        Some(Commands::Click { .. })
        | Some(Commands::Preflight { .. })
        | Some(Commands::ClickAt { .. }) => ActionKind::Click,
        Some(Commands::Hover { .. }) => ActionKind::Hover,
        Some(Commands::Drag { .. }) => ActionKind::Drag,
        Some(Commands::Type { .. }) => ActionKind::Type,
        Some(Commands::Key { .. }) => ActionKind::KeyPress,
        Some(Commands::KeyDown { .. }) => ActionKind::KeyDown,
        Some(Commands::KeyUp { .. }) => ActionKind::KeyUp,
        Some(Commands::Shortcut { .. }) => ActionKind::Shortcut,
        Some(Commands::Clear { .. }) => ActionKind::Clear,
        Some(Commands::Check { .. }) => ActionKind::Check,
        Some(Commands::Uncheck { .. }) => ActionKind::Uncheck,
        Some(Commands::Select { .. }) => ActionKind::Select,
        Some(Commands::Upload { .. }) => ActionKind::Upload,
        Some(Commands::Scroll { .. }) => ActionKind::Scroll,
        _ => ActionKind::Click,
    }
}

fn profile_manager_for_cli(cli: &Cli) -> ProfileManager {
    let chrome_path = crate::browser::chrome::resolve_chrome_path(cli.chrome_path.clone());
    ProfileManager::for_browser(chrome_path.as_deref())
}

fn dispatch_profiles(cli: &Cli, action: Option<&ProfileCommand>) -> BrowserResult<()> {
    let manager = profile_manager_for_cli(cli);
    match action {
        None | Some(ProfileCommand::List) => {
            let profiles = manager.list_profiles()?;
            if profiles.is_empty() {
                println!("no saved profiles");
            } else {
                for profile in profiles {
                    println!("{profile}");
                }
            }
        }
        Some(ProfileCommand::Create { name }) => {
            manager.create_profile(name)?;
            println!("created profile {name}");
        }
        Some(ProfileCommand::Delete { name }) => {
            manager.delete_profile(name)?;
            println!("deleted profile {name}");
        }
    }
    Ok(())
}

fn dispatch_certify(action: &CertifyCommand) -> BrowserResult<()> {
    match action {
        CertifyCommand::Run { .. } => {
            unreachable!("browser-backed reliability runs are handled after startup")
        }
        CertifyCommand::Plan { scenario, fixture } => {
            let scenario = ReliabilityScenario::from_value(read_json_input(Some(scenario))?)?;
            let fixture =
                ReliabilityFixtureManifest::from_json(&std::fs::read_to_string(fixture)?)?;
            let plan = scenario.execution_plan(&fixture)?;
            print_json(&serde_json::json!({
                "status": "valid",
                "plan": plan,
            }))?;
        }
        CertifyCommand::Release {
            version,
            scenarios,
            observations,
            replays,
        } => {
            let scenario_value = read_json_input(Some(scenarios))?;
            let scenarios: Vec<ReliabilityScenario> = if scenario_value.is_array() {
                serde_json::from_value(scenario_value)?
            } else {
                vec![serde_json::from_value(scenario_value)?]
            };
            let observations: Vec<ReliabilityScenarioObservation> =
                serde_json::from_value(read_json_input(Some(observations))?)?;
            let replays_validated = if let Some(replays) = replays {
                let replay_value = read_json_input(Some(replays))?;
                let replay_values: Vec<Value> = if replay_value.is_array() {
                    serde_json::from_value(replay_value)?
                } else {
                    vec![replay_value]
                };
                let mut replay_by_id = BTreeMap::new();
                for replay_value in replay_values {
                    let scenario_id = replay_value
                        .get("scenarioId")
                        .and_then(Value::as_str)
                        .ok_or("replay bundle is missing scenarioId")?
                        .to_string();
                    let scenario = scenarios
                        .iter()
                        .find(|scenario| scenario.id == scenario_id)
                        .ok_or_else(|| {
                            format!("replay references unknown scenario {scenario_id}")
                        })?;
                    let bundle = ReliabilityReplayBundle::from_value(replay_value, scenario)?;
                    if replay_by_id.insert(scenario_id.clone(), bundle).is_some() {
                        return Err(format!("duplicate replay for scenario {scenario_id}").into());
                    }
                }
                if replay_by_id.len() != scenarios.len() {
                    return Err("replay evidence must cover every scenario".into());
                }
                let observations_by_id: BTreeMap<_, _> = observations
                    .iter()
                    .map(|observation| (observation.scenario_id.as_str(), observation))
                    .collect();
                for (scenario_id, replay) in &replay_by_id {
                    let observation =
                        observations_by_id
                            .get(scenario_id.as_str())
                            .ok_or_else(|| {
                                format!("replay has no matching observation for {scenario_id}")
                            })?;
                    if serde_json::to_value(&replay.observation)?
                        != serde_json::to_value(observation)?
                    {
                        return Err(format!("replay observation differs for {scenario_id}").into());
                    }
                }
                true
            } else {
                false
            };
            let scorecard = build_reliability_scorecard(&scenarios, &observations)?;
            let certified = scorecard.certified;
            print_json(&serde_json::json!({
                "status": if certified { "certified" } else { "blocked" },
                "version": version,
                "tool": {"name": "glass", "version": env!("CARGO_PKG_VERSION")},
                "replaysValidated": replays_validated,
                "gate": &scorecard.gate,
                "scorecard": &scorecard,
            }))?;
            if !certified {
                return Err("reliability certification blocked".into());
            }
        }
        CertifyCommand::Replay { scenario, input } => {
            let scenario = ReliabilityScenario::from_value(read_json_input(Some(scenario))?)?;
            let bundle =
                ReliabilityReplayBundle::from_value(read_json_input(Some(input))?, &scenario)?;
            print_json(&serde_json::json!({
                "status": "valid",
                "scenarioId": &bundle.scenario_id,
                "replayHash": bundle.content_hash(&scenario)?,
            }))?;
        }
        CertifyCommand::ReplayDiff {
            scenario,
            before,
            after,
        } => {
            let scenario = ReliabilityScenario::from_value(read_json_input(Some(scenario))?)?;
            let before =
                ReliabilityReplayBundle::from_value(read_json_input(Some(before))?, &scenario)?;
            let after =
                ReliabilityReplayBundle::from_value(read_json_input(Some(after))?, &scenario)?;
            let comparison = before.compare(&after, &scenario)?;
            print_json(&serde_json::json!({
                "status": if comparison.equivalent { "equivalent" } else { "changed" },
                "comparison": comparison,
            }))?;
        }
    }
    Ok(())
}

async fn dispatch_certify_run(
    session: &BrowserSession,
    scenario_path: &std::path::Path,
    fixture_path: &std::path::Path,
    url: &str,
    workflow_root: &std::path::Path,
    inputs_path: Option<&std::path::Path>,
    output: Option<&std::path::Path>,
) -> BrowserResult<()> {
    let scenario = ReliabilityScenario::from_json(&std::fs::read_to_string(scenario_path)?)?;
    let fixture = ReliabilityFixtureManifest::from_json(&std::fs::read_to_string(fixture_path)?)?;
    let inputs: BTreeMap<String, Value> = match inputs_path {
        Some(path) => serde_json::from_str(&std::fs::read_to_string(path)?)
            .map_err(|error| format!("invalid workflow inputs: {error}"))?,
        None => BTreeMap::new(),
    };
    session.navigate(url).await?;
    let evidence = run_reliability_scenario(
        session,
        &scenario,
        &fixture,
        &ReliabilityRunOptions {
            workflow_root: workflow_root.to_path_buf(),
            inputs,
        },
    )
    .await?;
    let value = serde_json::json!({
        "observation": evidence.observation,
        "replay": evidence.replay,
    });
    if let Some(output) = output {
        tokio::fs::write(output, serde_json::to_vec_pretty(&value)?).await?;
    }
    print_json(&value)?;
    Ok(())
}
fn experience(operation: &str, result: Value) -> BrowserResult<()> {
    experience_with_refs(operation, result, Vec::new())
}

fn experience_with_refs(
    operation: &str,
    result: Value,
    resource_refs: Vec<ResourceReference>,
) -> BrowserResult<()> {
    let provenance = ExperienceProvenance {
        source: ProvenanceSource::Cli,
        authoritative: false,
        resource_ref: resource_refs.first().cloned(),
        revision: None,
        observed_at: Some(chrono::Utc::now().to_rfc3339()),
    };
    let mut envelope = ExperienceResult::new(operation, "ok", result).with_provenance(provenance);
    for resource_ref in resource_refs {
        envelope = envelope.with_resource_ref(resource_ref);
    }
    envelope.validate()?;
    print_json(&envelope)
}
fn dispatch_workspace(action: &WorkspaceCommand) -> BrowserResult<()> {
    let store = WorkspaceStore::open_default()?;
    match action {
        WorkspaceCommand::List => {
            let ids = store.list()?;
            let refs = ids
                .iter()
                .cloned()
                .map(ResourceReference::workspace)
                .collect();
            experience_with_refs("workspace.list", serde_json::to_value(ids)?, refs)
        }
        WorkspaceCommand::Inspect { id } => {
            let id = WorkspaceId::new(id)?;
            let workspace = store.open(&id)?;
            experience_with_refs(
                "workspace.inspect",
                serde_json::to_value(&workspace)?,
                vec![workspace.resource_reference()],
            )
        }
        WorkspaceCommand::Suspend { id } => {
            let id = WorkspaceId::new(id)?;
            let workspace = store.suspend(&id)?;
            experience_with_refs(
                "workspace.suspend",
                serde_json::to_value(&workspace)?,
                vec![workspace.resource_reference()],
            )
        }
        WorkspaceCommand::Resume { id } => {
            let id = WorkspaceId::new(id)?;
            let workspace = store.resume(&id)?;
            experience_with_refs(
                "workspace.resume",
                serde_json::to_value(&workspace)?,
                vec![workspace.resource_reference()],
            )
        }
        WorkspaceCommand::Delete { id } => {
            let id = WorkspaceId::new(id)?;
            let reference = ResourceReference::workspace(id.clone());
            store.delete(&id)?;
            experience_with_refs(
                "workspace.delete",
                serde_json::json!({"id": id, "deleted": true}),
                vec![reference],
            )
        }
    }
}

fn dispatch_memory(
    action: &MemoryCommand,
    explicit_path: Option<&Path>,
    profile: &str,
) -> BrowserResult<()> {
    ProfileManager::validate_name(profile)?;
    let path = explicit_path
        .map(Path::to_path_buf)
        .unwrap_or_else(|| default_knowledge_store_path(profile));
    let mut store = KnowledgeStore::open(path)?;
    match action {
        MemoryCommand::Status => experience("memory.status", serde_json::to_value(store.stats()?)?),
        MemoryCommand::Inspect { record_id } => {
            let record = store
                .get(record_id)
                .ok_or_else(|| format!("memory record not found: {record_id}"))?;
            experience("memory.inspect", serde_json::to_value(record)?)
        }
        MemoryCommand::Explain { record_id } => {
            let record = store
                .get(record_id)
                .ok_or_else(|| format!("memory record not found: {record_id}"))?;
            experience(
                "memory.explain",
                serde_json::json!({
                    "recordId": record.record_id,
                    "scope": record.scope,
                    "source": record.source,
                    "confidence": record.confidence,
                    "history": record.history,
                    "advisoryOnly": true,
                    "contentHash": record.content_hash()?,
                }),
            )
        }
        MemoryCommand::Forget { record_id } => experience(
            "memory.forget",
            serde_json::to_value(store.remove(record_id)?)?,
        ),
        MemoryCommand::Export { output } => {
            let canonical = store.snapshot().to_canonical_json()?;
            if let Some(output) = output {
                std::fs::write(output, canonical)?;
                experience(
                    "memory.export",
                    serde_json::json!({"output": output, "records": store.records().len()}),
                )
            } else {
                experience("memory.export", serde_json::from_str(&canonical)?)
            }
        }
        MemoryCommand::Prune => {
            let ids = store
                .records()
                .iter()
                .filter(|record| {
                    matches!(
                        record.confidence,
                        KnowledgeConfidence::Stale
                            | KnowledgeConfidence::Contradicted
                            | KnowledgeConfidence::Quarantined
                    )
                })
                .map(|record| record.record_id.clone())
                .collect::<Vec<_>>();
            let mut removed = Vec::new();
            for id in ids {
                let change = store.remove(&id)?;
                if change.removed {
                    removed.push(id);
                }
            }
            experience(
                "memory.prune",
                serde_json::json!({"removedRecordIds": removed}),
            )
        }
        MemoryCommand::Reindex => {
            store.refresh()?;
            experience("memory.reindex", serde_json::to_value(store.stats()?)?)
        }
    }
}

fn dispatch_surfaces(action: &SurfaceCommand) -> BrowserResult<()> {
    let input = match action {
        SurfaceCommand::Inspect { input } | SurfaceCommand::Coverage { input } => {
            read_json_input(Some(input))?
        }
    };
    let set: SurfaceSet = serde_json::from_value(input)?;
    set.validate()?;
    match action {
        SurfaceCommand::Inspect { .. } => {
            experience("surfaces.inspect", serde_json::to_value(set)?)
        }
        SurfaceCommand::Coverage { .. } => experience(
            "surfaces.coverage",
            serde_json::json!({
                "surfaceCount": set.surfaces.len(),
                "surfaces": set.surfaces.iter().map(|surface| serde_json::json!({
                    "surfaceId": surface.surface_id,
                    "kind": surface.kind,
                    "understanding": surface.understanding,
                    "coverage": surface.coverage,
                    "capabilities": surface.capabilities,
                    "evidenceCount": surface.evidence.len(),
                    "provenance": surface.evidence.iter().map(|evidence| &evidence.provenance).collect::<Vec<_>>(),
                })).collect::<Vec<_>>()
            }),
        ),
    }
}

async fn dispatch_backend(action: &BackendCommand) -> BrowserResult<()> {
    let input = match action {
        BackendCommand::Status { input }
        | BackendCommand::Capabilities { input }
        | BackendCommand::Test { input } => input,
    };
    let profile: BackendProfile = serde_json::from_value(read_json_input(Some(input))?)?;
    profile.validate()?;
    match action {
        BackendCommand::Status { .. } => experience(
            "backend.status",
            serde_json::json!({
                "identity": profile.identity.clone(),
                "certification": profile.identity.certification.clone(),
                "capabilities": profile.capabilities.clone(),
                "declaredCapabilities": profile.capabilities.len(),
            }),
        ),
        BackendCommand::Capabilities { .. } => experience(
            "backend.capabilities",
            serde_json::to_value(profile.capabilities)?,
        ),
        BackendCommand::Test { .. } => {
            if profile.identity.backend_id == "semantic-proof" {
                let backend = BackendFactory::proof()?;
                if backend.profile() != &profile {
                    return Err(
                        "semantic-proof input profile does not match the built-in proof profile"
                            .into(),
                    );
                }
                let dispatcher = BrowserBackendDispatcher::new(&backend);
                dispatcher.initialize().await?;
                let navigation = dispatcher
                    .navigate(NavigationRequest {
                        url: "proof://cli-test".into(),
                    })
                    .await?;
                let contexts = dispatcher
                    .contexts(ContextRequest {
                        include_background: false,
                    })
                    .await?;
                let context_id = contexts
                    .first()
                    .map(|context| context.context_id.clone())
                    .ok_or("proof backend returned no context")?;
                let evidence = dispatcher
                    .evidence(EvidenceRequest {
                        context_id: context_id.clone(),
                        level: EvidenceLevel::Deep,
                    })
                    .await?;
                let action = dispatcher
                    .action(ActionRequest {
                        context_id: context_id.clone(),
                        action: SemanticAction::Click {
                            target: "proof.button".into(),
                        },
                    })
                    .await?;
                let effects = dispatcher
                    .effects(EffectsRequest {
                        context_id,
                        since_revision: navigation.revision,
                    })
                    .await?;
                experience(
                    "backend.test",
                    serde_json::json!({
                        "valid": true,
                        "backend": {
                            "identity": profile.identity.clone(),
                            "certification": profile.identity.certification.clone(),
                            "capabilities": profile.capabilities.clone(),
                        },
                        "proof": {
                            "navigation": navigation,
                            "evidence": evidence,
                            "action": action,
                            "effects": effects,
                        },
                    }),
                )
            } else {
                let gates = BrowserCapability::ALL
                    .iter()
                    .map(|capability| {
                        let descriptor = profile.capability(*capability);
                        serde_json::json!({
                            "capability": capability,
                            "declared": profile.capabilities.contains_key(capability),
                            "level": descriptor.level,
                            "portability": descriptor.portability,
                        })
                    })
                    .collect::<Vec<_>>();
                experience(
                    "backend.test",
                    serde_json::json!({"valid": true, "gates": gates}),
                )
            }
        }
    }
}

fn dispatch_replay(action: &ReplayCommand) -> BrowserResult<()> {
    let scenario_path = match action {
        ReplayCommand::Inspect { scenario, .. }
        | ReplayCommand::Attach { scenario, .. }
        | ReplayCommand::Diff { scenario, .. } => scenario,
    };
    let scenario: ReliabilityScenario =
        ReliabilityScenario::from_value(read_json_input(Some(scenario_path))?)?;
    match action {
        ReplayCommand::Inspect { input, .. } | ReplayCommand::Attach { input, .. } => {
            let replay =
                ReliabilityReplayBundle::from_value(read_json_input(Some(input))?, &scenario)?;
            let operation = if matches!(action, ReplayCommand::Attach { .. }) {
                "replay.attach"
            } else {
                "replay.inspect"
            };
            experience(
                operation,
                serde_json::json!({
                    "scenarioId": replay.scenario_id,
                    "fixtureId": replay.fixture_id,
                    "eventCount": replay.events.len(),
                    "contentHash": replay.content_hash(&scenario)?,
                    "attached": matches!(action, ReplayCommand::Attach { .. }),
                }),
            )
        }
        ReplayCommand::Diff { before, after, .. } => {
            let left =
                ReliabilityReplayBundle::from_value(read_json_input(Some(before))?, &scenario)?;
            let right =
                ReliabilityReplayBundle::from_value(read_json_input(Some(after))?, &scenario)?;
            experience(
                "replay.diff",
                serde_json::to_value(left.compare(&right, &scenario)?)?,
            )
        }
    }
}

fn dispatch_knowledge(
    action: &KnowledgeCommand,
    explicit_path: Option<&std::path::Path>,
    profile: &str,
) -> BrowserResult<()> {
    ProfileManager::validate_name(profile)?;
    let path = explicit_path
        .map(std::path::Path::to_path_buf)
        .unwrap_or_else(|| default_knowledge_store_path(profile));
    let mut store = KnowledgeStore::open(path)?;
    match action {
        KnowledgeCommand::List => print_json(store.snapshot())?,
        KnowledgeCommand::Show { record_id } => {
            let record = store
                .get(record_id)
                .ok_or_else(|| format!("knowledge record not found: {record_id}"))?;
            print_json(record)?;
        }
        KnowledgeCommand::Explain { record_id } => {
            let record = store
                .get(record_id)
                .ok_or_else(|| format!("knowledge record not found: {record_id}"))?;
            print_json(&serde_json::json!({
                "recordId": &record.record_id,
                "kind": record.kind,
                "confidence": record.confidence,
                "scope": &record.scope,
                "source": &record.source,
                "invalidation": &record.invalidation,
                "history": &record.history,
                "contentHash": record.content_hash()?,
                "assessment": "requires a fresh observation; stored knowledge is never an authorization",
            }))?;
        }
        KnowledgeCommand::Stats => print_json(&store.stats()?)?,
        KnowledgeCommand::Export { output } => {
            let canonical = store.snapshot().to_canonical_json()?;
            if let Some(output) = output {
                std::fs::write(output, canonical)?;
                println!("exported knowledge to {}", output.display());
            } else {
                println!("{canonical}");
            }
        }
        KnowledgeCommand::Import { input } => {
            let snapshot = serde_json::from_value(read_json_input(Some(input))?)
                .map_err(|error| format!("invalid knowledge snapshot: {error}"))?;
            store.replace_snapshot(snapshot)?;
            print_json(&store.stats()?)?;
        }
        KnowledgeCommand::Invalidate {
            record_id,
            state,
            reason,
            observed_at,
        } => {
            let next = match state {
                KnowledgeInvalidationState::Stale => KnowledgeConfidence::Stale,
                KnowledgeInvalidationState::Contradicted => KnowledgeConfidence::Contradicted,
                KnowledgeInvalidationState::Quarantined => KnowledgeConfidence::Quarantined,
            };
            let change = store.transition(
                record_id,
                next,
                reason
                    .clone()
                    .unwrap_or_else(|| "caller invalidated record".into()),
                observed_at
                    .clone()
                    .unwrap_or_else(|| chrono::Utc::now().to_rfc3339()),
                false,
            )?;
            print_json(&change)?;
        }
        KnowledgeCommand::Purge { origin } => print_json(&store.purge_origin(origin)?)?,
    }
    Ok(())
}

fn dispatch_snapshot(action: &SnapshotCommand, profile: &str) -> BrowserResult<()> {
    ProfileManager::validate_name(profile)?;
    let store = SessionSnapshotStore::new(crate::browser::session::default_session_snapshot_path(
        profile,
    ));
    match action {
        SnapshotCommand::Create => unreachable!("snapshot creation requires a browser session"),
        SnapshotCommand::List => print_json(&store.list()?)?,
        SnapshotCommand::Inspect { snapshot_id } => print_json(&store.load(snapshot_id)?)?,
        SnapshotCommand::Diff { from, to } => print_json(&store.diff(from, to)?)?,
        SnapshotCommand::Purge => print_json(&serde_json::json!({"removed": store.purge()?}))?,
    }
    Ok(())
}

fn dispatch_task(action: &TaskCommand) -> BrowserResult<()> {
    match action {
        TaskCommand::Validate { input } => {
            let request = read_task_request(input, TASK_VALIDATE_OPERATION)?;
            let result = crate::protocol::validate_task_result(&request)?;
            print_json(&result)?;
        }
        TaskCommand::Compile {
            input,
            ir,
            output,
            explain,
        } => {
            let request = read_task_compile_request(input, ir)?;
            let task = request.decode_task_compile()?.task;
            let result = crate::protocol::compile_task_result(&request)?;
            if let Some(output) = output {
                std::fs::write(output, serde_json::to_vec_pretty(&result.plan)?)?;
                println!("compiled task to {}", output.display());
            } else {
                print_json(&result.plan)?;
            }
            if *explain {
                eprintln!("{}", explain_task(&task, &result.plan)?);
            }
        }
        TaskCommand::Execute { .. } => {
            unreachable!("browser task commands are handled after session startup")
        }
    }
    Ok(())
}

fn read_task_request(path: &Path, operation: &str) -> BrowserResult<GlassRequest> {
    let source = std::fs::read_to_string(path)?;
    let task: Value = serde_json::from_str(&source)?;
    let request = GlassRequest {
        protocol_version: GLASS_PROTOCOL_VERSION,
        request_id: format!("cli-{}", operation.replace('.', "-")),
        correlation_id: None,
        session_id: None,
        mutation_lease: None,
        operation: operation.into(),
        payload: serde_json::json!({"task": task}),
        deadline_ms: None,
    };
    request.validate()?;
    Ok(request)
}

fn read_task_compile_request(task_path: &Path, ir_path: &Path) -> BrowserResult<GlassRequest> {
    let task: Value = serde_json::from_str(&std::fs::read_to_string(task_path)?)?;
    let ir: Value = serde_json::from_str(&std::fs::read_to_string(ir_path)?)?;
    let request = GlassRequest {
        protocol_version: GLASS_PROTOCOL_VERSION,
        request_id: "cli-task-compile".into(),
        correlation_id: None,
        session_id: None,
        mutation_lease: None,
        operation: TASK_COMPILE_OPERATION.into(),
        payload: serde_json::json!({"task": task, "ir": ir}),
        deadline_ms: None,
    };
    request.validate()?;
    Ok(request)
}

fn read_task_execution_request(
    path: &Path,
    expected_revision: u64,
    confirmed: bool,
) -> BrowserResult<GlassRequest> {
    let source = std::fs::read_to_string(path)?;
    let task: Value = serde_json::from_str(&source)?;
    let request = GlassRequest {
        protocol_version: GLASS_PROTOCOL_VERSION,
        request_id: "cli-task-execute".into(),
        correlation_id: None,
        session_id: None,
        mutation_lease: None,
        operation: TASK_EXECUTE_OPERATION.into(),
        payload: serde_json::json!({
            "task": task,
            "expectedRevision": expected_revision,
            "confirmed": confirmed,
        }),
        deadline_ms: None,
    };
    request.validate()?;
    Ok(request)
}

fn dispatch_ir(action: &IrCommand) -> BrowserResult<()> {
    match action {
        IrCommand::Validate { input } => {
            let request = read_web_ir_request(input, WEB_IR_VALIDATE_OPERATION)?;
            let result = crate::protocol::web_ir_validate_result(&request)?;
            print_json(&result)?;
        }
        IrCommand::Inspect { input } => {
            let request = read_web_ir_request(input, WEB_IR_INSPECT_OPERATION)?;
            let result = crate::protocol::web_ir_inspect_result(&request)?;
            print_json(&result)?;
        }
        IrCommand::Diff {
            before,
            after,
            summary,
        } => {
            let before = read_web_ir(before)?;
            let after = read_web_ir(after)?;
            let diff = before.diff(&after)?;
            if *summary {
                print_json(&crate::protocol::WebIrDiffResult::from_diff(&diff))?;
            } else {
                print_json(&diff)?;
            }
        }
        IrCommand::Continuity {
            before,
            after,
            entity_id,
        } => {
            let request = read_web_ir_continuity_request(before, after, entity_id)?;
            let result = crate::protocol::web_ir_continuity_result(&request)?;
            print_json(&result)?;
        }
        IrCommand::Canonical { input } => {
            let draft = read_web_ir(input)?;
            println!("{}", draft.to_canonical_json()?);
        }
    }
    Ok(())
}

fn read_web_ir_request(path: &Path, operation: &str) -> BrowserResult<GlassRequest> {
    let source = std::fs::read_to_string(path)?;
    let draft: Value = serde_json::from_str(&source)?;
    let request = GlassRequest {
        protocol_version: GLASS_PROTOCOL_VERSION,
        request_id: format!("cli-{}", operation.replace('.', "-")),
        correlation_id: None,
        session_id: None,
        mutation_lease: None,
        operation: operation.into(),
        payload: serde_json::json!({"ir": draft}),
        deadline_ms: None,
    };
    request.validate()?;
    Ok(request)
}

fn read_web_ir_continuity_request(
    before: &Path,
    after: &Path,
    entity_id: &str,
) -> BrowserResult<GlassRequest> {
    let before_source = std::fs::read_to_string(before)?;
    let after_source = std::fs::read_to_string(after)?;
    let request = GlassRequest {
        protocol_version: GLASS_PROTOCOL_VERSION,
        request_id: "cli-web-ir-continuity".into(),
        correlation_id: None,
        session_id: None,
        mutation_lease: None,
        operation: WEB_IR_CONTINUITY_OPERATION.into(),
        payload: serde_json::json!({
            "before": serde_json::from_str::<Value>(&before_source)?,
            "after": serde_json::from_str::<Value>(&after_source)?,
            "entityId": entity_id,
        }),
        deadline_ms: None,
    };
    request.validate()?;
    Ok(request)
}

fn read_web_ir(path: &Path) -> BrowserResult<crate::web_ir::GlassWebIrV1> {
    let source = std::fs::read_to_string(path)?;
    let ir: crate::web_ir::GlassWebIrV1 = serde_json::from_str(&source)?;
    ir.validate()?;
    Ok(ir)
}

fn explain_task(
    task: &crate::task_protocol::GlassTask,
    plan: &crate::task_compiler::TaskExecutionPlan,
) -> BrowserResult<String> {
    let task_name = serde_json::to_string(&task.task)?
        .trim_matches('"')
        .to_owned();
    let risk = serde_json::to_string(&task.risk)?
        .trim_matches('"')
        .to_owned();
    let steps = plan
        .steps
        .iter()
        .map(|step| {
            let operation = serde_json::to_string(&step.operation)
                .expect("task plan operations are always serializable")
                .trim_matches('"')
                .to_owned();
            let inputs = if step.input_names.is_empty() {
                String::new()
            } else {
                format!(
                    " inputNames={}",
                    serde_json::to_string(&step.input_names)
                        .expect("task input names are always serializable")
                )
            };
            format!(
                "  {}. {}{} confirmationRequired={}",
                step.ordinal, operation, inputs, step.requires_confirmation
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    Ok(format!(
        "task: {task_name}\nscope: {}\nrisk: {risk}\nconfirmationRequired: {}\nsteps:\n{steps}\npostconditions: {}\n",
        serde_json::to_string(&task.scope)?,
        plan.confirmation_required,
        serde_json::to_string(&plan.postconditions)?
    ))
}

fn dispatch_workflow_authoring(action: &WorkflowAuthoringCommand) -> BrowserResult<()> {
    match action {
        WorkflowAuthoringCommand::Templates { name, output } => {
            let names = [
                "account-search",
                "checkout-submit",
                "profile-update",
                "report-download",
                "support-ticket",
            ];
            let Some(name) = name else {
                print_json(&names)?;
                return Ok(());
            };
            let source = workflow_template(name)
                .ok_or_else(|| format!("unknown workflow template: {name}"))?;
            let document = compile_workflow(source, WorkflowAuthoringFormat::Yaml)?;
            if document
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.severity == WorkflowDiagnosticSeverity::Error)
            {
                return Err(format!("workflow template {name} failed compilation").into());
            }
            if let Some(output) = output {
                std::fs::write(output, source)?;
            } else {
                print!("{source}");
            }
        }
        WorkflowAuthoringCommand::Init { name, output } => {
            let template = match name.as_str() {
                "search" => "account-search",
                "form-submit" => "checkout-submit",
                "paginated-extraction" => "report-download",
                "authenticated-session" => "profile-update",
                "dialog-and-download" => "support-ticket",
                _ => return Err(format!("unknown workflow starter: {name}").into()),
            };
            let source = workflow_template(template)
                .ok_or_else(|| format!("missing workflow starter: {template}"))?;
            let document = compile_workflow(source, WorkflowAuthoringFormat::Yaml)?;
            if document
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.severity == WorkflowDiagnosticSeverity::Error)
            {
                return Err(format!("workflow starter {name} failed compilation").into());
            }
            if let Some(output) = output {
                std::fs::write(output, source)?;
            } else {
                print!("{source}");
            }
        }
        WorkflowAuthoringCommand::Compile { input, output } => {
            let source = std::fs::read_to_string(input)?;
            let document = compile_workflow(&source, authoring_format(input))?;
            if let Some(output) = output {
                std::fs::write(output, &document.canonical_json)?;
                println!("compiled workflow to {}", output.display());
            } else {
                println!("{}", document.canonical_json);
            }
            if document
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.severity == WorkflowDiagnosticSeverity::Error)
            {
                return Err("workflow compilation produced error diagnostics".into());
            }
        }
        WorkflowAuthoringCommand::Format { input, output } => {
            let source = std::fs::read_to_string(input)?;
            let document = compile_workflow(&source, authoring_format(input))?;
            let formatted = format_workflow_yaml(&document.definition)?;
            if let Some(output) = output {
                std::fs::write(output, formatted)?;
                println!("formatted workflow to {}", output.display());
            } else {
                print!("{formatted}");
            }
        }
        WorkflowAuthoringCommand::Preview { input } => {
            let source = std::fs::read_to_string(input)?;
            let document = compile_workflow(&source, authoring_format(input))?;
            let preview = preview_workflow(&document.definition)?;
            print_json(&serde_json::json!({
                "preview": preview,
                "diagnostics": document.diagnostics,
            }))?;
        }
        WorkflowAuthoringCommand::Diff { before, after } => {
            let before_source = std::fs::read_to_string(before)?;
            let after_source = std::fs::read_to_string(after)?;
            let before_document = compile_workflow(&before_source, authoring_format(before))?;
            let after_document = compile_workflow(&after_source, authoring_format(after))?;
            let diff = diff_workflows(&before_document.definition, &after_document.definition)?;
            print_json(&serde_json::json!({
                "diff": diff,
                "beforeDiagnostics": before_document.diagnostics,
                "afterDiagnostics": after_document.diagnostics,
            }))?;
        }
        WorkflowAuthoringCommand::Record { input, output } => {
            let value = read_json_input(input.as_ref())?;
            let session: WorkflowRecordingSession = serde_json::from_value(value)?;
            let draft = record_semantic_events(session)?;
            let serialized = serde_json::to_string_pretty(&draft)?;
            if let Some(output) = output {
                std::fs::write(output, serialized)?;
                println!("recorded workflow draft to {}", output.display());
            } else {
                println!("{serialized}");
            }
        }
        WorkflowAuthoringCommand::Validate { input } => {
            let source = std::fs::read_to_string(input)?;
            let document = compile_workflow(&source, authoring_format(input))?;
            print_json(&document)?;
        }
        WorkflowAuthoringCommand::Lint {
            input,
            warnings_as_errors,
        } => {
            let source = std::fs::read_to_string(input)?;
            let document = compile_workflow(&source, authoring_format(input))?;
            let failed = document.diagnostics.iter().any(|diagnostic| {
                diagnostic.severity == WorkflowDiagnosticSeverity::Error
                    || (*warnings_as_errors
                        && diagnostic.severity == WorkflowDiagnosticSeverity::Warning)
            });
            print_json(&document.diagnostics)?;
            if failed {
                return Err("workflow lint failed".into());
            }
        }
    }
    Ok(())
}

fn authoring_format(path: &std::path::Path) -> WorkflowAuthoringFormat {
    if path
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case("json"))
    {
        WorkflowAuthoringFormat::Json
    } else {
        WorkflowAuthoringFormat::Yaml
    }
}

fn workflow_template(name: &str) -> Option<&'static str> {
    Some(match name {
        "account-search" => include_str!("templates/account-search.yaml"),
        "checkout-submit" => include_str!("templates/checkout-submit.yaml"),
        "profile-update" => include_str!("templates/profile-update.yaml"),
        "report-download" => include_str!("templates/report-download.yaml"),
        "support-ticket" => include_str!("templates/support-ticket.yaml"),
        _ => return None,
    })
}
async fn run_command(
    session: &BrowserSession,
    command: &Commands,
    response_mode: ResponseMode,
) -> BrowserResult<()> {
    match command {
        Commands::Certify {
            action:
                CertifyCommand::Run {
                    scenario,
                    fixture,
                    url,
                    workflow_root,
                    inputs,
                    output,
                },
        } => {
            dispatch_certify_run(
                session,
                scenario,
                fixture,
                url,
                workflow_root,
                inputs.as_deref(),
                output.as_deref(),
            )
            .await?;
        }
        Commands::Task {
            action:
                TaskCommand::Execute {
                    input,
                    expected_revision,
                    confirm,
                },
        } => {
            let request = read_task_execution_request(input, *expected_revision, *confirm)?;
            let payload = request.decode_task_execute()?;
            let result = session
                .execute_task(&payload.task, payload.expected_revision, payload.confirmed)
                .await?;
            print_json_mode(&result, response_mode)?;
        }
        Commands::Snapshot {
            action: SnapshotCommand::Create,
        } => {
            let observation = session
                .semantic_observe(SemanticObservationLevel::Structured)
                .await?;
            let snapshot = crate::browser::session::SessionSnapshot::from_observation(
                session.profile.clone(),
                observation,
            );
            let store = SessionSnapshotStore::new(
                crate::browser::session::default_session_snapshot_path(&session.profile),
            );
            store.save(&snapshot)?;
            print_json(&snapshot)?;
        }
        Commands::Update { .. }
        | Commands::Capabilities
        | Commands::Daemon { .. }
        | Commands::Doctor { .. }
        | Commands::McpConfig { .. }
        | Commands::Result { .. }
        | Commands::Certify { .. }
        | Commands::Knowledge { .. }
        | Commands::Snapshot { .. }
        | Commands::Workspace { .. }
        | Commands::Project { .. }
        | Commands::Agent { .. }
        | Commands::Harness { .. }
        | Commands::Memory { .. }
        | Commands::Surfaces { .. }
        | Commands::Backend { .. }
        | Commands::Replay { .. } => {
            unreachable!("offline commands are handled before browser startup")
        }
        Commands::Task {
            action: TaskCommand::Validate { .. } | TaskCommand::Compile { .. },
        } => unreachable!("offline task commands are handled before browser startup"),
        Commands::Navigate {
            url,
            timeout_ms,
            expected_revision,
        } => {
            if let Some(expected_revision) = expected_revision {
                print_json(
                    &session
                        .navigate_with_revision(
                            url,
                            Duration::from_millis(*timeout_ms),
                            *expected_revision,
                        )
                        .await?,
                )?;
            } else if *timeout_ms == 20_000 {
                // The typed adapter contract currently carries URL only;
                // retain the direct deadline path for custom timeouts.
                let backend = CdpSessionBackend::new(session)?;
                let dispatcher = BrowserBackendDispatcher::new(&backend);
                dispatcher
                    .navigate(NavigationRequest { url: url.clone() })
                    .await?;
                print_json(&session.page_info().await?)?;
            } else {
                let page = session
                    .navigate_with_deadline(url, Duration::from_millis(*timeout_ms))
                    .await?;
                print_json(&page)?;
            }
        }
        Commands::Click {
            target,
            expected_revision,
        } => {
            if let Some(expected_revision) = expected_revision {
                print_json(
                    &session
                        .click_with_revision(target, *expected_revision)
                        .await?,
                )?;
            } else {
                print_json(&session.click(target).await?)?;
            }
        }
        Commands::Preflight { target, action } => {
            print_json(&session.preflight_with_action(target, *action).await)?;
        }
        Commands::ClickAt { x, y } => {
            print_json(&session.click_at(*x, *y).await?)?;
        }
        Commands::ClickExpectPopup {
            target,
            expected_revision,
        } => {
            print_json(
                &session
                    .click_expect_popup_with_revision(target, *expected_revision)
                    .await?,
            )?;
        }
        Commands::DoubleClick {
            target,
            expected_revision,
        } => {
            print_json(
                &session
                    .double_click_with_revision(target, *expected_revision)
                    .await?,
            )?;
        }
        Commands::Hover { target } => print_json(&session.hover(target).await?)?,
        Commands::Drag {
            source,
            destination,
            expected_revision,
        } => {
            print_json(
                &session
                    .drag_with_revision(source, destination, *expected_revision)
                    .await?,
            )?;
        }
        Commands::Type {
            text,
            target,
            expected_revision,
        } => {
            print_json(
                &session
                    .type_text_with_expected_revision(text, target.as_deref(), *expected_revision)
                    .await?,
            )?;
        }
        Commands::Key {
            key,
            expected_revision,
        } => print_json(
            &session
                .key_press_with_revision(key, *expected_revision)
                .await?,
        )?,
        Commands::KeyDown {
            key,
            expected_revision,
        } => print_json(
            &session
                .key_down_with_revision(key, *expected_revision)
                .await?,
        )?,
        Commands::KeyUp {
            key,
            expected_revision,
        } => print_json(
            &session
                .key_up_with_revision(key, *expected_revision)
                .await?,
        )?,
        Commands::Shortcut {
            shortcut,
            expected_revision,
        } => print_json(
            &session
                .shortcut_with_revision(shortcut, *expected_revision)
                .await?,
        )?,
        Commands::Clear {
            target,
            expected_revision,
        } => print_json(
            &session
                .clear_with_revision(target, *expected_revision)
                .await?,
        )?,
        Commands::Check {
            target,
            expected_revision,
        } => print_json(
            &session
                .check_with_revision(target, *expected_revision)
                .await?,
        )?,
        Commands::Uncheck {
            target,
            expected_revision,
        } => print_json(
            &session
                .uncheck_with_revision(target, *expected_revision)
                .await?,
        )?,
        Commands::Select {
            target,
            value,
            expected_revision,
        } => {
            print_json(
                &session
                    .select_option_with_revision(target, value, *expected_revision)
                    .await?,
            )?;
        }
        Commands::Upload {
            target,
            files,
            expected_revision,
        } => {
            print_json(
                &session
                    .upload_files_with_revision(target, files, *expected_revision)
                    .await?,
            )?;
        }
        Commands::Screenshot {
            output,
            format,
            quality,
            scale,
            full_page,
            clip,
            target,
        } => {
            let output = session
                .policy()
                .require_output_path(std::path::Path::new(output))?;
            let capture = session
                .capture_visual(&VisualCaptureOptions {
                    format: *format,
                    quality: *quality,
                    scale: *scale,
                    clip: *clip,
                    full_page: *full_page,
                    target: target.clone(),
                })
                .await?;
            let mut source = base64::read::DecoderReader::new(
                capture.data.as_bytes(),
                &base64::engine::general_purpose::STANDARD,
            );
            let mut file = std::fs::File::create(&output)?;
            std::io::copy(&mut source, &mut file)?;
            println!("wrote {}", output.display());
            print_json(&capture.metadata)?;
        }
        Commands::Text => println!("{}", session.text().await?),
        Commands::Dom => print_json(&session.deep_dom().await?)?,
        Commands::Observe {
            deep_dom,
            screenshot,
            form_values,
            semantic_level,
            region,
        } => {
            if let Some(level_name) = semantic_level {
                if *deep_dom || *screenshot || *form_values {
                    return Err(
                        "semantic observation cannot be combined with deep DOM, screenshot, or form values"
                            .into(),
                    );
                }
                let level = parse_semantic_level(level_name)?;
                if let Some(region_id) = region {
                    let page = session.semantic_observe(level).await?;
                    print_json(
                        &session
                            .semantic_expand_region(region_id, page.revision, level)
                            .await?,
                    )?;
                } else {
                    print_json(&session.semantic_observe(level).await?)?;
                }
                return Ok(());
            }
            let context = match (*deep_dom, *screenshot, *form_values) {
                (false, false, false) => session.observe().await?,
                (true, false, false) => session.observe_with_dom().await?,
                (false, true, false) => session.observe_with_screenshot().await?,
                (true, true, false) => session.observe_with_dom_and_screenshot().await?,
                (false, false, true) => session.observe_with_form_values().await?,
                _ => return Err("form values can only be combined with compact observe".into()),
            };
            print_json_mode(&context, response_mode)?;
        }
        Commands::InspectPage => print_json_mode(&session.inspect_page().await?, response_mode)?,
        Commands::FindTarget { input } => {
            let request = SemanticIntentRequest::from_json(&serde_json::to_string(
                &read_json_input(Some(input))?,
            )?)?;
            print_json_mode(&session.find_target(&request).await?, response_mode)?;
        }
        Commands::ActAndVerify {
            input,
            predicate,
            timeout_ms,
        } => {
            let request = SemanticIntentExecutionRequest::from_json(&serde_json::to_string(
                &read_json_input(Some(input))?,
            )?)?;
            let predicate = predicate
                .as_deref()
                .map(serde_json::from_str::<VerificationPredicate>)
                .transpose()?;
            print_json_mode(
                &session
                    .act_and_verify(&request, predicate, Duration::from_millis(*timeout_ms))
                    .await?,
                response_mode,
            )?;
        }
        Commands::ExtractStructured { input } => {
            let request: StructuredExtractionRequest =
                serde_json::from_value(read_json_input(Some(input))?)?;
            print_json_mode(&session.extract_structured(&request).await?, response_mode)?;
        }
        Commands::RecoverRun { execution_id } => {
            print_json_mode(&session.recover_run(execution_id)?, response_mode)?;
        }
        Commands::Scroll {
            dx,
            dy,
            expected_revision,
        } => {
            print_json(
                &session
                    .scroll_with_revision(*dx, *dy, *expected_revision)
                    .await?,
            )?;
        }
        Commands::Wait {
            condition,
            timeout_ms,
        } => {
            print_json(
                &session
                    .wait(
                        WaitCondition::parse(condition)?,
                        Duration::from_millis(*timeout_ms),
                    )
                    .await?,
            )?;
        }
        Commands::Diagnostics { duration_ms } => print_json(
            &session
                .diagnostics(Duration::from_millis(*duration_ms))
                .await?,
        )?,
        Commands::AcceptDialog => {
            session.accept_dialog().await?;
            print_json(&serde_json::json!({"dialog": "accepted"}))?;
        }
        Commands::DismissDialog => {
            session.dismiss_dialog().await?;
            print_json(&serde_json::json!({"dialog": "dismissed"}))?;
        }
        Commands::DismissConsent => print_json(&session.dismiss_consent().await?)?,
        Commands::Download {
            destination,
            timeout_ms,
        } => print_json(
            &session
                .wait_for_download(destination, Duration::from_millis(*timeout_ms))
                .await?,
        )?,
        Commands::Targets => print_json(&session.list_targets().await?)?,
        Commands::ArchiveTargets { output } => {
            let targets = session.list_targets().await?;
            let target_count = targets.len();
            let archive = target_archive(&targets)?;
            let bytes = serde_json::to_vec_pretty(&archive)?;
            if let Some(output) = output {
                let path = session.policy().require_output_path(output)?;
                if path.is_dir() {
                    return Err("target archive output must name a file".into());
                }
                tokio::fs::write(&path, &bytes).await?;
                print_json(&serde_json::json!({
                    "schemaVersion": "glass.target-archive.v1",
                    "targetCount": target_count,
                    "output": path,
                }))?;
            } else {
                print_json(&archive)?;
            }
        }
        Commands::NewTarget { url } => print_json(&session.create_target(url).await?)?,
        Commands::SelectTarget { id } => print_json(&session.select_target(id).await?)?,
        Commands::CloseTarget { id } => {
            session.close_target(id).await?;
            print_json(&serde_json::json!({"closed": id}))?;
        }
        Commands::Frames => print_json(&session.list_frames().await?)?,
        Commands::SelectFrame { id } => print_json(&session.select_frame(id).await?)?,
        Commands::Evaluate { expression } => {
            print_json(&session.evaluate(expression).await?)?;
        }
        Commands::Cookies => print_json(&session.cookies().await?)?,
        Commands::ExportCookies { output } => {
            let output = session.policy().require_output_path(output)?;
            let cookies = session.cookies().await?;
            let bytes = serde_json::to_vec_pretty(&cookies)?;
            tokio::fs::write(&output, bytes).await?;
            println!("cookies exported to {}", output.display());
        }
        Commands::ImportCookies { input } => {
            const MAX_COOKIE_FILE_BYTES: u64 = 512 * 1024;
            let input = session.policy().require_existing_path(input)?;
            let metadata = tokio::fs::metadata(&input).await?;
            if metadata.len() > MAX_COOKIE_FILE_BYTES {
                return Err(format!(
                    "cookie file exceeds the {}-byte limit",
                    MAX_COOKIE_FILE_BYTES
                )
                .into());
            }
            let bytes = tokio::fs::read(&input).await?;
            let cookies: Vec<Cookie> = serde_json::from_slice(&bytes)?;
            session.set_cookies(&cookies).await?;
            println!("{} cookies imported", cookies.len());
        }
        Commands::Pdf { output, background } => {
            let output = session
                .policy()
                .require_output_path(std::path::Path::new(output))?;
            let mut opts = PdfOptions::letter();
            if *background {
                opts.print_background = Some(true);
            }
            let data = session.print_to_pdf(&opts).await?;
            let bytes = base64::engine::general_purpose::STANDARD.decode(&data)?;
            tokio::fs::write(&output, &bytes).await?;
            println!("PDF saved to {} ({} bytes)", output.display(), bytes.len());
        }
        Commands::FillForm {
            fields,
            expected_revision,
        } => {
            let parsed: Vec<serde_json::Value> = serde_json::from_str(fields)?;
            let field_refs: Vec<(String, String)> = parsed
                .iter()
                .map(|v| {
                    (
                        v["target"].as_str().unwrap_or("").to_string(),
                        v["value"].as_str().unwrap_or("").to_string(),
                    )
                })
                .collect();
            let field_slices: Vec<(&str, &str)> = field_refs
                .iter()
                .map(|(t, v)| (t.as_str(), v.as_str()))
                .collect();
            print_json(
                &session
                    .fill_form_with_expected_revision(&field_slices, *expected_revision)
                    .await?,
            )?;
        }
        Commands::Batch {
            input,
            atomic,
            mode,
            expected_revision,
        } => {
            let payload = read_json_input(input.as_ref())?;
            let steps_value = payload.get("steps").cloned().unwrap_or(payload);
            let steps: Vec<BatchStep> = serde_json::from_value(steps_value)
                .map_err(|error| format!("invalid batch document: {error}"))?;
            print_json(
                &session
                    .run_batch_with_mode(&steps, *atomic, *mode, *expected_revision)
                    .await?,
            )?;
        }
        Commands::Workflow {
            action: None,
            input,
        } => {
            let payload = read_json_input(input.as_ref())?;
            let workflow_value = payload
                .get("workflow")
                .cloned()
                .unwrap_or_else(|| payload.clone());
            let inputs_value = payload
                .get("inputs")
                .cloned()
                .unwrap_or_else(|| Value::Object(serde_json::Map::new()));
            let workflow = WorkflowDefinition::from_value(workflow_value)
                .map_err(|error| format!("invalid workflow: {error}"))?;
            let inputs: BTreeMap<String, Value> = serde_json::from_value(inputs_value)
                .map_err(|error| format!("invalid workflow inputs: {error}"))?;
            print_json(&session.run_workflow(&workflow, &inputs).await?)?;
        }
        Commands::Workflow {
            action: Some(_), ..
        } => unreachable!("offline workflow authoring commands are handled before browser startup"),
        Commands::WorkflowResume {
            workflow,
            checkpoint,
            inputs,
        } => {
            let workflow = WorkflowDefinition::from_value(read_json_input(Some(workflow))?)
                .map_err(|error| format!("invalid workflow: {error}"))?;
            let checkpoint: WorkflowCheckpoint =
                serde_json::from_value(read_json_input(Some(checkpoint))?)
                    .map_err(|error| format!("invalid workflow checkpoint: {error}"))?;
            let inputs: BTreeMap<String, Value> = match inputs {
                Some(path) => serde_json::from_value(read_json_input(Some(path))?)
                    .map_err(|error| format!("invalid workflow inputs: {error}"))?,
                None => BTreeMap::new(),
            };
            print_json(
                &session
                    .resume_workflow(&workflow, &inputs, &checkpoint)
                    .await?,
            )?;
        }
        Commands::ResolveIntent { input } => {
            let request = SemanticIntentRequest::from_json(&serde_json::to_string(
                &read_json_input(input.as_ref())?,
            )?)?;
            print_json(&session.resolve_intent(&request).await?)?;
        }
        Commands::ExecuteIntent { input } => {
            let execution = SemanticIntentExecutionRequest::from_json(&serde_json::to_string(
                &read_json_input(input.as_ref())?,
            )?)?;
            print_json(&session.execute_intent(&execution).await?)?;
        }
        Commands::Verify {
            predicate,
            timeout_ms,
        } => {
            let predicate: VerificationPredicate = serde_json::from_str(predicate)
                .map_err(|error| format!("invalid verification predicate: {error}"))?;
            print_json(
                &session
                    .verify(predicate, Duration::from_millis(*timeout_ms))
                    .await?,
            )?;
        }
        Commands::ReconcileRefs {
            from_revision,
            hints,
            scope,
            refs,
        } => {
            let options = ReconciliationOptions {
                hints: hints
                    .iter()
                    .map(|hint| Locator::parse(hint))
                    .collect::<BrowserResult<Vec<_>>>()?,
                scope_ref: scope.clone(),
            };
            print_json(
                &session
                    .reconcile_references_with_options(*from_revision, refs, &options)
                    .await?,
            )?;
        }
        Commands::ObserveDelta => {
            print_json(&session.observe_delta().await?)?;
        }
        Commands::Checkpoint { action } => match action {
            CheckpointCommand::Export => print_json(&session.export_checkpoint().await?)?,
            CheckpointCommand::Import { input } => {
                let checkpoint: CheckpointV1 =
                    serde_json::from_value(read_json_input(input.as_ref())?)
                        .map_err(|error| format!("invalid checkpoint: {error}"))?;
                session.import_checkpoint(&checkpoint).await?;
                print_json(&serde_json::json!({"status": "checkpoint_imported"}))?;
            }
        },
        Commands::ClipboardRead => {
            let text = session.clipboard_read().await?;
            println!("{text}");
        }
        Commands::ClipboardWrite { text } => {
            session.clipboard_write(text).await?;
            println!("Text written to clipboard");
        }
        Commands::SmokeSites { .. } => {
            unreachable!("handled before starting a browser session")
        }
        Commands::Tui
        | Commands::Browser { .. }
        | Commands::Session { .. }
        | Commands::Help { .. }
        | Commands::Ir { .. }
        | Commands::InstallChromium { .. }
        | Commands::Profiles { .. }
        | Commands::DeleteProfile { .. } => {
            unreachable!("handled before starting a browser session")
        }
    }
    Ok(())
}

fn parse_semantic_level(value: &str) -> BrowserResult<SemanticObservationLevel> {
    match value {
        "summary" => Ok(SemanticObservationLevel::Summary),
        "interactive" => Ok(SemanticObservationLevel::Interactive),
        "structured" => Ok(SemanticObservationLevel::Structured),
        "detailed" => Ok(SemanticObservationLevel::Detailed),
        "raw" => Ok(SemanticObservationLevel::Raw),
        _ => Err("expected summary, interactive, structured, detailed, or raw".into()),
    }
}

const MAX_EXPERIENCE_INPUT_BYTES: u64 = 4 * 1024 * 1024;

fn read_json_input(path: Option<&std::path::PathBuf>) -> BrowserResult<serde_json::Value> {
    let mut input = String::new();
    match path {
        Some(path) if path.as_os_str() == "-" => {
            std::io::stdin()
                .take(MAX_EXPERIENCE_INPUT_BYTES + 1)
                .read_to_string(&mut input)
                .map_err(|error| format!("could not read JSON input from stdin: {error}"))?;
        }
        Some(path) => {
            let text = path.to_string_lossy();
            if text.trim_start().starts_with('{') || text.trim_start().starts_with('[') {
                return Err(
                    "JSON input expects a file path; omit the path or use '-' to read stdin".into(),
                );
            }
            let file = std::fs::File::open(path).map_err(|error| {
                format!("could not read JSON input '{}': {error}", path.display())
            })?;
            if file.metadata()?.len() > MAX_EXPERIENCE_INPUT_BYTES {
                return Err(format!(
                    "JSON input exceeds the {}-byte bound",
                    MAX_EXPERIENCE_INPUT_BYTES
                )
                .into());
            }
            file.take(MAX_EXPERIENCE_INPUT_BYTES + 1)
                .read_to_string(&mut input)
                .map_err(|error| {
                    format!("could not read JSON input '{}': {error}", path.display())
                })?;
        }
        None => {
            std::io::stdin()
                .take(MAX_EXPERIENCE_INPUT_BYTES + 1)
                .read_to_string(&mut input)
                .map_err(|error| format!("could not read JSON input from stdin: {error}"))?;
        }
    };
    if input.len() as u64 > MAX_EXPERIENCE_INPUT_BYTES {
        return Err(format!(
            "JSON input exceeds the {}-byte bound",
            MAX_EXPERIENCE_INPUT_BYTES
        )
        .into());
    }
    serde_json::from_str(&input).map_err(|error| format!("invalid JSON input: {error}").into())
}

pub(crate) fn policy_from_cli(cli: &Cli) -> BrowserResult<BrowserPolicy> {
    let unrestricted_capabilities = [
        PolicyCapability::Attach,
        PolicyCapability::PersistentProfile,
        PolicyCapability::Evaluate,
        PolicyCapability::Upload,
        PolicyCapability::Download,
        PolicyCapability::Screenshot,
        PolicyCapability::RawCdp,
        PolicyCapability::ReadFormValues,
        PolicyCapability::ReadSensitiveFormValues,
        PolicyCapability::ReadSensitiveExtraction,
        PolicyCapability::CoordinateClick,
        PolicyCapability::ConsentDismissal,
        PolicyCapability::DeclaredAgentIdentity,
    ];
    Ok(BrowserPolicy::new(
        cli.policy,
        std::env::current_dir()?,
        if cli.yolo {
            unrestricted_capabilities.as_slice()
        } else {
            cli.policy_allow.as_slice()
        }
        .iter()
        .copied(),
        if cli.yolo {
            &[]
        } else {
            cli.policy_confirm.as_slice()
        }
        .iter()
        .copied(),
    )?
    .with_host_rules(
        cli.policy_allow_host.iter().cloned(),
        cli.policy_deny_host.iter().cloned(),
    )?
    .with_confirmation_tokens(
        if cli.yolo {
            &[]
        } else {
            cli.policy_confirm_once.as_slice()
        }
        .iter()
        .copied(),
    )?)
}

async fn run_prompt(
    session: &BrowserSession,
    prompt: &str,
    response_mode: ResponseMode,
) -> BrowserResult<()> {
    let trimmed = prompt.trim();
    let lower = trimmed.to_lowercase();

    for prefix in ["navigate to ", "go to ", "open "] {
        if lower.starts_with(prefix) {
            let page = session.navigate(trimmed[prefix.len()..].trim()).await?;
            print_json_mode(&page, response_mode)?;
            return Ok(());
        }
    }
    if let Some(rest) = lower.strip_prefix("click ") {
        let target = &trimmed[trimmed.len() - rest.len()..];
        print_json_mode(
            &session.click(target.trim_matches('"')).await?,
            response_mode,
        )?;
        return Ok(());
    }
    if let Some(rest) = lower.strip_prefix("double click ") {
        let target = &trimmed[trimmed.len() - rest.len()..];
        print_json_mode(
            &session.double_click(target.trim_matches('"')).await?,
            response_mode,
        )?;
        return Ok(());
    }
    if let Some(rest) = lower.strip_prefix("type ") {
        let text = &trimmed[trimmed.len() - rest.len()..];
        print_json_mode(
            &session.type_text(text.trim_matches('"'), None).await?,
            response_mode,
        )?;
        return Ok(());
    }
    if lower.starts_with("screenshot") {
        let output = trimmed
            .split_once(char::is_whitespace)
            .map(|(_, value)| value.trim())
            .filter(|value| !value.is_empty())
            .unwrap_or("screenshot.png");
        let output = session
            .policy()
            .require_output_path(std::path::Path::new(output))?;
        std::fs::write(&output, session.screenshot_png().await?)?;
        println!("wrote {}", output.display());
        return Ok(());
    }
    if matches!(
        lower.as_str(),
        "text" | "get text" | "page text" | "get content"
    ) {
        println!("{}", session.text().await?);
        return Ok(());
    }
    if matches!(lower.as_str(), "dom" | "snapshot" | "get dom") {
        print_json_mode(&session.deep_dom().await?, response_mode)?;
        return Ok(());
    }
    if matches!(lower.as_str(), "observe" | "context") {
        print_json_mode(&session.observe().await?, response_mode)?;
        return Ok(());
    }

    print_json_mode(&session.evaluate(trimmed).await?, response_mode)?;
    Ok(())
}

const MAX_TARGET_ARCHIVE_BYTES: usize = 64 * 1024;

fn target_archive<T: Serialize>(targets: &[T]) -> BrowserResult<Value> {
    let archive = serde_json::json!({
        "schemaVersion": "glass.target-archive.v1",
        "targets": targets,
    });
    if serde_json::to_vec(&archive)?.len() > MAX_TARGET_ARCHIVE_BYTES {
        return Err("target archive exceeds the 64 KiB output bound".into());
    }
    Ok(archive)
}
fn print_json_mode<T: Serialize + ?Sized>(value: &T, mode: ResponseMode) -> BrowserResult<()> {
    let value = serde_json::to_value(value)?;
    let projected = project_and_store(value, mode, "cli", default_result_store_path())?;
    println!("{}", compact_json(&projected)?);
    Ok(())
}

fn print_json<T: Serialize + ?Sized>(value: &T) -> BrowserResult<()> {
    println!("{}", compact_json(value)?);
    Ok(())
}

fn compact_json<T: Serialize + ?Sized>(value: &T) -> BrowserResult<String> {
    let mut value = serde_json::to_value(value)?;
    let payload = serde_json::to_vec(&value)?;
    let payload_bytes = payload.len();
    if let Some(object) = value.as_object_mut() {
        object.insert(
            "contextCost".to_string(),
            serde_json::json!({
                "payloadBytes": payload_bytes,
                "estimatedTokens": payload_bytes.div_ceil(4)
            }),
        );
    }

    Ok(serde_json::to_string(&value)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    #[test]
    fn default_dispatch_requires_both_terminal_streams_for_tui() {
        assert!(should_run_tui(true, true));
        assert!(!should_run_tui(false, true));
        assert!(!should_run_tui(true, false));
        assert!(!should_run_tui(false, false));
    }

    #[test]
    fn yolo_allows_capabilities_without_confirmation() {
        let cli = Cli::try_parse_from([
            "glass",
            "--yolo",
            "--policy",
            "hardened",
            "--policy-confirm",
            "evaluate",
            "doctor",
        ])
        .unwrap();
        let policy = policy_from_cli(&cli).unwrap();
        assert!(matches!(
            policy.decide(PolicyCapability::Evaluate),
            crate::browser::policy::PolicyDecision::Allow
        ));
        assert!(matches!(
            policy.decide(PolicyCapability::RawCdp),
            crate::browser::policy::PolicyDecision::Allow
        ));
    }

    #[test]
    fn non_interactive_default_has_concise_start_here_guidance() {
        let message = start_here_message();

        assert!(message.contains("START HERE"));
        assert!(message.contains("glass \"navigate to https://example.com\""));
        assert!(message.contains("glass doctor"));
        assert!(message.contains("glass --help"));
    }
    use serde_json::json;

    #[test]
    fn structured_cli_output_is_compact_json() {
        let output = compact_json(&json!({
            "page": {"title": "Glass", "url": "https://example.com"},
            "items": [1, 2]
        }))
        .unwrap();

        let parsed = serde_json::from_str::<serde_json::Value>(&output).unwrap();
        assert!(!output.contains('\n'));
        assert_eq!(parsed["items"], json!([1, 2]));
        assert!(parsed["contextCost"]["payloadBytes"].as_u64().unwrap() > 0);
        assert!(parsed["contextCost"]["estimatedTokens"].as_u64().unwrap() > 0);
    }

    #[test]
    fn target_archive_is_versioned_and_bounded() {
        let archive = target_archive(&[json!({
            "id": "page-1",
            "url": "https://example.test/docs",
            "title": "Docs",
            "active": true
        })])
        .unwrap();
        assert_eq!(archive["schemaVersion"], "glass.target-archive.v1");
        assert_eq!(archive["targets"].as_array().unwrap().len(), 1);

        let oversized = vec!["x".repeat(MAX_TARGET_ARCHIVE_BYTES)];
        let error = target_archive(&oversized).unwrap_err().to_string();
        assert!(error.contains("64 KiB"));
    }

    #[test]
    fn path_writability_accepts_missing_child_under_writable_parent() {
        let root = std::env::temp_dir().join(format!("glass-doctor-test-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        let missing_child = root.join("future").join("artifact");

        assert!(path_is_writable(Some(&missing_child)));

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn path_writability_rejects_missing_path_without_parent() {
        assert!(!path_is_writable(None));
    }

    #[test]
    fn workflow_starter_templates_compile_without_coordinate_actions() {
        for name in [
            "account-search",
            "checkout-submit",
            "profile-update",
            "report-download",
            "support-ticket",
        ] {
            let source = workflow_template(name).unwrap();
            let document = compile_workflow(source, WorkflowAuthoringFormat::Yaml).unwrap();
            assert!(
                !document
                    .diagnostics
                    .iter()
                    .any(|diagnostic| diagnostic.severity == WorkflowDiagnosticSeverity::Error),
                "{name} diagnostics: {:?}",
                document.diagnostics
            );
            assert!(!source.contains("clickAt"));
        }
    }
    #[test]
    fn task_explanation_contains_metadata_without_input_values() {
        let task = crate::task_protocol::GlassTask::from_json(
            r#"{
                "schemaVersion": 1,
                "task": "form.fill",
                "scope": {"regionName": "Checkout"},
                "inputs": {"email": "Kuching-secret"},
                "limits": {"maxActions": 4, "timeoutMs": 2000, "maxItems": 16},
                "risk": "localMutation"
            }"#,
        )
        .unwrap();
        let plan =
            crate::task_compiler::compile_task(&task, &crate::task_compiler::test_compiler_ir())
                .unwrap();

        let explanation = explain_task(&task, &plan).unwrap();

        assert!(explanation.contains("task: form.fill"));
        assert!(explanation.contains("scope: {\"regionName\":\"Checkout\"}"));
        assert!(explanation.contains("inputNames=[\"email\"]"));
        assert!(!explanation.contains("Kuching-secret"));
    }
    #[test]
    fn cli_task_requests_use_canonical_protocol_helpers() {
        let path = std::env::temp_dir().join(format!(
            "glass-cli-task-protocol-{}.json",
            std::process::id()
        ));
        let ir_path = std::env::temp_dir().join(format!(
            "glass-cli-task-protocol-ir-{}.json",
            std::process::id()
        ));
        std::fs::write(
            &ir_path,
            serde_json::to_vec(&crate::task_compiler::test_compiler_ir()).unwrap(),
        )
        .unwrap();
        std::fs::write(
            &path,
            r#"{
                "schemaVersion": 1,
                "task": "region.extract",
                "scope": {"regionName": "Checkout", "entityKind": "region"},
                "limits": {"maxActions": 4, "timeoutMs": 2000, "maxItems": 16},
                "risk": "readOnly"
            }"#,
        )
        .unwrap();

        let validate = read_task_request(&path, TASK_VALIDATE_OPERATION).unwrap();
        assert_eq!(validate.operation, TASK_VALIDATE_OPERATION);
        assert!(crate::protocol::validate_task_result(&validate).is_ok());

        let compile = read_task_compile_request(&path, &ir_path).unwrap();
        assert_eq!(compile.operation, TASK_COMPILE_OPERATION);
        assert!(crate::protocol::compile_task_result(&compile).is_ok());

        let execute = read_task_execution_request(&path, 7, true).unwrap();
        assert_eq!(execute.operation, TASK_EXECUTE_OPERATION);
        let payload = execute.decode_task_execute().unwrap();
        assert_eq!(payload.expected_revision, 7);
        assert!(payload.confirmed);

        std::fs::write(
            &path,
            r#"{
                "schemaVersion": 1,
                "task": "form.fill",
                "scope": {"regionName": "Checkout"},
                "limits": {"maxActions": 4, "timeoutMs": 2000, "maxItems": 16},
                "risk": "readOnly"
            }"#,
        )
        .unwrap();
        let invalid = read_task_compile_request(&path, &ir_path).unwrap();
        assert!(matches!(
            crate::protocol::compile_task_result(&invalid),
            Err(crate::protocol::ProtocolError::TaskCompilation(_))
        ));

        std::fs::remove_file(path).unwrap();
        std::fs::remove_file(ir_path).unwrap();
    }
    #[test]
    fn cli_web_ir_requests_use_canonical_protocol_helpers() {
        let fixture: Value =
            serde_json::from_str(include_str!("../../tests/fixtures/protocol-golden-v1.json"))
                .unwrap();
        let draft = fixture["requests"]
            .as_array()
            .unwrap()
            .iter()
            .find(|request| request["operation"] == WEB_IR_INSPECT_OPERATION)
            .and_then(|request| request["payload"]["ir"].as_object())
            .cloned()
            .map(Value::Object)
            .unwrap();
        let path = std::env::temp_dir().join(format!(
            "glass-cli-web-ir-protocol-{}.json",
            std::process::id()
        ));
        std::fs::write(&path, serde_json::to_vec(&draft).unwrap()).unwrap();

        let validate = read_web_ir_request(&path, WEB_IR_VALIDATE_OPERATION).unwrap();
        assert_eq!(validate.operation, WEB_IR_VALIDATE_OPERATION);
        assert!(crate::protocol::web_ir_validate_result(&validate).is_ok());

        let inspect = read_web_ir_request(&path, WEB_IR_INSPECT_OPERATION).unwrap();
        assert_eq!(inspect.operation, WEB_IR_INSPECT_OPERATION);
        assert!(crate::protocol::web_ir_inspect_result(&inspect).is_ok());
        let before = read_web_ir(&path).unwrap();
        let diff = before.diff(&before).unwrap();
        let summary = crate::protocol::WebIrDiffResult::from_diff(&diff);
        assert_eq!(summary.from_revision, diff.from_revision);
        assert_eq!(summary.to_revision, diff.to_revision);
        assert_eq!(summary.entity_added_count, 0);
        assert_eq!(summary.entity_removed_count, 0);
        assert_eq!(summary.entity_changed_count, 0);
        assert_eq!(summary.relationship_added_count, 0);
        assert_eq!(summary.relationship_removed_count, 0);

        let continuity = read_web_ir_continuity_request(&path, &path, "page").unwrap();
        assert_eq!(continuity.operation, WEB_IR_CONTINUITY_OPERATION);
        let result = crate::protocol::web_ir_continuity_result(&continuity).unwrap();
        assert_eq!(
            result.status,
            crate::web_ir::WebIrEntityContinuityStatus::Unchanged
        );

        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn batch_input_rejects_inline_json_and_missing_paths_clearly() {
        let inline = std::path::PathBuf::from(r#"{"steps":[]}"#);
        let error = read_json_input(Some(&inline)).unwrap_err().to_string();
        assert!(error.contains("expects a file path"));

        let missing = std::env::temp_dir().join(format!(
            "glass-batch-missing-input-{}.json",
            std::process::id()
        ));
        let error = read_json_input(Some(&missing)).unwrap_err().to_string();
        assert!(error.contains("could not read JSON input"));
    }
}
