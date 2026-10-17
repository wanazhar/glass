//! OpenAI-compatible adapter.
//!
//! One wire format reaches GPT-5, Kimi, GLM, Qwen, DeepSeek, Mistral, Gemini's
//! compatibility endpoint, Ollama, vLLM, and LM Studio. A new provider is a
//! base URL and a model id, which is why this adapter is the workhorse and
//! the Anthropic one is the specialist.

use std::io::Read;

use serde_json::{Value, json};

use super::{Error, Event, Provider, Request, Usage, http_agent, read_lossy};
use crate::sse::SseDecoder;

const DEFAULT_WINDOW: u32 = 128_000;

pub struct OpenAiCompat {
    pub base_url: String,
    pub adaptive_pacing: bool,
    pub api_key: Option<String>,
    pub model: String,
    session_identity: Option<String>,
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
    pub fn new(
        base_url: impl Into<String>,
        api_key: Option<String>,
        model: impl Into<String>,
    ) -> Self {
        Self {
            base_url: base_url.into(),
            adaptive_pacing: true,
            api_key,
            model: model.into(),
            session_identity: None,
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
    let mut reasoning: Option<&str> = None;
    let mut assistant_hash: Option<&str> = None;
    let mut pending = false;
    let mut batch=Vec::new();
    let mut emitted=std::collections::HashSet::new();
    for (index,m) in messages.iter().enumerate() {
        match m {
            Message::User(s) => {reasoning=None;assistant_hash=None;batch.clear();emitted.clear();pending=false;out.push(json!({ "role": "user", "content": s }));},
            Message::Image { media_type, data } => {reasoning=None;assistant_hash=None;batch.clear();emitted.clear();pending=false;out.push(json!({
                "role": "user",
                "content": [{
                    "type": "image_url",
                    "image_url": { "url": format!("data:{media_type};base64,{data}") }
                }]
            }));},
            Message::ChatReasoning {content,assistant_hash:hash,tool_call_ids} => {
                reasoning=content.as_deref();assistant_hash=hash.as_deref();let ids=tool_call_ids;pending=true;
                emitted.clear();batch.clear();
                for (relative,next) in messages[index+1..].iter().enumerate() {
                    match next {
                        Message::ChatReasoning{..}|Message::User(_)=>break,
                        Message::ToolCall{id,name,arguments} if ids.contains(id)=>batch.push((index+1+relative,json!({"id":id,"type":"function","function":{"name":name,"arguments":crate::tool_repair::wire_arguments(arguments)}}))),
                        Message::ToolCall{..}=>break,
                        _=>{},
                    }
                    if batch.len()>=ids.len() {break;}
                }
            }
            Message::Assistant(s) => {
                let mut value=json!({"role":"assistant","content":s});
                if pending && assistant_hash==Some(crate::workspace::hash(s.as_bytes()).as_str()) {
                    if let Some(text)=reasoning {value["reasoning_content"]=json!(text);}
                } else {reasoning=None;batch.clear();emitted.clear();}
                pending=false;
                out.push(value);
            },
            Message::ToolCall {
                id,
                name,
                arguments,
            } => {
                if emitted.contains(&index) {continue;}
                let associated=batch.iter().any(|(position,_)|*position==index);
                let call = json!({"id":id,"type":"function","function":{"name":name,"arguments":crate::tool_repair::wire_arguments(arguments)}});
                let calls=if associated {
                    batch.iter().map(|(position,call)|{emitted.insert(*position);call.clone()}).collect::<Vec<_>>()
                } else {vec![call]};
                let merge=!pending && out.last().is_some_and(|v|v["role"]=="assistant" && (associated || v.get("reasoning_content").is_none()));
                if merge {
                    let last=out.last_mut().unwrap();
                    if !last["tool_calls"].is_array() {last["tool_calls"]=json!([]);}
                    last["tool_calls"].as_array_mut().unwrap().extend(calls);
                } else {
                    let mut value=json!({"role":"assistant","tool_calls":calls});
                    if associated && let Some(text)=reasoning {value["reasoning_content"]=json!(text);}
                    out.push(value);
                }
                pending=false;
            }
            Message::System(s) => {if pending {pending=false;reasoning=None;batch.clear();emitted.clear();} out.push(json!({ "role": "system", "content": s }));},
            Message::ToolResult {
                call_id,
                name,
                body,
            } => out.push(json!({
                "role": "tool",
                "tool_call_id": call_id,
                "name": name,
                "content": body
            })),
        }
    }
    out
}

fn responses_input(messages: &[glass_mod_api::Message]) -> Vec<Value> {
    use glass_mod_api::Message;
    messages
        .iter()
        .filter_map(|m| if matches!(m,Message::ChatReasoning{..}) {None} else {Some(match m {
            Message::ToolCall {
                id,
                name,
                arguments,
            } => json!({"type":"function_call", "call_id":id, "name":name, "arguments":crate::tool_repair::wire_arguments(arguments)}),
            Message::ToolResult { call_id, body, .. } => {
                json!({"type":"function_call_output", "call_id":call_id, "output":body})
            }
            Message::User(s) => json!({"role":"user", "content":s}),
            Message::Image { media_type, data } => json!({
                "role": "user",
                "content": [{
                    "type": "input_image",
                    "image_url": format!("data:{media_type};base64,{data}")
                }]
            }),
            Message::Assistant(s) => json!({"role":"assistant", "content":s}),
            Message::System(s) => json!({"role":"developer", "content":s}),
            Message::ChatReasoning{..}=>unreachable!(),
        })})
        .collect()
}

fn build_tools(tools: &[(String, String, String)]) -> Value {
    Value::Array(
        tools
            .iter()
            .map(|(name, desc, schema)| {
                let params: Value =
                    serde_json::from_str(schema).unwrap_or_else(|_| json!({ "type": "object" }));
                json!({
                    "type": "function",
                    "function": { "name": name, "description": desc, "parameters": params }
                })
            })
            .collect(),
    )
}

impl Provider for OpenAiCompat {
    fn set_session_identity(&mut self, identity: Option<&str>) {
        self.session_identity = identity.map(str::to_owned);
    }
    fn model(&self) -> &str {
        &self.model
    }
    fn id(&self) -> &str {
        "openai"
    }

    fn estimate_tokens(&self,messages:&[glass_mod_api::Message])->u32 {
        super::default_estimate_for(messages,self.flavour==Flavour::ChatCompletions)
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
                "input": responses_input(req.messages),
                "store": false,
                "max_output_tokens": req.max_tokens,
            }),
        };

