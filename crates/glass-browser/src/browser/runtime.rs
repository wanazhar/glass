//! User-facing alternative browser runtime sessions.
//!
//! The existing [`crate::browser::session::BrowserSession`] remains the
//! Chromium-compatible high-level API. This module exposes the portable
//! semantic session used by the Glass-owned native runtime and external
//! protocol adapters while the native engine's richer owners are promoted
//! through the same session seam.

use super::backend_factory::{BackendFactory, BackendStartup};
use super::bidi_backend::BidiBackendConfig;
#[cfg(feature = "native-engine")]
use super::native_backend::NativeFrameInspectionSnapshot;
#[cfg(feature = "native-engine")]
use super::native_engine::{
    NativeEngineConfig, NativeFile, NativePreflightAction, NativeTargetPreflight,
};
#[cfg(feature = "native-engine")]
use super::policy::BrowserPolicy;
use crate::browser_backend::{
    ActionRequest, ActionResult, BackendProfile, BrowserBackendDispatcher, BrowsingContext,
    ContextRequest, EffectsRequest, EffectsResult, EvidenceLevel, EvidenceRequest, EvidenceResult,
    NavigationRequest, NavigationResult, ScriptRequest, ScriptResult, SemanticAction,
    StorageRequest, StorageResult,
};
use clap::ValueEnum;
use serde::{Deserialize, Serialize};

#[cfg(feature = "native-engine")]
use super::session::{
    ActAndVerifyResult, ActionFailureKind, ActionFailurePhase, ActionKind, ActionOutcome,
    ActionStatus, ActionTarget, ActionVerificationError, ActionVerificationEvidence,
    BootstrapObservation, CheckpointError, CheckpointObservation, CheckpointTopology, CheckpointV1,
    ConsoleEvidence, Cookie, DeltaControl, DiagnosticReport, DownloadOutcome, FindTargetResult,
    FrameInfo, InspectPageResult, IntentPolicyDecision, KnowledgeAssessmentStatus,
    KnowledgeLookupContext, KnowledgeLookupOptions, KnowledgeStore, LifecycleDiagnostics,
    MutationSummary, NavigationControlOutcome, ObservationBoundarySummary, ObservationDelta,
    ObservationIncompleteReason, PageInfo, PageTargetInfo, PendingDialog, PopupClickOutcome,
    PopupVerificationEvidence, RecoveryStrategy, SemanticIntentAction,
    SemanticIntentExecutionRequest, SemanticIntentExecutionResult, SemanticIntentExecutionStatus,
    SemanticIntentResult, SemanticObservation, SemanticObservationLevel, SemanticResolution,
    SemanticTarget, VerificationOutcome, VerificationPredicate, ViewportState, WaitCondition,
    WaitOutcome, WaitTimeout,
};
use super::session::{ActionContractError, BrowserResult};
#[cfg(feature = "native-engine")]
use crate::browser_backend::{PromptDecision, PromptResult};
#[cfg(feature = "native-engine")]
use crate::protocol::{RetryClassification, RetryGuidance};
#[cfg(feature = "native-engine")]
use std::{
    sync::atomic::{AtomicU64, Ordering},
    time::Duration,
};
use tokio::sync::Mutex;

#[cfg(feature = "native-engine")]
const NATIVE_WAIT_POLL_INTERVAL: Duration = Duration::from_millis(25);
#[cfg(feature = "native-engine")]
const NATIVE_MAX_WAIT_DEADLINE: Duration = Duration::from_secs(300);
#[cfg(feature = "native-engine")]
const NATIVE_SEMANTIC_TARGET_LIMIT: usize = 32;
#[cfg(feature = "native-engine")]
const NATIVE_SEMANTIC_ACCESSIBILITY_LIMIT: usize = 128;
#[cfg(feature = "native-engine")]
const NATIVE_SEMANTIC_TEXT_LIMIT: usize = 8 * 1024;

/// Browser runtimes supported by the portable semantic session.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum BrowserRuntime {
    /// Explicit Chrome/Chromium migration session through CDP.
    Chromium,
    /// Experimental direct WebDriver BiDi session.
    Firefox,
    /// Experimental W3C WebDriver session through SafariDriver.
    Safari,
    /// Glass-owned native browser runtime.
    #[cfg(feature = "native-engine")]
    Native,
}

impl BrowserRuntime {
    pub const fn browser_family(self) -> &'static str {
        match self {
            Self::Chromium => "chromium",
            Self::Firefox => "firefox",
            Self::Safari => "safari",
            #[cfg(feature = "native-engine")]
            Self::Native => "native",
        }
    }

    pub const fn is_native(self) -> bool {
        match self {
            #[cfg(feature = "native-engine")]
            Self::Native => true,
            _ => false,
        }
    }
}

/// Portable semantic browser session for external Firefox/Safari runtimes and
/// the explicit local native runtime.
pub struct BrowserRuntimeSession {
    runtime: BrowserRuntime,
    backend: BackendStartup,
    /// Serializes the revision read and the following mutating dispatch for
    /// one semantic session. The backend remains the owner of the actual
    /// document revision and state transition.
    operation_lock: Mutex<()>,
    #[cfg(feature = "native-engine")]
    next_execution_id: AtomicU64,
    #[cfg(feature = "native-engine")]
    native_observation_cache: Mutex<Option<SemanticObservation>>,
}

impl BrowserRuntimeSession {
    /// Connect and initialize one alternative runtime session.
    pub async fn connect(
        runtime: BrowserRuntime,
        endpoint: impl Into<String>,
    ) -> BrowserResult<Self> {
        let endpoint = endpoint.into();
        let backend = match runtime {
            BrowserRuntime::Chromium => {
                return Err("Chromium uses BrowserSession and its CDP lifecycle".into());
            }
            BrowserRuntime::Firefox => {
                BackendFactory::bidi(BidiBackendConfig::for_firefox(endpoint)).await?
            }
            BrowserRuntime::Safari => BackendFactory::safari_webdriver(endpoint)?,
            #[cfg(feature = "native-engine")]
            BrowserRuntime::Native => {
                return Err(
                    "native runtime does not accept an endpoint; use connect_native with NativeEngineConfig".into(),
                );
            }
        };
        let session = Self {
            runtime,
            backend,
            operation_lock: Mutex::new(()),
            #[cfg(feature = "native-engine")]
            next_execution_id: AtomicU64::new(1),
            #[cfg(feature = "native-engine")]
            native_observation_cache: Mutex::new(None),
        };
        BrowserBackendDispatcher::new(&session.backend)
            .initialize()
            .await?;
        Ok(session)
    }

    /// Construct and initialize the native runtime.
    #[cfg(feature = "native-engine")]
    pub async fn connect_native(config: NativeEngineConfig) -> BrowserResult<Self> {
        let backend = BackendFactory::native(config)?;
        let session = Self {
            runtime: BrowserRuntime::Native,
            backend,
            operation_lock: Mutex::new(()),
            next_execution_id: AtomicU64::new(1),
            native_observation_cache: Mutex::new(None),
        };
        BrowserBackendDispatcher::new(&session.backend)
            .initialize()
            .await?;
        Ok(session)
    }

    pub const fn runtime(&self) -> BrowserRuntime {
        self.runtime
    }

    pub fn profile(&self) -> &BackendProfile {
        self.backend.profile()
    }

    pub async fn navigate(&self, url: impl Into<String>) -> BrowserResult<NavigationResult> {
        let _operation = self.operation_lock.lock().await;
        self.navigate_unlocked(url.into()).await
    }

    /// Navigate only when the caller's observation is still current.
    ///
    /// The revision read and navigation dispatch share the session operation
    /// lock, so two concurrent callers cannot both pass the same guard.
    pub async fn navigate_with_revision(
        &self,
        url: impl Into<String>,
        expected_revision: u64,
    ) -> BrowserResult<NavigationResult> {
        let _operation = self.operation_lock.lock().await;
        self.require_current_revision(expected_revision).await?;
        self.navigate_unlocked(url.into()).await
    }

    async fn navigate_unlocked(&self, url: String) -> BrowserResult<NavigationResult> {
        Ok(BrowserBackendDispatcher::new(&self.backend)
            .navigate(NavigationRequest { url })
            .await?)
    }

    pub async fn contexts(&self) -> BrowserResult<Vec<BrowsingContext>> {
        Ok(BrowserBackendDispatcher::new(&self.backend)
            .contexts(ContextRequest {
                include_background: false,
            })
            .await?)
    }

    /// Return the native runtime's standard page-target projection.
    #[cfg(feature = "native-engine")]
    pub async fn native_list_targets(&self) -> BrowserResult<Vec<PageTargetInfo>> {
        let _operation = self.operation_lock.lock().await;
        match &self.backend {
            BackendStartup::Native(backend) => Ok(backend.list_targets()?),
            _ => Err("native target discovery is only available on the native runtime".into()),
        }
    }

    /// Return the native runtime's standard frame projection.
    #[cfg(feature = "native-engine")]
    pub async fn native_list_frames(&self) -> BrowserResult<Vec<FrameInfo>> {
        let _operation = self.operation_lock.lock().await;
        match &self.backend {
            BackendStartup::Native(backend) => Ok(backend.list_frames().await?),
            _ => Err("native frame discovery is only available on the native runtime".into()),
        }
    }

    /// Select one explicitly listed native page target.
    #[cfg(feature = "native-engine")]
    pub async fn native_select_target(&self, target_id: &str) -> BrowserResult<PageTargetInfo> {
        let _operation = self.operation_lock.lock().await;
        match &self.backend {
            BackendStartup::Native(backend) => Ok(backend.select_target(target_id)?),
            _ => Err("native target selection is only available on the native runtime".into()),
        }
    }

    /// Create a native page target without changing the active selection.
    #[cfg(feature = "native-engine")]
    pub async fn native_create_target(&self, url: &str) -> BrowserResult<PageTargetInfo> {
        let _operation = self.operation_lock.lock().await;
        match &self.backend {
            BackendStartup::Native(backend) => Ok(backend.create_target(url).await?),
            _ => Err("native target creation is only available on the native runtime".into()),
        }
    }

