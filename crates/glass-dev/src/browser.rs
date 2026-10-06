//! Resident browser and workflow ownership for the Glass development workspace.

use crate::development::{DevelopmentError, DevelopmentResult};
use crate::development::{RemoteFrame, RemoteInput, RemoteView};
use glass_browser::browser::policy::{BrowserPolicy, PolicyPreset};
use glass_browser::browser::session::{
    CdpBrowserSession, SemanticObservation, SemanticObservationLevel, SessionOptions,
    VerificationPredicate, WorkflowCheckpoint, WorkflowDefinition, WorkflowRunResult,
};
use glass_browser::browser::{
    BrowserRuntimeSession, NativeDialogController, NativeDialogResolution, NativeEngineConfig,
    NativeHistoryDirection, NativePendingDialog,
};
use glass_browser::browser_backend::{ActionResult, SemanticAction};
use glass_browser::extraction::ExtractionRequest;
use glass_browser::protocol::WebIrInspectionResult;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, SyncSender};
use std::sync::{Arc, Mutex};
use std::time::Duration;

const COMMAND_QUEUE: usize = 32;
const COMMAND_TIMEOUT: Duration = Duration::from_secs(180);
const BROWSER_WORKER_STACK_BYTES: usize = 8 * 1024 * 1024;

/// Options used to create or attach the resident development browser session.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct BrowserStartConfig {
    /// Chrome DevTools port.
    pub port: u16,
    /// Attach to an existing browser rather than launching one.
    pub attach: bool,
    /// Use an isolated incognito context.
    pub incognito: bool,
    /// Show the browser window.
    pub headed: bool,
    /// Named profile passed to the browser session.
    pub profile: String,
    /// Optional explicit Chrome executable path.
    pub chrome_path: Option<PathBuf>,
    /// Enable an out-of-band host controller for synchronous native page dialogs.
    pub modal_dialogs: bool,
}

impl Default for BrowserStartConfig {
    fn default() -> Self {
        Self {
            port: default_port(),
            attach: false,
            incognito: true,
            headed: false,
            profile: default_profile(),
            chrome_path: None,
            modal_dialogs: false,
        }
    }
}

fn default_port() -> u16 {
    9222
}

fn default_profile() -> String {
    "default".into()
}

/// Serializable state of the resident browser worker.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserRuntimeState {
    /// Policy preset governing browser capabilities and host access.
    pub policy_preset: PolicyPreset,
    /// Whether a browser session is connected.
    pub connected: bool,
    /// Backend serving the resident session. Normal starts are native;
    /// explicit attach requests report Chromium.
    pub browser_backend: Option<String>,
    /// PID when this service owns the browser process.
    pub browser_process_id: Option<u32>,
    /// Latest observation revision known to the worker.
    pub browser_revision: Option<u64>,
    /// Current workflow lifecycle state.
    pub workflow_state: String,
    /// Identifier of the active workflow, if any.
    pub active_workflow: Option<String>,
}

enum BrowserCommand {
    Start(BrowserStartConfig),
    Reconnect,
    Stop,
    State,
    Observe,
    Snapshot,
    Semantic(SemanticObservationLevel),
    WebIr,
    Diff,
    Targets,
    SelectTarget(String),
    Navigate {
        url: String,
        expected_revision: u64,
        timeout: Duration,
    },
    Back(u64),
    Forward(u64),
    Reload(u64),
    StopLoading(u64),
    Click {
        target: String,
        expected_revision: u64,
    },
    Type {
        text: String,
        target: Option<String>,
        expected_revision: u64,
    },
    Scroll {
        dx: f64,
        dy: f64,
        expected_revision: u64,
    },
    Screenshot,
    RunWorkflow {
        definition: Value,
        inputs: BTreeMap<String, Value>,
    },
    PauseWorkflow,
    ResumeWorkflow {
        definition: Value,
        inputs: BTreeMap<String, Value>,
        checkpoint: Value,
    },
    ListWorkflows,
    CancelWorkflow,
    VerifyWorkflow,
    VerifyPredicate {
        predicate: VerificationPredicate,
        timeout: Duration,
    },
    RemoteViewOpen,
    RemoteViewStatus,
    RemoteViewRevoke,
}

type Reply = SyncSender<DevelopmentResult<Value>>;

/// One resident browser connection. Native is the default product path;
/// Chromium is retained only behind the explicit attach/migration command.
enum ResidentBrowserSession {
    Native(BrowserRuntimeSession),
    Chromium(Box<CdpBrowserSession>),
}

impl ResidentBrowserSession {
    fn backend_id(&self) -> &'static str {
        match self {
            Self::Native(_) => "native",
            Self::Chromium(_) => "chromium",
        }
    }

    fn owned_chrome_pid(&self) -> Option<u32> {
        match self {
            Self::Native(_) => None,
            Self::Chromium(session) => session.owned_chrome_pid(),
        }
    }

    async fn close(self) -> DevelopmentResult<()> {
        match self {
            Self::Native(session) => session.close().await.map_err(browser_error),
            Self::Chromium(session) => session.close().await.map_err(browser_error),
        }
    }
}

/// Cloneable command handle for the one authoritative browser worker.
#[derive(Clone)]
pub(crate) struct BrowserService {
    commands: SyncSender<(BrowserCommand, Reply)>,
    native_dialog_controller: Arc<Mutex<Option<NativeDialogController>>>,
}

impl BrowserService {
    /// Create a resident browser worker with the development policy.
    #[cfg(test)]
    pub(crate) fn new(root: impl AsRef<Path>) -> DevelopmentResult<Self> {
        Self::new_with_policy(root, PolicyPreset::Development)
    }

    /// Create a resident browser worker with an explicit authorization preset.
    pub(crate) fn new_with_policy(
        root: impl AsRef<Path>,
        policy_preset: PolicyPreset,
    ) -> DevelopmentResult<Self> {
        let root = root.as_ref().to_path_buf();
        let (commands, receiver) = mpsc::sync_channel::<(BrowserCommand, Reply)>(COMMAND_QUEUE);
        let native_dialog_controller = Arc::new(Mutex::new(None));
        let worker_dialog_controller = Arc::clone(&native_dialog_controller);
        std::thread::Builder::new()
            .name("glass-browser-workspace".into())
            .stack_size(BROWSER_WORKER_STACK_BYTES)
            .spawn(move || {
                let runtime = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build();
                let Ok(runtime) = runtime else {
                    return;
                };
                let mut worker = BrowserWorker::new(root, policy_preset, worker_dialog_controller);
                while let Ok((command, reply)) = receiver.recv() {
                    let result = runtime.block_on(worker.execute(command));
                    let _ = reply.send(result);
                }
                runtime.block_on(worker.shutdown());
            })
            .map_err(|error| DevelopmentError::Process(error.to_string()))?;
        Ok(Self {
            commands,
            native_dialog_controller,
        })
    }

    fn call(&self, command: BrowserCommand) -> DevelopmentResult<Value> {
        let (reply, receiver) = mpsc::sync_channel(1);
        self.commands
            .send((command, reply))
            .map_err(|_| DevelopmentError::Process("resident browser worker stopped".into()))?;
        receiver.recv_timeout(COMMAND_TIMEOUT).map_err(|error| {
            DevelopmentError::Process(format!("resident browser command timed out: {error}"))
        })?
    }

    /// Start a browser session; fails if one is already connected.
    pub(crate) fn start(&self, config: BrowserStartConfig) -> DevelopmentResult<Value> {
        self.call(BrowserCommand::Start(config))
    }

