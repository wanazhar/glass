//! Self-hosted HTTP surface for the Glass marble product.
//!
//! Bound to loopback by default; a Tailscale address needs an explicit host and
//! an access token. The surface is intentionally small: state, a turn intake,
//! cancellation, approvals, and one server-sent event stream.

use crate::Approval;
use crate::control::ControlProfile;
use crate::conversation::{ApprovalInbox, SteerRequest};
use crate::events::{EventBus, TurnEvent};
use crate::memory::{self, MemoryCommand};
use crate::notify::{self, ApnsSettings, Notification};
use crate::state::{PendingApproval, ProductState, TaskRecord};
use crate::store::StateStore;
use crate::voice;
use crate::worker::{CancelRegistry, TurnCommand, TurnRequest, TurnSender, now_millis};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use std::time::Duration;

pub const STATIC_HTML: &str = include_str!("static/index.html");
/// Largest accepted JSON request body.
pub const MAX_BODY_BYTES: usize = 256 * 1024;
/// Largest accepted request head.
const MAX_HEAD_BYTES: usize = 16 * 1024;
const SESSION_COOKIE: &str = "marble_token";
/// How long a turn request waits for the worker before it is answered as queued.
const ADMISSION_WAIT: Duration = Duration::from_secs(2);
const STREAM_KEEPALIVE: Duration = Duration::from_secs(15);
/// Socket threads one instance runs at once. Beyond this a connection is told
/// the host is busy instead of growing an unbounded thread per client.
const MAX_CONNECTIONS: usize = 64;
/// Requests one keep-alive connection may serve before it is closed.
const MAX_REQUESTS_PER_CONNECTION: usize = 32;
/// Failed logins from one address inside the window before it is throttled.
const LOGIN_FAILURE_LIMIT: u32 = 5;
const LOGIN_WINDOW: Duration = Duration::from_secs(300);
const LOGIN_BLOCK: Duration = Duration::from_secs(60);

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

    /// A validated cache hit: no body, and the tag the client already has.
    pub fn not_modified(etag: &str) -> Self {
        Self {
            status: 304,
            body: String::new(),
            headers: vec![
                ("ETag".into(), etag.to_string()),
                ("Cache-Control".into(), "no-cache".into()),
            ],
        }
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
            304 => "Not Modified",
            409 => "Conflict",
            413 => "Payload Too Large",
            429 => "Too Many Requests",
            500 => "Internal Server Error",
            502 => "Bad Gateway",
            503 => "Service Unavailable",
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
    /// Two-lane turn queue: typed messages first, then routine firings.
    pub turn_tx: TurnSender,
    /// Corrections for a turn that is already running.
    pub steer_tx: mpsc::Sender<SteerRequest>,
    pub policy: AccessPolicy,
    /// Turns the worker is running, so one stop can cancel them all.
    pub cancels: CancelRegistry,
}

/// Login attempts per caller, so a token cannot be guessed at wire speed.
#[derive(Debug, Default)]
pub struct LoginThrottle {
    attempts: Mutex<HashMap<String, Attempt>>,
}

#[derive(Debug, Clone, Copy)]
struct Attempt {
    failures: u32,
    window_start: u64,
    blocked_until: u64,
}

impl LoginThrottle {
    /// Seconds the caller must wait, when it is currently throttled.
    fn blocked(&self, peer: &str, now: u64) -> Option<u64> {
        let attempts = self
            .attempts
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        let attempt = attempts.get(peer)?;
        (attempt.blocked_until > now).then(|| (attempt.blocked_until - now).div_ceil(1000))
    }

    fn record_failure(&self, peer: &str, now: u64) {
        let mut attempts = self
            .attempts
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        if attempts.len() > 1_024 {
            attempts.clear();
        }
        let attempt = attempts.entry(peer.to_string()).or_insert(Attempt {
            failures: 0,
            window_start: now,
            blocked_until: 0,
        });
        if now.saturating_sub(attempt.window_start) > LOGIN_WINDOW.as_millis() as u64 {
            attempt.failures = 0;
            attempt.window_start = now;
        }
        attempt.failures = attempt.failures.saturating_add(1);
        if attempt.failures >= LOGIN_FAILURE_LIMIT {
            attempt.failures = 0;
            attempt.window_start = now;
            attempt.blocked_until = now + LOGIN_BLOCK.as_millis() as u64;
            tracing::warn!(peer, "too many failed logins; throttling this caller");
        }
    }

    fn record_success(&self, peer: &str) {
        self.attempts
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .remove(peer);
    }
}

