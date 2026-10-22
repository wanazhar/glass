//! Workspace trust: consent for repository-controlled instruction text.
//!
//! Clone a repository and its `AGENTS.md` flows straight into the system
//! prompt — text the repository author controls, read before anything else.
//! For a single-user tool that is usually fine and often the point; the gate
//! exists so it is a decision rather than a surprise (pi separates "project
//! trust" from permissions for the same reason).
//!
//! Today the only workspace-controlled input is the instruction files;
//! extensions and skills are user-level. The gate covers what exists, keyed
//! by a content hash: changed instructions are a new decision, never the old
//! one silently reapplied.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::agents_md::Instruction;
use crate::auth;
use crate::config;

/// What a workspace's instructions were decided to be.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Decision {
    Allow,
    Deny,
}

/// What the store remembers about one workspace.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Entry {
    pub hash: String,
    pub decision: Decision,
}

#[derive(Debug, Default, serde::Serialize, serde::Deserialize)]
#[serde(default)]
struct Store {
    /// Canonical workspace path -> decision for one instruction hash.
    workspaces: BTreeMap<String, Entry>,
}

fn store_path() -> PathBuf {
    config::config_dir().join("trust.json")
}

fn load_from(path: &Path) -> Store {
    match std::fs::read(path) {
        Ok(bytes) => serde_json::from_slice(&bytes).unwrap_or_default(),
        Err(_) => Store::default(),
    }
}

fn save_to(path: &Path, store: &Store) -> Result<(), String> {
    auth::secure_write(
        path,
        &serde_json::to_vec_pretty(store).map_err(|e| e.to_string())?,
    )
}

/// Content hash of the loaded instructions: path, text, truncation marker.
///
/// FNV-1a: a consent check needs the hash to change when the content does,
/// not cryptographic strength, and avoiding a digest dependency keeps the
/// binary argument intact.
pub fn digest(instructions: &[Instruction]) -> String {
    const OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
    const PRIME: u64 = 0x0000_0100_0000_01b3;
    let mut hash = OFFSET;
    let mut eat = |bytes: &[u8]| {
        for b in bytes {
            hash ^= *b as u64;
            hash = hash.wrapping_mul(PRIME);
        }
    };
    for i in instructions {
        eat(i.path.to_string_lossy().as_bytes());
        eat(i.text.as_bytes());
        eat(&[i.truncated as u8]);
    }
    format!("{hash:016x}")
}

/// Canonical workspace key, so two spellings of one directory agree.
fn key(workspace: &Path) -> String {
    workspace
        .canonicalize()
        .unwrap_or_else(|_| workspace.to_path_buf())
        .to_string_lossy()
        .into_owned()
}

/// The stored decision for this workspace and exact instruction content.
pub fn remembered(workspace: &Path, digest: &str) -> Option<Decision> {
    remembered_in(&store_path(), workspace, digest)
}

pub fn remembered_in(path: &Path, workspace: &Path, digest: &str) -> Option<Decision> {
    let store = load_from(path);
    let entry = store.workspaces.get(&key(workspace))?;
    (entry.hash == digest).then_some(entry.decision)
}

/// Persist a decision for this workspace and content hash.
pub fn remember(workspace: &Path, digest: &str, decision: Decision) -> Result<(), String> {
    remember_in(&store_path(), workspace, digest, decision)
}

pub fn remember_in(
    path: &Path,
    workspace: &Path,
    digest: &str,
    decision: Decision,
) -> Result<(), String> {
    let mut store = load_from(path);
    store.workspaces.insert(
        key(workspace),
        Entry {
            hash: digest.to_string(),
            decision,
        },
    );
    save_to(path, &store)
}

/// Instructions allowed without asking.
///
/// Used by headless runs and by workflow turns that cannot prompt: `force`
/// is the `--trust` flag, `trust = "always"` skips the gate entirely, and
/// otherwise only a remembered allow for this exact content passes.
pub fn gated(cfg: &config::Config, instructions: &[Instruction], force: bool) -> Option<String> {
    gated_in(&store_path(), cfg, instructions, force)
}

/// `gated` against an explicit store, so tests never touch the real one.
pub fn gated_in(
    store: &Path,
    cfg: &config::Config,
    instructions: &[Instruction],
    force: bool,
) -> Option<String> {
    if instructions.is_empty() {
        return None;
    }
    let digest = digest(instructions);
    let allowed = force
        || cfg.trust == config::Trust::Always
        || remembered_in(store, &cfg.workspace, &digest) == Some(Decision::Allow);
    allowed.then(|| crate::agents_md::render_all(instructions)).flatten()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(tag: &str) -> PathBuf {
        static SEQ: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
        let n = SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let d = std::env::temp_dir().join(format!("gc-trust-{}-{tag}-{n}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn instruction(path: &str, text: &str) -> Instruction {
        Instruction {
            path: PathBuf::from(path),
            text: text.into(),
            truncated: false,
        }
    }

    #[test]
    fn digest_is_stable_and_content_sensitive() {
        let a = vec![instruction("/w/AGENTS.md", "use cargo")];
        let b = vec![instruction("/w/AGENTS.md", "use cargo")];
        let c = vec![instruction("/w/AGENTS.md", "use npm")];
        assert_eq!(digest(&a), digest(&b));
        assert_ne!(digest(&a), digest(&c));
        let truncated = vec![Instruction {
            truncated: true,
            ..instruction("/w/AGENTS.md", "use cargo")
        }];
        assert_ne!(digest(&a), digest(&truncated));
    }

    #[test]
    fn remember_round_trips_through_a_store_file() {
        let dir = scratch("store");
        let store = dir.join("trust.json");
        let ws = dir.join("project");
        std::fs::create_dir_all(&ws).unwrap();
        assert_eq!(remembered_in(&store, &ws, "hash1"), None);
        remember_in(&store, &ws, "hash1", Decision::Allow).unwrap();
        assert_eq!(remembered_in(&store, &ws, "hash1"), Some(Decision::Allow));
        assert_eq!(
            remembered_in(&store, &ws, "hash2"),
            None,
            "changed content is a new decision"
        );
        remember_in(&store, &ws, "hash2", Decision::Deny).unwrap();
        assert_eq!(remembered_in(&store, &ws, "hash2"), Some(Decision::Deny));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn gated_honours_config_force_and_the_store() {
        let dir = scratch("gated");
        let ws = dir.join("project");
        std::fs::create_dir_all(&ws).unwrap();
        let store = dir.join("trust.json");
        let instructions = vec![instruction("/w/AGENTS.md", "use cargo")];
        let mut cfg = config::Config {
            workspace: ws.clone(),
            ..config::Config::default()
        };

        assert!(
            gated_in(&store, &cfg, &instructions, false).is_none(),
            "untrusted"
        );
        assert!(
            gated_in(&store, &cfg, &instructions, true).is_some(),
            "--trust accepts for the run"
        );
        cfg.trust = config::Trust::Always;
        assert!(gated_in(&store, &cfg, &instructions, false).is_some());
        cfg.trust = config::Trust::Ask;

        let digest = digest(&instructions);
        remember_in(&store, &ws, &digest, Decision::Deny).unwrap();
        assert!(gated_in(&store, &cfg, &instructions, false).is_none(), "denied");
        remember_in(&store, &ws, &digest, Decision::Allow).unwrap();
        assert!(gated_in(&store, &cfg, &instructions, false).is_some(), "allowed");
        std::fs::remove_dir_all(&dir).ok();
    }
}
