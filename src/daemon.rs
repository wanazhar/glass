//! Local Unix-socket daemon lifecycle and client bridge.
//!
//! The daemon is deliberately local-only. Each socket client receives an
//! isolated MCP child session, so a daemon restart or client disconnect cannot
//! silently transfer a browser session or workflow lease to another client.

use clap::Parser;
use futures_util::FutureExt;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use crate::cli::args::Cli;

/// Version of the local daemon status and lifecycle contract.
pub const DAEMON_PROTOCOL_VERSION: u32 = 1;
/// Version of the persisted interrupted-run recovery record.
pub const DAEMON_RECOVERY_SCHEMA_VERSION: u32 = 1;
/// Maximum number of isolated MCP child sessions per daemon.
pub const MAX_DAEMON_CLIENT_SESSIONS: u32 = 4;
/// Maximum number of in-flight MCP operations across all daemon clients.
pub const MAX_DAEMON_CONCURRENT_REQUESTS: usize = 16;
/// Maximum number of in-flight MCP operations from one daemon client.
pub const MAX_DAEMON_CLIENT_CONCURRENT_REQUESTS: usize = 4;
/// Maximum number of workflow requests retained in the daemon status record.
pub const MAX_DAEMON_ACTIVE_RUNS: usize = 16;
/// Stable session namespace used by the first shared daemon runtime.
pub const DAEMON_DEFAULT_SESSION_ID: &str = "daemon-default";
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

/// Lease authority and owner identity for one daemon socket connection.
#[derive(Debug, Clone)]
pub struct DaemonLeaseContext {
    pub manager: Arc<tokio::sync::Mutex<MutationLeaseManager>>,
    pub owner_id: String,
    pub request_permits: Arc<tokio::sync::Semaphore>,
    pub client_request_permits: Arc<tokio::sync::Semaphore>,
    pub status: Arc<DaemonStatusState>,
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

    /// Verify that a mutation request is still owned by the given client.
    pub fn validate(
        &self,
        session_id: &str,
        owner_id: &str,
        token: &str,
        now_ms: u64,
    ) -> Result<(), LeaseError> {
        let lease = self.leases.get(session_id).ok_or(LeaseError::NotFound)?;
        if lease.owner_id != owner_id || lease.token != token {
            return Err(LeaseError::NotOwner);
        }
        if lease.expires_at_ms <= now_ms {
            return Err(LeaseError::Expired);
        }
        Ok(())
    }

    /// Release every lease held by a disconnected client owner.
    pub fn release_owner(&mut self, owner_id: &str) {
        self.leases.retain(|_, lease| lease.owner_id != owner_id);
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

/// One workflow request active in the daemon's shared session.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DaemonActiveRun {
    pub request_id: String,
    pub owner_id: String,
    pub started_at: String,
}

/// Persisted evidence that workflow requests need checkpoint reconciliation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DaemonRecovery {
    pub schema_version: u32,
    pub state: String,
    pub recovered_at: String,
    pub runs: Vec<DaemonActiveRun>,
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub log_path: Option<PathBuf>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub recovery_path: Option<PathBuf>,
    pub started_at: String,
    pub transport: String,
    pub client_sessions: u32,
    #[serde(default)]
    pub active_runs: Vec<DaemonActiveRun>,
}

/// Serialized status coordinator shared by daemon clients.
#[derive(Debug)]
pub struct DaemonStatusState {
    path: PathBuf,
    status: tokio::sync::Mutex<DaemonStatus>,
}

impl DaemonStatusState {
    pub fn new(path: &Path, status: DaemonStatus) -> Self {
        Self {
            path: path.to_path_buf(),
            status: tokio::sync::Mutex::new(status),
        }
    }

    pub async fn update_client_sessions(
        &self,
        client_sessions: u32,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let mut status = self.status.lock().await;
        status.client_sessions = client_sessions;
        write_status(&self.path, &status)
    }

    pub async fn begin_workflow(
        &self,
        request_id: &str,
        owner_id: &str,
    ) -> Result<(), Box<dyn std::error::Error>> {
        if request_id.is_empty() || request_id.len() > 128 {
            return Err("workflow request id exceeds the daemon status bound".into());
        }
        let mut status = self.status.lock().await;
        if status.active_runs.len() >= MAX_DAEMON_ACTIVE_RUNS {
            return Err("daemon active workflow limit reached".into());
        }
        if status
            .active_runs
            .iter()
            .any(|run| run.request_id == request_id)
        {
            return Err("daemon workflow request id is already active".into());
        }
        status.active_runs.push(DaemonActiveRun {
            request_id: request_id.into(),
            owner_id: owner_id.into(),
            started_at: chrono::Utc::now().to_rfc3339(),
        });
        write_status(&self.path, &status)
    }

