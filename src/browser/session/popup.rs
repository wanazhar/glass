//! Popup window click and witness tracking.
//!
//! Clicks a target that is expected to open a popup window, then verifies
//! the causal relationship by tracking CDP target creation events.

use super::*;

impl BrowserSession {
    /// Click a target that is expected to open a popup window.
    ///
    /// Monitors target creation events during the click, identifies the new
    /// popup target, and returns both the click outcome and the popup's
    /// [`PageTargetInfo`]. If no popup appears within the witness window,
    /// returns a [`PopupClickError`].
    pub async fn click_expect_popup(&self, target: &str) -> BrowserResult<PopupClickOutcome> {
        self.click_expect_popup_with_revision(target, None).await
    }

    /// Click a target expected to open a popup, optionally enforcing an observation revision.
    pub async fn click_expect_popup_with_revision(
        &self,
        target: &str,
        expected_revision: Option<u64>,
    ) -> BrowserResult<PopupClickOutcome> {
        let _scope = self.popup_click_scope.lock().await;
        self.cdp
            .with_current_route(async {
                self.require_expected_revision(expected_revision)?;
                let element = self.resolve_element(target).await?;
                let object_id = self
                    .cdp
                    .resolve_node_object(element.node_id, element.backend_dom_node_id)
                    .await
                    .map_err(|error| {
                        tracing::debug!(%error, "popup target node could not be resolved");
                        TargetError {
                            kind: TargetErrorKind::NotActionable,
                            reason: Some(TargetActionabilityReason::NodeUnavailable),
                            candidates: Vec::new(),
                            recovery: None,
                        }
                    })?;
                let remote = RemoteObjectGuard::new(self.cdp.clone(), object_id);
                let original_session_id = self
                    .cdp
                    .current_session_id()
                    .ok_or_else(|| {
                        TopologyError::new(
                            TopologyErrorKind::NoPageSession,
                            "popup click requires an attached page session; the session may need to be re-established",
                        )
                    })?;
                let original_frame_id = self
                    .cdp
                    .active_frame()
                    .ok_or_else(|| {
                        TopologyError::new(
                            TopologyErrorKind::StaleFrame,
                            "popup click requires an active frame; call listFrames to discover available frames",
                        )
                    })?;
                let backend_node_id = match (element.backend_dom_node_id, element.node_id) {
                    (Some(backend_node_id), _) => backend_node_id,
                    (None, Some(node_id)) => self
                        .cdp
                        .backend_node_id_for_node(node_id)
                        .await
                        .map_err(|error| {
                            popup_error(
                                PopupClickErrorKind::WitnessMissing,
                                format!(
                                    "resolved popup target has no readable backend identity: {error}"
                                ),
                            )
                        })?,
                    (None, None) => {
                        return Err(popup_error(
                            PopupClickErrorKind::WitnessMissing,
                            "popup target has no exact node identity",
                        ));
                    }
                };
                let mut witness = self
                    .arm_popup_witness(&original_session_id, &original_frame_id, backend_node_id)
                    .await?;
                let operation = self
                    .perform_popup_click(&remote.object_id, &element, &mut witness)
                    .await;
                let cleanup = witness.cleanup().await;
                match (operation, cleanup) {
                    (Ok(outcome), Ok(())) => Ok(outcome),
                    (Err(error), _) => Err(error),
                    (Ok(_), Err(error)) => Err(error),
                }
            })
            .await
    }

    async fn arm_popup_witness(
        &self,
        session_id: &str,
        frame_id: &str,
        backend_node_id: i64,
    ) -> BrowserResult<PopupWitnessGuard> {
        let cdp = self.cdp.clone();
        let session_id = session_id.to_string();
        let frame_id = frame_id.to_string();
        let (sender, receiver) = tokio::sync::oneshot::channel();
        tokio::spawn(async move {
            let result = arm_popup_witness_owned(cdp, session_id, frame_id, backend_node_id).await;
            let _ = sender.send(result);
        });
        receiver
            .await
            .map_err(|_| {
                popup_error(
                    PopupClickErrorKind::WitnessMissing,
                    "popup witness worker ended without a result",
                )
            })?
            .map_err(Into::into)
    }

