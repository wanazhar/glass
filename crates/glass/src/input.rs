//! A background key reader, so the UI stays live during a turn.
//!
//! `crossterm::event::read()` blocks, which is fine while idle and useless
//! while a model is streaming: the user could not type a steer, could not open
//! the transcript, and Ctrl-C would not interrupt. So one thread owns stdin and
//! pushes key events into a channel; every other consumer drains that channel
//! non-blockingly.
//!
//! Only one thread may read stdin at a time. The main loop never does, which is
//! the whole reason this works.

use std::io::Write;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, Sender, TryRecvError, channel};

use glass_tui::crossterm::event::Event;
use serde_json::{Map, Value, json};

const MAX_INPUT_BYTES: usize = 32 * 1024;
const MAX_FIELDS: usize = 16;

#[derive(Clone, Debug)]
pub struct ElicitationField {
    pub name: String,
    pub title: String,
    pub description: String,
    pub kind: String,
    pub required: bool,
    pub default: Option<Value>,
    pub choices: Vec<(Value, String)>,
    pub minimum: Option<f64>,
    pub maximum: Option<f64>,
    pub min_items: usize,
    pub max_items: usize,
    pub min_length: usize,
    pub max_length: usize,
    pub format: Option<String>,
}

#[derive(Clone, Debug)]
pub struct ElicitationRequest {
    pub server: String,
    pub message: String,
    pub mode: String,
    pub url: Option<String>,
    pub host: Option<String>,
    pub fields: Vec<ElicitationField>,
}

fn bounded_text(value: Option<&Value>, limit: usize) -> Result<String, String> {
    let text = value.and_then(Value::as_str).unwrap_or("");
    if text.len() > limit || text.chars().any(char::is_control) {
        return Err("input request contains invalid or oversized text".into());
    }
    Ok(text.to_string())
}

fn sensitive_name(name: &str) -> bool {
    let normalized = name.to_ascii_lowercase().replace(['_', '-'], " ");
    [
        "password",
        "secret",
        "token",
        "api key",
        "credential",
        "authorization",
        "private key",
    ]
    .iter()
    .any(|needle| normalized.contains(needle))
}

fn choices(schema: &Value) -> Result<Vec<(Value, String)>, String> {
    let mut out = Vec::new();
    if let Some(values) = schema.get("enum").and_then(Value::as_array) {
        for value in values {
            if !value.is_string() || value.as_str().unwrap_or("").len() > 256 {
                return Err("unsupported enum values".into());
            }
            out.push((value.clone(), value.as_str().unwrap_or("").to_string()));
        }
    } else if let Some(values) = schema.get("oneOf").and_then(Value::as_array) {
        for value in values {
            let constant = value
                .get("const")
                .filter(|v| v.is_string())
                .ok_or("unsupported titled enum")?;
            let title = bounded_text(value.get("title"), 256)?;
            if title.is_empty() {
                return Err("unsupported titled enum".into());
            }
            out.push((constant.clone(), title));
        }
    } else if let Some(values) = schema.pointer("/items/enum").and_then(Value::as_array) {
        for value in values {
            if !value.is_string() {
                return Err("unsupported multienum values".into());
            }
            out.push((value.clone(), value.as_str().unwrap_or("").to_string()));
        }
    } else if let Some(values) = schema.pointer("/items/anyOf").and_then(Value::as_array) {
        for value in values {
            let constant = value
                .get("const")
                .filter(|v| v.is_string())
                .ok_or("unsupported titled multienum")?;
            let title = bounded_text(value.get("title"), 256)?;
            if title.is_empty() {
                return Err("unsupported titled multienum".into());
            }
            out.push((constant.clone(), title));
        }
    }
    if !out.is_empty() && out.len() <= 32 {
        Ok(out)
    } else {
        Err("unsupported or oversized enum".into())
    }
}

