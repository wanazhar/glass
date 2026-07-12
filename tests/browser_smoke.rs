use glass::browser::chrome::detect_chrome;
use glass::browser::session::{
    ActionKind, BrowserSession, InteractionMode, SessionOptions, TargetError, TargetErrorKind,
};
use serde_json::{Value, json};
use std::{
    path::{Path, PathBuf},
    process::Stdio,
    time::{SystemTime, UNIX_EPOCH},
};
use tokio::{
    io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader},
    net::{TcpListener, TcpStream},
    process::{Child, Command},
    sync::oneshot,
};

struct FixtureServer {
    url: String,
    shutdown: Option<oneshot::Sender<()>>,
    task: tokio::task::JoinHandle<()>,
}

impl FixtureServer {
    async fn start(html: &'static str) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let (shutdown, mut shutdown_rx) = oneshot::channel();
        let task = tokio::spawn(async move {
            loop {
                tokio::select! {
                    _ = &mut shutdown_rx => break,
                    accepted = listener.accept() => match accepted {
                        Ok((stream, _)) => {
                            tokio::spawn(serve_fixture(stream, html));
                        }
                        Err(_) => break,
                    },
                }
            }
        });
        Self {
            url: format!("http://{address}/fixture.html"),
            shutdown: Some(shutdown),
            task,
        }
    }

    async fn close(mut self) {
        let _ = self.shutdown.take().unwrap().send(());
        let _ = self.task.await;
    }
}

async fn serve_fixture(mut stream: TcpStream, html: &'static str) {
    let mut request = [0; 4_096];
    let _ = stream.read(&mut request).await;
    let response = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        html.len(),
        html
    );
    let _ = stream.write_all(response.as_bytes()).await;
}

async fn page_target_id(port: u16) -> String {
    let targets: Vec<Value> = reqwest::get(format!("http://127.0.0.1:{port}/json"))
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    targets
        .into_iter()
        .find(|target| target["type"] == "page" && target["webSocketDebuggerUrl"].is_string())
        .and_then(|target| target["id"].as_str().map(str::to_string))
        .expect("Chrome should expose a page target")
}

fn glass_binary_path() -> PathBuf {
    std::env::var_os("CARGO_BIN_EXE_glass")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            let binary_name = if cfg!(windows) { "glass.exe" } else { "glass" };
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("target")
                .join("debug")
                .join(binary_name)
        })
}

async fn mcp_responses(mut child: Child, requests: &[Value]) -> Vec<Value> {
    let input = format!(
        "{}\n",
        requests
            .iter()
            .map(Value::to_string)
            .collect::<Vec<_>>()
            .join("\n")
    );
    child
        .stdin
        .take()
        .unwrap()
        .write_all(input.as_bytes())
        .await
        .unwrap();
    let output = child.wait_with_output().await.unwrap();
    assert!(
        output.status.success(),
        "MCP process failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout)
        .unwrap()
        .lines()
        .map(serde_json::from_str::<Value>)
        .collect::<Result<Vec<_>, _>>()
        .unwrap()
}

#[tokio::test]
async fn mcp_rejects_an_unnegotiated_client_before_tool_use() {
    let binary = glass_binary_path();
    let child = Command::new(binary)
        .arg("--mcp")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let responses = mcp_responses(
        child,
        &[
            json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"private-future-version"}}),
            json!({"jsonrpc":"2.0","id":2,"method":"tools/list"}),
        ],
    )
    .await;
    assert_eq!(responses[0]["error"]["code"], -32602);
    assert_eq!(responses[1]["error"]["code"], -32002);
    assert!(!responses[0].to_string().contains("private-future-version"));
}

