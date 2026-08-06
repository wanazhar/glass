use glass::browser::chrome::resolve_chrome_path;
use glass::browser::session::{
    ActionKind, BatchStep, BrowserSession, InteractionMode, SemanticObservationLevel,
    SessionOptions, TargetError, TargetErrorKind, VerificationPredicate, WaitCondition,
    WaitTimeout, WorkflowBudgets, WorkflowDefinition, WorkflowOutputDeclaration,
    WorkflowOutputSource, WorkflowRunStatus, WorkflowStep, WorkflowStepState, WorkflowTrace,
    WorkflowTransactionClass,
};
use glass::reliability::{
    ReliabilityFixtureManifest, ReliabilityRunClassification, ReliabilityScenario,
};
use glass::reliability_runner::{ReliabilityRunOptions, run_reliability_scenario};
use glass::{
    GlassTask, TASK_PROTOCOL_SCHEMA_VERSION, TaskAmbiguityPolicy, TaskKind, TaskLimits,
    TaskPostcondition, TaskPostconditionKind, TaskRiskClass, TaskScope,
};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
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
    let read = stream.read(&mut request).await.unwrap_or(0);
    if String::from_utf8_lossy(&request[..read]).starts_with("GET /redirect") {
        let _ = stream
            .write_all(b"HTTP/1.1 302 Found\r\nLocation: /fixture.html\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")
            .await;
        return;
    }
    let response = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        html.len(),
        html
    );
    let _ = stream.write_all(response.as_bytes()).await;
}

