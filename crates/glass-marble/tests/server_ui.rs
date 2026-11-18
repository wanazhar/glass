use glass_marble::{
    AccessPolicy, ApprovalInbox, Conversation, EventBus, HostPolicy, MarbleServer,
    MarbleServerParts, PendingApproval, ProductState, RequestHead, SteerRequest, TaskRecord,
    ToolRouter, TranscriptEntry, TurnCommand, spawn_turn_worker,
};
use serde_json::{Value, json};
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::process::Command;
use std::sync::{Arc, Mutex, mpsc};
use std::time::{Duration, Instant};

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
    });
    state.tasks.push(TaskRecord {
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
    });
    state.transcript.push(TranscriptEntry {
        conversation_id: "durable-fixture-1".into(),
        kind: "assistant".into(),
        text: "handled please list the workspace".into(),
        task_id: "generation-fixture-1".into(),
        at: 1,
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
        turn_tx: mpsc::channel().0,
        steer_tx: mpsc::channel().0,
        policy,
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
        turn_tx: mpsc::channel().0,
        steer_tx,
        policy: AccessPolicy::new(None),
    })
}

/// Server whose turn channel is kept alive by the caller, so dispatch is observable
/// without a real runtime.
fn server_with_turn_channel(
    state: ProductState,
    turn_tx: mpsc::Sender<TurnCommand>,
) -> MarbleServer {
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
    })
}

fn control_of(server: &MarbleServer) -> serde_json::Value {
    let (status, body) = server.handle("GET", "/api/control", "");
    assert_eq!(status, 200, "{body}");
    serde_json::from_str(&body).unwrap()
}

#[test]
fn paused_admission_queues_messages_and_resume_dispatches_in_order() {
    let (turn_tx, turn_rx) = mpsc::channel();
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
        turn_rx.try_recv().is_err(),
        "a paused instance must not admit turns"
    );

    let (status, body) = server.handle("POST", "/api/control/resume", "");
    assert_eq!(status, 200, "{body}");
    let resumed: serde_json::Value = serde_json::from_str(&body).unwrap();
    assert_eq!(resumed["dispatched"], serde_json::json!(2));
    let first = turn_rx
        .try_recv()
        .expect("first queued message is dispatched");
    let second = turn_rx
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
    );
    approvals.register(
        "host:fixture-session:generation-fixture-1:call-2",
        "call-2",
        "glass.workspace.list",
        serde_json::json!({}),
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
    let (status, _) = server.handle(
        "POST",
        "/api/approvals/host:fixture-session:generation-fixture-1:call-2/allow",
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
    let health = server.handle_request(&RequestHead::new("GET", "/healthz"), "");
    assert_eq!(health.status, 200);
    assert!(health.body.contains("\"ok\":true"));
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
        turn_tx: mpsc::channel().0,
        steer_tx: mpsc::channel().0,
        policy: AccessPolicy::new(None),
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
        turn_tx: mpsc::channel().0,
        steer_tx: mpsc::channel().0,
        policy: AccessPolicy::new(None),
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
    let mut stream = connect(port);
    stream
        .write_all(
            b"GET /api/events?since=0 HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n",
        )
        .unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    let mut response = String::new();
    let mut buffer = [0u8; 4096];
    // The stream stays open by design, so read until the replayed frame has
    // arrived or the read times out.
    while !response.contains("streamed") {
        match stream.read(&mut buffer) {
            Ok(0) => break,
            Ok(read) => response.push_str(&String::from_utf8_lossy(&buffer[..read])),
            Err(_) => break,
        }
    }
    assert!(response.contains("text/event-stream"), "{response}");
    assert!(response.contains("event: text"), "{response}");
    assert!(response.contains("streamed"), "{response}");
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
    let turn_tx = spawn_turn_worker(
        conversation,
        Arc::clone(&state),
        None,
        events.clone(),
        Duration::from_secs(60),
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

    let (status, _) = server.handle("POST", &format!("/api/tasks/{task_id}/cancel"), "");
    assert_eq!(status, 202);

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
    let (turn_tx, turn_rx) = mpsc::channel();
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
        turn_rx.try_recv().is_err(),
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
    let (turn_tx, turn_rx) = mpsc::channel();
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
    let command = turn_rx.try_recv().expect("the routine is admitted");
    assert_eq!(
        command.request.routine_id.as_deref(),
        Some(routine_id.as_str())
    );
    assert_eq!(command.request.text, "tidy the notes");

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
