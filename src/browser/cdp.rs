use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::error::Error;
use std::fmt::{Display, Formatter};
use std::path::Path;
use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};
use std::time::Duration;
use tokio::sync::{broadcast, mpsc, oneshot};
use tokio_tungstenite::tungstenite::Message;
use tracing::{debug, info, warn};

#[derive(Clone, Default)]
struct CdpRoute {
    target_id: Option<String>,
    session_id: Option<String>,
    context_id: Option<i64>,
    frame_id: Option<String>,
}

tokio::task_local! {
    static OPERATION_ROUTE: CdpRoute;
}

/// A CDP method call request.
#[derive(Debug, Serialize)]
pub struct CdpRequest {
    pub id: u64,
    pub method: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub params: Option<Value>,
    #[serde(rename = "sessionId", skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
}

/// A protocol or transport error returned by a CDP connection.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CdpError {
    pub code: i64,
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
    #[serde(skip)]
    kind: CdpErrorKind,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
enum CdpErrorKind {
    #[default]
    Protocol,
    Transport,
    ResponseTimeout,
}

impl CdpError {
    fn transport(message: impl Into<String>) -> Self {
        Self {
            code: -32_000,
            message: message.into(),
            data: None,
            kind: CdpErrorKind::Transport,
        }
    }

    fn response_timeout(timeout: Duration) -> Self {
        Self {
            code: -32_000,
            message: format!(
                "CDP response timeout after {} seconds",
                timeout.as_secs_f64()
            ),
            data: None,
            kind: CdpErrorKind::ResponseTimeout,
        }
    }

    /// Whether this error is the locally typed expiry of an unanswered CDP request.
    pub fn is_response_timeout(&self) -> bool {
        self.kind == CdpErrorKind::ResponseTimeout
    }
}

impl Display for CdpError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "CDP error {}: {}", self.code, self.message)
    }
}

impl Error for CdpError {}

/// Lightweight notification of a CDP event.
///
/// This default event stream intentionally omits payloads. Subscribe to
/// [`CdpClient::subscribe_events_with_params`] only when a caller needs them.
#[derive(Debug, Clone)]
pub struct CdpEvent {
    pub method: String,
}

/// CDP event with its full JSON payload for explicit diagnostic or media use.
#[derive(Debug, Clone)]
pub struct CdpEventWithParams {
    pub method: String,
    pub params: Value,
    pub session_id: Option<String>,
}

#[derive(Debug)]
pub struct CdpScreencastFrame {
    pub data: String,
    pub metadata: Value,
    pub session_id: Option<String>,
}

struct ScreencastSink {
    session_id: Option<String>,
    sender: mpsc::Sender<CdpScreencastFrame>,
}

#[derive(Debug, Deserialize)]
struct IncomingMessage {
    #[serde(default)]
    id: Option<u64>,
    #[serde(default)]
    result: Option<Value>,
    #[serde(default)]
    error: Option<CdpError>,
    #[serde(default)]
    method: Option<String>,
    #[serde(default, rename = "sessionId")]
    session_id: Option<String>,
}

#[derive(Debug, Deserialize)]
struct IncomingEventParams {
    #[serde(default)]
    params: Value,
}

enum Command {
    Request {
        id: u64,
        json: String,
        response: oneshot::Sender<Result<Value, CdpError>>,
    },
    Cancel {
        id: u64,
    },
    FireAndForget {
        json: String,
    },
    Close,
}

struct PendingRequestGuard {
    tx: mpsc::UnboundedSender<Command>,
    id: u64,
    armed: bool,
}

impl PendingRequestGuard {
    fn disarm(&mut self) {
        self.armed = false;
    }
}

impl Drop for PendingRequestGuard {
    fn drop(&mut self) {
        if self.armed {
            let _ = self.tx.send(Command::Cancel { id: self.id });
        }
    }
}

/// A multiplexed connection to Chrome DevTools Protocol.
///
/// The connection task owns the WebSocket and the pending request map. This
/// keeps response routing and event delivery in one place while allowing
/// multiple commands to be in flight at once.
#[derive(Clone)]
pub struct CdpClient {
    tx: mpsc::UnboundedSender<Command>,
    next_id: Arc<AtomicU64>,
    events: broadcast::Sender<CdpEvent>,
    payload_events: broadcast::Sender<CdpEventWithParams>,
    screencast_sink: Arc<std::sync::Mutex<Option<ScreencastSink>>>,
    screencast_received: Arc<AtomicU64>,
    screencast_dropped: Arc<AtomicU64>,
    timeout: Duration,
    active_route: Arc<std::sync::Mutex<CdpRoute>>,
}

impl CdpClient {
    /// Number of CDP requests allocated by this connection so far.
    ///
    /// This is a monotonic diagnostic counter and does not retain request
    /// payloads or alter command routing.
    pub fn request_count(&self) -> u64 {
        self.next_id.load(Ordering::Relaxed).saturating_sub(1)
    }

    /// Connect to a Chrome CDP page WebSocket using the default timeout.
    pub async fn connect(ws_url: &str) -> Result<Self, Box<dyn Error>> {
        Self::connect_with_timeout(ws_url, Duration::from_secs(30)).await
    }

