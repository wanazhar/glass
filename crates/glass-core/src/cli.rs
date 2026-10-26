//! Shared human CLI presentation. JSON, protocols and exported documents bypass it.
use glass_tui::theme::Tone;
use serde_json::Value;
use std::io::{self, IsTerminal, Write};
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, Ordering};

static HEADED: AtomicBool = AtomicBool::new(false);
static CLI_FRONTEND: OnceLock<CliFrontend> = OnceLock::new();

pub fn styled() -> bool {
    io::stdout().is_terminal()
}
fn dimensions() -> (usize, bool) {
    let width = glass_tui::crossterm::terminal::size()
        .map(|s| s.0 as usize)
        .unwrap_or(80);
    (
        width.clamp(1, 100),
        !glass_tui::theme::no_color() && std::env::var("TERM").as_deref() != Ok("dumb"),
    )
}
pub(crate) struct Page {
    pub width: usize,
    pub color: bool,
    pub text: String,
}
impl Page {
    pub fn row(&mut self, text: &str, tone: glass_tui::theme::Tone, bold: bool, indent: usize) {
        let indent = indent.min(self.width.saturating_sub(1));
        let clean = glass_tui::markdown::sanitize(text);
        for row in glass_tui::text::wrap_words(&clean, self.width.saturating_sub(indent).max(1)) {
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
    pub fn section(&mut self, title: &str, items: &[(&str, &str)]) {
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
impl Page {
    pub fn new(title: &str, width: usize, color: bool) -> Self {
        Self::new_with_frontend(title, width, color, current_frontend())
    }

    fn new_with_frontend(title: &str, width: usize, color: bool, frontend: CliFrontend) -> Self {
        let mut page = Self {
            width: width.max(1),
            color,
            text: String::new(),
        };
        let title = title
            .chars()
            .next()
            .map(|first| format!("{}{}", first.to_uppercase(), &title[first.len_utf8()..]))
            .unwrap_or_default();
        let heading = match frontend {
            CliFrontend::Glass => format!("Glass · {title}"),
            CliFrontend::GlassCore => format!("Glass Core · {title}"),
            CliFrontend::Standalone => format!("glass-core · {title}"),
        };
        page.row(&heading, Tone::Accent, true, 0);
        page.row(&"─".repeat(width.min(100)), Tone::Border, false, 0);
        page
    }
    pub fn body(&mut self, body: &str) {
        for line in glass_tui::markdown::sanitize(body).lines() {
            let line = line.strip_prefix("glass-core: ").unwrap_or(line);
            let lower = line.to_ascii_lowercase();
            let tone = if lower.contains("failed")
                || lower.starts_with("error")
                || lower.contains(": error")
                || line.starts_with('-') && !line.starts_with("---")
            {
                Tone::Error
            } else if lower.contains(": notice")
                || lower.contains("warning")
                || lower.contains("unsupported")
            {
                Tone::Warning
            } else if lower.contains(": ok")
                || lower.starts_with("wrote ")
                || lower.starts_with("sent ")
                || line.starts_with('+') && !line.starts_with("+++")
            {
                Tone::Success
            } else if line.trim_end().ends_with(':')
                || lower.starts_with("usage:")
                || line.starts_with("gc ")
                || line.starts_with("$ ")
            {
                Tone::Secondary
            } else if lower.starts_with("no ") || lower.contains("revision ") {
                Tone::Muted
            } else {
                Tone::Body
            };
            if line.is_empty() {
                self.text.push('\n');
            } else {
                self.row(line, tone, false, 2);
            }
        }
    }
    pub fn literal(&mut self, text: &str) {
        for line in glass_tui::markdown::sanitize(text).lines() {
            let tone = if line.starts_with('+') && !line.starts_with("+++") {
                Tone::Success
            } else if line.starts_with('-') && !line.starts_with("---") {
                Tone::Error
            } else if line.starts_with("$ ") {
                Tone::Secondary
            } else if line.contains("diagnostics ok") || line.starts_with("[exit 0]") {
                Tone::Success
            } else if line.contains("diagnostics error") {
                Tone::Error
            } else if line.starts_with("[file ") || line.starts_with("[partial read") {
                Tone::Muted
            } else {
                Tone::Body
            };
            let indent = 2.min(self.width.saturating_sub(1));
            for chunk in glass_tui::text::wrap(
                &line.replace('\t', "    "),
                self.width.saturating_sub(indent).max(1),
            ) {
                self.text.push_str(&" ".repeat(indent));
                if self.color {
                    self.text.push_str(&glass_tui::theme::ansi(tone));
                }
                self.text.push_str(&chunk);
                if self.color {
                    self.text.push_str("\x1b[0m");
                }
                self.text.push('\n');
            }
        }
    }
    fn field(&mut self, label: &str, value: &str, tone: Tone, indent: usize) {
        let label = glass_tui::markdown::sanitize(label);
        let column = 20usize;
        if self.width >= 72
            && glass_tui::text::width(&label) < column
            && indent + column + 4 < self.width
        {
            let rows = glass_tui::text::wrap_words(value, self.width - indent - column);
            self.text.push_str(&" ".repeat(indent));
            if self.color {
                self.text.push_str(&glass_tui::theme::ansi(Tone::Secondary));
            }
            self.text.push_str(&label);
            self.text
                .push_str(&" ".repeat(column - glass_tui::text::width(&label)));
            if self.color {
                self.text.push_str(&glass_tui::theme::ansi(tone));
            }
            self.text
                .push_str(rows.first().map(String::as_str).unwrap_or(""));
            if self.color {
                self.text.push_str("\x1b[0m");
            }
            self.text.push('\n');
            for row in rows.iter().skip(1) {
                self.row(row, tone, false, indent + column);
            }
        } else {
            self.row(&label, Tone::Secondary, true, indent);
            self.row(value, tone, false, indent + 2);
        }
    }
    fn value(&mut self, value: &Value, indent: usize) {
        match value {
            Value::Object(fields) if !fields.is_empty() => {
                for (key, value) in fields {
                    let label = key.replace('_', " ");
                    match value {
                        Value::Object(_) | Value::Array(_) => {
                            self.row(&label, Tone::Secondary, true, indent);
                            self.value(value, indent + 2);
                        }
                        _ => {
                            self.field(&label, &scalar(value), value_tone(value), indent);
                        }
                    }
                }
            }
            Value::Array(items) if !items.is_empty() => {
                for (i, item) in items.iter().enumerate() {
                    if items.len() > 1 {
                        self.row(
                            &format!("{} / {}", i + 1, items.len()),
                            Tone::Muted,
                            false,
                            indent,
                        );
                    }
                    self.value(item, indent + 2);
                    self.text.push('\n');
                }
            }
            Value::Object(_) | Value::Array(_) => self.row("None", Tone::Muted, false, indent),
            _ => self.row(&scalar(value), value_tone(value), false, indent),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum CliFrontend {
    Standalone,
    Glass,
    GlassCore,
}

fn cli_frontend(value: Option<&str>) -> CliFrontend {
    match value {
        Some("glass") => CliFrontend::Glass,
        Some("glass core") => CliFrontend::GlassCore,
        _ => CliFrontend::Standalone,
    }
}

fn current_frontend() -> CliFrontend {
    *CLI_FRONTEND
        .get_or_init(|| cli_frontend(std::env::var("GLASS_CORE_CLI_PRESENTATION").ok().as_deref()))
}

pub(crate) fn initialize_frontend() {
    let frontend = cli_frontend(std::env::var("GLASS_CORE_CLI_PRESENTATION").ok().as_deref());
    let _ = CLI_FRONTEND.set(frontend);
    // Glass sets a non-sensitive one-shot presentation marker for this Core
    // process. Remove it before Core can start user commands or provider
    // workers so child processes never inherit the private CLI handshake.
    unsafe { std::env::remove_var("GLASS_CORE_CLI_PRESENTATION") };
}

fn present_error_label(label: &str, frontend: CliFrontend) -> String {
    if let Some(suffix) = label.strip_prefix("gc ") {
        let prefix = match frontend {
            CliFrontend::Glass => "glass",
            CliFrontend::GlassCore => "glass core",
            CliFrontend::Standalone => "gc",
        };
        return format!("{prefix} {suffix}");
    }
    if label == "glass-core" || label.starts_with("glass-core ") {
        let prefix = match frontend {
            CliFrontend::Glass => "Glass",
            CliFrontend::GlassCore => "Glass Core",
            CliFrontend::Standalone => "glass-core",
        };
        return format!("{prefix}{}", &label["glass-core".len()..]);
    }
    label.to_string()
}

pub(crate) fn product_name() -> &'static str {
    product_name_for(current_frontend())
}

fn product_name_for(frontend: CliFrontend) -> &'static str {
    match frontend {
        CliFrontend::Glass => "Glass",
        CliFrontend::GlassCore => "Glass Core",
        CliFrontend::Standalone => "glass-core",
    }
}

pub(crate) fn product_tagline() -> &'static str {
    match current_frontend() {
        CliFrontend::Glass => "Glass workspace · Core-powered agent and tools",
        CliFrontend::GlassCore => "Glass Core · standalone agent and permission authority",
        CliFrontend::Standalone => "AI coding harness · gc is the short command",
    }
}

pub(crate) fn product_short_name() -> &'static str {
    product_short_name_for(current_frontend())
}

fn product_short_name_for(frontend: CliFrontend) -> &'static str {
    match frontend {
        CliFrontend::Glass => "Glass",
        CliFrontend::GlassCore => "Glass Core",
        CliFrontend::Standalone => "GC",
    }
}

pub(crate) fn command_prefix() -> &'static str {
    match current_frontend() {
        CliFrontend::Glass => "glass",
        CliFrontend::GlassCore => "glass core",
        CliFrontend::Standalone => "gc",
    }
}

pub(crate) fn connect_product_name() -> &'static str {
    connect_product_name_for(current_frontend())
}