    pub async fn finish_workflow(
        &self,
        request_id: &str,
        owner_id: &str,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let mut status = self.status.lock().await;
        status
            .active_runs
            .retain(|run| run.request_id != request_id || run.owner_id != owner_id);
        write_status(&self.path, &status)
    }

    pub async fn record_interrupted_workflows(&self) -> Result<usize, Box<dyn std::error::Error>> {
        let status = self.status.lock().await;
        write_recovery_report(&self.path, &status.active_runs)?;
        for run in &status.active_runs {
            append_daemon_log(
                &self.path,
                &format!(
                    "interrupted workflow request {} owned by {}; checkpoint reconciliation is required",
                    run.request_id, run.owner_id
                ),
            )?;
        }
        Ok(status.active_runs.len())
    }
}

/// Return the default local daemon paths.
pub fn default_paths() -> (PathBuf, PathBuf) {
    let root = dirs::data_local_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("glass");
    (root.join("glass.sock"), root.join("daemon.json"))
}

fn log_path_for(status_path: &Path) -> PathBuf {
    status_path.with_extension("log")
}

fn recovery_path_for(status_path: &Path) -> PathBuf {
    status_path.with_extension("recovery.json")
}

fn append_daemon_log(status_path: &Path, message: &str) -> Result<(), std::io::Error> {
    use std::io::Write;

    let path = log_path_for(status_path);
    let mut log = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)?;
    writeln!(log, "{message}")
}

fn write_recovery_report(
    status_path: &Path,
    runs: &[DaemonActiveRun],
) -> Result<(), Box<dyn std::error::Error>> {
    if runs.is_empty() {
        return Ok(());
    }
    let report = DaemonRecovery {
        schema_version: DAEMON_RECOVERY_SCHEMA_VERSION,
        state: "reconciliation_required".into(),
        recovered_at: chrono::Utc::now().to_rfc3339(),
        runs: runs.to_vec(),
    };
    std::fs::write(
        recovery_path_for(status_path),
        serde_json::to_vec_pretty(&report)?,
    )?;
    Ok(())
}

fn read_recovery(status_path: &Path) -> Result<Option<DaemonRecovery>, Box<dyn std::error::Error>> {
    let path = recovery_path_for(status_path);
    if !path.is_file() {
        return Ok(None);
    }
    let report: DaemonRecovery = serde_json::from_slice(&std::fs::read(path)?)?;
    if report.schema_version != DAEMON_RECOVERY_SCHEMA_VERSION {
        return Err("unsupported daemon recovery schema".into());
    }
    Ok(Some(report))
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
            write_recovery_report(status_path, &existing.active_runs)?;
            append_daemon_log(
                status_path,
                &format!(
                    "recovered stale daemon pid {}; active workflows are indeterminate and require checkpoint reconciliation",
                    existing.pid
                ),
            )?;
            for run in &existing.active_runs {
                append_daemon_log(
                    status_path,
                    &format!(
                        "recovered interrupted workflow request {} owned by {}; checkpoint reconciliation is required",
                        run.request_id, run.owner_id
                    ),
                )?;
            }
            let _ = std::fs::remove_file(status_path);
            let _ = remove_socket_if_safe(&existing.socket);
        }
        remove_socket_if_safe(socket)?;
        std::fs::create_dir_all(socket.parent().unwrap_or_else(|| Path::new(".")))?;
        std::fs::create_dir_all(status_path.parent().unwrap_or_else(|| Path::new(".")))?;
        let log_path = log_path_for(status_path);
        let log = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&log_path)?;
        let log_stderr = log.try_clone()?;
        let executable = std::env::current_exe()?;
        std::process::Command::new(executable)
            .args(["daemon", "serve"])
            .arg("--socket")
            .arg(socket)
            .arg("--status")
            .arg(status_path)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::from(log))
            .stderr(std::process::Stdio::from(log_stderr))
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
            "recovery": read_recovery(&status.status_path)?,
            "socketExists": status.socket.exists(),
            "pidAlive": process_is_alive(status.pid),
        })),
        Err(error) => Ok(serde_json::json!({
            "status": "unavailable",
            "detail": error.to_string(),
        })),
    }
}

