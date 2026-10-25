//! Versioned host-neutral runtime. Opt-in, bounded JSON-RPC over stdio.
//! A single process runs one turn at a time; separate processes isolate agents.
use crate::{
    agent::{Agent, Progress, Stop},
    config, ext,
    session::{Record, Session},
    tools,
};
use serde_json::{Value, json};
use std::cell::RefCell;
use std::collections::{BTreeMap, VecDeque};
use std::fs::OpenOptions;
use std::io::{BufRead, Read, Write};
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::mpsc::{self, Receiver};
use std::time::Instant;

#[path = "durable_runtime.rs"]
mod durable_host;
#[path = "runtime_support.rs"]
mod support;

const MAX_FRAME: usize = 1024 * 1024;
const MAX_PENDING: usize = 32;
type Schemas = Vec<(String, String, String)>;

fn session_turn_index(through: u64) -> Result<usize, &'static str> {
    checked_unsigned_index(through).ok_or("throughTurn exceeds platform capacity")
}

fn checked_unsigned_index<T: TryFrom<u64>>(value: u64) -> Option<T> {
    T::try_from(value).ok()
}

/// Optional privacy-safe timing trace for diagnosing a stalled host runtime.
/// Records only fixed frame classes, byte counts, and write durations.
fn trace_runtime_frame(
    component: &str,
    phase: &str,
    class: &str,
    bytes: usize,
    elapsed_us: Option<u128>,
    correlation_hash: Option<String>,
) {
    let Some(path) = std::env::var_os("GLASS_RUNTIME_DIAGNOSTIC_TRACE") else {
        return;
    };
    let path = std::path::PathBuf::from(path).with_extension(format!("{component}.jsonl"));
    let Ok(mut file) = OpenOptions::new().create(true).append(true).open(path) else {
        return;
    };
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Ok(metadata) = file.metadata() {
            let mut permissions = metadata.permissions();
            permissions.set_mode(0o600);
            let _ = file.set_permissions(permissions);
        }
    }
    let mut record = json!({
        "component": component,
        "phase": phase,
        "class": class,
        "bytes": bytes,
        "elapsed_us": elapsed_us,
    });
    if let Some(correlation_hash) = correlation_hash {
        record["correlation_hash"] = json!(correlation_hash);
    }
    let _ = writeln!(file, "{record}");
}

fn runtime_correlation_hash(value: &Value) -> Option<String> {
    let id = value
        .get("id")
        .or_else(|| value.pointer("/params/turnId"))
        .or_else(|| value.pointer("/params/id"))?;
    Some(crate::hashing::sha256(id.to_string().as_bytes()))
}

fn trace_turn_phase(phase: &str, class: &str, turn: &Value, elapsed_us: Option<u128>) {
    let hash = crate::hashing::sha256(turn.to_string().as_bytes());
    trace_runtime_frame("core_turn", phase, class, 0, elapsed_us, Some(hash));
}

fn persist_usage(
    session: &mut Session,
    usage: &crate::provider::Usage,
    turn: &Value,
) -> Result<(), String> {
    let started = Instant::now();
    trace_turn_phase("usage_persist_begin", "provider_usage", turn, None);
    let record = Record::Custom {
        kind: "runtime/usage".into(),
        data: Some(
            json!({
                "input": usage.input,
                "output": usage.output,
                "cacheRead": usage.cache_read,
                "cacheWrite": usage.cache_write
            })
            .to_string(),
        ),
    };
    match session.append_checked(&[record]) {
        Ok(()) => {
            trace_turn_phase(
                "usage_persist_complete",
                "provider_usage",
                turn,
                Some(started.elapsed().as_micros()),
            );
            Ok(())
        }
        Err(error) => {
            trace_turn_phase(
                "usage_persist_error",
                "provider_usage",
                turn,
                Some(started.elapsed().as_micros()),
            );
            Err(error.to_string())
        }
    }
}

fn runtime_frame_class(value: &Value) -> &'static str {
    match value.get("method").and_then(Value::as_str) {
        Some("runtime/event") => match value.pointer("/params/type").and_then(Value::as_str) {
            Some("usage") => "event:usage",
            Some("settled") => "event:settled",
            Some("text") => "event:text",
            Some("reasoning") => "event:reasoning",
            Some("tool_start") => "event:tool_start",
            Some("tool_end") => "event:tool_end",
            Some("tool_progress") => "event:tool_progress",
            Some("turn_start") => "event:turn_start",
            _ => "event:other",
        },
        Some("tool/execute") => "host_tool_request",
        Some(_) => "rpc_request",
        None if value.get("id").is_some() && value.get("error").is_some() => "rpc_error",
        None if value.get("id").is_some() => "rpc_response",
        _ => "other",
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ToolMode {
    Host,
    Local,
    Mixed,
}
impl ToolMode {
    fn as_str(self) -> &'static str {
        match self {
            Self::Host => "host",
            Self::Local => "local",
            Self::Mixed => "mixed",
        }
    }
}
fn validate_tool_mode(protocol_version: Option<u64>, mode: ToolMode) -> Result<(), String> {
    if mode == ToolMode::Mixed && protocol_version != Some(2) {
        Err("toolMode mixed requires initialize protocol v2 with mixedTools".into())
    } else {
        Ok(())
    }
}
fn tool_owners(mode: ToolMode, tools: Option<&Schemas>) -> Value {
    json!({
        "core": {"nativeTools": mode != ToolMode::Host},
        "host": {
            "namespace": if mode == ToolMode::Mixed { Some("glass.") } else { None },
            "tools": tools.into_iter().flatten().map(|(name, _, _)| name).collect::<Vec<_>>(),
        }
    })
}