    /// Stop the session and revoke any remote view.
    pub(crate) fn stop(&self) -> DevelopmentResult<Value> {
        self.call(BrowserCommand::Stop)
    }

    /// Restart using the most recent start configuration.
    pub(crate) fn reconnect(&self) -> DevelopmentResult<Value> {
        self.call(BrowserCommand::Reconnect)
    }

    /// Return connection, workflow, and latest revision state.
    pub(crate) fn state(&self) -> DevelopmentResult<Value> {
        self.call(BrowserCommand::State)
    }

    /// Capture a fresh observation and advance the worker's known revision.
    pub(crate) fn observe(&self) -> DevelopmentResult<Value> {
        self.call(BrowserCommand::Observe)
    }

    /// Capture the session's current snapshot without forcing a fresh observe.
    pub(crate) fn snapshot(&self) -> DevelopmentResult<Value> {
        self.call(BrowserCommand::Snapshot)
    }

    /// Capture semantic observations at the requested level.
    pub(crate) fn semantic(&self, level: SemanticObservationLevel) -> DevelopmentResult<Value> {
        self.call(BrowserCommand::Semantic(level))
    }

    /// Extract the current page into a bounded Web IR inspection summary.
    pub(crate) fn web_ir(&self) -> DevelopmentResult<Value> {
        self.call(BrowserCommand::WebIr)
    }

    /// Return the observation delta since the session's previous revision.
    pub(crate) fn diff(&self) -> DevelopmentResult<Value> {
        self.call(BrowserCommand::Diff)
    }

    /// List browser targets.
    pub(crate) fn targets(&self) -> DevelopmentResult<Value> {
        self.call(BrowserCommand::Targets)
    }

    /// Select a target and return a fresh observation for it.
    pub(crate) fn select_target(&self, target: String) -> DevelopmentResult<Value> {
        self.call(BrowserCommand::SelectTarget(target))
    }

    /// Navigate if `expected_revision` still matches; `timeout` bounds navigation.
    pub(crate) fn navigate(
        &self,
        url: String,
        expected_revision: u64,
        timeout: Duration,
    ) -> DevelopmentResult<Value> {
        self.call(BrowserCommand::Navigate {
            url,
            expected_revision,
            timeout,
        })
    }

    /// Read the exact pending process-backed native dialog without waiting for
    /// the serialized browser worker. This is available only after starting a
    /// native session with `modal_dialogs: true`.
    pub(crate) fn pending_native_dialog(&self) -> DevelopmentResult<Option<NativePendingDialog>> {
        self.dialog_controller()?
            .pending_dialog()
            .map_err(|error| DevelopmentError::Process(error.to_string()))
    }

    /// Whether this service currently exposes an out-of-band native dialog
    /// controller. This is a nonblocking capability check for interactive
    /// hosts that poll while a browser operation is suspended.
    pub(crate) fn native_dialog_control_enabled(&self) -> DevelopmentResult<bool> {
        self.native_dialog_controller
            .lock()
            .map(|controller| controller.is_some())
            .map_err(|_| {
                DevelopmentError::Process(
                    "native browser dialog control state is unavailable".into(),
                )
            })
    }

    /// Resolve the exact pending native dialog without waiting for the
    /// serialized browser worker. A stale identity is rejected and leaves the
    /// page operation suspended.
    pub(crate) fn resolve_native_dialog(
        &self,
        dialog_id: &str,
        resolution: NativeDialogResolution,
    ) -> DevelopmentResult<()> {
        self.dialog_controller()?
            .resolve_dialog(dialog_id, resolution)
            .map_err(|error| DevelopmentError::Process(error.to_string()))
    }

    fn dialog_controller(&self) -> DevelopmentResult<NativeDialogController> {
        self.native_dialog_controller
            .lock()
            .map_err(|_| {
                DevelopmentError::Process(
                    "native browser dialog control state is unavailable".into(),
                )
            })?
            .clone()
            .ok_or_else(|| {
                DevelopmentError::Conflict(
                    "native modal dialog control is not enabled for the connected browser".into(),
                )
            })
    }
    /// Go back if the supplied observation revision is current.
    pub(crate) fn back(&self, expected_revision: u64) -> DevelopmentResult<Value> {
        self.call(BrowserCommand::Back(expected_revision))
    }
    /// Go forward if the supplied observation revision is current.
    pub(crate) fn forward(&self, expected_revision: u64) -> DevelopmentResult<Value> {
        self.call(BrowserCommand::Forward(expected_revision))
    }
    /// Reload if the supplied observation revision is current.
    pub(crate) fn reload(&self, expected_revision: u64) -> DevelopmentResult<Value> {
        self.call(BrowserCommand::Reload(expected_revision))
    }
    /// Stop loading if the supplied observation revision is current.
    pub(crate) fn stop_loading(&self, expected_revision: u64) -> DevelopmentResult<Value> {
        self.call(BrowserCommand::StopLoading(expected_revision))
    }
    /// Click a target if the supplied observation revision is current.
    pub(crate) fn click(&self, target: String, expected_revision: u64) -> DevelopmentResult<Value> {
        self.call(BrowserCommand::Click {
            target,
            expected_revision,
        })
    }
    /// Type text into the selected or explicit target at the current revision.
    pub(crate) fn type_text(
        &self,
        text: String,
        target: Option<String>,
        expected_revision: u64,
    ) -> DevelopmentResult<Value> {
        self.call(BrowserCommand::Type {
            text,
            target,
            expected_revision,
        })
    }
    /// Scroll by the requested delta at the current observation revision.
    pub(crate) fn scroll(
        &self,
        dx: f64,
        dy: f64,
        expected_revision: u64,
    ) -> DevelopmentResult<Value> {
        self.call(BrowserCommand::Scroll {
            dx,
            dy,
            expected_revision,
        })
    }
    /// Capture a screenshot of the connected browser.
    pub(crate) fn screenshot(&self) -> DevelopmentResult<Value> {
        self.call(BrowserCommand::Screenshot)
    }

    /// Start a workflow from its JSON definition and named inputs.
    pub(crate) fn run_workflow(
        &self,
        definition: Value,
        inputs: BTreeMap<String, Value>,
    ) -> DevelopmentResult<Value> {
        self.call(BrowserCommand::RunWorkflow { definition, inputs })
    }

    /// Pause the active workflow and retain its checkpoint.
    pub(crate) fn pause_workflow(&self) -> DevelopmentResult<Value> {
        self.call(BrowserCommand::PauseWorkflow)
    }

    /// Resume a workflow from a caller-provided checkpoint.
    pub(crate) fn resume_workflow(
        &self,
        definition: Value,
        inputs: BTreeMap<String, Value>,
        checkpoint: Value,
    ) -> DevelopmentResult<Value> {
        self.call(BrowserCommand::ResumeWorkflow {
            definition,
            inputs,
            checkpoint,
        })
    }

    /// List workflow definitions known to the connected session.
    pub(crate) fn list_workflows(&self) -> DevelopmentResult<Value> {
        self.call(BrowserCommand::ListWorkflows)
    }

    /// Cancel the active workflow.
    pub(crate) fn cancel_workflow(&self) -> DevelopmentResult<Value> {
        self.call(BrowserCommand::CancelWorkflow)
    }

    /// Verify the active workflow's completion conditions.
    pub(crate) fn verify_workflow(&self) -> DevelopmentResult<Value> {
        self.call(BrowserCommand::VerifyWorkflow)
    }

