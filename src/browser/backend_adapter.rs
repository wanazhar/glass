//! CDP-backed implementation of the transport-neutral browser backend.
//!
//! The adapter owns a [`BrowserSession`] and translates the typed backend
//! protocol into the existing session API. CDP types stay below this module;
//! callers only observe `browser_backend` contracts and stable errors.

use super::session::{BrowserSession, PageTargetInfo, SemanticObservationLevel};
use crate::browser_backend::{
    ActionRequest, ActionResult, BackendContract, BackendFuture, BackendOperation, BackendProfile,
    BackendRequest, BackendResponse, BrowserBackend, BrowserBackendError, BrowserCapability,
    BrowsingContext, CaptureFormat, CaptureResult, EvidenceLevel, EvidenceResult, NavigationResult,
    PromptDecision, PromptResult, ScriptResult, SemanticAction, StorageOperation, StorageResult,
    StorageScope, SupportLevel,
};
use std::collections::BTreeMap;
use std::error::Error;
use std::sync::atomic::Ordering;
use tokio::sync::Mutex;

const DEFAULT_GLASS_VERSION: &str = "0.3.1";
const CDP_BACKEND_ID: &str = "cdp";
const CDP_BACKEND_VERSION: &str = "1";

/// The first Glass backend implementation, backed by the existing CDP session.
///
/// The session is owned so the lifecycle operation can close an owned browser
/// without exposing CDP or requiring a second browser runtime.
pub struct CdpBrowserBackend {
    session: Mutex<Option<BrowserSession>>,
    profile: BackendProfile,
}

impl CdpBrowserBackend {
    /// Wrap an existing session using the current Glass version for
    /// certification metadata.
    pub fn new(session: BrowserSession) -> Result<Self, BrowserBackendError> {
        Self::with_glass_version(session, DEFAULT_GLASS_VERSION)
    }

    /// Wrap an existing session with an explicit Glass version in the profile.
    pub fn with_glass_version(
        session: BrowserSession,
        glass_version: &str,
    ) -> Result<Self, BrowserBackendError> {
        let profile = Self::profile_for(glass_version)?;
        Ok(Self {
            session: Mutex::new(Some(session)),
            profile,
        })
    }

    /// Return the CDP capability declaration without opening a browser.
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
            BrowserCapability::Script,
            BrowserCapability::Capture,
            BrowserCapability::Storage,
            BrowserCapability::Prompts,
        ];
        let unavailable = [BrowserCapability::Effects, BrowserCapability::Downloads];
        let mut capabilities = BTreeMap::new();
        for capability in supported {
            capabilities.insert(
                capability,
                crate::browser_backend::CapabilityDescriptor {
                    level: SupportLevel::Available,
                    portability: match capability {
                        BrowserCapability::Script => crate::browser_backend::Portability::BackendSpecific,
                        BrowserCapability::Capture => crate::browser_backend::Portability::BackendCapabilityDependent,
                        _ => crate::browser_backend::Portability::SemanticPortable,
                    },
                    dependencies: Vec::new(),
                    limitations: Vec::new(),
                },
            );
        }
        for capability in unavailable {
            capabilities.insert(
                capability,
                crate::browser_backend::CapabilityDescriptor {
                    level: SupportLevel::Unavailable,
                    portability: crate::browser_backend::Portability::NonPortable,
                    dependencies: Vec::new(),
                    limitations: vec!["not implemented by the CDP BrowserSession boundary".into()],
                },
            );
        }
        let profile = BackendProfile {
            schema_version: crate::browser_backend::BROWSER_BACKEND_SCHEMA_VERSION,
            identity: crate::browser_backend::BackendIdentity {
                backend_id: CDP_BACKEND_ID.into(),
                version: CDP_BACKEND_VERSION.into(),
                browser: crate::browser_backend::BrowserVersionRange {
                    family: "chromium".into(),
                    minimum: None,
                    maximum: None,
                },
                certification: crate::browser_backend::CertificationProfile {
                    level: crate::browser_backend::CertificationLevel::Partial,
                    glass_version: glass_version.into(),
                    tested_capabilities: supported.to_vec(),
                    limitations: vec![
                        "CDP is the internal transport; callers use semantic backend contracts".into(),
                        "Effects and downloads remain unavailable at this adapter boundary".into(),
                    ],
                },
            },
            capabilities,
        };
        profile.validate()?;
        Ok(profile)
    }

    async fn select_context(
        session: &BrowserSession,
        context_id: &str,
        operation: &str,
    ) -> Result<(), BrowserBackendError> {
        let active = session.topology.lock().await.active_target_id.clone();
        if active.as_deref() == Some(context_id) {
            return Ok(());
        }
        session
            .select_target(context_id)
            .await
            .map(|_| ())
            .map_err(|error| translate_error(operation, error.as_ref()))
    }

    async fn session_error(
        &self,
        operation: BackendOperation,
    ) -> Result<tokio::sync::MutexGuard<'_, Option<BrowserSession>>, BrowserBackendError> {
        let guard = self.session.lock().await;
        if guard.is_none() {
            return Err(BrowserBackendError::Lifecycle {
                operation: operation_name(operation).into(),
                state: "closed".into(),
                reason: "the CDP browser session has already been closed".into(),
            });
        }
        Ok(guard)
    }
}

