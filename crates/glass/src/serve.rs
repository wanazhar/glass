//! `glass-core serve` / `glass-core attach`: a session that outlives the window.
//!
//! `serve` owns the session loop — provider, tools, checkpoints, ledger — and
//! speaks newline-delimited JSON over a Unix socket (0600). `attach` is a
//! small terminal client: it replays the conversation, streams live output,
//! submits prompts, and cancels; detaching never stops the server. Ported
//! from Crush's shared workspaces, `claude attach`, and Amp's threads
//! (research/harness-features-2026.md).
//!
//! Protocol (one JSON object per line):
//!
//! client → server: `{"t":"attach"}` · `{"t":"prompt","text":…}` ·
//! `{"t":"cancel"}`
//!
//! server → client: `{"t":"replay","session":…,"entries":[{"role","text"}…]}`
//! · `{"t":"text","delta":…}` · `{"t":"tool","command":…}` ·
//! `{"t":"tool_end","summary":…}` · `{"t":"done","stop":…,"text":…}` ·
//! `{"t":"error","message":…}`
//!
//! Up to four clients attach at once: each attach gets its own replay,
//! live output broadcasts to all of them, and a closed terminal is dropped
//! rather than stalling the session. Slash commands stay local to the
//! interactive session. The 0600 Unix socket is the auth boundary — same
//! user, same machine — which is why there is no token scheme to get wrong.
//! Loopback-only by construction.

