//! `glass-core acp`: an Agent Client Protocol server over stdio.
//!
//! ACP (protocol version 1, agentclientprotocol.com) lets editors like Zed
//! drive a coding agent as a subprocess: JSON-RPC 2.0 in both directions,
//! `session/update` notifications for streamed output, `session/cancel` to
//! interrupt a turn. This module speaks the subset glass-core actually has —
//! the headless engine, reused wholesale:
//!
//! - `initialize` / `authenticate`
//! - `session/new` (one glass session per ACP session, bound to the client's
//!   `cwd`)
//! - `session/prompt` (streams `agent_message_chunk`, `agent_thought_chunk`,
//!   `tool_call` / `tool_call_update`, and a final `usage_update`)
//! - `session/cancel` (interrupts the running turn; the prompt response is
//!   the protocol's `cancelled` stop reason)
//!
//! Not advertised, and answered with `-32601`: `session/load`,
//! `logout`, `session/set_mode`, and the client-side `fs/*` / `terminal/*`
//! capabilities — glass-core runs its own filesystem and shell through
//! `bash`, and permission prompting does not exist by design (mods and
//! extensions may still authorize calls via the existing hook chain).
//! Client-supplied MCP servers are accepted and ignored; use the mcp-bridge
//! extension on the host instead.

use std::collections::{BTreeMap, VecDeque};
use std::io::{BufRead, Write};

use serde_json::{Value, json};

use crate::agent::{Agent, Progress, Stop};
use crate::config;
use crate::ext;
use crate::session::{Record, Session};

const PROTOCOL_VERSION: u64 = 1;

fn send(message: Value) {
    let stdout = std::io::stdout();
    let mut out = stdout.lock();
    let _ = writeln!(out, "{message}");
    let _ = out.flush();
}

fn reply(id: &Value, result: Value) {
    send(json!({"jsonrpc": "2.0", "id": id, "result": result}));
}

fn reply_error(id: &Value, code: i64, message: &str) {
    send(json!({"jsonrpc": "2.0", "id": id, "error": {"code": code, "message": message}}));
}

fn notify(session: &str, update: Value) {
    send(json!({"jsonrpc": "2.0", "method": "session/update",
        "params": {"sessionId": session, "update": update}}));
}

/// One ACP conversation.
struct Conversation {
    session: Session,
    cfg: config::Config,
    state: crate::tools::State,
}

/// The text of a prompt's content blocks, with non-text blocks noted.
///
/// Baseline ACP guarantees `text` and `resource_link`; `resource` with inline
/// text is folded in as text so editors that attach files inline still work.
fn prompt_text(blocks: &[Value]) -> Result<String, String> {
    let mut parts = Vec::new();
    for block in blocks {
        match block.get("type").and_then(Value::as_str) {
            Some("text") => {
                if let Some(text) = block.get("text").and_then(Value::as_str) {
                    parts.push(text.to_string());
                }
            }
            Some("resource_link") => {
                if let Some(uri) = block.get("uri").and_then(Value::as_str) {
                    parts.push(format!("[resource: {uri}]"));
                }
            }
            Some("resource") => {
                let resource = block.get("resource").cloned().unwrap_or(Value::Null);
                if let Some(text) = resource.get("text").and_then(Value::as_str) {
                    let uri = resource
                        .get("uri")
                        .and_then(Value::as_str)
                        .unwrap_or("inline");
                    parts.push(format!("[resource: {uri}]\n{text}"));
                } else if let Some(uri) = resource.get("uri").and_then(Value::as_str) {
                    parts.push(format!("[resource: {uri} (binary, not inlined)]"));
                }
            }
            Some(other) => parts.push(format!("[{other} content omitted]")),
            None => return Err("prompt block is missing its `type`".into()),
        }
    }
    let text = parts.join("\n").trim().to_string();
    if text.is_empty() {
        return Err("prompt contains no usable content".into());
    }
    Ok(text)
}