impl BrowserBackend for CdpBrowserBackend {
    fn profile(&self) -> &BackendProfile {
        &self.profile
    }

    fn dispatch<'a>(
        &'a self,
        operation: BackendOperation,
        request: BackendRequest,
    ) -> BackendFuture<'a, BackendResponse> {
        Box::pin(async move {
            let result = async {
                request.validate()?;
                // Keep direct trait callers safe as well as callers that use the
                // mandatory BrowserBackendDispatcher gate.
                self.profile.require_operation(operation, SupportLevel::Available)?;
                match (operation, request) {
                (BackendOperation::Initialize, BackendRequest::Initialize) => {
                    let _guard = self.session_error(operation).await?;
                    Ok(BackendResponse::Unit)
                }
                (BackendOperation::Close, BackendRequest::Close) => {
                    let mut guard = self.session.lock().await;
                    let Some(session) = guard.take() else {
                        return Err(BrowserBackendError::Lifecycle {
                            operation: "close".into(),
                            state: "closed".into(),
                            reason: "the CDP browser session has already been closed".into(),
                        });
                    };
                    session
                        .close()
                        .await
                        .map_err(|error| translate_error("close", error.as_ref()))?;
                    Ok(BackendResponse::Unit)
                }
                (BackendOperation::Navigate, BackendRequest::Navigate(request)) => {
                    let guard = self.session_error(operation).await?;
                    let session = guard.as_ref().expect("session checked above");
                    let page = session
                        .navigate(&request.url)
                        .await
                        .map_err(|error| translate_error("navigate", error.as_ref()))?;
                    Ok(BackendResponse::Navigation(NavigationResult {
                        url: page.url,
                        revision: session.page_revision.load(Ordering::Relaxed),
                    }))
                }
                (BackendOperation::Contexts, BackendRequest::Contexts(request)) => {
                    let guard = self.session_error(operation).await?;
                    let session = guard.as_ref().expect("session checked above");
                    let targets = session
                        .list_targets()
                        .await
                        .map_err(|error| translate_error("contexts", error.as_ref()))?;
                    Ok(BackendResponse::Contexts(targets_to_contexts(
                        targets,
                        request.include_background,
                    )))
                }
                (BackendOperation::Evidence, BackendRequest::Evidence(request)) => {
                    let guard = self.session_error(operation).await?;
                    let session = guard.as_ref().expect("session checked above");
                    Self::select_context(session, &request.context_id, "evidence").await?;
                    if matches!(request.level, EvidenceLevel::Screenshot) {
                        return Err(unsupported("evidence", "screenshot evidence uses capture"));
                    }
                    let observed = session
                        .semantic_observe(SemanticObservationLevel::Structured)
                        .await
                        .map_err(|error| translate_error("evidence", error.as_ref()))?;
                    let visible_text = observed.text.unwrap_or_default();
                    Ok(BackendResponse::Evidence(EvidenceResult {
                        context_id: request.context_id,
                        revision: observed.revision,
                        url: observed.page.url,
                        title: observed.page.title,
                        visible_text,
                        complete: !observed.limits.truncated && !observed.limits.text_truncated,
                    }))
                }
                (BackendOperation::Action, BackendRequest::Action(request)) => {
                    let guard = self.session_error(operation).await?;
                    let session = guard.as_ref().expect("session checked above");
                    Self::select_context(session, &request.context_id, "action").await?;
                    let outcome = execute_action(session, &request).await?;
                    Ok(BackendResponse::Action(outcome))
                }
                (BackendOperation::Script, BackendRequest::Script(request)) => {
                    let guard = self.session_error(operation).await?;
                    let session = guard.as_ref().expect("session checked above");
                    Self::select_context(session, &request.context_id, "script").await?;
                    let value = session
                        .evaluate(&request.source)
                        .await
                        .map_err(|error| translate_error("script", error.as_ref()))?;
                    Ok(BackendResponse::Script(ScriptResult { value }))
                }
                (BackendOperation::Capture, BackendRequest::Capture(request)) => {
                    let guard = self.session_error(operation).await?;
                    let session = guard.as_ref().expect("session checked above");
                    Self::select_context(session, &request.context_id, "capture").await?;
                    if request.format != CaptureFormat::Png {
                        return Err(unsupported(
                            "capture",
                            "the CDP session adapter currently exposes PNG capture only",
                        ));
                    }
                    let bytes = session
                        .screenshot_png()
                        .await
                        .map_err(|error| translate_error("capture", error.as_ref()))?;
                    Ok(BackendResponse::Capture(CaptureResult {
                        format: CaptureFormat::Png,
                        bytes,
                    }))
                }
                (BackendOperation::Storage, BackendRequest::Storage(request)) => {
                    let guard = self.session_error(operation).await?;
                    let session = guard.as_ref().expect("session checked above");
                    Self::select_context(session, &request.context_id, "storage").await?;
                    let entries =
                        storage_operation(session, &request.scope, &request.operation).await?;
                    Ok(BackendResponse::Storage(StorageResult { entries }))
                }
                (BackendOperation::Prompt, BackendRequest::Prompt(request)) => {
                    let guard = self.session_error(operation).await?;
                    let session = guard.as_ref().expect("session checked above");
                    Self::select_context(session, &request.context_id, "prompt").await?;
                    match request.decision {
                        PromptDecision::Accept => session
                            .accept_dialog()
                            .await
                            .map_err(|error| translate_error("prompt", error.as_ref()))?,
                        PromptDecision::Dismiss => session
                            .dismiss_dialog()
                            .await
                            .map_err(|error| translate_error("prompt", error.as_ref()))?,
                    }
                    Ok(BackendResponse::Prompt(PromptResult { handled: true }))
                }
                (operation, _) => Err(unsupported(
                    operation_name(operation),
                    "request variant does not match the operation",
                )),
                }
            }
            .await;
            validate_dispatch_result(result)
        })

    }
}
fn validate_dispatch_result(
    result: Result<BackendResponse, BrowserBackendError>,
) -> Result<BackendResponse, BrowserBackendError> {
    match &result {
        Ok(response) => response.validate()?,
        Err(error) => error.validate()?,
    }
    result
}

