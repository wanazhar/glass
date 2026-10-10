//! Explicitly reviewed project facts, kept out of context unless enabled by the user.
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, path::{Path, PathBuf}};
use serde_json::json;
#[derive(Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Store { #[serde(default)] enabled: bool, #[serde(default)] facts: BTreeMap<String, Fact> }
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Fact { text: String, source: String, reviewed_at: u64 }
fn path(root: &Path) -> Result<PathBuf, String> {
    let root = root.canonicalize().map_err(|e| e.to_string())?;
    Ok(crate::config::data_dir().join("memory").join(format!("{}.json", crate::workspace::hash(root.as_os_str().as_encoded_bytes()))))
}
fn load(path: &Path) -> Result<Store, String> {
    if !path.exists() { return Ok(Store::default()); }
    let store: Store = serde_json::from_slice(&crate::requests::bounded(path, 64 * 1024)?).map_err(|e| format!("Invalid project memory: {e}"))?;
    if store.facts.len() > 128 || store.facts.iter().any(|(key, fact)| key.is_empty() || key.len()>128 || fact.text.len()>4096 || fact.source.len()>1024) { return Err("Project memory exceeds bounds".into()); }
    Ok(store)
}
pub fn context(root: &Path) -> Result<Option<String>, String> {
    if root.as_os_str().is_empty() { return Ok(None); }
    let store = load(&path(root)?)?;
    if !store.enabled { return Ok(None); }
    let mut facts = Vec::new();
    let mut bytes = 0;
    for (key, fact) in store.facts.iter().take(16) {
        let value = json!({"key":key,"fact":fact.text,"source":fact.source,"human_reviewed_unix":fact.reviewed_at,"freshness":"Review date is recorded; facts may become stale."});
        bytes += value.to_string().len();
        if bytes > 8192 { break; }
        facts.push(value);
    }
    Ok((!facts.is_empty()).then(|| format!("\nProject memory (user enabled, user reviewed reference facts; original instructions take precedence):\n{}", json!(facts))))
}
pub fn command(args: &[String]) -> Result<(), String> {
    let help = "gc memory [list|enable|disable|set KEY TEXT SOURCE|forget KEY] [--json]\nMemory is disabled by default. set records an explicit review action; facts are never extracted automatically from conversations. Context retrieval is capped at 16 facts / 8 KiB. List retains every fact and its review date.";
    if args.iter().any(|a| a == "--help") { crate::cli::text("Project memory", help); return Ok(()); }
    let root = std::env::current_dir().map_err(|e| e.to_string())?;
    let path = path(&root)?;
    let json_output = args.iter().any(|a| a == "--json");
    let args = args.iter().filter(|a| a.as_str() != "--json").map(String::as_str).collect::<Vec<_>>();
    std::fs::create_dir_all(path.parent().ok_or("Missing memory directory")?).map_err(|e| e.to_string())?;
    use std::os::unix::fs::OpenOptionsExt;
    let lock = std::fs::OpenOptions::new().read(true).write(true).create(true).mode(0o600).custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK).open(path.with_extension("lock")).map_err(|e| e.to_string())?;
    if !lock.metadata().map_err(|e| e.to_string())?.is_file() { return Err("Invalid memory lock".into()); }
    fs2::FileExt::try_lock_exclusive(&lock).map_err(|_| "Project memory is being updated; retry")?;
    let mut store = load(&path)?;
    let mutate = match args.as_slice() {
        [] | ["list"] => false,
        ["enable"] => { store.enabled = true; true },
        ["disable"] => { store.enabled = false; true },
        ["set", key, text, source] => {
            if key.trim().is_empty() || key.len()>128 || text.trim().is_empty() || text.len()>4096 || source.trim().is_empty() || source.len()>1024 { return Err("Supply a key, fact and provenance within the documented bounds".into()); }
            if !store.facts.contains_key(*key) && store.facts.len() >= 128 { return Err("Forget an old fact before adding more".into()); }
            store.facts.insert((*key).into(), Fact { text:(*text).into(),source:(*source).into(),reviewed_at:std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_err(|e| e.to_string())?.as_secs() });
            true
        },
        ["forget", key] => { if store.facts.remove(*key).is_none() { return Err("Unknown memory key".into()); } true },
        _ => return Err(help.into()),
    };
    if mutate {
        let bytes = serde_json::to_vec_pretty(&store).map_err(|e| e.to_string())?;
        if bytes.len()>64*1024 { return Err("Project memory exceeds 64 KiB; forget older facts".into()); }
        crate::auth::secure_write(&path, &bytes)?;
    }
    let value = serde_json::to_value(store).map_err(|e| e.to_string())?;
    if json_output { crate::emit_json(value); } else { crate::cli::report("Project memory", &value); }
    Ok(())
}
