use super::state::{DevSurface, DevTuiState};
use crate::agents::AgentSpec;
use crate::kernels::KernelKind;
use crate::tools::DevelopmentToolContext;
use glass_browser::development::{Actor, ToolAuthorization, ToolCall};
use serde_json::{Value, json};
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_TUI_TOOL: AtomicU64 = AtomicU64::new(1);

pub fn execute(state: &mut DevTuiState, input: &str) -> Result<String, String> {
    let mut parts = input.split_whitespace();
    let Some(command) = parts.next() else {
        return Ok("Command palette closed".into());
    };
    match command {
        "help" | "?" => Ok("Routes: view · editor · lsp · agent · process · browser · workflow · debug · kernel · git · test · experiment · replay · quit. All mutations use the resident authority/revision router.".into()),
        "quit" | "q" => {
            state.quit = true;
            Ok("Closing Glass Dev".into())
        }
        "view" => {
            let name = parts.next().ok_or("view requires a surface")?;
            state.surface = parse_surface(name).ok_or("unknown surface")?;
            Ok(format!("Opened {}", state.surface.label()))
        }
        "agent" => execute_agent(state, parts.collect()),
        "editor" => execute_editor(state, parts.collect()),
        "lsp" => execute_lsp(state, parts.collect()),
        "process" => execute_process(state, parts.collect()),
        "browser" | "workflow" => execute_browser(state, command, parts.collect()),
        "debug" => execute_debug(state, parts.collect()),
        "kernel" => execute_kernel(state, parts.collect()),
        "git" => execute_git(state, parts.collect()),
        "tests" | "test" => execute_test(state, parts.collect()),
        "experiment" | "experiments" => execute_experiment(state, parts.collect()),
        "replay" => {
            state.surface = DevSurface::Replay;
            Ok("Observable replay refreshed".into())
        }
        _ if state.workspace.customization().command(command).is_some() => {
            let result = run_tool(
                state,
                &format!("glass.command.{command}"),
                json!({}),
                true,
            )?;
            Ok(compact_result(command, &result))
        }
        _ => Err(format!("unknown command {command}; use help")),
    }
}

fn execute_agent(state: &mut DevTuiState, parts: Vec<&str>) -> Result<String, String> {
    let Some(action) = parts.first().copied() else {
        state.surface = DevSurface::Agent;
        return Ok("Opened Glass Agent".into());
    };
    match action {
        "spawn" => {
            let role = parts.get(1).ok_or("agent spawn requires ROLE TASK")?;
            let task = parts.get(2..).unwrap_or_default().join(" ");
            if task.is_empty() {
                return Err("agent spawn requires ROLE TASK".into());
            }
            let id = state
                .workspace
                .agents()
                .create(AgentSpec::new(*role, task))
                .map_err(|error| error.to_string())?;
            state.surface = DevSurface::Agents;
            Ok(format!("Spawned {}", id.as_str()))
        }
        "prompt" | "steer" | "follow-up" => {
            let id = parts.get(1).ok_or("agent action requires ID TEXT")?;
            let text = parts.get(2..).unwrap_or_default().join(" ");
            let agent = find_agent(state, id)?;
            match action {
                "prompt" => state.workspace.agents().prompt(&agent, text),
                "steer" => state.workspace.agents().steer(&agent, text),
                _ => state.workspace.agents().follow_up(&agent, text),
            }
            .map_err(|error| error.to_string())?;
            state.surface = DevSurface::Agent;
            Ok(format!("Queued {action} for {id}"))
        }
        "cancel" => {
            let id = parts.get(1).ok_or("agent cancel requires ID")?;
            let agent = find_agent(state, id)?;
            state
                .workspace
                .agents()
                .cancel(&agent)
                .map_err(|error| error.to_string())?;
            Ok(format!("Cancelled {id}"))
        }
        "compact" => agent_control(state, &parts, "glass.agent.compact", json!({})),
        "model" => {
            let provider = parts.get(2).ok_or("agent model requires ID PROVIDER MODEL")?;
            let model = parts.get(3).ok_or("agent model requires ID PROVIDER MODEL")?;
            agent_control(state, &parts, "glass.agent.model", json!({"provider":provider,"modelId":model}))
        }
        "thinking" => {
            let level = parts.get(2).ok_or("agent thinking requires ID LEVEL")?;
            agent_control(state, &parts, "glass.agent.thinking", json!({"level":level}))
        }
        "new" => agent_control(state, &parts, "glass.agent.new-session", json!({})),
        "clone" => agent_control(state, &parts, "glass.agent.clone-session", json!({})),
        "fork" => {
            let entry = parts.get(2).ok_or("agent fork requires ID ENTRY")?;
            agent_control(state, &parts, "glass.agent.fork", json!({"entryId":entry}))
        }
        "messages" => agent_control(state, &parts, "glass.agent.messages", json!({})),
        "entries" => agent_control(state, &parts, "glass.agent.entries", json!({})),
        "stats" => agent_control(state, &parts, "glass.agent.stats", json!({})),
        _ => Err("agent actions: spawn, prompt, steer, follow-up, cancel, compact, model, thinking, new, clone, fork, messages, entries, stats".into()),
    }
}

