//! Semantic backend adapter for the Glass-owned native engine.
//!
//! The adapter is intentionally small: the native engine owns DOM and
//! lifecycle state, while this module translates only the stable
//! `browser_backend` contract.
//!
//! The synchronous guards intentionally span content-worker awaits. The
//! backend's operation boundary keeps the active target, frame registry, and
//! engine transactionally aligned; splitting these guards would permit a
//! second request to observe or mutate a half-reconciled frame topology.
#![allow(clippy::await_holding_lock)]

use super::native_engine::NativeResourceLoader;
use super::native_engine::{
    MAX_NATIVE_COOKIE_PROFILE_ENTRIES, MAX_NATIVE_EFFECTS, MAX_NATIVE_VIEWPORT_DIMENSION,
    NativeAction, NativeCookieChange, NativeDialogControlPlane, NativeDialogController,
    NativeEffect, NativeEngine, NativeEngineConfig, NativeEngineError, NativeEventKind, NativeFile,
    NativeFrameScriptBinding, NativeFrameScriptContext, NativeFrameScriptRequest,
    NativeFrameScriptWindow, NativeHistoryDirection, NativeInspectionSnapshot,
    NativeLayoutSnapshot, NativeNavigationCancellation, NativeNavigationMethod,
    NativeNavigationRequest, NativeNodeSubtreeTransfer, NativeOrigin, NativePageMessagePortCommand,
    NativePendingDialog, NativePoint, NativePopupRequest, NativePostMessageRequest,
    NativePreflightAction, NativeRequestBody, NativeScriptCommand,
    NativeServiceWorkerClientMessage, NativeServiceWorkerOpenWindowRequest,
    NativeSharedWorkerCreateRequest, NativeSharedWorkerStorageKey, NativeSurface,
    NativeTargetPreflight, NativeWindowCloseRequest, NativeWindowNavigationRequest,
    NativeWindowProxyUpdate, NativeWorkerMessage, NativeWorkerRegistry, Viewport,
    parse_point_target, synchronize_service_worker_client_leases, validate_message_port_transfers,
    validate_page_message_port_command, validate_target_navigation_payload,
};
use crate::browser::session::{
    FrameInfo, GeoLocation, NavigationControlOutcome, NetworkConditions, PageTargetInfo,
    VisualCapture, VisualCaptureMetadata, VisualCaptureOptions, VisualClip, VisualFormat,
    redact_diagnostic_text, redact_diagnostic_url,
};
use crate::browser_backend::MAX_BACKEND_ID_BYTES;
use crate::browser_backend::{
    ActionResult, BROWSER_BACKEND_SCHEMA_VERSION, BackendFuture, BackendOperation, BackendProfile,
    BackendRequest, BackendResponse, BrowserBackend, BrowserBackendError, BrowserCapability,
    BrowsingContext, CapabilityDescriptor, CaptureFormat, CaptureResult, CertificationLevel,
    CertificationProfile, DownloadOperation, DownloadResult, EffectsResult, EvidenceLevel,
    EvidenceResult, NavigationResult, Portability, PromptDecision, PromptResult, ScriptResult,
    SemanticAction, StorageResult, SupportLevel,
};
use base64::Engine as _;
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::sync::{Mutex, MutexGuard};

/// Stable backend ID for the Glass-owned native engine.
pub const NATIVE_ENGINE_BACKEND_ID: &str = "native-engine";
const NATIVE_ENGINE_BACKEND_VERSION: &str = "0.1";
const NATIVE_ENGINE_BROWSER_FAMILY: &str = "native";

const NATIVE_MAX_TARGETS: usize = crate::browser::session::TOPOLOGY_MAX_TARGETS;
const NATIVE_MAX_FRAMES: usize = crate::browser::session::TOPOLOGY_MAX_FRAMES;
const NATIVE_MAX_CLOSED_TARGETS: usize = NATIVE_MAX_TARGETS * 4;
const NATIVE_MAX_MESSAGE_PORT_ROUTES: usize = NATIVE_MAX_TARGETS * MAX_NATIVE_EFFECTS;

/// Native visual capture is encoded by the software renderer. The option
/// shape intentionally matches the shared visual contract so the CLI and
/// runtime surfaces cannot silently discard geometry or format options.
const NATIVE_CAPTURE_MAX_AXIS: u32 = 16_384;
const NATIVE_CAPTURE_MAX_PIXELS: usize = 4 * 1024 * 1024;

struct NativeParkedFrame {
    engine: NativeEngine,
    parent_id: Option<String>,
    owner_node_index: Option<u32>,
    sandboxed_modals: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct NativeFrameDiscoveryStamp {
    document_generation: u32,
    document_revision: u64,
}

struct NativeRetiredFrame {
    frame_id: String,
    engine: NativeEngine,
}

#[derive(Clone)]
enum NativeFrameRoute {
    ActiveSelected,
    ActiveParked,
    ParkedSelected { target_id: String },
    ParkedParked { target_id: String },
}

#[derive(Debug, Clone)]
struct NativePageMessagePortRoute {
    context_id: String,
    frame_id: String,
}

#[derive(Debug, Clone)]
struct NativeSharedWorkerPageRoute {
    context_id: String,
    frame_id: String,
    document_generation: u32,
}

enum NativeWorkerCoordinatorEffect {
    PageMessagePort(NativePageMessagePortCommand),
    SharedWorkerMessage(NativePageMessagePortCommand),
    SharedWorkerCreate(NativeSharedWorkerCreateRequest),
    SharedWorkerError {
        route: NativeSharedWorkerPageRoute,
        message: NativeWorkerMessage,
    },
    SharedWorkerCookieChanges {
        changes: Vec<NativeCookieChange>,
    },
}

struct NativeSharedWorkerCoordinator {
    registry: NativeWorkerRegistry,
    loader: NativeResourceLoader,
    page_ports: BTreeMap<String, NativeSharedWorkerPageRoute>,
    cookie_overrides: BTreeMap<(String, String, String), NativeCookieChange>,
    next_connection_id: u64,
}

impl NativeSharedWorkerCoordinator {
    fn new(loader: NativeResourceLoader) -> Self {
        Self {
            registry: NativeWorkerRegistry::new_with_fetch_streams(),
            loader,
            page_ports: BTreeMap::new(),
            cookie_overrides: BTreeMap::new(),
            next_connection_id: 1,
        }
    }

    fn replay_cookie_overrides(&mut self) -> Result<(), NativeEngineError> {
        let changes = self.cookie_overrides.values().cloned().collect::<Vec<_>>();
        self.loader.apply_cookie_changes(&changes)
    }

    fn remember_cookie_changes(&mut self) -> Result<Vec<NativeCookieChange>, NativeEngineError> {
        let changes = self.loader.take_cookie_changes();
        for change in &changes {
            let key = (
                change.name.clone(),
                change.domain.clone(),
                change.path.clone(),
            );
            if !self.cookie_overrides.contains_key(&key)
                && self.cookie_overrides.len() >= MAX_NATIVE_COOKIE_PROFILE_ENTRIES
            {
                return Err(NativeEngineError::limit(
                    "native SharedWorker cookie change entries",
                    MAX_NATIVE_COOKIE_PROFILE_ENTRIES,
                    self.cookie_overrides.len().saturating_add(1),
                ));
            }
            self.cookie_overrides.insert(key, change.clone());
        }
        Ok(changes)
    }
}

async fn apply_native_cookie_changes_to_live_engine(
    engine: &mut NativeEngine,
    changes: &[NativeCookieChange],
    profile_written: &mut bool,
) -> Result<(), NativeEngineError> {
    if *profile_written {
        engine.apply_cookie_changes_to_runtime_async(changes).await
    } else {
        engine.apply_cookie_changes_async(changes).await?;
        *profile_written = true;
        Ok(())
    }
}

struct ActiveNativeNavigation {
    id: u64,
    starting_revision: u64,
    cancellation: NativeNavigationCancellation,
}

struct NativeNavigationControlState {
    next_id: u64,
    active: Option<ActiveNativeNavigation>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum NativeBrowserEffectSource {
    Popup,
    Message,
    Navigation,
    Close,
    ServiceWorkerOpenWindow,
    ServiceWorkerClientMessage,
    PageMessagePort,
}

impl NativeBrowserEffectSource {
    fn next(self) -> Self {
        match self {
            Self::Popup => Self::Message,
            Self::Message => Self::Navigation,
            Self::Navigation => Self::Close,
            Self::Close => Self::ServiceWorkerOpenWindow,
            Self::ServiceWorkerOpenWindow => Self::ServiceWorkerClientMessage,
            Self::ServiceWorkerClientMessage => Self::PageMessagePort,
            Self::PageMessagePort => Self::Popup,
        }
    }
}

fn next_ready_native_browser_effect_source(
    cursor: &mut NativeBrowserEffectSource,
    popup_ready: bool,
    message_ready: bool,
    navigation_ready: bool,
    close_ready: bool,
    service_worker_open_window_ready: bool,
    service_worker_client_message_ready: bool,
    page_message_port_ready: bool,
) -> Option<NativeBrowserEffectSource> {
    for _ in 0..7 {
        let candidate = *cursor;
        *cursor = candidate.next();
        let ready = match candidate {
            NativeBrowserEffectSource::Popup => popup_ready,
            NativeBrowserEffectSource::Message => message_ready,
            NativeBrowserEffectSource::Navigation => navigation_ready,
            NativeBrowserEffectSource::Close => close_ready,
            NativeBrowserEffectSource::ServiceWorkerOpenWindow => service_worker_open_window_ready,
            NativeBrowserEffectSource::ServiceWorkerClientMessage => {
                service_worker_client_message_ready
            }
            NativeBrowserEffectSource::PageMessagePort => page_message_port_ready,
        };
        if ready {
            return Some(candidate);
        }
    }
    None
}

struct NativeFrameState {
    active_frame_id: String,
    active_parent_id: Option<String>,
    active_owner_node_index: Option<u32>,
    active_sandboxed_modals: bool,
    focused_frame_id: Option<String>,
    parked: BTreeMap<String, NativeParkedFrame>,
    next_frame_number: u64,
    discovered_generation: Option<u32>,
    discovered_documents: BTreeMap<String, NativeFrameDiscoveryStamp>,
    observed_frame_owner_removal_sequences: BTreeMap<(String, u32, u32), u64>,
    pending_retired: Vec<NativeRetiredFrame>,
}

impl NativeFrameState {
    fn new(target_id: &str) -> Self {
        Self {
            active_frame_id: native_main_frame_id(target_id),
            active_parent_id: None,
            active_owner_node_index: None,
            active_sandboxed_modals: false,
            focused_frame_id: None,
            parked: BTreeMap::new(),
            next_frame_number: 1,
            discovered_generation: None,
            discovered_documents: BTreeMap::new(),
            observed_frame_owner_removal_sequences: BTreeMap::new(),
            pending_retired: Vec::new(),
        }
    }

    fn empty() -> Self {
        Self {
            active_frame_id: String::new(),
            active_parent_id: None,
            active_owner_node_index: None,
            active_sandboxed_modals: false,
            focused_frame_id: None,
            parked: BTreeMap::new(),
            next_frame_number: 1,
            discovered_generation: None,
            discovered_documents: BTreeMap::new(),
            observed_frame_owner_removal_sequences: BTreeMap::new(),
            pending_retired: Vec::new(),
        }
    }

    fn frame_count(&self) -> usize {
        self.parked.len() + usize::from(!self.active_frame_id.is_empty())
    }

    fn next_frame_id(&mut self, target_id: &str) -> String {
        loop {
            let number = self.next_frame_number;
            self.next_frame_number = self.next_frame_number.saturating_add(1);
            let candidate = format!("{target_id}:frame-{number}");
            if self.active_frame_id != candidate && !self.parked.contains_key(&candidate) {
                return candidate;
            }
        }
    }

    fn descendant_ids(&self, ancestor_id: &str) -> Vec<String> {
        let mut descendants = Vec::new();
        let mut parents = vec![ancestor_id.to_owned()];
        while let Some(parent_id) = parents.pop() {
            for (frame_id, frame) in &self.parked {
                if frame.parent_id.as_deref() == Some(parent_id.as_str())
                    && !descendants.contains(frame_id)
                {
                    descendants.push(frame_id.clone());
                    parents.push(frame_id.clone());
                }
            }
        }
        descendants
    }

    fn is_descendant(&self, frame_id: &str, ancestor_id: &str) -> bool {
        if frame_id == ancestor_id {
            return false;
        }
        let mut current = frame_id.to_owned();
        for _ in 0..NATIVE_MAX_FRAMES {
            let parent = if current == self.active_frame_id {
                self.active_parent_id.clone()
            } else {
                self.parked
                    .get(&current)
                    .and_then(|frame| frame.parent_id.clone())
            };
            let Some(parent) = parent else {
                return false;
            };
            if parent == ancestor_id {
                return true;
            }
            current = parent;
        }
        false
    }
}

struct NativeParkedTarget {
    engine: NativeEngine,
    opener_id: Option<String>,
    frames: NativeFrameState,
    name: Option<String>,
}

#[derive(Debug, Clone)]
struct NativeClosedTarget {
    url: String,
    name: String,
}

struct NativeTargetState {
    active_target_id: Option<String>,
    active_opener_id: Option<String>,
    active_name: Option<String>,
    active_frames: NativeFrameState,
    parked: BTreeMap<String, NativeParkedTarget>,
    closed: BTreeMap<String, NativeClosedTarget>,
    window_handles: BTreeMap<(String, String), String>,
    next_target_number: u64,
}

fn native_frame_route_in_state(
    targets: &NativeTargetState,
    frame_id: &str,
) -> Option<NativeFrameRoute> {
    if targets.active_frames.active_frame_id == frame_id {
        return Some(NativeFrameRoute::ActiveSelected);
    }
    if targets.active_frames.parked.contains_key(frame_id) {
        return Some(NativeFrameRoute::ActiveParked);
    }
    for (target_id, target) in &targets.parked {
        if target.frames.active_frame_id == frame_id {
            return Some(NativeFrameRoute::ParkedSelected {
                target_id: target_id.clone(),
            });
        }
        if target.frames.parked.contains_key(frame_id) {
            return Some(NativeFrameRoute::ParkedParked {
                target_id: target_id.clone(),
            });
        }
    }
    None
}

fn native_frame_route_owner_id<'a>(
    targets: &'a NativeTargetState,
    route: &'a NativeFrameRoute,
) -> Option<&'a str> {
    match route {
        NativeFrameRoute::ActiveSelected | NativeFrameRoute::ActiveParked => {
            targets.active_target_id.as_deref()
        }
        NativeFrameRoute::ParkedSelected { target_id }
        | NativeFrameRoute::ParkedParked { target_id } => Some(target_id),
    }
}

async fn coordinate_native_document_adoption(
    source: &mut NativeEngine,
    destination: &mut NativeEngine,
    source_frame_id: &str,
    destination_frame_id: &str,
    source_generation: u32,
    destination_generation: u32,
    transfers: &[NativeNodeSubtreeTransfer],
) -> Result<(), BrowserBackendError> {
    if source.frame_owner_id() != source_frame_id
        || destination.frame_owner_id() != destination_frame_id
        || source_frame_id == destination_frame_id
        || source.document_generation().map_err(native_error)? != source_generation
        || destination.document_generation().map_err(native_error)? != destination_generation
    {
        return Err(BrowserBackendError::SelectionFailed {
            reason: "native node transfer owner or document generation is stale".into(),
        });
    }

    let source_before = source.document_transfer_draft();
    let destination_before = destination.document_transfer_draft();
    let mut source_candidate = source_before.clone();
    let mut destination_candidate = destination_before.clone();
    source_candidate
        .transfer_script_nodes_to(&mut destination_candidate, transfers)
        .map_err(native_error)?;
    source
        .validate_document_transfer_candidate(&source_candidate)
        .map_err(native_error)?;
    destination
        .validate_document_transfer_candidate(&destination_candidate)
        .map_err(native_error)?;

    let source_sync = source
        .synchronize_document_transfer_snapshot(&source_candidate)
        .await;
    let destination_sync = if source_sync.is_ok() {
        destination
            .synchronize_document_transfer_snapshot(&destination_candidate)
            .await
    } else {
        Ok(())
    };
    if let Err(error) = source_sync.and(destination_sync) {
        let source_restore = source
            .synchronize_document_transfer_snapshot(&source_before)
            .await;
        let destination_restore = destination
            .synchronize_document_transfer_snapshot(&destination_before)
            .await;
        if source_restore.is_err() || destination_restore.is_err() {
            return Err(BrowserBackendError::Lifecycle {
                operation: "same-origin node adoption".into(),
                state: "worker-rollback-failed".into(),
                reason: format!(
                    "transfer failed ({error}); restoring both content workers also failed"
                ),
            });
        }
        return Err(native_error(error));
    }

    source.publish_document_transfer_snapshot(source_candidate);
    destination.publish_document_transfer_snapshot(destination_candidate);
    Ok(())
}

impl NativeTargetState {
    fn new(active_target_id: String, active_name: Option<String>) -> Self {
        Self {
            active_target_id: Some(active_target_id.clone()),
            active_opener_id: None,
            active_name,
            active_frames: NativeFrameState::new(&active_target_id),
            parked: BTreeMap::new(),
            closed: BTreeMap::new(),
            window_handles: BTreeMap::new(),
            next_target_number: 1,
        }
    }

    fn target_count(&self) -> usize {
        self.parked.len() + usize::from(self.active_target_id.is_some())
    }

    fn next_target_id(&mut self) -> String {
        loop {
            let number = self.next_target_number;
            self.next_target_number = self.next_target_number.saturating_add(1);
            let candidate = format!("native-target-{number}");
            if self.active_target_id.as_deref() != Some(candidate.as_str())
                && !self.parked.contains_key(&candidate)
            {
                return candidate;
            }
        }
    }

    fn remember_closed_target(&mut self, target_id: String, url: String, name: String) {
        if self.closed.len() >= NATIVE_MAX_CLOSED_TARGETS
            && !self.closed.contains_key(&target_id)
            && let Some(oldest) = self.closed.keys().next().cloned()
        {
            self.closed.remove(&oldest);
        }
        self.closed
            .insert(target_id, NativeClosedTarget { url, name });
    }
}

/// Semantic adapter around the native browser's target-owned engine pool.
pub struct NativeEngineBackend {
    profile: BackendProfile,
    engine: Mutex<NativeEngine>,
    dialog_control: NativeDialogControlPlane,
    targets: Mutex<NativeTargetState>,
    shared_workers: Mutex<NativeSharedWorkerCoordinator>,
    browser_effect_cursor: Mutex<NativeBrowserEffectSource>,
    page_message_port_routes: Mutex<BTreeMap<String, NativePageMessagePortRoute>>,
    navigation_control: Mutex<NativeNavigationControlState>,
}

/// One atomic semantic/layout snapshot together with the frame that owns it.
/// The root frame is returned first, followed by attached descendants in
/// deterministic tree order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NativeFrameInspectionSnapshot {
    pub frame_id: String,
    pub parent_id: Option<String>,
    pub inspection: NativeInspectionSnapshot,
}

impl NativeEngineBackend {
    pub fn new(config: NativeEngineConfig) -> Result<Self, BrowserBackendError> {
        Self::new_with_dialog_control(config, NativeDialogControlPlane::default())
    }

    pub(crate) fn new_with_dialog_control(
        config: NativeEngineConfig,
        dialog_control: NativeDialogControlPlane,
    ) -> Result<Self, BrowserBackendError> {
        let profile = Self::profile_for(env!("CARGO_PKG_VERSION"))?;
        let active_target_id = config.context_id.clone();
        let engine = NativeEngine::new_with_browser_shared_workers(config, dialog_control.clone())
            .map_err(native_error)?;
        let shared_workers = NativeSharedWorkerCoordinator::new(engine.clone_resource_loader());
        let active_name = native_window_name(&engine.config().window_name);
        Ok(Self {
            profile,
            engine: Mutex::new(engine),
            dialog_control,
            targets: Mutex::new(NativeTargetState::new(active_target_id, active_name)),
            shared_workers: Mutex::new(shared_workers),
            browser_effect_cursor: Mutex::new(NativeBrowserEffectSource::Popup),
            page_message_port_routes: Mutex::new(BTreeMap::new()),
            navigation_control: Mutex::new(NativeNavigationControlState {
                next_id: 1,
                active: None,
            }),
        })
    }

    pub(crate) fn current_revision(&self) -> Result<u64, BrowserBackendError> {
        Ok(self.lock_engine(BackendOperation::Evidence)?.revision())
    }

    pub(crate) fn pending_dialog_control(
        &self,
    ) -> Result<Option<NativePendingDialog>, BrowserBackendError> {
        self.dialog_control.pending().map_err(native_error)
    }

    pub fn native_dialog_controller(&self) -> Result<NativeDialogController, BrowserBackendError> {
        NativeDialogController::new(self.dialog_control.clone()).map_err(native_error)
    }

    pub(crate) fn resolve_dialog_control(
        &self,
        id: &str,
        accepted: bool,
        prompt_value: Option<String>,
    ) -> Result<Option<u64>, BrowserBackendError> {
        self.dialog_control
            .resolve(id, accepted, prompt_value)
            .map_err(native_error)
    }

    pub(crate) fn begin_navigation_control(
        &self,
        starting_revision: u64,
    ) -> Result<u64, BrowserBackendError> {
        let mut control = self
            .navigation_control
            .lock()
            .map_err(|_| poisoned_lock_error(BackendOperation::Navigate, "navigation control"))?;
        if control.active.is_some() {
            return Err(BrowserBackendError::Lifecycle {
                operation: "navigate".into(),
                state: "busy".into(),
                reason: "another native navigation control is already active".into(),
            });
        }
        let id = control.next_id;
        control.next_id =
            control
                .next_id
                .checked_add(1)
                .ok_or_else(|| BrowserBackendError::Lifecycle {
                    operation: "navigate".into(),
                    state: "exhausted".into(),
                    reason: "native navigation control identifiers are exhausted".into(),
                })?;
        let cancellation = NativeNavigationCancellation::new();
        control.active = Some(ActiveNativeNavigation {
            id,
            starting_revision,
            cancellation: cancellation.clone(),
        });
        Ok(id)
    }

    pub(crate) fn active_navigation_revision(&self) -> Result<Option<u64>, BrowserBackendError> {
        let control = self
            .navigation_control
            .lock()
            .map_err(|_| poisoned_lock_error(BackendOperation::Navigate, "navigation control"))?;
        Ok(control
            .active
            .as_ref()
            .map(|active| active.starting_revision))
    }

    pub(crate) fn cancel_active_navigation(
        &self,
        expected_revision: u64,
    ) -> Result<Option<NativeNavigationCancellation>, BrowserBackendError> {
        let control = self
            .navigation_control
            .lock()
            .map_err(|_| poisoned_lock_error(BackendOperation::Navigate, "navigation control"))?;
        let Some(active) = control.active.as_ref() else {
            return Ok(None);
        };
        if active.starting_revision != expected_revision
            || (!active.cancellation.request_cancel() && !active.cancellation.is_cancelled())
        {
            return Ok(None);
        }
        Ok(Some(active.cancellation.clone()))
    }

    pub(crate) fn finish_navigation_control(&self, id: u64) {
        if let Ok(mut control) = self.navigation_control.lock()
            && control
                .active
                .as_ref()
                .is_some_and(|active| active.id == id)
        {
            control.active = None;
        }
    }

    fn active_navigation_cancellation(
        &self,
    ) -> Result<Option<NativeNavigationCancellation>, BrowserBackendError> {
        let control = self
            .navigation_control
            .lock()
            .map_err(|_| poisoned_lock_error(BackendOperation::Navigate, "navigation control"))?;
        Ok(control
            .active
            .as_ref()
            .map(|active| active.cancellation.clone()))
    }

    pub fn profile(&self) -> &BackendProfile {
        &self.profile
    }

    /// Return the current bounded semantic tree from the engine owner.
    pub fn semantic_nodes(
        &self,
    ) -> Result<Vec<super::native_engine::NativeSemanticNode>, BrowserBackendError> {
        self.lock_engine(BackendOperation::Evidence)?
            .semantic_nodes()
            .map_err(native_error)
    }

    /// Capture the current logical native surface as PNG bytes.
    pub fn capture_png(&self) -> Result<Vec<u8>, BrowserBackendError> {
        let targets = self.lock_targets(BackendOperation::Capture)?;
        let engine = self.lock_engine_raw(BackendOperation::Capture)?;
        let frame_id = targets.active_frames.active_frame_id.clone();
        capture_native_frame_surface(&engine, &targets.active_frames, &frame_id)
            .and_then(|surface| surface.to_png().map_err(native_error))
    }

    /// Discover current child browsing contexts and capture the selected
    /// frame with every visible descendant frame composited into its owner.
    /// The asynchronous form is used by normal Glass operations so a caller
    /// does not need to discover frames before requesting a screenshot.
    pub async fn capture_png_async(&self) -> Result<Vec<u8>, BrowserBackendError> {
        let capture = self
            .capture_visual(&VisualCaptureOptions::default())
            .await?;
        base64::engine::general_purpose::STANDARD
            .decode(capture.data.as_bytes())
            .map_err(|error| BrowserBackendError::InvalidConfiguration {
                field: "native capture payload".into(),
                reason: error.to_string(),
            })
    }

    /// Capture the selected native frame using the shared visual-capture
    /// contract. The native renderer keeps composition, clipping, scaling,
    /// and encoding in one operation so metadata describes the bytes that
    /// were actually written.
    pub async fn capture_visual(
        &self,
        options: &VisualCaptureOptions,
    ) -> Result<VisualCapture, BrowserBackendError> {
        crate::browser::session::validate_visual_options(options).map_err(|error| {
            BrowserBackendError::InvalidConfiguration {
                field: "visual capture".into(),
                reason: error.to_string(),
            }
        })?;
        self.synchronize_native_service_worker_clients().await?;
        let mut targets = self.lock_targets(BackendOperation::Capture)?;
        let mut engine = self.lock_engine_raw(BackendOperation::Capture)?;
        let target_id =
            targets
                .active_target_id
                .clone()
                .ok_or_else(|| BrowserBackendError::Lifecycle {
                    operation: "capture".into(),
                    state: "no-target-selected".into(),
                    reason: "select an available native page target before capture".into(),
                })?;
        let frame_id = targets.active_frames.active_frame_id.clone();
        reconcile_native_frames(&mut targets.active_frames, &mut engine).await?;

        let target_node = options
            .target
            .as_deref()
            .map(|locator| engine.resolve_target(locator))
            .transpose()
            .map_err(native_error)?;
        let base_layout = engine.layout().map_err(native_error)?;
        let expanded_viewport = (options.full_page || target_node.is_some())
            .then(|| full_page_capture_viewport(&base_layout))
            .transpose()?;
        let layout = match expanded_viewport {
            Some(viewport) => engine
                .layout_at_viewport(viewport, NativePoint { x: 0, y: 0 })
                .map_err(native_error)?,
            None => base_layout,
        };
        let mut surface = capture_native_frame_surface_with_viewport(
            &engine,
            &targets.active_frames,
            &frame_id,
            expanded_viewport,
        )?;

        let effective_clip = if let Some(node_id) = target_node {
            let rect = layout
                .boxes
                .iter()
                .find(|layout_box| layout_box.node_id == node_id)
                .map(|layout_box| layout_box.rect)
                .ok_or_else(|| BrowserBackendError::SelectionFailed {
                    reason: "native visual target has no layout box".into(),
                })?;
            let clip = VisualClip {
                x: f64::from(rect.x),
                y: f64::from(rect.y),
                width: f64::from(rect.width),
                height: f64::from(rect.height),
            };
            surface = surface.crop(rect).map_err(native_error)?;
            Some(clip)
        } else if let Some(clip) = options.clip {
            let rect = native_capture_rect(clip)?;
            surface = surface.crop(rect).map_err(native_error)?;
            Some(clip)
        } else {
            None
        };
        surface = surface.scale_nearest(options.scale).map_err(native_error)?;
        let width = surface.width();
        let height = surface.height();
        let bytes = match options.format {
            VisualFormat::Png => surface.to_png(),
            VisualFormat::Jpeg => surface.to_jpeg(options.quality.unwrap_or(80)),
            VisualFormat::Webp => surface.to_webp(options.quality),
        }
        .map_err(native_error)?;
        let data = base64::engine::general_purpose::STANDARD.encode(&bytes);
        Ok(VisualCapture {
            data,
            metadata: VisualCaptureMetadata {
                format: options.format,
                width: usize::try_from(width).unwrap_or(usize::MAX),
                height: usize::try_from(height).unwrap_or(usize::MAX),
                encoded_bytes: bytes.len(),
                device_scale_factor: f64::from(engine.config().viewport.device_scale_factor_milli)
                    / 1000.0,
                scale: options.scale,
                full_page: options.full_page,
                clip: effective_clip,
                target_id,
                frame_id,
            },
        })
    }

    /// Route a point click through the currently selected frame tree when the
    /// point lands on a live embedded browsing context. A `None` result means
    /// that the point belongs to the selected document and should continue
    /// through the ordinary action path.
    async fn dispatch_point_click(
        &self,
        context_id: &str,
        target: &str,
        modifiers: u8,
    ) -> Result<Option<BackendResponse>, BrowserBackendError> {
        let Some((x, y)) = parse_point_target(target).map_err(native_error)? else {
            return Ok(None);
        };
        let Some((frame_id, local_x, local_y)) =
            ({
                let mut targets = self.lock_targets(BackendOperation::Action)?;
                let active_context_id = targets.active_target_id.clone().ok_or_else(|| {
                    BrowserBackendError::Lifecycle {
                        operation: "action".into(),
                        state: "no-target-selected".into(),
                        reason: "select an available native page target before clicking".into(),
                    }
                })?;
                require_context_id(context_id, &active_context_id)?;
                let mut engine = self.lock_engine_raw(BackendOperation::Action)?;
                let root_frame_id = targets.active_frames.active_frame_id.clone();
                reconcile_native_frames(&mut targets.active_frames, &mut engine).await?;
                find_native_point_frame(&targets.active_frames, &root_frame_id, &engine, x, y, 0)?
            })
        else {
            return Ok(None);
        };

        let route =
            self.frame_route(&frame_id)?
                .ok_or_else(|| BrowserBackendError::SelectionFailed {
                    reason: "native point target frame disappeared before action dispatch".into(),
                })?;
        let target = format!("point={local_x},{local_y}");
        let action = if modifiers == 0 {
            NativeAction::Click { target }
        } else {
            NativeAction::ClickWithModifiers { target, modifiers }
        };
        let proxy_updates = self.window_proxy_updates(&frame_id)?;
        let (revision, accepted, runtime_effects, owner_id) = self
            .apply_action_to_native_frame(route, &frame_id, action, &proxy_updates)
            .await?;
        self.sync_target_name(&owner_id, &runtime_effects.window_name)?;
        if accepted {
            self.set_active_frame_focus(&frame_id)?;
        }
        self.process_selected_frame_events(&frame_id, runtime_effects.events)
            .await?;
        self.process_pending_frame_scripts(runtime_effects.frame_scripts)
            .await?;
        self.process_pending_browser_effects(
            runtime_effects.browser.0,
            runtime_effects.browser.1,
            runtime_effects.browser.2,
            runtime_effects.browser.3,
            runtime_effects.browser.4,
            runtime_effects.browser.5,
            runtime_effects.browser.6,
        )
        .await?;
        Ok(Some(BackendResponse::Action(ActionResult {
            context_id: context_id.to_owned(),
            revision,
            accepted,
        })))
    }

    /// Resolve a targeted semantic action through the selected frame subtree.
    /// A `None` result keeps the ordinary selected-frame dispatcher in charge;
    /// a child match is applied through the same route/effect pipeline as a
    /// point-routed action.
    async fn dispatch_locator_action(
        &self,
        context_id: &str,
        action: &SemanticAction,
    ) -> Result<Option<BackendResponse>, BrowserBackendError> {
        // `point=` is a click coordinate, not a semantic locator. Child-frame
        // routing gets the first chance to claim it in `dispatch_point_click`;
        // when the point belongs to the selected document (or misses the
        // viewport), leave it for the ordinary native action path so the
        // engine can perform hit testing and return a typed action error.
        if matches!(
            action,
            SemanticAction::Click { target }
                | SemanticAction::ClickWithModifiers { target, .. }
                if target.starts_with("point=")
        ) {
            return Ok(None);
        }
        let native_action = match action {
            SemanticAction::Click { target } => NativeAction::Click {
                target: target.clone(),
            },
            SemanticAction::ClickWithModifiers { target, modifiers } => {
                NativeAction::ClickWithModifiers {
                    target: target.clone(),
                    modifiers: modifiers.native_mask(),
                }
            }
            SemanticAction::DoubleClick { target } => NativeAction::DoubleClick {
                target: target.clone(),
            },
            SemanticAction::Hover { target } => NativeAction::Hover {
                target: target.clone(),
            },
            SemanticAction::Drag {
                source,
                destination,
            } => NativeAction::Drag {
                source: source.clone(),
                destination: destination.clone(),
            },
            SemanticAction::Type { target, text } => NativeAction::Type {
                target: target.clone(),
                text: text.clone(),
            },
            SemanticAction::Clear { target } => NativeAction::Clear {
                target: target.clone(),
            },
            SemanticAction::Check { target } => NativeAction::Check {
                target: target.clone(),
            },
            SemanticAction::Uncheck { target } => NativeAction::Uncheck {
                target: target.clone(),
            },
            SemanticAction::Select { target, value } => NativeAction::Select {
                target: target.clone(),
                value: value.clone(),
            },
            _ => return Ok(None),
        };
        let locator_targets = match action {
            SemanticAction::Click { target }
            | SemanticAction::ClickWithModifiers { target, .. }
            | SemanticAction::DoubleClick { target }
            | SemanticAction::Hover { target }
            | SemanticAction::Type { target, .. }
            | SemanticAction::Clear { target }
            | SemanticAction::Check { target }
            | SemanticAction::Uncheck { target }
            | SemanticAction::Select { target, .. } => vec![target.as_str()],
            SemanticAction::Drag {
                source,
                destination,
            } => vec![source.as_str(), destination.as_str()],
            _ => return Ok(None),
        };
        let frame_id = {
            let mut targets = self.lock_targets(BackendOperation::Action)?;
            let active_context_id =
                targets
                    .active_target_id
                    .clone()
                    .ok_or_else(|| BrowserBackendError::Lifecycle {
                        operation: "action".into(),
                        state: "no-target-selected".into(),
                        reason: "select an available native page target before acting".into(),
                    })?;
            require_context_id(context_id, &active_context_id)?;
            let mut engine = self.lock_engine_raw(BackendOperation::Action)?;
            let root_frame_id = targets.active_frames.active_frame_id.clone();
            reconcile_native_frames(&mut targets.active_frames, &mut engine).await?;
            let candidate_frame_ids = std::iter::once(root_frame_id.clone())
                .chain(targets.active_frames.descendant_ids(&root_frame_id))
                .collect::<Vec<_>>();
            let mut matches = Vec::new();
            let mut first_error = None;
            for candidate_id in candidate_frame_ids {
                let result = if candidate_id == root_frame_id {
                    locator_targets
                        .iter()
                        .try_for_each(|target| engine.resolve_target(target).map(|_| ()))
                } else {
                    targets
                        .active_frames
                        .parked
                        .get(&candidate_id)
                        .ok_or(NativeEngineError::DetachedTarget)
                        .and_then(|frame| {
                            locator_targets.iter().try_for_each(|target| {
                                frame.engine.resolve_target(target).map(|_| ())
                            })
                        })
                };
                match result {
                    Ok(_) => matches.push(candidate_id),
                    Err(NativeEngineError::TargetNotFound) => {}
                    Err(error) => {
                        first_error.get_or_insert(error);
                    }
                }
            }
            if let Some(error) = first_error {
                return Err(native_error(error));
            }
            if matches.len() > 1 {
                return Err(BrowserBackendError::UnsupportedOperation {
                    operation: "action".into(),
                    reason: format!(
                        "native semantic locator matched {} frame documents; exactly one is required",
                        matches.len()
                    ),
                });
            }
            matches.into_iter().next()
        };
        let Some(frame_id) = frame_id else {
            return Ok(None);
        };
        let root_frame_id = self.active_frame_id()?;
        if frame_id == root_frame_id {
            return Ok(None);
        }
        let route =
            self.frame_route(&frame_id)?
                .ok_or_else(|| BrowserBackendError::SelectionFailed {
                    reason: "native locator target frame disappeared before action dispatch".into(),
                })?;
        let proxy_updates = self.window_proxy_updates(&frame_id)?;
        let (revision, accepted, runtime_effects, owner_id) = self
            .apply_action_to_native_frame(route, &frame_id, native_action, &proxy_updates)
            .await?;
        self.sync_target_name(&owner_id, &runtime_effects.window_name)?;
        if accepted {
            self.set_active_frame_focus(&frame_id)?;
        }
        self.process_selected_frame_events(&frame_id, runtime_effects.events)
            .await?;
        self.process_pending_frame_scripts(runtime_effects.frame_scripts)
            .await?;
        self.process_pending_browser_effects(
            runtime_effects.browser.0,
            runtime_effects.browser.1,
            runtime_effects.browser.2,
            runtime_effects.browser.3,
            runtime_effects.browser.4,
            runtime_effects.browser.5,
            runtime_effects.browser.6,
        )
        .await?;
        Ok(Some(BackendResponse::Action(ActionResult {
            context_id: context_id.to_owned(),
            revision,
            accepted,
        })))
    }

    /// Dispatch a semantic action to the frame selected by a fresh semantic
    /// candidate. This preserves local revisioned node references while
    /// preventing an identical reference in another frame from being chosen.
    pub async fn action_in_frame(
        &self,
        context_id: &str,
        frame_id: &str,
        action: SemanticAction,
    ) -> Result<ActionResult, BrowserBackendError> {
        let native_action = match action {
            SemanticAction::Click { target } => NativeAction::Click { target },
            SemanticAction::ClickWithModifiers { target, modifiers } => {
                NativeAction::ClickWithModifiers {
                    target,
                    modifiers: modifiers.native_mask(),
                }
            }
            SemanticAction::DoubleClick { target } => NativeAction::DoubleClick { target },
            SemanticAction::Hover { target } => NativeAction::Hover { target },
            SemanticAction::Drag {
                source,
                destination,
            } => NativeAction::Drag {
                source,
                destination,
            },
            SemanticAction::Type { target, text } => NativeAction::Type { target, text },
            SemanticAction::Clear { target } => NativeAction::Clear { target },
            SemanticAction::Check { target } => NativeAction::Check { target },
            SemanticAction::Uncheck { target } => NativeAction::Uncheck { target },
            SemanticAction::Select { target, value } => NativeAction::Select { target, value },
            SemanticAction::KeyDown { key } => NativeAction::KeyDown { key },
            SemanticAction::KeyUp { key } => NativeAction::KeyUp { key },
            SemanticAction::Shortcut { shortcut } => NativeAction::Shortcut { shortcut },
            SemanticAction::KeyPress { key } => NativeAction::KeyPress { key },
            SemanticAction::Scroll { delta_x, delta_y } => {
                NativeAction::Scroll { delta_x, delta_y }
            }
        };
        {
            let mut targets = self.lock_targets(BackendOperation::Action)?;
            let active_context_id =
                targets
                    .active_target_id
                    .clone()
                    .ok_or_else(|| BrowserBackendError::Lifecycle {
                        operation: "action".into(),
                        state: "no-target-selected".into(),
                        reason: "select an available native page target before acting".into(),
                    })?;
            require_context_id(context_id, &active_context_id)?;
            validate_native_topology_id(frame_id)?;
            let mut engine = self.lock_engine_raw(BackendOperation::Action)?;
            let root_frame_id = targets.active_frames.active_frame_id.clone();
            reconcile_native_frames(&mut targets.active_frames, &mut engine).await?;
            if frame_id != root_frame_id && !targets.active_frames.parked.contains_key(frame_id) {
                return Err(BrowserBackendError::SelectionFailed {
                    reason: "native semantic frame is no longer attached; inspect again".into(),
                });
            }
        }
        let route =
            self.frame_route(frame_id)?
                .ok_or_else(|| BrowserBackendError::SelectionFailed {
                    reason: "native semantic frame disappeared before action dispatch".into(),
                })?;
        let proxy_updates = self.window_proxy_updates(frame_id)?;
        let (revision, accepted, runtime_effects, owner_id) = self
            .apply_action_to_native_frame(route, frame_id, native_action, &proxy_updates)
            .await?;
        self.sync_target_name(&owner_id, &runtime_effects.window_name)?;
        if accepted {
            self.set_active_frame_focus(frame_id)?;
        }
        self.process_selected_frame_events(frame_id, runtime_effects.events)
            .await?;
        self.process_pending_frame_scripts(runtime_effects.frame_scripts)
            .await?;
        self.process_pending_browser_effects(
            runtime_effects.browser.0,
            runtime_effects.browser.1,
            runtime_effects.browser.2,
            runtime_effects.browser.3,
            runtime_effects.browser.4,
            runtime_effects.browser.5,
            runtime_effects.browser.6,
        )
        .await?;
        Ok(ActionResult {
            context_id: context_id.to_owned(),
            revision,
            accepted,
        })
    }

