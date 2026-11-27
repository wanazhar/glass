//! Self-hosted HTTP surface for the Glass marble product.
//!
//! Bound to loopback by default; a Tailscale address needs an explicit host and
//! an access token. The surface is intentionally small: state, a turn intake,
//! cancellation, approvals, and one server-sent event stream.

use crate::Approval;
use crate::control::ControlProfile;
use crate::conversation::{ApprovalInbox, SteerRequest};
use crate::crates::CrateMap;
use crate::events::{EventBus, TurnEvent};
use crate::memory::{self, MemoryCommand};
use crate::notify::{self, ApnsSettings, Notification};
use crate::prism::{DispatchRefusal, PrismMarble, PrismPool};
use crate::state::{PendingApproval, ProductState, TaskRecord};
use crate::store::StateStore;
use crate::voice;
use crate::worker::{CancelRegistry, SteerSink, TurnCommand, TurnRequest, TurnSender, now_millis};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::io::{BufRead, Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use std::time::Duration;

pub const STATIC_HTML: &str = include_str!("static/index.html");
const STATIC_CSS: &str = include_str!("static/app.css");
const STATIC_JS: &str = include_str!("static/app.js");
const STATIC_LOGIC: &str = include_str!("static/logic.js");
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

/// Percent-decoding for a path segment: no `+` handling, which is a query-only
/// form convention. Approval request ids carry `:` and clients send it escaped.
fn decode_path_segment(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%'
            && index + 2 < bytes.len()
            && let Ok(byte) = u8::from_str_radix(&value[index + 1..index + 3], 16)
        {
            decoded.push(byte);
            index += 3;
            continue;
        }
        decoded.push(bytes[index]);
        index += 1;
    }
    String::from_utf8_lossy(&decoded).into_owned()
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

    /// The configured token, for the tls-proof HMAC; never logged or served raw.
    pub fn token(&self) -> Option<&str> {
        self.token.as_deref()
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

/// Which credential channel a request carried. The log records the channel,
/// never the credential.
fn auth_kind(head: &RequestHead) -> &'static str {
    if head.header("authorization").is_some() {
        "bearer"
    } else if head.header("x-marble-token").is_some() {
        "x-marble-token"
    } else if head.cookie(SESSION_COOKIE).is_some() {
        "cookie"
    } else if head.query_param("token").is_some() {
        "query"
    } else {
        "none"
    }
}

/// One request/response line for the service log. The path is logged without
/// its query string because the query may carry a `token=` credential, and the
/// user agent is the only header value that may appear here.
struct RequestLog<'a> {
    peer: &'a str,
    method: &'a str,
    path: &'a str,
    status: u16,
    auth: &'a str,
    tls: bool,
    authorized: bool,
    head_bytes: usize,
    content_length: usize,
    user_agent: &'a str,
    elapsed: Duration,
}

impl RequestLog<'_> {
    fn line(&self) -> String {
        let agent: String = self.user_agent.chars().take(60).collect();
        format!(
            "request peer={} method={} path={} status={} auth={} tls={} authorized={} \
             head={}B body={}B ua=\"{}\" ms={}",
            self.peer,
            printable(self.method),
            printable(self.path),
            self.status,
            self.auth,
            if self.tls { "yes" } else { "no" },
            if self.authorized { "yes" } else { "no" },
            self.head_bytes,
            self.content_length,
            agent,
            self.elapsed.as_millis()
        )
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
    client_send_lock: Mutex<()>,
    listener: TcpListener,
    state: Arc<Mutex<ProductState>>,
    store: Option<Arc<StateStore>>,
    approvals: ApprovalInbox,
    events: EventBus,
    turn_tx: TurnSender,
    /// Corrections routed to the engine lane that owns each conversation.
    steer: SteerSink,
    policy: AccessPolicy,
    cancels: CancelRegistry,
    login: LoginThrottle,
    /// The Prism roster and its engines, when the host serves one.
    prism: Option<Arc<PrismPool>>,
    /// The workspace crate map, when the host was started inside one.
    crates: Option<CrateMap>,
    /// The TLS identity, when the operator serves https on the same port.
    tls: Option<Arc<crate::tls::TlsIdentity>>,
}

fn parse_agent_card(parsed: &Value) -> Result<Option<crate::prism::AgentCard>, Response> {
    let Some(value) = parsed.get("card") else {
        return Ok(None);
    };
    let invalid = |message: String| {
        Response::json(400, json!({"error": "invalid_marble", "message": message}))
    };
    let mut card: crate::prism::AgentCard =
        serde_json::from_value(value.clone()).map_err(|error| invalid(error.to_string()))?;
    card.validate().map_err(invalid)?;
    Ok(Some(card))
}

fn valid_device_time_zone(zone: &str) -> bool {
    zone.len() <= 64
        && zone.split('/').all(|part| {
            !part.is_empty()
                && part
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'+'))
        })
        && std::fs::File::open(std::path::Path::new("/usr/share/zoneinfo").join(zone)).is_ok_and(
            |mut file| {
                let mut signature = [0; 4];
                file.read_exact(&mut signature).is_ok() && &signature == b"TZif"
            },
        )
}

impl MarbleServer {
    pub fn new(parts: MarbleServerParts) -> Self {
        {
            let mut state = parts.state.lock().unwrap_or_else(|e| e.into_inner());
            state.ensure_entry_ids();
            crate::turn::separate_stored_reasoning(&mut state);
            state.prune_threads();
            parts.events.attach_replies(&mut state);
            for (task, thread) in &state.task_threads {
                parts.events.bind_thread(task, thread);
            }
        }
        Self {
            client_send_lock: Mutex::new(()),
            listener: parts.listener,
            state: parts.state,
            store: parts.store,
            approvals: parts.approvals,
            events: parts.events,
            turn_tx: parts.turn_tx,
            steer: SteerSink::single(parts.steer_tx),
            policy: parts.policy,
            cancels: parts.cancels,
            login: LoginThrottle::default(),
            prism: None,
            crates: None,
            tls: None,
        }
    }

    /// Serve https on the same port next to plain http.
    pub fn with_tls(mut self, tls: Arc<crate::tls::TlsIdentity>) -> Self {
        self.tls = Some(tls);
        self
    }

    /// Attach the Prism pool. Sessions that never swarm keep the roster read-only.
    pub fn with_prism(mut self, prism: Arc<PrismPool>) -> Self {
        self.prism = Some(prism);
        self
    }

    /// Attach the workspace crate map served at `/api/crates`.
    pub fn with_crates(mut self, crates: CrateMap) -> Self {
        self.crates = Some(crates);
        self
    }

