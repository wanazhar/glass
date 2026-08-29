use super::projection;
use super::state::{DevSurface, DevTuiState};
use crate::agents::AgentSpec;
use crate::development::{
    Actor, SemanticBreakpoint, SemanticSnapshot, ToolAuthorization, ToolCall,
};
use crate::tasks::TaskSpec;
use crate::tools::DevelopmentToolContext;
use serde_json::{Value, json};
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_TUI_TOOL: AtomicU64 = AtomicU64::new(1);
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SurfaceAction {
    pub label: &'static str,
    pub command: &'static str,
    pub key: &'static str,
    pub description: &'static str,
}

const AGENT_ACTIONS: &[SurfaceAction] = &[
    SurfaceAction {
        label: "Compose message",
        command: "i",
        key: ":",
        description: "ask the resident Glass Agent",
    },
    SurfaceAction {
        label: "Setup Pi runtime",
        command: "agent setup",
        key: ":",
        description: "install or repair the pinned SDK",
    },
    SurfaceAction {
        label: "Update Pi runtime",
        command: "agent update",
        key: ":",
        description: "refresh the pinned managed SDK",
    },
    SurfaceAction {
        label: "Authenticate",
        command: "agent setup login",
        key: ":",
        description: "open Pi's provider login",
    },
    SurfaceAction {
        label: "Readiness doctor",
        command: "agent doctor",
        key: ":",
        description: "check Node, SDK, and auth",
    },
    SurfaceAction {
        label: "New conversation",
        command: "agent new",
        key: ":",
        description: "start a separate resident session",
    },
    SurfaceAction {
        label: "List Pi sessions",
        command: "agent sessions",
        key: ":",
        description: "pick a persisted conversation",
    },
    SurfaceAction {
        label: "Inspect Pi session tree",
        command: "agent tree",
        key: ":",
        description: "inspect branchable conversation entries",
    },
    SurfaceAction {
        label: "Compact conversation",
        command: "agent compact",
        key: ":",
        description: "summarize the selected Pi session in place",
    },
    SurfaceAction {
        label: "Rewind Pi session",
        command: "agent rewind ENTRY_ID",
        key: ":",
        description: "branch the selected agent from an earlier entry",
    },
    SurfaceAction {
        label: "Task loop",
        command: "task list",
        key: ":",
        description: "inspect autonomous tasks and verification state",
    },
    SurfaceAction {
        label: "Create task",
        command: "task create TITLE PROMPT",
        key: ":",
        description: "queue a verified development task",
    },
    SurfaceAction {
        label: "Resume task",
        command: "task resume TASK_ID",
        key: ":",
        description: "resume a paused or blocked task",
    },
    SurfaceAction {
        label: "GitHub status",
        command: "github status",
        key: ":",
        description: "check the GitHub origin and gh authentication",
    },
    SurfaceAction {
        label: "Review workspace changes",
        command: "review",
        key: ":",
        description: "open the shippable review object",
    },
    SurfaceAction {
        label: "List external harnesses",
        command: "harness list",
        key: ":",
        description: "see coding harnesses available on PATH",
    },
    SurfaceAction {
        label: "Launch external harness",
        command: "harness start NAME",
        key: ":",
        description: "hand this terminal to an installed coding harness",
    },
    SurfaceAction {
        label: "Plan mode",
        command: "plan mode",
        key: ":",
        description: "inspect-only plan, then accept to implement",
    },
    SurfaceAction {
        label: "Delegate to external harness",
        command: "harness delegate NAME PROMPT",
        key: ":",
        description: "request a bounded read-only second opinion with Glass approval",
    },
];

const CODE_ACTIONS: &[SurfaceAction] = &[
    SurfaceAction {
        label: "Open selected file full-screen",
        command: "Enter",
        key: "Enter",
        description: "open the selected project file in the focused editor",
    },
    SurfaceAction {
        label: "Edit focused file",
        command: "i",
        key: ":",
        description: "open the focused buffer in the full-screen editor",
    },
    SurfaceAction {
        label: "Replace selection",
        command: "editor replace-selection",
        key: ":",
        description: "replace the highlighted editor text directly",
    },
    SurfaceAction {
        label: "Comment selection",
        command: "editor comment-selection",
        key: ":",
        description: "anchor a review comment to the current selection",
    },
    SurfaceAction {
        label: "Save buffer",
        command: "Ctrl-S",
        key: "Ctrl-S",
        description: "write the focused buffer",
    },
    SurfaceAction {
        label: "Jump to App",
        command: "app page",
        key: ":",
        description: "open the detected app at this handler",
    },
    SurfaceAction {
        label: "Propose edit",
        command: "editor propose PATH SUMMARY TEXT",
        key: ":",
        description: "stage an agent or human edit for approval",
    },
    SurfaceAction {
        label: "Review proposals",
        command: "editor proposals",
        key: ":",
        description: "inspect pending, accepted, and stale edits",
    },
    SurfaceAction {
        label: "Create checkpoint",
        command: "editor checkpoint NAME",
        key: ":",
        description: "save open buffers as an undoable checkpoint",
    },
    SurfaceAction {
        label: "Diagnostics",
        command: "lsp diagnostics",
        key: ":",
        description: "refresh language-server diagnostics",
    },
];

const APP_ACTIONS: &[SurfaceAction] = &[
    SurfaceAction {
        label: "Start browser",
        command: "browser start",
        key: ":",
        description: "launch or attach the browser",
    },
    SurfaceAction {
        label: "Observe page",
        command: "browser observe",
        key: ":",
        description: "refresh semantic page state",
    },
    SurfaceAction {
        label: "Navigate",
        command: "browser navigate URL",
        key: ":",
        description: "enter a page URL",
    },
    SurfaceAction {
        label: "Type into selected",
        command: "browser type TARGET TEXT",
        key: ":",
        description: "type into the selected entity",
    },
    SurfaceAction {
        label: "Targets",
        command: "browser targets",
        key: ":",
        description: "inspect and select browser pages",
    },
    SurfaceAction {
        label: "Live browser view",
        command: "browser view",
        key: ":",
        description: "request native or ANSI browser pixels",
    },
    SurfaceAction {
        label: "Jump to source",
        command: "app source",
        key: ":",
        description: "open the selected entity's handler",
    },
    SurfaceAction {
        label: "Open detected app",
        command: "app open",
        key: ":",
        description: "navigate to the detected loopback URL",
    },
    SurfaceAction {
        label: "Record click path",
        command: "workflow record start",
        key: ":",
        description: "capture App activations as a reviewable workflow draft",
    },
];

const TERMINAL_ACTIONS: &[SurfaceAction] = &[
    SurfaceAction {
        label: "Start detected suite",
        command: "process start dev",
        key: ":",
        description: "queue the project-detected development command",
    },
    SurfaceAction {
        label: "Start dev server",
        command: "process start dev COMMAND",
        key: ":",
        description: "run a governed project command",
    },
    SurfaceAction {
        label: "View logs",
        command: "process logs NAME",
        key: ":",
        description: "inspect a managed process",
    },
    SurfaceAction {
        label: "Restart process",
        command: "process restart NAME",
        key: ":",
        description: "restart the selected managed process",
    },
    SurfaceAction {
        label: "Stop process",
        command: "process stop NAME",
        key: ":",
        description: "stop a managed process",
    },
    SurfaceAction {
        label: "Attach detected URL",
        command: "app open",
        key: ":",
        description: "open the process loopback URL in App",
    },
];

const TASK_ACTIONS: &[SurfaceAction] = &[
    SurfaceAction {
        label: "Create task",
        command: "task create TITLE PROMPT",
        key: ":",
        description: "queue a verified task",
    },
    SurfaceAction {
        label: "Overnight crew",
        command: "task crew GOAL",
        key: ":",
        description: "queue architect, isolated implementers, testers, reviewer, and browser",
    },
    SurfaceAction {
        label: "Cancel task",
        command: "task cancel TASK_ID",
        key: ":",
        description: "cancel a queued or running task",
    },
    SurfaceAction {
        label: "Retry task",
        command: "task retry TASK_ID",
        key: ":",
        description: "retry a failed task",
    },
];

const GIT_ACTIONS: &[SurfaceAction] = &[
    SurfaceAction {
        label: "Stage all changes",
        command: "git stage .",
        key: ":",
        description: "stage the current project diff",
    },
    SurfaceAction {
        label: "Commit",
        command: "git commit MESSAGE",
        key: ":",
        description: "create a governed commit",
    },
    SurfaceAction {
        label: "View selected file diff",
        command: "git diff",
        key: ":",
        description: "open the focused file diff",
    },
    SurfaceAction {
        label: "Branches",
        command: "git branches",
        key: ":",
        description: "list project branches",
    },
    SurfaceAction {
        label: "Review pull request",
        command: "github review",
        key: ":",
        description: "inspect the current branch PR and checks",
    },
    SurfaceAction {
        label: "Ship pull request",
        command: "github ship TITLE",
        key: ":",
        description: "create a PR after one-use confirmation",
    },
    SurfaceAction {
        label: "Fetch and pull",
        command: "git pull",
        key: ":",
        description: "fetch/merge the upstream branch through Glass Git",
    },
    SurfaceAction {
        label: "Push branch",
        command: "git push",
        key: ":",
        description: "push the current branch through Glass Git",
    },
    SurfaceAction {
        label: "Merge branch",
        command: "git merge BRANCH",
        key: ":",
        description: "merge a branch into HEAD through Glass Git",
    },
    SurfaceAction {
        label: "Rebase onto",
        command: "git rebase ONTO",
        key: ":",
        description: "rebase HEAD onto another branch through Glass Git",
    },
    SurfaceAction {
        label: "Review object",
        command: "review",
        key: ":",
        description: "proposals, last verify, and ship",
    },
];

const DEBUG_ACTIONS: &[SurfaceAction] = &[
    SurfaceAction {
        label: "Start debug session",
        command: "debug start NAME COMMAND",
        key: ":",
        description: "launch a configured debugger",
    },
    SurfaceAction {
        label: "Refresh threads",
        command: "debug threads SESSION",
        key: ":",
        description: "list DAP threads for the selected session",
    },
    SurfaceAction {
        label: "Continue",
        command: "debug continue SESSION THREAD_ID",
        key: ":",
        description: "continue the selected paused thread",
    },
    SurfaceAction {
        label: "Step over",
        command: "debug step SESSION THREAD_ID over",
        key: ":",
        description: "step over the selected thread",
    },
    SurfaceAction {
        label: "Run tests",
        command: "test run RUN_ID SUITE_ID",
        key: ":",
        description: "run a discovered test suite",
    },
    SurfaceAction {
        label: "Test results",
        command: "test results",
        key: ":",
        description: "inspect the latest test result",
    },
];

const MORE_ACTIONS: &[SurfaceAction] = &[
    SurfaceAction {
        label: "Start detected suite",
        command: "process start dev",
        key: ":",
        description: "queue the project-detected development command",
    },
    SurfaceAction {
        label: "Experiments",
        command: "experiment create ID BRANCH",
        key: ":",
        description: "create a replayable experiment",
    },
    SurfaceAction {
        label: "Kernels",
        command: "kernel start NAME KIND",
        key: ":",
        description: "start a development kernel",
    },
    SurfaceAction {
        label: "Workspace state",
        command: "workspace",
        key: ":",
        description: "inspect resident workspace services",
    },
    SurfaceAction {
        label: "Start private cockpit",
        command: "cockpit start",
        key: ":",
        description: "open the loopback development cockpit",
    },
    SurfaceAction {
        label: "Cockpit status",
        command: "cockpit status",
        key: ":",
        description: "show the private cockpit URL",
    },
];

const TRUST_ACTIONS: &[SurfaceAction] = &[
    SurfaceAction {
        label: "Inspect configuration",
        command: "I",
        key: "I",
        description: "review executable project settings",
    },
    SurfaceAction {
        label: "Open read-only workspace",
        command: "O",
        key: "O",
        description: "continue with read-only project authority",
    },
    SurfaceAction {
        label: "Trust once",
        command: "1",
        key: "1",
        description: "allow this process only",
    },
    SurfaceAction {
        label: "Trust project",
        command: "T",
        key: "T",
        description: "remember project authority",
    },
];

pub fn surface_actions(surface: DevSurface) -> &'static [SurfaceAction] {
    match surface {
        DevSurface::Trust => TRUST_ACTIONS,
        DevSurface::Agent => AGENT_ACTIONS,
        DevSurface::Code => CODE_ACTIONS,
        DevSurface::App => APP_ACTIONS,
        DevSurface::Terminal => TERMINAL_ACTIONS,
        DevSurface::Tasks => TASK_ACTIONS,
        DevSurface::Git => GIT_ACTIONS,
        DevSurface::Debug => DEBUG_ACTIONS,
        DevSurface::More => MORE_ACTIONS,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CommandGroup {
    pub label: &'static str,
    pub roots: &'static [&'static str],
    pub example: &'static str,
}

const AGENT_ROOTS: &[&str] = &["agent", "harness"];
const BUILD_ROOTS: &[&str] = &["project", "editor", "github", "lsp", "git"];
const RUN_ROOTS: &[&str] = &["browser", "workflow", "process"];
const VERIFY_ROOTS: &[&str] = &["task", "test", "debug", "review"];
const WORKSPACE_ROOTS: &[&str] = &["cockpit", "workspace", "daemon", "kernel", "experiment"];
const INSPECT_ROOTS: &[&str] = &["replay", "memory", "surface", "tool"];
const CORE_ROOTS: &[&str] = &["trust", "view", "help", "quit"];

pub static COMMAND_GROUPS: &[CommandGroup] = &[
    CommandGroup {
        label: "Agent",
        roots: AGENT_ROOTS,
        example: "agent prompt TEXT",
    },
    CommandGroup {
        label: "Build",
        roots: BUILD_ROOTS,
        example: "project search QUERY",
    },
    CommandGroup {
        label: "Run",
        roots: RUN_ROOTS,
        example: "browser navigate URL",
    },
    CommandGroup {
        label: "Verify",
        roots: VERIFY_ROOTS,
        example: "test discover",
    },
    CommandGroup {
        label: "Workspace",
        roots: WORKSPACE_ROOTS,
        example: "process start NAME COMMAND",
    },
    CommandGroup {
        label: "Inspect",
        roots: INSPECT_ROOTS,
        example: "replay list",
    },
    CommandGroup {
        label: "Core",
        roots: CORE_ROOTS,
        example: "workspace trust status",
    },
];

