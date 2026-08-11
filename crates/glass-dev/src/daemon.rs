//! Local authenticated daemon that owns complete development workspaces.

use crate::{
    DevelopmentToolContext, DevelopmentWorkspace, ResidentAgentBroker, WorkspaceTrustStore,
};
use glass_browser::cli::args::DaemonCommand;
use glass_browser::development::{Actor, ToolAuthorization, ToolCall};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncWrite, AsyncWriteExt, BufReader};

const PROTOCOL_VERSION: u32 = 1;
const MAX_REQUEST_BYTES: usize = 1024 * 1024;
const MAX_RESPONSE_BYTES: usize = 1024 * 1024;
const MAX_WORKSPACES: usize = 8;
const MAX_CLIENTS: usize = 16;
const WORKSPACE_COMMAND_CAPACITY: usize = 64;

type WorkspaceRegistry = Rc<RefCell<BTreeMap<String, WorkspaceActorHandle>>>;

#[derive(Clone)]
struct WorkspaceActorHandle {
    sender: tokio::sync::mpsc::Sender<WorkspaceCommand>,
    summary: Value,
}

enum WorkspaceCommand {
    Inspect {
        response: tokio::sync::oneshot::Sender<Result<Value, String>>,
    },
    Tool {
        call: ToolCall,
        context: Box<DevelopmentToolContext>,
        response: tokio::sync::oneshot::Sender<Result<Value, String>>,
    },
    Shutdown {
        response: tokio::sync::oneshot::Sender<()>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DevelopmentDaemonStatus {
    pub protocol_version: u32,
    pub state: String,
    pub pid: u32,
    pub socket: PathBuf,
    pub status_path: PathBuf,
    pub token_path: PathBuf,
    pub started_at_ms: u128,
    pub workspace_count: usize,
    pub client_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DevelopmentDaemonRequest {
    pub id: String,
    pub token: String,
    pub operation: String,
    pub workspace_id: Option<String>,
    pub root: Option<PathBuf>,
    pub call: Option<ToolCall>,
    pub expected_generation: Option<u64>,
    pub expected_project_revision: Option<u64>,
    #[serde(default)]
    pub allow_mutation: bool,
    #[serde(default)]
    pub confirmed: bool,
    pub actor: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DevelopmentDaemonResponse {
    pub id: String,
    pub ok: bool,
    pub result: Value,
    pub error: Option<String>,
}

pub async fn dispatch(action: &DaemonCommand) -> Result<(), Box<dyn std::error::Error>> {
    match action {
        DaemonCommand::Start { socket, status } => {
            println!(
                "{}",
                serde_json::to_string_pretty(&start(socket.as_deref(), status.as_deref()).await?)?
            );
        }
        DaemonCommand::Status { socket, status } | DaemonCommand::Doctor { socket, status } => {
            println!(
                "{}",
                serde_json::to_string_pretty(&read_status_checked(
                    socket.as_deref(),
                    status.as_deref()
                )?)?
            );
        }
        DaemonCommand::Stop { socket, status } => {
            stop(socket.as_deref(), status.as_deref())?;
            println!("{{\"status\":\"stopped\"}}");
        }
        DaemonCommand::Logs { status } => {
            let (_, default_status) = default_paths();
            let status = status.as_deref().unwrap_or(&default_status);
            print!("{}", bounded_log(status)?);
        }
        DaemonCommand::AcknowledgeRecovery { .. } => {
            println!("{{\"status\":\"no-recovery-required\"}}");
        }
        DaemonCommand::Serve { socket, status } => serve(socket, status).await?,
    }
    Ok(())
}

pub fn default_paths() -> (PathBuf, PathBuf) {
    #[cfg(windows)]
    let base =
        dirs::data_local_dir().unwrap_or_else(|| std::env::temp_dir().join("glass-current-user"));
    #[cfg(not(windows))]
    let base = std::env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("XDG_DATA_HOME").map(PathBuf::from))
        .unwrap_or_else(|| std::env::temp_dir().join(format!("glass-{}", effective_uid())));
    let root = base.join("glass-dev");
    #[cfg(windows)]
    {
        use std::hash::{Hash, Hasher};
        let mut hasher = std::hash::DefaultHasher::new();
        root.hash(&mut hasher);
        let endpoint = PathBuf::from(format!(r"\\.\pipe\glass-dev-{:016x}", hasher.finish()));
        (endpoint, root.join("glassd.json"))
    }
    #[cfg(not(windows))]
    {
        (root.join("glassd.sock"), root.join("glassd.json"))
    }
}

pub async fn start(
    socket: Option<&Path>,
    status_path: Option<&Path>,
) -> Result<DevelopmentDaemonStatus, Box<dyn std::error::Error>> {
    #[cfg(not(any(unix, windows)))]
    {
        let _ = (socket, status_path);
        return Err("the durable development daemon currently requires Unix local sockets".into());
    }
    #[cfg(unix)]
    {
        let (default_socket, default_status) = default_paths();
        let socket = socket.unwrap_or(&default_socket);
        let status_path = status_path.unwrap_or(&default_status);
        if let Ok(existing) = read_status(status_path)
            && process_alive(existing.pid)
        {
            return Err(
                format!("development daemon is already running as {}", existing.pid).into(),
            );
        }
        remove_socket_if_safe(socket)?;
        if status_path.exists() {
            std::fs::remove_file(status_path)?;
        }
        std::fs::create_dir_all(socket.parent().unwrap_or_else(|| Path::new(".")))?;
        std::fs::create_dir_all(status_path.parent().unwrap_or_else(|| Path::new(".")))?;
        let log = log_path(status_path);
        let stdout = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&log)?;
        let stderr = stdout.try_clone()?;
        std::process::Command::new(std::env::current_exe()?)
            .args(["daemon", "serve"])
            .arg("--socket")
            .arg(socket)
            .arg("--status")
            .arg(status_path)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::from(stdout))
            .stderr(std::process::Stdio::from(stderr))
            .spawn()?;
        let deadline = std::time::Instant::now() + Duration::from_secs(3);
        loop {
            if let Ok(status) = read_status(status_path)
                && process_alive(status.pid)
                && status.socket == socket
            {
                return Ok(status);
            }
            if std::time::Instant::now() >= deadline {
                return Err("development daemon did not become ready within three seconds".into());
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
    }
    #[cfg(windows)]
    {
        let (default_socket, default_status) = default_paths();
        let socket = socket.unwrap_or(&default_socket);
        let status_path = status_path.unwrap_or(&default_status);
        validate_windows_pipe_name(socket)?;
        if let Ok(existing) = read_status(status_path)
            && process_alive(existing.pid)
        {
            return Err(
                format!("development daemon is already running as {}", existing.pid).into(),
            );
        }
        if status_path.exists() {
            std::fs::remove_file(status_path)?;
        }
        std::fs::create_dir_all(status_path.parent().unwrap_or_else(|| Path::new(".")))?;
        let log = log_path(status_path);
        let stdout = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&log)?;
        let stderr = stdout.try_clone()?;
        std::process::Command::new(std::env::current_exe()?)
            .args(["daemon", "serve"])
            .arg("--socket")
            .arg(socket)
            .arg("--status")
            .arg(status_path)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::from(stdout))
            .stderr(std::process::Stdio::from(stderr))
            .spawn()?;
        let deadline = std::time::Instant::now() + Duration::from_secs(3);
        loop {
            if let Ok(status) = read_status(status_path)
                && process_alive(status.pid)
                && status.socket == socket
            {
                return Ok(status);
            }
            if std::time::Instant::now() >= deadline {
                return Err("development daemon did not become ready within three seconds".into());
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
    }
}

pub fn read_status_checked(
    socket: Option<&Path>,
    status_path: Option<&Path>,
) -> Result<DevelopmentDaemonStatus, Box<dyn std::error::Error>> {
    let (default_socket, default_status) = default_paths();
    let expected_socket = socket.unwrap_or(&default_socket);
    let status_path = status_path.unwrap_or(&default_status);
    let mut status = read_status(status_path)?;
    validate_status(&status, status_path)?;
    if status.socket != expected_socket {
        return Err("daemon status socket does not match the requested socket".into());
    }
    if !process_alive(status.pid) {
        status.state = "stopped".into();
    }
    Ok(status)
}

pub fn stop(
    socket: Option<&Path>,
    status_path: Option<&Path>,
) -> Result<(), Box<dyn std::error::Error>> {
    #[cfg(not(any(unix, windows)))]
    {
        let _ = (socket, status_path);
        return Err("the durable development daemon currently requires Unix".into());
    }
    #[cfg(unix)]
    {
        let status = read_status_checked(socket, status_path)?;
        if status.pid == std::process::id() {
            return Err("refusing to stop the calling process".into());
        }
        let result = unsafe { libc::kill(status.pid as i32, libc::SIGTERM) };
        if result != 0 {
            return Err(std::io::Error::last_os_error().into());
        }
        Ok(())
    }
    #[cfg(windows)]
    {
        use windows_sys::Win32::System::Threading::{
            OpenProcess, PROCESS_TERMINATE, TerminateProcess,
        };
        let status = read_status_checked(socket, status_path)?;
        if status.pid == std::process::id() {
            return Err("refusing to stop the calling process".into());
        }
        let process = unsafe { OpenProcess(PROCESS_TERMINATE, 0, status.pid) };
        if process.is_null() {
            return Err(std::io::Error::last_os_error().into());
        }
        let terminated = unsafe { TerminateProcess(process, 0) };
        unsafe { windows_sys::Win32::Foundation::CloseHandle(process) };
        if terminated == 0 {
            return Err(std::io::Error::last_os_error().into());
        }
        Ok(())
    }
}

pub async fn serve(socket: &Path, status_path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    serve_with_store(
        socket,
        status_path,
        WorkspaceTrustStore::platform_default()?,
    )
    .await
}

async fn serve_with_store(
    socket: &Path,
    status_path: &Path,
    trust_store: WorkspaceTrustStore,
) -> Result<(), Box<dyn std::error::Error>> {
    #[cfg(not(any(unix, windows)))]
    {
        let _ = (socket, status_path);
        return Err("the durable development daemon currently requires Unix".into());
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        use tokio::net::UnixListener;
        use tokio::signal::unix::{SignalKind, signal};

        remove_socket_if_safe(socket)?;
        std::fs::create_dir_all(socket.parent().unwrap_or_else(|| Path::new(".")))?;
        std::fs::create_dir_all(status_path.parent().unwrap_or_else(|| Path::new(".")))?;
        let listener = UnixListener::bind(socket)?;
        std::fs::set_permissions(socket, std::fs::Permissions::from_mode(0o600))?;
        let token_path = token_path(status_path);
        let token = create_token()?;
        write_private(&token_path, token.as_bytes())?;
        let status = DevelopmentDaemonStatus {
            protocol_version: PROTOCOL_VERSION,
            state: "running".into(),
            pid: std::process::id(),
            socket: socket.to_path_buf(),
            status_path: status_path.to_path_buf(),
            token_path: token_path.clone(),
            started_at_ms: now_ms(),
            workspace_count: 0,
            client_count: 0,
        };
        write_status(status_path, &status)?;
        let workspaces: WorkspaceRegistry = Rc::new(RefCell::new(BTreeMap::new()));
        let clients = std::rc::Rc::new(std::cell::Cell::new(0_usize));
        let local = tokio::task::LocalSet::new();
        let mut terminate = signal(SignalKind::terminate())?;
        local
            .run_until(async {
                loop {
                    let (stream, _) = tokio::select! {
                        accepted = listener.accept() => accepted?,
                        _ = terminate.recv() => break,
                    };
                    authorize_local_peer(&stream)?;
                    if clients.get() >= MAX_CLIENTS {
                        continue;
                    }
                    clients.set(clients.get() + 1);
                    update_counts(status_path, workspaces.borrow().len(), clients.get())?;
                    let workspaces = Rc::clone(&workspaces);
                    let clients = Rc::clone(&clients);
                    let token = token.clone();
                    let status_path = status_path.to_path_buf();
                    let socket = socket.to_path_buf();
                    let trust_store = trust_store.clone();
                    tokio::task::spawn_local(async move {
                        if let Err(error) =
                            handle_client(stream, &token, &socket, &workspaces, &trust_store).await
                        {
                            tracing::warn!(%error, "development daemon client failed");
                        }
                        clients.set(clients.get().saturating_sub(1));
                        let workspace_count = workspaces.borrow().len();
                        let _ = update_counts(&status_path, workspace_count, clients.get());
                    });
                }
                Ok::<(), Box<dyn std::error::Error>>(())
            })
            .await?;
        let _ = std::fs::remove_file(status_path);
        let _ = std::fs::remove_file(token_path);
        remove_socket_if_safe(socket)?;
        Ok(())
    }
    #[cfg(windows)]
    {
        use tokio::net::windows::named_pipe::ServerOptions;

        validate_windows_pipe_name(socket)?;
        std::fs::create_dir_all(status_path.parent().unwrap_or_else(|| Path::new(".")))?;
        let token_path = token_path(status_path);
        let token = create_token()?;
        write_private(&token_path, token.as_bytes())?;
        let status = DevelopmentDaemonStatus {
            protocol_version: PROTOCOL_VERSION,
            state: "running".into(),
            pid: std::process::id(),
            socket: socket.to_path_buf(),
            status_path: status_path.to_path_buf(),
            token_path: token_path.clone(),
            started_at_ms: now_ms(),
            workspace_count: 0,
            client_count: 0,
        };
        write_status(status_path, &status)?;
        let workspaces: WorkspaceRegistry = Rc::new(RefCell::new(BTreeMap::new()));
        let clients = Rc::new(std::cell::Cell::new(0_usize));
        let local = tokio::task::LocalSet::new();
        let pipe_name = socket.to_string_lossy().to_string();
        local
            .run_until(async {
                let mut first = true;
                loop {
                    let server = ServerOptions::new()
                        .first_pipe_instance(first)
                        .reject_remote_clients(true)
                        .create(&pipe_name)?;
                    first = false;
                    tokio::select! {
                        connected = server.connect() => connected?,
                        _ = tokio::signal::ctrl_c() => break,
                    }
                    if clients.get() >= MAX_CLIENTS {
                        continue;
                    }
                    clients.set(clients.get() + 1);
                    update_counts(status_path, workspaces.borrow().len(), clients.get())?;
                    let workspaces = Rc::clone(&workspaces);
                    let clients = Rc::clone(&clients);
                    let token = token.clone();
                    let status_path = status_path.to_path_buf();
                    let socket = socket.to_path_buf();
                    let trust_store = trust_store.clone();
                    tokio::task::spawn_local(async move {
                        if let Err(error) =
                            handle_stream(server, &token, &socket, &workspaces, &trust_store).await
                        {
                            tracing::warn!(%error, "development daemon named-pipe client failed");
                        }
                        clients.set(clients.get().saturating_sub(1));
                        let _ =
                            update_counts(&status_path, workspaces.borrow().len(), clients.get());
                    });
                }
                Ok::<(), Box<dyn std::error::Error>>(())
            })
            .await?;
        let _ = std::fs::remove_file(status_path);
        let _ = std::fs::remove_file(token_path);
        Ok(())
    }
}

#[cfg(unix)]
async fn handle_client(
    stream: tokio::net::UnixStream,
    token: &str,
    socket: &Path,
    workspaces: &WorkspaceRegistry,
    trust_store: &WorkspaceTrustStore,
) -> Result<(), Box<dyn std::error::Error>> {
    handle_stream(stream, token, socket, workspaces, trust_store).await
}

async fn handle_stream<S>(
    stream: S,
    token: &str,
    socket: &Path,
    workspaces: &WorkspaceRegistry,
    trust_store: &WorkspaceTrustStore,
) -> Result<(), Box<dyn std::error::Error>>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let (read, mut write) = tokio::io::split(stream);
    let mut lines = BufReader::new(read).lines();
    while let Some(line) = lines.next_line().await? {
        let response = if line.len() > MAX_REQUEST_BYTES {
            DevelopmentDaemonResponse {
                id: "oversized".into(),
                ok: false,
                result: Value::Null,
                error: Some("daemon request exceeds the size limit".into()),
            }
        } else {
            match serde_json::from_str::<DevelopmentDaemonRequest>(&line) {
                Ok(request) => {
                    execute_request(request, token, socket, workspaces, trust_store).await
                }
                Err(error) => DevelopmentDaemonResponse {
                    id: "invalid".into(),
                    ok: false,
                    result: Value::Null,
                    error: Some(format!("invalid daemon request: {error}")),
                },
            }
        };
        let encoded = serde_json::to_vec(&response)?;
        if encoded.len() > MAX_RESPONSE_BYTES {
            return Err("daemon response exceeds the size limit".into());
        }
        write.write_all(&encoded).await?;
        write.write_all(b"\n").await?;
        write.flush().await?;
    }
    Ok(())
}

async fn execute_request(
    request: DevelopmentDaemonRequest,
    token: &str,
    socket: &Path,
    workspaces: &WorkspaceRegistry,
    trust_store: &WorkspaceTrustStore,
) -> DevelopmentDaemonResponse {
    let id = request.id.clone();
    let result = async {
        validate_request(&request, token)?;
        match request.operation.as_str() {
            "ping" => Ok(serde_json::json!({"protocolVersion":PROTOCOL_VERSION})),
            "workspace.list" => Ok(Value::Array(
                workspaces
                    .borrow()
                    .values()
                    .map(|handle| handle.summary.clone())
                    .collect(),
            )),
            "workspace.open" => {
                let workspace_id = required_workspace_id(&request)?;
                let root = request
                    .root
                    .as_deref()
                    .ok_or("workspace.open requires root")?;
                if workspaces.borrow().contains_key(workspace_id) {
                    return Err("workspace identity is already open".into());
                }
                if workspaces.borrow().len() >= MAX_WORKSPACES {
                    return Err("daemon workspace quota reached".into());
                }
                let mut workspace =
                    DevelopmentWorkspace::open_with_store(root, trust_store.clone())
                        .map_err(|error| error.to_string())?;
                workspace
                    .agents()
                    .set_resident_broker(ResidentAgentBroker {
                        socket: socket.to_path_buf(),
                        token: token.to_string(),
                        workspace_id: workspace_id.to_string(),
                    })
                    .map_err(|error| error.to_string())?;
                let result = workspace_summary(workspace_id, &workspace);
                let (sender, receiver) = tokio::sync::mpsc::channel(WORKSPACE_COMMAND_CAPACITY);
                tokio::task::spawn_local(run_workspace_actor(
                    workspace_id.into(),
                    workspace,
                    receiver,
                ));
                let mut state = workspaces.borrow_mut();
                if state.contains_key(workspace_id) || state.len() >= MAX_WORKSPACES {
                    return Err("daemon workspace registry changed while opening".into());
                }
                state.insert(
                    workspace_id.into(),
                    WorkspaceActorHandle {
                        sender,
                        summary: result.clone(),
                    },
                );
                Ok(result)
            }
            "workspace.inspect" => {
                let workspace_id = required_workspace_id(&request)?;
                workspace_inspect(workspace_handle(workspaces, workspace_id)?).await
            }
            "workspace.tool" => {
                let workspace_id = required_workspace_id(&request)?;
                let call = request
                    .call
                    .as_ref()
                    .ok_or("workspace.tool requires call")?;
                let actor = request
                    .actor
                    .as_deref()
                    .map(Actor::external)
                    .unwrap_or_else(Actor::local);
                let context = DevelopmentToolContext {
                    authorization: ToolAuthorization {
                        actor,
                        allow_mutation: request.allow_mutation,
                        confirmed: request.confirmed,
                    },
                    expected_generation: request
                        .expected_generation
                        .ok_or("workspace.tool requires expectedGeneration")?,
                    expected_project_revision: request
                        .expected_project_revision
                        .ok_or("workspace.tool requires expectedProjectRevision")?,
                };
                workspace_tool(
                    workspace_handle(workspaces, workspace_id)?,
                    call.clone(),
                    context,
                )
                .await
            }
            "workspace.close" => {
                let workspace_id = required_workspace_id(&request)?;
                let handle = workspaces.borrow_mut().remove(workspace_id);
                if let Some(handle) = handle {
                    let (response, received) = tokio::sync::oneshot::channel();
                    handle
                        .sender
                        .send(WorkspaceCommand::Shutdown { response })
                        .await
                        .map_err(|_| "workspace actor command queue closed")?;
                    received.await.map_err(|_| "workspace actor did not stop")?;
                    Ok(serde_json::json!({"closed":true}))
                } else {
                    Ok(serde_json::json!({"closed":false}))
                }
            }
            _ => Err("unknown daemon operation".into()),
        }
    }
    .await;
    match result {
        Ok(result) => DevelopmentDaemonResponse {
            id,
            ok: true,
            result,
            error: None,
        },
        Err(error) => DevelopmentDaemonResponse {
            id,
            ok: false,
            result: Value::Null,
            error: Some(error),
        },
    }
}

fn workspace_handle(
    workspaces: &WorkspaceRegistry,
    workspace_id: &str,
) -> Result<WorkspaceActorHandle, String> {
    workspaces
        .borrow()
        .get(workspace_id)
        .cloned()
        .ok_or_else(|| "unknown durable workspace".into())
}

async fn workspace_inspect(handle: WorkspaceActorHandle) -> Result<Value, String> {
    let (response, received) = tokio::sync::oneshot::channel();
    handle
        .sender
        .send(WorkspaceCommand::Inspect { response })
        .await
        .map_err(|_| "workspace actor command queue closed")?;
    received
        .await
        .map_err(|_| "workspace actor dropped its inspect response")?
}

async fn workspace_tool(
    handle: WorkspaceActorHandle,
    call: ToolCall,
    context: DevelopmentToolContext,
) -> Result<Value, String> {
    let (response, received) = tokio::sync::oneshot::channel();
    handle
        .sender
        .send(WorkspaceCommand::Tool {
            call,
            context: Box::new(context),
            response,
        })
        .await
        .map_err(|_| "workspace actor command queue closed")?;
    received
        .await
        .map_err(|_| "workspace actor dropped its tool response")?
}

async fn run_workspace_actor(
    workspace_id: String,
    mut workspace: DevelopmentWorkspace,
    mut commands: tokio::sync::mpsc::Receiver<WorkspaceCommand>,
) {
    let mut tick = tokio::time::interval(Duration::from_millis(50));
    tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    loop {
        tokio::select! {
            biased;
            command = commands.recv() => match command {
                Some(WorkspaceCommand::Inspect { response }) => {
                    let result = inspect_workspace(&workspace_id, &mut workspace);
                    let _ = response.send(result);
                }
                Some(WorkspaceCommand::Tool { call, context, response }) => {
                    let result = workspace
                        .execute_tool(&call, &context)
                        .map_err(|error| error.to_string());
                    let _ = response.send(result);
                }
                Some(WorkspaceCommand::Shutdown { response }) => {
                    let _ = response.send(());
                    break;
                }
                None => break,
            },
            _ = tick.tick() => {
                if let Err(error) = workspace.tasks() {
                    tracing::warn!(workspace_id, %error, "workspace task scheduler tick failed");
                }
            }
        }
    }
}

fn inspect_workspace(
    workspace_id: &str,
    workspace: &mut DevelopmentWorkspace,
) -> Result<Value, String> {
    let agents = workspace
        .agents()
        .list()
        .map_err(|error| error.to_string())?;
    let tasks = workspace.tasks().map_err(|error| error.to_string())?;
    let processes = workspace
        .project_mut()
        .processes()
        .list_checked()
        .map_err(|error| error.to_string())?;
    Ok(serde_json::json!({
        "workspace":workspace_summary(workspace_id, workspace),
        "processes":processes,
        "agents":agents,
        "tasks":tasks,
        "kernels":workspace.kernels().snapshots().collect::<Vec<_>>(),
        "debuggers":workspace.debugger_names().collect::<Vec<_>>(),
        "browser":workspace.browser().state().map_err(|error| error.to_string())?
    }))
}

#[cfg(unix)]
pub async fn request(
    socket: &Path,
    request: &DevelopmentDaemonRequest,
) -> Result<DevelopmentDaemonResponse, Box<dyn std::error::Error>> {
    let stream = tokio::net::UnixStream::connect(socket).await?;
    request_stream(stream, request).await
}

#[cfg(windows)]
pub async fn request(
    socket: &Path,
    request: &DevelopmentDaemonRequest,
) -> Result<DevelopmentDaemonResponse, Box<dyn std::error::Error>> {
    use tokio::net::windows::named_pipe::ClientOptions;
    validate_windows_pipe_name(socket)?;
    let deadline = std::time::Instant::now() + Duration::from_secs(3);
    let pipe_name = socket.to_string_lossy();
    let stream = loop {
        match ClientOptions::new().open(pipe_name.as_ref()) {
            Ok(stream) => break stream,
            Err(error)
                if error.raw_os_error() == Some(231) && std::time::Instant::now() < deadline =>
            {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
            Err(error) => return Err(error.into()),
        }
    };
    request_stream(stream, request).await
}

async fn request_stream<S>(
    stream: S,
    request: &DevelopmentDaemonRequest,
) -> Result<DevelopmentDaemonResponse, Box<dyn std::error::Error>>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let (read, mut write) = tokio::io::split(stream);
    let encoded = serde_json::to_vec(request)?;
    if encoded.len() > MAX_REQUEST_BYTES {
        return Err("daemon request exceeds the size limit".into());
    }
    write.write_all(&encoded).await?;
    write.write_all(b"\n").await?;
    write.flush().await?;
    let mut line = String::new();
    BufReader::new(read).read_line(&mut line).await?;
    Ok(serde_json::from_str(&line)?)
}

#[cfg(not(any(unix, windows)))]
pub async fn request(
    _socket: &Path,
    _request: &DevelopmentDaemonRequest,
) -> Result<DevelopmentDaemonResponse, Box<dyn std::error::Error>> {
    Err("the local development daemon requires Unix-domain sockets on this platform".into())
}

pub async fn forward_resident_tool_file(
    path: &Path,
    root: &Path,
    allow_mutation: bool,
    confirmed: bool,
) -> Result<Value, Box<dyn std::error::Error>> {
    let socket = PathBuf::from(
        std::env::var_os("GLASS_DEV_DAEMON_SOCKET")
            .ok_or("resident Pi broker socket is missing")?,
    );
    let token = std::env::var("GLASS_DEV_DAEMON_TOKEN")
        .map_err(|_| "resident Pi broker token is missing")?;
    let workspace_id = std::env::var("GLASS_DEV_DAEMON_WORKSPACE")
        .map_err(|_| "resident Pi broker workspace is missing")?;
    forward_resident_tool_file_with_context(
        &socket,
        &token,
        &workspace_id,
        path,
        root,
        allow_mutation,
        confirmed,
    )
    .await
}

async fn forward_resident_tool_file_with_context(
    socket: &Path,
    token: &str,
    workspace_id: &str,
    path: &Path,
    root: &Path,
    allow_mutation: bool,
    confirmed: bool,
) -> Result<Value, Box<dyn std::error::Error>> {
    let call = read_private_tool_call(path)?;
    forward_resident_tool_call(
        socket,
        token,
        workspace_id,
        &call,
        root,
        allow_mutation,
        confirmed,
    )
    .await
}

pub(crate) async fn forward_resident_tool_call_with_context(
    broker: &ResidentAgentBroker,
    call: &ToolCall,
    root: &Path,
    allow_mutation: bool,
    confirmed: bool,
) -> Result<Value, Box<dyn std::error::Error>> {
    forward_resident_tool_call(
        &broker.socket,
        &broker.token,
        &broker.workspace_id,
        call,
        root,
        allow_mutation,
        confirmed,
    )
    .await
}

async fn forward_resident_tool_call(
    socket: &Path,
    token: &str,
    workspace_id: &str,
    call: &ToolCall,
    root: &Path,
    allow_mutation: bool,
    confirmed: bool,
) -> Result<Value, Box<dyn std::error::Error>> {
    let inspect = request(
        socket,
        &DevelopmentDaemonRequest {
            id: format!("inspect-{}", call.id),
            token: token.to_string(),
            operation: "workspace.inspect".into(),
            workspace_id: Some(workspace_id.to_string()),
            root: None,
            call: None,
            expected_generation: None,
            expected_project_revision: None,
            allow_mutation: false,
            confirmed: false,
            actor: Some("pi".into()),
        },
    )
    .await?;
    if !inspect.ok {
        return Err(inspect
            .error
            .unwrap_or_else(|| "workspace inspect failed".into())
            .into());
    }
    let expected_root = std::fs::canonicalize(root)?;
    let observed_root = inspect
        .result
        .pointer("/workspace/root")
        .and_then(Value::as_str);
    if observed_root != expected_root.to_str() {
        return Err("resident Pi broker root does not match the durable workspace".into());
    }
    let generation = inspect
        .result
        .pointer("/workspace/generation")
        .and_then(Value::as_u64)
        .ok_or("durable workspace generation is missing")?;
    let revision = inspect
        .result
        .pointer("/workspace/projectRevision")
        .and_then(Value::as_u64)
        .ok_or("durable workspace revision is missing")?;
    let response = request(
        socket,
        &DevelopmentDaemonRequest {
            id: format!("tool-{}", call.id),
            token: token.to_string(),
            operation: "workspace.tool".into(),
            workspace_id: Some(workspace_id.to_string()),
            root: None,
            call: Some(call.clone()),
            expected_generation: Some(generation),
            expected_project_revision: Some(revision),
            allow_mutation,
            confirmed,
            actor: Some("pi".into()),
        },
    )
    .await?;
    if response.ok {
        Ok(response.result)
    } else {
        Err(response
            .error
            .unwrap_or_else(|| "resident tool call failed".into())
            .into())
    }
}

pub(crate) fn read_private_tool_call(path: &Path) -> Result<ToolCall, Box<dyn std::error::Error>> {
    use std::io::Read;
    const MAX_CALL_BYTES: u64 = 256 * 1024;
    let canonical = path.canonicalize()?;
    let temporary_root = std::env::temp_dir().canonicalize()?;
    if canonical.parent() != Some(temporary_root.as_path())
        || !canonical
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.starts_with("glass-pi-call-") && name.ends_with(".json"))
    {
        return Err("Pi broker requests must use a private Glass temporary file".into());
    }
    let mut options = std::fs::OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NOFOLLOW);
    }
    let file = options.open(&canonical)?;
    let metadata = file.metadata()?;
    if !metadata.is_file() || metadata.len() > MAX_CALL_BYTES {
        return Err("invalid Pi broker request file".into());
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::{MetadataExt, PermissionsExt};
        if metadata.uid() != unsafe { libc::geteuid() }
            || metadata.permissions().mode() & 0o077 != 0
        {
            return Err("Pi broker request must be owned by this user with mode 0600".into());
        }
    }
    let mut encoded = Vec::with_capacity(metadata.len() as usize);
    file.take(MAX_CALL_BYTES + 1).read_to_end(&mut encoded)?;
    std::fs::remove_file(&canonical)?;
    if encoded.len() as u64 > MAX_CALL_BYTES {
        return Err("Pi broker request exceeds the size limit".into());
    }
    Ok(serde_json::from_slice(&encoded)?)
}