    /// Connect to a Chrome CDP page WebSocket with a custom command timeout.
    pub async fn connect_with_timeout(
        ws_url: &str,
        timeout: Duration,
    ) -> Result<Self, Box<dyn Error>> {
        info!(%ws_url, "connecting to CDP");
        let (ws_stream, _) = tokio_tungstenite::connect_async(ws_url).await?;
        let (mut write, mut read) = ws_stream.split();
        let (tx, mut rx) = mpsc::unbounded_channel::<Command>();
        let (event_tx, _) = broadcast::channel::<CdpEvent>(128);
        let (payload_event_tx, _) = broadcast::channel::<CdpEventWithParams>(128);
        let actor_events = event_tx.clone();
        let actor_payload_events = payload_event_tx.clone();
        let screencast_sink = Arc::new(std::sync::Mutex::new(None));
        let actor_screencast_sink = Arc::clone(&screencast_sink);
        let screencast_received = Arc::new(AtomicU64::new(0));
        let actor_screencast_received = Arc::clone(&screencast_received);
        let screencast_dropped = Arc::new(AtomicU64::new(0));
        let actor_screencast_dropped = Arc::clone(&screencast_dropped);
        let actor_tx = tx.clone();
        let actor_next_id = Arc::new(AtomicU64::new(1));
        let next_id = Arc::clone(&actor_next_id);

        tokio::spawn(async move {
            let mut pending: HashMap<u64, oneshot::Sender<Result<Value, CdpError>>> =
                HashMap::new();
            let mut close_reason = "CDP connection closed".to_string();

            loop {
                tokio::select! {
                    command = rx.recv() => {
                        match command {
                            Some(Command::Request { id, json, response }) => {
                                pending.insert(id, response);
                                if let Err(error) = write.send(Message::Text(json.into())).await {
                                    close_reason = format!("CDP write failed: {error}");
                                    break;
                                }
                            }
                            Some(Command::Cancel { id }) => {
                                pending.remove(&id);
                            }
                            Some(Command::FireAndForget { json }) => {
                                if let Err(error) = write.send(Message::Text(json.into())).await {
                                    close_reason = format!("CDP write failed: {error}");
                                    break;
                                }
                            }
                            Some(Command::Close) | None => {
                                let _ = write.send(Message::Close(None)).await;
                                close_reason = "CDP connection closed by client".to_string();
                                break;
                            }
                        }
                    }
                    message = read.next() => {
                        match message {
                            Some(Ok(Message::Text(text))) => {
                                handle_incoming_message(
                                    &mut pending,
                                    &actor_events,
                                    &actor_payload_events,
                                    ScreencastDispatch {
                                        sink: &actor_screencast_sink,
                                        received: &actor_screencast_received,
                                        dropped: &actor_screencast_dropped,
                                        command_tx: &actor_tx,
                                        next_id: &actor_next_id,
                                    },
                                    text.as_ref(),
                                );
                            }
                            Some(Ok(Message::Binary(bytes))) => {
                                match std::str::from_utf8(bytes.as_ref()) {
                                    Ok(text) => handle_incoming_message(
                                        &mut pending,
                                        &actor_events,
                                        &actor_payload_events,
                                        ScreencastDispatch {
                                            sink: &actor_screencast_sink,
                                            received: &actor_screencast_received,
                                            dropped: &actor_screencast_dropped,
                                            command_tx: &actor_tx,
                                            next_id: &actor_next_id,
                                        },
                                        text,
                                    ),
                                    Err(error) => warn!(%error, "ignoring non-UTF-8 CDP frame"),
                                }
                            }
                            Some(Ok(Message::Ping(payload))) => {
                                if let Err(error) = write.send(Message::Pong(payload)).await {
                                    close_reason = format!("CDP pong failed: {error}");
                                    break;
                                }
                            }
                            Some(Ok(Message::Close(_))) => {
                                close_reason = "CDP server closed the connection".to_string();
                                break;
                            }
                            Some(Ok(_)) => {}
                            Some(Err(error)) => {
                                close_reason = format!("CDP read failed: {error}");
                                break;
                            }
                            None => break,
                        }
                    }
                }
            }

            let error = CdpError::transport(close_reason);
            for (_, response) in pending.drain() {
                let _ = response.send(Err(error.clone()));
            }
        });

        Ok(Self {
            tx,
            next_id,
            events: event_tx,
            payload_events: payload_event_tx,
            screencast_sink,
            screencast_received,
            screencast_dropped,
            timeout,
            active_route: Arc::new(std::sync::Mutex::new(CdpRoute::default())),
        })
    }

    /// Subscribe to method-only CDP events such as page lifecycle events.
    pub fn subscribe_events(&self) -> broadcast::Receiver<CdpEvent> {
        self.events.subscribe()
    }

    /// Subscribe to CDP events with payloads for explicit media or diagnostic work.
    ///
    /// Payload JSON is parsed only while this stream has at least one receiver.
    pub fn subscribe_events_with_params(&self) -> broadcast::Receiver<CdpEventWithParams> {
        self.payload_events.subscribe()
    }

    pub fn open_screencast_channel(
        &self,
        session_id: Option<String>,
    ) -> Result<mpsc::Receiver<CdpScreencastFrame>, CdpError> {
        let mut sink = self
            .screencast_sink
            .lock()
            .map_err(|_| CdpError::transport("screencast sink lock poisoned"))?;
        if sink.is_some() {
            return Err(CdpError::transport("a screencast scope is already active"));
        }
        let (sender, receiver) = mpsc::channel(2);
        *sink = Some(ScreencastSink { session_id, sender });
        self.screencast_received.store(0, Ordering::Relaxed);
        self.screencast_dropped.store(0, Ordering::Relaxed);
        Ok(receiver)
    }

    pub fn close_screencast_channel(&self) -> (u64, u64) {
        if let Ok(mut sink) = self.screencast_sink.lock() {
            *sink = None;
        }
        (
            self.screencast_received.load(Ordering::Relaxed),
            self.screencast_dropped.load(Ordering::Relaxed),
        )
    }

    pub fn screencast_stats(&self) -> (u64, u64) {
        (
            self.screencast_received.load(Ordering::Relaxed),
            self.screencast_dropped.load(Ordering::Relaxed),
        )
    }

    pub fn current_session_id(&self) -> Option<String> {
        self.current_route().session_id
    }

    pub async fn set_domain_enabled_for(
        &self,
        session_id: Option<String>,
        domain: &str,
        enabled: bool,
    ) -> Result<(), CdpError> {
        let method = format!("{domain}.{}", if enabled { "enable" } else { "disable" });
        self.send_routed(&method, None, session_id, self.timeout)
            .await?;
        Ok(())
    }

    /// Send a CDP command and wait for the response.
    pub async fn send(&self, method: &str, params: Option<Value>) -> Result<Value, CdpError> {
        let session_id = self.current_route().session_id;
        self.send_routed(method, params, session_id, self.timeout)
            .await
    }

    pub async fn send_browser(
        &self,
        method: &str,
        params: Option<Value>,
    ) -> Result<Value, CdpError> {
        self.send_routed(method, params, None, self.timeout).await
    }

    pub async fn send_to_session(
        &self,
        session_id: &str,
        method: &str,
        params: Option<Value>,
    ) -> Result<Value, CdpError> {
        self.send_routed(method, params, Some(session_id.to_string()), self.timeout)
            .await
    }

    /// Send one operation-scoped command without changing the connection's
    /// default response deadline or active route.
    pub async fn send_with_timeout(
        &self,
        method: &str,
        params: Option<Value>,
        timeout: Duration,
    ) -> Result<Value, CdpError> {
        let session_id = self.current_route().session_id;
        self.send_routed(method, params, session_id, timeout).await
    }

    async fn send_routed(
        &self,
        method: &str,
        params: Option<Value>,
        session_id: Option<String>,
        timeout: Duration,
    ) -> Result<Value, CdpError> {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let request = CdpRequest {
            id,
            method: method.to_string(),
            params,
            session_id,
        };
        let json = serde_json::to_string(&request)
            .map_err(|error| CdpError::transport(format!("failed to encode request: {error}")))?;
        let (response_tx, response_rx) = oneshot::channel();

        self.tx
            .send(Command::Request {
                id,
                json,
                response: response_tx,
            })
            .map_err(|_| CdpError::transport("CDP connection task is unavailable"))?;
        let mut pending_guard = PendingRequestGuard {
            tx: self.tx.clone(),
            id,
            armed: true,
        };

        match tokio::time::timeout(timeout, response_rx).await {
            Ok(Ok(result)) => {
                pending_guard.disarm();
                result
            }
            Ok(Err(_)) => {
                pending_guard.disarm();
                Err(CdpError::transport("CDP response channel closed"))
            }
            Err(_) => Err(CdpError::response_timeout(timeout)),
        }
    }