    /// Click a native target and return the one newly-created page target.
    /// The opener remains selected; the target is independently available via
    /// `native_list_targets` and `native_select_target`.
    #[cfg(feature = "native-engine")]
    pub async fn native_click_expect_popup(
        &self,
        target: &str,
        expected_revision: Option<u64>,
    ) -> BrowserResult<PopupClickOutcome> {
        let _operation = self.operation_lock.lock().await;
        let observation = self.native_semantic_observation_unlocked().await?;
        if let Some(expected_revision) = expected_revision
            && observation.revision != expected_revision
        {
            return Err(Box::new(ActionContractError::stale_revision(
                expected_revision,
                observation.revision,
            )));
        }
        let preflight = match &self.backend {
            BackendStartup::Native(backend) => {
                backend
                    .preflight_target(target, NativePreflightAction::Click)
                    .await?
            }
            _ => return Err("native popup clicks are only available on the native runtime".into()),
        };
        let (action, popup) = match &self.backend {
            BackendStartup::Native(backend) => {
                backend
                    .click_expect_popup_in_frame(target, preflight.frame_id.as_deref())
                    .await?
            }
            _ => return Err("native popup clicks are only available on the native runtime".into()),
        };
        let label = preflight
            .node
            .as_ref()
            .map(|node| node.name.clone())
            .unwrap_or_else(|| target.to_owned());
        let opener_id = observation.page.target_id.clone();
        Ok(PopupClickOutcome {
            action: ActionKind::ClickExpectPopup,
            execution_id: self.next_native_execution_id(),
            target: ActionTarget {
                label,
                reference: Some(target.to_owned()),
            },
            revision: action.revision,
            target_id: opener_id.clone(),
            frame_id: observation.page.frame_id,
            causally_verified_popup: true,
            popup_id: popup.id,
            opener_id,
            evidence: PopupVerificationEvidence {
                trusted_click_witness: true,
                release_acknowledged: true,
                release_ack_wait_ms: 0.0,
                topology_sequence_before_release: 0,
                popup_observed_sequence: 0,
                attached: true,
                ready_state: "complete".into(),
            },
        })
    }

    /// Close one native page target. Closing the active target clears selection.
    #[cfg(feature = "native-engine")]
    pub async fn native_close_target(&self, target_id: &str) -> BrowserResult<()> {
        let _operation = self.operation_lock.lock().await;
        match &self.backend {
            BackendStartup::Native(backend) => Ok(backend.close_target(target_id).await?),
            _ => Err("native target closure is only available on the native runtime".into()),
        }
    }

    /// Select one explicitly listed native frame.
    #[cfg(feature = "native-engine")]
    pub async fn native_select_frame(&self, frame_id: &str) -> BrowserResult<FrameInfo> {
        let _operation = self.operation_lock.lock().await;
        match &self.backend {
            BackendStartup::Native(backend) => Ok(backend.select_frame(frame_id).await?),
            _ => Err("native frame selection is only available on the native runtime".into()),
        }
    }

    /// Traverse native session history while preserving the revision-bound
    /// navigation-control result used by the Chromium session.
    #[cfg(feature = "native-engine")]
    pub async fn native_navigate_history(
        &self,
        direction: super::native_engine::NativeHistoryDirection,
    ) -> BrowserResult<NavigationControlOutcome> {
        let _operation = self.operation_lock.lock().await;
        match &self.backend {
            BackendStartup::Native(backend) => Ok(backend.navigate_history(direction).await?),
            _ => Err("native history is only available on the native runtime".into()),
        }
    }

    /// Traverse native history only when the caller's observation is current.
    #[cfg(feature = "native-engine")]
    pub async fn native_navigate_history_with_revision(
        &self,
        direction: super::native_engine::NativeHistoryDirection,
        expected_revision: u64,
    ) -> BrowserResult<NavigationControlOutcome> {
        let _operation = self.operation_lock.lock().await;
        self.require_current_revision(expected_revision).await?;
        match &self.backend {
            BackendStartup::Native(backend) => Ok(backend.navigate_history(direction).await?),
            _ => Err("native history is only available on the native runtime".into()),
        }
    }

    /// Reload the selected native page only when the caller's observation is
    /// current. Reload replaces the active history entry.
    #[cfg(feature = "native-engine")]
    pub async fn native_reload_with_revision(
        &self,
        expected_revision: u64,
    ) -> BrowserResult<NavigationControlOutcome> {
        let _operation = self.operation_lock.lock().await;
        self.require_current_revision(expected_revision).await?;
        match &self.backend {
            BackendStartup::Native(backend) => Ok(backend.reload().await?),
            _ => Err("native reload is only available on the native runtime".into()),
        }
    }

    /// Stop-loading is a revision-checked no-op for the current native owner.
    /// Native navigation is completed before its command returns, so there is
    /// no detached load task to cancel; preserving the control result keeps
    /// the resident protocol deterministic and honest.
    #[cfg(feature = "native-engine")]
    pub async fn native_stop_loading_with_revision(
        &self,
        expected_revision: u64,
    ) -> BrowserResult<NavigationControlOutcome> {
        let _operation = self.operation_lock.lock().await;
        self.require_current_revision(expected_revision).await?;
        Ok(NavigationControlOutcome {
            action: "stopLoading".into(),
            previous_revision: expected_revision,
            current_revision: expected_revision,
        })
    }

    /// Validate one native target for the resident highlight command.
    /// Visual composition does not mutate the page; target identity remains
    /// revision-scoped for the caller.
    #[cfg(feature = "native-engine")]
    pub async fn native_highlight_target_with_revision(
        &self,
        target: &str,
        expected_revision: u64,
    ) -> BrowserResult<()> {
        let _operation = self.operation_lock.lock().await;
        self.require_current_revision(expected_revision).await?;
        let preflight = match &self.backend {
            BackendStartup::Native(backend) => {
                backend
                    .preflight_target(target, NativePreflightAction::Click)
                    .await?
            }
            _ => return Err("native highlighting is only available on the native runtime".into()),
        };
        if !preflight.unique {
            return Err("native highlight target was not resolved uniquely".into());
        }
        Ok(())
    }

    /// Return the oldest unresolved native JavaScript dialog, if present.
    #[cfg(feature = "native-engine")]
    pub async fn native_pending_dialog(&self) -> BrowserResult<Option<PendingDialog>> {
        let _operation = self.operation_lock.lock().await;
        match &self.backend {
            BackendStartup::Native(backend) => Ok(backend.pending_dialog().await?),
            _ => Err("native dialog inspection is only available on the native runtime".into()),
        }
    }

    /// Accept or dismiss one native JavaScript dialog through the backend
    /// prompt contract.
    #[cfg(feature = "native-engine")]
    pub async fn native_resolve_dialog(
        &self,
        decision: PromptDecision,
    ) -> BrowserResult<PromptResult> {
        let _operation = self.operation_lock.lock().await;
        match &self.backend {
            BackendStartup::Native(backend) => Ok(backend.resolve_dialog(decision).await?),
            _ => Err("native dialog resolution is only available on the native runtime".into()),
        }
    }

    /// Report whether the native request owner has stayed idle for the
    /// requested bounded interval.
    #[cfg(feature = "native-engine")]
    pub async fn native_network_quiet(&self, duration: Duration) -> BrowserResult<(bool, String)> {
        let _operation = self.operation_lock.lock().await;
        match &self.backend {
            BackendStartup::Native(backend) => Ok(backend.network_quiet(duration)?),
            _ => Err("native network activity is only available on the native runtime".into()),
        }
    }

    /// Complete the oldest native anchor download into an authorized
    /// destination without allocating Chromium or using CDP.
    #[cfg(feature = "native-engine")]
    pub async fn native_wait_for_download(
        &self,
        destination: &std::path::Path,
        deadline: Duration,
    ) -> BrowserResult<DownloadOutcome> {
        let _operation = self.operation_lock.lock().await;
        match &self.backend {
            BackendStartup::Native(backend) => {
                Ok(backend.wait_for_download(destination, deadline).await?)
            }
            _ => Err("native downloads are only available on the native runtime".into()),
        }
    }

    /// Return the completed native download count for causal verification.
    #[cfg(feature = "native-engine")]
    pub async fn native_completed_download_count(&self) -> BrowserResult<u64> {
        let _operation = self.operation_lock.lock().await;
        match &self.backend {
            BackendStartup::Native(backend) => Ok(backend.completed_download_count()?),
            _ => Err("native downloads are only available on the native runtime".into()),
        }
    }

    pub async fn evidence(&self, level: EvidenceLevel) -> BrowserResult<EvidenceResult> {
        let context_id = self.active_context_id().await?;
        Ok(BrowserBackendDispatcher::new(&self.backend)
            .evidence(EvidenceRequest { context_id, level })
            .await?)
    }

    pub async fn script(&self, source: impl Into<String>) -> BrowserResult<ScriptResult> {
        let _operation = self.operation_lock.lock().await;
        let context_id = self.active_context_id().await?;
        Ok(BrowserBackendDispatcher::new(&self.backend)
            .script(ScriptRequest {
                context_id,
                source: source.into(),
            })
            .await?)
    }

    pub async fn action(&self, action: SemanticAction) -> BrowserResult<ActionResult> {
        let _operation = self.operation_lock.lock().await;
        self.action_unlocked(action).await
    }

    /// Apply one semantic action only when the caller's observation is still
    /// current. The guard covers the complete read/dispatch transaction.
    pub async fn action_with_revision(
        &self,
        action: SemanticAction,
        expected_revision: u64,
    ) -> BrowserResult<ActionResult> {
        let _operation = self.operation_lock.lock().await;
        self.require_current_revision(expected_revision).await?;
        self.action_unlocked(action).await
    }

    async fn action_unlocked(&self, action: SemanticAction) -> BrowserResult<ActionResult> {
        let context_id = self.active_context_id().await?;
        Ok(BrowserBackendDispatcher::new(&self.backend)
            .action(ActionRequest { context_id, action })
            .await?)
    }