/// Read the persisted interrupted-workflow recovery state.
pub fn recovery(
    status_path: Option<&Path>,
) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
    let (_, default_status) = default_paths();
    let status_path = status_path.unwrap_or(&default_status);
    let path = recovery_path_for(status_path);
    match read_recovery(status_path)? {
        Some(report) => Ok(serde_json::json!({
            "status": "reconciliation_required",
            "path": path,
            "recovery": report,
        })),
        None => Ok(serde_json::json!({
            "status": "clear",
            "path": path,
        })),
    }
}

/// Clear the recovery requirement after the caller reconciles every listed run.
pub fn acknowledge_recovery(
    status_path: Option<&Path>,
    reconciled_request_ids: &[String],
) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
    let (_, default_status) = default_paths();
    let status_path = status_path.unwrap_or(&default_status);
    let path = recovery_path_for(status_path);
    let Some(report) = read_recovery(status_path)? else {
        return Ok(serde_json::json!({
            "status": "clear",
            "path": path,
            "acknowledged": false,
        }));
    };
    let expected: std::collections::BTreeSet<&str> = report
        .runs
        .iter()
        .map(|run| run.request_id.as_str())
        .collect();
    let provided: std::collections::BTreeSet<&str> =
        reconciled_request_ids.iter().map(String::as_str).collect();
    if reconciled_request_ids.len() != provided.len() || expected != provided {
        return Err(format!(
            "recovery acknowledgement must name exactly these request IDs: {}",
            report
                .runs
                .iter()
                .map(|run| run.request_id.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        )
        .into());
    }
    std::fs::remove_file(&path)?;
    Ok(serde_json::json!({
        "status": "acknowledged",
        "path": path,
        "runs": report.runs.len(),
        "reconciledRequestIds": report
            .runs
            .iter()
            .map(|run| run.request_id.as_str())
            .collect::<Vec<_>>(),
        "acknowledged": true,
    }))
}

/// Read a bounded tail of the daemon log without exposing an unbounded file.
pub fn logs(status_path: Option<&Path>) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
    const MAX_LOG_BYTES: usize = 64 * 1024;
    let (_, default_status) = default_paths();
    let status_path = status_path.unwrap_or(&default_status);
    let path = log_path_for(status_path);
    if !path.is_file() {
        return Ok(serde_json::json!({
            "status": "unavailable",
            "path": path,
            "content": "",
            "truncated": false,
        }));
    }
    let bytes = std::fs::read(&path)?;
    let truncated = bytes.len() > MAX_LOG_BYTES;
    let start = bytes.len().saturating_sub(MAX_LOG_BYTES);
    Ok(serde_json::json!({
        "status": "available",
        "path": path,
        "content": String::from_utf8_lossy(&bytes[start..]),
        "truncated": truncated,
    }))
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
        let local = tokio::task::LocalSet::new();
        local.run_until(serve_local(socket, status_path)).await
    }
}

