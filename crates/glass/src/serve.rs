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

use serde_json::{Value, json};

use crate::agent::{Agent, Progress, Stop};
use crate::config;
use crate::ext;
use crate::session::{Record, Session};

const REPLAY_ENTRIES: usize = 200;
/// Concurrent attaches per served session.
const MAX_CLIENTS: usize = 4;

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
    stream: UnixStream,
    buf: Vec<u8>,
}

impl Client {
    fn try_incoming(&mut self) -> Incoming {
        let mut chunk = [0u8; 4096];
        loop {
            match self.stream.read(&mut chunk) {
                Ok(0) => return Incoming::Closed,
                Ok(n) => self.buf.extend_from_slice(&chunk[..n]),
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
    (entries_between(&records, start, cursor), cursor, reset)
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
    let _ = std::fs::remove_file(&socket_path);
    let listener = UnixListener::bind(&socket_path)
        .map_err(|e| format!("cannot bind {}: {e}", socket_path.display()))?;
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&socket_path, std::fs::Permissions::from_mode(0o600))
            .map_err(|e| e.to_string())?;
    }
    println!("glass-core: serving session {}", session.id);
    println!("glass-core: socket {}", socket_path.display());
    println!(
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
        let path = std::sync::Arc::new(std::sync::Mutex::new(ledger_path));
        std::sync::Arc::new(move || {
            crate::make_provider(
                &configured,
                std::sync::Arc::clone(&ledger),
                std::sync::Arc::clone(&path),
                None,
            )
        })
    };
    let instructions = crate::trust::gated(&cfg, &crate::agents_md::load(&cfg.workspace), false);

    let clients = std::rc::Rc::new(std::cell::RefCell::new(Vec::<Client>::new()));
    let mut mailbox = crate::inbox::Mailbox::open(&session.id);
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
                    slots.remove(0);
                }
                slots.push(Client {
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
                        if let Some((from, text)) = mailbox.next() {
                            broadcast(
                                &clients,
                                json!({"t": "text", "delta": format!("\n\x1b[90m[inbox from {from}]\x1b[0m {text}\n")}),
                            );
                            session.append(Record::System(format!(
                                "[inbox] message from session {from}"
                            )));
                            Some((None, json!({"t": "prompt", "text": text})))
                        } else if let Some(prompt) = crate::loops::due(&session) {
                            broadcast(
                                &clients,
                                json!({"t": "text", "delta": format!("\n\x1b[90m[loop]\x1b[0m {prompt}\n")}),
                            );
                            Some((None, json!({"t": "prompt", "text": prompt})))
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

        match message.get("t").and_then(Value::as_str) {
            Some("attach") => {
                let since = message
                    .get("since")
                    .and_then(Value::as_u64)
                    .map(|n| n as usize);
                let (entries, cursor, reset) = replay_from(&session, since);
                to_origin(json!({
                    "t": "replay",
                    "session": session.id,
                    "entries": entries,
                    "cursor": cursor,
                    "reset": reset,
                }));
            }
            Some("prompt") => {
                let text = message
                    .get("text")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .trim()
                    .to_string();
                if text.is_empty() {
                    to_origin(json!({"t": "error", "message": "empty prompt"}));
                    continue;
                }
                if let Some(id) = message.get("request_id").and_then(Value::as_str) {
                    if let Some(status) = seen_requests.get(id) {
                        to_origin(json!({"t": "duplicate", "request_id": id, "status": status,
                            "message": if status == "accepted" { "prior request was interrupted; it will not be replayed" } else { "request already completed" }}));
                        continue;
                    }
                    if let Err(error) = session.append_checked(&[Record::ServeRequest {
                        id: id.to_string(),
                        status: "accepted".into(),
                    }]) {
                        to_origin(
                            json!({"t":"error", "message": format!("cannot durably accept request; prompt was not run: {error}")}),
                        );
                        continue;
                    }
                    seen_requests.insert(id.to_string(), "accepted".into());
                }
                if let Err(error) = session.append_checked(&[Record::User(text)]) {
                    to_origin(json!({"t":"error", "message": format!("cannot durably record prompt; it was not run: {error}")}));
                    continue;
                }
                let provider = match factory() {
                    Ok(p) => p,
                    Err(e) => {
                        to_origin(json!({"t": "error", "message": e}));
                        continue;
                    }
                };
                let outer = std::rc::Rc::clone(&clients);
                let mut queued: Vec<Value> = Vec::new();
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
                        Progress::ToolStart { name, command } => broadcast(
                            &outer,
                            json!({"t": "tool", "name": name, "command": command}),
                        ),
                        Progress::ToolEnd { summary, .. } => {
                            broadcast(&outer, json!({"t": "tool_end", "summary": summary}))
                        }
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
                                        match message.get("t").and_then(Value::as_str) {
                                            Some("cancel") => *steer = Some(String::new()),
                                            _ => queued.push(message),
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
                let (stop, text) = match outcome {
                    Ok((out, stop)) => (stop, out.text),
                    Err(e) => {
                        broadcast(
                            &clients,
                            json!({"t": "error", "message": format!("turn failed: {e}")}),
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
                broadcast(&clients, json!({"t": "done", "stop": stop, "text": text}));
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
        println!("glass-core: stopped session {id}");
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
        Some(id) => println!("glass-core: respawning session {id}"),
        None => println!("glass-core: respawning the newest session"),
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
        println!("no activity in session {id}");
        return Ok(());
    }
    println!(
        "glass-core: last {} entries of session {id}\n",
        entries.len()
    );
    for entry in entries {
        println!("{entry}\n");
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
    let mut stream = UnixStream::connect(&socket)
        .map_err(|e| format!("cannot connect to {}: {e}", socket.display()))?;
    stream.set_nonblocking(true).map_err(|e| e.to_string())?;
    send(&mut stream, json!({"t": "attach"}));

    crate::term::enter().map_err(|e| format!("cannot set up terminal: {e}"))?;
    let keys = crate::input::KeyReader::spawn();
    let mut editor = glass_tui::Editor::new();
    let mut inbuf: Vec<u8> = Vec::new();
    let mut out = std::io::stdout();
    let _ = writeln!(
        out,
        "\x1b[90mattached · {} · enter send · ctrl-c cancel · ctrl-d quit\x1b[0m",
        socket.display()
    );
    let mut shown_prompt = false;
    // Every submission carries a request id so a retry over a flaky link
    // cannot run the same prompt twice.
    let mut request_seq: u64 = 0;

    let result = (|| -> Result<(), String> {
        loop {
            // Drain the server.
            let mut chunk = [0u8; 8192];
            loop {
                match stream.read(&mut chunk) {
                    Ok(0) => return Err("server closed the connection".into()),
                    Ok(n) => inbuf.extend_from_slice(&chunk[..n]),
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => break,
                    Err(e) => return Err(e.to_string()),
                }
            }
            while let Some(pos) = inbuf.iter().position(|b| *b == b'\n') {
                let line: Vec<u8> = inbuf.drain(..=pos).collect();
                if let Ok(value) = serde_json::from_slice::<Value>(&line[..line.len() - 1]) {
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
                            let _ = write!(out, "\x1b[36m❯\x1b[0m {}\r\n", raw(&text));
                            request_seq += 1;
                            send(
                                &mut stream,
                                json!({
                                    "t": "prompt",
                                    "text": text,
                                    "request_id": format!("{}-{request_seq}", std::process::id()),
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
