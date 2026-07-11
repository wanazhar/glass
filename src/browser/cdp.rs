use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::error::Error;
use std::fmt::{Display, Formatter};
use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};
use std::time::Duration;
use tokio::sync::{broadcast, mpsc, oneshot};
use tokio_tungstenite::tungstenite::Message;
use tracing::{debug, info, warn};

/// A CDP method call request.
#[derive(Debug, Serialize)]
pub struct CdpRequest {
    pub id: u64,
    pub method: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub params: Option<Value>,
}

/// A protocol or transport error returned by a CDP connection.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CdpError {
    pub code: i64,
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}

impl CdpError {
    fn transport(message: impl Into<String>) -> Self {
        Self {
            code: -32_000,
            message: message.into(),
            data: None,
        }
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
    Close,
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
    timeout: Duration,
}

impl CdpClient {
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
                                    text.as_ref(),
                                );
                            }
                            Some(Ok(Message::Binary(bytes))) => {
                                match std::str::from_utf8(bytes.as_ref()) {
                                    Ok(text) => handle_incoming_message(
                                        &mut pending,
                                        &actor_events,
                                        &actor_payload_events,
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
            next_id: Arc::new(AtomicU64::new(1)),
            events: event_tx,
            payload_events: payload_event_tx,
            timeout,
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

    /// Send a CDP command and wait for the response.
    pub async fn send(&self, method: &str, params: Option<Value>) -> Result<Value, CdpError> {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let request = CdpRequest {
            id,
            method: method.to_string(),
            params,
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

        match tokio::time::timeout(self.timeout, response_rx).await {
            Ok(Ok(result)) => result,
            Ok(Err(_)) => Err(CdpError::transport("CDP response channel closed")),
            Err(_) => {
                let _ = self.tx.send(Command::Cancel { id });
                Err(CdpError::transport(format!(
                    "CDP response timeout after {} seconds",
                    self.timeout.as_secs_f64()
                )))
            }
        }
    }

    /// Navigate to a URL.
    pub async fn navigate(&self, url: &str) -> Result<Value, CdpError> {
        self.send("Page.navigate", Some(serde_json::json!({ "url": url })))
            .await
    }

    /// Take a screenshot and return its base64-encoded image data.
    pub async fn screenshot(&self, format: &str) -> Result<String, CdpError> {
        let mut result = self
            .send(
                "Page.captureScreenshot",
                Some(serde_json::json!({
                    "format": format,
                    "optimizeForSpeed": true
                })),
            )
            .await?;
        match result.get_mut("data").map(Value::take) {
            Some(Value::String(data)) => Ok(data),
            _ => Err(CdpError::transport(
                "CDP screenshot response contained no data",
            )),
        }
    }

    /// Get the accessibility tree.
    pub async fn get_accessibility_tree(&self) -> Result<Value, CdpError> {
        self.send("Accessibility.getFullAXTree", None).await
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
        self.send(
            "Runtime.evaluate",
            Some(serde_json::json!({
                "expression": expression,
                "returnByValue": true,
                "awaitPromise": true
            })),
        )
        .await
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

    pub async fn enable_runtime(&self) -> Result<(), CdpError> {
        self.send("Runtime.enable", None).await?;
        Ok(())
    }

    pub async fn enable_network(&self) -> Result<(), CdpError> {
        self.send("Network.enable", None).await?;
        Ok(())
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
        self.send("Browser.close", None).await?;
        Ok(())
    }

    /// Ask the connection task to close its WebSocket.
    pub async fn close(&self) {
        let _ = self.tx.send(Command::Close);
    }
}

fn handle_incoming_message(
    pending: &mut HashMap<u64, oneshot::Sender<Result<Value, CdpError>>>,
    events: &broadcast::Sender<CdpEvent>,
    payload_events: &broadcast::Sender<CdpEventWithParams>,
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
        let _ = events.send(CdpEvent {
            method: method.clone(),
        });
        if payload_events.receiver_count() > 0 {
            match serde_json::from_str::<IncomingEventParams>(text) {
                Ok(payload) => {
                    let _ = payload_events.send(CdpEventWithParams {
                        method,
                        params: payload.params,
                    });
                }
                Err(error) => warn!(%error, "ignoring malformed CDP event payload"),
            }
        }
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

            websocket
                .send(Message::Text(
                    serde_json::json!({
                        "method": "Page.screencastFrame",
                        "params": {"sessionId": 9, "data": "frame-data"}
                    })
                    .to_string()
                    .into(),
                ))
                .await
                .unwrap();
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
        client.send("test.ready", None).await.unwrap();

        assert_eq!(methods.recv().await.unwrap().method, "Page.screencastFrame");
        let payload = payloads.recv().await.unwrap();
        assert_eq!(payload.method, "Page.screencastFrame");
        assert_eq!(payload.params["sessionId"], 9);
        assert_eq!(payload.params["data"], "frame-data");

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
        client.close().await;
        server.await.unwrap();
    }
}
