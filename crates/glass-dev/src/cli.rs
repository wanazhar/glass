//! Glass Dev-owned project CLI dispatch.

use crate::development::{
    Actor, ExperimentManager, HarnessRequest, LinkProvenance, LocalHarness, PiHarness,
    ProcessSnapshot, ProjectWorkspace, SemanticBreakpoint, SemanticSnapshot,
};
use glass_browser::cli::args::{
    AgentCommand, AgentHarness, ExperimentCommand, ExternalAgentSandbox, HarnessCommand,
    NeovimCommand, ProjectCommand, ProjectGraphCommand, ProjectProcessCommand,
};
use serde::Serialize;
use std::io::Read;
use std::time::Duration;

type CliResult<T> = Result<T, Box<dyn std::error::Error>>;

pub fn dispatch_agent(action: &AgentCommand, unrestricted: bool) -> CliResult<()> {
    match action {
        AgentCommand::Doctor | AgentCommand::Status => {
            return print_json(&crate::pi_runtime::pi_readiness()?);
        }
        AgentCommand::Setup {
            sdk_entry,
            agent_dir,
            update,
            login,
        } => {
            return print_json(&crate::pi_runtime::setup_pi_runtime(
                sdk_entry.as_deref(),
                agent_dir.as_deref(),
                *update,
                *login,
            )?);
        }
        _ => {}
    }
    if let AgentCommand::Delegate {
        harness,
        prompt,
        root,
        sandbox,
        timeout_secs,
        allow_mutation,
        yes,
    } = action
    {
        let sandbox = match sandbox {
            ExternalAgentSandbox::ReadOnly => crate::external_agents::ExternalSandbox::ReadOnly,
            ExternalAgentSandbox::WorkspaceWrite => {
                if !(*allow_mutation || unrestricted) {
                    return Err(
                        "workspace-write delegation requires --allow-mutation (or --yolo)".into(),
                    );
                }
                if !(*yes || unrestricted) {
                    return Err(
                        "workspace-write delegation requires --yes for the exact request".into(),
                    );
                }
                crate::external_agents::ExternalSandbox::WorkspaceWrite
            }
        };
        let prompt = if prompt == "-" {
            let mut stdin_prompt = String::new();
            std::io::stdin().read_to_string(&mut stdin_prompt)?;
            stdin_prompt
        } else {
            prompt.clone()
        };
        let result =
            crate::external_agents::delegate(crate::external_agents::ExternalAgentRequest {
                harness: harness.clone(),
                root: root.clone(),
                prompt,
                sandbox,
                timeout: Duration::from_secs(*timeout_secs),
                allow_mutation: *allow_mutation || unrestricted,
            })?;
        print_json(&result)?;
        if !result.success {
            let reason = if result.timed_out {
                " after the configured timeout".into()
            } else {
                result
                    .status
                    .map(|status| format!(" with status {status}"))
                    .unwrap_or_else(|| " before a successful exit".into())
            };
            return Err(format!("{} delegation failed{reason}", result.harness).into());
        }
        return Ok(());
    }
    let (root, request, adapter) = match action {
        AgentCommand::Tool { .. } | AgentCommand::ToolFile { .. } => {
            return Err("agent tool calls use the resident broker path".into());
        }
        AgentCommand::Hello { root, harness } => (root, HarnessRequest::Hello, *harness),
        AgentCommand::Prompt {
            root,
            text,
            harness,
        } => (
            root,
            HarnessRequest::Prompt { text: text.clone() },
            *harness,
        ),
        AgentCommand::Steer {
            root,
            text,
            harness,
        } => (root, HarnessRequest::Steer { text: text.clone() }, *harness),
        AgentCommand::FollowUp { root, text } => (
            root,
            HarnessRequest::FollowUp { text: text.clone() },
            AgentHarness::Pi,
        ),
        AgentCommand::Models { root } => (root, HarnessRequest::Models, AgentHarness::Pi),
        AgentCommand::SetModel {
            root,
            provider,
            model_id,
        } => (
            root,
            HarnessRequest::SetModel {
                provider: provider.clone(),
                model_id: model_id.clone(),
            },
            AgentHarness::Pi,
        ),
        AgentCommand::Thinking { root, level } => (
            root,
            HarnessRequest::SetThinking {
                level: level.clone(),
            },
            AgentHarness::Pi,
        ),
        AgentCommand::Abort { root } => (root, HarnessRequest::Abort, AgentHarness::Pi),
        AgentCommand::NewSession { root } => (root, HarnessRequest::NewSession, AgentHarness::Pi),
        AgentCommand::Doctor | AgentCommand::Setup { .. } | AgentCommand::Status => {
            unreachable!("readiness commands return before harness dispatch")
        }
        AgentCommand::Delegate { .. } => unreachable!("delegation returns before harness dispatch"),
    };
    let mut workspace = ProjectWorkspace::open(root)?;
    match adapter {
        AgentHarness::Local => {
            let mut harness = LocalHarness::default();
            print_json(&harness.handle(&mut workspace, request)?)?;
        }
        AgentHarness::Pi => {
            let mut harness = PiHarness::spawn_with_unrestricted(workspace.root(), unrestricted)?;
            print_json(&harness.request(request)?)?;
        }
    }
    Ok(())
}

