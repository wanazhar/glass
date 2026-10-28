//! Glass Dev-owned project CLI dispatch.

use crate::development::{
    Actor, ExperimentManager, HarnessRequest, LinkProvenance, LocalHarness, MAX_FILE_BYTES,
    ProcessSnapshot, ProjectWorkspace, SemanticBreakpoint, SemanticSnapshot,
};
use glass_browser::cli::args::{
    AgentCommand, ExperimentCommand, ExternalAgentSandbox, HarnessCommand, NeovimCommand,
    ProjectCommand, ProjectGitCommand, ProjectGithubCommand, ProjectGraphCommand,
    ProjectProcessCommand,
};
use serde::Serialize;
use std::io::{IsTerminal, Read, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

type CliResult<T> = Result<T, Box<dyn std::error::Error>>;
static JSON_OUTPUT: AtomicBool = AtomicBool::new(false);

pub(crate) fn dispatch_agent(action: &AgentCommand, json: bool) -> CliResult<()> {
    JSON_OUTPUT.store(json, Ordering::Relaxed);
    match action {
        AgentCommand::Doctor | AgentCommand::Status => {
            let readiness = crate::resident_harness::resident_readiness()?;
            if should_emit_json(json, std::io::stdout().is_terminal()) {
                return print_json(&readiness);
            }
            let output = if matches!(action, AgentCommand::Status) {
                format_agent_status(&readiness)
            } else {
                format_agent_doctor(&readiness)
            };
            let mut stdout = std::io::stdout().lock();
            stdout.write_all(output.as_bytes())?;
            stdout.write_all(b"\n")?;
            return Ok(());
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
                if !*allow_mutation {
                    return Err("workspace-write delegation requires --allow-mutation".into());
                }
                if !*yes {
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
                authorization: crate::development::ToolAuthorization::from_user_confirmation(
                    Actor::local(),
                    *allow_mutation,
                    *yes,
                ),
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
    let (root, request) = match action {
        AgentCommand::Tool { .. } | AgentCommand::ToolFile { .. } => {
            return Err("agent tool calls use the resident broker path".into());
        }
        AgentCommand::Hello { root } => (root, HarnessRequest::Hello),
        AgentCommand::Prompt { root, text } => {
            (root, HarnessRequest::Prompt { text: text.clone() })
        }
        AgentCommand::Steer { root, text } => (root, HarnessRequest::Steer { text: text.clone() }),
        AgentCommand::Doctor | AgentCommand::Status => {
            unreachable!("readiness commands return before harness dispatch")
        }
        AgentCommand::Delegate { .. } => unreachable!("delegation returns before harness dispatch"),
    };
    let mut workspace = ProjectWorkspace::open(root)?;
    let mut harness = LocalHarness::default();
    print_json(&harness.handle(&mut workspace, request)?)?;
    Ok(())
}

pub(crate) fn dispatch_harness(action: &HarnessCommand, json: bool) -> CliResult<()> {
    JSON_OUTPUT.store(json, Ordering::Relaxed);
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

pub(crate) fn dispatch_project(
    action: &ProjectCommand,
    allow_mutation: bool,
    confirmed: bool,
    json: bool,
) -> CliResult<()> {
    JSON_OUTPUT.store(json, Ordering::Relaxed);
    if project_command_requires_mutation(action)
        && !crate::development::ToolAuthorization::factors_permit_mutation(
            allow_mutation,
            confirmed,
        )
    {
        return Err("project operation requires --allow-mutation and --yes".into());
    }
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
        ProjectCommand::Delete { root, path } => {
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
            let before: SemanticSnapshot =
                serde_json::from_slice(&read_semantic_snapshot(before)?)?;
            let after: SemanticSnapshot = serde_json::from_slice(&read_semantic_snapshot(after)?)?;
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
        ProjectCommand::Git { root, action } => {
            print_json(&execute_project_git(
                root,
                action,
                allow_mutation,
                confirmed,
            )?)?;
        }
        ProjectCommand::Github { root, action } => {
            print_json(&execute_project_github(
                root,
                action,
                allow_mutation,
                confirmed,
            )?)?;
        }
    }
    Ok(())
}

fn read_semantic_snapshot(path: &std::path::Path) -> std::io::Result<Vec<u8>> {
    let file = std::fs::File::open(path)?;
    let metadata = file.metadata()?;
    if !metadata.is_file() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "semantic snapshot is not a regular file",
        ));
    }
    if metadata.len() > MAX_FILE_BYTES as u64 {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("semantic snapshot exceeds the {MAX_FILE_BYTES} byte limit"),
        ));
    }
    let mut bytes = Vec::with_capacity(metadata.len() as usize);
    file.take(MAX_FILE_BYTES as u64 + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() > MAX_FILE_BYTES {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("semantic snapshot exceeds the {MAX_FILE_BYTES} byte limit"),
        ));
    }
    Ok(bytes)
}