impl ElicitationRequest {
    pub fn parse(server: &str, params: &Value) -> Result<Self, String> {
        let raw = serde_json::to_vec(params).map_err(|_| "invalid input request")?;
        if raw.len() > MAX_INPUT_BYTES {
            return Err("input request exceeds size limit".into());
        }
        let mode = params.get("mode").and_then(Value::as_str).unwrap_or("form");
        let message = bounded_text(params.get("message"), 2048)?;
        if message.is_empty() {
            return Err("input request message is required".into());
        }
        if mode == "url" {
            let url_text = bounded_text(params.get("url"), 4096)?;
            let url = url::Url::parse(&url_text).map_err(|_| "input URL is invalid")?;
            if url.scheme() != "https"
                || url.host_str().is_none()
                || !url.username().is_empty()
                || url.password().is_some()
            {
                return Err("input URL must be HTTPS and cannot contain credentials".into());
            }
            return Ok(Self {
                server: server.into(),
                message,
                mode: mode.into(),
                host: url.host_str().map(str::to_string),
                url: Some(url.to_string()),
                fields: vec![],
            });
        }
        if mode != "form" {
            return Err("unsupported input request mode".into());
        }
        let schema = params
            .get("requestedSchema")
            .filter(|v| v.is_object())
            .ok_or("form schema is required")?;
        if schema.as_object().unwrap().keys().any(|k| {
            !["type", "title", "description", "properties", "required"].contains(&k.as_str())
        }) {
            return Err("unsupported form schema keyword".into());
        }
        if schema.get("type").and_then(Value::as_str) != Some("object") {
            return Err("form schema must be a flat object".into());
        }
        let properties = schema
            .get("properties")
            .and_then(Value::as_object)
            .ok_or("form properties are required")?;
        if properties.is_empty() || properties.len() > MAX_FIELDS {
            return Err("form field count is outside supported limits".into());
        }
        let required: Vec<String> = schema
            .get("required")
            .map(|v| v.as_array().ok_or("required must be an array"))
            .transpose()?
            .into_iter()
            .flatten()
            .map(|v| {
                v.as_str()
                    .map(str::to_string)
                    .ok_or("required names must be strings")
            })
            .collect::<Result<_, _>>()?;
        if required.iter().any(|name| !properties.contains_key(name)) {
            return Err("required names must refer to declared fields".into());
        }
        let mut fields = Vec::new();
        for (name, spec) in properties {
            if name.len() > 128 || sensitive_name(name) || !spec.is_object() {
                return Err("sensitive or unsupported form field".into());
            }
            if spec.as_object().unwrap().keys().any(|k| {
                ![
                    "type",
                    "title",
                    "description",
                    "default",
                    "enum",
                    "oneOf",
                    "minimum",
                    "maximum",
                    "minLength",
                    "maxLength",
                    "format",
                    "minItems",
                    "maxItems",
                    "items",
                ]
                .contains(&k.as_str())
            }) {
                return Err("unsupported form field keyword".into());
            }
            let title = bounded_text(spec.get("title"), 256)?;
            let description = bounded_text(spec.get("description"), 512)?;
            let ty = spec.get("type").and_then(Value::as_str).unwrap_or("");
            let (kind, opts) = if ty == "array" {
                if spec
                    .pointer("/items/type")
                    .and_then(Value::as_str)
                    .is_some_and(|t| t != "string")
                {
                    return Err("only string multienums are supported".into());
                }
                ("multi_enum", choices(spec)?)
            } else {
                let kind = match ty {
                    "string" => "string",
                    "number" => "number",
                    "integer" => "integer",
                    "boolean" => "boolean",
                    _ => return Err("unsupported form field type".into()),
                };
                let opts = if spec.get("enum").is_some() || spec.get("oneOf").is_some() {
                    choices(spec)?
                } else {
                    vec![]
                };
                if kind != "string" && !opts.is_empty() {
                    return Err("enum selections must be strings".into());
                }
                (kind, opts)
            };
            if let Some(default) = spec.get("default") {
                if serde_json::to_vec(default).map_or(true, |b| b.len() > 1024) {
                    return Err("form default is too large".into());
                }
                let matches_type = match kind {
                    "string" => {
                        default.is_string()
                            && (opts.is_empty() || opts.iter().any(|(v, _)| v == default))
                    }
                    "number" => default.as_f64().is_some(),
                    "integer" => default.as_i64().is_some() || default.as_u64().is_some(),
                    "boolean" => default.is_boolean(),
                    "multi_enum" => default.as_array().is_some_and(|a| {
                        a.iter().all(|v| opts.iter().any(|(choice, _)| choice == v))
                    }),
                    _ => false,
                };
                if !matches_type {
                    return Err("form default does not match its field schema".into());
                }
            }
            let number = |key: &str| spec.get(key).and_then(Value::as_f64);
            let format = spec
                .get("format")
                .and_then(Value::as_str)
                .map(str::to_string);
            if format
                .as_deref()
                .is_some_and(|f| !["email", "uri", "date", "date-time"].contains(&f))
            {
                return Err("unsupported string format".into());
            }
            if !matches!(kind, "string" | "multi_enum") && format.is_some() {
                return Err("format applies only to strings".into());
            }
            let min_length = spec.get("minLength").and_then(Value::as_u64).unwrap_or(0) as usize;
            let max_length = spec
                .get("maxLength")
                .and_then(Value::as_u64)
                .unwrap_or(4096) as usize;
            let min_items = spec.get("minItems").and_then(Value::as_u64).unwrap_or(0) as usize;
            let max_items = spec.get("maxItems").and_then(Value::as_u64).unwrap_or(32) as usize;
            if min_length > max_length
                || max_length > 4096
                || min_items > max_items
                || max_items > 32
            {
                return Err("form field bounds are invalid".into());
            }
            fields.push(ElicitationField {
                name: name.clone(),
                title: if title.is_empty() {
                    name.clone()
                } else {
                    title
                },
                description,
                kind: kind.into(),
                required: required.contains(name),
                default: spec.get("default").cloned(),
                choices: opts,
                minimum: number("minimum"),
                maximum: number("maximum"),
                min_items,
                max_items,
                min_length,
                max_length,
                format,
            });
        }
        Ok(Self {
            server: server.into(),
            message,
            mode: mode.into(),
            url: None,
            host: None,
            fields,
        })
    }

