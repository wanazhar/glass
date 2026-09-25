//! Native-first runtime sessions and explicit alternative-runtime adapters.
//!
//! `BrowserRuntimeSession` is the runtime-neutral semantic session used by
//! the Glass-owned native runtime and external protocol adapters. The public
//! `BrowserSession` name aliases it; the previous Chrome/CDP API is
//! `crate::browser::session::CdpBrowserSession`.

use super::backend_factory::{BackendFactory, BackendStartup};
use super::bidi_backend::BidiBackendConfig;
#[cfg(feature = "native-engine")]
use super::native_backend::{NativeEngineBackend, NativeFrameInspectionSnapshot};
#[cfg(feature = "native-engine")]
use super::native_engine::{
    NativeEngineConfig, NativeFile, NativePreflightAction, NativeTargetPreflight, Viewport,
};
#[cfg(feature = "native-engine")]
use super::policy::BrowserPolicy;
use crate::browser_backend::{
    ActionRequest, ActionResult, BackendProfile, BrowserBackendDispatcher, BrowserBackendError,
    BrowsingContext, CaptureFormat, CaptureRequest, CaptureResult, ContextRequest, EffectsRequest,
    EffectsResult, EvidenceLevel, EvidenceRequest, EvidenceResult, NavigationRequest,
    NavigationResult, ScriptRequest, ScriptResult, SemanticAction, StorageRequest, StorageResult,
};
use clap::ValueEnum;
use serde::{Deserialize, Serialize};

#[cfg(feature = "native-engine")]
use super::session::{
    ActAndVerifyResult, ActionFailureKind, ActionFailurePhase, ActionKind, ActionOutcome,
    ActionStatus, ActionTarget, ActionVerificationError, ActionVerificationEvidence,
    BootstrapObservation, CandidateSummary, CheckpointError, CheckpointObservation,
    CheckpointTopology, CheckpointV1, ConsoleEvidence, Cookie, DeltaControl, DiagnosticReport,
    DownloadOutcome, FillFieldResult, FillFormOutcome, FindTargetResult, FrameInfo, GeoLocation,
    InspectPageResult, IntentPolicyDecision, KnowledgeAssessmentStatus, KnowledgeLookupContext,
    KnowledgeLookupOptions, KnowledgeStore, LifecycleDiagnostics, MutationSummary,
    NavigationControlOutcome, NetworkConditions, ObservationBoundarySummary, ObservationDelta,
    ObservationIncompleteReason, PageInfo, PageTargetInfo, PendingDialog, PopupClickOutcome,
    PopupVerificationEvidence, ReconciliationOptions, ReconciliationOutcome, ReconciliationStatus,
    RecoveryStrategy, ReferenceLostReason, ReferenceMapping, ReferenceMatch, SemanticIntentAction,
    SemanticIntentExecutionRequest, SemanticIntentExecutionResult, SemanticIntentExecutionStatus,
    SemanticIntentResult, SemanticObservation, SemanticObservationLevel, SemanticResolution,
    SemanticTarget, VerificationOutcome, VerificationPredicate, ViewportState, VisualCapture,
    VisualCaptureOptions, WaitCondition, WaitOutcome, WaitTimeout,
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
use tokio::sync::Notify;

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
#[cfg(feature = "native-engine")]
const NATIVE_RECONCILIATION_CANDIDATE_LIMIT: usize = 8;

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
    native_navigation_changed: Notify,
    #[cfg(feature = "native-engine")]
    next_execution_id: AtomicU64,
    #[cfg(feature = "native-engine")]
    native_observation_cache: Mutex<Option<SemanticObservation>>,
    #[cfg(feature = "native-engine")]
    native_clipboard: Mutex<String>,
}

/// Canonical Glass browser session. With the default `native-engine` feature,
/// its `start` constructors select the native backend directly and never
/// fall back to Chromium/CDP.
pub type BrowserSession = BrowserRuntimeSession;

#[cfg(feature = "native-engine")]
struct NativeNavigationControlGuard<'a> {
    backend: &'a NativeEngineBackend,
    id: u64,
    changed: &'a Notify,
}

