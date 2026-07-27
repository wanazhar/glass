//! Frame tree discovery and selection.
//!
//! Lists all frames in the active page target's frame tree and provides
//! explicit frame selection for subsequent operations.

use super::*;

impl BrowserSession {
    /// List all frames in the active target's frame tree.
    ///
    /// Discovers iframe/out-of-process frames and marks the currently active
    /// frame. Call this before `select_frame` to discover available frame IDs.
    pub async fn list_frames(&self) -> BrowserResult<Vec<FrameInfo>> {
        let (target_id, target_session, active) = {
            let topology = self.topology.lock().await;
            let target_id = topology.active_target_id.clone();
            (
                target_id.clone(),
                topology.active_target_session_id.clone(),
                topology.active_frame_id.clone(),
            )
        };
        if target_id.is_none() {
            return Err(TopologyError::new(
                TopologyErrorKind::NoTargetSelected,
                "no active target is selected; call listTargets to discover available pages",
            )
            .into());
        }
        if target_session.is_none() {
            let mut topology = self.topology.lock().await;
            topology.active_target_id = None;
            topology.active_session_id = None;
            topology.active_target_session_id = None;
            topology.active_frame_id = None;
            topology.frames.clear();
            topology.frame_sessions.clear();
            topology.frame_parents.clear();
            self.cdp.set_active_target_route(None, None, None, None);
            return Err(TopologyError::new(
                TopologyErrorKind::NoTargetSelected,
                "no active target is selected; call listTargets to discover available pages",
            )
            .into());
        }
        let target_session = target_session.ok_or_else(|| {
            TopologyError::new(
                TopologyErrorKind::NoPageSession,
                "active target has no CDP session; the session may need to be re-established",
            )
        })?;
        let raw = match self
            .cdp
            .send_to_session(&target_session, "Page.getFrameTree", None)
            .await
        {
            Ok(raw) => raw,
            Err(error) => {
                // A crashed page can leave Target.targetCrashed and the
                // detached-session notification queued behind this request.
                // The page route is unusable regardless of which notification
                // arrives first, so make the recovery state deterministic.
                let mut topology = self.topology.lock().await;
                if topology.active_target_session_id.as_deref() == Some(target_session.as_str()) {
                    topology.active_target_id = None;
                    topology.active_session_id = None;
                    topology.active_target_session_id = None;
                    topology.active_frame_id = None;
                    topology.frames.clear();
                    topology.frame_sessions.clear();
                    topology.frame_parents.clear();
                    self.cdp.set_active_target_route(None, None, None, None);
                    return Err(TopologyError::new(
                        TopologyErrorKind::NoTargetSelected,
                        "no active target is selected; call listTargets to discover available pages",
                    )
                    .into());
                }
                return Err(error.into());
            }
        };
        let mut frames = Vec::new();
        collect_frames(&raw["frameTree"], None, active.as_deref(), &mut frames)?;
        let (attached_sessions, frame_parents) = {
            let topology = self.topology.lock().await;
            (
                topology.frame_sessions.clone(),
                topology.frame_parents.clone(),
            )
        };
        let mut discovered_frame_sessions = Vec::new();
        let mut stale_frame_sessions = HashSet::new();
        let mut queried_sessions = HashSet::new();
        for (attached_frame_id, session_id) in &attached_sessions {
            if !queried_sessions.insert(session_id.clone()) {
                continue;
            }
            let oopif_tree = match self
                .cdp
                .send_to_session(session_id, "Page.getFrameTree", None)
                .await
            {
                Ok(tree) => tree,
                Err(error) if error.code == -32_001 => {
                    stale_frame_sessions.insert(session_id.clone());
                    continue;
                }
                Err(error) => return Err(error.into()),
            };
            let start = frames.len();
            collect_frames(
                &oopif_tree["frameTree"],
                frame_parents.get(attached_frame_id).map(String::as_str),
                active.as_deref(),
                &mut frames,
            )?;
            for frame in &frames[start..] {
                discovered_frame_sessions.push((frame.id.clone(), session_id.clone()));
            }
            for frame in &mut frames[start..] {
                frame.out_of_process = true;
            }
        }
        let mut topology = self.topology.lock().await;
        topology
            .frame_sessions
            .retain(|_, session| !stale_frame_sessions.contains(session));
        for (frame_id, session_id) in discovered_frame_sessions {
            topology.frame_sessions.insert(frame_id, session_id);
        }
        topology.frames = frames.clone();
        Ok(frames)
    }

    /// Select a frame as the active context for subsequent operations.
    ///
    /// The frame must exist in the current target's frame tree. Selecting an
    /// iframe creates an isolated execution context for JavaScript evaluation.
    /// Invalidates the observation cache.
    pub async fn select_frame(&self, frame_id: &str) -> BrowserResult<FrameInfo> {
        validate_topology_id(frame_id)?;
        let frame = self
            .list_frames()
            .await?
            .into_iter()
            .find(|frame| frame.id == frame_id)
            .ok_or_else(|| {
                TopologyError::new(
                    TopologyErrorKind::NoSuchFrame,
                    format!("frame {frame_id} was not found; call listFrames to discover available frames"),
                )
            })?;
        let session_id = {
            let topology = self.topology.lock().await;
            topology
                .frame_sessions
                .get(frame_id)
                .cloned()
                .or_else(|| topology.active_target_session_id.clone())
                .ok_or_else(|| {
                    TopologyError::new(
                        TopologyErrorKind::NoPageSession,
                        "active target has no CDP session; the session may need to be re-established",
                    )
                })?
        };
        let context_id = if frame.parent_id.is_none() {
            None
        } else {
            let world = self
                .cdp
                .send_to_session(
                    &session_id,
                    "Page.createIsolatedWorld",
                    Some(serde_json::json!({"frameId": frame_id, "worldName":"glass"})),
                )
                .await?;
            Some(
                world["executionContextId"]
                    .as_i64()
                    .ok_or("Page.createIsolatedWorld returned no executionContextId")?,
            )
        };
        self.cdp.set_active_route(
            Some(session_id.clone()),
            Some(frame_id.to_string()),
            context_id,
        );
        {
            let mut topology = self.topology.lock().await;
            topology.active_frame_id = Some(frame_id.to_string());
            topology.active_session_id = Some(session_id);
        }
        self.invalidate_observation();
        Ok(FrameInfo {
            active: true,
            ..frame
        })
    }
}