#[cfg(test)]
mod semantic_snapshot_reader_tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT_PATH: AtomicU64 = AtomicU64::new(0);

    fn fixture_path() -> std::path::PathBuf {
        std::env::temp_dir().join(format!(
            "glass-semantic-snapshot-{}-{}",
            std::process::id(),
            NEXT_PATH.fetch_add(1, Ordering::Relaxed)
        ))
    }

    #[test]
    fn semantic_snapshot_reader_accepts_bounded_json() {
        let path = fixture_path();
        std::fs::write(&path, b"{}\n").unwrap();
        assert_eq!(read_semantic_snapshot(&path).unwrap(), b"{}\n");
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn semantic_snapshot_reader_rejects_oversized_input() {
        let path = fixture_path();
        std::fs::write(&path, vec![b' '; MAX_FILE_BYTES + 1]).unwrap();
        let error = read_semantic_snapshot(&path).unwrap_err();
        assert_eq!(error.kind(), std::io::ErrorKind::InvalidData);
        assert!(error.to_string().contains("byte limit"));
        std::fs::remove_file(path).unwrap();
    }
}

fn project_command_requires_mutation(action: &ProjectCommand) -> bool {
    match action {
        ProjectCommand::Edit { .. }
        | ProjectCommand::Mkdir { .. }
        | ProjectCommand::Rename { .. }
        | ProjectCommand::Delete { .. }
        | ProjectCommand::Diagnostics { .. }
        | ProjectCommand::Run { .. }
        | ProjectCommand::Test { .. }
        | ProjectCommand::Lint { .. }
        | ProjectCommand::Link { .. }
        | ProjectCommand::Breakpoint { .. }
        | ProjectCommand::Attach { .. }
        | ProjectCommand::Graph {
            action: ProjectGraphCommand::Discover,
            ..
        }
        | ProjectCommand::Neovim {
            action: NeovimCommand::Start { .. },
            ..
        }
        | ProjectCommand::Experiment { .. } => true,
        ProjectCommand::Git { action, .. } => !matches!(
            action,
            ProjectGitCommand::Status
                | ProjectGitCommand::Diff { .. }
                | ProjectGitCommand::Branches
                | ProjectGitCommand::StashList
                | ProjectGitCommand::WorktreeList
        ),
        ProjectCommand::Github { action, .. } => {
            matches!(action, ProjectGithubCommand::Ship { .. })
        }
        ProjectCommand::Process { action, .. } => !matches!(
            action,
            ProjectProcessCommand::List | ProjectProcessCommand::Output { .. }
        ),
        _ => false,
    }
}

