//! Agent Plugins 1.0: portable plugins carrying skills and MCP servers.
//!
//! The spec (agent-plugins.org, pinned from `agent-plugins-spec` 1.0.0) keeps
//! the portable surface deliberately small: a closed `plugin.json` at the
//! plugin root, skills as immediate child directories of `skills/` containing
//! `SKILL.md`, and MCP servers in a root `mcp.json`. Everything else is
//! client-specific under reverse-domain namespaces and is ignored here.
//!
//! Two rules matter for safety and are implemented verbatim: plugin-relative
//! paths must resolve inside the plugin root (symlinks included), and
//! component failures are non-fatal — a broken skill or server never takes
//! the session down. Plugins are **user-level only** (installed under the
//! config dir); a repository cannot add one, so this stays outside the
//! trust-gate surface.

use ring::digest::{SHA256, digest};
use std::path::{Path, PathBuf};

use crate::auth;
use crate::config;
use crate::skills::Skill;

/// The only manifest schema we recognise.
pub const PLUGIN_SCHEMA: &str = "https://agent-plugins.org/schemas/1.0.0/plugin.schema.json";

/// Manifest fields the spec permits; anything else is reported and ignored.
const ALLOWED_FIELDS: &[&str] = &[
    "$schema",
    "name",
    "version",
    "description",
    "author",
    "homepage",
    "repository",
    "license",
    "keywords",
    "extensions",
];

/// Where installed plugins live.
pub fn plugins_dir() -> PathBuf {
    config::config_dir().join("plugins")
}

/// The persistent per-plugin data directory (`${PLUGIN_DATA}`).
pub fn plugin_data_dir(name: &str) -> PathBuf {
    plugins_dir().join("data").join(name)
}

/// One installed, validated plugin.
pub struct Plugin {
    pub name: String,
    pub version: String,
    pub dir: PathBuf,
    pub skills: Vec<Skill>,
    /// Validated `mcp.json` path; parsing and expansion live in mcp-bridge.
    pub mcp: Option<PathBuf>,
}

/// The spec's plugin-name rules: 1–64 chars of `[a-z0-9.-]`, alphanumeric at
/// both ends, no consecutive `--` or `..`.
pub fn valid_name(name: &str) -> bool {
    let bytes = name.as_bytes();
    if bytes.is_empty() || bytes.len() > 64 {
        return false;
    }
    if !bytes[0].is_ascii_alphanumeric() || !bytes[bytes.len() - 1].is_ascii_alphanumeric() {
        return false;
    }
    name.chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '.')
        && !name.contains("--")
        && !name.contains("..")
}