fn agent_control(
    state: &mut DevTuiState,
    parts: &[&str],
    tool: &str,
    mut arguments: Value,
) -> Result<String, String> {
    let id = parts.get(1).ok_or("agent action requires ID")?;
    arguments["agentId"] = Value::String((*id).into());
    let result = run_tool(state, tool, arguments, true)?;
    state.surface = DevSurface::Agent;
    Ok(compact_result(tool, &result))
}

fn execute_editor(state: &mut DevTuiState, parts: Vec<&str>) -> Result<String, String> {
    let Some(action) = parts.first().copied() else {
        state.surface = DevSurface::Editor;
        return Ok("Opened shared editor".into());
    };
    let path = parts.get(1).ok_or("editor action requires PATH")?;
    let (tool, arguments, mutating) = match action {
        "open" => ("glass.editor.open", json!({"path":path}), true),
        "save" => ("glass.editor.save", json!({"path":path}), true),
        "selection" => ("glass.editor.selection", json!({"path":path}), false),
        "replace" => {
            let old = parts.get(2).ok_or("editor replace requires PATH OLD NEW")?;
            let new = parts.get(3..).unwrap_or_default().join(" ");
            (
                "glass.editor.replace",
                json!({"path":path,"oldText":old,"newText":new}),
                true,
            )
        }
        _ => return Err("editor actions: open, selection, replace, save".into()),
    };
    let result = run_tool(state, tool, arguments, mutating)?;
    state.surface = DevSurface::Editor;
    Ok(compact_result(tool, &result))
}