    async fn perform_popup_click(
        &self,
        object_id: &str,
        element: &ResolvedElement,
        witness: &mut PopupWitnessGuard,
    ) -> BrowserResult<PopupClickOutcome> {
        let local_point = self.verified_action_point(object_id).await?;
        let point = self.target_viewport_point(local_point).await?;
        let mut pointer = self.pointer.lock().await;
        let start = match (self.interaction_mode, *pointer) {
            (_, Some(point)) => point,
            (InteractionMode::Human, None) => self
                .viewport_center()
                .await
                .unwrap_or(Point { x: 640.0, y: 360.0 }),
            (InteractionMode::Fast, None) => point,
        };
        let path = interaction_path(self.interaction_mode, &self.mouse, start, point);
        if self.interaction_mode == InteractionMode::Human && pointer.is_none() {
            self.cdp
                .dispatch_mouse_event("mouseMoved", start.x, start.y, None, None)
                .await?;
        }
        for window in path.windows(2) {
            let next = window[1];
            if self.interaction_mode == InteractionMode::Human {
                tokio::time::sleep(self.mouse.move_delay(window[0], next)).await;
            }
            self.cdp
                .dispatch_mouse_event("mouseMoved", next.x, next.y, None, None)
                .await?;
        }
        let press_point = self.verified_action_point(object_id).await?;
        if (press_point.x - local_point.x).abs() > 1.0
            || (press_point.y - local_point.y).abs() > 1.0
        {
            return Err(TargetError {
                kind: TargetErrorKind::NotActionable,
                reason: Some(TargetActionabilityReason::GeometryChanged),
                candidates: Vec::new(),
                recovery: None,
            }
            .into());
        }
        self.cdp
            .dispatch_mouse_event("mousePressed", point.x, point.y, Some("left"), Some(1))
            .await?;
        let mut pressed = PressedButtonGuard {
            cdp: self.cdp.clone(),
            point,
            click_count: 1,
            armed: true,
        };
        if self.interaction_mode == InteractionMode::Human {
            tokio::time::sleep(self.mouse.click_delay()).await;
        }

        // This snapshot is intentionally adjacent to and before the release.
        let snapshot = self.popup_topology_snapshot().await?;
        let release_started = std::time::Instant::now();
        let release = self
            .cdp
            .dispatch_mouse_event_with_timeout(
                "mouseReleased",
                point.x,
                point.y,
                Some("left"),
                Some(1),
                POPUP_RELEASE_ACK_TIMEOUT,
            )
            .await;
        let release_ack_wait_ms = release_started.elapsed().as_secs_f64() * 1_000.0;
        let release_acknowledged = match release {
            Ok(_) => true,
            Err(error) if error.is_response_timeout() => false,
            Err(error) => {
                return Err(popup_error(
                    PopupClickErrorKind::ReleaseFailed,
                    format!("mouseReleased failed without a response timeout: {error}"),
                ));
            }
        };
        // The release was either acknowledged or causally witnessed. Never emit
        // the guard's second, fire-and-forget release for the timeout case.
        pressed.armed = false;

        let evidence_deadline = tokio::time::Instant::now() + POPUP_EVIDENCE_DEADLINE;
        let candidate = self
            .wait_for_causal_popup(&snapshot, witness, evidence_deadline)
            .await?;
        let ready_state = self
            .verify_popup_readiness(&snapshot, &candidate, evidence_deadline)
            .await?;
        *pointer = Some(point);
        Ok(PopupClickOutcome {
            action: ActionKind::ClickExpectPopup,
            execution_id: self.next_execution_id(),
            target: ActionTarget {
                label: element.label.clone(),
                reference: element.reference.clone(),
            },
            revision: self.invalidate_observation().await,
            target_id: snapshot.original_target_id.clone(),
            frame_id: snapshot.original_frame_id.clone(),
            causally_verified_popup: true,
            popup_id: candidate.target.id.clone(),
            opener_id: snapshot.original_target_id.clone(),
            evidence: PopupVerificationEvidence {
                trusted_click_witness: true,
                release_acknowledged,
                release_ack_wait_ms,
                topology_sequence_before_release: snapshot.sequence,
                popup_observed_sequence: candidate.observed_sequence,
                attached: true,
                ready_state,
            },
        })
    }

