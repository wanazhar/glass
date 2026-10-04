//! OpenAI-compatible adapter.
//!
//! One wire format reaches GPT-5, Kimi, GLM, Qwen, DeepSeek, Mistral, Gemini's
//! compatibility endpoint, Ollama, vLLM, and LM Studio. A new provider is a
//! base URL and a model id, which is why this adapter is the workhorse and
//! the Anthropic one is the specialist.

use std::io::Read;

use serde_json::{json, Value};

use super::{http_agent, read_lossy, Error, Event, Provider, Request};
use crate::sse::SseDecoder;

const DEFAULT_WINDOW: u32 = 128_000;

pub struct OpenAiCompat {
    pub base_url: String,
    pub api_key: Option<String>,
    pub model: String,
    /// Wire flavour. `/chat/completions` is universal; `/responses` is the
    /// newer API some providers require for reasoning models.
    pub flavour: Flavour,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Flavour {
    ChatCompletions,
    Responses,
}

impl OpenAiCompat {
    pub fn new(base_url: impl Into<String>, api_key: Option<String>, model: impl Into<String>) -> Self {
        Self {
            base_url: base_url.into(),
            api_key,
            model: model.into(),
            flavour: Flavour::ChatCompletions,
        }
    }

    pub fn with_flavour(mut self, flavour: Flavour) -> Self {
        self.flavour = flavour;
        self
    }

    fn endpoint(&self) -> String {
        let base = self.base_url.trim_end_matches('/');
        match self.flavour {
            Flavour::ChatCompletions => format!("{base}/chat/completions"),
            Flavour::Responses => format!("{base}/responses"),
        }
    }

    fn window_for(model: &str) -> u32 {
        let m = model.to_ascii_lowercase();
        // Known large-context families. Order matters: check the 1m tier
        // before the generic tier of the same name.
        if m.contains("1m") {
            return 1_000_000;
        }
        if m.starts_with("gpt-5") || m.starts_with("gpt-4.1") || m.contains("gpt-4o") {
            return 400_000;
        }
        if m.starts_with("gpt-4") {
            return 128_000;
        }
        if m.contains("claude") {
            return 200_000;
        }
        if m.starts_with("gemini") {
            return 1_000_000;
        }
        if m.contains("qwen") || m.contains("glm") || m.contains("kimi") || m.contains("deepseek") {
            return 128_000;
        }
        DEFAULT_WINDOW
    }
}

/// Map the conversation onto OpenAI chat-completions messages.
fn build_messages(system: &str, messages: &[glass_mod_api::Message]) -> Vec<Value> {
    use glass_mod_api::Message;
    let mut out = vec![json!({ "role": "system", "content": system })];
    for m in messages {
        match m {
            Message::User(s) => out.push(json!({ "role": "user", "content": s })),
            Message::Assistant(s) => out.push(json!({ "role": "assistant", "content": s })),
            Message::System(s) => out.push(json!({ "role": "system", "content": s })),
            Message::ToolResult { call_id, name, body } => out.push(json!({
                "role": "tool",
                "tool_call_id": call_id,
                "name": name,
                "content": body
            })),
        }
    }
    out
}

fn build_tools(tools: &[(String, String, String)]) -> Value {
    Value::Array(
        tools
            .iter()
            .map(|(name, desc, schema)| {
                let params: Value = serde_json::from_str(schema).unwrap_or_else(|_| json!({ "type": "object" }));
                json!({
                    "type": "function",
                    "function": { "name": name, "description": desc, "parameters": params }
                })
            })
            .collect(),
    )
}

impl Provider for OpenAiCompat {
    fn id(&self) -> &str {
        "openai"
    }

    fn context_window(&self) -> u32 {
        Self::window_for(&self.model)
    }

