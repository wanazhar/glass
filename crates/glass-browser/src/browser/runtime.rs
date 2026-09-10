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

use super::session::{ActionContractError, BrowserResult};
#[cfg(feature = "native-engine")]
use super::session::{FindTargetResult, InspectPageResult};
use tokio::sync::Mutex;

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

    #[cfg(feature = "native-engine")]
    async fn native_semantic_observation(
        &self,
    ) -> BrowserResult<super::session::SemanticObservation> {
        let _operation = self.operation_lock.lock().await;
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

    pub async fn close(self) -> BrowserResult<()> {
        Ok(BrowserBackendDispatcher::new(&self.backend).close().await?)
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