use std::collections::VecDeque;
use std::io::{Read, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::Path;
use std::path::PathBuf;
use std::process::Command;
use std::hash::{Hash, Hasher};

use serde_json::{Value, json};

use crate::agent::{Agent, Progress, Stop};
use crate::config;
use crate::ext;
use crate::session::{Record, Session};

const REPLAY_ENTRIES: usize = 200;
const REPLAY_BYTES: usize = 256 * 1024;
const REPLAY_ENTRY_BYTES: usize = 32 * 1024;
const PROTOCOL_VERSION: u64 = 1;
/// Concurrent attaches per served session.
const MAX_CLIENTS: usize = 4;

#[derive(Default, serde::Serialize, serde::Deserialize)]
#[serde(default)]
struct AttachState {
    session: Option<String>,
    cursor: usize,
    request_seq: u64,
    pending: Option<PendingRequest>,
}

#[derive(Clone, serde::Serialize, serde::Deserialize)]
struct PendingRequest {
    id: String,
    text: String,
}

fn attach_state_path(socket: &Path) -> PathBuf {
    let mut hash = std::collections::hash_map::DefaultHasher::new();
    socket.hash(&mut hash);
    config::data_dir().join(format!("attach-{:016x}.json", hash.finish()))
}

fn load_attach_state(path: &Path) -> AttachState {
    std::fs::read(path).ok().and_then(|bytes| serde_json::from_slice(&bytes).ok()).unwrap_or_default()
}

fn save_attach_state(path: &Path, state: &AttachState) -> Result<(), String> {
    let bytes = serde_json::to_vec(state).map_err(|error| error.to_string())?;
    crate::auth::secure_write(path, &bytes)
}

fn update_attach_state(path: &Path, state: &mut AttachState, message: &Value) -> Result<(), String> {
    match message.get("t").and_then(Value::as_str) {
        Some("replay") => {
            let session = message.get("session").and_then(Value::as_str).map(str::to_owned);
            if state.session != session {
                state.cursor = 0;
                state.pending = None;
                state.session = session;
            }
            if let Some(cursor) = message.get("cursor").and_then(Value::as_u64) {
                state.cursor = cursor.min(usize::MAX as u64) as usize;
            }
        }
        Some("done" | "error" | "duplicate") => {
            if let Some(cursor) = message.get("cursor").and_then(Value::as_u64) {
                state.cursor = cursor.min(usize::MAX as u64) as usize;
            }
            let response_id = message.get("request_id").and_then(Value::as_str);
            if state.pending.as_ref().is_some_and(|pending| response_id == Some(pending.id.as_str())) {
                state.pending = None;
            }
        }
        _ => return Ok(()),
    }
    save_attach_state(path, state)
}

fn attach_message(state: &AttachState) -> Value {
    json!({"t":"attach", "protocol_version":PROTOCOL_VERSION, "since":state.cursor, "session":state.session})
}

fn connect_cancellable(path: &Path, keys: &crate::input::KeyReader, out: &mut std::io::Stdout) -> Result<Option<UnixStream>, String> {
    let mut delay = std::time::Duration::from_millis(250);
    loop {
        match UnixStream::connect(path) {
            Ok(stream) => {
                stream.set_nonblocking(true).map_err(|error| error.to_string())?;
                let _ = writeln!(out, "\r\n\x1b[32mconnection available · replay cursor retained\x1b[0m");
                let _ = out.flush();
                return Ok(Some(stream));
            }
            Err(error) => {
                let _ = write!(out, "\r\x1b[33mconnection lost · retrying in {:.2}s · Ctrl-C cancel\x1b[0m", delay.as_secs_f32());
                let _ = out.flush();
                if let Some(glass_tui::crossterm::event::Event::Key(key)) = keys.next_timeout(delay)
                    && key.modifiers.contains(glass_tui::crossterm::event::KeyModifiers::CONTROL)
                    && matches!(key.code, glass_tui::crossterm::event::KeyCode::Char('c' | 'd'))
                {
                    return Ok(None);
                }
                delay = (delay * 2).min(std::time::Duration::from_secs(5));
                let _ = error;
            }
        }
    }
}

fn send(stream: &mut UnixStream, value: Value) {
    if let Ok(mut line) = serde_json::to_string(&value) {
        line.push('\n');
        let _ = stream.write_all(line.as_bytes());
        let _ = stream.flush();
    }
}

/// Send to every attached client, dropping any whose pipe stopped taking
/// bytes — a closed terminal must never stall the session.
fn broadcast(clients: &std::rc::Rc<std::cell::RefCell<Vec<Client>>>, value: Value) {
    let Ok(mut line) = serde_json::to_string(&value) else {
        return;
    };
    line.push('\n');
    let mut slots = clients.borrow_mut();
    slots.retain_mut(|c| {
        c.stream
            .write_all(line.as_bytes())
            .and_then(|_| c.stream.flush())
            .is_ok()
    });
}

enum Incoming {
    Message(Value),
    /// Nothing complete arrived this time.
    Idle,
    Malformed,
    Closed,
}

struct Client {
    id: u64,
    stream: UnixStream,
    buf: Vec<u8>,
}

impl Client {
    fn try_incoming(&mut self) -> Incoming {
        let mut chunk = [0u8; 4096];
        loop {
            match self.stream.read(&mut chunk) {
                Ok(0) => return Incoming::Closed,
                Ok(n) => {
                    if self.buf.len()+n > 128*1024 { return Incoming::Closed; }
                    self.buf.extend_from_slice(&chunk[..n]);
                    if self.buf.contains(&b'\n') { break; }
                },
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => break,
                Err(_) => return Incoming::Closed,
            }
        }
        let Some(pos) = self.buf.iter().position(|b| *b == b'\n') else {
            return Incoming::Idle;
        };
        let line: Vec<u8> = self.buf.drain(..=pos).collect();
        match serde_json::from_slice::<Value>(&line[..line.len() - 1]) {
            Ok(value) if value.is_object() => Incoming::Message(value),
            _ => Incoming::Malformed,
        }
    }
}

/// Transcript entries for the raw-record range `[start, end)`.
fn entries_between(records: &[Record], start: usize, end: usize) -> Vec<Value> {
    records[start.min(end)..end]
        .iter()
        .filter_map(|record| match record {
            Record::User(text) => Some(json!({"role": "user", "text": text})),
            Record::Assistant(text) => Some(json!({"role": "assistant", "text": text})),
            Record::Bash {
                command, output, ..
            } => Some(json!({"role": "tool", "text": format!("$ {command}\n{output}")})),
            _ => None,
        })
        .collect()
}

/// Replay for an attach: the entries, the cursor the client should keep, and
/// whether anything before the range was skipped.
///
/// The cursor is the raw-record count — a durable, append-only position, so
/// a client that kept one receives only what is new (`since=…`) instead of a
/// fixed window. Two cases force a reset: no cursor at all, or one ahead of
/// the log (a client from another session lineage); a `since` older than the
/// retained window is clamped and flagged so the client knows it missed
/// entries rather than silently believing it is current.
fn replay_from(session: &Session, since: Option<usize>) -> (Vec<Value>, usize, bool) {
    let records = crate::export::raw_records(session);
    let cursor = records.len();
    let floor = cursor.saturating_sub(REPLAY_ENTRIES);
    let (start, reset) = match since {
        Some(n) if n <= cursor => (n.max(floor), n < floor),
        _ => (floor, since.is_some()),
    };
    let entries = entries_between(&records, start, cursor);
    let (entries, clipped) = bound_replay(entries);
    (entries, cursor, reset || clipped)
}

/// Retain recent entries within a wire-size budget. Clipped text remains UTF-8;
/// each entry tells clients explicitly when the full transcript is needed.
fn bound_replay(entries: Vec<Value>) -> (Vec<Value>, bool) {
    let mut retained = Vec::new();
    let mut bytes = 2;
    let mut clipped = false;
    for mut entry in entries.into_iter().rev() {
        if let Some(text) = entry.get("text").and_then(Value::as_str)
            && text.len() > REPLAY_ENTRY_BYTES {
            let mut end = REPLAY_ENTRY_BYTES;
            while !text.is_char_boundary(end) { end -= 1; }
            let source_bytes = text.len();
            let preview = text[..end].to_owned();
            entry["source_bytes"] = json!(source_bytes);
            entry["text"] = json!(preview);
            entry["truncated"] = json!(true);
            clipped = true;
        }
        let size = serde_json::to_vec(&entry).map(|v| v.len() + 1).unwrap_or(REPLAY_BYTES);
        if bytes + size > REPLAY_BYTES { clipped = true; break; }
        bytes += size;
        retained.push(entry);
    }
    retained.reverse();
    (retained, clipped)
}

/// The newest server socket in the data dir, for `attach` without `--socket`.
fn default_socket() -> Result<PathBuf, String> {
    let dir = config::data_dir();
    let newest = std::fs::read_dir(&dir)
        .map_err(|e| e.to_string())?
        .filter_map(Result::ok)
        .filter(|e| {
            e.file_name().to_string_lossy().starts_with("serve-")
                && e.path().extension().is_some_and(|x| x == "sock")
        })
        .max_by_key(|e| e.metadata().ok().and_then(|m| m.modified().ok()))
        .map(|e| e.path());
    newest.ok_or_else(|| "no served session found; start one with `glass-core serve`".into())
}

pub fn serve(
    cfg: config::Config,
    session_id: Option<&str>,
    socket: Option<&str>,
) -> Result<(), String> {
    let _signals = crate::signals::Guard::install();
    let dir = config::data_dir();
    let mut session = match session_id {
        Some(id) => Session::open(&dir, id).map_err(|e| e.to_string())?,
        None => match crate::polish::sessions(&cfg.workspace).into_iter().next() {
            Some(s) => s,
            None => Session::create(&dir).map_err(|e| e.to_string())?,
        },
    };
    crate::polish::bind(&session, &cfg.workspace)?;

    let socket_path = match socket {
        Some(path) => PathBuf::from(path),
        None => dir.join(format!("serve-{}.sock", session.id)),
    };
    if std::fs::symlink_metadata(&socket_path).is_ok() {
        if UnixStream::connect(&socket_path).is_ok() { return Err("Socket already serves a live session; choose another socket".into()); }
        use std::os::unix::fs::FileTypeExt;
        if !std::fs::symlink_metadata(&socket_path).map_err(|e|e.to_string())?.file_type().is_socket() { return Err("Refusing to replace a non-socket path".into()); }
        std::fs::remove_file(&socket_path).map_err(|e|e.to_string())?;
    }
    let listener = UnixListener::bind(&socket_path)
        .map_err(|e| format!("cannot bind {}: {e}", socket_path.display()))?;
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&socket_path, std::fs::Permissions::from_mode(0o600))
            .map_err(|e| e.to_string())?;
    }
    crate::cli_println!("glass-core: serving session {}", session.id);
    crate::cli_println!("glass-core: socket {}", socket_path.display());
    crate::cli_println!(
        "glass-core: attach with `glass-core attach --socket {}`",
        socket_path.display()
    );

    let mut state = crate::tools::State::new();
    for r in &session.records {
        if let Record::State(items) = r {
            state.apply(&glass_mod_api::ToolCall {
                id: "restore".into(),
                name: "state".into(),
                arguments: serde_json::json!({"items": items}).to_string(),
            });
        }
    }
    let mods = crate::mods::load(&cfg.mods)?;
    let mut extensions = ext::Registry::load(&cfg.extensions, &cfg.workspace, &cfg.model);
    if let Ok(records) = Session::raw_records(&session.path) {
        extensions.seed_custom(&records);
    }
    let ledger_path = session.path.with_extension("cost.json");
    let ledger = crate::budget::load(&ledger_path, cfg.cost_limit);
    let factory: crate::warmer::ProviderFactory = {
        let configured = cfg.clone();
        let ledger = std::sync::Arc::clone(&ledger);
        let path = std::sync::Arc::new(std::sync::Mutex::new(ledger_path.clone()));
        std::sync::Arc::new(move || {
            crate::make_provider(
                &configured,
                std::sync::Arc::clone(&ledger),
                std::sync::Arc::clone(&path),
                None,
            )
        })
    };
    let factory_for_model: crate::warmer::ProviderForModel = {
        let configured = cfg.clone();
        let ledger = std::sync::Arc::clone(&ledger);
        let path = std::sync::Arc::new(std::sync::Mutex::new(ledger_path));
        std::sync::Arc::new(move |model: &str| {
            let configured = configured.clone();
            let ledger = std::sync::Arc::clone(&ledger);
            let path = std::sync::Arc::clone(&path);
            let model = model.to_string();
            let factory: crate::warmer::ProviderFactory = std::sync::Arc::new(move || {
                crate::make_provider(&configured, std::sync::Arc::clone(&ledger), std::sync::Arc::clone(&path), Some(&model))
            });
            factory
        })
    };
    let _team = crate::team::start(&session.id, &cfg.workspace, std::sync::Arc::clone(&factory), factory_for_model, &cfg.model)?;
    let instructions = crate::trust::gated(&cfg, &crate::agents_md::load(&cfg.workspace), false);

    let clients = std::rc::Rc::new(std::cell::RefCell::new(Vec::<Client>::new()));
    let mut client_sequence = 0u64;
    let owner = std::rc::Rc::new(std::cell::Cell::new(None::<u64>));
    let mut mailbox = crate::inbox::Mailbox::open(&session);
    let mut pending: VecDeque<Value> = VecDeque::new();
    // The accepted marker is committed before any prompt work starts. On a
    // restart an accepted-but-uncompleted request is reported as interrupted,
    // never replayed because its effects may have partially landed.
    let mut seen_requests: std::collections::HashMap<String, String> =
        Session::raw_records(&session.path)
            .unwrap_or_default()
            .into_iter()
            .filter_map(|record| match record {
                Record::ServeRequest { id, status } => Some((id, status)),
                _ => None,
            })
            .collect();
    let failure;
    listener.set_nonblocking(true).map_err(|e| e.to_string())?;

    'serve: loop {
        if crate::signals::interrupted() {
            failure = None;
            break 'serve;
        }
        // Attaches accumulate (up to a cap): mirroring one session into two
        // terminals is the point, and a new attach no longer evicts the old
        // one. Same-user access to a 0600 socket is the whole auth story.
        match listener.accept() {
            Ok((stream, _)) => {
                let _ = stream.set_nonblocking(true);
                let mut slots = clients.borrow_mut();
                if slots.len() >= MAX_CLIENTS {
                    // Preserve existing clients, particularly the input owner.
                    drop(stream);
                    continue;
                }
                client_sequence = client_sequence.checked_add(1).ok_or("Client sequence exhausted")?;
                slots.push(Client {
                    id: client_sequence,
                    stream,
                    buf: Vec::new(),
                });
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {}
            Err(e) => {
                failure = Some(e.to_string());
                break 'serve;
            }
        }

        // One message per iteration, with the client it came from (None for
        // messages from the mid-turn queue or produced by a loop).
        let message: Option<(Option<usize>, Value)> = match pending.pop_front() {
            Some(message) => Some((None, message)),
            None => {
                let mut found = None;
                let mut dead: Vec<usize> = Vec::new();
                {
                    let mut slots = clients.borrow_mut();
                    for (i, c) in slots.iter_mut().enumerate() {
                        match c.try_incoming() {
                            Incoming::Message(message) => {
                                found = Some((Some(i), message));
                                break;
                            }
                            Incoming::Closed => dead.push(i),
                            Incoming::Malformed | Incoming::Idle => {}
                        }
                    }
                    // Removing the dead shifts every later index down; the
                    // sender's index has to follow, or the reply goes to the
                    // wrong client (or to nobody).
                    if let Some((Some(sender), _)) = &found {
                        let sender = *sender;
                        let shift = dead.iter().filter(|d| **d < sender).count();
                        if shift > 0 {
                            found = found.map(|(_, message)| (Some(sender - shift), message));
                        }
                    }
                    for i in dead.into_iter().rev() {
                        slots.remove(i);
                    }
                }
                match found {
                    Some(found) => Some(found),
                    None => {
                        // Messages from other sessions first, then due loops:
                        // both become prompts when nobody is talking, and
                        // while a turn runs this branch is unreachable, so
                        // neither stacks behind live work. This runs even
                        // with no client attached: that is the autonomy story.
                        if let Some((from, text)) = mailbox.deliver(&mut session)? {
                            broadcast(
                                &clients,
                                json!({"t": "text", "delta": format!("\n\x1b[90m[inbox from {from}]\x1b[0m {text}\n")}),
                            );
                            Some((None, json!({"t": "prompt", "text": text, "inbox_recorded":true})))
                        } else if let Some(prompt) = crate::loops::due_for(&mut session,&cfg)? {
                            broadcast(
                                &clients,
                                json!({"t": "text", "delta": format!("\n\x1b[90m[loop]\x1b[0m {prompt}\n")}),
                            );
                            Some((None, json!({"t": "prompt", "text": prompt, "inbox_recorded":true})))
                        } else {
                            std::thread::sleep(std::time::Duration::from_millis(30));
                            continue;
                        }
                    }
                }
            }
        };
        let Some((origin, message)) = message else {
            continue;
        };

        // Send to the client that asked, if it is still attached.
        let to_origin = |value: Value| {
            if let Some(i) = origin
                && let Some(c) = clients.borrow_mut().get_mut(i)
            {
                send(&mut c.stream, value);
            }
        };

        if owner.get().is_some_and(|id| !clients.borrow().iter().any(|c|c.id==id)) { owner.set(None); }
        let sender_id=origin.and_then(|i|clients.borrow().get(i).map(|c|c.id));
        if origin.is_some() && matches!(message["t"].as_str(),Some("prompt"|"cancel")) && sender_id!=owner.get() {
            to_origin(json!({"t":"error","request_id":message.get("request_id"),"message":"Read-only observer: claim control before sending input","owner":owner.get()}));
            continue;
        }
        match message.get("t").and_then(Value::as_str) {
            Some("control") => {
                let action = message["action"].as_str();
                if !matches!(action, Some("claim" | "release")) {
                    to_origin(json!({"t":"error","message":"Control action must be claim or release"}));
                    continue;
                }
                let release=action==Some("release");
                if release && sender_id==owner.get() { owner.set(None); }
                else if !release && owner.get().is_none() { owner.set(sender_id); }
                to_origin(json!({"t":"control","owner":owner.get(),"client":sender_id,"can_write":sender_id.is_some()&&sender_id==owner.get()}));
            }
            Some("attach") => {
                if message.get("protocol_version").is_some_and(|v| v.as_u64() != Some(PROTOCOL_VERSION)) {
                    to_origin(json!({"t":"error","message":"Unsupported attach protocol version","protocol_version":PROTOCOL_VERSION}));
                    continue;
                }
                if owner.get().is_none() && message["observe"]!=Value::Bool(true) { owner.set(sender_id); }
                let claimed_session = message.get("session").and_then(Value::as_str);
                let wrong_session = claimed_session.is_some_and(|id| id != session.id);
                let since = message
                    .get("since")
                    .and_then(Value::as_u64)
                    .map(|n| n as usize).filter(|_| !wrong_session);
                let (entries, cursor, reset) = replay_from(&session, since);
                to_origin(json!({
                    "t": "replay",
                    "protocol_version": PROTOCOL_VERSION,
                    "replay_byte_limit": REPLAY_BYTES,
                    "client":sender_id,"owner":owner.get(),"can_write":sender_id.is_some()&&sender_id==owner.get(),
                    "session": session.id,
                    "entries": entries,
                    "cursor": cursor,
                    "reset": reset || wrong_session,
                }));
            }
            Some("prompt") => {
                if origin.is_some() && message.get("inbox_recorded").is_some() {
                    to_origin(json!({"t":"error","message":"Reserved delivery field"}));
                    continue;
                }
                let text = message
                    .get("text")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .trim()
                    .to_string();
                if text.is_empty() {
                    to_origin(
                        json!({"t": "error", "request_id":message.get("request_id"), "message": "empty prompt"}),
                    );
                    continue;
                }
                if let Some(id) = message.get("request_id").and_then(Value::as_str) {
                    if let Some(status) = seen_requests.get(id) {
                        to_origin(json!({"t": "duplicate", "request_id": id, "status": status,
                            "cursor":crate::export::raw_records(&session).len(),
                            "message": if status == "accepted" { "prior request was interrupted; it will not be replayed" } else if status == "failed" { "prior request failed before execution; submit a new request to retry" } else { "request already completed" }}));
                        continue;
                    }
                    if let Err(error) = session.append_checked(&[Record::ServeRequest {
                        id: id.to_string(),
                        status: "accepted".into(),
                    }]) {
                        to_origin(
                            json!({"t":"error", "request_id":message.get("request_id"), "message": format!("cannot durably accept request; prompt was not run: {error}")}),
                        );
                        continue;
                    }
                    seen_requests.insert(id.to_string(), "accepted".into());
                }
                if message["inbox_recorded"] != true
                    && let Err(error) = session.append_checked(&[Record::User(text)]) {
                    to_origin(
                        json!({"t":"error", "request_id":message.get("request_id"), "message": format!("cannot durably record prompt; it was not run: {error}")}),
                    );
                    continue;
                }
                let provider = match factory() {
                    Ok(p) => p,
                    Err(e) => {
                        if let Err(error) = crate::navigation::record(&session, &cfg.workspace, &cfg.model, "blocked") { eprintln!("navigation receipt: {error}"); }
                        let mut detail = crate::live::sanitize_provider_detail(&e);
                        if let Some(id) = message.get("request_id").and_then(Value::as_str) {
                            match session.append_checked(&[Record::ServeRequest {
                                id: id.to_string(),
                                status: "failed".into(),
                            }]) {
                                Ok(()) => {
                                    seen_requests.insert(id.to_string(), "failed".into());
                                }
                                Err(error) => {
                                    detail.push_str(&format!("; cannot record failure: {error}"))
                                }
                            }
                        }
                        to_origin(json!({"t": "error", "request_id":message.get("request_id"),
                            "cursor":crate::export::raw_records(&session).len(), "message":detail}));
                        continue;
                    }
                };
                let outer = std::rc::Rc::clone(&clients);
                let input_owner=std::rc::Rc::clone(&owner);
                let mut queued: Vec<Value> = Vec::new();
                if let Err(error) = crate::team::record_notices(&mut session) { eprintln!("team receipt: {error}"); }
                if let Err(error) = crate::navigation::record(&session, &cfg.workspace, &cfg.model, "running") { eprintln!("navigation receipt: {error}"); }
                let outcome = {
                    let mut agent = Agent::new(
                        provider.as_ref(),
                        &mut session,
                        &mut state,
                        &mods,
                        &mut extensions,
                        &cfg.workspace,
                    )
                    .prime_extensions()
                    .with_prompt(cfg.system_prompt.clone(), None, instructions.clone())
                    .with_live(cfg.live_flush_ms as u64)
                    .with_stream_factory(std::sync::Arc::clone(&factory))
                    .with_progress(move |progress| match progress {
                        Progress::Text(t) => broadcast(&outer, json!({"t": "text", "delta": t})),
                        Progress::ToolStart { call_id, name, command } => broadcast(
                            &outer,
                            json!({"t": "tool", "call_id": call_id, "name": name, "command": command}),
                        ),
                        Progress::ToolEnd { call_id, summary, .. } => {
                            broadcast(&outer, json!({"t": "tool_end", "call_id":call_id,"summary": summary}))
                        }
                        Progress::Retrying { attempt, delay_s, reason } => broadcast(&outer, json!({"t":"retrying", "attempt":attempt, "delay_s":delay_s, "reason":crate::live::sanitize_provider_detail(&reason)})),
                        Progress::ProviderNotice(text) => broadcast(&outer, json!({"t":"provider_notice", "message":crate::live::sanitize_provider_detail(text)})),
                        Progress::ProviderFailure(text) => broadcast(&outer, json!({"t":"provider_error", "message":crate::live::sanitize_provider_detail(text)})),
                        _ => {}
                    });
                    let outer = std::rc::Rc::clone(&clients);
                    agent.run_turn(|steer| {
                        if crate::signals::interrupted() {
                            *steer = Some(String::new());
                        }
                        let mut slots = outer.borrow_mut();
                        let mut dead: Vec<usize> = Vec::new();
                        for (i, c) in slots.iter_mut().enumerate() {
                            loop {
                                match c.try_incoming() {
                                    Incoming::Message(message) => {
                                        if Some(c.id)!=input_owner.get() && matches!(message["t"].as_str(),Some("prompt"|"cancel")) {
                                            send(&mut c.stream,json!({"t":"error","request_id":message.get("request_id"),"message":"Read-only observer cannot change an active turn"}));
                                        } else {
                                            match message.get("t").and_then(Value::as_str) {
                                                Some("cancel") => *steer = Some(String::new()),
                                                Some("prompt") if queued.len()<32 => queued.push(message),
                                                _ => send(&mut c.stream,json!({"t":"error","message":"Control and replay requests are available after this turn; queued input is limited to 32 prompts"})),
                                            }
                                        }
                                    }
                                    Incoming::Malformed | Incoming::Idle => break,
                                    Incoming::Closed => {
                                        dead.push(i);
                                        break;
                                    }
                                }
                            }
                        }
                        for i in dead.into_iter().rev() {
                            slots.remove(i);
                        }
                    })
                };
                pending.extend(queued);
                let status = match &outcome { Err(_) => "blocked", Ok((_, Stop::Cancelled)) => "cancelled", Ok((_, Stop::Steered)) => "steered", Ok((_, Stop::ToolBudget)) => "incomplete", Ok((_, Stop::Finished)) => "completed" };
                if let Err(error) = crate::navigation::record(&session, &cfg.workspace, &cfg.model, status) { eprintln!("navigation receipt: {error}"); }
                if matches!(&outcome, Ok((_, Stop::Finished))) && let Err(error) = crate::advisor::milestone(&session) { eprintln!("advisor: {error}"); }

                let (stop, text) = match outcome {
                    Ok((out, stop)) => (stop, out.text),
                    Err(e) => {
                        broadcast(
                            &clients,
                            json!({"t": "error", "request_id":message.get("request_id"), "cursor":crate::export::raw_records(&session).len(), "message": format!("turn failed: {e}")}),
                        );
                        continue;
                    }
                };
                let stop = match stop {
                    Stop::Finished => "finished",
                    Stop::Cancelled => "cancelled",
                    Stop::Steered => "steered",
                    Stop::ToolBudget => "tool_budget",
                };
                if let Some(id) = message.get("request_id").and_then(Value::as_str) {
                    if let Err(error) = session.append_checked(&[Record::ServeRequest {
                        id: id.to_string(),
                        status: "completed".into(),
                    }]) {
                        to_origin(
                            json!({"t":"error", "message": format!("turn completed but request status could not be recorded: {error}")}),
                        );
                    } else {
                        seen_requests.insert(id.to_string(), "completed".into());
                    }
                }
                broadcast(&clients, json!({"t": "done", "request_id":message.get("request_id"), "cursor":crate::export::raw_records(&session).len(), "stop": stop, "text": text}));
            }
            Some("cancel") => { /* no turn running: nothing to cancel */ }
            Some("stop") => {
                // Graceful shutdown. The turn still running, if any,
                // finishes first — the frame is queued behind it like
                // any other message — then the loop exit path shuts
                // extensions down and removes the socket.
                to_origin(json!({"t": "stopped", "session": session.id}));
                failure = None;
                break 'serve;
            }
            other => {
                to_origin(json!({"t": "error", "message": format!("unknown message {:?}", other)}));
            }
        }
    }

    extensions.shutdown();
    let _ = std::fs::remove_file(&socket_path);
    failure.map_or(Ok(()), Err)
}

