//! Loopback-only, token-scoped browser view transport for SSH forwarding.

use super::{DevelopmentError, DevelopmentResult};
use base64::Engine;
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use std::{net::Ipv4Addr, sync::Arc};
use tokio::{
    net::{TcpListener, TcpStream},
    sync::{Semaphore, mpsc, watch},
    task::JoinHandle,
    time::{Duration, timeout},
};
use tokio_tungstenite::{accept_async, tungstenite};

const MAX_CLIENTS: usize = 4;
const MAX_INPUT_BYTES: usize = 16 * 1024;
const MAX_FRAME_DATA_BYTES: usize = 8 * 1024 * 1024;
const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(3);

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RemoteFrame {
    pub browser_revision: u64,
    pub mime_type: String,
    pub data: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum RemoteInput {
    Click {
        x: f64,
        y: f64,
        expected_revision: u64,
    },
    Scroll {
        dx: f64,
        dy: f64,
        expected_revision: u64,
    },
    Key {
        key: String,
        expected_revision: u64,
    },
    Text {
        text: String,
        expected_revision: u64,
    },
}

impl RemoteInput {
    pub fn expected_revision(&self) -> u64 {
        match self {
            Self::Click {
                expected_revision, ..
            }
            | Self::Scroll {
                expected_revision, ..
            }
            | Self::Key {
                expected_revision, ..
            }
            | Self::Text {
                expected_revision, ..
            } => *expected_revision,
        }
    }

    fn validate(&self) -> DevelopmentResult<()> {
        match self {
            Self::Click { x, y, .. }
                if !x.is_finite()
                    || !y.is_finite()
                    || !(0.0..=1.0).contains(x)
                    || !(0.0..=1.0).contains(y) =>
            {
                Err(DevelopmentError::InvalidInput(
                    "remote click coordinates must be finite normalized values".into(),
                ))
            }
            Self::Scroll { dx, dy, .. }
                if !dx.is_finite()
                    || !dy.is_finite()
                    || dx.abs() > 10_000.0
                    || dy.abs() > 10_000.0 =>
            {
                Err(DevelopmentError::InvalidInput(
                    "remote scroll delta is outside the bounded range".into(),
                ))
            }
            Self::Key { key, .. } if key.is_empty() || key.len() > 128 => Err(
                DevelopmentError::InvalidInput("remote key must contain 1 to 128 bytes".into()),
            ),
            Self::Text { text, .. } if text.len() > 8 * 1024 => Err(
                DevelopmentError::InvalidInput("remote text exceeds 8192 bytes".into()),
            ),
            _ => Ok(()),
        }
    }
}

/// Running Remote View endpoint. It never owns a browser session: the caller
/// publishes frames from and applies inputs to its existing session worker.
pub struct RemoteView {
    port: u16,
    token: String,
    frame: watch::Sender<Option<RemoteFrame>>,
    inputs: mpsc::Receiver<RemoteInput>,
    shutdown: watch::Sender<bool>,
    task: JoinHandle<()>,
}

impl std::fmt::Debug for RemoteView {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("RemoteView")
            .field("port", &self.port)
            .finish_non_exhaustive()
    }
}

impl RemoteView {
    pub async fn bind() -> DevelopmentResult<Self> {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).await?;
        let port = listener.local_addr()?.port();
        let token = secure_token()?;
        let (frame, frame_rx) = watch::channel(None);
        let (input_tx, inputs) = mpsc::channel(64);
        let (shutdown, shutdown_rx) = watch::channel(false);
        let task_token = token.clone();
        let task = tokio::spawn(serve(listener, task_token, frame_rx, input_tx, shutdown_rx));
        Ok(Self {
            port,
            token,
            frame,
            inputs,
            shutdown,
            task,
        })
    }

    pub fn port(&self) -> u16 {
        self.port
    }

    pub fn local_url(&self) -> String {
        format!("http://127.0.0.1:{}/{}/", self.port, self.token)
    }

    pub fn ssh_forward_hint(&self) -> String {
        format!("ssh -N -L {0}:127.0.0.1:{0} USER@HOST", self.port)
    }

    /// Replace the pending frame when it is safe to retain and serve.
    /// Returns false for an unsupported MIME type or oversized payload.
    pub fn publish(&self, frame: RemoteFrame) -> bool {
        if frame.mime_type != "image/png" || frame.data.len() > MAX_FRAME_DATA_BYTES {
            return false;
        }
        self.frame.send_replace(Some(frame));
        true
    }

    pub fn try_recv_input(&mut self) -> Result<RemoteInput, mpsc::error::TryRecvError> {
        self.inputs.try_recv()
    }

    pub async fn revoke(self) {
        let _ = self.shutdown.send(true);
        let _ = self.task.await;
    }
}

