//! Semantic backend adapter for the Glass-owned native engine.
//!
//! The adapter is intentionally small: the native engine owns DOM and
//! lifecycle state, while this module translates only the stable
//! `browser_backend` contract.

use super::native_engine::{
    NativeAction, NativeEngine, NativeEngineConfig, NativeEngineError, NativeInspectionSnapshot,
    NativePreflightAction, NativeTargetPreflight,
};
use crate::browser_backend::{
    ActionResult, BROWSER_BACKEND_SCHEMA_VERSION, BackendFuture, BackendOperation, BackendProfile,
    BackendRequest, BackendResponse, BrowserBackend, BrowserBackendError, BrowserCapability,
    BrowsingContext, CapabilityDescriptor, CaptureFormat, CaptureResult, CertificationLevel,
    CertificationProfile, EffectsResult, EvidenceLevel, EvidenceResult, NavigationResult,
    Portability, ScriptResult, SemanticAction, StorageOperation, StorageResult, StorageScope,
    SupportLevel,
};
use std::collections::BTreeMap;
use std::sync::Mutex;

/// Stable backend ID for the Glass-owned native engine.
pub const NATIVE_ENGINE_BACKEND_ID: &str = "native-engine";
const NATIVE_ENGINE_BACKEND_VERSION: &str = "0.1";
const NATIVE_ENGINE_BROWSER_FAMILY: &str = "native";

/// Semantic adapter around one native engine instance.
pub struct NativeEngineBackend {
    profile: BackendProfile,
    engine: Mutex<NativeEngine>,
    storage: Mutex<NativeBackendStorage>,
}

/// Bounded state for explicit semantic backend storage calls.
///
/// This is intentionally separate from the page realm and resource loader.
/// It makes the semantic backend contract executable while page-visible Web
/// Storage, durable profiles, and the network cookie jar retain their own
/// origin-aware contracts.
#[derive(Debug, Default)]
struct NativeBackendStorage {
    local: BTreeMap<String, String>,
    session: BTreeMap<String, String>,
}

impl NativeBackendStorage {
    fn entries(
        &self,
        scope: &StorageScope,
    ) -> Result<&BTreeMap<String, String>, BrowserBackendError> {
        match scope {
            StorageScope::Local => Ok(&self.local),
            StorageScope::Session => Ok(&self.session),
            StorageScope::Cookies => Err(BrowserBackendError::UnsupportedOperation {
                operation: "storage".into(),
                reason: "native cookie storage is not yet exposed through the semantic backend map"
                    .into(),
            }),
        }
    }

    fn entries_mut(
        &mut self,
        scope: &StorageScope,
    ) -> Result<&mut BTreeMap<String, String>, BrowserBackendError> {
        match scope {
            StorageScope::Local => Ok(&mut self.local),
            StorageScope::Session => Ok(&mut self.session),
            StorageScope::Cookies => Err(BrowserBackendError::UnsupportedOperation {
                operation: "storage".into(),
                reason: "native cookie storage is not yet exposed through the semantic backend map"
                    .into(),
            }),
        }
    }
}

impl NativeEngineBackend {
    pub fn new(config: NativeEngineConfig) -> Result<Self, BrowserBackendError> {
        let profile = Self::profile_for(env!("CARGO_PKG_VERSION"))?;
        let engine = NativeEngine::new(config).map_err(native_error)?;
        Ok(Self {
            profile,
            engine: Mutex::new(engine),
            storage: Mutex::new(NativeBackendStorage::default()),
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
                BrowserCapability::Contexts => vec!["one active context only".into()],
                BrowserCapability::Storage => vec![
                    "origin-keyed page local/session Web Storage; opt-in revisioned localStorage and cookie profiles; profile-journal events; IndexedDB remains open".into(),
                ],
                BrowserCapability::Lifecycle => {
                    vec!["close is terminal for the engine instance".into()]
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
                    ],
                },
            },
            capabilities,
        };
        profile.validate()?;
        Ok(profile)
    }

    fn lock_engine(
        &self,
        operation: BackendOperation,
    ) -> Result<std::sync::MutexGuard<'_, NativeEngine>, BrowserBackendError> {
        self.engine
            .lock()
            .map_err(|_| BrowserBackendError::Lifecycle {
                operation: operation_name(operation).into(),
                state: "poisoned".into(),
                reason: "native engine state lock is unavailable".into(),
            })
    }

    fn lock_storage(
        &self,
        operation: BackendOperation,
    ) -> Result<std::sync::MutexGuard<'_, NativeBackendStorage>, BrowserBackendError> {
        self.storage
            .lock()
            .map_err(|_| BrowserBackendError::Lifecycle {
                operation: operation_name(operation).into(),
                state: "poisoned".into(),
                reason: "native backend storage state lock is unavailable".into(),
            })
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
            let mut engine = self.lock_engine(operation)?;
            let active_context_id = engine.config().context_id.clone();
            match (operation, request) {
                (BackendOperation::Initialize, BackendRequest::Initialize) => {
                    engine.initialize_async().await.map_err(native_error)?;
                    Ok(BackendResponse::Unit)
                }
                (BackendOperation::Close, BackendRequest::Close) => {
                    engine.close_async().await.map_err(native_error)?;
                    Ok(BackendResponse::Unit)
                }
                (BackendOperation::Navigate, BackendRequest::Navigate(request)) => {
                    let snapshot = engine
                        .navigate_async(request.url)
                        .await
                        .map_err(native_error)?;
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
                    drop(engine);
                    let mut storage = self.lock_storage(operation)?;
                    let entries = match request.operation {
                        StorageOperation::Read => storage.entries(&request.scope)?.clone(),
                        StorageOperation::Write { key, value } => {
                            let entries = storage.entries_mut(&request.scope)?;
                            entries.insert(key, value);
                            entries.clone()
                        }
                        StorageOperation::Clear => {
                            let entries = storage.entries_mut(&request.scope)?;
                            entries.clear();
                            entries.clone()
                        }
                    };
                    Ok(BackendResponse::Storage(StorageResult { entries }))
                }
                (operation, _) => Err(BrowserBackendError::UnsupportedOperation {
                    operation: operation_name(operation).into(),
                    reason: "native engine does not implement this operation".into(),
                }),
            }
        })
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