pub static ROOT_COMMANDS: &[&str] = &[
    "agent",
    "browser",
    "cockpit",
    "daemon",
    "debug",
    "editor",
    "experiment",
    "git",
    "github",
    "harness",
    "help",
    "kernel",
    "lsp",
    "memory",
    "process",
    "project",
    "quit",
    "replay",
    "review",
    "surface",
    "task",
    "test",
    "tool",
    "trust",
    "view",
    "workflow",
    "workspace",
];

pub fn command_group_for(surface: DevSurface) -> &'static CommandGroup {
    let index = match surface {
        DevSurface::Agent => 0,
        DevSurface::Code | DevSurface::Git => 1,
        DevSurface::App | DevSurface::Terminal => 2,
        DevSurface::Tasks | DevSurface::Debug => 3,
        DevSurface::More => 4,
        DevSurface::Trust => 6,
    };
    COMMAND_GROUPS
        .get(index)
        .expect("command group index must be valid")
}

const GLOBAL_PALETTE_ROOTS: &[&str] = &["help", "quit", "view"];
const CROSS_SURFACE_PALETTE_ROOTS: &[&str] = &["agent", "review"];

const TRUST_PALETTE_ROOTS: &[&str] = &["trust"];
const AGENT_PALETTE_ROOTS: &[&str] = &["agent", "harness", "task", "github", "review"];
const CODE_PALETTE_ROOTS: &[&str] = &["project", "editor", "lsp"];
const APP_PALETTE_ROOTS: &[&str] = &["browser", "workflow"];
const TERMINAL_PALETTE_ROOTS: &[&str] = &["process", "workflow"];
const TASK_PALETTE_ROOTS: &[&str] = &["task", "test"];
const GIT_PALETTE_ROOTS: &[&str] = &["git", "github"];
const DEBUG_PALETTE_ROOTS: &[&str] = &["debug", "test"];
const MORE_PALETTE_ROOTS: &[&str] = &[
    "cockpit",
    "workspace",
    "daemon",
    "kernel",
    "experiment",
    "replay",
    "memory",
    "surface",
    "tool",
];

fn surface_palette_roots(surface: DevSurface) -> &'static [&'static str] {
    match surface {
        DevSurface::Trust => TRUST_PALETTE_ROOTS,
        DevSurface::Agent => AGENT_PALETTE_ROOTS,
        DevSurface::Code => CODE_PALETTE_ROOTS,
        DevSurface::App => APP_PALETTE_ROOTS,
        DevSurface::Terminal => TERMINAL_PALETTE_ROOTS,
        DevSurface::Tasks => TASK_PALETTE_ROOTS,
        DevSurface::Git => GIT_PALETTE_ROOTS,
        DevSurface::Debug => DEBUG_PALETTE_ROOTS,
        DevSurface::More => MORE_PALETTE_ROOTS,
    }
}

pub fn palette_order(surface: DevSurface) -> Vec<&'static str> {
    let mut ordered = Vec::new();
    for action in surface_actions(surface) {
        if action.key == ":"
            && let Some(root) = action.command.split_whitespace().next()
            && ROOT_COMMANDS.contains(&root)
            && !ordered.contains(&root)
        {
            ordered.push(root);
        }
    }
    for root in surface_palette_roots(surface)
        .iter()
        .copied()
        .chain(
            (surface != DevSurface::Trust)
                .then_some(CROSS_SURFACE_PALETTE_ROOTS)
                .into_iter()
                .flatten()
                .copied(),
        )
        .chain(GLOBAL_PALETTE_ROOTS.iter().copied())
    {
        if !ordered.contains(&root) {
            ordered.push(root);
        }
    }
    ordered
}
pub fn palette_example(surface: DevSurface) -> &'static str {
    match surface {
        DevSurface::Trust => "trust status",
        DevSurface::Agent => "agent prompt TEXT",
        DevSurface::Code => "editor proposals",
        DevSurface::App => "browser observe",
        DevSurface::Terminal => "process start NAME COMMAND",
        DevSurface::Tasks => "task list",
        DevSurface::Git => "git status",
        DevSurface::Debug => "test results",
        DevSurface::More => "workspace status",
    }
}

pub fn route_guide() -> String {
    COMMAND_GROUPS
        .iter()
        .map(|group| format!("{}: {}", group.label, group.roots.join(" · ")))
        .collect::<Vec<_>>()
        .join(" · ")
}

pub fn execute(state: &mut DevTuiState, input: &str) -> Result<String, String> {
    let result = execute_inner(state, input);
    if result.is_err()
        && let Err(error) = &result
        && error.contains("browser")
    {
        state.note_browser_failure("glass.browser", error);
    }
    result
}

fn execute_inner(state: &mut DevTuiState, input: &str) -> Result<String, String> {
    let mut parts = input.split_whitespace();
    let Some(command) = parts.next() else {
        return Ok("Command palette closed".into());
    };
    match command {
        "search" => {
            let mut args = vec!["search"];
            args.extend(parts);
            execute_project(state, args)
        }
        "open" => match parts.next() {
            Some(path) => state.open_path(path),
            None => {
                state.open_file_picker();
                Ok("File picker opened · type to filter · Enter open".into())
            }
        },
        "doctor" => execute_agent(state, {
            let mut args = vec!["doctor"];
            args.extend(parts);
            args
        }),
        "help" | "?" => {
            let project_commands = state
                .ws()?
                .customization()
                .config()
                .commands
                .keys()
                .map(|name| format!("PROJECT:{name}"))
                .collect::<Vec<_>>()
                .join(" · ");
            Ok(format!(
                "Command center: `:actions` opens guided {} launchers; `:` searches every route. Groups: {}. `tool NAME JSON` exposes every resident tool. Mutations use one-use confirmation and revision guards. Project-provided commands: {}",
                command_group_for(state.surface).label,
                route_guide(),
                if project_commands.is_empty() {
                    "none"
                } else {
                    &project_commands
                }
            ))
        }
        "a" | "actions" => {
            state.open_menu();
            Ok("Actions menu opened · ↑↓ select · Enter run · Esc close".into())
        }
        "cockpit" => execute_cockpit(state, parts.collect()),
        "quit" | "q" => {
            state.request_quit();
            Ok("Quit confirmation · Enter exits · Esc stays".into())
        }
        "view" => {
            let name = parts.next().ok_or("view requires a surface")?;
            state.surface = parse_surface(name).ok_or("unknown surface")?;
            Ok(format!("Opened {}", state.surface.label()))
        }
        "trust" => execute_trust(state, parts.collect()),
        "workspace" | "daemon" => execute_workspace(state, command, parts.collect()),
        "project" => execute_project(state, parts.collect()),
        "agent" => execute_agent(state, parts.collect()),
        "plan" => execute_plan(state, parts.collect()),
        "task" | "tasks" => execute_task(state, parts.collect()),
        "editor" => execute_editor(state, parts.collect()),
        "lsp" => execute_lsp(state, parts.collect()),
        "process" => execute_process(state, parts.collect()),
        "app" => execute_app(state, parts.collect()),
        "browser" | "workflow" => execute_browser(state, command, parts.collect()),
        "github" | "gh" => execute_github(state, parts.collect()),
        "harness" => execute_harness(state, parts.collect()),
        "review" => execute_review(state, parts.collect()),
        "debug" => execute_debug(state, parts.collect()),
        "kernel" => execute_kernel(state, parts.collect()),
        "git" => execute_git(state, parts.collect()),
        "tests" | "test" => execute_test(state, parts.collect()),
        "experiment" | "experiments" => execute_experiment(state, parts.collect()),
        "replay" => execute_replay(state, parts.collect()),
        "memory" | "knowledge" => execute_memory(state, command, parts.collect()),
        "surface" | "surfaces" | "backend" => execute_data_surface(state, command, parts.collect()),
        "tool" | "tools" => execute_generic_tool(state, parts.collect()),
        _ if state.ws()?.customization().command(command).is_some() => {
            let result = run_tool(state, &format!("glass.command.{command}"), json!({}), true)?;
            Ok(format!(
                "PROJECT command {}: {}",
                command,
                compact_result(&format!("glass.command.{command}"), &result)
            ))
        }
        _ => Err(format!(
            "unknown command {command}; press : for guided launchers or type :help"
        )),
    }
}

fn execute_cockpit(state: &mut DevTuiState, parts: Vec<&str>) -> Result<String, String> {
    match parts.first().copied().unwrap_or("status") {
        "start" | "open" | "reconnect" => {
            let url = state.start_private_cockpit()?;
            state.surface = DevSurface::More;
            Ok(format!("Private cockpit ready · {url}"))
        }
        "status" => Ok(format!(
            "Private cockpit · {}",
            state.private_cockpit_status()
        )),
        "stop" | "close" => {
            state.stop_private_cockpit();
            Ok("Private cockpit stopped".into())
        }
        _ => Err("cockpit actions: start, status, stop".into()),
    }
}

fn execute_workflow_record(state: &mut DevTuiState, parts: &[&str]) -> Result<String, String> {
    match parts.first().copied().unwrap_or("status") {
        "start" => {
            let name = if parts.len() > 1 {
                parts[1..].join("-")
            } else {
                "click-path".into()
            };
            state.start_workflow_recording(&name)
        }
        "type" => {
            let input = parts
                .get(1)
                .copied()
                .ok_or("workflow record type requires INPUT_NAME")?;
            state.record_workflow_type(input)
        }
        "verify" => state.record_workflow_verify(),
        "stop" | "save" => state.stop_workflow_recording(),
        "status" | "show" => Ok(state.workflow_recording_status()),
        _ => Err("workflow record actions: start [NAME], type INPUT, verify, stop, status".into()),
    }
}

fn execute_github(state: &mut DevTuiState, parts: Vec<&str>) -> Result<String, String> {
    match parts.first().copied().unwrap_or("status") {
        "status" => Ok(format!(
            "GitHub · {}\n{}",
            state.github.summary(),
            state.github_review
        )),
        "review" | "checks" => {
            let result = run_tool(state, "glass.github.review", json!({}), false)?;
            state.surface = DevSurface::Git;
            Ok(compact_result("glass.github.review", &result))
        }
        "ship" | "create" => {
            require_trusted(state)?;
            let mut title_parts = parts.get(1..).unwrap_or_default().to_vec();
            let draft = title_parts.last().copied() == Some("--draft");
            if draft {
                title_parts.pop();
            }
            let title = title_parts.join(" ");
            if title.is_empty() {
                return Err("github ship requires TITLE; append --draft for a draft PR".into());
            }
            let result = run_tool(
                state,
                "glass.github.ship",
                json!({"title":title,"draft":draft}),
                true,
            )?;
            state.surface = DevSurface::Git;
            Ok(compact_result("glass.github.ship", &result))
        }
        _ => Err("GitHub actions: status, review, ship TITLE [--draft]".into()),
    }
}

fn execute_review(state: &mut DevTuiState, parts: Vec<&str>) -> Result<String, String> {
    match parts.first().copied().unwrap_or("show") {
        "show" | "changes" => {
            state.surface = DevSurface::Git;
            state.refresh_review_object();
            Ok("Review object ready · accept the pack · reject ID · ship TITLE · ask".into())
        }
        "ask" => {
            require_trusted(state)?;
            state.surface = DevSurface::Agent;
            state.prepare_review_prompt();
            Ok("Review prompt prepared in the Agent composer".into())
        }
        "accept" => {
            require_trusted(state)?;
            match parts.get(1).copied() {
                Some(id) => state.accept_review_proposal(Some(id)),
                None => state.accept_review_pack(),
            }
        }
        "reject" => {
            require_trusted(state)?;
            state.reject_review_proposal(parts.get(1).copied())
        }
        "ship" => {
            require_trusted(state)?;
            let mut title_parts = parts.get(1..).unwrap_or_default().to_vec();
            let draft = title_parts.last().copied() == Some("--draft");
            if draft {
                title_parts.pop();
            }
            let title = title_parts.join(" ");
            if title.is_empty() {
                return Err("review ship requires TITLE; append --draft for a draft PR".into());
            }
            let body = state.review_object_text();
            let result = run_tool(
                state,
                "glass.github.ship",
                json!({"title":title,"body":body,"draft":draft}),
                true,
            )?;
            state.surface = DevSurface::Git;
            Ok(compact_result("glass.github.ship", &result))
        }
        _ => {
            Err("review actions: show, accept [ID], reject [ID], ship TITLE [--draft], ask".into())
        }
    }
}

fn execute_harness(state: &mut DevTuiState, parts: Vec<&str>) -> Result<String, String> {
    match parts.first().copied().unwrap_or("list") {
        "list" | "status" => {
            state.surface = DevSurface::Agent;
            Ok(format!(
                "External harnesses · ● installed · ○ unavailable\n{}\n\n`harness start NAME` hands the terminal to a selected installed harness\n`harness delegate NAME PROMPT` runs a bounded read-only delegation for codex, claude, or opencode",
                state.harnesses
            ))
        }
        "start" | "open" => {
            require_trusted(state)?;
            if state.background_action_running() {
                return Err(
                    "finish the current Glass action before launching an external harness".into(),
                );
            }
            let name = parts
                .get(1)
                .ok_or("harness start requires NAME; use `harness list`")?;
            let resolved = crate::harness::resolve(name)?;
            state.harness_launch_requested = Some(resolved.spec.id.into());
            state.surface = DevSurface::Agent;
            state.status = format!(
                "{} ready · Enter will hand the terminal to {}",
                resolved.spec.id, resolved.spec.label
            );
            Ok(format!(
                "{} ready · Glass will resume after the external session exits",
                resolved.spec.label
            ))
        }
        "delegate" | "run" => execute_harness_delegate(state, parts),
        _ => {
            Err("harness actions: list, status, start NAME, delegate NAME PROMPT [OPTIONS]".into())
        }
    }
}

