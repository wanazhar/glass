//! MCP JSON-RPC 2.0 stdio server.
//!
//! Implements the Model Context Protocol (2024-11-05) over stdin/stdout,
//! providing browser automation tools with policy-gated execution, bounded
//! response sizes, and concurrent request handling.

use serde::{Deserialize, Deserializer, Serialize};
use serde_json::{Value, json};
use std::{
    borrow::Cow,
    collections::{BTreeMap, HashMap},
    io,
    path::Path,
    sync::{Arc, Mutex as StdMutex},
    time::Duration,
};
use tokio::io::{
    AsyncBufRead, AsyncBufReadExt, AsyncReadExt, AsyncWrite, AsyncWriteExt, BufReader,
};
use tokio::sync::{Mutex, Semaphore, mpsc, oneshot};
use tracing::{debug, info};

use crate::browser::cdp::CdpError;
use crate::browser::policy::{BrowserPolicy, PolicyError};
use crate::browser::profile::ProfileManager;
use crate::browser::session::{
    ActionContractError, ActionKind, ActionOutcome, ActionVerificationError, BatchMode, BatchStep,
    BrowserResult, BrowserSession, CheckpointV1, DownloadError, KnowledgeConfidence,
    KnowledgeLookupOptions, KnowledgeObservationMode, KnowledgeObservationReport,
    KnowledgeProfileScope, KnowledgeStore, Locator, PopupClickError, PreflightAction,
    ReconciliationOptions, SemanticIntentExecutionRequest, SemanticIntentRequest,
    SemanticObservationLevel, SessionOptions, SessionSnapshotStore, StructuredExtractionRequest,
    TargetError, VerificationPredicate, VisualCaptureOptions, VisualClip, VisualFormat,
    WaitCondition, WaitTimeout, default_knowledge_store_path, default_session_snapshot_path,
    recover_run,
};
use crate::capabilities::GlassCapabilityManifest;
use crate::cli::args::Cli;
use crate::daemon::{DaemonLeaseContext, LeaseError, MutationLeaseManager};
use crate::development::{
    Actor, DevelopmentError, ExperimentManager, HarnessRequest, LinkProvenance, ProcessState,
    ProjectWorkspace, ReconnectCapsule, ReconnectCapsuleStore, ResidentDevelopmentSessions,
    SemanticBreakpoint, SemanticSnapshot, VerificationCard, attention_inbox,
};
const MAX_PREFLIGHT_URL_BYTES: usize = 8 * 1024;
use crate::browser_backend::{BackendProfile, BrowserCapability};
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
const MAX_HEADER_BYTES: usize = 8 * 1024;
const MAX_MESSAGE_BYTES: usize = 4 * 1024 * 1024;
const MAX_RESPONSE_BYTES: usize = 32 * 1024 * 1024;
const MAX_ERROR_DETAILS_BYTES: usize = 16 * 1024;
const MAX_ERROR_MESSAGE_BYTES: usize = 512;
const MAX_CONCURRENT_REQUESTS: usize = 8;
const MAX_QUEUED_RESPONSES: usize = 16;
const FRAME_BODY_TIMEOUT: Duration = Duration::from_secs(10);

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

#[derive(Debug, PartialEq, Eq)]
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
    ProjectInspect {
        root: std::path::PathBuf,
    },
    ProjectFiles {
        root: std::path::PathBuf,
    },
    ProjectSearch {
        root: std::path::PathBuf,
        query: &'a str,
        limit: usize,
    },
    ProjectRead {
        root: std::path::PathBuf,
        path: &'a str,
    },
    ProjectEdit {
        root: std::path::PathBuf,
        path: &'a str,
        content: &'a str,
    },
    ProjectMkdir {
        root: std::path::PathBuf,
        path: &'a str,
    },
    ProjectRename {
        root: std::path::PathBuf,
        from: &'a str,
        to: &'a str,
    },
    ProjectDelete {
        root: std::path::PathBuf,
        path: &'a str,
        confirmed: bool,
    },
    ProjectDiagnostics {
        root: std::path::PathBuf,
        path: &'a str,
    },
    ProjectRun {
        root: std::path::PathBuf,
        name: &'a str,
        command: &'a str,
        wait: bool,
    },
    ProjectProcessList {
        root: std::path::PathBuf,
    },
    ProjectProcessStop {
        root: std::path::PathBuf,
        name: &'a str,
    },
    ProjectProcessOutput {
        root: std::path::PathBuf,
        name: &'a str,
    },
    ProjectDiff {
        root: std::path::PathBuf,
    },
    ProjectTimeline {
        root: std::path::PathBuf,
    },
    ProjectEvents {
        root: std::path::PathBuf,
        after_id: Option<&'a str>,
        limit: usize,
    },
    ProjectSessionStatus {
        root: std::path::PathBuf,
    },
    ProjectSessionDetach {
        root: std::path::PathBuf,
        confirmed: bool,
    },
    ProjectCapsuleSave {
        root: std::path::PathBuf,
        event_cursor: Option<&'a str>,
        mobile_view: Option<&'a str>,
        browser_target_id: Option<&'a str>,
        browser_revision: Option<u64>,
        pending_attention: Option<&'a str>,
        live_mode: Option<&'a str>,
        live_quality: Option<&'a str>,
    },
    ProjectCapsuleShow {
        root: std::path::PathBuf,
    },
    ProjectCapsuleClear {
        root: std::path::PathBuf,
        confirmed: bool,
    },
    ProjectInbox {
        root: std::path::PathBuf,
    },
    ProjectVerificationCard {
        root: std::path::PathBuf,
        title: &'a str,
        semantic_revision: Option<u64>,
    },
    ProjectReplay {
        root: std::path::PathBuf,
        start: usize,
        limit: usize,
    },
    ProjectGraph {
        root: std::path::PathBuf,
        operation: &'a str,
        entity: Option<&'a str>,
        path: Option<&'a str>,
        line: Option<u32>,
    },
    ProjectBreakpoint {
        root: std::path::PathBuf,
        kind: &'a str,
        entity: &'a str,
        before: &'a Value,
        after: &'a Value,
    },
    ProjectNeovimProbe,
    ProjectExperimentCreate {
        root: std::path::PathBuf,
        name: &'a str,
        port: u16,
    },
    ProjectAttach {
        root: std::path::PathBuf,
        actor: &'a str,
    },
    ProjectLink {
        root: std::path::PathBuf,
        entity: &'a str,
        path: &'a str,
        start_line: u32,
        end_line: u32,
        provenance: &'a str,
        confidence: f32,
        detail: &'a str,
    },
    AgentHello {
        root: std::path::PathBuf,
    },
    AgentPrompt {
        root: std::path::PathBuf,
        text: &'a str,
    },
    AgentSteer {
        root: std::path::PathBuf,
        text: &'a str,
    },
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
    response: JsonRpcResponse,
    format: FrameFormat,
}

