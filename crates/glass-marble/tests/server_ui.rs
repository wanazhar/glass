use glass_marble::{
    ApprovalInbox, Conversation, HostPolicy, MarbleServer, PendingApproval, ProductState,
    TaskRecord, TranscriptEntry, TurnCommand, run_turn,
};
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::process::Command;
use std::sync::{Arc, Mutex, mpsc};
use std::time::Duration;

fn test_state() -> ProductState {
    let mut state = ProductState::default();
    state.conversations.push(glass_marble::ConversationRecord {
        id: "durable-fixture-1".into(),
        task_ids: vec!["generation-fixture-1".into()],
    });
    state.tasks.push(TaskRecord {
        id: "generation-fixture-1".into(),
        conversation_id: "durable-fixture-1".into(),
        status: "completed".into(),
        owner: "human".into(),
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
    });
    state
}

fn spawn_fake_runtime(root: &std::path::Path) -> std::path::PathBuf {
    let script = root.join("fake_glass_core_runtime.py");
    std::fs::write(&script, include_str!("fixtures/fake_glass_core_runtime.py")).unwrap();
    script
}

fn python3_available() -> bool {
    Command::new("python3")
        .arg("--version")
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

#[test]
fn product_state_encoding_round_trips_through_json() {
    let state = test_state();
    let encoded = state.encode();
    let decoded = ProductState::decode(&encoded).expect("state should decode");
    assert_eq!(state, decoded);
    assert_eq!(
        encoded["conversations"][0]["id"],
        serde_json::json!("durable-fixture-1")
    );
    assert_eq!(
        encoded["conversations"][0]["taskIds"],
        serde_json::json!(["generation-fixture-1"])
    );
}

#[test]
fn server_approval_inbox_routes_allow_and_deny() {
    let approvals = ApprovalInbox::default();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let server = MarbleServer::bind(
        listener,
        Arc::new(Mutex::new(test_state())),
        approvals.clone(),
        mpsc::channel::<TurnCommand>().0,
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
    let (status, _) = server.handle("GET", "/api/nope", "");
    assert_eq!(status, 404);
}

#[test]
fn server_state_route_returns_product_state() {
    let approvals = ApprovalInbox::default();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let server = MarbleServer::bind(
        listener,
        Arc::new(Mutex::new(test_state())),
        approvals.clone(),
        mpsc::channel::<TurnCommand>().0,
    );
    approvals.register(
        "host:fixture-session:generation-fixture-1:call-1",
        "call-1",
        "glass.workspace.list",
        serde_json::json!({}),
    );
    let (status, body) = server.handle("GET", "/api/state", "");
    assert_eq!(status, 200);
    let value: serde_json::Value = serde_json::from_str(&body).expect("state body should be json");
    assert_eq!(value["tasks"][0]["status"], serde_json::json!("completed"));
    assert_eq!(value["tasks"][0]["owner"], serde_json::json!("human"));
    assert_eq!(value["approvals"].as_array().map(Vec::len), Some(1));
    assert_eq!(
        value["transcript"][0]["kind"],
        serde_json::json!("assistant")
    );
}

#[test]
fn server_state_route_serves_pending_approvals_from_live_inbox() {
    let approvals = ApprovalInbox::default();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let server = MarbleServer::bind(
        listener,
        Arc::new(Mutex::new(ProductState::default())),
        approvals.clone(),
        mpsc::channel::<TurnCommand>().0,
    );
    approvals.register(
        "host:fixture-session:generation-fixture-1:call-1",
        "call-1",
        "glass.workspace.list",
        serde_json::json!({}),
    );
    let (status, body) = server.handle("GET", "/api/state", "");
    assert_eq!(status, 200);
    let value: serde_json::Value = serde_json::from_str(&body).expect("state body should be json");
    let pending = value["approvals"].as_array().expect("approvals should be a list");
    assert_eq!(pending.len(), 1);
    assert_eq!(
        pending[0]["requestId"],
        serde_json::json!("host:fixture-session:generation-fixture-1:call-1")
    );
    assert_eq!(pending[0]["tool"], serde_json::json!("glass.workspace.list"));

    let (resolve_status, _) = server.handle(
        "POST",
        "/api/approvals/host:fixture-session:generation-fixture-1:call-1/allow",
        "",
    );
    assert_eq!(resolve_status, 200);
    let (status, body) = server.handle("GET", "/api/state", "");
    assert_eq!(status, 200);
    let value: serde_json::Value = serde_json::from_str(&body).expect("state body should be json");
    assert_eq!(value["approvals"].as_array().map(Vec::len), Some(0));
}

#[test]
fn server_turn_route_runs_durable_turn_and_updates_state() {
    if !python3_available() {
        return;
    }
    let root = std::env::temp_dir().join(format!(
        "glass-marble-{}-{:?}-{}",
        std::process::id(),
        std::thread::current().id(),
        "server-ui-turn"
    ));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    let script = spawn_fake_runtime(&root);
    let storage_dir = root.join("sessions");

    let (turn_tx, turn_rx) = mpsc::channel::<TurnCommand>();
    let mut python = Command::new("python3");
    python.arg(&script).arg("runtime");
    let mut conversation = Conversation::spawn_with_policy(
        python,
        &root,
        &storage_dir,
        HostPolicy::approving(),
        ApprovalInbox::default(),
    )
    .unwrap();
    std::thread::spawn(move || {
        while let Ok(command) = turn_rx.recv() {
            if let Ok(reply) = run_turn(&command.turn, &mut conversation) {
                let _ = command.reply.send(Ok(reply));
            }
        }
    });
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let server = MarbleServer::bind(
        listener,
        Arc::new(Mutex::new(ProductState::default())),
        ApprovalInbox::default(),
        turn_tx,
    );
    let (status, body) = server.handle(
        "POST",
        "/api/turns",
        r#"{"text":"please list the workspace"}"#,
    );
    assert_eq!(status, 200);
    let value: serde_json::Value = serde_json::from_str(&body).expect("turn body should be json");
    assert_eq!(
        value["reply"]["text"],
        serde_json::json!("handled please list the workspace")
    );

    let (status, state_body) = server.handle("GET", "/api/state", "");
    assert_eq!(status, 200);
    let state_value: serde_json::Value =
        serde_json::from_str(&state_body).expect("state should decode");
    assert_eq!(state_value["tasks"].as_array().map(Vec::len), Some(1));
    assert_eq!(state_value["tasks"][0]["owner"], serde_json::json!("human"));
    assert_eq!(state_value["transcript"].as_array().map(Vec::len), Some(2));

    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn server_allow_decision_reaches_inflight_turn() {
    if !python3_available() {
        return;
    }
    let root = std::env::temp_dir().join(format!(
        "glass-marble-{}-{:?}-{}",
        std::process::id(),
        std::thread::current().id(),
        "server-ui-allow-inflight"
    ));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    let script = spawn_fake_runtime(&root);
    let storage_dir = root.join("sessions");

    let approvals = ApprovalInbox::default();
    let (turn_tx, turn_rx) = mpsc::channel::<TurnCommand>();
    let mut python = Command::new("python3");
    python.arg(&script).arg("runtime");
    let mut conversation = Conversation::spawn_with_policy(
        python,
        &root,
        &storage_dir,
        HostPolicy::denying(),
        approvals.clone(),
    )
    .unwrap();
    std::thread::spawn(move || {
        while let Ok(command) = turn_rx.recv() {
            if let Ok(reply) = run_turn(&command.turn, &mut conversation) {
                let _ = command.reply.send(Ok(reply));
            }
        }
    });
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let server = MarbleServer::bind(
        listener,
        Arc::new(Mutex::new(ProductState::default())),
        approvals.clone(),
        turn_tx,
    );

    let (turn_result_tx, turn_result_rx) = mpsc::channel::<(u16, String)>();
    std::thread::scope(|s| {
        s.spawn(|| {
            let (status, body) = server.handle(
                "POST",
                "/api/turns",
                r#"{"text":"please list the workspace"}"#,
            );
            let _ = turn_result_tx.send((status, body));
        });
        let deadline = std::time::Instant::now() + Duration::from_secs(30);
        let pending_request_id = loop {
            if let Some(pending) = approvals.pending().first() {
                break pending.request_id.clone();
            }
            assert!(
                std::time::Instant::now() < deadline,
                "timed out waiting for pending approval"
            );
            std::thread::sleep(Duration::from_millis(25));
        };
        let _ = server.handle(
            "POST",
            &format!("/api/approvals/{pending_request_id}/allow"),
            "",
        );
    });
    let (status, body) = turn_result_rx
        .recv_timeout(Duration::from_secs(30))
        .expect("inflight turn should reply");
    assert_eq!(status, 200);
    let value: serde_json::Value = serde_json::from_str(&body).expect("turn body should be json");
    assert_eq!(
        value["reply"]["text"],
        serde_json::json!("handled please list the workspace")
    );

    let outcome: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(storage_dir.join("fixture-tool-outcome.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(outcome["isError"], serde_json::json!(false));
    assert!(outcome["text"].as_str().unwrap().contains("host approved"));

    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn server_serves_static_page_over_local_socket() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let server = MarbleServer::bind(
        listener,
        Arc::new(Mutex::new(ProductState::default())),
        ApprovalInbox::default(),
        mpsc::channel::<TurnCommand>().0,
    );
    std::thread::spawn(move || {
        let _ = server.serve();
    });
    let mut stream = None;
    for _ in 0..50 {
        if let Ok(candidate) = TcpStream::connect(("127.0.0.1", port)) {
            stream = Some(candidate);
            break;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    let mut stream = stream.expect("local server should accept connections");
    stream
        .write_all(b"GET /api/state HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n")
        .unwrap();
    let mut response = String::new();
    stream.read_to_string(&mut response).unwrap();
    assert!(response.starts_with("HTTP/1.1 200"));
    assert!(response.contains("Content-Type: application/json"));
}
