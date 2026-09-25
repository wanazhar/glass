//! MCP JSON-RPC 2.0 stdio server.
//!
//! Implements MCP (2024-11-05 and 2025-11-25) over stdin/stdout,
//! providing browser automation tools with policy-gated execution, bounded
//! response sizes, and concurrent request handling.

#[cfg(feature = "native-engine")]
use base64::Engine;
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::{Value, json};
use std::{
    borrow::Cow,
    collections::{BTreeMap, HashMap},
    future::Future,
    io,
    path::Path,
    pin::Pin,
    sync::{
        Arc, Mutex as StdMutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    time::Duration,
};
use tokio::io::{
    AsyncBufRead, AsyncBufReadExt, AsyncReadExt, AsyncWrite, AsyncWriteExt, BufReader,
};
use tokio::sync::{Mutex, Notify, Semaphore, mpsc, oneshot};
use tracing::{debug, info};

use crate::browser::cdp::CdpError;
#[cfg(feature = "native-engine")]
use crate::browser::native_engine::{
    NATIVE_BEFOREUNLOAD_MESSAGE, NativeDialogController, NativeDialogResolution,
    NativeEngineConfig, NativeFile, NativePendingDialog, Viewport,
};
use crate::browser::policy::{BrowserPolicy, PolicyError};
use crate::browser::profile::ProfileManager;
use crate::browser::runtime::BrowserRuntimeSession;
use crate::browser::session::{
    ActionContractError, ActionKind, ActionOutcome, ActionVerificationError, BatchMode, BatchStep,
    BrowserResult, CdpBrowserSession, CheckpointV1, DownloadError, KnowledgeConfidence,
    KnowledgeLookupContext, KnowledgeLookupOptions, KnowledgeObservationMode,
    KnowledgeObservationReport, KnowledgeProfileScope, KnowledgeStore, Locator, PopupClickError,
    PreflightAction, ReconciliationOptions, SemanticIntentExecutionRequest, SemanticIntentRequest,
    SemanticObservationLevel, SessionOptions, SessionSnapshotStore, StructuredExtractionRequest,
    TargetError, VerificationPredicate, VisualCaptureOptions, VisualClip, VisualFormat,
    WaitCondition, WaitTimeout, default_knowledge_store_path, default_session_snapshot_path,
    recover_run,
};
use crate::capabilities::GlassCapabilityManifest;
use crate::cli::args::Cli;
use crate::daemon::{DaemonLeaseContext, LeaseError, MutationLeaseManager};
const MAX_PREFLIGHT_URL_BYTES: usize = 8 * 1024;
use crate::browser_backend::{BackendProfile, BrowserCapability};
use crate::extraction::ExtractionRequest;
use crate::mcp::prompts;
use crate::mcp::resources;
use crate::protocol::{GLASS_PROTOCOL_VERSION, GlassRequest};
use crate::reliability::{ReliabilityReplayBundle, ReliabilityScenario};
use crate::results::{
    ExperienceProvenance, ExperienceResult, ProvenanceSource, ResponseMode,
    default_result_store_path, project_and_store,
};
use crate::surfaces::SurfaceSet;
use crate::task_compiler::TaskCompilationError;
use crate::workspace::{WorkspaceId, WorkspaceStore};

const MCP_PROTOCOL_VERSION: &str = "2024-11-05";
const MCP_PROTOCOL_VERSION_ELICITATION: &str = "2025-11-25";
const MAX_HEADER_BYTES: usize = 8 * 1024;
const MAX_MESSAGE_BYTES: usize = 4 * 1024 * 1024;
const MAX_RESPONSE_BYTES: usize = 32 * 1024 * 1024;
const MAX_ERROR_DETAILS_BYTES: usize = 16 * 1024;
const MAX_ERROR_MESSAGE_BYTES: usize = 512;
const MAX_CONCURRENT_REQUESTS: usize = 8;
const MAX_QUEUED_RESPONSES: usize = 16;
const FRAME_BODY_TIMEOUT: Duration = Duration::from_secs(10);
const MCP_DIALOG_REQUEST_TIMEOUT: Duration = Duration::from_secs(15 * 60);
const MCP_CANCELLATION_DRAIN_TIMEOUT: Duration = Duration::from_secs(2);
const MCP_PERSISTENT_STATUS_POLL_INTERVAL: Duration = Duration::from_millis(100);
const MCP_DIALOG_PROMPT_MAX_BYTES: usize = 256;

#[derive(Debug, Deserialize)]
struct JsonRpcRequest {
    jsonrpc: String,
    method: String,
    #[serde(default)]
    params: Value,
    #[serde(default)]
    id: RequestId,
}

#[derive(Debug, Default)]
enum RequestId {
    #[default]
    Missing,
    Present(Value),
}

impl<'de> Deserialize<'de> for RequestId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        Ok(Self::Present(Value::deserialize(deserializer)?))
    }
}

impl RequestId {
    fn is_notification(&self) -> bool {
        matches!(self, Self::Missing)
    }

    fn response_value(&self) -> Option<Value> {
        match self {
            Self::Missing => None,
            Self::Present(value) => Some(value.clone()),
        }
    }

    fn cancellation_key(&self) -> Option<String> {
        match self {
            Self::Present(value @ (Value::String(_) | Value::Number(_))) => Some(value.to_string()),
            Self::Missing | Self::Present(_) => None,
        }
    }

    fn is_valid(&self) -> bool {
        matches!(
            self,
            Self::Missing | Self::Present(Value::Null | Value::String(_) | Value::Number(_))
        )
    }
}

#[derive(Debug, Serialize)]
struct JsonRpcResponse {
    jsonrpc: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<JsonRpcError>,
    #[serde(skip_serializing_if = "Option::is_none")]
    id: Option<Value>,
}

#[derive(Debug, Serialize)]
struct JsonRpcError {
    code: i32,
    message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    data: Option<Value>,
}

#[derive(Debug, Serialize)]
struct Tool {
    name: &'static str,
    description: &'static str,
    #[serde(rename = "inputSchema")]
    input_schema: Value,
}

/// One host-provided MCP tool advertised alongside the browser tool catalog.
#[derive(Debug, Clone, Serialize)]
pub struct HostMcpTool {
    pub name: String,
    pub description: String,
    #[serde(rename = "inputSchema")]
    pub input_schema: Value,
}

/// Optional one-way extension point used by the full Glass product.
///
/// `glass-browser` owns the MCP transport and browser tools while a depending
/// package may contribute product-specific tools without reversing the crate
/// dependency direction.
pub trait HostMcpToolBackend: Send + Sync {
    fn tools(&self) -> Vec<HostMcpTool>;
    fn call(&self, name: &str, arguments: Value) -> Result<Value, String>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FrameFormat {
    ContentLength,
    Newline,
}

#[derive(Debug, PartialEq, Eq)]
struct RequestLogMetadata<'a> {
    method: &'a str,
    request_id_kind: &'static str,
    request_id_present: bool,
    body_bytes: usize,
}

enum ToolInvocation<'a> {
    PreflightNavigation {
        url: &'a str,
    },
    Navigate {
        url: &'a str,
        timeout_ms: u64,
        expected_revision: Option<u64>,
    },
    Click {
        target: Cow<'a, str>,
        expected_revision: Option<u64>,
    },
    Preflight {
        target: Cow<'a, str>,
        action: PreflightAction,
    },
    ClickAt {
        x: f64,
        y: f64,
    },
    ClickExpectPopup {
        target: Cow<'a, str>,
        expected_revision: Option<u64>,
    },
    DoubleClick {
        target: Cow<'a, str>,
        expected_revision: Option<u64>,
    },
    Hover {
        target: Cow<'a, str>,
        expected_revision: Option<u64>,
    },
    Drag {
        source: Cow<'a, str>,
        destination: Cow<'a, str>,
        expected_revision: Option<u64>,
    },
    Type {
        text: &'a str,
        target: Option<&'a str>,
        expected_revision: Option<u64>,
    },
    Key {
        key: &'a str,
        expected_revision: Option<u64>,
    },
    KeyDown {
        key: &'a str,
        expected_revision: Option<u64>,
    },
    KeyUp {
        key: &'a str,
        expected_revision: Option<u64>,
    },
    Shortcut {
        shortcut: &'a str,
        expected_revision: Option<u64>,
    },
    Clear {
        target: Cow<'a, str>,
        expected_revision: Option<u64>,
    },
    Check {
        target: Cow<'a, str>,
        expected_revision: Option<u64>,
    },
    Uncheck {
        target: Cow<'a, str>,
        expected_revision: Option<u64>,
    },
    Select {
        target: Cow<'a, str>,
        value: &'a str,
        expected_revision: Option<u64>,
    },
    Upload {
        target: Cow<'a, str>,
        files: Vec<std::path::PathBuf>,
        expected_revision: Option<u64>,
    },
    Screenshot {
        format: VisualFormat,
        quality: Option<u8>,
        scale: f64,
        full_page: bool,
        clip: Option<VisualClip>,
        target: Option<String>,
    },
    Observe {
        include_dom: bool,
        include_screenshot: bool,
        include_form_values: bool,
        level: Option<SemanticObservationLevel>,
        region: Option<&'a str>,
    },
    ObserveBootstrap,
    InspectPage,
    ExtractWebIr {
        request: ExtractionRequest,
    },
    InspectWebIr {
        ir: Value,
    },
    ValidateWebIr {
        ir: Value,
    },
    DiffWebIr {
        before: Value,
        after: Value,
    },
    ContinuityWebIr {
        before: Value,
        after: Value,
        entity_id: &'a str,
    },
    CompileTask {
        task: crate::task_protocol::GlassTask,
        ir: crate::web_ir::GlassWebIrV1,
    },
    ExecuteTask {
        task: crate::task_protocol::GlassTask,
        expected_revision: u64,
        confirmed: bool,
    },
    ValidateTask {
        task: Value,
    },
    FindTarget {
        request: SemanticIntentRequest,
    },
    ActAndVerify {
        request: SemanticIntentExecutionRequest,
        predicate: Option<VerificationPredicate>,
        timeout: Duration,
    },
    ExtractStructured {
        request: StructuredExtractionRequest,
    },
    RecoverRun {
        execution_id: &'a str,
    },
    SessionSnapshot {
        operation: Cow<'a, str>,
        from: Option<Cow<'a, str>>,
        to: Option<Cow<'a, str>>,
    },
    ObserveKnowledge {
        level: SemanticObservationLevel,
        fresh_only: bool,
        lookup: KnowledgeLookupOptions,
    },
    ResolveIntent {
        request: SemanticIntentRequest,
    },
    ResolveIntentWithKnowledge {
        request: SemanticIntentRequest,
        lookup: KnowledgeLookupOptions,
    },
    ExecuteIntent {
        request: SemanticIntentExecutionRequest,
    },
    KnowledgeList,
    KnowledgeShow {
        record_id: &'a str,
    },
    KnowledgeStats,
    KnowledgeInvalidate {
        record_id: &'a str,
        state: &'a str,
        reason: Option<&'a str>,
        observed_at: Option<&'a str>,
    },
    KnowledgePurge {
        origin: &'a str,
    },
    KnowledgeForget {
        record_id: &'a str,
    },
    KnowledgeExport,
    KnowledgePrune,
    KnowledgeReindex,
    SurfaceInspect {
        surfaces: Value,
        coverage_only: bool,
    },
    BackendInspect {
        profile: Value,
        test: bool,
    },
    WorkspaceStatus,
    WorkspaceInspect {
        id: Option<&'a str>,
    },
    ReplayInspect {
        scenario: Value,
        replay: Value,
    },
    ReplayDiff {
        scenario: Value,
        before: Value,
        after: Value,
    },
    ReplayAttach {
        scenario: Value,
        replay: Value,
    },
    GetDom,
    GetText,
    Evaluate {
        expression: &'a str,
    },
    Batch {
        steps: Value,
        atomic: bool,
        mode: BatchMode,
        expected_revision: Option<u64>,
    },
    Workflow {
        definition: Value,
        inputs: Value,
        checkpoint: Option<Value>,
    },
    Verify {
        predicate: Value,
        timeout_ms: u64,
    },
    ReconcileReferences {
        from_revision: u64,
        refs: Vec<String>,
        hints: Vec<String>,
        scope_ref: Option<String>,
    },
    ObserveDelta,
    SetNetworkConditions {
        preset: Option<String>,
        offline: bool,
        latency_ms: f64,
        download_throughput: f64,
        upload_throughput: f64,
    },
    ClearNetworkConditions,
    SetCpuThrottling {
        rate: f64,
    },
    ClearCpuThrottling,
    SetUserAgent {
        user_agent: String,
        accept_language: Option<String>,
        platform: Option<String>,
    },
    ClearUserAgent,
    ExportCheckpoint,
    ImportCheckpoint {
        checkpoint: Value,
    },
    Scroll {
        dx: f64,
        dy: f64,
        expected_revision: Option<u64>,
    },
    Wait {
        condition: &'a str,
        timeout_ms: u64,
    },
    Diagnostics {
        duration_ms: u64,
    },
    AcceptDialog,
    DismissDialog,
    DismissConsent,
    Download {
        destination: std::path::PathBuf,
        timeout_ms: u64,
    },
    ListTargets,
    CreateTarget {
        url: &'a str,
    },
    SelectTarget {
        id: &'a str,
    },
    CloseTarget {
        id: &'a str,
    },
    ListFrames,
    SelectFrame {
        id: &'a str,
    },
    Cookies,
    SetCookies {
        cookies: Value,
    },
    ClearCookies,
    LocalStorage,
    SessionStorage,
    PrintToPdf {
        options: serde_json::Value,
    },
    FillForm {
        fields: Vec<(String, String)>,
        expected_revision: Option<u64>,
    },
    ClipboardRead,
    ClipboardWrite {
        text: String,
    },
    SetGeolocation {
        latitude: f64,
        longitude: f64,
    },
    ClearGeolocation,
    SetTimezone {
        timezone_id: String,
    },
}

struct Outbound {
    payload: OutboundPayload,
    format: FrameFormat,
}

enum OutboundPayload {
    Response(JsonRpcResponse),
    Json(Value),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
enum CancellationKey {
    Request(String),
    Internal(u64),
}

type CancellationMap = Arc<StdMutex<HashMap<CancellationKey, oneshot::Sender<()>>>>;
type PendingServerResponses = Arc<StdMutex<HashMap<String, oneshot::Sender<Value>>>>;

#[derive(Clone)]
struct McpServerTransport {
    outbound: mpsc::Sender<Outbound>,
    pending_responses: PendingServerResponses,
    active_server_request_cancellations: CancellationMap,
    next_request_id: Arc<AtomicU64>,
}

#[derive(Clone)]
struct McpElicitationClient {
    transport: McpServerTransport,
    format: FrameFormat,
    parent_cancellation: Arc<Notify>,
    parent_cancelled: Arc<AtomicBool>,
    supports_form: bool,
}

#[cfg(feature = "native-engine")]
#[derive(Debug)]
struct NativeMcpFormElicitationRequired;

#[cfg(feature = "native-engine")]
impl std::fmt::Display for NativeMcpFormElicitationRequired {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(
            "MCP client did not negotiate form elicitation; native page dialog was dismissed",
        )
    }
}

#[cfg(feature = "native-engine")]
impl std::error::Error for NativeMcpFormElicitationRequired {}

struct PendingServerResponseCleanup {
    key: String,
    pending_responses: PendingServerResponses,
    request_id: String,
    cancellations: CancellationMap,
    outbound: mpsc::Sender<Outbound>,
    format: FrameFormat,
    request_sent: bool,
    cancel_on_drop: bool,
}

impl Drop for PendingServerResponseCleanup {
    fn drop(&mut self) {
        self.pending_responses
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .remove(&self.key);
        self.cancellations
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .remove(&CancellationKey::Request(self.key.clone()));
        if self.request_sent && self.cancel_on_drop {
            let _ = self.outbound.try_send(Outbound {
                payload: OutboundPayload::Json(json!({
                    "jsonrpc": "2.0",
                    "method": "notifications/cancelled",
                    "params": {"requestId": self.request_id}
                })),
                format: self.format,
            });
        }
    }
}

impl McpServerTransport {
    async fn request(
        &self,
        method: &str,
        params: Value,
        format: FrameFormat,
    ) -> Result<Value, String> {
        let sequence = self.next_request_id.fetch_add(1, Ordering::Relaxed);
        let id = format!("glass-server-{sequence}");
        let key = serde_json::to_string(&Value::String(id.clone()))
            .map_err(|error| format!("could not encode MCP server request ID: {error}"))?;
        let (response_tx, response_rx) = oneshot::channel();
        {
            let mut pending = self
                .pending_responses
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            if pending.contains_key(&key) {
                return Err("duplicate MCP server request ID".into());
            }
            pending.insert(key.clone(), response_tx);
        }
        let (cancel_tx, cancel_rx) = oneshot::channel();
        let duplicate_cancellation_id = {
            let mut cancellations = self
                .active_server_request_cancellations
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            let cancellation_key = CancellationKey::Request(key.clone());
            if cancellations.contains_key(&cancellation_key) {
                true
            } else {
                cancellations.insert(cancellation_key, cancel_tx);
                false
            }
        };
        if duplicate_cancellation_id {
            self.pending_responses
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .remove(&key);
            return Err("duplicate MCP server request cancellation ID".into());
        }
        let mut cleanup = PendingServerResponseCleanup {
            key,
            pending_responses: Arc::clone(&self.pending_responses),
            request_id: id.clone(),
            cancellations: Arc::clone(&self.active_server_request_cancellations),
            outbound: self.outbound.clone(),
            format,
            request_sent: false,
            cancel_on_drop: true,
        };
        let request = json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params,
        });
        send_json(&self.outbound, request, format)
            .await
            .map_err(|error| format!("could not send MCP server request: {error}"))?;
        cleanup.request_sent = true;
        let response = tokio::select! {
            response = tokio::time::timeout(MCP_DIALOG_REQUEST_TIMEOUT, response_rx) => {
                let response = response
                    .map_err(|_| "MCP client input request timed out".to_string())?
                    .map_err(|_| "MCP client disconnected during input request".to_string())?;
                cleanup.cancel_on_drop = false;
                response
            },
            _ = cancel_rx => {
                cleanup.cancel_on_drop = false;
                return Err("MCP client disconnected during input request".into());
            },
        };
        if response.get("error").is_some() {
            return Err("MCP client rejected the native dialog input request".into());
        }
        response
            .get("result")
            .cloned()
            .ok_or_else(|| "MCP client response omitted `result`".into())
    }
}
/// Retained source alias for daemon callers predating the product split.
/// Browser MCP no longer owns development sessions.
pub type DevelopmentSessionStore = ();

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Lifecycle {
    Uninitialized,
    Negotiated,
    Ready,
}

pub async fn run_mcp_server(cli: &Cli) -> BrowserResult<()> {
    let local = tokio::task::LocalSet::new();
    local.run_until(run_mcp_server_local(cli)).await
}

/// Run the stdio MCP server with host-owned tools merged into the catalog.
pub async fn run_mcp_server_with_backend(
    cli: &Cli,
    backend: Arc<dyn HostMcpToolBackend>,
) -> BrowserResult<()> {
    let local = tokio::task::LocalSet::new();
    local
        .run_until(run_mcp_server_local_with_backend(cli, Some(backend)))
        .await
}

async fn run_mcp_server_local(cli: &Cli) -> BrowserResult<()> {
    run_mcp_server_local_with_backend(cli, None).await
}

async fn run_mcp_server_local_with_backend(
    cli: &Cli,
    backend: Option<Arc<dyn HostMcpToolBackend>>,
) -> BrowserResult<()> {
    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();
    run_mcp_stream_inner(
        BufReader::new(stdin),
        stdout,
        cli,
        Arc::new(Mutex::new(None)),
        cli.browser_runtime.is_native(),
        true,
        false,
        None,
        backend,
    )
    .await
}

/// Serve one MCP connection against a caller-provided session store.
///
/// A daemon uses one shared session store for multiple connections. Stdio
/// callers pass an empty store and request cleanup on EOF, preserving the
/// one-process behavior of the standalone server.
pub async fn run_mcp_stream<R, W>(
    reader: R,
    writer: W,
    cli: &Cli,
    session: Arc<Mutex<Option<CdpBrowserSession>>>,
    close_session_on_eof: bool,
    local_daemon: bool,
    lease_context: Option<Arc<DaemonLeaseContext>>,
) -> BrowserResult<()>
where
    R: AsyncBufRead + Unpin,
    W: AsyncWrite + Unpin + 'static,
{
    run_mcp_stream_inner(
        reader,
        writer,
        cli,
        session,
        cli.browser_runtime.is_native(),
        close_session_on_eof,
        local_daemon,
        lease_context,
        None,
    )
    .await
}

#[allow(clippy::too_many_arguments)]
async fn run_mcp_stream_inner<R, W>(
    reader: R,
    writer: W,
    cli: &Cli,
    session: Arc<Mutex<Option<CdpBrowserSession>>>,
    native_runtime: bool,
    close_session_on_eof: bool,
    local_daemon: bool,
    lease_context: Option<Arc<DaemonLeaseContext>>,
    host_backend: Option<Arc<dyn HostMcpToolBackend>>,
) -> BrowserResult<()>
where
    R: AsyncBufRead + Unpin,
    W: AsyncWrite + Unpin + 'static,
{
    info!("MCP server starting on stdio");
    ProfileManager::validate_name(&cli.profile)?;
    let viewport = cli
        .viewport
        .as_deref()
        .map(|value| -> Result<(i64, i64), Box<dyn std::error::Error>> {
            let (width, height) = value
                .split_once('x')
                .ok_or("viewport must use WIDTHxHEIGHT")?;
            Ok((width.parse::<i64>()?, height.parse::<i64>()?))
        })
        .transpose()?;
    let options = SessionOptions {
        port: cli.port,
        chrome_path: cli.chrome_path.clone(),
        profile: cli.profile.clone(),
        incognito: cli.incognito,
        attach: cli.attach,
        target_id: cli.target_id.clone(),
        frame_id: cli.frame_id.clone(),
        headed: cli.headed,
        interaction_mode: cli.interaction,
        audit: cli.audit,
        policy: None,
    };
    let policy = crate::cli::runner::policy_from_cli(cli)?;
    let mut reader = reader;
    let cancellations: CancellationMap = Arc::new(StdMutex::new(HashMap::new()));
    let server_request_cancellations: CancellationMap = Arc::new(StdMutex::new(HashMap::new()));
    let permits = lease_context
        .as_ref()
        .map(|context| Arc::clone(&context.request_permits))
        .unwrap_or_else(|| Arc::new(Semaphore::new(MAX_CONCURRENT_REQUESTS)));
    let client_permits = lease_context
        .as_ref()
        .map(|context| Arc::clone(&context.client_request_permits));
    let (outbound_tx, mut outbound_rx) = mpsc::channel::<Outbound>(MAX_QUEUED_RESPONSES);
    let pending_server_responses: PendingServerResponses = Arc::new(StdMutex::new(HashMap::new()));
    let server_transport = McpServerTransport {
        outbound: outbound_tx.clone(),
        pending_responses: Arc::clone(&pending_server_responses),
        active_server_request_cancellations: Arc::clone(&server_request_cancellations),
        next_request_id: Arc::new(AtomicU64::new(1)),
    };
    let writer = tokio::task::spawn_local(async move {
        let mut writer = writer;
        while let Some(outbound) = outbound_rx.recv().await {
            match outbound.payload {
                OutboundPayload::Response(response) => {
                    write_response(&mut writer, &response, outbound.format).await?;
                }
                OutboundPayload::Json(message) => {
                    write_json_message(&mut writer, &message, outbound.format).await?;
                }
            }
        }
        Ok::<(), io::Error>(())
    });
    let mut next_internal_cancellation_id = 1_u64;
    let mut lifecycle = Lifecycle::Uninitialized;
    let mut client_supports_form_elicitation = false;
    let native_session = Arc::new(Mutex::new(None));
    let persistent_native_session = if native_runtime {
        cli.session.clone()
    } else {
        None
    };

    while let Some((body, format)) = read_message(&mut reader).await? {
        let body_bytes = body.len();
        let message: Value = match serde_json::from_str(&body) {
            Ok(message) => message,
            Err(error) => {
                debug!(body_bytes, "MCP request rejected: invalid JSON");
                let response =
                    error_response(Some(Value::Null), -32700, format!("parse error: {error}"));
                send_response(&outbound_tx, response, format).await?;
                continue;
            }
        };
        if message.get("method").is_none()
            && (message.get("result").is_some() || message.get("error").is_some())
        {
            if !route_server_response(&message, &pending_server_responses) {
                debug!(body_bytes, "MCP server-request response was not correlated");
            }
            continue;
        }
        let request: JsonRpcRequest = match serde_json::from_value(message) {
            Ok(request) => request,
            Err(error) => {
                debug!(body_bytes, "MCP request rejected: invalid request object");
                let response = error_response(
                    Some(Value::Null),
                    -32600,
                    format!("invalid JSON-RPC request: {error}"),
                );
                send_response(&outbound_tx, response, format).await?;
                continue;
            }
        };
        let log = request_log_metadata(&request, body_bytes);
        debug!(
            method = log.method,
            request_id_kind = log.request_id_kind,
            request_id_present = log.request_id_present,
            body_bytes = log.body_bytes,
            "MCP request received"
        );

        if !request.id.is_valid() {
            send_response(
                &outbound_tx,
                error_response(Some(Value::Null), -32600, "invalid JSON-RPC request id"),
                format,
            )
            .await?;
            continue;
        }
        if request.jsonrpc != "2.0" {
            if !request.id.is_notification() {
                send_response(
                    &outbound_tx,
                    error_response(request.id.response_value(), -32600, "jsonrpc must be 2.0"),
                    format,
                )
                .await?;
            }
            continue;
        }
        if request.method == "notifications/cancelled" && request.id.is_notification() {
            cancel_request(&request, &cancellations);
            continue;
        }
        if request.method == "initialize" {
            if request.id.is_notification() {
                continue;
            }
            if lifecycle != Lifecycle::Uninitialized {
                send_response(
                    &outbound_tx,
                    error_response(
                        request.id.response_value(),
                        -32600,
                        "initialize may only be requested once",
                    ),
                    format,
                )
                .await?;
                continue;
            }
            let response = initialize_response_in_mode(
                &request,
                &policy,
                local_daemon,
                cli.experimental_extensions,
            );
            if response.error.is_none() {
                lifecycle = Lifecycle::Negotiated;
                client_supports_form_elicitation = request
                    .params
                    .get("protocolVersion")
                    .and_then(Value::as_str)
                    == Some(MCP_PROTOCOL_VERSION_ELICITATION)
                    && client_declares_form_elicitation(&request);
            }
            send_response(&outbound_tx, response, format).await?;
            continue;
        }
        if request.id.is_notification() && request.method == "notifications/initialized" {
            if lifecycle == Lifecycle::Negotiated {
                lifecycle = Lifecycle::Ready;
            }
            continue;
        }
        if lifecycle != Lifecycle::Ready {
            if !request.id.is_notification() {
                send_response(
                    &outbound_tx,
                    error_response(
                        request.id.response_value(),
                        -32002,
                        "server is not initialized",
                    ),
                    format,
                )
                .await?;
            }
            continue;
        }

        let is_host_tool = request.method == "tools/call"
            && request
                .params
                .get("name")
                .and_then(Value::as_str)
                .is_some_and(|name| {
                    host_backend
                        .as_ref()
                        .is_some_and(|backend| backend.tools().iter().any(|tool| tool.name == name))
                });
        if request.method == "tools/call"
            && !is_host_tool
            && let Err(error) = canonical_tool_request(&request)
        {
            if !request.id.is_notification() {
                send_response(
                    &outbound_tx,
                    error_response(request.id.response_value(), -32602, error),
                    format,
                )
                .await?;
            }
            continue;
        }

        if local_daemon && is_lease_method(&request.method) {
            let lease_context = lease_context
                .as_ref()
                .expect("daemon lease context available");
            let response = handle_lease_request(
                &request,
                &lease_context.manager,
                &lease_context.session_id,
                &lease_context.owner_id,
                Some(&lease_context.status),
            )
            .await;
            if !request.id.is_notification() {
                send_response(&outbound_tx, response, format).await?;
            }
            continue;
        }
        if local_daemon
            && request.method == "tools/call"
            && let Some(lease_context) = lease_context.as_ref()
            && let Some(error) = mutation_lease_error(
                &request,
                &lease_context.manager,
                &lease_context.session_id,
                &lease_context.owner_id,
            )
            .await
        {
            if !request.id.is_notification() {
                send_response(&outbound_tx, error, format).await?;
            }
            continue;
        }

        let permit = match Arc::clone(&permits).try_acquire_owned() {
            Ok(permit) => permit,
            Err(_) => {
                if !request.id.is_notification() {
                    send_response(
                        &outbound_tx,
                        error_response(
                            request.id.response_value(),
                            -32000,
                            "too many concurrent requests",
                        ),
                        format,
                    )
                    .await?;
                }
                continue;
            }
        };
        let client_permit = match client_permits
            .as_ref()
            .map(|permits| Arc::clone(permits).try_acquire_owned())
        {
            Some(Ok(permit)) => Some(permit),
            Some(Err(_)) => {
                if !request.id.is_notification() {
                    send_response(
                        &outbound_tx,
                        error_response(
                            request.id.response_value(),
                            -32000,
                            "too many concurrent requests from this daemon client",
                        ),
                        format,
                    )
                    .await?;
                }
                continue;
            }
            None => None,
        };
        let cancellation_key = request
            .id
            .cancellation_key()
            .map(CancellationKey::Request)
            .unwrap_or_else(|| {
                let key = CancellationKey::Internal(next_internal_cancellation_id);
                next_internal_cancellation_id = next_internal_cancellation_id
                    .checked_add(1)
                    .expect("internal MCP cancellation ID exhausted");
                key
            });
        let (cancel_tx, cancel_rx) = oneshot::channel();
        let duplicate = {
            let mut active = cancellations.lock().expect("cancellation map poisoned");
            if active.contains_key(&cancellation_key) {
                true
            } else {
                active.insert(cancellation_key.clone(), cancel_tx);
                false
            }
        };
        if duplicate {
            send_response(
                &outbound_tx,
                error_response(
                    request.id.response_value(),
                    -32600,
                    "duplicate active request id",
                ),
                format,
            )
            .await?;
            continue;
        }
        let active_workflow_request = if local_daemon {
            workflow_request_id(&request)
        } else {
            None
        };
        let active_workflow_status = lease_context
            .as_ref()
            .map(|context| Arc::clone(&context.status));
        let active_workflow_owner = lease_context
            .as_ref()
            .map(|context| context.owner_id.clone());
        if let (Some(request_id), Some(status)) = (
            active_workflow_request.as_deref(),
            active_workflow_status.as_ref(),
        ) && let Err(error) = status
            .begin_workflow(
                request_id,
                &lease_context
                    .as_ref()
                    .expect("daemon lease context available")
                    .owner_id,
            )
            .await
        {
            task_cancellations_remove(&cancellations, &cancellation_key);
            if !request.id.is_notification() {
                send_response(
                    &outbound_tx,
                    error_response(
                        request.id.response_value(),
                        workflow_status_error_code(&error.to_string()),
                        error.to_string(),
                    ),
                    format,
                )
                .await?;
            }
            continue;
        }
        let task_session = Arc::clone(&session);
        let task_native_session = Arc::clone(&native_session);
        let task_options = options.clone();
        let task_policy = policy.clone();
        let task_viewport = viewport;
        let task_knowledge_store = cli.knowledge_store.clone();
        let task_development_sessions = ();
        let task_host_backend = host_backend.clone();
        let task_persistent_native_session = persistent_native_session.clone();
        let task_outbound = outbound_tx.clone();
        let task_cancellations = Arc::clone(&cancellations);
        let task_elicitation_cancellation = native_runtime.then(|| Arc::new(Notify::new()));
        let task_parent_cancelled = native_runtime.then(|| Arc::new(AtomicBool::new(false)));
        let persistent_dialog_host = task_persistent_native_session.is_some();
        let task_elicitation = (native_runtime
            && (client_supports_form_elicitation || persistent_dialog_host))
            .then(|| McpElicitationClient {
                transport: server_transport.clone(),
                format,
                parent_cancellation: Arc::clone(
                    task_elicitation_cancellation
                        .as_ref()
                        .expect("native request has a cancellation notifier"),
                ),
                parent_cancelled: Arc::clone(
                    task_parent_cancelled
                        .as_ref()
                        .expect("native request has a cancellation state"),
                ),
                supports_form: client_supports_form_elicitation,
            });
        let native_session_owned = Arc::new(AtomicBool::new(false));
        let task_native_session_owned = Arc::clone(&native_session_owned);
        tokio::task::spawn_local(async move {
            let _permit = permit;
            let _client_permit = client_permit;
            let is_notification = request.id.is_notification();
            let id = request.id.response_value();
            let operation = async {
                let mut session = task_session.lock().await;
                let mut native_session = task_native_session.lock().await;
                task_native_session_owned.store(true, Ordering::Release);
                handle_request_with_viewport(
                    &request,
                    &mut session,
                    &mut native_session,
                    native_runtime,
                    task_persistent_native_session.as_deref(),
                    &task_options,
                    &task_policy,
                    task_viewport,
                    task_knowledge_store.as_deref(),
                    &task_development_sessions,
                    task_host_backend.as_deref(),
                    task_elicitation.as_ref(),
                )
                .await
            };
            let mut operation = Box::pin(operation);
            let mut response = tokio::select! {
                response = &mut operation => response,
                _ = cancel_rx => {
                    #[cfg(feature = "native-engine")]
                    if native_runtime && native_session_owned.load(Ordering::Acquire) {
                        if let Some(cancelled) = &task_parent_cancelled {
                            cancelled.store(true, Ordering::Release);
                        }
                        if let Some(cancellation) = &task_elicitation_cancellation {
                            cancellation.notify_one();
                        }
                        if tokio::time::timeout(
                            MCP_CANCELLATION_DRAIN_TIMEOUT,
                            &mut operation,
                        )
                        .await
                        .is_err()
                        {
                            drop(operation);
                            discard_native_mcp_session(&task_native_session).await;
                        }
                    } else {
                        drop(operation);
                    }
                    #[cfg(not(feature = "native-engine"))]
                    drop(operation);
                    Some(error_response(id, -32800, "request cancelled"))
                }
            };
            if is_notification {
                response = None;
            }
            task_cancellations_remove(&task_cancellations, &cancellation_key);
            if let (Some(request_id), Some(owner_id), Some(status)) = (
                active_workflow_request.as_deref(),
                active_workflow_owner.as_deref(),
                active_workflow_status.as_ref(),
            ) && let Err(error) = status.finish_workflow(request_id, owner_id).await
            {
                tracing::warn!(%error, request_id, "failed to clear completed workflow status");
            }
            if let Some(response) = response {
                let _ = send_response(&task_outbound, response, format).await;
            }
        });
    }

    // EOF cancels in-flight client and server requests so a modal browser
    // operation cannot strand the stdio writer or owned browser session.
    drop(server_transport);
    cancel_active_requests(&cancellations);
    cancel_active_requests(&server_request_cancellations);
    drop(outbound_tx);
    writer.await??;
    if close_session_on_eof {
        let mut session = session.lock().await;
        if let Some(session) = session.take() {
            session.close().await?;
        }
        let mut native_session = native_session.lock().await;
        if let Some(session) = native_session.take() {
            session.close().await?;
        }
    }
    if let Some(lease_context) = lease_context {
        let mut manager = lease_context.manager.lock().await;
        manager.release_owner(&lease_context.owner_id);
        let owner = manager.current_owner(&lease_context.session_id, current_time_ms());
        drop(manager);
        let _ = lease_context
            .status
            .update_mutation_lease_owner(owner)
            .await;
    }
    Ok(())
}

fn is_lease_method(method: &str) -> bool {
    matches!(
        method,
        "glass/lease/acquire" | "glass/lease/renew" | "glass/lease/release"
    )
}

fn workflow_request_id(request: &JsonRpcRequest) -> Option<String> {
    if request.method != "tools/call"
        || request.params.get("name").and_then(Value::as_str) != Some("workflow")
    {
        return None;
    }
    match &request.id {
        RequestId::Present(Value::String(value)) if !value.is_empty() && value.len() <= 128 => {
            Some(value.clone())
        }
        RequestId::Present(Value::Number(value)) => Some(value.to_string()),
        _ => None,
    }
}

fn canonical_tool_request(request: &JsonRpcRequest) -> Result<GlassRequest, String> {
    let name = request
        .params
        .get("name")
        .and_then(Value::as_str)
        .ok_or_else(|| "tools/call requires a string name".to_string())?;
    let arguments = request
        .params
        .get("arguments")
        .cloned()
        .unwrap_or_else(|| json!({}));
    if !arguments.is_object() {
        return Err("tools/call arguments must be an object".into());
    }
    let request_id = match &request.id {
        RequestId::Present(Value::String(value)) if !value.is_empty() => value.clone(),
        RequestId::Present(Value::Number(value)) => value.to_string(),
        RequestId::Missing => "notification".into(),
        RequestId::Present(Value::Null) => "null-request".into(),
        RequestId::Present(_) => return Err("tools/call request id is not canonical".into()),
    };
    let operation = match name {
        "inspectWebIr" => crate::protocol::WEB_IR_INSPECT_OPERATION.to_string(),
        "validateWebIr" => crate::protocol::WEB_IR_VALIDATE_OPERATION.to_string(),
        "diffWebIr" => crate::protocol::WEB_IR_DIFF_OPERATION.to_string(),
        "continuityWebIr" => crate::protocol::WEB_IR_CONTINUITY_OPERATION.to_string(),
        "compileTask" => crate::protocol::TASK_COMPILE_OPERATION.to_string(),
        "executeTask" => crate::protocol::TASK_EXECUTE_OPERATION.to_string(),
        "validateTask" => crate::protocol::TASK_VALIDATE_OPERATION.to_string(),
        _ => format!("browser.{name}"),
    };
    let session_id = arguments
        .get("sessionId")
        .and_then(Value::as_str)
        .map(str::to_string);
    let mutation_lease = arguments
        .get("mutationLease")
        .and_then(Value::as_object)
        .and_then(|lease| {
            Some(crate::protocol::MutationLeaseRef {
                session_id: lease.get("sessionId")?.as_str()?.to_string(),
                token: lease.get("token")?.as_str()?.to_string(),
            })
        })
        .or_else(|| {
            Some(crate::protocol::MutationLeaseRef {
                session_id: session_id.clone()?,
                token: arguments.get("leaseToken")?.as_str()?.to_string(),
            })
        });
    let canonical = GlassRequest {
        protocol_version: GLASS_PROTOCOL_VERSION,
        request_id,
        correlation_id: None,
        session_id,
        mutation_lease,
        operation,
        payload: arguments,
        deadline_ms: None,
    };
    canonical.validate().map_err(|error| error.to_string())?;
    Ok(canonical)
}

fn task_cancellations_remove(cancellations: &CancellationMap, key: &CancellationKey) {
    cancellations
        .lock()
        .expect("cancellation map poisoned")
        .remove(key);
}

async fn handle_lease_request(
    request: &JsonRpcRequest,
    lease_manager: &Arc<Mutex<MutationLeaseManager>>,
    session_id: &str,
    owner_id: &str,
    status: Option<&Arc<crate::daemon::DaemonStatusState>>,
) -> JsonRpcResponse {
    let params = &request.params;
    let mut manager = lease_manager.lock().await;
    let now_ms = current_time_ms();
    let result = match request.method.as_str() {
        "glass/lease/acquire" => {
            let Some(ttl_ms) = params.get("ttlMs").and_then(Value::as_u64) else {
                return error_response(
                    request.id.response_value(),
                    -32602,
                    "glass/lease/acquire requires numeric ttlMs",
                );
            };
            manager.acquire(session_id, owner_id, now_ms, ttl_ms)
        }
        "glass/lease/renew" => {
            let Some(token) = params.get("token").and_then(Value::as_str) else {
                return error_response(
                    request.id.response_value(),
                    -32602,
                    "glass/lease/renew requires string token",
                );
            };
            let Some(ttl_ms) = params.get("ttlMs").and_then(Value::as_u64) else {
                return error_response(
                    request.id.response_value(),
                    -32602,
                    "glass/lease/renew requires numeric ttlMs",
                );
            };
            manager.renew(session_id, owner_id, token, now_ms, ttl_ms)
        }
        "glass/lease/release" => {
            let Some(token) = params.get("token").and_then(Value::as_str) else {
                return error_response(
                    request.id.response_value(),
                    -32602,
                    "glass/lease/release requires string token",
                );
            };
            return match manager.release(session_id, owner_id, token) {
                Ok(()) => {
                    if let Some(status) = status
                        && let Err(error) = status.update_mutation_lease_owner(None).await
                    {
                        tracing::warn!(%error, "failed to clear daemon lease owner status");
                    }
                    success_response(
                        request.id.response_value(),
                        json!({"sessionId": session_id, "released": true}),
                    )
                }
                Err(error) => lease_error_response(request, error),
            };
        }
        _ => {
            return error_response(
                request.id.response_value(),
                -32601,
                "unknown Glass lease method",
            );
        }
    };
    match result {
        Ok(lease) => {
            if let Some(status) = status
                && let Err(error) = status
                    .update_mutation_lease_owner(Some(owner_id.to_string()))
                    .await
            {
                tracing::warn!(%error, "failed to record daemon lease owner status");
            }
            success_response(request.id.response_value(), json!(lease))
        }
        Err(error) => lease_error_response(request, error),
    }
}

async fn mutation_lease_error(
    request: &JsonRpcRequest,
    lease_manager: &Arc<Mutex<MutationLeaseManager>>,
    session_id: &str,
    owner_id: &str,
) -> Option<JsonRpcResponse> {
    let tool_name = request.params.get("name").and_then(Value::as_str)?;
    if tool_name == "executeTask" {
        if !execute_task_requires_mutation_lease(request) {
            return None;
        }
    } else if tool_name == "sessionSnapshot" {
        if !session_snapshot_requires_mutation_lease(request) {
            return None;
        }
    } else if !tool_requires_mutation_lease(tool_name) {
        return None;
    }
    let arguments = request.params.get("arguments")?;
    let lease = arguments.get("mutationLease").and_then(Value::as_object);
    let token = lease
        .and_then(|lease| lease.get("token"))
        .and_then(Value::as_str)
        .or_else(|| arguments.get("leaseToken").and_then(Value::as_str));
    let Some(token) = token else {
        return Some(error_response(
            request.id.response_value(),
            -32003,
            "mutation lease required; call glass/lease/acquire first",
        ));
    };
    let manager = lease_manager.lock().await;
    manager
        .validate(session_id, owner_id, token, current_time_ms())
        .err()
        .map(|error| lease_error_response(request, error))
}
fn execute_task_requires_mutation_lease(request: &JsonRpcRequest) -> bool {
    let Some(task) = request
        .params
        .get("arguments")
        .and_then(|arguments| arguments.get("task"))
        .cloned()
    else {
        return true;
    };
    let Ok(task) = serde_json::from_value::<crate::task_protocol::GlassTask>(task) else {
        return true;
    };
    !matches!(
        task.task,
        crate::task_protocol::TaskKind::FormInspect
            | crate::task_protocol::TaskKind::FormValidate
            | crate::task_protocol::TaskKind::FieldRead
            | crate::task_protocol::TaskKind::TableExtract
            | crate::task_protocol::TaskKind::CollectionExtract
            | crate::task_protocol::TaskKind::RegionExtract
            | crate::task_protocol::TaskKind::DialogInspect
    )
}

fn session_snapshot_requires_mutation_lease(request: &JsonRpcRequest) -> bool {
    matches!(
        request
            .params
            .get("arguments")
            .and_then(|arguments| arguments.get("operation"))
            .and_then(Value::as_str),
        Some("create" | "purge")
    )
}

fn workflow_status_error_code(message: &str) -> i32 {
    if message.contains("already active") {
        -32600
    } else if message.contains("limit reached") {
        -32000
    } else if message.contains("bounded") {
        -32602
    } else {
        -32603
    }
}

fn tool_requires_mutation_lease(tool_name: &str) -> bool {
    !matches!(
        tool_name,
        "screenshot"
            | "observe"
            | "observeBootstrap"
            | "observeKnowledge"
            | "inspectPage"
            | "extractWebIr"
            | "inspectWebIr"
            | "validateWebIr"
            | "diffWebIr"
            | "continuityWebIr"
            | "compileTask"
            | "validateTask"
            | "findTarget"
            | "extractStructured"
            | "recoverRun"
            | "sessionSnapshot"
            | "replayInspect"
            | "replayDiff"
            | "replayAttach"
            | "resolveIntent"
            | "resolveIntentWithKnowledge"
            | "knowledgeList"
            | "knowledgeShow"
            | "knowledgeStats"
            | "preflight"
            | "preflightNavigation"
            | "getDOM"
            | "getText"
            | "listTargets"
            | "listFrames"
            | "cookies"
            | "localStorage"
            | "sessionStorage"
            | "diagnostics"
            | "verify"
            | "observeDelta"
            | "workspaceStatus"
            | "workspaceInspect"
            | "project.inspect"
            | "project.files"
            | "project.read"
            | "project.processes"
            | "project.process.output"
            | "project.diff"
            | "project.timeline"
            | "project.events"
            | "project.session.status"
            | "project.capsule.show"
            | "project.inbox"
            | "project.verification.card"
            | "agent.hello"
    )
}

fn lease_error_response(request: &JsonRpcRequest, error: LeaseError) -> JsonRpcResponse {
    let (code, retryable) = match error {
        LeaseError::AlreadyHeld => ("leaseHeld", true),
        LeaseError::Expired => ("leaseExpired", true),
        LeaseError::NotFound => ("leaseNotFound", true),
        LeaseError::NotOwner => ("leaseNotOwner", false),
        LeaseError::InvalidInput(_) => ("invalidLease", false),
    };
    error_response_with_data(
        request.id.response_value(),
        -32003,
        error.to_string(),
        json!({"code": code, "retryable": retryable}),
    )
}

fn current_time_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

async fn send_response(
    sender: &mpsc::Sender<Outbound>,
    response: JsonRpcResponse,
    format: FrameFormat,
) -> io::Result<()> {
    sender
        .send(Outbound {
            payload: OutboundPayload::Response(response),
            format,
        })
        .await
        .map_err(|_| io::Error::new(io::ErrorKind::BrokenPipe, "MCP output task stopped"))
}

async fn send_json(
    sender: &mpsc::Sender<Outbound>,
    message: Value,
    format: FrameFormat,
) -> io::Result<()> {
    sender
        .send(Outbound {
            payload: OutboundPayload::Json(message),
            format,
        })
        .await
        .map_err(|_| io::Error::new(io::ErrorKind::BrokenPipe, "MCP output task stopped"))
}

fn request_log_metadata(request: &JsonRpcRequest, body_bytes: usize) -> RequestLogMetadata<'_> {
    let request_id_kind = match &request.id {
        RequestId::Missing => "absent",
        RequestId::Present(Value::Null) => "null",
        RequestId::Present(Value::String(_)) => "string",
        RequestId::Present(Value::Number(_)) => "number",
        RequestId::Present(_) => "invalid",
    };
    RequestLogMetadata {
        method: &request.method,
        request_id_kind,
        request_id_present: !request.id.is_notification(),
        body_bytes,
    }
}

fn request_id_key(id: &Value) -> Option<String> {
    matches!(id, Value::String(_) | Value::Number(_)).then(|| id.to_string())
}

fn route_server_response(message: &Value, pending_responses: &PendingServerResponses) -> bool {
    if message.get("jsonrpc").and_then(Value::as_str) != Some("2.0")
        || message.get("result").is_some() == message.get("error").is_some()
    {
        return false;
    }
    let Some(key) = message.get("id").and_then(request_id_key) else {
        return false;
    };
    let response = pending_responses
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .remove(&key);
    response.is_some_and(|response| response.send(message.clone()).is_ok())
}

fn cancel_active_requests(cancellations: &CancellationMap) {
    let active = {
        let mut cancellations = cancellations
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        std::mem::take(&mut *cancellations)
    };
    for cancellation in active.into_values() {
        let _ = cancellation.send(());
    }
}

fn cancel_request(request: &JsonRpcRequest, cancellations: &CancellationMap) {
    let Some(key) = request.params.get("requestId").and_then(request_id_key) else {
        return;
    };
    if let Some(cancellation) = cancellations
        .lock()
        .expect("cancellation map poisoned")
        .remove(&CancellationKey::Request(key))
    {
        let _ = cancellation.send(());
    }
}

fn initialize_response(request: &JsonRpcRequest, policy: &BrowserPolicy) -> JsonRpcResponse {
    initialize_response_in_mode(request, policy, false, false)
}

fn initialize_response_in_mode(
    request: &JsonRpcRequest,
    policy: &BrowserPolicy,
    local_daemon: bool,
    experimental_extensions: bool,
) -> JsonRpcResponse {
    if request.jsonrpc != "2.0" {
        return error_response(request.id.response_value(), -32600, "jsonrpc must be 2.0");
    }
    let Some(version) = request
        .params
        .get("protocolVersion")
        .and_then(Value::as_str)
    else {
        return error_response(
            request.id.response_value(),
            -32602,
            "protocolVersion must be a supported string",
        );
    };
    if !matches!(
        version,
        MCP_PROTOCOL_VERSION | MCP_PROTOCOL_VERSION_ELICITATION
    ) {
        return error_response(
            request.id.response_value(),
            -32602,
            "unsupported MCP protocol version",
        );
    }
    let manifest = GlassCapabilityManifest::for_policy_in_mode_with_experimental_extensions(
        policy,
        local_daemon,
        experimental_extensions,
    );
    let agreement = manifest
        .negotiate(request.params.get("glass"))
        .map_err(|error| {
            error_response(
                request.id.response_value(),
                -32602,
                format!("Glass capability negotiation failed: {error}"),
            )
        });
    let agreement = match agreement {
        Ok(agreement) => agreement,
        Err(response) => return response,
    };
    success_response(
        request.id.response_value(),
        json!({
            "protocolVersion": version,
            "capabilities": {
                "tools": {"listChanged": false},
                "prompts": {"listChanged": false},
                "resources": {"subscribe": false, "listChanged": false}
            },
            "glass": manifest,
            "glassAgreement": agreement,
            "serverInfo": {"name": "glass", "version": env!("CARGO_PKG_VERSION")}
        }),
    )
}

fn client_declares_form_elicitation(request: &JsonRpcRequest) -> bool {
    let Some(capabilities) = request
        .params
        .get("capabilities")
        .and_then(|capabilities| capabilities.get("elicitation"))
        .and_then(Value::as_object)
    else {
        return false;
    };
    capabilities.is_empty() || capabilities.get("form").is_some_and(Value::is_object)
}

#[cfg(test)]
async fn handle_request(
    request: &JsonRpcRequest,
    session: &mut Option<CdpBrowserSession>,
    options: &SessionOptions,
    policy: &BrowserPolicy,
    knowledge_store_path: Option<&Path>,
) -> Option<JsonRpcResponse> {
    let development_sessions = ();
    let mut native_session = None;
    handle_request_with_viewport(
        request,
        session,
        &mut native_session,
        false,
        None,
        options,
        policy,
        None,
        knowledge_store_path,
        &development_sessions,
        None,
        None,
    )
    .await
}

#[allow(clippy::too_many_arguments)]
async fn handle_request_with_viewport(
    request: &JsonRpcRequest,
    session: &mut Option<CdpBrowserSession>,
    native_session: &mut Option<BrowserRuntimeSession>,
    native_runtime: bool,
    persistent_native_session: Option<&str>,
    options: &SessionOptions,
    policy: &BrowserPolicy,
    viewport: Option<(i64, i64)>,
    knowledge_store_path: Option<&Path>,
    _development_sessions: &DevelopmentSessionStore,
    host_backend: Option<&dyn HostMcpToolBackend>,
    elicitation: Option<&McpElicitationClient>,
) -> Option<JsonRpcResponse> {
    if request.id.is_notification() && request.method == "notifications/initialized" {
        return None;
    }
    if request.jsonrpc != "2.0" {
        return Some(error_response(
            request.id.response_value(),
            -32600,
            "jsonrpc must be 2.0",
        ));
    }

    let response = match request.method.as_str() {
        "initialize" => initialize_response(request, policy),
        "ping" => success_response(request.id.response_value(), json!({})),
        "tools/list" => {
            let mut catalog = tools()
                .into_iter()
                .filter_map(|tool| serde_json::to_value(tool).ok())
                .collect::<Vec<_>>();
            if let Some(backend) = host_backend {
                let host_tools = backend.tools();
                catalog.retain(|tool| {
                    let name = tool.get("name").and_then(Value::as_str);
                    !host_tools
                        .iter()
                        .any(|host| Some(host.name.as_str()) == name)
                });
                catalog.extend(
                    host_tools
                        .into_iter()
                        .filter_map(|tool| serde_json::to_value(tool).ok()),
                );
            }
            success_response(request.id.response_value(), json!({"tools": catalog}))
        }
        "prompts/list" => match prompts::list_prompts() {
            Ok(result) => success_response(request.id.response_value(), result),
            Err(error) => error_response(
                request.id.response_value(),
                -32603,
                format!("prompts/list failed: {error}"),
            ),
        },
        "prompts/get" => match request.params.get("name").and_then(Value::as_str) {
            Some(name) => match prompts::get_prompt(name) {
                Ok(result) => success_response(request.id.response_value(), result),
                Err(error) => error_response(
                    request.id.response_value(),
                    -32602,
                    format!("prompts/get failed: {error}"),
                ),
            },
            None => error_response(
                request.id.response_value(),
                -32602,
                "prompts/get requires a string `name` parameter",
            ),
        },
        "resources/list" => match resources::list_resources() {
            Ok(result) => success_response(request.id.response_value(), result),
            Err(error) => error_response(
                request.id.response_value(),
                -32603,
                format!("resources/list failed: {error}"),
            ),
        },
        "resources/read" => match request.params.get("uri").and_then(Value::as_str) {
            Some(uri) => match resources::read_resource(uri) {
                Ok(result) => success_response(request.id.response_value(), result),
                Err(error) => error_response(
                    request.id.response_value(),
                    -32602,
                    format!("resources/read failed: {error}"),
                ),
            },
            None => error_response(
                request.id.response_value(),
                -32602,
                "resources/read requires a string `uri` parameter",
            ),
        },
        "tools/call"
            if request
                .params
                .get("name")
                .and_then(Value::as_str)
                .is_some_and(|name| {
                    host_backend
                        .is_some_and(|backend| backend.tools().iter().any(|tool| tool.name == name))
                }) =>
        {
            let name = request.params["name"]
                .as_str()
                .expect("host tool name checked above");
            let arguments = request
                .params
                .get("arguments")
                .cloned()
                .unwrap_or_else(|| json!({}));
            match host_backend
                .expect("host backend checked above")
                .call(name, arguments)
            {
                Ok(result) => success_response(
                    request.id.response_value(),
                    json!({
                        "content":[{"type":"text","text":serde_json::to_string(&result).unwrap_or_else(|_| "null".into())}],
                        "structuredContent":result,
                        "isError":false
                    }),
                ),
                Err(error) => success_response(
                    request.id.response_value(),
                    json!({"content":[{"type":"text","text":error}],"isError":true}),
                ),
            }
        }
        "tools/call" => match Box::pin(call_tool(
            request,
            session,
            native_session,
            native_runtime,
            persistent_native_session,
            options,
            policy,
            viewport,
            knowledge_store_path,
            _development_sessions,
            elicitation,
        ))
        .await
        {
            Ok(result) => success_response(request.id.response_value(), result),
            Err(error) => {
                let text = typed_browser_error(error.as_ref())
                    .unwrap_or_else(|| "browser tool failed".to_string());
                let mut content = vec![json!({"type": "text", "text": text})];
                if requested_failure_trace(request) {
                    let trace = match session.as_ref() {
                        Some(browser_session) => serde_json::to_value(
                            browser_session
                                .failure_trace_for(
                                    mcp_trace_action(
                                        request.params.get("name").and_then(Value::as_str),
                                    ),
                                    error.to_string(),
                                )
                                .await,
                        )
                        .unwrap_or_else(|_| json!({"error": "failure trace serialization failed"})),
                        None => json!({
                            "error": error.to_string(),
                            "session": "not_started"
                        }),
                    };
                    content.push(json!({
                        "type": "text",
                        "text": serde_json::to_string(&trace).unwrap_or_else(|_| "{}".to_string())
                    }));
                }
                let mut response = success_response(
                    request.id.response_value(),
                    json!({
                        "content": content,
                        "isError": true
                    }),
                );
                response.error = None;
                response
            }
        },
        _ => error_response(
            request.id.response_value(),
            -32601,
            format!("method not found: {}", request.method),
        ),
    };
    Some(response)
}

fn requested_failure_trace(request: &JsonRpcRequest) -> bool {
    request
        .params
        .get("arguments")
        .and_then(|arguments| arguments.get("includeTrace"))
        .and_then(Value::as_bool)
        .unwrap_or(false)
}

fn mcp_trace_action(tool: Option<&str>) -> ActionKind {
    match tool {
        Some("clickExpectPopup") => ActionKind::ClickExpectPopup,
        Some("doubleClick") => ActionKind::DoubleClick,
        Some("hover") => ActionKind::Hover,
        Some("drag") => ActionKind::Drag,
        Some("type") => ActionKind::Type,
        Some("key") => ActionKind::KeyPress,
        Some("keyDown") => ActionKind::KeyDown,
        Some("keyUp") => ActionKind::KeyUp,
        Some("shortcut") => ActionKind::Shortcut,
        Some("clear") => ActionKind::Clear,
        Some("check") => ActionKind::Check,
        Some("uncheck") => ActionKind::Uncheck,
        Some("select") => ActionKind::Select,
        Some("upload") => ActionKind::Upload,
        Some("scroll") => ActionKind::Scroll,
        _ => ActionKind::Click,
    }
}

fn typed_browser_error(error: &(dyn std::error::Error + 'static)) -> Option<String> {
    #[cfg(feature = "native-engine")]
    if error
        .downcast_ref::<NativeMcpFormElicitationRequired>()
        .is_some()
    {
        return Some(error.to_string());
    }
    if let Some(error) = error.downcast_ref::<crate::web_ir::WebIrValidationError>() {
        return serde_json::to_string(&json!({
            "kind": "webIrValidation",
            "path": error.path,
            "reason": error.reason,
        }))
        .ok();
    }
    if let Some(error) = error.downcast_ref::<crate::task_protocol::TaskProtocolError>() {
        return serde_json::to_string(&json!({
            "kind": "taskValidation",
            "path": error.path,
            "reason": error.reason,
        }))
        .ok();
    }

    if let Some(error) = error.downcast_ref::<TaskCompilationError>() {
        return serde_json::to_string(&json!({
            "kind": "taskCompilation",
            "path": error.path,
            "reason": error.reason,
        }))
        .ok();
    }
    if let Some(error) = error.downcast_ref::<crate::protocol::ProtocolError>() {
        return match error {
            crate::protocol::ProtocolError::TaskValidation(error) => {
                serde_json::to_string(&json!({
                    "kind": "taskValidation",
                    "path": error.path,
                    "reason": error.reason,
                }))
                .ok()
            }
            crate::protocol::ProtocolError::TaskCompilation(error) => {
                serde_json::to_string(&json!({
                    "kind": "taskCompilation",
                    "path": error.path,
                    "reason": error.reason,
                }))
                .ok()
            }
            crate::protocol::ProtocolError::WebIrValidation(error) => {
                serde_json::to_string(&json!({
                    "kind": "webIrValidation",
                    "path": error.path,
                    "reason": error.reason,
                }))
                .ok()
            }
            _ => None,
        };
    }

    error
        .downcast_ref::<TargetError>()
        .and_then(|error| serde_json::to_string(error).ok())
        .or_else(|| {
            error
                .downcast_ref::<WaitTimeout>()
                .and_then(|error| serde_json::to_string(error).ok())
        })
        .or_else(|| {
            error
                .downcast_ref::<ActionContractError>()
                .and_then(|error| serde_json::to_string(error).ok())
        })
        .or_else(|| {
            error
                .downcast_ref::<ActionVerificationError>()
                .and_then(|error| serde_json::to_string(error).ok())
        })
        .or_else(|| {
            error.downcast_ref::<CdpError>().and_then(|error| {
                serde_json::to_string(&json!({
                    "kind": "transport",
                    "code": error.code,
                    "message": error.message,
                    "data": error.data,
                }))
                .ok()
            })
        })
        .or_else(|| {
            error
                .downcast_ref::<PolicyError>()
                .and_then(|error| serde_json::to_string(&error.contract()).ok())
        })
        .or_else(|| {
            error
                .downcast_ref::<PopupClickError>()
                .and_then(|error| serde_json::to_string(error).ok())
        })
        .or_else(|| {
            error
                .downcast_ref::<DownloadError>()
                .and_then(|error| serde_json::to_string(error).ok())
        })
}

fn canonical_payload_request(
    request: &JsonRpcRequest,
    payload: Value,
) -> BrowserResult<GlassRequest> {
    let mut canonical = canonical_tool_request(request)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidInput, error))?;
    canonical.payload = payload;
    Ok(canonical)
}
fn browser_free_session_snapshot(
    operation: &str,
    from: Option<&str>,
    to: Option<&str>,
    profile: &str,
    response_mode: ResponseMode,
) -> BrowserResult<Value> {
    let store = SessionSnapshotStore::new(default_session_snapshot_path(profile));
    match operation {
        "list" => serialized_result_mode(&store.list()?, response_mode),
        "inspect" => {
            let id = from.ok_or("sessionSnapshot inspect requires from")?;
            serialized_result_mode(&store.load(id)?, response_mode)
        }
        "diff" => {
            let left = from.ok_or("sessionSnapshot diff requires from")?;
            let right = to.ok_or("sessionSnapshot diff requires to")?;
            serialized_result_mode(&store.diff(left, right)?, response_mode)
        }
        "purge" => serialized_result_mode(&json!({"removed": store.purge()?}), response_mode),
        _ => Err("sessionSnapshot operation must be list, inspect, diff, or purge".into()),
    }
}

#[allow(clippy::too_many_arguments)]
async fn call_tool(
    request: &JsonRpcRequest,
    session: &mut Option<CdpBrowserSession>,
    _native_session: &mut Option<BrowserRuntimeSession>,
    native_runtime: bool,
    _persistent_native_session: Option<&str>,
    options: &SessionOptions,
    policy: &BrowserPolicy,
    viewport: Option<(i64, i64)>,
    knowledge_store_path: Option<&Path>,
    _development_sessions: &DevelopmentSessionStore,
    elicitation: Option<&McpElicitationClient>,
) -> BrowserResult<Value> {
    let response_mode = response_mode_from_params(&request.params)?;
    let invocation = parse_tool_invocation(&request.params)?;
    if let ToolInvocation::ValidateWebIr { ir } = &invocation {
        let canonical = canonical_payload_request(request, json!({"ir": ir.clone()}))?;
        let result = crate::protocol::web_ir_validate_result(&canonical)?;
        return serialized_result(&result);
    }
    if let ToolInvocation::InspectWebIr { ir } = &invocation {
        let canonical = canonical_payload_request(request, json!({"ir": ir.clone()}))?;
        let result = crate::protocol::web_ir_inspect_result(&canonical)?;
        return serialized_result(&result);
    }
    if let ToolInvocation::DiffWebIr { before, after } = &invocation {
        let canonical = canonical_payload_request(
            request,
            json!({"before": before.clone(), "after": after.clone()}),
        )?;
        let result = crate::protocol::web_ir_diff_result(&canonical)?;
        return serialized_result(&result);
    }
    if let ToolInvocation::PreflightNavigation { url } = &invocation {
        return serialized_result(&policy.preflight_navigation(url));
    }
    if let ToolInvocation::ContinuityWebIr {
        before,
        after,
        entity_id,
    } = &invocation
    {
        let canonical = canonical_payload_request(
            request,
            json!({
                "before": before.clone(),
                "after": after.clone(),
                "entityId": entity_id
            }),
        )?;
        let result = crate::protocol::web_ir_continuity_result(&canonical)?;
        return serialized_result(&result);
    }
    if let ToolInvocation::ValidateTask { task } = &invocation {
        let canonical = canonical_payload_request(request, json!({"task": task.clone()}))?;
        let result = crate::protocol::validate_task_result(&canonical)?;
        return serialized_result(&result);
    }
    if let ToolInvocation::CompileTask { task, ir } = &invocation {
        let canonical = canonical_payload_request(
            request,
            json!({"task": serde_json::to_value(task)?, "ir": serde_json::to_value(ir)?}),
        )?;
        let result = crate::protocol::compile_task_result(&canonical)?;
        return serialized_result(&result);
    }
    if let ToolInvocation::ExecuteTask {
        task,
        expected_revision,
        confirmed,
    } = &invocation
    {
        let canonical = canonical_payload_request(
            request,
            json!({
                "task": serde_json::to_value(task)?,
                "expectedRevision": expected_revision,
                "confirmed": confirmed,
            }),
        )?;
        canonical
            .decode_task_execute()
            .map_err(|error| error.to_string())?;
    }
    if matches!(
        &invocation,
        ToolInvocation::SurfaceInspect { .. }
            | ToolInvocation::BackendInspect { .. }
            | ToolInvocation::WorkspaceStatus
            | ToolInvocation::WorkspaceInspect { .. }
            | ToolInvocation::ReplayInspect { .. }
            | ToolInvocation::ReplayDiff { .. }
            | ToolInvocation::ReplayAttach { .. }
    ) {
        return call_experience_tool(invocation);
    }
    if matches!(
        &invocation,
        ToolInvocation::KnowledgeList
            | ToolInvocation::KnowledgeShow { .. }
            | ToolInvocation::KnowledgeStats
            | ToolInvocation::KnowledgeInvalidate { .. }
            | ToolInvocation::KnowledgePurge { .. }
            | ToolInvocation::KnowledgeForget { .. }
            | ToolInvocation::KnowledgeExport
            | ToolInvocation::KnowledgePrune
            | ToolInvocation::KnowledgeReindex
    ) {
        policy.require(crate::browser::policy::PolicyCapability::PersistentProfile)?;
        return call_knowledge_tool(invocation, options, knowledge_store_path);
    }
    if matches!(
        &invocation,
        ToolInvocation::ResolveIntentWithKnowledge { .. }
            | ToolInvocation::ObserveKnowledge {
                fresh_only: false,
                ..
            }
    ) {
        policy.require(crate::browser::policy::PolicyCapability::PersistentProfile)?;
    }
    if let ToolInvocation::SessionSnapshot {
        operation,
        from,
        to,
    } = &invocation
        && operation != "create"
    {
        return browser_free_session_snapshot(
            operation,
            from.as_deref(),
            to.as_deref(),
            &options.profile,
            response_mode,
        );
    }
    if let ToolInvocation::RecoverRun { execution_id } = &invocation {
        return serialized_result(&recover_run(execution_id)?);
    }
    if native_runtime {
        #[cfg(feature = "native-engine")]
        {
            if let Some(name) = _persistent_native_session {
                let navigation_timeout_ms = match &invocation {
                    ToolInvocation::Navigate { timeout_ms, .. } => Some(*timeout_ms),
                    _ => None,
                };
                let operation = crate::browser::persistent::execute_native_mcp_with_dialog_host(
                    name,
                    request.params.clone(),
                );
                return run_native_persistent_mcp_dialog_host(
                    name,
                    elicitation,
                    operation,
                    navigation_timeout_ms,
                )
                .await;
            }
            return Box::pin(call_native_tool(
                invocation,
                _native_session,
                viewport,
                &options.profile,
                options.incognito,
                policy,
                response_mode,
                elicitation,
            ))
            .await;
        }
        #[cfg(not(feature = "native-engine"))]
        {
            return Err("native runtime is unavailable in this build".into());
        }
    }
    let session = ensure_session(session, options, policy, viewport).await?;

    if let ToolInvocation::ExecuteTask {
        ref task,
        expected_revision,
        confirmed,
    } = invocation
    {
        let result = session
            .execute_task(task, expected_revision, confirmed)
            .await?;
        return serialized_result_mode(&result, response_mode);
    }
    match invocation {
        ToolInvocation::PreflightNavigation { url } => {
            unreachable!("preflightNavigation is handled before browser session startup: {url}")
        }
        ToolInvocation::Navigate {
            url,
            timeout_ms,
            expected_revision,
        } => {
            if let Some(expected_revision) = expected_revision {
                serialized_result(
                    &session
                        .navigate_with_revision(
                            url,
                            Duration::from_millis(timeout_ms),
                            expected_revision,
                        )
                        .await?,
                )
            } else {
                let page = session
                    .navigate_with_deadline(url, Duration::from_millis(timeout_ms))
                    .await?;
                serialized_result(&page)
            }
        }
        ToolInvocation::Click {
            target,
            expected_revision,
        } => {
            if let Some(expected_revision) = expected_revision {
                action_result(
                    session
                        .click_with_revision(target.as_ref(), expected_revision)
                        .await?,
                )
            } else {
                action_result(session.click(target.as_ref()).await?)
            }
        }
        ToolInvocation::Preflight { target, action } => {
            serialized_result(&session.preflight_with_action(target.as_ref(), action).await)
        }
        ToolInvocation::ClickAt { x, y } => serialized_result(&session.click_at(x, y).await?),
        ToolInvocation::ClickExpectPopup {
            target,
            expected_revision,
        } => serialized_result(
            &session
                .click_expect_popup_with_revision(target.as_ref(), expected_revision)
                .await?,
        ),
        ToolInvocation::DoubleClick {
            target,
            expected_revision,
        } => action_result(
            session
                .double_click_with_revision(target.as_ref(), expected_revision)
                .await?,
        ),
        ToolInvocation::Hover {
            target,
            expected_revision,
        } => action_result(
            session
                .hover_with_revision(target.as_ref(), expected_revision)
                .await?,
        ),
        ToolInvocation::Drag {
            source,
            destination,
            expected_revision,
        } => action_result(
            session
                .drag_with_revision(source.as_ref(), destination.as_ref(), expected_revision)
                .await?,
        ),
        ToolInvocation::Type {
            text,
            target,
            expected_revision,
        } => action_result(
            session
                .type_text_with_expected_revision(text, target, expected_revision)
                .await?,
        ),
        ToolInvocation::Key {
            key,
            expected_revision,
        } => action_result(
            session
                .key_press_with_revision(key, expected_revision)
                .await?,
        ),
        ToolInvocation::KeyDown {
            key,
            expected_revision,
        } => action_result(
            session
                .key_down_with_revision(key, expected_revision)
                .await?,
        ),
        ToolInvocation::KeyUp {
            key,
            expected_revision,
        } => action_result(session.key_up_with_revision(key, expected_revision).await?),
        ToolInvocation::Shortcut {
            shortcut,
            expected_revision,
        } => action_result(
            session
                .shortcut_with_revision(shortcut, expected_revision)
                .await?,
        ),
        ToolInvocation::Clear {
            target,
            expected_revision,
        } => action_result(
            session
                .clear_with_revision(target.as_ref(), expected_revision)
                .await?,
        ),
        ToolInvocation::Check {
            target,
            expected_revision,
        } => action_result(
            session
                .check_with_revision(target.as_ref(), expected_revision)
                .await?,
        ),
        ToolInvocation::Uncheck {
            target,
            expected_revision,
        } => action_result(
            session
                .uncheck_with_revision(target.as_ref(), expected_revision)
                .await?,
        ),
        ToolInvocation::Select {
            target,
            value,
            expected_revision,
        } => action_result(
            session
                .select_option_with_revision(target.as_ref(), value, expected_revision)
                .await?,
        ),
        ToolInvocation::Upload {
            target,
            files,
            expected_revision,
        } => action_result(
            session
                .upload_files_with_revision(target.as_ref(), &files, expected_revision)
                .await?,
        ),
        ToolInvocation::Screenshot {
            format,
            quality,
            scale,
            full_page,
            clip,
            target,
        } => {
            let capture = session
                .capture_visual(&VisualCaptureOptions {
                    format,
                    quality,
                    scale,
                    clip,
                    full_page,
                    target,
                })
                .await?;
            Ok(json!({
                "content": [{"type":"text", "text": serde_json::to_string(&capture.metadata)?}, {
                    "type": "image",
                    "data": capture.data,
                    "mimeType": format!("image/{}", format.as_cdp())
                }]
            }))
        }
        ToolInvocation::Observe {
            include_dom,
            include_screenshot,
            include_form_values,
            level,
            region,
        } => {
            if let Some(level) = level {
                if include_dom || include_screenshot || include_form_values {
                    return Err(
                        "semantic observation cannot be combined with DOM, screenshot, or form values"
                            .into(),
                    );
                }
                if let Some(region_id) = region {
                    let page = session.semantic_observe(level).await?;
                    return serialized_result(
                        &session
                            .semantic_expand_region(region_id, page.revision, level)
                            .await?,
                    );
                }
                return serialized_result(&session.semantic_observe(level).await?);
            }
            if region.is_some() {
                return Err("semantic region expansion requires an explicit level".into());
            }
            let mut context = match (include_dom, include_screenshot, include_form_values) {
                (false, false, false) => session.observe().await?,
                (true, false, false) => session.observe_with_dom().await?,
                (false, true, false) => session.observe_with_screenshot().await?,
                (true, true, false) => session.observe_with_dom_and_screenshot().await?,
                (false, false, true) => session.observe_with_form_values().await?,
                _ => {
                    return Err(
                        "form values may only be combined with default compact observe".into(),
                    );
                }
            };
            let screenshot = context.screenshot.take();
            let context_json = serde_json::to_string(&context)?;
            let context_bytes = context_json.len();
            let mut content = vec![json!({"type": "text", "text": context_json})];
            if let Some(data) = screenshot {
                content.push(json!({
                    "type": "image",
                    "data": data,
                    "mimeType": "image/png"
                }));
            }
            Ok(json!({
                "content": content,
                "_meta": {"contextCost": {
                    "payloadBytes": context_bytes,
                    "estimatedTokens": context_bytes.div_ceil(4)
                }}
            }))
        }
        ToolInvocation::ObserveBootstrap => {
            serialized_result_mode(&session.observe_bootstrap().await?, response_mode)
        }
        ToolInvocation::InspectPage => {
            serialized_result_mode(&session.inspect_page().await?, response_mode)
        }
        ToolInvocation::ExtractWebIr { request } => {
            serialized_result_mode(&session.extract_web_ir(&request).await?, response_mode)
        }
        ToolInvocation::FindTarget { request } => {
            serialized_result_mode(&session.find_target(&request).await?, response_mode)
        }
        ToolInvocation::ActAndVerify {
            request,
            predicate,
            timeout,
        } => serialized_result_mode(
            &session.act_and_verify(&request, predicate, timeout).await?,
            response_mode,
        ),
        ToolInvocation::ExtractStructured { request } => {
            serialized_result_mode(&session.extract_structured(&request).await?, response_mode)
        }
        ToolInvocation::RecoverRun { execution_id } => {
            serialized_result_mode(&session.recover_run(execution_id)?, response_mode)
        }
        ToolInvocation::SessionSnapshot {
            operation,
            from,
            to,
        } => {
            let store = SessionSnapshotStore::new(default_session_snapshot_path(&options.profile));
            match operation.as_ref() {
                "create" => {
                    let observation = session
                        .semantic_observe(SemanticObservationLevel::Structured)
                        .await?;
                    let snapshot = crate::browser::session::SessionSnapshot::from_observation(
                        options.profile.clone(),
                        observation,
                    );
                    store.save(&snapshot)?;
                    serialized_result_mode(&snapshot, response_mode)
                }
                "list" => serialized_result_mode(&store.list()?, response_mode),
                "inspect" => {
                    let id = from
                        .as_deref()
                        .ok_or("sessionSnapshot inspect requires from")?;
                    serialized_result_mode(&store.load(id)?, response_mode)
                }
                "diff" => {
                    let left = from
                        .as_deref()
                        .ok_or("sessionSnapshot diff requires from")?;
                    let right = to.as_deref().ok_or("sessionSnapshot diff requires to")?;
                    serialized_result_mode(&store.diff(left, right)?, response_mode)
                }
                "purge" => {
                    serialized_result_mode(&json!({"removed": store.purge()?}), response_mode)
                }
                _ => Err(
                    "sessionSnapshot operation must be create, list, inspect, diff, or purge"
                        .into(),
                ),
            }
        }
        ToolInvocation::ObserveKnowledge {
            level,
            fresh_only,
            mut lookup,
        } => {
            if fresh_only {
                let observation = session.semantic_observe(level).await?;
                return serialized_result(&KnowledgeObservationReport {
                    observation,
                    mode: KnowledgeObservationMode::FreshOnly,
                    assessments: Vec::new(),
                    eligible_record_ids: Vec::new(),
                    stale_record_ids: Vec::new(),
                    out_of_scope_record_ids: Vec::new(),
                });
            }
            let path = knowledge_store_path
                .map(Path::to_path_buf)
                .unwrap_or_else(|| default_knowledge_store_path(&options.profile));
            let store = KnowledgeStore::open(path)?;
            if lookup.profile_scope == KnowledgeProfileScope::ProfileBound
                && lookup.profile_key.is_none()
            {
                lookup.profile_key = Some(options.profile.clone());
            }
            lookup.policy_preset = serde_json::to_string(&policy.preset())?
                .trim_matches('"')
                .to_string();
            lookup.now_epoch_seconds = chrono::Utc::now().timestamp();
            serialized_result(
                &session
                    .semantic_observe_with_knowledge(level, &store, lookup, false)
                    .await?,
            )
        }
        ToolInvocation::ResolveIntent { request } => {
            serialized_result(&session.resolve_intent(&request).await?)
        }
        ToolInvocation::ResolveIntentWithKnowledge {
            request,
            mut lookup,
        } => {
            let path = knowledge_store_path
                .map(Path::to_path_buf)
                .unwrap_or_else(|| default_knowledge_store_path(&options.profile));
            let store = KnowledgeStore::open(path)?;
            if lookup.profile_scope == KnowledgeProfileScope::ProfileBound
                && lookup.profile_key.is_none()
            {
                lookup.profile_key = Some(options.profile.clone());
            }
            lookup.policy_preset = serde_json::to_string(&policy.preset())?
                .trim_matches('"')
                .to_string();
            lookup.now_epoch_seconds = chrono::Utc::now().timestamp();
            serialized_result(
                &session
                    .resolve_intent_with_knowledge(&request, &store, lookup)
                    .await?,
            )
        }
        ToolInvocation::ExecuteIntent { request } => {
            serialized_result(&session.execute_intent(&request).await?)
        }
        ToolInvocation::KnowledgeList
        | ToolInvocation::KnowledgeShow { .. }
        | ToolInvocation::KnowledgeStats
        | ToolInvocation::KnowledgeInvalidate { .. }
        | ToolInvocation::KnowledgePurge { .. } => {
            unreachable!("knowledge tools are dispatched before browser startup")
        }
        ToolInvocation::GetDom => serialized_result(&session.deep_dom().await?),
        ToolInvocation::GetText => Ok(text_result(session.text().await?)),
        ToolInvocation::Evaluate { expression } => {
            serialized_result(&session.evaluate(expression).await?)
        }
        ToolInvocation::Batch {
            steps,
            atomic,
            mode,
            expected_revision,
        } => {
            let parsed: Vec<BatchStep> = serde_json::from_value(steps.clone())
                .map_err(|e| format!("invalid batch steps: {e}"))?;
            serialized_result(
                &session
                    .run_batch_with_mode(&parsed, atomic, mode, expected_revision)
                    .await?,
            )
        }
        ToolInvocation::Workflow {
            definition,
            inputs,
            checkpoint,
        } => {
            let workflow = crate::browser::session::WorkflowDefinition::from_value(definition)
                .map_err(|error| format!("invalid workflow: {error}"))?;
            let inputs: BTreeMap<String, Value> = serde_json::from_value(inputs)
                .map_err(|error| format!("invalid workflow inputs: {error}"))?;
            let result = match checkpoint {
                Some(checkpoint) => {
                    let checkpoint = serde_json::from_value(checkpoint)
                        .map_err(|error| format!("invalid workflow checkpoint: {error}"))?;
                    session
                        .resume_workflow(&workflow, &inputs, &checkpoint)
                        .await?
                }
                None => session.run_workflow(&workflow, &inputs).await?,
            };
            serialized_result(&result)
        }
        ToolInvocation::Verify {
            predicate,
            timeout_ms,
        } => {
            let predicate: VerificationPredicate = serde_json::from_value(predicate)
                .map_err(|error| format!("invalid verification predicate: {error}"))?;
            serialized_result(
                &session
                    .verify(predicate, Duration::from_millis(timeout_ms))
                    .await?,
            )
        }
        ToolInvocation::ReconcileReferences {
            from_revision,
            refs,
            hints,
            scope_ref,
        } => {
            let options = ReconciliationOptions {
                hints: hints
                    .iter()
                    .map(|hint| Locator::parse(hint))
                    .collect::<BrowserResult<Vec<_>>>()?,
                scope_ref,
            };
            serialized_result(
                &session
                    .reconcile_references_with_options(from_revision, &refs, &options)
                    .await?,
            )
        }
        ToolInvocation::ObserveDelta => serialized_result(&session.observe_delta().await?),
        ToolInvocation::SetNetworkConditions {
            preset,
            offline,
            latency_ms,
            download_throughput,
            upload_throughput,
        } => {
            let conditions = if let Some(preset) = preset {
                crate::browser::session::NetworkConditions::preset(&preset)?
            } else {
                crate::browser::session::NetworkConditions {
                    offline,
                    latency_ms,
                    download_throughput_bytes: download_throughput,
                    upload_throughput_bytes: upload_throughput,
                    connection_type: None,
                }
            };
            session.set_network_conditions(Some(&conditions)).await?;
            serialized_result(&json!({"ok": true}))
        }
        ToolInvocation::ClearNetworkConditions => {
            session.set_network_conditions(None).await?;
            serialized_result(&json!({"ok": true}))
        }
        ToolInvocation::SetCpuThrottling { rate } => {
            session.set_cpu_throttling(Some(rate)).await?;
            serialized_result(&json!({"ok": true}))
        }
        ToolInvocation::ClearCpuThrottling => {
            session.set_cpu_throttling(None).await?;
            serialized_result(&json!({"ok": true}))
        }
        ToolInvocation::SetUserAgent {
            user_agent,
            accept_language,
            platform,
        } => {
            session
                .set_user_agent(
                    Some(&user_agent),
                    accept_language.as_deref(),
                    platform.as_deref(),
                )
                .await?;
            serialized_result(&json!({"ok": true}))
        }
        ToolInvocation::ClearUserAgent => {
            session.set_user_agent(None, None, None).await?;
            serialized_result(&json!({"ok": true}))
        }
        ToolInvocation::ExportCheckpoint => serialized_result(&session.export_checkpoint().await?),
        ToolInvocation::ImportCheckpoint { checkpoint } => {
            let ckpt: CheckpointV1 = serde_json::from_value(checkpoint.clone())
                .map_err(|e| format!("invalid checkpoint: {e}"))?;
            session.import_checkpoint(&ckpt).await?;
            serialized_result(&json!({"status": "checkpoint_imported"}))
        }
        ToolInvocation::Scroll {
            dx,
            dy,
            expected_revision,
        } => action_result(
            session
                .scroll_with_revision(dx, dy, expected_revision)
                .await?,
        ),
        ToolInvocation::Wait {
            condition,
            timeout_ms,
        } => Ok(text_result(serde_json::to_string(
            &session
                .wait(
                    WaitCondition::parse(condition)?,
                    Duration::from_millis(timeout_ms),
                )
                .await?,
        )?)),
        ToolInvocation::Diagnostics { duration_ms } => serialized_result(
            &session
                .diagnostics(Duration::from_millis(duration_ms))
                .await?,
        ),
        ToolInvocation::AcceptDialog => {
            session.accept_dialog().await?;
            serialized_result(&json!({"dialog": "accepted"}))
        }
        ToolInvocation::DismissDialog => {
            session.dismiss_dialog().await?;
            serialized_result(&json!({"dialog": "dismissed"}))
        }
        ToolInvocation::DismissConsent => serialized_result(&session.dismiss_consent().await?),
        ToolInvocation::Download {
            destination,
            timeout_ms,
        } => serialized_result(
            &session
                .wait_for_download(&destination, Duration::from_millis(timeout_ms))
                .await?,
        ),
        ToolInvocation::ListTargets => serialized_result(&session.list_targets().await?),
        ToolInvocation::CreateTarget { url } => {
            serialized_result(&session.create_target(url).await?)
        }
        ToolInvocation::SelectTarget { id } => serialized_result(&session.select_target(id).await?),
        ToolInvocation::CloseTarget { id } => {
            session.close_target(id).await?;
            serialized_result(&json!({"closed": id}))
        }
        ToolInvocation::ListFrames => serialized_result(&session.list_frames().await?),
        ToolInvocation::SelectFrame { id } => serialized_result(&session.select_frame(id).await?),
        ToolInvocation::Cookies => serialized_result(&session.cookies().await?),
        ToolInvocation::SetCookies { cookies } => {
            let parsed: Vec<crate::browser::session::storage::Cookie> =
                serde_json::from_value(cookies.clone())
                    .map_err(|e| format!("invalid cookies: {e}"))?;
            session.set_cookies(&parsed).await?;
            serialized_result(&json!({"ok": true}))
        }
        ToolInvocation::ClearCookies => {
            session.clear_cookies().await?;
            serialized_result(&json!({"ok": true}))
        }
        ToolInvocation::LocalStorage => serialized_result(&session.local_storage().await?),
        ToolInvocation::SessionStorage => serialized_result(&session.session_storage().await?),
        ToolInvocation::PrintToPdf { options } => {
            let opts: crate::browser::session::PdfOptions = serde_json::from_value(options)
                .map_err(|error| format!("invalid PDF options: {error}"))?;
            serialized_result(&session.print_to_pdf(&opts).await?)
        }
        ToolInvocation::FillForm {
            fields,
            expected_revision,
        } => {
            let refs: Vec<(&str, &str)> = fields
                .iter()
                .map(|(t, v)| (t.as_str(), v.as_str()))
                .collect();
            serialized_result(
                &session
                    .fill_form_with_expected_revision(&refs, expected_revision)
                    .await?,
            )
        }
        ToolInvocation::ClipboardRead => {
            let text = session.clipboard_read().await?;
            serialized_result(&serde_json::json!({"text": text}))
        }
        ToolInvocation::ClipboardWrite { text } => {
            session.clipboard_write(&text).await?;
            serialized_result(&serde_json::json!({"ok": true}))
        }
        ToolInvocation::SetGeolocation {
            latitude,
            longitude,
        } => {
            let loc = crate::browser::session::GeoLocation {
                latitude,
                longitude,
                accuracy: None,
            };
            session.set_geolocation(Some(&loc)).await?;
            serialized_result(&serde_json::json!({"ok": true}))
        }
        ToolInvocation::ClearGeolocation => {
            session.set_geolocation(None).await?;
            serialized_result(&serde_json::json!({"ok": true}))
        }
        ToolInvocation::SetTimezone { timezone_id } => {
            session.set_timezone(Some(&timezone_id)).await?;
            serialized_result(&serde_json::json!({"ok": true}))
        }
        ToolInvocation::InspectWebIr { .. } => {
            unreachable!("inspectWebIr is handled before browser session startup")
        }
        ToolInvocation::ValidateWebIr { .. } => {
            unreachable!("validateWebIr is handled before browser session startup")
        }
        ToolInvocation::DiffWebIr { .. } => {
            unreachable!("diffWebIr is handled before browser session startup")
        }
        ToolInvocation::ContinuityWebIr { .. } => {
            unreachable!("continuityWebIr is handled before browser session startup")
        }
        ToolInvocation::CompileTask { .. } => {
            unreachable!("compileTask is handled before browser session startup")
        }
        ToolInvocation::ExecuteTask { .. } => {
            unreachable!("executeTask is handled before browser operation dispatch")
        }
        ToolInvocation::ValidateTask { .. } => {
            unreachable!("validateTask is handled before browser session startup")
        }
        ToolInvocation::KnowledgeForget { .. }
        | ToolInvocation::KnowledgeExport
        | ToolInvocation::KnowledgePrune
        | ToolInvocation::KnowledgeReindex
        | ToolInvocation::SurfaceInspect { .. }
        | ToolInvocation::BackendInspect { .. }
        | ToolInvocation::WorkspaceStatus
        | ToolInvocation::WorkspaceInspect { .. }
        | ToolInvocation::ReplayInspect { .. }
        | ToolInvocation::ReplayDiff { .. }
        | ToolInvocation::ReplayAttach { .. } => {
            unreachable!("experience tools are handled before browser session startup")
        }
    }
}

fn call_knowledge_tool(
    invocation: ToolInvocation<'_>,
    options: &SessionOptions,
    explicit_path: Option<&Path>,
) -> BrowserResult<Value> {
    let path = explicit_path
        .map(Path::to_path_buf)
        .unwrap_or_else(|| default_knowledge_store_path(&options.profile));
    let mut store = KnowledgeStore::open(path)?;
    match invocation {
        ToolInvocation::KnowledgeList => serialized_result(store.snapshot()),
        ToolInvocation::KnowledgeShow { record_id } => {
            let record = store
                .get(record_id)
                .ok_or_else(|| format!("knowledge record not found: {record_id}"))?;
            serialized_result(record)
        }
        ToolInvocation::KnowledgeStats => serialized_result(&store.stats()?),
        ToolInvocation::KnowledgeInvalidate {
            record_id,
            state,
            reason,
            observed_at,
        } => {
            let next = match state {
                "stale" => KnowledgeConfidence::Stale,
                "contradicted" => KnowledgeConfidence::Contradicted,
                "quarantined" => KnowledgeConfidence::Quarantined,
                _ => return Err("state must be stale, contradicted, or quarantined".into()),
            };
            let change = store.transition(
                record_id,
                next,
                reason.unwrap_or("caller invalidated record").to_string(),
                observed_at
                    .map(str::to_string)
                    .unwrap_or_else(|| chrono::Utc::now().to_rfc3339()),
                false,
            )?;
            serialized_result(&change)
        }
        ToolInvocation::KnowledgePurge { origin } => {
            serialized_result(&store.purge_origin(origin)?)
        }
        ToolInvocation::KnowledgeForget { record_id } => {
            serialized_result(&store.remove(record_id)?)
        }
        ToolInvocation::KnowledgeExport => serialized_result(store.snapshot()),
        ToolInvocation::KnowledgePrune => {
            let ids = store
                .records()
                .iter()
                .filter(|record| {
                    matches!(
                        record.confidence,
                        KnowledgeConfidence::Stale
                            | KnowledgeConfidence::Contradicted
                            | KnowledgeConfidence::Quarantined
                    )
                })
                .map(|record| record.record_id.clone())
                .collect::<Vec<_>>();
            let mut removed = Vec::new();
            for id in ids {
                if store.remove(&id)?.removed {
                    removed.push(id);
                }
            }
            serialized_result(&json!({"removedRecordIds": removed}))
        }
        ToolInvocation::KnowledgeReindex => {
            store.refresh()?;
            serialized_result(&store.stats()?)
        }
        _ => unreachable!("non-knowledge tool passed to knowledge dispatcher"),
    }
}
fn call_experience_tool(invocation: ToolInvocation<'_>) -> BrowserResult<Value> {
    let result = match invocation {
        ToolInvocation::SurfaceInspect {
            surfaces,
            coverage_only,
        } => {
            let set: SurfaceSet = serde_json::from_value(surfaces)?;
            set.validate()?;
            if coverage_only {
                json!({
                    "surfaceCount": set.surfaces.len(),
                    "surfaces": set.surfaces.iter().map(|surface| json!({
                        "surfaceId": surface.surface_id,
                        "kind": surface.kind,
                        "understanding": surface.understanding,
                        "coverage": surface.coverage,
                        "capabilities": surface.capabilities,
                        "evidenceCount": surface.evidence.len(),
                        "provenance": surface.evidence.iter().map(|e| &e.provenance).collect::<Vec<_>>(),
                    })).collect::<Vec<_>>()
                })
            } else {
                serde_json::to_value(set)?
            }
        }
        ToolInvocation::BackendInspect { profile, test } => {
            let profile: BackendProfile = serde_json::from_value(profile)?;
            profile.validate()?;
            if test {
                json!({
                    "valid": true,
                    "gates": BrowserCapability::ALL.iter().map(|capability| json!({
                        "capability": capability,
                        "declared": profile.capabilities.contains_key(capability),
                        "level": profile.capability(*capability).level,
                        "portability": profile.capability(*capability).portability,
                    })).collect::<Vec<_>>()
                })
            } else {
                json!({"identity": profile.identity, "declaredCapabilities": profile.capabilities.len()})
            }
        }
        ToolInvocation::WorkspaceStatus => {
            let store = WorkspaceStore::open_default()?;
            json!({"workspaces": store.list()?})
        }
        ToolInvocation::WorkspaceInspect { id } => {
            let store = WorkspaceStore::open_default()?;
            match id {
                Some(id) => {
                    let id = WorkspaceId::new(id)?;
                    serde_json::to_value(store.open(&id)?)?
                }
                None => json!({"workspaces": store.list()?}),
            }
        }
        ToolInvocation::ReplayInspect { scenario, replay } => {
            let scenario = ReliabilityScenario::from_value(scenario)?;
            let replay = ReliabilityReplayBundle::from_value(replay, &scenario)?;
            json!({
                "scenarioId": replay.scenario_id,
                "fixtureId": replay.fixture_id,
                "eventCount": replay.events.len(),
                "contentHash": replay.content_hash(&scenario)?,
                "attached": false,
            })
        }
        ToolInvocation::ReplayDiff {
            scenario,
            before,
            after,
        } => {
            let scenario = ReliabilityScenario::from_value(scenario)?;
            let before = ReliabilityReplayBundle::from_value(before, &scenario)?;
            let after = ReliabilityReplayBundle::from_value(after, &scenario)?;
            serde_json::to_value(before.compare(&after, &scenario)?)?
        }
        ToolInvocation::ReplayAttach { scenario, replay } => {
            let scenario = ReliabilityScenario::from_value(scenario)?;
            let replay = ReliabilityReplayBundle::from_value(replay, &scenario)?;
            json!({
                "scenarioId": replay.scenario_id,
                "fixtureId": replay.fixture_id,
                "eventCount": replay.events.len(),
                "contentHash": replay.content_hash(&scenario)?,
                "attached": true,
                "sideEffects": false,
            })
        }
        _ => return Err("non-experience invocation".into()),
    };
    let envelope =
        ExperienceResult::new("experience", "ok", result).with_provenance(ExperienceProvenance {
            source: ProvenanceSource::Mcp,
            authoritative: false,
            resource_ref: None,
            revision: None,
            observed_at: Some(chrono::Utc::now().to_rfc3339()),
        });
    envelope.validate()?;
    serialized_result(&envelope)
}
fn response_mode_from_params(params: &Value) -> BrowserResult<ResponseMode> {
    let mode = params
        .get("arguments")
        .and_then(Value::as_object)
        .and_then(|arguments| arguments.get("responseMode"))
        .and_then(Value::as_str)
        .unwrap_or("minimal");
    match mode {
        "minimal" => Ok(ResponseMode::Minimal),
        "normal" => Ok(ResponseMode::Normal),
        "diagnostic" => Ok(ResponseMode::Diagnostic),
        _ => Err("responseMode must be minimal, normal, or diagnostic".into()),
    }
}

fn parse_tool_invocation(params: &Value) -> BrowserResult<ToolInvocation<'_>> {
    let tool_name = required_string(params, "name")?;
    let arguments = &params["arguments"];
    if !arguments.is_null() && !arguments.is_object() {
        return Err("tools/call arguments must be an object".into());
    }

    match tool_name {
        "preflightNavigation" => {
            let url = required_string(arguments, "url")?;
            if url.len() > MAX_PREFLIGHT_URL_BYTES {
                return Err("url must be at most 8192 bytes".into());
            }
            if arguments
                .as_object()
                .is_some_and(|object| object.keys().any(|key| key != "url"))
            {
                return Err("preflightNavigation accepts only the url argument".into());
            }
            Ok(ToolInvocation::PreflightNavigation { url })
        }
        "navigate" => Ok(ToolInvocation::Navigate {
            url: required_string(arguments, "url")?,
            timeout_ms: optional_u64(arguments, "timeoutMs", 20_000)?,
            expected_revision: optional_u64_value(arguments, "expectedRevision")?,
        }),
        "click" => Ok(ToolInvocation::Click {
            target: required_target(arguments)?,
            expected_revision: optional_u64_value(arguments, "expectedRevision")?,
        }),
        "preflight" => Ok(ToolInvocation::Preflight {
            target: required_target(arguments)?,
            action: match optional_string(arguments, "action")?.unwrap_or("click") {
                "click" => PreflightAction::Click,
                "hover" => PreflightAction::Hover,
                "type" => PreflightAction::Type,
                "check" => PreflightAction::Check,
                "select" => PreflightAction::Select,
                _ => return Err("action must be click, hover, type, check, or select".into()),
            },
        }),
        "clickAt" => Ok(ToolInvocation::ClickAt {
            x: required_number(arguments, "x")?,
            y: required_number(arguments, "y")?,
        }),
        "clickExpectPopup" => Ok(ToolInvocation::ClickExpectPopup {
            target: required_target(arguments)?,
            expected_revision: optional_u64_value(arguments, "expectedRevision")?,
        }),
        "doubleClick" => Ok(ToolInvocation::DoubleClick {
            target: required_target(arguments)?,
            expected_revision: optional_u64_value(arguments, "expectedRevision")?,
        }),
        "hover" => Ok(ToolInvocation::Hover {
            target: required_target(arguments)?,
            expected_revision: optional_u64_value(arguments, "expectedRevision")?,
        }),
        "drag" => Ok(ToolInvocation::Drag {
            source: Cow::Borrowed(required_string(arguments, "source")?),
            destination: Cow::Borrowed(required_string(arguments, "destination")?),
            expected_revision: optional_u64_value(arguments, "expectedRevision")?,
        }),
        "type" => Ok(ToolInvocation::Type {
            text: required_string(arguments, "text")?,
            target: optional_string(arguments, "target")?,
            expected_revision: optional_u64_value(arguments, "expectedRevision")?,
        }),
        "key" => Ok(ToolInvocation::Key {
            key: required_string(arguments, "key")?,
            expected_revision: optional_u64_value(arguments, "expectedRevision")?,
        }),
        "keyDown" => Ok(ToolInvocation::KeyDown {
            key: required_string(arguments, "key")?,
            expected_revision: optional_u64_value(arguments, "expectedRevision")?,
        }),
        "keyUp" => Ok(ToolInvocation::KeyUp {
            key: required_string(arguments, "key")?,
            expected_revision: optional_u64_value(arguments, "expectedRevision")?,
        }),
        "shortcut" => Ok(ToolInvocation::Shortcut {
            shortcut: required_string(arguments, "shortcut")?,
            expected_revision: optional_u64_value(arguments, "expectedRevision")?,
        }),
        "clear" => Ok(ToolInvocation::Clear {
            target: required_target(arguments)?,
            expected_revision: optional_u64_value(arguments, "expectedRevision")?,
        }),
        "check" => Ok(ToolInvocation::Check {
            target: required_target(arguments)?,
            expected_revision: optional_u64_value(arguments, "expectedRevision")?,
        }),
        "uncheck" => Ok(ToolInvocation::Uncheck {
            target: required_target(arguments)?,
            expected_revision: optional_u64_value(arguments, "expectedRevision")?,
        }),
        "select" => Ok(ToolInvocation::Select {
            target: required_target(arguments)?,
            value: required_string(arguments, "value")?,
            expected_revision: optional_u64_value(arguments, "expectedRevision")?,
        }),
        "upload" => Ok(ToolInvocation::Upload {
            target: required_target(arguments)?,
            files: required_path_array(arguments, "files")?,
            expected_revision: optional_u64_value(arguments, "expectedRevision")?,
        }),
        "screenshot" => Ok(ToolInvocation::Screenshot {
            format: parse_visual_format(optional_string(arguments, "format")?.unwrap_or("png"))?,
            quality: optional_u64_value(arguments, "quality")?
                .map(|value| u8::try_from(value).map_err(|_| "quality must be 0..=100"))
                .transpose()?,
            scale: optional_number(arguments, "scale", 1.0)?,
            full_page: optional_bool(arguments, "fullPage")?,
            clip: optional_visual_clip(arguments)?,
            target: optional_string(arguments, "target")?.map(str::to_string),
        }),
        "observe" => Ok(ToolInvocation::Observe {
            include_dom: optional_bool(arguments, "includeDom")?,
            include_screenshot: optional_bool(arguments, "includeScreenshot")?,
            include_form_values: optional_bool(arguments, "includeFormValues")?,
            level: optional_semantic_level(arguments)?,
            region: optional_string(arguments, "region")?,
        }),
        "observeBootstrap" => Ok(ToolInvocation::ObserveBootstrap),
        "inspectPage" => Ok(ToolInvocation::InspectPage),
        "extractWebIr" => {
            let request = arguments
                .get("request")
                .cloned()
                .map(serde_json::from_value)
                .transpose()?
                .unwrap_or_else(ExtractionRequest::default_document);
            request.validate()?;
            Ok(ToolInvocation::ExtractWebIr { request })
        }
        "inspectWebIr" => {
            let ir = arguments
                .get("ir")
                .cloned()
                .ok_or("inspectWebIr requires a Glass Web IR object")?;
            Ok(ToolInvocation::InspectWebIr { ir })
        }
        "validateWebIr" => {
            let ir = arguments
                .get("ir")
                .cloned()
                .ok_or("validateWebIr requires a Glass Web IR object")?;
            Ok(ToolInvocation::ValidateWebIr { ir })
        }
        "diffWebIr" => {
            let before = arguments
                .get("before")
                .cloned()
                .ok_or("diffWebIr requires a before Web IR object")?;
            let after = arguments
                .get("after")
                .cloned()
                .ok_or("diffWebIr requires an after Web IR object")?;
            Ok(ToolInvocation::DiffWebIr { before, after })
        }
        "continuityWebIr" => {
            let before = arguments
                .get("before")
                .cloned()
                .ok_or("continuityWebIr requires a before Web IR object")?;
            let after = arguments
                .get("after")
                .cloned()
                .ok_or("continuityWebIr requires an after Web IR object")?;
            let entity_id = required_string(arguments, "entityId")?;
            Ok(ToolInvocation::ContinuityWebIr {
                before,
                after,
                entity_id,
            })
        }
        "compileTask" => {
            let task = arguments
                .get("task")
                .cloned()
                .ok_or("compileTask requires a task object")?;
            let ir = arguments
                .get("ir")
                .cloned()
                .ok_or("compileTask requires a Glass Web IR object")?;
            Ok(ToolInvocation::CompileTask {
                task: serde_json::from_value(task)?,
                ir: serde_json::from_value(ir)?,
            })
        }
        "executeTask" => {
            let task = arguments
                .get("task")
                .cloned()
                .ok_or("executeTask requires a task object")?;
            let expected_revision = required_u64(arguments, "expectedRevision")?;
            Ok(ToolInvocation::ExecuteTask {
                task: serde_json::from_value(task)?,
                expected_revision,
                confirmed: arguments
                    .get("confirmed")
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
            })
        }
        "validateTask" => {
            let task = arguments
                .get("task")
                .cloned()
                .ok_or("validateTask requires a task object")?;
            Ok(ToolInvocation::ValidateTask { task })
        }
        "findTarget" => {
            let mut value = arguments.clone();
            value
                .as_object_mut()
                .ok_or("findTarget arguments must be an object")?
                .remove("responseMode");
            Ok(ToolInvocation::FindTarget {
                request: SemanticIntentRequest::from_json(&serde_json::to_string(&value)?)?,
            })
        }
        "actAndVerify" => {
            let mut value = arguments.clone();
            let object = value
                .as_object_mut()
                .ok_or("actAndVerify arguments must be an object")?;
            object.remove("responseMode");
            object.remove("leaseToken");
            let timeout_ms = object
                .remove("timeoutMs")
                .and_then(|value| value.as_u64())
                .unwrap_or(10_000);
            let predicate = object
                .remove("predicate")
                .map(serde_json::from_value)
                .transpose()?;
            Ok(ToolInvocation::ActAndVerify {
                request: serde_json::from_value(value)?,
                predicate,
                timeout: Duration::from_millis(timeout_ms),
            })
        }
        "extractStructured" => Ok(ToolInvocation::ExtractStructured {
            request: serde_json::from_value(arguments.clone())?,
        }),
        "recoverRun" => Ok(ToolInvocation::RecoverRun {
            execution_id: required_string(arguments, "executionId")?,
        }),
        "sessionSnapshot" => Ok(ToolInvocation::SessionSnapshot {
            operation: Cow::Borrowed(optional_string(arguments, "operation")?.unwrap_or("list")),
            from: optional_string(arguments, "from")?.map(Cow::Borrowed),
            to: optional_string(arguments, "to")?.map(Cow::Borrowed),
        }),
        "observeKnowledge" => Ok(ToolInvocation::ObserveKnowledge {
            level: optional_semantic_level(arguments)?.unwrap_or(SemanticObservationLevel::Summary),
            fresh_only: optional_bool(arguments, "freshOnly")?,
            lookup: parse_knowledge_lookup_options(arguments)?,
        }),
        "resolveIntent" => Ok(ToolInvocation::ResolveIntent {
            request: SemanticIntentRequest::from_json(&serde_json::to_string(arguments)?)?,
        }),
        "resolveIntentWithKnowledge" => {
            let mut request = arguments.clone();
            let object = request
                .as_object_mut()
                .ok_or("resolveIntentWithKnowledge arguments must be an object")?;
            for field in [
                "profileScope",
                "profileKey",
                "locale",
                "tenantKey",
                "browserFamily",
                "browserVersion",
            ] {
                object.remove(field);
            }
            let request = SemanticIntentRequest::from_json(&serde_json::to_string(&request)?)?;
            Ok(ToolInvocation::ResolveIntentWithKnowledge {
                request,
                lookup: parse_knowledge_lookup_options(arguments)?,
            })
        }
        "executeIntent" => {
            let candidate_id = required_string(arguments, "candidateId")?.to_string();
            let value = optional_string(arguments, "value")?.map(str::to_string);
            let mut request = arguments.clone();
            let object = request
                .as_object_mut()
                .ok_or("executeIntent arguments must be an object")?;
            object.remove("candidateId");
            object.remove("value");
            object.remove("leaseToken");
            let request = SemanticIntentRequest::from_json(&serde_json::to_string(&request)?)?;
            let request = SemanticIntentExecutionRequest {
                request,
                candidate_id,
                value,
            };
            request.validate()?;
            Ok(ToolInvocation::ExecuteIntent { request })
        }
        "knowledgeList" => Ok(ToolInvocation::KnowledgeList),
        "knowledgeShow" => Ok(ToolInvocation::KnowledgeShow {
            record_id: required_string(arguments, "recordId")?,
        }),
        "knowledgeStats" => Ok(ToolInvocation::KnowledgeStats),
        "knowledgeInvalidate" => Ok(ToolInvocation::KnowledgeInvalidate {
            record_id: required_string(arguments, "recordId")?,
            state: required_string(arguments, "state")?,
            reason: optional_string(arguments, "reason")?,
            observed_at: optional_string(arguments, "observedAt")?,
        }),
        "knowledgePurge" => Ok(ToolInvocation::KnowledgePurge {
            origin: required_string(arguments, "origin")?,
        }),
        "memoryStatus" => Ok(ToolInvocation::KnowledgeStats),
        "memoryInspect" | "memoryExplain" => Ok(ToolInvocation::KnowledgeShow {
            record_id: required_string(arguments, "recordId")?,
        }),
        "memoryForget" => Ok(ToolInvocation::KnowledgeForget {
            record_id: required_string(arguments, "recordId")?,
        }),
        "memoryExport" => Ok(ToolInvocation::KnowledgeExport),
        "memoryPrune" => Ok(ToolInvocation::KnowledgePrune),
        "memoryReindex" => Ok(ToolInvocation::KnowledgeReindex),
        "surfaceInspect" => Ok(ToolInvocation::SurfaceInspect {
            surfaces: arguments
                .get("surfaces")
                .cloned()
                .ok_or("surfaceInspect requires surfaces")?,
            coverage_only: optional_bool(arguments, "coverageOnly")?,
        }),
        "backendStatus" => Ok(ToolInvocation::BackendInspect {
            profile: arguments
                .get("profile")
                .cloned()
                .ok_or("backendStatus requires profile")?,
            test: false,
        }),
        "backendTest" => Ok(ToolInvocation::BackendInspect {
            profile: arguments
                .get("profile")
                .cloned()
                .ok_or("backendTest requires profile")?,
            test: true,
        }),
        "workspaceStatus" => Ok(ToolInvocation::WorkspaceStatus),
        "workspaceInspect" => Ok(ToolInvocation::WorkspaceInspect {
            id: optional_string(arguments, "id")?,
        }),
        "replayInspect" => Ok(ToolInvocation::ReplayInspect {
            scenario: arguments
                .get("scenario")
                .cloned()
                .ok_or("replayInspect requires scenario")?,
            replay: arguments
                .get("replay")
                .cloned()
                .ok_or("replayInspect requires replay")?,
        }),
        "replayDiff" => Ok(ToolInvocation::ReplayDiff {
            scenario: arguments
                .get("scenario")
                .cloned()
                .ok_or("replayDiff requires scenario")?,
            before: arguments
                .get("before")
                .cloned()
                .ok_or("replayDiff requires before")?,
            after: arguments
                .get("after")
                .cloned()
                .ok_or("replayDiff requires after")?,
        }),
        "replayAttach" => Ok(ToolInvocation::ReplayAttach {
            scenario: arguments
                .get("scenario")
                .cloned()
                .ok_or("replayAttach requires scenario")?,
            replay: arguments
                .get("replay")
                .cloned()
                .ok_or("replayAttach requires replay")?,
        }),
        "getDOM" | "dom" => Ok(ToolInvocation::GetDom),
        "getText" | "text" => Ok(ToolInvocation::GetText),
        "evaluate" => Ok(ToolInvocation::Evaluate {
            expression: required_string(arguments, "expression")?,
        }),
        "batch" => Ok(ToolInvocation::Batch {
            steps: arguments["steps"].clone(),
            atomic: optional_bool(arguments, "atomic")?,
            mode: optional_batch_mode(arguments)?,
            expected_revision: optional_u64_value(arguments, "expectedRevision")?,
        }),
        "workflow" => Ok(ToolInvocation::Workflow {
            definition: arguments
                .get("workflow")
                .cloned()
                .ok_or("workflow requires a workflow definition")?,
            inputs: arguments
                .get("inputs")
                .cloned()
                .unwrap_or_else(|| json!({})),
            checkpoint: arguments.get("checkpoint").cloned(),
        }),
        "verify" => Ok(ToolInvocation::Verify {
            predicate: arguments
                .get("predicate")
                .filter(|value| value.is_object())
                .cloned()
                .ok_or("verify requires an object predicate")?,
            timeout_ms: optional_u64(arguments, "timeoutMs", 10_000)?,
        }),
        "reconcileReferences" => Ok(ToolInvocation::ReconcileReferences {
            from_revision: required_u64(arguments, "fromRevision")?,
            refs: required_string_array(arguments, "refs")?
                .into_iter()
                .map(String::from)
                .collect(),
            hints: optional_string_array(arguments, "hints", 8)?,
            scope_ref: arguments
                .get("scopeRef")
                .and_then(Value::as_str)
                .map(String::from),
        }),
        "observeDelta" => Ok(ToolInvocation::ObserveDelta),
        "setNetworkConditions" => Ok(ToolInvocation::SetNetworkConditions {
            preset: optional_string(arguments, "preset")?.map(str::to_string),
            offline: optional_bool(arguments, "offline")?,
            latency_ms: optional_number(arguments, "latencyMs", 0.0)?,
            download_throughput: optional_number(arguments, "downloadThroughput", -1.0)?,
            upload_throughput: optional_number(arguments, "uploadThroughput", -1.0)?,
        }),
        "clearNetworkConditions" => Ok(ToolInvocation::ClearNetworkConditions),
        "setCpuThrottling" => Ok(ToolInvocation::SetCpuThrottling {
            rate: required_number(arguments, "rate")?,
        }),
        "clearCpuThrottling" => Ok(ToolInvocation::ClearCpuThrottling),
        "setUserAgent" => Ok(ToolInvocation::SetUserAgent {
            user_agent: required_string(arguments, "userAgent")?.to_string(),
            accept_language: optional_string(arguments, "acceptLanguage")?.map(str::to_string),
            platform: optional_string(arguments, "platform")?.map(str::to_string),
        }),
        "clearUserAgent" => Ok(ToolInvocation::ClearUserAgent),
        "exportCheckpoint" => Ok(ToolInvocation::ExportCheckpoint),
        "importCheckpoint" => {
            let checkpoint = arguments
                .get("checkpoint")
                .cloned()
                .ok_or("importCheckpoint requires a checkpoint object")?;
            if !checkpoint.is_object() {
                return Err("importCheckpoint checkpoint must be an object".into());
            }
            Ok(ToolInvocation::ImportCheckpoint { checkpoint })
        }
        "scroll" => Ok(ToolInvocation::Scroll {
            dx: optional_number(arguments, "dx", 0.0)?,
            dy: optional_number(arguments, "dy", 600.0)?,
            expected_revision: optional_u64_value(arguments, "expectedRevision")?,
        }),
        "wait" => Ok(ToolInvocation::Wait {
            condition: required_string(arguments, "condition")?,
            timeout_ms: optional_u64(arguments, "timeoutMs", 10_000)?,
        }),
        "diagnostics" => Ok(ToolInvocation::Diagnostics {
            duration_ms: optional_u64(arguments, "durationMs", 1_000)?,
        }),
        "acceptDialog" => Ok(ToolInvocation::AcceptDialog),
        "dismissDialog" => Ok(ToolInvocation::DismissDialog),
        "dismissConsent" => Ok(ToolInvocation::DismissConsent),
        "download" => Ok(ToolInvocation::Download {
            destination: std::path::PathBuf::from(required_string(arguments, "destination")?),
            timeout_ms: optional_u64(arguments, "timeoutMs", 30_000)?,
        }),
        "listTargets" => Ok(ToolInvocation::ListTargets),
        "createTarget" => Ok(ToolInvocation::CreateTarget {
            url: required_string(arguments, "url")?,
        }),
        "selectTarget" => Ok(ToolInvocation::SelectTarget {
            id: required_string(arguments, "id")?,
        }),
        "closeTarget" => Ok(ToolInvocation::CloseTarget {
            id: required_string(arguments, "id")?,
        }),
        "listFrames" => Ok(ToolInvocation::ListFrames),
        "selectFrame" => Ok(ToolInvocation::SelectFrame {
            id: required_string(arguments, "id")?,
        }),
        "cookies" => Ok(ToolInvocation::Cookies),
        "setCookies" => Ok(ToolInvocation::SetCookies {
            cookies: arguments["cookies"].clone(),
        }),
        "clearCookies" => Ok(ToolInvocation::ClearCookies),
        "localStorage" => Ok(ToolInvocation::LocalStorage),
        "sessionStorage" => Ok(ToolInvocation::SessionStorage),
        "printToPdf" => Ok(ToolInvocation::PrintToPdf {
            options: arguments.clone(),
        }),
        "fillForm" => {
            let arr = arguments["fields"]
                .as_array()
                .ok_or("fields must be an array")?;
            let mut fields = Vec::new();
            for entry in arr {
                let target = entry["target"]
                    .as_str()
                    .ok_or("field target required")?
                    .to_string();
                let value = entry["value"].as_str().unwrap_or("").to_string();
                fields.push((target, value));
            }
            Ok(ToolInvocation::FillForm {
                fields,
                expected_revision: optional_u64_value(arguments, "expectedRevision")?,
            })
        }
        "clipboardRead" => Ok(ToolInvocation::ClipboardRead),
        "clipboardWrite" => Ok(ToolInvocation::ClipboardWrite {
            text: required_string(arguments, "text")?.to_string(),
        }),
        "setGeolocation" => Ok(ToolInvocation::SetGeolocation {
            latitude: arguments["latitude"].as_f64().ok_or("latitude required")?,
            longitude: arguments["longitude"]
                .as_f64()
                .ok_or("longitude required")?,
        }),
        "clearGeolocation" => Ok(ToolInvocation::ClearGeolocation),
        "setTimezone" => Ok(ToolInvocation::SetTimezone {
            timezone_id: required_string(arguments, "timezoneId")?.to_string(),
        }),
        _ => Err(format!("unknown tool: {tool_name}").into()),
    }
}

#[cfg(feature = "native-engine")]
async fn call_native_tool(
    invocation: ToolInvocation<'_>,
    native_session: &mut Option<BrowserRuntimeSession>,
    viewport: Option<(i64, i64)>,
    profile: &str,
    incognito: bool,
    policy: &BrowserPolicy,
    response_mode: ResponseMode,
    elicitation: Option<&McpElicitationClient>,
) -> BrowserResult<Value> {
    let modal_dialogs = elicitation.is_some();
    let navigation_timeout_ms = match &invocation {
        ToolInvocation::Navigate { timeout_ms, .. } if modal_dialogs => Some(*timeout_ms),
        _ => None,
    };
    let session =
        ensure_native_session(native_session, viewport, profile, incognito, modal_dialogs).await?;
    let operation = call_native_tool_on_session(
        invocation,
        session,
        profile,
        policy,
        response_mode,
        modal_dialogs,
    );
    let (result, discard_session) = match elicitation {
        Some(elicitation) => {
            let (result, discard_session) = run_native_mcp_dialog_host(
                session.native_dialog_controller()?,
                elicitation,
                operation,
                navigation_timeout_ms,
            )
            .await;
            (result, discard_session)
        }
        None => (operation.await, false),
    };
    if discard_session {
        native_session.take();
        tracing::warn!(
            "discarded native MCP browser session after a cancelled operation failed to drain"
        );
    }
    result
}

#[cfg(feature = "native-engine")]
async fn run_native_mcp_dialog_host<F>(
    controller: NativeDialogController,
    client: &McpElicitationClient,
    operation: F,
    navigation_timeout_ms: Option<u64>,
) -> (BrowserResult<Value>, bool)
where
    F: Future<Output = BrowserResult<Value>>,
{
    let mut remaining = navigation_timeout_ms.map(Duration::from_millis);
    tokio::pin!(operation);
    loop {
        let pending_dialog = match controller.pending_dialog() {
            Ok(pending_dialog) => pending_dialog,
            Err(error) => return (Err(error.into()), true),
        };
        if let Some(pending) = pending_dialog {
            let resolution = match mcp_dialog_resolution(client, &pending).await {
                Ok(resolution) => resolution,
                Err(error) => {
                    if controller
                        .resolve_dialog(
                            &pending.id,
                            NativeDialogResolution {
                                accepted: false,
                                prompt_value: None,
                            },
                        )
                        .is_err()
                    {
                        return (Err(error.into()), true);
                    }
                    let drain_result =
                        tokio::time::timeout(MCP_CANCELLATION_DRAIN_TIMEOUT, &mut operation).await;
                    return (Err(error.into()), drain_result.is_err());
                }
            };
            if let Err(error) = controller.resolve_dialog(&pending.id, resolution) {
                return (Err(error.into()), true);
            }
            continue;
        }

        let active_started = tokio::time::Instant::now();
        match remaining {
            Some(timeout) if timeout.is_zero() => {
                match controller.pending_dialog() {
                    Ok(Some(_)) => continue,
                    Ok(None) => {}
                    Err(error) => return (Err(error.into()), true),
                }
                return (
                    Err(format!(
                        "native navigation exceeded its {}ms deadline",
                        navigation_timeout_ms.unwrap_or_default()
                    )
                    .into()),
                    true,
                );
            }
            Some(timeout) => {
                tokio::select! {
                    result = tokio::time::timeout(timeout, &mut operation) => match result {
                        Ok(result) => return (result, false),
                        Err(_) => match controller.pending_dialog() {
                            Ok(Some(_)) => remaining = Some(Duration::ZERO),
                            Ok(None) => return (
                                Err(format!(
                                    "native navigation exceeded its {}ms deadline",
                                    navigation_timeout_ms.unwrap_or_default()
                                ).into()),
                                true,
                            ),
                            Err(error) => return (Err(error.into()), true),
                        },
                    },
                    () = tokio::time::sleep(Duration::from_millis(50)) => {
                        remaining = Some(timeout.saturating_sub(active_started.elapsed()));
                    }
                }
            }
            None => {
                tokio::select! {
                    result = &mut operation => return (result, false),
                    () = tokio::time::sleep(Duration::from_millis(50)) => {}
                }
            }
        }
    }
}

#[cfg(feature = "native-engine")]
async fn run_native_persistent_mcp_dialog_host<F>(
    session_name: &str,
    client: Option<&McpElicitationClient>,
    operation: F,
    navigation_timeout_ms: Option<u64>,
) -> BrowserResult<Value>
where
    F: Future<Output = BrowserResult<Value>>,
{
    let mut remaining = navigation_timeout_ms.map(Duration::from_millis);
    tokio::pin!(operation);

    loop {
        let status_started = tokio::time::Instant::now();
        let status = crate::browser::persistent::native_mcp_dialog_status(session_name).await?;
        if let Some(pending) = status.pending_dialog {
            let client_supports_form = client.is_some_and(|client| client.supports_form);
            let resolution = match client {
                Some(client)
                    if client.supports_form && !client.parent_cancelled.load(Ordering::Acquire) =>
                {
                    mcp_dialog_resolution(client, &pending).await
                }
                Some(client) if client.parent_cancelled.load(Ordering::Acquire) => {
                    Err("parent MCP browser request was cancelled".into())
                }
                _ => Err(
                    "MCP client did not negotiate form elicitation; native page dialog dismissed"
                        .into(),
                ),
            };
            match resolution {
                Ok(resolution) => {
                    if !resolve_native_persistent_mcp_dialog(session_name, &pending, resolution)
                        .await?
                    {
                        continue;
                    }
                }
                Err(error) => {
                    if !resolve_native_persistent_mcp_dialog(
                        session_name,
                        &pending,
                        NativeDialogResolution {
                            accepted: false,
                            prompt_value: None,
                        },
                    )
                    .await?
                    {
                        continue;
                    }
                    stop_native_persistent_navigation(
                        session_name,
                        status.active_navigation_revision,
                    )
                    .await?;
                    if tokio::time::timeout(MCP_CANCELLATION_DRAIN_TIMEOUT, &mut operation)
                        .await
                        .is_err()
                    {
                        return Err(format!(
                            "persistent native MCP operation did not drain after dialog dismissal: {error}"
                        )
                        .into());
                    }
                    if !client_supports_form {
                        return Err(NativeMcpFormElicitationRequired.into());
                    }
                    return Err(error.into());
                }
            }
            continue;
        }

        if client.is_some_and(|client| client.parent_cancelled.load(Ordering::Acquire)) {
            stop_native_persistent_navigation(session_name, status.active_navigation_revision)
                .await?;
            if tokio::time::timeout(MCP_CANCELLATION_DRAIN_TIMEOUT, &mut operation)
                .await
                .is_err()
            {
                return Err(
                    "persistent native MCP operation did not drain after cancellation".into(),
                );
            }
            return Err("parent MCP browser request was cancelled".into());
        }

        match remaining {
            Some(timeout) => {
                let active_timeout = timeout.saturating_sub(status_started.elapsed());
                if active_timeout.is_zero() {
                    stop_native_persistent_navigation(
                        session_name,
                        status.active_navigation_revision,
                    )
                    .await?;
                    if tokio::time::timeout(MCP_CANCELLATION_DRAIN_TIMEOUT, &mut operation)
                        .await
                        .is_err()
                    {
                        return Err(
                            "persistent native MCP navigation did not drain at its deadline".into(),
                        );
                    }
                    return Err(format!(
                        "persistent native MCP navigation exceeded its {}ms deadline",
                        navigation_timeout_ms.unwrap_or_default()
                    )
                    .into());
                }
                let active_started = tokio::time::Instant::now();
                tokio::select! {
                    result = tokio::time::timeout(active_timeout, &mut operation) => match result {
                        Ok(result) => return result,
                        Err(_) => {
                            stop_native_persistent_navigation(
                                session_name,
                                status.active_navigation_revision,
                            )
                            .await?;
                            if tokio::time::timeout(
                                MCP_CANCELLATION_DRAIN_TIMEOUT,
                                &mut operation,
                            )
                            .await
                            .is_err()
                            {
                                return Err("persistent native MCP navigation did not drain at its deadline".into());
                            }
                            return Err(format!(
                                "persistent native MCP navigation exceeded its {}ms deadline",
                                navigation_timeout_ms.unwrap_or_default()
                            )
                            .into());
                        }
                    },
                    () = wait_for_native_mcp_parent_cancellation(client) => {
                        stop_native_persistent_navigation(
                            session_name,
                            status.active_navigation_revision,
                        )
                        .await?;
                        if tokio::time::timeout(
                            MCP_CANCELLATION_DRAIN_TIMEOUT,
                            &mut operation,
                        )
                        .await
                        .is_err()
                        {
                            return Err("persistent native MCP operation did not drain after cancellation".into());
                        }
                        return Err("parent MCP browser request was cancelled".into());
                    }
                    () = tokio::time::sleep(MCP_PERSISTENT_STATUS_POLL_INTERVAL) => {
                        remaining = Some(
                            active_timeout.saturating_sub(active_started.elapsed()),
                        );
                    }
                }
            }
            None => {
                tokio::select! {
                    result = &mut operation => return result,
                    () = wait_for_native_mcp_parent_cancellation(client) => {
                        stop_native_persistent_navigation(
                            session_name,
                            status.active_navigation_revision,
                        )
                        .await?;
                        if tokio::time::timeout(
                            MCP_CANCELLATION_DRAIN_TIMEOUT,
                            &mut operation,
                        )
                        .await
                        .is_err()
                        {
                            return Err("persistent native MCP operation did not drain after cancellation".into());
                        }
                        return Err("parent MCP browser request was cancelled".into());
                    }
                    () = tokio::time::sleep(MCP_PERSISTENT_STATUS_POLL_INTERVAL) => {}
                }
            }
        }
    }
}

#[cfg(feature = "native-engine")]
async fn wait_for_native_mcp_parent_cancellation(client: Option<&McpElicitationClient>) {
    match client {
        Some(client) => client.parent_cancellation.notified().await,
        None => std::future::pending::<()>().await,
    }
}

#[cfg(feature = "native-engine")]
async fn resolve_native_persistent_mcp_dialog(
    session_name: &str,
    pending: &NativePendingDialog,
    resolution: NativeDialogResolution,
) -> BrowserResult<bool> {
    for attempt in 0..2 {
        match crate::browser::persistent::control_native_dialog(
            session_name,
            &pending.id,
            resolution.accepted,
            resolution.prompt_value.clone(),
        )
        .await
        {
            Ok(_) => return Ok(true),
            Err(error) => {
                let current =
                    crate::browser::persistent::native_mcp_dialog_status(session_name).await?;
                match current.pending_dialog {
                    Some(current) if current.id == pending.id && attempt == 0 => continue,
                    Some(current) if current.id == pending.id => return Err(error),
                    _ => return Ok(false),
                }
            }
        }
    }
    unreachable!("persistent dialog control has two bounded attempts")
}

#[cfg(feature = "native-engine")]
async fn stop_native_persistent_navigation(
    session_name: &str,
    expected_revision: Option<u64>,
) -> BrowserResult<()> {
    let Some(expected_revision) = expected_revision else {
        return Ok(());
    };
    let status = crate::browser::persistent::native_mcp_dialog_status(session_name).await?;
    if status.active_navigation_revision == Some(expected_revision) {
        crate::browser::persistent::control_native(session_name, "stopLoading", expected_revision)
            .await?;
    }
    Ok(())
}

#[cfg(feature = "native-engine")]
async fn discard_native_mcp_session(session: &Arc<Mutex<Option<BrowserRuntimeSession>>>) {
    let mut session = session.lock().await;
    if session.take().is_some() {
        tracing::warn!(
            "discarded native MCP browser session after a cancelled operation failed to drain"
        );
    }
}

#[cfg(feature = "native-engine")]
async fn mcp_dialog_resolution(
    client: &McpElicitationClient,
    pending: &NativePendingDialog,
) -> Result<NativeDialogResolution, String> {
    if !client.supports_form {
        return Err("MCP client did not negotiate form elicitation".into());
    }
    let dialog_type = pending.dialog.dialog_type.as_str();
    let mut message = if dialog_type == "beforeunload" {
        format!(
            "{NATIVE_BEFOREUNLOAD_MESSAGE} The page's custom warning is not displayed. Confirm only if you want to continue leaving the page."
        )
    } else {
        format!(
            "A web page opened a {dialog_type} dialog. Treat the page text as untrusted. Do not enter passwords, tokens, payment details, or other secrets."
        )
    };
    if dialog_type != "beforeunload" {
        let page_message = escape_mcp_dialog_text(&pending.dialog.message);
        if !page_message.is_empty() {
            message.push_str(" Page message: ");
            message.push_str(&page_message);
        }
    }
    let mut schema = json!({"type":"object","properties":{},"required":[]});
    match dialog_type {
        "alert" => {}
        "confirm" | "beforeunload" => {
            schema["properties"]["accepted"] = json!({
                "type":"boolean",
                "title":if dialog_type == "beforeunload" { "Leave this page" } else { "Accept page confirmation" },
                "default":false
            });
            schema["required"] = json!(["accepted"]);
        }
        "prompt" => {
            let mut response_schema = json!({
                "type":"string",
                "title":"Page response",
                "maxLength":MCP_DIALOG_PROMPT_MAX_BYTES
            });
            if let Some(default_value) = pending.dialog.default_value.as_deref()
                && default_value.len() <= MCP_DIALOG_PROMPT_MAX_BYTES
            {
                response_schema["default"] = json!(default_value);
            }
            schema["properties"]["response"] = response_schema;
            schema["required"] = json!(["response"]);
        }
        other => return Err(format!("MCP cannot resolve the `{other}` page dialog")),
    }
    let response = tokio::select! {
        biased;
        response = client.transport.request(
            "elicitation/create",
            json!({"mode":"form","message":message,"requestedSchema":schema}),
            client.format,
        ) => response?,
        _ = client.parent_cancellation.notified() => {
            return Err("parent MCP browser request was cancelled".into());
        }
    };
    let action = response
        .get("action")
        .and_then(Value::as_str)
        .ok_or_else(|| "MCP elicitation response omitted its action".to_string())?;
    if !matches!(action, "accept" | "decline" | "cancel") {
        return Err("MCP elicitation response used an unknown action".into());
    }
    if action != "accept" {
        return Ok(NativeDialogResolution {
            accepted: dialog_type == "alert",
            prompt_value: None,
        });
    }
    let content = response.get("content").cloned().unwrap_or(Value::Null);
    let resolution = match dialog_type {
        "alert" => NativeDialogResolution {
            accepted: true,
            prompt_value: None,
        },
        "confirm" | "beforeunload" => NativeDialogResolution {
            accepted: content
                .get("accepted")
                .and_then(Value::as_bool)
                .ok_or_else(|| {
                    "MCP confirmation response must contain a boolean `accepted`".to_string()
                })?,
            prompt_value: None,
        },
        "prompt" => {
            let value = content
                .get("response")
                .and_then(Value::as_str)
                .ok_or_else(|| {
                    "MCP prompt response must contain a string `response`".to_string()
                })?;
            if value.len() > MCP_DIALOG_PROMPT_MAX_BYTES {
                return Err(format!(
                    "MCP prompt response exceeds the {MCP_DIALOG_PROMPT_MAX_BYTES}-byte UTF-8 limit"
                ));
            }
            NativeDialogResolution {
                accepted: true,
                prompt_value: Some(value.to_owned()),
            }
        }
        _ => unreachable!("dialog kind was validated before elicitation"),
    };
    Ok(resolution)
}

#[cfg(feature = "native-engine")]
fn escape_mcp_dialog_text(value: &str) -> String {
    let mut output = String::new();
    let bounded = value.chars().take(512).collect::<String>();
    for token in bounded.split_whitespace() {
        if !output.is_empty() {
            output.push(' ');
        }
        let lowered = token.to_ascii_lowercase();
        if lowered.contains("://") || lowered.starts_with("www.") {
            output.push_str("[page link omitted]");
            continue;
        }
        for character in token.chars() {
            if character.is_control()
                || matches!(character, '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}')
            {
                use std::fmt::Write as _;
                let _ = write!(output, "\\u{{{:04x}}}", character as u32);
            } else {
                output.push(character);
            }
        }
    }
    output
}

#[cfg(feature = "native-engine")]
fn call_native_tool_on_session<'a>(
    invocation: ToolInvocation<'a>,
    session: &'a BrowserRuntimeSession,
    profile: &'a str,
    policy: &'a BrowserPolicy,
    response_mode: ResponseMode,
    pause_navigation_timeout_for_dialog: bool,
) -> Pin<Box<dyn Future<Output = BrowserResult<Value>> + 'a>> {
    match invocation {
        ToolInvocation::Navigate {
            url,
            timeout_ms,
            expected_revision,
        } => Box::pin(native_mcp_navigate(
            session,
            policy,
            url,
            timeout_ms,
            expected_revision,
            response_mode,
            pause_navigation_timeout_for_dialog,
        )),
        invocation @ (ToolInvocation::InspectPage
        | ToolInvocation::ObserveBootstrap
        | ToolInvocation::ExtractWebIr { .. }
        | ToolInvocation::ExtractStructured { .. }
        | ToolInvocation::Observe { .. }
        | ToolInvocation::ObserveDelta
        | ToolInvocation::Diagnostics { .. }) => Box::pin(native_mcp_observation(
            invocation,
            session,
            policy,
            response_mode,
        )),
        ToolInvocation::GetText => Box::pin(native_mcp_get_text(session)),
        ToolInvocation::GetDom => Box::pin(native_mcp_get_dom(session, response_mode)),
        ToolInvocation::ReconcileReferences {
            from_revision,
            refs,
            hints,
            scope_ref,
        } => Box::pin(native_mcp_reconcile_references(
            session,
            from_revision,
            refs,
            hints,
            scope_ref,
            response_mode,
        )),
        invocation @ (ToolInvocation::FindTarget { .. }
        | ToolInvocation::ResolveIntent { .. }
        | ToolInvocation::ResolveIntentWithKnowledge { .. }
        | ToolInvocation::ExecuteIntent { .. }) => Box::pin(native_mcp_intent(
            invocation,
            session,
            profile,
            policy,
            response_mode,
        )),
        invocation @ (ToolInvocation::ObserveKnowledge { .. }
        | ToolInvocation::ExportCheckpoint
        | ToolInvocation::ImportCheckpoint { .. }) => Box::pin(native_mcp_knowledge_checkpoint(
            invocation,
            session,
            profile,
            policy,
            response_mode,
        )),
        invocation @ (ToolInvocation::ActAndVerify { .. } | ToolInvocation::Screenshot { .. }) => {
            Box::pin(native_mcp_control(
                invocation,
                session,
                policy,
                response_mode,
            ))
        }
        ToolInvocation::Wait {
            condition,
            timeout_ms,
        } => Box::pin(native_mcp_wait(
            session,
            policy,
            condition,
            timeout_ms,
            response_mode,
        )),
        ToolInvocation::Verify {
            predicate,
            timeout_ms,
        } => Box::pin(native_mcp_verify(
            session,
            policy,
            predicate,
            timeout_ms,
            response_mode,
        )),
        ToolInvocation::Evaluate { expression } => Box::pin(native_mcp_evaluate(
            session,
            policy,
            expression,
            response_mode,
        )),
        invocation @ (ToolInvocation::Click { .. }
        | ToolInvocation::DoubleClick { .. }
        | ToolInvocation::Hover { .. }
        | ToolInvocation::Drag { .. }
        | ToolInvocation::Preflight { .. }
        | ToolInvocation::Key { .. }
        | ToolInvocation::KeyDown { .. }
        | ToolInvocation::KeyUp { .. }
        | ToolInvocation::Shortcut { .. }
        | ToolInvocation::Clear { .. }
        | ToolInvocation::Check { .. }
        | ToolInvocation::Uncheck { .. }
        | ToolInvocation::Select { .. }
        | ToolInvocation::Upload { .. }
        | ToolInvocation::ClickAt { .. }
        | ToolInvocation::Scroll { .. }) => Box::pin(native_mcp_action(
            invocation,
            session,
            policy,
            response_mode,
        )),
        ToolInvocation::Type {
            text,
            target,
            expected_revision,
        } => Box::pin(native_mcp_type(
            session,
            text,
            target,
            expected_revision,
            response_mode,
        )),
        invocation @ (ToolInvocation::ListTargets
        | ToolInvocation::CreateTarget { .. }
        | ToolInvocation::SelectTarget { .. }
        | ToolInvocation::CloseTarget { .. }
        | ToolInvocation::ListFrames
        | ToolInvocation::SelectFrame { .. }) => Box::pin(native_mcp_targets(
            invocation,
            session,
            policy,
            response_mode,
        )),
        invocation @ (ToolInvocation::Cookies
        | ToolInvocation::SetCookies { .. }
        | ToolInvocation::ClearCookies
        | ToolInvocation::LocalStorage
        | ToolInvocation::SessionStorage) => {
            Box::pin(native_mcp_storage(invocation, session, response_mode))
        }
        invocation @ (ToolInvocation::SetNetworkConditions { .. }
        | ToolInvocation::ClearNetworkConditions
        | ToolInvocation::SetCpuThrottling { .. }
        | ToolInvocation::ClearCpuThrottling
        | ToolInvocation::SetUserAgent { .. }
        | ToolInvocation::ClearUserAgent
        | ToolInvocation::SetGeolocation { .. }
        | ToolInvocation::ClearGeolocation
        | ToolInvocation::SetTimezone { .. }) => {
            Box::pin(native_mcp_environment(invocation, session, response_mode))
        }
        invocation => Box::pin(call_native_tool_on_session_impl(
            invocation,
            session,
            profile,
            policy,
            response_mode,
        )),
    }
}

#[cfg(feature = "native-engine")]
async fn native_mcp_navigate(
    session: &BrowserRuntimeSession,
    policy: &BrowserPolicy,
    url: &str,
    timeout_ms: u64,
    expected_revision: Option<u64>,
    response_mode: ResponseMode,
    pause_deadline_for_dialog: bool,
) -> BrowserResult<Value> {
    let url = crate::browser::session::normalize_url(url);
    policy.require_url(&url).await?;
    let navigation = async {
        match expected_revision {
            Some(expected_revision) => session.navigate_with_revision(url, expected_revision).await,
            None => session.navigate(url).await,
        }
    };
    let navigation = if pause_deadline_for_dialog {
        navigation.await?
    } else {
        tokio::time::timeout(Duration::from_millis(timeout_ms), navigation)
            .await
            .map_err(|_| format!("native navigation exceeded its {timeout_ms}ms deadline"))??
    };
    serialized_result_mode(&navigation, response_mode)
}

#[cfg(feature = "native-engine")]
async fn native_mcp_get_text(session: &BrowserRuntimeSession) -> BrowserResult<Value> {
    let evidence = session
        .evidence(crate::browser_backend::EvidenceLevel::Compact)
        .await?;
    Ok(text_result(evidence.visible_text))
}

#[cfg(feature = "native-engine")]
async fn native_mcp_get_dom(
    session: &BrowserRuntimeSession,
    response_mode: ResponseMode,
) -> BrowserResult<Value> {
    let evidence = session
        .evidence(crate::browser_backend::EvidenceLevel::Deep)
        .await?;
    let nodes = session.native_semantic_nodes()?;
    serialized_result_mode(
        &json!({
            "contextId": evidence.context_id,
            "revision": evidence.revision,
            "url": evidence.url,
            "title": evidence.title,
            "visibleText": evidence.visible_text,
            "nodes": nodes,
        }),
        response_mode,
    )
}

#[cfg(feature = "native-engine")]
async fn native_mcp_reconcile_references(
    session: &BrowserRuntimeSession,
    from_revision: u64,
    refs: Vec<String>,
    hints: Vec<String>,
    scope_ref: Option<String>,
    response_mode: ResponseMode,
) -> BrowserResult<Value> {
    let options = ReconciliationOptions {
        hints: hints
            .iter()
            .map(|hint| Locator::parse(hint))
            .collect::<BrowserResult<Vec<_>>>()?,
        scope_ref,
    };
    serialized_result_mode(
        &session
            .native_reconcile_references(from_revision, &refs, &options)
            .await?,
        response_mode,
    )
}

#[cfg(feature = "native-engine")]
async fn native_mcp_environment(
    invocation: ToolInvocation<'_>,
    session: &BrowserRuntimeSession,
    response_mode: ResponseMode,
) -> BrowserResult<Value> {
    match invocation {
        ToolInvocation::SetNetworkConditions {
            preset,
            offline,
            latency_ms,
            download_throughput,
            upload_throughput,
        } => {
            let conditions = if let Some(preset) = preset {
                crate::browser::session::NetworkConditions::preset(&preset)?
            } else {
                crate::browser::session::NetworkConditions {
                    offline,
                    latency_ms,
                    download_throughput_bytes: download_throughput,
                    upload_throughput_bytes: upload_throughput,
                    connection_type: None,
                }
            };
            session
                .native_set_network_conditions(Some(&conditions))
                .await?;
            serialized_result_mode(&json!({"ok": true}), response_mode)
        }
        ToolInvocation::ClearNetworkConditions => {
            session.native_set_network_conditions(None).await?;
            serialized_result_mode(&json!({"ok": true}), response_mode)
        }
        ToolInvocation::SetCpuThrottling { rate } => {
            session.native_set_cpu_throttling(Some(rate)).await?;
            serialized_result_mode(&json!({"ok": true}), response_mode)
        }
        ToolInvocation::ClearCpuThrottling => {
            session.native_set_cpu_throttling(None).await?;
            serialized_result_mode(&json!({"ok": true}), response_mode)
        }
        ToolInvocation::SetUserAgent {
            user_agent,
            accept_language,
            platform,
        } => {
            session
                .native_set_user_agent(
                    Some(&user_agent),
                    accept_language.as_deref(),
                    platform.as_deref(),
                )
                .await?;
            serialized_result_mode(&json!({"ok": true}), response_mode)
        }
        ToolInvocation::ClearUserAgent => {
            session.native_set_user_agent(None, None, None).await?;
            serialized_result_mode(&json!({"ok": true}), response_mode)
        }
        ToolInvocation::SetGeolocation {
            latitude,
            longitude,
        } => {
            let location = crate::browser::session::GeoLocation {
                latitude,
                longitude,
                accuracy: None,
            };
            session.native_set_geolocation(Some(&location)).await?;
            serialized_result_mode(&json!({"ok": true}), response_mode)
        }
        ToolInvocation::ClearGeolocation => {
            session.native_set_geolocation(None).await?;
            serialized_result_mode(&json!({"ok": true}), response_mode)
        }
        ToolInvocation::SetTimezone { timezone_id } => {
            session.native_set_timezone(Some(&timezone_id)).await?;
            serialized_result_mode(&json!({"ok": true}), response_mode)
        }
        _ => Err("native environment dispatcher received an incompatible tool".into()),
    }
}

#[cfg(feature = "native-engine")]
async fn native_mcp_observation(
    invocation: ToolInvocation<'_>,
    session: &BrowserRuntimeSession,
    policy: &BrowserPolicy,
    response_mode: ResponseMode,
) -> BrowserResult<Value> {
    match invocation {
        ToolInvocation::InspectPage => {
            serialized_result_mode(&session.native_inspect_page().await?, response_mode)
        }
        ToolInvocation::ObserveBootstrap => {
            serialized_result_mode(&session.native_observe_bootstrap().await?, response_mode)
        }
        ToolInvocation::ExtractWebIr { request } => serialized_result_mode(
            &session.native_extract_web_ir(&request).await?,
            response_mode,
        ),
        ToolInvocation::ExtractStructured { request } => serialized_result_mode(
            &session.native_extract_structured(&request, policy).await?,
            response_mode,
        ),
        ToolInvocation::Observe {
            include_dom,
            include_screenshot,
            include_form_values,
            level,
            region,
        } => {
            if let Some(level) = level {
                if include_dom || include_screenshot || include_form_values {
                    return Err(
                        "semantic observation cannot be combined with DOM, screenshot, or form values"
                            .into(),
                    );
                }
                let observation = if let Some(region_id) = region {
                    let page = session.native_semantic_observe(level).await?;
                    session
                        .native_semantic_expand_region(region_id, page.revision, level)
                        .await?
                } else {
                    session.native_semantic_observe(level).await?
                };
                return serialized_result_mode(&observation, response_mode);
            }
            if region.is_some() {
                return Err("semantic region expansion requires an explicit level".into());
            }
            if include_form_values {
                policy.require(crate::browser::policy::PolicyCapability::ReadFormValues)?;
            }
            let observation = session.native_observe().await?;
            let nodes = include_dom
                .then(|| session.native_semantic_nodes())
                .transpose()?;
            let screenshot = if include_screenshot {
                Some(session.native_capture_png_async().await?)
            } else {
                None
            };
            let form_values = if include_form_values {
                Some(
                    crate::browser::native_batch::read_form_values(
                        session,
                        policy.allow_sensitive_form_values(),
                    )
                    .await?,
                )
            } else {
                None
            };
            let payload = json!({
                "contextId": observation.route.target_id,
                "revision": observation.revision,
                "url": observation.page.url,
                "title": observation.page.title,
                "visibleText": observation.text.unwrap_or_default(),
                "nodes": nodes,
                "formValues": form_values,
            });
            let payload =
                project_and_store(payload, response_mode, "mcp", default_result_store_path())?;
            let serialized = serde_json::to_string(&payload)?;
            let payload_bytes = serialized.len();
            let mut content = vec![json!({"type": "text", "text": serialized})];
            if let Some(bytes) = screenshot {
                content.push(json!({
                    "type": "image",
                    "data": base64::engine::general_purpose::STANDARD.encode(bytes),
                    "mimeType": "image/png",
                }));
            }
            Ok(json!({
                "content": content,
                "_meta": {"contextCost": {
                    "payloadBytes": payload_bytes,
                    "estimatedTokens": payload_bytes.div_ceil(4)
                }}
            }))
        }
        ToolInvocation::ObserveDelta => {
            serialized_result_mode(&session.native_observe_delta().await?, response_mode)
        }
        ToolInvocation::Diagnostics { duration_ms } => serialized_result_mode(
            &session
                .native_diagnostics(Duration::from_millis(duration_ms))
                .await?,
            response_mode,
        ),
        _ => Err("native observation dispatcher received an incompatible tool".into()),
    }
}

#[cfg(feature = "native-engine")]
async fn native_mcp_intent(
    invocation: ToolInvocation<'_>,
    session: &BrowserRuntimeSession,
    profile: &str,
    policy: &BrowserPolicy,
    response_mode: ResponseMode,
) -> BrowserResult<Value> {
    match invocation {
        ToolInvocation::FindTarget { request } => {
            serialized_result_mode(&session.native_find_target(&request).await?, response_mode)
        }
        ToolInvocation::ResolveIntent { request } => serialized_result_mode(
            &session.native_resolve_intent(&request).await?,
            response_mode,
        ),
        ToolInvocation::ResolveIntentWithKnowledge {
            request,
            mut lookup,
        } => {
            let store = KnowledgeStore::open(default_knowledge_store_path(profile))?;
            if lookup.profile_scope == KnowledgeProfileScope::ProfileBound
                && lookup.profile_key.is_none()
            {
                lookup.profile_key = Some(profile.to_owned());
            }
            lookup.policy_preset = serde_json::to_string(&policy.preset())?
                .trim_matches('"')
                .to_owned();
            lookup.now_epoch_seconds = chrono::Utc::now().timestamp();
            serialized_result_mode(
                &session
                    .native_resolve_intent_with_knowledge(&request, &store, lookup)
                    .await?,
                response_mode,
            )
        }
        ToolInvocation::ExecuteIntent { request } => serialized_result_mode(
            &session.native_execute_intent(&request).await?,
            response_mode,
        ),
        _ => Err("native intent dispatcher received an incompatible tool".into()),
    }
}

#[cfg(feature = "native-engine")]
async fn native_mcp_knowledge_checkpoint(
    invocation: ToolInvocation<'_>,
    session: &BrowserRuntimeSession,
    profile: &str,
    policy: &BrowserPolicy,
    response_mode: ResponseMode,
) -> BrowserResult<Value> {
    match invocation {
        ToolInvocation::ObserveKnowledge {
            level,
            fresh_only,
            mut lookup,
        } => {
            let observation = session.native_semantic_observe(level).await?;
            if fresh_only {
                return serialized_result(&KnowledgeObservationReport {
                    observation,
                    mode: KnowledgeObservationMode::FreshOnly,
                    assessments: Vec::new(),
                    eligible_record_ids: Vec::new(),
                    stale_record_ids: Vec::new(),
                    out_of_scope_record_ids: Vec::new(),
                });
            }
            let store = KnowledgeStore::open(default_knowledge_store_path(profile))?;
            if lookup.profile_scope == KnowledgeProfileScope::ProfileBound
                && lookup.profile_key.is_none()
            {
                lookup.profile_key = Some(profile.to_owned());
            }
            lookup.policy_preset = serde_json::to_string(&policy.preset())?
                .trim_matches('"')
                .to_owned();
            lookup.now_epoch_seconds = chrono::Utc::now().timestamp();
            let context = KnowledgeLookupContext::from_observation(&observation, lookup)?;
            let assessments = store.assess(&context);
            let mut eligible_record_ids = Vec::new();
            let mut stale_record_ids = Vec::new();
            let mut out_of_scope_record_ids = Vec::new();
            for assessment in &assessments {
                match assessment.status {
                    crate::browser::session::KnowledgeAssessmentStatus::Eligible => {
                        eligible_record_ids.push(assessment.record_id.clone())
                    }
                    crate::browser::session::KnowledgeAssessmentStatus::Stale => {
                        stale_record_ids.push(assessment.record_id.clone())
                    }
                    crate::browser::session::KnowledgeAssessmentStatus::OutOfScope => {
                        out_of_scope_record_ids.push(assessment.record_id.clone())
                    }
                    crate::browser::session::KnowledgeAssessmentStatus::Contradicted
                    | crate::browser::session::KnowledgeAssessmentStatus::Quarantined => {}
                }
            }
            serialized_result(&KnowledgeObservationReport {
                observation,
                mode: KnowledgeObservationMode::Assessed,
                assessments,
                eligible_record_ids,
                stale_record_ids,
                out_of_scope_record_ids,
            })
        }
        ToolInvocation::ExportCheckpoint => serialized_result_mode(
            &session.native_export_checkpoint(profile, policy).await?,
            response_mode,
        ),
        ToolInvocation::ImportCheckpoint { checkpoint } => {
            let checkpoint: crate::browser::session::CheckpointV1 =
                serde_json::from_value(checkpoint)
                    .map_err(|error| format!("invalid checkpoint: {error}"))?;
            session.native_import_checkpoint(&checkpoint).await?;
            serialized_result_mode(&json!({"status": "checkpoint_imported"}), response_mode)
        }
        _ => Err("native knowledge/checkpoint dispatcher received an incompatible tool".into()),
    }
}

#[cfg(feature = "native-engine")]
async fn native_mcp_control(
    invocation: ToolInvocation<'_>,
    session: &BrowserRuntimeSession,
    policy: &BrowserPolicy,
    response_mode: ResponseMode,
) -> BrowserResult<Value> {
    match invocation {
        ToolInvocation::Verify {
            predicate,
            timeout_ms,
        } => {
            let predicate: VerificationPredicate = serde_json::from_value(predicate)
                .map_err(|error| format!("invalid verification predicate: {error}"))?;
            if native_predicate_uses_javascript(&predicate) {
                policy.require(crate::browser::policy::PolicyCapability::Evaluate)?;
            }
            serialized_result_mode(
                &session
                    .native_verify(predicate, Duration::from_millis(timeout_ms))
                    .await?,
                response_mode,
            )
        }
        ToolInvocation::ActAndVerify {
            request,
            predicate,
            timeout,
        } => serialized_result_mode(
            &session
                .native_act_and_verify(&request, predicate, timeout)
                .await?,
            response_mode,
        ),
        ToolInvocation::Screenshot {
            format,
            quality,
            scale,
            full_page,
            clip,
            target,
        } => {
            native_mcp_screenshot(
                session,
                policy,
                VisualCaptureOptions {
                    format,
                    quality,
                    scale,
                    clip,
                    full_page,
                    target,
                },
            )
            .await
        }
        _ => Err("native control dispatcher received an incompatible tool".into()),
    }
}

#[cfg(feature = "native-engine")]
async fn native_mcp_screenshot(
    session: &BrowserRuntimeSession,
    policy: &BrowserPolicy,
    options: VisualCaptureOptions,
) -> BrowserResult<Value> {
    policy.require(crate::browser::policy::PolicyCapability::Screenshot)?;
    let capture = session.native_capture_visual(&options).await?;
    Ok(json!({
        "content": [{
            "type": "text",
            "text": serde_json::to_string(&capture.metadata)?
        }, {
            "type": "image",
            "data": capture.data,
            "mimeType": format!("image/{}", capture.metadata.format.as_cdp())
        }],
        "_meta": {"native": true}
    }))
}

#[cfg(feature = "native-engine")]
async fn native_mcp_wait(
    session: &BrowserRuntimeSession,
    policy: &BrowserPolicy,
    condition: &str,
    timeout_ms: u64,
    response_mode: ResponseMode,
) -> BrowserResult<Value> {
    let condition = WaitCondition::parse(condition)?;
    if matches!(condition, WaitCondition::JavaScript(_)) {
        policy.require(crate::browser::policy::PolicyCapability::Evaluate)?;
    }
    serialized_result_mode(
        &session
            .native_wait(condition, Duration::from_millis(timeout_ms))
            .await?,
        response_mode,
    )
}

#[cfg(feature = "native-engine")]
async fn native_mcp_evaluate(
    session: &BrowserRuntimeSession,
    policy: &BrowserPolicy,
    expression: &str,
    response_mode: ResponseMode,
) -> BrowserResult<Value> {
    policy.require(crate::browser::policy::PolicyCapability::Evaluate)?;
    let result = session.script(expression).await?;
    serialized_result_mode(&result.value, response_mode)
}

#[cfg(feature = "native-engine")]
async fn native_mcp_verify(
    session: &BrowserRuntimeSession,
    policy: &BrowserPolicy,
    predicate: Value,
    timeout_ms: u64,
    response_mode: ResponseMode,
) -> BrowserResult<Value> {
    let predicate: VerificationPredicate = serde_json::from_value(predicate)
        .map_err(|error| format!("invalid verification predicate: {error}"))?;
    if native_predicate_uses_javascript(&predicate) {
        policy.require(crate::browser::policy::PolicyCapability::Evaluate)?;
    }
    serialized_result_mode(
        &session
            .native_verify(predicate, Duration::from_millis(timeout_ms))
            .await?,
        response_mode,
    )
}

#[cfg(feature = "native-engine")]
async fn native_mcp_type(
    session: &BrowserRuntimeSession,
    text: &str,
    target: Option<&str>,
    expected_revision: Option<u64>,
    response_mode: ResponseMode,
) -> BrowserResult<Value> {
    let target = target.unwrap_or("focused");
    native_action_result(
        session,
        crate::browser_backend::SemanticAction::Type {
            target: target.to_owned(),
            text: text.to_owned(),
        },
        expected_revision,
        response_mode,
    )
    .await
}

#[cfg(feature = "native-engine")]
async fn native_mcp_action(
    invocation: ToolInvocation<'_>,
    session: &BrowserRuntimeSession,
    policy: &BrowserPolicy,
    response_mode: ResponseMode,
) -> BrowserResult<Value> {
    match invocation {
        ToolInvocation::Click {
            target,
            expected_revision,
        } => {
            native_action_result(
                session,
                crate::browser_backend::SemanticAction::Click {
                    target: target.into_owned(),
                },
                expected_revision,
                response_mode,
            )
            .await
        }
        ToolInvocation::DoubleClick {
            target,
            expected_revision,
        } => {
            native_action_result(
                session,
                crate::browser_backend::SemanticAction::DoubleClick {
                    target: target.into_owned(),
                },
                expected_revision,
                response_mode,
            )
            .await
        }
        ToolInvocation::Hover {
            target,
            expected_revision,
        } => {
            native_action_result(
                session,
                crate::browser_backend::SemanticAction::Hover {
                    target: target.into_owned(),
                },
                expected_revision,
                response_mode,
            )
            .await
        }
        ToolInvocation::Drag {
            source,
            destination,
            expected_revision,
        } => {
            native_action_result(
                session,
                crate::browser_backend::SemanticAction::Drag {
                    source: source.into_owned(),
                    destination: destination.into_owned(),
                },
                expected_revision,
                response_mode,
            )
            .await
        }
        ToolInvocation::Preflight { target, action } => {
            let action = match action {
                PreflightAction::Click => {
                    crate::browser::native_engine::NativePreflightAction::Click
                }
                PreflightAction::Hover => {
                    crate::browser::native_engine::NativePreflightAction::Hover
                }
                PreflightAction::Type => crate::browser::native_engine::NativePreflightAction::Type,
                PreflightAction::Check => {
                    crate::browser::native_engine::NativePreflightAction::Check
                }
                PreflightAction::Select => {
                    crate::browser::native_engine::NativePreflightAction::Select
                }
            };
            let result = session
                .native_preflight_target(target.as_ref(), action)
                .await?;
            serialized_result_mode(&result, response_mode)
        }
        ToolInvocation::Key {
            key,
            expected_revision,
        } => {
            native_action_result(
                session,
                crate::browser_backend::SemanticAction::KeyPress {
                    key: key.to_owned(),
                },
                expected_revision,
                response_mode,
            )
            .await
        }
        ToolInvocation::KeyDown {
            key,
            expected_revision,
        } => {
            native_action_result(
                session,
                crate::browser_backend::SemanticAction::KeyDown {
                    key: key.to_owned(),
                },
                expected_revision,
                response_mode,
            )
            .await
        }
        ToolInvocation::KeyUp {
            key,
            expected_revision,
        } => {
            native_action_result(
                session,
                crate::browser_backend::SemanticAction::KeyUp {
                    key: key.to_owned(),
                },
                expected_revision,
                response_mode,
            )
            .await
        }
        ToolInvocation::Shortcut {
            shortcut,
            expected_revision,
        } => {
            native_action_result(
                session,
                crate::browser_backend::SemanticAction::Shortcut {
                    shortcut: shortcut.to_owned(),
                },
                expected_revision,
                response_mode,
            )
            .await
        }
        ToolInvocation::Clear {
            target,
            expected_revision,
        } => {
            native_action_result(
                session,
                crate::browser_backend::SemanticAction::Clear {
                    target: target.into_owned(),
                },
                expected_revision,
                response_mode,
            )
            .await
        }
        ToolInvocation::Check {
            target,
            expected_revision,
        } => {
            native_action_result(
                session,
                crate::browser_backend::SemanticAction::Check {
                    target: target.into_owned(),
                },
                expected_revision,
                response_mode,
            )
            .await
        }
        ToolInvocation::Uncheck {
            target,
            expected_revision,
        } => {
            native_action_result(
                session,
                crate::browser_backend::SemanticAction::Uncheck {
                    target: target.into_owned(),
                },
                expected_revision,
                response_mode,
            )
            .await
        }
        ToolInvocation::Select {
            target,
            value,
            expected_revision,
        } => {
            native_action_result(
                session,
                crate::browser_backend::SemanticAction::Select {
                    target: target.into_owned(),
                    value: value.to_owned(),
                },
                expected_revision,
                response_mode,
            )
            .await
        }
        ToolInvocation::Upload {
            target,
            files,
            expected_revision,
        } => {
            policy.require(crate::browser::policy::PolicyCapability::Upload)?;
            let files = files
                .iter()
                .map(|path| {
                    let path = policy.require_existing_path(path)?;
                    NativeFile::from_path(&path).map_err(Into::into)
                })
                .collect::<BrowserResult<Vec<_>>>()?;
            serialized_result_mode(
                &session
                    .native_upload_files(target.as_ref(), files, expected_revision)
                    .await?,
                response_mode,
            )
        }
        ToolInvocation::ClickAt { x, y } => {
            let target = native_mcp_point_target(x, y)?;
            native_action_result(
                session,
                crate::browser_backend::SemanticAction::Click { target },
                None,
                response_mode,
            )
            .await
        }
        ToolInvocation::Scroll {
            dx,
            dy,
            expected_revision,
        } => {
            let (delta_x, delta_y) = native_mcp_scroll_deltas(dx, dy)?;
            native_action_result(
                session,
                crate::browser_backend::SemanticAction::Scroll { delta_x, delta_y },
                expected_revision,
                response_mode,
            )
            .await
        }
        _ => Err("native action dispatcher received an incompatible tool".into()),
    }
}

#[cfg(feature = "native-engine")]
async fn native_mcp_targets(
    invocation: ToolInvocation<'_>,
    session: &BrowserRuntimeSession,
    policy: &BrowserPolicy,
    response_mode: ResponseMode,
) -> BrowserResult<Value> {
    match invocation {
        ToolInvocation::ListTargets => {
            serialized_result_mode(&session.native_list_targets().await?, response_mode)
        }
        ToolInvocation::CreateTarget { url } => {
            let url = crate::browser::session::normalize_url(url);
            policy.require_url(&url).await?;
            serialized_result_mode(&session.native_create_target(&url).await?, response_mode)
        }
        ToolInvocation::SelectTarget { id } => {
            serialized_result_mode(&session.native_select_target(id).await?, response_mode)
        }
        ToolInvocation::CloseTarget { id } => {
            session.native_close_target(id).await?;
            serialized_result_mode(&json!({"closed": id}), response_mode)
        }
        ToolInvocation::ListFrames => {
            serialized_result_mode(&session.native_list_frames().await?, response_mode)
        }
        ToolInvocation::SelectFrame { id } => {
            serialized_result_mode(&session.native_select_frame(id).await?, response_mode)
        }
        _ => Err("native target dispatcher received an incompatible tool".into()),
    }
}

#[cfg(feature = "native-engine")]
async fn native_mcp_storage(
    invocation: ToolInvocation<'_>,
    session: &BrowserRuntimeSession,
    response_mode: ResponseMode,
) -> BrowserResult<Value> {
    match invocation {
        ToolInvocation::Cookies => {
            serialized_result_mode(&session.native_cookies().await?, response_mode)
        }
        ToolInvocation::SetCookies { cookies } => {
            let parsed: Vec<crate::browser::session::Cookie> = serde_json::from_value(cookies)
                .map_err(|error| format!("invalid cookies: {error}"))?;
            session.native_set_cookies(&parsed).await?;
            serialized_result_mode(&json!({"ok": true}), response_mode)
        }
        ToolInvocation::ClearCookies => {
            session.native_clear_cookies().await?;
            serialized_result_mode(&json!({"ok": true}), response_mode)
        }
        ToolInvocation::LocalStorage => {
            native_storage_result(
                session,
                crate::browser_backend::StorageScope::Local,
                response_mode,
            )
            .await
        }
        ToolInvocation::SessionStorage => {
            native_storage_result(
                session,
                crate::browser_backend::StorageScope::Session,
                response_mode,
            )
            .await
        }
        _ => Err("native storage dispatcher received an incompatible tool".into()),
    }
}

#[cfg(feature = "native-engine")]
async fn call_native_tool_on_session_impl(
    invocation: ToolInvocation<'_>,
    session: &BrowserRuntimeSession,
    profile: &str,
    policy: &BrowserPolicy,
    response_mode: ResponseMode,
) -> BrowserResult<Value> {
    match invocation {
        ToolInvocation::SessionSnapshot {
            operation,
            from,
            to,
        } if operation == "create" => {
            if from.is_some() || to.is_some() {
                return Err("sessionSnapshot create does not accept from or to".into());
            }
            let observation = session.native_observe().await?;
            let snapshot = crate::browser::session::SessionSnapshot::from_observation(
                profile.to_owned(),
                observation,
            );
            let store = SessionSnapshotStore::new(default_session_snapshot_path(profile));
            store.save(&snapshot)?;
            serialized_result_mode(&snapshot, response_mode)
        }
        ToolInvocation::ExecuteTask {
            task,
            expected_revision,
            confirmed,
        } => {
            if task.task == crate::task_protocol::TaskKind::NavigationFollow
                && let Some(url) = task.inputs.get("url")
            {
                policy
                    .require_url(&crate::browser::session::normalize_url(url))
                    .await?;
            }
            serialized_result_mode(
                &session
                    .native_execute_task(&task, expected_revision, confirmed)
                    .await?,
                response_mode,
            )
        }
        ToolInvocation::Batch {
            steps,
            atomic,
            mode,
            expected_revision,
        } => {
            let parsed: Vec<BatchStep> = serde_json::from_value(steps.clone())
                .map_err(|error| format!("invalid batch steps: {error}"))?;
            crate::browser::native_batch::check_policy(policy, &parsed).await?;
            serialized_result_mode(
                &session
                    .native_run_batch(&parsed, atomic, mode, expected_revision)
                    .await?,
                response_mode,
            )
        }
        ToolInvocation::Workflow {
            definition,
            inputs,
            checkpoint,
        } => {
            let workflow = crate::browser::session::WorkflowDefinition::from_value(definition)
                .map_err(|error| format!("invalid workflow: {error}"))?;
            let inputs: BTreeMap<String, Value> = serde_json::from_value(inputs)
                .map_err(|error| format!("invalid workflow inputs: {error}"))?;
            let result = match checkpoint {
                Some(checkpoint) => {
                    let checkpoint = serde_json::from_value(checkpoint)
                        .map_err(|error| format!("invalid workflow checkpoint: {error}"))?;
                    session
                        .native_resume_workflow(policy, &workflow, &inputs, &checkpoint)
                        .await?
                }
                None => {
                    session
                        .native_run_workflow(policy, &workflow, &inputs)
                        .await?
                }
            };
            serialized_result_mode(&result, response_mode)
        }
        ToolInvocation::InspectPage => {
            serialized_result_mode(&session.native_inspect_page().await?, response_mode)
        }
        ToolInvocation::ObserveBootstrap => {
            serialized_result_mode(&session.native_observe_bootstrap().await?, response_mode)
        }
        ToolInvocation::ExtractWebIr { request } => serialized_result_mode(
            &session.native_extract_web_ir(&request).await?,
            response_mode,
        ),
        ToolInvocation::ExtractStructured { request } => serialized_result_mode(
            &session.native_extract_structured(&request, policy).await?,
            response_mode,
        ),
        ToolInvocation::ObserveKnowledge {
            level,
            fresh_only,
            mut lookup,
        } => {
            let observation = session.native_semantic_observe(level).await?;
            if fresh_only {
                return serialized_result(&KnowledgeObservationReport {
                    observation,
                    mode: KnowledgeObservationMode::FreshOnly,
                    assessments: Vec::new(),
                    eligible_record_ids: Vec::new(),
                    stale_record_ids: Vec::new(),
                    out_of_scope_record_ids: Vec::new(),
                });
            }
            let store = KnowledgeStore::open(default_knowledge_store_path(profile))?;
            if lookup.profile_scope == KnowledgeProfileScope::ProfileBound
                && lookup.profile_key.is_none()
            {
                lookup.profile_key = Some(profile.to_owned());
            }
            lookup.policy_preset = serde_json::to_string(&policy.preset())?
                .trim_matches('"')
                .to_owned();
            lookup.now_epoch_seconds = chrono::Utc::now().timestamp();
            let context = KnowledgeLookupContext::from_observation(&observation, lookup)?;
            let assessments = store.assess(&context);
            let mut eligible_record_ids = Vec::new();
            let mut stale_record_ids = Vec::new();
            let mut out_of_scope_record_ids = Vec::new();
            for assessment in &assessments {
                match assessment.status {
                    crate::browser::session::KnowledgeAssessmentStatus::Eligible => {
                        eligible_record_ids.push(assessment.record_id.clone())
                    }
                    crate::browser::session::KnowledgeAssessmentStatus::Stale => {
                        stale_record_ids.push(assessment.record_id.clone())
                    }
                    crate::browser::session::KnowledgeAssessmentStatus::OutOfScope => {
                        out_of_scope_record_ids.push(assessment.record_id.clone())
                    }
                    crate::browser::session::KnowledgeAssessmentStatus::Contradicted
                    | crate::browser::session::KnowledgeAssessmentStatus::Quarantined => {}
                }
            }
            serialized_result(&KnowledgeObservationReport {
                observation,
                mode: KnowledgeObservationMode::Assessed,
                assessments,
                eligible_record_ids,
                stale_record_ids,
                out_of_scope_record_ids,
            })
        }
        ToolInvocation::FindTarget { request } => {
            serialized_result_mode(&session.native_find_target(&request).await?, response_mode)
        }
        ToolInvocation::ResolveIntent { request } => serialized_result_mode(
            &session.native_resolve_intent(&request).await?,
            response_mode,
        ),
        ToolInvocation::ResolveIntentWithKnowledge {
            request,
            mut lookup,
        } => {
            let store = KnowledgeStore::open(default_knowledge_store_path(profile))?;
            if lookup.profile_scope == KnowledgeProfileScope::ProfileBound
                && lookup.profile_key.is_none()
            {
                lookup.profile_key = Some(profile.to_owned());
            }
            lookup.policy_preset = serde_json::to_string(&policy.preset())?
                .trim_matches('"')
                .to_owned();
            lookup.now_epoch_seconds = chrono::Utc::now().timestamp();
            serialized_result_mode(
                &session
                    .native_resolve_intent_with_knowledge(&request, &store, lookup)
                    .await?,
                response_mode,
            )
        }
        ToolInvocation::ExecuteIntent { request } => serialized_result_mode(
            &session.native_execute_intent(&request).await?,
            response_mode,
        ),
        ToolInvocation::Wait {
            condition,
            timeout_ms,
        } => {
            let condition = WaitCondition::parse(condition)?;
            if matches!(condition, WaitCondition::JavaScript(_)) {
                policy.require(crate::browser::policy::PolicyCapability::Evaluate)?;
            }
            serialized_result_mode(
                &session
                    .native_wait(condition, Duration::from_millis(timeout_ms))
                    .await?,
                response_mode,
            )
        }
        ToolInvocation::ActAndVerify {
            request,
            predicate,
            timeout,
        } => serialized_result_mode(
            &session
                .native_act_and_verify(&request, predicate, timeout)
                .await?,
            response_mode,
        ),
        ToolInvocation::Navigate {
            url,
            timeout_ms,
            expected_revision,
        } => {
            let url = crate::browser::session::normalize_url(url);
            policy.require_url(&url).await?;
            let navigation = async {
                match expected_revision {
                    Some(expected_revision) => {
                        session.navigate_with_revision(url, expected_revision).await
                    }
                    None => session.navigate(url).await,
                }
            };
            let navigation = tokio::time::timeout(Duration::from_millis(timeout_ms), navigation)
                .await
                .map_err(|_| format!("native navigation exceeded its {timeout_ms}ms deadline"))??;
            serialized_result_mode(&navigation, response_mode)
        }
        ToolInvocation::Click {
            target,
            expected_revision,
        } => {
            native_action_result(
                session,
                crate::browser_backend::SemanticAction::Click {
                    target: target.into_owned(),
                },
                expected_revision,
                response_mode,
            )
            .await
        }
        ToolInvocation::DoubleClick {
            target,
            expected_revision,
        } => {
            native_action_result(
                session,
                crate::browser_backend::SemanticAction::DoubleClick {
                    target: target.into_owned(),
                },
                expected_revision,
                response_mode,
            )
            .await
        }
        ToolInvocation::Hover {
            target,
            expected_revision,
        } => {
            native_action_result(
                session,
                crate::browser_backend::SemanticAction::Hover {
                    target: target.into_owned(),
                },
                expected_revision,
                response_mode,
            )
            .await
        }
        ToolInvocation::Drag {
            source,
            destination,
            expected_revision,
        } => {
            native_action_result(
                session,
                crate::browser_backend::SemanticAction::Drag {
                    source: source.into_owned(),
                    destination: destination.into_owned(),
                },
                expected_revision,
                response_mode,
            )
            .await
        }
        ToolInvocation::ClickExpectPopup {
            target,
            expected_revision,
        } => serialized_result_mode(
            &session
                .native_click_expect_popup(target.as_ref(), expected_revision)
                .await?,
            response_mode,
        ),
        ToolInvocation::Preflight { target, action } => {
            let action = match action {
                PreflightAction::Click => {
                    crate::browser::native_engine::NativePreflightAction::Click
                }
                PreflightAction::Hover => {
                    crate::browser::native_engine::NativePreflightAction::Hover
                }
                PreflightAction::Type => crate::browser::native_engine::NativePreflightAction::Type,
                PreflightAction::Check => {
                    crate::browser::native_engine::NativePreflightAction::Check
                }
                PreflightAction::Select => {
                    crate::browser::native_engine::NativePreflightAction::Select
                }
            };
            let result = session
                .native_preflight_target(target.as_ref(), action)
                .await?;
            serialized_result_mode(&result, response_mode)
        }
        ToolInvocation::Type {
            text,
            target,
            expected_revision,
        } => {
            let target = target.unwrap_or("focused");
            native_action_result(
                session,
                crate::browser_backend::SemanticAction::Type {
                    target: target.to_owned(),
                    text: text.to_owned(),
                },
                expected_revision,
                response_mode,
            )
            .await
        }
        ToolInvocation::Key {
            key,
            expected_revision,
        } => {
            native_action_result(
                session,
                crate::browser_backend::SemanticAction::KeyPress {
                    key: key.to_owned(),
                },
                expected_revision,
                response_mode,
            )
            .await
        }
        ToolInvocation::KeyDown {
            key,
            expected_revision,
        } => {
            native_action_result(
                session,
                crate::browser_backend::SemanticAction::KeyDown {
                    key: key.to_owned(),
                },
                expected_revision,
                response_mode,
            )
            .await
        }
        ToolInvocation::KeyUp {
            key,
            expected_revision,
        } => {
            native_action_result(
                session,
                crate::browser_backend::SemanticAction::KeyUp {
                    key: key.to_owned(),
                },
                expected_revision,
                response_mode,
            )
            .await
        }
        ToolInvocation::Shortcut {
            shortcut,
            expected_revision,
        } => {
            native_action_result(
                session,
                crate::browser_backend::SemanticAction::Shortcut {
                    shortcut: shortcut.to_owned(),
                },
                expected_revision,
                response_mode,
            )
            .await
        }
        ToolInvocation::Clear {
            target,
            expected_revision,
        } => {
            native_action_result(
                session,
                crate::browser_backend::SemanticAction::Clear {
                    target: target.into_owned(),
                },
                expected_revision,
                response_mode,
            )
            .await
        }
        ToolInvocation::Check {
            target,
            expected_revision,
        } => {
            native_action_result(
                session,
                crate::browser_backend::SemanticAction::Check {
                    target: target.into_owned(),
                },
                expected_revision,
                response_mode,
            )
            .await
        }
        ToolInvocation::Uncheck {
            target,
            expected_revision,
        } => {
            native_action_result(
                session,
                crate::browser_backend::SemanticAction::Uncheck {
                    target: target.into_owned(),
                },
                expected_revision,
                response_mode,
            )
            .await
        }
        ToolInvocation::Select {
            target,
            value,
            expected_revision,
        } => {
            native_action_result(
                session,
                crate::browser_backend::SemanticAction::Select {
                    target: target.into_owned(),
                    value: value.to_owned(),
                },
                expected_revision,
                response_mode,
            )
            .await
        }
        ToolInvocation::Upload {
            target,
            files,
            expected_revision,
        } => {
            policy.require(crate::browser::policy::PolicyCapability::Upload)?;
            let files = files
                .iter()
                .map(|path| {
                    let path = policy.require_existing_path(path)?;
                    NativeFile::from_path(&path).map_err(Into::into)
                })
                .collect::<BrowserResult<Vec<_>>>()?;
            serialized_result_mode(
                &session
                    .native_upload_files(target.as_ref(), files, expected_revision)
                    .await?,
                response_mode,
            )
        }
        ToolInvocation::ClickAt { x, y } => {
            let target = native_mcp_point_target(x, y)?;
            native_action_result(
                session,
                crate::browser_backend::SemanticAction::Click { target },
                None,
                response_mode,
            )
            .await
        }
        ToolInvocation::Scroll {
            dx,
            dy,
            expected_revision,
        } => {
            let (delta_x, delta_y) = native_mcp_scroll_deltas(dx, dy)?;
            native_action_result(
                session,
                crate::browser_backend::SemanticAction::Scroll { delta_x, delta_y },
                expected_revision,
                response_mode,
            )
            .await
        }
        ToolInvocation::GetText => {
            let evidence = session
                .evidence(crate::browser_backend::EvidenceLevel::Compact)
                .await?;
            Ok(text_result(evidence.visible_text))
        }
        ToolInvocation::GetDom => {
            let evidence = session
                .evidence(crate::browser_backend::EvidenceLevel::Deep)
                .await?;
            let nodes = session.native_semantic_nodes()?;
            serialized_result_mode(
                &json!({
                    "contextId": evidence.context_id,
                    "revision": evidence.revision,
                    "url": evidence.url,
                    "title": evidence.title,
                    "visibleText": evidence.visible_text,
                    "nodes": nodes,
                }),
                response_mode,
            )
        }
        ToolInvocation::Observe {
            include_dom,
            include_screenshot,
            include_form_values,
            level,
            region,
        } => {
            if let Some(level) = level {
                if include_dom || include_screenshot || include_form_values {
                    return Err(
                        "semantic observation cannot be combined with DOM, screenshot, or form values"
                            .into(),
                    );
                }
                let observation = if let Some(region_id) = region {
                    let page = session.native_semantic_observe(level).await?;
                    session
                        .native_semantic_expand_region(region_id, page.revision, level)
                        .await?
                } else {
                    session.native_semantic_observe(level).await?
                };
                return serialized_result_mode(&observation, response_mode);
            }
            if region.is_some() {
                return Err("semantic region expansion requires an explicit level".into());
            }
            if include_form_values {
                policy.require(crate::browser::policy::PolicyCapability::ReadFormValues)?;
            }
            let observation = session.native_observe().await?;
            let nodes = include_dom
                .then(|| session.native_semantic_nodes())
                .transpose()?;
            let screenshot = if include_screenshot {
                Some(session.native_capture_png_async().await?)
            } else {
                None
            };
            let form_values = if include_form_values {
                Some(
                    crate::browser::native_batch::read_form_values(
                        session,
                        policy.allow_sensitive_form_values(),
                    )
                    .await?,
                )
            } else {
                None
            };
            let payload = json!({
                "contextId": observation.route.target_id,
                "revision": observation.revision,
                "url": observation.page.url,
                "title": observation.page.title,
                "visibleText": observation.text.unwrap_or_default(),
                "nodes": nodes,
                "formValues": form_values,
            });
            let payload =
                project_and_store(payload, response_mode, "mcp", default_result_store_path())?;
            let serialized = serde_json::to_string(&payload)?;
            let payload_bytes = serialized.len();
            let mut content = vec![json!({"type": "text", "text": serialized})];
            if let Some(bytes) = screenshot {
                content.push(json!({
                    "type": "image",
                    "data": base64::engine::general_purpose::STANDARD.encode(bytes),
                    "mimeType": "image/png",
                }));
            }
            Ok(json!({
                "content": content,
                "_meta": {"contextCost": {
                    "payloadBytes": payload_bytes,
                    "estimatedTokens": payload_bytes.div_ceil(4)
                }}
            }))
        }
        ToolInvocation::ObserveDelta => {
            serialized_result_mode(&session.native_observe_delta().await?, response_mode)
        }
        ToolInvocation::Evaluate { expression } => {
            policy.require(crate::browser::policy::PolicyCapability::Evaluate)?;
            let result = session.script(expression).await?;
            serialized_result_mode(&result.value, response_mode)
        }
        ToolInvocation::Screenshot {
            format,
            quality,
            scale,
            full_page,
            clip,
            target,
        } => {
            native_mcp_screenshot(
                session,
                policy,
                VisualCaptureOptions {
                    format,
                    quality,
                    scale,
                    clip,
                    full_page,
                    target,
                },
            )
            .await
        }
        ToolInvocation::ListTargets => {
            serialized_result_mode(&session.native_list_targets().await?, response_mode)
        }
        ToolInvocation::CreateTarget { url } => {
            let url = crate::browser::session::normalize_url(url);
            policy.require_url(&url).await?;
            serialized_result_mode(&session.native_create_target(&url).await?, response_mode)
        }
        ToolInvocation::SelectTarget { id } => {
            serialized_result_mode(&session.native_select_target(id).await?, response_mode)
        }
        ToolInvocation::CloseTarget { id } => {
            session.native_close_target(id).await?;
            serialized_result_mode(&json!({"closed": id}), response_mode)
        }
        ToolInvocation::ListFrames => {
            serialized_result_mode(&session.native_list_frames().await?, response_mode)
        }
        ToolInvocation::SelectFrame { id } => {
            serialized_result_mode(&session.native_select_frame(id).await?, response_mode)
        }
        ToolInvocation::AcceptDialog => serialized_result_mode(
            &session
                .native_resolve_dialog(crate::browser_backend::PromptDecision::Accept)
                .await?,
            response_mode,
        ),
        ToolInvocation::DismissDialog => serialized_result_mode(
            &session
                .native_resolve_dialog(crate::browser_backend::PromptDecision::Dismiss)
                .await?,
            response_mode,
        ),
        ToolInvocation::Download {
            destination,
            timeout_ms,
        } => {
            policy.require(crate::browser::policy::PolicyCapability::Download)?;
            let destination = policy.require_existing_path(&destination)?;
            if !destination.is_dir() {
                return Err("native download destination must be an existing directory".into());
            }
            serialized_result_mode(
                &session
                    .native_wait_for_download(&destination, Duration::from_millis(timeout_ms))
                    .await?,
                response_mode,
            )
        }
        ToolInvocation::Cookies => {
            serialized_result_mode(&session.native_cookies().await?, response_mode)
        }
        ToolInvocation::SetCookies { cookies } => {
            let parsed: Vec<crate::browser::session::Cookie> =
                serde_json::from_value(cookies.clone())
                    .map_err(|error| format!("invalid cookies: {error}"))?;
            session.native_set_cookies(&parsed).await?;
            serialized_result_mode(&json!({"ok": true}), response_mode)
        }
        ToolInvocation::ClearCookies => {
            session.native_clear_cookies().await?;
            serialized_result_mode(&json!({"ok": true}), response_mode)
        }
        ToolInvocation::LocalStorage => {
            native_storage_result(
                session,
                crate::browser_backend::StorageScope::Local,
                response_mode,
            )
            .await
        }
        ToolInvocation::SessionStorage => {
            native_storage_result(
                session,
                crate::browser_backend::StorageScope::Session,
                response_mode,
            )
            .await
        }
        ToolInvocation::ExportCheckpoint => serialized_result_mode(
            &session.native_export_checkpoint(profile, policy).await?,
            response_mode,
        ),
        ToolInvocation::ImportCheckpoint { checkpoint } => {
            let checkpoint: crate::browser::session::CheckpointV1 =
                serde_json::from_value(checkpoint)
                    .map_err(|error| format!("invalid checkpoint: {error}"))?;
            session.native_import_checkpoint(&checkpoint).await?;
            serialized_result_mode(&json!({"status": "checkpoint_imported"}), response_mode)
        }
        ToolInvocation::Diagnostics { duration_ms } => serialized_result_mode(
            &session
                .native_diagnostics(Duration::from_millis(duration_ms))
                .await?,
            response_mode,
        ),
        ToolInvocation::DismissConsent => {
            policy.require(crate::browser::policy::PolicyCapability::ConsentDismissal)?;
            serialized_result_mode(&session.native_dismiss_consent().await?, response_mode)
        }
        ToolInvocation::PrintToPdf { options } => {
            let options: crate::browser::session::PdfOptions = serde_json::from_value(options)
                .map_err(|error| format!("invalid PDF options: {error}"))?;
            serialized_result_mode(&session.native_print_to_pdf(&options).await?, response_mode)
        }
        ToolInvocation::FillForm {
            fields,
            expected_revision,
        } => {
            let fields = fields
                .iter()
                .map(|(target, value)| (target.as_str(), value.as_str()))
                .collect::<Vec<_>>();
            serialized_result_mode(
                &session.native_fill_form(&fields, expected_revision).await?,
                response_mode,
            )
        }
        ToolInvocation::ClipboardRead => serialized_result_mode(
            &serde_json::json!({"text": session.native_clipboard_read().await?}),
            response_mode,
        ),
        ToolInvocation::ClipboardWrite { text } => {
            session.native_clipboard_write(&text).await?;
            serialized_result_mode(&serde_json::json!({"ok": true}), response_mode)
        }
        _ => Err(
            "native MCP does not implement this tool in the current native session slice".into(),
        ),
    }
}

#[cfg(feature = "native-engine")]
pub(crate) async fn run_native_persistent_tool(
    params: Value,
    session: &BrowserRuntimeSession,
    profile: &str,
    policy: &BrowserPolicy,
    pause_navigation_timeout_for_dialog: bool,
) -> BrowserResult<Value> {
    let response_mode = response_mode_from_params(&params)?;
    let invocation = parse_tool_invocation(&params)?;
    call_native_tool_on_session(
        invocation,
        session,
        profile,
        policy,
        response_mode,
        pause_navigation_timeout_for_dialog,
    )
    .await
}

#[cfg(feature = "native-engine")]
async fn native_action_result(
    session: &BrowserRuntimeSession,
    action: crate::browser_backend::SemanticAction,
    expected_revision: Option<u64>,
    response_mode: ResponseMode,
) -> BrowserResult<Value> {
    let result = match expected_revision {
        Some(expected_revision) => {
            session
                .action_with_revision(action, expected_revision)
                .await?
        }
        None => session.action(action).await?,
    };
    serialized_result_mode(&result, response_mode)
}

#[cfg(feature = "native-engine")]
async fn native_storage_result(
    session: &BrowserRuntimeSession,
    scope: crate::browser_backend::StorageScope,
    response_mode: ResponseMode,
) -> BrowserResult<Value> {
    let context_id = session
        .contexts()
        .await?
        .into_iter()
        .find(|context| context.active)
        .map(|context| context.context_id)
        .ok_or("native MCP returned no active context")?;
    let result = session
        .storage(crate::browser_backend::StorageRequest {
            context_id,
            scope,
            operation: crate::browser_backend::StorageOperation::Read,
        })
        .await?;
    serialized_result_mode(&result, response_mode)
}

#[cfg(feature = "native-engine")]
async fn ensure_native_session<'a>(
    session: &'a mut Option<BrowserRuntimeSession>,
    viewport: Option<(i64, i64)>,
    profile: &str,
    incognito: bool,
    modal_dialogs: bool,
) -> BrowserResult<&'a mut BrowserRuntimeSession> {
    if session.is_none() {
        let viewport = viewport
            .map(|(width, height)| -> BrowserResult<Viewport> {
                Ok(Viewport {
                    width: u32::try_from(width)
                        .map_err(|_| "native MCP viewport width must be a positive u32")?,
                    height: u32::try_from(height)
                        .map_err(|_| "native MCP viewport height must be a positive u32")?,
                    device_scale_factor_milli: 1000,
                })
            })
            .transpose()?;
        let mut config = match viewport {
            Some(viewport) => NativeEngineConfig::default().with_viewport(viewport),
            None => NativeEngineConfig::default(),
        };
        if !incognito {
            config =
                config.with_storage_path(crate::cli::runner::native_profile_storage_path(profile)?);
        }
        *session = Some(if modal_dialogs {
            BrowserRuntimeSession::connect_native_with_modal_dialogs(config).await?
        } else {
            BrowserRuntimeSession::connect_native(config).await?
        });
    }
    Ok(session.as_mut().expect("native session initialized"))
}

#[cfg(feature = "native-engine")]
fn native_mcp_point_target(x: f64, y: f64) -> BrowserResult<String> {
    if !x.is_finite()
        || !y.is_finite()
        || x < 0.0
        || y < 0.0
        || x.fract() != 0.0
        || y.fract() != 0.0
    {
        return Err("native MCP clickAt coordinates must be finite unsigned integers".into());
    }
    Ok(format!("point={x:.0},{y:.0}"))
}

#[cfg(feature = "native-engine")]
fn native_mcp_scroll_deltas(dx: f64, dy: f64) -> BrowserResult<(i32, i32)> {
    if !dx.is_finite()
        || !dy.is_finite()
        || dx.fract() != 0.0
        || dy.fract() != 0.0
        || dx < f64::from(i32::MIN)
        || dx > f64::from(i32::MAX)
        || dy < f64::from(i32::MIN)
        || dy > f64::from(i32::MAX)
    {
        return Err("native MCP scroll deltas must be finite 32-bit integers".into());
    }
    Ok((dx as i32, dy as i32))
}

#[cfg(feature = "native-engine")]
fn native_predicate_uses_javascript(predicate: &VerificationPredicate) -> bool {
    match predicate {
        VerificationPredicate::All { all } => all.iter().any(native_predicate_uses_javascript),
        VerificationPredicate::Any { any } => any.iter().any(native_predicate_uses_javascript),
        VerificationPredicate::Not { not } => native_predicate_uses_javascript(not),
        VerificationPredicate::UrlEquals { .. }
        | VerificationPredicate::TitleContains { .. }
        | VerificationPredicate::Visible { .. }
        | VerificationPredicate::TextContains { .. }
        | VerificationPredicate::PopupOpened { .. }
        | VerificationPredicate::DialogOpen { .. }
        | VerificationPredicate::DownloadStarted { .. }
        | VerificationPredicate::RevisionEquals { .. } => false,
    }
}

async fn ensure_session<'a>(
    session: &'a mut Option<CdpBrowserSession>,
    options: &SessionOptions,
    policy: &BrowserPolicy,
    viewport: Option<(i64, i64)>,
) -> BrowserResult<&'a mut CdpBrowserSession> {
    if session.is_none() {
        *session = Some(
            CdpBrowserSession::start_with_policy_and_viewport(options, policy.clone(), viewport)
                .await?,
        );
    }
    Ok(session.as_mut().expect("session initialized"))
}

fn tools() -> Vec<Tool> {
    let tools = vec![
        Tool {
            name: "inspectWebIr",
            description: "Inspect a validated browser-free Glass Web IR v1 without starting Chrome.",
            input_schema: json!({
                "type": "object",
                "properties": {"ir": {"type": "object", "description": "Bounded Glass Web IR v1 JSON."}},
                "required": ["ir"],
                "additionalProperties": false
            }),
        },
        Tool {
            name: "validateWebIr",
            description: "Validate a browser-free Glass Web IR v1 without starting Chrome.",
            input_schema: json!({
                "type": "object",
                "properties": {"ir": {"type": "object", "description": "Bounded Glass Web IR v1 JSON."}},
                "required": ["ir"],
                "additionalProperties": false
            }),
        },
        Tool {
            name: "diffWebIr",
            description: "Return bounded revision-change counts for two validated Web IR documents without starting Chrome.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "before": {"type": "object", "description": "Earlier bounded Glass Web IR v1 JSON."},
                    "after": {"type": "object", "description": "Later bounded Glass Web IR v1 JSON."}
                },
                "required": ["before", "after"],
                "additionalProperties": false
            }),
        },
        Tool {
            name: "continuityWebIr",
            description: "Classify one entity across two validated Web IR revisions without starting Chrome.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "before": {"type": "object", "description": "Earlier bounded Glass Web IR v1 JSON."},
                    "after": {"type": "object", "description": "Later bounded Glass Web IR v1 JSON."},
                    "entityId": {"type": "string", "description": "Revision-local entity ID from the earlier Web IR."}
                },
                "required": ["before", "after", "entityId"],
                "additionalProperties": false
            }),
        },
        Tool {
            name: "validateTask",
            description: "Validate a semantic Task Protocol task without starting Chrome or compiling a plan.",
            input_schema: json!({
                "type": "object",
                "properties": {"task": {"type": "object", "description": "Strict Task Protocol v1 authored task."}},
                "required": ["task"],
                "additionalProperties": false
            }),
        },
        Tool {
            name: "compileTask",
            description: "Compile a validated Task Protocol task against stable Glass Web IR v1.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "task": {"type": "object", "description": "Strict Task Protocol v1 authored task."},
                    "ir": {"type": "object", "description": "Validated stable Glass Web IR v1 source document."}
                },
                "required": ["task", "ir"],
                "additionalProperties": false
            }),
        },
        Tool {
            name: "executeTask",
            description: "Execute a confirmed, revision-guarded Task Protocol v1 task from any validated browser-backed family in the current browser session.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "task": {"type": "object", "description": "Validated Task Protocol v1 authored task from a form, navigation, dialog, pagination, extraction, or field-read family."},
                    "expectedRevision": {"type": "integer", "minimum": 0, "description": "Revision returned by the caller's preceding semantic observation."},
                    "confirmed": {"type": "boolean", "default": false, "description": "Explicit confirmation for risky or ambiguity-gated tasks."},
                    "leaseToken": {"type": "string", "minLength": 1, "maxLength": 256, "description": "Mutation lease token issued by glass/lease/acquire when running against a daemon."},
                    "responseMode": {"type": "string", "enum": ["minimal", "normal", "diagnostic"], "default": "minimal"}
                },
                "required": ["task", "expectedRevision"],
                "additionalProperties": false
            }),
        },
        Tool {
            name: "preflightNavigation",
            description: "Check navigation URL policy without starting Chrome or consuming confirmation tokens.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "url": {"type": "string", "minLength": 1, "maxLength": 8192}
                },
                "required": ["url"],
                "additionalProperties": false
            }),
        },
        Tool {
            name: "navigate",
            description: "Navigate the browser to a URL.",
            input_schema: json!({
                "type": "object",
                "properties": {"url": {"type": "string"}, "timeoutMs": {"type":"integer", "minimum":1, "maximum":300000, "default":20000}, "expectedRevision":{"type":"integer","minimum":0}, "includeTrace": {"type":"boolean", "default":false}},
                "required": ["url"]
            }),
        },
        Tool {
            name: "click",
            description: "Click one uniquely resolved ref/name/role+name/text/CSS/ordinal locator.",
            input_schema: json!({
                "type": "object",
                "properties": {"target": {"type": "string"}, "selector": {"type": "string"}, "expectedRevision":{"type":"integer","minimum":0}, "includeTrace": {"type":"boolean", "default":false}},
                "anyOf": [{"required": ["target"]}, {"required": ["selector"]}]
            }),
        },
        Tool {
            name: "clickExpectPopup",
            description: "Click one target and return exactly one causally verified popup without selecting it.",
            input_schema: json!({
                "type": "object",
                "properties": {"target": {"type": "string"}, "selector": {"type": "string"}, "expectedRevision":{"type":"integer","minimum":0}, "includeTrace": {"type":"boolean", "default":false}},
                "anyOf": [{"required": ["target"]}, {"required": ["selector"]}]
            }),
        },
        Tool {
            name: "doubleClick",
            description: "Double-click one uniquely resolved ref/name/role+name/text/CSS/ordinal locator.",
            input_schema: json!({
                "type": "object",
                "properties": {"target": {"type": "string"}, "selector": {"type": "string"}, "expectedRevision":{"type":"integer","minimum":0}, "includeTrace": {"type":"boolean", "default":false}},
                "anyOf": [{"required": ["target"]}, {"required": ["selector"]}]
            }),
        },
        Tool {
            name: "hover",
            description: "Move the pointer over one actionable target.",
            input_schema: guarded_target_schema(),
        },
        Tool {
            name: "drag",
            description: "Drag one actionable target to another.",
            input_schema: json!({"type":"object","properties":{"source":{"type":"string"},"destination":{"type":"string"},"expectedRevision":{"type":"integer","minimum":0},"includeTrace":{"type":"boolean","default":false}},"required":["source","destination"]}),
        },
        Tool {
            name: "key",
            description: "Dispatch a complete browser key press.",
            input_schema: guarded_string_schema("key"),
        },
        Tool {
            name: "keyDown",
            description: "Dispatch a browser key-down event.",
            input_schema: guarded_string_schema("key"),
        },
        Tool {
            name: "keyUp",
            description: "Dispatch a browser key-up event.",
            input_schema: guarded_string_schema("key"),
        },
        Tool {
            name: "shortcut",
            description: "Dispatch one explicit modifier shortcut.",
            input_schema: guarded_string_schema("shortcut"),
        },
        Tool {
            name: "clear",
            description: "Clear one actionable editable target.",
            input_schema: guarded_target_schema(),
        },
        Tool {
            name: "check",
            description: "Ensure one checkbox or radio is checked.",
            input_schema: guarded_target_schema(),
        },
        Tool {
            name: "uncheck",
            description: "Ensure one checkbox is unchecked.",
            input_schema: guarded_target_schema(),
        },
        Tool {
            name: "select",
            description: "Select one exact option value.",
            input_schema: json!({"type":"object","properties":{"target":{"type":"string"},"value":{"type":"string"},"expectedRevision":{"type":"integer","minimum":0},"includeTrace":{"type":"boolean","default":false}},"required":["target","value"]}),
        },
        Tool {
            name: "upload",
            description: "Set bounded local regular files on one file input; contents are never returned.",
            input_schema: json!({"type":"object","properties":{"target":{"type":"string"},"files":{"type":"array","minItems":1,"maxItems":16,"items":{"type":"string"}},"expectedRevision":{"type":"integer","minimum":0},"includeTrace":{"type":"boolean","default":false}},"required":["target","files"]}),
        },
        Tool {
            name: "type",
            description: "Insert text into the focused element, optionally clicking a target.",
            input_schema: json!({
                "type": "object",
                "properties": {"text": {"type": "string"}, "target": {"type": "string"}, "expectedRevision":{"type":"integer","minimum":0}, "includeTrace": {"type":"boolean","default":false}},
                "required": ["text"]
            }),
        },
        Tool {
            name: "screenshot",
            description: "Capture explicit viewport, clip, element, or full-page visual evidence with metadata.",
            input_schema: json!({"type":"object","properties":{
                "format":{"type":"string","enum":["png","jpeg","webp"],"default":"png"},
                "quality":{"type":"integer","minimum":0,"maximum":100},
                "scale":{"type":"number","minimum":0.1,"maximum":4.0,"default":1.0},
                "fullPage":{"type":"boolean","default":false},
                "clip":{"type":"object","properties":{"x":{"type":"number"},"y":{"type":"number"},"width":{"type":"number"},"height":{"type":"number"}},"required":["x","y","width","height"]},
                "target":{"type":"string"}, "includeTrace":{"type":"boolean","default":false}
            }}),
        },
        Tool {
            name: "observe",
            description: "Return compact accessibility and visible-text context; full DOM, screenshots, and form values are opt-in.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "includeDom": {"type": "boolean", "default": false},
                    "includeScreenshot": {"type": "boolean", "default": false},
                    "includeFormValues": {"type": "boolean", "default": false},
                    "level": {"type": "string", "enum": ["summary", "interactive", "structured", "detailed", "raw"]},
                    "region": {"type": "string", "minLength": 1, "maxLength": 128}
                }
            }),
        },
        Tool {
            name: "observeBootstrap",
            description: "Return bounded advisory page URL, title, readiness, visible text, revision, and consistency evidence without action targets.",
            input_schema: json!({
                "type": "object",
                "additionalProperties": false,
                "properties": {
                    "responseMode": {"type": "string", "enum": ["minimal", "normal", "diagnostic"], "default": "minimal"}
                }
            }),
        },
        Tool {
            name: "inspectPage",
            description: "Return one bounded semantic page inspection with revision and route provenance.",
            input_schema: json!({
                "type":"object",
                "additionalProperties":false,
                "properties":{"responseMode":{"type":"string","enum":["minimal","normal","diagnostic"],"default":"minimal"}}
            }),
        },
        Tool {
            name: "extractWebIr",
            description: "Extract the live active page into bounded Glass Web IR v1 using the selected browser runtime.",
            input_schema: json!({
                "type": "object",
                "additionalProperties": false,
                "properties": {
                    "request": {
                        "type": "object",
                        "description": "Optional ExtractionRequest with scope, evidence sources, and hard budgets. Omit for the bounded document default."
                    },
                    "responseMode": {"type": "string", "enum": ["minimal", "normal", "diagnostic"], "default": "minimal"}
                }
            }),
        },
        Tool {
            name: "findTarget",
            description: "Resolve one declared intent into bounded candidates without dispatching an action.",
            input_schema: json!({
                "type":"object",
                "additionalProperties":false,
                "required":["schemaVersion","intent","action","resolutionPolicy"],
                "properties":{
                    "schemaVersion":{"const":1},
                    "intent":{"type":"string","minLength":1,"maxLength":512},
                    "action":{"type":"string"},
                    "scope":{"type":"object"},
                    "constraints":{"type":"object"},
                    "resolutionPolicy":{"type":"string"},
                    "expectedRevision":{"type":"integer","minimum":0},
                    "responseMode":{"type":"string","enum":["minimal","normal","diagnostic"],"default":"minimal"}
                }
            }),
        },
        Tool {
            name: "actAndVerify",
            description: "Execute one explicit semantic intent and return bounded verification evidence.",
            input_schema: json!({
                "type":"object",
                "additionalProperties":false,
                "required":["schemaVersion","intent","action","resolutionPolicy","candidateId"],
                "properties":{
                    "schemaVersion":{"const":1},
                    "intent":{"type":"string","minLength":1,"maxLength":512},
                    "action":{"type":"string"},
                    "scope":{"type":"object"},
                    "constraints":{"type":"object"},
                    "resolutionPolicy":{"type":"string"},
                    "candidateId":{"type":"string"},
                    "expectedRevision":{"type":"integer","minimum":0},
                    "predicate":{"type":"object"},
                    "timeoutMs":{"type":"integer","minimum":1,"maximum":300000,"default":10000},
                    "leaseToken":{"type":"string","minLength":1,"maxLength":256},
                    "responseMode":{"type":"string","enum":["minimal","normal","diagnostic"],"default":"minimal"}
                }
            }),
        },
        Tool {
            name: "extractStructured",
            description: "Extract bounded typed fields from a fresh semantic page or region; secret-like fields require the explicit read_sensitive_extraction capability.",
            input_schema: json!({
                "type":"object",
                "additionalProperties":false,
                "required":["fields"],
                "properties":{
                    "fields":{
                        "type":"array",
                        "minItems":1,
                        "maxItems":32,
                        "items":{
                            "type":"object",
                            "additionalProperties":false,
                            "required":["name","path","kind"],
                            "properties":{
                                "name":{"type":"string","minLength":1,"maxLength":128},
                                "path":{"type":"string","minLength":1,"maxLength":512},
                                "kind":{
                                    "type":"string",
                                    "enum":[
                                        "scalar","optionalScalar","string","optionalString",
                                        "number","currency","date","dateTime","boolean","url",
                                        "enum","list","record","object","table","repeatedItems"
                                    ]
                                }
                            }
                        }
                    },
                    "regionId":{"type":"string","minLength":1,"maxLength":128},
                    "maxItems":{"type":"integer","minimum":1,"maximum":256,"default":64},
                    "startIndex":{"type":"integer","minimum":0,"maximum":256,"default":0},
                    "continuation":{
                        "type":"object",
                        "additionalProperties":false,
                        "required":["nextIndex","sourceRevision","sourceRoute","contractHash"],
                        "properties":{
                            "nextIndex":{"type":"integer","minimum":0,"maximum":256},
                            "sourceRevision":{"type":"integer","minimum":0},
                            "contractHash":{"type":"string","minLength":71,"maxLength":71},
                            "regionId":{"type":"string","minLength":1,"maxLength":128},
                            "sourceRoute":{
                                "type":"object",
                                "additionalProperties":false,
                                "required":["targetId","frameId","url"],
                                "properties":{
                                    "targetId":{"type":"string","minLength":1,"maxLength":128},
                                    "frameId":{"type":"string","minLength":1,"maxLength":128},
                                    "url":{"type":"string","minLength":1,"maxLength":2048}
                                }
                            }
                        }
                    },
                    "maxBytes":{"type":"integer","minimum":1,"maximum":262144,"default":65536},
                    "responseMode":{"type":"string","enum":["minimal","normal","diagnostic"],"default":"minimal"}
                }
            }),
        },
        Tool {
            name: "recoverRun",
            description: "Return conservative recovery guidance for an indeterminate execution.",
            input_schema: json!({
                "type":"object",
                "additionalProperties":false,
                "required":["executionId"],
                "properties":{
                    "executionId":{"type":"string","minLength":1,"maxLength":128},
                    "responseMode":{"type":"string","enum":["minimal","normal","diagnostic"],"default":"minimal"}
                }
            }),
        },
        Tool {
            name: "sessionSnapshot",
            description: "Create, list, inspect, diff, or purge redacted bounded session snapshots.",
            input_schema: json!({
                "type":"object",
                "additionalProperties":false,
                "properties":{
                    "operation":{"type":"string","enum":["create","list","inspect","diff","purge"],"default":"list"},
                    "from":{"type":"string"},
                    "to":{"type":"string"},
                    "responseMode":{"type":"string","enum":["minimal","normal","diagnostic"],"default":"minimal"}
                }
            }),
        },
        Tool {
            name: "observeKnowledge",
            description: "Collect fresh semantic evidence and optionally assess scoped local knowledge; stored knowledge never authorizes an action.",
            input_schema: json!({
                "type": "object",
                "additionalProperties": false,
                "properties": {
                    "level": {"type": "string", "enum": ["summary", "interactive", "structured", "detailed", "raw"], "default": "summary"},
                    "freshOnly": {"type": "boolean", "default": false},
                    "profileScope": {"type": "string", "enum": ["anonymous", "authenticated", "profileBound"], "default": "profileBound"},
                    "profileKey": {"type": "string", "minLength": 1, "maxLength": 256},
                    "locale": {"type": "string", "minLength": 1, "maxLength": 256},
                    "tenantKey": {"type": "string", "minLength": 1, "maxLength": 256},
                    "browserFamily": {"type": "string", "minLength": 1, "maxLength": 256, "default": "chromium"},
                    "browserVersion": {"type": "string", "minLength": 1, "maxLength": 256}
                }
            }),
        },
        Tool {
            name: "resolveIntent",
            description: "Resolve declared browser intent into bounded, inspectable candidates without dispatching an action.",
            input_schema: json!({
                "type": "object",
                "additionalProperties": false,
                "required": ["schemaVersion", "intent", "action", "resolutionPolicy"],
                "properties": {
                    "schemaVersion": {"const": 1},
                    "intent": {"type": "string", "minLength": 1, "maxLength": 512},
                    "action": {"type": "string", "enum": ["click", "type", "clear", "check", "uncheck", "select", "submit", "open", "close", "search", "filter", "sort", "paginate", "toggle", "expand", "collapse", "download", "upload", "inspect", "extract"]},
                    "scope": {"type": "object"},
                    "constraints": {"type": "object"},
                    "resolutionPolicy": {"type": "string", "enum": ["reportOnly", "requireExact", "requireUniqueHighConfidence", "allowUniqueMediumConfidence", "interactiveConfirmation"]},
                    "expectedRevision": {"type": "integer", "minimum": 0}
                }
            }),
        },
        Tool {
            name: "resolveIntentWithKnowledge",
            description: "Resolve declared intent against fresh candidates with eligible local fingerprints as secondary evidence; never dispatches an action.",
            input_schema: json!({
                "type": "object",
                "additionalProperties": false,
                "required": ["schemaVersion", "intent", "action", "resolutionPolicy"],
                "properties": {
                    "schemaVersion": {"const": 1},
                    "intent": {"type": "string", "minLength": 1, "maxLength": 512},
                    "action": {"type": "string", "enum": ["click", "type", "clear", "check", "uncheck", "select", "submit", "open", "close", "search", "filter", "sort", "paginate", "toggle", "expand", "collapse", "download", "upload", "inspect", "extract"]},
                    "scope": {"type": "object"},
                    "constraints": {"type": "object"},
                    "resolutionPolicy": {"type": "string", "enum": ["reportOnly", "requireExact", "requireUniqueHighConfidence", "allowUniqueMediumConfidence", "interactiveConfirmation"]},
                    "expectedRevision": {"type": "integer", "minimum": 0},
                    "profileScope": {"type": "string", "enum": ["anonymous", "authenticated", "profileBound"], "default": "profileBound"},
                    "profileKey": {"type": "string", "minLength": 1, "maxLength": 256},
                    "locale": {"type": "string", "minLength": 1, "maxLength": 256},
                    "tenantKey": {"type": "string", "minLength": 1, "maxLength": 256},
                    "browserFamily": {"type": "string", "minLength": 1, "maxLength": 256, "default": "chromium"},
                    "browserVersion": {"type": "string", "minLength": 1, "maxLength": 256}
                }
            }),
        },
        Tool {
            name: "executeIntent",
            description: "Re-resolve one declared intent and execute only the explicitly selected candidate through revision-guarded actions.",
            input_schema: json!({
                "type": "object",
                "additionalProperties": false,
                "required": ["schemaVersion", "intent", "action", "resolutionPolicy", "candidateId"],
                "properties": {
                    "schemaVersion": {"const": 1},
                    "intent": {"type": "string", "minLength": 1, "maxLength": 512},
                    "action": {"type": "string", "enum": ["click", "type", "clear", "check", "uncheck", "select", "submit", "open", "close", "search", "filter", "sort", "paginate", "expand", "collapse"]},
                    "scope": {"type": "object"},
                    "constraints": {"type": "object"},
                    "resolutionPolicy": {"type": "string", "enum": ["reportOnly", "requireExact", "requireUniqueHighConfidence", "allowUniqueMediumConfidence", "interactiveConfirmation"]},
                    "expectedRevision": {"type": "integer", "minimum": 0},
                    "candidateId": {"type": "string", "minLength": 1, "maxLength": 128},
                    "value": {"type": "string", "maxLength": 4096},
                    "leaseToken": {"type": "string", "minLength": 1, "maxLength": 256}
                }
            }),
        },
        Tool {
            name: "workspaceStatus",
            description: "List persisted workspace identities and lifecycle state without browser side effects.",
            input_schema: json!({"type":"object","additionalProperties":false,"properties":{}}),
        },
        Tool {
            name: "workspaceInspect",
            description: "Inspect one persisted workspace by normalized identity.",
            input_schema: json!({"type":"object","additionalProperties":false,"properties":{"id":{"type":"string"}}}),
        },
        Tool {
            name: "surfaceInspect",
            description: "Validate surface evidence and report semantic coverage and provenance.",
            input_schema: json!({"type":"object","additionalProperties":false,"required":["surfaces"],"properties":{"surfaces":{"type":"object"},"coverageOnly":{"type":"boolean","default":false}}}),
        },
        Tool {
            name: "backendStatus",
            description: "Validate a transport-neutral backend profile and report declared capability evidence.",
            input_schema: json!({"type":"object","additionalProperties":false,"required":["profile"],"properties":{"profile":{"type":"object"}}}),
        },
        Tool {
            name: "backendTest",
            description: "Exercise fail-closed capability declarations in a backend profile.",
            input_schema: json!({"type":"object","additionalProperties":false,"required":["profile"],"properties":{"profile":{"type":"object"}}}),
        },
        Tool {
            name: "replayInspect",
            description: "Validate a redacted replay bundle against its exact scenario without starting a browser.",
            input_schema: json!({"type":"object","additionalProperties":false,"required":["scenario","replay"],"properties":{"scenario":{"type":"object"},"replay":{"type":"object"}}}),
        },
        Tool {
            name: "replayDiff",
            description: "Compare two bounded redacted replays bound to one exact scenario without browser side effects.",
            input_schema: json!({"type":"object","additionalProperties":false,"required":["scenario","before","after"],"properties":{"scenario":{"type":"object"},"before":{"type":"object"},"after":{"type":"object"}}}),
        },
        Tool {
            name: "replayAttach",
            description: "Validate and attach one bounded redacted replay reference without starting or mutating a browser.",
            input_schema: json!({"type":"object","additionalProperties":false,"required":["scenario","replay"],"properties":{"scenario":{"type":"object"},"replay":{"type":"object"}}}),
        },
        Tool {
            name: "memoryStatus",
            description: "Report bounded advisory memory lifecycle counts.",
            input_schema: json!({"type":"object","additionalProperties":false,"properties":{}}),
        },
        Tool {
            name: "memoryInspect",
            description: "Inspect one advisory memory record and provenance.",
            input_schema: json!({"type":"object","additionalProperties":false,"required":["recordId"],"properties":{"recordId":{"type":"string"}}}),
        },
        Tool {
            name: "memoryExplain",
            description: "Explain why an advisory memory record cannot authorize a mutation.",
            input_schema: json!({"type":"object","additionalProperties":false,"required":["recordId"],"properties":{"recordId":{"type":"string"}}}),
        },
        Tool {
            name: "memoryForget",
            description: "Forget one advisory memory record.",
            input_schema: json!({"type":"object","additionalProperties":false,"required":["recordId"],"properties":{"recordId":{"type":"string"}}}),
        },
        Tool {
            name: "memoryExport",
            description: "Export the validated advisory memory snapshot.",
            input_schema: json!({"type":"object","additionalProperties":false,"properties":{}}),
        },
        Tool {
            name: "memoryPrune",
            description: "Prune stale, contradicted, and quarantined advisory records.",
            input_schema: json!({"type":"object","additionalProperties":false,"properties":{}}),
        },
        Tool {
            name: "memoryReindex",
            description: "Refresh and validate the advisory memory snapshot from disk.",
            input_schema: json!({"type":"object","additionalProperties":false,"properties":{}}),
        },
        Tool {
            name: "knowledgeList",
            description: "List persistent, profile-scoped knowledge records without starting or inspecting a browser.",
            input_schema: json!({
                "type": "object",
                "additionalProperties": false,
                "properties": {}
            }),
        },
        Tool {
            name: "knowledgeShow",
            description: "Show one persistent knowledge record and its bounded provenance; records never authorize browser mutations.",
            input_schema: json!({
                "type": "object",
                "additionalProperties": false,
                "required": ["recordId"],
                "properties": {
                    "recordId": {"type": "string", "minLength": 1, "maxLength": 128}
                }
            }),
        },
        Tool {
            name: "knowledgeStats",
            description: "Report persistent knowledge-store counts and serialized size without starting a browser.",
            input_schema: json!({
                "type": "object",
                "additionalProperties": false,
                "properties": {}
            }),
        },
        Tool {
            name: "knowledgeInvalidate",
            description: "Move one persistent knowledge record to stale, contradicted, or quarantined state.",
            input_schema: json!({
                "type": "object",
                "additionalProperties": false,
                "required": ["recordId", "state"],
                "properties": {
                    "recordId": {"type": "string", "minLength": 1, "maxLength": 128},
                    "state": {"type": "string", "enum": ["stale", "contradicted", "quarantined"]},
                    "reason": {"type": "string", "maxLength": 256},
                    "observedAt": {"type": "string", "maxLength": 64},
                    "leaseToken": {"type": "string", "minLength": 1, "maxLength": 256}
                }
            }),
        },
        Tool {
            name: "knowledgePurge",
            description: "Purge persistent knowledge records for one exact origin after policy checks.",
            input_schema: json!({
                "type": "object",
                "additionalProperties": false,
                "required": ["origin"],
                "properties": {
                    "origin": {"type": "string", "minLength": 1, "maxLength": 2048},
                    "leaseToken": {"type": "string", "minLength": 1, "maxLength": 256}
                }
            }),
        },
        Tool {
            name: "preflight",
            description: "Dry-run target resolution and clickability without pointer events, focus, scrolling, or revision changes.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "target": {"type": "string"},
                    "action": {"type": "string", "enum": ["click", "hover", "type", "check", "select"], "default": "click"}
                },
                "required": ["target"]
            }),
        },
        Tool {
            name: "clickAt",
            description: "Click exact viewport coordinates for canvas or map surfaces; policy-gated and never retargeted.",
            input_schema: json!({
                "type": "object",
                "properties": {"x": {"type": "number"}, "y": {"type": "number"}},
                "required": ["x", "y"]
            }),
        },
        Tool {
            name: "getDOM",
            description: "Return the full DOM tree. This is an explicit deep-inspection request.",
            input_schema: json!({"type": "object", "properties": {}}),
        },
        Tool {
            name: "getText",
            description: "Return visible text from the current page.",
            input_schema: json!({"type": "object", "properties": {}}),
        },
        Tool {
            name: "reconcileReferences",
            description: "Reconcile prior revisioned refs against the current page to find Preserved/Relocated/Lost mappings.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "fromRevision": {"type": "integer", "minimum": 0},
                    "refs": {"type": "array", "items": {"type": "string"}, "maxItems": 16},
                    "hints": {"type": "array", "items": {"type": "string"}, "maxItems": 8},
                    "scopeRef": {"type": "string"}
                },
                "required": ["fromRevision", "refs"]
            }),
        },
        Tool {
            name: "observeDelta",
            description: "Compare the last compact observation with a fresh same-route observation using bounded added/removed/changed controls.",
            input_schema: json!({"type": "object", "properties": {}}),
        },
        Tool {
            name: "setNetworkConditions",
            description: "Apply bounded session-scoped network conditions using slow-3g, fast-3g, offline, or explicit values.",
            input_schema: json!({"type":"object","properties":{
                "preset":{"type":"string","enum":["slow-3g","fast-3g","offline"]},
                "offline":{"type":"boolean"},"latencyMs":{"type":"number"},
                "downloadThroughput":{"type":"number"},"uploadThroughput":{"type":"number"}
            }}),
        },
        Tool {
            name: "clearNetworkConditions",
            description: "Reset session network conditions.",
            input_schema: json!({"type":"object","properties":{}}),
        },
        Tool {
            name: "setCpuThrottling",
            description: "Set a bounded session CPU throttling multiplier.",
            input_schema: json!({"type":"object","properties":{"rate":{"type":"number","exclusiveMinimum":0,"maximum":20}},"required":["rate"]}),
        },
        Tool {
            name: "clearCpuThrottling",
            description: "Reset CPU throttling to 1x.",
            input_schema: json!({"type":"object","properties":{}}),
        },
        Tool {
            name: "setUserAgent",
            description: "Apply a declared session user-agent and optional Accept-Language/platform override.",
            input_schema: json!({"type":"object","properties":{"userAgent":{"type":"string","maxLength":512},"acceptLanguage":{"type":"string","maxLength":128},"platform":{"type":"string","maxLength":128}},"required":["userAgent"]}),
        },
        Tool {
            name: "clearUserAgent",
            description: "Restore the user agent captured before Glass's override.",
            input_schema: json!({"type":"object","properties":{}}),
        },
        Tool {
            name: "exportCheckpoint",
            description: "Export a session checkpoint (≤ 4 KiB) for cross-process resume.",
            input_schema: json!({"type": "object", "properties": {}}),
        },
        Tool {
            name: "importCheckpoint",
            description: "Import a checkpoint and restore target/frame context.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "checkpoint": {"type": "object"}
                },
                "required": ["checkpoint"]
            }),
        },
        Tool {
            name: "evaluate",
            description: "Evaluate JavaScript in the current page.",
            input_schema: json!({
                "type": "object",
                "properties": {"expression": {"type": "string"}},
                "required": ["expression"]
            }),
        },
        Tool {
            name: "batch",
            description: "Execute an ordered batch of typed operations (max 32 steps). Policy is pre-flighted before any step runs.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "atomic": {"type": "boolean", "default": false},
                    "mode": {"type": "string", "enum": ["fixed", "chain", "unguarded"], "default": "unguarded"},
                    "expectedRevision": {"type": "integer", "minimum": 0},
                    "steps": {
                        "type": "array",
                        "items": {
                            "type": "object",
                            "properties": {
                                "action": {
                                    "type": "string",
                                    "enum": ["navigate", "click", "type", "check", "uncheck", "select", "clear", "scroll", "wait", "observe", "screenshot", "evaluate", "acceptDialog", "dismissDialog"]
                                }
                            },
                            "required": ["action"]
                        }
                    }
                },
                "required": ["steps"]
            }),
        },
        Tool {
            name: "workflow",
            description: "Validate and execute a declarative workflow with bounded states, terminal proof, typed outputs, and deterministic trace.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "workflow": {"type": "object"},
                    "inputs": {"type": "object"},
                    "checkpoint": {"type": "object"}
                },
                "required": ["workflow"]
            }),
        },
        Tool {
            name: "verify",
            description: "Evaluate a bounded URL, title, visibility, text, topology, dialog, download, revision, or boolean-composed predicate.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "predicate": {"type": "object"},
                    "timeoutMs": {"type": "integer", "minimum": 1, "maximum": 300000, "default": 10000}
                },
                "required": ["predicate"]
            }),
        },
        Tool {
            name: "scroll",
            description: "Scroll the page by CSS pixel deltas.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "dx": {"type": "number", "default": 0},
                    "dy": {"type": "number", "default": 600},
                    "expectedRevision": {"type": "integer", "minimum": 0}
                }
            }),
        },
        Tool {
            name: "wait",
            description: "Wait for one typed condition until an explicit deadline.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "condition": {"type": "string"},
                    "timeoutMs": {"type": "integer", "minimum": 1, "default": 10000},
                    "includeTrace": {"type":"boolean","default":false}
                },
                "required": ["condition"]
            }),
        },
        Tool {
            name: "diagnostics",
            description: "Collect bounded, redacted console and network evidence in an explicit scope.",
            input_schema: json!({"type":"object","properties":{"durationMs":{"type":"integer","minimum":1,"maximum":30000,"default":1000}}}),
        },
        Tool {
            name: "acceptDialog",
            description: "Accept the currently open JavaScript dialog.",
            input_schema: json!({"type":"object","properties":{}}),
        },
        Tool {
            name: "dismissDialog",
            description: "Dismiss the currently open JavaScript dialog.",
            input_schema: json!({"type":"object","properties":{}}),
        },
        Tool {
            name: "dismissConsent",
            description: "Dismiss a visible OneTrust or Cookiebot consent control; UX assistance only, never anti-bot bypass.",
            input_schema: json!({"type":"object","properties":{}}),
        },
        Tool {
            name: "download",
            description: "Wait for one download into an authorized existing directory.",
            input_schema: json!({"type":"object","properties":{"destination":{"type":"string"},"timeoutMs":{"type":"integer","minimum":1,"maximum":30000,"default":30000}},"required":["destination"]}),
        },
        Tool {
            name: "listTargets",
            description: "List bounded page targets without changing the active target.",
            input_schema: json!({"type": "object", "properties": {}}),
        },
        Tool {
            name: "createTarget",
            description: "Create a page target without selecting it.",
            input_schema: json!({"type":"object", "properties":{"url":{"type":"string"}}, "required":["url"]}),
        },
        Tool {
            name: "selectTarget",
            description: "Explicitly select the page target used by subsequent tools.",
            input_schema: json!({"type":"object", "properties":{"id":{"type":"string"}}, "required":["id"]}),
        },
        Tool {
            name: "closeTarget",
            description: "Close one page target; closing the active target leaves no implicit selection.",
            input_schema: json!({"type":"object", "properties":{"id":{"type":"string"}}, "required":["id"]}),
        },
        Tool {
            name: "listFrames",
            description: "List bounded frames in the active page target.",
            input_schema: json!({"type": "object", "properties": {}}),
        },
        Tool {
            name: "selectFrame",
            description: "Explicitly select the frame used by subsequent tools.",
            input_schema: json!({"type":"object", "properties":{"id":{"type":"string"}}, "required":["id"]}),
        },
        Tool {
            name: "cookies",
            description: "Read all browser cookies for the current page URL. Requires persistent profile.",
            input_schema: json!({"type":"object","properties":{}}),
        },
        Tool {
            name: "setCookies",
            description: "Set browser cookies. Requires persistent profile.",
            input_schema: json!({"type":"object","properties":{"cookies":{"type":"array"}},"required":["cookies"]}),
        },
        Tool {
            name: "clearCookies",
            description: "Clear all browser cookies. Requires persistent profile.",
            input_schema: json!({"type":"object","properties":{}}),
        },
        Tool {
            name: "localStorage",
            description: "Read localStorage items (bounded to 64 entries, 1 KiB per value). Requires persistent profile.",
            input_schema: json!({"type":"object","properties":{}}),
        },
        Tool {
            name: "sessionStorage",
            description: "Read sessionStorage items (bounded to 64 entries, 1 KiB per value). Requires persistent profile.",
            input_schema: json!({"type":"object","properties":{}}),
        },
        Tool {
            name: "printToPdf",
            description: "Generate a PDF of the current page. Returns base64-encoded data.",
            input_schema: json!({"type":"object","properties":{"paperWidth":{"type":"number"},"paperHeight":{"type":"number"},"printBackground":{"type":"boolean"}}}),
        },
        Tool {
            name: "fillForm",
            description: "Fill multiple form fields atomically (max 16). Resolves all locators first.",
            input_schema: json!({"type":"object","properties":{"fields":{"type":"array","items":{"type":"object","properties":{"target":{"type":"string"},"value":{"type":"string"}},"required":["target"]}},"expectedRevision":{"type":"integer","minimum":0}},"required":["fields"]}),
        },
        Tool {
            name: "clipboardRead",
            description: "Read text from the system clipboard. Returns up to 8 KiB.",
            input_schema: json!({"type":"object","properties":{}}),
        },
        Tool {
            name: "clipboardWrite",
            description: "Write text to the system clipboard. Truncated to 8 KiB.",
            input_schema: json!({"type":"object","properties":{"text":{"type":"string"}},"required":["text"]}),
        },
        Tool {
            name: "setGeolocation",
            description: "Override browser geolocation. Use clearGeolocation to reset.",
            input_schema: json!({"type":"object","properties":{"latitude":{"type":"number"},"longitude":{"type":"number"}},"required":["latitude","longitude"]}),
        },
        Tool {
            name: "clearGeolocation",
            description: "Clear geolocation override.",
            input_schema: json!({"type":"object","properties":{}}),
        },
        Tool {
            name: "setTimezone",
            description: "Override browser timezone (IANA ID like America/New_York).",
            input_schema: json!({"type":"object","properties":{"timezoneId":{"type":"string"}},"required":["timezoneId"]}),
        },
    ];
    tools
        .into_iter()
        .filter(|tool| !tool.name.starts_with("project.") && !tool.name.starts_with("agent."))
        .collect()
}

fn guarded_target_schema() -> Value {
    json!({"type":"object","properties":{"target":{"type":"string"},"expectedRevision":{"type":"integer","minimum":0},"includeTrace":{"type":"boolean","default":false}},"required":["target"]})
}

fn guarded_string_schema(name: &str) -> Value {
    json!({"type":"object","properties":{(name):{"type":"string"},"expectedRevision":{"type":"integer","minimum":0},"includeTrace":{"type":"boolean","default":false}},"required":[name]})
}

fn required_u64(arguments: &Value, name: &str) -> BrowserResult<u64> {
    arguments
        .get(name)
        .and_then(Value::as_u64)
        .ok_or_else(|| format!("{name} must be a non-negative integer").into())
}

fn required_string_array<'a>(arguments: &'a Value, name: &str) -> BrowserResult<Vec<&'a str>> {
    arguments
        .get(name)
        .and_then(Value::as_array)
        .filter(|arr| !arr.is_empty() && arr.len() <= 32)
        .ok_or_else(|| format!("{name} must be a non-empty array with at most 32 entries"))?
        .iter()
        .map(|v| {
            v.as_str()
                .filter(|s| !s.is_empty())
                .ok_or_else(|| format!("{name} entries must be non-empty strings").into())
        })
        .collect()
}

fn optional_string_array(
    arguments: &Value,
    name: &str,
    max_entries: usize,
) -> BrowserResult<Vec<String>> {
    let Some(value) = arguments.get(name) else {
        return Ok(Vec::new());
    };
    let values = value
        .as_array()
        .filter(|values| values.len() <= max_entries)
        .ok_or_else(|| format!("{name} must be an array with at most {max_entries} entries"))?;
    values
        .iter()
        .map(|value| {
            value
                .as_str()
                .filter(|entry| !entry.trim().is_empty())
                .map(str::to_string)
                .ok_or_else(|| format!("{name} entries must be non-empty strings").into())
        })
        .collect()
}

fn required_string<'a>(arguments: &'a Value, name: &str) -> BrowserResult<&'a str> {
    arguments
        .get(name)
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| format!("{name} must be a non-empty string").into())
}

fn required_path_array(arguments: &Value, name: &str) -> BrowserResult<Vec<std::path::PathBuf>> {
    let values = arguments
        .get(name)
        .and_then(Value::as_array)
        .filter(|values| !values.is_empty() && values.len() <= 16)
        .ok_or_else(|| format!("{name} must contain 1..=16 paths"))?;
    values
        .iter()
        .map(|value| {
            value
                .as_str()
                .filter(|path| !path.is_empty())
                .map(std::path::PathBuf::from)
                .ok_or_else(|| format!("{name} entries must be non-empty strings").into())
        })
        .collect()
}

fn required_target(arguments: &Value) -> BrowserResult<Cow<'_, str>> {
    if let Some(target) = optional_string(arguments, "target")? {
        return Ok(Cow::Borrowed(target));
    }
    Ok(Cow::Owned(format!(
        "css={}",
        required_string(arguments, "selector")?
    )))
}

fn optional_string<'a>(arguments: &'a Value, name: &str) -> BrowserResult<Option<&'a str>> {
    match arguments.get(name) {
        None => Ok(None),
        Some(Value::String(value)) if !value.trim().is_empty() => Ok(Some(value)),
        Some(_) => Err(format!("{name} must be a non-empty string").into()),
    }
}

fn optional_semantic_level(arguments: &Value) -> BrowserResult<Option<SemanticObservationLevel>> {
    match optional_string(arguments, "level")? {
        None => Ok(None),
        Some("summary") => Ok(Some(SemanticObservationLevel::Summary)),
        Some("interactive") => Ok(Some(SemanticObservationLevel::Interactive)),
        Some("structured") => Ok(Some(SemanticObservationLevel::Structured)),
        Some("detailed") => Ok(Some(SemanticObservationLevel::Detailed)),
        Some("raw") => Ok(Some(SemanticObservationLevel::Raw)),
        Some(_) => Err("level must be summary, interactive, structured, detailed, or raw".into()),
    }
}

fn parse_knowledge_lookup_options(arguments: &Value) -> BrowserResult<KnowledgeLookupOptions> {
    let profile_scope = match optional_string(arguments, "profileScope")?.unwrap_or("profileBound")
    {
        "anonymous" => KnowledgeProfileScope::Anonymous,
        "authenticated" => KnowledgeProfileScope::Authenticated,
        "profileBound" => KnowledgeProfileScope::ProfileBound,
        _ => return Err("profileScope must be anonymous, authenticated, or profileBound".into()),
    };
    Ok(KnowledgeLookupOptions {
        profile_scope,
        profile_key: optional_string(arguments, "profileKey")?.map(str::to_string),
        locale: optional_string(arguments, "locale")?.map(str::to_string),
        tenant_key: optional_string(arguments, "tenantKey")?.map(str::to_string),
        browser_family: optional_string(arguments, "browserFamily")?
            .unwrap_or("chromium")
            .to_string(),
        browser_version: optional_string(arguments, "browserVersion")?.map(str::to_string),
        glass_schema_version: 1,
        policy_preset: String::new(),
        now_epoch_seconds: 0,
        current_revision: None,
        current_extension_id: None,
        surface_kind: None,
        backend_kind: None,
        backend_capabilities: Vec::new(),
        workspace_id: optional_string(arguments, "workspaceId")?.map(str::to_string),
        workspace_generation: optional_u64_value(arguments, "workspaceGeneration")?,
    })
}

fn optional_bool(arguments: &Value, name: &str) -> BrowserResult<bool> {
    match arguments.get(name) {
        None => Ok(false),
        Some(Value::Bool(value)) => Ok(*value),
        Some(_) => Err(format!("{name} must be a boolean").into()),
    }
}

fn optional_batch_mode(arguments: &Value) -> BrowserResult<BatchMode> {
    match optional_string(arguments, "mode")? {
        None => Ok(BatchMode::Unguarded),
        Some("fixed") => Ok(BatchMode::Fixed),
        Some("chain") => Ok(BatchMode::Chain),
        Some("unguarded") => Ok(BatchMode::Unguarded),
        Some(_) => Err("mode must be fixed, chain, or unguarded".into()),
    }
}

fn optional_number(arguments: &Value, name: &str, default: f64) -> BrowserResult<f64> {
    match arguments.get(name) {
        None => Ok(default),
        Some(value) => value
            .as_f64()
            .ok_or_else(|| format!("{name} must be a number").into()),
    }
}

fn required_number(arguments: &Value, name: &str) -> BrowserResult<f64> {
    arguments
        .get(name)
        .and_then(Value::as_f64)
        .filter(|value| value.is_finite())
        .ok_or_else(|| format!("{name} must be a finite number").into())
}

fn optional_u64(arguments: &Value, name: &str, default: u64) -> BrowserResult<u64> {
    match arguments.get(name) {
        None => Ok(default),
        Some(value) => value
            .as_u64()
            .filter(|value| (1..=300_000).contains(value))
            .ok_or_else(|| format!("{name} must be an integer from 1 to 300000").into()),
    }
}

fn optional_u64_value(arguments: &Value, name: &str) -> BrowserResult<Option<u64>> {
    match arguments.get(name) {
        None | Some(Value::Null) => Ok(None),
        Some(value) => value
            .as_u64()
            .map(Some)
            .ok_or_else(|| format!("{name} must be a non-negative integer").into()),
    }
}

fn parse_visual_format(value: &str) -> BrowserResult<VisualFormat> {
    match value {
        "png" => Ok(VisualFormat::Png),
        "jpeg" => Ok(VisualFormat::Jpeg),
        "webp" => Ok(VisualFormat::Webp),
        _ => Err("format must be png, jpeg, or webp".into()),
    }
}

fn optional_visual_clip(arguments: &Value) -> BrowserResult<Option<VisualClip>> {
    let Some(value) = arguments.get("clip").filter(|value| !value.is_null()) else {
        return Ok(None);
    };
    let object = value.as_object().ok_or("clip must be an object")?;
    let number = |name: &str| {
        object
            .get(name)
            .and_then(Value::as_f64)
            .ok_or_else(|| format!("clip.{name} must be numeric"))
    };
    Ok(Some(VisualClip {
        x: number("x")?,
        y: number("y")?,
        width: number("width")?,
        height: number("height")?,
    }))
}

fn text_result(text: impl Into<String>) -> Value {
    let text = text.into();
    let payload_bytes = text.len();
    json!({
        "content": [{"type": "text", "text": text}],
        "_meta": {"contextCost": {
            "payloadBytes": payload_bytes,
            "estimatedTokens": payload_bytes.div_ceil(4)
        }}
    })
}

fn action_result(outcome: ActionOutcome) -> BrowserResult<Value> {
    serialized_result(&outcome)
}

fn serialized_result<T: Serialize + ?Sized>(value: &T) -> BrowserResult<Value> {
    Ok(text_result(serde_json::to_string(value)?))
}

fn serialized_result_mode<T: Serialize + ?Sized>(
    value: &T,
    mode: ResponseMode,
) -> BrowserResult<Value> {
    let value = serde_json::to_value(value)?;
    let projected = project_and_store(value, mode, "mcp", default_result_store_path())?;
    Ok(text_result(serde_json::to_string(&projected)?))
}

fn success_response(id: Option<Value>, result: Value) -> JsonRpcResponse {
    JsonRpcResponse {
        jsonrpc: "2.0",
        result: Some(result),
        error: None,
        id,
    }
}

fn error_response(id: Option<Value>, code: i32, message: impl Into<String>) -> JsonRpcResponse {
    error_response_with_data(id, code, message, Value::Null)
}

fn bounded_error_message(message: String) -> String {
    if message.len() <= MAX_ERROR_MESSAGE_BYTES {
        return message;
    }
    let mut bounded = message;
    let mut end = MAX_ERROR_MESSAGE_BYTES;
    while !bounded.is_char_boundary(end) {
        end -= 1;
    }
    bounded.truncate(end);
    bounded
}

fn bounded_error_details(details: Value) -> Value {
    if details.is_null() {
        return details;
    }
    let original_bytes = serde_json::to_vec(&details)
        .map(|serialized| serialized.len())
        .unwrap_or(MAX_ERROR_DETAILS_BYTES + 1);
    if original_bytes <= MAX_ERROR_DETAILS_BYTES {
        return details;
    }
    json!({
        "truncated": true,
        "originalBytes": original_bytes,
        "maxBytes": MAX_ERROR_DETAILS_BYTES
    })
}

fn error_response_with_data(
    id: Option<Value>,
    code: i32,
    message: impl Into<String>,
    details: Value,
) -> JsonRpcResponse {
    let message = bounded_error_message(message.into());
    let details = bounded_error_details(details);
    let canonical_code = match code {
        -32600 => "protocol.invalidRequest",
        -32601 => "protocol.methodNotFound",
        -32602 => "protocol.invalidParams",
        -32603 => "protocol.internal",
        -32800 => "protocol.cancelled",
        -32003 => "policy.mutationLease",
        -32002 => "protocol.notInitialized",
        -32000 => "resource.busy",
        _ => "protocol.error",
    };
    let data = serde_json::json!({
        "code": canonical_code,
        "phase": "preflight",
        "message": message.clone(),
        "mutationPossible": false,
        "retry": {
            "classification": "safeAfterReobserve",
            "recommendedOperation": "inspect_page"
        },
        "details": (!details.is_null()).then_some(details),
    });
    JsonRpcResponse {
        jsonrpc: "2.0",
        result: None,
        error: Some(JsonRpcError {
            code,
            message,
            data: Some(data),
        }),
        id,
    }
}

async fn read_message<R: AsyncBufRead + Unpin>(
    reader: &mut R,
) -> io::Result<Option<(String, FrameFormat)>> {
    let mut preamble_bytes = 0_usize;
    loop {
        let Some(first_line) = read_initial_line(reader, MAX_HEADER_BYTES).await? else {
            return Ok(None);
        };
        preamble_bytes = preamble_bytes.saturating_add(first_line.len());
        if preamble_bytes > MAX_HEADER_BYTES {
            return Err(invalid_data("MCP preamble exceeds the size limit"));
        }
        let first_line = strip_line_ending(&first_line);
        if first_line.iter().all(u8::is_ascii_whitespace) {
            continue;
        }
        if first_line.starts_with(b"Content-Length:") {
            let length_text = std::str::from_utf8(&first_line[b"Content-Length:".len()..])
                .map_err(invalid_data)?
                .trim();
            let length = length_text
                .parse::<usize>()
                .map_err(|_| invalid_data("invalid Content-Length"))?;
            if length > MAX_MESSAGE_BYTES {
                return Err(invalid_data("MCP message exceeds the size limit"));
            }
            let separator = tokio::time::timeout(
                FRAME_BODY_TIMEOUT,
                read_limited_line(reader, MAX_HEADER_BYTES),
            )
            .await
            .map_err(|_| io::Error::new(io::ErrorKind::TimedOut, "MCP header timed out"))??
            .ok_or_else(|| invalid_data("truncated MCP header"))?;
            if !strip_line_ending(&separator).is_empty() {
                return Err(invalid_data(
                    "Content-Length header must end with a blank line",
                ));
            }
            let mut body = vec![0_u8; length];
            tokio::time::timeout(FRAME_BODY_TIMEOUT, reader.read_exact(&mut body))
                .await
                .map_err(|_| io::Error::new(io::ErrorKind::TimedOut, "MCP body timed out"))??;
            let body = String::from_utf8(body).map_err(invalid_data)?;
            return Ok(Some((body, FrameFormat::ContentLength)));
        }
        if first_line.len() > MAX_MESSAGE_BYTES {
            return Err(invalid_data("MCP newline message exceeds the size limit"));
        }
        let body = String::from_utf8(first_line.to_vec()).map_err(invalid_data)?;
        return Ok(Some((body, FrameFormat::Newline)));
    }
}

/// Exercise the production MCP frame reader without exposing frame contents.
#[cfg(feature = "fuzzing")]
#[doc(hidden)]
pub async fn fuzz_frame(data: &[u8]) {
    let mut reader = BufReader::new(data);
    let _ = read_message(&mut reader).await;
}

async fn read_initial_line<R: AsyncBufRead + Unpin>(
    reader: &mut R,
    limit: usize,
) -> io::Result<Option<Vec<u8>>> {
    let has_data = !reader.fill_buf().await?.is_empty();
    if !has_data {
        return Ok(None);
    }
    tokio::time::timeout(FRAME_BODY_TIMEOUT, read_limited_line(reader, limit))
        .await
        .map_err(|_| io::Error::new(io::ErrorKind::TimedOut, "MCP line timed out"))?
}

async fn read_limited_line<R: AsyncBufRead + Unpin>(
    reader: &mut R,
    limit: usize,
) -> io::Result<Option<Vec<u8>>> {
    let mut line = Vec::new();
    loop {
        let buffer = reader.fill_buf().await?;
        if buffer.is_empty() {
            return Ok((!line.is_empty()).then_some(line));
        }
        let take = buffer
            .iter()
            .position(|byte| *byte == b'\n')
            .map_or(buffer.len(), |index| index + 1);
        if line.len().saturating_add(take) > limit {
            return Err(invalid_data("MCP line exceeds the size limit"));
        }
        line.extend_from_slice(&buffer[..take]);
        reader.consume(take);
        if line.ends_with(b"\n") {
            return Ok(Some(line));
        }
    }
}

fn strip_line_ending(line: &[u8]) -> &[u8] {
    line.strip_suffix(b"\r\n")
        .or_else(|| line.strip_suffix(b"\n"))
        .unwrap_or(line)
}

fn invalid_data(error: impl std::fmt::Display) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, error.to_string())
}

async fn write_response<W: AsyncWrite + Unpin>(
    writer: &mut W,
    response: &JsonRpcResponse,
    format: FrameFormat,
) -> io::Result<()> {
    let body = encode_response(response, MAX_RESPONSE_BYTES)?;
    write_framed_message(writer, &body, format).await
}

async fn write_json_message<W: AsyncWrite + Unpin>(
    writer: &mut W,
    message: &Value,
    format: FrameFormat,
) -> io::Result<()> {
    let body = serde_json::to_string(message).map_err(io::Error::other)?;
    if body.len() > MAX_RESPONSE_BYTES {
        return Err(invalid_data("MCP server request exceeds the size limit"));
    }
    write_framed_message(writer, &body, format).await
}

async fn write_framed_message<W: AsyncWrite + Unpin>(
    writer: &mut W,
    body: &str,
    format: FrameFormat,
) -> io::Result<()> {
    match format {
        FrameFormat::ContentLength => {
            writer
                .write_all(format!("Content-Length: {}\r\n\r\n", body.len()).as_bytes())
                .await?;
            writer.write_all(body.as_bytes()).await?;
        }
        FrameFormat::Newline => {
            writer.write_all(body.as_bytes()).await?;
            writer.write_all(b"\n").await?;
        }
    }
    writer.flush().await
}

fn encode_response(response: &JsonRpcResponse, limit: usize) -> io::Result<String> {
    let body = serde_json::to_string(response).map_err(io::Error::other)?;
    if body.len() > limit {
        return serde_json::to_string(&error_response(
            response.id.clone(),
            -32001,
            "MCP response exceeds the size limit",
        ))
        .map_err(io::Error::other);
    }
    Ok(body)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::browser::session::{ActionKind, ActionStatus, ActionVerificationEvidence};

    fn valid_web_ir_fixture() -> Value {
        json!({
            "schemaVersion": 1,
            "revision": 7,
            "document": {"revision": 7},
            "entities": [
                {
                    "id": "page",
                    "kind": "page",
                    "quality": "confirmed",
                    "evidenceSources": []
                },
                {
                    "id": "field-1",
                    "kind": "field",
                    "role": "textbox",
                    "name": "Email",
                    "quality": "strong",
                    "evidenceSources": ["dom"]
                }
            ],
            "relationships": [
                {"from": "page", "to": "field-1", "kind": "contains"}
            ],
            "coverage": {
                "structural": "strong",
                "semantic": "strong",
                "interactiveEntitiesObserved": 1,
                "opaqueRegions": 0,
                "reasons": []
            },
            "limits": {
                "truncated": false,
                "omittedFacts": 0,
                "textBytes": 0,
                "missingSources": []
            }
        })
    }

    #[derive(Deserialize)]
    struct FramingCorpusCase {
        name: String,
        bytes: Vec<u8>,
        valid: bool,
    }

    #[test]
    fn request_log_metadata_excludes_params_and_raw_id() {
        let body = json!({
            "jsonrpc": "2.0",
            "id": "private-request-id",
            "method": "tools/call",
            "params": {
                "name": "type",
                "arguments": {"text": "super-secret-value"}
            }
        })
        .to_string();
        let request: JsonRpcRequest = serde_json::from_str(&body).unwrap();

        let metadata = request_log_metadata(&request, body.len());

        assert_eq!(metadata.method, "tools/call");
        assert_eq!(metadata.request_id_kind, "string");
        assert!(metadata.request_id_present);
        assert_eq!(metadata.body_bytes, body.len());
        let rendered = format!("{metadata:?}");
        assert!(!rendered.contains("private-request-id"));
        assert!(!rendered.contains("super-secret-value"));
    }

    #[test]
    fn mcp_errors_include_canonical_recovery_fields() {
        let response = error_response(Some(json!("request-1")), -32602, "invalid tool arguments");
        let error = response.error.unwrap();
        assert_eq!(error.code, -32602);
        assert_eq!(
            error.data.as_ref().unwrap()["code"],
            "protocol.invalidParams"
        );
        assert_eq!(error.data.as_ref().unwrap()["phase"], "preflight");
        assert_eq!(
            error.data.as_ref().unwrap()["retry"]["recommendedOperation"],
            "inspect_page"
        );
        assert_eq!(error.data.as_ref().unwrap()["mutationPossible"], false);
    }

    #[test]
    fn mcp_error_messages_are_bounded_before_transport_encoding() {
        let response = error_response(
            Some(json!("request-1")),
            -32601,
            "x".repeat(MAX_ERROR_MESSAGE_BYTES + 1),
        );
        let error = response.error.unwrap();
        assert_eq!(error.message.len(), MAX_ERROR_MESSAGE_BYTES);
        assert_eq!(
            error.data.unwrap()["message"].as_str().unwrap().len(),
            MAX_ERROR_MESSAGE_BYTES
        );

        let response = error_response(
            Some(json!("request-1")),
            -32601,
            "é".repeat(MAX_ERROR_MESSAGE_BYTES),
        );
        assert!(response.error.unwrap().message.len() <= MAX_ERROR_MESSAGE_BYTES);
    }

    #[test]
    fn mcp_error_details_are_bounded_before_transport_encoding() {
        let response = error_response_with_data(
            Some(json!("request-1")),
            -32602,
            "invalid tool arguments",
            json!({"diagnostic": "x".repeat(MAX_ERROR_DETAILS_BYTES)}),
        );
        let details = &response.error.unwrap().data.unwrap()["details"];
        assert!(details["originalBytes"].as_u64().unwrap() > MAX_ERROR_DETAILS_BYTES as u64);
        assert_eq!(details["maxBytes"], MAX_ERROR_DETAILS_BYTES);
        assert!(serde_json::to_vec(details).unwrap().len() < MAX_ERROR_DETAILS_BYTES);
    }

    #[test]
    fn mcp_tool_calls_map_to_the_canonical_glass_request() {
        let request: JsonRpcRequest = serde_json::from_value(json!({
            "jsonrpc": "2.0",
            "id": "request-1",
            "method": "tools/call",
            "params": {
                "name": "observe",
                "arguments": {"level": "interactive"}
            }
        }))
        .unwrap();

        let canonical = canonical_tool_request(&request).unwrap();

        assert_eq!(canonical.operation, "browser.observe");
        assert_eq!(canonical.request_id, "request-1");
        assert_eq!(canonical.payload["level"], "interactive");
        canonical.validate().unwrap();
    }

    #[test]
    fn web_ir_revision_tools_map_to_canonical_operations() {
        for (name, operation) in [
            ("diffWebIr", crate::protocol::WEB_IR_DIFF_OPERATION),
            (
                "continuityWebIr",
                crate::protocol::WEB_IR_CONTINUITY_OPERATION,
            ),
        ] {
            let request: JsonRpcRequest = serde_json::from_value(json!({
                "jsonrpc": "2.0",
                "id": name,
                "method": "tools/call",
                "params": {
                    "name": name,
                    "arguments": {
                        "before": valid_web_ir_fixture(),
                        "after": valid_web_ir_fixture(),
                        "entityId": "field-1"
                    }
                }
            }))
            .unwrap();
            let canonical = canonical_tool_request(&request).unwrap();
            assert_eq!(canonical.operation, operation);
            assert_eq!(canonical.payload["before"]["schemaVersion"], 1);
            canonical.validate().unwrap();
        }

        for (name, operation) in [
            ("inspectWebIr", crate::protocol::WEB_IR_INSPECT_OPERATION),
            ("validateWebIr", crate::protocol::WEB_IR_VALIDATE_OPERATION),
        ] {
            let request: JsonRpcRequest = serde_json::from_value(json!({
                "jsonrpc": "2.0",
                "id": name,
                "method": "tools/call",
                "params": {
                    "name": name,
                    "arguments": {"ir": valid_web_ir_fixture()}
                }
            }))
            .unwrap();
            let canonical = canonical_tool_request(&request).unwrap();
            assert_eq!(canonical.operation, operation);
            assert_eq!(canonical.payload["ir"]["schemaVersion"], 1);
            canonical.validate().unwrap();
        }
    }

    #[test]
    fn task_tools_map_to_canonical_operations() {
        let task = json!({
            "schemaVersion": 1,
            "task": "region.extract",
            "scope": {"regionName": "Checkout"},
            "limits": {"maxActions": 4, "timeoutMs": 2000, "maxItems": 16},
            "risk": "readOnly"
        });
        for (name, operation) in [
            ("compileTask", crate::protocol::TASK_COMPILE_OPERATION),
            ("executeTask", crate::protocol::TASK_EXECUTE_OPERATION),
            ("validateTask", crate::protocol::TASK_VALIDATE_OPERATION),
        ] {
            let mut arguments = json!({"task": task.clone()});
            if operation == crate::protocol::TASK_EXECUTE_OPERATION {
                arguments["expectedRevision"] = json!(7);
                arguments["confirmed"] = json!(false);
            }
            if operation == crate::protocol::TASK_COMPILE_OPERATION {
                arguments["ir"] =
                    serde_json::to_value(crate::task_compiler::test_compiler_ir()).unwrap();
            }
            let request: JsonRpcRequest = serde_json::from_value(json!({
                "jsonrpc": "2.0",
                "id": name,
                "method": "tools/call",
                "params": {
                    "name": name,
                    "arguments": arguments
                }
            }))
            .unwrap();
            let canonical = canonical_tool_request(&request).unwrap();
            assert_eq!(canonical.operation, operation);
            assert_eq!(canonical.payload["task"]["schemaVersion"], 1);
            canonical.validate().unwrap();
            if operation == crate::protocol::TASK_VALIDATE_OPERATION {
                canonical.decode_task_validate().unwrap();
            } else if operation == crate::protocol::TASK_COMPILE_OPERATION {
                canonical.decode_task_compile().unwrap();
            } else {
                canonical.decode_task_execute().unwrap();
            }
        }
    }

    #[test]
    fn canonical_payload_request_excludes_mcp_transport_options() {
        let task = json!({
            "schemaVersion": 1,
            "task": "region.extract",
            "scope": {"regionName": "Checkout"},
            "limits": {"maxActions": 4, "timeoutMs": 2000, "maxItems": 16},
            "risk": "readOnly"
        });
        let request: JsonRpcRequest = serde_json::from_value(json!({
            "jsonrpc": "2.0",
            "id": "validate",
            "method": "tools/call",
            "params": {
                "name": "validateTask",
                "arguments": {
                    "task": task.clone(),
                    "responseMode": "reference",
                    "includeTrace": true
                }
            }
        }))
        .unwrap();
        let canonical = canonical_payload_request(&request, json!({"task": task})).unwrap();
        assert_eq!(
            canonical.operation,
            crate::protocol::TASK_VALIDATE_OPERATION
        );
        assert_eq!(
            canonical.payload,
            json!({"task": canonical.payload["task"].clone()})
        );
        canonical.decode_task_validate().unwrap();
    }

    #[test]
    fn canonical_web_ir_request_excludes_mcp_transport_options() {
        let draft = valid_web_ir_fixture();
        let request: JsonRpcRequest = serde_json::from_value(json!({
            "jsonrpc": "2.0",
            "id": "inspect",
            "method": "tools/call",
            "params": {
                "name": "inspectWebIr",
                "arguments": {
                    "ir": draft.clone(),
                    "responseMode": "reference",
                    "includeTrace": true
                }
            }
        }))
        .unwrap();
        let canonical = canonical_payload_request(&request, json!({"ir": draft})).unwrap();
        assert_eq!(
            canonical.operation,
            crate::protocol::WEB_IR_INSPECT_OPERATION
        );
        assert_eq!(
            canonical.payload,
            json!({"ir": canonical.payload["ir"].clone()})
        );
        canonical.decode_web_ir_inspect().unwrap();
    }

    #[tokio::test]
    async fn initializes_and_advertises_browser_tools_without_starting_chrome() {
        let initialize: JsonRpcRequest = serde_json::from_value(json!({
            "jsonrpc": "2.0",
            "id": "initialize",
            "method": "initialize",
            "params": {"protocolVersion": "2024-11-05"}
        }))
        .unwrap();
        let request: JsonRpcRequest = serde_json::from_value(json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "tools/list"
        }))
        .unwrap();
        let mut session = None;
        let policy = BrowserPolicy::development(std::env::current_dir().unwrap()).unwrap();
        let initialized = handle_request(
            &initialize,
            &mut session,
            &SessionOptions::default(),
            &policy,
            None,
        )
        .await
        .unwrap();
        let caps = &initialized.result.as_ref().unwrap()["capabilities"];
        let glass = &initialized.result.as_ref().unwrap()["glass"];
        assert_eq!(
            initialized.result.as_ref().unwrap()["serverInfo"]["name"],
            "glass"
        );
        assert_eq!(glass["protocolVersion"], 1);
        assert_eq!(glass["schemas"]["workflow"], json!([1]));
        assert_eq!(glass["capabilities"]["localDaemon"], false);
        assert_eq!(caps["tools"]["listChanged"], false);
        assert_eq!(caps["resources"]["listChanged"], false);
        assert!(session.is_none());

        let result = handle_request(
            &request,
            &mut session,
            &SessionOptions::default(),
            &policy,
            None,
        )
        .await
        .unwrap();
        let result = result.result.unwrap();
        let tools = result["tools"].as_array().unwrap();
        assert!(tools.len() >= 85);
        let preflight = tools
            .iter()
            .find(|tool| tool["name"] == "preflightNavigation")
            .expect("preflightNavigation must be advertised");
        assert_eq!(preflight["inputSchema"]["required"], json!(["url"]));
        assert_eq!(preflight["inputSchema"]["additionalProperties"], false);
        let execute_task = tools
            .iter()
            .find(|tool| tool["name"] == "executeTask")
            .expect("executeTask must be advertised");
        assert_eq!(
            execute_task["description"],
            "Execute a confirmed, revision-guarded Task Protocol v1 task from any validated browser-backed family in the current browser session."
        );
        assert!(
            !execute_task["description"]
                .as_str()
                .unwrap()
                .contains("form Task Protocol")
        );
        assert_eq!(
            execute_task["inputSchema"]["properties"]["task"]["description"],
            "Validated Task Protocol v1 authored task from a form, navigation, dialog, pagination, extraction, or field-read family."
        );
        assert_eq!(
            execute_task["inputSchema"]["properties"]["expectedRevision"]["type"],
            "integer"
        );
        assert_eq!(
            execute_task["inputSchema"]["properties"]["confirmed"]["description"],
            "Explicit confirmation for risky or ambiguity-gated tasks."
        );
        assert_eq!(
            execute_task["inputSchema"]["properties"]["leaseToken"]["type"],
            "string"
        );
        assert_eq!(
            execute_task["inputSchema"]["properties"]["responseMode"]["enum"],
            json!(["minimal", "normal", "diagnostic"])
        );
        assert!(tools.iter().any(|tool| tool["name"] == "continuityWebIr"));
        assert!(tools.iter().any(|tool| tool["name"] == "diffWebIr"));
        assert!(tools.iter().any(|tool| tool["name"] == "executeTask"));
        assert!(tools.iter().any(|tool| tool["name"] == "extractWebIr"));
        assert!(tools.iter().any(|tool| tool["name"] == "inspectWebIr"));
        assert!(tools.iter().any(|tool| tool["name"] == "validateTask"));
        assert!(tools.iter().any(|tool| tool["name"] == "validateWebIr"));
        let extraction = tools
            .iter()
            .find(|tool| tool["name"] == "extractStructured")
            .expect("extractStructured must be advertised");
        assert!(
            extraction["description"]
                .as_str()
                .unwrap()
                .contains("read_sensitive_extraction")
        );
        assert_eq!(
            extraction["inputSchema"]["properties"]["fields"]["items"]["required"],
            json!(["name", "path", "kind"])
        );
        assert_eq!(
            extraction["inputSchema"]["properties"]["fields"]["items"]["properties"]["kind"]["enum"]
                [7],
            "dateTime"
        );
        assert_eq!(
            extraction["inputSchema"]["properties"]["startIndex"]["minimum"],
            0
        );
        assert_eq!(
            extraction["inputSchema"]["properties"]["continuation"]["properties"]["contractHash"]["minLength"],
            71
        );
        let bootstrap = tools
            .iter()
            .find(|tool| tool["name"] == "observeBootstrap")
            .expect("observeBootstrap must be advertised");
        assert_eq!(
            bootstrap["inputSchema"]["properties"]["responseMode"]["enum"],
            json!(["minimal", "normal", "diagnostic"])
        );
        assert_eq!(bootstrap["inputSchema"]["additionalProperties"], false);
        let observe = tools.iter().find(|tool| tool["name"] == "observe").unwrap();
        assert_eq!(
            observe["inputSchema"]["properties"]["includeScreenshot"]["default"],
            false
        );
        for tool_name in [
            "inspectPage",
            "findTarget",
            "actAndVerify",
            "extractStructured",
            "extractWebIr",
            "recoverRun",
            "sessionSnapshot",
        ] {
            assert!(tools.iter().any(|tool| tool["name"] == tool_name));
        }
        assert_eq!(
            tools
                .iter()
                .find(|tool| tool["name"] == "actAndVerify")
                .unwrap()["inputSchema"]["properties"]["timeoutMs"]["default"],
            10_000
        );
        assert_eq!(
            tools
                .iter()
                .find(|tool| tool["name"] == "findTarget")
                .unwrap()["inputSchema"]["required"][0],
            "schemaVersion"
        );
        assert_eq!(
            tools
                .iter()
                .find(|tool| tool["name"] == "inspectPage")
                .unwrap()["inputSchema"]["additionalProperties"],
            false
        );
        assert!(
            tools
                .iter()
                .find(|tool| tool["name"] == "sessionSnapshot")
                .is_some()
        );
        for tool_name in ["navigate", "click", "type", "fillForm"] {
            let tool = tools.iter().find(|tool| tool["name"] == tool_name).unwrap();
            assert_eq!(
                tool["inputSchema"]["properties"]["expectedRevision"]["type"],
                "integer"
            );
        }
        assert_eq!(
            observe["inputSchema"]["properties"]["includeDom"]["default"],
            false
        );
        assert_eq!(
            observe["inputSchema"]["properties"]["level"]["enum"],
            json!(["summary", "interactive", "structured", "detailed", "raw"])
        );
        assert!(tools.iter().any(|tool| tool["name"] == "screenshot"));
        assert!(tools.iter().any(|tool| tool["name"] == "resolveIntent"));
        assert!(
            tools
                .iter()
                .any(|tool| tool["name"] == "resolveIntentWithKnowledge")
        );
        assert!(tools.iter().any(|tool| tool["name"] == "executeIntent"));
        assert!(tools.iter().any(|tool| tool["name"] == "observeKnowledge"));
        for tool_name in [
            "knowledgeList",
            "knowledgeShow",
            "knowledgeStats",
            "knowledgeInvalidate",
            "knowledgePurge",
        ] {
            assert!(tools.iter().any(|tool| tool["name"] == tool_name));
        }
        assert!(tools.iter().any(|tool| tool["name"] == "doubleClick"));
        for tool_name in [
            "clickExpectPopup",
            "doubleClick",
            "hover",
            "drag",
            "key",
            "keyDown",
            "keyUp",
            "shortcut",
            "clear",
            "check",
            "uncheck",
            "select",
            "scroll",
            "upload",
        ] {
            let tool = tools.iter().find(|tool| tool["name"] == tool_name).unwrap();
            assert_eq!(
                tool["inputSchema"]["properties"]["expectedRevision"]["type"], "integer",
                "{tool_name} must expose expectedRevision"
            );
        }
        let popup_click = tools
            .iter()
            .find(|tool| tool["name"] == "clickExpectPopup")
            .unwrap();
        assert_eq!(
            popup_click["inputSchema"],
            json!({
                "type": "object",
                "properties": {"target": {"type": "string"}, "selector": {"type": "string"}, "expectedRevision":{"type":"integer","minimum":0}, "includeTrace": {"type":"boolean", "default": false}},
                "anyOf": [{"required": ["target"]}, {"required": ["selector"]}]
            })
        );
        for name in [
            "listTargets",
            "createTarget",
            "selectTarget",
            "closeTarget",
            "listFrames",
            "selectFrame",
        ] {
            assert!(tools.iter().any(|tool| tool["name"] == name));
        }
        for name in [
            "hover", "drag", "key", "shortcut", "clear", "check", "uncheck", "select", "upload",
        ] {
            assert!(tools.iter().any(|tool| tool["name"] == name));
        }
        for name in [
            "diagnostics",
            "acceptDialog",
            "dismissDialog",
            "dismissConsent",
            "download",
        ] {
            assert!(tools.iter().any(|tool| tool["name"] == name));
        }
        assert!(tools.iter().any(|tool| {
            tool["name"] == "getDOM"
                && tool["description"]
                    .as_str()
                    .is_some_and(|description| description.contains("explicit"))
        }));
        assert!(session.is_none());
    }

    #[tokio::test]
    async fn recover_run_is_browser_free_and_conservative() {
        let request: JsonRpcRequest = serde_json::from_value(json!({
            "jsonrpc": "2.0",
            "id": "recover",
            "method": "tools/call",
            "params": {
                "name": "recoverRun",
                "arguments": {"executionId": "run-123"}
            }
        }))
        .unwrap();
        let mut session = None;
        let policy = BrowserPolicy::development(std::env::current_dir().unwrap()).unwrap();
        let response = handle_request(
            &request,
            &mut session,
            &SessionOptions::default(),
            &policy,
            None,
        )
        .await
        .unwrap();
        let text = response.result.unwrap()["content"][0]["text"]
            .as_str()
            .unwrap()
            .to_owned();
        let result: Value = serde_json::from_str(&text).unwrap();
        assert_eq!(result["executionId"], "run-123");
        assert_eq!(result["known"], false);
        assert_eq!(result["mutationPossible"], true);
        assert_eq!(result["retry"]["classification"], "unsafeUntilReconciled");
        assert!(session.is_none());
    }

    #[cfg(feature = "native-engine")]
    async fn invoke_native_mcp_tool(
        name: &str,
        arguments: Value,
        session: &mut Option<CdpBrowserSession>,
        native_session: &mut Option<BrowserRuntimeSession>,
        options: &SessionOptions,
        policy: &BrowserPolicy,
    ) -> JsonRpcResponse {
        let request: JsonRpcRequest = serde_json::from_value(json!({
            "jsonrpc": "2.0",
            "id": name,
            "method": "tools/call",
            "params": {"name": name, "arguments": arguments}
        }))
        .unwrap();
        let development_sessions = ();
        handle_request_with_viewport(
            &request,
            session,
            native_session,
            true,
            None,
            options,
            policy,
            None,
            None,
            &development_sessions,
            None,
            None,
        )
        .await
        .expect("native MCP tool should return a response")
    }

    #[cfg(feature = "native-engine")]
    #[test]
    fn native_mcp_routes_core_browser_tools_without_chromium() {
        std::thread::Builder::new()
            .name("glass-native-mcp-core-test".into())
            .stack_size(8 * 1024 * 1024)
            .spawn(|| {
                let runtime = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .expect("native MCP test runtime should build");
                runtime.block_on(native_mcp_routes_core_browser_tools_without_chromium_inner());
            })
            .expect("native MCP test thread should spawn")
            .join()
            .expect("native MCP test thread should not panic");
    }

    #[cfg(feature = "native-engine")]
    async fn native_mcp_routes_core_browser_tools_without_chromium_inner() {
        let mut session = None;
        let mut native_session = None;
        let options = SessionOptions::default();
        let policy = BrowserPolicy::development(std::env::current_dir().unwrap()).unwrap();
        let url = "data:text/html,%3Ctitle%3EMCP%3C%2Ftitle%3E%3Cbutton%20id%3D%22save%22%3ESave%3C%2Fbutton%3E%3Cinput%20id%3D%22name%22%3E";

        let navigate = invoke_native_mcp_tool(
            "navigate",
            json!({"url": url}),
            &mut session,
            &mut native_session,
            &options,
            &policy,
        )
        .await;
        assert!(navigate.error.is_none());
        let navigation: Value = serde_json::from_str(
            navigate.result.unwrap()["content"][0]["text"]
                .as_str()
                .unwrap(),
        )
        .unwrap();
        assert_eq!(navigation["url"], url);
        assert!(session.is_none());

        let preflight = invoke_native_mcp_tool(
            "preflight",
            json!({"target": "id=save", "action": "click"}),
            &mut session,
            &mut native_session,
            &options,
            &policy,
        )
        .await;
        assert!(preflight.error.is_none());
        let preflight: Value = serde_json::from_str(
            preflight.result.unwrap()["content"][0]["text"]
                .as_str()
                .unwrap(),
        )
        .unwrap();
        assert_eq!(preflight["unique"], true);
        assert_eq!(preflight["actionable"], true);
        assert_eq!(preflight["node"]["role"], "button");
        assert!(preflight["geometry"]["width"].as_u64().unwrap_or(0) > 0);
        assert!(session.is_none());

        let inspection = invoke_native_mcp_tool(
            "inspectPage",
            json!({}),
            &mut session,
            &mut native_session,
            &options,
            &policy,
        )
        .await;
        assert!(inspection.error.is_none());
        let inspection: Value = serde_json::from_str(
            inspection.result.unwrap()["content"][0]["text"]
                .as_str()
                .unwrap(),
        )
        .unwrap();
        assert_eq!(inspection["page"]["title"], "MCP");
        assert_eq!(inspection["regions"][0]["targets"][0]["role"], "button");

        let targets = invoke_native_mcp_tool(
            "listTargets",
            json!({}),
            &mut session,
            &mut native_session,
            &options,
            &policy,
        )
        .await;
        assert!(targets.error.is_none());
        let targets: Value = serde_json::from_str(
            targets.result.unwrap()["content"][0]["text"]
                .as_str()
                .unwrap(),
        )
        .unwrap();
        assert_eq!(targets["result"][0]["id"], "native-context");
        assert_eq!(targets["result"][0]["title"], "MCP");

        let selected_target = invoke_native_mcp_tool(
            "selectTarget",
            json!({"id": "native-context"}),
            &mut session,
            &mut native_session,
            &options,
            &policy,
        )
        .await;
        assert!(selected_target.error.is_none());

        let frames = invoke_native_mcp_tool(
            "listFrames",
            json!({}),
            &mut session,
            &mut native_session,
            &options,
            &policy,
        )
        .await;
        assert!(frames.error.is_none());
        let frames: Value = serde_json::from_str(
            frames.result.unwrap()["content"][0]["text"]
                .as_str()
                .unwrap(),
        )
        .unwrap();
        assert_eq!(frames["result"][0]["id"], "native-context:main");
        assert_eq!(frames["result"][0]["out_of_process"], false);

        let selected_frame = invoke_native_mcp_tool(
            "selectFrame",
            json!({"id": "native-context:main"}),
            &mut session,
            &mut native_session,
            &options,
            &policy,
        )
        .await;
        assert!(selected_frame.error.is_none());

        let find_target = invoke_native_mcp_tool(
            "findTarget",
            json!({
                "schemaVersion": 1,
                "intent": "save",
                "action": "click",
                "constraints": {"role": "button", "name": "Save"},
                "resolutionPolicy": "requireExact",
                "expectedRevision": preflight["revision"]
            }),
            &mut session,
            &mut native_session,
            &options,
            &policy,
        )
        .await;
        assert!(find_target.error.is_none());
        let find_target: Value = serde_json::from_str(
            find_target.result.unwrap()["content"][0]["text"]
                .as_str()
                .unwrap(),
        )
        .unwrap();
        assert_eq!(find_target["ambiguity"], "none");
        assert_eq!(find_target["candidates"][0]["name"], "Save");

        let text = invoke_native_mcp_tool(
            "getText",
            json!({}),
            &mut session,
            &mut native_session,
            &options,
            &policy,
        )
        .await;
        assert_eq!(text.result.unwrap()["content"][0]["text"], "Save");

        let action = invoke_native_mcp_tool(
            "type",
            json!({"target": "id=name", "text": "Glass"}),
            &mut session,
            &mut native_session,
            &options,
            &policy,
        )
        .await;
        assert!(action.error.is_none());

        let script = invoke_native_mcp_tool(
            "evaluate",
            json!({"expression": "document.querySelector('#name').value"}),
            &mut session,
            &mut native_session,
            &options,
            &policy,
        )
        .await;
        let value: Value = serde_json::from_str(
            script.result.unwrap()["content"][0]["text"]
                .as_str()
                .unwrap(),
        )
        .unwrap();
        assert_eq!(value["result"], "Glass");

        let dom = invoke_native_mcp_tool(
            "getDOM",
            json!({}),
            &mut session,
            &mut native_session,
            &options,
            &policy,
        )
        .await;
        let dom: Value =
            serde_json::from_str(dom.result.unwrap()["content"][0]["text"].as_str().unwrap())
                .unwrap();
        assert_eq!(dom["title"], "MCP");
        assert!(
            dom["nodes"]
                .as_array()
                .is_some_and(|nodes| !nodes.is_empty())
        );

        let screenshot = invoke_native_mcp_tool(
            "screenshot",
            json!({}),
            &mut session,
            &mut native_session,
            &options,
            &policy,
        )
        .await;
        let screenshot_result = screenshot.result.unwrap();
        let metadata: Value =
            serde_json::from_str(screenshot_result["content"][0]["text"].as_str().unwrap())
                .unwrap();
        assert_eq!(metadata["format"], "png");
        let image = screenshot_result["content"][1]["data"].as_str().unwrap();
        assert!(image.starts_with("iVBORw0KGgo"));

        let jpeg = invoke_native_mcp_tool(
            "screenshot",
            json!({"format": "jpeg", "quality": 75, "scale": 1.5}),
            &mut session,
            &mut native_session,
            &options,
            &policy,
        )
        .await;
        assert!(jpeg.error.is_none());
        let jpeg_result = jpeg.result.unwrap();
        let jpeg_metadata: Value =
            serde_json::from_str(jpeg_result["content"][0]["text"].as_str().unwrap()).unwrap();
        assert_eq!(jpeg_metadata["format"], "jpeg");
        assert!(
            jpeg_result["content"][1]["data"]
                .as_str()
                .unwrap()
                .starts_with("/9j/")
        );

        let storage = invoke_native_mcp_tool(
            "localStorage",
            json!({}),
            &mut session,
            &mut native_session,
            &options,
            &policy,
        )
        .await;
        assert!(storage.error.is_none());
        assert!(
            storage.result.unwrap()["content"][0]["text"]
                .as_str()
                .unwrap()
                .contains("entries")
        );

        let cookies = invoke_native_mcp_tool(
            "cookies",
            json!({}),
            &mut session,
            &mut native_session,
            &options,
            &policy,
        )
        .await;
        assert!(cookies.error.is_none());
        assert!(
            cookies.result.unwrap()["content"][0]["text"]
                .as_str()
                .unwrap()
                .contains("[]")
        );

        let clear_cookies = invoke_native_mcp_tool(
            "clearCookies",
            json!({}),
            &mut session,
            &mut native_session,
            &options,
            &policy,
        )
        .await;
        assert!(clear_cookies.error.is_none());

        let wait = invoke_native_mcp_tool(
            "wait",
            json!({"condition": "text=Save", "timeoutMs": 100}),
            &mut session,
            &mut native_session,
            &options,
            &policy,
        )
        .await;
        assert!(wait.error.is_none());
        let wait: Value =
            serde_json::from_str(wait.result.unwrap()["content"][0]["text"].as_str().unwrap())
                .unwrap();
        assert_eq!(wait["condition"], "text");

        let verify = invoke_native_mcp_tool(
            "verify",
            json!({
                "predicate": {
                    "all": [
                        {"titleContains": "MCP"},
                        {"textContains": "Save"}
                    ]
                },
                "timeoutMs": 100
            }),
            &mut session,
            &mut native_session,
            &options,
            &policy,
        )
        .await;
        assert!(verify.error.is_none());
        let verify: Value = serde_json::from_str(
            verify.result.unwrap()["content"][0]["text"]
                .as_str()
                .unwrap(),
        )
        .unwrap();
        assert_eq!(verify["status"], "satisfied");

        let acted = invoke_native_mcp_tool(
            "actAndVerify",
            json!({
                "request": {
                    "schemaVersion": 1,
                    "intent": "save",
                    "action": "click",
                    "constraints": {"role": "button", "name": "Save"},
                    "resolutionPolicy": "requireExact"
                },
                "candidateId": "candidate_1",
                "predicate": {"textContains": "Save"},
                "timeoutMs": 100
            }),
            &mut session,
            &mut native_session,
            &options,
            &policy,
        )
        .await;
        assert!(acted.error.is_none());
        let acted: Value = serde_json::from_str(
            acted.result.unwrap()["content"][0]["text"]
                .as_str()
                .unwrap(),
        )
        .unwrap();
        assert_eq!(acted["status"], "verified");
        assert!(
            acted["execution"]["action"]["executionId"]
                .as_str()
                .is_some()
        );

        let javascript = invoke_native_mcp_tool(
            "wait",
            json!({"condition": "js=document.readyState === 'complete'", "timeoutMs": 100}),
            &mut session,
            &mut native_session,
            &options,
            &policy,
        )
        .await;
        assert!(javascript.error.is_none());
        let javascript: Value = serde_json::from_str(
            javascript.result.unwrap()["content"][0]["text"]
                .as_str()
                .unwrap(),
        )
        .unwrap();
        assert_eq!(javascript["last_state"], "true");
        assert!(native_session.is_some());
        native_session.take().unwrap().close().await.unwrap();
    }

    #[cfg(feature = "native-engine")]
    #[test]
    fn native_mcp_routes_shared_semantic_contracts_without_chromium() {
        std::thread::Builder::new()
            .name("glass-native-mcp-semantic-test".into())
            .stack_size(8 * 1024 * 1024)
            .spawn(|| {
                let runtime = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .expect("native MCP test runtime should build");
                runtime
                    .block_on(native_mcp_routes_shared_semantic_contracts_without_chromium_inner());
            })
            .expect("native MCP test thread should spawn")
            .join()
            .expect("native MCP test thread should not panic");
    }

    #[cfg(feature = "native-engine")]
    async fn native_mcp_routes_shared_semantic_contracts_without_chromium_inner() {
        let mut session = None;
        let mut native_session = None;
        let options = SessionOptions::default();
        let policy = BrowserPolicy::development(std::env::current_dir().unwrap()).unwrap();
        let url = "data:text/html,%3Ctitle%3ESemantic%20MCP%3C%2Ftitle%3E%3Cmain%3E%3Clabel%20for%3D%22name%22%3EName%3C%2Flabel%3E%3Cinput%20id%3D%22name%22%3E%3Cbutton%3ESave%3C%2Fbutton%3E%3C%2Fmain%3E";

        let invoke_json = |response: JsonRpcResponse| -> Value {
            assert!(response.error.is_none());
            serde_json::from_str(
                response.result.unwrap()["content"][0]["text"]
                    .as_str()
                    .unwrap(),
            )
            .unwrap()
        };

        let navigation = invoke_native_mcp_tool(
            "navigate",
            json!({"url": url}),
            &mut session,
            &mut native_session,
            &options,
            &policy,
        )
        .await;
        assert!(navigation.error.is_none());

        let bootstrap = invoke_json(
            invoke_native_mcp_tool(
                "observeBootstrap",
                json!({}),
                &mut session,
                &mut native_session,
                &options,
                &policy,
            )
            .await,
        );
        assert_eq!(bootstrap["page"]["title"], "Semantic MCP");
        assert_eq!(bootstrap["ready"], true);

        let observation = invoke_json(
            invoke_native_mcp_tool(
                "observe",
                json!({"level": "structured"}),
                &mut session,
                &mut native_session,
                &options,
                &policy,
            )
            .await,
        );
        let revision = observation["revision"].as_u64().unwrap();
        assert_eq!(observation["page"]["title"], "Semantic MCP");

        let extracted = invoke_json(
            invoke_native_mcp_tool(
                "extractStructured",
                json!({
                    "fields": [{"name": "title", "path": "$.page.title", "kind": "string"}],
                    "maxItems": 16,
                    "maxBytes": 16384
                }),
                &mut session,
                &mut native_session,
                &options,
                &policy,
            )
            .await,
        );
        assert_eq!(extracted["records"][0]["title"], "Semantic MCP");

        let resolved = invoke_json(
            invoke_native_mcp_tool(
                "resolveIntent",
                json!({
                    "schemaVersion": 1,
                    "intent": "save",
                    "action": "click",
                    "constraints": {"role": "button", "name": "Save"},
                    "resolutionPolicy": "requireExact",
                    "expectedRevision": revision
                }),
                &mut session,
                &mut native_session,
                &options,
                &policy,
            )
            .await,
        );
        let candidate_id = resolved["candidates"][0]["id"].as_str().unwrap();
        assert_eq!(resolved["resolution"], "exact");

        let executed = invoke_json(
            invoke_native_mcp_tool(
                "executeIntent",
                json!({
                    "schemaVersion": 1,
                    "intent": "save",
                    "action": "click",
                    "constraints": {"role": "button", "name": "Save"},
                    "resolutionPolicy": "requireExact",
                    "expectedRevision": revision,
                    "candidateId": candidate_id
                }),
                &mut session,
                &mut native_session,
                &options,
                &policy,
            )
            .await,
        );
        assert_eq!(executed["status"], "executed");

        let delta = invoke_json(
            invoke_native_mcp_tool(
                "observeDelta",
                json!({}),
                &mut session,
                &mut native_session,
                &options,
                &policy,
            )
            .await,
        );
        assert!(delta["toRevision"].as_u64().unwrap() >= delta["fromRevision"].as_u64().unwrap());

        let fresh_knowledge = invoke_json(
            invoke_native_mcp_tool(
                "observeKnowledge",
                json!({"freshOnly": true, "level": "summary"}),
                &mut session,
                &mut native_session,
                &options,
                &policy,
            )
            .await,
        );
        assert_eq!(fresh_knowledge["mode"], "freshOnly");

        let checkpoint = invoke_json(
            invoke_native_mcp_tool(
                "exportCheckpoint",
                json!({}),
                &mut session,
                &mut native_session,
                &options,
                &policy,
            )
            .await,
        );
        assert_eq!(checkpoint["attachMode"], false);
        let imported = invoke_native_mcp_tool(
            "importCheckpoint",
            json!({"checkpoint": checkpoint}),
            &mut session,
            &mut native_session,
            &options,
            &policy,
        )
        .await;
        assert!(imported.error.is_none());

        let diagnostics = invoke_json(
            invoke_native_mcp_tool(
                "diagnostics",
                json!({"durationMs": 1}),
                &mut session,
                &mut native_session,
                &options,
                &policy,
            )
            .await,
        );
        assert_eq!(diagnostics["target_id"], "native-context");
        assert_eq!(diagnostics["lifecycle"]["browserReady"], true);

        native_session.take().unwrap().close().await.unwrap();
    }

    #[cfg(feature = "native-engine")]
    #[test]
    fn native_mcp_routes_target_lifecycle_without_chromium() {
        std::thread::Builder::new()
            .name("glass-native-mcp-target-test".into())
            .stack_size(8 * 1024 * 1024)
            .spawn(|| {
                let runtime = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .expect("native MCP test runtime should build");
                runtime.block_on(native_mcp_routes_target_lifecycle_without_chromium_inner());
            })
            .expect("native MCP test thread should spawn")
            .join()
            .expect("native MCP test thread should not panic");
    }

    #[cfg(feature = "native-engine")]
    async fn native_mcp_routes_target_lifecycle_without_chromium_inner() {
        let mut session = None;
        let mut native_session = None;
        let options = SessionOptions::default();
        let policy = BrowserPolicy::development(std::env::current_dir().unwrap()).unwrap();
        let first = invoke_native_mcp_tool(
            "navigate",
            json!({"url": "data:text/html,%3Ctitle%3EFirst%3C%2Ftitle%3E%3Cp%3Efirst%3C%2Fp%3E"}),
            &mut session,
            &mut native_session,
            &options,
            &policy,
        )
        .await;
        assert!(first.error.is_none());

        let created = invoke_native_mcp_tool(
            "createTarget",
            json!({"url": "data:text/html,%3Ctitle%3ESecond%3C%2Ftitle%3E%3Cp%3Esecond%3C%2Fp%3E"}),
            &mut session,
            &mut native_session,
            &options,
            &policy,
        )
        .await;
        assert!(created.error.is_none());
        let created: Value = serde_json::from_str(
            created.result.unwrap()["content"][0]["text"]
                .as_str()
                .unwrap(),
        )
        .unwrap();
        assert_eq!(created["id"], "native-target-1");
        assert_eq!(created["title"], "Second");
        assert_eq!(created["active"], false);
        assert_eq!(created["opener_id"], "native-context");

        let selected = invoke_native_mcp_tool(
            "selectTarget",
            json!({"id": "native-target-1"}),
            &mut session,
            &mut native_session,
            &options,
            &policy,
        )
        .await;
        assert!(selected.error.is_none());
        let selected: Value = serde_json::from_str(
            selected.result.unwrap()["content"][0]["text"]
                .as_str()
                .unwrap(),
        )
        .unwrap();
        assert_eq!(selected["active"], true);

        let second_text = invoke_native_mcp_tool(
            "getText",
            json!({}),
            &mut session,
            &mut native_session,
            &options,
            &policy,
        )
        .await;
        assert!(second_text.error.is_none());
        assert_eq!(second_text.result.unwrap()["content"][0]["text"], "second");

        let restored = invoke_native_mcp_tool(
            "selectTarget",
            json!({"id": "native-context"}),
            &mut session,
            &mut native_session,
            &options,
            &policy,
        )
        .await;
        assert!(restored.error.is_none());

        let closed = invoke_native_mcp_tool(
            "closeTarget",
            json!({"id": "native-target-1"}),
            &mut session,
            &mut native_session,
            &options,
            &policy,
        )
        .await;
        assert!(closed.error.is_none());

        let targets = invoke_native_mcp_tool(
            "listTargets",
            json!({}),
            &mut session,
            &mut native_session,
            &options,
            &policy,
        )
        .await;
        assert!(targets.error.is_none());
        let targets: Value = serde_json::from_str(
            targets.result.unwrap()["content"][0]["text"]
                .as_str()
                .unwrap(),
        )
        .unwrap();
        assert_eq!(targets["result"].as_array().unwrap().len(), 1);
        assert_eq!(targets["result"][0]["id"], "native-context");
        native_session.take().unwrap().close().await.unwrap();
    }

    #[cfg(feature = "native-engine")]
    #[test]
    fn native_mcp_routes_dialog_lifecycle_without_chromium() {
        std::thread::Builder::new()
            .name("glass-native-mcp-dialog-test".into())
            .stack_size(8 * 1024 * 1024)
            .spawn(|| {
                let runtime = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .expect("native MCP test runtime should build");
                runtime.block_on(native_mcp_routes_dialog_lifecycle_without_chromium_inner());
            })
            .expect("native MCP test thread should spawn")
            .join()
            .expect("native MCP test thread should not panic");
    }

    #[cfg(feature = "native-engine")]
    async fn native_mcp_routes_dialog_lifecycle_without_chromium_inner() {
        let mut session = None;
        let mut native_session = None;
        let options = SessionOptions::default();
        let policy = BrowserPolicy::development(std::env::current_dir().unwrap()).unwrap();

        let navigate = invoke_native_mcp_tool(
            "navigate",
            json!({"url": "data:text/html,%3Ctitle%3EDialog%3C%2Ftitle%3E"}),
            &mut session,
            &mut native_session,
            &options,
            &policy,
        )
        .await;
        assert!(navigate.error.is_none());

        let evaluate = invoke_native_mcp_tool(
            "evaluate",
            json!({"expression": "alert('MCP dialog'); 'continued'"}),
            &mut session,
            &mut native_session,
            &options,
            &policy,
        )
        .await;
        assert!(evaluate.error.is_none());
        let value: Value = serde_json::from_str(
            evaluate.result.unwrap()["content"][0]["text"]
                .as_str()
                .unwrap(),
        )
        .unwrap();
        assert_eq!(value["result"], "continued");

        let open = invoke_native_mcp_tool(
            "verify",
            json!({"predicate": {"dialogOpen": true}, "timeoutMs": 100}),
            &mut session,
            &mut native_session,
            &options,
            &policy,
        )
        .await;
        assert!(open.error.is_none());
        let open: Value =
            serde_json::from_str(open.result.unwrap()["content"][0]["text"].as_str().unwrap())
                .unwrap();
        assert_eq!(open["status"], "satisfied");

        let accepted = invoke_native_mcp_tool(
            "acceptDialog",
            json!({}),
            &mut session,
            &mut native_session,
            &options,
            &policy,
        )
        .await;
        assert!(accepted.error.is_none());
        let accepted: Value = serde_json::from_str(
            accepted.result.unwrap()["content"][0]["text"]
                .as_str()
                .unwrap(),
        )
        .unwrap();
        assert_eq!(accepted["handled"], true);

        let closed = invoke_native_mcp_tool(
            "verify",
            json!({"predicate": {"dialogOpen": false}, "timeoutMs": 100}),
            &mut session,
            &mut native_session,
            &options,
            &policy,
        )
        .await;
        assert!(closed.error.is_none());

        native_session.take().unwrap().close().await.unwrap();
    }

    #[cfg(feature = "native-engine")]
    #[test]
    fn native_mcp_stdio_elicitation_resumes_process_page_dialogs() {
        std::thread::Builder::new()
            .name("glass-mcp-dialog-test".into())
            .stack_size(8 * 1024 * 1024)
            .spawn(|| {
                let runtime = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .expect("native MCP test runtime should build");
                let local = tokio::task::LocalSet::new();
                runtime.block_on(local.run_until(native_mcp_stdio_elicitation_inner()));
            })
            .expect("native MCP test thread should spawn")
            .join()
            .expect("native MCP test thread should not panic");
    }

    #[cfg(feature = "native-engine")]
    async fn native_mcp_stdio_elicitation_inner() {
        use clap::Parser as _;

        let html = "<!doctype html><title>Dialog</title><body><script>var name=prompt('Name','Ada');var accepted=confirm('Save?');document.title='Saved '+name+' '+accepted;document.body.textContent=document.title;alert('Saved '+name);</script></body>";
        let single_dialog_html = "<!doctype html><title>Single dialog</title><body><script>var answer=prompt('Single prompt','');document.body.textContent='Prompt dismissed '+(answer===null);</script></body>";
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let page = html.to_owned();
        let page_server = tokio::task::spawn_local(async move {
            loop {
                let (mut stream, _) = listener.accept().await.unwrap();
                let mut request = [0; 4096];
                let read = stream.read(&mut request).await.unwrap_or_default();
                let request_line = String::from_utf8_lossy(&request[..read]);
                let (status, body) = if request_line.starts_with("GET /single HTTP/") {
                    ("200 OK", single_dialog_html)
                } else if request_line.starts_with("GET / HTTP/") {
                    ("200 OK", page.as_str())
                } else {
                    ("404 Not Found", "")
                };
                let response = format!(
                    "HTTP/1.1 {status}\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                let _ = stream.write_all(response.as_bytes()).await;
            }
        });

        let cli = Cli::try_parse_from(["glass", "--mcp"]).unwrap();
        assert_eq!(
            cli.browser_runtime,
            crate::browser::runtime::BrowserRuntime::Native
        );
        let (mut client_to_server, server_reader) = tokio::io::duplex(1024 * 1024);
        let (server_writer, client_from_server) = tokio::io::duplex(1024 * 1024);
        let server = tokio::task::spawn_local(async move {
            run_mcp_stream_inner(
                BufReader::new(server_reader),
                server_writer,
                &cli,
                Arc::new(Mutex::new(None)),
                true,
                true,
                false,
                None,
                None,
            )
            .await
        });
        let mut client_from_server = BufReader::new(client_from_server);
        let initialize = json!({
            "jsonrpc":"2.0","id":"initialize","method":"initialize",
            "params":{
                "protocolVersion":MCP_PROTOCOL_VERSION_ELICITATION,
                "capabilities":{"elicitation":{"form":{}}},
                "clientInfo":{"name":"glass-mcp-test","version":"1"}
            }
        });
        write_test_mcp_message(&mut client_to_server, &initialize).await;
        let initialized = read_test_mcp_message(&mut client_from_server).await;
        assert_eq!(
            initialized["result"]["protocolVersion"],
            MCP_PROTOCOL_VERSION_ELICITATION
        );
        write_test_mcp_message(
            &mut client_to_server,
            &json!({"jsonrpc":"2.0","method":"notifications/initialized"}),
        )
        .await;
        write_test_mcp_message(
            &mut client_to_server,
            &json!({
                "jsonrpc":"2.0","id":"navigate","method":"tools/call",
                "params":{"name":"navigate","arguments":{
                    "url":format!("http://{address}/"),"timeoutMs":30_000,"includeTrace":true
                }}
            }),
        )
        .await;

        let response = tokio::time::timeout(Duration::from_secs(30), async {
            let mut observed_dialogs = Vec::new();
            let mut delayed_human_reply = false;
            loop {
                let message = read_test_mcp_message(&mut client_from_server).await;
                if message.get("method").and_then(Value::as_str) == Some("elicitation/create") {
                    let prompt = message["params"]["message"].as_str().unwrap();
                    let result = if prompt.contains("prompt dialog") {
                        observed_dialogs.push("prompt");
                        if !delayed_human_reply {
                            tokio::time::sleep(Duration::from_millis(100)).await;
                            delayed_human_reply = true;
                        }
                        json!({"action":"accept","content":{"response":"Grace"}})
                    } else if prompt.contains("confirm dialog") {
                        observed_dialogs.push("confirm");
                        json!({"action":"accept","content":{"accepted":true}})
                    } else {
                        observed_dialogs.push("alert");
                        json!({"action":"accept","content":{}})
                    };
                    write_test_mcp_message(
                        &mut client_to_server,
                        &json!({"jsonrpc":"2.0","id":message["id"],"result":result}),
                    )
                    .await;
                    continue;
                }
                assert_eq!(message["id"], "navigate");
                assert!(message.get("error").is_none(), "{message}");
                assert_ne!(message["result"]["isError"], true, "{message}");
                assert_eq!(
                    observed_dialogs,
                    ["prompt", "confirm", "alert"],
                    "unexpected MCP response: {message}"
                );
                assert_ne!(message["result"]["isError"], true, "{message}");
                break;
            }
        })
        .await
        .expect("native MCP dialog sequence should finish");
        let _ = response;
        write_test_mcp_message(
            &mut client_to_server,
            &json!({
                "jsonrpc":"2.0","id":"page-text","method":"tools/call",
                "params":{"name":"getText","arguments":{}}
            }),
        )
        .await;
        let page_text = read_test_mcp_message(&mut client_from_server).await;
        assert_eq!(page_text["id"], "page-text");
        assert!(
            page_text["result"]["content"][0]["text"]
                .as_str()
                .unwrap()
                .contains("Saved Grace true")
        );

        write_test_mcp_message(
            &mut client_to_server,
            &json!({
                "jsonrpc":"2.0","id":"deadline","method":"tools/call",
                "params":{"name":"navigate","arguments":{
                    "url":format!("http://{address}/single"),
                    "timeoutMs":5_000
                }}
            }),
        )
        .await;
        let deadline_elicitation = tokio::time::timeout(Duration::from_secs(15), async {
            loop {
                let message = read_test_mcp_message(&mut client_from_server).await;
                if message.get("method").and_then(Value::as_str) == Some("elicitation/create") {
                    break message;
                }
                assert_ne!(
                    message["id"], "deadline",
                    "navigation finished before elicitation: {message}"
                );
            }
        })
        .await
        .expect("navigation deadline test should reach its page prompt");
        assert!(
            deadline_elicitation["params"]["message"]
                .as_str()
                .unwrap()
                .contains("prompt dialog")
        );
        tokio::time::sleep(Duration::from_millis(5_100)).await;
        write_test_mcp_message(
            &mut client_to_server,
            &json!({
                "jsonrpc":"2.0","id":deadline_elicitation["id"],
                "result":{"action":"accept","content":{"response":"still alive"}}
            }),
        )
        .await;
        let resumed = read_test_mcp_message(&mut client_from_server).await;
        assert_eq!(resumed["id"], "deadline");
        assert!(resumed.get("error").is_none(), "{resumed}");
        assert_ne!(resumed["result"]["isError"], true, "{resumed}");

        client_to_server.shutdown().await.unwrap();
        server.await.unwrap().unwrap();

        let cli = Cli::try_parse_from(["glass", "--mcp"]).unwrap();
        let (mut client_to_server, mut client_from_server, server) =
            start_native_test_mcp_connection(cli);
        initialize_native_test_mcp_connection(&mut client_to_server, &mut client_from_server).await;
        write_test_mcp_message(
            &mut client_to_server,
            &json!({
                "jsonrpc":"2.0","id":"cancel-me","method":"tools/call",
                "params":{"name":"navigate","arguments":{
                    "url":format!("http://{address}/single"),"timeoutMs":10_000
                }}
            }),
        )
        .await;
        let elicitation = tokio::time::timeout(Duration::from_secs(15), async {
            loop {
                let message = read_test_mcp_message(&mut client_from_server).await;
                if message.get("method").and_then(Value::as_str) == Some("elicitation/create") {
                    break message;
                }
            }
        })
        .await
        .expect("native MCP prompt should be elicited before cancellation");
        write_test_mcp_message(
            &mut client_to_server,
            &json!({
                "jsonrpc":"2.0","method":"notifications/cancelled",
                "params":{"requestId":"cancel-me"}
            }),
        )
        .await;
        let cancellation = read_test_mcp_message(&mut client_from_server).await;
        assert_eq!(cancellation["method"], "notifications/cancelled");
        assert_eq!(cancellation["params"]["requestId"], elicitation["id"]);
        let cancelled = read_test_mcp_message(&mut client_from_server).await;
        assert_eq!(cancelled["id"], "cancel-me");
        assert_eq!(cancelled["error"]["code"], -32800);
        write_test_mcp_message(
            &mut client_to_server,
            &json!({
                "jsonrpc":"2.0","id":"after-cancel","method":"tools/call",
                "params":{"name":"getText","arguments":{}}
            }),
        )
        .await;
        let after_cancel = read_test_mcp_message(&mut client_from_server).await;
        assert_eq!(after_cancel["id"], "after-cancel");
        assert_ne!(after_cancel["result"]["isError"], true, "{after_cancel}");
        assert!(
            after_cancel["result"]["content"][0]["text"]
                .as_str()
                .unwrap()
                .contains("Prompt dismissed true")
        );
        client_to_server.shutdown().await.unwrap();
        tokio::time::timeout(Duration::from_secs(10), server)
            .await
            .expect("cancelled native MCP session should shut down")
            .unwrap()
            .unwrap();

        let cli = Cli::try_parse_from(["glass", "--mcp"]).unwrap();
        let (mut client_to_server, mut client_from_server, server) =
            start_native_test_mcp_connection(cli);
        initialize_native_test_mcp_connection(&mut client_to_server, &mut client_from_server).await;
        write_test_mcp_message(
            &mut client_to_server,
            &json!({
                "jsonrpc":"2.0","id":"eof-me","method":"tools/call",
                "params":{"name":"navigate","arguments":{
                    "url":format!("http://{address}/single"),"timeoutMs":10_000
                }}
            }),
        )
        .await;
        tokio::time::timeout(Duration::from_secs(15), async {
            loop {
                let message = read_test_mcp_message(&mut client_from_server).await;
                if message.get("method").and_then(Value::as_str) == Some("elicitation/create") {
                    break;
                }
            }
        })
        .await
        .expect("native MCP prompt should be elicited before stdio EOF");
        client_to_server.shutdown().await.unwrap();
        tokio::time::timeout(Duration::from_secs(10), server)
            .await
            .expect("stdio EOF should release the modal native browser call")
            .unwrap()
            .unwrap();
        page_server.abort();
    }

    #[cfg(all(unix, feature = "native-engine"))]
    struct NativePersistentMcpTestCleanup {
        paths: crate::browser::persistent::PersistentSessionPaths,
        owner: Option<tokio::task::JoinHandle<BrowserResult<()>>>,
    }

    #[cfg(all(unix, feature = "native-engine"))]
    impl Drop for NativePersistentMcpTestCleanup {
        fn drop(&mut self) {
            if let Some(owner) = &self.owner {
                owner.abort();
            }
            for path in [&self.paths.socket, &self.paths.status] {
                if path.exists() {
                    let _ = std::fs::remove_file(path);
                }
            }
        }
    }

    #[cfg(all(unix, feature = "native-engine"))]
    #[test]
    fn native_persistent_mcp_stdio_elicitation_keeps_owner_usable() {
        let session_name = format!(
            "mcp-dialog-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        let paths = crate::browser::persistent::paths(&session_name).unwrap();
        let storage_path = std::env::temp_dir().join(format!(
            "glass-native-persistent-mcp-storage-{}.json",
            session_name
        ));
        std::thread::Builder::new()
            .name("glass-mcp-persistent-dialog-test".into())
            .stack_size(8 * 1024 * 1024)
            .spawn(move || {
                let runtime = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .expect("persistent native MCP test runtime should build");
                let local = tokio::task::LocalSet::new();
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    runtime.block_on(local.run_until(
                        native_persistent_mcp_stdio_elicitation_inner(
                            session_name.clone(),
                            storage_path.clone(),
                        ),
                    ));
                }));
                drop(local);
                drop(runtime);
                for path in [&paths.socket, &paths.status, &storage_path] {
                    if path.exists() {
                        let _ = std::fs::remove_file(path);
                    }
                }
                let lock_path = storage_path.with_extension("lock");
                if lock_path.exists() {
                    let _ = std::fs::remove_file(lock_path);
                }
                if let Err(panic) = result {
                    std::panic::resume_unwind(panic);
                }
            })
            .expect("persistent native MCP test thread should spawn")
            .join()
            .expect("persistent native MCP test thread should not panic");
    }

    #[cfg(all(unix, feature = "native-engine"))]
    async fn native_persistent_mcp_stdio_elicitation_inner(
        session_name: String,
        storage_path: std::path::PathBuf,
    ) {
        use clap::Parser as _;

        let dialog_html = r#"<!doctype html><title>Persistent dialog</title><body><script>
window.dialogTrace = ["before"];
window.dialogTrace.push("confirm:" + confirm("continue?"));
window.dialogTrace.push("prompt:" + prompt("name?", "Ada"));
alert("finish");
window.dialogTrace.push("after");
document.body.textContent = window.dialogTrace.join("|");
</script></body>"#;
        let warm_html = "<!doctype html><title>Warm</title><body>Native MCP warm page</body>";
        let single_dialog_html = "<!doctype html><title>Single dialog</title><body><script>var answer=prompt('Single prompt','');document.body.textContent='Prompt dismissed '+(answer===null);</script></body>";
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let observed_paths = Arc::new(StdMutex::new(Vec::new()));
        let page_observed_paths = Arc::clone(&observed_paths);
        let page_server = tokio::task::spawn_local(async move {
            loop {
                let (stream, _) = listener.accept().await.unwrap();
                let mut reader = BufReader::new(stream);
                let mut request_line = String::new();
                if reader
                    .read_line(&mut request_line)
                    .await
                    .unwrap_or_default()
                    == 0
                {
                    continue;
                }
                loop {
                    let mut header = String::new();
                    if reader.read_line(&mut header).await.unwrap_or_default() == 0
                        || header == "\r\n"
                    {
                        break;
                    }
                }
                let mut stream = reader.into_inner();
                let path = request_line.split_whitespace().nth(1).unwrap_or_default();
                page_observed_paths.lock().unwrap().push(path.to_owned());
                let (status, body) = if path.starts_with("/warm") {
                    ("200 OK", warm_html)
                } else if path.starts_with("/single") {
                    ("200 OK", single_dialog_html)
                } else if path.starts_with("/dialogs") {
                    ("200 OK", dialog_html)
                } else {
                    ("404 Not Found", "")
                };
                let response = format!(
                    "HTTP/1.1 {status}\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                stream.write_all(response.as_bytes()).await.unwrap();
            }
        });

        let paths = crate::browser::persistent::paths(&session_name).unwrap();
        let policy = BrowserPolicy::development(std::env::current_dir().unwrap()).unwrap();
        let owner = tokio::task::spawn_local(crate::browser::persistent::serve(
            crate::browser::persistent::PersistentSessionServeConfig {
                name: session_name.clone(),
                socket: paths.socket.clone(),
                status_path: paths.status.clone(),
                runtime: crate::browser::runtime::BrowserRuntime::Native,
                port: 0,
                profile: session_name.clone(),
                headed: false,
                chrome_path: None,
                policy,
                native_config: Some(NativeEngineConfig::default().with_storage_path(&storage_path)),
            },
        ));
        let mut cleanup = NativePersistentMcpTestCleanup {
            paths: paths.clone(),
            owner: Some(owner),
        };
        let record = tokio::time::timeout(Duration::from_secs(15), async {
            loop {
                if let Some(record) = crate::browser::persistent::read_record(&session_name)
                    .unwrap()
                    .filter(|record| record.state == "running")
                {
                    break record;
                }
                tokio::time::sleep(Duration::from_millis(25)).await;
            }
        })
        .await
        .expect("native persistent owner should become ready");
        assert_eq!(
            record.runtime,
            crate::browser::runtime::BrowserRuntime::Native
        );
        assert_eq!(record.browser_pid, 0);
        assert_eq!(record.port, 0);
        let idle_status = crate::browser::persistent::native_mcp_dialog_status(&session_name)
            .await
            .unwrap();
        assert!(idle_status.pending_dialog.is_none());
        assert!(idle_status.active_navigation_revision.is_none());

        let cli = Cli::try_parse_from([
            "glass",
            "--browser-runtime",
            "native",
            "--session",
            session_name.as_str(),
            "--mcp",
        ])
        .unwrap();
        assert_eq!(
            cli.browser_runtime,
            crate::browser::runtime::BrowserRuntime::Native
        );
        assert_eq!(cli.session.as_deref(), Some(session_name.as_str()));
        let (mut client_to_server, mut client_from_server, server) =
            start_native_test_mcp_connection(cli);
        initialize_native_test_mcp_connection(&mut client_to_server, &mut client_from_server).await;
        write_test_mcp_message(
            &mut client_to_server,
            &json!({
                "jsonrpc":"2.0","id":"warm","method":"tools/call",
                "params":{"name":"observe","arguments":{}}
            }),
        )
        .await;
        let warm = read_test_mcp_message(&mut client_from_server).await;
        assert_eq!(warm["id"], "warm");
        assert_ne!(warm["result"]["isError"], true, "{warm}");
        write_test_mcp_message(
            &mut client_to_server,
            &json!({
                "jsonrpc":"2.0","id":"warm-navigation","method":"tools/call",
                "params":{"name":"navigate","arguments":{
                    "url":format!("http://{address}/warm"),"timeoutMs":30_000
                }}
            }),
        )
        .await;
        let warm_navigation = tokio::time::timeout(
            Duration::from_secs(35),
            read_test_mcp_message(&mut client_from_server),
        )
        .await
        .expect("persistent native MCP warm navigation should finish");
        assert_eq!(warm_navigation["id"], "warm-navigation");
        assert_ne!(
            warm_navigation["result"]["isError"], true,
            "{warm_navigation}"
        );
        write_test_mcp_message(
            &mut client_to_server,
            &json!({
                "jsonrpc":"2.0","id":"navigate","method":"tools/call",
                "params":{"name":"navigate","arguments":{
                    "url":format!("http://{address}/dialogs"),"timeoutMs":5_000,
                    "includeTrace":true
                }}
            }),
        )
        .await;
        let navigate = tokio::time::timeout(Duration::from_secs(25), async {
            let mut observed = Vec::new();
            let mut owner_status = None;
            loop {
                let message = tokio::select! {
                    message = read_test_mcp_message(&mut client_from_server) => message,
                    () = tokio::time::sleep(MCP_PERSISTENT_STATUS_POLL_INTERVAL) => {
                        owner_status = Some(
                            crate::browser::persistent::native_mcp_dialog_status(&session_name).await,
                        );
                        continue;
                    }
                };
                if message.get("method").and_then(Value::as_str) == Some("elicitation/create") {
                    let prompt = message["params"]["message"].as_str().unwrap();
                    let result = if prompt.contains("confirm dialog") {
                        observed.push("confirm");
                        json!({"action":"accept","content":{"accepted":true}})
                    } else if prompt.contains("prompt dialog") {
                        observed.push("prompt");
                        let status =
                            crate::browser::persistent::native_mcp_dialog_status(&session_name)
                                .await
                                .unwrap();
                        assert!(status.pending_dialog.is_some());
                        assert!(status.active_navigation_revision.is_some());
                        tokio::time::sleep(Duration::from_millis(5_100)).await;
                        json!({"action":"accept","content":{"response":"Grace"}})
                    } else {
                        observed.push("alert");
                        json!({"action":"accept","content":{}})
                    };
                    write_test_mcp_message(
                        &mut client_to_server,
                        &json!({"jsonrpc":"2.0","id":message["id"],"result":result}),
                    )
                    .await;
                    continue;
                }
                assert_eq!(message["id"], "navigate");
                assert!(message.get("error").is_none(), "{message}");
                let page_text_after_failure = if message["result"]["isError"] == true {
                    crate::browser::persistent::execute_native_mcp(
                        &session_name,
                        json!({"name":"getText","arguments":{}}),
                    )
                    .await
                    .ok()
                } else {
                    None
                };
                assert_ne!(
                    message["result"]["isError"],
                    true,
                    "{message}; owner status: {owner_status:?}; HTTP paths: {:?}; page text: {page_text_after_failure:?}",
                    observed_paths.lock().unwrap()
                );
                assert_eq!(observed, ["confirm", "prompt", "alert"], "{message}");
                break message;
            }
        })
        .await
        .expect("persistent dialog sequence should finish beyond its navigation deadline");
        assert_eq!(navigate["id"], "navigate");
        write_test_mcp_message(
            &mut client_to_server,
            &json!({
                "jsonrpc":"2.0","id":"page-text","method":"tools/call",
                "params":{"name":"getText","arguments":{}}
            }),
        )
        .await;
        let page_text = read_test_mcp_message(&mut client_from_server).await;
        assert_eq!(page_text["id"], "page-text");
        assert!(
            page_text["result"]["content"][0]["text"]
                .as_str()
                .unwrap()
                .contains("before|confirm:true|prompt:Grace|after")
        );
        client_to_server.shutdown().await.unwrap();
        server.await.unwrap().unwrap();

        let old_client_cli = Cli::try_parse_from([
            "glass",
            "--browser-runtime",
            "native",
            "--session",
            session_name.as_str(),
            "--mcp",
        ])
        .unwrap();
        let (mut old_client_to_server, mut old_client_from_server, old_server) =
            start_native_test_mcp_connection(old_client_cli);
        let initialize = json!({
            "jsonrpc":"2.0","id":"initialize","method":"initialize",
            "params":{
                "protocolVersion":MCP_PROTOCOL_VERSION,
                "capabilities":{},
                "clientInfo":{"name":"glass-mcp-legacy-test","version":"1"}
            }
        });
        write_test_mcp_message(&mut old_client_to_server, &initialize).await;
        let initialized = read_test_mcp_message(&mut old_client_from_server).await;
        assert_eq!(
            initialized["result"]["protocolVersion"],
            MCP_PROTOCOL_VERSION
        );
        write_test_mcp_message(
            &mut old_client_to_server,
            &json!({"jsonrpc":"2.0","method":"notifications/initialized"}),
        )
        .await;
        write_test_mcp_message(
            &mut old_client_to_server,
            &json!({
                "jsonrpc":"2.0","id":"no-form","method":"tools/call",
                "params":{"name":"navigate","arguments":{
                    "url":format!("http://{address}/single"),"timeoutMs":5_000
                }}
            }),
        )
        .await;
        let no_form = tokio::time::timeout(
            Duration::from_secs(15),
            read_test_mcp_message(&mut old_client_from_server),
        )
        .await
        .expect("legacy MCP client should receive a bounded dialog error");
        assert_eq!(no_form["id"], "no-form");
        assert_eq!(no_form["result"]["isError"], true, "{no_form}");
        assert!(
            no_form["result"]["content"][0]["text"]
                .as_str()
                .unwrap()
                .contains("did not negotiate form elicitation"),
            "unexpected legacy-client dialog result: {no_form}"
        );
        write_test_mcp_message(
            &mut old_client_to_server,
            &json!({
                "jsonrpc":"2.0","id":"legacy-after-error","method":"tools/call",
                "params":{"name":"getText","arguments":{}}
            }),
        )
        .await;
        let after_no_form = read_test_mcp_message(&mut old_client_from_server).await;
        assert_eq!(after_no_form["id"], "legacy-after-error");
        assert_ne!(
            after_no_form["result"]["isError"], true,
            "persistent owner was unusable after dismissing the legacy client's dialog: {after_no_form}"
        );
        assert!(
            crate::browser::persistent::native_mcp_dialog_status(&session_name)
                .await
                .unwrap()
                .pending_dialog
                .is_none()
        );
        old_client_to_server.shutdown().await.unwrap();
        old_server.await.unwrap().unwrap();

        let cli = Cli::try_parse_from([
            "glass",
            "--browser-runtime",
            "native",
            "--session",
            session_name.as_str(),
            "--mcp",
        ])
        .unwrap();
        let (mut cancel_to_server, mut cancel_from_server, cancel_server) =
            start_native_test_mcp_connection(cli);
        initialize_native_test_mcp_connection(&mut cancel_to_server, &mut cancel_from_server).await;
        write_test_mcp_message(
            &mut cancel_to_server,
            &json!({
                "jsonrpc":"2.0","id":"cancel-me","method":"tools/call",
                "params":{"name":"navigate","arguments":{
                    "url":format!("http://{address}/single"),"timeoutMs":10_000
                }}
            }),
        )
        .await;
        let elicitation = tokio::time::timeout(Duration::from_secs(15), async {
            loop {
                let message = read_test_mcp_message(&mut cancel_from_server).await;
                if message.get("method").and_then(Value::as_str) == Some("elicitation/create") {
                    break message;
                }
            }
        })
        .await
        .expect("persistent MCP prompt should be elicited before cancellation");
        assert!(
            crate::browser::persistent::native_mcp_dialog_status(&session_name)
                .await
                .unwrap()
                .pending_dialog
                .is_some()
        );
        write_test_mcp_message(
            &mut cancel_to_server,
            &json!({
                "jsonrpc":"2.0","method":"notifications/cancelled",
                "params":{"requestId":"cancel-me"}
            }),
        )
        .await;
        let cancelled_elicitation = read_test_mcp_message(&mut cancel_from_server).await;
        assert_eq!(cancelled_elicitation["method"], "notifications/cancelled");
        assert_eq!(
            cancelled_elicitation["params"]["requestId"],
            elicitation["id"]
        );
        let cancelled = read_test_mcp_message(&mut cancel_from_server).await;
        assert_eq!(cancelled["id"], "cancel-me");
        assert_eq!(cancelled["error"]["code"], -32800);
        assert!(
            crate::browser::persistent::native_mcp_dialog_status(&session_name)
                .await
                .unwrap()
                .pending_dialog
                .is_none(),
            "owner retained a pending dialog after cancellation completed"
        );
        write_test_mcp_message(
            &mut cancel_to_server,
            &json!({
                "jsonrpc":"2.0","id":"after-cancel","method":"tools/call",
                "params":{"name":"getText","arguments":{}}
            }),
        )
        .await;
        let after_cancel = read_test_mcp_message(&mut cancel_from_server).await;
        assert_eq!(after_cancel["id"], "after-cancel");
        assert_ne!(
            after_cancel["result"]["isError"], true,
            "persistent owner was unusable after parent cancellation: {after_cancel}"
        );
        assert!(
            crate::browser::persistent::native_mcp_dialog_status(&session_name)
                .await
                .unwrap()
                .pending_dialog
                .is_none()
        );
        cancel_to_server.shutdown().await.unwrap();
        cancel_server.await.unwrap().unwrap();

        let cli = Cli::try_parse_from([
            "glass",
            "--browser-runtime",
            "native",
            "--session",
            session_name.as_str(),
            "--mcp",
        ])
        .unwrap();
        let (mut eof_to_server, mut eof_from_server, eof_server) =
            start_native_test_mcp_connection(cli);
        initialize_native_test_mcp_connection(&mut eof_to_server, &mut eof_from_server).await;
        write_test_mcp_message(
            &mut eof_to_server,
            &json!({
                "jsonrpc":"2.0","id":"eof-me","method":"tools/call",
                "params":{"name":"navigate","arguments":{
                    "url":format!("http://{address}/single"),"timeoutMs":10_000
                }}
            }),
        )
        .await;
        tokio::time::timeout(Duration::from_secs(15), async {
            loop {
                let message = read_test_mcp_message(&mut eof_from_server).await;
                if message.get("method").and_then(Value::as_str) == Some("elicitation/create") {
                    break;
                }
            }
        })
        .await
        .expect("persistent MCP prompt should be elicited before stdio EOF");
        eof_to_server.shutdown().await.unwrap();
        tokio::time::timeout(Duration::from_secs(10), eof_server)
            .await
            .expect("stdio EOF should release the persistent modal browser call")
            .unwrap()
            .unwrap();
        let after_eof = crate::browser::persistent::execute_native_mcp(
            &session_name,
            json!({"name":"getText","arguments":{}}),
        )
        .await
        .unwrap();
        assert!(
            after_eof["isError"] != true,
            "persistent owner was unusable after stdio EOF: {after_eof}"
        );
        assert!(
            crate::browser::persistent::native_mcp_dialog_status(&session_name)
                .await
                .unwrap()
                .pending_dialog
                .is_none()
        );
        let final_record = crate::browser::persistent::read_record(&session_name)
            .unwrap()
            .expect("persistent owner should survive MCP EOF");
        assert_eq!(final_record.pid, record.pid);
        assert_eq!(final_record.browser_pid, 0);

        crate::browser::persistent::stop(&session_name)
            .await
            .unwrap();
        cleanup.owner.take().unwrap().await.unwrap().unwrap();
        page_server.abort();
    }

    #[cfg(feature = "native-engine")]
    fn start_native_test_mcp_connection(
        cli: Cli,
    ) -> (
        tokio::io::DuplexStream,
        BufReader<tokio::io::DuplexStream>,
        tokio::task::JoinHandle<BrowserResult<()>>,
    ) {
        let (client_to_server, server_reader) = tokio::io::duplex(1024 * 1024);
        let (server_writer, client_from_server) = tokio::io::duplex(1024 * 1024);
        let server = tokio::task::spawn_local(async move {
            run_mcp_stream_inner(
                BufReader::new(server_reader),
                server_writer,
                &cli,
                Arc::new(Mutex::new(None)),
                true,
                true,
                false,
                None,
                None,
            )
            .await
        });
        (client_to_server, BufReader::new(client_from_server), server)
    }

    #[cfg(feature = "native-engine")]
    async fn initialize_native_test_mcp_connection(
        client_to_server: &mut tokio::io::DuplexStream,
        client_from_server: &mut BufReader<tokio::io::DuplexStream>,
    ) {
        let initialize = json!({
            "jsonrpc":"2.0","id":"initialize","method":"initialize",
            "params":{
                "protocolVersion":MCP_PROTOCOL_VERSION_ELICITATION,
                "capabilities":{"elicitation":{"form":{}}},
                "clientInfo":{"name":"glass-mcp-test","version":"1"}
            }
        });
        write_test_mcp_message(client_to_server, &initialize).await;
        let initialized = read_test_mcp_message(client_from_server).await;
        assert_eq!(
            initialized["result"]["protocolVersion"],
            MCP_PROTOCOL_VERSION_ELICITATION
        );
        write_test_mcp_message(
            client_to_server,
            &json!({"jsonrpc":"2.0","method":"notifications/initialized"}),
        )
        .await;
    }

    #[cfg(test)]
    async fn write_test_mcp_message<W: AsyncWrite + Unpin>(writer: &mut W, message: &Value) {
        writer
            .write_all(serde_json::to_string(message).unwrap().as_bytes())
            .await
            .unwrap();
        writer.write_all(b"\n").await.unwrap();
        writer.flush().await.unwrap();
    }

    #[cfg(test)]
    async fn read_test_mcp_message<R: AsyncBufRead + Unpin>(reader: &mut R) -> Value {
        let (body, format) = read_message(reader).await.unwrap().unwrap();
        assert_eq!(format, FrameFormat::Newline);
        serde_json::from_str(&body).unwrap()
    }

    #[tokio::test]
    async fn session_snapshot_read_is_browser_free() {
        let request: JsonRpcRequest = serde_json::from_value(json!({
            "jsonrpc": "2.0",
            "id": "snapshot",
            "method": "tools/call",
            "params": {
                "name": "sessionSnapshot",
                "arguments": {"operation": "inspect"}
            }
        }))
        .unwrap();
        let mut session = None;
        let policy = BrowserPolicy::development(std::env::current_dir().unwrap()).unwrap();
        let response = handle_request(
            &request,
            &mut session,
            &SessionOptions::default(),
            &policy,
            None,
        )
        .await
        .unwrap();
        assert_eq!(response.result.unwrap()["isError"], true);
        assert!(session.is_none());
    }

    #[tokio::test]
    async fn preflight_navigation_is_browser_free_and_machine_readable() {
        async fn invoke(policy: &BrowserPolicy, url: &str) -> (Value, Option<CdpBrowserSession>) {
            let request: JsonRpcRequest = serde_json::from_value(json!({
                "jsonrpc": "2.0",
                "id": "preflight",
                "method": "tools/call",
                "params": {
                    "name": "preflightNavigation",
                    "arguments": {"url": url}
                }
            }))
            .unwrap();
            let mut session = None;
            let response = handle_request(
                &request,
                &mut session,
                &SessionOptions::default(),
                policy,
                None,
            )
            .await
            .unwrap();
            let text = response
                .result
                .as_ref()
                .and_then(|result| result["content"][0]["text"].as_str())
                .expect("preflight result text");
            (serde_json::from_str(text).unwrap(), session)
        }

        let root = std::env::current_dir().unwrap();
        let allowed_policy = BrowserPolicy::development(&root).unwrap();
        let (allowed, session) = invoke(&allowed_policy, "example.com/path").await;
        assert_eq!(allowed["decision"], "allow");
        assert_eq!(allowed["normalizedUrl"], "https://example.com/path");
        assert_eq!(allowed["host"], "example.com");
        assert_eq!(allowed["confirmationRequired"], false);
        assert!(session.is_none());

        let denied_policy = allowed_policy
            .clone()
            .with_host_rules([], ["example.com".to_string()])
            .unwrap();
        let (denied, session) = invoke(&denied_policy, "https://example.com/").await;
        assert_eq!(denied["decision"], "deny");
        assert!(
            denied["reason"]
                .as_str()
                .unwrap()
                .contains("explicitly denied")
        );
        assert!(session.is_none());

        let (malformed, session) = invoke(&allowed_policy, "http://[").await;
        assert_eq!(malformed["decision"], "deny");
        assert!(
            malformed["reason"]
                .as_str()
                .unwrap()
                .contains("URL is invalid")
        );
        assert!(session.is_none());
    }

    #[tokio::test]
    async fn inspect_web_ir_tool_is_browser_free_and_bounded() {
        let request: JsonRpcRequest = serde_json::from_value(json!({
            "jsonrpc": "2.0",
            "id": "inspect-ir",
            "method": "tools/call",
            "params": {
                "name": "inspectWebIr",
                "arguments": {"ir": valid_web_ir_fixture()}
            }
        }))
        .unwrap();
        let mut session = None;
        let policy = BrowserPolicy::development(std::env::current_dir().unwrap()).unwrap();
        let response = handle_request(
            &request,
            &mut session,
            &SessionOptions::default(),
            &policy,
            None,
        )
        .await
        .unwrap();
        let result_value = response.result.unwrap();
        let text = result_value["content"][0]["text"]
            .as_str()
            .unwrap()
            .to_string();
        let result: crate::protocol::WebIrInspectionResult = serde_json::from_str(&text).unwrap();
        assert_eq!(result.schema_version, 1);
        assert_eq!(result.revision, 7);
        assert_eq!(result.entity_count, 2);
        assert_eq!(result.relationship_count, 1);
        assert!(!text.contains("field-1"));
        assert!(session.is_none());
    }

    #[tokio::test]
    async fn task_tools_return_typed_invalid_task_without_starting_chrome() {
        let task = json!({
            "schemaVersion": 1,
            "task": "form.fill",
            "scope": {"regionName": "Checkout"},
            "limits": {"maxActions": 4, "timeoutMs": 2000, "maxItems": 16},
            "risk": "readOnly"
        });
        let policy = BrowserPolicy::development(std::env::current_dir().unwrap()).unwrap();

        for name in ["validateTask", "compileTask"] {
            let mut arguments = json!({"task": task.clone()});
            if name == "compileTask" {
                arguments["ir"] =
                    serde_json::to_value(crate::task_compiler::test_compiler_ir()).unwrap();
            }
            let request: JsonRpcRequest = serde_json::from_value(json!({
                "jsonrpc": "2.0",
                "id": name,
                "method": "tools/call",
                "params": {
                    "name": name,
                    "arguments": arguments
                }
            }))
            .unwrap();
            let mut session = None;
            let response = handle_request(
                &request,
                &mut session,
                &SessionOptions::default(),
                &policy,
                None,
            )
            .await
            .unwrap();
            let result = response.result.unwrap();
            let expected_kind = if name == "validateTask" {
                "taskValidation"
            } else {
                "taskCompilation"
            };
            assert_eq!(result["isError"], true);
            let error: Value =
                serde_json::from_str(result["content"][0]["text"].as_str().unwrap()).unwrap();
            assert_eq!(
                error,
                json!({
                    "kind": expected_kind,
                    "path": "inputs",
                    "reason": "form.fill requires at least one bounded input"
                })
            );
            assert!(session.is_none());
        }
    }

    #[tokio::test]
    async fn validate_web_ir_tool_returns_typed_invalid_draft_without_starting_chrome() {
        let mut draft = valid_web_ir_fixture();
        draft["relationships"][0]["to"] = json!("missing");
        let request: JsonRpcRequest = serde_json::from_value(json!({
            "jsonrpc": "2.0",
            "id": "invalid-ir",
            "method": "tools/call",
            "params": {
                "name": "validateWebIr",
                "arguments": {"ir": draft}
            }
        }))
        .unwrap();
        let mut session = None;
        let policy = BrowserPolicy::development(std::env::current_dir().unwrap()).unwrap();
        let response = handle_request(
            &request,
            &mut session,
            &SessionOptions::default(),
            &policy,
            None,
        )
        .await
        .unwrap();
        let result = response.result.unwrap();
        assert_eq!(result["isError"], true);
        let error: Value =
            serde_json::from_str(result["content"][0]["text"].as_str().unwrap()).unwrap();
        assert_eq!(
            error,
            json!({
                "kind": "webIrValidation",
                "path": "relationships[0]",
                "reason": "relationships must reference two distinct known entities"
            })
        );
        assert!(session.is_none());
    }

    #[tokio::test]
    async fn diff_web_ir_tool_returns_bounded_summary_without_starting_chrome() {
        let mut after = valid_web_ir_fixture();
        after["revision"] = json!(8);
        after["document"]["revision"] = json!(8);
        after["entities"][1]["name"] = json!("Email address");
        let request: JsonRpcRequest = serde_json::from_value(json!({
            "jsonrpc": "2.0",
            "id": "diff-ir",
            "method": "tools/call",
            "params": {
                "name": "diffWebIr",
                "arguments": {
                    "before": valid_web_ir_fixture(),
                    "after": after
                }
            }
        }))
        .unwrap();
        let mut session = None;
        let policy = BrowserPolicy::development(std::env::current_dir().unwrap()).unwrap();
        let response = handle_request(
            &request,
            &mut session,
            &SessionOptions::default(),
            &policy,
            None,
        )
        .await
        .unwrap();
        let text = response.result.unwrap()["content"][0]["text"]
            .as_str()
            .unwrap()
            .to_string();
        let result: crate::protocol::WebIrDiffResult = serde_json::from_str(&text).unwrap();
        assert_eq!(result.from_revision, 7);
        assert_eq!(result.to_revision, 8);
        assert_eq!(result.entity_changed_count, 1);
        assert_eq!(result.entity_added_count, 0);
        assert!(!text.contains("field-1"));
        assert!(session.is_none());
    }

    #[tokio::test]
    async fn continuity_web_ir_tool_classifies_entity_without_starting_chrome() {
        let mut after = valid_web_ir_fixture();
        after["revision"] = json!(8);
        after["document"]["revision"] = json!(8);
        after["entities"][1]["name"] = json!("Email address");
        let request: JsonRpcRequest = serde_json::from_value(json!({
            "jsonrpc": "2.0",
            "id": "continuity-ir",
            "method": "tools/call",
            "params": {
                "name": "continuityWebIr",
                "arguments": {
                    "before": valid_web_ir_fixture(),
                    "after": after,
                    "entityId": "field-1"
                }
            }
        }))
        .unwrap();
        let mut session = None;
        let policy = BrowserPolicy::development(std::env::current_dir().unwrap()).unwrap();
        let response = handle_request(
            &request,
            &mut session,
            &SessionOptions::default(),
            &policy,
            None,
        )
        .await
        .unwrap();
        let result_value = response.result.unwrap();
        let text = result_value["content"][0]["text"].as_str().unwrap();
        let result: crate::protocol::WebIrContinuityResult = serde_json::from_str(text).unwrap();
        assert_eq!(result.requested_id, "field-1");
        assert_eq!(
            result.status,
            crate::web_ir::WebIrEntityContinuityStatus::Changed
        );
        assert_eq!(result.current_id.as_deref(), Some("field-1"));
        assert!(session.is_none());
    }

    #[tokio::test]
    async fn compile_task_tool_returns_a_plan_without_starting_chrome() {
        let request: JsonRpcRequest = serde_json::from_value(json!({
            "jsonrpc": "2.0",
            "id": "compile",
            "method": "tools/call",
            "params": {
                "name": "compileTask",
                "arguments": {
                    "task": {
                        "schemaVersion": 1,
                        "task": "region.extract",
                        "scope": {"regionName": "Checkout", "entityKind": "region"},
                        "limits": {"maxActions": 4, "timeoutMs": 2000, "maxItems": 16},
                        "risk": "readOnly"
                    },
                    "ir": crate::task_compiler::test_compiler_ir()
                }
            }
        }))
        .unwrap();
        let mut session = None;
        let policy = BrowserPolicy::development(std::env::current_dir().unwrap()).unwrap();
        let response = handle_request(
            &request,
            &mut session,
            &SessionOptions::default(),
            &policy,
            None,
        )
        .await
        .unwrap();
        let result_value = response.result.unwrap();
        let text = result_value["content"][0]["text"].as_str().unwrap();
        let result: crate::protocol::TaskCompileResult = serde_json::from_str(text).unwrap();
        assert_eq!(
            result.plan.task,
            crate::task_protocol::TaskKind::RegionExtract
        );
        assert!(session.is_none());
    }

    #[tokio::test]
    async fn validate_task_tool_is_browser_free_and_redacts_inputs() {
        let request: JsonRpcRequest = serde_json::from_value(json!({
            "jsonrpc": "2.0",
            "id": "validate",
            "method": "tools/call",
            "params": {
                "name": "validateTask",
                "arguments": {
                    "task": {
                        "schemaVersion": 1,
                        "task": "form.fill",
                        "scope": {"regionName": "Checkout"},
                        "inputs": {"city": "sensitive-city"},
                        "limits": {"maxActions": 4, "timeoutMs": 2000, "maxItems": 16},
                        "risk": "localMutation"
                    }
                }
            }
        }))
        .unwrap();
        let mut session = None;
        let policy = BrowserPolicy::development(std::env::current_dir().unwrap()).unwrap();
        let response = handle_request(
            &request,
            &mut session,
            &SessionOptions::default(),
            &policy,
            None,
        )
        .await
        .unwrap();
        let text = response.result.unwrap()["content"][0]["text"]
            .as_str()
            .unwrap()
            .to_string();
        let result: crate::protocol::TaskValidationResult = serde_json::from_str(&text).unwrap();
        assert!(result.valid);
        assert_eq!(result.schema_version, 1);
        assert_eq!(result.task, crate::task_protocol::TaskKind::FormFill);
        assert!(!text.contains("sensitive-city"));
        assert!(session.is_none());
    }

    #[tokio::test]
    async fn validate_task_tool_returns_typed_invalid_task_without_starting_chrome() {
        let request: JsonRpcRequest = serde_json::from_value(json!({
            "jsonrpc": "2.0",
            "id": "invalid-validate",
            "method": "tools/call",
            "params": {
                "name": "validateTask",
                "arguments": {
                    "task": {
                        "schemaVersion": 1,
                        "task": "form.fill",
                        "scope": {"regionName": "Checkout"},
                        "limits": {"maxActions": 4, "timeoutMs": 2000, "maxItems": 16},
                        "risk": "localMutation"
                    }
                }
            }
        }))
        .unwrap();
        let mut session = None;
        let policy = BrowserPolicy::development(std::env::current_dir().unwrap()).unwrap();
        let response = handle_request(
            &request,
            &mut session,
            &SessionOptions::default(),
            &policy,
            None,
        )
        .await
        .unwrap();
        let result = response.result.unwrap();
        assert_eq!(result["isError"], true);
        assert_eq!(
            serde_json::from_str::<Value>(result["content"][0]["text"].as_str().unwrap()).unwrap(),
            json!({
                "kind": "taskValidation",
                "path": "inputs",
                "reason": "form.fill requires at least one bounded input"
            })
        );
        assert!(session.is_none());
    }
    #[tokio::test]
    async fn compile_task_tool_returns_typed_invalid_task_without_starting_chrome() {
        let request: JsonRpcRequest = serde_json::from_value(json!({
            "jsonrpc": "2.0",
            "id": "invalid-compile",
            "method": "tools/call",
            "params": {
                "name": "compileTask",
                "arguments": {
                    "task": {
                        "schemaVersion": 1,
                        "task": "form.fill",
                        "scope": {"regionName": "Checkout"},
                        "limits": {"maxActions": 4, "timeoutMs": 2000, "maxItems": 16},
                        "risk": "localMutation"
                    },
                    "ir": crate::task_compiler::test_compiler_ir()
                }
            }
        }))
        .unwrap();
        let mut session = None;
        let policy = BrowserPolicy::development(std::env::current_dir().unwrap()).unwrap();
        let response = handle_request(
            &request,
            &mut session,
            &SessionOptions::default(),
            &policy,
            None,
        )
        .await
        .unwrap();
        let result = response.result.unwrap();
        assert_eq!(result["isError"], true);
        let error: Value =
            serde_json::from_str(result["content"][0]["text"].as_str().unwrap()).unwrap();
        assert_eq!(
            error,
            json!({
                "kind": "taskCompilation",
                "path": "inputs",
                "reason": "form.fill requires at least one bounded input"
            })
        );
        assert!(session.is_none());
    }

    #[test]
    fn rejects_unsupported_protocol_versions_without_echoing_them() {
        let request: JsonRpcRequest = serde_json::from_value(json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {"protocolVersion": "private-future-version"}
        }))
        .unwrap();

        let policy = BrowserPolicy::development(std::env::current_dir().unwrap()).unwrap();
        let response = initialize_response(&request, &policy);
        let error = response.error.unwrap();
        assert_eq!(error.code, -32602);
        assert_eq!(error.message, "unsupported MCP protocol version");
        assert!(!error.message.contains("private-future-version"));
    }

    #[test]
    fn rejects_an_incompatible_glass_schema_before_ready_state() {
        let request: JsonRpcRequest = serde_json::from_value(json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {
                "protocolVersion": "2024-11-05",
                "glass": {
                    "protocolVersion": 1,
                    "schemas": {"workflow": [99]}
                }
            }
        }))
        .unwrap();
        let policy = BrowserPolicy::development(std::env::current_dir().unwrap()).unwrap();

        let response = initialize_response(&request, &policy);
        let error = response.error.unwrap();
        assert_eq!(error.code, -32602);
        assert!(
            error
                .message
                .contains("Glass capability negotiation failed")
        );
    }

    #[test]
    fn cancellation_matches_string_and_numeric_request_ids() {
        for request_id in [json!(7), json!("task-7")] {
            let cancellations: CancellationMap = Arc::new(StdMutex::new(HashMap::new()));
            let (sender, mut receiver) = oneshot::channel();
            cancellations.lock().unwrap().insert(
                CancellationKey::Request(request_id_key(&request_id).unwrap()),
                sender,
            );
            let request: JsonRpcRequest = serde_json::from_value(json!({
                "jsonrpc": "2.0",
                "method": "notifications/cancelled",
                "params": {"requestId": request_id}
            }))
            .unwrap();

            cancel_request(&request, &cancellations);
            assert!(receiver.try_recv().is_ok());
            assert!(cancellations.lock().unwrap().is_empty());
        }
    }
    #[test]
    fn parses_preflight_navigation_url() {
        let oversized = json!({
            "name": "preflightNavigation",
            "arguments": {"url": "https://example.com/".to_string() + &"a".repeat(MAX_PREFLIGHT_URL_BYTES)}
        });
        assert!(parse_tool_invocation(&oversized).is_err());
        let extra = json!({
            "name": "preflightNavigation",
            "arguments": {"url": "example.com", "unexpected": true}
        });
        assert!(parse_tool_invocation(&extra).is_err());
        let params = json!({
            "name": "preflightNavigation",
            "arguments": {"url": "example.com"}
        });
        assert!(matches!(
            parse_tool_invocation(&params).unwrap(),
            ToolInvocation::PreflightNavigation { url: "example.com" }
        ));
        let missing = json!({"name": "preflightNavigation", "arguments": {}});
        assert!(parse_tool_invocation(&missing).is_err());
    }

    #[test]
    fn parses_revision_guarded_execute_task() {
        let params = json!({
            "name": "executeTask",
            "arguments": {
                "task": {
                    "schemaVersion": 1,
                    "task": "form.inspect",
                    "scope": {"regionName": "Checkout"},
                    "limits": {"maxActions": 4, "timeoutMs": 2000, "maxItems": 16},
                    "risk": "readOnly"
                },
                "expectedRevision": 17,
                "confirmed": true
            }
        });
        let ToolInvocation::ExecuteTask {
            task,
            expected_revision,
            confirmed,
        } = parse_tool_invocation(&params).unwrap()
        else {
            panic!("expected executeTask invocation");
        };
        assert_eq!(task.task, crate::task_protocol::TaskKind::FormInspect);
        assert_eq!(expected_revision, 17);
        assert!(confirmed);
    }

    #[test]
    fn execute_task_lease_requirement_matches_task_family() {
        let request = |task: &str| {
            serde_json::from_value::<JsonRpcRequest>(json!({
                "jsonrpc": "2.0",
                "id": 1,
                "method": "tools/call",
                "params": {
                    "name": "executeTask",
                    "arguments": {
                        "task": {
                            "schemaVersion": 1,
                            "task": task,
                            "scope": {"regionName": "Checkout"},
                            "limits": {"maxActions": 4, "timeoutMs": 2000, "maxItems": 16},
                            "risk": if task == "form.inspect" { "readOnly" } else { "localMutation" }
                        },
                        "expectedRevision": 17,
                        "confirmed": true
                    }
                }
            }))
            .unwrap()
        };

        assert!(!execute_task_requires_mutation_lease(&request(
            "form.inspect"
        )));
        assert!(!execute_task_requires_mutation_lease(&request(
            "table.extract"
        )));
        assert!(execute_task_requires_mutation_lease(&request("form.fill")));
        assert!(execute_task_requires_mutation_lease(&request(
            "navigation.follow"
        )));
    }
    #[test]
    fn parses_observe_bootstrap_with_response_modes() {
        let params = json!({
            "name": "observeBootstrap",
            "arguments": {"responseMode": "normal"}
        });
        assert!(matches!(
            parse_tool_invocation(&params).unwrap(),
            ToolInvocation::ObserveBootstrap
        ));
        assert_eq!(
            response_mode_from_params(&params).unwrap(),
            ResponseMode::Normal
        );

        let malformed = json!({
            "name": "observeBootstrap",
            "arguments": {"responseMode": "verbose"}
        });
        let error =
            response_mode_from_params(&malformed).expect_err("invalid response mode should fail");
        assert!(error.to_string().contains("responseMode must be"));
    }

    #[test]
    fn parses_observation_options_strictly() {
        let params = json!({
            "name": "observe",
            "arguments": {"includeDom": true, "includeScreenshot": false}
        });
        assert!(matches!(
            parse_tool_invocation(&params).unwrap(),
            ToolInvocation::Observe {
                include_dom: true,
                include_screenshot: false,
                ..
            }
        ));

        let invalid = json!({
            "name": "observe",
            "arguments": {"includeDom": "true"}
        });
        let error = parse_tool_invocation(&invalid)
            .err()
            .expect("invalid boolean option should fail");
        assert!(error.to_string().contains("includeDom must be a boolean"));

        let semantic = json!({
            "name": "observe",
            "arguments": {"level": "interactive", "region": "region_search_1"}
        });
        assert!(matches!(
            parse_tool_invocation(&semantic).unwrap(),
            ToolInvocation::Observe {
                level: Some(SemanticObservationLevel::Interactive),
                region: Some("region_search_1"),
                ..
            }
        ));

        let knowledge = json!({
            "name": "observeKnowledge",
            "arguments": {
                "level": "summary",
                "freshOnly": true,
                "profileScope": "anonymous",
                "locale": "en-US",
                "browserVersion": "120.0"
            }
        });
        let ToolInvocation::ObserveKnowledge {
            level,
            fresh_only,
            lookup,
        } = parse_tool_invocation(&knowledge).unwrap()
        else {
            panic!("expected knowledge observation invocation");
        };
        assert_eq!(level, SemanticObservationLevel::Summary);
        assert!(fresh_only);
        assert_eq!(lookup.profile_scope, KnowledgeProfileScope::Anonymous);
        assert_eq!(lookup.locale.as_deref(), Some("en-US"));
        assert_eq!(lookup.browser_version.as_deref(), Some("120.0"));

        let intent_with_knowledge = json!({
            "name": "resolveIntentWithKnowledge",
            "arguments": {
                "schemaVersion": 1,
                "intent": "open settings",
                "action": "click",
                "resolutionPolicy": "reportOnly",
                "profileScope": "anonymous"
            }
        });
        let ToolInvocation::ResolveIntentWithKnowledge { request, lookup } =
            parse_tool_invocation(&intent_with_knowledge).unwrap()
        else {
            panic!("expected knowledge-backed intent invocation");
        };
        assert_eq!(request.intent, "open settings");
        assert_eq!(lookup.profile_scope, KnowledgeProfileScope::Anonymous);

        let execute = json!({
            "name": "executeIntent",
            "arguments": {
                "schemaVersion": 1,
                "intent": "open settings",
                "action": "click",
                "resolutionPolicy": "interactiveConfirmation",
                "candidateId": "candidate_1",
                "expectedRevision": 42
            }
        });
        let ToolInvocation::ExecuteIntent { request } = parse_tool_invocation(&execute).unwrap()
        else {
            panic!("expected execute intent invocation");
        };
        assert_eq!(request.candidate_id, "candidate_1");
        assert_eq!(request.request.expected_revision, Some(42));

        let invalid_level = json!({
            "name": "observe",
            "arguments": {"level": "verbose"}
        });
        let error = match parse_tool_invocation(&invalid_level) {
            Ok(_) => panic!("invalid semantic level should be rejected"),
            Err(error) => error,
        };
        assert!(error.to_string().contains("level must be"));

        let intent = json!({
            "name": "resolveIntent",
            "arguments": {
                "schemaVersion": 1,
                "intent": "open settings",
                "action": "click",
                "resolutionPolicy": "reportOnly"
            }
        });
        assert!(matches!(
            parse_tool_invocation(&intent).unwrap(),
            ToolInvocation::ResolveIntent { request }
                if request.intent == "open settings"
        ));
    }

    #[test]
    fn parses_click_expect_popup_target_and_legacy_selector() {
        for (arguments, expected) in [
            (json!({"target": "css=#popup"}), "css=#popup"),
            (json!({"selector": "#popup"}), "css=#popup"),
        ] {
            let params = json!({"name": "clickExpectPopup", "arguments": arguments});
            assert!(matches!(
                parse_tool_invocation(&params).unwrap(),
                ToolInvocation::ClickExpectPopup {
                    target,
                    expected_revision: None,
                } if target == expected
            ));
        }
    }

    #[test]
    fn parses_revision_guard_without_changing_legacy_invocations() {
        let guarded_params = json!({
            "name": "click",
            "arguments": {"target": "r7:b42", "expectedRevision": 7}
        });
        let guarded = parse_tool_invocation(&guarded_params).unwrap();
        assert!(matches!(
            guarded,
            ToolInvocation::Click {
                expected_revision: Some(7),
                ..
            }
        ));

        let legacy_params = json!({
            "name": "click",
            "arguments": {"target": "Save"}
        });
        let legacy = parse_tool_invocation(&legacy_params).unwrap();
        assert!(matches!(
            legacy,
            ToolInvocation::Click {
                expected_revision: None,
                ..
            }
        ));
    }

    #[test]
    fn parses_revision_guards_for_extended_mutations() {
        let cases = [
            ("clickExpectPopup", json!({"target": "r7:b42"})),
            ("doubleClick", json!({"target": "r7:b42"})),
            ("clear", json!({"target": "r7:b42"})),
            ("check", json!({"target": "r7:b42"})),
            ("uncheck", json!({"target": "r7:b42"})),
            ("select", json!({"target": "r7:b42", "value": "on"})),
            ("scroll", json!({"dy": 20})),
            ("drag", json!({"source": "r7:b42", "destination": "r7:b43"})),
            ("key", json!({"key": "Enter"})),
            ("keyDown", json!({"key": "Shift"})),
            ("keyUp", json!({"key": "Shift"})),
            ("shortcut", json!({"shortcut": "Control+A"})),
            (
                "upload",
                json!({"target": "r7:b42", "files": ["/tmp/a.txt"]}),
            ),
        ];
        for (name, mut arguments) in cases {
            arguments["expectedRevision"] = json!(7);
            let params = json!({"name": name, "arguments": arguments});
            let invocation = parse_tool_invocation(&params).unwrap();
            let revision = match invocation {
                ToolInvocation::ClickExpectPopup {
                    expected_revision, ..
                }
                | ToolInvocation::DoubleClick {
                    expected_revision, ..
                }
                | ToolInvocation::Clear {
                    expected_revision, ..
                }
                | ToolInvocation::Check {
                    expected_revision, ..
                }
                | ToolInvocation::Uncheck {
                    expected_revision, ..
                }
                | ToolInvocation::Select {
                    expected_revision, ..
                }
                | ToolInvocation::Scroll {
                    expected_revision, ..
                }
                | ToolInvocation::Drag {
                    expected_revision, ..
                }
                | ToolInvocation::Key {
                    expected_revision, ..
                }
                | ToolInvocation::KeyDown {
                    expected_revision, ..
                }
                | ToolInvocation::KeyUp {
                    expected_revision, ..
                }
                | ToolInvocation::Shortcut {
                    expected_revision, ..
                }
                | ToolInvocation::Upload {
                    expected_revision, ..
                } => expected_revision,
                _ => None,
            };
            assert_eq!(revision, Some(7), "tool {name} lost expectedRevision");
        }
    }

    #[test]
    fn parses_batch_revision_modes() {
        for (mode, expected) in [
            ("fixed", BatchMode::Fixed),
            ("chain", BatchMode::Chain),
            ("unguarded", BatchMode::Unguarded),
        ] {
            let params = json!({
                "name": "batch",
                "arguments": {
                    "mode": mode,
                    "expectedRevision": 7,
                    "steps": [{"action": "scroll", "dy": 10}]
                }
            });
            assert!(matches!(
                parse_tool_invocation(&params).unwrap(),
                ToolInvocation::Batch {
                    mode: actual,
                    expected_revision: Some(7),
                    ..
                } if actual == expected
            ));
        }
    }

    #[test]
    fn parses_workflow_definition_and_inputs() {
        let params = json!({
            "name": "workflow",
            "arguments": {
                "workflow": {"schemaVersion": 1, "name": "demo"},
                "inputs": {"name": "Ada"}
            }
        });
        let ToolInvocation::Workflow {
            definition, inputs, ..
        } = parse_tool_invocation(&params).unwrap()
        else {
            panic!("expected workflow invocation");
        };
        assert_eq!(definition["name"], "demo");
        assert_eq!(inputs["name"], "Ada");
    }

    #[test]
    fn parses_bounded_verification_predicates() {
        let params = json!({
            "name": "verify",
            "arguments": {
                "timeoutMs": 5000,
                "predicate": {
                    "all": [
                        {"urlEquals": "https://example.test"},
                        {"any": [{"titleContains": "Ready"}, {"dialogOpen": false}]}
                    ]
                }
            }
        });
        let ToolInvocation::Verify {
            predicate,
            timeout_ms,
        } = parse_tool_invocation(&params).unwrap()
        else {
            panic!("expected verify invocation");
        };
        let predicate: VerificationPredicate = serde_json::from_value(predicate).unwrap();
        predicate.validate(0).unwrap();
        assert_eq!(timeout_ms, 5000);
    }

    #[test]
    fn serializes_popup_failures_as_typed_mcp_content() {
        let error = PopupClickError {
            kind: crate::browser::session::PopupClickErrorKind::PopupAmbiguous,
            message: "two opener-matching popups".to_string(),
        };
        let text = typed_browser_error(&error).expect("popup error should remain typed");
        assert_eq!(
            serde_json::from_str::<Value>(&text).unwrap(),
            json!({
                "kind": "popup_ambiguous",
                "message": "two opener-matching popups"
            })
        );
    }

    #[test]
    fn serializes_download_failures_as_typed_mcp_content() {
        for (kind, expected) in [
            (
                crate::browser::session::DownloadErrorKind::AuthorizationFailed,
                "authorization_failed",
            ),
            (
                crate::browser::session::DownloadErrorKind::RestorationFailed,
                "restoration_failed",
            ),
        ] {
            let error = DownloadError {
                kind,
                message: "bounded download failure".to_string(),
            };
            let text = typed_browser_error(&error).expect("download error should remain typed");
            assert_eq!(
                serde_json::from_str::<Value>(&text).unwrap(),
                json!({
                    "kind": expected,
                    "message": "bounded download failure"
                })
            );
        }
    }

    #[test]
    fn serializes_policy_denials_as_typed_mcp_content() {
        let error = crate::browser::policy::PolicyError::Denied {
            operation: "read_sensitive_extraction".to_string(),
            reason: "explicit capability required".to_string(),
        };
        let text = typed_browser_error(&error).expect("policy error should remain typed");
        let value: Value = serde_json::from_str(&text).unwrap();
        assert_eq!(value["kind"], "denied");
        assert_eq!(value["operation"], "read_sensitive_extraction");
        assert_eq!(value["ruleId"], "policy.read_sensitive_extraction.denied");
        assert_eq!(value["phase"], "preflight");
    }

    #[test]
    fn action_results_are_compact_json_text() {
        let result = action_result(ActionOutcome {
            status: ActionStatus::Succeeded,
            action: ActionKind::Scroll,
            execution_id: "act_test_1".to_string(),
            target: None,
            revision: 9,
            previous_revision: 8,
            current_revision: 9,
            target_id: "target-1".to_string(),
            frame_id: "frame-1".to_string(),
            verification: ActionVerificationEvidence {
                revision_delta: 1,
                ..ActionVerificationEvidence::default()
            },
            evidence: None,
        })
        .unwrap();
        let text = result["content"][0]["text"].as_str().unwrap();

        assert!(!text.contains('\n'));
        assert_eq!(
            serde_json::from_str::<Value>(text).unwrap(),
            json!({"status":"succeeded", "action": "scroll", "executionId":"act_test_1", "revision": 9, "previousRevision":8, "currentRevision":9, "target_id":"target-1", "frame_id":"frame-1", "verification":{"revisionDelta":1,"urlChanged":false,"titleChanged":false,"targetChanged":false,"frameChanged":false}})
        );
    }

    #[tokio::test]
    async fn rejects_invalid_tool_calls_without_starting_chrome() {
        let request: JsonRpcRequest = serde_json::from_value(json!({
            "jsonrpc": "2.0",
            "id": 7,
            "method": "tools/call",
            "params": {
                "name": "observe",
                "arguments": {"includeScreenshot": "yes"}
            }
        }))
        .unwrap();
        let mut session = None;
        let policy = BrowserPolicy::development(std::env::current_dir().unwrap()).unwrap();

        let response = handle_request(
            &request,
            &mut session,
            &SessionOptions::default(),
            &policy,
            None,
        )
        .await
        .unwrap();
        let result = response.result.unwrap();

        assert_eq!(result["isError"], true);
        assert_eq!(result["content"][0]["text"], "browser tool failed");
        assert!(!result.to_string().contains("yes"));
        assert!(session.is_none());
    }

    #[tokio::test]
    async fn knowledge_stats_does_not_start_chrome() {
        let path =
            std::env::temp_dir().join(format!("glass-mcp-knowledge-{}.json", std::process::id()));
        let request: JsonRpcRequest = serde_json::from_value(json!({
            "jsonrpc": "2.0",
            "id": 8,
            "method": "tools/call",
            "params": {"name": "knowledgeStats", "arguments": {}}
        }))
        .unwrap();
        let mut session = None;
        let policy = BrowserPolicy::development(std::env::current_dir().unwrap()).unwrap();

        let response = handle_request(
            &request,
            &mut session,
            &SessionOptions::default(),
            &policy,
            Some(&path),
        )
        .await
        .unwrap();

        assert!(response.error.is_none());
        assert!(response.result.as_ref().unwrap()["content"].is_array());
        assert!(session.is_none());
        let _ = std::fs::remove_file(path);
    }

    #[tokio::test]
    async fn memory_export_is_browser_free() {
        let path = std::env::temp_dir().join(format!(
            "glass-mcp-memory-export-{}.json",
            std::process::id()
        ));
        let request: JsonRpcRequest = serde_json::from_value(json!({
            "jsonrpc": "2.0",
            "id": 9,
            "method": "tools/call",
            "params": {"name": "memoryExport", "arguments": {}}
        }))
        .unwrap();
        let mut session = None;
        let policy = BrowserPolicy::development(std::env::current_dir().unwrap()).unwrap();

        let response = handle_request(
            &request,
            &mut session,
            &SessionOptions::default(),
            &policy,
            Some(&path),
        )
        .await
        .unwrap();

        assert!(response.error.is_none());
        assert_ne!(response.result.as_ref().unwrap()["isError"], true);
        assert!(session.is_none());
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn import_checkpoint_reads_the_advertised_checkpoint_object() {
        let params = json!({
            "name": "importCheckpoint",
            "arguments": {
                "checkpoint": {"schemaVersion": 1, "url": "https://example.test/"}
            }
        });
        let ToolInvocation::ImportCheckpoint { checkpoint } =
            parse_tool_invocation(&params).unwrap()
        else {
            panic!("expected importCheckpoint invocation");
        };
        assert_eq!(checkpoint["schemaVersion"], 1);
        assert_eq!(checkpoint["url"], "https://example.test/");
        assert!(
            parse_tool_invocation(&json!({
                "name": "importCheckpoint",
                "arguments": {"schemaVersion": 1}
            }))
            .is_err()
        );
    }

    #[test]
    fn session_snapshot_mutations_require_a_lease() {
        let inspect: JsonRpcRequest = serde_json::from_value(json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "tools/call",
            "params": {"name": "sessionSnapshot", "arguments": {"operation": "list"}}
        }))
        .unwrap();
        let purge: JsonRpcRequest = serde_json::from_value(json!({
            "jsonrpc": "2.0",
            "id": 2,
            "method": "tools/call",
            "params": {"name": "sessionSnapshot", "arguments": {"operation": "purge"}}
        }))
        .unwrap();
        assert!(!session_snapshot_requires_mutation_lease(&inspect));
        assert!(session_snapshot_requires_mutation_lease(&purge));
        assert_eq!(
            workflow_status_error_code("daemon workflow request id is already active"),
            -32600
        );
        assert_eq!(
            workflow_status_error_code("daemon active workflow limit reached"),
            -32000
        );
    }

    #[tokio::test]
    async fn preserves_content_length_framing() {
        let body = r#"{"jsonrpc":"2.0","id":1,"method":"ping"}"#;
        let (mut sender, receiver) = tokio::io::duplex(512);
        sender
            .write_all(format!("Content-Length: {}\r\n\r\n{body}", body.len()).as_bytes())
            .await
            .unwrap();
        sender.shutdown().await.unwrap();

        let mut reader = BufReader::new(receiver);
        let (decoded, format) = read_message(&mut reader).await.unwrap().unwrap();
        assert_eq!(decoded, body);
        assert_eq!(format, FrameFormat::ContentLength);

        let response = success_response(Some(json!(1)), json!({"ok": true}));
        let (mut sender, mut receiver) = tokio::io::duplex(512);
        write_response(&mut sender, &response, FrameFormat::ContentLength)
            .await
            .unwrap();
        sender.shutdown().await.unwrap();
        let mut encoded = Vec::new();
        receiver.read_to_end(&mut encoded).await.unwrap();
        let encoded = String::from_utf8(encoded).unwrap();

        assert!(encoded.starts_with("Content-Length: "));
        assert!(encoded.ends_with(r#"{"jsonrpc":"2.0","result":{"ok":true},"id":1}"#));
    }

    #[test]
    fn negotiates_legacy_and_elicitation_mcp_versions() {
        let policy = BrowserPolicy::development(std::env::current_dir().unwrap()).unwrap();
        for version in [MCP_PROTOCOL_VERSION, MCP_PROTOCOL_VERSION_ELICITATION] {
            let request: JsonRpcRequest = serde_json::from_value(json!({
                "jsonrpc":"2.0",
                "id":1,
                "method":"initialize",
                "params": {
                    "protocolVersion":version,
                    "capabilities":{"elicitation":{"form":{}}},
                    "clientInfo":{"name":"fixture","version":"1"}
                }
            }))
            .unwrap();
            let response = initialize_response(&request, &policy);
            assert!(response.error.is_none());
            assert_eq!(response.result.unwrap()["protocolVersion"], version);
        }

        let supports_form: JsonRpcRequest = serde_json::from_value(json!({
            "jsonrpc":"2.0","id":1,"method":"initialize",
            "params":{"protocolVersion":MCP_PROTOCOL_VERSION_ELICITATION,
                "capabilities":{"elicitation":{"form":{}}}}
        }))
        .unwrap();
        assert!(client_declares_form_elicitation(&supports_form));

        let legacy_form_alias: JsonRpcRequest = serde_json::from_value(json!({
            "jsonrpc":"2.0","id":1,"method":"initialize",
            "params":{"protocolVersion":MCP_PROTOCOL_VERSION_ELICITATION,
                "capabilities":{"elicitation":{}}}
        }))
        .unwrap();
        assert!(client_declares_form_elicitation(&legacy_form_alias));

        let url_only: JsonRpcRequest = serde_json::from_value(json!({
            "jsonrpc":"2.0","id":1,"method":"initialize",
            "params":{"protocolVersion":MCP_PROTOCOL_VERSION_ELICITATION,
                "capabilities":{"elicitation":{"url":{}}}}
        }))
        .unwrap();
        assert!(!client_declares_form_elicitation(&url_only));
    }

    #[tokio::test]
    async fn routes_mcp_server_request_responses_by_id() {
        tokio::task::LocalSet::new()
            .run_until(async {
                let (outbound, mut outbound_rx) = mpsc::channel(2);
                let pending_responses: PendingServerResponses =
                    Arc::new(StdMutex::new(HashMap::new()));
                let transport = McpServerTransport {
                    outbound,
                    pending_responses: Arc::clone(&pending_responses),
                    active_server_request_cancellations: Arc::new(StdMutex::new(HashMap::new())),
                    next_request_id: Arc::new(AtomicU64::new(1)),
                };
                let request = tokio::task::spawn_local(async move {
                    transport
                        .request(
                            "elicitation/create",
                            json!({"mode":"form","message":"answer"}),
                            FrameFormat::Newline,
                        )
                        .await
                });
                let outbound = outbound_rx.recv().await.unwrap();
                let OutboundPayload::Json(message) = outbound.payload else {
                    panic!("expected outbound server request");
                };
                assert_eq!(message["method"], "elicitation/create");
                let wrong_id = json!({
                    "jsonrpc":"2.0","id":"not-the-request","result":{"action":"cancel"}
                });
                assert!(!route_server_response(&wrong_id, &pending_responses));
                let response = json!({
                    "jsonrpc":"2.0","id":message["id"],
                    "result":{"action":"accept","content":{"value":"human"}}
                });
                assert!(route_server_response(&response, &pending_responses));
                assert_eq!(request.await.unwrap().unwrap(), response["result"]);

                let client_cancellations: CancellationMap = Arc::new(StdMutex::new(HashMap::new()));
                let server_cancellations: CancellationMap = Arc::new(StdMutex::new(HashMap::new()));
                let server_pending_responses: PendingServerResponses =
                    Arc::new(StdMutex::new(HashMap::new()));
                let (outbound, mut outbound_rx) = mpsc::channel(2);
                let transport = McpServerTransport {
                    outbound,
                    pending_responses: Arc::clone(&server_pending_responses),
                    active_server_request_cancellations: Arc::clone(&server_cancellations),
                    next_request_id: Arc::new(AtomicU64::new(2)),
                };
                let request = tokio::task::spawn_local(async move {
                    transport
                        .request(
                            "elicitation/create",
                            json!({"mode":"form","message":"cancel me"}),
                            FrameFormat::Newline,
                        )
                        .await
                });
                let outbound = outbound_rx.recv().await.unwrap();
                let OutboundPayload::Json(message) = outbound.payload else {
                    panic!("expected outbound server request");
                };
                let cancel: JsonRpcRequest = serde_json::from_value(json!({
                    "jsonrpc":"2.0","method":"notifications/cancelled",
                    "params":{"requestId":message["id"]}
                }))
                .unwrap();
                cancel_request(&cancel, &client_cancellations);
                assert!(!request.is_finished());
                let response = json!({
                    "jsonrpc":"2.0","id":message["id"],
                    "result":{"action":"accept"}
                });
                assert!(route_server_response(&response, &server_pending_responses));
                assert_eq!(request.await.unwrap().unwrap(), response["result"]);
                assert!(client_cancellations.lock().unwrap().is_empty());
                assert!(server_cancellations.lock().unwrap().is_empty());

                let (outbound, mut outbound_rx) = mpsc::channel(2);
                let transport = McpServerTransport {
                    outbound,
                    pending_responses: Arc::new(StdMutex::new(HashMap::new())),
                    active_server_request_cancellations: Arc::new(StdMutex::new(HashMap::new())),
                    next_request_id: Arc::new(AtomicU64::new(3)),
                };
                let request = tokio::task::spawn_local(async move {
                    transport
                        .request(
                            "elicitation/create",
                            json!({"mode":"form","message":"cancel me"}),
                            FrameFormat::Newline,
                        )
                        .await
                });
                let outbound = outbound_rx.recv().await.unwrap();
                let OutboundPayload::Json(message) = outbound.payload else {
                    panic!("expected outbound server request");
                };
                request.abort();
                assert!(request.await.unwrap_err().is_cancelled());
                let outbound = outbound_rx.recv().await.unwrap();
                let OutboundPayload::Json(cancellation) = outbound.payload else {
                    panic!("expected outbound cancellation notification");
                };
                assert_eq!(cancellation["method"], "notifications/cancelled");
                assert_eq!(cancellation["params"]["requestId"], message["id"]);
            })
            .await;
    }

    #[cfg(feature = "native-engine")]
    async fn simulated_mcp_dialog_response(
        pending: NativePendingDialog,
        response: Value,
    ) -> (Result<NativeDialogResolution, String>, Value) {
        let (outbound, mut outbound_rx) = mpsc::channel(2);
        let pending_responses: PendingServerResponses = Arc::new(StdMutex::new(HashMap::new()));
        let client = McpElicitationClient {
            transport: McpServerTransport {
                outbound,
                pending_responses: Arc::clone(&pending_responses),
                active_server_request_cancellations: Arc::new(StdMutex::new(HashMap::new())),
                next_request_id: Arc::new(AtomicU64::new(1)),
            },
            format: FrameFormat::Newline,
            parent_cancellation: Arc::new(Notify::new()),
            parent_cancelled: Arc::new(AtomicBool::new(false)),
            supports_form: true,
        };
        let dialog =
            tokio::task::spawn_local(async move { mcp_dialog_resolution(&client, &pending).await });
        let outbound = outbound_rx.recv().await.unwrap();
        let OutboundPayload::Json(message) = outbound.payload else {
            panic!("expected outbound elicitation request");
        };
        let response_message = json!({
            "jsonrpc":"2.0","id":message["id"],"result":response
        });
        assert!(route_server_response(&response_message, &pending_responses));
        (dialog.await.unwrap(), message)
    }

    #[cfg(feature = "native-engine")]
    fn test_pending_mcp_dialog(dialog_type: &str, message: &str) -> NativePendingDialog {
        NativePendingDialog {
            id: "dialog-1".into(),
            context_id: "context-1".into(),
            frame_id: "frame-1".into(),
            dialog: crate::browser::session::PendingDialog {
                dialog_type: dialog_type.into(),
                message: message.into(),
                default_value: (dialog_type == "prompt").then(|| "Ada".into()),
                url: "https://user:secret@example.test/".into(),
            },
        }
    }

    #[cfg(feature = "native-engine")]
    #[tokio::test]
    async fn mcp_elicitation_resolves_dialog_semantics_and_bounds_prompt_bytes() {
        tokio::task::LocalSet::new()
            .run_until(async {
                let prompt = test_pending_mcp_dialog(
                    "prompt",
                    "Name\u{001b}[31m https://example.test/path \u{202e}",
                );
                let value = format!("{}a", "€".repeat(85));
                let (resolution, request) = simulated_mcp_dialog_response(
                    prompt,
                    json!({"action":"accept","content":{"response":value}}),
                )
                .await;
                let resolution = resolution.unwrap();
                assert!(resolution.accepted);
                assert_eq!(resolution.prompt_value.as_deref().unwrap().len(), 256);
                assert_eq!(request["params"]["mode"], "form");
                assert_eq!(
                    request["params"]["requestedSchema"]["properties"]["response"]["maxLength"],
                    MCP_DIALOG_PROMPT_MAX_BYTES
                );
                assert!(
                    request["params"]["message"]
                        .as_str()
                        .unwrap()
                        .contains("\\u{001b}")
                );
                assert!(
                    request["params"]["message"]
                        .as_str()
                        .unwrap()
                        .contains("\\u{202e}")
                );
                assert!(
                    request["params"]["message"]
                        .as_str()
                        .unwrap()
                        .contains("Do not enter passwords, tokens")
                );
                assert!(
                    !request["params"]["message"]
                        .as_str()
                        .unwrap()
                        .contains("example.test")
                );
                assert!(request["params"].get("url").is_none());

                let too_long = "€".repeat(86);
                let (resolution, _) = simulated_mcp_dialog_response(
                    test_pending_mcp_dialog("prompt", "secret"),
                    json!({"action":"accept","content":{"response":too_long}}),
                )
                .await;
                assert!(resolution.unwrap_err().contains("256-byte UTF-8 limit"));

                let (resolution, _) = simulated_mcp_dialog_response(
                    test_pending_mcp_dialog("confirm", "Continue?"),
                    json!({"action":"accept","content":{"accepted":false}}),
                )
                .await;
                assert!(!resolution.unwrap().accepted);

                let (resolution, request) = simulated_mcp_dialog_response(
                    test_pending_mcp_dialog("beforeunload", "site-authored warning"),
                    json!({"action":"accept","content":{"accepted":true}}),
                )
                .await;
                assert!(resolution.unwrap().accepted);
                let message = request["params"]["message"].as_str().unwrap();
                assert!(message.contains(NATIVE_BEFOREUNLOAD_MESSAGE));
                assert!(message.contains("custom warning is not displayed"));
                assert!(!message.contains("site-authored warning"));
                assert_eq!(
                    request["params"]["requestedSchema"]["properties"]["accepted"]["default"],
                    false
                );
                assert_eq!(
                    request["params"]["requestedSchema"]["properties"]["accepted"]["title"],
                    "Leave this page"
                );

                let (resolution, request) = simulated_mcp_dialog_response(
                    test_pending_mcp_dialog("beforeunload", "another site-authored warning"),
                    json!({"action":"accept","content":{"accepted":false}}),
                )
                .await;
                assert!(!resolution.unwrap().accepted);
                assert!(
                    !request["params"]["message"]
                        .as_str()
                        .unwrap()
                        .contains("another site-authored warning")
                );

                let (resolution, _) = simulated_mcp_dialog_response(
                    test_pending_mcp_dialog("prompt", "Name"),
                    json!({"action":"cancel"}),
                )
                .await;
                let resolution = resolution.unwrap();
                assert!(!resolution.accepted);
                assert!(resolution.prompt_value.is_none());

                let (resolution, _) = simulated_mcp_dialog_response(
                    test_pending_mcp_dialog("alert", "Done"),
                    json!({"action":"decline"}),
                )
                .await;
                assert!(resolution.unwrap().accepted);
            })
            .await;
    }

    #[tokio::test]
    async fn rejects_oversized_and_malformed_frames_before_allocating_bodies() {
        let oversized = format!("Content-Length: {}\r\n\r\n", MAX_MESSAGE_BYTES + 1);
        let mut reader = BufReader::new(oversized.as_bytes());
        let error = read_message(&mut reader).await.unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::InvalidData);

        let malformed = b"Content-Length: 2\r\nnot-blank\r\n{}";
        let mut reader = BufReader::new(&malformed[..]);
        let error = read_message(&mut reader).await.unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::InvalidData);

        let mut long_line = vec![b'x'; MAX_HEADER_BYTES + 1];
        long_line.push(b'\n');
        let mut reader = BufReader::new(long_line.as_slice());
        let error = read_message(&mut reader).await.unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::InvalidData);

        let mut blank_preamble = b"\n".repeat(MAX_HEADER_BYTES + 1);
        blank_preamble.extend_from_slice(b"{}\n");
        let mut reader = BufReader::new(blank_preamble.as_slice());
        let error = read_message(&mut reader).await.unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::InvalidData);
    }

    #[tokio::test]
    async fn rejects_invalid_utf8_and_truncated_content_frames() {
        let invalid_utf8 = [0xff, b'\n'];
        let mut reader = BufReader::new(&invalid_utf8[..]);
        let error = read_message(&mut reader).await.unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::InvalidData);

        let truncated = b"Content-Length: 4\r\n\r\n{}";
        let mut reader = BufReader::new(&truncated[..]);
        let error = read_message(&mut reader).await.unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::UnexpectedEof);
    }

    #[tokio::test]
    async fn framing_regression_corpus_has_stable_outcomes() {
        let cases: Vec<FramingCorpusCase> =
            serde_json::from_str(include_str!("../../tests/fixtures/mcp_framing_corpus.json"))
                .unwrap();
        for case in cases {
            let mut reader = BufReader::new(case.bytes.as_slice());
            let result = read_message(&mut reader).await;
            assert_eq!(result.is_ok(), case.valid, "corpus case {}", case.name);
        }
    }

    #[tokio::test]
    async fn framing_property_sweep_handles_truncation_lengths_and_bytes() {
        let complete = b"Content-Length: 2\r\n\r\n{}";
        for end in 0..complete.len() {
            let mut reader = BufReader::new(&complete[..end]);
            let _ = read_message(&mut reader).await;
        }
        for digits in 1..=128 {
            let frame = format!("Content-Length: {}\r\n\r\n", "9".repeat(digits));
            let mut reader = BufReader::new(frame.as_bytes());
            assert!(read_message(&mut reader).await.is_err());
        }
        for byte in 0_u8..=u8::MAX {
            let line = [byte, b'\n'];
            let mut reader = BufReader::new(&line[..]);
            let _ = read_message(&mut reader).await;
        }
    }

    #[test]
    fn oversized_responses_become_small_protocol_errors() {
        let response = success_response(Some(json!(9)), json!({"value": "x".repeat(128)}));
        let encoded = encode_response(&response, 64).unwrap();
        let value: Value = serde_json::from_str(&encoded).unwrap();
        assert_eq!(value["id"], 9);
        assert_eq!(value["error"]["code"], -32001);
        assert!(!encoded.contains(&"x".repeat(128)));
    }

    #[tokio::test]
    async fn prompts_list_returns_all_agent_prompts() {
        let request: JsonRpcRequest = serde_json::from_value(json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "prompts/list"
        }))
        .unwrap();
        let mut session = None;
        let policy = BrowserPolicy::development(std::env::current_dir().unwrap()).unwrap();

        let response = handle_request(
            &request,
            &mut session,
            &SessionOptions::default(),
            &policy,
            None,
        )
        .await
        .unwrap();
        let result = response.result.unwrap();
        let prompts = result["prompts"].as_array().unwrap();
        assert_eq!(prompts.len(), 4);
        let names: Vec<&str> = prompts
            .iter()
            .map(|p| p["name"].as_str().unwrap())
            .collect();
        assert!(names.contains(&"glass-safe-navigation"));
        assert!(names.contains(&"glass-target-selection"));
        assert!(names.contains(&"glass-topology"));
        assert!(names.contains(&"glass-recovery"));
    }

    #[tokio::test]
    async fn prompts_get_returns_specific_prompt_content() {
        let request: JsonRpcRequest = serde_json::from_value(json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "prompts/get",
            "params": {"name": "glass-safe-navigation"}
        }))
        .unwrap();
        let mut session = None;
        let policy = BrowserPolicy::development(std::env::current_dir().unwrap()).unwrap();

        let response = handle_request(
            &request,
            &mut session,
            &SessionOptions::default(),
            &policy,
            None,
        )
        .await
        .unwrap();
        let result = response.result.unwrap();
        let messages = result["messages"].as_array().unwrap();
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0]["role"], "user");
        let text = messages[0]["content"]["text"].as_str().unwrap();
        assert!(text.contains("Glass Safe Navigation Loop"));
    }

    #[tokio::test]
    async fn prompts_get_rejects_missing_name_param() {
        let request: JsonRpcRequest = serde_json::from_value(json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "prompts/get",
            "params": {}
        }))
        .unwrap();
        let mut session = None;
        let policy = BrowserPolicy::development(std::env::current_dir().unwrap()).unwrap();

        let response = handle_request(
            &request,
            &mut session,
            &SessionOptions::default(),
            &policy,
            None,
        )
        .await
        .unwrap();
        assert!(response.error.is_some());
        assert_eq!(response.error.unwrap().code, -32602);
    }

    #[tokio::test]
    async fn resources_list_returns_all_contract_resources() {
        let request: JsonRpcRequest = serde_json::from_value(json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "resources/list"
        }))
        .unwrap();
        let mut session = None;
        let policy = BrowserPolicy::development(std::env::current_dir().unwrap()).unwrap();

        let response = handle_request(
            &request,
            &mut session,
            &SessionOptions::default(),
            &policy,
            None,
        )
        .await
        .unwrap();
        let result = response.result.unwrap();
        let resources = result["resources"].as_array().unwrap();
        assert_eq!(resources.len(), 6);
        let uris: Vec<&str> = resources
            .iter()
            .map(|r| r["uri"].as_str().unwrap())
            .collect();
        assert!(uris.contains(&"glass://contract/actions"));
        assert!(uris.contains(&"glass://contract/locators"));
        assert!(uris.contains(&"glass://contract/errors"));
        assert!(uris.contains(&"glass://contract/limits"));
        assert!(uris.contains(&"glass://contract/topology"));
    }

    #[tokio::test]
    async fn resources_read_returns_markdown_content() {
        let request: JsonRpcRequest = serde_json::from_value(json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "resources/read",
            "params": {"uri": "glass://contract/locators"}
        }))
        .unwrap();
        let mut session = None;
        let policy = BrowserPolicy::development(std::env::current_dir().unwrap()).unwrap();

        let response = handle_request(
            &request,
            &mut session,
            &SessionOptions::default(),
            &policy,
            None,
        )
        .await
        .unwrap();
        let result = response.result.unwrap();
        let contents = result["contents"].as_array().unwrap();
        assert_eq!(contents.len(), 1);
        assert_eq!(contents[0]["mimeType"], "text/markdown");
        let text = contents[0]["text"].as_str().unwrap();
        assert!(text.contains("Glass Locator Grammar"));
    }

    #[tokio::test]
    async fn resources_read_rejects_unknown_uri_with_32602() {
        let request: JsonRpcRequest = serde_json::from_value(json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "resources/read",
            "params": {"uri": "glass://nonexistent"}
        }))
        .unwrap();
        let mut session = None;
        let policy = BrowserPolicy::development(std::env::current_dir().unwrap()).unwrap();

        let response = handle_request(
            &request,
            &mut session,
            &SessionOptions::default(),
            &policy,
            None,
        )
        .await
        .unwrap();
        assert!(response.error.is_some());
        assert_eq!(response.error.unwrap().code, -32602);
    }

    #[tokio::test]
    async fn resources_read_rejects_missing_uri_param() {
        let request: JsonRpcRequest = serde_json::from_value(json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "resources/read",
            "params": {}
        }))
        .unwrap();
        let mut session = None;
        let policy = BrowserPolicy::development(std::env::current_dir().unwrap()).unwrap();

        let response = handle_request(
            &request,
            &mut session,
            &SessionOptions::default(),
            &policy,
            None,
        )
        .await
        .unwrap();
        assert!(response.error.is_some());
        assert_eq!(response.error.unwrap().code, -32602);
    }

    #[test]
    fn concurrent_request_limit_rejects_the_ninth_permit() {
        let semaphore = Arc::new(Semaphore::new(MAX_CONCURRENT_REQUESTS));
        let permits = (0..MAX_CONCURRENT_REQUESTS)
            .map(|_| Arc::clone(&semaphore).try_acquire_owned().unwrap())
            .collect::<Vec<_>>();
        assert!(Arc::clone(&semaphore).try_acquire_owned().is_err());
        drop(permits);
        assert!(Arc::clone(&semaphore).try_acquire_owned().is_ok());
    }
}