fn execute_lsp(state: &mut DevTuiState, parts: Vec<&str>) -> Result<String, String> {
    let action = parts.first().copied().unwrap_or("list");
    let server = parts.get(1).copied();
    let (tool, arguments, mutating) = match action {
        "start" => ("glass.lsp.start", json!({"server":server.ok_or("lsp start requires SERVER COMMAND")?,"command":parts.get(2).ok_or("lsp start requires SERVER COMMAND")?,"arguments":parts.get(3..).unwrap_or_default()}), true),
        "stop" => ("glass.lsp.stop", json!({"server":server.ok_or("lsp stop requires SERVER")?}), true),
        "list" => ("glass.lsp.list", json!({}), false),
        "events" => ("glass.lsp.events", json!({"since":parts.get(1).and_then(|value| value.parse::<u64>().ok()).unwrap_or(0)}), false),
        "diagnostics" | "symbols" | "format" | "tokens" => {
            let path = parts.get(2).ok_or("lsp action requires SERVER PATH")?;
            let tool = match action {
                "diagnostics" => "glass.lsp.diagnostics",
                "symbols" => "glass.lsp.document_symbols",
                "format" => "glass.lsp.formatting",
                _ => "glass.lsp.semantic_tokens",
            };
            (tool, json!({"server":server.ok_or("missing SERVER")?,"path":path}), false)
        }
        "workspace-symbols" => ("glass.lsp.workspace_symbols", json!({"server":server.ok_or("lsp workspace-symbols requires SERVER QUERY")?,"query":parts.get(2..).unwrap_or_default().join(" ")}), false),
        "hover" | "complete" | "definition" | "declaration" | "implementation" | "references" | "signature" => {
            let tool = match action {
                "hover" => "glass.lsp.hover",
                "complete" => "glass.lsp.completion",
                "definition" => "glass.lsp.definition",
                "declaration" => "glass.lsp.declaration",
                "implementation" => "glass.lsp.implementation",
                "references" => "glass.lsp.references",
                _ => "glass.lsp.signature_help",
            };
            (tool, json!({"server":server.ok_or("lsp position action requires SERVER PATH LINE CHARACTER")?,"path":parts.get(2).ok_or("missing PATH")?,"line":parse_u64(parts.get(3), "LINE")?,"character":parse_u64(parts.get(4), "CHARACTER")?}), false)
        }
        "rename" => ("glass.lsp.rename", json!({"server":server.ok_or("lsp rename requires SERVER PATH LINE CHARACTER NAME")?,"path":parts.get(2).ok_or("missing PATH")?,"line":parse_u64(parts.get(3), "LINE")?,"character":parse_u64(parts.get(4), "CHARACTER")?,"newName":parts.get(5).ok_or("missing NAME")?}), false),
        _ => return Err("lsp actions: start, stop, list, events, diagnostics, hover, complete, definition, declaration, implementation, references, symbols, workspace-symbols, signature, format, tokens, rename".into()),
    };
    let result = run_tool(state, tool, arguments, mutating)?;
    state.editor = serde_json::to_string_pretty(&result).unwrap_or_else(|_| result.to_string());
    state.surface = DevSurface::Editor;
    Ok(compact_result(tool, &result))
}

fn execute_process(state: &mut DevTuiState, parts: Vec<&str>) -> Result<String, String> {
    let Some(action) = parts.first().copied() else {
        state.surface = DevSurface::Processes;
        return Ok("Opened processes".into());
    };
    let name = parts.get(1).ok_or("process action requires NAME")?;
    let (tool, arguments, mutating) = match action {
        "start" => (
            "glass.process.start",
            json!({"name":name,"command":parts.get(2..).unwrap_or_default().join(" ")}),
            true,
        ),
        "stop" => ("glass.process.stop", json!({"name":name}), true),
        "restart" => ("glass.process.restart", json!({"name":name}), true),
        "logs" => ("glass.process.logs", json!({"name":name}), false),
        "health" => ("glass.process.health", json!({"name":name}), false),
        "input" => (
            "glass.process.input",
            json!({"name":name,"input":parts.get(2..).unwrap_or_default().join(" ")}),
            true,
        ),
        "resize" => (
            "glass.process.resize",
            json!({"name":name,"cols":parse_u64(parts.get(2), "COLS")?,"rows":parse_u64(parts.get(3), "ROWS")?}),
            true,
        ),
        "ports" => ("glass.process.ports", json!({}), false),
        _ => {
            return Err(
                "process actions: start, stop, restart, logs, input, resize, health, ports".into(),
            );
        }
    };
    let result = run_tool(state, tool, arguments, mutating)?;
    state.surface = DevSurface::Processes;
    Ok(compact_result(tool, &result))
}

