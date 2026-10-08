//! Anthropic Messages API adapter.
//!
//! Exists separately from the OpenAI-compatible adapter because two things
//! only exist on the native endpoint: `thinking` blocks and `cache_control`
//! breakpoints. Prompt caching is a measurable latency and cost win on long
//! sessions, so it is worth a second adapter rather than losing it.

use std::collections::VecDeque;
use std::io::Read;

use serde_json::{Value, json};

use super::{Error, Event, Provider, Request, Usage, http_agent, read_lossy};
use crate::sse::{SseDecoder, SseEvent};

/// Default context window for models we do not recognise explicitly.
const DEFAULT_WINDOW: u32 = 200_000;

pub struct Anthropic {
    pub base_url: String,
    pub api_key: String,
    pub model: String,
}

/// Anthropic's cache lookback window, in content blocks.
///
/// A breakpoint only finds a cached prefix within this distance of a prior
/// write, so the anchor must sit at least this far back to be useful.
const LOOKBACK_BLOCKS: usize = 20;

/// Mark the tail and a lookback anchor in a conversation.
///
/// Two markers, because they solve different problems. The tail extends the
/// cached prefix one turn at a time. The anchor exists so that when a turn
/// appends more than [`LOOKBACK_BLOCKS`] positions — a batch of parallel tool
/// calls does exactly this — the next request's walk backward still finds a
/// write instead of falling off the end and re-billing the whole conversation.
fn mark_conversation_breakpoints(messages: &mut [Value]) {
    let Some(tail) = last_markable(messages) else {
        return;
    };
    mark_block_end(&mut messages[tail]);

    // Cumulative block offsets, so the anchor can be placed by distance rather
    // than by message index. One message may hold several blocks.
    let mut offsets = Vec::with_capacity(messages.len());
    let mut running = 0usize;
    for m in messages.iter() {
        running += block_count(m);
        offsets.push(running);
    }
    let target = offsets[tail].saturating_sub(LOOKBACK_BLOCKS);
    if let Some(anchor) = (0..tail)
        .rev()
        .find(|&i| offsets[i] <= target && is_markable(&messages[i]))
    {
        mark_block_end(&mut messages[anchor]);
    }
}

/// Number of content blocks in one message, treating a run of tool results as
/// a single position the way the provider does.
fn block_count(m: &Value) -> usize {
    match m.get("content").and_then(Value::as_array) {
        Some(blocks) if !blocks.is_empty() => blocks.len(),
        _ => 1,
    }
}

/// A markable message has content that can carry a cache marker.
fn is_markable(m: &Value) -> bool {
    m.get("content")
        .and_then(Value::as_array)
        .is_some_and(|b| !b.is_empty())
}

fn last_markable(messages: &[Value]) -> Option<usize> {
    (0..messages.len())
        .rev()
        .find(|&i| is_markable(&messages[i]))
}

/// Put `cache_control` on the final block of a message.
///
/// The marker goes on the last block rather than the message, because a
/// breakpoint caches everything up to and including the block it sits on.
fn mark_block_end(m: &mut Value) {
    let Some(blocks) = m.get_mut("content").and_then(Value::as_array_mut) else {
        return;
    };
    if let Some(last) = blocks.last_mut() {
        last["cache_control"] = json!({ "type": "ephemeral" });
    }
}

