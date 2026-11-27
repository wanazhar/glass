use glass_marble::{
    AccessPolicy, ApprovalInbox, CancelRegistry, Conversation, EventBus, HostPolicy, LaneFactory,
    LiveStore, MarbleError, MarbleServer, MarbleServerParts, PendingApproval, ProductState,
    RequestHead, StateStore, SteerRequest, TaskRecord, ToolRouter, TranscriptEntry, TurnInbox,
    TurnLane, TurnSender, spawn_turn_pool,
};
use serde_json::{Value, json};
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::process::Command;
use std::sync::{Arc, Mutex, mpsc};
use std::time::{Duration, Instant};

/// A pool of one lane around an engine the test already spawned.
///
/// The pool is the only turn loop, so a single-engine host runs the same
/// dispatcher and lane code as a host that opens several chats.
fn spawn_single_lane(
    conversation: Conversation,
    state: Arc<Mutex<ProductState>>,
    store: Option<Arc<StateStore>>,
    live: LiveStore,
    events: EventBus,
    timeout: Duration,
    cancels: CancelRegistry,
) -> TurnSender {
    let conversation = Mutex::new(Some(conversation));
    let factory: LaneFactory = Arc::new(move |_lane| {
        conversation
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .take()
            .ok_or_else(|| MarbleError::Protocol("this test lane was already started".into()))
    });
    spawn_turn_pool(1, factory, state, store, live, events, timeout, cancels)
        .expect("a one lane pool")
        .sender
}

