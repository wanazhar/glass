//! Local Unix-socket daemon lifecycle and client bridge.
//!
//! The daemon is deliberately local-only. Each socket client receives an
//! isolated MCP child session, so a daemon restart or client disconnect cannot
//! silently transfer a browser session or workflow lease to another client.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

/// Version of the local daemon status and lifecycle contract.
pub const DAEMON_PROTOCOL_VERSION: u32 = 1;
/// Maximum number of isolated MCP child sessions per daemon.
pub const MAX_DAEMON_CLIENT_SESSIONS: u32 = 4;
const MIN_LEASE_TTL_MS: u64 = 100;
const MAX_LEASE_TTL_MS: u64 = 15 * 60 * 1_000;

/// A single mutation lease held by one local client owner.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MutationLease {
    pub session_id: String,
    pub owner_id: String,
    pub token: String,
    pub expires_at_ms: u64,
}

/// Typed lease failure used by daemon/session integrations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LeaseError {
    InvalidInput(String),
    AlreadyHeld,
    NotFound,
    NotOwner,
    Expired,
}

impl std::fmt::Display for LeaseError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidInput(detail) => write!(formatter, "invalid lease input: {detail}"),
            Self::AlreadyHeld => formatter.write_str("mutation lease is already held"),
            Self::NotFound => formatter.write_str("mutation lease was not found"),
            Self::NotOwner => formatter.write_str("mutation lease is held by another owner"),
            Self::Expired => formatter.write_str("mutation lease has expired"),
        }
    }
}

impl std::error::Error for LeaseError {}

/// In-memory lease authority for one daemon session namespace.
#[derive(Debug, Default)]
pub struct MutationLeaseManager {
    leases: BTreeMap<String, MutationLease>,
    next_token: u64,
}

impl MutationLeaseManager {
    /// Acquire the only mutation lease for a session.
    pub fn acquire(
        &mut self,
        session_id: &str,
        owner_id: &str,
        now_ms: u64,
        ttl_ms: u64,
    ) -> Result<MutationLease, LeaseError> {
        validate_lease_identity(session_id, "sessionId")?;
        validate_lease_identity(owner_id, "ownerId")?;
        if !(MIN_LEASE_TTL_MS..=MAX_LEASE_TTL_MS).contains(&ttl_ms) {
            return Err(LeaseError::InvalidInput(format!(
                "ttlMs must be {MIN_LEASE_TTL_MS}..={MAX_LEASE_TTL_MS}"
            )));
        }
        if let Some(existing) = self.leases.get(session_id)
            && existing.expires_at_ms > now_ms
        {
            return Err(LeaseError::AlreadyHeld);
        }
        self.next_token = self.next_token.saturating_add(1);
        let lease = MutationLease {
            session_id: session_id.into(),
            owner_id: owner_id.into(),
            token: format!("lease-{}", self.next_token),
            expires_at_ms: now_ms.saturating_add(ttl_ms),
        };
        self.leases.insert(session_id.into(), lease.clone());
        Ok(lease)
    }

    /// Renew a lease without allowing a different owner to take it over.
    pub fn renew(
        &mut self,
        session_id: &str,
        owner_id: &str,
        token: &str,
        now_ms: u64,
        ttl_ms: u64,
    ) -> Result<MutationLease, LeaseError> {
        if !(MIN_LEASE_TTL_MS..=MAX_LEASE_TTL_MS).contains(&ttl_ms) {
            return Err(LeaseError::InvalidInput(format!(
                "ttlMs must be {MIN_LEASE_TTL_MS}..={MAX_LEASE_TTL_MS}"
            )));
        }
        let lease = self
            .leases
            .get_mut(session_id)
            .ok_or(LeaseError::NotFound)?;
        if lease.owner_id != owner_id || lease.token != token {
            return Err(LeaseError::NotOwner);
        }
        if lease.expires_at_ms <= now_ms {
            return Err(LeaseError::Expired);
        }
        lease.expires_at_ms = now_ms.saturating_add(ttl_ms);
        Ok(lease.clone())
    }