    /// Evaluate a causal verification predicate against the live page.
    pub(crate) fn verify_predicate(
        &self,
        predicate: VerificationPredicate,
        timeout: Duration,
    ) -> DevelopmentResult<Value> {
        self.call(BrowserCommand::VerifyPredicate { predicate, timeout })
    }

    /// Open a remote-view capability for the connected browser.
    pub(crate) fn open_remote_view(&self) -> DevelopmentResult<Value> {
        self.call(BrowserCommand::RemoteViewOpen)
    }

    /// Return remote-view status and current capability state.
    pub(crate) fn remote_view_status(&self) -> DevelopmentResult<Value> {
        self.call(BrowserCommand::RemoteViewStatus)
    }

    /// Revoke the remote-view capability.
    pub(crate) fn revoke_remote_view(&self) -> DevelopmentResult<Value> {
        self.call(BrowserCommand::RemoteViewRevoke)
    }
}
struct BrowserWorker {
    root: PathBuf,
    policy_preset: PolicyPreset,
    native_dialog_controller: Arc<Mutex<Option<NativeDialogController>>>,
    session: Option<ResidentBrowserSession>,
    revision: Option<u64>,
    native_previous_observation: Option<SemanticObservation>,
    workflow_state: String,
    active_workflow: Option<String>,
    last_workflow: Option<(WorkflowDefinition, WorkflowRunResult)>,
    last_config: Option<BrowserStartConfig>,
    remote_view: Option<RemoteView>,
}

impl BrowserWorker {
    fn new(
        root: PathBuf,
        policy_preset: PolicyPreset,
        native_dialog_controller: Arc<Mutex<Option<NativeDialogController>>>,
    ) -> Self {
        Self {
            root,
            policy_preset,
            native_dialog_controller,
            session: None,
            revision: None,
            native_previous_observation: None,
            workflow_state: "idle".into(),
            active_workflow: None,
            last_workflow: None,
            last_config: None,
            remote_view: None,
        }
    }

    fn state(&self) -> BrowserRuntimeState {
        BrowserRuntimeState {
            policy_preset: self.policy_preset,
            connected: self.session.is_some(),
            browser_backend: self
                .session
                .as_ref()
                .map(ResidentBrowserSession::backend_id)
                .map(str::to_owned),
            browser_process_id: self
                .session
                .as_ref()
                .and_then(ResidentBrowserSession::owned_chrome_pid),
            browser_revision: self.revision,
            workflow_state: self.workflow_state.clone(),
            active_workflow: self.active_workflow.clone(),
        }
    }

    fn session(&self) -> DevelopmentResult<&ResidentBrowserSession> {
        self.session.as_ref().ok_or_else(|| {
            DevelopmentError::Conflict(
                "browser is not connected; call glass.browser.start first".into(),
            )
        })
    }

    async fn shutdown(&mut self) {
        match self.native_dialog_controller.lock() {
            Ok(mut controller) => *controller = None,
            Err(poisoned) => *poisoned.into_inner() = None,
        }
        if let Some(view) = self.remote_view.take() {
            view.revoke().await;
        }
        if let Some(session) = self.session.take() {
            let _ = session.close().await;
        }
        self.revision = None;
        self.native_previous_observation = None;
        self.workflow_state = "idle".into();
        self.active_workflow = None;
        self.last_workflow = None;
    }

    async fn start_session(&mut self, config: BrowserStartConfig) -> DevelopmentResult<Value> {
        if self.session.is_some() {
            return Err(DevelopmentError::Conflict(
                "browser workspace already has a connected session".into(),
            ));
        }
        if config.attach && config.modal_dialogs {
            return Err(DevelopmentError::InvalidInput(
                "modalDialogs is supported only by the native browser runtime".into(),
            ));
        }
        let session = if config.attach {
            let policy = BrowserPolicy::from_preset(self.policy_preset, &self.root)
                .map_err(|error| DevelopmentError::Process(error.to_string()))?;
            let mut builder = SessionOptions::builder()
                .port(config.port)
                .attach(true)
                .incognito(config.incognito)
                .headed(config.headed)
                .profile(config.profile.clone())
                .policy(policy);
            if let Some(path) = config.chrome_path.clone() {
                builder = builder.chrome_path(path);
            }
            let options = builder
                .build()
                .map_err(|error| DevelopmentError::InvalidInput(error.to_string()))?;
            ResidentBrowserSession::Chromium(Box::new(
                CdpBrowserSession::start(&options)
                    .await
                    .map_err(|error| DevelopmentError::Process(error.to_string()))?,
            ))
        } else {
            if config.headed || config.chrome_path.is_some() {
                return Err(DevelopmentError::InvalidInput(
                    "headed and chromePath require explicit glass.browser.attach; glass.browser.start uses the native engine"
                        .into(),
                ));
            }
            let mut native = NativeEngineConfig::default();
            if !config.incognito {
                native = native.with_storage_path(native_profile_storage_path(&config.profile)?);
            }
            let runtime = if config.modal_dialogs {
                let runtime = BrowserRuntimeSession::connect_native_with_modal_dialogs(native)
                    .await
                    .map_err(browser_error)?;
                let controller = runtime.native_dialog_controller().map_err(browser_error)?;
                *self.native_dialog_controller.lock().map_err(|_| {
                    DevelopmentError::Process(
                        "native browser dialog control state is unavailable".into(),
                    )
                })? = Some(controller);
                runtime
            } else {
                BrowserRuntimeSession::connect_native(native)
                    .await
                    .map_err(browser_error)?
            };
            ResidentBrowserSession::Native(runtime)
        };
        self.revision = Some(1);
        self.session = Some(session);
        self.last_config = Some(config);
        serde_json::to_value(self.state()).map_err(Into::into)
    }

    fn browser_policy(&self) -> DevelopmentResult<BrowserPolicy> {
        BrowserPolicy::from_preset(self.policy_preset, &self.root)
            .map_err(|error| DevelopmentError::Process(error.to_string()))
    }

