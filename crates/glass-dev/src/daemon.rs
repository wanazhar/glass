//! Local authenticated daemon that owns complete development workspaces.

use crate::development::{Actor, ToolAuthorization, ToolCall};
use crate::{
    DevelopmentToolContext, DevelopmentWorkspace, ResidentAgentBroker, WorkspaceTrustStore,
};
use glass_browser::cli::args::DaemonCommand;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::cell::RefCell;
use std::collections::{BTreeMap, VecDeque};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncWrite, AsyncWriteExt, BufReader};

const PROTOCOL_VERSION: u32 = 1;
const MAX_REQUEST_BYTES: usize = 1024 * 1024;
const MAX_RESPONSE_BYTES: usize = 1024 * 1024;
const MAX_WORKSPACES: usize = 8;
const MAX_CLIENTS: usize = 16;
const WORKSPACE_COMMAND_CAPACITY: usize = 64;
const WORKSPACE_EVENT_CAPACITY: usize = 512;
const MAX_EVENT_BATCH: usize = 256;
const DAEMON_REQUEST_TIMEOUT: Duration = Duration::from_secs(10);
const OPERATION_CAPACITY: usize = 128;
const OPERATION_EVENT_CAPACITY: usize = 512;

type WorkspaceRegistry = Rc<RefCell<BTreeMap<String, WorkspaceActorHandle>>>;
type QueuedOperation = (String, ToolCall, Box<DevelopmentToolContext>);
type OperationExecution = (
    DevelopmentWorkspace,
    String,
    String,
    u64,
    Result<Value, String>,
);
type OperationCompletion = Result<OperationExecution, tokio::task::JoinError>;

