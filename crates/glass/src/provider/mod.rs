//! Provider abstraction: one trait, two wire formats.
//!
//! The trait is deliberately tiny. Everything a provider needs beyond these
//! methods arrives in [`Request`]. A new provider is a base URL and a model
//! id, not a new implementation.

mod anthropic;
mod openai;

pub use anthropic::Anthropic;
pub use openai::{Flavour, OpenAiCompat};

use std::io::{self, Read};

use glass_mod_api::Message;

/// Token accounting for one request.
///
/// Providers disagree about what "input tokens" means, so the parts are kept
/// apart and summed only for display. Anthropic charges `cache_read` at 0.1x,
/// `cache_write` at 1.25x, and `input` at 1.0x.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Usage {
    /// Prompt tokens that were neither read from nor written to cache.
    pub input: u32,
    /// Prompt tokens served from an existing cache entry.
    pub cache_read: u32,
    /// Prompt tokens written to cache this request.
    pub cache_write: u32,
    pub output: u32,
}

impl Usage {
    /// Total prompt tokens actually sent, however they were billed.
    pub fn prompt_total(&self) -> u32 {
        self.input + self.cache_read + self.cache_write
    }

    /// Fraction of prompt tokens served from cache, 0.0 to 1.0.
    ///
    /// This is the number that tells you whether the breakpoint layout is
    /// working. A rate that sits near zero means either the prefix is below the
    /// model's minimum or the breakpoint is on a block that changes.
    pub fn hit_rate(&self) -> f32 {
        let total = self.prompt_total();
        if total == 0 {
            return 0.0;
        }
        self.cache_read as f32 / total as f32
    }
}

/// A piece of a streamed model response.
#[derive(Debug, PartialEq, Eq)]
pub enum Event {
    /// Visible assistant text.
    Text(String),
    /// Reasoning content, when the provider exposes it.
    Reasoning(String),
    /// The model wants a tool run.
    ToolCall { id: String, name: String, arguments: String },
    /// Token counts reported by the provider, when available.
    ///
    /// `input` is the *uncached* portion. On Anthropic that is only the
    /// post-breakpoint remainder, so total prompt tokens is
    /// `cache_read + cache_write + input`. Getting this wrong makes every cost
    /// estimate wrong, which is why the fields are separate rather than
    /// pre-summed.
    Usage(Usage),
    /// Terminal event for this response.
    Done,
    /// Non-fatal problem worth surfacing.
    Warning(String),
}

/// Everything needed to produce one model turn.
pub struct Request<'a> {
    /// Model id, for adapters whose endpoint carries it per request rather
    /// than in configuration.
    #[allow(dead_code)]
    pub model: &'a str,
    pub system: &'a str,
    pub messages: &'a [Message],
    /// `(name, description, json_schema)` triples.
    pub tools: &'a [(String, String, String)],
    /// Upper bound on generated tokens.
    pub max_tokens: u32,
    /// Provider-native reasoning budget, when the adapter supports one.
    pub reasoning: Option<u32>,
}

pub trait Provider {
    /// Stable identifier used in config and the status bar.
    fn id(&self) -> &str;

    /// Stream one turn. Implementations must emit `Done` before returning.
    fn stream(&self, req: &Request<'_>) -> Result<Box<dyn Iterator<Item = Event> + '_>, Error>;

    /// Context window in tokens, used to decide when to compact.
    fn context_window(&self) -> u32;

    /// Approximate prompt tokens. Providers with a real count endpoint should
    /// override this; the default is a deliberately conservative estimate.
    fn estimate_tokens(&self, messages: &[Message]) -> u32 {
        default_estimate(messages)
    }
}

/// ~3.6 bytes per token is the usual English-with-code ratio. We round up and
/// add headroom, because under-counting triggers a context overflow while
/// over-counting only compacts slightly early.
pub fn default_estimate(messages: &[Message]) -> u32 {
    let mut chars: usize = 0;
    for m in messages {
        chars += m.text().len();
        chars += 8; // role and framing overhead per message
    }
    let tokens = chars.div_ceil(4);
    (tokens as f64 * 1.15) as u32
}

#[derive(Debug)]
pub enum Error {
    /// Transport failure, with the provider message when one was returned.
    Transport(String),
    /// Provider returned a non-success status.
    Status(u16, String),
    /// Configuration is unusable.
    Config(String),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Transport(m) => write!(f, "transport: {m}"),
            Error::Status(c, m) => write!(f, "http {c}: {m}"),
            Error::Config(m) => write!(f, "config: {m}"),
        }
    }
}

impl std::error::Error for Error {}

/// Read a response body as UTF-8, replacing invalid sequences.
///
/// Model output can contain partial multi-byte characters at chunk edges;
/// decoding lossily keeps the stream alive instead of erroring mid-turn.
pub fn read_lossy(mut r: impl Read) -> io::Result<String> {
    let mut out = Vec::new();
    r.read_to_end(&mut out)?;
    Ok(String::from_utf8_lossy(&out).into_owned())
}

/// Build an HTTP agent with sane timeouts and no automatic error statuses,
/// so callers can read the body of a 429 or 500 for the error message.
pub fn http_agent() -> ureq::Agent {
    ureq::Agent::config_builder()
        .timeout_global(Some(std::time::Duration::from_secs(600)))
        .http_status_as_error(false)
        .build()
        .new_agent()
}

/// Build the provider named in config.
///
/// The dispatch is on a string in config rather than on inferred capability,
/// so adding a provider is a config edit and never a code change.
pub fn build(kind: &str, base_url: &str, api_key: Option<String>, model: &str) -> Result<Box<dyn Provider>, Error> {
    match kind {
        "anthropic" => Ok(Box::new(Anthropic::new(base_url, api_key.unwrap_or_default(), model))),
        "openai" => Ok(Box::new(OpenAiCompat::new(base_url, api_key, model))),
        "openai-responses" => Ok(Box::new(
            OpenAiCompat::new(base_url, api_key, model).with_flavour(Flavour::Responses),
        )),
        other => Err(Error::Config(format!("unknown provider `{other}`"))),
    }
}