    async fn popup_topology_snapshot(&self) -> BrowserResult<PopupTopologySnapshot> {
        let raw_targets = popup_verification_call(
            self.cdp.send_browser("Target.getTargets", None),
            "pre-release target snapshot",
        )
        .await?;
        let mut preexisting_target_ids = HashSet::new();
        for info in raw_targets["targetInfos"].as_array().into_iter().flatten() {
            if info["type"].as_str() != Some("page") {
                continue;
            }
            let id = info["targetId"].as_str().ok_or_else(|| {
                popup_error(
                    PopupClickErrorKind::PopupUnreadable,
                    "pre-release target snapshot contained a page without an ID",
                )
            })?;
            validate_topology_id(id)?;
            preexisting_target_ids.insert(id.to_string());
        }
        let topology = self.topology.lock().await;
        let original_target_id = topology.active_target_id.clone().ok_or_else(|| {
            TopologyError::new(
                TopologyErrorKind::NoTargetSelected,
                "popup click has no active target; call listTargets to discover available pages",
            )
        })?;
        let original_frame_id = topology.active_frame_id.clone().ok_or_else(|| {
            TopologyError::new(
                TopologyErrorKind::StaleFrame,
                "popup click has no active frame; call listFrames to discover available frames",
            )
        })?;
        Ok(PopupTopologySnapshot {
            original_target_id,
            original_frame_id,
            preexisting_target_ids,
            sequence: topology.sequence,
            event_loss_count: topology.event_loss_count,
        })
    }

    async fn wait_for_causal_popup(
        &self,
        snapshot: &PopupTopologySnapshot,
        witness: &PopupWitnessGuard,
        deadline: tokio::time::Instant,
    ) -> BrowserResult<PopupCandidate> {
        let mut witnessed = false;
        loop {
            if !witnessed {
                witnessed = witness.fired().await?;
            }

            let assessment = {
                let topology = self.topology.lock().await;
                assess_popup_topology(snapshot, &topology, witnessed)
            };
            match assessment {
                Ok(candidate) => {
                    return wait_for_stable_popup_topology(
                        &self.topology,
                        snapshot,
                        &candidate,
                        deadline,
                        POPUP_TOPOLOGY_QUIET_INTERVAL,
                    )
                    .await
                    .map_err(Into::into);
                }
                Err(error)
                    if matches!(
                        error.kind,
                        PopupClickErrorKind::TopologyLagged
                            | PopupClickErrorKind::PopupAmbiguous
                            | PopupClickErrorKind::PopupDestroyed
                    ) =>
                {
                    return Err(error.into());
                }
                Err(error) if tokio::time::Instant::now() >= deadline => {
                    return Err(error.into());
                }
                Err(_) => tokio::time::sleep(Duration::from_millis(10)).await,
            }
        }
    }

    async fn verify_popup_readiness(
        &self,
        snapshot: &PopupTopologySnapshot,
        candidate: &PopupCandidate,
        deadline: tokio::time::Instant,
    ) -> BrowserResult<String> {
        let mut attachment = self.attach_popup(&candidate.target.id).await?;
        popup_verification_call(
            self.cdp.send_to_session(
                &attachment.session_id,
                "Runtime.runIfWaitingForDebugger",
                None,
            ),
            "popup debugger resume",
        )
        .await?;
        let result = popup_verification_call(
            self.cdp.send_to_session(
                &attachment.session_id,
                "Runtime.evaluate",
                Some(serde_json::json!({
                    "expression": "document.readyState",
                    "returnByValue": true,
                    "awaitPromise": false
                })),
            ),
            "popup readiness evaluation",
        )
        .await?;
        let ready_state = result["result"]["value"]
            .as_str()
            .filter(|state| matches!(*state, "loading" | "interactive" | "complete"))
            .map(str::to_string)
            .ok_or_else(|| {
                popup_error(
                    PopupClickErrorKind::PopupUnreadable,
                    "popup returned no valid document.readyState",
                )
            })?;
        self.final_popup_verification(snapshot, candidate, deadline)
            .await?;
        attachment.detach().await?;
        Ok(ready_state)
    }

    pub(crate) async fn attach_popup(
        &self,
        target_id: &str,
    ) -> BrowserResult<PopupAttachmentGuard> {
        let cdp = self.cdp.clone();
        let target_id = target_id.to_string();
        let (sender, receiver) = tokio::sync::oneshot::channel();
        tokio::spawn(async move {
            let result = match tokio::time::timeout(
                POPUP_VERIFY_CALL_TIMEOUT,
                cdp.send_browser(
                    "Target.attachToTarget",
                    Some(serde_json::json!({"targetId": target_id, "flatten": true})),
                ),
            )
            .await
            {
                Ok(Ok(value)) => value["sessionId"]
                    .as_str()
                    .map(|session_id| PopupAttachmentGuard {
                        cdp: cdp.clone(),
                        session_id: session_id.to_string(),
                        armed: true,
                    })
                    .ok_or_else(|| {
                        popup_typed_error(
                            PopupClickErrorKind::PopupUnreadable,
                            "popup attach returned no session ID",
                        )
                    }),
                Ok(Err(error)) => Err(popup_typed_error(
                    PopupClickErrorKind::PopupUnreadable,
                    format!("popup attach failed: {error}"),
                )),
                Err(_) => Err(popup_typed_error(
                    PopupClickErrorKind::PopupUnreadable,
                    "popup attach exceeded its bounded deadline",
                )),
            };
            let _ = sender.send(result);
        });
        receiver
            .await
            .map_err(|_| {
                popup_error(
                    PopupClickErrorKind::PopupUnreadable,
                    "popup attach worker ended without a result",
                )
            })?
            .map_err(Into::into)
    }