    pub async fn effects(&self, since_revision: u64) -> BrowserResult<EffectsResult> {
        let context_id = self.active_context_id().await?;
        Ok(BrowserBackendDispatcher::new(&self.backend)
            .effects(EffectsRequest {
                context_id,
                since_revision,
            })
            .await?)
    }

    pub async fn storage(&self, request: StorageRequest) -> BrowserResult<StorageResult> {
        let _operation = self.operation_lock.lock().await;
        Ok(BrowserBackendDispatcher::new(&self.backend)
            .storage(request)
            .await?)
    }

    #[cfg(feature = "native-engine")]
    pub async fn native_cookies(&self) -> BrowserResult<Vec<Cookie>> {
        let _operation = self.operation_lock.lock().await;
        match &self.backend {
            BackendStartup::Native(backend) => Ok(backend.cookies().await?),
            _ => Err("native cookies are only available on the native runtime".into()),
        }
    }

    #[cfg(feature = "native-engine")]
    pub async fn native_set_cookies(&self, cookies: &[Cookie]) -> BrowserResult<()> {
        let _operation = self.operation_lock.lock().await;
        match &self.backend {
            BackendStartup::Native(backend) => Ok(backend.set_cookies(cookies).await?),
            _ => Err("native cookie import is only available on the native runtime".into()),
        }
    }

    #[cfg(feature = "native-engine")]
    pub async fn native_clear_cookies(&self) -> BrowserResult<()> {
        let _operation = self.operation_lock.lock().await;
        match &self.backend {
            BackendStartup::Native(backend) => Ok(backend.clear_cookies().await?),
            _ => Err("native cookie clearing is only available on the native runtime".into()),
        }
    }

    /// Return the native engine's bounded semantic accessibility projection.
    #[cfg(feature = "native-engine")]
    pub fn native_semantic_nodes(
        &self,
    ) -> BrowserResult<Vec<super::native_engine::NativeSemanticNode>> {
        match &self.backend {
            BackendStartup::Native(backend) => Ok(backend.semantic_nodes()?),
            _ => Err("semantic node projection is only available on the native runtime".into()),
        }
    }

    /// Capture a native logical software surface as PNG bytes.
    #[cfg(feature = "native-engine")]
    pub fn native_capture_png(&self) -> BrowserResult<Vec<u8>> {
        match &self.backend {
            BackendStartup::Native(backend) => Ok(backend.capture_png()?),
            _ => Err("native PNG capture is only available on the native runtime".into()),
        }
    }

    /// Return the native logical viewport dimensions.
    #[cfg(feature = "native-engine")]
    pub fn native_viewport_size(&self) -> BrowserResult<(f64, f64)> {
        match &self.backend {
            BackendStartup::Native(backend) => Ok(backend.viewport_size()?),
            _ => Err("native viewport inspection is only available on the native runtime".into()),
        }
    }

    /// Capture a native logical software surface after discovering and
    /// compositing the selected frame's live child browsing contexts.
    #[cfg(feature = "native-engine")]
    pub async fn native_capture_png_async(&self) -> BrowserResult<Vec<u8>> {
        let _operation = self.operation_lock.lock().await;
        match &self.backend {
            BackendStartup::Native(backend) => Ok(backend.capture_png_async().await?),
            _ => Err("native PNG capture is only available on the native runtime".into()),
        }
    }

    /// Run a side-effect-free native target preflight under the session's
    /// operation lock.
    #[cfg(feature = "native-engine")]
    pub async fn native_preflight_target(
        &self,
        target: &str,
        action: NativePreflightAction,
    ) -> BrowserResult<NativeTargetPreflight> {
        let _operation = self.operation_lock.lock().await;
        match &self.backend {
            BackendStartup::Native(backend) => Ok(backend.preflight_target(target, action).await?),
            _ => Err("native target preflight is only available on the native runtime".into()),
        }
    }

    /// Upload bounded file objects to one fresh, uniquely-resolved native
    /// file input while preserving the caller's optional semantic revision.
    #[cfg(feature = "native-engine")]
    pub async fn native_upload_files(
        &self,
        target: &str,
        files: Vec<NativeFile>,
        expected_revision: Option<u64>,
    ) -> BrowserResult<ActionResult> {
        let _operation = self.operation_lock.lock().await;
        NativeFile::validate_many(&files)?;
        let observation = self.native_semantic_observation_unlocked().await?;
        if let Some(expected_revision) = expected_revision
            && observation.revision != expected_revision
        {
            return Err(Box::new(ActionContractError::stale_revision(
                expected_revision,
                observation.revision,
            )));
        }
        let preflight = match &self.backend {
            BackendStartup::Native(backend) => {
                backend
                    .preflight_target(target, NativePreflightAction::Upload)
                    .await?
            }
            _ => return Err("native uploads are only available on the native runtime".into()),
        };
        if !preflight.unique {
            return Err(format!(
                "native upload target could not be resolved uniquely: {:?}",
                preflight.error_kind
            )
            .into());
        }
        if preflight.actionable != Some(true) {
            return Err(format!(
                "native upload target is not actionable: {:?}",
                preflight.actionability_reason
            )
            .into());
        }
        let frame_id = preflight
            .frame_id
            .as_deref()
            .unwrap_or(&observation.page.frame_id);
        match &self.backend {
            BackendStartup::Native(backend) => Ok(backend
                .upload_files_in_frame(&observation.route.target_id, frame_id, target, files)
                .await?),
            _ => Err("native uploads are only available on the native runtime".into()),
        }
    }

    /// Build the standard Glass agent inspection envelope from one atomic
    /// native page/semantic/layout snapshot.
    #[cfg(feature = "native-engine")]
    pub async fn native_inspect_page(&self) -> BrowserResult<InspectPageResult> {
        let observation = self.native_semantic_observation().await?;
        Ok(InspectPageResult {
            page: observation.page,
            revision: observation.revision,
            regions: observation.regions,
            limits: observation.limits,
            focused_target: None,
            alerts: Vec::new(),
        })
    }

    /// Return one complete native semantic observation for storage, workflow,
    /// and protocol adapters that need the same revisioned source as inspect.
    #[cfg(feature = "native-engine")]
    pub async fn native_observe(&self) -> BrowserResult<super::session::SemanticObservation> {
        self.native_semantic_observation().await
    }

    /// Return native page-state evidence through the same bootstrap contract
    /// used by the CDP session. Bootstrap never publishes action references.
    #[cfg(feature = "native-engine")]
    pub async fn native_observe_bootstrap(&self) -> BrowserResult<BootstrapObservation> {
        let _operation = self.operation_lock.lock().await;
        let frames = self.native_inspection_snapshots_unlocked().await?;
        let root = frames
            .first()
            .ok_or("native bootstrap returned no active frame")?;
        let snapshot = &root.inspection.snapshot;
        let (text, text_truncated) = bounded_native_semantic_text(&frames);
        let page = PageInfo {
            url: snapshot.url.clone(),
            title: snapshot.title.clone(),
            ready_state: if snapshot.lifecycle.as_str() == "running" {
                "complete".into()
            } else {
                snapshot.lifecycle.as_str().into()
            },
            target_id: root.inspection.context_id.clone(),
            frame_id: root.frame_id.clone(),
        };
        let landmarks = root
            .inspection
            .nodes
            .iter()
            .take(NATIVE_SEMANTIC_TARGET_LIMIT)
            .map(|node| super::session::PageStateLandmark {
                role: node.role.clone(),
                name: node.name.clone(),
            })
            .collect::<Vec<_>>();
        let child_frames = frames.len().saturating_sub(1);
        let mut incomplete = Vec::new();
        if snapshot.text_truncated || text_truncated {
            incomplete.push(ObservationIncompleteReason::VisibleText);
        }
        if child_frames > 0 {
            incomplete.push(ObservationIncompleteReason::FrameBoundary);
        }
        let viewport = root.inspection.layout.viewport;
        let scroll_offset = root.inspection.layout.scroll_offset;
        let boundaries = ObservationBoundarySummary {
            scanned_elements: root.inspection.nodes.len(),
            scan_limit: root.inspection.nodes.len(),
            shadow_roots: 0,
            child_frames,
            canvases: 0,
            canvas_2d: 0,
            webgl_canvases: 0,
            webgpu_canvases: 0,
            svg_elements: 0,
            media_elements: 0,
            embedded_documents: child_frames,
            pdf_documents: 0,
            native_surfaces: 1,
            truncated: snapshot.title_truncated || snapshot.text_truncated,
            text_truncated,
            viewport: Some(ViewportState {
                scroll_x: f64::from(scroll_offset.x),
                scroll_y: f64::from(scroll_offset.y),
                width: f64::from(viewport.width),
                height: f64::from(viewport.height),
                document_width: f64::from(root.inspection.layout.content_width),
                document_height: f64::from(root.inspection.layout.content_height),
            }),
        };
        let revision = native_aggregate_revision(&frames);
        let ready = snapshot.lifecycle.as_str() == "running";
        let page_context_id = format!("{}:{revision}", page.target_id);
        let complete = ready && incomplete.is_empty();
        Ok(BootstrapObservation {
            page: page.clone(),
            text: text.clone(),
            classification: super::session::classify_page_state(&page, &text, &landmarks),
            revision,
            context_id: 0,
            page_context_id,
            ready,
            complete,
            consistency: super::session::ObservationConsistency {
                consistent: true,
                attempts: 1,
                start_revision: revision,
                end_revision: revision,
                start_mutation_revision: revision,
                end_mutation_revision: revision,
            },
            boundaries,
            incomplete,
        })
    }

    /// Extract structured records from a current native semantic observation
    /// using the shared bounded extraction implementation.
    #[cfg(feature = "native-engine")]
    pub async fn native_extract_structured(
        &self,
        request: &super::session::StructuredExtractionRequest,
        policy: &BrowserPolicy,
    ) -> BrowserResult<super::session::StructuredExtractionResult> {
        if super::session::extraction_request_requires_sensitive_access(request) {
            policy.require_sensitive_extraction()?;
        }
        let observation = self
            .native_semantic_observe(SemanticObservationLevel::Structured)
            .await?;
        super::session::extract_structured_from_observation(
            &observation,
            request,
            policy.allow_sensitive_extraction(),
        )
    }