    pub fn set_active_session(&self, session_id: Option<String>) {
        let mut route = self
            .active_route
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        route.session_id = session_id;
        route.context_id = None;
        route.frame_id = None;
    }

    pub fn set_active_context(&self, context_id: Option<i64>) {
        self.active_route
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .context_id = context_id;
    }

    pub fn set_active_frame_context(&self, frame_id: Option<String>, context_id: Option<i64>) {
        let mut route = self
            .active_route
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        route.frame_id = frame_id;
        route.context_id = context_id;
    }

    pub fn set_active_route(
        &self,
        session_id: Option<String>,
        frame_id: Option<String>,
        context_id: Option<i64>,
    ) {
        let mut route = self
            .active_route
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        route.session_id = session_id;
        route.frame_id = frame_id;
        route.context_id = context_id;
    }

    pub fn set_active_target_route(
        &self,
        target_id: Option<String>,
        session_id: Option<String>,
        frame_id: Option<String>,
        context_id: Option<i64>,
    ) {
        *self
            .active_route
            .lock()
            .unwrap_or_else(|poison| poison.into_inner()) = CdpRoute {
            target_id,
            session_id,
            frame_id,
            context_id,
        };
    }

    pub fn operation_identity(&self) -> Option<(String, String)> {
        let route = self.current_route();
        Some((route.target_id?, route.frame_id?))
    }

    pub fn set_active_frame(&self, frame_id: Option<String>) {
        self.active_route
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .frame_id = frame_id;
    }

    pub fn active_frame(&self) -> Option<String> {
        self.current_route().frame_id
    }

    fn current_route(&self) -> CdpRoute {
        OPERATION_ROUTE.try_with(Clone::clone).unwrap_or_else(|_| {
            self.active_route
                .lock()
                .unwrap_or_else(|poison| poison.into_inner())
                .clone()
        })
    }

    pub async fn with_current_route<F: std::future::Future>(&self, future: F) -> F::Output {
        if OPERATION_ROUTE.try_with(|_| ()).is_ok() {
            future.await
        } else {
            OPERATION_ROUTE.scope(self.current_route(), future).await
        }
    }

    pub async fn with_current_target_route<F: std::future::Future>(&self, future: F) -> F::Output {
        let mut route = self.current_route();
        route.context_id = None;
        OPERATION_ROUTE.scope(route, future).await
    }

    /// Return the selected child frame viewport origin in target coordinates.
    pub async fn frame_viewport_offset(&self, frame_id: &str) -> Result<(f64, f64), CdpError> {
        let owner = self
            .send(
                "DOM.getFrameOwner",
                Some(serde_json::json!({"frameId": frame_id})),
            )
            .await?;
        let backend_node_id = owner["backendNodeId"]
            .as_i64()
            .ok_or_else(|| CdpError::transport("frame owner contained no backend node ID"))?;
        let model = self
            .send(
                "DOM.getBoxModel",
                Some(serde_json::json!({"backendNodeId": backend_node_id})),
            )
            .await?;
        let content = model["model"]["content"]
            .as_array()
            .filter(|quad| quad.len() >= 2)
            .ok_or_else(|| CdpError::transport("frame owner contained no content quad"))?;
        let x = content[0]
            .as_f64()
            .ok_or_else(|| CdpError::transport("frame owner x was not numeric"))?;
        let y = content[1]
            .as_f64()
            .ok_or_else(|| CdpError::transport("frame owner y was not numeric"))?;
        Ok((x, y))
    }

    /// Navigate to a URL.
    pub async fn navigate(&self, url: &str) -> Result<Value, CdpError> {
        self.send("Page.navigate", Some(serde_json::json!({ "url": url })))
            .await
    }

    /// Take a screenshot and return its base64-encoded image data.
    pub async fn screenshot(&self, format: &str) -> Result<String, CdpError> {
        self.screenshot_with_params(serde_json::json!({
            "format": format,
            "optimizeForSpeed": true
        }))
        .await
    }

    pub async fn screenshot_with_params(&self, params: Value) -> Result<String, CdpError> {
        let mut result = self.send("Page.captureScreenshot", Some(params)).await?;
        match result.get_mut("data").map(Value::take) {
            Some(Value::String(data)) => Ok(data),
            _ => Err(CdpError::transport(
                "CDP screenshot response contained no data",
            )),
        }
    }

    pub async fn get_layout_metrics(&self) -> Result<Value, CdpError> {
        self.send("Page.getLayoutMetrics", None).await
    }

    /// Get the accessibility tree.
    pub async fn get_accessibility_tree(&self) -> Result<Value, CdpError> {
        let frame_id = self.current_route().frame_id;
        self.send(
            "Accessibility.getFullAXTree",
            frame_id.map(|frame_id| serde_json::json!({"frameId": frame_id})),
        )
        .await
    }

    /// Get a flattened document tree including shadow DOM content.
    /// Uses `pierce: true` to include open shadow roots up to the given depth.
    pub async fn get_flattened_document(&self, depth: i64) -> Result<Value, CdpError> {
        self.send(
            "DOM.getFlattenedDocument",
            Some(serde_json::json!({ "depth": depth, "pierce": true })),
        )
        .await
    }

    /// Get the full document tree for an explicit deep-DOM inspection.
    pub async fn get_deep_document(&self) -> Result<Value, CdpError> {
        self.send("DOM.getDocument", Some(serde_json::json!({ "depth": -1 })))
            .await
    }

    /// Get only the document root for operations that do not need descendants.
    pub async fn get_document_root(&self) -> Result<Value, CdpError> {
        self.send("DOM.getDocument", Some(serde_json::json!({ "depth": 0 })))
            .await
    }

    /// Compatibility alias for the explicit deep-DOM request.
    pub async fn get_document(&self) -> Result<Value, CdpError> {
        self.get_deep_document().await
    }

    /// Query a CSS selector and return the matching node.
    pub async fn query_selector(&self, selector: &str) -> Result<Value, CdpError> {
        let document = self.get_document_root().await?;
        let root_id = document["root"]["nodeId"]
            .as_i64()
            .ok_or_else(|| CdpError::transport("DOM document response contained no root nodeId"))?;
        self.send(
            "DOM.querySelector",
            Some(serde_json::json!({ "nodeId": root_id, "selector": selector })),
        )
        .await
    }

