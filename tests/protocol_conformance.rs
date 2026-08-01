use std::io::{BufRead, BufReader, Write};
use std::process::{Command, Stdio};

use glass::protocol::{GlassRequest, GlassResponse};
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
    writeln!(
        stdin,
        "{}",
        serde_json::json!({
            "jsonrpc": "2.0",
            "method": "notifications/initialized",
            "params": {}
        })
    )
    .unwrap();
    writeln!(
        stdin,
        "{}",
        serde_json::json!({
            "jsonrpc": "2.0",
            "id": 2,
            "method": "tools/list",
            "params": {}
        })
    )
    .unwrap();
    drop(stdin);
    let mut stdout = BufReader::new(child.stdout.take().unwrap());
    let mut line = String::new();
    stdout.read_line(&mut line).unwrap();
    let mcp_response: Value = serde_json::from_str(&line).unwrap();
    assert_eq!(mcp_response["result"]["glass"], cli_manifest);
    assert_eq!(
        mcp_response["result"]["glassAgreement"]["protocolVersion"],
        cli_manifest["protocolVersion"]
    );
    assert_eq!(
        mcp_response["result"]["glassAgreement"]["agreedSchemas"]["action"],
        1
    );
    line.clear();
    stdout.read_line(&mut line).unwrap();
    let tools_response: Value = serde_json::from_str(&line).unwrap();
    let fixture: Value = serde_json::from_str(include_str!("fixtures/client-conformance-v1.json"))
        .expect("client conformance fixture should be valid JSON");
    let mut tool_names = tools_response["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|tool| tool["name"].as_str().unwrap().to_string())
        .collect::<Vec<_>>();
    tool_names.sort();
    assert_eq!(
        tool_names,
        fixture["tools"]
            .as_array()
            .unwrap()
            .iter()
            .map(|name| name.as_str().unwrap().to_string())
            .collect::<Vec<_>>()
    );
    let _ = child.kill();
    let _ = child.wait();
}

#[test]
fn checked_in_protocol_golden_scenarios_round_trip_on_the_canonical_envelopes() {
    let fixture: Value = serde_json::from_str(include_str!("fixtures/protocol-golden-v1.json"))
        .expect("protocol golden fixture should be valid JSON");
    assert_eq!(fixture["schemaVersion"], 1);

    for value in fixture["requests"].as_array().unwrap() {
        let request: GlassRequest = serde_json::from_value(value.clone()).unwrap();
        request.validate().unwrap();
        assert_eq!(
            serde_json::from_value::<GlassRequest>(serde_json::to_value(&request).unwrap())
                .unwrap(),
            request
        );
    }
    for value in fixture["responses"].as_array().unwrap() {
        let response: GlassResponse = serde_json::from_value(value.clone()).unwrap();
        response.validate().unwrap();
        if let Some(error) = &response.error {
            error.validate().unwrap();
        }
        assert_eq!(
            serde_json::from_value::<GlassResponse>(serde_json::to_value(&response).unwrap())
                .unwrap(),
            response
        );

        let operation = fixture["requests"]
            .as_array()
            .unwrap()
            .iter()
            .find(|request| request["requestId"] == response.request_id)
            .and_then(|request| request["operation"].as_str())
            .unwrap_or_else(|| panic!("response requestId must match a fixture request"));
        match operation {
            "task.validate" => {
                assert!(response.decode_task_validation_result().unwrap().valid);
            }
            "webIr.continuity" => {
                assert_eq!(
                    response.decode_web_ir_continuity_result().unwrap().status,
                    glass::web_ir::DraftEntityContinuityStatus::Changed
                );
            }
            "webIr.diff" => {
                assert_eq!(
                    response
                        .decode_web_ir_diff_result()
                        .unwrap()
                        .entity_changed_count,
                    1
                );
            }
            "webIr.inspect" => {
                assert_eq!(
                    response
                        .decode_web_ir_inspection_result()
                        .unwrap()
                        .entity_count,
                    1
                );
            }
            "webIr.validate" => {
                assert!(response.decode_web_ir_validation_result().unwrap().valid);
            }
            _ => {}
        }
    }
}

#[cfg(unix)]
#[test]
fn daemon_handshake_advertises_isolated_session_mode() {
    use std::os::unix::net::UnixStream;

    let root = std::env::temp_dir().join(format!(
        "glass-pc-{}-{}",
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
    let mut reader = BufReader::new(stream.try_clone().unwrap());
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
    reader.read_line(&mut response).unwrap();
    let response_value: Value = serde_json::from_str(&response).unwrap();
    assert_eq!(
        response_value["result"]["glass"]["capabilities"]["localDaemon"],
        true
    );
    writeln!(
        stream,
        "{}",
        serde_json::json!({
            "jsonrpc": "2.0",
            "method": "notifications/initialized",
            "params": {}
        })
    )
    .unwrap();

    writeln!(
        stream,
        "{}",
        serde_json::json!({
            "jsonrpc": "2.0",
            "id": "lease-acquire",
            "method": "glass/lease/acquire",
            "params": {"ttlMs": 1000}
        })
    )
    .unwrap();
    response.clear();
    reader.read_line(&mut response).unwrap();
    let lease_response: Value = serde_json::from_str(&response).unwrap();
    assert_eq!(lease_response["result"]["ownerId"], "daemon-client-1");
    assert_eq!(lease_response["result"]["sessionId"], "daemon-session-1");
    let status_value: Value = serde_json::from_slice(&std::fs::read(&status).unwrap()).unwrap();
    assert_eq!(status_value["mutationLeaseOwner"], "daemon-client-1");
    assert!(status_value.get("mutationLease").is_none());

    writeln!(
        stream,
        "{}",
        serde_json::json!({
            "jsonrpc": "2.0",
            "id": "lease-release",
            "method": "glass/lease/release",
            "params": {"token": lease_response["result"]["token"]}
        })
    )
    .unwrap();
    response.clear();
    reader.read_line(&mut response).unwrap();
    assert_eq!(
        serde_json::from_str::<Value>(&response).unwrap()["result"]["released"],
        true
    );
    let status_value: Value = serde_json::from_slice(&std::fs::read(&status).unwrap()).unwrap();
    assert!(status_value.get("mutationLeaseOwner").is_none());

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