/// Entries a `logs` run prints: the tail of the session log.
const LOG_ENTRIES: usize = 40;

/// The session id a socket belongs to: `serve-<id>.sock`.
fn socket_session(socket: &std::path::Path) -> Option<String> {
    socket
        .file_stem()
        .map(|s| s.to_string_lossy().trim_start_matches("serve-").to_string())
        .filter(|s| !s.is_empty())
}

/// Stop a served session: connect to its server and ask it to exit.
///
/// The server finishes any turn still running — the frame is
/// queued behind it like any other message — then shuts down and
/// removes its socket. The reply is read until EOF so a caller
/// knows the server is fully down, not merely acknowledged.
pub fn stop(session_id: Option<&str>) -> Result<(), String> {
    let socket = match session_id {
        Some(id) => config::data_dir().join(format!("serve-{id}.sock")),
        None => default_socket()?,
    };
    let id = socket_session(&socket).unwrap_or_else(|| "unknown".into());
    let mut stream = UnixStream::connect(&socket)
        .map_err(|e| format!("cannot connect to {}: {e}", socket.display()))?;
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
    send(&mut stream, json!({"t": "stop"}));
    let mut buf = [0u8; 512];
    let mut pending = Vec::new();
    let mut oversized_frame = false;
    let mut stopped = false;
    let mut completed = false;
    loop {
        let remaining = deadline.saturating_duration_since(std::time::Instant::now());
        if remaining.is_zero() {
            break;
        }
        stream
            .set_read_timeout(Some(remaining))
            .map_err(|e| e.to_string())?;
        match stream.read(&mut buf) {
            Ok(0) => {
                completed = true;
                break;
            }
            Ok(n) => {
                pending.extend_from_slice(&buf[..n]);
                stopped |= consume_stop_frames(&mut pending, &mut oversized_frame);
            }
            Err(e)
                if e.kind() == std::io::ErrorKind::WouldBlock
                    || e.kind() == std::io::ErrorKind::TimedOut =>
            {
                break;
            }
            Err(e) => return Err(e.to_string()),
        }
    }
    if stop_completed(stopped, completed, &pending) {
        crate::cli_println!("glass-core: stopped session {id}");
        Ok(())
    } else {
        Err(format!(
            "server for session {id} did not complete the stop (acknowledged: {stopped}, EOF: {completed})"
        ))
    }
}