/// Load one plugin directory. `None` means "not a plugin" or "rejected";
/// `report` collects the spec-mandated diagnostics either way.
pub fn load_one(dir: &Path, report: &mut Vec<String>) -> Option<Plugin> {
    if dir.parent() == Some(plugins_dir().as_path()) {
        let lock = match read_lock() {
            Ok(lock) => lock,
            Err(error) => {
                report.push(format!(
                    "plugin {}: integrity receipt is invalid: {error}",
                    dir.display()
                ));
                return None;
            }
        };
        let label = dir.file_name().unwrap_or_default().to_string_lossy();
        if let Some(entry) = lock.plugins.get(label.as_ref()) {
            let current = match content_hash(dir) {
                Ok(hash) => hash,
                Err(error) => {
                    report.push(format!(
                        "plugin {label}: integrity verification failed: {error}"
                    ));
                    return None;
                }
            };
            if current != entry.hash {
                report.push(format!(
                    "plugin {label}: integrity verification failed; content changed since install"
                ));
                return None;
            }
        } else {
            report.push(format!(
                "plugin {label}: legacy unmanaged install; integrity unverified"
            ));
        }
    }
    let manifest_path = dir.join("plugin.json");
    if std::fs::metadata(&manifest_path)
        .map(|m| m.len() > 262_144)
        .unwrap_or(false)
    {
        report.push(format!(
            "plugin {}: plugin.json exceeds size limit",
            dir.display()
        ));
        return None;
    }
    let text = std::fs::read_to_string(&manifest_path).ok()?;
    let value: serde_json::Value = match serde_json::from_str(&text) {
        Ok(v) => v,
        Err(e) => {
            report.push(format!(
                "plugin {}: plugin.json is not valid JSON: {e}",
                dir.display()
            ));
            return None;
        }
    };
    let obj = value.as_object()?;
    let label = dir
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| dir.display().to_string());

    // Unknown top-level fields are non-fatal, per the spec's closed schema.
    for key in obj.keys() {
        if !ALLOWED_FIELDS.contains(&key.as_str()) {
            report.push(format!(
                "plugin {label}: unknown manifest field `{key}` ignored"
            ));
        }
    }

    let schema = obj.get("$schema").and_then(|v| v.as_str()).unwrap_or("");
    if schema != PLUGIN_SCHEMA {
        report.push(format!(
            "plugin {label}: unsupported spec `{schema}`; rejected"
        ));
        return None;
    }
    let name = match obj.get("name").and_then(|v| v.as_str()) {
        Some(n) if valid_name(n) => n.to_string(),
        Some(n) => {
            report.push(format!(
                "plugin {label}: invalid plugin name `{n}`; rejected"
            ));
            return None;
        }
        None => {
            report.push(format!("plugin {label}: missing required `name`; rejected"));
            return None;
        }
    };
    let version = obj
        .get("version")
        .and_then(|v| v.as_str())
        .unwrap_or("0")
        .to_string();

    // Containment checks resolve against the plugin root, symlinks included.
    let Ok(root) = dir.canonicalize() else {
        report.push(format!("plugin {name}: cannot resolve {}", dir.display()));
        return None;
    };

    // Skills: immediate child directories of skills/ holding SKILL.md.
    let mut skills = Vec::new();
    if let Ok(entries) = std::fs::read_dir(root.join("skills")) {
        for entry in entries.flatten() {
            let skill_md = entry.path().join("SKILL.md");
            if !skill_md.is_file() {
                continue;
            }
            let Ok(resolved) = skill_md.canonicalize() else {
                continue;
            };
            if !resolved.starts_with(&root) {
                report.push(format!(
                    "plugin {name}: skill {} escapes the plugin root; skipped",
                    resolved.display()
                ));
                continue;
            }
            let Ok(body) = std::fs::read_to_string(&resolved) else {
                continue;
            };
            match crate::skills::parse(&body, resolved) {
                Ok(skill) => skills.push(skill),
                Err(e) => report.push(format!("plugin {name}: invalid skill skipped: {e}")),
            }
        }
    }
    skills.sort_by(|a, b| a.name.cmp(&b.name));

    // MCP configuration: root mcp.json, validated for containment only; the
    // bridge owns schema validation and placeholder expansion.
    let mut mcp = None;
    let mcp_path = root.join("mcp.json");
    if mcp_path.is_file() {
        match mcp_path.canonicalize() {
            Ok(resolved) if resolved.starts_with(&root) => mcp = Some(resolved),
            _ => report.push(format!(
                "plugin {name}: mcp.json escapes the plugin root; MCP disabled for it"
            )),
        }
    }

    Some(Plugin {
        name,
        version,
        dir: root,
        skills,
        mcp,
    })
}

/// Every installed plugin, sorted by name. Failures are diagnostics, never
/// fatal: one bad plugin directory cannot stop the session starting.
pub fn load_all(report: &mut Vec<String>) -> Vec<Plugin> {
    let dir = plugins_dir();
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return Vec::new();
    };
    let mut out: Vec<Plugin> = entries
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().map(|t| t.is_dir()).unwrap_or(false))
        .filter(|e| e.file_name() != "data")
        .filter_map(|e| load_one(&e.path(), report))
        .collect();
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out
}