#[cfg(unix)]
async fn serve_local(socket: &Path, status_path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    use std::os::unix::fs::PermissionsExt;
    use tokio::net::UnixListener;
    use tokio::signal::unix::{SignalKind, signal};

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
        log_path: Some(log_path_for(status_path)),
        recovery_path: Some(recovery_path_for(status_path)),
        started_at: chrono::Utc::now().to_rfc3339(),
        transport: "unix-mcp-shared-session".into(),
        client_sessions: 0,
        active_runs: Vec::new(),
    };
    std::fs::write(status_path, serde_json::to_vec_pretty(&status)?)?;
    let status_state = Arc::new(DaemonStatusState::new(status_path, status));
    let client_sessions = Arc::new(std::sync::atomic::AtomicU32::new(0));
    let shared_session = Arc::new(tokio::sync::Mutex::new(None));
    let lease_manager = Arc::new(tokio::sync::Mutex::new(MutationLeaseManager::default()));
    let request_permits = Arc::new(tokio::sync::Semaphore::new(MAX_DAEMON_CONCURRENT_REQUESTS));
    let next_owner_id = std::sync::atomic::AtomicU64::new(0);
    let mut terminate = signal(SignalKind::terminate())?;
    let mut clients = Vec::new();
    loop {
        let (stream, _) = tokio::select! {
            result = listener.accept() => result?,
            _ = terminate.recv() => break,
        };
        if let Err(error) = authorize_local_peer(&stream) {
            tracing::warn!(%error, "daemon rejected unauthorized local peer");
            drop(stream);
            continue;
        }
        let client_sessions = Arc::clone(&client_sessions);
        if client_sessions.load(std::sync::atomic::Ordering::Relaxed) >= MAX_DAEMON_CLIENT_SESSIONS
        {
            tracing::warn!("daemon client session limit reached");
            drop(stream);
            continue;
        }
        let active = client_sessions.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;
        status_state.update_client_sessions(active).await?;
        let status_state = Arc::clone(&status_state);
        let shared_session = Arc::clone(&shared_session);
        let cli = Cli::parse_from(["glass", "--mcp"]);
        let owner_id = format!(
            "daemon-client-{}",
            next_owner_id.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1
        );
        let lease_context = Arc::new(DaemonLeaseContext {
            manager: Arc::clone(&lease_manager),
            owner_id,
            request_permits: Arc::clone(&request_permits),
            client_request_permits: Arc::new(tokio::sync::Semaphore::new(
                MAX_DAEMON_CLIENT_CONCURRENT_REQUESTS,
            )),
            status: Arc::clone(&status_state),
        });
        clients.push(tokio::task::spawn_local(async move {
            let (socket_read, socket_write) = stream.into_split();
            let result = std::panic::AssertUnwindSafe(crate::mcp::server::run_mcp_stream(
                tokio::io::BufReader::new(socket_read),
                socket_write,
                &cli,
                shared_session,
                false,
                true,
                Some(lease_context),
            ))
            .catch_unwind()
            .await;
            match result {
                Ok(Ok(())) => {}
                Ok(Err(error)) => tracing::warn!(%error, "daemon client bridge stopped"),
                Err(panic) => tracing::error!(?panic, "daemon client bridge panicked"),
            }
            let active = client_sessions
                .fetch_sub(1, std::sync::atomic::Ordering::Relaxed)
                .saturating_sub(1);
            let _ = status_state.update_client_sessions(active).await;
        }));
    }
    let _ = status_state.record_interrupted_workflows().await;
    for client in clients {
        client.abort();
    }
    let mut session = shared_session.lock().await;
    if let Some(session) = session.take() {
        let _ = session.close().await;
    }
    let _ = std::fs::remove_file(status_path);
    let _ = remove_socket_if_safe(socket);
    Ok(())
}

