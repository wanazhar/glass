//! Page target discovery, selection, and creation.
//!
//! Lists open page targets, creates new ones, and provides explicit
//! target selection for subsequent operations.

use super::*;

impl BrowserSession {
    /// List all open page targets in the browser.
    ///
    /// Returns the target ID, URL, title, opener relationship, and whether
    /// each target is currently active. Capped at 32 targets.
    pub async fn list_targets(&self) -> BrowserResult<Vec<PageTargetInfo>> {
        let raw = self.cdp.send_browser("Target.getTargets", None).await?;
        let active = self.topology.lock().await.active_target_id.clone();
        let mut targets = Vec::new();
        for info in raw["targetInfos"].as_array().into_iter().flatten() {
            if info["type"].as_str() != Some("page") {
                continue;
            }
            let Some(id) = info["targetId"].as_str() else {
                continue;
            };
            validate_topology_id(id)?;
            if targets.len() == TOPOLOGY_MAX_TARGETS {
                return Err("page target limit exceeded".into());
            }
            targets.push(PageTargetInfo {
                id: id.to_string(),
                url: bounded_topology_text(info["url"].as_str().unwrap_or_default()),
                title: bounded_topology_text(info["title"].as_str().unwrap_or_default()),
                opener_id: retained_optional_topology_id(info["openerId"].as_str())?,
                active: active.as_deref() == Some(id),
            });
        }
        self.topology.lock().await.targets = targets.clone();
        Ok(targets)
    }

    /// Return recent topology change events for diagnostic purposes.
    ///
    /// Each event includes a sequence number, kind (e.g. `targetCreated`),
    /// and the affected target/frame ID. Bounded to 64 recent events.
    pub async fn topology_events(&self) -> Vec<TopologyEventSummary> {
        self.topology.lock().await.events.iter().cloned().collect()
    }

    pub(crate) async fn route_identity(&self) -> BrowserResult<(String, String)> {
        if let Some(identity) = self.cdp.operation_identity() {
            return Ok(identity);
        }

        let topology = self.topology.lock().await;
        let (kind, message) = match (
            topology.active_target_id.as_ref(),
            topology.active_session_id.as_ref(),
            topology.active_frame_id.as_ref(),
        ) {
            (None, _, _) => (
                TopologyErrorKind::NoTargetSelected,
                "no active target is selected; call listTargets to discover available pages",
            ),
            (Some(_), None, _) => (
                TopologyErrorKind::NoPageSession,
                "active target has no CDP session; the session may need to be re-established",
            ),
            (Some(_), Some(_), None) => (
                TopologyErrorKind::StaleFrame,
                "active target has no selected frame; call listFrames and selectFrame",
            ),
            (Some(_), Some(_), Some(_)) => (
                TopologyErrorKind::RoutingLost,
                "active target/frame routing is unavailable; re-synchronize the session",
            ),
        };
        Err(TopologyError::new(kind, message).into())
    }

    pub(crate) async fn ensured_route_identity(&self) -> BrowserResult<(String, String)> {
        if let Ok(route) = self.route_identity().await {
            return Ok(route);
        }
        let main_frame = self
            .list_frames()
            .await?
            .into_iter()
            .find(|frame| frame.parent_id.is_none())
            .ok_or("active target returned no main frame")?;
        self.select_frame(&main_frame.id).await?;
        self.route_identity().await
    }

    /// Open a new page target navigating to the given URL.
    ///
    /// The URL is normalized and validated against the active policy.
    /// The new target is discoverable via [`list_targets`](Self::list_targets)
    /// but does not become the active target automatically.
    pub async fn create_target(&self, url: &str) -> BrowserResult<PageTargetInfo> {
        let url = normalize_url(url);
        self.policy.require_url(&url).await?;
        let result = self
            .cdp
            .send_browser("Target.createTarget", Some(serde_json::json!({"url": url})))
            .await?;
        let id = result["targetId"]
            .as_str()
            .ok_or("Target.createTarget returned no targetId")?;
        validate_topology_id(id)?;
        let targets = self.list_targets().await?;
        let target = targets
            .into_iter()
            .find(|target| target.id == id)
            .ok_or_else(|| -> Box<dyn Error> { "created target was not discoverable".into() })?;
        self.record_audit("attach", &url);
        Ok(target)
    }

