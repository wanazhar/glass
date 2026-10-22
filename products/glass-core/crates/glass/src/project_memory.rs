//! Explicitly reviewed project facts, kept out of context unless enabled by the user.
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::{collections::BTreeMap, path::Path};
#[derive(Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Store {
    #[serde(default)]
    pub(crate) enabled: bool,
    #[serde(default)]
    pub(crate) facts: BTreeMap<String, Fact>,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Fact {
    pub(crate) text: String,
    pub(crate) source: String,
    pub(crate) reviewed_at: u64,
}
pub(crate) fn load(path: &Path) -> Result<Store, String> {
    match std::fs::symlink_metadata(path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Store::default()),
        Err(e) => return Err(e.to_string()),
        Ok(_) => {}
    }
    let store: Store = serde_json::from_slice(&crate::requests::bounded(path, 64 * 1024)?)
        .map_err(|e| format!("Invalid project memory: {e}"))?;
    validate(&store)?;
    Ok(store)
}
pub(crate) fn validate(store: &Store) -> Result<(), String> {
    if store.facts.len() > 128
        || store.facts.iter().any(|(key, f)| {
            key.is_empty() || key.len() > 128 || f.text.len() > 4096 || f.source.len() > 1024
        })
        || serde_json::to_vec_pretty(store)
            .map_err(|e| e.to_string())?
            .len()
            > 64 * 1024
    {
        return Err("Project memory exceeds bounds".into());
    }
    Ok(())
}
pub(crate) fn retrieved(store: &Store) -> Vec<serde_json::Value> {
    if !store.enabled {
        return Vec::new();
    }
    let mut facts = Vec::new();
    let mut bytes = 2;
    for (key, fact) in store.facts.iter().take(16) {
        let value = json!({"key":key,"fact":fact.text,"source":fact.source,"saved_unix":fact.reviewed_at,"freshness":"Review date is recorded; facts may become stale."});
        let size = value.to_string().replace('<', "\\u003c").len() + usize::from(!facts.is_empty());
        if bytes + size > 8192 {
            break;
        }
        bytes += size;
        facts.push(value);
    }
    facts
}
pub(crate) fn context_store(store: &Store) -> Result<Option<String>, String> {
    let facts = retrieved(store);
    Ok((!facts.is_empty()).then(|| format!("\nProject memory (explicitly enabled and saved reference facts; original instructions take precedence):\n{}", json!(facts).to_string().replace('<', "\\u003c"))))
}
pub fn command(args: &[String]) -> Result<(), String> {
    let help = "gc memory [list|enable|disable|set KEY TEXT SOURCE|forget KEY] [--json]\nMemory is disabled by default. set records an explicit review action; facts are never extracted automatically from conversations. Context retrieval is capped at 16 facts / 8 KiB. List retains every fact and its review date.";
    if args.iter().any(|a| a == "--help") {
        crate::cli::text("Project memory", help);
        return Ok(());
    }
    let root = std::env::current_dir().map_err(|e| e.to_string())?;
    let json_output = args.iter().any(|a| a == "--json");
    let args = args
        .iter()
        .filter(|a| a.as_str() != "--json")
        .map(String::as_str)
        .collect::<Vec<_>>();
    let value = crate::neuralnets::update_reviewed(&root, |store| {
        let mutate = match args.as_slice() {
            [] | ["list"] => false,
            ["enable"] => {
                store.enabled = true;
                true
            }
            ["disable"] => {
                store.enabled = false;
                true
            }
            ["set", key, text, source] => {
                if key.trim().is_empty()
                    || key.len() > 128
                    || text.trim().is_empty()
                    || text.len() > 4096
                    || source.trim().is_empty()
                    || source.len() > 1024
                {
                    return Err(
                        "Supply a key, fact and provenance within the documented bounds".into(),
                    );
                }
                if !store.facts.contains_key(*key) && store.facts.len() >= 128 {
                    return Err("Forget an old fact before adding more".into());
                }
                store.facts.insert(
                    (*key).into(),
                    Fact {
                        text: (*text).into(),
                        source: (*source).into(),
                        reviewed_at: std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .map_err(|e| e.to_string())?
                            .as_secs(),
                    },
                );
                true
            }
            ["forget", key] => {
                if store.facts.remove(*key).is_none() {
                    return Err("Unknown memory key".into());
                }
                true
            }
            _ => return Err(help.into()),
        };
        if mutate {
            let bytes = serde_json::to_vec_pretty(&store).map_err(|e| e.to_string())?;
            if bytes.len() > 64 * 1024 {
                return Err("Project memory exceeds 64 KiB; forget older facts".into());
            }
            // The unified owner writes both sections atomically under one lock.
        }
        Ok(mutate)
    })?;
    if json_output {
        crate::emit_json(value);
    } else {
        crate::cli::report("Project memory", &value);
    }
    Ok(())
}

#[cfg(test)]
mod retrieval_tests {
    use super::*;
    #[test]
    fn escaped_facts_respect_wire_budget() {
        let mut store = Store {
            enabled: true,
            ..Default::default()
        };
        for i in 0..16 {
            store.facts.insert(
                i.to_string(),
                Fact {
                    text: "<".repeat(1000),
                    source: "test".into(),
                    reviewed_at: 1,
                },
            );
        }
        let rows = retrieved(&store);
        assert_eq!(rows.len(), 1);
        assert!(json!(rows).to_string().replace('<', "\\u003c").len() <= 8192);
    }
}