const MAX_STOP_FRAME: usize = 64 * 1024;

fn consume_stop_frames(pending: &mut Vec<u8>, oversized_frame: &mut bool) -> bool {
    let mut stopped = false;
    while let Some(pos) = pending.iter().position(|b| *b == b'\n') {
        let frame: Vec<_> = pending.drain(..=pos).collect();
        if !*oversized_frame
            && frame.len() <= MAX_STOP_FRAME
            && let Ok(value) = serde_json::from_slice::<Value>(&frame[..frame.len() - 1])
        {
            stopped |= value.get("t").and_then(Value::as_str) == Some("stopped");
        }
        *oversized_frame = false;
    }
    if pending.len() > MAX_STOP_FRAME {
        pending.clear();
        *oversized_frame = true;
    }
    stopped
}

fn stop_completed(stopped: bool, eof: bool, pending: &[u8]) -> bool {
    stopped && eof && pending.is_empty()
}

/// Respawn a served session: stop the running server, if any,
/// then start a fresh one for the same session.
///
/// The new server is detached into its own process group so it
/// outlives the command and the shell that ran it; its startup
/// lines stay on this terminal. With no id, the newest served
/// session is respawned — or, with none running, the newest
/// session in this workspace.
pub fn respawn(session_id: Option<&str>) -> Result<(), String> {
    let id = match session_id {
        Some(id) => Some(id.to_string()),
        None => default_socket().ok().as_deref().and_then(socket_session),
    };
    // Stop the running server first. A session with no server
    // is not an error: respawn then just starts one.
    if let Some(id) = &id {
        let socket = config::data_dir().join(format!("serve-{id}.sock"));
        if socket.exists() {
            stop(Some(id))?;
        }
    }
    let exe = std::env::current_exe().map_err(|e| format!("cannot find glass-core: {e}"))?;
    let mut cmd = Command::new(exe);
    cmd.arg("serve");
    if let Some(id) = &id {
        cmd.arg(id);
    }
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        cmd.process_group(0);
    }
    cmd.spawn()
        .map_err(|e| format!("cannot respawn the server: {e}"))?;
    match &id {
        Some(id) => crate::cli_println!("glass-core: respawning session {id}"),
        None => crate::cli_println!("glass-core: respawning the newest session"),
    }
    Ok(())
}