fn connect_product_name_for(frontend: CliFrontend) -> &'static str {
    match frontend {
        CliFrontend::Glass => "Glass Connect",
        CliFrontend::GlassCore => "Glass Core Connect",
        CliFrontend::Standalone => "GC Connect",
    }
}

pub(crate) fn connect_command() -> &'static str {
    connect_command_for(current_frontend())
}

fn connect_command_for(frontend: CliFrontend) -> &'static str {
    match frontend {
        CliFrontend::Glass => "glass connect",
        CliFrontend::GlassCore => "glass core connect",
        CliFrontend::Standalone => "gc connect",
    }
}

pub(crate) fn present_command_references(text: &str) -> String {
    present_command_references_with_prefix(text, command_prefix())
}

fn present_command_references_with_prefix(text: &str, prefix: &str) -> String {
    if prefix == "gc" {
        return text.to_string();
    }
    const ROOTS: &[&str] = &[
        "acp",
        "agent",
        "attach",
        "attention",
        "blobs",
        "capture",
        "checkpoint",
        "completions",
        "companion",
        "config",
        "connect",
        "context",
        "contract",
        "details",
        "diagnostics",
        "doctor",
        "export",
        "extensions",
        "file",
        "guidance",
        "handoff",
        "help",
        "history",
        "hub",
        "init",
        "inspect",
        "keys",
        "list",
        "memory",
        "models",
        "neuralnets",
        "plugin",
        "policy",
        "prewalk",
        "profiles",
        "pr",
        "pwd",
        "requests",
        "resume",
        "respawn",
        "runtime",
        "review",
        "run",
        "search",
        "send",
        "serve",
        "sessions",
        "settings",
        "skill",
        "structure",
        "status",
        "stop",
        "team",
        "transaction",
        "tools",
        "usage",
        "worktree",
        "workspace",
    ];
    let bytes = text.as_bytes();
    let mut out = String::with_capacity(text.len() + prefix.len());
    let mut cursor = 0;
    while cursor < bytes.len() {
        let is_gc = bytes[cursor..].starts_with(b"gc")
            && (cursor == 0
                || !bytes[cursor - 1].is_ascii_alphanumeric() && bytes[cursor - 1] != b'_')
            && (cursor + 2 == bytes.len()
                || !bytes[cursor + 2].is_ascii_alphanumeric() && bytes[cursor + 2] != b'_');
        if !is_gc {
            let ch = text[cursor..].chars().next().expect("cursor is in bounds");
            out.push(ch);
            cursor += ch.len_utf8();
            continue;
        }
        let mut end = cursor + 2;
        while end < bytes.len() && bytes[end].is_ascii_whitespace() {
            end += 1;
        }
        let command_end = (end..bytes.len())
            .find(|index| !bytes[*index].is_ascii_alphanumeric() && bytes[*index] != b'-')
            .unwrap_or(bytes.len());
        let command = text.get(end..command_end).unwrap_or_default();
        if ROOTS.contains(&command) {
            out.push_str(prefix);
            cursor += 2;
            continue;
        }
        out.push_str("gc");
        cursor += 2;
    }
    out
}
fn scalar(v: &Value) -> String {
    glass_tui::markdown::sanitize(&match v {
        Value::String(s) => s.clone(),
        Value::Null => "Not set".into(),
        _ => v.to_string(),
    })
}
fn value_tone(v: &Value) -> Tone {
    match v.as_str() {
        Some("ok" | "done" | "idle" | "succeeded") => Tone::Success,
        Some("error" | "failed" | "blocked") => Tone::Error,
        Some("notice" | "warning" | "working") => Tone::Warning,
        _ => Tone::Body,
    }
}
fn heading(title: &str) -> Page {
    let (width, color) = dimensions();
    if !HEADED.swap(true, Ordering::Relaxed) {
        Page::new(title, width, color)
    } else {
        Page {
            width,
            color,
            text: String::new(),
        }
    }
}
/// Preserve plain pipe output verbatim, style and wrap terminal output.
pub fn text(title: &str, body: &str) {
    let output = if styled() {
        let mut p = heading(title);
        p.body(body);
        p.text
    } else {
        body.to_string()
    };
    let mut out = io::stdout().lock();
    let _ = out.write_all(output.as_bytes());
    let _ = out.flush();
}
/// File and tool content preserve indentation, line numbers and whitespace.
pub fn literal(title: &str, body: &str) {
    if !styled() {
        text(title, body);
        return;
    }
    let mut p = heading(title);
    p.literal(body);
    let _ = io::stdout()
        .lock()
        .write_all(crate::osc8::link_urls(&p.text).as_bytes());
}
pub fn line(body: &str) {
    let title = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "glass-core".into());
    text(&title, &format!("{body}\n"));
}
pub fn report(title: &str, value: &Value) {
    if !styled() {
        if let Some(text) = value["text"].as_str() {
            println!("{text}");
        } else {
            println!(
                "{}",
                serde_json::to_string_pretty(value).unwrap_or_default()
            );
        }
        return;
    }
    let mut p = heading(title);
    if let Some(checks) = value["report"]["checks"].as_array() {
        let notes = checks
            .iter()
            .filter(|check| check["status"] != "ok")
            .count();
        p.row(
            &format!("{} checks · {notes} need attention", checks.len()),
            Tone::Muted,
            false,
            2,
        );
        for check in checks {
            p.text.push('\n');
            let status = check["status"].as_str().unwrap_or("unknown");
            let symbol = if status == "ok" {
                "✓"
            } else if status == "error" {
                "×"
            } else {
                "!"
            };
            p.row(
                &format!(
                    "{symbol} {} · {status}",
                    check["area"].as_str().unwrap_or("Check")
                ),
                value_tone(&check["status"]),
                true,
                2,
            );
            p.row(check["detail"].as_str().unwrap_or(""), Tone::Body, false, 4);
        }
    } else if title == "status" {
        for key in ["provider", "model", "status", "endpoint"] {
            p.field(key, &scalar(&value[key]), value_tone(&value[key]), 2);
        }
        p.text.push('\n');
        p.row("Saved connections", Tone::Secondary, true, 2);
        p.value(&value["connections"], 4);
        p.text.push('\n');
        p.row(
            "/model chooses a model · /login connects a provider",
            Tone::Muted,
            false,
            2,
        );
    } else if let Some(text) = value["text"].as_str() {
        if title == "details" {
            p.literal(text);
        } else {
            p.body(text);
        }
    } else {
        p.value(value, 2);
    }
    let _ = io::stdout()
        .lock()
        .write_all(crate::osc8::link_urls(&p.text).as_bytes());
}
/// Agent CLI historically emits compact JSON to pipes without a --json flag.
pub fn agent(value: &Value) {
    if styled() {
        report("Teammates", value);
    } else {
        println!("{value}");
    }
}
pub fn error(label: &str, error: &str) {
    let frontend = current_frontend();
    let label = present_error_label(label, frontend);
    let error = present_command_references(error);
    if !io::stderr().is_terminal() {
        eprintln!("{label}: {error}");
        return;
    }
    let (width, color) = dimensions();
    let mut p = Page {
        width,
        color,
        text: String::new(),
    };
    p.row(&format!("{label} · Error"), Tone::Error, true, 0);
    for line in glass_tui::markdown::sanitize(&error).lines() {
        p.row(line, Tone::Error, false, 2);
    }
    let _ = io::stderr().lock().write_all(p.text.as_bytes());
}

