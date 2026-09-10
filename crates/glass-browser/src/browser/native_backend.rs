//! Semantic backend adapter for the Glass-owned native engine.
//!
//! The adapter is intentionally small: the native engine owns DOM and
//! lifecycle state, while this module translates only the stable
//! `browser_backend` contract.

use super::native_engine::{
    NativeAction, NativeEngine, NativeEngineConfig, NativeEngineError, NativeHistoryDirection,
    NativeInspectionSnapshot, NativePopupRequest, NativePostMessageRequest, NativePreflightAction,
    NativeTargetPreflight,
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

struct NativeParkedFrame {
    engine: NativeEngine,
    parent_id: Option<String>,
}

struct NativeFrameState {
    active_frame_id: String,
    active_parent_id: Option<String>,
    parked: BTreeMap<String, NativeParkedFrame>,
    next_frame_number: u64,
    discovered_generation: Option<u32>,
}

impl NativeFrameState {
    fn new(target_id: &str) -> Self {
        Self {
            active_frame_id: native_main_frame_id(target_id),
            active_parent_id: None,
            parked: BTreeMap::new(),
            next_frame_number: 1,
            discovered_generation: None,
        }
    }

    fn empty() -> Self {
        Self {
            active_frame_id: String::new(),
            active_parent_id: None,
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

struct NativeTargetState {
    active_target_id: Option<String>,
    active_opener_id: Option<String>,
    active_name: Option<String>,
    active_frames: NativeFrameState,
    parked: BTreeMap<String, NativeParkedTarget>,
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
        self.lock_engine(BackendOperation::Capture)?
            .capture_png()
            .map_err(native_error)
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
            .map(|(target, _, _)| target)
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
        ),
        BrowserBackendError,
    > {
        if let Some(name) = name.as_deref() {
            validate_native_popup_name(name)?;
        }
        let base_config = self
            .engine
            .lock()
            .map_err(|_| poisoned_lock_error(BackendOperation::Contexts, "engine"))?
            .config()
            .clone();
        let (target_id, opener_id, opener_window_name) = {
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
            (target_id, opener_id, opener_window_name)
        };
        let initial_window_name = name.clone().unwrap_or_default();
        let config = base_config
            .with_context_id(target_id.clone())
            .with_opener_window_name(opener_window_name)
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
        Ok((target, nested, nested_messages))
    }

    /// Click a native target and require the action to create exactly one
    /// causally-owned page target. The returned popup remains parked and the
    /// opener remains selected, matching the shared popup contract.
    pub async fn click_expect_popup(
        &self,
        target: &str,
    ) -> Result<(ActionResult, PageTargetInfo), BrowserBackendError> {
        let mut engine = self.lock_engine(BackendOperation::Action)?;
        let active_context_id = engine.config().context_id.clone();
        let outcome = engine
            .action_async(NativeAction::Click {
                target: target.to_owned(),
            })
            .await
            .map_err(native_error)?;
        let popup_requests = engine.take_pending_popups();
        let post_messages = engine.take_pending_post_messages();
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
            .process_pending_browser_effects(popup_requests, post_messages)
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
    ) -> Result<Vec<PageTargetInfo>, BrowserBackendError> {
        let mut pending_popups = VecDeque::from(popup_requests);
        let mut pending_messages = VecDeque::from(post_messages);
        let mut created = Vec::new();
        let mut processed = 0usize;
        while !pending_popups.is_empty() || !pending_messages.is_empty() {
            processed = processed.saturating_add(1);
            if processed > NATIVE_MAX_TARGETS.saturating_mul(8) {
                return Err(BrowserBackendError::SelectionFailed {
                    reason: "native browser-effect cascade exceeded its bounded limit".into(),
                });
            }
            let Some(request) = pending_popups.pop_front() else {
                let message = pending_messages
                    .pop_front()
                    .expect("message queue is non-empty when popup queue is empty");
                let (nested_popups, nested_messages) = self.deliver_post_message(message).await?;
                pending_popups.extend(nested_popups);
                pending_messages.extend(nested_messages);
                continue;
            };
            let name = native_popup_name(&request.target);
            if let Some(name) = name.as_deref()
                && let Some((target_id, active)) = self.target_named(name)?
            {
                let (_, nested, nested_messages) = self
                    .navigate_named_target(&target_id, active, &request.url)
                    .await?;
                self.bind_window_handle(&request, &target_id)?;
                pending_popups.extend(nested);
                pending_messages.extend(nested_messages);
                continue;
            }
            match self
                .create_target_named(&request.url, name, Some(request.source_context_id.clone()))
                .await
            {
                Ok((target, nested, nested_messages)) => {
                    self.bind_window_handle(&request, &target.id)?;
                    created.push(target);
                    pending_popups.extend(nested);
                    pending_messages.extend(nested_messages);
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
        }
        drop(targets);
        self.target_named(&message.target)
    }

    async fn deliver_post_message(
        &self,
        message: NativePostMessageRequest,
    ) -> Result<(Vec<NativePopupRequest>, Vec<NativePostMessageRequest>), BrowserBackendError> {
        let Some((target_id, active)) = self.message_target(&message)? else {
            return Ok((Vec::new(), Vec::new()));
        };
        if active {
            let mut engine = self.lock_engine_raw(BackendOperation::Script)?;
            let target_origin = engine.snapshot().map_err(native_error)?.origin.serialized();
            if !native_message_target_origin_matches(
                &message.source_origin,
                &message.target_origin,
                &target_origin,
            )? {
                return Ok((Vec::new(), Vec::new()));
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
            let window_name = engine.config().window_name.clone();
            drop(engine);
            self.sync_target_name(&target_id, &window_name)?;
            return Ok((popup_requests, post_messages));
        }
        let mut targets = self.lock_targets(BackendOperation::Script)?;
        let Some(parked) = targets.parked.get_mut(&target_id) else {
            return Ok((Vec::new(), Vec::new()));
        };
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
            return Ok((Vec::new(), Vec::new()));
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
        let window_name = parked.engine.config().window_name.clone();
        drop(targets);
        self.sync_target_name(&target_id, &window_name)?;
        Ok((popup_requests, post_messages))
    }

    async fn navigate_named_target(
        &self,
        target_id: &str,
        active: bool,
        url: &str,
    ) -> Result<
        (
            PageTargetInfo,
            Vec<NativePopupRequest>,
            Vec<NativePostMessageRequest>,
        ),
        BrowserBackendError,
    > {
        if active {
            let opener_id = self
                .lock_targets(BackendOperation::Contexts)?
                .active_opener_id
                .clone();
            let (target, nested, nested_messages, window_name) = {
                let mut engine = self.lock_engine_raw(BackendOperation::Navigate)?;
                engine.navigate_async(url).await.map_err(native_error)?;
                let nested = engine.take_pending_popups();
                let nested_messages = engine.take_pending_post_messages();
                let window_name = engine.config().window_name.clone();
                let target = project_native_target(&engine, target_id, opener_id, true)?;
                (target, nested, nested_messages, window_name)
            };
            let mut targets = self.lock_targets(BackendOperation::Contexts)?;
            targets.active_frames = NativeFrameState::new(target_id);
            targets.active_name = native_window_name(&window_name);
            Ok((target, nested, nested_messages))
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
            let result = engine.navigate_async(url).await;
            let nested = engine.take_pending_popups();
            let nested_messages = engine.take_pending_post_messages();
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
            Ok((target, nested, nested_messages))
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
            engine.close_async().await.map_err(native_error)?;
            let mut frames =
                std::mem::replace(&mut targets.active_frames, NativeFrameState::empty());
            targets.active_target_id = None;
            targets.active_opener_id = None;
            targets.active_name = None;
            targets
                .window_handles
                .retain(|_, target| target != target_id);
            return close_parked_frames(&mut frames).await;
        }
        let Some(mut parked) = targets.parked.remove(target_id) else {
            return Err(BrowserBackendError::SelectionFailed {
                reason: "native page target was not found; call listTargets to refresh topology"
                    .into(),
            });
        };
        if let Err(error) = parked.engine.close_async().await {
            targets.parked.insert(target_id.to_owned(), parked);
            return Err(native_error(error));
        }
        targets
            .window_handles
            .retain(|_, target| target != target_id);
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
        } = parked;
        let mut active_engine = self.lock_engine_raw(BackendOperation::Contexts)?;
        let old_engine = std::mem::replace(&mut *active_engine, parked_engine);
        let old_frame_id = std::mem::replace(
            &mut targets.active_frames.active_frame_id,
            frame_id.to_owned(),
        );
        let old_parent_id =
            std::mem::replace(&mut targets.active_frames.active_parent_id, parent_id);
        targets.active_frames.discovered_generation = None;
        targets.active_frames.parked.insert(
            old_frame_id,
            NativeParkedFrame {
                engine: old_engine,
                parent_id: old_parent_id,
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
                        "bounded click/type/key-down/key-up/shortcut/key-press/clear/check/uncheck/select/scroll, single/multi-select, form defaults, root scrolling, and native point targets; advanced selection geometry, IME, and nested scrolling remain open".into(),
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
                    "origin-keyed page local/session Web Storage; opt-in revisioned localStorage and cookie profiles; profile-journal events; IndexedDB remains open".into(),
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
                        "bounded page Web Storage and session document.cookie; opt-in revisioned localStorage/cookie profiles; per-context sessionStorage; profile-journal events; IndexedDB unavailable".into(),
                        "actions are limited to bounded click/type/key-down/key-up/shortcut/key-press/clear/check/uncheck/select/scroll, select controls, form defaults, root scrolling, and native point targets".into(),
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
            let mut engine = self.lock_engine(operation)?;
            let active_context_id = engine.config().context_id.clone();
            match (operation, request) {
                (BackendOperation::Initialize, BackendRequest::Initialize) => {
                    engine.initialize_async().await.map_err(native_error)?;
                    let popup_requests = engine.take_pending_popups();
                    let post_messages = engine.take_pending_post_messages();
                    let window_name = engine.config().window_name.clone();
                    drop(engine);
                    self.sync_target_name(&active_context_id, &window_name)?;
                    self.process_pending_browser_effects(popup_requests, post_messages)
                        .await?;
                    Ok(BackendResponse::Unit)
                }
                (BackendOperation::Navigate, BackendRequest::Navigate(request)) => {
                    let snapshot = engine
                        .navigate_async(request.url)
                        .await
                        .map_err(native_error)?;
                    let popup_requests = engine.take_pending_popups();
                    let post_messages = engine.take_pending_post_messages();
                    let window_name = engine.config().window_name.clone();
                    drop(engine);
                    self.sync_target_name(&active_context_id, &window_name)?;
                    self.process_pending_browser_effects(popup_requests, post_messages)
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
                    let outcome = engine.action_async(action).await.map_err(native_error)?;
                    let popup_requests = engine.take_pending_popups();
                    let post_messages = engine.take_pending_post_messages();
                    let window_name = engine.config().window_name.clone();
                    drop(engine);
                    self.sync_target_name(&active_context_id, &window_name)?;
                    self.process_pending_browser_effects(popup_requests, post_messages)
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
                    let value = engine
                        .evaluate_async(request.source)
                        .await
                        .map_err(native_error)?;
                    let popup_requests = engine.take_pending_popups();
                    let post_messages = engine.take_pending_post_messages();
                    let window_name = engine.config().window_name.clone();
                    drop(engine);
                    self.sync_target_name(&active_context_id, &window_name)?;
                    self.process_pending_browser_effects(popup_requests, post_messages)
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
                    let bytes = engine.capture_png().map_err(native_error)?;
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
        for (_node_index, source) in sources {
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
            let (embedding_document_url, embedding_frame_sources) =
                parent_engine.frame_navigation_policy();
            let mut child = NativeEngine::new(base_config.clone().with_initial_url(frame_url))
                .map_err(native_error)?;
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
                },
            );
        }
    }
    frames.parked.extend(created);
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