    async fn execute(&mut self, command: BrowserCommand) -> DevelopmentResult<Value> {
        self.apply_remote_inputs().await?;
        match command {
            BrowserCommand::Start(config) => self.start_session(config).await,
            BrowserCommand::Reconnect => {
                let config = self.last_config.clone().ok_or_else(|| {
                    DevelopmentError::Conflict(
                        "browser has no prior connection configuration to reconnect".into(),
                    )
                })?;
                self.shutdown().await;
                self.start_session(config).await
            }
            BrowserCommand::Stop => {
                self.shutdown().await;
                serde_json::to_value(self.state()).map_err(Into::into)
            }
            BrowserCommand::State => serde_json::to_value(self.state()).map_err(Into::into),
            BrowserCommand::Observe => {
                match self.session()? {
                    ResidentBrowserSession::Native(session) => {
                        let observation = session.native_observe().await.map_err(browser_error)?;
                        self.revision = Some(observation.revision);
                        self.native_previous_observation = Some(observation.clone());
                        serde_json::to_value(observation).map_err(Into::into)
                    }
                    ResidentBrowserSession::Chromium(session) => {
                        let context = session.observe_fresh().await.map_err(browser_error)?;
                        self.revision = Some(context.consistency.end_revision);
                        serde_json::to_value(context).map_err(Into::into)
                    }
                }
            }
            BrowserCommand::Snapshot => {
                match self.session()? {
                    ResidentBrowserSession::Native(session) => {
                        let snapshot = session.native_inspect_page().await.map_err(browser_error)?;
                        self.revision = Some(snapshot.revision);
                        serde_json::to_value(snapshot).map_err(Into::into)
                    }
                    ResidentBrowserSession::Chromium(session) => {
                        let snapshot = session.snapshot().await.map_err(browser_error)?;
                        serde_json::to_value(snapshot).map_err(Into::into)
                    }
                }
            }
            BrowserCommand::Semantic(level) => {
                match self.session()? {
                    ResidentBrowserSession::Native(session) => {
                        let observation = session
                            .native_semantic_observe(level)
                            .await
                            .map_err(browser_error)?;
                        self.revision = Some(observation.revision);
                        serde_json::to_value(observation).map_err(Into::into)
                    }
                    ResidentBrowserSession::Chromium(session) => {
                        let observation = session
                            .semantic_observe(level)
                            .await
                            .map_err(browser_error)?;
                        serde_json::to_value(observation).map_err(Into::into)
                    }
                }
            }
            BrowserCommand::WebIr => {
                match self.session()? {
                    ResidentBrowserSession::Native(session) => {
                        let ir = session
                            .native_extract_web_ir(&ExtractionRequest::default_document())
                            .await
                            .map_err(browser_error)?;
                        self.revision = Some(ir.revision);
                        serde_json::to_value(WebIrInspectionResult::from_ir(&ir)).map_err(Into::into)
                    }
                    ResidentBrowserSession::Chromium(session) => {
                        let ir = session
                            .extract_web_ir(&ExtractionRequest::default_document())
                            .await
                            .map_err(browser_error)?;
                        self.revision = Some(ir.revision);
                        serde_json::to_value(WebIrInspectionResult::from_ir(&ir)).map_err(Into::into)
                    }
                }
            }
            BrowserCommand::Diff => {
                match self.session()? {
                    ResidentBrowserSession::Native(session) => {
                        let previous = self.native_previous_observation.clone().ok_or_else(|| {
                            DevelopmentError::Conflict(
                                "browser diff requires a prior native observation".into(),
                            )
                        })?;
                        let current = session.native_observe().await.map_err(browser_error)?;
                        let delta = current
                            .diff_from(&previous)
                            .map_err(|error| DevelopmentError::Process(error.to_string()))?;
                        self.revision = Some(current.revision);
                        self.native_previous_observation = Some(current);
                        serde_json::to_value(delta).map_err(Into::into)
                    }
                    ResidentBrowserSession::Chromium(session) => {
                        let delta = session.observe_delta().await.map_err(browser_error)?;
                        self.revision = Some(delta.to_revision);
                        serde_json::to_value(delta).map_err(Into::into)
                    }
                }
            }
            BrowserCommand::Targets => {
                match self.session()? {
                    ResidentBrowserSession::Native(session) => {
                        let targets = session.native_list_targets().await.map_err(browser_error)?;
                        serde_json::to_value(targets).map_err(Into::into)
                    }
                    ResidentBrowserSession::Chromium(session) => {
                        let targets = session.list_targets().await.map_err(browser_error)?;
                        serde_json::to_value(targets).map_err(Into::into)
                    }
                }
            }
            BrowserCommand::SelectTarget(target) => {
                match self.session()? {
                    ResidentBrowserSession::Native(session) => {
                        let selected = session
                            .native_select_target(&target)
                            .await
                            .map_err(browser_error)?;
                        let observation = session.native_observe().await.map_err(browser_error)?;
                        self.revision = Some(observation.revision);
                        self.native_previous_observation = Some(observation.clone());
                        Ok(serde_json::json!({"target":selected,"observation":observation}))
                    }
                    ResidentBrowserSession::Chromium(session) => {
                        let selected = session
                            .select_target(&target)
                            .await
                            .map_err(browser_error)?;
                        let observation = session.observe_fresh().await.map_err(browser_error)?;
                        self.revision = Some(observation.consistency.end_revision);
                        Ok(serde_json::json!({"target":selected,"observation":observation}))
                    }
                }
            }
            BrowserCommand::Navigate {
                url,
                expected_revision,
                timeout,
            } => {
                match self.session()? {
                    ResidentBrowserSession::Native(session) => {
                        let outcome = tokio::time::timeout(
                            timeout,
                            session.navigate_with_revision(&url, expected_revision),
                        )
                        .await
                        .map_err(|_| {
                            DevelopmentError::Process("native browser navigation timed out".into())
                        })?
                        .map_err(browser_error)?;
                        self.revision = Some(outcome.revision);
                        Ok(serde_json::json!({
                            "url": outcome.url,
                            "revision": outcome.revision,
                            "currentRevision": outcome.revision,
                            "browserRevision": outcome.revision,
                            "backend": "native"
                        }))
                    }
                    ResidentBrowserSession::Chromium(session) => {
                        let outcome = session
                            .navigate_with_revision(&url, timeout, expected_revision)
                            .await
                            .map_err(browser_error)?;
                        let value = serde_json::to_value(outcome)?;
                        self.revision = value.get("currentRevision").and_then(Value::as_u64);
                        Ok(value)
                    }
                }
            }
            BrowserCommand::Back(expected_revision) => {
                match self.session()? {
                    ResidentBrowserSession::Native(session) => {
                        let outcome = session
                            .native_navigate_history_with_revision(
                                NativeHistoryDirection::Back,
                                expected_revision,
                            )
                            .await
                            .map_err(browser_error)?;
                        self.revision = Some(outcome.current_revision);
                        serde_json::to_value(outcome).map_err(Into::into)
                    }
                    ResidentBrowserSession::Chromium(session) => {
                        let outcome = session
                            .go_back_with_revision(expected_revision)
                            .await
                            .map_err(browser_error)?;
                        self.revision = Some(outcome.current_revision);
                        serde_json::to_value(outcome).map_err(Into::into)
                    }
                }
            }
            BrowserCommand::Forward(expected_revision) => {
                match self.session()? {
                    ResidentBrowserSession::Native(session) => {
                        let outcome = session
                            .native_navigate_history_with_revision(
                                NativeHistoryDirection::Forward,
                                expected_revision,
                            )
                            .await
                            .map_err(browser_error)?;
                        self.revision = Some(outcome.current_revision);
                        serde_json::to_value(outcome).map_err(Into::into)
                    }
                    ResidentBrowserSession::Chromium(session) => {
                        let outcome = session
                            .go_forward_with_revision(expected_revision)
                            .await
                            .map_err(browser_error)?;
                        self.revision = Some(outcome.current_revision);
                        serde_json::to_value(outcome).map_err(Into::into)
                    }
                }
            }
            BrowserCommand::Reload(expected_revision) => {
                match self.session()? {
                    ResidentBrowserSession::Native(session) => {
                        let outcome = session
                            .native_reload_with_revision(expected_revision)
                            .await
                            .map_err(browser_error)?;
                        self.revision = Some(outcome.current_revision);
                        serde_json::to_value(outcome).map_err(Into::into)
                    }
                    ResidentBrowserSession::Chromium(session) => {
                        let outcome = session
                            .reload_with_revision(expected_revision)
                            .await
                            .map_err(browser_error)?;
                        self.revision = Some(outcome.current_revision);
                        serde_json::to_value(outcome).map_err(Into::into)
                    }
                }
            }
            BrowserCommand::StopLoading(expected_revision) => {
                match self.session()? {
                    ResidentBrowserSession::Native(session) => {
                        let outcome = session
                            .native_stop_loading_with_revision(expected_revision)
                            .await
                            .map_err(browser_error)?;
                        self.revision = Some(outcome.current_revision);
                        serde_json::to_value(outcome).map_err(Into::into)
                    }
                    ResidentBrowserSession::Chromium(session) => {
                        let outcome = session
                            .stop_loading_with_revision(expected_revision)
                            .await
                            .map_err(browser_error)?;
                        self.revision = Some(outcome.current_revision);
                        serde_json::to_value(outcome).map_err(Into::into)
                    }
                }
            }
            BrowserCommand::Click {
                target,
                expected_revision,
            } => {
                match self.session()? {
                    ResidentBrowserSession::Native(session) => {
                        let result = session
                            .action_with_revision(
                                SemanticAction::Click {
                                    target: target.clone(),
                                },
                                expected_revision,
                            )
                            .await
                            .map_err(browser_error)?;
                        self.revision = Some(result.revision);
                        Ok(native_action_value(
                            result,
                            "click",
                            Some(&target),
                            expected_revision,
                        ))
                    }
                    ResidentBrowserSession::Chromium(session) => {
                        let outcome = session
                            .click_with_revision(&target, expected_revision)
                            .await
                            .map_err(browser_error)?;
                        self.revision = Some(outcome.current_revision);
                        serde_json::to_value(outcome).map_err(Into::into)
                    }
                }
            }
            BrowserCommand::Type {
                text,
                target,
                expected_revision,
            } => {
                match self.session()? {
                    ResidentBrowserSession::Native(session) => {
                        let native_target = target.clone().unwrap_or_else(|| "focused".into());
                        let result = session
                            .action_with_revision(
                                SemanticAction::Type {
                                    target: native_target,
                                    text: text.clone(),
                                },
                                expected_revision,
                            )
                            .await
                            .map_err(browser_error)?;
                        self.revision = Some(result.revision);
                        Ok(native_action_value(
                            result,
                            "type",
                            target.as_deref(),
                            expected_revision,
                        ))
                    }
                    ResidentBrowserSession::Chromium(session) => {
                        let outcome = session
                            .type_text_with_expected_revision(
                                &text,
                                target.as_deref(),
                                Some(expected_revision),
                            )
                            .await
                            .map_err(browser_error)?;
                        self.revision = Some(outcome.current_revision);
                        serde_json::to_value(outcome).map_err(Into::into)
                    }
                }
            }
            BrowserCommand::Scroll {
                dx,
                dy,
                expected_revision,
            } => {
                match self.session()? {
                    ResidentBrowserSession::Native(session) => {
                        let delta_x = bounded_native_scroll_delta(dx, "dx")?;
                        let delta_y = bounded_native_scroll_delta(dy, "dy")?;
                        let result = session
                            .action_with_revision(
                                SemanticAction::Scroll { delta_x, delta_y },
                                expected_revision,
                            )
                            .await
                            .map_err(browser_error)?;
                        self.revision = Some(result.revision);
                        Ok(native_action_value(
                            result,
                            "scroll",
                            None,
                            expected_revision,
                        ))
                    }
                    ResidentBrowserSession::Chromium(session) => {
                        let outcome = session
                            .scroll_with_revision(dx, dy, Some(expected_revision))
                            .await
                            .map_err(browser_error)?;
                        self.revision = Some(outcome.current_revision);
                        serde_json::to_value(outcome).map_err(Into::into)
                    }
                }
            }
            BrowserCommand::Screenshot => {
                let png = match self.session()? {
                    ResidentBrowserSession::Native(session) => session
                        .native_capture_png_async()
                        .await
                        .map_err(browser_error)?,
                    ResidentBrowserSession::Chromium(session) => session
                        .screenshot_png()
                        .await
                        .map_err(browser_error)?,
                };
                Ok(serde_json::json!({
                    "mimeType":"image/png",
                    "bytes":png.len(),
                    "base64":base64::Engine::encode(&base64::engine::general_purpose::STANDARD, png)
                }))
            }
            BrowserCommand::RunWorkflow { definition, inputs } => {
                let workflow = WorkflowDefinition::from_value(definition)
                    .map_err(|error| DevelopmentError::InvalidInput(error.to_string()))?;
                self.workflow_state = "running".into();
                self.active_workflow = Some(workflow.name.clone());
                let result = match self.session()? {
                    ResidentBrowserSession::Native(session) => {
                        let policy = self.browser_policy()?;
                        session.native_run_workflow(&policy, &workflow, &inputs).await
                    }
                    ResidentBrowserSession::Chromium(session) => {
                        session.run_workflow(&workflow, &inputs).await
                    }
                };
                self.workflow_state = if result.is_ok() {
                    "completed"
                } else {
                    "failed"
                }
                .into();
                let result = result.map_err(browser_error)?;
                self.revision = Some(result.final_revision);
                self.last_workflow = Some((workflow, result.clone()));
                serde_json::to_value(result).map_err(Into::into)
            }
            BrowserCommand::PauseWorkflow => {
                let (workflow, result) = self.last_workflow.as_ref().ok_or_else(|| {
                    DevelopmentError::Conflict(
                        "no workflow result is available to checkpoint".into(),
                    )
                })?;
                let checkpoint = match self.session()? {
                    ResidentBrowserSession::Native(session) => session
                        .native_export_workflow_checkpoint(workflow, result)
                        .await
                        .map_err(browser_error)?,
                    ResidentBrowserSession::Chromium(session) => session
                        .export_workflow_checkpoint(workflow, result)
                        .await
                        .map_err(browser_error)?,
                };
                self.workflow_state = "paused".into();
                serde_json::to_value(checkpoint).map_err(Into::into)
            }
            BrowserCommand::ResumeWorkflow {
                definition,
                inputs,
                checkpoint,
            } => {
                let workflow = WorkflowDefinition::from_value(definition)
                    .map_err(|error| DevelopmentError::InvalidInput(error.to_string()))?;
                let checkpoint: WorkflowCheckpoint = serde_json::from_value(checkpoint)?;
                self.workflow_state = "running".into();
                self.active_workflow = Some(workflow.name.clone());
                let result = match self.session()? {
                    ResidentBrowserSession::Native(session) => {
                        let policy = self.browser_policy()?;
                        session
                            .native_resume_workflow(&policy, &workflow, &inputs, &checkpoint)
                            .await
                    }
                    ResidentBrowserSession::Chromium(session) => {
                        session.resume_workflow(&workflow, &inputs, &checkpoint).await
                    }
                };
                self.workflow_state = if result.is_ok() {
                    "completed"
                } else {
                    "failed"
                }
                .into();
                let result = result.map_err(browser_error)?;
                self.revision = Some(result.final_revision);
                self.last_workflow = Some((workflow, result.clone()));
                serde_json::to_value(result).map_err(Into::into)
            }
            BrowserCommand::ListWorkflows => Ok(serde_json::json!({
                "state":self.workflow_state,
                "active":self.active_workflow,
                "last":self.last_workflow.as_ref().map(|(definition, result)| serde_json::json!({
                    "name":definition.name,
                    "version":definition.workflow_version,
                    "result":result
                }))
            })),
            BrowserCommand::CancelWorkflow => {
                if self.workflow_state == "running" {
                    return Err(DevelopmentError::Conflict(
                        "workflow cancellation cannot preempt an in-flight synchronous browser command"
                            .into(),
                    ));
                }
                let previous = self.workflow_state.clone();
                self.workflow_state = "cancelled".into();
                self.active_workflow = None;
                Ok(serde_json::json!({"cancelled":true,"previousState":previous}))
            }
            BrowserCommand::VerifyPredicate { predicate, timeout } => {
                let outcome = match self.session()? {
                    ResidentBrowserSession::Native(session) => session
                        .native_verify(predicate, timeout)
                        .await
                        .map_err(browser_error)?,
                    ResidentBrowserSession::Chromium(session) => session
                        .verify(predicate, timeout)
                        .await
                        .map_err(browser_error)?,
                };
                serde_json::to_value(outcome).map_err(Into::into)
            }
            BrowserCommand::VerifyWorkflow => {
                let (definition, result) = self.last_workflow.as_ref().ok_or_else(|| {
                    DevelopmentError::Conflict("no workflow result is available to verify".into())
                })?;
                Ok(serde_json::json!({
                    "verified":self.workflow_state == "completed",
                    "name":definition.name,
                    "version":definition.workflow_version,
                    "finalRevision":result.final_revision,
                    "result":result
                }))
            }
            BrowserCommand::RemoteViewOpen => {
                if self.remote_view.is_some() {
                    return Err(DevelopmentError::Conflict("Remote View is already open".into()));
                }
                self.session()?;
                let view = RemoteView::bind().await?;
                let response = serde_json::json!({
                    "active":true,
                    "localUrl":view.local_url(),
                    "sshForwardHint":view.ssh_forward_hint(),
                    "loopbackOnly":true,
                });
                self.remote_view = Some(view);
                self.publish_remote_frame().await?;
                Ok(response)
            }
            BrowserCommand::RemoteViewStatus => Ok(self.remote_view.as_ref().map_or_else(
                || serde_json::json!({"active":false,"loopbackOnly":true}),
                |view| serde_json::json!({"active":true,"localUrl":view.local_url(),"sshForwardHint":view.ssh_forward_hint(),"loopbackOnly":true}),
            )),
            BrowserCommand::RemoteViewRevoke => {
                if let Some(view) = self.remote_view.take() {
                    view.revoke().await;
                    Ok(serde_json::json!({"revoked":true}))
                } else {
                    Ok(serde_json::json!({"revoked":false}))
                }
            }
        }
    }

