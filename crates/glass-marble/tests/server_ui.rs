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
        assert!(
            response.contains("event: hello\ndata: {\"cursor\":1}"),
            "{response}"
        );
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
    let script = script.to_path_buf();
    let factory: Arc<dyn Fn() -> Command + Send + Sync> = Arc::new(move || {
        let mut command = Command::new("python3");
        command.arg(&script).arg("runtime");
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
        store: None,
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
    root: std::path::PathBuf,
    script: std::path::PathBuf,
    state: Arc<Mutex<ProductState>>,
    pool: Arc<glass_marble::PrismPool>,
    server: MarbleServer,
    marble_id: String,
}

impl NamedTurnFixture {
    fn new(label: &str) -> Self {
        Self::with_args(label, Vec::new())
    }

    fn with_args(label: &str, args: Vec<String>) -> Self {
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
        let pool = fixture_prism_pool(&script, &root, &state, &events, args);
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
        Self {
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

impl Drop for NamedTurnFixture {
    fn drop(&mut self) {
        self.pool.shutdown();
        let _ = std::fs::remove_dir_all(&self.root);
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

/// A busy marble does not go silent: the shared timeline says it is busy.
#[test]
fn a_busy_marble_says_so_on_the_shared_plane() {
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
            .add("ben", "careful tester", "circle", 200, "", "")
            .unwrap();
    }
    let pool = fixture_prism_pool(
        &script,
        &root,
        &state,
        &events,
        vec!["--delay-ms".into(), "2500".into()],
    );
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

    let (status, body) = server.handle("POST", "/api/everyone", r#"{"text":"@ben first"}"#);
    assert_eq!(status, 202, "{body}");
    // The first turn is still inside the fixture delay, so the second mention
    // meets a working marble.
    std::thread::sleep(Duration::from_millis(300));
    let (status, body) = server.handle("POST", "/api/everyone", r#"{"text":"@ben second"}"#);
    assert_eq!(status, 202, "{body}");
    assert_eq!(
        serde_json::from_str::<Value>(&body).unwrap()["busy"],
        json!(["ben"]),
        "{body}"
    );
    let entries = everyone_entries(&state);
    assert!(
        entries
            .iter()
            .any(|entry| entry.text.contains("busy with another task")),
        "{entries:?}"
    );

    let _ = std::fs::remove_dir_all(&root);
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
