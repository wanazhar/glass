//! Anthropic Messages API adapter.
//!
//! Exists separately from the OpenAI-compatible adapter because two things
//! only exist on the native endpoint: `thinking` blocks and `cache_control`
//! breakpoints. Prompt caching is a measurable latency and cost win on long
//! sessions, so it is worth a second adapter rather than losing it.

use std::io::Read;

use serde_json::{json, Value};

use super::{http_agent, read_lossy, Error, Event, Provider, Request};
use crate::sse::SseDecoder;

/// Default context window for models we do not recognise explicitly.
const DEFAULT_WINDOW: u32 = 200_000;

/// Cache the system prompt and the first user message, which together hold the
/// stable prefix. The breakpoint goes on the last block of the prefix so the
/// whole prefix is reused on every turn.
fn with_cache(mut block: Value, last: bool) -> Value {
    if last {
        block["cache_control"] = json!({ "type": "ephemeral" });
    }
    block
}

pub struct Anthropic {
    pub base_url: String,
    pub api_key: String,
    pub model: String,
}

impl Anthropic {
    pub fn new(base_url: impl Into<String>, api_key: impl Into<String>, model: impl Into<String>) -> Self {
        Self { base_url: base_url.into(), api_key: api_key.into(), model: model.into() }
    }

    fn window_for(model: &str) -> u32 {
        if model.contains("claude-") {
            // The 1M variants are opt-in on the model id itself.
            if model.contains("[1m]") || model.contains("-1m") {
                return 1_000_000;
            }
            return 200_000;
        }
        DEFAULT_WINDOW
    }

    /// Map a conversation onto the Anthropic message shape.
    ///
    /// Two invariants matter: tool results must follow the assistant turn that
    /// requested them, and consecutive tool results belong in one user turn.
    /// Violating either produces a 400 from the API, so the grouping here is
    /// load-bearing rather than cosmetic.
    fn build_messages(messages: &[glass_mod_api::Message]) -> Vec<Value> {
        use glass_mod_api::Message;
        let mut out: Vec<Value> = Vec::new();

        for m in messages {
            match m {
                Message::User(s) => {
                    out.push(json!({ "role": "user", "content": [{ "type": "text", "text": s }] }));
                }
                Message::Assistant(s) => {
                    out.push(json!({ "role": "assistant", "content": [{ "type": "text", "text": s }] }));
                }
                Message::System(s) => {
                    out.push(json!({ "role": "user", "content": [{ "type": "text", "text": s }] }));
                }
                Message::ToolResult { call_id, body, .. } => {
                    let block = json!({
                        "type": "tool_result",
                        "tool_use_id": call_id,
                        "content": [{ "type": "text", "text": body }]
                    });
                    // Merge into the previous user turn if it is already a
                    // tool-result carrier, otherwise start a new one.
                    let merge = out.last().is_some_and(is_tool_result_turn);
                    if merge {
                        let last = out.last_mut().expect("checked non-empty");
                        last["content"].as_array_mut().expect("array").push(block);
                    } else {
                        out.push(json!({ "role": "user", "content": [block] }));
                    }
                }
            }
        }
        out
    }
}

/// True when the last message is a user turn carrying a tool result.
fn is_tool_result_turn(v: &Value) -> bool {
    v.get("role").and_then(Value::as_str) == Some("user")
        && v.get("content")
            .and_then(Value::as_array)
            .map(|blocks| blocks.iter().any(|b| b.get("type").and_then(Value::as_str) == Some("tool_result")))
            .unwrap_or(false)
}

fn build_tools(tools: &[(String, String, String)]) -> Vec<Value> {
    tools
        .iter()
        .map(|(name, desc, schema)| {
            let parsed: Value = serde_json::from_str(schema).unwrap_or_else(|_| json!({ "type": "object" }));
            json!({
                "name": name,
                "description": desc,
                "input_schema": parsed
            })
        })
        .collect()
}

impl Provider for Anthropic {
    fn id(&self) -> &str {
        "anthropic"
    }

    fn context_window(&self) -> u32 {
        Self::window_for(&self.model)
    }