    /// Presents one validated request through the existing key reader. Values are returned
    /// only to the caller; callers must not log or append this result to the transcript.
    pub fn present(&self, keys: &KeyReader) -> Result<Value, String> {
        use crate::modal;
        let title = format!("MCP input · {}", self.server);
        if self.mode == "url" {
            let host = self.host.as_deref().unwrap_or("unknown host");
            let choice = modal::choose(
                keys,
                &title,
                &format!(
                    "{}\nHTTPS destination: {host}\nGlass will not open this URL.",
                    self.message
                ),
                &[
                    "Consent to continue".into(),
                    "Decline".into(),
                    "Cancel".into(),
                ],
            );
            return Ok(
                json!({"action": match choice { Some(0) => "accept", Some(2) => "cancel", _ => "decline" }}),
            );
        }
        let mut content = Map::new();
        for field in &self.fields {
            let value = if !field.choices.is_empty() && field.kind == "string" {
                let labels: Vec<String> = field
                    .choices
                    .iter()
                    .map(|(_, label)| label.clone())
                    .collect();
                let note = format!("{} · {}", self.message, field.description);
                let selected =
                    modal::choose(keys, &format!("{title} · {}", field.title), &note, &labels);
                match selected {
                    Some(i) => field.choices[i].0.clone(),
                    None => return Ok(json!({"action":"cancel"})),
                }
            } else if field.kind == "boolean" {
                match modal::choose(
                    keys,
                    &format!("{title} · {}", field.title),
                    &field.description,
                    &["No".into(), "Yes".into()],
                ) {
                    Some(0) => json!(false),
                    Some(_) => json!(true),
                    None => return Ok(json!({"action":"cancel"})),
                }
            } else {
                let initial = field
                    .default
                    .as_ref()
                    .map(|v| {
                        if field.kind == "multi_enum" {
                            v.as_array()
                                .map(|a| {
                                    a.iter()
                                        .filter_map(Value::as_str)
                                        .collect::<Vec<_>>()
                                        .join(", ")
                                })
                                .unwrap_or_default()
                        } else {
                            v.as_str()
                                .map(str::to_string)
                                .unwrap_or_else(|| v.to_string())
                        }
                    })
                    .unwrap_or_default();
                let answer = modal::field(
                    keys,
                    &format!("{title} · {}", field.title),
                    &field.description,
                    &initial,
                    false,
                );
                let Some(answer) = answer else {
                    return Ok(json!({"action":"cancel"}));
                };
                let value = if field.kind == "number" || field.kind == "integer" {
                    let n: f64 = answer
                        .trim()
                        .parse()
                        .map_err(|_| format!("{} must be a number", field.title))?;
                    if field.kind == "integer" && n.fract() != 0.0
                        || field.minimum.is_some_and(|m| n < m)
                        || field.maximum.is_some_and(|m| n > m)
                    {
                        return Err(format!("{} is outside the allowed range", field.title));
                    }
                    json!(n)
                } else if field.kind == "multi_enum" {
                    let selected: Vec<String> = answer
                        .split(',')
                        .map(str::trim)
                        .filter(|s| !s.is_empty())
                        .map(|s| {
                            field
                                .choices
                                .iter()
                                .find(|(value, label)| value.as_str() == Some(s) || label == s)
                                .map(|(value, _)| value.as_str().unwrap_or(s).to_string())
                                .unwrap_or_else(|| s.to_string())
                        })
                        .collect();
                    if selected.len() < field.min_items
                        || selected.len() > field.max_items
                        || selected
                            .iter()
                            .any(|s| !field.choices.iter().any(|(v, _)| v.as_str() == Some(s)))
                    {
                        return Err(format!("{} must contain allowed selections", field.title));
                    }
                    json!(selected)
                } else {
                    let trimmed = answer.trim();
                    let length = answer.chars().count();
                    if length < field.min_length || length > field.max_length {
                        return Err(format!(
                            "{} must be between {} and {} characters",
                            field.title, field.min_length, field.max_length
                        ));
                    }
                    if let Some(format) = field.format.as_deref() {
                        let valid = match format {
                            "email" => trimmed.split_once('@').is_some_and(|(local, domain)| {
                                !local.is_empty()
                                    && domain.contains('.')
                                    && !domain.starts_with('.')
                                    && !domain.ends_with('.')
                            }),
                            "uri" => url::Url::parse(trimmed).is_ok(),
                            "date" => {
                                trimmed.len() == 10
                                    && trimmed.chars().enumerate().all(|(i, c)| {
                                        if [4, 7].contains(&i) {
                                            c == '-'
                                        } else {
                                            c.is_ascii_digit()
                                        }
                                    })
                            }
                            "date-time" => {
                                trimmed.contains('T')
                                    && (trimmed.ends_with('Z')
                                        || trimmed.rfind(['+', '-']).is_some_and(|i| i > 10))
                            }
                            _ => false,
                        };
                        if !valid {
                            return Err(format!(
                                "{} does not match the requested format",
                                field.title
                            ));
                        }
                    }
                    json!(answer)
                };
                if field.required && (value.is_null() || value.as_str().is_some_and(str::is_empty))
                {
                    return Err(format!("{} is required", field.title));
                }
                value
            };
            content.insert(field.name.clone(), value);
        }
        let summary = content
            .iter()
            .map(|(k, v)| format!("{k}: {v}"))
            .collect::<Vec<_>>()
            .join("\n");
        let answer = modal::choose(
            keys,
            &format!("Review · {}", self.server),
            &summary,
            &["Submit response".into(), "Decline".into(), "Cancel".into()],
        );
        match answer {
            Some(0) => Ok(json!({"action":"accept", "content":content})),
            Some(2) => Ok(json!({"action":"cancel"})),
            _ => Ok(json!({"action":"decline"})),
        }
    }
}

