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

#[cfg(unix)]
#[test]
fn daemon_handshake_advertises_shared_session_mode() {
    use std::os::unix::net::UnixStream;

    let root = std::env::temp_dir().join(format!(
        "glass-protocol-daemon-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&root).unwrap();
    let socket = root.join("glass.sock");
    let status = root.join("daemon.json");
    let start = Command::new(glass_binary())
        .args([
            "daemon",
            "start",
            "--socket",
            socket.to_str().unwrap(),
            "--status",
            status.to_str().unwrap(),
        ])
        .output()
        .expect("daemon should start");
    assert!(
        start.status.success(),
        "{}",
        String::from_utf8_lossy(&start.stderr)
    );

    let mut stream = UnixStream::connect(&socket).unwrap();
    writeln!(
        stream,
        "{}",
        serde_json::json!({
            "jsonrpc": "2.0",
            "id": "daemon-init",
            "method": "initialize",
            "params": {"protocolVersion": "2024-11-05"}
        })
    )
    .unwrap();
    let mut response = String::new();
    BufReader::new(stream.try_clone().unwrap())
        .read_line(&mut response)
        .unwrap();
    let response: Value = serde_json::from_str(&response).unwrap();
    assert_eq!(
        response["result"]["glass"]["capabilities"]["localDaemon"],
        true
    );

    let stop = Command::new(glass_binary())
        .args([
            "daemon",
            "stop",
            "--socket",
            socket.to_str().unwrap(),
            "--status",
            status.to_str().unwrap(),
        ])
        .output()
        .expect("daemon should stop");
    assert!(
        stop.status.success(),
        "{}",
        String::from_utf8_lossy(&stop.stderr)
    );
    drop(stream);
    std::fs::remove_file(status.with_extension("log")).unwrap();
    std::fs::remove_dir(root).unwrap();
}
