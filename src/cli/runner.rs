//! CLI command dispatch and session orchestration.
//!
//! Routes parsed CLI arguments to the appropriate runner: one-shot browser
//! commands, interactive TUI, or the MCP stdio server.

use super::args::{
    CertifyCommand, CheckpointCommand, Cli, Commands, DaemonCommand, IrCommand, KnowledgeCommand,
    KnowledgeInvalidationState, McpClient, ProfileCommand, ResultCommand, SnapshotCommand,
    TaskCommand, WorkflowAuthoringCommand,
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
use crate::capabilities::GlassCapabilityManifest;
use crate::reliability::{
    ReliabilityFixtureManifest, ReliabilityReplayBundle, ReliabilityScenario,
    ReliabilityScenarioObservation, build_reliability_scorecard,
};
use crate::reliability_runner::{ReliabilityRunOptions, run_reliability_scenario};
use crate::results::{ResponseMode, ResultStore, project_and_store};
use base64::Engine;
use serde::Serialize;
use serde_json::Value;
use std::collections::BTreeMap;
use std::io::Read;
use std::path::Path;
use std::process::Stdio;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

/// Top-level command-line entry point: parses CLI arguments and dispatches
/// to the appropriate runner (one-shot, TUI, or MCP server).
pub async fn dispatch(cli: Cli) -> BrowserResult<()> {
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
            dispatch_profiles(action.as_ref())?;
            return Ok(());
        }
        Some(Commands::DeleteProfile { name }) => {
            policy.require(PolicyCapability::PersistentProfile)?;
            ProfileManager::new().delete_profile(name)?;
            println!("deleted profile {name}");
            return Ok(());
        }
        Some(Commands::Knowledge { action }) => {
            policy.require(PolicyCapability::PersistentProfile)?;
            dispatch_knowledge(action, cli.knowledge_store.as_deref(), &cli.profile)?;
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
            input: None,
        }) => {
            dispatch_workflow_authoring(action)?;
            return Ok(());
        }
        Some(Commands::Task { action }) => {
            dispatch_task(action)?;
            return Ok(());
        }
        Some(Commands::Ir { action }) => {
            dispatch_ir(action)?;
            return Ok(());
        }
        Some(Commands::Tui) | None if cli.prompt.is_none() => {
            return crate::tui::app::run_tui(&cli).await;
        }
        _ => {}
    }

    let options = SessionOptions {
        port: cli.port,
        chrome_path: cli.chrome_path.clone(),
        profile: cli.profile.clone(),
        incognito: cli.incognito,
        attach: cli.attach,
        target_id: cli.target_id.clone(),
        frame_id: cli.frame_id.clone(),
        headed: cli.headed,
        interaction_mode: cli.interaction,
        audit: cli.audit,
        policy: None,
    };
    let session = BrowserSession::start_with_policy(&options, policy).await?;
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

async fn dispatch_doctor(cli: &Cli, policy: &BrowserPolicy, json: bool) -> BrowserResult<()> {
    let executable = std::env::current_exe()
        .ok()
        .map(|path| path.display().to_string());
    let chrome_path =
        crate::browser::chrome::resolve_chrome_path(None).map(|path| path.display().to_string());
    let profile_path = ProfileManager::new().profile_dir(&cli.profile);
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
    let profiles = ProfileManager::new().list_profiles().unwrap_or_default();
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

fn dispatch_profiles(action: Option<&ProfileCommand>) -> BrowserResult<()> {
    let manager = ProfileManager::new();
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
            let source = std::fs::read_to_string(input)?;
            let task = crate::task_protocol::GlassTask::from_json(&source)?;
            print_json(&serde_json::json!({
                "valid": true,
                "schemaVersion": task.schema_version,
                "task": task.task,
            }))?;
        }
        TaskCommand::Compile {
            input,
            output,
            explain,
        } => {
            let source = std::fs::read_to_string(input)?;
            let task = crate::task_protocol::GlassTask::from_json(&source)?;
            let plan = crate::task_compiler::compile_task(&task)?;
            if let Some(output) = output {
                std::fs::write(output, serde_json::to_vec_pretty(&plan)?)?;
                println!("compiled task to {}", output.display());
            } else {
                print_json(&plan)?;
            }
            if *explain {
                eprintln!("{}", explain_task(&task, &plan)?);
            }
        }
    }
    Ok(())
}