fn send(value: Value) -> bool {
    let encoded = value.to_string();
    if encoded.len() + 1 > MAX_FRAME {
        return false;
    }
    let class = runtime_frame_class(&value);
    let correlation_hash = runtime_correlation_hash(&value);
    trace_runtime_frame(
        "core_stdout",
        "write_begin",
        class,
        encoded.len() + 1,
        None,
        correlation_hash.clone(),
    );
    let stdout = std::io::stdout();
    let write_started = Instant::now();
    let mut out = stdout.lock();
    // Blocking writes propagate backpressure rather than accumulating events.
    if writeln!(out, "{encoded}")
        .and_then(|_| out.flush())
        .is_err()
    {
        trace_runtime_frame(
            "core_stdout",
            "write_error",
            class,
            encoded.len() + 1,
            Some(write_started.elapsed().as_micros()),
            correlation_hash,
        );
        std::process::exit(0);
    }
    trace_runtime_frame(
        "core_stdout",
        "write_complete",
        class,
        encoded.len() + 1,
        Some(write_started.elapsed().as_micros()),
        correlation_hash,
    );
    true
}
fn reply(id: &Value, result: Value) {
    if !send(json!({"jsonrpc":"2.0","id":id,"result":result})) {
        error(id, -32002, "response exceeds 1 MiB; request a smaller page");
    }
}
fn error(id: &Value, code: i32, message: impl AsRef<str>) {
    let message = message.as_ref();
    let mut end = message.len().min(4096);
    while !message.is_char_boundary(end) {
        end -= 1;
    }
    let id = if id.as_str().is_some_and(|s| s.len() > 128) {
        &Value::Null
    } else {
        id
    };
    send(
        json!({"jsonrpc":"2.0","id":id,"error":{"code":code,"message":&message[..end],"data":{"kind":error_kind(code,message),"retryable":message.contains("429") || message.contains("cooldown")}}}),
    );
}
fn event(session: &str, turn: &Value, kind: &str, mut data: Value) {
    // Preserve every text byte while bounding escaped JSON (up to six bytes
    // per input byte). Chunk metadata lets hosts reassemble tool-end bodies.
    if let Some(text) = data["text"]
        .as_str()
        .filter(|text| text.len() > 64 * 1024)
        .map(str::to_owned)
    {
        let mut offset = 0;
        while offset < text.len() {
            let mut end = (offset + 64 * 1024).min(text.len());
            while !text.is_char_boundary(end) {
                end -= 1;
            }
            data["text"] = json!(&text[offset..end]);
            data["chunkOffset"] = json!(offset);
            data["chunkFinal"] = json!(end == text.len());
            event(session, turn, kind, data.clone());
            offset = end;
        }
        return;
    }
    let value = json!({"jsonrpc":"2.0","method":"runtime/event","params":{"sessionId":session,"turnId":turn,"type":kind,"data":data}});
    if !send(value) {
        send(
            json!({"jsonrpc":"2.0","method":"runtime/event","params":{"sessionId":session,"turnId":turn,"type":"error",
            "data":{"message":"non-text event metadata exceeds frame limit","originalType":kind}}}),
        );
    }
}
fn string<'a>(v: &'a Value, key: &str) -> Result<&'a str, String> {
    v.get(key)
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| format!("{key} must be a nonempty string"))
}
fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 128
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.'))
}
fn schemas(value: &Value) -> Result<Schemas, String> {
    const MAX_HOST_TOOLS: usize = 512;
    let entries = value.as_array().ok_or("tools must be an array")?;
    let mut out: Schemas = Vec::new();
    if entries.len() > MAX_HOST_TOOLS {
        return Err(format!("at most {MAX_HOST_TOOLS} tools may be registered"));
    }
    for entry in entries {
        let name = string(entry, "name")?;
        if !valid_id(name) || out.iter().any(|(n, _, _)| n == name) {
            return Err("tool names must be unique ASCII identifiers".into());
        }
        let schema = entry
            .get("inputSchema")
            .filter(|s| s.get("type").and_then(Value::as_str) == Some("object"))
            .ok_or("inputSchema must describe an object")?;
        crate::tool_repair::schema_shape(schema, 0)?;
        out.push((
            name.into(),
            entry
                .get("description")
                .and_then(Value::as_str)
                .unwrap_or("")
                .into(),
            schema.to_string(),
        ));
    }
    Ok(out)
}
fn mixed_schemas(
    value: &Value,
    native: &std::collections::HashSet<String>,
    extensions: &std::collections::HashSet<String>,
) -> Result<Schemas, String> {
    let registered = schemas(value)?;
    if registered
        .iter()
        .any(|(name, _, _)| !name.starts_with("glass."))
    {
        return Err("mixed-mode host tools must use the glass. namespace".into());
    }
    if registered.iter().any(|(name, _, _)| native.contains(name)) {
        return Err("mixed-mode host tool collides with a Core tool".into());
    }
    if registered
        .iter()
        .any(|(name, _, _)| extensions.contains(name))
    {
        return Err("mixed-mode host tool collides with a Core extension".into());
    }
    Ok(registered)
}
struct Wire {
    rx: Receiver<Result<Value, String>>,
    pending: VecDeque<Value>,
    disconnected: bool,
    steer: Option<String>,
    steer_records: Vec<Record>,
    image_enabled: bool,
    completing: bool,
    cancel_grace_ms: u64,
    durable: Option<Rc<RefCell<crate::durable::Store>>>,
    durable_task: Option<String>,
    protocol_version: Option<u64>,
}
impl Wire {
    fn incoming(&mut self) -> Option<Value> {
        match self.rx.try_recv() {
            Ok(Ok(v)) => Some(v),
            Ok(Err(e)) => {
                error(&Value::Null, -32700, e);
                None
            }
            Err(mpsc::TryRecvError::Disconnected) => {
                self.disconnected = true;
                None
            }
            Err(_) => None,
        }
    }
    fn active(&mut self, m: Value, session: &str, turn: &Value) {
        let id = m.get("id").cloned().unwrap_or(Value::Null);
        trace_runtime_frame(
            "core_stdin",
            "frame_received",
            runtime_frame_class(&m),
            m.to_string().len(),
            None,
            runtime_correlation_hash(&m),
        );
        let p = &m["params"];
        if self.completing
            && !matches!(
                m["method"].as_str(),
                Some("turn/cancel" | "completion/cancel")
            )
        {
            error(
                &id,
                -32001,
                "completion is active; only cancellation is accepted",
            );
            return;
        }
        if let Some(store) = &self.durable {
            let result = match m["method"].as_str() {
                Some("durable/changes") => Some(store.borrow().changes(
                    p["cursor"].as_u64().unwrap_or(0),
                    p["limit"].as_u64().unwrap_or(25).clamp(1, 256) as usize,
                )),
                Some("durable/taskGraph") => Some(Ok(store.borrow().task_graph())),
                Some("durable/abort") => Some((|| {
                    let task = string(p, "taskId")?;
                    let mut store = store.borrow_mut();
                    let seq = store.state.sequence;
                    store.commit(
                        seq,
                        vec![crate::durable::Op::Abort {
                            id: task.into(),
                            include_background: p["includeBackground"].as_bool().unwrap_or(false),
                        }],
                    )?;
                    Ok(json!({"accepted":true}))
                })()),
                _ => None,
            };
            if let Some(result) = result {
                match result {
                    Ok(value) => reply(&id, value),
                    Err(message) => error(&id, -32602, message),
                }
                return;
            }
        }
        match m["method"].as_str() {
            Some("turn/cancel" | "turn/steer" | "completion/cancel")
                if p["sessionId"].as_str() == Some(session) =>
            {
                if p.get("turnId").is_some_and(|v| v != turn) {
                    error(&id, -32602, "turnId does not identify the active turn");
                    return;
                }
                let text: Result<String, String> = if m["method"] != "turn/steer" {
                    Ok(String::new())
                } else {
                    match support::input(p, self.image_enabled) {
                        Ok(mut records) => {
                            let text = match records.remove(0) {
                                Record::User(text) => text,
                                _ => unreachable!(),
                            };
                            // The pending correction owns its attached data.
                            if self.steer.is_none() {
                                self.steer_records = records;
                            }
                            Ok(text)
                        }
                        Err(e) => {
                            error(&id, -32602, e);
                            return;
                        }
                    }
                };
                match text {
                    Ok(text) => {
                        if self.steer.is_some() {
                            error(
                                &id,
                                -32001,
                                "a correction or cancellation is already pending",
                            );
                        } else {
                            self.steer = Some(text);
                            reply(
                                &id,
                                json!({"accepted":true,"applies":"before_next_request_or_tool"}),
                            );
                        }
                    }
                    Err(e) => error(&id, -32602, e),
                }
            }
            Some("turn/followUp") if p["sessionId"].as_str() == Some(session) => {
                if self.pending.len() >= MAX_PENDING {
                    error(&id, -32001, "follow-up queue is full");
                } else {
                    event(session, turn, "queued", json!({"requestId":id}));
                    self.pending.push_back(m);
                }
            }
            _ => error(
                &id,
                -32001,
                "runtime is busy; only steering, cancellation, follow-up and matching interaction responses are accepted",
            ),
        }
    }
    fn pump(&mut self, slot: &mut Option<String>, session: &str, turn: &Value) {
        while let Some(m) = self.incoming() {
            self.active(m, session, turn);
        }
        let durable_cancel = self.durable.as_ref().is_some_and(|store| {
            store.borrow().is_poisoned()
                || self.durable_task.as_ref().is_some_and(|id| {
                    store
                        .borrow()
                        .state
                        .tasks
                        .get(id)
                        .is_some_and(|task| task.abort_requested)
                })
        });
        if self.disconnected || durable_cancel {
            self.steer = None;
            *slot = Some(String::new());
        } else if let Some(text) = self.steer.take() {
            *slot = Some(text);
        }
    }
    fn tool(
        &mut self,
        call: &glass_mod_api::ToolCall,
        session: &str,
        turn: &Value,
        sequence: u64,
    ) -> crate::agent::HostToolOutcome {
        use crate::agent::HostToolOutcome;
        let request = format!("host:{session}:{sequence}:{}", call.id);
        let plain = |message: &str| HostToolOutcome::plain(tools::Outcome::err(message));
        if call.id.len() > 256 {
            return plain("Provider tool call ID exceeds 256 bytes");
        }
        if !send(
            json!({"jsonrpc":"2.0","id":request,"method":"tool/execute","params":{
            "sessionId":session,"turnId":turn,"callId":call.id,"name":call.name,
            "arguments":serde_json::from_str::<Value>(&call.arguments).unwrap_or(Value::Null)}}),
        ) {
            return plain("Host tool request exceeds 1 MiB; not dispatched");
        }
        let mut deadline = None;
        let mut progress_bytes = 0usize;
        let mut progress_updates = 0usize;
        let mut progress_limited = false;
        let mut execution_status = "unknown";
        let mut output = loop {
            if self.disconnected {
                break plain(
                    "Host disconnected; tool effects are unknown. Reconcile before repeating mutations.",
                );
            }
            let durable_abort = self.durable.as_ref().is_some_and(|shared| {
                let store = shared.borrow();
                store.is_poisoned()
                    || self.durable_task.as_ref().is_some_and(|id| {
                        store
                            .state
                            .tasks
                            .get(id)
                            .is_some_and(|task| task.abort_requested)
                    })
            });
            if durable_abort {
                self.steer = Some(String::new());
            }
            let tool_abort = self.durable.as_ref().is_some_and(|shared| {
                shared.borrow().state.tasks.values().any(|task| {
                    task.kind == "gc.tool"
                        && task.owner == self.durable_task
                        && task.input["callId"].as_str() == Some(&call.id)
                        && task.abort_requested
                })
            });
            if (self.steer.is_some() || tool_abort) && deadline.is_none() {
                event(
                    session,
                    turn,
                    "tool_cancel",
                    json!({"callId":call.id,"requestId":request,"acknowledgementRequired":true}),
                );
                deadline = Some(
                    std::time::Instant::now()
                        + std::time::Duration::from_millis(self.cancel_grace_ms),
                );
            }
            if deadline.is_some_and(|at| std::time::Instant::now() >= at) {
                break plain(
                    "Host tool interruption was not acknowledged; effects are unknown. Reconcile before repeating mutations.",
                );
            }
            match self.rx.recv_timeout(std::time::Duration::from_millis(10)) {
                Ok(Ok(m)) if m.get("method").is_none() && m["id"] == request => {
                    if m.get("result").is_some() == m.get("error").is_some() {
                        break plain(
                            "Host response requires exactly one of result or error; execution status is unknown",
                        );
                    }
                    if let Some(e) = m.get("error") {
                        execution_status = "failed";
                        break plain(&format!("Host execution failed: {e}"));
                    }
                    match support::tool_result(&m["result"], self.image_enabled) {
                        Ok(result) => {
                            execution_status = if result.outcome.exit_code == Some(0) {
                                "completed"
                            } else {
                                "failed"
                            };
                            break result;
                        }
                        Err(e) => break plain(&format!("{e}; execution status is unknown")),
                    }
                }
                Ok(Ok(m))
                    if m["method"] == "tool/progress" && m["params"]["requestId"] == request =>
                {
                    let id = m.get("id");
                    if m["params"]["sessionId"].as_str() != Some(session) {
                        if let Some(id) = id {
                            error(id, -32602, "sessionId mismatch");
                        }
                        continue;
                    }
                    if let Some(text) = m["params"]["text"].as_str().filter(|text| !text.is_empty())
                    {
                        if progress_updates >= 1024
                            || text.len() > 64 * 1024
                            || progress_bytes.saturating_add(text.len()) > 512 * 1024
                        {
                            if !progress_limited {
                                event(
                                    session,
                                    turn,
                                    "tool_progress",
                                    json!({"callId":call.id,"requestId":request,"truncated":true}),
                                );
                                progress_limited = true;
                            }
                            if let Some(id) = id {
                                error(id, -32002, "tool progress limit reached");
                            }
                        } else {
                            progress_bytes += text.len();
                            progress_updates += 1;
                            event(
                                session,
                                turn,
                                "tool_progress",
                                json!({"callId":call.id,"requestId":request,"text":text}),
                            );
                            if let Some(id) = id {
                                reply(id, json!({"accepted":true}));
                            }
                        }
                    } else if let Some(id) = id {
                        error(id, -32602, "progress requires text");
                    }
                }
                Ok(Ok(m))
                    if m["method"] == "tool/cancelled" && m["params"]["requestId"] == request =>
                {
                    let id = m.get("id").cloned().unwrap_or(Value::Null);
                    if deadline.is_none() || m["params"]["sessionId"].as_str() != Some(session) {
                        error(&id, -32602, "no matching cancellation is pending");
                        continue;
                    }
                    let status = m["params"]["executionStatus"].as_str().unwrap_or("");
                    if !matches!(status, "cancelled" | "completed" | "unknown") {
                        error(&id, -32602, "invalid cancellation status");
                        continue;
                    }
                    execution_status = match status {
                        "cancelled" => "cancelled",
                        "completed" => "completed",
                        _ => "unknown",
                    };
                    reply(&id, json!({"recorded":true,"executionStatus":status}));
                    event(
                        session,
                        turn,
                        "tool_cancel_ack",
                        json!({"callId":call.id,"requestId":request,"executionStatus":status}),
                    );
                    break plain(if status == "cancelled" {
                        "Host confirmed cancellation."
                    } else if status == "completed" {
                        "Host reports tool already completed; result unavailable. Do not repeat its effects."
                    } else {
                        "Host could not determine tool effects. Reconcile before repeating mutations."
                    });
                }
                Ok(Ok(m)) => self.active(m, session, turn),
                Ok(Err(e)) => error(&Value::Null, -32700, e),
                Err(mpsc::RecvTimeoutError::Disconnected) => self.disconnected = true,
                Err(_) => {}
            }
        };
        output.attachments.push(Record::Custom {
            kind: "runtime/tool_metadata".into(),
            data: Some(
                json!({
            "requestId":request,"callId":call.id,"turnId":turn,"name":call.name,
            "executionStatus":execution_status,"isError":output.outcome.exit_code!=Some(0)})
                .to_string(),
            ),
        });
        event(
            session,
            turn,
            "tool_status",
            json!({"requestId":request,"callId":call.id,"executionStatus":execution_status}),
        );
        output
    }
}
struct Conversation {
    session: Session,
    cfg: config::Config,
    state: tools::State,
    tools: Option<Schemas>,
    tool_mode: ToolMode,
    instructions: String,
    stats_cache: Option<(u64, Value)>,
    durable: Option<durable_host::Context>,
}
fn metadata(
    p: &Value,
    cfg: &config::Config,
) -> Result<(config::Config, Option<Schemas>, ToolMode, String), String> {
    let mut cfg = cfg.clone();
    if let Some(id) = p.get("connection").and_then(Value::as_str) {
        let store = crate::auth::load()?;
        let selected = store
            .connections
            .iter()
            .find(|c| c.id == id)
            .ok_or("unknown connection")?;
        crate::auth::configure(&mut cfg, selected);
    }
    let cwd = PathBuf::from(string(p, "cwd")?);
    if !cwd.is_absolute() || !cwd.is_dir() {
        return Err("cwd must be an existing absolute directory".into());
    }
    cfg.workspace = cwd.canonicalize().map_err(|e| e.to_string())?;
    if let Some(image) = p.get("imageInput") {
        cfg.image_input = image.as_bool().ok_or("imageInput must be boolean")?;
    }
    if let Some(tokens) = p.get("maxTokens") {
        cfg.max_tokens = u32::try_from(
            tokens
                .as_u64()
                .filter(|n| *n > 0 && *n <= 128_000)
                .ok_or("maxTokens must be 1..128000")?,
        )
        .map_err(|e| e.to_string())?;
    }
    if let Some(model) = p.get("model") {
        cfg.model = model
            .as_str()
            .filter(|s| !s.is_empty())
            .ok_or("model must be a nonempty string")?
            .into();
    }
    if let Some(budget) = p.get("reasoningBudget") {
        cfg.reasoning_budget = Some(
            u32::try_from(
                budget
                    .as_u64()
                    .ok_or("reasoningBudget must be an unsigned integer")?,
            )
            .map_err(|e| e.to_string())?,
        );
    }
    let mode = match p.get("toolMode").and_then(Value::as_str).unwrap_or("host") {
        "host" => ToolMode::Host,
        "local" => {
            if p.get("tools").is_some() {
                return Err("local mode cannot register host tools".into());
            }
            ToolMode::Local
        }
        "mixed" => ToolMode::Mixed,
        _ => return Err("toolMode must be host, local, or mixed".into()),
    };
    let empty_catalog = json!([]);
    let catalog = p.get("tools").unwrap_or(&empty_catalog);
    let host_tools = if mode == ToolMode::Local {
        None
    } else {
        let registered = if mode == ToolMode::Mixed {
            let native = crate::provider::collect_tools(&[])
                .into_iter()
                .map(|(name, _, _)| name)
                .collect::<std::collections::HashSet<_>>();
            let mut extension_registry = ext::Registry::load(&[], &cfg.workspace, &cfg.model);
            let extensions = extension_registry
                .declared_tools()
                .0
                .into_iter()
                .map(|(name, _, _)| name)
                .collect::<std::collections::HashSet<_>>();
            mixed_schemas(catalog, &native, &extensions)?
        } else {
            schemas(catalog)?
        };
        Some(registered)
    };
    for (key, target) in [
        ("provider", &mut cfg.provider),
        ("baseUrl", &mut cfg.base_url),
    ] {
        if let Some(v) = p.get(key) {
            *target = v
                .as_str()
                .filter(|s| !s.is_empty())
                .ok_or_else(|| format!("{key} must be a nonempty string"))?
                .into();
        }
    }
    for (key, target) in [
        ("connection", &mut cfg.connection),
        ("apiKeyEnv", &mut cfg.api_key_env),
    ] {
        if let Some(v) = p.get(key) {
            *target = if v.is_null() {
                None
            } else {
                Some(
                    v.as_str()
                        .ok_or_else(|| format!("{key} must be a string or null"))?
                        .into(),
                )
            };
        }
    }
    if !matches!(
        cfg.provider.as_str(),
        "anthropic" | "openai" | "openai-responses"
    ) {
        return Err("unsupported provider transport".into());
    }
    crate::provider::validate_endpoint(&cfg.base_url).map_err(|e| e.to_string())?;
    if let Some(reasoning) = cfg.reasoning_budget {
        if reasoning > cfg.max_tokens {
            return Err("reasoningBudget cannot exceed maxTokens".into());
        }
        if cfg.provider == "anthropic"
            && reasoning > 0
            && (reasoning < 1024 || reasoning >= cfg.max_tokens)
        {
            return Err(
                "Anthropic reasoningBudget must be zero or at least 1024 and below maxTokens"
                    .into(),
            );
        }
    }
    let instructions=p.get("instructions").map(|v|v.as_str().ok_or("instructions must be a string")).transpose()?.unwrap_or(
        if mode == ToolMode::Host {"You are a coding agent. Use only the supplied tools. The host owns permissions and user interactions."}else{&cfg.system_prompt}).to_owned();
    Ok((cfg, host_tools, mode, instructions))
}
fn runtime_command_catalogue() -> Value {
    let commands = crate::actions::catalogue().into_iter().map(|command| {
        let mode = if matches!(command.name.as_str(), "help" | "status" | "compact") { "runtime" } else { "interactive" };
        json!({"name":command.name,"hint":command.hint,"description":command.description,"aliases":command.aliases,"requiredArgs":command.required_args,"runtimeMode":mode})
    }).collect::<Vec<_>>();
    json!({"source":"glass-core","scope":"built-in actions","completeBuiltinCatalogue":true,"commands":commands})
}