/// Show a session's recent activity from its log.
///
/// With no id, the newest session in the workspace. Reads the
/// durable log, so it works whether or not a server is running.
pub fn logs(session_id: Option<&str>, workspace: &Path) -> Result<(), String> {
    let dir = config::data_dir();
    let id = match session_id {
        Some(id) => id.to_string(),
        None => newest_session_id_readonly(&dir, workspace)
            .ok_or_else(|| "no sessions in this workspace".to_string())?,
    };
    let path = if id.ends_with(".jsonl") {
        PathBuf::from(&id)
    } else {
        dir.join(format!("{id}.jsonl"))
    };
    let records = Session::raw_records(&path).map_err(|e| e.to_string())?;
    let entries: Vec<String> = records
        .iter()
        .rev()
        .take(LOG_ENTRIES)
        .rev()
        .filter_map(|record| match record {
            Record::User(text) => Some(format!("❯ {text}")),
            Record::Assistant(text) => Some(text.clone()),
            Record::Bash {
                command, exit_code, ..
            } => Some(format!(
                "$ {command}{}",
                exit_code
                    .map(|code| format!(" [exit {code}]"))
                    .unwrap_or_default()
            )),
            Record::ToolResult { name, body, .. } if name != "bash" => {
                Some(format!("  {name}: {}", glass_tui::text::clip(body, 200)))
            }
            Record::Custom { kind, .. } => Some(format!("  [extension entry: {kind}]")),
            _ => None,
        })
        .collect();
    if entries.is_empty() {
        crate::cli_println!("no activity in session {id}");
        return Ok(());
    }
    crate::cli_println!(
        "glass-core: last {} entries of session {id}\n",
        entries.len()
    );
    for entry in entries {
        crate::cli_println!("{entry}\n");
    }
    Ok(())
}

