//! Scoped download lifecycle management.
//!
//! Waits for and captures a single file download initiated by the browser,
//! returning the downloaded bytes. Only one download may be authorized at a
//! time per session.

use super::*;

impl BrowserSession {
    /// Wait for one explicitly authorized download lifecycle.
    pub async fn wait_for_download(
        &self,
        destination: &Path,
        deadline: Duration,
    ) -> BrowserResult<DownloadOutcome> {
        self.policy.require(PolicyCapability::Download)?;
        if deadline.is_zero() || deadline > MAX_DIAGNOSTIC_DURATION {
            return Err("download deadline must be between 1 ms and 30 seconds".into());
        }
        let destination = self.policy.require_existing_path(destination)?;
        if !destination.is_dir() || !destination.starts_with(&self.upload_root) {
            return Err(
                "download destination must be a directory inside the authorized root".into(),
            );
        }
        let (target_id, frame_id) = self.route_identity().await?;
        let page_session_id = if use_page_download_compatibility(
            self.chrome.is_some(),
            self.disposable_profile.is_some(),
        ) {
            let topology = self.topology.lock().await;
            if topology.active_target_id.as_deref() != Some(target_id.as_str()) {
                return Err(download_error(
                    DownloadErrorKind::AuthorizationFailed,
                    "incognito download route changed during capture",
                )
                .into());
            }
            Some(topology.active_target_session_id.clone().ok_or_else(|| {
                download_error(
                    DownloadErrorKind::AuthorizationFailed,
                    "incognito download has no captured top-level page session",
                )
            })?)
        } else {
            None
        };
        let _download_scope = self.download_scope.lock().await;
        let mut events = self.cdp.subscribe_events_with_params();
        let mut download_guard = match page_session_id {
            Some(page_session_id) => {
                DownloadBehaviorGuard::acquire_for_incognito(
                    self.cdp.clone(),
                    destination.clone(),
                    target_id.clone(),
                    page_session_id,
                    self.launched_incognito_context_id.clone().ok_or_else(|| {
                        download_error(
                            DownloadErrorKind::AuthorizationFailed,
                            "owned incognito session has no original browser context ID",
                        )
                    })?,
                )
                .await?
            }
            None => {
                DownloadBehaviorGuard::acquire(self.cdp.clone(), destination.clone(), None).await?
            }
        };
        let result = tokio::time::timeout(deadline, async {
            let mut guid = None;
            let mut filename = String::new();
            loop {
                match events.recv().await {
                    Ok(event) if event.method == "Browser.downloadWillBegin" => {
                        if event.params["frameId"].as_str() != Some(frame_id.as_str()) {
                            continue;
                        }
                        guid = event.params["guid"].as_str().map(bounded_diagnostic_text);
                        filename = bounded_diagnostic_text(
                            event.params["suggestedFilename"]
                                .as_str()
                                .unwrap_or("download"),
                        );
                    }
                    Ok(event) if event.method == "Browser.downloadProgress" => {
                        let Some(active_guid) = guid.as_deref() else {
                            continue;
                        };
                        if event.params["guid"].as_str() != Some(active_guid) {
                            continue;
                        }
                        let state = event.params["state"].as_str().unwrap_or("inProgress");
                        if matches!(state, "completed" | "canceled") {
                            self.download_sequence.fetch_add(1, Ordering::Relaxed);
                            self.record_audit(
                                "download",
                                format!("{} (state={})", filename, state),
                            );
                            return BrowserResult::Ok(DownloadOutcome {
                                guid: active_guid.to_string(),
                                suggested_filename: filename,
                                state: state.to_ascii_lowercase(),
                                received_bytes: finite_nonnegative_u64(
                                    &event.params["receivedBytes"],
                                ),
                                total_bytes: finite_nonnegative_u64(&event.params["totalBytes"]),
                                target_id: target_id.clone(),
                                frame_id: frame_id.clone(),
                                sha256: None,
                            });
                        }
                    }
                    Ok(_) => {}
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(count)) => {
                        return Err(format!("download event stream dropped {count} events").into());
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                        return Err("download event stream closed".into());
                    }
                }
            }
        })
        .await
        .unwrap_or_else(|_| Err("download deadline exceeded".into()));
        download_guard.disable().await?;
        result
    }
}