fn workspace_summary(id: &str, workspace: &DevelopmentWorkspace) -> Value {
    serde_json::json!({
        "id":id,
        "root":workspace.root(),
        "trust":workspace.trust(),
        "generation":workspace.generation(),
        "projectRevision":workspace.project().revision()
    })
}

fn validate_request(request: &DevelopmentDaemonRequest, token: &str) -> Result<(), String> {
    if request.token.as_bytes() != token.as_bytes() {
        return Err("invalid daemon authentication token".into());
    }
    validate_identifier(&request.id, "request")?;
    if request.operation.is_empty()
        || request.operation.len() > 128
        || request.operation.chars().any(char::is_control)
    {
        return Err("invalid daemon operation".into());
    }
    Ok(())
}

fn required_workspace_id(request: &DevelopmentDaemonRequest) -> Result<&str, String> {
    let id = request
        .workspace_id
        .as_deref()
        .ok_or("daemon request requires workspaceId")?;
    validate_identifier(id, "workspace")?;
    Ok(id)
}

fn validate_identifier(value: &str, description: &str) -> Result<(), String> {
    if value.is_empty()
        || value.len() > 128
        || !value
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || "-_.".contains(character))
    {
        return Err(format!("invalid {description} identifier"));
    }
    Ok(())
}

