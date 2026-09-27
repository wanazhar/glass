//! Bounded W3C WebDriver backend for browser runtimes without a certified
//! direct BiDi endpoint.
//!
//! Safari currently exposes its supported automation path through
//! `safaridriver`. This adapter keeps that HTTP wire protocol below the same
//! transport-neutral semantic backend boundary used by CDP and BiDi. It is a
//! deliberately small slice: navigation, one active window context, script,
//! evidence, click/type actions, and revision effects.

use crate::browser_backend::{
    ActionResult, BROWSER_BACKEND_SCHEMA_VERSION, BackendContract, BackendFuture, BackendOperation,
    BackendProfile, BackendRequest, BackendResponse, BrowserBackend, BrowserBackendError,
    BrowserCapability, BrowsingContext, CapabilityDependency, CapabilityDescriptor, EffectsResult,
    EvidenceLevel, EvidenceResult, NavigationResult, Portability, ScriptResult, SemanticAction,
    SupportLevel,
};
use futures_util::StreamExt;
use reqwest::{Client, Method, StatusCode};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::time::Duration;
use tokio::sync::Mutex;
use tokio::time::timeout;
use url::Url;

const WEBDRIVER_BACKEND_ID: &str = "webdriver-classic";
const WEBDRIVER_BACKEND_VERSION: &str = "1";
const DEFAULT_COMMAND_TIMEOUT: Duration = Duration::from_secs(10);
const MAX_WIRE_BYTES: usize = 256 * 1024;

/// Configuration for a W3C WebDriver HTTP endpoint such as `safaridriver`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WebDriverBackendConfig {
    /// Driver server base URL, for example `http://127.0.0.1:4444`.
    pub endpoint: String,
    /// Browser family recorded in the backend profile.
    pub browser_family: String,
    /// W3C `browserName` requested during session creation.
    pub browser_name: String,
    /// Glass version recorded in the backend profile.
    pub glass_version: String,
    /// Per-request transport deadline.
    pub command_timeout: Duration,
}

impl WebDriverBackendConfig {
    /// Configure Safari through a local `safaridriver` HTTP server.
    pub fn for_safari(endpoint: impl Into<String>) -> Self {
        Self {
            endpoint: endpoint.into(),
            browser_family: "safari".into(),
            browser_name: "safari".into(),
            glass_version: env!("CARGO_PKG_VERSION").into(),
            command_timeout: DEFAULT_COMMAND_TIMEOUT,
        }
    }

    pub fn validate(&self) -> Result<(), BrowserBackendError> {
        let endpoint = Url::parse(self.endpoint.trim()).map_err(|error| {
            invalid(
                "webdriver endpoint",
                &format!("endpoint must be a valid URL: {error}"),
            )
        })?;
        if !matches!(endpoint.scheme(), "http" | "https") {
            return Err(invalid(
                "webdriver endpoint",
                "endpoint must use http or https",
            ));
        }
        if endpoint.host_str().is_none() {
            return Err(invalid(
                "webdriver endpoint",
                "endpoint must include a host",
            ));
        }
        if self.browser_family.is_empty() || self.browser_family.len() > 64 {
            return Err(invalid(
                "browser family",
                "family is empty or exceeds the bounded size",
            ));
        }
        if self.browser_name.is_empty() || self.browser_name.len() > 128 {
            return Err(invalid(
                "browser name",
                "name is empty or exceeds the bounded size",
            ));
        }
        if self.glass_version.is_empty() || self.glass_version.len() > 128 {
            return Err(invalid(
                "glass version",
                "version is empty or exceeds the bounded size",
            ));
        }
        if self.command_timeout.is_zero() || self.command_timeout > Duration::from_secs(60) {
            return Err(invalid(
                "command timeout",
                "timeout must be between one millisecond and sixty seconds",
            ));
        }
        Ok(())
    }
}

struct WebDriverState {
    initialized: bool,
    session_id: Option<String>,
    context_id: Option<String>,
    url: String,
    revision: u64,
}

