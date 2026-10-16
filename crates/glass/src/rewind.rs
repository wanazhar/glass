//! Explicit conversation forks with optional guarded file rewind.
use crate::{
    checkpoint::Checkpoint,
    session::{Record, Session},
};
use serde_json::Value;
use std::path::Path;

pub fn turn_checkpoints(session: &Session) -> Vec<Value> {
    session
        .history_records()
        .iter()
        .filter_map(|record| match record {
            Record::Custom {
                kind,
                data: Some(data),
            } if kind == "turn/checkpoint" => serde_json::from_str(data).ok(),
            _ => None,
        })
        .collect()
}

pub fn review(keys: &crate::input::KeyReader, session: &Session) {
    let checkpoints = turn_checkpoints(session);
    let rows = checkpoints
        .iter()
        .rev()
        .map(|c| {
            format!(
                "Turn {} · {}",
                c["turn"],
                c["label"].as_str().unwrap_or("checkpoint")
            )
        })
        .collect::<Vec<_>>();
    if rows.is_empty() {
        crate::modal::info(
            keys,
            "Turn changes",
            "No turn checkpoint receipts yet. New Git-backed turns record boundaries automatically.",
        );
        return;
    }
    while let Some(index) = crate::modal::choose(
        keys,
        "Turn changes",
        "Observed interval changes; concurrent edits are not exclusive agent ownership",
        &rows,
    ) {
        let receipt = &checkpoints[checkpoints.len() - 1 - index];
        let label = receipt["label"].as_str().unwrap_or("");
        let dir = crate::config::data_dir()
            .join("checkpoints")
            .join(&session.id);
        match Checkpoint::load(&dir, label).and_then(|c| c.diff()) {
            Ok(diff) => crate::modal::diff(keys, &rows[index], &diff),
            Err(error) => crate::modal::info(keys, "Diff unavailable", &error),
        }
    }
}

/// Hold inactive same-project session writer locks while restoring. Unknown locks
/// fail closed; external editors remain outside this coordination protocol.
fn guard_other_sessions(session: &Session, workspace: &Path) -> Result<Vec<std::fs::File>, String> {
    let mut guards = Vec::new();
    use std::os::unix::fs::OpenOptionsExt;
    for thread in crate::resume_picker::index(&crate::config::data_dir(), workspace) {
        if thread.id == session.id {
            continue;
        }
        let path = crate::config::data_dir().join(format!("{}.writer.lock", thread.id));
        let file = match std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
            .open(path)
        {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
            Err(e) => return Err(format!("Cannot inspect session ownership: {e}")),
            Ok(file) => file,
        };
        if !file.metadata().map_err(|e| e.to_string())?.is_file() {
            return Err("Invalid session lock".into());
        }
        fs2::FileExt::try_lock_exclusive(&file).map_err(|_|"Another session in this workspace is active; use keep-files rewind or a separate worktree")?;
        guards.push(file);
    }
    if crate::team::get(&session.id).is_some_and(|hub| {
        hub.snapshot()
            .iter()
            .any(|info| info.status == "working" && !info.read_only)
    }) {
        return Err(
            "A writing teammate is running; use keep-files rewind or wait for it to finish".into(),
        );
    }
    Ok(guards)
}