        if !req.tools.is_empty() {
            // `build_tools` already returns an array of function objects.
            // Wrapping it again produced `[[{...}]]`, which every strict
            // endpoint rejects — and lenient ones ignore, leaving the model
            // with no tools at all and the agent silently inert.
            body["tools"] = if self.flavour == Flavour::Responses {
                Value::Array(req.tools.iter().map(|(name, description, schema)| json!({
                    "type": "function", "name": name, "description": description,
                    "parameters": serde_json::from_str::<Value>(schema).unwrap_or_else(|_| json!({"type":"object"}))
                })).collect())
            } else {
                build_tools(req.tools)
            };
        }

        // Reasoning budget is vendor-specific even inside this format. GPT-5
        // takes a verbosity hint, o-series and most Chinese providers take
        // `thinking`, and the rest ignore both. Passing an unknown field is
        // usually tolerated, but not universally, so we gate on the model id.
        if let Some(budget) = req.reasoning {
            let m = self.model.to_ascii_lowercase();
            if m.starts_with("gpt-5")
                || m.starts_with("o1")
                || m.starts_with("o3")
                || m.starts_with("o4")
            {
                if self.flavour == Flavour::Responses {
                    body["reasoning"] = json!({"effort": effort_label(budget), "summary": "auto"});
                } else {
                    body["reasoning_effort"] = json!(effort_label(budget));
                }
            } else if m.contains("glm") || m.contains("qwen3") || m.contains("deepseek") {
                body["thinking"] = if budget == 0 {
                    json!({"type":"disabled"})
                } else {
                    json!({ "type": "enabled", "budget_tokens": budget })
                };
            }
        }

        let mut rb = http_agent().post(self.endpoint());
        if let Some(identity) = &self.session_identity {
            rb = rb
                .header("x-session-id", identity)
                .header("x-session-affinity", identity);
            if url::Url::parse(&self.base_url)
                .ok()
                .and_then(|url| url.host_str().map(str::to_owned))
                .as_deref()
                == Some("api.openai.com")
            {
                body["prompt_cache_key"] = json!(identity);
            }
        }
        if let Some(k) = &self.api_key {
            rb = rb.header("authorization", format!("Bearer {k}"));
        }
        let pacing_key = super::pacing::acquire(
            &self.base_url,
            self.api_key.as_deref().unwrap_or(""),
            self.adaptive_pacing,
        )?;
        let resp = rb.send_json(&body).map_err(map_transport)?;
        let status = resp.status().as_u16();
        let pacing_headers = [
            "retry-after",
            "x-ratelimit-remaining-requests",
            "x-ratelimit-reset-requests",
        ]
        .map(|name| {
            resp.headers()
                .get(name)
                .and_then(|v| v.to_str().ok())
                .map(str::to_owned)
        });
        let header = |name: &str| match name {
            "retry-after" => pacing_headers[0].clone(),
            "x-ratelimit-remaining-requests" => pacing_headers[1].clone(),
            "x-ratelimit-reset-requests" => pacing_headers[2].clone(),
            _ => None,
        };
        // A quota refusal is not pressure. Keep its server deadlines, but do
        // not teach the pacer to slow healthy future requests.
        if status != 429 {
            super::pacing::observe(pacing_key, status, header);
        }
        let mut reader = resp.into_body().into_reader();
        if !(200..300).contains(&status) {
            let text = read_lossy(&mut reader).unwrap_or_default();
            if status == 429 {
                let observed = if super::pacing::quota_limited(&extract_error(&text)) {
                    400
                } else {
                    status
                };
                super::pacing::observe(pacing_key, observed, header);
            }
            return Err(Error::Status(
                status,
                match &self.api_key {
                    Some(key) if !key.is_empty() => extract_error(&text).replace(key, "[redacted]"),
                    _ => extract_error(&text),
                },
            ));
        }