/// Build the per-call callback used by extension dispatch. Invalid or unsupported
/// requests receive an explicit decline and are never surfaced as tool arguments.
pub fn mcp_input_callback(keys: &KeyReader) -> impl FnMut(&str, &Value) -> Value + '_ {
    move |server, params| {
        ElicitationRequest::parse(server, params)
            .and_then(|request| request.present(keys))
            .unwrap_or_else(|_| json!({"action":"decline"}))
    }
}

/// Handle to the reader thread.
pub struct KeyReader {
    rx: Receiver<Event>,
    stop: Arc<AtomicBool>,
    /// Kept so the thread outlives every clone of this struct.
    _handle: Option<std::thread::JoinHandle<()>>,
}

impl KeyReader {
    /// Spawn the reader. Should be called once, after raw mode is enabled.
    pub fn spawn() -> Self {
        let (tx, rx): (Sender<Event>, Receiver<Event>) = channel();
        let stop = Arc::new(AtomicBool::new(false));
        let stop_thread = Arc::clone(&stop);

        let handle = std::thread::spawn(move || {
            loop {
                if stop_thread.load(Ordering::Relaxed) {
                    return;
                }
                // `poll` with a timeout rather than a blocking read, so the stop
                // flag is observed promptly instead of only after the next key.
                match glass_tui::crossterm::event::poll(std::time::Duration::from_millis(120)) {
                    Ok(true) => match glass_tui::crossterm::event::read() {
                        Ok(ev) => {
                            if tx.send(ev).is_err() {
                                return; // receiver dropped, reader is useless
                            }
                        }
                        Err(e) => {
                            let _ =
                                writeln!(std::io::stderr(), "\r\x1b[31minput error: {e}\x1b[0m");
                            return;
                        }
                    },
                    Ok(false) => {}
                    Err(_) => return,
                }
            }
        });

        Self {
            rx,
            stop,
            _handle: Some(handle),
        }
    }