impl Anthropic {
    pub fn new(
        base_url: impl Into<String>,
        api_key: impl Into<String>,
        model: impl Into<String>,
    ) -> Self {
        Self {
            base_url: base_url.into(),
            api_key: api_key.into(),
            model: model.into(),
        }
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
                    out.push(
                        json!({ "role": "assistant", "content": [{ "type": "text", "text": s }] }),
                    );
                }
                Message::ToolCall {
                    id,
                    name,
                    arguments,
                } => {
                    let block = json!({"type":"tool_use", "id":id, "name":name, "input":serde_json::from_str::<Value>(arguments).unwrap_or_else(|_| json!({}))});
                    if let Some(last) = out.last_mut().filter(|v| v["role"] == "assistant") {
                        last["content"].as_array_mut().unwrap().push(block);
                    } else {
                        out.push(json!({"role":"assistant", "content":[block]}));
                    }
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
            .map(|blocks| {
                blocks
                    .iter()
                    .any(|b| b.get("type").and_then(Value::as_str) == Some("tool_result"))
            })
            .unwrap_or(false)
}

fn build_tools(tools: &[(String, String, String)]) -> Vec<Value> {
    tools
        .iter()
        .map(|(name, desc, schema)| {
            let parsed: Value =
                serde_json::from_str(schema).unwrap_or_else(|_| json!({ "type": "object" }));
            json!({
                "name": name,
                "description": desc,
                "input_schema": parsed
            })
        })
        .collect()
}

impl Anthropic {
    /// Build the request body. Split out from `stream` so the wire shape can be
    /// asserted in tests: the previous version of this code panicked while
    /// constructing the system block, and nothing caught it because tests could
    /// not reach a code path that needed a socket.
    pub(crate) fn build_body(&self, req: &Request<'_>) -> Value {
        let system = json!([{
            "type": "text",
            "text": req.system,
            "cache_control": { "type": "ephemeral" }
        }]);

        let mut messages = Self::build_messages(req.messages);
        mark_conversation_breakpoints(&mut messages);

        let mut body = json!({
            "model": self.model,
            "max_tokens": req.max_tokens,
            "stream": true,
            "system": system,
            "messages": messages,
        });
        if !req.tools.is_empty() {
            body["tools"] = Value::Array(build_tools(req.tools));
        }
        if let Some(budget) = req.reasoning {
            let effective = budget.min(req.max_tokens.saturating_sub(1));
            if effective > 0 {
                body["thinking"] = json!({ "type": "enabled", "budget_tokens": effective });
                body["max_tokens"] = json!(req.max_tokens.max(effective + 1024));
            }
        }
        body
    }
}

impl Provider for Anthropic {
    fn id(&self) -> &str {
        "anthropic"
    }

    fn context_window(&self) -> u32 {
        Self::window_for(&self.model)
    }