        let key = self.api_key.clone();
        let mut stream = StreamEvents::new(reader);
        stream.acc.model = self.model.chars().take(160).collect();
        stream.acc.finish = "not reported".into();
        stream.acc.response_id = "not reported".into();
        stream.acc.delta_keys = "none".into();
        Ok(Box::new(stream.map(move |event| {
            if let Event::Error(ref message) = event {
                let status = super::pacing::stream_status(message);
                if super::pacing::rate_limited(message) {
                    super::pacing::observe(pacing_key, 429, |_| None);
                } else if status.is_some_and(|status| status >= 500) {
                    super::pacing::observe(pacing_key, 503, |_| None);
                }
            }
            super::redact_event(event, key.as_deref())
        })))
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
    super::http_error_message(body)
}

/// Everything accumulated across the stream for a single response.
#[derive(Default)]
struct Accum {
    text: String,
    reasoning: String,
    chat_response: bool,
    summaries: std::collections::BTreeMap<(String, u64), String>,
    tools: std::collections::BTreeMap<usize, Partial>,
    usage: Option<Usage>,
    /// Guards `seal` against running twice.
    sealed: bool,
    events: usize,
    finish: String,
    model: String,
    response_id: String,
    delta_keys: String,
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
        if self
            .acc
            .tools
            .values()
            .any(|p| p.id.is_empty() || p.name.is_empty())
        {
            self.fail("Provider returned an incomplete or invalid tool call".into());
            return;
        }
        if self.acc.text.trim().is_empty() && self.acc.tools.is_empty() {
            let usage = self
                .acc
                .usage
                .as_ref()
                .map(|u| format!("{} input / {} output", u.prompt_total(), u.output))
                .unwrap_or_else(|| "not reported".into());
            let cause = match self.acc.finish.as_str() {
                "length" => {
                    "output limit reached before an answer; increase the output budget or reduce reasoning"
                }
                "content_filter" => {
                    "provider filtered the answer; revise the request or switch model"
                }
                _ if !self.acc.reasoning.is_empty() => {
                    "reasoning arrived without an answer; retry or switch model"
                }
                _ => "provider completed an empty answer; retry or switch model",
            };
            if let Some(u) = self.acc.usage.take() {
                self.outbox.push_back(Event::Usage(u));
            }
            self.fail(format!("provider returned no text or tool calls: {cause}. Stream diagnostics: events={}, finish={}, model={}, response={}, reasoning={} chars, usage={}, delta fields={}",
                self.acc.events, self.acc.finish, self.acc.model, self.acc.response_id,
                self.acc.reasoning.chars().count(), usage, self.acc.delta_keys));
            return;
        }
        self.acc.sealed = true;
        if self.acc.chat_response {self.outbox.push_back(Event::ChatResponse);}

