//! Persistent local browser sessions for CLI and terminal clients.
//!
//! A session owner process keeps the [`BrowserSession`] and Chrome child alive
//! between CLI invocations. Clients attach through the verified loopback CDP
//! port; the owner is the only process allowed to close the owned browser.

use super::policy::BrowserPolicy;
use super::session::{BrowserResult, BrowserSession, SessionOptions};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

const SESSION_SCHEMA_VERSION: u32 = 1;
const MAX_SESSION_NAME_BYTES: usize = 64;
const MAX_STATUS_BYTES: usize = 32 * 1024;
const START_TIMEOUT: Duration = Duration::from_secs(20);
const START_POLL: Duration = Duration::from_millis(25);

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PersistentSessionRecord {
    pub schema_version: u32,
    pub name: String,
    pub state: String,
    pub pid: u32,
    pub browser_pid: u32,
    pub port: u16,
    pub profile: String,
    pub headed: bool,
    pub socket: PathBuf,
    pub status_path: PathBuf,
    pub started_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Clone)]
pub struct PersistentSessionConfig {
    pub name: String,
    pub port: u16,
    pub profile: String,
    pub headed: bool,
    pub chrome_path: Option<PathBuf>,
    pub policy_args: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct PersistentSessionPaths {
    pub socket: PathBuf,
    pub status: PathBuf,
}
#[derive(Debug, Clone)]
pub struct PersistentSessionServeConfig {
    pub name: String,
    pub socket: PathBuf,
    pub status_path: PathBuf,
    pub port: u16,
    pub profile: String,
    pub headed: bool,
    pub chrome_path: Option<PathBuf>,
    pub policy: BrowserPolicy,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct SessionStatusView {
    name: String,
    state: String,
    port: Option<u16>,
    profile: Option<String>,
    pid: Option<u32>,
    browser_pid: Option<u32>,
    headed: Option<bool>,
    started_at: Option<String>,
    socket: Option<PathBuf>,
    error: Option<String>,
}

#[derive(Debug, Deserialize)]
struct SessionRequest {
    op: String,
}

pub fn validate_name(name: &str) -> BrowserResult<()> {
    if name.is_empty() || name.len() > MAX_SESSION_NAME_BYTES {
        return Err(format!("session name must be 1..{MAX_SESSION_NAME_BYTES} bytes").into());
    }
    if name == "."
        || name == ".."
        || !name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
    {
        return Err("session name may contain only ASCII letters, digits, '-', '_', or '.'".into());
    }
    Ok(())
}

pub fn session_root() -> PathBuf {
    std::env::var_os("GLASS_CONFIG_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| dirs::data_local_dir().unwrap_or_else(|| PathBuf::from(".")))
        .join("glass")
        .join("sessions")
}

pub fn paths(name: &str) -> BrowserResult<PersistentSessionPaths> {
    validate_name(name)?;
    let root = session_root();
    Ok(PersistentSessionPaths {
        socket: root.join(format!("{name}.sock")),
        status: root.join(format!("{name}.json")),
    })
}

pub fn read_record(name: &str) -> BrowserResult<Option<PersistentSessionRecord>> {
    let paths = paths(name)?;
    let bytes = match std::fs::read(&paths.status) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.into()),
    };
    if bytes.len() > MAX_STATUS_BYTES {
        return Err(format!("session status exceeds {MAX_STATUS_BYTES} bytes").into());
    }
    let record: PersistentSessionRecord = serde_json::from_slice(&bytes)?;
    if record.schema_version != SESSION_SCHEMA_VERSION {
        return Err("unsupported persistent session status schema".into());
    }
    if record.name != name || record.socket != paths.socket || record.status_path != paths.status {
        return Err("persistent session status identity does not match its name".into());
    }
    Ok(Some(record))
}

pub fn status(name: &str) -> BrowserResult<serde_json::Value> {
    let record = read_record(name)?;
    let view = match record {
        Some(record) => {
            let server_alive = process_is_alive(record.pid);
            let browser_alive = process_is_alive(record.browser_pid);
            let state = if record.state == "running" && server_alive && browser_alive {
                "running"
            } else if record.state == "failed" {
                "failed"
            } else {
                "stale"
            };
            SessionStatusView {
                name: record.name,
                state: state.into(),
                port: Some(record.port),
                profile: Some(record.profile),
                pid: Some(record.pid),
                browser_pid: Some(record.browser_pid),
                headed: Some(record.headed),
                started_at: Some(record.started_at),
                socket: Some(record.socket),
                error: record.error,
            }
        }
        None => SessionStatusView {
            name: name.into(),
            state: "stopped".into(),
            port: None,
            profile: None,
            pid: None,
            browser_pid: None,
            headed: None,
            started_at: None,
            socket: None,
            error: None,
        },
    };
    Ok(serde_json::to_value(view)?)
}

pub async fn start(config: PersistentSessionConfig) -> BrowserResult<PersistentSessionRecord> {
    validate_name(&config.name)?;
    if config.port == 0 {
        return Err("persistent session port must be non-zero".into());
    }
    let paths = paths(&config.name)?;
    if let Some(existing) = read_record(&config.name)? {
        if process_is_alive(existing.pid) {
            return Err(format!(
                "persistent session `{}` is already running as pid {} on port {}",
                existing.name, existing.pid, existing.port
            )
            .into());
        }
        remove_stale_artifacts(&existing)?;
    }
    std::fs::create_dir_all(session_root())?;
    remove_socket_if_safe(&paths.socket)?;
    let executable = std::env::current_exe()?;
    let mut command = std::process::Command::new(executable);
    command
        .arg("--policy")
        .arg(
            config
                .policy_args
                .first()
                .cloned()
                .unwrap_or_else(|| "development".into()),
        )
        .arg("--profile")
        .arg(&config.profile)
        .arg("--port")
        .arg(config.port.to_string());
    if config.headed {
        command.arg("--headed");
    }
    if let Some(chrome_path) = &config.chrome_path {
        command.arg("--chrome-path").arg(chrome_path);
    }
    for argument in config.policy_args.iter().skip(1) {
        command.arg(argument);
    }
    command
        .arg("session")
        .arg("serve")
        .arg(&config.name)
        .arg("--socket")
        .arg(&paths.socket)
        .arg("--status")
        .arg(&paths.status)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()?;

    let deadline = Instant::now() + START_TIMEOUT;
    loop {
        if let Some(record) = read_record(&config.name)? {
            if record.state == "failed" {
                return Err(record
                    .error
                    .unwrap_or_else(|| "persistent browser session failed to start".into())
                    .into());
            }
            if record.state == "running" && process_is_alive(record.pid) {
                return Ok(record);
            }
        }
        if Instant::now() >= deadline {
            return Err(format!(
                "persistent session `{}` did not become ready within {} seconds",
                config.name,
                START_TIMEOUT.as_secs()
            )
            .into());
        }
        tokio::time::sleep(START_POLL).await;
    }
}

pub async fn stop(name: &str) -> BrowserResult<serde_json::Value> {
    let Some(record) = read_record(name)? else {
        return status(name);
    };
    if !process_is_alive(record.pid) {
        remove_stale_artifacts(&record)?;
        return status(name);
    }
    let response = send_request(&record.socket, "stop").await?;
    let deadline = Instant::now() + START_TIMEOUT;
    while Instant::now() < deadline {
        if read_record(name)?.is_none() || !process_is_alive(record.pid) {
            return Ok(response);
        }
        tokio::time::sleep(START_POLL).await;
    }
    Err(
        format!("persistent session `{name}` did not stop; inspect `glass session status {name}`")
            .into(),
    )
}

pub fn open_message(name: &str) -> BrowserResult<String> {
    let value = status(name)?;
    if value.get("state").and_then(serde_json::Value::as_str) != Some("running") {
        return Err(format!(
            "persistent session `{name}` is not running; start it with `glass session start {name}`"
        )
        .into());
    }
    let port = value
        .get("port")
        .and_then(serde_json::Value::as_u64)
        .ok_or("persistent session has no verified port")?;
    Ok(format!(
        "Session `{name}` is ready on loopback port {port}.\n\nAttach one command:\n  glass --session {name} observe --level interactive\n\nLaunch the browser terminal:\n  glass browser --session {name}"
    ))
}

pub async fn serve(config: PersistentSessionServeConfig) -> BrowserResult<()> {
    validate_name(&config.name)?;
    if config.port == 0 {
        return Err("persistent session port must be non-zero".into());
    }
    #[cfg(not(unix))]
    {
        let _ = config;
        return Err("persistent browser sessions require a Unix local socket".into());
    }
    #[cfg(unix)]
    {
        serve_unix(config).await
    }
}

#[cfg(unix)]
async fn serve_unix(config: PersistentSessionServeConfig) -> BrowserResult<()> {
    use std::os::unix::fs::PermissionsExt;
    use tokio::net::UnixListener;

    let PersistentSessionServeConfig {
        name,
        socket,
        status_path,
        port,
        profile,
        headed,
        chrome_path,
        policy,
    } = config;

    remove_socket_if_safe(&socket)?;
    if let Some(parent) = socket.parent() {
        std::fs::create_dir_all(parent)?;
    }
    if let Some(parent) = status_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let listener = UnixListener::bind(&socket)?;
    std::fs::set_permissions(&socket, std::fs::Permissions::from_mode(0o600))?;

    let options = SessionOptions {
        port,
        profile: profile.clone(),
        chrome_path,
        headed,
        policy: Some(policy),
        ..SessionOptions::default()
    };

    let session = match BrowserSession::start(&options).await {
        Ok(session) => session,
        Err(error) => {
            let failed = PersistentSessionRecord {
                schema_version: SESSION_SCHEMA_VERSION,
                name,
                state: "failed".into(),
                pid: std::process::id(),
                browser_pid: 0,
                port,
                profile: profile.clone(),
                headed,
                socket: socket.to_path_buf(),
                status_path: status_path.to_path_buf(),
                started_at: chrono::Utc::now().to_rfc3339(),
                error: Some(error.to_string()),
            };
            write_record(&status_path, &failed)?;
            return Err(error);
        }
    };
    let record = PersistentSessionRecord {
        schema_version: SESSION_SCHEMA_VERSION,
        name,
        state: "running".into(),
        pid: std::process::id(),
        browser_pid: session.owned_chrome_pid().unwrap_or_default(),
        port,
        profile,
        headed,
        socket: socket.to_path_buf(),
        status_path: status_path.to_path_buf(),
        started_at: chrono::Utc::now().to_rfc3339(),
        error: None,
    };
    write_record(&status_path, &record)?;

    let mut session = Some(session);
    let mut shutdown = false;
    let mut lines = String::new();
    while !shutdown {
        tokio::select! {
            accepted = listener.accept() => {
                let (stream, _) = accepted?;
                let (read, mut write) = stream.into_split();
                let mut reader = BufReader::new(read);
                lines.clear();
                reader.read_line(&mut lines).await?;
                let request = serde_json::from_str::<SessionRequest>(lines.trim())
                    .map_err(|error| format!("invalid session request: {error}"))?;
                let response = match request.op.as_str() {
                    "status" => serde_json::to_value(&record)?,
                    "stop" => {
                        shutdown = true;
                        json!({"ok": true, "state": "stopping"})
                    }
                    _ => json!({"ok": false, "error": "unknown session operation"}),
                };
                write.write_all(serde_json::to_string(&response)?.as_bytes()).await?;
                write.write_all(b"\n").await?;
            }
            _ = tokio::signal::ctrl_c() => {
                shutdown = true;
            }
        }
    }
    if let Some(session) = session.take() {
        let _ = session.close().await;
    }
    let _ = std::fs::remove_file(status_path);
    remove_socket_if_safe(&socket)?;
    Ok(())
}

async fn send_request(socket: &Path, op: &str) -> BrowserResult<serde_json::Value> {
    #[cfg(not(unix))]
    {
        let _ = (socket, op);
        return Err("persistent browser sessions require a Unix local socket".into());
    }
    #[cfg(unix)]
    {
        use tokio::net::UnixStream;
        let stream = UnixStream::connect(socket).await?;
        let (read, mut write) = stream.into_split();
        write
            .write_all(serde_json::to_string(&json!({"op": op}))?.as_bytes())
            .await?;
        write.write_all(b"\n").await?;
        let mut reader = BufReader::new(read);
        let mut line = String::new();
        reader.read_line(&mut line).await?;
        let value: serde_json::Value = serde_json::from_str(line.trim())?;
        if value.get("ok") == Some(&serde_json::Value::Bool(false)) {
            return Err(value
                .get("error")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("persistent session request failed")
                .into());
        }
        Ok(value)
    }
}

fn write_record(path: &Path, record: &PersistentSessionRecord) -> BrowserResult<()> {
    let bytes = serde_json::to_vec_pretty(record)?;
    if bytes.len() > MAX_STATUS_BYTES {
        return Err("persistent session status exceeds its size bound".into());
    }
    let temporary = path.with_extension(format!("json.tmp-{}", std::process::id()));
    std::fs::write(&temporary, bytes)?;
    std::fs::rename(temporary, path)?;
    Ok(())
}

fn remove_stale_artifacts(record: &PersistentSessionRecord) -> BrowserResult<()> {
    let _ = std::fs::remove_file(&record.status_path);
    remove_socket_if_safe(&record.socket)
}

fn remove_socket_if_safe(path: &Path) -> BrowserResult<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::FileTypeExt;
        match std::fs::symlink_metadata(path) {
            Ok(metadata) if metadata.file_type().is_socket() => {
                std::fs::remove_file(path)?;
            }
            Ok(_) => {
                return Err(format!(
                    "refusing to replace non-socket session path {}",
                    path.display()
                )
                .into());
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
    }
    #[cfg(not(unix))]
    {
        let _ = path;
    }
    Ok(())
}

#[cfg(unix)]
fn process_is_alive(pid: u32) -> bool {
    pid != 0 && unsafe { libc::kill(pid as libc::pid_t, 0) == 0 }
}

#[cfg(not(unix))]
fn process_is_alive(_pid: u32) -> bool {
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_names_are_path_safe() {
        assert!(validate_name("default").is_ok());
        assert!(validate_name("team-1.dev").is_ok());
        assert!(validate_name("../escape").is_err());
        assert!(validate_name("a/b").is_err());
    }

    #[test]
    fn stopped_status_is_explicit_and_browser_free() {
        let value = status("missing-test-session").unwrap();
        assert_eq!(value["state"], "stopped");
        assert_eq!(value["name"], "missing-test-session");
        assert!(value.get("port").is_none_or(serde_json::Value::is_null));
    }
}