pub fn dispatch_harness(action: &HarnessCommand) -> CliResult<()> {
    match action {
        HarnessCommand::List => {
            let harnesses = crate::harness::discover()
                .into_iter()
                .map(|status| {
                    serde_json::json!({
                        "id": status.spec.id,
                        "label": status.spec.label,
                        "binary": status.spec.binary,
                        "description": status.spec.description,
                        "installed": status.path.is_some(),
                        "path": status.path,
                        "temporaryDelegation": matches!(
                            status.spec.id,
                            "codex" | "claude" | "opencode"
                        ),
                    })
                })
                .collect::<Vec<_>>();
            print_json(&serde_json::json!({"harnesses": harnesses}))?;
        }
        HarnessCommand::Start { name, root } => {
            let resolved = crate::harness::resolve(name)?;
            let status = crate::harness::launch_resolved(&resolved, root)?;
            print_json(&serde_json::json!({
                "harness": resolved.spec.id,
                "label": resolved.spec.label,
                "path": resolved.path,
                "root": root,
                "success": status.success(),
                "status": status.code(),
            }))?;
            if !status.success() {
                return Err(format!(
                    "{} exited{}",
                    resolved.spec.label,
                    status.code().map_or_else(
                        || " from a signal".into(),
                        |code| format!(" with status {code}")
                    )
                )
                .into());
            }
        }
    }
    Ok(())
}