/// A bounded W3C WebDriver backend.
pub struct WebDriverBrowserBackend {
    client: Client,
    endpoint: String,
    browser_name: String,
    command_timeout: Duration,
    profile: BackendProfile,
    state: Mutex<WebDriverState>,
}

impl WebDriverBrowserBackend {
    /// Construct a backend. The remote driver is contacted on initialize.
    pub fn new(config: WebDriverBackendConfig) -> Result<Self, BrowserBackendError> {
        config.validate()?;
        let profile = profile_for(&config)?;
        Ok(Self {
            client: Client::new(),
            endpoint: config.endpoint.trim_end_matches('/').to_owned(),
            browser_name: config.browser_name,
            command_timeout: config.command_timeout,
            profile,
            state: Mutex::new(WebDriverState {
                initialized: false,
                session_id: None,
                context_id: None,
                url: String::new(),
                revision: 0,
            }),
        })
    }

    pub fn profile_for(
        config: &WebDriverBackendConfig,
    ) -> Result<BackendProfile, BrowserBackendError> {
        profile_for(config)
    }

    async fn request(
        &self,
        method: Method,
        path: &str,
        body: Option<Value>,
    ) -> Result<Value, BrowserBackendError> {
        let url = format!("{}{}", self.endpoint, path);
        let mut request = self.client.request(method, url);
        if let Some(body) = body {
            request = request.json(&body);
        }
        let response = timeout(self.command_timeout, request.send())
            .await
            .map_err(|_| connection_error("request", "WebDriver request timed out"))?
            .map_err(|error| connection_error("request", &error.to_string()))?;
        let status = response.status();
        let content_length = response.content_length();
        if content_length.is_some_and(|length| length > MAX_WIRE_BYTES as u64) {
            return Err(connection_error("response", "response exceeds wire budget"));
        }
        let mut stream = response.bytes_stream();
        let bytes = timeout(self.command_timeout, async {
            let mut bytes = Vec::with_capacity(
                content_length
                    .unwrap_or_default()
                    .min(MAX_WIRE_BYTES as u64) as usize,
            );
            while let Some(chunk) = stream.next().await {
                let chunk =
                    chunk.map_err(|error| connection_error("response", &error.to_string()))?;
                if bytes.len().saturating_add(chunk.len()) > MAX_WIRE_BYTES {
                    return Err(connection_error("response", "response exceeds wire budget"));
                }
                bytes.extend_from_slice(&chunk);
            }
            Ok(bytes)
        })
        .await
        .map_err(|_| connection_error("response", "WebDriver response timed out"))??;
        let envelope = if bytes.is_empty() {
            Value::Null
        } else {
            serde_json::from_slice::<Value>(&bytes)
                .map_err(|error| connection_error("response", &format!("invalid JSON: {error}")))?
        };
        let value = envelope.get("value").cloned().unwrap_or(envelope.clone());
        if !status.is_success() {
            return Err(webdriver_error("request", status, &value));
        }
        if value.get("error").is_some() {
            return Err(webdriver_error("request", status, &value));
        }
        Ok(value)
    }