    /// Resolve one native intent without dispatching an action.
    #[cfg(feature = "native-engine")]
    pub async fn native_resolve_intent(
        &self,
        request: &super::session::SemanticIntentRequest,
    ) -> BrowserResult<SemanticIntentResult> {
        let observation = self
            .native_semantic_observe(SemanticObservationLevel::Interactive)
            .await?;
        Ok(super::session::resolve_intent(request, &observation))
    }

    /// Resolve one native intent with historical fingerprints used only as
    /// bounded explanation evidence; stored knowledge never supplies a target.
    #[cfg(feature = "native-engine")]
    pub async fn native_resolve_intent_with_knowledge(
        &self,
        request: &super::session::SemanticIntentRequest,
        store: &KnowledgeStore,
        lookup_options: KnowledgeLookupOptions,
    ) -> BrowserResult<SemanticIntentResult> {
        let observation = self
            .native_semantic_observe(SemanticObservationLevel::Interactive)
            .await?;
        let context = KnowledgeLookupContext::from_observation(&observation, lookup_options)?;
        let assessments = store.assess(&context);
        let historical_fingerprints = store
            .records()
            .iter()
            .zip(assessments)
            .filter(|(record, assessment)| {
                record.kind == super::session::KnowledgeRecordKind::TargetFingerprint
                    && assessment.status == KnowledgeAssessmentStatus::Eligible
            })
            .filter_map(|(record, _)| {
                record
                    .data
                    .get("fingerprint")
                    .and_then(|value| value.as_str())
            })
            .map(str::to_string)
            .collect::<std::collections::BTreeSet<_>>();
        Ok(super::session::resolve_intent_with_historical_matches(
            request,
            &observation,
            &historical_fingerprints,
        ))
    }

    /// Resolve and execute one native intent through the guarded action path.
    #[cfg(feature = "native-engine")]
    pub async fn native_execute_intent(
        &self,
        request: &SemanticIntentExecutionRequest,
    ) -> BrowserResult<SemanticIntentExecutionResult> {
        Ok(self
            .native_act_and_verify(request, None, Duration::from_millis(1))
            .await?
            .execution)
    }

    /// Return a bounded delta from the last public native observation.
    #[cfg(feature = "native-engine")]
    pub async fn native_observe_delta(&self) -> BrowserResult<ObservationDelta> {
        let _operation = self.operation_lock.lock().await;
        let previous = self
            .native_observation_cache
            .lock()
            .await
            .clone()
            .ok_or("observe_delta requires a prior native semantic observation")?;
        let current = self
            .native_semantic_observation_level_unlocked(SemanticObservationLevel::Structured)
            .await?;
        if previous.route != current.route {
            return Err("native observe_delta cannot compare different routes".into());
        }
        let old_targets = previous
            .regions
            .iter()
            .flat_map(|region| region.targets.iter())
            .collect::<Vec<_>>();
        let current_targets = current
            .regions
            .iter()
            .flat_map(|region| region.targets.iter())
            .collect::<Vec<_>>();
        let control = |target: &SemanticTarget| DeltaControl {
            reference: target.reference.clone(),
            role: target.role.clone(),
            name: target.name.clone(),
        };
        let mut added = Vec::new();
        let mut removed = Vec::new();
        let mut changed = Vec::new();
        for target in &current_targets {
            match old_targets
                .iter()
                .find(|previous| previous.reference == target.reference)
            {
                None if added.len() < 8 => added.push(control(target)),
                Some(previous) if *previous != *target && changed.len() < 8 => {
                    changed.push(control(target));
                }
                _ => {}
            }
        }
        for target in &old_targets {
            if !current_targets
                .iter()
                .any(|current| current.reference == target.reference)
                && removed.len() < 8
            {
                removed.push(control(target));
            }
        }
        let delta = ObservationDelta {
            from_revision: previous.revision,
            to_revision: current.revision,
            mutation_summary: MutationSummary {
                url_changed: previous.page.url != current.page.url,
                title_changed: previous.page.title != current.page.title,
                revision_delta: current.revision.saturating_sub(previous.revision),
                soft_navigation_suspected: current.revision > previous.revision
                    && previous.page.url == current.page.url
                    && previous.page.title == current.page.title,
            },
            added,
            removed,
            changed,
            prior_incomplete: native_observation_incomplete(&previous),
            current_incomplete: native_observation_incomplete(&current),
        };
        *self.native_observation_cache.lock().await = Some(current);
        Ok(delta)
    }

    /// Export the same bounded checkpoint envelope as the CDP session.
    #[cfg(feature = "native-engine")]
    pub async fn native_export_checkpoint(
        &self,
        profile: &str,
        policy: &BrowserPolicy,
    ) -> BrowserResult<CheckpointV1> {
        let _operation = self.operation_lock.lock().await;
        let frames = self.native_inspection_snapshots_unlocked().await?;
        let observation =
            build_native_semantic_observation(&frames, SemanticObservationLevel::Structured)?;
        let root = frames
            .first()
            .ok_or("native checkpoint export returned no active frame")?;
        let last_refs = observation
            .regions
            .iter()
            .flat_map(|region| region.targets.iter())
            .take(8)
            .map(|target| target.reference.clone())
            .collect();
        let checkpoint = CheckpointV1 {
            schema_version: 1,
            glass_version: env!("CARGO_PKG_VERSION").to_string(),
            exported_at: chrono::Utc::now().to_rfc3339(),
            profile: profile.to_owned(),
            attach_mode: false,
            topology: CheckpointTopology {
                target_id: Some(root.inspection.context_id.clone()),
                frame_id: Some(root.frame_id.clone()),
                url: bounded_checkpoint_text(&root.inspection.snapshot.url, 1024),
                title: bounded_checkpoint_text(&root.inspection.snapshot.title, 1024),
            },
            observation: CheckpointObservation {
                revision: observation.revision,
                last_refs,
            },
            policy: format!("{:?}", policy.preset()).to_lowercase(),
        };
        if serde_json::to_vec(&checkpoint)?.len() > 4 * 1024 {
            return Err("checkpoint exceeds the 4 KiB serialized limit".into());
        }
        Ok(checkpoint)
    }

    /// Restore only native target/frame selection from a checkpoint. No action
    /// is ever replayed during import.
    #[cfg(feature = "native-engine")]
    pub async fn native_import_checkpoint(&self, checkpoint: &CheckpointV1) -> BrowserResult<()> {
        if checkpoint.schema_version != 1 {
            return Err(CheckpointError::SchemaVersionMismatch {
                expected: 1,
                found: checkpoint.schema_version,
            }
            .into());
        }
        if let Some(target_id) = checkpoint.topology.target_id.as_deref() {
            let targets = self.native_list_targets().await?;
            if !targets.iter().any(|target| target.id == target_id) {
                return Err(CheckpointError::TargetClosed.into());
            }
            self.native_select_target(target_id).await?;
        }
        if let Some(frame_id) = checkpoint.topology.frame_id.as_deref() {
            let frames = self.native_list_frames().await?;
            if !frames.iter().any(|frame| frame.id == frame_id) {
                return Err(CheckpointError::Stale.into());
            }
            self.native_select_frame(frame_id).await?;
        }
        Ok(())
    }

    /// Project native parser/presentation diagnostics into the stable Glass
    /// diagnostics envelope. Native diagnostics are intentionally limited to
    /// sanitized CSS/document warnings; no synthetic network or console event
    /// is invented when the native owner did not observe one.
    #[cfg(feature = "native-engine")]
    pub async fn native_diagnostics(&self, duration: Duration) -> BrowserResult<DiagnosticReport> {
        let _operation = self.operation_lock.lock().await;
        let (diagnostics, inspection) = match &self.backend {
            BackendStartup::Native(backend) => {
                (backend.diagnostics()?, backend.inspection_snapshot()?)
            }
            _ => return Err("native diagnostics are only available on the native runtime".into()),
        };
        let console = diagnostics
            .diagnostics
            .into_iter()
            .map(|diagnostic| ConsoleEvidence {
                level: "warning".into(),
                text: format!(
                    "native {:?} {:?} at {}: {}",
                    diagnostic.code, diagnostic.source, diagnostic.offset, diagnostic.detail
                ),
            })
            .collect();
        let ready = inspection.snapshot.lifecycle.as_str() == "running";
        Ok(DiagnosticReport {
            target_id: inspection.context_id.clone(),
            frame_id: format!("{}:main", inspection.context_id),
            duration_ms: duration.as_millis() as u64,
            console,
            network: Vec::new(),
            dropped_events: u64::from(diagnostics.truncated),
            startup_diagnostics: Default::default(),
            lifecycle: LifecycleDiagnostics {
                browser_ready: ready,
                navigation_started: ready,
                evidence_ready: ready,
                action_verified: false,
            },
        })
    }

    /// Extract the active native document into the stable, bounded Web IR v1
    /// contract without creating a Chromium/CDP session.
    #[cfg(feature = "native-engine")]
    pub async fn native_extract_web_ir(
        &self,
        request: &crate::extraction::ExtractionRequest,
    ) -> BrowserResult<crate::web_ir::GlassWebIrV1> {
        request.validate()?;
        let _operation = self.operation_lock.lock().await;
        let frames = self.native_inspection_snapshots_unlocked().await?;
        let selected_frames = match &request.scope {
            crate::extraction::ExtractionScope::Frame { frame_id } => {
                let frame = frames
                    .iter()
                    .find(|frame| frame.frame_id == *frame_id)
                    .ok_or_else(|| {
                        crate::extraction::ExtractionContractError::new(
                            "scope.frameId",
                            format!(
                                "frame {frame_id:?} is not present at the current native revision"
                            ),
                        )
                    })?;
                vec![frame.clone()]
            }
            crate::extraction::ExtractionScope::Document
            | crate::extraction::ExtractionScope::Region { .. } => frames,
        };
        let observation = build_native_semantic_observation(
            &selected_frames,
            super::session::SemanticObservationLevel::Structured,
        )?;
        super::native_extraction::extract(&observation, request)
    }