fn read_status(path: &Path) -> Result<DevelopmentDaemonStatus, Box<dyn std::error::Error>> {
    let bytes = std::fs::read(path)?;
    if bytes.len() > 64 * 1024 {
        return Err("daemon status exceeds the size limit".into());
    }
    Ok(serde_json::from_slice(&bytes)?)
}

fn validate_status(
    status: &DevelopmentDaemonStatus,
    path: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    if status.protocol_version != PROTOCOL_VERSION || status.status_path != path {
        return Err("invalid development daemon status contract".into());
    }
    if status.workspace_count > MAX_WORKSPACES || status.client_count > MAX_CLIENTS {
        return Err("daemon status exceeds resource quotas".into());
    }
    Ok(())
}

fn write_status(
    path: &Path,
    status: &DevelopmentDaemonStatus,
) -> Result<(), Box<dyn std::error::Error>> {
    let encoded = serde_json::to_vec_pretty(status)?;
    write_private(path, &encoded)
}

fn update_counts(
    status_path: &Path,
    workspace_count: usize,
    client_count: usize,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut status = read_status(status_path)?;
    status.workspace_count = workspace_count;
    status.client_count = client_count;
    write_status(status_path, &status)
}

fn write_private(path: &Path, bytes: &[u8]) -> Result<(), Box<dyn std::error::Error>> {
    use std::io::Write;
    #[cfg(unix)]
    use std::os::unix::fs::OpenOptionsExt;
    let temporary = path.with_extension(format!("tmp-{}", std::process::id()));
    let mut options = std::fs::OpenOptions::new();
    options.create_new(true).write(true);
    #[cfg(unix)]
    options.mode(0o600);
    let mut file = options.open(&temporary)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    std::fs::rename(temporary, path)?;
    Ok(())
}