fn execute_browser(
    state: &mut DevTuiState,
    command: &str,
    parts: Vec<&str>,
) -> Result<String, String> {
    let Some(action) = parts.first().copied() else {
        state.surface = DevSurface::Browser;
        return Ok("Opened resident browser".into());
    };
    let (tool, arguments, mutating) = if command == "workflow" {
        match action {
            "run" => ("glass.workflow.run", json!({"definition":read_project_json(state, parts.get(1).ok_or("workflow run requires DEFINITION.json")?)?,"inputs":parse_inline_json(parts.get(2), json!({}))?}), true),
            "pause" => ("glass.workflow.pause", json!({}), true),
            "resume" => ("glass.workflow.resume", json!({"definition":read_project_json(state, parts.get(1).ok_or("workflow resume requires DEFINITION.json CHECKPOINT.json")?)?,"checkpoint":read_project_json(state, parts.get(2).ok_or("workflow resume requires DEFINITION.json CHECKPOINT.json")?)?,"inputs":parse_inline_json(parts.get(3), json!({}))?}), true),
            _ => return Err("workflow actions: run DEFINITION.json [INPUTS_JSON], pause, resume DEFINITION.json CHECKPOINT.json [INPUTS_JSON]".into()),
        }
    } else {
        match action {
            "start" => ("glass.browser.start", json!({"port":parts.get(1).and_then(|value| value.parse::<u16>().ok()).unwrap_or(9222),"incognito":true,"chromePath":parts.get(2)}), true),
            "stop" => ("glass.browser.stop", json!({}), true),
            "state" => ("glass.browser.state", json!({}), false),
            "observe" => ("glass.browser.observe", json!({}), false),
            "targets" => ("glass.browser.targets", json!({}), false),
            "select" => ("glass.browser.target.select", json!({"targetId":parts.get(1).ok_or("browser select requires TARGET_ID")?}), true),
            "navigate" => ("glass.browser.navigate", json!({"url":parts.get(1).ok_or("browser navigate requires URL REVISION")?,"browserRevision":parse_u64(parts.get(2), "REVISION")?}), true),
            "click" => ("glass.browser.act", json!({"action":"click","target":parts.get(1).ok_or("browser click requires TARGET REVISION")?,"browserRevision":parse_u64(parts.get(2), "REVISION")?}), true),
            "type" => ("glass.browser.act", json!({"action":"type","target":parts.get(1).ok_or("browser type requires TARGET REVISION TEXT")?,"browserRevision":parse_u64(parts.get(2), "REVISION")?,"text":parts.get(3..).unwrap_or_default().join(" ")}), true),
            "scroll" => ("glass.browser.act", json!({"action":"scroll","dx":parts.get(1).and_then(|value| value.parse::<f64>().ok()).unwrap_or(0.0),"dy":parts.get(2).and_then(|value| value.parse::<f64>().ok()).unwrap_or(600.0),"browserRevision":parse_u64(parts.get(3), "REVISION")?}), true),
            "screenshot" => ("glass.browser.screenshot", json!({}), false),
            _ => return Err("browser actions: start, stop, state, observe, targets, select, navigate, click, type, scroll, screenshot".into()),
        }
    };
    let result = run_tool(state, tool, arguments, mutating)?;
    state.browser_detail =
        serde_json::to_string_pretty(&result).unwrap_or_else(|_| result.to_string());
    state.surface = DevSurface::Browser;
    Ok(compact_result(tool, &result))
}

fn read_project_json(state: &mut DevTuiState, path: &str) -> Result<Value, String> {
    let content = state
        .workspace
        .project_mut()
        .read_file(path)
        .map_err(|error| error.to_string())?;
    serde_json::from_str(&content).map_err(|error| format!("invalid JSON in {path}: {error}"))
}

fn parse_inline_json(value: Option<&&str>, default: Value) -> Result<Value, String> {
    value
        .map(|value| {
            serde_json::from_str(value).map_err(|error| format!("invalid inline JSON: {error}"))
        })
        .unwrap_or(Ok(default))
}