/// Content and relative path hash. Symlinks are rejected rather than followed.
fn content_hash(root: &Path) -> Result<String, String> {
    let canonical = root.canonicalize().map_err(|e| e.to_string())?;
    let mut entries = Vec::new();
    let mut stack = vec![canonical.clone()];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir).map_err(|e| e.to_string())? {
            let entry = entry.map_err(|e| e.to_string())?;
            let path = entry.path();
            let kind = entry.file_type().map_err(|e| e.to_string())?;
            if kind.is_symlink() {
                return Err(format!("symlink is not allowed: {}", path.display()));
            }
            if kind.is_dir() {
                stack.push(path);
            } else if kind.is_file() {
                let rel = path
                    .strip_prefix(&canonical)
                    .map_err(|e| e.to_string())?
                    .to_string_lossy()
                    .replace('\\', "/");
                let bytes = std::fs::read(&path).map_err(|e| e.to_string())?;
                entries.push((rel, bytes));
            }
        }
    }
    entries.sort_by(|a, b| a.0.cmp(&b.0));
    let mut data = Vec::new();
    for (path, bytes) in entries {
        data.extend_from_slice(&(path.len() as u64).to_le_bytes());
        data.extend_from_slice(path.as_bytes());
        data.extend_from_slice(&(bytes.len() as u64).to_le_bytes());
        data.extend_from_slice(&bytes);
    }
    Ok(hex(digest(&SHA256, &data).as_ref()))
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Kept for the existing list drift field; now cryptographic and content based.
pub fn tree_hash(root: &Path) -> String {
    content_hash(root).unwrap_or_default()
}

#[derive(Default, serde::Serialize, serde::Deserialize)]
#[serde(default)]
struct Lock {
    /// plugin name -> where it came from and the hash recorded at install.
    plugins: std::collections::BTreeMap<String, LockEntry>,
}

#[derive(serde::Serialize, serde::Deserialize)]
struct LockEntry {
    source: String,
    hash: String,
    #[serde(default)]
    version: String,
    #[serde(default)]
    revision: Option<String>,
    #[serde(default)]
    capabilities: serde_json::Value,
}

fn lock_path() -> PathBuf {
    plugins_dir().join("lock.json")
}

fn load_lock() -> Lock {
    read_lock().unwrap_or_default()
}

fn read_lock() -> Result<Lock, String> {
    match std::fs::read(lock_path()) {
        Ok(bytes) => serde_json::from_slice(&bytes).map_err(|e| e.to_string()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Lock::default()),
        Err(e) => Err(e.to_string()),
    }
}

fn save_lock(lock: &Lock) -> Result<(), String> {
    auth::secure_write(
        &lock_path(),
        &serde_json::to_vec_pretty(lock).map_err(|e| e.to_string())?,
    )
}

fn git_revision(path: &Path) -> Option<String> {
    let out = std::process::Command::new("git")
        .args(["-C"])
        .arg(path)
        .args(["rev-parse", "HEAD"])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let rev = String::from_utf8_lossy(&out.stdout).trim().to_string();
    (rev.len() == 40 && rev.bytes().all(|b| b.is_ascii_hexdigit())).then_some(rev)
}

fn capabilities(dir: &Path) -> Result<serde_json::Value, String> {
    let bytes = std::fs::read(dir.join("plugin.json")).map_err(|e| e.to_string())?;
    if bytes.len() > 262_144 {
        return Err("plugin.json exceeds size limit".into());
    }
    let manifest: serde_json::Value = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
    let value = manifest
        .pointer("/extensions/org.glass-core/capabilities")
        .cloned()
        .unwrap_or_else(|| serde_json::json!({}));
    if !value.is_object() {
        return Err("capabilities must be an object".into());
    }
    for key in value.as_object().unwrap().keys() {
        if !["filesystem", "network", "subprocess"].contains(&key.as_str()) {
            return Err(format!("unknown capability `{key}`"));
        }
    }
    let encoded = serde_json::to_vec(&value).map_err(|e| e.to_string())?;
    if encoded.len() > 16_384 {
        return Err("capabilities exceed size limit".into());
    }
    Ok(value)
}

