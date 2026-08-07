//! Deterministic, browser-free proof backend for semantic conformance.
//!
//! This adapter intentionally does not emulate a browser or expose CDP. It
//! keeps one bounded context in memory and exercises the same semantic
//! requests and responses used by transport-backed adapters.

use std::collections::BTreeMap;
use std::sync::Mutex;

use crate::browser_backend::{
    ActionResult, BROWSER_BACKEND_SCHEMA_VERSION, BackendFuture, BackendOperation, BackendProfile,
    BackendRequest, BackendResponse, BrowserBackend, BrowserBackendError, BrowserCapability,
    BrowsingContext, CapabilityDescriptor, CertificationLevel, CertificationProfile, EvidenceLevel,
    EvidenceResult, MAX_TEXT_BYTES, NavigationResult, Portability, SemanticAction, SupportLevel,
};

/// Stable identifier for the browser-free proof backend.
pub const PROOF_BACKEND_ID: &str = "semantic-proof";
/// Stable deterministic context identifier returned by [`ProofBackend`].
pub const PROOF_CONTEXT_ID: &str = "proof-context";
/// Deterministic click target accepted by the proof backend.
pub const PROOF_CLICK_TARGET: &str = "proof.button";
/// Deterministic type target accepted by the proof backend.
pub const PROOF_TYPE_TARGET: &str = "proof.input";
const PROOF_BACKEND_VERSION: &str = "1";
const PROOF_BROWSER_FAMILY: &str = "browser-free";
const PROOF_TITLE: &str = "Glass semantic proof page";
const PROOF_MAX_TYPED_BYTES: usize = 1024;

/// A bounded in-memory backend used to prove semantic dispatcher conformance.
///
/// The backend has no browser dependency. Its state is deliberately limited to
/// one context, one URL, one typed value, and a click marker. It is useful for
/// protocol tests and browser-free integrations, not as a browser substitute.
pub struct ProofBackend {
    profile: BackendProfile,
    state: Mutex<ProofState>,
}

#[derive(Debug, Default)]
struct ProofState {
    initialized: bool,
    closed: bool,
    url: String,
    revision: u64,
    clicked: bool,
    typed_text: String,
}

impl ProofBackend {
    /// Construct the default experimental proof backend.
    pub fn new() -> Result<Self, BrowserBackendError> {
        Self::with_profile(Self::profile_for(env!("CARGO_PKG_VERSION"))?)
    }

    /// Access the validated semantic capability profile.
    pub fn profile(&self) -> &BackendProfile {
        &self.profile
    }

    /// Construct a proof backend with explicit certification metadata.
    ///
    /// This is primarily useful to conformance tests that intentionally remove
    /// a capability. The profile is always validated before construction.
    pub fn with_profile(profile: BackendProfile) -> Result<Self, BrowserBackendError> {
        profile.validate()?;
        Ok(Self {
            profile,
            state: Mutex::new(ProofState {
                url: "proof://initial".into(),
                ..ProofState::default()
            }),
        })
    }

