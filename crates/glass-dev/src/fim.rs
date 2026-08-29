//! Configured fill-in-the-middle completions for editor ghosts.
//!
//! Local identifier/LSP/`TODO` fallbacks stay available. A provider is used
//! only when `editor.fim` or `GLASS_FIM_ENDPOINT` is set. `stub://` is for
//! tests and never opens a socket.

use crate::customization::EditorConfig;
use serde_json::{Value, json};
use std::env;
use std::time::Duration;

const MAX_PREFIX_BYTES: usize = 6 * 1024;
const MAX_SUFFIX_BYTES: usize = 2 * 1024;
const MAX_GHOST_BYTES: usize = 256;
const DEFAULT_MAX_TOKENS: u32 = 64;
const REQUEST_TIMEOUT: Duration = Duration::from_millis(800);

/// Resolved FIM provider. Clone so a worker thread can own a copy.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FimProvider {
    pub endpoint: String,
    pub model: String,
    pub api_key: Option<String>,
    pub max_tokens: u32,
}

impl FimProvider {
    pub fn from_editor(editor: &EditorConfig) -> Option<Self> {
        let endpoint = editor
            .fim
            .endpoint
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
            .or_else(|| env_nonempty("GLASS_FIM_ENDPOINT"))?;
        let model = editor
            .fim
            .model
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
            .or_else(|| env_nonempty("GLASS_FIM_MODEL"))
            .unwrap_or_else(|| "fim".into());
        let key_env = editor
            .fim
            .api_key_env
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or("GLASS_FIM_API_KEY");
        let api_key = env_nonempty(key_env)
            .or_else(|| env_nonempty("GLASS_FIM_API_KEY"))
            .or_else(|| env_nonempty("XAI_API_KEY"))
            .or_else(|| env_nonempty("OPENAI_API_KEY"));
        if !endpoint.starts_with("stub:") && api_key.is_none() {
            return None;
        }
        Some(Self {
            endpoint,
            model,
            api_key,
            max_tokens: editor
                .fim
                .max_tokens
                .unwrap_or(DEFAULT_MAX_TOKENS)
                .clamp(8, 256),
        })
    }

    pub fn complete(&self, prefix: &str, suffix: &str) -> Result<String, String> {
        let prefix = trim_prefix(prefix);
        let suffix = trim_suffix(suffix);
        if self.endpoint.starts_with("stub:") {
            return Ok(bound_ghost(&stub_complete(prefix, suffix)));
        }
        let body = json!({
            "model": self.model,
            "prompt": prefix,
            "suffix": suffix,
            "max_tokens": self.max_tokens,
            "temperature": 0.0,
            "stop": ["\n\n"],
        });
        let mut request = reqwest::blocking::Client::builder()
            .timeout(REQUEST_TIMEOUT)
            .build()
            .map_err(|error| error.to_string())?
            .post(&self.endpoint)
            .header("content-type", "application/json");
        if let Some(key) = &self.api_key {
            request = request.bearer_auth(key);
        }
        let response = request
            .json(&body)
            .send()
            .map_err(|error| error.to_string())?;
        if !response.status().is_success() {
            return Err(format!("FIM provider returned {}", response.status()));
        }
        let value = response
            .json::<Value>()
            .map_err(|error| error.to_string())?;
        let text = parse_fim_text(&value).ok_or_else(|| "FIM response had no text".to_string())?;
        Ok(bound_ghost(&text))
    }
}

pub fn parse_fim_text(value: &Value) -> Option<String> {
    let choice = value
        .pointer("/choices/0")
        .or_else(|| value.get("choices").and_then(Value::as_array)?.first())?;
    let text = choice
        .get("text")
        .and_then(Value::as_str)
        .or_else(|| choice.pointer("/message/content").and_then(Value::as_str))
        .or_else(|| value.get("content").and_then(Value::as_str))?;
    let text = text.trim_end_matches('\0').to_string();
    (!text.trim().is_empty()).then_some(text)
}

fn stub_complete(prefix: &str, suffix: &str) -> String {
    let line = prefix.rsplit('\n').next().unwrap_or(prefix);
    let trimmed = line.trim_end();
    if trimmed.ends_with('(') {
        return "ctx)".into();
    }
    if prefix.ends_with("{\n")
        || prefix.ends_with("{\n    ")
        || trimmed.ends_with('{')
        || line.chars().all(|character| character.is_whitespace())
    {
        if suffix.trim_start().starts_with('}') {
            return "todo!()".into();
        }
        return "todo!()".into();
    }
    "todo!()".into()
}

fn trim_prefix(prefix: &str) -> &str {
    if prefix.len() <= MAX_PREFIX_BYTES {
        return prefix;
    }
    let start = prefix.len() - MAX_PREFIX_BYTES;
    let start = prefix
        .char_indices()
        .find(|(index, _)| *index >= start)
        .map(|(index, _)| index)
        .unwrap_or(start);
    &prefix[start..]
}

fn trim_suffix(suffix: &str) -> &str {
    if suffix.len() <= MAX_SUFFIX_BYTES {
        return suffix;
    }
    let mut end = MAX_SUFFIX_BYTES;
    while end > 0 && !suffix.is_char_boundary(end) {
        end -= 1;
    }
    &suffix[..end]
}

fn bound_ghost(text: &str) -> String {
    let mut end = text.len().min(MAX_GHOST_BYTES);
    while end > 0 && !text.is_char_boundary(end) {
        end -= 1;
    }
    text[..end].to_string()
}

fn env_nonempty(name: &str) -> Option<String> {
    env::var(name)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::customization::{EditorConfig, FimConfig};

    #[test]
    fn stub_provider_fills_an_empty_block() {
        let provider = FimProvider {
            endpoint: "stub://local".into(),
            model: "test".into(),
            api_key: None,
            max_tokens: 32,
        };
        let text = provider
            .complete("fn main() {\n    ", "\n}\n")
            .expect("stub");
        assert_eq!(text, "todo!()");
        assert_eq!(
            provider.complete("fn greet(", ") {}\n").expect("call"),
            "ctx)"
        );
    }

    #[test]
    fn parse_fim_text_reads_openai_and_chat_shapes() {
        assert_eq!(
            parse_fim_text(&json!({"choices":[{"text":" hello"}]})).as_deref(),
            Some(" hello")
        );
        assert_eq!(
            parse_fim_text(&json!({"choices":[{"message":{"content":"world"}}]})).as_deref(),
            Some("world")
        );
        assert!(parse_fim_text(&json!({"choices":[{"text":"   "}]})).is_none());
    }

    #[test]
    fn from_editor_accepts_stub_without_a_key() {
        let stub = EditorConfig {
            fim: FimConfig {
                endpoint: Some("stub://unit".into()),
                model: Some("demo".into()),
                ..FimConfig::default()
            },
            ..EditorConfig::default()
        };
        assert_eq!(
            FimProvider::from_editor(&stub).map(|provider| provider.endpoint),
            Some("stub://unit".into())
        );
    }
}
