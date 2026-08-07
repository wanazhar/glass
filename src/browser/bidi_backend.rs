//! WebDriver BiDi adapter for the transport-neutral backend boundary.
//!
//! This module deliberately contains all BiDi wire details.  Callers use the
//! semantic [`crate::browser_backend`] contracts and receive typed failures
//! when a BiDi endpoint omits a capability or returns an incomplete response.

#[cfg(test)]
use crate::browser_backend::{
    ActionRequest, ContextRequest, EffectsRequest, EvidenceRequest, NavigationRequest,
};
use crate::browser_backend::{
    ActionResult, BROWSER_BACKEND_SCHEMA_VERSION, BackendContract, BackendFuture, BackendOperation,
    BackendProfile, BackendRequest, BackendResponse, BrowserBackend, BrowserBackendError,
    BrowserCapability, BrowsingContext, CapabilityDescriptor, EffectsResult, EvidenceLevel,
    EvidenceResult, NavigationResult, Portability, ScriptResult, SemanticAction, SupportLevel,
};
use futures_util::{SinkExt, StreamExt};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::time::Duration;
use tokio::net::TcpStream;
use tokio::sync::Mutex;
use tokio::time::timeout;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream, connect_async, tungstenite::Message};

const BIDI_BACKEND_ID: &str = "webdriver-bidi";
const BIDI_BACKEND_VERSION: &str = "1";
const DEFAULT_COMMAND_TIMEOUT: Duration = Duration::from_secs(10);
const MAX_WIRE_MESSAGES: usize = 128;
const MAX_WIRE_BYTES: usize = 256 * 1024;
const MAX_CONTEXT_TREE: usize = 64;

/// Configuration for a WebDriver BiDi endpoint.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BidiBackendConfig {
    /// A `ws://`/`wss://` BiDi endpoint, or an HTTP endpoint returning a
    /// `webSocketUrl`/`websocketUrl` discovery field.
    pub endpoint: String,
    /// Glass version recorded in the machine-readable certification profile.
    pub glass_version: String,
    /// Capabilities intentionally disabled for protocol-shock and deployment
    /// testing. Disabled capabilities are advertised as unavailable.
    pub disabled_capabilities: Vec<BrowserCapability>,
    /// Per-command transport deadline.
    pub command_timeout: Duration,
}

impl BidiBackendConfig {
    pub fn new(endpoint: impl Into<String>) -> Self {
        Self {
            endpoint: endpoint.into(),
            glass_version: env!("CARGO_PKG_VERSION").into(),
            disabled_capabilities: Vec::new(),
            command_timeout: DEFAULT_COMMAND_TIMEOUT,
        }
    }

    pub fn validate(&self) -> Result<(), BrowserBackendError> {
        if self.endpoint.is_empty() {
            return Err(BrowserBackendError::InvalidConfiguration {
                field: "bidi endpoint".into(),
                reason: "endpoint must not be empty".into(),
            });
        }
        if self.glass_version.is_empty() {
            return Err(BrowserBackendError::InvalidConfiguration {
                field: "glass version".into(),
                reason: "glass version must not be empty".into(),
            });
        }
        if self.command_timeout.is_zero() || self.command_timeout > Duration::from_secs(60) {
            return Err(BrowserBackendError::InvalidConfiguration {
                field: "command timeout".into(),
                reason: "timeout must be between one millisecond and sixty seconds".into(),
            });
        }
        if self.disabled_capabilities.len() > crate::browser_backend::MAX_CAPABILITIES {
            return Err(BrowserBackendError::InvalidConfiguration {
                field: "disabled capabilities".into(),
                reason: "too many disabled capabilities".into(),
            });
        }
        Ok(())
    }
}

type BidiSocket = WebSocketStream<MaybeTlsStream<TcpStream>>;

struct BidiState {
    socket: Option<BidiSocket>,
    initialized: bool,
    session_id: Option<String>,
    context_id: Option<String>,
    url: String,
    revision: u64,
    next_command_id: u64,
}

/// A WebDriver BiDi backend with one serialized, bounded command stream.
pub struct BidiBrowserBackend {
    state: Mutex<BidiState>,
    profile: BackendProfile,
    command_timeout: Duration,
}

impl BidiBrowserBackend {
    /// Connect to a BiDi WebSocket or HTTP discovery endpoint.
    pub async fn connect(endpoint: impl Into<String>) -> Result<Self, BrowserBackendError> {
        Self::connect_with_config(BidiBackendConfig::new(endpoint)).await
    }