fn execute_harness_delegate(state: &mut DevTuiState, parts: Vec<&str>) -> Result<String, String> {
    require_trusted(state)?;
    let name = parts
        .get(1)
        .ok_or("harness delegate requires NAME PROMPT")?;
    let harness = crate::external_agents::ExternalHarness::parse(name)?;
    let mut prompt_parts = Vec::new();
    let mut sandbox = crate::external_agents::ExternalSandbox::ReadOnly;
    let mut timeout_secs = crate::external_agents::DEFAULT_TIMEOUT_SECS;
    let mut allow_mutation = false;
    let mut confirmed = false;
    let mut parse_options = true;
    let mut index = 2;
    while index < parts.len() {
        let part = parts[index];
        if parse_options && part == "--" {
            parse_options = false;
            index += 1;
            continue;
        }
        if parse_options && part == "--allow-mutation" {
            allow_mutation = true;
            index += 1;
            continue;
        }
        if parse_options && part == "--yes" {
            confirmed = true;
            index += 1;
            continue;
        }
        if parse_options && let Some(value) = part.strip_prefix("--sandbox=") {
            sandbox = crate::external_agents::ExternalSandbox::parse(value)?;
            index += 1;
            continue;
        }
        if parse_options && part == "--sandbox" {
            let value = parts
                .get(index + 1)
                .ok_or("--sandbox requires read-only or workspace-write")?;
            sandbox = crate::external_agents::ExternalSandbox::parse(value)?;
            index += 2;
            continue;
        }
        if parse_options
            && let Some(value) = part
                .strip_prefix("--timeout=")
                .or_else(|| part.strip_prefix("--timeout-secs="))
        {
            timeout_secs = value
                .parse()
                .map_err(|_| "--timeout requires an integer number of seconds")?;
            index += 1;
            continue;
        }
        if parse_options && matches!(part, "--timeout" | "--timeout-secs") {
            let value = parts
                .get(index + 1)
                .ok_or("--timeout requires an integer number of seconds")?;
            timeout_secs = value
                .parse()
                .map_err(|_| "--timeout requires an integer number of seconds")?;
            index += 2;
            continue;
        }
        if parse_options && part.starts_with("--") {
            return Err(format!("unknown harness delegate option `{part}`"));
        }
        prompt_parts.push(part);
        index += 1;
    }
    let prompt = prompt_parts.join(" ");
    if prompt.trim().is_empty() {
        return Err("harness delegate requires PROMPT".into());
    }
    if !(1..=3600).contains(&timeout_secs) {
        return Err("harness delegate timeout must be between 1 and 3600 seconds".into());
    }
    if sandbox == crate::external_agents::ExternalSandbox::WorkspaceWrite
        && (!allow_mutation || !confirmed)
    {
        return Err(
            "workspace-write delegation requires --allow-mutation and --yes in the TUI command"
                .into(),
        );
    }
    execute_named_tool(
        state,
        "glass.agent.delegate",
        json!({
            "harness": harness.id(),
            "prompt": prompt,
            "sandbox": sandbox.id(),
            "timeoutSeconds": timeout_secs,
        }),
        true,
        DevSurface::Agent,
    )
}

fn execute_workspace(
    state: &mut DevTuiState,
    command: &str,
    parts: Vec<&str>,
) -> Result<String, String> {
    let action = parts.first().copied().unwrap_or("inspect");
    if command == "daemon" {
        let (tool, mutating) = match action {
            "status" | "doctor" => ("glass.daemon.status", false),
            "start" => ("glass.daemon.start", true),
            "stop" => ("glass.daemon.stop", true),
            "logs" => ("glass.daemon.logs", false),
            _ => {
                return Err(
                    "daemon actions: status, doctor, start, stop, logs; optional paths use `tool`"
                        .into(),
                );
            }
        };
        return execute_named_tool(state, tool, json!({}), mutating, DevSurface::More);
    }
    if matches!(action, "trust" | "trust-status" | "trust-inspect") {
        let trust_action = match action {
            "trust-status" => "status",
            "trust-inspect" => "inspect",
            _ => parts.get(1).copied().unwrap_or("inspect"),
        };
        return execute_trust(state, vec![trust_action]);
    }
    if action == "tools" {
        let tools = state
            .ws()?
            .tool_descriptors()
            .into_iter()
            .map(|tool| {
                format!(
                    "{} {}{}",
                    tool.name,
                    if tool.available { "✓" } else { "×" },
                    if tool.mutating { " · mutating" } else { "" }
                )
            })
            .collect::<Vec<_>>();
        state.editor = tools.join("\n");
        state.surface = DevSurface::More;
        return Ok(format!("{} resident tools listed in Code", tools.len()));
    }
    let result = match action {
        "status" => execute_named_tool(
            state,
            "glass.runtime.inspect",
            json!({}),
            false,
            DevSurface::More,
        )?,
        "inspect" if parts.get(1).is_some() => execute_named_tool(
            state,
            "glass.workspace.inspect",
            json!({"id":parts.get(1).copied().unwrap_or_default()}),
            false,
            DevSurface::More,
        )?,
        "inspect" => execute_named_tool(
            state,
            "glass.runtime.inspect",
            json!({}),
            false,
            DevSurface::More,
        )?,
        "list" => execute_named_tool(
            state,
            "glass.workspace.list",
            json!({}),
            false,
            DevSurface::More,
        )?,
        "suspend" | "resume" | "delete" => {
            let id = parts.get(1).ok_or("workspace action requires ID")?;
            execute_named_tool(
                state,
                &format!("glass.workspace.{action}"),
                json!({"id":id}),
                true,
                DevSurface::More,
            )?
        }
        _ => {
            return Err(
                "workspace actions: status, inspect, tools, list, suspend ID, resume ID, delete ID, trust"
                    .into(),
            );
        }
    };
    Ok(result)
}

fn execute_project(state: &mut DevTuiState, parts: Vec<&str>) -> Result<String, String> {
    let action = parts.first().copied().unwrap_or("inspect");
    match action {
        "inspect" => execute_named_tool(
            state,
            "glass.runtime.inspect",
            json!({}),
            false,
            DevSurface::Code,
        ),
        "files" => execute_named_tool(
            state,
            "glass.file.list",
            json!({"path":parts.get(1).copied().unwrap_or("")}),
            false,
            DevSurface::Code,
        ),
        "search" => {
            let query = parts.get(1..).unwrap_or_default().join(" ");
            if query.is_empty() {
                return Err("project search requires QUERY".into());
            }
            execute_named_tool(
                state,
                "glass.file.search",
                json!({"query":query}),
                false,
                DevSurface::Code,
            )
        }
        "read" => execute_named_tool(
            state,
            "glass.file.read",
            json!({"path":parts.get(1).ok_or("project read requires PATH")?}),
            false,
            DevSurface::Code,
        ),
        "edit" => {
            let path = parts.get(1).ok_or("project edit requires PATH CONTENT")?;
            let content = parts.get(2..).unwrap_or_default().join(" ");
            if content.is_empty() {
                return Err("project edit requires PATH CONTENT".into());
            }
            execute_named_tool(
                state,
                "glass.file.write",
                json!({"path":path,"content":content}),
                true,
                DevSurface::Code,
            )
        }
        "mkdir" => execute_named_tool(
            state,
            "glass.file.mkdir",
            json!({"path":parts.get(1).ok_or("project mkdir requires PATH")?}),
            true,
            DevSurface::Code,
        ),
        "rename" => execute_named_tool(
            state,
            "glass.file.rename",
            json!({
                "from":parts.get(1).ok_or("project rename requires FROM TO")?,
                "to":parts.get(2).ok_or("project rename requires FROM TO")?
            }),
            true,
            DevSurface::Code,
        ),
        "delete" => execute_named_tool(
            state,
            "glass.file.delete",
            json!({"path":parts.get(1).ok_or("project delete requires PATH")?}),
            true,
            DevSurface::Code,
        ),
        "diagnostics" => execute_named_tool(
            state,
            "glass.diagnostics.run",
            json!({"path":parts.get(1).ok_or("project diagnostics requires PATH")?}),
            false,
            DevSurface::Code,
        ),
        "run" => {
            let name = parts.get(1).ok_or("project run requires NAME COMMAND")?;
            let command = parts.get(2..).unwrap_or_default().join(" ");
            if command.is_empty() {
                return Err("project run requires NAME COMMAND".into());
            }
            execute_named_tool(
                state,
                "glass.process.start",
                json!({"name":name,"command":command}),
                true,
                DevSurface::Terminal,
            )
        }
        "test" | "lint" => {
            let detection = state.ws()?.project().detection().clone();
            let command = if action == "test" {
                detection.test_command
            } else {
                detection.lint_command
            }
            .ok_or_else(|| format!("project has no detected {action} command"))?;
            execute_named_tool(
                state,
                "glass.test.run",
                json!({"name":action,"command":command}),
                true,
                DevSurface::Tasks,
            )
        }
        "process" => execute_process(state, parts.get(1..).unwrap_or_default().to_vec()),
        "diff" => execute_named_tool(
            state,
            "glass.editor.diff",
            json!({}),
            false,
            DevSurface::Git,
        ),
        "link" => execute_named_tool(
            state,
            "glass.graph.link",
            json!({
                "from":parts.get(1).ok_or("project link requires ENTITY PATH START_LINE END_LINE")?,
                "to":parts.get(2).ok_or("project link requires ENTITY PATH START_LINE END_LINE")?,
                "relation":"sourceRuntime",
                "evidence":{"startLine":parse_u64(parts.get(3),"START_LINE")?,"endLine":parse_u64(parts.get(4),"END_LINE")?}
            }),
            true,
            DevSurface::More,
        ),
        "graph" => execute_project_graph(state, parts.get(1..).unwrap_or_default().to_vec()),
        "breakpoint" => execute_project_breakpoint(state, parts.get(1..).unwrap_or_default()),
        "timeline" => execute_named_tool(
            state,
            "glass.replay.list",
            json!({"since":0,"limit":512}),
            false,
            DevSurface::More,
        ),
        "replay" => execute_replay(state, parts.get(1..).unwrap_or_default().to_vec()),
        "experiment" => execute_experiment(state, parts.get(1..).unwrap_or_default().to_vec()),
        "attach" => {
            let actor = parts.get(1).ok_or("project attach requires ACTOR")?;
            execute_named_tool(
                state,
                "glass.project.attach",
                json!({"actor":actor}),
                true,
                DevSurface::More,
            )
        }
        "neovim" => execute_neovim(state, parts.get(1..).unwrap_or_default()),
        _ => Err("project actions: inspect, files, search, read, edit, mkdir, rename, delete, diagnostics, run, test, lint, process, diff, link, graph, breakpoint, timeline, replay, neovim, experiment, attach".into()),
    }
}

fn execute_project_graph(state: &mut DevTuiState, parts: Vec<&str>) -> Result<String, String> {
    match parts.first().copied().unwrap_or("discover") {
        "discover" => execute_named_tool(
            state,
            "glass.semantic.links",
            json!({}),
            false,
            DevSurface::More,
        ),
        "entity" => execute_named_tool(
            state,
            "glass.graph.query",
            json!({"id":parts.get(1).ok_or("project graph entity requires ENTITY")?}),
            false,
            DevSurface::More,
        ),
        "source" => execute_named_tool(
            state,
            "glass.graph.source",
            json!({
                "path":parts.get(1).ok_or("project graph source requires PATH")?,
                "line":parts.get(2).map(|value| parse_u64(Some(value), "LINE")).transpose()?
            }),
            false,
            DevSurface::More,
        ),
        _ => Err("project graph actions: discover, entity ENTITY, source PATH [LINE]".into()),
    }
}
fn execute_project_breakpoint(state: &mut DevTuiState, parts: &[&str]) -> Result<String, String> {
    require_trusted(state)?;
    let kind = parts
        .first()
        .ok_or("project breakpoint requires KIND ENTITY BEFORE.json AFTER.json")?;
    let entity = parts
        .get(1)
        .ok_or("project breakpoint requires KIND ENTITY BEFORE.json AFTER.json")?;
    let before_path = parts
        .get(2)
        .ok_or("project breakpoint requires KIND ENTITY BEFORE.json AFTER.json")?;
    let after_path = parts
        .get(3)
        .ok_or("project breakpoint requires KIND ENTITY BEFORE.json AFTER.json")?;
    let before: SemanticSnapshot =
        serde_json::from_value(read_project_json(state, before_path)?)
            .map_err(|error| format!("invalid semantic snapshot {before_path}: {error}"))?;
    let after: SemanticSnapshot = serde_json::from_value(read_project_json(state, after_path)?)
        .map_err(|error| format!("invalid semantic snapshot {after_path}: {error}"))?;
    let breakpoint = match *kind {
        "disappears" => SemanticBreakpoint::EntityDisappears {
            entity_id: (*entity).into(),
        },
        "name-missing" => SemanticBreakpoint::AccessibleNameMissing {
            entity_id: Some((*entity).into()),
        },
        "role-changes" => SemanticBreakpoint::RoleChanges {
            entity_id: (*entity).into(),
        },
        "actionability-lost" => SemanticBreakpoint::ActionabilityLost {
            entity_id: (*entity).into(),
        },
        _ => {
            return Err(
                "breakpoint kind must be disappears, name-missing, role-changes, or actionability-lost"
                    .into(),
            );
        }
    };
    let hits = {
        let mut workspace = state.ws_mut()?;
        workspace
            .project_mut()
            .discover_runtime_links()
            .map_err(|error| error.to_string())?;
        workspace
            .project_mut()
            .evaluate_semantic_breakpoints(&[breakpoint], &before, &after)
            .map_err(|error| error.to_string())?
    };
    state.editor = serde_json::to_string_pretty(&hits).map_err(|error| error.to_string())?;
    state.surface = DevSurface::More;
    Ok(format!(
        "Semantic breakpoint evaluated · {} hit(s)",
        hits.len()
    ))
}