fn stop_reason(stop: &Stop) -> &'static str {
    match stop {
        Stop::Finished | Stop::Steered => "end_turn",
        Stop::Cancelled => "cancelled",
        Stop::ToolBudget => "max_turn_requests",
    }
}

pub fn serve(cfg: config::Config) -> Result<(), String> {
    // The reader runs on its own thread so `session/cancel` can arrive while a
    // turn is streaming; the protocol is full-duplex even though glass's turn
    // loop is single-threaded.
    let (tx, rx) = std::sync::mpsc::channel::<Result<Value, ()>>();
    std::thread::spawn(move || {
        let stdin = std::io::stdin();
        for line in stdin.lock().lines() {
            let Ok(line) = line else { break };
            if line.trim().is_empty() {
                continue;
            }
            let parsed = serde_json::from_str::<Value>(&line).map_err(|_| ());
            if tx.send(parsed).is_err() {
                break;
            }
        }
    });

    let mods = crate::mods::load(&cfg.mods)?;
    let mut extensions = ext::Registry::load(&cfg.extensions, &cfg.workspace, &cfg.model);
    let skills = cfg
        .skills_dir
        .as_deref()
        .map(crate::skills::load_dir)
        .unwrap_or_default();
    let catalogue = crate::skills::catalogue(&skills);

    let mut conversations: BTreeMap<String, Conversation> = BTreeMap::new();
    let mut initialized = false;
    let mut pending: VecDeque<Value> = VecDeque::new();

    let result = loop {
        let message = match pending.pop_front() {
            Some(message) => Ok(message),
            None => match rx.recv() {
                Ok(message) => message,
                Err(_) => break Ok(()),
            },
        };
        let message = match message {
            Ok(m) => m,
            Err(()) => {
                reply_error(&Value::Null, -32700, "parse error: not a JSON object");
                continue;
            }
        };
        let id = message.get("id").cloned().unwrap_or(Value::Null);
        let method = message.get("method").and_then(Value::as_str).unwrap_or("");
        let params = message.get("params").cloned().unwrap_or(json!({}));
        let is_request = message.get("id").is_some();

        match method {
            "initialize" => {
                initialized = true;
                reply(
                    &id,
                    json!({
                        "protocolVersion": PROTOCOL_VERSION,
                        "agentCapabilities": {
                            "loadSession": false,
                            "promptCapabilities": {
                                "image": false, "audio": false, "embeddedContext": true
                            }
                        },
                        "agentInfo": {
                            "name": "glass-core",
                            "title": "glass-core",
                            "version": env!("CARGO_PKG_VERSION")
                        },
                        "authMethods": []
                    }),
                );
            }
            "authenticate" => match crate::auth::resolve(&cfg) {
                Ok(_) => reply(&id, json!({})),
                Err(e) => reply_error(
                    &id,
                    -32000,
                    &format!("no usable credentials on the host: {e}; run /login there"),
                ),
            },
            "session/new" => {
                if !initialized {
                    reply_error(&id, -32000, "initialize must be called first");
                    continue;
                }
                let cwd = params
                    .get("cwd")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string();
                if cwd.is_empty() || !std::path::Path::new(&cwd).is_dir() {
                    reply_error(&id, -32602, "cwd must be an existing absolute directory");
                    continue;
                }
                let mut session_cfg = cfg.clone();
                session_cfg.workspace = std::path::PathBuf::from(&cwd);
                let session = match Session::create(&config::data_dir()) {
                    Ok(s) => s,
                    Err(e) => {
                        reply_error(&id, -32603, &format!("cannot create session: {e}"));
                        continue;
                    }
                };
                if let Err(e) = crate::polish::bind(&session, &session_cfg.workspace) {
                    reply_error(&id, -32603, &format!("cannot bind session: {e}"));
                    continue;
                }
                let id_text = session.id.clone();
                conversations.insert(
                    id_text.clone(),
                    Conversation {
                        session,
                        cfg: session_cfg,
                        state: crate::tools::State::new(),
                    },
                );
                reply(&id, json!({"sessionId": id_text}));
            }
            "session/prompt" => {
                let Some(session_id) = params.get("sessionId").and_then(Value::as_str) else {
                    reply_error(&id, -32602, "sessionId is required");
                    continue;
                };
                let Some(conversation) = conversations.get_mut(session_id) else {
                    reply_error(&id, -32602, "unknown sessionId");
                    continue;
                };
                let blocks = params
                    .get("prompt")
                    .and_then(Value::as_array)
                    .cloned()
                    .unwrap_or_default();
                let text = match prompt_text(&blocks) {
                    Ok(text) => text,
                    Err(e) => {
                        reply_error(&id, -32602, &e);
                        continue;
                    }
                };
                conversation.session.append(Record::User(text));

                let ledger_path = conversation.session.path.with_extension("cost.json");
                let ledger = crate::budget::load(&ledger_path, conversation.cfg.cost_limit);
                let ledger_path = std::sync::Arc::new(std::sync::Mutex::new(ledger_path));
                let factory: crate::warmer::ProviderFactory = {
                    let configured = conversation.cfg.clone();
                    let ledger = std::sync::Arc::clone(&ledger);
                    let path = std::sync::Arc::clone(&ledger_path);
                    std::sync::Arc::new(move || {
                        crate::make_provider(&configured, std::sync::Arc::clone(&ledger), std::sync::Arc::clone(&path), None)
                    })
                };
                let provider = match factory() {
                    Ok(p) => p,
                    Err(e) => {
                        reply_error(&id, -32000, &e);
                        continue;
                    }
                };
                let instructions = crate::trust::gated(
                    &conversation.cfg,
                    &crate::agents_md::load(&conversation.cfg.workspace),
                    false,
                );
                // Synthesized tool-call ids: Progress carries the command, not
                // the provider's call id, so starts and ends pair FIFO. The
                // sink is `Fn`, hence the interior mutability.
                let next_call = std::cell::Cell::new(0u64);
                let open_tools = std::cell::RefCell::new(Vec::<String>::new());
                let session_id_owned = session_id.to_string();
                let emit = |update: Value| notify(&session_id_owned, update);
                let agent_result = {
                    let mut agent = Agent::new(
                        provider.as_ref(),
                        &mut conversation.session,
                        &mut conversation.state,
                        &mods,
                        &mut extensions,
                        &conversation.cfg.workspace,
                    )
                    .prime_extensions()
                    .with_prompt(
                        conversation.cfg.system_prompt.clone(),
                        catalogue.clone(),
                        instructions,
                    )
                    .with_live(conversation.cfg.live_flush_ms as u64)
                    .with_stream_factory(std::sync::Arc::clone(&factory))
                    .with_progress(|progress| match progress {
                        Progress::Text(t) => emit(json!({
                            "sessionUpdate": "agent_message_chunk",
                            "content": {"type": "text", "text": t}
                        })),
                        Progress::Reasoning(t) => emit(json!({
                            "sessionUpdate": "agent_thought_chunk",
                            "content": {"type": "text", "text": t}
                        })),
                        Progress::ToolStart { name, command } => {
                            let call_id = format!("gc-{}", next_call.get() + 1);
                            next_call.set(next_call.get() + 1);
                            open_tools.borrow_mut().push(call_id.clone());
                            emit(json!({
                                "sessionUpdate": "tool_call",
                                "toolCallId": call_id,
                                "title": command.unwrap_or(name),
                                "kind": if name == "bash" { "execute" } else { "other" },
                                "status": "in_progress"
                            }));
                        }
                        Progress::ToolEnd { summary, output } => {
                            let call_id = {
                                let mut open = open_tools.borrow_mut();
                                if open.is_empty() {
                                    "gc-unknown".to_string()
                                } else {
                                    open.remove(0)
                                }
                            };
                            let text = if output.trim().is_empty() {
                                summary
                            } else {
                                output.to_string()
                            };
                            emit(json!({
                                "sessionUpdate": "tool_call_update",
                                "toolCallId": call_id,
                                "status": "completed",
                                "content": [{"type": "content",
                                    "content": {"type": "text", "text": text}}]
                            }));
                        }
                        Progress::Retrying { reason, .. } => emit(json!({
                            "sessionUpdate": "agent_thought_chunk",
                            "content": {"type": "text", "text": format!("retrying: {reason}")}
                        })),
                        _ => {}
                    });
                    let mut busy: Vec<Value> = Vec::new();
                    let outcome = agent.run_turn(|slot| {
                        while let Ok(message) = rx.try_recv() {
                            match message {
                                Ok(m)
                                    if m.get("method").and_then(Value::as_str)
                                        == Some("session/cancel") =>
                                {
                                    // The empty steer is the existing cancel
                                    // path: stop before the next command and
                                    // kill owned process groups.
                                    *slot = Some(String::new());
                                }
                                Ok(m) => busy.push(m),
                                Err(()) => {}
                            }
                        }
                    });
                    pending.extend(busy);
                    outcome
                };
                match agent_result {
                    Ok((out, stop)) => {
                        if let Some(usage) = out.usage {
                            let used = usage.prompt_total() as u64;
                            let size = provider.context_window() as u64;
                            let spent = ledger.lock().map(|l| (l.spent, l.unknown)).unwrap_or((0.0, true));
                            let update = if !spent.1 && spent.0 > 0.0 {
                                json!({"sessionUpdate": "usage_update", "used": used,
                                    "size": size, "cost": {"amount": spent.0, "currency": "USD"}})
                            } else {
                                json!({"sessionUpdate": "usage_update", "used": used, "size": size})
                            };
                            emit(update);
                        }
                        reply(&id, json!({"stopReason": stop_reason(&stop)}));
                    }
                    Err(e) => reply_error(&id, -32603, &format!("turn failed: {e}")),
                }
            }
            "session/cancel" => {
                // A cancel that arrives outside a turn is a no-op notification.
                if !is_request {
                    continue;
                }
                reply(&id, json!({}));
            }
            other if !is_request => {
                let _ = other; // unknown notification: ignore, per JSON-RPC
            }
            other => {
                reply_error(&id, -32601, &format!("method not found: {other}"));
            }
        }
    };

    extensions.shutdown();
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prompt_text_folds_text_links_and_inline_resources() {
        let blocks = vec![
            json!({"type": "text", "text": "fix the parser"}),
            json!({"type": "resource_link", "uri": "file:///w/src/main.rs"}),
            json!({"type": "resource", "resource": {"uri": "file:///w/a.txt", "text": "A"}}),
        ];
        let text = prompt_text(&blocks).unwrap();
        assert!(text.contains("fix the parser"));
        assert!(text.contains("file:///w/src/main.rs"));
        assert!(text.contains("[resource: file:///w/a.txt]\nA"));
    }

    #[test]
    fn prompt_text_notes_unsupported_blocks_and_rejects_empty_ones() {
        let text = prompt_text(&[json!({"type": "image"})]).unwrap();
        assert!(text.contains("image content omitted"));
        assert!(prompt_text(&[]).is_err());
        assert!(prompt_text(&[json!({"type": "text", "text": "  "})]).is_err());
        assert!(prompt_text(&[json!({"nope": 1})]).is_err());
    }

    #[test]
    fn stop_reasons_map_to_the_protocol() {
        assert_eq!(stop_reason(&Stop::Finished), "end_turn");
        assert_eq!(stop_reason(&Stop::Steered), "end_turn");
        assert_eq!(stop_reason(&Stop::Cancelled), "cancelled");
        assert_eq!(stop_reason(&Stop::ToolBudget), "max_turn_requests");
    }
}