#[macro_export]
macro_rules! cli_println {
    ($($argument:tt)*) => { $crate::cli::line(&format!($($argument)*)) };
}

/// Static help never initializes accounts, providers, extensions or sessions.
pub fn command_help(args: &[String]) -> Result<bool, String> {
    let Some(command) = args.first().map(String::as_str) else {
        return Ok(false);
    };
    let mut wants_help = false;
    let mut index = 1;
    while index < args.len() {
        let arg = args[index].as_str();
        if arg == "--" {
            break;
        }
        if matches!(arg, "--help" | "-h") {
            wants_help = true;
            break;
        }
        if command == "run" {
            if matches!(
                arg,
                "--resume"
                    | "--model"
                    | "--connection"
                    | "--attach"
                    | "--budget"
                    | "--output-schema"
            ) {
                index += 2;
                continue;
            }
            if !arg.starts_with('-') {
                break;
            }
        }
        index += 1;
    }
    if !wants_help {
        return Ok(false);
    }
    let (usage, description, examples) = match command {
        "settings" => (
            "gc settings [--json]",
            "Inspect effective settings and the last layer that changed each value. Prompt contents, executable arguments and credentials are omitted. Reset the workspace binding through /profiles.",
            "gc settings --json",
        ),
        "usage" => (
            "gc usage SESSION [--json]",
            "Inspect bounded recent reported usage, cache hit ratio and cost completeness. Missing usage and provider quotas remain explicitly unknown.",
            "gc usage SESSION_ID",
        ),
        "attention" => (
            "gc attention SESSION [--json]",
            "Inspect recent recorded failures and teammate events with exact raw-record positions. The interactive /attention view searches event previews.",
            "gc attention SESSION_ID",
        ),
        "history" => (
            "gc history SESSION [--json]",
            "Inspect retained history boundaries, compaction summary previews and branch lineage without opening a session for writing.",
            "gc history SESSION_ID",
        ),
        "handoff" => (
            "gc handoff SESSION [--json]",
            "Preview a portable bounded handoff with durable notes and references to omitted history. Review project data before sharing; credentials and running processes are not transferred.",
            "gc handoff SESSION_ID --json > handoff.json",
        ),
        "contract" => (
            "gc contract SCHEMA.json RESULT.json",
            "Validate a bounded local JSON result contract. Unsupported schema keywords fail explicitly; no provider request is made.",
            "gc contract result-schema.json result.json",
        ),
        "diagnostics" => (
            "gc diagnostics SESSION [--json] [--save FILE]",
            "Preview allowlisted local diagnostic metadata. Prompts, commands, outputs and credentials are omitted. Saving creates a new private file; nothing is uploaded.",
            "gc diagnostics SESSION_ID\ngc diagnostics SESSION_ID --save diagnostic.json",
        ),
        "memory" => (
            "gc memory [list|enable|disable|set KEY TEXT SOURCE|forget KEY] [--json]",
            "Review opt-in project memory with source attribution. Memory is disabled by default. Explicitly saved facts are included within a bounded context budget when enabled.",
            "gc memory list\ngc memory set build 'cargo build' README.md\ngc memory enable",
        ),
        "skill" => (
            "gc skill capture SESSION NAME FROM THROUGH\ngc skill save NAME REVIEWED_DRAFT.md",
            "Capture selected successful steps as a draft. Review it before explicitly saving; capturing never executes commands or installs anything.",
            "gc skill capture SESSION_ID local-check 1 2 > draft.md\ngc skill save local-check draft.md",
        ),
        "inspect" => (
            "gc inspect FILE [--json]",
            "Render bounded JSON, diagnostic and text outputs using native terminal presentation. Raw JSON is available for scripts.",
            "gc inspect report.json",
        ),
        "policy" => (
            "gc policy [shell COMMAND | explain COMMAND | extension SPEC | provider KIND ENDPOINT] [--json]",
            "Inspect global hard denials and explain a decision. Shell patterns match text; they are not a replacement for kernel confinement.",
            "gc policy\ngc policy shell 'git push'",
        ),
        "workspace" => {
            crate::workspace::command(&["--help".into()])?;
            return Ok(true);
        }
        "completions" => (
            "gc completions bash|zsh|fish",
            "Generate offline shell completions. Commands are read from shared metadata; model and session candidates are loaded only when requested.",
            "gc completions bash > ~/.local/share/bash-completion/completions/gc",
        ),
        "requests" => (
            "gc requests SESSION_ID [--json]",
            "Inspect recent request timings, context changes and cache usage without copying prompts or credentials.",
            "gc requests SESSION_ID --json",
        ),
        "resume" => (
            "gc resume [ID | --all | --browse]",
            "Choose a saved conversation in the current project. Type to search, use arrows to select, Enter to resume, or Esc to cancel. Use --all for cross-project search or --browse for preview and fork actions. Supplying an ID resumes it directly. Requires an interactive terminal; scripts should use gc run --resume ID.",
            "gc resume\ngc resume SESSION_ID",
        ),
        "status" => (
            "gc status [--json]",
            "Inspect the active provider, model, endpoint and saved connections. Credentials are not printed.",
            "gc status\ngc status --json",
        ),
        "doctor" => (
            "gc doctor [--probe] [--json]",
            "Check local configuration, authentication references, permissions and terminal support. Offline by default. --probe explicitly checks the configured provider.",
            "gc doctor\ngc doctor --probe --json",
        ),
        "sessions" => (
            "gc sessions [--json]",
            "List conversations belonging to the current workspace. Use gc resume to choose one interactively.",
            "gc sessions\ngc resume",
        ),
        "list" => (
            "gc list",
            "List saved conversations across all workspaces, newest first, with titles and modification times.",
            "gc list",
        ),
        "search" => (
            "gc search <pattern>",
            "Search saved user and model messages, case-insensitively. Shows up to 100 matching messages.",
            "gc search \"authentication\"",
        ),
        "context" => (
            "gc context ID [--json]",
            "Inspect loaded instructions, attachments, compaction and the latest recorded request or provider failure.",
            "gc context SESSION_ID --json",
        ),
        "review" => (
            "gc review ID [--json]",
            "Inspect recorded tools and workspace changes for a conversation.",
            "gc review SESSION_ID",
        ),
        "details" => (
            "gc details ID [--json]",
            "Read complete recorded tool output for a conversation.",
            "gc details SESSION_ID",
        ),
        "profiles" => (
            "gc profiles [--json]",
            "Inspect saved workspace profiles. Manage profiles with /profiles inside the interactive UI.",
            "gc profiles",
        ),
        "extensions" => (
            "gc extensions [--json]",
            "Inspect configured extensions and available bundles. Manage them with /extensions in the interactive UI.",
            "gc extensions",
        ),
        "init" => (
            "gc init [--write] [--json]",
            "Ask the configured model to draft AGENTS.md for this workspace. Prints a draft by default; --write saves it. This command uses a provider request.",
            "gc init\ngc init --write",
        ),
        "export" => (
            "gc export [ID|--latest] [--thinking] [--out PATH] [--json]",
            "Export a saved conversation as Markdown. --thinking includes recorded reasoning. Without --out, stdout is the unmodified Markdown document.",
            "gc export --latest --out conversation.md",
        ),
        "plugin" => (
            "gc plugin list [--json]\ngc plugin add <path|git-url>\ngc plugin remove NAME\ngc plugin verify\ngc plugin update NAME [--accept-permissions]\ngc plugin scaffold NAME DIRECTORY\ngc plugin validate DIRECTORY [--json]\ngc plugin trace [--json]",
            "Manage installed plugins and verify their receipts. Review source and permissions before activating extensions.",
            "gc plugin list\ngc plugin verify",
        ),
        "worktree" => (
            "gc worktree list [--json]\ngc worktree new NAME [BASE]\ngc worktree show NAME\ngc worktree cleanup-preview NAME\ngc worktree apply [NAME] [--remove]",
            "Inspect isolated workspaces and apply their changes to the current Git workspace. --remove removes the worktree after applying it.",
            "gc worktree list",
        ),
        "send" => (
            "gc send ID <message>",
            "Send a follow-up to another running session through its local inbox.",
            "gc send SESSION_ID \"Please focus on the parser\"",
        ),
        "connect" => (
            "gc connect [--local | --http]\ngc connect status\ngc connect stop",
            "Open the installable web workspace through private Tailscale HTTPS. Existing routes are preserved. Closing the browser does not stop tasks. --http adds a tailnet HTTP link using the host IP. --local uses a private loopback access link. stop removes only GC remote access; served tasks continue.",
            "gc connect\ngc connect status",
        ),
        "serve" => (
            "gc serve [ID] [--socket PATH]",
            "Host a session on a private local Unix socket. Attach from another terminal to interact.",
            "gc serve\ngc attach --socket /path/to/socket",
        ),
        "attach" => (
            "gc attach [--socket PATH]",
            "Attach your terminal to a hosted conversation. The host owns the session and provider state.",
            "gc attach",
        ),
        "stop" => (
            "gc stop [ID]",
            "Request that a hosted session stop cleanly. Omitting the ID selects the newest hosted session.",
            "gc stop SESSION_ID",
        ),
        "respawn" => (
            "gc respawn [ID]",
            "Restart a hosted session. Omitting the ID selects the newest session.",
            "gc respawn SESSION_ID",
        ),
        "logs" => (
            "gc logs [ID]",
            "Inspect recorded activity from a hosted session. Omitting the ID selects the newest session.",
            "gc logs SESSION_ID",
        ),
        "runtime" => (
            "gc runtime",
            "Serve the host-neutral runtime over stdio. Initialize protocolVersion 1, create a session, then submit turns. Host mode supplies all tools and permissions. See docs/reference/runtime.md.",
            "gc runtime
python3 examples/runtime_host.py --storage /tmp/gc-host-demo 'Read README.md'",
        ),
        "acp" => (
            "gc acp",
            "Serve the Agent Client Protocol over stdio. Stdout is reserved for protocol messages.",
            "gc acp",
        ),
        "run" => {
            crate::help::show(&["run".into()])?;
            return Ok(true);
        }
        "agent" => {
            crate::team::command(&["--help".into()])?;
            return Ok(true);
        }
        "file" => {
            crate::files::command(&["--help".into()])?;
            return Ok(true);
        }
        _ => return Ok(false),
    };
    let (width, color) = if styled() { dimensions() } else { (80, false) };
    let mut p = Page::new(command, width, color);
    p.text.push('\n');
    p.row("Usage", Tone::Secondary, true, 0);
    for row in present_command_references(usage).lines() {
        p.row(row, Tone::Secondary, false, 2);
    }
    p.text.push('\n');
    p.row(
        &present_command_references(description),
        Tone::Body,
        false,
        2,
    );
    p.text.push('\n');
    p.row("Examples", Tone::Secondary, true, 0);
    for row in present_command_references(examples).lines() {
        p.row(row, Tone::Body, false, 2);
    }
    p.text.push('\n');
    p.row(
        &present_command_references("gc help commands · Browse every command"),
        Tone::Muted,
        false,
        0,
    );
    io::stdout()
        .lock()
        .write_all(p.text.as_bytes())
        .map_err(|e| e.to_string())?;
    Ok(true)
}

