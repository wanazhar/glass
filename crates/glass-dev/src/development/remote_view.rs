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
const MAX_AUDIO_ASSET_BYTES: usize = 2 * 1024 * 1024;
const MAX_AUDIO_LABEL_BYTES: usize = 128;
const MAX_AUDIO_ID_BYTES: usize = 64;
const MAX_HTTP_HEADER_BYTES: usize = 16 * 1024;
const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(3);

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RemoteFrame {
    pub browser_revision: u64,
    pub mime_type: String,
    pub data: String,
}

/// A bounded, already-decoded WAV artifact for a user-controlled Remote View
/// player. The source URL and request metadata are intentionally not retained.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoteAudioAsset {
    pub id: String,
    pub label: String,
    pub bytes: Vec<u8>,
}

/// Safe metadata sent with Remote View frames. The player requests the WAV
/// through this view's token-scoped endpoint only after the user presses play.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteAudioDescriptor {
    pub id: String,
    pub label: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct RemoteFrameMessage {
    browser_revision: u64,
    mime_type: String,
    data: String,
    audio: Option<RemoteAudioDescriptor>,
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
    audio: watch::Sender<Option<RemoteAudioAsset>>,
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
        let (audio, audio_rx) = watch::channel(None);
        let (input_tx, inputs) = mpsc::channel(64);
        let (shutdown, shutdown_rx) = watch::channel(false);
        let task_token = token.clone();
        let task = tokio::spawn(serve(
            listener,
            task_token,
            frame_rx,
            audio_rx,
            input_tx,
            shutdown_rx,
        ));
        Ok(Self {
            port,
            token,
            frame,
            audio,
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

    /// Replace the current audio artifact, or clear it with `None`.
    /// Invalid assets clear the previous artifact and return `None`, ensuring
    /// the viewer cannot continue to play stale media after a failed update.
    pub fn publish_audio(&self, audio: Option<RemoteAudioAsset>) -> Option<RemoteAudioDescriptor> {
        let Some(audio) = audio else {
            self.audio.send_replace(None);
            return None;
        };
        if !valid_audio_asset(&audio) {
            self.audio.send_replace(None);
            return None;
        }
        let descriptor = RemoteAudioDescriptor {
            id: audio.id.clone(),
            label: audio.label.clone(),
        };
        if self.audio.borrow().as_ref() == Some(&audio) {
            return Some(descriptor);
        }
        self.audio.send_replace(Some(audio));
        Some(descriptor)
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
    audio: watch::Receiver<Option<RemoteAudioAsset>>,
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
                let audio = audio.clone();
                let inputs = inputs.clone();
                let revoked = shutdown.clone();
                tokio::spawn(async move {
                    let _permit = permit;
                    let _ = handle_client(stream, token, frame, audio, inputs, revoked).await;
                });
            }
        }
    }
}

