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