async fn serve(
    listener: TcpListener,
    token: String,
    frame: watch::Receiver<Option<RemoteFrame>>,
    inputs: mpsc::Sender<RemoteInput>,
    mut shutdown: watch::Receiver<bool>,
) {
    let clients = Arc::new(Semaphore::new(MAX_CLIENTS));
    loop {
        tokio::select! {
            changed = shutdown.changed() => if changed.is_err() || *shutdown.borrow() { break },
            accepted = listener.accept() => {
                let Ok((stream, address)) = accepted else { continue };
                if !address.ip().is_loopback() { continue; }
                let Ok(permit) = Arc::clone(&clients).try_acquire_owned() else { continue };
                let token = token.clone();
                let frame = frame.clone();
                let inputs = inputs.clone();
                let revoked = shutdown.clone();
                tokio::spawn(async move {
                    let _permit = permit;
                    let _ = handle_client(stream, token, frame, inputs, revoked).await;
                });
            }
        }
    }
}

async fn handle_client(
    mut stream: TcpStream,
    token: String,
    mut frame: watch::Receiver<Option<RemoteFrame>>,
    inputs: mpsc::Sender<RemoteInput>,
    mut revoked: watch::Receiver<bool>,
) -> DevelopmentResult<()> {
    let mut peek = [0_u8; 4096];
    let size = timeout(HANDSHAKE_TIMEOUT, stream.peek(&mut peek))
        .await
        .map_err(|_| DevelopmentError::Process("remote view handshake timed out".into()))??;
    let request = std::str::from_utf8(&peek[..size]).unwrap_or("");
    let expected_page = format!("GET /{token}/ ");
    if request.starts_with(&expected_page) {
        // Consume the request before closing the socket. Windows sends an RST
        // when a socket is closed with unread inbound bytes, which can discard
        // an otherwise complete HTTP response at the client.
        let mut request_bytes = vec![0_u8; size];
        tokio::io::AsyncReadExt::read_exact(&mut stream, &mut request_bytes).await?;
        let body = viewer_html(&token);
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nCache-Control: no-store\r\nContent-Security-Policy: default-src 'self'; img-src 'self' data:; script-src 'unsafe-inline'; connect-src 'self'\r\nX-Frame-Options: DENY\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            body.len(),
            body
        );
        tokio::io::AsyncWriteExt::write_all(&mut stream, response.as_bytes()).await?;
        tokio::io::AsyncWriteExt::shutdown(&mut stream).await?;
        return Ok(());
    }
    if !request.starts_with(&format!("GET /ws/{token} ")) {
        tokio::io::AsyncWriteExt::write_all(
            &mut stream,
            b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
        )
        .await?;
        return Ok(());
    }
    let mut socket = accept_async(stream).await.map_err(|error| {
        DevelopmentError::Process(format!("remote view handshake failed: {error}"))
    })?;

    loop {
        tokio::select! {
            changed = revoked.changed() => if changed.is_err() || *revoked.borrow() { break },
            changed = frame.changed() => {
                if changed.is_err() { break; }
                let latest = { frame.borrow().clone() };
                if let Some(value) = latest {
                    let text = serde_json::to_string(&value)?;
                    socket.send(tungstenite::Message::Text(text.into())).await.map_err(|error| DevelopmentError::Process(error.to_string()))?;
                }
            }
            message = socket.next() => match message {
                Some(Ok(tungstenite::Message::Text(text))) if text.len() <= MAX_INPUT_BYTES => {
                    let input: RemoteInput = serde_json::from_str(&text)?;
                    input.validate()?;
                    inputs.try_send(input).map_err(|_| DevelopmentError::Process("remote input queue is full".into()))?;
                }
                Some(Ok(tungstenite::Message::Close(_))) | None => break,
                Some(Ok(_)) => {}
                Some(Err(error)) => return Err(DevelopmentError::Process(error.to_string())),
            }
        }
    }
    Ok(())
}

fn secure_token() -> DevelopmentResult<String> {
    let mut bytes = [0_u8; 32];
    getrandom::fill(&mut bytes).map_err(|error| {
        DevelopmentError::Process(format!("secure token generation failed: {error}"))
    })?;
    Ok(base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes))
}