    /// Upload bounded in-memory file objects to one exact native frame.
    /// Unlike pointer and keyboard actions, file selection does not require a
    /// visible hit target and therefore does not update the focused frame.
    pub async fn upload_files_in_frame(
        &self,
        context_id: &str,
        frame_id: &str,
        target: &str,
        files: Vec<NativeFile>,
    ) -> Result<ActionResult, BrowserBackendError> {
        NativeFile::validate_many(&files).map_err(native_error)?;
        {
            let mut targets = self.lock_targets(BackendOperation::Action)?;
            let active_context_id =
                targets
                    .active_target_id
                    .clone()
                    .ok_or_else(|| BrowserBackendError::Lifecycle {
                        operation: "upload".into(),
                        state: "no-target-selected".into(),
                        reason: "select an available native page target before uploading".into(),
                    })?;
            require_context_id(context_id, &active_context_id)?;
            validate_native_topology_id(frame_id)?;
            let mut engine = self.lock_engine_raw(BackendOperation::Action)?;
            let root_frame_id = targets.active_frames.active_frame_id.clone();
            reconcile_native_frames(&mut targets.active_frames, &mut engine).await?;
            if frame_id != root_frame_id && !targets.active_frames.parked.contains_key(frame_id) {
                return Err(BrowserBackendError::SelectionFailed {
                    reason: "native upload frame is no longer attached; inspect again".into(),
                });
            }
        }
        let route =
            self.frame_route(frame_id)?
                .ok_or_else(|| BrowserBackendError::SelectionFailed {
                    reason: "native upload frame disappeared before action dispatch".into(),
                })?;
        let proxy_updates = self.window_proxy_updates(frame_id)?;
        let (revision, accepted, runtime_effects, owner_id) = self
            .apply_action_to_native_frame(
                route,
                frame_id,
                NativeAction::Upload {
                    target: target.to_owned(),
                    files,
                },
                &proxy_updates,
            )
            .await?;
        self.sync_target_name(&owner_id, &runtime_effects.window_name)?;
        self.process_selected_frame_events(frame_id, runtime_effects.events)
            .await?;
        self.process_pending_frame_scripts(runtime_effects.frame_scripts)
            .await?;
        self.process_pending_browser_effects(
            runtime_effects.browser.0,
            runtime_effects.browser.1,
            runtime_effects.browser.2,
            runtime_effects.browser.3,
            runtime_effects.browser.4,
            runtime_effects.browser.5,
            runtime_effects.browser.6,
        )
        .await?;
        Ok(ActionResult {
            context_id: context_id.to_owned(),
            revision,
            accepted,
        })
    }

    async fn apply_action_to_native_frame(
        &self,
        route: NativeFrameRoute,
        frame_id: &str,
        action: NativeAction,
        proxy_updates: &[NativeWindowProxyUpdate],
    ) -> Result<(u64, bool, NativeFrameRuntimeEffects, String), BrowserBackendError> {
        let frame_script_context = if matches!(
            &route,
            NativeFrameRoute::ActiveSelected | NativeFrameRoute::ActiveParked
        ) {
            self.frame_script_context_for_active_target_frame(frame_id)
                .await?
        } else {
            None
        };
        match route {
            NativeFrameRoute::ActiveSelected => {
                let mut engine = self.lock_engine_raw(BackendOperation::Action)?;
                engine.set_frame_script_context(frame_script_context);
                engine
                    .sync_window_proxies(proxy_updates)
                    .await
                    .map_err(native_error)?;
                let previous_revision = engine.revision();
                let outcome = engine.action_async(action).await.map_err(native_error)?;
                let runtime = take_native_frame_runtime_effects(&mut engine, previous_revision)?;
                let owner_id = engine.config().context_id.clone();
                Ok((outcome.revision, outcome.accepted, runtime, owner_id))
            }
            NativeFrameRoute::ActiveParked => {
                let mut targets = self.lock_targets(BackendOperation::Action)?;
                let owner_id = targets.active_target_id.clone().ok_or_else(|| {
                    BrowserBackendError::SelectionFailed {
                        reason: "native frame owner target disappeared during point dispatch"
                            .into(),
                    }
                })?;
                let frame = targets
                    .active_frames
                    .parked
                    .get_mut(frame_id)
                    .ok_or_else(|| BrowserBackendError::SelectionFailed {
                        reason: "native point target frame disappeared during action dispatch"
                            .into(),
                    })?;
                frame.engine.set_frame_script_context(frame_script_context);
                frame
                    .engine
                    .sync_window_proxies(proxy_updates)
                    .await
                    .map_err(native_error)?;
                let previous_revision = frame.engine.revision();
                let outcome = frame
                    .engine
                    .action_async(action)
                    .await
                    .map_err(native_error)?;
                let runtime =
                    take_native_frame_runtime_effects(&mut frame.engine, previous_revision)?;
                Ok((outcome.revision, outcome.accepted, runtime, owner_id))
            }
            NativeFrameRoute::ParkedSelected { target_id } => {
                let mut targets = self.lock_targets(BackendOperation::Action)?;
                let target = targets.parked.get_mut(&target_id).ok_or_else(|| {
                    BrowserBackendError::SelectionFailed {
                        reason: "native frame owner target disappeared during point dispatch"
                            .into(),
                    }
                })?;
                target
                    .engine
                    .sync_window_proxies(proxy_updates)
                    .await
                    .map_err(native_error)?;
                let previous_revision = target.engine.revision();
                let outcome = target
                    .engine
                    .action_async(action)
                    .await
                    .map_err(native_error)?;
                let runtime =
                    take_native_frame_runtime_effects(&mut target.engine, previous_revision)?;
                Ok((outcome.revision, outcome.accepted, runtime, target_id))
            }
            NativeFrameRoute::ParkedParked { target_id } => {
                let mut targets = self.lock_targets(BackendOperation::Action)?;
                let target = targets.parked.get_mut(&target_id).ok_or_else(|| {
                    BrowserBackendError::SelectionFailed {
                        reason: "native frame owner target disappeared during point dispatch"
                            .into(),
                    }
                })?;
                let frame = target.frames.parked.get_mut(frame_id).ok_or_else(|| {
                    BrowserBackendError::SelectionFailed {
                        reason: "native point target frame disappeared during action dispatch"
                            .into(),
                    }
                })?;
                frame
                    .engine
                    .sync_window_proxies(proxy_updates)
                    .await
                    .map_err(native_error)?;
                let previous_revision = frame.engine.revision();
                let outcome = frame
                    .engine
                    .action_async(action)
                    .await
                    .map_err(native_error)?;
                let runtime =
                    take_native_frame_runtime_effects(&mut frame.engine, previous_revision)?;
                Ok((outcome.revision, outcome.accepted, runtime, target_id))
            }
        }
    }

    async fn dispatch_focused_frame_action(
        &self,
        context_id: &str,
        action: NativeAction,
    ) -> Result<Option<BackendResponse>, BrowserBackendError> {
        let frame_id =
            {
                let mut targets = self.lock_targets(BackendOperation::Action)?;
                let active_context_id = targets.active_target_id.clone().ok_or_else(|| {
                    BrowserBackendError::Lifecycle {
                        operation: "action".into(),
                        state: "no-target-selected".into(),
                        reason: "select an available native page target before key input".into(),
                    }
                })?;
                require_context_id(context_id, &active_context_id)?;
                let mut engine = self.lock_engine_raw(BackendOperation::Action)?;
                let root_frame_id = targets.active_frames.active_frame_id.clone();
                reconcile_native_frames(&mut targets.active_frames, &mut engine).await?;
                let Some(focused_frame_id) = targets.active_frames.focused_frame_id.clone() else {
                    return Ok(None);
                };
                if focused_frame_id == root_frame_id
                    || !targets.active_frames.parked.contains_key(&focused_frame_id)
                {
                    return Ok(None);
                }
                focused_frame_id
            };
        let route =
            self.frame_route(&frame_id)?
                .ok_or_else(|| BrowserBackendError::SelectionFailed {
                    reason: "native focused frame disappeared before key dispatch".into(),
                })?;
        let proxy_updates = self.window_proxy_updates(&frame_id)?;
        let (revision, accepted, runtime_effects, owner_id) = self
            .apply_action_to_native_frame(route, &frame_id, action, &proxy_updates)
            .await?;
        self.sync_target_name(&owner_id, &runtime_effects.window_name)?;
        self.process_selected_frame_events(&frame_id, runtime_effects.events)
            .await?;
        self.process_pending_frame_scripts(runtime_effects.frame_scripts)
            .await?;
        self.process_pending_browser_effects(
            runtime_effects.browser.0,
            runtime_effects.browser.1,
            runtime_effects.browser.2,
            runtime_effects.browser.3,
            runtime_effects.browser.4,
            runtime_effects.browser.5,
            runtime_effects.browser.6,
        )
        .await?;
        Ok(Some(BackendResponse::Action(ActionResult {
            context_id: context_id.to_owned(),
            revision,
            accepted,
        })))
    }

    fn set_active_frame_focus(&self, frame_id: &str) -> Result<(), BrowserBackendError> {
        validate_native_topology_id(frame_id)?;
        let mut targets = self.lock_targets(BackendOperation::Action)?;
        if targets.active_target_id.is_none()
            || (targets.active_frames.active_frame_id != frame_id
                && !targets.active_frames.parked.contains_key(frame_id))
        {
            return Err(BrowserBackendError::SelectionFailed {
                reason: "native focus frame is no longer attached".into(),
            });
        }
        targets.active_frames.focused_frame_id = Some(frame_id.to_owned());
        Ok(())
    }

    /// Return a side-effect-free, revision-bound target preflight result.
    /// Locators are resolved across the selected frame subtree and the
    /// winning result records the owning frame for later exact dispatch.
    pub async fn preflight_target(
        &self,
        target: &str,
        action: NativePreflightAction,
    ) -> Result<NativeTargetPreflight, BrowserBackendError> {
        let mut targets = self.lock_targets(BackendOperation::Evidence)?;
        if targets.active_target_id.is_none() {
            return Err(BrowserBackendError::Lifecycle {
                operation: "evidence".into(),
                state: "no-target-selected".into(),
                reason: "select an available native page target before target preflight".into(),
            });
        }
        let mut engine = self.lock_engine_raw(BackendOperation::Evidence)?;
        let root_frame_id = targets.active_frames.active_frame_id.clone();
        reconcile_native_frames(&mut targets.active_frames, &mut engine).await?;
        let frame_ids = std::iter::once(root_frame_id.clone())
            .chain(targets.active_frames.descendant_ids(&root_frame_id))
            .collect::<Vec<_>>();
        let mut matches = Vec::new();
        let mut fallback = None;
        for frame_id in frame_ids {
            let result = if frame_id == root_frame_id {
                engine
                    .preflight_target(target, action)
                    .map_err(native_error)?
            } else {
                let frame = targets.active_frames.parked.get(&frame_id).ok_or_else(|| {
                    BrowserBackendError::SelectionFailed {
                        reason: "native frame disappeared during target preflight".into(),
                    }
                })?;
                frame
                    .engine
                    .preflight_target(target, action)
                    .map_err(native_error)?
            };
            if result.unique {
                matches.push((frame_id, result));
            } else if result.error_kind
                != Some(super::native_engine::NativeTargetErrorKind::NotFound)
            {
                fallback.get_or_insert(result);
            }
        }

        if matches.len() > 1 {
            let revision = engine.revision();
            return Ok(NativeTargetPreflight {
                action,
                unique: false,
                node: None,
                actionable: None,
                actionability_reason: None,
                error_kind: Some(super::native_engine::NativeTargetErrorKind::Ambiguous),
                revision,
                frame_id: None,
                geometry: None,
                likely_navigation: false,
                likely_popup: false,
                likely_form_submit: false,
            });
        }
        if let Some((frame_id, mut result)) = matches.pop() {
            result.frame_id = Some(frame_id);
            return Ok(result);
        }
        Ok(fallback.unwrap_or_else(|| NativeTargetPreflight {
            action,
            unique: false,
            node: None,
            actionable: None,
            actionability_reason: None,
            error_kind: Some(super::native_engine::NativeTargetErrorKind::NotFound),
            revision: engine.revision(),
            frame_id: None,
            geometry: None,
            likely_navigation: false,
            likely_popup: false,
            likely_form_submit: false,
        }))
    }

    /// Return one atomic page/semantic/layout snapshot for agent discovery.
    pub fn inspection_snapshot(&self) -> Result<NativeInspectionSnapshot, BrowserBackendError> {
        self.lock_engine(BackendOperation::Evidence)?
            .inspection_snapshot()
            .map_err(native_error)
    }

    /// Return bounded parser and presentation diagnostics for the active
    /// native document. Diagnostics are read-only and carry no page secrets.
    pub fn diagnostics(
        &self,
    ) -> Result<super::native_engine::NativeDiagnosticsSnapshot, BrowserBackendError> {
        self.lock_engine(BackendOperation::Evidence)?
            .diagnostics()
            .map_err(native_error)
    }

    /// Discover and snapshot the selected frame subtree under one registry
    /// lock. Each child remains an independent native document, but callers
    /// receive its owning frame identity alongside its revision-bound handles.
    pub async fn inspection_snapshots(
        &self,
    ) -> Result<Vec<NativeFrameInspectionSnapshot>, BrowserBackendError> {
        self.synchronize_native_service_worker_clients().await?;
        let mut targets = self.lock_targets(BackendOperation::Evidence)?;
        if targets.active_target_id.is_none() {
            return Err(BrowserBackendError::Lifecycle {
                operation: "evidence".into(),
                state: "no-target-selected".into(),
                reason: "select an available native page target before semantic inspection".into(),
            });
        }
        let mut engine = self.lock_engine_raw(BackendOperation::Evidence)?;
        let root_frame_id = targets.active_frames.active_frame_id.clone();
        reconcile_native_frames(&mut targets.active_frames, &mut engine).await?;
        let frame_ids = std::iter::once(root_frame_id.clone())
            .chain(targets.active_frames.descendant_ids(&root_frame_id))
            .collect::<Vec<_>>();
        let mut snapshots = Vec::with_capacity(frame_ids.len());
        for frame_id in frame_ids {
            if frame_id == root_frame_id {
                snapshots.push(NativeFrameInspectionSnapshot {
                    frame_id,
                    parent_id: targets.active_frames.active_parent_id.clone(),
                    inspection: engine.inspection_snapshot().map_err(native_error)?,
                });
                continue;
            }
            let frame = targets.active_frames.parked.get(&frame_id).ok_or_else(|| {
                BrowserBackendError::SelectionFailed {
                    reason: "native frame disappeared during semantic inspection".into(),
                }
            })?;
            snapshots.push(NativeFrameInspectionSnapshot {
                frame_id,
                parent_id: frame.parent_id.clone(),
                inspection: frame.engine.inspection_snapshot().map_err(native_error)?,
            });
        }
        Ok(snapshots)
    }

    pub async fn cookies(
        &self,
    ) -> Result<Vec<crate::browser::session::Cookie>, BrowserBackendError> {
        self.lock_engine(BackendOperation::Storage)?
            .cookies_async()
            .await
            .map_err(native_error)
    }

    pub async fn set_cookies(
        &self,
        cookies: &[crate::browser::session::Cookie],
    ) -> Result<(), BrowserBackendError> {
        self.lock_engine(BackendOperation::Storage)?
            .set_cookies_async(cookies)
            .await
            .map_err(native_error)
    }

    pub async fn clear_cookies(&self) -> Result<(), BrowserBackendError> {
        self.lock_engine(BackendOperation::Storage)?
            .clear_cookies_async()
            .await
            .map_err(native_error)
    }

    pub async fn set_network_conditions(
        &self,
        conditions: Option<&NetworkConditions>,
    ) -> Result<(), BrowserBackendError> {
        self.lock_engine(BackendOperation::Navigate)?
            .set_network_conditions_async(conditions)
            .await
            .map_err(native_error)
    }

    pub async fn set_cpu_throttling(&self, rate: Option<f64>) -> Result<(), BrowserBackendError> {
        self.lock_engine(BackendOperation::Script)?
            .set_cpu_throttling_async(rate)
            .await
            .map_err(native_error)
    }

    pub async fn set_user_agent(
        &self,
        user_agent: Option<&str>,
        accept_language: Option<&str>,
        platform: Option<&str>,
    ) -> Result<(), BrowserBackendError> {
        self.lock_engine(BackendOperation::Script)?
            .set_user_agent_async(user_agent, accept_language, platform)
            .await
            .map_err(native_error)
    }

    pub async fn set_geolocation(
        &self,
        location: Option<&GeoLocation>,
    ) -> Result<(), BrowserBackendError> {
        self.lock_engine(BackendOperation::Script)?
            .set_geolocation_async(location)
            .await
            .map_err(native_error)
    }

    pub async fn set_timezone(&self, timezone_id: Option<&str>) -> Result<(), BrowserBackendError> {
        self.lock_engine(BackendOperation::Script)?
            .set_timezone_async(timezone_id)
            .await
            .map_err(native_error)
    }

    pub async fn set_viewport(&self, viewport: Viewport) -> Result<(), BrowserBackendError> {
        self.lock_engine(BackendOperation::Script)?
            .set_viewport_async(viewport)
            .await
            .map_err(native_error)
    }

    pub async fn pending_dialog(
        &self,
    ) -> Result<Option<crate::browser::session::PendingDialog>, BrowserBackendError> {
        self.lock_engine(BackendOperation::Prompt)?
            .pending_dialog()
            .map_err(native_error)
    }

    pub fn network_quiet(
        &self,
        duration: std::time::Duration,
    ) -> Result<(bool, String), BrowserBackendError> {
        self.lock_engine(BackendOperation::Effects)?
            .network_quiet(duration)
            .map_err(native_error)
    }

    pub async fn wait_for_download(
        &self,
        destination: &std::path::Path,
        deadline: std::time::Duration,
    ) -> Result<crate::browser::session::DownloadOutcome, BrowserBackendError> {
        self.lock_engine(BackendOperation::Download)?
            .wait_for_download_async(destination, deadline)
            .await
            .map_err(native_error)
    }

    pub fn download_ids(&self) -> Result<Vec<String>, BrowserBackendError> {
        self.lock_engine(BackendOperation::Download)?
            .download_ids()
            .map_err(native_error)
    }

    pub fn cancel_download(&self, download_id: &str) -> Result<bool, BrowserBackendError> {
        self.lock_engine(BackendOperation::Download)?
            .cancel_download(download_id)
            .map_err(native_error)
    }

    pub fn completed_download_count(&self) -> Result<u64, BrowserBackendError> {
        self.lock_engine(BackendOperation::Download)?
            .completed_download_count()
            .map_err(native_error)
    }

    pub async fn resolve_dialog(
        &self,
        decision: PromptDecision,
    ) -> Result<PromptResult, BrowserBackendError> {
        self.lock_engine(BackendOperation::Prompt)?
            .resolve_dialog(decision)
            .map_err(native_error)
    }

    /// Project every target-owned native engine into the standard page-target
    /// contract. A parked target keeps its complete document and runtime state
    /// alive; selecting it only changes which owner receives subsequent work.
    pub fn list_targets(&self) -> Result<Vec<PageTargetInfo>, BrowserBackendError> {
        let targets = self.lock_targets(BackendOperation::Contexts)?;
        let engine = self.lock_engine_raw(BackendOperation::Contexts)?;
        let mut projected = Vec::with_capacity(targets.target_count());
        if let Some(active_target_id) = targets.active_target_id.as_deref() {
            projected.push(project_native_target(
                &engine,
                active_target_id,
                targets.active_opener_id.clone(),
                true,
            )?);
        }
        for (target_id, parked) in &targets.parked {
            projected.push(project_native_target(
                &parked.engine,
                target_id,
                parked.opener_id.clone(),
                false,
            )?);
        }
        Ok(projected)
    }

    /// Project the selected native target's complete bounded frame tree. A
    /// child frame is initialized before publication, so every returned frame
    /// has a live native document owner.
    pub async fn list_frames(&self) -> Result<Vec<FrameInfo>, BrowserBackendError> {
        self.synchronize_native_service_worker_clients().await?;
        let mut targets = self.lock_targets(BackendOperation::Contexts)?;
        if targets.active_target_id.is_none() {
            return Err(BrowserBackendError::Lifecycle {
                operation: "contexts".into(),
                state: "no-target-selected".into(),
                reason: "select an available native page target before frame discovery".into(),
            });
        }
        let mut engine = self.lock_engine_raw(BackendOperation::Contexts)?;
        reconcile_native_frames(&mut targets.active_frames, &mut engine).await?;
        let active_frame = project_native_frame(
            &engine,
            &targets.active_frames.active_frame_id,
            targets.active_frames.active_parent_id.clone(),
            true,
        )?;
        let mut frames = vec![active_frame];
        for (frame_id, frame) in &targets.active_frames.parked {
            frames.push(project_native_frame(
                &frame.engine,
                frame_id,
                frame.parent_id.clone(),
                false,
            )?);
        }
        Ok(order_native_frames(frames))
    }

    async fn frame_script_bindings(
        &self,
    ) -> Result<Vec<NativeFrameScriptBinding>, BrowserBackendError> {
        let mut targets = self.lock_targets(BackendOperation::Script)?;
        if targets.active_target_id.is_none() {
            return Ok(Vec::new());
        }
        let mut engine = self.lock_engine_raw(BackendOperation::Script)?;
        reconcile_native_frames(&mut targets.active_frames, &mut engine).await?;
        let active_frame_id = targets.active_frames.active_frame_id.clone();
        let parent_snapshot = engine.snapshot().map_err(native_error)?;
        native_frame_script_children(
            &targets.active_frames,
            &engine,
            &active_frame_id,
            &parent_snapshot.origin,
        )
    }

    async fn frame_script_context(
        &self,
    ) -> Result<Option<NativeFrameScriptContext>, BrowserBackendError> {
        let mut targets = self.lock_targets(BackendOperation::Script)?;
        let Some(target_id) = targets.active_target_id.clone() else {
            return Ok(None);
        };
        let mut engine = self.lock_engine_raw(BackendOperation::Script)?;
        reconcile_native_frames(&mut targets.active_frames, &mut engine).await?;
        let Some(parent_id) = targets.active_frames.active_parent_id.clone() else {
            return Ok(None);
        };
        let current_snapshot = engine.snapshot().map_err(native_error)?;
        let parent_engine = native_frame_engine(&targets.active_frames, &engine, &parent_id)
            .ok_or_else(|| BrowserBackendError::SelectionFailed {
                reason: "native frame parent disappeared during script context projection".into(),
            })?;
        let parent_snapshot = parent_engine.snapshot().map_err(native_error)?;
        let parent_same_origin = current_snapshot.origin != NativeOrigin::Opaque
            && current_snapshot.origin == parent_snapshot.origin;
        let parent_window = native_frame_script_window(
            &targets.active_frames,
            &engine,
            &parent_id,
            parent_engine,
            parent_same_origin,
        )?;
        let top_id = native_main_frame_id(&target_id);
        let top_engine =
            native_frame_engine(&targets.active_frames, &engine, &top_id).ok_or_else(|| {
                BrowserBackendError::SelectionFailed {
                    reason: "native top frame disappeared during script context projection".into(),
                }
            })?;
        let top_snapshot = top_engine.snapshot().map_err(native_error)?;
        let top_same_origin = current_snapshot.origin != NativeOrigin::Opaque
            && current_snapshot.origin == top_snapshot.origin;
        let top_window = native_frame_script_window(
            &targets.active_frames,
            &engine,
            &top_id,
            top_engine,
            top_same_origin,
        )?;
        let frame_element = targets
            .active_frames
            .active_owner_node_index
            .and_then(|node_index| {
                parent_engine
                    .script_document_snapshot()
                    .ok()
                    .and_then(|document| {
                        document
                            .elements
                            .into_iter()
                            .find(|element| element.node_index == node_index)
                    })
            });
        Ok(Some(NativeFrameScriptContext {
            current_frame_id: targets.active_frames.active_frame_id.clone(),
            parent: Some(parent_window),
            top: Some(top_window),
            frame_element,
        }))
    }

    async fn frame_script_context_for_active_target_frame(
        &self,
        frame_id: &str,
    ) -> Result<Option<NativeFrameScriptContext>, BrowserBackendError> {
        let mut targets = self.lock_targets(BackendOperation::Action)?;
        let Some(target_id) = targets.active_target_id.clone() else {
            return Ok(None);
        };
        let mut engine = self.lock_engine_raw(BackendOperation::Action)?;
        reconcile_native_frames(&mut targets.active_frames, &mut engine).await?;
        let (parent_id, owner_node_index) = if targets.active_frames.active_frame_id == frame_id {
            (
                targets.active_frames.active_parent_id.clone(),
                targets.active_frames.active_owner_node_index,
            )
        } else if let Some(frame) = targets.active_frames.parked.get(frame_id) {
            (frame.parent_id.clone(), frame.owner_node_index)
        } else {
            return Ok(None);
        };
        let Some(parent_id) = parent_id else {
            return Ok(None);
        };
        let current_engine = native_frame_engine(&targets.active_frames, &engine, frame_id)
            .ok_or_else(|| BrowserBackendError::SelectionFailed {
                reason: "native frame disappeared during action context projection".into(),
            })?;
        let current_snapshot = current_engine.snapshot().map_err(native_error)?;
        let parent_engine = native_frame_engine(&targets.active_frames, &engine, &parent_id)
            .ok_or_else(|| BrowserBackendError::SelectionFailed {
                reason: "native frame parent disappeared during action context projection".into(),
            })?;
        let parent_snapshot = parent_engine.snapshot().map_err(native_error)?;
        let parent_same_origin = current_snapshot.origin != NativeOrigin::Opaque
            && current_snapshot.origin == parent_snapshot.origin;
        let parent_window = native_frame_script_window(
            &targets.active_frames,
            &engine,
            &parent_id,
            parent_engine,
            parent_same_origin,
        )?;
        let top_id = native_main_frame_id(&target_id);
        let top_engine =
            native_frame_engine(&targets.active_frames, &engine, &top_id).ok_or_else(|| {
                BrowserBackendError::SelectionFailed {
                    reason: "native top frame disappeared during action context projection".into(),
                }
            })?;
        let top_snapshot = top_engine.snapshot().map_err(native_error)?;
        let top_same_origin = current_snapshot.origin != NativeOrigin::Opaque
            && current_snapshot.origin == top_snapshot.origin;
        let top_window = native_frame_script_window(
            &targets.active_frames,
            &engine,
            &top_id,
            top_engine,
            top_same_origin,
        )?;
        let frame_element = owner_node_index.and_then(|node_index| {
            parent_engine
                .script_document_snapshot()
                .ok()
                .and_then(|document| {
                    document
                        .elements
                        .into_iter()
                        .find(|element| element.node_index == node_index)
                })
        });
        Ok(Some(NativeFrameScriptContext {
            current_frame_id: frame_id.to_owned(),
            parent: Some(parent_window),
            top: Some(top_window),
            frame_element,
        }))
    }

    fn frame_route(&self, frame_id: &str) -> Result<Option<NativeFrameRoute>, BrowserBackendError> {
        validate_native_topology_id(frame_id)?;
        let targets = self.lock_targets(BackendOperation::Script)?;
        if targets.active_frames.active_frame_id == frame_id {
            return Ok(Some(NativeFrameRoute::ActiveSelected));
        }
        if targets.active_frames.parked.contains_key(frame_id) {
            return Ok(Some(NativeFrameRoute::ActiveParked));
        }
        for (target_id, target) in &targets.parked {
            if target.frames.active_frame_id == frame_id {
                return Ok(Some(NativeFrameRoute::ParkedSelected {
                    target_id: target_id.clone(),
                }));
            }
            if target.frames.parked.contains_key(frame_id) {
                return Ok(Some(NativeFrameRoute::ParkedParked {
                    target_id: target_id.clone(),
                }));
            }
        }
        Ok(None)
    }

    fn service_worker_client_route(
        &self,
        client_id: &str,
    ) -> Result<Option<(NativeFrameRoute, String)>, BrowserBackendError> {
        if client_id.is_empty() {
            return Err(BrowserBackendError::InvalidConfiguration {
                field: "service worker client message target".into(),
                reason: "client id must not be empty".into(),
            });
        }
        if client_id.len() > MAX_BACKEND_ID_BYTES {
            return Err(BrowserBackendError::InvalidConfiguration {
                field: "service worker client message target".into(),
                reason: format!(
                    "client id exceeds the bounded limit ({MAX_BACKEND_ID_BYTES} bytes)"
                ),
            });
        }
        let targets = self.lock_targets(BackendOperation::Script)?;
        let engine = self.lock_engine_raw(BackendOperation::Script)?;
        if targets.active_target_id.is_some() {
            if engine.service_worker_client_id() == client_id {
                return Ok(Some((
                    NativeFrameRoute::ActiveSelected,
                    targets.active_frames.active_frame_id.clone(),
                )));
            }
            for (frame_id, frame) in &targets.active_frames.parked {
                if frame.engine.service_worker_client_id() == client_id {
                    return Ok(Some((NativeFrameRoute::ActiveParked, frame_id.clone())));
                }
            }
        }
        for (target_id, target) in &targets.parked {
            if target.engine.service_worker_client_id() == client_id {
                return Ok(Some((
                    NativeFrameRoute::ParkedSelected {
                        target_id: target_id.clone(),
                    },
                    target.frames.active_frame_id.clone(),
                )));
            }
            for (frame_id, frame) in &target.frames.parked {
                if frame.engine.service_worker_client_id() == client_id {
                    return Ok(Some((
                        NativeFrameRoute::ParkedParked {
                            target_id: target_id.clone(),
                        },
                        frame_id.clone(),
                    )));
                }
            }
        }
        Ok(None)
    }

    fn frame_parent_id(&self, frame_id: &str) -> Result<Option<String>, BrowserBackendError> {
        validate_native_topology_id(frame_id)?;
        let targets = self.lock_targets(BackendOperation::Script)?;
        if targets.active_frames.active_frame_id == frame_id {
            return Ok(targets.active_frames.active_parent_id.clone());
        }
        if let Some(frame) = targets.active_frames.parked.get(frame_id) {
            return Ok(frame.parent_id.clone());
        }
        for target in targets.parked.values() {
            if target.frames.active_frame_id == frame_id {
                return Ok(target.frames.active_parent_id.clone());
            }
            if let Some(frame) = target.frames.parked.get(frame_id) {
                return Ok(frame.parent_id.clone());
            }
        }
        Ok(None)
    }

    fn frame_is_ancestor(
        &self,
        ancestor_id: &str,
        descendant_id: &str,
    ) -> Result<bool, BrowserBackendError> {
        validate_native_topology_id(ancestor_id)?;
        validate_native_topology_id(descendant_id)?;
        let mut current_id = descendant_id.to_owned();
        for _ in 0..NATIVE_MAX_FRAMES {
            let Some(parent_id) = self.frame_parent_id(&current_id)? else {
                return Ok(false);
            };
            if parent_id == ancestor_id {
                return Ok(true);
            }
            current_id = parent_id;
        }
        Err(BrowserBackendError::SelectionFailed {
            reason: "native frame ancestry exceeded its bounded depth".into(),
        })
    }

    fn frame_origin(&self, frame_id: &str) -> Result<Option<NativeOrigin>, BrowserBackendError> {
        validate_native_topology_id(frame_id)?;
        let targets = self.lock_targets(BackendOperation::Script)?;
        let engine = self.lock_engine_raw(BackendOperation::Script)?;
        if targets.active_target_id.is_none() {
            return Ok(None);
        }
        if targets.active_frames.active_frame_id == frame_id {
            return Ok(Some(engine.snapshot().map_err(native_error)?.origin));
        }
        if let Some(frame) = targets.active_frames.parked.get(frame_id) {
            return Ok(Some(frame.engine.snapshot().map_err(native_error)?.origin));
        }
        for target in targets.parked.values() {
            if target.frames.active_frame_id == frame_id {
                return Ok(Some(target.engine.snapshot().map_err(native_error)?.origin));
            }
            if let Some(frame) = target.frames.parked.get(frame_id) {
                return Ok(Some(frame.engine.snapshot().map_err(native_error)?.origin));
            }
        }
        Ok(None)
    }

    async fn process_pending_frame_scripts(
        &self,
        requests: Vec<NativeFrameScriptRequest>,
    ) -> Result<(), BrowserBackendError> {
        let mut pending = VecDeque::from(requests);
        let mut processed = 0usize;
        while let Some(request) = pending.pop_front() {
            processed = processed.saturating_add(1);
            if processed > MAX_NATIVE_EFFECTS {
                return Err(BrowserBackendError::SelectionFailed {
                    reason: "native same-origin frame-script cascade exceeded its bounded limit"
                        .into(),
                });
            }
            let frame_id = request.frame_id.clone();
            let route = self.frame_route(&frame_id)?.ok_or_else(|| {
                BrowserBackendError::SelectionFailed {
                    reason: "native frame disappeared before same-origin script routing".into(),
                }
            })?;
            let source_frame_id = request.source_frame_id.clone();
            match request.command.as_ref() {
                NativeScriptCommand::AdoptNode { .. } => {
                    self.apply_frame_node_adoption(route, request).await?;
                    continue;
                }
                NativeScriptCommand::AdoptedNodeCommand { .. } => {
                    let (nested, effects, event_effects) =
                        self.apply_adopted_node_command(request).await?;
                    self.process_frame_event_effects(&source_frame_id, event_effects)
                        .await?;
                    Box::pin(self.process_pending_browser_effects(
                        effects.0, effects.1, effects.2, effects.3, effects.4, effects.5, effects.6,
                    ))
                    .await?;
                    pending.extend(nested);
                    continue;
                }
                _ => {}
            }
            let (nested, effects, event_effects) =
                self.apply_frame_script_to_frame(route, request).await?;
            self.process_frame_event_effects(&source_frame_id, event_effects)
                .await?;
            Box::pin(self.process_pending_browser_effects(
                effects.0, effects.1, effects.2, effects.3, effects.4, effects.5, effects.6,
            ))
            .await?;
            pending.extend(nested);
        }
        Ok(())
    }

    async fn apply_frame_node_adoption(
        &self,
        _destination_route: NativeFrameRoute,
        request: NativeFrameScriptRequest,
    ) -> Result<(), BrowserBackendError> {
        let NativeScriptCommand::AdoptNode {
            source_frame_id,
            destination_frame_id,
            source_generation,
            destination_generation,
            transfers,
        } = *request.command
        else {
            return Err(BrowserBackendError::InvalidConfiguration {
                field: "native node transfer command".into(),
                reason: "frame request did not contain an adoption command".into(),
            });
        };
        validate_native_topology_id(&source_frame_id)?;
        validate_native_topology_id(&destination_frame_id)?;
        validate_native_topology_id(&request.source_frame_id)?;
        if (destination_frame_id != request.frame_id && source_frame_id != request.frame_id)
            || transfers.is_empty()
        {
            return Err(BrowserBackendError::SelectionFailed {
                reason: "native node transfer target or identity map is invalid".into(),
            });
        }

        let caller_origin = self
            .frame_origin(&request.source_frame_id)?
            .ok_or_else(|| BrowserBackendError::SelectionFailed {
                reason: "native node transfer caller disappeared before routing".into(),
            })?;
        let source_origin = self.frame_origin(&source_frame_id)?.ok_or_else(|| {
            BrowserBackendError::SelectionFailed {
                reason: "native node transfer source disappeared before routing".into(),
            }
        })?;
        let destination_origin = self.frame_origin(&destination_frame_id)?.ok_or_else(|| {
            BrowserBackendError::SelectionFailed {
                reason: "native node transfer destination disappeared before routing".into(),
            }
        })?;
        if caller_origin == NativeOrigin::Opaque
            || source_origin == NativeOrigin::Opaque
            || destination_origin == NativeOrigin::Opaque
            || caller_origin != source_origin
            || source_origin != destination_origin
        {
            return Err(BrowserBackendError::UnsupportedOperation {
                operation: "same-origin frame node adoption".into(),
                reason: "cross-origin native node transfer was rejected".into(),
            });
        }

        self.transfer_native_node_between_frames(
            &request.source_frame_id,
            &source_frame_id,
            &destination_frame_id,
            source_generation,
            destination_generation,
            &transfers,
        )
        .await
    }

    async fn apply_adopted_node_command(
        &self,
        request: NativeFrameScriptRequest,
    ) -> Result<
        (
            Vec<NativeFrameScriptRequest>,
            NativeQueuedBrowserEffects,
            Vec<NativeEffect>,
        ),
        BrowserBackendError,
    > {
        let NativeScriptCommand::AdoptedNodeCommand {
            destination_frame_id,
            command,
        } = *request.command
        else {
            return Err(BrowserBackendError::InvalidConfiguration {
                field: "adopted native node command".into(),
                reason: "frame request did not contain a routed node command".into(),
            });
        };
        validate_native_topology_id(&destination_frame_id)?;
        if request.frame_id == destination_frame_id
            || matches!(
                command.as_ref(),
                NativeScriptCommand::AdoptNode { .. }
                    | NativeScriptCommand::AdoptedNodeCommand { .. }
                    | NativeScriptCommand::FrameScript { .. }
            )
        {
            return Err(BrowserBackendError::SelectionFailed {
                reason: "adopted native node relay target or command is invalid".into(),
            });
        }
        let caller_origin = self
            .frame_origin(&request.source_frame_id)?
            .ok_or_else(|| BrowserBackendError::SelectionFailed {
                reason: "adopted native node caller disappeared before routing".into(),
            })?;
        let destination_origin = self.frame_origin(&destination_frame_id)?.ok_or_else(|| {
            BrowserBackendError::SelectionFailed {
                reason: "adopted native node owner disappeared before routing".into(),
            }
        })?;
        if caller_origin == NativeOrigin::Opaque || caller_origin != destination_origin {
            return Err(BrowserBackendError::UnsupportedOperation {
                operation: "adopted native node mutation".into(),
                reason: "cross-origin native node mutation was rejected".into(),
            });
        }
        {
            let targets = self.lock_targets(BackendOperation::Script)?;
            let caller_route = native_frame_route_in_state(&targets, &request.source_frame_id)
                .ok_or_else(|| BrowserBackendError::SelectionFailed {
                    reason: "adopted native node caller is no longer attached".into(),
                })?;
            let relay_route =
                native_frame_route_in_state(&targets, &request.frame_id).ok_or_else(|| {
                    BrowserBackendError::SelectionFailed {
                        reason: "adopted native node relay is no longer attached".into(),
                    }
                })?;
            let destination_route = native_frame_route_in_state(&targets, &destination_frame_id)
                .ok_or_else(|| BrowserBackendError::SelectionFailed {
                    reason: "adopted native node owner is no longer attached".into(),
                })?;
            let caller_owner = native_frame_route_owner_id(&targets, &caller_route);
            if caller_owner.is_none()
                || caller_owner != native_frame_route_owner_id(&targets, &relay_route)
                || caller_owner != native_frame_route_owner_id(&targets, &destination_route)
            {
                return Err(BrowserBackendError::UnsupportedOperation {
                    operation: "adopted native node mutation".into(),
                    reason: "owner, relay, and caller must share one browsing-context tree".into(),
                });
            }
        }
        let route = self.frame_route(&destination_frame_id)?.ok_or_else(|| {
            BrowserBackendError::SelectionFailed {
                reason: "adopted native node owner is no longer attached".into(),
            }
        })?;
        self.apply_frame_script_to_frame(
            route,
            NativeFrameScriptRequest {
                frame_id: destination_frame_id,
                source_frame_id: request.source_frame_id,
                command,
            },
        )
        .await
    }