fn create_token() -> Result<String, Box<dyn std::error::Error>> {
    let mut bytes = [0_u8; 32];
    getrandom::fill(&mut bytes)?;
    Ok(bytes.iter().map(|byte| format!("{byte:02x}")).collect())
}

fn token_path(status: &Path) -> PathBuf {
    status.with_extension("token")
}

fn log_path(status: &Path) -> PathBuf {
    status.with_extension("log")
}

fn bounded_log(status: &Path) -> Result<String, Box<dyn std::error::Error>> {
    use std::io::{Read, Seek, SeekFrom};
    let mut file = std::fs::File::open(log_path(status))?;
    let length = file.metadata()?.len();
    let start = length.saturating_sub(64 * 1024);
    file.seek(SeekFrom::Start(start))?;
    let mut output = String::new();
    file.read_to_string(&mut output)?;
    Ok(output)
}

#[cfg(unix)]
fn authorize_local_peer(stream: &tokio::net::UnixStream) -> Result<(), Box<dyn std::error::Error>> {
    #[cfg(target_os = "linux")]
    {
        use std::os::fd::AsRawFd;
        let mut credentials = libc::ucred {
            pid: 0,
            uid: 0,
            gid: 0,
        };
        let mut length = std::mem::size_of::<libc::ucred>() as libc::socklen_t;
        let result = unsafe {
            libc::getsockopt(
                stream.as_raw_fd(),
                libc::SOL_SOCKET,
                libc::SO_PEERCRED,
                (&mut credentials as *mut libc::ucred).cast(),
                &mut length,
            )
        };
        if result != 0 || credentials.uid != unsafe { libc::geteuid() } {
            return Err("daemon rejected a non-local-user peer".into());
        }
    }
    Ok(())
}

