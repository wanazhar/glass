#![cfg(unix)]

use std::io::Write;
use std::os::unix::net::UnixListener;
use std::path::PathBuf;
use std::process::Command;

fn glass_binary() -> PathBuf {
    std::env::var_os("CARGO_BIN_EXE_glass")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/debug/glass"))
}

#[test]
fn daemon_recovers_dead_status_and_stale_socket() {
    let root = std::env::temp_dir().join(format!(
        "glass-daemon-recovery-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&root).unwrap();
    let socket = root.join("glass.sock");
    let status = root.join("daemon.json");
    let log = root.join("daemon.log");

    let listener = UnixListener::bind(&socket).unwrap();
    let stale_status = serde_json::json!({
        "protocolVersion": 1,
        "state": "running",
        "pid": std::process::id().saturating_add(1_000_000),
        "socket": socket,
        "statusPath": status,
        "startedAt": "2026-07-28T00:00:00Z",
        "transport": "unix-mcp-shared-session",
        "clientSessions": 0,
        "activeRuns": [{
            "requestId": "workflow-recovery-1",
            "ownerId": "daemon-client-1",
            "startedAt": "2026-07-28T00:00:01Z"
        }],
        "logPath": log,
    });
    drop(listener);
    let mut status_file = std::fs::File::create(&status).unwrap();
    write!(status_file, "{stale_status}").unwrap();

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
        .expect("daemon recovery start should run");
    assert!(
        start.status.success(),
        "{}",
        String::from_utf8_lossy(&start.stderr)
    );
    assert!(socket.exists());
    assert!(status.exists());

    let doctor = Command::new(glass_binary())
        .args([
            "daemon",
            "doctor",
            "--socket",
            socket.to_str().unwrap(),
            "--status",
            status.to_str().unwrap(),
        ])
        .output()
        .expect("daemon recovery doctor should run");
    assert!(doctor.status.success());
    let doctor: serde_json::Value = serde_json::from_slice(&doctor.stdout).unwrap();
    assert_eq!(doctor["recovery"]["state"], "reconciliation_required");

    let logs = Command::new(glass_binary())
        .args(["daemon", "logs", "--status", status.to_str().unwrap()])
        .output()
        .expect("daemon recovery logs should run");
    assert!(logs.status.success());
    let logs: serde_json::Value = serde_json::from_slice(&logs.stdout).unwrap();
    let content = logs["content"].as_str().unwrap();
    assert!(content.contains("active workflows are indeterminate"));
    assert!(content.contains("checkpoint reconciliation"));
    assert!(content.contains("workflow-recovery-1"));

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
        .expect("daemon recovery stop should run");
    assert!(
        stop.status.success(),
        "{}",
        String::from_utf8_lossy(&stop.stderr)
    );
    assert!(!socket.exists());
    assert!(!status.exists());
    std::fs::remove_file(log).unwrap();
    std::fs::remove_file(status.with_extension("recovery.json")).unwrap();
    std::fs::remove_dir(root).unwrap();
}
