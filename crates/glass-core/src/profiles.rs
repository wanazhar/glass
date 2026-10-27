//! Workspace settings saved outside repositories; no credentials are copied.
use crate::{auth, config, input::KeyReader, modal};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, path::Path};

#[derive(Clone, Serialize, Deserialize)]
pub struct Profile {
    pub name: String,
    pub provider: String,
    pub base_url: String,
    pub model: String,
    pub connection: Option<String>,
    pub api_key_env: Option<String>,
    pub mods: Vec<String>,
    pub extensions: Vec<String>,
    pub system_prompt: String,
    pub theme: usize,
    #[serde(default)]
    pub animation: usize,
    #[serde(default = "enabled")]
    pub adaptive_pacing: bool,
}
fn enabled() -> bool {
    true
}
#[derive(Default, Serialize, Deserialize)]
pub struct Store {
    pub profiles: BTreeMap<String, Profile>,
    pub active: BTreeMap<String, String>,
}
pub fn load() -> Result<Store, String> {
    match config::read_bounded_text(&config::config_dir().join("profiles.json"), 1024 * 1024) {
        Ok(Some(text)) => serde_json::from_str(&text)
            .map_err(|_| "Workspace profiles are invalid; not overwritten".into()),
        Ok(None) => Ok(Store::default()),
        Err(e) => Err(e.to_string()),
    }
}
fn save(store: &Store) -> Result<(), String> {
    auth::secure_write(
        &config::config_dir().join("profiles.json"),
        &serde_json::to_vec_pretty(store).map_err(|e| e.to_string())?,
    )
}
fn workspace(path: &Path) -> Result<String, String> {
    path.canonicalize()
        .map(|p| p.to_string_lossy().into_owned())
        .map_err(|e| e.to_string())
}
impl Profile {
    pub fn capture(name: String, cfg: &config::Config) -> Self {
        Self {
            name,
            provider: cfg.provider.clone(),
            base_url: cfg.base_url.clone(),
            model: cfg.model.clone(),
            connection: cfg.connection.clone(),
            api_key_env: cfg.api_key_env.clone(),
            mods: cfg.mods.clone(),
            extensions: cfg.extensions.clone(),
            system_prompt: cfg.system_prompt.clone(),
            theme: glass_tui::theme::current(),
            animation: glass_tui::theme::current_animation(),
            adaptive_pacing: cfg.adaptive_pacing,
        }
    }
    pub fn apply(&self, cfg: &mut config::Config) -> Result<(), String> {
        crate::provider::validate_endpoint(&self.base_url).map_err(|e| e.to_string())?;
        cfg.adaptive_pacing = self.adaptive_pacing;
        cfg.provider = self.provider.clone();
        cfg.base_url = self.base_url.clone();
        cfg.model = self.model.clone();
        cfg.connection = self.connection.clone();
        cfg.api_key_env = self.api_key_env.clone();
        cfg.mods = self.mods.clone();
        cfg.extensions = self.extensions.clone();
        cfg.system_prompt = self.system_prompt.clone();
        glass_tui::theme::select(self.theme);
        glass_tui::theme::select_animation(self.animation);
        Ok(())
    }
}
pub fn apply_active(cfg: &mut config::Config) -> Result<(), String> {
    let store = load()?;
    if let Some(name) = store.active.get(&workspace(&cfg.workspace)?) {
        store
            .profiles
            .get(name)
            .ok_or("Active workspace profile is missing")?
            .apply(cfg)?;
    }
    Ok(())
}
pub fn update_active(cfg: &config::Config) -> Result<(), String> {
    let mut store = load()?;
    if let Some(name) = store.active.get(&workspace(&cfg.workspace)?).cloned() {
        store
            .profiles
            .insert(name.clone(), Profile::capture(name, cfg));
        save(&store)?;
    }
    Ok(())
}

