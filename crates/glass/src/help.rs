//! Small, dependency-free CLI help. Pipes are plain; terminal rows fit their width.
use std::io::{self, IsTerminal, Write};

type Item = (&'static str, &'static str);
const START: &[Item] = &[
    ("gc", "Start a conversation in the current directory."),
    (
        "gc resume [ID]",
        "Continue the latest session, or choose one by ID.",
    ),
    (
        "gc run <prompt>",
        "Complete a task without the interactive UI.",
    ),
];
const SESSIONS: &[Item] = &[
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
        "gc agent --help",
        "Create and message persistent teammates.",
    ),
    (
        "gc file --help",
        "Read, edit and write files with revision checks.",
    ),
    (
        "gc worktree list|apply",
        "Inspect and reclaim isolated workspace changes.",
    ),
    (
        "gc send ID <message>",
        "Send a message to another running session.",
    ),
];
const CONNECT: &[Item] = &[
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
        "gc plugin list|add|remove",
        "Manage plugins, skills and MCP servers.",
    ),
];
const INTEGRATE: &[Item] = &[
    ("gc serve [ID]", "Host a session over a Unix socket."),
    (
        "gc attach [--socket PATH]",
        "Attach a terminal to a hosted session.",
    ),
    ("gc acp", "Serve the Agent Client Protocol over stdio."),
];
const KEYS: &[Item] = &[
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
    ("Ctrl+D", "Quit."),
];
const RUN: &[Item] = &[
    ("--json", "Return structured JSON for scripts."),
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
struct Page {
    width: usize,
    color: bool,
    text: String,
}
impl Page {
    fn row(&mut self, text: &str, tone: glass_tui::theme::Tone, bold: bool, indent: usize) {
        let indent = indent.min(self.width.saturating_sub(1));
        for row in glass_tui::text::wrap_words(text, self.width.saturating_sub(indent).max(1)) {
            self.text.push_str(&" ".repeat(indent));
            if self.color {
                self.text.push_str(&glass_tui::theme::ansi(tone));
                if bold {
                    self.text.push_str("\x1b[1m");
                }
            }
            self.text.push_str(&row);
            if self.color {
                self.text.push_str("\x1b[0m");
            }
            self.text.push('\n');
        }
    }
    fn section(&mut self, title: &str, items: &[Item]) {
        use glass_tui::theme::Tone;
        self.text.push('\n');
        self.row(title, Tone::Accent, true, 0);
        let column = items
            .iter()
            .map(|(a, _)| glass_tui::text::width(a))
            .max()
            .unwrap_or(0)
            + 3;
        for (command, description) in items {
            if self.width >= 72 && column < self.width / 2 {
                let rows = glass_tui::text::wrap_words(description, self.width - column - 2);
                self.text.push_str("  ");
                if self.color {
                    self.text.push_str(&glass_tui::theme::ansi(Tone::Secondary));
                }
                self.text.push_str(command);
                if self.color {
                    self.text.push_str("\x1b[0m");
                }
                self.text
                    .push_str(&" ".repeat(column - glass_tui::text::width(command)));
                self.text
                    .push_str(rows.first().map(String::as_str).unwrap_or(""));
                self.text.push('\n');
                for row in rows.iter().skip(1) {
                    self.row(row, Tone::Body, false, column + 2);
                }
            } else {
                self.row(command, Tone::Secondary, true, 2);
                self.row(description, Tone::Body, false, 4);
            }
        }
    }
}
fn render(topic: &str, width: usize, color: bool) -> Result<String, String> {
    use glass_tui::theme::Tone;
    let mut p = Page {
        width: width.clamp(8, 100),
        color,
        text: String::new(),
    };
    p.row(
        &format!("glass-core {}", env!("CARGO_PKG_VERSION")),
        Tone::Accent,
        true,
        0,
    );
    p.row(
        "AI coding harness · gc is the short command",
        Tone::Muted,
        false,
        0,
    );
    match topic {
        "" | "commands" | "all" => {
            p.section("Start here", START);
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
            p.row("Inside GC: /help opens help, /login connects a provider, /security reviews permissions.",Tone::Muted,false,0);
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
                "unknown topic `{}`; use gc help, gc help commands, gc help run or gc help keys",
                glass_tui::markdown::sanitize(topic)
            ));
        }
    }
    Ok(p.text)
}
pub fn show(args: &[String]) -> Result<(), String> {
    let topic = args.first().map(String::as_str).unwrap_or("");
    if args.len() > 1 {
        return Err("use gc help [commands|run|keys|all|agent|file]".into());
    }
    if topic == "agent" {
        return crate::team::command(&["--help".into()]);
    }
    if topic == "file" {
        return crate::files::command(&["--help".into()]);
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