fn newest_session_id_readonly(dir: &Path, workspace: &Path) -> Option<String> {
    let root = workspace
        .canonicalize()
        .unwrap_or_else(|_| workspace.into());
    let mut files: Vec<_> = std::fs::read_dir(dir)
        .ok()?
        .filter_map(Result::ok)
        .filter(|entry| entry.path().extension().is_some_and(|ext| ext == "jsonl"))
        .collect();
    files.sort_by_key(|entry| {
        std::cmp::Reverse(entry.metadata().ok().and_then(|m| m.modified().ok()))
    });
    files.into_iter().find_map(|entry| {
        let metadata_path = entry.path().with_extension("meta.json");
        let metadata: crate::polish::Metadata =
            serde_json::from_slice(&std::fs::read(metadata_path).ok()?).ok()?;
        if metadata.workspace == root {
            entry
                .path()
                .file_stem()
                .map(|stem| stem.to_string_lossy().into_owned())
        } else {
            None
        }
    })
}

/// A small terminal client for a served session.
pub fn attach(socket: Option<&str>) -> Result<(), String> {
    if !crate::term::stdin_is_tty() {
        return Err("attach needs an interactive terminal".into());
    }
    let socket = match socket {
        Some(path) => PathBuf::from(path),
        None => default_socket()?,
    };
    crate::term::enter().map_err(|e| format!("cannot set up terminal: {e}"))?;
    let keys = crate::input::KeyReader::spawn();
    let state_path = attach_state_path(&socket);
    let mut state = load_attach_state(&state_path);
    let mut editor = glass_tui::Editor::new();
    let mut inbuf: Vec<u8> = Vec::new();
    let mut out = std::io::stdout();
    let _ = writeln!(
        out,
        "\x1b[90mattached · {} · enter send · ctrl-c cancel · ctrl-d quit\x1b[0m",
        socket.display()
    );
    let mut shown_prompt = false;
    let result = (|| -> Result<(), String> {
        let Some(mut stream) = connect_cancellable(&socket, &keys, &mut out)? else {
            return Ok(());
        };
        send(&mut stream, attach_message(&state));
        loop {
            // Drain the server.
            let mut chunk = [0u8; 8192];
            let mut disconnected = false;
            loop {
                match stream.read(&mut chunk) {
                    Ok(0) => { disconnected = true; break; }
                    Ok(n) => inbuf.extend_from_slice(&chunk[..n]),
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => break,
                    Err(_) => { disconnected = true; break; }
                }
            }
            if disconnected {
                inbuf.clear();
                let Some(reconnected) = connect_cancellable(&socket, &keys, &mut out)? else { return Ok(()); };
                stream = reconnected;
                send(&mut stream, attach_message(&state));
                shown_prompt = false;
                continue;
            }
            while let Some(pos) = inbuf.iter().position(|b| *b == b'\n') {
                let line: Vec<u8> = inbuf.drain(..=pos).collect();
                if let Ok(value) = serde_json::from_slice::<Value>(&line[..line.len() - 1]) {
                    let previous_session = state.session.clone();
                    update_attach_state(&state_path, &mut state, &value)?;
                    if value.get("t").and_then(Value::as_str) == Some("replay")
                        && previous_session.is_some()
                        && previous_session == state.session
                        && let Some(pending) = &state.pending
                    {
                        // Replay confirms this is still the request's session
                        // before any uncertain prompt is resent.
                        send(&mut stream, json!({"t":"prompt", "text":pending.text, "request_id":pending.id}));
                    }
                    shown_prompt = render(&mut out, value, shown_prompt)?;
                }
            }

            // Keys.
            while let Some(event) = keys.try_next() {
                let glass_tui::crossterm::event::Event::Key(key) = event else {
                    continue;
                };
                use glass_tui::crossterm::event::{KeyCode, KeyModifiers};
                let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
                match key.code {
                    KeyCode::Char('d') if ctrl => return Ok(()),
                    KeyCode::Char('c') if ctrl => {
                        send(&mut stream, json!({"t": "cancel"}));
                        write!(out, "^C\r\n").ok();
                        shown_prompt = false;
                    }
                    _ => match editor.key(key) {
                        glass_tui::Edit::Submit(text) => {
                            if state.pending.is_some() {
                                let _ = writeln!(out, "\r\n\x1b[33mwaiting for the prior request to resolve\x1b[0m");
                                continue;
                            }
                            let _ = write!(out, "\x1b[36m❯\x1b[0m {}\r\n", raw(&text));
                            state.request_seq = state.request_seq.saturating_add(1);
                            let pending = PendingRequest {
                                id: format!("{}-{}", std::process::id(), state.request_seq),
                                text,
                            };
                            state.pending = Some(pending.clone());
                            save_attach_state(&state_path, &state)?;
                            send(
                                &mut stream,
                                json!({
                                    "t": "prompt",
                                    "text": pending.text,
                                    "request_id": pending.id,
                                }),
                            );
                            shown_prompt = false;
                        }
                        glass_tui::Edit::Continue => {}
                    },
                }
            }

            // Keep the input line visible; single-row viewport in v1.
            if !shown_prompt {
                let _ = write!(out, "\x1b[2K\r\x1b[90m❯\x1b[0m {}", raw(editor.text()));
                let _ = out.flush();
                shown_prompt = true;
            }
            std::thread::sleep(std::time::Duration::from_millis(30));
        }
    })();

    crate::term::leave().ok();
    result
}

