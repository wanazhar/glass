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
//! v1 limits, documented in the README: one attached client at a time, no
//! remote modals (slash commands stay local to the interactive session), and
//! loopback-only by construction (a Unix socket).

use std::collections::VecDeque;
use std::io::{Read, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::PathBuf;

use serde_json::{Value, json};

use crate::agent::{Agent, Progress, Stop};
use crate::config;
use crate::ext;
use crate::session::{Record, Session};

const REPLAY_ENTRIES: usize = 200;

fn send(stream: &mut UnixStream, value: Value) {
    if let Ok(mut line) = serde_json::to_string(&value) {
        line.push('\n');
        let _ = stream.write_all(line.as_bytes());
        let _ = stream.flush();
    }
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

fn replay_entries(session: &Session) -> Vec<Value> {
    let records = crate::export::raw_records(session);
    let start = records.len().saturating_sub(REPLAY_ENTRIES);
    records[start..]
        .iter()
        .filter_map(|record| match record {
            Record::User(text) => Some(json!({"role": "user", "text": text})),
            Record::Assistant(text) => Some(json!({"role": "assistant", "text": text})),
            Record::Bash { command, output, .. } => {
                Some(json!({"role": "tool", "text": format!("$ {command}\n{output}")}))
            }
            _ => None,
        })
        .collect()
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
    let instructions = crate::trust::gated(
        &cfg,
        &crate::agents_md::load(&cfg.workspace),
        false,
    );

    let client = std::rc::Rc::new(std::cell::RefCell::new(None::<Client>));
    let mut pending: VecDeque<Value> = VecDeque::new();
    let failure;
    listener.set_nonblocking(true).map_err(|e| e.to_string())?;

    'serve: loop {
        if crate::signals::interrupted() {
            failure = None;
            break 'serve;
        }
        // A new connection replaces the previous client, attached or not:
        // reattaching from another terminal is the whole point of the socket.
        match listener.accept() {
            Ok((stream, _)) => {
                let _ = stream.set_nonblocking(true);
                *client.borrow_mut() = Some(Client {
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
        if client.borrow().is_none() {
            std::thread::sleep(std::time::Duration::from_millis(30));
            continue;
        }

        let message = match pending.pop_front() {
            Some(message) => Some(message),
            None => {
                let incoming = client
                    .borrow_mut()
                    .as_mut()
                    .map(Client::try_incoming)
                    .unwrap_or(Incoming::Idle);
                match incoming {
                    Incoming::Message(message) => Some(message),
                    Incoming::Idle => {
                        std::thread::sleep(std::time::Duration::from_millis(30));
                        continue;
                    }
                    Incoming::Malformed => continue,
                    Incoming::Closed => {
                        *client.borrow_mut() = None;
                        continue;
                    }
                }
            }
        };
        let Some(message) = message else { continue };

        match message.get("t").and_then(Value::as_str) {
            Some("attach") => {
                let entries = replay_entries(&session);
                if let Some(c) = client.borrow_mut().as_mut() {
                    send(
                        &mut c.stream,
                        json!({"t": "replay", "session": session.id, "entries": entries}),
                    );
                }
            }
            Some("prompt") => {
                let text = message
                    .get("text")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .trim()
                    .to_string();
                if text.is_empty() {
                    if let Some(c) = client.borrow_mut().as_mut() {
                        send(&mut c.stream, json!({"t": "error", "message": "empty prompt"}));
                    }
                    continue;
                }
                session.append(Record::User(text));
                let provider = match factory() {
                    Ok(p) => p,
                    Err(e) => {
                        if let Some(c) = client.borrow_mut().as_mut() {
                            send(&mut c.stream, json!({"t": "error", "message": e}));
                        }
                        continue;
                    }
                };
                let outer = std::rc::Rc::clone(&client);
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
                    .with_stream_factory(std::sync::Arc::clone(&factory))
                    .with_progress(move |progress| {
                        let mut slot = outer.borrow_mut();
                        let Some(c) = slot.as_mut() else { return };
                        match progress {
                            Progress::Text(t) => {
                                send(&mut c.stream, json!({"t": "text", "delta": t}))
                            }
                            Progress::ToolStart { name, command } => send(
                                &mut c.stream,
                                json!({"t": "tool", "name": name, "command": command}),
                            ),
                            Progress::ToolEnd { summary, .. } => send(
                                &mut c.stream,
                                json!({"t": "tool_end", "summary": summary}),
                            ),
                            _ => {}
                        }
                    });
                    let outer = std::rc::Rc::clone(&client);
                    agent.run_turn(|steer| {
                        if crate::signals::interrupted() {
                            *steer = Some(String::new());
                        }
                        let mut slot = outer.borrow_mut();
                        let Some(c) = slot.as_mut() else { return };
                        loop {
                            match c.try_incoming() {
                                Incoming::Message(message) => match message
                                    .get("t")
                                    .and_then(Value::as_str)
                                {
                                    Some("cancel") => *steer = Some(String::new()),
                                    _ => queued.push(message),
                                },
                                Incoming::Malformed | Incoming::Idle => break,
                                Incoming::Closed => {
                                    *slot = None;
                                    break;
                                }
                            }
                        }
                    })
                };
                pending.extend(queued);
                let (stop, text) = match outcome {
                    Ok((out, stop)) => (stop, out.text),
                    Err(e) => {
                        if let Some(c) = client.borrow_mut().as_mut() {
                            send(
                                &mut c.stream,
                                json!({"t": "error", "message": format!("turn failed: {e}")}),
                            );
                        }
                        continue;
                    }
                };
                let stop = match stop {
                    Stop::Finished => "finished",
                    Stop::Cancelled => "cancelled",
                    Stop::Steered => "steered",
                    Stop::ToolBudget => "tool_budget",
                };
                if let Some(c) = client.borrow_mut().as_mut() {
                    send(&mut c.stream, json!({"t": "done", "stop": stop, "text": text}));
                }
            }
            Some("cancel") => { /* no turn running: nothing to cancel */ }
            other => {
                if let Some(c) = client.borrow_mut().as_mut() {
                    send(
                        &mut c.stream,
                        json!({"t": "error", "message": format!("unknown message {:?}", other)}),
                    );
                }
            }
        }
    }

    extensions.shutdown();
    let _ = std::fs::remove_file(&socket_path);
    failure.map_or(Ok(()), Err)
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
                            send(&mut stream, json!({"t": "prompt", "text": text}));
                            shown_prompt = false;
                        }
                        glass_tui::Edit::Continue => {}
                    },
                }
            }

            // Keep the input line visible; single-row viewport in v1.
            if !shown_prompt {
                let _ = write!(
                    out,
                    "\x1b[2K\r\x1b[90m❯\x1b[0m {}",
                    raw(editor.text())
                );
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