#[cfg(unix)]
fn remove_socket_if_safe(path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    use std::os::unix::fs::FileTypeExt;
    match std::fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_socket() => std::fs::remove_file(path)?,
        Ok(_) => return Err(format!("refusing to replace non-socket {}", path.display()).into()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.into()),
    }
    Ok(())
}

#[cfg(not(unix))]
fn remove_socket_if_safe(_path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    Ok(())
}

fn process_alive(pid: u32) -> bool {
    #[cfg(unix)]
    {
        unsafe { libc::kill(pid as i32, 0) == 0 }
    }
    #[cfg(windows)]
    {
        use windows_sys::Win32::Foundation::{CloseHandle, STILL_ACTIVE};
        use windows_sys::Win32::System::Threading::{
            GetExitCodeProcess, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION,
        };
        let process = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
        if process.is_null() {
            return false;
        }
        let mut exit_code = 0_u32;
        let success = unsafe { GetExitCodeProcess(process, &mut exit_code) } != 0;
        unsafe { CloseHandle(process) };
        success && exit_code == STILL_ACTIVE as u32
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = pid;
        false
    }
}

#[cfg(windows)]
fn validate_windows_pipe_name(path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let value = path
        .to_str()
        .ok_or("Windows named-pipe endpoint must be Unicode")?;
    if !valid_windows_pipe_name(value) {
        return Err("invalid Windows Glass daemon pipe name".into());
    }
    Ok(())
}