    async fn transfer_native_node_between_frames(
        &self,
        caller_frame_id: &str,
        source_frame_id: &str,
        destination_frame_id: &str,
        source_generation: u32,
        destination_generation: u32,
        transfers: &[NativeNodeSubtreeTransfer],
    ) -> Result<(), BrowserBackendError> {
        let mut targets = self.lock_targets(BackendOperation::Script)?;
        let caller_route =
            native_frame_route_in_state(&targets, caller_frame_id).ok_or_else(|| {
                BrowserBackendError::SelectionFailed {
                    reason: "native node transfer caller frame is no longer attached".into(),
                }
            })?;
        let source_route =
            native_frame_route_in_state(&targets, source_frame_id).ok_or_else(|| {
                BrowserBackendError::SelectionFailed {
                    reason: "native node transfer source frame is no longer attached".into(),
                }
            })?;
        let destination_route = native_frame_route_in_state(&targets, destination_frame_id)
            .ok_or_else(|| BrowserBackendError::SelectionFailed {
                reason: "native node transfer destination frame is no longer attached".into(),
            })?;
        let caller_owner = native_frame_route_owner_id(&targets, &caller_route);
        let source_owner = native_frame_route_owner_id(&targets, &source_route);
        let destination_owner = native_frame_route_owner_id(&targets, &destination_route);
        if caller_owner.is_none()
            || caller_owner != source_owner
            || source_owner != destination_owner
            || source_frame_id == destination_frame_id
        {
            return Err(BrowserBackendError::UnsupportedOperation {
                operation: "same-tree native node adoption".into(),
                reason: "native node transfer requires live owners in one top-level browsing tree"
                    .into(),
            });
        }

        let result = match (&source_route, &destination_route) {
            (NativeFrameRoute::ActiveSelected, NativeFrameRoute::ActiveParked)
            | (NativeFrameRoute::ActiveParked, NativeFrameRoute::ActiveSelected) => {
                let source_is_root = matches!(&source_route, NativeFrameRoute::ActiveSelected);
                let parked_frame_id = if source_is_root {
                    destination_frame_id
                } else {
                    source_frame_id
                };
                let mut engine = self.lock_engine_raw(BackendOperation::Script)?;
                let mut frame = targets
                    .active_frames
                    .parked
                    .remove(parked_frame_id)
                    .ok_or_else(|| BrowserBackendError::SelectionFailed {
                        reason: "native transfer frame disappeared while locking its owner".into(),
                    })?;
                let result = if source_is_root {
                    coordinate_native_document_adoption(
                        &mut engine,
                        &mut frame.engine,
                        source_frame_id,
                        destination_frame_id,
                        source_generation,
                        destination_generation,
                        transfers,
                    )
                    .await
                } else {
                    coordinate_native_document_adoption(
                        &mut frame.engine,
                        &mut engine,
                        source_frame_id,
                        destination_frame_id,
                        source_generation,
                        destination_generation,
                        transfers,
                    )
                    .await
                };
                targets
                    .active_frames
                    .parked
                    .insert(parked_frame_id.to_owned(), frame);
                result
            }
            (NativeFrameRoute::ActiveParked, NativeFrameRoute::ActiveParked) => {
                let mut source_frame = targets
                    .active_frames
                    .parked
                    .remove(source_frame_id)
                    .ok_or_else(|| BrowserBackendError::SelectionFailed {
                        reason: "native transfer source frame disappeared while locking its owner"
                            .into(),
                    })?;
                let Some(mut destination_frame) =
                    targets.active_frames.parked.remove(destination_frame_id)
                else {
                    targets
                        .active_frames
                        .parked
                        .insert(source_frame_id.to_owned(), source_frame);
                    return Err(BrowserBackendError::SelectionFailed {
                        reason:
                            "native transfer destination frame disappeared while locking its owner"
                                .into(),
                    });
                };
                let result = coordinate_native_document_adoption(
                    &mut source_frame.engine,
                    &mut destination_frame.engine,
                    source_frame_id,
                    destination_frame_id,
                    source_generation,
                    destination_generation,
                    transfers,
                )
                .await;
                targets
                    .active_frames
                    .parked
                    .insert(source_frame_id.to_owned(), source_frame);
                targets
                    .active_frames
                    .parked
                    .insert(destination_frame_id.to_owned(), destination_frame);
                result
            }
            (
                NativeFrameRoute::ParkedSelected {
                    target_id: source_target,
                },
                NativeFrameRoute::ParkedParked {
                    target_id: destination_target,
                },
            ) if source_target == destination_target => {
                let target = targets.parked.get_mut(source_target).ok_or_else(|| {
                    BrowserBackendError::SelectionFailed {
                        reason: "native transfer top-level target disappeared".into(),
                    }
                })?;
                let mut frame = target
                    .frames
                    .parked
                    .remove(destination_frame_id)
                    .ok_or_else(|| BrowserBackendError::SelectionFailed {
                        reason: "native transfer child frame disappeared while locking its owner"
                            .into(),
                    })?;
                let result = coordinate_native_document_adoption(
                    &mut target.engine,
                    &mut frame.engine,
                    source_frame_id,
                    destination_frame_id,
                    source_generation,
                    destination_generation,
                    transfers,
                )
                .await;
                target
                    .frames
                    .parked
                    .insert(destination_frame_id.to_owned(), frame);
                result
            }
            (
                NativeFrameRoute::ParkedParked {
                    target_id: source_target,
                },
                NativeFrameRoute::ParkedSelected {
                    target_id: destination_target,
                },
            ) if source_target == destination_target => {
                let target = targets.parked.get_mut(source_target).ok_or_else(|| {
                    BrowserBackendError::SelectionFailed {
                        reason: "native transfer top-level target disappeared".into(),
                    }
                })?;
                let mut frame = target
                    .frames
                    .parked
                    .remove(source_frame_id)
                    .ok_or_else(|| BrowserBackendError::SelectionFailed {
                        reason: "native transfer source frame disappeared while locking its owner"
                            .into(),
                    })?;
                let result = coordinate_native_document_adoption(
                    &mut frame.engine,
                    &mut target.engine,
                    source_frame_id,
                    destination_frame_id,
                    source_generation,
                    destination_generation,
                    transfers,
                )
                .await;
                target
                    .frames
                    .parked
                    .insert(source_frame_id.to_owned(), frame);
                result
            }
            (
                NativeFrameRoute::ParkedParked {
                    target_id: source_target,
                },
                NativeFrameRoute::ParkedParked {
                    target_id: destination_target,
                },
            ) if source_target == destination_target => {
                let target = targets.parked.get_mut(source_target).ok_or_else(|| {
                    BrowserBackendError::SelectionFailed {
                        reason: "native transfer top-level target disappeared".into(),
                    }
                })?;
                let mut source_frame =
                    target
                        .frames
                        .parked
                        .remove(source_frame_id)
                        .ok_or_else(|| BrowserBackendError::SelectionFailed {
                            reason:
                                "native transfer source frame disappeared while locking its owner"
                                    .into(),
                        })?;
                let Some(mut destination_frame) = target.frames.parked.remove(destination_frame_id)
                else {
                    target
                        .frames
                        .parked
                        .insert(source_frame_id.to_owned(), source_frame);
                    return Err(BrowserBackendError::SelectionFailed {
                        reason:
                            "native transfer destination frame disappeared while locking its owner"
                                .into(),
                    });
                };
                let result = coordinate_native_document_adoption(
                    &mut source_frame.engine,
                    &mut destination_frame.engine,
                    source_frame_id,
                    destination_frame_id,
                    source_generation,
                    destination_generation,
                    transfers,
                )
                .await;
                target
                    .frames
                    .parked
                    .insert(source_frame_id.to_owned(), source_frame);
                target
                    .frames
                    .parked
                    .insert(destination_frame_id.to_owned(), destination_frame);
                result
            }
            _ => Err(BrowserBackendError::UnsupportedOperation {
                operation: "same-tree native node adoption".into(),
                reason:
                    "native node transfer topology is unsupported or belongs to independent roots"
                        .into(),
            }),
        };
        result
    }

    async fn apply_frame_script_to_frame(
        &self,
        route: NativeFrameRoute,
        request: NativeFrameScriptRequest,
    ) -> Result<
        (
            Vec<NativeFrameScriptRequest>,
            NativeQueuedBrowserEffects,
            Vec<NativeEffect>,
        ),
        BrowserBackendError,
    > {
        let source_origin = self
            .frame_origin(&request.source_frame_id)?
            .ok_or_else(|| BrowserBackendError::SelectionFailed {
                reason: "native frame script source disappeared before routing".into(),
            })?;
        let target_origin = self.frame_origin(&request.frame_id)?.ok_or_else(|| {
            BrowserBackendError::SelectionFailed {
                reason: "native frame script target disappeared before routing".into(),
            }
        })?;
        if source_origin == NativeOrigin::Opaque || source_origin != target_origin {
            return Err(BrowserBackendError::UnsupportedOperation {
                operation: "same-origin frame script".into(),
                reason: "cross-origin frame DOM access was rejected".into(),
            });
        }

        let target_frame_id = request.frame_id.clone();
        let NativeFrameScriptRequest { command, .. } = request;
        let suppress_parent_events = parent_projected_event_kinds(&command);
        let (nested, effects, event_effects, owner_id, window_name) = match route {
            NativeFrameRoute::ActiveSelected => {
                let mut engine = self.lock_engine_raw(BackendOperation::Script)?;
                let event_effects = engine
                    .apply_frame_script_command_with_effects_async(*command)
                    .await
                    .map_err(native_error)?;
                let nested = engine.take_pending_frame_scripts();
                let (effects, window_name) = take_native_browser_effects(&mut engine);
                let owner_id = engine.config().context_id.clone();
                (nested, effects, event_effects, owner_id, window_name)
            }
            NativeFrameRoute::ActiveParked => {
                let mut targets = self.lock_targets(BackendOperation::Script)?;
                let mut frame = targets
                    .active_frames
                    .parked
                    .remove(&target_frame_id)
                    .ok_or_else(|| BrowserBackendError::SelectionFailed {
                        reason: "native frame disappeared before same-origin script routing".into(),
                    })?;
                let result = frame
                    .engine
                    .apply_frame_script_command_with_effects_async(*command)
                    .await
                    .map_err(native_error);
                let nested = frame.engine.take_pending_frame_scripts();
                let event_effects = match result {
                    Ok(event_effects) => event_effects,
                    Err(error) => {
                        targets.active_frames.parked.insert(target_frame_id, frame);
                        return Err(error);
                    }
                };
                let (effects, window_name) = take_native_browser_effects(&mut frame.engine);
                let owner_id = targets.active_target_id.clone().ok_or_else(|| {
                    BrowserBackendError::SelectionFailed {
                        reason: "native frame owner target disappeared during script routing"
                            .into(),
                    }
                })?;
                targets.active_frames.parked.insert(target_frame_id, frame);
                (nested, effects, event_effects, owner_id, window_name)
            }
            NativeFrameRoute::ParkedSelected { target_id } => {
                let mut targets = self.lock_targets(BackendOperation::Script)?;
                let target = targets.parked.get_mut(&target_id).ok_or_else(|| {
                    BrowserBackendError::SelectionFailed {
                        reason: "native frame owner target disappeared during script routing"
                            .into(),
                    }
                })?;
                let event_effects = target
                    .engine
                    .apply_frame_script_command_with_effects_async(*command)
                    .await
                    .map_err(native_error)?;
                let nested = target.engine.take_pending_frame_scripts();
                let (effects, window_name) = take_native_browser_effects(&mut target.engine);
                (nested, effects, event_effects, target_id, window_name)
            }
            NativeFrameRoute::ParkedParked { target_id } => {
                let mut targets = self.lock_targets(BackendOperation::Script)?;
                let target = targets.parked.get_mut(&target_id).ok_or_else(|| {
                    BrowserBackendError::SelectionFailed {
                        reason: "native frame owner target disappeared during script routing"
                            .into(),
                    }
                })?;
                let frame = target
                    .frames
                    .parked
                    .get_mut(&target_frame_id)
                    .ok_or_else(|| BrowserBackendError::SelectionFailed {
                        reason: "native frame disappeared before same-origin script routing".into(),
                    })?;
                let event_effects = frame
                    .engine
                    .apply_frame_script_command_with_effects_async(*command)
                    .await
                    .map_err(native_error)?;
                let nested = frame.engine.take_pending_frame_scripts();
                let (effects, window_name) = take_native_browser_effects(&mut frame.engine);
                (nested, effects, event_effects, target_id, window_name)
            }
        };
        self.sync_target_name(&owner_id, &window_name)?;
        let event_effects = event_effects
            .into_iter()
            .filter(|effect| !suppress_parent_events.contains(&effect.kind))
            .collect();
        Ok((nested, effects, event_effects))
    }

    async fn dispatch_frame_events_to_parent(
        &self,
        parent_id: &str,
        child_id: &str,
        effects: &[NativeEffect],
    ) -> Result<
        (
            Vec<NativeFrameScriptRequest>,
            NativeQueuedBrowserEffects,
            Vec<NativeEffect>,
        ),
        BrowserBackendError,
    > {
        let empty = NativeQueuedBrowserEffects::default;
        if effects.is_empty() || !self.frame_is_ancestor(parent_id, child_id)? {
            return Ok((Vec::new(), empty(), Vec::new()));
        }
        let Some(source_origin) = self.frame_origin(child_id)? else {
            return Ok((Vec::new(), empty(), Vec::new()));
        };
        let Some(parent_origin) = self.frame_origin(parent_id)? else {
            return Ok((Vec::new(), empty(), Vec::new()));
        };
        if source_origin == NativeOrigin::Opaque || source_origin != parent_origin {
            return Ok((Vec::new(), empty(), Vec::new()));
        }
        let route =
            self.frame_route(parent_id)?
                .ok_or_else(|| BrowserBackendError::SelectionFailed {
                    reason: "native frame parent disappeared before event projection".into(),
                })?;
        let child_frame_id = child_id.to_owned();
        let (nested, queued, propagated, owner_id, window_name) = match route {
            NativeFrameRoute::ActiveSelected => {
                let targets = self.lock_targets(BackendOperation::Script)?;
                let mut engine = self.lock_engine_raw(BackendOperation::Script)?;
                let parent_origin = engine.snapshot().map_err(native_error)?.origin;
                let bindings = native_frame_script_children(
                    &targets.active_frames,
                    &engine,
                    parent_id,
                    &parent_origin,
                )?;
                engine.set_frame_script_bindings(bindings);
                let (nested, queued, propagated) =
                    dispatch_frame_events_to_parent_engine(&mut engine, &child_frame_id, effects)
                        .await?;
                let window_name = engine.config().window_name.clone();
                let owner_id = targets.active_target_id.clone().ok_or_else(|| {
                    BrowserBackendError::SelectionFailed {
                        reason: "native frame owner target disappeared during event projection"
                            .into(),
                    }
                })?;
                (nested, queued, propagated, owner_id, window_name)
            }
            NativeFrameRoute::ActiveParked => {
                let mut targets = self.lock_targets(BackendOperation::Script)?;
                let active_engine = self.lock_engine_raw(BackendOperation::Script)?;
                let parent_origin = targets
                    .active_frames
                    .parked
                    .get(parent_id)
                    .ok_or_else(|| BrowserBackendError::SelectionFailed {
                        reason: "native frame parent disappeared before event projection".into(),
                    })?
                    .engine
                    .snapshot()
                    .map_err(native_error)?
                    .origin;
                let bindings = native_frame_script_children(
                    &targets.active_frames,
                    &active_engine,
                    parent_id,
                    &parent_origin,
                )?;
                let parent = targets
                    .active_frames
                    .parked
                    .get_mut(parent_id)
                    .ok_or_else(|| BrowserBackendError::SelectionFailed {
                        reason: "native frame parent disappeared before event projection".into(),
                    })?;
                parent.engine.set_frame_script_bindings(bindings);
                let (nested, queued, propagated) = dispatch_frame_events_to_parent_engine(
                    &mut parent.engine,
                    &child_frame_id,
                    effects,
                )
                .await?;
                let window_name = parent.engine.config().window_name.clone();
                let owner_id = targets.active_target_id.clone().ok_or_else(|| {
                    BrowserBackendError::SelectionFailed {
                        reason: "native frame owner target disappeared during event projection"
                            .into(),
                    }
                })?;
                (nested, queued, propagated, owner_id, window_name)
            }
            NativeFrameRoute::ParkedSelected { target_id } => {
                let mut targets = self.lock_targets(BackendOperation::Script)?;
                let target = targets.parked.get_mut(&target_id).ok_or_else(|| {
                    BrowserBackendError::SelectionFailed {
                        reason: "native frame owner target disappeared during event projection"
                            .into(),
                    }
                })?;
                let parent_origin = target.engine.snapshot().map_err(native_error)?.origin;
                let bindings = native_frame_script_children(
                    &target.frames,
                    &target.engine,
                    parent_id,
                    &parent_origin,
                )?;
                target.engine.set_frame_script_bindings(bindings);
                let (nested, queued, propagated) = dispatch_frame_events_to_parent_engine(
                    &mut target.engine,
                    &child_frame_id,
                    effects,
                )
                .await?;
                let window_name = target.engine.config().window_name.clone();
                (nested, queued, propagated, target_id, window_name)
            }
            NativeFrameRoute::ParkedParked { target_id } => {
                let mut targets = self.lock_targets(BackendOperation::Script)?;
                let target = targets.parked.get_mut(&target_id).ok_or_else(|| {
                    BrowserBackendError::SelectionFailed {
                        reason: "native frame owner target disappeared during event projection"
                            .into(),
                    }
                })?;
                let parent_origin = target
                    .frames
                    .parked
                    .get(parent_id)
                    .ok_or_else(|| BrowserBackendError::SelectionFailed {
                        reason: "native frame parent disappeared before event projection".into(),
                    })?
                    .engine
                    .snapshot()
                    .map_err(native_error)?
                    .origin;
                let bindings = native_frame_script_children(
                    &target.frames,
                    &target.engine,
                    parent_id,
                    &parent_origin,
                )?;
                let parent = target.frames.parked.get_mut(parent_id).ok_or_else(|| {
                    BrowserBackendError::SelectionFailed {
                        reason: "native frame parent disappeared during event projection".into(),
                    }
                })?;
                parent.engine.set_frame_script_bindings(bindings);
                let (nested, queued, propagated) = dispatch_frame_events_to_parent_engine(
                    &mut parent.engine,
                    &child_frame_id,
                    effects,
                )
                .await?;
                let window_name = parent.engine.config().window_name.clone();
                (nested, queued, propagated, target_id, window_name)
            }
        };
        self.sync_target_name(&owner_id, &window_name)?;
        Ok((nested, queued, propagated))
    }

    async fn process_frame_event_effects(
        &self,
        source_frame_id: &str,
        effects: Vec<NativeEffect>,
    ) -> Result<(), BrowserBackendError> {
        let Some(first_parent_id) = self.frame_parent_id(source_frame_id)? else {
            return Ok(());
        };
        let mut pending_events =
            VecDeque::from([(source_frame_id.to_owned(), first_parent_id, effects, true)]);
        let mut processed = 0usize;
        let mut pending_scripts = Vec::new();
        let mut pending_browser = Vec::new();
        while let Some((source_id, parent_id, effects, forward_original)) =
            pending_events.pop_front()
        {
            if effects.is_empty() {
                continue;
            }
            processed = processed.saturating_add(1);
            if processed > MAX_NATIVE_EFFECTS.saturating_mul(NATIVE_MAX_FRAMES) {
                return Err(BrowserBackendError::SelectionFailed {
                    reason: "native frame event propagation exceeded its bounded work limit".into(),
                });
            }
            let (nested, queued, parent_effects) = self
                .dispatch_frame_events_to_parent(&parent_id, &source_id, &effects)
                .await?;
            pending_scripts.extend(nested);
            pending_browser.push(queued);
            if let Some(next_parent_id) = self.frame_parent_id(&parent_id)? {
                if !parent_effects.is_empty() {
                    pending_events.push_front((
                        parent_id.clone(),
                        next_parent_id.clone(),
                        parent_effects,
                        false,
                    ));
                }
                if forward_original {
                    pending_events.push_back((source_id, next_parent_id, effects, true));
                }
            }
        }
        Box::pin(self.process_pending_frame_scripts(pending_scripts)).await?;
        for queued in pending_browser {
            Box::pin(self.process_pending_browser_effects(
                queued.0, queued.1, queued.2, queued.3, queued.4, queued.5, queued.6,
            ))
            .await?;
        }
        Ok(())
    }

    async fn process_native_frame_lifecycle_effects(
        &self,
        owner_id: &str,
        effects: Vec<NativeFrameLifecycleEffects>,
    ) -> Result<NativeQueuedBrowserEffects, BrowserBackendError> {
        let mut browser = NativeQueuedBrowserEffects::default();
        for effect in effects {
            self.sync_target_name(owner_id, &effect.runtime.window_name)?;
            self.process_frame_event_effects(&effect.frame_id, effect.runtime.events)
                .await?;
            Box::pin(self.process_pending_frame_scripts(effect.runtime.frame_scripts)).await?;
            append_native_queued_browser_effects(&mut browser, effect.runtime.browser);
        }
        Ok(browser)
    }

    async fn process_selected_frame_events(
        &self,
        frame_id: &str,
        effects: Vec<NativeEffect>,
    ) -> Result<(), BrowserBackendError> {
        self.process_frame_event_effects(frame_id, effects).await
    }

    pub fn active_frame_id(&self) -> Result<String, BrowserBackendError> {
        let targets = self.lock_targets(BackendOperation::Contexts)?;
        if targets.active_target_id.is_none() {
            return Err(BrowserBackendError::Lifecycle {
                operation: "contexts".into(),
                state: "no-target-selected".into(),
                reason: "select an available native page target before frame discovery".into(),
            });
        }
        Ok(targets.active_frames.active_frame_id.clone())
    }

    pub fn select_target(&self, target_id: &str) -> Result<PageTargetInfo, BrowserBackendError> {
        validate_native_topology_id(target_id)?;
        let mut targets = self.lock_targets(BackendOperation::Contexts)?;
        if targets.active_target_id.as_deref() == Some(target_id) {
            let engine = self.lock_engine_raw(BackendOperation::Contexts)?;
            return project_native_target(
                &engine,
                target_id,
                targets.active_opener_id.clone(),
                true,
            );
        }

        let Some(parked) = targets.parked.remove(target_id) else {
            return Err(BrowserBackendError::SelectionFailed {
                reason: "native page target was not found; call listTargets to refresh topology"
                    .into(),
            });
        };
        if let Err(error) = parked.engine.context() {
            targets.parked.insert(target_id.to_owned(), parked);
            return Err(native_error(error));
        }
        let NativeParkedTarget {
            engine: parked_engine,
            opener_id,
            frames: parked_frames,
            name,
        } = parked;
        let mut active_engine = self.lock_engine_raw(BackendOperation::Contexts)?;
        let old_engine = std::mem::replace(&mut *active_engine, parked_engine);
        let old_frames = std::mem::replace(&mut targets.active_frames, parked_frames);
        let old_target_id = targets.active_target_id.replace(target_id.to_owned());
        let old_opener_id = std::mem::replace(&mut targets.active_opener_id, opener_id);
        let old_name = std::mem::replace(&mut targets.active_name, name);
        if let Some(old_target_id) = old_target_id {
            targets.parked.insert(
                old_target_id,
                NativeParkedTarget {
                    engine: old_engine,
                    opener_id: old_opener_id,
                    frames: old_frames,
                    name: old_name,
                },
            );
        }
        project_native_target(
            &active_engine,
            target_id,
            targets.active_opener_id.clone(),
            true,
        )
    }

    /// Create and initialize a new independent native page target. The new
    /// target is intentionally not selected, matching the public target API.
    pub async fn create_target(&self, url: &str) -> Result<PageTargetInfo, BrowserBackendError> {
        let navigation = NativeNavigationRequest::get(url);
        let (
            target,
            popups,
            messages,
            closes,
            navigations,
            service_worker_open_windows,
            service_worker_client_messages,
            worker_coordinator_effects,
        ) = self.create_target_named(&navigation, None, None).await?;
        self.process_pending_browser_effects(
            popups,
            messages,
            closes,
            navigations,
            service_worker_open_windows,
            service_worker_client_messages,
            worker_coordinator_effects,
        )
        .await?;
        Ok(target)
    }

    async fn create_target_named(
        &self,
        navigation: &NativeNavigationRequest,
        name: Option<String>,
        opener_id: Option<String>,
    ) -> Result<
        (
            PageTargetInfo,
            Vec<NativePopupRequest>,
            Vec<NativePostMessageRequest>,
            Vec<NativeWindowCloseRequest>,
            Vec<NativeWindowNavigationRequest>,
            Vec<NativeServiceWorkerOpenWindowRequest>,
            Vec<NativeServiceWorkerClientMessage>,
            Vec<NativeWorkerCoordinatorEffect>,
        ),
        BrowserBackendError,
    > {
        if let Some(name) = name.as_deref() {
            validate_native_popup_name(name)?;
        }
        let (base_config, active_url) = {
            let engine = self
                .engine
                .lock()
                .map_err(|_| poisoned_lock_error(BackendOperation::Contexts, "engine"))?;
            let active_url = engine.context().map_err(native_error)?.url;
            (engine.config().clone(), active_url)
        };
        let (target_id, opener_id, opener_window_name, opener_url) = {
            let mut targets = self.lock_targets(BackendOperation::Contexts)?;
            if targets.target_count() >= NATIVE_MAX_TARGETS {
                return Err(BrowserBackendError::SelectionFailed {
                    reason: format!("native target limit reached ({NATIVE_MAX_TARGETS})"),
                });
            }
            let target_id = targets.next_target_id();
            let opener_id = opener_id.or_else(|| targets.active_target_id.clone());
            let opener_window_name = opener_id
                .as_deref()
                .and_then(|opener_id| {
                    if targets.active_target_id.as_deref() == Some(opener_id) {
                        targets.active_name.clone()
                    } else {
                        targets
                            .parked
                            .get(opener_id)
                            .and_then(|target| target.name.clone())
                    }
                })
                .unwrap_or_default();
            let opener_url = if let Some(opener_id) = opener_id.as_deref() {
                if targets.active_target_id.as_deref() == Some(opener_id) {
                    active_url.clone()
                } else if let Some(target) = targets.parked.get(opener_id) {
                    target.engine.context().map_err(native_error)?.url
                } else {
                    String::new()
                }
            } else {
                String::new()
            };
            (target_id, opener_id, opener_window_name, opener_url)
        };
        let initial_window_name = name.clone().unwrap_or_default();
        let initial_url = if navigation.method == NativeNavigationMethod::Post
            || navigation.object_url.is_some()
        {
            "about:blank"
        } else {
            navigation.url.as_str()
        };
        let config = base_config
            .with_context_id(target_id.clone())
            .with_opener_window_name(opener_window_name)
            .with_opener_url(opener_url)
            .with_window_name(initial_window_name)
            .with_initial_url(initial_url.to_owned());
        let config = if let Some(opener_id) = opener_id.as_deref() {
            config.with_opener_context_id(opener_id.to_owned())
        } else {
            config
        };
        let dialog_control = self.dialog_control.clone();
        let navigation = navigation.clone();
        let task_target_id = target_id.clone();
        let task_opener_id = opener_id.clone();
        let (
            mut engine,
            target,
            target_name,
            nested,
            nested_messages,
            nested_window_closes,
            nested_window_navigations,
            nested_service_worker_open_windows,
            nested_service_worker_client_messages,
            nested_page_message_port_commands,
        ) = tokio::spawn(async move {
            let mut engine = NativeEngine::new_with_browser_shared_workers(config, dialog_control)
                .map_err(native_error)?;
            if let Err(error) = engine.initialize_async().await {
                let _ = engine.close_async().await;
                return Err(native_error(error));
            }
            if navigation.method == NativeNavigationMethod::Post || navigation.object_url.is_some()
            {
                if let Err(error) = engine.navigate_request_async(navigation, 0).await {
                    let _ = engine.close_async().await;
                    return Err(native_error(error));
                }
            }
            let nested = engine.take_pending_popups();
            let nested_messages = engine.take_pending_post_messages();
            let nested_window_closes = engine.take_pending_window_closes();
            let nested_window_navigations = engine.take_pending_window_navigations();
            let nested_service_worker_open_windows =
                engine.take_pending_service_worker_open_windows();
            let nested_service_worker_client_messages =
                engine.take_pending_service_worker_client_messages();
            let nested_page_message_port_commands =
                take_native_worker_coordinator_effects(&mut engine);
            let target_name = native_window_name(&engine.config().window_name);
            let target = match project_native_target(
                &engine,
                &task_target_id,
                task_opener_id.clone(),
                false,
            ) {
                Ok(target) => target,
                Err(error) => {
                    let _ = engine.close_async().await;
                    return Err(error);
                }
            };
            Ok::<_, BrowserBackendError>((
                engine,
                target,
                target_name,
                nested,
                nested_messages,
                nested_window_closes,
                nested_window_navigations,
                nested_service_worker_open_windows,
                nested_service_worker_client_messages,
                nested_page_message_port_commands,
            ))
        })
        .await
        .map_err(|_| BrowserBackendError::Lifecycle {
            operation: "create native page target".into(),
            state: "initialization-task-failed".into(),
            reason: "target initialization task terminated before returning".into(),
        })??;
        let mut targets = self.lock_targets(BackendOperation::Contexts)?;
        if targets.target_count() >= NATIVE_MAX_TARGETS || targets.parked.contains_key(&target_id) {
            let _ = engine.close_async().await;
            return Err(BrowserBackendError::SelectionFailed {
                reason: format!("native target limit reached ({NATIVE_MAX_TARGETS})"),
            });
        }
        targets.parked.insert(
            target_id.clone(),
            NativeParkedTarget {
                engine,
                opener_id,
                frames: NativeFrameState::new(&target_id),
                name: target_name,
            },
        );
        Ok((
            target,
            nested,
            nested_messages,
            nested_window_closes,
            nested_window_navigations,
            nested_service_worker_open_windows,
            nested_service_worker_client_messages,
            nested_page_message_port_commands,
        ))
    }

    /// Click a native target and require the action to create exactly one
    /// causally-owned page target. The returned popup remains parked and the
    /// opener remains selected, matching the shared popup contract.
    pub async fn click_expect_popup(
        &self,
        target: &str,
    ) -> Result<(ActionResult, PageTargetInfo), BrowserBackendError> {
        let preflight = self
            .preflight_target(target, NativePreflightAction::Click)
            .await?;
        self.click_expect_popup_in_frame(target, preflight.frame_id.as_deref())
            .await
    }

    /// Click one exact frame-local target and require exactly one popup. The
    /// frame ID normally comes from the same fresh preflight used by the
    /// caller, so a child-frame target cannot be accidentally sent to the
    /// active root document.
    pub async fn click_expect_popup_in_frame(
        &self,
        target: &str,
        frame_id: Option<&str>,
    ) -> Result<(ActionResult, PageTargetInfo), BrowserBackendError> {
        let frame_id = frame_id.ok_or_else(|| BrowserBackendError::UnsupportedOperation {
            operation: "clickExpectPopup".into(),
            reason: "popup click target did not resolve to one native frame".into(),
        })?;
        let route =
            self.frame_route(frame_id)?
                .ok_or_else(|| BrowserBackendError::SelectionFailed {
                    reason: "native popup click frame disappeared before dispatch".into(),
                })?;
        let proxy_updates = self.window_proxy_updates(frame_id)?;
        let (revision, accepted, runtime_effects, owner_id) = self
            .apply_action_to_native_frame(
                route,
                frame_id,
                NativeAction::Click {
                    target: target.to_owned(),
                },
                &proxy_updates,
            )
            .await?;
        let popup_count = runtime_effects.browser.0.len();
        if popup_count != 1 {
            return Err(BrowserBackendError::UnsupportedOperation {
                operation: "clickExpectPopup".into(),
                reason: if popup_count == 0 {
                    "click did not create a new native page target".into()
                } else {
                    format!(
                        "click created {popup_count} native page targets; exactly one is required"
                    )
                },
            });
        }
        self.sync_target_name(&owner_id, &runtime_effects.window_name)?;
        if accepted {
            self.set_active_frame_focus(frame_id)?;
        }
        self.process_selected_frame_events(frame_id, runtime_effects.events)
            .await?;
        self.process_pending_frame_scripts(runtime_effects.frame_scripts)
            .await?;
        let mut created = self
            .process_pending_browser_effects(
                runtime_effects.browser.0,
                runtime_effects.browser.1,
                runtime_effects.browser.2,
                runtime_effects.browser.3,
                runtime_effects.browser.4,
                runtime_effects.browser.5,
                runtime_effects.browser.6,
            )
            .await?;
        let popup = created
            .pop()
            .expect("popup URL count was validated before materialization");
        Ok((
            ActionResult {
                context_id: owner_id,
                revision,
                accepted,
            },
            popup,
        ))
    }