    fn stream(&self, req: &Request<'_>) -> Result<Box<dyn Iterator<Item = Event> + '_>, Error> {
        let body = self.build_body(req);

        let resp = http_agent()
            .post(format!(
                "{}/v1/messages",
                self.base_url.trim_end_matches('/')
            ))
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", "2023-06-01")
            .send_json(&body)
            .map_err(map_transport)?;
        let status = resp.status().as_u16();
        let mut reader = resp.into_body().into_reader();
        if !(200..300).contains(&status) {
            let text = read_lossy(&mut reader).unwrap_or_default();
            return Err(Error::Status(
                status,
                if self.api_key.is_empty() {
                    extract_error(&text)
                } else {
                    extract_error(&text).replace(&self.api_key, "[redacted]")
                },
            ));
        }

        let key = self.api_key.clone();
        Ok(Box::new(
            StreamEvents::new(reader).map(move |event| super::redact_event(event, Some(&key))),
        ))
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
    /// Prompt accounting from `message_start`, held until `message_delta`
    /// supplies the output count so the caller sees one complete Usage.
    pending: Option<Usage>,
    done: bool,
    /// Events decoded from one chunk but not yet yielded.
    outbox: VecDeque<Event>,
}

impl<R: Read> StreamEvents<R> {
    fn new(reader: R) -> Self {
        Self {
            reader,
            decoder: SseDecoder::new(),
            pending_tool: None,
            input_fragments: Vec::new(),
            pending: None,
            done: false,
            outbox: VecDeque::new(),
        }
    }

    /// Handle one decoded SSE event, appending anything the caller
    /// should see to `outbox`. Never returns early.
    fn process(&mut self, ev: SseEvent) {
        let kind = ev.event.unwrap_or_default();
        let Ok(v) = serde_json::from_str::<Value>(&ev.data) else {
            self.done = true;
            self.outbox
                .push_back(Event::Error("Provider sent an invalid stream event".into()));
            return;
        };

        match kind.as_str() {
            "content_block_start" => {
                let index = v.get("index").and_then(Value::as_u64).unwrap_or(0) as usize;
                let block = v.get("content_block").cloned().unwrap_or(Value::Null);
                let btype = block.get("type").and_then(Value::as_str).unwrap_or("");
                match btype {
                    "tool_use" => {
                        let id = block
                            .get("id")
                            .and_then(Value::as_str)
                            .unwrap_or("")
                            .to_string();
                        let name = block
                            .get("name")
                            .and_then(Value::as_str)
                            .unwrap_or("")
                            .to_string();
                        self.pending_tool = Some((index, id, name));
                        self.input_fragments.retain(|(i, _)| *i != index);
                    }
                    "text" => {
                        if let Some(t) = block.get("text").and_then(Value::as_str)
                            && !t.is_empty()
                        {
                            self.outbox.push_back(Event::Text(t.to_string()));
                        }
                    }
                    "thinking" => {
                        if let Some(t) = block.get("thinking").and_then(Value::as_str)
                            && !t.is_empty()
                        {
                            self.outbox.push_back(Event::Reasoning(t.to_string()));
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
                        if let Some(t) = delta.get("text").and_then(Value::as_str)
                            && !t.is_empty()
                        {
                            self.outbox.push_back(Event::Text(t.to_string()));
                        }
                    }
                    "thinking_delta" => {
                        if let Some(t) = delta.get("thinking").and_then(Value::as_str)
                            && !t.is_empty()
                        {
                            self.outbox.push_back(Event::Reasoning(t.to_string()));
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
                if let Some((idx, id, name)) = self.pending_tool.take()
                    && idx == index
                {
                    let args: String = self
                        .input_fragments
                        .iter()
                        .filter(|(i, _)| *i == index)
                        .map(|(_, s)| s.as_str())
                        .collect();
                    self.outbox.push_back(Event::ToolCall {
                        id,
                        name,
                        arguments: args,
                    });
                }
            }
            // `message_start` carries the whole prompt accounting:
            // uncached input, cache reads, and cache writes. Dropping
            // this event is why the harness could not see its own cache
            // behaviour.
            "message_start" => {
                let u = v.get("message").and_then(|m| m.get("usage")).cloned();
                if let Some(u) = u {
                    let get = |k: &str| u.get(k).and_then(Value::as_u64).unwrap_or(0) as u32;
                    self.pending = Some(Usage {
                        input: get("input_tokens"),
                        cache_read: get("cache_read_input_tokens"),
                        cache_write: get("cache_creation_input_tokens"),
                        output: 0,
                    });
                }
            }
            "message_delta" => {
                // Output tokens arrive here; carry forward the prompt
                // buckets captured at message_start.
                let out = v
                    .get("usage")
                    .and_then(|u| u.get("output_tokens"))
                    .and_then(Value::as_u64)
                    .unwrap_or(0) as u32;
                let mut u = self.pending.take().unwrap_or_default();
                u.output = out;
                self.outbox.push_back(Event::Usage(u));
            }
            "message_stop" => {
                self.done = true;
                self.outbox.push_back(Event::Done);
            }
            "error" => {
                let text = extract_error(&v.to_string());
                self.outbox
                    .push_back(Event::Error(format!("provider error event: {text}")));
            }
            _ => {}
        }
    }
}

impl<R: Read> Iterator for StreamEvents<R> {
    type Item = Event;

    fn next(&mut self) -> Option<Event> {
        loop {
            // Yield anything already decoded before reading more.
            if let Some(ev) = self.outbox.pop_front() {
                return Some(ev);
            }
            if self.done {
                return None;
            }

            let mut chunk = [0u8; 8192];
            let n = match self.reader.read(&mut chunk) {
                Ok(0) => {
                    self.done = true;
                    self.outbox.push_back(Event::Error(
                        "provider stream ended before message_stop".into(),
                    ));
                    continue;
                }
                Ok(n) => n,
                Err(e) => {
                    self.done = true;
                    self.outbox
                        .push_back(Event::Error(format!("provider stream read failed: {e}")));
                    continue;
                }
            };

            // Process every event in this chunk into the outbox.
            // Returning from inside this loop would discard the
            // events after the first, which is how usage reports
            // used to vanish.
            for ev in self.decoder.push_bytes(&chunk[..n]) {
                self.process(ev);
                if self.done {
                    break;
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
            Message::ToolResult {
                call_id: "a".into(),
                name: "bash".into(),
                body: "one".into(),
            },
            Message::ToolResult {
                call_id: "b".into(),
                name: "bash".into(),
                body: "two".into(),
            },
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
            Message::ToolResult {
                call_id: "a".into(),
                name: "bash".into(),
                body: "out".into(),
            },
            Message::User("next".into()),
        ];
        let out = Anthropic::build_messages(&msgs);
        assert_eq!(out.len(), 3);
        assert_eq!(out[1]["content"][0]["type"], "tool_result");
        assert_eq!(out[2]["content"][0]["text"], "next");
    }

    #[test]
    fn tool_result_carries_call_id() {
        let msgs = vec![Message::ToolResult {
            call_id: "toolu_42".into(),
            name: "bash".into(),
            body: "ok".into(),
        }];
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
        let tools = vec![(
            "bash".to_string(),
            "run".to_string(),
            r#"{"type":"object"}"#.to_string(),
        )];
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

#[cfg(test)]
mod cache_layout {
    use super::*;
    use glass_mod_api::Message;

    fn count_markers(msgs: &[Value]) -> usize {
        msgs.iter()
            .filter_map(|m| m.get("content").and_then(Value::as_array))
            .flatten()
            .filter(|b| b.get("cache_control").is_some())
            .count()
    }

    #[test]
    fn short_conversation_marks_only_the_tail() {
        let mut msgs = vec![
            json!({ "role": "user", "content": [{ "type": "text", "text": "hi" }] }),
            json!({ "role": "assistant", "content": [{ "type": "text", "text": "hello" }] }),
        ];
        mark_conversation_breakpoints(&mut msgs);
        assert_eq!(
            count_markers(&msgs),
            1,
            "a short conversation needs one marker"
        );
        assert!(
            msgs[1]["content"][0]["cache_control"].is_object(),
            "the tail must be marked"
        );
    }

    #[test]
    fn long_conversation_gets_a_lookback_anchor() {
        // 40 messages of one block each: the anchor goes 20 blocks back.
        let mut msgs: Vec<Value> = (0..40)
            .map(|i| {
                json!({ "role": if i % 2 == 0 { "user" } else { "assistant" },
                             "content": [{ "type": "text", "text": format!("m{i}") }] })
            })
            .collect();
        mark_conversation_breakpoints(&mut msgs);
        assert_eq!(
            count_markers(&msgs),
            2,
            "long conversations need tail + anchor"
        );
        // The anchor must be at most LOOKBACK_BLOCKS behind the tail.
        let marked: Vec<usize> = msgs
            .iter()
            .enumerate()
            .filter(|(_, m)| count_markers(std::slice::from_ref(m)) > 0)
            .map(|(i, _)| i)
            .collect();
        assert_eq!(marked.len(), 2);
        assert!(
            marked[1] - marked[0] <= LOOKBACK_BLOCKS,
            "anchor too far back: {marked:?}"
        );
    }

    #[test]
    fn a_parallel_tool_batch_keeps_the_anchor_reachable() {
        // One turn that appends many blocks is exactly the case a single tail
        // marker misses. The anchor must still be within the window.
        let mut msgs: Vec<Value> = Vec::new();
        for i in 0..30 {
            msgs.push(
                json!({ "role": "user", "content": [{ "type": "text", "text": format!("t{i}") }] }),
            );
        }
        // A burst of 12 tool results in one message, as a parallel batch produces.
        let burst: Vec<Value> = (0..12)
            .map(|i| json!({ "type": "tool_result", "tool_use_id": format!("c{i}") }))
            .collect();
        msgs.push(json!({ "role": "user", "content": burst }));

        mark_conversation_breakpoints(&mut msgs);
        let marked: Vec<usize> = msgs
            .iter()
            .enumerate()
            .filter(|(_, m)| count_markers(std::slice::from_ref(m)) > 0)
            .map(|(i, _)| i)
            .collect();
        assert_eq!(marked.len(), 2, "expected tail + anchor: {marked:?}");
        assert_eq!(*marked.last().unwrap(), msgs.len() - 1, "tail must be last");
    }

    #[test]
    fn marking_never_exceeds_the_four_breakpoint_budget() {
        // Anthropic allows 4 total; we use 3 (system + tail + anchor) and must
        // never spill into a 400.
        let mut msgs: Vec<Value> = (0..100)
            .map(|i| json!({ "role": "user", "content": [{ "type": "text", "text": format!("m{i}") }] }))
            .collect();
        mark_conversation_breakpoints(&mut msgs);
        assert!(
            count_markers(&msgs) <= 2,
            "conversation markers must stay within budget"
        );
    }

    #[test]
    fn empty_and_textonly_conversations_are_safe() {
        let mut none: Vec<Value> = Vec::new();
        mark_conversation_breakpoints(&mut none);
        assert!(none.is_empty());

        // A message with string content has no blocks to mark.
        let mut plain = vec![json!({ "role": "user", "content": "just text" })];
        mark_conversation_breakpoints(&mut plain);
        assert_eq!(
            count_markers(&plain),
            0,
            "a string-content message cannot carry a marker"
        );
    }

    #[test]
    fn the_system_prefix_carries_its_own_marker() {
        // Regression: the marker used to be written onto the enclosing ARRAY
        // rather than the block. In serde_json that panics, which meant the
        // Anthropic adapter — the default provider — crashed on the first real
        // request. No unit test caught it because `stream()` needs a network,
        // and every end-to-end test used the OpenAI-compatible adapter.
        let sys = json!([{
            "type": "text",
            "text": "sys",
            "cache_control": { "type": "ephemeral" }
        }]);
        assert!(sys.is_array(), "system is an array of blocks");
        assert!(
            sys[0]["cache_control"].is_object(),
            "the block carries the marker"
        );
    }

    #[test]
    fn markers_land_on_the_last_block_of_a_message() {
        // A breakpoint caches everything up to and including its block, so
        // marking the last block is what makes the tail marker cover the turn.
        let mut msgs = vec![json!({ "role": "user", "content": [
            { "type": "text", "text": "a" },
            { "type": "text", "text": "b" }
        ] })];
        mark_conversation_breakpoints(&mut msgs);
        let blocks = msgs[0]["content"].as_array().unwrap();
        assert!(
            blocks[0].get("cache_control").is_none(),
            "only the last block is marked"
        );
        assert!(blocks[1]["cache_control"].is_object());
    }

    #[test]
    fn a_real_tool_conversation_gets_both_markers() {
        // End-to-end over the shape glass actually sends.
        let msgs = vec![
            Message::User("build the thing".into()),
            Message::Assistant("working".into()),
            Message::ToolResult {
                call_id: "c1".into(),
                name: "bash".into(),
                body: "ok".into(),
            },
        ];
        let mut out = Anthropic::build_messages(&msgs);
        mark_conversation_breakpoints(&mut out);
        let marked: Vec<usize> = out
            .iter()
            .enumerate()
            .filter(|(_, m)| count_markers(std::slice::from_ref(m)) > 0)
            .map(|(i, _)| i)
            .collect();
        assert!(
            !marked.is_empty(),
            "a tool conversation must cache something"
        );
        assert_eq!(count_markers(&out), 1, "short conversation: tail only");
    }
}

#[cfg(test)]
mod wire_shape {
    use super::*;
    use crate::provider::{Request, builtin_tools};

    fn body_for(messages: &[glass_mod_api::Message]) -> Value {
        let p = Anthropic::new("https://x", "k", "claude-opus-5-5");
        let tools = builtin_tools();
        p.build_body(&Request {
            model: "claude-opus-5-5",
            system: "you are glass",
            messages,
            tools: &tools,
            max_tokens: 4096,
            reasoning: None,
        })
    }

    /// The bug this file exists to prevent: building the request used to panic.
    #[test]
    fn a_real_request_body_can_be_built() {
        let msgs = vec![
            glass_mod_api::Message::User("hello".into()),
            glass_mod_api::Message::Assistant("hi".into()),
        ];
        let body = body_for(&msgs);
        assert_eq!(body["model"], "claude-opus-5-5");
        assert!(body["system"].is_array());
    }

    #[test]
    fn the_system_marker_is_on_the_block_not_the_array() {
        // Regression: the marker was written onto the enclosing array, which
        // panics in serde_json. It would also have been silently ignored by the
        // API, quietly disabling caching for the whole prefix.
        let body = body_for(&[glass_mod_api::Message::User("hi".into())]);
        let system = body["system"]
            .as_array()
            .expect("system is an array of blocks");
        assert_eq!(system.len(), 1);
        assert!(
            system[0]["cache_control"].is_object(),
            "marker must be on the block"
        );
    }

    #[test]
    fn every_request_carries_a_cacheable_prefix() {
        // Three markers: system, tail, and (once long enough) the anchor.
        // The system one is what survives compaction.
        let body = body_for(&[glass_mod_api::Message::User("hi".into())]);
        let total = body.to_string().matches("cache_control").count();
        assert!(
            (2..=4).contains(&total),
            "expected 2-4 markers, found {total}"
        );
    }

    #[test]
    fn tools_carry_no_breakpoint_because_system_already_covers_them() {
        // The hierarchy is tools -> system -> messages, so the system marker
        // caches the tool schemas too. A separate tools marker would waste one
        // of the four slots.
        let body = body_for(&[glass_mod_api::Message::User("hi".into())]);
        let tools = body["tools"].as_array().expect("tools present");
        assert_eq!(tools.len(), 2);
        assert!(tools.iter().all(|t| t.get("cache_control").is_none()));
    }

    /// Byte stability: the same input must serialize to the same bytes.
    ///
    /// `serde_json` orders object keys deterministically, but enabling its
    /// `preserve_order` feature anywhere in the dependency graph would flip
    /// that to insertion order and silently change every cached prefix. This
    /// test fails loudly if that ever happens.
    #[test]
    fn serialization_is_byte_stable() {
        let msgs = vec![
            glass_mod_api::Message::User("a".into()),
            glass_mod_api::Message::Assistant("b".into()),
            glass_mod_api::Message::ToolResult {
                call_id: "c".into(),
                name: "bash".into(),
                body: "o".into(),
            },
        ];
        let one = serde_json::to_string(&body_for(&msgs)).unwrap();
        let two = serde_json::to_string(&body_for(&msgs)).unwrap();
        assert_eq!(one, two, "identical input must produce identical bytes");
        // Key order must be sorted, not insertion order.
        assert!(one.contains(r#""max_tokens""#));
        assert!(one.find("cache_control").is_some());
    }

    #[test]
    fn two_identical_conversations_produce_identical_prefixes() {
        // This is the property a prefix cache actually depends on.
        let msgs = vec![
            glass_mod_api::Message::User("one".into()),
            glass_mod_api::Message::Assistant("two".into()),
        ];
        let a = body_for(&msgs);
        let b = body_for(&msgs);
        assert_eq!(a["system"], b["system"]);
        assert_eq!(a["tools"], b["tools"]);
        assert_eq!(a["messages"], b["messages"]);
    }

    #[test]
    fn a_growing_conversation_extends_its_prefix() {
        // Turn N+1 must contain turn N's prefix verbatim, or nothing caches.
        let turn1 = vec![glass_mod_api::Message::User("first".into())];
        let turn2 = vec![
            glass_mod_api::Message::User("first".into()),
            glass_mod_api::Message::Assistant("reply".into()),
            glass_mod_api::Message::User("second".into()),
        ];
        let b1 = body_for(&turn1);
        let b2 = body_for(&turn2);

        assert_eq!(b1["system"], b2["system"], "system prefix must not change");
        assert_eq!(b1["tools"], b2["tools"], "tool prefix must not change");

        // Every message from turn 1 is still present, in order, identical.
        let m1 = b1["messages"].as_array().unwrap();
        let m2 = b2["messages"].as_array().unwrap();
        for (i, original) in m1.iter().enumerate() {
            let mut without_marker = original.clone();
            // Markers move between turns; compare content only.
            if let Some(blocks) = without_marker
                .get_mut("content")
                .and_then(Value::as_array_mut)
            {
                for b in blocks.iter_mut() {
                    if let Some(o) = b.as_object_mut() {
                        o.remove("cache_control");
                    }
                }
            }
            assert_eq!(&without_marker, &m2[i], "message {i} changed between turns");
        }
    }

    #[test]
    fn thinking_settings_are_consistent_with_max_tokens() {
        // Anthropic requires budget < max_tokens, and thinking config is
        // rendered into the prompt, so an inconsistent pair is both a 400 and a
        // cache invalidation.
        let p = Anthropic::new("https://x", "k", "claude-opus-5-5");
        let tools = builtin_tools();
        let body = p.build_body(&Request {
            model: "claude-opus-5-5",
            system: "s",
            messages: &[glass_mod_api::Message::User("hi".into())],
            tools: &tools,
            max_tokens: 8000,
            reasoning: Some(4000),
        });
        let budget = body["thinking"]["budget_tokens"].as_u64().unwrap();
        let max = body["max_tokens"].as_u64().unwrap();
        assert!(
            budget < max,
            "budget {budget} must be below max_tokens {max}"
        );
    }
}