fn execute_project_git(
    root: &std::path::Path,
    action: &ProjectGitCommand,
    allow_mutation: bool,
    confirmed: bool,
) -> CliResult<serde_json::Value> {
    let (name, arguments) = match action {
        ProjectGitCommand::Status => ("glass.git.status", serde_json::json!({})),
        ProjectGitCommand::Diff { staged, path } => (
            "glass.git.diff",
            serde_json::json!({"staged":staged,"path":path}),
        ),
        ProjectGitCommand::Stage { paths } => {
            ("glass.git.stage", serde_json::json!({"paths":paths}))
        }
        ProjectGitCommand::Unstage { paths } => {
            ("glass.git.unstage", serde_json::json!({"paths":paths}))
        }
        ProjectGitCommand::Discard { paths } => {
            ("glass.git.discard", serde_json::json!({"paths":paths}))
        }
        ProjectGitCommand::Fetch { remote } => {
            ("glass.git.fetch", serde_json::json!({"remote":remote}))
        }
        ProjectGitCommand::Pull { remote, branch } => (
            "glass.git.pull",
            serde_json::json!({"remote":remote,"branch":branch}),
        ),
        ProjectGitCommand::Push { remote, branch } => (
            "glass.git.push",
            serde_json::json!({"remote":remote,"branch":branch}),
        ),
        ProjectGitCommand::Merge { branch } => {
            ("glass.git.merge", serde_json::json!({"branch":branch}))
        }
        ProjectGitCommand::Rebase { onto } => {
            ("glass.git.rebase", serde_json::json!({"onto":onto}))
        }
        ProjectGitCommand::Commit { message } => {
            ("glass.git.commit", serde_json::json!({"message":message}))
        }
        ProjectGitCommand::Branches => ("glass.git.branches", serde_json::json!({})),
        ProjectGitCommand::BranchCreate { name, start_point } => (
            "glass.git.branch.create",
            serde_json::json!({"name":name,"startPoint":start_point}),
        ),
        ProjectGitCommand::BranchSwitch { name, create } => (
            "glass.git.branch.switch",
            serde_json::json!({"name":name,"create":create}),
        ),
        ProjectGitCommand::StashList => ("glass.git.stash.list", serde_json::json!({})),
        ProjectGitCommand::StashPush {
            message,
            include_untracked,
        } => (
            "glass.git.stash.push",
            serde_json::json!({"message":message,"includeUntracked":include_untracked}),
        ),
        ProjectGitCommand::StashPop { reference } => (
            "glass.git.stash.pop",
            serde_json::json!({"reference":reference}),
        ),
        ProjectGitCommand::WorktreeList => ("glass.git.worktree.list", serde_json::json!({})),
        ProjectGitCommand::WorktreeCreate {
            path,
            branch,
            create_branch,
        } => (
            "glass.git.worktree.create",
            serde_json::json!({"path":path,"branch":branch,"createBranch":create_branch}),
        ),
        ProjectGitCommand::WorktreeRemove { path, force } => (
            "glass.git.worktree.remove",
            serde_json::json!({"path":path,"force":force}),
        ),
    };
    execute_project_tool(root, name, arguments, allow_mutation, confirmed)
}

fn execute_project_github(
    root: &std::path::Path,
    action: &ProjectGithubCommand,
    allow_mutation: bool,
    confirmed: bool,
) -> CliResult<serde_json::Value> {
    let (name, arguments) = match action {
        ProjectGithubCommand::Review => ("glass.github.review", serde_json::json!({})),
        ProjectGithubCommand::Ship {
            title,
            body,
            base,
            draft,
        } => (
            "glass.github.ship",
            serde_json::json!({"title":title,"body":body,"base":base,"draft":draft}),
        ),
    };
    execute_project_tool(root, name, arguments, allow_mutation, confirmed)
}

