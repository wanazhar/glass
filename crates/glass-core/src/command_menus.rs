//! Discoverable subcommands. Handlers remain responsible for effects and validation.
use glass_tui::palette::Command;
const CHILDREN: &[(&str, &str, &str)] = &[
    (
        "agents view",
        "Conversations",
        "Read teammate conversations and tool output",
    ),
    (
        "agents inspect",
        "Inspect teammate",
        "Choose a teammate and review its status",
    ),
    (
        "agents interrupt",
        "Interrupt a task",
        "Choose an active teammate to interrupt",
    ),
    (
        "agents archive",
        "Archive teammate",
        "Keep its saved conversation and remove it from active work",
    ),
    (
        "agents budget",
        "Token allowance",
        "Choose a teammate and set its cumulative token allowance",
    ),
    (
        "agents status",
        "Workflow status",
        "Review delegation state and remaining allowance",
    ),
    (
        "agents resume",
        "Resume delegation",
        "Enable new assignments without raising token allowances",
    ),
    (
        "agents stop",
        "Pause delegation",
        "Stop automatic continuation and park current work",
    ),
    (
        "desktop quick",
        "Quick desktop",
        "Open a temporary XFCE desktop",
    ),
    (
        "desktop reusable",
        "Reusable workspace",
        "Open GNOME with its saved profile",
    ),
    (
        "desktop attach",
        "Attach to desktop",
        "Use the current desktop and preserve its input owner",
    ),
    (
        "desktop status",
        "Desktop status",
        "Inspect the desktop without starting one",
    ),
    (
        "desktop stop",
        "Stop desktop",
        "Close desktop processes and retain workspace files",
    ),
    (
        "constraints list",
        "View reminders",
        "Review saved task reminders",
    ),
    (
        "constraints add",
        "Add a reminder",
        "Enter a reminder; adding does not enable reminders",
    ),
    (
        "constraints on",
        "Enable reminders",
        "Include reminders once per new user turn",
    ),
    (
        "constraints off",
        "Disable reminders",
        "Keep saved reminders without including them",
    ),
    (
        "constraints clear",
        "Clear reminders",
        "Remove the saved reminder list",
    ),
    ("loop list", "View schedules", "Review idle-time schedules"),
    (
        "loop create",
        "Create a schedule",
        "Choose an interval and enter the task",
    ),
    (
        "loop history",
        "Schedule history",
        "Review recorded deliveries",
    ),
    (
        "loop rm",
        "Remove a schedule",
        "Choose a saved schedule to remove",
    ),
    (
        "loop bind",
        "Bind a schedule",
        "Pin a schedule to this host, workspace, and model",
    ),
    (
        "loop missed",
        "Missed-run policy",
        "Choose whether a missed run fires once or is skipped",
    ),
    (
        "loop clear",
        "Clear schedules",
        "Remove all schedules after confirmation",
    ),
    (
        "prewalk status",
        "Handoff status",
        "Review the configured target model",
    ),
    (
        "prewalk start",
        "Hand off to target",
        "Continue with the configured target model",
    ),
    (
        "prewalk off",
        "Disable handoff",
        "Disable the configured handoff target",
    ),
];
pub fn extend(commands: &mut Vec<Command>) {
    for (name, label) in [
        ("agents", "Teammates"),
        ("desktop", "Desktop"),
        ("constraints", "Task reminders"),
        ("loop", "Schedules"),
        ("prewalk", "Model handoff"),
    ] {
        if let Some(command) = commands.iter_mut().find(|command| command.name == name) {
            command.label = label.into();
        }
    }
    for (name, _, _) in CHILDREN {
        let parent = name.split_once(' ').unwrap().0;
        if let Some(command) = commands.iter_mut().find(|command| command.name == parent)
            && command.hint.is_empty()
        {
            command.hint = "[action]".into();
        }
    }
    for (name, label, description) in CHILDREN {
        commands.push(Command {
            name: (*name).into(),
            label: (*label).into(),
            description: (*description).into(),
            ..Default::default()
        });
    }
}
pub fn has_children(text: &str) -> bool {
    let prefix = format!("{} ", text.trim_start_matches('/'));
    CHILDREN
        .iter()
        .any(|(name, _, _)| name.starts_with(&prefix))
}

