//! Explicit multi-file changes: validate all, retain recovery data, then apply.
//! Renames are atomic per file; this is deliberately not an atomic filesystem batch.
use crate::files;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{io::Read, path::Path};

#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Change {
    path: String,
    expected: Option<String>,
    text: String,
    #[serde(default)]
    validate: bool,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Plan {
    files: Vec<Change>,
}
#[derive(Serialize, Deserialize)]
struct Saved {
    workspace: std::path::PathBuf,
    changes: Vec<Change>,
    originals: Vec<Option<String>>,
    applied: usize,
    state: String,
}
fn request(c: &Change, preview: bool) -> Value {
    json!({"path":c.path,"expected":c.expected,"text":c.text,"validate":c.validate,"preview":preview})
}
fn id_valid(id: &str) -> bool {
    !id.is_empty() && id.len() < 100 && id.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'-')
}
fn persist(path: &Path, saved: &Saved) -> Result<(), String> {
    crate::auth::secure_write(path, &serde_json::to_vec(saved).map_err(|e| e.to_string())?)
}
// One writer owns a journal through application or recovery. A recovery
// command cannot roll back a batch that is still being applied.
fn lock_receipt(path: &Path) -> Result<std::fs::File, String> {
    let lock = path.with_extension("lock");
    let mut options = std::fs::OpenOptions::new();
    options.read(true).write(true).create(true).truncate(false);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
    }
    let file = options.open(lock).map_err(|e| e.to_string())?;
    if !file.metadata().map_err(|e| e.to_string())?.is_file() {
        return Err("Transaction lock must be a regular file".into());
    }
    fs2::FileExt::try_lock_exclusive(&file)
        .map_err(|_| "Transaction is already applying or recovering".to_string())?;
    Ok(file)
}
fn reverse(root: &Path, saved: &mut Saved, path: &Path) -> Result<(), String> {
    while saved.applied > 0 {
        let index = saved.applied - 1;
        let change = &saved.changes[index];
        let expected = crate::workspace::hash(change.text.as_bytes());
        let current = files::snapshot_text(root, &change.path)?;
        if current == saved.originals[index] {
            saved.applied = index;
            persist(path, saved)?;
            continue;
        }
        if let Some(original) = &saved.originals[index] {
            files::execute(
                root,
                "write",
                &json!({"path":change.path,"expected":expected,"text":original}),
            )?;
        } else {
            files::remove_created(root, &change.path, &expected)?;
        }
        saved.applied = index;
        persist(path, saved)?;
    }
    saved.state = "recovered".into();
    persist(path, saved)
}
pub fn execute(root: &Path, input: Value, apply: bool) -> Result<Value, String> {
    let plan: Plan = serde_json::from_value(input).map_err(|e| e.to_string())?;
    if plan.files.is_empty() || plan.files.len() > 32 {
        return Err("Transaction needs 1..32 files".into());
    }
    let root = root.canonicalize().map_err(|e| e.to_string())?;
    let mut seen = std::collections::BTreeSet::new();
    let mut originals = Vec::new();
    let mut previews = Vec::new();
    let mut total = 0usize;
    for change in &plan.files {
        // Native path validation and revision checks are shared with normal writes.
        let preview = files::execute(&root, "write", &request(change, true))?;
        let normalized = preview["path"].as_str().ok_or("Missing normalized path")?;
        let normalized = std::path::Path::new(normalized)
            .components()
            .map(|c| c.as_os_str().to_string_lossy())
            .collect::<Vec<_>>()
            .join("/");
        if !seen.insert(normalized) {
            return Err("Duplicate file in transaction".into());
        }
        let original = files::snapshot_text(&root, &change.path)?;
        if original
            .as_ref()
            .map(|s| crate::workspace::hash(s.as_bytes()))
            != change.expected
        {
            return Err("File changed during planning; reread all transaction inputs".into());
        }
        total = total
            .checked_add(change.text.len() + original.as_ref().map_or(0, String::len))
            .ok_or("Transaction size overflow")?;
        if total > 16 * 1024 * 1024 {
            return Err("Transaction recovery data exceeds 16 MiB".into());
        }
        originals.push(original);
        previews.push(preview);
    }
    if !apply {
        return Ok(
            json!({"status":"proposed","files":previews,"applied":false,"atomicity":"Per-file atomic replacement; batch recovery is revision-checked, not globally atomic."}),
        );
    }
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|e| e.to_string())?
        .as_nanos();
    let id = format!("{}-{stamp}", std::process::id());
    let dir = crate::config::data_dir().join("transactions");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let path = dir.join(format!("{id}.json"));
    let _lock = lock_receipt(&path)?;
    let mut saved = Saved {
        workspace: root.clone(),
        changes: plan.files,
        originals,
        applied: 0,
        state: "applying".into(),
    };
    persist(&path, &saved)?;
    for index in 0..saved.changes.len() {
        // Save the intended operation before applying: recovery also tolerates a
        // crash between the per-file rename and the next journal update.
        saved.applied = index + 1;
        persist(&path, &saved)?;
        if let Err(error) = files::execute(&root, "write", &request(&saved.changes[index], false)) {
            let recovery = reverse(&root, &mut saved, &path);
            return Err(format!(
                "Transaction stopped: {error}; recovery: {recovery:?}; receipt {id}. Inspect before gc transaction recover {id}"
            ));
        }
    }
    saved.state = "applied".into();
    persist(&path, &saved)?;
    Ok(
        json!({"status":"applied","receipt":id,"files":previews,"atomicity":"Per-file replacement. Concurrent external writers are not locked across the batch.","recover":format!("gc transaction recover {id}")}),
    )
}
pub fn command(args: &[String]) -> Result<(), String> {
    let args = args
        .iter()
        .filter(|a| a.as_str() != "--json")
        .collect::<Vec<_>>();
    let action = args.first().map(|a| a.as_str()).unwrap_or("--help");
    if action == "--help" {
        crate::cli::text(
            "File transactions",
            "gc transaction preview|apply < plan.json\ngc transaction recover RECEIPT\nPlan: {\"files\":[{\"path\":\"a.py\",\"expected\":\"SHA256\",\"text\":\"new text\",\"validate\":true}]}\nAll files validate before application. Private local receipts retain original content. Recovery refuses later edits; multi-file replacement is not atomic.",
        );
        return Ok(());
    }
    let value = if action == "recover" {
        if args.len() != 2 || !id_valid(args[1]) {
            return Err("gc transaction recover RECEIPT".into());
        }
        let path = crate::config::data_dir()
            .join("transactions")
            .join(format!("{}.json", args[1]));
        let _lock = lock_receipt(&path)?;
        let mut saved: Saved =
            serde_json::from_slice(&crate::requests::bounded(&path, 100 * 1024 * 1024)?)
                .map_err(|e| e.to_string())?;
        let root = std::env::current_dir()
            .map_err(|e| e.to_string())?
            .canonicalize()
            .map_err(|e| e.to_string())?;
        if saved.changes.is_empty()
            || saved.changes.len() > 32
            || saved.changes.iter().map(|c| c.text.len()).sum::<usize>()
                + saved
                    .originals
                    .iter()
                    .flatten()
                    .map(String::len)
                    .sum::<usize>()
                > 16 * 1024 * 1024
        {
            return Err("Recovery receipt exceeds transaction bounds".into());
        }
        if root != saved.workspace
            || saved.applied > saved.changes.len()
            || saved.originals.len() != saved.changes.len()
        {
            return Err("Recovery receipt workspace or bounds differ".into());
        }
        reverse(&root, &mut saved, &path)?;
        json!({"status":"recovered","receipt":args[1]})
    } else {
        if args.len() != 1 || !matches!(action, "preview" | "apply") {
            return Err("gc transaction preview|apply < plan.json".into());
        }
        let mut bytes = Vec::new();
        std::io::stdin()
            .take(20 * 1024 * 1024 + 1)
            .read_to_end(&mut bytes)
            .map_err(|e| e.to_string())?;
        if bytes.len() > 20 * 1024 * 1024 {
            return Err("Transaction request exceeds 20 MiB".into());
        }
        execute(
            &std::env::current_dir().map_err(|e| e.to_string())?,
            serde_json::from_slice(&bytes).map_err(|e| e.to_string())?,
            action == "apply",
        )?
    };
    if std::env::args().any(|a| a == "--json") {
        crate::emit_json(value);
    } else {
        crate::cli::report("File transaction", &value);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn receipt_lock_excludes_concurrent_recovery() {
        let dir = std::env::temp_dir().join(format!("gc-transaction-lock-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("receipt.json");
        let guard = lock_receipt(&path).unwrap();
        assert!(lock_receipt(&path).is_err());
        drop(guard);
        assert!(lock_receipt(&path).is_ok());
        let _ = std::fs::remove_dir_all(dir);
    }
}
