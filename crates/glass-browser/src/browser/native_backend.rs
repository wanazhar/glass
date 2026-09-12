//! Semantic backend adapter for the Glass-owned native engine.
//!
//! The adapter is intentionally small: the native engine owns DOM and
//! lifecycle state, while this module translates only the stable
//! `browser_backend` contract.

use super::native_engine::{
    MAX_NATIVE_EFFECTS, MAX_NATIVE_VIEWPORT_DIMENSION, NativeAction, NativeEffect, NativeEngine,
    NativeEngineConfig, NativeEngineError, NativeEventKind, NativeFrameScriptBinding,
    NativeFrameScriptContext, NativeFrameScriptRequest, NativeFrameScriptWindow,
    NativeHistoryDirection, NativeInspectionSnapshot, NativeOrigin, NativePoint,
    NativePopupRequest, NativePostMessageRequest, NativePreflightAction, NativeScriptCommand,
    NativeSurface, NativeTargetPreflight, NativeWindowCloseRequest, NativeWindowNavigationRequest,
    NativeWindowProxyUpdate, Viewport, parse_point_target,
};
use crate::browser::session::{
    FrameInfo, NavigationControlOutcome, PageTargetInfo, redact_diagnostic_text,
    redact_diagnostic_url,
};
use crate::browser_backend::{
    ActionResult, BROWSER_BACKEND_SCHEMA_VERSION, BackendFuture, BackendOperation, BackendProfile,
    BackendRequest, BackendResponse, BrowserBackend, BrowserBackendError, BrowserCapability,
    BrowsingContext, CapabilityDescriptor, CaptureFormat, CaptureResult, CertificationLevel,
    CertificationProfile, DownloadOperation, DownloadResult, EffectsResult, EvidenceLevel,
    EvidenceResult, NavigationResult, Portability, PromptDecision, PromptResult, ScriptResult,
    SemanticAction, StorageResult, StorageScope, SupportLevel,
};
use std::collections::{BTreeMap, VecDeque};
use std::sync::{Mutex, MutexGuard};

/// Stable backend ID for the Glass-owned native engine.
pub const NATIVE_ENGINE_BACKEND_ID: &str = "native-engine";
const NATIVE_ENGINE_BACKEND_VERSION: &str = "0.1";
const NATIVE_ENGINE_BROWSER_FAMILY: &str = "native";

const NATIVE_MAX_TARGETS: usize = crate::browser::session::TOPOLOGY_MAX_TARGETS;
const NATIVE_MAX_FRAMES: usize = crate::browser::session::TOPOLOGY_MAX_FRAMES;
const NATIVE_MAX_CLOSED_TARGETS: usize = NATIVE_MAX_TARGETS * 4;

struct NativeParkedFrame {
    engine: NativeEngine,
    parent_id: Option<String>,
    owner_node_index: Option<u32>,
}

enum NativeFrameRoute {
    ActiveSelected,
    ActiveParked,
    ParkedSelected { target_id: String },
    ParkedParked { target_id: String },
}

struct NativeFrameState {
    active_frame_id: String,
    active_parent_id: Option<String>,
    active_owner_node_index: Option<u32>,
    focused_frame_id: Option<String>,
    parked: BTreeMap<String, NativeParkedFrame>,
    next_frame_number: u64,
    discovered_generation: Option<u32>,
}

impl NativeFrameState {
    fn new(target_id: &str) -> Self {
        Self {
            active_frame_id: native_main_frame_id(target_id),
            active_parent_id: None,
            active_owner_node_index: None,
            focused_frame_id: None,
            parked: BTreeMap::new(),
            next_frame_number: 1,
            discovered_generation: None,
        }
    }