    async fn dispatch_inner(
        &self,
        operation: BackendOperation,
        request: BackendRequest,
    ) -> Result<BackendResponse, BrowserBackendError> {
        request.validate()?;
        self.profile
            .require_operation(operation, SupportLevel::Available)?;
        let mut state = self.state.lock().await;
        match (operation, request) {
            (BackendOperation::Initialize, BackendRequest::Initialize) => {
                if state.initialized {
                    return Err(lifecycle_error("initialize", "initialized"));
                }
                let value = self
                    .request(
                        Method::POST,
                        "/session",
                        Some(json!({
                            "capabilities": {"alwaysMatch": {"browserName": self.browser_name}}
                        })),
                    )
                    .await?;
                let session_id = value
                    .get("sessionId")
                    .and_then(Value::as_str)
                    .ok_or_else(|| connection_error("initialize", "response omitted sessionId"))?;
                validate_session_id(session_id)?;
                let handles = self
                    .request(
                        Method::GET,
                        &format!("/session/{session_id}/window/handles"),
                        None,
                    )
                    .await?;
                let context_id = handles
                    .as_array()
                    .and_then(|handles| handles.first())
                    .and_then(Value::as_str)
                    .ok_or_else(|| connection_error("initialize", "driver returned no window"))?;
                let url = self
                    .request(Method::GET, &format!("/session/{session_id}/url"), None)
                    .await?
                    .as_str()
                    .unwrap_or("about:blank")
                    .to_owned();
                state.session_id = Some(session_id.to_owned());
                state.context_id = Some(context_id.to_owned());
                state.url = nonempty_url(url);
                state.initialized = true;
                Ok(BackendResponse::Unit)
            }
            (BackendOperation::Close, BackendRequest::Close) => {
                let Some(session_id) = state.session_id.clone() else {
                    return Err(lifecycle_error("close", "closed"));
                };
                let result = self
                    .request(Method::DELETE, &format!("/session/{session_id}"), None)
                    .await;
                state.initialized = false;
                state.session_id = None;
                state.context_id = None;
                match result {
                    Ok(_) => Ok(BackendResponse::Unit),
                    Err(error) => Err(error),
                }
            }
            (BackendOperation::Navigate, BackendRequest::Navigate(request)) => {
                ensure_initialized(&state, "navigate")?;
                let session_id = session_id(&state)?;
                self.request(
                    Method::POST,
                    &format!("/session/{session_id}/url"),
                    Some(json!({"url": request.url})),
                )
                .await?;
                state.url = self
                    .request(Method::GET, &format!("/session/{session_id}/url"), None)
                    .await?
                    .as_str()
                    .unwrap_or(&state.url)
                    .to_owned();
                state.url = nonempty_url(std::mem::take(&mut state.url));
                state.revision = state.revision.saturating_add(1);
                Ok(BackendResponse::Navigation(NavigationResult {
                    url: state.url.clone(),
                    revision: state.revision,
                }))
            }
            (BackendOperation::Contexts, BackendRequest::Contexts(_)) => {
                ensure_initialized(&state, "contexts")?;
                let context_id = state
                    .context_id
                    .clone()
                    .ok_or_else(|| connection_error("contexts", "no active window"))?;
                let session_id = session_id(&state)?;
                state.url = self
                    .request(Method::GET, &format!("/session/{session_id}/url"), None)
                    .await?
                    .as_str()
                    .unwrap_or(&state.url)
                    .to_owned();
                state.url = nonempty_url(std::mem::take(&mut state.url));
                Ok(BackendResponse::Contexts(vec![BrowsingContext {
                    context_id,
                    url: state.url.clone(),
                    active: true,
                }]))
            }
            (BackendOperation::Evidence, BackendRequest::Evidence(request)) => {
                ensure_context(&state, &request.context_id, "evidence")?;
                let value = self
                    .execute(
                        &state,
                        "({url: location.href, title: document.title || '', text: document.body ? (document.body.innerText || '') : ''})",
                    )
                    .await?;
                let url = value
                    .get("url")
                    .and_then(Value::as_str)
                    .unwrap_or(&state.url)
                    .to_owned();
                let title = value
                    .get("title")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_owned();
                let visible_text = value
                    .get("text")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_owned();
                Ok(BackendResponse::Evidence(EvidenceResult {
                    context_id: request.context_id,
                    revision: state.revision,
                    url: nonempty_url(url),
                    title,
                    visible_text,
                    complete: !matches!(request.level, EvidenceLevel::Screenshot),
                }))
            }
            (BackendOperation::Script, BackendRequest::Script(request)) => {
                ensure_context(&state, &request.context_id, "script")?;
                Ok(BackendResponse::Script(ScriptResult {
                    value: self.execute(&state, &request.source).await?,
                }))
            }
            (BackendOperation::Action, BackendRequest::Action(request)) => {
                ensure_context(&state, &request.context_id, "action")?;
                let source = action_source(&request.action)?;
                let result = self.execute(&state, &source).await?;
                if result.as_bool() == Some(false) {
                    return Err(BrowserBackendError::UnsupportedOperation {
                        operation: "action".into(),
                        reason: "WebDriver target was not found or action was rejected".into(),
                    });
                }
                state.revision = state.revision.saturating_add(1);
                Ok(BackendResponse::Action(ActionResult {
                    context_id: request.context_id,
                    revision: state.revision,
                    accepted: true,
                }))
            }
            (BackendOperation::Effects, BackendRequest::Effects(request)) => {
                ensure_context(&state, &request.context_id, "effects")?;
                Ok(BackendResponse::Effects(EffectsResult {
                    context_id: request.context_id,
                    revision: state.revision,
                    changed: state.revision > request.since_revision,
                }))
            }
            (operation, _) => Err(BrowserBackendError::UnsupportedOperation {
                operation: operation_name(operation).into(),
                reason: "request variant does not match the operation".into(),
            }),
        }
    }