fn execute_neovim(state: &mut DevTuiState, parts: &[&str]) -> Result<String, String> {
    match parts.first().copied().unwrap_or("probe") {
        "probe" => {
            let capabilities =
                crate::development::probe_neovim().map_err(|error| error.to_string())?;
            state.editor =
                serde_json::to_string_pretty(&capabilities).map_err(|error| error.to_string())?;
            state.surface = DevSurface::Code;
            Ok("Neovim capability probe completed".into())
        }
        "start" => {
            let name = parts.get(1).copied().unwrap_or("neovim");
            let result = execute_named_tool(
                state,
                "glass.neovim.start",
                json!({"name":name,"path":parts.get(2)}),
                true,
                DevSurface::Terminal,
            )?;
            Ok(result)
        }
        _ => Err("project neovim actions: probe, start".into()),
    }
}

fn execute_replay(state: &mut DevTuiState, parts: Vec<&str>) -> Result<String, String> {
    let action = parts.first().copied().unwrap_or("list");
    let (tool, arguments) = match action {
        "list" | "inspect" | "attach" => (
            "glass.replay.list",
            json!({
                "since":parts.get(1).and_then(|value| value.parse::<u64>().ok()).unwrap_or(0),
                "limit":parts.get(2).and_then(|value| value.parse::<u64>().ok()).unwrap_or(128)
            }),
        ),
        "diff" => (
            "glass.replay.diff",
            json!({
                "from":parse_u64(parts.get(1),"FROM")?,
                "to":parse_u64(parts.get(2),"TO")?
            }),
        ),
        _ => return Err("replay actions: list, inspect, diff FROM TO, attach".into()),
    };
    execute_named_tool(state, tool, arguments, false, DevSurface::More)
}

fn execute_memory(
    state: &mut DevTuiState,
    command: &str,
    parts: Vec<&str>,
) -> Result<String, String> {
    if command == "knowledge" {
        return execute_named_tool(
            state,
            "glass.memory.retrieve",
            json!({"limit":parts.first().and_then(|value| value.parse::<u64>().ok()).unwrap_or(128)}),
            false,
            DevSurface::More,
        );
    }
    match parts.first().copied().unwrap_or("status") {
        "status" | "stats" | "list" | "export" | "reindex" => execute_named_tool(
            state,
            "glass.memory.retrieve",
            json!({"limit":128}),
            false,
            DevSurface::More,
        ),
        "inspect" | "explain" => execute_named_tool(
            state,
            if parts.first() == Some(&"inspect") {
                "glass.memory.retrieve"
            } else {
                "glass.memory.explain"
            },
            json!({"recordId":parts.get(1).ok_or("memory action requires RECORD_ID")?}),
            false,
            DevSurface::More,
        ),
        "forget" | "prune" => execute_named_tool(
            state,
            "glass.memory.forget",
            json!({"recordId":parts.get(1).ok_or("memory forget requires RECORD_ID")?}),
            true,
            DevSurface::More,
        ),
        _ => Err("memory actions: status, list, inspect ID, explain ID, forget ID, export, prune, reindex".into()),
    }
}

fn execute_data_surface(
    state: &mut DevTuiState,
    command: &str,
    parts: Vec<&str>,
) -> Result<String, String> {
    let action = parts.first().copied().unwrap_or("inspect");
    let tool = match (command, action) {
        ("backend", "status" | "capabilities" | "test") => "glass.capabilities.inspect",
        ("surface" | "surfaces", "inspect" | "coverage") => "glass.semantic.links",
        _ => {
            return Err(format!(
                "{command} actions: {}",
                if command == "backend" {
                    "status INPUT, capabilities INPUT, test INPUT"
                } else {
                    "inspect INPUT, coverage INPUT"
                }
            ));
        }
    };
    execute_named_tool(state, tool, json!({}), false, DevSurface::More)
}

fn execute_generic_tool(state: &mut DevTuiState, parts: Vec<&str>) -> Result<String, String> {
    let name = parts.first().ok_or("tool requires NAME [JSON]")?;
    let name = if name.starts_with("glass.") {
        (*name).to_string()
    } else {
        format!("glass.{name}")
    };
    let descriptor = state
        .ws()?
        .tool_descriptors()
        .into_iter()
        .find(|tool| tool.name == name)
        .ok_or_else(|| format!("unknown resident tool {name}; use workspace tools"))?;
    if !descriptor.available {
        return Err(descriptor
            .unavailable_reason
            .unwrap_or_else(|| format!("{name} is unavailable")));
    }
    let arguments = parts
        .get(1..)
        .filter(|values| !values.is_empty())
        .map(|values| values.join(" "))
        .map(|value| {
            serde_json::from_str(&value)
                .map_err(|error| format!("tool arguments must be valid JSON: {error}"))
        })
        .transpose()?
        .unwrap_or_else(|| json!({}));
    let surface = surface_for_tool(&name);
    execute_named_tool(state, &name, arguments, descriptor.mutating, surface)
}

fn execute_named_tool(
    state: &mut DevTuiState,
    tool: &str,
    arguments: Value,
    mutating: bool,
    surface: DevSurface,
) -> Result<String, String> {
    let result = run_tool(state, tool, arguments, mutating)?;
    state.surface = surface;
    Ok(compact_result(tool, &result))
}

fn surface_for_tool(tool: &str) -> DevSurface {
    if tool.starts_with("glass.agent") {
        DevSurface::Agent
    } else if tool.starts_with("glass.browser") || tool.starts_with("glass.workflow") {
        DevSurface::App
    } else if tool.starts_with("glass.process") {
        DevSurface::Terminal
    } else if tool.starts_with("glass.git") {
        DevSurface::Git
    } else if tool.starts_with("glass.task")
        || tool.starts_with("glass.test")
        || tool.starts_with("glass.eval")
    {
        DevSurface::Tasks
    } else if tool.starts_with("glass.debug") {
        DevSurface::Debug
    } else if tool.starts_with("glass.editor")
        || tool.starts_with("glass.file")
        || tool.starts_with("glass.lsp")
        || tool.starts_with("glass.diagnostics")
    {
        DevSurface::Code
    } else {
        DevSurface::More
    }
}

fn execute_trust(state: &mut DevTuiState, parts: Vec<&str>) -> Result<String, String> {
    match parts.first().copied().unwrap_or("inspect") {
        "status" => Ok(format!("Workspace trust: {}", state.ws()?.trust().label())),
        "inspect" => {
            state.surface = DevSurface::Trust;
            Ok(format!(
                "Inspecting {} configuration items",
                state.ws()?.trust_inspection().len()
            ))
        }
        action @ ("untrusted" | "once" | "project") => {
            let decision = match action {
                "untrusted" => crate::LocalTrustDecision::OpenUntrusted,
                "once" => crate::LocalTrustDecision::TrustOnce,
                _ => crate::LocalTrustDecision::TrustProject,
            };
            let trust = state
                .ws()?
                .apply_local_trust_decision(decision)
                .map_err(|error| error.to_string())?;
            state.snapshot_trust_label = trust.label().into();
            state.surface = DevSurface::Agent;
            Ok(format!("Workspace trust is now {}", trust.label()))
        }
        _ => Err("trust actions: status, inspect, untrusted, once, project".into()),
    }
}

fn execute_plan(state: &mut DevTuiState, parts: Vec<&str>) -> Result<String, String> {
    require_trusted(state)?;
    match parts.first().copied().unwrap_or("show") {
        "ask" => {
            state.set_composer_run_mode(crate::AgentTurnMode::Ask);
            Ok("Ask mode · read-only".into())
        }
        "plan" | "mode" => {
            state.set_composer_run_mode(crate::AgentTurnMode::Plan);
            state.focus_composer_dock();
            Ok("Plan mode · describe a goal, then Enter".into())
        }
        "agent" => {
            state.set_composer_run_mode(crate::AgentTurnMode::Agent);
            Ok("Agent mode · proposals unless unrestricted".into())
        }
        "show" | "status" => Ok(match &state.pending_plan {
            Some(plan) => format!(
                "PLAN {}\n{}\n{}\n{}",
                plan.id,
                plan.goal,
                if plan.accepted { "accepted" } else { "draft" },
                plan.body
            ),
            None => "No plan yet · :plan mode then send a goal".into(),
        }),
        "accept" => {
            // Worker is not in this command path; stash an implement prompt.
            let Some(plan) = state.pending_plan.clone() else {
                return Err("no plan to accept".into());
            };
            state.pending_plan = Some(super::state::WorkspacePlan {
                accepted: true,
                ..plan.clone()
            });
            state.set_composer_run_mode(crate::AgentTurnMode::Agent);
            state.composer_input = format!(
                "Implement this accepted plan. Stay in proposals unless I say otherwise.\n\nGoal: {}\n\n{}",
                plan.goal, plan.body
            );
            state.composer_cursor = state.composer_input.len();
            state.open_composer();
            Ok(format!("Plan {} ready · Enter implements", plan.id))
        }
        "reject" => {
            state.reject_pending_plan();
            Ok(state.status.clone())
        }
        _ => Err("plan actions: show, accept, reject, ask, plan, agent".into()),
    }
}

fn execute_agent(state: &mut DevTuiState, parts: Vec<&str>) -> Result<String, String> {
    let Some(action) = parts.first().copied() else {
        state.surface = DevSurface::Agent;
        return Ok("Opened Glass Agent".into());
    };
    if !matches!(action, "doctor" | "status" | "setup" | "update") {
        require_trusted(state)?;
    }
    match action {
        "doctor" | "status" => {
            let ready = state.refresh_agent_readiness()?;
            state.surface = DevSurface::Agent;
            Ok(if ready {
                "Glass Agent is ready · press Enter or type to start a conversation".into()
            } else {
                "Glass Agent needs setup · use :agent setup or :agent setup login, then Enter to chat"
                    .into()
            })
        }
        "setup" | "update" => {
            let options = parts.get(1..).unwrap_or_default();
            let login = options
                .iter()
                .any(|value| matches!(*value, "login" | "--login"));
            let update = action == "update"
                || options
                    .iter()
                    .any(|value| matches!(*value, "update" | "--update"));
            if options.iter().any(|value| {
                !matches!(*value, "login" | "--login" | "update" | "--update")
            }) {
                return Err(
                    "agent setup options: [login|--login] [update|--update]".into(),
                );
            }
            if login && update {
                return Err("agent setup cannot combine login and update".into());
            }
            state.surface = DevSurface::Agent;
            if login {
                state.request_agent_login()?;
                return Ok("Pi login will open in this terminal".into());
            }
            let result = run_tool(
                state,
                "glass.agent.setup",
                json!({"login": false, "update": update}),
                true,
            )?;
            Ok(compact_result("glass.agent.setup", &result))
        }
        "spawn" => {
            let role = parts.get(1).ok_or("agent spawn requires ROLE TASK")?;
            let task = parts.get(2..).unwrap_or_default().join(" ");
            if task.is_empty() {
                return Err("agent spawn requires ROLE TASK".into());
            }
            let id = state
                .ws_mut()?
                .agents()
                .create(AgentSpec::new(*role, task))
                .map_err(|error| error.to_string())?;
            state.surface = DevSurface::Agent;
            Ok(format!("Spawned {}", id.as_str()))
        }
        "prompt" | "steer" | "follow-up" => {
            let (agent_id, offset) = explicit_or_active_agent(state, &parts)?;
            let text_start = if offset == 1 { 2 } else { 1 };
            let text = parts.get(text_start..).unwrap_or_default().join(" ");
            if text.trim().is_empty() {
                return Err(format!("agent {action} requires TEXT"));
            }
            let mode = match action {
                "steer" => "steer",
                "follow-up" => "follow-up",
                _ => "prompt",
            };
            let mut arguments = json!({"text":text,"mode":mode});
            if let Some(agent_id) = agent_id {
                arguments["agentId"] = Value::String(agent_id);
            }
            let result = run_tool(state, "glass.agent.send", arguments, true)?;
            state.surface = DevSurface::Agent;
            Ok(compact_result("glass.agent.send", &result))
        }
        "hello" | "models" => {
            let tool = if action == "hello" {
                "glass.agent.hello"
            } else {
                "glass.agent.models"
            };
            let result = run_tool(state, tool, json!({}), false)?;
            state.surface = DevSurface::Agent;
            Ok(compact_result(tool, &result))
        }
        "cancel" | "abort" => {
            let (agent_id, _) = explicit_or_active_agent(state, &parts)?;
            let agent_id = agent_id.ok_or("no active agent session")?;
            let result = run_tool(
                state,
                "glass.agent.abort",
                json!({"agentId":agent_id}),
                true,
            )?;
            state.surface = DevSurface::Agent;
            Ok(compact_result("glass.agent.abort", &result))
        }
        "compact" => agent_control(state, &parts, "glass.agent.compact", json!({})),
        "model" | "set-model" => {
            let (agent_id, offset) = explicit_or_active_agent(state, &parts)?;
            let provider = parts
                .get(offset + 1)
                .ok_or("agent model requires [ID] PROVIDER MODEL")?;
            let model = parts
                .get(offset + 2)
                .ok_or("agent model requires [ID] PROVIDER MODEL")?;
            let mut arguments = json!({"provider":provider,"modelId":model});
            if let Some(agent_id) = agent_id {
                arguments["agentId"] = Value::String(agent_id);
            }
            let result = run_tool(state, "glass.agent.model", arguments, true)?;
            state.surface = DevSurface::Agent;
            Ok(compact_result("glass.agent.model", &result))
        }
        "thinking" | "set-thinking" => {
            let (agent_id, offset) = explicit_or_active_agent(state, &parts)?;
            let level = parts
                .get(offset + 1)
                .ok_or("agent thinking requires [ID] LEVEL")?;
            let mut arguments = json!({"level":level});
            if let Some(agent_id) = agent_id {
                arguments["agentId"] = Value::String(agent_id);
            }
            let result = run_tool(state, "glass.agent.thinking", arguments, true)?;
            state.surface = DevSurface::Agent;
            Ok(compact_result("glass.agent.thinking", &result))
        }
        "new" | "new-session" => {
            agent_control(state, &parts, "glass.agent.new-session", json!({}))
        }
        "clone" | "clone-session" => {
            agent_control(state, &parts, "glass.agent.clone-session", json!({}))
        }
        "rewind" => {
            let (agent_id, offset) = explicit_or_active_agent(state, &parts)?;
            let entry = parts
                .get(offset + 1)
                .ok_or("agent rewind requires [ID] ENTRY")?;
            let mut arguments = json!({"entryId":entry});
            if let Some(agent_id) = agent_id {
                arguments["agentId"] = Value::String(agent_id);
            }
            let result = run_tool(state, "glass.agent.rewind", arguments, true)?;
            state.surface = DevSurface::Agent;
            Ok(compact_result("glass.agent.rewind", &result))
        }
        "fork" => {
            let (agent_id, offset) = explicit_or_active_agent(state, &parts)?;
            let entry = parts
                .get(offset + 1)
                .ok_or("agent fork requires [ID] ENTRY")?;
            let mut arguments = json!({"entryId":entry});
            if let Some(agent_id) = agent_id {
                arguments["agentId"] = Value::String(agent_id);
            }
            let result = run_tool(state, "glass.agent.fork", arguments, true)?;
            state.surface = DevSurface::Agent;
            Ok(compact_result("glass.agent.fork", &result))
        }
        "switch" | "switch-session" => {
            let (agent_id, offset) = explicit_or_active_agent(state, &parts)?;
            let path = parts
                .get(offset + 1)
                .ok_or("agent switch requires [ID] SESSION_PATH")?;
            let mut arguments = json!({"path":path});
            if let Some(agent_id) = agent_id {
                arguments["agentId"] = Value::String(agent_id);
            }
            let result = run_tool(state, "glass.agent.switch-session", arguments, true)?;
            state.surface = DevSurface::Agent;
            Ok(compact_result("glass.agent.switch-session", &result))
        }
        "sessions" | "list-sessions" | "tree" | "messages" | "entries" | "stats" => {
            let tool = match action {
                "sessions" | "list-sessions" => "glass.agent.sessions",
                "tree" => "glass.agent.tree",
                "messages" => "glass.agent.messages",
                "entries" => "glass.agent.entries",
                _ => "glass.agent.stats",
            };
            let (agent_id, offset) = explicit_or_active_agent(state, &parts)?;
            let mut arguments = json!({});
            if let Some(agent_id) = agent_id {
                arguments["agentId"] = Value::String(agent_id);
            }
            if action == "entries"
                && let Some(since) = parts.get(offset + 1)
            {
                arguments["since"] = Value::String((*since).into());
            }
            let result = run_tool(state, tool, arguments, false)?;
            state.surface = DevSurface::Agent;
            if matches!(action, "sessions" | "list-sessions") {
                state.open_session_picker(&result);
                return Ok(state.status.clone());
            }
            if action == "stats" {
                state.agent_token_summary = super::projection::first_meaningful(&result)
                    .lines()
                    .next()
                    .unwrap_or("stats")
                    .to_string();
            }
            Ok(compact_result(tool, &result))
        }
        _ => Err("agent actions: doctor, status, setup [login], hello, models, spawn, prompt, steer, follow-up, cancel, abort, compact, model, set-model, thinking, set-thinking, new, new-session, clone, clone-session, rewind, fork, switch, sessions, tree, messages, entries, stats".into()),
    }
}