    fn stream(&self, req: &Request<'_>) -> Result<Box<dyn Iterator<Item = Event> + '_>, Error> {
        let mut body = match self.flavour {
            Flavour::ChatCompletions => json!({
                "model": self.model,
                "stream": true,
                "stream_options": { "include_usage": true },
                "messages": build_messages(req.system, req.messages),
                "max_tokens": req.max_tokens,
            }),
            Flavour::Responses => json!({
                "model": self.model,
                "stream": true,
                "instructions": req.system,
                "input": build_messages("", req.messages),
                "max_output_tokens": req.max_tokens,
            }),
        };

        if !req.tools.is_empty() {
            let tools = build_tools(req.tools);
            body["tools"] = if self.flavour == Flavour::Responses {
                tools
            } else {
                json!([tools])
            };
        }

        // Reasoning budget is vendor-specific even inside this format. GPT-5
        // takes a verbosity hint, o-series and most Chinese providers take
        // `thinking`, and the rest ignore both. Passing an unknown field is
        // usually tolerated, but not universally, so we gate on the model id.
        if let Some(budget) = req.reasoning {
            let m = self.model.to_ascii_lowercase();
            if m.starts_with("gpt-5") {
                body["reasoning_effort"] = json!(effort_label(budget));
            } else if m.starts_with("o1") || m.starts_with("o3") || m.starts_with("o4") {
                body["reasoning_effort"] = json!(effort_label(budget));
            } else if m.contains("glm") || m.contains("qwen3") || m.contains("deepseek") {
                body["thinking"] = json!({ "type": "enabled", "budget_tokens": budget });
            }
        }

        let mut rb = http_agent().post(self.endpoint());
        if let Some(k) = &self.api_key {
            rb = rb.header("authorization", format!("Bearer {k}"));
        }
        let resp = rb.send_json(&body).map_err(map_transport)?;
        let status = resp.status().as_u16();
        let mut reader = resp.into_body().into_reader();
        if !(200..300).contains(&status) {
            let text = read_lossy(&mut reader).unwrap_or_default();
            return Err(Error::Status(status, extract_error(&text)));
        }

        Ok(Box::new(StreamEvents::new(reader)))
    }
}

fn effort_label(budget: u32) -> &'static str {
    match budget {
        0..=2_000 => "low",
        2_001..=16_000 => "medium",
        _ => "high",
    }
}

fn map_transport(e: ureq::Error) -> Error {
    match e {
        ureq::Error::StatusCode(code) => Error::Status(code, String::new()),
        other => Error::Transport(other.to_string()),
    }
}

fn extract_error(body: &str) -> String {
    serde_json::from_str::<Value>(body)
        .ok()
        .and_then(|v| {
            v.get("error")
                .and_then(|e| e.get("message"))
                .and_then(Value::as_str)
                .map(str::to_string)
                .or_else(|| v.get("message").and_then(Value::as_str).map(str::to_string))
        })
        .unwrap_or_else(|| body.chars().take(400).collect())
}

/// Everything accumulated across the stream for a single response.
#[derive(Default)]
struct Accum {
    text: String,
    reasoning: String,
    tools: std::collections::BTreeMap<usize, Partial>,
    usage: Option<(u32, u32)>,
    /// Guards `seal` against running twice.
    sealed: bool,
}

/// One tool call, reconstructed from however many deltas it spanned.
#[derive(Default)]
struct Partial {
    id: String,
    name: String,
    args: String,
}

/// Decodes chat-completions SSE.
///
/// The wire shape is awkward for tools: `tool_calls` arrive as an array whose
/// elements may be split across several deltas, indexed by position, with the
/// function name sometimes absent and only its prefix present. So arguments
/// accumulate per index and are only emitted once the stream ends.
struct StreamEvents<R: Read> {
    reader: R,
    decoder: SseDecoder,
    done: bool,
    acc: Accum,
    /// Events produced but not yet handed to the caller. Tool calls are only
    /// known once the stream ends, so they queue behind the text deltas.
    outbox: std::collections::VecDeque<Event>,
}

impl<R: Read> StreamEvents<R> {
    fn new(reader: R) -> Self {
        Self {
            reader,
            decoder: SseDecoder::new(),
            done: false,
            acc: Accum::default(),
            outbox: std::collections::VecDeque::new(),
        }
    }

    /// Move everything accumulated so far into the outbox, in the order the
    /// caller expects: usage, then tool calls by index, then Done.
    fn seal(&mut self) {
        if self.acc.sealed {
            return;
        }
        self.acc.sealed = true;

        if let Some((input, output)) = self.acc.usage.take() {
            self.outbox.push_back(Event::Usage { input, output });
        }
        let calls: Vec<_> = std::mem::take(&mut self.acc.tools)
            .into_iter()
            .filter(|(_, p)| !p.name.is_empty())
            .map(|(_, p)| Event::ToolCall { id: p.id, name: p.name, arguments: p.args })
            .collect();
        self.outbox.extend(calls);
        self.outbox.push_back(Event::Done);
    }

    /// Drain one buffered event, refilling from the socket when empty.
    fn next_event(&mut self) -> Option<Event> {
        loop {
            if let Some(e) = self.outbox.pop_front() {
                return Some(e);
            }
            if self.done {
                self.seal();
                continue;
            }

            let mut chunk = [0u8; 8192];
            let n = match self.reader.read(&mut chunk) {
                Ok(0) | Err(_) => {
                    self.done = true;
                    // A stream cut without `[DONE]` still has usable content;
                    // sealing flushes whatever we accumulated.
                    if let Some(ev) = self.decoder.finish()
                        && let Ok(v) = serde_json::from_str::<Value>(&ev.data)
                    {
                        self.consume(&v);
                    }
                    continue;
                }
                Ok(n) => n,
            };

            let text = String::from_utf8_lossy(&chunk[..n]);
            for ev in self.decoder.push(&text) {
                if ev.data.trim() == "[DONE]" {
                    self.done = true;
                    break;
                }
                let Ok(v) = serde_json::from_str::<Value>(&ev.data) else { continue };
                self.consume(&v);
            }
        }
    }

