//! Provider/model selection and account setup, inspired by pi and Hermes.
use crate::{
    auth::{self, Connection, Credential, Store},
    config::Config,
    input::KeyReader,
    modal,
};
use serde_json::Value;

struct Preset {
    label: &'static str,
    provider: &'static str,
    url: &'static str,
    env: &'static str,
}
const PRESETS: &[Preset] = &[
    Preset {
        label: "Continue with ChatGPT",
        provider: "openai-responses",
        url: "https://api.openai.com/v1",
        env: "",
    },
    Preset {
        label: "Anthropic",
        provider: "anthropic",
        url: "https://api.anthropic.com",
        env: "ANTHROPIC_API_KEY",
    },
    Preset {
        label: "OpenAI API",
        provider: "openai-responses",
        url: "https://api.openai.com/v1",
        env: "OPENAI_API_KEY",
    },
    Preset {
        label: "Google Gemini",
        provider: "openai",
        url: "https://generativelanguage.googleapis.com/v1beta/openai",
        env: "GEMINI_API_KEY",
    },
    Preset {
        label: "OpenRouter",
        provider: "openai",
        url: "https://openrouter.ai/api/v1",
        env: "OPENROUTER_API_KEY",
    },
    Preset {
        label: "DeepSeek",
        provider: "openai",
        url: "https://api.deepseek.com/v1",
        env: "DEEPSEEK_API_KEY",
    },
    Preset {
        label: "Groq",
        provider: "openai",
        url: "https://api.groq.com/openai/v1",
        env: "GROQ_API_KEY",
    },
    Preset {
        label: "Mistral",
        provider: "openai",
        url: "https://api.mistral.ai/v1",
        env: "MISTRAL_API_KEY",
    },
    Preset {
        label: "xAI",
        provider: "openai",
        url: "https://api.x.ai/v1",
        env: "XAI_API_KEY",
    },
    Preset {
        label: "Cerebras",
        provider: "openai",
        url: "https://api.cerebras.ai/v1",
        env: "CEREBRAS_API_KEY",
    },
    Preset {
        label: "Z.AI Coding Plan",
        provider: "openai",
        url: "https://api.z.ai/api/coding/paas/v4",
        env: "ZAI_API_KEY",
    },
    Preset {
        label: "Ollama",
        provider: "openai",
        url: "http://127.0.0.1:11434/v1",
        env: "",
    },
    Preset {
        label: "LM Studio",
        provider: "openai",
        url: "http://127.0.0.1:1234/v1",
        env: "",
    },
];
#[derive(Clone)]
pub struct Model {
    pub id: String,
    pub label: String,
}
pub fn parse_models(v: &Value) -> Vec<Model> {
    if let Some(models) = v["models"].as_array() {
        return models
            .iter()
            .filter(|m| m["visibility"] == "list")
            .filter_map(|m| {
                Some(Model {
                    id: m["slug"].as_str()?.into(),
                    label: m["display_name"]
                        .as_str()
                        .unwrap_or(m["slug"].as_str()?)
                        .into(),
                })
            })
            .collect();
    }
    v["data"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|m| {
            Some(Model {
                id: m["id"].as_str()?.into(),
                label: m["display_name"]
                    .as_str()
                    .or_else(|| m["name"].as_str())
                    .unwrap_or(m["id"].as_str()?)
                    .into(),
            })
        })
        .collect()
}
pub fn fetch_models(cfg: &Config, key: Option<String>) -> Result<Vec<Model>, String> {
    let url = format!(
        "{}/{}",
        cfg.base_url.trim_end_matches('/'),
        if cfg.provider == "anthropic" {
            "v1/models"
        } else {
            "models"
        }
    );
    let mut request = auth::http().get(url);
    if cfg.provider == "anthropic" {
        request = request.header("anthropic-version", "2023-06-01");
        if let Some(k) = key {
            request = request.header("x-api-key", k);
        }
    } else if let Some(k) = key {
        request = request.header("authorization", format!("Bearer {k}"));
    }
    let mut response = request.call().map_err(|_| {
        "Could not reach the provider. Check the endpoint and your connection.".to_string()
    })?;
    let status = response.status().as_u16();
    if !(200..300).contains(&status) {
        return Err(match status {
            401 | 403 => format!("Authentication rejected (HTTP {status}). Reconnect with /login."),
            429 => "Provider rate limit reached. Retry shortly.".into(),
            _ => format!(
                "Model discovery failed (HTTP {status}). This endpoint may require a custom model ID."
            ),
        });
    }
    let value: Value = response
        .body_mut()
        .read_json()
        .map_err(|_| "Provider returned an invalid model catalog".to_string())?;
    Ok(parse_models(&value))
}
fn model_for(keys: &KeyReader, cfg: &Config, key: Option<String>) -> Option<String> {
    let cfg_for_fetch = cfg.clone();
    let result = modal::work(keys, "Loading available models", move || {
        fetch_models(&cfg_for_fetch, key)
    })?;
    let models = match result {
        Ok(m) => m,
        Err(e) => {
            modal::info(keys, "Could not load models", &e);
            if e.starts_with("Authentication rejected") {
                return None;
            }
            let action = modal::choose(
                keys,
                "Model selection",
                "Discovery is unavailable",
                &["Enter a custom model ID".into(), "Cancel".into()],
            )?;
            return if action == 0 {
                modal::field(
                    keys,
                    "Custom model",
                    "Model ID supplied by your provider",
                    &cfg.model,
                    false,
                )
            } else {
                None
            };
        }
    };
    let mut items: Vec<_> = models
        .iter()
        .map(|m| {
            format!(
                "{}{} · {}",
                if m.id == cfg.model { "✓ " } else { "" },
                m.label,
                m.id
            )
        })
        .collect();
    items.push("Enter a custom model ID…".into());
    let idx = modal::choose(
        keys,
        "Provider and model",
        &format!("{} · {} models", cfg.base_url, models.len()),
        &items,
    )?;
    if idx == models.len() {
        modal::field(
            keys,
            "Custom model",
            "Model ID supplied by your provider",
            &cfg.model,
            false,
        )
    } else {
        Some(models[idx].id.clone())
    }
}
fn candidate(cfg: &Config, c: &Connection, store: &Store) -> Result<Option<String>, String> {
    if let Some(name) = &c.api_key_env
        && let Ok(k) = std::env::var(name)
        && !k.trim().is_empty()
    {
        return Ok(Some(k));
    }
    if let Some(id) = &c.credential {
        return store
            .credentials
            .get(id)
            .map(|x| Some(x.access_token.clone()))
            .ok_or("Stored credentials are missing; reconnect".into());
    }
    if let Some(name) = &cfg.api_key_env {
        return Err(format!("{name} is not configured"));
    }
    Ok(None)
}
fn remember(store: &Store, c: &Connection) -> Result<(), String> {
    auth::edit(|fresh| {
        if let Some(id) = &c.credential
            && let Some(cred) = store.credentials.get(id)
        {
            fresh.credentials.insert(id.clone(), cred.clone());
        }
        if let Some(row) = fresh.connections.iter_mut().find(|x| x.id == c.id) {
            *row = c.clone();
        } else {
            fresh.connections.push(c.clone());
        }
        Ok(())
    })
}
fn commit(
    cfg: &mut Config,
    store: &mut Store,
    c: Connection,
    replace_credential: bool,
) -> Result<(), String> {
    auth::edit(|fresh| {
        if replace_credential
            && let Some(id) = &c.credential
            && let Some(cred) = store.credentials.get(id)
        {
            fresh.credentials.insert(id.clone(), cred.clone());
        }
        if let Some(row) = fresh.connections.iter_mut().find(|x| x.id == c.id) {
            *row = c.clone();
        } else {
            fresh.connections.push(c.clone());
        }
        fresh.active = Some(c.id.clone());
        Ok(())
    })?;
    if cfg.provider != c.provider || cfg.base_url != c.base_url {
        cfg.pricing = None;
    }
    auth::configure(cfg, &c);
    *store = auth::load()?;
    Ok(())
}