#[cfg(any(windows, test))]
fn valid_windows_pipe_name(value: &str) -> bool {
    value
        .strip_prefix(r"\\.\pipe\glass-dev-")
        .is_some_and(|name| {
            !name.is_empty()
                && name.len() <= 64
                && name
                    .chars()
                    .all(|character| character.is_ascii_alphanumeric() || character == '-')
        })
}

fn effective_uid() -> u32 {
    #[cfg(unix)]
    {
        unsafe { libc::geteuid() }
    }
    #[cfg(not(unix))]
    {
        0
    }
}

fn now_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT_ROOT: AtomicU64 = AtomicU64::new(0);

    #[test]
    fn windows_pipe_names_are_local_and_glass_owned() {
        assert!(valid_windows_pipe_name(
            r"\\.\pipe\glass-dev-0123456789abcdef"
        ));
        assert!(!valid_windows_pipe_name(r"\\server\pipe\glass-dev-test"));
        assert!(!valid_windows_pipe_name(r"\\.\pipe\other-product"));
        assert!(!valid_windows_pipe_name(r"\\.\pipe\glass-dev-bad/name"));
    }

    fn test_context() -> DevelopmentToolContext {
        DevelopmentToolContext {
            authorization: ToolAuthorization {
                actor: Actor::external("daemon-concurrency-test"),
                allow_mutation: true,
                confirmed: true,
            },
            expected_generation: 1,
            expected_project_revision: 0,
        }
    }

    #[tokio::test(flavor = "current_thread")]
    async fn workspace_actors_do_not_serialize_unrelated_long_operations() {
        let sequence = NEXT_ROOT.fetch_add(1, Ordering::Relaxed);
        let base = std::env::temp_dir().join(format!(
            "glassd-actor-concurrency-{}-{sequence}",
            std::process::id()
        ));
        let first_root = base.join("first");
        let second_root = base.join("second");
        std::fs::create_dir_all(&first_root).unwrap();
        std::fs::create_dir_all(&second_root).unwrap();
        let store = WorkspaceTrustStore::at(base.join("trust.json"));
        for root in [&first_root, &second_root] {
            store
                .trust_project(&crate::WorkspaceIdentity::inspect(root).unwrap())
                .unwrap();
        }
        let local = tokio::task::LocalSet::new();
        local
            .run_until(async {
                let actor = |id: &str, root: &Path| {
                    let workspace =
                        DevelopmentWorkspace::open_with_store(root, store.clone()).unwrap();
                    let summary = workspace_summary(id, &workspace);
                    let (sender, receiver) = tokio::sync::mpsc::channel(WORKSPACE_COMMAND_CAPACITY);
                    tokio::task::spawn_local(run_workspace_actor(id.into(), workspace, receiver));
                    WorkspaceActorHandle { sender, summary }
                };
                let first = actor("first", &first_root);
                let second = actor("second", &second_root);
                workspace_tool(
                    first.clone(),
                    ToolCall {
                        id: "shell-start".into(),
                        name: "glass.eval.start".into(),
                        arguments: serde_json::json!({"name":"slow","kind":"shell"}),
                    },
                    test_context(),
                )
                .await
                .unwrap();
                let slow = tokio::task::spawn_local(workspace_tool(
                    first,
                    ToolCall {
                        id: "shell-slow".into(),
                        name: "glass.eval.execute".into(),
                        arguments: serde_json::json!({
                            "name":"slow",
                            "code":"sleep 1; echo complete",
                            "timeoutSeconds":3
                        }),
                    },
                    test_context(),
                ));
                tokio::time::sleep(Duration::from_millis(50)).await;
                let started = std::time::Instant::now();
                let inspect = workspace_inspect(second).await.unwrap();
                let elapsed = started.elapsed();
                assert_eq!(inspect["workspace"]["id"], "second");
                assert!(
                    elapsed < Duration::from_millis(300),
                    "unrelated workspace inspect waited {elapsed:?}"
                );
                assert!(slow.await.unwrap().is_ok());
            })
            .await;
        std::fs::remove_dir_all(base).unwrap();
    }

    #[tokio::test(flavor = "current_thread")]
    #[cfg(unix)]
    async fn daemon_preserves_resources_across_client_disconnects() {
        let sequence = NEXT_ROOT.fetch_add(1, Ordering::Relaxed);
        let base =
            std::env::temp_dir().join(format!("glassd-test-{}-{sequence}", std::process::id()));
        let project = base.join("project");
        std::fs::create_dir_all(&project).unwrap();
        std::fs::write(
            project.join("Cargo.toml"),
            "[package]\nname='x'\nversion='0.1.0'\n",
        )
        .unwrap();
        let trust_store = WorkspaceTrustStore::at(base.join("trust.json"));
        trust_store
            .trust_project(&crate::WorkspaceIdentity::inspect(&project).unwrap())
            .unwrap();
        let socket = base.join("glassd.sock");
        let status = base.join("glassd.json");
        let local = tokio::task::LocalSet::new();
        local
            .run_until(async {
                let server_socket = socket.clone();
                let server_status = status.clone();
                let server_trust_store = trust_store.clone();
                let server = tokio::task::spawn_local(async move {
                    serve_with_store(&server_socket, &server_status, server_trust_store)
                        .await
                        .unwrap()
                });
                for _ in 0..100 {
                    if status.exists() && socket.exists() {
                        break;
                    }
                    tokio::time::sleep(Duration::from_millis(10)).await;
                }
                let token = std::fs::read_to_string(token_path(&status)).unwrap();
                let base_request = |id: &str, operation: &str| DevelopmentDaemonRequest {
                    id: id.into(),
                    token: token.clone(),
                    operation: operation.into(),
                    workspace_id: Some("fixture".into()),
                    root: None,
                    call: None,
                    expected_generation: None,
                    expected_project_revision: None,
                    allow_mutation: false,
                    confirmed: false,
                    actor: Some("test-client".into()),
                };
                let mut open = base_request("open", "workspace.open");
                open.root = Some(project.clone());
                assert!(request(&socket, &open).await.unwrap().ok);
                let mut start = base_request("start", "workspace.tool");
                start.call = Some(ToolCall {
                    id: "kernel-start".into(),
                    name: "glass.eval.start".into(),
                    arguments: serde_json::json!({"name":"durable","kind":"sql"}),
                });
                start.expected_generation = Some(1);
                start.expected_project_revision = Some(0);
                start.allow_mutation = true;
                start.confirmed = true;
                assert!(request(&socket, &start).await.unwrap().ok);
                let mut process_start = base_request("process-start", "workspace.tool");
                process_start.call = Some(ToolCall {
                    id: "process-start".into(),
                    name: "glass.process.start".into(),
                    arguments: serde_json::json!({"name":"durable-process","command":"sleep 30"}),
                });
                process_start.expected_generation = Some(1);
                process_start.expected_project_revision = Some(0);
                process_start.allow_mutation = true;
                process_start.confirmed = true;
                assert!(request(&socket, &process_start).await.unwrap().ok);
                let pi_available = std::process::Command::new("pi")
                    .arg("--version")
                    .output()
                    .is_ok();
                if pi_available {
                    let mut agent_start = base_request("agent-start", "workspace.tool");
                    agent_start.call = Some(ToolCall {
                        id: "agent-start".into(),
                        name: "glass.agent.spawn".into(),
                        arguments: serde_json::json!({"spec":{
                            "role":"durability-probe",
                            "task":"Remain idle while the client reconnects",
                            "dependencies":[],
                            "model":null,
                            "thinking":null,
                            "worktree":null,
                            "unrestricted":false,
                            "maxRuntimeSeconds":60,
                            "maxEvents":100
                        }}),
                    });
                    agent_start.expected_generation = Some(1);
                    agent_start.expected_project_revision = Some(1);
                    agent_start.allow_mutation = true;
                    agent_start.confirmed = true;
                    let agent_response = request(&socket, &agent_start).await.unwrap();
                    assert!(agent_response.ok, "{:?}", agent_response.error);
                }
                let browser_e2e = std::env::var("GLASS_E2E").as_deref() == Ok("1")
                    && std::env::var_os("GLASS_CHROME_PATH").is_some();
                if browser_e2e {
                    let port_probe = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
                    let browser_port = port_probe.local_addr().unwrap().port();
                    drop(port_probe);
                    let mut browser_start = base_request("browser-start", "workspace.tool");
                    browser_start.call = Some(ToolCall {
                        id: "browser-start".into(),
                        name: "glass.browser.start".into(),
                        arguments: serde_json::json!({
                            "port":browser_port,
                            "incognito":true,
                            "chromePath":std::env::var("GLASS_CHROME_PATH").unwrap()
                        }),
                    });
                    browser_start.expected_generation = Some(1);
                    browser_start.expected_project_revision = Some(1);
                    browser_start.allow_mutation = true;
                    browser_start.confirmed = true;
                    let response = request(&socket, &browser_start).await.unwrap();
                    assert!(response.ok, "{:?}", response.error);
                }
                // The Pi-style broker opens a new process/socket and still
                // executes against the same daemon-owned kernel.
                let call = ToolCall {
                    id: "kernel-execute".into(),
                    name: "glass.eval.execute".into(),
                    arguments: serde_json::json!({"name":"durable","code":"SELECT 42 AS answer"}),
                };
                let call_path = std::env::temp_dir().join(format!(
                    "glass-pi-call-{}-{sequence}.json",
                    std::process::id()
                ));
                write_private(&call_path, &serde_json::to_vec(&call).unwrap()).unwrap();
                let result = forward_resident_tool_file_with_context(
                    &socket, &token, "fixture", &call_path, &project, true, true,
                )
                .await
                .unwrap();
                assert_eq!(result["value"][0]["answer"], 42);
                // A fresh socket observes the process and optional Pi worker
                // created by earlier, disconnected clients.
                let inspect = request(
                    &socket,
                    &base_request("inspect-reconnected", "workspace.inspect"),
                )
                .await
                .unwrap();
                assert!(inspect.ok);
                assert!(
                    inspect.result["processes"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .any(|process| process["name"] == "durable-process")
                );
                assert!(
                    inspect.result["kernels"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .any(|kernel| kernel["name"] == "durable")
                );
                if pi_available {
                    assert!(!inspect.result["agents"].as_array().unwrap().is_empty());
                }
                assert_eq!(inspect.result["browser"]["connected"], browser_e2e);
                let mut process_stop = base_request("process-stop", "workspace.tool");
                process_stop.call = Some(ToolCall {
                    id: "process-stop".into(),
                    name: "glass.process.stop".into(),
                    arguments: serde_json::json!({"name":"durable-process"}),
                });
                process_stop.expected_generation = Some(1);
                process_stop.expected_project_revision = Some(1);
                process_stop.allow_mutation = true;
                process_stop.confirmed = true;
                assert!(request(&socket, &process_stop).await.unwrap().ok);
                if browser_e2e {
                    let mut browser_stop = base_request("browser-stop", "workspace.tool");
                    browser_stop.call = Some(ToolCall {
                        id: "browser-stop".into(),
                        name: "glass.browser.stop".into(),
                        arguments: serde_json::json!({}),
                    });
                    browser_stop.expected_generation = Some(1);
                    browser_stop.expected_project_revision = Some(2);
                    browser_stop.allow_mutation = true;
                    browser_stop.confirmed = true;
                    assert!(request(&socket, &browser_stop).await.unwrap().ok);
                }
                server.abort();
            })
            .await;
        let _ = std::fs::remove_dir_all(base);
    }

    #[tokio::test(flavor = "current_thread")]
    #[cfg(unix)]
    async fn daemon_reconnect_preserves_untrusted_state_and_cannot_elevate_it() {
        let sequence = NEXT_ROOT.fetch_add(1, Ordering::Relaxed);
        let base = std::env::temp_dir().join(format!(
            "glassd-trust-test-{}-{sequence}",
            std::process::id()
        ));
        let project = base.join("project");
        std::fs::create_dir_all(&project).unwrap();
        std::fs::write(
            project.join("glass.toml"),
            "[tools.hostile]\ndescription='hostile'\ncommand='echo unsafe'\n",
        )
        .unwrap();
        let socket = base.join("glassd.sock");
        let status = base.join("glassd.json");
        let local = tokio::task::LocalSet::new();
        local
            .run_until(async {
                let server_socket = socket.clone();
                let server_status = status.clone();
                let store = WorkspaceTrustStore::at(base.join("trust.json"));
                let server = tokio::task::spawn_local(async move {
                    serve_with_store(&server_socket, &server_status, store)
                        .await
                        .unwrap()
                });
                for _ in 0..100 {
                    if status.exists() && socket.exists() {
                        break;
                    }
                    tokio::time::sleep(Duration::from_millis(10)).await;
                }
                let token = std::fs::read_to_string(token_path(&status)).unwrap();
                let base_request = |id: &str, operation: &str| DevelopmentDaemonRequest {
                    id: id.into(),
                    token: token.clone(),
                    operation: operation.into(),
                    workspace_id: Some("untrusted".into()),
                    root: None,
                    call: None,
                    expected_generation: None,
                    expected_project_revision: None,
                    allow_mutation: false,
                    confirmed: false,
                    actor: Some("external-agent".into()),
                };
                let mut open = base_request("open", "workspace.open");
                open.root = Some(project.clone());
                let opened = request(&socket, &open).await.unwrap();
                assert!(opened.ok);
                assert_eq!(opened.result["trust"], "untrusted");

                // Each request creates a fresh client connection. Reconnecting
                // observes the same state but has no trust-mutation operation.
                let inspected = request(&socket, &base_request("inspect", "workspace.inspect"))
                    .await
                    .unwrap();
                assert!(inspected.ok);
                assert_eq!(inspected.result["workspace"]["trust"], "untrusted");

                let mut tool = base_request("tool", "workspace.tool");
                tool.call = Some(ToolCall {
                    id: "hostile".into(),
                    name: "glass.custom.hostile".into(),
                    arguments: serde_json::json!({}),
                });
                tool.expected_generation = Some(1);
                tool.expected_project_revision = Some(0);
                tool.allow_mutation = true;
                tool.confirmed = true;
                let denied = request(&socket, &tool).await.unwrap();
                assert!(!denied.ok);
                assert!(denied.error.unwrap().contains("trust"));
                server.abort();
            })
            .await;
        let _ = std::fs::remove_dir_all(base);
    }

    #[test]
    fn daemon_identifiers_and_status_quotas_fail_closed() {
        assert!(validate_identifier("workspace-1", "workspace").is_ok());
        assert!(validate_identifier("../escape", "workspace").is_err());
    }
}