#[derive(Clone)]
struct WorkspaceActorHandle {
    sender: tokio::sync::mpsc::Sender<WorkspaceCommand>,
    summary: Value,
    operations: Arc<Mutex<OperationRegistry>>,
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
    SubmitOperation {
        operation_id: String,
        call: ToolCall,
        context: Box<DevelopmentToolContext>,
    },
    Events {
        since: u64,
        limit: usize,
        response: tokio::sync::oneshot::Sender<Value>,
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
    #[serde(default)]
    pub since: Option<u64>,
    #[serde(default)]
    pub limit: Option<usize>,
    pub operation_id: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DevelopmentOperationState {
    Queued,
    Running,
    Succeeded,
    Failed,
    Cancelled,
    Indeterminate,
}

impl DevelopmentOperationState {
    fn terminal(self) -> bool {
        matches!(
            self,
            Self::Succeeded | Self::Failed | Self::Cancelled | Self::Indeterminate
        )
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DevelopmentOperation {
    pub id: String,
    pub workspace_id: String,
    pub actor: String,
    pub operation_type: String,
    pub state: DevelopmentOperationState,
    pub submitted_at_ms: u128,
    pub started_at_ms: Option<u128>,
    pub completed_at_ms: Option<u128>,
    pub revision_before: u64,
    pub revision_after: Option<u64>,
    pub result_ref: Option<String>,
    pub result: Option<Value>,
    pub failure_reason: Option<String>,
    pub retry_safe: bool,
    pub indeterminate: bool,
    pub cancellation_requested: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DevelopmentOperationEvent {
    pub sequence: u64,
    pub operation_id: String,
    pub timestamp_ms: u128,
    pub state: DevelopmentOperationState,
    pub message: String,
}

struct OperationRegistry {
    workspace_id: String,
    records: BTreeMap<String, DevelopmentOperation>,
    order: VecDeque<String>,
    idempotency: BTreeMap<String, String>,
    events: VecDeque<DevelopmentOperationEvent>,
    next_operation: u64,
    next_event: u64,
}

impl OperationRegistry {
    fn new(workspace_id: String) -> Self {
        Self {
            workspace_id,
            records: BTreeMap::new(),
            order: VecDeque::new(),
            idempotency: BTreeMap::new(),
            events: VecDeque::new(),
            next_operation: 1,
            next_event: 1,
        }
    }

    fn submit(
        &mut self,
        request_id: &str,
        actor: String,
        operation_type: String,
        revision_before: u64,
        retry_safe: bool,
    ) -> Result<(DevelopmentOperation, bool), String> {
        if let Some(operation_id) = self.idempotency.get(request_id) {
            return self
                .records
                .get(operation_id)
                .cloned()
                .map(|record| (record, false))
                .ok_or_else(|| "operation idempotency record is inconsistent".into());
        }
        self.prune_terminal();
        if self.records.len() >= OPERATION_CAPACITY {
            return Err("workspace operation retention limit reached".into());
        }
        let id = format!("{}-op-{:06}", self.workspace_id, self.next_operation);
        self.next_operation = self.next_operation.saturating_add(1);
        let record = DevelopmentOperation {
            id: id.clone(),
            workspace_id: self.workspace_id.clone(),
            actor,
            operation_type,
            state: DevelopmentOperationState::Queued,
            submitted_at_ms: now_ms(),
            started_at_ms: None,
            completed_at_ms: None,
            revision_before,
            revision_after: None,
            result_ref: None,
            result: None,
            failure_reason: None,
            retry_safe,
            indeterminate: false,
            cancellation_requested: false,
        };
        self.records.insert(id.clone(), record.clone());
        self.order.push_back(id.clone());
        self.idempotency.insert(request_id.into(), id.clone());
        self.event(&id, DevelopmentOperationState::Queued, "operation accepted");
        Ok((record, true))
    }

    fn prune_terminal(&mut self) {
        while self.records.len() >= OPERATION_CAPACITY {
            let Some(id) = self.order.iter().find_map(|id| {
                self.records
                    .get(id)
                    .is_some_and(|record| record.state.terminal())
                    .then(|| id.clone())
            }) else {
                break;
            };
            self.order.retain(|operation_id| operation_id != &id);
            self.records.remove(&id);
            self.idempotency
                .retain(|_, operation_id| operation_id != &id);
        }
    }

    fn event(&mut self, id: &str, state: DevelopmentOperationState, message: &str) {
        if self.events.len() == OPERATION_EVENT_CAPACITY {
            self.events.pop_front();
        }
        self.events.push_back(DevelopmentOperationEvent {
            sequence: self.next_event,
            operation_id: id.into(),
            timestamp_ms: now_ms(),
            state,
            message: message.into(),
        });
        self.next_event = self.next_event.saturating_add(1);
    }

    fn start(&mut self, id: &str) -> bool {
        let Some(record) = self.records.get_mut(id) else {
            return false;
        };
        if record.cancellation_requested {
            record.state = DevelopmentOperationState::Cancelled;
            record.completed_at_ms = Some(now_ms());
            self.event(
                id,
                DevelopmentOperationState::Cancelled,
                "cancelled before start",
            );
            return false;
        }
        record.state = DevelopmentOperationState::Running;
        record.started_at_ms = Some(now_ms());
        self.event(id, DevelopmentOperationState::Running, "operation started");
        true
    }

    fn finish(&mut self, id: &str, revision_after: u64, result: Result<Value, String>) {
        let Some(record) = self.records.get_mut(id) else {
            return;
        };
        record.revision_after = Some(revision_after);
        record.completed_at_ms = Some(now_ms());
        let (state, message) = if record.cancellation_requested {
            record.state = DevelopmentOperationState::Cancelled;
            record.result = None;
            record.failure_reason =
                Some("operation completed after cancellation was requested".into());
            (DevelopmentOperationState::Cancelled, "operation cancelled")
        } else {
            match result {
                Ok(value) => {
                    record.state = DevelopmentOperationState::Succeeded;
                    record.result_ref =
                        Some(format!("operation://{}/{}/result", self.workspace_id, id));
                    record.result = Some(value);
                    (DevelopmentOperationState::Succeeded, "operation succeeded")
                }
                Err(error) => {
                    record.state = DevelopmentOperationState::Failed;
                    record.failure_reason = Some(error);
                    (DevelopmentOperationState::Failed, "operation failed")
                }
            }
        };
        self.event(id, state, message);
    }

    fn indeterminate(&mut self, id: &str, error: String) {
        if let Some(record) = self.records.get_mut(id) {
            record.state = DevelopmentOperationState::Indeterminate;
            record.indeterminate = true;
            record.failure_reason = Some(error);
            record.completed_at_ms = Some(now_ms());
        }
        self.event(
            id,
            DevelopmentOperationState::Indeterminate,
            "operation outcome is indeterminate",
        );
    }

    fn cancel(&mut self, id: &str) -> Result<DevelopmentOperation, String> {
        let (state, record) = {
            let record = self
                .records
                .get_mut(id)
                .ok_or_else(|| "unknown workspace operation".to_string())?;
            if !record.state.terminal() {
                record.cancellation_requested = true;
                if record.state == DevelopmentOperationState::Queued {
                    record.state = DevelopmentOperationState::Cancelled;
                    record.completed_at_ms = Some(now_ms());
                }
            }
            (record.state, record.clone())
        };
        self.event(id, state, "cancellation requested");
        Ok(record)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DevelopmentDaemonEvent {
    pub sequence: u64,
    pub timestamp_ms: u128,
    pub kind: String,
    pub workspace_id: String,
    pub tool_call_id: Option<String>,
    pub success: bool,
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
                let operations = Arc::new(Mutex::new(OperationRegistry::new(workspace_id.into())));
                tokio::task::spawn_local(run_workspace_actor(
                    workspace_id.into(),
                    workspace,
                    receiver,
                    Arc::clone(&operations),
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
                        operations,
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
                if request.allow_mutation || long_operation_tool(&call.name) {
                    return Err(
                        "mutating or long-running tools require operation.submit so clients can reconnect and reconcile"
                            .into(),
                    );
                }
                let context = request_tool_context(&request, "workspace.tool")?;
                workspace_tool(
                    workspace_handle(workspaces, workspace_id)?,
                    call.clone(),
                    context,
                )
                .await
            }
            "operation.submit" => {
                let workspace_id = required_workspace_id(&request)?;
                let call = request
                    .call
                    .as_ref()
                    .ok_or("operation.submit requires call")?;
                let context = request_tool_context(&request, "operation.submit")?;
                let handle = workspace_handle(workspaces, workspace_id)?;
                let actor = request.actor.clone().unwrap_or_else(|| "local".into());
                let (operation, created) = handle
                    .operations
                    .lock()
                    .map_err(|_| "workspace operation registry is poisoned")?
                    .submit(
                        &request.id,
                        actor,
                        call.name.clone(),
                        context.expected_project_revision,
                        !request.allow_mutation,
                    )?;
                if created
                    && handle
                        .sender
                        .try_send(WorkspaceCommand::SubmitOperation {
                            operation_id: operation.id.clone(),
                            call: call.clone(),
                            context: Box::new(context),
                        })
                        .is_err()
                {
                    if let Ok(mut registry) = handle.operations.lock() {
                        registry.indeterminate(
                            &operation.id,
                            "workspace actor command queue closed".into(),
                        );
                    }
                    return Err("workspace actor command queue closed".into());
                }
                Ok(serde_json::json!({"accepted":true,"created":created,"operation":operation}))
            }
            "operation.inspect" | "operation.reconcile" => {
                let workspace_id = required_workspace_id(&request)?;
                let operation_id = required_operation_id(&request)?;
                let handle = workspace_handle(workspaces, workspace_id)?;
                let operation = handle
                    .operations
                    .lock()
                    .map_err(|_| "workspace operation registry is poisoned")?
                    .records
                    .get(operation_id)
                    .cloned()
                    .ok_or("unknown workspace operation")?;
                let reconciled = operation.state.terminal();
                let retry_safe = matches!(
                    operation.state,
                    DevelopmentOperationState::Failed
                        | DevelopmentOperationState::Cancelled
                        | DevelopmentOperationState::Indeterminate
                ) && operation.retry_safe;
                Ok(serde_json::json!({
                    "operation":operation,
                    "reconciled":reconciled,
                    "retrySafe":retry_safe,
                }))
            }
            "operation.list" => {
                let workspace_id = required_workspace_id(&request)?;
                let handle = workspace_handle(workspaces, workspace_id)?;
                let registry = handle
                    .operations
                    .lock()
                    .map_err(|_| "workspace operation registry is poisoned")?;
                let operations = registry
                    .order
                    .iter()
                    .filter_map(|id| registry.records.get(id).cloned())
                    .collect::<Vec<_>>();
                Ok(serde_json::json!({"operations":operations,"capacity":OPERATION_CAPACITY}))
            }
            "operation.cancel" => {
                let workspace_id = required_workspace_id(&request)?;
                let operation_id = required_operation_id(&request)?;
                let handle = workspace_handle(workspaces, workspace_id)?;
                let operation = handle
                    .operations
                    .lock()
                    .map_err(|_| "workspace operation registry is poisoned")?
                    .cancel(operation_id)?;
                Ok(serde_json::json!({"operation":operation}))
            }
            "operation.events" => {
                let workspace_id = required_workspace_id(&request)?;
                let operation_id = request.operation_id.as_deref();
                if let Some(operation_id) = operation_id {
                    validate_identifier(operation_id, "operation")?;
                }
                let limit = request.limit.unwrap_or(128);
                if limit == 0 || limit > MAX_EVENT_BATCH {
                    return Err(format!(
                        "operation event limit must be 1..={MAX_EVENT_BATCH}"
                    ));
                }
                let handle = workspace_handle(workspaces, workspace_id)?;
                let registry = handle
                    .operations
                    .lock()
                    .map_err(|_| "workspace operation registry is poisoned")?;
                let since = request.since.unwrap_or(0);
                let events = registry
                    .events
                    .iter()
                    .filter(|event| event.sequence > since)
                    .filter(|event| operation_id.is_none_or(|id| event.operation_id == id))
                    .take(limit)
                    .cloned()
                    .collect::<Vec<_>>();
                Ok(serde_json::json!({
                    "events":events,
                    "newestSequence":registry.events.back().map_or(0, |event| event.sequence),
                    "capacity":OPERATION_EVENT_CAPACITY,
                }))
            }
            "workspace.events" => {
                let workspace_id = required_workspace_id(&request)?;
                workspace_events(
                    workspace_handle(workspaces, workspace_id)?,
                    request.since.unwrap_or(0),
                    request.limit.unwrap_or(128),
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

async fn workspace_events(
    handle: WorkspaceActorHandle,
    since: u64,
    limit: usize,
) -> Result<Value, String> {
    if limit == 0 || limit > MAX_EVENT_BATCH {
        return Err(format!(
            "workspace event limit must be 1..={MAX_EVENT_BATCH}"
        ));
    }
    let (response, received) = tokio::sync::oneshot::channel();
    handle
        .sender
        .send(WorkspaceCommand::Events {
            since,
            limit,
            response,
        })
        .await
        .map_err(|_| "workspace actor command queue closed")?;
    received
        .await
        .map_err(|_| "workspace actor dropped its event response".into())
}

async fn run_workspace_actor(
    workspace_id: String,
    workspace: DevelopmentWorkspace,
    mut commands: tokio::sync::mpsc::Receiver<WorkspaceCommand>,
    operations: Arc<Mutex<OperationRegistry>>,
) {
    let mut workspace = Some(workspace);
    let mut queued_operations: VecDeque<QueuedOperation> = VecDeque::new();
    let (completion_tx, mut completions) = tokio::sync::mpsc::channel::<OperationCompletion>(1);
    let mut operation_running = false;
    let mut command_channel_open = true;
    let mut closing: Option<tokio::sync::oneshot::Sender<()>> = None;
    let mut events = VecDeque::with_capacity(WORKSPACE_EVENT_CAPACITY);
    let mut next_event = 1_u64;
    let mut dropped_events = 0_u64;
    push_workspace_event(
        &workspace_id,
        &mut events,
        &mut next_event,
        &mut dropped_events,
        "workspace.opened",
        None,
        true,
    );
    let mut tick = tokio::time::interval(Duration::from_millis(50));
    tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    loop {
        if closing.is_none() && !operation_running && workspace.is_some() {
            while let Some((operation_id, call, context)) = queued_operations.pop_front() {
                let should_start = operations
                    .lock()
                    .map(|mut registry| registry.start(&operation_id))
                    .unwrap_or(false);
                if !should_start {
                    continue;
                }
                let mut owned_workspace = workspace
                    .take()
                    .expect("workspace operation start owns the workspace");
                let completion = completion_tx.clone();
                operation_running = true;
                tokio::task::spawn_local(async move {
                    let joined = tokio::task::spawn_blocking(move || {
                        let result = owned_workspace
                            .execute_tool(&call, &context)
                            .map_err(|error| error.to_string());
                        let revision_after = owned_workspace.project().revision();
                        (
                            owned_workspace,
                            operation_id,
                            call.id,
                            revision_after,
                            result,
                        )
                    })
                    .await;
                    let _ = completion.send(joined).await;
                });
                break;
            }
        }
        if closing.is_some() && !operation_running {
            if let Some(response) = closing.take() {
                let _ = response.send(());
            }
            break;
        }
        tokio::select! {
            biased;
            completion = completions.recv(), if operation_running => {
                operation_running = false;
                match completion {
                    Some(Ok((returned_workspace, operation_id, call_id, revision_after, result))) => {
                        workspace = Some(returned_workspace);
                        if let Ok(mut registry) = operations.lock() {
                            registry.finish(&operation_id, revision_after, result.clone());
                        }
                        push_workspace_event(
                            &workspace_id,
                            &mut events,
                            &mut next_event,
                            &mut dropped_events,
                            "operation.completed",
                            Some(call_id),
                            result.is_ok(),
                        );
                    }
                    Some(Err(error)) => {
                        let running_id = operations.lock().ok().and_then(|registry| {
                            registry.records.values().find(|record| {
                                record.state == DevelopmentOperationState::Running
                            }).map(|record| record.id.clone())
                        });
                        if let Some(operation_id) = running_id
                            && let Ok(mut registry) = operations.lock()
                        {
                            registry.indeterminate(
                                &operation_id,
                                format!("workspace operation worker failed: {error}"),
                            );
                        }
                        tracing::error!(workspace_id, %error, "workspace operation worker failed");
                        if let Some(response) = closing.take() {
                            let _ = response.send(());
                        }
                        break;
                    }
                    None => break,
                }
            }
            command = commands.recv(), if command_channel_open => match command {
                Some(WorkspaceCommand::Inspect { response }) => {
                    let result = workspace.as_mut().map_or_else(
                        || Err("workspace is busy with a recoverable operation; inspect the operation ID".into()),
                        |workspace| inspect_workspace(&workspace_id, workspace),
                    );
                    let _ = response.send(result);
                }
                Some(WorkspaceCommand::Tool { call, context, response }) => {
                    let Some(mut owned_workspace) = workspace.take() else {
                        let _ = response.send(Err(
                            "workspace is busy; submit long work with operation.submit and inspect its ID".into(),
                        ));
                        continue;
                    };
                    let kind = daemon_event_kind(&call.name);
                    let execution = tokio::task::spawn_blocking(move || {
                        let result = owned_workspace
                            .execute_tool(&call, &context)
                            .map_err(|error| error.to_string());
                        (owned_workspace, call.id, result)
                    })
                    .await;
                    let (returned_workspace, call_id, result) = match execution {
                        Ok(execution) => execution,
                        Err(error) => {
                            let _ = response.send(Err(format!(
                                "workspace actor tool execution failed: {error}"
                            )));
                            break;
                        }
                    };
                    workspace = Some(returned_workspace);
                    push_workspace_event(
                        &workspace_id,
                        &mut events,
                        &mut next_event,
                        &mut dropped_events,
                        kind,
                        Some(call_id),
                        result.is_ok(),
                    );
                    let _ = response.send(result);
                }
                Some(WorkspaceCommand::SubmitOperation { operation_id, call, context }) => {
                    queued_operations.push_back((operation_id, call, context));
                }
                Some(WorkspaceCommand::Events { since, limit, response }) => {
                    let _ = response.send(workspace_event_batch(
                        &events,
                        next_event,
                        dropped_events,
                        since,
                        limit,
                    ));
                }
                Some(WorkspaceCommand::Shutdown { response }) => {
                    if let Ok(mut registry) = operations.lock() {
                        let active = registry
                            .records
                            .values()
                            .filter(|record| !record.state.terminal())
                            .map(|record| record.id.clone())
                            .collect::<Vec<_>>();
                        for id in active {
                            let _ = registry.cancel(&id);
                        }
                    }
                    queued_operations.clear();
                    closing = Some(response);
                }
                None => {
                    command_channel_open = false;
                    if let Ok(mut registry) = operations.lock() {
                        let active = registry
                            .records
                            .values()
                            .filter(|record| !record.state.terminal())
                            .map(|record| record.id.clone())
                            .collect::<Vec<_>>();
                        for id in active {
                            let _ = registry.cancel(&id);
                        }
                    }
                    queued_operations.clear();
                    if !operation_running {
                        break;
                    }
                },
            },
            _ = tick.tick() => {
                if let Some(workspace) = workspace.as_mut()
                    && let Err(error) = workspace.tasks()
                {
                    tracing::warn!(workspace_id, %error, "workspace task scheduler tick failed");
                }
            }
        }
    }
}

fn workspace_event_batch(
    events: &VecDeque<DevelopmentDaemonEvent>,
    next_event: u64,
    dropped_events: u64,
    since: u64,
    limit: usize,
) -> Value {
    let oldest = events
        .front()
        .map(|event| event.sequence)
        .unwrap_or(next_event);
    let newest = events.back().map(|event| event.sequence).unwrap_or(0);
    let selected = events
        .iter()
        .filter(|event| event.sequence > since)
        .take(limit)
        .cloned()
        .collect::<Vec<_>>();
    serde_json::json!({
        "events": selected,
        "oldestSequence": oldest,
        "newestSequence": newest,
        "lostBefore": (since.saturating_add(1) < oldest).then_some(oldest),
        "droppedEvents": dropped_events,
        "capacity": WORKSPACE_EVENT_CAPACITY,
    })
}

fn push_workspace_event(
    workspace_id: &str,
    events: &mut VecDeque<DevelopmentDaemonEvent>,
    next_event: &mut u64,
    dropped_events: &mut u64,
    kind: &str,
    tool_call_id: Option<String>,
    success: bool,
) {
    if events.len() == WORKSPACE_EVENT_CAPACITY {
        events.pop_front();
        *dropped_events = dropped_events.saturating_add(1);
    }
    events.push_back(DevelopmentDaemonEvent {
        sequence: *next_event,
        timestamp_ms: now_ms(),
        kind: kind.into(),
        workspace_id: workspace_id.into(),
        tool_call_id,
        success,
    });
    *next_event = next_event.saturating_add(1);
}

fn daemon_event_kind(tool: &str) -> &'static str {
    if tool.starts_with("glass.agent.") {
        "agent.event"
    } else if tool.starts_with("glass.task.") {
        "task.changed"
    } else if tool.starts_with("glass.process.") {
        "process.output"
    } else if tool.starts_with("glass.browser.") || tool.starts_with("glass.web.") {
        "browser.revision"
    } else if tool.starts_with("glass.lsp.") {
        "lsp.diagnostics"
    } else if tool.starts_with("glass.debug.") {
        "debugger.stopped"
    } else if tool.starts_with("glass.test.") || tool.starts_with("glass.workflow.") {
        "test.completed"
    } else if tool.starts_with("glass.git.") {
        "git.changed"
    } else if tool.starts_with("glass.experiment.") {
        "experiment.changed"
    } else {
        "workspace.changed"
    }
}

fn long_operation_tool(name: &str) -> bool {
    [
        "glass.process.",
        "glass.test.",
        "glass.eval.",
        "glass.debug.",
        "glass.git.",
        "glass.workflow.",
        "glass.experiment.",
        "glass.agent.",
        "glass.task.",
        "glass.browser.start",
        "glass.browser.stop",
        "glass.browser.reconnect",
    ]
    .iter()
    .any(|prefix| name.starts_with(prefix))
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
    bounded_request_stream(stream, request).await
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
    bounded_request_stream(stream, request).await
}

async fn bounded_request_stream<S>(
    stream: S,
    request: &DevelopmentDaemonRequest,
) -> Result<DevelopmentDaemonResponse, Box<dyn std::error::Error>>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    tokio::time::timeout(DAEMON_REQUEST_TIMEOUT, request_stream(stream, request))
        .await
        .map_err(|_| {
            format!(
                "daemon operation {} timed out after {} seconds",
                request.operation,
                DAEMON_REQUEST_TIMEOUT.as_secs()
            )
        })?
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
            since: None,
            limit: None,
            operation_id: None,
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
    let mut response = request(
        socket,
        &DevelopmentDaemonRequest {
            id: format!("tool-{}", call.id),
            token: token.to_string(),
            operation: "operation.submit".into(),
            workspace_id: Some(workspace_id.to_string()),
            root: None,
            call: Some(call.clone()),
            expected_generation: Some(generation),
            expected_project_revision: Some(revision),
            allow_mutation,
            confirmed,
            actor: Some("pi".into()),
            since: None,
            limit: None,
            operation_id: None,
        },
    )
    .await?;
    if !response.ok {
        return Err(response
            .error
            .unwrap_or_else(|| "resident operation submission failed".into())
            .into());
    }
    let operation_id = response
        .result
        .pointer("/operation/id")
        .and_then(Value::as_str)
        .ok_or("daemon operation submission returned no operation ID")?
        .to_string();
    loop {
        let state = response
            .result
            .pointer("/operation/state")
            .and_then(Value::as_str);
        if state == Some("succeeded") {
            return response
                .result
                .pointer("/operation/result")
                .cloned()
                .ok_or_else(|| "successful daemon operation returned no result".into());
        }
        if matches!(state, Some("failed" | "cancelled" | "indeterminate")) {
            let reason = response
                .result
                .pointer("/operation/failureReason")
                .and_then(Value::as_str)
                .unwrap_or("resident operation did not succeed");
            return Err(reason.into());
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
        response = request(
            socket,
            &DevelopmentDaemonRequest {
                id: format!("inspect-operation-{}", call.id),
                token: token.to_string(),
                operation: "operation.inspect".into(),
                workspace_id: Some(workspace_id.to_string()),
                root: None,
                call: None,
                expected_generation: None,
                expected_project_revision: None,
                allow_mutation: false,
                confirmed: false,
                actor: Some("pi".into()),
                since: None,
                limit: None,
                operation_id: Some(operation_id.clone()),
            },
        )
        .await?;
        if !response.ok {
            return Err(response
                .error
                .unwrap_or_else(|| "resident operation inspection failed".into())
                .into());
        }
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

fn required_operation_id(request: &DevelopmentDaemonRequest) -> Result<&str, String> {
    let id = request
        .operation_id
        .as_deref()
        .ok_or("daemon request requires operationId")?;
    validate_identifier(id, "operation")?;
    Ok(id)
}

fn request_tool_context(
    request: &DevelopmentDaemonRequest,
    operation: &str,
) -> Result<DevelopmentToolContext, String> {
    let actor = request
        .actor
        .as_deref()
        .map(Actor::external)
        .unwrap_or_else(Actor::local);
    Ok(DevelopmentToolContext {
        authorization: ToolAuthorization {
            actor,
            allow_mutation: request.allow_mutation,
            confirmed: request.confirmed,
        },
        initiator: None,
        expected_generation: request
            .expected_generation
            .ok_or_else(|| format!("{operation} requires expectedGeneration"))?,
        expected_project_revision: request
            .expected_project_revision
            .ok_or_else(|| format!("{operation} requires expectedProjectRevision"))?,
    })
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

pub fn log_tail(status_path: Option<&Path>) -> Result<String, Box<dyn std::error::Error>> {
    let (_, default_status) = default_paths();
    bounded_log(status_path.unwrap_or(&default_status))
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
fn authorize_local_peer(
    _stream: &tokio::net::UnixStream,
) -> Result<(), Box<dyn std::error::Error>> {
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
                _stream.as_raw_fd(),
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
            initiator: None,
            expected_generation: 1,
            expected_project_revision: 0,
        }
    }

    async fn wait_for_operation_state(
        operations: &Arc<Mutex<OperationRegistry>>,
        operation_id: &str,
        expected: DevelopmentOperationState,
    ) {
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        loop {
            let state = operations.lock().unwrap().records[operation_id].state;
            if state == expected {
                return;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "operation {operation_id} remained {state:?}, expected {expected:?}; failure_reason={:?}",
                operations.lock().unwrap().records[operation_id].failure_reason
            );
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }

    async fn submit_and_wait(
        socket: &Path,
        request_value: &DevelopmentDaemonRequest,
    ) -> DevelopmentDaemonResponse {
        let mut submission = request_value.clone();
        submission.operation = "operation.submit".into();
        let accepted = request(socket, &submission).await.unwrap();
        if !accepted.ok {
            return accepted;
        }
        let operation_id = accepted.result["operation"]["id"]
            .as_str()
            .unwrap()
            .to_string();
        for sequence in 0..2_000 {
            let mut inspect = submission.clone();
            inspect.id = format!("inspect-{}-{sequence}", submission.id);
            inspect.operation = "operation.inspect".into();
            inspect.call = None;
            inspect.expected_generation = None;
            inspect.expected_project_revision = None;
            inspect.allow_mutation = false;
            inspect.confirmed = false;
            inspect.operation_id = Some(operation_id.clone());
            let response = request(socket, &inspect).await.unwrap();
            if !response.ok {
                return response;
            }
            match response.result["operation"]["state"].as_str() {
                Some("succeeded") => {
                    return DevelopmentDaemonResponse {
                        id: submission.id,
                        ok: true,
                        result: response.result["operation"]["result"].clone(),
                        error: None,
                    };
                }
                Some("failed" | "cancelled" | "indeterminate") => {
                    return DevelopmentDaemonResponse {
                        id: submission.id,
                        ok: false,
                        result: Value::Null,
                        error: response.result["operation"]["failureReason"]
                            .as_str()
                            .map(str::to_string)
                            .or_else(|| Some("operation did not succeed".into())),
                    };
                }
                _ => tokio::time::sleep(Duration::from_millis(10)).await,
            }
        }
        DevelopmentDaemonResponse {
            id: submission.id,
            ok: false,
            result: Value::Null,
            error: Some("operation did not settle in the test deadline".into()),
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
        for root in [&first_root, &second_root] {
            std::fs::write(
                root.join("Cargo.toml"),
                "[package]\nname='stress-fixture'\nversion='0.1.0'\n",
            )
            .unwrap();
        }
        let dap_adapter = second_root.join("stress_dap.py");
        std::fs::write(
            &dap_adapter,
            r#"import json, sys
while True:
    line = sys.stdin.buffer.readline()
    if not line:
        break
    if not line.lower().startswith(b'content-length:'):
        continue
    length = int(line.split(b':', 1)[1])
    while sys.stdin.buffer.readline() not in (b'\r\n', b'\n', b''):
        pass
    request = json.loads(sys.stdin.buffer.read(length))
    response = {'seq': request['seq'] + 1000, 'type': 'response', 'request_seq': request['seq'], 'command': request['command'], 'success': True, 'body': {}}
    body = json.dumps(response).encode()
    sys.stdout.buffer.write(f'Content-Length: {len(body)}\r\n\r\n'.encode() + body)
    sys.stdout.buffer.flush()
"#,
        )
        .unwrap();
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
                    let operations = Arc::new(Mutex::new(OperationRegistry::new(id.into())));
                    tokio::task::spawn_local(run_workspace_actor(
                        id.into(),
                        workspace,
                        receiver,
                        Arc::clone(&operations),
                    ));
                    WorkspaceActorHandle {
                        sender,
                        summary,
                        operations,
                    }
                };
                let first = actor("first", &first_root);
                let second = actor("second", &second_root);
                let python = ["python3", "python"]
                    .into_iter()
                    .find(|program| {
                        std::process::Command::new(program)
                            .arg("--version")
                            .output()
                            .is_ok_and(|output| output.status.success())
                    })
                    .expect("full-suite stress requires Python for the fixture DAP adapter");
                workspace_tool(
                    second.clone(),
                    ToolCall {
                        id: "debug-start".into(),
                        name: "glass.debug.start".into(),
                        arguments: serde_json::json!({
                            "session":"stress",
                            "command":python,
                            "arguments":[dap_adapter],
                            "timeoutSeconds":3
                        }),
                    },
                    test_context(),
                )
                .await
                .unwrap();
                let (kernel_kind, kernel_code) = if cfg!(windows) {
                    ("python", "import time; time.sleep(3); print('complete')")
                } else {
                    ("shell", "sleep 3; echo complete")
                };
                workspace_tool(
                    first.clone(),
                    ToolCall {
                        id: "shell-start".into(),
                        name: "glass.eval.start".into(),
                        arguments: serde_json::json!({"name":"slow","kind":kernel_kind}),
                    },
                    test_context(),
                )
                .await
                .unwrap();
                let slow = tokio::task::spawn_local(workspace_tool(
                    first.clone(),
                    ToolCall {
                        id: "shell-slow".into(),
                        name: "glass.eval.execute".into(),
                        arguments: serde_json::json!({
                            "name":"slow",
                            "code":kernel_code,
                            "timeoutSeconds":5
                        }),
                    },
                    test_context(),
                ));
                tokio::time::sleep(Duration::from_millis(50)).await;
                let (inspect, browser, lsp, debug, tests, processes) = tokio::join!(
                    workspace_inspect(second.clone()),
                    workspace_tool(
                        second.clone(),
                        ToolCall {
                            id: "browser-state".into(),
                            name: "glass.browser.state".into(),
                            arguments: serde_json::json!({}),
                        },
                        test_context(),
                    ),
                    workspace_tool(
                        second.clone(),
                        ToolCall {
                            id: "lsp-list".into(),
                            name: "glass.lsp.list".into(),
                            arguments: serde_json::json!({}),
                        },
                        test_context(),
                    ),
                    workspace_tool(
                        second.clone(),
                        ToolCall {
                            id: "debug-events".into(),
                            name: "glass.debug.events".into(),
                            arguments: serde_json::json!({"session":"stress"}),
                        },
                        test_context(),
                    ),
                    workspace_tool(
                        second.clone(),
                        ToolCall {
                            id: "test-discover".into(),
                            name: "glass.test.discover".into(),
                            arguments: serde_json::json!({}),
                        },
                        test_context(),
                    ),
                    workspace_tool(
                        second.clone(),
                        ToolCall {
                            id: "process-list".into(),
                            name: "glass.process.list".into(),
                            arguments: serde_json::json!({}),
                        },
                        test_context(),
                    ),
                );
                let inspect = inspect.unwrap();
                assert_eq!(inspect["workspace"]["id"], "second");
                assert!(browser.is_ok());
                assert!(lsp.is_ok());
                assert!(debug.is_ok());
                assert!(tests.is_ok());
                assert!(processes.is_ok());
                assert!(
                    !slow.is_finished(),
                    "unrelated workspace work waited for the slow operation"
                );
                assert!(slow.await.unwrap().is_ok());
                for handle in [first, second] {
                    let (response, received) = tokio::sync::oneshot::channel();
                    handle
                        .sender
                        .send(WorkspaceCommand::Shutdown { response })
                        .await
                        .unwrap();
                    received.await.unwrap();
                }
            })
            .await;
        std::fs::remove_dir_all(base).unwrap();
    }

    #[tokio::test(flavor = "current_thread")]
    async fn long_operations_reconnect_reconcile_and_cancel_by_stable_id() {
        let sequence = NEXT_ROOT.fetch_add(1, Ordering::Relaxed);
        let root = std::env::temp_dir().join(format!(
            "glassd-operation-lifecycle-{}-{sequence}",
            std::process::id()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let store = WorkspaceTrustStore::at(root.join("trust.json"));
        store
            .trust_project(&crate::WorkspaceIdentity::inspect(&root).unwrap())
            .unwrap();
        let local = tokio::task::LocalSet::new();
        local
            .run_until(async {
                let workspace =
                    DevelopmentWorkspace::open_with_store(&root, store.clone()).unwrap();
                let summary = workspace_summary("operations", &workspace);
                let (sender, receiver) = tokio::sync::mpsc::channel(WORKSPACE_COMMAND_CAPACITY);
                let operations = Arc::new(Mutex::new(OperationRegistry::new("operations".into())));
                tokio::task::spawn_local(run_workspace_actor(
                    "operations".into(),
                    workspace,
                    receiver,
                    Arc::clone(&operations),
                ));
                let handle = WorkspaceActorHandle {
                    sender: sender.clone(),
                    summary: summary.clone(),
                    operations: Arc::clone(&operations),
                };
                let (kernel_kind, slow_code) = if cfg!(windows) {
                    ("python", "import time; time.sleep(0.2); print('complete')")
                } else {
                    ("shell", "sleep 0.2; echo complete")
                };
                workspace_tool(
                    handle.clone(),
                    ToolCall {
                        id: "operation-kernel-start".into(),
                        name: "glass.eval.start".into(),
                        arguments: serde_json::json!({"name":"operation","kind":kernel_kind}),
                    },
                    test_context(),
                )
                .await
                .unwrap();

                let (first, created) = operations
                    .lock()
                    .unwrap()
                    .submit(
                        "stable-request",
                        "test-client".into(),
                        "glass.eval.execute".into(),
                        0,
                        true,
                    )
                    .unwrap();
                assert!(created);
                sender
                    .send(WorkspaceCommand::SubmitOperation {
                        operation_id: first.id.clone(),
                        call: ToolCall {
                            id: "operation-slow".into(),
                            name: "glass.eval.execute".into(),
                            arguments: serde_json::json!({
                                "name":"operation",
                                "code":slow_code,
                                "timeoutSeconds":2
                            }),
                        },
                        context: Box::new(test_context()),
                    })
                    .await
                    .unwrap();

                // A fresh handle models a disconnected client recovering the same operation.
                let reconnected = WorkspaceActorHandle {
                    sender: sender.clone(),
                    summary,
                    operations: Arc::clone(&operations),
                };
                let immediate = reconnected.operations.lock().unwrap().records[&first.id].clone();
                assert!(matches!(
                    immediate.state,
                    DevelopmentOperationState::Queued | DevelopmentOperationState::Running
                ));
                let duplicate = operations
                    .lock()
                    .unwrap()
                    .submit(
                        "stable-request",
                        "test-client".into(),
                        "glass.eval.execute".into(),
                        0,
                        true,
                    )
                    .unwrap();
                assert!(!duplicate.1);
                assert_eq!(duplicate.0.id, first.id);
                wait_for_operation_state(
                    &operations,
                    &first.id,
                    DevelopmentOperationState::Succeeded,
                )
                .await;
                let finished = operations.lock().unwrap().records[&first.id].clone();
                assert_eq!(finished.state, DevelopmentOperationState::Succeeded);
                assert!(finished.result.is_some());
                assert_eq!(finished.revision_after, Some(0));

                let (cancelled, _) = operations
                    .lock()
                    .unwrap()
                    .submit(
                        "cancel-request",
                        "test-client".into(),
                        "glass.eval.execute".into(),
                        0,
                        true,
                    )
                    .unwrap();
                sender
                    .send(WorkspaceCommand::SubmitOperation {
                        operation_id: cancelled.id.clone(),
                        call: ToolCall {
                            id: "operation-cancel".into(),
                            name: "glass.eval.execute".into(),
                            arguments: serde_json::json!({
                                "name":"operation",
                                "code":if cfg!(windows) {
                                    "import time; time.sleep(2); print('cancel')"
                                } else {
                                    "sleep 2; echo cancel"
                                },
                                "timeoutSeconds":5
                            }),
                        },
                        context: Box::new(test_context()),
                    })
                    .await
                    .unwrap();
                wait_for_operation_state(
                    &operations,
                    &cancelled.id,
                    DevelopmentOperationState::Running,
                )
                .await;
                let cancelling = operations.lock().unwrap().cancel(&cancelled.id).unwrap();
                assert!(cancelling.cancellation_requested);
                wait_for_operation_state(
                    &operations,
                    &cancelled.id,
                    DevelopmentOperationState::Cancelled,
                )
                .await;
                assert_eq!(
                    operations.lock().unwrap().records[&cancelled.id].state,
                    DevelopmentOperationState::Cancelled
                );
                assert!(
                    operations
                        .lock()
                        .unwrap()
                        .events
                        .iter()
                        .any(|event| event.operation_id == first.id
                            && event.state == DevelopmentOperationState::Succeeded)
                );

                let (response, received) = tokio::sync::oneshot::channel();
                reconnected
                    .sender
                    .send(WorkspaceCommand::Shutdown { response })
                    .await
                    .unwrap();
                received.await.unwrap();
            })
            .await;
        std::fs::remove_dir_all(root).unwrap();
    }

    #[tokio::test(flavor = "current_thread")]
    async fn workspace_event_cursors_survive_client_reconnects() {
        let sequence = NEXT_ROOT.fetch_add(1, Ordering::Relaxed);
        let root = std::env::temp_dir().join(format!(
            "glassd-event-cursor-{}-{sequence}",
            std::process::id()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let local = tokio::task::LocalSet::new();
        local
            .run_until(async {
                let workspace = DevelopmentWorkspace::open(&root).unwrap();
                let summary = workspace_summary("events", &workspace);
                let (sender, receiver) = tokio::sync::mpsc::channel(WORKSPACE_COMMAND_CAPACITY);
                let operations = Arc::new(Mutex::new(OperationRegistry::new("events".into())));
                tokio::task::spawn_local(run_workspace_actor(
                    "events".into(),
                    workspace,
                    receiver,
                    Arc::clone(&operations),
                ));
                let first_client = WorkspaceActorHandle {
                    sender: sender.clone(),
                    summary: summary.clone(),
                    operations: Arc::clone(&operations),
                };
                workspace_tool(
                    first_client,
                    ToolCall {
                        id: "task-list".into(),
                        name: "glass.task.list".into(),
                        arguments: serde_json::json!({}),
                    },
                    test_context(),
                )
                .await
                .unwrap();
                let first = workspace_events(
                    WorkspaceActorHandle {
                        sender: sender.clone(),
                        summary: summary.clone(),
                        operations: Arc::clone(&operations),
                    },
                    0,
                    16,
                )
                .await
                .unwrap();
                assert_eq!(first["events"][0]["kind"], "workspace.opened");
                assert_eq!(first["events"][1]["kind"], "task.changed");
                let cursor = first["newestSequence"].as_u64().unwrap();

                // A fresh handle models a client reconnecting to the same actor.
                let reconnected = WorkspaceActorHandle {
                    sender,
                    summary,
                    operations,
                };
                workspace_tool(
                    reconnected.clone(),
                    ToolCall {
                        id: "task-list-after-reconnect".into(),
                        name: "glass.task.list".into(),
                        arguments: serde_json::json!({}),
                    },
                    test_context(),
                )
                .await
                .unwrap();
                let resumed = workspace_events(reconnected, cursor, 16).await.unwrap();
                assert_eq!(resumed["events"].as_array().unwrap().len(), 1);
                assert_eq!(resumed["events"][0]["kind"], "task.changed");
                assert_eq!(resumed["lostBefore"], Value::Null);
            })
            .await;
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn workspace_event_overflow_is_bounded_and_observable() {
        let mut events = VecDeque::with_capacity(WORKSPACE_EVENT_CAPACITY);
        let mut next = 1;
        let mut dropped = 0;
        for index in 0..=WORKSPACE_EVENT_CAPACITY {
            push_workspace_event(
                "bounded",
                &mut events,
                &mut next,
                &mut dropped,
                "workspace.changed",
                Some(format!("call-{index}")),
                true,
            );
        }
        let batch = workspace_event_batch(&events, next, dropped, 0, MAX_EVENT_BATCH);
        assert_eq!(events.len(), WORKSPACE_EVENT_CAPACITY);
        assert_eq!(batch["droppedEvents"], 1);
        assert_eq!(batch["lostBefore"], 2);
        assert_eq!(batch["events"].as_array().unwrap().len(), MAX_EVENT_BATCH);
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
                    since: None,
                    limit: None,
                    operation_id: None,
                };
                let mut open = base_request("open", "workspace.open");
                open.root = Some(project.clone());
                assert!(request(&socket, &open).await.unwrap().ok);
                let mut start = base_request("start", "operation.submit");
                start.call = Some(ToolCall {
                    id: "kernel-start".into(),
                    name: "glass.eval.start".into(),
                    arguments: serde_json::json!({"name":"durable","kind":"sql"}),
                });
                start.expected_generation = Some(1);
                start.expected_project_revision = Some(0);
                start.allow_mutation = true;
                start.confirmed = true;
                assert!(submit_and_wait(&socket, &start).await.ok);
                let mut process_start = base_request("process-start", "operation.submit");
                process_start.call = Some(ToolCall {
                    id: "process-start".into(),
                    name: "glass.process.start".into(),
                    arguments: serde_json::json!({"name":"durable-process","command":"sleep 30"}),
                });
                process_start.expected_generation = Some(1);
                process_start.expected_project_revision = Some(0);
                process_start.allow_mutation = true;
                process_start.confirmed = true;
                assert!(submit_and_wait(&socket, &process_start).await.ok);
                let pi_available = std::process::Command::new("pi")
                    .arg("--version")
                    .output()
                    .is_ok();
                if pi_available {
                    let mut agent_start = base_request("agent-start", "operation.submit");
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
                    let agent_response = submit_and_wait(&socket, &agent_start).await;
                    assert!(agent_response.ok, "{:?}", agent_response.error);
                }
                let browser_e2e = std::env::var("GLASS_E2E").as_deref() == Ok("1")
                    && std::env::var_os("GLASS_CHROME_PATH").is_some();
                if browser_e2e {
                    let port_probe = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
                    let browser_port = port_probe.local_addr().unwrap().port();
                    drop(port_probe);
                    let mut browser_start = base_request("browser-start", "operation.submit");
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
                    let response = submit_and_wait(&socket, &browser_start).await;
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
                let mut process_stop = base_request("process-stop", "operation.submit");
                process_stop.call = Some(ToolCall {
                    id: "process-stop".into(),
                    name: "glass.process.stop".into(),
                    arguments: serde_json::json!({"name":"durable-process"}),
                });
                process_stop.expected_generation = Some(1);
                process_stop.expected_project_revision = Some(1);
                process_stop.allow_mutation = true;
                process_stop.confirmed = true;
                assert!(submit_and_wait(&socket, &process_stop).await.ok);
                if browser_e2e {
                    let mut browser_stop = base_request("browser-stop", "operation.submit");
                    browser_stop.call = Some(ToolCall {
                        id: "browser-stop".into(),
                        name: "glass.browser.stop".into(),
                        arguments: serde_json::json!({}),
                    });
                    browser_stop.expected_generation = Some(1);
                    browser_stop.expected_project_revision = Some(2);
                    browser_stop.allow_mutation = true;
                    browser_stop.confirmed = true;
                    assert!(submit_and_wait(&socket, &browser_stop).await.ok);
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
                    since: None,
                    limit: None,
                    operation_id: None,
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

                let mut tool = base_request("tool", "operation.submit");
                tool.call = Some(ToolCall {
                    id: "hostile".into(),
                    name: "glass.custom.hostile".into(),
                    arguments: serde_json::json!({}),
                });
                tool.expected_generation = Some(1);
                tool.expected_project_revision = Some(0);
                tool.allow_mutation = true;
                tool.confirmed = true;
                let denied = submit_and_wait(&socket, &tool).await;
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
