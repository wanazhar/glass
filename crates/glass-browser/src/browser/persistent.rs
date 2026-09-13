//! Persistent local browser sessions for CLI and terminal clients.
//!
//! A session owner process keeps the [`BrowserSession`] and Chrome child alive
//! between CLI invocations. Clients attach through the verified loopback CDP
//! port; the owner is the only process allowed to close the owned browser.

use super::policy::BrowserPolicy;
use super::runtime::BrowserRuntime;
use super::session::{BrowserResult, BrowserSession, SessionOptions};
#[cfg(feature = "native-engine")]
use super::{BrowserRuntimeSession, NativeEngineConfig, NativeHistoryDirection};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

const SESSION_SCHEMA_VERSION: u32 = 1;
const MAX_SESSION_NAME_BYTES: usize = 64;
const MAX_STATUS_BYTES: usize = 32 * 1024;
const MAX_SESSION_REQUEST_BYTES: usize = 256 * 1024;
const START_TIMEOUT: Duration = Duration::from_secs(20);
const START_POLL: Duration = Duration::from_millis(25);

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PersistentSessionRecord {
    pub schema_version: u32,
    pub name: String,
    /// Runtime owned by this session. Missing values in schema-v1 records are
    /// treated as Chromium for backwards-compatible status inspection.
    #[serde(default = "default_runtime")]
    pub runtime: BrowserRuntime,
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
    pub runtime: BrowserRuntime,
    pub port: u16,
    pub profile: String,
    pub headed: bool,
    pub chrome_path: Option<PathBuf>,
    pub policy_args: Vec<String>,
    #[cfg(feature = "native-engine")]
    pub native_config: Option<NativeEngineConfig>,
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
    pub runtime: BrowserRuntime,
    pub port: u16,
    pub profile: String,
    pub headed: bool,
    pub chrome_path: Option<PathBuf>,
    pub policy: BrowserPolicy,
    #[cfg(feature = "native-engine")]
    pub native_config: Option<NativeEngineConfig>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct SessionStatusView {
    name: String,
    runtime: String,
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

#[derive(Debug, Serialize, Deserialize)]
struct SessionRequest {
    op: String,
    #[serde(default)]
    argv: Vec<String>,
    #[serde(default)]
    control: Option<NativeControlRequest>,
    #[serde(default)]
    mcp_params: Option<serde_json::Value>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct NativeControlRequest {
    action: String,
    expected_revision: u64,
}

fn default_runtime() -> BrowserRuntime {
    BrowserRuntime::Chromium
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
            let browser_alive = if record.runtime.is_native() {
                // Native sessions keep their browser state in the owner
                // process; they do not have a Chrome child PID.
                server_alive
            } else {
                process_is_alive(record.browser_pid)
            };
            let state = if record.state == "running" && server_alive && browser_alive {
                "running"
            } else if record.state == "failed" {
                "failed"
            } else {
                "stale"
            };
            SessionStatusView {
                name: record.name,
                runtime: record.runtime.browser_family().into(),
                state: state.into(),
                port: (record.port != 0).then_some(record.port),
                profile: Some(record.profile),
                pid: Some(record.pid),
                browser_pid: (record.browser_pid != 0).then_some(record.browser_pid),
                headed: Some(record.headed),
                started_at: Some(record.started_at),
                socket: Some(record.socket),
                error: record.error,
            }
        }
        None => SessionStatusView {
            name: name.into(),
            runtime: default_runtime().browser_family().into(),
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
    if config.runtime == BrowserRuntime::Chromium && config.port == 0 {
        return Err("persistent session port must be non-zero".into());
    }
    if !matches!(config.runtime, BrowserRuntime::Chromium) && !config.runtime.is_native() {
        return Err("persistent sessions support only Chromium and native runtimes".into());
    }
    let paths = paths(&config.name)?;
    if let Some(existing) = read_record(&config.name)? {
        if process_is_alive(existing.pid) {
            return Err(format!(
                "persistent session `{}` is already running as pid {} ({})",
                existing.name,
                existing.pid,
                existing.runtime.browser_family()
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
    if config.runtime != BrowserRuntime::Chromium {
        command
            .arg("--browser-runtime")
            .arg(config.runtime.browser_family());
    }
    #[cfg(feature = "native-engine")]
    if let Some(native_config) = &config.native_config {
        if native_config.storage_path.is_none() {
            command.arg("--incognito");
        }
        command.arg("--viewport").arg(format!(
            "{}x{}",
            native_config.viewport.width, native_config.viewport.height
        ));
    }
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
    if value.get("runtime").and_then(serde_json::Value::as_str) == Some("native") {
        return Ok(format!(
            "Native session `{name}` is ready.\n\nAttach one command:\n  glass --browser-runtime native --session {name} observe\n\nThe owner keeps the native browser state alive between commands."
        ));
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
    if config.runtime == BrowserRuntime::Chromium && config.port == 0 {
        return Err("persistent session port must be non-zero".into());
    }
    if !matches!(config.runtime, BrowserRuntime::Chromium) && !config.runtime.is_native() {
        return Err("persistent sessions support only Chromium and native runtimes".into());
    }
    #[cfg(not(unix))]
    {
        let _ = config;
        return Err("persistent browser sessions require a Unix local socket".into());
    }
    #[cfg(unix)]
    {
        if config.runtime.is_native() {
            #[cfg(feature = "native-engine")]
            {
                return serve_native_unix(config).await;
            }
            #[cfg(not(feature = "native-engine"))]
            {
                return Err("native runtime support is not enabled in this build".into());
            }
        }
        serve_chromium_unix(config).await
    }
}

#[cfg(unix)]
async fn serve_chromium_unix(config: PersistentSessionServeConfig) -> BrowserResult<()> {
    use std::os::unix::fs::PermissionsExt;
    use tokio::net::UnixListener;

    let PersistentSessionServeConfig {
        name,
        socket,
        status_path,
        runtime,
        port,
        profile,
        headed,
        chrome_path,
        policy,
        #[cfg(feature = "native-engine")]
            native_config: _,
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
                runtime,
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
        runtime,
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
                if lines.len() > MAX_SESSION_REQUEST_BYTES {
                    return Err("persistent session request exceeds its size bound".into());
                }
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

#[cfg(all(unix, feature = "native-engine"))]
async fn serve_native_unix(config: PersistentSessionServeConfig) -> BrowserResult<()> {
    use std::os::unix::fs::PermissionsExt;
    use tokio::net::UnixListener;

    let PersistentSessionServeConfig {
        name,
        socket,
        status_path,
        runtime,
        port,
        policy,
        native_config,
        profile,
        headed,
        chrome_path: _,
    } = config;
    let native_config =
        native_config.ok_or("native persistent session is missing its engine configuration")?;

    remove_socket_if_safe(&socket)?;
    if let Some(parent) = socket.parent() {
        std::fs::create_dir_all(parent)?;
    }
    if let Some(parent) = status_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let listener = UnixListener::bind(&socket)?;
    std::fs::set_permissions(&socket, std::fs::Permissions::from_mode(0o600))?;

    let session = match BrowserRuntimeSession::connect_native(native_config).await {
        Ok(session) => session,
        Err(error) => {
            let failed = PersistentSessionRecord {
                schema_version: SESSION_SCHEMA_VERSION,
                name,
                runtime,
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
        runtime,
        state: "running".into(),
        pid: std::process::id(),
        browser_pid: 0,
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
    while !shutdown {
        tokio::select! {
            accepted = listener.accept() => {
                let (stream, _) = accepted?;
                let (read, mut write) = stream.into_split();
                let mut reader = BufReader::new(read);
                let mut line = String::new();
                reader.read_line(&mut line).await?;
                if line.len() > MAX_SESSION_REQUEST_BYTES {
                    return Err("persistent session request exceeds its size bound".into());
                }
                let request = serde_json::from_str::<SessionRequest>(line.trim())
                    .map_err(|error| format!("invalid session request: {error}"))?;
                let response = match request.op.as_str() {
                    "status" => serde_json::to_value(&record)?,
                    "stop" => {
                        shutdown = true;
                        json!({"ok": true, "state": "stopping"})
                    }
                    "execute" => {
                        let session = session
                            .as_ref()
                            .ok_or("native persistent session is stopping")?;
                        match crate::cli::runner::run_native_persistent_request(
                            session,
                            request.argv,
                            &record.profile,
                            &policy,
                        )
                        .await
                        {
                            Ok(output) => json!({"ok": true, "output": output}),
                            Err(error) => json!({"ok": false, "error": error.to_string()}),
                        }
                    }
                    "control" => {
                        let request = request
                            .control
                            .ok_or("native control request is missing its payload")?;
                        let session = session
                            .as_ref()
                            .ok_or("native persistent session is stopping")?;
                        let actual_revision = session
                            .evidence(crate::browser_backend::EvidenceLevel::Compact)
                            .await?
                            .revision;
                        if actual_revision != request.expected_revision {
                            return Err(format!(
                                "stale browser revision: expected {}, observed {actual_revision}",
                                request.expected_revision
                            )
                            .into());
                        }
                        let outcome = match request.action.as_str() {
                            "back" => session
                                .native_navigate_history(NativeHistoryDirection::Back)
                                .await?,
                            "forward" => session
                                .native_navigate_history(NativeHistoryDirection::Forward)
                                .await?,
                            "reload" => {
                                session.script("location.reload()").await?;
                                let revision = session
                                    .evidence(crate::browser_backend::EvidenceLevel::Compact)
                                    .await?
                                    .revision;
                                crate::browser::session::NavigationControlOutcome {
                                    action: "reload".into(),
                                    previous_revision: request.expected_revision,
                                    current_revision: revision,
                                }
                            }
                            "stopLoading" => crate::browser::session::NavigationControlOutcome {
                                action: "stopLoading".into(),
                                previous_revision: request.expected_revision,
                                current_revision: actual_revision,
                            },
                            _ => {
                                return Err(format!(
                                    "unsupported native session control `{}`",
                                    request.action
                                )
                                .into());
                            }
                        };
                        json!({
                            "ok": true,
                            "action": outcome.action,
                            "currentRevision": outcome.current_revision,
                        })
                    }
                    "mcp" => {
                        let params = request
                            .mcp_params
                            .ok_or("native MCP request is missing its params")?;
                        let session = session
                            .as_ref()
                            .ok_or("native persistent session is stopping")?;
                        match crate::mcp::server::run_native_persistent_tool(
                            params,
                            session,
                            &record.profile,
                            &policy,
                        )
                        .await
                        {
                            Ok(result) => json!({"ok": true, "result": result}),
                            Err(error) => json!({"ok": false, "error": error.to_string()}),
                        }
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
    send_request_payload(
        socket,
        &SessionRequest {
            op: op.to_owned(),
            argv: Vec::new(),
            control: None,
            mcp_params: None,
        },
    )
    .await
}

#[cfg(feature = "native-engine")]
pub async fn execute_native(name: &str, argv: Vec<String>) -> BrowserResult<serde_json::Value> {
    let Some(record) = read_record(name)? else {
        return Err(format!("persistent session `{name}` is not running; start it first").into());
    };
    if !record.runtime.is_native() {
        return Err(format!("persistent session `{name}` is not a native session").into());
    }
    if !process_is_alive(record.pid) {
        return Err(format!("persistent session `{name}` is stale; restart it first").into());
    }
    let argv = with_owner_profile(argv, &record.profile);
    send_request_payload(
        &record.socket,
        &SessionRequest {
            op: "execute".into(),
            argv,
            control: None,
            mcp_params: None,
        },
    )
    .await
}

#[cfg(feature = "native-engine")]
pub async fn control_native(
    name: &str,
    action: &str,
    expected_revision: u64,
) -> BrowserResult<serde_json::Value> {
    if !matches!(action, "back" | "forward" | "reload" | "stopLoading") {
        return Err(format!("unsupported native session control `{action}`").into());
    }
    let Some(record) = read_record(name)? else {
        return Err(format!("persistent session `{name}` is not running; start it first").into());
    };
    if !record.runtime.is_native() {
        return Err(format!("persistent session `{name}` is not a native session").into());
    }
    if !process_is_alive(record.pid) {
        return Err(format!("persistent session `{name}` is stale; restart it first").into());
    }
    send_request_payload(
        &record.socket,
        &SessionRequest {
            op: "control".into(),
            argv: Vec::new(),
            control: Some(NativeControlRequest {
                action: action.into(),
                expected_revision,
            }),
            mcp_params: None,
        },
    )
    .await
}

#[cfg(feature = "native-engine")]
pub async fn execute_native_mcp(
    name: &str,
    params: serde_json::Value,
) -> BrowserResult<serde_json::Value> {
    let Some(record) = read_record(name)? else {
        return Err(format!("persistent session `{name}` is not running; start it first").into());
    };
    if !record.runtime.is_native() {
        return Err(format!("persistent session `{name}` is not a native session").into());
    }
    if !process_is_alive(record.pid) {
        return Err(format!("persistent session `{name}` is stale; restart it first").into());
    }
    let response = send_request_payload(
        &record.socket,
        &SessionRequest {
            op: "mcp".into(),
            argv: Vec::new(),
            control: None,
            mcp_params: Some(params),
        },
    )
    .await?;
    response
        .get("result")
        .cloned()
        .ok_or_else(|| "native persistent MCP request returned no result".into())
}

#[cfg(feature = "native-engine")]
fn with_owner_profile(mut argv: Vec<String>, owner_profile: &str) -> Vec<String> {
    let has_profile = argv
        .iter()
        .any(|argument| argument == "--profile" || argument.starts_with("--profile="));
    if !has_profile {
        argv.splice(0..0, ["--profile".to_owned(), owner_profile.to_owned()]);
    }
    argv
}

async fn send_request_payload(
    socket: &Path,
    request: &SessionRequest,
) -> BrowserResult<serde_json::Value> {
    #[cfg(not(unix))]
    {
        let _ = (socket, request);
        return Err("persistent browser sessions require a Unix local socket".into());
    }
    #[cfg(unix)]
    {
        use tokio::net::UnixStream;
        let stream = UnixStream::connect(socket).await?;
        let (read, mut write) = stream.into_split();
        let payload = serde_json::to_vec(request)?;
        if payload.len() > MAX_SESSION_REQUEST_BYTES {
            return Err("persistent session request exceeds its size bound".into());
        }
        write.write_all(&payload).await?;
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

    #[cfg(feature = "native-engine")]
    #[test]
    fn native_owner_profile_is_injected_only_when_not_explicit() {
        assert_eq!(
            with_owner_profile(vec!["observe".into()], "work"),
            vec!["--profile", "work", "observe"]
        );
        assert_eq!(
            with_owner_profile(
                vec!["--profile".into(), "other".into(), "observe".into()],
                "work"
            ),
            vec!["--profile", "other", "observe"]
        );
        assert_eq!(
            with_owner_profile(vec!["--profile=other".into(), "observe".into()], "work"),
            vec!["--profile=other", "observe"]
        );
    }

    #[cfg(all(unix, feature = "native-engine"))]
    #[test]
    fn native_owner_keeps_one_engine_alive_for_multiple_ipc_commands() {
        std::thread::Builder::new()
            .name("glass-native-session-test".into())
            .stack_size(8 * 1024 * 1024)
            .spawn(|| {
                let runtime = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .expect("native persistent test runtime should build");
                runtime.block_on(tokio::task::LocalSet::new().run_until(async {
                    let root = std::env::temp_dir().join(format!(
                        "glass-native-session-test-{}-{}",
                        std::process::id(),
                        chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default()
                    ));
                    std::fs::create_dir_all(&root).unwrap();
                    let socket = root.join("session.sock");
                    let status_path = root.join("session.json");
                    let policy =
                        BrowserPolicy::development(std::env::current_dir().unwrap()).unwrap();
                    let server = tokio::task::spawn_local(serve(PersistentSessionServeConfig {
                        name: "native-test".into(),
                        socket: socket.clone(),
                        status_path: status_path.clone(),
                        runtime: BrowserRuntime::Native,
                        port: 0,
                        profile: "native-test".into(),
                        headed: false,
                        chrome_path: None,
                        policy,
                        native_config: Some(
                            NativeEngineConfig::default()
                                .with_storage_path(root.join("storage.json")),
                        ),
                    }));

                    let record = tokio::time::timeout(Duration::from_secs(10), async {
                        loop {
                            if let Ok(bytes) = tokio::fs::read(&status_path).await {
                                let record: PersistentSessionRecord =
                                    serde_json::from_slice(&bytes).unwrap();
                                if record.state == "running" {
                                    break record;
                                }
                            }
                            tokio::time::sleep(Duration::from_millis(25)).await;
                        }
                    })
                    .await
                    .expect("native persistent owner did not become ready");
                    assert_eq!(record.runtime, BrowserRuntime::Native);
                    assert_eq!(record.browser_pid, 0);

                    let request = SessionRequest {
                        op: "execute".into(),
                        argv: vec![
                            "--browser-runtime".into(),
                            "native".into(),
                            "--profile".into(),
                            "native-test".into(),
                            "observe".into(),
                        ],
                        control: None,
                        mcp_params: None,
                    };
                    let first = send_request_payload(&socket, &request).await.unwrap();
                    assert_eq!(first["ok"], true);
                    assert_eq!(first["output"]["kind"], "json");
                    let revision = first["output"]["value"]["revision"]
                        .as_u64()
                        .expect("native owner observation should expose a revision");
                    let control = SessionRequest {
                        op: "control".into(),
                        argv: Vec::new(),
                        control: Some(NativeControlRequest {
                            action: "stopLoading".into(),
                            expected_revision: revision,
                        }),
                        mcp_params: None,
                    };
                    let control_result = send_request_payload(&socket, &control).await.unwrap();
                    assert_eq!(control_result["ok"], true);
                    assert_eq!(control_result["currentRevision"], revision);
                    let mcp_request = SessionRequest {
                        op: "mcp".into(),
                        argv: Vec::new(),
                        control: None,
                        mcp_params: Some(json!({
                            "name": "observe",
                            "arguments": {}
                        })),
                    };
                    let mcp_result = send_request_payload(&socket, &mcp_request).await.unwrap();
                    assert_eq!(mcp_result["ok"], true);
                    assert!(mcp_result["result"]["content"].is_array());
                    let owner_pid = record.pid;

                    let second = send_request_payload(&socket, &request).await.unwrap();
                    assert_eq!(second["ok"], true);
                    assert_eq!(read_record_from_path(&status_path).unwrap().pid, owner_pid);

                    let stopped = send_request(&socket, "stop").await.unwrap();
                    assert_eq!(stopped["state"], "stopping");
                    tokio::time::timeout(Duration::from_secs(10), server)
                        .await
                        .expect("native persistent owner did not stop")
                        .expect("native persistent owner task panicked")
                        .expect("native persistent owner failed");
                    assert!(!status_path.exists());
                    assert!(!socket.exists());
                    std::fs::remove_dir_all(root).unwrap();
                }))
            })
            .expect("native persistent test thread should join")
            .join()
            .expect("native persistent test thread should not panic");
    }

    #[cfg(all(unix, feature = "native-engine"))]
    fn read_record_from_path(path: &Path) -> BrowserResult<PersistentSessionRecord> {
        Ok(serde_json::from_slice(&std::fs::read(path)?)?)
    }
}