    /// Return the requested bounded semantic observation level from one
    /// revisioned native frame snapshot.
    #[cfg(feature = "native-engine")]
    pub async fn native_semantic_observe(
        &self,
        level: super::session::SemanticObservationLevel,
    ) -> BrowserResult<super::session::SemanticObservation> {
        let _operation = self.operation_lock.lock().await;
        let observation = self
            .native_semantic_observation_level_unlocked(level)
            .await?;
        *self.native_observation_cache.lock().await = Some(observation.clone());
        Ok(observation)
    }

    /// Expand one native semantic region while keeping its revision and route
    /// contract. The source frames and expanded payload are captured under
    /// the same runtime operation lock.
    #[cfg(feature = "native-engine")]
    pub async fn native_semantic_expand_region(
        &self,
        region_id: &str,
        expected_revision: u64,
        level: super::session::SemanticObservationLevel,
    ) -> BrowserResult<super::session::SemanticObservation> {
        if region_id.trim().is_empty() {
            return Err(super::session::SemanticObservationError::new(
                "regionId",
                "semantic region ID cannot be empty",
            )
            .into());
        }
        let _operation = self.operation_lock.lock().await;
        let frames = self.native_inspection_snapshots_unlocked().await?;
        let mut observation = build_native_semantic_observation(&frames, level)?;
        if observation.revision != expected_revision {
            return Err(Box::new(ActionContractError::stale_revision(
                expected_revision,
                observation.revision,
            )));
        }
        let region_index = observation
            .regions
            .iter()
            .position(|region| region.id == region_id)
            .ok_or_else(|| {
                super::session::SemanticObservationError::new(
                    "regionId",
                    format!("region {region_id:?} is not present at revision {expected_revision}"),
                )
            })?;
        let selected = observation.regions.remove(region_index);
        let omitted_regions = observation.regions.len();
        observation.regions = vec![selected];
        observation.limits.omitted_regions = omitted_regions;
        observation.limits.truncated |= omitted_regions > 0;
        observation.limits.omitted_targets = observation
            .regions
            .first()
            .map(|region| {
                region
                    .interactive_count
                    .saturating_sub(region.targets.len())
            })
            .unwrap_or_default();

        let selected_frames = std::slice::from_ref(&frames[region_index]);
        if native_level_includes_text(level) {
            let (text, text_truncated) = bounded_native_semantic_text(selected_frames);
            observation.text = Some(text.clone());
            observation.limits.text_bytes = Some(text.len());
            observation.limits.text_truncated = text_truncated;
            observation.limits.truncated |= text_truncated;
        } else {
            observation.text = None;
            observation.limits.text_bytes = None;
            observation.limits.text_truncated = false;
        }
        observation.accessibility = native_level_includes_accessibility(level)
            .then(|| native_accessibility_nodes(selected_frames));
        observation.raw_accessibility = native_level_includes_raw_accessibility(level)
            .then(|| native_accessibility_nodes(selected_frames));

        observation.validate()?;
        Ok(observation)
    }

    /// Read the bounded native form-value projection. Password controls are
    /// redacted unless the caller has separately granted sensitive form
    /// access through policy.
    #[cfg(feature = "native-engine")]
    pub async fn native_form_values(
        &self,
        allow_sensitive: bool,
    ) -> BrowserResult<serde_json::Value> {
        super::native_batch::read_form_values(self, allow_sensitive).await
    }

    /// Execute one authored Glass Task Protocol request entirely in the
    /// native runtime. The adapter preserves the task envelope and revision
    /// contract used by the CDP session without creating a CDP session.
    #[cfg(feature = "native-engine")]
    pub async fn native_execute_task(
        &self,
        task: &crate::task_protocol::GlassTask,
        expected_revision: u64,
        confirmed: bool,
    ) -> BrowserResult<super::session::TaskExecutionResult> {
        super::native_task::execute(self, task, expected_revision, confirmed).await
    }

    /// Execute one ordered batch through the native owner. CLI, MCP, and
    /// persistent-session callers all enter through this method so revision
    /// chaining and action dispatch cannot diverge between surfaces.
    #[cfg(feature = "native-engine")]
    pub async fn native_run_batch(
        &self,
        steps: &[super::session::BatchStep],
        atomic: bool,
        mode: super::session::BatchMode,
        expected_revision: Option<u64>,
    ) -> BrowserResult<super::session::BatchOutcome> {
        super::native_batch::run(self, steps, atomic, mode, expected_revision).await
    }

    /// Execute a declarative workflow through the native engine and its
    /// caller's policy boundary.
    #[cfg(feature = "native-engine")]
    pub async fn native_run_workflow(
        &self,
        policy: &super::policy::BrowserPolicy,
        workflow: &super::session::WorkflowDefinition,
        inputs: &std::collections::BTreeMap<String, serde_json::Value>,
    ) -> BrowserResult<super::session::WorkflowRunResult> {
        super::native_workflow::run(self, policy, workflow, inputs).await
    }

    /// Export a native workflow result using the shared bounded checkpoint
    /// schema.
    #[cfg(feature = "native-engine")]
    pub async fn native_export_workflow_checkpoint(
        &self,
        workflow: &super::session::WorkflowDefinition,
        result: &super::session::WorkflowRunResult,
    ) -> BrowserResult<super::session::WorkflowCheckpoint> {
        super::native_workflow::export_checkpoint(self, workflow, result).await
    }

    /// Reconcile and resume only the safe pending suffix of a native
    /// workflow checkpoint.
    #[cfg(feature = "native-engine")]
    pub async fn native_resume_workflow(
        &self,
        policy: &super::policy::BrowserPolicy,
        workflow: &super::session::WorkflowDefinition,
        inputs: &std::collections::BTreeMap<String, serde_json::Value>,
        checkpoint: &super::session::WorkflowCheckpoint,
    ) -> BrowserResult<super::session::WorkflowRunResult> {
        super::native_workflow::resume(self, policy, workflow, inputs, checkpoint).await
    }

    /// Resolve native candidates through the same pure intent resolver used by
    /// the Chromium session, backed by one current native observation.
    #[cfg(feature = "native-engine")]
    pub async fn native_find_target(
        &self,
        request: &super::session::SemanticIntentRequest,
    ) -> BrowserResult<FindTargetResult> {
        let observation = self.native_semantic_observation().await?;
        let result = super::session::resolve_intent(request, &observation);
        let ambiguity = match result.resolution {
            super::session::SemanticResolution::Exact
            | super::session::SemanticResolution::UniqueHighConfidence
            | super::session::SemanticResolution::UniqueLowConfidence => "none",
            super::session::SemanticResolution::Ambiguous => "ambiguous",
            super::session::SemanticResolution::NotFound => "not_found",
            super::session::SemanticResolution::StaleRevision => "stale_revision",
            super::session::SemanticResolution::PolicyRejected => "policy_rejected",
            super::session::SemanticResolution::UnsupportedIntent => "unsupported_intent",
        };
        Ok(FindTargetResult {
            normalized_intent: result.normalized_intent,
            revision: result.revision,
            candidates: result.candidates,
            ambiguity: ambiguity.into(),
            suggested_constraints: result.suggested_constraints,
        })
    }

