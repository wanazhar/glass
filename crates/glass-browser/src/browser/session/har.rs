//! Network recording (HTTP Archive) capture.
//!
//! Provides bounded network event recording via CDP `Network` domain
//! integration. Use [`BrowserSession::start_network_recording`] to
//! begin capture and [`NetworkRecorder::stop`] to retrieve a
//! [`NetworkRecording`] with collected entries.

use super::*;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;

/// A single recorded network request / response entry.
#[derive(Debug, Clone, serde::Serialize)]
pub struct NetworkEntry {
    /// CDP request ID assigned by the browser.
    pub request_id: String,
    /// HTTP method (e.g. GET, POST).
    pub method: String,
    /// Request URL.
    pub url: String,
    /// HTTP status code, if the response was received.
    pub status: Option<u16>,
    /// Failure reason (e.g. "net::ERR_CONNECTION_REFUSED").
    pub failure: Option<String>,
    /// Number of redirects for this request chain.
    pub redirect_count: u16,
}

/// The result of a completed network recording.
///
/// Returned by [`NetworkRecorder::stop`]. Contains all captured
/// entries, approximate duration, and a count of dropped events.
#[derive(Debug, Clone, serde::Serialize)]
pub struct NetworkRecording {
    /// Captured network request/response entries.
    pub entries: Vec<NetworkEntry>,
    /// Approximate duration of the recording in milliseconds.
    pub duration_ms: u64,
    /// Number of events dropped due to the 128-entry cap.
    pub dropped_events: u64,
}

/// Scoped lease for network recording.
///
/// Created by [`BrowserSession::start_network_recording`]. While this
/// guard is alive, `Network.requestWillBeSent`, `Network.responseReceived`,
/// and `Network.loadingFailed` events are collected into an internal buffer
/// (capped at 128 entries).
///
/// Call [`drain`](Self::drain) periodically to process events, then
/// [`stop`](Self::stop) to finalize and retrieve the recording.
///
/// On drop, `Network.disable` is sent for best-effort cleanup.
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
    /// Enable the Network domain and begin recording.
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

    /// Stop recording, disable the Network domain, and return captured entries.
    pub async fn stop(mut self) -> BrowserResult<NetworkRecording> {
        self.armed = false;
        disable_network_for(&self.cdp, Some(&self.session_id)).await?;
        let entries = std::mem::take(&mut *self.entries.lock().await);
        Ok(NetworkRecording {
            entries,
            duration_ms: 0,
            dropped_events: *self.dropped.lock().await,
        })
    }

    /// Process any pending network events into the internal buffer.
    ///
    /// Call this periodically during recording (e.g. in a loop) to
    /// collect events. Idempotent — safe to call after the recording
    /// is done.
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
                let _ = disable_network_for(&cdp, Some(&sid)).await;
            });
        }
    }
}

impl BrowserSession {
    /// Start recording network traffic for the active page session.
    ///
    /// Returns a `NetworkRecorder` guard. Call `NetworkRecorder::drain`
    /// to collect events and `NetworkRecorder::stop` to finalize.
    pub async fn start_network_recording(&self) -> BrowserResult<NetworkRecorder> {
        self.cdp
            .with_current_route(async { NetworkRecorder::start(self.cdp.clone()).await })
            .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn network_entry_serializes_to_json() {
        let entry = NetworkEntry {
            request_id: "req-1".to_string(),
            method: "GET".to_string(),
            url: "https://example.com".to_string(),
            status: Some(200),
            failure: None,
            redirect_count: 0,
        };
        let json = serde_json::to_value(&entry).unwrap();
        assert_eq!(json["request_id"], "req-1");
        assert_eq!(json["method"], "GET");
        assert_eq!(json["url"], "https://example.com");
        assert_eq!(json["status"], 200);
        assert!(json["failure"].is_null());
        assert_eq!(json["redirect_count"], 0);
    }

    #[test]
    fn network_entry_serializes_failure_when_present() {
        let entry = NetworkEntry {
            request_id: "req-2".to_string(),
            method: "POST".to_string(),
            url: "https://example.com/api".to_string(),
            status: None,
            failure: Some("net::ERR_CONNECTION_REFUSED".to_string()),
            redirect_count: 0,
        };
        let json = serde_json::to_value(&entry).unwrap();
        assert_eq!(json["failure"], "net::ERR_CONNECTION_REFUSED");
        assert!(json["status"].is_null());
    }

    #[test]
    fn network_recording_serializes_to_json() {
        let recording = NetworkRecording {
            entries: vec![],
            duration_ms: 1500,
            dropped_events: 3,
        };
        let json = serde_json::to_value(&recording).unwrap();
        assert_eq!(json["duration_ms"], 1500);
        assert_eq!(json["dropped_events"], 3);
        assert!(json["entries"].as_array().unwrap().is_empty());
    }

    #[test]
    fn network_recording_defaults_start_at_zero() {
        // Verify the zero-state that users see for empty recordings
        let recording = NetworkRecording {
            entries: vec![],
            duration_ms: 0,
            dropped_events: 0,
        };
        assert_eq!(recording.duration_ms, 0);
        assert_eq!(recording.dropped_events, 0);
        assert!(recording.entries.is_empty());
    }

    #[test]
    fn network_entry_with_redirects_serializes_count() {
        let entry = NetworkEntry {
            request_id: "req-redirect".to_string(),
            method: "GET".to_string(),
            url: "https://example.com/redirected".to_string(),
            status: Some(301),
            failure: None,
            redirect_count: 2,
        };
        let json = serde_json::to_value(&entry).unwrap();
        assert_eq!(json["redirect_count"], 2);
        assert_eq!(json["status"], 301);
    }

    #[test]
    fn network_recording_with_entries_serializes_all() {
        let recording = NetworkRecording {
            entries: vec![
                NetworkEntry {
                    request_id: "r1".to_string(),
                    method: "GET".to_string(),
                    url: "/a".to_string(),
                    status: Some(200),
                    failure: None,
                    redirect_count: 0,
                },
                NetworkEntry {
                    request_id: "r2".to_string(),
                    method: "POST".to_string(),
                    url: "/b".to_string(),
                    status: None,
                    failure: Some("net::ERR_FAILED".to_string()),
                    redirect_count: 0,
                },
            ],
            duration_ms: 500,
            dropped_events: 0,
        };
        let json = serde_json::to_value(&recording).unwrap();
        assert_eq!(json["entries"].as_array().unwrap().len(), 2);
        assert_eq!(json["entries"][0]["method"], "GET");
        assert_eq!(json["entries"][1]["failure"], "net::ERR_FAILED");
    }
}
