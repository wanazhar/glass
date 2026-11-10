//! Minimal local HTTP surface for the Glass marble product UI.

use crate::conversation::ApprovalInbox;
use crate::state::{ConversationRecord, ProductState, TaskRecord, TranscriptEntry};
use crate::turn::MarbleTurn;
use crate::{Approval, InboundMessage, MarbleError, MarbleReply, to_turn_input};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::sync::{Arc, Mutex, mpsc};
use std::time::Duration;

pub const STATIC_HTML: &str = include_str!("static/index.html");

/// One queued durable turn awaiting a runtime reply.
pub struct TurnCommand {
    pub reply: mpsc::Sender<Result<MarbleReply, MarbleError>>,
    pub turn: MarbleTurn,
}

/// Local server owning product state and explicit host decisions.
pub struct MarbleServer {
    listener: TcpListener,
    state: Arc<Mutex<ProductState>>,
    approvals: ApprovalInbox,
    turn_tx: mpsc::Sender<TurnCommand>,
}

impl MarbleServer {
    pub fn bind(
        listener: TcpListener,
        state: Arc<Mutex<ProductState>>,
        approvals: ApprovalInbox,
        turn_tx: mpsc::Sender<TurnCommand>,
    ) -> Self {
        Self {
            listener,
            state,
            approvals,
            turn_tx,
        }
    }

    pub fn local_addr(&self) -> SocketAddr {
        self.listener
            .local_addr()
            .expect("glass-marble listener bound to an ephemeral local address")
    }

    pub fn state(&self) -> ProductState {
        self.state
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .clone()
    }

    pub fn handle(&self, method: &str, target: &str, body: &str) -> (u16, String) {
        let approvals = self.approvals.clone();
        let turn_tx = self.turn_tx.clone();
        Self::route(&self.state, &approvals, turn_tx, method, target, body)
    }

    fn route(
        state: &Arc<Mutex<ProductState>>,
        approvals: &ApprovalInbox,
        turn_tx: mpsc::Sender<TurnCommand>,
        method: &str,
        target: &str,
        body: &str,
    ) -> (u16, String) {
        let path = target.split('?').next().unwrap_or(target);
        if method == "GET" && (path == "/" || path == "/index.html") {
            return (200, STATIC_HTML.to_string());
        }
        if method == "GET" && path == "/api/state" {
            let mut state = state
                .lock()
                .map(|state| state.clone())
                .unwrap_or_else(|_| ProductState::default());
            state.approvals = approvals.pending();
            let encoded = serde_json::to_string_pretty(&state).unwrap_or_else(|_| "{}".to_string());
            return (200, encoded);
        }
        if method == "POST" && path == "/api/turns" {
            return Self::handle_turn(turn_tx, state, body);
        }
        if method == "POST"
            && let Some(rest) = path.strip_prefix("/api/approvals/")
            && let Some((request_id, action)) = rest.split_once('/')
        {
            let approval = if action == "allow" {
                Approval::Approve
            } else if action == "deny" {
                Approval::Deny
            } else {
                return (
                    400,
                    serde_json::to_string(&serde_json::json!({
                        "error": "unknown_action",
                        "action": action,
                    }))
                    .unwrap_or_default(),
                );
            };
            let resolved = approvals.resolve(request_id, approval);
            let status = if resolved { 200 } else { 409 };
            return (
                status,
                serde_json::to_string(&serde_json::json!({ "ok": resolved })).unwrap_or_default(),
            );
        }
        (
            404,
            serde_json::to_string(&serde_json::json!({ "error": "not_found" })).unwrap_or_default(),
        )
    }