    /// Take a pending event without blocking.
    pub fn try_next(&self) -> Option<Event> {
        match self.rx.try_recv() {
            Ok(ev) => Some(ev),
            Err(TryRecvError::Empty) => None,
            Err(TryRecvError::Disconnected) => None,
        }
    }

    pub fn next_timeout(&self, timeout: std::time::Duration) -> Option<Event> {
        self.rx.recv_timeout(timeout).ok()
    }

    pub fn wait_timeout(
        &self,
        timeout: std::time::Duration,
    ) -> Result<Option<Event>, std::sync::mpsc::RecvTimeoutError> {
        match self.rx.recv_timeout(timeout) {
            Ok(event) => Ok(Some(event)),
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => Ok(None),
            Err(error) => Err(error),
        }
    }

    /// Block until the next event. Only for the idle path, where blocking is
    /// what we want and saves a spin.
    pub fn next_blocking(&self) -> Option<Event> {
        self.rx.recv().ok()
    }

    /// Stop the reader. Called before leaving raw mode so the thread cannot
    /// read from a terminal that is about to be handed back.
    pub fn stop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(h) = self._handle.take() {
            let _ = h.join();
        }
    }
}

impl Drop for KeyReader {
    fn drop(&mut self) {
        self.stop();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn try_next_is_empty_when_nothing_pending() {
        let (_tx, rx): (Sender<Event>, Receiver<Event>) = channel();
        let reader = KeyReader {
            rx,
            stop: Arc::new(AtomicBool::new(false)),
            _handle: None,
        };
        assert!(reader.try_next().is_none());
    }

    #[test]
    fn try_next_drains_a_queued_event() {
        let (tx, rx): (Sender<Event>, Receiver<Event>) = channel();
        let reader = KeyReader {
            rx,
            stop: Arc::new(AtomicBool::new(false)),
            _handle: None,
        };
        tx.send(Event::FocusGained).unwrap();
        assert!(reader.try_next().is_some());
        assert!(
            reader.try_next().is_none(),
            "must not redeliver the same event"
        );
    }

    #[test]
    fn elicitation_accepts_flat_primitive_and_titled_multi_enum_schemas() {
        let request = json!({
            "mode":"form", "message":"Choose settings",
            "requestedSchema":{"type":"object","properties":{
                "label":{"type":"string","title":"Label","minLength":1},
                "count":{"type":"integer","minimum":1,"maximum":4,"default":2},
                "enabled":{"type":"boolean","default":true},
                "colors":{"type":"array","minItems":1,"maxItems":2,"items":{"anyOf":[
                    {"const":"r","title":"Red"},{"const":"b","title":"Blue"}
                ]}}
            },"required":["label"]}
        });
        let parsed = ElicitationRequest::parse("fixture", &request).unwrap();
        assert_eq!(parsed.fields.len(), 4);
        assert_eq!(
            parsed
                .fields
                .iter()
                .find(|f| f.name == "colors")
                .unwrap()
                .choices[0]
                .1,
            "Red"
        );
    }

    #[test]
    fn elicitation_rejects_credentials_unsupported_schema_and_non_https_url() {
        let secret = json!({"message":"Authorize", "requestedSchema":{"type":"object","properties":{"api_key":{"type":"string"}}}});
        assert!(ElicitationRequest::parse("fixture", &secret).is_err());
        let nested = json!({"message":"Nested", "requestedSchema":{"type":"object","properties":{"x":{"type":"object"}}}});
        assert!(ElicitationRequest::parse("fixture", &nested).is_err());
        let insecure =
            json!({"mode":"url", "message":"Continue", "url":"http://example.test/auth"});
        assert!(ElicitationRequest::parse("fixture", &insecure).is_err());
    }
}