    /// Resolve a DOM/backend node to a reusable remote object.
    pub async fn resolve_node_object(
        &self,
        node_id: Option<i64>,
        backend_node_id: Option<i64>,
    ) -> Result<String, CdpError> {
        let mut params = serde_json::Map::new();
        if let Some(node_id) = node_id {
            params.insert("nodeId".to_string(), Value::from(node_id));
        }
        if let Some(backend_node_id) = backend_node_id {
            params.insert("backendNodeId".to_string(), Value::from(backend_node_id));
        }
        let resolved = self
            .send("DOM.resolveNode", Some(Value::Object(params)))
            .await?;
        resolved["object"]["objectId"]
            .as_str()
            .map(str::to_string)
            .ok_or_else(|| CdpError::transport("DOM.resolveNode returned no objectId"))
    }

    /// Translate one already-resolved frontend node into its immutable backend
    /// identity without repeating the caller's locator query.
    pub async fn backend_node_id_for_node(&self, node_id: i64) -> Result<i64, CdpError> {
        let described = self
            .send(
                "DOM.describeNode",
                Some(serde_json::json!({"nodeId": node_id, "depth": 0})),
            )
            .await?;
        described["node"]["backendNodeId"]
            .as_i64()
            .filter(|id| *id > 0)
            .ok_or_else(|| CdpError::transport("DOM.describeNode returned no backendNodeId"))
    }

    /// Invoke a function on a previously resolved remote object.
    pub async fn call_on_object(
        &self,
        object_id: &str,
        function_declaration: &str,
    ) -> Result<Value, CdpError> {
        self.send(
            "Runtime.callFunctionOn",
            Some(serde_json::json!({
                "objectId": object_id,
                "functionDeclaration": function_declaration,
                "returnByValue": true,
                "awaitPromise": true
            })),
        )
        .await
    }

    pub async fn release_object(&self, object_id: &str) -> Result<Value, CdpError> {
        self.send(
            "Runtime.releaseObject",
            Some(serde_json::json!({ "objectId": object_id })),
        )
        .await
    }

    pub async fn release_object_for_session(
        &self,
        session_id: &str,
        object_id: &str,
    ) -> Result<Value, CdpError> {
        self.send_to_session(
            session_id,
            "Runtime.releaseObject",
            Some(serde_json::json!({"objectId": object_id})),
        )
        .await
    }

    /// Resolve a page-produced, bounded remote element array into DOM node IDs.
    ///
    /// The expression must return an Array with at most `limit` elements and a
    /// numeric `glassCount` property containing the total logical match count.
    pub async fn bounded_element_query(
        &self,
        expression: &str,
        limit: usize,
    ) -> Result<(usize, Vec<i64>), CdpError> {
        // DOM.requestNode only returns frontend node IDs after the document has
        // been requested in this CDP session.
        self.get_document_root().await?;
        let context_id = self.current_route().context_id;
        let mut params = serde_json::json!({
            "expression": expression,
            "returnByValue": false,
            "awaitPromise": true
        });
        if let Some(context_id) = context_id {
            params["contextId"] = Value::from(context_id);
        }
        let evaluated = self.send("Runtime.evaluate", Some(params)).await?;
        if evaluated.get("exceptionDetails").is_some() {
            return Err(CdpError::transport("element query evaluation failed"));
        }
        let array_id = evaluated["result"]["objectId"]
            .as_str()
            .ok_or_else(|| CdpError::transport("element query returned no remote array"))?;
        let properties = self
            .send(
                "Runtime.getProperties",
                Some(serde_json::json!({
                    "objectId": array_id,
                    "ownProperties": true
                })),
            )
            .await?;
        let mut count = 0;
        let mut objects = Vec::with_capacity(limit);
        for property in properties["result"].as_array().into_iter().flatten() {
            if property["name"].as_str() == Some("glassCount") {
                count = property["value"]["value"].as_u64().unwrap_or(0) as usize;
                continue;
            }
            if property["name"]
                .as_str()
                .and_then(|name| name.parse::<usize>().ok())
                .is_some_and(|index| index < limit)
                && let Some(object_id) = property["value"]["objectId"].as_str()
            {
                objects.push(object_id.to_string());
            }
        }
        let mut node_ids = Vec::with_capacity(objects.len());
        for object_id in objects {
            let requested = self
                .send(
                    "DOM.requestNode",
                    Some(serde_json::json!({ "objectId": object_id })),
                )
                .await;
            let _ = self
                .send(
                    "Runtime.releaseObject",
                    Some(serde_json::json!({ "objectId": object_id })),
                )
                .await;
            let requested = requested?;
            if let Some(node_id) = requested["nodeId"].as_i64().filter(|id| *id != 0) {
                node_ids.push(node_id);
            }
        }
        let _ = self
            .send(
                "Runtime.releaseObject",
                Some(serde_json::json!({ "objectId": array_id })),
            )
            .await;
        Ok((count, node_ids))
    }

    /// Get the bounding box of a DOM node.
    pub async fn get_box_model(&self, node_id: i64) -> Result<Value, CdpError> {
        self.get_box_model_inner(Some(node_id), None).await
    }

    /// Get the bounding box of a backend DOM node from an accessibility tree.
    pub async fn get_box_model_for_backend(&self, backend_node_id: i64) -> Result<Value, CdpError> {
        self.get_box_model_inner(None, Some(backend_node_id)).await
    }

    /// Ask Chrome to scroll a DOM node into view only when it is necessary.
    ///
    /// The browser owns the visibility decision, avoiding a separate layout
    /// probe and avoiding a scroll when the target is already actionable.
    pub async fn scroll_into_view_if_needed(
        &self,
        node_id: Option<i64>,
        backend_node_id: Option<i64>,
    ) -> Result<Value, CdpError> {
        let mut params = serde_json::Map::new();
        if let Some(node_id) = node_id {
            params.insert("nodeId".to_string(), Value::from(node_id));
        }
        if let Some(backend_node_id) = backend_node_id {
            params.insert("backendNodeId".to_string(), Value::from(backend_node_id));
        }
        if params.is_empty() {
            return Err(CdpError::transport(
                "scrollIntoViewIfNeeded requires a nodeId or backendNodeId",
            ));
        }
        self.send("DOM.scrollIntoViewIfNeeded", Some(Value::Object(params)))
            .await
    }

    async fn get_box_model_inner(
        &self,
        node_id: Option<i64>,
        backend_node_id: Option<i64>,
    ) -> Result<Value, CdpError> {
        let mut params = serde_json::Map::new();
        if let Some(node_id) = node_id {
            params.insert("nodeId".to_string(), Value::from(node_id));
        }
        if let Some(backend_node_id) = backend_node_id {
            params.insert("backendNodeId".to_string(), Value::from(backend_node_id));
        }
        self.send("DOM.getBoxModel", Some(Value::Object(params)))
            .await
    }

    /// Evaluate JavaScript in the page.
    pub async fn evaluate(&self, expression: &str) -> Result<Value, CdpError> {
        let context_id = self.current_route().context_id;
        self.evaluate_in_context(expression, context_id).await
    }