    async fn process_pending_browser_effects(
        &self,
        popup_requests: Vec<NativePopupRequest>,
        post_messages: Vec<NativePostMessageRequest>,
        window_closes: Vec<NativeWindowCloseRequest>,
        window_navigations: Vec<NativeWindowNavigationRequest>,
        service_worker_open_window_requests: Vec<NativeServiceWorkerOpenWindowRequest>,
        service_worker_client_messages: Vec<NativeServiceWorkerClientMessage>,
        worker_coordinator_effects: Vec<NativeWorkerCoordinatorEffect>,
    ) -> Result<Vec<PageTargetInfo>, BrowserBackendError> {
        let mut pending_popups = VecDeque::from(popup_requests);
        let mut pending_messages = VecDeque::from(post_messages);
        let mut pending_window_closes = VecDeque::from(window_closes);
        let mut pending_window_navigations = VecDeque::from(window_navigations);
        let mut pending_service_worker_open_windows =
            VecDeque::from(service_worker_open_window_requests);
        let mut pending_service_worker_client_messages =
            VecDeque::from(service_worker_client_messages);
        let mut pending_worker_coordinator_effects = VecDeque::from(worker_coordinator_effects);
        let mut created: Vec<PageTargetInfo> = Vec::new();
        let mut next_source = *self
            .browser_effect_cursor
            .lock()
            .map_err(|_| poisoned_lock_error(BackendOperation::Effects, "browser effect"))?;
        let mut processed = 0usize;
        while !pending_popups.is_empty()
            || !pending_messages.is_empty()
            || !pending_window_closes.is_empty()
            || !pending_window_navigations.is_empty()
            || !pending_service_worker_open_windows.is_empty()
            || !pending_service_worker_client_messages.is_empty()
            || !pending_worker_coordinator_effects.is_empty()
        {
            processed = processed.saturating_add(1);
            if processed > NATIVE_MAX_TARGETS.saturating_mul(8) {
                return Err(BrowserBackendError::SelectionFailed {
                    reason: "native browser-effect cascade exceeded its bounded limit".into(),
                });
            }
            let Some(source) = next_ready_native_browser_effect_source(
                &mut next_source,
                !pending_popups.is_empty(),
                !pending_messages.is_empty(),
                !pending_window_navigations.is_empty(),
                !pending_window_closes.is_empty(),
                !pending_service_worker_open_windows.is_empty(),
                !pending_service_worker_client_messages.is_empty(),
                !pending_worker_coordinator_effects.is_empty(),
            ) else {
                break;
            };
            match source {
                NativeBrowserEffectSource::Popup => {
                    let request = pending_popups
                        .pop_front()
                        .expect("popup queue is non-empty after source selection");
                    let navigation = native_navigation_request_from_popup(&request)?;
                    let name = native_popup_name(&request.target);
                    if let Some(name) = name.as_deref()
                        && let Some((target_id, active)) = self.target_named(name)?
                    {
                        let (
                            _,
                            nested,
                            nested_messages,
                            nested_window_closes,
                            nested_window_navigations,
                            nested_service_worker_open_windows,
                            nested_service_worker_client_messages,
                            nested_page_message_port_commands,
                        ) = self
                            .navigate_named_target(&target_id, active, &navigation)
                            .await?;
                        self.bind_window_handle(&request, &target_id)?;
                        pending_popups.extend(nested);
                        pending_messages.extend(nested_messages);
                        pending_window_closes.extend(nested_window_closes);
                        pending_window_navigations.extend(nested_window_navigations);
                        pending_service_worker_open_windows
                            .extend(nested_service_worker_open_windows);
                        pending_service_worker_client_messages
                            .extend(nested_service_worker_client_messages);
                        pending_worker_coordinator_effects
                            .extend(nested_page_message_port_commands);
                        continue;
                    }
                    match self
                        .create_target_named(
                            &navigation,
                            name,
                            Some(request.source_context_id.clone()),
                        )
                        .await
                    {
                        Ok((
                            target,
                            nested,
                            nested_messages,
                            nested_window_closes,
                            nested_window_navigations,
                            nested_service_worker_open_windows,
                            nested_service_worker_client_messages,
                            nested_page_message_port_commands,
                        )) => {
                            self.bind_window_handle(&request, &target.id)?;
                            created.push(target);
                            pending_popups.extend(nested);
                            pending_messages.extend(nested_messages);
                            pending_window_closes.extend(nested_window_closes);
                            pending_window_navigations.extend(nested_window_navigations);
                            pending_service_worker_open_windows
                                .extend(nested_service_worker_open_windows);
                            pending_service_worker_client_messages
                                .extend(nested_service_worker_client_messages);
                            pending_worker_coordinator_effects
                                .extend(nested_page_message_port_commands);
                        }
                        Err(error) => {
                            for target in created {
                                let _ = self.close_target(&target.id).await;
                            }
                            return Err(error);
                        }
                    }
                }
                NativeBrowserEffectSource::Message => {
                    let message = pending_messages
                        .pop_front()
                        .expect("message queue is non-empty after source selection");
                    let (
                        nested_popups,
                        nested_messages,
                        nested_window_closes,
                        nested_window_navigations,
                        nested_service_worker_open_windows,
                        nested_service_worker_client_messages,
                        nested_page_message_port_commands,
                    ) = self.deliver_post_message(message).await?;
                    pending_popups.extend(nested_popups);
                    pending_messages.extend(nested_messages);
                    pending_window_closes.extend(nested_window_closes);
                    pending_window_navigations.extend(nested_window_navigations);
                    pending_service_worker_open_windows.extend(nested_service_worker_open_windows);
                    pending_service_worker_client_messages
                        .extend(nested_service_worker_client_messages);
                    pending_worker_coordinator_effects.extend(nested_page_message_port_commands);
                }
                NativeBrowserEffectSource::Navigation => {
                    let navigation = pending_window_navigations
                        .pop_front()
                        .expect("navigation queue is non-empty after source selection");
                    let navigation_request = native_navigation_request_from_window(&navigation)?;
                    if let Some(frame_id) = navigation.target_context_id.as_deref()
                        && let Some(route) = self.frame_route(frame_id)?
                    {
                        let (
                            nested_popups,
                            nested_messages,
                            nested_window_closes,
                            nested_window_navigations,
                            nested_service_worker_open_windows,
                            nested_service_worker_client_messages,
                            nested_page_message_port_commands,
                        ) = Box::pin(self.navigate_frame_target(
                            route,
                            frame_id,
                            &navigation.source_context_id,
                            &navigation_request,
                        ))
                        .await?;
                        pending_popups.extend(nested_popups);
                        pending_messages.extend(nested_messages);
                        pending_window_closes.extend(nested_window_closes);
                        pending_window_navigations.extend(nested_window_navigations);
                        pending_service_worker_open_windows
                            .extend(nested_service_worker_open_windows);
                        pending_service_worker_client_messages
                            .extend(nested_service_worker_client_messages);
                        pending_worker_coordinator_effects
                            .extend(nested_page_message_port_commands);
                        continue;
                    }
                    let Some((target_id, active)) = self.window_navigation_target(&navigation)?
                    else {
                        continue;
                    };
                    let (
                        _,
                        nested,
                        nested_messages,
                        nested_closes,
                        nested_navigations,
                        nested_service_worker_open_windows,
                        nested_service_worker_client_messages,
                        nested_page_message_port_commands,
                    ) = self
                        .navigate_named_target(&target_id, active, &navigation_request)
                        .await?;
                    pending_popups.extend(nested);
                    pending_messages.extend(nested_messages);
                    pending_window_closes.extend(nested_closes);
                    pending_window_navigations.extend(nested_navigations);
                    pending_service_worker_open_windows.extend(nested_service_worker_open_windows);
                    pending_service_worker_client_messages
                        .extend(nested_service_worker_client_messages);
                    pending_worker_coordinator_effects.extend(nested_page_message_port_commands);
                }
                NativeBrowserEffectSource::Close => {
                    let request = pending_window_closes
                        .pop_front()
                        .expect("close queue is non-empty after source selection");
                    if let Some((target_id, _)) = self.window_close_target(&request)? {
                        self.close_target(&target_id).await?;
                        created.retain(|target| target.id != target_id);
                    }
                }
                NativeBrowserEffectSource::ServiceWorkerOpenWindow => {
                    let request = pending_service_worker_open_windows.pop_front().expect(
                        "service worker openWindow queue is non-empty after source selection",
                    );
                    let (
                        target,
                        nested,
                        nested_messages,
                        nested_window_closes,
                        nested_window_navigations,
                        nested_service_worker_open_windows,
                        nested_service_worker_client_messages,
                        nested_page_message_port_commands,
                    ) = self
                        .create_target_named(
                            &NativeNavigationRequest::get(request.url.clone()),
                            None,
                            Some(request.source_context_id.clone()),
                        )
                        .await?;
                    self.synchronize_native_service_worker_clients().await?;
                    let window = {
                        let targets = self.lock_targets(BackendOperation::Contexts)?;
                        let parked = targets.parked.get(&target.id).ok_or_else(|| {
                            BrowserBackendError::SelectionFailed {
                                reason: "native service worker openWindow target disappeared before resolution".into(),
                            }
                        })?;
                        let client = parked
                            .engine
                            .service_worker_client_state("top-level", false);
                        serde_json::json!({
                            "clientId": client.id,
                            "clientUrl": client.url,
                            "clientType": client.client_type,
                            "frameType": client.frame_type,
                            "visibilityState": client.visibility_state,
                            "focused": client.focused,
                        })
                    };
                    let resolved = self
                        .resolve_service_worker_open_window(&request, window)
                        .await?;
                    created.push(target);
                    pending_popups.extend(nested);
                    pending_messages.extend(nested_messages);
                    pending_window_closes.extend(nested_window_closes);
                    pending_window_navigations.extend(nested_window_navigations);
                    pending_service_worker_open_windows.extend(nested_service_worker_open_windows);
                    pending_service_worker_client_messages
                        .extend(nested_service_worker_client_messages);
                    pending_worker_coordinator_effects.extend(nested_page_message_port_commands);
                    pending_popups.extend(resolved.0);
                    pending_messages.extend(resolved.1);
                    pending_window_closes.extend(resolved.2);
                    pending_window_navigations.extend(resolved.3);
                    pending_service_worker_open_windows.extend(resolved.4);
                    pending_service_worker_client_messages.extend(resolved.5);
                    pending_worker_coordinator_effects.extend(resolved.6);
                }
                NativeBrowserEffectSource::ServiceWorkerClientMessage => {
                    let message = pending_service_worker_client_messages.pop_front().expect(
                        "service worker client message queue is non-empty after source selection",
                    );
                    let nested = self.deliver_service_worker_client_message(message).await?;
                    pending_popups.extend(nested.0);
                    pending_messages.extend(nested.1);
                    pending_window_closes.extend(nested.2);
                    pending_window_navigations.extend(nested.3);
                    pending_service_worker_open_windows.extend(nested.4);
                    pending_service_worker_client_messages.extend(nested.5);
                    pending_worker_coordinator_effects.extend(nested.6);
                }
                NativeBrowserEffectSource::PageMessagePort => {
                    let effect = pending_worker_coordinator_effects.pop_front().expect(
                        "native worker coordinator effect queue is non-empty after source selection",
                    );
                    let nested = match effect {
                        NativeWorkerCoordinatorEffect::PageMessagePort(command) => {
                            if self.is_shared_worker_page_port(&command)? {
                                self.deliver_shared_worker_page_message_port(command)
                                    .await?
                            } else if command.close {
                                NativeQueuedBrowserEffects::default()
                            } else {
                                self.deliver_page_message_port(command).await?
                            }
                        }
                        NativeWorkerCoordinatorEffect::SharedWorkerMessage(command) => {
                            self.deliver_page_message_port(command).await?
                        }
                        NativeWorkerCoordinatorEffect::SharedWorkerCreate(request) => {
                            self.create_shared_worker_connection(request).await?
                        }
                        NativeWorkerCoordinatorEffect::SharedWorkerError { route, message } => {
                            self.deliver_shared_worker_error(route, message).await?
                        }
                        NativeWorkerCoordinatorEffect::SharedWorkerCookieChanges { changes } => {
                            self.deliver_shared_worker_cookie_changes(changes).await?
                        }
                    };
                    pending_popups.extend(nested.0);
                    pending_messages.extend(nested.1);
                    pending_window_closes.extend(nested.2);
                    pending_window_navigations.extend(nested.3);
                    pending_service_worker_open_windows.extend(nested.4);
                    pending_service_worker_client_messages.extend(nested.5);
                    pending_worker_coordinator_effects.extend(nested.6);
                }
            }
        }
        *self
            .browser_effect_cursor
            .lock()
            .map_err(|_| poisoned_lock_error(BackendOperation::Effects, "browser effect"))? =
            next_source;
        Ok(created)
    }

    async fn resolve_service_worker_open_window(
        &self,
        request: &NativeServiceWorkerOpenWindowRequest,
        window: serde_json::Value,
    ) -> Result<NativeQueuedBrowserEffects, BrowserBackendError> {
        if request.worker_id == 0 || request.request_id == 0 {
            return Err(BrowserBackendError::InvalidConfiguration {
                field: "service worker openWindow request".into(),
                reason: "worker and request ids must be positive".into(),
            });
        }
        validate_native_topology_id(&request.source_context_id)?;
        validate_native_topology_id(&request.source_frame_id)?;
        let route = self.frame_route(&request.source_frame_id)?.ok_or_else(|| {
            BrowserBackendError::SelectionFailed {
                reason: "service worker openWindow source frame disappeared before resolution"
                    .into(),
            }
        })?;
        let (effects, owner_id, window_name) = match route {
            NativeFrameRoute::ActiveSelected => {
                let mut engine = self.lock_engine_raw(BackendOperation::Script)?;
                require_context_id(&request.source_context_id, &engine.config().context_id)?;
                engine
                    .resolve_service_worker_open_window_async(
                        request.worker_id,
                        request.request_id,
                        window.clone(),
                    )
                    .await
                    .map_err(native_error)?;
                let owner_id = engine.config().context_id.clone();
                let (effects, window_name) = take_native_browser_effects(&mut engine);
                (effects, owner_id, window_name)
            }
            NativeFrameRoute::ActiveParked => {
                let mut targets = self.lock_targets(BackendOperation::Script)?;
                let owner_id = targets.active_target_id.clone().ok_or_else(|| {
                    BrowserBackendError::SelectionFailed {
                        reason: "service worker openWindow owner target disappeared".into(),
                    }
                })?;
                require_context_id(&request.source_context_id, &owner_id)?;
                let frame = targets
                    .active_frames
                    .parked
                    .get_mut(&request.source_frame_id)
                    .ok_or_else(|| BrowserBackendError::SelectionFailed {
                        reason: "service worker openWindow source frame disappeared".into(),
                    })?;
                frame
                    .engine
                    .resolve_service_worker_open_window_async(
                        request.worker_id,
                        request.request_id,
                        window.clone(),
                    )
                    .await
                    .map_err(native_error)?;
                let (effects, window_name) = take_native_browser_effects(&mut frame.engine);
                (effects, owner_id, window_name)
            }
            NativeFrameRoute::ParkedSelected { target_id } => {
                require_context_id(&request.source_context_id, &target_id)?;
                let mut targets = self.lock_targets(BackendOperation::Script)?;
                let target = targets.parked.get_mut(&target_id).ok_or_else(|| {
                    BrowserBackendError::SelectionFailed {
                        reason: "service worker openWindow owner target disappeared".into(),
                    }
                })?;
                target
                    .engine
                    .resolve_service_worker_open_window_async(
                        request.worker_id,
                        request.request_id,
                        window.clone(),
                    )
                    .await
                    .map_err(native_error)?;
                let (effects, window_name) = take_native_browser_effects(&mut target.engine);
                (effects, target_id, window_name)
            }
            NativeFrameRoute::ParkedParked { target_id } => {
                require_context_id(&request.source_context_id, &target_id)?;
                let mut targets = self.lock_targets(BackendOperation::Script)?;
                let target = targets.parked.get_mut(&target_id).ok_or_else(|| {
                    BrowserBackendError::SelectionFailed {
                        reason: "service worker openWindow owner target disappeared".into(),
                    }
                })?;
                let frame = target
                    .frames
                    .parked
                    .get_mut(&request.source_frame_id)
                    .ok_or_else(|| BrowserBackendError::SelectionFailed {
                        reason: "service worker openWindow source frame disappeared".into(),
                    })?;
                frame
                    .engine
                    .resolve_service_worker_open_window_async(
                        request.worker_id,
                        request.request_id,
                        window,
                    )
                    .await
                    .map_err(native_error)?;
                let (effects, window_name) = take_native_browser_effects(&mut frame.engine);
                (effects, target_id, window_name)
            }
        };
        self.sync_target_name(&owner_id, &window_name)?;
        Ok(effects)
    }

    async fn deliver_service_worker_client_message(
        &self,
        message: NativeServiceWorkerClientMessage,
    ) -> Result<NativeQueuedBrowserEffects, BrowserBackendError> {
        if message.worker_id == 0 || message.client_id.is_empty() {
            return Err(BrowserBackendError::InvalidConfiguration {
                field: "service worker client message".into(),
                reason: "worker and client identifiers must be present".into(),
            });
        }
        let Some((route, frame_id)) = self.service_worker_client_route(&message.client_id)? else {
            return Err(BrowserBackendError::SelectionFailed {
                reason: "service worker client disappeared before message delivery".into(),
            });
        };
        let (effects, owner_id, window_name) = match route {
            NativeFrameRoute::ActiveSelected => {
                let mut engine = self.lock_engine_raw(BackendOperation::Script)?;
                engine
                    .dispatch_service_worker_client_message_async(message)
                    .await
                    .map_err(native_error)?;
                let owner_id = engine.config().context_id.clone();
                let (effects, window_name) = take_native_browser_effects(&mut engine);
                (effects, owner_id, window_name)
            }
            NativeFrameRoute::ActiveParked => {
                let mut targets = self.lock_targets(BackendOperation::Script)?;
                let owner_id = targets.active_target_id.clone().ok_or_else(|| {
                    BrowserBackendError::SelectionFailed {
                        reason: "service worker client owner target disappeared".into(),
                    }
                })?;
                let frame = targets
                    .active_frames
                    .parked
                    .get_mut(&frame_id)
                    .ok_or_else(|| BrowserBackendError::SelectionFailed {
                        reason: "service worker client frame disappeared".into(),
                    })?;
                frame
                    .engine
                    .dispatch_service_worker_client_message_async(message)
                    .await
                    .map_err(native_error)?;
                let (effects, window_name) = take_native_browser_effects(&mut frame.engine);
                (effects, owner_id, window_name)
            }
            NativeFrameRoute::ParkedSelected { target_id } => {
                let mut targets = self.lock_targets(BackendOperation::Script)?;
                let target = targets.parked.get_mut(&target_id).ok_or_else(|| {
                    BrowserBackendError::SelectionFailed {
                        reason: "service worker client owner target disappeared".into(),
                    }
                })?;
                target
                    .engine
                    .dispatch_service_worker_client_message_async(message)
                    .await
                    .map_err(native_error)?;
                let (effects, window_name) = take_native_browser_effects(&mut target.engine);
                (effects, target_id, window_name)
            }
            NativeFrameRoute::ParkedParked { target_id } => {
                let mut targets = self.lock_targets(BackendOperation::Script)?;
                let target = targets.parked.get_mut(&target_id).ok_or_else(|| {
                    BrowserBackendError::SelectionFailed {
                        reason: "service worker client owner target disappeared".into(),
                    }
                })?;
                let frame = target.frames.parked.get_mut(&frame_id).ok_or_else(|| {
                    BrowserBackendError::SelectionFailed {
                        reason: "service worker client frame disappeared".into(),
                    }
                })?;
                frame
                    .engine
                    .dispatch_service_worker_client_message_async(message)
                    .await
                    .map_err(native_error)?;
                let (effects, window_name) = take_native_browser_effects(&mut frame.engine);
                (effects, target_id, window_name)
            }
        };
        self.sync_target_name(&owner_id, &window_name)?;
        Ok(effects)
    }

    fn bind_window_handle(
        &self,
        request: &NativePopupRequest,
        target_id: &str,
    ) -> Result<(), BrowserBackendError> {
        let Some(handle) = request.handle.as_deref() else {
            return Ok(());
        };
        if handle.is_empty() {
            return Ok(());
        }
        validate_native_topology_id(&request.source_context_id)?;
        validate_native_topology_id(handle)?;
        let mut targets = self.lock_targets(BackendOperation::Contexts)?;
        targets.window_handles.insert(
            (request.source_context_id.clone(), handle.to_owned()),
            target_id.to_owned(),
        );
        Ok(())
    }

    fn window_proxy_updates(
        &self,
        source_id: &str,
    ) -> Result<Vec<NativeWindowProxyUpdate>, BrowserBackendError> {
        let targets = self.lock_targets(BackendOperation::Script)?;
        let Some(active_target_id) = targets.active_target_id.as_deref() else {
            return Ok(Vec::new());
        };
        let active_engine = self.lock_engine_raw(BackendOperation::Script)?;
        let active_url = active_engine.context().map_err(native_error)?.url;
        let mut snapshots = BTreeMap::new();
        snapshots.insert(
            active_target_id.to_owned(),
            (
                active_url,
                targets.active_name.clone().unwrap_or_default(),
                false,
            ),
        );
        for (target_id, target) in &targets.parked {
            let context = target.engine.context().map_err(native_error)?;
            snapshots.insert(
                target_id.clone(),
                (context.url, target.name.clone().unwrap_or_default(), false),
            );
        }
        for (target_id, target) in &targets.closed {
            snapshots.insert(
                target_id.clone(),
                (target.url.clone(), target.name.clone(), true),
            );
        }

        let mut updates = Vec::with_capacity(snapshots.len() + targets.window_handles.len());
        for (target_id, (url, name, closed)) in &snapshots {
            updates.push(NativeWindowProxyUpdate {
                cache_key: format!("{target_id}\\u0000"),
                target_context_id: target_id.clone(),
                href: url.clone(),
                name: name.clone(),
                closed: *closed,
            });
        }
        for ((handle_source_id, handle), target_id) in &targets.window_handles {
            if handle_source_id != source_id {
                continue;
            }
            let Some((url, name, closed)) = snapshots.get(target_id) else {
                continue;
            };
            updates.push(NativeWindowProxyUpdate {
                cache_key: format!("\\u0000{handle}"),
                target_context_id: target_id.clone(),
                href: url.clone(),
                name: name.clone(),
                closed: *closed,
            });
        }
        if updates.len() > MAX_NATIVE_EFFECTS {
            return Err(BrowserBackendError::UnsupportedOperation {
                operation: "WindowProxy synchronization".into(),
                reason: format!(
                    "native WindowProxy update set exceeded its bounded limit ({MAX_NATIVE_EFFECTS})"
                ),
            });
        }
        Ok(updates)
    }

    fn active_window_proxy_updates(
        &self,
    ) -> Result<Vec<NativeWindowProxyUpdate>, BrowserBackendError> {
        let source_id = self
            .lock_targets(BackendOperation::Script)?
            .active_target_id
            .clone();
        source_id.map_or_else(|| Ok(Vec::new()), |id| self.window_proxy_updates(&id))
    }

    fn target_named(&self, name: &str) -> Result<Option<(String, bool)>, BrowserBackendError> {
        let targets = self.lock_targets(BackendOperation::Contexts)?;
        if targets.active_name.as_deref() == Some(name) {
            return Ok(targets
                .active_target_id
                .as_ref()
                .map(|target_id| (target_id.clone(), true)));
        }
        Ok(targets
            .parked
            .iter()
            .find(|(_, target)| target.name.as_deref() == Some(name))
            .map(|(target_id, _)| (target_id.clone(), false)))
    }

    fn sync_target_name(&self, target_id: &str, name: &str) -> Result<(), BrowserBackendError> {
        validate_native_topology_id(target_id)?;
        if !name.is_empty() {
            validate_native_popup_name(name)?;
        }
        let mut targets = self.lock_targets(BackendOperation::Contexts)?;
        let target_name = native_window_name(name);
        if targets.active_target_id.as_deref() == Some(target_id) {
            targets.active_name = target_name;
        } else if let Some(target) = targets.parked.get_mut(target_id) {
            target.name = target_name;
        }
        Ok(())
    }

    fn message_target(
        &self,
        message: &NativePostMessageRequest,
    ) -> Result<Option<(String, bool)>, BrowserBackendError> {
        let targets = self.lock_targets(BackendOperation::Contexts)?;
        if let Some(target_id) = message.target_context_id.as_deref() {
            validate_native_topology_id(target_id)?;
            if targets.active_target_id.as_deref() == Some(target_id) {
                return Ok(Some((target_id.to_owned(), true)));
            }
            return Ok(targets
                .parked
                .contains_key(target_id)
                .then(|| (target_id.to_owned(), false)));
        }
        if message.target.is_empty() || message.source_context_id.is_empty() {
            return Ok(None);
        }
        if let Some(target_id) = targets
            .window_handles
            .get(&(message.source_context_id.clone(), message.target.clone()))
        {
            let active = targets.active_target_id.as_deref() == Some(target_id.as_str());
            if active || targets.parked.contains_key(target_id) {
                return Ok(Some((target_id.clone(), active)));
            }
            return Ok(None);
        }
        drop(targets);
        self.target_named(&message.target)
    }

    fn window_close_target(
        &self,
        request: &NativeWindowCloseRequest,
    ) -> Result<Option<(String, bool)>, BrowserBackendError> {
        let targets = self.lock_targets(BackendOperation::Close)?;
        if let Some(target_id) = request.target_context_id.as_deref() {
            validate_native_topology_id(target_id)?;
            if targets.active_target_id.as_deref() == Some(target_id) {
                return Ok(Some((target_id.to_owned(), true)));
            }
            return Ok(targets
                .parked
                .contains_key(target_id)
                .then(|| (target_id.to_owned(), false)));
        }
        if request.target.is_empty() || request.source_context_id.is_empty() {
            return Ok(None);
        }
        if let Some(target_id) = targets
            .window_handles
            .get(&(request.source_context_id.clone(), request.target.clone()))
        {
            let active = targets.active_target_id.as_deref() == Some(target_id.as_str());
            if active || targets.parked.contains_key(target_id) {
                return Ok(Some((target_id.clone(), active)));
            }
            return Ok(None);
        }
        drop(targets);
        self.target_named(&request.target)
    }

    fn window_navigation_target(
        &self,
        request: &NativeWindowNavigationRequest,
    ) -> Result<Option<(String, bool)>, BrowserBackendError> {
        let targets = self.lock_targets(BackendOperation::Navigate)?;
        if let Some(target_id) = request.target_context_id.as_deref() {
            validate_native_topology_id(target_id)?;
            if targets.active_target_id.as_deref() == Some(target_id) {
                return Ok(Some((target_id.to_owned(), true)));
            }
            return Ok(targets
                .parked
                .contains_key(target_id)
                .then(|| (target_id.to_owned(), false)));
        }
        if request.target.is_empty() || request.source_context_id.is_empty() {
            return Ok(None);
        }
        if let Some(target_id) = targets
            .window_handles
            .get(&(request.source_context_id.clone(), request.target.clone()))
        {
            let active = targets.active_target_id.as_deref() == Some(target_id.as_str());
            if active || targets.parked.contains_key(target_id) {
                return Ok(Some((target_id.clone(), active)));
            }
            return Ok(None);
        }
        drop(targets);
        self.target_named(&request.target)
    }

    fn frame_owner_context_id(
        &self,
        frame_id: &str,
    ) -> Result<Option<String>, BrowserBackendError> {
        let Some(route) = self.frame_route(frame_id)? else {
            return Ok(None);
        };
        match route {
            NativeFrameRoute::ActiveSelected => Ok(self
                .lock_engine_raw(BackendOperation::Script)?
                .config()
                .context_id
                .clone()
                .into()),
            NativeFrameRoute::ActiveParked => Ok(self
                .lock_targets(BackendOperation::Script)?
                .active_target_id
                .clone()),
            NativeFrameRoute::ParkedSelected { target_id }
            | NativeFrameRoute::ParkedParked { target_id } => Ok(Some(target_id)),
        }
    }

    fn frame_owner_document_generation(
        &self,
        frame_id: &str,
    ) -> Result<Option<u32>, BrowserBackendError> {
        let Some(route) = self.frame_route(frame_id)? else {
            return Ok(None);
        };
        let generation = match route {
            NativeFrameRoute::ActiveSelected => self
                .lock_engine_raw(BackendOperation::Script)?
                .document_generation()
                .map_err(native_error)?,
            NativeFrameRoute::ActiveParked => {
                let targets = self.lock_targets(BackendOperation::Script)?;
                let Some(target) = targets.active_frames.parked.get(frame_id) else {
                    return Ok(None);
                };
                target.engine.document_generation().map_err(native_error)?
            }
            NativeFrameRoute::ParkedSelected { target_id } => {
                let targets = self.lock_targets(BackendOperation::Script)?;
                let Some(target) = targets.parked.get(&target_id) else {
                    return Ok(None);
                };
                target.engine.document_generation().map_err(native_error)?
            }
            NativeFrameRoute::ParkedParked { target_id } => {
                let targets = self.lock_targets(BackendOperation::Script)?;
                let Some(frame) = targets
                    .parked
                    .get(&target_id)
                    .and_then(|target| target.frames.parked.get(frame_id))
                else {
                    return Ok(None);
                };
                frame.engine.document_generation().map_err(native_error)?
            }
        };
        Ok(Some(generation))
    }

    fn register_page_message_port_routes(
        &self,
        source_context_id: &str,
        source_frame_id: &str,
        transfers: &[super::native_engine::NativeMessagePortTransfer],
    ) -> Result<(), BrowserBackendError> {
        if transfers.is_empty() {
            return Ok(());
        }
        validate_native_topology_id(source_context_id)?;
        validate_native_topology_id(source_frame_id)?;
        validate_message_port_transfers(transfers).map_err(native_error)?;
        let Some(owner_context_id) = self.frame_owner_context_id(source_frame_id)? else {
            return Ok(());
        };
        if owner_context_id != source_context_id {
            return Err(BrowserBackendError::SelectionFailed {
                reason: "native MessagePort source frame does not belong to its context".into(),
            });
        }
        self.prune_page_message_port_routes()?;
        let mut routes = self
            .page_message_port_routes
            .lock()
            .map_err(|_| poisoned_lock_error(BackendOperation::Script, "page MessagePort route"))?;
        if transfers.len() > NATIVE_MAX_MESSAGE_PORT_ROUTES.saturating_sub(routes.len()) {
            return Err(BrowserBackendError::SelectionFailed {
                reason: format!(
                    "native MessagePort route limit reached ({NATIVE_MAX_MESSAGE_PORT_ROUTES})"
                ),
            });
        }
        if transfers
            .iter()
            .any(|transfer| routes.contains_key(&transfer.bridge_key))
        {
            return Err(BrowserBackendError::InvalidConfiguration {
                field: "native MessagePort transfer".into(),
                reason: "bridge key was already routed".into(),
            });
        }
        for transfer in transfers {
            routes.insert(
                transfer.bridge_key.clone(),
                NativePageMessagePortRoute {
                    context_id: source_context_id.to_owned(),
                    frame_id: source_frame_id.to_owned(),
                },
            );
        }
        Ok(())
    }

    fn page_message_port_route(
        &self,
        bridge_key: &str,
    ) -> Result<Option<NativePageMessagePortRoute>, BrowserBackendError> {
        let route = self
            .page_message_port_routes
            .lock()
            .map_err(|_| poisoned_lock_error(BackendOperation::Script, "page MessagePort route"))?
            .get(bridge_key)
            .cloned();
        let Some(route) = route else {
            return Ok(None);
        };
        if self.frame_route(&route.frame_id)?.is_some() {
            if self.frame_owner_context_id(&route.frame_id)?.as_deref()
                == Some(route.context_id.as_str())
            {
                return Ok(Some(route));
            }
        }
        self.page_message_port_routes
            .lock()
            .map_err(|_| poisoned_lock_error(BackendOperation::Script, "page MessagePort route"))?
            .remove(bridge_key);
        Ok(None)
    }

    fn remove_page_message_port_route(&self, bridge_key: &str) -> Result<(), BrowserBackendError> {
        self.page_message_port_routes
            .lock()
            .map_err(|_| poisoned_lock_error(BackendOperation::Script, "page MessagePort route"))?
            .remove(bridge_key);
        Ok(())
    }

    fn clear_page_message_port_routes_for_frame(
        &self,
        frame_id: &str,
    ) -> Result<(), BrowserBackendError> {
        validate_native_topology_id(frame_id)?;
        self.page_message_port_routes
            .lock()
            .map_err(|_| poisoned_lock_error(BackendOperation::Script, "page MessagePort route"))?
            .retain(|_, route| route.frame_id != frame_id);
        self.shared_workers
            .lock()
            .map_err(|_| poisoned_lock_error(BackendOperation::Script, "SharedWorker coordinator"))?
            .page_ports
            .retain(|_, route| route.frame_id != frame_id);
        Ok(())
    }

    fn clear_page_message_port_routes_for_context(
        &self,
        context_id: &str,
    ) -> Result<(), BrowserBackendError> {
        validate_native_topology_id(context_id)?;
        self.page_message_port_routes
            .lock()
            .map_err(|_| poisoned_lock_error(BackendOperation::Script, "page MessagePort route"))?
            .retain(|_, route| route.context_id != context_id);
        self.shared_workers
            .lock()
            .map_err(|_| poisoned_lock_error(BackendOperation::Script, "SharedWorker coordinator"))?
            .page_ports
            .retain(|_, route| route.context_id != context_id);
        Ok(())
    }

    async fn close_shared_worker_owners_for_context(
        &self,
        context_id: &str,
    ) -> Result<(), BrowserBackendError> {
        validate_native_topology_id(context_id)?;
        let bridge_keys = {
            let workers = self.shared_workers.lock().map_err(|_| {
                poisoned_lock_error(BackendOperation::Close, "SharedWorker coordinator")
            })?;
            workers
                .page_ports
                .iter()
                .filter_map(|(bridge_key, route)| {
                    (route.context_id == context_id).then_some(bridge_key.clone())
                })
                .collect::<Vec<_>>()
        };
        self.clear_page_message_port_routes_for_context(context_id)?;

        let mut first_error = None;
        for bridge_key in bridge_keys {
            let (close_result, effects) = {
                let mut coordinator = self.shared_workers.lock().map_err(|_| {
                    poisoned_lock_error(BackendOperation::Close, "SharedWorker coordinator")
                })?;
                let result = {
                    let NativeSharedWorkerCoordinator {
                        registry, loader, ..
                    } = &mut *coordinator;
                    registry
                        .apply_page_message_port_commands(
                            vec![NativeScriptCommand::MessagePortClose {
                                bridge_key,
                                worker_id: None,
                            }],
                            loader,
                        )
                        .await
                };
                let effects = take_shared_worker_page_messages(&mut coordinator);
                (result, effects)
            };
            if let Err(error) = close_result {
                first_error.get_or_insert_with(|| native_error(error));
            }
            let (
                popups,
                messages,
                closes,
                navigations,
                service_worker_windows,
                service_worker_messages,
                _,
            ) = NativeQueuedBrowserEffects::default();
            if let Err(error) = Box::pin(self.process_pending_browser_effects(
                popups,
                messages,
                closes,
                navigations,
                service_worker_windows,
                service_worker_messages,
                effects,
            ))
            .await
            {
                first_error.get_or_insert(error);
            }
        }

        {
            let mut coordinator = self.shared_workers.lock().map_err(|_| {
                poisoned_lock_error(BackendOperation::Close, "SharedWorker coordinator")
            })?;
            let unowned = coordinator
                .registry
                .remove_shared_worker_owners_for_context(context_id);
            for worker_id in unowned {
                coordinator
                    .registry
                    .terminate_unowned_shared_worker(worker_id);
            }
            let NativeSharedWorkerCoordinator {
                registry,
                page_ports,
                ..
            } = &mut *coordinator;
            page_ports
                .retain(|bridge_key, _| registry.shared_worker_id_for_port(bridge_key).is_some());
        }

        first_error.map_or(Ok(()), Err)
    }

    async fn close_shared_worker_owners_for_document(
        &self,
        context_id: &str,
        frame_id: &str,
        document_generation: u32,
    ) -> Result<(), BrowserBackendError> {
        validate_native_topology_id(context_id)?;
        validate_native_topology_id(frame_id)?;
        if document_generation == 0 {
            return Err(BrowserBackendError::InvalidConfiguration {
                field: "native SharedWorker owner Document generation".into(),
                reason: "must be positive".into(),
            });
        }
        let bridge_keys = {
            let workers = self.shared_workers.lock().map_err(|_| {
                poisoned_lock_error(BackendOperation::Navigate, "SharedWorker coordinator")
            })?;
            workers
                .page_ports
                .iter()
                .filter_map(|(bridge_key, route)| {
                    (route.context_id == context_id
                        && route.frame_id == frame_id
                        && route.document_generation == document_generation)
                        .then_some(bridge_key.clone())
                })
                .collect::<Vec<_>>()
        };
        let close_result = self.close_shared_worker_bridges(bridge_keys).await;
        let owner_result =
            self.remove_shared_worker_document_owner(context_id, frame_id, document_generation);
        close_result?;
        owner_result
    }

    async fn close_shared_worker_owners_for_frame(
        &self,
        context_id: &str,
        frame_id: &str,
    ) -> Result<(), BrowserBackendError> {
        validate_native_topology_id(context_id)?;
        validate_native_topology_id(frame_id)?;
        let bridge_keys = {
            let workers = self.shared_workers.lock().map_err(|_| {
                poisoned_lock_error(BackendOperation::Navigate, "SharedWorker coordinator")
            })?;
            workers
                .page_ports
                .iter()
                .filter_map(|(bridge_key, route)| {
                    (route.context_id == context_id && route.frame_id == frame_id)
                        .then_some(bridge_key.clone())
                })
                .collect::<Vec<_>>()
        };
        let close_result = self.close_shared_worker_bridges(bridge_keys).await;
        let owner_result = self.remove_shared_worker_frame_owners(context_id, frame_id);
        close_result?;
        owner_result
    }

    async fn close_shared_worker_bridges(
        &self,
        bridge_keys: Vec<String>,
    ) -> Result<(), BrowserBackendError> {
        let bridge_keys = bridge_keys.into_iter().collect::<BTreeSet<_>>();
        if bridge_keys.is_empty() {
            return Ok(());
        }
        self.page_message_port_routes
            .lock()
            .map_err(|_| poisoned_lock_error(BackendOperation::Navigate, "page MessagePort route"))?
            .retain(|bridge_key, _| !bridge_keys.contains(bridge_key));
        self.shared_workers
            .lock()
            .map_err(|_| {
                poisoned_lock_error(BackendOperation::Navigate, "SharedWorker coordinator")
            })?
            .page_ports
            .retain(|bridge_key, _| !bridge_keys.contains(bridge_key));

        let mut first_error = None;
        for bridge_key in bridge_keys {
            let (close_result, effects) = {
                let mut coordinator = self.shared_workers.lock().map_err(|_| {
                    poisoned_lock_error(BackendOperation::Navigate, "SharedWorker coordinator")
                })?;
                let result = {
                    let NativeSharedWorkerCoordinator {
                        registry, loader, ..
                    } = &mut *coordinator;
                    registry
                        .apply_page_message_port_commands(
                            vec![NativeScriptCommand::MessagePortClose {
                                bridge_key,
                                worker_id: None,
                            }],
                            loader,
                        )
                        .await
                };
                let effects = take_shared_worker_page_messages(&mut coordinator);
                (result, effects)
            };
            if let Err(error) = close_result {
                first_error.get_or_insert_with(|| native_error(error));
            }
            let (
                popups,
                messages,
                closes,
                navigations,
                service_worker_windows,
                service_worker_messages,
                _,
            ) = NativeQueuedBrowserEffects::default();
            if let Err(error) = Box::pin(self.process_pending_browser_effects(
                popups,
                messages,
                closes,
                navigations,
                service_worker_windows,
                service_worker_messages,
                effects,
            ))
            .await
            {
                first_error.get_or_insert(error);
            }
        }
        first_error.map_or(Ok(()), Err)
    }

    fn remove_shared_worker_document_owner(
        &self,
        context_id: &str,
        frame_id: &str,
        document_generation: u32,
    ) -> Result<(), BrowserBackendError> {
        let mut coordinator = self.shared_workers.lock().map_err(|_| {
            poisoned_lock_error(BackendOperation::Navigate, "SharedWorker coordinator")
        })?;
        let unowned = coordinator
            .registry
            .remove_shared_worker_owner_for_document(context_id, frame_id, document_generation);
        for worker_id in unowned {
            coordinator
                .registry
                .terminate_unowned_shared_worker(worker_id);
        }
        let NativeSharedWorkerCoordinator {
            registry,
            page_ports,
            ..
        } = &mut *coordinator;
        page_ports.retain(|bridge_key, _| registry.shared_worker_id_for_port(bridge_key).is_some());
        Ok(())
    }

    fn remove_shared_worker_frame_owners(
        &self,
        context_id: &str,
        frame_id: &str,
    ) -> Result<(), BrowserBackendError> {
        let mut coordinator = self.shared_workers.lock().map_err(|_| {
            poisoned_lock_error(BackendOperation::Navigate, "SharedWorker coordinator")
        })?;
        let unowned = coordinator
            .registry
            .remove_shared_worker_owners_for_frame(context_id, frame_id);
        for worker_id in unowned {
            coordinator
                .registry
                .terminate_unowned_shared_worker(worker_id);
        }
        let NativeSharedWorkerCoordinator {
            registry,
            page_ports,
            ..
        } = &mut *coordinator;
        page_ports.retain(|bridge_key, _| registry.shared_worker_id_for_port(bridge_key).is_some());
        Ok(())
    }

    async fn teardown_replaced_document(
        &self,
        context_id: &str,
        frame_id: &str,
        document_generation: u32,
        destroyed_descendant_frame_ids: &[String],
    ) -> Result<(), BrowserBackendError> {
        let mut first_error = self
            .close_shared_worker_owners_for_document(context_id, frame_id, document_generation)
            .await
            .err();
        for descendant_frame_id in destroyed_descendant_frame_ids {
            if let Err(error) = self
                .close_shared_worker_owners_for_frame(context_id, descendant_frame_id)
                .await
            {
                first_error.get_or_insert(error);
            }
        }
        if let Err(error) = self.clear_page_message_port_routes_for_frame(frame_id) {
            first_error.get_or_insert(error);
        }
        for descendant_frame_id in destroyed_descendant_frame_ids {
            if let Err(error) = self.clear_page_message_port_routes_for_frame(descendant_frame_id) {
                first_error.get_or_insert(error);
            }
        }
        if let Err(error) = self.prune_page_message_port_routes() {
            first_error.get_or_insert(error);
        }
        first_error.map_or(Ok(()), Err)
    }

    fn clear_page_message_port_routes(&self) -> Result<(), BrowserBackendError> {
        self.page_message_port_routes
            .lock()
            .map_err(|_| poisoned_lock_error(BackendOperation::Script, "page MessagePort route"))?
            .clear();
        let mut workers = self.shared_workers.lock().map_err(|_| {
            poisoned_lock_error(BackendOperation::Script, "SharedWorker coordinator")
        })?;
        workers.page_ports.clear();
        workers.registry.clear();
        Ok(())
    }

    fn prune_page_message_port_routes(&self) -> Result<(), BrowserBackendError> {
        let routes = self
            .page_message_port_routes
            .lock()
            .map_err(|_| poisoned_lock_error(BackendOperation::Script, "page MessagePort route"))?
            .iter()
            .map(|(key, route)| (key.clone(), route.clone()))
            .collect::<Vec<_>>();
        let mut stale = BTreeSet::new();
        for (key, route) in routes {
            if self.frame_owner_context_id(&route.frame_id)?.as_deref()
                != Some(route.context_id.as_str())
            {
                stale.insert(key);
            }
        }
        if stale.is_empty() {
            return Ok(());
        }
        self.page_message_port_routes
            .lock()
            .map_err(|_| poisoned_lock_error(BackendOperation::Script, "page MessagePort route"))?
            .retain(|key, _| !stale.contains(key));
        Ok(())
    }

    fn page_post_message_target_allowed(
        &self,
        frame_id: &str,
        message: &NativePostMessageRequest,
    ) -> Result<bool, BrowserBackendError> {
        let Some(origin) = self.frame_origin(frame_id)? else {
            return Ok(false);
        };
        native_message_target_origin_matches(
            &message.source_origin,
            &message.target_origin,
            &origin.serialized(),
        )
    }