    pub async fn connect_with_config(
        config: BidiBackendConfig,
    ) -> Result<Self, BrowserBackendError> {
        config.validate()?;
        let websocket_url =
            discover_websocket_url(&config.endpoint, config.command_timeout).await?;
        let (socket, _) = timeout(config.command_timeout, connect_async(&websocket_url))
            .await
            .map_err(|_| connection_error("connect", "connection timed out"))?
            .map_err(|error| connection_error("connect", &error.to_string()))?;
        let profile = Self::profile_for(&config)?;
        Ok(Self {
            state: Mutex::new(BidiState {
                socket: Some(socket),
                initialized: false,
                session_id: None,
                context_id: None,
                url: String::new(),
                revision: 0,
                next_command_id: 1,
            }),
            profile,
            command_timeout: config.command_timeout,
        })
    }

    pub fn profile_for(config: &BidiBackendConfig) -> Result<BackendProfile, BrowserBackendError> {
        config.validate()?;
        let disabled: std::collections::BTreeSet<_> =
            config.disabled_capabilities.iter().copied().collect();
        let mut capabilities = BTreeMap::new();
        let all_supported = [
            BrowserCapability::Lifecycle,
            BrowserCapability::Navigation,
            BrowserCapability::Contexts,
            BrowserCapability::Evidence,
            BrowserCapability::Action,
            BrowserCapability::Effects,
            BrowserCapability::Script,
        ];
        for capability in BrowserCapability::ALL {
            let available = all_supported.contains(&capability) && !disabled.contains(&capability);
            let dependent = matches!(
                capability,
                BrowserCapability::Evidence | BrowserCapability::Action
            ) && disabled.contains(&BrowserCapability::Script);
            let available = available && !dependent;
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
                    dependencies: Vec::new(),
                    limitations: if available {
                        Vec::new()
                    } else {
                        vec!["capability is unavailable at this BiDi boundary".into()]
                    },
                },
            );
        }
        let tested_capabilities = all_supported
            .into_iter()
            .filter(|capability| capabilities[capability].level != SupportLevel::Unavailable)
            .collect::<Vec<_>>();
        let profile = BackendProfile {
            schema_version: BROWSER_BACKEND_SCHEMA_VERSION,
            identity: crate::browser_backend::BackendIdentity {
                backend_id: BIDI_BACKEND_ID.into(),
                version: BIDI_BACKEND_VERSION.into(),
                browser: crate::browser_backend::BrowserVersionRange {
                    family: "webdriver-bidi".into(),
                    minimum: None,
                    maximum: None,
                },
                certification: crate::browser_backend::CertificationProfile {
                    level: crate::browser_backend::CertificationLevel::Experimental,
                    glass_version: config.glass_version.clone(),
                    tested_capabilities,
                    limitations: vec![
                        "BiDi support is bounded to navigation, contexts, evidence, script, action, and effects".into(),
                        "capture, storage, prompts, and downloads fail closed until certified".into(),
                    ],
                },
            },
            capabilities,
        };
        profile.validate()?;
        Ok(profile)
    }
    async fn command(
        &self,
        state: &mut BidiState,
        method: &str,
        params: Value,
    ) -> Result<Value, BrowserBackendError> {
        let id = next_command_id(state);
        let socket = state
            .socket
            .as_mut()
            .ok_or_else(|| lifecycle_error(method, "closed"))?;
        let payload = json!({"id": id, "method": method, "params": params});
        let encoded = serde_json::to_string(&payload)
            .map_err(|error| connection_error(method, &error.to_string()))?;
        if encoded.len() > MAX_WIRE_BYTES {
            return Err(connection_error(method, "command exceeds wire budget"));
        }
        timeout(
            self.command_timeout,
            socket.send(Message::Text(encoded.into())),
        )
        .await
        .map_err(|_| connection_error(method, "send timed out"))?
        .map_err(|error| connection_error(method, &error.to_string()))?;
        for _ in 0..MAX_WIRE_MESSAGES {
            let message = timeout(self.command_timeout, socket.next())
                .await
                .map_err(|_| connection_error(method, "receive timed out"))?
                .ok_or_else(|| connection_error(method, "endpoint closed the connection"))?
                .map_err(|error| connection_error(method, &error.to_string()))?;
            let bytes = match message {
                Message::Text(text) => text.as_bytes().to_vec(),
                Message::Binary(bytes) => bytes.to_vec(),
                Message::Ping(_) | Message::Pong(_) => continue,
                Message::Close(_) => {
                    return Err(connection_error(method, "endpoint closed the connection"));
                }
                Message::Frame(_) => continue,
            };
            if bytes.len() > MAX_WIRE_BYTES {
                return Err(connection_error(method, "response exceeds wire budget"));
            }
            let value: Value = serde_json::from_slice(&bytes).map_err(|error| {
                connection_error(method, &format!("invalid BiDi response: {error}"))
            })?;
            if value.get("id").and_then(Value::as_u64) != Some(id) {
                continue;
            }
            if let Some(error) = value.get("error") {
                let detail = error
                    .get("message")
                    .and_then(Value::as_str)
                    .unwrap_or("BiDi command failed");
                return Err(connection_error(method, detail));
            }
            return Ok(value.get("result").cloned().unwrap_or(Value::Null));
        }
        Err(connection_error(method, "response budget exhausted"))
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
                let result = self
                    .command(
                        &mut state,
                        "session.new",
                        json!({"capabilities": {"alwaysMatch": {}}}),
                    )
                    .await?;
                state.session_id = result
                    .get("session")
                    .and_then(Value::as_str)
                    .map(str::to_owned);
                let tree = self
                    .command(&mut state, "browsingContext.getTree", json!({}))
                    .await?;
                state.context_id = flatten_contexts(&tree)
                    .into_iter()
                    .next()
                    .map(|context| context.context_id);
                if state.context_id.is_none() {
                    return Err(connection_error(
                        "initialize",
                        "BiDi endpoint returned no browsing context",
                    ));
                }
                state.initialized = true;
                Ok(BackendResponse::Unit)
            }
            (BackendOperation::Close, BackendRequest::Close) => {
                if !state.initialized {
                    return Err(lifecycle_error("close", "closed"));
                }
                let _ = self.command(&mut state, "session.end", json!({})).await;
                if let Some(mut socket) = state.socket.take() {
                    let _ = socket.close(None).await;
                }
                state.initialized = false;
                Ok(BackendResponse::Unit)
            }
            (BackendOperation::Navigate, BackendRequest::Navigate(request)) => {
                ensure_initialized(&state, "navigate")?;
                let context = state
                    .context_id
                    .clone()
                    .ok_or_else(|| connection_error("navigate", "no browsing context"))?;
                let requested_url = request.url;
                let result = self
                    .command(
                        &mut state,
                        "browsingContext.navigate",
                        json!({"context": context, "url": requested_url, "wait": "complete"}),
                    )
                    .await?;
                state.revision = state.revision.saturating_add(1);
                state.url = result
                    .get("url")
                    .and_then(Value::as_str)
                    .unwrap_or(&requested_url)
                    .to_owned();
                Ok(BackendResponse::Navigation(NavigationResult {
                    url: state.url.clone(),
                    revision: state.revision,
                }))
            }
            (BackendOperation::Contexts, BackendRequest::Contexts(_request)) => {
                ensure_initialized(&state, "contexts")?;
                let tree = self
                    .command(&mut state, "browsingContext.getTree", json!({}))
                    .await?;
                let contexts = flatten_contexts(&tree);
                Ok(BackendResponse::Contexts(
                    contexts
                        .into_iter()
                        .map(|mut context| {
                            context.active =
                                state.context_id.as_deref() == Some(context.context_id.as_str());
                            context
                        })
                        .collect(),
                ))
            }
            (BackendOperation::Evidence, BackendRequest::Evidence(request)) => {
                ensure_context(&state, &request.context_id, "evidence")?;
                let value = self.evaluate_value(&mut state, &request.context_id, "({url: location.href, title: document.title || '', text: document.body ? (document.body.innerText || '') : ''})").await?;
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
                    url,
                    title,
                    visible_text,
                    complete: !matches!(request.level, EvidenceLevel::Screenshot),
                }))
            }
            (BackendOperation::Script, BackendRequest::Script(request)) => {
                ensure_context(&state, &request.context_id, "script")?;
                Ok(BackendResponse::Script(ScriptResult {
                    value: self
                        .evaluate_value(&mut state, &request.context_id, &request.source)
                        .await?,
                }))
            }
            (BackendOperation::Action, BackendRequest::Action(request)) => {
                ensure_context(&state, &request.context_id, "action")?;
                let source = action_source(&request.action)?;
                let value = self
                    .evaluate_value(&mut state, &request.context_id, &source)
                    .await?;
                if value.as_bool() == Some(false) {
                    return Err(BrowserBackendError::UnsupportedOperation {
                        operation: "action".into(),
                        reason: "BiDi target was not found or action was rejected".into(),
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

    async fn evaluate_value(
        &self,
        state: &mut BidiState,
        context: &str,
        source: &str,
    ) -> Result<Value, BrowserBackendError> {
        let result = self.command(state, "script.evaluate", json!({"expression": source, "target": {"context": context}, "awaitPromise": true, "resultOwnership": "none"})).await?;
        Ok(result
            .get("result")
            .and_then(|value| value.get("value"))
            .cloned()
            .unwrap_or(result))
    }
}

impl BrowserBackend for BidiBrowserBackend {
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

async fn discover_websocket_url(
    endpoint: &str,
    command_timeout: Duration,
) -> Result<String, BrowserBackendError> {
    if endpoint.starts_with("ws://") || endpoint.starts_with("wss://") {
        return Ok(endpoint.to_owned());
    }
    if !(endpoint.starts_with("http://") || endpoint.starts_with("https://")) {
        return Err(BrowserBackendError::InvalidConfiguration {
            field: "bidi endpoint".into(),
            reason: "endpoint must use ws, wss, http, or https".into(),
        });
    }
    let response = timeout(command_timeout, reqwest::get(endpoint))
        .await
        .map_err(|_| connection_error("discover", "HTTP discovery timed out"))?
        .map_err(|error| connection_error("discover", &error.to_string()))?;
    let document: Value = timeout(command_timeout, response.json())
        .await
        .map_err(|_| connection_error("discover", "HTTP response timed out"))?
        .map_err(|error| connection_error("discover", &error.to_string()))?;
    document
        .get("webSocketUrl")
        .or_else(|| document.get("websocketUrl"))
        .and_then(Value::as_str)
        .map(str::to_owned)
        .ok_or_else(|| connection_error("discover", "HTTP response omitted webSocketUrl"))
}

fn flatten_contexts(value: &Value) -> Vec<BrowsingContext> {
    fn visit(value: &Value, output: &mut Vec<BrowsingContext>) {
        if output.len() >= MAX_CONTEXT_TREE {
            return;
        }
        let Some(context_id) = value.get("context").and_then(Value::as_str) else {
            return;
        };
        let context = BrowsingContext {
            context_id: context_id.to_owned(),
            url: value
                .get("url")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_owned(),
            active: false,
        };
        output.push(context);
        if let Some(children) = value.get("children").and_then(Value::as_array) {
            for child in children {
                visit(child, output);
            }
        }
    }
    let mut output = Vec::new();
    if let Some(contexts) = value.get("contexts").and_then(Value::as_array) {
        for context in contexts {
            visit(context, &mut output);
        }
    }
    output
}

fn action_source(action: &SemanticAction) -> Result<String, BrowserBackendError> {
    match action {
        SemanticAction::Click { target } => Ok(format!(
            "(() => {{ const e = document.querySelector({}); if (!e) return false; e.click(); return true; }})()",
            serde_json::to_string(target).unwrap_or_else(|_| "\"\"".into())
        )),
        SemanticAction::Type { target, text } => Ok(format!(
            "(() => {{ const e = document.querySelector({}); if (!e) return false; e.focus(); e.value = {}; e.dispatchEvent(new Event('input', {{bubbles: true}})); e.dispatchEvent(new Event('change', {{bubbles: true}})); return true; }})()",
            serde_json::to_string(target).unwrap_or_else(|_| "\"\"".into()),
            serde_json::to_string(text).unwrap_or_else(|_| "\"\"".into())
        )),
        SemanticAction::KeyPress { .. } | SemanticAction::Scroll { .. } => {
            Err(BrowserBackendError::UnsupportedOperation {
                operation: "action".into(),
                reason: "BiDi adapter only certifies click and type input".into(),
            })
        }
    }
}
fn next_command_id(state: &mut BidiState) -> u64 {
    let id = state.next_command_id;
    state.next_command_id = state.next_command_id.saturating_add(1);
    id
}
fn ensure_initialized(state: &BidiState, operation: &str) -> Result<(), BrowserBackendError> {
    if state.initialized {
        Ok(())
    } else {
        Err(lifecycle_error(operation, "not initialized"))
    }
}
fn ensure_context(
    state: &BidiState,
    context: &str,
    operation: &str,
) -> Result<(), BrowserBackendError> {
    ensure_initialized(state, operation)?;
    if state.context_id.as_deref() == Some(context) {
        Ok(())
    } else {
        Err(BrowserBackendError::UnsupportedOperation {
            operation: operation.into(),
            reason: "context is not the active BiDi context".into(),
        })
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
        reason: "BiDi backend lifecycle state does not permit this operation".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn profile_is_explicit_and_fail_closed() {
        let mut config = BidiBackendConfig::new("ws://127.0.0.1:1");
        config.disabled_capabilities.push(BrowserCapability::Script);
        let profile = BidiBrowserBackend::profile_for(&config).unwrap();
        assert_eq!(profile.identity.backend_id, BIDI_BACKEND_ID);
        assert_eq!(
            profile.capability(BrowserCapability::Script).level,
            SupportLevel::Unavailable
        );
        assert_eq!(
            profile.capability(BrowserCapability::Action).level,
            SupportLevel::Unavailable
        );
        assert_eq!(
            profile.capability(BrowserCapability::Capture).level,
            SupportLevel::Unavailable
        );
    }

    #[test]
    fn action_translation_is_bounded() {
        let source = action_source(&SemanticAction::Type {
            target: "input[name=q]".into(),
            text: "hello".into(),
        })
        .unwrap();
        assert!(source.contains("dispatchEvent"));
        assert!(source.len() < crate::browser_backend::MAX_JSON_BYTES);
    }
    #[tokio::test]
    async fn mock_bidi_flow_navigates_extracts_acts_and_reports_effects() {
        let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
            .await
            .unwrap();
        let address = listener.local_addr().unwrap();
        tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let mut socket = tokio_tungstenite::accept_async(stream).await.unwrap();
            while let Some(Ok(Message::Text(text))) = socket.next().await {
                let request: Value = serde_json::from_str(text.as_ref()).unwrap();
                let id = request.get("id").and_then(Value::as_u64).unwrap();
                let method = request.get("method").and_then(Value::as_str).unwrap();
                let result = match method {
                    "session.new" => json!({"session": "mock-session"}),
                    "browsingContext.getTree" => {
                        json!({"contexts": [{"context": "mock-context", "url": "about:blank", "children": []}]})
                    }
                    "browsingContext.navigate" => json!({"url": "https://example.test"}),
                    "script.evaluate" => {
                        let expression = request
                            .pointer("/params/expression")
                            .and_then(Value::as_str)
                            .unwrap_or("");
                        if expression.contains("location.href") {
                            json!({"result": {"type": "object", "value": {"url": "https://example.test", "title": "Mock", "text": "hello"}}})
                        } else {
                            json!({"result": {"type": "boolean", "value": true}})
                        }
                    }
                    "session.end" => json!({}),
                    _ => json!({}),
                };
                socket
                    .send(Message::Text(
                        json!({"id": id, "result": result}).to_string().into(),
                    ))
                    .await
                    .unwrap();
                if method == "session.end" {
                    break;
                }
            }
        });

        let backend = BidiBrowserBackend::connect(format!("ws://{address}"))
            .await
            .unwrap();
        let dispatcher = crate::browser_backend::BrowserBackendDispatcher::new(&backend);
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
        assert_eq!(contexts[0].context_id, "mock-context");
        let evidence = dispatcher
            .evidence(EvidenceRequest {
                context_id: "mock-context".into(),
                level: EvidenceLevel::Deep,
            })
            .await
            .unwrap();
        assert_eq!(evidence.visible_text, "hello");
        let action = dispatcher
            .action(ActionRequest {
                context_id: "mock-context".into(),
                action: SemanticAction::Click {
                    target: "#submit".into(),
                },
            })
            .await
            .unwrap();
        let effects = dispatcher
            .effects(EffectsRequest {
                context_id: "mock-context".into(),
                since_revision: navigation.revision,
            })
            .await
            .unwrap();
        assert!(action.accepted);
        assert!(effects.changed);
        dispatcher.close().await.unwrap();
    }
}