fn explicit_or_active_agent(
    state: &mut DevTuiState,
    parts: &[&str],
) -> Result<(Option<String>, usize), String> {
    if let Some(id) = parts
        .get(1)
        .copied()
        .filter(|value| value.starts_with("agent-"))
    {
        let agent = find_agent(state, id)?;
        return Ok((Some(agent.as_str().to_string()), 1));
    }
    if let Some(id) = state.selected_agent.as_ref() {
        return Ok((Some(id.as_str().to_string()), 0));
    }
    let id = state
        .ws_mut()?
        .agents()
        .list()
        .map_err(|error| error.to_string())?
        .into_iter()
        .find(|agent| {
            !matches!(
                agent.status,
                crate::AgentStatus::Completed
                    | crate::AgentStatus::Failed
                    | crate::AgentStatus::Cancelled
            )
        })
        .map(|agent| agent.id.as_str().to_string());
    Ok((id, 0))
}

fn agent_control(
    state: &mut DevTuiState,
    parts: &[&str],
    tool: &str,
    mut arguments: Value,
) -> Result<String, String> {
    let (id, _) = explicit_or_active_agent(state, parts)?;
    let id = id.ok_or("no active agent session")?;
    arguments["agentId"] = Value::String(id);
    let result = run_tool(state, tool, arguments, true)?;
    state.surface = DevSurface::Agent;
    Ok(compact_result(tool, &result))
}

fn execute_task(state: &mut DevTuiState, parts: Vec<&str>) -> Result<String, String> {
    let Some(action) = parts.first().copied() else {
        state.surface = DevSurface::Tasks;
        return Ok("Opened autonomous task DAG".into());
    };
    match action {
        "list" => execute_named_tool(
            state,
            "glass.task.list",
            json!({}),
            false,
            DevSurface::Tasks,
        ),
        "wake" => execute_named_tool(
            state,
            "glass.task.wake",
            json!({}),
            false,
            DevSurface::Tasks,
        ),
        "get" | "inspect" => execute_named_tool(
            state,
            "glass.task.inspect",
            json!({"taskId":parts.get(1).ok_or("task inspect requires TASK_ID")?}),
            false,
            DevSurface::Tasks,
        ),
        "crew" => {
            require_trusted(state)?;
            let goal = parts.get(1..).unwrap_or_default().join(" ");
            if goal.is_empty() {
                return Err("task crew requires GOAL".into());
            }
            execute_named_tool(
                state,
                "glass.task.crew",
                json!({"goal": goal}),
                true,
                DevSurface::Tasks,
            )
        }
        "create" | "create-after" => {
            require_trusted(state)?;
            let offset = usize::from(action == "create-after");
            let title = parts
                .get(1 + offset)
                .ok_or("task create requires TITLE PROMPT")?;
            let prompt = parts.get(2 + offset..).unwrap_or_default().join(" ");
            if prompt.is_empty() {
                return Err("task create requires TITLE PROMPT".into());
            }
            let mut spec = TaskSpec::new(*title, prompt);
            if action == "create-after" {
                spec.dependencies.push(crate::TaskId::parse(
                    *parts
                        .get(1)
                        .ok_or("task create-after requires DEPENDENCY TITLE PROMPT")?,
                )
                .map_err(|error| error.to_string())?);
            }
            execute_named_tool(
                state,
                "glass.task.create",
                serde_json::to_value(spec).map_err(|error| error.to_string())?,
                true,
                DevSurface::Tasks,
            )
        }
        "pause" | "resume" | "cancel" | "retry" | "override" => {
            require_trusted(state)?;
            let task_id = parts.get(1).ok_or("task action requires TASK_ID")?;
            let tool = match action {
                "pause" => "glass.task.pause",
                "resume" => "glass.task.resume",
                "cancel" => "glass.task.cancel",
                "retry" => "glass.task.retry",
                _ => "glass.task.override-blocked",
            };
            execute_named_tool(
                state,
                tool,
                json!({"taskId":task_id}),
                true,
                DevSurface::Tasks,
            )
        }
        "reassign" => {
            require_trusted(state)?;
            execute_named_tool(
                state,
                "glass.task.reassign",
                json!({
                    "taskId":parts.get(1).ok_or("task reassign requires TASK_ID ROLE [MODEL] [THINKING]")?,
                    "role":parts.get(2).ok_or("task reassign requires TASK_ID ROLE [MODEL] [THINKING]")?,
                    "model":parts.get(3),
                    "thinking":parts.get(4)
                }),
                true,
                DevSurface::Tasks,
            )
        }
        "evidence" | "verify" => {
            require_trusted(state)?;
            let encoded = parts.get(4..).unwrap_or_default().join(" ");
            let details = if encoded.is_empty() {
                Value::Null
            } else {
                serde_json::from_str(&encoded).map_err(|error| error.to_string())?
            };
            execute_named_tool(
                state,
                if action == "verify" {
                    "glass.task.verify"
                } else {
                    "glass.task.evidence"
                },
                json!({
                    "taskId":parts.get(1).ok_or("task evidence requires TASK_ID KIND PASS [JSON]")?,
                    "kind":parts.get(2).ok_or("task evidence requires TASK_ID KIND PASS [JSON]")?,
                    "passed":parts.get(3).ok_or("task evidence PASS must be true or false")?.parse::<bool>().map_err(|_| "task evidence PASS must be true or false")?,
                    "details":details
                }),
                true,
                DevSurface::Tasks,
            )
        }
        _ => Err("task actions: list, get, inspect, create, create-after, crew, wake, pause, resume, cancel, retry, reassign, override, evidence, verify".into()),
    }
}

fn execute_editor(state: &mut DevTuiState, parts: Vec<&str>) -> Result<String, String> {
    let Some(action) = parts.first().copied() else {
        state.surface = DevSurface::Code;
        return Ok("Opened shared editor".into());
    };
    if action == "edit" {
        state.surface = DevSurface::Code;
        state.enter_code_edit();
        return Ok("Editor edit mode opened · Esc closes".into());
    }
    if action == "comments" {
        let path = parts.get(1).copied();
        let result = run_tool(state, "glass.editor.comments", json!({"path":path}), false)?;
        state.editor = serde_json::to_string_pretty(&result).map_err(|error| error.to_string())?;
        state.surface = DevSurface::Code;
        return Ok(compact_result("glass.editor.comments", &result));
    }
    if action == "proposals" {
        let result = run_tool(state, "glass.editor.proposals", json!({}), false)?;
        state.editor = serde_json::to_string_pretty(&result).map_err(|error| error.to_string())?;
        state.surface = DevSurface::Code;
        return Ok(compact_result("glass.editor.proposals", &result));
    }
    if action == "checkpoints" {
        let result = run_tool(state, "glass.editor.checkpoints", json!({}), false)?;
        state.editor = serde_json::to_string_pretty(&result).map_err(|error| error.to_string())?;
        state.surface = DevSurface::Code;
        return Ok(compact_result("glass.editor.checkpoints", &result));
    }
    if action == "replace-selection" {
        let path = state.focused_editor_path.clone();
        if path.is_empty() {
            return Err("editor replace-selection requires an open buffer".into());
        }
        let replacement = parts.get(1..).unwrap_or_default().join(" ");
        if replacement.is_empty() {
            return Err("editor replace-selection requires TEXT".into());
        }
        let result = run_tool(
            state,
            "glass.editor.replace_selection",
            json!({"path":path,"replacement":replacement}),
            true,
        )?;
        state.surface = DevSurface::Code;
        state.refresh_editor_projection();
        return Ok(compact_result("glass.editor.replace_selection", &result));
    }
    if action == "comment-selection" {
        let path = state.focused_editor_path.clone();
        if path.is_empty() {
            return Err("editor comment-selection requires an open buffer".into());
        }
        let text = parts.get(1..).unwrap_or_default().join(" ");
        if text.is_empty() {
            return Err("editor comment-selection requires TEXT".into());
        }
        let (start_line, end_line) = state
            .focused_editor_selection
            .as_ref()
            .filter(|selection| !selection.is_empty())
            .map(crate::development::TextSelection::ordered)
            .map(|(start, end)| (start.line, end.line))
            .unwrap_or((state.focused_editor_line, state.focused_editor_line));
        let result = run_tool(
            state,
            "glass.editor.comment.add",
            json!({"path":path,"startLine":start_line,"endLine":end_line,"text":text}),
            true,
        )?;
        state.surface = DevSurface::Code;
        state.refresh_editor_projection();
        return Ok(compact_result("glass.editor.comment.add", &result));
    }
    if action == "comment" {
        let path = parts
            .get(1)
            .ok_or("editor comment requires PATH START END TEXT")?;
        let start_line = parse_u64(parts.get(2), "START")?;
        let end_line = parse_u64(parts.get(3), "END")?;
        let text = parts.get(4..).unwrap_or_default().join(" ");
        if text.is_empty() {
            return Err("editor comment requires TEXT".into());
        }
        let result = run_tool(
            state,
            "glass.editor.comment.add",
            json!({"path":path,"startLine":start_line,"endLine":end_line,"text":text}),
            true,
        )?;
        state.surface = DevSurface::Code;
        return Ok(compact_result("glass.editor.comment.add", &result));
    }
    if action == "comment-resolve" {
        let id = parts.get(1).ok_or("editor comment-resolve requires ID")?;
        let result = run_tool(
            state,
            "glass.editor.comment.resolve",
            json!({"id":id}),
            true,
        )?;
        state.surface = DevSurface::Code;
        return Ok(compact_result("glass.editor.comment.resolve", &result));
    }
    if action == "propose" {
        let path = parts
            .get(1)
            .ok_or("editor propose requires PATH SUMMARY TEXT")?;
        let summary = parts
            .get(2)
            .ok_or("editor propose requires PATH SUMMARY TEXT")?;
        let proposed = parts.get(3..).unwrap_or_default().join(" ");
        if proposed.is_empty() {
            return Err("editor propose requires TEXT".into());
        }
        let original = state
            .ws()?
            .project()
            .buffer(path)
            .map(|buffer| buffer.content.clone())
            .ok_or_else(|| format!("buffer {path} must be open before proposing an edit"))?;
        let result = run_tool(
            state,
            "glass.editor.proposal.create",
            json!({"path":path,"original":original,"proposed":proposed,"summary":summary}),
            true,
        )?;
        state.surface = DevSurface::Code;
        return Ok(compact_result("glass.editor.proposal.create", &result));
    }
    if action == "accept" || action == "reject" {
        let id = parts.get(1).ok_or("editor proposal action requires ID")?;
        let tool = if action == "accept" {
            "glass.editor.proposal.accept"
        } else {
            "glass.editor.proposal.reject"
        };
        let result = run_tool(state, tool, json!({"id":id}), true)?;
        state.surface = DevSurface::Code;
        state.refresh_editor_projection();
        return Ok(compact_result(tool, &result));
    }
    if action == "checkpoint" {
        let name = parts.get(1..).unwrap_or_default().join(" ");
        if name.is_empty() {
            return Err("editor checkpoint requires NAME".into());
        }
        let result = run_tool(
            state,
            "glass.editor.checkpoint.create",
            json!({"name":name}),
            true,
        )?;
        state.surface = DevSurface::Code;
        return Ok(compact_result("glass.editor.checkpoint.create", &result));
    }
    if action == "restore" {
        let id = parts
            .get(1)
            .ok_or("editor restore requires CHECKPOINT_ID")?;
        let result = run_tool(
            state,
            "glass.editor.checkpoint.restore",
            json!({"id":id}),
            true,
        )?;
        state.surface = DevSurface::Code;
        state.refresh_editor_projection();
        return Ok(compact_result("glass.editor.checkpoint.restore", &result));
    }
    let path = parts.get(1).ok_or("editor action requires PATH")?;
    if matches!(action, "undo" | "redo") {
        require_trusted(state)?;
        let result = if action == "undo" {
            state.ws_mut()?.project_mut().undo_buffer(path)
        } else {
            state.ws_mut()?.project_mut().redo_buffer(path)
        }
        .map_err(|error| error.to_string())?;
        state.surface = DevSurface::Code;
        state.refresh_editor_projection();
        return Ok(format!(
            "{} {} · dirty {}",
            action, result.path, result.dirty
        ));
    }
    if action == "search" {
        let query = parts.get(1..).unwrap_or_default().join(" ");
        let hits = state
            .ws_mut()?
            .project_mut()
            .search(&query, 64)
            .map_err(|error| error.to_string())?;
        state.editor = hits
            .iter()
            .map(|hit| format!("{} · {}", hit.label, hit.detail))
            .collect::<Vec<_>>()
            .join("\n");
        state.surface = DevSurface::Code;
        return Ok(format!("{} search results", hits.len()));
    }
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
        _ => {
            return Err(
                "editor actions: open, selection, replace, replace-selection, save, undo, redo, search, comments, comment, comment-selection, comment-resolve, proposals, propose, accept, reject, checkpoints, checkpoint, restore".into(),
            );
        }
    };
    let result = run_tool(state, tool, arguments, mutating)?;
    state.surface = DevSurface::Code;
    Ok(compact_result(tool, &result))
}