async fn handle_client(
    mut stream: TcpStream,
    token: String,
    mut frame: watch::Receiver<Option<RemoteFrame>>,
    mut audio: watch::Receiver<Option<RemoteAudioAsset>>,
    inputs: mpsc::Sender<RemoteInput>,
    mut revoked: watch::Receiver<bool>,
) -> DevelopmentResult<()> {
    let request_bytes = match timeout(HANDSHAKE_TIMEOUT, read_http_header(&mut stream)).await {
        Ok(Ok(request)) => request,
        Ok(Err(_)) => {
            tokio::io::AsyncWriteExt::write_all(
                &mut stream,
                b"HTTP/1.1 431 Request Header Fields Too Large\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
            ).await?;
            return Ok(());
        }
        Err(_) => {
            return Err(DevelopmentError::Process(
                "remote view handshake timed out".into(),
            ));
        }
    };
    let request_line = request_bytes
        .split(|byte| *byte == b'\n')
        .next()
        .unwrap_or_default();
    let request = std::str::from_utf8(request_line).unwrap_or("").trim_end();
    let expected_page = format!("GET /{token}/ ");
    if request.starts_with(&expected_page) {
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
    if let Some(id) = audio_asset_request_id(request, &token) {
        let asset = audio.borrow().clone();
        if let Some(asset) = asset.filter(|asset| asset.id == id) {
            let header = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: audio/wav\r\nContent-Length: {}\r\nCache-Control: no-store\r\nX-Content-Type-Options: nosniff\r\nContent-Disposition: inline; filename=audio.wav\r\nConnection: close\r\n\r\n",
                asset.bytes.len()
            );
            tokio::io::AsyncWriteExt::write_all(&mut stream, header.as_bytes()).await?;
            tokio::io::AsyncWriteExt::write_all(&mut stream, &asset.bytes).await?;
        } else {
            tokio::io::AsyncWriteExt::write_all(
                &mut stream,
                b"HTTP/1.1 404 Not Found\r\nCache-Control: no-store\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
            )
            .await?;
        }
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
    // Replay the consumed header (and any read-ahead bytes) to tungstenite,
    // retaining the original write half for its handshake response and frames.
    use tokio::io::AsyncReadExt;
    let (read, write) = stream.into_split();
    let transport = tokio::io::join(std::io::Cursor::new(request_bytes).chain(read), write);
    let mut socket = timeout(HANDSHAKE_TIMEOUT, accept_async(transport))
        .await
        .map_err(|_| DevelopmentError::Process("remote view handshake timed out".into()))?
        .map_err(|error| {
            DevelopmentError::Process(format!("remote view handshake failed: {error}"))
        })?;

    let current_frame = frame.borrow().clone();
    let current_audio = audio.borrow().clone();
    if let Some(current) = current_frame {
        send_frame_message(&mut socket, current, current_audio.as_ref()).await?;
    }

    loop {
        tokio::select! {
            changed = revoked.changed() => if changed.is_err() || *revoked.borrow() { break },
            changed = frame.changed() => {
                if changed.is_err() { break; }
                let latest = { frame.borrow().clone() };
                if let Some(value) = latest {
                    let current_audio = audio.borrow().clone();
                    send_frame_message(&mut socket, value, current_audio.as_ref()).await?;
                }
            }
            changed = audio.changed() => {
                if changed.is_err() { break; }
                let latest = { frame.borrow().clone() };
                if let Some(value) = latest {
                    let current_audio = audio.borrow().clone();
                    send_frame_message(&mut socket, value, current_audio.as_ref()).await?;
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

async fn send_frame_message<S>(
    socket: &mut tokio_tungstenite::WebSocketStream<S>,
    frame: RemoteFrame,
    audio: Option<&RemoteAudioAsset>,
) -> DevelopmentResult<()>
where
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin,
{
    let message = RemoteFrameMessage {
        browser_revision: frame.browser_revision,
        mime_type: frame.mime_type,
        data: frame.data,
        audio: audio.map(|asset| RemoteAudioDescriptor {
            id: asset.id.clone(),
            label: asset.label.clone(),
        }),
    };
    let text = serde_json::to_string(&message)?;
    socket
        .send(tungstenite::Message::Text(text.into()))
        .await
        .map_err(|error| DevelopmentError::Process(error.to_string()))
}

fn valid_audio_asset(asset: &RemoteAudioAsset) -> bool {
    let bytes = &asset.bytes;
    let Some(riff_size) = bytes.get(4..8).and_then(|size| size.try_into().ok()) else {
        return false;
    };
    let riff_size = u32::from_le_bytes(riff_size) as usize;
    bytes.len() >= 12
        && bytes.len() <= MAX_AUDIO_ASSET_BYTES
        && bytes.get(..4) == Some(b"RIFF")
        && bytes.get(8..12) == Some(b"WAVE")
        && riff_size.checked_add(8) == Some(bytes.len())
        && valid_audio_id(&asset.id)
        && !asset.label.is_empty()
        && asset.label.len() <= MAX_AUDIO_LABEL_BYTES
        && asset.label.chars().all(|character| {
            character.is_alphanumeric()
                || character.is_whitespace()
                || matches!(character, '·' | '-' | '_')
        })
}

fn valid_audio_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= MAX_AUDIO_ID_BYTES
        && id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
}

fn audio_asset_request_id<'a>(request: &'a str, token: &str) -> Option<&'a str> {
    let prefix = format!("GET /{token}/media/");
    let path = request.strip_prefix(&prefix)?;
    let path = path
        .strip_suffix(" HTTP/1.1")
        .or_else(|| path.strip_suffix(" HTTP/1.0"))?;
    let id = path.strip_suffix(".wav")?;
    valid_audio_id(id).then_some(id)
}

async fn read_http_header<R>(reader: &mut R) -> DevelopmentResult<Vec<u8>>
where
    R: tokio::io::AsyncRead + Unpin,
{
    use tokio::io::AsyncReadExt;

    let mut header = Vec::with_capacity(1024);
    let mut chunk = [0_u8; 512];
    loop {
        let read = reader.read(&mut chunk).await?;
        if read == 0 {
            return Err(DevelopmentError::Process(
                "remote view request ended before headers completed".into(),
            ));
        }
        header.extend_from_slice(&chunk[..read]);
        if let Some(end) = header.windows(4).position(|window| window == b"\r\n\r\n") {
            if end + 4 > MAX_HTTP_HEADER_BYTES {
                return Err(DevelopmentError::Process(
                    "remote view request headers exceed the bounded size".into(),
                ));
            }
            return Ok(header);
        }
        if header.len() >= MAX_HTTP_HEADER_BYTES {
            return Err(DevelopmentError::Process(
                "remote view request headers exceed the bounded size".into(),
            ));
        }
    }
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
        r#"<!doctype html><meta name=viewport content="width=device-width,initial-scale=1,viewport-fit=cover"><title>Glass Remote View</title><style>html,body{{margin:0;background:#101418;color:#eef;font:14px system-ui}}header{{padding:10px}}img{{display:block;width:100%;height:auto;touch-action:none}}input{{box-sizing:border-box;width:100%;padding:12px;background:#172231;color:#eef;border:1px solid #39485d}}#a{{padding:10px}}audio{{display:block;width:100%}}</style><header>Glass Remote View · loopback session</header><img id=v tabindex=0 alt="Browser frame"><section id=a hidden><label for=m>Decoded native browser audio · <span id=l></span></label><audio id=m controls preload=none></audio></section><input id=t autocomplete=off placeholder="Type into focused browser control, then press Enter"><script>const v=document.querySelector('#v'),t=document.querySelector('#t'),a=document.querySelector('#a'),m=document.querySelector('#m'),l=document.querySelector('#l');let r=0,audioId=null;const send=o=>{{if(w.readyState===1)w.send(JSON.stringify({{...o,expectedRevision:r}}))}},w=new WebSocket(`ws://${{location.host}}/ws/{token}`);w.onmessage=e=>{{const f=JSON.parse(e.data);r=f.browserRevision;v.src=`data:${{f.mimeType}};base64,${{f.data}}`;const d=f.audio;if(!d){{if(audioId!==null){{m.pause();m.removeAttribute('src');m.load();audioId=null}}a.hidden=true;return}}l.textContent=d.label;if(audioId!==d.id){{m.pause();m.src=`/${{encodeURIComponent("{token}")}}/media/${{encodeURIComponent(d.id)}}.wav`;m.load();audioId=d.id}}a.hidden=false}};v.onclick=e=>{{const b=v.getBoundingClientRect();send({{type:'click',x:(e.clientX-b.left)/b.width,y:(e.clientY-b.top)/b.height}});v.focus()}};v.onwheel=e=>{{e.preventDefault();send({{type:'scroll',dx:e.deltaX,dy:e.deltaY}})}};window.onkeydown=e=>{{if(e.target!==t&&e.target!==m){{e.preventDefault();send({{type:'key',key:e.key}})}}}};t.onkeydown=e=>{{if(e.key==='Enter'&&t.value){{send({{type:'text',text:t.value}});t.value='';e.preventDefault()}}}};</script>"#
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn fragmented_viewer_and_websocket_requests_keep_the_complete_header() {
        use tokio::io::AsyncWriteExt;

        let view = RemoteView::bind().await.unwrap();
        let local_url = view.local_url();
        let token = local_url
            .trim_end_matches('/')
            .rsplit('/')
            .next()
            .unwrap()
            .to_string();
        for websocket in [false, true] {
            let mut stream = TcpStream::connect(("127.0.0.1", view.port()))
                .await
                .unwrap();
            stream.write_all(b"GET /").await.unwrap();
            // Separate the prefix from the remainder so a single TCP peek
            // cannot classify the request before the token has arrived.
            tokio::time::sleep(Duration::from_millis(20)).await;
            let path = if websocket {
                format!("ws/{token}")
            } else {
                format!("{token}/")
            };
            let upgrade = if websocket {
                "Upgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Version: 13\r\nSec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\n"
            } else {
                ""
            };
            stream
                .write_all(
                    format!(
                        "{path} HTTP/1.1\r\nHost: localhost\r\n{upgrade}X-Filler: {}\r\n\r\n",
                        "x".repeat(5 * 1024)
                    )
                    .as_bytes(),
                )
                .await
                .unwrap();
            let response = timeout(Duration::from_secs(2), read_http_header(&mut stream))
                .await
                .unwrap()
                .unwrap();
            let expected = if websocket {
                "HTTP/1.1 101"
            } else {
                "HTTP/1.1 200"
            };
            assert!(String::from_utf8_lossy(&response).starts_with(expected));
        }
        view.revoke().await;
    }

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
    async fn wav_artifact_is_token_scoped_current_only_and_user_controlled() {
        let view = RemoteView::bind().await.unwrap();
        let local_url = view.local_url();
        let token = local_url
            .trim_end_matches('/')
            .rsplit('/')
            .next()
            .unwrap()
            .to_string();
        let first = test_wav();
        let descriptor = view
            .publish_audio(Some(RemoteAudioAsset {
                id: "decoded-audio-1".into(),
                label: "Decoded local fixture · 250 ms".into(),
                bytes: first.clone(),
            }))
            .expect("valid WAV artifact should be published");
        assert_eq!(descriptor.id, "decoded-audio-1");
        assert_eq!(descriptor.label, "Decoded local fixture · 250 ms");

        let page = reqwest::get(view.local_url()).await.unwrap();
        let html = page.text().await.unwrap();
        assert!(html.contains("<audio id=m controls preload=none></audio>"));
        assert!(html.contains("l.textContent=d.label"));
        assert!(!html.contains("autoplay"));

        let url = format!(
            "http://127.0.0.1:{}/{token}/media/{}.wav",
            view.port(),
            descriptor.id
        );
        let response = reqwest::get(&url).await.unwrap();
        assert_eq!(response.status(), reqwest::StatusCode::OK);
        assert_eq!(response.headers()["content-type"], "audio/wav");
        assert_eq!(response.headers()["cache-control"], "no-store");
        assert_eq!(response.bytes().await.unwrap().as_ref(), first.as_slice());

        // The old opaque ID stops working immediately after replacement.
        view.publish_audio(Some(RemoteAudioAsset {
            id: "decoded-audio-2".into(),
            label: "Updated decoded fixture".into(),
            bytes: test_wav(),
        }))
        .expect("replacement WAV artifact should be published");
        assert_eq!(
            reqwest::get(&url).await.unwrap().status(),
            reqwest::StatusCode::NOT_FOUND
        );
        view.publish_audio(None);
        let current_url = format!(
            "http://127.0.0.1:{}/{token}/media/decoded-audio-2.wav",
            view.port()
        );
        assert_eq!(
            reqwest::get(current_url).await.unwrap().status(),
            reqwest::StatusCode::NOT_FOUND
        );
        let wrong_token = format!(
            "http://127.0.0.1:{}/{token}-wrong/media/decoded-audio-1.wav",
            view.port()
        );
        assert_eq!(
            reqwest::get(wrong_token).await.unwrap().status(),
            reqwest::StatusCode::NOT_FOUND
        );
        view.revoke().await;
    }

    #[tokio::test]
    async fn audio_publication_rejects_non_wav_oversize_urls_and_unsafe_ids() {
        let view = RemoteView::bind().await.unwrap();
        for asset in [
            RemoteAudioAsset {
                id: "not-wav".into(),
                label: "fixture".into(),
                bytes: b"OggSnot-a-wave".to_vec(),
            },
            RemoteAudioAsset {
                id: "too-large".into(),
                label: "fixture".into(),
                bytes: vec![0; MAX_AUDIO_ASSET_BYTES + 1],
            },
            RemoteAudioAsset {
                id: "../private".into(),
                label: "fixture".into(),
                bytes: test_wav(),
            },
            RemoteAudioAsset {
                id: "url-label".into(),
                label: "https://private.example/audio.wav".into(),
                bytes: test_wav(),
            },
        ] {
            assert!(view.publish_audio(Some(asset)).is_none());
        }
        view.publish_audio(Some(RemoteAudioAsset {
            id: "valid-before-invalid".into(),
            label: "Decoder output · 1 second".into(),
            bytes: test_wav(),
        }))
        .expect("valid artifact should be accepted");
        assert!(
            view.publish_audio(Some(RemoteAudioAsset {
                id: "invalid-replacement".into(),
                label: "source.example/file.wav".into(),
                bytes: test_wav(),
            }))
            .is_none()
        );
        let local_url = view.local_url();
        let token = local_url.trim_end_matches('/').rsplit('/').next().unwrap();
        let url = format!(
            "http://127.0.0.1:{}/{token}/media/valid-before-invalid.wav",
            view.port()
        );
        assert_eq!(
            reqwest::get(url).await.unwrap().status(),
            reqwest::StatusCode::NOT_FOUND
        );
        view.revoke().await;
    }

    #[tokio::test]
    async fn websocket_delivers_latest_frame_and_revision_bound_input() {
        let mut view = RemoteView::bind().await.unwrap();
        let local_url = view.local_url();
        let token = local_url
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

    #[tokio::test]
    async fn websocket_frame_includes_safe_audio_descriptor_and_clear_state() {
        let view = RemoteView::bind().await.unwrap();
        let descriptor = view
            .publish_audio(Some(RemoteAudioAsset {
                id: "clip-42".into(),
                label: "Decoded audio sample".into(),
                bytes: test_wav(),
            }))
            .unwrap();
        assert_eq!(descriptor.id, "clip-42");
        view.publish(RemoteFrame {
            browser_revision: 4,
            mime_type: "image/png".into(),
            data: "AA==".into(),
        });
        let local_url = view.local_url();
        let token = local_url
            .trim_end_matches('/')
            .rsplit('/')
            .next()
            .unwrap()
            .to_string();
        let url = format!("ws://127.0.0.1:{}/ws/{token}", view.port());
        let (mut socket, _) = tokio_tungstenite::connect_async(url).await.unwrap();
        let first = timeout(Duration::from_secs(2), socket.next())
            .await
            .unwrap()
            .unwrap()
            .unwrap()
            .into_text()
            .unwrap();
        let first: serde_json::Value = serde_json::from_str(&first).unwrap();
        assert_eq!(first["audio"]["id"], "clip-42");
        assert_eq!(first["audio"]["label"], "Decoded audio sample");
        assert!(first["audio"].get("url").is_none());

        view.publish_audio(None);
        timeout(Duration::from_secs(2), async {
            loop {
                let message = socket.next().await.unwrap().unwrap().into_text().unwrap();
                let message: serde_json::Value = serde_json::from_str(&message).unwrap();
                if message["audio"].is_null() {
                    break;
                }
            }
        })
        .await
        .expect("viewer receives an explicit audio clear update");
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

    fn test_wav() -> Vec<u8> {
        let mut bytes = Vec::from(&b"RIFF\0\0\0\0WAVEfmt "[..]);
        bytes.extend_from_slice(&16_u32.to_le_bytes());
        bytes.extend_from_slice(&1_u16.to_le_bytes()); // PCM
        bytes.extend_from_slice(&1_u16.to_le_bytes()); // mono
        bytes.extend_from_slice(&8_000_u32.to_le_bytes());
        bytes.extend_from_slice(&8_000_u32.to_le_bytes());
        bytes.extend_from_slice(&1_u16.to_le_bytes());
        bytes.extend_from_slice(&8_u16.to_le_bytes());
        bytes.extend_from_slice(b"data");
        bytes.extend_from_slice(&2_u32.to_le_bytes());
        bytes.extend_from_slice(&[128, 192]);
        let riff_size = u32::try_from(bytes.len() - 8).unwrap();
        bytes[4..8].copy_from_slice(&riff_size.to_le_bytes());
        bytes
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

    #[tokio::test]
    async fn viewer_request_headers_larger_than_peek_buffer_are_consumed_completely() {
        let (mut writer, mut reader) = tokio::io::duplex(MAX_HTTP_HEADER_BYTES + 1024);
        let request = format!(
            "GET /token/ HTTP/1.1\r\nX-Filler: {}\r\n\r\n",
            "x".repeat(5 * 1024)
        );
        let expected = request.as_bytes().to_vec();
        tokio::spawn(async move {
            use tokio::io::AsyncWriteExt;
            writer.write_all(&expected).await.unwrap();
        });

        let header = read_http_header(&mut reader).await.unwrap();
        assert!(header.len() > 4096);
        assert!(header.ends_with(b"\r\n\r\n"));
        assert!(String::from_utf8(header).unwrap().contains("X-Filler:"));
    }

    #[tokio::test]
    async fn viewer_rejects_terminated_headers_over_the_size_limit() {
        let (mut writer, mut reader) = tokio::io::duplex(MAX_HTTP_HEADER_BYTES + 1024);
        let request = format!(
            "GET /token/ HTTP/1.1\r\nX-Filler: {}\r\n\r\n",
            "x".repeat(MAX_HTTP_HEADER_BYTES)
        );
        tokio::spawn(async move {
            use tokio::io::AsyncWriteExt;
            writer.write_all(request.as_bytes()).await.unwrap();
        });

        let error = read_http_header(&mut reader).await.unwrap_err();
        assert!(
            error
                .to_string()
                .contains("headers exceed the bounded size")
        );
    }
}