/// Render one server message. Returns whether an input prompt is on screen.
fn render(out: &mut std::io::Stdout, value: Value, shown: bool) -> Result<bool, String> {
    match value.get("t").and_then(Value::as_str) {
        Some("replay") => {
            if value.get("reset").and_then(Value::as_bool) == Some(true) {
                let _ = writeln!(out, "\r\n\x1b[33mconnection history reset · session changed or saved cursor expired\x1b[0m");
            }
            if let Some(entries) = value.get("entries").and_then(Value::as_array) {
                for entry in entries {
                    let role = entry.get("role").and_then(Value::as_str).unwrap_or("");
                    let text = entry.get("text").and_then(Value::as_str).unwrap_or("");
                    match role {
                        "user" => {
                            let _ = write!(out, "\x1b[36m❯\x1b[0m {}\r\n", raw(text));
                        }
                        "assistant" => {
                            let _ = write!(out, "{}\r\n", raw(text));
                        }
                        _ => {
                            let _ = write!(out, "\x1b[90m{}\x1b[0m\r\n", raw(text));
                        }
                    }
                }
            }
        }
        Some("text") => {
            let delta = value.get("delta").and_then(Value::as_str).unwrap_or("");
            let _ = write!(out, "{}", raw(delta));
            let _ = out.flush();
        }
        Some("tool") => {
            let name = value.get("name").and_then(Value::as_str).unwrap_or("tool");
            let command = value.get("command").and_then(Value::as_str).unwrap_or(name);
            let _ = write!(out, "\r\n\x1b[90m▸ {}\x1b[0m\r\n", raw(command));
        }
        Some("tool_end") => {
            let summary = value.get("summary").and_then(Value::as_str).unwrap_or("");
            let _ = write!(out, "\x1b[90m  {}\x1b[0m\r\n", raw(summary));
        }
        Some("done") => {
            let stop = value.get("stop").and_then(Value::as_str).unwrap_or("");
            let _ = write!(out, "\r\n\x1b[90m— {stop} —\x1b[0m\r\n");
        }
        Some("error") => {
            let message = value.get("message").and_then(Value::as_str).unwrap_or("");
            let _ = write!(out, "\r\n\x1b[31m{message}\x1b[0m\r\n");
        }
        Some("duplicate") => {
            let message = value.get("message").and_then(Value::as_str).unwrap_or("request was already accepted");
            let _ = writeln!(out, "\r\n\x1b[33mrequest already handled · {message}\x1b[0m");
        }
        Some("retrying") => {
            let attempt = value.get("attempt").and_then(Value::as_u64).unwrap_or(0);
            let delay = value.get("delay_s").and_then(Value::as_u64).unwrap_or(0);
            let reason = value.get("reason").and_then(Value::as_str).unwrap_or("transient provider failure");
            let _ = writeln!(out, "\r\n\x1b[33mprovider retry {attempt} in {delay}s · {reason} · /retry /status\x1b[0m");
        }
        Some("provider_notice") => {
            let message = value.get("message").and_then(Value::as_str).unwrap_or("fallback active");
            let _ = writeln!(out, "\r\n\x1b[33mprovider fallback · {message} · /status /model\x1b[0m");
        }
        Some("provider_error") => {
            let message = value.get("message").and_then(Value::as_str).unwrap_or("provider stream failed");
            let _ = writeln!(out, "\r\n\x1b[31mprovider error · {message} · recover with /retry, /status or /login\x1b[0m");
        }
        _ => return Ok(shown),
    }
    Ok(false)
}