    async fn deliver_post_message(
        &self,
        message: NativePostMessageRequest,
    ) -> Result<NativeQueuedBrowserEffects, BrowserBackendError> {
        if let Some(frame_id) = message.target_context_id.clone()
            && let Some(route) = self.frame_route(&frame_id)?
        {
            return self
                .deliver_post_message_to_frame(route, &frame_id, message)
                .await;
        }
        let Some((target_id, active)) = self.message_target(&message)? else {
            return Ok(NativeQueuedBrowserEffects::default());
        };
        let proxy_updates = self.window_proxy_updates(&target_id)?;
        if active {
            let mut engine = self.lock_engine_raw(BackendOperation::Script)?;
            engine
                .sync_window_proxies(&proxy_updates)
                .await
                .map_err(native_error)?;
            let target_origin = engine.snapshot().map_err(native_error)?.origin.serialized();
            if !native_message_target_origin_matches(
                &message.source_origin,
                &message.target_origin,
                &target_origin,
            )? {
                return Ok(NativeQueuedBrowserEffects::default());
            }
            engine
                .dispatch_post_message(
                    &message.source_context_id,
                    &message.source_origin,
                    &message.data,
                    &message.transfer_ports,
                    &message.object_urls,
                )
                .await
                .map_err(native_error)?;
            let (effects, window_name) = take_native_browser_effects(&mut engine);
            drop(engine);
            self.sync_target_name(&target_id, &window_name)?;
            self.register_page_message_port_routes(
                &message.source_context_id,
                &message.source_frame_id,
                &message.transfer_ports,
            )?;
            return Ok(effects);
        }
        let mut targets = self.lock_targets(BackendOperation::Script)?;
        let Some(parked) = targets.parked.get_mut(&target_id) else {
            return Ok(NativeQueuedBrowserEffects::default());
        };
        parked
            .engine
            .sync_window_proxies(&proxy_updates)
            .await
            .map_err(native_error)?;
        let target_origin = parked
            .engine
            .snapshot()
            .map_err(native_error)?
            .origin
            .serialized();
        if !native_message_target_origin_matches(
            &message.source_origin,
            &message.target_origin,
            &target_origin,
        )? {
            return Ok(NativeQueuedBrowserEffects::default());
        }
        parked
            .engine
            .dispatch_post_message(
                &message.source_context_id,
                &message.source_origin,
                &message.data,
                &message.transfer_ports,
                &message.object_urls,
            )
            .await
            .map_err(native_error)?;
        let (effects, window_name) = take_native_browser_effects(&mut parked.engine);
        drop(targets);
        self.sync_target_name(&target_id, &window_name)?;
        self.register_page_message_port_routes(
            &message.source_context_id,
            &message.source_frame_id,
            &message.transfer_ports,
        )?;
        Ok(effects)
    }

    async fn deliver_post_message_to_frame(
        &self,
        route: NativeFrameRoute,
        frame_id: &str,
        message: NativePostMessageRequest,
    ) -> Result<NativeQueuedBrowserEffects, BrowserBackendError> {
        let proxy_updates = self.window_proxy_updates(&message.source_context_id)?;
        let (runtime_effects, owner_id) = match route {
            NativeFrameRoute::ActiveSelected => {
                let mut engine = self.lock_engine_raw(BackendOperation::Script)?;
                let result =
                    dispatch_post_message_to_native_frame(&mut engine, &message, &proxy_updates)
                        .await?;
                let owner_id = engine.config().context_id.clone();
                (result, owner_id)
            }
            NativeFrameRoute::ActiveParked => {
                let mut targets = self.lock_targets(BackendOperation::Script)?;
                let owner_id = targets.active_target_id.clone().ok_or_else(|| {
                    BrowserBackendError::SelectionFailed {
                        reason: "native frame owner target disappeared during message delivery"
                            .into(),
                    }
                })?;
                let frame = targets
                    .active_frames
                    .parked
                    .get_mut(frame_id)
                    .ok_or_else(|| BrowserBackendError::SelectionFailed {
                        reason: "native frame disappeared during message delivery".into(),
                    })?;
                let result = dispatch_post_message_to_native_frame(
                    &mut frame.engine,
                    &message,
                    &proxy_updates,
                )
                .await?;
                (result, owner_id)
            }
            NativeFrameRoute::ParkedSelected { target_id } => {
                let mut targets = self.lock_targets(BackendOperation::Script)?;
                let target = targets.parked.get_mut(&target_id).ok_or_else(|| {
                    BrowserBackendError::SelectionFailed {
                        reason: "native frame owner target disappeared during message delivery"
                            .into(),
                    }
                })?;
                let result = dispatch_post_message_to_native_frame(
                    &mut target.engine,
                    &message,
                    &proxy_updates,
                )
                .await?;
                (result, target_id)
            }
            NativeFrameRoute::ParkedParked { target_id } => {
                let mut targets = self.lock_targets(BackendOperation::Script)?;
                let target = targets.parked.get_mut(&target_id).ok_or_else(|| {
                    BrowserBackendError::SelectionFailed {
                        reason: "native frame owner target disappeared during message delivery"
                            .into(),
                    }
                })?;
                let frame = target.frames.parked.get_mut(frame_id).ok_or_else(|| {
                    BrowserBackendError::SelectionFailed {
                        reason: "native frame disappeared during message delivery".into(),
                    }
                })?;
                let result = dispatch_post_message_to_native_frame(
                    &mut frame.engine,
                    &message,
                    &proxy_updates,
                )
                .await?;
                (result, target_id)
            }
        };
        self.sync_target_name(&owner_id, &runtime_effects.window_name)?;
        self.process_frame_event_effects(frame_id, runtime_effects.events)
            .await?;
        Box::pin(self.process_pending_frame_scripts(runtime_effects.frame_scripts)).await?;
        if self.page_post_message_target_allowed(frame_id, &message)? {
            self.register_page_message_port_routes(
                &message.source_context_id,
                &message.source_frame_id,
                &message.transfer_ports,
            )?;
        }
        Ok(runtime_effects.browser)
    }

    fn is_shared_worker_page_port(
        &self,
        command: &NativePageMessagePortCommand,
    ) -> Result<bool, BrowserBackendError> {
        let route = self
            .shared_workers
            .lock()
            .map_err(|_| poisoned_lock_error(BackendOperation::Script, "SharedWorker coordinator"))?
            .page_ports
            .get(&command.bridge_key)
            .cloned();
        let Some(route) = route else {
            return Ok(false);
        };
        if route.context_id != command.source_context_id
            || route.frame_id != command.source_frame_id
        {
            return Ok(false);
        }
        if self.frame_owner_context_id(&route.frame_id)?.as_deref()
            != Some(route.context_id.as_str())
            || self.frame_owner_document_generation(&route.frame_id)?
                != Some(route.document_generation)
        {
            self.shared_workers
                .lock()
                .map_err(|_| {
                    poisoned_lock_error(BackendOperation::Script, "SharedWorker coordinator")
                })?
                .page_ports
                .remove(&command.bridge_key);
            return Ok(false);
        }
        Ok(true)
    }

    async fn create_shared_worker_connection(
        &self,
        request: NativeSharedWorkerCreateRequest,
    ) -> Result<NativeQueuedBrowserEffects, BrowserBackendError> {
        validate_native_topology_id(&request.source_context_id)?;
        validate_native_topology_id(&request.source_frame_id)?;
        if request.document_generation == 0 {
            return Err(BrowserBackendError::InvalidConfiguration {
                field: "native SharedWorker owner Document generation".into(),
                reason: "must be positive".into(),
            });
        }
        let current_context = self.frame_owner_context_id(&request.source_frame_id)?;
        let current_generation = self.frame_owner_document_generation(&request.source_frame_id)?;
        if current_context.as_deref() != Some(request.source_context_id.as_str())
            || current_generation != Some(request.document_generation)
        {
            return Err(BrowserBackendError::SelectionFailed {
                reason: format!(
                    "native SharedWorker create came from stale Document owner {}:{} generation {}; current owner is {:?} generation {:?}",
                    request.source_context_id,
                    request.source_frame_id,
                    request.document_generation,
                    current_context,
                    current_generation
                ),
            });
        }
        validate_message_port_transfers(std::slice::from_ref(&request.transfer_port))
            .map_err(native_error)?;
        self.register_page_message_port_routes(
            &request.source_context_id,
            &request.source_frame_id,
            std::slice::from_ref(&request.transfer_port),
        )?;
        let mut coordinator = self.shared_workers.lock().map_err(|_| {
            poisoned_lock_error(BackendOperation::Script, "SharedWorker coordinator")
        })?;
        let connection_id = u32::try_from(coordinator.next_connection_id).map_err(|_| {
            BrowserBackendError::SelectionFailed {
                reason: "native SharedWorker connection identity space is exhausted".into(),
            }
        })?;
        coordinator.next_connection_id =
            coordinator
                .next_connection_id
                .checked_add(1)
                .ok_or_else(|| BrowserBackendError::SelectionFailed {
                    reason: "native SharedWorker connection identity space is exhausted".into(),
                })?;
        if coordinator
            .page_ports
            .contains_key(&request.transfer_port.bridge_key)
        {
            self.remove_page_message_port_route(&request.transfer_port.bridge_key)?;
            return Err(BrowserBackendError::InvalidConfiguration {
                field: "native SharedWorker port".into(),
                reason: "MessagePort bridge was already attached to a SharedWorker".into(),
            });
        }
        let page_route = NativeSharedWorkerPageRoute {
            context_id: request.source_context_id.clone(),
            frame_id: request.source_frame_id.clone(),
            document_generation: request.document_generation,
        };
        coordinator
            .page_ports
            .insert(request.transfer_port.bridge_key.clone(), page_route.clone());
        let command = NativeScriptCommand::SharedWorkerCreate {
            connection_id,
            href: request.href,
            name: request.name,
            worker_type: request.worker_type,
            credentials: request.credentials,
            extended_lifetime: request.extended_lifetime,
            cookie_profile: Vec::new(),
            constructor_storage_key: Some(NativeSharedWorkerStorageKey::for_document(
                &request.constructor_origin,
                &request.source_context_id,
                &request.source_frame_id,
                request.document_generation,
            )),
            transfer_port: request.transfer_port.clone(),
        };
        let result = match coordinator
            .loader
            .replace_cookie_profiles(&request.cookie_profile)
        {
            Ok(()) => match coordinator.replay_cookie_overrides() {
                Ok(()) => {
                    let NativeSharedWorkerCoordinator {
                        registry, loader, ..
                    } = &mut *coordinator;
                    registry
                        .apply_commands(vec![command], loader, &request.owner_url)
                        .await
                }
                Err(error) => Err(error),
            },
            Err(error) => Err(error),
        };
        let mut cookie_changes = Vec::new();
        let result = match (result, coordinator.remember_cookie_changes()) {
            (Err(error), Ok(changes)) => {
                cookie_changes = changes;
                Err(error)
            }
            (Err(error), Err(_)) => Err(error),
            (Ok(()), Ok(changes)) => {
                cookie_changes = changes;
                Ok(())
            }
            (Ok(()), Err(error)) => Err(error),
        };
        if let Err(error) = result {
            coordinator
                .page_ports
                .remove(&request.transfer_port.bridge_key);
            drop(coordinator);
            self.remove_page_message_port_route(&request.transfer_port.bridge_key)?;
            return Err(native_error(error));
        }
        let bridge_key = request.transfer_port.bridge_key.clone();
        let worker_messages = coordinator.registry.take_messages_for_worker(connection_id);
        let mut queued = NativeQueuedBrowserEffects::default();
        if !cookie_changes.is_empty() {
            queued
                .6
                .push(NativeWorkerCoordinatorEffect::SharedWorkerCookieChanges {
                    changes: cookie_changes,
                });
        }
        if !worker_messages.is_empty() {
            queued
                .6
                .extend(worker_messages.into_iter().map(|mut message| {
                    message.worker_id = request.page_worker_id;
                    NativeWorkerCoordinatorEffect::SharedWorkerError {
                        route: page_route.clone(),
                        message,
                    }
                }));
            coordinator.page_ports.remove(&bridge_key);
            drop(coordinator);
            self.remove_page_message_port_route(&bridge_key)?;
            return Ok(queued);
        }
        let Some(worker_id) = coordinator.registry.shared_worker_id_for_port(&bridge_key) else {
            coordinator.page_ports.remove(&bridge_key);
            drop(coordinator);
            self.remove_page_message_port_route(&bridge_key)?;
            return Ok(queued);
        };
        if let Err(error) = coordinator.registry.add_shared_worker_owner(
            worker_id,
            request.source_context_id,
            request.source_frame_id,
            request.document_generation,
        ) {
            coordinator.page_ports.remove(&bridge_key);
            drop(coordinator);
            self.remove_page_message_port_route(&bridge_key)?;
            return Err(native_error(error));
        }
        let effects = take_shared_worker_page_messages(&mut coordinator);
        queued.6.extend(effects);
        Ok(queued)
    }

    async fn deliver_shared_worker_error(
        &self,
        route_info: NativeSharedWorkerPageRoute,
        message: NativeWorkerMessage,
    ) -> Result<NativeQueuedBrowserEffects, BrowserBackendError> {
        validate_native_topology_id(&route_info.context_id)?;
        validate_native_topology_id(&route_info.frame_id)?;
        if self
            .frame_owner_context_id(&route_info.frame_id)?
            .as_deref()
            != Some(route_info.context_id.as_str())
            || self.frame_owner_document_generation(&route_info.frame_id)?
                != Some(route_info.document_generation)
        {
            return Ok(NativeQueuedBrowserEffects::default());
        }
        let Some(route) = self.frame_route(&route_info.frame_id)? else {
            return Ok(NativeQueuedBrowserEffects::default());
        };
        let (runtime_effects, owner_id) = match route {
            NativeFrameRoute::ActiveSelected => {
                let mut engine = self.lock_engine_raw(BackendOperation::Script)?;
                let runtime_effects =
                    dispatch_worker_messages_to_native_frame(&mut engine, &[message]).await?;
                let owner_id = engine.config().context_id.clone();
                (runtime_effects, owner_id)
            }
            NativeFrameRoute::ActiveParked => {
                let mut targets = self.lock_targets(BackendOperation::Script)?;
                let owner_id = targets.active_target_id.clone().ok_or_else(|| {
                    BrowserBackendError::SelectionFailed {
                        reason: "native SharedWorker error owner target disappeared".into(),
                    }
                })?;
                let frame = targets
                    .active_frames
                    .parked
                    .get_mut(&route_info.frame_id)
                    .ok_or_else(|| BrowserBackendError::SelectionFailed {
                        reason: "native SharedWorker error owner frame disappeared".into(),
                    })?;
                let runtime_effects =
                    dispatch_worker_messages_to_native_frame(&mut frame.engine, &[message]).await?;
                (runtime_effects, owner_id)
            }
            NativeFrameRoute::ParkedSelected { target_id } => {
                let mut targets = self.lock_targets(BackendOperation::Script)?;
                let target = targets.parked.get_mut(&target_id).ok_or_else(|| {
                    BrowserBackendError::SelectionFailed {
                        reason: "native SharedWorker error owner target disappeared".into(),
                    }
                })?;
                let runtime_effects =
                    dispatch_worker_messages_to_native_frame(&mut target.engine, &[message])
                        .await?;
                (runtime_effects, target_id)
            }
            NativeFrameRoute::ParkedParked { target_id } => {
                let mut targets = self.lock_targets(BackendOperation::Script)?;
                let target = targets.parked.get_mut(&target_id).ok_or_else(|| {
                    BrowserBackendError::SelectionFailed {
                        reason: "native SharedWorker error owner target disappeared".into(),
                    }
                })?;
                let frame = target
                    .frames
                    .parked
                    .get_mut(&route_info.frame_id)
                    .ok_or_else(|| BrowserBackendError::SelectionFailed {
                        reason: "native SharedWorker error owner frame disappeared".into(),
                    })?;
                let runtime_effects =
                    dispatch_worker_messages_to_native_frame(&mut frame.engine, &[message]).await?;
                (runtime_effects, target_id)
            }
        };
        self.sync_target_name(&owner_id, &runtime_effects.window_name)?;
        self.process_frame_event_effects(&route_info.frame_id, runtime_effects.events)
            .await?;
        Box::pin(self.process_pending_frame_scripts(runtime_effects.frame_scripts)).await?;
        Ok(runtime_effects.browser)
    }

    async fn deliver_shared_worker_page_message_port(
        &self,
        command: NativePageMessagePortCommand,
    ) -> Result<NativeQueuedBrowserEffects, BrowserBackendError> {
        validate_page_message_port_command(&command).map_err(native_error)?;
        validate_native_topology_id(&command.source_context_id)?;
        validate_native_topology_id(&command.source_frame_id)?;
        let route = self
            .shared_workers
            .lock()
            .map_err(|_| poisoned_lock_error(BackendOperation::Script, "SharedWorker coordinator"))?
            .page_ports
            .get(&command.bridge_key)
            .cloned();
        let Some(route) = route else {
            return Ok(NativeQueuedBrowserEffects::default());
        };
        if route.context_id != command.source_context_id
            || route.frame_id != command.source_frame_id
            || self.frame_owner_context_id(&route.frame_id)?.as_deref()
                != Some(route.context_id.as_str())
            || self.frame_owner_document_generation(&route.frame_id)?
                != Some(route.document_generation)
        {
            return Ok(NativeQueuedBrowserEffects::default());
        }
        if !command.close {
            self.register_page_message_port_routes(
                &command.source_context_id,
                &command.source_frame_id,
                &command.transfer_ports,
            )?;
        }
        let bridge_key = command.bridge_key.clone();
        let mut coordinator = self.shared_workers.lock().map_err(|_| {
            poisoned_lock_error(BackendOperation::Script, "SharedWorker coordinator")
        })?;
        if command.close {
            coordinator.page_ports.remove(&bridge_key);
        } else {
            for transfer in &command.transfer_ports {
                coordinator.page_ports.insert(
                    transfer.bridge_key.clone(),
                    NativeSharedWorkerPageRoute {
                        context_id: route.context_id.clone(),
                        frame_id: route.frame_id.clone(),
                        document_generation: route.document_generation,
                    },
                );
            }
        }
        let worker_command = if command.close {
            NativeScriptCommand::MessagePortClose {
                bridge_key: command.bridge_key,
                worker_id: None,
            }
        } else {
            NativeScriptCommand::MessagePortPostMessage {
                bridge_key: command.bridge_key,
                data: command.data,
                worker_id: None,
                transfer_ports: command.transfer_ports,
                object_urls: command.object_urls,
            }
        };
        let result = {
            let NativeSharedWorkerCoordinator {
                registry, loader, ..
            } = &mut *coordinator;
            registry
                .apply_page_message_port_commands(vec![worker_command], loader)
                .await
        };
        let cookie_capture = coordinator.remember_cookie_changes();
        let effects = take_shared_worker_page_messages(&mut coordinator);
        drop(coordinator);
        if command.close {
            self.remove_page_message_port_route(&bridge_key)?;
        }
        result.map_err(native_error)?;
        let cookie_changes = cookie_capture.map_err(native_error)?;
        let mut queued = NativeQueuedBrowserEffects::default();
        if !cookie_changes.is_empty() {
            queued
                .6
                .push(NativeWorkerCoordinatorEffect::SharedWorkerCookieChanges {
                    changes: cookie_changes,
                });
        }
        queued.6.extend(effects);
        Ok(queued)
    }

    async fn deliver_shared_worker_cookie_changes(
        &self,
        changes: Vec<NativeCookieChange>,
    ) -> Result<NativeQueuedBrowserEffects, BrowserBackendError> {
        let mut targets = self.lock_targets(BackendOperation::Script)?;
        let mut engine = self.lock_engine_raw(BackendOperation::Script)?;
        let mut profile_written = false;
        if targets.active_target_id.is_some() {
            apply_native_cookie_changes_to_live_engine(&mut engine, &changes, &mut profile_written)
                .await
                .map_err(native_error)?;
        }
        for frame in targets.active_frames.parked.values_mut() {
            apply_native_cookie_changes_to_live_engine(
                &mut frame.engine,
                &changes,
                &mut profile_written,
            )
            .await
            .map_err(native_error)?;
        }
        for target in targets.parked.values_mut() {
            apply_native_cookie_changes_to_live_engine(
                &mut target.engine,
                &changes,
                &mut profile_written,
            )
            .await
            .map_err(native_error)?;
            for frame in target.frames.parked.values_mut() {
                apply_native_cookie_changes_to_live_engine(
                    &mut frame.engine,
                    &changes,
                    &mut profile_written,
                )
                .await
                .map_err(native_error)?;
            }
        }
        if !profile_written {
            return Err(BrowserBackendError::Lifecycle {
                operation: "synchronize native SharedWorker cookies".into(),
                state: "no-live-context".into(),
                reason: "accepted cookie changes have no live profile writer".into(),
            });
        }
        Ok(NativeQueuedBrowserEffects::default())
    }

    async fn deliver_page_message_port(
        &self,
        command: NativePageMessagePortCommand,
    ) -> Result<NativeQueuedBrowserEffects, BrowserBackendError> {
        validate_page_message_port_command(&command).map_err(native_error)?;
        validate_native_topology_id(&command.source_context_id)?;
        validate_native_topology_id(&command.source_frame_id)?;
        let Some(route_info) = self.page_message_port_route(&command.bridge_key)? else {
            return Ok(NativeQueuedBrowserEffects::default());
        };
        if route_info.context_id
            != self
                .frame_owner_context_id(&route_info.frame_id)?
                .unwrap_or_default()
        {
            return Ok(NativeQueuedBrowserEffects::default());
        }
        let Some(route) = self.frame_route(&route_info.frame_id)? else {
            return Ok(NativeQueuedBrowserEffects::default());
        };
        let (runtime_effects, owner_id) = match route {
            NativeFrameRoute::ActiveSelected => {
                let mut engine = self.lock_engine_raw(BackendOperation::Script)?;
                let result =
                    dispatch_page_message_port_to_native_frame(&mut engine, &command).await?;
                let owner_id = engine.config().context_id.clone();
                (result, owner_id)
            }
            NativeFrameRoute::ActiveParked => {
                let mut targets = self.lock_targets(BackendOperation::Script)?;
                let owner_id = targets.active_target_id.clone().ok_or_else(|| {
                    BrowserBackendError::SelectionFailed {
                        reason: "native MessagePort owner target disappeared during delivery"
                            .into(),
                    }
                })?;
                let frame = targets
                    .active_frames
                    .parked
                    .get_mut(&route_info.frame_id)
                    .ok_or_else(|| BrowserBackendError::SelectionFailed {
                        reason: "native MessagePort owner frame disappeared during delivery".into(),
                    })?;
                let result =
                    dispatch_page_message_port_to_native_frame(&mut frame.engine, &command).await?;
                (result, owner_id)
            }
            NativeFrameRoute::ParkedSelected { target_id } => {
                let mut targets = self.lock_targets(BackendOperation::Script)?;
                let target = targets.parked.get_mut(&target_id).ok_or_else(|| {
                    BrowserBackendError::SelectionFailed {
                        reason: "native MessagePort owner target disappeared during delivery"
                            .into(),
                    }
                })?;
                let result =
                    dispatch_page_message_port_to_native_frame(&mut target.engine, &command)
                        .await?;
                (result, target_id)
            }
            NativeFrameRoute::ParkedParked { target_id } => {
                let mut targets = self.lock_targets(BackendOperation::Script)?;
                let target = targets.parked.get_mut(&target_id).ok_or_else(|| {
                    BrowserBackendError::SelectionFailed {
                        reason: "native MessagePort owner target disappeared during delivery"
                            .into(),
                    }
                })?;
                let frame = target
                    .frames
                    .parked
                    .get_mut(&route_info.frame_id)
                    .ok_or_else(|| BrowserBackendError::SelectionFailed {
                        reason: "native MessagePort owner frame disappeared during delivery".into(),
                    })?;
                let result =
                    dispatch_page_message_port_to_native_frame(&mut frame.engine, &command).await?;
                (result, target_id)
            }
        };
        self.sync_target_name(&owner_id, &runtime_effects.window_name)?;
        self.process_frame_event_effects(&route_info.frame_id, runtime_effects.events)
            .await?;
        Box::pin(self.process_pending_frame_scripts(runtime_effects.frame_scripts)).await?;
        if command.close {
            self.remove_page_message_port_route(&command.bridge_key)?;
            self.shared_workers
                .lock()
                .map_err(|_| {
                    poisoned_lock_error(BackendOperation::Script, "SharedWorker coordinator")
                })?
                .page_ports
                .remove(&command.bridge_key);
        } else {
            self.register_page_message_port_routes(
                &command.source_context_id,
                &command.source_frame_id,
                &command.transfer_ports,
            )?;
        }
        Ok(runtime_effects.browser)
    }

    async fn navigate_active_frame_request(
        &self,
        url: &str,
        proxy_updates: &[NativeWindowProxyUpdate],
    ) -> Result<BackendResponse, BrowserBackendError> {
        let cancellation = self.active_navigation_cancellation()?;
        let (
            owner_id,
            frame_id,
            lifecycle,
            runtime,
            document_replaced,
            outgoing_document_generation,
            destroyed_descendant_frame_ids,
        ) = {
            let mut targets = self.lock_targets(BackendOperation::Navigate)?;
            let mut engine = self.lock_engine_raw(BackendOperation::Navigate)?;
            let owner_id =
                targets
                    .active_target_id
                    .clone()
                    .ok_or_else(|| BrowserBackendError::Lifecycle {
                        operation: "navigate".into(),
                        state: "no-target-selected".into(),
                        reason: "select an available native page target before navigating".into(),
                    })?;
            let frame_id = targets.active_frames.active_frame_id.clone();
            let destroyed_descendant_frame_ids = targets.active_frames.descendant_ids(&frame_id);
            engine
                .sync_window_proxies(proxy_updates)
                .await
                .map_err(native_error)?;
            if cancellation
                .as_ref()
                .is_some_and(NativeNavigationCancellation::is_cancelled)
            {
                return Err(native_error(NativeEngineError::NavigationCancelled));
            }
            let initial_generation = engine.document_generation().map_err(native_error)?;
            let requires_document_lifecycle = engine.navigation_requires_document_lifecycle(url);
            let replacement_sandboxed_modals = if requires_document_lifecycle {
                native_frame_sandboxed_modals_for_replacement(
                    &targets.active_frames,
                    &engine,
                    &frame_id,
                )?
            } else {
                targets.active_frames.active_sandboxed_modals
            };
            let lifecycle = if requires_document_lifecycle {
                let sandboxed_modals = targets.active_frames.active_sandboxed_modals;
                dispatch_native_frame_tree_lifecycle(
                    &mut engine,
                    &mut targets.active_frames,
                    &frame_id,
                    sandboxed_modals,
                    cancellation.as_ref(),
                )
                .await?
            } else {
                NativeFrameLifecycleResult {
                    allowed: true,
                    cancelled: false,
                    navigation: None,
                    effects: Vec::new(),
                }
            };
            let mut runtime = NativeFrameRuntimeEffects {
                browser: NativeQueuedBrowserEffects::default(),
                frame_scripts: Vec::new(),
                events: Vec::new(),
                window_name: engine.config().window_name.clone(),
            };
            let mut document_replaced = false;
            if lifecycle.allowed {
                let previous_revision = engine.revision();
                let navigation = lifecycle
                    .navigation
                    .clone()
                    .unwrap_or_else(|| NativeNavigationRequest::get(url));
                engine
                    .navigate_request_async_after_lifecycle(navigation, 0, cancellation)
                    .await
                    .map_err(native_error)?;
                runtime = take_native_frame_runtime_effects(&mut engine, previous_revision)?;
                document_replaced =
                    engine.document_generation().map_err(native_error)? != initial_generation;
                if document_replaced {
                    targets.active_frames.active_sandboxed_modals = replacement_sandboxed_modals;
                }
            }
            (
                owner_id,
                frame_id,
                lifecycle,
                runtime,
                document_replaced,
                initial_generation,
                destroyed_descendant_frame_ids,
            )
        };

        let lifecycle_browser = self
            .process_native_frame_lifecycle_effects(&owner_id, lifecycle.effects)
            .await?;
        self.process_pending_browser_effects(
            lifecycle_browser.0,
            lifecycle_browser.1,
            lifecycle_browser.2,
            lifecycle_browser.3,
            lifecycle_browser.4,
            lifecycle_browser.5,
            lifecycle_browser.6,
        )
        .await?;
        if document_replaced {
            self.teardown_replaced_document(
                &owner_id,
                &frame_id,
                outgoing_document_generation,
                &destroyed_descendant_frame_ids,
            )
            .await?;
            let mut targets = self.lock_targets(BackendOperation::Navigate)?;
            close_native_frame_descendants(&mut targets.active_frames, &frame_id).await?;
            targets.active_frames.discovered_generation = None;
        }
        let lifecycle_cancelled = lifecycle.cancelled;
        self.sync_target_name(&owner_id, &runtime.window_name)?;
        self.process_frame_event_effects(&frame_id, runtime.events)
            .await?;
        Box::pin(self.process_pending_frame_scripts(runtime.frame_scripts)).await?;
        let browser = runtime.browser;
        self.process_pending_browser_effects(
            browser.0, browser.1, browser.2, browser.3, browser.4, browser.5, browser.6,
        )
        .await?;
        self.synchronize_native_service_worker_clients().await?;
        if lifecycle_cancelled {
            return Err(native_error(NativeEngineError::NavigationCancelled));
        }
        let snapshot = self
            .lock_engine(BackendOperation::Navigate)?
            .snapshot()
            .map_err(native_error)?;
        Ok(BackendResponse::Navigation(NavigationResult {
            url: snapshot.url,
            revision: snapshot.revision,
        }))
    }

    async fn navigate_frame_target(
        &self,
        route: NativeFrameRoute,
        frame_id: &str,
        source_context_id: &str,
        navigation: &NativeNavigationRequest,
    ) -> Result<NativeQueuedBrowserEffects, BrowserBackendError> {
        let proxy_updates = self.window_proxy_updates(source_context_id)?;
        let cleanup_route = route.clone();
        let (navigation_result, owner_id) = match route {
            NativeFrameRoute::ActiveSelected => {
                let mut targets = self.lock_targets(BackendOperation::Navigate)?;
                let mut engine = self.lock_engine_raw(BackendOperation::Navigate)?;
                let sandboxed_modals = targets.active_frames.active_sandboxed_modals;
                let replacement_sandboxed_modals =
                    if engine.navigation_requires_document_lifecycle(&navigation.url) {
                        native_frame_sandboxed_modals_for_replacement(
                            &targets.active_frames,
                            &engine,
                            frame_id,
                        )?
                    } else {
                        sandboxed_modals
                    };
                let result = navigate_native_frame(
                    &mut engine,
                    navigation,
                    &proxy_updates,
                    &mut targets.active_frames,
                    frame_id,
                    sandboxed_modals,
                    replacement_sandboxed_modals,
                )
                .await?;
                if result.document_replaced {
                    targets.active_frames.active_sandboxed_modals =
                        result.replacement_sandboxed_modals;
                }
                let owner_id = engine.config().context_id.clone();
                (result, owner_id)
            }
            NativeFrameRoute::ActiveParked => {
                let mut targets = self.lock_targets(BackendOperation::Navigate)?;
                let owner_id = targets.active_target_id.clone().ok_or_else(|| {
                    BrowserBackendError::SelectionFailed {
                        reason: "native frame owner target disappeared during navigation".into(),
                    }
                })?;
                let mut active_engine = self.lock_engine_raw(BackendOperation::Navigate)?;
                let replacement_sandboxed_modals =
                    if active_engine.navigation_requires_document_lifecycle(&navigation.url) {
                        native_frame_sandboxed_modals_for_replacement(
                            &targets.active_frames,
                            &active_engine,
                            frame_id,
                        )?
                    } else {
                        targets
                            .active_frames
                            .parked
                            .get(frame_id)
                            .map(|frame| frame.sandboxed_modals)
                            .ok_or_else(|| BrowserBackendError::SelectionFailed {
                                reason: "native frame disappeared during navigation".into(),
                            })?
                    };
                if targets
                    .active_frames
                    .is_descendant(&targets.active_frames.active_frame_id, frame_id)
                {
                    let previous_selection = activate_parked_frame_for_navigation(
                        &mut targets.active_frames,
                        &mut active_engine,
                        frame_id,
                    )?;
                    let sandboxed_modals = targets.active_frames.active_sandboxed_modals;
                    let result = navigate_native_frame(
                        &mut active_engine,
                        navigation,
                        &proxy_updates,
                        &mut targets.active_frames,
                        frame_id,
                        sandboxed_modals,
                        replacement_sandboxed_modals,
                    )
                    .await;
                    let restore_selection = match &result {
                        Ok(result) => !result.document_replaced,
                        Err(_) => true,
                    };
                    if restore_selection {
                        restore_previous_frame_selection(
                            &mut targets.active_frames,
                            &mut active_engine,
                            previous_selection,
                        )?;
                    } else {
                        targets.active_frames.active_sandboxed_modals =
                            replacement_sandboxed_modals;
                    }
                    (result?, owner_id)
                } else {
                    let mut frame =
                        targets
                            .active_frames
                            .parked
                            .remove(frame_id)
                            .ok_or_else(|| BrowserBackendError::SelectionFailed {
                                reason: "native frame disappeared during navigation".into(),
                            })?;
                    let result = navigate_native_frame(
                        &mut frame.engine,
                        navigation,
                        &proxy_updates,
                        &mut targets.active_frames,
                        frame_id,
                        frame.sandboxed_modals,
                        replacement_sandboxed_modals,
                    )
                    .await;
                    if let Ok(result) = &result
                        && result.document_replaced
                    {
                        frame.sandboxed_modals = result.replacement_sandboxed_modals;
                    }
                    targets
                        .active_frames
                        .parked
                        .insert(frame_id.to_owned(), frame);
                    (result?, owner_id)
                }
            }
            NativeFrameRoute::ParkedSelected { target_id } => {
                let mut targets = self.lock_targets(BackendOperation::Navigate)?;
                let target = targets.parked.get_mut(&target_id).ok_or_else(|| {
                    BrowserBackendError::SelectionFailed {
                        reason: "native frame owner target disappeared during navigation".into(),
                    }
                })?;
                let sandboxed_modals = target.frames.active_sandboxed_modals;
                let replacement_sandboxed_modals = if target
                    .engine
                    .navigation_requires_document_lifecycle(&navigation.url)
                {
                    native_frame_sandboxed_modals_for_replacement(
                        &target.frames,
                        &target.engine,
                        frame_id,
                    )?
                } else {
                    sandboxed_modals
                };
                let result = navigate_native_frame(
                    &mut target.engine,
                    navigation,
                    &proxy_updates,
                    &mut target.frames,
                    frame_id,
                    sandboxed_modals,
                    replacement_sandboxed_modals,
                )
                .await?;
                if result.document_replaced {
                    target.frames.active_sandboxed_modals = result.replacement_sandboxed_modals;
                }
                (result, target_id)
            }
            NativeFrameRoute::ParkedParked { target_id } => {
                let mut targets = self.lock_targets(BackendOperation::Navigate)?;
                let target = targets.parked.get_mut(&target_id).ok_or_else(|| {
                    BrowserBackendError::SelectionFailed {
                        reason: "native frame owner target disappeared during navigation".into(),
                    }
                })?;
                let replacement_sandboxed_modals = if target
                    .engine
                    .navigation_requires_document_lifecycle(&navigation.url)
                {
                    native_frame_sandboxed_modals_for_replacement(
                        &target.frames,
                        &target.engine,
                        frame_id,
                    )?
                } else {
                    target
                        .frames
                        .parked
                        .get(frame_id)
                        .map(|frame| frame.sandboxed_modals)
                        .ok_or_else(|| BrowserBackendError::SelectionFailed {
                            reason: "native frame disappeared during navigation".into(),
                        })?
                };
                let result = if target
                    .frames
                    .is_descendant(&target.frames.active_frame_id, frame_id)
                {
                    let previous_selection = activate_parked_frame_for_navigation(
                        &mut target.frames,
                        &mut target.engine,
                        frame_id,
                    )?;
                    let sandboxed_modals = target.frames.active_sandboxed_modals;
                    let result = navigate_native_frame(
                        &mut target.engine,
                        navigation,
                        &proxy_updates,
                        &mut target.frames,
                        frame_id,
                        sandboxed_modals,
                        replacement_sandboxed_modals,
                    )
                    .await;
                    let restore_selection = match &result {
                        Ok(result) => !result.document_replaced,
                        Err(_) => true,
                    };
                    if restore_selection {
                        restore_previous_frame_selection(
                            &mut target.frames,
                            &mut target.engine,
                            previous_selection,
                        )?;
                    } else {
                        target.frames.active_sandboxed_modals = replacement_sandboxed_modals;
                    }
                    result?
                } else {
                    let mut frame = target.frames.parked.remove(frame_id).ok_or_else(|| {
                        BrowserBackendError::SelectionFailed {
                            reason: "native frame disappeared during navigation".into(),
                        }
                    })?;
                    let result = navigate_native_frame(
                        &mut frame.engine,
                        navigation,
                        &proxy_updates,
                        &mut target.frames,
                        frame_id,
                        frame.sandboxed_modals,
                        replacement_sandboxed_modals,
                    )
                    .await;
                    if let Ok(result) = &result
                        && result.document_replaced
                    {
                        frame.sandboxed_modals = result.replacement_sandboxed_modals;
                    }
                    target.frames.parked.insert(frame_id.to_owned(), frame);
                    result?
                };
                (result, target_id)
            }
        };
        let lifecycle_browser = self
            .process_native_frame_lifecycle_effects(&owner_id, navigation_result.lifecycle.effects)
            .await?;
        self.process_pending_browser_effects(
            lifecycle_browser.0,
            lifecycle_browser.1,
            lifecycle_browser.2,
            lifecycle_browser.3,
            lifecycle_browser.4,
            lifecycle_browser.5,
            lifecycle_browser.6,
        )
        .await?;

        if navigation_result.document_replaced {
            self.teardown_replaced_document(
                &owner_id,
                frame_id,
                navigation_result.outgoing_document_generation,
                &navigation_result.destroyed_descendant_frame_ids,
            )
            .await?;
            match cleanup_route {
                NativeFrameRoute::ActiveSelected => {
                    let mut targets = self.lock_targets(BackendOperation::Navigate)?;
                    close_native_frame_descendants(&mut targets.active_frames, frame_id).await?;
                    targets.active_frames.discovered_generation = None;
                }
                NativeFrameRoute::ActiveParked => {
                    let mut targets = self.lock_targets(BackendOperation::Navigate)?;
                    if targets
                        .active_frames
                        .is_descendant(&targets.active_frames.active_frame_id, frame_id)
                    {
                        let mut engine = self.lock_engine_raw(BackendOperation::Navigate)?;
                        activate_navigated_frame_if_ancestor(
                            &mut targets.active_frames,
                            &mut engine,
                            frame_id,
                        )
                        .await?;
                    } else {
                        close_native_frame_descendants(&mut targets.active_frames, frame_id)
                            .await?;
                    }
                }
                NativeFrameRoute::ParkedSelected { target_id } => {
                    let mut targets = self.lock_targets(BackendOperation::Navigate)?;
                    let target = targets.parked.get_mut(&target_id).ok_or_else(|| {
                        BrowserBackendError::SelectionFailed {
                            reason: "native frame owner target disappeared after navigation".into(),
                        }
                    })?;
                    close_native_frame_descendants(&mut target.frames, frame_id).await?;
                    target.frames.discovered_generation = None;
                }
                NativeFrameRoute::ParkedParked { target_id } => {
                    let mut targets = self.lock_targets(BackendOperation::Navigate)?;
                    let target = targets.parked.get_mut(&target_id).ok_or_else(|| {
                        BrowserBackendError::SelectionFailed {
                            reason: "native frame owner target disappeared after navigation".into(),
                        }
                    })?;
                    if target
                        .frames
                        .is_descendant(&target.frames.active_frame_id, frame_id)
                    {
                        activate_navigated_frame_if_ancestor(
                            &mut target.frames,
                            &mut target.engine,
                            frame_id,
                        )
                        .await?;
                    } else {
                        close_native_frame_descendants(&mut target.frames, frame_id).await?;
                    }
                }
            }
        }
        self.sync_target_name(&owner_id, &navigation_result.runtime.window_name)?;
        self.process_frame_event_effects(frame_id, navigation_result.runtime.events)
            .await?;
        Box::pin(self.process_pending_frame_scripts(navigation_result.runtime.frame_scripts))
            .await?;
        Ok(navigation_result.runtime.browser)
    }