type CancellationMap = Arc<StdMutex<HashMap<String, oneshot::Sender<()>>>>;
pub type DevelopmentSessionStore = Arc<StdMutex<ResidentDevelopmentSessions>>;

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

async fn run_mcp_server_local(cli: &Cli) -> BrowserResult<()> {
    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();
    run_mcp_stream(
        BufReader::new(stdin),
        stdout,
        cli,
        Arc::new(Mutex::new(None)),
        true,
        false,
        None,
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
    session: Arc<Mutex<Option<BrowserSession>>>,
    close_session_on_eof: bool,
    local_daemon: bool,
    lease_context: Option<Arc<DaemonLeaseContext>>,
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
    let development_sessions = lease_context
        .as_ref()
        .map(|context| Arc::clone(&context.development_sessions))
        .unwrap_or_else(|| Arc::new(StdMutex::new(ResidentDevelopmentSessions::default())));
    let mut reader = reader;
    let cancellations: CancellationMap = Arc::new(StdMutex::new(HashMap::new()));
    let permits = lease_context
        .as_ref()
        .map(|context| Arc::clone(&context.request_permits))
        .unwrap_or_else(|| Arc::new(Semaphore::new(MAX_CONCURRENT_REQUESTS)));
    let client_permits = lease_context
        .as_ref()
        .map(|context| Arc::clone(&context.client_request_permits));
    let (outbound_tx, mut outbound_rx) = mpsc::channel::<Outbound>(MAX_QUEUED_RESPONSES);
    let writer = tokio::task::spawn_local(async move {
        let mut writer = writer;
        while let Some(outbound) = outbound_rx.recv().await {
            write_response(&mut writer, &outbound.response, outbound.format).await?;
        }
        Ok::<(), io::Error>(())
    });
    let mut lifecycle = Lifecycle::Uninitialized;

    while let Some((body, format)) = read_message(&mut reader).await? {
        let body_bytes = body.len();
        let request: JsonRpcRequest = match serde_json::from_str(&body) {
            Ok(request) => request,
            Err(error) => {
                debug!(body_bytes, "MCP request rejected: invalid JSON");
                let response =
                    error_response(Some(Value::Null), -32700, format!("parse error: {error}"));
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

        if request.method == "tools/call"
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
        let cancellation_key = request.id.cancellation_key();
        let (cancel_tx, cancel_rx) = oneshot::channel();
        let mut cancel_guard = Some(cancel_tx);
        if let Some(key) = cancellation_key.as_ref() {
            let duplicate = {
                let mut active = cancellations.lock().expect("cancellation map poisoned");
                if active.contains_key(key) {
                    true
                } else {
                    active.insert(key.clone(), cancel_guard.take().expect("sender available"));
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
            if let Some(key) = cancellation_key.as_ref() {
                task_cancellations_remove(&cancellations, key);
            }
            return Err(error);
        }
        let task_session = Arc::clone(&session);
        let task_options = options.clone();
        let task_policy = policy.clone();
        let task_viewport = viewport;
        let task_knowledge_store = cli.knowledge_store.clone();
        let task_development_sessions = Arc::clone(&development_sessions);
        let task_outbound = outbound_tx.clone();
        let task_cancellations = Arc::clone(&cancellations);
        tokio::task::spawn_local(async move {
            let _permit = permit;
            let _client_permit = client_permit;
            let _cancel_guard = cancel_guard;
            let is_notification = request.id.is_notification();
            let id = request.id.response_value();
            let operation = async {
                let mut session = task_session.lock().await;
                handle_request_with_viewport(
                    &request,
                    &mut session,
                    &task_options,
                    &task_policy,
                    task_viewport,
                    task_knowledge_store.as_deref(),
                    &task_development_sessions,
                )
                .await
            };
            let mut response = tokio::select! {
                response = operation => response,
                _ = cancel_rx => Some(error_response(id, -32800, "request cancelled")),
            };
            if is_notification {
                response = None;
            }
            if let Some(key) = cancellation_key {
                task_cancellations
                    .lock()
                    .expect("cancellation map poisoned")
                    .remove(&key);
            }
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

    // EOF is a graceful client shutdown: allow already accepted requests to
    // finish and flush their responses before closing the owned browser.
    drop(outbound_tx);
    writer.await??;
    if close_session_on_eof {
        let mut session = session.lock().await;
        if let Some(session) = session.take() {
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

fn task_cancellations_remove(cancellations: &CancellationMap, key: &str) {
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

fn tool_requires_mutation_lease(tool_name: &str) -> bool {
    !matches!(
        tool_name,
        "screenshot"
            | "observe"
            | "observeBootstrap"
            | "observeKnowledge"
            | "inspectPage"
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
        .send(Outbound { response, format })
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

fn cancel_request(request: &JsonRpcRequest, cancellations: &CancellationMap) {
    let Some(key) = request.params.get("requestId").and_then(request_id_key) else {
        return;
    };
    if let Some(cancellation) = cancellations
        .lock()
        .expect("cancellation map poisoned")
        .remove(&key)
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
    if version != MCP_PROTOCOL_VERSION {
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
            "protocolVersion": MCP_PROTOCOL_VERSION,
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

#[cfg(test)]
async fn handle_request(
    request: &JsonRpcRequest,
    session: &mut Option<BrowserSession>,
    options: &SessionOptions,
    policy: &BrowserPolicy,
    knowledge_store_path: Option<&Path>,
) -> Option<JsonRpcResponse> {
    let development_sessions = Arc::new(StdMutex::new(ResidentDevelopmentSessions::default()));
    handle_request_with_viewport(
        request,
        session,
        options,
        policy,
        None,
        knowledge_store_path,
        &development_sessions,
    )
    .await
}

async fn handle_request_with_viewport(
    request: &JsonRpcRequest,
    session: &mut Option<BrowserSession>,
    options: &SessionOptions,
    policy: &BrowserPolicy,
    viewport: Option<(i64, i64)>,
    knowledge_store_path: Option<&Path>,
    development_sessions: &DevelopmentSessionStore,
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
        "tools/list" => success_response(request.id.response_value(), json!({"tools": tools()})),
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
        "tools/call" => match call_tool(
            request,
            session,
            options,
            policy,
            viewport,
            knowledge_store_path,
            development_sessions,
        )
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

async fn call_tool(
    request: &JsonRpcRequest,
    session: &mut Option<BrowserSession>,
    options: &SessionOptions,
    policy: &BrowserPolicy,
    viewport: Option<(i64, i64)>,
    knowledge_store_path: Option<&Path>,
    development_sessions: &DevelopmentSessionStore,
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
    ) {
        policy.require(crate::browser::policy::PolicyCapability::PersistentProfile)?;
        return call_knowledge_tool(invocation, options, knowledge_store_path);
    }
    if matches!(
        &invocation,
        ToolInvocation::ResolveIntentWithKnowledge { .. }
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
    if matches!(
        &invocation,
        ToolInvocation::ProjectInspect { .. }
            | ToolInvocation::ProjectFiles { .. }
            | ToolInvocation::ProjectSearch { .. }
            | ToolInvocation::ProjectRead { .. }
            | ToolInvocation::ProjectEdit { .. }
            | ToolInvocation::ProjectMkdir { .. }
            | ToolInvocation::ProjectRename { .. }
            | ToolInvocation::ProjectDelete { .. }
            | ToolInvocation::ProjectDiagnostics { .. }
            | ToolInvocation::ProjectRun { .. }
            | ToolInvocation::ProjectProcessList { .. }
            | ToolInvocation::ProjectProcessStop { .. }
            | ToolInvocation::ProjectProcessOutput { .. }
            | ToolInvocation::ProjectDiff { .. }
            | ToolInvocation::ProjectTimeline { .. }
            | ToolInvocation::ProjectEvents { .. }
            | ToolInvocation::ProjectSessionStatus { .. }
            | ToolInvocation::ProjectSessionDetach { .. }
            | ToolInvocation::ProjectCapsuleSave { .. }
            | ToolInvocation::ProjectCapsuleShow { .. }
            | ToolInvocation::ProjectCapsuleClear { .. }
            | ToolInvocation::ProjectInbox { .. }
            | ToolInvocation::ProjectVerificationCard { .. }
            | ToolInvocation::ProjectReplay { .. }
            | ToolInvocation::ProjectGraph { .. }
            | ToolInvocation::ProjectBreakpoint { .. }
            | ToolInvocation::ProjectNeovimProbe
            | ToolInvocation::ProjectExperimentCreate { .. }
            | ToolInvocation::ProjectAttach { .. }
            | ToolInvocation::ProjectLink { .. }
            | ToolInvocation::AgentHello { .. }
            | ToolInvocation::AgentPrompt { .. }
            | ToolInvocation::AgentSteer { .. }
    ) {
        if matches!(
            &invocation,
            ToolInvocation::ProjectTimeline { .. }
                | ToolInvocation::ProjectEvents { .. }
                | ToolInvocation::ProjectCapsuleShow { .. }
                | ToolInvocation::ProjectInbox { .. }
        ) {
            return call_persisted_development_read(invocation);
        }
        let mut sessions = development_sessions
            .lock()
            .map_err(|_| "development session registry poisoned")?;
        return call_development_tool(invocation, &mut sessions);
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
        ToolInvocation::Hover { target } => action_result(session.hover(target.as_ref()).await?),
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
            let opts: crate::browser::session::PdfOptions =
                serde_json::from_value(options).unwrap_or_default();
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
        ToolInvocation::ProjectInspect { .. }
        | ToolInvocation::ProjectFiles { .. }
        | ToolInvocation::ProjectSearch { .. }
        | ToolInvocation::ProjectRead { .. }
        | ToolInvocation::ProjectEdit { .. }
        | ToolInvocation::ProjectMkdir { .. }
        | ToolInvocation::ProjectRename { .. }
        | ToolInvocation::ProjectDelete { .. }
        | ToolInvocation::ProjectDiagnostics { .. }
        | ToolInvocation::ProjectRun { .. }
        | ToolInvocation::ProjectProcessList { .. }
        | ToolInvocation::ProjectProcessStop { .. }
        | ToolInvocation::ProjectProcessOutput { .. }
        | ToolInvocation::ProjectDiff { .. }
        | ToolInvocation::ProjectTimeline { .. }
        | ToolInvocation::ProjectEvents { .. }
        | ToolInvocation::ProjectSessionStatus { .. }
        | ToolInvocation::ProjectSessionDetach { .. }
        | ToolInvocation::ProjectCapsuleSave { .. }
        | ToolInvocation::ProjectCapsuleShow { .. }
        | ToolInvocation::ProjectCapsuleClear { .. }
        | ToolInvocation::ProjectInbox { .. }
        | ToolInvocation::ProjectVerificationCard { .. }
        | ToolInvocation::ProjectReplay { .. }
        | ToolInvocation::ProjectGraph { .. }
        | ToolInvocation::ProjectBreakpoint { .. }
        | ToolInvocation::ProjectNeovimProbe
        | ToolInvocation::ProjectExperimentCreate { .. }
        | ToolInvocation::ProjectAttach { .. }
        | ToolInvocation::ProjectLink { .. }
        | ToolInvocation::AgentHello { .. }
        | ToolInvocation::AgentPrompt { .. }
        | ToolInvocation::AgentSteer { .. } => {
            unreachable!("development tools are handled before browser session startup")
        }
    }
}

fn call_development_tool(
    invocation: ToolInvocation<'_>,
    sessions: &mut ResidentDevelopmentSessions,
) -> BrowserResult<Value> {
    match invocation {
        ToolInvocation::ProjectInspect { root } => {
            Ok(sessions.with_workspace(root, |workspace| {
                Ok(json!({
                    "schemaVersion": crate::development::DEVELOPMENT_SCHEMA_VERSION,
                    "root": workspace.root(),
                    "detection": workspace.detection(),
                    "config": workspace.config(),
                    "revision": workspace.revision(),
                }))
            })?)
        }
        ToolInvocation::ProjectFiles { root } => Ok(sessions
            .with_workspace(root, |workspace| {
                Ok(serde_json::to_value(workspace.list_files()?)?)
            })?),
        ToolInvocation::ProjectSearch { root, query, limit } => Ok(sessions
            .with_workspace(root, |workspace| {
                Ok(serde_json::to_value(workspace.search(query, limit)?)?)
            })?),
        ToolInvocation::ProjectRead { root, path } => Ok(sessions
            .with_workspace(root, |workspace| {
                Ok(json!({"path": path, "content": workspace.read_file(path)?}))
            })?),
        ToolInvocation::ProjectEdit {
            root,
            path,
            content,
        } => Ok(sessions.with_workspace(root, |workspace| {
            workspace.edit_buffer(path, content.into(), Actor::external("mcp"))?;
            Ok(serde_json::to_value(workspace.save_buffer(path)?)?)
        })?),
        ToolInvocation::ProjectMkdir { root, path } => {
            Ok(sessions.with_workspace(root, |workspace| {
                workspace.create_directory(path, Actor::external("mcp"))?;
                Ok(json!({"path": path, "created": true}))
            })?)
        }
        ToolInvocation::ProjectRename { root, from, to } => {
            Ok(sessions.with_workspace(root, |workspace| {
                workspace.rename_path(from, to, Actor::external("mcp"))?;
                Ok(json!({"from": from, "to": to, "renamed": true}))
            })?)
        }
        ToolInvocation::ProjectDelete {
            root,
            path,
            confirmed,
        } => {
            if !confirmed {
                return Err("project.delete requires confirmed=true".into());
            }
            Ok(sessions.with_workspace(root, |workspace| {
                workspace.delete_path(path, Actor::external("mcp"))?;
                Ok(json!({"path": path, "deleted": true}))
            })?)
        }
        ToolInvocation::ProjectDiagnostics { root, path } => {
            Ok(sessions.with_workspace(root, |workspace| {
                Ok(serde_json::to_value(
                    workspace.publish_rust_diagnostics(path)?,
                )?)
            })?)
        }
        ToolInvocation::ProjectRun {
            root,
            name,
            command,
            wait,
        } => Ok(sessions.with_workspace(root, |workspace| {
            workspace.start_process(name, command)?;
            let snapshot = if wait {
                wait_for_development_process(workspace, name)
                    .map_err(|error| DevelopmentError::Process(error.to_string()))?
            } else {
                workspace
                    .processes()
                    .list()
                    .into_iter()
                    .find(|process| process.name == name)
                    .ok_or_else(|| DevelopmentError::NotFound(format!("process {name}")))?
            };
            Ok(serde_json::to_value(snapshot)?)
        })?),
        ToolInvocation::ProjectProcessList { root } => Ok(sessions
            .with_workspace(root, |workspace| {
                Ok(serde_json::to_value(workspace.processes().list())?)
            })?),
        ToolInvocation::ProjectProcessStop { root, name } => Ok(sessions
            .with_workspace(root, |workspace| {
                Ok(serde_json::to_value(workspace.stop_process(name)?)?)
            })?),
        ToolInvocation::ProjectProcessOutput { root, name } => Ok(sessions
            .with_workspace(root, |workspace| {
                Ok(json!({"name": name, "output": workspace.processes().output(name)?}))
            })?),
        ToolInvocation::ProjectDiff { root } => Ok(sessions.with_workspace(root, |workspace| {
            Ok(serde_json::to_value(workspace.diff()?)?)
        })?),
        ToolInvocation::ProjectTimeline { root } => Ok(serde_json::to_value(
            ProjectWorkspace::timeline_snapshot(root)?,
        )?),
        ToolInvocation::ProjectEvents {
            root,
            after_id,
            limit,
        } => Ok(serde_json::to_value(ProjectWorkspace::event_page(
            root, after_id, limit,
        )?)?),
        ToolInvocation::ProjectSessionStatus { root } => Ok(json!({
            "root": std::fs::canonicalize(&root)?,
            "resident": sessions.contains(&root),
            "residentSessionCount": sessions.len(),
            "capacity": sessions.capacity(),
        })),
        ToolInvocation::ProjectSessionDetach { root, confirmed } => {
            if !confirmed {
                return Err("project.session.detach requires confirmed=true".into());
            }
            Ok(json!({"root": std::fs::canonicalize(&root)?, "detached": sessions.detach(root)?}))
        }
        ToolInvocation::ProjectCapsuleSave {
            root,
            event_cursor,
            mobile_view,
            browser_target_id,
            browser_revision,
            pending_attention,
            live_mode,
            live_quality,
        } => {
            let mut capsule = ReconnectCapsule::new(root)?;
            capsule.event_cursor = event_cursor.map(str::to_string);
            capsule.mobile_view = mobile_view.map(str::to_string);
            capsule.browser_target_id = browser_target_id.map(str::to_string);
            capsule.browser_revision = browser_revision;
            capsule.pending_attention = pending_attention.map(str::to_string);
            capsule.live_mode = live_mode.map(str::to_string);
            capsule.live_quality = live_quality.map(str::to_string);
            capsule.saved_at_ms = current_time_ms();
            let path = ReconnectCapsuleStore::save(&capsule)?;
            Ok(json!({"capsule": capsule, "path": path}))
        }
        ToolInvocation::ProjectCapsuleShow { root } => Ok(json!({
            "capsule": ReconnectCapsuleStore::load(root)?,
        })),
        ToolInvocation::ProjectCapsuleClear { root, confirmed } => {
            if !confirmed {
                return Err("project.capsule.clear requires confirmed=true".into());
            }
            Ok(json!({"cleared": ReconnectCapsuleStore::clear(root)?}))
        }
        ToolInvocation::ProjectInbox { root } => {
            let events = ProjectWorkspace::timeline_snapshot(root)?;
            Ok(serde_json::to_value(attention_inbox(events.into_iter()))?)
        }
        ToolInvocation::ProjectVerificationCard {
            root,
            title,
            semantic_revision,
        } => Ok(sessions.with_workspace(root, |workspace| {
            let diff = workspace.diff()?;
            Ok(serde_json::to_value(VerificationCard::from_diff(
                title,
                &diff,
                semantic_revision,
            )?)?)
        })?),
        ToolInvocation::ProjectReplay { root, start, limit } => Ok(sessions
            .with_workspace(root, |workspace| {
                Ok(serde_json::to_value(workspace.replay(start, limit)?)?)
            })?),
        ToolInvocation::ProjectGraph {
            root,
            operation,
            entity,
            path,
            line,
        } => Ok(sessions.with_workspace(root, |workspace| match operation {
            "discover" => Ok(serde_json::to_value(workspace.discover_runtime_links()?)?),
            "entity" => Ok(serde_json::to_value(workspace.graph().links_for(
                entity.ok_or_else(|| {
                    DevelopmentError::InvalidInput(
                        "project.graph entity operation requires entity".into(),
                    )
                })?,
            ))?),
            "source" => Ok(serde_json::to_value(
                workspace.graph().entities_for_source(
                    path.ok_or_else(|| {
                        DevelopmentError::InvalidInput(
                            "project.graph source operation requires path".into(),
                        )
                    })?,
                    line,
                ),
            )?),
            _ => Err(DevelopmentError::InvalidInput(
                "project.graph operation must be discover, entity, or source".into(),
            )),
        })?),
        ToolInvocation::ProjectBreakpoint {
            root,
            kind,
            entity,
            before,
            after,
        } => {
            let before: SemanticSnapshot = serde_json::from_value(before.clone())?;
            let after: SemanticSnapshot = serde_json::from_value(after.clone())?;
            let breakpoint = match kind {
                "disappears" => SemanticBreakpoint::EntityDisappears { entity_id: entity.into() },
                "name-missing" => SemanticBreakpoint::AccessibleNameMissing { entity_id: Some(entity.into()) },
                "role-changes" => SemanticBreakpoint::RoleChanges { entity_id: entity.into() },
                "actionability-lost" => SemanticBreakpoint::ActionabilityLost { entity_id: entity.into() },
                _ => return Err("breakpoint kind must be disappears, name-missing, role-changes, or actionability-lost".into()),
            };
            Ok(sessions.with_workspace(root, |workspace| {
                workspace.discover_runtime_links()?;
                Ok(serde_json::to_value(
                    workspace.evaluate_semantic_breakpoints(&[breakpoint], &before, &after)?,
                )?)
            })?)
        }
        ToolInvocation::ProjectNeovimProbe => {
            Ok(serde_json::to_value(crate::development::probe_neovim()?)?)
        }
        ToolInvocation::ProjectExperimentCreate { root, name, port } => Ok(serde_json::to_value(
            ExperimentManager::new(&root)?.create(name, port)?,
        )?),
        ToolInvocation::ProjectAttach { root, actor } => {
            let actor = Actor::external(actor);
            Ok(sessions.with_workspace(root, |workspace| {
                workspace.attach_actor(actor.clone())?;
                Ok(serde_json::to_value(actor)?)
            })?)
        }
        ToolInvocation::ProjectLink {
            root,
            entity,
            path,
            start_line,
            end_line,
            provenance,
            confidence,
            detail,
        } => {
            let provenance = parse_development_provenance(provenance)?;
            Ok(sessions.with_workspace(root, |workspace| {
                Ok(serde_json::to_value(workspace.link_runtime_source(
                    entity,
                    path,
                    start_line,
                    end_line,
                    provenance,
                    detail,
                    confidence,
                    Actor::external("mcp"),
                )?)?)
            })?)
        }
        ToolInvocation::AgentHello { root } => {
            Ok(sessions.with_runtime(root, |workspace, harness| {
                Ok(serde_json::to_value(
                    harness.handle(workspace, HarnessRequest::Hello)?,
                )?)
            })?)
        }
        ToolInvocation::AgentPrompt { root, text } => {
            Ok(sessions.with_runtime(root, |workspace, harness| {
                Ok(serde_json::to_value(harness.handle(
                    workspace,
                    HarnessRequest::Prompt { text: text.into() },
                )?)?)
            })?)
        }
        ToolInvocation::AgentSteer { root, text } => {
            Ok(sessions.with_runtime(root, |workspace, harness| {
                Ok(serde_json::to_value(harness.handle(
                    workspace,
                    HarnessRequest::Steer { text: text.into() },
                )?)?)
            })?)
        }
        _ => Err("non-development tool passed to development dispatcher".into()),
    }
}

fn call_persisted_development_read(invocation: ToolInvocation<'_>) -> BrowserResult<Value> {
    match invocation {
        ToolInvocation::ProjectTimeline { root } => Ok(serde_json::to_value(
            ProjectWorkspace::timeline_snapshot(root)?,
        )?),
        ToolInvocation::ProjectEvents {
            root,
            after_id,
            limit,
        } => Ok(serde_json::to_value(ProjectWorkspace::event_page(
            root, after_id, limit,
        )?)?),
        ToolInvocation::ProjectCapsuleShow { root } => Ok(json!({
            "capsule": ReconnectCapsuleStore::load(root)?,
        })),
        ToolInvocation::ProjectInbox { root } => {
            let events = ProjectWorkspace::timeline_snapshot(root)?;
            Ok(serde_json::to_value(attention_inbox(events.into_iter()))?)
        }
        _ => Err("stateful development tool passed to persisted read dispatcher".into()),
    }
}

fn wait_for_development_process(
    workspace: &mut ProjectWorkspace,
    name: &str,
) -> BrowserResult<crate::development::ProcessSnapshot> {
    loop {
        let snapshot = workspace
            .processes()
            .poll()?
            .into_iter()
            .find(|process| process.name == name)
            .ok_or_else(|| format!("process {name} disappeared"))?;
        if !matches!(snapshot.state, ProcessState::Running) {
            return Ok(snapshot);
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}

fn parse_development_provenance(value: &str) -> BrowserResult<LinkProvenance> {
    match value {
        "explicit-marker" => Ok(LinkProvenance::ExplicitMarker),
        "runtime-observation" => Ok(LinkProvenance::RuntimeObservation),
        "static-analysis" => Ok(LinkProvenance::StaticAnalysis),
        "inferred" => Ok(LinkProvenance::Inferred),
        _ => Err(
            "provenance must be explicit-marker, runtime-observation, static-analysis, or inferred"
                .into(),
        ),
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
        "project.inspect" => Ok(ToolInvocation::ProjectInspect {
            root: development_root(arguments)?,
        }),
        "project.files" => Ok(ToolInvocation::ProjectFiles {
            root: development_root(arguments)?,
        }),
        "project.search" => Ok(ToolInvocation::ProjectSearch {
            root: development_root(arguments)?,
            query: required_string(arguments, "query")?,
            limit: optional_u64(arguments, "limit", 64)? as usize,
        }),
        "project.read" => Ok(ToolInvocation::ProjectRead {
            root: development_root(arguments)?,
            path: required_string(arguments, "path")?,
        }),
        "project.edit" => Ok(ToolInvocation::ProjectEdit {
            root: development_root(arguments)?,
            path: required_string(arguments, "path")?,
            content: required_string(arguments, "content")?,
        }),
        "project.mkdir" => Ok(ToolInvocation::ProjectMkdir {
            root: development_root(arguments)?,
            path: required_string(arguments, "path")?,
        }),
        "project.rename" => Ok(ToolInvocation::ProjectRename {
            root: development_root(arguments)?,
            from: required_string(arguments, "from")?,
            to: required_string(arguments, "to")?,
        }),
        "project.delete" => Ok(ToolInvocation::ProjectDelete {
            root: development_root(arguments)?,
            path: required_string(arguments, "path")?,
            confirmed: optional_bool(arguments, "confirmed")?,
        }),
        "project.diagnostics" => Ok(ToolInvocation::ProjectDiagnostics {
            root: development_root(arguments)?,
            path: required_string(arguments, "path")?,
        }),
        "project.run" => Ok(ToolInvocation::ProjectRun {
            root: development_root(arguments)?,
            name: required_string(arguments, "name")?,
            command: required_string(arguments, "command")?,
            wait: optional_bool(arguments, "wait")?,
        }),
        "project.processes" => Ok(ToolInvocation::ProjectProcessList {
            root: development_root(arguments)?,
        }),
        "project.process.stop" => Ok(ToolInvocation::ProjectProcessStop {
            root: development_root(arguments)?,
            name: required_string(arguments, "name")?,
        }),
        "project.process.output" => Ok(ToolInvocation::ProjectProcessOutput {
            root: development_root(arguments)?,
            name: required_string(arguments, "name")?,
        }),
        "project.diff" => Ok(ToolInvocation::ProjectDiff {
            root: development_root(arguments)?,
        }),
        "project.timeline" => Ok(ToolInvocation::ProjectTimeline {
            root: development_root(arguments)?,
        }),
        "project.events" => {
            let after_id = optional_string(arguments, "afterId")?;
            if after_id.is_some_and(|value| value.len() > 128) {
                return Err("afterId must be at most 128 bytes".into());
            }
            let limit = optional_u64(arguments, "limit", 64)?;
            if !(1..=256).contains(&limit) {
                return Err("limit must be between 1 and 256".into());
            }
            Ok(ToolInvocation::ProjectEvents {
                root: development_root(arguments)?,
                after_id,
                limit: limit as usize,
            })
        }
        "project.session.status" => Ok(ToolInvocation::ProjectSessionStatus {
            root: development_root(arguments)?,
        }),
        "project.session.detach" => Ok(ToolInvocation::ProjectSessionDetach {
            root: development_root(arguments)?,
            confirmed: optional_bool(arguments, "confirmed")?,
        }),
        "project.capsule.save" => Ok(ToolInvocation::ProjectCapsuleSave {
            root: development_root(arguments)?,
            event_cursor: optional_string(arguments, "eventCursor")?,
            mobile_view: optional_string(arguments, "mobileView")?,
            browser_target_id: optional_string(arguments, "browserTargetId")?,
            browser_revision: optional_u64_value(arguments, "browserRevision")?,
            pending_attention: optional_string(arguments, "pendingAttention")?,
            live_mode: optional_string(arguments, "liveMode")?,
            live_quality: optional_string(arguments, "liveQuality")?,
        }),
        "project.capsule.show" => Ok(ToolInvocation::ProjectCapsuleShow {
            root: development_root(arguments)?,
        }),
        "project.capsule.clear" => Ok(ToolInvocation::ProjectCapsuleClear {
            root: development_root(arguments)?,
            confirmed: optional_bool(arguments, "confirmed")?,
        }),
        "project.inbox" => Ok(ToolInvocation::ProjectInbox {
            root: development_root(arguments)?,
        }),
        "project.verification.card" => Ok(ToolInvocation::ProjectVerificationCard {
            root: development_root(arguments)?,
            title: required_string(arguments, "title")?,
            semantic_revision: optional_u64_value(arguments, "semanticRevision")?,
        }),
        "project.replay" => Ok(ToolInvocation::ProjectReplay {
            root: development_root(arguments)?,
            start: optional_u64(arguments, "start", 0)? as usize,
            limit: optional_u64(arguments, "limit", 64)? as usize,
        }),
        "project.graph" => Ok(ToolInvocation::ProjectGraph {
            root: development_root(arguments)?,
            operation: required_string(arguments, "operation")?,
            entity: arguments.get("entity").and_then(Value::as_str),
            path: arguments.get("path").and_then(Value::as_str),
            line: optional_u64_value(arguments, "line")?.map(|line| line as u32),
        }),
        "project.breakpoint" => Ok(ToolInvocation::ProjectBreakpoint {
            root: development_root(arguments)?,
            kind: required_string(arguments, "kind")?,
            entity: required_string(arguments, "entity")?,
            before: arguments
                .get("before")
                .ok_or("project.breakpoint requires before")?,
            after: arguments
                .get("after")
                .ok_or("project.breakpoint requires after")?,
        }),
        "project.neovim.probe" => Ok(ToolInvocation::ProjectNeovimProbe),
        "project.experiment.create" => Ok(ToolInvocation::ProjectExperimentCreate {
            root: development_root(arguments)?,
            name: required_string(arguments, "name")?,
            port: required_u32(arguments, "port")?
                .try_into()
                .map_err(|_| "port must be <= 65535")?,
        }),
        "project.attach" => Ok(ToolInvocation::ProjectAttach {
            root: development_root(arguments)?,
            actor: required_string(arguments, "actor")?,
        }),
        "project.link" => Ok(ToolInvocation::ProjectLink {
            root: development_root(arguments)?,
            entity: required_string(arguments, "entity")?,
            path: required_string(arguments, "path")?,
            start_line: required_u32(arguments, "startLine")?,
            end_line: required_u32(arguments, "endLine")?,
            provenance: arguments
                .get("provenance")
                .and_then(Value::as_str)
                .unwrap_or("explicit-marker"),
            confidence: optional_number(arguments, "confidence", 1.0)? as f32,
            detail: arguments
                .get("detail")
                .and_then(Value::as_str)
                .unwrap_or("explicit project link"),
        }),
        "agent.hello" => Ok(ToolInvocation::AgentHello {
            root: development_root(arguments)?,
        }),
        "agent.prompt" => Ok(ToolInvocation::AgentPrompt {
            root: development_root(arguments)?,
            text: required_string(arguments, "text")?,
        }),
        "agent.steer" => Ok(ToolInvocation::AgentSteer {
            root: development_root(arguments)?,
            text: required_string(arguments, "text")?,
        }),
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
        "importCheckpoint" => Ok(ToolInvocation::ImportCheckpoint {
            checkpoint: arguments.clone(),
        }),
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

async fn ensure_session<'a>(
    session: &'a mut Option<BrowserSession>,
    options: &SessionOptions,
    policy: &BrowserPolicy,
    viewport: Option<(i64, i64)>,
) -> BrowserResult<&'a mut BrowserSession> {
    if session.is_none() {
        *session = Some(
            BrowserSession::start_with_policy_and_viewport(options, policy.clone(), viewport)
                .await?,
        );
    }
    Ok(session.as_mut().expect("session initialized"))
}

fn tools() -> Vec<Tool> {
    vec![
        Tool {
            name: "project.inspect",
            description: "Detect a local project and return bounded runtime configuration without starting Chrome.",
            input_schema: json!({"type":"object","properties":{"root":{"type":"string","default":"."}},"additionalProperties":false}),
        },
        Tool {
            name: "project.files",
            description: "List bounded, workspace-confined project files without starting Chrome.",
            input_schema: json!({"type":"object","properties":{"root":{"type":"string","default":"."}},"additionalProperties":false}),
        },
        Tool {
            name: "project.search",
            description: "Fuzzy-search files, runtime entities, processes, events, and commands.",
            input_schema: json!({"type":"object","properties":{"root":{"type":"string","default":"."},"query":{"type":"string"},"limit":{"type":"integer","minimum":1,"maximum":256,"default":64}},"required":["query"],"additionalProperties":false}),
        },
        Tool {
            name: "project.read",
            description: "Read one bounded file inside a local project workspace.",
            input_schema: json!({"type":"object","properties":{"root":{"type":"string","default":"."},"path":{"type":"string"}},"required":["path"],"additionalProperties":false}),
        },
        Tool {
            name: "project.edit",
            description: "Replace and save one bounded workspace-confined file with external-agent provenance.",
            input_schema: json!({"type":"object","properties":{"root":{"type":"string","default":"."},"path":{"type":"string"},"content":{"type":"string"}},"required":["path","content"],"additionalProperties":false}),
        },
        Tool {
            name: "project.mkdir",
            description: "Create one workspace-confined directory with external-agent provenance.",
            input_schema: json!({"type":"object","properties":{"root":{"type":"string","default":"."},"path":{"type":"string"}},"required":["path"],"additionalProperties":false}),
        },
        Tool {
            name: "project.rename",
            description: "Rename or move one workspace-confined path with external-agent provenance.",
            input_schema: json!({"type":"object","properties":{"root":{"type":"string","default":"."},"from":{"type":"string"},"to":{"type":"string"}},"required":["from","to"],"additionalProperties":false}),
        },
        Tool {
            name: "project.delete",
            description: "Delete one file or empty directory after explicit confirmation.",
            input_schema: json!({"type":"object","properties":{"root":{"type":"string","default":"."},"path":{"type":"string"},"confirmed":{"type":"boolean","const":true}},"required":["path","confirmed"],"additionalProperties":false}),
        },
        Tool {
            name: "project.diagnostics",
            description: "Request real rust-analyzer diagnostics through the bounded LSP client.",
            input_schema: json!({"type":"object","properties":{"root":{"type":"string","default":"."},"path":{"type":"string"}},"required":["path"],"additionalProperties":false}),
        },
        Tool {
            name: "project.run",
            description: "Run a local project command in a real PTY with a bounded output tail.",
            input_schema: json!({"type":"object","properties":{"root":{"type":"string","default":"."},"name":{"type":"string"},"command":{"type":"string"},"wait":{"type":"boolean","default":false}},"required":["name","command"],"additionalProperties":false}),
        },
        Tool {
            name: "project.processes",
            description: "List managed local project processes without starting Chrome.",
            input_schema: json!({"type":"object","properties":{"root":{"type":"string","default":"."}},"additionalProperties":false}),
        },
        Tool {
            name: "project.process.stop",
            description: "Stop one managed local project process.",
            input_schema: json!({"type":"object","properties":{"root":{"type":"string","default":"."},"name":{"type":"string"}},"required":["name"],"additionalProperties":false}),
        },
        Tool {
            name: "project.process.output",
            description: "Read the bounded output tail of one managed project process.",
            input_schema: json!({"type":"object","properties":{"root":{"type":"string","default":"."},"name":{"type":"string"}},"required":["name"],"additionalProperties":false}),
        },
        Tool {
            name: "project.diff",
            description: "Return code, runtime, semantic, and workflow impact for a local project.",
            input_schema: json!({"type":"object","properties":{"root":{"type":"string","default":"."}},"additionalProperties":false}),
        },
        Tool {
            name: "project.timeline",
            description: "Return the bounded actor-attributed local development timeline.",
            input_schema: json!({"type":"object","properties":{"root":{"type":"string","default":"."}},"additionalProperties":false}),
        },
        Tool {
            name: "project.events",
            description: "Read one cursor-bounded page from the actor-attributed development event feed.",
            input_schema: json!({"type":"object","properties":{"root":{"type":"string","default":"."},"afterId":{"type":"string","minLength":1,"maxLength":128},"limit":{"type":"integer","minimum":1,"maximum":256,"default":64}},"additionalProperties":false}),
        },
        Tool {
            name: "project.session.status",
            description: "Inspect whether a canonical project has a resident development session.",
            input_schema: json!({"type":"object","properties":{"root":{"type":"string","default":"."}},"additionalProperties":false}),
        },
        Tool {
            name: "project.session.detach",
            description: "Detach and clean up one resident project session after explicit confirmation.",
            input_schema: json!({"type":"object","properties":{"root":{"type":"string","default":"."},"confirmed":{"type":"boolean","const":true}},"required":["confirmed"],"additionalProperties":false}),
        },
        Tool {
            name: "project.capsule.save",
            description: "Atomically save a bounded, non-sensitive reconnect capsule for one project.",
            input_schema: json!({"type":"object","properties":{"root":{"type":"string","default":"."},"eventCursor":{"type":"string","maxLength":128},"mobileView":{"enum":["home","agent","app","diff","project"]},"browserTargetId":{"type":"string","maxLength":128},"browserRevision":{"type":"integer","minimum":0},"pendingAttention":{"type":"string","maxLength":256},"liveMode":{"enum":["off","auto","on"]},"liveQuality":{"enum":["auto","data","balanced","smooth"]}},"additionalProperties":false}),
        },
        Tool {
            name: "project.capsule.show",
            description: "Read the bounded reconnect capsule for one project when present.",
            input_schema: json!({"type":"object","properties":{"root":{"type":"string","default":"."}},"additionalProperties":false}),
        },
        Tool {
            name: "project.capsule.clear",
            description: "Remove one reconnect capsule after explicit confirmation.",
            input_schema: json!({"type":"object","properties":{"root":{"type":"string","default":"."},"confirmed":{"type":"boolean","const":true}},"required":["confirmed"],"additionalProperties":false}),
        },
        Tool {
            name: "project.inbox",
            description: "Return the bounded mobile attention inbox derived from actor-attributed events.",
            input_schema: json!({"type":"object","properties":{"root":{"type":"string","default":"."}},"additionalProperties":false}),
        },
        Tool {
            name: "project.verification.card",
            description: "Build a compact verification card from the resident project diff without implicit screenshots.",
            input_schema: json!({"type":"object","properties":{"root":{"type":"string","default":"."},"title":{"type":"string","minLength":1,"maxLength":128},"semanticRevision":{"type":"integer","minimum":0}},"required":["title"],"additionalProperties":false}),
        },
        Tool {
            name: "project.replay",
            description: "Replay a bounded window of actor-attributed development revisions.",
            input_schema: json!({"type":"object","properties":{"root":{"type":"string","default":"."},"start":{"type":"integer","minimum":0,"default":0},"limit":{"type":"integer","minimum":1,"maximum":256,"default":64}},"additionalProperties":false}),
        },
        Tool {
            name: "project.graph",
            description: "Discover explicit source/runtime markers or navigate the graph in either direction.",
            input_schema: json!({"type":"object","properties":{"root":{"type":"string","default":"."},"operation":{"type":"string","enum":["discover","entity","source"]},"entity":{"type":"string"},"path":{"type":"string"},"line":{"type":"integer","minimum":1}},"required":["operation"],"additionalProperties":false}),
        },
        Tool {
            name: "project.breakpoint",
            description: "Evaluate one semantic breakpoint against bounded before/after entity snapshots.",
            input_schema: json!({"type":"object","properties":{"root":{"type":"string","default":"."},"kind":{"type":"string","enum":["disappears","name-missing","role-changes","actionability-lost"]},"entity":{"type":"string"},"before":{"type":"object"},"after":{"type":"object"}},"required":["kind","entity","before","after"],"additionalProperties":false}),
        },
        Tool {
            name: "project.neovim.probe",
            description: "Probe real Neovim PTY compatibility and the headless RPC architecture prototype.",
            input_schema: json!({"type":"object","properties":{},"additionalProperties":false}),
        },
        Tool {
            name: "project.experiment.create",
            description: "Create an isolated Git worktree experiment with a dedicated local dev port.",
            input_schema: json!({"type":"object","properties":{"root":{"type":"string","default":"."},"name":{"type":"string"},"port":{"type":"integer","minimum":1,"maximum":65535}},"required":["name","port"],"additionalProperties":false}),
        },
        Tool {
            name: "project.attach",
            description: "Attach an external agent actor to the attributed development timeline.",
            input_schema: json!({"type":"object","properties":{"root":{"type":"string","default":"."},"actor":{"type":"string"}},"required":["actor"],"additionalProperties":false}),
        },
        Tool {
            name: "project.link",
            description: "Record a source/runtime link with explicit provenance and confidence.",
            input_schema: json!({"type":"object","properties":{"root":{"type":"string","default":"."},"entity":{"type":"string"},"path":{"type":"string"},"startLine":{"type":"integer","minimum":1},"endLine":{"type":"integer","minimum":1},"provenance":{"type":"string","enum":["explicit-marker","runtime-observation","static-analysis","inferred"],"default":"explicit-marker"},"confidence":{"type":"number","minimum":0,"maximum":1,"default":1},"detail":{"type":"string"}},"required":["entity","path","startLine","endLine"],"additionalProperties":false}),
        },
        Tool {
            name: "agent.hello",
            description: "Negotiate the Glass-owned local harness protocol.",
            input_schema: json!({"type":"object","properties":{"root":{"type":"string","default":"."}},"additionalProperties":false}),
        },
        Tool {
            name: "agent.prompt",
            description: "Run one bounded prompt through the deterministic local Glass harness.",
            input_schema: json!({"type":"object","properties":{"root":{"type":"string","default":"."},"text":{"type":"string"}},"required":["text"],"additionalProperties":false}),
        },
        Tool {
            name: "agent.steer",
            description: "Send a steering event to the local Glass harness.",
            input_schema: json!({"type":"object","properties":{"root":{"type":"string","default":"."},"text":{"type":"string"}},"required":["text"],"additionalProperties":false}),
        },
        Tool {
            name: "inspectWebIr",
            description: "Inspect a validated browser-free Glass Web IR v1 without starting Chrome.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "ir": {
                        "type": "object",
                        "description": "Bounded Glass Web IR v1 JSON."
                    }
                },
                "required": ["ir"],
                "additionalProperties": false
            }),
        },
        Tool {
            name: "validateWebIr",
            description: "Validate a browser-free Glass Web IR v1 without starting Chrome.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "ir": {
                        "type": "object",
                        "description": "Bounded Glass Web IR v1 JSON."
                    }
                },
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
                    "before": {
                        "type": "object",
                        "description": "Earlier bounded Glass Web IR v1 JSON."
                    },
                    "after": {
                        "type": "object",
                        "description": "Later bounded Glass Web IR v1 JSON."
                    }
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
                    "before": {
                        "type": "object",
                        "description": "Earlier bounded Glass Web IR v1 JSON."
                    },
                    "after": {
                        "type": "object",
                        "description": "Later bounded Glass Web IR v1 JSON."
                    },
                    "entityId": {
                        "type": "string",
                        "description": "Revision-local entity ID from the earlier Web IR."
                    }
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
                "properties": {
                    "task": {
                        "type": "object",
                        "description": "Strict Task Protocol v1 authored task."
                    }
                },
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
                    "task": {
                        "type": "object",
                        "description": "Strict Task Protocol v1 authored task."
                    },
                    "ir": {
                        "type": "object",
                        "description": "Validated stable Glass Web IR v1 source document."
                    }
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
                    "task": {
                        "type": "object",
                        "description": "Validated Task Protocol v1 authored task from a form, navigation, dialog, pagination, extraction, or field-read family."
                    },
                    "expectedRevision": {
                        "type": "integer",
                        "minimum": 0,
                        "description": "Revision returned by the caller's preceding semantic observation."
                    },
                    "confirmed": {
                        "type": "boolean",
                        "default": false,
                        "description": "Explicit confirmation for risky or ambiguity-gated tasks."
                    },
                    "leaseToken": {
                        "type": "string",
                        "minLength": 1,
                        "maxLength": 256,
                        "description": "Mutation lease token issued by glass/lease/acquire when running against a daemon."
                    },
                    "responseMode": {
                        "type": "string",
                        "enum": ["minimal", "normal", "diagnostic"],
                        "default": "minimal"
                    }
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
            input_schema: target_schema(),
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
    ]
}

fn target_schema() -> Value {
    json!({"type":"object","properties":{"target":{"type":"string"},"includeTrace":{"type":"boolean","default":false}},"required":["target"]})
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

fn required_u32(arguments: &Value, name: &str) -> BrowserResult<u32> {
    let value = required_u64(arguments, name)?;
    u32::try_from(value)
        .map_err(|_| format!("{name} must fit in an unsigned 32-bit integer").into())
}

fn development_root(arguments: &Value) -> BrowserResult<std::path::PathBuf> {
    let root = arguments.get("root").and_then(Value::as_str).unwrap_or(".");
    if root.is_empty() || root.len() > 1024 {
        return Err("root must be a path of 1-1024 bytes".into());
    }
    Ok(std::path::PathBuf::from(root))
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
        async fn invoke(policy: &BrowserPolicy, url: &str) -> (Value, Option<BrowserSession>) {
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
            cancellations
                .lock()
                .unwrap()
                .insert(request_id_key(&request_id).unwrap(), sender);
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

    #[test]
    fn development_dispatch_reuses_resident_workspace_state() {
        let root = std::env::temp_dir().join(format!(
            "glass-mcp-resident-{}-{}",
            std::process::id(),
            current_time_ms()
        ));
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(
            root.join("Cargo.toml"),
            "[package]\nname='resident'\nversion='0.1.0'\n",
        )
        .unwrap();
        let mut sessions = ResidentDevelopmentSessions::default();
        call_development_tool(
            ToolInvocation::ProjectAttach {
                root: root.clone(),
                actor: "sdk",
            },
            &mut sessions,
        )
        .unwrap();
        let actors = sessions
            .with_workspace(&root, |workspace| {
                Ok(workspace
                    .actors()
                    .map(|actor| actor.id.clone())
                    .collect::<Vec<_>>())
            })
            .unwrap();
        assert!(actors.iter().any(|actor| actor == "external:sdk"));
        let opened = ProjectWorkspace::timeline_snapshot(&root)
            .unwrap()
            .into_iter()
            .filter(|event| {
                matches!(
                    event.kind,
                    crate::development::DevelopmentEventKind::WorkspaceOpened
                )
            })
            .count();
        assert_eq!(opened, 1);
        assert!(sessions.detach(&root).unwrap());
        let _ = std::fs::remove_dir_all(root);
    }
}
