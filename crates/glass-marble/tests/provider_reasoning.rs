use glass_marble::{
    AccessPolicy, ApprovalInbox, CancelRegistry, EventBus, LiveStore, MarbleServer,
    MarbleServerParts, PrismPool, PrismPoolParts, ProductState, StateStore, TurnSender,
};
use serde_json::{Value, json};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::process::Command;
use std::sync::{Arc, Mutex, mpsc};
use std::time::{Duration, Instant};

#[path = "../../glass-core/tests/fixtures/dietrich_provider.rs"]
mod dietrich;

#[test]
fn real_core_provider_reasoning_is_private_in_named_and_everyone_transcripts() {
    let Some(binary) = std::env::var_os("GLASS_CORE_TEST_BINARY") else {
        eprintln!("Set GLASS_CORE_TEST_BINARY to run the real Core/provider integration.");
        return;
    };
    for case in dietrich::cases() {
        for everyone in [false, true] {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            let base_url = format!("http://{}", listener.local_addr().unwrap());
            let frames = case.frames.clone();
            let provider = std::thread::spawn(move || {
                listener.set_nonblocking(true).unwrap();
                let deadline = Instant::now() + Duration::from_secs(20);
                let mut stream = loop {
                    if let Ok((stream, _)) = listener.accept() {
                        break stream;
                    }
                    assert!(Instant::now() < deadline, "provider not called");
                    std::thread::sleep(Duration::from_millis(10));
                };
                stream
                    .set_read_timeout(Some(Duration::from_secs(10)))
                    .unwrap();
                let mut reader = BufReader::new(stream.try_clone().unwrap());
                let mut length = 0;
                loop {
                    let mut line = String::new();
                    reader.read_line(&mut line).unwrap();
                    if line == "\r\n" {
                        break;
                    }
                    if let Some(value) = line.to_ascii_lowercase().strip_prefix("content-length:") {
                        length = value.trim().parse::<usize>().unwrap();
                    }
                }
                let mut input = vec![0; length];
                reader.read_exact(&mut input).unwrap();
                let mut body = String::new();
                for frame in frames {
                    if let Some(kind) = frame["type"].as_str() {
                        body.push_str(&format!("event: {kind}\n"));
                    }
                    body.push_str(&format!("data: {frame}\n\n"));
                }
                body.push_str("data: [DONE]\n\n");
                write!(stream, "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).unwrap();
            });
            let root = std::env::temp_dir().join(format!(
                "marble-provider-{}-{}-{everyone}",
                std::process::id(),
                case.name
            ));
            std::fs::create_dir_all(root.join("config/glass-core")).unwrap();
            std::fs::write(root.join("config/glass-core/config.toml"), format!(
                "provider = {:?}\nbase_url = {base_url:?}\nmodel = \"fixture\"\napi_key_env = \"DIETRICH_FIXTURE_KEY\"\nadaptive_pacing = false\n", case.provider
            )).unwrap();
            let mut product = ProductState::default();
            product.control.profile = glass_marble::ControlProfile::Unattended;
            let id = product
                .prism
                .add("Dietrich", "writer", "star", 35, "", "")
                .unwrap()
                .id
                .clone();
            let state = Arc::new(Mutex::new(product));
            let events = EventBus::default();
            let store = Arc::new(StateStore::in_data_dir(&root));
            let config = root.join("config");
            let data = root.join("data");
            let binary = binary.clone();
            let pool = Arc::new(PrismPool::new(PrismPoolParts {
                factory: Arc::new(move || {
                    let mut command = Command::new(&binary);
                    command
                        .arg("runtime")
                        .env("XDG_CONFIG_HOME", &config)
                        .env("XDG_DATA_HOME", &data)
                        .env("DIETRICH_FIXTURE_KEY", "fixture-key");
                    command
                }),
                cwd: root.clone(),
                sessions: root.join("sessions"),
                state: state.clone(),
                store: Some(store.clone()),
                live: LiveStore::in_data_dir(&root),
                events: events.clone(),
                cancels: CancelRegistry::default(),
                approvals: ApprovalInbox::default(),
                timeout: Duration::from_secs(20),
            }));
            let server = MarbleServer::new(MarbleServerParts {
                listener: TcpListener::bind("127.0.0.1:0").unwrap(),
                state: state.clone(),
                store: Some(store.clone()),
                approvals: ApprovalInbox::default(),
                events: events.clone(),
                turn_tx: TurnSender::detached(),
                steer_tx: mpsc::channel().0,
                policy: AccessPolicy::new(None),
                cancels: CancelRegistry::default(),
            })
            .with_prism(pool.clone());
            let (status, body) = server.handle(
                "POST",
                if everyone {
                    "/api/everyone"
                } else {
                    "/api/turns"
                },
                &json!({"text":"@Dietrich why are you leaking reasoning?", "marbleId":id})
                    .to_string(),
            );
            assert_eq!(status, 202, "{body}");
            let deadline = Instant::now() + Duration::from_secs(20);
            let snapshot = loop {
                let snapshot = store.load().unwrap();
                if snapshot.tasks.iter().any(|task| task.status == "completed") {
                    break snapshot;
                }
                assert!(
                    !snapshot.tasks.iter().any(|task| task.status == "failed"),
                    "{}: {:?}",
                    case.name,
                    snapshot.tasks
                );
                assert!(
                    Instant::now() < deadline,
                    "{}: no completed turn",
                    case.name
                );
                std::thread::sleep(Duration::from_millis(10));
            };
            let private = snapshot
                .transcript
                .iter()
                .find(|entry| entry.kind == "assistant" && entry.conversation_id != "everyone")
                .unwrap();
            assert_eq!(private.text, dietrich::ANSWER, "{}", case.name);
            let reasoning = snapshot
                .transcript
                .iter()
                .find(|entry| {
                    entry.kind == "reasoning" && entry.conversation_id == private.conversation_id
                })
                .unwrap();
            assert_eq!(reasoning.text, dietrich::PLAN, "{}", case.name);
            if everyone {
                let (status, body) = server.handle("GET", "/api/everyone?limit=50", "");
                assert_eq!(status, 200);
                let value: Value = serde_json::from_str(&body).unwrap();
                let rows = value["entries"].as_array().unwrap();
                assert_eq!(
                    rows.iter().find(|row| row["kind"] == "assistant").unwrap()["text"],
                    dietrich::ANSWER
                );
                assert_eq!(
                    rows.iter().find(|row| row["kind"] == "reasoning").unwrap()["text"],
                    dietrich::PLAN
                );
            }
            let mut visible = String::new();
            for event in events.since(0) {
                if event.task_id == private.task_id
                    && event.conversation_id == private.conversation_id
                {
                    match event.kind.as_str() {
                        "text" => visible.push_str(event.data["text"].as_str().unwrap()),
                        "text_reset" => visible.clear(),
                        _ => {}
                    }
                }
            }
            assert_eq!(visible, dietrich::ANSWER, "{} live stream", case.name);
            pool.shutdown();
            provider.join().unwrap();
            drop(server);
            drop(pool);
            std::fs::remove_dir_all(&root).unwrap();
        }
    }
}