    fn stream(&self, req: &Request<'_>) -> Result<Box<dyn Iterator<Item = Event> + '_>, Error> {
        let system = with_cache(
            json!([{ "type": "text", "text": req.system, "cache_control": { "type": "ephemeral" } }]),
            true,
        );

        let mut body = json!({
            "model": self.model,
            "max_tokens": req.max_tokens,
            "stream": true,
            "system": system,
            "messages": Self::build_messages(req.messages),
        });

        if !req.tools.is_empty() {
            body["tools"] = Value::Array(build_tools(req.tools));
        }

        if let Some(budget) = req.reasoning {
            // Anthropic requires budget < max_tokens, so give it half the room
            // and leave the remainder for visible output.
            let effective = budget.min(req.max_tokens.saturating_sub(1));
            if effective > 0 {
                body["thinking"] = json!({ "type": "enabled", "budget_tokens": effective });
                body["max_tokens"] = json!(req.max_tokens.max(effective + 1024));
            }
        }

        let resp = http_agent()
            .post(format!("{}/v1/messages", self.base_url.trim_end_matches('/')))
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", "2023-06-01")
            .send_json(&body)
            .map_err(map_transport)?;
        let status = resp.status().as_u16();
        let mut reader = resp.into_body().into_reader();
        if !(200..300).contains(&status) {
            let text = read_lossy(&mut reader).unwrap_or_default();
            return Err(Error::Status(status, extract_error(&text)));
        }

        Ok(Box::new(StreamEvents::new(reader)))
    }
}

fn map_transport(e: ureq::Error) -> Error {
    match e {
        ureq::Error::StatusCode(code) => Error::Status(code, String::new()),
        other => Error::Transport(other.to_string()),
    }
}

/// Pull the human-readable message out of an Anthropic error body.
fn extract_error(body: &str) -> String {
    serde_json::from_str::<Value>(body)
        .ok()
        .and_then(|v| {
            v.get("error")
                .and_then(|e| e.get("message"))
                .and_then(Value::as_str)
                .map(str::to_string)
        })
        .unwrap_or_else(|| body.chars().take(400).collect())
}

/// Turns the SSE byte stream into [`Event`]s.
///
/// Anthropic streams `content_block_start` with the full tool input skeleton
/// and then emits incremental `input_json_delta` fragments, so partial
/// arguments have to be accumulated across events and paired by block index.
struct StreamEvents<R: Read> {
    reader: R,
    decoder: SseDecoder,
    pending_tool: Option<(usize, String, String)>,
    input_fragments: Vec<(usize, String)>,
    done: bool,
}

impl<R: Read> StreamEvents<R> {
    fn new(reader: R) -> Self {
        Self { reader, decoder: SseDecoder::new(), pending_tool: None, input_fragments: Vec::new(), done: false }
    }
}

impl<R: Read> Iterator for StreamEvents<R> {
    type Item = Event;