fn find_agent(state: &mut DevTuiState, id: &str) -> Result<crate::AgentId, String> {
    state
        .workspace
        .agents()
        .list()
        .map_err(|error| error.to_string())?
        .into_iter()
        .find(|agent| agent.id.as_str() == id)
        .map(|agent| agent.id)
        .ok_or_else(|| "unknown agent".into())
}

fn execute_debug(state: &mut DevTuiState, parts: Vec<&str>) -> Result<String, String> {
    let result = match parts.first().copied() {
        None => {
            state.surface = DevSurface::Debugger;
            return Ok("Opened debugger".into());
        }
        Some("start") => {
            let name = parts.get(1).ok_or("debug start requires NAME COMMAND")?;
            let command = parts.get(2).ok_or("debug start requires NAME COMMAND")?;
            let arguments = parts
                .get(3..)
                .unwrap_or_default()
                .iter()
                .map(|value| (*value).to_string())
                .collect::<Vec<_>>();
            run_tool(
                state,
                "glass.debug.start",
                json!({"session":name,"command":command,"arguments":arguments}),
                true,
            )?
        }
        Some(action) => {
            let session = parts.get(1).ok_or("debug action requires SESSION")?;
            let (tool, arguments, mutating) = match action {
                "launch" | "attach" => {
                    let configuration = serde_json::from_str::<Value>(&parts.get(2..).unwrap_or_default().join(" ")).map_err(|error| format!("debug {action} requires a JSON configuration: {error}"))?;
                    (if action == "launch" { "glass.debug.launch" } else { "glass.debug.attach" }, json!({"session":session,"configuration":configuration}), true)
                }
                "configured" => ("glass.debug.configuration_done", json!({"session":session}), true),
                "break" => {
                    let lines = parts.get(3..).unwrap_or_default().iter().map(|line| line.parse::<u64>().map_err(|_| "breakpoint lines must be integers")).collect::<Result<Vec<_>, _>>()?;
                    ("glass.debug.breakpoint.set", json!({"session":session,"path":parts.get(2).ok_or("debug break requires SESSION PATH LINES...")?,"lines":lines}), true)
                }
                "continue" | "pause" => (if action == "continue" { "glass.debug.continue" } else { "glass.debug.pause" }, json!({"session":session,"threadId":parse_u64(parts.get(2), "THREAD_ID")?}), true),
                "step" => ("glass.debug.step", json!({"session":session,"threadId":parse_u64(parts.get(2), "THREAD_ID")?,"kind":parts.get(3).ok_or("debug step requires SESSION THREAD_ID over|in|out")?}), true),
                "threads" => ("glass.debug.threads", json!({"session":session}), false),
                "stack" => ("glass.debug.stack", json!({"session":session,"threadId":parse_u64(parts.get(2), "THREAD_ID")?}), false),
                "scopes" => ("glass.debug.scopes", json!({"session":session,"frameId":parse_u64(parts.get(2), "FRAME_ID")?}), false),
                "variables" => ("glass.debug.variables", json!({"session":session,"variablesReference":parse_u64(parts.get(2), "VARIABLES_REFERENCE")?}), false),
                "evaluate" => ("glass.debug.evaluate", json!({"session":session,"frameId":parse_u64(parts.get(2), "FRAME_ID")?,"expression":parts.get(3..).unwrap_or_default().join(" "),"context":"repl"}), false),
                "events" => ("glass.debug.events", json!({"session":session}), false),
                "stop" => ("glass.debug.stop", json!({"session":session}), true),
                _ => return Err("debug actions: start, launch, attach, configured, break, continue, pause, step, threads, stack, scopes, variables, evaluate, events, stop".into()),
            };
            run_tool(state, tool, arguments, mutating)?
        }
    };
    state.debugger = serde_json::to_string_pretty(&result).unwrap_or_else(|_| result.to_string());
    state.surface = DevSurface::Debugger;
    Ok(compact_result("debug", &result))
}