    /// Execute one caller-selected native intent and optionally verify its
    /// postcondition without creating a Chromium session.
    #[cfg(feature = "native-engine")]
    pub async fn native_act_and_verify(
        &self,
        execution: &SemanticIntentExecutionRequest,
        predicate: Option<VerificationPredicate>,
        timeout: Duration,
    ) -> BrowserResult<ActAndVerifyResult> {
        validate_native_deadline(timeout)?;
        execution.validate()?;

        let execution_result = {
            let _operation = self.operation_lock.lock().await;
            let observation = self.native_semantic_observation_unlocked().await?;
            let resolution = super::session::resolve_intent(&execution.request, &observation);
            let resolution_id = super::session::intent_resolution_id(
                &execution.request,
                resolution.revision.unwrap_or(observation.revision),
                &execution.candidate_id,
            )?;
            let candidate = resolution
                .candidates
                .iter()
                .find(|candidate| candidate.id == execution.candidate_id);
            let eligible = candidate.is_some()
                && match resolution.policy_decision {
                    IntentPolicyDecision::Allowed => {
                        resolution.selected_candidate.as_deref() == Some(&execution.candidate_id)
                    }
                    IntentPolicyDecision::ConfirmationRequired => !matches!(
                        resolution.resolution,
                        SemanticResolution::NotFound
                            | SemanticResolution::StaleRevision
                            | SemanticResolution::PolicyRejected
                            | SemanticResolution::UnsupportedIntent
                    ),
                    IntentPolicyDecision::ReportOnly | IntentPolicyDecision::Rejected => false,
                };

            let Some(candidate) = candidate else {
                return Ok(native_not_executed_result(
                    resolution,
                    resolution_id,
                    execution.candidate_id.clone(),
                    "selected candidate is not present in the fresh resolution",
                ));
            };
            if !eligible {
                let reason = resolution
                    .reason
                    .as_deref()
                    .unwrap_or("resolution policy did not authorize this candidate")
                    .to_string();
                return Ok(native_not_executed_result(
                    resolution,
                    resolution_id,
                    execution.candidate_id.clone(),
                    &reason,
                ));
            }

            let candidate_reference = candidate.reference.clone();
            let candidate_name = candidate.name.clone();
            let candidate_frame_id = candidate.frame_id.clone();
            let (action_kind, action) = match execution.request.action {
                SemanticIntentAction::Click
                | SemanticIntentAction::Submit
                | SemanticIntentAction::Open
                | SemanticIntentAction::Close
                | SemanticIntentAction::Search
                | SemanticIntentAction::Filter
                | SemanticIntentAction::Sort
                | SemanticIntentAction::Paginate
                | SemanticIntentAction::Expand
                | SemanticIntentAction::Collapse => (
                    ActionKind::Click,
                    SemanticAction::Click {
                        target: candidate_reference.clone(),
                    },
                ),
                SemanticIntentAction::Type => (
                    ActionKind::Type,
                    SemanticAction::Type {
                        target: candidate_reference.clone(),
                        text: execution
                            .value
                            .as_deref()
                            .ok_or("type requires a value")?
                            .to_string(),
                    },
                ),
                SemanticIntentAction::Clear => (
                    ActionKind::Clear,
                    SemanticAction::Clear {
                        target: candidate_reference.clone(),
                    },
                ),
                SemanticIntentAction::Check => (
                    ActionKind::Check,
                    SemanticAction::Check {
                        target: candidate_reference.clone(),
                    },
                ),
                SemanticIntentAction::Uncheck => (
                    ActionKind::Uncheck,
                    SemanticAction::Uncheck {
                        target: candidate_reference.clone(),
                    },
                ),
                SemanticIntentAction::Select => (
                    ActionKind::Select,
                    SemanticAction::Select {
                        target: candidate_reference.clone(),
                        value: execution
                            .value
                            .as_deref()
                            .ok_or("select requires a value")?
                            .to_string(),
                    },
                ),
                SemanticIntentAction::Toggle
                | SemanticIntentAction::Download
                | SemanticIntentAction::Upload
                | SemanticIntentAction::Inspect
                | SemanticIntentAction::Extract => {
                    unreachable!("validated native intent action")
                }
            };

            let action_result = match (&self.backend, candidate_frame_id.as_deref()) {
                (BackendStartup::Native(backend), Some(frame_id)) => {
                    backend
                        .action_in_frame(&observation.route.target_id, frame_id, action)
                        .await?
                }
                _ => self.action_unlocked(action).await?,
            };
            if !action_result.accepted {
                return Ok(ActAndVerifyResult {
                    status: "not_executed".into(),
                    phase: "dispatch".into(),
                    mutation_possible: false,
                    execution: SemanticIntentExecutionResult {
                        resolution_id,
                        candidate_id: execution.candidate_id.clone(),
                        status: SemanticIntentExecutionStatus::NotExecuted,
                        resolution,
                        action: None,
                        execution_id: None,
                        reason: Some("native action was not accepted".into()),
                    },
                    verification: None,
                    retry: RetryGuidance {
                        classification: RetryClassification::SafeAfterReobserve,
                        recommended_operation: "find_target".into(),
                    },
                });
            }

            let after = self.native_semantic_observation_unlocked().await?;
            let current_revision = after.revision;
            let action_outcome = ActionOutcome {
                status: ActionStatus::Succeeded,
                action: action_kind,
                execution_id: self.next_native_execution_id(),
                target: Some(ActionTarget {
                    label: candidate_name,
                    reference: Some(candidate_reference),
                }),
                revision: current_revision,
                previous_revision: observation.revision,
                current_revision,
                target_id: after.route.target_id.clone(),
                frame_id: candidate_frame_id.unwrap_or(after.route.frame_id.clone()),
                verification: ActionVerificationEvidence {
                    revision_delta: current_revision.saturating_sub(observation.revision),
                    url_changed: observation.page.url != after.page.url,
                    title_changed: observation.page.title != after.page.title,
                    target_changed: observation.page.target_id != after.page.target_id,
                    frame_changed: observation.page.frame_id != after.page.frame_id,
                    ..ActionVerificationEvidence::default()
                },
                evidence: None,
            };
            let execution_id = action_outcome.execution_id.clone();
            SemanticIntentExecutionResult {
                resolution_id,
                candidate_id: execution.candidate_id.clone(),
                status: SemanticIntentExecutionStatus::Executed,
                resolution,
                action: Some(action_outcome),
                execution_id: Some(execution_id),
                reason: None,
            }
        };

        let Some(predicate) = predicate else {
            return Ok(ActAndVerifyResult {
                status: "dispatched_unverified".into(),
                phase: "post_dispatch".into(),
                mutation_possible: true,
                execution: execution_result,
                verification: None,
                retry: RetryGuidance {
                    classification: RetryClassification::RequiresUserDecision,
                    recommended_operation: "inspect_page".into(),
                },
            });
        };

        match self.native_verify(predicate, timeout).await {
            Ok(verification) => Ok(ActAndVerifyResult {
                status: "verified".into(),
                phase: "verification".into(),
                mutation_possible: false,
                execution: execution_result,
                verification: Some(verification),
                retry: RetryGuidance {
                    classification: RetryClassification::SafeImmediate,
                    recommended_operation: "inspect_page".into(),
                },
            }),
            Err(_error) => Ok(ActAndVerifyResult {
                status: "indeterminate".into(),
                phase: "verification".into(),
                mutation_possible: true,
                execution: execution_result,
                verification: None,
                retry: RetryGuidance {
                    classification: RetryClassification::UnsafeUntilReconciled,
                    recommended_operation: "recover_run".into(),
                },
            }),
        }
    }

    /// Evaluate one verification predicate once for workflow branches and
    /// retry effect markers. Unlike `native_verify`, this never waits or
    /// converts a false result into a timeout error.
    #[cfg(feature = "native-engine")]
    pub(crate) async fn native_verify_once(
        &self,
        predicate: &VerificationPredicate,
    ) -> BrowserResult<(bool, String)> {
        predicate.validate(0)?;
        self.native_check_verification_predicate(predicate).await
    }

    #[cfg(feature = "native-engine")]
    async fn native_semantic_observation(
        &self,
    ) -> BrowserResult<super::session::SemanticObservation> {
        self.native_semantic_observe(super::session::SemanticObservationLevel::Structured)
            .await
    }

    #[cfg(feature = "native-engine")]
    async fn native_semantic_observation_unlocked(
        &self,
    ) -> BrowserResult<super::session::SemanticObservation> {
        self.native_semantic_observation_level_unlocked(
            super::session::SemanticObservationLevel::Structured,
        )
        .await
    }

    #[cfg(feature = "native-engine")]
    async fn native_semantic_observation_level_unlocked(
        &self,
        level: super::session::SemanticObservationLevel,
    ) -> BrowserResult<super::session::SemanticObservation> {
        let frames = self.native_inspection_snapshots_unlocked().await?;
        build_native_semantic_observation(&frames, level)
    }

    #[cfg(feature = "native-engine")]
    async fn native_inspection_snapshots_unlocked(
        &self,
    ) -> BrowserResult<Vec<NativeFrameInspectionSnapshot>> {
        match &self.backend {
            BackendStartup::Native(backend) => Ok(backend.inspection_snapshots().await?),
            _ => Err("native semantic inspection is only available on the native runtime".into()),
        }
    }

    /// Evaluate a bounded native verification predicate until it is
    /// satisfied or the caller's deadline expires.
    #[cfg(feature = "native-engine")]
    pub async fn native_verify(
        &self,
        predicate: VerificationPredicate,
        deadline: Duration,
    ) -> BrowserResult<VerificationOutcome> {
        validate_native_deadline(deadline)?;
        predicate.validate(0)?;
        let started = tokio::time::Instant::now();
        let expires = started + deadline;
        loop {
            let (matched, observed) = self.native_check_verification_predicate(&predicate).await?;
            if matched {
                return Ok(VerificationOutcome {
                    status: "satisfied",
                    predicate,
                    elapsed_ms: started.elapsed().as_millis() as u64,
                    state: observed,
                });
            }
            if tokio::time::Instant::now() >= expires {
                let revision = self.native_semantic_observation().await?.revision;
                return Err(Box::new(ActionVerificationError {
                    kind: ActionFailureKind::VerificationFailed,
                    action: ActionKind::Click,
                    phase: ActionFailurePhase::Verification,
                    recovery_strategy: RecoveryStrategy::Report,
                    execution_id: None,
                    target: None,
                    revision,
                    reason: format!("verification predicate not satisfied: {observed}"),
                }));
            }
            tokio::time::sleep(
                expires
                    .saturating_duration_since(tokio::time::Instant::now())
                    .min(NATIVE_WAIT_POLL_INTERVAL),
            )
            .await;
        }
    }

    /// Wait for a bounded native lifecycle, URL, text, target, region, or
    /// JavaScript condition.
    #[cfg(feature = "native-engine")]
    pub async fn native_wait(
        &self,
        condition: WaitCondition,
        deadline: Duration,
    ) -> BrowserResult<WaitOutcome> {
        validate_native_deadline(deadline)?;
        condition.validate()?;
        match condition {
            WaitCondition::JavaScript(expression) => {
                self.native_wait_javascript(expression, deadline).await
            }
            condition => self.native_wait_non_javascript(condition, deadline).await,
        }
    }

    #[cfg(feature = "native-engine")]
    async fn native_wait_javascript(
        &self,
        expression: String,
        deadline: Duration,
    ) -> BrowserResult<WaitOutcome> {
        let description = WaitCondition::JavaScript(expression.clone()).description();
        let started = tokio::time::Instant::now();
        let expires = started + deadline;
        loop {
            let result = self.script(&expression).await?;
            let matched = result
                .value
                .as_bool()
                .ok_or("native wait JavaScript predicate must return a boolean")?;
            let state = matched.to_string();
            if matched {
                let observation = self.native_semantic_observation().await?;
                return Ok(WaitOutcome {
                    condition: description,
                    elapsed_ms: started.elapsed().as_millis() as u64,
                    last_state: state,
                    target_id: observation.route.target_id,
                    frame_id: observation.route.frame_id,
                });
            }
            if tokio::time::Instant::now() >= expires {
                let observation = self.native_semantic_observation().await?;
                return Err(Box::new(WaitTimeout {
                    condition: description,
                    deadline_ms: deadline.as_millis() as u64,
                    last_state: state,
                    observed_page: Some(super::session::PageInfo {
                        url: observation.page.url,
                        title: observation.page.title,
                        ready_state: "complete".into(),
                        target_id: observation.route.target_id,
                        frame_id: observation.route.frame_id,
                    }),
                    reason: "deadline_exceeded",
                }));
            }
            tokio::time::sleep(
                expires
                    .saturating_duration_since(tokio::time::Instant::now())
                    .min(NATIVE_WAIT_POLL_INTERVAL),
            )
            .await;
        }
    }

