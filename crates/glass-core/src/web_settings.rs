//! Universal host preferences. No session, model invocation or conversation writer is used.
use crate::{auth, config};
use serde_json::{Value, json};
use std::path::Path;
fn raw_config() -> Result<String, String> {
    config::read_bounded_text(&config::default_path(), 1024 * 1024)
        .map(|v| v.unwrap_or_default())
        .map_err(|_| "Cannot read global configuration".into())
}
fn revision() -> Result<String, String> {
    let mut bytes = raw_config()?.into_bytes();
    for file in [
        "auth.json",
        "addons.json",
        "profiles.json",
        "requirements.toml",
    ] {
        bytes.extend(
            crate::requests::bounded_optional(&config::config_dir().join(file), 1024 * 1024)?
                .unwrap_or_default(),
        );
    }
    Ok(crate::hashing::sha256(&bytes))
}
fn defaults() -> Result<config::Config, String> {
    config::load(&config::default_path())
        .map_err(|_| "Global configuration is invalid; it has not been overwritten".into())
}
pub fn snapshot() -> Result<Value, String> {
    for _ in 0..3 {
        let before = revision()?;
        let snapshot = snapshot_once()?;
        if snapshot["revision"].as_str() == Some(before.as_str()) {
            return Ok(snapshot);
        }
    }
    Err("Settings changed while loading. Refresh to read a consistent snapshot.".into())
}
fn snapshot_once() -> Result<Value, String> {
    let cfg = defaults()?;
    let accounts = auth::load()?;
    let profiles = crate::profiles::load()?;
    let requirements = crate::requirements::load(&config::config_dir())?;
    let skills = cfg
        .skills_dir
        .as_deref()
        .map(crate::skills::load_dir)
        .unwrap_or_default();
    Ok(
        json!({"revision":revision()?,"scope":"global","applies":"new_conversations","defaults":{
        "provider":cfg.provider,"base_url":cfg.base_url,"model":cfg.model,"max_tokens":cfg.max_tokens,"reasoning_budget":cfg.reasoning_budget,"cost_limit":cfg.cost_limit,"adaptive_pacing":cfg.adaptive_pacing,"image_input":cfg.image_input,"system_prompt":cfg.system_prompt,"skills_dir":cfg.skills_dir,"permission_profile":format!("{:?}",cfg.permission_profile).to_lowercase().replace("workspaceedit","workspace-edit").replace("readonly","read-only"),"sandbox":format!("{:?}",requirements.sandbox.unwrap_or(cfg.sandbox)).to_lowercase(),"sandbox_managed":requirements.sandbox.is_some(),"mods":cfg.mods},
        "active_connection":accounts.active,"connections":accounts.connections.iter().map(|c|json!({"id":c.id,"name":c.label,"provider":c.provider,"endpoint":c.base_url,"model":c.model,"has_credential":c.credential.is_some()||c.api_key_env.is_some()})).collect::<Vec<_>>(),
        "available_mods":crate::mods::available(),"extensions":crate::addons::global_settings(&cfg)?,
        "profiles":profiles.profiles.values().map(|p|json!({"name":p.name,"model":p.model,"instructions":p.system_prompt,"adaptive_pacing":p.adaptive_pacing,"workspaces":profiles.active.iter().filter(|(_,name)|*name==&p.name).map(|(path,_)|path).collect::<Vec<_>>()})).collect::<Vec<_>>(),
        "skills":skills.iter().map(|s|json!({"name":s.name,"description":s.description})).collect::<Vec<_>>() }),
    )
}
/// Preserve unrelated TOML and comments. Refuse ambiguous layouts instead of rewriting them.
fn patch_text(text: &str, key: &str, value: Option<toml::Value>) -> Result<String, String> {
    let mut expected: toml::Value =
        toml::from_str(text).map_err(|_| "Global configuration is invalid")?;
    let table = expected
        .as_table_mut()
        .ok_or("Expected a configuration table")?;
    if let Some(v) = &value {
        table.insert(key.into(), v.clone());
    } else {
        table.remove(key);
    }
    let mut document = text
        .parse::<toml_edit::DocumentMut>()
        .map_err(|_| "Global configuration cannot be edited")?;
    if let Some(value) = value {
        let mut edited = value
            .to_string()
            .parse::<toml_edit::Value>()
            .map_err(|_| "Invalid setting value")?;
        if let Some(old) = document.get(key).and_then(toml_edit::Item::as_value) {
            *edited.decor_mut() = old.decor().clone();
        }
        document[key] = toml_edit::Item::Value(edited);
    } else {
        document.remove(key);
    }
    let output = document.to_string();
    if toml::from_str::<toml::Value>(&output).ok().as_ref() != Some(&expected) {
        return Err(
            "This configuration layout needs manual editing; no settings were changed".into(),
        );
    }
    Ok(output)
}
fn checked(key: &str, value: &Value, cfg: &config::Config) -> Result<Option<toml::Value>, String> {
    let string = || {
        value
            .as_str()
            .filter(|s| s.len() <= 60000 && !s.contains('\0'))
            .map(str::to_owned)
            .ok_or_else(|| "Enter a valid text value".to_string())
    };
    Ok(Some(match key {
        "model" => {
            let v = string()?;
            if v.trim().is_empty() || v.len() > 256 || v.chars().any(char::is_control) {
                return Err("Enter a model ID up to 256 characters".into());
            }
            toml::Value::String(v)
        }
        "provider" => {
            let v = string()?;
            if !matches!(v.as_str(), "openai" | "openai-responses" | "anthropic") {
                return Err("Unknown provider".into());
            }
            toml::Value::String(v)
        }
        "base_url" => {
            let v = string()?;
            crate::provider::validate_endpoint(&v)
                .map_err(|_| "Enter a valid provider endpoint")?;
            toml::Value::String(v)
        }
        "system_prompt" => toml::Value::String(string()?),
        "skills_dir" => {
            let v = string()?;
            if v.is_empty() {
                return Ok(None);
            }
            if !Path::new(&v).is_dir() {
                return Err("Choose an existing skills folder on the host".into());
            }
            toml::Value::String(v)
        }
        "max_tokens" => {
            let n = value
                .as_u64()
                .filter(|n| *n > 0 && *n <= 128000)
                .ok_or("Output limit must be 1–128000")?;
            if cfg.reasoning_budget.is_some_and(|r| u64::from(r) >= n) {
                return Err("Output allowance must exceed reasoning allowance".into());
            }
            toml::Value::Integer(n as i64)
        }
        "reasoning_budget" => {
            if value.is_null() {
                return Ok(None);
            }
            let n = value
                .as_u64()
                .filter(|n| *n < u64::from(cfg.max_tokens))
                .ok_or("Reasoning allowance must be below output allowance")?;
            toml::Value::Integer(n as i64)
        }
        "cost_limit" => {
            if value.is_null() {
                return Ok(None);
            }
            let n = value
                .as_f64()
                .filter(|n| n.is_finite() && *n > 0.0)
                .ok_or("Enter a positive USD limit or leave empty for no limit")?;
            toml::Value::Float(n)
        }
        "adaptive_pacing" | "image_input" => {
            toml::Value::Boolean(value.as_bool().ok_or("Expected on or off")?)
        }
        "permission_profile" => {
            let v = string()?;
            crate::sandbox::PermissionProfile::parse(&v)
                .ok_or("Choose unrestricted, workspace-edit or read-only")?;
            toml::Value::String(v)
        }
        "sandbox" => {
            if crate::requirements::load(&config::config_dir())?
                .sandbox
                .is_some()
            {
                return Err("This setting is managed by the host administrator".into());
            }
            let v = string()?;
            crate::sandbox::Policy::parse(&v).ok_or("Choose off, workspace or strict")?;
            toml::Value::String(v)
        }
        "mods" => {
            let names = value.as_array().ok_or("Expected a tool list")?;
            let available = crate::mods::available();
            let mut selected = Vec::new();
            for n in names {
                let n = n.as_str().ok_or("Invalid tool")?;
                if !available.iter().any(|v| *v == n) {
                    return Err("Unknown native tool".into());
                }
                if !selected.contains(&toml::Value::String(n.into())) {
                    selected.push(toml::Value::String(n.into()));
                }
            }
            toml::Value::Array(selected)
        }
        _ => return Err("Unknown global preference".into()),
    }))
}
pub fn update(v: &Value) -> Result<Value, String> {
    std::fs::create_dir_all(config::config_dir()).map_err(|e| e.to_string())?;
    let file = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(config::config_dir().join("settings.lock"))
        .map_err(|e| e.to_string())?;
    fs2::FileExt::lock_exclusive(&file).map_err(|e| e.to_string())?;
    if v["revision"].as_str() != Some(revision()?.as_str()) {
        return Err("Settings changed elsewhere. Refresh before saving your changes.".into());
    }
    let action = v["action"].as_str().unwrap_or("");
    match action {
        "preference" => {
            let cfg = defaults()?;
            let key = v["key"].as_str().ok_or("Missing preference")?;
            let value = checked(key, &v["value"], &cfg)?;
            let before = raw_config()?;
            let output = patch_text(&before, key, value)?;
            if before != raw_config()? {
                return Err("Configuration changed elsewhere. Refresh and try again.".into());
            }
            auth::secure_write(&config::default_path(), output.as_bytes())?;
        }
        "extension" => crate::addons::global_toggle(
            v["id"].as_str().ok_or("Missing extension")?,
            v["enabled"].as_bool().ok_or("Missing enabled state")?,
        )?,
        "connection-use" | "connection-model" | "connection-remove" => {
            let id = v["id"].as_str().ok_or("Missing connection")?;
            let model = v["model"].as_str().unwrap_or("");
            if action == "connection-model"
                && (model.trim().is_empty()
                    || model.len() > 256
                    || model.chars().any(char::is_control))
            {
                return Err("Enter a model ID up to 256 characters".into());
            }
            auth::edit(|store| {
                if v["revision"].as_str() != Some(revision()?.as_str()) {
                    return Err("Settings changed elsewhere. Refresh before saving.".into());
                }
                if action == "connection-use" && id.is_empty() {
                    store.active = None;
                    return Ok(());
                }
                let c = store
                    .connections
                    .iter()
                    .find(|c| c.id == id)
                    .cloned()
                    .ok_or("Connection no longer exists")?;
                match action {
                    "connection-use" => store.active = Some(id.into()),
                    "connection-model" => {
                        store
                            .connections
                            .iter_mut()
                            .find(|c| c.id == id)
                            .unwrap()
                            .model = model.into()
                    }
                    _ => {
                        if v["confirmed"] != true {
                            return Err("Confirm removing this connection".into());
                        }
                        store.connections.retain(|c| c.id != id);
                        if let Some(credential) = c.credential
                            && !store
                                .connections
                                .iter()
                                .any(|c| c.credential.as_ref() == Some(&credential))
                        {
                            store.credentials.remove(&credential);
                        }
                        if store.active.as_deref() == Some(id) {
                            store.active = None;
                        }
                    }
                }
                Ok(())
            })?;
        }
        "connection-add" | "oauth" | "callback" => {
            let mut cfg = defaults()?;
            let mut request = v.clone();
            request["confirmed"] = json!(true);
            let mode = match action {
                "connection-add" => "api",
                "oauth" => "oauth",
                _ => "callback",
            };
            let login = crate::web_commands::login(&mut cfg, "universal-settings", mode, &request);
            if let Some(Value::String(secret)) = request.get_mut("secret") {
                zeroize::Zeroize::zeroize(secret);
            }
            let login = login?;
            let mut result = snapshot()?;
            result["sign_in"] = login;
            return Ok(result);
        }
        "profile" | "profile-create" | "profile-delete" | "profile-assign" => {
            let name = v["name"].as_str().ok_or("Missing profile name")?;
            if name.trim().is_empty() || name.len() > 200 || name.chars().any(char::is_control) {
                return Err("Enter a profile name up to 200 characters".into());
            }
            let mut store = crate::profiles::load()?;
            if action == "profile-create" {
                if store.profiles.contains_key(name) {
                    return Err("A profile with this name already exists".into());
                }
                let mut cfg = defaults()?;
                auth::apply(&mut cfg)?;
                store.profiles.insert(
                    name.into(),
                    crate::profiles::Profile::capture(name.into(), &cfg),
                );
            }
            if !store.profiles.contains_key(name) {
                return Err("Profile no longer exists".into());
            }
            if action == "profile-delete" {
                if v["confirmed"] != true {
                    return Err("Confirm deleting this profile".into());
                }
                store.profiles.remove(name);
                store.active.retain(|_, profile| profile != name);
            } else if action == "profile-assign" {
                let path = Path::new(v["workspace"].as_str().ok_or("Enter a workspace folder")?)
                    .canonicalize()
                    .map_err(|_| "Workspace folder does not exist")?;
                if !path.is_dir() {
                    return Err("Choose a workspace folder".into());
                }
                store
                    .active
                    .insert(path.to_string_lossy().into_owned(), name.into());
            } else {
                let p = store.profiles.get_mut(name).unwrap();
                if let Some(model) = v["model"].as_str() {
                    if model.trim().is_empty()
                        || model.len() > 256
                        || model.chars().any(char::is_control)
                    {
                        return Err("Invalid model ID".into());
                    }
                    p.model = model.into();
                }
                if let Some(text) = v["instructions"].as_str() {
                    if text.len() > 60000 {
                        return Err("Instructions exceed 60000 bytes".into());
                    }
                    p.system_prompt = text.into();
                }
            }
            if v["revision"].as_str() != Some(revision()?.as_str()) {
                return Err("Profiles changed elsewhere. Refresh before saving.".into());
            }
            auth::secure_write(
                &config::config_dir().join("profiles.json"),
                &serde_json::to_vec_pretty(&store).map_err(|e| e.to_string())?,
            )?;
        }
        _ => return Err("Unknown settings action".into()),
    }
    snapshot()
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn patch_preserves_other_values_and_rejects_ambiguous_layouts() {
        let text = "# preferences\nmodel = \"old\"\n[hooks]\nvalue = \"retained\"\n";
        let out = patch_text(text, "model", Some(toml::Value::String("new".into()))).unwrap();
        assert!(out.contains("# preferences"));
        assert!(out.contains("value = \"retained\""));
        let out = patch_text(&out, "adaptive_pacing", Some(toml::Value::Boolean(false))).unwrap();
        assert!(
            !toml::from_str::<toml::Value>(&out).unwrap()["adaptive_pacing"]
                .as_bool()
                .unwrap()
        );
    }
    #[test]
    fn edits_multiline_instructions_without_rewriting_other_tables() {
        let text = "system_prompt = \"\"\"first\nsecond\"\"\" # keep\n\n[custom]\nsecret_ref = \"ENV_ONLY\"\n";
        let output = patch_text(
            text,
            "system_prompt",
            Some(toml::Value::String("new\ninstructions".into())),
        )
        .unwrap();
        assert!(output.contains("# keep"));
        assert!(output.contains("secret_ref = \"ENV_ONLY\""));
        assert_eq!(
            toml::from_str::<toml::Value>(&output).unwrap()["system_prompt"].as_str(),
            Some("new\ninstructions")
        );
    }
    #[test]
    fn validates_before_writing() {
        let cfg = config::Config::default();
        assert!(checked("max_tokens", &json!(0), &cfg).is_err());
        assert!(checked("unknown", &json!(true), &cfg).is_err());
        assert!(
            checked("reasoning_budget", &Value::Null, &cfg)
                .unwrap()
                .is_none()
        );
        assert!(checked("mods", &json!(["not-a-tool"]), &cfg).is_err());
    }
}