/// Reuse native Markdown for completed model output; raw pipes stay copyable.
pub fn markdown(title: &str, body: &str) {
    if !styled() {
        text(title, body);
        return;
    }
    let p = heading(title);
    let mut bytes = p.text.into_bytes();
    for line in glass_tui::markdown::lines(body, p.width) {
        glass_tui::markdown::write_ansi(&mut bytes, &line);
        bytes.push(b'\n');
    }
    if !p.color {
        bytes = glass_tui::markdown::sanitize(&String::from_utf8_lossy(&bytes)).into_bytes();
    }
    let mut out = glass_tui::theme::TerminalWriter::new(io::stdout().lock());
    let _ = out.write_all(&bytes);
}

/// A successful init write deserves an explicit destination confirmation.
pub fn written(value: &Value) {
    if value["written"] == true {
        line(&format!(
            "wrote {}",
            value["path"].as_str().unwrap_or("AGENTS.md")
        ));
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn literal_content_preserves_indentation() {
        let mut p = Page {
            width: 80,
            color: false,
            text: String::new(),
        };
        p.literal("1 |     nested    value\n+    added    value\n2 |\tindented");
        assert!(p.text.contains("1 |     nested    value"));
        assert!(p.text.contains("+    added    value"));
        assert!(p.text.contains("2 |    indented"));
    }
    #[test]
    fn frontend_context_changes_human_command_examples_only() {
        let authored_help =
            "Usage: gc status [--json]\nRun gc doctor first; gc is the legacy short command.";
        let glass = present_command_references_with_prefix(authored_help, "glass");
        assert!(glass.contains("Usage: glass status [--json]"));
        assert!(glass.contains("Run glass doctor first; gc is the legacy short command."));
        let glass_core = present_command_references_with_prefix(authored_help, "glass core");
        assert!(glass_core.contains("Usage: glass core status [--json]"));
        assert!(
            glass_core.contains("Run glass core doctor first; gc is the legacy short command.")
        );
        assert_eq!(
            present_command_references_with_prefix(authored_help, "gc"),
            authored_help
        );
        assert_eq!(cli_frontend(Some("glass")), CliFrontend::Glass);
        assert_eq!(cli_frontend(Some("glass core")), CliFrontend::GlassCore);
        assert_eq!(cli_frontend(None), CliFrontend::Standalone);
        assert!(
            Page::new_with_frontend("Status", 80, false, CliFrontend::Glass)
                .text
                .contains("Glass · Status")
        );
        assert!(
            Page::new_with_frontend("Doctor", 80, false, CliFrontend::GlassCore)
                .text
                .contains("Glass Core · Doctor")
        );
        assert!(
            Page::new_with_frontend("Config", 80, false, CliFrontend::Standalone)
                .text
                .contains("glass-core · Config")
        );
        assert_eq!(
            present_error_label("glass-core", CliFrontend::Glass),
            "Glass"
        );
        assert_eq!(
            present_error_label("gc doctor", CliFrontend::GlassCore),
            "glass core doctor"
        );
        assert_eq!(
            present_command_references_with_prefix(
                "Role is not configured; use gc workspace role set reviewer",
                "glass"
            ),
            "Role is not configured; use glass workspace role set reviewer"
        );
        assert_eq!(product_name_for(CliFrontend::Glass), "Glass");
        assert_eq!(product_name_for(CliFrontend::GlassCore), "Glass Core");
        assert_eq!(product_name_for(CliFrontend::Standalone), "glass-core");
        assert_eq!(product_short_name_for(CliFrontend::Glass), "Glass");
        assert_eq!(product_short_name_for(CliFrontend::GlassCore), "Glass Core");
        assert_eq!(product_short_name_for(CliFrontend::Standalone), "GC");
        assert_eq!(
            connect_product_name_for(CliFrontend::Glass),
            "Glass Connect"
        );
        assert_eq!(connect_command_for(CliFrontend::Glass), "glass connect");
        assert_eq!(
            connect_product_name_for(CliFrontend::GlassCore),
            "Glass Core Connect"
        );
        assert_eq!(
            connect_command_for(CliFrontend::GlassCore),
            "glass core connect"
        );
        assert_eq!(
            connect_product_name_for(CliFrontend::Standalone),
            "GC Connect"
        );
        assert_eq!(connect_command_for(CliFrontend::Standalone), "gc connect");
        let roots = [
            "connect", "serve", "runtime", "run", "resume", "models", "status", "doctor",
            "settings", "config",
        ];
        for (frontend, prefix) in [
            (CliFrontend::Standalone, "gc"),
            (CliFrontend::Glass, "glass"),
            (CliFrontend::GlassCore, "glass core"),
        ] {
            for root in roots {
                let source = format!("Usage: gc {root} [--help]");
                let displayed = present_command_references_with_prefix(&source, prefix);
                assert_eq!(displayed, format!("Usage: {prefix} {root} [--help]"));
                let heading = Page::new_with_frontend(root, 80, false, frontend);
                let brand = match frontend {
                    CliFrontend::Standalone => "glass-core",
                    CliFrontend::Glass => "Glass",
                    CliFrontend::GlassCore => "Glass Core",
                };
                assert!(heading.text.starts_with(&format!("{brand} · ")));
            }
        }
    }
    #[test]
    fn shared_reports_fit_and_sanitize() {
        for width in [1, 8, 20, 40, 80, 100] {
            let mut p = Page::new("Status", width, false);
            p.value(&serde_json::json!({"model":"a very long model name 界界界", "connections":[{"label":"evil\u{1b}]52;c;bad\u{7}","status":"ok"}],"empty":[]}),2);
            p.body("terminal: ok — detected\nconfiguration: notice — defaults\nerror: test\n+added\n-removed");
            assert!(!p.text.contains('\x1b'));
            assert!(
                p.text.lines().all(|s| glass_tui::text::width(s) <= width),
                "{width}: {}",
                p.text
            );
        }
    }
}