    pub async fn evaluate_in_context(
        &self,
        expression: &str,
        context_id: Option<i64>,
    ) -> Result<Value, CdpError> {
        let mut params = serde_json::json!({
            "expression": expression,
            "returnByValue": true,
            "awaitPromise": true
        });
        if let Some(context_id) = context_id {
            params["contextId"] = Value::from(context_id);
        }
        self.send("Runtime.evaluate", Some(params)).await
    }

    /// Insert text into the currently focused element.
    pub async fn insert_text(&self, text: &str) -> Result<Value, CdpError> {
        self.send(
            "Input.insertText",
            Some(serde_json::json!({ "text": text })),
        )
        .await
    }

    /// Dispatch a mouse event via CDP Input.
    pub async fn dispatch_mouse_event(
        &self,
        event_type: &str,
        x: f64,
        y: f64,
        button: Option<&str>,
        click_count: Option<u32>,
    ) -> Result<Value, CdpError> {
        let mut params = serde_json::json!({
            "type": event_type,
            "x": x,
            "y": y,
        });
        if let Some(button) = button {
            params["button"] = Value::from(button);
        }
        if let Some(click_count) = click_count {
            params["clickCount"] = Value::from(click_count);
        }
        self.send("Input.dispatchMouseEvent", Some(params)).await
    }

    /// Dispatch one mouse event with a caller-owned response window.
    ///
    /// This does not alter the connection default used by ordinary input.
    pub async fn dispatch_mouse_event_with_timeout(
        &self,
        event_type: &str,
        x: f64,
        y: f64,
        button: Option<&str>,
        click_count: Option<u32>,
        timeout: Duration,
    ) -> Result<Value, CdpError> {
        let mut params = serde_json::json!({"type": event_type, "x": x, "y": y});
        if let Some(button) = button {
            params["button"] = Value::from(button);
        }
        if let Some(click_count) = click_count {
            params["clickCount"] = Value::from(click_count);
        }
        self.send_with_timeout("Input.dispatchMouseEvent", Some(params), timeout)
            .await
    }

    /// Dispatch a keyboard event via CDP Input.
    pub async fn dispatch_key_event(
        &self,
        event_type: &str,
        key: &str,
        code: &str,
    ) -> Result<Value, CdpError> {
        self.send(
            "Input.dispatchKeyEvent",
            Some(serde_json::json!({
                "type": event_type,
                "key": key,
                "code": code,
                "text": if event_type == "keyDown" { key } else { "" }
            })),
        )
        .await
    }

    pub async fn dispatch_key_event_with_modifiers(
        &self,
        event_type: &str,
        key: &str,
        code: &str,
        text: &str,
        modifiers: i64,
    ) -> Result<Value, CdpError> {
        let virtual_key_code = match key {
            "Backspace" => 8,
            "Tab" => 9,
            "Enter" => 13,
            "Escape" => 27,
            "Delete" => 46,
            _ if key.len() == 1 => key.as_bytes()[0].to_ascii_uppercase() as i64,
            _ => 0,
        };
        self.send(
            "Input.dispatchKeyEvent",
            Some(serde_json::json!({
                "type": event_type,
                "key": key,
                "code": code,
                "text": text,
                "modifiers": modifiers,
                "windowsVirtualKeyCode": virtual_key_code,
                "nativeVirtualKeyCode": virtual_key_code
            })),
        )
        .await
    }

    /// Invoke Blink's platform-independent editing command on the focused node.
    pub async fn dispatch_select_all(&self) -> Result<Value, CdpError> {
        self.send(
            "Input.dispatchKeyEvent",
            Some(serde_json::json!({
                "type": "rawKeyDown",
                "key": "a",
                "code": "KeyA",
                "commands": ["selectAll"]
            })),
        )
        .await
    }

    pub async fn set_file_input_files(
        &self,
        node_id: Option<i64>,
        backend_node_id: Option<i64>,
        files: &[String],
    ) -> Result<Value, CdpError> {
        let mut params = serde_json::json!({"files": files});
        if let Some(node_id) = node_id {
            params["nodeId"] = Value::from(node_id);
        }
        if let Some(backend_node_id) = backend_node_id {
            params["backendNodeId"] = Value::from(backend_node_id);
        }
        self.send("DOM.setFileInputFiles", Some(params)).await
    }

    /// Scroll the current page by a delta in CSS pixels.
    pub async fn scroll_by(&self, dx: f64, dy: f64) -> Result<Value, CdpError> {
        let expression = format!(
            "window.scrollBy({:.4}, {:.4}); window.scrollX + ',' + window.scrollY",
            dx, dy
        );
        self.evaluate(&expression).await
    }

    pub async fn get_cookies(&self) -> Result<Value, CdpError> {
        self.send("Network.getCookies", None).await
    }

    pub async fn set_cookies(&self, cookies: Value) -> Result<Value, CdpError> {
        self.send(
            "Network.setCookies",
            Some(serde_json::json!({ "cookies": cookies })),
        )
        .await
    }

    pub async fn clear_browser_cookies(&self) -> Result<(), CdpError> {
        self.send("Network.clearBrowserCookies", None).await?;
        Ok(())
    }
    pub async fn enable_page(&self) -> Result<(), CdpError> {
        self.send("Page.enable", None).await?;
        Ok(())
    }

    /// Enable the only event domains required to wait for navigation and
    /// invalidate compact observations.
    pub async fn enable_observation_events(&self) -> Result<(), CdpError> {
        self.enable_page().await?;
        self.enable_dom().await?;
        Ok(())
    }

    pub async fn enable_observation_events_for(&self, session_id: &str) -> Result<(), CdpError> {
        self.send_to_session(session_id, "Page.enable", None)
            .await?;
        self.send_to_session(session_id, "DOM.enable", None).await?;
        Ok(())
    }

    pub async fn enable_runtime(&self) -> Result<(), CdpError> {
        self.send("Runtime.enable", None).await?;
        Ok(())
    }

    pub async fn disable_runtime(&self) -> Result<(), CdpError> {
        self.send("Runtime.disable", None).await?;
        Ok(())
    }

    pub async fn enable_log(&self) -> Result<(), CdpError> {
        self.send("Log.enable", None).await?;
        Ok(())
    }

    pub async fn disable_log(&self) -> Result<(), CdpError> {
        self.send("Log.disable", None).await?;
        Ok(())
    }

    pub async fn enable_network(&self) -> Result<(), CdpError> {
        self.send("Network.enable", None).await?;
        Ok(())
    }

    pub async fn disable_network(&self) -> Result<(), CdpError> {
        self.send("Network.disable", None).await?;
        Ok(())
    }

    pub async fn handle_javascript_dialog(&self, accept: bool) -> Result<Value, CdpError> {
        self.send(
            "Page.handleJavaScriptDialog",
            Some(serde_json::json!({"accept": accept})),
        )
        .await
    }

