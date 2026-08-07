use serde_json::Value;
use std::{
    io::Write,
    process::{Command, Stdio},
    time::{SystemTime, UNIX_EPOCH},
};

fn glass_binary() -> std::path::PathBuf {
    std::env::var_os("CARGO_BIN_EXE_glass")
        .map(Into::into)
        .expect("Cargo should expose the glass-dev integration-test binary")
}

fn temp_project() -> std::path::PathBuf {
    let suffix = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock should be after the Unix epoch")
        .as_nanos();
    let root = std::env::temp_dir().join(format!("glass-development-runtime-{suffix}"));
    std::fs::create_dir_all(&root).expect("temporary project should be created");
    std::fs::write(root.join("note.txt"), "hello from the project\n")
        .expect("temporary project file should be written");
    root
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
            "printf rc-ok",
            "--wait",
            "--root",
            root.to_str().unwrap(),
        ])
        .output()
        .expect("project run should start");
    assert!(run.status.success(), "project run failed: {:?}", run.stderr);
    let run_report: Value = serde_json::from_slice(&run.stdout).unwrap();
    assert_eq!(run_report["pty"], true);
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
fn mcp_project_read_stays_on_clean_json_rpc_stdout() {
    let root = temp_project();
    let mut child = Command::new(glass_binary())
        .arg("--mcp")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
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
            "method": "tools/call",
            "params": {
                "name": "project.read",
                "arguments": {"path": "note.txt", "root": root}
            }
        }),
    ] {
        writeln!(stdin, "{request}").unwrap();
    }
    drop(stdin);
    let output = child.wait_with_output().expect("MCP server should exit");
    assert!(output.status.success());
    let lines = String::from_utf8(output.stdout).unwrap();
    let responses = lines
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).expect("stdout must be JSON-RPC JSONL"))
        .collect::<Vec<_>>();
    assert_eq!(responses.len(), 2);
    assert!(
        responses[1]["result"]["content"]
            .as_str()
            .unwrap()
            .contains("hello from the project")
    );

    std::fs::remove_dir_all(root).expect("temporary project should be removed");
}
