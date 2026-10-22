//! On-demand effective settings with explicit layer attribution. Secrets stay out.
use crate::{config, profiles};
use serde_json::{Value, json};
use std::collections::BTreeMap;

fn values(c: &config::Config) -> BTreeMap<String, Value> {
    let value = json!({"provider":c.provider,"model":c.model,"image_input":c.image_input,"connection":c.connection,
        "base_url_fingerprint":crate::workspace::hash(c.base_url.as_bytes()),
        "adaptive_pacing":c.adaptive_pacing,"max_tokens":c.max_tokens,"reasoning_budget":c.reasoning_budget,"cost_limit":c.cost_limit,
        "mods":c.mods,"extension_fingerprints":c.extensions.iter().map(|s|crate::workspace::hash(s.as_bytes())).collect::<Vec<_>>(),
        "system_prompt_fingerprint":crate::workspace::hash(c.system_prompt.as_bytes()),"system_prompt_bytes":c.system_prompt.len(),
        "trust":format!("{:?}",c.trust),"sandbox":format!("{:?}",c.sandbox),
        "permission_profile":format!("{:?}",c.permission_profile),"extension_isolation":c.extension_isolation,
        "sandbox_ports":c.sandbox_ports,"sandbox_env":c.sandbox_env,"log_fsync":c.log_fsync,
        "live_flush_ms":c.live_flush_ms,"program_status":c.program_status});
    value
        .as_object()
        .unwrap()
        .iter()
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect()
}

pub fn snapshot(effective: &config::Config) -> Result<Value, String> {
    let requirements = crate::requirements::load(&config::config_dir())?;
    let mut governed = effective.clone();
    crate::requirements::apply(&requirements, &mut governed);
    let effective = &governed;
    let mut candidate = config::load(&config::default_path()).map_err(|e| e.to_string())?;
    candidate.workspace = effective.workspace.clone();
    let mut current = values(&candidate);
    let defaults = values(&config::Config::default());
    let mut sources: BTreeMap<String, String> = current
        .iter()
        .map(|(k, v)| {
            (
                k.clone(),
                if defaults.get(k) == Some(v) {
                    "default or explicitly configured equivalent"
                } else {
                    "global configuration"
                }
                .into(),
            )
        })
        .collect();
    let mut apply = |layer: &str, candidate: &config::Config| {
        let next = values(candidate);
        for (key, value) in &next {
            if current.get(key) != Some(value) {
                sources.insert(key.clone(), layer.into());
            }
        }
        current = next;
    };
    crate::auth::apply(&mut candidate)?;
    apply("active saved connection", &candidate);
    crate::addons::apply(&mut candidate)?;
    apply("extension preferences", &candidate);
    let store = profiles::load()?;
    let root = effective
        .workspace
        .canonicalize()
        .map_err(|e| e.to_string())?
        .to_string_lossy()
        .into_owned();
    let active = store.active.get(&root).cloned();
    if let Some(name) = &active {
        let profile = store
            .profiles
            .get(name)
            .ok_or("Active workspace profile is missing")?;
        let theme = glass_tui::theme::current();
        let animation = glass_tui::theme::current_animation();
        let result = profile.apply(&mut candidate);
        glass_tui::theme::select(theme);
        glass_tui::theme::select_animation(animation);
        result?;
        apply("workspace profile", &candidate);
    }
    apply("current process override", effective);
    if requirements.sandbox.is_some() {
        sources.insert("sandbox".into(), "admin requirement".into());
    }
    if requirements.sandbox_env.is_some() {
        sources.insert("sandbox_env".into(), "admin requirement".into());
    }
    let rows = values(effective)
        .into_iter()
        .map(|(key, value)| json!({"setting":key,"value":value,"source":sources.get(&key)}))
        .collect::<Vec<_>>();
    Ok(
        json!({"schema_version":1,"active_workspace_profile":active,"settings":rows,
        "precedence":["defaults","global configuration","active saved connection","extension preferences","workspace profile","current process override","admin requirement"],
        "attribution":"Source is the last layer that changed a value; explicitly equal overrides cannot be distinguished from defaults by value comparison.",
        "redaction":"Endpoint URLs, executable arguments, prompt contents and credentials are omitted; fingerprints identify changes.",
        "reset":"Use /profiles → Use global settings for this workspace to remove the profile binding. Existing saved profiles are preserved."}),
    )
}

pub fn command(args: &[String]) -> Result<(), String> {
    if args.iter().any(|a| a != "--json") || args.len() > 1 {
        return Err("gc settings [--json]".into());
    }
    let mut cfg = config::load(&config::default_path()).map_err(|e| e.to_string())?;
    crate::auth::apply(&mut cfg)?;
    crate::addons::apply(&mut cfg)?;
    profiles::apply_active(&mut cfg)?;
    let value = snapshot(&cfg)?;
    if args.iter().any(|a| a == "--json") {
        crate::emit_json(value);
    } else {
        crate::cli::report("Effective settings", &value);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn settings_projection_omits_secrets_and_executable_arguments() {
        let cfg = config::Config {
            base_url: "https://host/?token=endpoint-secret".into(),
            extensions: vec!["helper --key executable-secret".into()],
            system_prompt: "private-prompt-secret".into(),
            ..Default::default()
        };
        let rendered = serde_json::to_string(&values(&cfg)).unwrap();
        for secret in [
            "endpoint-secret",
            "executable-secret",
            "private-prompt-secret",
        ] {
            assert!(!rendered.contains(secret));
        }
    }
}