    /// Return the deterministic profile without creating mutable backend state.
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
        ];
        let mut capabilities = BTreeMap::new();
        for capability in supported {
            let limitations = match capability {
                BrowserCapability::Contexts => vec!["one deterministic context is exposed".into()],
                BrowserCapability::Evidence => {
                    vec!["evidence is bounded semantic metadata; no pixels are captured".into()]
                }
                BrowserCapability::Action => {
                    vec!["only click and type proof targets are supported".into()]
                }
                BrowserCapability::Effects => {
                    vec!["effects are revision markers, not browser events".into()]
                }
                _ => Vec::new(),
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
                backend_id: PROOF_BACKEND_ID.into(),
                version: PROOF_BACKEND_VERSION.into(),
                browser: crate::browser_backend::BrowserVersionRange {
                    family: PROOF_BROWSER_FAMILY.into(),
                    minimum: None,
                    maximum: None,
                },
                certification: CertificationProfile {
                    level: CertificationLevel::Experimental,
                    glass_version: glass_version.into(),
                    tested_capabilities: supported.to_vec(),
                    limitations: vec![
                        "browser-free deterministic proof only; this backend does not claim browser parity".into(),
                        "state is process-local and is not persisted".into(),
                    ],
                },
            },
            capabilities,
        };
        profile.validate()?;
        Ok(profile)
    }

    fn dispatch_locked(
        &self,
        operation: BackendOperation,
        request: BackendRequest,
        state: &mut ProofState,
    ) -> Result<BackendResponse, BrowserBackendError> {
        if request.operation() != operation {
            return Err(BrowserBackendError::UnsupportedOperation {
                operation: operation_name(operation).into(),
                reason: "request variant does not match the semantic operation".into(),
            });
        }
        match (operation, request) {
            (BackendOperation::Initialize, BackendRequest::Initialize) => {
                if state.initialized && !state.closed {
                    return Ok(BackendResponse::Unit);
                }
                state.initialized = true;
                state.closed = false;
                Ok(BackendResponse::Unit)
            }
            (BackendOperation::Close, BackendRequest::Close) => {
                require_initialized(state, "close")?;
                state.closed = true;
                Ok(BackendResponse::Unit)
            }
            (BackendOperation::Navigate, BackendRequest::Navigate(request)) => {
                require_initialized(state, "navigate")?;
                state.url = request.url;
                state.clicked = false;
                state.typed_text.clear();
                state.revision = state.revision.saturating_add(1);
                Ok(BackendResponse::Navigation(NavigationResult {
                    url: state.url.clone(),
                    revision: state.revision,
                }))
            }
            (BackendOperation::Contexts, BackendRequest::Contexts(_request)) => {
                require_initialized(state, "contexts")?;
                Ok(BackendResponse::Contexts(vec![BrowsingContext {
                    context_id: PROOF_CONTEXT_ID.into(),
                    url: state.url.clone(),
                    active: true,
                }]))
            }
            (BackendOperation::Evidence, BackendRequest::Evidence(request)) => {
                require_initialized(state, "evidence")?;
                require_context(&request.context_id)?;
                let mut visible_text = if state.clicked {
                    "proof:clicked".to_string()
                } else {
                    "proof:ready".to_string()
                };
                if !state.typed_text.is_empty() {
                    visible_text.push_str(" proof:typed=");
                    visible_text.push_str(&state.typed_text);
                }
                debug_assert!(visible_text.len() <= MAX_TEXT_BYTES);
                Ok(BackendResponse::Evidence(EvidenceResult {
                    context_id: PROOF_CONTEXT_ID.into(),
                    revision: state.revision,
                    url: state.url.clone(),
                    title: PROOF_TITLE.into(),
                    visible_text,
                    complete: !matches!(
                        request.level,
                        EvidenceLevel::Screenshot | EvidenceLevel::Combined
                    ),
                }))
            }
            (BackendOperation::Action, BackendRequest::Action(request)) => {
                require_initialized(state, "action")?;
                require_context(&request.context_id)?;
                match request.action {
                    SemanticAction::Click { target } if target == PROOF_CLICK_TARGET => {
                        state.clicked = true;
                    }
                    SemanticAction::Type { target, text } if target == PROOF_TYPE_TARGET => {
                        if text.len() > PROOF_MAX_TYPED_BYTES {
                            return Err(invalid_action("typed proof text exceeds bounded state"));
                        }
                        state.typed_text = text;
                    }
                    SemanticAction::Click { .. } | SemanticAction::Type { .. } => {
                        return Err(invalid_action("unknown proof target"));
                    }
                    SemanticAction::KeyPress { .. } | SemanticAction::Scroll { .. } => {
                        return Err(invalid_action("proof backend supports click and type only"));
                    }
                }
                state.revision = state.revision.saturating_add(1);
                Ok(BackendResponse::Action(ActionResult {
                    context_id: PROOF_CONTEXT_ID.into(),
                    revision: state.revision,
                    accepted: true,
                }))
            }
            (BackendOperation::Effects, BackendRequest::Effects(request)) => {
                require_initialized(state, "effects")?;
                require_context(&request.context_id)?;
                if request.since_revision > state.revision {
                    return Err(BrowserBackendError::InvalidConfiguration {
                        field: "since revision".into(),
                        reason: "since revision cannot exceed current proof revision".into(),
                    });
                }
                Ok(BackendResponse::Effects(
                    crate::browser_backend::EffectsResult {
                        context_id: PROOF_CONTEXT_ID.into(),
                        revision: state.revision,
                        changed: request.since_revision < state.revision,
                    },
                ))
            }
            (operation, _) => Err(BrowserBackendError::UnsupportedOperation {
                operation: operation_name(operation).into(),
                reason: "proof backend does not implement this semantic operation".into(),
            }),
        }
    }
}

