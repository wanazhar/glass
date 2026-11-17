//! Self-hosted HTTP surface for the Glass marble product.
//!
//! Bound to loopback by default; a Tailscale address needs an explicit host and
//! an access token. The surface is intentionally small: state, a turn intake,
//! cancellation, approvals, and one server-sent event stream.

use crate::Approval;
use crate::control::ControlProfile;
use crate::conversation::ApprovalInbox;
use crate::events::{EventBus, TurnEvent};
use crate::memory::{self, MemoryCommand};
use crate::notify::{self, ApnsSettings, Notification};
use crate::state::{PendingApproval, ProductState};
use crate::store::StateStore;
use crate::voice;
use crate::worker::{TurnCommand, TurnRequest, now_millis};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use std::time::Duration;

pub const STATIC_HTML: &str = include_str!("static/index.html");
/// Largest accepted JSON request body.
pub const MAX_BODY_BYTES: usize = 256 * 1024;
/// Largest accepted request head.
const MAX_HEAD_BYTES: usize = 16 * 1024;
const SESSION_COOKIE: &str = "marble_token";
const ACCEPT_TIMEOUT: Duration = Duration::from_secs(25);
const STREAM_KEEPALIVE: Duration = Duration::from_secs(15);

/// One HTTP response returned by the router.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Response {
    pub status: u16,
    pub body: String,
    pub headers: Vec<(String, String)>,
}

impl Response {
    pub fn json(status: u16, value: Value) -> Self {
        Self {
            status,
            body: value.to_string(),
            headers: vec![(
                "Content-Type".into(),
                "application/json; charset=utf-8".into(),
            )],
        }
    }

    pub fn html(body: impl Into<String>) -> Self {
        Self {
            status: 200,
            body: body.into(),
            headers: vec![("Content-Type".into(), "text/html; charset=utf-8".into())],
        }
    }

    pub fn error(status: u16, code: &str) -> Self {
        Self::json(status, json!({"error": code}))
    }

    pub fn with_header(mut self, name: &str, value: impl Into<String>) -> Self {
        self.headers.push((name.into(), value.into()));
        self
    }

    pub fn into_parts(self) -> (u16, String) {
        (self.status, self.body)
    }

    pub fn reason(&self) -> &'static str {
        match self.status {
            200 => "OK",
            202 => "Accepted",
            400 => "Bad Request",
            401 => "Unauthorized",
            403 => "Forbidden",
            404 => "Not Found",
            405 => "Method Not Allowed",
            409 => "Conflict",
            413 => "Payload Too Large",
            500 => "Internal Server Error",
            502 => "Bad Gateway",
            504 => "Gateway Timeout",
            _ => "OK",
        }
    }
}

/// Parsed request line plus headers, without a body.
#[derive(Debug, Clone)]
pub struct RequestHead {
    pub method: String,
    pub target: String,
    pub headers: Vec<(String, String)>,
}

impl RequestHead {
    pub fn new(method: &str, target: &str) -> Self {
        Self {
            method: method.to_string(),
            target: target.to_string(),
            headers: Vec::new(),
        }
    }

    pub fn with_header(mut self, name: &str, value: &str) -> Self {
        self.headers.push((name.to_string(), value.to_string()));
        self
    }

    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(key, _)| key.eq_ignore_ascii_case(name))
            .map(|(_, value)| value.as_str())
    }

    /// Request path without the query string.
    pub fn path(&self) -> &str {
        self.target.split('?').next().unwrap_or(&self.target)
    }

    /// Minimal query parameter extraction (`+` and percent escapes decoded).
    pub fn query_param(&self, name: &str) -> Option<String> {
        let query = self.target.split_once('?')?.1;
        for pair in query.split('&') {
            let (key, value) = pair.split_once('=').unwrap_or((pair, ""));
            if key == name {
                return Some(decode_query(value));
            }
        }
        None
    }

    fn cookie(&self, name: &str) -> Option<&str> {
        self.header("cookie")?
            .split(';')
            .filter_map(|pair| pair.trim().split_once('='))
            .find(|(key, _)| *key == name)
            .map(|(_, value)| value.trim())
    }

    /// Cross-site guard for mutating requests: a present `Origin` must match the
    /// `Host` that served the page.
    pub fn same_origin(&self) -> bool {
        let Some(origin) = self.header("origin") else {
            return true;
        };
        let origin_authority = origin
            .trim_end_matches('/')
            .split_once("://")
            .map(|(_, rest)| rest)
            .unwrap_or(origin.trim_end_matches('/'));
        match self.header("host") {
            Some(host) => host_of(host).eq_ignore_ascii_case(origin_authority),
            None => false,
        }
    }
}

fn host_of(authority: &str) -> &str {
    authority.split('/').next().unwrap_or(authority)
}

fn decode_query(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        match bytes[index] {
            b'+' => decoded.push(b' '),
            b'%' if index + 2 < bytes.len() => {
                let hex = std::str::from_utf8(&bytes[index + 1..index + 3]).unwrap_or_default();
                match u8::from_str_radix(hex, 16) {
                    Ok(byte) => {
                        decoded.push(byte);
                        index += 2;
                    }
                    Err(_) => decoded.push(b'%'),
                }
            }
            byte => decoded.push(byte),
        }
        index += 1;
    }
    String::from_utf8_lossy(&decoded).to_string()
}

fn constant_time_eq(left: &str, right: &str) -> bool {
    let left = left.as_bytes();
    let right = right.as_bytes();
    let mut difference = left.len() ^ right.len();
    for index in 0..left.len().max(right.len()) {
        let a = left.get(index).copied().unwrap_or(0);
        let b = right.get(index).copied().unwrap_or(0);
        difference |= usize::from(a ^ b);
    }
    difference == 0
}

/// Who may call the product API.
#[derive(Debug, Clone, Default)]
pub struct AccessPolicy {
    token: Option<String>,
}

impl AccessPolicy {
    /// `None` allows loopback-only deployments without a token.
    pub fn new(token: Option<String>) -> Self {
        Self {
            token: token.filter(|token| !token.is_empty()),
        }
    }

