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
fn yolo_agent_tool_mutates_without_approval_and_normal_mode_stays_gated() {
    let root = temp_project();
    let binary = glass_binary();
    let call = r#"{"id":"write","name":"glass.file.write","arguments":{"path":"yolo.txt","content":"approval-free\n"}}"#;

    let denied = Command::new(&binary)
        .args(["agent", "tool", call, "--root", root.to_str().unwrap()])
        .output()
        .expect("normal agent tool should run");
    assert!(!denied.status.success());
    assert!(!root.join("yolo.txt").exists());
    assert!(
        String::from_utf8_lossy(&denied.stderr)
            .contains("requires mutation authority and confirmation")
    );

    let allowed = Command::new(&binary)
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
    assert_eq!(responses.len(), 4);
    let response = |id: u64| {
        responses
            .iter()
            .find(|response| response["id"] == id)
            .unwrap_or_else(|| panic!("missing MCP response {id}"))
    };
    assert!(
        response(3)["result"]["content"]
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