async fn execute_action(
    session: &BrowserSession,
    request: &ActionRequest,
) -> Result<ActionResult, BrowserBackendError> {
    let outcome = match &request.action {
        SemanticAction::Click { target } => session
            .click(target)
            .await
            .map_err(|error| translate_error("action", error.as_ref()))?,
        SemanticAction::Type { target, text } => session
            .type_text(text, Some(target))
            .await
            .map_err(|error| translate_error("action", error.as_ref()))?,
        SemanticAction::KeyPress { key } => session
            .key_press(key)
            .await
            .map_err(|error| translate_error("action", error.as_ref()))?,
        SemanticAction::Scroll { delta_x, delta_y } => session
            .scroll(f64::from(*delta_x), f64::from(*delta_y))
            .await
            .map_err(|error| translate_error("action", error.as_ref()))?,
    };
    Ok(ActionResult {
        context_id: request.context_id.clone(),
        revision: outcome.current_revision,
        accepted: true,
    })
}

async fn storage_operation(
    session: &BrowserSession,
    scope: &StorageScope,
    operation: &StorageOperation,
) -> Result<BTreeMap<String, String>, BrowserBackendError> {
    match (scope, operation) {
        (StorageScope::Cookies, StorageOperation::Read) => read_storage(session, scope).await,
        (StorageScope::Cookies, StorageOperation::Clear) => Err(unsupported(
            "storage",
            "cookie clear is global in CDP and is not exposed as a context-scoped operation",
        )),
        (StorageScope::Cookies, StorageOperation::Write { .. }) => Err(unsupported(
            "storage",
            "cookie writes require cookie metadata outside the semantic map",
        )),
        (scope @ (StorageScope::Local | StorageScope::Session), StorageOperation::Read) => {
            read_storage(session, scope).await
        }
        (scope @ (StorageScope::Local | StorageScope::Session), StorageOperation::Write { key, value }) => {
            let storage = storage_name(scope);
            let key =
                serde_json::to_string(key).map_err(|error| translate_error("storage", &error))?;
            let value =
                serde_json::to_string(value).map_err(|error| translate_error("storage", &error))?;
            session
                .evaluate(&format!("window.{storage}.setItem({key}, {value}); true"))
                .await
                .map_err(|error| translate_error("storage", error.as_ref()))?;
            read_storage(session, scope).await
        }
        (scope @ (StorageScope::Local | StorageScope::Session), StorageOperation::Clear) => {
            let storage = storage_name(scope);
            session
                .evaluate(&format!("window.{storage}.clear(); true"))
                .await
                .map_err(|error| translate_error("storage", error.as_ref()))?;
            Ok(BTreeMap::new())
        }
    }
}