        if let Some(u) = self.acc.usage.take() {
            self.outbox.push_back(Event::Usage(u));
        }
        let calls: Vec<_> = std::mem::take(&mut self.acc.tools)
            .into_iter()
            .filter(|(_, p)| !p.name.is_empty())
            .map(|(_, p)| Event::ToolCall {
                id: p.id,
                name: p.name,
                arguments: p.args,
            })
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
                if self.acc.sealed {
                    return None;
                }
                self.seal();
                continue;
            }

            let mut chunk = [0u8; 8192];
            let n = match self.reader.read(&mut chunk) {
                Ok(0) => {
                    if let Some(ev) = self.decoder.finish()
                        && let Ok(v) = serde_json::from_str::<Value>(&ev.data)
                    {
                        self.consume(&v);
                    }
                    if !self.done {
                        self.fail("provider stream ended before completion".into());
                    }
                    continue;
                }
                Err(e) => {
                    self.fail(format!("provider stream read failed: {e}"));
                    continue;
                }
                Ok(n) => n,
            };

            for ev in self.decoder.push_bytes(&chunk[..n]) {
                if ev.data.trim() == "[DONE]" {
                    self.done = true;
                    break;
                }
                let Ok(v) = serde_json::from_str::<Value>(&ev.data) else {
                    self.fail("Provider sent an invalid stream event".into());
                    break;
                };
                self.consume(&v);
                if self.done {
                    break;
                }
            }
        }
    }

    fn fail(&mut self, message: String) {
        self.done = true;
        self.acc.sealed = true;
        self.acc.tools.clear();
        self.outbox.push_back(Event::Error(message));
    }

    /// Fold one delta object into the accumulator.
    fn consume(&mut self, v: &Value) {
        self.acc.events += 1;
        for (key, target) in [
            ("model", &mut self.acc.model),
            ("id", &mut self.acc.response_id),
        ] {
            if let Some(value) = v[key].as_str().or_else(|| v["response"][key].as_str()) {
                *target = value.chars().take(160).collect();
            }
        }
        if let Some(error) = v
            .get("error")
            .or_else(|| v.get("response").and_then(|r| r.get("error")))
        {
            let throttled = error["code"].as_u64() == Some(429)
                || error["code"]
                    .as_str()
                    .is_some_and(|c| c == "429" || super::pacing::rate_limited(c))
                || error["status"].as_u64() == Some(429)
                || error["type"]
                    .as_str()
                    .is_some_and(super::pacing::rate_limited);
            let status = ["code", "status"].iter().find_map(|name| {
                error[*name]
                    .as_u64()
                    .and_then(|n| u16::try_from(n).ok())
                    .or_else(|| {
                        error[*name]
                            .as_str()
                            .and_then(|code| code.parse::<u16>().ok())
                    })
                    .filter(|status| (400..600).contains(status))
            });
            if let Some(message) = error
                .get("message")
                .and_then(Value::as_str)
                .or_else(|| error.as_str())
                .or(throttled.then_some("provider temporarily throttled requests"))
                .or(status.map(|_| "provider rejected request"))
            {
                self.fail(if throttled {
                    format!("provider rejected stream (HTTP 429): rate limit: {message}")
                } else if let Some(status) = status.filter(|status| (400..600).contains(status)) {
                    format!("provider rejected stream (HTTP {status}): {message}")
                } else {
                    message.to_string()
                });
                return;
            }
        }

        if let Some(kind) = v.get("type").and_then(Value::as_str) {
            match kind {
                "response.output_text.delta" | "response.refusal.delta" => {
                    if let Some(t) = v["delta"].as_str() {
                        self.acc.text.push_str(t);
                        self.outbox.push_back(Event::Text(t.into()));
                    }
                }
                "response.reasoning_summary_text.delta"
                | "response.reasoning_summary_text.done" => {
                    let key = (
                        v["item_id"]
                            .as_str()
                            .map(str::to_owned)
                            .unwrap_or_else(|| v["output_index"].to_string()),
                        v["summary_index"].as_u64().unwrap_or(0),
                    );
                    let previous = self.acc.summaries.entry(key).or_default();
                    let text = if kind.ends_with(".delta") {
                        v["delta"].as_str().unwrap_or("")
                    } else if previous.is_empty() {
                        v["text"].as_str().unwrap_or("")
                    } else {
                        ""
                    };
                    if !text.is_empty() {
                        previous.push_str(text);
                        self.acc.reasoning.push_str(text);
                        self.outbox.push_back(Event::Reasoning(text.into()));
                    }
                }
                "response.output_item.added" => {
                    if v["item"]["type"] == "function_call" {
                        let i = v["output_index"].as_u64().unwrap_or(0) as usize;
                        self.acc.tools.insert(
                            i,
                            Partial {
                                id: v["item"]["call_id"].as_str().unwrap_or("").into(),
                                name: v["item"]["name"].as_str().unwrap_or("").into(),
                                args: String::new(),
                            },
                        );
                    }
                }
                "response.function_call_arguments.delta" => {
                    let i = v["output_index"].as_u64().unwrap_or(0) as usize;
                    self.acc
                        .tools
                        .entry(i)
                        .or_default()
                        .args
                        .push_str(v["delta"].as_str().unwrap_or(""));
                }
                "response.output_item.done" => {
                    if v["item"]["type"] == "function_call" {
                        let i = v["output_index"].as_u64().unwrap_or(0) as usize;
                        self.acc.tools.insert(
                            i,
                            Partial {
                                id: v["item"]["call_id"].as_str().unwrap_or("").into(),
                                name: v["item"]["name"].as_str().unwrap_or("").into(),
                                args: v["item"]["arguments"].as_str().unwrap_or("{}").into(),
                            },
                        );
                    }
                }
                "response.completed" => {
                    self.read_usage(&v["response"]);
                    self.done = true;
                }
                "response.failed" | "response.incomplete" | "error" => {
                    self.fail(
                        v["response"]["error"]["message"]
                            .as_str()
                            .or_else(|| v["message"].as_str())
                            .unwrap_or("provider did not complete the response")
                            .into(),
                    );
                }
                _ => {}
            }
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
        if let Some(reason) = choice["finish_reason"].as_str() {
            self.acc.finish = reason.chars().take(80).collect();
        }
        if choice["finish_reason"] == "length" {
            self.outbox.push_back(Event::Warning(
                "Response reached the output token limit".into(),
            ));
        }
        let delta = choice
            .get("delta")
            .filter(|d| !d.is_null())
            .or_else(|| choice.get("message"));
        if let Some(fields) = delta.and_then(Value::as_object)
            && !fields.is_empty()
        {
            self.acc.delta_keys = fields
                .keys()
                .take(16)
                .map(|k| k.chars().take(40).collect::<String>())
                .collect::<Vec<_>>()
                .join(",");
        }

        self.acc.chat_response = true;
        if let Some(t)=delta.and_then(|d|d.get("reasoning_content").or_else(||d.get("reasoning"))).and_then(Value::as_str) {
            self.outbox.push_back(Event::ChatReasoning(t.into()));
        }
        if let Some(t) = delta
            .and_then(|d| d.get("reasoning_content").and_then(Value::as_str))
            .or_else(|| {
                delta
                    .and_then(|d| d.get("reasoning"))
                    .and_then(Value::as_str)
            })
            && !t.is_empty()
        {
            self.acc.reasoning.push_str(t);
            self.outbox.push_back(Event::Reasoning(t.into()));
        }

        if let Some(t) = delta.and_then(|d| d.get("content")).and_then(Value::as_str)
            && !t.is_empty()
        {
            self.acc.text.push_str(t);
            self.outbox.push_back(Event::Text(t.to_string()));
        }

        if let Some(d) = delta {
            // Some compatible endpoints return typed text parts or refusals.
            if let Some(parts) = d["content"].as_array() {
                for part in parts {
                    if matches!(part["type"].as_str(), Some("text" | "output_text"))
                        && let Some(t) = part["text"].as_str().filter(|t| !t.is_empty())
                    {
                        self.acc.text.push_str(t);
                        self.outbox.push_back(Event::Text(t.into()));
                    }
                }
            }
            if let Some(t) = d["refusal"].as_str().filter(|t| !t.is_empty()) {
                self.acc.text.push_str(t);
                self.outbox.push_back(Event::Text(t.into()));
            }
        }

        if let Some(calls) = delta
            .and_then(|d| d.get("tool_calls"))
            .and_then(Value::as_array)
        {
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
                if let Some(args) = func
                    .and_then(|f| f.get("arguments"))
                    .and_then(Value::as_str)
                {
                    slot.args.push_str(args);
                }
            }
        }

        self.read_usage(v);
    }

    fn read_usage(&mut self, v: &Value) {
        if let Some(usage) = v.get("usage").filter(|u| !u.is_null()) {
            let prompt = usage
                .get("prompt_tokens")
                .or_else(|| usage.get("input_tokens"))
                .and_then(Value::as_u64)
                .unwrap_or(0) as u32;
            let output = usage
                .get("completion_tokens")
                .or_else(|| usage.get("output_tokens"))
                .and_then(Value::as_u64)
                .unwrap_or(0) as u32;

            // Unlike Anthropic, OpenAI reports `cached_tokens` as a SUBSET of
            // `prompt_tokens`, not in addition to it. Subtracting is what makes
            // the buckets mean the same thing across both adapters, so a cost
            // model does not have to special-case the provider.
            let cached = usage
                .get("prompt_tokens_details")
                .or_else(|| usage.get("input_tokens_details"))
                .and_then(|d| d.get("cached_tokens"))
                .and_then(Value::as_u64)
                .unwrap_or(0) as u32;
            let written = usage
                .get("prompt_tokens_details")
                .or_else(|| usage.get("input_tokens_details"))
                .and_then(|d| d.get("cache_write_tokens"))
                .and_then(Value::as_u64)
                .unwrap_or(0) as u32;

            if prompt > 0 || output > 0 {
                self.acc.usage = Some(Usage {
                    input: prompt.saturating_sub(cached).saturating_sub(written),
                    cache_read: cached,
                    cache_write: written,
                    output,
                });
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
    fn checkpoint_reports_do_not_split_wire_tool_batches() {
        use crate::session::Record;
        let records = vec![
            Record::User("task".into()),
            Record::ToolCall {
                id: "rew".into(),
                name: "rewind".into(),
                arguments: "{}".into(),
            },
            Record::ToolCall {
                id: "next".into(),
                name: "read".into(),
                arguments: "{}".into(),
            },
            Record::Checkpoint {
                label: "a".into(),
                log_len: 3,
                rewind_summary: Some("retained finding".into()),
            },
            Record::ToolResult {
                call_id: "rew".into(),
                name: "rewind".into(),
                body: "done".into(),
            },
            Record::ToolResult {
                call_id: "next".into(),
                name: "read".into(),
                body: "contents".into(),
            },
        ];
        let wire = build_messages("system", &crate::compact::project(&records));
        let call = wire
            .iter()
            .position(|m| m["tool_calls"].is_array())
            .unwrap();
        assert_eq!(wire[call]["tool_calls"].as_array().unwrap().len(), 2);
        assert_eq!(wire[call + 1]["role"], "tool");
        assert_eq!(wire[call + 2]["role"], "tool");
        assert!(wire[..call].iter().any(|m| {
            m["content"]
                .as_str()
                .is_some_and(|t| t.contains("retained finding"))
        }));
    }

    #[test]
    fn an_image_message_becomes_a_data_uri_in_both_flavours() {
        let messages = vec![Message::Image {
            media_type: "image/png".into(),
            data: "QUJD".into(),
        }];
        let chat = build_messages("sys", &messages);
        assert_eq!(chat[1]["content"][0]["type"], "image_url");
        assert_eq!(
            chat[1]["content"][0]["image_url"]["url"],
            "data:image/png;base64,QUJD"
        );
        let responses = responses_input(&messages);
        assert_eq!(responses[0]["content"][0]["type"], "input_image");
        assert_eq!(
            responses[0]["content"][0]["image_url"],
            "data:image/png;base64,QUJD"
        );
    }

    #[test]
    fn window_by_family() {
        assert_eq!(OpenAiCompat::window_for("gpt-5"), 400_000);
        assert_eq!(OpenAiCompat::window_for("gpt-4-turbo"), 128_000);
        assert_eq!(OpenAiCompat::window_for("gemini-3-pro"), 1_000_000);
        assert_eq!(OpenAiCompat::window_for("glm-4.6"), 128_000);
        assert_eq!(
            OpenAiCompat::window_for("something-unknown"),
            DEFAULT_WINDOW
        );
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
        let msgs = vec![Message::ToolResult {
            call_id: "call_1".into(),
            name: "bash".into(),
            body: "ok".into(),
        }];
        let out = build_messages("sys", &msgs);
        assert_eq!(out[0]["role"], "system");
        assert_eq!(out[1]["role"], "tool");
        assert_eq!(out[1]["tool_call_id"], "call_1");
    }

    #[test]
    fn tools_use_function_wrapper() {
        let tools = vec![(
            "bash".to_string(),
            "run".to_string(),
            r#"{"type":"object"}"#.to_string(),
        )];
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

#[cfg(test)]
mod shape_tests {
    use super::*;
    use crate::provider::{Request, builtin_tools};

    /// The wire format must be a flat array of function objects.
    ///
    /// Regression: `tools` was wrapped twice, producing `[[{...}]]`. Strict
    /// endpoints reject that shape outright and lenient ones ignore it, which
    /// leaves the model with no tools and the agent silently inert. The symptom
    /// is invisible from inside the harness because every unit test still
    /// passed — it only showed up against a real endpoint.
    #[test]
    fn tools_are_not_double_wrapped() {
        let tools = build_tools(&builtin_tools());
        assert!(tools.is_array(), "tools must be an array");
        let arr = tools.as_array().expect("array");
        assert_eq!(arr.len(), 6, "one entry per tool, not one entry per array");
        for t in arr {
            assert!(
                t.get("type").and_then(Value::as_str) == Some("function"),
                "{t}"
            );
            assert!(
                t.get("function").is_some(),
                "each entry needs a function object: {t}"
            );
            assert!(
                t.as_array().is_none(),
                "entries must be objects, not arrays"
            );
        }
    }

    #[test]
    fn tool_entry_carries_name_and_schema() {
        let tools = build_tools(&builtin_tools());
        let arr = tools.as_array().unwrap();
        let names: Vec<&str> = arr
            .iter()
            .filter_map(|t| t["function"]["name"].as_str())
            .collect();
        assert_eq!(
            names,
            vec!["bash", "state", "view_image", "read", "edit", "write"]
        );
        assert!(arr[0]["function"]["parameters"]["properties"].is_object());
    }

    #[test]
    fn empty_tool_list_produces_an_empty_array() {
        assert!(build_tools(&[]).as_array().unwrap().is_empty());
    }

    /// The request built for a real call must carry the flat shape.
    #[test]
    fn request_body_shape_is_flat() {
        let p = OpenAiCompat::new("https://x", None, "gpt-5");
        let tools = builtin_tools();
        let req = Request {
            model: "gpt-5",
            system: "s",
            messages: &[glass_mod_api::Message::User("hi".into())],
            tools: &tools,
            max_tokens: 100,
            reasoning: None,
        };
        // Build the same body the adapter would, without a network call.
        let mut body = json!({
            "model": "gpt-5",
            "stream": true,
            "messages": build_messages("s", req.messages),
        });
        body["tools"] = build_tools(req.tools);
        let arr = body["tools"].as_array().expect("array");
        assert_eq!(arr.len(), 6);
        assert!(arr.iter().all(|t| t["function"]["name"].is_string()));
        let _ = p;
    }
}

#[cfg(test)]
mod stream_tests {
    use super::*;

    /// Drive the SSE decoder over a byte-for-byte copy of what a real
    /// endpoint sends for a tool call, and check what comes out.
    fn run_stream(chunks: &[&str]) -> Vec<Event> {
        use crate::sse::SseDecoder;
        let mut acc = Accum::default();
        let mut outbox = std::collections::VecDeque::new();
        let mut decoder = SseDecoder::new();

        for chunk in chunks {
            for ev in decoder.push(chunk) {
                if ev.data.trim() == "[DONE]" {
                    continue;
                }
                let Ok(v) = serde_json::from_str::<Value>(&ev.data) else {
                    continue;
                };
                for c in v["choices"][0]["delta"]["tool_calls"]
                    .as_array()
                    .into_iter()
                    .flatten()
                {
                    let idx = c.get("index").and_then(Value::as_u64).unwrap_or(0) as usize;
                    let slot = acc.tools.entry(idx).or_default();
                    if let Some(n) = c["function"]["name"].as_str() {
                        slot.name.push_str(n);
                    }
                    if let Some(a) = c["function"]["arguments"].as_str() {
                        slot.args.push_str(a);
                    }
                    if let Some(i) = c["id"].as_str() {
                        slot.id.push_str(i);
                    }
                }
            }
        }
        {
            for (_, p) in std::mem::take(&mut acc.tools) {
                if !p.name.is_empty() {
                    outbox.push_back(Event::ToolCall {
                        id: p.id,
                        name: p.name,
                        arguments: p.args,
                    });
                }
            }
        }
        outbox.into_iter().collect()
    }

    #[test]
    fn a_single_chunk_tool_call_is_emitted() {
        let chunk = "data: {\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"id\":\"c1\",\"function\":{\"name\":\"hl_edit\",\"arguments\":\"{\\\"path\\\":\\\"/x\\\"}\"}}]}}]}\n\n";
        let evs = run_stream(&[chunk]);
        assert_eq!(evs.len(), 1, "{evs:?}");
        match &evs[0] {
            Event::ToolCall {
                name,
                arguments,
                id,
            } => {
                assert_eq!(name, "hl_edit");
                assert_eq!(id, "c1");
                assert!(arguments.contains("/x"), "{arguments}");
            }
            other => panic!("expected a tool call, got {other:?}"),
        }
    }

    #[test]
    fn arguments_split_across_chunks_are_reassembled() {
        let a = "data: {\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"id\":\"c2\",\"function\":{\"name\":\"bash\",\"arguments\":\"{\\\"command\\\":\"}}]}}]}\n\n";
        let b = "data: {\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"function\":{\"arguments\":\"\\\"ls -la\\\"}\"}}]}}]}\n\n";
        let evs = run_stream(&[a, b]);
        assert_eq!(evs.len(), 1);
        match &evs[0] {
            Event::ToolCall { arguments, .. } => {
                let parsed: Value =
                    serde_json::from_str(arguments).expect("reassembled args must be valid JSON");
                assert_eq!(parsed["command"], "ls -la");
            }
            other => panic!("expected a tool call, got {other:?}"),
        }
    }

    #[test]
    fn two_tools_in_one_chunk_both_emit() {
        let chunk = "data: {\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"id\":\"a\",\"function\":{\"name\":\"one\",\"arguments\":\"{}\"}},{\"index\":1,\"id\":\"b\",\"function\":{\"name\":\"two\",\"arguments\":\"{}\"}}]}}]}\n\n";
        let evs = run_stream(&[chunk]);
        assert_eq!(evs.len(), 2, "{evs:?}");
    }

    #[test]
    fn a_text_only_chunk_produces_no_tool_calls() {
        let chunk = "data: {\"choices\":[{\"delta\":{\"content\":\"hi\"}}]}\n\n";
        assert!(run_stream(&[chunk]).is_empty());
    }
}

#[cfg(test)]
mod protocol_regressions {
    use super::*;
    fn events(values: &[Value], done: bool) -> Vec<Event> {
        let mut wire = values
            .iter()
            .map(|v| format!("data: {v}\n\n"))
            .collect::<String>();
        if done {
            wire.push_str("data: [DONE]\n\n");
        }
        StreamEvents::new(std::io::Cursor::new(wire.into_bytes())).collect()
    }
    #[test]
    fn responses_text_usage_and_completion_are_decoded() {
        let output = events(
            &[
                json!({"type":"response.output_text.delta","delta":"Hello"}),
                json!({"type":"response.completed","response":{"usage":{"input_tokens":100,"output_tokens":5,"input_tokens_details":{"cached_tokens":40}}}}),
            ],
            false,
        );
        assert_eq!(
            output,
            vec![
                Event::Text("Hello".into()),
                Event::Usage(Usage {
                    input: 60,
                    output: 5,
                    cache_read: 40,
                    cache_write: 0
                }),
                Event::Done
            ]
        );
    }
    #[test]
    fn responses_tools_use_call_ids_and_complete_arguments() {
        let output = events(
            &[
                json!({"type":"response.output_item.added","output_index":0,"item":{"type":"function_call","call_id":"c1","name":"bash"}}),
                json!({"type":"response.function_call_arguments.delta","output_index":0,"delta":"{\"command\":\"ls\"}"}),
                json!({"type":"response.completed","response":{}}),
            ],
            false,
        );
        assert!(
            matches!(&output[0],Event::ToolCall{id,name,arguments} if id=="c1" && name=="bash" && arguments=="{\"command\":\"ls\"}")
        );
        assert_eq!(output.last(), Some(&Event::Done));
    }
    #[test]
    fn partial_streams_never_execute_tools() {
        let output = events(
            &[
                json!({"choices":[{"delta":{"tool_calls":[{"index":0,"id":"c1","function":{"name":"bash","arguments":"{}"}}]}}]}),
            ],
            false,
        );
        assert!(output.iter().any(|e| matches!(e, Event::Error(_))));
        assert!(
            !output
                .iter()
                .any(|e| matches!(e, Event::ToolCall { .. } | Event::Done))
        );
    }
    #[test]
    fn error_and_failed_events_are_fatal() {
        for v in [
            json!({"error":{"message":"bad key"}}),
            json!({"type":"response.failed","response":{"error":{"message":"bad key"}}}),
            json!({"type":"response.incomplete"}),
        ] {
            let output = events(&[v], false);
            assert!(matches!(&output[0], Event::Error(_)));
            assert!(!output.contains(&Event::Done));
        }
    }
    #[test]
    fn empty_stream_explains_finish_reason_usage_and_shape() {
        let output = events(
            &[
                json!({"id":"r-empty","model":"actual-model","choices":[{"delta":{"reasoning_content":"private reasoning"},"finish_reason":"length"}],"usage":{"prompt_tokens":40,"completion_tokens":20}}),
            ],
            true,
        );
        assert!(
            output
                .iter()
                .any(|e| matches!(e, Event::Usage(u) if u.output == 20))
        );
        let error = output
            .iter()
            .find_map(|e| {
                if let Event::Error(t) = e {
                    Some(t)
                } else {
                    None
                }
            })
            .unwrap();
        for field in [
            "output limit",
            "finish=length",
            "actual-model",
            "r-empty",
            "40 input / 20 output",
            "17 chars",
            "reasoning_content",
        ] {
            assert!(error.contains(field), "{error}");
        }
        assert!(!error.contains("private reasoning"));
        assert!(!output.contains(&Event::Done));
    }

    #[test]
    fn compatible_typed_text_and_refusals_are_visible() {
        for delta in [
            json!({"content":[{"type":"text","text":"visible"}]}),
            json!({"refusal":"visible"}),
        ] {
            let output = events(
                &[json!({"choices":[{"delta":delta,"finish_reason":"stop"}]})],
                true,
            );
            assert_eq!(output, vec![Event::Text("visible".into()), Event::Done]);
        }
    }

    #[test]
    fn chat_stream_is_finite() {
        assert_eq!(
            events(&[json!({"choices":[{"delta":{"content":"ok"}}]})], true),
            vec![Event::Text("ok".into()), Event::Done]
        );
    }
    #[test]
    fn read_failures_are_fatal() {
        struct Broken;
        impl Read for Broken {
            fn read(&mut self, _: &mut [u8]) -> std::io::Result<usize> {
                Err(std::io::Error::other("socket closed"))
            }
        }
        let output = StreamEvents::new(Broken).collect::<Vec<_>>();
        assert!(matches!(&output[0],Event::Error(s) if s.contains("socket closed")));
    }
    #[test]
    fn invalid_tool_arguments_are_delivered_for_agent_validation() {
        let output = events(
            &[
                json!({"choices":[{"delta":{"tool_calls":[{"id":"c","index":0,"function":{"name":"bash","arguments":"{"}}]}}]}),
            ],
            true,
        );
        assert!(matches!(&output[0], Event::ToolCall { arguments, .. } if arguments == "{"));
        assert!(matches!(output.last(), Some(Event::Done)));
    }
    #[test]
    fn responses_history_contains_request_and_result() {
        use glass_mod_api::Message;
        let history = [
            Message::ToolCall {
                id: "c".into(),
                name: "bash".into(),
                arguments: "{}".into(),
            },
            Message::ToolResult {
                call_id: "c".into(),
                name: "bash".into(),
                body: "ok".into(),
            },
        ];
        let wire = responses_input(&history);
        assert_eq!(wire[0]["type"], "function_call");
        assert_eq!(wire[1]["type"], "function_call_output");
        assert_eq!(wire[0]["call_id"], wire[1]["call_id"]);
        let chat = build_messages("sys", &history);
        assert_eq!(chat[1]["tool_calls"][0]["id"], "c");
    }
}

#[cfg(test)]
mod reasoning_tests {
    use super::*;
    #[test]
    fn public_summary_deltas_and_done_are_not_duplicated() {
        let mut s = StreamEvents::new(std::io::empty());
        s.consume(&json!({"type":"response.reasoning_summary_text.delta","item_id":"r1","summary_index":0,"delta":"Plan "}));
        s.consume(&json!({"type":"response.reasoning_summary_text.delta","item_id":"r1","summary_index":0,"delta":"changes"}));
        s.consume(&json!({"type":"response.reasoning_summary_text.done","item_id":"r1","summary_index":0,"text":"Plan changes"}));
        let text: String = s
            .outbox
            .iter()
            .filter_map(|e| {
                if let Event::Reasoning(t) = e {
                    Some(t.as_str())
                } else {
                    None
                }
            })
            .collect();
        assert_eq!(text, "Plan changes");
    }
    #[test]
    fn done_fallback_exposes_only_public_summary_and_compatible_text() {
        let mut s = StreamEvents::new(std::io::empty());
        s.consume(&json!({"type":"response.reasoning_summary_text.done","item_id":"r1","summary_index":0,"text":"Public summary"}));
        s.consume(&json!({"type":"response.output_item.done","item":{"type":"reasoning","encrypted_content":"SECRET","signature":"SECRET","summary":[]}}));
        s.consume(&json!({"choices":[{"delta":{"reasoning_content":"Compatible"}}]}));
        let text: String = s
            .outbox
            .iter()
            .filter_map(|e| {
                if let Event::Reasoning(t) = e {
                    Some(t.as_str())
                } else {
                    None
                }
            })
            .collect();
        assert_eq!(text, "Public summaryCompatible");
        assert!(!text.contains("SECRET"));
    }
}