pub fn dispatch_project(action: &ProjectCommand) -> CliResult<()> {
    match action {
        ProjectCommand::Inspect { root } => {
            let workspace = ProjectWorkspace::open(root)?;
            print_json(&serde_json::json!({
                "schemaVersion": crate::development::DEVELOPMENT_SCHEMA_VERSION,
                "root": workspace.root(),
                "detection": workspace.detection(),
                "config": workspace.config(),
                "revision": workspace.revision(),
            }))?;
        }
        ProjectCommand::Files { root } => {
            let workspace = ProjectWorkspace::open(root)?;
            print_json(&workspace.list_files_result()?)?;
        }
        ProjectCommand::Search { root, query, limit } => {
            let mut workspace = ProjectWorkspace::open(root)?;
            print_json(&workspace.search(query, *limit)?)?;
        }
        ProjectCommand::Read { root, path } => {
            let mut workspace = ProjectWorkspace::open(root)?;
            let content = workspace.read_file(path)?;
            print_json(&serde_json::json!({"path": path, "content": content}))?;
        }
        ProjectCommand::Edit {
            root,
            path,
            content,
            input,
        } => {
            let content = match (content, input) {
                (Some(content), None) => content.clone(),
                (None, Some(input)) => std::fs::read_to_string(input)?,
                (None, None) => {
                    return Err("project edit requires --content or --input".into());
                }
                (Some(_), Some(_)) => unreachable!("clap prevents conflicting edit inputs"),
            };
            let mut workspace = ProjectWorkspace::open(root)?;
            workspace.edit_buffer(path, content, Actor::local())?;
            print_json(&workspace.save_buffer(path)?)?;
        }
        ProjectCommand::Mkdir { root, path } => {
            let mut workspace = ProjectWorkspace::open(root)?;
            workspace.create_directory(path, Actor::local())?;
            print_json(&serde_json::json!({"path": path, "created": true}))?;
        }
        ProjectCommand::Rename { root, from, to } => {
            let mut workspace = ProjectWorkspace::open(root)?;
            workspace.rename_path(from, to, Actor::local())?;
            print_json(&serde_json::json!({"from": from, "to": to, "renamed": true}))?;
        }
        ProjectCommand::Delete { root, path, yes } => {
            if !yes {
                return Err("project delete requires --yes confirmation".into());
            }
            let mut workspace = ProjectWorkspace::open(root)?;
            workspace.delete_path(path, Actor::local())?;
            print_json(&serde_json::json!({"path": path, "deleted": true}))?;
        }
        ProjectCommand::Diagnostics { root, path } => {
            let mut workspace = ProjectWorkspace::open(root)?;
            print_json(&workspace.publish_rust_diagnostics(path)?)?;
        }
        ProjectCommand::Run {
            root,
            name,
            command,
            wait,
        } => {
            if !wait {
                return Err("one-shot CLI process runs require --wait; use the TUI for persistent interactive processes".into());
            }
            let mut workspace = ProjectWorkspace::open(root)?;
            let command = command
                .clone()
                .or_else(|| detected_command(workspace.detection(), name))
                .ok_or_else(|| format!("no configured command for process {name}"))?;
            #[cfg(windows)]
            let snapshot = workspace.run_command_to_completion(
                name,
                &command,
                std::time::Duration::from_secs(600),
            )?;
            #[cfg(not(windows))]
            let snapshot = {
                let snapshot = workspace.start_process(name, &command)?;
                if *wait {
                    workspace.processes().close_input(name)?;
                    wait_for_project_process(&mut workspace, name)?
                } else {
                    snapshot
                }
            };
            print_json(&snapshot)?;
        }
        ProjectCommand::Test { root } => {
            run_detected_command(root, "test", workspace_test_command)?;
        }
        ProjectCommand::Lint { root } => {
            run_detected_command(root, "lint", workspace_lint_command)?;
        }
        ProjectCommand::Process { root, action } => {
            let mut workspace = ProjectWorkspace::open(root)?;
            match action {
                ProjectProcessCommand::List => print_json(&workspace.processes().list_checked()?)?,
                ProjectProcessCommand::Start {
                    name,
                    command,
                    wait,
                } => {
                    if !wait {
                        return Err("one-shot CLI process starts require --wait; use the TUI for persistent interactive processes".into());
                    }
                    #[cfg(windows)]
                    let snapshot = workspace.run_command_to_completion(
                        name,
                        command,
                        std::time::Duration::from_secs(600),
                    )?;
                    #[cfg(not(windows))]
                    let snapshot = {
                        workspace.start_process(name, command)?;
                        if *wait {
                            workspace.processes().close_input(name)?;
                            wait_for_project_process(&mut workspace, name)?
                        } else {
                            workspace
                                .processes()
                                .list()
                                .into_iter()
                                .find(|process| process.name == *name)
                                .ok_or_else(|| format!("process {name} disappeared"))?
                        }
                    };
                    print_json(&snapshot)?;
                }
                ProjectProcessCommand::Stop { name } => {
                    print_json(&workspace.stop_process(name)?)?;
                }
                ProjectProcessCommand::Restart { name } => {
                    print_json(&workspace.processes().restart(name)?)?;
                }
                ProjectProcessCommand::Remove { name } => {
                    print_json(&workspace.processes().remove(name)?)?;
                }
                ProjectProcessCommand::Input { name, input } => {
                    workspace.processes().send_input(name, input)?;
                    print_json(&serde_json::json!({"name": name, "accepted": true}))?;
                }
                ProjectProcessCommand::Resize { name, cols, rows } => {
                    workspace.processes().resize(name, *cols, *rows)?;
                    print_json(&serde_json::json!({"name": name, "cols": cols, "rows": rows}))?;
                }
                ProjectProcessCommand::Output { name } => {
                    print_json(&serde_json::json!({
                        "name": name,
                        "output": workspace.processes().output(name)?,
                    }))?;
                }
            }
        }
        ProjectCommand::Diff { root } => {
            let mut workspace = ProjectWorkspace::open(root)?;
            print_json(&workspace.diff()?)?;
        }
        ProjectCommand::Link {
            root,
            entity,
            path,
            start_line,
            end_line,
            provenance,
            confidence,
            detail,
        } => {
            let mut workspace = ProjectWorkspace::open(root)?;
            let provenance = parse_link_provenance(provenance)?;
            let link = workspace.link_runtime_source(
                entity,
                path,
                *start_line,
                *end_line,
                provenance,
                detail,
                *confidence,
                Actor::local(),
            )?;
            print_json(&link)?;
        }
        ProjectCommand::Timeline { root } => {
            let workspace = ProjectWorkspace::open(root)?;
            print_json(&workspace.timeline().events().collect::<Vec<_>>())?;
        }
        ProjectCommand::Graph { root, action } => {
            let mut workspace = ProjectWorkspace::open(root)?;
            match action {
                ProjectGraphCommand::Discover => print_json(&workspace.discover_runtime_links()?)?,
                ProjectGraphCommand::Entity { entity } => {
                    print_json(&workspace.graph().links_for(entity))?
                }
                ProjectGraphCommand::Source { path, line } => {
                    print_json(&workspace.graph().entities_for_source(path, *line))?
                }
            }
        }
        ProjectCommand::Breakpoint {
            root,
            kind,
            entity,
            before,
            after,
        } => {
            let mut workspace = ProjectWorkspace::open(root)?;
            workspace.discover_runtime_links()?;
            let before: SemanticSnapshot = serde_json::from_slice(&std::fs::read(before)?)?;
            let after: SemanticSnapshot = serde_json::from_slice(&std::fs::read(after)?)?;
            let breakpoint = match kind.as_str() {
                "disappears" => SemanticBreakpoint::EntityDisappears { entity_id: entity.clone() },
                "name-missing" => SemanticBreakpoint::AccessibleNameMissing { entity_id: Some(entity.clone()) },
                "role-changes" => SemanticBreakpoint::RoleChanges { entity_id: entity.clone() },
                "actionability-lost" => SemanticBreakpoint::ActionabilityLost { entity_id: entity.clone() },
                _ => return Err("breakpoint kind must be disappears, name-missing, role-changes, or actionability-lost".into()),
            };
            print_json(&workspace.evaluate_semantic_breakpoints(
                &[breakpoint],
                &before,
                &after,
            )?)?;
        }
        ProjectCommand::Replay { root, start, limit } => {
            let workspace = ProjectWorkspace::open(root)?;
            print_json(&workspace.replay(*start, *limit)?)?;
        }
        ProjectCommand::Neovim { root, action } => {
            let mut workspace = ProjectWorkspace::open(root)?;
            match action {
                NeovimCommand::Probe => print_json(&crate::development::probe_neovim()?)?,
                NeovimCommand::Start { name, path } => {
                    print_json(&crate::development::start_neovim(
                        workspace.processes(),
                        name,
                        path.as_deref(),
                    )?)?
                }
            }
        }
        ProjectCommand::Experiment { root, action } => {
            let manager = ExperimentManager::new(root)?;
            match action {
                ExperimentCommand::Create { name, port } => {
                    print_json(&manager.create(name, *port)?)?
                }
            }
        }
        ProjectCommand::Attach { root, actor } => {
            let mut workspace = ProjectWorkspace::open(root)?;
            let actor = Actor::external(actor);
            workspace.attach_actor(actor.clone())?;
            print_json(&actor)?;
        }
    }
    Ok(())
}