fn setup(
    keys: &KeyReader,
    cfg: &mut Config,
    store: &mut Store,
    preset: Option<usize>,
) -> Result<bool, String> {
    let mut c = if let Some(i) = preset {
        let p = &PRESETS[i];
        Connection {
            id: auth::random(12)?,
            label: p.label.into(),
            provider: p.provider.into(),
            base_url: p.url.into(),
            model: String::new(),
            api_key_env: (!p.env.is_empty()).then(|| p.env.into()),
            credential: None,
        }
    } else {
        let Some(label) = modal::field(
            keys,
            "Custom provider",
            "Connection name",
            "Custom provider",
            false,
        ) else {
            return Ok(false);
        };
        let choices = vec![
            "OpenAI Chat Completions".into(),
            "OpenAI Responses".into(),
            "Anthropic Messages".into(),
        ];
        let Some(i) = modal::choose(keys, "Custom provider", "Choose the wire API", &choices)
        else {
            return Ok(false);
        };
        let Some(url) = modal::validated_field(
            keys,
            "Custom provider",
            "Base URL (include /v1 for OpenAI APIs)",
            if i == 2 {
                "http://127.0.0.1:8080"
            } else {
                "http://127.0.0.1:8080/v1"
            },
            false,
            |s| crate::provider::validate_endpoint(s).map_err(|e| e.to_string()),
        ) else {
            return Ok(false);
        };
        Connection {
            id: auth::random(12)?,
            label,
            provider: ["openai", "openai-responses", "anthropic"][i].into(),
            base_url: url,
            model: String::new(),
            api_key_env: None,
            credential: None,
        }
    };
    let oauth = preset == Some(0);
    if oauth {
        let Some(cred) = auth::chatgpt(keys, store, None)? else {
            return Ok(false);
        };
        c.label = format!(
            "ChatGPT · {} · {}",
            cred.email,
            cred.client_id
                .chars()
                .rev()
                .take(6)
                .collect::<String>()
                .chars()
                .rev()
                .collect::<String>()
        );
        c.credential = Some(c.id.clone());
        store.credentials.insert(c.id.clone(), cred);
        remember(store, &c)?;
    } else {
        let env = c
            .api_key_env
            .as_ref()
            .filter(|name| std::env::var(name).is_ok_and(|s| !s.trim().is_empty()))
            .cloned();
        let mut methods = vec!["Enter API key (stored privately)".into()];
        if let Some(name) = &env {
            methods.insert(0, format!("Use {name} from environment"));
        }
        if c.api_key_env.is_none() {
            methods.push("No key — local/custom endpoint".into());
        }
        let Some(method) = modal::choose(
            keys,
            &format!("Connect {}", c.label),
            "Choose authentication",
            &methods,
        ) else {
            return Ok(false);
        };
        let use_env = env.is_some() && method == 0;
        let no_key = c.api_key_env.is_none() && method == methods.len() - 1;
        if use_env {
            c.api_key_env = env;
        } else if no_key {
            c.api_key_env = None;
        } else {
            let Some(key) = modal::field(
                keys,
                &format!("Connect {}", c.label),
                "API key · hidden · saved only in glass-core/auth.json",
                "",
                true,
            ) else {
                return Ok(false);
            };
            c.api_key_env = None;
            c.credential = Some(c.id.clone());
            store.credentials.insert(
                c.id.clone(),
                Credential {
                    access_token: key,
                    refresh_token: String::new(),
                    expires_at: 0,
                    client_id: String::new(),
                    subject: String::new(),
                    email: String::new(),
                    id_token: String::new(),
                    scopes: Vec::new(),
                },
            );
        }
    }
    let mut next = cfg.clone();
    auth::configure(&mut next, &c);
    let key = candidate(&next, &c, store)?;
    let Some(model) = model_for(keys, &next, key) else {
        return Ok(false);
    };
    c.model = model;
    commit(cfg, store, c, true)?;
    Ok(true)
}
pub fn login(keys: &KeyReader, cfg: &mut Config) -> Result<bool, String> {
    let mut store = auth::load()?;
    let mut items: Vec<_> = store
        .connections
        .iter()
        .map(|c| format!("Reconnect {}", c.label))
        .collect();
    let saved = items.len();
    items.extend(PRESETS.iter().map(|p| p.label.into()));
    items.push("Custom provider…".into());
    let Some(idx) = modal::choose(
        keys,
        "Connect a provider",
        "Choose an account or add a connection",
        &items,
    ) else {
        return Ok(false);
    };
    if idx < saved {
        let mut c = store.connections[idx].clone();
        let existing = c
            .credential
            .as_ref()
            .and_then(|id| store.credentials.get(id))
            .cloned();
        if let Some(cred) = existing.filter(|c| !c.client_id.is_empty()) {
            let Some(cred) = auth::chatgpt(keys, &mut store, Some(cred))? else {
                return Ok(false);
            };
            store
                .credentials
                .insert(c.credential.clone().unwrap(), cred);
            remember(&store, &c)?;
        } else {
            let Some(key) =
                modal::field(keys, "Reconnect provider", "New API key · hidden", "", true)
            else {
                return Ok(false);
            };
            let id = c.credential.clone().unwrap_or_else(|| c.id.clone());
            store.credentials.insert(
                id.clone(),
                Credential {
                    access_token: key,
                    refresh_token: String::new(),
                    expires_at: 0,
                    client_id: String::new(),
                    subject: String::new(),
                    email: String::new(),
                    id_token: String::new(),
                    scopes: Vec::new(),
                },
            );
            c.credential = Some(id);
            c.api_key_env = None;
        }
        let mut next = cfg.clone();
        auth::configure(&mut next, &c);
        let key = candidate(&next, &c, &store)?;
        let Some(model) = model_for(keys, &next, key) else {
            return Ok(false);
        };
        c.model = model;
        commit(cfg, &mut store, c, true)?;
        Ok(true)
    } else {
        let i = idx - saved;
        setup(keys, cfg, &mut store, (i < PRESETS.len()).then_some(i))
    }
}
pub fn model(keys: &KeyReader, cfg: &mut Config) -> Result<bool, String> {
    let mut store = auth::load()?;
    let mut connections = store.connections.clone();
    if auth::current(&store, cfg).is_none() {
        connections.insert(
            0,
            Connection {
                id: auth::random(12)?,
                label: format!("Configured {}", cfg.provider),
                provider: cfg.provider.clone(),
                base_url: cfg.base_url.clone(),
                model: cfg.model.clone(),
                api_key_env: cfg.api_key_env.clone(),
                credential: None,
            },
        );
    }
    let mut items: Vec<_> = connections
        .iter()
        .map(|c| {
            format!(
                "{}{} · {}",
                if (cfg.connection.as_ref() == Some(&c.id) || cfg.connection.is_none())
                    && c.provider == cfg.provider
                    && c.base_url == cfg.base_url
                    && c.model == cfg.model
                {
                    "✓ "
                } else {
                    ""
                },
                c.label,
                c.model
            )
        })
        .collect();
    items.push("Add provider or account…".into());
    let Some(i) = modal::choose(
        keys,
        "Provider and model",
        "Choose a provider/account, then its available model",
        &items,
    ) else {
        return Ok(false);
    };
    if i == connections.len() {
        return login(keys, cfg);
    }
    let mut c = connections[i].clone();
    let mut next = cfg.clone();
    auth::configure(&mut next, &c);
    let key = if let Some(id) = &c.credential {
        auth::resolve_credential(id).map(Some)
    } else {
        auth::resolve(&next)
    };
    let key = match key {
        Ok(k) => k,
        Err(e) => {
            modal::info(keys, "Connection needs setup", &e);
            return login(keys, cfg);
        }
    };
    let Some(model) = model_for(keys, &next, key) else {
        return Ok(false);
    };
    c.model = model;
    commit(cfg, &mut store, c, false)?;
    Ok(true)
}
pub fn logout(keys: &KeyReader, cfg: &mut Config) -> Result<bool, String> {
    let store = auth::load()?;
    if store.connections.is_empty() {
        modal::info(
            keys,
            "Sign out",
            "No saved connections. Environment credentials are managed by your shell.",
        );
        return Ok(false);
    }
    let items = store
        .connections
        .iter()
        .map(|c| c.label.clone())
        .collect::<Vec<_>>();
    let Some(i) = modal::choose(
        keys,
        "Sign out",
        "Choose a connection to remove from this device",
        &items,
    ) else {
        return Ok(false);
    };
    let c = store.connections[i].clone();
    let Some(action) = modal::choose(
        keys,
        "Remove connection?",
        &c.label,
        &[
            "Cancel".into(),
            "Remove saved connection and credentials".into(),
        ],
    ) else {
        return Ok(false);
    };
    if action == 0 {
        return Ok(false);
    }
    let active = cfg.connection.as_ref() == Some(&c.id);
    auth::edit(|fresh| {
        fresh.connections.retain(|x| x.id != c.id);
        if let Some(id) = &c.credential {
            fresh.credentials.remove(id);
        }
        if fresh.active.as_ref() == Some(&c.id) {
            fresh.active = None;
        }
        Ok(())
    })?;
    if active {
        cfg.connection = None;
        cfg.api_key_env = Some("GLASS_CORE_SIGNED_OUT".into());
    }
    Ok(active)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn chatgpt_catalog_keeps_server_order_and_visibility() {
        let models = parse_models(
            &json!({"models":[{"slug":"second","display_name":"Second","visibility":"list"},{"slug":"hidden","visibility":"hide"},{"slug":"first","display_name":"First","visibility":"list"}]}),
        );
        assert_eq!(
            models.iter().map(|m| m.id.as_str()).collect::<Vec<_>>(),
            ["second", "first"]
        );
        assert_eq!(models[0].label, "Second");
    }
    #[test]
    fn api_key_catalog_uses_ids_and_display_names() {
        let m = parse_models(&json!({"data":[{"id":"a","display_name":"A"},{"id":"b"}]}));
        assert_eq!(m[0].label, "A");
        assert_eq!(m[1].label, "b");
    }
}
