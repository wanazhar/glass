use serde_json::Value;
use std::{
    io::Write,
    process::{Command, Stdio},
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

fn glass_binary() -> std::path::PathBuf {
    std::env::var_os("CARGO_BIN_EXE_glass")
        .map(Into::into)
        .expect("Cargo should expose the glass-dev integration-test binary")
}

#[cfg(windows)]
#[tokio::test]
async fn windows_named_pipe_daemon_lifecycle_reconnect_and_permissions() {
    use glass_dev::daemon::{DevelopmentDaemonRequest, DevelopmentDaemonStatus, request};
    use glass_dev::development::ToolCall;
    use std::time::Duration;

    let base = temp_project();
    let project = base.join("project");
    let status_path = base.join("glassd.json");
    let pipe = format!(r"\\.\pipe\glass-dev-native-test-{}", std::process::id());
    std::fs::create_dir_all(&project).unwrap();
    std::fs::write(project.join("README.md"), "native windows pipe\n").unwrap();
    let glass = glass_binary();
    let started = Command::new(&glass)
        .args(["daemon", "start", "--socket", &pipe, "--status"])
        .arg(&status_path)
        .output()
        .unwrap();
    assert!(
        started.status.success(),
        "{}",
        String::from_utf8_lossy(&started.stderr)
    );
    let status: DevelopmentDaemonStatus = serde_json::from_slice(&started.stdout).unwrap();
    assert_eq!(status.socket, std::path::PathBuf::from(&pipe));
    let token = std::fs::read_to_string(&status.token_path).unwrap();
    let base_request = |id: &str, operation: &str| DevelopmentDaemonRequest {
        id: id.into(),
        token: token.clone(),
        operation: operation.into(),
        workspace_id: Some("native-windows".into()),
        root: None,
        call: None,
        expected_generation: None,
        expected_project_revision: None,
        allow_mutation: false,
        confirmed: false,
        actor: Some("windows-native-test".into()),
        since: None,
        limit: None,
    };
    let mut open = base_request("open", "workspace.open");
    open.root = Some(project.clone());
    assert!(
        request(std::path::Path::new(&pipe), &open)
            .await
            .unwrap()
            .ok
    );
    let mut read = base_request("read", "workspace.tool");
    read.call = Some(ToolCall {
        id: "readme".into(),
        name: "glass.file.read".into(),
        arguments: serde_json::json!({"path":"README.md"}),
    });
    read.expected_generation = Some(1);
    read.expected_project_revision = Some(0);
    let read = request(std::path::Path::new(&pipe), &read).await.unwrap();
    assert!(read.ok, "{:?}", read.error);
    assert_eq!(read.result["content"], "native windows pipe\n");
    let inspect = request(
        std::path::Path::new(&pipe),
        &base_request("reconnect", "workspace.inspect"),
    )
    .await
    .unwrap();
    assert!(inspect.ok);
    assert_eq!(inspect.result["workspace"]["id"], "native-windows");
    let invalid = Command::new(&glass)
        .args([
            "daemon",
            "start",
            "--socket",
            r"\\.\pipe\not-glass-owned",
            "--status",
        ])
        .arg(base.join("invalid.json"))
        .status()
        .unwrap();
    assert!(!invalid.success());
    let stopped = Command::new(&glass)
        .args(["daemon", "stop", "--socket", &pipe, "--status"])
        .arg(&status_path)
        .output()
        .unwrap();
    assert!(
        stopped.status.success(),
        "{}",
        String::from_utf8_lossy(&stopped.stderr)
    );
    tokio::time::sleep(Duration::from_millis(100)).await;
    std::fs::remove_dir_all(base).unwrap();
}

fn glass_browser_binary() -> std::path::PathBuf {
    std::env::var_os("CARGO_BIN_EXE_glass-browser")
        .map(Into::into)
        .expect("Cargo should expose the glass-browser integration-test binary")
}

fn temp_project() -> std::path::PathBuf {
    static NEXT_PROJECT: AtomicU64 = AtomicU64::new(1);
    let suffix = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock should be after the Unix epoch")
        .as_nanos();
    let sequence = NEXT_PROJECT.fetch_add(1, Ordering::Relaxed);
    let root = std::env::temp_dir().join(format!(
        "glass-development-runtime-{}-{suffix}-{sequence}",
        std::process::id()
    ));
    std::fs::create_dir_all(&root).expect("temporary project should be created");
    std::fs::write(root.join("note.txt"), "hello from the project\n")
        .expect("temporary project file should be written");
    root
}

fn trusted_store(root: &std::path::Path) -> std::path::PathBuf {
    let path = root.with_extension("trust.json");
    glass_dev::WorkspaceTrustStore::at(&path)
        .trust_project(&glass_dev::WorkspaceIdentity::inspect(root).unwrap())
        .unwrap();
    path
}

#[test]
fn both_cli_help_paths_exit_successfully() {
    for binary in [glass_binary(), glass_browser_binary()] {
        let output = Command::new(binary)
            .arg("--help")
            .output()
            .expect("CLI help should start");
        assert!(
            output.status.success(),
            "CLI help failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(String::from_utf8_lossy(&output.stdout).contains("Usage:"));
    }
}

#[test]
fn cli_project_and_agent_paths_are_browser_free() {
    let root = temp_project();
    let binary = glass_binary();
    let trust_store = trusted_store(&root);

    let run = Command::new(&binary)
        .args([
            "project",
            "run",
            "smoke",
            "--command",
            "echo rc-ok",
            "--wait",
            "--root",
            root.to_str().unwrap(),
        ])
        .env("GLASS_TRUST_STORE_PATH", &trust_store)
        .output()
        .expect("project run should start");
    assert!(run.status.success(), "project run failed: {:?}", run.stderr);
    let run_report: Value = serde_json::from_slice(&run.stdout).unwrap();
    assert_eq!(run_report["pty"], cfg!(not(windows)));
    assert_eq!(run_report["state"]["exited"]["code"], 0);
    assert!(run_report["output"].as_str().unwrap().contains("rc-ok"));

    let agent = Command::new(&binary)
        .args([
            "agent",
            "prompt",
            "read note.txt",
            "--root",
            root.to_str().unwrap(),
        ])
        .env("GLASS_TRUST_STORE_PATH", &trust_store)
        .output()
        .expect("agent prompt should start");
    assert!(
        agent.status.success(),
        "agent prompt failed: {:?}",
        agent.stderr
    );
    let agent_report: Value = serde_json::from_slice(&agent.stdout).unwrap();
    assert!(agent_report.as_array().unwrap().iter().any(|event| {
        event["type"] == "toolResult" && event["result"]["content"] == "hello from the project\n"
    }));

    std::fs::remove_dir_all(root).expect("temporary project should be removed");
}

#[test]
fn yolo_does_not_bypass_workspace_trust_and_normal_mode_stays_gated() {
    let root = temp_project();
    let binary = glass_binary();
    let call = r#"{"id":"write","name":"glass.file.write","arguments":{"path":"yolo.txt","content":"approval-free\n"}}"#;

    let denied = Command::new(&binary)
        .args(["agent", "tool", call, "--root", root.to_str().unwrap()])
        .output()
        .expect("normal agent tool should run");
    assert!(!denied.status.success());
    assert!(!root.join("yolo.txt").exists());
    assert!(String::from_utf8_lossy(&denied.stderr).contains("trust"));

    let untrusted_yolo = Command::new(&binary)
        .args([
            "--yolo",
            "agent",
            "tool",
            call,
            "--root",
            root.to_str().unwrap(),
        ])
        .output()
        .expect("YOLO agent tool should run");
    assert!(!untrusted_yolo.status.success());
    assert!(!root.join("yolo.txt").exists());
    assert!(String::from_utf8_lossy(&untrusted_yolo.stderr).contains("trusted"));

    let trust_store = trusted_store(&root);
    let allowed = Command::new(&binary)
        .args([
            "--yolo",
            "agent",
            "tool",
            call,
            "--root",
            root.to_str().unwrap(),
        ])
        .env("GLASS_TRUST_STORE_PATH", trust_store)
        .output()
        .expect("trusted YOLO agent tool should run");
    assert!(
        allowed.status.success(),
        "YOLO agent tool failed: {:?}",
        allowed.stderr
    );
    let report: Value = serde_json::from_slice(&allowed.stdout).unwrap();
    assert_eq!(report["written"], true);
    assert_eq!(
        std::fs::read_to_string(root.join("yolo.txt")).unwrap(),
        "approval-free\n"
    );

    std::fs::remove_dir_all(root).expect("temporary project should be removed");
}

#[test]
fn mcp_combines_browser_and_resident_dev_tools_on_clean_json_rpc_stdout() {
    let root = temp_project();
    let mut child = Command::new(glass_binary())
        .arg("--mcp")
        .current_dir(&root)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("MCP server should start");
    let mut stdin = child.stdin.take().unwrap();
    for request in [
        serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {"protocolVersion": "2024-11-05"}
        }),
        serde_json::json!({
            "jsonrpc": "2.0",
            "method": "notifications/initialized",
            "params": {}
        }),
        serde_json::json!({
            "jsonrpc": "2.0",
            "id": 2,
            "method": "tools/list",
            "params": {}
        }),
        serde_json::json!({
            "jsonrpc": "2.0",
            "id": 3,
            "method": "tools/call",
            "params": {
                "name": "project.read",
                "arguments": {"path": "note.txt", "root": root}
            }
        }),
        serde_json::json!({
            "jsonrpc": "2.0",
            "id": 4,
            "method": "tools/call",
            "params": {
                "name": "glass.file.read",
                "arguments": {"path": "note.txt"}
            }
        }),
        serde_json::json!({
            "jsonrpc": "2.0",
            "id": 5,
            "method": "tools/call",
            "params": {
                "name": "project.run",
                "arguments": {
                    "name": "blocked",
                    "command": "echo should-not-run",
                    "wait": true,
                    "_glass": {"allowMutation": true, "confirmed": true}
                }
            }
        }),
    ] {
        writeln!(stdin, "{request}").unwrap();
    }
    drop(stdin);
    let output = child.wait_with_output().expect("MCP server should exit");
    assert!(
        output.status.success(),
        "MCP process failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let lines = String::from_utf8(output.stdout).unwrap();
    let responses = lines
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).expect("stdout must be JSON-RPC JSONL"))
        .collect::<Vec<_>>();
    assert_eq!(responses.len(), 5);
    let response = |id: u64| {
        responses
            .iter()
            .find(|response| response["id"] == id)
            .unwrap_or_else(|| panic!("missing MCP response {id}"))
    };
    assert!(
        response(3)["result"]["structuredContent"]["content"]
            .as_str()
            .unwrap()
            .contains("hello from the project")
    );
    let tools = response(2)["result"]["tools"].as_array().unwrap();
    for name in [
        "project.read",
        "observe",
        "glass.file.read",
        "glass.debug.threads",
    ] {
        assert!(
            tools.iter().any(|tool| tool["name"] == name),
            "missing MCP tool {name}"
        );
    }
    assert_eq!(
        response(4)["result"]["structuredContent"]["content"],
        "hello from the project\n"
    );
    assert_eq!(response(5)["result"]["isError"], true);
    assert!(
        response(5)["result"]["content"][0]["text"]
            .as_str()
            .unwrap()
            .contains("trusted")
    );
    assert!(!root.join("should-not-run").exists());
    let fixture: Value = serde_json::from_str(include_str!("fixtures/client-conformance-v1.json"))
        .expect("development conformance fixture should be valid JSON");
    let mut live_names = tools
        .iter()
        .map(|tool| tool["name"].as_str().unwrap().to_string())
        .collect::<Vec<_>>();
    live_names.sort();
    assert_eq!(
        live_names,
        fixture["tools"]
            .as_array()
            .unwrap()
            .iter()
            .map(|name| name.as_str().unwrap().to_string())
            .collect::<Vec<_>>()
    );

    std::fs::remove_dir_all(root).expect("temporary project should be removed");
}