    async fn navigate_named_target(
        &self,
        target_id: &str,
        active: bool,
        navigation: &NativeNavigationRequest,
    ) -> Result<
        (
            PageTargetInfo,
            Vec<NativePopupRequest>,
            Vec<NativePostMessageRequest>,
            Vec<NativeWindowCloseRequest>,
            Vec<NativeWindowNavigationRequest>,
            Vec<NativeServiceWorkerOpenWindowRequest>,
            Vec<NativeServiceWorkerClientMessage>,
            Vec<NativeWorkerCoordinatorEffect>,
        ),
        BrowserBackendError,
    > {
        let proxy_updates = self.window_proxy_updates(target_id)?;
        if active {
            let (target, owner_id, frame_id, navigation_result) = {
                let mut targets = self.lock_targets(BackendOperation::Navigate)?;
                let opener_id = targets.active_opener_id.clone();
                let mut engine = self.lock_engine_raw(BackendOperation::Navigate)?;
                let frame_id = targets.active_frames.active_frame_id.clone();
                let sandboxed_modals = targets.active_frames.active_sandboxed_modals;
                let replacement_sandboxed_modals =
                    if engine.navigation_requires_document_lifecycle(&navigation.url) {
                        native_frame_sandboxed_modals_for_replacement(
                            &targets.active_frames,
                            &engine,
                            &frame_id,
                        )?
                    } else {
                        sandboxed_modals
                    };
                let navigation_result = navigate_native_frame(
                    &mut engine,
                    navigation,
                    &proxy_updates,
                    &mut targets.active_frames,
                    &frame_id,
                    sandboxed_modals,
                    replacement_sandboxed_modals,
                )
                .await?;
                if navigation_result.document_replaced {
                    targets.active_frames.active_sandboxed_modals =
                        navigation_result.replacement_sandboxed_modals;
                }
                let target = project_native_target(&engine, target_id, opener_id, true)?;
                (target, target_id.to_owned(), frame_id, navigation_result)
            };
            let lifecycle_browser = self
                .process_native_frame_lifecycle_effects(
                    &owner_id,
                    navigation_result.lifecycle.effects,
                )
                .await?;
            Box::pin(self.process_pending_browser_effects(
                lifecycle_browser.0,
                lifecycle_browser.1,
                lifecycle_browser.2,
                lifecycle_browser.3,
                lifecycle_browser.4,
                lifecycle_browser.5,
                lifecycle_browser.6,
            ))
            .await?;
            if navigation_result.document_replaced {
                self.close_shared_worker_owners_for_context(&owner_id)
                    .await?;
                let mut targets = self.lock_targets(BackendOperation::Navigate)?;
                close_parked_frames(&mut targets.active_frames).await?;
                targets.active_frames = NativeFrameState::new(target_id);
                targets.active_name = native_window_name(&navigation_result.runtime.window_name);
            }
            self.sync_target_name(&owner_id, &navigation_result.runtime.window_name)?;
            self.process_frame_event_effects(&frame_id, navigation_result.runtime.events)
                .await?;
            Box::pin(self.process_pending_frame_scripts(navigation_result.runtime.frame_scripts))
                .await?;
            let browser = navigation_result.runtime.browser;
            Ok((
                target, browser.0, browser.1, browser.2, browser.3, browser.4, browser.5, browser.6,
            ))
        } else {
            let (target, frame_id, navigation_result) = {
                let mut targets = self.lock_targets(BackendOperation::Navigate)?;
                let parked = targets.parked.remove(target_id).ok_or_else(|| {
                    BrowserBackendError::SelectionFailed {
                        reason: "named native popup target disappeared before navigation".into(),
                    }
                })?;
                let NativeParkedTarget {
                    mut engine,
                    opener_id,
                    name: _previous_name,
                    mut frames,
                } = parked;
                let frame_id = frames.active_frame_id.clone();
                let sandboxed_modals = frames.active_sandboxed_modals;
                let replacement_sandboxed_modals =
                    if engine.navigation_requires_document_lifecycle(&navigation.url) {
                        native_frame_sandboxed_modals_for_replacement(&frames, &engine, &frame_id)?
                    } else {
                        sandboxed_modals
                    };
                let navigation_result = navigate_native_frame(
                    &mut engine,
                    navigation,
                    &proxy_updates,
                    &mut frames,
                    &frame_id,
                    sandboxed_modals,
                    replacement_sandboxed_modals,
                )
                .await;
                let navigation_result = match navigation_result {
                    Ok(result) => result,
                    Err(error) => {
                        targets.parked.insert(
                            target_id.to_owned(),
                            NativeParkedTarget {
                                name: native_window_name(&engine.config().window_name),
                                engine,
                                opener_id,
                                frames,
                            },
                        );
                        return Err(error);
                    }
                };
                if navigation_result.document_replaced {
                    frames.active_sandboxed_modals = navigation_result.replacement_sandboxed_modals;
                }
                let target = project_native_target(&engine, target_id, opener_id.clone(), false)?;
                let window_name = engine.config().window_name.clone();
                targets.parked.insert(
                    target_id.to_owned(),
                    NativeParkedTarget {
                        engine,
                        opener_id,
                        frames,
                        name: native_window_name(&window_name),
                    },
                );
                (target, frame_id, navigation_result)
            };
            let lifecycle_browser = self
                .process_native_frame_lifecycle_effects(
                    target_id,
                    navigation_result.lifecycle.effects,
                )
                .await?;
            Box::pin(self.process_pending_browser_effects(
                lifecycle_browser.0,
                lifecycle_browser.1,
                lifecycle_browser.2,
                lifecycle_browser.3,
                lifecycle_browser.4,
                lifecycle_browser.5,
                lifecycle_browser.6,
            ))
            .await?;
            if navigation_result.document_replaced {
                self.close_shared_worker_owners_for_context(target_id)
                    .await?;
                let mut targets = self.lock_targets(BackendOperation::Navigate)?;
                let mut parked = targets.parked.remove(target_id).ok_or_else(|| {
                    BrowserBackendError::SelectionFailed {
                        reason: "named native popup target disappeared after navigation".into(),
                    }
                })?;
                close_parked_frames(&mut parked.frames).await?;
                parked.frames = NativeFrameState::new(target_id);
                parked.name = native_window_name(&navigation_result.runtime.window_name);
                targets.parked.insert(target_id.to_owned(), parked);
            }
            self.sync_target_name(target_id, &navigation_result.runtime.window_name)?;
            self.process_frame_event_effects(&frame_id, navigation_result.runtime.events)
                .await?;
            Box::pin(self.process_pending_frame_scripts(navigation_result.runtime.frame_scripts))
                .await?;
            let browser = navigation_result.runtime.browser;
            Ok((
                target, browser.0, browser.1, browser.2, browser.3, browser.4, browser.5, browser.6,
            ))
        }
    }

    /// Close one target and release its worker, storage lease, and document
    /// owner. Closing the selected target deliberately leaves no implicit
    /// selection, even when parked targets remain available.
    pub async fn close_target(&self, target_id: &str) -> Result<(), BrowserBackendError> {
        validate_native_topology_id(target_id)?;
        let mut frames = {
            let mut targets = self.lock_targets(BackendOperation::Close)?;
            if targets.active_target_id.as_deref() == Some(target_id) {
                let mut engine = self.lock_engine_raw(BackendOperation::Close)?;
                let closed_url = engine.context().map_err(native_error)?.url;
                let closed_name = targets.active_name.clone().unwrap_or_default();
                engine.close_async().await.map_err(native_error)?;
                let frames =
                    std::mem::replace(&mut targets.active_frames, NativeFrameState::empty());
                targets.active_target_id = None;
                targets.active_opener_id = None;
                targets.active_name = None;
                targets.remember_closed_target(target_id.to_owned(), closed_url, closed_name);
                frames
            } else {
                let Some(mut parked) = targets.parked.remove(target_id) else {
                    return Err(BrowserBackendError::SelectionFailed {
                        reason:
                            "native page target was not found; call listTargets to refresh topology"
                                .into(),
                    });
                };
                let closed_url = parked.engine.context().map_err(native_error)?.url;
                let closed_name = parked.name.clone().unwrap_or_default();
                if let Err(error) = parked.engine.close_async().await {
                    targets.parked.insert(target_id.to_owned(), parked);
                    return Err(native_error(error));
                }
                targets.remember_closed_target(target_id.to_owned(), closed_url, closed_name);
                parked.frames
            }
        };
        let worker_close_result = self.close_shared_worker_owners_for_context(target_id).await;
        let frames_close_result = close_parked_frames(&mut frames).await;
        worker_close_result?;
        frames_close_result
    }

    async fn close_all(&self) -> Result<(), BrowserBackendError> {
        let (active_error, active_frames, parked) = {
            let mut targets = self.lock_targets(BackendOperation::Close)?;
            let mut active_engine = self.lock_engine_raw(BackendOperation::Close)?;
            let active_error = if targets.active_target_id.is_some()
                && active_engine.lifecycle() == super::native_engine::NativeLifecycleState::Running
            {
                active_engine.close_async().await.err().map(native_error)
            } else {
                None
            };
            let active_frames =
                std::mem::replace(&mut targets.active_frames, NativeFrameState::empty());
            targets.active_target_id = None;
            targets.active_opener_id = None;
            targets.active_name = None;
            targets.window_handles.clear();
            (
                active_error,
                active_frames,
                std::mem::take(&mut targets.parked),
            )
        };
        self.clear_page_message_port_routes()?;

        let mut first_error = active_error;
        let mut active_frames = active_frames;
        if let Err(error) = close_parked_frames(&mut active_frames).await {
            first_error.get_or_insert(error);
        }
        for (_, mut parked) in parked {
            if parked.engine.lifecycle() == super::native_engine::NativeLifecycleState::Running
                && let Err(error) = parked.engine.close_async().await.map_err(native_error)
                && first_error.is_none()
            {
                first_error = Some(error);
            }
            if let Err(error) = close_parked_frames(&mut parked.frames).await
                && first_error.is_none()
            {
                first_error = Some(error);
            }
        }
        first_error.map_or(Ok(()), Err)
    }

    pub async fn select_frame(&self, frame_id: &str) -> Result<FrameInfo, BrowserBackendError> {
        validate_native_topology_id(frame_id)?;
        self.list_frames()
            .await?
            .into_iter()
            .find(|frame| frame.id == frame_id)
            .ok_or_else(|| BrowserBackendError::SelectionFailed {
                reason: "native frame was not found; call listFrames to refresh topology".into(),
            })?;
        let mut targets = self.lock_targets(BackendOperation::Contexts)?;
        if targets.active_frames.active_frame_id == frame_id {
            targets.active_frames.focused_frame_id = Some(frame_id.to_owned());
            let engine = self.lock_engine_raw(BackendOperation::Contexts)?;
            return project_native_frame(
                &engine,
                frame_id,
                targets.active_frames.active_parent_id.clone(),
                true,
            );
        }
        let Some(parked) = targets.active_frames.parked.remove(frame_id) else {
            return Err(BrowserBackendError::SelectionFailed {
                reason: "native frame was not found; call listFrames to refresh topology".into(),
            });
        };
        let NativeParkedFrame {
            engine: parked_engine,
            parent_id,
            owner_node_index,
            sandboxed_modals,
        } = parked;
        let mut active_engine = self.lock_engine_raw(BackendOperation::Contexts)?;
        let old_engine = std::mem::replace(&mut *active_engine, parked_engine);
        let old_frame_id = std::mem::replace(
            &mut targets.active_frames.active_frame_id,
            frame_id.to_owned(),
        );
        let old_parent_id =
            std::mem::replace(&mut targets.active_frames.active_parent_id, parent_id);
        let old_owner_node_index = std::mem::replace(
            &mut targets.active_frames.active_owner_node_index,
            owner_node_index,
        );
        let old_sandboxed_modals = std::mem::replace(
            &mut targets.active_frames.active_sandboxed_modals,
            sandboxed_modals,
        );
        targets.active_frames.focused_frame_id = Some(frame_id.to_owned());
        targets.active_frames.discovered_generation =
            Some(active_engine.document_generation().map_err(native_error)?);
        targets.active_frames.parked.insert(
            old_frame_id,
            NativeParkedFrame {
                engine: old_engine,
                parent_id: old_parent_id,
                owner_node_index: old_owner_node_index,
                sandboxed_modals: old_sandboxed_modals,
            },
        );
        project_native_frame(
            &active_engine,
            frame_id,
            targets.active_frames.active_parent_id.clone(),
            true,
        )
    }

    pub async fn navigate_history(
        &self,
        direction: NativeHistoryDirection,
    ) -> Result<NavigationControlOutcome, BrowserBackendError> {
        let (
            previous_revision,
            owner_id,
            frame_id,
            lifecycle,
            runtime,
            document_replaced,
            initial_generation,
            destroyed_descendant_frame_ids,
            snapshot,
        ) = {
            let mut targets = self.lock_targets(BackendOperation::Navigate)?;
            let mut engine = self.lock_engine_raw(BackendOperation::Navigate)?;
            let previous_revision = engine.revision();
            let initial_generation = engine.document_generation().map_err(native_error)?;
            let frame_id = targets.active_frames.active_frame_id.clone();
            let destroyed_descendant_frame_ids = targets.active_frames.descendant_ids(&frame_id);
            let cross_document = engine.history_target_is_cross_document(direction) == Some(true);
            let replacement_sandboxed_modals = if cross_document {
                native_frame_sandboxed_modals_for_replacement(
                    &targets.active_frames,
                    &engine,
                    &frame_id,
                )?
            } else {
                targets.active_frames.active_sandboxed_modals
            };
            let lifecycle = if cross_document {
                let sandboxed_modals = targets.active_frames.active_sandboxed_modals;
                dispatch_native_frame_tree_lifecycle(
                    &mut engine,
                    &mut targets.active_frames,
                    &frame_id,
                    sandboxed_modals,
                    None,
                )
                .await?
            } else {
                NativeFrameLifecycleResult {
                    allowed: true,
                    cancelled: false,
                    navigation: None,
                    effects: Vec::new(),
                }
            };
            let mut runtime = NativeFrameRuntimeEffects {
                browser: NativeQueuedBrowserEffects::default(),
                frame_scripts: Vec::new(),
                events: Vec::new(),
                window_name: engine.config().window_name.clone(),
            };
            let snapshot = if lifecycle.allowed {
                let navigation_revision = engine.revision();
                let snapshot = if let Some(navigation) = lifecycle.navigation.clone() {
                    Some(
                        engine
                            .navigate_request_async_after_lifecycle(navigation, 0, None)
                            .await
                            .map_err(native_error)?,
                    )
                } else {
                    engine
                        .traverse_history_async_after_lifecycle(direction)
                        .await
                        .map_err(native_error)?
                };
                runtime = take_native_frame_runtime_effects(&mut engine, navigation_revision)?;
                snapshot
            } else {
                Some(engine.snapshot().map_err(native_error)?)
            };
            let document_replaced =
                engine.document_generation().map_err(native_error)? != initial_generation;
            if document_replaced {
                targets.active_frames.active_sandboxed_modals = replacement_sandboxed_modals;
            }
            (
                previous_revision,
                engine.config().context_id.clone(),
                frame_id,
                lifecycle,
                runtime,
                document_replaced,
                initial_generation,
                destroyed_descendant_frame_ids,
                snapshot,
            )
        };
        let snapshot = snapshot.ok_or_else(|| BrowserBackendError::UnsupportedOperation {
            operation: match direction {
                NativeHistoryDirection::Back => "back",
                NativeHistoryDirection::Forward => "forward",
            }
            .into(),
            reason: match direction {
                NativeHistoryDirection::Back => "no previous history entry",
                NativeHistoryDirection::Forward => "no forward history entry",
            }
            .into(),
        })?;
        let lifecycle_browser = self
            .process_native_frame_lifecycle_effects(&owner_id, lifecycle.effects)
            .await?;
        self.process_pending_browser_effects(
            lifecycle_browser.0,
            lifecycle_browser.1,
            lifecycle_browser.2,
            lifecycle_browser.3,
            lifecycle_browser.4,
            lifecycle_browser.5,
            lifecycle_browser.6,
        )
        .await?;
        if document_replaced {
            self.teardown_replaced_document(
                &owner_id,
                &frame_id,
                initial_generation,
                &destroyed_descendant_frame_ids,
            )
            .await?;
            let mut targets = self.lock_targets(BackendOperation::Navigate)?;
            close_native_frame_descendants(&mut targets.active_frames, &frame_id).await?;
            targets.active_frames.discovered_generation = None;
        }
        self.sync_target_name(&owner_id, &runtime.window_name)?;
        self.process_frame_event_effects(&frame_id, runtime.events)
            .await?;
        Box::pin(self.process_pending_frame_scripts(runtime.frame_scripts)).await?;
        let browser = runtime.browser;
        self.process_pending_browser_effects(
            browser.0, browser.1, browser.2, browser.3, browser.4, browser.5, browser.6,
        )
        .await?;
        self.synchronize_native_service_worker_clients().await?;
        Ok(NavigationControlOutcome {
            action: match direction {
                NativeHistoryDirection::Back => "back",
                NativeHistoryDirection::Forward => "forward",
            }
            .into(),
            previous_revision,
            current_revision: snapshot.revision,
        })
    }

    /// Reload the selected native page without adding a history entry.
    ///
    /// Reload goes through the same target/effect pipeline as an ordinary
    /// navigation so lifecycle scripts, frame state, and browser-owned
    /// effects remain synchronized with the selected target.
    pub async fn reload(&self) -> Result<NavigationControlOutcome, BrowserBackendError> {
        let (target_id, url, previous_revision) =
            {
                let targets = self.lock_targets(BackendOperation::Navigate)?;
                let target_id = targets.active_target_id.clone().ok_or_else(|| {
                    BrowserBackendError::Lifecycle {
                        operation: "reload".into(),
                        state: "no-target-selected".into(),
                        reason: "select an available native page target before reloading".into(),
                    }
                })?;
                let engine = self.lock_engine_raw(BackendOperation::Navigate)?;
                let url = engine.context().map_err(native_error)?.url;
                (target_id, url, engine.revision())
            };
        let mut navigation = NativeNavigationRequest::get(url);
        navigation.replace_history = true;
        let (
            _,
            popups,
            messages,
            closes,
            navigations,
            service_worker_open_windows,
            service_worker_client_messages,
            page_message_port_commands,
        ) = self
            .navigate_named_target(&target_id, true, &navigation)
            .await?;
        self.process_pending_browser_effects(
            popups,
            messages,
            closes,
            navigations,
            service_worker_open_windows,
            service_worker_client_messages,
            page_message_port_commands,
        )
        .await?;
        let current_revision = self.lock_engine(BackendOperation::Navigate)?.revision();
        Ok(NavigationControlOutcome {
            action: "reload".into(),
            previous_revision,
            current_revision,
        })
    }

    /// Rebuild the active native document owner and reload its current URL.
    /// The operation never changes backend or replays the failed mutation;
    /// it replaces the current history entry and returns the new revision.
    pub async fn recover(&self) -> Result<NavigationControlOutcome, BrowserBackendError> {
        let proxy_updates = self.active_window_proxy_updates()?;
        let (target_id, opener_id, previous_revision) =
            {
                let targets = self.lock_targets(BackendOperation::Navigate)?;
                let target_id = targets.active_target_id.clone().ok_or_else(|| {
                    BrowserBackendError::Lifecycle {
                        operation: "recover".into(),
                        state: "no-target-selected".into(),
                        reason: "select an available native page target before recovery".into(),
                    }
                })?;
                let engine = self.lock_engine_raw(BackendOperation::Navigate)?;
                (
                    target_id,
                    targets.active_opener_id.clone(),
                    engine.revision(),
                )
            };
        let (
            popups,
            messages,
            closes,
            navigations,
            service_worker_open_windows,
            service_worker_client_messages,
            page_message_port_commands,
            window_name,
            current_revision,
            outgoing_document_generation,
            frame_id,
            destroyed_descendant_frame_ids,
        ) = {
            let targets = self.lock_targets(BackendOperation::Navigate)?;
            let frame_id = targets.active_frames.active_frame_id.clone();
            let destroyed_descendant_frame_ids = targets.active_frames.descendant_ids(&frame_id);
            let mut engine = self.lock_engine_raw(BackendOperation::Navigate)?;
            let outgoing_document_generation =
                engine.document_generation().map_err(native_error)?;
            engine
                .sync_window_proxies(&proxy_updates)
                .await
                .map_err(native_error)?;
            let snapshot = engine.recover_async().await.map_err(native_error)?;
            let popups = engine.take_pending_popups();
            let messages = engine.take_pending_post_messages();
            let closes = engine.take_pending_window_closes();
            let navigations = engine.take_pending_window_navigations();
            let service_worker_open_windows = engine.take_pending_service_worker_open_windows();
            let service_worker_client_messages =
                engine.take_pending_service_worker_client_messages();
            let page_message_port_commands = take_native_worker_coordinator_effects(&mut engine);
            let window_name = engine.config().window_name.clone();
            project_native_target(&engine, &target_id, opener_id, true)?;
            (
                popups,
                messages,
                closes,
                navigations,
                service_worker_open_windows,
                service_worker_client_messages,
                page_message_port_commands,
                window_name,
                snapshot.revision,
                outgoing_document_generation,
                frame_id,
                destroyed_descendant_frame_ids,
            )
        };
        self.teardown_replaced_document(
            &target_id,
            &frame_id,
            outgoing_document_generation,
            &destroyed_descendant_frame_ids,
        )
        .await?;
        let mut targets = self.lock_targets(BackendOperation::Contexts)?;
        close_native_frame_descendants(&mut targets.active_frames, &frame_id).await?;
        targets.active_frames = NativeFrameState::new(&target_id);
        targets.active_name = native_window_name(&window_name);
        drop(targets);
        self.process_pending_browser_effects(
            popups,
            messages,
            closes,
            navigations,
            service_worker_open_windows,
            service_worker_client_messages,
            page_message_port_commands,
        )
        .await?;
        Ok(NavigationControlOutcome {
            action: "recover".into(),
            previous_revision,
            current_revision,
        })
    }

    /// Return the logical viewport used by native point routing and capture.
    pub fn viewport_size(&self) -> Result<(f64, f64), BrowserBackendError> {
        let engine = self.lock_engine(BackendOperation::Capture)?;
        Ok((
            f64::from(engine.config().viewport.width),
            f64::from(engine.config().viewport.height),
        ))
    }

    pub fn profile_for(glass_version: &str) -> Result<BackendProfile, BrowserBackendError> {
        if glass_version.is_empty() {
            return Err(BrowserBackendError::InvalidConfiguration {
                field: "glass version".into(),
                reason: "glass version must not be empty".into(),
            });
        }
        let supported = [
            BrowserCapability::Lifecycle,
            BrowserCapability::Navigation,
            BrowserCapability::Contexts,
            BrowserCapability::Evidence,
            BrowserCapability::Action,
            BrowserCapability::Effects,
            BrowserCapability::Script,
            BrowserCapability::Capture,
            BrowserCapability::Storage,
            BrowserCapability::Prompts,
            BrowserCapability::Downloads,
        ];
        let mut capabilities = BTreeMap::new();
        for capability in supported {
            let limitations = match capability {
                BrowserCapability::Navigation => {
                    vec!["bounded HTTP(S) navigation, scripts, forms, validation, and string fetch/FormData/URLSearchParams/XHR; broad subresources and full parser timing remain open".into()]
                }
                BrowserCapability::Evidence => {
                    vec!["bounded URL, title, and visible text only; no DOM or pixels".into()]
                }
                BrowserCapability::Action => {
                    vec![
                        "bounded click/type/key/shortcut/clear/check/select/scroll, form defaults, root and nested scrolling with history restoration, and point targets; advanced selection geometry and IME remain open".into(),
                    ]
                }
                BrowserCapability::Effects => {
                    vec![
                        "bounded changed/revision signal; native event details remain internal"
                            .into(),
                    ]
                }
                BrowserCapability::Script => vec![
                    "bounded QuickJS DOM/event scripting with modules, tasks, policy-owned fetch/CORS, text-backed FormData/File/Blob/URLSearchParams, and async XHR; binary/stream parity, workers, subresources, and Web IDL identity remain open".into(),
                ],
                BrowserCapability::Capture => {
                    vec![
                        "bounded PNG, JPEG, WebP, or PDF of the current logical page surface; screenshot-containing evidence remains a separate evidence schema".into(),
                    ]
                }
                BrowserCapability::Contexts => vec![
                    "up to 32 independent native page targets with one explicitly selected active context".into(),
                ],
                BrowserCapability::Storage => vec![
                    "bounded origin-keyed Web Storage/cookies, IndexedDB stores/transactions/indexes/cursors, and profile persistence; full browser storage parity remains open".into(),
                ],
                BrowserCapability::Prompts => vec![
                    "bounded alert, confirm, and prompt metadata is surfaced and can be accepted or dismissed; modal JavaScript continuation remains a separate browser-loop gate".into(),
                ],
                BrowserCapability::Downloads => vec![
                    "bounded HTTP(S) anchor download attributes, parent-owned queued transfer, safe collision-free file writes, and completion IDs; programmatic download APIs, chooser UI, and streaming transfer parity remain open".into(),
                ],
                BrowserCapability::Lifecycle => {
                    vec!["close is terminal for the engine instance".into()]
                }
            };
            capabilities.insert(
                capability,
                CapabilityDescriptor {
                    level: SupportLevel::Available,
                    portability: Portability::SemanticPortable,
                    dependencies: Vec::new(),
                    limitations,
                },
            );
        }
        let profile = BackendProfile {
            schema_version: BROWSER_BACKEND_SCHEMA_VERSION,
            identity: crate::browser_backend::BackendIdentity {
                backend_id: NATIVE_ENGINE_BACKEND_ID.into(),
                version: NATIVE_ENGINE_BACKEND_VERSION.into(),
                browser: crate::browser_backend::BrowserVersionRange {
                    family: NATIVE_ENGINE_BROWSER_FAMILY.into(),
                    minimum: None,
                    maximum: None,
                },
                certification: CertificationProfile {
                    level: CertificationLevel::Partial,
                    glass_version: glass_version.into(),
                    tested_capabilities: supported.to_vec(),
                    limitations: vec![
                        "network navigation and scripting are bounded web-platform slices, not browser parity".into(),
                        "in-process local execution is not a security boundary for hostile content; external documents use the sandboxed content worker".into(),
                        "bounded Web Storage, cookies, IndexedDB databases/stores/transactions/indexes/cursors/structured values, and profile persistence; full browser storage parity remains open".into(),
                        "actions are limited to bounded click/type/key/shortcut/clear/check/select/scroll, form defaults, root/nested scrolling with history restoration, and point targets".into(),
                        "JavaScript alert, confirm, and prompt calls are surfaced through the native prompt lifecycle".into(),
                        "download links use a bounded parent-owned transfer queue and authorized destination; popup, chooser, and general download API parity remain open".into(),
                    ],
                },
            },
            capabilities,
        };
        profile.validate()?;
        Ok(profile)
    }

    fn lock_targets(
        &self,
        operation: BackendOperation,
    ) -> Result<MutexGuard<'_, NativeTargetState>, BrowserBackendError> {
        self.targets
            .lock()
            .map_err(|_| poisoned_lock_error(operation, "target registry"))
    }

    fn lock_engine_raw(
        &self,
        operation: BackendOperation,
    ) -> Result<MutexGuard<'_, NativeEngine>, BrowserBackendError> {
        self.engine
            .lock()
            .map_err(|_| poisoned_lock_error(operation, "engine"))
    }

    fn lock_engine(
        &self,
        operation: BackendOperation,
    ) -> Result<MutexGuard<'_, NativeEngine>, BrowserBackendError> {
        let has_active_target = self.lock_targets(operation)?.active_target_id.is_some();
        if !has_active_target {
            return Err(BrowserBackendError::Lifecycle {
                operation: operation_name(operation).into(),
                state: "no-target-selected".into(),
                reason: "select an available native page target before this operation".into(),
            });
        }
        self.lock_engine_raw(operation)
    }
}

impl NativeEngineBackend {
    async fn synchronize_native_service_worker_clients(&self) -> Result<(), BrowserBackendError> {
        let mut targets = self.lock_targets(BackendOperation::Contexts)?;
        let mut engine = self.lock_engine_raw(BackendOperation::Contexts)?;
        if targets.active_target_id.is_none()
            || engine.lifecycle() != super::native_engine::NativeLifecycleState::Running
        {
            return Ok(());
        }

        reconcile_native_frame_tree(&mut targets.active_frames, &mut engine).await?;
        let active_context_id = engine.config().context_id.clone();
        let mut retired_frames = std::mem::take(&mut targets.active_frames.pending_retired)
            .into_iter()
            .map(|frame| (active_context_id.clone(), frame))
            .collect::<Vec<_>>();
        for target in targets.parked.values_mut() {
            if target.engine.lifecycle() == super::native_engine::NativeLifecycleState::Running {
                reconcile_native_frame_tree(&mut target.frames, &mut target.engine).await?;
            }
            let target_id = target.engine.config().context_id.clone();
            retired_frames.extend(
                std::mem::take(&mut target.frames.pending_retired)
                    .into_iter()
                    .map(|frame| (target_id.clone(), frame)),
            );
        }

        engine
            .synchronize_service_worker_registrations()
            .await
            .map_err(native_error)?;
        for target in targets.parked.values_mut() {
            if target.engine.lifecycle() == super::native_engine::NativeLifecycleState::Running {
                target
                    .engine
                    .synchronize_service_worker_registrations()
                    .await
                    .map_err(native_error)?;
            }
            for frame in target.frames.parked.values_mut() {
                if frame.engine.lifecycle() == super::native_engine::NativeLifecycleState::Running {
                    frame
                        .engine
                        .synchronize_service_worker_registrations()
                        .await
                        .map_err(native_error)?;
                }
            }
        }
        for frame in targets.active_frames.parked.values_mut() {
            if frame.engine.lifecycle() == super::native_engine::NativeLifecycleState::Running {
                frame
                    .engine
                    .synchronize_service_worker_registrations()
                    .await
                    .map_err(native_error)?;
            }
        }

        let active_frame_id = targets.active_frames.active_frame_id.clone();
        let focused_frame_id = targets
            .active_frames
            .focused_frame_id
            .clone()
            .unwrap_or_else(|| active_frame_id.clone());
        let active_frame_type = if targets.active_frames.active_parent_id.is_some() {
            "nested"
        } else {
            "top-level"
        };
        let mut local_leases = Vec::new();
        let active_focused = focused_frame_id == active_frame_id;
        local_leases.push(engine.service_worker_client_lease(active_frame_type, active_focused));
        for (frame_id, frame) in &targets.active_frames.parked {
            if frame.engine.lifecycle() != super::native_engine::NativeLifecycleState::Running {
                continue;
            }
            let frame_type = if frame.parent_id.is_some() {
                "nested"
            } else {
                "top-level"
            };
            let focused = focused_frame_id == *frame_id;
            local_leases.push(
                frame
                    .engine
                    .service_worker_client_lease(frame_type, focused),
            );
        }
        for target in targets.parked.values() {
            if target.engine.lifecycle() == super::native_engine::NativeLifecycleState::Running {
                let frame_type = if target.frames.active_parent_id.is_some() {
                    "nested"
                } else {
                    "top-level"
                };
                local_leases.push(target.engine.service_worker_client_lease(frame_type, false));
            }
            for frame in target.frames.parked.values() {
                if frame.engine.lifecycle() != super::native_engine::NativeLifecycleState::Running {
                    continue;
                }
                let frame_type = if frame.parent_id.is_some() {
                    "nested"
                } else {
                    "top-level"
                };
                local_leases.push(frame.engine.service_worker_client_lease(frame_type, false));
            }
        }
        let clients = synchronize_service_worker_client_leases(
            engine.config().storage_path.as_deref(),
            &local_leases,
        )
        .map_err(native_error)?;

        engine
            .replace_service_worker_clients(clients.clone())
            .await
            .map_err(native_error)?;
        for target in targets.parked.values_mut() {
            target
                .engine
                .replace_service_worker_clients(clients.clone())
                .await
                .map_err(native_error)?;
            for frame in target.frames.parked.values_mut() {
                frame
                    .engine
                    .replace_service_worker_clients(clients.clone())
                    .await
                    .map_err(native_error)?;
            }
        }
        for frame in targets.active_frames.parked.values_mut() {
            frame
                .engine
                .replace_service_worker_clients(clients.clone())
                .await
                .map_err(native_error)?;
        }
        drop(engine);
        drop(targets);

        let mut first_error = None;
        for (context_id, mut retired) in retired_frames {
            if let Err(error) = self
                .close_shared_worker_owners_for_frame(&context_id, &retired.frame_id)
                .await
            {
                first_error.get_or_insert(error);
            }
            if let Err(error) = self.clear_page_message_port_routes_for_frame(&retired.frame_id) {
                first_error.get_or_insert(error);
            }
            if retired.engine.lifecycle() == super::native_engine::NativeLifecycleState::Running
                && let Err(error) = retired.engine.close_async().await.map_err(native_error)
            {
                first_error.get_or_insert(error);
            }
        }
        first_error.map_or(Ok(()), Err)
    }
}

impl BrowserBackend for NativeEngineBackend {
    fn profile(&self) -> &BackendProfile {
        &self.profile
    }