fn execute_lsp(state: &mut DevTuiState, parts: Vec<&str>) -> Result<String, String> {
    let action = parts.first().copied().unwrap_or("list");
    let server = parts.get(1).copied();
    let (tool, arguments, mutating) = match action {
        "start" => {
            let server = server.ok_or("lsp start requires SERVER COMMAND")?;
            let command = parts
                .get(2)
                .ok_or("lsp start requires SERVER COMMAND")?;
            let mut arguments = json!({"server":server,"command":command});
            if let Some(extra) = parts.get(3..)
                && !extra.is_empty()
            {
                arguments["arguments"] = json!(extra);
            }
            ("glass.lsp.start", arguments, true)
        }
        "stop" => ("glass.lsp.stop", json!({"server":server.ok_or("lsp stop requires SERVER")?}), true),
        "list" => ("glass.lsp.list", json!({}), false),
        "events" => ("glass.lsp.events", json!({"since":parts.get(1).and_then(|value| value.parse::<u64>().ok()).unwrap_or(0)}), false),
        "diagnostics" | "symbols" | "format" | "tokens" | "inlay" | "inlays" => {
            let path = parts.get(2).ok_or("lsp action requires SERVER PATH")?;
            let tool = match action {
                "diagnostics" => "glass.lsp.diagnostics",
                "symbols" => "glass.lsp.document_symbols",
                "format" => "glass.lsp.formatting",
                "inlay" | "inlays" => "glass.lsp.inlay_hints",
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
        _ => return Err("lsp actions: start, stop, list, events, diagnostics, hover, complete, definition, declaration, implementation, references, symbols, workspace-symbols, inlay, signature, format, tokens, rename".into()),
    };
    let result = run_tool(state, tool, arguments, mutating)?;
    state.editor = if tool == "glass.lsp.diagnostics" {
        projection::lsp(Some(&result))
    } else {
        projection::first_meaningful(&result)
    };
    state.surface = DevSurface::Code;
    Ok(compact_result(tool, &result))
}

fn execute_process(state: &mut DevTuiState, parts: Vec<&str>) -> Result<String, String> {
    let Some(action) = parts.first().copied() else {
        state.surface = DevSurface::Terminal;
        return Ok("Opened processes".into());
    };
    if action == "list" {
        return execute_named_tool(
            state,
            "glass.process.list",
            json!({}),
            false,
            DevSurface::Terminal,
        );
    }
    if action == "ports" {
        return execute_named_tool(
            state,
            "glass.process.ports",
            json!({}),
            false,
            DevSurface::Terminal,
        );
    }
    let name = parts.get(1).ok_or("process action requires NAME")?;
    let detected_command = (action == "start"
        && parts
            .get(2..)
            .is_none_or(|values| values.iter().all(|value| value.is_empty())))
    .then(|| state.ws().ok()?.project().detection().dev_command.clone())
    .flatten();
    let (tool, arguments, mutating) = match action {
        "start" => {
            let command = parts.get(2..).unwrap_or_default().join(" ");
            let command = if command.trim().is_empty() {
                detected_command
                    .as_deref()
                    .ok_or("no detected dev command; use process start NAME COMMAND")?
            } else {
                command.as_str()
            };
            (
                "glass.process.start",
                json!({"name":name,"command":command}),
                true,
            )
        }
        "stop" => ("glass.process.stop", json!({"name":name}), true),
        "restart" => ("glass.process.restart", json!({"name":name}), true),
        "remove" => ("glass.process.remove", json!({"name":name}), true),
        "logs" | "output" => ("glass.process.logs", json!({"name":name}), false),
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
        _ => {
            return Err(
                "process actions: list, start, stop, restart, remove, logs, output, input, resize, health, ports"
                    .into(),
            );
        }
    };
    execute_named_tool(state, tool, arguments, mutating, DevSurface::Terminal)
}

fn execute_app(state: &mut DevTuiState, parts: Vec<&str>) -> Result<String, String> {
    match parts.first().copied().unwrap_or("open") {
        "open" | "attach" => state.attach_detected_app(),
        "source" => {
            state.jump_source_from_page();
            Ok(state.status.clone())
        }
        "page" => {
            state.jump_page_from_source();
            Ok(state.status.clone())
        }
        _ => Err("app actions: open, attach, source, page".into()),
    }
}

fn execute_browser(
    state: &mut DevTuiState,
    command: &str,
    parts: Vec<&str>,
) -> Result<String, String> {
    let Some(action) = parts.first().copied() else {
        state.surface = DevSurface::App;
        return Ok(format!("Opened resident {command}"));
    };
    let (tool, arguments, mutating) = if command == "workflow" {
        if action == "record" {
            return execute_workflow_record(state, &parts[1..]);
        }
        match action {
            "run" => ("glass.workflow.run", json!({"definition":read_project_json(state, parts.get(1).ok_or("workflow run requires DEFINITION.json")?)?,"inputs":parse_inline_json(parts.get(2), json!({}))?}), true),
            "pause" => ("glass.workflow.pause", json!({}), true),
            "resume" => ("glass.workflow.resume", json!({"definition":read_project_json(state, parts.get(1).ok_or("workflow resume requires DEFINITION.json CHECKPOINT.json")?)?,"checkpoint":read_project_json(state, parts.get(2).ok_or("workflow resume requires DEFINITION.json CHECKPOINT.json")?)?,"inputs":parse_inline_json(parts.get(3), json!({}))?}), true),
            "list" => ("glass.workflow.list", json!({}), false),
            "cancel" => ("glass.workflow.cancel", json!({}), true),
            "verify" => ("glass.workflow.verify", json!({}), false),
            _ => return Err("workflow actions: list, run DEFINITION.json [INPUTS_JSON], pause, resume DEFINITION.json CHECKPOINT.json [INPUTS_JSON], cancel, verify, record start [NAME]".into()),
        }
    } else {
        if matches!(action, "human" | "takeover") {
            let _ = state
                .browser_workspace
                .reduce(glass_browser::browser_workspace::BrowserWorkspaceIntent::TakeHumanControl);
            state.browser = state.browser_workspace_summary();
            state.surface = DevSurface::App;
            return Ok("Human browser control acquired · agent mutation paused".into());
        }
        if matches!(action, "release" | "reconcile") {
            state.browser_workspace.reconcile_takeover();
            state.browser = state.browser_workspace_summary();
            state.surface = DevSurface::App;
            return Ok("Browser checkpoint reconciled · control returned to Glass".into());
        }
        if matches!(action, "targets" | "target") {
            let query = parts.get(1..).unwrap_or_default().join(" ");
            state.request_browser_target_picker(query)?;
            state.surface = DevSurface::App;
            return Ok("Loading browser targets…".into());
        }
        let browser_state = state.browser_workspace.state();
        let visible_revision = browser_state.browser_revision.unwrap_or(0);
        if action == "navigate" && (visible_revision == 0 || browser_state.semantic_invalidated) {
            let url = parts.get(1).ok_or("browser navigate requires URL")?;
            return state.prepare_browser_navigation(url);
        }
        if matches!(
            action,
            "back" | "forward" | "reload" | "stop-loading" | "click" | "type" | "scroll"
        ) && visible_revision == 0
        {
            return Err("start and observe the browser before a revision-bound action".into());
        }
        match action {
            "start" => {
                let port = parts
                    .get(1)
                    .and_then(|value| value.parse::<u16>().ok())
                    .unwrap_or(9222);
                let attach = parts.contains(&"--attach");
                let incognito = parts.contains(&"--incognito");
                let headed = !parts.contains(&"--headless");
                let chrome_path = parts
                    .get(2..)
                    .unwrap_or_default()
                    .iter()
                    .copied()
                    .find(|value| !value.starts_with("--"));
                (
                    "glass.browser.start",
                    json!({
                        "port": port,
                        "attach": attach,
                        "incognito": incognito,
                        "headed": headed,
                        "chromePath": chrome_path
                    }),
                    true,
                )
            }
            "stop" => ("glass.browser.stop", json!({}), true),
            "state" => ("glass.browser.state", json!({}), false),
            "observe" => ("glass.browser.observe", json!({}), false),
            "web-ir" => ("glass.browser.web_ir", json!({}), false),
            "targets" => ("glass.browser.targets", json!({}), false),
            "select" => ("glass.browser.target.select", json!({"targetId":parts.get(1).ok_or("browser select requires TARGET_ID")?}), true),
            "navigate" => ("glass.browser.navigate", json!({"url":parts.get(1).ok_or("browser navigate requires URL")?,"browserRevision":visible_revision}), true),
            "back" | "forward" | "reload" | "stop-loading" => ("glass.browser.act", json!({"action":if action == "stop-loading" { "stopLoading" } else { action },"browserRevision":visible_revision}), true),
            "click" => {
                let (target, revision) = {
                    let selected = state.browser_workspace.state().selected();
                    let target = parts.get(1).copied().or_else(|| selected.map(|entity| entity.reference.as_str())).ok_or("browser click requires a target or semantic selection")?;
                    let revision = selected.filter(|entity| entity.reference == target).map(|entity| entity.revision).unwrap_or(visible_revision);
                    (target.to_string(), revision)
                };
                let _ = state.capture_workflow_click();
                ("glass.browser.act", json!({"action":"click","target":target,"browserRevision":revision}), true)
            },
            "type" => {
                let (target, revision, input_name) = {
                    let selected = state.browser_workspace.state().selected();
                    let target = parts.get(1).copied().or_else(|| selected.map(|entity| entity.reference.as_str())).ok_or("browser type requires a target or semantic selection")?;
                    let revision = selected.filter(|entity| entity.reference == target).map(|entity| entity.revision).unwrap_or(visible_revision);
                    let input_name = selected.map(|entity| entity.name.clone());
                    (target.to_string(), revision, input_name)
                };
                if state.workflow_recording.is_some() {
                    let _ = state.record_workflow_type(input_name.as_deref().unwrap_or("value"));
                }
                ("glass.browser.act", json!({"action":"type","target":target,"browserRevision":revision,"text":parts.get(2..).unwrap_or_default().join(" ")}), true)
            },
            "scroll" => ("glass.browser.act", json!({"action":"scroll","dx":parts.get(1).and_then(|value| value.parse::<f64>().ok()).unwrap_or(0.0),"dy":parts.get(2).and_then(|value| value.parse::<f64>().ok()).unwrap_or(600.0),"browserRevision":visible_revision}), true),
            "screenshot" => ("glass.browser.screenshot", json!({}), false),
            "remote-open" => ("glass.browser.remote-view.open", json!({}), true),
            "remote-status" => ("glass.browser.remote-view.status", json!({}), false),
            "remote-revoke" => ("glass.browser.remote-view.revoke", json!({}), true),
            _ => return Err("browser actions: start, stop, state, observe, web-ir, targets, select, navigate, back, forward, reload, stop-loading, click, type, scroll, screenshot, remote-open, remote-status, remote-revoke".into()),
        }
    };
    let result = run_tool(state, tool, arguments, mutating)?;
    state.apply_browser_result(tool, &result);
    state.browser_detail = projection::browser_result(tool, &result);
    state.surface = DevSurface::App;
    Ok(compact_result(tool, &result))
}

fn read_project_json(state: &mut DevTuiState, path: &str) -> Result<Value, String> {
    let content = state
        .ws_mut()?
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
        .ws_mut()?
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
            state.surface = DevSurface::Debug;
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
                "inspect" => ("glass.debug.inspect", json!({"session":session}), false),
                "processes" => ("glass.debug.processes", json!({"session":session}), false),
                "watch" | "console" => ("glass.debug.evaluate", json!({"session":session,"expression":parts.get(2..).unwrap_or_default().join(" "),"context":if action == "watch" { "watch" } else { "repl" }}), false),
                "stop" => ("glass.debug.stop", json!({"session":session}), true),
                _ => return Err("debug actions: start, launch, attach, configured, break, continue, pause, step, threads, stack, scopes, variables, evaluate, watch, console, events, inspect, processes, stop".into()),
            };
            run_tool(state, tool, arguments, mutating)?
        }
    };
    state.debugger = projection::debugger(Some(&result));
    state.surface = DevSurface::Debug;
    Ok(compact_result("debug", &result))
}