fn python3_available() -> bool {
    Command::new("python3")
        .arg("--version")
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

fn test_state() -> ProductState {
    let mut state = ProductState::default();
    state.bump();
    state.conversations.push(glass_marble::ConversationRecord {
        id: "durable-fixture-1".into(),
        title: "list the workspace".into(),
        task_ids: vec!["generation-fixture-1".into()],
        updated_at: 1,
        lane: 0,
        ..Default::default()
    });
    state.tasks.push(TaskRecord {
        marble_id: String::new(),
        id: "generation-fixture-1".into(),
        conversation_id: "durable-fixture-1".into(),
        status: "completed".into(),
        owner: "agent".into(),
        prompt: "list the workspace".into(),
        at: 1,
        routine_id: String::new(),
    });
    state.approvals.push(PendingApproval {
        request_id: "host:fixture-session:1:call-1".into(),
        call_id: "call-1".into(),
        tool: "glass.workspace.list".into(),
        arguments: serde_json::json!({}),
        conflict: None,
    });
    state.transcript.push(TranscriptEntry {
        conversation_id: "durable-fixture-1".into(),
        kind: "assistant".into(),
        text: "handled please list the workspace".into(),
        task_id: "generation-fixture-1".into(),
        at: 1,
        ..Default::default()
    });
    state
}

fn server_with(
    state: ProductState,
    approvals: ApprovalInbox,
    events: EventBus,
    policy: AccessPolicy,
) -> MarbleServer {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    MarbleServer::new(MarbleServerParts {
        listener,
        state: Arc::new(Mutex::new(state)),
        store: None,
        approvals,
        events,
        turn_tx: TurnSender::detached(),
        steer_tx: mpsc::channel().0,
        policy,
        cancels: CancelRegistry::default(),
    })
}

/// Server with a live steering channel, so corrections are observable.
fn server_with_steering(state: ProductState, steer_tx: mpsc::Sender<SteerRequest>) -> MarbleServer {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    MarbleServer::new(MarbleServerParts {
        listener,
        state: Arc::new(Mutex::new(state)),
        store: None,
        approvals: ApprovalInbox::default(),
        events: EventBus::default(),
        turn_tx: TurnSender::detached(),
        steer_tx,
        policy: AccessPolicy::new(None),
        cancels: CancelRegistry::default(),
    })
}

/// Server whose turn channel is kept alive by the caller, so dispatch is observable
/// without a real runtime.
fn server_with_turn_channel(state: ProductState, turn_tx: TurnSender) -> MarbleServer {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    MarbleServer::new(MarbleServerParts {
        listener,
        state: Arc::new(Mutex::new(state)),
        store: None,
        approvals: ApprovalInbox::default(),
        events: EventBus::default(),
        turn_tx,
        steer_tx: mpsc::channel().0,
        policy: AccessPolicy::new(None),
        cancels: CancelRegistry::default(),
    })
}

fn control_of(server: &MarbleServer) -> serde_json::Value {
    let (status, body) = server.handle("GET", "/api/control", "");
    assert_eq!(status, 200, "{body}");
    serde_json::from_str(&body).unwrap()
}

#[test]
fn cancelling_a_paused_message_withdraws_it_before_resume() {
    let (turn_tx, inbox) = TurnInbox::channel();
    let server = server_with_turn_channel(ProductState::default(), turn_tx);
    assert_eq!(server.handle("POST", "/api/control/pause", "").0, 200);
    let (_, body) = server.handle("POST", "/api/turns", r#"{"text":"withdraw this"}"#);
    let queued: Value = serde_json::from_str(&body).unwrap();
    let id = queued["queueId"].as_str().unwrap();
    let (status, body) = server.handle("POST", &format!("/api/tasks/{id}/cancel"), "");
    assert_eq!(status, 200, "{body}");
    assert!(control_of(&server)["queue"].as_array().unwrap().is_empty());
    let (_, body) = server.handle("GET", "/api/state?lean=1", "");
    let state: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(state["tasks"][0]["status"], "cancelled");
    assert_eq!(server.handle("POST", "/api/control/resume", "").0, 200);
    assert!(inbox.try_recv().is_none(), "withdrawn work must not resume");
    assert_eq!(
        server
            .handle("POST", &format!("/api/tasks/{id}/cancel"), "")
            .0,
        409
    );
}

#[test]
fn paused_admission_queues_messages_and_resume_dispatches_in_order() {
    let (turn_tx, inbox) = TurnInbox::channel();
    let server = server_with_turn_channel(ProductState::default(), turn_tx);

    assert_eq!(server.handle("POST", "/api/control/pause", "").0, 200);
    let (status, body) = server.handle("POST", "/api/turns", r#"{"text":"first"}"#);
    assert_eq!(status, 202, "{body}");
    let queued: serde_json::Value = serde_json::from_str(&body).unwrap();
    assert_eq!(queued["queued"], serde_json::json!(true));
    assert_eq!(queued["position"], serde_json::json!(1));
    let (status, body) = server.handle("POST", "/api/turns", r#"{"text":"second"}"#);
    assert_eq!(status, 202, "{body}");
    assert_eq!(
        control_of(&server)["queue"].as_array().map(Vec::len),
        Some(2)
    );
    assert!(
        inbox.try_recv().is_none(),
        "a paused instance must not admit turns"
    );

    let (status, body) = server.handle("POST", "/api/control/resume", "");
    assert_eq!(status, 200, "{body}");
    let resumed: serde_json::Value = serde_json::from_str(&body).unwrap();
    assert_eq!(resumed["dispatched"], serde_json::json!(2));
    let first = inbox
        .try_recv()
        .expect("first queued message is dispatched");
    let second = inbox
        .try_recv()
        .expect("second queued message is dispatched");
    assert_eq!(first.request.text, "first");
    assert_eq!(second.request.text, "second");
    assert_eq!(
        control_of(&server)["queue"].as_array().map(Vec::len),
        Some(0)
    );
}

#[test]
fn control_routes_change_the_profile_and_manage_grants() {
    let server = free_server(ProductState::default());
    assert_eq!(
        control_of(&server)["profile"],
        serde_json::json!("workspace")
    );
    assert_eq!(
        server
            .handle("POST", "/api/control/profile", r#"{"profile":"chaos"}"#)
            .0,
        400
    );

    let (status, body) = server.handle("POST", "/api/control/profile", r#"{"profile":"observe"}"#);
    assert_eq!(status, 200, "{body}");
    let control = control_of(&server);
    assert_eq!(control["profile"], serde_json::json!("observe"));
    assert_eq!(control["profiles"].as_array().map(Vec::len), Some(3));
    assert!(
        control["deniedPaths"]
            .as_array()
            .is_some_and(|rules| !rules.is_empty())
    );

    let (status, body) = server.handle(
        "POST",
        "/api/control/grants",
        r#"{"tool":"glass.workspace.read","pathPrefix":"docs"}"#,
    );
    assert_eq!(status, 200, "{body}");
    let granted: serde_json::Value = serde_json::from_str(&body).unwrap();
    let grant_id = granted["grant"]["id"].as_str().unwrap().to_string();
    assert_eq!(
        control_of(&server)["grants"].as_array().map(Vec::len),
        Some(1)
    );
    assert_eq!(
        server
            .handle(
                "POST",
                "/api/control/grants",
                r#"{"tool":"shell","pathPrefix":"/"}"#
            )
            .0,
        400
    );

    let (status, _) = server.handle(
        "POST",
        &format!("/api/control/grants/{grant_id}/revoke"),
        "",
    );
    assert_eq!(status, 200);
    assert_eq!(
        control_of(&server)["grants"].as_array().map(Vec::len),
        Some(0)
    );
    assert_eq!(
        server
            .handle(
                "POST",
                &format!("/api/control/grants/{grant_id}/revoke"),
                ""
            )
            .0,
        404
    );
}

#[test]
fn allowing_an_approval_with_remember_leaves_a_revocable_grant() {
    let approvals = ApprovalInbox::default();
    let server = server_with(
        ProductState::default(),
        approvals.clone(),
        EventBus::default(),
        AccessPolicy::new(None),
    );
    approvals.register(
        "host:fixture-session:1:call-1",
        "call-1",
        "glass.workspace.read",
        serde_json::json!({"path": "docs/notes.md"}),
        None,
    );
    let (status, body) = server.handle(
        "POST",
        "/api/approvals/host:fixture-session:1:call-1/allow",
        r#"{"remember":true}"#,
    );
    assert_eq!(status, 200, "{body}");
    let control = control_of(&server);
    let grant = control["grants"][0].clone();
    assert_eq!(grant["tool"], serde_json::json!("glass.workspace.read"));
    assert_eq!(grant["pathPrefix"], serde_json::json!("docs"));
}

fn free_server(state: ProductState) -> MarbleServer {
    server_with(
        state,
        ApprovalInbox::default(),
        EventBus::default(),
        AccessPolicy::new(None),
    )
}

#[test]
fn product_state_encoding_round_trips_through_json() {
    let state = test_state();
    let encoded = state.encode();
    let decoded = ProductState::decode(&encoded).expect("state should decode");
    assert_eq!(state, decoded);
    assert_eq!(
        encoded["conversations"][0]["taskIds"],
        serde_json::json!(["generation-fixture-1"])
    );
    assert_eq!(encoded["revision"], serde_json::json!(1));
}

#[test]
fn server_approval_inbox_routes_allow_and_deny() {
    let approvals = ApprovalInbox::default();
    let server = server_with(
        test_state(),
        approvals.clone(),
        EventBus::default(),
        AccessPolicy::new(None),
    );
    approvals.register(
        "host:fixture-session:generation-fixture-1:call-1",
        "call-1",
        "glass.workspace.list",
        serde_json::json!({}),
        None,
    );
    approvals.register(
        "host:fixture-session:generation-fixture-1:call-2",
        "call-2",
        "glass.workspace.list",
        serde_json::json!({}),
        None,
    );
    let (status, _) = server.handle(
        "POST",
        "/api/approvals/host:fixture-session:generation-fixture-1:call-1/deny",
        "",
    );
    assert_eq!(status, 200);
    let (status, _) = server.handle(
        "POST",
        "/api/approvals/host:fixture-session:generation-fixture-1:call-1/deny",
        "",
    );
    assert_eq!(status, 409);
    // Clients percent-escape the `:` in request ids; the route decodes them.
    let (status, _) = server.handle(
        "POST",
        "/api/approvals/host%3Afixture-session%3Ageneration-fixture-1%3Acall-2/allow",
        "",
    );
    assert_eq!(status, 200);
    assert_eq!(server.handle("GET", "/api/nope", "").0, 404);
}

#[test]
fn server_state_route_serves_live_pending_approvals() {
    let approvals = ApprovalInbox::default();
    let server = server_with(
        ProductState::default(),
        approvals.clone(),
        EventBus::default(),
        AccessPolicy::new(None),
    );
    approvals.register(
        "host:fixture-session:generation-fixture-1:call-1",
        "call-1",
        "glass.workspace.list",
        serde_json::json!({}),
        None,
    );
    let (status, body) = server.handle("GET", "/api/state", "");
    assert_eq!(status, 200);
    let value: serde_json::Value = serde_json::from_str(&body).unwrap();
    assert_eq!(value["approvals"].as_array().map(Vec::len), Some(1));
    assert_eq!(
        value["approvals"][0]["requestId"],
        serde_json::json!("host:fixture-session:generation-fixture-1:call-1")
    );
}

#[test]
fn token_protected_state_requires_login_and_accepts_the_cookie() {
    let server = server_with(
        test_state(),
        ApprovalInbox::default(),
        EventBus::default(),
        AccessPolicy::new(Some("s3cret".into())),
    );
    let (status, body) = server.handle("GET", "/api/state", "");
    assert_eq!(status, 401);
    assert!(body.contains("unauthorized"));

    let (status, _) = server.handle("POST", "/api/login", r#"{"token":"wrong"}"#);
    assert_eq!(status, 401);

    let response = server.handle_request(
        &RequestHead::new("POST", "/api/login"),
        r#"{"token":"s3cret"}"#,
    );
    assert_eq!(response.status, 200);
    assert!(
        response
            .headers
            .iter()
            .any(|(name, value)| name == "Set-Cookie" && value.contains("marble_token=s3cret"))
    );

    let authorized = server.handle_request(
        &RequestHead::new("GET", "/api/state").with_header("cookie", "marble_token=s3cret"),
        "",
    );
    assert_eq!(authorized.status, 200);
}

#[test]
fn cross_origin_mutations_are_refused() {
    let server = free_server(test_state());
    let response = server.handle_request(
        &RequestHead::new("POST", "/api/turns")
            .with_header("host", "100.64.0.5:4783")
            .with_header("origin", "https://attacker.example"),
        r#"{"text":"hi"}"#,
    );
    assert_eq!(response.status, 403);
}

#[test]
fn static_page_and_health_are_served() {
    let server = free_server(ProductState::default());
    let page = server.handle_request(&RequestHead::new("GET", "/"), "");
    assert_eq!(page.status, 200);
    assert!(page.body.contains("Glass Marble"));
    assert!(page.body.contains("href=\"/app.css\""));
    assert!(page.body.contains("src=\"/app.js\""));
    assert!(page.body.contains("src=\"/logic.js\""));
    assert!(!page.body.contains("<style>"));
    assert!(!page.body.contains("<script>"));
    let health = server.handle_request(&RequestHead::new("GET", "/healthz"), "");
    assert_eq!(health.status, 200);
    assert!(health.body.contains("\"ok\":true"));
}

#[test]
fn static_assets_have_types_cache_policy_and_do_not_require_unlocking() {
    let server = MarbleServer::new(MarbleServerParts {
        listener: TcpListener::bind("127.0.0.1:0").unwrap(),
        state: Arc::new(Mutex::new(ProductState::default())),
        store: None,
        approvals: ApprovalInbox::default(),
        events: EventBus::default(),
        turn_tx: TurnSender::detached(),
        steer_tx: mpsc::channel().0,
        policy: AccessPolicy::new(Some("secret".into())),
        cancels: CancelRegistry::default(),
    });
    for (path, kind, marker) in [
        (
            "/app.css",
            "text/css; charset=utf-8",
            "prefers-color-scheme",
        ),
        ("/app.js", "text/javascript; charset=utf-8", "EventSource"),
        ("/logic.js", "text/javascript; charset=utf-8", "whoAnswers"),
    ] {
        let response = server.handle_request(&RequestHead::new("GET", path), "");
        assert_eq!(response.status, 200, "{path}");
        assert!(response.body.contains(marker));
        assert!(
            response
                .headers
                .iter()
                .any(|(key, value)| key == "Content-Type" && value == kind)
        );
        assert!(
            response
                .headers
                .iter()
                .any(|(key, value)| key == "Cache-Control" && value == "no-cache")
        );
        assert!(
            response
                .headers
                .iter()
                .any(|(key, value)| key == "X-Content-Type-Options" && value == "nosniff")
        );
    }
    assert_eq!(server.handle("GET", "/api/state", "").0, 401);
    let logout = server.handle_request(&RequestHead::new("POST", "/api/logout"), "{}");
    assert_eq!(logout.status, 200);
    assert!(
        logout
            .headers
            .iter()
            .any(|(key, value)| key == "Set-Cookie" && value.contains("Max-Age=0"))
    );
    let cross_origin = RequestHead::new("POST", "/api/logout")
        .with_header("Origin", "https://other.example")
        .with_header("Host", "localhost");
    assert_eq!(server.handle_request(&cross_origin, "{}").status, 403);
    assert_eq!(
        free_server(ProductState::default())
            .handle("GET", "/missing.js", "")
            .0,
        404
    );
}

#[test]
fn oversized_bodies_are_rejected_before_reading() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let server = MarbleServer::new(MarbleServerParts {
        listener,
        state: Arc::new(Mutex::new(ProductState::default())),
        store: None,
        approvals: ApprovalInbox::default(),
        events: EventBus::default(),
        turn_tx: TurnSender::detached(),
        steer_tx: mpsc::channel().0,
        policy: AccessPolicy::new(None),
        cancels: CancelRegistry::default(),
    });
    std::thread::spawn(move || {
        let _ = server.serve();
    });
    let mut stream = connect(port);
    stream
        .write_all(
            b"POST /api/turns HTTP/1.1\r\nHost: 127.0.0.1\r\nContent-Length: 9999999\r\n\r\n",
        )
        .unwrap();
    let mut response = String::new();
    stream.read_to_string(&mut response).unwrap();
    assert!(response.starts_with("HTTP/1.1 413"), "{response}");
}

#[test]
fn explicit_shared_memory_owner_and_edits_preserve_scope() {
    let server = free_server(ProductState::default());
    for (body, owner) in [
        (r#"{"text":"Shared preference", "owner":""}"#, ""),
        (r#"{"text":"Default preference"}"#, "marble"),
        (
            r#"{"text":"Private preference", "owner":"marble"}"#,
            "marble",
        ),
    ] {
        let (status, body) = server.handle("POST", "/api/memory", body);
        assert_eq!(status, 200, "{body}");
        let value: Value = serde_json::from_str(&body).unwrap();
        assert_eq!(value["remembered"]["owner"], owner);
        let id = value["remembered"]["id"].as_str().unwrap();
        let (status, updated) = server.handle(
            "POST",
            &format!("/api/memory/{id}/update"),
            &serde_json::json!({"text": format!("Edited {id}")}).to_string(),
        );
        assert_eq!(status, 200);
        let updated: Value = serde_json::from_str(&updated).unwrap();
        assert_eq!(updated["remembered"]["owner"], owner);
    }
    assert_eq!(
        server
            .handle(
                "POST",
                "/api/memory",
                r#"{"text":"Bad", "owner":"unknown"}"#
            )
            .0,
        400
    );
    assert_eq!(
        server
            .handle("POST", "/api/memory", r#"{"text":"", "owner":""}"#)
            .0,
        400
    );
}

#[test]
fn chunked_request_bodies_are_decoded() {
    // A reverse proxy may forward a login chunked even though the browser sent
    // a length, so the body has to arrive either way.
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let server = MarbleServer::new(MarbleServerParts {
        listener,
        state: Arc::new(Mutex::new(ProductState::default())),
        store: None,
        approvals: ApprovalInbox::default(),
        events: EventBus::default(),
        turn_tx: TurnSender::detached(),
        steer_tx: mpsc::channel().0,
        policy: AccessPolicy::new(Some("s3cret".into())),
        cancels: CancelRegistry::default(),
    });
    std::thread::spawn(move || {
        let _ = server.serve();
    });
    let body = r#"{"token":"s3cret"}"#;
    let mut stream = connect(port);
    stream
        .write_all(
            format!(
                "POST /api/login HTTP/1.1\r\nHost: 127.0.0.1\r\n\
                 Content-Type: application/json\r\nTransfer-Encoding: chunked\r\n\
                 Connection: close\r\n\r\n{:x}\r\n{body}\r\n0\r\n\r\n",
                body.len()
            )
            .as_bytes(),
        )
        .unwrap();
    let mut response = String::new();
    stream.read_to_string(&mut response).unwrap();
    assert!(response.starts_with("HTTP/1.1 200"), "{response}");
    assert!(response.contains("marble_token=s3cret"), "{response}");
}

#[test]
fn event_stream_greets_a_quiet_connection_immediately() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let server = MarbleServer::new(MarbleServerParts {
        listener,
        state: Arc::new(Mutex::new(ProductState::default())),
        store: None,
        approvals: ApprovalInbox::default(),
        events: EventBus::default(),
        turn_tx: TurnSender::detached(),
        steer_tx: mpsc::channel().0,
        policy: AccessPolicy::new(None),
        cancels: CancelRegistry::default(),
    });
    std::thread::spawn(move || {
        let _ = server.serve();
    });
    let mut stream = connect(port);
    stream
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    stream
        .write_all(b"GET /api/events HTTP/1.1\r\nHost: 127.0.0.1\r\n\r\n")
        .unwrap();
    let mut response = String::new();
    let mut buffer = [0; 1024];
    while !response.contains(": connected\n\n") {
        let read = stream.read(&mut buffer).unwrap();
        assert_ne!(read, 0);
        response.push_str(&String::from_utf8_lossy(&buffer[..read]));
    }
    assert!(response.contains("200 OK"));
}

#[test]
fn health_and_cli_report_the_embedded_host_build() {
    let build = glass_marble::build_info::document();
    assert_eq!(build["version"], env!("CARGO_PKG_VERSION"));
    let commit = build["gitCommit"].as_str().unwrap();
    let short = build["gitCommitShort"].as_str().unwrap();
    if commit == "unknown" {
        assert_eq!(short, "unknown");
    } else {
        assert!(matches!(commit.len(), 40 | 64));
        assert!(commit.bytes().all(|byte| byte.is_ascii_hexdigit()));
        assert_eq!(short, &commit[..12]);
    }
    let server = free_server(ProductState::default());
    let (status, body) = server.handle("GET", "/healthz", "");
    assert_eq!(status, 200, "{body}");
    let health: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(health["build"], build);
    assert_eq!(health["version"], build["version"]);
    let output = Command::new(env!("CARGO_BIN_EXE_glass-marble"))
        .arg("--version")
        .output()
        .unwrap();
    assert!(output.status.success());
    assert_eq!(
        String::from_utf8(output.stdout).unwrap().trim(),
        format!("glass-marble {}", glass_marble::build_info::CLI_VERSION)
    );
}

#[test]
fn event_stream_replays_from_a_cursor() {
    let events = EventBus::default();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let server = MarbleServer::new(MarbleServerParts {
        listener,
        state: Arc::new(Mutex::new(ProductState::default())),
        store: None,
        approvals: ApprovalInbox::default(),
        events: events.clone(),
        turn_tx: TurnSender::detached(),
        steer_tx: mpsc::channel().0,
        policy: AccessPolicy::new(None),
        cancels: CancelRegistry::default(),
    });
    std::thread::spawn(move || {
        let _ = server.serve();
    });
    events.publish(
        "durable-fixture-1",
        "generation-fixture-1",
        "text",
        serde_json::json!({"text": "streamed"}),
    );
    for cursor in [0, 999] {
        let mut stream = connect(port);
        write!(stream, "GET /api/events?since={cursor} HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n").unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        let mut response = String::new();
        let mut buffer = [0u8; 4096];
        while !response.contains("streamed") {
            match stream.read(&mut buffer) {
                Ok(0) => break,
                Ok(read) => response.push_str(&String::from_utf8_lossy(&buffer[..read])),
                Err(_) => break,
            }
        }
        assert!(response.contains("text/event-stream"), "{response}");
        let hello = response
            .split("event: hello\ndata: ")
            .nth(1)
            .unwrap()
            .lines()
            .next()
            .unwrap();
        let hello: Value = serde_json::from_str(hello).unwrap();
        assert_eq!(hello["cursor"], 1);
        assert_eq!(hello["build"], glass_marble::build_info::document());
        assert!(response.contains("event: text"), "{response}");
        assert!(response.contains("streamed"), "{response}");
    }
}

#[test]
fn think_blocks_are_split_in_sse_journal_and_persisted_reply() {
    if !python3_available() {
        return;
    }
    let root = std::env::temp_dir().join(format!("marble-think-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let store = Arc::new(StateStore::in_data_dir(&root));
    let events = EventBus::default();
    let state = Arc::new(Mutex::new(ProductState::default()));
    let mut python = Command::new("python3");
    python.arg(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/fake_glass_core_runtime.py"))
        .arg("runtime").arg("--text-chunks")
        .arg(r#"["visible ","<th","ink>secret"," thought</think>","answer","<think>unclosed", " hidden"]"#);
    let conversation = Conversation::spawn_with_router(
        python,
        &root,
        &root.join("sessions"),
        HostPolicy::approving(),
        ApprovalInbox::default(),
        ToolRouter::new(&root).unwrap(),
        events.clone(),
    )
    .unwrap();
    let turn_tx = spawn_single_lane(
        conversation,
        state.clone(),
        Some(store.clone()),
        LiveStore::in_data_dir(&root),
        events.clone(),
        Duration::from_secs(30),
        CancelRegistry::default(),
    );
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let server = MarbleServer::new(MarbleServerParts {
        listener,
        state: state.clone(),
        store: Some(store.clone()),
        approvals: ApprovalInbox::default(),
        events: events.clone(),
        turn_tx,
        steer_tx: mpsc::channel().0,
        policy: AccessPolicy::new(None),
        cancels: CancelRegistry::default(),
    });
    let (status, body) = server.handle("POST", "/api/turns", r#"{"text":"hello"}"#);
    assert_eq!(status, 202, "{body}");
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut journal_checked = false;
    loop {
        for path in std::fs::read_dir(root.join("live")).unwrap() {
            if let Ok(bytes) = std::fs::read(path.unwrap().path())
                && let Ok(record) = serde_json::from_slice::<Value>(&bytes)
                && record["reasoning"]
                    .as_str()
                    .is_some_and(|s| s.contains("secret"))
            {
                assert!(!record["text"].as_str().unwrap().contains("secret"));
                assert!(!record["text"].as_str().unwrap().contains("<think"));
                journal_checked = true;
            }
        }
        if state
            .lock()
            .unwrap()
            .tasks
            .iter()
            .any(|task| task.status == "completed")
        {
            break;
        }
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(20));
    }
    assert!(journal_checked);
    let deadline = Instant::now() + Duration::from_secs(5);
    let saved = loop {
        let saved = store.load().unwrap();
        if saved.transcript.iter().any(|e| e.kind == "assistant") {
            break saved;
        }
        assert!(Instant::now() < deadline);
        std::thread::yield_now();
    };
    assert_eq!(
        saved
            .transcript
            .iter()
            .find(|e| e.kind == "assistant")
            .unwrap()
            .text,
        "visible answer"
    );
    assert_eq!(
        saved
            .transcript
            .iter()
            .find(|e| e.kind == "reasoning")
            .unwrap()
            .text,
        "secret thoughtunclosed hidden"
    );
    std::thread::spawn(move || {
        let _ = server.serve();
    });
    let mut stream = connect(port);
    stream
        .write_all(b"GET /api/events?since=0 HTTP/1.1\r\nHost: 127.0.0.1\r\n\r\n")
        .unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    let mut response = String::new();
    let mut buffer = [0; 4096];
    while !response.contains("\"status\":\"completed\"") {
        let read = stream.read(&mut buffer).unwrap();
        assert_ne!(read, 0);
        response.push_str(&String::from_utf8_lossy(&buffer[..read]));
    }
    assert!(!response.contains("<think"), "{response}");
    let frames: Vec<Value> = response
        .lines()
        .filter_map(|line| line.strip_prefix("data: "))
        .map(|s| serde_json::from_str(s).unwrap())
        .collect();
    let text: String = frames
        .iter()
        .filter(|f| f["kind"] == "text")
        .map(|f| f["data"]["text"].as_str().unwrap())
        .collect();
    let reasoning: String = frames
        .iter()
        .filter(|f| f["kind"] == "reasoning")
        .map(|f| f["data"]["text"].as_str().unwrap())
        .collect();
    assert_eq!(text, "visible answer");
    assert_eq!(reasoning, "secret thoughtunclosed hidden");
}

#[test]
fn server_turn_route_admits_a_durable_turn_and_tracks_it() {
    if !python3_available() {
        return;
    }
    let root = std::env::temp_dir().join(format!("glass-marble-server-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    let script = root.join("fake_glass_core_runtime.py");
    std::fs::write(&script, include_str!("fixtures/fake_glass_core_runtime.py")).unwrap();
    let storage_dir = root.join("sessions");

    let events = EventBus::default();
    let state = Arc::new(Mutex::new(ProductState::default()));
    let mut python = Command::new("python3");
    python.arg(&script).arg("runtime");
    let conversation = Conversation::spawn_with_router(
        python,
        &root,
        &storage_dir,
        HostPolicy::approving(),
        ApprovalInbox::default(),
        ToolRouter::new(&root).unwrap(),
        events.clone(),
    )
    .unwrap();
    let turn_tx = spawn_single_lane(
        conversation,
        Arc::clone(&state),
        None,
        LiveStore::in_data_dir(&root),
        events.clone(),
        Duration::from_secs(60),
        CancelRegistry::default(),
    );
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let server = MarbleServer::new(MarbleServerParts {
        listener,
        state: Arc::clone(&state),
        store: None,
        approvals: ApprovalInbox::default(),
        events,
        turn_tx,
        steer_tx: mpsc::channel().0,
        policy: AccessPolicy::new(None),
        cancels: CancelRegistry::default(),
    });

    let (status, body) = server.handle(
        "POST",
        "/api/turns",
        r#"{"text":"please list the workspace"}"#,
    );
    assert_eq!(status, 202, "{body}");
    let accepted: serde_json::Value = serde_json::from_str(&body).unwrap();
    assert_eq!(accepted["accepted"], serde_json::json!(true));
    let task_id = accepted["taskId"].as_str().unwrap().to_string();

    let deadline = Instant::now() + Duration::from_secs(45);
    loop {
        let (status, body) = server.handle("GET", "/api/state", "");
        assert_eq!(status, 200);
        let state: serde_json::Value = serde_json::from_str(&body).unwrap();
        let task = state["tasks"]
            .as_array()
            .unwrap()
            .iter()
            .find(|task| task["id"] == serde_json::json!(task_id))
            .cloned();
        if let Some(task) = task
            && task["status"] == serde_json::json!("completed")
        {
            assert_eq!(task["owner"], serde_json::json!("agent"));
            assert_eq!(
                state["tasks"].as_array().map(Vec::len),
                Some(1),
                "the queued placeholder is replaced by the real task: {body}"
            );
            assert_eq!(state["transcript"].as_array().map(Vec::len), Some(2));
            assert_eq!(state["transcript"][0]["kind"], serde_json::json!("user"));
            assert_eq!(
                state["transcript"][1]["text"],
                serde_json::json!("handled please list the workspace")
            );
            break;
        }
        assert!(Instant::now() < deadline, "turn did not settle: {body}");
        std::thread::sleep(Duration::from_millis(50));
    }

    // The turn has settled, so the worker no longer holds a cancel flag for it.
    let (status, _) = server.handle("POST", &format!("/api/tasks/{task_id}/cancel"), "");
    assert_eq!(status, 409);
    let (status, _) = server.handle("POST", "/api/tasks/not-a-task/cancel", "");
    assert_eq!(status, 404);

    let _ = std::fs::remove_dir_all(&root);
}

/// A turn cancelled before it replies must never leak its prompt into the
/// next turn: the engine's newest input is the follow-up, and the settled
/// reply belongs to the follow-up's own task.
#[test]
fn cancelled_turn_does_not_steal_the_next_turns_reply() {
    if !python3_available() {
        return;
    }
    let root = std::env::temp_dir().join(format!("glass-marble-cancel-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    let script = root.join("fake_glass_core_runtime.py");
    std::fs::write(&script, include_str!("fixtures/fake_glass_core_runtime.py")).unwrap();
    let storage_dir = root.join("sessions");

    let events = EventBus::default();
    let state = Arc::new(Mutex::new(ProductState::default()));
    let mut python = Command::new("python3");
    python.arg(&script).arg("runtime").arg("--late-abort-ack");
    let conversation = Conversation::spawn_with_router(
        python,
        &root,
        &storage_dir,
        HostPolicy::approving(),
        ApprovalInbox::default(),
        ToolRouter::new(&root).unwrap(),
        events.clone(),
    )
    .unwrap();
    let cancels = CancelRegistry::default();
    let turn_tx = spawn_single_lane(
        conversation,
        Arc::clone(&state),
        None,
        LiveStore::in_data_dir(&root),
        events.clone(),
        Duration::from_secs(60),
        cancels.clone(),
    );
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let server = MarbleServer::new(MarbleServerParts {
        listener,
        state: Arc::clone(&state),
        store: None,
        approvals: ApprovalInbox::default(),
        events,
        turn_tx,
        steer_tx: mpsc::channel().0,
        policy: AccessPolicy::new(None),
        cancels,
    });

    let (status, body) = server.handle("POST", "/api/turns", r#"{"text":"first"}"#);
    assert_eq!(status, 202, "{body}");
    let first_task = serde_json::from_str::<Value>(&body).unwrap()["taskId"]
        .as_str()
        .unwrap()
        .to_string();
    let conversation_id = serde_json::from_str::<Value>(&body).unwrap()["conversationId"]
        .as_str()
        .unwrap()
        .to_string();
    // Wait for the turn to be on the engine, then cancel it mid-run.
    let mut cancelled = false;
    let mut last_status: Option<String> = None;
    for _ in 0..80 {
        let (_, body) = server.handle("GET", "/api/state", "");
        let doc: Value = serde_json::from_str(&body).unwrap();
        let running = doc["tasks"]
            .as_array()
            .unwrap()
            .iter()
            .any(|task| task["id"] == first_task.as_str() && task["status"] == "running");
        if running {
            let (status, body) =
                server.handle("POST", &format!("/api/tasks/{first_task}/cancel"), "");
            if status == 200 || status == 202 {
                cancelled = true;
                break;
            }
            last_status = Some(format!("{status} {body}"));
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    assert!(
        cancelled,
        "the running turn could not be cancelled: {last_status:?}"
    );

    // Whatever the cancelled turn still asks for is denied, so it unwinds.
    let deadline = Instant::now() + Duration::from_secs(30);
    let mut first_settled = false;
    while Instant::now() < deadline {
        let (_, body) = server.handle("GET", "/api/state", "");
        let doc: Value = serde_json::from_str(&body).unwrap();
        if let Some(pending) = doc["approvals"].as_array() {
            for approval in pending {
                let id = approval["requestId"].as_str().unwrap();
                let encoded = id.replace(':', "%3A");
                server.handle("POST", &format!("/api/approvals/{encoded}/deny"), "");
            }
        }
        let task = doc["tasks"]
            .as_array()
            .unwrap()
            .iter()
            .find(|task| task["id"] == first_task.as_str())
            .cloned();
        if let Some(task) = task
            && task["status"] != "running"
        {
            first_settled = true;
            break;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    assert!(first_settled, "cancelled turn never settled");

    // The follow-up is what the engine must see next — not the stale prompt.
    let (status, body) = server.handle(
        "POST",
        "/api/turns",
        &json!({"text":"hello", "conversationId": conversation_id}).to_string(),
    );
    assert_eq!(status, 202, "{body}");
    let hello_task = serde_json::from_str::<Value>(&body).unwrap()["taskId"]
        .as_str()
        .unwrap()
        .to_string();

    let deadline = Instant::now() + Duration::from_secs(45);
    loop {
        let (_, body) = server.handle("GET", "/api/state", "");
        let doc: Value = serde_json::from_str(&body).unwrap();
        if let Some(pending) = doc["approvals"].as_array() {
            for approval in pending {
                let id = approval["requestId"].as_str().unwrap();
                let encoded = id.replace(':', "%3A");
                server.handle("POST", &format!("/api/approvals/{encoded}/allow"), "");
            }
        }
        let entries = doc["transcript"].as_array().unwrap();
        let done = entries
            .iter()
            .any(|entry| entry["taskId"] == hello_task.as_str() && entry["kind"] == "assistant");
        if done {
            let submitted: Value = serde_json::from_slice(
                &std::fs::read(storage_dir.join("fixture-input.json")).unwrap(),
            )
            .unwrap();
            assert_eq!(submitted["conversationId"], conversation_id);
            assert_eq!(submitted["input"].as_object().unwrap().len(), 1);
            let engine_text = submitted["input"]["text"].as_str().unwrap();
            assert!(engine_text.ends_with("[User message]\nhello"));
            assert!(engine_text.contains("withdrawn"));
            assert!(engine_text.contains("Current date and time:"));
            let user_for_hello = entries
                .iter()
                .find(|entry| entry["taskId"] == hello_task.as_str() && entry["kind"] == "user");
            assert_eq!(
                user_for_hello.map(|e| e["text"].clone()),
                Some(Value::from("hello")),
                "the engine's newest input was not the follow-up: {body}"
            );
            let reply = entries.iter().find(|entry| {
                entry["taskId"] == hello_task.as_str() && entry["kind"] == "assistant"
            });
            assert_eq!(
                reply.map(|e| e["text"].clone()),
                Some(Value::from("handled hello")),
                "the reply was attributed to the wrong turn: {body}"
            );
            break;
        }
        assert!(Instant::now() < deadline, "hello never replied: {body}");
        std::thread::sleep(Duration::from_millis(100));
    }

    let _ = std::fs::remove_dir_all(&root);
}

fn connect(port: u16) -> TcpStream {
    for _ in 0..100 {
        if let Ok(stream) = TcpStream::connect(("127.0.0.1", port)) {
            return stream;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    panic!("local server should accept connections");
}

#[test]
fn presence_reports_an_always_on_assistant() {
    let mut state = ProductState::default();
    state.presence.mark_start();
    let server = free_server(state);
    let (status, body) = server.handle("GET", "/api/presence", "");
    assert_eq!(status, 200, "{body}");
    let presence: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(presence["status"], json!("idle"));
    assert!(presence["uptimeSeconds"].as_u64().is_some());
    assert!(
        presence["instance"]
            .as_str()
            .is_some_and(|name| !name.is_empty())
    );

    let (_, state_body) = server.handle("GET", "/api/state", "");
    let state_document: Value = serde_json::from_str(&state_body).unwrap();
    assert_eq!(state_document["presence"]["status"], json!("idle"));
    assert!(
        state_document["routines"]["limit"]
            .as_u64()
            .unwrap_or_default()
            > 0
    );
}

#[test]
fn explicit_memory_commands_are_remembered_without_a_model_turn() {
    let (turn_tx, inbox) = TurnInbox::channel();
    let server = server_with_turn_channel(ProductState::default(), turn_tx);
    let (status, body) = server.handle(
        "POST",
        "/api/turns",
        r#"{"text":"Remember this: my name is Azhar","conversationId":"durable-fixture-1"}"#,
    );
    assert_eq!(status, 200, "{body}");
    let value: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(value["remembered"]["kind"], json!("role"));
    assert!(
        inbox.try_recv().is_none(),
        "a memory command must not become a model turn"
    );

    let (status, body) = server.handle("GET", "/api/memory", "");
    assert_eq!(status, 200);
    let memory: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(memory["facts"][0]["text"], json!("my name is Azhar"));

    let (status, body) = server.handle(
        "POST",
        "/api/turns",
        r#"{"text":"forget: Azhar","conversationId":"durable-fixture-1"}"#,
    );
    assert_eq!(status, 200, "{body}");
    let forgotten: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(forgotten["forgotten"], json!(1));
}

#[test]
fn chat_forget_commands_only_remove_the_target_owners_private_memories() {
    for target in ["named-id", "named-chat", "default-new", "default-chat"] {
        let mut state = ProductState::default();
        let ada = state
            .prism
            .add("Ada", "writer", "star", 35, "", "")
            .unwrap()
            .id
            .clone();
        let ben = state
            .prism
            .add("Ben", "reviewer", "hex", 210, "", "")
            .unwrap()
            .id
            .clone();
        state.prism.find_mut(&ada).unwrap().conversation_id = "ada-chat".into();
        for owner in [&ada, &ben, "marble", ""] {
            state
                .memory
                .remember("office wifi password", "fact", "", true, owner)
                .unwrap();
        }
        let server = free_server(state);
        let body = match target {
            "named-id" => json!({"text":"forget: WIFI", "marbleId":ada}),
            "named-chat" => json!({"text":"forget: WIFI", "conversationId":"ada-chat"}),
            "default-chat" => json!({"text":"forget: WIFI", "conversationId":"default-chat"}),
            _ => json!({"text":"forget: WIFI"}),
        };
        let (status, body) = server.handle("POST", "/api/turns", &body.to_string());
        assert_eq!(status, 200, "{body}");
        let result: Value = serde_json::from_str(&body).unwrap();
        assert_eq!(result["forgotten"], 1, "{target}: {result}");
        let state = server.state();
        let owner = if target.starts_with("named") {
            &ada
        } else {
            "marble"
        };
        assert!(!state.memory.facts.iter().any(|fact| fact.owner == owner));
        assert_eq!(state.memory.facts.len(), 3);
        assert!(state.memory.facts.iter().any(|fact| fact.owner.is_empty()));
        assert_eq!(
            server
                .handle(
                    "POST",
                    "/api/turns",
                    r#"{"text":"forget: wifi","marbleId":"missing"}"#
                )
                .0,
            404
        );
        assert_eq!(server.state().memory.facts.len(), 3);
    }
}

#[test]
fn memory_suggestions_need_an_explicit_decision() {
    let mut state = ProductState::default();
    let suggestion = state
        .memory
        .suggest(
            "I prefer short commit messages",
            "preference",
            "noticed",
            "c1",
        )
        .expect("a suggestion");
    let server = free_server(state);
    let (status, _) = server.handle("POST", "/api/memory/bogus/accept", "");
    assert_eq!(status, 404);
    let (status, body) =
        server.handle("POST", &format!("/api/memory/{}/accept", suggestion.id), "");
    assert_eq!(status, 200, "{body}");
    let value: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(
        value["remembered"]["text"],
        json!("I prefer short commit messages")
    );
    assert_eq!(value["memory"]["count"], json!(1));
    assert!(
        value["memory"]["suggestions"]
            .as_array()
            .unwrap()
            .is_empty()
    );
}

#[test]
fn routines_can_be_added_run_paused_and_removed() {
    let (turn_tx, inbox) = TurnInbox::channel();
    let server = server_with_turn_channel(ProductState::default(), turn_tx);
    let (status, body) = server.handle(
        "POST",
        "/api/routines",
        r#"{"title":"Tidy","prompt":"tidy the notes","schedule":"every:30m","conversationId":"durable-fixture-1"}"#,
    );
    assert_eq!(status, 200, "{body}");
    let created: Value = serde_json::from_str(&body).unwrap();
    let routine_id = created["routine"]["id"].as_str().unwrap().to_string();
    assert_eq!(created["routines"]["enabled"], json!(1));

    let (status, body) = server.handle("POST", &format!("/api/routines/{routine_id}/run"), "");
    assert_eq!(status, 202, "{body}");
    let command = inbox.try_recv().expect("the routine is admitted");
    assert_eq!(
        command.request.routine_id.as_deref(),
        Some(routine_id.as_str())
    );
    assert_eq!(command.request.text, "tidy the notes");

    // A paused assistant queues a firing with its routine, so the queue panel
    // can name the routine and say it is held rather than admit it silently.
    assert_eq!(server.handle("POST", "/api/control/pause", "").0, 200);
    let (status, body) = server.handle("POST", &format!("/api/routines/{routine_id}/run"), "");
    assert_eq!(status, 202, "{body}");
    let control = control_of(&server);
    assert_eq!(control["queue"][0]["routineId"], json!(routine_id));
    assert_eq!(server.handle("POST", "/api/control/resume", "").0, 200);
    let resumed = inbox
        .try_recv()
        .expect("resuming dispatches the queued firing")
        .request;
    assert_eq!(resumed.text, "tidy the notes");
    assert_eq!(
        resumed.routine_id.as_deref(),
        Some(routine_id.as_str()),
        "a held firing resumes as a firing, not as a typed message"
    );
    assert_eq!(resumed.lane(), TurnLane::Routine);
    assert!(
        resumed.routine_counted,
        "the run was counted when admission held it, so admitting it must not count again"
    );

    assert_eq!(
        server
            .handle(
                "POST",
                "/api/routines",
                r#"{"prompt":"x","schedule":"never"}"#
            )
            .0,
        400
    );
    assert_eq!(
        server
            .handle("POST", &format!("/api/routines/{routine_id}/disable"), "")
            .0,
        200
    );
    let (_, body) = server.handle("GET", "/api/routines", "");
    let routines: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(routines["enabled"], json!(0));
    assert_eq!(
        server
            .handle("POST", &format!("/api/routines/{routine_id}/remove"), "")
            .0,
        200
    );
    assert_eq!(server.handle("POST", "/api/routines/nope/run", "").0, 404);
}

#[test]
fn resuming_admits_typed_messages_before_held_firings() {
    let (turn_tx, inbox) = TurnInbox::channel();
    let server = server_with_turn_channel(ProductState::default(), turn_tx);
    let (_, body) = server.handle(
        "POST",
        "/api/routines",
        r#"{"title":"Sweep","prompt":"sweep the notes","schedule":"every:1h"}"#,
    );
    let routine_id = serde_json::from_str::<Value>(&body).unwrap()["routine"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    assert_eq!(server.handle("POST", "/api/control/pause", "").0, 200);
    // The firing is held first, so a plain arrival-order drain would admit it
    // ahead of the message the person typed.
    assert_eq!(
        server
            .handle("POST", &format!("/api/routines/{routine_id}/run"), "")
            .0,
        202
    );
    assert_eq!(
        server
            .handle("POST", "/api/turns", r#"{"text":"typed while paused"}"#)
            .0,
        202
    );
    assert_eq!(server.handle("POST", "/api/control/resume", "").0, 200);

    let typed = inbox
        .try_recv()
        .expect("the typed message is admitted first")
        .request;
    assert_eq!(typed.text, "typed while paused");
    assert_eq!(typed.lane(), TurnLane::Human);
    let firing = inbox.try_recv().expect("the held firing follows");
    assert_eq!(
        firing.request.routine_id.as_deref(),
        Some(routine_id.as_str())
    );
    assert_eq!(firing.request.lane(), TurnLane::Routine);
    assert!(firing.request.routine_counted);

    // While it waits in the routine lane it is still a durable row, so the panel
    // and presence can name it instead of losing sight of it on resume.
    let (_, body) = server.handle("GET", "/api/state?lean=1", "");
    let state: Value = serde_json::from_str(&body).unwrap();
    let waiting = state["tasks"]
        .as_array()
        .unwrap()
        .iter()
        .find(|task| task["status"] == json!("queued") && task["routineId"] == json!(routine_id))
        .expect("the resumed firing keeps a durable row while it waits");
    assert_eq!(waiting["prompt"], json!("sweep the notes"));
    assert_eq!(state["presence"]["waiting"]["routine"], json!(1));
    let task_id = waiting["id"].as_str().unwrap();
    assert_eq!(
        server
            .handle("POST", &format!("/api/tasks/{task_id}/cancel"), "")
            .0,
        200
    );
    let (_, body) = server.handle("GET", "/api/state?lean=1", "");
    let state: Value = serde_json::from_str(&body).unwrap();
    let routine = &state["routines"]["routines"][0];
    assert_eq!(routine["lastTaskId"], task_id);
    assert_eq!(routine["lastStatus"], "cancelled");
    assert_eq!(routine["runs"], 1);
}

#[test]
fn a_hand_run_of_a_disabled_routine_overrides_the_pause() {
    let (turn_tx, inbox) = TurnInbox::channel();
    let server = server_with_turn_channel(ProductState::default(), turn_tx);
    let (_, body) = server.handle(
        "POST",
        "/api/routines",
        r#"{"title":"Sweep","prompt":"sweep the notes","schedule":"every:1h"}"#,
    );
    let routine_id = serde_json::from_str::<Value>(&body).unwrap()["routine"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    assert_eq!(
        server
            .handle("POST", &format!("/api/routines/{routine_id}/disable"), "")
            .0,
        200
    );
    let (status, body) = server.handle("POST", &format!("/api/routines/{routine_id}/run"), "");
    assert_eq!(status, 202, "{body}");
    let value: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(value["override"], json!(true));
    assert!(
        value["message"]
            .as_str()
            .unwrap_or_default()
            .contains("overrides the pause"),
        "{body}"
    );
    let request = inbox.try_recv().expect("the hand run is admitted").request;
    assert_eq!(
        request.routine_id.as_deref(),
        Some(routine_id.as_str()),
        "it is still the routine's firing"
    );
    assert!(request.routine_override);

    // A paused queue keeps the override, so resuming still runs what was asked
    // for even though the routine stays disabled.
    assert_eq!(server.handle("POST", "/api/control/pause", "").0, 200);
    assert_eq!(
        server
            .handle("POST", &format!("/api/routines/{routine_id}/run"), "")
            .0,
        202
    );
    assert_eq!(
        control_of(&server)["queue"][0]["routineOverride"],
        json!(true)
    );
    assert_eq!(server.handle("POST", "/api/control/resume", "").0, 200);
    let resumed = inbox
        .try_recv()
        .expect("resuming dispatches the held hand run")
        .request;
    assert!(resumed.routine_override);
    assert!(resumed.routine_counted);
    assert_eq!(resumed.lane(), TurnLane::Routine);
}

#[test]
fn notification_and_voice_settings_round_trip() {
    let server = free_server(ProductState::default());
    let (status, body) = server.handle(
        "POST",
        "/api/notifications",
        r#"{"webhookUrl":"http://127.0.0.1:9/hook","events":["approval","routine"],"apns":{"teamId":"TEAM","keyId":"KEY","topic":"dev.glass.marble","keyFile":"/keys/AuthKey.p8"}}"#,
    );
    assert_eq!(status, 200, "{body}");
    let settings: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(settings["webhookConfigured"], json!(true));
    assert_eq!(settings["apnsConfigured"], json!(true));
    assert_eq!(settings["events"], json!(["approval", "routine"]));
    assert_eq!(settings["webhookTokenSet"], json!(false));

    assert_eq!(
        server
            .handle(
                "POST",
                "/api/notifications",
                r#"{"webhookUrl":"ftp://nope"}"#
            )
            .0,
        400
    );

    let token = "ab".repeat(32);
    let (status, body) = server.handle(
        "POST",
        "/api/devices",
        &format!(r#"{{"token":"{token}","platform":"ios","topic":"dev.glass.marble"}}"#),
    );
    assert_eq!(status, 200, "{body}");
    let (_, devices) = server.handle("GET", "/api/devices", "");
    let devices: Value = serde_json::from_str(&devices).unwrap();
    assert_eq!(devices["devices"].as_array().map(Vec::len), Some(1));
    assert_eq!(
        server.handle("POST", "/api/devices/nothex/remove", "").0,
        404
    );

    let (status, body) = server.handle("POST", "/api/voice", r#"{"enabled":true,"rate":0.45}"#);
    assert_eq!(status, 200, "{body}");
    let voice: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(voice["enabled"], json!(true));
    assert_eq!(voice["provider"], json!("on-device"));

    let (status, body) = server.handle(
        "POST",
        "/api/voice/speak",
        r#"{"text":"**Done** with `code`"}"#,
    );
    assert_eq!(status, 200, "{body}");
    let spoken: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(spoken["spoken"], json!("Done with code"));
}

#[test]
fn a_correction_reaches_the_running_turn_and_idle_hosts_refuse_it() {
    let (steer_tx, steer_rx) = mpsc::channel();
    let mut state = ProductState::default();
    state.tasks.push(TaskRecord {
        marble_id: String::new(),
        id: "t1".into(),
        conversation_id: "c1".into(),
        status: "running".into(),
        owner: "agent".into(),
        prompt: "tidy".into(),
        at: 1,
        routine_id: String::new(),
    });
    let server = server_with_steering(state, steer_tx);
    let (status, body) = server.handle(
        "POST",
        "/api/turns/steer",
        r#"{"conversationId":"c1","text":"use the other file"}"#,
    );
    assert_eq!(status, 202, "{body}");
    let request = steer_rx
        .try_recv()
        .expect("the worker receives the correction");
    assert_eq!(request.conversation_id, "c1");
    assert_eq!(request.task_id, "t1");
    assert_eq!(request.text, "use the other file");
    assert_eq!(
        server
            .handle(
                "POST",
                "/api/turns/steer",
                r#"{"conversationId":"gone","text":"x"}"#
            )
            .0,
        409
    );
    assert_eq!(
        server
            .handle("POST", "/api/turns/steer", r#"{"text":"  "}"#)
            .0,
        400
    );
}

#[test]
fn memory_can_be_edited_in_place() {
    let server = server_with(
        ProductState::default(),
        ApprovalInbox::default(),
        EventBus::default(),
        AccessPolicy::new(None),
    );
    let (status, body) = server.handle("POST", "/api/memory", r#"{"text":"my timezone is UTC"}"#);
    assert_eq!(status, 200, "{body}");
    let id = serde_json::from_str::<Value>(&body).unwrap()["remembered"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    let (status, body) = server.handle(
        "POST",
        &format!("/api/memory/{id}/update"),
        r#"{"text":"my timezone is Asia/Kolkata","kind":"preference"}"#,
    );
    assert_eq!(status, 200, "{body}");
    assert!(body.contains("Asia/Kolkata"));
    assert_eq!(
        server
            .handle("POST", "/api/memory/missing/update", r#"{"text":"x"}"#)
            .0,
        404
    );
    assert_eq!(
        server
            .handle(
                "POST",
                &format!("/api/memory/{id}/update"),
                r#"{"text":""}"#
            )
            .0,
        400
    );
    let (_, memory) = server.handle("GET", "/api/memory", "");
    let memory: Value = serde_json::from_str(&memory).unwrap();
    assert_eq!(memory["facts"].as_array().map(Vec::len), Some(1));
    assert_eq!(memory["facts"][0]["kind"], json!("preference"));
}

#[test]
fn state_reports_retention_and_routines_can_skip_catch_up() {
    let server = server_with(
        ProductState::default(),
        ApprovalInbox::default(),
        EventBus::default(),
        AccessPolicy::new(None),
    );
    let (_, body) = server.handle("GET", "/api/state", "");
    let state: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(state["retention"]["trimmedTasks"], json!(0));
    assert_eq!(state["retention"]["limits"]["tasks"], json!(500));

    let (status, body) = server.handle(
        "POST",
        "/api/routines",
        r#"{"title":"Digest","prompt":"summarize","schedule":"every:1h"}"#,
    );
    assert_eq!(status, 200, "{body}");
    let id = serde_json::from_str::<Value>(&body).unwrap()["routine"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    let (status, body) = server.handle(
        "POST",
        &format!("/api/routines/{id}/catch-up"),
        r#"{"enabled":false}"#,
    );
    assert_eq!(status, 200, "{body}");
    let (_, body) = server.handle("GET", "/api/routines", "");
    let routines: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(routines["routines"][0]["catchUp"], json!(false));

    let (_, body) = server.handle("GET", "/api/notifications", "");
    let notifications: Value = serde_json::from_str(&body).unwrap();
    assert!(notifications["recent"].is_array());
    assert_eq!(notifications["quietHours"]["enabled"], json!(false));
    assert_eq!(notifications["retries"], json!(2));
}

#[test]
fn transcript_pages_search_and_conversation_lifecycle() {
    let mut state = ProductState::default();
    state.conversation_mut("c1").title = "Notes".into();
    for (index, text) in ["first note", "second note", "third note"]
        .iter()
        .enumerate()
    {
        state.push_transcript(crate::TranscriptEntry {
            conversation_id: "c1".into(),
            kind: "assistant".into(),
            text: (*text).into(),
            task_id: format!("t{index}"),
            at: 10 * (index as u64 + 1),
            ..Default::default()
        });
    }
    // A settled task from this conversation, and an orphan left behind by a
    // chat that is already gone — the delete must take both kinds of history
    // it owns, and the load path prunes what it does not.
    state.tasks.push(TaskRecord {
        marble_id: String::new(),
        id: "t1".into(),
        conversation_id: "c1".into(),
        status: "completed".into(),
        owner: "agent".into(),
        prompt: "done".into(),
        at: 5,
        routine_id: String::new(),
    });
    let server = server_with(
        state,
        ApprovalInbox::default(),
        EventBus::default(),
        AccessPolicy::new(None),
    );

    let (_, body) = server.handle("GET", "/api/state?lean=1", "");
    let lean: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(lean["transcript"].as_array().map(Vec::len), Some(0));
    assert_eq!(lean["transcriptCount"], json!(3));
    assert_eq!(lean["transcriptOmitted"], json!(true));

    let (status, body) = server.handle("GET", "/api/transcript?conversationId=c1&after=15", "");
    assert_eq!(status, 200, "{body}");
    let page: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(page["entries"].as_array().map(Vec::len), Some(2));
    assert_eq!(page["cursor"], json!(30));

    let (_, body) = server.handle("GET", "/api/transcript/search?q=SECOND", "");
    let hits: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(hits["hits"].as_array().map(Vec::len), Some(1));
    assert_eq!(hits["hits"][0]["conversationId"], json!("c1"));
    assert_eq!(hits["hits"][0]["title"], json!("Notes"));

    let (status, _) = server.handle(
        "POST",
        "/api/conversations/%631/rename",
        r#"{"title":"Deploys"}"#,
    );
    assert_eq!(status, 200);
    let (_, body) = server.handle("GET", "/api/state?lean=1", "");
    let lean: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(lean["conversations"][0]["title"], json!("Deploys"));

    let (status, _) = server.handle("POST", "/api/conversations/%631/delete", "");
    assert_eq!(status, 200);
    let (status, body) = server.handle("GET", "/api/transcript?conversationId=c1", "");
    let page: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(status, 404);
    assert_eq!(page["error"], "not_found");
    assert_eq!(page["message"], "This chat no longer exists");
    // Its tasks go with it: nothing is left to render as orphaned "recent
    // turns" on the chat list.
    let (_, body) = server.handle("GET", "/api/state?lean=1", "");
    let lean: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(lean["tasks"].as_array().map(Vec::len), Some(0));
    assert_eq!(lean["conversations"].as_array().map(Vec::len), Some(0));
    assert_eq!(
        server
            .handle("POST", "/api/conversations/c1/rename", r#"{"title":"x"}"#)
            .0,
        404
    );
    assert_eq!(
        server
            .handle("POST", "/api/conversations/c1/rename", r#"{"title":" "}"#)
            .0,
        400
    );
    assert_eq!(
        server.handle("POST", "/api/conversations/c1/delete", "").0,
        404
    );
}

#[test]
fn a_daily_routine_takes_its_timezone_from_the_surface_that_saved_it() {
    let server = server_with(
        ProductState::default(),
        ApprovalInbox::default(),
        EventBus::default(),
        AccessPolicy::new(None),
    );
    let (status, body) = server.handle(
        "POST",
        "/api/routines",
        r#"{"title":"Digest","prompt":"summarize","schedule":"daily:09:00","offsetMinutes":330}"#,
    );
    assert_eq!(status, 200, "{body}");
    let routines: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(routines["routines"]["offsetMinutes"], json!(330));
    let (status, body) = server.handle("POST", "/api/routines/offset", r#"{"offsetMinutes":-480}"#);
    assert_eq!(status, 200, "{body}");
    let routines: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(routines["routines"]["offsetMinutes"], json!(-480));
    assert_eq!(
        server.handle("POST", "/api/routines/offset", r#"{}"#).0,
        400
    );
}

#[test]
fn export_covers_the_whole_document_or_one_conversation() {
    let mut state = ProductState::default();
    state.conversation_mut("c1").title = "Notes".into();
    state.tasks.push(TaskRecord {
        marble_id: String::new(),
        id: "t1".into(),
        conversation_id: "c1".into(),
        status: "completed".into(),
        owner: "agent".into(),
        prompt: "tidy".into(),
        at: 4,
        routine_id: String::new(),
    });
    state.push_transcript(crate::TranscriptEntry {
        conversation_id: "c1".into(),
        kind: "assistant".into(),
        text: "tidied the notes".into(),
        task_id: "t1".into(),
        at: 5,
        ..Default::default()
    });
    let server = server_with(
        state,
        ApprovalInbox::default(),
        EventBus::default(),
        AccessPolicy::new(None),
    );

    let (status, body) = server.handle("GET", "/api/export", "");
    assert_eq!(status, 200, "{body}");
    let export: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(export["format"], json!("glass-marble/v1"));
    assert_eq!(
        export["state"]["transcript"].as_array().map(Vec::len),
        Some(1)
    );

    let (status, body) = server.handle("GET", "/api/export?conversationId=c1", "");
    assert_eq!(status, 200, "{body}");
    let export: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(export["format"], json!("glass-marble-conversation/v1"));
    assert_eq!(export["conversation"]["title"], json!("Notes"));
    assert_eq!(export["transcript"].as_array().map(Vec::len), Some(1));
    assert_eq!(
        server
            .handle("GET", "/api/export?conversationId=missing", "")
            .0,
        404
    );
}

#[test]
fn the_model_can_be_changed_and_is_validated() {
    let server = server_with(
        ProductState::default(),
        ApprovalInbox::default(),
        EventBus::default(),
        AccessPolicy::new(None),
    );
    let (status, body) = server.handle("POST", "/api/model", r#"{"model":"claude-sonnet-4"}"#);
    assert_eq!(status, 200, "{body}");
    let changed: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(changed["appliesTo"], json!("new conversations"));
    let (_, body) = server.handle("GET", "/api/state?lean=1", "");
    let state: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(state["model"], json!("claude-sonnet-4"));

    let (status, body) = server.handle("POST", "/api/model", r#"{"model":""}"#);
    assert_eq!(status, 200, "{body}");
    assert_eq!(
        serde_json::from_str::<Value>(&body).unwrap()["engineDefault"],
        json!(true)
    );
    assert_eq!(
        server
            .handle("POST", "/api/model", r#"{"model":"bad\u0000name"}"#)
            .0,
        400
    );
}

#[test]
fn cancelling_a_queued_turn_removes_it_before_execution() {
    let (turn_tx, inbox) = TurnInbox::channel();
    let mut state = ProductState::default();
    for index in 0..glass_marble::state::CONVERSATION_LIMIT {
        state.conversation_mut(&format!("old-{index}")).updated_at = index as u64 + 1;
    }
    let server = server_with_turn_channel(state, turn_tx);
    let (status, body) = server.handle("POST", "/api/turns", r#"{"text":"never run"}"#);
    assert_eq!(status, 202);
    let queued: Value = serde_json::from_str(&body).unwrap();
    let id = queued["taskId"].as_str().unwrap();
    let route = format!("/api/tasks/{id}/cancel");
    assert_eq!(server.handle("POST", &route, "").0, 200);
    assert!(inbox.try_recv().is_none());
    let (_, body) = server.handle("GET", "/api/state", "");
    let doc: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(doc["tasks"][0]["status"], "cancelled");
    assert_eq!(doc["transcript"][0]["text"], "Cancelled before it started");
    let mut state: ProductState = serde_json::from_str(&body).unwrap();
    state.prune(glass_marble::worker::now_millis());
    let recovered = format!("recovered-{id}");
    assert!(
        state
            .conversations
            .iter()
            .any(|chat| chat.id == recovered && chat.updated_at > 100)
    );
    assert!(
        state
            .transcript
            .iter()
            .any(|entry| entry.conversation_id == recovered)
    );
    assert_eq!(server.handle("POST", &route, "").0, 409);
    assert_eq!(
        server.handle("POST", "/api/tasks/missing/cancel", "").0,
        404
    );
}

#[test]
fn a_busy_worker_answers_queued_with_its_place_in_line() {
    let (turn_tx, inbox) = TurnInbox::channel();
    let server = server_with_turn_channel(ProductState::default(), turn_tx);

    let started = Instant::now();
    let (status, body) = server.handle("POST", "/api/turns", r#"{"text":"first"}"#);
    assert_eq!(status, 202, "{body}");
    let queued: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(queued["queued"], json!(true));
    assert_eq!(queued["accepted"], json!(false));
    assert_eq!(queued["position"], json!(1));
    assert!(
        started.elapsed() >= Duration::from_secs(1),
        "the surface waits briefly for a free worker before answering"
    );

    // The message is in the worker's channel, not answered and forgotten.
    let command = inbox
        .recv_timeout(Duration::from_secs(1))
        .expect("the queued message is waiting for the worker");
    assert_eq!(command.request.text, "first");

    let (status, body) = server.handle("POST", "/api/turns", r#"{"text":"second"}"#);
    assert_eq!(status, 202, "{body}");
    let queued: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(
        queued["position"],
        json!(1),
        "the first message is already in the worker's hands, so this one is next in its lane"
    );

    // The queued message is durable while it waits, not only a channel slot.
    let (_, body) = server.handle("GET", "/api/state?lean=1", "");
    let state: Value = serde_json::from_str(&body).unwrap();
    let statuses: Vec<&str> = state["tasks"]
        .as_array()
        .unwrap()
        .iter()
        .map(|task| task["status"].as_str().unwrap_or_default())
        .collect();
    assert_eq!(statuses, vec!["queued", "queued"], "{body}");
    assert_eq!(state["presence"]["waitingTurns"], json!(2));
    assert_eq!(
        state["tasks"][0]["prompt"],
        json!("first"),
        "the waiting row keeps the message"
    );
}

#[test]
fn a_typed_message_runs_before_a_waiting_routine_firing() {
    let (turn_tx, inbox) = TurnInbox::channel();
    let server = server_with_turn_channel(ProductState::default(), turn_tx);
    let (status, body) = server.handle(
        "POST",
        "/api/routines",
        r#"{"title":"Tidy","prompt":"tidy the notes","schedule":"every:30m","conversationId":"durable-fixture-1"}"#,
    );
    assert_eq!(status, 200, "{body}");
    let created: Value = serde_json::from_str(&body).unwrap();
    let routine_id = created["routine"]["id"].as_str().unwrap().to_string();
    let (status, body) = server.handle("POST", &format!("/api/routines/{routine_id}/run"), "");
    assert_eq!(status, 202, "{body}");

    // The firing is already waiting in the routine lane when a person types.
    let (status, body) = server.handle(
        "POST",
        "/api/turns",
        r#"{"text":"tell me where I left off"}"#,
    );
    assert_eq!(status, 202, "{body}");
    assert_eq!(
        serde_json::from_str::<Value>(&body).unwrap()["position"],
        json!(1),
        "a typed message counts only typed messages in front of it"
    );

    let typed = inbox.try_recv().expect("the typed message runs next");
    assert_eq!(typed.request.text, "tell me where I left off");
    assert!(typed.request.routine_id.is_none());
    let firing = inbox.try_recv().expect("the routine follows");
    assert_eq!(
        firing.request.routine_id.as_deref(),
        Some(routine_id.as_str())
    );
    assert_eq!(firing.request.text, "tidy the notes");

    // Presence explains the wait instead of leaving it a mystery.
    let (_, body) = server.handle("GET", "/api/state?lean=1", "");
    let state: Value = serde_json::from_str(&body).unwrap();
    let presence = &state["presence"];
    assert_eq!(presence["waitingTurns"], json!(2));
    assert_eq!(presence["waiting"]["human"], json!(1));
    assert_eq!(presence["waiting"]["routine"], json!(1));
    assert_eq!(presence["waiting"]["nextLane"], json!("human"));
    assert_eq!(
        presence["waiting"]["note"],
        json!("typed messages run before routine firings")
    );
}

#[test]
fn health_reports_storage_write_failure_and_recovery() {
    const ISOLATED: &str = "MARBLE_STORAGE_HEALTH_TEST";
    if std::env::var_os(ISOLATED).is_none() {
        let output = Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "health_reports_storage_write_failure_and_recovery",
                "--nocapture",
            ])
            .env(ISOLATED, "1")
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        return;
    }
    let root = std::env::temp_dir().join(format!("marble-storage-health-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let store = Arc::new(StateStore::in_data_dir(&root));
    let server = MarbleServer::new(MarbleServerParts {
        listener: TcpListener::bind("127.0.0.1:0").unwrap(),
        state: Arc::new(Mutex::new(ProductState::default())),
        store: Some(store.clone()),
        approvals: ApprovalInbox::default(),
        events: EventBus::default(),
        turn_tx: TurnSender::detached(),
        steer_tx: mpsc::channel().0,
        policy: AccessPolicy::new(None),
        cancels: CancelRegistry::default(),
    });
    let (status, body) = server.handle("GET", "/healthz", "");
    assert_eq!(status, 200, "{body}");
    assert_eq!(
        serde_json::from_str::<Value>(&body).unwrap()["storage"]["document"],
        false
    );
    std::fs::create_dir(store.path()).unwrap();
    glass_marble::store::persist(&store, &ProductState::default());
    let (status, body) = server.handle("GET", "/healthz", "");
    assert_eq!(status, 503, "{body}");
    assert!(
        !serde_json::from_str::<Value>(&body).unwrap()["lastPersistError"]
            .as_str()
            .unwrap()
            .is_empty()
    );
    std::fs::remove_dir(store.path()).unwrap();
    glass_marble::store::persist(&store, &ProductState::default());
    assert_eq!(server.handle("GET", "/healthz", "").0, 200);
    std::fs::remove_dir_all(&root).unwrap();
}

#[test]
fn idle_engine_exit_makes_health_unhealthy() {
    if !python3_available() {
        return;
    }
    let root = std::env::temp_dir().join(format!("marble-idle-health-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let state = Arc::new(Mutex::new(ProductState::default()));
    let events = EventBus::default();
    let mut python = Command::new("python3");
    python
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/fake_glass_core_runtime.py"
        ))
        .arg("runtime")
        .arg("--exit-after-initialize");
    let conversation = Conversation::spawn_with_router(
        python,
        &root,
        &root.join("sessions"),
        HostPolicy::approving(),
        ApprovalInbox::default(),
        ToolRouter::new(&root).unwrap(),
        events.clone(),
    )
    .unwrap()
    .with_respawn(|| {
        let mut command = Command::new("python3");
        command
            .arg(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/tests/fixtures/fake_glass_core_runtime.py"
            ))
            .arg("runtime");
        command
    });
    let turn_tx = spawn_single_lane(
        conversation,
        state.clone(),
        None,
        LiveStore::in_data_dir(&root),
        events.clone(),
        Duration::from_secs(10),
        CancelRegistry::default(),
    );
    let server = MarbleServer::new(MarbleServerParts {
        listener: TcpListener::bind("127.0.0.1:0").unwrap(),
        state: state.clone(),
        store: None,
        approvals: ApprovalInbox::default(),
        events: events.clone(),
        turn_tx,
        steer_tx: mpsc::channel().0,
        policy: AccessPolicy::new(None),
        cancels: CancelRegistry::default(),
    });
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let (status, body) = server.handle("GET", "/healthz", "");
        if status == 503 {
            let health: Value = serde_json::from_str(&body).unwrap();
            assert_eq!(health["runtime"]["health"], "failed");
            assert_eq!(health["ok"], false);
            break;
        }
        assert!(Instant::now() < deadline, "{body}");
        std::thread::sleep(Duration::from_millis(20));
    }
    let mut python = Command::new("python3");
    python
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/fake_glass_core_runtime.py"
        ))
        .arg("runtime");
    let healthy = Conversation::spawn_with_router(
        python,
        &root,
        &root.join("healthy-sessions"),
        HostPolicy::approving(),
        ApprovalInbox::default(),
        ToolRouter::new(&root).unwrap(),
        events.clone(),
    )
    .unwrap();
    let healthy_tx = spawn_single_lane(
        healthy,
        state.clone(),
        None,
        LiveStore::in_data_dir(&root),
        events,
        Duration::from_secs(10),
        CancelRegistry::default(),
    );
    let healthy_server = MarbleServer::new(MarbleServerParts {
        listener: TcpListener::bind("127.0.0.1:0").unwrap(),
        state: state.clone(),
        store: None,
        approvals: ApprovalInbox::default(),
        events: EventBus::default(),
        turn_tx: healthy_tx,
        steer_tx: mpsc::channel().0,
        policy: AccessPolicy::new(None),
        cancels: CancelRegistry::default(),
    });
    for (surface, prompt) in [
        (&healthy_server, "healthy lane"),
        (&server, "restarted lane"),
    ] {
        let (status, body) =
            surface.handle("POST", "/api/turns", &json!({"text": prompt}).to_string());
        assert_eq!(status, 202, "{body}");
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            if state
                .lock()
                .unwrap()
                .tasks
                .iter()
                .any(|task| task.prompt == prompt && task.status == "completed")
            {
                break;
            }
            assert!(Instant::now() < deadline, "{prompt} did not complete");
            std::thread::sleep(Duration::from_millis(20));
        }
        assert_eq!(
            server.handle("GET", "/healthz", "").0,
            if prompt == "healthy lane" { 503 } else { 200 }
        );
    }
}

#[test]
fn health_reports_the_runtime_the_document_and_the_backup() {
    let dir = std::env::temp_dir().join(format!("glass-marble-health-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let store = glass_marble::StateStore::in_data_dir(&dir);
    store.save(&test_state()).unwrap();
    let state = Arc::new(Mutex::new(test_state()));
    let server = MarbleServer::new(MarbleServerParts {
        listener: TcpListener::bind("127.0.0.1:0").unwrap(),
        state: Arc::clone(&state),
        store: Some(Arc::new(store)),
        approvals: ApprovalInbox::default(),
        events: EventBus::default(),
        turn_tx: TurnSender::detached(),
        steer_tx: mpsc::channel().0,
        policy: AccessPolicy::new(None),
        cancels: CancelRegistry::default(),
    });

    let (status, body) = server.handle("GET", "/healthz", "");
    assert_eq!(status, 200, "{body}");
    let health: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(health["ok"], json!(true));
    assert!(
        health["version"]
            .as_str()
            .is_some_and(|value| !value.is_empty())
    );
    assert_eq!(health["runtime"]["health"], json!("ok"));
    assert_eq!(health["storage"]["document"], json!(true));
    assert_eq!(health["storage"]["quarantined"], json!(0));
    assert_eq!(health["storage"]["backup"]["count"], json!(0));
    assert!(health["lastPersistError"].is_null());
    assert_eq!(health["recoveredTasks"], json!(0));

    std::fs::remove_file(dir.join("product-state.json")).unwrap();
    let (status, body) = server.handle("GET", "/healthz", "");
    assert_eq!(status, 200, "{body}");
    assert_eq!(
        serde_json::from_str::<Value>(&body).unwrap()["storage"]["document"],
        false
    );

    // An engine that stopped answering is not a healthy host, and the answer
    // says so instead of reporting "ok" and leaving it to a log.
    state
        .lock()
        .unwrap()
        .presence
        .runtime
        .note_failure("the pipe closed");
    let (status, body) = server.handle("GET", "/healthz", "");
    assert_eq!(status, 503, "{body}");
    let health: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(health["ok"], json!(false));
    assert_eq!(health["runtime"]["lastError"], json!("the pipe closed"));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn retry_repeats_the_last_turn_that_did_not_finish() {
    let mut state = ProductState::default();
    state.tasks.push(TaskRecord {
        marble_id: String::new(),
        id: "t-old".into(),
        conversation_id: "c1".into(),
        status: "completed".into(),
        owner: "agent".into(),
        prompt: "already done".into(),
        at: 1,
        routine_id: String::new(),
    });
    state.tasks.push(TaskRecord {
        marble_id: String::new(),
        id: "t-failed".into(),
        conversation_id: "c1".into(),
        status: "failed".into(),
        owner: "agent".into(),
        prompt: "tidy the notes".into(),
        at: 2,
        routine_id: String::new(),
    });
    let (turn_tx, inbox) = TurnInbox::channel();
    let server = server_with_turn_channel(state, turn_tx);
    let accept = std::thread::spawn(move || {
        let command = inbox
            .recv_timeout(Duration::from_secs(5))
            .expect("the retried turn reaches the worker");
        assert_eq!(command.request.text, "tidy the notes");
        assert_eq!(command.request.conversation_id.as_deref(), Some("c1"));
        command
            .accepted
            .send(Ok(glass_marble::AcceptedTurn {
                conversation_id: "c1".into(),
                task_id: "t-retry".into(),
            }))
            .unwrap();
    });
    let (status, body) = server.handle("POST", "/api/turns/retry", "{}");
    assert_eq!(status, 202, "{body}");
    let retried: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(retried["accepted"], json!(true));
    assert_eq!(retried["taskId"], json!("t-retry"));
    assert_eq!(retried["retrying"], json!("t-failed"));
    accept.join().unwrap();

    // Nothing failed in a fresh document, so there is nothing to repeat.
    let server = free_server(ProductState::default());
    assert_eq!(server.handle("POST", "/api/turns/retry", "{}").0, 409);
}

#[test]
fn retry_rejects_archived_everyone_thread_before_mutating_state() {
    let mut state = ProductState::default();
    state.conversation_mut("chat");
    state.upsert_task(TaskRecord {
        id: "failed".into(),
        conversation_id: "chat".into(),
        status: "failed".into(),
        prompt: "Retry this task".into(),
        ..Default::default()
    });
    let server = free_server(state);
    let (_, body) = server.handle(
        "POST",
        "/api/threads",
        r#"{"conversation_id":"everyone","title":"Archived"}"#,
    );
    let document: Value = serde_json::from_str(&body).unwrap();
    let id = document["thread"]["id"].as_str().unwrap();
    assert_eq!(
        server
            .handle("POST", &format!("/api/threads/{id}/archive"), "{}")
            .0,
        200
    );
    let mut state = server.state();
    state.task_threads.insert("failed".into(), id.into());
    let server = free_server(state);
    let (status, body) = server.handle("POST", "/api/turns/retry", r#"{"taskId":"failed"}"#);
    assert_eq!(status, 409, "{body}");
    assert!(body.contains("thread_archived"), "{body}");
    assert!(server.state().conversations[0].retry_task_id.is_empty());
}

#[test]
fn routines_can_hold_a_quiet_window() {
    let server = free_server(ProductState::default());
    let (status, body) = server.handle(
        "POST",
        "/api/routines/quiet",
        r#"{"enabled":true,"startMinute":1320,"endMinute":420,"offsetMinutes":330}"#,
    );
    assert_eq!(status, 200, "{body}");
    let routines: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(routines["changed"], json!(true));
    assert_eq!(routines["routines"]["quietHours"]["enabled"], json!(true));
    assert_eq!(
        routines["routines"]["quietHours"]["startMinute"],
        json!(1320)
    );
    assert_eq!(
        routines["routines"]["quietHours"]["offsetMinutes"],
        json!(330)
    );

    // Saving the same window again changes nothing.
    let (status, body) = server.handle(
        "POST",
        "/api/routines/quiet",
        r#"{"startMinute":1320,"endMinute":420,"offsetMinutes":330}"#,
    );
    assert_eq!(status, 200, "{body}");
    assert_eq!(
        serde_json::from_str::<Value>(&body).unwrap()["changed"],
        json!(false)
    );
    assert_eq!(
        server.handle("POST", "/api/routines/quiet", "{ not json").0,
        400
    );
}

#[test]
fn state_revalidates_with_an_etag_instead_of_resending_the_document() {
    let server = free_server(test_state());
    let first = server.handle_request(&RequestHead::new("GET", "/api/state?lean=1"), "");
    assert_eq!(first.status, 200);
    let etag = first
        .headers
        .iter()
        .find(|(name, _)| name == "ETag")
        .map(|(_, value)| value.clone())
        .expect("state responses carry an ETag");

    let second = server.handle_request(
        &RequestHead::new("GET", "/api/state?lean=1").with_header("if-none-match", &etag),
        "",
    );
    assert_eq!(second.status, 304);
    assert!(second.body.is_empty());
    assert_eq!(
        second
            .headers
            .iter()
            .find(|(name, _)| name == "ETag")
            .map(|(_, value)| value.as_str()),
        Some(etag.as_str())
    );

    // The full document is a different representation of the same revision.
    let full = server.handle_request(
        &RequestHead::new("GET", "/api/state").with_header("if-none-match", &etag),
        "",
    );
    assert_eq!(full.status, 200);
    assert!(!full.body.is_empty());
}

#[test]
fn repeated_login_failures_are_throttled_per_caller() {
    let server = server_with(
        test_state(),
        ApprovalInbox::default(),
        EventBus::default(),
        AccessPolicy::new(Some("s3cret".into())),
    );
    for _ in 0..5 {
        let (status, _) = server.handle("POST", "/api/login", r#"{"token":"nope"}"#);
        assert_eq!(status, 401);
    }
    let (status, body) = server.handle("POST", "/api/login", r#"{"token":"s3cret"}"#);
    assert_eq!(status, 429, "{body}");
    assert_eq!(
        serde_json::from_str::<Value>(&body).unwrap()["error"],
        json!("too_many_attempts")
    );

    // Another address is unaffected, so one attacker cannot lock the owner out.
    let response = server.handle_request_from(
        &RequestHead::new("POST", "/api/login"),
        r#"{"token":"s3cret"}"#,
        "100.64.0.9",
    );
    assert_eq!(response.status, 200);
}

#[test]
fn group_rounds_handoff_pass_interrupt_and_close_without_leaking_sessions() {
    if !python3_available() {
        return;
    }
    let root = std::env::temp_dir().join(format!("glass-marble-group-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let script = root.join("fake_glass_core_runtime.py");
    std::fs::write(&script, include_str!("fixtures/fake_glass_core_runtime.py")).unwrap();
    let mut product = ProductState::default();
    product.prism.install_template();
    product.control.profile = glass_marble::ControlProfile::Unattended;
    product
        .prism
        .find_mut("reviewer")
        .unwrap()
        .card
        .as_mut()
        .unwrap()
        .when_to_speak = glass_marble::prism::WhenToSpeak::Mentioned;
    product
        .prism
        .find_mut("designer")
        .unwrap()
        .card
        .as_mut()
        .unwrap()
        .when_to_speak = glass_marble::prism::WhenToSpeak::Always;
    let state = Arc::new(Mutex::new(product));
    let events = EventBus::default();
    let pool = fixture_prism_pool(
        &script,
        &root,
        &state,
        &events,
        vec!["--group-rounds".into()],
    );
    let server = MarbleServer::new(MarbleServerParts {
        listener: TcpListener::bind("127.0.0.1:0").unwrap(),
        state: Arc::clone(&state),
        store: None,
        approvals: ApprovalInbox::default(),
        events,
        turn_tx: TurnSender::detached(),
        steer_tx: mpsc::channel().0,
        policy: AccessPolicy::new(None),
        cancels: CancelRegistry::default(),
    })
    .with_prism(pool);
    let (status, body) = server.handle("POST", "/api/threads",
        r#"{"conversation_id":"group-chats","title":"Launch","group":{"members":["designer","engineer","reviewer","finance"],"goal":"Plan launch","doneWhen":"Checklist ready"}}"#);
    assert_eq!(status, 201, "{body}");
    let created: Value = serde_json::from_str(&body).unwrap();
    let id = created["thread"]["id"].as_str().unwrap();
    let post = |text: &str| {
        server.handle(
            "POST",
            "/api/turns",
            &json!({"threadId": id, "text":text, "clientMessageId":text}).to_string(),
        )
    };
    let wait = |check: &dyn Fn(&ProductState) -> bool| {
        let deadline = Instant::now() + Duration::from_secs(10);
        while !check(&state.lock().unwrap()) {
            assert!(
                Instant::now() < deadline,
                "group round did not reach the expected state"
            );
            std::thread::sleep(Duration::from_millis(20));
        }
    };
    let (status, body) = post("Plan the launch");
    assert_eq!(status, 202, "{body}");
    assert_eq!(post("Plan the launch"), (status, body));
    wait(&|state| {
        state.tasks.len() == 4 && state.tasks.iter().all(|task| task.status == "completed")
    });
    {
        let state = state.lock().unwrap();
        assert_eq!(
            state
                .tasks
                .iter()
                .map(|task| task.marble_id.as_str())
                .collect::<Vec<_>>(),
            ["lead", "engineer", "reviewer", "designer"]
        );
        let thread = state.threads.iter().find(|thread| thread.id == id).unwrap();
        let entries = state.thread_entries(thread);
        assert_eq!(
            entries.iter().filter(|entry| entry.kind == "user").count(),
            1
        );
        let replies: Vec<_> = entries
            .iter()
            .filter(|entry| entry.kind == "assistant")
            .collect();
        assert_eq!(replies.len(), 3);
        assert!(replies[1].text.contains("checklist from Lead"));
        assert!(
            entries
                .iter()
                .all(|entry| !entry.text.contains("[[PASS]]") && !entry.text.contains("[[SUGGEST"))
        );
        let group = thread.group.as_ref().unwrap();
        assert!(!group.members.contains(&"researcher".into()));
        assert_eq!(group.suggestions[0].agent_id, "researcher");
    }
    let path = format!("/api/threads/{id}/suggestion");
    assert_eq!(
        server
            .handle("POST", &path, r#"{"agentId":"researcher","add":false}"#)
            .0,
        200
    );
    assert_eq!(post("Pause this round").0, 202);
    wait(&|state| {
        state
            .tasks
            .iter()
            .any(|task| task.prompt == "Pause this round" && task.status == "running")
    });
    assert_eq!(post("Close the task").0, 202);
    wait(&|state| {
        state
            .threads
            .iter()
            .any(|thread| thread.group.as_ref().is_some_and(|group| group.closed))
    });
    let before = server.state();
    let thread = before
        .threads
        .iter()
        .find(|thread| thread.id == id)
        .unwrap();
    let entries = before.thread_entries(thread);
    assert_eq!(
        before
            .tasks
            .iter()
            .filter(|task| task.prompt == "Pause this round")
            .map(|task| task.status.as_str())
            .collect::<Vec<_>>(),
        ["cancelled"]
    );
    assert!(entries.iter().any(|entry| entry.kind == "assistant"
        && entry.speaker_name == "Lead"
        && entry.text == "The launch checklist is ready for review."));
    assert!(
        !thread
            .group
            .as_ref()
            .unwrap()
            .members
            .contains(&"researcher".into())
    );
    assert_eq!(post("After close").0, 409);
    assert_eq!(server.handle("POST", "/api/everyone/clear", "{}").0, 200);
    assert_eq!(server.state().thread_entries(thread), entries);
    assert_eq!(
        server
            .handle("POST", "/api/prism/marbles/engineer/remove", "")
            .0,
        200
    );
    let after = server.state();
    let saved_thread = after.threads.iter().find(|item| item.id == id).unwrap();
    assert!(
        !saved_thread
            .group
            .as_ref()
            .unwrap()
            .members
            .iter()
            .any(|member| member == "engineer")
    );
    assert_eq!(after.thread_entries(saved_thread), entries);
}

#[test]
fn group_threads_keep_lead_and_stay_out_of_everyone() {
    let mut state = ProductState::default();
    state.prism.install_template();
    let server = server_with(
        state,
        ApprovalInbox::default(),
        EventBus::default(),
        AccessPolicy::new(None),
    );
    let (status, body) = server.handle(
        "POST", "/api/threads",
        r#"{"conversation_id":"group-chats","title":"Build it","preset":"launch","group":{"members":[],"goal":"File a release","doneWhen":"The build passes"}}"#,
    );
    assert_eq!(status, 201, "{body}");
    let created: Value = serde_json::from_str(&body).unwrap();
    let thread = &created["thread"];
    let id = thread["id"].as_str().unwrap();
    assert_eq!(
        thread["group"]["members"],
        json!(["lead", "designer", "engineer", "reviewer"])
    );
    assert_eq!(thread["group"]["doneWhen"], "The build passes");
    let (status, body) = server.handle(
        "POST",
        "/api/turns",
        &json!({"text":"Start the work","thread_id":id}).to_string(),
    );
    assert_eq!(status, 503, "{body}");
    assert!(body.contains("prism_unavailable"), "{body}");
    let (_, body) = server.handle("GET", "/api/threads?conversation_id=everyone", "");
    assert_eq!(
        serde_json::from_str::<Value>(&body).unwrap()["threads"],
        json!([])
    );
    assert_eq!(
        server
            .handle(
                "POST",
                &format!("/api/threads/{id}/members"),
                r#"["marble","researcher"]"#
            )
            .0,
        200
    );
    assert_eq!(server.handle("POST", "/api/everyone/clear", "").0, 200);
    let (status, body) = server.handle("GET", &format!("/api/threads/{id}"), "");
    assert_eq!(status, 200, "{body}");
    assert_eq!(
        serde_json::from_str::<Value>(&body).unwrap()["thread"]["group"]["members"],
        json!(["lead", "marble", "researcher"])
    );
}

#[test]
fn template_installation_keeps_existing_agents_and_duplicate_copies_only_the_card() {
    let mut state = ProductState::default();
    for name in ["Ada", "Dietrich"] {
        state
            .prism
            .add(name, "", "", 0, "", "My original prompt")
            .unwrap();
    }
    assert!(state.prism.install_template());
    assert!(!state.prism.install_template());
    assert_eq!(state.prism.marbles.len(), 9);
    assert_eq!(state.prism.marbles[0].persona, "My original prompt");
    assert_eq!(
        state
            .prism
            .get("engineer")
            .unwrap()
            .card
            .as_ref()
            .unwrap()
            .tool_call_limit,
        Some(25)
    );
    let server = server_with(
        state,
        ApprovalInbox::default(),
        EventBus::default(),
        AccessPolicy::new(None),
    );
    let (status, body) = server.handle(
        "POST",
        "/api/prism/marbles",
        r#"{"name":"Second engineer","duplicateId":"engineer"}"#,
    );
    assert_eq!(status, 202, "{body}");
    let added: Value = serde_json::from_str(&body).unwrap();
    let duplicate = &added["marble"];
    assert_eq!(duplicate["card"]["toolCallLimit"], 25);
    assert_eq!(duplicate["card"]["turnTimeoutSecs"], 600);
    assert_eq!(duplicate["card"]["inEveryone"], false);
    assert_eq!(duplicate["conversationId"], "");
    assert_eq!(duplicate["runs"], 0);
    assert_eq!(
        server
            .handle(
                "POST",
                "/api/prism/marbles",
                r#"{"name":"Bad card","card":{"tools":{"allowed":["unknown"]}}}"#,
            )
            .0,
        400
    );
}

#[test]
fn group_lead_is_installed_alongside_an_existing_namesake() {
    for already_installed in [false, true] {
        let mut state = ProductState::default();
        let original = state
            .prism
            .add("Lead", "", "", 0, "", "My own Lead")
            .unwrap()
            .id
            .clone();
        let mut saved = serde_json::to_value(&state).unwrap();
        saved["prism"]["templateInstalled"] = json!(already_installed);
        let mut state: ProductState = serde_json::from_value(saved).unwrap();
        state.prism.install_template();
        let server = server_with(
            state,
            ApprovalInbox::default(),
            EventBus::default(),
            AccessPolicy::new(None),
        );
        let (status, body) = server.handle(
            "POST",
            "/api/threads",
            r#"{"conversation_id":"group-chats","title":"Built-in Lead","group":{"members":[]}}"#,
        );
        assert_eq!(status, 201, "{body}");
        assert_eq!(
            serde_json::from_str::<Value>(&body).unwrap()["thread"]["group"]["members"],
            json!(["lead"])
        );
        let state = server.state();
        assert_eq!(state.prism.get(&original).unwrap().name, "Lead");
        assert_ne!(state.prism.get("lead").unwrap().name, "Lead");
    }
}

#[test]
fn group_prompts_survive_startup_orphan_pruning() {
    let mut state = ProductState::default();
    state.prism.install_template();
    let server = server_with(
        state,
        ApprovalInbox::default(),
        EventBus::default(),
        AccessPolicy::new(None),
    );
    let (status, body) = server.handle(
        "POST",
        "/api/threads",
        r#"{"conversation_id":"group-chats","title":"Keep the prompt","group":{"members":[]}}"#,
    );
    assert_eq!(status, 201, "{body}");
    let id = serde_json::from_str::<Value>(&body).unwrap()["thread"]["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let mut reloaded: ProductState =
        serde_json::from_value(serde_json::to_value(server.state()).unwrap()).unwrap();
    reloaded.push_transcript(glass_marble::TranscriptEntry {
        thread_id: Some(id),
        conversation_id: "group-chats".into(),
        kind: "user".into(),
        text: "Start with this prompt".into(),
        ..Default::default()
    });
    assert_eq!(reloaded.prune_orphaned_tasks(), 0);
    assert_eq!(reloaded.transcript[0].text, "Start with this prompt");
}

#[test]
fn group_workspaces_are_shared_inside_one_thread_only() {
    if !python3_available() {
        return;
    }
    let fixture =
        NamedTurnFixture::with_persistence("group-workspaces", vec!["--group-rounds".into()], true);
    fixture.state.lock().unwrap().prism.install_template();
    let group = |title: &str| {
        let (status, body) = fixture.post(
            "/api/threads",
            json!({
                "conversation_id":"group-chats","title":title,"group":{"members":["engineer"]}
            }),
        );
        assert_eq!(status, 201, "{body}");
        body["thread"]["id"].as_str().unwrap().to_owned()
    };
    let cwd = |speaker: &str, thread: &str| {
        let path = fixture
            .root
            .join("conversation-sessions")
            .join(format!("everyone-session-{thread}__{speaker}"))
            .join("fixture-session-options.json");
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            if let Ok(value) = std::fs::read(&path)
                && let Ok(value) = serde_json::from_slice::<Value>(&value)
            {
                return std::path::PathBuf::from(value["cwd"].as_str().unwrap());
            }
            assert!(Instant::now() < deadline, "missing {}", path.display());
            std::thread::sleep(Duration::from_millis(10));
        }
    };
    let first = group("First");
    let second = group("Second");
    assert_eq!(
        fixture
            .post(
                "/api/turns",
                json!({"threadId":first,"text":"Create handoff files"})
            )
            .0,
        202
    );
    let lead = cwd("lead", &first);
    std::fs::write(lead.join("handoff.txt"), "For the next agent").unwrap();
    let engineer = cwd("engineer", &first);
    assert_eq!(
        std::fs::read_to_string(engineer.join("handoff.txt"))
            .ok()
            .as_deref(),
        Some("For the next agent")
    );
    assert_eq!(
        fixture
            .post(
                "/api/turns",
                json!({"threadId":second,"text":"Separate workspace"})
            )
            .0,
        202
    );
    assert!(!cwd("lead", &second).join("handoff.txt").exists());
}

#[test]
fn prism_roster_is_served_and_editable_without_an_engine() {
    let server = server_with(
        ProductState::default(),
        ApprovalInbox::default(),
        EventBus::default(),
        AccessPolicy::new(None),
    );

    let (status, body) = server.handle("GET", "/api/prism", "");
    assert_eq!(status, 200, "{body}");
    let document: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(document["protocol"], json!("prism"));
    // The roster always carries the default assistant, id `marble`.
    assert_eq!(document["marbles"].as_array().unwrap().len(), 1);
    assert_eq!(document["marbles"][0]["id"], json!("marble"));
    assert_eq!(document["engine"], json!(false));

    let (status, body) = server.handle(
        "POST",
        "/api/prism/marbles",
        r#"{"name":"Scout","role":"reads the repo","glyph":"hex","hue":210}"#,
    );
    assert_eq!(status, 202, "{body}");
    let added: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(added["marble"]["name"], json!("Scout"));
    assert_eq!(added["prism"]["marbles"].as_array().unwrap().len(), 2);

    // A roster is only scannable if two marbles cannot share a name.
    let (status, _) = server.handle("POST", "/api/prism/marbles", r#"{"name":"scout"}"#);
    assert_eq!(status, 400);

    // Without a pool the host says so instead of accepting work nobody runs.
    let (status, body) = server.handle("POST", "/api/prism/dispatch", r#"{"text":"look around"}"#);
    assert_eq!(status, 503, "{body}");

    let id = added["marble"]["id"].as_str().unwrap().to_string();
    let path = format!("/api/prism/marbles/{id}/remove");
    assert_eq!(server.handle("POST", &path, "").0, 200);
    let (_, body) = server.handle("GET", "/api/prism", "");
    let document: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(document["marbles"].as_array().unwrap().len(), 1);
}

#[test]
fn the_tool_inventory_reports_both_owners() {
    let mut state = ProductState::default();
    state.note_engine_tool("read_file");
    state.note_engine_tool("read_file");
    state.note_engine_tool_error("read_file");
    let server = server_with(
        state,
        ApprovalInbox::default(),
        EventBus::default(),
        AccessPolicy::new(None),
    );
    let (status, body) = server.handle("GET", "/api/tools", "");
    assert_eq!(status, 200, "{body}");
    let tools: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(tools["protocol"], json!("mixed"));
    assert_eq!(tools["core"]["observed"][0]["name"], json!("read_file"));
    assert_eq!(tools["core"]["observed"][0]["runs"], json!(2));
    assert_eq!(tools["core"]["observed"][0]["errors"], json!(1));
    let governed = tools["governed"].as_array().unwrap();
    assert_eq!(governed.len(), 6);
    assert!(
        governed
            .iter()
            .any(|tool| { tool["name"] == json!("glass.marble.ask") })
    );
    assert!(governed.iter().any(|tool| {
        tool["name"] == json!("glass.workspace.write") && tool["effect"] == json!("write")
    }));
    assert!(governed.iter().any(|tool| {
        tool["name"] == json!("glass.crates.list") && tool["effect"] == json!("read")
    }));
}

#[test]
fn the_workspace_crate_map_is_served_when_the_host_has_one() {
    let root = std::env::temp_dir().join(format!("glass-marble-crates-ui-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("crates/alpha")).unwrap();
    std::fs::write(
        root.join("Cargo.toml"),
        "[workspace]\nmembers = [\"crates/alpha\"]\n",
    )
    .unwrap();
    std::fs::write(
        root.join("crates/alpha/Cargo.toml"),
        "[package]\nname = \"alpha\"\nversion = \"0.1.0\"\n\n[[bin]]\nname = \"alpha-cli\"\n",
    )
    .unwrap();

    // Without a map the host says so instead of inventing a workspace.
    let server = server_with(
        ProductState::default(),
        ApprovalInbox::default(),
        EventBus::default(),
        AccessPolicy::new(None),
    );
    let (status, body) = server.handle("GET", "/api/crates", "");
    assert_eq!(status, 503, "{body}");
    // Control still answers, and carries no map of its own.
    let control = control_of(&server);
    assert_eq!(control["crates"], Value::Null);

    let server = server.with_crates(glass_marble::CrateMap::load(&root));
    let (status, body) = server.handle("GET", "/api/crates", "");
    assert_eq!(status, 200, "{body}");
    let document: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(document["workspace"]["members"], json!(1));
    assert_eq!(document["crates"][0]["name"], json!("alpha"));
    assert_eq!(
        document["crates"][0]["binaries"][0]["name"],
        json!("alpha-cli")
    );
    assert_eq!(document["crates"][0]["path"], json!("crates/alpha"));

    // The map is a workspace scan, so the state poll does not carry it: a
    // surface fetches it when it opens the sheet that shows crates.
    let control = control_of(&server);
    assert_eq!(control["crates"], Value::Null);
    let (status, body) = server.handle("GET", "/api/state?lean=1", "");
    assert_eq!(status, 200, "{body}");
    let state: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(state["control"]["crates"], Value::Null);

    let _ = std::fs::remove_dir_all(&root);
}

/// `openssl s_client` doubles as the TLS client so the test exercises a real
/// handshake, not just the rustls plumbing.
fn openssl_cli() -> bool {
    Command::new("openssl")
        .arg("version")
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

/// One request/response over TLS via the openssl CLI.
fn tls_get(port: u16, path: &str) -> String {
    let output = Command::new("sh")
        .arg("-c")
        .arg(format!(
            "printf 'GET {path} HTTP/1.1\\r\\nHost: x\\r\\nConnection: close\\r\\n\\r\\n' | \
             openssl s_client -connect 127.0.0.1:{port} -quiet -servername localhost 2>/dev/null"
        ))
        .output()
        .unwrap();
    String::from_utf8_lossy(&output.stdout).to_string()
}

#[test]
fn tls_and_plain_http_share_one_port() {
    if !openssl_cli() {
        eprintln!("openssl CLI unavailable; skipping TLS socket test");
        return;
    }
    let root = std::env::temp_dir().join(format!("marble-tls-test-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    let cert = root.join("cert.pem");
    let key = root.join("key.pem");
    let status = Command::new("openssl")
        .args([
            "req",
            "-x509",
            "-newkey",
            "ec",
            "-pkeyopt",
            "ec_paramgen_curve:prime256v1",
            "-keyout",
        ])
        .arg(&key)
        .arg("-out")
        .arg(&cert)
        .args([
            "-days",
            "1",
            "-nodes",
            "-subj",
            "/CN=localhost",
            "-addext",
            "subjectAltName=DNS:localhost,IP:127.0.0.1",
        ])
        .output()
        .unwrap();
    assert!(status.status.success(), "{:?}", status.stderr);
    let identity = glass_marble::tls::load_identity(&cert, &key).unwrap();
    let leaf = identity.leaf_sha256_hex();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let server = MarbleServer::new(MarbleServerParts {
        listener,
        state: Arc::new(Mutex::new(ProductState::default())),
        store: None,
        approvals: ApprovalInbox::default(),
        events: EventBus::default(),
        turn_tx: TurnSender::detached(),
        steer_tx: mpsc::channel().0,
        policy: AccessPolicy::new(Some("tokentest".into())),
        cancels: CancelRegistry::default(),
    })
    .with_tls(Arc::new(identity));
    std::thread::spawn(move || {
        let _ = server.serve();
    });

    // Plain HTTP keeps working on the same port.
    let mut stream = connect(port);
    stream
        .write_all(b"GET /healthz HTTP/1.1\r\nHost: x\r\nConnection: close\r\n\r\n")
        .unwrap();
    let mut plain = String::new();
    stream.read_to_string(&mut plain).unwrap();
    assert!(plain.starts_with("HTTP/1.1 200"), "{plain}");

    // tls-proof is TLS-only: the same request over plain HTTP gets 400.
    let mut stream = connect(port);
    let nonce = "a".repeat(64);
    stream
        .write_all(
            format!(
                "GET /api/tls-proof?nonce={nonce} HTTP/1.1\r\nHost: x\r\nConnection: close\r\n\r\n"
            )
            .as_bytes(),
        )
        .unwrap();
    let mut refused = String::new();
    stream.read_to_string(&mut refused).unwrap();
    assert!(refused.starts_with("HTTP/1.1 400"), "{refused}");
    assert!(refused.contains("tls_only"), "{refused}");

    // TLS: healthz answers inside the handshake-protected stream.
    let secure = tls_get(port, "/healthz");
    assert!(secure.contains("200"), "{secure}");
    assert!(secure.contains("\"ok\":true"), "{secure}");

    // tls-proof over TLS: verify the HMAC the app recomputes.
    let proof_reply = tls_get(port, &format!("/api/tls-proof?nonce={nonce}"));
    let body = proof_reply
        .split("\r\n\r\n")
        .nth(1)
        .unwrap_or(&proof_reply)
        .trim()
        .to_string();
    let body: Value = serde_json::from_str(&body)
        .unwrap_or_else(|_| panic!("tls-proof did not return JSON: {proof_reply}"));
    assert_eq!(body["certSha256"], serde_json::json!(leaf));
    let expected = glass_marble::tls::tls_proof("tokentest", &nonce, &leaf);
    assert_eq!(body["proof"], serde_json::json!(expected));

    let _ = std::fs::remove_dir_all(&root);
}

/// A message held while admission is paused still runs to completion when the
/// queue resumes — the `202 queued` promise is kept, not just relayed.
#[test]
fn a_paused_queue_resumes_and_the_turn_completes() {
    if !python3_available() {
        return;
    }
    let root = std::env::temp_dir().join(format!("glass-marble-resume-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    let script = root.join("fake_glass_core_runtime.py");
    std::fs::write(&script, include_str!("fixtures/fake_glass_core_runtime.py")).unwrap();
    let storage_dir = root.join("sessions");

    let events = EventBus::default();
    let state = Arc::new(Mutex::new(ProductState::default()));
    let mut python = Command::new("python3");
    python.arg(&script).arg("runtime");
    let conversation = Conversation::spawn_with_router(
        python,
        &root,
        &storage_dir,
        HostPolicy::approving(),
        ApprovalInbox::default(),
        ToolRouter::new(&root).unwrap(),
        events.clone(),
    )
    .unwrap();
    let turn_tx = spawn_single_lane(
        conversation,
        Arc::clone(&state),
        None,
        LiveStore::in_data_dir(&root),
        events.clone(),
        Duration::from_secs(60),
        CancelRegistry::default(),
    );
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let server = MarbleServer::new(MarbleServerParts {
        listener,
        state: Arc::clone(&state),
        store: None,
        approvals: ApprovalInbox::default(),
        events,
        turn_tx,
        steer_tx: mpsc::channel().0,
        policy: AccessPolicy::new(None),
        cancels: CancelRegistry::default(),
    });

    assert_eq!(server.handle("POST", "/api/control/pause", "").0, 200);
    let (status, body) = server.handle("POST", "/api/turns", r#"{"text":"resume me"}"#);
    assert_eq!(status, 202, "{body}");
    assert_eq!(
        serde_json::from_str::<Value>(&body).unwrap()["queued"],
        json!(true)
    );
    assert_eq!(server.handle("POST", "/api/control/resume", "").0, 200);

    let deadline = Instant::now() + Duration::from_secs(45);
    loop {
        let (status, body) = server.handle("GET", "/api/state", "");
        assert_eq!(status, 200);
        let doc: Value = serde_json::from_str(&body).unwrap();
        let tasks = doc["tasks"].as_array().unwrap();
        if let Some(task) = tasks
            .iter()
            .find(|task| task["prompt"] == json!("resume me"))
        {
            if task["status"] == json!("completed") {
                let entries: Vec<&Value> = doc["transcript"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .filter(|entry| entry["taskId"] == task["id"])
                    .collect();
                let kinds: Vec<&str> = entries
                    .iter()
                    .filter_map(|entry| entry["kind"].as_str())
                    .collect();
                assert_eq!(kinds, ["user", "assistant"], "{doc}");
                assert_eq!(entries[1]["text"], json!("handled resume me"));
                break;
            }
            assert_ne!(
                task["status"],
                json!("failed"),
                "the resumed turn failed: {doc}"
            );
        }
        assert!(
            Instant::now() < deadline,
            "resumed turn did not settle: {body}"
        );
        std::thread::sleep(Duration::from_millis(50));
    }

    let _ = std::fs::remove_dir_all(&root);
}

/// A queued placeholder is a promise. When the lane that would run it can
/// never start, the row fails in bounded time instead of sitting in `queued`
/// until a restart turns it "interrupted".
#[test]
fn a_lane_that_cannot_start_fails_the_queued_placeholder() {
    if !python3_available() {
        return;
    }
    let root = std::env::temp_dir().join(format!("glass-marble-deadlane-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    let script = root.join("fake_glass_core_runtime.py");
    std::fs::write(&script, include_str!("fixtures/fake_glass_core_runtime.py")).unwrap();
    let storage_dir = root.join("sessions");

    let events = EventBus::default();
    let state = Arc::new(Mutex::new(ProductState::default()));
    let factory: LaneFactory = {
        let events = events.clone();
        let root = root.clone();
        let storage_dir = storage_dir.clone();
        Arc::new(move |lane| {
            if lane > 0 {
                // A spawned lane takes its engine-start failure on its own
                // thread. The sleep keeps the admission window from answering
                // the turn before the failure, which is the `202 queued` path
                // under test.
                std::thread::sleep(Duration::from_secs(3));
                return Err(MarbleError::Protocol("no engine for this lane".into()));
            }
            let mut python = Command::new("python3");
            python.arg(&script).arg("runtime");
            Conversation::spawn_with_router(
                python,
                &root,
                &storage_dir,
                HostPolicy::approving(),
                ApprovalInbox::default(),
                ToolRouter::new(&root).unwrap(),
                events.clone(),
            )
        })
    };
    let pool = spawn_turn_pool(
        2,
        factory,
        Arc::clone(&state),
        None,
        LiveStore::in_data_dir(&root),
        events.clone(),
        Duration::from_secs(60),
        CancelRegistry::default(),
    )
    .expect("a two lane pool");
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let server = MarbleServer::new(MarbleServerParts {
        listener,
        state: Arc::clone(&state),
        store: None,
        approvals: ApprovalInbox::default(),
        events,
        turn_tx: pool.sender,
        steer_tx: mpsc::channel().0,
        policy: AccessPolicy::new(None),
        cancels: CancelRegistry::default(),
    });

    // A fresh chat picks the least loaded lane — the one that cannot start.
    let (status, body) = server.handle("POST", "/api/turns", r#"{"text":"anyone there"}"#);
    assert_eq!(status, 202, "{body}");
    assert_eq!(
        serde_json::from_str::<Value>(&body).unwrap()["queued"],
        json!(true)
    );

    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        let (status, body) = server.handle("GET", "/api/state", "");
        assert_eq!(status, 200);
        let doc: Value = serde_json::from_str(&body).unwrap();
        let tasks = doc["tasks"].as_array().unwrap();
        if let Some(task) = tasks
            .iter()
            .find(|task| task["prompt"] == json!("anyone there"))
        {
            if task["status"] == json!("failed") {
                break;
            }
            assert_eq!(
                task["status"],
                json!("queued"),
                "the placeholder settled to an unexpected state: {doc}"
            );
        }
        assert!(
            Instant::now() < deadline,
            "the queued placeholder never failed: {body}"
        );
        std::thread::sleep(Duration::from_millis(50));
    }

    let _ = std::fs::remove_dir_all(&root);
}

/// One Prism pool around the fake engine, with one factory recipe.
fn fixture_prism_pool(
    script: &std::path::Path,
    root: &std::path::Path,
    state: &Arc<Mutex<ProductState>>,
    events: &EventBus,
    extra_args: Vec<String>,
) -> Arc<glass_marble::PrismPool> {
    fixture_prism_pool_with_store(script, root, state, events, extra_args, None)
}

fn fixture_prism_pool_with_store(
    script: &std::path::Path,
    root: &std::path::Path,
    state: &Arc<Mutex<ProductState>>,
    events: &EventBus,
    extra_args: Vec<String>,
    store: Option<Arc<StateStore>>,
) -> Arc<glass_marble::PrismPool> {
    let script = script.to_path_buf();
    let sequence = std::sync::atomic::AtomicUsize::new(0);
    let factory: Arc<dyn Fn() -> Command + Send + Sync> = Arc::new(move || {
        let mut command = Command::new("python3");
        command.arg(&script).arg("runtime");
        if !extra_args.iter().any(|arg| arg == "--id-prefix") {
            command.arg("--id-prefix").arg(format!(
                "prism-{}-",
                sequence.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
            ));
        }
        for arg in &extra_args {
            command.arg(arg);
        }
        command
    });
    Arc::new(glass_marble::PrismPool::new(glass_marble::PrismPoolParts {
        factory,
        cwd: root.to_path_buf(),
        sessions: root.join("prism-sessions"),
        state: Arc::clone(state),
        store,
        live: LiveStore::in_data_dir(root),
        events: events.clone(),
        cancels: CancelRegistry::default(),
        approvals: ApprovalInbox::default(),
        timeout: Duration::from_secs(60),
    }))
}

fn everyone_entries(state: &Arc<Mutex<ProductState>>) -> Vec<TranscriptEntry> {
    let guard = state.lock().unwrap_or_else(|error| error.into_inner());
    guard
        .transcript
        .iter()
        .filter(|entry| entry.conversation_id == "everyone")
        .cloned()
        .collect()
}

struct NamedTurnFixture {
    events: EventBus,
    store: Option<Arc<StateStore>>,
    root: std::path::PathBuf,
    script: std::path::PathBuf,
    state: Arc<Mutex<ProductState>>,
    pool: Arc<glass_marble::PrismPool>,
    server: MarbleServer,
    marble_id: String,
}

impl NamedTurnFixture {
    fn session_root(&self, conversation: &str, marble: &str) -> std::path::PathBuf {
        let isolated = self.root.join("conversation-sessions").join(conversation);
        if isolated.is_dir() {
            isolated
        } else if marble == "marble" {
            self.root.join("default-sessions")
        } else {
            self.root.join("prism-sessions").join(marble)
        }
    }

    fn install_default_worker(&mut self) {
        self.install_default_worker_with_args(Vec::new());
    }

    fn install_default_worker_with_args(&mut self, args: Vec<String>) {
        let root = self.root.clone();
        let script = self.script.clone();
        let state = self.state.clone();
        let engine_events = self.events.clone();
        let factory: LaneFactory = Arc::new(move |_| {
            let mut command = Command::new("python3");
            command
                .arg(&script)
                .arg("runtime")
                .arg("--fail-once")
                .arg("--error-once")
                .arg("--id-prefix")
                .arg("default-")
                .arg("--pre-tool-text")
                .arg("Step commentary")
                .args(&args);
            Conversation::spawn_with_router(
                command,
                &root,
                &root.join("default-sessions"),
                HostPolicy::approving(),
                ApprovalInbox::default(),
                ToolRouter::new(&root)?,
                engine_events.clone(),
            )
            .map(|conversation| conversation.with_product(state.clone()))
        });
        let events = self.events.clone();
        let pool = spawn_turn_pool(
            1,
            factory,
            self.state.clone(),
            self.store.clone(),
            LiveStore::in_data_dir(&self.root),
            events.clone(),
            Duration::from_secs(30),
            CancelRegistry::default(),
        )
        .unwrap();
        self.server = MarbleServer::new(MarbleServerParts {
            listener: TcpListener::bind("127.0.0.1:0").unwrap(),
            state: self.state.clone(),
            store: self.store.clone(),
            approvals: ApprovalInbox::default(),
            events,
            turn_tx: pool.sender,
            steer_tx: mpsc::channel().0,
            policy: AccessPolicy::new(None),
            cancels: CancelRegistry::default(),
        })
        .with_prism(self.pool.clone());
    }
    fn new(label: &str) -> Self {
        Self::with_args(label, Vec::new())
    }

    fn with_args(label: &str, args: Vec<String>) -> Self {
        Self::with_persistence(label, args, false)
    }

    fn with_persistence(label: &str, args: Vec<String>, persistent: bool) -> Self {
        let root =
            std::env::temp_dir().join(format!("glass-marble-{label}-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        let script = root.join("runtime.py");
        std::fs::write(&script, include_str!("fixtures/fake_glass_core_runtime.py")).unwrap();
        let events = EventBus::default();
        let mut initial = ProductState::default();
        initial.control.profile = glass_marble::ControlProfile::Unattended;
        let marble_id = initial
            .prism
            .add("Ada", "writer", "star", 35, "", "")
            .unwrap()
            .id
            .clone();
        let state = Arc::new(Mutex::new(initial));
        let store = persistent.then(|| Arc::new(StateStore::in_data_dir(&root)));
        let pool =
            fixture_prism_pool_with_store(&script, &root, &state, &events, args, store.clone());
        let server = MarbleServer::new(MarbleServerParts {
            listener: TcpListener::bind("127.0.0.1:0").unwrap(),
            state: Arc::clone(&state),
            store: store.clone(),
            approvals: ApprovalInbox::default(),
            events: events.clone(),
            turn_tx: TurnSender::detached(),
            steer_tx: mpsc::channel().0,
            policy: AccessPolicy::new(None),
            cancels: CancelRegistry::default(),
        })
        .with_prism(Arc::clone(&pool));
        Self {
            events: events.clone(),
            store,
            root,
            script,
            state,
            pool,
            server,
            marble_id,
        }
    }

    fn post(&self, path: &str, body: Value) -> (u16, Value) {
        let (status, body) = self.server.handle("POST", path, &body.to_string());
        (status, serde_json::from_str(&body).unwrap())
    }

    fn settled(&self, prompt: &str) -> TaskRecord {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            if let Some(task) = self
                .state
                .lock()
                .unwrap()
                .tasks
                .iter()
                .rev()
                .find(|task| {
                    task.prompt == prompt
                        && matches!(task.status.as_str(), "completed" | "failed" | "cancelled")
                })
                .cloned()
            {
                return task;
            }
            assert!(Instant::now() < deadline, "no settled task for {prompt}");
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    fn completed(&self, prompt: &str) -> TaskRecord {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            let task = self.settled(prompt);
            if task.status == "completed" {
                return task;
            }
            assert!(Instant::now() < deadline, "no completed task for {prompt}");
            std::thread::sleep(Duration::from_millis(10));
        }
    }
}

#[test]
fn device_time_zone_reaches_the_agent_shell_and_rejects_paths() {
    if !python3_available() {
        return;
    }
    let fixture = NamedTurnFixture::with_persistence(
        "device-time-zone",
        vec!["--report-shell-time-zone".into()],
        true,
    );
    let host_zone = Command::new("date").arg("+%z").output().unwrap();
    assert!(host_zone.status.success());
    let host_offset = String::from_utf8(host_zone.stdout).unwrap();
    for (zone, expected) in [
        ("Asia/Kuala_Lumpur", "+0800"),
        ("../../etc/passwd", host_offset.trim()),
        ("Etc/UTC", "+0000"),
    ] {
        let prompt = format!("Report shell time for {zone}");
        let head = glass_marble::server::RequestHead::new("POST", "/api/turns")
            .with_header("x-marble-time-zone", zone);
        let response = fixture.server.handle_request(
            &head,
            &json!({"marbleId": fixture.marble_id, "text": prompt}).to_string(),
        );
        assert_eq!(response.status, 202, "{}", response.body);
        let task = fixture.completed(&prompt);
        assert!(fixture.server.state().transcript.iter().any(|row| {
            row.task_id == task.id && row.kind == "assistant" && row.text == expected
        }));
    }
}

#[test]
fn paused_turns_keep_the_sending_devices_time_zone() {
    if !python3_available() {
        return;
    }
    let fixture = NamedTurnFixture::with_persistence(
        "queued-device-time-zones",
        vec!["--report-shell-time-zone".into()],
        true,
    );
    assert_eq!(fixture.post("/api/control/pause", json!({})).0, 200);
    let turns = [
        ("First phone shell time", "Asia/Kuala_Lumpur", "+0800"),
        ("Second phone shell time", "Etc/UTC", "+0000"),
    ];
    for (prompt, zone, _) in turns {
        let head = RequestHead::new("POST", "/api/turns").with_header("x-marble-time-zone", zone);
        let response = fixture.server.handle_request(
            &head,
            &json!({"marbleId": fixture.marble_id, "text": prompt}).to_string(),
        );
        assert_eq!(response.status, 202, "{}", response.body);
        assert_eq!(
            serde_json::from_str::<Value>(&response.body).unwrap()["paused"],
            true
        );
    }
    assert_eq!(fixture.post("/api/control/resume", json!({})).0, 200);
    for (prompt, _, expected) in turns {
        let task = fixture.completed(prompt);
        assert!(
            fixture.server.state().transcript.iter().any(|row| {
                row.task_id == task.id && row.kind == "assistant" && row.text == expected
            }),
            "wrong shell time for {prompt}"
        );
    }
}

#[test]
fn steering_targets_the_named_engine_mid_reply_without_touching_other_marbles() {
    if !python3_available() {
        return;
    }
    let fixture = NamedTurnFixture::with_args("named-steer", vec!["--wait-for-steer".into()]);
    let ben = fixture
        .state
        .lock()
        .unwrap()
        .prism
        .add("Ben", "", "hex", 12, "", "")
        .unwrap()
        .id
        .clone();
    let events = fixture.events.subscribe();
    let dispatch = |marble_id: &str, prompt: &str| {
        let (status, body) =
            fixture.post("/api/turns", json!({"marbleId": marble_id, "text": prompt}));
        assert_eq!(status, 202, "{body}");
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            let event = events
                .recv_timeout(deadline.saturating_duration_since(Instant::now()))
                .expect("named turn starts");
            if event.kind == "turn_start" {
                return event;
            }
        }
    };
    let ada = dispatch(&fixture.marble_id, "Plan a riverside walk");
    let ben = dispatch(&ben, "Plan a different walk");
    assert_ne!(ada.conversation_id, ben.conversation_id);

    let (status, _) = fixture.post(
        "/api/turns/steer",
        json!({
            "conversationId": ben.conversation_id,
            "taskId": ada.task_id,
            "text": "Do not send this correction"
        }),
    );
    assert_eq!(status, 409);

    let (status, body) = fixture.post(
        "/api/turns/steer",
        json!({
            "conversationId": ada.conversation_id,
            "taskId": ada.task_id,
            "text": "Choose a shaded trail instead."
        }),
    );
    assert_eq!(status, 202, "{body}");
    let settled = fixture.completed("Plan a riverside walk");
    assert_eq!(settled.id, ada.task_id);
    {
        let state = fixture.state.lock().unwrap();
        assert!(state.transcript.iter().any(|entry| {
            entry.task_id == ada.task_id
                && entry.kind == "assistant"
                && entry
                    .text
                    .contains("With your correction in mind: Choose a shaded trail instead.")
        }));
        assert!(
            state
                .tasks
                .iter()
                .any(|task| task.id == ben.task_id && task.status == "running")
        );
    }
    let (status, _) = fixture.post(
        "/api/turns/steer",
        json!({
            "conversationId": ada.conversation_id,
            "taskId": ada.task_id,
            "text": "Do not redirect this to Ben"
        }),
    );
    assert_eq!(status, 409);
    let (status, body) = fixture.post(
        "/api/turns/steer",
        json!({
            "conversationId": ben.conversation_id,
            "text": "Choose a shorter trail."
        }),
    );
    assert_eq!(status, 202, "{body}");
    assert_eq!(fixture.completed("Plan a different walk").id, ben.task_id);
    fixture.pool.shutdown();
    let _ = std::fs::remove_dir_all(&fixture.root);
}

#[test]
fn client_message_ids_correlate_and_deduplicate_all_chat_send_paths() {
    if !python3_available() {
        return;
    }
    for target in [
        "default-first",
        "default-private",
        "named-first",
        "named-private",
        "dispatch",
        "everyone",
    ] {
        for threaded in [false, true] {
            let mut fixture = NamedTurnFixture::with_persistence(
                &format!("client-send-{target}-{threaded}"),
                vec![],
                true,
            );
            fixture.install_default_worker();
            let everyone = target == "everyone";
            let named = target.starts_with("named") || target == "dispatch";
            let path = if everyone {
                "/api/everyone"
            } else if target == "dispatch" {
                "/api/prism/dispatch"
            } else {
                "/api/turns"
            };
            let mut request =
                json!({"text":if everyone { "@everyone client send" } else { "client send" }});
            if named {
                if target == "dispatch" {
                    request["marbles"] = json!([fixture.marble_id]);
                } else {
                    request["marbleId"] = json!(fixture.marble_id);
                }
            }
            if threaded || target.ends_with("private") {
                let mut bootstrap = request.clone();
                bootstrap["text"] = json!(if everyone { "@Ada parent" } else { "parent" });
                let (status, response) = fixture.post(path, bootstrap.clone());
                assert_eq!(status, 202, "{response}");
                assert!(response.get("client_message_id").is_none());
                assert!(response.get("entry_id").is_none());
                let task = fixture.completed(bootstrap["text"].as_str().unwrap());
                let cv = if everyone {
                    "everyone"
                } else {
                    &task.conversation_id
                };
                if target.ends_with("private") {
                    request["conversationId"] = json!(cv);
                }
                if threaded {
                    let parent = fixture
                        .state
                        .lock()
                        .unwrap()
                        .transcript
                        .iter()
                        .find(|entry| entry.conversation_id == cv && entry.kind == "user")
                        .unwrap()
                        .id
                        .clone();
                    let (status, thread) = fixture.post(
                        "/api/threads",
                        json!({"conversation_id":cv,"parent_entry_id":parent}),
                    );
                    assert_eq!(status, 200, "{thread}");
                    request["thread_id"] = thread["thread"]["id"].clone();
                }
            }
            request[if threaded {
                "clientMessageId"
            } else {
                "client_message_id"
            }] = json!("ios-send-1");
            let ((status, accepted), (duplicate_status, duplicate)) = std::thread::scope(|scope| {
                let first = scope.spawn(|| fixture.post(path, request.clone()));
                let second = scope.spawn(|| fixture.post(path, request.clone()));
                (first.join().unwrap(), second.join().unwrap())
            });
            assert_eq!(status, 202, "{accepted}");
            assert_eq!(duplicate_status, status);
            assert_eq!(duplicate, accepted);
            assert_eq!(accepted["client_message_id"], "ios-send-1");
            assert!(accepted["entry_id"].is_string());
            let prompt = request["text"].as_str().unwrap();
            let deadline = Instant::now() + Duration::from_secs(5);
            loop {
                let state = fixture.state.lock().unwrap();
                let tasks: Vec<_> = state
                    .tasks
                    .iter()
                    .filter(|task| task.prompt == prompt)
                    .collect();
                if tasks.len() == if everyone { 2 } else { 1 }
                    && tasks.iter().all(|task| task.status == "completed")
                {
                    break;
                }
                drop(state);
                assert!(
                    Instant::now() < deadline,
                    "send did not complete: {target} {threaded}"
                );
                std::thread::sleep(Duration::from_millis(10));
            }
            let saved = fixture.store.as_ref().unwrap().load().unwrap();
            let entries: Vec<_> = saved
                .transcript
                .iter()
                .filter(|entry| entry.client_message_id.as_deref() == Some("ios-send-1"))
                .collect();
            assert_eq!(entries.len(), 1, "{target} {threaded}: {entries:?}");
            let entry = entries[0];
            assert_eq!(entry.id, accepted["entry_id"]);
            assert_eq!(entry.kind, "user");
            assert_eq!(entry.thread_id.as_deref(), request["thread_id"].as_str());
            let serialized = serde_json::to_value(entry).unwrap();
            assert_eq!(serialized["client_message_id"], "ios-send-1");
            assert!(serialized.get("clientMessageId").is_none());
            let view = if threaded {
                format!("/api/threads/{}", request["thread_id"].as_str().unwrap())
            } else if everyone {
                "/api/everyone".into()
            } else {
                format!("/api/transcript?conversationId={}", entry.conversation_id)
            };
            let (view_status, view_body) = fixture.server.handle("GET", &view, "");
            assert_eq!(view_status, 200, "{view_body}");
            let view: Value = serde_json::from_str(&view_body).unwrap();
            assert_eq!(
                view["entries"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .find(|item| item["id"] == entry.id)
                    .unwrap()["client_message_id"],
                "ios-send-1"
            );
            let events = fixture.events.since(0);
            let entry_events: Vec<_> = events
                .iter()
                .filter(|event| event.kind == "entry" && event.data["id"] == entry.id)
                .collect();
            assert_eq!(entry_events.len(), 1, "{target} {threaded}");
            assert_eq!(entry_events[0].data["client_message_id"], "ios-send-1");
            let restarted = free_server(saved);
            let (status, replay) = restarted.handle("POST", path, &request.to_string());
            assert_eq!(status, 202, "{replay}");
            assert_eq!(serde_json::from_str::<Value>(&replay).unwrap(), accepted);
            let (status, replay) = fixture.post(path, request.clone());
            assert_eq!(status, 202, "{replay}");
            assert_eq!(replay, accepted);
            let mut altered = request.clone();
            altered["text"] = json!("different text on a retried request");
            assert_eq!(fixture.post(path, altered).1, accepted);
        }
    }
}

#[test]
fn client_message_ids_survive_paused_admission_and_remain_chat_scoped() {
    if !python3_available() {
        return;
    }
    for named in [false, true] {
        let mut fixture =
            NamedTurnFixture::with_persistence(&format!("client-send-held-{named}"), vec![], true);
        fixture.install_default_worker();
        fixture.state.lock().unwrap().control.paused = true;
        let request = json!({"text":"held client message", "marbleId":named.then_some(&fixture.marble_id), "client_message_id":"held-ios"});
        let (status, accepted) = fixture.post("/api/turns", request.clone());
        assert_eq!(status, 202, "{accepted}");
        assert_eq!(accepted["paused"], true);
        assert_eq!(fixture.post("/api/turns", request.clone()).1, accepted);
        let saved = fixture.store.as_ref().unwrap().load().unwrap();
        assert_eq!(saved.control.queue.len(), 1);
        let restarted = free_server(saved);
        assert_eq!(
            serde_json::from_str::<Value>(
                &restarted
                    .handle("POST", "/api/turns", &request.to_string())
                    .1
            )
            .unwrap(),
            accepted
        );
        assert_eq!(fixture.post("/api/control/resume", json!({})).0, 200);
        let task = fixture.completed("held client message");
        let state = fixture.store.as_ref().unwrap().load().unwrap();
        let entries: Vec<_> = state
            .transcript
            .iter()
            .filter(|entry| entry.client_message_id.as_deref() == Some("held-ios"))
            .collect();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].id, accepted["entry_id"]);
        assert_eq!(entries[0].task_id, task.id);
        assert_eq!(fixture.post("/api/turns", request.clone()).1, accepted);
        let mut explicit = request.clone();
        explicit["conversationId"] = json!(task.conversation_id);
        assert_eq!(fixture.post("/api/turns", explicit).1, accepted);
        let other = json!({"text":"other chat", "client_message_id":"held-ios"});
        let (status, response) = fixture.post("/api/everyone", other);
        assert_eq!(status, 202, "{response}");
        assert_ne!(response["entry_id"], accepted["entry_id"]);
    }
}

#[test]
fn invalid_client_message_ids_are_rejected_without_side_effects() {
    for path in ["/api/turns", "/api/everyone", "/api/prism/dispatch"] {
        let server = free_server(ProductState::default());
        for invalid in [
            json!(""),
            json!("x".repeat(129)),
            json!("bad\nvalue"),
            json!("bad\u{7f}"),
            json!("é"),
            json!(12),
            Value::Null,
        ] {
            let (status, body) = server.handle(
                "POST",
                path,
                &json!({"text":"hello", "client_message_id":invalid}).to_string(),
            );
            assert_eq!(status, 400, "{body}");
            assert_eq!(
                serde_json::from_str::<Value>(&body).unwrap()["error"],
                "invalid_client_message_id"
            );
        }
        let (status, _) = server.handle(
            "POST",
            path,
            &json!({"text":"hello", "client_message_id":"one", "clientMessageId":"two"})
                .to_string(),
        );
        assert_eq!(status, 400);
        let (status, _) = server.handle(
            "POST",
            path,
            &json!({"text":" ", "client_message_id":"empty"}).to_string(),
        );
        assert_eq!(status, 400);
        assert!(server.state().client_sends.is_empty());
        assert!(server.state().transcript.is_empty());
    }
}

#[test]
fn governed_tools_emit_one_lifecycle_per_private_and_everyone_row() {
    if !python3_available() {
        return;
    }
    for (tool, expected, path) in [
        ("glass.workspace.list", "ok", "."),
        ("glass.workspace.read", "failed", "missing-file"),
    ] {
        let args = vec![
            "--core-tool-progress".into(),
            "--tool".into(),
            tool.into(),
            "--path".into(),
            path.into(),
        ];
        let mut fixture = NamedTurnFixture::with_persistence(
            &format!("everyone-governed-events-{expected}"),
            args.clone(),
            true,
        );
        fixture.install_default_worker_with_args(args);
        for (text, named, everyone) in [
            ("private default tool", false, false),
            ("private named tool", true, false),
            ("@Marble shared tool", false, true),
            ("@Ada shared tool", true, true),
        ] {
            let path = if everyone {
                "/api/everyone"
            } else {
                "/api/turns"
            };
            let (status, body) = fixture.post(
                path,
                json!({"text":text, "marbleId":named.then_some(&fixture.marble_id)}),
            );
            assert_eq!(status, 202, "{body}");
            let task = fixture.completed(text);
            let state = fixture.store.as_ref().unwrap().load().unwrap();
            let rows: Vec<_> = state
                .transcript
                .iter()
                .filter(|e| e.task_id == task.id && e.kind == "tool")
                .collect();
            assert_eq!(rows.len(), if everyone { 2 } else { 1 });
            let events = fixture.events.since(0);
            for row in rows {
                assert!(
                    row.text.ends_with(&format!(" · {expected}")),
                    "{}",
                    row.text
                );
                let lifecycle: Vec<_> = events
                    .iter()
                    .filter(|event| {
                        event.task_id == task.id
                            && event.conversation_id == row.conversation_id
                            && event.data["entry_id"] == row.id
                            && matches!(event.kind.as_str(), "tool_start" | "tool_end")
                    })
                    .collect();
                assert_eq!(
                    lifecycle.len(),
                    2,
                    "duplicate lifecycle for {text} in {}",
                    row.conversation_id
                );
                assert_eq!(lifecycle[0].kind, "tool_start");
                assert_eq!(lifecycle[1].kind, "tool_end");
                assert_eq!(
                    lifecycle[0].data["requestId"],
                    lifecycle[1].data["requestId"]
                );
                assert!(lifecycle[0].data["requestId"].is_string());
                assert_eq!(lifecycle[0].data["callId"], lifecycle[1].data["callId"]);
                assert_eq!(lifecycle[1].data["tool"], tool);
                assert_eq!(lifecycle[1].data["isError"], expected == "failed");
            }
        }
    }
}

#[test]
fn everyone_default_keeps_history_after_a_new_private_chat() {
    if !python3_available() {
        return;
    }
    let mut fixture = NamedTurnFixture::with_persistence("everyone-stable-default", vec![], true);
    fixture.install_default_worker_with_args(vec![]);
    for text in [
        "@Marble remember Everyone",
        "new private chat",
        "@Marble recall Everyone",
    ] {
        let path = if text.starts_with('@') {
            "/api/everyone"
        } else {
            "/api/turns"
        };
        let (status, body) = fixture.post(path, json!({"text":text}));
        assert_eq!(status, 202, "{body}");
        fixture.completed(text);
        if text == "new private chat" {
            let mut state = fixture.state.lock().unwrap();
            *state = serde_json::from_value(serde_json::to_value(&*state).unwrap()).unwrap();
        }
    }
    let state = fixture.state.lock().unwrap();
    let conversation = |prompt| {
        state
            .tasks
            .iter()
            .find(|task| task.prompt == prompt)
            .unwrap()
            .conversation_id
            .clone()
    };
    let original = conversation("@Marble remember Everyone");
    assert_ne!(conversation("new private chat"), original);
    assert_eq!(
        conversation("@Marble recall Everyone"),
        original,
        "Everyone lost its engine history after a new private chat"
    );
}

#[test]
fn explicit_reasoning_is_private_for_all_agents_and_chats() {
    if !python3_available() {
        return;
    }
    let args = vec!["--reasoning-channels".into()];
    let mut fixture = NamedTurnFixture::with_persistence("reasoning-channels", args.clone(), true);
    fixture.install_default_worker_with_args(args);
    for id in ["marble".to_string(), fixture.marble_id.clone()] {
        let text = format!("private reasoning {id}");
        let (status, body) = fixture.post(
            "/api/turns",
            json!({"text":text,"marbleId":(id != "marble").then_some(id)}),
        );
        assert_eq!(status, 202, "{body}");
        fixture.completed(&text);
    }
    let (status, body) = fixture.post("/api/everyone", json!({"text":"@everyone reasoning"}));
    assert_eq!(status, 202, "{body}");
    let deadline = Instant::now() + Duration::from_secs(5);
    while fixture
        .events
        .since(0)
        .iter()
        .filter(|e| e.kind == "reply" && e.conversation_id == "everyone")
        .count()
        < 2
    {
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(10));
    }
    let state = fixture.store.as_ref().unwrap().load().unwrap();
    for conversation in ["everyone".to_string()]
        .into_iter()
        .chain(state.tasks.iter().map(|t| t.conversation_id.clone()))
    {
        let entries: Vec<_> = state
            .transcript
            .iter()
            .filter(|e| e.conversation_id == conversation)
            .collect();
        assert!(
            entries
                .iter()
                .any(|e| e.kind == "assistant" && e.text == "Public answer.")
        );
        if conversation != "everyone" {
            assert!(
                entries
                    .iter()
                    .any(|e| e.kind == "reasoning" && e.text.contains("secret."))
            );
        }
        assert!(
            entries
                .iter()
                .filter(|e| e.kind == "assistant")
                .all(|e| e.text == "Public answer.")
        );
    }
    for event in fixture
        .events
        .since(0)
        .iter()
        .filter(|e| matches!(e.kind.as_str(), "text" | "reply"))
    {
        assert!(!event.data.to_string().contains("secret."));
    }
}

#[test]
fn everyone_reasoning_survives_paging_and_server_restart_with_stable_ids() {
    if !python3_available() {
        return;
    }
    let args = vec!["--everyone-reasoning".into()];
    let mut fixture =
        NamedTurnFixture::with_persistence("everyone-durable-reasoning", args.clone(), true);
    fixture
        .state
        .lock()
        .unwrap()
        .prism
        .add("Dietrich", "tester", "square", 200, "", "")
        .unwrap();
    fixture.install_default_worker_with_args(args);
    assert_eq!(
        fixture
            .post(
                "/api/everyone",
                json!({"text":"@everyone durable reasoning"})
            )
            .0,
        202
    );
    let deadline = Instant::now() + Duration::from_secs(10);
    while fixture
        .events
        .since(0)
        .iter()
        .filter(|event| event.kind == "reply" && event.conversation_id == "everyone")
        .count()
        < 3
    {
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(10));
    }
    let read = |server: &MarbleServer, path: &str| {
        let (status, body) = server.handle("GET", path, "");
        assert_eq!(status, 200, "{body}");
        serde_json::from_str::<Value>(&body).unwrap()["entries"].clone()
    };
    let entries = read(&fixture.server, "/api/everyone");
    assert_eq!(entries, read(&fixture.server, "/api/everyone?limit=50"));
    let rows = entries.as_array().unwrap();
    assert_eq!(
        rows.iter()
            .take(7)
            .map(|entry| entry["kind"].as_str().unwrap())
            .collect::<Vec<_>>(),
        [
            "user",
            "reasoning",
            "assistant",
            "reasoning",
            "assistant",
            "reasoning",
            "assistant"
        ]
    );
    for name in ["Marble", "Ada", "Dietrich"] {
        let index = rows
            .iter()
            .position(|row| row["kind"] == "reasoning" && row["speakerName"] == name)
            .unwrap();
        let expected = if name == "Ada" {
            format!("Channel for {name}.")
        } else {
            format!("Channel for {name}.\n\nPlanning for {name}.")
        };
        assert_eq!(rows[index]["text"], expected);
        assert_eq!(rows[index + 1]["kind"], "assistant");
        assert_eq!(rows[index + 1]["text"], "Public answer.");
        assert_eq!(rows[index]["taskId"], rows[index + 1]["taskId"]);
        assert!(
            rows.iter()
                .any(|row| row["kind"] == "tool" && row["taskId"] == rows[index]["taskId"])
        );
    }
    fixture.pool.shutdown();
    let mut reloaded = fixture.store.as_ref().unwrap().load().unwrap();
    assert_eq!(reloaded.recover_interrupted(), 0);
    assert!(
        LiveStore::in_data_dir(&fixture.root)
            .recover(&mut reloaded)
            .is_empty()
    );
    assert_eq!(
        serde_json::to_value(reloaded.transcript_for("everyone")).unwrap(),
        entries
    );
    assert_eq!(
        reloaded
            .transcript
            .iter()
            .filter(|entry| entry.kind == "reasoning")
            .count(),
        6
    );
    for row in rows.iter().filter(|row| row["kind"] == "reasoning") {
        let source = reloaded
            .transcript
            .iter()
            .find(|entry| {
                entry.conversation_id != "everyone"
                    && entry.kind == "reasoning"
                    && entry.task_id == row["taskId"].as_str().unwrap()
            })
            .unwrap();
        assert_eq!(row["id"], format!("{}-everyone", source.id));
    }
    let server = MarbleServer::new(MarbleServerParts {
        listener: TcpListener::bind("127.0.0.1:0").unwrap(),
        state: Arc::new(Mutex::new(reloaded)),
        store: fixture.store.clone(),
        approvals: ApprovalInbox::default(),
        events: EventBus::default(),
        turn_tx: TurnSender::detached(),
        steer_tx: mpsc::channel().0,
        policy: AccessPolicy::new(None),
        cancels: CancelRegistry::default(),
    });
    assert_eq!(read(&server, "/api/everyone"), entries);
    assert_eq!(read(&server, "/api/everyone?limit=50"), entries);
    println!("EVERYONE_REASONING_ENDPOINT={entries}");
}

#[test]
fn plain_text_blocks_never_publish_planning_as_replies_for_any_agent_or_chat() {
    if !python3_available() {
        return;
    }
    let args = vec!["--text-blocks".into()];
    let mut fixture = NamedTurnFixture::with_persistence("plain-text-blocks", args.clone(), true);
    let dietrich = fixture
        .state
        .lock()
        .unwrap()
        .prism
        .add("Dietrich", "tester", "square", 200, "", "")
        .unwrap()
        .id
        .clone();
    fixture.install_default_worker_with_args(args);
    for id in ["marble".to_string(), fixture.marble_id.clone(), dietrich] {
        let text = format!("private blocks {id}");
        assert_eq!(
            fixture
                .post(
                    "/api/turns",
                    json!({"text":text,"marbleId":(id != "marble").then_some(id)})
                )
                .0,
            202
        );
        fixture.completed(&text);
    }
    assert_eq!(
        fixture
            .post("/api/everyone", json!({"text":"@everyone blocks"}))
            .0,
        202
    );
    let deadline = Instant::now() + Duration::from_secs(5);
    while fixture
        .events
        .since(0)
        .iter()
        .filter(|event| event.kind == "reply" && event.conversation_id == "everyone")
        .count()
        < 3
    {
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(10));
    }
    let state = fixture.store.as_ref().unwrap().load().unwrap();
    assert_eq!(state.tasks.len(), 6);
    for task in &state.tasks {
        let rows: Vec<_> = state
            .transcript
            .iter()
            .filter(|row| row.task_id == task.id && row.conversation_id == task.conversation_id)
            .collect();
        assert!(
            rows.iter()
                .any(|row| row.kind == "assistant" && row.text == "Public answer.")
        );
        assert!(
            rows.iter()
                .any(|row| row.kind == "reasoning"
                    && row.text == "Planning secret.\n\nMore planning.")
        );
        let mut text = String::new();
        let mut reasoning = String::new();
        for event in fixture.events.since(0).iter().filter(|event| {
            event.task_id == task.id && event.conversation_id == task.conversation_id
        }) {
            match event.kind.as_str() {
                "text" => {
                    let delta = event.data["text"].as_str().unwrap();
                    assert!(!delta.contains("planning") && !delta.contains("Planning"));
                    text.push_str(delta);
                }
                "reasoning" => reasoning.push_str(event.data["text"].as_str().unwrap()),
                "reply" => assert_eq!(event.data["text"], "Public answer."),
                _ => {}
            }
        }
        assert_eq!(text, "Public answer.");
        assert_eq!(reasoning, "Planning secret.\n\nMore planning.");
    }
    assert!(
        state
            .transcript
            .iter()
            .filter(|row| row.kind == "assistant")
            .all(|row| row.text == "Public answer.")
    );
}

#[test]
fn empty_or_failed_final_blocks_never_fall_back_to_planning_text() {
    if !python3_available() {
        return;
    }
    for mode in [
        "--text-blocks-empty",
        "--text-blocks-error",
        "--text-blocks-tool-empty",
    ] {
        let args = vec!["--text-blocks".into(), mode.into()];
        let mut fixture =
            NamedTurnFixture::with_persistence(&format!("blocks-{mode}"), args.clone(), true);
        fixture.install_default_worker_with_args(args);
        for id in ["marble".to_string(), fixture.marble_id.clone()] {
            let prompt = format!("empty blocks {id}");
            assert_eq!(
                fixture
                    .post(
                        "/api/turns",
                        json!({"text":prompt,"marbleId":(id != "marble").then_some(id)})
                    )
                    .0,
                202
            );
            let task = fixture.settled(&prompt);
            assert_eq!(task.status, "failed");
            assert!(
                fixture
                    .state
                    .lock()
                    .unwrap()
                    .transcript
                    .iter()
                    .filter(|row| row.task_id == task.id
                        && matches!(row.kind.as_str(), "assistant" | "failure"))
                    .all(|row| !row.text.contains("planning") && !row.text.contains("Planning"))
            );
            let mut visible = String::new();
            for event in fixture.events.since(0).iter().filter(|event| {
                event.task_id == task.id && event.conversation_id == task.conversation_id
            }) {
                if event.kind == "text" {
                    visible.push_str(event.data["text"].as_str().unwrap());
                }
                if event.kind == "text_reset" {
                    visible.clear();
                }
            }
            assert!(visible.is_empty(), "{visible}");
        }
    }
}

#[test]
fn text_blocks_have_independent_reasoning_tag_state() {
    if !python3_available() {
        return;
    }
    let fixture = NamedTurnFixture::with_persistence(
        "blocks-open-tag",
        vec!["--text-blocks".into(), "--text-blocks-open-tag".into()],
        true,
    );
    assert_eq!(
        fixture
            .post(
                "/api/turns",
                json!({"text":"blocks reset tag state","marbleId":fixture.marble_id})
            )
            .0,
        202
    );
    let task = fixture.completed("blocks reset tag state");
    assert!(
        fixture
            .state
            .lock()
            .unwrap()
            .transcript
            .iter()
            .any(|entry| entry.task_id == task.id
                && entry.kind == "assistant"
                && entry.text == "Public answer.")
    );
}

#[test]
fn everyone_and_private_agents_share_native_tools_and_finish_visible_rows() {
    if !python3_available() {
        return;
    }
    let args = vec![
        "--native-date".into(),
        "--pre-tool-text".into(),
        "Plan one.".into(),
    ];
    let mut fixture =
        NamedTurnFixture::with_persistence("everyone-native-tools", args.clone(), true);
    fixture.state.lock().unwrap().model = "fixture-model".into();
    let dietrich = fixture
        .state
        .lock()
        .unwrap()
        .prism
        .add("Dietrich", "tester", "square", 200, "", "")
        .unwrap()
        .id
        .clone();
    fixture.install_default_worker_with_args(args);
    let agents = [
        ("marble".to_string(), "Marble"),
        (fixture.marble_id.clone(), "Ada"),
        (dietrich, "Dietrich"),
    ];
    let mut private = Vec::new();
    for (id, _) in &agents {
        let text = format!("private date {id}");
        let (status, body) = fixture.post(
            "/api/turns",
            json!({"text":text, "marbleId":(id != "marble").then_some(id)}),
        );
        assert_eq!(status, 202, "{body}");
        let task = fixture.completed(&text);
        private.push(task);
    }
    let cursor = fixture.events.sequence();
    let (status, body) = fixture.post(
        "/api/everyone",
        json!({"text":"@everyone whats the system time?"}),
    );
    assert_eq!(status, 202, "{body}");
    let receiver = fixture.events.subscribe();
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let replies = fixture
            .events
            .since(cursor)
            .iter()
            .filter(|e| e.kind == "reply" && e.conversation_id == "everyone")
            .count();
        if replies == 3 {
            break;
        }
        assert!(Instant::now() < deadline, "Everyone did not finish");
        let _ = receiver.recv_timeout(Duration::from_millis(50));
    }
    let disk = fixture.store.as_ref().unwrap().load().unwrap();
    let events = fixture.events.since(0);
    for ((id, name), previous) in agents.iter().zip(private.iter()) {
        let shared = disk
            .tasks
            .iter()
            .find(|t| {
                (t.marble_id == *id || (id == "marble" && t.marble_id.is_empty()))
                    && t.prompt == "@everyone whats the system time?"
            })
            .unwrap();
        assert_ne!(
            shared.conversation_id, previous.conversation_id,
            "Everyone reused a private conversation"
        );
        for task in [previous, shared] {
            let source: Vec<_> = disk
                .transcript
                .iter()
                .filter(|e| {
                    e.task_id == task.id
                        && e.kind == "tool"
                        && e.conversation_id == task.conversation_id
                })
                .collect();
            assert_eq!(source.len(), 2);
            assert!(
                source
                    .iter()
                    .all(|e| e.text == "bash · date -u -Iseconds · ok")
            );
            let reply = disk
                .transcript
                .iter()
                .find(|e| {
                    e.task_id == task.id
                        && e.kind == "assistant"
                        && e.conversation_id == task.conversation_id
                })
                .unwrap();
            assert!(
                reply.text.starts_with("The system time is "),
                "{}",
                reply.text
            );
            assert!(
                !reply.text.contains('@')
                    && !reply.text.contains("Plan")
                    && !reply.text.contains("commentary")
            );
            let reasoning = disk
                .transcript
                .iter()
                .find(|e| {
                    e.task_id == task.id
                        && e.kind == "reasoning"
                        && e.conversation_id == task.conversation_id
                })
                .unwrap();
            assert!(
                reasoning.text.ends_with("\n\nPlan two."),
                "{}",
                reasoning.text
            );
            for row in source {
                let lifecycle: Vec<_> = events
                    .iter()
                    .filter(|e| {
                        e.task_id == task.id
                            && e.conversation_id == task.conversation_id
                            && e.data["entry_id"] == row.id
                            && matches!(e.kind.as_str(), "tool_start" | "tool_end")
                    })
                    .collect();
                assert_eq!(
                    lifecycle.len(),
                    2,
                    "private lifecycle missing for {}",
                    row.id
                );
                assert_eq!(lifecycle[0].kind, "tool_start");
                assert_eq!(lifecycle[1].kind, "tool_end");
                assert_eq!(lifecycle[0].data["callId"], lifecycle[1].data["callId"]);
                assert_eq!(lifecycle[1].data["tool"], "bash");
                assert_eq!(lifecycle[1].data["isError"], false);
            }
        }
        let mirrors: Vec<_> = disk
            .transcript
            .iter()
            .filter(|e| {
                e.conversation_id == "everyone" && e.task_id == shared.id && e.kind == "tool"
            })
            .collect();
        assert_eq!(mirrors.len(), 2, "Everyone tool rows missing for {name}");
        for row in mirrors {
            assert_eq!(row.speaker_name, *name);
            assert!(row.text.ends_with(" · ok"));
            let lifecycle: Vec<_> = events
                .iter()
                .filter(|e| {
                    e.conversation_id == "everyone"
                        && e.task_id == shared.id
                        && e.data["entry_id"] == row.id
                        && matches!(e.kind.as_str(), "tool_start" | "tool_end")
                })
                .collect();
            assert_eq!(lifecycle.len(), 2);
            assert_eq!(lifecycle[0].kind, "tool_start");
            assert_eq!(lifecycle[1].kind, "tool_end");
            assert_eq!(lifecycle[1].data["sender"], *name);
            assert_eq!(lifecycle[0].data["callId"], lifecycle[1].data["callId"]);
        }
        let visible_reply = disk
            .transcript
            .iter()
            .find(|e| {
                e.task_id == shared.id && e.kind == "assistant" && e.conversation_id == "everyone"
            })
            .unwrap();
        assert_eq!(
            visible_reply.text,
            disk.transcript
                .iter()
                .find(|e| e.task_id == shared.id
                    && e.kind == "assistant"
                    && e.conversation_id == shared.conversation_id)
                .unwrap()
                .text
        );
        let reply_events: Vec<_> = events
            .iter()
            .filter(|e| e.kind == "reply" && e.data["id"] == visible_reply.id)
            .collect();
        assert_eq!(reply_events.len(), 1);
        assert_eq!(reply_events[0].conversation_id, "everyone");
        assert_eq!(reply_events[0].data["sender"], *name);
        assert_eq!(reply_events[0].data["text"], visible_reply.text);
        for task in [previous, shared] {
            let sessions = fixture.session_root(&task.conversation_id, id);
            let executed: Vec<Value> =
                std::fs::read_to_string(sessions.join("fixture-date-executions.jsonl"))
                    .unwrap()
                    .lines()
                    .map(|line| serde_json::from_str(line).unwrap())
                    .collect();
            assert_eq!(executed.len(), if id == "marble" { 4 } else { 2 });
            assert_eq!(executed.iter().filter(|e| e["task"] == task.id).count(), 2);
            let output = executed
                .iter()
                .rev()
                .find(|e| e["task"] == task.id)
                .unwrap()["output"]
                .as_str()
                .unwrap();
            let reply = disk
                .transcript
                .iter()
                .find(|e| {
                    e.task_id == task.id
                        && e.kind == "assistant"
                        && e.conversation_id == task.conversation_id
                })
                .unwrap();
            assert_eq!(reply.text, format!("The system time is {output}."));
        }
        let sessions = fixture.session_root(&shared.conversation_id, id);
        let options: Value = serde_json::from_str(
            &std::fs::read_to_string(sessions.join("fixture-session-options.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(options["toolMode"], "mixed");
        assert_eq!(options["model"], "fixture-model");
        assert!(!options["tools"].as_array().unwrap().is_empty());
        assert!(
            options["instructions"]
                .as_str()
                .unwrap()
                .starts_with(&format!("You are {name}"))
        );
        assert!(
            options["instructions"]
                .as_str()
                .unwrap()
                .contains("Never @mention yourself")
        );
        let history: Value = serde_json::from_str(
            &std::fs::read_to_string(sessions.join("fixture-history.json")).unwrap(),
        )
        .unwrap();
        let conversations = history["conversations"].as_object().unwrap();
        assert_eq!(conversations.len(), if id == "marble" { 2 } else { 1 });
        let shared_engine = &disk
            .conversations
            .iter()
            .find(|record| record.id == shared.conversation_id)
            .unwrap()
            .engine_conversation_id;
        let messages = conversations[shared_engine].as_array().unwrap();
        assert_eq!(messages.len(), 1);
        assert!(
            messages[0]["text"]
                .as_str()
                .unwrap()
                .contains("This message is addressed to: Marble (lead), Ada, Dietrich.")
        );
    }
}

#[test]
fn everyone_handoff_reply_emits_the_visible_delegate_entry_once() {
    if !python3_available() {
        return;
    }
    let fixture = NamedTurnFixture::with_persistence("handoff-reply-event", Vec::new(), true);
    let ben = fixture
        .state
        .lock()
        .unwrap()
        .prism
        .add("Ben", "tester", "square", 200, "", "")
        .unwrap()
        .id
        .clone();
    let receiver = fixture.events.subscribe();
    let (status, body) = fixture.post(
        "/api/everyone",
        json!({"text":"@Ada ASK Ben: check the failing tests"}),
    );
    assert_eq!(status, 202, "{body}");
    loop {
        let event = receiver
            .recv_timeout(Duration::from_secs(5))
            .expect("delegate settlement");
        if event.kind == "state"
            && event.data["marble"] == ben
            && event.data["status"] == "completed"
        {
            break;
        }
    }
    let disk = fixture.store.as_ref().unwrap().load().unwrap();
    let delegate = disk
        .tasks
        .iter()
        .find(|t| t.marble_id == ben && t.status == "completed")
        .unwrap();
    let thread = disk
        .threads
        .iter()
        .find(|t| disk.task_threads.get(&delegate.id) == Some(&t.id))
        .unwrap();
    assert_eq!(thread.conversation_id, "everyone");
    assert!(
        delegate
            .conversation_id
            .starts_with(&format!("everyone-session-{}__", thread.id))
    );
    assert!(disk.prism.get(&ben).unwrap().conversation_id.is_empty());
    let parent = disk
        .transcript
        .iter()
        .find(|e| e.id == thread.parent_entry_id)
        .unwrap();
    assert_eq!(parent.kind, "handoff");
    assert_eq!(parent.handoff.as_ref().unwrap().status, "asked");
    let (status, body) = fixture
        .server
        .handle("GET", &format!("/api/threads/{}", thread.id), "");
    assert_eq!(status, 200, "{body}");
    let detail: Value = serde_json::from_str(&body).unwrap();
    let entries: Vec<TranscriptEntry> = serde_json::from_value(detail["entries"].clone()).unwrap();
    let reply = entries
        .iter()
        .find(|e| e.task_id == delegate.id && e.kind == "assistant")
        .expect("delegate reply is visible in thread detail");
    assert_ne!(reply.conversation_id, "everyone");
    assert!(!disk.transcript.iter().any(|e| e.task_id == delegate.id
        && e.kind == "assistant"
        && e.conversation_id == "everyone"));
    let replies: Vec<_> = fixture
        .events
        .since(0)
        .into_iter()
        .filter(|e| e.kind == "reply" && e.data["id"] == reply.id)
        .collect();
    assert_eq!(
        replies.len(),
        1,
        "visible Everyone handoff reply has no matching SSE event"
    );
    assert_eq!(replies[0].conversation_id, "everyone");
    assert_eq!(replies[0].data["thread_id"], thread.id);
    assert_eq!(replies[0].data["task_id"], delegate.id);
    assert_eq!(replies[0].data["sender"], "Ben");
    assert_eq!(replies[0].data["text"], reply.text);
    fixture
        .store
        .as_ref()
        .unwrap()
        .save(&fixture.state.lock().unwrap())
        .unwrap();
    assert_eq!(
        fixture
            .events
            .since(0)
            .iter()
            .filter(|e| e.kind == "reply" && e.data["id"] == reply.id)
            .count(),
        1
    );
}

#[test]
fn durable_reply_events_match_visible_entries_for_default_named_and_everyone_turns() {
    if !python3_available() {
        return;
    }
    for named in [false, true] {
        for everyone in [false, true] {
            for threaded in [false, true] {
                for failed in [false, true] {
                    let label = format!("reply-event-{named}-{everyone}-{threaded}-{failed}");
                    let mut fixture = NamedTurnFixture::with_persistence(
                        &label,
                        vec![
                            "--fail-once".into(),
                            "--pre-tool-text".into(),
                            "Step commentary".into(),
                        ],
                        true,
                    );
                    if !named {
                        fixture.install_default_worker_with_args(vec![
                            "--generation-failure-once".into(),
                        ]);
                    }
                    let store = fixture.store.as_ref().unwrap();
                    let receiver = fixture.events.subscribe();
                    let next_reply = || loop {
                        let event = receiver
                            .recv_timeout(Duration::from_secs(5))
                            .expect("reply SSE event");
                        if event.kind == "reply" {
                            let disk = store.load().unwrap();
                            assert!(
                                disk.transcript.iter().any(|e| event.data["id"] == e.id),
                                "event arrived before its entry was durable"
                            );
                            break event;
                        }
                    };
                    let path = if everyone {
                        "/api/everyone"
                    } else {
                        "/api/turns"
                    };
                    let text = |message: &str| {
                        if everyone {
                            format!("@{} {message}", if named { "Ada" } else { "Marble" })
                        } else {
                            message.to_string()
                        }
                    };
                    let (status, body) = fixture.post(path, json!({"text":text("seed"), "marbleId":named.then_some(&fixture.marble_id)}));
                    assert_eq!(status, 202, "{body}");
                    let mut seed = next_reply();
                    if everyone {
                        let other = next_reply();
                        if other.conversation_id == "everyone" {
                            seed = other;
                        }
                        assert_eq!(seed.conversation_id, "everyone");
                    }
                    let thread = if threaded {
                        let (status, body) = fixture.post("/api/threads", json!({"conversation_id":seed.conversation_id, "parent_entry_id":seed.data["id"]}));
                        assert_eq!(status, 200, "{body}");
                        Some(body["thread"]["id"].as_str().unwrap().to_string())
                    } else {
                        None
                    };
                    let cursor = fixture.events.sequence();
                    let (status, body) = fixture.post(path, json!({"text":text(if failed { "FAIL reply" } else { "next reply" }), "marbleId":named.then_some(&fixture.marble_id), "conversationId":seed.conversation_id, "thread_id":thread}));
                    assert_eq!(status, 202, "{body}");
                    let count = if everyone && !threaded { 2 } else { 1 };
                    let mut ids = std::collections::HashSet::new();
                    for _ in 0..count {
                        let event = next_reply();
                        assert!(ids.insert(event.data["id"].as_str().unwrap().to_string()));
                        assert_eq!(
                            event.data["kind"],
                            if failed { "failure" } else { "assistant" }
                        );
                        assert_eq!(event.data["sender"], if named { "Ada" } else { "Marble" });
                        assert_eq!(event.data["task_id"], event.task_id);
                        assert_eq!(
                            event.data.get("thread_id").and_then(Value::as_str),
                            thread.as_deref()
                        );
                        let disk = store.load().unwrap();
                        let entry = disk
                            .transcript
                            .iter()
                            .find(|e| event.data["id"] == e.id)
                            .unwrap();
                        assert_eq!(event.data["text"], entry.text);
                        assert_eq!(event.conversation_id, entry.conversation_id);
                        let (status, body) = fixture.server.handle(
                            "GET",
                            &format!(
                                "/api/transcript?conversationId={}&around={}",
                                event.conversation_id, entry.id
                            ),
                            "",
                        );
                        assert_eq!(status, 200, "{body}");
                        assert!(
                            serde_json::from_str::<Value>(&body).unwrap()["entries"]
                                .as_array()
                                .unwrap()
                                .iter()
                                .any(|e| e["id"] == entry.id)
                        );
                        if let Some(thread) = &thread {
                            let (_, body) =
                                fixture
                                    .server
                                    .handle("GET", &format!("/api/threads/{thread}"), "");
                            assert!(
                                serde_json::from_str::<Value>(&body).unwrap()["entries"]
                                    .as_array()
                                    .unwrap()
                                    .iter()
                                    .any(|e| e["id"] == entry.id)
                            );
                        }
                    }
                    store.save(&fixture.state.lock().unwrap()).unwrap();
                    assert_eq!(
                        fixture
                            .events
                            .since(cursor)
                            .iter()
                            .filter(|e| e.kind == "reply")
                            .count(),
                        count
                    );
                }
            }
        }
    }
}

#[test]
fn named_steps_persist_commentary_as_reasoning_and_only_deliver_the_answer() {
    if !python3_available() {
        return;
    }
    let fixture = NamedTurnFixture::with_args(
        "step-commentary",
        vec![
            "--pre-tool-text".into(),
            "Research info.".into(),
            "--text-chunks".into(),
            r#"["The answer."]"#.into(),
        ],
    );
    let (status, _) = fixture.post(
        "/api/turns",
        json!({"text":"research", "marbleId":fixture.marble_id}),
    );
    assert_eq!(status, 202);
    let task = fixture.completed("research");
    let state = fixture.state.lock().unwrap();
    let entries: Vec<_> = state
        .transcript
        .iter()
        .filter(|e| e.task_id == task.id)
        .collect();
    assert!(
        entries
            .iter()
            .any(|e| e.kind == "reasoning" && e.text == "Research info.")
    );
    assert!(
        entries
            .iter()
            .any(|e| e.kind == "assistant" && e.text == "The answer.")
    );
}

#[test]
fn named_replies_strip_only_their_own_leading_mention() {
    if !python3_available() {
        return;
    }
    let fixture = NamedTurnFixture::with_args(
        "own-mention",
        vec![
            "--text-chunks".into(),
            r#"["@Ada I'll take the lead."]"#.into(),
        ],
    );
    let (status, _) = fixture.post(
        "/api/turns",
        json!({"text":"lead", "marbleId":fixture.marble_id}),
    );
    assert_eq!(status, 202);
    let task = fixture.completed("lead");
    assert!(
        fixture
            .state
            .lock()
            .unwrap()
            .transcript
            .iter()
            .any(|entry| entry.task_id == task.id
                && entry.kind == "assistant"
                && entry.text == "I'll take the lead.")
    );
}

impl Drop for NamedTurnFixture {
    fn drop(&mut self) {
        self.pool.shutdown();
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

#[test]
fn thread_routes_are_idempotent_searchable_and_paginated_with_stable_ids() {
    let fixture = NamedTurnFixture::new("thread-contract");
    let parent = {
        let mut state = fixture.state.lock().unwrap();
        state.conversation_mut("chat");
        for text in ["before", "😀 CAFÉ parent", "after"] {
            state.push_transcript(TranscriptEntry {
                conversation_id: "chat".into(),
                kind: "user".into(),
                text: text.into(),
                at: 1,
                ..Default::default()
            });
        }
        state.transcript[1].id.clone()
    };
    let (status, created) = fixture.post(
        "/api/threads",
        json!({"conversation_id":"chat", "parent_entry_id":parent}),
    );
    assert_eq!(status, 200);
    let id = created["thread"]["id"].as_str().unwrap();
    assert_eq!(created["thread"]["parent_sender"], "You");
    assert_eq!(
        fixture
            .post(
                "/api/threads",
                json!({"conversation_id":"chat", "parent_entry_id":parent})
            )
            .1,
        created
    );
    assert_eq!(
        fixture
            .post(
                "/api/threads",
                json!({"conversation_id":"wrong", "parent_entry_id":parent})
            )
            .0,
        404
    );
    assert_eq!(fixture.post("/api/threads", json!({})).0, 400);
    assert_eq!(fixture.server.handle("POST", "/api/threads", "{").0, 400);
    {
        let mut state = fixture.state.lock().unwrap();
        for kind in ["user", "tool", "reasoning", "assistant", "failure"] {
            state.push_transcript(TranscriptEntry {
                conversation_id: "chat".into(),
                thread_id: Some(id.into()),
                kind: kind.into(),
                text: "thread-only needle".into(),
                at: u64::MAX - 1,
                ..Default::default()
            });
        }
    }
    let get = |path: &str| {
        let (status, body) = fixture.server.handle("GET", path, "");
        assert_eq!(status, 200, "{body}");
        serde_json::from_str::<Value>(&body).unwrap()
    };
    let page = get(&format!("/api/threads/{id}"));
    assert_eq!(page["entries"][0]["id"], parent);
    assert_eq!(page["entries"].as_array().unwrap().len(), 6);
    assert_eq!(page["thread"]["reply_count"], 3);
    let main = get("/api/transcript?conversationId=chat&main=1");
    assert_eq!(main["entries"].as_array().unwrap().len(), 3);
    assert_eq!(main["entries"][1]["thread"]["reply_count"], 3);
    assert_eq!(
        get("/api/transcript?conversationId=chat")["entries"]
            .as_array()
            .unwrap()
            .len(),
        8
    );
    let around = get(&format!(
        "/api/transcript?conversationId=chat&main=1&around={parent}&limit=1&after=999"
    ));
    assert_eq!(around["entries"][0]["id"], parent);
    assert_eq!(
        fixture
            .server
            .handle(
                "GET",
                "/api/transcript?conversationId=chat&around=absent",
                ""
            )
            .0,
        404
    );
    assert_eq!(
        fixture.server.handle("GET", "/api/threads/absent", "").0,
        404
    );
    let hits = get("/api/search?q=needle");
    assert_eq!(hits["hits"].as_array().unwrap().len(), 3);
    assert_eq!(
        get(&format!("/api/search?q=needle&thread_id={id}&limit=1"))["hits"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert!(
        get("/api/search?q=needle&thread_id=other")["hits"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert!(get("/api/search?q=")["hits"].as_array().unwrap().is_empty());
    let unicode = get("/api/search?q=caf%C3%A9");
    assert_eq!(unicode["hits"][0]["match_ranges"], json!([[5, 5]]));
    assert_eq!(unicode["hits"][0]["entry_id"], parent);
    assert_eq!(
        get("/api/threads?conversation_id=chat")["threads"][0]["id"],
        id
    );
    let after_id = main["entries"][2]["id"].as_str().unwrap();
    assert_eq!(
        fixture
            .post(
                "/api/threads",
                json!({"conversation_id":"chat", "parent_entry_id":after_id})
            )
            .0,
        200
    );
    assert_eq!(
        get("/api/threads?conversation_id=chat")["threads"][0]["id"],
        id
    );
    assert!(
        fixture
            .events
            .since(0)
            .iter()
            .any(|e| e.kind == "thread" && e.data["thread"]["id"] == id)
    );
    let store = StateStore::in_data_dir(&fixture.root);
    store.save(&fixture.state.lock().unwrap()).unwrap();
    let restored = store.load().unwrap();
    assert_eq!(restored.threads[0].id, id);
    assert_eq!(restored.transcript[1].id, parent);

    let locked = server_with(
        ProductState::default(),
        ApprovalInbox::default(),
        EventBus::default(),
        AccessPolicy::new(Some("private-token".into())),
    );
    for path in [
        "/api/search?q=parent",
        "/api/threads?conversation_id=chat",
        "/api/threads/test",
    ] {
        assert_eq!(locked.handle("GET", path, "").0, 401);
    }
    assert_eq!(locked.handle("POST", "/api/threads", "{}").0, 401);
}

#[test]
fn clear_everyone_keeps_surviving_thread_replies_openable_in_private_chat() {
    let mut state = ProductState::default();
    state.conversation_mut("everyone");
    state.conversation_mut("private");
    state.push_transcript(TranscriptEntry {
        id: "parent".into(),
        conversation_id: "everyone".into(),
        kind: "assistant".into(),
        text: "parent".into(),
        ..Default::default()
    });
    let thread = state.create_thread("everyone", "parent").unwrap();
    for (conversation, id) in [("private", "private-reply"), ("everyone", "mirror-reply")] {
        state.push_transcript(TranscriptEntry {
            id: id.into(),
            conversation_id: conversation.into(),
            thread_id: Some(thread.id.clone()),
            task_id: "reply-task".into(),
            kind: "assistant".into(),
            text: "survivor".into(),
            ..Default::default()
        });
    }
    let server = free_server(state);
    assert_eq!(server.handle("POST", "/api/everyone/clear", "").0, 200);
    let get = |path: &str| {
        let (status, body) = server.handle("GET", path, "");
        assert_eq!(status, 200, "{body}");
        serde_json::from_str::<Value>(&body).unwrap()
    };
    let main = get("/api/transcript?conversationId=private&main=1");
    assert_eq!(main["entries"].as_array().unwrap().len(), 1);
    assert_eq!(main["entries"][0]["id"], "private-reply");
    let around = get("/api/transcript?conversationId=private&main=1&around=private-reply");
    assert_eq!(around["entries"][0]["id"], "private-reply");
    let search = get("/api/search?q=survivor");
    assert_eq!(search["hits"].as_array().unwrap().len(), 1);
    assert_eq!(search["hits"][0]["entry_id"], "private-reply");
    assert_eq!(search["hits"][0]["thread_id"], Value::Null);
    assert_eq!(
        server
            .handle("GET", &format!("/api/threads/{}", thread.id), "")
            .0,
        404
    );
    assert!(
        get("/api/everyone")["entries"]
            .as_array()
            .unwrap()
            .is_empty()
    );
}

#[test]
fn cancelling_queued_everyone_turns_does_not_claim_a_later_busy_message() {
    for threaded in [false, true] {
        let (turn_tx, inbox) = TurnInbox::channel();
        let mut state = ProductState::default();
        state
            .prism
            .add("Ada", "writer", "star", 40, "", "")
            .unwrap();
        let thread = if threaded {
            glass_marble::everyone::mirror(
                &mut state,
                "user",
                "You",
                "user",
                "thread parent",
                "",
                None,
            );
            let parent = state.transcript.last().unwrap().id.clone();
            Some(state.create_thread("everyone", &parent).unwrap().id)
        } else {
            None
        };
        let server = server_with_turn_channel(state, turn_tx);
        let post = |path: &str, body: Value| {
            let (status, body) = server.handle("POST", path, &body.to_string());
            assert!(status == 200 || status == 202, "{status}: {body}");
            serde_json::from_str::<Value>(&body).unwrap()
        };
        post(
            "/api/everyone",
            json!({"text":"@Marble first queued message", "thread_id":thread}),
        );
        let second = post(
            "/api/everyone",
            json!({"text":"@Marble second queued message", "thread_id":thread}),
        );
        let busy = post(
            "/api/everyone",
            json!({"text":"@Ada unrelated later message", "thread_id":thread}),
        );
        assert_eq!(busy["busy"], json!(["Ada"]));
        let (_, body) = server.handle("GET", "/api/state", "");
        let queued: ProductState = serde_json::from_str(&body).unwrap();
        let later_id = queued
            .transcript
            .iter()
            .find(|e| e.text == "@Ada unrelated later message")
            .unwrap()
            .id
            .clone();
        let first = queued
            .tasks
            .iter()
            .find(|t| t.prompt == "@Marble first queued message")
            .unwrap();
        let first_id = first.id.as_str();
        let shared_chat = &first.conversation_id;
        let second_id = queued
            .tasks
            .iter()
            .find(|t| t.prompt == "@Marble second queued message")
            .unwrap()
            .id
            .clone();
        assert_eq!(second["answering"], json!(["Marble"]));
        for (id, prompt) in [
            (first_id, "@Marble first queued message"),
            (second_id.as_str(), "@Marble second queued message"),
        ] {
            post(&format!("/api/tasks/{id}/cancel"), json!({}));
            if id == second_id {
                assert_eq!(
                    server
                        .handle(
                            "POST",
                            &format!("/api/conversations/{shared_chat}/delete"),
                            "{}",
                        )
                        .0,
                    409
                );
            }
            let (_, body) = server.handle("GET", "/api/state", "");
            let mut state: ProductState = serde_json::from_str(&body).unwrap();
            if id == second_id {
                state.delete_conversation(shared_chat).unwrap();
            }
            let later = state
                .transcript
                .iter()
                .find(|e| e.id == later_id)
                .expect("deleting the cancelled chat removed an unrelated busy message");
            assert_ne!(later.task_id, id);
            if id == second_id {
                assert!(
                    !state
                        .transcript
                        .iter()
                        .any(|e| e.kind == "user" && e.text == prompt)
                );
            }
        }
        let (_, body) = server.handle("GET", "/api/state", "");
        let mut final_state: ProductState = serde_json::from_str(&body).unwrap();
        final_state.delete_conversation(shared_chat).unwrap();
        assert!(
            !final_state
                .transcript
                .iter()
                .any(|e| e.text == "@Marble first queued message")
        );
        assert!(inbox.try_recv().is_none());
    }
}

#[test]
fn cancelling_a_queued_everyone_lead_releases_buffered_replies() {
    let mut state = ProductState::default();
    state.conversation_mut("chat");
    state.upsert_task(TaskRecord {
        id: "queued-lead".into(),
        conversation_id: "chat".into(),
        status: "queued".into(),
        owner: "agent".into(),
        prompt: "lead".into(),
        ..Default::default()
    });
    state.everyone_orders.insert(
        "message".into(),
        ["queued-lead".into(), "task:next".into(), "task:last".into()].into(),
    );
    for (key, speaker) in [
        ("queued-lead", "marble"),
        ("task:next", "ada"),
        ("task:last", "ben"),
    ] {
        state.everyone_pending.insert(key.into(), speaker.into());
    }
    glass_marble::everyone::mirror_settlement(&mut state, "last", "Last answer", "completed");
    glass_marble::everyone::mirror_settlement(&mut state, "next", "Next answer", "completed");
    assert!(state.transcript.is_empty());
    let server = free_server(state);
    assert_eq!(
        server.handle("POST", "/api/tasks/queued-lead/cancel", "").0,
        200
    );
    let (status, body) = server.handle("GET", "/api/everyone", "");
    assert_eq!(status, 200);
    let timeline: Value = serde_json::from_str(&body).unwrap();
    let entries = timeline["entries"].as_array().unwrap();
    assert_eq!(entries.len(), 3, "{body}");
    assert_eq!(entries[0]["kind"], "system");
    assert_eq!(entries[1]["text"], "Next answer");
    assert_eq!(entries[2]["text"], "Last answer");
    assert_eq!(
        server.handle("POST", "/api/tasks/queued-lead/cancel", "").0,
        409
    );
}

#[test]
fn a_created_chat_survives_reload_and_accepts_its_first_message() {
    if !python3_available() {
        return;
    }
    let mut fixture = NamedTurnFixture::with_persistence("create-empty-chat", Vec::new(), true);
    fixture.install_default_worker_with_args(vec!["--strict-conversations".into()]);
    let (status, reply) = fixture.post("/api/conversations", json!({}));
    assert_eq!(status, 201, "{reply}");
    let id = reply["conversation"]["id"].as_str().unwrap();
    assert_eq!(reply["conversation"]["taskIds"], json!([]));
    let store = fixture.store.as_ref().unwrap();
    let loaded = store.load().unwrap();
    assert_eq!(loaded.conversations[0].id, id);
    assert!(loaded.tasks.is_empty());
    assert!(loaded.transcript_for(id).is_empty());
    *fixture.state.lock().unwrap() = loaded;

    let prompt = "First message in the new chat";
    let (status, reply) = fixture.post("/api/turns", json!({"conversationId": id, "text": prompt}));
    assert_eq!(status, 202, "{reply}");
    let task = fixture.settled(prompt);
    assert_eq!(task.status, "completed");
    assert_eq!(task.conversation_id, id);
    let state = fixture.state.lock().unwrap();
    let chat = state
        .conversations
        .iter()
        .find(|chat| chat.id == id)
        .unwrap();
    assert!(!chat.needs_engine_session);
    assert!(!chat.engine_conversation_id.is_empty());
    assert_eq!(chat.title, prompt);
    assert!(
        state
            .transcript_for(id)
            .iter()
            .any(|entry| entry.kind == "assistant")
    );
}

#[test]
fn a_paused_created_chat_survives_pruning_and_runs_on_resume() {
    if !python3_available() {
        return;
    }
    let mut fixture = NamedTurnFixture::with_persistence("paused-created-chat", Vec::new(), true);
    fixture.install_default_worker_with_args(vec!["--strict-conversations".into()]);
    let (status, reply) = fixture.post("/api/conversations", json!({}));
    assert_eq!(status, 201, "{reply}");
    let id = reply["conversation"]["id"].as_str().unwrap().to_string();
    fixture
        .state
        .lock()
        .unwrap()
        .conversation_mut(&id)
        .updated_at = 0;
    assert_eq!(fixture.post("/api/control/pause", json!({})).0, 200);
    let prompt = "First message held while paused";
    let (status, reply) = fixture.post("/api/turns", json!({"conversationId":id,"text":prompt}));
    assert_eq!(status, 202, "{reply}");
    assert_eq!(reply["queued"], true);
    for _ in 0..glass_marble::state::UNSENT_CONVERSATION_LIMIT + 1 {
        assert_eq!(fixture.post("/api/conversations", json!({})).0, 201);
    }
    let mut loaded = fixture.store.as_ref().unwrap().load().unwrap();
    assert!(loaded.tasks.is_empty());
    assert_eq!(
        loaded.control.queue[0].conversation_id.as_deref(),
        Some(id.as_str())
    );
    loaded.prune(glass_marble::worker::now_millis());
    assert!(
        loaded
            .conversations
            .iter()
            .any(|record| record.id == id && record.needs_engine_session)
    );
    *fixture.state.lock().unwrap() = loaded;
    assert_eq!(fixture.post("/api/control/resume", json!({})).0, 200);
    let task = fixture.settled(prompt);
    assert_eq!(task.status, "completed");
    assert_eq!(task.conversation_id, id);
    let state = fixture.state.lock().unwrap();
    let record = state
        .conversations
        .iter()
        .find(|record| record.id == id)
        .unwrap();
    assert!(!record.needs_engine_session);
    assert!(!record.engine_conversation_id.is_empty());
    assert!(
        state
            .transcript_for(&id)
            .iter()
            .any(|entry| entry.kind == "assistant")
    );
    assert!(state.control.queue.is_empty());
}

#[test]
fn everyone_default_recovers_after_first_admission_fails() {
    if !python3_available() {
        return;
    }
    let mut fixture = NamedTurnFixture::new("everyone-first-admission-failure");
    fixture.install_default_worker_with_args(vec![
        "--reject-create-once".into(),
        "--strict-conversations".into(),
    ]);
    let _ = fixture.post("/api/everyone", json!({"text":"@Marble first attempt"}));
    let failed = fixture.settled("@Marble first attempt");
    assert_eq!(failed.status, "failed");
    assert!(failed.conversation_id.starts_with("everyone-session-"));
    let store = StateStore::in_data_dir(&fixture.root);
    store.save(&fixture.state.lock().unwrap()).unwrap();
    *fixture.state.lock().unwrap() = store.load().unwrap();
    for prompt in ["@Marble second attempt", "@Marble third attempt"] {
        let (status, body) = fixture.post("/api/everyone", json!({"text":prompt}));
        assert_eq!(status, 202, "{body}");
        let task = fixture.settled(prompt);
        assert_eq!(task.status, "completed");
        assert_eq!(task.conversation_id, failed.conversation_id);
    }
    assert!(
        fixture
            .state
            .lock()
            .unwrap()
            .tasks
            .iter()
            .any(|t| t.id == failed.id && t.status == "failed")
    );
}

#[test]
#[ignore = "manual 2000-entry payload and handler timing measurement"]
fn history_paging_measurement() {
    let mut state = ProductState::default();
    state.conversation_mut("everyone");
    for i in 0..2000 {
        state.push_transcript(TranscriptEntry {
            conversation_id: "everyone".into(),
            kind: if i % 4 == 0 { "user" } else { "assistant" }.into(),
            text: format!(
                "entry {i}: {}",
                "a representative multi-agent conversation reply with useful details. ".repeat(8)
            ),
            speaker: ["user", "marble", "ada", "dietrich"][i % 4].into(),
            at: i as u64 + 1,
            ..Default::default()
        });
    }
    let parent = state.transcript[0].id.clone();
    let thread = state.create_thread("everyone", &parent).unwrap();
    for entry in &mut state.transcript[1..] {
        entry.thread_id = Some(thread.id.clone());
    }
    let server = free_server(state);
    for path in [
        "/api/everyone".to_string(),
        "/api/transcript?conversationId=everyone".into(),
        format!("/api/threads/{}", thread.id),
    ] {
        for query in ["", "limit=50"] {
            let url = if query.is_empty() {
                path.clone()
            } else {
                format!(
                    "{path}{}{query}",
                    if path.contains('?') { "&" } else { "?" }
                )
            };
            let mut bytes = 0;
            let mut count = 0;
            let start = Instant::now();
            for _ in 0..100 {
                let (status, body) = server.handle("GET", &url, "");
                assert_eq!(status, 200, "{body}");
                bytes = body.len();
            }
            let mean_us = start.elapsed().as_micros() / 100;
            let (_, body) = server.handle("GET", &url, "");
            let value: Value = serde_json::from_str(&body).unwrap();
            count += value["entries"].as_array().unwrap().len();
            println!("PAGING {url}: entries={count} bytes={bytes} mean_handler_us={mean_us}");
        }
    }
}

fn paged_history_state() -> ProductState {
    let mut state = ProductState::default();
    for conversation in ["everyone", "chat"] {
        state.conversation_mut(conversation);
        for i in 0..513 {
            state.push_transcript(TranscriptEntry {
                id: format!("entry-{conversation}-{i:04}"),
                conversation_id: conversation.into(),
                kind: if i % 4 == 0 { "user" } else { "assistant" }.into(),
                speaker: ["user", "marble", "ada", "dietrich"][i % 4].into(),
                text: format!("message {i}"),
                at: 100,
                ..Default::default()
            });
        }
        let thread = state
            .create_thread(conversation, &format!("entry-{conversation}-0000"))
            .unwrap();
        for entry in &mut state.transcript {
            if entry.conversation_id == conversation && entry.id != thread.parent_entry_id {
                entry.thread_id = Some(thread.id.clone());
            }
        }
    }
    state.conversation_mut("empty");
    state
}

#[test]
fn history_paging_preserves_defaults_and_visits_equal_timestamp_entries_once() {
    let state = paged_history_state();
    let mut paths = vec![
        ("/api/everyone".into(), "everyone", 513),
        (
            "/api/transcript?conversationId=everyone".into(),
            "everyone",
            500,
        ),
        ("/api/transcript?conversationId=chat".into(), "chat", 500),
    ];
    for thread in &state.threads {
        paths.push((
            format!("/api/threads/{}", thread.id),
            if thread.conversation_id == "everyone" {
                "everyone"
            } else {
                "chat"
            },
            513,
        ));
    }
    let state: ProductState =
        serde_json::from_str(&serde_json::to_string(&state).unwrap()).unwrap();
    let server = free_server(state);
    for (path, conversation, default_count) in paths {
        let (status, body) = server.handle("GET", &path, "");
        assert_eq!(status, 200, "{body}");
        let body: Value = serde_json::from_str(&body).unwrap();
        assert_eq!(body["entries"].as_array().unwrap().len(), default_count);
        assert_eq!(body["hasMore"], default_count < 513);
        let separator = if path.contains('?') { "&" } else { "?" };
        let mut before = String::new();
        let mut all = Vec::new();
        loop {
            let url = format!("{path}{separator}limit=7{before}");
            let (status, body) = server.handle("GET", &url, "");
            assert_eq!(status, 200, "{body}");
            let body: Value = serde_json::from_str(&body).unwrap();
            let entries = body["entries"].as_array().unwrap();
            assert!(!entries.is_empty());
            assert!(entries.len() <= 7);
            let page: Vec<_> = entries
                .iter()
                .map(|e| e["id"].as_str().unwrap().to_string())
                .collect();
            assert!(page.windows(2).all(|pair| pair[0] < pair[1]));
            all.splice(0..0, page);
            if body["hasMore"] == false {
                assert!(body["nextBefore"].is_null());
                break;
            }
            assert_eq!(body["nextBefore"], entries[0]["id"]);
            before = format!("&before={}", body["nextBefore"].as_str().unwrap());
            assert!(all.len() < 513);
        }
        assert_eq!(
            all,
            (0..513)
                .map(|i| format!("entry-{conversation}-{i:04}"))
                .collect::<Vec<_>>()
        );
    }
    let (_, body) = server.handle(
        "GET",
        "/api/transcript?conversationId=chat&around=entry-chat-0500&limit=7",
        "",
    );
    let body: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(body["entries"][0]["id"], "entry-chat-0497");
    assert_eq!(body["entries"][6]["id"], "entry-chat-0503");
    assert_eq!(body["omitted"], 0);
    for path in [
        "/api/everyone?main=1&limit=5",
        "/api/transcript?conversationId=chat&main=1&limit=5",
    ] {
        let (_, body) = server.handle("GET", path, "");
        let body: Value = serde_json::from_str(&body).unwrap();
        assert_eq!(body["entries"].as_array().unwrap().len(), 1);
        assert_eq!(body["hasMore"], false);
    }
    let (_, body) = server.handle("GET", "/api/everyone?after=100&limit=5", "");
    assert_eq!(
        serde_json::from_str::<Value>(&body).unwrap()["entries"],
        json!([])
    );
    let (_, body) = server.handle(
        "GET",
        "/api/transcript?conversationId=chat&after=100&limit=5",
        "",
    );
    assert_eq!(
        serde_json::from_str::<Value>(&body).unwrap()["entries"]
            .as_array()
            .unwrap()
            .len(),
        5
    );
}

#[test]
fn history_paging_empty_boundaries_and_invalid_parameters_are_scoped() {
    let state = paged_history_state();
    let thread = state
        .threads
        .iter()
        .find(|t| t.conversation_id == "chat")
        .unwrap()
        .id
        .clone();
    let server = free_server(state);
    for (path, conversation) in [
        ("/api/everyone".into(), "everyone"),
        ("/api/transcript?conversationId=chat".into(), "chat"),
        (format!("/api/threads/{thread}"), "chat"),
    ] {
        let separator = if path.contains('?') { "&" } else { "?" };
        for (query, expected, error) in [
            ("limit=nope", 400, "invalid_limit"),
            ("limit=-1", 400, "invalid_limit"),
            ("limit=184467440737095516160", 400, "invalid_limit"),
            ("before=", 400, "invalid_cursor"),
            ("before=%0A", 400, "invalid_cursor"),
            ("before=missing", 404, "unknown_cursor"),
            ("before=missing&after=0", 400, "conflicting_cursors"),
            ("before=missing&around=missing", 400, "conflicting_cursors"),
        ] {
            let (status, body) = server.handle("GET", &format!("{path}{separator}{query}"), "");
            assert_eq!(status, expected, "{path}: {body}");
            assert_eq!(
                serde_json::from_str::<Value>(&body).unwrap()["error"],
                error
            );
        }
        for cursor in [
            "x".repeat(257),
            format!(
                "entry-{}-0000",
                if conversation == "chat" {
                    "everyone"
                } else {
                    "chat"
                }
            ),
        ] {
            assert!(matches!(
                server
                    .handle("GET", &format!("{path}{separator}before={cursor}"), "")
                    .0,
                400 | 404
            ));
        }
        let (status, body) = server.handle(
            "GET",
            &format!("{path}{separator}before=entry-{conversation}-0000&limit=5"),
            "",
        );
        assert_eq!(status, 200);
        let body: Value = serde_json::from_str(&body).unwrap();
        assert_eq!(body["entries"], json!([]));
        assert_eq!(body["hasMore"], false);
        assert!(body["nextBefore"].is_null());
        for (limit, count) in [(0, 1), (1001, 513)] {
            let (_, body) = server.handle("GET", &format!("{path}{separator}limit={limit}"), "");
            assert_eq!(
                serde_json::from_str::<Value>(&body).unwrap()["entries"]
                    .as_array()
                    .unwrap()
                    .len(),
                count
            );
        }
    }
    for path in [
        "/api/transcript?conversationId=empty&limit=5",
        "/api/everyone?main=1&before=entry-everyone-0000",
    ] {
        let (_, body) = server.handle("GET", path, "");
        assert_eq!(
            serde_json::from_str::<Value>(&body).unwrap()["entries"],
            json!([])
        );
    }
    assert_eq!(
        server
            .handle(
                "GET",
                "/api/transcript?conversationId=chat&main=1&before=entry-chat-0001",
                ""
            )
            .0,
        404
    );
}

#[test]
fn history_paging_cursor_survives_new_entries_settlement_and_reload() {
    let fixture = NamedTurnFixture::new("paging-concurrent-updates");
    {
        let mut state = fixture.state.lock().unwrap();
        *state = paged_history_state();
        let anchor = state
            .transcript
            .iter_mut()
            .find(|e| e.id == "entry-chat-0510")
            .unwrap();
        anchor.kind = "pending".into();
        anchor.task_id = "anchor-task".into();
    }
    let (status, body) =
        fixture
            .server
            .handle("GET", "/api/transcript?conversationId=chat&limit=3", "");
    assert_eq!(status, 200);
    let body: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(body["nextBefore"], "entry-chat-0510");
    let reloaded = {
        let mut state = fixture.state.lock().unwrap();
        state.push_transcript(TranscriptEntry {
            conversation_id: "chat".into(),
            task_id: "anchor-task".into(),
            kind: "assistant".into(),
            text: "settled answer".into(),
            at: 200,
            ..Default::default()
        });
        state.push_transcript(TranscriptEntry {
            conversation_id: "chat".into(),
            kind: "user".into(),
            text: "newer message".into(),
            at: 201,
            ..Default::default()
        });
        serde_json::from_str::<ProductState>(&serde_json::to_string(&*state).unwrap()).unwrap()
    };
    let server = free_server(reloaded);
    let (status, body) = server.handle(
        "GET",
        "/api/transcript?conversationId=chat&limit=3&before=entry-chat-0510",
        "",
    );
    assert_eq!(status, 200, "{body}");
    let body: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(
        body["entries"]
            .as_array()
            .unwrap()
            .iter()
            .map(|e| e["id"].as_str().unwrap())
            .collect::<Vec<_>>(),
        ["entry-chat-0507", "entry-chat-0508", "entry-chat-0509"]
    );
}

#[test]
fn everyone_reserves_reply_order_even_when_the_last_agent_finishes_first() {
    if !python3_available() {
        return;
    }
    let mut fixture = NamedTurnFixture::with_persistence(
        "group-out-of-order",
        vec!["--reasoning-channels".into()],
        true,
    );
    fixture.install_default_worker_with_args(vec!["--delay-ms".into(), "1800".into()]);
    {
        let mut state = fixture.state.lock().unwrap();
        glass_marble::everyone::mirror(
            &mut state,
            "other",
            "Other",
            "assistant",
            "FOREIGN_IN_FLIGHT_OR_EARLIER_REPLY",
            "",
            None,
        );
    }
    let (code, body) = fixture.post("/api/everyone", json!({"text":"@everyone next question"}));
    assert_eq!(code, 202, "{body}");
    let deadline = Instant::now() + Duration::from_secs(5);
    let ids = loop {
        let state = fixture.state.lock().unwrap();
        if state
            .tasks
            .iter()
            .any(|t| t.marble_id == fixture.marble_id && t.status == "completed")
        {
            assert!(
                state
                    .tasks
                    .iter()
                    .any(|t| t.owner == "agent" && t.status == "running")
            );
            let rows: Vec<_> = state
                .transcript
                .iter()
                .filter(|e| {
                    e.conversation_id == "everyone"
                        && e.speaker != "other"
                        && matches!(e.kind.as_str(), "pending" | "assistant" | "failure")
                })
                .collect();
            assert_eq!(rows.len(), 2);
            assert_eq!(rows[0].kind, "pending");
            assert_eq!(rows[0].speaker, "marble");
            assert_eq!(rows[1].kind, "assistant");
            assert_eq!(rows[1].speaker, fixture.marble_id);
            break rows.iter().map(|e| e.id.clone()).collect::<Vec<_>>();
        }
        assert!(Instant::now() < deadline);
        drop(state);
        std::thread::sleep(Duration::from_millis(10));
    };
    let deadline = Instant::now() + Duration::from_secs(5);
    while fixture
        .events
        .since(0)
        .iter()
        .filter(|e| {
            e.kind == "reply"
                && e.conversation_id == "everyone"
                && ids.iter().any(|id| e.data["id"] == *id)
        })
        .count()
        < 2
    {
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(10));
    }
    let state = fixture.store.as_ref().unwrap().load().unwrap();
    assert!(
        state.tasks.iter().all(|t| t.status == "completed"),
        "{:?}",
        state.tasks
    );
    let replies: Vec<_> = state
        .transcript
        .iter()
        .filter(|e| {
            e.conversation_id == "everyone" && e.speaker != "other" && e.kind == "assistant"
        })
        .map(|e| e.id.clone())
        .collect();
    assert_eq!(replies, ids);
    for sessions in [
        fixture.root.join("default-sessions"),
        fixture.session_root(
            &format!("everyone-session-lobby__{}", fixture.marble_id),
            &fixture.marble_id,
        ),
    ] {
        let input: Value =
            serde_json::from_slice(&std::fs::read(sessions.join("fixture-input.json")).unwrap())
                .unwrap();
        let text = input["input"]["text"].as_str().unwrap();
        assert!(!text.contains("FOREIGN_IN_FLIGHT_OR_EARLIER_REPLY"));
        assert!(!text.contains("Public answer."));
    }
}

#[test]
fn everyone_retries_transient_generation_once_with_stable_entries_and_tasks() {
    if !python3_available() {
        return;
    }
    for (label, args, attempts, status) in [
        (
            "once",
            vec!["--fail-once", "--error-once", "--strict-conversations"],
            2,
            "completed",
        ),
        ("always", vec!["--transient-error-always"], 2, "failed"),
        ("permanent", vec!["--permanent-error"], 1, "failed"),
        ("empty", vec!["--empty-reply"], 1, "failed"),
    ] {
        let args: Vec<String> = args.into_iter().map(str::to_string).collect();
        let mut fixture =
            NamedTurnFixture::with_persistence(&format!("group-retry-{label}"), args.clone(), true);
        fixture.install_default_worker_with_args(args);
        let prompt = format!("@everyone FAIL {label}");
        let request = json!({"text":prompt,"client_message_id":format!("group-{label}")});
        let (code, response) = fixture.post("/api/everyone", request.clone());
        assert_eq!(code, 202, "{response}");
        let deadline = Instant::now() + Duration::from_secs(8);
        loop {
            let state = fixture.state.lock().unwrap();
            let tasks: Vec<_> = state.tasks.iter().filter(|t| t.prompt == prompt).collect();
            if tasks.len() == 2 && tasks.iter().all(|t| t.status == status) {
                break;
            }
            assert!(Instant::now() < deadline, "{label}: {tasks:?}");
            drop(state);
            std::thread::sleep(Duration::from_millis(10));
        }
        assert_eq!(fixture.post("/api/everyone", request).1, response);
        let deadline = Instant::now() + Duration::from_secs(3);
        while fixture
            .events
            .since(0)
            .iter()
            .filter(|e| e.kind == "reply" && e.conversation_id == "everyone")
            .count()
            < 2
        {
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(10));
        }
        let state = fixture.store.as_ref().unwrap().load().unwrap();
        let expected_kind = if status == "completed" {
            "assistant"
        } else {
            "failure"
        };
        let replies: Vec<_> = state
            .transcript
            .iter()
            .filter(|e| e.conversation_id == "everyone" && e.kind == expected_kind)
            .collect();
        assert_eq!(replies.len(), 2, "{label}: {:?}", state.transcript);
        assert_eq!(replies[0].speaker, "marble");
        assert_eq!(replies[1].speaker, fixture.marble_id);
        assert!(!state.transcript.iter().any(|e| e.kind == "pending"));
        assert!(state.everyone_pending.is_empty());
        for reply in replies {
            let created = fixture
                .events
                .since(0)
                .into_iter()
                .find(|e| {
                    e.kind == "entry" && e.data["kind"] == "pending" && e.data["id"] == reply.id
                })
                .expect("reply was not reserved before dispatch");
            assert_eq!(created.data["at"], reply.at);
            let accepted = fixture
                .events
                .since(0)
                .into_iter()
                .find(|e| {
                    e.kind == "entry"
                        && e.data["id"] == reply.id
                        && e.data["taskId"] == reply.task_id
                })
                .expect("accepted task changed identity");
            assert_eq!(accepted.data["id"], reply.id);
            assert_eq!(
                state
                    .transcript
                    .iter()
                    .filter(|e| e.kind == "user"
                        && e.task_id == reply.task_id
                        && e.conversation_id != "everyone")
                    .count(),
                1
            );
            if status == "failed" {
                assert!(reply.failure.is_some());
            }
        }
        for sessions in [
            fixture.root.join("default-sessions"),
            fixture.session_root(
                &format!("everyone-session-lobby__{}", fixture.marble_id),
                &fixture.marble_id,
            ),
        ] {
            let submissions: Value = serde_json::from_slice(
                &std::fs::read(sessions.join("fixture-submissions.json")).unwrap(),
            )
            .unwrap();
            assert_eq!(submissions.as_object().unwrap().len(), attempts, "{label}");
        }
        let events = fixture.events.since(0);
        assert_eq!(
            events
                .iter()
                .filter(|e| e.kind == "reply" && e.conversation_id == "everyone")
                .count(),
            2
        );
        assert!(
            !serde_json::to_string(&events)
                .unwrap()
                .contains("secret-token")
        );
    }
}

#[test]
fn everyone_cold_default_retries_transient_admission_once_without_duplicate_sends() {
    if !python3_available() {
        return;
    }
    let mut fixture =
        NamedTurnFixture::with_persistence("cold-default-transient", Vec::new(), true);
    fixture.install_default_worker_with_args(vec![
        "--reject-create-once".into(),
        "--transient-create-error".into(),
        "--strict-conversations".into(),
    ]);
    let request = json!({"text":"@everyone cold start","client_message_id":"cold-first"});
    let (status, body) = fixture.post("/api/everyone", request.clone());
    assert_eq!(status, 202, "{body}");
    assert_eq!(
        body["busy"],
        json!([]),
        "default first admission failed: {body}"
    );
    assert_eq!(body["answering"].as_array().unwrap().len(), 2);
    let deadline = Instant::now() + Duration::from_secs(5);
    while fixture
        .events
        .since(0)
        .iter()
        .filter(|e| e.kind == "reply" && e.conversation_id == "everyone")
        .count()
        < 2
    {
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(fixture.post("/api/everyone", request).1, body);
    let state = fixture.store.as_ref().unwrap().load().unwrap();
    assert_eq!(
        state
            .tasks
            .iter()
            .filter(|t| t.prompt == "@everyone cold start")
            .count(),
        2
    );
    assert!(state.tasks.iter().all(|t| t.status == "completed"));
    assert_eq!(
        state
            .transcript
            .iter()
            .filter(|e| e.conversation_id == "everyone" && e.kind == "user")
            .count(),
        1
    );
}

#[test]
fn named_core_admission_failures_settle_private_and_everyone_entries_with_retry() {
    if !python3_available() {
        return;
    }
    for flag in ["--reject-create-once", "--reject-submit-once"] {
        for everyone in [false, true] {
            let fixture = NamedTurnFixture::with_persistence(
                &format!("named-admission-{flag}-{everyone}"),
                vec![flag.into()],
                true,
            );
            let path = if everyone {
                "/api/everyone"
            } else {
                "/api/turns"
            };
            let prompt = if everyone {
                "@Ada first admission"
            } else {
                "first admission"
            };
            let (status, response) = fixture.post(path, json!({"text":prompt, "marbleId":fixture.marble_id, "client_message_id":"admission-id"}));
            assert_eq!(status, 202, "{response}");
            let failed = fixture.settled(prompt);
            assert_eq!(failed.status, "failed");
            // The in-memory task can settle before the separate durable
            // write that includes the failure row and its Everyone mirror.
            let store = fixture.store.as_ref().unwrap();
            let deadline = Instant::now() + Duration::from_secs(5);
            let state = loop {
                let state = store.load().unwrap();
                if state.transcript.iter().any(|entry| {
                    entry.task_id == failed.id
                        && entry.kind == "failure"
                        && (!everyone || entry.conversation_id == "everyone")
                }) {
                    break state;
                }
                assert!(Instant::now() < deadline, "failure row was not persisted");
                std::thread::sleep(Duration::from_millis(10));
            };
            assert!(state.control.queue.is_empty());
            assert!(state.everyone_pending.is_empty());
            assert!(!state.transcript.iter().any(|e| e.kind == "pending"));
            let failure = state
                .transcript
                .iter()
                .find(|e| {
                    e.task_id == failed.id
                        && e.kind == "failure"
                        && (!everyone || e.conversation_id == "everyone")
                })
                .unwrap();
            assert!(failure.failure.as_ref().unwrap().retryable);
            let user = state
                .transcript
                .iter()
                .find(|e| e.id == response["entry_id"].as_str().unwrap())
                .unwrap();
            assert_eq!(user.client_message_id.as_deref(), Some("admission-id"));
            assert_eq!(
                fixture
                    .post("/api/turns/retry", json!({"taskId":failed.id}))
                    .0,
                202
            );
            fixture.completed(prompt);
        }
    }
}

#[test]
fn rejected_named_retry_keeps_the_original_pre_input_checkpoint() {
    if !python3_available() {
        return;
    }
    let fixture = NamedTurnFixture::with_persistence(
        "retry-admission-checkpoint",
        vec![
            "--fail-once".into(),
            "--generation-failure-once".into(),
            "--reject-retry-submit-once".into(),
        ],
        true,
    );
    let prompt = "FAIL once then reject retry";
    assert_eq!(
        fixture
            .post(
                "/api/turns",
                json!({"text":prompt,"marbleId":fixture.marble_id})
            )
            .0,
        202
    );
    let original = fixture.settled(prompt);
    assert_eq!(original.status, "failed");
    assert_eq!(
        fixture
            .post("/api/turns/retry", json!({"taskId":original.id}))
            .0,
        202
    );
    let deadline = Instant::now() + Duration::from_secs(5);
    let rejected = loop {
        let task = fixture.settled(prompt);
        if task.id != original.id {
            break task;
        }
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(10));
    };
    assert_eq!(rejected.status, "failed");
    {
        let state = fixture.state.lock().unwrap();
        let record = state
            .conversations
            .iter()
            .find(|c| c.id == original.conversation_id)
            .unwrap();
        assert_eq!(
            record.checkpoints.get(&rejected.id),
            record.checkpoints.get(&original.id)
        );
    }
    assert_eq!(
        fixture
            .post("/api/turns/retry", json!({"taskId":rejected.id}))
            .0,
        202
    );
    let completed = fixture.completed(prompt);
    let engine = fixture
        .state
        .lock()
        .unwrap()
        .conversations
        .iter()
        .find(|c| c.id == completed.conversation_id)
        .unwrap()
        .engine_conversation_id
        .clone();
    let history: Value = serde_json::from_slice(
        &std::fs::read(
            fixture
                .session_root(&completed.conversation_id, &fixture.marble_id)
                .join("fixture-history.json"),
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(
        history["conversations"][&engine].as_array().unwrap().len(),
        1,
        "{history}"
    );
}

#[test]
fn everyone_thread_search_returns_only_the_jumpable_reply() {
    if !python3_available() {
        return;
    }
    let fixture = NamedTurnFixture::new("thread-search-visibility");
    assert_eq!(
        fixture.post("/api/everyone", json!({"text":"@Ada root"})).0,
        202
    );
    let root = fixture.completed("@Ada root");
    let parent = fixture
        .state
        .lock()
        .unwrap()
        .transcript
        .iter()
        .find(|e| e.conversation_id == "everyone" && e.task_id == root.id && e.kind == "assistant")
        .unwrap()
        .id
        .clone();
    let thread = fixture
        .post(
            "/api/threads",
            json!({"conversation_id":"everyone", "parent_entry_id":parent}),
        )
        .1["thread"]["id"]
        .as_str()
        .unwrap()
        .to_owned();
    assert_eq!(
        fixture
            .post(
                "/api/everyone",
                json!({"text":"@Ada jumpneedle", "thread_id":thread})
            )
            .0,
        202
    );
    let reply = fixture.completed("@Ada jumpneedle");
    let visible = {
        let state = fixture.state.lock().unwrap();
        let copies: Vec<_> = state
            .transcript
            .iter()
            .filter(|e| e.task_id == reply.id && e.kind == "assistant")
            .collect();
        assert_eq!(
            copies.len(),
            2,
            "fixture must exercise both mirrored copies"
        );
        copies
            .iter()
            .find(|e| e.conversation_id == "everyone")
            .unwrap()
            .id
            .clone()
    };
    let get = |path: &str| {
        let (status, body) = fixture.server.handle("GET", path, "");
        assert_eq!(status, 200, "{body}");
        serde_json::from_str::<Value>(&body).unwrap()
    };
    let page = get(&format!("/api/threads/{thread}"));
    for scope in [String::new(), format!("&thread_id={thread}")] {
        let hits = get(&format!(
            "/api/search?q=handled%20%40Ada%20jumpneedle{scope}"
        ));
        let hits = hits["hits"].as_array().unwrap();
        assert_eq!(hits.len(), 1, "{hits:?}");
        assert_eq!(hits[0]["entry_id"], visible);
        assert_eq!(hits[0]["conversation_id"], "everyone");
        assert_eq!(hits[0]["thread_id"], thread);
        assert!(
            page["entries"]
                .as_array()
                .unwrap()
                .iter()
                .any(|e| e["id"] == hits[0]["entry_id"])
        );
        let around = get(&format!(
            "/api/transcript?conversationId=everyone&around={visible}&limit=1"
        ));
        assert_eq!(around["entries"][0]["id"], visible);
    }
    let users = get(&format!("/api/search?q=jumpneedle&thread_id={thread}"));
    assert_eq!(
        users["hits"].as_array().unwrap().len(),
        2,
        "one visible user and one assistant reply"
    );
}

#[test]
fn thread_turns_route_and_propagate_to_every_entry_and_retry_after_reload() {
    if !python3_available() {
        return;
    }
    for named in [false, true] {
        for everyone in [false, true] {
            let mut fixture = NamedTurnFixture::with_args(
                &format!("thread-turn-{named}-{everyone}"),
                vec![
                    "--pre-tool-text".into(),
                    "Step commentary".into(),
                    "--fail-once".into(),
                    "--generation-failure-once".into(),
                ],
            );
            fixture.install_default_worker_with_args(vec!["--generation-failure-once".into()]);
            let prefix = match (everyone, named) {
                (true, true) => "@Ada ",
                (true, false) => "@Marble ",
                (false, _) => "",
            };
            let root_prompt = format!("{prefix}root");
            let path = if everyone {
                "/api/everyone"
            } else {
                "/api/turns"
            };
            let (status, body) = fixture.post(
                path,
                json!({"text":root_prompt,"marbleId":if named {Some(&fixture.marble_id)} else {None}}),
            );
            assert_eq!(status, 202);
            if everyone {
                assert_eq!(
                    body["answering"],
                    json!([if named { "Ada" } else { "Marble" }])
                );
            }
            let root_task = fixture.completed(&root_prompt);
            let cv = if everyone {
                "everyone".into()
            } else {
                root_task.conversation_id.clone()
            };
            let parent = fixture
                .state
                .lock()
                .unwrap()
                .transcript
                .iter()
                .find(|e| {
                    e.conversation_id == cv && e.task_id == root_task.id && e.kind == "assistant"
                })
                .unwrap()
                .id
                .clone();
            let thread = fixture
                .post(
                    "/api/threads",
                    json!({"conversation_id":cv,"parent_entry_id":parent}),
                )
                .1["thread"]["id"]
                .as_str()
                .unwrap()
                .to_string();
            let prompt = format!("{prefix}reply");
            assert_eq!(
                fixture
                    .post(
                        path,
                        json!({"text":prompt,"conversationId":cv,"thread_id":thread})
                    )
                    .0,
                202
            );
            let task = fixture.completed(&prompt);
            assert_eq!(!task.marble_id.is_empty(), named);
            {
                let state = fixture.state.lock().unwrap();
                let entries: Vec<_> = state
                    .transcript
                    .iter()
                    .filter(|e| e.task_id == task.id)
                    .collect();
                for kind in ["user", "tool", "reasoning", "assistant"] {
                    assert!(entries.iter().any(|e| e.kind == kind), "missing {kind}");
                }
                assert!(
                    entries
                        .iter()
                        .all(|e| !e.id.is_empty() && e.thread_id.as_deref() == Some(&thread))
                );
            }
            let engine_storage = if named {
                fixture.root.join("prism-sessions").join(&fixture.marble_id)
            } else {
                fixture.root.join("default-sessions")
            };
            let input: Value = serde_json::from_slice(
                &std::fs::read(engine_storage.join("fixture-input.json")).unwrap(),
            )
            .unwrap();
            assert!(
                input["input"]["text"]
                    .as_str()
                    .unwrap()
                    .contains("This is a thread reply. Parent message from")
            );
            assert!(
                input["input"]["text"]
                    .as_str()
                    .unwrap()
                    .contains("handled ")
            );
            let failed_prompt = format!("{prefix}FAIL reply");
            assert_eq!(
                fixture
                    .post(
                        path,
                        json!({"text":failed_prompt,"conversationId":cv,"thread_id":thread})
                    )
                    .0,
                202
            );
            let failed = fixture.settled(&failed_prompt);
            assert_eq!(failed.status, "failed");
            assert!(fixture.state.lock().unwrap().transcript.iter().any(|e| {
                e.task_id == failed.id
                    && e.failure
                        .as_ref()
                        .is_some_and(|f| f.category == "provider_rejected")
            }));
            assert!(
                fixture
                    .state
                    .lock()
                    .unwrap()
                    .transcript
                    .iter()
                    .filter(|e| e.task_id == failed.id && e.kind == "failure")
                    .all(|e| e.thread_id.as_deref() == Some(&thread))
            );
            let store = StateStore::in_data_dir(&fixture.root);
            store.save(&fixture.state.lock().unwrap()).unwrap();
            *fixture.state.lock().unwrap() = store.load().unwrap();
            fixture.pool.shutdown_marble(&fixture.marble_id);
            fixture.install_default_worker_with_args(vec!["--generation-failure-once".into()]);
            assert_eq!(
                fixture
                    .post("/api/turns/retry", json!({"taskId":failed.id}))
                    .0,
                202
            );
            let retried = fixture.completed(&failed_prompt);
            assert!(
                fixture
                    .state
                    .lock()
                    .unwrap()
                    .transcript
                    .iter()
                    .filter(|e| e.task_id == retried.id)
                    .all(|e| e.thread_id.as_deref() == Some(&thread))
            );
            let events = fixture.events.since(0);
            for kind in ["text", "reasoning", "tool_start", "tool_end"] {
                assert!(
                    events.iter().any(|e| e.task_id == task.id
                        && e.kind == kind
                        && e.data["thread_id"] == thread),
                    "missing threaded {kind}"
                );
            }
            assert_eq!(
                fixture
                    .post(path, json!({"text":"invalid thread","thread_id":"missing"}))
                    .0,
                404
            );
        }
    }
}

#[test]
fn named_engines_use_the_host_model_and_keep_tool_rows_after_reload() {
    if !python3_available() {
        return;
    }
    let fixture = NamedTurnFixture::new("named-model-tools");
    fixture.state.lock().unwrap().model = "selected-model".into();
    let (status, _) = fixture.post(
        "/api/turns",
        json!({"text":"inspect", "marbleId":fixture.marble_id}),
    );
    assert_eq!(status, 202);
    let task = fixture.completed("inspect");
    let submitted: Value = serde_json::from_slice(
        &std::fs::read(
            fixture
                .root
                .join("prism-sessions")
                .join(&fixture.marble_id)
                .join("fixture-input.json"),
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(submitted["input"].as_object().unwrap().len(), 1);
    assert!(
        submitted["input"]["text"]
            .as_str()
            .unwrap()
            .ends_with("[User message]\ninspect")
    );
    assert!(
        fixture
            .state
            .lock()
            .unwrap()
            .transcript
            .iter()
            .any(|entry| entry.task_id == task.id
                && entry.kind == "user"
                && entry.text == "inspect")
    );
    let options: Value = serde_json::from_slice(
        &std::fs::read(
            fixture
                .root
                .join("prism-sessions")
                .join(&fixture.marble_id)
                .join("fixture-session-options.json"),
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(options["model"], "selected-model");
    let store = StateStore::in_data_dir(&fixture.root);
    store.save(&fixture.state.lock().unwrap()).unwrap();
    let saved = store.load().unwrap();
    assert!(saved.transcript.iter().any(|entry| entry.task_id == task.id
        && entry.kind == "tool"
        && entry.text == "glass.workspace.list · ok"));
}

#[test]
fn budget_stops_private_tools_and_everyone_time_with_an_agent_reply() {
    if !python3_available() {
        return;
    }
    for (label, count, everyone, expected) in [
        ("tools", "8", false, "Stopped at my limit of 8 tools."),
        ("time", "0", true, "Stopped at my time limit."),
    ] {
        let fixture = NamedTurnFixture::with_args(
            &format!("budget-{label}"),
            vec!["--budget-tools".into(), count.into()],
        );
        if everyone {
            fixture.state.lock().unwrap().prism.marbles[0].turn_timeout_secs = Some(1);
        }
        let prompt = if everyone {
            "@Ada Inspect the workspace"
        } else {
            "Inspect the workspace"
        };
        let (status, _) = if everyone {
            fixture.post("/api/everyone", json!({"text":prompt}))
        } else {
            fixture.post(
                "/api/turns",
                json!({"text":prompt,"marbleId":fixture.marble_id}),
            )
        };
        assert_eq!(status, 202);
        let task = fixture.completed(prompt);
        let state = fixture.state.lock().unwrap();
        let conversation = if everyone {
            "everyone"
        } else {
            &task.conversation_id
        };
        let reply = state
            .transcript
            .iter()
            .find(|row| {
                row.task_id == task.id
                    && row.conversation_id == conversation
                    && row.kind == "assistant"
            })
            .unwrap();
        assert!(reply.text.starts_with(expected), "{}", reply.text);
        if !everyone {
            assert!(reply.text.contains("Completed tools: read."));
        }
        let aborted = fixture
            .root
            .join("prism-sessions")
            .join(&fixture.marble_id)
            .join("fixture-aborted-task");
        assert_eq!(std::fs::read_to_string(aborted).unwrap(), task.id);
    }
}

#[test]
fn everyone_agent_workspaces_are_distinct_and_deleted_with_the_thread() {
    if !python3_available() {
        return;
    }
    let fixture = NamedTurnFixture::with_persistence("scoped-everyone-storage", vec![], true);
    let start = |title: &str| {
        let (status, body) = fixture.post(
            "/api/threads",
            json!({"conversation_id":"everyone","title":title}),
        );
        assert_eq!(status, 201);
        let thread = body["thread"]["id"].as_str().unwrap().to_owned();
        let prompt = format!("@Ada List the files in {title}");
        assert_eq!(
            fixture
                .post("/api/everyone", json!({"text":prompt,"thread_id":thread}))
                .0,
            202
        );
        let task = fixture.completed(&prompt);
        let root = fixture
            .root
            .join("conversation-sessions")
            .join(task.conversation_id);
        let options: Value = serde_json::from_slice(
            &std::fs::read(root.join("fixture-session-options.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(
            std::path::Path::new(options["cwd"].as_str().unwrap()),
            root.join("workspace").canonicalize().unwrap()
        );
        (thread, root)
    };
    let (first, first_root) = start("First");
    let (second, second_root) = start("Second");
    assert_ne!(first_root, second_root);
    std::fs::write(first_root.join("workspace").join("private.txt"), "first").unwrap();
    assert!(!second_root.join("workspace").join("private.txt").exists());
    let backing = first_root.file_name().unwrap().to_str().unwrap();
    let (status, body) = fixture.post(&format!("/api/conversations/{backing}/delete"), json!({}));
    assert_eq!(status, 409);
    assert_eq!(body["error"], "thread_owned_conversation");
    assert!(first_root.exists());
    assert_eq!(
        fixture
            .post(&format!("/api/threads/{first}/delete"), json!({}))
            .0,
        200
    );
    assert!(!first_root.exists());
    assert!(second_root.exists());
    let (status, _) = fixture.post("/api/everyone/clear", json!({}));
    assert_eq!(status, 200);
    assert!(!second_root.exists());
    assert_eq!(
        fixture
            .server
            .handle("GET", &format!("/api/threads/{second}"), "")
            .0,
        404
    );
}

#[test]
fn clearing_everyone_persists_before_attempting_workspace_cleanup() {
    let fixture = NamedTurnFixture::with_persistence("clear-everyone-disk", vec![], true);
    let store = fixture.store.as_ref().unwrap();
    let mut roots = Vec::new();
    let mut threads = Vec::new();
    for title in ["Removable", "Blocked", "Blocked until startup"] {
        let (status, body) = fixture.post(
            "/api/threads",
            json!({"conversation_id":"everyone","title":title}),
        );
        assert_eq!(status, 201, "{body}");
        let thread = body["thread"]["id"].as_str().unwrap().to_string();
        let conversation = format!("everyone-session-{thread}__ada");
        {
            let mut state = fixture.state.lock().unwrap();
            state.conversation_mut(&conversation).private_storage = true;
            state.push_transcript(TranscriptEntry {
                conversation_id: "everyone".into(),
                kind: "assistant".into(),
                text: title.into(),
                thread_id: Some(thread.clone()),
                ..Default::default()
            });
        }
        threads.push(thread);
        roots.push(
            fixture
                .root
                .join("conversation-sessions")
                .join(conversation),
        );
    }
    std::fs::create_dir_all(roots[0].join("workspace")).unwrap();
    std::fs::write(&roots[1], b"not a directory").unwrap();
    std::fs::write(&roots[2], b"not a directory").unwrap();
    store.save(&fixture.server.state()).unwrap();

    let backup = store.path().with_extension("original");
    std::fs::rename(store.path(), &backup).unwrap();
    std::fs::create_dir(store.path()).unwrap();
    let (status, body) = fixture.post("/api/everyone/clear", json!({}));
    assert_eq!(status, 503, "{body}");
    assert!(roots[0].is_dir());
    assert_eq!(fixture.server.state().threads.len(), 3);
    std::fs::remove_dir(store.path()).unwrap();
    std::fs::rename(backup, store.path()).unwrap();

    let (status, body) = fixture.post("/api/everyone/clear", json!({}));
    assert_eq!(status, 200, "{body}");
    assert_eq!(body["cleared"], 3);
    assert!(!roots[0].exists());
    assert!(roots[1].is_file());
    let reloaded = store.load().unwrap();
    assert!(reloaded.threads.is_empty());
    assert!(reloaded.transcript.is_empty());
    assert!(
        reloaded
            .conversations
            .iter()
            .all(|conversation| { !conversation.id.starts_with("everyone-session-") })
    );
    for thread in threads {
        let (status, _) = fixture
            .server
            .handle("GET", &format!("/api/threads/{thread}"), "");
        assert_eq!(status, 404);
    }

    let pending = |state: ProductState| {
        serde_json::to_value(state).unwrap()["pendingEveryoneCleanup"].clone()
    };
    let id = |index: usize| roots[index].file_name().unwrap().to_str().unwrap();
    let mut expected = [id(1), id(2)];
    expected.sort_unstable();
    assert_eq!(pending(reloaded), json!(expected));
    std::fs::remove_file(&roots[1]).unwrap();
    std::fs::create_dir_all(roots[1].join("workspace")).unwrap();
    assert_eq!(fixture.post("/api/everyone/clear", json!({})).0, 200);
    assert!(!roots[1].exists());
    let mut restarted = store.load().unwrap();
    assert_eq!(pending(restarted.clone()), json!([id(2)]));
    std::fs::remove_file(&roots[2]).unwrap();
    std::fs::create_dir_all(roots[2].join("workspace")).unwrap();
    store.retry_everyone_cleanup(&mut restarted);
    assert!(!roots[2].exists());
    assert!(pending(store.load().unwrap()).is_null());
}

#[test]
fn private_small_talk_disables_tools_without_changing_the_conversation() {
    if !python3_available() {
        return;
    }
    let fixture = NamedTurnFixture::new("private-small-talk-tool-policy");
    let turn = |prompt: &str| {
        assert_eq!(
            fixture
                .post(
                    "/api/turns",
                    json!({"text":prompt,"marbleId":fixture.marble_id})
                )
                .0,
            202
        );
        fixture.completed(prompt)
    };
    let fixture_json = |file: &str| -> Value {
        let path = fixture
            .root
            .join("prism-sessions")
            .join(&fixture.marble_id)
            .join(file);
        serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap()
    };
    let greeting = turn("Hello!");
    assert_eq!(
        fixture_json("fixture-tool-policy.json"),
        json!({"toolMode":"host","tools":[]})
    );
    assert_eq!(
        fixture_json("fixture-tool-outcome.json"),
        json!({"isError":true,"text":"Continue without tools: this is a conversational turn."})
    );
    let task = turn("List the workspace");
    assert_eq!(greeting.conversation_id, task.conversation_id);
    assert_eq!(
        fixture_json("fixture-tool-policy.json")["toolMode"],
        "mixed"
    );
    assert_eq!(fixture_json("fixture-tool-outcome.json")["isError"], false);
    let state = fixture.state.lock().unwrap();
    assert!(
        !state
            .transcript
            .iter()
            .any(|row| row.task_id == greeting.id && row.kind == "tool")
    );
    assert!(
        state
            .transcript
            .iter()
            .any(|row| row.task_id == task.id && row.kind == "tool")
    );
}

#[test]
fn agent_card_limits_host_tool_registration_and_execution_in_both_modes() {
    if !python3_available() {
        return;
    }
    let fixture = NamedTurnFixture::new("card-tool-policy");
    let update = format!("/api/prism/marbles/{}/update", fixture.marble_id);
    let path = fixture.root.join("prism-sessions").join(&fixture.marble_id);
    let read_json = |file: &str| -> Value {
        serde_json::from_slice(&std::fs::read(path.join(file)).unwrap()).unwrap()
    };
    for (native, allowed, prompt, mode, denied) in [
        (
            false,
            vec!["glass.workspace.read"],
            "List the workspace without permission",
            "host",
            true,
        ),
        (
            true,
            vec!["glass.workspace.read"],
            "List the workspace using mixed mode",
            "mixed",
            true,
        ),
        (
            false,
            vec!["glass.workspace.list"],
            "List the workspace with permission",
            "host",
            false,
        ),
    ] {
        assert_eq!(
            fixture
                .post(
                    &update,
                    json!({"card":{"tools":{"native":native,"allowed":allowed}}})
                )
                .0,
            200
        );
        assert_eq!(
            fixture
                .post(
                    "/api/turns",
                    json!({"text":prompt,"marbleId":fixture.marble_id})
                )
                .0,
            202
        );
        fixture.completed(prompt);
        let policy = read_json("fixture-tool-policy.json");
        assert_eq!(policy["toolMode"], mode);
        assert_eq!(policy["tools"].as_array().unwrap().len(), 1);
        assert_eq!(policy["tools"][0]["name"], allowed[0]);
        let outcome = read_json("fixture-tool-outcome.json");
        assert_eq!(outcome["isError"], denied);
        if denied {
            assert_eq!(
                outcome["text"],
                "This tool is not on the agent's allowed tools list."
            );
        }
    }
}

#[test]
fn everyone_threads_keep_ada_private_history_out_and_support_archive_restore_and_delete() {
    if !python3_available() {
        return;
    }
    let fixture =
        NamedTurnFixture::with_args("isolated-greeting", vec!["--isolated-greeting".into()]);
    let private_prompt = "Inspect remark.pdf";
    assert_eq!(
        fixture
            .post(
                "/api/turns",
                json!({"text":private_prompt, "marbleId":fixture.marble_id})
            )
            .0,
        202
    );
    let private = fixture.completed(private_prompt);
    let (status, created) = fixture.post(
        "/api/threads",
        json!({"conversation_id":"everyone","title":"Hello Ada"}),
    );
    assert_eq!(status, 201);
    assert_eq!(created["thread"]["title"], "Hello Ada");
    let id = created["thread"]["id"].as_str().unwrap();
    assert_eq!(
        fixture
            .post(
                "/api/everyone",
                json!({"text":"@Ada hello", "thread_id":id})
            )
            .0,
        202
    );
    let shared = fixture.completed("@Ada hello");
    {
        let state = fixture.state.lock().unwrap();
        assert_ne!(private.conversation_id, shared.conversation_id);
        assert_eq!(
            state.prism.get(&fixture.marble_id).unwrap().conversation_id,
            private.conversation_id
        );
        assert!(
            state
                .transcript
                .iter()
                .any(|entry| entry.task_id == shared.id
                    && entry.conversation_id == "everyone"
                    && entry.kind == "assistant"
                    && entry.text == "Hello!")
        );
        assert!(
            !state
                .transcript
                .iter()
                .any(|entry| entry.task_id == shared.id && entry.kind == "tool")
        );
    }
    let path = format!("/api/threads/{id}");
    assert_eq!(fixture.post(&format!("{path}/archive"), json!({})).0, 200);
    assert_eq!(
        fixture
            .post(
                "/api/everyone",
                json!({"text":"@Ada hello", "thread_id":id})
            )
            .0,
        409
    );
    assert_eq!(fixture.post(&format!("{path}/restore"), json!({})).0, 200);
    assert_eq!(fixture.post(&format!("{path}/delete"), json!({})).0, 200);
    let state = fixture.state.lock().unwrap();
    assert!(!state.threads.iter().any(|thread| thread.id == id));
    assert!(
        !state
            .conversations
            .iter()
            .any(|chat| chat.id == shared.conversation_id)
    );
    assert!(
        !state
            .transcript
            .iter()
            .any(|entry| entry.thread_id.as_deref() == Some(id))
    );
    assert!(state.tasks.iter().any(|task| task.id == private.id));
}

#[test]
fn everyone_dispatches_to_named_members_or_all_when_none_are_mentioned() {
    if !python3_available() {
        return;
    }
    let mut fixture = NamedTurnFixture::with_args("everyone-default-routing", Vec::new());
    fixture.install_default_worker();
    for (prompt, named, default) in [
        ("@Ada hi", true, false),
        ("@Marble take the lead @everyone", true, true),
        ("@Marble and @Ada help", true, true),
        ("no mention here", true, true),
    ] {
        assert_eq!(fixture.post("/api/everyone", json!({"text":prompt})).0, 202);
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            let state = fixture.state.lock().unwrap();
            let turns: Vec<_> = state.tasks.iter().filter(|t| t.prompt == prompt).collect();
            if turns.len() == usize::from(named) + usize::from(default)
                && turns.iter().all(|t| t.status == "completed")
            {
                assert_eq!(
                    turns.iter().any(|t| t.marble_id == fixture.marble_id),
                    named
                );
                assert_eq!(turns.iter().any(|t| t.marble_id.is_empty()), default);
                break;
            }
            assert!(
                Instant::now() < deadline,
                "dispatch did not settle: {turns:?}"
            );
            drop(state);
            std::thread::sleep(Duration::from_millis(10));
        }
        if named && default {
            for (storage, owner) in [
                (fixture.root.join("default-sessions"), "Marble"),
                (
                    fixture.root.join("prism-sessions").join(&fixture.marble_id),
                    "Ada",
                ),
            ] {
                let input: Value = serde_json::from_slice(
                    &std::fs::read(storage.join("fixture-input.json")).unwrap(),
                )
                .unwrap();
                assert!(input["input"]["text"].as_str().unwrap().contains(&format!(
                    "This message is addressed to: Marble (lead), Ada. You are {owner}."
                )));
            }
        }
    }
}

#[test]
fn everyone_failures_retry_on_a_checkpoint_fork_or_keep_later_turns_across_restart() {
    if !python3_available() {
        return;
    }
    for named in [false, true] {
        for later in [false, true] {
            let mut fixture = NamedTurnFixture::with_args(
                &format!("fork-retry-{named}-{later}"),
                vec!["--fail-once".into()],
            );
            fixture.install_default_worker_with_args(vec!["--generation-failure-once".into()]);
            let prefix = if named { "@Ada " } else { "@Marble " };
            let before = format!("{prefix}hello");
            assert_eq!(fixture.post("/api/everyone", json!({"text":before})).0, 202);
            fixture.completed(&before);
            let prompt = format!("{prefix}FAIL research");
            assert_eq!(fixture.post("/api/everyone", json!({"text":prompt})).0, 202);
            let failed = fixture.settled(&prompt);
            assert_eq!(failed.status, "failed");
            {
                let state = fixture.state.lock().unwrap();
                let failures: Vec<_> = state
                    .transcript
                    .iter()
                    .filter(|e| e.task_id == failed.id && e.kind == "failure")
                    .collect();
                assert_eq!(
                    failures.len(),
                    2,
                    "own chat and Everyone both carry a failure"
                );
                assert!(
                    failures
                        .iter()
                        .all(|e| e.text == glass_marble::failure::MESSAGE
                            && e.failure.as_ref().unwrap().category == "provider_rejected")
                );
                assert!(
                    !serde_json::to_string(&state.transcript)
                        .unwrap()
                        .contains("secret-token")
                );
            }
            if later {
                let later = format!("{prefix}later message");
                assert_eq!(fixture.post("/api/everyone", json!({"text":later})).0, 202);
                fixture.completed(&later);
            }
            let failed_engine_id = fixture
                .state
                .lock()
                .unwrap()
                .conversations
                .iter()
                .find(|record| record.id == failed.conversation_id)
                .unwrap()
                .engine_conversation_id
                .clone();
            let store = StateStore::in_data_dir(&fixture.root);
            store.save(&fixture.state.lock().unwrap()).unwrap();
            *fixture.state.lock().unwrap() = store.load().unwrap();
            fixture.pool.shutdown_marble(&fixture.marble_id);
            fixture.install_default_worker();
            assert_eq!(
                fixture
                    .post("/api/turns/retry", json!({"taskId":failed.id}))
                    .0,
                202
            );
            let retried = fixture.completed(&prompt);
            assert_ne!(retried.id, failed.id);
            let storage = if named {
                fixture.root.join("prism-sessions").join(&fixture.marble_id)
            } else {
                fixture.root.join("default-sessions")
            };
            let submitted: Value =
                serde_json::from_slice(&std::fs::read(storage.join("fixture-input.json")).unwrap())
                    .unwrap();
            let engine_id = submitted["conversationId"].as_str().unwrap().to_string();
            let history: Value = serde_json::from_slice(
                &std::fs::read(storage.join("fixture-history.json")).unwrap(),
            )
            .unwrap();
            let entries = history["conversations"][&engine_id].as_array().unwrap();
            if later {
                assert_eq!(engine_id, failed_engine_id);
                assert!(
                    submitted["input"]["text"]
                        .as_str()
                        .unwrap()
                        .contains("retry of an earlier failed message")
                );
                assert_eq!(entries.len(), 4);
            } else {
                assert_ne!(engine_id, failed_engine_id);
                assert_eq!(
                    entries.len(),
                    2,
                    "one prior message and one retry, never two failed prompts"
                );
            }
            let continued = format!("{prefix}continue");
            store.save(&fixture.state.lock().unwrap()).unwrap();
            *fixture.state.lock().unwrap() = store.load().unwrap();
            fixture.pool.shutdown_marble(&fixture.marble_id);
            fixture.install_default_worker();
            assert_eq!(
                fixture.post("/api/everyone", json!({"text":continued})).0,
                202
            );
            fixture.completed(&continued);
            let next: Value =
                serde_json::from_slice(&std::fs::read(storage.join("fixture-input.json")).unwrap())
                    .unwrap();
            assert_eq!(next["conversationId"], engine_id);
        }
    }
}

#[test]
fn named_followups_include_ios_acceptance_and_legacy_dispatch_fields() {
    if !python3_available() {
        return;
    }
    #[derive(serde::Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Accepted {
        accepted: bool,
        request_id: String,
        conversation_id: String,
        task_id: String,
        marble_name: Option<String>,
    }
    let fixture = NamedTurnFixture::new("named-acceptance");
    assert_eq!(
        fixture
            .post(
                "/api/turns",
                json!({"text":"first message", "marbleId":fixture.marble_id})
            )
            .0,
        202
    );
    let first = fixture.settled("first message");
    assert_eq!(first.status, "completed");
    let (status, value) = fixture.post(
        "/api/turns",
        json!({"text":"follow up", "conversationId":first.conversation_id}),
    );
    assert_eq!(status, 202, "{value}");
    let accepted: Accepted = serde_json::from_value(value.clone()).expect("iOS acceptance fields");
    assert!(accepted.accepted);
    assert_eq!(accepted.conversation_id, first.conversation_id);
    assert_eq!(accepted.marble_name.as_deref(), Some("Ada"));
    assert!(accepted.request_id.is_empty());
    assert!(accepted.task_id.is_empty());
    assert_eq!(value["dispatched"], json!([fixture.marble_id]));
    assert_eq!(value["marbles"], json!(["Ada"]));
    assert_eq!(value["swarm"], false);
    assert_eq!(value["ok"], true);
    assert!(value["prism"].is_object());
    assert_eq!(fixture.settled("follow up").status, "completed");
    assert_eq!(
        fixture
            .post(
                "/api/turns",
                json!({"text":"", "marbleId":fixture.marble_id})
            )
            .0,
        400
    );
    assert_eq!(
        fixture
            .post("/api/turns", json!({"text":"hello", "marbleId":"missing"}))
            .0,
        404
    );
}

#[test]
fn held_first_chat_startup_failures_are_recorded_and_retry_to_the_named_marble() {
    if !python3_available() {
        return;
    }
    let fixture = NamedTurnFixture::new("held-startup-failure");
    std::fs::write(
        &fixture.script,
        "raise RuntimeError('fixture refuses startup')\n",
    )
    .unwrap();
    assert_eq!(fixture.post("/api/control/pause", json!({})).0, 200);
    for prompt in ["first startup failure", "second startup failure"] {
        let (status, body) = fixture.post(
            "/api/turns",
            json!({"text":prompt, "marbleId":fixture.marble_id}),
        );
        assert_eq!(status, 202, "{body}");
    }
    assert_eq!(fixture.post("/api/control/resume", json!({})).0, 200);
    let first = fixture.settled("first startup failure");
    let second = fixture.settled("second startup failure");
    for task in [&first, &second] {
        assert_eq!(task.status, "failed");
        assert_eq!(
            serde_json::to_value(task).unwrap()["marbleId"],
            fixture.marble_id
        );
    }
    let saved = serde_json::to_value(fixture.server.state()).unwrap();
    *fixture.state.lock().unwrap() = serde_json::from_value(saved).unwrap();
    std::fs::write(
        &fixture.script,
        include_str!("fixtures/fake_glass_core_runtime.py"),
    )
    .unwrap();
    let (status, body) = fixture.post("/api/turns/retry", json!({"taskId":first.id}));
    assert_eq!(status, 202, "{body}");
    assert_eq!(body["marbleName"], "Ada");
    let completed = fixture.completed("first startup failure");
    assert_eq!(
        serde_json::to_value(&completed).unwrap()["marbleId"],
        fixture.marble_id
    );
    assert_ne!(completed.id, first.id);
    assert!(fixture.state.lock().unwrap().control.queue.is_empty());
}

#[test]
fn removing_a_marble_clears_all_failed_first_chat_history() {
    if !python3_available() {
        return;
    }
    let fixture = NamedTurnFixture::new("remove-failed-first-chat");
    std::fs::write(
        &fixture.script,
        "raise RuntimeError('fixture refuses startup')\n",
    )
    .unwrap();
    let ben = fixture
        .state
        .lock()
        .unwrap()
        .prism
        .add("Ben", "reviewer", "hex", 210, "", "")
        .unwrap()
        .id
        .clone();
    let mut failed = Vec::new();
    for prompt in ["first private prompt", "second private prompt"] {
        assert_eq!(
            fixture
                .post(
                    "/api/turns",
                    json!({"text":prompt, "marbleId":fixture.marble_id})
                )
                .0,
            202
        );
        failed.push(fixture.settled(prompt));
    }
    assert_eq!(
        fixture
            .post(
                "/api/turns",
                json!({"text":"Ben keeps this", "marbleId":ben})
            )
            .0,
        202
    );
    let kept = fixture.settled("Ben keeps this");
    assert!(
        fixture
            .state
            .lock()
            .unwrap()
            .prism
            .get(&fixture.marble_id)
            .unwrap()
            .conversation_id
            .is_empty()
    );
    let store = Arc::new(StateStore::in_data_dir(&fixture.root));
    store.save(&fixture.server.state()).unwrap();
    *fixture.state.lock().unwrap() = store.load().unwrap();
    let server = MarbleServer::new(MarbleServerParts {
        listener: TcpListener::bind("127.0.0.1:0").unwrap(),
        state: Arc::clone(&fixture.state),
        store: Some(Arc::clone(&store)),
        approvals: ApprovalInbox::default(),
        events: EventBus::default(),
        turn_tx: TurnSender::detached(),
        steer_tx: mpsc::channel().0,
        policy: AccessPolicy::new(None),
        cancels: CancelRegistry::default(),
    })
    .with_prism(Arc::clone(&fixture.pool));
    let path = format!("/api/prism/marbles/{}/remove", fixture.marble_id);
    assert_eq!(server.handle("POST", &path, "{}").0, 200);
    for state in [server.state(), store.load().unwrap()] {
        assert!(state.prism.get(&fixture.marble_id).is_none());
        for task in &failed {
            assert!(
                !state.tasks.iter().any(|item| item.id == task.id),
                "removed Marble's task survived"
            );
            assert!(
                !state
                    .conversations
                    .iter()
                    .any(|item| item.id == task.conversation_id)
            );
            assert!(!state.transcript.iter().any(
                |item| item.task_id == task.id || item.conversation_id == task.conversation_id
            ));
        }
        assert!(state.tasks.iter().any(|task| task.id == kept.id));
        assert!(state.transcript.iter().any(|item| item.task_id == kept.id));
    }
    assert_eq!(server.handle("POST", &path, "{}").0, 404);
}

#[test]
fn cancelling_a_held_first_chat_preserves_its_named_retry_target() {
    if !python3_available() {
        return;
    }
    let fixture = NamedTurnFixture::new("held-cancel-retry");
    assert!(
        fixture
            .state
            .lock()
            .unwrap()
            .prism
            .get(&fixture.marble_id)
            .unwrap()
            .conversation_id
            .is_empty()
    );
    assert_eq!(fixture.post("/api/control/pause", json!({})).0, 200);
    let prompt = "retry my cancelled named message";
    let (status, queued) = fixture.post(
        "/api/turns",
        json!({"text":prompt, "marbleId":fixture.marble_id}),
    );
    assert_eq!(status, 202, "{queued}");
    let queue_id = queued["queueId"].as_str().unwrap();
    assert_eq!(
        fixture
            .post(&format!("/api/tasks/{queue_id}/cancel"), json!({}))
            .0,
        200
    );
    let saved = serde_json::to_value(fixture.server.state()).unwrap();
    *fixture.state.lock().unwrap() = serde_json::from_value(saved).unwrap();
    assert_eq!(fixture.post("/api/control/resume", json!({})).0, 200);
    let (status, accepted) = fixture.post("/api/turns/retry", json!({"taskId":queue_id}));
    assert_eq!(status, 202, "{accepted}");
    assert_eq!(accepted["marbleId"], fixture.marble_id);
    let completed = fixture.completed(prompt);
    assert_eq!(completed.marble_id, fixture.marble_id);
    assert_eq!(
        fixture
            .state
            .lock()
            .unwrap()
            .prism
            .get(&fixture.marble_id)
            .unwrap()
            .conversation_id,
        completed.conversation_id
    );
    let state = fixture.state.lock().unwrap();
    let cancelled = state.tasks.iter().find(|task| task.id == queue_id).unwrap();
    assert_eq!(cancelled.status, "cancelled");
    assert_eq!(cancelled.marble_id, fixture.marble_id);
}

#[test]
fn named_turn_intake_handles_memory_and_resumes_held_messages_in_order() {
    if !python3_available() {
        return;
    }
    let root =
        std::env::temp_dir().join(format!("glass-marble-named-intake-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let script = root.join("fake_glass_core_runtime.py");
    std::fs::write(&script, include_str!("fixtures/fake_glass_core_runtime.py")).unwrap();
    let events = EventBus::default();
    let mut initial = ProductState::default();
    initial.control.profile = glass_marble::ControlProfile::Unattended;
    let ada = initial
        .prism
        .add("Ada", "writer", "star", 35, "", "")
        .unwrap()
        .clone();
    let state = Arc::new(Mutex::new(initial));
    let pool = fixture_prism_pool(&script, &root, &state, &events, Vec::new());
    let server = MarbleServer::new(MarbleServerParts {
        listener: TcpListener::bind("127.0.0.1:0").unwrap(),
        state: Arc::clone(&state),
        store: None,
        approvals: ApprovalInbox::default(),
        events,
        turn_tx: TurnSender::detached(),
        steer_tx: mpsc::channel().0,
        policy: AccessPolicy::new(None),
        cancels: CancelRegistry::default(),
    })
    .with_prism(Arc::clone(&pool));
    assert_eq!(server.handle("POST", "/api/control/pause", "").0, 200);
    let remember =
        json!({"text": "Remember this: prefer concise notes", "marbleId": ada.id}).to_string();
    let (status, body) = server.handle("POST", "/api/turns", &remember);
    assert_eq!(status, 200, "{body}");
    assert_eq!(
        serde_json::from_str::<Value>(&body).unwrap()["remembered"]["owner"],
        ada.id
    );
    assert!(state.lock().unwrap().tasks.is_empty());
    let forget = json!({"text": "forget: concise notes", "marbleId": ada.id}).to_string();
    assert_eq!(server.handle("POST", "/api/turns", &forget).0, 200);
    assert!(state.lock().unwrap().memory.facts.is_empty());
    let message = json!({"text": "write an outline", "marbleId": ada.id}).to_string();
    let (status, body) = server.handle("POST", "/api/turns", &message);
    assert_eq!(status, 202, "{body}");
    assert!(state.lock().unwrap().tasks.is_empty());
    let queue = control_of(&server)["queue"].clone();
    assert_eq!(queue[0]["marbleId"], ada.id);
    let cancel = format!("/api/tasks/{}/cancel", queue[0]["id"].as_str().unwrap());
    assert_eq!(server.handle("POST", &cancel, "").0, 200);
    assert_eq!(server.handle("POST", "/api/turns", &message).0, 202);
    let second = json!({"text": "check the outline", "marbleId": ada.id}).to_string();
    assert_eq!(server.handle("POST", "/api/turns", &second).0, 202);
    assert_eq!(server.handle("POST", "/api/control/resume", "").0, 200);
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        if state
            .lock()
            .unwrap()
            .tasks
            .iter()
            .filter(|task| task.status == "completed")
            .count()
            == 2
        {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "named queued message did not finish"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
    let conversation = state
        .lock()
        .unwrap()
        .prism
        .get(&ada.id)
        .unwrap()
        .conversation_id
        .clone();
    assert!(!conversation.is_empty());
    {
        let state = state.lock().unwrap();
        let messages: Vec<_> = state
            .transcript
            .iter()
            .filter(|entry| entry.conversation_id == conversation && entry.kind == "user")
            .map(|entry| entry.text.as_str())
            .collect();
        assert_eq!(messages, ["write an outline", "check the outline"]);
        let settled: Vec<_> = state
            .tasks
            .iter()
            .filter(|task| task.status == "completed")
            .collect();
        assert_eq!(settled.len(), 2);
        assert!(
            settled
                .iter()
                .all(|task| task.conversation_id == conversation)
        );
        assert!(state.control.queue.is_empty());
    }
    let remember = json!({"text": "Remember this: check the sources", "marbleId": ada.id, "conversationId": conversation}).to_string();
    assert_eq!(server.handle("POST", "/api/turns", &remember).0, 200);
    assert_eq!(state.lock().unwrap().memory.facts[0].owner, ada.id);
    assert_eq!(
        server
            .handle(
                "POST",
                "/api/turns",
                r#"{"text":"hello","marbleId":"missing"}"#
            )
            .0,
        404
    );
    assert_eq!(
        server
            .handle("POST", "/api/turns", r#"{"text":"hello","marbleId":1}"#)
            .0,
        400
    );
    assert_eq!(
        server
            .handle(
                "POST",
                "/api/turns",
                &json!({"text":"hello","marbleId":ada.id,"conversationId":"wrong-chat"})
                    .to_string()
            )
            .0,
        400
    );
    pool.shutdown();
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn retiring_failed_prism_engines_restores_health() {
    if !python3_available() {
        return;
    }
    let root = std::env::temp_dir().join(format!("marble-prism-health-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let script = root.join("fake_glass_core_runtime.py");
    std::fs::write(&script, include_str!("fixtures/fake_glass_core_runtime.py")).unwrap();
    let events = EventBus::default();
    let state = Arc::new(Mutex::new(ProductState::default()));
    let ids: Vec<String> = ["ben", "ada"]
        .into_iter()
        .map(|name| {
            state
                .lock()
                .unwrap()
                .prism
                .add(name, "tester", "circle", 200, "", "")
                .unwrap()
                .id
                .clone()
        })
        .collect();
    let pool = fixture_prism_pool(
        &script,
        &root,
        &state,
        &events,
        vec!["--exit-after-initialize".into()],
    );
    let server = MarbleServer::new(MarbleServerParts {
        listener: TcpListener::bind("127.0.0.1:0").unwrap(),
        state: state.clone(),
        store: None,
        approvals: ApprovalInbox::default(),
        events,
        turn_tx: TurnSender::detached(),
        steer_tx: mpsc::channel().0,
        policy: AccessPolicy::new(None),
        cancels: CancelRegistry::default(),
    })
    .with_prism(pool);
    let dispatch = |id: &str| {
        let (status, body) = server.handle(
            "POST",
            "/api/prism/dispatch",
            &json!({"text": "hello", "marbles": [id]}).to_string(),
        );
        assert_eq!(status, 202, "{body}");
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            let failed = state.lock().unwrap().prism.get(id).unwrap().state == "blocked";
            if failed && server.handle("GET", "/healthz", "").0 == 503 {
                break;
            }
            assert!(Instant::now() < deadline, "engine did not report failure");
            std::thread::sleep(Duration::from_millis(20));
        }
    };
    dispatch(&ids[0]);
    dispatch(&ids[1]);
    assert_eq!(
        server
            .handle(
                "POST",
                &format!("/api/prism/marbles/{}/update", ids[0]),
                r#"{"persona":"new personality"}"#
            )
            .0,
        200
    );
    assert_eq!(server.handle("GET", "/healthz", "").0, 503);
    assert_eq!(
        server
            .handle("POST", &format!("/api/prism/marbles/{}/remove", ids[1]), "")
            .0,
        200
    );
    assert_eq!(server.handle("GET", "/healthz", "").0, 200);
    dispatch(&ids[0]);
    assert_eq!(
        server
            .handle("POST", &format!("/api/prism/marbles/{}/remove", ids[0]), "")
            .0,
        200
    );
    assert_eq!(server.handle("GET", "/healthz", "").0, 200);
}

/// An @mention answers through the named marble's own engine and the reply
/// lands on the shared timeline with its name on it.
#[test]
fn an_everyone_mention_routes_to_the_named_marble() {
    if !python3_available() {
        return;
    }
    let root = std::env::temp_dir().join(format!("glass-marble-everyone-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    let script = root.join("fake_glass_core_runtime.py");
    std::fs::write(&script, include_str!("fixtures/fake_glass_core_runtime.py")).unwrap();

    let events = EventBus::default();
    let state = Arc::new(Mutex::new(ProductState::default()));
    {
        let mut guard = state.lock().unwrap_or_else(|error| error.into_inner());
        guard.control.profile = glass_marble::ControlProfile::Unattended;
        guard
            .prism
            .add("ben", "careful tester", "circle", 200, "", "")
            .unwrap();
    }
    let pool = fixture_prism_pool(&script, &root, &state, &events, Vec::new());
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let server = MarbleServer::new(MarbleServerParts {
        listener,
        state: Arc::clone(&state),
        store: None,
        approvals: ApprovalInbox::default(),
        events,
        turn_tx: TurnSender::detached(),
        steer_tx: mpsc::channel().0,
        policy: AccessPolicy::new(None),
        cancels: CancelRegistry::default(),
    })
    .with_prism(pool);

    let (status, body) = server.handle("POST", "/api/everyone", r#"{"text":"@ben hello there"}"#);
    assert_eq!(status, 202, "{body}");
    let deadline = Instant::now() + Duration::from_secs(45);
    loop {
        let entries = everyone_entries(&state);
        if let Some(reply) = entries
            .iter()
            .find(|entry| entry.kind == "assistant" && entry.speaker_name == "ben")
        {
            assert_eq!(reply.text, "handled @ben hello there");
            break;
        }
        assert!(Instant::now() < deadline, "no mirrored reply: {entries:?}");
        std::thread::sleep(Duration::from_millis(50));
    }
    let entries = everyone_entries(&state);
    assert!(
        state
            .lock()
            .unwrap()
            .tasks
            .iter()
            .all(|task| !task.marble_id.is_empty())
    );
    assert!(
        entries
            .iter()
            .any(|entry| entry.kind == "user" && entry.speaker == "user")
    );
    let (status, body) = server.handle("GET", "/api/everyone", "");
    assert_eq!(status, 200, "{body}");
    let timeline: Value = serde_json::from_str(&body).unwrap();
    assert!(timeline["entries"].as_array().unwrap().len() >= 2);

    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn everyone_follow_up_steers_the_active_agent_without_a_second_run() {
    if !python3_available() {
        return;
    }
    let root = std::env::temp_dir().join(format!("glass-marble-busy-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    let script = root.join("fake_glass_core_runtime.py");
    std::fs::write(&script, include_str!("fixtures/fake_glass_core_runtime.py")).unwrap();

    let events = EventBus::default();
    let state = Arc::new(Mutex::new(ProductState::default()));
    {
        let mut guard = state.lock().unwrap_or_else(|error| error.into_inner());
        guard.control.profile = glass_marble::ControlProfile::Unattended;
        guard
            .prism
            .add("Ada", "careful tester", "circle", 200, "", "")
            .unwrap();
    }
    let pool = fixture_prism_pool(
        &script,
        &root,
        &state,
        &events,
        vec!["--wait-for-steer".into()],
    );
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let server = MarbleServer::new(MarbleServerParts {
        listener,
        state: Arc::clone(&state),
        store: None,
        approvals: ApprovalInbox::default(),
        events: events.clone(),
        turn_tx: TurnSender::detached(),
        steer_tx: mpsc::channel().0,
        policy: AccessPolicy::new(None),
        cancels: CancelRegistry::default(),
    })
    .with_prism(pool);

    let (_, body) = server.handle(
        "POST",
        "/api/threads",
        r#"{"conversation_id":"everyone","title":"Walk"}"#,
    );
    let created: Value = serde_json::from_str(&body).unwrap();
    let thread = &created["thread"]["id"];
    let receiver = events.subscribe();
    let (status, body) = server.handle(
        "POST",
        "/api/everyone",
        &json!({"text":"@Ada first","thread_id":thread}).to_string(),
    );
    assert_eq!(status, 202, "{body}");
    while receiver.recv_timeout(Duration::from_secs(5)).unwrap().kind != "turn_start" {}
    let (status, body) = server.handle(
        "POST",
        "/api/everyone",
        &json!({"text":"@Ada second","thread_id":thread}).to_string(),
    );
    assert_eq!(status, 202, "{body}");
    assert_eq!(
        serde_json::from_str::<Value>(&body).unwrap()["busy"],
        json!([]),
        "{body}"
    );
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let entries = everyone_entries(&state);
        let replies: Vec<_> = entries
            .iter()
            .filter(|entry| entry.kind == "assistant")
            .collect();
        if !replies.is_empty() {
            assert_eq!(replies.len(), 1);
            assert!(
                replies[0]
                    .text
                    .contains("With your correction in mind: @Ada second")
            );
            let guard = state.lock().unwrap();
            assert_eq!(guard.tasks.len(), 1);
            assert!(guard.control.queue.is_empty());
            break;
        }
        assert!(Instant::now() < deadline, "{entries:?}");
        std::thread::sleep(Duration::from_millis(20));
    }

    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn everyone_task_follow_up_waits_for_a_tool_free_greeting_to_settle() {
    if !python3_available() {
        return;
    }
    let fixture = NamedTurnFixture::with_persistence(
        "queued-after-greeting",
        vec!["--delay-ms".into(), "1000".into()],
        true,
    );
    let receiver = fixture.events.subscribe();
    assert_eq!(
        fixture
            .post("/api/everyone", json!({"text":"@Ada hello"}))
            .0,
        202
    );
    while receiver.recv_timeout(Duration::from_secs(5)).unwrap().kind != "turn_start" {}
    let prompt = "@Ada list the workspace files";
    assert_eq!(fixture.post("/api/everyone", json!({"text":prompt})).0, 202);
    let greeting = fixture.completed("@Ada hello");
    let task = fixture.completed(prompt);
    assert_ne!(greeting.id, task.id);
    assert_eq!(greeting.conversation_id, task.conversation_id);
    let state = fixture.server.state();
    assert!(
        !state
            .transcript
            .iter()
            .any(|e| e.task_id == greeting.id && e.kind == "tool")
    );
    assert!(
        state
            .transcript
            .iter()
            .any(|e| e.task_id == task.id && e.kind == "tool")
    );
}

/// One marble can ask another for help; both sides of the ask are written in
/// plain words on the shared timeline.
#[test]
fn a_marble_can_ask_another_and_the_ask_is_visible() {
    if !python3_available() {
        return;
    }
    let root = std::env::temp_dir().join(format!("glass-marble-ask-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    let script = root.join("fake_glass_core_runtime.py");
    std::fs::write(&script, include_str!("fixtures/fake_glass_core_runtime.py")).unwrap();

    let events = EventBus::default();
    let state = Arc::new(Mutex::new(ProductState::default()));
    let (ada_id, ben_id) = {
        let mut guard = state.lock().unwrap_or_else(|error| error.into_inner());
        guard.control.profile = glass_marble::ControlProfile::Unattended;
        let ada = guard
            .prism
            .add("ada", "thoughtful planner", "circle", 20, "", "")
            .unwrap()
            .id
            .clone();
        let ben = guard
            .prism
            .add("ben", "careful tester", "square", 200, "", "")
            .unwrap()
            .id
            .clone();
        (ada, ben)
    };
    let pool = fixture_prism_pool(&script, &root, &state, &events, Vec::new());
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let server = MarbleServer::new(MarbleServerParts {
        listener,
        state: Arc::clone(&state),
        store: None,
        approvals: ApprovalInbox::default(),
        events,
        turn_tx: TurnSender::detached(),
        steer_tx: mpsc::channel().0,
        policy: AccessPolicy::new(None),
        cancels: CancelRegistry::default(),
    })
    .with_prism(pool);

    // Ada's fake engine reads the prompt and asks Ben to check the tests.
    let (status, body) = server.handle(
        "POST",
        "/api/everyone",
        r#"{"text":"@ada ASK ben: check the failing tests"}"#,
    );
    assert_eq!(status, 202, "{body}");
    let deadline = Instant::now() + Duration::from_secs(60);
    loop {
        let entries = everyone_entries(&state);
        let finished = entries.iter().any(|entry| {
            entry.kind == "handoff"
                && entry
                    .handoff
                    .as_ref()
                    .is_some_and(|handoff| handoff.status == "finished")
        });
        if finished {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "the ask never finished: {entries:?}"
        );
        std::thread::sleep(Duration::from_millis(50));
    }
    let entries = everyone_entries(&state);
    let asked = entries.iter().find_map(|entry| {
        entry
            .handoff
            .as_ref()
            .filter(|handoff| handoff.status == "asked")
    });
    let asked = asked.expect("an asked line was written");
    assert_eq!(asked.from, ada_id);
    assert_eq!(asked.to, ben_id);
    assert_eq!(asked.instruction, "check the failing tests");
    let finished = entries
        .iter()
        .find_map(|entry| {
            entry
                .handoff
                .as_ref()
                .filter(|handoff| handoff.status == "finished")
        })
        .expect("a finished line was written");
    assert!(finished.summary.contains("check the failing tests"));

    loop {
        let guard = state.lock().unwrap();
        if let Some(delegate) = guard
            .tasks
            .iter()
            .find(|t| t.marble_id == ben_id && t.status == "completed")
        {
            let thread_id = guard
                .task_threads
                .get(&delegate.id)
                .expect("delegation starts a thread");
            let thread = guard.threads.iter().find(|t| &t.id == thread_id).unwrap();
            let parent = guard
                .transcript
                .iter()
                .find(|e| e.id == thread.parent_entry_id)
                .unwrap();
            assert_eq!(parent.kind, "handoff");
            assert_eq!(parent.conversation_id, "everyone");
            assert_eq!(parent.handoff.as_ref().unwrap().status, "asked");
            let replies: Vec<_> = guard
                .transcript
                .iter()
                .filter(|e| e.task_id == delegate.id)
                .collect();
            assert!(replies.iter().any(|e| e.kind == "tool"));
            assert!(replies.iter().any(|e| e.kind == "assistant"));
            assert!(
                replies
                    .iter()
                    .all(|e| e.thread_id.as_ref() == Some(thread_id))
            );
            assert_eq!(guard.thread_entries(thread)[0].id, parent.id);
            break;
        }
        assert!(Instant::now() < deadline, "delegate did not settle");
        drop(guard);
        std::thread::sleep(Duration::from_millis(10));
    }

    let _ = std::fs::remove_dir_all(&root);
}

/// Clearing the shared plane empties the timeline and leaves other chats
/// alone.
#[test]
fn clearing_everyone_empties_the_timeline() {
    let events = EventBus::default();
    let state = Arc::new(Mutex::new(ProductState::default()));
    {
        let mut guard = state.lock().unwrap_or_else(|error| error.into_inner());
        for (task, kind) in [("", "user"), ("t-1", "assistant"), ("t-1", "handoff")] {
            guard.transcript.push(TranscriptEntry {
                conversation_id: "everyone".into(),
                kind: kind.into(),
                text: "line".into(),
                task_id: task.into(),
                at: 1,
                speaker: "user".into(),
                speaker_name: "You".into(),
                handoff: None,
                ..Default::default()
            });
        }
        guard.transcript.push(TranscriptEntry {
            conversation_id: "chat-1".into(),
            kind: "user".into(),
            text: "private".into(),
            task_id: "t-2".into(),
            at: 1,
            speaker: "user".into(),
            speaker_name: "You".into(),
            handoff: None,
            ..Default::default()
        });
    }
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let server = MarbleServer::new(MarbleServerParts {
        listener,
        state: Arc::clone(&state),
        store: None,
        approvals: ApprovalInbox::default(),
        events,
        turn_tx: TurnSender::detached(),
        steer_tx: mpsc::channel().0,
        policy: AccessPolicy::new(None),
        cancels: CancelRegistry::default(),
    });

    let (status, body) = server.handle("POST", "/api/everyone/clear", "");
    assert_eq!(status, 200, "{body}");
    assert!(everyone_entries(&state).is_empty());
    let guard = state.lock().unwrap_or_else(|error| error.into_inner());
    assert_eq!(guard.transcript.len(), 1);
    assert_eq!(guard.transcript[0].conversation_id, "chat-1");
}

/// Removing a marble takes its chat, its tasks, and the Everyone lines
/// quoting them — "Remove Ada and her chat" means gone.
#[test]
fn removing_a_marble_deletes_its_chat_and_everyone_lines() {
    let events = EventBus::default();
    let state = Arc::new(Mutex::new(ProductState::default()));
    {
        let mut guard = state.lock().unwrap_or_else(|error| error.into_inner());
        let id = guard
            .prism
            .add("ada", "helpful", "star", 45, "", "")
            .unwrap()
            .id
            .clone();
        guard.conversations.push(glass_marble::ConversationRecord {
            id: "conv-ada".into(),
            title: "ada".into(),
            task_ids: vec![],
            updated_at: 0,
            lane: 0,
            ..Default::default()
        });
        guard.tasks.push(TaskRecord {
            marble_id: String::new(),
            id: "t-ada".into(),
            conversation_id: "conv-ada".into(),
            status: "completed".into(),
            owner: "agent".into(),
            prompt: "hi".into(),
            at: 1,
            routine_id: String::new(),
        });
        if let Some(marble) = guard.prism.marbles.iter_mut().find(|m| m.id == id) {
            marble.conversation_id = "conv-ada".into();
        }
        // The person's stamped line and the marble's reply on Everyone.
        for kind in ["user", "assistant"] {
            guard.transcript.push(TranscriptEntry {
                conversation_id: "everyone".into(),
                kind: kind.into(),
                text: "line".into(),
                task_id: "t-ada".into(),
                at: 1,
                speaker: "user".into(),
                speaker_name: "You".into(),
                handoff: None,
                ..Default::default()
            });
        }
    }
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let server = MarbleServer::new(MarbleServerParts {
        listener,
        state: Arc::clone(&state),
        store: None,
        approvals: ApprovalInbox::default(),
        events,
        turn_tx: TurnSender::detached(),
        steer_tx: mpsc::channel().0,
        policy: AccessPolicy::new(None),
        cancels: CancelRegistry::default(),
    });

    let marble_id = {
        state
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .prism
            .marbles[0]
            .id
            .clone()
    };
    let (status, body) = server.handle(
        "POST",
        &format!("/api/prism/marbles/{marble_id}/remove"),
        "",
    );
    assert_eq!(status, 200, "{body}");
    let guard = state.lock().unwrap_or_else(|error| error.into_inner());
    assert!(guard.prism.get(&marble_id).is_none());
    assert!(guard.conversations.iter().all(|c| c.id != "conv-ada"));
    assert!(guard.tasks.iter().all(|t| t.id != "t-ada"));
    assert!(
        guard.transcript.iter().all(|e| e.task_id != "t-ada"),
        "everyone echoes survived: {:?}",
        guard.transcript
    );
}