    fn dispatch<'a>(
        &'a self,
        operation: BackendOperation,
        request: BackendRequest,
    ) -> BackendFuture<'a, BackendResponse> {
        Box::pin(async move {
            request.validate()?;
            self.profile
                .require_operation(operation, SupportLevel::Available)?;
            if matches!(
                (&operation, &request),
                (BackendOperation::Close, BackendRequest::Close)
            ) {
                self.close_all().await?;
                return Ok(BackendResponse::Unit);
            }
            if !matches!(operation, BackendOperation::Initialize) {
                self.synchronize_native_service_worker_clients().await?;
            }
            let proxy_updates = if matches!(
                operation,
                BackendOperation::Navigate
                    | BackendOperation::Action
                    | BackendOperation::Effects
                    | BackendOperation::Script
                    | BackendOperation::Prompt
            ) {
                Some(self.active_window_proxy_updates()?)
            } else {
                None
            };
            let frame_script_bindings = if matches!(operation, BackendOperation::Script) {
                Some(self.frame_script_bindings().await?)
            } else {
                None
            };
            let frame_script_context = if matches!(
                operation,
                BackendOperation::Script | BackendOperation::Action
            ) {
                Some(self.frame_script_context().await?)
            } else {
                None
            };
            let selected_frame_id = if matches!(
                operation,
                BackendOperation::Navigate | BackendOperation::Action | BackendOperation::Script
            ) {
                Some(self.active_frame_id()?)
            } else {
                None
            };
            if let (BackendOperation::Action, BackendRequest::Action(action_request)) =
                (&operation, &request)
            {
                let point_click = match &action_request.action {
                    SemanticAction::Click { target } => Some((target.as_str(), 0)),
                    SemanticAction::ClickWithModifiers { target, modifiers } => {
                        Some((target.as_str(), modifiers.native_mask()))
                    }
                    _ => None,
                };
                if let Some((target, modifiers)) = point_click
                    && let Some(response) = self
                        .dispatch_point_click(&action_request.context_id, target, modifiers)
                        .await?
                {
                    self.synchronize_native_service_worker_clients().await?;
                    return Ok(response);
                }
            }
            if let (BackendOperation::Action, BackendRequest::Action(action_request)) =
                (&operation, &request)
                && let Some(response) = self
                    .dispatch_locator_action(&action_request.context_id, &action_request.action)
                    .await?
            {
                self.synchronize_native_service_worker_clients().await?;
                return Ok(response);
            }
            if let (BackendOperation::Action, BackendRequest::Action(action_request)) =
                (&operation, &request)
            {
                let focused_action = match &action_request.action {
                    SemanticAction::KeyDown { key } => {
                        Some(NativeAction::KeyDown { key: key.clone() })
                    }
                    SemanticAction::KeyUp { key } => Some(NativeAction::KeyUp { key: key.clone() }),
                    SemanticAction::Shortcut { shortcut } => Some(NativeAction::Shortcut {
                        shortcut: shortcut.clone(),
                    }),
                    SemanticAction::KeyPress { key } => {
                        Some(NativeAction::KeyPress { key: key.clone() })
                    }
                    _ => None,
                };
                if let Some(focused_action) = focused_action
                    && let Some(response) = self
                        .dispatch_focused_frame_action(&action_request.context_id, focused_action)
                        .await?
                {
                    self.synchronize_native_service_worker_clients().await?;
                    return Ok(response);
                }
            }
            if let (BackendOperation::Navigate, BackendRequest::Navigate(navigation)) =
                (&operation, &request)
            {
                return self
                    .navigate_active_frame_request(
                        &navigation.url,
                        proxy_updates.as_deref().unwrap_or_default(),
                    )
                    .await;
            }
            let mut engine = self.lock_engine(operation)?;
            if let Some(proxy_updates) = proxy_updates.as_deref() {
                engine
                    .sync_window_proxies(proxy_updates)
                    .await
                    .map_err(native_error)?;
            }
            if let Some(bindings) = frame_script_bindings {
                engine.set_frame_script_bindings(bindings);
            }
            if let Some(context) = frame_script_context {
                engine.set_frame_script_context(context);
            }
            let active_context_id = engine.config().context_id.clone();
            match (operation, request) {
                (BackendOperation::Initialize, BackendRequest::Initialize) => {
                    engine.initialize_async().await.map_err(native_error)?;
                    let popup_requests = engine.take_pending_popups();
                    let post_messages = engine.take_pending_post_messages();
                    let window_closes = engine.take_pending_window_closes();
                    let window_navigations = engine.take_pending_window_navigations();
                    let service_worker_open_windows =
                        engine.take_pending_service_worker_open_windows();
                    let service_worker_client_messages =
                        engine.take_pending_service_worker_client_messages();
                    let page_message_port_commands =
                        take_native_worker_coordinator_effects(&mut engine);
                    let window_name = engine.config().window_name.clone();
                    drop(engine);
                    self.sync_target_name(&active_context_id, &window_name)?;
                    self.process_pending_browser_effects(
                        popup_requests,
                        post_messages,
                        window_closes,
                        window_navigations,
                        service_worker_open_windows,
                        service_worker_client_messages,
                        page_message_port_commands,
                    )
                    .await?;
                    self.synchronize_native_service_worker_clients().await?;
                    Ok(BackendResponse::Unit)
                }
                (BackendOperation::Navigate, BackendRequest::Navigate(request)) => {
                    let _ = request;
                    Err(BrowserBackendError::Lifecycle {
                        operation: "navigate".into(),
                        state: "dispatch-bypass".into(),
                        reason: "native navigation must pass the frame-tree lifecycle gate".into(),
                    })
                }
                (BackendOperation::Contexts, BackendRequest::Contexts(_request)) => {
                    let context = engine.context().map_err(native_error)?;
                    Ok(BackendResponse::Contexts(vec![BrowsingContext {
                        context_id: context.context_id,
                        url: context.url,
                        active: context.active,
                    }]))
                }
                (BackendOperation::Evidence, BackendRequest::Evidence(request)) => {
                    require_context_id(&request.context_id, &active_context_id)?;
                    if matches!(
                        request.level,
                        EvidenceLevel::Screenshot | EvidenceLevel::Combined
                    ) {
                        return Err(BrowserBackendError::UnsupportedOperation {
                            operation: "evidence".into(),
                            reason: "native evidence does not carry image data; use the capture operation".into(),
                        });
                    }
                    let snapshot = engine.snapshot().map_err(native_error)?;
                    Ok(BackendResponse::Evidence(EvidenceResult {
                        context_id: active_context_id.clone(),
                        revision: snapshot.revision,
                        url: snapshot.url,
                        title: snapshot.title,
                        visible_text: snapshot.visible_text,
                        complete: matches!(request.level, EvidenceLevel::Compact)
                            && !snapshot.title_truncated
                            && !snapshot.text_truncated,
                    }))
                }
                (BackendOperation::Action, BackendRequest::Action(request)) => {
                    require_context_id(&request.context_id, &active_context_id)?;
                    let updates_focus = matches!(
                        &request.action,
                        SemanticAction::Click { .. }
                            | SemanticAction::ClickWithModifiers { .. }
                            | SemanticAction::DoubleClick { .. }
                            | SemanticAction::Drag { .. }
                            | SemanticAction::Type { .. }
                            | SemanticAction::Clear { .. }
                            | SemanticAction::Check { .. }
                            | SemanticAction::Uncheck { .. }
                            | SemanticAction::Select { .. }
                            | SemanticAction::KeyDown { .. }
                            | SemanticAction::KeyUp { .. }
                            | SemanticAction::Shortcut { .. }
                            | SemanticAction::KeyPress { .. }
                    );
                    let action = match request.action {
                        SemanticAction::Click { target } => NativeAction::Click { target },
                        SemanticAction::ClickWithModifiers { target, modifiers } => {
                            NativeAction::ClickWithModifiers {
                                target,
                                modifiers: modifiers.native_mask(),
                            }
                        }
                        SemanticAction::DoubleClick { target } => {
                            NativeAction::DoubleClick { target }
                        }
                        SemanticAction::Hover { target } => NativeAction::Hover { target },
                        SemanticAction::Drag {
                            source,
                            destination,
                        } => NativeAction::Drag {
                            source,
                            destination,
                        },
                        SemanticAction::Type { target, text } => {
                            NativeAction::Type { target, text }
                        }
                        SemanticAction::Clear { target } => NativeAction::Clear { target },
                        SemanticAction::Check { target } => NativeAction::Check { target },
                        SemanticAction::Uncheck { target } => NativeAction::Uncheck { target },
                        SemanticAction::Select { target, value } => {
                            NativeAction::Select { target, value }
                        }
                        SemanticAction::KeyDown { key } => NativeAction::KeyDown { key },
                        SemanticAction::KeyUp { key } => NativeAction::KeyUp { key },
                        SemanticAction::Shortcut { shortcut } => {
                            NativeAction::Shortcut { shortcut }
                        }
                        SemanticAction::KeyPress { key } => NativeAction::KeyPress { key },
                        SemanticAction::Scroll { delta_x, delta_y } => {
                            NativeAction::Scroll { delta_x, delta_y }
                        }
                    };
                    let previous_revision = engine.revision();
                    let outcome = engine.action_async(action).await.map_err(native_error)?;
                    let event_effects = engine
                        .effects_since(previous_revision)
                        .map_err(native_error)?
                        .effects;
                    let popup_requests = engine.take_pending_popups();
                    let post_messages = engine.take_pending_post_messages();
                    let window_closes = engine.take_pending_window_closes();
                    let window_navigations = engine.take_pending_window_navigations();
                    let service_worker_open_windows =
                        engine.take_pending_service_worker_open_windows();
                    let service_worker_client_messages =
                        engine.take_pending_service_worker_client_messages();
                    let page_message_port_commands =
                        take_native_worker_coordinator_effects(&mut engine);
                    let window_name = engine.config().window_name.clone();
                    drop(engine);
                    self.sync_target_name(&active_context_id, &window_name)?;
                    if updates_focus && let Some(frame_id) = selected_frame_id.as_deref() {
                        self.set_active_frame_focus(frame_id)?;
                    }
                    if let Some(frame_id) = selected_frame_id.as_deref() {
                        self.process_selected_frame_events(frame_id, event_effects)
                            .await?;
                    }
                    self.process_pending_browser_effects(
                        popup_requests,
                        post_messages,
                        window_closes,
                        window_navigations,
                        service_worker_open_windows,
                        service_worker_client_messages,
                        page_message_port_commands,
                    )
                    .await?;
                    self.synchronize_native_service_worker_clients().await?;
                    Ok(BackendResponse::Action(ActionResult {
                        context_id: active_context_id.clone(),
                        revision: outcome.revision,
                        accepted: outcome.accepted,
                    }))
                }
                (BackendOperation::Effects, BackendRequest::Effects(request)) => {
                    require_context_id(&request.context_id, &active_context_id)?;
                    let snapshot = engine
                        .effects_since(request.since_revision)
                        .map_err(native_error)?;
                    Ok(BackendResponse::Effects(EffectsResult {
                        context_id: active_context_id.clone(),
                        revision: snapshot.revision,
                        changed: snapshot.changed,
                    }))
                }
                (BackendOperation::Script, BackendRequest::Script(request)) => {
                    require_context_id(&request.context_id, &active_context_id)?;
                    let previous_revision = engine.revision();
                    let value = engine
                        .evaluate_async(request.source)
                        .await
                        .map_err(native_error)?;
                    let event_effects = engine
                        .effects_since(previous_revision)
                        .map_err(native_error)?
                        .effects;
                    let frame_scripts = engine.take_pending_frame_scripts();
                    let popup_requests = engine.take_pending_popups();
                    let post_messages = engine.take_pending_post_messages();
                    let window_closes = engine.take_pending_window_closes();
                    let window_navigations = engine.take_pending_window_navigations();
                    let service_worker_open_windows =
                        engine.take_pending_service_worker_open_windows();
                    let service_worker_client_messages =
                        engine.take_pending_service_worker_client_messages();
                    let page_message_port_commands =
                        take_native_worker_coordinator_effects(&mut engine);
                    let window_name = engine.config().window_name.clone();
                    drop(engine);
                    self.sync_target_name(&active_context_id, &window_name)?;
                    if let Some(frame_id) = selected_frame_id.as_deref() {
                        self.process_selected_frame_events(frame_id, event_effects)
                            .await?;
                    }
                    self.process_pending_frame_scripts(frame_scripts).await?;
                    self.process_pending_browser_effects(
                        popup_requests,
                        post_messages,
                        window_closes,
                        window_navigations,
                        service_worker_open_windows,
                        service_worker_client_messages,
                        page_message_port_commands,
                    )
                    .await?;
                    self.synchronize_native_service_worker_clients().await?;
                    Ok(BackendResponse::Script(ScriptResult { value }))
                }
                (BackendOperation::Capture, BackendRequest::Capture(request)) => {
                    require_context_id(&request.context_id, &active_context_id)?;
                    let format = request.format;
                    let bytes = match format {
                        CaptureFormat::Png | CaptureFormat::Jpeg => {
                            drop(engine);
                            let visual_format = match format {
                                CaptureFormat::Png => VisualFormat::Png,
                                CaptureFormat::Jpeg => VisualFormat::Jpeg,
                                CaptureFormat::Pdf => unreachable!("PDF handled separately"),
                            };
                            let quality = (format == CaptureFormat::Jpeg).then_some(80);
                            let capture = self
                                .capture_visual(&VisualCaptureOptions {
                                    format: visual_format,
                                    quality,
                                    ..VisualCaptureOptions::default()
                                })
                                .await?;
                            base64::engine::general_purpose::STANDARD
                                .decode(capture.data.as_bytes())
                                .map_err(|error| BrowserBackendError::InvalidConfiguration {
                                    field: "native capture payload".into(),
                                    reason: error.to_string(),
                                })?
                        }
                        CaptureFormat::Pdf => {
                            let snapshot = engine.snapshot().map_err(native_error)?;
                            drop(engine);
                            super::native_pdf::render_snapshot(
                                &snapshot,
                                &super::session::PdfOptions::default(),
                            )
                            .map_err(|error| {
                                BrowserBackendError::InvalidConfiguration {
                                    field: "native PDF".into(),
                                    reason: error.to_string(),
                                }
                            })?
                        }
                    };
                    Ok(BackendResponse::Capture(CaptureResult { format, bytes }))
                }
                (BackendOperation::Storage, BackendRequest::Storage(request)) => {
                    require_context_id(&request.context_id, &active_context_id)?;
                    let entries = engine
                        .storage_async(request.scope, request.operation)
                        .await
                        .map_err(native_error)?;
                    Ok(BackendResponse::Storage(StorageResult { entries }))
                }
                (BackendOperation::Prompt, BackendRequest::Prompt(request)) => {
                    require_context_id(&request.context_id, &active_context_id)?;
                    Ok(BackendResponse::Prompt(
                        engine
                            .resolve_dialog(request.decision)
                            .map_err(native_error)?,
                    ))
                }
                (BackendOperation::Download, BackendRequest::Download(request)) => {
                    require_context_id(&request.context_id, &active_context_id)?;
                    match request.operation {
                        DownloadOperation::List => Ok(BackendResponse::Download(DownloadResult {
                            download_ids: engine.download_ids().map_err(native_error)?,
                        })),
                        DownloadOperation::Cancel { download_id } => {
                            engine.cancel_download(&download_id).map_err(native_error)?;
                            Ok(BackendResponse::Download(DownloadResult {
                                download_ids: engine.download_ids().map_err(native_error)?,
                            }))
                        }
                    }
                }
                (operation, _) => Err(BrowserBackendError::UnsupportedOperation {
                    operation: operation_name(operation).into(),
                    reason: "native engine does not implement this operation".into(),
                }),
            }
        })
    }
}

fn project_native_target(
    engine: &NativeEngine,
    target_id: &str,
    opener_id: Option<String>,
    active: bool,
) -> Result<PageTargetInfo, BrowserBackendError> {
    let context = engine.context().map_err(native_error)?;
    let snapshot = engine.snapshot().map_err(native_error)?;
    Ok(PageTargetInfo {
        id: target_id.to_owned(),
        url: redact_diagnostic_url(&context.url),
        title: redact_diagnostic_text(&snapshot.title),
        opener_id,
        active,
    })
}

fn capture_native_frame_surface(
    engine: &NativeEngine,
    frames: &NativeFrameState,
    frame_id: &str,
) -> Result<NativeSurface, BrowserBackendError> {
    capture_native_frame_surface_with_viewport(engine, frames, frame_id, None)
}

fn full_page_capture_viewport(
    layout: &NativeLayoutSnapshot,
) -> Result<Viewport, BrowserBackendError> {
    let width = layout.viewport.width;
    let height = layout.content_height.max(layout.viewport.height);
    if width == 0
        || height == 0
        || width > NATIVE_CAPTURE_MAX_AXIS
        || height > NATIVE_CAPTURE_MAX_AXIS
    {
        return Err(BrowserBackendError::InvalidConfiguration {
            field: "native full-page capture".into(),
            reason: format!(
                "document dimensions must fit the {NATIVE_CAPTURE_MAX_AXIS}-pixel axis budget"
            ),
        });
    }
    let viewport = Viewport {
        width,
        height,
        device_scale_factor_milli: layout.viewport.device_scale_factor_milli,
    };
    viewport.validate().map_err(native_error)?;
    Ok(viewport)
}

fn native_capture_rect(
    clip: VisualClip,
) -> Result<super::native_engine::NativeRect, BrowserBackendError> {
    for value in [clip.x, clip.y, clip.width, clip.height] {
        if !value.is_finite() {
            return Err(BrowserBackendError::InvalidConfiguration {
                field: "native capture clip".into(),
                reason: "clip values must be finite".into(),
            });
        }
    }
    if clip.x < 0.0 || clip.y < 0.0 || clip.width <= 0.0 || clip.height <= 0.0 {
        return Err(BrowserBackendError::InvalidConfiguration {
            field: "native capture clip".into(),
            reason: "clip must have a non-negative origin and positive size".into(),
        });
    }
    let right = (clip.x + clip.width).ceil();
    let bottom = (clip.y + clip.height).ceil();
    let x = clip.x.floor();
    let y = clip.y.floor();
    if x > f64::from(NATIVE_CAPTURE_MAX_AXIS)
        || y > f64::from(NATIVE_CAPTURE_MAX_AXIS)
        || right > f64::from(NATIVE_CAPTURE_MAX_AXIS)
        || bottom > f64::from(NATIVE_CAPTURE_MAX_AXIS)
    {
        return Err(BrowserBackendError::InvalidConfiguration {
            field: "native capture clip".into(),
            reason: format!("clip must fit the {NATIVE_CAPTURE_MAX_AXIS}-pixel axis budget"),
        });
    }
    let x = x as u32;
    let y = y as u32;
    let right = right as u32;
    let bottom = bottom as u32;
    let width = right.saturating_sub(x);
    let height = bottom.saturating_sub(y);
    let pixels = usize::try_from(width)
        .ok()
        .and_then(|width| {
            usize::try_from(height)
                .ok()
                .and_then(|height| width.checked_mul(height))
        })
        .unwrap_or(usize::MAX);
    if pixels > NATIVE_CAPTURE_MAX_PIXELS {
        return Err(BrowserBackendError::InvalidConfiguration {
            field: "native capture clip".into(),
            reason: "clip exceeds the native 4-megapixel surface budget".into(),
        });
    }
    Ok(super::native_engine::NativeRect {
        x,
        y,
        width,
        height,
    })
}

fn capture_native_frame_surface_with_viewport(
    engine: &NativeEngine,
    frames: &NativeFrameState,
    frame_id: &str,
    root_viewport: Option<Viewport>,
) -> Result<NativeSurface, BrowserBackendError> {
    capture_native_frame_surface_at_depth(engine, frames, frame_id, 0, root_viewport)
}

fn capture_native_frame_surface_at_depth(
    engine: &NativeEngine,
    frames: &NativeFrameState,
    frame_id: &str,
    depth: usize,
    root_viewport: Option<Viewport>,
) -> Result<NativeSurface, BrowserBackendError> {
    if depth >= NATIVE_MAX_FRAMES {
        return Err(BrowserBackendError::SelectionFailed {
            reason: "native frame surface composition exceeded its bounded depth".into(),
        });
    }
    let (mut surface, layout) = match (depth, root_viewport) {
        (0, Some(viewport)) => (
            engine
                .rasterize_at_viewport(viewport)
                .map_err(native_error)?,
            engine
                .layout_at_viewport(viewport, NativePoint { x: 0, y: 0 })
                .map_err(native_error)?,
        ),
        _ => (
            engine.rasterize().map_err(native_error)?,
            engine.layout().map_err(native_error)?,
        ),
    };
    let mut children = frames
        .parked
        .iter()
        .filter(|(_, frame)| frame.parent_id.as_deref() == Some(frame_id))
        .filter_map(|(child_id, frame)| {
            let owner_node_index = frame.owner_node_index?;
            let (box_order, owner_node_id) = layout
                .boxes
                .iter()
                .enumerate()
                .find(|(_, layout_box)| layout_box.node_id.index() == owner_node_index)
                .map(|(order, layout_box)| (order, layout_box.node_id))?;
            let (destination, source_offset) = layout.viewport_projection_for(owner_node_id)?;
            Some((box_order, child_id.clone(), destination, source_offset))
        })
        .collect::<Vec<_>>();
    children.sort_by_key(|(box_order, child_id, _, _)| (*box_order, child_id.clone()));

    for (_, child_id, destination, source_offset) in children {
        let Some(child) = frames.parked.get(&child_id) else {
            continue;
        };
        let child_surface = capture_native_frame_surface_at_depth(
            &child.engine,
            frames,
            &child_id,
            depth.saturating_add(1),
            None,
        )?;
        surface
            .composite_child(&child_surface, destination, source_offset)
            .map_err(native_error)?;
    }
    Ok(surface)
}

fn find_native_point_frame(
    frames: &NativeFrameState,
    frame_id: &str,
    engine: &NativeEngine,
    x: i64,
    y: i64,
    depth: usize,
) -> Result<Option<(String, i64, i64)>, BrowserBackendError> {
    if depth >= NATIVE_MAX_FRAMES {
        return Err(BrowserBackendError::SelectionFailed {
            reason: "native point routing exceeded its bounded frame depth".into(),
        });
    }
    let layout = engine.layout().map_err(native_error)?;
    let hit = engine.hit_test(x, y).map_err(native_error)?;
    let point = NativePoint {
        x: u32::try_from(x).map_err(|_| BrowserBackendError::InvalidConfiguration {
            field: "point target".into(),
            reason: "point coordinates must be unsigned integers".into(),
        })?,
        y: u32::try_from(y).map_err(|_| BrowserBackendError::InvalidConfiguration {
            field: "point target".into(),
            reason: "point coordinates must be unsigned integers".into(),
        })?,
    };
    let mut candidates = Vec::new();
    for (child_id, frame) in &frames.parked {
        if frame.parent_id.as_deref() != Some(frame_id) {
            continue;
        }
        let Some(owner_node_index) = frame.owner_node_index else {
            continue;
        };
        for (order, layout_box) in layout.boxes.iter().enumerate() {
            if layout_box.node_id.index() != owner_node_index || !layout_box.pointer_events {
                continue;
            }
            let Some((destination, source_offset)) =
                layout.viewport_projection_for(layout_box.node_id)
            else {
                continue;
            };
            if destination.width == 0 || destination.height == 0 || !destination.contains(point) {
                continue;
            }
            candidates.push((
                layout_box.z_index,
                layout_box.depth,
                order,
                child_id.clone(),
                layout_box.node_id,
                destination,
                source_offset,
            ));
        }
    }
    candidates.sort_by_key(|(z_index, box_depth, order, child_id, _, _, _)| {
        (*z_index, *box_depth, *order, child_id.clone())
    });

    for (_, _, _, child_id, owner_node_id, destination, source_offset) in
        candidates.into_iter().rev()
    {
        let is_owner_hit = match hit {
            Some(hit_node) => engine
                .is_descendant_or_self(hit_node, owner_node_id)
                .map_err(native_error)?,
            None => false,
        };
        if !is_owner_hit {
            continue;
        }
        let local_x =
            i64::from(source_offset.x).saturating_add(x.saturating_sub(i64::from(destination.x)));
        let local_y =
            i64::from(source_offset.y).saturating_add(y.saturating_sub(i64::from(destination.y)));
        let child =
            frames
                .parked
                .get(&child_id)
                .ok_or_else(|| BrowserBackendError::SelectionFailed {
                    reason: "native point target child disappeared during hit testing".into(),
                })?;
        if let Some(target) = find_native_point_frame(
            frames,
            &child_id,
            &child.engine,
            local_x,
            local_y,
            depth.saturating_add(1),
        )? {
            return Ok(Some(target));
        }
        return Ok(Some((child_id, local_x, local_y)));
    }
    Ok(None)
}

fn native_window_name(name: &str) -> Option<String> {
    (!name.is_empty()).then(|| name.to_owned())
}

fn native_popup_name(target: &str) -> Option<String> {
    if target.is_empty()
        || matches!(
            target.to_ascii_lowercase().as_str(),
            "_blank" | "_self" | "_parent" | "_top" | "_unfencedtop"
        )
    {
        None
    } else {
        Some(target.to_owned())
    }
}

fn validate_native_popup_name(name: &str) -> Result<(), BrowserBackendError> {
    if name.is_empty() {
        return Err(BrowserBackendError::InvalidConfiguration {
            field: "popup target name".into(),
            reason: "must not be empty".into(),
        });
    }
    if name.len() > crate::browser_backend::MAX_BACKEND_ID_BYTES {
        return Err(BrowserBackendError::InvalidConfiguration {
            field: "popup target name".into(),
            reason: format!(
                "must be at most {} UTF-8 bytes",
                crate::browser_backend::MAX_BACKEND_ID_BYTES
            ),
        });
    }
    if !name.is_char_boundary(name.len()) || name.chars().any(char::is_control) {
        return Err(BrowserBackendError::InvalidConfiguration {
            field: "popup target name".into(),
            reason: "must be valid UTF-8 without control characters".into(),
        });
    }
    Ok(())
}

fn native_message_target_origin_matches(
    source_origin: &str,
    target_origin: &str,
    destination_origin: &str,
) -> Result<bool, BrowserBackendError> {
    if target_origin == "*" {
        return Ok(true);
    }
    if target_origin == "/" {
        return Ok(source_origin != "null" && source_origin == destination_origin);
    }
    let parsed =
        url::Url::parse(target_origin).map_err(|_| BrowserBackendError::InvalidConfiguration {
            field: "postMessage target origin".into(),
            reason: "must be *, /, or a valid HTTP(S) origin".into(),
        })?;
    if !matches!(parsed.scheme(), "http" | "https")
        || parsed.host_str().is_none()
        || parsed.path() != "/"
        || parsed.query().is_some()
        || parsed.fragment().is_some()
    {
        return Err(BrowserBackendError::InvalidConfiguration {
            field: "postMessage target origin".into(),
            reason: "must be *, /, or a serialized HTTP(S) origin".into(),
        });
    }
    let canonical = super::native_engine::NativeOrigin::from_url(&parsed)
        .map_err(native_error)?
        .serialized();
    Ok(canonical == destination_origin)
}

type NativeQueuedBrowserEffects = (
    Vec<NativePopupRequest>,
    Vec<NativePostMessageRequest>,
    Vec<NativeWindowCloseRequest>,
    Vec<NativeWindowNavigationRequest>,
    Vec<NativeServiceWorkerOpenWindowRequest>,
    Vec<NativeServiceWorkerClientMessage>,
    Vec<NativeWorkerCoordinatorEffect>,
);

fn take_shared_worker_page_messages(
    coordinator: &mut NativeSharedWorkerCoordinator,
) -> Vec<NativeWorkerCoordinatorEffect> {
    let mut effects = Vec::new();
    for message in coordinator.registry.take_message_port_messages() {
        let Some(route) = coordinator.page_ports.get(&message.bridge_key).cloned() else {
            continue;
        };
        if message.close {
            coordinator.page_ports.remove(&message.bridge_key);
        } else {
            for transfer in &message.transfer_ports {
                coordinator
                    .page_ports
                    .insert(transfer.bridge_key.clone(), route.clone());
            }
        }
        effects.push(NativeWorkerCoordinatorEffect::SharedWorkerMessage(
            NativePageMessagePortCommand {
                bridge_key: message.bridge_key,
                data: message.data,
                close: message.close,
                transfer_ports: message.transfer_ports,
                object_urls: message.object_urls,
                source_context_id: route.context_id,
                source_frame_id: route.frame_id,
            },
        ));
    }
    effects
}

struct NativeFrameRuntimeEffects {
    browser: NativeQueuedBrowserEffects,
    frame_scripts: Vec<NativeFrameScriptRequest>,
    events: Vec<NativeEffect>,
    window_name: String,
}

struct NativeFrameLifecycleEffects {
    frame_id: String,
    runtime: NativeFrameRuntimeEffects,
}

struct NativeFrameLifecycleResult {
    allowed: bool,
    cancelled: bool,
    navigation: Option<NativeNavigationRequest>,
    effects: Vec<NativeFrameLifecycleEffects>,
}

struct NativeFrameNavigationResult {
    runtime: NativeFrameRuntimeEffects,
    lifecycle: NativeFrameLifecycleResult,
    document_replaced: bool,
    outgoing_document_generation: u32,
    destroyed_descendant_frame_ids: Vec<String>,
    replacement_sandboxed_modals: bool,
}

#[derive(Clone)]
struct NativeFrameSelectionState {
    frame_id: String,
    parent_id: Option<String>,
    owner_node_index: Option<u32>,
    sandboxed_modals: bool,
    focused_frame_id: Option<String>,
    discovered_generation: Option<u32>,
}

fn parent_projected_event_kinds(command: &NativeScriptCommand) -> &'static [NativeEventKind] {
    match command {
        NativeScriptCommand::Focus { .. } => &[NativeEventKind::Focus],
        NativeScriptCommand::Blur { .. } => &[NativeEventKind::Blur],
        NativeScriptCommand::Click { .. } => &[NativeEventKind::Click],
        _ => &[],
    }
}

async fn close_native_frame_descendants(
    frames: &mut NativeFrameState,
    frame_id: &str,
) -> Result<(), BrowserBackendError> {
    let descendants = frames.descendant_ids(frame_id);
    let mut first_error = None;
    for descendant in descendants {
        let Some(mut frame) = frames.parked.remove(&descendant) else {
            continue;
        };
        frames.discovered_documents.remove(&descendant);
        frames
            .observed_frame_owner_removal_sequences
            .retain(|(observed_frame_id, _, _), _| observed_frame_id != &descendant);
        if frame.engine.lifecycle() == super::native_engine::NativeLifecycleState::Running
            && let Err(error) = frame.engine.close_async().await.map_err(native_error)
        {
            first_error.get_or_insert(error);
        }
    }
    frames.discovered_documents.remove(frame_id);
    frames
        .observed_frame_owner_removal_sequences
        .retain(|(observed_frame_id, _, _), _| observed_frame_id != frame_id);
    first_error.map_or(Ok(()), Err)
}

async fn activate_navigated_frame_if_ancestor(
    frames: &mut NativeFrameState,
    active_engine: &mut NativeEngine,
    frame_id: &str,
) -> Result<(), BrowserBackendError> {
    if !frames.is_descendant(&frames.active_frame_id, frame_id) {
        return close_native_frame_descendants(frames, frame_id).await;
    }
    let descendants = frames.descendant_ids(frame_id);
    let parked =
        frames
            .parked
            .remove(frame_id)
            .ok_or_else(|| BrowserBackendError::SelectionFailed {
                reason: "native navigated ancestor frame disappeared before activation".into(),
            })?;
    let NativeParkedFrame {
        engine: navigated_engine,
        parent_id,
        owner_node_index,
        sandboxed_modals,
    } = parked;
    let mut old_active_engine = std::mem::replace(active_engine, navigated_engine);
    let mut first_error = old_active_engine
        .close_async()
        .await
        .err()
        .map(native_error);
    for descendant in descendants {
        if let Some(mut frame) = frames.parked.remove(&descendant)
            && frame.engine.lifecycle() == super::native_engine::NativeLifecycleState::Running
            && let Err(error) = frame.engine.close_async().await.map_err(native_error)
        {
            first_error.get_or_insert(error);
        }
        frames.discovered_documents.remove(&descendant);
        frames
            .observed_frame_owner_removal_sequences
            .retain(|(observed_frame_id, _, _), _| observed_frame_id != &descendant);
    }
    frames.discovered_documents.remove(frame_id);
    frames
        .observed_frame_owner_removal_sequences
        .retain(|(observed_frame_id, _, _), _| observed_frame_id != frame_id);
    frames.active_frame_id = frame_id.to_owned();
    frames.active_parent_id = parent_id;
    frames.active_owner_node_index = owner_node_index;
    frames.active_sandboxed_modals = sandboxed_modals;
    frames.focused_frame_id = Some(frame_id.to_owned());
    frames.discovered_generation = None;
    first_error.map_or(Ok(()), Err)
}

fn take_native_browser_effects(engine: &mut NativeEngine) -> (NativeQueuedBrowserEffects, String) {
    let popups = engine.take_pending_popups();
    let messages = engine.take_pending_post_messages();
    let closes = engine.take_pending_window_closes();
    let navigations = engine.take_pending_window_navigations();
    let service_worker_open_windows = engine.take_pending_service_worker_open_windows();
    let service_worker_client_messages = engine.take_pending_service_worker_client_messages();
    let worker_effects = take_native_worker_coordinator_effects(engine);
    let window_name = engine.config().window_name.clone();
    (
        (
            popups,
            messages,
            closes,
            navigations,
            service_worker_open_windows,
            service_worker_client_messages,
            worker_effects,
        ),
        window_name,
    )
}

fn take_native_worker_coordinator_effects(
    engine: &mut NativeEngine,
) -> Vec<NativeWorkerCoordinatorEffect> {
    let mut effects = engine
        .take_pending_shared_worker_creates()
        .into_iter()
        .map(NativeWorkerCoordinatorEffect::SharedWorkerCreate)
        .collect::<Vec<_>>();
    effects.extend(
        engine
            .take_pending_page_message_port_commands()
            .into_iter()
            .map(NativeWorkerCoordinatorEffect::PageMessagePort),
    );
    effects
}

fn append_native_queued_browser_effects(
    target: &mut NativeQueuedBrowserEffects,
    mut source: NativeQueuedBrowserEffects,
) {
    target.0.append(&mut source.0);
    target.1.append(&mut source.1);
    target.2.append(&mut source.2);
    target.3.append(&mut source.3);
    target.4.append(&mut source.4);
    target.5.append(&mut source.5);
    target.6.append(&mut source.6);
}

fn take_native_frame_runtime_effects(
    engine: &mut NativeEngine,
    previous_revision: u64,
) -> Result<NativeFrameRuntimeEffects, BrowserBackendError> {
    let events = engine
        .effects_since(previous_revision)
        .map_err(native_error)?
        .effects;
    let frame_scripts = engine.take_pending_frame_scripts();
    let (browser, window_name) = take_native_browser_effects(engine);
    Ok(NativeFrameRuntimeEffects {
        browser,
        frame_scripts,
        events,
        window_name,
    })
}

async fn dispatch_frame_events_to_parent_engine(
    engine: &mut NativeEngine,
    frame_id: &str,
    effects: &[NativeEffect],
) -> Result<
    (
        Vec<NativeFrameScriptRequest>,
        NativeQueuedBrowserEffects,
        Vec<NativeEffect>,
    ),
    BrowserBackendError,
> {
    let previous_revision = engine.revision();
    engine
        .dispatch_frame_events_async(frame_id, effects)
        .await
        .map_err(native_error)?;
    let propagated = engine
        .effects_since(previous_revision)
        .map_err(native_error)?
        .effects;
    let nested = engine.take_pending_frame_scripts();
    let (queued, _) = take_native_browser_effects(engine);
    Ok((nested, queued, propagated))
}

async fn dispatch_post_message_to_native_frame(
    engine: &mut NativeEngine,
    message: &NativePostMessageRequest,
    proxy_updates: &[NativeWindowProxyUpdate],
) -> Result<NativeFrameRuntimeEffects, BrowserBackendError> {
    let previous_revision = engine.revision();
    engine
        .sync_window_proxies(proxy_updates)
        .await
        .map_err(native_error)?;
    let target_origin = engine.snapshot().map_err(native_error)?.origin.serialized();
    if !native_message_target_origin_matches(
        &message.source_origin,
        &message.target_origin,
        &target_origin,
    )? {
        return Ok(NativeFrameRuntimeEffects {
            browser: NativeQueuedBrowserEffects::default(),
            frame_scripts: Vec::new(),
            events: Vec::new(),
            window_name: engine.config().window_name.clone(),
        });
    }
    engine
        .dispatch_post_message(
            &message.source_context_id,
            &message.source_origin,
            &message.data,
            &message.transfer_ports,
            &message.object_urls,
        )
        .await
        .map_err(native_error)?;
    take_native_frame_runtime_effects(engine, previous_revision)
}

async fn dispatch_page_message_port_to_native_frame(
    engine: &mut NativeEngine,
    command: &NativePageMessagePortCommand,
) -> Result<NativeFrameRuntimeEffects, BrowserBackendError> {
    let previous_revision = engine.revision();
    if command.close {
        engine
            .dispatch_page_message_port_close(&command.bridge_key)
            .await
            .map_err(native_error)?;
    } else {
        engine
            .dispatch_page_message_port(
                &command.bridge_key,
                &command.data,
                &command.transfer_ports,
                &command.object_urls,
            )
            .await
            .map_err(native_error)?;
    }
    take_native_frame_runtime_effects(engine, previous_revision)
}

async fn dispatch_worker_messages_to_native_frame(
    engine: &mut NativeEngine,
    messages: &[NativeWorkerMessage],
) -> Result<NativeFrameRuntimeEffects, BrowserBackendError> {
    let previous_revision = engine.revision();
    engine
        .dispatch_worker_messages_async(messages)
        .await
        .map_err(native_error)?;
    take_native_frame_runtime_effects(engine, previous_revision)
}

async fn navigate_native_frame(
    engine: &mut NativeEngine,
    navigation: &NativeNavigationRequest,
    proxy_updates: &[NativeWindowProxyUpdate],
    frames: &mut NativeFrameState,
    frame_id: &str,
    sandboxed_modals: bool,
    replacement_sandboxed_modals: bool,
) -> Result<NativeFrameNavigationResult, BrowserBackendError> {
    engine
        .sync_window_proxies(proxy_updates)
        .await
        .map_err(native_error)?;
    let initial_generation = engine.document_generation().map_err(native_error)?;
    let descendant_frame_ids = frames.descendant_ids(frame_id);
    let lifecycle = if engine.navigation_requires_document_lifecycle(&navigation.url) {
        dispatch_native_frame_tree_lifecycle(engine, frames, frame_id, sandboxed_modals, None)
            .await?
    } else {
        NativeFrameLifecycleResult {
            allowed: true,
            cancelled: false,
            navigation: None,
            effects: Vec::new(),
        }
    };
    let mut runtime = NativeFrameRuntimeEffects {
        browser: NativeQueuedBrowserEffects::default(),
        frame_scripts: Vec::new(),
        events: Vec::new(),
        window_name: engine.config().window_name.clone(),
    };
    let mut document_replaced = false;
    if lifecycle.allowed {
        let previous_revision = engine.revision();
        let request = lifecycle
            .navigation
            .clone()
            .unwrap_or_else(|| navigation.clone());
        engine
            .navigate_request_async_after_lifecycle(request, 0, None)
            .await
            .map_err(native_error)?;
        runtime = take_native_frame_runtime_effects(engine, previous_revision)?;
        document_replaced =
            engine.document_generation().map_err(native_error)? != initial_generation;
    }
    Ok(NativeFrameNavigationResult {
        runtime,
        lifecycle,
        document_replaced,
        outgoing_document_generation: initial_generation,
        destroyed_descendant_frame_ids: if document_replaced {
            descendant_frame_ids
        } else {
            Vec::new()
        },
        replacement_sandboxed_modals,
    })
}

fn native_frame_sandboxed_modals_for_replacement(
    frames: &NativeFrameState,
    active_engine: &NativeEngine,
    frame_id: &str,
) -> Result<bool, BrowserBackendError> {
    let (parent_id, owner_node_index) =
        if frames.active_frame_id == frame_id {
            (
                frames.active_parent_id.as_deref(),
                frames.active_owner_node_index,
            )
        } else {
            let frame = frames.parked.get(frame_id).ok_or_else(|| {
                BrowserBackendError::SelectionFailed {
                    reason: "native frame disappeared while resolving replacement sandbox flags"
                        .into(),
                }
            })?;
            (frame.parent_id.as_deref(), frame.owner_node_index)
        };
    let Some(parent_id) = parent_id else {
        return Ok(false);
    };
    let owner_node_index =
        owner_node_index.ok_or_else(|| BrowserBackendError::SelectionFailed {
            reason: "native child frame is missing its owning iframe node".into(),
        })?;
    let parent_sandboxed_modals = if parent_id == frames.active_frame_id {
        frames.active_sandboxed_modals
    } else {
        frames
            .parked
            .get(parent_id)
            .map(|parent| parent.sandboxed_modals)
            .ok_or_else(|| BrowserBackendError::SelectionFailed {
                reason: "native frame parent disappeared while resolving sandbox flags".into(),
            })?
    };
    let parent_engine = native_frame_engine(frames, active_engine, parent_id).ok_or_else(|| {
        BrowserBackendError::SelectionFailed {
            reason: "native frame parent has no live document owner".into(),
        }
    })?;
    let sandbox = parent_engine
        .embedded_frame_sources()
        .map_err(native_error)?
        .into_iter()
        .find(|(node_index, _, _)| *node_index == owner_node_index)
        .map(|(_, _, sandbox)| sandbox)
        .ok_or_else(|| BrowserBackendError::SelectionFailed {
            reason: "native child frame owner is no longer present in its parent document".into(),
        })?;
    Ok(iframe_sandboxed_modals(
        parent_sandboxed_modals,
        sandbox.as_deref(),
    ))
}

fn activate_parked_frame_for_navigation(
    frames: &mut NativeFrameState,
    active_engine: &mut NativeEngine,
    frame_id: &str,
) -> Result<NativeFrameSelectionState, BrowserBackendError> {
    let target_generation = frames
        .parked
        .get(frame_id)
        .ok_or_else(|| BrowserBackendError::SelectionFailed {
            reason: "native ancestor frame disappeared before navigation".into(),
        })?
        .engine
        .document_generation()
        .map_err(native_error)?;
    let target =
        frames
            .parked
            .remove(frame_id)
            .ok_or_else(|| BrowserBackendError::SelectionFailed {
                reason: "native ancestor frame disappeared before navigation".into(),
            })?;
    let previous = NativeFrameSelectionState {
        frame_id: frames.active_frame_id.clone(),
        parent_id: frames.active_parent_id.clone(),
        owner_node_index: frames.active_owner_node_index,
        sandboxed_modals: frames.active_sandboxed_modals,
        focused_frame_id: frames.focused_frame_id.clone(),
        discovered_generation: frames.discovered_generation,
    };
    let old_active_engine = std::mem::replace(active_engine, target.engine);
    frames.parked.insert(
        previous.frame_id.clone(),
        NativeParkedFrame {
            engine: old_active_engine,
            parent_id: previous.parent_id.clone(),
            owner_node_index: previous.owner_node_index,
            sandboxed_modals: previous.sandboxed_modals,
        },
    );
    frames.active_frame_id = frame_id.to_owned();
    frames.active_parent_id = target.parent_id;
    frames.active_owner_node_index = target.owner_node_index;
    frames.active_sandboxed_modals = target.sandboxed_modals;
    frames.focused_frame_id = Some(frame_id.to_owned());
    frames.discovered_generation = Some(target_generation);
    Ok(previous)
}

fn restore_previous_frame_selection(
    frames: &mut NativeFrameState,
    active_engine: &mut NativeEngine,
    previous: NativeFrameSelectionState,
) -> Result<(), BrowserBackendError> {
    let current_frame_id = frames.active_frame_id.clone();
    let previous_active = frames.parked.remove(&previous.frame_id).ok_or_else(|| {
        BrowserBackendError::SelectionFailed {
            reason: "previously selected native frame disappeared during navigation".into(),
        }
    })?;
    let current_engine = std::mem::replace(active_engine, previous_active.engine);
    frames.parked.insert(
        current_frame_id,
        NativeParkedFrame {
            engine: current_engine,
            parent_id: frames.active_parent_id.clone(),
            owner_node_index: frames.active_owner_node_index,
            sandboxed_modals: frames.active_sandboxed_modals,
        },
    );
    frames.active_frame_id = previous.frame_id;
    frames.active_parent_id = previous.parent_id;
    frames.active_owner_node_index = previous.owner_node_index;
    frames.active_sandboxed_modals = previous.sandboxed_modals;
    frames.focused_frame_id = previous.focused_frame_id;
    frames.discovered_generation = previous.discovered_generation;
    Ok(())
}