fn execute_kernel(state: &mut DevTuiState, parts: Vec<&str>) -> Result<String, String> {
    require_trusted(state)?;
    let action = parts
        .first()
        .copied()
        .ok_or("kernel requires start, execute, cancel, reset, or stop")?;
    let name = parts.get(1).ok_or("kernel action requires NAME")?;
    let (tool, arguments) = match action {
        "start" => {
            let kind = match parts.get(2).copied().ok_or("kernel start requires KIND")? {
                "python" => "python",
                "javascript" => "javascript",
                "shell" => "shell",
                "sql" => "sql",
                _ => return Err("kernel kind must be python, javascript, shell, or sql".into()),
            };
            (
                "glass.eval.start",
                json!({"name":name,"kind":kind,"capabilities":parts.get(3..).unwrap_or_default()}),
            )
        }
        "execute" => (
            "glass.eval.execute",
            json!({"name":name,"code":parts.get(2..).unwrap_or_default().join(" ")}),
        ),
        "cancel" => ("glass.eval.cancel", json!({"name":name})),
        "reset" => ("glass.eval.reset", json!({"name":name})),
        "stop" => ("glass.eval.stop", json!({"name":name})),
        _ => return Err("kernel actions: start, execute, cancel, reset, stop".into()),
    };
    let result = run_tool(state, tool, arguments, true)?;
    state.kernels = projection::kernels(Some(&result));
    state.surface = DevSurface::More;
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
        "discard" => (
            "glass.git.discard",
            json!({"paths":parts.get(1..).unwrap_or_default()}),
            true,
        ),
        "push" => (
            "glass.git.push",
            json!({"remote":parts.get(1),"branch":parts.get(2)}),
            true,
        ),
        "fetch" => ("glass.git.fetch", json!({"remote":parts.get(1)}), true),
        "pull" => (
            "glass.git.pull",
            json!({"remote":parts.get(1),"branch":parts.get(2)}),
            true,
        ),
        "merge" => (
            "glass.git.merge",
            json!({"branch":parts.get(1).ok_or("git merge requires BRANCH")?}),
            true,
        ),
        "rebase" => (
            "glass.git.rebase",
            json!({"onto":parts.get(1).ok_or("git rebase requires ONTO")?}),
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
                "git actions: status, diff, stage, unstage, discard, commit, push, fetch, pull, merge, rebase, branches, switch".into(),
            );
        }
    };
    let result = run_tool(state, tool, arguments, mutating)?;
    if tool.contains("git") {
        state.git = format!("{}\n{}", state.git, projection::git(Some(&result)));
    }
    state.surface = DevSurface::Git;
    Ok(compact_result(tool, &result))
}

fn execute_test(state: &mut DevTuiState, parts: Vec<&str>) -> Result<String, String> {
    let Some(action) = parts.first().copied() else {
        state.surface = DevSurface::Tasks;
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
    state.tests = projection::tests(Some(&result));
    state.surface = DevSurface::Tasks;
    Ok(compact_result(tool, &result))
}

fn execute_experiment(state: &mut DevTuiState, parts: Vec<&str>) -> Result<String, String> {
    let Some(action) = parts.first().copied() else {
        state.surface = DevSurface::More;
        return Ok("Opened experiments".into());
    };
    require_trusted(state)?;
    let mut workspace = state.ws_mut()?;
    let experiments = workspace.experiments().map_err(|error| error.to_string())?;
    let mut comparison = None;
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
            comparison = Some(experiments.compare());
            "Compared experiment evidence".into()
        }
        "collect" => {
            let id = parts.get(1).ok_or("experiment collect requires ID")?;
            let evidence = experiments
                .collect_automatic(id)
                .map_err(|error| error.to_string())?;
            format!(
                "Collected {} measured experiment metrics for {id}",
                evidence
                    .provenance
                    .values()
                    .filter(|provenance| provenance.measured && provenance.available)
                    .count()
            )
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
        _ => return Err("experiment actions: create, collect, compare, select, remove".into()),
    };
    drop(workspace);
    if let Some(comparison) = comparison {
        state.experiment_comparison = Some(comparison);
    }
    state.surface = DevSurface::More;
    Ok(message)
}

fn run_tool(
    state: &mut DevTuiState,
    name: &str,
    arguments: Value,
    mutating: bool,
) -> Result<Value, String> {
    let workspace = state.ws()?;
    let expected_generation = workspace.generation();
    let expected_project_revision = workspace.project().revision();
    drop(workspace);
    let context = DevelopmentToolContext {
        authorization: ToolAuthorization {
            actor: Actor::local(),
            allow_mutation: mutating,
            confirmed: mutating,
            unrestricted: state.yolo_mode,
        },
        initiator: None,
        expected_generation,
        expected_project_revision,
    };
    let call = ToolCall {
        id: format!("tui-{}", NEXT_TUI_TOOL.fetch_add(1, Ordering::Relaxed)),
        name: name.into(),
        arguments,
    };
    if mutating && state.yolo_mode {
        if state.background_action_running() {
            return Err("another tool action is already awaiting or running".into());
        }
        state.queue_tool_request(call, context)?;
        return Ok(json!({"queued": true, "tool": name, "yolo": true}));
    }
    if mutating && !state.yolo_mode {
        if state.background_action_running() {
            return Err("another tool action is already awaiting or running".into());
        }
        let summary = format!(
            "{} · {}",
            name,
            call.arguments
                .as_object()
                .map(|object| object.keys().cloned().collect::<Vec<_>>().join(", "))
                .unwrap_or_else(|| "no arguments".into())
        );
        state.pending_confirmation = Some(super::state::PendingConfirmation {
            call,
            context,
            summary: summary.clone(),
        });
        return Ok(json!({"confirmationRequired":true,"summary":summary}));
    }
    state.queue_tool_request(call, context)?;
    Ok(json!({"queued": true, "tool": name}))
}

fn compact_result(tool: &str, value: &Value) -> String {
    if value.get("confirmationRequired").and_then(Value::as_bool) == Some(true) {
        return format!("{tool} · one-use confirmation required");
    }
    if value.get("queued").and_then(Value::as_bool) == Some(true) {
        return format!("{tool} · queued in background");
    }
    let projection = match tool {
        name if name.starts_with("glass.browser") => super::projection::browser_result(tool, value),
        name if name.starts_with("glass.git") => super::projection::git(Some(value)),
        name if name.starts_with("glass.test") => super::projection::tests(Some(value)),
        name if name.starts_with("glass.lsp") => super::projection::lsp(Some(value)),
        name if name.starts_with("glass.debug") => super::projection::debugger(Some(value)),
        name if name.starts_with("glass.kernel") => super::projection::kernels(Some(value)),
        _ => super::projection::first_meaningful(value),
    };
    format!(
        "{tool} · {}",
        projection.lines().next().unwrap_or("completed")
    )
}