    pub fn requires_token(&self) -> bool {
        self.token.is_some()
    }

    fn matches(&self, candidate: &str) -> bool {
        self.token
            .as_deref()
            .map(|token| constant_time_eq(token, candidate))
            .unwrap_or(false)
    }

    /// Cookie first so server-sent events work without custom headers, then the
    /// bearer and explicit header forms for scripts, then a query token.
    pub fn authorize(&self, head: &RequestHead) -> bool {
        let Some(token) = self.token.as_deref() else {
            return true;
        };
        if head
            .cookie(SESSION_COOKIE)
            .map(|candidate| constant_time_eq(token, candidate))
            .unwrap_or(false)
        {
            return true;
        }
        if head
            .header("authorization")
            .and_then(|value| value.strip_prefix("Bearer "))
            .map(|candidate| constant_time_eq(token, candidate.trim()))
            .unwrap_or(false)
        {
            return true;
        }
        if head
            .header("x-marble-token")
            .map(|candidate| constant_time_eq(token, candidate.trim()))
            .unwrap_or(false)
        {
            return true;
        }
        head.query_param("token")
            .map(|candidate| constant_time_eq(token, &candidate))
            .unwrap_or(false)
    }

    /// Validate a token submitted to the login route.
    pub fn accepts_token(&self, candidate: &str) -> bool {
        self.matches(candidate)
    }
}

/// Construction inputs for a served marble product.
pub struct MarbleServerParts {
    pub listener: TcpListener,
    pub state: Arc<Mutex<ProductState>>,
    pub store: Option<Arc<StateStore>>,
    pub approvals: ApprovalInbox,
    pub events: EventBus,
    pub turn_tx: mpsc::Sender<TurnCommand>,
    pub policy: AccessPolicy,
}

/// Local server owning product state, events, and explicit host decisions.
pub struct MarbleServer {
    listener: TcpListener,
    state: Arc<Mutex<ProductState>>,
    store: Option<Arc<StateStore>>,
    approvals: ApprovalInbox,
    events: EventBus,
    turn_tx: mpsc::Sender<TurnCommand>,
    policy: AccessPolicy,
    cancellations: Arc<Mutex<HashMap<String, Arc<AtomicBool>>>>,
}