    async fn publish_remote_frame(&self) -> DevelopmentResult<()> {
        let (Some(view), Some(session), Some(revision)) = (
            self.remote_view.as_ref(),
            self.session.as_ref(),
            self.revision,
        ) else {
            return Ok(());
        };
        let png = match session {
            ResidentBrowserSession::Native(session) => session
                .native_capture_png_async()
                .await
                .map_err(browser_error)?,
            ResidentBrowserSession::Chromium(session) => {
                session.screenshot_png().await.map_err(browser_error)?
            }
        };
        let published = view.publish(RemoteFrame {
            browser_revision: revision,
            mime_type: "image/png".into(),
            data: base64::Engine::encode(&base64::engine::general_purpose::STANDARD, png),
        });
        if !published {
            return Err(DevelopmentError::Process(
                "Remote View rejected the bounded frame".into(),
            ));
        }
        Ok(())
    }

    async fn apply_remote_inputs(&mut self) -> DevelopmentResult<()> {
        let mut inputs = Vec::new();
        if let Some(view) = self.remote_view.as_mut() {
            while let Ok(input) = view.try_recv_input() {
                inputs.push(input);
                if inputs.len() == 64 {
                    break;
                }
            }
        }
        for input in inputs {
            let revision = input.expected_revision();
            let current_revision = match (self.session()?, input) {
                (ResidentBrowserSession::Native(session), RemoteInput::Click { x, y, .. }) => {
                    let (width, height) = session.native_viewport_size().map_err(browser_error)?;
                    let result = session
                        .action_with_revision(
                            SemanticAction::Click {
                                target: format!("point={},{}", x * width, y * height),
                            },
                            revision,
                        )
                        .await
                        .map_err(browser_error)?;
                    result.revision
                }
                (ResidentBrowserSession::Native(session), RemoteInput::Scroll { dx, dy, .. }) => {
                    let result = session
                        .action_with_revision(
                            SemanticAction::Scroll {
                                delta_x: bounded_native_scroll_delta(dx, "dx")?,
                                delta_y: bounded_native_scroll_delta(dy, "dy")?,
                            },
                            revision,
                        )
                        .await
                        .map_err(browser_error)?;
                    result.revision
                }
                (ResidentBrowserSession::Native(session), RemoteInput::Key { key, .. }) => {
                    let result = session
                        .action_with_revision(SemanticAction::KeyPress { key }, revision)
                        .await
                        .map_err(browser_error)?;
                    result.revision
                }
                (ResidentBrowserSession::Native(session), RemoteInput::Text { text, .. }) => {
                    let result = session
                        .action_with_revision(
                            SemanticAction::Type {
                                target: "focused".into(),
                                text,
                            },
                            revision,
                        )
                        .await
                        .map_err(browser_error)?;
                    result.revision
                }
                (ResidentBrowserSession::Chromium(session), RemoteInput::Click { x, y, .. }) => {
                    let (width, height) = session.viewport_size().await.map_err(browser_error)?;
                    session
                        .click_at_with_revision(x * width, y * height, Some(revision))
                        .await
                        .map_err(browser_error)?
                        .revision
                }
                (ResidentBrowserSession::Chromium(session), RemoteInput::Scroll { dx, dy, .. }) => {
                    session
                        .scroll_with_revision(dx, dy, Some(revision))
                        .await
                        .map_err(browser_error)?
                        .current_revision
                }
                (ResidentBrowserSession::Chromium(session), RemoteInput::Key { key, .. }) => {
                    session
                        .key_press_with_revision(&key, Some(revision))
                        .await
                        .map_err(browser_error)?
                        .current_revision
                }
                (ResidentBrowserSession::Chromium(session), RemoteInput::Text { text, .. }) => {
                    session
                        .type_text_with_expected_revision(&text, None, Some(revision))
                        .await
                        .map_err(browser_error)?
                        .current_revision
                }
            };
            self.revision = Some(current_revision);
        }
        if self.remote_view.is_some() {
            self.publish_remote_frame().await?;
        }
        Ok(())
    }
}