fn detected_command(
    detection: &crate::development::ProjectDetection,
    name: &str,
) -> Option<String> {
    match name {
        "dev" => detection.dev_command.clone(),
        "test" => detection.test_command.clone(),
        "lint" => detection.lint_command.clone(),
        "build" => detection.build_command.clone(),
        _ => None,
    }
}

fn workspace_test_command(detection: &crate::development::ProjectDetection) -> Option<String> {
    detection.test_command.clone()
}

fn workspace_lint_command(detection: &crate::development::ProjectDetection) -> Option<String> {
    detection.lint_command.clone()
}

fn run_detected_command(
    root: &std::path::Path,
    name: &str,
    command: fn(&crate::development::ProjectDetection) -> Option<String>,
) -> CliResult<()> {
    let mut workspace = ProjectWorkspace::open(root)?;
    let command = command(workspace.detection())
        .ok_or_else(|| format!("project has no detected {name} command"))?;
    print_json(&workspace.run_verification(name, &command, Duration::from_secs(600))?)?;
    Ok(())
}

fn wait_for_project_process(
    workspace: &mut ProjectWorkspace,
    name: &str,
) -> CliResult<ProcessSnapshot> {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(600);
    loop {
        let snapshots = workspace.processes().poll()?;
        let snapshot = snapshots
            .into_iter()
            .find(|process| process.name == name)
            .ok_or_else(|| format!("process {name} disappeared"))?;
        if !matches!(snapshot.state, crate::development::ProcessState::Running) {
            return Ok(snapshot);
        }
        if std::time::Instant::now() >= deadline {
            workspace.stop_process(name)?;
            return Err(format!("process {name} exceeded 600 seconds").into());
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
}

fn parse_link_provenance(value: &str) -> CliResult<LinkProvenance> {
    match value {
        "explicit-marker" => Ok(LinkProvenance::ExplicitMarker),
        "runtime-observation" => Ok(LinkProvenance::RuntimeObservation),
        "static-analysis" => Ok(LinkProvenance::StaticAnalysis),
        "inferred" => Ok(LinkProvenance::Inferred),
        _ => Err(
            "provenance must be explicit-marker, runtime-observation, static-analysis, or inferred"
                .into(),
        ),
    }
}

fn print_json<T: Serialize + ?Sized>(value: &T) -> CliResult<()> {
    println!("{}", serde_json::to_string_pretty(value)?);
    Ok(())
}