async fn reliability_snapshot(session: &BrowserSession) -> Value {
    session
        .evaluate("window.reliabilityLab.snapshot()")
        .await
        .unwrap()
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

fn required_chrome() -> PathBuf {
    resolve_chrome_path(None).expect(
        "GLASS_E2E=1 requires a discoverable Chrome; install the release-pinned browser first",
    )
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
    let chrome_path = required_chrome();
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

    write_mcp_line(
        &mut stdin,
        json!({"jsonrpc":"2.0","id":7,"method":"tools/call","params":{"name":"wait","arguments":{"condition":"js=false","timeoutMs":60000}}}),
    )
    .await;
    tokio::time::sleep(std::time::Duration::from_millis(150)).await;
    write_mcp_line(
        &mut stdin,
        json!({"jsonrpc":"2.0","method":"notifications/cancelled","params":{"requestId":7}}),
    )
    .await;
    let cancelled_wait = read_mcp_line(&mut stdout).await;
    assert_eq!(cancelled_wait["id"], 7);
    assert_eq!(cancelled_wait["error"]["code"], -32800);
    write_mcp_line(
        &mut stdin,
        json!({"jsonrpc":"2.0","id":8,"method":"tools/call","params":{"name":"evaluate","arguments":{"expression":"7 * 6"}}}),
    )
    .await;
    assert_eq!(
        read_mcp_line(&mut stdout).await["result"]["content"][0]["text"],
        "42"
    );

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
#[tokio::test]
async fn mcp_reuses_browser_session_and_target_across_calls() {
    if std::env::var("GLASS_E2E").as_deref() != Ok("1") {
        eprintln!("skipping browser smoke test; set GLASS_E2E=1 to run it");
        return;
    }
    let chrome_path = required_chrome();
    let binary = glass_binary_path();
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);
    let mut child = Command::new(binary)
        .args([
            "--mcp",
            "--chrome-path",
            chrome_path.to_str().unwrap(),
            "--incognito",
            "--port",
            &port.to_string(),
        ])
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
        json!({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"listTargets","arguments":{}}}),
    )
    .await;
    let first_targets = read_mcp_line(&mut stdout).await;
    let first_targets: Value = serde_json::from_str(
        first_targets["result"]["content"][0]["text"]
            .as_str()
            .unwrap(),
    )
    .unwrap();
    let first_target_id = first_targets
        .as_array()
        .unwrap()
        .iter()
        .find(|target| target["active"] == true)
        .and_then(|target| target["id"].as_str())
        .expect("MCP should expose an active target")
        .to_string();

    write_mcp_line(
        &mut stdin,
        json!({"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"evaluate","arguments":{"expression":"window.__glass_session_reuse_marker = 7319"}}}),
    )
    .await;
    assert_eq!(
        read_mcp_line(&mut stdout).await["result"]["content"][0]["text"],
        "7319"
    );

    write_mcp_line(
        &mut stdin,
        json!({"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"listTargets","arguments":{}}}),
    )
    .await;
    let second_targets = read_mcp_line(&mut stdout).await;
    let second_targets: Value = serde_json::from_str(
        second_targets["result"]["content"][0]["text"]
            .as_str()
            .unwrap(),
    )
    .unwrap();
    let second_target_id = second_targets
        .as_array()
        .unwrap()
        .iter()
        .find(|target| target["active"] == true)
        .and_then(|target| target["id"].as_str())
        .expect("MCP should retain an active target")
        .to_string();
    assert_eq!(
        second_target_id, first_target_id,
        "consecutive calls must reuse the same browser target"
    );

    write_mcp_line(
        &mut stdin,
        json!({"jsonrpc":"2.0","id":5,"method":"tools/call","params":{"name":"evaluate","arguments":{"expression":"window.__glass_session_reuse_marker"}}}),
    )
    .await;
    assert_eq!(
        read_mcp_line(&mut stdout).await["result"]["content"][0]["text"],
        "7319"
    );

    drop(stdin);
    let output = child.wait_with_output().await.unwrap();
    assert!(
        output.status.success(),
        "MCP session reuse process failed: {}",
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

fn expected_profile_config_dir(home: &Path) -> PathBuf {
    home.join("config")
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
        .env("GLASS_CONFIG_HOME", home.join("config"))
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
    let chrome_path = required_chrome();

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
        frame_id: None,
        headed: false,
        interaction_mode: InteractionMode::Fast,
        audit: false,
        policy: None,
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
    let chrome_path = required_chrome();

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
        frame_id: None,
        headed: false,
        audit: false,
        policy: None,
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
    let cli_input = Command::new(&binary)
        .args([
            "--attach",
            "--port",
            &port_arg,
            "--target-id",
            &target_id,
            "check",
            "css=#agree",
        ])
        .output()
        .await
        .unwrap();
    assert!(cli_input.status.success());
    assert_eq!(
        serde_json::from_slice::<Value>(&cli_input.stdout).unwrap()["action"],
        "check"
    );

    let workflow_definition = json!({
        "schemaVersion": 1,
        "name": "frontend-workflow",
        "workflowVersion": "1.0.0",
        "inputs": {},
        "budgets": {"maxSteps": 1, "maxDurationMs": 30000, "maxRetries": 0, "maxExtractedBytes": 4096},
        "steps": [{"id": "observe", "action": "observe", "transaction": "read_only"}],
        "terminalCondition": {"titleContains": "Glass Fixture"},
        "outputs": {}
    });
    let mut cli_workflow = Command::new(&binary)
        .args([
            "--attach",
            "--port",
            &port_arg,
            "--target-id",
            &target_id,
            "workflow",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    cli_workflow
        .stdin
        .take()
        .unwrap()
        .write_all(
            json!({"workflow": workflow_definition.clone(), "inputs": {}})
                .to_string()
                .as_bytes(),
        )
        .await
        .unwrap();
    let cli_workflow_output = cli_workflow.wait_with_output().await.unwrap();
    assert!(
        cli_workflow_output.status.success(),
        "CLI workflow failed: {}",
        String::from_utf8_lossy(&cli_workflow_output.stderr)
    );
    let cli_workflow_result: Value = serde_json::from_slice(&cli_workflow_output.stdout).unwrap();
    assert_eq!(cli_workflow_result["status"], "completed");
    assert!(cli_workflow_result["trace"]["events"].is_array());

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
            json!({"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"check","arguments":{"target":"css=#agree"}}}),
            json!({"jsonrpc":"2.0","id":5,"method":"tools/call","params":{"name":"select","arguments":{"target":"css=#choice","value":"b"}}}),
            json!({"jsonrpc":"2.0","id":6,"method":"tools/call","params":{"name":"workflow","arguments":{"workflow":workflow_definition,"inputs":{}}}}),
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
    assert_eq!(responses[3]["result"]["isError"], Value::Null);
    assert_eq!(responses[4]["result"]["isError"], Value::Null);
    let mcp_workflow_text = responses[5]["result"]["content"][0]["text"]
        .as_str()
        .unwrap();
    let mcp_workflow_result: Value = serde_json::from_str(mcp_workflow_text).unwrap();
    assert_eq!(mcp_workflow_result["status"], "completed");
    assert!(mcp_workflow_result["trace"]["events"].is_array());

    session.close().await.unwrap();
    fixture_server.close().await;
}

#[tokio::test]
async fn named_profile_mcp_persists_fixture_storage_between_sessions() {
    if std::env::var("GLASS_E2E").as_deref() != Ok("1") {
        eprintln!("skipping browser smoke test; set GLASS_E2E=1 to run it");
        return;
    }
    if cfg!(target_os = "macos") && std::env::var("GITHUB_ACTIONS").as_deref() == Ok("true") {
        eprintln!(
            "skipping persistent-profile smoke test on GitHub-hosted macOS; the runner's CDP is too slow for this bounded scenario"
        );
        return;
    }
    let chrome_path = required_chrome();

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
                "params": {"name": "navigate", "arguments": {"url": url, "timeoutMs": 300000, "includeTrace": true}}
            }),
            json!({
                "jsonrpc": "2.0",
                "id": 3,
                "method": "tools/call",
                "params": {
                    "name": "evaluate",
                    "arguments": {"expression": "new Promise(resolve => { localStorage.setItem('glass-persistent', 'saved'); setTimeout(() => resolve(localStorage.getItem('glass-persistent')), 500) })"}
                }
            }),
        ],
    )
    .await;
    assert_eq!(first_responses[0]["result"]["serverInfo"]["name"], "glass");
    assert!(
        expected_profile_config_dir(home.path())
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
                "params": {"name": "navigate", "arguments": {"url": url, "timeoutMs": 300000, "includeTrace": true}}
            }),
            json!({
                "jsonrpc": "2.0",
                "id": 3,
                "method": "tools/call",
                "params": {
                    "name": "evaluate",
                    "arguments": {"expression": "localStorage.getItem('glass-persistent')", "includeTrace": true}
                }
            }),
        ],
    )
    .await;
    let persisted_text = second_responses[2]["result"]["content"][0]["text"]
        .as_str()
        .unwrap_or_else(|| {
            panic!("second-session evaluate response was not text: {second_responses:?}")
        });
    let persisted = serde_json::from_str::<Value>(persisted_text).unwrap_or_else(|error| {
        panic!("second-session evaluate payload was invalid JSON ({error}): {second_responses:?}")
    });
    assert_eq!(
        persisted, "saved",
        "second-session evaluate response: {second_responses:?}"
    );

    fixture_server.close().await;
}

#[tokio::test]
async fn browser_session_drives_a_local_fixture() {
    if std::env::var("GLASS_E2E").as_deref() != Ok("1") {
        eprintln!("skipping browser smoke test; set GLASS_E2E=1 to run it");
        return;
    }
    if cfg!(target_os = "macos")
        && cfg!(target_arch = "x86_64")
        && std::env::var("GITHUB_ACTIONS").as_deref() == Ok("true")
    {
        eprintln!(
            "skipping local-fixture smoke test on GitHub-hosted Intel macOS; the runner's CDP is too slow for this bounded scenario"
        );
        return;
    }
    let chrome_path = required_chrome();

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
        frame_id: None,
        audit: false,
        policy: None,
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
        audit: false,
        policy: None,
        frame_id: None,
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
    let redirected = session
        .navigate(&fixture_server.url.replace("/fixture.html", "/redirect"))
        .await
        .unwrap();
    assert!(redirected.url.ends_with("/fixture.html"));
    session
        .verify(
            VerificationPredicate::All {
                all: vec![
                    VerificationPredicate::TitleContains {
                        value: "Glass Fixture".to_string(),
                    },
                    VerificationPredicate::UrlEquals {
                        value: redirected.url.clone(),
                    },
                ],
            },
            std::time::Duration::from_secs(1),
        )
        .await
        .unwrap();
    let workflow = WorkflowDefinition {
        schema_version: 1,
        name: "save-fixture-result".into(),
        workflow_version: "1.0.0".into(),
        description: None,
        inputs: BTreeMap::new(),
        budgets: WorkflowBudgets {
            max_steps: 2,
            max_duration_ms: 10_000,
            max_retries: 0,
            max_extracted_bytes: 4_096,
        },
        preconditions: vec![],
        steps: vec![WorkflowStep {
            id: "save".into(),
            action: BatchStep::Observe {
                include_dom: false,
                include_screenshot: false,
                include_form_values: false,
            },
            intent: None,
            when: None,
            expect: Some(VerificationPredicate::TitleContains {
                value: "Glass Fixture".into(),
            }),
            before_retry: None,
            transaction: WorkflowTransactionClass::ReadOnly,
            idempotency_key: None,
            max_retries: 0,
            repeat: 2,
        }],
        terminal_condition: VerificationPredicate::TitleContains {
            value: "Glass Fixture".into(),
        },
        outputs: BTreeMap::from([(
            "title".into(),
            WorkflowOutputDeclaration {
                value_type: glass::browser::session::WorkflowValueType::String,
                source: WorkflowOutputSource::PageTitle,
                required: true,
                sensitive: false,
            },
        )]),
    };
    let workflow_result = session
        .run_workflow(&workflow, &BTreeMap::new())
        .await
        .unwrap();
    assert_eq!(workflow_result.status, WorkflowRunStatus::Completed);
    assert_eq!(workflow_result.steps[0].state, WorkflowStepState::Committed);
    assert_eq!(workflow_result.steps[0].attempts, 2);
    assert_eq!(
        workflow_result.outputs["title"].value,
        json!("Glass Fixture")
    );

    let mut marker_workflow = workflow.clone();
    marker_workflow.budgets.max_retries = 1;
    marker_workflow.steps[0].action = BatchStep::Click {
        target: "css=#missing-marker-target".into(),
    };
    marker_workflow.steps[0].expect = None;
    marker_workflow.steps[0].max_retries = 1;
    marker_workflow.steps[0].repeat = 1;
    marker_workflow.steps[0].before_retry = Some(VerificationPredicate::TitleContains {
        value: "Glass Fixture".into(),
    });
    let marker_result = session
        .run_workflow(&marker_workflow, &BTreeMap::new())
        .await
        .unwrap();
    assert_eq!(marker_result.status, WorkflowRunStatus::Completed);
    assert_eq!(marker_result.steps[0].state, WorkflowStepState::Committed);
    assert_eq!(marker_result.steps[0].attempts, 1);
    assert!(marker_result.steps[0].execution_ids.is_empty());
    assert!(marker_result.steps[0].effect_observed);

    let workflow_checkpoint = session
        .export_workflow_checkpoint(&workflow, &workflow_result)
        .await
        .unwrap();
    let checkpoint_json = workflow_checkpoint.to_canonical_json().unwrap();
    let parsed_checkpoint = BrowserSession::parse_workflow_checkpoint(&checkpoint_json).unwrap();
    let resume_plan = session
        .reconcile_workflow_checkpoint(&workflow, &parsed_checkpoint)
        .await
        .unwrap();
    assert_eq!(resume_plan.next_step_index, 1);
    assert!(resume_plan.reconciled);

    let mut resumable_workflow = workflow.clone();
    resumable_workflow.budgets.max_steps = 2;
    resumable_workflow.steps[0].repeat = 1;
    resumable_workflow.steps.push(WorkflowStep {
        id: "recover-duplicate".into(),
        action: BatchStep::Click {
            target: "name=Duplicate".into(),
        },
        intent: None,
        when: None,
        expect: None,
        before_retry: None,
        transaction: WorkflowTransactionClass::Idempotent,
        idempotency_key: None,
        max_retries: 0,
        repeat: 1,
    });
    let failed_resume = session
        .run_workflow(&resumable_workflow, &BTreeMap::new())
        .await
        .unwrap();
    assert_eq!(failed_resume.status, WorkflowRunStatus::Failed);
    assert_eq!(failed_resume.steps.len(), 2);
    let failed_checkpoint = session
        .export_workflow_checkpoint(&resumable_workflow, &failed_resume)
        .await
        .unwrap();
    assert_eq!(failed_checkpoint.next_step_index, 1);
    session
        .evaluate("document.querySelectorAll('.duplicate')[1]?.remove()")
        .await
        .unwrap();
    let resumed = session
        .resume_workflow(&resumable_workflow, &BTreeMap::new(), &failed_checkpoint)
        .await
        .unwrap();
    assert_eq!(resumed.status, WorkflowRunStatus::Completed);
    assert_ne!(resumed.run_id, failed_resume.run_id);
    assert_eq!(resumed.steps.len(), resumable_workflow.steps.len());
    assert_eq!(resumed.steps[0].state, WorkflowStepState::Committed);
    assert_eq!(resumed.steps[1].state, WorkflowStepState::Committed);
    assert!(WorkflowTrace::replay(&resumed.trace, &resumable_workflow).is_ok());
    let resumed_checkpoint = session
        .export_workflow_checkpoint(&resumable_workflow, &resumed)
        .await
        .unwrap();
    assert_eq!(resumed_checkpoint.next_step_index, 2);
    session
        .evaluate("document.body.insertAdjacentHTML('beforeend', '<button class=\\\"duplicate\\\">Duplicate</button>')")
        .await
        .unwrap();
    for condition in [
        WaitCondition::Lifecycle("complete".to_string()),
        WaitCondition::UrlExact(redirected.url.clone()),
        WaitCondition::TargetAttached("name=Save".to_string()),
        WaitCondition::TargetVisible("name=Save".to_string()),
        WaitCondition::TargetHidden("css=#sticky-covered".to_string()),
        WaitCondition::TargetEnabled("name=Save".to_string()),
        WaitCondition::JavaScript("true".to_string()),
    ] {
        session
            .wait(condition, std::time::Duration::from_secs(1))
            .await
            .unwrap();
    }
    let (quiet_a, quiet_b) = tokio::join!(
        session.wait(
            WaitCondition::NetworkQuiet(std::time::Duration::from_millis(80)),
            std::time::Duration::from_secs(1),
        ),
        session.wait(
            WaitCondition::NetworkQuiet(std::time::Duration::from_millis(100)),
            std::time::Duration::from_secs(1),
        )
    );
    quiet_a.unwrap();
    quiet_b.unwrap();
    let (cancelled_lease, surviving_lease) = tokio::join!(
        tokio::time::timeout(
            std::time::Duration::from_millis(30),
            session.wait(
                WaitCondition::NetworkQuiet(std::time::Duration::from_millis(500)),
                std::time::Duration::from_secs(1),
            )
        ),
        session.wait(
            WaitCondition::NetworkQuiet(std::time::Duration::from_millis(120)),
            std::time::Duration::from_secs(1),
        )
    );
    assert!(cancelled_lease.is_err());
    surviving_lease.unwrap();
    session
        .wait(
            WaitCondition::NetworkQuiet(std::time::Duration::from_millis(80)),
            std::time::Duration::from_secs(1),
        )
        .await
        .unwrap();
    session
        .evaluate(&format!(
            "window.waitPulse = setInterval(() => fetch('{}?pulse=' + Date.now()), 20)",
            fixture_server.url
        ))
        .await
        .unwrap();
    let never_idle = session
        .wait(
            WaitCondition::NetworkQuiet(std::time::Duration::from_millis(100)),
            std::time::Duration::from_millis(350),
        )
        .await;
    session
        .evaluate("clearInterval(window.waitPulse)")
        .await
        .unwrap();
    assert!(
        never_idle
            .unwrap_err()
            .downcast_ref::<WaitTimeout>()
            .is_some()
    );

    session
        .evaluate("setTimeout(() => { const node = document.createElement('p'); node.textContent = 'Delayed wait content'; document.body.append(node); }, 120)")
        .await
        .unwrap();
    let waited = session
        .wait(
            WaitCondition::Text("Delayed wait content".to_string()),
            std::time::Duration::from_secs(2),
        )
        .await
        .unwrap();
    assert_eq!(waited.condition, "text");
    session
        .evaluate("document.body.insertAdjacentHTML('beforeend', '<div style=\"opacity:0\">Invisible wait only</div><div style=\"width:0;height:0;overflow:hidden\">Clipped wait only</div>')")
        .await
        .unwrap();
    for hidden_text in ["Invisible wait only", "Clipped wait only"] {
        assert!(
            session
                .wait(
                    WaitCondition::Text(hidden_text.to_string()),
                    std::time::Duration::from_millis(120),
                )
                .await
                .unwrap_err()
                .downcast_ref::<WaitTimeout>()
                .is_some()
        );
    }
    session
        .evaluate("setTimeout(() => history.pushState({}, '', '#wait-spa'), 80)")
        .await
        .unwrap();
    session
        .wait(
            WaitCondition::UrlPrefix(format!("{}#wait", fixture_server.url)),
            std::time::Duration::from_secs(2),
        )
        .await
        .unwrap();
    session
        .wait(
            WaitCondition::TargetStable("name=Save".to_string()),
            std::time::Duration::from_secs(2),
        )
        .await
        .unwrap();
    let timeout = session
        .wait(
            WaitCondition::JavaScript("false".to_string()),
            std::time::Duration::from_millis(120),
        )
        .await
        .unwrap_err();
    let timeout = timeout.downcast_ref::<WaitTimeout>().unwrap();
    assert_eq!(timeout.reason, "deadline_exceeded");
    assert!(timeout.last_state.len() <= 512);
    assert!(timeout.observed_page.is_some());
    let cancelled = tokio::time::timeout(
        std::time::Duration::from_millis(100),
        session.wait(
            WaitCondition::JavaScript("false".to_string()),
            std::time::Duration::from_secs(10),
        ),
    )
    .await;
    assert!(cancelled.is_err());
    assert_eq!(session.evaluate("6 * 7").await.unwrap(), 42);
    let started = std::time::Instant::now();
    let pending_timeout = session
        .wait(
            WaitCondition::JavaScript("new Promise(() => {})".to_string()),
            std::time::Duration::from_millis(120),
        )
        .await
        .unwrap_err();
    assert!(pending_timeout.downcast_ref::<WaitTimeout>().is_some());
    assert!(started.elapsed() < std::time::Duration::from_secs(1));

    session
        .evaluate(
            "(() => { const host=document.createElement('div'); host.attachShadow({mode:'open'}).innerHTML='<button>Shadow action</button>'; document.body.append(host, document.createElement('canvas')); const frame=document.createElement('iframe'); frame.srcdoc='<p>child frame</p>'; document.body.append(frame); return true; })()",
        )
        .await
        .unwrap();
    let context = session.observe().await.unwrap();
    assert!(context.dom.is_none());
    assert!(!context.accessibility.interactive.is_empty());
    assert!(context.screenshot.is_none());
    assert!(context.consistency.consistent);
    assert_eq!(context.boundaries.shadow_roots, 1);
    assert_eq!(context.boundaries.child_frames, 1);
    assert_eq!(context.boundaries.canvases, 1);
    assert!(
        context
            .incomplete
            .contains(&glass::browser::session::ObservationIncompleteReason::ShadowBoundary)
    );

    let clip_capture = session
        .capture_visual(&glass::browser::session::VisualCaptureOptions {
            clip: Some(glass::browser::session::VisualClip {
                x: 0.0,
                y: 0.0,
                width: 120.0,
                height: 80.0,
            }),
            scale: 2.0,
            ..Default::default()
        })
        .await
        .unwrap();
    assert_eq!(
        (clip_capture.metadata.width, clip_capture.metadata.height),
        (240, 160)
    );
    assert!(clip_capture.metadata.encoded_bytes > 0);
    let viewport_capture = session.capture_visual(&Default::default()).await.unwrap();
    assert!(viewport_capture.metadata.width > 0 && viewport_capture.metadata.height > 0);
    session
        .raw_cdp()
        .unwrap()
        .send(
            "Emulation.setDeviceMetricsOverride",
            Some(json!({"width":400,"height":300,"deviceScaleFactor":2,"mobile":false})),
        )
        .await
        .unwrap();
    let hidpi_capture = session.capture_visual(&Default::default()).await.unwrap();
    assert_eq!(
        (hidpi_capture.metadata.width, hidpi_capture.metadata.height),
        (800, 600)
    );
    assert_eq!(hidpi_capture.metadata.device_scale_factor, 2.0);
    session
        .raw_cdp()
        .unwrap()
        .send("Emulation.clearDeviceMetricsOverride", None)
        .await
        .unwrap();
    let jpeg_capture = session
        .capture_visual(&glass::browser::session::VisualCaptureOptions {
            format: glass::browser::session::VisualFormat::Jpeg,
            quality: Some(80),
            clip: Some(glass::browser::session::VisualClip {
                x: 10.0,
                y: 10.0,
                width: 64.0,
                height: 48.0,
            }),
            ..Default::default()
        })
        .await
        .unwrap();
    assert_eq!(
        (jpeg_capture.metadata.width, jpeg_capture.metadata.height),
        (64, 48)
    );
    let webp_capture = session
        .capture_visual(&glass::browser::session::VisualCaptureOptions {
            format: glass::browser::session::VisualFormat::Webp,
            quality: Some(80),
            clip: Some(glass::browser::session::VisualClip {
                x: 10.0,
                y: 10.0,
                width: 64.0,
                height: 48.0,
            }),
            ..Default::default()
        })
        .await
        .unwrap();
    assert_eq!(
        (webp_capture.metadata.width, webp_capture.metadata.height),
        (64, 48)
    );
    session
        .evaluate("document.body.style.minHeight='2000px'; scrollTo(0, 150); true")
        .await
        .unwrap();
    let scaled_viewport = session
        .capture_visual(&glass::browser::session::VisualCaptureOptions {
            scale: 1.5,
            ..Default::default()
        })
        .await
        .unwrap();
    assert!(scaled_viewport.metadata.clip.unwrap().y >= 100.0);
    assert_eq!(
        scaled_viewport.metadata.width,
        (scaled_viewport.metadata.clip.unwrap().width * 1.5) as usize
    );
    let full_capture = session
        .capture_visual(&glass::browser::session::VisualCaptureOptions {
            full_page: true,
            ..Default::default()
        })
        .await
        .unwrap();
    assert!(full_capture.metadata.width >= clip_capture.metadata.width);
    assert!(full_capture.metadata.height >= clip_capture.metadata.height);
    let element_capture = session
        .capture_visual(&glass::browser::session::VisualCaptureOptions {
            target: Some("name=Save".to_string()),
            ..Default::default()
        })
        .await
        .unwrap();
    assert!(element_capture.metadata.width > 0 && element_capture.metadata.height > 0);
    assert_eq!(element_capture.metadata.target_id, context.page.target_id);
    let mut screencast = session
        .start_screencast(glass::browser::session::VisualFormat::Jpeg, 70, 640, 480)
        .await
        .unwrap();
    session
        .evaluate("document.body.style.backgroundColor='rgb(1, 2, 3)'; true")
        .await
        .unwrap();
    let frame = tokio::time::timeout(std::time::Duration::from_secs(2), screencast.next_frame())
        .await
        .unwrap()
        .unwrap();
    assert!(!frame.data.is_empty());
    let stream_stats = screencast.stop().await.unwrap();
    assert!(stream_stats.received >= 1);
    assert!(
        context
            .incomplete
            .contains(&glass::browser::session::ObservationIncompleteReason::FrameBoundary)
    );
    assert!(
        context
            .incomplete
            .contains(&glass::browser::session::ObservationIncompleteReason::Canvas)
    );

    session
        .evaluate(&format!(
            "setTimeout(() => {{ console.error('diagnostic-boom'); fetch('http://127.0.0.1:1/fail?token=secret').catch(() => {{}}); fetch({}, {{headers:{{Authorization:'Bearer secret'}}}}); }}, 50); true",
            serde_json::to_string(
                &fixture_server
                    .url
                    .replace("/fixture.html", "/redirect?credential=secret")
            )
            .unwrap()
        ))
        .await
        .unwrap();
    let diagnostics = session
        .diagnostics(std::time::Duration::from_millis(300))
        .await
        .unwrap();
    assert!(
        diagnostics
            .console
            .iter()
            .any(|entry| entry.level == "error" && entry.text.contains("redacted"))
    );
    assert!(
        diagnostics
            .network
            .iter()
            .any(|entry| entry.redirect_count > 0)
    );
    assert!(
        diagnostics
            .network
            .iter()
            .any(|entry| entry.failure.is_some())
    );
    let diagnostic_json = serde_json::to_string(&diagnostics).unwrap();
    assert!(!diagnostic_json.contains("secret"));

    session
        .evaluate("setTimeout(() => alert('bounded dialog'), 20); true")
        .await
        .unwrap();
    // Poll for the dialog instead of a fixed sleep to handle timing
    // variability across different CI runners.
    for _ in 0..25 {
        if session.dismiss_dialog().await.is_ok() {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    session
        .evaluate("setTimeout(() => confirm('accept dialog'), 20); true")
        .await
        .unwrap();
    for _ in 0..25 {
        if session.accept_dialog().await.is_ok() {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }

    let download_dir = std::env::current_dir()
        .unwrap()
        .join(format!("glass-download-test-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&download_dir);
    std::fs::create_dir_all(&download_dir).unwrap();
    session
        .evaluate("setTimeout(() => { const a=document.createElement('a'); a.href='data:text/plain,glass-download'; a.download='glass.txt'; a.click(); }, 50); true")
        .await
        .unwrap();
    let download = session
        .wait_for_download(&download_dir, std::time::Duration::from_secs(5))
        .await
        .unwrap();
    assert!(matches!(download.state.as_str(), "completed" | "canceled"));
    assert_eq!(download.suggested_filename, "glass.txt");
    if download.state == "completed" {
        assert_eq!(download.received_bytes, 14);
    }
    let _ = std::fs::remove_dir_all(download_dir);
    assert!(session.observe_with_dom().await.unwrap().dom.is_some());
    let context = session.observe_fresh().await.unwrap();

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

    let (revision, reference_tail) = save_reference
        .split_once(":c")
        .expect("context-bound references should include a context segment");
    let (_, backend_id) = reference_tail
        .split_once(':')
        .expect("context-bound references should include a backend node");
    let cross_context_reference =
        format!("{revision}:cother:b{}", backend_id.trim_start_matches('b'));
    let cross_context_error = session
        .click(&cross_context_reference)
        .await
        .expect_err("references from another page context must be rejected");
    assert_eq!(
        cross_context_error
            .downcast_ref::<TargetError>()
            .unwrap()
            .kind,
        TargetErrorKind::StaleReference
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
        .evaluate("globalThis.glassRaceHost=document.body.appendChild(document.createElement('div')); for(let i=0;i<1000;i++){const node=document.createElement('span');node.textContent='race-'+i;globalThis.glassRaceHost.appendChild(node)}; globalThis.glassMutationTimer=setInterval(() => { for(let i=0;i<50;i++){const node=globalThis.glassRaceHost.firstChild;node.textContent=String(performance.now());globalThis.glassRaceHost.appendChild(node)} document.body.dataset.race=String(performance.now()) }, 0); true")
        .await
        .unwrap();
    let raced = session.observe().await.unwrap();
    assert!(!raced.consistency.consistent);
    assert_eq!(raced.consistency.attempts, 2);
    assert!(
        raced
            .incomplete
            .contains(&glass::browser::session::ObservationIncompleteReason::MutationRace)
    );
    session
        .evaluate(
            "clearInterval(globalThis.glassMutationTimer); globalThis.glassRaceHost.remove(); delete globalThis.glassRaceHost; delete document.body.dataset.race; true",
        )
        .await
        .unwrap();
    tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    let stable = session.observe().await.unwrap();
    assert!(stable.consistency.consistent);

    let rss_before_huge = process_rss_bytes();
    session
        .evaluate("document.querySelector('#result').textContent='界'.repeat(200000)")
        .await
        .unwrap();
    let huge = session.observe().await.unwrap();
    let huge_json = serde_json::to_vec(&huge).unwrap();
    assert!(
        huge.incomplete
            .contains(&glass::browser::session::ObservationIncompleteReason::VisibleText)
    );
    assert!(huge.text.len() <= glass::browser::session::COMPACT_TEXT_MAX_BYTES);
    assert!(huge_json.len() < 32 * 1024);
    if let (Some(before), Some(after)) = (rss_before_huge, process_rss_bytes()) {
        assert!(after.saturating_sub(before) < 64 * 1024 * 1024);
    }

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
    let ambiguous_css = ambiguous_css.downcast_ref::<TargetError>().unwrap();
    assert_eq!(ambiguous_css.kind, TargetErrorKind::Ambiguous);
    assert!(
        ambiguous_css
            .candidates
            .iter()
            .all(|candidate| candidate.label.starts_with("css match"))
    );
    assert!(
        !serde_json::to_string(ambiguous_css)
            .unwrap()
            .contains(".duplicate")
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
    session.hover("css=#save").await.unwrap();
    session.clear("css=#name").await.unwrap();
    assert_eq!(
        session
            .evaluate("document.querySelector('#name').value")
            .await
            .unwrap(),
        ""
    );
    session.click("css=#editable").await.unwrap();
    session.key_press("x").await.unwrap();
    assert!(
        session
            .evaluate("document.querySelector('#editable').textContent.includes('x')")
            .await
            .unwrap()
            .as_bool()
            .unwrap()
    );
    session.shortcut("Control+A").await.unwrap();
    let key_events = session.evaluate("window.keyEvents").await.unwrap();
    let x_events: Vec<_> = key_events
        .as_array()
        .unwrap()
        .iter()
        .filter(|event| event["key"] == "x")
        .map(|event| event["type"].as_str().unwrap())
        .collect();
    assert_eq!(x_events, ["keydown", "keypress", "keyup"]);
    assert!(
        key_events
            .as_array()
            .unwrap()
            .iter()
            .any(|event| event["ctrl"] == true)
    );
    session.type_text("Par", Some("css=#city")).await.unwrap();
    session.key_press("ArrowDown").await.unwrap();
    session.key_press("Enter").await.unwrap();
    assert!(
        session
            .evaluate("document.querySelector('#city').value.startsWith('Par')")
            .await
            .unwrap()
            .as_bool()
            .unwrap()
    );
    session.check("css=#agree").await.unwrap();
    assert_eq!(
        session
            .evaluate("document.querySelector('#agree').checked")
            .await
            .unwrap(),
        true
    );
    session.uncheck("css=#agree").await.unwrap();
    assert_eq!(
        session
            .evaluate("document.querySelector('#agree').checked")
            .await
            .unwrap(),
        false
    );
    session.select_option("css=#choice", "b").await.unwrap();
    assert_eq!(
        session
            .evaluate("document.querySelector('#choice').value")
            .await
            .unwrap(),
        "b"
    );
    session
        .drag("css=#drag-source", "css=#drag-target")
        .await
        .unwrap();
    assert_eq!(
        session
            .evaluate("document.querySelector('#drag-target').dataset.dropped")
            .await
            .unwrap(),
        "yes"
    );
    assert!(
        session
            .evaluate("window.pointerEvents.some(event => event.type === 'mouseup')")
            .await
            .unwrap()
            .as_bool()
            .unwrap()
    );
    let upload_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/basic.html");
    session
        .upload_files("css=#upload", std::slice::from_ref(&upload_path))
        .await
        .unwrap();
    assert_eq!(
        session
            .evaluate("document.querySelector('#upload').files.length")
            .await
            .unwrap(),
        1
    );
    session.type_text("Ada", Some("Name")).await.unwrap();
    session.evaluate("window.pointerEvents = []").await.unwrap();
    session.click("Save").await.unwrap();
    let pointer_events = session.evaluate("window.pointerEvents").await.unwrap();
    let pointer_events = pointer_events.as_array().unwrap();
    assert!(
        pointer_events
            .iter()
            .filter(|event| event["type"] == "mousemove")
            .count()
            > 1,
        "expected human motion samples: {pointer_events:?}"
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

    session.evaluate("window.pointerEvents = []").await.unwrap();
    let mut cancelled_click = Box::pin(session.click("css=#cancel-release"));
    loop {
        tokio::select! {
            result = &mut cancelled_click => panic!("click completed before cancellation: {result:?}"),
            _ = tokio::time::sleep(std::time::Duration::from_millis(10)) => {
                let events = session.evaluate("window.pointerEvents").await.unwrap();
                if events.as_array().unwrap().iter().any(|event| event["type"] == "mousedown") {
                    break;
                }
            }
        }
    }
    drop(cancelled_click);
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    let cancellation_events = session.evaluate("window.pointerEvents").await.unwrap();
    assert!(
        cancellation_events
            .as_array()
            .unwrap()
            .iter()
            .any(|event| event["type"] == "mouseup"),
        "dropping an in-progress click must release the pressed button"
    );

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
        audit: false,
        policy: None,
        target_id: Some(page_target_id(port).await),
        frame_id: None,
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
        audit: false,
        policy: None,
        attach: false,
        target_id: None,
        frame_id: None,
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

#[tokio::test]
async fn reliability_lab_controls_produce_independent_oracle_state() {
    if std::env::var("GLASS_E2E").as_deref() != Ok("1") {
        eprintln!("skipping reliability fixture smoke test; set GLASS_E2E=1 to run it");
        return;
    }
    if cfg!(target_os = "macos")
        && cfg!(target_arch = "x86_64")
        && std::env::var("GITHUB_ACTIONS").as_deref() == Ok("true")
    {
        eprintln!(
            "skipping reliability fixture smoke test on GitHub-hosted Intel macOS; the runner's CDP is too slow for this bounded scenario"
        );
        return;
    }
    let chrome_path = required_chrome();
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);
    let fixture_server = FixtureServer::start(include_str!("fixtures/reliability-lab.html")).await;
    let session = BrowserSession::start(&SessionOptions {
        port,
        chrome_path: Some(chrome_path),
        profile: "reliability-lab-e2e".to_string(),
        incognito: true,
        attach: false,
        target_id: None,
        frame_id: None,
        audit: false,
        policy: None,
        headed: false,
        interaction_mode: InteractionMode::Fast,
    })
    .await
    .unwrap();
    session.navigate(&fixture_server.url).await.unwrap();

    session
        .evaluate("window.reliabilityLab.reset(); true")
        .await
        .unwrap();
    let initial = reliability_snapshot(&session).await;
    assert_eq!(initial["state"], "idle");
    assert_eq!(initial["submitCount"], 0);
    assert_eq!(initial["targetPresent"], true);
    assert_eq!(initial["framePresent"], true);

    session
        .evaluate("window.reliabilityLab.replaceTarget(); true")
        .await
        .unwrap();
    assert_eq!(
        reliability_snapshot(&session).await["state"],
        "target-replaced"
    );
    session
        .evaluate("window.reliabilityLab.reset(); true")
        .await
        .unwrap();
    session
        .evaluate("window.reliabilityLab.moveTargetToOtherRegion(); true")
        .await
        .unwrap();
    assert_eq!(
        reliability_snapshot(&session).await["state"],
        "region-moved"
    );
    session
        .evaluate("window.reliabilityLab.reset(); true")
        .await
        .unwrap();

    session.click("css=#target").await.unwrap();
    let submitted = reliability_snapshot(&session).await;
    assert_eq!(submitted["state"], "submitted");
    assert_eq!(submitted["submitCount"], 1);

    for operation in [
        "duplicateTarget",
        "reorderTargets",
        "renameTarget",
        "showOverlay",
        "moveTarget",
        "detachFrame",
    ] {
        session
            .evaluate(&format!("window.reliabilityLab.{operation}(); true"))
            .await
            .unwrap();
    }
    let changed = reliability_snapshot(&session).await;
    assert_eq!(changed["state"], "frame-detached");
    assert_eq!(changed["overlayVisible"], true);
    assert_eq!(changed["framePresent"], false);

    session
        .evaluate("window.reliabilityLab.scheduleEffectMarker(0); true")
        .await
        .unwrap();
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(1);
    loop {
        if reliability_snapshot(&session).await["state"] == "effect-visible" {
            break;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "effect marker did not become visible within the bounded wait"
        );
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }

    session.close().await.unwrap();
    fixture_server.close().await;
}

#[tokio::test]
async fn reliability_runner_generates_live_fixture_evidence() {
    if std::env::var("GLASS_E2E").as_deref() != Ok("1") {
        eprintln!("skipping reliability runner smoke test; set GLASS_E2E=1 to run it");
        return;
    }
    if !(cfg!(all(target_os = "linux", target_arch = "x86_64"))
        || cfg!(all(target_os = "linux", target_arch = "aarch64"))
        || cfg!(all(target_os = "macos", target_arch = "x86_64"))
        || cfg!(all(target_os = "macos", target_arch = "aarch64")))
    {
        eprintln!("skipping reliability runner smoke test on an unsupported release target");
        return;
    }
    let chrome_path = required_chrome();
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);
    let fixture_server = FixtureServer::start(include_str!("fixtures/reliability-lab.html")).await;
    let session = BrowserSession::start(&SessionOptions {
        port,
        chrome_path: Some(chrome_path),
        profile: "reliability-runner-e2e".to_string(),
        incognito: true,
        attach: false,
        target_id: None,
        frame_id: None,
        audit: false,
        policy: None,
        headed: false,
        interaction_mode: InteractionMode::Fast,
    })
    .await
    .unwrap();
    session.navigate(&fixture_server.url).await.unwrap();

    let scenario = ReliabilityScenario::from_json(
        r#"{
          "schemaVersion": 1,
          "id": "live-submit-runner",
          "category": "transactional-workflow",
          "fixture": "checkout-submit",
          "platforms": ["linux-x86-64", "linux-arm64", "macos-x86-64", "macos-arm64"],
          "capabilities": ["workflow", "idempotency"],
          "setup": {"browser": "chromium", "policy": "development"},
          "steps": [
            {"applyControl": "reset"},
            {"runWorkflow": "reliability-submit.json"}
          ],
          "expect": {
            "terminalState": "submitted",
            "sideEffectCount": {"submit": 1}
          },
          "forbid": ["nonIdempotentMutationDuplicated", "falseWorkflowCompletion"],
          "budgets": {"maxDurationMs": 30000, "maxBrowserActions": 8}
        }"#,
    )
    .unwrap();
    let fixture =
        ReliabilityFixtureManifest::from_json(include_str!("fixtures/reliability-fixture-v1.json"))
            .unwrap();
    let options = ReliabilityRunOptions {
        workflow_root: PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures"),
        inputs: BTreeMap::new(),
    };

    let evidence = run_reliability_scenario(&session, &scenario, &fixture, &options)
        .await
        .unwrap();
    assert_eq!(
        evidence.observation.classification,
        ReliabilityRunClassification::Passed
    );
    assert_eq!(evidence.observation.side_effect_count["submit"], 1);
    assert!(evidence.observation.oracle_evidence);
    evidence.replay.validate(&scenario).unwrap();

    session.close().await.unwrap();
    fixture_server.close().await;
}

#[tokio::test]
async fn reliability_capability_suite_generates_live_evidence() {
    if std::env::var("GLASS_E2E").as_deref() != Ok("1") {
        eprintln!("skipping reliability capability suite; set GLASS_E2E=1 to run it");
        return;
    }
    let chrome_path = required_chrome();
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);
    let fixture_server = FixtureServer::start(include_str!("fixtures/reliability-lab.html")).await;
    let session = BrowserSession::start(&SessionOptions {
        port,
        chrome_path: Some(chrome_path),
        profile: "reliability-capability-suite-e2e".to_string(),
        incognito: true,
        attach: false,
        target_id: None,
        frame_id: None,
        audit: false,
        policy: None,
        headed: false,
        interaction_mode: InteractionMode::Fast,
    })
    .await
    .unwrap();
    session.navigate(&fixture_server.url).await.unwrap();

    let scenario_values: Vec<Value> = serde_json::from_str(include_str!(
        "fixtures/reliability-capability-suite-v1.json"
    ))
    .unwrap();
    let scenarios: Vec<ReliabilityScenario> = scenario_values
        .into_iter()
        .map(ReliabilityScenario::from_value)
        .collect::<Result<_, _>>()
        .unwrap();
    let fixture =
        ReliabilityFixtureManifest::from_json(include_str!("fixtures/reliability-fixture-v1.json"))
            .unwrap();
    let options = ReliabilityRunOptions {
        workflow_root: PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures"),
        inputs: BTreeMap::new(),
    };

    for scenario in scenarios {
        session
            .evaluate("window.reliabilityLab.reset(); true")
            .await
            .unwrap();
        let evidence = run_reliability_scenario(&session, &scenario, &fixture, &options)
            .await
            .unwrap();
        assert!(
            matches!(
                evidence.observation.classification,
                ReliabilityRunClassification::Passed | ReliabilityRunClassification::SafeRefusal
            ),
            "scenario {} did not certify: {:?}",
            scenario.id,
            evidence.observation
        );
        assert_eq!(
            evidence.observation.terminal_state.as_deref(),
            Some(scenario.expect.terminal_state.as_str()),
            "scenario {} reported an unexpected fixture terminal state",
            scenario.id
        );
        evidence.replay.validate(&scenario).unwrap();
        write_reliability_evidence(&scenario.id, &evidence);
    }

    session.close().await.unwrap();
    fixture_server.close().await;
}

fn write_reliability_evidence(
    scenario_id: &str,
    evidence: &glass::reliability_runner::ReliabilityRunEvidence,
) {
    let Some(directory) = std::env::var_os("GLASS_RELIABILITY_EVIDENCE_DIR") else {
        return;
    };
    let directory = PathBuf::from(directory);
    std::fs::create_dir_all(&directory).unwrap();
    let path = directory.join(format!("{scenario_id}.json"));
    let value = json!({
        "observation": evidence.observation,
        "replay": evidence.replay,
    });
    std::fs::write(path, serde_json::to_vec_pretty(&value).unwrap()).unwrap();
}

fn process_rss_bytes() -> Option<u64> {
    let status = std::fs::read_to_string("/proc/self/status").ok()?;
    let kib = status
        .lines()
        .find_map(|line| line.strip_prefix("VmRSS:"))?
        .split_whitespace()
        .next()?
        .parse::<u64>()
        .ok()?;
    Some(kib * 1024)
}

#[tokio::test]
async fn browser_session_routes_explicit_targets_and_frames() {
    if std::env::var("GLASS_E2E").as_deref() != Ok("1") {
        eprintln!("skipping browser smoke test; set GLASS_E2E=1 to run it");
        return;
    }
    let chrome_path = required_chrome();
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);
    let cross_origin = FixtureServer::start(
        "<button id='cross' onclick=\"document.body.dataset.clicked='yes'\">cross origin frame</button><input id='cross-check' type='checkbox'><input id='cross-input'><input id='cross-upload' type='file'>",
    )
    .await;
    let cross_origin_url = cross_origin.url.replace("127.0.0.1", "localhost");
    let html = format!(
        "<title>topology</title><a id='popup' href='about:blank' target='_blank'>popup</a><iframe style='margin:80px' srcdoc=\"<button id='nested' onclick=&quot;document.body.dataset.clicked='yes'&quot;>nested frame</button>\"></iframe><iframe style='margin:70px' srcdoc=\"<iframe style='margin:50px' srcdoc=&quot;<button id='deep' onclick='document.body.dataset.clicked=1'>deep frame</button>&quot;></iframe>\"></iframe><iframe style='margin:60px' src='{}'></iframe>",
        cross_origin_url
    );
    let fixture = FixtureServer::start(Box::leak(html.into_boxed_str())).await;
    let session = BrowserSession::start(&SessionOptions {
        port,
        chrome_path: Some(chrome_path),
        profile: "topology-e2e".to_string(),
        audit: false,
        policy: None,
        incognito: true,
        attach: false,
        target_id: None,
        frame_id: None,
        headed: false,
        interaction_mode: InteractionMode::Fast,
    })
    .await
    .unwrap();
    session.navigate(&fixture.url).await.unwrap();

    let original = session
        .list_targets()
        .await
        .unwrap()
        .into_iter()
        .find(|target| target.active)
        .unwrap();
    session.click("css=#popup").await.unwrap();
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    let targets = session.list_targets().await.unwrap();
    assert_eq!(targets.len(), 2);
    assert!(
        targets
            .iter()
            .any(|target| target.id == original.id && target.active)
    );
    let popup = targets
        .into_iter()
        .find(|target| target.id != original.id)
        .unwrap();
    assert_eq!(popup.opener_id.as_deref(), Some(original.id.as_str()));
    session.select_target(&popup.id).await.unwrap();
    session
        .navigate("data:text/html,<title>popup</title>")
        .await
        .unwrap();
    assert_eq!(session.evaluate("document.title").await.unwrap(), "popup");
    session.select_target(&original.id).await.unwrap();
    tokio::time::sleep(std::time::Duration::from_millis(800)).await;

    let frames = session.list_frames().await.unwrap();
    assert!(
        frames.len() >= 5,
        "expected main, direct, grandchild, and cross-origin frames: {frames:?}"
    );
    let cross = frames
        .iter()
        .find(|frame| frame.url == cross_origin_url)
        .unwrap();
    assert!(
        cross.out_of_process,
        "cross-site frame must use an OOPIF session"
    );
    session.select_frame(&cross.id).await.unwrap();
    assert_eq!(
        session.evaluate("document.body.innerText").await.unwrap(),
        "cross origin frame"
    );
    session.click("css=#cross").await.unwrap();
    assert_eq!(
        session
            .evaluate("document.body.dataset.clicked")
            .await
            .unwrap(),
        "yes"
    );
    session.check("css=#cross-check").await.unwrap();
    assert_eq!(
        session
            .evaluate("document.querySelector('#cross-check').checked")
            .await
            .unwrap(),
        true
    );
    session
        .type_text("frame", Some("css=#cross-input"))
        .await
        .unwrap();
    session.clear("css=#cross-input").await.unwrap();
    assert_eq!(
        session
            .evaluate("document.querySelector('#cross-input').value")
            .await
            .unwrap(),
        ""
    );
    let frame_upload = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/basic.html");
    session
        .upload_files("css=#cross-upload", &[frame_upload])
        .await
        .unwrap();
    assert_eq!(
        session
            .evaluate("document.querySelector('#cross-upload').files.length")
            .await
            .unwrap(),
        1
    );
    let deep = frames
        .iter()
        .find(|frame| {
            frame.parent_id.as_deref().is_some_and(|parent| {
                frames
                    .iter()
                    .find(|candidate| candidate.id == parent)
                    .is_some_and(|candidate| candidate.parent_id.is_some())
            })
        })
        .unwrap();
    session.select_frame(&deep.id).await.unwrap();
    session.click("css=#deep").await.unwrap();
    assert_eq!(
        session
            .evaluate("document.body.dataset.clicked")
            .await
            .unwrap(),
        "1"
    );
    let nested = frames
        .iter()
        .find(|frame| frame.url == "about:srcdoc")
        .unwrap();
    session.select_frame(&nested.id).await.unwrap();
    session.click("css=#nested").await.unwrap();
    assert_eq!(
        session
            .evaluate("document.body.dataset.clicked")
            .await
            .unwrap(),
        "yes"
    );
    session.select_target(&popup.id).await.unwrap();
    session.close_target(&popup.id).await.unwrap();
    assert!(
        session
            .list_frames()
            .await
            .unwrap_err()
            .to_string()
            .contains("no active target")
    );
    session.select_target(&original.id).await.unwrap();
    let binary = glass_binary_path();
    let cli_targets = Command::new(&binary)
        .args(["--attach", "--port", &port.to_string(), "targets"])
        .output()
        .await
        .unwrap();
    assert!(cli_targets.status.success());
    let cli_targets: Value = serde_json::from_slice(&cli_targets.stdout).unwrap();
    let cli_target_id = cli_targets[0]["id"].as_str().unwrap().to_string();
    let cli_frames = Command::new(&binary)
        .args([
            "--attach",
            "--port",
            &port.to_string(),
            "--target-id",
            &cli_target_id,
            "frames",
        ])
        .output()
        .await
        .unwrap();
    assert!(cli_frames.status.success());
    let cli_frames: Value = serde_json::from_slice(&cli_frames.stdout).unwrap();
    let cli_nested_id = cli_frames
        .as_array()
        .unwrap()
        .iter()
        .find(|frame| frame["url"] == "about:srcdoc")
        .and_then(|frame| frame["id"].as_str())
        .unwrap()
        .to_string();
    let cli_output = Command::new(&binary)
        .args([
            "--attach",
            "--port",
            &port.to_string(),
            "--target-id",
            &cli_target_id,
            "--frame-id",
            &cli_nested_id,
            "evaluate",
            "document.body.innerText",
        ])
        .output()
        .await
        .unwrap();
    assert!(cli_output.status.success());
    assert_eq!(
        serde_json::from_slice::<Value>(&cli_output.stdout).unwrap(),
        "nested frame"
    );
    let mut mcp = Command::new(&binary)
        .args(["--attach", "--port", &port.to_string(), "--mcp"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut mcp_stdin = mcp.stdin.take().unwrap();
    let mut mcp_stdout = BufReader::new(mcp.stdout.take().unwrap());
    write_mcp_line(&mut mcp_stdin, json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05"}})).await;
    read_mcp_line(&mut mcp_stdout).await;
    write_mcp_line(
        &mut mcp_stdin,
        json!({"jsonrpc":"2.0","method":"notifications/initialized"}),
    )
    .await;
    write_mcp_line(&mut mcp_stdin, json!({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"listTargets","arguments":{}}})).await;
    let targets = read_mcp_line(&mut mcp_stdout).await;
    let targets: Value =
        serde_json::from_str(targets["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
    let mcp_target_id = targets[0]["id"].as_str().unwrap();
    write_mcp_line(&mut mcp_stdin, json!({"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"selectTarget","arguments":{"id":mcp_target_id}}})).await;
    read_mcp_line(&mut mcp_stdout).await;
    write_mcp_line(&mut mcp_stdin, json!({"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"listFrames","arguments":{}}})).await;
    let frames = read_mcp_line(&mut mcp_stdout).await;
    let frames: Value =
        serde_json::from_str(frames["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
    let mcp_nested_id = frames
        .as_array()
        .unwrap()
        .iter()
        .find(|frame| frame["url"] == "about:srcdoc")
        .and_then(|frame| frame["id"].as_str())
        .unwrap();
    write_mcp_line(&mut mcp_stdin, json!({"jsonrpc":"2.0","id":5,"method":"tools/call","params":{"name":"selectFrame","arguments":{"id":mcp_nested_id}}})).await;
    read_mcp_line(&mut mcp_stdout).await;
    write_mcp_line(&mut mcp_stdin, json!({"jsonrpc":"2.0","id":6,"method":"tools/call","params":{"name":"evaluate","arguments":{"expression":"document.body.innerText"}}})).await;
    let evaluated = read_mcp_line(&mut mcp_stdout).await;
    let text = evaluated["result"]["content"][0]["text"].as_str().unwrap();
    assert_eq!(serde_json::from_str::<Value>(text).unwrap(), "nested frame");
    drop(mcp_stdin);
    assert!(mcp.wait().await.unwrap().success());
    let _ = tokio::time::timeout(
        std::time::Duration::from_secs(1),
        session.raw_cdp().unwrap().send("Page.crash", None),
    )
    .await;
    tokio::time::sleep(std::time::Duration::from_millis(150)).await;
    assert!(
        session
            .list_frames()
            .await
            .unwrap_err()
            .to_string()
            .contains("no active target")
    );

    session.close().await.unwrap();
    fixture.close().await;
    cross_origin.close().await;
}

#[tokio::test]
async fn browser_session_rejects_unverified_menu_open() {
    if std::env::var("GLASS_E2E").as_deref() != Ok("1") {
        eprintln!("skipping menu verification smoke test; set GLASS_E2E=1 to run it");
        return;
    }
    let chrome_path = required_chrome();
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);
    let fixture_server = FixtureServer::start(include_str!("fixtures/task-form.html")).await;
    let session = BrowserSession::start(&SessionOptions {
        port,
        chrome_path: Some(chrome_path),
        profile: "menu-verification-e2e".into(),
        incognito: true,
        attach: false,
        target_id: None,
        frame_id: None,
        audit: false,
        policy: None,
        headed: false,
        interaction_mode: InteractionMode::Fast,
    })
    .await
    .unwrap();
    session.navigate(&fixture_server.url).await.unwrap();
    let observation = session
        .semantic_observe(SemanticObservationLevel::Structured)
        .await
        .unwrap();
    let task = GlassTask {
        schema_version: TASK_PROTOCOL_SCHEMA_VERSION,
        task: TaskKind::NavigationOpenMenu,
        scope: TaskScope {
            region_name: Some("Main navigation".into()),
            ..TaskScope::default()
        },
        inputs: BTreeMap::from([(String::from("menu"), String::from("No-op menu"))]),
        limits: TaskLimits::default(),
        risk: TaskRiskClass::LocalMutation,
        ambiguity: TaskAmbiguityPolicy::Fail,
        revision: Default::default(),
        postconditions: Vec::new(),
    };
    let result = session
        .execute_task(&task, observation.revision, false)
        .await
        .unwrap();
    assert_eq!(result.status, "indeterminate");
    session.close().await.unwrap();
    fixture_server.close().await;
}

#[tokio::test]
async fn browser_session_rejects_unverified_tab_selection() {
    if std::env::var("GLASS_E2E").as_deref() != Ok("1") {
        eprintln!("skipping tab verification smoke test; set GLASS_E2E=1 to run it");
        return;
    }
    let chrome_path = required_chrome();
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);
    let fixture_server = FixtureServer::start(include_str!("fixtures/task-form.html")).await;
    let session = BrowserSession::start(&SessionOptions {
        port,
        chrome_path: Some(chrome_path),
        profile: "tab-verification-e2e".into(),
        incognito: true,
        attach: false,
        target_id: None,
        frame_id: None,
        audit: false,
        policy: None,
        headed: false,
        interaction_mode: InteractionMode::Fast,
    })
    .await
    .unwrap();
    session.navigate(&fixture_server.url).await.unwrap();
    let observation = session
        .semantic_observe(SemanticObservationLevel::Structured)
        .await
        .unwrap();
    let task = GlassTask {
        schema_version: TASK_PROTOCOL_SCHEMA_VERSION,
        task: TaskKind::NavigationSelectTab,
        scope: TaskScope {
            region_name: Some("Checkout".into()),
            ..TaskScope::default()
        },
        inputs: BTreeMap::from([(String::from("tab"), String::from("No-op tab"))]),
        limits: TaskLimits {
            timeout_ms: 100,
            ..TaskLimits::default()
        },
        risk: TaskRiskClass::LocalMutation,
        ambiguity: TaskAmbiguityPolicy::Fail,
        revision: Default::default(),
        postconditions: Vec::new(),
    };
    let result = session
        .execute_task(&task, observation.revision, false)
        .await
        .unwrap();
    assert_eq!(result.status, "indeterminate");
    session.close().await.unwrap();
    fixture_server.close().await;
}

#[tokio::test]
async fn browser_session_rejects_unverified_pagination_next() {
    if std::env::var("GLASS_E2E").as_deref() != Ok("1") {
        eprintln!("skipping pagination verification smoke test; set GLASS_E2E=1 to run it");
        return;
    }
    let chrome_path = required_chrome();
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);
    let fixture_server = FixtureServer::start(include_str!("fixtures/task-form.html")).await;
    let session = BrowserSession::start(&SessionOptions {
        port,
        chrome_path: Some(chrome_path),
        profile: "pagination-verification-e2e".into(),
        incognito: true,
        attach: false,
        target_id: None,
        frame_id: None,
        audit: false,
        policy: None,
        headed: false,
        interaction_mode: InteractionMode::Fast,
    })
    .await
    .unwrap();
    session.navigate(&fixture_server.url).await.unwrap();
    let observation = session
        .semantic_observe(SemanticObservationLevel::Structured)
        .await
        .unwrap();
    let task = GlassTask {
        schema_version: TASK_PROTOCOL_SCHEMA_VERSION,
        task: TaskKind::PaginationNext,
        scope: TaskScope {
            region_name: Some("Pagination".into()),
            ..TaskScope::default()
        },
        inputs: BTreeMap::from([(String::from("next"), String::from("No-op next"))]),
        limits: TaskLimits {
            timeout_ms: 100,
            ..TaskLimits::default()
        },
        risk: TaskRiskClass::LocalMutation,
        ambiguity: TaskAmbiguityPolicy::Fail,
        revision: Default::default(),
        postconditions: Vec::new(),
    };
    let result = session
        .execute_task(&task, observation.revision, false)
        .await
        .unwrap();
    assert_eq!(result.status, "indeterminate");
    session.close().await.unwrap();
    fixture_server.close().await;
}

#[tokio::test]
async fn browser_session_executes_scoped_form_tasks() {
    if std::env::var("GLASS_E2E").as_deref() != Ok("1") {
        eprintln!("skipping verified form smoke test; set GLASS_E2E=1 to run it");
        return;
    }
    let chrome_path = required_chrome();
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);
    let fixture_server = FixtureServer::start(include_str!("fixtures/task-form.html")).await;
    let session = BrowserSession::start(&SessionOptions {
        port,
        chrome_path: Some(chrome_path),
        profile: "task-form-e2e".into(),
        incognito: true,
        attach: false,
        target_id: None,
        frame_id: None,
        audit: false,
        policy: None,
        headed: false,
        interaction_mode: InteractionMode::Fast,
    })
    .await
    .unwrap();
    session.navigate(&fixture_server.url).await.unwrap();

    let before_navigation = session
        .semantic_observe(SemanticObservationLevel::Structured)
        .await
        .unwrap();
    let navigation_task = GlassTask {
        schema_version: TASK_PROTOCOL_SCHEMA_VERSION,
        task: TaskKind::NavigationFollow,
        scope: TaskScope {
            region_name: Some("Checkout".into()),
            ..TaskScope::default()
        },
        inputs: BTreeMap::from([(String::from("url"), fixture_server.url.clone())]),
        limits: TaskLimits::default(),
        risk: TaskRiskClass::ReadOnly,
        ambiguity: TaskAmbiguityPolicy::Fail,
        revision: Default::default(),
        postconditions: Vec::new(),
    };
    let navigated = session
        .execute_navigation_task(&navigation_task, before_navigation.revision, false)
        .await
        .unwrap();
    assert_eq!(navigated.status, "succeeded");
    assert_eq!(navigated.phase, "navigation-verification");
    let before_redirect = session
        .semantic_observe(SemanticObservationLevel::Structured)
        .await
        .unwrap();
    let redirect_task = GlassTask {
        schema_version: TASK_PROTOCOL_SCHEMA_VERSION,
        task: TaskKind::NavigationFollow,
        scope: TaskScope {
            region_name: Some("Checkout".into()),
            ..TaskScope::default()
        },
        inputs: BTreeMap::from([(
            String::from("url"),
            fixture_server.url.replace("/fixture.html", "/redirect"),
        )]),
        limits: TaskLimits::default(),
        risk: TaskRiskClass::ReadOnly,
        ambiguity: TaskAmbiguityPolicy::Fail,
        revision: Default::default(),
        postconditions: Vec::new(),
    };
    let redirected = session
        .execute_navigation_task(&redirect_task, before_redirect.revision, false)
        .await
        .unwrap();
    assert_eq!(redirected.status, "indeterminate");
    assert_eq!(
        redirected.steps[1].detail.as_deref(),
        Some("navigation destination was not verified")
    );

    let observation = session
        .semantic_observe(SemanticObservationLevel::Structured)
        .await
        .unwrap();
    let select_tab_task = GlassTask {
        schema_version: TASK_PROTOCOL_SCHEMA_VERSION,
        task: TaskKind::NavigationSelectTab,
        scope: TaskScope {
            region_name: Some("Checkout".into()),
            ..TaskScope::default()
        },
        inputs: BTreeMap::from([(String::from("tab"), String::from("Payment"))]),
        limits: TaskLimits::default(),
        risk: TaskRiskClass::LocalMutation,
        ambiguity: TaskAmbiguityPolicy::Fail,
        revision: Default::default(),
        postconditions: Vec::new(),
    };
    let selected = session
        .execute_form_task(&select_tab_task, observation.revision, false)
        .await
        .unwrap();
    assert_eq!(selected.status, "succeeded");
    let observation = session
        .semantic_observe(SemanticObservationLevel::Structured)
        .await
        .unwrap();
    let confirmation_target = observation
        .regions
        .iter()
        .flat_map(|region| region.targets.iter())
        .find(|target| target.name == "Confirm order")
        .expect("fixture confirmation target");
    let dialog_opened = session.click(&confirmation_target.reference).await.unwrap();
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    assert!(session.pending_dialog().await.is_some());
    let inspect_dialog_task = GlassTask {
        schema_version: TASK_PROTOCOL_SCHEMA_VERSION,
        task: TaskKind::DialogInspect,
        scope: TaskScope {
            region_name: Some("Checkout".into()),
            ..TaskScope::default()
        },
        inputs: BTreeMap::new(),
        limits: TaskLimits::default(),
        risk: TaskRiskClass::ReadOnly,
        ambiguity: TaskAmbiguityPolicy::Fail,
        revision: Default::default(),
        postconditions: vec![TaskPostcondition {
            kind: TaskPostconditionKind::DialogClosed,
            expected: None,
        }],
    };
    let inspected_dialog = session
        .execute_dialog_task(&inspect_dialog_task, dialog_opened.current_revision, false)
        .await
        .unwrap();
    assert_eq!(
        inspected_dialog.dialog.as_ref().unwrap().message,
        "Confirm order?"
    );
    assert_eq!(inspected_dialog.status, "indeterminate");
    let dialog_task = GlassTask {
        schema_version: TASK_PROTOCOL_SCHEMA_VERSION,
        task: TaskKind::DialogConfirm,
        scope: TaskScope {
            region_name: Some("Checkout".into()),
            ..TaskScope::default()
        },
        inputs: BTreeMap::new(),
        limits: TaskLimits::default(),
        risk: TaskRiskClass::RemoteIrreversible,
        ambiguity: TaskAmbiguityPolicy::Fail,
        revision: Default::default(),
        postconditions: Vec::new(),
    };
    let confirmed_dialog = session
        .execute_dialog_task(&dialog_task, dialog_opened.current_revision, true)
        .await
        .unwrap();
    assert_eq!(confirmed_dialog.status, "succeeded");
    let observation = session
        .semantic_observe(SemanticObservationLevel::Structured)
        .await
        .unwrap();
    let menu_task = GlassTask {
        schema_version: TASK_PROTOCOL_SCHEMA_VERSION,
        task: TaskKind::NavigationOpenMenu,
        scope: TaskScope {
            region_name: Some("Main navigation".into()),
            ..TaskScope::default()
        },
        inputs: BTreeMap::from([(String::from("menu"), String::from("Products"))]),
        limits: TaskLimits::default(),
        risk: TaskRiskClass::LocalMutation,
        ambiguity: TaskAmbiguityPolicy::Fail,
        revision: Default::default(),
        postconditions: Vec::new(),
    };
    let opened_menu = session
        .execute_task(&menu_task, observation.revision, false)
        .await
        .unwrap();
    assert_eq!(opened_menu.status, "succeeded");
    let observation = session
        .semantic_observe(SemanticObservationLevel::Structured)
        .await
        .unwrap();

    let pagination_task = GlassTask {
        schema_version: TASK_PROTOCOL_SCHEMA_VERSION,
        task: TaskKind::PaginationNext,
        scope: TaskScope {
            region_name: Some("Pagination".into()),
            ..TaskScope::default()
        },
        inputs: BTreeMap::from([(String::from("next"), String::from("Next page"))]),
        limits: TaskLimits::default(),
        risk: TaskRiskClass::LocalMutation,
        ambiguity: TaskAmbiguityPolicy::Fail,
        revision: Default::default(),
        postconditions: Vec::new(),
    };
    let paginated = session
        .execute_form_task(&pagination_task, observation.revision, false)
        .await
        .unwrap();
    assert_eq!(paginated.status, "succeeded");
    let collect_task = GlassTask {
        schema_version: TASK_PROTOCOL_SCHEMA_VERSION,
        task: TaskKind::PaginationCollect,
        scope: TaskScope {
            region_name: Some("Pagination".into()),
            ..TaskScope::default()
        },
        inputs: BTreeMap::from([(String::from("next"), String::from("Next page"))]),
        limits: TaskLimits {
            max_items: 2,
            ..TaskLimits::default()
        },
        risk: TaskRiskClass::LocalMutation,
        ambiguity: TaskAmbiguityPolicy::Fail,
        revision: Default::default(),
        postconditions: Vec::new(),
    };
    let collected = session
        .execute_task(&collect_task, paginated.current_revision, false)
        .await
        .unwrap();
    assert_eq!(collected.status, "succeeded");
    assert_eq!(collected.steps.len(), 3);
    session.navigate(&fixture_server.url).await.unwrap();
    let observation = session
        .semantic_observe(SemanticObservationLevel::Structured)
        .await
        .unwrap();

    let extract_task = GlassTask {
        schema_version: TASK_PROTOCOL_SCHEMA_VERSION,
        task: TaskKind::RegionExtract,
        scope: TaskScope {
            region_name: Some("Checkout".into()),
            ..TaskScope::default()
        },
        inputs: BTreeMap::new(),
        limits: TaskLimits::default(),
        risk: TaskRiskClass::ReadOnly,
        ambiguity: TaskAmbiguityPolicy::Fail,
        revision: Default::default(),
        postconditions: Vec::new(),
    };
    let extracted = session
        .execute_task(&extract_task, observation.revision, false)
        .await
        .unwrap();
    assert_eq!(extracted.status, "succeeded");
    assert_eq!(extracted.extraction.as_ref().unwrap().records.len(), 1);
    assert_eq!(extracted.extraction.as_ref().unwrap().provenance, vec!["$"]);
    let collection_task = GlassTask {
        schema_version: TASK_PROTOCOL_SCHEMA_VERSION,
        task: TaskKind::CollectionExtract,
        scope: TaskScope {
            region_name: Some("Results".into()),
            ..TaskScope::default()
        },
        inputs: BTreeMap::new(),
        limits: TaskLimits::default(),
        risk: TaskRiskClass::ReadOnly,
        ambiguity: TaskAmbiguityPolicy::Fail,
        revision: Default::default(),
        postconditions: Vec::new(),
    };
    let collection = session
        .execute_task(&collection_task, observation.revision, false)
        .await
        .unwrap();
    assert_eq!(collection.status, "succeeded");
    assert_eq!(
        collection.extraction.as_ref().unwrap().provenance,
        vec!["$.structuredRecords"]
    );
    assert!(
        !collection
            .extraction
            .as_ref()
            .unwrap()
            .record_items
            .is_empty()
    );
    let table_task = GlassTask {
        schema_version: TASK_PROTOCOL_SCHEMA_VERSION,
        task: TaskKind::TableExtract,
        scope: TaskScope {
            region_name: Some("Orders".into()),
            ..TaskScope::default()
        },
        inputs: BTreeMap::new(),
        limits: TaskLimits::default(),
        risk: TaskRiskClass::ReadOnly,
        ambiguity: TaskAmbiguityPolicy::Fail,
        revision: Default::default(),
        postconditions: Vec::new(),
    };
    let table = session
        .execute_task(&table_task, observation.revision, false)
        .await
        .unwrap();
    assert_eq!(table.status, "succeeded");
    assert_eq!(
        table.extraction.as_ref().unwrap().provenance,
        vec!["$.structuredRecords"]
    );
    assert!(!table.extraction.as_ref().unwrap().record_items.is_empty());
    let field_read_task = GlassTask {
        schema_version: TASK_PROTOCOL_SCHEMA_VERSION,
        task: TaskKind::FieldRead,
        scope: TaskScope {
            region_name: Some("Checkout".into()),
            ..TaskScope::default()
        },
        inputs: BTreeMap::from([(String::from("field"), String::from("Email"))]),
        limits: TaskLimits::default(),
        risk: TaskRiskClass::ReadOnly,
        ambiguity: TaskAmbiguityPolicy::Fail,
        revision: Default::default(),
        postconditions: Vec::new(),
    };
    let field_read = session
        .execute_task(&field_read_task, observation.revision, false)
        .await
        .unwrap();
    assert_eq!(field_read.status, "succeeded");
    assert_eq!(
        field_read.extraction.as_ref().unwrap().records[0]["empty"],
        true
    );
    let password_read_task = GlassTask {
        schema_version: TASK_PROTOCOL_SCHEMA_VERSION,
        task: TaskKind::FieldRead,
        scope: TaskScope {
            region_name: Some("Checkout".into()),
            ..TaskScope::default()
        },
        inputs: BTreeMap::from([(String::from("field"), String::from("Password"))]),
        limits: TaskLimits::default(),
        risk: TaskRiskClass::ReadOnly,
        ambiguity: TaskAmbiguityPolicy::Fail,
        revision: Default::default(),
        postconditions: Vec::new(),
    };
    let password_read = session
        .execute_task(&password_read_task, observation.revision, false)
        .await
        .unwrap();
    assert_eq!(password_read.status, "succeeded");
    assert_eq!(
        password_read.extraction.as_ref().unwrap().records[0]["value"],
        "<redacted>"
    );

    let fill_task = GlassTask {
        schema_version: TASK_PROTOCOL_SCHEMA_VERSION,
        task: TaskKind::FormFill,
        scope: TaskScope {
            region_name: Some("Checkout".into()),
            ..TaskScope::default()
        },
        inputs: BTreeMap::from([(String::from("Email"), String::from("agent@example.test"))]),
        limits: TaskLimits::default(),
        risk: TaskRiskClass::LocalMutation,
        ambiguity: TaskAmbiguityPolicy::Fail,
        revision: Default::default(),
        postconditions: Vec::new(),
    };
    let fill_result = session
        .execute_form_task(&fill_task, observation.revision, false)
        .await
        .unwrap();
    assert_eq!(fill_result.status, "succeeded");
    assert_eq!(
        session
            .evaluate("document.querySelector('[name=email]').value")
            .await
            .unwrap(),
        "agent@example.test"
    );

    let submit_task = GlassTask {
        schema_version: TASK_PROTOCOL_SCHEMA_VERSION,
        task: TaskKind::FormSubmit,
        scope: fill_task.scope.clone(),
        inputs: BTreeMap::from([(String::from("submit"), String::from("Submit"))]),
        limits: TaskLimits::default(),
        risk: TaskRiskClass::RemoteIrreversible,
        ambiguity: TaskAmbiguityPolicy::Fail,
        revision: Default::default(),
        postconditions: vec![TaskPostcondition {
            kind: TaskPostconditionKind::NavigationOccurred,
            expected: None,
        }],
    };
    let before_submit = session
        .semantic_observe(SemanticObservationLevel::Structured)
        .await
        .unwrap();
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    let blocked = session
        .execute_form_task(&submit_task, before_submit.revision, false)
        .await
        .unwrap();
    assert_eq!(blocked.status, "preflight-failed");
    assert_eq!(
        session
            .evaluate("document.querySelector('#result').textContent")
            .await
            .unwrap(),
        ""
    );
    let retry_observation = session
        .semantic_observe(SemanticObservationLevel::Structured)
        .await
        .unwrap();
    let mut invalid_submit = submit_task.clone();
    invalid_submit
        .inputs
        .insert(String::from("submit"), String::from("Email"));
    let invalid_submit_result = session
        .execute_form_task(&invalid_submit, retry_observation.revision, true)
        .await
        .unwrap();
    assert_eq!(invalid_submit_result.status, "preflight-failed");
    assert_eq!(
        invalid_submit_result.steps[1].detail.as_deref(),
        Some("form.submit target is not a semantic submit control")
    );

    let submitted = session
        .execute_form_task(&submit_task, retry_observation.revision, true)
        .await
        .unwrap();
    assert_eq!(submitted.status, "succeeded");
    assert_eq!(
        session
            .evaluate("document.querySelector('#result').textContent")
            .await
            .unwrap(),
        "submitted"
    );

    session.close().await.unwrap();
    fixture_server.close().await;
}