#[cfg(unix)]
fn authorize_local_peer(stream: &tokio::net::UnixStream) -> Result<(), Box<dyn std::error::Error>> {
    #[cfg(target_os = "linux")]
    {
        use std::mem::size_of;
        use std::os::fd::AsRawFd;

        let mut credentials = libc::ucred {
            pid: 0,
            uid: 0,
            gid: 0,
        };
        let mut length = size_of::<libc::ucred>() as libc::socklen_t;
        let result = unsafe {
            libc::getsockopt(
                stream.as_raw_fd(),
                libc::SOL_SOCKET,
                libc::SO_PEERCRED,
                (&mut credentials as *mut libc::ucred).cast(),
                &mut length,
            )
        };
        if result != 0 {
            return Err(std::io::Error::last_os_error().into());
        }
        if length as usize != size_of::<libc::ucred>() {
            return Err("unexpected Unix peer credential size".into());
        }
        let current_uid = unsafe { libc::geteuid() };
        if credentials.uid != current_uid {
            return Err(format!(
                "peer uid {} does not match daemon uid {}",
                credentials.uid, current_uid
            )
            .into());
        }
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = stream;
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

fn write_status(path: &Path, status: &DaemonStatus) -> Result<(), Box<dyn std::error::Error>> {
    std::fs::write(path, serde_json::to_vec_pretty(status)?)?;
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
            log_path: None,
            recovery_path: None,
            started_at: "2026-07-28T00:00:00Z".into(),
            transport: "unix-mcp-shared-session".into(),
            client_sessions: 0,
            active_runs: Vec::new(),
        };
        let value = serde_json::to_value(&status).unwrap();

        assert_eq!(value["protocolVersion"], 1);
        assert_eq!(value["transport"], "unix-mcp-shared-session");
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

        let lease = manager
            .acquire("session-1", "owner-a", 2_000, 1_000)
            .unwrap();
        manager
            .validate("session-1", "owner-a", &lease.token, 2_500)
            .unwrap();
        manager.release_owner("owner-a");
        assert_eq!(
            manager.validate("session-1", "owner-a", &lease.token, 2_500),
            Err(LeaseError::NotFound)
        );
    }

    #[tokio::test]
    async fn per_client_request_budget_is_bounded() {
        let permits = Arc::new(tokio::sync::Semaphore::new(
            MAX_DAEMON_CLIENT_CONCURRENT_REQUESTS,
        ));
        let mut held = Vec::new();
        for _ in 0..MAX_DAEMON_CLIENT_CONCURRENT_REQUESTS {
            held.push(Arc::clone(&permits).try_acquire_owned().unwrap());
        }
        assert!(Arc::clone(&permits).try_acquire_owned().is_err());
        drop(held.pop());
        assert!(Arc::clone(&permits).try_acquire_owned().is_ok());
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

    #[cfg(unix)]
    #[tokio::test]
    async fn local_peer_authorization_accepts_same_user_socket() {
        let (stream, _peer) = tokio::net::UnixStream::pair().unwrap();
        authorize_local_peer(&stream).unwrap();
    }

    #[tokio::test]
    async fn active_workflow_status_is_added_and_removed_atomically() {
        let root = std::env::temp_dir().join(format!(
            "glass-daemon-active-run-{}-{}",
            std::process::id(),
            chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let status_path = root.join("daemon.json");
        let status = DaemonStatus {
            protocol_version: DAEMON_PROTOCOL_VERSION,
            state: "running".into(),
            pid: 42,
            socket: root.join("glass.sock"),
            status_path: status_path.clone(),
            log_path: None,
            recovery_path: None,
            started_at: "2026-07-28T00:00:00Z".into(),
            transport: "unix-mcp-shared-session".into(),
            client_sessions: 0,
            active_runs: Vec::new(),
        };
        write_status(&status_path, &status).unwrap();
        let state = DaemonStatusState::new(&status_path, status);

        state
            .begin_workflow("workflow-1", "daemon-client-1")
            .await
            .unwrap();
        let active: DaemonStatus =
            serde_json::from_slice(&std::fs::read(&status_path).unwrap()).unwrap();
        assert_eq!(active.active_runs[0].request_id, "workflow-1");
        assert_eq!(state.record_interrupted_workflows().await.unwrap(), 1);
        let recovery_log = std::fs::read_to_string(log_path_for(&status_path)).unwrap();
        assert!(recovery_log.contains("workflow-1"));

        state
            .finish_workflow("workflow-1", "daemon-client-1")
            .await
            .unwrap();
        let finished: DaemonStatus =
            serde_json::from_slice(&std::fs::read(&status_path).unwrap()).unwrap();
        assert!(finished.active_runs.is_empty());

        std::fs::remove_file(status_path).unwrap();
        std::fs::remove_file(log_path_for(&root.join("daemon.json"))).unwrap();
        std::fs::remove_file(recovery_path_for(&root.join("daemon.json"))).unwrap();
        std::fs::remove_dir(root).unwrap();
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn stale_status_cleanup_refuses_a_regular_recorded_socket() {
        let root = std::env::temp_dir().join(format!("glass-daemon-stale-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        let socket = root.join("glass.sock");
        let status_path = root.join("daemon.json");
        std::fs::write(&socket, b"not a socket").unwrap();
        let status = DaemonStatus {
            protocol_version: DAEMON_PROTOCOL_VERSION,
            state: "running".into(),
            pid: std::process::id().saturating_add(1_000_000),
            socket: socket.clone(),
            status_path: status_path.clone(),
            log_path: None,
            recovery_path: None,
            started_at: "2026-07-28T00:00:00Z".into(),
            transport: "unix-mcp-shared-session".into(),
            client_sessions: 0,
            active_runs: Vec::new(),
        };
        std::fs::write(&status_path, serde_json::to_vec(&status).unwrap()).unwrap();

        let result = start(Some(&socket), Some(&status_path)).await;

        assert!(result.is_err());
        assert!(socket.is_file());
        assert!(!status_path.exists());
        std::fs::remove_file(socket).unwrap();
        std::fs::remove_file(log_path_for(&status_path)).unwrap();
        std::fs::remove_dir(root).unwrap();
    }

    #[test]
    fn daemon_logs_return_only_a_bounded_tail() {
        let root = std::env::temp_dir().join(format!("glass-daemon-logs-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        let status_path = root.join("daemon.json");
        let log_path = log_path_for(&status_path);
        std::fs::write(&log_path, vec![b'x'; 70_000]).unwrap();

        let value = logs(Some(&status_path)).unwrap();

        assert_eq!(value["status"], "available");
        assert_eq!(value["truncated"], true);
        assert_eq!(value["content"].as_str().unwrap().len(), 64 * 1024);
        std::fs::remove_file(log_path).unwrap();
        std::fs::remove_dir(root).unwrap();
    }
}