async fn read_storage(
    session: &BrowserSession,
    scope: &StorageScope,
) -> Result<BTreeMap<String, String>, BrowserBackendError> {
    match scope {
        StorageScope::Cookies => {
            let cookies = session
                .cookies()
                .await
                .map_err(|error| translate_error("storage", error.as_ref()))?;
            Ok(cookies
                .into_iter()
                .map(|cookie| (cookie.name, cookie.value))
                .collect())
        }
        StorageScope::Local | StorageScope::Session => {
            let items = match scope {
                StorageScope::Local => session.local_storage().await,
                StorageScope::Session => session.session_storage().await,
                StorageScope::Cookies => unreachable!(),
            }
            .map_err(|error| translate_error("storage", error.as_ref()))?;
            Ok(items
                .items
                .into_iter()
                .map(|item| (item.key, item.value))
                .collect())
        }
    }
}

fn storage_name(scope: &StorageScope) -> &'static str {
    match scope {
        StorageScope::Local => "localStorage",
        StorageScope::Session => "sessionStorage",
        StorageScope::Cookies => "document.cookie",
    }
}

fn targets_to_contexts(targets: Vec<PageTargetInfo>, include_background: bool) -> Vec<BrowsingContext> {
    targets
        .into_iter()
        .filter(|target| include_background || target.active)
        .map(|target| BrowsingContext {
            context_id: target.id,
            url: target.url,
            active: target.active,
        })
        .collect()
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

fn unsupported(operation: &str, reason: &str) -> BrowserBackendError {
    BrowserBackendError::UnsupportedOperation {
        operation: operation.into(),
        reason: reason.into(),
    }
}

fn translate_error(operation: &str, error: &dyn Error) -> BrowserBackendError {
    let reason = bounded_error(error.to_string());
    let lower = reason.to_ascii_lowercase();
    if lower.contains("unsupported") || lower.contains("not supported") || lower.contains("policy") {
        return unsupported(operation, &reason);
    }
    if matches!(operation, "initialize" | "close") {
        return BrowserBackendError::Lifecycle {
            operation: operation.into(),
            state: "failed".into(),
            reason,
        };
    }
    BrowserBackendError::Connection {
        operation: operation.into(),
        reason,
    }
}

fn bounded_error(mut reason: String) -> String {
    if reason.len() <= crate::browser_backend::MAX_DIAGNOSTIC_BYTES {
        return reason;
    }
    let limit = crate::browser_backend::MAX_DIAGNOSTIC_BYTES.saturating_sub(3);
    while !reason.is_char_boundary(limit) {
        reason.pop();
    }
    reason.truncate(limit);
    reason.push_str("...");
    reason
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::browser_backend::BackendContract;
    use std::io;

    #[test]
    fn cdp_profile_declares_supported_and_unavailable_capabilities() {
        let profile = CdpBrowserBackend::profile_for("test").expect("valid profile");
        assert_eq!(profile.identity.backend_id, "cdp");
        assert_eq!(profile.capability(BrowserCapability::Navigation).level, SupportLevel::Available);
        assert_eq!(profile.capability(BrowserCapability::Downloads).level, SupportLevel::Unavailable);
        assert!(profile.identity.certification.tested_capabilities.contains(&BrowserCapability::Action));
    }

    #[test]
    fn error_translation_is_stable_and_bounded() {
        let error = io::Error::other("CDP websocket response timeout");
        let translated = translate_error("navigate", &error);
        assert!(matches!(translated, BrowserBackendError::Connection { operation, .. } if operation == "navigate"));
        let oversized = io::Error::other("x".repeat(crate::browser_backend::MAX_DIAGNOSTIC_BYTES + 100));
        let translated = translate_error("action", &oversized);
        assert!(translated.validate().is_ok());
    }

    #[test]
    fn unsupported_operation_translation_is_typed() {
        let error = io::Error::other("operation not supported by this CDP session");
        assert!(matches!(translate_error("download", &error), BrowserBackendError::UnsupportedOperation { operation, .. } if operation == "download"));
    }
}