#[tokio::test]
async fn mcp_enforces_initialization_lifecycle_and_notification_silence() {
    let binary = glass_binary_path();
    let child = Command::new(binary)
        .arg("--mcp")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let responses = mcp_responses(
        child,
        &[
            json!({"jsonrpc":"2.0","method":"ping"}),
            json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05"}}),
            json!({"jsonrpc":"1.0","method":"notifications/initialized"}),
            json!({"jsonrpc":"2.0","id":2,"method":"tools/list"}),
            json!({"jsonrpc":"2.0","method":"notifications/initialized"}),
            json!({"jsonrpc":"2.0","id":3,"method":"tools/list"}),
            json!({"jsonrpc":"2.0","id":null,"method":"ping"}),
            json!({"jsonrpc":"2.0","id":4,"method":"initialize","params":{"protocolVersion":"2024-11-05"}}),
        ],
    )
    .await;
    assert_eq!(
        responses.len(),
        5,
        "notifications must not produce responses"
    );
    let response = |id: i64| {
        responses
            .iter()
            .find(|response| response["id"] == id)
            .unwrap()
    };
    assert_eq!(response(1)["result"]["serverInfo"]["name"], "glass");
    assert_eq!(response(2)["error"]["code"], -32002);
    assert!(response(3)["result"]["tools"].is_array());
    let null_id = responses
        .iter()
        .find(|response| response["id"].is_null())
        .unwrap();
    assert_eq!(null_id["result"], json!({}));
    assert_eq!(response(4)["error"]["code"], -32600);
}

async fn write_mcp_line(writer: &mut tokio::process::ChildStdin, message: Value) {
    writer
        .write_all(format!("{message}\n").as_bytes())
        .await
        .unwrap();
    writer.flush().await.unwrap();
}

async fn read_mcp_line(reader: &mut BufReader<tokio::process::ChildStdout>) -> Value {
    let mut line = String::new();
    tokio::time::timeout(
        std::time::Duration::from_secs(5),
        reader.read_line(&mut line),
    )
    .await
    .expect("MCP response timed out")
    .unwrap();
    serde_json::from_str(&line).unwrap()
}

