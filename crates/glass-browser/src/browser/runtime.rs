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
use super::native_engine::{NativeEngineConfig, NativePreflightAction, NativeTargetPreflight};
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
    ActionStatus, ActionTarget, ActionVerificationError, ActionVerificationEvidence, Cookie,
    FindTargetResult, FrameInfo, InspectPageResult, IntentPolicyDecision, NavigationControlOutcome,
    PageTargetInfo, RecoveryStrategy, SemanticIntentAction, SemanticIntentExecutionRequest,
    SemanticIntentExecutionResult, SemanticIntentExecutionStatus, SemanticIntentResult,
    SemanticResolution, VerificationOutcome, VerificationPredicate, WaitCondition, WaitOutcome,
    WaitTimeout,
};
use super::session::{ActionContractError, BrowserResult};
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

/// Browser runtimes supported by the portable semantic session.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum BrowserRuntime {
    /// Full production Chrome/Chromium session through CDP.
    Chromium,
    /// Experimental direct WebDriver BiDi session.
    Firefox,
    /// Experimental W3C WebDriver session through SafariDriver.
    Safari,
    /// Experimental Glass-owned local-content engine.
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
            BackendStartup::Native(backend) => Ok(backend.list_frames()?),
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

    /// Select one explicitly listed native frame.
    #[cfg(feature = "native-engine")]
    pub async fn native_select_frame(&self, frame_id: &str) -> BrowserResult<FrameInfo> {
        let _operation = self.operation_lock.lock().await;
        match &self.backend {
            BackendStartup::Native(backend) => Ok(backend.select_frame(frame_id)?),
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
            BackendStartup::Native(backend) => Ok(backend.preflight_target(target, action)?),
            _ => Err("native target preflight is only available on the native runtime".into()),
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
            let observation = self.native_semantic_observation_unlocked()?;
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

            let action_result = self.action_unlocked(action).await?;
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

            let after = self.native_semantic_observation_unlocked()?;
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
                frame_id: after.route.frame_id.clone(),
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

    #[cfg(feature = "native-engine")]
    async fn native_semantic_observation(
        &self,
    ) -> BrowserResult<super::session::SemanticObservation> {
        let _operation = self.operation_lock.lock().await;
        self.native_semantic_observation_unlocked()
    }

    #[cfg(feature = "native-engine")]
    fn native_semantic_observation_unlocked(
        &self,
    ) -> BrowserResult<super::session::SemanticObservation> {
        let native = match &self.backend {
            BackendStartup::Native(backend) => backend.inspection_snapshot()?,
            _ => {
                return Err(
                    "native semantic inspection is only available on the native runtime".into(),
                );
            }
        };
        let route = super::session::SemanticRouteIdentity {
            target_id: native.context_id.clone(),
            frame_id: format!("{}:main", native.context_id),
            url: native.snapshot.url.clone(),
        };
        let targets = native
            .nodes
            .iter()
            .map(native_semantic_target)
            .collect::<Vec<_>>();
        let region = super::session::SemanticRegion {
            id: "region_main".into(),
            kind: super::session::SemanticRegionKind::Main,
            label: "Main content".into(),
            interactive_count: targets.len(),
            item_count: Some(targets.len()),
            confidence: super::session::SemanticConfidence::High,
            structured_records: Vec::new(),
            evidence: vec!["native semantic node projection".into()],
            targets,
            expansion: Some(super::session::SemanticExpansionHandle {
                region_id: "region_main".into(),
                revision: native.snapshot.revision,
                route: route.clone(),
            }),
        };
        let viewport = native.layout.viewport;
        let scroll_offset = native.layout.scroll_offset;
        let observation = super::session::SemanticObservation {
            schema_version: super::session::SEMANTIC_OBSERVATION_SCHEMA_VERSION,
            revision: native.snapshot.revision,
            level: super::session::SemanticObservationLevel::Structured,
            route: route.clone(),
            page: super::session::SemanticPage {
                kind: super::session::SemanticPageKind::Generic,
                title: native.snapshot.title,
                url: native.snapshot.url,
                target_id: route.target_id.clone(),
                frame_id: route.frame_id.clone(),
                confidence: super::session::SemanticConfidence::Medium,
                evidence: vec!["native page snapshot".into()],
            },
            regions: vec![region],
            text: Some(native.snapshot.visible_text.clone()),
            accessibility: None,
            raw_accessibility: None,
            changes: None,
            limits: super::session::SemanticObservationLimits {
                truncated: native.snapshot.title_truncated || native.snapshot.text_truncated,
                omitted_regions: 0,
                omitted_targets: 0,
                omitted_structured_records: 0,
                structured_bytes: Some(0),
                omitted_bytes: None,
                text_bytes: Some(native.snapshot.visible_text.len()),
                text_truncated: native.snapshot.text_truncated,
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
        Ok(observation)
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
        if matches!(condition, WaitCondition::NetworkQuiet(_)) {
            return Err(
                "native network-quiet waits require native request lifecycle accounting".into(),
            );
        }
        let description = condition.description();
        let started = tokio::time::Instant::now();
        let expires = started + deadline;
        let mut previous_geometry = None;
        loop {
            let (matched, state, geometry) = self
                .native_check_wait_condition(&condition, previous_geometry.as_deref())
                .await?;
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
            VerificationPredicate::PopupOpened { .. }
            | VerificationPredicate::DialogOpen { .. }
            | VerificationPredicate::DownloadStarted { .. } => Err(
                "native verification does not yet expose popup, dialog, or download topology"
                    .into(),
            ),
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
    fn next_native_execution_id(&self) -> String {
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
fn native_semantic_target(
    node: &super::native_engine::NativeSemanticNode,
) -> super::session::SemanticTarget {
    super::session::SemanticTarget {
        reference: node.reference.clone(),
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