fn browser_error(error: Box<dyn std::error::Error>) -> DevelopmentError {
    DevelopmentError::Process(error.to_string())
}

fn native_profile_storage_path(profile: &str) -> DevelopmentResult<PathBuf> {
    if profile.is_empty()
        || profile == "."
        || profile == ".."
        || profile.contains(['/', '\\', '\0'])
    {
        return Err(DevelopmentError::InvalidInput(
            "native browser profile must be a single safe name".into(),
        ));
    }
    let config_home = std::env::var_os("GLASS_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(dirs::config_dir)
        .unwrap_or_else(|| PathBuf::from("."));
    Ok(config_home
        .join("glass")
        .join("native-profiles")
        .join(profile)
        .join("storage.json"))
}

fn bounded_native_scroll_delta(value: f64, field: &str) -> DevelopmentResult<i32> {
    if !value.is_finite() || value < f64::from(i32::MIN) || value > f64::from(i32::MAX) {
        return Err(DevelopmentError::InvalidInput(format!(
            "native browser scroll {field} must be a finite i32-sized number"
        )));
    }
    Ok(value as i32)
}

fn native_action_value(
    result: ActionResult,
    action: &str,
    target: Option<&str>,
    previous_revision: u64,
) -> Value {
    serde_json::json!({
        "status": if result.accepted { "succeeded" } else { "not_executed" },
        "action": action,
        "executionId": format!("act_native_resident_{}", result.revision),
        "target": target.map(|target| serde_json::json!({"label":target,"reference":target})),
        "revision": result.revision,
        "previousRevision": previous_revision,
        "currentRevision": result.revision,
        "browserRevision": result.revision,
        "contextId": result.context_id,
        "accepted": result.accepted,
        "backend": "native"
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::{TcpListener, TcpStream};
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};

    #[test]
    fn disconnected_service_reports_authoritative_state() {
        let service = BrowserService::new(std::env::temp_dir()).unwrap();
        let state = service.state().unwrap();
        assert_eq!(state["connected"], false);
        assert_eq!(state["workflowState"], "idle");
        let error = service.observe().unwrap_err();
        assert!(error.to_string().contains("glass.browser.start"));
        assert_eq!(service.remote_view_status().unwrap()["active"], false);
        let web_ir_error = service.web_ir().unwrap_err();
        assert!(web_ir_error.to_string().contains("glass.browser.start"));
        assert!(
            service
                .open_remote_view()
                .unwrap_err()
                .to_string()
                .contains("browser")
        );
        assert_eq!(service.revoke_remote_view().unwrap()["revoked"], false);
    }

    #[test]
    fn explicit_browser_policy_is_exposed_before_startup() {
        let service =
            BrowserService::new_with_policy(std::env::temp_dir(), PolicyPreset::Hardened).unwrap();
        let state = service.state().unwrap();
        assert_eq!(state["policyPreset"], "hardened");
        assert_eq!(state["connected"], false);
    }

    #[test]
    fn resident_browser_defaults_to_native_runtime() {
        let service = BrowserService::new(std::env::temp_dir()).unwrap();
        let started = service.start(BrowserStartConfig::default()).unwrap();
        assert_eq!(started["connected"], true);
        assert_eq!(started["browserBackend"], "native");
        assert!(started["browserProcessId"].is_null());

        let observation = service.observe().unwrap();
        assert_eq!(observation["level"], "structured");
        assert!(observation["page"].is_object());
        let web_ir = service.web_ir().unwrap();
        assert_eq!(web_ir["schemaVersion"], 1);
        assert!(web_ir["entityCount"].as_u64().is_some());

        let stopped = service.stop().unwrap();
        assert_eq!(stopped["connected"], false);
    }

    #[test]
    fn native_dialog_control_is_opt_in_and_rejects_chromium_sessions() {
        let service = BrowserService::new(std::env::temp_dir()).unwrap();
        assert!(!BrowserStartConfig::default().modal_dialogs);
        assert!(!service.native_dialog_control_enabled().unwrap());
        assert!(service.pending_native_dialog().is_err());
        assert!(
            service
                .resolve_native_dialog(
                    "native-dialog-1",
                    NativeDialogResolution {
                        accepted: true,
                        prompt_value: None,
                    },
                )
                .is_err()
        );

        let error = service
            .start(BrowserStartConfig {
                attach: true,
                modal_dialogs: true,
                ..BrowserStartConfig::default()
            })
            .unwrap_err();
        assert!(
            error
                .to_string()
                .contains("only by the native browser runtime")
        );
        assert_eq!(service.state().unwrap()["connected"], false);
    }

    #[test]
    fn resident_native_dialog_controller_resolves_suspended_navigation_out_of_band() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let address = listener.local_addr().unwrap();
        let running = Arc::new(AtomicBool::new(true));
        let server_running = Arc::clone(&running);
        let server = std::thread::spawn(move || {
            let baseline_html: &[u8] =
                br#"<!doctype html><title>Baseline service</title><body>Initial</body><script>
                document.body.textContent = 'baseline mutation';
            </script>"#;
            let dialog_html: &[u8] =
                br#"<!doctype html><title>Dialog service</title><body></body><script>
                const accepted = confirm('Continue?');
                const name = prompt('Your name?', 'Ada');
                document.body.textContent = `${accepted}|${name}`;
            </script>"#;
            let alert_html: &[u8] = br#"<!doctype html><title>Alert service</title><script>
                alert('Dialog flow complete');
            </script>"#;
            while server_running.load(Ordering::Relaxed) {
                match listener.accept() {
                    Ok((mut stream, _)) => {
                        let mut request = [0; 4096];
                        let received = stream.read(&mut request).unwrap_or_default();
                        let request = String::from_utf8_lossy(&request[..received]);
                        let path = request
                            .lines()
                            .next()
                            .and_then(|line| line.split_whitespace().nth(1));
                        let html = match path {
                            Some("/baseline") => baseline_html,
                            Some("/alert") => alert_html,
                            _ => dialog_html,
                        };
                        let header = format!(
                            "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                            html.len()
                        );
                        let _ = stream.write_all(header.as_bytes());
                        let _ = stream.write_all(html);
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        std::thread::sleep(Duration::from_millis(5));
                    }
                    Err(_) => break,
                }
            }
        });

        let root = std::env::temp_dir().join(format!(
            "glass-native-dialog-tui-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let mut tui = crate::tui::DevTuiState::open_for_tui(
            &root,
            glass_browser::cli::args::TuiLayout::Desktop,
        )
        .unwrap();
        let service = tui.browser_service.clone();
        let started = service
            .start(BrowserStartConfig {
                modal_dialogs: true,
                ..BrowserStartConfig::default()
            })
            .unwrap();
        assert!(service.native_dialog_control_enabled().unwrap());
        let revision = started["browserRevision"].as_u64().unwrap();
        let baseline_result = service
            .navigate(
                format!("http://{address}/baseline"),
                revision,
                Duration::from_secs(30),
            )
            .unwrap();
        assert_eq!(baseline_result["backend"], "native");
        assert_eq!(
            service.observe().unwrap()["text"],
            "baseline mutation",
            "the non-modal control flow must publish its DOM mutation"
        );
        let revision = service.state().unwrap()["browserRevision"]
            .as_u64()
            .unwrap();
        let navigation_service = service.clone();
        let navigation = std::thread::spawn(move || {
            navigation_service.navigate(
                format!("http://{address}/dialogs"),
                revision,
                Duration::from_secs(30),
            )
        });

        let wait_for_dialog = |expected_type: &str| {
            let deadline = std::time::Instant::now() + Duration::from_secs(15);
            loop {
                if let Some(dialog) = service.pending_native_dialog().unwrap() {
                    assert_eq!(dialog.dialog.dialog_type, expected_type);
                    return dialog;
                }
                assert!(
                    !navigation.is_finished(),
                    "navigation completed before the {expected_type} dialog was resolved"
                );
                assert!(
                    std::time::Instant::now() < deadline,
                    "timed out waiting for the {expected_type} dialog"
                );
                std::thread::sleep(Duration::from_millis(10));
            }
        };

        let confirm = wait_for_dialog("confirm");
        let stale = service.resolve_native_dialog(
            "native-dialog-stale",
            NativeDialogResolution {
                accepted: true,
                prompt_value: None,
            },
        );
        assert!(stale.is_err());
        assert_eq!(
            service.pending_native_dialog().unwrap().unwrap().id,
            confirm.id
        );
        let shared_workspace = tui.workspace.clone();
        let workspace_guard = shared_workspace.lock().unwrap();
        assert!(tui.poll_native_browser_dialog());
        assert_eq!(tui.browser_dialog.as_ref().unwrap().pending.id, confirm.id);
        tui.handle_native_browser_dialog_key(
            crossterm::event::KeyCode::Char('Y'),
            crossterm::event::KeyModifiers::NONE,
        );
        drop(workspace_guard);
        assert!(tui.browser_dialog.is_none());

        let prompt = wait_for_dialog("prompt");
        assert!(tui.poll_native_browser_dialog());
        assert_eq!(tui.browser_dialog.as_ref().unwrap().pending.id, prompt.id);
        assert_eq!(tui.browser_dialog.as_ref().unwrap().prompt_input, "Ada");
        tui.handle_native_browser_dialog_key(
            crossterm::event::KeyCode::Char('G'),
            crossterm::event::KeyModifiers::NONE,
        );
        assert_eq!(tui.browser_dialog.as_ref().unwrap().prompt_input, "AdaG");
        tui.handle_native_browser_dialog_key(
            crossterm::event::KeyCode::Enter,
            crossterm::event::KeyModifiers::NONE,
        );
        assert!(tui.browser_dialog.is_none());

        let navigation_result = navigation.join().unwrap().unwrap();
        assert_eq!(navigation_result["backend"], "native");
        assert_eq!(
            navigation_result["url"],
            format!("http://{address}/dialogs")
        );
        assert_eq!(service.observe().unwrap()["text"], "true|AdaG");

        let revision = service.state().unwrap()["browserRevision"]
            .as_u64()
            .unwrap();
        let alert_service = service.clone();
        let alert_navigation = std::thread::spawn(move || {
            alert_service.navigate(
                format!("http://{address}/alert"),
                revision,
                Duration::from_secs(30),
            )
        });
        let alert_deadline = std::time::Instant::now() + Duration::from_secs(15);
        let alert = loop {
            if let Some(dialog) = service.pending_native_dialog().unwrap() {
                break dialog;
            }
            assert!(
                !alert_navigation.is_finished(),
                "alert navigation completed before the alert dialog was resolved"
            );
            assert!(
                std::time::Instant::now() < alert_deadline,
                "timed out waiting for the alert dialog"
            );
            std::thread::sleep(Duration::from_millis(10));
        };
        assert_eq!(alert.dialog.dialog_type, "alert");
        assert!(tui.poll_native_browser_dialog());
        assert_eq!(tui.browser_dialog.as_ref().unwrap().pending.id, alert.id);
        tui.request_quit();
        tui.confirm_quit();
        assert!(tui.quit);
        assert!(tui.browser_dialog.is_none());
        let alert_result = alert_navigation.join().unwrap().unwrap();
        assert_eq!(alert_result["backend"], "native");
        assert_eq!(alert_result["url"], format!("http://{address}/alert"));

        assert_eq!(service.stop().unwrap()["connected"], false);
        assert!(!service.native_dialog_control_enabled().unwrap());
        assert!(service.pending_native_dialog().is_err());

        running.store(false, Ordering::Relaxed);
        server.join().unwrap();
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn resident_browser_executes_revision_safe_fixture_flow() {
        if std::env::var("GLASS_E2E").as_deref() != Ok("1") {
            return;
        }
        let chrome_path =
            std::env::var("GLASS_CHROME_PATH").expect("GLASS_E2E=1 requires GLASS_CHROME_PATH");
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let address = listener.local_addr().unwrap();
        let running = Arc::new(AtomicBool::new(true));
        let server_running = Arc::clone(&running);
        let server = std::thread::spawn(move || {
            let html = br#"<!doctype html><title>Resident Glass</title><label>Name<input id="name"></label><button id="save" onclick="document.querySelector('p').textContent='Saved '+document.querySelector('#name').value">Save</button><p></p>"#;
            while server_running.load(Ordering::Relaxed) {
                match listener.accept() {
                    Ok((mut stream, _)) => {
                        let mut request = [0; 4096];
                        let _ = stream.read(&mut request);
                        let header = format!(
                            "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                            html.len()
                        );
                        let _ = stream.write_all(header.as_bytes());
                        let _ = stream.write_all(html);
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        std::thread::sleep(Duration::from_millis(5));
                    }
                    Err(_) => break,
                }
            }
        });
        let port_probe = TcpListener::bind("127.0.0.1:0").unwrap();
        let browser_port = port_probe.local_addr().unwrap().port();
        drop(port_probe);

        let service = BrowserService::new(std::env::temp_dir()).unwrap();
        let started = service
            .start(BrowserStartConfig {
                port: browser_port,
                attach: true,
                chrome_path: Some(chrome_path.into()),
                ..BrowserStartConfig::default()
            })
            .unwrap();
        let browser_pid = started["browserProcessId"].as_u64().unwrap() as u32;
        let initial = service.observe().unwrap();
        let initial_revision = initial["consistency"]["end_revision"]
            .as_u64()
            .or_else(|| initial["consistency"]["endRevision"].as_u64())
            .unwrap();
        service
            .navigate(
                format!("http://{address}/fixture.html"),
                initial_revision,
                Duration::from_secs(30),
            )
            .unwrap();
        assert!(
            service
                .navigate(
                    format!("http://{address}/stale"),
                    initial_revision,
                    Duration::from_secs(30),
                )
                .unwrap_err()
                .to_string()
                .contains("revision")
        );
        let observed = service.observe().unwrap();
        assert_eq!(observed["page"]["title"], "Resident Glass");
        assert!(
            service
                .snapshot()
                .unwrap()
                .as_object()
                .is_some_and(|snapshot| !snapshot.is_empty())
        );
        assert_eq!(
            service
                .semantic(SemanticObservationLevel::Structured)
                .unwrap()["page"]["title"],
            "Resident Glass"
        );
        let web_ir = service.web_ir().unwrap();
        assert!(web_ir["revision"].as_u64().is_some());
        assert!(web_ir["entityCount"].as_u64().unwrap_or_default() > 0);
        assert!(web_ir["actionableEntities"].is_array());
        assert!(service.diff().unwrap()["toRevision"].as_u64().is_some());
        let revision = observed["consistency"]["end_revision"]
            .as_u64()
            .or_else(|| observed["consistency"]["endRevision"].as_u64())
            .unwrap();
        let typed = service
            .type_text("Ada".into(), Some("Name".into()), revision)
            .unwrap();
        let revision = typed["currentRevision"].as_u64().unwrap();
        service.click("Save".into(), revision).unwrap();
        assert!(
            service.observe().unwrap()["text"]
                .as_str()
                .unwrap()
                .contains("Saved Ada")
        );
        let remote = service.open_remote_view().unwrap();
        assert_eq!(remote["active"], true);
        assert!(
            remote["localUrl"]
                .as_str()
                .unwrap()
                .starts_with("http://127.0.0.1:")
        );
        assert_eq!(service.remote_view_status().unwrap()["active"], true);
        assert_eq!(service.revoke_remote_view().unwrap()["revoked"], true);
        let observed = service.observe().unwrap();
        let revision = observed["consistency"]["end_revision"]
            .as_u64()
            .or_else(|| observed["consistency"]["endRevision"].as_u64())
            .unwrap();
        assert_eq!(service.reload(revision).unwrap()["action"], "reload");
        service.stop().unwrap();
        #[cfg(unix)]
        assert!(unsafe { libc::kill(browser_pid as i32, 0) } == -1);
        let port_closed = (0..100).any(|_| {
            if TcpStream::connect(("127.0.0.1", browser_port)).is_err() {
                true
            } else {
                std::thread::sleep(Duration::from_millis(10));
                false
            }
        });
        assert!(port_closed, "owned browser endpoint survived resident stop");
        running.store(false, Ordering::Relaxed);
        server.join().unwrap();
    }
}