    async fn execute(
        &self,
        state: &WebDriverState,
        source: &str,
    ) -> Result<Value, BrowserBackendError> {
        let session_id = session_id(state)?;
        self.request(
            Method::POST,
            &format!("/session/{session_id}/execute/sync"),
            Some(json!({"script": source, "args": []})),
        )
        .await
    }
}

impl BrowserBackend for WebDriverBrowserBackend {
    fn profile(&self) -> &BackendProfile {
        &self.profile
    }

    fn dispatch<'a>(
        &'a self,
        operation: BackendOperation,
        request: BackendRequest,
    ) -> BackendFuture<'a, BackendResponse> {
        Box::pin(async move {
            let result = self.dispatch_inner(operation, request).await;
            if let Ok(response) = &result {
                response.validate()?;
            }
            if let Err(error) = &result {
                error.validate()?;
            }
            result
        })
    }
}

fn profile_for(config: &WebDriverBackendConfig) -> Result<BackendProfile, BrowserBackendError> {
    config.validate()?;
    let mut capabilities = BTreeMap::new();
    let supported = [
        BrowserCapability::Lifecycle,
        BrowserCapability::Navigation,
        BrowserCapability::Contexts,
        BrowserCapability::Evidence,
        BrowserCapability::Action,
        BrowserCapability::Effects,
        BrowserCapability::Script,
    ];
    for capability in BrowserCapability::ALL {
        let available = supported.contains(&capability);
        let dependencies = if available
            && matches!(
                capability,
                BrowserCapability::Evidence | BrowserCapability::Action
            ) {
            vec![CapabilityDependency {
                capability: BrowserCapability::Script,
                minimum: SupportLevel::Available,
                reason: "the WebDriver adapter derives this semantic operation through script evaluation"
                    .into(),
            }]
        } else {
            Vec::new()
        };
        capabilities.insert(
            capability,
            CapabilityDescriptor {
                level: if available {
                    SupportLevel::Available
                } else {
                    SupportLevel::Unavailable
                },
                portability: if available {
                    match capability {
                        BrowserCapability::Script => Portability::BackendSpecific,
                        _ => Portability::SemanticPortable,
                    }
                } else {
                    Portability::NonPortable
                },
                dependencies,
                limitations: if available {
                    Vec::new()
                } else {
                    vec!["capability is unavailable at the WebDriver boundary".into()]
                },
            },
        );
    }
    let profile = BackendProfile {
        schema_version: BROWSER_BACKEND_SCHEMA_VERSION,
        identity: crate::browser_backend::BackendIdentity {
            backend_id: format!("{WEBDRIVER_BACKEND_ID}-{}", config.browser_family),
            version: WEBDRIVER_BACKEND_VERSION.into(),
            browser: crate::browser_backend::BrowserVersionRange {
                family: config.browser_family.clone(),
                minimum: None,
                maximum: None,
            },
            certification: crate::browser_backend::CertificationProfile {
                level: crate::browser_backend::CertificationLevel::Experimental,
                glass_version: config.glass_version.clone(),
                tested_capabilities: supported.to_vec(),
                limitations: vec![
                    "WebDriver support is bounded to one active window, navigation, evidence, script, click/type, and effects".into(),
                    "capture, storage, prompts, downloads, keyboard, scrolling, and multi-window operations fail closed until certified".into(),
                ],
            },
        },
        capabilities,
    };
    profile.validate()?;
    Ok(profile)
}