impl MarbleServer {
    pub fn new(parts: MarbleServerParts) -> Self {
        Self {
            listener: parts.listener,
            state: parts.state,
            store: parts.store,
            approvals: parts.approvals,
            events: parts.events,
            turn_tx: parts.turn_tx,
            policy: parts.policy,
            cancellations: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub fn local_addr(&self) -> SocketAddr {
        self.listener
            .local_addr()
            .expect("glass-marble listener bound to an ephemeral local address")
    }

    pub fn state(&self) -> ProductState {
        self.lock_state().clone()
    }

    fn lock_state(&self) -> std::sync::MutexGuard<'_, ProductState> {
        self.state.lock().unwrap_or_else(|error| error.into_inner())
    }

    /// Route one request without transport, used by tests.
    pub fn handle(&self, method: &str, target: &str, body: &str) -> (u16, String) {
        self.handle_request(&RequestHead::new(method, target), body)
            .into_parts()
    }

    pub fn handle_request(&self, head: &RequestHead, body: &str) -> Response {
        let path = head.path();
        if head.method == "GET" && (path == "/" || path == "/index.html") {
            return Response::html(STATIC_HTML)
                .with_header("X-Content-Type-Options", "nosniff")
                .with_header("Referrer-Policy", "no-referrer")
                .with_header(
                    "Content-Security-Policy",
                    "default-src 'self'; img-src 'self' data:; style-src 'self' 'unsafe-inline'; \
                     script-src 'self' 'unsafe-inline'; connect-src 'self'; frame-ancestors 'none'",
                );
        }
        if head.method == "GET" && path == "/healthz" {
            let (revision, control, presence) = {
                let guard = self.lock_state();
                (
                    guard.revision,
                    guard.control.clone(),
                    guard.presence.document(&guard),
                )
            };
            return Response::json(
                200,
                json!({
                    "ok": true,
                    "revision": revision,
                    "sequence": self.events.sequence(),
                    "authRequired": self.policy.requires_token(),
                    "profile": control.profile,
                    "paused": control.paused,
                    "queued": control.queue.len(),
                    "status": presence["status"],
                    "uptimeSeconds": presence["uptimeSeconds"],
                    "routines": presence["routines"],
                    "memories": presence["memories"],
                }),
            );
        }
        if head.method == "POST" && path == "/api/login" {
            return self.login(body);
        }
        if !self.policy.authorize(head) {
            return Response::error(401, "unauthorized");
        }
        if head.method == "POST" && !head.same_origin() {
            return Response::error(403, "cross_origin_request");
        }
        if head.method == "GET" && path == "/api/state" {
            return self.state_response();
        }
        if head.method == "GET" && path == "/api/events" {
            return Response::error(400, "stream_requires_socket");
        }
        if head.method == "POST" && path == "/api/turns" {
            return self.handle_turn(body);
        }
        if head.method == "GET" && path == "/api/control" {
            return Response::json(200, self.control_document());
        }
        if head.method == "POST" && path == "/api/control/profile" {
            return self.set_profile(body);
        }
        if head.method == "POST" && path == "/api/control/pause" {
            return self.set_paused(true);
        }
        if head.method == "POST" && path == "/api/control/resume" {
            return self.set_paused(false);
        }
        if head.method == "POST" && path == "/api/control/stop" {
            return self.stop_all();
        }
        if head.method == "POST" && path == "/api/control/grants" {
            return self.add_grant(body);
        }
        if head.method == "POST"
            && let Some(rest) = path.strip_prefix("/api/control/grants/")
            && let Some(grant_id) = rest.strip_suffix("/revoke")
        {
            return self.revoke_grant(grant_id);
        }
        if head.method == "GET" && path == "/api/presence" {
            return self.presence_response();
        }
        if head.method == "GET" && path == "/api/memory" {
            return self.memory_response();
        }
        if head.method == "POST" && path == "/api/memory" {
            return self.remember_route(body);
        }
        if head.method == "POST"
            && let Some(rest) = path.strip_prefix("/api/memory/")
        {
            if let Some(id) = rest.strip_suffix("/forget") {
                return self.forget_memory(id);
            }
            if let Some(id) = rest.strip_suffix("/accept") {
                return self.accept_suggestion(id);
            }
            if let Some(id) = rest.strip_suffix("/dismiss") {
                return self.dismiss_suggestion(id);
            }
        }
        if head.method == "GET" && path == "/api/routines" {
            return self.routines_response();
        }
        if head.method == "POST" && path == "/api/routines" {
            return self.add_routine(body);
        }
        if head.method == "POST"
            && let Some(rest) = path.strip_prefix("/api/routines/")
        {
            if let Some(id) = rest.strip_suffix("/run") {
                return self.run_routine(id);
            }
            if let Some(id) = rest.strip_suffix("/enable") {
                return self.set_routine_enabled(id, true);
            }
            if let Some(id) = rest.strip_suffix("/disable") {
                return self.set_routine_enabled(id, false);
            }
            if let Some(id) = rest.strip_suffix("/remove") {
                return self.remove_routine(id);
            }
        }
        if head.method == "GET" && path == "/api/notifications" {
            return self.notifications_response();
        }
        if head.method == "POST" && path == "/api/notifications" {
            return self.save_notifications(body);
        }
        if head.method == "POST" && path == "/api/notifications/test" {
            return self.test_notification();
        }
        if head.method == "GET" && path == "/api/devices" {
            return self.devices_response();
        }
        if head.method == "POST" && path == "/api/devices" {
            return self.register_device(body);
        }
        if head.method == "POST"
            && let Some(token) = path
                .strip_prefix("/api/devices/")
                .and_then(|rest| rest.strip_suffix("/remove"))
        {
            return self.remove_device(token);
        }
        if head.method == "GET" && path == "/api/voice" {
            return self.voice_response();
        }
        if head.method == "POST" && path == "/api/voice" {
            return self.save_voice(body);
        }
        if head.method == "POST" && path == "/api/voice/speak" {
            return self.speak(body);
        }
        if head.method == "POST"
            && let Some(rest) = path.strip_prefix("/api/tasks/")
            && let Some(task_id) = rest.strip_suffix("/cancel")
        {
            return self.cancel_task(task_id);
        }
        if head.method == "POST"
            && let Some(rest) = path.strip_prefix("/api/approvals/")
            && let Some((request_id, action)) = rest.rsplit_once('/')
        {
            return self.decide_approval(request_id, action, body);
        }
        Response::error(404, "not_found")
    }

    fn login(&self, body: &str) -> Response {
        if !self.policy.requires_token() {
            return Response::json(200, json!({"ok": true, "required": false}));
        }
        let parsed: Value = match serde_json::from_str(body) {
            Ok(value) => value,
            Err(_) => return Response::error(400, "invalid_json"),
        };
        let Some(token) = parsed.get("token").and_then(Value::as_str) else {
            return Response::error(400, "invalid_token");
        };
        if !self.policy.accepts_token(token) {
            return Response::error(401, "unauthorized");
        }
        Response::json(200, json!({"ok": true, "required": true})).with_header(
            "Set-Cookie",
            format!("{SESSION_COOKIE}={token}; Path=/; HttpOnly; SameSite=Strict; Max-Age=2592000"),
        )
    }

    fn state_response(&self) -> Response {
        let mut guard = self.lock_state();
        guard.approvals = self.approvals.pending();
        let snapshot = guard.clone();
        drop(guard);
        let mut value = snapshot.encode();
        if let Value::Object(ref mut map) = value {
            // Derived documents: the raw counters are not the whole story.
            map.insert("presence".into(), snapshot.presence.document(&snapshot));
            map.insert("memory".into(), snapshot.memory.document());
            map.insert("routines".into(), snapshot.routines.document());
            map.insert("notifications".into(), snapshot.notifications.document());
            map.insert("voice".into(), snapshot.voice.document());
            map.insert(
                "devices".into(),
                Value::Array(
                    snapshot
                        .devices
                        .iter()
                        .map(|device| {
                            json!({
                                "tokenPrefix": device.token.chars().take(8).collect::<String>(),
                                "platform": device.platform,
                                "topic": device.topic,
                                "lastSeenAt": device.last_seen_at,
                            })
                        })
                        .collect(),
                ),
            );
        }
        Response::json(200, value)
    }

    fn presence_response(&self) -> Response {
        let guard = self.lock_state();
        Response::json(200, guard.presence.document(&guard))
    }

    fn memory_response(&self) -> Response {
        let guard = self.lock_state();
        Response::json(200, guard.memory.document())
    }

    /// `POST /api/memory` — remember one statement explicitly.
    fn remember_route(&self, body: &str) -> Response {
        let parsed: Value = match serde_json::from_str(body) {
            Ok(value) => value,
            Err(_) => return Response::error(400, "invalid_json"),
        };
        let text = parsed
            .get("text")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let kind = parsed.get("kind").and_then(Value::as_str).unwrap_or("fact");
        self.remember_text(text, kind, "", true)
    }

    fn remember_text(
        &self,
        text: &str,
        kind: &str,
        conversation_id: &str,
        explicit: bool,
    ) -> Response {
        let outcome = {
            let mut guard = self.lock_state();
            let outcome = guard.memory.remember(text, kind, conversation_id, explicit);
            if outcome.is_ok() {
                guard.bump();
            }
            outcome
        };
        match outcome {
            Ok(fact) => {
                self.save_state();
                self.events.publish(
                    conversation_id,
                    "",
                    "memory",
                    json!({"remembered": fact.id, "text": fact.text, "kind": fact.kind}),
                );
                Response::json(
                    200,
                    json!({
                        "ok": true,
                        "remembered": fact,
                        "memory": self.lock_state().memory.document(),
                    }),
                )
            }
            Err(reason) => {
                Response::json(400, json!({"error": "invalid_memory", "message": reason}))
            }
        }
    }

    fn forget_memory(&self, id: &str) -> Response {
        let forgotten = {
            let mut guard = self.lock_state();
            let forgotten = guard.memory.forget(id);
            if forgotten {
                guard.bump();
            }
            forgotten
        };
        if forgotten {
            self.save_state();
            self.events
                .publish("", "", "memory", json!({"forgotten": id}));
        }
        Response::json(
            if forgotten { 200 } else { 404 },
            json!({"ok": forgotten, "memory": self.lock_state().memory.document()}),
        )
    }

    /// Explicit "forget: …" command: drop every fact matching the text.
    fn forget_text(&self, query: &str) -> Response {
        let forgotten = {
            let mut guard = self.lock_state();
            let forgotten = guard.memory.forget_matching(query);
            if forgotten > 0 {
                guard.bump();
            }
            forgotten
        };
        if forgotten > 0 {
            self.save_state();
        }
        Response::json(
            200,
            json!({
                "ok": true,
                "forgotten": forgotten,
                "memory": self.lock_state().memory.document(),
            }),
        )
    }

    fn accept_suggestion(&self, id: &str) -> Response {
        let fact = {
            let mut guard = self.lock_state();
            let fact = guard.memory.accept(id);
            if fact.is_some() {
                guard.bump();
            }
            fact
        };
        match fact {
            Some(fact) => {
                self.save_state();
                self.events.publish(
                    "",
                    "",
                    "memory",
                    json!({"remembered": fact.id, "text": fact.text}),
                );
                Response::json(
                    200,
                    json!({"ok": true, "remembered": fact, "memory": self.lock_state().memory.document()}),
                )
            }
            None => Response::json(404, json!({"error": "unknown_suggestion", "id": id})),
        }
    }

    fn dismiss_suggestion(&self, id: &str) -> Response {
        let dismissed = {
            let mut guard = self.lock_state();
            let dismissed = guard.memory.dismiss(id);
            if dismissed {
                guard.bump();
            }
            dismissed
        };
        if dismissed {
            self.save_state();
        }
        Response::json(
            if dismissed { 200 } else { 404 },
            json!({"ok": dismissed, "memory": self.lock_state().memory.document()}),
        )
    }

    fn routines_response(&self) -> Response {
        let guard = self.lock_state();
        Response::json(200, guard.routines.document())
    }

    fn add_routine(&self, body: &str) -> Response {
        let parsed: Value = match serde_json::from_str(body) {
            Ok(value) => value,
            Err(_) => return Response::error(400, "invalid_json"),
        };
        let title = parsed
            .get("title")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let prompt = parsed
            .get("prompt")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let schedule = parsed
            .get("schedule")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let conversation_id = parsed
            .get("conversationId")
            .and_then(Value::as_str)
            .filter(|id| !id.is_empty())
            .map(str::to_string);
        let outcome = {
            let mut guard = self.lock_state();
            let outcome = guard.routines.add(title, prompt, schedule, conversation_id);
            if outcome.is_ok() {
                guard.bump();
            }
            outcome
        };
        match outcome {
            Ok(routine) => {
                self.save_state();
                self.events.publish(
                    "",
                    "",
                    "routine",
                    json!({"routineId": routine.id, "title": routine.title, "created": true}),
                );
                Response::json(
                    200,
                    json!({"ok": true, "routine": routine, "routines": self.lock_state().routines.document()}),
                )
            }
            Err(reason) => {
                Response::json(400, json!({"error": "invalid_routine", "message": reason}))
            }
        }
    }

    /// Admit one routine now, honouring the pause queue like any other message.
    fn run_routine(&self, id: &str) -> Response {
        let routine = {
            let guard = self.lock_state();
            guard.routines.get(id).cloned()
        };
        let Some(routine) = routine else {
            return Response::json(404, json!({"error": "unknown_routine", "id": id}));
        };
        let paused = self.lock_state().control.paused;
        if paused {
            let queued = {
                let mut guard = self.lock_state();
                let outcome = guard
                    .control
                    .enqueue(routine.conversation_id.clone(), routine.prompt.clone());
                if outcome.is_ok() {
                    guard.routines.note_run(&routine.id, "", now_millis());
                    guard.routines.note_status(&routine.id, "queued");
                    guard.bump();
                }
                outcome
            };
            return match queued {
                Ok(entry) => {
                    self.save_state();
                    Response::json(
                        202,
                        json!({"queued": true, "queueId": entry.id, "routines": self.lock_state().routines.document()}),
                    )
                }
                Err(reason) => {
                    Response::json(409, json!({"error": "queue_full", "message": reason}))
                }
            };
        }
        let mut request = TurnRequest::new(routine.conversation_id.clone(), routine.prompt.clone());
        request.routine_id = Some(routine.id.clone());
        if self
            .turn_tx
            .send(TurnCommand {
                request,
                accepted: mpsc::channel().0,
            })
            .is_err()
        {
            return Response::error(503, "turn_worker_unavailable");
        }
        // The worker records the run once the durable task exists, so a manual
        // run and a scheduled one are counted exactly once.
        self.save_state();
        self.events.publish(
            "",
            "",
            "routine",
            json!({"routineId": routine.id, "title": routine.title, "queued": false}),
        );
        Response::json(
            202,
            json!({"admitted": true, "routines": self.lock_state().routines.document()}),
        )
    }

    fn set_routine_enabled(&self, id: &str, enabled: bool) -> Response {
        let changed = {
            let mut guard = self.lock_state();
            let changed = guard.routines.set_enabled(id, enabled);
            if changed {
                guard.bump();
            }
            changed
        };
        if changed {
            self.save_state();
        }
        Response::json(
            if changed { 200 } else { 404 },
            json!({"ok": changed, "routines": self.lock_state().routines.document()}),
        )
    }

    fn remove_routine(&self, id: &str) -> Response {
        let removed = {
            let mut guard = self.lock_state();
            let removed = guard.routines.remove(id);
            if removed {
                guard.bump();
            }
            removed
        };
        if removed {
            self.save_state();
        }
        Response::json(
            if removed { 200 } else { 404 },
            json!({"ok": removed, "routines": self.lock_state().routines.document()}),
        )
    }

    fn notifications_response(&self) -> Response {
        let guard = self.lock_state();
        let mut document = guard.notifications.document();
        if let Value::Object(ref mut map) = document {
            map.insert(
                "devices".into(),
                Value::Array(
                    guard
                        .devices
                        .iter()
                        .map(|device| {
                            json!({
                                "tokenPrefix": device.token.chars().take(8).collect::<String>(),
                                "platform": device.platform,
                                "topic": device.topic,
                                "lastSeenAt": device.last_seen_at,
                            })
                        })
                        .collect(),
                ),
            );
        }
        Response::json(200, document)
    }

    fn save_notifications(&self, body: &str) -> Response {
        let parsed: Value = match serde_json::from_str(body) {
            Ok(value) => value,
            Err(_) => return Response::error(400, "invalid_json"),
        };
        let mut settings = self.lock_state().notifications.clone();
        if let Some(url) = parsed.get("webhookUrl").and_then(Value::as_str) {
            let url = url.trim();
            if !url.is_empty() && !(url.starts_with("http://") || url.starts_with("https://")) {
                return Response::json(
                    400,
                    json!({"error": "invalid_webhook", "message": "use an http:// or https:// URL"}),
                );
            }
            settings.webhook_url = url.to_string();
        }
        if let Some(token) = parsed.get("webhookToken").and_then(Value::as_str) {
            settings.webhook_token = token.trim().to_string();
        }
        if let Some(events) = parsed.get("events").and_then(Value::as_array) {
            let mut selected: Vec<String> = Vec::new();
            for event in events {
                let Some(name) = event.as_str() else { continue };
                if notify::NOTIFY_EVENTS.contains(&name)
                    && !selected.iter().any(|known| known == name)
                {
                    selected.push(name.to_string());
                }
            }
            settings.events = selected;
        }
        if let Some(apns) = parsed.get("apns") {
            if apns.is_null() {
                settings.apns = None;
            } else {
                let existing = settings.apns.clone().unwrap_or_default();
                let field = |name: &str, fallback: String| {
                    apns.get(name)
                        .and_then(Value::as_str)
                        .map(|value| value.trim().to_string())
                        .unwrap_or(fallback)
                };
                settings.apns = Some(ApnsSettings {
                    key_file: field("keyFile", existing.key_file),
                    team_id: field("teamId", existing.team_id),
                    key_id: field("keyId", existing.key_id),
                    topic: field("topic", existing.topic),
                    environment: field("environment", existing.environment),
                });
            }
        }
        {
            let mut guard = self.lock_state();
            guard.notifications = settings;
            guard.bump();
        }
        self.save_state();
        self.notifications_response()
    }

    fn test_notification(&self) -> Response {
        let (settings, devices) = {
            let guard = self.lock_state();
            (guard.notifications.clone(), guard.devices.clone())
        };
        let mut probe = settings;
        probe.events = vec!["test".into()];
        let reports: Vec<Value> = notify::deliver(
            &probe,
            &devices,
            &Notification::new(
                "test",
                "Marble test",
                "If you can read this, Marble can reach you.",
                "",
                "",
            ),
        )
        .into_iter()
        .map(|report| json!({"channel": report.channel, "ok": report.ok, "detail": report.detail}))
        .collect();
        Response::json(
            200,
            json!({
                "ok": reports.iter().any(|report| report["ok"] == json!(true)),
                "delivered": reports,
            }),
        )
    }

    fn devices_response(&self) -> Response {
        let guard = self.lock_state();
        Response::json(
            200,
            json!({
                "devices": Value::Array(
                    guard
                        .devices
                        .iter()
                        .map(|device| {
                            json!({
                                "tokenPrefix": device.token.chars().take(8).collect::<String>(),
                                "platform": device.platform,
                                "topic": device.topic,
                                "lastSeenAt": device.last_seen_at,
                            })
                        })
                        .collect(),
                ),
                "limit": notify::DEVICE_LIMIT,
            }),
        )
    }

    fn register_device(&self, body: &str) -> Response {
        let parsed: Value = match serde_json::from_str(body) {
            Ok(value) => value,
            Err(_) => return Response::error(400, "invalid_json"),
        };
        let token = parsed
            .get("token")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let platform = parsed
            .get("platform")
            .and_then(Value::as_str)
            .unwrap_or("ios");
        let topic = parsed
            .get("topic")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let outcome = {
            let mut guard = self.lock_state();
            let outcome = notify::upsert_device(&mut guard.devices, token, platform, topic);
            if outcome.is_ok() {
                guard.bump();
            }
            outcome
        };
        match outcome {
            Ok(device) => {
                self.save_state();
                Response::json(
                    200,
                    json!({"ok": true, "device": {"tokenPrefix": device.token.chars().take(8).collect::<String>(), "platform": device.platform}}),
                )
            }
            Err(reason) => {
                Response::json(400, json!({"error": "invalid_device", "message": reason}))
            }
        }
    }

    fn remove_device(&self, token: &str) -> Response {
        let needle = token.trim().to_ascii_lowercase();
        let removed = {
            let mut guard = self.lock_state();
            let before = guard.devices.len();
            guard
                .devices
                .retain(|device| !device.token.starts_with(&needle) || needle.is_empty());
            let removed = guard.devices.len() != before;
            if removed {
                guard.bump();
            }
            removed
        };
        if removed {
            self.save_state();
        }
        Response::json(if removed { 200 } else { 404 }, json!({"ok": removed}))
    }

    fn voice_response(&self) -> Response {
        let guard = self.lock_state();
        Response::json(200, guard.voice.document())
    }

    fn save_voice(&self, body: &str) -> Response {
        let parsed: Value = match serde_json::from_str(body) {
            Ok(value) => value,
            Err(_) => return Response::error(400, "invalid_json"),
        };
        let mut voice = self.lock_state().voice.clone();
        if let Some(enabled) = parsed.get("enabled").and_then(Value::as_bool) {
            voice.enabled = enabled;
        }
        if let Some(rate) = parsed.get("rate").and_then(Value::as_f64) {
            voice.rate = rate as f32;
        }
        if let Some(name) = parsed.get("voice").and_then(Value::as_str) {
            voice.voice = name.to_string();
        }
        let voice = voice.normalize();
        {
            let mut guard = self.lock_state();
            guard.voice = voice;
            guard.bump();
        }
        self.save_state();
        self.voice_response()
    }

    fn speak(&self, body: &str) -> Response {
        let parsed: Value = match serde_json::from_str(body) {
            Ok(value) => value,
            Err(_) => return Response::error(400, "invalid_json"),
        };
        let Some(text) = parsed.get("text").and_then(Value::as_str) else {
            return Response::error(400, "invalid_text");
        };
        Response::json(
            200,
            json!({
                "spoken": voice::spoken_form(text, voice::SPOKEN_LIMIT),
                "provider": "on-device",
            }),
        )
    }

    fn control_document(&self) -> Value {
        let control = self.lock_state().control.clone();
        control.document()
    }

    fn set_profile(&self, body: &str) -> Response {
        let parsed: Value = match serde_json::from_str(body) {
            Ok(value) => value,
            Err(_) => return Response::error(400, "invalid_json"),
        };
        let Some(raw) = parsed.get("profile").and_then(Value::as_str) else {
            return Response::error(400, "invalid_profile");
        };
        let Some(profile) = ControlProfile::parse(raw) else {
            return Response::json(400, json!({"error": "invalid_profile", "profile": raw}));
        };
        {
            let mut guard = self.lock_state();
            guard.control.profile = profile;
            guard.control.note(
                "",
                "",
                "",
                "",
                "allowed",
                &format!("profile set to {}", profile.as_str()),
            );
            guard.bump();
        }
        self.save_state();
        self.events
            .publish("", "", "control", json!({"profile": profile.as_str()}));
        Response::json(200, json!({"ok": true, "control": self.control_document()}))
    }

    /// Pause or resume admission. Resuming drains the queue in arrival order.
    fn set_paused(&self, paused: bool) -> Response {
        let queued = {
            let mut guard = self.lock_state();
            guard.control.paused = paused;
            guard.control.note(
                "",
                "",
                "",
                "",
                "allowed",
                if paused {
                    "admission paused"
                } else {
                    "admission resumed"
                },
            );
            guard.bump();
            if paused {
                Vec::new()
            } else {
                guard.control.take_queue()
            }
        };
        self.save_state();
        let mut dispatched = 0usize;
        for entry in queued {
            let request = TurnRequest::new(entry.conversation_id, entry.text);
            let (acknowledged, _ack) = mpsc::channel();
            if self
                .turn_tx
                .send(TurnCommand {
                    request,
                    accepted: acknowledged,
                })
                .is_ok()
            {
                dispatched += 1;
            }
        }
        self.events.publish(
            "",
            "",
            "control",
            json!({"paused": paused, "dispatched": dispatched}),
        );
        Response::json(
            200,
            json!({
                "ok": true,
                "paused": paused,
                "dispatched": dispatched,
                "control": self.control_document(),
            }),
        )
    }

    /// Pause admission and cancel every running task in one deliberate stop.
    fn stop_all(&self) -> Response {
        {
            let mut guard = self.lock_state();
            guard.control.paused = true;
            guard
                .control
                .note("", "", "", "", "allowed", "stop requested for every task");
            guard.bump();
        }
        self.save_state();
        let flags: Vec<(String, Arc<AtomicBool>)> = self
            .cancellations
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .iter()
            .map(|(task_id, flag)| (task_id.clone(), Arc::clone(flag)))
            .collect();
        let mut cancelled = Vec::new();
        for (task_id, flag) in flags {
            flag.store(true, Ordering::Relaxed);
            self.events.publish(
                "",
                &task_id,
                "cancel_requested",
                json!({ "taskId": task_id }),
            );
            cancelled.push(task_id);
        }
        self.events
            .publish("", "", "control", json!({ "stopped": cancelled.len() }));
        Response::json(
            200,
            json!({
                "ok": true,
                "paused": true,
                "cancelled": cancelled,
                "control": self.control_document(),
            }),
        )
    }

    fn add_grant(&self, body: &str) -> Response {
        let parsed: Value = match serde_json::from_str(body) {
            Ok(value) => value,
            Err(_) => return Response::error(400, "invalid_json"),
        };
        let tool = parsed
            .get("tool")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let prefix = parsed
            .get("pathPrefix")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let outcome = {
            let mut guard = self.lock_state();
            let outcome = guard.control.grant(tool, prefix);
            if outcome.is_ok() {
                guard.bump();
            }
            outcome
        };
        match outcome {
            Ok(grant) => {
                self.save_state();
                Response::json(
                    200,
                    json!({ "ok": true, "grant": grant, "control": self.control_document() }),
                )
            }
            Err(reason) => {
                Response::json(400, json!({ "error": "invalid_grant", "message": reason }))
            }
        }
    }

    fn revoke_grant(&self, grant_id: &str) -> Response {
        let revoked = {
            let mut guard = self.lock_state();
            let revoked = guard.control.revoke(grant_id);
            if revoked {
                guard.bump();
            }
            revoked
        };
        if revoked {
            self.save_state();
        }
        Response::json(
            if revoked { 200 } else { 404 },
            json!({ "ok": revoked, "grantId": grant_id, "control": self.control_document() }),
        )
    }

    fn handle_turn(&self, body: &str) -> Response {
        let parsed: Value = match serde_json::from_str(body) {
            Ok(value) => value,
            Err(_) => return Response::error(400, "invalid_json"),
        };
        let text = parsed
            .get("text")
            .and_then(Value::as_str)
            .map(str::trim)
            .unwrap_or_default()
            .to_string();
        if text.is_empty() {
            return Response::error(400, "empty_message");
        }
        if text.len() > 64 * 1024 {
            return Response::error(413, "message_too_large");
        }
        let conversation_id = match parsed.get("conversationId") {
            None | Some(Value::Null) => None,
            Some(Value::String(id)) if valid_conversation_id(id) => Some(id.clone()),
            Some(_) => return Response::error(400, "invalid_conversation_id"),
        };
        // Explicit memory instructions are answered by the product itself: a
        // remembered fact is a durable write, not a model turn.
        match memory::parse_command(&text) {
            Some(MemoryCommand::Remember { text: fact, kind }) => {
                return self.remember_text(
                    &fact,
                    &kind,
                    conversation_id.as_deref().unwrap_or_default(),
                    true,
                );
            }
            Some(MemoryCommand::Forget { text: query }) => return self.forget_text(&query),
            None => {}
        }
        if self.lock_state().control.paused {
            // Paused admission is a queue, not a rejection: the message keeps its
            // place, and resuming dispatches it in arrival order.
            let queued = {
                let mut guard = self.lock_state();
                match guard.control.enqueue(conversation_id, text) {
                    Ok(entry) => {
                        let position = guard.control.queue.len();
                        guard.bump();
                        Ok((entry, position))
                    }
                    Err(reason) => Err(reason),
                }
            };
            return match queued {
                Ok((entry, position)) => {
                    self.save_state();
                    self.events.publish(
                        "",
                        "",
                        "queued",
                        json!({ "queueId": entry.id, "position": position }),
                    );
                    Response::json(
                        202,
                        json!({
                            "queued": true,
                            "queueId": entry.id,
                            "position": position,
                            "paused": true,
                        }),
                    )
                }
                Err(reason) => {
                    Response::json(409, json!({ "error": "queue_full", "message": reason }))
                }
            };
        }
        let request = TurnRequest::new(conversation_id, text);
        let request_id = request.request_id.clone();
        let cancel = request.cancel.clone();
        let (accepted_tx, accepted_rx) = mpsc::channel();
        if self
            .turn_tx
            .send(TurnCommand {
                request,
                accepted: accepted_tx,
            })
            .is_err()
        {
            return Response::error(503, "turn_worker_unavailable");
        }
        let accepted = match accepted_rx.recv_timeout(ACCEPT_TIMEOUT) {
            Ok(Ok(accepted)) => accepted,
            Ok(Err(error)) => {
                return Response::json(
                    502,
                    json!({"error": "turn_failed", "message": error.to_string()}),
                );
            }
            Err(_) => return Response::error(504, "turn_accept_timeout"),
        };
        self.cancellations
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .insert(accepted.task_id.clone(), cancel);
        Response::json(
            202,
            json!({
                "accepted": true,
                "requestId": request_id,
                "conversationId": accepted.conversation_id,
                "taskId": accepted.task_id,
            }),
        )
    }

    fn cancel_task(&self, task_id: &str) -> Response {
        let flag = self
            .cancellations
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .get(task_id)
            .cloned();
        match flag {
            Some(flag) => {
                flag.store(true, Ordering::Relaxed);
                self.events
                    .publish("", task_id, "cancel_requested", json!({"taskId": task_id}));
                self.save_state();
                Response::json(202, json!({"cancelRequested": true, "taskId": task_id}))
            }
            None => Response::error(404, "unknown_task"),
        }
    }

    fn decide_approval(&self, request_id: &str, action: &str, body: &str) -> Response {
        let approval = match action {
            "allow" => Approval::Approve,
            "deny" => Approval::Deny,
            other => {
                return Response::json(400, json!({"error": "unknown_action", "action": other}));
            }
        };
        // "Allow and remember" widens one decision into a standing grant for the
        // directory it was approved in, which the human can revoke later.
        let remember = serde_json::from_str::<Value>(body)
            .ok()
            .and_then(|value| value.get("remember").and_then(Value::as_bool))
            .unwrap_or(false);
        let mut granted = None;
        if approval == Approval::Approve
            && remember
            && let Some(pending) = self
                .approvals
                .pending()
                .into_iter()
                .find(|pending| pending.request_id == request_id)
        {
            let path = pending
                .arguments
                .get("path")
                .and_then(Value::as_str)
                .unwrap_or_default();
            let scope = crate::control::directory_scope(path);
            let outcome = {
                let mut guard = self.lock_state();
                let outcome = guard.control.grant(&pending.tool, &scope);
                if outcome.is_ok() {
                    guard.bump();
                }
                outcome
            };
            if let Ok(grant) = outcome {
                granted = Some(grant);
                self.save_state();
            }
        }
        let resolved = self.approvals.resolve(request_id, approval);
        Response::json(
            if resolved { 200 } else { 409 },
            json!({"ok": resolved, "requestId": request_id, "grant": granted}),
        )
    }

    fn save_state(&self) {
        let Some(store) = &self.store else {
            return;
        };
        let snapshot = self.lock_state().clone();
        crate::store::persist(store, &snapshot)
    }

    pub fn serve(self) -> std::io::Result<()> {
        let server = Arc::new(self);
        loop {
            let next = server.listener.incoming().next();
            let stream = match next {
                Some(Ok(stream)) => stream,
                Some(Err(error)) => return Err(error),
                None => break,
            };
            let server = Arc::clone(&server);
            std::thread::spawn(move || {
                let _ = server.serve_connection(stream);
            });
        }
        Ok(())
    }

    fn serve_connection(&self, mut stream: TcpStream) -> std::io::Result<()> {
        let _ = stream.set_read_timeout(Some(Duration::from_secs(30)));
        let mut reader = BufReader::new(&mut stream);
        let head_bytes = read_head(&mut reader)?;
        let head_text = String::from_utf8_lossy(&head_bytes).to_string();
        let head = parse_head(&head_text);
        let content_length = head
            .header("content-length")
            .and_then(|value| value.trim().parse::<usize>().ok())
            .unwrap_or(0);
        if content_length > MAX_BODY_BYTES {
            return write_response(&mut stream, Response::error(413, "body_too_large"));
        }
        let mut body = vec![0u8; content_length];
        if content_length > 0 {
            reader.read_exact(&mut body)?;
        }
        let body = String::from_utf8_lossy(&body).to_string();
        if head.method == "GET" && head.path() == "/api/events" {
            if !self.policy.authorize(&head) {
                return write_response(&mut stream, Response::error(401, "unauthorized"));
            }
            return self.stream_events(&head, &mut stream);
        }
        let response = self.handle_request(&head, &body);
        write_response(&mut stream, response)
    }

    /// Server-sent events with bounded replay from the client's own cursor.
    fn stream_events(&self, head: &RequestHead, stream: &mut TcpStream) -> std::io::Result<()> {
        let since = head
            .query_param("since")
            .and_then(|value| value.parse::<u64>().ok())
            .unwrap_or_else(|| self.events.sequence());
        stream.write_all(
            b"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream; charset=utf-8\r\n\
              Cache-Control: no-store\r\nConnection: close\r\nX-Accel-Buffering: no\r\n\r\n",
        )?;
        stream.flush()?;
        let receiver = self.events.subscribe();
        for event in self.events.since(since) {
            write_event(stream, &event)?;
        }
        loop {
            match receiver.recv_timeout(STREAM_KEEPALIVE) {
                Ok(event) => write_event(stream, &event)?,
                Err(mpsc::RecvTimeoutError::Timeout) => {
                    stream.write_all(b": keepalive\n\n")?;
                    stream.flush()?;
                }
                Err(mpsc::RecvTimeoutError::Disconnected) => break,
            }
        }
        Ok(())
    }
}

fn valid_conversation_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 128
        && id
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || matches!(character, '-' | '_'))
}