    #[cfg(feature = "native-engine")]
    async fn native_wait_non_javascript(
        &self,
        condition: WaitCondition,
        deadline: Duration,
    ) -> BrowserResult<WaitOutcome> {
        let description = condition.description();
        let started = tokio::time::Instant::now();
        let expires = started + deadline;
        let mut previous_geometry = None;
        loop {
            let (matched, state, geometry) = match &condition {
                WaitCondition::NetworkQuiet(duration) => {
                    let (matched, state) = self.native_network_quiet(*duration).await?;
                    (matched, state, None)
                }
                _ => {
                    self.native_check_wait_condition(&condition, previous_geometry.as_deref())
                        .await?
                }
            };
            if matched {
                let observation = self.native_semantic_observation().await?;
                return Ok(WaitOutcome {
                    condition: description,
                    elapsed_ms: started.elapsed().as_millis() as u64,
                    last_state: state,
                    target_id: observation.route.target_id,
                    frame_id: observation.route.frame_id,
                });
            }
            if tokio::time::Instant::now() >= expires {
                let observation = self.native_semantic_observation().await?;
                return Err(Box::new(WaitTimeout {
                    condition: description,
                    deadline_ms: deadline.as_millis() as u64,
                    last_state: state,
                    observed_page: Some(super::session::PageInfo {
                        url: observation.page.url,
                        title: observation.page.title,
                        ready_state: "complete".into(),
                        target_id: observation.route.target_id,
                        frame_id: observation.route.frame_id,
                    }),
                    reason: "deadline_exceeded",
                }));
            }
            previous_geometry = geometry;
            tokio::time::sleep(
                expires
                    .saturating_duration_since(tokio::time::Instant::now())
                    .min(NATIVE_WAIT_POLL_INTERVAL),
            )
            .await;
        }
    }

    #[cfg(feature = "native-engine")]
    async fn native_check_verification_predicate(
        &self,
        predicate: &VerificationPredicate,
    ) -> BrowserResult<(bool, String)> {
        match predicate {
            VerificationPredicate::UrlEquals { value } => {
                let observation = self.native_semantic_observation().await?;
                Ok((
                    observation.page.url == *value,
                    format!("url={}", observation.page.url),
                ))
            }
            VerificationPredicate::TitleContains { value } => {
                let observation = self.native_semantic_observation().await?;
                Ok((
                    observation.page.title.contains(value),
                    format!("title={}", observation.page.title),
                ))
            }
            VerificationPredicate::Visible { visible } => {
                let target = self
                    .native_preflight_target(visible, NativePreflightAction::Click)
                    .await?;
                let matched = target.node.as_ref().is_some_and(|node| !node.hidden)
                    && target.geometry.is_some();
                Ok((matched, format!("visible={matched}")))
            }
            VerificationPredicate::TextContains { value } => {
                let observation = self.native_semantic_observation().await?;
                let text = observation.text.unwrap_or_default();
                let matched = text.contains(value);
                Ok((matched, format!("textContains={matched}")))
            }
            VerificationPredicate::RevisionEquals { value } => {
                let observation = self.native_semantic_observation().await?;
                Ok((
                    observation.revision == *value,
                    format!("revision={}", observation.revision),
                ))
            }
            VerificationPredicate::DialogOpen { value } => {
                let open = self.native_pending_dialog().await?.is_some();
                Ok((open == *value, format!("dialogOpen={open}")))
            }
            VerificationPredicate::DownloadStarted { value } => {
                let started = self.native_completed_download_count().await? > 0;
                Ok((started == *value, format!("downloadStarted={started}")))
            }
            VerificationPredicate::All { all } => {
                let mut states = Vec::with_capacity(all.len());
                let mut matched = true;
                for predicate in all {
                    let (child_matched, state) =
                        Box::pin(self.native_check_verification_predicate(predicate)).await?;
                    matched &= child_matched;
                    states.push(state);
                }
                Ok((matched, format!("all=[{}]", states.join(","))))
            }
            VerificationPredicate::Any { any } => {
                let mut states = Vec::with_capacity(any.len());
                let mut matched = false;
                for predicate in any {
                    let (child_matched, state) =
                        Box::pin(self.native_check_verification_predicate(predicate)).await?;
                    matched |= child_matched;
                    states.push(state);
                }
                Ok((matched, format!("any=[{}]", states.join(","))))
            }
            VerificationPredicate::Not { not } => {
                let (matched, state) =
                    Box::pin(self.native_check_verification_predicate(not)).await?;
                Ok((!matched, format!("not({state})")))
            }
            VerificationPredicate::PopupOpened { value } => {
                let opened = self.native_list_targets().await?.len() > 1;
                Ok((opened == *value, format!("popupOpened={opened}")))
            }
        }
    }

    #[cfg(feature = "native-engine")]
    async fn native_check_wait_condition(
        &self,
        condition: &WaitCondition,
        previous_geometry: Option<&str>,
    ) -> BrowserResult<(bool, String, Option<String>)> {
        match condition {
            WaitCondition::Lifecycle(expected) => {
                let _observation = self.native_semantic_observation().await?;
                let matched = matches!(expected.as_str(), "interactive" | "complete");
                Ok((matched, "lifecycle=complete".into(), None))
            }
            WaitCondition::UrlExact(expected) => {
                let observation = self.native_semantic_observation().await?;
                Ok((
                    observation.page.url == *expected,
                    format!("url={}", observation.page.url),
                    None,
                ))
            }
            WaitCondition::UrlPrefix(prefix) => {
                let observation = self.native_semantic_observation().await?;
                Ok((
                    observation.page.url.starts_with(prefix),
                    format!("url={}", observation.page.url),
                    None,
                ))
            }
            WaitCondition::Text(expected) => {
                let observation = self.native_semantic_observation().await?;
                let text = observation.text.unwrap_or_default();
                let matched = text.contains(expected);
                Ok((matched, format!("present={matched}"), None))
            }
            WaitCondition::SemanticRegion(region_id) => {
                let observation = self.native_semantic_observation().await?;
                let matched = observation
                    .regions
                    .iter()
                    .find(|region| region.id == *region_id)
                    .is_some_and(|region| !region.targets.is_empty());
                Ok((matched, format!("region={region_id};ready={matched}"), None))
            }
            WaitCondition::JavaScript(expression) => {
                let result = self.script(expression).await?;
                let matched = result
                    .value
                    .as_bool()
                    .ok_or("native wait JavaScript predicate must return a boolean")?;
                Ok((matched, matched.to_string(), None))
            }
            WaitCondition::TargetAttached(target)
            | WaitCondition::TargetVisible(target)
            | WaitCondition::TargetHidden(target)
            | WaitCondition::TargetEnabled(target)
            | WaitCondition::TargetStable(target) => {
                let result = self
                    .native_preflight_target(target, NativePreflightAction::Click)
                    .await?;
                let present = result.unique;
                let visible = result.node.as_ref().is_some_and(|node| !node.hidden)
                    && result.geometry.is_some();
                let enabled = visible && result.node.as_ref().is_some_and(|node| !node.disabled);
                let geometry = result
                    .geometry
                    .as_ref()
                    .map(serde_json::to_string)
                    .transpose()?;
                let matched = match condition {
                    WaitCondition::TargetAttached(_) => present,
                    WaitCondition::TargetVisible(_) => visible,
                    WaitCondition::TargetHidden(_) => !present || !visible,
                    WaitCondition::TargetEnabled(_) => enabled,
                    WaitCondition::TargetStable(_) => {
                        visible
                            && geometry
                                .as_deref()
                                .is_some_and(|value| previous_geometry == Some(value))
                    }
                    _ => unreachable!(),
                };
                Ok((
                    matched,
                    format!("present={present};visible={visible};enabled={enabled}"),
                    geometry,
                ))
            }
            WaitCondition::NetworkQuiet(_) => unreachable!("handled by native_wait"),
        }
    }

    pub async fn close(self) -> BrowserResult<()> {
        Ok(BrowserBackendDispatcher::new(&self.backend).close().await?)
    }

    #[cfg(feature = "native-engine")]
    pub(crate) fn next_native_execution_id(&self) -> String {
        format!(
            "act_native_{}",
            self.next_execution_id.fetch_add(1, Ordering::Relaxed)
        )
    }

    async fn require_current_revision(&self, expected_revision: u64) -> BrowserResult<()> {
        let evidence = self.evidence(EvidenceLevel::Compact).await?;
        if evidence.revision == expected_revision {
            return Ok(());
        }
        Err(Box::new(ActionContractError::stale_revision(
            expected_revision,
            evidence.revision,
        )))
    }

    async fn active_context_id(&self) -> BrowserResult<String> {
        Ok(self
            .contexts()
            .await?
            .into_iter()
            .find(|context| context.active)
            .map(|context| context.context_id)
            .ok_or("alternative runtime returned no active context")?)
    }
}

#[cfg(feature = "native-engine")]
fn native_observation_incomplete(
    observation: &SemanticObservation,
) -> Vec<ObservationIncompleteReason> {
    let mut incomplete = Vec::new();
    if observation.limits.text_truncated {
        incomplete.push(ObservationIncompleteReason::VisibleText);
    }
    if observation.limits.omitted_targets > 0 {
        incomplete.push(ObservationIncompleteReason::Control);
    }
    if observation.limits.omitted_regions > 0 || observation.limits.truncated {
        incomplete.push(ObservationIncompleteReason::BoundaryScan);
    }
    incomplete
}

#[cfg(feature = "native-engine")]
fn bounded_checkpoint_text(value: &str, max_bytes: usize) -> String {
    if value.len() <= max_bytes {
        return value.to_owned();
    }
    let mut end = max_bytes;
    while end > 0 && !value.is_char_boundary(end) {
        end -= 1;
    }
    value[..end].to_owned()
}

#[cfg(feature = "native-engine")]
fn validate_native_deadline(deadline: Duration) -> BrowserResult<()> {
    if deadline.is_zero() || deadline > NATIVE_MAX_WAIT_DEADLINE {
        return Err("native wait deadline must be between 1 ms and 300000 ms".into());
    }
    Ok(())
}

