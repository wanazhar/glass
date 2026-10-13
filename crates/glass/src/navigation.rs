//! On-demand navigation metadata. Transcript bodies load only after selection.
use crate::{config, session::Session};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

#[derive(Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Receipt {
    pub model: String,
    pub branch: String,
    pub status: String,
    pub parent: Option<String>,
    pub fork_turn: Option<usize>,
    pub updated: u64,
    pub events: usize,
    pub viewed: usize,
    pub process: Option<(u32, String)>,
}
fn path(id: &str) -> PathBuf {
    config::data_dir().join(format!("{id}.nav.json"))
}
fn read(path: &Path) -> Receipt {
    crate::requests::bounded(path, 8192)
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default()
}
pub fn load(id: &str) -> Receipt {
    read(&path(id))
}
fn save(session: &Session, receipt: &Receipt) -> Result<(), String> {
    crate::auth::secure_write(
        &session.path.with_extension("nav.json"),
        &serde_json::to_vec(receipt).map_err(|e| e.to_string())?,
    )
}
fn process_stamp(pid: u32) -> Option<String> {
    let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    // Field 22, after the parenthesized process name (which may contain spaces).
    Some(
        stat.rsplit_once(") ")?
            .1
            .split_whitespace()
            .nth(19)?
            .to_owned(),
    )
}
pub fn viewed(session: &Session) -> Result<(), String> {
    let mut r = read(&session.path.with_extension("nav.json"));
    r.viewed = session.log_len();
    r.events = r.events.max(r.viewed);
    save(session, &r)
}
pub fn lineage(session: &Session, parent: &str, through: usize) -> Result<(), String> {
    let r = Receipt {
        parent: Some(parent.into()),
        fork_turn: Some(through),
        ..Default::default()
    };
    save(session, &r)
}
pub fn record(
    session: &Session,
    workspace: &Path,
    model: &str,
    status: &str,
) -> Result<(), String> {
    let mut r = read(&session.path.with_extension("nav.json"));
    r.model = model.into();
    r.status = status.into();
    r.updated = crate::loops::now();
    r.events = session.log_len();
    let pid = std::process::id();
    r.process = (status == "running")
        .then(|| process_stamp(pid).map(|stamp| (pid, stamp)))
        .flatten();
    r.branch = std::process::Command::new("git")
        .current_dir(workspace)
        .args(["symbolic-ref", "--quiet", "--short", "HEAD"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| {
            String::from_utf8_lossy(&o.stdout)
                .trim()
                .chars()
                .take(128)
                .collect()
        })
        .unwrap_or_else(|| "detached or no Git".into());
    save(session, &r)
}
pub fn inspect(workspace: &Path) -> Value {
    let indexed = crate::resume_picker::all(&config::data_dir());
    let omitted = indexed.len().saturating_sub(512);
    let rows = indexed.into_iter().take(512).map(|thread| {
        let r = load(&thread.id);
        use std::os::unix::fs::FileTypeExt;
        let live = std::fs::symlink_metadata(config::data_dir().join(format!("serve-{}.sock",thread.id)))
            .is_ok_and(|m|m.file_type().is_socket());
        let running = r.process.as_ref().is_some_and(|(pid,stamp)|process_stamp(*pid).as_ref()==Some(stamp));
        let status = if r.status=="running" && !running {"interrupted"} else {&r.status};
        let team = crate::team::get(&thread.id).map(|h|h.snapshot().len());
        json!({"session":thread.id,"title":thread.title,"workspace":thread.workspace,
            "modified":thread.modified,"model":r.model,"branch":r.branch,"last_status":status,
            "running_process_verified":running,"server_socket_present":live,
            "server_liveness":"Socket presence is not a connectivity probe.",
            "unread_events":r.events.saturating_sub(r.viewed),"recorded_events":r.events,
            "viewed_cursor":r.viewed,"unread_scope":"Recorded events since last explicitly viewed cursor; not messages.",
            "teammates_in_this_process":team,"parent":r.parent,"fork_turn":r.fork_turn,
            "current_workspace":thread.workspace==workspace})
    }).collect::<Vec<_>>();
    json!({"threads":rows,"limit":512,"omitted_threads":omitted,"branch_families_complete":omitted==0,"history_loaded":false})
}
fn family(all: &[Value], session: &str) -> BTreeSet<String> {
    let mut edges: std::collections::HashMap<&str, Vec<&str>> = std::collections::HashMap::new();
    for row in all {
        if let (Some(id), Some(parent)) = (row["session"].as_str(), row["parent"].as_str()) {
            edges.entry(id).or_default().push(parent);
            edges.entry(parent).or_default().push(id);
        }
    }
    let mut ids = BTreeSet::new();
    let mut pending = vec![session];
    while let Some(id) = pending.pop() {
        if ids.insert(id.to_owned()) && let Some(neighbors) = edges.get(id) {
            pending.extend(neighbors.iter().copied());
        }
    }
    ids
}
/// One metadata-first chooser for resume, sessions and branch alternatives.
/// Only the selected bounded tail is read; mutation actions remain explicit.
pub(crate) fn choose(
    keys: &crate::input::KeyReader,
    workspace: &Path,
    current: Option<&Session>,
    all_projects: bool,
    branches: bool,
    direct: bool,
) -> Result<Option<crate::resume_picker::Pick>, String> {
    use crate::{modal, resume_picker};
    let mut all = all_projects;
    let mut page = 0usize;
    let mut query = String::new();
    loop {
        let threads = if all || branches {
            resume_picker::all(&config::data_dir())
        } else {
            resume_picker::index(&config::data_dir(), workspace)
        };
        let relatives = if branches {
            let rows: Vec<_> = threads
                .iter()
                .map(|t| json!({"session":t.id,"parent":load(&t.id).parent}))
                .collect();
            family(&rows, current.map(|s| s.id.as_str()).unwrap_or(""))
        } else {
            BTreeSet::new()
        };
        let candidates: Vec<_> = threads
            .iter()
            .filter(|t| !branches || relatives.contains(&t.id))
            .filter(|t| {
                let text = format!("{} {} {}", t.title, t.id, t.workspace.display()).to_lowercase();
                query
                    .split_whitespace()
                    .all(|word| text.contains(&word.to_lowercase()))
            })
            .collect();
        const PAGE: usize = 64;
        page = page.min(candidates.len().saturating_sub(1) / PAGE);
        let visible: Vec<_> = candidates
            .iter()
            .skip(page * PAGE)
            .take(PAGE)
            .copied()
            .collect();
        let mut labels: Vec<_> = visible
            .iter()
            .map(|t| {
                let receipt = load(&t.id);
                format!(
                    "{} · {} · {} unread events{}",
                    glass_tui::text::clip(&t.title, 48),
                    t.id,
                    receipt.events.saturating_sub(receipt.viewed),
                    if current.is_some_and(|s| s.id == t.id) {
                        " · current"
                    } else {
                        ""
                    }
                )
            })
            .collect();
        let count = labels.len();
        labels.extend([
            "Search all indexed conversations…".into(),
            if all {
                "Current project only".into()
            } else {
                "All projects".into()
            },
            "Next page".into(),
            "Previous page".into(),
        ]);
        let searchable: Vec<_> = visible
            .iter()
            .zip(&labels)
            .map(|(t, label)| {
                format!(
                    "{label} {} {} {}",
                    t.modified,
                    t.workspace.display(),
                    load(&t.id).model
                )
            })
            .chain(labels[count..].iter().cloned())
            .collect();
        let Some(index) = modal::choose_search(
            keys,
            if branches {
                "Conversation alternatives"
            } else {
                "Conversations"
            },
            &format!(
                "{} · page {}/{} · search this page; search all below",
                if candidates.is_empty() { "No conversations matched" } else if all {
                    "All projects"
                } else {
                    "Current project"
                },
                page + 1,
                candidates.len().div_ceil(PAGE).max(1)
            ),
            &labels,
            &searchable,
        ) else {
            return Ok(None);
        };
        if index >= count {
            match index - count {
                0 => {
                    if let Some(text) = modal::field(
                        keys,
                        "Search conversations",
                        "Search every indexed title, ID or project",
                        &query,
                        false,
                    ) {
                        query = text;
                        page = 0;
                    }
                }
                1 => {
                    all = !all;
                    page = 0;
                }
                2 => page = (page + 1).min(candidates.len().saturating_sub(1) / PAGE),
                _ => page = page.saturating_sub(1),
            }
            continue;
        }
        let thread = visible[index];
        let pick = || resume_picker::Pick {
            id: thread.id.clone(),
            workspace: thread.workspace.clone(),
        };
        if direct {
            return Ok(Some(pick()));
        }
        loop {
            let receipt = load(&thread.id);
            let path = config::data_dir().join(format!("{}.jsonl", thread.id));
            let preview = preview(&path).unwrap_or_else(|e| format!("Preview unavailable: {e}"));
            let detail = format!(
                "### {}\n\n{} · {}\n\nStatus: {} · model: {}\n\nParent: {} · fork turn: {}\n\n{} unread recorded events (not messages). Preview does not change the read cursor.\n\n---\n\n{}\n\n---\n\nr resume · p full preview · n rename · f fork · s follow-up · d delete",
                thread.title,
                thread.workspace.display(),
                thread.modified,
                receipt.status,
                receipt.model,
                receipt.parent.as_deref().unwrap_or("none"),
                receipt
                    .fork_turn
                    .map(|n| n.to_string())
                    .unwrap_or_else(|| "none".into()),
                receipt.events.saturating_sub(receipt.viewed),
                preview
            );
            let action = modal::rich_actions(
                keys,
                "Conversation",
                &detail,
                &[
                    ('r', "resume"),
                    ('p', "preview"),
                    ('n', "rename"),
                    ('f', "fork"),
                    ('s', "send"),
                    ('d', "delete"),
                ],
            );
            let result: Result<Option<resume_picker::Pick>, String> = (|| {
                match action {
                    Some('r') => {
                        if current.is_some() && thread.workspace != workspace {
                            return Err(format!(
                                "Other workspace: open gc in {} and run gc resume {}.",
                                thread.workspace.display(),
                                thread.id
                            ));
                        }
                        return Ok(Some(pick()));
                    }
                    Some('p') => {
                        let s = Session::open_readonly(&config::data_dir(), &thread.id)
                            .map_err(|e| e.to_string())?;
                        let mut reader = crate::reader::Reader::overlay(
                            s.history_entries(),
                            "Conversation preview",
                        )
                        .map_err(|e| e.to_string())?;
                        while let Some(event) = keys.next_blocking() {
                            if !matches!(
                                reader.event(&event, false).map_err(|e| e.to_string())?,
                                crate::reader::Action::Handled
                            ) {
                                break;
                            }
                        }
                    }
                    Some('n') => {
                        if let Some(title) =
                            modal::field(keys, "Rename conversation", "Title", &thread.title, false)
                        {
                            let s = Session::open_readonly(&config::data_dir(), &thread.id)
                                .map_err(|e| e.to_string())?;
                            let mut meta = selected_metadata(&s, &thread.workspace)?;
                            meta.title = title;
                            crate::polish::save(&s, &meta)?;
                        }
                    }
                    Some('f') => {
                        if let Some(through) = modal::validated_field(
                            keys,
                            "Fork conversation",
                            "Through turn number (positive)",
                            "1",
                            false,
                            |s| {
                                if s.parse::<usize>().is_ok_and(|n| n > 0) {
                                    Ok(())
                                } else {
                                    Err("Enter a positive turn number".into())
                                }
                            },
                        ) {
                            let s = Session::open_readonly(&config::data_dir(), &thread.id)
                                .map_err(|e| e.to_string())?;
                            let meta = selected_metadata(&s, &thread.workspace)?;
                            if meta.workspace != thread.workspace {
                                return Err("Workspace changed; reopen the chooser".into());
                            }
                            if current.is_some() && thread.workspace != workspace {
                                return Err(
                                    "Switch workspace before forking this conversation".into()
                                );
                            }
                            let fork = s
                                .branch(&config::data_dir(), through.parse().unwrap())
                                .map_err(|e| e.to_string())?;
                            let mut meta = meta;
                            meta.title = format!("{} (branch)", thread.title);
                            crate::polish::save(&fork, &meta)?;
                            return Ok(Some(resume_picker::Pick {
                                id: fork.id.clone(),
                                workspace: thread.workspace.clone(),
                            }));
                        }
                    }
                    Some('s') => {
                        if let Some(text) = modal::field(
                            keys,
                            "Background follow-up",
                            "Saved to inbox; delivery requires a running idle target",
                            "",
                            false,
                        ) {
                            crate::inbox::send(
                                &thread.id,
                                &text,
                                current.map(|s| s.id.as_str()).unwrap_or("human"),
                            )?;
                            modal::info(
                                keys,
                                "Follow-up saved",
                                "Inbox accepted; provider completion is separate.",
                            );
                        }
                    }
                    Some('d') => {
                        if current.is_some_and(|s| s.id == thread.id) {
                            return Err(
                                "Switch conversations before deleting the current one".into()
                            );
                        }
                        if modal::choose(
                            keys,
                            "Delete conversation?",
                            "Deletes transcript and preferences; workspace files and checkpoints stay.",
                            &["Keep conversation".into(), "Delete permanently".into()],
                        ) == Some(1)
                        {
                            // Hold the writer lock; never unlink a live writer's transcript.
                            let s = Session::open(&config::data_dir(), &thread.id)
                                .map_err(|e| e.to_string())?;
                            std::fs::remove_file(&s.path).map_err(|e| e.to_string())?;
                            for ext in ["meta.json", "cost.json", "context", "nav.json"] {
                                let p = s.path.with_extension(ext);
                                if p.exists() {
                                    std::fs::remove_file(p).map_err(|e| e.to_string())?;
                                }
                            }
                        }
                    }
                    _ => {}
                }
                Ok(None)
            })();
            match result {
                Ok(Some(pick)) => return Ok(Some(pick)),
                Err(e) => modal::info(keys, "Conversation action unavailable", &e),
                _ => {}
            }
            if matches!(action, None | Some('n') | Some('d')) {
                break;
            }
        }
    }
}
fn selected_metadata(
    session: &Session,
    workspace: &Path,
) -> Result<crate::polish::Metadata, String> {
    let meta: crate::polish::Metadata = serde_json::from_slice(&crate::requests::bounded(
        &session.path.with_extension("meta.json"),
        16 * 1024 * 1024,
    )?)
    .map_err(|_| "Invalid metadata; nothing changed")?;
    if meta.workspace != workspace {
        return Err("Workspace changed; reopen the chooser".into());
    }
    Ok(meta)
}
fn preview(path: &Path) -> Result<String, String> {
    let rows = crate::requests::tail(path)?;
    let mut text = String::new();
    for row in rows
        .iter()
        .rev()
        .take(32)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
    {
        let body = match row["k"].as_str() {
            Some("user") => format!("**You** · {}", row["t"].as_str().unwrap_or("")),
            Some("assistant") => row["t"].as_str().unwrap_or("").to_owned(),
            _ => continue,
        };
        let body = glass_tui::text::clip(&body, 2048);
        if text.len() + body.len() + 2 > 16384 {
            break;
        }
        text.push_str(&body);
        text.push_str("\n\n");
    }
    Ok(if text.is_empty() {
        "No chat text in this bounded tail preview.".into()
    } else {
        text
    })
}
pub fn view(
    keys: &crate::input::KeyReader,
    session: &Session,
    workspace: &Path,
    branches: bool,
) -> Option<String> {
    match choose(keys, workspace, Some(session), branches, branches, false) {
        Ok(pick) => pick.map(|p| p.id),
        Err(e) => {
            crate::modal::info(keys, "Conversations unavailable", &e);
            None
        }
    }
}

pub fn command(args: &[String]) -> Result<(), String> {
    if args.iter().any(|a| a != "--json") || args.len() > 1 {
        return Err("gc hub [--json]".into());
    }
    let value = inspect(
        &std::env::current_dir()
            .map_err(|e| e.to_string())?
            .canonicalize()
            .map_err(|e| e.to_string())?,
    );
    if args.iter().any(|a| a == "--json") {
        crate::emit_json(value);
    } else {
        crate::cli::report("Workspace hub", &value);
    }
    Ok(())
}

#[cfg(test)]
mod chooser_tests {
    use super::*;
    #[test]
    fn family_preserves_ancestors_siblings_descendants_and_cycles() {
        let rows = vec![
            json!({"session":"root"}),
            json!({"session":"a","parent":"root"}),
            json!({"session":"b","parent":"root"}),
            json!({"session":"c","parent":"a"}),
            json!({"session":"other"}),
        ];
        assert_eq!(
            family(&rows, "c"),
            BTreeSet::from(["root", "a", "b", "c"].map(String::from))
        );
        let cycle = vec![
            json!({"session":"a","parent":"b"}),
            json!({"session":"b","parent":"a"}),
        ];
        assert_eq!(family(&cycle, "a").len(), 2);
    }
}
