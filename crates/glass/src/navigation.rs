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
    let mut ids = BTreeSet::from([session.to_owned()]);
    // Walk ancestors first, then all descendants. Bound by metadata rows, not recursion.
    for _ in 0..all.len() {
        let before = ids.len();
        for r in all {
            if ids.contains(r["session"].as_str().unwrap_or(""))
                && let Some(parent) = r["parent"].as_str()
            {
                ids.insert(parent.into());
            }
        }
        if before == ids.len() {
            break;
        }
    }
    for _ in 0..all.len() {
        let before = ids.len();
        for r in all {
            if r["parent"].as_str().is_some_and(|p| ids.contains(p))
                && let Some(id) = r["session"].as_str()
            {
                ids.insert(id.into());
            }
        }
        if before == ids.len() {
            break;
        }
    }
    ids
}
pub fn view(
    keys: &crate::input::KeyReader,
    session: &Session,
    workspace: &Path,
    branches: bool,
) -> Option<String> {
    let value = inspect(workspace);
    let all = value["threads"].as_array()?;
    if branches && value["branch_families_complete"] == false {
        crate::modal::info(
            keys,
            "Bounded branch catalogue",
            "The most recent 512 indexed threads are shown. Older relatives may be omitted; gc sessions/search and explicit gc resume ID retain access.",
        );
    }
    let family = family(all, &session.id);
    let rows = all
        .iter()
        .filter(|r| !branches || family.contains(r["session"].as_str().unwrap_or("")))
        .collect::<Vec<_>>();
    let labels = rows
        .iter()
        .map(|r| {
            format!(
                "{} · {} · {} · {} · {} unread",
                r["title"].as_str().unwrap_or("Untitled"),
                r["workspace"].as_str().unwrap_or(""),
                r["model"].as_str().unwrap_or(""),
                r["last_status"].as_str().unwrap_or("unknown"),
                r["unread_events"]
            )
        })
        .collect::<Vec<_>>();
    let search = rows
        .iter()
        .zip(&labels)
        .map(|(r, l)| format!("{l} {} {}", r["session"], r["branch"]))
        .collect::<Vec<_>>();
    if rows.is_empty() {
        crate::modal::info(keys, "Conversations", "No indexed conversations yet.");
        return None;
    }
    loop {
        let i = crate::modal::choose_search(
            keys,
            if branches {
                "Conversation alternatives"
            } else {
                "Workspace conversations"
            },
            "Search project, branch, model, status or title · details load on selection",
            &labels,
            &search,
        )?;
        let row = rows[i];
        let id = row["session"].as_str()?;
        let actions = [
            "Inspect metadata",
            "Preview conversation",
            "Resume conversation",
            "Send background follow-up",
        ]
        .map(String::from);
        match crate::modal::choose(keys, "Conversation actions", &labels[i], &actions) {
            Some(0) => crate::structured::view(keys, "Conversation metadata", &row.to_string()),
            Some(1) => match Session::open_readonly(&config::data_dir(), id) {
                Ok(s) => {
                    let entries = s.history_entries();
                    let mut preview = String::from(
                        "Recent conversation · older entries remain in gc history SESSION --range START COUNT\n\n",
                    );
                    for entry in entries
                        .iter()
                        .rev()
                        .take(128)
                        .collect::<Vec<_>>()
                        .into_iter()
                        .rev()
                    {
                        if preview.len() >= 65536 {
                            break;
                        }
                        preview.push_str(&format!(
                            "{:?} · turn {}\n{}\n\n",
                            entry.role,
                            entry.turn,
                            glass_tui::text::clip(&entry.text, 2048)
                        ));
                    }
                    crate::modal::markdown(keys, "Conversation preview", &preview);
                    if let Err(e) = viewed(&s) {
                        crate::modal::info(keys, "Read cursor not saved", &e);
                    }
                }
                Err(e) => crate::modal::info(keys, "Preview unavailable", &e.to_string()),
            },
            Some(2) if row["current_workspace"] == true => return Some(id.into()),
            Some(2) => crate::modal::info(
                keys,
                "Other workspace",
                &format!(
                    "Open glass-core in {} and run gc resume {id}.",
                    row["workspace"].as_str().unwrap_or("")
                ),
            ),
            Some(3) => {
                if let Some(text) = crate::modal::field(
                    keys,
                    "Background follow-up",
                    "Saved in the target inbox; processed when the target is running and idle",
                    "",
                    false,
                ) {
                    match crate::inbox::send(id, &text, &session.id) {
                        Ok(()) => crate::modal::info(
                            keys,
                            "Follow-up saved",
                            "Inbox accepted the message; provider completion is separate.",
                        ),
                        Err(e) => crate::modal::info(keys, "Follow-up unavailable", &e),
                    }
                }
            }
            _ => {}
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
