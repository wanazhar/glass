//! Persistent local browser sessions for CLI and terminal clients.
//!
//! A session owner process keeps the [`CdpBrowserSession`] and Chrome child alive
//! between CLI invocations. Clients attach through the verified loopback CDP
//! port; the owner is the only process allowed to close the owned browser.

#[cfg(feature = "native-engine")]
use super::native_engine::{MAX_NATIVE_DIALOG_TEXT_BYTES, NativePendingDialog};
use super::policy::BrowserPolicy;
use super::runtime::BrowserRuntime;
use super::session::{
    BrowserResult, CdpBrowserSession, SessionOptions, WorkflowCheckpoint, WorkflowDefinition,
    WorkflowRunResult,
};
#[cfg(feature = "native-engine")]
use super::{BrowserRuntimeSession, NativeEngineConfig, NativeHistoryDirection};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::BTreeMap;
#[cfg(all(unix, feature = "native-engine"))]
use std::future::Future;
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

#[cfg(feature = "native-engine")]
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct NativeMcpDialogStatus {
    #[serde(default)]
    pub(crate) pending_dialog: Option<NativePendingDialog>,
    #[serde(default)]
    pub(crate) active_navigation_revision: Option<u64>,
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
    #[serde(default)]
    workflow: Option<NativeWorkflowRequest>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct NativeControlRequest {
    action: String,
    #[serde(default)]
    expected_revision: Option<u64>,
    #[serde(default)]
    dialog_id: Option<String>,
    #[serde(default)]
    prompt_value: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct NativeWorkflowRequest {
    action: String,
    workflow: WorkflowDefinition,
    #[serde(default)]
    inputs: BTreeMap<String, serde_json::Value>,
    #[serde(default)]
    checkpoint: Option<WorkflowCheckpoint>,
    #[serde(default)]
    result: Option<WorkflowRunResult>,
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

    let session = match CdpBrowserSession::start(&options).await {
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

    let session =
        match BrowserRuntimeSession::connect_native_with_modal_dialogs(native_config).await {
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
                        let operation = async {
                            let output = crate::cli::runner::run_native_persistent_request(
                                session,
                                request.argv,
                                &record.profile,
                                &policy,
                            )
                            .await?;
                            Ok(json!({"ok": true, "output": output}))
                        };
                        match run_native_operation_with_controls(
                            &listener,
                            session,
                            &record,
                            operation,
                        )
                        .await
                        {
                            Ok(response) => response,
                            Err(error) => json!({"ok": false, "error": error.to_string()}),
                        }
                    }
                    "workflow" => {
                        match request.workflow {
                            Some(request) => {
                                let session = session
                                    .as_ref()
                                    .ok_or("native persistent session is stopping")?;
                                let operation = async {
                                    let result =
                                        run_native_workflow_request(session, &policy, request)
                                            .await?;
                                    Ok(json!({"ok": true, "result": result}))
                                };
                                match run_native_operation_with_controls(
                                    &listener,
                                    session,
                                    &record,
                                    operation,
                                )
                                .await
                                {
                                    Ok(response) => response,
                                    Err(error) => {
                                        json!({"ok": false, "error": error.to_string()})
                                    }
                                }
                            }
                            None => json!({
                                "ok": false,
                                "error": "native workflow request is missing its payload"
                            }),
                        }
                    }
                    "control" => {
                        let request = request
                            .control
                            .ok_or("native control request is missing its payload")?;
                        let session = session
                            .as_ref()
                            .ok_or("native persistent session is stopping")?;
                        if request.action == "stopLoading" {
                            match request.expected_revision {
                                Some(expected_revision) => {
                                    match session
                                        .stop_loading_with_revision(expected_revision)
                                        .await
                                    {
                                        Ok(outcome) => json!({
                                            "ok": true,
                                            "action": outcome.action,
                                            "currentRevision": outcome.current_revision,
                                        }),
                                        Err(error) => {
                                            json!({"ok": false, "error": error.to_string()})
                                        }
                                    }
                                }
                                None => json!({
                                    "ok": false,
                                    "error": "stopLoading requires expectedRevision"
                                }),
                            }
                        } else {
                            let operation = async {
                                execute_native_control(session, request).await
                            };
                            match run_native_operation_with_controls(
                                &listener,
                                session,
                                &record,
                                operation,
                            )
                            .await
                            {
                                Ok(response) => response,
                                Err(error) => {
                                    json!({"ok": false, "error": error.to_string()})
                                }
                            }
                        }
                    }
                    "mcp" | "mcp-hosted-dialogs" => {
                        let pause_navigation_timeout_for_dialog =
                            request.op == "mcp-hosted-dialogs";
                        let params = request
                            .mcp_params
                            .ok_or("native MCP request is missing its params")?;
                        let session = session
                            .as_ref()
                            .ok_or("native persistent session is stopping")?;
                        let operation = async {
                            let result = crate::mcp::server::run_native_persistent_tool(
                                params,
                                session,
                                &record.profile,
                                &policy,
                                pause_navigation_timeout_for_dialog,
                            )
                            .await?;
                            Ok(json!({"ok": true, "result": result}))
                        };
                        match run_native_operation_with_controls(
                            &listener,
                            session,
                            &record,
                            operation,
                        )
                        .await
                        {
                            Ok(response) => response,
                            Err(error) => json!({"ok": false, "error": error.to_string()}),
                        }
                    }
                    _ => json!({"ok": false, "error": "unknown session operation"}),
                };
                write.write_all(serde_json::to_string(&response)?.as_bytes()).await?;
                write.write_all(b"\n").await?;
                if response.get("state").and_then(serde_json::Value::as_str)
                    == Some("stopping")
                {
                    shutdown = true;
                }
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
async fn execute_native_control(
    session: &BrowserRuntimeSession,
    request: NativeControlRequest,
) -> BrowserResult<serde_json::Value> {
    if request.action == "stopLoading" {
        let expected_revision = request
            .expected_revision
            .ok_or("stopLoading requires expectedRevision")?;
        let outcome = session
            .stop_loading_with_revision(expected_revision)
            .await?;
        return Ok(json!({
            "ok": true,
            "action": outcome.action,
            "currentRevision": outcome.current_revision,
        }));
    }

    let actual_revision = session
        .evidence(crate::browser_backend::EvidenceLevel::Compact)
        .await?
        .revision;
    let expected_revision = request
        .expected_revision
        .ok_or("native session control requires expectedRevision")?;
    if actual_revision != expected_revision {
        return Err(format!(
            "stale browser revision: expected {}, observed {actual_revision}",
            expected_revision
        )
        .into());
    }
    let outcome = match request.action.as_str() {
        "back" => {
            session
                .native_navigate_history(NativeHistoryDirection::Back)
                .await?
        }
        "forward" => {
            session
                .native_navigate_history(NativeHistoryDirection::Forward)
                .await?
        }
        "reload" => {
            session.script("location.reload()").await?;
            let revision = session
                .evidence(crate::browser_backend::EvidenceLevel::Compact)
                .await?
                .revision;
            crate::browser::session::NavigationControlOutcome {
                action: "reload".into(),
                previous_revision: expected_revision,
                current_revision: revision,
            }
        }
        _ => {
            return Err(format!("unsupported native session control `{}`", request.action).into());
        }
    };
    Ok(json!({
        "ok": true,
        "action": outcome.action,
        "currentRevision": outcome.current_revision,
    }))
}

#[cfg(all(unix, feature = "native-engine"))]
async fn accept_native_owner_request(
    listener: &tokio::net::UnixListener,
) -> BrowserResult<(
    tokio::net::unix::OwnedWriteHalf,
    Result<SessionRequest, String>,
)> {
    let (stream, _) = listener.accept().await?;
    let (read, write) = stream.into_split();
    let mut reader = BufReader::new(read);
    let line = match tokio::time::timeout(
        Duration::from_secs(2),
        read_bounded_native_request_line(&mut reader),
    )
    .await
    {
        Err(_) => Err("persistent session request timed out".to_owned()),
        Ok(Err(error)) => Err(error),
        Ok(Ok(line)) => serde_json::from_str::<SessionRequest>(line.trim())
            .map_err(|error| format!("invalid session request: {error}")),
    };
    Ok((write, line))
}

#[cfg(all(unix, feature = "native-engine"))]
async fn read_bounded_native_request_line(
    reader: &mut BufReader<tokio::net::unix::OwnedReadHalf>,
) -> Result<String, String> {
    use tokio::io::AsyncReadExt;

    let mut bytes = Vec::with_capacity(1024);
    let mut buffer = [0; 4096];
    loop {
        let remaining = MAX_SESSION_REQUEST_BYTES + 1 - bytes.len();
        let read_limit = remaining.min(buffer.len());
        let read = reader
            .read(&mut buffer[..read_limit])
            .await
            .map_err(|error| error.to_string())?;
        if read == 0 {
            if bytes.is_empty() {
                return Err("persistent session request closed before sending a line".into());
            }
            break;
        }
        let newline = buffer[..read].iter().position(|byte| *byte == b'\n');
        let count = newline.unwrap_or(read);
        bytes.extend_from_slice(&buffer[..count]);
        if bytes.len() > MAX_SESSION_REQUEST_BYTES {
            return Err(format!(
                "persistent session request exceeds its {MAX_SESSION_REQUEST_BYTES}-byte size bound"
            ));
        }
        if newline.is_some() {
            break;
        }
    }
    String::from_utf8(bytes).map_err(|error| format!("persistent request is not UTF-8: {error}"))
}

#[cfg(all(unix, feature = "native-engine"))]
async fn write_native_owner_response(
    writer: &mut tokio::net::unix::OwnedWriteHalf,
    response: &serde_json::Value,
) -> BrowserResult<()> {
    writer.write_all(&serde_json::to_vec(response)?).await?;
    writer.write_all(b"\n").await?;
    Ok(())
}

#[cfg(all(unix, feature = "native-engine"))]
fn native_owner_status(
    record: &PersistentSessionRecord,
    pending_dialog: Option<NativePendingDialog>,
    active_navigation_revision: Option<u64>,
) -> BrowserResult<serde_json::Value> {
    let mut status = serde_json::to_value(record)?;
    let fields = status
        .as_object_mut()
        .ok_or("native persistent status record is not an object")?;
    fields.insert(
        "pendingDialog".into(),
        serde_json::to_value(pending_dialog)?,
    );
    fields.insert(
        "activeNavigationRevision".into(),
        serde_json::to_value(active_navigation_revision)?,
    );
    Ok(status)
}

#[cfg(all(unix, feature = "native-engine"))]
async fn run_native_operation_with_controls<F>(
    listener: &tokio::net::UnixListener,
    session: &BrowserRuntimeSession,
    record: &PersistentSessionRecord,
    operation: F,
) -> BrowserResult<serde_json::Value>
where
    F: Future<Output = BrowserResult<serde_json::Value>>,
{
    let (operation_result, close_target_after_operation) = {
        tokio::pin!(operation);
        let mut close_target_after_operation = None;
        loop {
            tokio::select! {
                result = &mut operation => {
                    break (result, close_target_after_operation.take());
                }
                accepted = accept_native_owner_request(listener) => {
                let (mut writer, request) = accepted?;
                let response = match request {
                    Err(error) => json!({"ok": false, "error": error}),
                    Ok(request) if request.op == "stop" => {
                        let response = json!({"ok": true, "state": "stopping"});
                        write_native_owner_response(&mut writer, &response).await?;
                        return Ok(response);
                    }
                    Ok(request) if request.op == "status" => {
                        match (
                            session.native_pending_dialog_control(),
                            session.active_native_navigation_revision(),
                        ) {
                            (Ok(pending_dialog), Ok(active_revision)) => {
                                match native_owner_status(record, pending_dialog, active_revision) {
                                    Ok(status) => status,
                                    Err(error) => json!({"ok": false, "error": error.to_string()}),
                                }
                            }
                            (Err(error), _) | (_, Err(error)) => {
                                json!({"ok": false, "error": error.to_string()})
                            }
                        }
                    }
                    Ok(request) if request.op == "control" => {
                        match request.control {
                            Some(control) if control.action == "stopLoading" => {
                                let Some(expected_revision) = control.expected_revision else {
                                    let _ = write_native_owner_response(
                                        &mut writer,
                                        &json!({"ok": false, "error": "stopLoading requires expectedRevision"}),
                                    ).await;
                                    continue;
                                };
                                match session.active_native_navigation_revision() {
                                    Ok(Some(_)) => match session
                                        .stop_loading_with_revision(expected_revision)
                                        .await
                                    {
                                        Ok(outcome) => json!({
                                            "ok": true,
                                            "action": outcome.action,
                                            "currentRevision": outcome.current_revision,
                                        }),
                                        Err(error) => json!({
                                            "ok": false,
                                            "error": error.to_string(),
                                        }),
                                    },
                                    Ok(None) => json!({
                                        "ok": false,
                                        "error": "native persistent owner is busy; no interruptible HTTP(S) navigation is active",
                                    }),
                                    Err(error) => json!({
                                        "ok": false,
                                        "error": error.to_string(),
                                    }),
                                }
                            }
                            Some(control)
                                if matches!(control.action.as_str(), "acceptDialog" | "dismissDialog") =>
                            {
                                if close_target_after_operation.is_some() {
                                    json!({
                                        "ok": false,
                                        "error": "native dialog target is already closing",
                                    })
                                } else if let Some(dialog_id) = control.dialog_id.as_deref() {
                                    let accepted = control.action == "acceptDialog";
                                    match session.native_resolve_dialog_control(
                                        dialog_id,
                                        accepted,
                                        control.prompt_value,
                                    ) {
                                        Ok(child_dialog_id) => json!({
                                            "ok": true,
                                            "action": control.action,
                                            "dialogId": dialog_id,
                                            "childDialogId": child_dialog_id,
                                        }),
                                        Err(error) => json!({"ok": false, "error": error.to_string()}),
                                    }
                                } else {
                                    json!({"ok": false, "error": "dialog control requires dialogId"})
                                }
                            }
                            Some(control) if control.action == "closeDialogTarget" => {
                                if close_target_after_operation.is_some() {
                                    json!({
                                        "ok": false,
                                        "error": "a native dialog target is already closing",
                                    })
                                } else if control.prompt_value.is_some() {
                                    json!({
                                        "ok": false,
                                        "error": "closeDialogTarget does not accept promptValue",
                                    })
                                } else {
                                    let Some(dialog_id) = control.dialog_id.as_deref() else {
                                        let _ = write_native_owner_response(
                                            &mut writer,
                                            &json!({
                                                "ok": false,
                                                "error": "closeDialogTarget requires dialogId",
                                            }),
                                        ).await;
                                        continue;
                                    };
                                    let Some(expected_revision) = control.expected_revision else {
                                        let _ = write_native_owner_response(
                                            &mut writer,
                                            &json!({
                                                "ok": false,
                                                "error": "closeDialogTarget requires expectedRevision",
                                            }),
                                        ).await;
                                        continue;
                                    };
                                    match session.native_pending_dialog_control() {
                                        Ok(Some(pending)) if pending.id == dialog_id => {
                                            match session.active_native_navigation_revision() {
                                                Ok(Some(active_revision)) if active_revision == expected_revision => {
                                                    match session.stop_loading_with_revision(expected_revision).await {
                                                        Ok(_) => {
                                                            let target_id = pending.context_id;
                                                            close_target_after_operation = Some(target_id.clone());
                                                            json!({
                                                                "ok": true,
                                                                "action": control.action,
                                                                "dialogId": dialog_id,
                                                                "targetId": target_id,
                                                                "state": "closing",
                                                            })
                                                        }
                                                        Err(error) => json!({"ok": false, "error": error.to_string()}),
                                                    }
                                                }
                                                Ok(Some(active_revision)) => json!({
                                                    "ok": false,
                                                    "error": format!(
                                                        "stale browser revision: expected {expected_revision}, active navigation began at {active_revision}"
                                                    ),
                                                }),
                                                Ok(None) => json!({
                                                    "ok": false,
                                                    "error": "closeDialogTarget requires an active native HTTP(S) navigation",
                                                }),
                                                Err(error) => json!({"ok": false, "error": error.to_string()}),
                                            }
                                        }
                                        Ok(Some(_)) => json!({
                                            "ok": false,
                                            "error": "closeDialogTarget dialogId does not match the pending dialog",
                                        }),
                                        Ok(None) => json!({
                                            "ok": false,
                                            "error": "closeDialogTarget requires a pending native dialog",
                                        }),
                                        Err(error) => json!({"ok": false, "error": error.to_string()}),
                                    }
                                }
                            }
                            _ => json!({
                                "ok": false,
                                "error": "native persistent owner is busy; only status, stopLoading, exact pending-dialog controls, and closeDialogTarget are accepted during an active operation",
                            }),
                        }
                    }
                    Ok(_) => json!({
                        "ok": false,
                        "error": "native persistent owner is busy; only status and stopLoading are accepted during an active operation",
                    }),
                };
                // A disconnected control client must not drop the active
                // browser command future and thereby change navigation state.
                let _ = write_native_owner_response(&mut writer, &response).await;
            }
                _ = tokio::signal::ctrl_c() => {
                    return Ok(json!({
                        "ok": false,
                        "state": "stopping",
                        "error": "persistent owner was interrupted while a browser operation was active",
                    }));
                }
            }
        }
    };
    if let Some(target_id) = close_target_after_operation {
        session.close_target(&target_id).await?;
    }
    operation_result
}

#[cfg(all(unix, feature = "native-engine"))]
async fn run_native_workflow_request(
    session: &BrowserRuntimeSession,
    policy: &BrowserPolicy,
    request: NativeWorkflowRequest,
) -> BrowserResult<serde_json::Value> {
    match request.action.as_str() {
        "run" => Ok(serde_json::to_value(
            session
                .native_run_workflow(policy, &request.workflow, &request.inputs)
                .await?,
        )?),
        "checkpoint" => {
            let result = request
                .result
                .ok_or("native workflow checkpoint request is missing its result")?;
            Ok(serde_json::to_value(
                session
                    .native_export_workflow_checkpoint(&request.workflow, &result)
                    .await?,
            )?)
        }
        "resume" => {
            let checkpoint = request
                .checkpoint
                .ok_or("native workflow resume request is missing its checkpoint")?;
            Ok(serde_json::to_value(
                session
                    .native_resume_workflow(policy, &request.workflow, &request.inputs, &checkpoint)
                    .await?,
            )?)
        }
        action => Err(format!("unsupported native workflow action `{action}`").into()),
    }
}

async fn send_request(socket: &Path, op: &str) -> BrowserResult<serde_json::Value> {
    send_request_payload(
        socket,
        &SessionRequest {
            op: op.to_owned(),
            argv: Vec::new(),
            control: None,
            mcp_params: None,
            workflow: None,
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
    let response = send_request_payload(
        &record.socket,
        &SessionRequest {
            op: "execute".into(),
            argv,
            control: None,
            mcp_params: None,
            workflow: None,
        },
    )
    .await?;
    if response.get("ok").and_then(serde_json::Value::as_bool) == Some(false) {
        return Err(response
            .get("error")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("native persistent request was rejected")
            .to_owned()
            .into());
    }
    Ok(response)
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
    let response = send_request_payload(
        &record.socket,
        &SessionRequest {
            op: "control".into(),
            argv: Vec::new(),
            control: Some(NativeControlRequest {
                action: action.into(),
                expected_revision: Some(expected_revision),
                dialog_id: None,
                prompt_value: None,
            }),
            mcp_params: None,
            workflow: None,
        },
    )
    .await?;
    if response.get("ok").and_then(serde_json::Value::as_bool) == Some(false) {
        return Err(response
            .get("error")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("native persistent control was rejected")
            .to_owned()
            .into());
    }
    Ok(response)
}

#[cfg(feature = "native-engine")]
pub async fn control_native_dialog(
    name: &str,
    dialog_id: &str,
    accepted: bool,
    prompt_value: Option<String>,
) -> BrowserResult<serde_json::Value> {
    if dialog_id.is_empty()
        || dialog_id.len() > 64
        || !dialog_id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
    {
        return Err("native dialog control requires a valid dialogId".into());
    }
    if prompt_value
        .as_ref()
        .is_some_and(|value| value.len() > MAX_NATIVE_DIALOG_TEXT_BYTES)
    {
        return Err(
            format!("native prompt response exceeds {MAX_NATIVE_DIALOG_TEXT_BYTES} bytes").into(),
        );
    }
    if !accepted && prompt_value.is_some() {
        return Err("a dismissed native dialog cannot carry promptValue".into());
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
    let response = send_request_payload(
        &record.socket,
        &SessionRequest {
            op: "control".into(),
            argv: Vec::new(),
            control: Some(NativeControlRequest {
                action: if accepted {
                    "acceptDialog".into()
                } else {
                    "dismissDialog".into()
                },
                expected_revision: None,
                dialog_id: Some(dialog_id.into()),
                prompt_value,
            }),
            mcp_params: None,
            workflow: None,
        },
    )
    .await?;
    if response.get("ok").and_then(serde_json::Value::as_bool) == Some(false) {
        return Err(response
            .get("error")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("native dialog control was rejected")
            .to_owned()
            .into());
    }
    Ok(response)
}

#[cfg(feature = "native-engine")]
pub async fn execute_native_mcp(
    name: &str,
    params: serde_json::Value,
) -> BrowserResult<serde_json::Value> {
    execute_native_mcp_request(name, params, "mcp").await
}

#[cfg(feature = "native-engine")]
pub(crate) async fn execute_native_mcp_with_dialog_host(
    name: &str,
    params: serde_json::Value,
) -> BrowserResult<serde_json::Value> {
    execute_native_mcp_request(name, params, "mcp-hosted-dialogs").await
}

#[cfg(feature = "native-engine")]
async fn execute_native_mcp_request(
    name: &str,
    params: serde_json::Value,
    operation: &str,
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
            op: operation.into(),
            argv: Vec::new(),
            control: None,
            mcp_params: Some(params),
            workflow: None,
        },
    )
    .await?;
    response
        .get("result")
        .cloned()
        .ok_or_else(|| "native persistent MCP request returned no result".into())
}

#[cfg(feature = "native-engine")]
pub(crate) async fn native_mcp_dialog_status(name: &str) -> BrowserResult<NativeMcpDialogStatus> {
    let Some(record) = read_record(name)? else {
        return Err(format!("persistent session `{name}` is not running; start it first").into());
    };
    if !record.runtime.is_native() {
        return Err(format!("persistent session `{name}` is not a native session").into());
    }
    if !process_is_alive(record.pid) {
        return Err(format!("persistent session `{name}` is stale; restart it first").into());
    }
    let response = send_request(&record.socket, "status").await?;
    Ok(serde_json::from_value(response)?)
}

#[cfg(feature = "native-engine")]
async fn send_native_workflow_request(
    name: &str,
    request: NativeWorkflowRequest,
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
            op: "workflow".into(),
            argv: Vec::new(),
            control: None,
            mcp_params: None,
            workflow: Some(request),
        },
    )
    .await?;
    response
        .get("result")
        .cloned()
        .ok_or_else(|| "native persistent workflow request returned no result".into())
}

#[cfg(feature = "native-engine")]
pub async fn run_native_workflow(
    name: &str,
    workflow: WorkflowDefinition,
    inputs: BTreeMap<String, serde_json::Value>,
) -> BrowserResult<WorkflowRunResult> {
    let result = send_native_workflow_request(
        name,
        NativeWorkflowRequest {
            action: "run".into(),
            workflow,
            inputs,
            checkpoint: None,
            result: None,
        },
    )
    .await?;
    Ok(serde_json::from_value(result)?)
}

#[cfg(feature = "native-engine")]
pub async fn export_native_workflow_checkpoint(
    name: &str,
    workflow: WorkflowDefinition,
    result: WorkflowRunResult,
) -> BrowserResult<WorkflowCheckpoint> {
    let checkpoint = send_native_workflow_request(
        name,
        NativeWorkflowRequest {
            action: "checkpoint".into(),
            workflow,
            inputs: BTreeMap::new(),
            checkpoint: None,
            result: Some(result),
        },
    )
    .await?;
    Ok(serde_json::from_value(checkpoint)?)
}

#[cfg(feature = "native-engine")]
pub async fn resume_native_workflow(
    name: &str,
    workflow: WorkflowDefinition,
    inputs: BTreeMap<String, serde_json::Value>,
    checkpoint: WorkflowCheckpoint,
) -> BrowserResult<WorkflowRunResult> {
    let result = send_native_workflow_request(
        name,
        NativeWorkflowRequest {
            action: "resume".into(),
            workflow,
            inputs,
            checkpoint: Some(checkpoint),
            result: None,
        },
    )
    .await?;
    Ok(serde_json::from_value(result)?)
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

    #[cfg(feature = "native-engine")]
    use crate::browser::session::{WorkflowRunStatus, WorkflowStepState};

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
    async fn wait_for_native_pending_dialog(
        socket: &Path,
        expected_type: &str,
    ) -> serde_json::Value {
        tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                let status = send_request(socket, "status")
                    .await
                    .expect("persistent owner status should remain available");
                let pending = &status["pendingDialog"];
                if pending["dialog"]["type"].as_str() == Some(expected_type) {
                    return pending.clone();
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap_or_else(|_| panic!("native owner did not expose a pending {expected_type} dialog"))
    }

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
                    let fixture_listener =
                        tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
                    let fixture_address = fixture_listener.local_addr().unwrap();
                    let (slow_request_tx, slow_request_rx) = tokio::sync::oneshot::channel();
                    let (release_slow_tx, release_slow_rx) = tokio::sync::oneshot::channel();
                    let (fast_request_tx, fast_request_rx) = tokio::sync::oneshot::channel();
                    let fixture_server = tokio::task::spawn_local(async move {
                        use tokio::io::AsyncWriteExt;

                        let (slow_stream, _) = fixture_listener.accept().await.unwrap();
                        let mut slow_reader = BufReader::new(slow_stream);
                        loop {
                            let mut line = String::new();
                            if slow_reader.read_line(&mut line).await.unwrap() == 0
                                || line == "\r\n"
                            {
                                break;
                            }
                        }
                        let _ = slow_request_tx.send(());
                        let mut slow_stream = slow_reader.into_inner();
                        let _ = release_slow_rx.await;
                        let _ = slow_stream
                            .write_all(
                                b"HTTP/1.1 200 OK\r\nContent-Length: 5\r\nConnection: close\r\n\r\nslow!",
                            )
                            .await;

                        let (fast_stream, _) = fixture_listener.accept().await.unwrap();
                        let mut fast_reader = BufReader::new(fast_stream);
                        loop {
                            let mut line = String::new();
                            if fast_reader.read_line(&mut line).await.unwrap() == 0
                                || line == "\r\n"
                            {
                                break;
                            }
                        }
                        let _ = fast_request_tx.send(());
                        let mut fast_stream = fast_reader.into_inner();
                        let body = b"<!doctype html><title>owner-follow-up</title><p>ok</p>";
                        let headers = format!(
                            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                            body.len()
                        );
                        fast_stream.write_all(headers.as_bytes()).await.unwrap();
                        fast_stream.write_all(body).await.unwrap();

                        let modal_bodies: &[&[u8]] = &[
                            br#"<!doctype html><body><script>
                                    window.dialogTrace = ["before"];
                                    window.dialogTrace.push("confirm:" + confirm("continue?"));
                                    window.dialogTrace.push("prompt-default:" + prompt("default?", "seed"));
                                    window.dialogTrace.push("prompt:" + prompt("name?", "ignored"));
                                    alert("finish");
                                    window.dialogTrace.push("after");
                                    document.body.textContent = window.dialogTrace.join("|");
                                </script></body>"#,
                            br#"<!doctype html><body><script>
                                window.dialogDismissTrace = ["before-dismissals"];
                                window.dialogDismissTrace.push("confirm:" + confirm("dismiss confirm?"));
                                window.dialogDismissTrace.push("prompt:" + prompt("dismiss prompt?", "fallback"));
                                window.dialogDismissTrace.push("alert:" + alert("accept alert"));
                                window.dialogDismissTrace.push("after-dismissals");
                                document.body.textContent = window.dialogDismissTrace.join("|");
                            </script></body>"#,
                            br#"<!doctype html><body><script>
                                window.dialogTrace = ["before-cancel"];
                                alert("cancel navigation");
                                window.dialogTrace.push("after-cancel");
                            </script></body>"#,
                            b"<!doctype html><title>worker-restarted</title><p>healthy</p>",
                            br#"<!doctype html><body><script>
                                alert("close target");
                            </script></body>"#,
                            br#"<!doctype html><body><script>
                                window.stopTrace = ["before-stop"];
                                alert("stop owner");
                                window.stopTrace.push("after-stop");
                            </script></body>"#,
                        ];
                        for body in modal_bodies {
                            let (stream, _) = fixture_listener.accept().await.unwrap();
                            let mut reader = BufReader::new(stream);
                            loop {
                                let mut line = String::new();
                                if reader.read_line(&mut line).await.unwrap() == 0
                                    || line == "\r\n"
                                {
                                    break;
                                }
                            }
                            let mut stream = reader.into_inner();
                            let headers = format!(
                                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                                body.len()
                            );
                            stream.write_all(headers.as_bytes()).await.unwrap();
                            stream.write_all(body).await.unwrap();
                        }
                    });
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
                        workflow: None,
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
                            expected_revision: Some(revision),
                            dialog_id: None,
                            prompt_value: None,
                        }),
                        mcp_params: None,
                        workflow: None,
                    };
                    let control_result = send_request_payload(&socket, &control).await.unwrap();
                    assert_eq!(control_result["ok"], true);
                    assert_eq!(control_result["action"], "stopLoading");
                    assert_eq!(control_result["currentRevision"], revision);

                    let navigate_request = SessionRequest {
                        op: "execute".into(),
                        argv: vec![
                            "--browser-runtime".into(),
                            "native".into(),
                            "--profile".into(),
                            "native-test".into(),
                            "navigate".into(),
                            format!("http://{fixture_address}/slow"),
                        ],
                        control: None,
                        mcp_params: None,
                        workflow: None,
                    };
                    let navigate_socket = socket.clone();
                    let pending_navigation = tokio::task::spawn_local(async move {
                        send_request_payload(&navigate_socket, &navigate_request).await
                    });
                    tokio::time::timeout(Duration::from_secs(5), slow_request_rx)
                        .await
                        .expect("slow HTTP fixture was never reached")
                        .expect("slow HTTP fixture notification was dropped");

                    let status = tokio::time::timeout(
                        Duration::from_secs(2),
                        send_request(&socket, "status"),
                    )
                    .await
                    .expect("status was blocked behind active navigation")
                    .unwrap();
                    assert_eq!(status["state"], "running");

                    let busy = send_request_payload(&socket, &request)
                        .await
                        .expect_err("a second browser command ran concurrently");
                    assert!(busy.to_string().contains("busy"));

                    let stale_stop = send_request_payload(
                        &socket,
                        &SessionRequest {
                            op: "control".into(),
                            argv: Vec::new(),
                            control: Some(NativeControlRequest {
                                action: "stopLoading".into(),
                                expected_revision: Some(revision.saturating_add(1)),
                                dialog_id: None,
                                prompt_value: None,
                            }),
                            mcp_params: None,
                            workflow: None,
                        },
                    )
                    .await
                    .expect_err("stop-loading accepted a stale revision");
                    assert!(
                        stale_stop.to_string().contains("stale page revision"),
                        "unexpected stale stop error: {stale_stop}"
                    );

                    let stopped = tokio::time::timeout(
                        Duration::from_secs(2),
                        send_request_payload(
                            &socket,
                            &SessionRequest {
                                op: "control".into(),
                                argv: Vec::new(),
                                control: Some(NativeControlRequest {
                                    action: "stopLoading".into(),
                                    expected_revision: Some(revision),
                                    dialog_id: None,
                                    prompt_value: None,
                                }),
                                mcp_params: None,
                                workflow: None,
                            },
                        ),
                    )
                    .await
                    .expect("stop-loading did not respond promptly")
                    .unwrap();
                    assert_eq!(stopped["action"], "stopLoading");
                    assert_eq!(stopped["currentRevision"], revision);
                    let _ = release_slow_tx.send(());

                    let cancelled = tokio::time::timeout(
                        Duration::from_secs(5),
                        pending_navigation,
                    )
                    .await
                    .expect("cancelled owner command did not settle")
                    .expect("navigation client task panicked")
                    .expect_err("cancelled navigation reported success");
                    assert!(cancelled.to_string().to_lowercase().contains("cancel"));

                    let after_cancel = send_request_payload(&socket, &request).await.unwrap();
                    assert_eq!(after_cancel["output"]["value"]["revision"], revision);

                    let follow_up = SessionRequest {
                        op: "execute".into(),
                        argv: vec![
                            "--browser-runtime".into(),
                            "native".into(),
                            "--profile".into(),
                            "native-test".into(),
                            "navigate".into(),
                            format!("http://{fixture_address}/fast"),
                        ],
                        control: None,
                        mcp_params: None,
                        workflow: None,
                    };
                    let follow_up_socket = socket.clone();
                    let follow_up_client = tokio::task::spawn_local(async move {
                        send_request_payload(&follow_up_socket, &follow_up).await
                    });
                    tokio::time::timeout(Duration::from_secs(5), fast_request_rx)
                        .await
                        .expect("follow-up navigation never reached the HTTP fixture")
                        .expect("HTTP fixture follow-up notification was dropped");
                    let follow_up = tokio::time::timeout(Duration::from_secs(10), follow_up_client)
                    .await
                    .expect("follow-up navigation was blocked by cancelled operation")
                    .expect("follow-up navigation client task panicked")
                    .unwrap();
                    assert_eq!(follow_up["ok"], true);
                    let after_follow_up = send_request_payload(&socket, &request).await.unwrap();
                    assert!(
                        after_follow_up["output"]["value"]["revision"]
                            .as_u64()
                            .expect("follow-up observation should expose a revision")
                            > revision
                    );
                    let mcp_request = SessionRequest {
                        op: "mcp".into(),
                        argv: Vec::new(),
                        control: None,
                        mcp_params: Some(json!({
                            "name": "observe",
                            "arguments": {}
                        })),
                        workflow: None,
                    };
                    let mcp_result = send_request_payload(&socket, &mcp_request).await.unwrap();
                    assert_eq!(mcp_result["ok"], true);
                    assert!(mcp_result["result"]["content"].is_array());

                    let workflow = WorkflowDefinition::from_value(json!({
                        "schemaVersion": 1,
                        "name": "owner-workflow",
                        "workflowVersion": "1.0.0",
                        "inputs": {},
                        "budgets": {
                            "maxSteps": 2,
                            "maxDurationMs": 30_000,
                            "maxRetries": 0,
                            "maxExtractedBytes": 4_096
                        },
                        "steps": [{
                            "id": "observe",
                            "action": "observe"
                        }],
                        "terminalCondition": {
                            "urlEquals": format!("http://{fixture_address}/fast")
                        },
                        "outputs": {}
                    }))
                    .unwrap();
                    let workflow_request = SessionRequest {
                        op: "workflow".into(),
                        argv: Vec::new(),
                        control: None,
                        mcp_params: None,
                        workflow: Some(NativeWorkflowRequest {
                            action: "run".into(),
                            workflow: workflow.clone(),
                            inputs: BTreeMap::new(),
                            checkpoint: None,
                            result: None,
                        }),
                    };
                    let workflow_response = send_request_payload(&socket, &workflow_request)
                        .await
                        .unwrap();
                    assert_eq!(workflow_response["ok"], true);
                    let workflow_result: WorkflowRunResult =
                        serde_json::from_value(workflow_response["result"].clone()).unwrap();
                    assert_eq!(workflow_result.status, WorkflowRunStatus::Completed);
                    assert_eq!(workflow_result.steps.len(), 1);
                    assert_eq!(workflow_result.steps[0].state, WorkflowStepState::Committed);

                    let checkpoint_request = SessionRequest {
                        op: "workflow".into(),
                        argv: Vec::new(),
                        control: None,
                        mcp_params: None,
                        workflow: Some(NativeWorkflowRequest {
                            action: "checkpoint".into(),
                            workflow,
                            inputs: BTreeMap::new(),
                            checkpoint: None,
                            result: Some(workflow_result),
                        }),
                    };
                    let checkpoint_response = send_request_payload(&socket, &checkpoint_request)
                        .await
                        .unwrap();
                    assert_eq!(checkpoint_response["ok"], true);
                    assert_eq!(checkpoint_response["result"]["status"], "completed");
                    assert_eq!(checkpoint_response["result"]["nextStepIndex"], 1);
                    let owner_pid = record.pid;

                    let second = send_request_payload(&socket, &request).await.unwrap();
                    assert_eq!(second["ok"], true);
                    assert_eq!(read_record_from_path(&status_path).unwrap().pid, owner_pid);

                    let native_execute = |args: Vec<String>| {
                        let mut argv = vec![
                            "--browser-runtime".into(),
                            "native".into(),
                            "--profile".into(),
                            "native-test".into(),
                        ];
                        argv.extend(args);
                        SessionRequest {
                            op: "execute".into(),
                            argv,
                            control: None,
                            mcp_params: None,
                            workflow: None,
                        }
                    };
                    let dialog_control = |action: &str,
                                          dialog_id: &str,
                                          prompt_value: Option<String>| {
                        SessionRequest {
                            op: "control".into(),
                            argv: Vec::new(),
                            control: Some(NativeControlRequest {
                                action: action.into(),
                                expected_revision: None,
                                dialog_id: Some(dialog_id.into()),
                                prompt_value,
                            }),
                            mcp_params: None,
                            workflow: None,
                        }
                    };
                    let native_mcp = |name: &str, arguments: serde_json::Value| SessionRequest {
                        op: "mcp".into(),
                        argv: Vec::new(),
                        control: None,
                        mcp_params: Some(json!({"name": name, "arguments": arguments})),
                        workflow: None,
                    };
                    let close_dialog_target = |dialog_id: &str, expected_revision| {
                        SessionRequest {
                            op: "control".into(),
                            argv: Vec::new(),
                            control: Some(NativeControlRequest {
                                action: "closeDialogTarget".into(),
                                expected_revision: Some(expected_revision),
                                dialog_id: Some(dialog_id.into()),
                                prompt_value: None,
                            }),
                            mcp_params: None,
                            workflow: None,
                        }
                    };

                    let dialog_navigation = native_execute(vec![
                        "navigate".into(),
                        format!("http://{fixture_address}/dialog-sequence"),
                    ]);
                    let dialog_socket = socket.clone();
                    let pending_dialog_navigation = tokio::task::spawn_local(async move {
                        send_request_payload(&dialog_socket, &dialog_navigation).await
                    });
                    let confirm = wait_for_native_pending_dialog(&socket, "confirm").await;
                    assert_eq!(confirm["dialog"]["message"], "continue?");
                    assert!(confirm["contextId"].as_str().is_some_and(|id| !id.is_empty()));
                    assert!(confirm["frameId"].as_str().is_some_and(|id| !id.is_empty()));
                    assert!(!pending_dialog_navigation.is_finished());

                    let busy = send_request_payload(&socket, &request)
                        .await
                        .expect_err("a second browser command ran during a modal dialog");
                    assert!(busy.to_string().contains("busy"));
                    let stale_dialog = dialog_control(
                        "acceptDialog",
                        "native-dialog-stale",
                        None,
                    );
                    send_request_payload(&socket, &stale_dialog)
                        .await
                        .expect_err("a stale dialog identity was accepted");
                    assert_eq!(
                        wait_for_native_pending_dialog(&socket, "confirm").await["id"],
                        confirm["id"]
                    );

                    let accepted_confirm = send_request_payload(
                        &socket,
                        &dialog_control("acceptDialog", confirm["id"].as_str().unwrap(), None),
                    )
                    .await
                    .unwrap();
                    assert_eq!(accepted_confirm["ok"], true);
                    let default_prompt = wait_for_native_pending_dialog(&socket, "prompt").await;
                    assert_eq!(default_prompt["dialog"]["default_value"], "seed");
                    send_request_payload(
                        &socket,
                        &dialog_control(
                            "acceptDialog",
                            default_prompt["id"].as_str().unwrap(),
                            None,
                        ),
                    )
                    .await
                    .unwrap();
                    let explicit_prompt = wait_for_native_pending_dialog(&socket, "prompt").await;
                    assert_eq!(explicit_prompt["dialog"]["default_value"], "ignored");
                    send_request_payload(
                        &socket,
                        &dialog_control(
                            "acceptDialog",
                            explicit_prompt["id"].as_str().unwrap(),
                            Some("Ada".into()),
                        ),
                    )
                    .await
                    .unwrap();
                    let alert = wait_for_native_pending_dialog(&socket, "alert").await;
                    send_request_payload(
                        &socket,
                        &dialog_control("dismissDialog", alert["id"].as_str().unwrap(), None),
                    )
                    .await
                    .unwrap();
                    let dialog_result = tokio::time::timeout(
                        Duration::from_secs(15),
                        pending_dialog_navigation,
                    )
                    .await
                    .expect("dialog navigation did not resume after its decisions")
                    .expect("dialog navigation task panicked")
                    .unwrap();
                    assert_eq!(dialog_result["ok"], true);
                    let evaluate_trace = send_request_payload(
                        &socket,
                        &native_execute(vec![
                            "evaluate".into(),
                            "document.body.textContent".into(),
                        ]),
                    )
                    .await
                    .unwrap();
                    assert!(evaluate_trace.to_string().contains(
                        "before|confirm:true|prompt-default:seed|prompt:Ada|after"
                    ));

                    let evaluate_with_dialogs = native_execute(vec![
                        "evaluate".into(),
                        "window.evaluateDialogTrace = ['before']; window.evaluateDialogTrace.push('confirm:' + confirm('evaluate confirm?')); window.evaluateDialogTrace.push('prompt:' + prompt('evaluate prompt?', 'fallback')); alert('evaluate alert'); window.evaluateDialogTrace.push('after'); window.evaluateDialogTrace.join('|')".into(),
                    ]);
                    let evaluate_socket = socket.clone();
                    let mut pending_evaluate = tokio::task::spawn_local(async move {
                        send_request_payload(&evaluate_socket, &evaluate_with_dialogs).await
                    });
                    let evaluate_confirm = tokio::time::timeout(Duration::from_secs(10), async {
                        loop {
                            if pending_evaluate.is_finished() {
                                let result = (&mut pending_evaluate)
                                    .await
                                    .expect("explicit evaluate task should not panic");
                                panic!(
                                    "explicit evaluate completed before publishing its confirm dialog: {result:?}"
                                );
                            }
                            let status = send_request(&socket, "status").await.unwrap();
                            let pending = status["pendingDialog"].clone();
                            if pending["dialog"]["type"].as_str() == Some("confirm") {
                                break pending;
                            }
                            tokio::time::sleep(Duration::from_millis(10)).await;
                        }
                    })
                    .await
                    .expect("native owner did not expose the explicit-evaluate confirm dialog");
                    assert_eq!(evaluate_confirm["dialog"]["message"], "evaluate confirm?");
                    assert!(
                        !pending_evaluate.is_finished(),
                        "evaluate completed before its confirm call was resolved"
                    );
                    send_request_payload(
                        &socket,
                        &dialog_control(
                            "acceptDialog",
                            evaluate_confirm["id"].as_str().unwrap(),
                            None,
                        ),
                    )
                    .await
                    .unwrap();
                    let evaluate_prompt = wait_for_native_pending_dialog(&socket, "prompt").await;
                    assert_eq!(evaluate_prompt["dialog"]["default_value"], "fallback");
                    assert!(!pending_evaluate.is_finished());
                    send_request_payload(
                        &socket,
                        &dialog_control(
                            "acceptDialog",
                            evaluate_prompt["id"].as_str().unwrap(),
                            Some("Grace".into()),
                        ),
                    )
                    .await
                    .unwrap();
                    let evaluate_alert = wait_for_native_pending_dialog(&socket, "alert").await;
                    assert_eq!(evaluate_alert["dialog"]["message"], "evaluate alert");
                    assert!(!pending_evaluate.is_finished());
                    send_request_payload(
                        &socket,
                        &dialog_control(
                            "acceptDialog",
                            evaluate_alert["id"].as_str().unwrap(),
                            None,
                        ),
                    )
                    .await
                    .unwrap();
                    let evaluate_result = tokio::time::timeout(
                        Duration::from_secs(10),
                        pending_evaluate,
                    )
                    .await
                    .expect("explicit evaluate did not resume after dialog decisions")
                    .expect("explicit evaluate client task panicked")
                    .unwrap();
                    assert_eq!(evaluate_result["ok"], true);
                    assert!(evaluate_result.to_string().contains(
                        "before|confirm:true|prompt:Grace|after"
                    ));

                    let dismiss_navigation = native_execute(vec![
                        "navigate".into(),
                        format!("http://{fixture_address}/dialog-dismiss-sequence"),
                    ]);
                    let dismiss_socket = socket.clone();
                    let pending_dismiss_navigation = tokio::task::spawn_local(async move {
                        send_request_payload(&dismiss_socket, &dismiss_navigation).await
                    });
                    let dismissed_confirm =
                        wait_for_native_pending_dialog(&socket, "confirm").await;
                    assert_eq!(dismissed_confirm["dialog"]["message"], "dismiss confirm?");
                    assert!(!pending_dismiss_navigation.is_finished());
                    send_request_payload(
                        &socket,
                        &dialog_control(
                            "dismissDialog",
                            dismissed_confirm["id"].as_str().unwrap(),
                            None,
                        ),
                    )
                    .await
                    .unwrap();
                    let dismissed_prompt = wait_for_native_pending_dialog(&socket, "prompt").await;
                    assert_eq!(dismissed_prompt["dialog"]["default_value"], "fallback");
                    assert!(!pending_dismiss_navigation.is_finished());
                    send_request_payload(
                        &socket,
                        &dialog_control(
                            "dismissDialog",
                            dismissed_prompt["id"].as_str().unwrap(),
                            None,
                        ),
                    )
                    .await
                    .unwrap();
                    let accepted_alert = wait_for_native_pending_dialog(&socket, "alert").await;
                    assert_eq!(accepted_alert["dialog"]["message"], "accept alert");
                    assert!(!pending_dismiss_navigation.is_finished());
                    send_request_payload(
                        &socket,
                        &dialog_control(
                            "acceptDialog",
                            accepted_alert["id"].as_str().unwrap(),
                            None,
                        ),
                    )
                    .await
                    .unwrap();
                    let dismiss_result = tokio::time::timeout(
                        Duration::from_secs(15),
                        pending_dismiss_navigation,
                    )
                    .await
                    .expect("dismissed-dialog navigation did not resume")
                    .expect("dismissed-dialog navigation task panicked")
                    .unwrap();
                    assert_eq!(dismiss_result["ok"], true);
                    let dismiss_trace = send_request_payload(
                        &socket,
                        &native_execute(vec![
                            "evaluate".into(),
                            "document.body.textContent".into(),
                        ]),
                    )
                    .await
                    .unwrap();
                    assert!(dismiss_trace.to_string().contains(
                        "before-dismissals|confirm:false|prompt:null|alert:undefined|after-dismissals"
                    ));

                    let dialog_revision = send_request_payload(&socket, &request)
                        .await
                        .unwrap()["output"]["value"]["revision"]
                        .as_u64()
                        .expect("dialog page observation should expose its revision");
                    let cancel_navigation = native_execute(vec![
                        "navigate".into(),
                        format!("http://{fixture_address}/dialog-cancel"),
                    ]);
                    let cancel_socket = socket.clone();
                    let pending_cancel_navigation = tokio::task::spawn_local(async move {
                        send_request_payload(&cancel_socket, &cancel_navigation).await
                    });
                    let cancel_alert = wait_for_native_pending_dialog(&socket, "alert").await;
                    assert_eq!(cancel_alert["dialog"]["message"], "cancel navigation");
                    let stop_dialog_navigation = send_request_payload(
                        &socket,
                        &SessionRequest {
                            op: "control".into(),
                            argv: Vec::new(),
                            control: Some(NativeControlRequest {
                                action: "stopLoading".into(),
                                expected_revision: Some(dialog_revision),
                                dialog_id: None,
                                prompt_value: None,
                            }),
                            mcp_params: None,
                            workflow: None,
                        },
                    )
                    .await
                    .unwrap();
                    assert_eq!(stop_dialog_navigation["action"], "stopLoading");
                    let cancelled_dialog_navigation = tokio::time::timeout(
                        Duration::from_secs(5),
                        pending_cancel_navigation,
                    )
                    .await
                    .expect("cancelled modal navigation did not settle")
                    .expect("cancelled modal navigation task panicked")
                    .expect_err("cancelled modal navigation committed");
                    assert!(cancelled_dialog_navigation
                        .to_string()
                        .to_lowercase()
                        .contains("cancel"));
                    let after_dialog_cancel = send_request(&socket, "status").await.unwrap();
                    assert!(after_dialog_cancel["pendingDialog"].is_null());
                    let unchanged_revision = send_request_payload(&socket, &request)
                        .await
                        .unwrap()["output"]["value"]["revision"]
                        .as_u64()
                        .expect("cancelled dialog must preserve the committed document");
                    assert_eq!(unchanged_revision, dialog_revision);

                    let healthy_navigation = native_execute(vec![
                        "navigate".into(),
                        format!("http://{fixture_address}/healthy"),
                    ]);
                    let healthy = send_request_payload(&socket, &healthy_navigation)
                        .await
                        .unwrap();
                    assert_eq!(healthy["ok"], true);
                    let healthy_revision = send_request_payload(&socket, &request)
                        .await
                        .unwrap()["output"]["value"]["revision"]
                        .as_u64()
                        .expect("healthy follow-up page should expose a revision");

                    let parked_target_response = send_request_payload(
                        &socket,
                        &native_mcp("createTarget", json!({"url": "about:blank"})),
                    )
                    .await
                    .unwrap();
                    assert_eq!(parked_target_response["ok"], true);
                    let parked_target: serde_json::Value = serde_json::from_str(
                        parked_target_response["result"]["content"][0]["text"]
                            .as_str()
                            .expect("createTarget should return a serialized target"),
                    )
                    .unwrap();
                    let parked_target_id = parked_target["id"].as_str().unwrap().to_owned();

                    let close_navigation = native_execute(vec![
                        "navigate".into(),
                        format!("http://{fixture_address}/dialog-close"),
                    ]);
                    let close_socket = socket.clone();
                    let pending_close_navigation = tokio::task::spawn_local(async move {
                        send_request_payload(&close_socket, &close_navigation).await
                    });
                    let close_dialog = wait_for_native_pending_dialog(&socket, "alert").await;
                    let close_target_id = close_dialog["contextId"].as_str().unwrap();
                    let stale_target_close = send_request_payload(
                        &socket,
                        &close_dialog_target("native-dialog-stale", healthy_revision),
                    )
                    .await
                    .expect_err("target close accepted a stale dialog identity");
                    assert!(stale_target_close
                        .to_string()
                        .contains("does not match the pending dialog"));
                    assert_eq!(
                        wait_for_native_pending_dialog(&socket, "alert").await["id"],
                        close_dialog["id"]
                    );
                    let stale_target_close_revision = send_request_payload(
                        &socket,
                        &close_dialog_target(
                            close_dialog["id"].as_str().unwrap(),
                            healthy_revision.saturating_add(1),
                        ),
                    )
                    .await
                    .expect_err("target close accepted a stale page revision");
                    assert!(stale_target_close_revision
                        .to_string()
                        .contains("stale browser revision"));
                    let close_result = send_request_payload(
                        &socket,
                        &close_dialog_target(
                            close_dialog["id"].as_str().unwrap(),
                            healthy_revision,
                        ),
                    )
                    .await
                    .unwrap();
                    assert_eq!(close_result["ok"], true);
                    assert_eq!(close_result["targetId"], close_target_id);
                    let closed_navigation = tokio::time::timeout(
                        Duration::from_secs(5),
                        pending_close_navigation,
                    )
                    .await
                    .expect("target-close navigation did not settle")
                    .expect("target-close navigation task panicked")
                    .expect_err("navigation succeeded after its dialog target was closed");
                    assert!(closed_navigation
                        .to_string()
                        .to_lowercase()
                        .contains("cancel"));

                    let listed_targets = send_request_payload(
                        &socket,
                        &native_mcp("listTargets", json!({})),
                    )
                    .await
                    .unwrap();
                    let listed_targets: serde_json::Value = serde_json::from_str(
                        listed_targets["result"]["content"][0]["text"]
                            .as_str()
                            .expect("listTargets should return serialized targets"),
                    )
                    .unwrap();
                    let listed_targets = listed_targets["result"]
                        .as_array()
                        .expect("listTargets result should contain a target array");
                    assert!(!listed_targets
                        .iter()
                        .any(|target| target["id"] == close_target_id));
                    assert!(listed_targets
                        .iter()
                        .any(|target| target["id"] == parked_target_id));
                    send_request_payload(
                        &socket,
                        &native_mcp(
                            "selectTarget",
                            json!({"id": parked_target_id.clone()}),
                        ),
                    )
                    .await
                    .unwrap();

                    let stop_navigation = native_execute(vec![
                        "navigate".into(),
                        format!("http://{fixture_address}/dialog-stop"),
                    ]);
                    let stop_socket = socket.clone();
                    let mut pending_stop_navigation = tokio::task::spawn_local(async move {
                        send_request_payload(&stop_socket, &stop_navigation).await
                    });
                    let stop_dialog = tokio::time::timeout(Duration::from_secs(10), async {
                        loop {
                            let status = send_request(&socket, "status").await.unwrap();
                            if status["pendingDialog"].is_object() {
                                break status["pendingDialog"].clone();
                            }
                            if pending_stop_navigation.is_finished() {
                                let result = (&mut pending_stop_navigation).await;
                                panic!(
                                    "owner-stop navigation completed before opening a dialog: {result:?}"
                                );
                            }
                            tokio::time::sleep(Duration::from_millis(10)).await;
                        }
                    })
                    .await
                    .expect("owner-stop navigation did not publish a pending dialog");
                    assert_eq!(stop_dialog["dialog"]["type"], "alert");
                    assert_eq!(stop_dialog["dialog"]["message"], "stop owner");
                    let stopped = send_request(&socket, "stop").await.unwrap();
                    assert_eq!(stopped["state"], "stopping");
                    let stopped_navigation = tokio::time::timeout(
                        Duration::from_secs(5),
                        pending_stop_navigation,
                    )
                    .await
                    .expect("owner stop did not release the active modal command")
                    .expect("stopped modal command task panicked")
                    .unwrap();
                    assert_eq!(stopped_navigation["state"], "stopping");
                    tokio::time::timeout(Duration::from_secs(5), fixture_server)
                        .await
                        .expect("HTTP modal fixtures did not complete")
                        .expect("HTTP modal fixture task panicked");
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