    /// Route corrections through the engine lanes instead of one channel.
    pub fn with_steer_sink(mut self, steer: SteerSink) -> Self {
        self.steer = steer;
        self
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
                .with_header("Cache-Control", "no-cache")
                .with_header("X-Content-Type-Options", "nosniff")
                .with_header("Referrer-Policy", "no-referrer")
                .with_header(
                    "Content-Security-Policy",
                    "default-src 'self'; img-src 'self' data:; style-src 'self' 'unsafe-inline'; \
                     script-src 'self' 'unsafe-inline'; connect-src 'self'; frame-ancestors 'none'",
                );
        }
        if head.method == "GET" {
            let asset = match path {
                "/app.css" => Some((STATIC_CSS, "text/css; charset=utf-8")),
                "/app.js" => Some((STATIC_JS, "text/javascript; charset=utf-8")),
                "/logic.js" => Some((STATIC_LOGIC, "text/javascript; charset=utf-8")),
                _ => None,
            };
            if let Some((body, content_type)) = asset {
                return Response {
                    status: 200,
                    body: body.into(),
                    headers: vec![("Content-Type".into(), content_type.into())],
                }
                .with_header("Cache-Control", "no-cache")
                .with_header("X-Content-Type-Options", "nosniff");
            }
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
                    "build": crate::build_info::document(),
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
        if head.method == "POST" && path == "/api/logout" {
            if !head.same_origin() {
                return Response::error(403, "cross_origin_request");
            }
            return Response::json(200, json!({"ok": true})).with_header(
                "Set-Cookie",
                "marble_token=; Path=/; HttpOnly; SameSite=Strict; Max-Age=0",
            );
        }
        if !self.policy.authorize(head) {
            return Response::error(401, "unauthorized");
        }
        if head.method == "POST" && !head.same_origin() {
            return Response::error(403, "cross_origin_request");
        }
        let device_time_zone = head
            .header("x-marble-time-zone")
            .filter(|zone| valid_device_time_zone(zone));
        if head.method == "GET" && path == "/api/state" {
            let lean = head.query_param("lean").as_deref() == Some("1")
                || head.query_param("transcript").as_deref() == Some("0");
            return self.state_response(head, lean);
        }
        if path == "/api/threads" {
            return self.threads_route(head, body);
        }
        if head.method == "POST"
            && let Some(rest) = path.strip_prefix("/api/threads/")
            && let Some((id, action)) = rest.split_once('/')
        {
            return self.update_everyone_thread(&decode_path_segment(id), action, body);
        }
        if head.method == "GET"
            && let Some(id) = path.strip_prefix("/api/threads/")
        {
            let state = self.lock_state();
            return match state
                .threads
                .iter()
                .find(|t| t.id == decode_path_segment(id))
            {
                Some(thread) => match history_page(state.thread_entry_refs(thread), head, None) {
                    Ok(page) => Response::json(
                        200,
                        json!({"thread":thread,"entries":page.entries,"hasMore":page.has_more,"nextBefore":page.next_before}),
                    ),
                    Err(response) => response,
                },
                None => Response::error(404, "unknown_thread"),
            };
        }
        if head.method == "GET" && path == "/api/search" {
            let query = head.query_param("q").unwrap_or_default();
            let thread = head.query_param("thread_id");
            let limit = head
                .query_param("limit")
                .and_then(|s| s.parse().ok())
                .unwrap_or(30);
            return Response::json(
                200,
                json!({"hits":self.lock_state().search_messages(&query, thread.as_deref(), limit)}),
            );
        }
        if head.method == "GET" && path == "/api/transcript/search" {
            return self.search_transcript(head);
        }
        if head.method == "GET" && path == "/api/transcript" {
            return self.transcript_response(head);
        }
        if head.method == "POST" && path == "/api/conversations" {
            return self.create_conversation(body);
        }
        if head.method == "POST"
            && let Some(rest) = path.strip_prefix("/api/conversations/")
        {
            if let Some(id) = rest.strip_suffix("/rename") {
                return self.rename_conversation(&decode_path_segment(id), body);
            }
            if let Some(id) = rest.strip_suffix("/delete") {
                return self.delete_conversation(&decode_path_segment(id));
            }
            if let Some((id, action @ ("archive" | "unarchive"))) = rest.split_once('/') {
                return self.archive_conversation(&decode_path_segment(id), action == "archive");
            }
            if let Some(id) = rest.strip_suffix("/restore") {
                return self.restore_deleted_chat(&decode_path_segment(id));
            }
        }
        if head.method == "GET" && path == "/api/events" {
            return Response::error(400, "stream_requires_socket");
        }
        if head.method == "POST" && path == "/api/model" {
            return self.set_model(body);
        }
        if head.method == "POST" && path == "/api/turns" {
            return self.client_send(path, body, device_time_zone);
        }
        if head.method == "POST" && path == "/api/turns/steer" {
            return self.steer_turn(body);
        }
        if head.method == "POST" && path == "/api/turns/retry" {
            return self.retry_turn(body, device_time_zone);
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
            return self.revoke_grant(&decode_path_segment(grant_id));
        }
        if head.method == "GET" && path == "/api/export" {
            return self.export_response(head);
        }
        if head.method == "GET" && path == "/api/presence" {
            return self.presence_response();
        }
        if head.method == "GET" && path == "/api/tools" {
            return self.tools_response();
        }
        if head.method == "GET" && path == "/api/crates" {
            return self.crates_response();
        }
        if head.method == "GET" && path == "/api/prism" {
            return self.prism_response();
        }
        if head.method == "POST" && path == "/api/prism/marbles" {
            return self.add_marble(body);
        }
        if head.method == "POST" && path == "/api/everyone" {
            return self.client_send(path, body, device_time_zone);
        }
        if head.method == "POST" && path == "/api/everyone/clear" {
            return self.clear_everyone();
        }
        if head.method == "GET" && path == "/api/everyone" {
            return self.everyone_timeline(head);
        }
        if head.method == "POST" && path == "/api/prism/dispatch" {
            return self.client_send(path, body, device_time_zone);
        }
        if head.method == "POST"
            && let Some(rest) = path.strip_prefix("/api/prism/marbles/")
        {
            if let Some(id) = rest.strip_suffix("/update") {
                return self.update_marble(&decode_path_segment(id), body);
            }
            if let Some(id) = rest.strip_suffix("/remove") {
                return self.remove_marble(&decode_path_segment(id));
            }
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
                return self.forget_memory(&decode_path_segment(id));
            }
            if let Some(id) = rest.strip_suffix("/update") {
                return self.update_memory(&decode_path_segment(id), body);
            }
            if let Some(id) = rest.strip_suffix("/accept") {
                return self.accept_suggestion(&decode_path_segment(id));
            }
            if let Some(id) = rest.strip_suffix("/dismiss") {
                return self.dismiss_suggestion(&decode_path_segment(id));
            }
        }
        if head.method == "POST"
            && let Some((id, action)) = path
                .strip_prefix("/api/knowledge/")
                .and_then(|path| path.split_once('/'))
        {
            return self.update_knowledge(&decode_path_segment(id), action, body);
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
                return self.run_routine(&decode_path_segment(id));
            }
            if let Some(id) = rest.strip_suffix("/enable") {
                return self.set_routine_enabled(&decode_path_segment(id), true);
            }
            if let Some(id) = rest.strip_suffix("/disable") {
                return self.set_routine_enabled(&decode_path_segment(id), false);
            }
            if let Some(id) = rest.strip_suffix("/remove") {
                return self.remove_routine(&decode_path_segment(id));
            }
            if let Some(id) = rest.strip_suffix("/catch-up") {
                return self.set_routine_catch_up(&decode_path_segment(id), body);
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
            return self.remove_device(&decode_path_segment(token));
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
            return self.cancel_task(&decode_path_segment(task_id));
        }
        if head.method == "POST"
            && let Some(rest) = path.strip_prefix("/api/approvals/")
            && let Some((request_id, action)) = rest.rsplit_once('/')
        {
            return self.decide_approval(&decode_path_segment(request_id), action, body);
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

    /// `GET /api/tools` — everything this marble can reach for.
    ///
    /// Two owners, one document. Core executes its own coding tools inside the
    /// session; Marble executes the `glass.*` integrations and classifies each
    /// one through the control layer. A surface that shows what an assistant can
    /// do has to show both, and say which one asked permission first.
    fn tools_response(&self) -> Response {
        let (profile, observed) = {
            let guard = self.lock_state();
            (guard.control.profile, guard.engine_tools.clone())
        };
        let governed = crate::tools::document();
        let governed_count = governed.as_array().map(Vec::len).unwrap_or_default();
        Response::json(
            200,
            json!({
                "protocol": "mixed",
                "profile": profile.as_str(),
                "core": {
                    "owner": "core",
                    "nativeTools": true,
                    "observed": observed,
                    "note": "Core runs these inside the session; Marble shows them but does not approve them.",
                },
                "governed": governed,
                "counts": {
                    "governed": governed_count,
                    "observed": observed.len(),
                },
            }),
        )
    }

    /// `GET /api/crates` — the workspace's crates and how this host reaches them.
    fn crates_response(&self) -> Response {
        match &self.crates {
            Some(crates) => Response::json(200, crates.document()),
            // A host started outside a Rust workspace has no map to serve, and
            // saying so beats pretending every workspace has one.
            None => Response::json(
                503,
                json!({
                    "error": "crate_map_unavailable",
                    "message": "this host was not started with a workspace crate map",
                }),
            ),
        }
    }

    /// `GET /api/prism` — the marble roster and what it has done.
    fn prism_response(&self) -> Response {
        let mut document = { self.lock_state().prism_document() };
        if let Value::Object(map) = &mut document {
            map.insert("engine".into(), json!(self.prism.is_some()));
        }
        Response::json(200, document)
    }

    /// `POST /api/prism/marbles` — add one named marble to the roster.
    fn add_marble(&self, body: &str) -> Response {
        let parsed: Value = match serde_json::from_str(body) {
            Ok(value) => value,
            Err(_) => return Response::error(400, "invalid_json"),
        };
        let field = |key: &str| {
            parsed
                .get(key)
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string()
        };
        let hue = parsed.get("hue").and_then(Value::as_u64).unwrap_or(0) as u16;
        let marble = {
            let mut guard = self.lock_state();
            let source = if let Some(id) = parsed.get("duplicateId").and_then(Value::as_str) {
                let source = if id == "marble" {
                    guard.prism.default_marble()
                } else if let Some(agent) = guard.prism.get(id) {
                    agent.clone()
                } else {
                    return Response::error(404, "unknown_marble");
                };
                Some(source)
            } else {
                None
            };
            let card = match parse_agent_card(&parsed) {
                Ok(card) => card,
                Err(response) => return response,
            };
            let (role, glyph, hue, workspace, persona, mut card) = match source {
                Some(source) => (
                    source.role,
                    source.glyph,
                    source.hue,
                    String::new(),
                    source.persona,
                    source.card.unwrap_or_default(),
                ),
                None => (
                    field("role"),
                    field("glyph"),
                    hue,
                    field("workspace"),
                    field("persona"),
                    card.unwrap_or_default(),
                ),
            };
            card.in_everyone = false;
            let added =
                match guard
                    .prism
                    .add(&field("name"), &role, &glyph, hue, &workspace, &persona)
                {
                    Ok(marble) => marble.clone(),
                    Err(message) => {
                        return Response::json(
                            400,
                            json!({"error": "invalid_marble", "message": message}),
                        );
                    }
                };
            let added = guard.prism.find_mut(&added.id).expect("just added");
            added.card = Some(card);
            let added = added.clone();
            guard.bump();
            added
        };
        self.save_state();
        self.events
            .publish("", "", "prism", json!({"marble": marble.id, "added": true}));
        let roster = { self.lock_state().prism_document() };
        Response::json(202, json!({"ok": true, "marble": marble, "prism": roster}))
    }

    /// `POST /api/prism/marbles/{id}/update` — rename, re-role, or move a marble.
    fn update_marble(&self, id: &str, body: &str) -> Response {
        let parsed: Value = match serde_json::from_str(body) {
            Ok(value) => value,
            Err(_) => return Response::error(400, "invalid_json"),
        };
        let text = |key: &str| parsed.get(key).and_then(Value::as_str);
        let hue = parsed
            .get("hue")
            .and_then(Value::as_u64)
            .map(|hue| hue as u16);
        let card = match parse_agent_card(&parsed) {
            Ok(card) => card,
            Err(response) => return response,
        };
        // A persona (or a rename) only reaches the model when the next
        // conversation starts, so an identity change drops the marble's engine
        // and its conversation id. The transcript rows stay — the old chat is
        // still readable.
        let outcome = {
            let mut guard = self.lock_state();
            match guard.prism.update(
                id,
                text("name"),
                text("role"),
                text("glyph"),
                hue,
                text("workspace"),
                text("persona"),
            ) {
                Ok(identity_changed) => {
                    if let Some(card) = card {
                        if id == "marble" {
                            guard.prism.default_card = Some(card);
                        } else if let Some(agent) = guard.prism.find_mut(id) {
                            agent.card = Some(card);
                        }
                    }
                    if identity_changed {
                        guard.prism.restart_conversation(id);
                    }
                    guard.bump();
                    let agent = if id == "marble" {
                        Some(guard.prism.default_marble())
                    } else {
                        guard.prism.get(id).cloned()
                    };
                    Ok((agent, identity_changed))
                }
                Err(message) => Err(message),
            }
        };
        match outcome {
            Ok((marble, identity_changed)) => {
                if identity_changed && let Some(pool) = &self.prism {
                    pool.shutdown_marble(id);
                }
                self.save_state();
                let roster = { self.lock_state().prism_document() };
                Response::json(200, json!({"ok": true, "marble": marble, "prism": roster}))
            }
            Err(message) => Response::json(
                if message == "unknown marble" {
                    404
                } else {
                    400
                },
                json!({ "error": "invalid_marble", "message": message }),
            ),
        }
    }

    /// `POST /api/prism/marbles/{id}/remove` — take a marble off the roster.
    ///
    /// Remove its registered chat and recovered history targeting it.
    fn remove_marble(&self, id: &str) -> Response {
        if let Some(pool) = &self.prism {
            pool.shutdown_marble(id);
        }
        let removed = {
            let mut guard = self.lock_state();
            match guard.prism.remove(id) {
                Ok(conversation) => {
                    for group in guard
                        .threads
                        .iter_mut()
                        .filter_map(|thread| thread.group.as_mut())
                    {
                        group.members.retain(|member| member != id);
                        group
                            .suggestions
                            .retain(|suggestion| suggestion.agent_id != id);
                    }
                    let conversations: std::collections::HashSet<String> = guard
                        .tasks
                        .iter()
                        .filter(|task| task.marble_id == id)
                        .map(|task| task.conversation_id.clone())
                        .chain(std::iter::once(conversation.clone()))
                        .filter(|id| !id.is_empty() && !crate::everyone::is_group_session(id))
                        .collect();
                    let mut deleted = Vec::new();
                    // Its chat, tasks, and the Everyone lines quoting them go
                    // with the marble — "Remove Ada and her chat" means gone,
                    // not just off the roster. A conversation that refuses
                    // (a turn still running) is left readable rather than
                    // torn down mid-turn.
                    for candidate in conversations {
                        if self
                            .delete_conversation_record(&mut guard, &candidate)
                            .is_ok()
                        {
                            deleted.push(candidate);
                        } else {
                            tracing::info!(
                                marble = %id,
                                conversation = %candidate,
                                "removed a marble whose chat could not be deleted; chat kept"
                            );
                        }
                    }
                    guard.bump();
                    Ok((conversation, deleted))
                }
                Err(message) => Err(message),
            }
        };
        match removed {
            Ok((conversation, deleted)) => {
                if let Some(store) = &self.store {
                    let live = crate::live::LiveStore::for_store(store);
                    for conversation in deleted {
                        let removed = live.remove_conversation(&conversation);
                        if removed > 0 {
                            tracing::info!(removed, conversation = %conversation, "cleared live journal for a removed marble");
                        }
                    }
                }
                self.save_state();
                self.events.publish(
                    "",
                    "",
                    "prism",
                    json!({"marble": id, "removed": true, "conversationId": conversation}),
                );
                Response::json(200, json!({"ok": true, "removed": id}))
            }
            Err(message) => {
                Response::json(404, json!({"error": "unknown_marble", "message": message}))
            }
        }
    }

    /// `POST /api/everyone/clear` — empty the shared timeline. The plane
    /// itself stays; only what was said in it goes.
    fn clear_everyone(&self) -> Response {
        let removed = {
            let mut guard = self.lock_state();
            let records: Vec<_> = guard
                .conversations
                .iter()
                .filter(|record| {
                    crate::everyone::is_session(&record.id)
                        && !crate::everyone::is_group_session(&record.id)
                })
                .map(|record| record.id.clone())
                .collect();
            if records
                .iter()
                .any(|id| guard.check_conversation_deletion(id).is_err())
                || guard.transcript.iter().any(|entry| {
                    entry.conversation_id == crate::everyone::EVERYONE_CONVERSATION
                        && entry.kind == "pending"
                })
            {
                return Response::error(409, "conversation_busy");
            }
            let removed = guard
                .transcript
                .iter()
                .filter(|entry| entry.conversation_id == crate::everyone::EVERYONE_CONVERSATION)
                .count();
            let mut cleared = guard.clone();
            cleared.thread_events = Default::default();
            if self.store.is_some() {
                cleared
                    .pending_everyone_cleanup
                    .extend(records.iter().cloned());
            }
            for id in &records {
                if cleared.delete_conversation(id).is_err() {
                    return Response::error(409, "conversation_busy");
                }
            }
            cleared.clear_everyone();
            cleared.bump();
            if let Some(store) = &self.store
                && let Err(error) = store.save(&cleared)
            {
                tracing::warn!(%error, "could not persist Clear Everyone");
                return Response::error(503, "state_save_failed");
            }
            cleared.thread_events = guard.thread_events.clone();
            if let Some(events) = &cleared.thread_events.0 {
                for task in guard.task_threads.keys() {
                    if !cleared.task_threads.contains_key(task) {
                        events.unbind_thread(task);
                    }
                }
                events.publish_replies(&cleared);
            }
            *guard = cleared;
            if let Some(store) = &self.store {
                store.retry_everyone_cleanup(&mut guard);
            }
            removed
        };
        self.events.publish(
            crate::everyone::EVERYONE_CONVERSATION,
            "",
            "everyone",
            json!({"cleared": removed}),
        );
        Response::json(200, json!({"ok": true, "cleared": removed}))
    }

    /// `GET /api/everyone?after=` — the shared timeline, newest entries after a
    /// timestamp so a surface can poll without re-reading the whole plane.
    fn everyone_timeline(&self, head: &RequestHead) -> Response {
        let after: u64 = head
            .query_param("after")
            .and_then(|value| value.parse().ok())
            .unwrap_or(0);
        let main = head.query_param("main").as_deref() == Some("1");
        let guard = self.lock_state();
        let entries = guard
            .transcript
            .iter()
            .filter(|entry| {
                entry.conversation_id == crate::everyone::EVERYONE_CONVERSATION
                    && entry.at > after
                    && (!main || entry.thread_id.is_none())
            })
            .collect();
        let page = match history_page(entries, head, None) {
            Ok(page) => page,
            Err(response) => return response,
        };
        Response::json(
            200,
            json!({"ok": true, "entries": page.entries, "hasMore":page.has_more,"nextBefore":page.next_before}),
        )
    }

    fn client_send(&self, path: &str, body: &str, device_time_zone: Option<&str>) -> Response {
        let send = |client| match path {
            "/api/everyone" => self.post_everyone(body, client, device_time_zone),
            "/api/prism/dispatch" => self.dispatch_swarm(body, client, device_time_zone),
            _ => self.handle_turn(body, client, device_time_zone),
        };
        let parsed: Value = match serde_json::from_str(body) {
            Ok(value) => value,
            Err(_) => return Response::error(400, "invalid_json"),
        };
        let Some(value) = parsed
            .get("client_message_id")
            .or_else(|| parsed.get("clientMessageId"))
        else {
            return send(None);
        };
        let Some(id) = value.as_str().filter(|id| {
            !id.is_empty() && id.len() <= 128 && id.bytes().all(|byte| (32..=126).contains(&byte))
        }) else {
            return Response::error(400, "invalid_client_message_id");
        };
        if parsed
            .get("client_message_id")
            .zip(parsed.get("clientMessageId"))
            .is_some_and(|(a, b)| a != b)
        {
            return Response::error(400, "invalid_client_message_id");
        }
        let text = parsed
            .get("text")
            .and_then(Value::as_str)
            .map(str::trim)
            .unwrap_or_default();
        if text.is_empty() {
            return Response::error(400, "empty_message");
        }
        if text.len() > 64 * 1024 {
            return Response::error(413, "message_too_large");
        }
        let _serial = self
            .client_send_lock
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        let scope = {
            let state = self.lock_state();
            if path == "/api/everyone" {
                "everyone".to_string()
            } else if let Some(thread) = parsed
                .get("thread_id")
                .or_else(|| parsed.get("threadId"))
                .and_then(Value::as_str)
                .and_then(|id| {
                    state
                        .threads
                        .iter()
                        .find(|thread| thread.id == id && thread.group.is_some())
                })
            {
                thread.id.clone()
            } else if let Some(cv) = parsed
                .get("conversationId")
                .and_then(Value::as_str)
                .filter(|cv| !cv.is_empty())
            {
                cv.to_string()
            } else if let Some(thread) = parsed
                .get("thread_id")
                .or_else(|| parsed.get("threadId"))
                .and_then(Value::as_str)
                .and_then(|id| state.threads.iter().find(|thread| thread.id == id))
            {
                thread.conversation_id.clone()
            } else {
                let marble = parsed.get("marbleId").and_then(Value::as_str).or_else(|| {
                    parsed
                        .get("marbles")
                        .and_then(Value::as_array)
                        .and_then(|ids| ids.first())
                        .and_then(Value::as_str)
                });
                match marble {
                    Some(id) => state
                        .prism
                        .get(id)
                        .map(|marble| marble.conversation_id.clone())
                        .filter(|cv| !cv.is_empty())
                        .unwrap_or_else(|| format!("marble:{id}")),
                    None => "new:marble".into(),
                }
            }
        };
        {
            let state = self.lock_state();
            if let Some(previous) = state
                .client_sends
                .iter()
                .find(|send| send.message.id == id && send.scopes.contains(&scope))
            {
                return match &previous.response {
                    Some(response) => Response::json(previous.status, response.clone()),
                    None => Response::error(409, "client_message_pending"),
                };
            }
        }
        let client = crate::state::ClientMessage {
            id: id.into(),
            entry_id: format!("entry-{}", crate::next_request_id()),
        };
        {
            let mut state = self.lock_state();
            state.client_sends.push(crate::state::ClientSend {
                message: client.clone(),
                scopes: vec![scope.clone()],
                status: 202,
                response: None,
            });
            state.bump();
            if let Some(store) = &self.store
                && store.save(&state).is_err()
            {
                state
                    .client_sends
                    .retain(|send| send.message.entry_id != client.entry_id);
                return Response::error(503, "state_save_failed");
            }
        }
        let mut response = send(Some(client.clone()));
        if response.status >= 400 {
            self.lock_state()
                .client_sends
                .retain(|send| send.message.entry_id != client.entry_id);
            self.save_state();
            return response;
        }
        let mut document: Value =
            serde_json::from_str(&response.body).unwrap_or_else(|_| json!({}));
        document["client_message_id"] = json!(id);
        document["entry_id"] = json!(client.entry_id);
        let held_round = path == "/api/everyone"
            && self.prism.is_some()
            && document["queued"] == true
            && parsed
                .get("thread_id")
                .or_else(|| parsed.get("threadId"))
                .is_some_and(Value::is_string);
        let entry = {
            let mut state = self.lock_state();
            if !held_round
                && !state
                    .transcript
                    .iter()
                    .any(|entry| entry.id == client.entry_id)
            {
                state.push_transcript(crate::state::TranscriptEntry {
                    id: client.entry_id.clone(),
                    client_message_id: Some(id.into()),
                    conversation_id: document["conversationId"]
                        .as_str()
                        .filter(|cv| !cv.is_empty())
                        .unwrap_or(
                            if scope.starts_with("new:") || scope.starts_with("marble:") {
                                ""
                            } else {
                                &scope
                            },
                        )
                        .into(),
                    kind: "user".into(),
                    text: parsed["text"].as_str().unwrap_or_default().trim().into(),
                    task_id: document["taskId"].as_str().unwrap_or_default().into(),
                    thread_id: parsed
                        .get("thread_id")
                        .or_else(|| parsed.get("threadId"))
                        .and_then(Value::as_str)
                        .map(str::to_string),
                    at: now_millis(),
                    ..Default::default()
                });
            }
            let entry = state
                .transcript
                .iter()
                .find(|entry| entry.id == client.entry_id)
                .cloned();
            if let Some(entry) = &entry {
                if !entry.conversation_id.is_empty() {
                    document["conversationId"] = json!(entry.conversation_id);
                }
                if !entry.task_id.is_empty()
                    && document["taskId"].as_str().is_none_or(str::is_empty)
                {
                    document["taskId"] = json!(entry.task_id);
                }
            }
            let receipt = state
                .client_sends
                .iter_mut()
                .find(|send| send.message.entry_id == client.entry_id)
                .unwrap();
            receipt.status = response.status;
            receipt.response = Some(document.clone());
            state.bump();
            if let Some(store) = &self.store
                && store.save(&state).is_err()
            {
                return Response::error(503, "state_save_failed");
            }
            entry
        };
        response.body = document.to_string();
        if path == "/api/everyone"
            && !held_round
            && let Some(entry) = entry
        {
            self.events.publish(
                "everyone",
                &entry.task_id,
                "entry",
                serde_json::to_value(&entry).unwrap(),
            );
        }
        response
    }

    /// `POST /api/everyone` — one line to the shared plane, starting a round.
    fn post_everyone(
        &self,
        body: &str,
        client: Option<crate::state::ClientMessage>,
        device_time_zone: Option<&str>,
    ) -> Response {
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
        let thread =
            match self.requested_thread(&parsed, Some(crate::everyone::EVERYONE_CONVERSATION)) {
                Ok(id) => id,
                Err(response) => return response,
            };
        if let Some((pool, round_thread)) = self.prism.as_ref().zip(thread.as_ref()) {
            let answering = {
                let state = self.lock_state();
                match crate::everyone::plan(&state, &text) {
                    crate::everyone::EveryoneTarget::Named(ids) => ids
                        .into_iter()
                        .map(|id| {
                            if id == "marble" {
                                state.prism.default_marble().name
                            } else {
                                state
                                    .prism
                                    .get(&id)
                                    .map_or(id.clone(), |agent| agent.name.clone())
                            }
                        })
                        .collect::<Vec<_>>(),
                    crate::everyone::EveryoneTarget::All => Vec::new(),
                }
            };
            return match pool.start_everyone_round(
                round_thread.clone(),
                text,
                client,
                device_time_zone.map(str::to_owned),
            ) {
                Ok(held) => {
                    let mut response = json!({
                            "ok":true,"accepted":true,"answering":answering,"busy":[],
                            "conversationId":crate::everyone::EVERYONE_CONVERSATION,
                    });
                    if let Some(crate::prism::HeldRound { id, position }) = held {
                        self.events.publish(
                            "",
                            "",
                            "queued",
                            json!({"queueId":id,"position":position}),
                        );
                        response["queued"] = json!(true);
                        response["queueId"] = json!(id);
                        response["position"] = json!(position);
                        response["paused"] = json!(true);
                        response["answering"] = json!([]);
                    } else {
                        self.events.publish(
                            crate::everyone::EVERYONE_CONVERSATION,
                            "",
                            "everyone",
                            json!({"kind":"user","thread_id":thread}),
                        );
                    }
                    Response::json(202, response)
                }
                Err((status, error)) => Response::error(status, error),
            };
        }
        let entry_id = {
            let mut guard = self.lock_state();
            if thread
                .as_ref()
                .is_some_and(|id| !guard.threads.iter().any(|t| &t.id == id))
            {
                return Response::error(404, "unknown_thread");
            }
            if thread.as_ref().is_some_and(|id| {
                guard
                    .threads
                    .iter()
                    .any(|thread| &thread.id == id && thread.archived)
            }) {
                return Response::error(409, "thread_archived");
            }
            crate::everyone::mirror(
                &mut guard,
                crate::everyone::USER_SPEAKER,
                "You",
                "user",
                text.clone(),
                "",
                None,
            );
            let entry = guard
                .transcript
                .last_mut()
                .expect("the user entry was just mirrored");
            entry.thread_id = thread.clone();
            if let Some(client) = &client {
                entry.id = client.entry_id.clone();
                entry.client_message_id = Some(client.id.clone());
            }
            let entry_id = entry.id.clone();
            if let Some(id) = &thread {
                guard.refresh_thread(id);
            }
            guard.bump();
            entry_id
        };
        self.save_state();
        self.events.publish(
            crate::everyone::EVERYONE_CONVERSATION,
            "",
            "everyone",
            json!({"kind": "user", "thread_id":thread}),
        );
        let plan = crate::everyone::plan(&self.lock_state(), &text);
        match plan {
            crate::everyone::EveryoneTarget::Named(ids) => {
                self.dispatch_everyone(&text, ids, thread.as_deref(), &entry_id, device_time_zone)
            }
            crate::everyone::EveryoneTarget::All => {
                let ids: Vec<String> = {
                    let guard = self.lock_state();
                    std::iter::once(crate::everyone::DEFAULT_SPEAKER.to_string())
                        .chain(
                            guard
                                .prism
                                .marbles
                                .iter()
                                .filter(|agent| agent.in_everyone())
                                .map(|marble| marble.id.clone()),
                        )
                        .collect()
                };
                self.dispatch_everyone(&text, ids, thread.as_deref(), &entry_id, device_time_zone)
            }
        }
    }

    /// Reserve replies before dispatch; busy named engines drain their queue.
    fn dispatch_everyone(
        &self,
        text: &str,
        ids: Vec<String>,
        thread: Option<&str>,
        entry_id: &str,
        device_time_zone: Option<&str>,
    ) -> Response {
        if ids.is_empty() {
            let mut guard = self.lock_state();
            crate::everyone::mirror(
                &mut guard,
                "",
                "",
                "system",
                "There are no Marbles to answer yet.",
                "",
                None,
            );
            guard.bump();
            drop(guard);
            self.save_state();
            return Response::json(200, json!({"ok": true, "answering": [], "busy": []}));
        }
        let mut prepared = Vec::new();
        let mut busy = Vec::new();
        let mut seen = std::collections::HashSet::new();
        let small_talk = crate::normalize::is_small_talk(text);
        for id in &ids {
            if !seen.insert(id.clone()) {
                continue;
            }
            let conversation = crate::everyone::session_id(id, thread);
            let follow_up = {
                let mut state = self.lock_state();
                if let Some(task) = state.tasks.iter().rev().find(|task| {
                    task.status == "running"
                        && !task.id.starts_with("queued-")
                        && task.conversation_id == conversation
                        && crate::normalize::is_small_talk(&task.prompt) == small_talk
                }) {
                    let task_id = task.id.clone();
                    let lane = state.conversation_mut(&conversation).lane;
                    Some((task_id, lane))
                } else {
                    None
                }
            };
            if let Some((task_id, lane)) = follow_up {
                let request = SteerRequest {
                    conversation_id: conversation.clone(),
                    task_id,
                    text: text.to_string(),
                };
                let steered = if id == crate::everyone::DEFAULT_SPEAKER {
                    self.steer.send(lane, request)
                } else {
                    self.prism
                        .as_ref()
                        .is_some_and(|pool| pool.steer(id, request))
                };
                let name = {
                    let state = self.lock_state();
                    state
                        .prism
                        .get(id)
                        .map(|marble| marble.name.clone())
                        .unwrap_or_else(|| state.prism.default_marble().name)
                };
                if steered {
                    prepared.push((id.clone(), name, String::new(), None));
                } else {
                    busy.push(name);
                }
                continue;
            }
            if id == crate::everyone::DEFAULT_SPEAKER {
                let mut request = TurnRequest::new(Some(conversation.clone()), text.to_string());
                request.thread_id = thread.map(str::to_string);
                request.device_time_zone = device_time_zone.map(str::to_owned);
                let key = format!("queued-{}", request.request_id);
                let mut state = self.lock_state();
                crate::everyone::ensure_session(&mut state, &conversation);
                crate::everyone::reserve(&mut state, entry_id, &key, id, "Marble", thread);
                prepared.push((id.clone(), "Marble".to_string(), key, Some(request)));
            } else {
                let mut state = self.lock_state();
                let Some(marble) = state.prism.get(id).cloned() else {
                    continue;
                };
                crate::everyone::ensure_session(&mut state, &conversation);
                let queued =
                    state
                        .control
                        .enqueue_marble(id.clone(), Some(conversation), text.to_string());
                let key = match queued {
                    Ok(entry) => {
                        let queued = state
                            .control
                            .queue
                            .iter_mut()
                            .find(|e| e.id == entry.id)
                            .unwrap();
                        queued.thread_id = thread.map(str::to_string);
                        queued.device_time_zone = device_time_zone.map(str::to_owned);
                        entry.id
                    }
                    Err(error) => {
                        tracing::warn!(%error, marble = %id, "Everyone queue is full");
                        format!("failed-{}", crate::next_request_id())
                    }
                };
                crate::everyone::reserve(&mut state, entry_id, &key, id, &marble.name, thread);
                prepared.push((id.clone(), marble.name, key, None));
            }
        }
        self.lock_state().bump();
        self.save_state();
        let mut answering = Vec::new();
        for (id, name, key, request) in prepared {
            if key.is_empty() {
                answering.push(name);
                continue;
            }
            if id == crate::everyone::DEFAULT_SPEAKER {
                let response = self.submit_request(request.unwrap());
                if response.status < 400 {
                    answering.push("Marble".to_string());
                } else {
                    busy.push("Marble".to_string());
                    self.fail_everyone_dispatch(&id, &key, text, "default admission failed");
                }
                continue;
            }
            if self.prism.is_none() || key.starts_with("failed-") {
                self.fail_everyone_dispatch(&id, &key, text, "named admission unavailable");
                busy.push(name);
                continue;
            }
            answering.push(name);
        }
        if let Some(pool) = &self.prism {
            pool.resume_held();
        }
        self.events.publish(
            crate::everyone::EVERYONE_CONVERSATION,
            "",
            "everyone",
            json!({"kind": "dispatch", "answering": answering}),
        );
        Response::json(
            202,
            json!({"ok": true, "answering": answering, "busy": busy}),
        )
    }

    fn fail_everyone_dispatch(&self, speaker: &str, source: &str, prompt: &str, error: &str) {
        let mut state = self.lock_state();
        if !state.everyone_pending.contains_key(source) {
            return;
        }
        state.control.queue.retain(|entry| entry.id != source);
        let thread = state
            .transcript
            .iter()
            .find(|e| e.kind == "pending" && e.task_id == source)
            .and_then(|e| e.thread_id.clone());
        let conversation_id = crate::everyone::session_id(speaker, thread.as_deref());
        crate::everyone::ensure_session(&mut state, &conversation_id);
        state.bind_thread(source, thread.as_deref());
        state
            .conversation_mut(&conversation_id)
            .task_ids
            .push(source.into());
        state.upsert_task(crate::TaskRecord {
            id: source.into(),
            conversation_id: conversation_id.clone(),
            marble_id: if speaker == "marble" {
                String::new()
            } else {
                speaker.into()
            },
            owner: if speaker == "marble" {
                "agent"
            } else {
                "prism"
            }
            .into(),
            status: "failed".into(),
            prompt: prompt.into(),
            at: crate::worker::now_millis(),
            ..Default::default()
        });
        crate::everyone::claim(&mut state, source, source);
        state.record_failure(&conversation_id, source, error);
        crate::everyone::mirror_settlement(&mut state, source, "", "failed");
        state.bump();
        drop(state);
        self.save_state();
    }

    /// `POST /api/prism/dispatch` — hand one prompt to several marbles at once.
    fn dispatch_swarm(
        &self,
        body: &str,
        client: Option<crate::state::ClientMessage>,
        device_time_zone: Option<&str>,
    ) -> Response {
        let Some(pool) = self.prism.as_ref() else {
            return Response::json(
                503,
                json!({
                    "error": "prism_unavailable",
                    "message": "this host did not start a Prism pool",
                }),
            );
        };
        let parsed: Value = match serde_json::from_str(body) {
            Ok(value) => value,
            Err(_) => return Response::error(400, "invalid_json"),
        };
        let text = parsed
            .get("text")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        let ids: Option<Vec<String>> =
            parsed
                .get("marbles")
                .and_then(Value::as_array)
                .map(|values| {
                    values
                        .iter()
                        .filter_map(Value::as_str)
                        .map(str::to_string)
                        .collect()
                });
        let thread = match self.requested_thread(&parsed, None) {
            Ok(id) => id,
            Err(response) => return response,
        };
        match pool.dispatch_client_in_thread(
            &text,
            ids.as_deref(),
            thread.as_deref(),
            client,
            device_time_zone,
        ) {
            Ok(dispatch) => {
                let mut document = dispatch.document();
                if let Value::Object(map) = &mut document {
                    map.insert("ok".into(), json!(true));
                    map.insert("prism".into(), self.lock_state().prism_document());
                }
                Response::json(202, document)
            }
            Err(refusal) => Response::json(
                refusal.status,
                json!({"error": refusal.code, "message": refusal.message}),
            ),
        }
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
        let transcript_count = guard.transcript.len();
        // A lean poll omits the transcript, so drop it before the clone: copying
        // thousands of entries only to encode an empty array is the most
        // expensive thing a polling surface asks this host to do.
        let snapshot = if lean {
            let mut snapshot = guard.clone();
            snapshot.transcript.clear();
            snapshot
        } else {
            guard.clone()
        };
        drop(guard);
        let mut value = snapshot.encode();
        if let Value::Object(ref mut map) = value {
            map.insert(
                "deletedChats".into(),
                json!(
                    snapshot
                        .deleted_chats
                        .iter()
                        .map(|item| json!({
                            "id": item.id, "title": item.title, "deletedAt": item.deleted_at,
                        }))
                        .collect::<Vec<_>>()
                ),
            );
            if let Some(Value::Array(conversations)) = map.get_mut("conversations") {
                conversations.retain(|chat| chat["id"] != crate::threads::GROUP_CONVERSATION);
            }
            map.insert("transcriptCount".into(), json!(transcript_count));
            if lean {
                map.insert("transcript".into(), Value::Array(Vec::new()));
                map.insert("transcriptOmitted".into(), json!(true));
            }
            // Derived documents: the raw counters are not the whole story.
            map.insert("presence".into(), snapshot.presence.document(&snapshot));
            map.insert("prism".into(), snapshot.prism_document());
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

    fn create_conversation(&self, body: &str) -> Response {
        let parsed: Value = if body.trim().is_empty() {
            Value::Null
        } else {
            match serde_json::from_str(body) {
                Ok(value) => value,
                Err(_) => return Response::error(400, "invalid_json"),
            }
        };
        let agent = parsed["marbleId"].as_str().filter(|id| *id != "marble");
        let id = format!("chat-{}", crate::next_request_id());
        let conversation = {
            let mut state = self.lock_state();
            if agent.is_some_and(|id| state.prism.get(id).is_none()) {
                return Response::error(404, "unknown_marble");
            }
            let record = state.conversation_mut(&id);
            record.marble_id = agent.map(str::to_owned);
            record.updated_at = now_millis();
            record.lane = crate::state::ConversationRecord::UNASSIGNED_LANE;
            record.needs_engine_session = true;
            let conversation = record.clone();
            state.bump();
            conversation
        };
        self.save_state();
        self.events
            .publish(&id, "", "conversation", json!({"created": id}));
        Response::json(201, json!({"conversation": conversation}))
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

    fn archive_conversation(&self, id: &str, archived: bool) -> Response {
        if crate::everyone::is_session(id)
            || id == crate::threads::GROUP_CONVERSATION
            || id == crate::everyone::EVERYONE_CONVERSATION
        {
            return Response::error(409, "thread_owned_conversation");
        }
        {
            let mut state = self.lock_state();
            let Some(record) = state.conversations.iter_mut().find(|chat| chat.id == id) else {
                return Response::error(404, "unknown_conversation");
            };
            record.archived = archived;
            state.bump();
        }
        self.save_state();
        self.events
            .publish(id, "", "conversation", json!({"archived": archived}));
        Response::json(200, json!({"ok":true,"archived":archived}))
    }

    fn delete_conversation(&self, id: &str) -> Response {
        if crate::everyone::is_session(id) || id == crate::threads::GROUP_CONVERSATION {
            return Response::error(409, "thread_owned_conversation");
        }
        let outcome = {
            let mut guard = self.lock_state();
            guard.check_conversation_deletion(id).and_then(|()| {
                let mut updated = guard.clone();
                updated.thread_events = Default::default();
                updated.retain_deleted_chat(id, &[id.into()], None);
                updated.delete_conversation(id)?;
                updated.bump();
                if let Some(store) = &self.store {
                    store
                        .save(&updated)
                        .map_err(|error| format!("could not save removed conversation: {error}"))?;
                }
                self.commit_removed_chat(&mut guard, updated);
                Ok(())
            })
        };
        match outcome {
            Ok(()) => {
                // Its live journal goes too — a deleted chat must not replay
                // a recovered turn into the transcript on the next boot.
                if let Some(store) = &self.store {
                    let removed = crate::live::LiveStore::for_store(store).remove_conversation(id);
                    if removed > 0 {
                        tracing::info!(removed, conversation = %id, "cleared live journal for a deleted conversation");
                    }
                }
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
            Err(reason) if reason.starts_with("could not save removed conversation") => {
                Response::json(
                    503,
                    json!({"error":"conversation_delete_failed", "message":reason}),
                )
            }
            Err(reason) => Response::json(
                409,
                json!({"error": "conversation_busy", "message": reason}),
            ),
        }
    }

    fn restore_deleted_chat(&self, id: &str) -> Response {
        let mut state = self.lock_state();
        let mut restored = state.clone();
        restored.thread_events = Default::default();
        if !restored.restore_deleted_chat(id, now_millis()) {
            return Response::error(404, "unknown_deleted_chat");
        }
        restored.bump();
        if let Some(store) = &self.store
            && store.save(&restored).is_err()
        {
            return Response::error(503, "state_save_failed");
        }
        self.commit_removed_chat(&mut state, restored);
        drop(state);
        self.events
            .publish(id, "", "conversation", json!({"restored": id}));
        if let Some(pool) = &self.prism {
            pool.resume_held_everyone(id);
        }
        Response::json(200, json!({"ok": true}))
    }

    fn delete_conversation_record(&self, state: &mut ProductState, id: &str) -> Result<(), String> {
        state.check_conversation_deletion(id)?;
        self.remove_conversation_files(state, id)?;
        state.delete_conversation(id)
    }

    fn remove_conversation_files(&self, state: &ProductState, id: &str) -> Result<(), String> {
        if let Some(root) = self
            .store
            .as_ref()
            .filter(|_| {
                state
                    .conversations
                    .iter()
                    .any(|record| record.id == id && record.private_storage)
            })
            .and_then(|store| store.conversation_root(id))
        {
            crate::store::remove_conversation_root(&root)
                .map_err(|error| format!("could not remove conversation files: {error}"))?;
        }
        Ok(())
    }

    /// `GET /api/transcript?conversationId=…&after=…` — bounded history for one
    /// conversation. `after` is inclusive so entries that share a millisecond
    /// are never lost; clients deduplicate by content.
    fn threads_route(&self, head: &RequestHead, body: &str) -> Response {
        if head.method == "GET" {
            let conversation = head.query_param("conversation_id").unwrap_or_default();
            let state = self.lock_state();
            let mut threads: Vec<_> = state
                .threads
                .iter()
                .filter(|t| t.conversation_id == conversation)
                .collect();
            threads.sort_by_key(|t| std::cmp::Reverse(t.last_activity_at));
            return Response::json(200, json!({"threads":threads}));
        }
        if head.method != "POST" {
            return Response::error(405, "method_not_allowed");
        }
        let Ok(value) = serde_json::from_str::<Value>(body) else {
            return Response::error(400, "invalid_json");
        };
        if value["conversation_id"] == crate::threads::GROUP_CONVERSATION {
            let mut group: crate::threads::GroupThread =
                match serde_json::from_value(value["group"].clone()) {
                    Ok(group) => group,
                    Err(_) => return Response::error(400, "invalid_group"),
                };
            let title = value["title"].as_str().unwrap_or_default().trim();
            if title.is_empty() {
                return Response::error(400, "missing_title");
            }
            let preset = match value["preset"].as_str() {
                Some("refactor") => &["engineer", "reviewer"][..],
                Some("research") => &["researcher", "reviewer"][..],
                Some("launch") => &["designer", "engineer", "reviewer"][..],
                None => &[][..],
                Some(_) => return Response::error(400, "unknown_crew_preset"),
            };
            group
                .members
                .extend(preset.iter().map(|id| (*id).to_owned()));
            group.closed = false;
            group.suggestions.clear();
            let mut state = self.lock_state();
            group.members = match state.group_members(group.members) {
                Ok(members) => members,
                Err(error) => return Response::error(400, error),
            };
            let thread = state.start_group_thread(title, group);
            state.bump();
            drop(state);
            self.save_state();
            return Response::json(201, json!({"thread":thread}));
        }
        if value["conversation_id"] == crate::everyone::EVERYONE_CONVERSATION
            && value["parent_entry_id"].is_null()
        {
            let thread = {
                let mut state = self.lock_state();
                let thread =
                    state.start_everyone_thread(value["title"].as_str().unwrap_or_default());
                state.bump();
                thread
            };
            self.save_state();
            return Response::json(201, json!({"thread":thread}));
        }
        let (Some(conversation), Some(parent)) = (
            value["conversation_id"].as_str(),
            value["parent_entry_id"].as_str(),
        ) else {
            return Response::error(400, "missing_parent");
        };
        let thread = {
            let mut state = self.lock_state();
            let thread = state.create_thread(conversation, parent);
            if thread.is_some() {
                state.bump();
            }
            thread
        };
        match thread {
            Some(thread) => {
                self.save_state();
                Response::json(200, json!({"thread":thread}))
            }
            None => Response::error(404, "unknown_parent"),
        }
    }

    fn update_everyone_thread(&self, id: &str, action: &str, body: &str) -> Response {
        let mut state = self.lock_state();
        let Some(index) = state.threads.iter().position(|thread| {
            thread.id == id
                && (thread.conversation_id == crate::everyone::EVERYONE_CONVERSATION
                    || thread.group.is_some())
        }) else {
            return Response::error(404, "unknown_thread");
        };
        let conversation = state.threads[index].conversation_id.clone();
        match action {
            "members" if state.threads[index].group.is_some() => {
                let Ok(members) = serde_json::from_str::<Vec<String>>(body) else {
                    return Response::error(400, "invalid_group_members");
                };
                let members = match state.group_members(members) {
                    Ok(members) => members,
                    Err(error) => return Response::error(400, error),
                };
                state.threads[index].group.as_mut().unwrap().members = members;
            }
            "suggestion" if state.threads[index].group.is_some() => {
                let Ok(value) = serde_json::from_str::<Value>(body) else {
                    return Response::error(400, "invalid_suggestion");
                };
                let (Some(agent), Some(add)) = (value["agentId"].as_str(), value["add"].as_bool())
                else {
                    return Response::error(400, "invalid_suggestion");
                };
                let group = state.threads[index].group.as_ref().unwrap();
                if !group.suggestions.iter().any(|item| item.agent_id == agent) {
                    return Response::error(404, "unknown_suggestion");
                }
                if add {
                    let mut members = group.members.clone();
                    members.push(agent.into());
                    let members = match state.group_members(members) {
                        Ok(members) => members,
                        Err(error) => return Response::error(400, error),
                    };
                    state.threads[index].group.as_mut().unwrap().members = members;
                }
                state.threads[index]
                    .group
                    .as_mut()
                    .unwrap()
                    .suggestions
                    .retain(|item| item.agent_id != agent);
            }
            "archive" | "restore" => state.threads[index].archived = action == "archive",
            "rename" => {
                let title = serde_json::from_str::<Value>(body).ok().and_then(|value| {
                    value["title"]
                        .as_str()
                        .map(|title| title.trim().chars().take(80).collect::<String>())
                });
                let Some(title) = title.filter(|title| !title.is_empty()) else {
                    return Response::error(400, "missing_title");
                };
                state.threads[index].title = title;
            }
            "delete" => {
                if state
                    .transcript
                    .iter()
                    .any(|entry| entry.thread_id.as_deref() == Some(id) && entry.kind == "pending")
                {
                    return Response::error(409, "conversation_busy");
                }
                let records = state
                    .conversations
                    .iter()
                    .filter(|record| crate::everyone::session_in_thread(&record.id, id))
                    .map(|record| record.id.clone())
                    .collect::<Vec<_>>();
                if records
                    .iter()
                    .any(|record| state.check_conversation_deletion(record).is_err())
                {
                    return Response::error(409, "conversation_busy");
                }
                return self.delete_thread(&mut state, id, &records, &conversation);
            }
            _ => return Response::error(404, "unknown_thread_action"),
        }
        state.bump();
        drop(state);
        self.save_state();
        self.events.publish(
            &conversation,
            "",
            "conversation",
            json!({"thread_id":id, "action":action}),
        );
        if action == "restore"
            && conversation == crate::everyone::EVERYONE_CONVERSATION
            && let Some(pool) = &self.prism
        {
            pool.resume_held_everyone(id);
        }
        Response::json(200, json!({"ok":true}))
    }

    fn delete_thread(
        &self,
        state: &mut ProductState,
        id: &str,
        records: &[String],
        conversation: &str,
    ) -> Response {
        let mut deleted = state.clone();
        deleted.thread_events = Default::default();
        deleted.retain_deleted_chat(id, records, Some(id));
        for record in records {
            if deleted.delete_conversation(record).is_err() {
                return Response::error(409, "conversation_busy");
            }
        }
        deleted.threads.retain(|thread| thread.id != id);
        deleted
            .transcript
            .retain(|entry| entry.thread_id.as_deref() != Some(id));
        deleted.prune_threads();
        deleted.bump();
        if let Some(store) = &self.store
            && let Err(error) = store.save(&deleted)
        {
            tracing::warn!(%error, thread = %id, "could not persist thread deletion");
            return Response::error(503, "state_save_failed");
        }
        self.commit_removed_chat(state, deleted);
        self.events.publish(
            conversation,
            "",
            "conversation",
            json!({"thread_id":id, "action":"delete"}),
        );
        Response::json(200, json!({"ok":true}))
    }

    fn commit_removed_chat(&self, state: &mut ProductState, mut deleted: ProductState) {
        deleted.thread_events = state.thread_events.clone();
        if let Some(events) = &deleted.thread_events.0 {
            for task in state.task_threads.keys() {
                if !deleted.task_threads.contains_key(task) {
                    events.unbind_thread(task);
                }
            }
            for (task, thread) in &deleted.task_threads {
                if !state.task_threads.contains_key(task) {
                    events.bind_thread(task, thread);
                }
            }
            events.publish_replies(&deleted);
        }
        *state = deleted;
    }

    fn requested_thread(
        &self,
        value: &Value,
        conversation: Option<&str>,
    ) -> Result<Option<String>, Response> {
        match value.get("thread_id").or_else(|| value.get("threadId")) {
            None | Some(Value::Null) => Ok(None),
            Some(Value::String(id)) => {
                let state = self.lock_state();
                let thread = state
                    .threads
                    .iter()
                    .find(|t| &t.id == id)
                    .ok_or_else(|| Response::error(404, "unknown_thread"))?;
                if conversation.is_some_and(|c| c != thread.conversation_id) {
                    return Err(Response::error(409, "thread_conversation_mismatch"));
                }
                if thread.group.is_some() {
                    return Err(Response::error(409, "group_round_required"));
                }
                Ok(Some(id.clone()))
            }
            _ => Err(Response::error(400, "invalid_thread_id")),
        }
    }

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
        let guard = self.lock_state();
        if !conversation_id.is_empty()
            && !guard
                .conversations
                .iter()
                .any(|conversation| conversation.id == conversation_id)
        {
            return Response::json(
                404,
                json!({"error": "not_found", "message": "This chat no longer exists"}),
            );
        }
        let main = head.query_param("main").as_deref() == Some("1");
        let around = head.query_param("around");
        let entries: Vec<&crate::state::TranscriptEntry> = guard
            .transcript
            .iter()
            .filter(|entry| conversation_id.is_empty() || entry.conversation_id == conversation_id)
            .filter(|entry| !main || entry.thread_id.is_none())
            .filter(|entry| around.is_some() || after.is_none_or(|cursor| entry.at >= cursor))
            .collect();
        let page = match history_page(entries, head, Some(500)) {
            Ok(page) => page,
            Err(response) => return response,
        };
        let cursor = page
            .entries
            .iter()
            .map(|entry| entry.at)
            .max()
            .unwrap_or_default();
        Response::json(
            200,
            json!({
                "conversationId": conversation_id,
                "entries": page.entries,
                "cursor": cursor,
                "omitted": page.omitted,
                "revision": guard.revision,
                "hasMore": page.has_more,
                "nextBefore": page.next_before,
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

    fn update_knowledge(&self, id: &str, action: &str, body: &str) -> Response {
        let data = if body.trim().is_empty() {
            Value::Null
        } else {
            match serde_json::from_str::<Value>(body) {
                Ok(data) => data,
                Err(_) => return Response::error(400, "invalid_json"),
            }
        };
        let strings = |value: &Value| serde_json::from_value::<Vec<String>>(value.clone()).ok();
        let text = |key: &str| data[key].as_str().unwrap_or_default().to_owned();
        let mut state = self.lock_state();
        let updated = match action {
            "delete" => state.memory.delete(id),
            "restore" => state.memory.restore(id),
            "share-memory" => {
                let Some(shared) = data["shared"].as_bool() else {
                    return Response::error(400, "invalid_sharing");
                };
                if let Some(fact) = state.memory.facts.iter_mut().find(|fact| fact.id == id) {
                    fact.shared = shared;
                    true
                } else {
                    false
                }
            }
            "edit-skill" => {
                let Some(steps) = strings(&data["steps"]) else {
                    return Response::error(400, "invalid_steps");
                };
                match state
                    .memory
                    .update_skill(id, text("name"), text("whenToUse"), steps)
                {
                    Ok(()) => true,
                    Err("unknown_skill") => false,
                    Err(crate::memory::SKILL_CONTAINS_PRIVATE_MEMORY) => {
                        return Response::error(409, crate::memory::SKILL_CONTAINS_PRIVATE_MEMORY);
                    }
                    Err(reason) => {
                        return Response::json(
                            400,
                            json!({"error":"invalid_skill","message":reason}),
                        );
                    }
                }
            }
            "share-skill" => {
                let Some(mut agents) = strings(&data["agents"]) else {
                    return Response::error(400, "invalid_agents");
                };
                let Some(skill) = state.memory.skills.iter().find(|skill| skill.id == id) else {
                    return Response::error(404, "unknown_skill");
                };
                agents.retain(|agent| {
                    agent != &skill.owner && (agent == "marble" || state.prism.get(agent).is_some())
                });
                agents.sort_unstable();
                agents.dedup();
                if !agents.is_empty() && state.memory.skill_contains_private_fact(skill) {
                    return Response::error(409, crate::memory::SKILL_CONTAINS_PRIVATE_MEMORY);
                }
                state
                    .memory
                    .skills
                    .iter_mut()
                    .find(|skill| skill.id == id)
                    .unwrap()
                    .shared_with = agents;
                true
            }
            "confirm-pruning" => {
                let Some(selected) = strings(&data["selected"]) else {
                    return Response::error(400, "invalid_selection");
                };
                state.memory.resolve_pruning(id, &selected)
            }
            "resolve-consent" => {
                let fact = match &data["factId"] {
                    Value::Null => None,
                    Value::String(id) => Some(id.as_str()),
                    _ => return Response::error(400, "invalid_memory"),
                };
                state.memory.resolve_consent(id, fact)
            }
            _ => return Response::error(404, "unknown_knowledge_action"),
        };
        if !updated {
            return Response::error(404, "unknown_saved_item");
        }
        state.bump();
        let document = state.memory.document();
        drop(state);
        self.save_state();
        self.events
            .publish("", "", "memory", json!({"action":action,"id":id}));
        Response::json(200, json!({"ok":true,"memory":document}))
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
        let owner = parsed
            .get("owner")
            .and_then(Value::as_str)
            .map(str::trim)
            .unwrap_or_default();
        if !owner.is_empty() {
            let known = {
                let guard = self.lock_state();
                owner == "marble" || guard.prism.get(owner).is_some()
            };
            if !known {
                return Response::json(
                    400,
                    json!({"error": "invalid_memory", "message": "unknown memory owner"}),
                );
            }
        }
        self.remember_text(text, kind, "", true, parsed.get("owner").map(|_| owner))
    }

    fn memory_owner(&self, conversation_id: &str, owner: Option<&str>) -> String {
        if let Some(owner) = owner {
            return owner.to_string();
        }
        self.lock_state()
            .conversation_agent(conversation_id)
            .map(|marble| marble.id.clone())
            .unwrap_or_else(|| "marble".to_string())
    }

    fn remember_text(
        &self,
        text: &str,
        kind: &str,
        conversation_id: &str,
        explicit: bool,
        owner: Option<&str>,
    ) -> Response {
        // The owner of a remembered statement is the marble whose chat taught
        // it, unless the caller named an owner explicitly.
        let owner = self.memory_owner(conversation_id, owner);
        let outcome = {
            let mut guard = self.lock_state();
            let outcome = guard
                .memory
                .remember(text, kind, conversation_id, explicit, &owner);
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

    /// Explicit "forget: …" command: drop matching facts owned by this chat.
    fn forget_text(&self, query: &str, conversation_id: &str, owner: Option<&str>) -> Response {
        let owner = self.memory_owner(conversation_id, owner);
        let forgotten = {
            let mut guard = self.lock_state();
            let forgotten = guard.memory.forget_matching(query, &owner);
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
            let owner = guard
                .memory
                .suggestions
                .iter()
                .find(|suggestion| suggestion.id == id)
                .map(|suggestion| {
                    guard
                        .prism
                        .owner_of(&suggestion.conversation_id)
                        .map(|marble| marble.id.clone())
                        .unwrap_or_else(|| "marble".to_string())
                })
                .unwrap_or_else(|| "marble".to_string());
            let fact = guard.memory.accept(id, &owner);
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
        // A disabled routine keeps its schedule paused, but running it by hand
        // is a deliberate act: the firing runs and the answer says it overrode
        // the pause, instead of failing silently or pretending the routine is on.
        let overrides_pause = !routine.enabled;
        let paused = self.lock_state().control.paused;
        if paused {
            let queued = {
                let mut guard = self.lock_state();
                let outcome = guard.control.enqueue_routine(
                    routine.id.clone(),
                    routine.conversation_id.clone(),
                    routine.prompt.clone(),
                    overrides_pause,
                );
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
                    let mut value = json!({"queued": true, "queueId": entry.id});
                    if overrides_pause && let Value::Object(ref mut map) = value {
                        map.insert("override".into(), json!(true));
                        map.insert(
                            "message".into(),
                            json!("that routine is paused; this run will override the pause when the queue resumes"),
                        );
                    }
                    if let Value::Object(ref mut map) = value {
                        map.insert("routines".into(), self.lock_state().routines.document());
                    }
                    Response::json(202, value)
                }
                Err(reason) => {
                    Response::json(409, json!({"error": "queue_full", "message": reason}))
                }
            };
        }
        let mut request = TurnRequest::new(routine.conversation_id.clone(), routine.prompt.clone());
        request.routine_id = Some(routine.id.clone());
        request.routine_override = overrides_pause;
        // A firing waits in the routine lane behind typed messages, so it gets
        // the same durable row a typed message has while it waits. The worker
        // replaces the row once the engine admits the turn.
        let queued_id = format!("queued-{}", request.request_id);
        {
            let mut guard = self.lock_state();
            guard.upsert_task(TaskRecord {
                marble_id: String::new(),
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
            json!({
                "routineId": routine.id,
                "title": routine.title,
                "queued": false,
                "override": overrides_pause,
            }),
        );
        let mut value = json!({"admitted": true});
        if overrides_pause && let Value::Object(ref mut map) = value {
            map.insert("override".into(), json!(true));
            map.insert(
                "message".into(),
                json!("that routine is paused; this run overrides the pause"),
            );
        }
        if let Value::Object(ref mut map) = value {
            map.insert("routines".into(), self.lock_state().routines.document());
        }
        Response::json(202, value)
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
        // The crate map is a workspace scan, not a control fact, so it is served
        // at `/api/crates` and fetched when a surface shows it. Padding every
        // state poll with it made each poll pay for a document nobody read.
        self.lock_state().control.document()
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

    /// Pause or resume admission. Resuming hands each held entry to its lane, so
    /// a typed message still drains before a held routine firing.
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
                let (held, queued) = guard
                    .control
                    .take_queue()
                    .into_iter()
                    .partition(|entry| entry.marble_id.is_some() || entry.everyone_round);
                guard.control.queue = held;
                queued
            }
        };
        self.save_state();
        // An idle worker takes the first thing offered, so hand over typed
        // messages first: a resume keeps the priority the lanes promise instead
        // of admitting whichever entry happened to be held first.
        let mut queued = queued;
        queued.sort_by_key(|entry| entry.routine_id.is_some());
        let mut requests = Vec::with_capacity(queued.len());
        let mut dispatched = 0usize;
        for entry in queued {
            let mut request = TurnRequest::new(entry.conversation_id, entry.text);
            request.device_time_zone = entry.device_time_zone;
            request.client_message = entry.client_message;
            request.thread_id = entry.thread_id;
            if let Some(routine_id) = entry.routine_id {
                // A held firing resumes as a firing: it goes back in the routine
                // lane, and its run was counted when admission held it, so only
                // the task id is recorded now. A hand-run firing of a disabled
                // routine keeps its override through the queue.
                request = request.counted_routine(routine_id);
                request.routine_override = entry.routine_override;
            }
            requests.push(request);
        }
        // A resumed entry waits in its lane like any other queued turn, so it
        // gets the same durable row and presence can still count it; the worker
        // replaces the row when the engine admits the turn.
        if !requests.is_empty() {
            let mut guard = self.lock_state();
            for request in &requests {
                let queued_id = format!("queued-{}", request.request_id);
                if let Some(routine_id) = &request.routine_id {
                    guard.routines.note_admitted(routine_id, &queued_id);
                    guard.routines.note_status(routine_id, "queued");
                }
                guard.upsert_task(TaskRecord {
                    marble_id: String::new(),
                    id: queued_id,
                    conversation_id: request.conversation_id.clone().unwrap_or_default(),
                    status: "queued".into(),
                    owner: "agent".into(),
                    prompt: request.text.clone(),
                    at: now_millis(),
                    routine_id: request.routine_id.clone().unwrap_or_default(),
                });
            }
            guard.bump();
            drop(guard);
            self.save_state();
        }
        for request in requests {
            let queued_id = format!("queued-{}", request.request_id);
            // A held message for a marble's conversation resumes into that
            // marble's engine, not into the chat engine that cannot see it.
            if let Some((_, outcome)) = self.dispatch_to_owner(
                request.conversation_id.as_deref(),
                &request.text,
                request.thread_id.as_deref(),
                request.device_time_zone.as_deref(),
            ) {
                match outcome {
                    Ok(_) => dispatched += 1,
                    Err(refusal) => tracing::warn!(
                        code = refusal.code,
                        message = %refusal.message,
                        "could not resume a held marble turn"
                    ),
                }
                self.forget_queued(&queued_id);
                continue;
            }
            let (acknowledged, _ack) = mpsc::channel();
            match self.turn_tx.send(TurnCommand {
                request,
                accepted: acknowledged,
            }) {
                Ok(_) => dispatched += 1,
                Err(error) => {
                    tracing::warn!(%error, "the turn worker is gone; dropping a queued message");
                    self.forget_queued(&queued_id);
                }
            }
        }
        if !paused && let Some(pool) = &self.prism {
            dispatched += pool.resume_held();
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
        let flags = {
            let mut guard = self.lock_state();
            guard.control.paused = true;
            guard
                .control
                .note("", "", "", "", "allowed", "stop requested for every task");
            guard.bump();
            let flags = self.cancels.running();
            for (_, flag) in &flags {
                flag.store(true, Ordering::Relaxed);
            }
            flags
        };
        self.save_state();
        let mut cancelled = Vec::new();
        for (task_id, _) in flags {
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

    fn handle_turn(
        &self,
        body: &str,
        client: Option<crate::state::ClientMessage>,
        device_time_zone: Option<&str>,
    ) -> Response {
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
        if conversation_id
            .as_deref()
            .is_some_and(|id| self.lock_state().chat_in_trash(id))
        {
            return Response::error(404, "unknown_conversation");
        }
        let group_thread = parsed
            .get("thread_id")
            .or_else(|| parsed.get("threadId"))
            .and_then(Value::as_str)
            .filter(|id| {
                self.lock_state()
                    .threads
                    .iter()
                    .any(|thread| thread.id == *id && thread.group.is_some())
            });
        if let Some(thread) = group_thread {
            if conversation_id
                .as_deref()
                .is_some_and(|id| id != crate::threads::GROUP_CONVERSATION)
            {
                return Response::error(409, "thread_conversation_mismatch");
            }
            let Some(pool) = &self.prism else {
                return Response::error(503, "prism_unavailable");
            };
            return match pool.start_group_round(
                thread.into(),
                text,
                client,
                device_time_zone.map(str::to_owned),
            ) {
                Ok(()) => Response::json(
                    202,
                    json!({"accepted":true, "requestId":"", "taskId":"", "conversationId": crate::threads::GROUP_CONVERSATION}),
                ),
                Err((status, error)) => Response::error(status, error),
            };
        }
        if conversation_id.as_deref() == Some(crate::threads::GROUP_CONVERSATION) {
            return Response::error(409, "group_thread_required");
        }
        let marble = match parsed.get("marbleId") {
            None | Some(Value::Null) => None,
            Some(Value::String(id)) => match self.lock_state().prism.get(id).cloned() {
                Some(marble) => Some(marble),
                None => return Response::error(404, "unknown_marble"),
            },
            Some(_) => return Response::error(400, "invalid_marble_id"),
        };
        if let (Some(marble), Some(conversation)) = (&marble, &conversation_id)
            && self
                .lock_state()
                .conversation_agent(conversation)
                .map(|owner| &owner.id)
                != Some(&marble.id)
        {
            return Response::error(400, "conversation_owner_mismatch");
        }
        let conversation_id = conversation_id.or_else(|| {
            marble
                .as_ref()
                .map(|marble| marble.conversation_id.clone())
                .filter(|id| !id.is_empty())
        });
        // Explicit memory instructions are answered by the product itself: a
        // remembered fact is a durable write, not a model turn.
        match memory::parse_command(&text) {
            Some(MemoryCommand::Remember { text: fact, kind }) => {
                return self.remember_text(
                    &fact,
                    &kind,
                    conversation_id.as_deref().unwrap_or_default(),
                    true,
                    marble.as_ref().map(|marble| marble.id.as_str()),
                );
            }
            Some(MemoryCommand::Forget { text: query }) => {
                return self.forget_text(
                    &query,
                    conversation_id.as_deref().unwrap_or_default(),
                    marble.as_ref().map(|marble| marble.id.as_str()),
                );
            }
            None => {}
        }
        let thread = match self.requested_thread(&parsed, conversation_id.as_deref()) {
            Ok(id) => id,
            Err(response) => return response,
        };
        let conversation_id = conversation_id.or_else(|| {
            thread.as_ref().and_then(|id| {
                self.lock_state()
                    .threads
                    .iter()
                    .find(|t| &t.id == id)
                    .map(|t| t.conversation_id.clone())
            })
        });
        self.submit_text_for_thread(
            conversation_id,
            text,
            marble.map(|marble| marble.id),
            thread,
            client,
            device_time_zone,
        )
    }

    fn submit_text_for_thread(
        &self,
        conversation_id: Option<String>,
        text: String,
        marble_id: Option<String>,
        thread: Option<String>,
        client: Option<crate::state::ClientMessage>,
        device_time_zone: Option<&str>,
    ) -> Response {
        let marble_id = marble_id.or_else(|| {
            let state = self.lock_state();
            conversation_id
                .as_deref()
                .and_then(|id| state.conversation_agent(id))
                .map(|marble| marble.id.clone())
        });
        if self.lock_state().control.paused {
            // Paused admission is a queue, not a rejection: the message keeps its
            // place, and resuming dispatches it in arrival order.
            let queued = {
                let mut guard = self.lock_state();
                let result = match marble_id {
                    Some(id) => guard.control.enqueue_marble(id, conversation_id, text),
                    None => guard.control.enqueue(conversation_id, text),
                };
                match result {
                    Ok(mut entry) => {
                        entry.thread_id = thread.clone();
                        if let Some(queued) =
                            guard.control.queue.iter_mut().find(|q| q.id == entry.id)
                        {
                            queued.thread_id = thread.clone();
                            queued.client_message = client.clone();
                            queued.device_time_zone = device_time_zone.map(str::to_owned);
                        }
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
        if let Some(id) = marble_id {
            let Some(marble) = self.lock_state().prism.get(&id).cloned() else {
                return Response::error(404, "unknown_marble");
            };
            let mut response = if let Some((pool, conversation)) =
                self.prism.as_ref().zip(conversation_id.as_deref())
            {
                match pool.dispatch_private_chat(
                    &text,
                    &id,
                    conversation,
                    thread.as_deref(),
                    client,
                    device_time_zone,
                ) {
                    Ok(dispatch) => {
                        let mut document = dispatch.document();
                        document["ok"] = json!(true);
                        document["prism"] = self.lock_state().prism_document();
                        Response::json(202, document)
                    }
                    Err(error) => Response::json(
                        error.status,
                        json!({"error":error.code,"message":error.message}),
                    ),
                }
            } else {
                self.dispatch_swarm(
                    &json!({"text": text, "marbles": [id], "thread_id":thread}).to_string(),
                    client,
                    device_time_zone,
                )
            };
            if response.status == 202
                && let Ok(Value::Object(mut document)) =
                    serde_json::from_str::<Value>(&response.body)
            {
                document.insert("accepted".into(), json!(true));
                document.insert("queued".into(), json!(false));
                document.insert(
                    "conversationId".into(),
                    json!(conversation_id.unwrap_or(marble.conversation_id)),
                );
                document.insert("requestId".into(), json!(""));
                document.insert("taskId".into(), json!(""));
                document.insert("marbleId".into(), json!(marble.id));
                document.insert("marbleName".into(), json!(marble.name));
                response.body = Value::Object(document).to_string();
            }
            return response;
        }
        let mut request = TurnRequest::new(conversation_id, text);
        request.client_message = client;
        request.thread_id = thread;
        request.device_time_zone = device_time_zone.map(str::to_owned);
        self.submit_request(request)
    }

    /// The marble that keeps a conversation, when the roster has one.
    fn dispatch_to_owner(
        &self,
        conversation_id: Option<&str>,
        text: &str,
        thread: Option<&str>,
        device_time_zone: Option<&str>,
    ) -> Option<(
        PrismMarble,
        Result<crate::prism::PrismDispatch, DispatchRefusal>,
    )> {
        let id = conversation_id.filter(|id| !id.is_empty())?;
        let marble = {
            let state = self.lock_state();
            state.prism.owner_of(id).cloned()
        }?;
        let prism = self.prism.as_ref()?;
        let outcome = prism.dispatch_client_in_thread(
            text,
            Some(std::slice::from_ref(&marble.id)),
            thread,
            None,
            device_time_zone,
        );
        Some((marble, outcome))
    }

    /// Hand one message to the worker, answering as soon as it is admitted.
    ///
    /// The worker runs one turn at a time. A busy host used to leave the caller
    /// waiting until a timeout and then answer a gateway error, which was both
    /// slow and untrue: the message was never lost. Waiting a short bound keeps
    /// the immediate answer when the worker is free; when it is not, the `202`
    /// says where the message stands, and it is already in the worker's channel.
    fn submit_request(&self, request: TurnRequest) -> Response {
        if let Some(id) = &request.conversation_id {
            let mut state = self.lock_state();
            let retry = state
                .conversations
                .iter()
                .find(|c| &c.id == id)
                .map(|c| c.retry_task_id.clone())
                .unwrap_or_default();
            if state.transcript.iter().any(|e| {
                e.task_id == retry
                    && e.kind == "failure"
                    && e.conversation_id == crate::everyone::EVERYONE_CONVERSATION
                    && e.speaker == crate::everyone::DEFAULT_SPEAKER
            }) {
                state.everyone_pending.insert(
                    format!("queued-{}", request.request_id),
                    crate::everyone::DEFAULT_SPEAKER.into(),
                );
            }
        }
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
                marble_id: String::new(),
                id: queued_id.clone(),
                conversation_id: conversation_id.clone(),
                status: "queued".into(),
                owner: "agent".into(),
                prompt: request.text.clone(),
                at: now_millis(),
                routine_id: request.routine_id.clone().unwrap_or_default(),
            });
            guard.bind_thread(&queued_id, request.thread_id.as_deref());
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
                    json!({"error": "turn_failed", "message": crate::failure::MESSAGE, "failure":crate::failure::FailureInfo::of(&error.to_string())}),
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
                        "taskId": queued_id,
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
            guard.tasks.retain(|task| {
                task.id != queued_id
                    || matches!(task.status.as_str(), "failed" | "cancelled" | "interrupted")
            });
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
    fn retry_turn(&self, body: &str, device_time_zone: Option<&str>) -> Response {
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
        let retry_conversation = task.conversation_id.clone();
        if crate::everyone::is_group_session(&retry_conversation) {
            let thread = self.lock_state().task_threads.get(&retrying).cloned();
            let Some((pool, thread)) = self.prism.as_ref().zip(thread) else {
                return Response::error(409, "nothing_to_retry");
            };
            return match pool.start_group_round(
                thread,
                task.prompt,
                None,
                device_time_zone.map(str::to_owned),
            ) {
                Ok(()) => Response::json(
                    202,
                    json!({"accepted":true, "requestId":"", "taskId":"", "conversationId": crate::threads::GROUP_CONVERSATION}),
                ),
                Err((status, error)) => Response::error(status, error),
            };
        }
        if crate::everyone::is_session(&retry_conversation) && task.owner == "prism" {
            let thread = self.lock_state().task_threads.get(&retrying).cloned();
            if let Some((pool, thread)) = self.prism.as_ref().zip(thread) {
                return match pool.retry_everyone_round(
                    thread,
                    task,
                    device_time_zone.map(str::to_owned),
                ) {
                    Ok(()) => Response::json(
                        202,
                        json!({
                            "accepted":true,"retrying":retrying,"conversationId":crate::everyone::EVERYONE_CONVERSATION,
                        }),
                    ),
                    Err((status, error)) => Response::error(status, error),
                };
            }
        }
        let thread = {
            let mut state = self.lock_state();
            let thread = state.task_threads.get(&retrying).cloned();
            if thread.as_ref().is_some_and(|id| {
                state
                    .threads
                    .iter()
                    .any(|thread| &thread.id == id && thread.archived)
            }) {
                return Response::error(409, "thread_archived");
            }
            state.conversation_mut(&task.conversation_id).retry_task_id = task.id.clone();
            if let Some(speaker) = state
                .transcript
                .iter()
                .find(|e| {
                    e.conversation_id == crate::everyone::EVERYONE_CONVERSATION
                        && e.task_id == task.id
                        && e.kind == "failure"
                })
                .map(|e| e.speaker.clone())
                && speaker != crate::everyone::DEFAULT_SPEAKER
            {
                state
                    .everyone_pending
                    .insert(format!("marble:{speaker}"), speaker);
            }
            thread
        };
        self.save_state();
        // A turn recovered from a lost queue lives in a placeholder chat the
        // engine never created, so repeating it starts a fresh conversation.
        let conversation =
            (!task.conversation_id.starts_with("recovered-")).then_some(task.conversation_id);
        let marble_id = (!task.marble_id.is_empty()).then_some(task.marble_id);
        let mut response = self.submit_text_for_thread(
            conversation,
            task.prompt,
            marble_id,
            thread,
            None,
            device_time_zone,
        );
        if response.status >= 400 {
            self.lock_state()
                .conversation_mut(&retry_conversation)
                .retry_task_id
                .clear();
            self.save_state();
        }
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
            guard
                .tasks
                .iter()
                .rev()
                .find(|task| {
                    task.status == "running"
                        && requested_task.is_none_or(|id| task.id == id)
                        && requested_conversation.is_none_or(|id| task.conversation_id == id)
                })
                .map(|task| {
                    let lane = guard
                        .conversations
                        .iter()
                        .find(|record| record.id == task.conversation_id)
                        .map_or(0, |record| record.lane);
                    (
                        task.conversation_id.clone(),
                        task.id.clone(),
                        (task.owner == "prism").then(|| task.marble_id.clone()),
                        lane,
                    )
                })
        };
        let Some((conversation_id, task_id, marble_id, lane)) = target else {
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
        let queued = if let Some(marble_id) = marble_id {
            self.prism
                .as_ref()
                .is_some_and(|pool| pool.steer(&marble_id, request))
        } else {
            self.steer.send(lane, request)
        };
        if !queued {
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
        let queued = {
            let mut guard = self.lock_state();
            if let Some(index) = guard
                .control
                .queue
                .iter()
                .position(|entry| entry.id == task_id)
            {
                let entry = guard.control.queue.remove(index);
                guard.bind_thread(task_id, entry.thread_id.as_deref());
                if let Some(routine_id) = &entry.routine_id {
                    guard.routines.note_admitted(routine_id, task_id);
                }
                guard.upsert_task(TaskRecord {
                    id: entry.id,
                    conversation_id: entry.conversation_id.unwrap_or_default(),
                    marble_id: entry.marble_id.unwrap_or_default(),
                    status: "queued".into(),
                    owner: "agent".into(),
                    prompt: entry.text,
                    at: entry.at,
                    routine_id: entry.routine_id.unwrap_or_default(),
                });
            }
            if let Some(task) = guard
                .tasks
                .iter_mut()
                .find(|task| task.id == task_id && task.status == "queued")
            {
                task.status = "cancelled".into();
                if task.conversation_id.is_empty() {
                    task.conversation_id = format!("recovered-{task_id}");
                }
                let conversation_id = task.conversation_id.clone();
                let prompt = task.prompt.clone();
                let conversation = guard.conversation_mut(&conversation_id);
                conversation.updated_at = now_millis();
                if conversation.title.is_empty() {
                    conversation.title = prompt.chars().take(64).collect();
                }
                guard.push_transcript(crate::state::TranscriptEntry {
                    conversation_id,
                    task_id: task_id.into(),
                    kind: "system".into(),
                    text: "Cancelled before it started".into(),
                    at: now_millis(),
                    ..Default::default()
                });
                guard.routines.note_settled(task_id, "cancelled");
                crate::everyone::claim(&mut guard, task_id, task_id);
                crate::everyone::mirror_settlement(&mut guard, task_id, "", "cancelled");
                guard.bump();
                true
            } else {
                false
            }
        };
        if queued {
            self.turn_tx.remove_queued(task_id);
            self.save_state();
            self.events.publish(
                "",
                task_id,
                "state",
                json!({"status": "cancelled", "message": "Cancelled before it started"}),
            );
            return Response::json(200, json!({"cancelled": true, "taskId": task_id}));
        }
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
        // Persist straight from the borrowed document: a full clone per mutation
        // is the most expensive part of a write that is otherwise bounded.
        let guard = self.lock_state();
        crate::store::persist_optional(self.store.as_deref(), &guard)
    }

    pub fn serve(self) -> std::io::Result<()> {
        let server = Arc::new(self);
        let weak = Arc::downgrade(&server);
        std::thread::spawn(move || {
            loop {
                std::thread::park_timeout(Duration::from_secs(60));
                let Some(server) = weak.upgrade() else { return };
                let changed = {
                    let mut state = server.lock_state();
                    let count = state.memory.recently_deleted.len();
                    state.memory.expire_deleted(now_millis());
                    let expired_chats = state.expire_deleted_chats(now_millis());
                    if expired_chats || state.memory.recently_deleted.len() != count {
                        state.bump();
                        true
                    } else {
                        false
                    }
                };
                if let Some(store) = &server.store {
                    let mut state = server.lock_state();
                    if (!changed && state.pending_everyone_cleanup.is_empty())
                        || store.save(&state).is_err()
                    {
                        continue;
                    }
                    store.retry_everyone_cleanup(&mut state);
                }
            }
        });
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
                let peer = stream
                    .peer_addr()
                    .map(|address| address.ip().to_string())
                    .unwrap_or_default();
                tracing::info!(
                    "{}",
                    RequestLog {
                        peer: &peer,
                        method: "-",
                        path: "-",
                        status: 503,
                        auth: "none",
                        tls: false,
                        authorized: false,
                        head_bytes: 0,
                        content_length: 0,
                        user_agent: "-",
                        elapsed: Duration::ZERO,
                    }
                    .line()
                );
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
        // Peek (without consuming) at the connection's first bytes: a TLS
        // ClientHello on this port gets a real handshake when a cert is
        // configured, a fatal alert otherwise.
        let mut first = [0u8; 3];
        let mut n = stream.peek(&mut first)?;
        for _ in 0..100 {
            if n >= 3 || n == 0 {
                break;
            }
            std::thread::sleep(Duration::from_millis(5));
            n = stream.peek(&mut first)?;
        }
        if is_tls_handshake(&first[..n]) {
            let Some(tls) = self.tls.clone() else {
                tracing::info!(
                    "tls handshake on plain-http port peer={peer} — this host serves HTTP; \
                     use http:// not https://"
                );
                let _ = stream.write_all(&[0x15, 0x03, 0x01, 0x00, 0x02, 0x02, 0x28]);
                return Ok(());
            };
            let connection = match rustls::ServerConnection::new(Arc::clone(&tls.config)) {
                Ok(connection) => connection,
                Err(error) => {
                    tracing::warn!("TLS handshake setup failed for {peer}: {error}");
                    return Ok(());
                }
            };
            return self.serve_stream(
                IoBuf::new(rustls::StreamOwned::new(connection, stream)),
                &peer,
                true,
            );
        }
        self.serve_stream(IoBuf::new(stream), &peer, false)
    }

    /// One connection, plain or TLS, sharing routing and keep-alive.
    fn serve_stream<S: Read + Write>(
        &self,
        mut conn: IoBuf<S>,
        peer: &str,
        tls: bool,
    ) -> std::io::Result<()> {
        let mut served = 0usize;
        loop {
            let started = std::time::Instant::now();
            let head_bytes = read_head(&mut conn)?;
            if head_bytes.is_empty() {
                return Ok(());
            }
            served += 1;
            let head_text = String::from_utf8_lossy(&head_bytes).to_string();
            let head = parse_head(&head_text);
            let body = match read_body(&mut conn, &head)? {
                BodyRead::Bytes(body) => String::from_utf8_lossy(&body).to_string(),
                BodyRead::TooLarge => {
                    self.log_request(peer, head_bytes.len(), &head, 413, started, tls);
                    return write_response(
                        &mut conn,
                        Response::error(413, "body_too_large"),
                        false,
                    );
                }
            };
            if head.method == "GET" && head.path() == "/api/events" {
                let authorized = self.policy.authorize(&head);
                if !authorized {
                    self.log_request(peer, head_bytes.len(), &head, 401, started, tls);
                    return write_response(&mut conn, Response::error(401, "unauthorized"), false);
                }
                self.log_request(peer, head_bytes.len(), &head, 200, started, tls);
                return self.stream_events(&head, &mut conn);
            }
            let response = if head.method == "GET" && head.path() == "/api/tls-proof" {
                self.tls_proof(&head, tls)
            } else {
                self.handle_request_from(&head, &body, peer)
            };
            let status = response.status;
            self.log_request(peer, head_bytes.len(), &head, status, started, tls);
            let keep_alive = wants_keep_alive(&head)
                && response.status < 400
                && served < MAX_REQUESTS_PER_CONNECTION;
            write_response(&mut conn, response, keep_alive)?;
            if !keep_alive {
                return Ok(());
            }
        }
    }

    /// Unauthenticated proof that a client holding the token saw this
    /// certificate: HMAC-SHA256 keyed on the token over the leaf hash it
    /// observed and a caller-chosen nonce. TLS connections only.
    fn tls_proof(&self, head: &RequestHead, tls: bool) -> Response {
        if !tls {
            return Response::error(400, "tls_only");
        }
        let Some(identity) = &self.tls else {
            return Response::error(404, "not_found");
        };
        let Some(token) = self.policy.token() else {
            return Response::error(404, "not_found");
        };
        let nonce = head.query_param("nonce").unwrap_or_default();
        let valid_nonce = (32..=128).contains(&nonce.len())
            && nonce.chars().all(|character| character.is_ascii_hexdigit());
        if !valid_nonce {
            return Response::error(400, "bad_nonce");
        }
        let cert_sha256 = identity.leaf_sha256_hex();
        let proof = crate::tls::tls_proof(token, &nonce, &cert_sha256);
        Response::json(
            200,
            json!({
                "certSha256": cert_sha256,
                "proof": proof,
            }),
        )
    }

    /// One line per request, after its status is known.
    fn log_request(
        &self,
        peer: &str,
        head_bytes: usize,
        head: &RequestHead,
        status: u16,
        started: std::time::Instant,
        tls: bool,
    ) {
        let content_length = head
            .header("content-length")
            .and_then(|value| value.trim().parse::<usize>().ok())
            .unwrap_or(0);
        tracing::info!(
            "{}",
            RequestLog {
                peer,
                method: &head.method,
                path: head.path(),
                status,
                auth: auth_kind(head),
                tls,
                authorized: self.policy.authorize(head),
                head_bytes,
                content_length,
                user_agent: head.header("user-agent").unwrap_or("-"),
                elapsed: started.elapsed(),
            }
            .line()
        );
    }

    /// Server-sent events with bounded replay from the client's own cursor.
    fn stream_events(&self, head: &RequestHead, stream: &mut impl Write) -> std::io::Result<()> {
        let since = head
            .query_param("since")
            .and_then(|value| value.parse::<u64>().ok())
            .unwrap_or_else(|| self.events.sequence());
        stream.write_all(
            b"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream; charset=utf-8\r\n\
              Cache-Control: no-store\r\nConnection: close\r\nX-Accel-Buffering: no\r\n\r\n",
        )?;
        stream.write_all(b": connected\n\n")?;
        stream.flush()?;
        let receiver = self.events.subscribe();
        let cursor = self.events.sequence();
        write!(
            stream,
            "event: hello\ndata: {}\n\n",
            json!({"cursor": cursor, "build": crate::build_info::document()})
        )?;
        stream.flush()?;
        let since = if since > cursor { 0 } else { since };
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

struct HistoryPage<'a> {
    entries: Vec<&'a crate::TranscriptEntry>,
    has_more: bool,
    next_before: Option<String>,
    omitted: usize,
}

fn history_page<'a>(
    mut entries: Vec<&'a crate::TranscriptEntry>,
    head: &RequestHead,
    default_limit: Option<usize>,
) -> Result<HistoryPage<'a>, Response> {
    let before = head.query_param("before");
    let around = head.query_param("around");
    if before.is_some() && (around.is_some() || head.query_param("after").is_some()) {
        return Err(Response::error(400, "conflicting_cursors"));
    }
    let limit = match head.query_param("limit") {
        Some(raw) => raw
            .parse::<usize>()
            .map(|n| n.clamp(1, 1_000))
            .map_err(|_| Response::error(400, "invalid_limit"))?,
        None => default_limit.unwrap_or(if before.is_some() { 500 } else { usize::MAX }),
    };
    if let Some(id) = before {
        if id.is_empty() || id.len() > 256 || id.chars().any(char::is_control) {
            return Err(Response::error(400, "invalid_cursor"));
        }
        let index = entries
            .iter()
            .position(|e| e.id == id)
            .ok_or_else(|| Response::error(404, "unknown_cursor"))?;
        entries.truncate(index);
    }
    let start = if let Some(id) = &around {
        let index = entries
            .iter()
            .position(|e| e.id == *id)
            .ok_or_else(|| Response::error(404, "unknown_entry"))?;
        index
            .saturating_sub(limit / 2)
            .min(entries.len().saturating_sub(limit))
    } else {
        entries.len().saturating_sub(limit)
    };
    let entries: Vec<_> = entries.into_iter().skip(start).take(limit).collect();
    Ok(HistoryPage {
        has_more: start > 0,
        next_before: (start > 0).then(|| entries[0].id.clone()),
        omitted: if around.is_some() { 0 } else { start },
        entries,
    })
}

fn valid_conversation_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 128
        && id
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || matches!(character, '-' | '_'))
}

fn write_event(stream: &mut impl Write, event: &TurnEvent) -> std::io::Result<()> {
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
    stream: &mut impl Write,
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

/// A request body read from the wire, or the fact that it is larger than this
/// instance accepts.
enum BodyRead {
    Bytes(Vec<u8>),
    TooLarge,
}

/// Read one request body. A reverse proxy may forward an upload with
/// `Transfer-Encoding: chunked` even when the client sent a length, so both
/// framings are accepted; both are bounded by `MAX_BODY_BYTES` before the body
/// is buffered.
fn read_body(reader: &mut impl BufRead, head: &RequestHead) -> std::io::Result<BodyRead> {
    let chunked = head
        .header("transfer-encoding")
        .map(|value| {
            value
                .split(',')
                .any(|token| token.trim().eq_ignore_ascii_case("chunked"))
        })
        .unwrap_or(false);
    if chunked {
        return read_chunked_body(reader);
    }
    let content_length = head
        .header("content-length")
        .and_then(|value| value.trim().parse::<usize>().ok())
        .unwrap_or(0);
    if content_length > MAX_BODY_BYTES {
        return Ok(BodyRead::TooLarge);
    }
    let mut body = vec![0u8; content_length];
    if content_length > 0 {
        reader.read_exact(&mut body)?;
    }
    Ok(BodyRead::Bytes(body))
}

fn read_chunked_body(reader: &mut impl BufRead) -> std::io::Result<BodyRead> {
    let mut body = Vec::new();
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line)? == 0 {
            return Ok(BodyRead::Bytes(body));
        }
        let size_text = line.trim().split(';').next().unwrap_or_default().trim();
        let size = usize::from_str_radix(size_text, 16).map_err(|_| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "a chunk size is hexadecimal",
            )
        })?;
        if size == 0 {
            // Trailer fields carry nothing here, but they still end at a blank
            // line and must be consumed before the next request on this socket.
            loop {
                let mut trailer = String::new();
                if reader.read_line(&mut trailer)? == 0 || trailer.trim().is_empty() {
                    break;
                }
            }
            return Ok(BodyRead::Bytes(body));
        }
        if body.len() + size > MAX_BODY_BYTES {
            return Ok(BodyRead::TooLarge);
        }
        let start = body.len();
        body.resize(start + size, 0);
        reader.read_exact(&mut body[start..])?;
        let mut terminator = [0u8; 2];
        reader.read_exact(&mut terminator)?;
        if &terminator != b"\r\n" {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "a chunk ends with CRLF",
            ));
        }
    }
}

/// One connection's buffered reader plus writer, so plain TCP and
/// `rustls::StreamOwned` share the routing loop without a `try_clone`.
struct IoBuf<S> {
    inner: S,
    buf: Vec<u8>,
    pos: usize,
}

impl<S: Read> IoBuf<S> {
    fn new(inner: S) -> Self {
        Self {
            inner,
            buf: Vec::new(),
            pos: 0,
        }
    }
}

impl<S: Read> Read for IoBuf<S> {
    fn read(&mut self, out: &mut [u8]) -> std::io::Result<usize> {
        let available = self.fill_buf()?;
        let n = available.len().min(out.len());
        out[..n].copy_from_slice(&available[..n]);
        self.consume(n);
        Ok(n)
    }
}

impl<S: Read> BufRead for IoBuf<S> {
    fn fill_buf(&mut self) -> std::io::Result<&[u8]> {
        if self.pos >= self.buf.len() {
            self.buf.resize(8192, 0);
            let read = self.inner.read(&mut self.buf)?;
            self.buf.truncate(read);
            self.pos = 0;
        }
        Ok(&self.buf[self.pos..])
    }

    fn consume(&mut self, amount: usize) {
        self.pos = (self.pos + amount).min(self.buf.len());
    }
}

impl<S: Write> Write for IoBuf<S> {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.inner.write(bytes)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.inner.flush()
    }
}

/// A TLS handshake record: content type 0x16, version 0x03 0x00..0x04.
fn is_tls_handshake(buf: &[u8]) -> bool {
    buf.len() >= 3 && buf[0] == 0x16 && buf[1] == 0x03 && buf[2] <= 0x04
}

/// Log fields may come from bytes that were not HTTP at all (e.g. a TLS
/// ClientHello); binary fields collapse to `<binary>` instead of raw bytes.
fn printable(field: &str) -> &str {
    if field.chars().all(|c| c.is_ascii_graphic() || c == ' ') {
        field
    } else {
        "<binary>"
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
    fn path_segments_decode_percent_escapes() {
        assert_eq!(
            decode_path_segment("host%3Asession%3Atask%3Acall-1"),
            "host:session:task:call-1"
        );
        assert_eq!(decode_path_segment("plain-id"), "plain-id");
        assert_eq!(decode_path_segment("a+b"), "a+b");
        assert_eq!(decode_path_segment("trailing%"), "trailing%");
        assert_eq!(decode_path_segment("bad%zz"), "bad%zz");
    }

    #[test]
    fn query_tokens_decode_and_match() {
        let head = RequestHead::new("GET", "/api/events?since=4&token=a%2Bb+c");
        assert_eq!(head.query_param("since"), Some("4".to_string()));
        assert_eq!(head.query_param("token"), Some("a+b c".to_string()));
        assert_eq!(head.path(), "/api/events");
    }

    #[test]
    fn request_log_line_never_carries_a_token_or_query() {
        let head = RequestHead::new("GET", "/api/state?lean=1&token=super-secret")
            .with_header("authorization", "Bearer super-secret")
            .with_header("user-agent", "GlassMarble/1 CFNetwork/1 Darwin/24")
            .with_header("content-length", "42");
        assert_eq!(auth_kind(&head), "bearer");
        let line = RequestLog {
            peer: "100.64.0.5",
            method: &head.method,
            path: head.path(),
            status: 401,
            auth: auth_kind(&head),
            tls: false,
            authorized: false,
            head_bytes: 256,
            content_length: 42,
            user_agent: head.header("user-agent").unwrap_or("-"),
            elapsed: Duration::from_millis(7),
        }
        .line();
        assert!(line.contains("peer=100.64.0.5"));
        assert!(line.contains("method=GET"));
        assert!(line.contains("path=/api/state"));
        assert!(line.contains("status=401"));
        assert!(line.contains("auth=bearer"));
        assert!(line.contains("authorized=no"));
        assert!(line.contains("head=256B"));
        assert!(line.contains("body=42B"));
        assert!(line.contains("ms=7"));
        assert!(!line.contains("super-secret"));
        assert!(!line.contains("?"));
        assert!(!line.contains("lean=1"));

        let long_agent = "x".repeat(120);
        let truncated = RequestLog {
            peer: "",
            method: "GET",
            path: "/",
            status: 200,
            auth: "none",
            tls: false,
            authorized: false,
            head_bytes: 0,
            content_length: 0,
            user_agent: &long_agent,
            elapsed: Duration::ZERO,
        }
        .line();
        assert!(truncated.contains(&"x".repeat(60)));
        assert!(!truncated.contains(&"x".repeat(61)));
    }

    #[test]
    fn tls_handshake_detection() {
        for version in 0x00u8..=0x04 {
            assert!(is_tls_handshake(&[0x16, 0x03, version, 0x00]));
        }
        // A real ClientHello head: handshake record, TLS 1.0 on the wire.
        assert!(is_tls_handshake(&[0x16, 0x03, 0x01, 0x01, 0x2f]));
        assert!(!is_tls_handshake(b"GET / HTTP/1.1\r\n"));
        assert!(!is_tls_handshake(b"POST /api/turns HTTP/1.1\r\n"));
        assert!(!is_tls_handshake(b""));
        assert!(!is_tls_handshake(&[0x16]));
        assert!(!is_tls_handshake(&[0x16, 0x03]));
        // Alert record, not a handshake.
        assert!(!is_tls_handshake(&[0x15, 0x03, 0x01]));
        assert!(!is_tls_handshake(&[0x16, 0x03, 0x05]));
    }

    #[test]
    fn request_log_sanitises_binary_method_and_path() {
        let line = RequestLog {
            peer: "1.2.3.4",
            method: "\u{16}\u{3}\u{1}",
            path: "\u{0}\u{1}\u{2}",
            status: 0,
            auth: "none",
            tls: false,
            authorized: false,
            head_bytes: 512,
            content_length: 0,
            user_agent: "-",
            elapsed: Duration::ZERO,
        }
        .line();
        assert!(line.contains("method=<binary>"), "{line}");
        assert!(line.contains("path=<binary>"), "{line}");
        assert_eq!(printable("GET"), "GET");
        assert_eq!(printable("/api/state"), "/api/state");
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