fn write_event(stream: &mut TcpStream, event: &TurnEvent) -> std::io::Result<()> {
    let data = serde_json::to_string(&EventBus::encode(event)).unwrap_or_default();
    stream.write_all(
        format!(
            "id: {}\nevent: {}\ndata: {}\n\n",
            event.sequence, event.kind, data
        )
        .as_bytes(),
    )?;
    stream.flush()
}

fn write_response(stream: &mut TcpStream, response: Response) -> std::io::Result<()> {
    let mut head = format!(
        "HTTP/1.1 {} {}\r\nContent-Length: {}\r\nConnection: close\r\n",
        response.status,
        response.reason(),
        response.body.len()
    );
    for (name, value) in &response.headers {
        head.push_str(&format!("{name}: {value}\r\n"));
    }
    head.push_str("\r\n");
    stream.write_all(head.as_bytes())?;
    stream.write_all(response.body.as_bytes())?;
    stream.flush()
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
        if head.len() > MAX_HEAD_BYTES {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "request head exceeds the accepted limit",
            ));
        }
    }
    Ok(head)
}

fn parse_head(head_text: &str) -> RequestHead {
    let mut lines = head_text.split("\r\n");
    let request_line = lines.next().unwrap_or_default();
    let mut parts = request_line.splitn(3, ' ');
    let method = parts.next().unwrap_or_default().to_string();
    let target = parts.next().unwrap_or_default().to_string();
    let mut head = RequestHead::new(&method, &target);
    for line in lines {
        if let Some((name, value)) = line.split_once(':') {
            head.headers
                .push((name.trim().to_string(), value.trim().to_string()));
        }
    }
    head
}