    pub(crate) async fn final_popup_verification(
        &self,
        snapshot: &PopupTopologySnapshot,
        candidate: &PopupCandidate,
        deadline: tokio::time::Instant,
    ) -> BrowserResult<()> {
        loop {
            if tokio::time::Instant::now() >= deadline {
                return Err(popup_error(
                    PopupClickErrorKind::TopologyLagged,
                    "popup topology did not settle before final verification deadline",
                ));
            }
            let (stable_sequence, stable_loss) = {
                let topology = self.topology.lock().await;
                let current = assess_popup_topology(snapshot, &topology, true)?;
                if current.target.id != candidate.target.id {
                    return Err(popup_error(
                        PopupClickErrorKind::PopupAmbiguous,
                        "popup candidate changed during readiness verification",
                    ));
                }
                (topology.sequence, topology.event_loss_count)
            };
            let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
            if remaining.is_zero() {
                return Err(popup_error(
                    PopupClickErrorKind::TopologyLagged,
                    "popup topology deadline expired before authoritative discovery",
                ));
            }
            let targets = match tokio::time::timeout(
                remaining,
                self.cdp.send_browser("Target.getTargets", None),
            )
            .await
            {
                Ok(Ok(targets)) => targets,
                Ok(Err(error)) => {
                    return Err(popup_error(
                        PopupClickErrorKind::PopupUnreadable,
                        format!("final authoritative popup target discovery failed: {error}"),
                    ));
                }
                Err(_) => {
                    return Err(popup_error(
                        PopupClickErrorKind::TopologyLagged,
                        "final authoritative popup target discovery exceeded the evidence deadline",
                    ));
                }
            };
            let mut matches = Vec::new();
            for info in targets["targetInfos"].as_array().ok_or_else(|| {
                popup_error(
                    PopupClickErrorKind::PopupUnreadable,
                    "final target discovery returned no target list",
                )
            })? {
                if info["type"].as_str() != Some("page") {
                    continue;
                }
                let id = info["targetId"].as_str().ok_or_else(|| {
                    popup_error(
                        PopupClickErrorKind::PopupUnreadable,
                        "final target discovery contained a page without an ID",
                    )
                })?;
                validate_topology_id(id)?;
                if !snapshot.preexisting_target_ids.contains(id)
                    && info["openerId"].as_str() == Some(snapshot.original_target_id.as_str())
                {
                    matches.push(id);
                }
            }
            if matches.len() != 1 || matches[0] != candidate.target.id {
                return Err(popup_error(
                    if matches.len() > 1 {
                        PopupClickErrorKind::PopupAmbiguous
                    } else {
                        PopupClickErrorKind::PopupDestroyed
                    },
                    format!(
                        "final target discovery found {} live later opener matches",
                        matches.len()
                    ),
                ));
            }
            let topology = self.topology.lock().await;
            if topology.event_loss_count != stable_loss {
                return Err(popup_error(
                    PopupClickErrorKind::TopologyLagged,
                    "popup topology event loss changed during final verification",
                ));
            }
            let current = assess_popup_topology(snapshot, &topology, true)?;
            if current.target.id != candidate.target.id {
                return Err(popup_error(
                    PopupClickErrorKind::PopupAmbiguous,
                    "popup candidate changed at final topology verification",
                ));
            }
            if topology.sequence == stable_sequence {
                if tokio::time::Instant::now() >= deadline {
                    return Err(popup_error(
                        PopupClickErrorKind::TopologyLagged,
                        "popup topology deadline expired before final success",
                    ));
                }
                return Ok(());
            }
            drop(topology);
            wait_for_stable_popup_topology(
                &self.topology,
                snapshot,
                candidate,
                deadline,
                POPUP_TOPOLOGY_QUIET_INTERVAL,
            )
            .await?;
        }
    }
}