/// `gc plugin …` entry point.
pub fn command(args: &[String]) -> Result<(), String> {
    match args.first().map(String::as_str) {
        Some("scaffold" | "validate" | "trace") => crate::plugin_dev::command(args),
        Some("list") => {
            let json = args.iter().any(|a| a == "--json");
            let mut report = Vec::new();
            let installed = load_all(&mut report);
            let lock = load_lock();
            if json {
                let plugins: Vec<serde_json::Value> = installed
                    .iter()
                    .map(|p| {
                        let drift = lock
                            .plugins
                            .get(&p.name)
                            .map(|e| e.hash != tree_hash(&p.dir))
                            .unwrap_or(false);
                        serde_json::json!({
                            "name": p.name, "version": p.version, "path": p.dir,
                            "skills": p.skills.iter().map(|s| &s.name).collect::<Vec<_>>(),
                            "mcp": p.mcp.is_some(), "drift": drift,
                        })
                    })
                    .collect();
                crate::emit_json(serde_json::json!({"plugins": plugins}));
            } else if installed.is_empty() {
                crate::cli_println!("no plugins installed in {}", plugins_dir().display());
            } else {
                for p in &installed {
                    let drift = lock
                        .plugins
                        .get(&p.name)
                        .map(|e| {
                            if e.hash != tree_hash(&p.dir) {
                                " · modified since install"
                            } else {
                                ""
                            }
                        })
                        .unwrap_or("");
                    crate::cli_println!(
                        "{} v{} · {} skills · mcp: {}{drift}\n  {}",
                        p.name,
                        p.version,
                        p.skills.len(),
                        if p.mcp.is_some() { "yes" } else { "no" },
                        p.dir.display()
                    );
                }
            }
            for line in report {
                eprintln!("glass-core: {line}");
            }
            Ok(())
        }
        Some("add") => {
            let source = args
                .get(1)
                .ok_or("usage: glass-core plugin add <path|git-url>")?;
            add(source)
        }
        Some("remove") => {
            let name = args
                .get(1)
                .ok_or("usage: glass-core plugin remove <name>")?;
            remove(name)
        }
        Some("verify") => verify(),
        Some("update") => {
            let name = args
                .get(1)
                .ok_or("usage: glass-core plugin update <name> [--accept-permissions]")?;
            update(name, args.iter().any(|a| a == "--accept-permissions"))
        }
        _ => Err("usage: glass-core plugin list|add|remove|verify|update".into()),
    }
}