    /// Select a page target as the active context.
    ///
    /// Attaches to the target via CDP, selects its main frame, and detaches
    /// from the previously active target. Invalidates the observation cache.
    pub async fn select_target(&self, target_id: &str) -> BrowserResult<PageTargetInfo> {
        validate_topology_id(target_id)?;
        let target = self
            .list_targets()
            .await?
            .into_iter()
            .find(|target| target.id == target_id)
            .ok_or("page target was not found")?;
        let attached = self
            .cdp
            .send_browser(
                "Target.attachToTarget",
                Some(serde_json::json!({"targetId": target_id, "flatten": true})),
            )
            .await?;
        let new_session = attached["sessionId"]
            .as_str()
            .ok_or("Target.attachToTarget returned no sessionId")?
            .to_string();
        let old_session = self.topology.lock().await.active_session_id.clone();
        if let Err(error) = self.cdp.enable_observation_events_for(&new_session).await {
            let _ = self
                .cdp
                .send_browser(
                    "Target.detachFromTarget",
                    Some(serde_json::json!({"sessionId": new_session})),
                )
                .await;
            return Err(error.into());
        }
        if let Some(interception) = &self.policy_interception {
            if let Err(error) = enable_fetch_for(&self.cdp, &new_session).await {
                let _ = self
                    .cdp
                    .send_browser(
                        "Target.detachFromTarget",
                        Some(serde_json::json!({"sessionId": new_session})),
                    )
                    .await;
                return Err(error);
            }
            interception
                .sessions
                .lock()
                .await
                .insert(new_session.clone());
        }
        if let Err(error) = self
            .cdp
            .send_to_session(
                &new_session,
                "Target.setAutoAttach",
                Some(serde_json::json!({
                    "autoAttach": true,
                    "waitForDebuggerOnStart": matches!(
                        self.policy.preset(),
                        PolicyPreset::Hardened | PolicyPreset::UntrustedMcp
                    ),
                    "flatten": true
                })),
            )
            .await
        {
            let _ = self
                .cdp
                .send_browser(
                    "Target.detachFromTarget",
                    Some(serde_json::json!({"sessionId": new_session})),
                )
                .await;
            return Err(error.into());
        }
        let prepared = async {
            let raw_frames = self
                .cdp
                .send_to_session(&new_session, "Page.getFrameTree", None)
                .await?;
            let mut frames = Vec::new();
            collect_frames(&raw_frames["frameTree"], None, None, &mut frames)?;
            let main_frame = frames
                .iter()
                .find(|frame| frame.parent_id.is_none())
                .ok_or("selected target returned no main frame")?
                .id
                .clone();
            Ok::<_, Box<dyn Error>>((frames, main_frame))
        }
        .await;
        let (mut frames, main_frame) = match prepared {
            Ok(prepared) => prepared,
            Err(error) => {
                let _ = self
                    .cdp
                    .send_browser(
                        "Target.detachFromTarget",
                        Some(serde_json::json!({"sessionId": new_session})),
                    )
                    .await;
                return Err(error);
            }
        };
        for frame in &mut frames {
            frame.active = frame.id == main_frame;
        }
        {
            let mut topology = self.topology.lock().await;
            topology.active_target_id = Some(target_id.to_string());
            topology.active_session_id = Some(new_session.clone());
            topology.active_target_session_id = Some(new_session.clone());
            topology.active_frame_id = Some(main_frame.clone());
            topology.frames = frames;
        }
        self.cdp.set_active_target_route(
            Some(target_id.to_string()),
            Some(new_session.clone()),
            Some(main_frame),
            None,
        );
        if let Some(old_session) = old_session {
            let _ = self
                .cdp
                .send_browser(
                    "Target.detachFromTarget",
                    Some(serde_json::json!({"sessionId": old_session})),
                )
                .await;
        }
        self.invalidate_observation().await;
        Ok(PageTargetInfo {
            active: true,
            ..target
        })
    }

    /// Close a page target by ID.
    ///
    /// If the closed target was the active target, the session's active
    /// target, session, and frame state are cleared.
    pub async fn close_target(&self, target_id: &str) -> BrowserResult<()> {
        validate_topology_id(target_id)?;
        let result = self
            .cdp
            .send_browser(
                "Target.closeTarget",
                Some(serde_json::json!({"targetId": target_id})),
            )
            .await?;
        if result["success"].as_bool() != Some(true) {
            return Err("Chrome refused to close target".into());
        }
        let mut topology = self.topology.lock().await;
        if topology.active_target_id.as_deref() == Some(target_id) {
            topology.active_target_id = None;
            topology.active_session_id = None;
            topology.active_target_session_id = None;
            topology.active_frame_id = None;
            topology.frames.clear();
            topology.frame_sessions.clear();
            topology.frame_parents.clear();
            self.cdp.set_active_target_route(None, None, None, None);
        }
        topology.targets.retain(|target| target.id != target_id);
        Ok(())
    }
}
