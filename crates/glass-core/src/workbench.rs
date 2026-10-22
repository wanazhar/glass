//! Local destinations use the shared registry; domain handlers own all effects.
use crate::{
    actions::{self, Group},
    input::KeyReader,
    modal,
};
pub fn choose(keys: &KeyReader, group: Group) -> Option<String> {
    let commands: Vec<_> = actions::catalogue()
        .into_iter()
        .filter(|command| !command.primary && actions::group(&command.name) == group)
        .collect();
    let labels: Vec<_> = commands
        .iter()
        .map(|c| format!("{} · {}", c.name, c.description))
        .collect();
    let search: Vec<_> = commands
        .iter()
        .map(|c| format!("{} {} {}", c.name, c.keywords, c.description))
        .collect();
    let title = match group {
        Group::Conversations => "Conversations",
        Group::Work => "Work",
        Group::Settings => "Settings",
    };
    modal::choose_search(
        keys,
        title,
        "Search actions · effects shown before selection · Esc back",
        &labels,
        &search,
    )
    .map(|index| format!("/{}", commands[index].name))
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) enum Key {
    Record(String, usize),
    Interrupted(String, usize),
    Job(String, u32),
    Loop(String, usize),
    Task(u32),
    Agent(String),
    Attention(String, String),
    Command(String),
}
enum Target {
    Output(crate::session::Record),
    Job(u32),
    Loop(usize, String),
    Detail(String),
    Value(serde_json::Value),
    Command(String),
}
struct Item {
    key: Key,
    label: String,
    target: Target,
}
#[derive(Clone, Copy)]
pub enum Scope {
    All,
    Activity,
    Attention,
}
fn projection(session: &crate::session::Session, jobs: &str, scope: Scope) -> Vec<Item> {
    use crate::polish::ActivityAction;
    let mut items = Vec::new();
    if !matches!(scope, Scope::Attention) {
        let records = crate::polish::raw_records(session);
        let mut turn = 0usize;
        let mut epoch = 0usize;
        let scopes: Vec<_> = records
            .iter()
            .map(|record| {
                if matches!(record, crate::session::Record::User(_)) {
                    turn += 1;
                }
                if matches!(record, crate::session::Record::Compacted { .. }) {
                    epoch += 1;
                }
                (turn, epoch)
            })
            .collect();
        let mut shell_counts = std::collections::HashMap::new();
        let mut result_counts = std::collections::HashMap::new();
        let mut call_counts = std::collections::HashMap::new();
        for (index, record) in records.iter().enumerate() {
            let (counts, id) = match record {
                crate::session::Record::Bash {
                    call_id: Some(id), ..
                } => (&mut shell_counts, id),
                crate::session::Record::ToolResult { call_id, .. } => (&mut result_counts, call_id),
                crate::session::Record::ToolCall { id, .. } => (&mut call_counts, id),
                _ => continue,
            };
            *counts.entry((scopes[index], id.as_str())).or_insert(0usize) += 1;
        }
        // Reuse canonical turn/compaction-scoped pairing, including malformed IDs.
        for (index, entry) in session
            .history_entries()
            .into_iter()
            .enumerate()
            .rev()
            .filter(|(_, e)| {
                e.tool_card
                    .as_ref()
                    .is_some_and(|card| card.status == "interrupted")
            })
            .take(100)
        {
            let card = entry.tool_card.expect("filtered tool card");
            items.push(Item {
                key: Key::Interrupted(session.id.clone(), index),
                label: format!(
                    "Tool · interrupted · {}",
                    glass_tui::text::clip(&card.command, 60)
                ),
                target: Target::Detail(crate::session::tool_card_text(&card, true)),
            });
        }
        for (index,record) in records.iter().enumerate().rev().filter(|(_,r)|matches!(r,crate::session::Record::Bash{..}|crate::session::Record::ToolResult{..})).filter(|(index,r)|!matches!(r,crate::session::Record::ToolResult {call_id,name,..} if name=="bash" && shell_counts.get(&(scopes[*index], call_id.as_str())) == Some(&1) && result_counts.get(&(scopes[*index], call_id.as_str())) == Some(&1) && call_counts.get(&(scopes[*index], call_id.as_str())).copied().unwrap_or(0) <= 1)).take(100) {
            let label=match record {crate::session::Record::Bash{command,exit_code,..}=>format!("Tool · exit {} · {}",exit_code.map(|n|n.to_string()).unwrap_or_else(||"unknown".into()),glass_tui::text::clip(command,60)),crate::session::Record::ToolResult{name,..}=>format!("Tool · {name}"),_=>unreachable!()};
            items.push(Item{key:Key::Record(session.id.clone(),index),label,target:Target::Output(record.clone())});
        }
        for item in crate::polish::activity_items(session, jobs) {
            let (key, target) = match item.action {
                ActivityAction::Job(id) => (Key::Job(session.id.clone(), id), Target::Job(id)),
                ActivityAction::Loop(index) => (
                    Key::Loop(session.id.clone(), index),
                    Target::Loop(index, item.detail),
                ),
                _ => continue,
            };
            items.push(Item {
                key,
                label: format!("{} · {}", item.status, item.label),
                target,
            });
        }
    }
    if !matches!(scope, Scope::Activity) {
        for target in crate::attention::targets(session) {
            // Job entries are already represented by their exact owner/id above.
            if let Some(id) = target["job"].as_u64().and_then(|n| u32::try_from(n).ok()) {
                let key = Key::Job(session.id.clone(), id);
                if !items.iter().any(|item| item.key == key) {
                    items.push(Item {
                        key,
                        label: format!("Attention · job {id}"),
                        target: Target::Job(id),
                    });
                }
                continue;
            }
            items.push(Item {
                key: Key::Attention(
                    session.id.clone(),
                    format!(
                        "{}:{}:{}:{}:{}",
                        target["kind"],
                        target["raw_record"],
                        target["agent"],
                        target["finding_id"],
                        target["task"]
                    ),
                ),
                label: format!(
                    "Attention · {} · {}",
                    target["kind"].as_str().unwrap_or("event"),
                    glass_tui::text::clip(target["preview"].as_str().unwrap_or(""), 64)
                ),
                target: Target::Value(target),
            });
        }
    }
    if matches!(scope, Scope::All) {
        let root = crate::polish::metadata(session).workspace;
        if let Ok(board) = crate::workspace::load(&root) {
            for task in board.tasks.iter().take(128) {
                items.push(Item {
                    key: Key::Task(task.id),
                    label: format!("Task {} · {} · {}", task.id, task.status, task.title),
                    target: Target::Value(serde_json::json!(task)),
                });
            }
        }
        if let Some(hub) = crate::team::get(&session.id) {
            for agent in hub.snapshot().into_iter().take(32) {
                items.push(Item{key:Key::Agent(agent.id.clone()),label:format!("Teammate · {} · {}",agent.name,agent.status),target:Target::Detail(format!("### {}\n\n{} · {}\n\n{}\n\n/agents opens the interactive transcript and controls.",agent.name,agent.model,agent.status,glass_tui::text::clip(&agent.result,4096)))});
            }
        }
    }
    if matches!(scope, Scope::All) {
        for command in actions::catalogue().into_iter().filter(|c| {
            !c.primary
                && actions::group(&c.name) == Group::Work
                && !matches!(c.name.as_str(), "attention" | "activity")
        }) {
            items.push(Item {
                key: Key::Command(command.name.clone()),
                label: format!("/{} · {}", command.name, command.description),
                target: Target::Command(format!("/{}", command.name)),
            });
        }
    }
    items
}
pub fn view(
    keys: &KeyReader,
    session: &crate::session::Session,
    jobs: &mut crate::jobs::Handle,
    scope: Scope,
) -> Option<String> {
    loop {
        let items = projection(session, &jobs.list(), scope);
        if items.is_empty() {
            modal::info(
                keys,
                "Work",
                "No matching recorded activity in this conversation.",
            );
            return None;
        }
        let labels: Vec<_> = items.iter().map(|item| item.label.clone()).collect();
        let searchable: Vec<_> = items.iter().map(|item| item.label.clone()).collect();
        let title = match scope {
            Scope::All => "Work",
            Scope::Activity => "Activity",
            Scope::Attention => "Attention",
        };
        let index = modal::choose_search(
            keys,
            title,
            "Current project / conversation · selected details load on demand",
            &labels,
            &searchable,
        )?;
        let item = &items[index];
        match &item.target {
            Target::Command(command) => return Some(command.clone()),
            Target::Output(record) => crate::output_view::inspect(keys, record),
            Target::Detail(text) => modal::rich_info(keys, &item.label, text),
            Target::Value(value) => {
                modal::rich_info(keys, &item.label, &crate::structured::render(value))
            }
            Target::Job(id) => {
                match modal::rich_actions(
                    keys,
                    &item.label,
                    &jobs.output(*id).unwrap_or_else(|error| error),
                    &[('o', "live output / stdin"), ('s', "stop")],
                ) {
                    Some('o') => {
                        if let Err(error) = jobs.inspect(keys, *id) {
                            modal::info(keys, "Job unavailable", &error);
                        }
                    }
                    Some('s')
                        if modal::choose(
                            keys,
                            "Stop background job?",
                            &item.label,
                            &["Cancel".into(), "Stop owned job".into()],
                        ) == Some(1) =>
                    {
                        let result = jobs.stop(*id);
                        modal::info(
                            keys,
                            "Background job",
                            &result.unwrap_or_else(|error| error),
                        );
                    }
                    _ => {}
                }
            }
            Target::Loop(index, text) => {
                // The selection is from this snapshot; verify it before a destructive action.
                let before =
                    serde_json::to_string(&crate::loops::load(session).entries.get(*index))
                        .unwrap_or_default();
                if modal::rich_actions(keys, &item.label, text, &[('d', "remove")]) == Some('d')
                    && modal::choose(
                        keys,
                        "Remove loop?",
                        &item.label,
                        &["Cancel".into(), "Remove loop".into()],
                    ) == Some(1)
                {
                    let after =
                        serde_json::to_string(&crate::loops::load(session).entries.get(*index))
                            .unwrap_or_default();
                    let result = if before == after {
                        crate::loops::command(session, &format!("rm {}", index + 1))
                    } else {
                        Err("Loop changed; reopen Work and select it again".into())
                    };
                    modal::info(
                        keys,
                        "Scheduled loop",
                        &result.unwrap_or_else(|error| error),
                    );
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn keys_separate_owners_and_sources() {
        assert_ne!(Key::Job("a".into(), 1), Key::Job("b".into(), 1));
        assert_ne!(
            Key::Record("a".into(), 1),
            Key::Attention("a".into(), "1".into())
        );
        assert_ne!(Key::Task(1), Key::Loop("a".into(), 1));
    }
}