fn runtime_command_help() -> String {
    crate::actions::catalogue()
        .into_iter()
        .map(|command| {
            let mode = if matches!(command.name.as_str(), "help" | "status" | "compact") {
                "resident"
            } else {
                "native GC UI"
            };
            format!("/{:<18} {} [{}]", command.name, command.description, mode)
        })
        .collect::<Vec<_>>()
        .join("\n")
}

pub fn serve(cfg: config::Config) -> Result<(), String> {
    let (tx, rx) = mpsc::sync_channel(64);
    std::thread::spawn(move || {
        let stdin = std::io::stdin();
        let mut reader = stdin.lock();
        loop {
            let mut line = Vec::new();
            match reader
                .by_ref()
                .take((MAX_FRAME + 1) as u64)
                .read_until(b'\n', &mut line)
            {
                Ok(0) | Err(_) => break,
                Ok(_) if line.len() > MAX_FRAME => {
                    let _ = tx.send(Err("frame exceeds 1 MiB; connection closed".into()));
                    break;
                }
                Ok(_) => {
                    let parsed = serde_json::from_slice::<Value>(&line)
                        .map_err(|e| e.to_string())
                        .and_then(|v| {
                            if v.is_object()
                                && v["jsonrpc"] == "2.0"
                                && (v.get("method").is_some() || v.get("id").is_some())
                            {
                                Ok(v)
                            } else {
                                Err("expected a JSON-RPC 2.0 object".into())
                            }
                        });
                    if tx.send(parsed).is_err() {
                        break;
                    }
                }
            }
        }
    });
    let wire = Rc::new(RefCell::new(Wire {
        rx,
        pending: VecDeque::new(),
        disconnected: false,
        steer: None,
        steer_records: Vec::new(),
        image_enabled: true,
        completing: false,
        cancel_grace_ms: 250,
        durable: None,
        durable_task: None,
        protocol_version: None,
    }));
    let mut root: Option<PathBuf> = None;
    let mut conversations = BTreeMap::<String, Conversation>::new();
    let mut durable: Option<durable_host::Host> = None;
    loop {
        let queued = wire.borrow_mut().pending.pop_front();
        let m = match queued {
            Some(m) => m,
            None => match wire.borrow().rx.recv() {
                Ok(Ok(m)) => m,
                Ok(Err(e)) => {
                    error(&Value::Null, -32700, e);
                    continue;
                }
                Err(_) => break,
            },
        };
        let id = m.get("id").cloned().unwrap_or(Value::Null);
        if !m.get("id").is_some_and(|id| {
            id.is_number() || id.as_str().is_some_and(|s| !s.is_empty() && s.len() <= 128)
        }) {
            error(
                &Value::Null,
                -32600,
                "requests require a nonempty string ID of at most 128 bytes or a number",
            );
            continue;
        }
        trace_runtime_frame(
            "core_stdin",
            "frame_received",
            runtime_frame_class(&m),
            m.to_string().len(),
            None,
            runtime_correlation_hash(&m),
        );
        let method = m["method"].as_str().unwrap_or("");
        let p = &m["params"];
        if method == "initialize" {
            if root.is_some() {
                error(&id, -32001, "already initialized");
                continue;
            }
            let requested_protocol = p["protocolVersion"].as_u64().unwrap_or_default();
            if !matches!(requested_protocol, 1 | 2) {
                error(&id, -32602, "protocolVersion must be 1 or 2");
                continue;
            }
            if requested_protocol == 2 {
                let Some(capabilities) = p["requiredCapabilities"].as_array() else {
                    error(
                        &id,
                        -32602,
                        "protocol v2 requires the mixedTools capability declaration",
                    );
                    continue;
                };
                if capabilities.iter().any(|value| value != "mixedTools") {
                    error(&id, -32602, "unsupported required runtime capability");
                    continue;
                }
                if !capabilities.iter().any(|value| value == "mixedTools") {
                    error(
                        &id,
                        -32602,
                        "protocol v2 requires the mixedTools capability declaration",
                    );
                    continue;
                }
            }
            let dir = p
                .get("storageDir")
                .and_then(Value::as_str)
                .map(PathBuf::from)
                .unwrap_or_else(|| config::data_dir().join("runtime"));
            if !dir.is_absolute() {
                error(&id, -32602, "storageDir must be absolute");
                continue;
            }
            std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
            if let Some(ms) = p.get("cancelGraceMs") {
                let Some(ms) = ms.as_u64().filter(|n| *n <= 5000) else {
                    error(&id, -32602, "cancelGraceMs must be 0..5000");
                    continue;
                };
                wire.borrow_mut().cancel_grace_ms = ms;
            }
            root = Some(dir.canonicalize().map_err(|e| e.to_string())?);
            wire.borrow_mut().protocol_version = Some(requested_protocol);
            reply(
                &id,
                json!({"protocolVersion":requested_protocol,"runtime":{"name":"glass-core","version":env!("CARGO_PKG_VERSION")},
                "capabilities":{"durable":true,"hostTools":true,"localTools":true,"steering":true,"followUp":true,"cancel":true,
                    "mixedTools":requested_protocol >= 2,
                    "resume":true,"fork":true,"rewind":"context_only","compact":true,"sessionConfigure":true,
                    "hostInteractions":"host tools provide workspace capabilities and structured questions; Glass does not re-decide Core tool permissions","concurrentTurns":false,
                    "imagePrompt":true,"completion":true,"typedContext":true,"structuredResults":true,"toolProgress":true,"toolCancellationAck":true,"incrementalEntries":true,"modelDiscovery":true,"authenticationReadiness":true,"structuredEntries":true,"sessionNames":true,"statistics":true,"lineage":true,"completionCancellation":true,"safeResumeById":true,"toolReconciliation":true,"commandDiscovery":true,"commandDispatch":true},
                "limits":{"frameBytes":MAX_FRAME,"toolResultBytes":512*1024,"queuedFollowUps":MAX_PENDING},
                "storageDir":root}),
            );
            continue;
        }
        let Some(dir) = root.as_ref() else {
            error(&id, -32001, "initialize must be called first");
            continue;
        };
        if method == "shutdown" {
            reply(&id, json!({}));
            break;
        }
        let result: Result<Value, String> = (|| match method {
            method if method.starts_with("durable/") => {
                if durable.is_none() {
                    durable = Some(durable_host::Host::open(dir, cfg.log_fsync == "always")?);
                }
                durable
                    .as_mut()
                    .unwrap()
                    .handle(method, p, dir, &cfg, &wire, &id)
            }
            "session/new" | "session/resume" => {
                if conversations.len() >= 32 {
                    return Err("close an idle session before opening another".into());
                }
                let (session, mut manifest, settings, host_tools, tool_mode, instructions) =
                    if method == "session/resume" {
                        let sid = string(p, "sessionId")?;
                        let manifest = support::read_manifest(dir, sid)?;
                        let expected = p
                            .get("expectedToolMode")
                            .or_else(|| p.get("toolMode"))
                            .and_then(Value::as_str)
                            .unwrap_or("host");
                        let actual = manifest
                            .get("toolMode")
                            .and_then(Value::as_str)
                            .unwrap_or("host");
                        if expected != actual {
                            return Err(
                                "stored tool ownership differs from expectedToolMode".into()
                            );
                        }
                        for (key, value) in p.as_object().ok_or("params must be an object")? {
                            if !matches!(key.as_str(), "sessionId" | "expectedToolMode")
                                && manifest.get(key) != Some(value)
                            {
                                return Err(format!("resume configuration mismatch: {key}"));
                            }
                        }
                        let (settings, tools, mode, instructions) = metadata(&manifest, &cfg)?;
                        validate_tool_mode(wire.borrow().protocol_version, mode)?;
                        (
                            Session::open(dir, sid).map_err(|e| e.to_string())?,
                            manifest,
                            settings,
                            tools,
                            mode,
                            instructions,
                        )
                    } else {
                        let (settings, tools, mode, instructions) = metadata(p, &cfg)?;
                        validate_tool_mode(wire.borrow().protocol_version, mode)?;
                        let session = Session::create(dir).map_err(|e| e.to_string())?;
                        (session, p.clone(), settings, tools, mode, instructions)
                    };
                let sid = session.id.clone();
                manifest["sessionId"] = json!(sid);
                manifest["cwd"] = json!(settings.workspace);
                manifest["toolMode"] = json!(tool_mode.as_str());
                manifest["toolOwners"] = tool_owners(tool_mode, host_tools.as_ref());
                manifest["provider"] = json!(settings.provider);
                manifest["baseUrl"] = json!(settings.base_url);
                manifest["model"] = json!(settings.model);
                manifest["imageInput"] = json!(settings.image_input);
                manifest["maxTokens"] = json!(settings.max_tokens);
                manifest["instructions"] = json!(instructions);
                if let Some(reasoning) = settings.reasoning_budget {
                    manifest["reasoningBudget"] = json!(reasoning);
                }
                if let Some(env) = &settings.api_key_env {
                    manifest["apiKeyEnv"] = json!(env);
                }
                if settings.connection.is_some() {
                    manifest["connection"] = json!(settings.connection);
                }
                support::save_manifest(dir, &sid, &manifest)?;
                conversations.insert(
                    sid.clone(),
                    Conversation {
                        session,
                        cfg: settings,
                        state: tools::State::new(),
                        tools: host_tools,
                        tool_mode,
                        instructions,
                        stats_cache: None,
                        durable: None,
                    },
                );
                Ok(json!({"sessionId":sid,"toolOwners":manifest["toolOwners"]}))
            }
            "session/list" | "session/tree" => support::list(dir, p, &conversations),
            "auth/status" | "models/list" => {
                let selected = if let Some(sid) = p.get("sessionId").and_then(Value::as_str) {
                    &conversations.get(sid).ok_or("unknown open session")?.cfg
                } else {
                    &cfg
                };
                if method == "auth/status" {
                    auth_status(selected)
                } else {
                    model_list(selected, p)
                }
            }
            "session/close" => {
                let sid = string(p, "sessionId")?;
                conversations.remove(sid).ok_or("unknown open session")?;
                Ok(json!({}))
            }
            _ => {
                if !matches!(
                    method,
                    "session/inspect"
                        | "session/entries"
                        | "session/entry"
                        | "session/stats"
                        | "session/name"
                        | "session/describe"
                        | "session/validate"
                        | "session/interruptedTools"
                        | "tool/reconcile"
                        | "completion/request"
                        | "session/configure"
                        | "session/fork"
                        | "session/rewind"
                        | "turn/prompt"
                        | "turn/followUp"
                        | "session/compact"
                        | "commands/list"
                        | "commands/execute"
                        | "turn/cancel"
                ) {
                    return Err(format!("method not found: {method}"));
                }
                let sid = string(p, "sessionId")?;
                let c = conversations.get_mut(sid).ok_or("unknown open session")?;
                match method {
                    "commands/list" => Ok(runtime_command_catalogue()),
                    "commands/execute" => {
                        let requested = string(p, "name")?.trim_start_matches('/');
                        let command = crate::actions::catalogue()
                            .into_iter()
                            .find(|command| {
                                command.name == requested
                                    || command.aliases.iter().any(|alias| alias == requested)
                            })
                            .ok_or_else(|| format!("unknown Glass Core command: /{requested}"))?;
                        let arguments = p.get("arguments").and_then(Value::as_str).unwrap_or("");
                        if arguments.len() > 16 * 1024 {
                            return Err("command arguments exceed 16 KiB".into());
                        }
                        match command.name.as_str() {
                            "help" => Ok(json!({"name":"help","text":runtime_command_help()})),
                            "status" => Ok(
                                json!({"name":"status","text":format!("Glass Core · {} · {} · session {}", c.cfg.provider, c.cfg.model, c.session.id),"provider":c.cfg.provider,"model":c.cfg.model,"sessionId":c.session.id}),
                            ),
                            "compact" => {
                                let provider = provider(&c.cfg, &c.session)?;
                                let (kept, dropped) = crate::compact::run_manual(
                                    provider.as_ref(),
                                    &mut c.session,
                                    &c.state,
                                    (!arguments.trim().is_empty()).then_some(arguments),
                                )?;
                                Ok(
                                    json!({"name":"compact","text":format!("Compacted session · kept {kept}, dropped {dropped}"),"kept":kept,"dropped":dropped}),
                                )
                            }
                            _ => Ok(
                                json!({"name":command.name,"requiresInteractive":true,"message":format!("/{0} is available in the standalone Glass Core interface. Glass Dev does not currently provide this command.", command.name)}),
                            ),
                        }
                    }
                    "session/entries" => support::entries(&c.session, p),
                    "session/entry" => support::entry(&c.session, p),
                    "session/stats" => {
                        let size = std::fs::metadata(&c.session.path)
                            .map_err(|e| e.to_string())?
                            .len();
                        if let Some((stamp, value)) = &c.stats_cache
                            && *stamp == size
                        {
                            return Ok(value.clone());
                        }
                        let value = support::stats(&c.session)?;
                        c.stats_cache = Some((size, value.clone()));
                        Ok(value)
                    }
                    "session/interruptedTools" => support::interrupted(&c.session),
                    "session/describe" => Ok(support::read_manifest(dir, sid)?),
                    "session/validate" => {
                        let provider = provider(&c.cfg, &c.session)?;
                        Ok(
                            json!({"configured":true,"model":provider.model(),"contextWindow":provider.context_window(),
                            "imageInput":c.cfg.image_input,"reasoning":reasoning_capability(&c.cfg.model),"providerReachable":null}),
                        )
                    }
                    "session/name" => {
                        let name = string(p, "name")?;
                        if name.len() > 256 || name.chars().any(char::is_control) {
                            return Err(
                                "name must be at most 256 bytes without control characters".into(),
                            );
                        }
                        let mut manifest = support::read_manifest(dir, sid)?;
                        manifest["name"] = json!(name);
                        support::save_manifest(dir, sid, &manifest)?;
                        Ok(json!({"name":name}))
                    }
                    "tool/reconcile" => {
                        let request = string(p, "requestId")?;
                        let pending = support::tool_state(&c.session, request)?;
                        if !pending.is_some_and(|tool| tool["executionStatus"] == "unknown") {
                            return Err("requestId is not an unresolved host operation".into());
                        }
                        let status = string(p, "executionStatus")?;
                        if !matches!(status, "completed" | "cancelled" | "failed" | "unknown") {
                            return Err("invalid reconciliation status".into());
                        }
                        let summary = p.get("text").and_then(Value::as_str).unwrap_or("");
                        if summary.len() > 16 * 1024 {
                            return Err("reconciliation summary exceeds 16 KiB".into());
                        }
                        let data =
                            json!({"requestId":request,"executionStatus":status,"summary":summary});
                        c.session.append_checked(&[
                            Record::Custom{kind:"runtime/tool_reconciliation".into(),data:Some(data.to_string())},
                            Record::Custom{kind:"runtime/context".into(),data:Some(json!([{"type":"json","value":{"hostToolReconciliation":data}}]).to_string())}
                        ]).map_err(|e|e.to_string())?;
                        Ok(json!({"recorded":true,"requestId":request}))
                    }
                    "completion/request" => completion(c, &wire, &id, p),
                    "session/inspect" => {
                        let records = c.session.history_records();
                        let offset = p.get("cursor").and_then(Value::as_u64).unwrap_or(0);
                        let offset = checked_unsigned_index::<usize>(offset)
                            .ok_or("cursor exceeds platform capacity")?;
                        let limit = p
                            .get("limit")
                            .and_then(Value::as_u64)
                            .unwrap_or(20)
                            .clamp(1, 100) as usize;
                        if offset > records.len() {
                            return Err("cursor exceeds record count".into());
                        }
                        let end = offset.saturating_add(limit).min(records.len());
                        Ok(json!({"sessionId":sid,"model":c.cfg.model,
                            "reasoningBudget":c.cfg.reasoning_budget,"toolMode":c.tool_mode.as_str(),
                            "toolOwners":tool_owners(c.tool_mode,c.tools.as_ref()),
                            "records":records[offset..end].iter().map(record).collect::<Vec<_>>(),
                            "recordCount":records.len(),"nextCursor":if end<records.len(){Some(end)}else{None}}))
                    }
                    "session/configure" => {
                        if p.as_object().is_some_and(|fields| {
                            fields.keys().any(|key| {
                                !matches!(
                                    key.as_str(),
                                    "sessionId"
                                        | "model"
                                        | "reasoningBudget"
                                        | "instructions"
                                        | "provider"
                                        | "baseUrl"
                                        | "connection"
                                        | "apiKeyEnv"
                                        | "imageInput"
                                        | "maxTokens"
                                )
                            })
                        }) {
                            return Err("session/configure cannot change tools, ownership, or workspace; create a new session".into());
                        }
                        let mut manifest = support::read_manifest(dir, sid)?;
                        if let Some(connection) = p.get("connection").and_then(Value::as_str) {
                            let store = crate::auth::load()?;
                            let account = store
                                .connections
                                .iter()
                                .find(|c| c.id == connection)
                                .ok_or("unknown connection")?;
                            manifest["provider"] = json!(account.provider);
                            manifest["baseUrl"] = json!(account.base_url);
                            manifest["model"] = json!(account.model);
                            manifest["apiKeyEnv"] = json!(account.api_key_env);
                        }
                        for key in [
                            "model",
                            "reasoningBudget",
                            "instructions",
                            "provider",
                            "baseUrl",
                            "connection",
                            "apiKeyEnv",
                            "imageInput",
                            "maxTokens",
                        ] {
                            if let Some(v) = p.get(key) {
                                manifest[key] = v.clone();
                            }
                        }
                        let (settings, _, _, instructions) = metadata(&manifest, &cfg)?;
                        crate::auth::secure_write(
                            &dir.join(format!("{sid}.host.json")),
                            &serde_json::to_vec(&manifest).map_err(|e| e.to_string())?,
                        )
                        .map_err(|e| e.to_string())?;
                        c.cfg = settings;
                        c.instructions = instructions;
                        Ok(json!({}))
                    }
                    "session/fork" => {
                        let turns = c.session.raw_turns().map_err(|e| e.to_string())?;
                        let through = p
                            .get("throughTurn")
                            .and_then(Value::as_u64)
                            .unwrap_or(turns as u64);
                        if through == 0 || through > turns as u64 {
                            return Err("throughTurn must identify an existing user turn".into());
                        }
                        let fork = c
                            .session
                            .branch(dir, session_turn_index(through).map_err(str::to_string)?)
                            .map_err(|e| e.to_string())?;
                        let mut manifest = support::read_manifest(dir, sid)?;
                        manifest["sessionId"] = json!(fork.id);
                        manifest["parent"] = json!({"sessionId":sid,"throughTurn":through});
                        crate::auth::secure_write(
                            &dir.join(format!("{}.host.json", fork.id)),
                            &serde_json::to_vec(&manifest).map_err(|e| e.to_string())?,
                        )
                        .map_err(|e| e.to_string())?;
                        Ok(json!({"sessionId":fork.id,"open":false}))
                    }
                    "turn/prompt" | "turn/followUp" => {
                        let records = support::input(p, c.cfg.image_input)?;
                        run(c, &wire, &id, records)?;
                        Ok(json!({"sessionId":sid,"settled":true}))
                    }
                    "session/rewind" => {
                        let through = p["throughTurn"]
                            .as_u64()
                            .ok_or("throughTurn must be an unsigned integer")?;
                        let through = session_turn_index(through).map_err(str::to_string)?;
                        let turns = c.session.raw_turns().map_err(|e| e.to_string())?;
                        if through > turns {
                            return Err("throughTurn exceeds session turn count".into());
                        }
                        let edits = (through + 1..=turns)
                            .map(|turn| Record::ContextEdit {
                                turn,
                                mode: "hide".into(),
                                text: None,
                            })
                            .collect::<Vec<_>>();
                        c.session
                            .append_checked(&edits)
                            .map_err(|e| e.to_string())?;
                        Ok(json!({"hiddenTurns":turns-through,"files":"unchanged"}))
                    }
                    "session/compact" => {
                        let provider = provider(&c.cfg, &c.session)?;
                        let (kept, dropped) = crate::compact::run_manual(
                            provider.as_ref(),
                            &mut c.session,
                            &c.state,
                            p.get("instructions").and_then(Value::as_str),
                        )?;
                        Ok(json!({"kept":kept,"dropped":dropped}))
                    }
                    "turn/cancel" => Ok(json!({"accepted":false,"reason":"idle"})),
                    _ => Err(format!("method not found: {method}")),
                }
            }
        })();
        match result {
            Ok(value) => reply(&id, value),
            Err(e) => error(
                &id,
                if e.starts_with("method not found") {
                    -32601
                } else {
                    -32000
                },
                e,
            ),
        }
        if wire.borrow().disconnected {
            break;
        }
    }
    Ok(())
}
fn provider(
    cfg: &config::Config,
    session: &Session,
) -> Result<Box<dyn crate::provider::Provider>, String> {
    let path = session.path.with_extension("cost.json");
    let ledger = crate::budget::load(&path, cfg.cost_limit);
    crate::make_provider(
        cfg,
        ledger,
        std::sync::Arc::new(std::sync::Mutex::new(path)),
        None,
    )
}
fn record(r: &Record) -> Value {
    let (role, text) = r.to_entry();
    let mut end = text.len().min(4096);
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    json!({"role":format!("{role:?}").to_lowercase(),"text":&text[..end],"truncated":end<text.len(),"bytes":text.len()})
}
fn run(
    c: &mut Conversation,
    wire: &Rc<RefCell<Wire>>,
    turn: &Value,
    records: Vec<Record>,
) -> Result<(), String> {
    let factory: crate::warmer::ProviderFactory = {
        let settings = c.cfg.clone();
        let path = c.session.path.with_extension("cost.json");
        let ledger = crate::budget::load(&path, settings.cost_limit);
        let path = std::sync::Arc::new(std::sync::Mutex::new(path));
        std::sync::Arc::new(move || {
            crate::make_provider(&settings, ledger.clone(), path.clone(), None)
        })
    };
    let provider = factory()?;
    c.session
        .append_checked(&records)
        .map_err(|e| e.to_string())?;
    {
        let mut io = wire.borrow_mut();
        io.image_enabled = c.cfg.image_input;
        io.completing = false;
        io.steer = None;
        io.steer_records.clear();
    }
    let mut extensions = ext::Registry::load(&[], &c.cfg.workspace, &c.cfg.model);
    let sid = c.session.id.clone();
    let statuses = Rc::new(RefCell::new(BTreeMap::<String, bool>::new()));
    let emit_status = statuses.clone();
    let durable = c.durable.as_ref();
    let mut agent=Agent::new(provider.as_ref(),&mut c.session,&mut c.state,&[],&mut extensions,&c.cfg.workspace)
        .with_image_input(c.cfg.image_input)
        .with_prompt(c.instructions.clone(),None,None)
        .with_stream_factory(factory)
        .with_limits(c.cfg.max_tokens,c.cfg.reasoning_budget,usize::MAX)
        .with_progress(|progress|{
            let publish=|kind:&str,data:Value|{if durable.is_none_or(|context|context.publish(kind,data.clone(),turn)){event(&sid,turn,kind,data);}};
            match progress{
                Progress::Text(text)=>publish("text",json!({"text":text})),
                Progress::Reasoning(text)=>publish("reasoning",json!({"text":text})),
                Progress::ToolStart{call_id,name,command}=>publish("tool_start",json!({"callId":call_id,"name":name,"title":command})),
                Progress::ToolEnd{call_id,output,..}=>publish("tool_end",json!({"callId":call_id,"text":output,"isError":emit_status.borrow_mut().remove(call_id).unwrap_or_else(||output.starts_with("error:")||output.starts_with("[exit ")&&!output.starts_with("[exit 0]"))})),
                Progress::Waiting=>publish("waiting",json!({})),
                Progress::Compacted{kept,dropped}=>publish("compacted",json!({"kept":kept,"dropped":dropped})),
                Progress::Steered(text)=>publish("steered",json!({"text":text})),
                Progress::ProviderFailure(text)=>publish("error",json!({"message":text})),
                Progress::ProviderNotice(text)=>publish("notice",json!({"text":text})),
                _=>{},
            }
        });
    if let Some(tools) = &c.tools {
        let io = wire.clone();
        let status = statuses.clone();
        let session = sid.clone();
        let executor: crate::agent::HostExecutor<'_> = Box::new(move |call, _pump, sequence| {
            let result = io.borrow_mut().tool(call, &session, turn, sequence);
            status
                .borrow_mut()
                .insert(call.id.clone(), result.outcome.exit_code != Some(0));
            result
        });
        agent = if c.tool_mode == ToolMode::Mixed {
            agent.with_mixed_host_tools(tools.clone(), executor)
        } else {
            agent.with_host_tools(tools.clone(), executor)
        };
    }
    event(&sid, turn, "turn_start", json!({}));
    // A steer interrupts the current request, preserves its transcript, and
    // immediately continues the same turn using the appended correction.
    loop {
        let started = Instant::now();
        trace_turn_phase("run_turn_begin", "provider_turn", turn, None);
        let outcome = agent.run_turn(|slot| wire.borrow_mut().pump(slot, &sid, turn));
        let outcome_class = match &outcome {
            Ok((_, Stop::Finished)) => "stop_finished",
            Ok((_, Stop::Steered)) => "stop_steered",
            Ok((_, Stop::Cancelled)) => "stop_cancelled",
            Ok((_, Stop::ToolBudget)) => "stop_tool_budget",
            Err(_) => "provider_or_tool_error",
        };
        trace_turn_phase(
            "run_turn_return",
            outcome_class,
            turn,
            Some(started.elapsed().as_micros()),
        );
        match outcome {
            Ok((out, Stop::Steered)) => {
                let records = std::mem::take(&mut wire.borrow_mut().steer_records);
                agent
                    .session
                    .append_checked(&records)
                    .map_err(|e| e.to_string())?;
                if wire.borrow().disconnected {
                    break;
                }
                if let Some(usage) = out.usage {
                    persist_usage(agent.session, &usage, turn)?;
                    event(
                        &sid,
                        turn,
                        "usage",
                        json!({"input":usage.input,"cacheRead":usage.cache_read,"cacheWrite":usage.cache_write,"output":usage.output}),
                    );
                }
                continue;
            }
            Ok((out, stop)) => {
                if let Some(usage) = out.usage {
                    persist_usage(agent.session, &usage, turn)?;
                    event(
                        &sid,
                        turn,
                        "usage",
                        json!({"input":usage.input,"cacheRead":usage.cache_read,"cacheWrite":usage.cache_write,"output":usage.output}),
                    );
                }
                let reason = settlement_reason(wire.borrow().disconnected, stop);
                if let Some(context) = durable {
                    context.finish(reason, None, turn)?;
                }
                trace_turn_phase("settled_event_begin", reason, turn, None);
                event(&sid, turn, "settled", json!({"reason":reason}));
                trace_turn_phase("settled_event_complete", reason, turn, None);
                break;
            }
            Err(e) => {
                if let Some(context) = durable {
                    context.finish("failed", Some(&e.to_string()), turn)?;
                }
                trace_turn_phase("settled_event_begin", "failed", turn, None);
                event(
                    &sid,
                    turn,
                    "settled",
                    json!({"reason":"failed","message":e.to_string()}),
                );
                trace_turn_phase("settled_event_complete", "failed", turn, None);
                return Err(e.to_string());
            }
        }
    }
    Ok(())
}