fn require_trusted(state: &DevTuiState) -> Result<(), String> {
    if state.snapshot_trust_label == "untrusted" {
        Err(
            "repository-controlled execution is blocked; inspect and trust the workspace first"
                .into(),
        )
    } else {
        Ok(())
    }
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
            "trust" => Some(DevSurface::Trust),
            "home" => Some(DevSurface::Agent),
            "agent" => Some(DevSurface::Agent),
            "code" | "editor" | "lsp" => Some(DevSurface::Code),
            "app" | "browser" | "workflow" => Some(DevSurface::App),
            "task" => Some(DevSurface::Tasks),
            "terminal" | "process" => Some(DevSurface::Terminal),
            "debug" => Some(DevSurface::Debug),
            "git" => Some(DevSurface::Git),
            "test" => Some(DevSurface::Tasks),
            "experiment" | "kernel" | "workspace" | "daemon" | "replay" => Some(DevSurface::More),
            _ => None,
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use glass_browser::cli::args::TuiLayout;
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn test_state(label: &str) -> (DevTuiState, PathBuf) {
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock")
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "glass-tui-command-{label}-{}-{suffix}",
            std::process::id()
        ));
        fs::create_dir_all(&root).expect("create test root");
        let mut state = DevTuiState::open_for_tui(&root, TuiLayout::Desktop).expect("open state");
        state
            .ws_mut()
            .expect("workspace lock")
            .apply_local_trust_decision(crate::LocalTrustDecision::TrustProject)
            .expect("trust project");
        state.snapshot_trust_label = "trusted-project".into();
        (state, root)
    }

    #[test]
    fn every_major_surface_has_a_palette_route() {
        for surface in DevSurface::ALL {
            assert_eq!(parse_surface(surface.label()), Some(surface));
        }
    }

    #[test]
    fn detached_navigation_guides_browser_start_before_url() {
        let (mut state, root) = test_state("detached-navigation");
        let output = execute(&mut state, "browser navigate google.com")
            .expect("detached navigation should queue startup");
        assert!(output.contains("Browser detached"));
        assert_eq!(
            state.pending_browser_navigation.as_deref(),
            Some("google.com")
        );
        let pending = state
            .pending_confirmation
            .as_ref()
            .expect("browser startup confirmation");
        assert_eq!(pending.call.name, "glass.browser.start");
        state.deny_confirmation();
        assert!(state.pending_browser_navigation.is_none());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn connected_navigation_refreshes_before_revision_bound_action() {
        let (mut state, root) = test_state("connected-navigation");
        state.browser_workspace.connected(true, None, Some(1));
        let output = execute(&mut state, "browser navigate google.com")
            .expect("connected navigation should refresh first");
        assert!(output.contains("refreshing page"));
        assert_eq!(
            state.pending_browser_navigation.as_deref(),
            Some("google.com")
        );
        let (call, _) = state
            .queued_tool_request
            .take()
            .expect("fresh browser observation");
        assert_eq!(call.name, "glass.browser.observe");
        assert!(state.pending_confirmation.is_none());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn search_open_and_doctor_are_first_class_palette_routes() {
        let (mut state, root) = test_state("cli-parity");
        state.files = vec!["src/lib.rs".into()];
        std::fs::create_dir_all(root.join("src")).unwrap();
        std::fs::write(root.join("src/lib.rs"), "fn lib() {}\n").unwrap();

        execute(&mut state, "open").expect("open without path opens picker");
        assert!(state.file_picker_open);
        state.close_file_picker();

        execute(&mut state, "open src/lib.rs").expect("open PATH");
        assert_eq!(state.focused_editor_path, "src/lib.rs");

        let error = execute(&mut state, "search").expect_err("search needs a query");
        assert!(error.contains("project search requires QUERY"));

        execute(&mut state, "doctor").expect("doctor aliases agent doctor");
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn help_explains_the_guided_command_center() {
        let (mut state, root) = test_state("help");
        let output = execute(&mut state, "help").expect("help route");
        assert!(output.contains("`:actions` opens guided Agent launchers"));
        assert!(output.contains("Agent: agent"));
        assert!(output.contains("Build: project · editor · github · lsp · git"));
        let error = execute(&mut state, "not-a-route").expect_err("unknown route");
        assert!(error.contains("press : for guided launchers"));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn prefixed_browser_control_routes_replace_bare_keys() {
        let (mut state, root) = test_state("browser-control-prefix");

        let output = execute(&mut state, "browser human").expect("human control route");
        assert!(output.contains("Human browser control"));
        assert_eq!(
            state.browser_workspace.state().input_owner,
            glass_browser::browser_workspace::BrowserInputOwner::Human
        );

        let output = execute(&mut state, "browser release").expect("release control route");
        assert!(output.contains("control returned to Glass"));
        assert_eq!(
            state.browser_workspace.state().input_owner,
            glass_browser::browser_workspace::BrowserInputOwner::Glass
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn yolo_prefixed_mutations_skip_the_confirmation_sheet() {
        let (mut state, root) = test_state("yolo-prefix");
        state.yolo_mode = true;

        let output = execute(&mut state, "agent update").expect("queue yolo update");

        assert!(output.contains("queued"));
        assert!(state.pending_confirmation.is_none());
        assert_eq!(
            state
                .queued_tool_request
                .as_ref()
                .map(|(call, _)| call.name.as_str()),
            Some("glass.agent.setup")
        );
        let _ = fs::remove_dir_all(root);
    }
    #[test]
    fn quit_route_requests_confirmation_without_exiting() {
        let (mut state, root) = test_state("quit");
        let output = execute(&mut state, "quit").expect("quit route");
        assert!(output.contains("Enter exits"));
        assert!(state.quit_confirmation);
        assert!(!state.quit);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn palette_order_starts_with_current_surface_launchers() {
        assert_eq!(palette_order(DevSurface::Agent)[0], "agent");
        assert_eq!(palette_order(DevSurface::Terminal)[0], "process");
        assert_eq!(palette_order(DevSurface::Code)[0], "editor");
        assert_eq!(palette_order(DevSurface::Tasks)[0], "task");
    }
    #[test]
    fn palette_order_scopes_routes_to_current_surface_and_shared_roots() {
        let git = palette_order(DevSurface::Git);
        for root in ["git", "github", "agent", "review", "help", "quit", "view"] {
            assert!(git.contains(&root), "Git palette missing {root}");
        }
        for root in [
            "editor", "project", "lsp", "browser", "process", "task", "test", "debug",
        ] {
            assert!(!git.contains(&root), "Git palette leaked {root}");
        }

        let code = palette_order(DevSurface::Code);
        for root in [
            "project", "editor", "lsp", "agent", "review", "help", "quit", "view",
        ] {
            assert!(code.contains(&root), "Code palette missing {root}");
        }
        for root in ["git", "github", "browser", "process", "debug"] {
            assert!(!code.contains(&root), "Code palette leaked {root}");
        }

        let trust = palette_order(DevSurface::Trust);
        for root in ["trust", "help", "quit", "view"] {
            assert!(trust.contains(&root), "Trust palette missing {root}");
        }
        assert!(!trust.contains(&"agent"));
        assert!(!trust.contains(&"review"));
    }

    #[test]
    fn shared_palette_routes_still_switch_surfaces() {
        let (mut state, root) = test_state("surface-route");
        state.surface = DevSurface::Git;
        execute(&mut state, "agent").expect("agent route");
        assert_eq!(state.surface, DevSurface::Agent);
        state.surface = DevSurface::Code;
        execute(&mut state, "review").expect("review route");
        assert_eq!(state.surface, DevSurface::Git);
        assert!(state.git_diff.starts_with("REVIEW"));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn agent_prompt_without_id_queues_native_interactive_send() {
        let (mut state, root) = test_state("agent");
        execute(&mut state, "agent prompt inspect workspace").expect("queue agent prompt");
        let pending = state.pending_confirmation.take().expect("confirmation");
        assert_eq!(pending.call.name, "glass.agent.send");
        assert_eq!(pending.call.arguments["text"], "inspect workspace");
        assert!(pending.call.arguments.get("agentId").is_none());
        assert_eq!(
            pending.context.expected_generation,
            state.ws().expect("workspace lock").generation()
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn agent_update_routes_forced_pinned_sdk_refresh() {
        let (mut state, root) = test_state("agent-update");
        let output = execute(&mut state, "agent update").expect("queue Pi update");
        assert!(output.contains("confirmation"));
        let pending = state.pending_confirmation.take().expect("confirmation");
        assert_eq!(pending.call.name, "glass.agent.setup");
        assert_eq!(pending.call.arguments["login"], false);
        assert_eq!(pending.call.arguments["update"], true);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn review_route_prefills_evidence_aware_agent_prompt() {
        let (mut state, root) = test_state("review");
        let output = execute(&mut state, "review").expect("review route");
        assert!(output.contains("Review object ready"));
        assert!(state.git_diff.starts_with("REVIEW"));
        assert!(state.git_diff.contains("PROPOSALS"));
        assert_eq!(state.surface, DevSurface::Git);
        let asked = execute(&mut state, "review ask").expect("review ask");
        assert!(asked.contains("Review prompt prepared"));
        assert!(state.composer_mode);
        assert!(state.composer_input.contains("Git diff"));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn review_ship_attaches_the_review_object_body() {
        let (mut state, root) = test_state("review-ship");
        execute(&mut state, "review").expect("open review");
        execute(&mut state, "review ship overnight-crew").expect("queue review ship");
        let ship = state
            .pending_confirmation
            .take()
            .expect("ship confirmation");
        assert_eq!(ship.call.name, "glass.github.ship");
        assert_eq!(ship.call.arguments["title"], "overnight-crew");
        assert!(
            ship.call.arguments["body"]
                .as_str()
                .expect("ship body")
                .contains("PROPOSALS")
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn task_crew_queues_the_factory_loop() {
        let (mut state, root) = test_state("task-crew");
        execute(&mut state, "task crew add settings toggle").expect("queue crew");
        let call = state
            .pending_confirmation
            .as_ref()
            .map(|item| &item.call)
            .or_else(|| state.queued_tool_request.as_ref().map(|(call, _)| call))
            .expect("crew request");
        assert_eq!(call.name, "glass.task.crew");
        assert_eq!(call.arguments["goal"], "add settings toggle");
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn review_object_includes_a_persisted_crew_wake() {
        let (mut state, root) = test_state("review-wake");
        state.last_crew_wake = Some("WAKE crew-1\n  goal add settings toggle".into());
        execute(&mut state, "review").expect("open review");
        assert!(state.git_diff.contains("WAKE crew-1"));
        assert!(state.git_diff.contains("goal add settings toggle"));
        state.last_crew_wake = Some(
            "WAKE crew-1\n  goal add settings toggle\n  accept proposal-1\n\nVERIFY\n  PROOF ✓"
                .into(),
        );
        execute(&mut state, "review").expect("open packed review");
        assert!(state.git_diff.contains("VERIFY"));
        assert!(state.git_diff.contains("accept proposal-1"));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn app_open_attaches_the_detected_loopback_url() {
        let (mut state, root) = test_state("app-open");
        state.process_urls = vec!["http://localhost:3000/".into()];
        execute(&mut state, "app open").expect("queue app open");
        assert_eq!(
            state.pending_browser_navigation.as_deref(),
            Some("http://localhost:3000")
        );
        assert_eq!(state.surface, DevSurface::App);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn app_page_uses_the_handler_route() {
        let (mut state, root) = test_state("app-page");
        state.focused_editor_path = "app/settings/page.tsx".into();
        state.focused_editor_line = 1;
        state.process_urls = vec!["http://127.0.0.1:5173/".into()];
        execute(&mut state, "app page").expect("jump to page");
        assert_eq!(
            state.pending_browser_navigation.as_deref(),
            Some("http://127.0.0.1:5173/settings")
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn lsp_inlay_routes_to_the_resident_tool() {
        let (mut state, root) = test_state("lsp-inlay");
        execute(&mut state, "lsp inlay rust-analyzer src/lib.rs").expect("queue inlay");
        let (call, _) = state.queued_tool_request.take().expect("inlay request");
        assert_eq!(call.name, "glass.lsp.inlay_hints");
        assert_eq!(call.arguments["server"], "rust-analyzer");
        assert_eq!(call.arguments["path"], "src/lib.rs");
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn harness_list_route_exposes_the_external_bridge_without_launching() {
        let (mut state, root) = test_state("harness-list");
        let output = execute(&mut state, "harness list").expect("harness list route");
        assert!(output.contains("External harnesses"));
        assert!(output.contains("harness start NAME"));
        assert!(output.contains("harness delegate NAME PROMPT"));
        assert_eq!(state.surface, DevSurface::Agent);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn harness_delegate_route_matches_cli_request_and_authority_rules() {
        let (mut state, root) = test_state("harness-delegate");
        let output = execute(
            &mut state,
            "harness delegate codex inspect current diff --sandbox read-only --timeout-secs 30",
        )
        .expect("queue read-only external delegation");
        assert!(output.contains("confirmation"));
        let pending = state
            .pending_confirmation
            .take()
            .expect("delegation confirmation");
        assert_eq!(pending.call.name, "glass.agent.delegate");
        assert_eq!(pending.call.arguments["harness"], "codex");
        assert_eq!(pending.call.arguments["prompt"], "inspect current diff");
        assert_eq!(pending.call.arguments["sandbox"], "read-only");
        assert_eq!(pending.call.arguments["timeoutSeconds"], 30);

        let error = execute(
            &mut state,
            "harness delegate codex update files --sandbox workspace-write",
        )
        .expect_err("workspace-write must require explicit flags");
        assert!(error.contains("--allow-mutation and --yes"));
        let output = execute(
            &mut state,
            "harness delegate codex update files --sandbox workspace-write --allow-mutation --yes",
        )
        .expect("queue authorized workspace-write delegation");
        assert!(output.contains("confirmation"));
        let pending = state
            .pending_confirmation
            .take()
            .expect("workspace-write confirmation");
        assert_eq!(pending.call.arguments["sandbox"], "workspace-write");
        let _ = fs::remove_dir_all(root);
    }
    #[test]
    fn cockpit_routes_start_status_and_stop_without_leaving_the_tui() {
        let (mut state, root) = test_state("cockpit");
        let started = execute(&mut state, "cockpit start").expect("start cockpit");
        assert!(started.starts_with("Private cockpit ready · http://127.0.0.1:"));
        let status = execute(&mut state, "cockpit status").expect("cockpit status");
        assert!(status.contains("running · http://127.0.0.1:"));
        execute(&mut state, "cockpit stop").expect("stop cockpit");
        assert!(state.private_cockpit.is_none());
        assert!(state.private_cockpit_status().contains("not running"));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn workflow_record_start_stop_writes_a_locator_draft() {
        let (mut state, root) = test_state("workflow-record");
        state.browser_workspace.replace_entities(
            3,
            vec![glass_browser::browser_workspace::BrowserWorkspaceEntity {
                reference: "r3:continue".into(),
                role: "button".into(),
                name: "Continue".into(),
                actionable: true,
                revision: 3,
            }],
        );
        let started = execute(&mut state, "workflow record start checkout").expect("start");
        assert!(started.contains("Recording checkout"));
        assert_eq!(state.surface, DevSurface::App);
        state.queue_browser_intent(
            glass_browser::browser_workspace::BrowserWorkspaceIntent::ActivateSelected,
        );
        let stopped = execute(&mut state, "workflow record stop").expect("stop");
        assert!(stopped.contains("1 step"));
        let draft =
            fs::read_to_string(root.join(".glass/workflows/checkout.draft.json")).expect("draft");
        assert!(draft.contains("role=button;name=Continue"));
        let _ = fs::remove_dir_all(root);
    }
    #[test]
    fn github_routes_review_and_confirmation_gated_ship() {
        let (mut state, root) = test_state("github-routes");
        execute(&mut state, "github review").expect("queue GitHub review");
        let review = state
            .queued_tool_request
            .take()
            .expect("GitHub review request");
        assert_eq!(review.0.name, "glass.github.review");
        execute(&mut state, "github ship release").expect("queue GitHub ship");
        let ship = state
            .pending_confirmation
            .take()
            .expect("ship confirmation");
        assert_eq!(ship.call.name, "glass.github.ship");
        assert_eq!(ship.call.arguments["title"], "release");
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn tool_request_uses_cached_revisions_while_workspace_refreshes() {
        let (state, root) = test_state("busy");
        let workspace = state.workspace.clone();
        let _guard = workspace.lock().expect("hold workspace lock");
        let (_, context) = state
            .tool_request("glass.agent.send", json!({"text": "inspect"}), true)
            .expect("chat request stays responsive during refresh");
        assert_eq!(context.expected_generation, state.snapshot_generation);
        assert_eq!(
            context.expected_project_revision,
            state.snapshot_project_revision
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn process_ports_and_task_list_need_no_placeholder_name_or_id() {
        let (mut state, root) = test_state("read-routes");
        execute(&mut state, "process ports").expect("queue process ports");
        let process_call = state.queued_tool_request.take().expect("process request");
        assert_eq!(process_call.0.name, "glass.process.ports");
        execute(&mut state, "task list").expect("queue task list");
        let task_call = state.queued_tool_request.take().expect("task request");
        assert_eq!(task_call.0.name, "glass.task.list");
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn editor_collaboration_commands_use_the_focused_buffer_and_selection() {
        let (mut state, root) = test_state("editor-collaboration");
        fs::create_dir_all(root.join("src")).expect("create source directory");
        fs::write(root.join("src/main.rs"), "fn main() {}\n").expect("write source");
        state
            .ws_mut()
            .expect("workspace lock")
            .project_mut()
            .open_buffer("src/main.rs", crate::development::Actor::local())
            .expect("open editor buffer");
        state
            .ws_mut()
            .expect("workspace lock")
            .project_mut()
            .set_buffer_selection(
                "src/main.rs",
                Some(crate::development::TextSelection {
                    anchor: crate::development::TextPosition { line: 1, column: 1 },
                    active: crate::development::TextPosition { line: 1, column: 3 },
                }),
                crate::development::Actor::local(),
            )
            .expect("set editor selection");
        state.refresh_editor_projection();

        execute(&mut state, "editor replace-selection fn").expect("queue selection replacement");
        let replacement = state
            .pending_confirmation
            .take()
            .expect("replacement confirmation");
        assert_eq!(replacement.call.name, "glass.editor.replace_selection");
        assert_eq!(replacement.call.arguments["path"], "src/main.rs");
        assert_eq!(replacement.call.arguments["replacement"], "fn");

        execute(&mut state, "editor comment-selection simplify this")
            .expect("queue selection comment");
        let comment = state
            .pending_confirmation
            .take()
            .expect("comment confirmation");
        assert_eq!(comment.call.name, "glass.editor.comment.add");
        assert_eq!(comment.call.arguments["startLine"], 1);
        assert_eq!(comment.call.arguments["endLine"], 1);
        assert_eq!(comment.call.arguments["text"], "simplify this");
        execute(
            &mut state,
            "editor propose src/main.rs add-output fn main() { println!(\"ok\"); }",
        )
        .expect("queue editor proposal");
        let proposal = state
            .pending_confirmation
            .take()
            .expect("proposal confirmation");
        assert_eq!(proposal.call.name, "glass.editor.proposal.create");
        assert_eq!(proposal.call.arguments["path"], "src/main.rs");
        assert_eq!(proposal.call.arguments["summary"], "add-output");
        assert_eq!(proposal.call.arguments["original"], "fn main() {}\n");

        execute(&mut state, "editor checkpoint before-agent").expect("queue editor checkpoint");
        let checkpoint = state
            .pending_confirmation
            .take()
            .expect("checkpoint confirmation");
        assert_eq!(checkpoint.call.name, "glass.editor.checkpoint.create");
        assert_eq!(checkpoint.call.arguments["name"], "before-agent");

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn every_route_family_queues_the_shared_resident_tool() {
        let routes = [
            ("workspace", "workspace list", "glass.workspace.list"),
            ("project", "project files", "glass.file.list"),
            ("agent", "agent hello", "glass.agent.hello"),
            ("task", "task list", "glass.task.list"),
            (
                "editor",
                "editor selection src/lib.rs",
                "glass.editor.selection",
            ),
            ("lsp", "lsp list", "glass.lsp.list"),
            ("process", "process ports", "glass.process.ports"),
            ("browser", "browser state", "glass.browser.state"),
            ("workflow", "workflow list", "glass.workflow.list"),
            ("debug", "debug threads session", "glass.debug.threads"),
            ("git", "git status", "glass.git.status"),
            ("test", "test discover", "glass.test.discover"),
            ("replay", "replay list", "glass.replay.list"),
            ("memory", "memory status", "glass.memory.retrieve"),
            ("surface", "surface inspect", "glass.semantic.links"),
            ("backend", "backend status", "glass.capabilities.inspect"),
            ("generic", "tool glass.lsp.events {}", "glass.lsp.events"),
        ];
        for (label, input, expected_tool) in routes {
            let (mut state, root) = test_state(label);
            execute(&mut state, input).unwrap_or_else(|error| {
                panic!("{input} should route through the resident tool gateway: {error}")
            });
            let (call, _) = state
                .queued_tool_request
                .take()
                .unwrap_or_else(|| panic!("{input} did not queue a tool"));
            assert_eq!(call.name, expected_tool, "route {input}");
            let _ = fs::remove_dir_all(root);
        }
    }
}