fn execute_project_tool(
    root: &std::path::Path,
    name: &str,
    arguments: serde_json::Value,
    allow_mutation: bool,
    confirmed: bool,
) -> CliResult<serde_json::Value> {
    use crate::development::{Actor, ToolAuthorization, ToolCall};
    let mut workspace = crate::workspace::DevelopmentWorkspace::open(root)?;
    let context = crate::tools::DevelopmentToolContext {
        authorization: ToolAuthorization::from_user_confirmation(
            Actor::local(),
            allow_mutation,
            confirmed,
        ),
        initiator: None,
        expected_generation: workspace.generation(),
        expected_project_revision: workspace.project().revision(),
    };
    Ok(workspace.execute_tool(
        &ToolCall {
            id: format!("cli-{name}"),
            name: name.into(),
            arguments,
        },
        &context,
    )?)
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

pub(crate) fn configure_json_output(json: bool) {
    JSON_OUTPUT.store(json, Ordering::Relaxed);
}

pub(crate) fn print_json<T: Serialize + ?Sized>(value: &T) -> CliResult<()> {
    print_cli_value(
        &cli_command_title(),
        value,
        JSON_OUTPUT.load(Ordering::Relaxed),
    )
}

pub(crate) fn print_text(title: &str, body: &str) -> CliResult<()> {
    if JSON_OUTPUT.load(Ordering::Relaxed) {
        return print_json(&serde_json::json!({"text": body}));
    }
    if std::io::stdout().is_terminal() {
        let output = render_human_report(title, "ready", &[("Recent output".into(), body.into())]);
        std::io::stdout().lock().write_all(output.as_bytes())?;
    } else {
        std::io::stdout().lock().write_all(body.as_bytes())?;
    }
    Ok(())
}

fn print_cli_value<T: Serialize + ?Sized>(title: &str, value: &T, json: bool) -> CliResult<()> {
    if should_emit_json(json, std::io::stdout().is_terminal()) {
        println!("{}", serde_json::to_string_pretty(value)?);
        return Ok(());
    }
    let value = serde_json::to_value(value)?;
    let status = if value["success"] == false {
        "failed"
    } else {
        value["status"].as_str().unwrap_or("complete")
    };
    let rows = cli_value_rows(&value);
    let rendered = render_human_report(title, status, &rows);
    std::io::stdout().lock().write_all(rendered.as_bytes())?;
    Ok(())
}

fn should_emit_json(explicit_json: bool, stdout_is_terminal: bool) -> bool {
    explicit_json || !stdout_is_terminal
}

fn render_human_report(title: &str, status: &str, rows: &[(String, String)]) -> String {
    let width = glass_tui::crossterm::terminal::size()
        .map(|(width, _)| usize::from(width))
        .unwrap_or(80)
        .clamp(24, 120);
    let color = cli_color_enabled(
        glass_tui::theme::no_color(),
        std::env::var("TERM").ok().as_deref(),
    );
    render_human_report_at(title, status, rows, width, color)
}

fn cli_color_enabled(no_color: bool, term: Option<&str>) -> bool {
    !no_color && term != Some("dumb")
}

fn render_human_report_at(
    title: &str,
    status: &str,
    rows: &[(String, String)],
    width: usize,
    color: bool,
) -> String {
    let rows = rows
        .iter()
        .map(|(label, value)| (label.as_str(), value.clone()))
        .collect::<Vec<_>>();
    glass_tui::cli::report("Glass", title, status, &rows, width.clamp(24, 120), color)
}

fn cli_command_title() -> String {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    let command_index = args
        .iter()
        .position(|arg| matches!(arg.as_str(), "agent" | "harness" | "project" | "daemon"));
    let parts = command_index
        .into_iter()
        .flat_map(|index| args[index..].iter())
        .filter(|arg| !arg.starts_with('-'))
        .take(2)
        .map(|arg| {
            arg.split(['-', '_'])
                .filter(|part| !part.is_empty())
                .map(|part| {
                    let mut chars = part.chars();
                    chars
                        .next()
                        .map(|first| first.to_uppercase().collect::<String>() + chars.as_str())
                        .unwrap_or_default()
                })
                .collect::<Vec<_>>()
                .join(" ")
        })
        .collect::<Vec<_>>();
    if parts.is_empty() {
        "Command result".to_owned()
    } else {
        parts.join(" · ")
    }
}

fn cli_value_rows(value: &serde_json::Value) -> Vec<(String, String)> {
    const MAX_ROWS: usize = 32;
    const MAX_SCALAR_CHARS: usize = 900;

    fn bounded(value: &str) -> String {
        let mut chars = value.chars();
        let clipped = chars.by_ref().take(MAX_SCALAR_CHARS).collect::<String>();
        if chars.next().is_some() {
            format!("{clipped}… (truncated)")
        } else {
            clipped
        }
    }

    let mut rows = Vec::new();
    let Some(fields) = value.as_object() else {
        rows.push((
            "Result".into(),
            match value {
                serde_json::Value::String(text) => bounded(text),
                serde_json::Value::Null => "None".into(),
                scalar => scalar.to_string(),
            },
        ));
        return rows;
    };

    for (key, value) in fields {
        if rows.len() >= MAX_ROWS
            || matches!(
                key.as_str(),
                "status" | "success" | "socket" | "socketPath" | "statusPath" | "logPath"
            )
        {
            continue;
        }
        let label = human_cli_label(key);
        match value {
            serde_json::Value::Array(items) => {
                rows.push((
                    label.clone(),
                    if items.is_empty() {
                        "None".into()
                    } else {
                        format!("{} entries", items.len())
                    },
                ));
                for item in items.iter().take(8) {
                    if rows.len() >= MAX_ROWS {
                        break;
                    }
                    rows.push((label.clone(), human_cli_entry(item)));
                }
                if items.len() > 8 && rows.len() < MAX_ROWS {
                    rows.push((label, format!("{} more entries", items.len() - 8)));
                }
            }
            serde_json::Value::Object(nested) => {
                rows.push((label, human_cli_object_entry(nested)));
            }
            serde_json::Value::String(text) => rows.push((label, bounded(text))),
            serde_json::Value::Null => rows.push((label, "None".into())),
            scalar => rows.push((label, scalar.to_string())),
        }
    }
    if rows.is_empty() {
        rows.push(("Result".to_owned(), "No details".to_owned()));
    }
    rows
}

fn human_cli_label(key: &str) -> String {
    let label = match key {
        "project" => "Project",
        "root" => "Workspace",
        "config" => "Configuration",
        "checks" => "Checks",
        "content" => "Content",
        "success" => "Succeeded",
        "revision" => "Revision",
        "path" => "Path",
        "files" => "Files",
        "processes" => "Processes",
        "diagnostics" => "Diagnostics",
        "runtime" => "Runtime",
        "status" => "Status",
        "message" => "Message",
        "reason" => "Reason",
        "url" => "URL",
        "title" => "Title",
        "name" => "Name",
        "id" => "ID",
        "count" => "Count",
        "results" => "Results",
        "items" => "Items",
        _ => return split_cli_key(key),
    };
    label.to_owned()
}

fn split_cli_key(key: &str) -> String {
    key.split(['-', '_'])
        .flat_map(|part| {
            let mut start = 0;
            let mut words = Vec::new();
            for (index, character) in part.char_indices() {
                if character.is_ascii_uppercase() && index > start {
                    words.push(part[start..index].to_owned());
                    start = index;
                }
            }
            if start < part.len() {
                words.push(part[start..].to_owned());
            }
            words
        })
        .map(|word| {
            let mut chars = word.chars();
            chars
                .next()
                .map(|first| first.to_uppercase().collect::<String>() + chars.as_str())
                .unwrap_or_default()
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn human_cli_entry(value: &serde_json::Value) -> String {
    if let Some(text) = value.as_str() {
        return text.to_owned();
    }
    let Some(fields) = value.as_object() else {
        return value.to_string();
    };
    human_cli_object_entry(fields)
}

fn human_cli_object_entry(fields: &serde_json::Map<String, serde_json::Value>) -> String {
    let primary = ["label", "title", "name", "path", "url", "id", "root"]
        .iter()
        .find_map(|key| fields.get(*key).and_then(serde_json::Value::as_str));
    let status = fields
        .get("status")
        .and_then(serde_json::Value::as_str)
        .map(str::to_owned)
        .or_else(|| {
            fields
                .get("success")
                .and_then(serde_json::Value::as_bool)
                .map(|ok| if ok { "done" } else { "failed" }.to_owned())
        });
    match (primary, status) {
        (Some(primary), Some(status)) => format!("{primary} · {status}"),
        (Some(primary), None) => primary.to_owned(),
        (None, _) => format!("{} details", fields.len()),
    }
}

fn format_agent_status(readiness: &crate::resident_harness::ResidentReadiness) -> String {
    use crate::core_runtime::{CoreReadinessState, GLASS_CORE_PROTOCOL_VERSION};

    let status = if readiness.ready { "ready" } else { "degraded" };
    let engine = readiness
        .glass_core
        .binary
        .version
        .as_deref()
        .map(|version| format!("Glass Core {version}"))
        .unwrap_or_else(|| "Glass Core".to_owned());
    let protocol = if readiness.glass_core.protocol.state == CoreReadinessState::Ready {
        format!("JSON-RPC {GLASS_CORE_PROTOCOL_VERSION} · mixed tools negotiated")
    } else {
        match readiness.glass_core.protocol.state {
            CoreReadinessState::Missing => "engine unavailable",
            CoreReadinessState::Incompatible => "needs update",
            CoreReadinessState::Unauthenticated => "provider setup needed",
            CoreReadinessState::Unknown => "could not verify",
            CoreReadinessState::Ready => unreachable!(),
        }
        .to_owned()
    };
    let provider = match (&readiness.glass_core.provider, &readiness.glass_core.model) {
        (Some(provider), Some(model)) => format!("{provider} · {model}"),
        (Some(provider), None) => provider.clone(),
        (None, _) => match readiness.glass_core.authentication.state {
            CoreReadinessState::Ready => "connected",
            CoreReadinessState::Unauthenticated => "not connected",
            CoreReadinessState::Missing => "engine unavailable",
            CoreReadinessState::Incompatible => "needs update",
            CoreReadinessState::Unknown => "could not verify",
        }
        .to_owned(),
    };
    render_human_report(
        "Agent status",
        status,
        &[
            ("Engine".into(), engine),
            ("Runtime".into(), protocol),
            ("Provider".into(), provider),
        ],
    )
}

fn format_agent_doctor(readiness: &crate::resident_harness::ResidentReadiness) -> String {
    use crate::core_runtime::CoreReadinessState;

    fn component_row(
        label: &str,
        component: &crate::core_runtime::CoreReadinessComponent,
    ) -> (String, String) {
        let state = match component.state {
            CoreReadinessState::Ready => "available",
            CoreReadinessState::Missing => "not installed",
            CoreReadinessState::Incompatible => "needs update",
            CoreReadinessState::Unauthenticated => "provider setup needed",
            CoreReadinessState::Unknown => "could not verify",
        };
        (label.to_owned(), state.to_owned())
    }

    let status = if readiness.ready { "ready" } else { "degraded" };
    let mut rows = vec![
        component_row("Engine", &readiness.glass_core.binary),
        component_row("Runtime", &readiness.glass_core.protocol),
        component_row("Provider", &readiness.glass_core.authentication),
    ];
    if let Some(provider) = &readiness.glass_core.provider {
        let model = readiness
            .glass_core
            .model
            .as_deref()
            .unwrap_or("model unknown");
        rows.push(("Selected".into(), format!("{provider} · {model}")));
    }
    if !readiness.remediation.is_empty() {
        rows.extend(
            readiness
                .remediation
                .iter()
                .map(|step| ("Next step".into(), step.clone())),
        );
    }
    render_human_report("Agent doctor", status, &rows)
}

#[cfg(test)]
mod project_authorization_tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT_ROOT: AtomicU64 = AtomicU64::new(0);

    #[test]
    fn direct_project_mutations_require_authority_and_confirmation() {
        let root = std::env::temp_dir().join(format!(
            "glass-project-cli-auth-{}-{}",
            std::process::id(),
            NEXT_ROOT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&root).unwrap();
        let action = ProjectCommand::Mkdir {
            path: "authorized-dir".into(),
            root: root.clone(),
        };
        for (allow_mutation, confirmed) in [(false, false), (false, true), (true, false)] {
            let error = dispatch_project(&action, allow_mutation, confirmed, false)
                .expect_err("incomplete project authorization must fail");
            assert!(error.to_string().contains("--allow-mutation and --yes"));
            assert!(!root.join("authorized-dir").exists());
        }
        for action in [
            ProjectCommand::Git {
                root: root.clone(),
                action: ProjectGitCommand::Stage {
                    paths: vec!["src/main.rs".into()],
                },
            },
            ProjectCommand::Github {
                root: root.clone(),
                action: ProjectGithubCommand::Ship {
                    title: None,
                    body: None,
                    base: None,
                    draft: false,
                },
            },
        ] {
            let error = dispatch_project(&action, true, false, false)
                .expect_err("Git and GitHub writes require separate confirmation");
            assert!(error.to_string().contains("--allow-mutation and --yes"));
        }
        dispatch_project(&action, true, true, false).unwrap();
        assert!(root.join("authorized-dir").is_dir());
        std::fs::remove_dir_all(root).unwrap();
    }
}

#[cfg(test)]
mod agent_readiness_presentation_tests {
    use super::*;
    use crate::core_runtime::{CoreReadiness, CoreReadinessComponent, CoreReadinessState};
    use crate::resident_harness::{ResidentHarnessKind, ResidentReadiness};

    fn readiness(ready: bool) -> ResidentReadiness {
        let component = |state, detail: &str, version: Option<&str>| CoreReadinessComponent {
            state,
            version: version.map(str::to_owned),
            path: None,
            detail: detail.into(),
        };
        ResidentReadiness {
            selected: ResidentHarnessKind::GlassCore,
            ready,
            detail: "internal detail deliberately not shown by status".into(),
            glass_core: CoreReadiness {
                ready,
                binary: component(CoreReadinessState::Ready, "engine available", Some("1.2.3")),
                protocol: component(CoreReadinessState::Ready, "protocol negotiated", None),
                authentication: component(
                    if ready {
                        CoreReadinessState::Ready
                    } else {
                        CoreReadinessState::Unauthenticated
                    },
                    "provider configuration accepted",
                    None,
                ),
                provider: Some("fixture-provider".into()),
                model: Some("fixture-model".into()),
                session: None,
                storage_dir: Default::default(),
                remediation: if ready {
                    Vec::new()
                } else {
                    vec!["Connect a provider".into()]
                },
            },
            remediation: if ready {
                Vec::new()
            } else {
                vec!["Connect a provider".into()]
            },
        }
    }

    #[test]
    fn agent_status_is_a_concise_human_summary() {
        let rendered = format_agent_status(&readiness(true));
        assert!(rendered.contains("Agent status"));
        assert!(rendered.contains("1.2.3"));
        assert!(rendered.contains("JSON-RPC 2"));
        assert!(rendered.contains("fixture-provider · fixture-model"));
        assert!(!rendered.contains("internal detail"));
    }

    #[test]
    fn agent_doctor_shows_component_states_and_repair_steps() {
        let rendered = format_agent_doctor(&readiness(false));
        assert!(rendered.contains("Agent doctor"));
        assert!(rendered.contains("not installed") || rendered.contains("available"));
        assert!(rendered.contains("provider setup needed"));
        assert!(rendered.contains("Connect a provider"));
        assert!(!rendered.contains("internal detail"));
    }

    #[test]
    fn generic_cli_report_uses_human_labels_and_bounds_large_fields() {
        let value = serde_json::json!({
            "status": "ready",
            "project": {"root": "/workspace", "config": {"name": "fixture"}},
            "checks": [{"name": "build", "success": true}],
            "content": "x".repeat(1200),
        });
        let rows = cli_value_rows(&value);
        assert!(
            rows.iter()
                .any(|(label, value)| { label == "Project" && value == "/workspace" })
        );
        assert!(
            rows.iter()
                .any(|(label, value)| { label == "Checks" && value == "1 entries" })
        );
        assert!(
            rows.iter()
                .any(|(label, value)| { label == "Checks" && value == "build · done" })
        );
        assert!(
            rows.iter()
                .any(|(label, value)| { label == "Content" && value.ends_with("… (truncated)") })
        );
        assert!(!rows.iter().any(|(label, _)| label.contains('·')));
        assert!(!rows.iter().any(|(_, value)| value.starts_with('{')));
    }

    #[test]
    fn terminal_report_wraps_and_respects_color_settings() {
        let rows = vec![("Detail".into(), "wide content ".repeat(12))];
        let plain = render_human_report_at("Project", "complete", &rows, 24, false);
        assert!(!plain.contains('\u{1b}'));
        assert!(plain.lines().count() > 3);

        let colored = render_human_report_at("Project", "complete", &rows, 80, true);
        assert!(colored.contains('\u{1b}'));
        assert!(!cli_color_enabled(true, Some("xterm-256color")));
        assert!(!cli_color_enabled(false, Some("dumb")));
        assert!(cli_color_enabled(false, Some("xterm-256color")));
        assert!(should_emit_json(false, false));
        assert!(should_emit_json(true, true));
        assert!(!should_emit_json(false, true));
    }

    #[test]
    fn doctor_human_report_omits_machine_paths() {
        let mut readiness = readiness(false);
        readiness.glass_core.binary.detail = "/private/install/glass-core not found".into();
        readiness.glass_core.storage_dir = "/private/sessions".into();
        let rendered = format_agent_doctor(&readiness);
        assert!(!rendered.contains("/private/install"));
        assert!(!rendered.contains("/private/sessions"));
    }
}