#[cfg(feature = "native-engine")]
impl Drop for NativeNavigationControlGuard<'_> {
    fn drop(&mut self) {
        self.backend.finish_navigation_control(self.id);
        self.changed.notify_waiters();
    }
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
                return Err("Chromium must be opened through CdpBrowserSession".into());
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
            native_navigation_changed: Notify::new(),
            #[cfg(feature = "native-engine")]
            next_execution_id: AtomicU64::new(1),
            #[cfg(feature = "native-engine")]
            native_observation_cache: Mutex::new(None),
            #[cfg(feature = "native-engine")]
            native_clipboard: Mutex::new(String::new()),
        };
        BrowserBackendDispatcher::new(&session.backend)
            .initialize()
            .await?;
        Ok(session)
    }

    /// Start the canonical native browser session with explicit configuration.
    ///
    /// This calls the native backend factory directly. It never probes for
    /// Chrome, connects to CDP, or switches to another runtime.
    #[cfg(feature = "native-engine")]
    pub async fn start(config: NativeEngineConfig) -> BrowserResult<Self> {
        Self::connect_native(config).await
    }

    /// Start the canonical native browser session with default configuration.
    #[cfg(feature = "native-engine")]
    pub async fn start_default() -> BrowserResult<Self> {
        Self::start(NativeEngineConfig::default()).await
    }

    /// Construct and initialize the native runtime.
    #[cfg(feature = "native-engine")]
    pub async fn connect_native(config: NativeEngineConfig) -> BrowserResult<Self> {
        let backend = BackendFactory::native(config)?;
        let session = Self {
            runtime: BrowserRuntime::Native,
            backend,
            operation_lock: Mutex::new(()),
            native_navigation_changed: Notify::new(),
            next_execution_id: AtomicU64::new(1),
            native_observation_cache: Mutex::new(None),
            native_clipboard: Mutex::new(String::new()),
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
        #[cfg(feature = "native-engine")]
        let _navigation_control = match &self.backend {
            BackendStartup::Native(backend) if is_http_navigation_url(&url) => {
                let starting_revision = backend.current_revision()?;
                let id = backend.begin_navigation_control(starting_revision)?;
                self.native_navigation_changed.notify_waiters();
                Some(NativeNavigationControlGuard {
                    backend,
                    id,
                    changed: &self.native_navigation_changed,
                })
            }
            _ => None,
        };
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

    /// List page targets owned by the native browser session.
    #[cfg(feature = "native-engine")]
    pub async fn list_targets(&self) -> BrowserResult<Vec<PageTargetInfo>> {
        self.require_native_operation("list_targets")?;
        self.native_list_targets().await
    }

    /// Create a parked native page target without changing active selection.
    #[cfg(feature = "native-engine")]
    pub async fn create_target(&self, url: &str) -> BrowserResult<PageTargetInfo> {
        self.require_native_operation("create_target")?;
        self.native_create_target(url).await
    }

    /// Explicitly select one native page target for subsequent operations.
    #[cfg(feature = "native-engine")]
    pub async fn select_target(&self, target_id: &str) -> BrowserResult<PageTargetInfo> {
        self.require_native_operation("select_target")?;
        self.native_select_target(target_id).await
    }

    /// Close one native page target. Closing the active target clears selection.
    #[cfg(feature = "native-engine")]
    pub async fn close_target(&self, target_id: &str) -> BrowserResult<()> {
        self.require_native_operation("close_target")?;
        self.native_close_target(target_id).await
    }

    /// List frames in the currently selected native page target.
    #[cfg(feature = "native-engine")]
    pub async fn list_frames(&self) -> BrowserResult<Vec<FrameInfo>> {
        self.require_native_operation("list_frames")?;
        self.native_list_frames().await
    }

    /// Explicitly select one frame returned by [`Self::list_frames`].
    #[cfg(feature = "native-engine")]
    pub async fn select_frame(&self, frame_id: &str) -> BrowserResult<FrameInfo> {
        self.require_native_operation("select_frame")?;
        self.native_select_frame(frame_id).await
    }

    /// Traverse to the previous entry in native session history.
    #[cfg(feature = "native-engine")]
    pub async fn go_back(&self) -> BrowserResult<NavigationControlOutcome> {
        self.require_native_operation("go_back")?;
        self.native_navigate_history(super::native_engine::NativeHistoryDirection::Back)
            .await
    }

    /// Traverse backward only when the caller's observation is still current.
    #[cfg(feature = "native-engine")]
    pub async fn go_back_with_revision(
        &self,
        expected_revision: u64,
    ) -> BrowserResult<NavigationControlOutcome> {
        self.require_native_operation("go_back_with_revision")?;
        self.native_navigate_history_with_revision(
            super::native_engine::NativeHistoryDirection::Back,
            expected_revision,
        )
        .await
    }

    /// Traverse to the next entry in native session history.
    #[cfg(feature = "native-engine")]
    pub async fn go_forward(&self) -> BrowserResult<NavigationControlOutcome> {
        self.require_native_operation("go_forward")?;
        self.native_navigate_history(super::native_engine::NativeHistoryDirection::Forward)
            .await
    }

    /// Traverse forward only when the caller's observation is still current.
    #[cfg(feature = "native-engine")]
    pub async fn go_forward_with_revision(
        &self,
        expected_revision: u64,
    ) -> BrowserResult<NavigationControlOutcome> {
        self.require_native_operation("go_forward_with_revision")?;
        self.native_navigate_history_with_revision(
            super::native_engine::NativeHistoryDirection::Forward,
            expected_revision,
        )
        .await
    }

    /// Reload the active native history entry only when its observation is current.
    #[cfg(feature = "native-engine")]
    pub async fn reload_with_revision(
        &self,
        expected_revision: u64,
    ) -> BrowserResult<NavigationControlOutcome> {
        self.require_native_operation("reload_with_revision")?;
        self.native_reload_with_revision(expected_revision).await
    }

    /// Rebuild the native document owner and reload the active URL.
    #[cfg(feature = "native-engine")]
    pub async fn recover(&self) -> BrowserResult<NavigationControlOutcome> {
        self.require_native_operation("recover")?;
        self.native_recover(None).await
    }

    /// Recover the native document only when the caller's observation is current.
    #[cfg(feature = "native-engine")]
    pub async fn recover_with_revision(
        &self,
        expected_revision: u64,
    ) -> BrowserResult<NavigationControlOutcome> {
        self.require_native_operation("recover_with_revision")?;
        self.native_recover(Some(expected_revision)).await
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

    /// Rebuild the native document owner and reload the current URL. An
    /// optional revision guard prevents recovery from racing a newer caller.
    #[cfg(feature = "native-engine")]
    pub async fn native_recover(
        &self,
        expected_revision: Option<u64>,
    ) -> BrowserResult<NavigationControlOutcome> {
        let _operation = self.operation_lock.lock().await;
        if let Some(expected_revision) = expected_revision {
            self.require_current_revision(expected_revision).await?;
        }
        match &self.backend {
            BackendStartup::Native(backend) => Ok(backend.recover().await?),
            _ => Err("native recovery is only available on the native runtime".into()),
        }
    }

    /// Request cancellation of an active process-backed native HTTP(S)
    /// navigation, or validate the current revision and return unchanged when
    /// no navigation is active. An accepted request returns before the
    /// in-flight navigation future reaps its content process; callers that own
    /// that future should continue polling it to completion. If the navigation
    /// has already claimed the commit phase, stop fails instead of reporting a
    /// cancellation that can no longer be honored.
    #[cfg(feature = "native-engine")]
    pub async fn stop_loading_with_revision(
        &self,
        expected_revision: u64,
    ) -> BrowserResult<NavigationControlOutcome> {
        self.require_native_operation("stop_loading_with_revision")?;
        let BackendStartup::Native(backend) = &self.backend else {
            unreachable!("native operation check rejected a non-native runtime")
        };

        loop {
            let changed = self.native_navigation_changed.notified();
            tokio::pin!(changed);
            let _ = changed.as_mut().enable();

            if let Some(active_revision) = backend.active_navigation_revision()? {
                if active_revision != expected_revision {
                    return Err(Box::new(ActionContractError::stale_revision(
                        expected_revision,
                        active_revision,
                    )));
                }
                if backend
                    .cancel_active_navigation(expected_revision)?
                    .is_some()
                {
                    return Ok(NavigationControlOutcome {
                        action: "stopLoading".into(),
                        previous_revision: expected_revision,
                        current_revision: expected_revision,
                    });
                }
                return Err(Box::new(BrowserBackendError::Lifecycle {
                    operation: "stop_loading".into(),
                    state: "committing".into(),
                    reason: "native navigation has entered its commit phase and can no longer be stopped"
                        .into(),
                }));
            }

            tokio::select! {
                _ = &mut changed => continue,
                _operation = self.operation_lock.lock() => {
                    if let Some(active_revision) = backend.active_navigation_revision()? {
                        if active_revision != expected_revision {
                            return Err(Box::new(ActionContractError::stale_revision(
                                expected_revision,
                                active_revision,
                            )));
                        }
                        if backend
                            .cancel_active_navigation(expected_revision)?
                            .is_some()
                        {
                            return Ok(NavigationControlOutcome {
                                action: "stopLoading".into(),
                                previous_revision: expected_revision,
                                current_revision: expected_revision,
                            });
                        }
                        return Err(Box::new(BrowserBackendError::Lifecycle {
                            operation: "stop_loading".into(),
                            state: "committing".into(),
                            reason: "native navigation has entered its commit phase and can no longer be stopped"
                                .into(),
                        }));
                    }
                    self.require_current_revision(expected_revision).await?;
                    return Ok(NavigationControlOutcome {
                        action: "stopLoading".into(),
                        previous_revision: expected_revision,
                        current_revision: expected_revision,
                    });
                }
            }
        }
    }

    /// Compatibility spelling retained for native-only callers.
    #[cfg(feature = "native-engine")]
    pub async fn native_stop_loading_with_revision(
        &self,
        expected_revision: u64,
    ) -> BrowserResult<NavigationControlOutcome> {
        self.stop_loading_with_revision(expected_revision).await
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

    /// Capture the active context through the selected backend's native image
    /// or document owner. Native PNG, JPEG, and PDF are first-class captures;
    /// a backend returns a typed unsupported error when it cannot encode the
    /// requested format rather than silently switching transports.
    pub async fn capture(&self, format: CaptureFormat) -> BrowserResult<CaptureResult> {
        let _operation = self.operation_lock.lock().await;
        let context_id = self.active_context_id().await?;
        Ok(BrowserBackendDispatcher::new(&self.backend)
            .capture(CaptureRequest { context_id, format })
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

    #[cfg(feature = "native-engine")]
    pub async fn native_set_network_conditions(
        &self,
        conditions: Option<&NetworkConditions>,
    ) -> BrowserResult<()> {
        let _operation = self.operation_lock.lock().await;
        match &self.backend {
            BackendStartup::Native(backend) => {
                Ok(backend.set_network_conditions(conditions).await?)
            }
            _ => Err("native network emulation is only available on the native runtime".into()),
        }
    }

    #[cfg(feature = "native-engine")]
    pub async fn native_set_cpu_throttling(&self, rate: Option<f64>) -> BrowserResult<()> {
        let _operation = self.operation_lock.lock().await;
        match &self.backend {
            BackendStartup::Native(backend) => Ok(backend.set_cpu_throttling(rate).await?),
            _ => Err("native CPU emulation is only available on the native runtime".into()),
        }
    }

    #[cfg(feature = "native-engine")]
    pub async fn native_set_user_agent(
        &self,
        user_agent: Option<&str>,
        accept_language: Option<&str>,
        platform: Option<&str>,
    ) -> BrowserResult<()> {
        let _operation = self.operation_lock.lock().await;
        match &self.backend {
            BackendStartup::Native(backend) => Ok(backend
                .set_user_agent(user_agent, accept_language, platform)
                .await?),
            _ => Err("native user-agent emulation is only available on the native runtime".into()),
        }
    }

    #[cfg(feature = "native-engine")]
    pub async fn native_set_geolocation(
        &self,
        location: Option<&GeoLocation>,
    ) -> BrowserResult<()> {
        let _operation = self.operation_lock.lock().await;
        match &self.backend {
            BackendStartup::Native(backend) => Ok(backend.set_geolocation(location).await?),
            _ => Err("native geolocation is only available on the native runtime".into()),
        }
    }

    #[cfg(feature = "native-engine")]
    pub async fn native_set_timezone(&self, timezone_id: Option<&str>) -> BrowserResult<()> {
        let _operation = self.operation_lock.lock().await;
        match &self.backend {
            BackendStartup::Native(backend) => Ok(backend.set_timezone(timezone_id).await?),
            _ => Err("native timezone emulation is only available on the native runtime".into()),
        }
    }

    #[cfg(feature = "native-engine")]
    pub async fn native_set_viewport(&self, viewport: Viewport) -> BrowserResult<()> {
        let _operation = self.operation_lock.lock().await;
        match &self.backend {
            BackendStartup::Native(backend) => Ok(backend.set_viewport(viewport).await?),
            _ => Err("native viewport updates are only available on the native runtime".into()),
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

    /// Capture a native visual using the same option and metadata contract as
    /// the Chromium session, without routing the request through CDP.
    #[cfg(feature = "native-engine")]
    pub async fn native_capture_visual(
        &self,
        options: &VisualCaptureOptions,
    ) -> BrowserResult<VisualCapture> {
        let _operation = self.operation_lock.lock().await;
        match &self.backend {
            BackendStartup::Native(backend) => Ok(backend.capture_visual(options).await?),
            _ => Err("native visual capture is only available on the native runtime".into()),
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

    /// Return the canonical structured native semantic observation.
    ///
    /// This standard Rust API reads one native frame snapshot and never probes
    /// or falls back to another browser runtime.
    #[cfg(feature = "native-engine")]
    pub async fn observe(&self) -> BrowserResult<super::session::SemanticObservation> {
        self.require_native_operation("observe")?;
        self.native_semantic_observe(super::session::SemanticObservationLevel::Structured)
            .await
    }

    /// Return one native semantic observation at the requested bounded level.
    /// Non-native runtime adapters return a typed unsupported-operation error.
    #[cfg(feature = "native-engine")]
    pub async fn semantic_observe(
        &self,
        level: super::session::SemanticObservationLevel,
    ) -> BrowserResult<super::session::SemanticObservation> {
        self.require_native_operation("semantic_observe")?;
        self.native_semantic_observe(level).await
    }

    /// Return the standard native inspection envelope for the active page.
    #[cfg(feature = "native-engine")]
    pub async fn inspect_page(&self) -> BrowserResult<InspectPageResult> {
        self.require_native_operation("inspect_page")?;
        self.native_inspect_page().await
    }

    /// Return page-state bootstrap evidence without publishing action targets.
    #[cfg(feature = "native-engine")]
    pub async fn observe_bootstrap(&self) -> BrowserResult<BootstrapObservation> {
        self.require_native_operation("observe_bootstrap")?;
        self.native_observe_bootstrap().await
    }

    /// Expand one region only if the observation revision is still current.
    #[cfg(feature = "native-engine")]
    pub async fn semantic_expand_region(
        &self,
        region_id: &str,
        expected_revision: u64,
        level: super::session::SemanticObservationLevel,
    ) -> BrowserResult<super::session::SemanticObservation> {
        self.require_native_operation("semantic_expand_region")?;
        self.native_semantic_expand_region(region_id, expected_revision, level)
            .await
    }

    #[cfg(feature = "native-engine")]
    fn require_native_operation(&self, operation: &str) -> BrowserResult<()> {
        if matches!(&self.backend, BackendStartup::Native(_)) {
            return Ok(());
        }
        Err(Box::new(BrowserBackendError::UnsupportedOperation {
            operation: operation.to_owned(),
            reason: format!(
                "native {operation} is unavailable for the {} runtime",
                self.runtime.browser_family()
            ),
        }))
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

    /// Reconcile revisioned native semantic references against the latest
    /// observation. The native path uses the same bounded identity rules as
    /// the Chromium session: exact references and backend identity win, then
    /// unique role/name or caller-supplied hints may relocate a target.
    #[cfg(feature = "native-engine")]
    pub async fn native_reconcile_references(
        &self,
        from_revision: u64,
        refs: &[String],
        options: &ReconciliationOptions,
    ) -> BrowserResult<ReconciliationOutcome> {
        if refs.len() > super::session::MAX_RECONCILE_REFS {
            return Err(format!(
                "too many refs to reconcile: {} (max {})",
                refs.len(),
                super::session::MAX_RECONCILE_REFS
            )
            .into());
        }
        if options.hints.len() > super::session::MAX_RECONCILE_HINTS {
            return Err(format!(
                "too many reconciliation hints: {} (max {})",
                options.hints.len(),
                super::session::MAX_RECONCILE_HINTS
            )
            .into());
        }

        let _operation = self.operation_lock.lock().await;
        let prior = self
            .native_observation_cache
            .lock()
            .await
            .as_ref()
            .filter(|observation| observation.revision == from_revision)
            .cloned();
        let current = self
            .native_semantic_observation_level_unlocked(SemanticObservationLevel::Structured)
            .await?;
        let mutation_summary = |prior: Option<&SemanticObservation>| MutationSummary {
            url_changed: prior.is_some_and(|prior| prior.page.url != current.page.url),
            title_changed: prior.is_some_and(|prior| prior.page.title != current.page.title),
            revision_delta: current.revision.saturating_sub(from_revision),
            soft_navigation_suspected: prior.is_some_and(|prior| {
                current.revision > from_revision
                    && prior.page.url == current.page.url
                    && prior.page.title == current.page.title
            }),
        };

        let Some(prior) = prior else {
            return bounded_native_reconciliation_outcome(ReconciliationOutcome {
                status: ReconciliationStatus::Complete,
                to_revision: current.revision,
                mappings: refs
                    .iter()
                    .map(|old| ReferenceMapping::Lost {
                        old: old.clone(),
                        reason: ReferenceLostReason::StaleBoundary,
                    })
                    .collect(),
                preserved: 0,
                relocated: 0,
                lost: refs.len(),
                mutation_summary: mutation_summary(None),
                incomplete: vec![ObservationIncompleteReason::BoundaryScan],
            });
        };

        if prior.route != current.route {
            return bounded_native_reconciliation_outcome(ReconciliationOutcome {
                status: ReconciliationStatus::RouteChanged,
                to_revision: current.revision,
                mappings: refs
                    .iter()
                    .map(|old| ReferenceMapping::Lost {
                        old: old.clone(),
                        reason: ReferenceLostReason::StaleBoundary,
                    })
                    .collect(),
                preserved: 0,
                relocated: 0,
                lost: refs.len(),
                mutation_summary: mutation_summary(Some(&prior)),
                incomplete: native_observation_incomplete(&current),
            });
        }

        let prior_targets = prior
            .regions
            .iter()
            .flat_map(|region| {
                region
                    .targets
                    .iter()
                    .map(move |target| (region.id.as_str(), target))
            })
            .collect::<Vec<_>>();
        let current_targets = current
            .regions
            .iter()
            .flat_map(|region| {
                region
                    .targets
                    .iter()
                    .map(move |target| (region.id.as_str(), target))
            })
            .collect::<Vec<_>>();
        let scope_region = options.scope_ref.as_deref().and_then(|scope_ref| {
            prior_targets
                .iter()
                .find(|(_, target)| target.reference == scope_ref)
                .map(|(region, _)| *region)
        });
        let scope_invalid = options.scope_ref.is_some() && scope_region.is_none();
        let in_scope = |region: &str| scope_region.is_none_or(|scope| scope == region);
        let current_backend = |target: &SemanticTarget| {
            parse_native_semantic_reference(&target.reference)
                .ok()
                .flatten()
                .map(|reference| (reference.context_id, reference.backend_dom_node_id))
        };
        let mut mappings = Vec::with_capacity(refs.len());
        let mut preserved = 0;
        let mut relocated = 0;
        let mut lost = 0;

        for (index, old) in refs.iter().enumerate() {
            let parsed = parse_native_semantic_reference(old).ok().flatten();
            let valid_revision = parsed
                .as_ref()
                .is_some_and(|reference| reference.revision == from_revision);
            if !valid_revision {
                mappings.push(ReferenceMapping::Lost {
                    old: old.clone(),
                    reason: ReferenceLostReason::StaleBoundary,
                });
                lost += 1;
                continue;
            }
            if scope_invalid {
                mappings.push(ReferenceMapping::Lost {
                    old: old.clone(),
                    reason: ReferenceLostReason::OutOfScope,
                });
                lost += 1;
                continue;
            }

            let prior_target = prior_targets
                .iter()
                .find(|(_, target)| target.reference == *old);
            if current.revision == from_revision {
                let preserved_target = prior_target.and_then(|(region, _)| {
                    in_scope(region).then(|| {
                        current_targets.iter().find(|(current_region, target)| {
                            *current_region == *region && target.reference == *old
                        })
                    })
                });
                if preserved_target.flatten().is_some() {
                    mappings.push(ReferenceMapping::Preserved {
                        old: old.clone(),
                        new: old.clone(),
                    });
                    preserved += 1;
                } else {
                    mappings.push(ReferenceMapping::Lost {
                        old: old.clone(),
                        reason: if prior_target.is_some_and(|(region, _)| !in_scope(region)) {
                            ReferenceLostReason::OutOfScope
                        } else {
                            ReferenceLostReason::StaleBoundary
                        },
                    });
                    lost += 1;
                }
                continue;
            }

            if let Some((_, backend_id)) = parsed
                .as_ref()
                .map(|reference| (reference.context_id.clone(), reference.backend_dom_node_id))
            {
                let backend_matches = current_targets
                    .iter()
                    .filter(|(region, target)| {
                        in_scope(region)
                            && current_backend(target).is_some_and(|(context, candidate)| {
                                context
                                    == parsed
                                        .as_ref()
                                        .and_then(|reference| reference.context_id.clone())
                                    && candidate == backend_id
                            })
                    })
                    .collect::<Vec<_>>();
                if let [(_, target)] = backend_matches.as_slice() {
                    mappings.push(ReferenceMapping::Preserved {
                        old: old.clone(),
                        new: target.reference.clone(),
                    });
                    preserved += 1;
                    continue;
                }
            }

            let mut matches = prior_target
                .map(|(_, target)| {
                    current_targets
                        .iter()
                        .filter(|(region, candidate)| {
                            in_scope(region)
                                && candidate.role.eq_ignore_ascii_case(&target.role)
                                && candidate.name.eq_ignore_ascii_case(&target.name)
                        })
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            let mut matched_by = ReferenceMatch::RoleAndName;
            if matches.is_empty()
                && let Some(hint) = options.hints.get(index)
            {
                matches = current_targets
                    .iter()
                    .filter(|(region, target)| {
                        in_scope(region) && native_locator_matches(hint, target)
                    })
                    .collect();
                matched_by = match hint {
                    super::session::Locator::AccessibleName(_) => ReferenceMatch::AccessibleName,
                    super::session::Locator::RoleAndName { .. } => ReferenceMatch::RoleAndName,
                    _ => ReferenceMatch::Hint,
                };
            }
            match matches.as_slice() {
                [(_, target)] => {
                    mappings.push(ReferenceMapping::Relocated {
                        old: old.clone(),
                        new: target.reference.clone(),
                        matched_by,
                    });
                    relocated += 1;
                }
                [] => {
                    mappings.push(ReferenceMapping::Lost {
                        old: old.clone(),
                        reason: if prior_target.is_some_and(|(region, _)| !in_scope(region)) {
                            ReferenceLostReason::OutOfScope
                        } else {
                            ReferenceLostReason::NotFound
                        },
                    });
                    lost += 1;
                }
                many => {
                    mappings.push(ReferenceMapping::Lost {
                        old: old.clone(),
                        reason: ReferenceLostReason::Ambiguous {
                            candidates: many
                                .iter()
                                .take(NATIVE_RECONCILIATION_CANDIDATE_LIMIT)
                                .map(|(_, target)| CandidateSummary {
                                    label: bounded_native_candidate_label(&format!(
                                        "{} {}",
                                        target.role, target.name
                                    )),
                                    reference: Some(target.reference.clone()),
                                })
                                .collect(),
                        },
                    });
                    lost += 1;
                }
            }
        }

        bounded_native_reconciliation_outcome(ReconciliationOutcome {
            status: ReconciliationStatus::Complete,
            to_revision: current.revision,
            mappings,
            preserved,
            relocated,
            lost,
            mutation_summary: mutation_summary(Some(&prior)),
            incomplete: native_observation_incomplete(&current),
        })
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

    /// Fill a bounded set of native form controls after resolving every
    /// locator. The resolution phase is side-effect free; each action then
    /// uses the same semantic action owner as ordinary native input.
    #[cfg(feature = "native-engine")]
    pub async fn native_fill_form(
        &self,
        fields: &[(&str, &str)],
        expected_revision: Option<u64>,
    ) -> BrowserResult<FillFormOutcome> {
        const MAX_FIELDS: usize = 16;
        if fields.len() > MAX_FIELDS {
            return Err(format!("fill_form: max {MAX_FIELDS} fields, got {}", fields.len()).into());
        }
        let before = self.native_observe().await?;
        if let Some(expected_revision) = expected_revision
            && before.revision != expected_revision
        {
            return Err(Box::new(ActionContractError::stale_revision(
                expected_revision,
                before.revision,
            )));
        }

        let mut plans = Vec::with_capacity(fields.len());
        for (target, value) in fields {
            let click_preflight = self
                .native_preflight_target(target, NativePreflightAction::Click)
                .await?;
            if !click_preflight.unique {
                return Err(
                    format!("native fill target could not be resolved uniquely: {target}").into(),
                );
            }
            let node = click_preflight
                .node
                .ok_or_else(|| format!("native fill target returned no semantic node: {target}"))?;
            let (action, preflight_action) = match node.role.as_str() {
                "listbox" | "combobox" => ("select", NativePreflightAction::Select),
                "checkbox" => {
                    let should_check =
                        !value.is_empty() && *value != "false" && *value != "0" && *value != "off";
                    if should_check {
                        ("check", NativePreflightAction::Check)
                    } else {
                        ("uncheck", NativePreflightAction::Check)
                    }
                }
                "radio" => ("click", NativePreflightAction::Click),
                "textbox" => ("type", NativePreflightAction::Type),
                role => {
                    return Err(format!(
                        "native fill does not support semantic role {role:?} for target {target}"
                    )
                    .into());
                }
            };
            let actionability = self
                .native_preflight_target(target, preflight_action)
                .await?;
            if !actionability.unique || actionability.actionable != Some(true) {
                return Err(format!(
                    "native fill target is not actionable: {target} ({:?})",
                    actionability.actionability_reason
                )
                .into());
            }
            plans.push((
                (*target).to_owned(),
                (*value).to_owned(),
                action.to_owned(),
                (!node.name.is_empty()).then_some(node.name),
            ));
        }

        let mut results = Vec::with_capacity(plans.len());
        let mut filled = 0usize;
        for (index, (target, value, action, label)) in plans.iter().enumerate() {
            let semantic_action = match action.as_str() {
                "select" => SemanticAction::Select {
                    target: target.clone(),
                    value: value.clone(),
                },
                "check" => SemanticAction::Check {
                    target: target.clone(),
                },
                "uncheck" => SemanticAction::Uncheck {
                    target: target.clone(),
                },
                "click" => SemanticAction::Click {
                    target: target.clone(),
                },
                "type" => SemanticAction::Type {
                    target: target.clone(),
                    text: value.clone(),
                },
                _ => unreachable!("native fill plan action was validated"),
            };
            let action_result = if index == 0 {
                match expected_revision {
                    Some(expected_revision) => {
                        self.action_with_revision(semantic_action, expected_revision)
                            .await
                    }
                    None => self.action(semantic_action).await,
                }
            } else {
                self.action(semantic_action).await
            };
            let (success, error) = match action_result {
                Ok(result) if result.accepted => {
                    filled += 1;
                    (true, None)
                }
                Ok(_) => (false, Some("native action was not accepted".to_owned())),
                Err(error) => (false, Some(error.to_string())),
            };
            results.push(FillFieldResult {
                target: target.clone(),
                action: action.clone(),
                label: label.clone(),
                success,
                error,
            });
        }
        let after = self.native_observe().await?;
        Ok(FillFormOutcome {
            status: if filled == fields.len() {
                ActionStatus::Succeeded
            } else {
                ActionStatus::CompletedWithVerificationFailure
            },
            failure_kind: (filled != fields.len()).then_some(ActionFailureKind::VerificationFailed),
            execution_id: self.next_native_execution_id(),
            filled,
            total: fields.len(),
            fields: results,
            previous_revision: before.revision,
            current_revision: after.revision,
            verification: ActionVerificationEvidence {
                revision_delta: after.revision.saturating_sub(before.revision),
                url_changed: before.page.url != after.page.url,
                title_changed: before.page.title != after.page.title,
                ..ActionVerificationEvidence::default()
            },
        })
    }

    /// Read the native session clipboard broker, bounded to the same 8 KiB
    /// contract as the Chromium clipboard extension.
    #[cfg(feature = "native-engine")]
    pub async fn native_clipboard_read(&self) -> BrowserResult<String> {
        Ok(self.native_clipboard.lock().await.clone())
    }

    /// Write the native session clipboard broker with a UTF-8 byte bound.
    #[cfg(feature = "native-engine")]
    pub async fn native_clipboard_write(&self, text: &str) -> BrowserResult<()> {
        const MAX_BYTES: usize = 8192;
        let end = text.len().min(MAX_BYTES);
        let end = (0..=end)
            .rev()
            .find(|index| text.is_char_boundary(*index))
            .expect("zero is a UTF-8 boundary");
        *self.native_clipboard.lock().await = text[..end].to_owned();
        Ok(())
    }

    /// Dismiss a recognized consent wall through the native page realm.
    #[cfg(feature = "native-engine")]
    pub async fn native_dismiss_consent(
        &self,
    ) -> BrowserResult<super::session::ConsentDismissalOutcome> {
        const SCRIPT: &str = r#"(() => {
            const visible = (el) => {
                if (!el) return false;
                const style = getComputedStyle(el);
                const rect = el.getBoundingClientRect();
                return style.display !== 'none' && style.visibility !== 'hidden' &&
                    Number(style.opacity) !== 0 && rect.width > 0 && rect.height > 0;
            };
            const click = (root, selectors) => {
                for (const selector of selectors) {
                    const el = root.querySelector(selector);
                    if (visible(el)) { el.click(); return true; }
                }
                return false;
            };
            const oneTrust = document.querySelector('#onetrust-banner-sdk, #onetrust-consent-sdk, .onetrust-pc-dark-filter');
            if (oneTrust) return {framework:'onetrust', dismissed: click(document, ['#onetrust-accept-btn-handler', '#onetrust-reject-all-handler'])};
            const cookiebot = document.querySelector('#CybotCookiebotDialog, [data-template="cookiebot"]');
            if (cookiebot) return {framework:'cookiebot', dismissed: click(cookiebot, ['#CybotCookiebotDialogBodyLevelButtonAccept', '#CybotCookiebotDialogBodyButtonDecline'])};
            return {framework:null, dismissed:false};
        })()"#;
        let mut value = self.script(SCRIPT).await?.value;
        if let Some(text) = value.as_str() {
            value = serde_json::from_str(text).unwrap_or(serde_json::Value::Null);
        }
        match (
            value["framework"].as_str(),
            value["dismissed"].as_bool().unwrap_or(false),
        ) {
            (_, true) => Ok(super::session::ConsentDismissalOutcome::Dismissed),
            (Some("onetrust" | "cookiebot"), false) => {
                Ok(super::session::ConsentDismissalOutcome::UnrecognizedFramework)
            }
            (None, false) => Ok(super::session::ConsentDismissalOutcome::NoConsentFound),
            _ => Ok(super::session::ConsentDismissalOutcome::UnrecognizedFramework),
        }
    }

    /// Render the current native semantic page as a bounded PDF document.
    #[cfg(feature = "native-engine")]
    pub async fn native_print_to_pdf(
        &self,
        options: &super::session::PdfOptions,
    ) -> BrowserResult<String> {
        let observation = self.native_observe().await?;
        super::native_pdf::render(&observation, options)
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
fn is_http_navigation_url(value: &str) -> bool {
    url::Url::parse(value).is_ok_and(|url| matches!(url.scheme(), "http" | "https"))
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
fn native_locator_matches(locator: &super::session::Locator, target: &SemanticTarget) -> bool {
    match locator {
        super::session::Locator::Reference(reference) => target.reference == *reference,
        super::session::Locator::AccessibleName(name) => target.name.eq_ignore_ascii_case(name),
        super::session::Locator::RoleAndName { role, name } => {
            target.role.eq_ignore_ascii_case(role) && target.name.eq_ignore_ascii_case(name)
        }
        super::session::Locator::Text(text) => target.name.eq_ignore_ascii_case(text),
        super::session::Locator::Css(_) | super::session::Locator::Ordinal(_) => false,
    }
}

#[cfg(feature = "native-engine")]
fn bounded_native_candidate_label(value: &str) -> String {
    const MAX_BYTES: usize = 128;
    if value.len() <= MAX_BYTES {
        return value.to_owned();
    }
    let mut end = MAX_BYTES;
    while end > 0 && !value.is_char_boundary(end) {
        end -= 1;
    }
    value[..end].to_owned()
}

#[cfg(feature = "native-engine")]
fn bounded_native_reconciliation_outcome(
    outcome: ReconciliationOutcome,
) -> BrowserResult<ReconciliationOutcome> {
    if serde_json::to_vec(&outcome)?.len() > super::session::MAX_RECONCILIATION_BYTES {
        return Err(format!(
            "reconciliation response exceeds {} bytes; retry with fewer refs",
            super::session::MAX_RECONCILIATION_BYTES
        )
        .into());
    }
    Ok(outcome)
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
fn parse_native_semantic_reference(
    value: &str,
) -> BrowserResult<Option<super::session::RevisionedElementReference>> {
    let value = value.strip_prefix("ref=").unwrap_or(value);
    let normalized = value
        .strip_prefix('r')
        .and_then(|rest| rest.split_once(":n"))
        .map(|(revision, node)| format!("r{revision}:b{node}"));
    super::session::parse_revisioned_reference(normalized.as_deref().unwrap_or(value))
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

#[cfg(all(test, feature = "native-engine"))]
mod public_session_tests {
    use super::{BackendStartup, BrowserRuntime, BrowserRuntimeSession, BrowserSession};
    use crate::browser_backend::BrowserBackendError;

    #[tokio::test]
    async fn canonical_browser_session_starts_native_without_cdp_fallback() {
        let session = BrowserSession::start_default()
            .await
            .expect("default BrowserSession must initialize the native backend");

        assert_eq!(session.runtime(), BrowserRuntime::Native);
        assert!(matches!(&session.backend, BackendStartup::Native(_)));
        session
            .close()
            .await
            .expect("native BrowserSession should close cleanly");
    }

    #[tokio::test]
    async fn standard_semantic_observation_rejects_non_native_backend_typed() {
        let session = BrowserRuntimeSession {
            runtime: BrowserRuntime::Firefox,
            backend: BackendStartup::Proof(Box::new(
                crate::browser::proof_backend::ProofBackend::new()
                    .expect("proof backend should construct"),
            )),
            operation_lock: tokio::sync::Mutex::new(()),
            native_navigation_changed: tokio::sync::Notify::new(),
            next_execution_id: std::sync::atomic::AtomicU64::new(1),
            native_observation_cache: tokio::sync::Mutex::new(None),
            native_clipboard: tokio::sync::Mutex::new(String::new()),
        };

        macro_rules! assert_typed_unsupported {
            ($operation:literal, $result:expr) => {{
                let error = $result
                    .expect_err("non-native sessions must not run native semantic inspection");
                assert!(matches!(
                    error.downcast_ref::<BrowserBackendError>(),
                    Some(BrowserBackendError::UnsupportedOperation {
                        operation,
                        reason
                    }) if operation == $operation && reason.contains("firefox")
                ));
            }};
        }

        assert_typed_unsupported!("observe", session.observe().await);
        assert_typed_unsupported!(
            "semantic_observe",
            session
                .semantic_observe(super::super::session::SemanticObservationLevel::Structured)
                .await
        );
        assert_typed_unsupported!("inspect_page", session.inspect_page().await);
        assert_typed_unsupported!("observe_bootstrap", session.observe_bootstrap().await);
        assert_typed_unsupported!(
            "semantic_expand_region",
            session
                .semantic_expand_region(
                    "region_main",
                    0,
                    super::super::session::SemanticObservationLevel::Structured,
                )
                .await
        );
    }

    #[tokio::test]
    async fn standard_topology_rejects_non_native_backend_typed() {
        let session = BrowserRuntimeSession {
            runtime: BrowserRuntime::Firefox,
            backend: BackendStartup::Proof(Box::new(
                crate::browser::proof_backend::ProofBackend::new()
                    .expect("proof backend should construct"),
            )),
            operation_lock: tokio::sync::Mutex::new(()),
            native_navigation_changed: tokio::sync::Notify::new(),
            next_execution_id: std::sync::atomic::AtomicU64::new(1),
            native_observation_cache: tokio::sync::Mutex::new(None),
            native_clipboard: tokio::sync::Mutex::new(String::new()),
        };

        macro_rules! assert_typed_unsupported {
            ($operation:literal, $result:expr) => {{
                let error = $result
                    .expect_err("non-native sessions must not access native topology");
                assert!(matches!(
                    error.downcast_ref::<BrowserBackendError>(),
                    Some(BrowserBackendError::UnsupportedOperation {
                        operation,
                        reason
                    }) if operation == $operation && reason.contains("firefox")
                ));
            }};
        }

        assert_typed_unsupported!("list_targets", session.list_targets().await);
        assert_typed_unsupported!("create_target", session.create_target("about:blank").await);
        assert_typed_unsupported!("select_target", session.select_target("target-1").await);
        assert_typed_unsupported!("close_target", session.close_target("target-1").await);
        assert_typed_unsupported!("list_frames", session.list_frames().await);
        assert_typed_unsupported!("select_frame", session.select_frame("frame-1").await);
    }

    #[tokio::test]
    async fn standard_history_and_recovery_reject_non_native_backend_typed() {
        let session = BrowserRuntimeSession {
            runtime: BrowserRuntime::Firefox,
            backend: BackendStartup::Proof(Box::new(
                crate::browser::proof_backend::ProofBackend::new()
                    .expect("proof backend should construct"),
            )),
            operation_lock: tokio::sync::Mutex::new(()),
            native_navigation_changed: tokio::sync::Notify::new(),
            next_execution_id: std::sync::atomic::AtomicU64::new(1),
            native_observation_cache: tokio::sync::Mutex::new(None),
            native_clipboard: tokio::sync::Mutex::new(String::new()),
        };

        macro_rules! assert_typed_unsupported {
            ($operation:literal, $result:expr) => {{
                let error = $result
                    .expect_err("non-native sessions must not run native navigation controls");
                assert!(matches!(
                    error.downcast_ref::<BrowserBackendError>(),
                    Some(BrowserBackendError::UnsupportedOperation {
                        operation,
                        reason
                    }) if operation == $operation && reason.contains("firefox")
                ));
            }};
        }

        assert_typed_unsupported!("go_back", session.go_back().await);
        assert_typed_unsupported!(
            "go_back_with_revision",
            session.go_back_with_revision(0).await
        );
        assert_typed_unsupported!("go_forward", session.go_forward().await);
        assert_typed_unsupported!(
            "go_forward_with_revision",
            session.go_forward_with_revision(0).await
        );
        assert_typed_unsupported!(
            "reload_with_revision",
            session.reload_with_revision(0).await
        );
        assert_typed_unsupported!("recover", session.recover().await);
        assert_typed_unsupported!(
            "recover_with_revision",
            session.recover_with_revision(0).await
        );
        assert_typed_unsupported!(
            "stop_loading_with_revision",
            session.stop_loading_with_revision(0).await
        );
    }
}