/// Approvals still waiting for an explicit decision, for product surfaces that
/// read state before the next poll.
pub fn pending_approvals(approvals: &ApprovalInbox) -> Vec<PendingApproval> {
    approvals.pending()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn query_tokens_decode_and_match() {
        let head = RequestHead::new("GET", "/api/events?since=4&token=a%2Bb+c");
        assert_eq!(head.query_param("since"), Some("4".to_string()));
        assert_eq!(head.query_param("token"), Some("a+b c".to_string()));
        assert_eq!(head.path(), "/api/events");
    }

    #[test]
    fn access_policy_accepts_cookie_bearer_and_query_tokens() {
        let policy = AccessPolicy::new(Some("secret".into()));
        assert!(!policy.authorize(&RequestHead::new("GET", "/api/state")));
        assert!(policy.authorize(
            &RequestHead::new("GET", "/api/state").with_header("cookie", "marble_token=secret")
        ));
        assert!(policy.authorize(
            &RequestHead::new("GET", "/api/state").with_header("authorization", "Bearer secret")
        ));
        assert!(policy.authorize(&RequestHead::new("GET", "/api/events?token=secret")));
        assert!(!policy.authorize(&RequestHead::new("GET", "/api/events?token=wrong")));
    }

    #[test]
    fn loopback_deployment_without_a_token_allows_requests() {
        let policy = AccessPolicy::new(None);
        assert!(!policy.requires_token());
        assert!(policy.authorize(&RequestHead::new("GET", "/api/state")));
    }

    #[test]
    fn cross_origin_posts_are_rejected() {
        let same = RequestHead::new("POST", "/api/turns")
            .with_header("host", "127.0.0.1:4783")
            .with_header("origin", "http://127.0.0.1:4783");
        assert!(same.same_origin());
        let foreign = RequestHead::new("POST", "/api/turns")
            .with_header("host", "127.0.0.1:4783")
            .with_header("origin", "https://evil.example");
        assert!(!foreign.same_origin());
        assert!(
            !RequestHead::new("POST", "/api/turns")
                .with_header("origin", "https://evil.example")
                .same_origin()
        );
    }

    #[test]
    fn conversation_ids_are_restricted_to_safe_characters() {
        assert!(valid_conversation_id("durable-fixture-1"));
        assert!(!valid_conversation_id(""));
        assert!(!valid_conversation_id("../../etc"));
        assert!(!valid_conversation_id("has space"));
    }

    #[test]
    fn constant_time_comparison_matches_ordinary_equality() {
        assert!(constant_time_eq("abc", "abc"));
        assert!(!constant_time_eq("abc", "abd"));
        assert!(!constant_time_eq("abc", "ab"));
        assert!(constant_time_eq("", ""));
    }
}