/// Returns settings to apply live. Executable changes always originate in this UI.
pub fn manage(keys: &KeyReader, cfg: &config::Config) -> Result<Option<config::Config>, String> {
    let root = workspace(&cfg.workspace)?;
    loop {
        let mut store = load()?;
        let mut rows = vec![
            "Save current workspace settings…".into(),
            "Use global settings for this workspace".into(),
            "Edit current instructions…".into(),
        ];
        let names = store.profiles.keys().cloned().collect::<Vec<_>>();
        rows.extend(names.iter().map(|n| {
            format!(
                "{}{}",
                if store.active.get(&root) == Some(n) {
                    "✓ "
                } else {
                    ""
                },
                n
            )
        }));
        let Some(i) = modal::choose(
            keys,
            "Workspace profiles",
            "Private profiles contain settings and account IDs, never account tokens. Selecting a profile can start its explicitly saved extensions.",
            &rows,
        ) else {
            return Ok(None);
        };
        if i == 0 {
            let Some(name) =
                modal::validated_field(keys, "Save workspace profile", "Name", "", false, |s| {
                    if s.trim().is_empty() {
                        Err("Enter a profile name".into())
                    } else {
                        Ok(())
                    }
                })
            else {
                continue;
            };
            let name = name.trim().to_string();
            if store.profiles.contains_key(&name)
                && modal::choose(
                    keys,
                    "Replace profile?",
                    &name,
                    &[
                        "Keep existing profile".into(),
                        "Replace saved profile".into(),
                    ],
                ) != Some(1)
            {
                continue;
            }
            store
                .profiles
                .insert(name.clone(), Profile::capture(name.clone(), cfg));
            store.active.insert(root.clone(), name);
            save(&store)?;
            modal::info(
                keys,
                "Profile saved",
                "These settings will load automatically in this workspace on the next launch.",
            );
        } else if i == 1 {
            store.active.remove(&root);
            save(&store)?;
            let mut global = config::load(&config::default_path()).map_err(|e| e.to_string())?;
            global.workspace = cfg.workspace.clone();
            auth::apply(&mut global)?;
            crate::addons::apply(&mut global)?;
            return Ok(Some(global));
        } else if i == 2 {
            if let Some(prompt) = modal::field(
                keys,
                "Workspace instructions",
                "Edit instructions; save a profile to persist",
                &cfg.system_prompt,
                false,
            ) {
                let mut candidate = cfg.clone();
                candidate.system_prompt = prompt;
                return Ok(Some(candidate));
            }
        } else {
            let name = &names[i - 3];
            let p = &store.profiles[name];
            let actions = [
                "Use in this workspace".into(),
                "Inspect settings".into(),
                "Delete profile".into(),
            ];
            match modal::choose(
                keys,
                name,
                &format!(
                    "{} · {} · {} extensions",
                    p.provider,
                    p.model,
                    p.extensions.len()
                ),
                &actions,
            ) {
                Some(0) => {
                    let mut candidate = cfg.clone();
                    p.apply(&mut candidate)?;
                    store.active.insert(root.clone(), name.clone());
                    save(&store)?;
                    return Ok(Some(candidate));
                }
                Some(1) => modal::info(
                    keys,
                    "Profile settings",
                    &serde_json::to_string_pretty(p).map_err(|e| e.to_string())?,
                ),
                Some(2)
                    if modal::choose(
                        keys,
                        "Delete profile?",
                        name,
                        &["Keep profile".into(), "Delete saved profile".into()],
                    ) == Some(1) =>
                {
                    store.profiles.remove(name);
                    store.active.retain(|_, n| n != name);
                    save(&store)?;
                }
                _ => {}
            }
        }
    }
}

/// Explicit human profile changes shared by local and remote frontends.
pub fn request(
    cfg: &mut config::Config,
    action: &str,
    name: &str,
    text: &str,
    confirmed: bool,
) -> Result<serde_json::Value, String> {
    let mut store = load()?;
    let root = workspace(&cfg.workspace)?;
    match action {
        "" | "list" => {}
        "save" => {
            if name.trim().is_empty() || name.len() > 200 {
                return Err("Use a profile name up to 200 bytes".into());
            }
            if store.profiles.contains_key(name) && !confirmed {
                return Err("Confirm replacing the saved profile".into());
            }
            store
                .profiles
                .insert(name.into(), Profile::capture(name.into(), cfg));
            store.active.insert(root.clone(), name.into());
            save(&store)?;
        }
        "use" => {
            if !confirmed {
                return Err("Confirm applying the profile, including its extensions".into());
            }
            let profile = store.profiles.get(name).ok_or("Unknown profile")?;
            profile.apply(cfg)?;
            store.active.insert(root.clone(), name.into());
            save(&store)?;
        }
        "delete" => {
            if !confirmed {
                return Err("Confirm deleting the profile".into());
            }
            store.profiles.remove(name);
            store.active.retain(|_, v| v != name);
            save(&store)?;
        }
        "instructions" => {
            if text.len() > 60000 {
                return Err("Instructions exceed 60000 bytes".into());
            }
            cfg.system_prompt = text.into();
            update_active(cfg)?;
        }
        "global" => {
            if !confirmed {
                return Err("Confirm applying global settings".into());
            }
            let mut global = config::load(&config::default_path()).map_err(|e| e.to_string())?;
            global.workspace = cfg.workspace.clone();
            auth::apply(&mut global)?;
            crate::addons::apply(&mut global)?;
            store.active.remove(&root);
            save(&store)?;
            *cfg = global;
        }
        _ => return Err("Choose list, save, use, delete, instructions, or global".into()),
    }
    Ok(
        serde_json::json!({"active":store.active.get(&root),"profiles":store.profiles.values().map(|p|serde_json::json!({"name":p.name,"provider":p.provider,"model":p.model,"mods":p.mods,"extensions":p.extensions})).collect::<Vec<_>>()}),
    )
}