pub fn choose(
    keys: &crate::input::KeyReader,
    session: &Session,
    workspace: &Path,
) -> Result<Option<(Session, usize)>, String> {
    let turns = session.raw_turns().map_err(|e| e.to_string())?;
    if turns == 0 {
        return Err("No conversation turns to rewind".into());
    }
    let rows = (1..=turns)
        .rev()
        .map(|t| format!("After turn {t}"))
        .collect::<Vec<_>>();
    let Some(index) = crate::modal::choose(
        keys,
        "Rewind conversation",
        "Original conversation is retained as an alternative",
        &rows,
    ) else {
        return Ok(None);
    };
    let through = turns - index;
    let modes = ["Keep workspace files", "Rewind conversation and files"].map(String::from);
    let Some(mode) = crate::modal::choose(
        keys,
        "Rewind mode",
        "Keep-files is safe for shared workspaces. File rewind needs recorded checkpoints.",
        &modes,
    ) else {
        return Ok(None);
    };
    let dir = crate::config::data_dir()
        .join("checkpoints")
        .join(&session.id);
    let mut _guards = Vec::new();
    let restore = if mode == 1 {
        _guards = guard_other_sessions(session, workspace)?;
        let receipts = turn_checkpoints(session);
        let target = receipts
            .iter()
            .rev()
            .find(|r| r["turn"].as_u64() == Some(through as u64))
            .ok_or("No checkpoint receipt for that turn; keep files instead")?;
        let latest = receipts.last().ok_or("No latest checkpoint receipt")?;
        let target = Checkpoint::load(
            &dir,
            target["label"]
                .as_str()
                .ok_or("Invalid checkpoint receipt")?,
        )?;
        let latest = Checkpoint::load(
            &dir,
            latest["label"]
                .as_str()
                .ok_or("Invalid checkpoint receipt")?,
        )?;
        let expected_workspace = workspace.canonicalize().map_err(|e| e.to_string())?;
        if target.workspace.canonicalize().map_err(|e| e.to_string())? != expected_workspace
            || latest.workspace.canonicalize().map_err(|e| e.to_string())? != expected_workspace
        {
            return Err("Checkpoint belongs to a different workspace".into());
        }
        let restore = Checkpoint {
            before: target.after.ok_or("Target checkpoint is unfinished")?,
            after: latest.after,
            workspace: target.workspace,
            label: format!("rewind-{through}"),
        };
        let preview = restore.diff()?;
        crate::modal::diff(keys, "Changes to reverse", &preview);
        if crate::modal::choose(
            keys,
            "Apply file rewind?",
            "Later conflicting edits refuse restoration. A recovery checkpoint is saved first. External editors are not locked.",
            &["Cancel".into(), "Rewind files and fork conversation".into()],
        ) != Some(1)
        {
            return Ok(None);
        }
        Some(restore)
    } else {
        None
    };
    let mut fork = session
        .branch(&crate::config::data_dir(), through)
        .map_err(|e| e.to_string())?;
    crate::polish::save(&fork, &crate::polish::metadata(session))?;
    if let Some(checkpoint) = restore {
        checkpoint.undo(&dir).map_err(|e| {
            format!(
                "File rewind failed: {e}. Retained fork: {}. Recovery checkpoint: undo-backup-{}",
                fork.id, checkpoint.label
            )
        })?;
    }
    fork.append_checked(&[Record::Meta(format!(
        "Rewound after turn {through}; {}. Original session {} retained.",
        if mode == 0 {
            "files kept"
        } else {
            "files restored"
        },
        session.id
    ))])
    .map_err(|e| e.to_string())?;
    Ok(Some((fork, through)))
}

/// Guarded file rewind shared with the browser; preview and apply use a hash.
pub fn portable(
    session: &Session,
    workspace: &Path,
    through: usize,
    files: bool,
    expected: Option<&str>,
) -> Result<Value, String> {
    let turns = session.raw_turns().map_err(|e| e.to_string())?;
    if through == 0 || through > turns {
        return Err("Choose an existing turn".into());
    }
    let dir = crate::config::data_dir()
        .join("checkpoints")
        .join(&session.id);
    let _guards = if files {
        guard_other_sessions(session, workspace)?
    } else {
        Vec::new()
    };
    let restore = if files {
        let receipts = turn_checkpoints(session);
        let target = receipts
            .iter()
            .rev()
            .find(|r| r["turn"].as_u64() == Some(through as u64))
            .ok_or("No checkpoint for that turn; choose keep files")?;
        let latest = receipts.last().ok_or("No latest checkpoint")?;
        let target = Checkpoint::load(&dir, target["label"].as_str().ok_or("Invalid checkpoint")?)?;
        let latest = Checkpoint::load(&dir, latest["label"].as_str().ok_or("Invalid checkpoint")?)?;
        let root = workspace.canonicalize().map_err(|e| e.to_string())?;
        if target.workspace.canonicalize().map_err(|e| e.to_string())? != root
            || latest.workspace.canonicalize().map_err(|e| e.to_string())? != root
        {
            return Err("Checkpoint belongs to another workspace".into());
        }
        Some(Checkpoint {
            before: target.after.ok_or("Checkpoint unfinished")?,
            after: latest.after,
            workspace: target.workspace,
            label: format!("rewind-{through}"),
        })
    } else {
        None
    };
    if let Some(c) = &restore {
        let diff = c.diff()?;
        let hash = crate::workspace::hash(diff.as_bytes());
        if expected.is_none() {
            return Ok(
                serde_json::json!({"preview":diff,"expected":hash,"through":through,"files":true}),
            );
        }
        if expected != Some(hash.as_str()) {
            return Err("Workspace changed since preview; review the rewind again".into());
        }
    }
    let mut fork = session
        .branch(&crate::config::data_dir(), through)
        .map_err(|e| e.to_string())?;
    crate::polish::save(&fork, &crate::polish::metadata(session))?;
    if let Some(c) = restore {
        c.undo(&dir)
            .map_err(|e| format!("Rewind failed: {e}; retained fork {}", fork.id))?;
    }
    crate::navigation::lineage(&fork, &session.id, through)?;
    fork.append_checked(&[Record::Meta(format!(
        "Rewound after turn {through}; files {}. Original {} retained.",
        if files { "restored" } else { "kept" },
        session.id
    ))])
    .map_err(|e| e.to_string())?;
    Ok(serde_json::json!({"open_thread":fork.id,"through":through,"files_restored":files}))
}