/// Newlines must be explicit carriage returns inside raw mode.
fn raw(text: &str) -> String {
    text.replace('\r', "").replace('\n', "\r\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replay_is_bounded_and_marks_utf8_previews() {
        let entries = (0..200).map(|n| json!({"role":"tool","text":"界".repeat(40_000),"n":n})).collect();
        let (entries, clipped) = bound_replay(entries);
        assert!(clipped);
        assert!(serde_json::to_vec(&entries).unwrap().len() <= REPLAY_BYTES);
        assert_eq!(entries.last().unwrap()["n"], 199);
        assert!(entries.iter().all(|e| e["truncated"] == true));
        assert!(entries.iter().all(|e| e["text"].as_str().unwrap().len() <= REPLAY_ENTRY_BYTES));
    }


    fn scratch(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("gc-serve-{}-{tag}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn a_cursor_sends_only_later_entries_and_a_stale_one_resets() {
        let dir = scratch("cursor");
        let mut s = Session::create(&dir).unwrap();
        s.append(Record::User("one".into()));
        s.append(Record::Assistant("two".into()));
        s.append(Record::Bash {
            call_id: None,
            command: "true".into(),
            output: "ok".into(),
            exit_code: Some(0),
            truncated: false,
            spill: None,
        });

        let (entries, cursor, reset) = replay_from(&s, None);
        assert_eq!(cursor, 3);
        assert!(!reset, "a first attach missed nothing");
        assert_eq!(entries.len(), 3);

        let (entries, cursor, reset) = replay_from(&s, Some(2));
        assert_eq!(cursor, 3);
        assert!(!reset);
        assert_eq!(entries.len(), 1, "only the record after the cursor");
        assert!(
            entries[0]["text"].as_str().unwrap().contains("true"),
            "the later entry is the bash record"
        );

        let (entries, _, reset) = replay_from(&s, Some(99));
        assert!(reset, "a cursor from another lineage resets");
        assert_eq!(entries.len(), 3, "and gets the full window");

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_cursor_older_than_the_retained_window_is_flagged() {
        let dir = scratch("cursor-floor");
        let mut s = Session::create(&dir).unwrap();
        s.append(Record::User("old".into()));
        for i in 0..REPLAY_ENTRIES {
            s.append(Record::Assistant(format!("answer {i}")));
        }
        let (entries, cursor, reset) = replay_from(&s, Some(0));
        assert!(reset, "the client missed entries and must know");
        assert_eq!(entries.len(), REPLAY_ENTRIES, "the window is retained");
        assert_eq!(cursor, REPLAY_ENTRIES + 1);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn attach_state_survives_restart_with_exact_cursor_and_pending_request_id() {
        let dir = scratch("attach-state");
        let path = dir.join("attach.json");
        let state = AttachState {
            session: Some("session-a".into()),
            cursor: 17,
            request_seq: 9,
            pending: Some(PendingRequest { id: "request-exact-9".into(), text: "do it once".into() }),
        };
        save_attach_state(&path, &state).unwrap();
        let mut restarted = load_attach_state(&path);
        assert_eq!(attach_message(&restarted)["since"], 17);
        assert_eq!(restarted.pending.as_ref().unwrap().id, "request-exact-9");
        update_attach_state(&path, &mut restarted, &json!({"t":"replay", "session":"session-a", "cursor":18})).unwrap();
        assert_eq!(restarted.cursor, 18);
        assert_eq!(restarted.pending.as_ref().unwrap().id, "request-exact-9");
        update_attach_state(&path, &mut restarted, &json!({"t":"duplicate", "request_id":"request-exact-9", "cursor":20})).unwrap();
        assert_eq!(restarted.cursor, 20);
        assert!(restarted.pending.is_none());
        assert_eq!(restarted.request_seq, 9);
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn stop_ack_requires_complete_exact_json_frame_and_eof() {
        let mut pending = b"{\"t\":\"stopped\"}".to_vec();
        let mut oversized = false;
        assert!(
            !consume_stop_frames(&mut pending, &mut oversized),
            "unterminated frame is ignored"
        );
        pending.extend_from_slice(b"\nnoise\n{\"t\":\"not-stopped\"}\n{\"t\":\"stopped\"}");
        assert!(
            consume_stop_frames(&mut pending, &mut oversized),
            "split exact frame is acknowledged"
        );
        assert!(
            !pending.is_empty(),
            "a partial trailing frame remains incomplete"
        );
        assert!(
            !stop_completed(true, false, &[]),
            "timeout is not shutdown completion"
        );
        assert!(
            !stop_completed(true, true, &pending),
            "trailing partial frame is incomplete"
        );
        assert!(stop_completed(true, true, &[]));
    }
}
