//! Local Unix-socket daemon lifecycle and client bridge.
//!
//! The daemon is deliberately local-only. Each socket client receives an
//! isolated MCP child session, so a daemon restart or client disconnect cannot
//! silently transfer a browser session or workflow lease to another client.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::time::Duration;

/// Version of the local daemon status and lifecycle contract.
pub const DAEMON_PROTOCOL_VERSION: u32 = 1;

/// Stable daemon status written beside the Unix socket.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DaemonStatus {
    pub protocol_version: u32,
    pub state: String,
    pub pid: u32,
    pub socket: PathBuf,
    pub status_path: PathBuf,
    pub started_at: String,
    pub transport: String,
    pub client_sessions: u32,
}

/// Return the default local daemon paths.
pub fn default_paths() -> (PathBuf, PathBuf) {
    let root = dirs::data_local_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("glass");
    (root.join("glass.sock"), root.join("daemon.json"))
}

/// Start one background daemon and return its status contract.
pub async fn start(
    socket: Option<&Path>,
    status_path: Option<&Path>,
) -> Result<DaemonStatus, Box<dyn std::error::Error>> {
    #[cfg(not(unix))]
    {
        let _ = (socket, status_path);
        return Err("the local daemon supports Linux and macOS only".into());
    }
    #[cfg(unix)]
    {
        let (default_socket, default_status) = default_paths();
        let socket = socket.unwrap_or(&default_socket);
        let status_path = status_path.unwrap_or(&default_status);
        if let Some(existing) = read_status(status_path)? {
            if process_is_alive(existing.pid) {
                return Err(format!("daemon is already running as pid {}", existing.pid).into());
            }
            let _ = std::fs::remove_file(status_path);
            let _ = std::fs::remove_file(&existing.socket);
        }
        if socket.exists() {
            std::fs::remove_file(socket)?;
        }
        std::fs::create_dir_all(socket.parent().unwrap_or_else(|| Path::new(".")))?;
        std::fs::create_dir_all(status_path.parent().unwrap_or_else(|| Path::new(".")))?;
        let executable = std::env::current_exe()?;
        std::process::Command::new(executable)
            .args(["daemon", "serve"])
            .arg("--socket")
            .arg(socket)
            .arg("--status")
            .arg(status_path)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()?;
        let deadline = std::time::Instant::now() + Duration::from_secs(2);
        loop {
            if let Some(status) = read_status(status_path)?
                && status.state == "running"
                && process_is_alive(status.pid)
            {
                return Ok(status);
            }
            if std::time::Instant::now() >= deadline {
                return Err("daemon did not become ready within two seconds".into());
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
    }
}

/// Read and validate the current daemon status.
pub fn status(
    socket: Option<&Path>,
    status_path: Option<&Path>,
) -> Result<DaemonStatus, Box<dyn std::error::Error>> {
    let (default_socket, default_status) = default_paths();
    let socket = socket.unwrap_or(&default_socket);
    let status_path = status_path.unwrap_or(&default_status);
    let Some(mut status) = read_status(status_path)? else {
        return Err("daemon is not running".into());
    };
    if status.socket != socket {
        return Err("daemon status socket does not match the requested socket".into());
    }
    if !process_is_alive(status.pid) {
        status.state = "stopped".into();
    }
    Ok(status)
}

/// Stop a daemon by its recorded PID and remove its local status artifacts.
pub fn stop(
    socket: Option<&Path>,
    status_path: Option<&Path>,
) -> Result<(), Box<dyn std::error::Error>> {
    #[cfg(not(unix))]
    {
        let _ = (socket, status_path);
        return Err("the local daemon supports Linux and macOS only".into());
    }
    #[cfg(unix)]
    {
        let (default_socket, default_status) = default_paths();
        let socket = socket.unwrap_or(&default_socket);
        let status_path = status_path.unwrap_or(&default_status);
        let Some(status) = read_status(status_path)? else {
            return Err("daemon is not running".into());
        };
        if status.socket != socket {
            return Err("daemon status socket does not match the requested socket".into());
        }
        if process_is_alive(status.pid) {
            let result = unsafe { libc::kill(status.pid as libc::pid_t, libc::SIGTERM) };
            if result != 0 {
                return Err(std::io::Error::last_os_error().into());
            }
        }
        let _ = std::fs::remove_file(status_path);
        let _ = std::fs::remove_file(socket);
        Ok(())
    }
}

/// Validate the daemon process, status file, and socket without connecting.
pub fn doctor(
    socket: Option<&Path>,
    status_path: Option<&Path>,
) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
    match status(socket, status_path) {
        Ok(status) => Ok(serde_json::json!({
            "status": "healthy",
            "daemon": status,
            "socketExists": status.socket.exists(),
            "pidAlive": process_is_alive(status.pid),
        })),
        Err(error) => Ok(serde_json::json!({
            "status": "unavailable",
            "detail": error.to_string(),
        })),
    }
}

/// Run the foreground Unix-socket server used by `daemon start`.
pub async fn serve(socket: &Path, status_path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    #[cfg(not(unix))]
    {
        let _ = (socket, status_path);
        return Err("the local daemon supports Linux and macOS only".into());
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        use tokio::net::UnixListener;

        if socket.exists() {
            std::fs::remove_file(socket)?;
        }
        std::fs::create_dir_all(socket.parent().unwrap_or_else(|| Path::new(".")))?;
        std::fs::create_dir_all(status_path.parent().unwrap_or_else(|| Path::new(".")))?;
        let listener = UnixListener::bind(socket)?;
        std::fs::set_permissions(socket, std::fs::Permissions::from_mode(0o600))?;
        let status = DaemonStatus {
            protocol_version: DAEMON_PROTOCOL_VERSION,
            state: "running".into(),
            pid: std::process::id(),
            socket: socket.to_path_buf(),
            status_path: status_path.to_path_buf(),
            started_at: chrono::Utc::now().to_rfc3339(),
            transport: "unix-mcp-stdio-bridge".into(),
            client_sessions: 0,
        };
        std::fs::write(status_path, serde_json::to_vec_pretty(&status)?)?;
        loop {
            let (stream, _) = listener.accept().await?;
            tokio::spawn(async move {
                if let Err(error) = bridge_client(stream).await {
                    tracing::warn!(%error, "daemon client bridge stopped");
                }
            });
        }
    }
}

#[cfg(unix)]
async fn bridge_client(
    stream: tokio::net::UnixStream,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let executable = std::env::current_exe()?;
    let mut child = tokio::process::Command::new(executable)
        .arg("--mcp")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn()?;
    let mut child_stdin = child.stdin.take().ok_or("MCP child stdin unavailable")?;
    let mut child_stdout = child.stdout.take().ok_or("MCP child stdout unavailable")?;
    let (mut socket_read, mut socket_write) = stream.into_split();
    let client_closed = {
        let to_child = tokio::io::copy(&mut socket_read, &mut child_stdin);
        let to_client = tokio::io::copy(&mut child_stdout, &mut socket_write);
        tokio::pin!(to_child);
        tokio::pin!(to_client);
        tokio::select! {
            _ = &mut to_child => true,
            _ = &mut to_client => false,
        }
    };
    drop(child_stdin);
    if client_closed {
        let _ = child.wait().await;
    } else {
        let _ = child.kill().await;
        let _ = child.wait().await;
    }
    Ok(())
}

fn read_status(path: &Path) -> Result<Option<DaemonStatus>, Box<dyn std::error::Error>> {
    if !path.is_file() {
        return Ok(None);
    }
    let status: DaemonStatus = serde_json::from_slice(&std::fs::read(path)?)?;
    if status.protocol_version != DAEMON_PROTOCOL_VERSION {
        return Err("unsupported daemon status protocol".into());
    }
    Ok(Some(status))
}

#[cfg(unix)]
fn process_is_alive(pid: u32) -> bool {
    unsafe { libc::kill(pid as libc::pid_t, 0) == 0 }
}

#[cfg(not(unix))]
fn process_is_alive(_pid: u32) -> bool {
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn daemon_status_serialization_is_versioned_and_local_only() {
        let status = DaemonStatus {
            protocol_version: DAEMON_PROTOCOL_VERSION,
            state: "running".into(),
            pid: 42,
            socket: PathBuf::from("/tmp/glass.sock"),
            status_path: PathBuf::from("/tmp/glass.json"),
            started_at: "2026-07-28T00:00:00Z".into(),
            transport: "unix-mcp-stdio-bridge".into(),
            client_sessions: 0,
        };
        let value = serde_json::to_value(&status).unwrap();

        assert_eq!(value["protocolVersion"], 1);
        assert_eq!(value["transport"], "unix-mcp-stdio-bridge");
        assert!(value["socket"].as_str().unwrap().starts_with('/'));
    }
}
