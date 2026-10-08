//! Embedded, inspectable extension catalogue and explicit persistent choices.
use crate::{auth, config, ext, input::KeyReader, modal};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

pub struct Bundle {
    pub name: &'static str,
    pub description: &'static str,
    pub interpreter: &'static str,
    pub file: &'static str,
    pub source: &'static str,
    pub access: &'static str,
}
pub const BUNDLES: &[Bundle] = &[
    Bundle {
        name: "todo",
        description: "Task checklist",
        interpreter: "python3",
        file: "todo.py",
        source: include_str!("../../../extensions/todo/todo.py"),
        access: "Maintains its task list in memory. Contributes prompt text and tools.",
    },
    Bundle {
        name: "hashline",
        description: "Read and edit files with line hashes",
        interpreter: "python3",
        file: "hashline.py",
        source: include_str!("../../../extensions/hashline/hashline.py"),
        access: "Reads and writes workspace files.",
    },
    Bundle {
        name: "codemode",
        description: "Run scripts over host tools",
        interpreter: "node",
        file: "codemode.js",
        source: include_str!("../../../extensions/codemode/codemode.js"),
        access: "Executes JavaScript and calls host tools. Can change files and run commands.",
    },
    Bundle {
        name: "subagent",
        description: "Delegate a task to a fresh agent",
        interpreter: "python3",
        file: "subagent.py",
        source: include_str!("../../../extensions/subagent/subagent.py"),
        access: "Starts account-backed delegated agents with shell access. Uses model quota.",
    },
    Bundle {
        name: "cache-warmer",
        description: "Refresh prompt cache while idle",
        interpreter: "python3",
        file: "cache-warmer.py",
        source: include_str!("../../../extensions/cache-warmer/cache-warmer.py"),
        access: "Requests background provider cache refreshes. Uses model quota even while idle.",
    },
    Bundle {
        name: "web",
        description: "Read pages and search the web",
        interpreter: "python3",
        file: "web.py",
        source: include_str!("../../../extensions/web/web.py"),
        access: "Makes outbound HTTP requests to URLs and search backends; reads ~/.config/glass-core/web.json.",
    },
    Bundle {
        name: "mcp-bridge",
        description: "Call MCP servers' tools",
        interpreter: "python3",
        file: "mcp_bridge.py",
        source: include_str!("../../../extensions/mcp-bridge/mcp_bridge.py"),
        access: "Spawns configured MCP servers and calls their tools; reads ~/.config/glass-core/mcp.json.",
    },
];
#[derive(Default, Serialize, Deserialize)]
#[serde(default)]
struct Preferences {
    enabled: BTreeSet<String>,
    disabled: BTreeSet<String>,
    custom_enabled: BTreeSet<String>,
}
fn load() -> Result<Preferences, String> {
    match std::fs::read(config::config_dir().join("addons.json")) {
        Ok(bytes) => serde_json::from_slice(&bytes)
            .map_err(|_| "Add-on preferences are invalid; not overwritten".into()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Preferences::default()),
        Err(e) => Err(e.to_string()),
    }
}
fn save(p: &Preferences) -> Result<(), String> {
    auth::secure_write(
        &config::config_dir().join("addons.json"),
        &serde_json::to_vec_pretty(p).map_err(|e| e.to_string())?,
    )
}
fn command(bundle: &Bundle) -> Result<String, String> {
    let path = config::config_dir()
        .join("extensions/bundled")
        .join(bundle.file);
    auth::secure_write(&path, bundle.source.as_bytes())?;
    // JSON strings are parsed as argument vectors by the host; spaces in HOME are safe.
    serde_json::to_string(&vec![
        bundle.interpreter.to_string(),
        path.to_string_lossy().into_owned(),
    ])
    .map_err(|e| e.to_string())
}
pub fn apply(cfg: &mut config::Config) -> Result<(), String> {
    let preferences = load()?;
    cfg.extensions.retain(|s| !preferences.disabled.contains(s));
    for bundle in BUNDLES
        .iter()
        .filter(|b| preferences.enabled.contains(b.name))
    {
        let cmd = command(bundle)?;
        if !cfg.extensions.contains(&cmd) {
            cfg.extensions.push(cmd);
        }
    }
    for command in preferences.custom_enabled {
        if !preferences.disabled.contains(&command) && !cfg.extensions.contains(&command) {
            cfg.extensions.push(command);
        }
    }
    Ok(())
}
fn enable(
    keys: &KeyReader,
    cfg: &config::Config,
    registry: &mut ext::Registry,
    command: &str,
) -> Result<bool, String> {
    let command_for_worker = command.to_string();
    let workspace = cfg.workspace.clone();
    let model = cfg.model.clone();
    match modal::task(keys, "Starting extension", move |cancel, _| {
        ext::host::Extension::spawn_cancellable(
            &command_for_worker,
            &workspace.to_string_lossy(),
            &model,
            cancel,
        )
    }) {
        Some(Ok(mut extension)) => {
            extension.finish_startup();
            let names = extension
                .manifest
                .tools
                .iter()
                .map(|t| t.name.as_str())
                .collect::<Vec<_>>();
            let mods = crate::mods::load(&cfg.mods)?;
            if crate::provider::collect_tools(&mods)
                .iter()
                .any(|(name, _, _)| names.contains(&name.as_str()))
            {
                return Err("Extension tool conflicts with an enabled mod or core tool".into());
            }
            registry.register(extension, command)?;
            Ok(true)
        }
        Some(Err(error)) => Err(error),
        None => Ok(false),
    }
}
pub fn manage(
    keys: &KeyReader,
    cfg: &mut config::Config,
    registry: &mut ext::Registry,
) -> Result<(), String> {
    loop {
        let loaded = registry
            .names()
            .iter()
            .map(|s| s.to_string())
            .collect::<Vec<_>>();
        let mut rows = BUNDLES
            .iter()
            .map(|b| {
                format!(
                    "{} · {} · {}",
                    b.name,
                    if loaded.iter().any(|n| n == b.name) {
                        "enabled"
                    } else {
                        "disabled"
                    },
                    b.description
                )
            })
            .collect::<Vec<_>>();
        let custom = loaded
            .iter()
            .filter(|n| !BUNDLES.iter().any(|b| b.name == n.as_str()))
            .cloned()
            .collect::<Vec<_>>();
        rows.extend(custom.iter().map(|n| format!("{n} · custom · enabled")));
        let prefs = load()?;
        let disabled = prefs
            .disabled
            .iter()
            .filter(|spec| {
                let Ok(args) = ext::host::arguments(spec) else {
                    return true;
                };
                !BUNDLES.iter().any(|b| {
                    args.last().is_some_and(|path| {
                        path == &config::config_dir()
                            .join("extensions/bundled")
                            .join(b.file)
                            .to_string_lossy()
                    })
                })
            })
            .cloned()
            .collect::<Vec<_>>();
        rows.extend(
            disabled
                .iter()
                .map(|spec| format!("Disabled custom · {spec}")),
        );
        let add_index = rows.len();
        rows.push("Add a custom extension…".into());
        let warning_index = rows.len();
        if !registry.failures().is_empty() {
            rows.push("Startup warnings".into());
        }
        let Some(i) = modal::choose(
            keys,
            "Extension manager",
            "Explicit local programs only. Enablement persists globally and updates the active workspace profile.",
            &rows,
        ) else {
            return Ok(());
        };
        if i == warning_index {
            modal::info(
                keys,
                "Extension startup warnings",
                &registry.failures().join("\n\n"),
            );
            continue;
        }
        if i == add_index || i >= BUNDLES.len() + custom.len() {
            let command = if i == add_index {
                let Some(command) = modal::validated_field(
                    keys,
                    "Enable custom extension",
                    "Trusted command or JSON argument vector",
                    "",
                    false,
                    |s| ext::host::arguments(s).map(|_| ()),
                ) else {
                    continue;
                };
                command
            } else {
                let command = disabled[i - BUNDLES.len() - custom.len()].clone();
                match modal::choose(
                    keys,
                    "Disabled extension",
                    &command,
                    &["Inspect command and trust".into(), "Enable and save".into()],
                ) {
                    Some(0) => {
                        modal::info(
                            keys,
                            "Custom extension",
                            &format!(
                                "{command}\n\nTrusted local executable with filesystem, network and host-tool access. No sandbox."
                            ),
                        );
                        continue;
                    }
                    Some(1) => command,
                    _ => continue,
                }
            };
            let mut prefs = load()?;
            if enable(keys, cfg, registry, &command)? {
                prefs.disabled.remove(&command);
                prefs.custom_enabled.insert(command.clone());
                if let Err(error) = save(&prefs) {
                    let name = registry
                        .names()
                        .iter()
                        .find(|name| registry.source(name) == Some(command.as_str()))
                        .map(|name| name.to_string());
                    if let Some(name) = name {
                        registry.disable(&name);
                    }
                    return Err(error);
                }
                if !cfg.extensions.contains(&command) {
                    cfg.extensions.push(command);
                }
                crate::profiles::update_active(cfg)?;
            }
            continue;
        }
        let (name, source, access, spec) = if i < BUNDLES.len() {
            let b = &BUNDLES[i];
            (
                b.name.to_string(),
                Some(b.source),
                b.access.to_string(),
                None,
            )
        } else {
            let name = custom[i - BUNDLES.len()].clone();
            let spec = registry.source(&name).map(str::to_string);
            (name, None, "Custom executable: trusted filesystem, network, and host-tool access. Inspect its source before use.".into(), spec)
        };
        let enabled = loaded.contains(&name);
        let actions = vec![
            "Inspect access and details".into(),
            "View bundled source".into(),
            if enabled {
                "Disable and save".into()
            } else {
                "Enable and save".into()
            },
        ];
        match modal::choose(keys, &name, &access, &actions) {
            Some(0) => {
                let details = registry
                    .describe()
                    .into_iter()
                    .find(|s| s.starts_with(&format!("{name} v")))
                    .unwrap_or_else(|| "Not running".into());
                modal::info(
                    keys,
                    "Extension access",
                    &format!(
                        "{details}\n\n{access}\n\nExtensions are not sandboxed. Provider-secret environment variables are filtered. Source is copied from this GC binary; no download or package install occurs.\n\n{}",
                        spec.as_deref().unwrap_or("Bundled extension")
                    ),
                );
            }
            Some(1) => modal::info(
                keys,
                "Extension source",
                source.unwrap_or(
                    "Custom source is not embedded. Inspect the configured executable on disk.",
                ),
            ),
            Some(2) => {
                let mut prefs = load()?;
                if enabled {
                    let spec = registry
                        .source(&name)
                        .map(str::to_string)
                        .ok_or("Extension source is unavailable")?;
                    prefs.enabled.remove(&name);
                    prefs.custom_enabled.remove(&spec);
                    prefs.disabled.insert(spec.clone());
                    save(&prefs)?;
                    registry.disable(&name);
                    cfg.extensions.retain(|s| s != &spec);
                } else {
                    let bundle = &BUNDLES[i];
                    let spec = command(bundle)?;
                    if !enable(keys, cfg, registry, &spec)? {
                        continue;
                    }
                    prefs.enabled.insert(name);
                    prefs.disabled.remove(&spec);
                    if let Err(e) = save(&prefs) {
                        registry.disable(bundle.name);
                        return Err(e);
                    }
                    if !cfg.extensions.contains(&spec) {
                        cfg.extensions.push(spec);
                    }
                }
                crate::profiles::update_active(cfg)?;
            }
            _ => {}
        }
    }
}