fn dispatch_ir(action: &IrCommand) -> BrowserResult<()> {
    match action {
        IrCommand::Inspect { input } => {
            let draft = read_web_ir_draft(input)?;
            print_json(&serde_json::json!({
                "schemaVersion": draft.schema_version,
                "revision": draft.revision,
                "entityCount": draft.entities.len(),
                "relationshipCount": draft.relationships.len(),
                "coverage": draft.coverage,
                "truncated": draft.limits.truncated,
                "opaqueRegions": draft.coverage.opaque_regions,
                "diagnosticCount": draft.diagnostics.len(),
                "relationshipHintDiagnosticCount": draft.relationship_hint_diagnostics.len(),
            }))?;
        }
        IrCommand::Diff { before, after } => {
            let before = read_web_ir_draft(before)?;
            let after = read_web_ir_draft(after)?;
            print_json(&before.diff(&after)?)?;
        }
        IrCommand::Continuity {
            before,
            after,
            entity_id,
        } => {
            let before = read_web_ir_draft(before)?;
            let after = read_web_ir_draft(after)?;
            print_json(&before.classify_entity_continuity(&after, entity_id)?)?;
        }
        IrCommand::Canonical { input } => {
            let draft = read_web_ir_draft(input)?;
            println!("{}", draft.to_canonical_json()?);
        }
    }
    Ok(())
}

fn read_web_ir_draft(path: &Path) -> BrowserResult<crate::web_ir::GlassWebIrDraft> {
    let source = std::fs::read_to_string(path)?;
    let draft: crate::web_ir::GlassWebIrDraft = serde_json::from_str(&source)?;
    draft.validate()?;
    Ok(draft)
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
        Commands::Capabilities
        | Commands::Daemon { .. }
        | Commands::Doctor { .. }
        | Commands::McpConfig { .. }
        | Commands::Result { .. }
        | Commands::Certify { .. }
        | Commands::Knowledge { .. }
        | Commands::Snapshot { .. }
        | Commands::Task { .. } => {
            unreachable!("offline commands are handled before browser startup")
        }
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
            let cookies = session.cookies().await?;
            let bytes = serde_json::to_vec_pretty(&cookies)?;
            tokio::fs::write(output, bytes).await?;
            println!("cookies exported to {}", output.display());
        }
        Commands::ImportCookies { input } => {
            const MAX_COOKIE_FILE_BYTES: u64 = 512 * 1024;
            let metadata = tokio::fs::metadata(input).await?;
            if metadata.len() > MAX_COOKIE_FILE_BYTES {
                return Err(format!(
                    "cookie file exceeds the {}-byte limit",
                    MAX_COOKIE_FILE_BYTES
                )
                .into());
            }
            let bytes = tokio::fs::read(input).await?;
            let cookies: Vec<Cookie> = serde_json::from_slice(&bytes)?;
            session.set_cookies(&cookies).await?;
            println!("{} cookies imported", cookies.len());
        }
        Commands::Pdf { output, background } => {
            let mut opts = PdfOptions::letter();
            if *background {
                opts.print_background = Some(true);
            }
            let data = session.print_to_pdf(&opts).await?;
            let bytes = base64::engine::general_purpose::STANDARD.decode(&data)?;
            tokio::fs::write(&output, &bytes).await?;
            println!("PDF saved to {output} ({} bytes)", bytes.len());
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
        Commands::Tui
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

fn read_json_input(path: Option<&std::path::PathBuf>) -> BrowserResult<serde_json::Value> {
    let mut input = String::new();
    match path {
        Some(path) => std::fs::File::open(path)?.read_to_string(&mut input)?,
        None => std::io::stdin().read_to_string(&mut input)?,
    };
    Ok(serde_json::from_str(&input)?)
}

pub(crate) fn policy_from_cli(cli: &Cli) -> BrowserResult<BrowserPolicy> {
    Ok(BrowserPolicy::new(
        cli.policy,
        std::env::current_dir()?,
        cli.policy_allow.iter().copied(),
        cli.policy_confirm.iter().copied(),
    )?
    .with_host_rules(
        cli.policy_allow_host.iter().cloned(),
        cli.policy_deny_host.iter().cloned(),
    )?
    .with_confirmation_tokens(cli.policy_confirm_once.iter().copied())?)
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
                "scope": {"regionName": "Shipping address"},
                "inputs": {"city": "Kuching-secret"},
                "limits": {"maxActions": 4, "timeoutMs": 2000, "maxItems": 16},
                "risk": "localMutation"
            }"#,
        )
        .unwrap();
        let plan = crate::task_compiler::compile_task(&task).unwrap();

        let explanation = explain_task(&task, &plan).unwrap();

        assert!(explanation.contains("task: form.fill"));
        assert!(explanation.contains("scope: {\"regionName\":\"Shipping address\"}"));
        assert!(explanation.contains("inputNames=[\"city\"]"));
        assert!(!explanation.contains("Kuching-secret"));
    }
}