    fn empty() -> Self {
        Self {
            active_frame_id: String::new(),
            active_parent_id: None,
            active_owner_node_index: None,
            focused_frame_id: None,
            parked: BTreeMap::new(),
            next_frame_number: 1,
            discovered_generation: None,
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
    targets: Mutex<NativeTargetState>,
}

impl NativeEngineBackend {
    pub fn new(config: NativeEngineConfig) -> Result<Self, BrowserBackendError> {
        let profile = Self::profile_for(env!("CARGO_PKG_VERSION"))?;
        let active_target_id = config.context_id.clone();
        let engine = NativeEngine::new(config).map_err(native_error)?;
        let active_name = native_window_name(&engine.config().window_name);
        Ok(Self {
            profile,
            engine: Mutex::new(engine),
            targets: Mutex::new(NativeTargetState::new(active_target_id, active_name)),
        })
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
        let mut targets = self.lock_targets(BackendOperation::Capture)?;
        let engine = self.lock_engine_raw(BackendOperation::Capture)?;
        let frame_id = targets.active_frames.active_frame_id.clone();
        reconcile_native_frames(&mut targets.active_frames, &engine).await?;
        capture_native_frame_surface(&engine, &targets.active_frames, &frame_id)
            .and_then(|surface| surface.to_png().map_err(native_error))
    }

    /// Route a point click through the currently selected frame tree when the
    /// point lands on a live embedded browsing context. A `None` result means
    /// that the point belongs to the selected document and should continue
    /// through the ordinary action path.
    async fn dispatch_point_click(
        &self,
        context_id: &str,
        target: &str,
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
                let engine = self.lock_engine_raw(BackendOperation::Action)?;
                let root_frame_id = targets.active_frames.active_frame_id.clone();
                reconcile_native_frames(&mut targets.active_frames, &engine).await?;
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
        let action = NativeAction::Click {
            target: format!("point={local_x},{local_y}"),
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
        )
        .await?;
        Ok(Some(BackendResponse::Action(ActionResult {
            context_id: context_id.to_owned(),
            revision,
            accepted,
        })))
    }

    async fn apply_action_to_native_frame(
        &self,
        route: NativeFrameRoute,
        frame_id: &str,
        action: NativeAction,
        proxy_updates: &[NativeWindowProxyUpdate],
    ) -> Result<(u64, bool, NativeFrameRuntimeEffects, String), BrowserBackendError> {
        match route {
            NativeFrameRoute::ActiveSelected => {
                let mut engine = self.lock_engine_raw(BackendOperation::Action)?;
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
                let engine = self.lock_engine_raw(BackendOperation::Action)?;
                let root_frame_id = targets.active_frames.active_frame_id.clone();
                reconcile_native_frames(&mut targets.active_frames, &engine).await?;
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
    pub fn preflight_target(
        &self,
        target: &str,
        action: NativePreflightAction,
    ) -> Result<NativeTargetPreflight, BrowserBackendError> {
        self.lock_engine(BackendOperation::Evidence)?
            .preflight_target(target, action)
            .map_err(native_error)
    }

    /// Return one atomic page/semantic/layout snapshot for agent discovery.
    pub fn inspection_snapshot(&self) -> Result<NativeInspectionSnapshot, BrowserBackendError> {
        self.lock_engine(BackendOperation::Evidence)?
            .inspection_snapshot()
            .map_err(native_error)
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
        let mut targets = self.lock_targets(BackendOperation::Contexts)?;
        if targets.active_target_id.is_none() {
            return Err(BrowserBackendError::Lifecycle {
                operation: "contexts".into(),
                state: "no-target-selected".into(),
                reason: "select an available native page target before frame discovery".into(),
            });
        }
        let engine = self.lock_engine_raw(BackendOperation::Contexts)?;
        reconcile_native_frames(&mut targets.active_frames, &engine).await?;
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
        let engine = self.lock_engine_raw(BackendOperation::Script)?;
        reconcile_native_frames(&mut targets.active_frames, &engine).await?;
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
        let engine = self.lock_engine_raw(BackendOperation::Script)?;
        reconcile_native_frames(&mut targets.active_frames, &engine).await?;
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
            let (nested, effects, event_effects) =
                self.apply_frame_script_to_frame(route, request).await?;
            self.process_frame_event_effects(&source_frame_id, event_effects)
                .await?;
            Box::pin(
                self.process_pending_browser_effects(effects.0, effects.1, effects.2, effects.3),
            )
            .await?;
            pending.extend(nested);
        }
        Ok(())
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
        let empty = || (Vec::new(), Vec::new(), Vec::new(), Vec::new());
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
            Box::pin(self.process_pending_browser_effects(queued.0, queued.1, queued.2, queued.3))
                .await?;
        }
        Ok(())
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
        self.create_target_named(url, None, None)
            .await
            .map(|(target, _, _, _, _)| target)
    }

    async fn create_target_named(
        &self,
        url: &str,
        name: Option<String>,
        opener_id: Option<String>,
    ) -> Result<
        (
            PageTargetInfo,
            Vec<NativePopupRequest>,
            Vec<NativePostMessageRequest>,
            Vec<NativeWindowCloseRequest>,
            Vec<NativeWindowNavigationRequest>,
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
        let config = base_config
            .with_context_id(target_id.clone())
            .with_opener_window_name(opener_window_name)
            .with_opener_url(opener_url)
            .with_window_name(initial_window_name)
            .with_initial_url(url.to_owned());
        let config = if let Some(opener_id) = opener_id.as_deref() {
            config.with_opener_context_id(opener_id.to_owned())
        } else {
            config
        };
        let mut engine = NativeEngine::new(config).map_err(native_error)?;
        if let Err(error) = engine.initialize_async().await {
            let _ = engine.close_async().await;
            return Err(native_error(error));
        }
        let nested = engine.take_pending_popups();
        let nested_messages = engine.take_pending_post_messages();
        let nested_window_closes = engine.take_pending_window_closes();
        let nested_window_navigations = engine.take_pending_window_navigations();
        let target_name = native_window_name(&engine.config().window_name);
        let target = match project_native_target(&engine, &target_id, opener_id.clone(), false) {
            Ok(target) => target,
            Err(error) => {
                let _ = engine.close_async().await;
                return Err(error);
            }
        };
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
        ))
    }

    /// Click a native target and require the action to create exactly one
    /// causally-owned page target. The returned popup remains parked and the
    /// opener remains selected, matching the shared popup contract.
    pub async fn click_expect_popup(
        &self,
        target: &str,
    ) -> Result<(ActionResult, PageTargetInfo), BrowserBackendError> {
        let proxy_updates = self.active_window_proxy_updates()?;
        let mut engine = self.lock_engine(BackendOperation::Action)?;
        engine
            .sync_window_proxies(&proxy_updates)
            .await
            .map_err(native_error)?;
        let active_context_id = engine.config().context_id.clone();
        let outcome = engine
            .action_async(NativeAction::Click {
                target: target.to_owned(),
            })
            .await
            .map_err(native_error)?;
        let popup_requests = engine.take_pending_popups();
        let post_messages = engine.take_pending_post_messages();
        let window_closes = engine.take_pending_window_closes();
        let window_navigations = engine.take_pending_window_navigations();
        let window_name = engine.config().window_name.clone();
        drop(engine);
        self.sync_target_name(&active_context_id, &window_name)?;

        if popup_requests.len() != 1 {
            return Err(BrowserBackendError::UnsupportedOperation {
                operation: "clickExpectPopup".into(),
                reason: if popup_requests.is_empty() {
                    "click did not create a new native page target".into()
                } else {
                    format!(
                        "click created {} native page targets; exactly one is required",
                        popup_requests.len()
                    )
                },
            });
        }
        let mut created = self
            .process_pending_browser_effects(
                popup_requests,
                post_messages,
                window_closes,
                window_navigations,
            )
            .await?;
        let popup = created
            .pop()
            .expect("popup URL count was validated before materialization");
        Ok((
            ActionResult {
                context_id: active_context_id,
                revision: outcome.revision,
                accepted: outcome.accepted,
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
    ) -> Result<Vec<PageTargetInfo>, BrowserBackendError> {
        let mut pending_popups = VecDeque::from(popup_requests);
        let mut pending_messages = VecDeque::from(post_messages);
        let mut pending_window_closes = VecDeque::from(window_closes);
        let mut pending_window_navigations = VecDeque::from(window_navigations);
        let mut created: Vec<PageTargetInfo> = Vec::new();
        let mut processed = 0usize;
        while !pending_popups.is_empty()
            || !pending_messages.is_empty()
            || !pending_window_closes.is_empty()
            || !pending_window_navigations.is_empty()
        {
            processed = processed.saturating_add(1);
            if processed > NATIVE_MAX_TARGETS.saturating_mul(8) {
                return Err(BrowserBackendError::SelectionFailed {
                    reason: "native browser-effect cascade exceeded its bounded limit".into(),
                });
            }
            let Some(request) = pending_popups.pop_front() else {
                let Some(message) = pending_messages.pop_front() else {
                    let Some(navigation) = pending_window_navigations.pop_front() else {
                        let request = pending_window_closes
                            .pop_front()
                            .expect("window close queue is non-empty when other queues are empty");
                        if let Some((target_id, _)) = self.window_close_target(&request)? {
                            self.close_target(&target_id).await?;
                            created.retain(|target| target.id != target_id);
                        }
                        continue;
                    };
                    if let Some(frame_id) = navigation.target_context_id.as_deref()
                        && let Some(route) = self.frame_route(frame_id)?
                    {
                        let (
                            nested_popups,
                            nested_messages,
                            nested_window_closes,
                            nested_window_navigations,
                        ) = self
                            .navigate_frame_target(
                                route,
                                frame_id,
                                &navigation.source_context_id,
                                &navigation.href,
                                navigation.replace,
                            )
                            .await?;
                        pending_popups.extend(nested_popups);
                        pending_messages.extend(nested_messages);
                        pending_window_closes.extend(nested_window_closes);
                        pending_window_navigations.extend(nested_window_navigations);
                        continue;
                    }
                    let Some((target_id, active)) = self.window_navigation_target(&navigation)?
                    else {
                        continue;
                    };
                    let (_, nested, nested_messages, nested_closes, nested_navigations) = self
                        .navigate_named_target(
                            &target_id,
                            active,
                            &navigation.href,
                            navigation.replace,
                        )
                        .await?;
                    pending_popups.extend(nested);
                    pending_messages.extend(nested_messages);
                    pending_window_closes.extend(nested_closes);
                    pending_window_navigations.extend(nested_navigations);
                    continue;
                };
                let (
                    nested_popups,
                    nested_messages,
                    nested_window_closes,
                    nested_window_navigations,
                ) = self.deliver_post_message(message).await?;
                pending_popups.extend(nested_popups);
                pending_messages.extend(nested_messages);
                pending_window_closes.extend(nested_window_closes);
                pending_window_navigations.extend(nested_window_navigations);
                continue;
            };
            let name = native_popup_name(&request.target);
            if let Some(name) = name.as_deref()
                && let Some((target_id, active)) = self.target_named(name)?
            {
                let (_, nested, nested_messages, nested_window_closes, nested_window_navigations) =
                    self.navigate_named_target(&target_id, active, &request.url, false)
                        .await?;
                self.bind_window_handle(&request, &target_id)?;
                pending_popups.extend(nested);
                pending_messages.extend(nested_messages);
                pending_window_closes.extend(nested_window_closes);
                pending_window_navigations.extend(nested_window_navigations);
                continue;
            }
            match self
                .create_target_named(&request.url, name, Some(request.source_context_id.clone()))
                .await
            {
                Ok((
                    target,
                    nested,
                    nested_messages,
                    nested_window_closes,
                    nested_window_navigations,
                )) => {
                    self.bind_window_handle(&request, &target.id)?;
                    created.push(target);
                    pending_popups.extend(nested);
                    pending_messages.extend(nested_messages);
                    pending_window_closes.extend(nested_window_closes);
                    pending_window_navigations.extend(nested_window_navigations);
                }
                Err(error) => {
                    for target in created {
                        let _ = self.close_target(&target.id).await;
                    }
                    return Err(error);
                }
            }
        }
        Ok(created)
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

    async fn deliver_post_message(
        &self,
        message: NativePostMessageRequest,
    ) -> Result<
        (
            Vec<NativePopupRequest>,
            Vec<NativePostMessageRequest>,
            Vec<NativeWindowCloseRequest>,
            Vec<NativeWindowNavigationRequest>,
        ),
        BrowserBackendError,
    > {
        if let Some(frame_id) = message.target_context_id.clone()
            && let Some(route) = self.frame_route(&frame_id)?
        {
            return self
                .deliver_post_message_to_frame(route, &frame_id, message)
                .await;
        }
        let Some((target_id, active)) = self.message_target(&message)? else {
            return Ok((Vec::new(), Vec::new(), Vec::new(), Vec::new()));
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
                return Ok((Vec::new(), Vec::new(), Vec::new(), Vec::new()));
            }
            engine
                .dispatch_post_message(
                    &message.source_context_id,
                    &message.source_origin,
                    &message.data,
                )
                .await
                .map_err(native_error)?;
            let popup_requests = engine.take_pending_popups();
            let post_messages = engine.take_pending_post_messages();
            let window_closes = engine.take_pending_window_closes();
            let window_navigations = engine.take_pending_window_navigations();
            let window_name = engine.config().window_name.clone();
            drop(engine);
            self.sync_target_name(&target_id, &window_name)?;
            return Ok((
                popup_requests,
                post_messages,
                window_closes,
                window_navigations,
            ));
        }
        let mut targets = self.lock_targets(BackendOperation::Script)?;
        let Some(parked) = targets.parked.get_mut(&target_id) else {
            return Ok((Vec::new(), Vec::new(), Vec::new(), Vec::new()));
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
            return Ok((Vec::new(), Vec::new(), Vec::new(), Vec::new()));
        }
        parked
            .engine
            .dispatch_post_message(
                &message.source_context_id,
                &message.source_origin,
                &message.data,
            )
            .await
            .map_err(native_error)?;
        let popup_requests = parked.engine.take_pending_popups();
        let post_messages = parked.engine.take_pending_post_messages();
        let window_closes = parked.engine.take_pending_window_closes();
        let window_navigations = parked.engine.take_pending_window_navigations();
        let window_name = parked.engine.config().window_name.clone();
        drop(targets);
        self.sync_target_name(&target_id, &window_name)?;
        Ok((
            popup_requests,
            post_messages,
            window_closes,
            window_navigations,
        ))
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
        Ok(runtime_effects.browser)
    }

    async fn navigate_frame_target(
        &self,
        route: NativeFrameRoute,
        frame_id: &str,
        source_context_id: &str,
        url: &str,
        replace_history: bool,
    ) -> Result<NativeQueuedBrowserEffects, BrowserBackendError> {
        let proxy_updates = self.window_proxy_updates(source_context_id)?;
        let (runtime_effects, owner_id) = match route {
            NativeFrameRoute::ActiveSelected => {
                let mut targets = self.lock_targets(BackendOperation::Navigate)?;
                let mut engine = self.lock_engine_raw(BackendOperation::Navigate)?;
                let result =
                    navigate_native_frame(&mut engine, url, replace_history, &proxy_updates)
                        .await?;
                close_native_frame_descendants(&mut targets.active_frames, frame_id).await?;
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
                let frame = targets
                    .active_frames
                    .parked
                    .get_mut(frame_id)
                    .ok_or_else(|| BrowserBackendError::SelectionFailed {
                        reason: "native frame disappeared during navigation".into(),
                    })?;
                let result =
                    navigate_native_frame(&mut frame.engine, url, replace_history, &proxy_updates)
                        .await?;
                close_native_frame_descendants(&mut targets.active_frames, frame_id).await?;
                (result, owner_id)
            }
            NativeFrameRoute::ParkedSelected { target_id } => {
                let mut targets = self.lock_targets(BackendOperation::Navigate)?;
                let target = targets.parked.get_mut(&target_id).ok_or_else(|| {
                    BrowserBackendError::SelectionFailed {
                        reason: "native frame owner target disappeared during navigation".into(),
                    }
                })?;
                let result =
                    navigate_native_frame(&mut target.engine, url, replace_history, &proxy_updates)
                        .await?;
                close_native_frame_descendants(&mut target.frames, frame_id).await?;
                (result, target_id)
            }
            NativeFrameRoute::ParkedParked { target_id } => {
                let mut targets = self.lock_targets(BackendOperation::Navigate)?;
                let target = targets.parked.get_mut(&target_id).ok_or_else(|| {
                    BrowserBackendError::SelectionFailed {
                        reason: "native frame owner target disappeared during navigation".into(),
                    }
                })?;
                let frame = target.frames.parked.get_mut(frame_id).ok_or_else(|| {
                    BrowserBackendError::SelectionFailed {
                        reason: "native frame disappeared during navigation".into(),
                    }
                })?;
                let result =
                    navigate_native_frame(&mut frame.engine, url, replace_history, &proxy_updates)
                        .await?;
                close_native_frame_descendants(&mut target.frames, frame_id).await?;
                (result, target_id)
            }
        };
        self.sync_target_name(&owner_id, &runtime_effects.window_name)?;
        self.process_frame_event_effects(frame_id, runtime_effects.events)
            .await?;
        Box::pin(self.process_pending_frame_scripts(runtime_effects.frame_scripts)).await?;
        Ok(runtime_effects.browser)
    }

    async fn navigate_named_target(
        &self,
        target_id: &str,
        active: bool,
        url: &str,
        replace_history: bool,
    ) -> Result<
        (
            PageTargetInfo,
            Vec<NativePopupRequest>,
            Vec<NativePostMessageRequest>,
            Vec<NativeWindowCloseRequest>,
            Vec<NativeWindowNavigationRequest>,
        ),
        BrowserBackendError,
    > {
        let proxy_updates = self.window_proxy_updates(target_id)?;
        if active {
            let opener_id = self
                .lock_targets(BackendOperation::Contexts)?
                .active_opener_id
                .clone();
            let (
                target,
                nested,
                nested_messages,
                nested_window_closes,
                nested_window_navigations,
                window_name,
            ) = {
                let mut engine = self.lock_engine_raw(BackendOperation::Navigate)?;
                engine
                    .sync_window_proxies(&proxy_updates)
                    .await
                    .map_err(native_error)?;
                engine
                    .navigate_async_with_history(url, replace_history)
                    .await
                    .map_err(native_error)?;
                let nested = engine.take_pending_popups();
                let nested_messages = engine.take_pending_post_messages();
                let nested_window_closes = engine.take_pending_window_closes();
                let nested_window_navigations = engine.take_pending_window_navigations();
                let window_name = engine.config().window_name.clone();
                let target = project_native_target(&engine, target_id, opener_id, true)?;
                (
                    target,
                    nested,
                    nested_messages,
                    nested_window_closes,
                    nested_window_navigations,
                    window_name,
                )
            };
            let mut targets = self.lock_targets(BackendOperation::Contexts)?;
            targets.active_frames = NativeFrameState::new(target_id);
            targets.active_name = native_window_name(&window_name);
            Ok((
                target,
                nested,
                nested_messages,
                nested_window_closes,
                nested_window_navigations,
            ))
        } else {
            let parked = {
                let mut targets = self.lock_targets(BackendOperation::Navigate)?;
                targets.parked.remove(target_id)
            }
            .ok_or_else(|| BrowserBackendError::SelectionFailed {
                reason: "named native popup target disappeared before navigation".into(),
            })?;
            let NativeParkedTarget {
                mut engine,
                opener_id,
                name,
                frames,
            } = parked;
            engine
                .sync_window_proxies(&proxy_updates)
                .await
                .map_err(native_error)?;
            let result = engine
                .navigate_async_with_history(url, replace_history)
                .await;
            let nested = engine.take_pending_popups();
            let nested_messages = engine.take_pending_post_messages();
            let nested_window_closes = engine.take_pending_window_closes();
            let nested_window_navigations = engine.take_pending_window_navigations();
            let window_name = engine.config().window_name.clone();
            let mut targets = self.lock_targets(BackendOperation::Contexts)?;
            if let Err(error) = result {
                targets.parked.insert(
                    target_id.to_owned(),
                    NativeParkedTarget {
                        engine,
                        opener_id,
                        frames,
                        name,
                    },
                );
                return Err(native_error(error));
            }
            let frames = NativeFrameState::new(target_id);
            let target = project_native_target(&engine, target_id, opener_id.clone(), false)?;
            targets.parked.insert(
                target_id.to_owned(),
                NativeParkedTarget {
                    engine,
                    opener_id,
                    frames,
                    name: native_window_name(&window_name),
                },
            );
            Ok((
                target,
                nested,
                nested_messages,
                nested_window_closes,
                nested_window_navigations,
            ))
        }
    }

    /// Close one target and release its worker, storage lease, and document
    /// owner. Closing the selected target deliberately leaves no implicit
    /// selection, even when parked targets remain available.
    pub async fn close_target(&self, target_id: &str) -> Result<(), BrowserBackendError> {
        validate_native_topology_id(target_id)?;
        let mut targets = self.lock_targets(BackendOperation::Close)?;
        if targets.active_target_id.as_deref() == Some(target_id) {
            let mut engine = self.lock_engine_raw(BackendOperation::Close)?;
            let closed_url = engine.context().map_err(native_error)?.url;
            let closed_name = targets.active_name.clone().unwrap_or_default();
            engine.close_async().await.map_err(native_error)?;
            let mut frames =
                std::mem::replace(&mut targets.active_frames, NativeFrameState::empty());
            targets.active_target_id = None;
            targets.active_opener_id = None;
            targets.active_name = None;
            targets.remember_closed_target(target_id.to_owned(), closed_url, closed_name);
            return close_parked_frames(&mut frames).await;
        }
        let Some(mut parked) = targets.parked.remove(target_id) else {
            return Err(BrowserBackendError::SelectionFailed {
                reason: "native page target was not found; call listTargets to refresh topology"
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
        close_parked_frames(&mut parked.frames).await
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

        let mut first_error = active_error;
        let mut active_frames = active_frames;
        if let Err(error) = close_parked_frames(&mut active_frames).await {
            first_error.get_or_insert(error);
        }
        for (_, mut parked) in parked {
            if parked.engine.lifecycle() == super::native_engine::NativeLifecycleState::Running
                && let Err(error) = parked.engine.close_async().await.map_err(native_error)
            {
                if first_error.is_none() {
                    first_error = Some(error);
                }
            }
            if let Err(error) = close_parked_frames(&mut parked.frames).await {
                if first_error.is_none() {
                    first_error = Some(error);
                }
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
        targets.active_frames.focused_frame_id = Some(frame_id.to_owned());
        targets.active_frames.discovered_generation =
            Some(active_engine.document_generation().map_err(native_error)?);
        targets.active_frames.parked.insert(
            old_frame_id,
            NativeParkedFrame {
                engine: old_engine,
                parent_id: old_parent_id,
                owner_node_index: old_owner_node_index,
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
        let mut engine = self.lock_engine(BackendOperation::Navigate)?;
        let previous_revision = engine.revision();
        let snapshot = match direction {
            NativeHistoryDirection::Back => engine.go_back_async().await,
            NativeHistoryDirection::Forward => engine.go_forward_async().await,
        }
        .map_err(native_error)?
        .ok_or_else(|| BrowserBackendError::UnsupportedOperation {
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
                        "bounded PNG of the current logical RGBA surface; JPEG/PDF and screenshot-containing evidence are unavailable".into(),
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
                    level: CertificationLevel::Experimental,
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
            let frame_script_context = if matches!(operation, BackendOperation::Script) {
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
                && let SemanticAction::Click { target } = &action_request.action
                && let Some(response) = self
                    .dispatch_point_click(&action_request.context_id, target)
                    .await?
            {
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
                    return Ok(response);
                }
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
                    let window_name = engine.config().window_name.clone();
                    drop(engine);
                    self.sync_target_name(&active_context_id, &window_name)?;
                    self.process_pending_browser_effects(
                        popup_requests,
                        post_messages,
                        window_closes,
                        window_navigations,
                    )
                    .await?;
                    Ok(BackendResponse::Unit)
                }
                (BackendOperation::Navigate, BackendRequest::Navigate(request)) => {
                    let previous_revision = engine.revision();
                    let snapshot = engine
                        .navigate_async(request.url)
                        .await
                        .map_err(native_error)?;
                    let event_effects = engine
                        .effects_since(previous_revision)
                        .map_err(native_error)?
                        .effects;
                    let popup_requests = engine.take_pending_popups();
                    let post_messages = engine.take_pending_post_messages();
                    let window_closes = engine.take_pending_window_closes();
                    let window_navigations = engine.take_pending_window_navigations();
                    let window_name = engine.config().window_name.clone();
                    drop(engine);
                    self.sync_target_name(&active_context_id, &window_name)?;
                    if let Some(frame_id) = selected_frame_id.as_deref() {
                        self.process_selected_frame_events(frame_id, event_effects)
                            .await?;
                    }
                    self.process_pending_browser_effects(
                        popup_requests,
                        post_messages,
                        window_closes,
                        window_navigations,
                    )
                    .await?;
                    Ok(BackendResponse::Navigation(NavigationResult {
                        url: snapshot.url,
                        revision: snapshot.revision,
                    }))
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
                            reason: "native engine does not implement screenshots".into(),
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
                    )
                    .await?;
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
                    )
                    .await?;
                    Ok(BackendResponse::Script(ScriptResult { value }))
                }
                (BackendOperation::Capture, BackendRequest::Capture(request)) => {
                    require_context_id(&request.context_id, &active_context_id)?;
                    if request.format != CaptureFormat::Png {
                        return Err(BrowserBackendError::UnsupportedOperation {
                            operation: "capture".into(),
                            reason: "native engine supports only bounded PNG capture".into(),
                        });
                    }
                    drop(engine);
                    let bytes = self.capture_png_async().await?;
                    Ok(BackendResponse::Capture(CaptureResult {
                        format: CaptureFormat::Png,
                        bytes,
                    }))
                }
                (BackendOperation::Storage, BackendRequest::Storage(request)) => {
                    require_context_id(&request.context_id, &active_context_id)?;
                    if matches!(&request.scope, StorageScope::Cookies)
                        && matches!(
                            &request.operation,
                            crate::browser_backend::StorageOperation::Write { .. }
                        )
                    {
                        return Err(BrowserBackendError::UnsupportedOperation {
                            operation: "storage".into(),
                            reason: "cookie writes require domain, path, and security metadata"
                                .into(),
                        });
                    }
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
    capture_native_frame_surface_at_depth(engine, frames, frame_id, 0)
}

fn capture_native_frame_surface_at_depth(
    engine: &NativeEngine,
    frames: &NativeFrameState,
    frame_id: &str,
    depth: usize,
) -> Result<NativeSurface, BrowserBackendError> {
    if depth >= NATIVE_MAX_FRAMES {
        return Err(BrowserBackendError::SelectionFailed {
            reason: "native frame surface composition exceeded its bounded depth".into(),
        });
    }
    let mut surface = engine.rasterize().map_err(native_error)?;
    let layout = engine.layout().map_err(native_error)?;
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
    if matches!(
        target.to_ascii_lowercase().as_str(),
        "_blank" | "_self" | "_parent" | "_top" | "_unfencedtop"
    ) {
        None
    } else if target.is_empty() {
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
);

struct NativeFrameRuntimeEffects {
    browser: NativeQueuedBrowserEffects,
    frame_scripts: Vec<NativeFrameScriptRequest>,
    events: Vec<NativeEffect>,
    window_name: String,
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
        if frame.engine.lifecycle() == super::native_engine::NativeLifecycleState::Running
            && let Err(error) = frame.engine.close_async().await.map_err(native_error)
        {
            first_error.get_or_insert(error);
        }
    }
    first_error.map_or(Ok(()), Err)
}

fn take_native_browser_effects(engine: &mut NativeEngine) -> (NativeQueuedBrowserEffects, String) {
    let popups = engine.take_pending_popups();
    let messages = engine.take_pending_post_messages();
    let closes = engine.take_pending_window_closes();
    let navigations = engine.take_pending_window_navigations();
    let window_name = engine.config().window_name.clone();
    ((popups, messages, closes, navigations), window_name)
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
        )
        .await
        .map_err(native_error)?;
    take_native_frame_runtime_effects(engine, previous_revision)
}

async fn navigate_native_frame(
    engine: &mut NativeEngine,
    url: &str,
    replace_history: bool,
    proxy_updates: &[NativeWindowProxyUpdate],
) -> Result<NativeFrameRuntimeEffects, BrowserBackendError> {
    let previous_revision = engine.revision();
    engine
        .sync_window_proxies(proxy_updates)
        .await
        .map_err(native_error)?;
    engine
        .navigate_async_with_history(url, replace_history)
        .await
        .map_err(native_error)?;
    take_native_frame_runtime_effects(engine, previous_revision)
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
        .max(1)
        .min(MAX_NATIVE_VIEWPORT_DIMENSION);
    let height = owner
        .content_rect
        .height
        .max(1)
        .min(MAX_NATIVE_VIEWPORT_DIMENSION);
    Ok(Some(Viewport {
        width,
        height,
        device_scale_factor_milli: parent_engine.config().viewport.device_scale_factor_milli,
    }))
}

async fn reconcile_native_frames(
    frames: &mut NativeFrameState,
    engine: &NativeEngine,
) -> Result<(), BrowserBackendError> {
    let generation = engine.document_generation().map_err(native_error)?;
    if frames.discovered_generation == Some(generation) {
        return Ok(());
    }
    let active_frame_id = frames.active_frame_id.clone();
    for frame_id in frames.descendant_ids(&active_frame_id) {
        if let Some(mut frame) = frames.parked.remove(&frame_id)
            && frame.engine.lifecycle() == super::native_engine::NativeLifecycleState::Running
        {
            let _ = frame.engine.close_async().await;
        }
    }
    let target_id = engine.config().context_id.clone();
    let mut created: BTreeMap<String, NativeParkedFrame> = BTreeMap::new();
    let mut pending_parents = VecDeque::from([active_frame_id.clone()]);
    while let Some(parent_id) = pending_parents.pop_front() {
        let (sources, base_config) = if parent_id == active_frame_id {
            (
                engine.embedded_frame_sources().map_err(native_error)?,
                engine.config().clone(),
            )
        } else {
            let parent =
                created
                    .get(&parent_id)
                    .ok_or_else(|| BrowserBackendError::SelectionFailed {
                        reason: "native frame parent disappeared during discovery".into(),
                    })?;
            (
                parent
                    .engine
                    .embedded_frame_sources()
                    .map_err(native_error)?,
                parent.engine.config().clone(),
            )
        };
        for (node_index, source) in sources {
            if frames.frame_count().saturating_add(created.len()) >= NATIVE_MAX_FRAMES {
                return Err(BrowserBackendError::SelectionFailed {
                    reason: format!("native frame limit reached ({NATIVE_MAX_FRAMES})"),
                });
            }
            let frame_id = frames.next_frame_id(&target_id);
            let parent_engine = if parent_id == active_frame_id {
                engine
            } else {
                &created
                    .get(&parent_id)
                    .ok_or_else(|| BrowserBackendError::SelectionFailed {
                        reason: "native frame parent disappeared during discovery".into(),
                    })?
                    .engine
            };
            let requested_frame_url = parent_engine
                .resolve_embedded_frame_url(&source)
                .map_err(native_error)?;
            let frame_url = if parent_engine
                .allows_embedded_frame_url(&requested_frame_url)
                .map_err(native_error)?
            {
                requested_frame_url
            } else {
                "about:blank".to_owned()
            };
            let frame_viewport = native_frame_viewport(parent_engine, node_index)?;
            let (embedding_document_url, embedding_frame_sources) =
                parent_engine.frame_navigation_policy();
            let child_config = base_config.clone().with_initial_url(frame_url);
            let child_config = if let Some(viewport) = frame_viewport {
                child_config.with_viewport(viewport)
            } else {
                child_config
            };
            let mut child = NativeEngine::new(child_config).map_err(native_error)?;
            child.set_frame_id(frame_id.clone());
            child.set_embedding_frame_policy(embedding_document_url, embedding_frame_sources);
            if let Err(error) = child.initialize_async().await {
                let _ = child.close_async().await;
                return Err(native_error(error));
            }
            pending_parents.push_back(frame_id.clone());
            created.insert(
                frame_id,
                NativeParkedFrame {
                    engine: child,
                    parent_id: Some(parent_id.clone()),
                    owner_node_index: Some(node_index),
                },
            );
        }
    }
    frames.parked.extend(created);
    if frames
        .focused_frame_id
        .as_deref()
        .is_some_and(|focused| focused != active_frame_id && !frames.parked.contains_key(focused))
    {
        frames.focused_frame_id = Some(active_frame_id.clone());
    }
    frames.discovered_generation = Some(generation);
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
