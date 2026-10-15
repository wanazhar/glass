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
use std::io::{BufRead, Read, Write};
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::mpsc::{self, Receiver};

const MAX_FRAME: usize = 1024 * 1024;
const MAX_PENDING: usize = 32;
type Schemas = Vec<(String, String, String)>;

fn send(value: Value) -> bool {
    let encoded = value.to_string();
    if encoded.len() + 1 > MAX_FRAME {
        return false;
    }
    let stdout = std::io::stdout();
    let mut out = stdout.lock();
    // Blocking writes propagate backpressure rather than accumulating events.
    if writeln!(out, "{encoded}")
        .and_then(|_| out.flush())
        .is_err()
    {
        std::process::exit(0);
    }
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
    send(json!({"jsonrpc":"2.0","id":id,"error":{"code":code,"message":&message[..end]}}));
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
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}
fn schemas(value: &Value) -> Result<Schemas, String> {
    let entries = value.as_array().ok_or("tools must be an array")?;
    let mut out: Schemas = Vec::new();
    if entries.len() > 128 {
        return Err("at most 128 tools may be registered".into());
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
struct Wire {
    rx: Receiver<Result<Value, String>>,
    pending: VecDeque<Value>,
    disconnected: bool,
    steer: Option<String>,
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
        let p = &m["params"];
        match m["method"].as_str() {
            Some("turn/cancel" | "turn/steer") if p["sessionId"].as_str() == Some(session) => {
                if p.get("turnId").is_some_and(|v| v != turn) {
                    error(&id, -32602, "turnId does not identify the active turn");
                    return;
                }
                let text = if m["method"] == "turn/cancel" {
                    Ok("")
                } else {
                    string(p, "text")
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
                            self.steer = Some(text.to_owned());
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
        if self.disconnected {
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
    ) -> tools::Outcome {
        let request = format!("host:{session}:{}:{}", turn, call.id);
        if call.id.len() > 256 {
            return tools::Outcome::err("Provider tool call ID exceeds 256 bytes");
        }
        if !send(json!({"jsonrpc":"2.0","id":request,"method":"tool/execute",
            "params":{"sessionId":session,"turnId":turn,"callId":call.id,"name":call.name,
                "arguments":serde_json::from_str::<Value>(&call.arguments).unwrap_or(Value::Null)}}))
        {
            return tools::Outcome::err("Host tool request exceeds 1 MiB; not dispatched");
        }
        loop {
            if self.disconnected || self.steer.is_some() {
                event(
                    session,
                    turn,
                    "tool_cancel",
                    json!({"callId":call.id,"requestId":request}),
                );
                return tools::Outcome::err(
                    "Host tool interrupted; execution status must be reconciled by the host",
                );
            }
            match self.rx.recv_timeout(std::time::Duration::from_millis(10)) {
                Ok(Ok(m)) if m.get("method").is_none() && m["id"] == request => {
                    if m.get("result").is_some() == m.get("error").is_some() {
                        return tools::Outcome::err(
                            "Host response requires exactly one of result or error",
                        );
                    }
                    let result = &m["result"];
                    if let Some(e) = m.get("error") {
                        return tools::Outcome::err(format!("Host execution failed: {e}"));
                    }
                    if result.get("isError").is_some_and(|v| !v.is_boolean()) {
                        return tools::Outcome::err("Host isError must be a boolean");
                    }
                    let Some(body) = result["text"].as_str() else {
                        return tools::Outcome::err("Host tool result requires text");
                    };
                    if body.len() > 512 * 1024 {
                        return tools::Outcome::err("Host tool result exceeds 512 KiB");
                    }
                    return if result["isError"].as_bool().unwrap_or(false) {
                        tools::Outcome::err(body)
                    } else {
                        tools::Outcome::ok(body.into())
                    };
                }
                Ok(Ok(m)) => self.active(m, session, turn),
                Ok(Err(e)) => error(&Value::Null, -32700, e),
                Err(mpsc::RecvTimeoutError::Disconnected) => self.disconnected = true,
                Err(_) => {}
            }
        }
    }
}
struct Conversation {
    session: Session,
    cfg: config::Config,
    state: tools::State,
    tools: Option<Schemas>,
    instructions: String,
}
fn metadata(
    p: &Value,
    cfg: &config::Config,
) -> Result<(config::Config, Option<Schemas>, String), String> {
    let mut cfg = cfg.clone();
    let cwd = PathBuf::from(string(p, "cwd")?);
    if !cwd.is_absolute() || !cwd.is_dir() {
        return Err("cwd must be an existing absolute directory".into());
    }
    cfg.workspace = cwd.canonicalize().map_err(|e| e.to_string())?;
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
    let host_tools = match p.get("toolMode").and_then(Value::as_str).unwrap_or("host") {
        "host" => Some(schemas(p.get("tools").unwrap_or(&json!([])))?),
        "local" => {
            if p.get("tools").is_some() {
                return Err("local mode cannot register host tools".into());
            }
            None
        }
        _ => return Err("toolMode must be host or local".into()),
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
    let instructions=p.get("instructions").map(|v|v.as_str().ok_or("instructions must be a string")).transpose()?.unwrap_or(
        if host_tools.is_some() {"You are a coding agent. Use only the supplied tools. The host owns permissions and user interactions."}else{&cfg.system_prompt}).to_owned();
    Ok((cfg, host_tools, instructions))
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
    }));
    let mut root: Option<PathBuf> = None;
    let mut conversations = BTreeMap::<String, Conversation>::new();
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
        let method = m["method"].as_str().unwrap_or("");
        let p = &m["params"];
        if method == "initialize" {
            if root.is_some() {
                error(&id, -32001, "already initialized");
                continue;
            }
            if p["protocolVersion"] != 1 {
                error(&id, -32602, "protocolVersion must be 1");
                continue;
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
            root = Some(dir.canonicalize().map_err(|e| e.to_string())?);
            reply(
                &id,
                json!({"protocolVersion":1,"runtime":{"name":"glass-core","version":env!("CARGO_PKG_VERSION")},
                "capabilities":{"hostTools":true,"localTools":true,"steering":true,"followUp":true,"cancel":true,
                    "resume":true,"fork":true,"rewind":"context_only","compact":true,"sessionConfigure":true,
                    "hostInteractions":"host tools may implement questions and approvals","concurrentTurns":false,
                    "imagePrompt":false,"completion":false},
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
            "session/new" | "session/resume" => {
                if conversations.len() >= 32 {
                    return Err("close an idle session before opening another".into());
                }
                let (settings, host_tools, instructions) = metadata(p, &cfg)?;
                let (session, manifest) = if method == "session/resume" {
                    let sid = string(p, "sessionId")?;
                    if !valid_id(sid) {
                        return Err("invalid sessionId".into());
                    }
                    let manifest: Value = serde_json::from_slice(
                        &std::fs::read(dir.join(format!("{sid}.host.json")))
                            .map_err(|e| e.to_string())?,
                    )
                    .map_err(|e| e.to_string())?;
                    if manifest != *p {
                        return Err("resume requires the original cwd, tools, instructions, and model settings; use session/configure after resume".into());
                    }
                    (
                        Session::open(dir, sid).map_err(|e| e.to_string())?,
                        manifest,
                    )
                } else {
                    let session = Session::create(dir).map_err(|e| e.to_string())?;
                    let mut manifest = p.clone();
                    manifest["sessionId"] = json!(session.id);
                    (session, manifest)
                };
                let sid = session.id.clone();
                crate::auth::secure_write(
                    &dir.join(format!("{sid}.host.json")),
                    &serde_json::to_vec(&manifest).map_err(|e| e.to_string())?,
                )
                .map_err(|e| e.to_string())?;
                conversations.insert(
                    sid.clone(),
                    Conversation {
                        session,
                        cfg: settings,
                        state: tools::State::new(),
                        tools: host_tools,
                        instructions,
                    },
                );
                Ok(json!({"sessionId":sid}))
            }
            "session/list" => {
                let mut list = Vec::new();
                for entry in std::fs::read_dir(dir).map_err(|e| e.to_string())? {
                    let entry = entry.map_err(|e| e.to_string())?;
                    let name = entry.file_name().to_string_lossy().into_owned();
                    if let Some(sid) = name.strip_suffix(".host.json") {
                        list.push(json!({"sessionId":sid,"open":conversations.contains_key(sid)}));
                    }
                }
                list.sort_by(|a, b| a["sessionId"].as_str().cmp(&b["sessionId"].as_str()));
                Ok(json!({"sessions":list}))
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
                        | "session/configure"
                        | "session/fork"
                        | "session/rewind"
                        | "turn/prompt"
                        | "turn/followUp"
                        | "session/compact"
                        | "turn/cancel"
                ) {
                    return Err(format!("method not found: {method}"));
                }
                let sid = string(p, "sessionId")?;
                let c = conversations.get_mut(sid).ok_or("unknown open session")?;
                match method {
                    "session/inspect" => {
                        let records = c.session.history_records();
                        let offset = p.get("cursor").and_then(Value::as_u64).unwrap_or(0) as usize;
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
                            "reasoningBudget":c.cfg.reasoning_budget,"toolMode":if c.tools.is_some(){"host"}else{"local"},
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
                                )
                            })
                        }) {
                            return Err("session/configure cannot change tools, ownership, or workspace; create a new session".into());
                        }
                        let mut manifest: Value = serde_json::from_slice(
                            &std::fs::read(dir.join(format!("{sid}.host.json")))
                                .map_err(|e| e.to_string())?,
                        )
                        .map_err(|e| e.to_string())?;
                        for key in [
                            "model",
                            "reasoningBudget",
                            "instructions",
                            "provider",
                            "baseUrl",
                            "connection",
                            "apiKeyEnv",
                        ] {
                            if let Some(v) = p.get(key) {
                                manifest[key] = v.clone();
                            }
                        }
                        let (settings, _, instructions) = metadata(&manifest, &cfg)?;
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
                            .branch(dir, through as usize)
                            .map_err(|e| e.to_string())?;
                        let mut manifest: Value = serde_json::from_slice(
                            &std::fs::read(dir.join(format!("{sid}.host.json")))
                                .map_err(|e| e.to_string())?,
                        )
                        .map_err(|e| e.to_string())?;
                        manifest["sessionId"] = json!(fork.id);
                        crate::auth::secure_write(
                            &dir.join(format!("{}.host.json", fork.id)),
                            &serde_json::to_vec(&manifest).map_err(|e| e.to_string())?,
                        )
                        .map_err(|e| e.to_string())?;
                        Ok(json!({"sessionId":fork.id,"open":false}))
                    }
                    "turn/prompt" | "turn/followUp" => {
                        let text = string(p, "text")?;
                        c.session
                            .append_checked(&[Record::User(text.into())])
                            .map_err(|e| e.to_string())?;
                        run(c, &wire, &id)?;
                        Ok(json!({"sessionId":sid,"settled":true}))
                    }
                    "session/rewind" => {
                        let through = p["throughTurn"]
                            .as_u64()
                            .ok_or("throughTurn must be an unsigned integer")?
                            as usize;
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
fn run(c: &mut Conversation, wire: &Rc<RefCell<Wire>>, turn: &Value) -> Result<(), String> {
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
    let mut extensions = ext::Registry::load(&[], &c.cfg.workspace, &c.cfg.model);
    let sid = c.session.id.clone();
    let statuses = Rc::new(RefCell::new(BTreeMap::<String, bool>::new()));
    let emit_status = statuses.clone();
    let mut agent=Agent::new(provider.as_ref(),&mut c.session,&mut c.state,&[],&mut extensions,&c.cfg.workspace)
        .with_image_input(c.cfg.image_input)
        .with_prompt(c.instructions.clone(),None,None)
        .with_stream_factory(factory)
        .with_limits(c.cfg.max_tokens,c.cfg.reasoning_budget,usize::MAX)
        .with_progress(|progress|match progress{
            Progress::Text(text)=>event(&sid,turn,"text",json!({"text":text})),
            Progress::Reasoning(text)=>event(&sid,turn,"reasoning",json!({"text":text})),
            Progress::ToolStart{call_id,name,command}=>event(&sid,turn,"tool_start",json!({"callId":call_id,"name":name,"title":command})),
            Progress::ToolEnd{call_id,output,..}=>event(&sid,turn,"tool_end",json!({"callId":call_id,"text":output,
                "isError":emit_status.borrow_mut().remove(call_id).unwrap_or_else(||output.starts_with("error:") || output.starts_with("[exit ") && !output.starts_with("[exit 0]"))})),
            Progress::Waiting=>event(&sid,turn,"waiting",json!({})),
            Progress::Compacted{kept,dropped}=>event(&sid,turn,"compacted",json!({"kept":kept,"dropped":dropped})),
            Progress::Steered(text)=>event(&sid,turn,"steered",json!({"text":text})),
            Progress::ProviderFailure(text)=>event(&sid,turn,"error",json!({"message":text})),
            Progress::ProviderNotice(text)=>event(&sid,turn,"notice",json!({"text":text})),
            _=>{},
        });
    if let Some(tools) = &c.tools {
        let io = wire.clone();
        let status = statuses.clone();
        let session = sid.clone();
        agent = agent.with_host_tools(
            tools.clone(),
            Box::new(move |call, _pump| {
                let result = io.borrow_mut().tool(call, &session, turn);
                status
                    .borrow_mut()
                    .insert(call.id.clone(), result.exit_code != Some(0));
                result
            }),
        );
    }
    event(&sid, turn, "turn_start", json!({}));
    // A steer interrupts the current request, preserves its transcript, and
    // immediately continues the same turn using the appended correction.
    loop {
        let outcome = agent.run_turn(|slot| wire.borrow_mut().pump(slot, &sid, turn));
        match outcome {
            Ok((out, Stop::Steered)) => {
                if wire.borrow().disconnected {
                    break;
                }
                if let Some(usage) = out.usage {
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
                    event(
                        &sid,
                        turn,
                        "usage",
                        json!({"input":usage.input,"cacheRead":usage.cache_read,"cacheWrite":usage.cache_write,"output":usage.output}),
                    );
                }
                event(
                    &sid,
                    turn,
                    "settled",
                    json!({"reason":if stop==Stop::Cancelled{"cancelled"}else{"completed"}}),
                );
                break;
            }
            Err(e) => {
                event(
                    &sid,
                    turn,
                    "settled",
                    json!({"reason":"failed","message":e.to_string()}),
                );
                return Err(e.to_string());
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
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
    fn session_identifiers_cannot_escape_storage() {
        for id in ["../x", "/tmp/x", "a/b", "x.jsonl", ""] {
            assert!(!valid_id(id));
        }
        assert!(valid_id("session-01_abc"));
    }
}