fn native_frame_descendants_preorder(frames: &NativeFrameState, root_id: &str) -> Vec<String> {
    fn visit(frames: &NativeFrameState, parent_id: &str, ordered: &mut Vec<String>) {
        let mut children = frames
            .parked
            .iter()
            .filter(|(_, frame)| frame.parent_id.as_deref() == Some(parent_id))
            .map(|(id, frame)| (frame.owner_node_index.unwrap_or(u32::MAX), id.clone()))
            .collect::<Vec<_>>();
        children.sort();
        for (_, child_id) in children {
            ordered.push(child_id.clone());
            visit(frames, &child_id, ordered);
        }
    }

    let mut ordered = Vec::new();
    visit(frames, root_id, &mut ordered);
    ordered
}

fn native_frame_descendants_postorder(frames: &NativeFrameState, root_id: &str) -> Vec<String> {
    fn visit(frames: &NativeFrameState, parent_id: &str, ordered: &mut Vec<String>) {
        let mut children = frames
            .parked
            .iter()
            .filter(|(_, frame)| frame.parent_id.as_deref() == Some(parent_id))
            .map(|(id, frame)| (frame.owner_node_index.unwrap_or(u32::MAX), id.clone()))
            .collect::<Vec<_>>();
        children.sort();
        for (_, child_id) in children {
            visit(frames, &child_id, ordered);
            ordered.push(child_id);
        }
    }

    let mut ordered = Vec::new();
    visit(frames, root_id, &mut ordered);
    ordered
}

async fn dispatch_native_frame_tree_lifecycle(
    root_engine: &mut NativeEngine,
    frames: &mut NativeFrameState,
    root_frame_id: &str,
    root_sandboxed_modals: bool,
    cancellation: Option<&NativeNavigationCancellation>,
) -> Result<NativeFrameLifecycleResult, BrowserBackendError> {
    let descendants = native_frame_descendants_preorder(frames, root_frame_id);
    let mut prompt_shown = false;
    let mut allowed = true;
    let mut navigation = None;
    let mut effects = Vec::with_capacity(descendants.len().saturating_add(1) * 2);

    for frame_id in std::iter::once(root_frame_id.to_owned()).chain(descendants.iter().cloned()) {
        let sandboxed_modals = if frame_id == root_frame_id {
            root_sandboxed_modals
        } else {
            let frame = frames.parked.get(&frame_id).ok_or_else(|| {
                BrowserBackendError::SelectionFailed {
                    reason: "native descendant frame disappeared during beforeunload".into(),
                }
            })?;
            frame.sandboxed_modals
        };
        let previous_revision = if frame_id == root_frame_id {
            root_engine.revision()
        } else {
            frames
                .parked
                .get(&frame_id)
                .map(|frame| frame.engine.revision())
                .ok_or_else(|| BrowserBackendError::SelectionFailed {
                    reason: "native descendant frame disappeared during beforeunload".into(),
                })?
        };
        let (canceled, sticky_activation, next_navigation) = if frame_id == root_frame_id {
            root_engine
                .dispatch_before_unload_for_navigation()
                .await
                .map_err(native_error)?
        } else {
            frames
                .parked
                .get_mut(&frame_id)
                .ok_or_else(|| BrowserBackendError::SelectionFailed {
                    reason: "native descendant frame disappeared during beforeunload".into(),
                })?
                .engine
                .dispatch_before_unload_for_navigation()
                .await
                .map_err(native_error)?
        };
        let runtime = if frame_id == root_frame_id {
            take_native_frame_runtime_effects(root_engine, previous_revision)?
        } else {
            let frame = frames.parked.get_mut(&frame_id).ok_or_else(|| {
                BrowserBackendError::SelectionFailed {
                    reason: "native descendant frame disappeared after beforeunload".into(),
                }
            })?;
            take_native_frame_runtime_effects(&mut frame.engine, previous_revision)?
        };
        effects.push(NativeFrameLifecycleEffects {
            frame_id: frame_id.clone(),
            runtime,
        });
        if let Some(next_navigation) = next_navigation {
            if navigation.is_some() {
                return Err(native_error(NativeEngineError::TargetNotActionable {
                    reason: "multiple outgoing lifecycle navigations are not supported".into(),
                }));
            }
            navigation = Some(next_navigation);
        }
        if canceled && sticky_activation && !sandboxed_modals && !prompt_shown {
            prompt_shown = true;
            let confirmation = if frame_id == root_frame_id {
                wait_for_native_frame_beforeunload(root_engine, cancellation).await
            } else {
                let frame = frames.parked.get(&frame_id).ok_or_else(|| {
                    BrowserBackendError::SelectionFailed {
                        reason: "native descendant frame disappeared before its prompt".into(),
                    }
                })?;
                wait_for_native_frame_beforeunload(&frame.engine, cancellation).await
            };
            match confirmation {
                Ok(true) => {}
                Ok(false) | Err(NativeEngineError::NavigationCancelled) => allowed = false,
                Err(error) => return Err(native_error(error)),
            }
        }
    }

    let cancelled = cancellation.is_some_and(NativeNavigationCancellation::is_cancelled);
    if cancelled {
        allowed = false;
    }

    if allowed {
        let mut unload_order = native_frame_descendants_postorder(frames, root_frame_id);
        unload_order.push(root_frame_id.to_owned());
        for frame_id in unload_order {
            let previous_revision = if frame_id == root_frame_id {
                root_engine.revision()
            } else {
                frames
                    .parked
                    .get(&frame_id)
                    .map(|frame| frame.engine.revision())
                    .ok_or_else(|| BrowserBackendError::SelectionFailed {
                        reason: "native descendant frame disappeared during unload".into(),
                    })?
            };
            let next_navigation = if frame_id == root_frame_id {
                root_engine
                    .dispatch_unload_for_navigation()
                    .await
                    .map_err(native_error)?
            } else {
                frames
                    .parked
                    .get_mut(&frame_id)
                    .ok_or_else(|| BrowserBackendError::SelectionFailed {
                        reason: "native descendant frame disappeared during unload".into(),
                    })?
                    .engine
                    .dispatch_unload_for_navigation()
                    .await
                    .map_err(native_error)?
            };
            let runtime = if frame_id == root_frame_id {
                take_native_frame_runtime_effects(root_engine, previous_revision)?
            } else {
                let frame = frames.parked.get_mut(&frame_id).ok_or_else(|| {
                    BrowserBackendError::SelectionFailed {
                        reason: "native descendant frame disappeared after unload".into(),
                    }
                })?;
                take_native_frame_runtime_effects(&mut frame.engine, previous_revision)?
            };
            effects.push(NativeFrameLifecycleEffects {
                frame_id: frame_id.clone(),
                runtime,
            });
            if let Some(next_navigation) = next_navigation {
                if navigation.is_some() {
                    return Err(native_error(NativeEngineError::TargetNotActionable {
                        reason: "multiple outgoing lifecycle navigations are not supported".into(),
                    }));
                }
                navigation = Some(next_navigation);
            }
        }
    }

    Ok(NativeFrameLifecycleResult {
        allowed,
        cancelled,
        navigation,
        effects,
    })
}

async fn wait_for_native_frame_beforeunload(
    engine: &NativeEngine,
    cancellation: Option<&NativeNavigationCancellation>,
) -> Result<bool, NativeEngineError> {
    if let Some(cancellation) = cancellation {
        tokio::select! {
            result = engine.confirm_beforeunload_for_navigation() => result,
            _ = cancellation.cancelled() => Err(NativeEngineError::NavigationCancelled),
        }
    } else {
        engine.confirm_beforeunload_for_navigation().await
    }
}

fn native_navigation_request_from_parts(
    url: &str,
    method: NativeNavigationMethod,
    body: Option<NativeRequestBody>,
    body_content_type: Option<String>,
    replace_history: bool,
    operation: &str,
) -> Result<NativeNavigationRequest, BrowserBackendError> {
    validate_target_navigation_payload(
        method,
        body.as_ref(),
        body_content_type.as_deref(),
        operation,
    )
    .map_err(native_error)?;
    Ok(NativeNavigationRequest {
        method,
        url: url.to_owned(),
        body,
        body_content_type,
        replace_history,
        target: None,
        object_url: None,
    })
}

fn native_navigation_request_from_popup(
    request: &NativePopupRequest,
) -> Result<NativeNavigationRequest, BrowserBackendError> {
    native_navigation_request_from_parts(
        &request.url,
        request.method,
        request.body.clone(),
        request.body_content_type.clone(),
        false,
        "popup navigation payload",
    )
    .map(|mut navigation| {
        navigation.object_url = request.object_url.clone();
        navigation
    })
}

fn native_navigation_request_from_window(
    request: &NativeWindowNavigationRequest,
) -> Result<NativeNavigationRequest, BrowserBackendError> {
    native_navigation_request_from_parts(
        &request.href,
        request.method,
        request.body.clone(),
        request.body_content_type.clone(),
        request.replace,
        "window navigation payload",
    )
    .map(|mut navigation| {
        navigation.object_url = request.object_url.clone();
        navigation
    })
}

fn project_native_frame(
    engine: &NativeEngine,
    frame_id: &str,
    parent_id: Option<String>,
    active: bool,
) -> Result<FrameInfo, BrowserBackendError> {
    let context = engine.context().map_err(native_error)?;
    Ok(FrameInfo {
        id: frame_id.to_owned(),
        parent_id,
        url: redact_diagnostic_url(&context.url),
        active,
        out_of_process: false,
    })
}

fn native_frame_script_binding(
    frames: &NativeFrameState,
    active_engine: &NativeEngine,
    frame_id: &str,
    frame: &NativeParkedFrame,
    parent_origin: &NativeOrigin,
) -> Result<NativeFrameScriptBinding, BrowserBackendError> {
    native_frame_script_binding_from_engine(
        frames,
        active_engine,
        frame_id,
        &frame.engine,
        frame.owner_node_index,
        parent_origin,
    )
}

fn native_frame_script_binding_from_engine(
    frames: &NativeFrameState,
    active_engine: &NativeEngine,
    frame_id: &str,
    frame_engine: &NativeEngine,
    owner_node_index: Option<u32>,
    parent_origin: &NativeOrigin,
) -> Result<NativeFrameScriptBinding, BrowserBackendError> {
    let child_snapshot = frame_engine.snapshot().map_err(native_error)?;
    let generation = frame_engine.document_generation().map_err(native_error)?;
    let same_origin =
        *parent_origin != NativeOrigin::Opaque && *parent_origin == child_snapshot.origin;
    let children =
        native_frame_script_children(frames, active_engine, frame_id, &child_snapshot.origin)?;
    Ok(NativeFrameScriptBinding {
        node_index: owner_node_index.ok_or_else(|| BrowserBackendError::SelectionFailed {
            reason: "native frame owner node disappeared during script projection".into(),
        })?,
        frame_id: frame_id.to_owned(),
        url: child_snapshot.url,
        origin: child_snapshot.origin.serialized(),
        generation,
        revision: child_snapshot.revision,
        same_origin,
        document: frame_engine
            .script_document_snapshot()
            .map_err(native_error)?,
        children,
    })
}

fn native_frame_script_children(
    frames: &NativeFrameState,
    active_engine: &NativeEngine,
    parent_id: &str,
    parent_origin: &NativeOrigin,
) -> Result<Vec<NativeFrameScriptBinding>, BrowserBackendError> {
    let mut children = frames
        .parked
        .iter()
        .filter(|(_, frame)| frame.parent_id.as_deref() == Some(parent_id))
        .map(|(frame_id, frame)| {
            native_frame_script_binding(frames, active_engine, frame_id, frame, parent_origin)
        })
        .collect::<Result<Vec<_>, _>>()?;
    if frames.active_parent_id.as_deref() == Some(parent_id)
        && !frames.parked.contains_key(&frames.active_frame_id)
    {
        children.push(native_frame_script_binding_from_engine(
            frames,
            active_engine,
            &frames.active_frame_id,
            active_engine,
            frames.active_owner_node_index,
            parent_origin,
        )?);
    }
    children.sort_by_key(|binding| binding.node_index);
    Ok(children)
}

fn native_frame_engine<'a>(
    frames: &'a NativeFrameState,
    active_engine: &'a NativeEngine,
    frame_id: &str,
) -> Option<&'a NativeEngine> {
    if frames.active_frame_id == frame_id {
        Some(active_engine)
    } else {
        frames.parked.get(frame_id).map(|frame| &frame.engine)
    }
}

fn native_frame_script_window(
    frames: &NativeFrameState,
    active_engine: &NativeEngine,
    frame_id: &str,
    engine: &NativeEngine,
    same_origin: bool,
) -> Result<NativeFrameScriptWindow, BrowserBackendError> {
    let snapshot = engine.snapshot().map_err(native_error)?;
    let generation = engine.document_generation().map_err(native_error)?;
    Ok(NativeFrameScriptWindow {
        context_id: frame_id.to_owned(),
        url: snapshot.url,
        origin: snapshot.origin.serialized(),
        generation,
        revision: snapshot.revision,
        same_origin,
        document: engine.script_document_snapshot().map_err(native_error)?,
        children: native_frame_script_children(frames, active_engine, frame_id, &snapshot.origin)?,
    })
}

fn native_frame_viewport(
    parent_engine: &NativeEngine,
    owner_node_index: u32,
) -> Result<Option<Viewport>, BrowserBackendError> {
    let layout = parent_engine.layout().map_err(native_error)?;
    let Some(owner) = layout
        .boxes
        .iter()
        .find(|layout_box| layout_box.node_id.index() == owner_node_index)
    else {
        return Ok(None);
    };
    let width = owner
        .content_rect
        .width
        .clamp(1, MAX_NATIVE_VIEWPORT_DIMENSION);
    let height = owner
        .content_rect
        .height
        .clamp(1, MAX_NATIVE_VIEWPORT_DIMENSION);
    Ok(Some(Viewport {
        width,
        height,
        device_scale_factor_milli: parent_engine.config().viewport.device_scale_factor_milli,
    }))
}

fn iframe_sandboxed_modals(inherited: bool, sandbox: Option<&str>) -> bool {
    inherited
        || sandbox.is_some_and(|tokens| {
            !tokens
                .split_ascii_whitespace()
                .any(|token| token.eq_ignore_ascii_case("allow-modals"))
        })
}

async fn reconcile_native_frames(
    frames: &mut NativeFrameState,
    engine: &mut NativeEngine,
) -> Result<(), BrowserBackendError> {
    let generation = engine.document_generation().map_err(native_error)?;
    let revision = engine.document_revision().map_err(native_error)?;
    let active_frame_id = frames.active_frame_id.clone();
    let stamp = NativeFrameDiscoveryStamp {
        document_generation: generation,
        document_revision: revision,
    };
    let previous_stamp = frames.discovered_documents.get(&active_frame_id).copied();
    frames.observed_frame_owner_removal_sequences.retain(
        |(frame_id, document_generation, _), _| {
            frame_id != &active_frame_id || *document_generation == generation
        },
    );
    let mut removed_owner_indices = BTreeSet::new();
    for (owner_node_index, sequence) in engine
        .embedded_frame_owner_removal_sequences()
        .map_err(native_error)?
    {
        let key = (active_frame_id.clone(), generation, owner_node_index);
        let previous_sequence = frames
            .observed_frame_owner_removal_sequences
            .get(&key)
            .copied()
            .unwrap_or_default();
        if sequence > previous_sequence {
            removed_owner_indices.insert(owner_node_index);
            frames
                .observed_frame_owner_removal_sequences
                .insert(key, sequence);
        }
    }
    if previous_stamp == Some(stamp) && removed_owner_indices.is_empty() {
        return Ok(());
    }

    let sources = engine.embedded_frame_sources().map_err(native_error)?;
    let source_owner_indices = sources
        .iter()
        .map(|(node_index, _, _)| *node_index)
        .collect::<BTreeSet<_>>();
    let generation_changed =
        previous_stamp.is_none_or(|previous| previous.document_generation != generation);
    let mut retired_roots = frames
        .parked
        .iter()
        .filter(|(_, frame)| frame.parent_id.as_deref() == Some(active_frame_id.as_str()))
        .filter_map(|(frame_id, frame)| {
            let owner_node_index = frame.owner_node_index?;
            (generation_changed
                || !source_owner_indices.contains(&owner_node_index)
                || removed_owner_indices.contains(&owner_node_index))
            .then_some(frame_id.clone())
        })
        .collect::<Vec<_>>();
    retired_roots.sort();
    for frame_id in retired_roots {
        retire_native_frame_subtree(frames, &frame_id);
    }

    let target_id = engine.config().context_id.clone();
    let base_config = engine.config().clone();
    let parent_sandboxed_modals = frames.active_sandboxed_modals;
    let parent_service_worker_clients = engine.service_worker_clients();
    let (embedding_document_url, embedding_frame_sources) = engine.frame_navigation_policy();
    let mut children_to_create = Vec::new();
    for (node_index, source, sandbox) in sources {
        let already_attached = frames.parked.values().any(|frame| {
            frame.parent_id.as_deref() == Some(active_frame_id.as_str())
                && frame.owner_node_index == Some(node_index)
        });
        if !already_attached {
            children_to_create.push((node_index, source, sandbox));
        }
    }
    for (node_index, source, sandbox) in children_to_create {
        if frames.frame_count() >= NATIVE_MAX_FRAMES {
            return Err(BrowserBackendError::SelectionFailed {
                reason: format!("native frame limit reached ({NATIVE_MAX_FRAMES})"),
            });
        }
        let requested_frame_url = engine
            .resolve_embedded_frame_url(&source)
            .map_err(native_error)?;
        let frame_url = if engine
            .allows_embedded_frame_url(&requested_frame_url)
            .map_err(native_error)?
        {
            requested_frame_url
        } else {
            "about:blank".to_owned()
        };
        let frame_viewport = native_frame_viewport(engine, node_index)?;
        let sandboxed_modals = iframe_sandboxed_modals(parent_sandboxed_modals, sandbox.as_deref());
        let child_config = base_config.clone().with_initial_url(frame_url);
        let child_config = if let Some(viewport) = frame_viewport {
            child_config.with_viewport(viewport)
        } else {
            child_config
        };
        let frame_id = frames.next_frame_id(&target_id);
        let mut child = NativeEngine::new_with_browser_shared_workers(
            child_config,
            engine.dialog_control_plane(),
        )
        .map_err(native_error)?;
        child.set_frame_id(frame_id.clone());
        child.inherit_service_worker_clients(&parent_service_worker_clients);
        child.set_embedding_frame_policy(
            embedding_document_url.clone(),
            embedding_frame_sources.clone(),
        );
        if let Err(error) = child.initialize_async().await {
            let _ = child.close_async().await;
            return Err(native_error(error));
        }
        frames.parked.insert(
            frame_id,
            NativeParkedFrame {
                engine: child,
                parent_id: Some(active_frame_id.clone()),
                owner_node_index: Some(node_index),
                sandboxed_modals,
            },
        );
    }
    if frames
        .focused_frame_id
        .as_deref()
        .is_some_and(|focused| focused != active_frame_id && !frames.parked.contains_key(focused))
    {
        frames.focused_frame_id = Some(active_frame_id.clone());
    }
    frames.discovered_generation = Some(generation);
    frames.discovered_documents.insert(active_frame_id, stamp);
    Ok(())
}

fn retire_native_frame_subtree(frames: &mut NativeFrameState, root_frame_id: &str) {
    let mut frame_ids = frames.descendant_ids(root_frame_id);
    frame_ids.push(root_frame_id.to_owned());
    for frame_id in frame_ids {
        if let Some(frame) = frames.parked.remove(&frame_id) {
            frames.pending_retired.push(NativeRetiredFrame {
                frame_id: frame_id.clone(),
                engine: frame.engine,
            });
        }
        frames.discovered_documents.remove(&frame_id);
        frames
            .observed_frame_owner_removal_sequences
            .retain(|(observed_frame_id, _, _), _| observed_frame_id != &frame_id);
        if frames.focused_frame_id.as_deref() == Some(frame_id.as_str()) {
            frames.focused_frame_id = Some(frames.active_frame_id.clone());
        }
    }
}

async fn reconcile_native_frame_tree(
    frames: &mut NativeFrameState,
    active_engine: &mut NativeEngine,
) -> Result<(), BrowserBackendError> {
    let target_id = active_engine.config().context_id.clone();
    let root_frame_id = native_main_frame_id(&target_id);
    let original_active_frame_id = frames.active_frame_id.clone();
    let original_focused_frame_id = frames.focused_frame_id.clone();
    if root_frame_id != frames.active_frame_id && !frames.parked.contains_key(&root_frame_id) {
        return Err(BrowserBackendError::SelectionFailed {
            reason: "native frame tree has no live root document".into(),
        });
    }

    let mut pending_parents = VecDeque::from([root_frame_id]);
    let mut visited = BTreeSet::new();
    while let Some(parent_frame_id) = pending_parents.pop_front() {
        if !visited.insert(parent_frame_id.clone()) {
            continue;
        }
        if parent_frame_id != frames.active_frame_id {
            if !frames.parked.contains_key(&parent_frame_id) {
                continue;
            }
            activate_parked_frame_for_navigation(frames, active_engine, &parent_frame_id)?;
        }
        reconcile_native_frames(frames, active_engine).await?;
        let mut children = frames
            .parked
            .iter()
            .filter(|(_, frame)| frame.parent_id.as_deref() == Some(parent_frame_id.as_str()))
            .map(|(frame_id, frame)| (frame.owner_node_index.unwrap_or(u32::MAX), frame_id.clone()))
            .collect::<Vec<_>>();
        children.sort();
        pending_parents.extend(children.into_iter().map(|(_, frame_id)| frame_id));
    }

    if frames.active_frame_id != original_active_frame_id
        && frames.parked.contains_key(&original_active_frame_id)
    {
        activate_parked_frame_for_navigation(frames, active_engine, &original_active_frame_id)?;
    }
    frames.focused_frame_id = match original_focused_frame_id {
        Some(focused)
            if focused == frames.active_frame_id || frames.parked.contains_key(&focused) =>
        {
            Some(focused)
        }
        Some(_) => Some(frames.active_frame_id.clone()),
        None => None,
    };
    Ok(())
}

fn order_native_frames(frames: Vec<FrameInfo>) -> Vec<FrameInfo> {
    fn visit(id: &str, remaining: &mut BTreeMap<String, FrameInfo>, ordered: &mut Vec<FrameInfo>) {
        let Some(frame) = remaining.remove(id) else {
            return;
        };
        let child_ids = remaining
            .iter()
            .filter(|(_, child)| child.parent_id.as_deref() == Some(id))
            .map(|(child_id, _)| child_id.clone())
            .collect::<Vec<_>>();
        ordered.push(frame);
        for child_id in child_ids {
            visit(&child_id, remaining, ordered);
        }
    }

    let mut remaining = frames
        .into_iter()
        .map(|frame| (frame.id.clone(), frame))
        .collect::<BTreeMap<_, _>>();
    let root_ids = remaining
        .iter()
        .filter(|(_, frame)| frame.parent_id.is_none())
        .map(|(id, _)| id.clone())
        .collect::<Vec<_>>();
    let mut ordered = Vec::new();
    for root_id in root_ids {
        visit(&root_id, &mut remaining, &mut ordered);
    }
    let remaining_ids = remaining.keys().cloned().collect::<Vec<_>>();
    for frame_id in remaining_ids {
        visit(&frame_id, &mut remaining, &mut ordered);
    }
    ordered
}

async fn close_parked_frames(frames: &mut NativeFrameState) -> Result<(), BrowserBackendError> {
    let parked = std::mem::take(&mut frames.parked);
    let mut first_error = None;
    for (_, mut frame) in parked {
        if frame.engine.lifecycle() != super::native_engine::NativeLifecycleState::Running {
            continue;
        }
        if let Err(error) = frame.engine.close_async().await.map_err(native_error)
            && first_error.is_none()
        {
            first_error = Some(error);
        }
    }
    first_error.map_or(Ok(()), Err)
}

fn poisoned_lock_error(operation: BackendOperation, owner: &str) -> BrowserBackendError {
    BrowserBackendError::Lifecycle {
        operation: operation_name(operation).into(),
        state: "poisoned".into(),
        reason: format!("native {owner} lock is unavailable"),
    }
}

fn require_context_id(
    context_id: &str,
    active_context_id: &str,
) -> Result<(), BrowserBackendError> {
    if context_id == active_context_id {
        return Ok(());
    }
    Err(BrowserBackendError::InvalidConfiguration {
        field: "context id".into(),
        reason: "native engine accepts only its active context".into(),
    })
}

fn validate_native_topology_id(id: &str) -> Result<(), BrowserBackendError> {
    crate::browser::session::validate_topology_id(id).map_err(|error| {
        BrowserBackendError::SelectionFailed {
            reason: error.to_string(),
        }
    })
}

fn native_main_frame_id(context_id: &str) -> String {
    format!("{context_id}:main")
}

fn native_error(error: NativeEngineError) -> BrowserBackendError {
    match error {
        NativeEngineError::InvalidConfiguration { field, reason } => {
            BrowserBackendError::InvalidConfiguration { field, reason }
        }
        NativeEngineError::Lifecycle {
            operation,
            state,
            reason,
        } => BrowserBackendError::Lifecycle {
            operation,
            state: state.as_str().into(),
            reason,
        },
        NativeEngineError::UnsupportedUrl { reason } => BrowserBackendError::InvalidConfiguration {
            field: "navigation URL".into(),
            reason,
        },
        NativeEngineError::Network { operation, reason } => {
            BrowserBackendError::Connection { operation, reason }
        }
        NativeEngineError::Worker { operation, reason } => {
            BrowserBackendError::Connection { operation, reason }
        }
        NativeEngineError::WorkerFailure {
            operation,
            kind,
            reason,
        } => BrowserBackendError::Connection {
            operation,
            reason: format!("{kind:?}: {reason}"),
        },
        NativeEngineError::NavigationCancelled => BrowserBackendError::Lifecycle {
            operation: "navigate".into(),
            state: "cancelled".into(),
            reason: "navigation was stopped before its document committed".into(),
        },
        NativeEngineError::StorageProfileLocked { path } => BrowserBackendError::Connection {
            operation: "storage".into(),
            reason: format!("native Web Storage profile is already owned: {path}"),
        },
        NativeEngineError::Parse { offset, reason } => BrowserBackendError::InvalidConfiguration {
            field: "native HTML document".into(),
            reason: format!("parse failure at byte {offset}: {reason}"),
        },
        NativeEngineError::LimitExceeded {
            resource,
            limit,
            actual,
        } => BrowserBackendError::InvalidConfiguration {
            field: resource,
            reason: format!("value {actual} exceeds limit {limit}"),
        },
        NativeEngineError::Scheduler { reason } => BrowserBackendError::InvalidConfiguration {
            field: "native scheduler".into(),
            reason,
        },
        NativeEngineError::TargetNotFound => BrowserBackendError::UnsupportedOperation {
            operation: "action".into(),
            reason: "native action target was not found".into(),
        },
        NativeEngineError::AmbiguousTarget { matches } => {
            BrowserBackendError::UnsupportedOperation {
                operation: "action".into(),
                reason: format!(
                    "native action target matched {matches} elements; exactly one is required"
                ),
            }
        }
        NativeEngineError::DetachedTarget => BrowserBackendError::UnsupportedOperation {
            operation: "action".into(),
            reason: "native action target is stale or detached; observe again".into(),
        },
        NativeEngineError::TargetNotActionable { reason } => {
            BrowserBackendError::UnsupportedOperation {
                operation: "action".into(),
                reason: format!("native action target is not actionable: {reason}"),
            }
        }
        NativeEngineError::DisabledTarget => BrowserBackendError::UnsupportedOperation {
            operation: "action".into(),
            reason: "native action target is disabled".into(),
        },
        NativeEngineError::ReadOnlyTarget => BrowserBackendError::UnsupportedOperation {
            operation: "action".into(),
            reason: "native action target is read-only".into(),
        },
    }
}

fn operation_name(operation: BackendOperation) -> &'static str {
    match operation {
        BackendOperation::Initialize => "initialize",
        BackendOperation::Close => "close",
        BackendOperation::Navigate => "navigate",
        BackendOperation::Contexts => "contexts",
        BackendOperation::Evidence => "evidence",
        BackendOperation::Action => "action",
        BackendOperation::Effects => "effects",
        BackendOperation::Script => "script",
        BackendOperation::Capture => "capture",
        BackendOperation::Storage => "storage",
        BackendOperation::Prompt => "prompt",
        BackendOperation::Download => "download",
    }
}

#[cfg(test)]
mod tests {
    use super::{
        NativeBrowserEffectSource, NativeFrameState, NativeParkedFrame,
        activate_parked_frame_for_navigation, dispatch_native_frame_tree_lifecycle,
        iframe_sandboxed_modals, next_ready_native_browser_effect_source,
    };
    use crate::browser::native_engine::{
        NativeDialogControlPlane, NativeEngine, NativeEngineConfig, NativeNodeSubtreeTransfer,
    };

    async fn lifecycle_test_engine(
        config: NativeEngineConfig,
        url: &str,
        frame_id: &str,
    ) -> NativeEngine {
        let mut engine = NativeEngine::new_with_dialog_control(
            config.with_initial_url(url),
            NativeDialogControlPlane::default(),
        )
        .unwrap();
        engine.set_frame_id(frame_id.to_owned());
        engine.initialize_async().await.unwrap();
        engine
    }

    #[tokio::test]
    async fn native_document_adoption_preflight_failures_leave_both_owners_unchanged() {
        let source_frame_id = "native-context:main";
        let destination_frame_id = "native-context:frame-1";
        let config = NativeEngineConfig::default()
            .with_fixture(
                "fixture://adoption-transaction.test/source",
                "<main><p id='source'>source</p></main>",
            )
            .unwrap()
            .with_fixture(
                "fixture://adoption-transaction.test/destination",
                "<main><p id='destination'>destination</p></main>",
            )
            .unwrap();
        let mut source = lifecycle_test_engine(
            config.clone(),
            "fixture://adoption-transaction.test/source",
            source_frame_id,
        )
        .await;
        let mut destination = lifecycle_test_engine(
            config,
            "fixture://adoption-transaction.test/destination",
            destination_frame_id,
        )
        .await;
        let source_before = source.document_transfer_draft();
        let destination_before = destination.document_transfer_draft();
        let transfers = [NativeNodeSubtreeTransfer {
            source_index: 1,
            identities: Vec::new(),
        }];

        assert!(
            super::coordinate_native_document_adoption(
                &mut source,
                &mut destination,
                source_frame_id,
                destination_frame_id,
                source_before.generation().saturating_add(1),
                destination_before.generation(),
                &transfers,
            )
            .await
            .is_err()
        );
        assert_eq!(source.document_transfer_draft(), source_before);
        assert_eq!(destination.document_transfer_draft(), destination_before);

        assert!(
            super::coordinate_native_document_adoption(
                &mut source,
                &mut destination,
                source_frame_id,
                destination_frame_id,
                source_before.generation(),
                destination_before.generation(),
                &transfers,
            )
            .await
            .is_err()
        );
        assert_eq!(source.document_transfer_draft(), source_before);
        assert_eq!(destination.document_transfer_draft(), destination_before);

        source.close_async().await.unwrap();
        destination.close_async().await.unwrap();
    }

    #[test]
    fn iframe_sandboxed_modals_uses_ascii_tokens_and_preserves_inheritance() {
        assert!(!iframe_sandboxed_modals(false, None));
        assert!(iframe_sandboxed_modals(false, Some("allow-scripts")));
        assert!(!iframe_sandboxed_modals(
            false,
            Some("allow-scripts\tALLOW-MODALS")
        ));
        assert!(iframe_sandboxed_modals(true, Some("allow-modals")));
    }

    #[tokio::test]
    async fn native_frame_tree_lifecycle_checks_preorder_and_unloads_postorder() {
        let config = NativeEngineConfig::default()
            .with_fixture(
                "fixture://lifecycle-tree.test/root",
                "<script>globalThis.trace = []; addEventListener('beforeunload', event => { trace.push('before'); event.preventDefault(); event.returnValue = 'ignored'; }); addEventListener('pagehide', () => trace.push('pagehide')); addEventListener('unload', () => trace.push('unload'));</script><title>Root</title>",
            )
            .unwrap()
            .with_fixture(
                "fixture://lifecycle-tree.test/child",
                "<script>globalThis.trace = []; addEventListener('beforeunload', event => { trace.push('before'); event.preventDefault(); event.returnValue = 'ignored'; }); addEventListener('pagehide', () => trace.push('pagehide')); addEventListener('unload', () => trace.push('unload'));</script><title>Child</title>",
            )
            .unwrap()
            .with_fixture(
                "fixture://lifecycle-tree.test/grandchild",
                "<script>globalThis.trace = []; addEventListener('beforeunload', event => { trace.push('before'); event.preventDefault(); event.returnValue = 'ignored'; }); addEventListener('pagehide', () => trace.push('pagehide')); addEventListener('unload', () => trace.push('unload'));</script><title>Grandchild</title>",
            )
            .unwrap();
        let root_id = "native-context:main";
        let child_id = "native-context:frame-1";
        let grandchild_id = "native-context:frame-2";
        let mut root = lifecycle_test_engine(
            config.clone(),
            "fixture://lifecycle-tree.test/root",
            root_id,
        )
        .await;
        let child = lifecycle_test_engine(
            config.clone(),
            "fixture://lifecycle-tree.test/child",
            child_id,
        )
        .await;
        let grandchild = lifecycle_test_engine(
            config,
            "fixture://lifecycle-tree.test/grandchild",
            grandchild_id,
        )
        .await;
        let mut frames = NativeFrameState::new("native-context");
        frames.parked.insert(
            child_id.into(),
            NativeParkedFrame {
                engine: child,
                parent_id: Some(root_id.into()),
                owner_node_index: Some(1),
                sandboxed_modals: false,
            },
        );
        frames.parked.insert(
            grandchild_id.into(),
            NativeParkedFrame {
                engine: grandchild,
                parent_id: Some(child_id.into()),
                owner_node_index: Some(2),
                sandboxed_modals: false,
            },
        );

        let result =
            dispatch_native_frame_tree_lifecycle(&mut root, &mut frames, root_id, false, None)
                .await
                .unwrap();
        assert!(result.allowed);
        let observed = result
            .effects
            .iter()
            .map(|frame_effects| frame_effects.frame_id.as_str())
            .collect::<Vec<_>>();
        assert_eq!(
            observed,
            vec![
                root_id,
                child_id,
                grandchild_id,
                grandchild_id,
                child_id,
                root_id,
            ]
        );

        assert_eq!(
            root.evaluate_async("trace").await.unwrap(),
            serde_json::json!(["before", "pagehide", "unload"])
        );
        assert_eq!(
            frames
                .parked
                .get_mut(child_id)
                .unwrap()
                .engine
                .evaluate_async("trace")
                .await
                .unwrap(),
            serde_json::json!(["before", "pagehide", "unload"])
        );
        assert_eq!(
            frames
                .parked
                .get_mut(grandchild_id)
                .unwrap()
                .engine
                .evaluate_async("trace")
                .await
                .unwrap(),
            serde_json::json!(["before", "pagehide", "unload"])
        );

        root.close_async().await.unwrap();
        for (_, mut frame) in frames.parked {
            frame.engine.close_async().await.unwrap();
        }
    }

    #[tokio::test]
    async fn native_frame_lifecycle_includes_the_selected_descendant_of_parked_ancestor() {
        let config = NativeEngineConfig::default()
            .with_fixture(
                "fixture://selected-descendant.test/root",
                "<title>Root</title>",
            )
            .unwrap()
            .with_fixture(
                "fixture://selected-descendant.test/child",
                "<script>globalThis.trace = []; addEventListener('beforeunload', event => { trace.push('before'); event.preventDefault(); event.returnValue = 'ignored'; }); addEventListener('pagehide', () => trace.push('pagehide')); addEventListener('unload', () => trace.push('unload'));</script><title>Child</title>",
            )
            .unwrap();
        let root_id = "native-context:main";
        let child_id = "native-context:frame-1";
        let root = lifecycle_test_engine(
            config.clone(),
            "fixture://selected-descendant.test/root",
            root_id,
        )
        .await;
        let child =
            lifecycle_test_engine(config, "fixture://selected-descendant.test/child", child_id)
                .await;
        let mut frames = NativeFrameState::new("native-context");
        frames.parked.insert(
            root_id.into(),
            NativeParkedFrame {
                engine: root,
                parent_id: None,
                owner_node_index: None,
                sandboxed_modals: false,
            },
        );
        frames.active_frame_id = child_id.into();
        frames.active_parent_id = Some(root_id.into());
        frames.active_owner_node_index = Some(1);
        let mut active_engine = child;
        activate_parked_frame_for_navigation(&mut frames, &mut active_engine, root_id).unwrap();

        let root_sandboxed_modals = frames.active_sandboxed_modals;
        let result = dispatch_native_frame_tree_lifecycle(
            &mut active_engine,
            &mut frames,
            root_id,
            root_sandboxed_modals,
            None,
        )
        .await
        .unwrap();
        assert!(result.allowed);
        assert_eq!(
            frames
                .parked
                .get_mut(child_id)
                .unwrap()
                .engine
                .evaluate_async("trace")
                .await
                .unwrap(),
            serde_json::json!(["before", "pagehide", "unload"])
        );
        assert_eq!(frames.active_frame_id, root_id);
        assert!(frames.parked.contains_key(child_id));
        let mut parked_child = frames.parked.remove(child_id).unwrap();
        parked_child.engine.close_async().await.unwrap();
        active_engine.close_async().await.unwrap();
    }

    #[test]
    fn native_browser_effect_sources_round_robin_without_starvation() {
        let mut cursor = NativeBrowserEffectSource::Popup;
        let mut all_ready = || {
            next_ready_native_browser_effect_source(
                &mut cursor,
                true,
                true,
                true,
                true,
                true,
                true,
                true,
            )
        };

        assert_eq!(all_ready(), Some(NativeBrowserEffectSource::Popup));
        assert_eq!(all_ready(), Some(NativeBrowserEffectSource::Message));
        assert_eq!(all_ready(), Some(NativeBrowserEffectSource::Navigation));
        assert_eq!(all_ready(), Some(NativeBrowserEffectSource::Close));
        assert_eq!(
            all_ready(),
            Some(NativeBrowserEffectSource::ServiceWorkerOpenWindow)
        );
        assert_eq!(
            all_ready(),
            Some(NativeBrowserEffectSource::ServiceWorkerClientMessage)
        );
        assert_eq!(
            all_ready(),
            Some(NativeBrowserEffectSource::PageMessagePort)
        );
        assert_eq!(all_ready(), Some(NativeBrowserEffectSource::Popup));
    }

    #[test]
    fn native_browser_effect_sources_skip_empty_queues_and_resume_rotation() {
        let mut cursor = NativeBrowserEffectSource::Popup;

        assert_eq!(
            next_ready_native_browser_effect_source(
                &mut cursor,
                false,
                true,
                false,
                true,
                false,
                false,
                false
            ),
            Some(NativeBrowserEffectSource::Message)
        );
        assert_eq!(
            next_ready_native_browser_effect_source(
                &mut cursor,
                false,
                false,
                false,
                true,
                false,
                false,
                false
            ),
            Some(NativeBrowserEffectSource::Close)
        );
        assert_eq!(
            next_ready_native_browser_effect_source(
                &mut cursor,
                true,
                false,
                false,
                false,
                false,
                false,
                false
            ),
            Some(NativeBrowserEffectSource::Popup)
        );
        assert_eq!(
            next_ready_native_browser_effect_source(
                &mut cursor,
                false,
                false,
                false,
                false,
                false,
                false,
                false,
            ),
            None
        );
    }
}
