use super::*;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;

#[derive(Debug, Clone, serde::Serialize)]
pub struct NetworkEntry {
    pub request_id: String,
    pub method: String,
    pub url: String,
    pub status: Option<u16>,
    pub failure: Option<String>,
    pub redirect_count: u16,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct NetworkRecording {
    pub entries: Vec<NetworkEntry>,
    pub duration_ms: u64,
    pub dropped_events: u64,
}

pub struct NetworkRecorder {
    cdp: CdpClient,
    session_id: String,
    events: tokio::sync::broadcast::Receiver<crate::browser::cdp::CdpEventWithParams>,
    entries: Arc<Mutex<Vec<NetworkEntry>>>,
    request_indexes: Arc<Mutex<HashMap<String, usize>>>,
    dropped: Arc<Mutex<u64>>,
    armed: bool,
}

impl NetworkRecorder {
    pub(crate) async fn start(cdp: CdpClient) -> BrowserResult<Self> {
        let session_id = cdp
            .current_session_id()
            .ok_or("network recorder requires an active page session")?;
        let events = cdp.subscribe_events_with_params();
        cdp.send_to_session(&session_id, "Network.enable", None)
            .await?;
        Ok(Self {
            cdp,
            session_id,
            events,
            entries: Arc::new(Mutex::new(Vec::new())),
            request_indexes: Arc::new(Mutex::new(HashMap::new())),
            dropped: Arc::new(Mutex::new(0)),
            armed: true,
        })
    }

    pub async fn stop(mut self) -> BrowserResult<NetworkRecording> {
        self.armed = false;
        disable_fetch_for(&self.cdp, Some(&self.session_id)).await?;
        let entries = std::mem::take(&mut *self.entries.lock().await);
        Ok(NetworkRecording {
            entries,
            duration_ms: 0,
            dropped_events: *self.dropped.lock().await,
        })
    }

    pub async fn drain(&mut self) {
        let mut entries = self.entries.lock().await;
        let mut indexes = self.request_indexes.lock().await;
        let mut dropped = self.dropped.lock().await;
        while let Ok(event) = self.events.try_recv() {
            if event.method == "Network.requestWillBeSent" {
                let Some(rid) = event.params["requestId"].as_str() else {
                    continue;
                };
                if let Some(idx) = indexes.get(rid).copied() {
                    entries[idx].redirect_count = entries[idx].redirect_count.saturating_add(1);
                    continue;
                }
                if entries.len() >= 128 {
                    *dropped = dropped.saturating_add(1);
                    continue;
                }
                let req = &event.params["request"];
                let idx = entries.len();
                indexes.insert(rid.to_string(), idx);
                entries.push(NetworkEntry {
                    request_id: rid.to_string(),
                    method: req["method"].as_str().unwrap_or("").to_string(),
                    url: req["url"].as_str().unwrap_or("").to_string(),
                    status: None,
                    failure: None,
                    redirect_count: 0,
                });
            } else if event.method == "Network.responseReceived" {
                if let Some(idx) = event.params["requestId"]
                    .as_str()
                    .and_then(|id| indexes.get(id))
                    .copied()
                {
                    entries[idx].status = event.params["response"]["status"]
                        .as_u64()
                        .and_then(|s| u16::try_from(s).ok());
                }
            } else if event.method == "Network.loadingFailed"
                && let Some(idx) = event.params["requestId"]
                    .as_str()
                    .and_then(|id| indexes.get(id))
                    .copied()
            {
                entries[idx].failure = Some(
                    event.params["errorText"]
                        .as_str()
                        .unwrap_or("unknown")
                        .to_string(),
                );
            }
        }
    }
}

impl Drop for NetworkRecorder {
    fn drop(&mut self) {
        if self.armed {
            let cdp = self.cdp.clone();
            let sid = self.session_id.clone();
            tokio::spawn(async move {
                let _ = disable_fetch_for(&cdp, Some(&sid)).await;
            });
        }
    }
}

impl BrowserSession {
    pub async fn start_network_recording(&self) -> BrowserResult<NetworkRecorder> {
        self.cdp
            .with_current_route(async { NetworkRecorder::start(self.cdp.clone()).await })
            .await
    }
}