impl BrowserBackend for ProofBackend {
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
            let mut state = self
                .state
                .lock()
                .map_err(|_| BrowserBackendError::Lifecycle {
                    operation: "dispatch".into(),
                    state: "poisoned".into(),
                    reason: "proof state lock is unavailable".into(),
                })?;
            self.dispatch_locked(operation, request, &mut state)
        })
    }
}

fn require_initialized(state: &ProofState, operation: &str) -> Result<(), BrowserBackendError> {
    if !state.initialized || state.closed {
        return Err(BrowserBackendError::Lifecycle {
            operation: operation.into(),
            state: if state.closed {
                "closed"
            } else {
                "uninitialized"
            }
            .into(),
            reason: "initialize the proof backend before semantic operations".into(),
        });
    }
    Ok(())
}

fn require_context(context_id: &str) -> Result<(), BrowserBackendError> {
    if context_id == PROOF_CONTEXT_ID {
        return Ok(());
    }
    Err(BrowserBackendError::InvalidConfiguration {
        field: "context id".into(),
        reason: "proof backend accepts only its deterministic context".into(),
    })
}

fn invalid_action(reason: &str) -> BrowserBackendError {
    BrowserBackendError::UnsupportedOperation {
        operation: "action".into(),
        reason: reason.into(),
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
    use super::*;
    use crate::browser_backend::{
        ActionRequest, BrowserBackendDispatcher, EffectsRequest, EvidenceLevel, EvidenceRequest,
        NavigationRequest, ScriptRequest,
    };
    #[tokio::test]
    async fn proof_flow_runs_only_through_dispatcher() {
        let backend = ProofBackend::new().unwrap();
        let dispatcher = BrowserBackendDispatcher::new(&backend);

        dispatcher.initialize().await.unwrap();
        assert_eq!(backend.profile().identity.backend_id, PROOF_BACKEND_ID);
        assert_eq!(
            backend.profile().identity.certification.level,
            CertificationLevel::Experimental
        );
        assert!(
            !backend
                .profile()
                .capabilities
                .contains_key(&BrowserCapability::Script)
        );
        let navigation = dispatcher
            .navigate(NavigationRequest {
                url: "proof://example".into(),
            })
            .await
            .unwrap();
        let contexts = dispatcher
            .contexts(crate::browser_backend::ContextRequest {
                include_background: false,
            })
            .await
            .unwrap();
        assert_eq!(contexts.len(), 1);
        assert_eq!(contexts[0].context_id, PROOF_CONTEXT_ID);

        let initial = dispatcher
            .evidence(EvidenceRequest {
                context_id: PROOF_CONTEXT_ID.into(),
                level: EvidenceLevel::Compact,
            })
            .await
            .unwrap();
        assert_eq!(initial.visible_text, "proof:ready");

        let click = dispatcher
            .action(ActionRequest {
                context_id: PROOF_CONTEXT_ID.into(),
                action: SemanticAction::Click {
                    target: PROOF_CLICK_TARGET.into(),
                },
            })
            .await
            .unwrap();
        let type_action = dispatcher
            .action(ActionRequest {
                context_id: PROOF_CONTEXT_ID.into(),
                action: SemanticAction::Type {
                    target: PROOF_TYPE_TARGET.into(),
                    text: "deterministic".into(),
                },
            })
            .await
            .unwrap();
        assert!(type_action.revision > click.revision);

        let effects = dispatcher
            .effects(EffectsRequest {
                context_id: PROOF_CONTEXT_ID.into(),
                since_revision: navigation.revision,
            })
            .await
            .unwrap();
        assert!(effects.changed);
        assert_eq!(effects.revision, type_action.revision);

        let after = dispatcher
            .evidence(EvidenceRequest {
                context_id: PROOF_CONTEXT_ID.into(),
                level: EvidenceLevel::Combined,
            })
            .await
            .unwrap();
        assert_eq!(
            after.visible_text,
            "proof:clicked proof:typed=deterministic"
        );
        assert!(!after.complete);
        assert!(
            !dispatcher
                .effects(EffectsRequest {
                    context_id: PROOF_CONTEXT_ID.into(),
                    since_revision: after.revision,
                })
                .await
                .unwrap()
                .changed
        );
        dispatcher.close().await.unwrap();
    }

    #[tokio::test]
    async fn omitted_capability_fails_closed_through_dispatcher() {
        let backend = ProofBackend::new().unwrap();
        let dispatcher = BrowserBackendDispatcher::new(&backend);
        let error = dispatcher
            .script(ScriptRequest {
                context_id: PROOF_CONTEXT_ID.into(),
                source: "1 + 1".into(),
            })
            .await
            .unwrap_err();
        assert!(matches!(
            error,
            BrowserBackendError::CapabilityUnavailable {
                capability: BrowserCapability::Script,
                actual: SupportLevel::Unavailable,
                declared: false,
                ..
            }
        ));
    }

    #[tokio::test]
    async fn dispatcher_rejects_incompatible_protocol_response() {
        struct ShockBackend {
            profile: BackendProfile,
        }
        impl BrowserBackend for ShockBackend {
            fn profile(&self) -> &BackendProfile {
                &self.profile
            }
            fn dispatch<'a>(
                &'a self,
                _operation: BackendOperation,
                _request: BackendRequest,
            ) -> BackendFuture<'a, BackendResponse> {
                Box::pin(async { Ok(BackendResponse::Unit) })
            }
        }

        let backend = ShockBackend {
            profile: ProofBackend::profile_for("0.3.1").unwrap(),
        };
        let dispatcher = BrowserBackendDispatcher::new(&backend);
        let error = dispatcher
            .navigate(NavigationRequest {
                url: "proof://shock".into(),
            })
            .await
            .unwrap_err();
        assert!(matches!(
            error,
            BrowserBackendError::UnsupportedOperation { operation, .. } if operation == "navigate"
        ));
    }

    #[tokio::test]
    async fn stale_context_and_malformed_payload_fail_without_partial_effects() {
        let backend = ProofBackend::new().unwrap();
        let dispatcher = BrowserBackendDispatcher::new(&backend);
        dispatcher.initialize().await.unwrap();
        let navigation = dispatcher
            .navigate(NavigationRequest {
                url: "proof://shock".into(),
            })
            .await
            .unwrap();

        let stale = dispatcher
            .action(ActionRequest {
                context_id: "stale-context".into(),
                action: SemanticAction::Click {
                    target: PROOF_CLICK_TARGET.into(),
                },
            })
            .await
            .unwrap_err();
        assert!(matches!(
            stale,
            BrowserBackendError::InvalidConfiguration { field, .. } if field == "context id"
        ));

        let malformed = dispatcher
            .navigate(NavigationRequest {
                url: "x".repeat(crate::browser_backend::MAX_TEXT_BYTES + 1),
            })
            .await
            .unwrap_err();
        assert!(matches!(
            malformed,
            BrowserBackendError::InvalidConfiguration { field, .. } if field == "navigation url"
        ));

        let evidence = dispatcher
            .evidence(EvidenceRequest {
                context_id: PROOF_CONTEXT_ID.into(),
                level: EvidenceLevel::Compact,
            })
            .await
            .unwrap();
        assert_eq!(evidence.revision, navigation.revision);
        assert_eq!(evidence.visible_text, "proof:ready");
    }
}