pub fn choose(keys: &crate::input::KeyReader, text: &str) -> Option<String> {
    let prefix = format!("{} ", text.trim_start_matches('/'));
    let children = CHILDREN
        .iter()
        .filter(|(name, _, _)| name.starts_with(&prefix))
        .collect::<Vec<_>>();
    let labels = children
        .iter()
        .map(|(_, label, description)| format!("{label} — {description}"))
        .collect::<Vec<_>>();
    crate::modal::choose(keys, text, "Choose an action", &labels)
        .map(|index| format!("/{}", children[index].0))
}

#[cfg(test)]
pub fn children() -> impl Iterator<Item = &'static str> {
    CHILDREN.iter().map(|(name, _, _)| *name)
}

fn field(
    keys: &crate::input::KeyReader,
    title: &str,
    label: &str,
    initial: &str,
) -> Option<String> {
    crate::modal::field(keys, title, label, initial, false).filter(|text| !text.trim().is_empty())
}

/// Fill required arguments without escaping the guided command flow.
pub fn prepare(
    keys: &crate::input::KeyReader,
    session: &crate::session::Session,
    cfg: &crate::config::Config,
    text: &str,
) -> Result<Option<String>, String> {
    use crate::modal;
    let _screen = matches!(
        text,
        "/loop create"
            | "/loop rm"
            | "/loop bind"
            | "/loop missed"
            | "/send"
            | "/forget"
            | "/unforget"
            | "/rewrite"
    )
    .then(modal::Screen::new);
    let result = match text {
        "/constraints add" => field(keys, "Add reminder", "Reminder (up to 512 bytes)", "")
            .map(|value| format!("/constraints add {value}")),
        "/constraints clear" | "/loop clear" => {
            let what = if text.starts_with("/loop") {
                "saved schedules"
            } else {
                "saved reminders"
            };
            (modal::choose(
                keys,
                "Clear saved items",
                &format!("Remove all {what}?"),
                &["Cancel".into(), format!("Clear {what}")],
            ) == Some(1))
            .then(|| text.into())
        }
        "/loop create" => loop {
            let choices = [
                "Every minute",
                "Every 5 minutes",
                "Every 30 minutes",
                "Every hour",
                "Custom interval…",
            ]
            .map(str::to_owned);
            let Some(index) = modal::choose(
                keys,
                "Schedule interval",
                "Runs when this session is idle",
                &choices,
            ) else {
                return Ok(None);
            };
            let interval = if index == 4 {
                field(
                    keys,
                    "Schedule interval",
                    "Positive interval, such as 90s, 30m, or 2h",
                    "30m",
                )
            } else {
                Some(["1m", "5m", "30m", "1h"][index].into())
            };
            let Some(interval) = interval else {
                continue;
            };
            if let Some(task) = field(keys, "Scheduled task", "What should the main agent do?", "")
            {
                break Some(format!("/loop {interval} {task}"));
            }
        },
        "/loop rm" | "/loop bind" | "/loop missed" => loop {
            let loops = crate::loops::load(session);
            if loops.entries.is_empty() {
                modal::info(
                    keys,
                    "Schedules",
                    "No schedules yet. Choose Create a schedule to add one.",
                );
                return Ok(None);
            }
            let items = loops
                .entries
                .iter()
                .enumerate()
                .map(|(i, entry)| {
                    format!("{}. {}", i + 1, glass_tui::text::clip(&entry.prompt, 90))
                })
                .collect::<Vec<_>>();
            let Some(index) =
                modal::choose(keys, "Choose schedule", "Select a saved schedule", &items)
            else {
                return Ok(None);
            };
            let n = index + 1;
            if text.ends_with("missed") {
                if let Some(policy) = modal::choose(
                    keys,
                    "Missed-run policy",
                    &items[index],
                    &["Run once when available".into(), "Skip missed runs".into()],
                ) {
                    break Some(format!("/loop missed {n} {}", ["once", "skip"][policy]));
                }
            } else {
                if text == "/loop rm"
                    && modal::choose(
                        keys,
                        "Remove schedule",
                        &items[index],
                        &["Cancel".into(), "Remove schedule".into()],
                    ) != Some(1)
                {
                    continue;
                }
                break Some(format!("{text} {n}"));
            }
        },
        "/send" => loop {
            let sessions = crate::polish::sessions(&cfg.workspace)
                .into_iter()
                .filter(|other| other.id != session.id)
                .collect::<Vec<_>>();
            if sessions.is_empty() {
                modal::info(
                    keys,
                    "Send message",
                    "No other conversations in this workspace. Start another conversation before sending a message.",
                );
                return Ok(None);
            }
            let labels = sessions
                .iter()
                .map(crate::polish::title)
                .collect::<Vec<_>>();
            let Some(index) =
                modal::choose(keys, "Send to conversation", "Choose a recipient", &labels)
            else {
                return Ok(None);
            };
            if let Some(message) = field(
                keys,
                "Send message",
                &format!("Message to {}", labels[index]),
                "",
            ) {
                break Some(format!("/send {} {message}", sessions[index].id));
            }
        },
        "/forget" | "/unforget" | "/rewrite" => loop {
            let entries = session.history_entries();
            let mut turns = std::collections::BTreeMap::new();
            for entry in entries.into_iter().filter(|entry| {
                entry.role == glass_tui::Role::User && !entry.text.starts_with("[image:")
            }) {
                turns.entry(entry.turn).or_insert(entry.text);
            }
            let turns = turns.into_iter().collect::<Vec<_>>();
            if turns.is_empty() {
                modal::info(
                    keys,
                    "Conversation context",
                    "There are no user turns to edit yet.",
                );
                return Ok(None);
            }
            let labels = turns
                .iter()
                .map(|(n, text)| {
                    format!(
                        "Turn {n} · {}",
                        glass_tui::text::clip(&text.replace('\n', " "), 80)
                    )
                })
                .collect::<Vec<_>>();
            let Some(index) = modal::choose(
                keys,
                "Choose conversation turn",
                "Original history and files are retained",
                &labels,
            ) else {
                return Ok(None);
            };
            let (turn, original) = &turns[index];
            if text == "/rewrite" {
                if let Some(replacement) =
                    field(keys, "Rewrite future context", "Replacement text", original)
                {
                    break Some(format!("/rewrite {turn} {replacement}"));
                }
            } else {
                break Some(format!("{text} {turn}"));
            }
        },
        "/prewalk status" => Some("/prewalk inspect".into()),
        _ => Some(text.to_owned()),
    };
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn every_registered_route_is_selectable_without_typing_arguments() {
        use glass_tui::{
            Editor,
            crossterm::event::{KeyCode, KeyEvent, KeyModifiers},
            palette::Palette,
        };
        let mut commands = crate::actions::catalogue();
        extend(&mut commands);
        for command in &commands {
            for name in std::iter::once(&command.name).chain(command.aliases.iter()) {
                let mut palette = Palette::new(commands.clone());
                let mut editor = Editor::new();
                palette.open_managed(&mut editor).unwrap();
                editor.set_text(&format!("/{name}"));
                let result = palette.key(
                    &mut editor,
                    KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
                );
                if has_children(&command.name) {
                    assert!(result.is_none(), "{name} must open its actions");
                    assert_eq!(editor.text(), format!("/{} ", command.name));
                } else {
                    assert_eq!(
                        result.as_deref(),
                        Some(format!("/{name}").as_str()),
                        "{name} has no selectable route"
                    );
                }
            }
        }
    }
    #[test]
    fn subcommands_have_registered_parents_and_unique_routes() {
        let mut commands = crate::actions::catalogue();
        extend(&mut commands);
        let mut names = std::collections::HashSet::new();
        for command in &commands {
            assert!(names.insert(&command.name), "{}", command.name);
        }
        for child in children() {
            let (parent, _) = child.split_once(' ').unwrap();
            assert!(
                commands.iter().any(|command| command.name == parent),
                "{child}"
            );
        }
    }
}
