//! Fill-in-the-middle ghosts served by a resident Pi AgentSession.
//!
//! Local identifier/LSP/`TODO` fallbacks stay available. A `stub://` endpoint
//! is for tests and never starts Pi. Otherwise Glass queues `complete` on a
//! resident Pi session so ghosts use the same SDK, auth, and model as the
//! factory — not a second HTTP client.

use crate::customization::EditorConfig;
use serde_json::Value;
use std::env;

const MAX_PREFIX_BYTES: usize = 6 * 1024;
const MAX_SUFFIX_BYTES: usize = 2 * 1024;
const MAX_GHOST_BYTES: usize = 256;

/// Who supplies the ghost fill.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FimBackend {
    Stub,
    Pi,
}

/// Resolved FIM provider.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FimProvider {
    pub backend: FimBackend,
    pub model: Option<String>,
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
            .or_else(|| env_nonempty("GLASS_FIM_ENDPOINT"));
        if endpoint
            .as_deref()
            .is_some_and(|value| matches!(value, "off" | "none" | "disable"))
        {
            return None;
        }
        if endpoint
            .as_deref()
            .is_some_and(|value| value.starts_with("stub:"))
        {
            return Some(Self {
                backend: FimBackend::Stub,
                model: None,
            });
        }
        Some(Self {
            backend: FimBackend::Pi,
            model: editor
                .fim
                .model
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string)
                .or_else(|| env_nonempty("GLASS_FIM_MODEL")),
        })
    }

    pub fn complete(&self, prefix: &str, suffix: &str) -> Result<String, String> {
        let prefix = trim_prefix(prefix);
        let suffix = trim_suffix(suffix);
        match self.backend {
            FimBackend::Stub => Ok(bound_ghost(&stub_complete(prefix, suffix))),
            FimBackend::Pi => {
                Err("Pi FIM is served by a resident AgentSession complete operation".into())
            }
        }
    }
}

pub fn parse_fim_text(value: &Value) -> Option<String> {
    let text = value
        .pointer("/result/text")
        .and_then(Value::as_str)
        .or_else(|| value.get("text").and_then(Value::as_str))
        .or_else(|| value.pointer("/choices/0/text").and_then(Value::as_str))?;
    let text = text.trim_end_matches('\0').to_string();
    (!text.trim().is_empty()).then_some(text)
}

fn stub_complete(prefix: &str, suffix: &str) -> String {
    let _ = suffix;
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
    use serde_json::json;

    #[test]
    fn stub_provider_fills_an_empty_block() {
        let provider = FimProvider {
            backend: FimBackend::Stub,
            model: None,
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
    fn parse_fim_text_reads_pi_complete_and_legacy_shapes() {
        assert_eq!(
            parse_fim_text(&json!({"result":{"text":" hello"}})).as_deref(),
            Some(" hello")
        );
        assert_eq!(
            parse_fim_text(&json!({"choices":[{"text":"world"}]})).as_deref(),
            Some("world")
        );
        assert!(parse_fim_text(&json!({"result":{"text":"   "}})).is_none());
    }

    #[test]
    fn from_editor_uses_stub_or_resident_pi() {
        let stub = EditorConfig {
            fim: FimConfig {
                endpoint: Some("stub://unit".into()),
                model: Some("demo".into()),
                ..FimConfig::default()
            },
            ..EditorConfig::default()
        };
        assert_eq!(
            FimProvider::from_editor(&stub).map(|provider| provider.backend),
            Some(FimBackend::Stub)
        );
        let pi = EditorConfig::default();
        assert_eq!(
            FimProvider::from_editor(&pi).map(|provider| provider.backend),
            Some(FimBackend::Pi)
        );
        let off = EditorConfig {
            fim: FimConfig {
                endpoint: Some("off".into()),
                ..FimConfig::default()
            },
            ..EditorConfig::default()
        };
        assert!(FimProvider::from_editor(&off).is_none());
    }
}