    pub async fn set_download_behavior(
        &self,
        behavior: &str,
        download_path: Option<&Path>,
        events_enabled: bool,
    ) -> Result<Value, CdpError> {
        let mut params = serde_json::json!({
            "behavior": behavior,
            "eventsEnabled": events_enabled
        });
        if let Some(path) = download_path {
            params["downloadPath"] = Value::from(path.to_string_lossy().into_owned());
        }
        self.send_browser("Browser.setDownloadBehavior", Some(params))
            .await
    }

    pub async fn enable_dom(&self) -> Result<(), CdpError> {
        self.send("DOM.enable", None).await?;
        Ok(())
    }

    pub async fn enable_accessibility(&self) -> Result<(), CdpError> {
        self.send("Accessibility.enable", None).await?;
        Ok(())
    }

    /// Ask an owned Chrome browser to close itself before process-level
    /// shutdown. This gives profile-backed state a chance to flush cleanly.
    pub async fn close_browser(&self) -> Result<(), CdpError> {
        self.send_browser("Browser.close", None).await?;
        Ok(())
    }

    /// Override device metrics for viewport emulation.
    pub async fn set_device_metrics_override(
        &self,
        width: i64,
        height: i64,
        device_scale_factor: f64,
        mobile: bool,
    ) -> Result<Value, CdpError> {
        self.send(
            "Emulation.setDeviceMetricsOverride",
            Some(serde_json::json!({
                "width": width,
                "height": height,
                "deviceScaleFactor": device_scale_factor,
                "mobile": mobile,
            })),
        )
        .await
    }

    /// Clear device metrics override.
    pub async fn clear_device_metrics_override(&self) -> Result<Value, CdpError> {
        self.send("Emulation.clearDeviceMetricsOverride", None)
            .await
    }

    /// Ask the connection task to close its WebSocket.
    pub async fn close(&self) {
        let _ = self.tx.send(Command::Close);
    }
}

struct ScreencastDispatch<'a> {
    sink: &'a std::sync::Mutex<Option<ScreencastSink>>,
    received: &'a AtomicU64,
    dropped: &'a AtomicU64,
    command_tx: &'a mpsc::UnboundedSender<Command>,
    next_id: &'a AtomicU64,
}

fn handle_incoming_message(
    pending: &mut HashMap<u64, oneshot::Sender<Result<Value, CdpError>>>,
    events: &broadcast::Sender<CdpEvent>,
    payload_events: &broadcast::Sender<CdpEventWithParams>,
    screencast: ScreencastDispatch<'_>,
    text: &str,
) {
    let message: IncomingMessage = match serde_json::from_str(text) {
        Ok(message) => message,
        Err(error) => {
            warn!(%error, "ignoring malformed CDP message");
            return;
        }
    };

    if let Some(id) = message.id {
        if let Some(response) = pending.remove(&id) {
            let result = match message.error {
                Some(error) => Err(error),
                None => Ok(message.result.unwrap_or(Value::Null)),
            };
            let _ = response.send(result);
        } else {
            debug!(id, "received CDP response with no pending request");
        }
        return;
    }

    if let Some(method) = message.method {
        if method == "Page.screencastFrame" {
            match serde_json::from_str::<IncomingEventParams>(text) {
                Ok(mut payload) => {
                    let frame_session_id = payload.params["sessionId"].as_u64();
                    if let Some(frame_session_id) = frame_session_id {
                        let id = screencast.next_id.fetch_add(1, Ordering::Relaxed);
                        let mut ack = serde_json::json!({
                            "id": id,
                            "method": "Page.screencastFrameAck",
                            "params": {"sessionId": frame_session_id}
                        });
                        if let Some(session_id) = message.session_id.as_deref() {
                            ack["sessionId"] = Value::from(session_id);
                        }
                        let _ = screencast.command_tx.send(Command::FireAndForget {
                            json: ack.to_string(),
                        });
                    }
                    let data = payload.params["data"].take();
                    let metadata = payload.params["metadata"].take();
                    let frame = match data {
                        Value::String(data) => Some(CdpScreencastFrame {
                            data,
                            metadata,
                            session_id: message.session_id,
                        }),
                        _ => None,
                    };
                    if let Some(frame) = frame {
                        let sink = screencast.sink.lock().expect("screencast sink poisoned");
                        if let Some(sink) = sink.as_ref()
                            && sink.session_id == frame.session_id
                        {
                            screencast.received.fetch_add(1, Ordering::Relaxed);
                            if frame.data.len() > 32 * 1024 * 1024
                                || sink.sender.try_send(frame).is_err()
                            {
                                screencast.dropped.fetch_add(1, Ordering::Relaxed);
                            }
                        }
                    }
                }
                Err(error) => warn!(%error, "ignoring malformed screencast payload"),
            }
            let _ = events.send(CdpEvent { method });
            return;
        }
        let _ = events.send(CdpEvent {
            method: method.clone(),
        });
        if payload_events.receiver_count() > 0 {
            match serde_json::from_str::<IncomingEventParams>(text) {
                Ok(payload) => {
                    let _ = payload_events.send(CdpEventWithParams {
                        method,
                        params: payload.params,
                        session_id: message.session_id,
                    });
                }
                Err(error) => warn!(%error, "ignoring malformed CDP event payload"),
            }
        }
    }
}