    fn handle_turn(
        turn_tx: mpsc::Sender<TurnCommand>,
        state: &Arc<Mutex<ProductState>>,
        body: &str,
    ) -> (u16, String) {
        let parsed: serde_json::Value = match serde_json::from_str(body) {
            Ok(value) => value,
            Err(_) => {
                return (
                    400,
                    serde_json::to_string(&serde_json::json!({ "error": "invalid_json" }))
                        .unwrap_or_default(),
                );
            }
        };
        let text = parsed
            .get("text")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default()
            .to_string();
        let conversation_id = parsed
            .get("conversationId")
            .and_then(serde_json::Value::as_str)
            .map(str::to_owned);
        let turn = MarbleTurn {
            conversation_id,
            request_id: format!("marble-{}", std::process::id()),
            input: to_turn_input(&InboundMessage::new(&text)),
        };
        let (reply_tx, reply_rx) = mpsc::channel();
        if turn_tx
            .send(TurnCommand {
                reply: reply_tx,
                turn,
            })
            .is_err()
        {
            return (
                503,
                serde_json::to_string(&serde_json::json!({ "error": "turn_rejected" }))
                    .unwrap_or_default(),
            );
        }
        let reply = reply_rx
            .recv_timeout(Duration::from_secs(crate::TURN_TIMEOUT.as_secs()))
            .ok()
            .and_then(|result| result.ok());
        let Some(reply) = reply else {
            return (
                504,
                serde_json::to_string(&serde_json::json!({ "error": "turn_timeout" }))
                    .unwrap_or_default(),
            );
        };
        let mut updated = state
            .lock()
            .map(|state| state.clone())
            .unwrap_or_else(|_| ProductState::default());
        updated.transcript.push(TranscriptEntry {
            conversation_id: reply.conversation_id.clone(),
            kind: "user".to_string(),
            text: text.clone(),
        });
        updated.transcript.push(TranscriptEntry {
            conversation_id: reply.conversation_id.clone(),
            kind: "assistant".to_string(),
            text: reply.text.clone(),
        });
        if let Some(conversation) = updated
            .conversations
            .iter_mut()
            .find(|conversation| conversation.id == reply.conversation_id)
        {
            if !conversation.task_ids.contains(&reply.task_id) {
                conversation.task_ids.push(reply.task_id.clone());
            }
        } else {
            updated.conversations.push(ConversationRecord {
                id: reply.conversation_id.clone(),
                task_ids: vec![reply.task_id.clone()],
            });
        }
        updated.tasks.retain(|task| task.id != reply.task_id);
        updated.tasks.push(TaskRecord {
            id: reply.task_id.clone(),
            conversation_id: reply.conversation_id.clone(),
            status: "completed".to_string(),
        });
        if let Ok(mut guard) = state.lock() {
            *guard = updated;
        }
        let body = serde_json::to_string(&serde_json::json!({
            "reply": {
                "conversationId": reply.conversation_id,
                "taskId": reply.task_id,
                "text": reply.text,
            }
        }))
        .unwrap_or_default();
        (200, body)
    }

    pub fn serve(self) -> std::io::Result<()> {
        loop {
            let next = self.listener.incoming().next();
            let stream = match next {
                Some(Ok(stream)) => stream,
                Some(Err(error)) => return Err(error),
                None => break,
            };
            let state = self.state.clone();
            let approvals = self.approvals.clone();
            let turn_tx = self.turn_tx.clone();
            std::thread::spawn(move || {
                let _ = Self::serve_connection(stream, state, approvals, turn_tx);
            });
        }
        Ok(())
    }

    fn serve_connection(
        stream: TcpStream,
        state: Arc<Mutex<ProductState>>,
        approvals: ApprovalInbox,
        turn_tx: mpsc::Sender<TurnCommand>,
    ) -> std::io::Result<()> {
        let mut stream = stream;
        let mut reader = BufReader::new(&mut stream);
        let head = read_head(&mut reader)?;
        let head_text = String::from_utf8_lossy(&head).to_string();
        let mut content_length = 0usize;
        for line in head_text.split("\r\n") {
            if let Some((key, value)) = line.split_once(':')
                && key.trim().eq_ignore_ascii_case("content-length")
            {
                content_length = value.trim().parse().unwrap_or(0);
            }
        }
        let mut body = vec![0u8; content_length];
        reader.read_exact(&mut body)?;
        let body = String::from_utf8_lossy(&body).to_string();
        let mut parts = head_text.splitn(3, ' ');
        let method = parts.next().unwrap_or_default().to_string();
        let target = parts.next().unwrap_or_default().to_string();
        let _version = parts.next();
        let (status, response_body) =
            Self::route(&state, &approvals, turn_tx, &method, &target, &body);
        let path = target.split('?').next().unwrap_or_default().to_string();
        let content_type = if method == "GET" && (path == "/" || path == "/index.html") {
            "text/html; charset=utf-8"
        } else {
            "application/json; charset=utf-8"
        };
        let reason = match status {
            200 => "OK",
            400 => "Bad Request",
            404 => "Not Found",
            409 => "Conflict",
            503 => "Service Unavailable",
            504 => "Gateway Timeout",
            _ => "OK",
        };
        let response = format!(
            "HTTP/1.1 {status} {reason}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{response_body}",
            response_body.len()
        );
        stream.write_all(response.as_bytes())?;
        stream.flush()?;
        Ok(())
    }
}

fn read_head(reader: &mut impl BufRead) -> std::io::Result<Vec<u8>> {
    let mut head = Vec::new();
    let mut byte = [0u8; 1];
    loop {
        let read = reader.read(&mut byte)?;
        if read == 0 {
            break;
        }
        head.push(byte[0]);
        if head.ends_with(b"\r\n\r\n") {
            break;
        }
    }
    Ok(head)
}