#[cfg(feature = "native-engine")]
fn native_not_executed_result(
    resolution: SemanticIntentResult,
    resolution_id: String,
    candidate_id: String,
    reason: &str,
) -> ActAndVerifyResult {
    ActAndVerifyResult {
        status: "not_executed".into(),
        phase: "preflight".into(),
        mutation_possible: false,
        execution: SemanticIntentExecutionResult {
            resolution_id,
            candidate_id,
            status: SemanticIntentExecutionStatus::NotExecuted,
            resolution,
            action: None,
            execution_id: None,
            reason: Some(reason.into()),
        },
        verification: None,
        retry: RetryGuidance {
            classification: RetryClassification::SafeAfterReobserve,
            recommended_operation: "find_target".into(),
        },
    }
}

#[cfg(feature = "native-engine")]
fn build_native_semantic_observation(
    frames: &[NativeFrameInspectionSnapshot],
    level: super::session::SemanticObservationLevel,
) -> BrowserResult<super::session::SemanticObservation> {
    let native = frames
        .first()
        .ok_or("native semantic inspection returned no active frame")?;
    let native = &native.inspection;
    let frame_id = frames[0].frame_id.clone();
    let route = super::session::SemanticRouteIdentity {
        target_id: native.context_id.clone(),
        frame_id: frame_id.clone(),
        url: native.snapshot.url.clone(),
    };
    let revision = native_aggregate_revision(frames);
    let mut omitted_targets = 0usize;
    let regions = frames
        .iter()
        .enumerate()
        .map(|(index, frame)| {
            let available = frame
                .inspection
                .nodes
                .iter()
                .filter(|node| native_semantic_node_is_interactive(node))
                .count();
            let targets = if native_level_includes_targets(level) {
                frame
                    .inspection
                    .nodes
                    .iter()
                    .filter(|node| native_semantic_node_is_interactive(node))
                    .take(NATIVE_SEMANTIC_TARGET_LIMIT)
                    .map(|node| native_semantic_target(node, &frame.frame_id))
                    .collect::<Vec<_>>()
            } else {
                Vec::new()
            };
            omitted_targets =
                omitted_targets.saturating_add(available.saturating_sub(targets.len()));
            let region_id = if index == 0 {
                "region_main".to_owned()
            } else {
                format!("region_frame_{index}")
            };
            super::session::SemanticRegion {
                id: region_id.clone(),
                kind: super::session::SemanticRegionKind::Main,
                label: if index == 0 {
                    "Main content".into()
                } else {
                    format!("Embedded frame {index}")
                },
                interactive_count: available,
                item_count: Some(available),
                confidence: super::session::SemanticConfidence::High,
                structured_records: Vec::new(),
                evidence: vec!["native frame semantic projection".into()],
                targets,
                expansion: Some(super::session::SemanticExpansionHandle {
                    region_id,
                    revision,
                    route: route.clone(),
                }),
            }
        })
        .collect::<Vec<_>>();
    let (text, text_truncated) = bounded_native_semantic_text(frames);
    let viewport = native.layout.viewport;
    let scroll_offset = native.layout.scroll_offset;
    let observation = super::session::SemanticObservation {
        schema_version: super::session::SEMANTIC_OBSERVATION_SCHEMA_VERSION,
        revision,
        level,
        route: route.clone(),
        page: super::session::SemanticPage {
            kind: super::session::SemanticPageKind::Generic,
            title: native.snapshot.title.clone(),
            url: native.snapshot.url.clone(),
            target_id: route.target_id.clone(),
            frame_id: route.frame_id.clone(),
            confidence: super::session::SemanticConfidence::Medium,
            evidence: vec!["native page snapshot".into()],
        },
        regions,
        text: native_level_includes_text(level).then_some(text.clone()),
        accessibility: native_level_includes_accessibility(level)
            .then(|| native_accessibility_nodes(frames)),
        raw_accessibility: native_level_includes_raw_accessibility(level)
            .then(|| native_accessibility_nodes(frames)),
        changes: None,
        limits: super::session::SemanticObservationLimits {
            truncated: frames.iter().any(|frame| {
                frame.inspection.snapshot.title_truncated
                    || frame.inspection.snapshot.text_truncated
            }) || omitted_targets > 0
                || (native_level_includes_text(level) && text_truncated)
                || (native_level_includes_accessibility(level)
                    && native_accessibility_node_count(frames)
                        > NATIVE_SEMANTIC_ACCESSIBILITY_LIMIT),
            omitted_regions: 0,
            omitted_targets,
            omitted_structured_records: 0,
            structured_bytes: native_level_includes_text(level).then_some(0),
            omitted_bytes: None,
            text_bytes: native_level_includes_text(level).then_some(text.len()),
            text_truncated: native_level_includes_text(level) && text_truncated,
            viewport: Some(super::session::SemanticViewport {
                scroll_x: f64::from(scroll_offset.x),
                scroll_y: f64::from(scroll_offset.y),
                width: f64::from(viewport.width),
                height: f64::from(viewport.height),
                document_width: f64::from(native.layout.content_width),
                document_height: f64::from(native.layout.content_height),
            }),
        },
    };
    observation.validate()?;
    Ok(observation)
}

#[cfg(feature = "native-engine")]
const fn native_level_includes_targets(level: super::session::SemanticObservationLevel) -> bool {
    matches!(
        level,
        super::session::SemanticObservationLevel::Interactive
            | super::session::SemanticObservationLevel::Structured
            | super::session::SemanticObservationLevel::Detailed
            | super::session::SemanticObservationLevel::Raw
    )
}

#[cfg(feature = "native-engine")]
const fn native_level_includes_text(level: super::session::SemanticObservationLevel) -> bool {
    matches!(
        level,
        super::session::SemanticObservationLevel::Structured
            | super::session::SemanticObservationLevel::Detailed
            | super::session::SemanticObservationLevel::Raw
    )
}

#[cfg(feature = "native-engine")]
const fn native_level_includes_accessibility(
    level: super::session::SemanticObservationLevel,
) -> bool {
    matches!(
        level,
        super::session::SemanticObservationLevel::Detailed
            | super::session::SemanticObservationLevel::Raw
    )
}

#[cfg(feature = "native-engine")]
const fn native_level_includes_raw_accessibility(
    level: super::session::SemanticObservationLevel,
) -> bool {
    matches!(level, super::session::SemanticObservationLevel::Raw)
}

#[cfg(feature = "native-engine")]
fn native_accessibility_nodes(
    frames: &[NativeFrameInspectionSnapshot],
) -> Vec<super::session::SemanticAccessibilityNode> {
    frames
        .iter()
        .flat_map(|frame| frame.inspection.nodes.iter())
        .take(NATIVE_SEMANTIC_ACCESSIBILITY_LIMIT)
        .map(|node| super::session::SemanticAccessibilityNode {
            role: node.role.clone(),
            name: node.name.clone(),
            children: Vec::new(),
            interactive: native_semantic_node_is_interactive(node),
        })
        .collect()
}

#[cfg(feature = "native-engine")]
fn native_semantic_node_is_interactive(node: &super::native_engine::NativeSemanticNode) -> bool {
    matches!(
        node.role.as_str(),
        "button"
            | "checkbox"
            | "combobox"
            | "file"
            | "link"
            | "listbox"
            | "menuitem"
            | "option"
            | "radio"
            | "slider"
            | "spinbutton"
            | "switch"
            | "tab"
            | "textbox"
    )
}

#[cfg(feature = "native-engine")]
fn native_accessibility_node_count(frames: &[NativeFrameInspectionSnapshot]) -> usize {
    frames
        .iter()
        .map(|frame| frame.inspection.nodes.len())
        .sum()
}

#[cfg(feature = "native-engine")]
fn native_semantic_target(
    node: &super::native_engine::NativeSemanticNode,
    frame_id: &str,
) -> super::session::SemanticTarget {
    super::session::SemanticTarget {
        reference: node.reference.clone(),
        frame_id: Some(frame_id.to_owned()),
        role: node.role.clone(),
        name: node.name.clone(),
        input_type: node.input_type.clone(),
        disabled: Some(node.disabled),
        read_only: Some(node.read_only),
        required: Some(node.required),
        checked: node.checked,
        empty: node.empty,
    }
}

#[cfg(feature = "native-engine")]
fn native_aggregate_revision(frames: &[NativeFrameInspectionSnapshot]) -> u64 {
    if frames.len() == 1 {
        return frames[0].inspection.snapshot.revision;
    }
    let mut hash = 0xcbf29ce484222325_u64;
    for frame in frames {
        for byte in frame.frame_id.as_bytes() {
            hash ^= u64::from(*byte);
            hash = hash.wrapping_mul(0x100000001b3);
        }
        hash ^= 0xff;
        hash = hash.wrapping_mul(0x100000001b3);
        for byte in frame.inspection.snapshot.revision.to_le_bytes() {
            hash ^= u64::from(byte);
            hash = hash.wrapping_mul(0x100000001b3);
        }
    }
    if hash != 0 { hash } else { 1 }
}

#[cfg(feature = "native-engine")]
fn bounded_native_semantic_text(frames: &[NativeFrameInspectionSnapshot]) -> (String, bool) {
    let mut output = String::new();
    let mut truncated = false;
    for frame in frames {
        let value = frame.inspection.snapshot.visible_text.as_str();
        if value.is_empty() {
            continue;
        }
        if !output.is_empty() {
            if output.len().saturating_add(1) > NATIVE_SEMANTIC_TEXT_LIMIT {
                truncated = true;
                break;
            }
            output.push('\n');
        }
        let available = NATIVE_SEMANTIC_TEXT_LIMIT.saturating_sub(output.len());
        if value.len() <= available {
            output.push_str(value);
            continue;
        }
        let mut end = available.min(value.len());
        while end > 0 && !value.is_char_boundary(end) {
            end -= 1;
        }
        output.push_str(&value[..end]);
        truncated = true;
        break;
    }
    (output, truncated)
}