/// Local server owning product state, events, and explicit host decisions.
pub struct MarbleServer {
    listener: TcpListener,
    state: Arc<Mutex<ProductState>>,
    store: Option<Arc<StateStore>>,
    approvals: ApprovalInbox,
    events: EventBus,
    turn_tx: TurnSender,
    steer_tx: mpsc::Sender<SteerRequest>,
    policy: AccessPolicy,
    cancels: CancelRegistry,
    login: LoginThrottle,
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
            steer_tx: parts.steer_tx,
            policy: parts.policy,
            cancels: parts.cancels,
            login: LoginThrottle::default(),
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
        self.handle_request_from(head, body, "")
    }

    /// Route one request together with the caller that sent it, so login
    /// failures can be throttled per address.
    pub fn handle_request_from(&self, head: &RequestHead, body: &str, peer: &str) -> Response {
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
            // Liveness means the pieces Marble depends on are actually working:
            // an engine that answers, a document that is being written, and a
            // backup that is not months stale. Reporting `ok` unconditionally is
            // worse than reporting nothing, because it hides the one failure a
            // person would want to be told about.
            let (revision, control, presence, recovered) = {
                let guard = self.lock_state();
                (
                    guard.revision,
                    guard.control.clone(),
                    guard.presence.document(&guard),
                    guard.presence.recovered_tasks,
                )
            };
            let runtime = presence.get("runtime").cloned().unwrap_or(Value::Null);
            let runtime_ok = runtime.get("health").and_then(Value::as_str) == Some("ok");
            let persist_error = crate::store::last_persist_error();
            let healthy = runtime_ok && persist_error.is_none();
            let storage = match self.store.as_deref() {
                Some(store) => json!({
                    "path": store.data_dir().map(|dir| dir.display().to_string()),
                    "document": store.document_exists(),
                    "backup": store.backup_status(),
                    "quarantined": store.quarantined_count(),
                }),
                None => Value::Null,
            };
            return Response::json(
                if healthy { 200 } else { 503 },
                json!({
                    "ok": healthy,
                    "version": env!("CARGO_PKG_VERSION"),
                    "pid": std::process::id(),
                    "revision": revision,
                    "sequence": self.events.sequence(),
                    "authRequired": self.policy.requires_token(),
                    "profile": control.profile,
                    "paused": control.paused,
                    "queued": control.queue.len(),
                    "waiting": self.turn_tx.waiting(),
                    "running": self.cancels.running().len(),
                    "status": presence["status"],
                    "uptimeSeconds": presence["uptimeSeconds"],
                    "routines": presence["routines"],
                    "memories": presence["memories"],
                    "recoveredTasks": recovered,
                    "runtime": runtime,
                    "storage": storage,
                    "lastPersistError": persist_error,
                }),
            );
        }
        if head.method == "POST" && path == "/api/login" {
            return self.login(body, peer);
        }
        if !self.policy.authorize(head) {
            return Response::error(401, "unauthorized");
        }
        if head.method == "POST" && !head.same_origin() {
            return Response::error(403, "cross_origin_request");
        }
        if head.method == "GET" && path == "/api/state" {
            let lean = head.query_param("lean").as_deref() == Some("1")
                || head.query_param("transcript").as_deref() == Some("0");
            return self.state_response(head, lean);
        }
        if head.method == "GET" && path == "/api/transcript/search" {
            return self.search_transcript(head);
        }
        if head.method == "GET" && path == "/api/transcript" {
            return self.transcript_response(head);
        }
        if head.method == "POST"
            && let Some(rest) = path.strip_prefix("/api/conversations/")
        {
            if let Some(id) = rest.strip_suffix("/rename") {
                return self.rename_conversation(id, body);
            }
            if let Some(id) = rest.strip_suffix("/delete") {
                return self.delete_conversation(id);
            }
        }
        if head.method == "GET" && path == "/api/events" {
            return Response::error(400, "stream_requires_socket");
        }
        if head.method == "POST" && path == "/api/model" {
            return self.set_model(body);
        }
        if head.method == "POST" && path == "/api/turns" {
            return self.handle_turn(body);
        }
        if head.method == "POST" && path == "/api/turns/steer" {
            return self.steer_turn(body);
        }
        if head.method == "POST" && path == "/api/turns/retry" {
            return self.retry_turn(body);
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
        if head.method == "GET" && path == "/api/export" {
            return self.export_response(head);
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
            if let Some(id) = rest.strip_suffix("/update") {
                return self.update_memory(id, body);
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
        if head.method == "POST" && path == "/api/routines/offset" {
            return self.set_routine_offset(body);
        }
        if head.method == "POST" && path == "/api/routines/quiet" {
            return self.set_routine_quiet(body);
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
            if let Some(id) = rest.strip_suffix("/catch-up") {
                return self.set_routine_catch_up(id, body);
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

    /// `POST /api/model` — choose the model new conversations use. The worker
    /// applies it before the next conversation is created; Core keeps a live
    /// session on the model it started with.
    fn set_model(&self, body: &str) -> Response {
        let parsed: Value = match serde_json::from_str(body) {
            Ok(value) => value,
            Err(_) => return Response::error(400, "invalid_json"),
        };
        let model = parsed
            .get("model")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .trim()
            .to_string();
        if model.chars().count() > 200 || model.chars().any(char::is_control) {
            return Response::json(
                400,
                json!({
                    "error": "invalid_model",
                    "message": "a model name is at most 200 printable characters",
                }),
            );
        }
        {
            let mut guard = self.lock_state();
            guard.model = model.clone();
            guard.bump();
        }
        self.save_state();
        self.events
            .publish("", "", "control", json!({"model": model}));
        Response::json(
            200,
            json!({
                "ok": true,
                "model": model,
                "engineDefault": model.is_empty(),
                "appliesTo": "new conversations",
            }),
        )
    }

    fn login(&self, body: &str, peer: &str) -> Response {
        if !self.policy.requires_token() {
            return Response::json(200, json!({"ok": true, "required": false}));
        }
        if let Some(remaining) = self.login.blocked(peer, now_millis()) {
            return Response::json(
                429,
                json!({
                    "error": "too_many_attempts",
                    "message": "too many failed logins; wait before trying again",
                    "retryAfter": remaining,
                }),
            )
            .with_header("Retry-After", remaining.to_string());
        }
        let parsed: Value = match serde_json::from_str(body) {
            Ok(value) => value,
            Err(_) => return Response::error(400, "invalid_json"),
        };
        let Some(token) = parsed.get("token").and_then(Value::as_str) else {
            return Response::error(400, "invalid_token");
        };
        if !self.policy.accepts_token(token) {
            self.login.record_failure(peer, now_millis());
            return Response::error(401, "unauthorized");
        }
        self.login.record_success(peer);
        Response::json(200, json!({"ok": true, "required": true})).with_header(
            "Set-Cookie",
            format!("{SESSION_COOKIE}={token}; Path=/; HttpOnly; SameSite=Strict; Max-Age=2592000"),
        )
    }

    /// Product document. `lean` keeps the structure but omits the transcript, so
    /// a polling client can refresh control, presence, and counts without
    /// re-reading months of history over the tailnet.
    fn state_response(&self, head: &RequestHead, lean: bool) -> Response {
        // A revision is the document's identity: nothing in it changes without a
        // bump, so an unchanged poll can be answered without encoding a copy.
        // Pending approvals arrive from the runtime rather than the document, so
        // their count is part of the identity too.
        let revision = self.lock_state().revision;
        let approvals = self.approvals.pending().len();
        let etag = format!(
            "\"{revision}-{approvals}{}\"",
            if lean { "-lean" } else { "" }
        );
        if etag_matches(head, &etag) {
            return Response::not_modified(&etag);
        }
        let mut guard = self.lock_state();
        guard.approvals = self.approvals.pending();
        let snapshot = guard.clone();
        drop(guard);
        let transcript_count = snapshot.transcript.len();
        let mut value = snapshot.encode();
        if let Value::Object(ref mut map) = value {
            map.insert("transcriptCount".into(), json!(transcript_count));
            if lean {
                map.insert("transcript".into(), Value::Array(Vec::new()));
                map.insert("transcriptOmitted".into(), json!(true));
            }
            // Derived documents: the raw counters are not the whole story.
            map.insert("presence".into(), snapshot.presence.document(&snapshot));
            map.insert("memory".into(), snapshot.memory.document());
            map.insert("routines".into(), snapshot.routines.document());
            map.insert("notifications".into(), snapshot.notifications.document());
            map.insert("voice".into(), snapshot.voice.document());
            map.insert("retention".into(), snapshot.retention.document());
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
            .with_header("ETag", etag)
            .with_header("Cache-Control", "no-cache")
    }

    /// `GET /api/export` — a portable copy of the durable document. With
    /// `conversationId`, only that conversation's record, tasks, and history.
    fn export_response(&self, head: &RequestHead) -> Response {
        let conversation_id = head.query_param("conversationId").unwrap_or_default();
        if !conversation_id.is_empty() && !valid_conversation_id(&conversation_id) {
            return Response::error(400, "invalid_conversation_id");
        }
        let guard = self.lock_state();
        if conversation_id.is_empty() {
            return Response::json(
                200,
                json!({
                    "format": "glass-marble/v1",
                    "exportedAt": now_millis(),
                    "revision": guard.revision,
                    "state": &*guard,
                }),
            )
            .with_header(
                "Content-Disposition",
                "attachment; filename=\"marble-export.json\"",
            );
        }
        let Some(conversation) = guard
            .conversations
            .iter()
            .find(|conversation| conversation.id == conversation_id)
        else {
            return Response::json(404, json!({"error": "unknown_conversation"}));
        };
        let tasks: Vec<&crate::state::TaskRecord> = guard
            .tasks
            .iter()
            .filter(|task| task.conversation_id == conversation_id)
            .collect();
        let transcript: Vec<&crate::state::TranscriptEntry> = guard
            .transcript
            .iter()
            .filter(|entry| entry.conversation_id == conversation_id)
            .collect();
        Response::json(
            200,
            json!({
                "format": "glass-marble-conversation/v1",
                "exportedAt": now_millis(),
                "revision": guard.revision,
                "conversation": conversation,
                "tasks": tasks,
                "transcript": transcript,
            }),
        )
        .with_header(
            "Content-Disposition",
            "attachment; filename=\"marble-conversation.json\"",
        )
    }

    /// `GET /api/transcript/search?q=…` — newest matches across all history.
    fn search_transcript(&self, head: &RequestHead) -> Response {
        let query = head.query_param("q").unwrap_or_default();
        if query.trim().is_empty() {
            return Response::json(200, json!({"query": query, "hits": []}));
        }
        let limit = head
            .query_param("limit")
            .and_then(|raw| raw.parse::<usize>().ok())
            .unwrap_or(30);
        let guard = self.lock_state();
        let hits: Vec<Value> = guard
            .search_transcript(&query, limit)
            .into_iter()
            .map(|entry| {
                let title = guard
                    .conversations
                    .iter()
                    .find(|conversation| conversation.id == entry.conversation_id)
                    .map(|conversation| conversation.title.clone())
                    .unwrap_or_default();
                let preview: String = entry.text.chars().take(280).collect();
                json!({
                    "conversationId": entry.conversation_id,
                    "title": title,
                    "kind": entry.kind,
                    "text": preview,
                    "at": entry.at,
                    "taskId": entry.task_id,
                })
            })
            .collect();
        Response::json(200, json!({"query": query, "hits": hits}))
    }

    fn rename_conversation(&self, id: &str, body: &str) -> Response {
        let parsed: Value = match serde_json::from_str(body) {
            Ok(value) => value,
            Err(_) => return Response::error(400, "invalid_json"),
        };
        let title = parsed
            .get("title")
            .and_then(Value::as_str)
            .unwrap_or_default();
        if title.trim().is_empty() {
            return Response::error(400, "invalid_title");
        }
        let renamed = {
            let mut guard = self.lock_state();
            let renamed = guard.rename_conversation(id, title);
            if renamed {
                guard.bump();
            }
            renamed
        };
        if renamed {
            self.save_state();
            self.events
                .publish("", "", "conversation", json!({"renamed": id}));
        }
        Response::json(
            if renamed { 200 } else { 404 },
            json!({"ok": renamed, "conversations": self.lock_state().conversations}),
        )
    }

    fn delete_conversation(&self, id: &str) -> Response {
        let outcome = {
            let mut guard = self.lock_state();
            let outcome = guard.delete_conversation(id);
            if outcome.is_ok() {
                guard.bump();
            }
            outcome
        };
        match outcome {
            Ok(()) => {
                self.save_state();
                self.events
                    .publish("", "", "conversation", json!({"deleted": id}));
                Response::json(
                    200,
                    json!({"ok": true, "conversations": self.lock_state().conversations}),
                )
            }
            Err(reason) if reason == "unknown conversation" => {
                Response::json(404, json!({"error": "unknown_conversation", "id": id}))
            }
            Err(reason) => Response::json(
                409,
                json!({"error": "conversation_busy", "message": reason}),
            ),
        }
    }

    /// `GET /api/transcript?conversationId=…&after=…` — bounded history for one
    /// conversation. `after` is inclusive so entries that share a millisecond
    /// are never lost; clients deduplicate by content.
    fn transcript_response(&self, head: &RequestHead) -> Response {
        let conversation_id = head.query_param("conversationId").unwrap_or_default();
        if !conversation_id.is_empty() && !valid_conversation_id(&conversation_id) {
            return Response::error(400, "invalid_conversation_id");
        }
        let after = match head.query_param("after") {
            Some(raw) if !raw.is_empty() => match raw.parse::<u64>() {
                Ok(value) => Some(value),
                Err(_) => return Response::error(400, "invalid_cursor"),
            },
            _ => None,
        };
        let limit = head
            .query_param("limit")
            .and_then(|raw| raw.parse::<usize>().ok())
            .map(|value| value.clamp(1, 1_000))
            .unwrap_or(500);
        let guard = self.lock_state();
        let mut entries: Vec<&crate::state::TranscriptEntry> = guard
            .transcript
            .iter()
            .filter(|entry| conversation_id.is_empty() || entry.conversation_id == conversation_id)
            .filter(|entry| after.is_none_or(|cursor| entry.at >= cursor))
            .collect();
        let omitted = entries.len().saturating_sub(limit);
        if omitted > 0 {
            entries.drain(..omitted);
        }
        let cursor = entries
            .iter()
            .map(|entry| entry.at)
            .max()
            .unwrap_or_default();
        Response::json(
            200,
            json!({
                "conversationId": conversation_id,
                "entries": entries,
                "cursor": cursor,
                "omitted": omitted,
                "revision": guard.revision,
            }),
        )
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

    /// `POST /api/memory/{id}/update` — correct one remembered statement.
    fn update_memory(&self, id: &str, body: &str) -> Response {
        let parsed: Value = match serde_json::from_str(body) {
            Ok(value) => value,
            Err(_) => return Response::error(400, "invalid_json"),
        };
        let text = parsed
            .get("text")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let kind = parsed
            .get("kind")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let outcome = {
            let mut guard = self.lock_state();
            let outcome = guard.memory.update(id, text, kind);
            if outcome.is_ok() {
                guard.bump();
            }
            outcome
        };
        match outcome {
            Ok(fact) => {
                self.save_state();
                self.events.publish(
                    "",
                    "",
                    "memory",
                    json!({"updated": fact.id, "text": fact.text, "kind": fact.kind}),
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
            Err(reason) if reason == "unknown memory" => {
                Response::json(404, json!({"error": "unknown_memory", "id": id}))
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
            if let Some(offset) = parsed.get("offsetMinutes").and_then(Value::as_i64) {
                // A surface knows its own timezone, so saving a routine from it
                // also fixes what `daily:HH:MM` means on this host.
                guard.routines.set_offset(offset as i32);
            }
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

    /// `POST /api/routines/offset` with `{"offsetMinutes": n}`.
    fn set_routine_offset(&self, body: &str) -> Response {
        let parsed: Value = match serde_json::from_str(body) {
            Ok(value) => value,
            Err(_) => return Response::error(400, "invalid_json"),
        };
        let Some(offset) = parsed.get("offsetMinutes").and_then(Value::as_i64) else {
            return Response::error(400, "invalid_offset");
        };
        let changed = {
            let mut guard = self.lock_state();
            let changed = guard.routines.set_offset(offset as i32);
            if changed {
                guard.bump();
            }
            changed
        };
        if changed {
            self.save_state();
        }
        Response::json(
            200,
            json!({"ok": true, "changed": changed, "routines": self.lock_state().routines.document()}),
        )
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
        // A firing waits in the routine lane behind typed messages, so it gets
        // the same durable row a typed message has while it waits. The worker
        // replaces the row once the engine admits the turn.
        let queued_id = format!("queued-{}", request.request_id);
        {
            let mut guard = self.lock_state();
            guard.upsert_task(TaskRecord {
                id: queued_id.clone(),
                conversation_id: routine.conversation_id.clone().unwrap_or_default(),
                status: "queued".into(),
                owner: "agent".into(),
                prompt: routine.prompt.clone(),
                at: now_millis(),
                routine_id: routine.id.clone(),
            });
            guard.bump();
        }
        if self
            .turn_tx
            .send(TurnCommand {
                request,
                accepted: mpsc::channel().0,
            })
            .is_err()
        {
            self.forget_queued(&queued_id);
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

    /// `POST /api/routines/{id}/catch-up` with `{"enabled": bool}`.
    fn set_routine_catch_up(&self, id: &str, body: &str) -> Response {
        let parsed: Value = match serde_json::from_str(body) {
            Ok(value) => value,
            Err(_) => return Response::error(400, "invalid_json"),
        };
        let enabled = parsed
            .get("enabled")
            .and_then(Value::as_bool)
            .unwrap_or(true);
        let changed = {
            let mut guard = self.lock_state();
            let changed = guard.routines.set_catch_up(id, enabled);
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
            map.insert("recent".into(), json!(notify::recent_deliveries()));
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
        if let Some(retries) = parsed.get("retries").and_then(Value::as_u64) {
            settings.retries = retries.min(5) as u32;
        }
        if let Some(dedupe) = parsed.get("dedupeSeconds").and_then(Value::as_u64) {
            settings.dedupe_seconds = dedupe.min(86_400);
        }
        if let Some(re_alert) = parsed.get("reAlertSeconds").and_then(Value::as_u64) {
            settings.re_alert_seconds = re_alert.min(86_400);
        }
        if let Some(quiet) = parsed.get("quietHours") {
            if quiet.is_null() {
                settings.quiet_hours = notify::QuietHours::default();
            } else {
                let existing = settings.quiet_hours.clone();
                let minute = |name: &str, fallback: u16| {
                    quiet
                        .get(name)
                        .and_then(Value::as_u64)
                        .map(|value| (value % 1440) as u16)
                        .unwrap_or(fallback)
                };
                settings.quiet_hours = notify::QuietHours {
                    enabled: quiet
                        .get("enabled")
                        .and_then(Value::as_bool)
                        .unwrap_or(existing.enabled),
                    start_minute: minute("startMinute", existing.start_minute),
                    end_minute: minute("endMinute", existing.end_minute),
                    offset_minutes: quiet
                        .get("offsetMinutes")
                        .and_then(Value::as_i64)
                        .map(|value| value.clamp(-840, 840) as i32)
                        .unwrap_or(existing.offset_minutes),
                };
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

    /// `POST /api/routines/quiet` — hold scheduled routines inside a window.
    ///
    /// A routine exists to be useful at a time that suits the person, so the
    /// quiet window belongs to the routine document rather than to notification
    /// settings: a message that runs at 06:00 is not made acceptable by merely
    /// not pushing it. A skipped occurrence is counted, so a routine with
    /// catch-up runs once when the window ends.
    fn set_routine_quiet(&self, body: &str) -> Response {
        let parsed: Value = match serde_json::from_str(body) {
            Ok(value) => value,
            Err(_) => return Response::error(400, "invalid_json"),
        };
        let quiet = {
            let guard = self.lock_state();
            let existing = guard.routines.quiet_hours.clone();
            let minute = |name: &str, fallback: u16| {
                parsed
                    .get(name)
                    .and_then(Value::as_u64)
                    .map(|value| (value % 1440) as u16)
                    .unwrap_or(fallback)
            };
            notify::QuietHours {
                enabled: parsed
                    .get("enabled")
                    .and_then(Value::as_bool)
                    .unwrap_or(existing.enabled),
                start_minute: minute("startMinute", existing.start_minute),
                end_minute: minute("endMinute", existing.end_minute),
                offset_minutes: parsed
                    .get("offsetMinutes")
                    .and_then(Value::as_i64)
                    .map(|value| value.clamp(-840, 840) as i32)
                    .unwrap_or(existing.offset_minutes),
            }
        };
        let changed = {
            let mut guard = self.lock_state();
            let changed = guard.routines.set_quiet(quiet);
            if changed {
                guard.bump();
            }
            changed
        };
        if changed {
            self.save_state();
        }
        Response::json(
            200,
            json!({
                "ok": true,
                "changed": changed,
                "routines": self.lock_state().routines.document(),
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
        let flags = self.cancels.running();
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
        self.submit_text(conversation_id, text)
    }

    /// Hold a message while admission is paused, or hand it to the worker.
    fn submit_text(&self, conversation_id: Option<String>, text: String) -> Response {
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
        self.submit_request(TurnRequest::new(conversation_id, text))
    }

    /// Hand one message to the worker, answering as soon as it is admitted.
    ///
    /// The worker runs one turn at a time. A busy host used to leave the caller
    /// waiting until a timeout and then answer a gateway error, which was both
    /// slow and untrue: the message was never lost. Waiting a short bound keeps
    /// the immediate answer when the worker is free; when it is not, the `202`
    /// says where the message stands, and it is already in the worker's channel.
    fn submit_request(&self, request: TurnRequest) -> Response {
        let request_id = request.request_id.clone();
        let queued_id = format!("queued-{request_id}");
        let conversation_id = request.conversation_id.clone().unwrap_or_default();
        // A queued message is a durable row, not only a slot in the worker's
        // channel: otherwise `202 queued` is a promise the document does not
        // keep, and a restart would forget the message entirely. The worker
        // swaps this row for the real task once the engine issues an id.
        {
            let mut guard = self.lock_state();
            guard.upsert_task(TaskRecord {
                id: queued_id.clone(),
                conversation_id: conversation_id.clone(),
                status: "queued".into(),
                owner: "agent".into(),
                prompt: request.text.clone(),
                at: now_millis(),
                routine_id: request.routine_id.clone().unwrap_or_default(),
            });
            guard.bump();
        }
        self.save_state();
        let (accepted_tx, accepted_rx) = mpsc::channel();
        // A typed message joins the human lane, which the worker drains before
        // any routine firing, so its place in line counts messages only.
        let position = match self.turn_tx.send(TurnCommand {
            request,
            accepted: accepted_tx,
        }) {
            Ok(position) => position,
            Err(error) => {
                tracing::warn!(%error, "the turn worker is gone; dropping the message");
                self.forget_queued(&queued_id);
                return Response::error(503, "turn_worker_unavailable");
            }
        };
        self.events.publish(
            &conversation_id,
            &queued_id,
            "queued",
            json!({"taskId": queued_id, "requestId": request_id, "position": position}),
        );
        let accepted = match accepted_rx.recv_timeout(ADMISSION_WAIT) {
            Ok(Ok(accepted)) => accepted,
            Ok(Err(error)) => {
                self.forget_queued(&queued_id);
                return Response::json(
                    502,
                    json!({"error": "turn_failed", "message": error.to_string()}),
                );
            }
            Err(_) => {
                self.events.publish(
                    "",
                    "",
                    "queued",
                    json!({"requestId": request_id, "position": position}),
                );
                return Response::json(
                    202,
                    json!({
                        "queued": true,
                        "accepted": false,
                        "requestId": request_id,
                        "position": position,
                        "message": "the assistant is busy; this message keeps its place and runs next",
                    }),
                );
            }
        };
        Response::json(
            202,
            json!({
                "accepted": true,
                "queued": false,
                "requestId": request_id,
                "conversationId": accepted.conversation_id,
                "taskId": accepted.task_id,
                "position": position,
            }),
        )
    }

    /// Drop a queued placeholder once it can never become a turn.
    fn forget_queued(&self, queued_id: &str) {
        let removed = {
            let mut guard = self.lock_state();
            let before = guard.tasks.len();
            guard.tasks.retain(|task| task.id != queued_id);
            let removed = guard.tasks.len() != before;
            if removed {
                guard.bump();
            }
            removed
        };
        if removed {
            self.save_state();
        }
    }

    /// `POST /api/turns/retry` — run the last turn that did not finish.
    ///
    /// An interrupted or failed turn is the most likely thing a person wants to
    /// repeat, and retyping it is a poor way to recover. `taskId` picks one
    /// explicitly; otherwise the newest failed, cancelled, or interrupted turn
    /// is used, optionally scoped to `conversationId`.
    fn retry_turn(&self, body: &str) -> Response {
        let parsed: Value = serde_json::from_str(body).unwrap_or(Value::Null);
        let requested_task = parsed
            .get("taskId")
            .and_then(Value::as_str)
            .filter(|id| !id.is_empty());
        let requested_conversation = parsed
            .get("conversationId")
            .and_then(Value::as_str)
            .filter(|id| !id.is_empty());
        let retryable = |task: &crate::state::TaskRecord| {
            matches!(task.status.as_str(), "failed" | "cancelled" | "interrupted")
        };
        let task = {
            let guard = self.lock_state();
            match requested_task {
                Some(task_id) => guard
                    .tasks
                    .iter()
                    .find(|task| task.id == task_id && retryable(task))
                    .cloned(),
                None => guard
                    .tasks
                    .iter()
                    .rev()
                    .filter(|task| retryable(task))
                    .find(|task| requested_conversation.is_none_or(|id| task.conversation_id == id))
                    .cloned(),
            }
        };
        let Some(task) = task else {
            return Response::json(
                409,
                json!({
                    "error": "nothing_to_retry",
                    "message": "no failed, cancelled, or interrupted turn to retry",
                }),
            );
        };
        if task.prompt.trim().is_empty() {
            return Response::json(
                409,
                json!({ "error": "nothing_to_retry", "message": "that turn has no prompt to repeat" }),
            );
        }
        let retrying = task.id.clone();
        let mut response = self.submit_text(Some(task.conversation_id), task.prompt);
        // Name the turn being repeated, so a surface can say what it restarted.
        if let Ok(mut body) = serde_json::from_str::<Value>(&response.body)
            && let Value::Object(ref mut map) = body
        {
            map.insert("retrying".to_string(), json!(retrying));
            response.body = body.to_string();
        }
        response
    }

    /// `POST /api/turns/steer` — correct the turn that is running now.
    ///
    /// The request is matched against the durable task table before it reaches
    /// the worker, so a correction can never land on a different turn than the
    /// one the person was looking at.
    fn steer_turn(&self, body: &str) -> Response {
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
        let requested_conversation = parsed
            .get("conversationId")
            .and_then(Value::as_str)
            .filter(|id| !id.is_empty());
        let requested_task = parsed
            .get("taskId")
            .and_then(Value::as_str)
            .filter(|id| !id.is_empty());
        let target = {
            let guard = self.lock_state();
            let running = |task: &&crate::state::TaskRecord| task.status == "running";
            if let Some(task_id) = requested_task {
                guard
                    .tasks
                    .iter()
                    .find(|task| task.id == task_id && task.status == "running")
                    .map(|task| (task.conversation_id.clone(), task.id.clone()))
            } else if let Some(conversation_id) = requested_conversation {
                guard
                    .tasks
                    .iter()
                    .rev()
                    .find(running)
                    .filter(|task| task.conversation_id == conversation_id)
                    .map(|task| (task.conversation_id.clone(), task.id.clone()))
            } else {
                guard
                    .tasks
                    .iter()
                    .rev()
                    .find(running)
                    .map(|task| (task.conversation_id.clone(), task.id.clone()))
            }
        };
        let Some((conversation_id, task_id)) = target else {
            return Response::json(
                409,
                json!({
                    "error": "no_running_turn",
                    "message": "there is no running turn to correct",
                }),
            );
        };
        let request = SteerRequest {
            conversation_id: conversation_id.clone(),
            task_id: task_id.clone(),
            text: text.clone(),
        };
        if self.steer_tx.send(request).is_err() {
            return Response::error(503, "turn_worker_unavailable");
        }
        self.events.publish(
            &conversation_id,
            &task_id,
            "steer_queued",
            json!({"text": text, "taskId": task_id}),
        );
        Response::json(
            202,
            json!({
                "steered": true,
                "conversationId": conversation_id,
                "taskId": task_id,
                "text": text,
            }),
        )
    }

    /// `POST /api/tasks/{id}/cancel` — stop one running turn.
    ///
    /// The worker owns the cancel table, so every turn it runs is cancellable:
    /// a typed message, a routine firing on its own clock, and one a resume
    /// dispatched all answer here. A task that already settled is reported as
    /// such rather than as unknown.
    fn cancel_task(&self, task_id: &str) -> Response {
        if self.cancels.request(task_id) {
            self.events
                .publish("", task_id, "cancel_requested", json!({"taskId": task_id}));
            self.save_state();
            return Response::json(202, json!({"cancelRequested": true, "taskId": task_id}));
        }
        let known = {
            let guard = self.lock_state();
            guard
                .tasks
                .iter()
                .find(|task| task.id == task_id)
                .map(|task| task.status.clone())
        };
        match known.as_deref() {
            Some("queued" | "running") => Response::json(
                409,
                json!({
                    "error": "turn_not_running",
                    "message": "this turn is not on the engine that is running now, so there is nothing to stop",
                    "taskId": task_id,
                }),
            ),
            Some(status) => Response::json(
                409,
                json!({
                    "error": "turn_settled",
                    "message": format!("that turn already finished as {status}"),
                    "taskId": task_id,
                }),
            ),
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
        // Persist straight from the borrowed document: a full clone per mutation
        // is the most expensive part of a write that is otherwise bounded.
        let guard = self.lock_state();
        crate::store::persist(store, &guard)
    }

    pub fn serve(self) -> std::io::Result<()> {
        let server = Arc::new(self);
        let active = Arc::new(AtomicUsize::new(0));
        loop {
            let next = server.listener.incoming().next();
            let stream = match next {
                Some(Ok(stream)) => stream,
                Some(Err(error)) => return Err(error),
                None => break,
            };
            if active.fetch_add(1, Ordering::SeqCst) >= MAX_CONNECTIONS {
                active.fetch_sub(1, Ordering::SeqCst);
                // Refusing here is honest: accepting the socket and queueing it
                // would only move the queue into the kernel and hold a thread.
                let mut stream = stream;
                let _ = write_response(
                    &mut stream,
                    Response::json(
                        503,
                        json!({
                            "error": "busy",
                            "message": format!(
                                "this instance serves at most {MAX_CONNECTIONS} connections at once"
                            ),
                        }),
                    ),
                    false,
                );
                continue;
            }
            let guard = ConnectionGuard {
                active: Arc::clone(&active),
            };
            let server = Arc::clone(&server);
            std::thread::spawn(move || {
                let _guard = guard;
                let _ = server.serve_connection(stream);
            });
        }
        Ok(())
    }

    fn serve_connection(&self, mut stream: TcpStream) -> std::io::Result<()> {
        let _ = stream.set_read_timeout(Some(Duration::from_secs(30)));
        let _ = stream.set_write_timeout(Some(Duration::from_secs(30)));
        let peer = stream
            .peer_addr()
            .map(|address| address.ip().to_string())
            .unwrap_or_default();
        // Read through a clone so the buffer survives across requests while the
        // responses are written to the original stream.
        let mut reader = BufReader::new(stream.try_clone()?);
        let mut served = 0usize;
        loop {
            let head_bytes = read_head(&mut reader)?;
            if head_bytes.is_empty() {
                return Ok(());
            }
            served += 1;
            let head_text = String::from_utf8_lossy(&head_bytes).to_string();
            let head = parse_head(&head_text);
            let content_length = head
                .header("content-length")
                .and_then(|value| value.trim().parse::<usize>().ok())
                .unwrap_or(0);
            if content_length > MAX_BODY_BYTES {
                return write_response(&mut stream, Response::error(413, "body_too_large"), false);
            }
            let mut body = vec![0u8; content_length];
            if content_length > 0 {
                reader.read_exact(&mut body)?;
            }
            let body = String::from_utf8_lossy(&body).to_string();
            if head.method == "GET" && head.path() == "/api/events" {
                if !self.policy.authorize(&head) {
                    return write_response(
                        &mut stream,
                        Response::error(401, "unauthorized"),
                        false,
                    );
                }
                return self.stream_events(&head, &mut stream);
            }
            let response = self.handle_request_from(&head, &body, &peer);
            let keep_alive = wants_keep_alive(&head)
                && response.status < 400
                && served < MAX_REQUESTS_PER_CONNECTION;
            write_response(&mut stream, response, keep_alive)?;
            if !keep_alive {
                return Ok(());
            }
        }
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

/// One accepted connection, counted until its thread finishes.
struct ConnectionGuard {
    active: Arc<AtomicUsize>,
}

impl Drop for ConnectionGuard {
    fn drop(&mut self) {
        self.active.fetch_sub(1, Ordering::SeqCst);
    }
}

/// Whether one request asked for the connection to stay open.
///
/// HTTP/1.1 keeps connections alive unless told otherwise; a client that sends
/// `Connection: close` gets exactly that.
fn wants_keep_alive(head: &RequestHead) -> bool {
    match head.header("connection") {
        Some(value) => value
            .split(',')
            .any(|token| token.trim().eq_ignore_ascii_case("keep-alive")),
        None => true,
    }
}

/// Whether the caller already holds this exact representation.
fn etag_matches(head: &RequestHead, etag: &str) -> bool {
    head.header("if-none-match")
        .map(|value| {
            value.split(',').any(|candidate| {
                let candidate = candidate.trim();
                candidate == "*" || candidate == etag || candidate.strip_prefix("W/") == Some(etag)
            })
        })
        .unwrap_or(false)
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

fn write_response(
    stream: &mut TcpStream,
    response: Response,
    keep_alive: bool,
) -> std::io::Result<()> {
    let mut head = format!(
        "HTTP/1.1 {} {}\r\nConnection: {}\r\n",
        response.status,
        response.reason(),
        if keep_alive { "keep-alive" } else { "close" }
    );
    // A 304 carries no body, so it must not claim a length for one.
    if response.status != 304 {
        head.push_str(&format!("Content-Length: {}\r\n", response.body.len()));
    }
    for (name, value) in &response.headers {
        head.push_str(&format!("{name}: {value}\r\n"));
    }
    head.push_str("\r\n");
    stream.write_all(head.as_bytes())?;
    if response.status != 304 {
        stream.write_all(response.body.as_bytes())?;
    }
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
