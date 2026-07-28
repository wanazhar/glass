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
    std::fs::remove_dir(root).unwrap();
}
