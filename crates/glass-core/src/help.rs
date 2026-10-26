//! Small, dependency-free CLI help. Pipes are plain; terminal rows fit their width.
use std::io::{self, IsTerminal, Write};

type Item = (&'static str, &'static str);
const START: &[Item] = &[
    ("gc", "Start a conversation in the current directory."),
    (
        "gc resume [ID]",
        "Choose a conversation to resume, or supply its ID.",
    ),
    (
        "gc run <prompt>",
        "Complete a task without the interactive UI.",
    ),
];
const SESSIONS: &[Item] = &[
    (
        "gc hub [--json]",
        "Search projects, conversations, branch families and unread events.",
    ),
    (
        "gc handoff import FILE [--apply]",
        "Preview portable context, then explicitly create a receiving conversation.",
    ),
    (
        "gc attention ID",
        "Find recorded failures and teammate event targets.",
    ),
    (
        "gc history ID",
        "Inspect compaction boundaries and branch lineage.",
    ),
    (
        "gc handoff ID [--json]",
        "Preview bounded context with omitted-history references.",
    ),
    (
        "gc sessions [--json]",
        "List conversations in this project.",
    ),
    (
        "gc search <pattern>",
        "Find saved conversations by their messages.",
    ),
    (
        "gc context ID",
        "Inspect loaded instructions and request diagnostics.",
    ),
    (
        "gc review ID",
        "Review recorded commands and workspace changes.",
    ),
    ("gc details ID", "Read complete recorded tool output."),
    ("gc export ID", "Export a conversation to Markdown."),
];
const TOOLS: &[Item] = &[
    (
        "gc neuralnets --help",
        "Review attributed local learning and pending proposals.",
    ),
    (
        "gc guidance --help",
        "Discover model capabilities and workflow recipes.",
    ),
    (
        "gc advisor SESSION",
        "Opt-in milestone reviews, separate usage and dispositions.",
    ),
    (
        "gc pr --help",
        "Inspect PR checks and reviews; preview explicit draft publication.",
    ),
    (
        "gc structure --help",
        "Lazy AST previews and stale-checked transactional edits.",
    ),
    (
        "gc companion SOCKET [PORT]",
        "Optional authenticated loopback observer with explicit input control.",
    ),
    (
        "gc usage ID",
        "Inspect reported usage, cache ratios and cost completeness.",
    ),
    (
        "gc contract SCHEMA RESULT",
        "Validate a local JSON result contract.",
    ),
    (
        "gc diagnostics ID",
        "Preview redacted local diagnostic metadata.",
    ),
    (
        "gc memory --help",
        "Review opt-in attributed project memory.",
    ),
    (
        "gc workspace --help",
        "Tasks, debug evidence, artifacts, handoffs and role budgets.",
    ),
    (
        "gc requests ID",
        "Inspect request timing, context changes and cache diagnostics.",
    ),
    (
        "gc inspect FILE",
        "Present structured local output with native formatting.",
    ),
    (
        "gc policy [--json]",
        "Explain global provider and shell denials.",
    ),
    (
        "gc completions bash|zsh|fish",
        "Generate shell completions.",
    ),
    (
        "gc skill capture|save",
        "Capture selected workflow steps into a reviewed skill.",
    ),
    (
        "gc agent --help",
        "Create and message persistent teammates.",
    ),
    (
        "gc file --help",
        "Read, edit and write files with revision checks.",
    ),
    (
        "gc worktree list|new|show|cleanup-preview|apply",
        "Inspect and reclaim isolated workspace changes.",
    ),
    (
        "gc send ID <message>",
        "Send a message to another running session.",
    ),
];
const CONNECT: &[Item] = &[
    (
        "gc settings [--json]",
        "Inspect effective settings and configuration provenance.",
    ),
    (
        "gc status [--json]",
        "Inspect the active provider, model and saved connections.",
    ),
    (
        "gc doctor [--probe]",
        "Check local setup and provider compatibility.",
    ),
    (
        "gc profiles|extensions",
        "Inspect saved profiles and extensions.",
    ),
    (
        "gc plugin list|add|update|rollback|remove",
        "Manage plugins, skills and MCP servers.",
    ),
];
const INTEGRATE: &[Item] = &[
    (
        "gc connect [status|stop]",
        "Open the shared web workspace privately through Tailscale.",
    ),
    ("gc serve [ID]", "Host a session over a Unix socket."),
    (
        "gc attach [--socket PATH]",
        "Attach a terminal to a hosted session.",
    ),
    ("gc runtime", "Host-neutral JSON-RPC runtime over stdio."),
    (
        "gc durable",
        "Runtime alias with optional checkpointed task APIs.",
    ),
    ("gc acp", "Serve the Agent Client Protocol over stdio."),
];
const KEYS: &[Item] = &[
    (
        "/queue-run",
        "Run saved follow-ups after resuming a conversation.",
    ),
    ("/turns", "Review observed workspace changes by turn."),
    (
        "/rewind",
        "Fork earlier context and optionally restore checkpointed files.",
    ),
    ("/ or Ctrl+P", "Open the command palette; Esc closes it."),
    (
        "PageUp / Ctrl+B",
        "Scroll the conversation; End or Esc returns to the bottom.",
    ),
    ("Ctrl+F", "Search the transcript."),
    ("Ctrl+G", "Open the conversation reader."),
    (
        "Alt+A / /agents",
        "Review teammates and their interactions.",
    ),
    ("Ctrl+C", "Interrupt the current turn."),
    ("Ctrl+R", "Search prompt history."),
    ("Ctrl+X", "Edit your draft in VISUAL or EDITOR."),
    ("Tab", "Search project file paths."),
    ("Alt+Enter", "Insert a newline."),
    ("Ctrl+D", "Confirm exit; Enter quits and Esc stays."),
];
const RUN: &[Item] = &[
    (
        "--restricted",
        "Launch with workspace edits, network disabled by default and explicit approvals.",
    ),
    (
        "--yolo / --unrestricted",
        "Launch without local sandbox or approval prompts; administrator rules still apply.",
    ),
    ("--trust", "Allow this workspace’s AGENTS.md for the run."),
    ("--json", "Return structured JSON for scripts."),
    (
        "--output-schema FILE",
        "Validate the final JSON answer; allow one format correction.",
    ),
    ("--plan", "Inspect and plan with read-only tools."),
    ("--resume ID", "Continue a saved conversation."),
    (
        "--retry",
        "Retry the failed response; requires --resume ID and no new prompt.",
    ),
    ("--attach FILE", "Attach a text snapshot or image."),
    ("--model MODEL", "Choose the model for this run."),
    ("--connection ID", "Use a saved provider connection."),
    ("--budget USD", "Set the session spending limit."),
];
use crate::cli::Page;
fn render(topic: &str, width: usize, color: bool) -> Result<String, String> {
    use glass_tui::theme::Tone;
    let mut p = Page {
        width: width.clamp(8, 100),
        color,
        text: String::new(),
    };
    p.row(
        &format!(
            "{} {}",
            crate::cli::product_name(),
            env!("CARGO_PKG_VERSION")
        ),
        Tone::Accent,
        true,
        0,
    );
    p.row(crate::cli::product_tagline(), Tone::Muted, false, 0);
    match topic {
        "" | "commands" | "all" => {
            let mut start = START.to_vec();
            start[0].0 = crate::cli::command_prefix();
            p.section("Start here", &start);
            if topic.is_empty() {
                p.section(
                    "Explore",
                    &[
                        ("gc help commands", "Browse all command groups."),
                        ("gc help run", "Headless options and examples."),
                        ("gc help keys", "Keyboard shortcuts for the interactive UI."),
                        (
                            "gc agent --help",
                            "Persistent teammates and transcript review.",
                        ),
                        ("gc file --help", "Native file tools and diagnostics."),
                        ("gc status", "Check the active provider connection."),
                        (
                            "gc doctor",
                            "Diagnose local setup and provider compatibility.",
                        ),
                    ],
                );
            } else {
                p.section("Conversations", SESSIONS);
                p.section("Tools and teammates", TOOLS);
                p.section("Connections and extensions", CONNECT);
                p.section("Editors and remote sessions", INTEGRATE);
                p.section(
                    "Other commands",
                    &[
                        ("gc list", "List all saved sessions."),
                        ("gc --version", "Print the installed version."),
                        (
                            "gc init [--write]",
                            "Draft project instructions; optionally save AGENTS.md.",
                        ),
                        (
                            "gc stop|respawn [ID]",
                            "Stop or restart a hosted conversation.",
                        ),
                        ("gc logs [ID]", "Inspect recent conversation activity."),
                        ("gc COMMAND --help", "Read a focused guide for any command."),
                    ],
                );
            }
            p.section(
                "Try it",
                &[
                    (
                        "gc run \"explain this project\"",
                        "Run a task from any directory, including outside Git.",
                    ),
                    (
                        "gc run --json \"review this change\"",
                        "Get a script-friendly response over SSH or in automation.",
                    ),
                ],
            );
            p.text.push('\n');
            p.row(
                &format!(
                    "Inside {}: /help opens help, /login connects a provider, /security reviews permissions.",
                    crate::cli::product_short_name()
                ),
                Tone::Muted,
                false,
                0,
            );
            if topic == "all" {
                p.section("Keyboard shortcuts", KEYS);
                p.section("Headless options", RUN);
            }
        }
        "keys" => {
            p.section("Keyboard shortcuts", KEYS);
            p.section(
                "Useful palettes",
                &[
                    ("/model", "Choose provider and model."),
                    (
                        "/context",
                        "Inspect instructions, usage and the latest provider failure.",
                    ),
                    (
                        "/security",
                        "Review permissions and extension capabilities.",
                    ),
                    ("/help", "Browse all interactive commands."),
                ],
            );
        }
        "run" => {
            p.section(
                "Usage",
                &[(
                    "gc run [options] <prompt>",
                    "Run a task without a terminal UI.",
                )],
            );
            p.section("Options", RUN);
            p.section(
                "Examples",
                &[
                    (
                        "gc run --json \"review this change\"",
                        "Produce JSON for scripts and CI.",
                    ),
                    (
                        "gc run --plan \"explain this project\"",
                        "Inspect without editing workspace files.",
                    ),
                    (
                        "gc run --json --resume ID --retry",
                        "Retry a failed response without repeating completed tools.",
                    ),
                ],
            );
        }
        _ => {
            return Err(format!(
                "unknown topic `{}`; use {} help, {} help commands, {} help run or {} help keys",
                glass_tui::markdown::sanitize(topic),
                crate::cli::command_prefix(),
                crate::cli::command_prefix(),
                crate::cli::command_prefix(),
                crate::cli::command_prefix()
            ));
        }
    }
    Ok(crate::cli::present_command_references(&p.text))
}
pub fn show(args: &[String]) -> Result<(), String> {
    let topic = args.first().map(String::as_str).unwrap_or("");
    if args.len() > 1 {
        return Err(format!(
            "use {} help [commands|run|keys|all|COMMAND]",
            crate::cli::command_prefix()
        ));
    }
    if topic == "agent" {
        return crate::team::command(&["--help".into()]);
    }
    if topic == "file" {
        return crate::files::command(&["--help".into()]);
    }
    if !matches!(topic, "" | "commands" | "run" | "keys" | "all")
        && crate::cli::command_help(&[topic.into(), "--help".into()])?
    {
        return Ok(());
    }
    let tty = io::stdout().is_terminal();
    let width = if tty {
        glass_tui::crossterm::terminal::size()
            .map(|s| s.0 as usize)
            .unwrap_or(80)
    } else {
        80
    };
    let text = render(topic, width, tty && !glass_tui::theme::no_color())?;
    io::stdout()
        .lock()
        .write_all(text.as_bytes())
        .map_err(|e| e.to_string())
}
/// Shared human and model command discovery.
pub fn catalogue() -> Vec<Item> {
    [START, SESSIONS, TOOLS, CONNECT, INTEGRATE, KEYS, RUN]
        .into_iter()
        .flatten()
        .copied()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn pages_fit_narrow_terminals_and_cover_current_features() {
        for topic in ["", "commands", "run", "keys", "all"] {
            for width in [8, 20, 40, 80, 100] {
                let text = render(topic, width, false).unwrap();
                assert!(
                    text.lines().all(|r| glass_tui::text::width(r) <= width),
                    "{topic}/{width}: {text}"
                );
                assert!(!text.contains('\x1b'));
                assert!(!text.contains("ships no permission system"));
            }
        }
        let commands = render("commands", 100, false).unwrap();
        for name in [
            "gc agent",
            "gc file",
            "gc acp",
            "gc serve",
            "gc plugin",
            "gc export",
        ] {
            assert!(commands.contains(name));
        }
        assert!(render("missing", 80, false).is_err());
    }
}
