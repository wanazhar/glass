//! Shared human action registry. Discovery is local; handlers retain effect gates.
use glass_tui::palette::Command;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Group {
    Conversations,
    Work,
    Settings,
}
pub fn group(name: &str) -> Group {
    match name {
        "conversations" | "sessions" | "branches" | "name" | "clear" | "branch" | "history"
        | "handoff" | "recap" | "export" | "search" | "quote" | "prompts" | "editor" | "btw"
        | "context" | "attach" | "forget" | "unforget" | "rewrite" | "compact" | "compaction"
        | "rewind" => Group::Conversations,
        "work" | "workspace" | "dashboard" | "hub" | "outputs" | "responses" | "details"
        | "thinking" | "review" | "turns" | "diff" | "git" | "pr" | "code-review" | "advisor"
        | "checkpoint" | "checkpoints" | "undo" | "restore" | "activity" | "attention"
        | "agents" | "jobs" | "job" | "bg" | "loop" | "send" | "queue" | "queue-run" | "plan"
        | "constraints" | "todo" | "retry" | "recovery" | "diagnostics" | "tasks" => Group::Work,
        _ => Group::Settings,
    }
}
pub fn canonical(name: &str) -> &str {
    match name {
        "sessions" => "conversations",
        "dashboard" | "hub" | "workspace" | "activity" => "work",
        "responses" | "details" => "outputs",
        "compaction" => "compact",
        "permissions" => "security",
        "memory" => "neuralnets",
        _ => name,
    }
}
pub fn available_during_turn(name: &str) -> bool {
    matches!(
        canonical(name),
        "queue" | "agents" | "outputs" | "thinking" | "help" | "work" | "todo"
    )
}
pub fn effect(name: &str) -> &'static str {
    match name {
        "code-review" | "advisor" | "btw" | "plan" | "compact" => "model request",
        "init" => "model + files",
        "retry" | "queue-run" => "continues model work",
        "git" | "pr" | "restore" | "undo" | "checkpoint" | "rewind" => "inspect; explicit changes",
        "login" | "logout" | "model" => "account / model",
        "bg" | "loop" | "job" | "send" | "agents" => "explicit task control",
        _ => "local",
    }
}
fn keywords(name: &str) -> &'static str {
    match name {
        "conversations" => "resume continue sessions threads branches switch conversation",
        "work" => "workspace dashboard hub activity running processes tasks evidence",
        "settings" => "configure preferences options setup",
        "outputs" => "copy code read response full output tools details json markdown",
        "model" => "change model choose provider account switch model",
        "animation" => "spinner dot charm pulse motion loading waiting working",
        "recovery" => "why failed error failure debug repair troubleshoot",
        "undo" | "restore" | "rewind" => "undo file restore previous changes rollback",
        "search" => "search conversation find text chat",
        "doctor" => "setup connection check diagnose internet",
        "usage" => "tokens cost cache hit ratio spending",
        "neuralnets" | "memory" => "knowledge learning facts preferences memory",
        _ => "",
    }
}
const BUILTIN: &[(&str, &str, &str, bool)] = &[
    (
        "memory",
        "",
        "Reviewed project facts and context inclusion",
        false,
    ),
    ("help", "", "Show help", false),
    (
        "todo",
        "",
        "Inspect and manage the session checklist",
        false,
    ),
    (
        "neuralnets",
        "",
        "Review attributed project learning",
        false,
    ),
    (
        "guidance",
        "",
        "Discover model workflows and capabilities",
        false,
    ),
    (
        "history",
        "",
        "Inspect compaction receipts and branch lineage",
        false,
    ),
    (
        "branches",
        "",
        "Browse retained conversation alternatives",
        false,
    ),
    ("queue-run", "", "Run saved follow-ups now", false),
    (
        "rewind",
        "",
        "Fork earlier context; optionally restore guarded files",
        false,
    ),
    ("turns", "", "Review observed changes by turn", false),
    (
        "constraints",
        "[list|on|off|clear|add TEXT]",
        "Opt-in turn-boundary task reminders",
        false,
    ),
    (
        "settings",
        "",
        "Effective settings and layer provenance",
        false,
    ),
    (
        "usage",
        "",
        "Reported usage, cost completeness and cache changes",
        false,
    ),
    (
        "attention",
        "",
        "Review failures and teammate event targets",
        false,
    ),
    (
        "quote",
        "",
        "Attach attributed conversation excerpts",
        false,
    ),
    (
        "advisor",
        "",
        "Review cadence, notes and separate usage",
        false,
    ),
    ("pr", "", "Pull request, CI and review cockpit", false),
    (
        "hub",
        "",
        "Conversations, teammates, attention and tasks",
        false,
    ),
    ("handoff", "", "Preview a bounded portable handoff", false),
    (
        "recap",
        "",
        "Reopen your welcome-back recap · no extra tokens",
        false,
    ),
    (
        "workspace",
        "",
        "Tasks, debug evidence, artifacts and role presets",
        false,
    ),
    (
        "capture",
        "",
        "Capture selected turns as a reviewed reusable skill",
        false,
    ),
    (
        "outputs",
        "",
        "Native structured response and tool output viewer",
        false,
    ),
    (
        "requests",
        "",
        "Request timing, context changes and cache diagnostics",
        false,
    ),
    (
        "keys",
        "",
        "Discover draft, active-turn and viewer shortcuts",
        false,
    ),
    ("btw", "<question>", "Ask a side question", true),
    ("model", "[name]", "Choose provider and model", false),
    ("login", "", "Connect a provider or ChatGPT account", false),
    ("logout", "", "Remove saved credentials", false),
    ("status", "", "View connection and session status", false),
    ("sessions", "", "Browse project conversations", false),
    ("search", "", "Search this conversation", false),
    ("name", "[title]", "Name this conversation", false),
    ("skill-doctor", "", "Skill sizes and context cost", false),
    (
        "code-review",
        "[max]",
        "Review the diff for bugs, read-only",
        false,
    ),
    ("retry", "", "Recover a failed response", false),
    ("details", "", "Expand tool activity and output", false),
    (
        "agents",
        "",
        "Persistent teammates, interactions and transcripts",
        false,
    ),
    (
        "activity",
        "",
        "Inspect tools, subagents, jobs and loops",
        false,
    ),
    (
        "thinking",
        "",
        "View live provider reasoning · Ctrl-T",
        false,
    ),
    (
        "context",
        "",
        "Inspect context and manage attached files",
        false,
    ),
    (
        "attach",
        "[path]",
        "Attach a text file to model context",
        false,
    ),
    (
        "review",
        "",
        "Review task summary, commands and changes",
        false,
    ),
    ("prompts", "", "Search prompt history", false),
    ("editor", "", "Edit your draft in VISUAL or EDITOR", false),
    ("redraw", "", "Repaint after terminal drift", false),
    ("clear", "", "Start a fresh session", false),
    (
        "queue",
        "",
        "Manage saved follow-ups: edit, reorder, promote or delete",
        false,
    ),
    (
        "diagnostics",
        "",
        "Preview a private diagnostic bundle",
        false,
    ),
    (
        "recovery",
        "",
        "Inspect recent failure classes, retries and cache changes",
        false,
    ),
    ("compact", "[focus]", "Compact the conversation now", false),
    (
        "compaction",
        "[focus]",
        "Compact the conversation now (alias)",
        false,
    ),
    ("diff", "[checkpoint]", "Review task changes", false),
    ("budget", "[USD|off]", "Show or set spending limit", false),
    (
        "loop",
        "[interval] [prompt]",
        "Schedule a repeating prompt",
        false,
    ),
    (
        "send",
        "[session-id] [message]",
        "Message another running session",
        false,
    ),
    (
        "forget",
        "[turn]",
        "Hide a turn from future model context",
        false,
    ),
    ("unforget", "[turn]", "Restore a forgotten turn", false),
    (
        "rewrite",
        "[turn] [text]",
        "Rewrite a turn for future context",
        false,
    ),
    ("branch", "[turn]", "Fork conversation history", false),
    ("checkpoint", "", "Save workspace baseline", false),
    ("checkpoints", "", "List saved checkpoints", false),
    ("undo", "[checkpoint]", "Undo task changes", false),
    ("restore", "<checkpoint>", "Restore a saved baseline", true),
    ("bg", "<command>", "Start a background command", true),
    ("jobs", "", "List background jobs", false),
    ("job", "<id> [stop]", "Read output or stop a job", true),
    (
        "plan",
        "[task]",
        "Inspect read-only, review a plan, then choose execution",
        false,
    ),
    (
        "init",
        "[notes]",
        "Draft AGENTS.md for this repository",
        false,
    ),
    (
        "git",
        "",
        "Review files and hunks, stage, and draft commits",
        false,
    ),
    (
        "doctor",
        "",
        "Diagnose provider, streaming, and tool compatibility",
        false,
    ),
    ("profiles", "", "Save and select workspace settings", false),
    (
        "responses",
        "",
        "Read Markdown and copy responses or code blocks",
        false,
    ),
    (
        "export",
        "[path]",
        "Write the conversation as a Markdown file",
        false,
    ),
    ("theme", "", "Choose a terminal color palette", false),
    (
        "pacing",
        "",
        "Choose adaptive provider pacing or maximum frequency",
        false,
    ),
    (
        "animation",
        "",
        "Preview and choose the activity animation",
        false,
    ),
    (
        "dashboard",
        "",
        "Workspace dashboard and quick actions",
        false,
    ),
    (
        "security",
        "",
        "Review permission profiles, exact-command exceptions, and extension capabilities",
        false,
    ),
    (
        "permissions",
        "",
        "Review permission profiles, exact-command exceptions, and extension capabilities",
        false,
    ),
    (
        "mods",
        "",
        "Enable or disable built-in mods for this session",
        false,
    ),
    (
        "extensions",
        "",
        "Inspect and stop loaded extensions",
        false,
    ),
    ("skills", "", "List loaded skills", false),
    (
        "conversations",
        "",
        "Resume, preview and manage conversations",
        false,
    ),
    (
        "work",
        "",
        "Changes, runs, teammates, tasks and attention",
        false,
    ),
    (
        "tasks",
        "",
        "Task board, evidence, artifacts and role presets",
        false,
    ),
    (
        "settings-info",
        "",
        "Effective configuration and provenance",
        false,
    ),
    ("tools", "", "Choose expanded or compact tool cards", false),
    (
        "prewalk",
        "[model]",
        "Prepare context for a model transition",
        false,
    ),
];
pub fn catalogue() -> Vec<Command> {
    let mut commands = Vec::new();
    // Exactly three roots, in a predictable order; all other actions are searched.
    for root in ["conversations", "work", "settings"] {
        add(&mut commands, root);
    }
    for (name, ..) in BUILTIN {
        let name = canonical(name);
        if !commands.iter().any(|c| c.name == name) {
            add(&mut commands, name);
        }
    }
    commands
}
fn add(commands: &mut Vec<Command>, name: &str) {
    let Some((_, hint, description, required_args)) = BUILTIN.iter().find(|(n, ..)| *n == name)
    else {
        return;
    };
    let aliases: Vec<String> = BUILTIN
        .iter()
        .filter(|(n, ..)| *n != name && canonical(n) == name)
        .map(|(n, ..)| (*n).into())
        .collect();
    let primary = matches!(name, "conversations" | "work" | "settings");
    commands.push(Command {
        name: name.into(),
        hint: (*hint).into(),
        description: format!("{description} · {}", effect(name)),
        required_args: *required_args,
        label: match name {
            "conversations" => "Conversations",
            "work" => "Work",
            "settings" => "Settings",
            _ => name,
        }
        .into(),
        aliases,
        keywords: keywords(name).into(),
        primary,
    });
}
pub fn commands(extensions: &crate::ext::Registry) -> Vec<Command> {
    let mut commands = catalogue();
    for (name, description) in extensions.commands() {
        if !commands
            .iter()
            .any(|c| c.name == name || c.aliases.contains(&name))
        {
            commands.push(Command {
                name,
                description,
                ..Default::default()
            });
        }
    }
    commands
}
pub fn references() -> Vec<(String, String)> {
    catalogue()
        .into_iter()
        .map(|c| {
            (
                format!("/{} {}", c.name, c.hint).trim().into(),
                c.description,
            )
        })
        .collect()
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn roots_and_legacy_routes_are_complete() {
        let c = catalogue();
        assert_eq!(
            c.iter()
                .filter(|c| c.primary)
                .map(|c| c.name.as_str())
                .collect::<Vec<_>>(),
            ["conversations", "work", "settings"]
        );
        for (name, ..) in BUILTIN {
            assert!(
                c.iter()
                    .any(|c| c.name == *name || c.aliases.iter().any(|a| a == name)),
                "{name}"
            );
        }
        let mut names = std::collections::HashSet::new();
        for c in &c {
            assert!(names.insert(c.name.as_str()));
            for a in &c.aliases {
                assert!(names.insert(a.as_str()));
            }
        }
        assert!(available_during_turn("details"));
        assert!(!available_during_turn("restore"));
        assert_ne!(effect("review"), effect("code-review"));
    }
}
