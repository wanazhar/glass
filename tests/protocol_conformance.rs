use std::io::{BufRead, BufReader, Write};
use std::process::{Command, Stdio};

use serde_json::Value;

fn glass_binary() -> std::path::PathBuf {
    std::env::var_os("CARGO_BIN_EXE_glass")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| {
            std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/debug/glass")
        })
}

#[test]
fn cli_and_mcp_advertise_the_same_capability_inventory() {
    let cli_output = Command::new(glass_binary())
        .arg("capabilities")
        .output()
        .expect("capability command should start");
    assert!(cli_output.status.success());
    let mut cli_manifest: Value = serde_json::from_slice(&cli_output.stdout).unwrap();
    cli_manifest.as_object_mut().unwrap().remove("contextCost");

    let mut child = Command::new(glass_binary())
        .arg("--mcp")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("MCP server should start");
    let mut stdin = child.stdin.take().unwrap();
    writeln!(
        stdin,
        "{}",
        serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {"protocolVersion": "2024-11-05"}
        })
    )
    .unwrap();
    drop(stdin);
    let mut stdout = BufReader::new(child.stdout.take().unwrap());
    let mut line = String::new();
    stdout.read_line(&mut line).unwrap();
    let mcp_response: Value = serde_json::from_str(&line).unwrap();
    assert_eq!(mcp_response["result"]["glass"], cli_manifest);
    let _ = child.kill();
    let _ = child.wait();
}