fn action_source(action: &SemanticAction) -> Result<String, BrowserBackendError> {
    let selector = |target: &str| {
        let selector = target.strip_prefix("css=").unwrap_or(target);
        if selector.is_empty()
            || ["ref=", "role=", "text=", "name=", "ordinal="]
                .iter()
                .any(|prefix| target.starts_with(prefix))
        {
            return Err(BrowserBackendError::UnsupportedOperation {
                operation: "action".into(),
                reason: "Safari/WebDriver actions require a CSS selector (use css=...)".into(),
            });
        }
        Ok(serde_json::to_string(selector).unwrap_or_else(|_| "\"\"".into()))
    };
    match action {
        SemanticAction::Click { target } => Ok(format!(
            "(() => {{ const e = document.querySelector({}); if (!e) return false; e.click(); return true; }})()",
            selector(target)?
        )),
        SemanticAction::ClickWithModifiers { target, modifiers } => {
            if !modifiers.is_empty() {
                return Err(BrowserBackendError::UnsupportedOperation {
                    operation: "action".into(),
                    reason: "modifier-aware click is implemented by the native browser runtime only".into(),
                });
            }
            Ok(format!(
                "(() => {{ const e = document.querySelector({}); if (!e) return false; e.click(); return true; }})()",
                selector(target)?
            ))
        }
        SemanticAction::Type { target, text } => Ok(format!(
            "(() => {{ const e = document.querySelector({}); if (!e) return false; e.focus(); e.value = {}; e.dispatchEvent(new Event('input', {{bubbles: true}})); e.dispatchEvent(new Event('change', {{bubbles: true}})); return true; }})()",
            selector(target)?,
            serde_json::to_string(text).unwrap_or_else(|_| "\"\"".into())
        )),
        SemanticAction::DoubleClick { .. }
        | SemanticAction::Hover { .. }
        | SemanticAction::Drag { .. }
        | SemanticAction::Clear { .. }
        | SemanticAction::Check { .. }
        | SemanticAction::Uncheck { .. }
        | SemanticAction::Select { .. }
        | SemanticAction::KeyDown { .. }
        | SemanticAction::KeyUp { .. }
        | SemanticAction::Shortcut { .. }
        | SemanticAction::KeyPress { .. }
        | SemanticAction::Scroll { .. } => {
            Err(BrowserBackendError::UnsupportedOperation {
                operation: "action".into(),
                reason: "WebDriver adapter only certifies click and type input; form-control actions are not certified".into(),
            })
        }
    }
}

fn session_id(state: &WebDriverState) -> Result<&str, BrowserBackendError> {
    state
        .session_id
        .as_deref()
        .ok_or_else(|| lifecycle_error("request", "not initialized"))
}

fn validate_session_id(value: &str) -> Result<(), BrowserBackendError> {
    if value.is_empty()
        || value.len() > crate::browser_backend::MAX_BACKEND_ID_BYTES
        || !value
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || "-_.:".contains(character))
    {
        return Err(connection_error(
            "initialize",
            "driver returned an invalid session id",
        ));
    }
    Ok(())
}

fn ensure_initialized(state: &WebDriverState, operation: &str) -> Result<(), BrowserBackendError> {
    if state.initialized {
        Ok(())
    } else {
        Err(lifecycle_error(operation, "not initialized"))
    }
}

fn ensure_context(
    state: &WebDriverState,
    context: &str,
    operation: &str,
) -> Result<(), BrowserBackendError> {
    ensure_initialized(state, operation)?;
    if state.context_id.as_deref() == Some(context) {
        Ok(())
    } else {
        Err(BrowserBackendError::UnsupportedOperation {
            operation: operation.into(),
            reason: "context is not the active WebDriver window".into(),
        })
    }
}

fn nonempty_url(url: String) -> String {
    if url.is_empty() {
        "about:blank".into()
    } else {
        url
    }
}