    /// Release a lease only when the owner and token match.
    pub fn release(
        &mut self,
        session_id: &str,
        owner_id: &str,
        token: &str,
    ) -> Result<(), LeaseError> {
        let lease = self.leases.get(session_id).ok_or(LeaseError::NotFound)?;
        if lease.owner_id != owner_id || lease.token != token {
            return Err(LeaseError::NotOwner);
        }
        self.leases.remove(session_id);
        Ok(())
    }
}

fn validate_lease_identity(value: &str, field: &str) -> Result<(), LeaseError> {
    if value.is_empty() || value.len() > 128 || value.chars().any(char::is_whitespace) {
        return Err(LeaseError::InvalidInput(format!(
            "{field} must be a bounded non-whitespace identifier"
        )));
    }
    Ok(())
}

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
        remove_socket_if_safe(socket)?;
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
        let _ = remove_socket_if_safe(socket);
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

        remove_socket_if_safe(socket)?;
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
        let client_sessions = Arc::new(std::sync::atomic::AtomicU32::new(0));
        loop {
            let (stream, _) = listener.accept().await?;
            let client_sessions = Arc::clone(&client_sessions);
            if client_sessions.load(std::sync::atomic::Ordering::Relaxed)
                >= MAX_DAEMON_CLIENT_SESSIONS
            {
                tracing::warn!("daemon client session limit reached");
                drop(stream);
                continue;
            }
            let active = client_sessions.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;
            update_client_sessions(status_path, active)?;
            let status_path = status_path.to_path_buf();
            tokio::spawn(async move {
                if let Err(error) = bridge_client(stream).await {
                    tracing::warn!(%error, "daemon client bridge stopped");
                }
                let active = client_sessions
                    .fetch_sub(1, std::sync::atomic::Ordering::Relaxed)
                    .saturating_sub(1);
                let _ = update_client_sessions(&status_path, active);
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
fn remove_socket_if_safe(path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    use std::os::unix::fs::FileTypeExt;

    let metadata = match std::fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error.into()),
    };
    if !metadata.file_type().is_socket() {
        return Err(format!(
            "refusing to replace non-socket daemon path {}",
            path.display()
        )
        .into());
    }
    std::fs::remove_file(path)?;
    Ok(())
}

fn update_client_sessions(
    path: &Path,
    client_sessions: u32,
) -> Result<(), Box<dyn std::error::Error>> {
    let Some(mut status) = read_status(path)? else {
        return Ok(());
    };
    status.client_sessions = client_sessions;
    std::fs::write(path, serde_json::to_vec_pretty(&status)?)?;
    Ok(())
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

    #[test]
    fn mutation_leases_are_exclusive_and_owner_bound() {
        let mut manager = MutationLeaseManager::default();
        let lease = manager
            .acquire("session-1", "owner-a", 1_000, 1_000)
            .unwrap();
        assert_eq!(lease.token, "lease-1");
        assert_eq!(
            manager.acquire("session-1", "owner-b", 1_100, 1_000),
            Err(LeaseError::AlreadyHeld)
        );
        assert_eq!(
            manager.renew("session-1", "owner-b", &lease.token, 1_200, 1_000),
            Err(LeaseError::NotOwner)
        );
        manager
            .renew("session-1", "owner-a", &lease.token, 1_200, 1_000)
            .unwrap();
        manager
            .release("session-1", "owner-a", &lease.token)
            .unwrap();
        assert_eq!(
            manager.release("session-1", "owner-a", &lease.token),
            Err(LeaseError::NotFound)
        );
    }

    #[cfg(unix)]
    #[test]
    fn daemon_cleanup_refuses_to_replace_a_regular_file() {
        let path =
            std::env::temp_dir().join(format!("glass-daemon-regular-{}", std::process::id()));
        std::fs::write(&path, b"not a socket").unwrap();
        assert!(remove_socket_if_safe(&path).is_err());
        std::fs::remove_file(path).unwrap();
    }
}