fn settlement_reason(disconnected: bool, stop: crate::agent::Stop) -> &'static str {
    if disconnected {
        "interrupted"
    } else if stop == crate::agent::Stop::Cancelled {
        "cancelled"
    } else {
        "completed"
    }
}

fn error_kind(code: i32, message: &str) -> &'static str {
    match code {
        -32700 | -32600 => "protocol",
        -32601 => "unsupported_method",
        -32602 => "invalid_input",
        -32001 => "busy",
        -32002 => "limit",
        _ if message.contains("credential")
            || message.contains("Not connected")
            || message.contains("Authentication") =>
        {
            "authentication"
        }
        _ if message.contains("provider")
            || message.contains("transport")
            || message.contains("http ") =>
        {
            "provider"
        }
        _ => "operation",
    }
}
fn reasoning_capability(model: &str) -> Value {
    let m = model.to_ascii_lowercase();
    let support = if ["gpt-5", "o1", "o3", "o4"]
        .iter()
        .any(|prefix| m.starts_with(prefix))
    {
        "effort_hint"
    } else if ["glm", "qwen3", "deepseek"]
        .iter()
        .any(|part| m.contains(part))
    {
        "budget_hint"
    } else {
        "unknown"
    };
    json!({"control":support,"providerEnforcement":"not_verified"})
}
fn auth_status(cfg: &config::Config) -> Result<Value, String> {
    let refresh = crate::auth::needs_refresh(cfg);
    let credential = if refresh {
        false
    } else {
        crate::auth::resolve(cfg).is_ok()
    };
    Ok(
        json!({"credentialReady":credential,"requiresRefresh":refresh,"providerReachable":null,
        "provider":cfg.provider,"model":cfg.model,"connection":cfg.connection,
        "source":if cfg.connection.is_some(){"saved_connection"}else if cfg.api_key_env.is_some(){"environment"}else{"unauthenticated_endpoint"}}),
    )
}
fn model_list(cfg: &config::Config, p: &Value) -> Result<Value, String> {
    type CatalogCache = std::sync::Mutex<Vec<(String, std::time::Instant, Value)>>;
    static CACHE: std::sync::OnceLock<CatalogCache> = std::sync::OnceLock::new();
    let cache_key = crate::hashing::sha256(
        format!(
            "{}:{}:{:?}:{:?}",
            cfg.provider, cfg.base_url, cfg.connection, cfg.api_key_env
        )
        .as_bytes(),
    );
    let cache = CACHE.get_or_init(|| std::sync::Mutex::new(Vec::new()));
    if !p["refresh"].as_bool().unwrap_or(false)
        && let Ok(cache) = cache.lock()
        && let Some((_, _, data)) = cache
            .iter()
            .find(|(k, at, _)| *k == cache_key && at.elapsed() < std::time::Duration::from_secs(60))
    {
        return model_page(data, p, true, &cfg.provider, &cfg.model);
    }
    let key = crate::auth::resolve(cfg)?;
    let url = format!(
        "{}/{}",
        cfg.base_url.trim_end_matches('/'),
        if cfg.provider == "anthropic" {
            "v1/models"
        } else {
            "models"
        }
    );
    let mut req = crate::auth::http().get(url);
    if cfg.provider == "anthropic" {
        req = req.header("anthropic-version", "2023-06-01");
    }
    if let Some(key) = key {
        req = if cfg.provider == "anthropic" {
            req.header("x-api-key", key)
        } else {
            req.header("authorization", format!("Bearer {key}"))
        };
    }
    let mut response = req
        .call()
        .map_err(|_| "model discovery failed; inspect authentication and endpoint")?;
    let mut bytes = Vec::new();
    response
        .body_mut()
        .as_reader()
        .take(2 * 1024 * 1024 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| "model catalog could not be read")?;
    if bytes.len() > 2 * 1024 * 1024 {
        return Err("model catalog exceeds 2 MiB".into());
    }
    let data: Value = serde_json::from_slice(&bytes).map_err(|_| "invalid model catalog")?;
    let models=crate::onboarding::parse_models(&data).into_iter().take(2048).map(|m|json!({
        "id":m.id,"label":m.label,"reasoning":reasoning_capability(&m.id),"vision":null,"toolCalling":null
    })).collect::<Vec<_>>();
    let catalog = json!(models);
    if let Ok(mut cache) = cache.lock() {
        cache.retain(|(k, _, _)| *k != cache_key);
        if cache.len() >= 4 {
            cache.remove(0);
        }
        cache.push((cache_key, std::time::Instant::now(), catalog.clone()));
    }
    model_page(&catalog, p, false, &cfg.provider, &cfg.model)
}
fn model_page(
    catalog: &Value,
    p: &Value,
    cached: bool,
    provider: &str,
    current_model: &str,
) -> Result<Value, String> {
    let models = catalog.as_array().ok_or("invalid cached catalog")?;
    let cursor = p.get("cursor").and_then(Value::as_u64).unwrap_or(0);
    let cursor =
        checked_unsigned_index::<usize>(cursor).ok_or("model cursor exceeds platform capacity")?;
    if cursor > models.len() {
        return Err("model cursor exceeds catalog".into());
    }
    let end = cursor
        .saturating_add(
            p.get("limit")
                .and_then(Value::as_u64)
                .unwrap_or(50)
                .clamp(1, 100) as usize,
        )
        .min(models.len());
    Ok(
        json!({"models":&models[cursor..end],"nextCursor":if end<models.len(){Some(end)}else{None},
        "provider":provider,"currentModel":current_model,"source":"provider","cached":cached,"capabilitiesVerified":false}),
    )
}
fn completion(
    c: &Conversation,
    wire: &Rc<RefCell<Wire>>,
    id: &Value,
    p: &Value,
) -> Result<Value, String> {
    use std::sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    };
    let prefix = p.get("prefix").and_then(Value::as_str).unwrap_or("");
    let suffix = p.get("suffix").and_then(Value::as_str).unwrap_or("");
    if prefix.len() + suffix.len() > 128 * 1024 {
        return Err("completion context exceeds 128 KiB".into());
    }
    let max_tokens = p.get("maxTokens").and_then(Value::as_u64).unwrap_or(1024);
    if !(1..=4096).contains(&max_tokens) {
        return Err("completion maxTokens must be 1..4096".into());
    }
    // Admission checks happen before completion_start, and never append history.
    let _admission = provider(&c.cfg, &c.session)?;
    let settings = c.cfg.clone();
    let path = c.session.path.with_extension("cost.json");
    let ledger = crate::budget::load(&path, settings.cost_limit);
    let path = Arc::new(std::sync::Mutex::new(path));
    let messages = vec![glass_mod_api::Message::User(format!(
        "Insert source text between the following prefix and suffix. Return only the insertion.\n<prefix>\n{prefix}\n</prefix>\n<suffix>\n{suffix}\n</suffix>"
    ))];
    let cancel = Arc::new(AtomicBool::new(false));
    let worker_cancel = cancel.clone();
    let (tx, rx) = mpsc::sync_channel(64);
    std::thread::spawn(move || {
        let result = crate::provider::with_request_cancel(
            worker_cancel.clone(),
            || -> Result<(), String> {
                let provider = crate::make_provider(&settings, ledger, path, None)?;
                let request = crate::provider::Request {
                    model: &settings.model,
                    system: "Return only inserted source text. No tools, explanation, or Markdown fences.",
                    messages: &messages,
                    tools: &[],
                    max_tokens: max_tokens as u32,
                    reasoning: Some(0),
                };
                for event in provider.stream(&request).map_err(|e| e.to_string())? {
                    if worker_cancel.load(Ordering::Acquire) {
                        break;
                    }
                    if tx.send(Ok(event)).is_err() {
                        return Ok(());
                    }
                }
                Ok(())
            },
        );
        if let Err(error) = result {
            let _ = tx.send(Err(error));
        }
    });
    wire.borrow_mut().completing = true;
    event(&c.session.id, id, "completion_start", json!({}));
    let mut text = String::new();
    let mut usage = None;
    let mut done = false;
    let mut failure = None;
    let mut interrupted = None;
    loop {
        wire.borrow_mut().pump(&mut interrupted, &c.session.id, id);
        if interrupted.is_some() {
            cancel.store(true, Ordering::Release);
            break;
        }
        match rx.recv_timeout(std::time::Duration::from_millis(10)) {
            Ok(Ok(crate::provider::Event::Text(chunk))) => {
                if text.len() + chunk.len() > 64 * 1024 {
                    failure = Some("completion output exceeds 64 KiB".into());
                    cancel.store(true, Ordering::Release);
                    break;
                }
                text.push_str(&chunk);
                event(&c.session.id, id, "completion_text", json!({"text":chunk}));
            }
            Ok(Ok(crate::provider::Event::Usage(u))) => {
                usage = Some(
                    json!({"input":u.input,"output":u.output,"cacheRead":u.cache_read,"cacheWrite":u.cache_write}),
                )
            }
            Ok(Ok(crate::provider::Event::Done)) => {
                done = true;
                break;
            }
            Ok(Ok(crate::provider::Event::Error(e))) | Ok(Err(e)) => {
                failure = Some(e);
                break;
            }
            Ok(Ok(crate::provider::Event::ToolCall { .. })) => {
                failure = Some("completion provider attempted a tool call; not executed".into());
                break;
            }
            Ok(Ok(_)) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
            Err(_) => {}
        }
    }
    cancel.store(true, Ordering::Release);
    wire.borrow_mut().completing = false;
    let reason = if interrupted.is_some() {
        "cancelled"
    } else if failure.is_some() || !done || text.trim().is_empty() {
        "failed"
    } else {
        "completed"
    };
    event(
        &c.session.id,
        id,
        "completion_settled",
        json!({"reason":reason}),
    );
    if reason == "failed" {
        return Err(crate::live::sanitize_provider_detail(
            &failure.unwrap_or("completion ended without insertion".into()),
        ));
    }
    Ok(json!({"text":text,"stopReason":reason,"usage":usage}))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_turn_index_rejects_wrapping_u64_values() {
        let beyond_32_bit = (u32::MAX as u64) + 1;
        assert_eq!(checked_unsigned_index::<u32>(beyond_32_bit), None);
        if usize::BITS == 32 {
            assert_eq!(
                session_turn_index(beyond_32_bit),
                Err("throughTurn exceeds platform capacity")
            );
        }
    }

    #[test]
    fn host_schemas_are_exclusive_and_validated() {
        assert_eq!(
            schemas(&json!([{"name":"host_read","inputSchema":{"type":"object"}}]))
                .unwrap()
                .len(),
            1
        );
        assert!(schemas(&json!([{"name":"bad/name","inputSchema":{"type":"object"}}])).is_err());
        assert!(schemas(&json!([{"name":"x","inputSchema":{"type":"object"}},{"name":"x","inputSchema":{"type":"object"}}])).is_err());
        assert!(schemas(&json!([{"name":"x","inputSchema":{}}])).is_err());
    }

    #[test]
    fn mixed_catalog_requires_reserved_namespace_and_rejects_core_collisions() {
        let none = std::collections::HashSet::new();
        assert_eq!(
            mixed_schemas(
                &json!([{"name":"glass.browser.open","inputSchema":{"type":"object"}}]),
                &none,
                &none
            )
            .unwrap()
            .len(),
            1
        );
        assert_eq!(
            mixed_schemas(
                &json!([{"name":"read_file","inputSchema":{"type":"object"}}]),
                &none,
                &none
            )
            .unwrap_err(),
            "mixed-mode host tools must use the glass. namespace"
        );
        assert_eq!(
            mixed_schemas(
                &json!([{"name":"glass.bash","inputSchema":{"type":"object"}}]),
                &none,
                &none
            )
            .unwrap()
            .len(),
            1
        );
        let native = std::collections::HashSet::from(["glass.bash".to_string()]);
        assert_eq!(
            mixed_schemas(
                &json!([{"name":"glass.bash","inputSchema":{"type":"object"}}]),
                &native,
                &none
            )
            .unwrap_err(),
            "mixed-mode host tool collides with a Core tool"
        );
        let extensions = std::collections::HashSet::from(["glass.ext.run".to_string()]);
        assert_eq!(
            mixed_schemas(
                &json!([{"name":"glass.ext.run","inputSchema":{"type":"object"}}]),
                &none,
                &extensions
            )
            .unwrap_err(),
            "mixed-mode host tool collides with a Core extension"
        );
    }

    #[test]
    fn only_protocol_v2_can_open_mixed_sessions() {
        assert!(validate_tool_mode(Some(1), ToolMode::Host).is_ok());
        assert!(validate_tool_mode(Some(1), ToolMode::Local).is_ok());
        assert!(validate_tool_mode(Some(1), ToolMode::Mixed).is_err());
        assert!(validate_tool_mode(Some(2), ToolMode::Mixed).is_ok());
    }

    #[test]
    fn command_catalogue_is_gc_owned_and_marks_only_resident_commands() {
        let catalogue = runtime_command_catalogue();
        assert_eq!(catalogue["source"], "glass-core");
        assert_eq!(catalogue["completeBuiltinCatalogue"], true);
        let commands = catalogue["commands"].as_array().unwrap();
        assert!(
            commands
                .iter()
                .any(|command| command["name"] == "status" && command["runtimeMode"] == "runtime")
        );
        assert!(
            commands.iter().any(
                |command| command["name"] == "model" && command["runtimeMode"] == "interactive"
            )
        );
        assert!(
            commands
                .iter()
                .all(|command| command.get("description").is_some())
        );
    }

    #[test]
    fn mixed_session_reports_both_tool_owners() {
        let tools = vec![(
            "glass.browser.open".into(),
            "Open a browser page".into(),
            "{\"type\":\"object\"}".into(),
        )];
        assert_eq!(
            tool_owners(ToolMode::Mixed, Some(&tools)),
            json!({
                "core":{"nativeTools":true},
                "host":{"namespace":"glass.","tools":["glass.browser.open"]}
            })
        );
        assert_eq!(
            tool_owners(ToolMode::Local, None)["core"]["nativeTools"],
            true
        );
    }

    #[test]
    fn host_schema_catalog_accepts_large_workspace_tool_sets_with_a_fixed_bound() {
        let descriptor = |index| {
            json!({
                "name": format!("glass.tool_{index}.run"),
                "inputSchema": {"type":"object"}
            })
        };
        let catalog = (0..512).map(descriptor).collect::<Vec<_>>();
        assert_eq!(schemas(&Value::Array(catalog)).unwrap().len(), 512);

        let oversized = (0..513).map(descriptor).collect::<Vec<_>>();
        assert_eq!(
            schemas(&Value::Array(oversized)).unwrap_err(),
            "at most 512 tools may be registered"
        );
    }
    #[test]
    fn host_tool_names_allow_namespaces_without_path_separators() {
        for id in ["../x", "/tmp/x", "a/b", ""] {
            assert!(!valid_id(id));
        }
        assert!(valid_id("session-01_abc"));
        assert!(valid_id("glass.file.read"));
    }

    #[test]
    fn disconnected_turn_does_not_report_a_completed_or_cancelled_settlement() {
        assert_eq!(
            settlement_reason(true, crate::agent::Stop::Finished),
            "interrupted"
        );
        assert_eq!(
            settlement_reason(true, crate::agent::Stop::Cancelled),
            "interrupted"
        );
        assert_eq!(
            settlement_reason(false, crate::agent::Stop::Cancelled),
            "cancelled"
        );
        assert_eq!(
            settlement_reason(false, crate::agent::Stop::Finished),
            "completed"
        );
    }

    #[test]
    fn diagnostic_correlation_matches_request_and_event_without_raw_id() {
        let request = json!({"id":"prompt-private-value"});
        let event = json!({"params":{"turnId":"prompt-private-value"}});
        let request_hash = runtime_correlation_hash(&request).unwrap();
        let event_hash = runtime_correlation_hash(&event).unwrap();
        assert_eq!(request_hash, event_hash);
        assert!(!request_hash.contains("prompt-private-value"));
    }
}