fn execute_kernel(state: &mut DevTuiState, parts: Vec<&str>) -> Result<String, String> {
    let action = parts
        .first()
        .copied()
        .ok_or("kernel requires start, reset, or stop")?;
    let name = parts.get(1).ok_or("kernel action requires NAME")?;
    match action {
        "start" => {
            let kind = match parts.get(2).copied().ok_or("kernel start requires KIND")? {
                "python" => KernelKind::Python,
                "javascript" => KernelKind::JavaScript,
                "shell" => KernelKind::Shell,
                "sql" => KernelKind::Sql,
                _ => return Err("kernel kind must be python, javascript, shell, or sql".into()),
            };
            state
                .workspace
                .kernels_mut()
                .start(name, kind, "human")
                .map_err(|error| error.to_string())?;
        }
        "reset" => state
            .workspace
            .kernels_mut()
            .reset(name, "human")
            .map_err(|error| error.to_string())?,
        "stop" => {
            state
                .workspace
                .kernels_mut()
                .stop(name)
                .map_err(|error| error.to_string())?;
        }
        _ => return Err("kernel actions: start, reset, stop".into()),
    }
    Ok(format!("Kernel {name}: {action}"))
}

fn execute_git(state: &mut DevTuiState, parts: Vec<&str>) -> Result<String, String> {
    let Some(action) = parts.first().copied() else {
        state.surface = DevSurface::Git;
        return Ok("Git status refreshed".into());
    };
    let (tool, arguments, mutating) = match action {
        "status" => ("glass.git.status", json!({}), false),
        "diff" => (
            "glass.git.diff",
            json!({"staged":parts.get(1) == Some(&"staged")}),
            false,
        ),
        "stage" => (
            "glass.git.stage",
            json!({"paths":parts.get(1..).unwrap_or_default()}),
            true,
        ),
        "unstage" => (
            "glass.git.unstage",
            json!({"paths":parts.get(1..).unwrap_or_default()}),
            true,
        ),
        "commit" => (
            "glass.git.commit",
            json!({"message":parts.get(1..).unwrap_or_default().join(" ")}),
            true,
        ),
        "branches" => ("glass.git.branches", json!({}), false),
        "switch" => (
            "glass.git.branch.switch",
            json!({"name":parts.get(1).ok_or("git switch requires BRANCH")?}),
            true,
        ),
        _ => {
            return Err(
                "git actions: status, diff, stage, unstage, commit, branches, switch".into(),
            );
        }
    };
    let result = run_tool(state, tool, arguments, mutating)?;
    state.git = serde_json::to_string_pretty(&result).unwrap_or_else(|_| result.to_string());
    state.surface = DevSurface::Git;
    Ok(compact_result(tool, &result))
}

fn execute_test(state: &mut DevTuiState, parts: Vec<&str>) -> Result<String, String> {
    let Some(action) = parts.first().copied() else {
        state.surface = DevSurface::Tests;
        return Ok("Test results refreshed".into());
    };
    let (tool, arguments, mutating) = match action {
        "discover" | "list" => ("glass.test.discover", json!({}), false),
        "results" => ("glass.test.results", json!({}), false),
        "run" => (
            "glass.test.run",
            json!({"runId":parts.get(1).ok_or("test run requires RUN_ID SUITE_ID")?,"suiteId":parts.get(2).ok_or("test run requires RUN_ID SUITE_ID")?}),
            true,
        ),
        "cancel" => (
            "glass.test.cancel",
            json!({"runId":parts.get(1).ok_or("test cancel requires RUN_ID")?}),
            true,
        ),
        _ => return Err("test actions: discover, run, results, cancel".into()),
    };
    let result = run_tool(state, tool, arguments, mutating)?;
    state.tests = serde_json::to_string_pretty(&result).unwrap_or_else(|_| result.to_string());
    state.surface = DevSurface::Tests;
    Ok(compact_result(tool, &result))
}