/// Tool schemas for the two tools glass ships.
///
/// These are deliberately the entire tool surface. The model can read, write,
/// search, and edit via `bash`, and can durably record its own progress via
/// `state`. Everything else is a shell one-liner away, and every tool we do not
/// ship is schema the model has to read on every request.
pub fn builtin_tools() -> Vec<(String, String, String)> {
    vec![
        (
            "bash".to_string(),
            "Run a shell command in the workspace. Use this for everything: reading files \
             (cat, sed, rg), writing them (cat > f, tee), searching (rg, fd, grep), \
             building, testing, and version control. Set `timeout_ms` for long commands; \
             the default is 120000."
                .to_string(),
            serde_json::json!({
                "type": "object",
                "properties": {
                    "command": { "type": "string", "description": "Shell command to run" },
                    "cwd": { "type": "string", "description": "Working directory, relative to the workspace root" },
                    "timeout_ms": { "type": "integer", "description": "Kill the command after this many milliseconds" }
                },
                "required": ["command"]
            })
            .to_string(),
        ),
        (
            "state".to_string(),
            "Record durable working state so it survives context compaction. Call this \
             BEFORE context fills up, and after any milestone on a long task: list the \
             specific facts, file paths, and next steps you would need to resume cold. \
             Call `state` with no arguments to read back what is currently recorded."
                .to_string(),
            serde_json::json!({
                "type": "object",
                "properties": {
                    "items": {
                        "type": "array",
                        "description": "State items to record. Omit to read current state.",
                        "items": { "type": "string" }
                    }
                }
            })
            .to_string(),
        ),
    ]
}

/// Merge mod-provided tools with the built-ins.
///
/// Mods may add but never remove: a prompt written against the base surface
/// has to keep working when someone enables a mod.
pub fn collect_tools(mods: &[Box<dyn glass_mod_api::Mod_>]) -> Vec<(String, String, String)> {
    let mut tools = builtin_tools();
    for m in mods {
        for (name, desc, schema) in m.tools() {
            if tools.iter().any(|(n, _, _)| *n == name) {
                continue;
            }
            tools.push((name, desc, schema));
        }
    }
    tools
}

/// Count how much of the context window a request would occupy, as a fraction.
pub fn pressure(provider: &dyn Provider, messages: &[glass_mod_api::Message]) -> f32 {
    let window = provider.context_window().max(1);
    provider.estimate_tokens(messages) as f32 / window as f32
}

#[cfg(test)]
mod tests {
    use super::*;
    use glass_mod_api::Message;
    use serde_json::Value;

    #[test]
    fn builtin_surface_is_exactly_two_tools() {
        let tools = builtin_tools();
        assert_eq!(tools.len(), 2);
        assert_eq!(tools[0].0, "bash");
        assert_eq!(tools[1].0, "state");
    }

    #[test]
    fn tool_schemas_are_valid_json_objects() {
        for (_, _, schema) in builtin_tools() {
            let v: Value = serde_json::from_str(&schema).expect("schema must parse");
            assert_eq!(v["type"], "object");
        }
    }

    #[test]
    fn bash_requires_command_but_state_requires_nothing() {
        let tools = builtin_tools();
        let bash: Value = serde_json::from_str(&tools[0].2).unwrap();
        let state: Value = serde_json::from_str(&tools[1].2).unwrap();
        assert!(bash["required"].as_array().unwrap().contains(&Value::String("command".into())));
        // `state` must be callable with zero arguments, or the read-back path
        // the compaction flow depends on becomes awkward to express.
        assert!(state.get("required").is_none());
    }

    #[test]
    fn mods_can_add_but_not_shadow_builtin_tools() {
        struct Greedy;
        impl glass_mod_api::Mod_ for Greedy {
            fn tools(&self) -> Vec<(String, String, String)> {
                vec![
                    ("bash".to_string(), "hijack".to_string(), "{}".to_string()),
                    ("custom".to_string(), "mine".to_string(), "{}".to_string()),
                ]
            }
        }
        let mods: Vec<Box<dyn glass_mod_api::Mod_>> = vec![Box::new(Greedy)];
        let tools = collect_tools(&mods);
        let names: Vec<_> = tools.iter().map(|(n, _, _)| n.as_str()).collect();
        assert_eq!(names, vec!["bash", "state", "custom"]);
    }

    #[test]
    fn unknown_provider_is_a_config_error() {
        match build("mystery", "https://x", None, "m") {
            Err(Error::Config(m)) => assert!(m.contains("mystery")),
            Err(other) => panic!("wrong error: {other:?}"),
            Ok(_) => panic!("expected a config error"),
        }
    }

    #[test]
    fn provider_dispatch_covers_all_three_flavours() {
        assert_eq!(build("anthropic", "u", None, "m").unwrap().id(), "anthropic");
        assert_eq!(build("openai", "u", None, "m").unwrap().id(), "openai");
        assert_eq!(build("openai-responses", "u", None, "m").unwrap().id(), "openai");
    }

    #[test]
    fn pressure_is_a_fraction_between_zero_and_one() {
        let p: Box<dyn Provider> = Box::new(OpenAiCompat::new("u", None, "gpt-5"));
        let msgs: Vec<Message> = (0..50).map(|i| Message::User(format!("message {i}"))).collect();
        let f = pressure(p.as_ref(), &msgs);
        assert!(f > 0.0 && f < 1.0, "{f}");
    }
}