fn webdriver_error(operation: &str, status: StatusCode, value: &Value) -> BrowserBackendError {
    let detail = value
        .get("message")
        .and_then(Value::as_str)
        .or_else(|| value.get("error").and_then(Value::as_str))
        .unwrap_or("WebDriver request failed");
    connection_error(operation, &format!("HTTP {status}: {detail}"))
}

fn invalid(field: &str, reason: &str) -> BrowserBackendError {
    BrowserBackendError::InvalidConfiguration {
        field: field.into(),
        reason: reason.into(),
    }
}

fn connection_error(operation: &str, reason: &str) -> BrowserBackendError {
    BrowserBackendError::Connection {
        operation: operation.into(),
        reason: reason
            .chars()
            .take(crate::browser_backend::MAX_DIAGNOSTIC_BYTES)
            .collect(),
    }
}

fn lifecycle_error(operation: &str, state: &str) -> BrowserBackendError {
    BrowserBackendError::Lifecycle {
        operation: operation.into(),
        state: state.into(),
        reason: "WebDriver backend lifecycle state does not permit this operation".into(),
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
        _ => "unsupported",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::browser_backend::{
        ActionRequest, BrowserBackendDispatcher, ContextRequest, EffectsRequest, EvidenceRequest,
        NavigationRequest, ScriptRequest,
    };
    use serde_json::json;
    use std::sync::{Arc, Mutex};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    #[test]
    fn safari_profile_declares_family_and_fail_closed_features() {
        let profile = WebDriverBrowserBackend::profile_for(&WebDriverBackendConfig::for_safari(
            "http://127.0.0.1:4444",
        ))
        .unwrap();
        assert_eq!(profile.identity.backend_id, "webdriver-classic-safari");
        assert_eq!(profile.identity.browser.family, "safari");
        assert_eq!(
            profile.capability(BrowserCapability::Capture).level,
            SupportLevel::Unavailable
        );
        profile.validate().unwrap();
    }

    #[test]
    fn webdriver_endpoint_validation_rejects_missing_host() {
        let config = WebDriverBackendConfig::for_safari("https://");
        let error = config.validate().unwrap_err();
        assert!(error.to_string().contains("valid URL"));
    }

    #[test]
    fn action_translation_rejects_non_css_semantic_references() {
        let error = action_source(&SemanticAction::Click {
            target: "role=button[name=Save]".into(),
        })
        .unwrap_err();
        assert!(error.to_string().contains("CSS selector"));
    }

    #[test]
    fn modifier_click_fails_closed_when_runtime_cannot_preserve_input_state() {
        let error = action_source(&SemanticAction::ClickWithModifiers {
            target: "button.submit".into(),
            modifiers: crate::browser_backend::ClickModifiers {
                control: true,
                ..Default::default()
            },
        })
        .unwrap_err();
        assert!(matches!(
            error,
            BrowserBackendError::UnsupportedOperation { .. }
        ));
    }

    #[tokio::test]
    async fn mock_safari_webdriver_flow_covers_portable_semantics() {
        let listener = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
        let address = listener.local_addr().unwrap();
        let current_url = Arc::new(Mutex::new("about:blank".to_owned()));
        let server_url = Arc::clone(&current_url);
        let server = tokio::spawn(async move {
            while let Ok((mut stream, _)) = listener.accept().await {
                let current_url = Arc::clone(&server_url);
                tokio::spawn(async move {
                    let Some((method, path, body)) = read_http_request(&mut stream).await else {
                        return;
                    };
                    let value = match (method.as_str(), path.as_str()) {
                        ("POST", "/session") => {
                            json!({"sessionId": "safari-session", "capabilities": {}})
                        }
                        ("GET", "/session/safari-session/window/handles") => {
                            json!(["window-1"])
                        }
                        ("GET", "/session/safari-session/url") => {
                            json!(current_url.lock().unwrap().clone())
                        }
                        ("POST", "/session/safari-session/url") => {
                            *current_url.lock().unwrap() =
                                body["url"].as_str().unwrap_or("about:blank").to_owned();
                            Value::Null
                        }
                        ("POST", "/session/safari-session/execute/sync") => {
                            let script = body["script"].as_str().unwrap_or_default();
                            if script.contains("location.href") {
                                json!({"url": current_url.lock().unwrap().clone(), "title": "Safari mock", "text": "hello"})
                            } else if script.contains("e.click") || script.contains("e.value") {
                                json!(true)
                            } else {
                                json!(2)
                            }
                        }
                        ("DELETE", "/session/safari-session") => Value::Null,
                        _ => json!({"error": "unknown command", "message": "mock route missing"}),
                    };
                    let body = json!({"value": value}).to_string();
                    let response = format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                        body.len(),
                        body
                    );
                    let _ = stream.write_all(response.as_bytes()).await;
                });
            }
        });

        let backend = WebDriverBrowserBackend::new(WebDriverBackendConfig::for_safari(format!(
            "http://{address}"
        )))
        .unwrap();
        let dispatcher = BrowserBackendDispatcher::new(&backend);
        dispatcher.initialize().await.unwrap();
        let navigation = dispatcher
            .navigate(NavigationRequest {
                url: "https://example.test".into(),
            })
            .await
            .unwrap();
        assert_eq!(navigation.url, "https://example.test");
        let contexts = dispatcher
            .contexts(ContextRequest {
                include_background: false,
            })
            .await
            .unwrap();
        assert_eq!(contexts[0].context_id, "window-1");
        let evidence = dispatcher
            .evidence(EvidenceRequest {
                context_id: "window-1".into(),
                level: EvidenceLevel::Compact,
            })
            .await
            .unwrap();
        assert_eq!(evidence.visible_text, "hello");
        let script = dispatcher
            .script(ScriptRequest {
                context_id: "window-1".into(),
                source: "1 + 1".into(),
            })
            .await
            .unwrap();
        assert_eq!(script.value, json!(2));
        let action = dispatcher
            .action(ActionRequest {
                context_id: "window-1".into(),
                action: SemanticAction::Click {
                    target: "css=#submit".into(),
                },
            })
            .await
            .unwrap();
        let effects = dispatcher
            .effects(EffectsRequest {
                context_id: "window-1".into(),
                since_revision: navigation.revision,
            })
            .await
            .unwrap();
        assert!(action.accepted);
        assert!(effects.changed);
        dispatcher.close().await.unwrap();
        server.abort();
    }

    async fn read_http_request(
        stream: &mut tokio::net::TcpStream,
    ) -> Option<(String, String, Value)> {
        let mut bytes = Vec::new();
        let header_end;
        loop {
            let mut chunk = [0_u8; 4096];
            let count = stream.read(&mut chunk).await.ok()?;
            if count == 0 {
                return None;
            }
            bytes.extend_from_slice(&chunk[..count]);
            if let Some(position) = bytes.windows(4).position(|window| window == b"\r\n\r\n") {
                header_end = position + 4;
                break;
            }
            if bytes.len() > MAX_WIRE_BYTES {
                return None;
            }
        }
        let headers = std::str::from_utf8(&bytes[..header_end]).ok()?.to_owned();
        let content_length = headers
            .lines()
            .find_map(|line| {
                let (name, value) = line.split_once(':')?;
                name.eq_ignore_ascii_case("content-length")
                    .then_some(value.trim())
            })
            .and_then(|value| value.parse::<usize>().ok())
            .unwrap_or(0);
        while bytes.len() < header_end + content_length {
            let mut chunk = [0_u8; 4096];
            let count = stream.read(&mut chunk).await.ok()?;
            if count == 0 {
                return None;
            }
            bytes.extend_from_slice(&chunk[..count]);
            if bytes.len() > MAX_WIRE_BYTES {
                return None;
            }
        }
        let mut request_line = headers.lines().next()?.split_whitespace();
        let method = request_line.next()?.to_owned();
        let path = request_line.next()?.to_owned();
        let body = if content_length == 0 {
            Value::Null
        } else {
            serde_json::from_slice(&bytes[header_end..header_end + content_length]).ok()?
        };
        Some((method, path, body))
    }
}