fn execute_experiment(state: &mut DevTuiState, parts: Vec<&str>) -> Result<String, String> {
    let Some(action) = parts.first().copied() else {
        state.surface = DevSurface::Experiments;
        return Ok("Opened experiments".into());
    };
    if state.experiment_manager.is_none() {
        let root = state.workspace.root();
        let repository_name = root
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or("workspace");
        let worktrees = root
            .parent()
            .unwrap_or(root)
            .join(format!(".glass-{repository_name}-experiments"));
        state.experiment_manager = Some(
            crate::ExperimentManager::new(root, worktrees).map_err(|error| error.to_string())?,
        );
    }
    let experiments = state
        .experiment_manager
        .as_mut()
        .expect("initialized above");
    let message = match action {
        "create" => {
            let id = parts
                .get(1)
                .ok_or("experiment create requires ID BRANCH [PORT]")?;
            let branch = parts
                .get(2)
                .ok_or("experiment create requires ID BRANCH [PORT]")?;
            let port = parts
                .get(3)
                .map(|value| value.parse::<u16>().map_err(|_| "invalid PORT"))
                .transpose()?;
            let snapshot = experiments
                .create(id, branch, port)
                .map_err(|error| error.to_string())?;
            format!(
                "Created experiment {} at {}",
                snapshot.id,
                snapshot.worktree.display()
            )
        }
        "compare" => {
            state.experiment_comparison = Some(experiments.compare());
            "Compared experiment evidence".into()
        }
        "select" => {
            let id = parts.get(1).ok_or("experiment select requires ID")?;
            experiments.select(id).map_err(|error| error.to_string())?;
            format!("Selected experiment {id}")
        }
        "remove" => {
            let id = parts.get(1).ok_or("experiment remove requires ID")?;
            experiments
                .remove(id, false)
                .map_err(|error| error.to_string())?;
            format!("Removed experiment {id}")
        }
        _ => return Err("experiment actions: create, compare, select, remove".into()),
    };
    state.surface = DevSurface::Experiments;
    Ok(message)
}

fn run_tool(
    state: &mut DevTuiState,
    name: &str,
    arguments: Value,
    mutating: bool,
) -> Result<Value, String> {
    let context = DevelopmentToolContext {
        authorization: ToolAuthorization {
            actor: Actor::local(),
            allow_mutation: mutating,
            confirmed: mutating,
        },
        expected_generation: state.workspace.generation(),
        expected_project_revision: state.workspace.project().revision(),
    };
    let call = ToolCall {
        id: format!("tui-{}", NEXT_TUI_TOOL.fetch_add(1, Ordering::Relaxed)),
        name: name.into(),
        arguments,
    };
    state
        .workspace
        .execute_tool(&call, &context)
        .map_err(|error| error.to_string())
}

fn compact_result(tool: &str, value: &Value) -> String {
    let text = serde_json::to_string(value).unwrap_or_else(|_| "result unavailable".into());
    let preview = text.chars().take(180).collect::<String>();
    format!("{tool}: {preview}")
}

fn parse_u64(value: Option<&&str>, label: &str) -> Result<u64, String> {
    value
        .ok_or_else(|| format!("missing {label}"))?
        .parse::<u64>()
        .map_err(|_| format!("invalid {label}"))
}

fn parse_surface(name: &str) -> Option<DevSurface> {
    DevSurface::ALL
        .into_iter()
        .find(|surface| surface.label().eq_ignore_ascii_case(name))
        .or(match name {
            "home" => Some(DevSurface::Dashboard),
            "agent" => Some(DevSurface::Agent),
            "process" => Some(DevSurface::Processes),
            "debug" => Some(DevSurface::Debugger),
            "test" => Some(DevSurface::Tests),
            "experiment" => Some(DevSurface::Experiments),
            "browser" => Some(DevSurface::Browser),
            _ => None,
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_major_surface_has_a_palette_route() {
        for surface in DevSurface::ALL {
            assert_eq!(parse_surface(surface.label()), Some(surface));
        }
    }
}