    /// Fold one delta object into the accumulator.
    fn consume(&mut self, v: &Value) {
        if let Some(err) = v.get("error").and_then(|e| e.get("message")).and_then(Value::as_str) {
            self.outbox.push_back(Event::Warning(err.to_string()));
            self.done = true;
            return;
        }

        let Some(choices) = v.get("choices").and_then(Value::as_array) else {
            // Usage-only chunks carry no choices.
            self.read_usage(v);
            return;
        };
        let Some(choice) = choices.first() else {
            self.read_usage(v);
            return;
        };
        let delta = choice.get("delta").or_else(|| choice.get("message"));

        if let Some(t) = delta
            .and_then(|d| d.get("reasoning_content").and_then(Value::as_str))
            .or_else(|| delta.and_then(|d| d.get("reasoning")).and_then(Value::as_str))
            && !t.is_empty()
        {
            self.acc.reasoning.push_str(t);
        }

        if let Some(t) = delta.and_then(|d| d.get("content")).and_then(Value::as_str)
            && !t.is_empty()
        {
            self.acc.text.push_str(t);
            self.outbox.push_back(Event::Text(t.to_string()));
        }

        if let Some(calls) = delta.and_then(|d| d.get("tool_calls")).and_then(Value::as_array) {
            for c in calls {
                let idx = c.get("index").and_then(Value::as_u64).unwrap_or(0) as usize;
                let slot = self.acc.tools.entry(idx).or_default();
                if let Some(id) = c.get("id").and_then(Value::as_str)
                    && !id.is_empty()
                {
                    slot.id.push_str(id);
                }
                let func = c.get("function");
                if let Some(name) = func.and_then(|f| f.get("name")).and_then(Value::as_str)
                    && !name.is_empty()
                {
                    slot.name.push_str(name);
                }
                if let Some(args) = func.and_then(|f| f.get("arguments")).and_then(Value::as_str) {
                    slot.args.push_str(args);
                }
            }
        }

        self.read_usage(v);
    }

    fn read_usage(&mut self, v: &Value) {
        if let Some(usage) = v.get("usage").filter(|u| !u.is_null()) {
            let input = usage.get("prompt_tokens").and_then(Value::as_u64).unwrap_or(0) as u32;
            let output = usage.get("completion_tokens").and_then(Value::as_u64).unwrap_or(0) as u32;
            if input > 0 || output > 0 {
                self.acc.usage = Some((input, output));
            }
        }
    }
}

impl<R: Read> Iterator for StreamEvents<R> {
    type Item = Event;

    fn next(&mut self) -> Option<Event> {
        self.next_event()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use glass_mod_api::Message;

    #[test]
    fn window_by_family() {
        assert_eq!(OpenAiCompat::window_for("gpt-5"), 400_000);
        assert_eq!(OpenAiCompat::window_for("gpt-4-turbo"), 128_000);
        assert_eq!(OpenAiCompat::window_for("gemini-3-pro"), 1_000_000);
        assert_eq!(OpenAiCompat::window_for("glm-4.6"), 128_000);
        assert_eq!(OpenAiCompat::window_for("something-unknown"), DEFAULT_WINDOW);
    }

    #[test]
    fn one_m_suffix_wins_over_family_default() {
        // "1m" must be checked before the gemini/gpt families so a
        // gpt-5-1m style id is not silently capped at 400k.
        assert_eq!(OpenAiCompat::window_for("gpt-5-1m"), 1_000_000);
    }

    #[test]
    fn endpoint_trims_trailing_slash() {
        let p = OpenAiCompat::new("https://api.example.com/", None, "m");
        assert_eq!(p.endpoint(), "https://api.example.com/chat/completions");
        let r = p.with_flavour(Flavour::Responses);
        assert_eq!(r.endpoint(), "https://api.example.com/responses");
    }

    #[test]
    fn tool_result_becomes_tool_role() {
        let msgs = vec![Message::ToolResult { call_id: "call_1".into(), name: "bash".into(), body: "ok".into() }];
        let out = build_messages("sys", &msgs);
        assert_eq!(out[0]["role"], "system");
        assert_eq!(out[1]["role"], "tool");
        assert_eq!(out[1]["tool_call_id"], "call_1");
    }

    #[test]
    fn tools_use_function_wrapper() {
        let tools = vec![("bash".to_string(), "run".to_string(), r#"{"type":"object"}"#.to_string())];
        let v = build_tools(&tools);
        assert_eq!(v[0]["type"], "function");
        assert_eq!(v[0]["function"]["name"], "bash");
    }

    #[test]
    fn effort_label_buckets() {
        assert_eq!(effort_label(1_000), "low");
        assert_eq!(effort_label(8_000), "medium");
        assert_eq!(effort_label(32_000), "high");
    }

    #[test]
    fn error_body_is_unwrapped() {
        let body = r#"{"error":{"message":"rate limit reached","type":"rate_limit"}}"#;
        assert_eq!(extract_error(body), "rate limit reached");
    }
}