/// Exercise the production CDP envelope and event payload decoders.
#[cfg(feature = "fuzzing")]
#[doc(hidden)]
pub fn fuzz_incoming_message(text: &str) {
    if let Ok(message) = serde_json::from_str::<IncomingMessage>(text)
        && message.method.is_some()
    {
        let _ = serde_json::from_str::<IncomingEventParams>(text);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures_util::{SinkExt, StreamExt};
    use tokio::net::TcpListener;
    use tokio_tungstenite::accept_async;

    #[tokio::test]
    async fn routes_concurrent_responses_by_id_and_delivers_events() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();

        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let websocket = accept_async(stream).await.unwrap();
            let (mut write, mut read) = websocket.split();

            let first = read.next().await.unwrap().unwrap();
            let second = read.next().await.unwrap().unwrap();
            let first: Value = match first {
                Message::Text(text) => serde_json::from_str(text.as_ref()).unwrap(),
                _ => panic!("expected text frame"),
            };
            let second: Value = match second {
                Message::Text(text) => serde_json::from_str(text.as_ref()).unwrap(),
                _ => panic!("expected text frame"),
            };

            write
                .send(Message::Text(
                    serde_json::json!({
                        "method": "Page.loadEventFired",
                        "params": {"frameId": "main"}
                    })
                    .to_string()
                    .into(),
                ))
                .await
                .unwrap();

            for request in [second, first] {
                write
                    .send(Message::Text(
                        serde_json::json!({
                            "id": request["id"],
                            "result": {"method": request["method"]}
                        })
                        .to_string()
                        .into(),
                    ))
                    .await
                    .unwrap();
            }
        });

        let client =
            CdpClient::connect_with_timeout(&format!("ws://{address}"), Duration::from_secs(2))
                .await
                .unwrap();
        let mut events = client.subscribe_events();

        let (first, second) =
            tokio::join!(client.send("first", None), client.send("second", None),);
        assert_eq!(first.unwrap()["method"], "first");
        assert_eq!(second.unwrap()["method"], "second");
        assert_eq!(events.recv().await.unwrap().method, "Page.loadEventFired");

        client.close().await;
        server.await.unwrap();
    }

    #[tokio::test]
    async fn operation_route_is_immutable_across_selection_changes() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let mut websocket = accept_async(stream).await.unwrap();
            for expected_session in ["old", "old", "new"] {
                let request = websocket.next().await.unwrap().unwrap();
                let request: Value = match request {
                    Message::Text(text) => serde_json::from_str(text.as_ref()).unwrap(),
                    _ => panic!("expected text frame"),
                };
                assert_eq!(request["sessionId"], expected_session);
                websocket
                    .send(Message::Text(
                        serde_json::json!({"id": request["id"], "result": {}})
                            .to_string()
                            .into(),
                    ))
                    .await
                    .unwrap();
            }
        });
        let client = CdpClient::connect(&format!("ws://{address}"))
            .await
            .unwrap();
        client.set_active_target_route(
            Some("old-target".to_string()),
            Some("old".to_string()),
            Some("old-frame".to_string()),
            None,
        );
        client
            .with_current_route(async {
                client.send("first", None).await.unwrap();
                client.set_active_target_route(
                    Some("new-target".to_string()),
                    Some("new".to_string()),
                    Some("new-frame".to_string()),
                    None,
                );
                assert_eq!(
                    client.operation_identity(),
                    Some(("old-target".to_string(), "old-frame".to_string()))
                );
                client.send("second", None).await.unwrap();
            })
            .await;
        client.send("third", None).await.unwrap();
        client.close().await;
        server.await.unwrap();
    }

    #[tokio::test]
    async fn delivers_event_payloads_only_to_opt_in_subscribers() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let mut websocket = accept_async(stream).await.unwrap();
            let request = websocket.next().await.unwrap().unwrap();
            let request: Value = match request {
                Message::Text(text) => serde_json::from_str(text.as_ref()).unwrap(),
                _ => panic!("expected text frame"),
            };

            for index in 0..5 {
                let route = if index == 0 { "foreign" } else { "wanted" };
                websocket
                    .send(Message::Text(
                        serde_json::json!({
                            "method": "Page.screencastFrame",
                            "sessionId": route,
                            "params": {"sessionId": 9, "data": format!("frame-{index}")}
                        })
                        .to_string()
                        .into(),
                    ))
                    .await
                    .unwrap();
                let ack = websocket.next().await.unwrap().unwrap();
                let ack: Value = match ack {
                    Message::Text(text) => serde_json::from_str(text.as_ref()).unwrap(),
                    _ => panic!("expected text frame"),
                };
                assert_eq!(ack["method"], "Page.screencastFrameAck");
                assert_eq!(ack["params"]["sessionId"], 9);
                assert_eq!(ack["sessionId"], route);
            }
            websocket
                .send(Message::Text(
                    serde_json::json!({"id": request["id"], "result": {}})
                        .to_string()
                        .into(),
                ))
                .await
                .unwrap();
        });

        let client = CdpClient::connect(&format!("ws://{address}"))
            .await
            .unwrap();
        let mut methods = client.subscribe_events();
        let mut payloads = client.subscribe_events_with_params();
        let mut frames = client
            .open_screencast_channel(Some("wanted".to_string()))
            .unwrap();
        client.send("test.ready", None).await.unwrap();

        assert_eq!(methods.recv().await.unwrap().method, "Page.screencastFrame");
        assert_eq!(frames.recv().await.unwrap().data, "frame-1");
        assert_eq!(frames.recv().await.unwrap().data, "frame-2");
        assert!(payloads.try_recv().is_err());
        assert_eq!(client.screencast_stats(), (4, 2));

        client.close().await;
        server.await.unwrap();
    }

    #[tokio::test]
    async fn requests_fast_screenshot_encoding_and_moves_the_payload() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let mut websocket = accept_async(stream).await.unwrap();
            let request = websocket.next().await.unwrap().unwrap();
            let request: Value = match request {
                Message::Text(text) => serde_json::from_str(text.as_ref()).unwrap(),
                _ => panic!("expected text frame"),
            };
            assert_eq!(request["method"], "Page.captureScreenshot");
            assert_eq!(request["params"]["format"], "png");
            assert_eq!(request["params"]["optimizeForSpeed"], true);
            websocket
                .send(Message::Text(
                    serde_json::json!({
                        "id": request["id"],
                        "result": {"data": "cG5n"}
                    })
                    .to_string()
                    .into(),
                ))
                .await
                .unwrap();
        });

        let client = CdpClient::connect(&format!("ws://{address}"))
            .await
            .unwrap();
        assert_eq!(client.screenshot("png").await.unwrap(), "cG5n");
        client.close().await;
        server.await.unwrap();
    }

    #[tokio::test]
    async fn selector_lookup_fetches_only_the_document_root() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let mut websocket = accept_async(stream).await.unwrap();

            let root_request = websocket.next().await.unwrap().unwrap();
            let root_request: Value = match root_request {
                Message::Text(text) => serde_json::from_str(text.as_ref()).unwrap(),
                _ => panic!("expected text frame"),
            };
            assert_eq!(root_request["method"], "DOM.getDocument");
            assert_eq!(root_request["params"], serde_json::json!({ "depth": 0 }));
            websocket
                .send(Message::Text(
                    serde_json::json!({
                        "id": root_request["id"],
                        "result": {"root": {"nodeId": 42}}
                    })
                    .to_string()
                    .into(),
                ))
                .await
                .unwrap();

            let selector_request = websocket.next().await.unwrap().unwrap();
            let selector_request: Value = match selector_request {
                Message::Text(text) => serde_json::from_str(text.as_ref()).unwrap(),
                _ => panic!("expected text frame"),
            };
            assert_eq!(selector_request["method"], "DOM.querySelector");
            assert_eq!(
                selector_request["params"],
                serde_json::json!({ "nodeId": 42, "selector": "#save" })
            );
            websocket
                .send(Message::Text(
                    serde_json::json!({
                        "id": selector_request["id"],
                        "result": {"nodeId": 7}
                    })
                    .to_string()
                    .into(),
                ))
                .await
                .unwrap();
        });

        let client = CdpClient::connect(&format!("ws://{address}"))
            .await
            .unwrap();
        assert_eq!(client.query_selector("#save").await.unwrap()["nodeId"], 7);
        client.close().await;
        server.await.unwrap();
    }

    #[tokio::test]
    async fn backend_identity_describes_the_existing_frontend_node_without_a_second_query() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let mut websocket = accept_async(stream).await.unwrap();
            let request = websocket.next().await.unwrap().unwrap();
            let request: Value = match request {
                Message::Text(text) => serde_json::from_str(text.as_ref()).unwrap(),
                _ => panic!("expected text frame"),
            };
            assert_eq!(request["method"], "DOM.describeNode");
            assert_eq!(
                request["params"],
                serde_json::json!({"nodeId": 17, "depth": 0})
            );
            websocket
                .send(Message::Text(
                    serde_json::json!({
                        "id": request["id"],
                        "result": {"node": {"backendNodeId": 91}}
                    })
                    .to_string()
                    .into(),
                ))
                .await
                .unwrap();
            assert!(
                tokio::time::timeout(Duration::from_millis(25), websocket.next())
                    .await
                    .is_err(),
                "backend translation must not repeat the selector query"
            );
        });
        let client = CdpClient::connect(&format!("ws://{address}"))
            .await
            .unwrap();
        assert_eq!(client.backend_node_id_for_node(17).await.unwrap(), 91);
        server.await.unwrap();
        client.close().await;
    }

    #[tokio::test]
    async fn backend_identity_rejects_a_describe_response_without_backend_id() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let mut websocket = accept_async(stream).await.unwrap();
            let request = websocket.next().await.unwrap().unwrap();
            let request: Value = match request {
                Message::Text(text) => serde_json::from_str(text.as_ref()).unwrap(),
                _ => panic!("expected text frame"),
            };
            websocket
                .send(Message::Text(
                    serde_json::json!({"id": request["id"], "result": {"node": {}}})
                        .to_string()
                        .into(),
                ))
                .await
                .unwrap();
        });
        let client = CdpClient::connect(&format!("ws://{address}"))
            .await
            .unwrap();
        let error = client.backend_node_id_for_node(17).await.unwrap_err();
        assert!(error.message.contains("no backendNodeId"));
        client.close().await;
        server.await.unwrap();
    }

    #[tokio::test]
    async fn scroll_into_view_uses_the_backend_node_without_a_layout_probe() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let mut websocket = accept_async(stream).await.unwrap();
            let request = websocket.next().await.unwrap().unwrap();
            let request: Value = match request {
                Message::Text(text) => serde_json::from_str(text.as_ref()).unwrap(),
                _ => panic!("expected text frame"),
            };
            assert_eq!(request["method"], "DOM.scrollIntoViewIfNeeded");
            assert_eq!(
                request["params"],
                serde_json::json!({ "backendNodeId": 42 })
            );
            websocket
                .send(Message::Text(
                    serde_json::json!({"id": request["id"], "result": {}})
                        .to_string()
                        .into(),
                ))
                .await
                .unwrap();
        });

        let client = CdpClient::connect(&format!("ws://{address}"))
            .await
            .unwrap();
        client
            .scroll_into_view_if_needed(None, Some(42))
            .await
            .unwrap();
        client.close().await;
        server.await.unwrap();
    }

    #[tokio::test]
    async fn observation_event_setup_enables_only_page_and_dom() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let mut websocket = accept_async(stream).await.unwrap();
            let mut methods = Vec::new();

            for _ in 0..2 {
                let request = websocket.next().await.unwrap().unwrap();
                let request: Value = match request {
                    Message::Text(text) => serde_json::from_str(text.as_ref()).unwrap(),
                    _ => panic!("expected text frame"),
                };
                methods.push(request["method"].as_str().unwrap().to_string());
                websocket
                    .send(Message::Text(
                        serde_json::json!({"id": request["id"], "result": {}})
                            .to_string()
                            .into(),
                    ))
                    .await
                    .unwrap();
            }

            assert_eq!(methods, ["Page.enable", "DOM.enable"]);
        });

        let client = CdpClient::connect(&format!("ws://{address}"))
            .await
            .unwrap();
        client.enable_observation_events().await.unwrap();
        client.close().await;
        server.await.unwrap();
    }

    #[tokio::test]
    async fn sends_browser_close_for_owned_session_shutdown() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let mut websocket = accept_async(stream).await.unwrap();
            let request = websocket.next().await.unwrap().unwrap();
            let request: Value = match request {
                Message::Text(text) => serde_json::from_str(text.as_ref()).unwrap(),
                _ => panic!("expected text frame"),
            };
            assert_eq!(request["method"], "Browser.close");
            assert!(request.get("params").is_none());
            websocket
                .send(Message::Text(
                    serde_json::json!({"id": request["id"], "result": {}})
                        .to_string()
                        .into(),
                ))
                .await
                .unwrap();
        });

        let client = CdpClient::connect(&format!("ws://{address}"))
            .await
            .unwrap();
        client.close_browser().await.unwrap();
        client.close().await;
        server.await.unwrap();
    }

    #[tokio::test]
    async fn returns_a_timeout_when_the_server_does_not_respond() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let (_write, mut read) = accept_async(stream).await.unwrap().split();
            let _ = read.next().await;
            tokio::time::sleep(Duration::from_millis(200)).await;
        });

        let client =
            CdpClient::connect_with_timeout(&format!("ws://{address}"), Duration::from_millis(50))
                .await
                .unwrap();
        let error = client.send("never", None).await.unwrap_err();
        assert!(error.message.contains("timeout"));
        assert!(error.is_response_timeout());
        client.close().await;
        server.await.unwrap();
    }

    #[tokio::test]
    async fn operation_timeout_is_short_and_does_not_change_the_connection_default() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let mut websocket = accept_async(stream).await.unwrap();
            let first = websocket.next().await.unwrap().unwrap();
            let first: Value = match first {
                Message::Text(text) => serde_json::from_str(text.as_ref()).unwrap(),
                _ => panic!("expected text frame"),
            };
            assert_eq!(first["method"], "short");
            let second = websocket.next().await.unwrap().unwrap();
            let second: Value = match second {
                Message::Text(text) => serde_json::from_str(text.as_ref()).unwrap(),
                _ => panic!("expected text frame"),
            };
            assert_eq!(second["method"], "ordinary");
            websocket
                .send(Message::Text(
                    serde_json::json!({"id": second["id"], "result": {"ok": true}})
                        .to_string()
                        .into(),
                ))
                .await
                .unwrap();
        });

        let client =
            CdpClient::connect_with_timeout(&format!("ws://{address}"), Duration::from_millis(500))
                .await
                .unwrap();
        let started = tokio::time::Instant::now();
        let error = client
            .send_with_timeout("short", None, Duration::from_millis(20))
            .await
            .unwrap_err();
        assert!(error.is_response_timeout());
        assert!(started.elapsed() < Duration::from_millis(200));
        assert_eq!(client.send("ordinary", None).await.unwrap()["ok"], true);
        client.close().await;
        server.await.unwrap();
    }

    #[test]
    fn protocol_errors_are_not_typed_as_response_timeouts() {
        let error: CdpError = serde_json::from_value(serde_json::json!({
            "code": -32000,
            "message": "some protocol failure"
        }))
        .unwrap();
        assert!(!error.is_response_timeout());
    }
}
