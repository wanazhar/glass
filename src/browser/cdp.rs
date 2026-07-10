use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tokio::sync::{mpsc, oneshot};
use tokio_tungstenite::tungstenite::Message;
use futures_util::{SinkExt, StreamExt};
use tracing::{debug, error, info, warn};

/// A CDP method call request.
#[derive(Debug, Serialize)]
pub struct CdpRequest {
    pub id: u64,
    pub method: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub params: Option<serde_json::Value>,
}

/// Response from a CDP method call.
#[derive(Debug, Deserialize)]
pub struct CdpResponse {
    pub id: Option<u64>,
    pub result: Option<serde_json::Value>,
    pub error: Option<CdpError>,
}

/// CDP error object.
#[derive(Debug, Deserialize)]
pub struct CdpError {
    pub code: i64,
    pub message: String,
}

/// Event pushed from Chrome.
#[derive(Debug, Deserialize)]
pub struct CdpEvent {
    pub method: String,
    pub params: serde_json::Value,
}

/// Connection to Chrome DevTools Protocol.
pub struct CdpClient {
    tx: mpsc::UnboundedSender<String>,
    pending: HashMap<u64, oneshot::Sender<Result<serde_json::Value, CdpError>>>,
    next_id: u64,
}

impl CdpClient {
    /// Connect to Chrome CDP WebSocket at the given URL.
    pub async fn connect(ws_url: &str) -> Result<Self, Box<dyn std::error::Error>> {
        info!("Connecting to CDP: {ws_url}");
        let (ws_stream, _) = tokio_tungstenite::connect_async(ws_url).await?;
        let (mut write, mut read) = ws_stream.split();

        let (tx, mut rx) = mpsc::unbounded_channel::<String>();

        // Writer task: forward outgoing messages to WebSocket.
        tokio::spawn(async move {
            while let Some(msg) = rx.recv().await {
                if let Err(e) = write.send(Message::Text(msg.into())).await {
                    error!("CDP write error: {e}");
                    break;
                }
            }
        });

        let (event_tx, _event_rx) = mpsc::unbounded_channel::<CdpEvent>();

        // Reader task: dispatch responses and events.
        tokio::spawn(async move {
            while let Some(msg_result) = read.next().await {
                match msg_result {
                    Ok(Message::Text(text)) => {
                        let text_str = text.to_string();
                        debug!("CDP recv: {text_str}");
                        // Responses and events are dispatched via the channels.
                        // This is a simplified implementation; a production version
                        // would use a shared state to route by message id.
                        let _ = event_tx.send(serde_json::from_str(&text_str).unwrap_or_else(|_| CdpEvent {
                            method: "unknown".into(),
                            params: serde_json::Value::Null,
                        }));
                    }
                    Ok(Message::Close(_)) => {
                        warn!("CDP WebSocket closed by server");
                        break;
                    }
                    Err(e) => {
                        error!("CDP read error: {e}");
                        break;
                    }
                    _ => {}
                }
            }
        });

        Ok(Self {
            tx,
            pending: HashMap::new(),
            next_id: 1,
        })
    }

    /// Send a CDP command and wait for the response.
    pub async fn send(
        &mut self,
        method: &str,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
        let id = self.next_id;
        self.next_id += 1;

        let request = CdpRequest {
            id,
            method: method.to_string(),
            params,
        };

        let json = serde_json::to_string(&request)?;
        let (response_tx, response_rx) = oneshot::channel();
        self.pending.insert(id, response_tx);

        self.tx.send(json)?;

        match tokio::time::timeout(std::time::Duration::from_secs(30), response_rx).await {
            Ok(Ok(Ok(result))) => Ok(result),
            Ok(Ok(Err(e))) => Err(format!("CDP error: {} ({})", e.message, e.code).into()),
            Ok(Err(_)) => Err("Response channel closed".into()),
            Err(_) => {
                self.pending.remove(&id);
                Err("CDP response timeout".into())
            }
        }
    }

    /// Navigate to a URL.
    pub async fn navigate(&mut self, url: &str) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
        self.send("Page.navigate", Some(serde_json::json!({ "url": url }))).await
    }

    /// Take a screenshot (returns base64-encoded PNG).
    pub async fn screenshot(&mut self, format: &str) -> Result<String, Box<dyn std::error::Error>> {
        let result = self.send(
            "Page.captureScreenshot",
            Some(serde_json::json!({ "format": format })),
        ).await?;
        result["data"]
            .as_str()
            .map(String::from)
            .ok_or("No screenshot data in response".into())
    }

    /// Get the accessibility tree.
    pub async fn get_accessibility_tree(&mut self) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
        self.send("Accessibility.getFullAXTree", None).await
    }

    /// Get the document root.
    pub async fn get_document(&mut self) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
        self.send("DOM.getDocument", Some(serde_json::json!({ "depth": -1 }))).await
    }

    /// Query selector to get a node.
    pub async fn query_selector(
        &mut self,
        selector: &str,
    ) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
        let doc = self.get_document().await?;
        let root_id = doc["root"]["nodeId"]
            .as_i64()
            .ok_or("No root nodeId")?;
        self.send(
            "DOM.querySelector",
            Some(serde_json::json!({ "nodeId": root_id, "selector": selector })),
        ).await
    }

    /// Get the bounding box of a node.
    pub async fn get_box_model(
        &mut self,
        node_id: i64,
    ) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
        self.send(
            "DOM.getBoxModel",
            Some(serde_json::json!({ "nodeId": node_id })),
        ).await
    }

    /// Evaluate JavaScript in the page.
    pub async fn evaluate(
        &mut self,
        expression: &str,
    ) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
        self.send(
            "Runtime.evaluate",
            Some(serde_json::json!({ "expression": expression, "returnByValue": true })),
        ).await
    }

    /// Get all cookies.
    pub async fn get_cookies(&mut self) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
        self.send("Network.getCookies", None).await
    }

    /// Set cookies.
    pub async fn set_cookies(
        &mut self,
        cookies: serde_json::Value,
    ) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
        self.send("Network.setCookies", Some(serde_json::json!({ "cookies": cookies }))).await
    }

    /// Dispatch a mouse event via CDP Input domain.
    pub async fn dispatch_mouse_event(
        &mut self,
        event_type: &str,
        x: f64,
        y: f64,
        button: Option<&str>,
        click_count: Option<u32>,
    ) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
        let mut params = serde_json::json!({
            "type": event_type,
            "x": x,
            "y": y,
        });
        if let Some(btn) = button {
            params["button"] = serde_json::json!(btn);
        }
        if let Some(count) = click_count {
            params["clickCount"] = serde_json::json!(count);
        }
        self.send("Input.dispatchMouseEvent", Some(params)).await
    }

    /// Dispatch a keyboard event.
    pub async fn dispatch_key_event(
        &mut self,
        event_type: &str,
        key: &str,
        code: &str,
    ) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
        self.send(
            "Input.dispatchKeyEvent",
            Some(serde_json::json!({
                "type": event_type,
                "key": key,
                "code": code,
                "text": if event_type == "keyDown" { key } else { "" },
            })),
        ).await
    }

    /// Enable Page domain events.
    pub async fn enable_page(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        self.send("Page.enable", None).await?;
        Ok(())
    }

    /// Enable Runtime domain events.
    pub async fn enable_runtime(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        self.send("Runtime.enable", None).await?;
        Ok(())
    }

    /// Enable Network domain events.
    pub async fn enable_network(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        self.send("Network.enable", None).await?;
        Ok(())
    }

    /// Close the CDP connection.
    pub async fn close(&mut self) {
        let _ = self.tx.send("close".to_string());
    }
}