#[tokio::test]
async fn mcp_cancellation_interrupts_a_tool_and_preserves_the_session() {
    if std::env::var("GLASS_E2E").as_deref() != Ok("1") {
        eprintln!("skipping browser smoke test; set GLASS_E2E=1 to run it");
        return;
    }
    let Some(chrome_path) = detect_chrome() else {
        eprintln!("skipping browser smoke test; Chrome/Chromium is unavailable");
        return;
    };
    let binary = glass_binary_path();
    let mut port = 20_000 + (std::process::id() % 5_000) as u16;
    loop {
        match TcpListener::bind(("127.0.0.1", port)).await {
            Ok(listener) => {
                drop(listener);
                break;
            }
            Err(_) => port += 1,
        }
    }
    let mut child = Command::new(binary)
        .arg("--mcp")
        .arg("--chrome-path")
        .arg(chrome_path)
        .arg("--incognito")
        .arg("--port")
        .arg(port.to_string())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut stdin = child.stdin.take().unwrap();
    let mut stdout = BufReader::new(child.stdout.take().unwrap());

    write_mcp_line(
        &mut stdin,
        json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05"}}),
    )
    .await;
    assert_eq!(read_mcp_line(&mut stdout).await["id"], 1);
    write_mcp_line(
        &mut stdin,
        json!({"jsonrpc":"2.0","method":"notifications/initialized"}),
    )
    .await;
    write_mcp_line(
        &mut stdin,
        json!({"jsonrpc":"2.0","id":2,"method":"tools/list"}),
    )
    .await;
    assert!(read_mcp_line(&mut stdout).await["result"]["tools"].is_array());
    write_mcp_line(
        &mut stdin,
        json!({"jsonrpc":"2.0","id":5,"method":"tools/call","params":{"name":"evaluate","arguments":{"expression":"0"}}}),
    )
    .await;
    assert_eq!(read_mcp_line(&mut stdout).await["id"], 5);

    write_mcp_line(
        &mut stdin,
        json!({
            "jsonrpc":"2.0",
            "id":3,
            "method":"tools/call",
            "params":{"name":"evaluate","arguments":{"expression":"new Promise(resolve => setTimeout(() => resolve('late'), 60000))"}}
        }),
    )
    .await;
    tokio::time::sleep(std::time::Duration::from_secs(1)).await;
    write_mcp_line(&mut stdin, json!({"jsonrpc":"2.0","id":3,"method":"ping"})).await;
    let duplicate = read_mcp_line(&mut stdout).await;
    assert_eq!(duplicate["id"], 3);
    assert_eq!(duplicate["error"]["code"], -32600);
    assert_eq!(duplicate["error"]["message"], "duplicate active request id");
    write_mcp_line(
        &mut stdin,
        json!({"jsonrpc":"1.0","method":"notifications/cancelled","params":{"requestId":3}}),
    )
    .await;
    assert!(
        tokio::time::timeout(
            std::time::Duration::from_millis(200),
            read_mcp_line(&mut stdout)
        )
        .await
        .is_err(),
        "an invalid JSON-RPC notification must not cancel active work"
    );
    write_mcp_line(
        &mut stdin,
        json!({"jsonrpc":"2.0","method":"notifications/cancelled","params":{"requestId":3,"reason":"private reason must not be logged"}}),
    )
    .await;
    let cancelled = read_mcp_line(&mut stdout).await;
    assert_eq!(cancelled["id"], 3);
    assert_eq!(cancelled["error"]["code"], -32800);
    assert_eq!(cancelled["error"]["message"], "request cancelled");

    write_mcp_line(
        &mut stdin,
        json!({"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"evaluate","arguments":{"expression":"6 * 7"}}}),
    )
    .await;
    let recovered = read_mcp_line(&mut stdout).await;
    assert_eq!(recovered["id"], 4);
    assert_eq!(recovered["result"]["content"][0]["text"], "42");

    let sentinel = "#private-target-token-7319";
    write_mcp_line(
        &mut stdin,
        json!({"jsonrpc":"2.0","id":6,"method":"tools/call","params":{"name":"click","arguments":{"target":sentinel}}}),
    )
    .await;
    let sanitized = read_mcp_line(&mut stdout).await;
    assert_eq!(sanitized["id"], 6);
    let target_error: Value =
        serde_json::from_str(sanitized["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(target_error["kind"], "not_found");
    assert!(!sanitized.to_string().contains(sentinel));

    drop(stdin);
    let output = child.wait_with_output().await.unwrap();
    assert!(
        output.status.success(),
        "MCP cancellation process failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

struct TemporaryProfileHome {
    path: PathBuf,
}

impl TemporaryProfileHome {
    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TemporaryProfileHome {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

fn temporary_profile_home() -> TemporaryProfileHome {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let path =
        std::env::temp_dir().join(format!("glass-e2e-profile-{}-{nonce}", std::process::id()));
    std::fs::create_dir(&path).unwrap();
    TemporaryProfileHome { path }
}

fn named_profile_mcp(
    binary: &Path,
    chrome_path: &Path,
    home: &Path,
    profile: &str,
    port: u16,
) -> Child {
    let port_arg = port.to_string();
    Command::new(binary)
        .env("HOME", home)
        .env("XDG_CONFIG_HOME", home.join("config"))
        .arg("--mcp")
        .arg("--chrome-path")
        .arg(chrome_path)
        .arg("--profile")
        .arg(profile)
        .arg("--port")
        .arg(port_arg)
        .arg("--interaction")
        .arg("fast")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap()
}

#[tokio::test]
async fn concurrent_owned_sessions_on_one_port_do_not_adopt_each_other() {
    if std::env::var("GLASS_E2E").as_deref() != Ok("1") {
        eprintln!("skipping browser smoke test; set GLASS_E2E=1 to run it");
        return;
    }
    let Some(chrome_path) = detect_chrome() else {
        eprintln!("skipping browser smoke test; Chrome/Chromium is unavailable");
        return;
    };

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);

    let first_options = SessionOptions {
        port,
        chrome_path: Some(chrome_path.clone()),
        profile: "e2e-concurrent-first".to_string(),
        incognito: true,
        attach: false,
        target_id: None,
        headed: false,
        interaction_mode: InteractionMode::Fast,
    };
    let second_options = SessionOptions {
        profile: "e2e-concurrent-second".to_string(),
        ..first_options.clone()
    };

    let (first, second) = tokio::join!(
        BrowserSession::start(&first_options),
        BrowserSession::start(&second_options),
    );
    match (first, second) {
        (Ok(session), Err(error)) | (Err(error), Ok(session)) => {
            assert!(
                error.to_string().contains("--attach"),
                "second owned startup should fail explicitly: {error}"
            );
            session.close().await.unwrap();
        }
        (Ok(first), Ok(second)) => {
            first.close().await.unwrap();
            second.close().await.unwrap();
            panic!("only one concurrent owned session may start on one CDP port");
        }
        (Err(first), Err(second)) => {
            panic!("one owned session should start; first error: {first}; second error: {second}");
        }
    }
}

#[tokio::test]
async fn cli_and_mcp_attach_to_a_fixture_with_compact_results() {
    if std::env::var("GLASS_E2E").as_deref() != Ok("1") {
        eprintln!("skipping browser smoke test; set GLASS_E2E=1 to run it");
        return;
    }
    let Some(chrome_path) = detect_chrome() else {
        eprintln!("skipping browser smoke test; Chrome/Chromium is unavailable");
        return;
    };

    let binary = glass_binary_path();
    assert!(
        binary.is_file(),
        "Glass binary is required for frontend integration: {}",
        binary.display()
    );
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);

    let fixture_server = FixtureServer::start(include_str!("fixtures/basic.html")).await;
    let session = BrowserSession::start(&SessionOptions {
        port,
        chrome_path: Some(chrome_path),
        profile: "e2e-frontends".to_string(),
        incognito: true,
        attach: false,
        target_id: None,
        headed: false,
        interaction_mode: InteractionMode::Fast,
    })
    .await
    .unwrap();
    session.navigate(&fixture_server.url).await.unwrap();
    let target_id = page_target_id(port).await;
    let port_arg = port.to_string();

    let cli = Command::new(&binary)
        .args([
            "--attach",
            "--port",
            &port_arg,
            "--target-id",
            &target_id,
            "--interaction",
            "fast",
            "click",
            "Save",
        ])
        .output()
        .await
        .unwrap();
    assert!(
        cli.status.success(),
        "CLI attach failed: {}",
        String::from_utf8_lossy(&cli.stderr)
    );
    let cli_outcome: Value = serde_json::from_slice(&cli.stdout).unwrap();
    assert_eq!(cli_outcome["action"], "click");
    assert!(cli_outcome["revision"].is_u64());

    let mcp = Command::new(&binary)
        .args([
            "--mcp",
            "--attach",
            "--port",
            &port_arg,
            "--target-id",
            &target_id,
            "--interaction",
            "fast",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let responses = mcp_responses(
        mcp,
        &[
            json!({
                "jsonrpc": "2.0",
                "id": 1,
                "method": "initialize",
                "params": {"protocolVersion": "2024-11-05"}
            }),
            json!({
                "jsonrpc": "2.0",
                "method": "notifications/initialized"
            }),
            json!({
                "jsonrpc": "2.0",
                "id": 2,
                "method": "tools/call",
                "params": {"name": "observe", "arguments": {}}
            }),
            json!({
                "jsonrpc": "2.0",
                "id": 3,
                "method": "tools/call",
                "params": {"name": "click", "arguments": {"target": "name=Duplicate"}}
            }),
        ],
    )
    .await;
    assert_eq!(responses[0]["result"]["serverInfo"]["name"], "glass");
    let context_json = responses[1]["result"]["content"][0]["text"]
        .as_str()
        .unwrap();
    let context: Value = serde_json::from_str(context_json).unwrap();
    assert!(context.get("dom").is_none());
    assert!(context.get("screenshot").is_none());
    assert!(context["accessibility"]["interactive"].is_array());
    let target_error: Value = serde_json::from_str(
        responses[2]["result"]["content"][0]["text"]
            .as_str()
            .unwrap(),
    )
    .unwrap();
    assert_eq!(target_error["kind"], "ambiguous");
    assert_eq!(target_error["candidates"].as_array().unwrap().len(), 2);

    session.close().await.unwrap();
    fixture_server.close().await;
}

#[tokio::test]
async fn named_profile_mcp_persists_fixture_storage_between_sessions() {
    if std::env::var("GLASS_E2E").as_deref() != Ok("1") {
        eprintln!("skipping browser smoke test; set GLASS_E2E=1 to run it");
        return;
    }
    let Some(chrome_path) = detect_chrome() else {
        eprintln!("skipping browser smoke test; Chrome/Chromium is unavailable");
        return;
    };

    let binary = glass_binary_path();
    assert!(
        binary.is_file(),
        "Glass binary is required for frontend integration: {}",
        binary.display()
    );
    let home = temporary_profile_home();
    let profile = "persistent";
    let fixture_server = FixtureServer::start(include_str!("fixtures/basic.html")).await;
    let url = fixture_server.url.clone();

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let first_port = listener.local_addr().unwrap().port();
    drop(listener);
    let first_responses = mcp_responses(
        named_profile_mcp(&binary, &chrome_path, home.path(), profile, first_port),
        &[
            json!({
                "jsonrpc": "2.0",
                "id": 1,
                "method": "initialize",
                "params": {"protocolVersion": "2024-11-05"}
            }),
            json!({
                "jsonrpc": "2.0",
                "method": "notifications/initialized"
            }),
            json!({
                "jsonrpc": "2.0",
                "id": 2,
                "method": "tools/call",
                "params": {"name": "navigate", "arguments": {"url": url}}
            }),
            json!({
                "jsonrpc": "2.0",
                "id": 3,
                "method": "tools/call",
                "params": {
                    "name": "evaluate",
                    "arguments": {"expression": "localStorage.setItem('glass-persistent', 'saved')"}
                }
            }),
        ],
    )
    .await;
    assert_eq!(first_responses[0]["result"]["serverInfo"]["name"], "glass");
    assert!(
        home.path()
            .join("config")
            .join("glass")
            .join("profiles")
            .join("data")
            .join(profile)
            .is_dir()
    );

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let second_port = listener.local_addr().unwrap().port();
    drop(listener);
    let second_responses = mcp_responses(
        named_profile_mcp(&binary, &chrome_path, home.path(), profile, second_port),
        &[
            json!({
                "jsonrpc": "2.0",
                "id": 1,
                "method": "initialize",
                "params": {"protocolVersion": "2024-11-05"}
            }),
            json!({
                "jsonrpc": "2.0",
                "method": "notifications/initialized"
            }),
            json!({
                "jsonrpc": "2.0",
                "id": 2,
                "method": "tools/call",
                "params": {"name": "navigate", "arguments": {"url": url}}
            }),
            json!({
                "jsonrpc": "2.0",
                "id": 3,
                "method": "tools/call",
                "params": {
                    "name": "evaluate",
                    "arguments": {"expression": "localStorage.getItem('glass-persistent')"}
                }
            }),
        ],
    )
    .await;
    let persisted = second_responses[2]["result"]["content"][0]["text"]
        .as_str()
        .unwrap();
    assert_eq!(serde_json::from_str::<Value>(persisted).unwrap(), "saved");

    fixture_server.close().await;
}

#[tokio::test]
async fn browser_session_drives_a_local_fixture() {
    if std::env::var("GLASS_E2E").as_deref() != Ok("1") {
        eprintln!("skipping browser smoke test; set GLASS_E2E=1 to run it");
        return;
    }
    let Some(chrome_path) = detect_chrome() else {
        eprintln!("skipping browser smoke test; Chrome/Chromium is unavailable");
        return;
    };

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);

    let fixture_server = FixtureServer::start(include_str!("fixtures/basic.html")).await;
    let url = fixture_server.url.clone();
    let session = BrowserSession::start(&SessionOptions {
        port,
        chrome_path: Some(chrome_path.clone()),
        profile: "e2e".to_string(),
        incognito: true,
        attach: false,
        target_id: None,
        headed: false,
        interaction_mode: InteractionMode::Human,
    })
    .await
    .unwrap();
    assert!(session.owns_chrome());
    assert!(!session.is_attached());

    let owned_error = BrowserSession::start(&SessionOptions {
        port,
        chrome_path: None,
        profile: "e2e-conflict".to_string(),
        incognito: true,
        attach: false,
        target_id: None,
        headed: false,
        interaction_mode: InteractionMode::Fast,
    })
    .await
    .err()
    .expect("an owned session must not adopt an occupied CDP endpoint");
    assert!(owned_error.to_string().contains("--attach"));

    let page = session.navigate(&url).await.unwrap();
    assert_eq!(page.title, "Glass Fixture");
    assert!(session.text().await.unwrap().contains("Glass Fixture"));

    let context = session.observe().await.unwrap();
    assert!(context.dom.is_none());
    assert!(!context.accessibility.interactive.is_empty());
    assert!(context.screenshot.is_none());
    assert!(session.observe_with_dom().await.unwrap().dom.is_some());

    let save_reference = context
        .accessibility
        .interactive
        .iter()
        .find(|element| element.name == "Save")
        .expect("compact context should publish Save")
        .reference
        .clone();
    assert!(save_reference.starts_with('r'));
    let initial_click = session.click(&save_reference).await.unwrap();
    assert_eq!(initial_click.action, ActionKind::Click);
    assert_eq!(
        initial_click
            .target
            .as_ref()
            .and_then(|target| target.reference.as_deref()),
        Some(save_reference.as_str())
    );

    session
        .evaluate("document.body.dataset.glassRevision = 'changed'")
        .await
        .unwrap();
    let stale_error = session.click(&save_reference).await.unwrap_err();
    assert_eq!(
        stale_error.downcast_ref::<TargetError>().unwrap().kind,
        TargetErrorKind::StaleReference
    );

    session
        .evaluate("document.querySelector('#result').textContent = 'Changed'")
        .await
        .unwrap();
    assert!(session.observe().await.unwrap().text.contains("Changed"));

    let snapshot = session.snapshot().await.unwrap();
    assert!(
        snapshot
            .interactive
            .iter()
            .any(|element| element.name == "Save")
    );
    assert!(
        snapshot
            .interactive
            .iter()
            .any(|element| element.name == "Name")
    );

    session.click("text=Unique visible phrase").await.unwrap();
    assert_eq!(
        session
            .evaluate("document.querySelector('#result').textContent")
            .await
            .unwrap(),
        "Text unique clicked"
    );
    assert_eq!(
        session
            .click("text=Repeated phrase")
            .await
            .unwrap_err()
            .downcast_ref::<TargetError>()
            .unwrap()
            .kind,
        TargetErrorKind::Ambiguous
    );
    session.click("text=#save").await.unwrap();
    assert_eq!(
        session
            .evaluate("document.querySelector('#result').textContent")
            .await
            .unwrap(),
        "Selector text clicked"
    );

    session.evaluate("window.pointerEvents = []").await.unwrap();
    let ambiguous = session.click("name=Duplicate").await.unwrap_err();
    assert_eq!(
        ambiguous.downcast_ref::<TargetError>().unwrap().kind,
        TargetErrorKind::Ambiguous
    );
    let ambiguous_css = session.click("css=.duplicate").await.unwrap_err();
    assert_eq!(
        ambiguous_css.downcast_ref::<TargetError>().unwrap().kind,
        TargetErrorKind::Ambiguous
    );
    assert!(
        session
            .click("role=button")
            .await
            .unwrap_err()
            .to_string()
            .contains("role=<role>;name=<accessible name>")
    );
    let disabled = session
        .click("role=button;name=Disabled action")
        .await
        .unwrap_err();
    assert_eq!(
        disabled.downcast_ref::<TargetError>().unwrap().kind,
        TargetErrorKind::NotActionable
    );
    let covered = session.click("css=#covered").await.unwrap_err();
    assert_eq!(
        covered.downcast_ref::<TargetError>().unwrap().kind,
        TargetErrorKind::NotActionable
    );
    session
        .evaluate("document.querySelector('#sticky-covered').style.display = 'block'; document.querySelector('#sticky-cover').style.display = 'block'")
        .await
        .unwrap();
    let sticky = session.click("css=#sticky-covered").await.unwrap_err();
    assert_eq!(
        sticky.downcast_ref::<TargetError>().unwrap().kind,
        TargetErrorKind::NotActionable
    );
    session
        .evaluate("document.querySelector('#sticky-covered').style.display = 'none'; document.querySelector('#sticky-cover').style.display = 'none'")
        .await
        .unwrap();
    let moving = session.click("css=#moving").await.unwrap_err();
    assert_eq!(
        moving.downcast_ref::<TargetError>().unwrap().kind,
        TargetErrorKind::NotActionable
    );
    let hover_reflow = session.click("css=#hover-reflow").await.unwrap_err();
    assert_eq!(
        hover_reflow.downcast_ref::<TargetError>().unwrap().kind,
        TargetErrorKind::NotActionable
    );
    session.evaluate("window.scrollTo(0, 0)").await.unwrap();
    let detached = session.click("css=#detach-on-scroll").await.unwrap_err();
    assert!(
        detached.downcast_ref::<TargetError>().is_some()
            || detached.to_string().contains("Could not find node")
    );
    let failed_pointer_events = session.evaluate("window.pointerEvents").await.unwrap();
    assert!(
        failed_pointer_events
            .as_array()
            .unwrap()
            .iter()
            .all(|event| { event["type"] != "mousedown" && event["type"] != "mouseup" })
    );

    let typed = session.type_text("Ada", Some("Name")).await.unwrap();
    assert_eq!(typed.action, ActionKind::Type);
    session.evaluate("window.pointerEvents = []").await.unwrap();
    let saved = session.click("Save").await.unwrap();
    assert_eq!(saved.action, ActionKind::Click);
    let pointer_events = session.evaluate("window.pointerEvents").await.unwrap();
    let pointer_events = pointer_events.as_array().unwrap();
    assert!(
        pointer_events
            .iter()
            .filter(|event| event["type"] == "mousemove")
            .count()
            > 1
    );
    assert_eq!(
        pointer_events[pointer_events.len() - 2]["type"],
        "mousedown"
    );
    assert_eq!(pointer_events[pointer_events.len() - 1]["type"], "mouseup");
    assert_eq!(
        session
            .evaluate("document.querySelector('#result').textContent")
            .await
            .unwrap(),
        "Saved Ada"
    );
    let double_clicked = session.double_click("Double").await.unwrap();
    assert_eq!(double_clicked.action, ActionKind::DoubleClick);
    assert_eq!(session.evaluate("window.doubleClicks").await.unwrap(), 1);

    session.evaluate("window.scrollTo(0, 0)").await.unwrap();
    let offscreen = session.click("Offscreen Save").await.unwrap();
    assert_eq!(offscreen.action, ActionKind::Click);
    assert!(
        session
            .evaluate("window.scrollY")
            .await
            .unwrap()
            .as_f64()
            .unwrap()
            > 0.0
    );
    assert_eq!(
        session
            .evaluate("document.querySelector('#result').textContent")
            .await
            .unwrap(),
        "Offscreen saved"
    );
    session
        .evaluate("localStorage.setItem('glass-incognito', 'private')")
        .await
        .unwrap();
    let visual_context = session.observe_with_screenshot().await.unwrap();
    assert!(visual_context.screenshot.is_some());
    assert!(session.observe().await.unwrap().screenshot.is_none());
    let screenshot = session.screenshot_png().await.unwrap();
    assert!(screenshot.len() > 100);
    assert!(screenshot.starts_with(b"\x89PNG\r\n\x1a\n"));

    let attached_session = BrowserSession::start(&SessionOptions {
        port,
        chrome_path: None,
        profile: "default".to_string(),
        incognito: false,
        attach: true,
        target_id: Some(page_target_id(port).await),
        headed: false,
        interaction_mode: InteractionMode::Fast,
    })
    .await
    .unwrap();
    assert!(attached_session.is_attached());
    assert!(!attached_session.owns_chrome());
    attached_session.close().await.unwrap();
    assert_eq!(
        session.evaluate("document.title").await.unwrap(),
        "Glass Fixture"
    );

    session.close().await.unwrap();

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);
    let fast_session = BrowserSession::start(&SessionOptions {
        port,
        chrome_path: Some(chrome_path.clone()),
        profile: "e2e-fast".to_string(),
        incognito: true,
        attach: false,
        target_id: None,
        headed: false,
        interaction_mode: InteractionMode::Fast,
    })
    .await
    .unwrap();
    fast_session.navigate(&url).await.unwrap();
    assert_eq!(
        fast_session
            .evaluate("localStorage.getItem('glass-incognito')")
            .await
            .unwrap(),
        Value::Null
    );
    fast_session
        .evaluate("window.pointerEvents = []")
        .await
        .unwrap();
    fast_session.click("Save").await.unwrap();
    let pointer_events = fast_session.evaluate("window.pointerEvents").await.unwrap();
    let pointer_events = pointer_events.as_array().unwrap();
    assert_eq!(
        pointer_events
            .iter()
            .filter(|event| event["type"] == "mousemove")
            .count(),
        1
    );
    assert_eq!(
        pointer_events[pointer_events.len() - 2]["type"],
        "mousedown"
    );
    assert_eq!(pointer_events[pointer_events.len() - 1]["type"], "mouseup");
    fast_session.close().await.unwrap();

    fixture_server.close().await;
}