fn add(source: &str) -> Result<(), String> {
    let plugins = plugins_dir();
    std::fs::create_dir_all(&plugins).map_err(|e| e.to_string())?;
    // Stage first, then move into place under the manifest's name: an
    // interrupted install never leaves a half-written plugin visible.
    let staging = plugins.join(format!(".staging-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&staging);
    let is_remote = source.contains("://") || source.ends_with(".git");
    if is_remote {
        let status = std::process::Command::new("git")
            .args(["clone", "--depth", "1", "--quiet", source])
            .arg(&staging)
            .status()
            .map_err(|e| format!("git clone: {e}"))?;
        if !status.success() {
            let _ = std::fs::remove_dir_all(&staging);
            return Err(format!("git clone failed for {source}"));
        }
    } else {
        copy_dir(Path::new(source), &staging)?;
    }
    let mut report = Vec::new();
    let plugin = load_one(&staging, &mut report);
    for line in &report {
        eprintln!("glass-core: {line}");
    }
    let Some(plugin) = plugin else {
        let _ = std::fs::remove_dir_all(&staging);
        return Err(format!("{source} is not a valid Agent Plugin"));
    };
    let dest = plugins.join(&plugin.name);
    if dest.exists() {
        let _ = std::fs::remove_dir_all(&staging);
        return Err(format!(
            "plugin `{}` is already installed; remove it first",
            plugin.name
        ));
    }
    std::fs::rename(&staging, &dest).map_err(|e| e.to_string())?;
    // Reload from the final location so the hash covers what is installed.
    let mut report = Vec::new();
    let installed = load_one(&dest, &mut report).ok_or("install verification failed")?;
    let mut lock = load_lock();
    lock.plugins.insert(
        installed.name.clone(),
        LockEntry {
            source: source.to_string(),
            hash: tree_hash(&installed.dir),
            version: installed.version.clone(),
            revision: git_revision(&installed.dir),
            capabilities: capabilities(&installed.dir)?,
        },
    );
    save_lock(&lock)?;
    crate::cli_println!(
        "installed plugin `{}` ({} skills, mcp: {})",
        installed.name,
        installed.skills.len(),
        if installed.mcp.is_some() { "yes" } else { "no" }
    );
    Ok(())
}

fn verify() -> Result<(), String> {
    let mut lock = load_lock();
    let mut names: Vec<_> = lock.plugins.keys().cloned().collect();
    names.sort();
    if names.is_empty() {
        crate::cli_println!("no managed plugins to verify");
        return Ok(());
    }
    let mut failed = false;
    for name in names {
        let dir = plugins_dir().join(&name);
        let actual = content_hash(&dir);
        let entry = lock.plugins.get_mut(&name).unwrap();
        match actual {
            Ok(hash) if hash == entry.hash => crate::cli_println!(
                "{name}: verified v{} ({})",
                entry.version,
                entry.revision.as_deref().unwrap_or("no git revision")
            ),
            Ok(_) => {
                crate::cli_println!("{name}: FAILED content differs from receipt");
                failed = true;
            }
            Err(e) => {
                crate::cli_println!("{name}: FAILED {e}");
                failed = true;
            }
        }
    }
    if failed {
        Err("one or more plugin integrity checks failed".into())
    } else {
        Ok(())
    }
}

fn update(name: &str, accept_permissions: bool) -> Result<(), String> {
    if !valid_name(name) {
        return Err("invalid plugin name".into());
    }
    let plugins = plugins_dir();
    let dest = plugins.join(name);
    let mut lock = load_lock();
    let old = lock
        .plugins
        .get(name)
        .ok_or("plugin is unmanaged; update is unavailable")?;
    let source = old.source.clone();
    let stage = plugins.join(format!(".staging-update-{}-{}", std::process::id(), name));
    let _ = std::fs::remove_dir_all(&stage);
    if source.contains("://") || source.ends_with(".git") {
        let status = std::process::Command::new("git")
            .args(["clone", "--depth", "1", "--quiet", &source])
            .arg(&stage)
            .status()
            .map_err(|e| e.to_string())?;
        if !status.success() {
            let _ = std::fs::remove_dir_all(&stage);
            return Err(format!("git clone failed for {source}"));
        }
    } else {
        copy_dir(Path::new(&source), &stage)?;
    }
    let mut report = Vec::new();
    let candidate = load_one(&stage, &mut report).ok_or_else(|| {
        let _ = std::fs::remove_dir_all(&stage);
        "updated source is not a valid plugin".to_string()
    })?;
    if candidate.name != name {
        let _ = std::fs::remove_dir_all(&stage);
        return Err("updated source plugin name changed".into());
    }
    let old_caps = old.capabilities.clone();
    let new_caps = capabilities(&stage)?;
    if old_caps != new_caps {
        crate::cli_println!(
            "capabilities changed for {name}:\n  old: {}\n  new: {}",
            old_caps,
            new_caps
        );
        if !accept_permissions {
            let _ = std::fs::remove_dir_all(&stage);
            return Err("capability change requires --accept-permissions".into());
        }
    }
    // Keep user data out of the replacement. A backup permits rollback if rename fails.
    let backup = plugins.join(format!(".backup-update-{}-{}", std::process::id(), name));
    let _ = std::fs::remove_dir_all(&backup);
    replace_staged(&stage, &dest, &backup)?;
    let receipt = LockEntry {
        source,
        hash: content_hash(&dest)?,
        version: candidate.version,
        revision: git_revision(&dest),
        capabilities: new_caps,
    };
    lock.plugins.insert(name.to_string(), receipt);
    if let Err(e) = save_lock(&lock) {
        let _ = std::fs::remove_dir_all(&dest);
        let _ = std::fs::rename(&backup, &dest);
        return Err(format!(
            "could not save plugin receipt; previous plugin restored: {e}"
        ));
    }
    std::fs::remove_dir_all(&backup).map_err(|e| e.to_string())?;
    crate::cli_println!("updated plugin `{name}`");
    Ok(())
}

fn replace_staged(stage: &Path, dest: &Path, backup: &Path) -> Result<(), String> {
    std::fs::rename(dest, backup).map_err(|e| e.to_string())?;
    if let Err(error) = std::fs::rename(stage, dest) {
        let restored = std::fs::rename(backup, dest);
        return Err(format!(
            "atomic plugin replacement failed: {error}{}",
            restored
                .err()
                .map(|e| format!("; rollback failed: {e}"))
                .unwrap_or_default()
        ));
    }
    Ok(())
}

fn remove(name: &str) -> Result<(), String> {
    if !valid_name(name) {
        return Err("invalid plugin name".into());
    }
    let dir = plugins_dir().join(name);
    if !dir.is_dir() {
        return Err(format!("no plugin named `{name}`"));
    }
    std::fs::remove_dir_all(&dir).map_err(|e| e.to_string())?;
    let mut lock = load_lock();
    lock.plugins.remove(name);
    save_lock(&lock)?;
    let data = plugin_data_dir(name);
    crate::cli_println!("removed plugin `{name}`");
    if data.is_dir() {
        crate::cli_println!(
            "note: its data directory is kept at {} (delete it when truly done)",
            data.display()
        );
    }
    Ok(())
}

/// Recursive copy that refuses to follow symlinks out of the source tree.
fn copy_dir(from: &Path, to: &Path) -> Result<(), String> {
    let root = from.canonicalize().map_err(|e| e.to_string())?;
    std::fs::create_dir_all(to).map_err(|e| e.to_string())?;
    for entry in std::fs::read_dir(&root)
        .map_err(|e| e.to_string())?
        .flatten()
    {
        let path = entry.path();
        let meta = entry.file_type().map_err(|e| e.to_string())?;
        let target = to.join(entry.file_name());
        if meta.is_symlink() {
            return Err(format!(
                "symlink is not allowed in plugin source: {}",
                path.display()
            ));
        }
        if meta.is_dir() {
            copy_dir(&path, &target)?;
        } else if meta.is_file() {
            std::fs::copy(&path, &target).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(tag: &str) -> PathBuf {
        static SEQ: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
        let n = SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let d = std::env::temp_dir().join(format!("gc-plugin-{}-{tag}-{n}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn write_plugin(root: &Path) -> PathBuf {
        let dir = root.join("demo");
        std::fs::create_dir_all(dir.join("skills/greet")).unwrap();
        std::fs::write(
            dir.join("plugin.json"),
            serde_json::json!({
                "$schema": PLUGIN_SCHEMA, "name": "demo", "version": "1.2.0",
                "description": "demo plugin"
            })
            .to_string(),
        )
        .unwrap();
        std::fs::write(
            dir.join("skills/greet/SKILL.md"),
            "---\nname: greet\ndescription: Say hello.\n---\n\nHello!",
        )
        .unwrap();
        dir
    }

    #[test]
    fn name_rules_follow_the_spec() {
        for good in ["a", "my-plugin", "acme.tools", "lint3r"] {
            assert!(valid_name(good), "{good}");
        }
        for bad in [
            "",
            "My-Plugin",
            "-start",
            "end-",
            "has--double",
            "too..many",
            "s p a c e",
        ] {
            assert!(!valid_name(bad), "{bad}");
        }
    }

    #[test]
    fn a_valid_plugin_loads_skills_and_is_explicit_about_mcp() {
        let root = scratch("load");
        let dir = write_plugin(&root);
        let mut report = Vec::new();
        let plugin = load_one(&dir, &mut report).expect("valid plugin");
        assert_eq!(plugin.name, "demo");
        assert_eq!(plugin.version, "1.2.0");
        assert_eq!(plugin.skills.len(), 1);
        assert_eq!(plugin.skills[0].name, "greet");
        assert!(plugin.skills[0].description.contains("Say hello"));
        assert!(plugin.mcp.is_none(), "no mcp.json means no MCP component");
        assert!(report.is_empty(), "{report:?}");
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn manifest_violations_reject_but_unknown_fields_only_warn() {
        let root = scratch("reject");
        let dir = write_plugin(&root);
        // Unknown top-level field: reported, still loads.
        std::fs::write(
            dir.join("plugin.json"),
            serde_json::json!({
                "$schema": PLUGIN_SCHEMA, "name": "demo", "future_field": true
            })
            .to_string(),
        )
        .unwrap();
        let mut report = Vec::new();
        assert!(load_one(&dir, &mut report).is_some());
        assert!(
            report.iter().any(|r| r.contains("future_field")),
            "{report:?}"
        );
        // Wrong schema: rejected outright.
        std::fs::write(
            dir.join("plugin.json"),
            serde_json::json!({ "$schema": "https://example.com/other.json", "name": "demo" })
                .to_string(),
        )
        .unwrap();
        let mut report = Vec::new();
        assert!(load_one(&dir, &mut report).is_none());
        assert!(
            report.iter().any(|r| r.contains("unsupported")),
            "{report:?}"
        );
        // Invalid name: rejected.
        std::fs::write(
            dir.join("plugin.json"),
            serde_json::json!({ "$schema": PLUGIN_SCHEMA, "name": "Not-Good" }).to_string(),
        )
        .unwrap();
        assert!(load_one(&dir, &mut Vec::new()).is_none());
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn skills_outside_the_plugin_root_are_skipped() {
        let root = scratch("escape");
        let dir = write_plugin(&root);
        // Replace the skill directory with a symlink pointing outside.
        let outside = root.join("elsewhere");
        std::fs::create_dir_all(&outside).unwrap();
        std::fs::write(
            outside.join("SKILL.md"),
            "---\nname: evil\ndescription: escaped\n---\nbody",
        )
        .unwrap();
        std::fs::remove_dir_all(dir.join("skills/greet")).unwrap();
        std::os::unix::fs::symlink(&outside, dir.join("skills/greet")).unwrap();
        let mut report = Vec::new();
        let plugin = load_one(&dir, &mut report).expect("plugin still loads");
        assert!(plugin.skills.is_empty(), "escaping skill must be skipped");
        assert!(report.iter().any(|r| r.contains("escapes")), "{report:?}");
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn tree_hash_notices_content_drift() {
        let root = scratch("hash");
        let dir = write_plugin(&root);
        let before = tree_hash(&dir);
        assert_eq!(before, tree_hash(&dir), "stable across calls");
        // mtime changes surface via a sleep-free rewrite only if the header
        // moves; write a new file to make drift unambiguous.
        std::fs::write(dir.join("extra.txt"), "x").unwrap();
        assert_ne!(before, tree_hash(&dir));
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn failed_staged_replacement_restores_previous_plugin() {
        let root = scratch("replace-fail");
        let dest = root.join("plugin");
        let backup = root.join("backup");
        let stage = root.join("missing-stage");
        std::fs::create_dir(&dest).unwrap();
        std::fs::write(dest.join("marker"), "old version").unwrap();
        assert!(replace_staged(&stage, &dest, &backup).is_err());
        assert_eq!(
            std::fs::read_to_string(dest.join("marker")).unwrap(),
            "old version"
        );
        assert!(!backup.exists());
        std::fs::remove_dir_all(root).ok();
    }
}