    fn next(&mut self) -> Option<Event> {
        loop {
            if self.done {
                return None;
            }

            let mut chunk = [0u8; 8192];
            let n = match self.reader.read(&mut chunk) {
                Ok(0) => {
                    self.done = true;
                    if let Some(ev) = self.decoder.finish() {
                        // Tail without a blank line still carries data.
                        let text = ev.data;
                        if !text.trim().is_empty() {
                            return Some(Event::Warning(format!("truncated stream: {}", &text[..text.len().min(80)])));
                        }
                    }
                    return Some(Event::Done);
                }
                Ok(n) => n,
                Err(_) => {
                    self.done = true;
                    return Some(Event::Done);
                }
            };

            let text = String::from_utf8_lossy(&chunk[..n]);
            for ev in self.decoder.push(&text) {
                let kind = ev.event.unwrap_or_default();
                let Ok(v) = serde_json::from_str::<Value>(&ev.data) else { continue };

                match kind.as_str() {
                    "content_block_start" => {
                        let index = v.get("index").and_then(Value::as_u64).unwrap_or(0) as usize;
                        let block = v.get("content_block").cloned().unwrap_or(Value::Null);
                        let btype = block.get("type").and_then(Value::as_str).unwrap_or("");
                        match btype {
                            "tool_use" => {
                                let id = block.get("id").and_then(Value::as_str).unwrap_or("").to_string();
                                let name = block.get("name").and_then(Value::as_str).unwrap_or("").to_string();
                                self.pending_tool = Some((index, id, name));
                                self.input_fragments.retain(|(i, _)| *i != index);
                            }
                            "text" => {
                                if let Some(t) = block.get("text").and_then(Value::as_str) {
                                    if !t.is_empty() {
                                        return Some(Event::Text(t.to_string()));
                                    }
                                }
                            }
                            "thinking" => {
                                if let Some(t) = block.get("thinking").and_then(Value::as_str) {
                                    if !t.is_empty() {
                                        return Some(Event::Reasoning(t.to_string()));
                                    }
                                }
                            }
                            _ => {}
                        }
                    }
                    "content_block_delta" => {
                        let index = v.get("index").and_then(Value::as_u64).unwrap_or(0) as usize;
                        let delta = v.get("delta").cloned().unwrap_or(Value::Null);
                        let dtype = delta.get("type").and_then(Value::as_str).unwrap_or("");
                        match dtype {
                            "text_delta" => {
                                if let Some(t) = delta.get("text").and_then(Value::as_str) {
                                    if !t.is_empty() {
                                        return Some(Event::Text(t.to_string()));
                                    }
                                }
                            }
                            "thinking_delta" => {
                                if let Some(t) = delta.get("thinking").and_then(Value::as_str) {
                                    if !t.is_empty() {
                                        return Some(Event::Reasoning(t.to_string()));
                                    }
                                }
                            }
                            "input_json_delta" => {
                                if let Some(frag) = delta.get("partial_json").and_then(Value::as_str) {
                                    self.input_fragments.push((index, frag.to_string()));
                                }
                            }
                            _ => {}
                        }
                    }
                    "content_block_stop" => {
                        let index = v.get("index").and_then(Value::as_u64).unwrap_or(0) as usize;
                        if let Some((idx, id, name)) = self.pending_tool.take() {
                            if idx == index {
                                let args: String = self
                                    .input_fragments
                                    .iter()
                                    .filter(|(i, _)| *i == index)
                                    .map(|(_, s)| s.as_str())
                                    .collect();
                                return Some(Event::ToolCall { id, name, arguments: args });
                            }
                        }
                    }
                    "message_delta" => {
                        let usage = v.get("usage").cloned();
                        if let Some(u) = usage {
                            let out = u.get("output_tokens").and_then(Value::as_u64).unwrap_or(0) as u32;
                            return Some(Event::Usage { input: 0, output: out });
                        }
                    }
                    "message_stop" => {
                        self.done = true;
                        return Some(Event::Done);
                    }
                    "error" => {
                        let msg = v
                            .get("error")
                            .and_then(|e| e.get("message"))
                            .and_then(Value::as_str)
                            .unwrap_or("stream error")
                            .to_string();
                        self.done = true;
                        return Some(Event::Warning(msg));
                    }
                    "ping" => {}
                    _ => {}
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use glass_mod_api::Message;

    #[test]
    fn window_detects_1m_variants() {
        assert_eq!(Anthropic::window_for("claude-opus-4-5"), 200_000);
        assert_eq!(Anthropic::window_for("claude-opus-4-5[1m]"), 1_000_000);
        assert_eq!(Anthropic::window_for("mystery-model"), DEFAULT_WINDOW);
    }

    #[test]
    fn tool_results_group_into_one_user_turn() {
        let msgs = vec![
            Message::User("hi".into()),
            Message::Assistant("working".into()),
            Message::ToolResult { call_id: "a".into(), name: "bash".into(), body: "one".into() },
            Message::ToolResult { call_id: "b".into(), name: "bash".into(), body: "two".into() },
        ];
        let out = Anthropic::build_messages(&msgs);
        assert_eq!(out.len(), 3, "consecutive tool results must merge");
        let last = &out[2];
        assert_eq!(last["role"], "user");
        assert_eq!(last["content"].as_array().unwrap().len(), 2);
    }

    #[test]
    fn tool_result_after_user_starts_new_turn() {
        let msgs = vec![
            Message::User("hi".into()),
            Message::ToolResult { call_id: "a".into(), name: "bash".into(), body: "out".into() },
            Message::User("next".into()),
        ];
        let out = Anthropic::build_messages(&msgs);
        assert_eq!(out.len(), 3);
        assert_eq!(out[1]["content"][0]["type"], "tool_result");
        assert_eq!(out[2]["content"][0]["text"], "next");
    }

    #[test]
    fn tool_result_carries_call_id() {
        let msgs = vec![Message::ToolResult { call_id: "toolu_42".into(), name: "bash".into(), body: "ok".into() }];
        let out = Anthropic::build_messages(&msgs);
        assert_eq!(out[0]["content"][0]["tool_use_id"], "toolu_42");
    }

    #[test]
    fn error_body_is_unwrapped() {
        let body = r#"{"type":"error","error":{"type":"overloaded_error","message":"Overloaded"}}"#;
        assert_eq!(extract_error(body), "Overloaded");
    }

    #[test]
    fn error_body_falls_back_to_raw() {
        assert_eq!(extract_error("not json"), "not json");
    }

    #[test]
    fn tools_are_shaped_for_api() {
        let tools = vec![("bash".to_string(), "run".to_string(), r#"{"type":"object"}"#.to_string())];
        let out = build_tools(&tools);
        assert_eq!(out[0]["name"], "bash");
        assert_eq!(out[0]["input_schema"]["type"], "object");
    }

    #[test]
    fn invalid_tool_schema_degrades_to_object() {
        let tools = vec![("x".to_string(), "d".to_string(), "{not json".to_string())];
        let out = build_tools(&tools);
        assert_eq!(out[0]["input_schema"]["type"], "object");
    }
}