fn viewer_html(token: &str) -> String {
    format!(
        r#"<!doctype html><meta name=viewport content="width=device-width,initial-scale=1,viewport-fit=cover"><title>Glass Remote View</title><style>html,body{{margin:0;background:#101418;color:#eef;font:14px system-ui}}header{{padding:10px}}img{{display:block;width:100%;height:auto;touch-action:none}}input{{box-sizing:border-box;width:100%;padding:12px;background:#172231;color:#eef;border:1px solid #39485d}}</style><header>Glass Remote View · loopback session</header><img id=v tabindex=0 alt="Browser frame"><input id=t autocomplete=off placeholder="Type into focused browser control, then press Enter"><script>const v=document.querySelector('#v'),t=document.querySelector('#t');let r=0;const send=o=>{{if(w.readyState===1)w.send(JSON.stringify({{...o,expectedRevision:r}}))}},w=new WebSocket(`ws://${{location.host}}/ws/{token}`);w.onmessage=e=>{{const f=JSON.parse(e.data);r=f.browserRevision;v.src=`data:${{f.mimeType}};base64,${{f.data}}`}};v.onclick=e=>{{const b=v.getBoundingClientRect();send({{type:'click',x:(e.clientX-b.left)/b.width,y:(e.clientY-b.top)/b.height}});v.focus()}};v.onwheel=e=>{{e.preventDefault();send({{type:'scroll',dx:e.deltaX,dy:e.deltaY}})}};window.onkeydown=e=>{{if(e.target!==t){{e.preventDefault();send({{type:'key',key:e.key}})}}}};t.onkeydown=e=>{{if(e.key==='Enter'&&t.value){{send({{type:'text',text:t.value}});t.value='';e.preventDefault()}}}};</script>"#
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn endpoint_is_loopback_tokenized_and_revocable() {
        let view = RemoteView::bind().await.unwrap();
        assert!(view.local_url().starts_with("http://127.0.0.1:"));
        assert!(!view.local_url().contains("="));
        assert!(view.ssh_forward_hint().contains("127.0.0.1"));
        let response = reqwest::get(view.local_url()).await.unwrap();
        assert!(response.status().is_success());
        assert!(
            response.headers()["cache-control"]
                .to_str()
                .unwrap()
                .contains("no-store")
        );
        view.revoke().await;
    }

    #[tokio::test]
    async fn websocket_delivers_latest_frame_and_revision_bound_input() {
        let mut view = RemoteView::bind().await.unwrap();
        let token = view
            .local_url()
            .trim_end_matches('/')
            .rsplit('/')
            .next()
            .unwrap()
            .to_string();
        let ws_url = format!("ws://127.0.0.1:{}/ws/{token}", view.port());
        let (mut socket, _) = tokio_tungstenite::connect_async(ws_url).await.unwrap();
        view.publish(RemoteFrame {
            browser_revision: 7,
            mime_type: "image/png".into(),
            data: "AA==".into(),
        });
        let message = timeout(Duration::from_secs(2), socket.next())
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        assert!(message.to_text().unwrap().contains("\"browserRevision\":7"));
        socket
            .send(tungstenite::Message::Text(
                serde_json::to_string(&RemoteInput::Key {
                    key: "Enter".into(),
                    expected_revision: 7,
                })
                .unwrap()
                .into(),
            ))
            .await
            .unwrap();
        let input = timeout(Duration::from_secs(2), async {
            loop {
                if let Ok(input) = view.try_recv_input() {
                    break input;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();
        assert_eq!(input.expected_revision(), 7);
        view.revoke().await;
    }

    #[test]
    fn inputs_are_revision_bound_and_bounded() {
        let input = RemoteInput::Click {
            x: 0.5,
            y: 0.25,
            expected_revision: 9,
        };
        assert_eq!(input.expected_revision(), 9);
        assert!(input.validate().is_ok());
        assert!(
            RemoteInput::Click {
                x: 2.0,
                y: 0.0,
                expected_revision: 9
            }
            .validate()
            .is_err()
        );
    }

    #[tokio::test]
    async fn frame_mailbox_rejects_unsupported_and_oversized_payloads() {
        let view = RemoteView::bind().await.unwrap();
        assert!(!view.publish(RemoteFrame {
            browser_revision: 1,
            mime_type: "text/html".into(),
            data: "x".into(),
        }));
        assert!(!view.publish(RemoteFrame {
            browser_revision: 1,
            mime_type: "image/png".into(),
            data: "x".repeat(MAX_FRAME_DATA_BYTES + 1),
        }));
        view.revoke().await;
    }
}
