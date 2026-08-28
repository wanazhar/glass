//! Page wait conditions and lifecycle detection.
//!
//! Waits for conditions such as URL changes, element visibility, text
//! appearance, navigation completion, or configurable timeouts.

use super::*;
use std::collections::HashSet;
use std::future::Future;
use std::pin::Pin;

type VerificationCheckFuture<'a> =
    Pin<Box<dyn Future<Output = BrowserResult<(bool, String)>> + 'a>>;

struct VerificationBaseline {
    target_count: usize,
    download_sequence: u64,
}

pub(crate) fn is_ignored_network_resource_type(resource_type: Option<&str>) -> bool {
    matches!(
        resource_type,
        Some("WebSocket" | "EventSource" | "Media" | "Ping")
    )
}

impl BrowserSession {
    pub(crate) async fn evaluate_predicate_once(
        &self,
        predicate: &VerificationPredicate,
    ) -> BrowserResult<(bool, String)> {
        predicate.validate(0)?;
        self.check_verification_predicate(predicate, None).await
    }

    /// Evaluate a bounded, composable postcondition until it becomes true.
    pub async fn verify(
        &self,
        predicate: VerificationPredicate,
        deadline: Duration,
    ) -> BrowserResult<VerificationOutcome> {
        validate_wait_deadline(deadline)?;
        predicate.validate(0)?;
        let started = tokio::time::Instant::now();
        let expires = started + deadline;
        let baseline = self.verification_baseline().await;
        loop {
            let (matched, observed) = self
                .check_verification_predicate(&predicate, Some(&baseline))
                .await?;
            let state = bounded_diagnostic_text(&observed);
            if matched {
                return Ok(VerificationOutcome {
                    status: "satisfied",
                    predicate,
                    elapsed_ms: started.elapsed().as_millis() as u64,
                    state,
                });
            }
            if tokio::time::Instant::now() >= expires {
                return Err(ActionVerificationError {
                    kind: ActionFailureKind::VerificationFailed,
                    action: ActionKind::Click,
                    phase: ActionFailurePhase::Verification,
                    recovery_strategy: RecoveryStrategy::Report,
                    execution_id: Some(self.next_execution_id()),
                    target: None,
                    revision: self.page_revision.load(Ordering::Relaxed),
                    reason: format!("verification predicate not satisfied: {state}"),
                }
                .into());
            }
            tokio::time::sleep(WAIT_POLL_INTERVAL).await;
        }
    }

    async fn verification_baseline(&self) -> VerificationBaseline {
        let topology = self.topology.lock().await;
        VerificationBaseline {
            target_count: topology.targets.len(),
            download_sequence: self.download_sequence.load(Ordering::Relaxed),
        }
    }

    fn check_verification_predicate<'a>(
        &'a self,
        predicate: &'a VerificationPredicate,
        baseline: Option<&'a VerificationBaseline>,
    ) -> VerificationCheckFuture<'a> {
        Box::pin(async move {
            match predicate {
                VerificationPredicate::UrlEquals { value } => {
                    let url = self.page_info().await?.url;
                    Ok((url == *value, format!("url={url}")))
                }
                VerificationPredicate::TitleContains { value } => {
                    let title = self.page_info().await?.title;
                    Ok((title.contains(value), format!("title={title}")))
                }
                VerificationPredicate::Visible { visible } => {
                    let (matched, state, _) = self
                        .check_wait_condition(&WaitCondition::TargetVisible(visible.clone()), None)
                        .await?;
                    Ok((matched, state))
                }
                VerificationPredicate::TextContains { value } => {
                    let (matched, state, _) = self
                        .check_wait_condition(&WaitCondition::Text(value.clone()), None)
                        .await?;
                    Ok((matched, state))
                }
                VerificationPredicate::PopupOpened { value } => {
                    let topology = self.topology.lock().await;
                    let opened = causal_popup_opened(topology.targets.len(), baseline);
                    Ok((opened == *value, format!("popupOpened={opened}")))
                }
                VerificationPredicate::DialogOpen { value } => {
                    let topology = self.topology.lock().await;
                    let open = topology.pending_dialog.is_some();
                    Ok((open == *value, format!("dialogOpen={open}")))
                }
                VerificationPredicate::DownloadStarted { value } => {
                    let sequence = self.download_sequence.load(Ordering::Relaxed);
                    let started = causal_download_started(sequence, baseline);
                    Ok((started == *value, format!("downloadStarted={started}")))
                }
                VerificationPredicate::RevisionEquals { value } => {
                    let revision = self.page_revision.load(Ordering::Relaxed);
                    Ok((revision == *value, format!("revision={revision}")))
                }
                VerificationPredicate::All { all } => {
                    let mut states = Vec::with_capacity(all.len());
                    let mut matched = true;
                    for child in all {
                        let (child_matched, state) =
                            self.check_verification_predicate(child, baseline).await?;
                        matched &= child_matched;
                        states.push(state);
                    }
                    Ok((matched, format!("all=[{}]", states.join(","))))
                }
                VerificationPredicate::Any { any } => {
                    let mut states = Vec::with_capacity(any.len());
                    let mut matched = false;
                    for child in any {
                        let (child_matched, state) =
                            self.check_verification_predicate(child, baseline).await?;
                        matched |= child_matched;
                        states.push(state);
                    }
                    Ok((matched, format!("any=[{}]", states.join(","))))
                }
                VerificationPredicate::Not { not } => {
                    let (matched, state) = self.check_verification_predicate(not, baseline).await?;
                    Ok((!matched, format!("not({state})")))
                }
            }
        })
    }

    /// Wait for a condition to be satisfied on the page.
    ///
    /// Supported conditions include lifecycle events (`"complete"`, `"interactive"`),
    /// URL matching, target visibility/enabled/stability, text presence,
    /// JavaScript expressions, and network quiet. Returns a [`WaitOutcome`]
    /// on success or a [`WaitTimeout`] error if the deadline expires.
    pub async fn wait(
        &self,
        condition: WaitCondition,
        deadline: Duration,
    ) -> BrowserResult<WaitOutcome> {
        validate_wait_deadline(deadline)?;
        condition.validate()?;
        if let WaitCondition::NetworkQuiet(quiet) = condition {
            return tokio::time::timeout(deadline, self.wait_for_network_quiet(quiet, deadline))
                .await
                .map_err(|_| wait_timeout("network_quiet", deadline, "network_check_pending"))?;
        }
        let mut events = self.cdp.subscribe_events();
        self.wait_loop(condition, deadline, deadline, &mut events, false)
            .await
    }

    pub(crate) async fn wait_loop(
        &self,
        condition: WaitCondition,
        deadline: Duration,
        reported_deadline: Duration,
        events: &mut tokio::sync::broadcast::Receiver<crate::browser::cdp::CdpEvent>,
        require_load_event: bool,
    ) -> BrowserResult<WaitOutcome> {
        let started = tokio::time::Instant::now();
        let expires = started + deadline;
        let mut previous_geometry = None;
        let description = condition.description();
        let mut load_event_seen = !require_load_event;
        let mut last_state = "not_checked".to_string();
        loop {
            let now = tokio::time::Instant::now();
            if now >= expires {
                return Err(WaitTimeout {
                    condition: description.clone(),
                    deadline_ms: reported_deadline.as_millis() as u64,
                    last_state: bounded_wait_state(&last_state),
                    observed_page: self.page_info().await.ok(),
                    reason: "deadline_exceeded",
                }
                .into());
            }
            // Refresh the frame route before every probe. This is cheap for
            // stable pages and prevents a post-SPA-navigation stale-frame
            // error from poisoning the remainder of the wait.
            let remaining = expires.saturating_duration_since(now);
            tokio::time::timeout(remaining, self.ensured_route_identity())
                .await
                .map_err(|_| wait_timeout(&description, reported_deadline, &last_state))??;
            let remaining = expires.saturating_duration_since(tokio::time::Instant::now());
            let (matched, state, geometry) = tokio::time::timeout(
                remaining,
                self.check_wait_condition(&condition, previous_geometry.as_deref()),
            )
            .await
            .map_err(|_| wait_timeout(&description, reported_deadline, &last_state))??;
            last_state = bounded_wait_state(&state);
            previous_geometry = geometry;
            if matched && load_event_seen {
                let (target_id, frame_id) = self.ensured_route_identity().await?;
                return Ok(WaitOutcome {
                    condition: description,
                    elapsed_ms: started.elapsed().as_millis() as u64,
                    last_state,
                    target_id,
                    frame_id,
                });
            }
            let now = tokio::time::Instant::now();
            let remaining = expires.saturating_duration_since(now);
            tokio::select! {
                _ = tokio::time::sleep(WAIT_POLL_INTERVAL.min(remaining)) => {}
                event = events.recv() => match event {
                    Ok(event) => { load_event_seen |= event.method == "Page.loadEventFired"; }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {}
                    Err(_) => return Err("CDP event stream closed during wait".into()),
                }
            }
        }
    }

    async fn check_wait_condition(
        &self,
        condition: &WaitCondition,
        previous_geometry: Option<&str>,
    ) -> BrowserResult<(bool, String, Option<String>)> {
        match condition {
            WaitCondition::Lifecycle(expected) => {
                let page = self.page_info().await?;
                Ok((page.ready_state == *expected, page.ready_state, None))
            }
            WaitCondition::UrlExact(expected) => {
                let page = self.page_info().await?;
                Ok((page.url == *expected, page.url, None))
            }
            WaitCondition::UrlPrefix(prefix) => {
                let page = self.page_info().await?;
                Ok((page.url.starts_with(prefix), page.url, None))
            }
            WaitCondition::Text(expected) => {
                let expression = visible_text_contains_expression(expected)?;
                let value = self.evaluate_value(&expression).await?;
                let matched = value.as_bool().unwrap_or(false);
                Ok((matched, format!("present={matched}"), None))
            }
            WaitCondition::SemanticRegion(region_id) => {
                let context = self.observe_fresh().await?;
                let observation = SemanticObservation::from_page_context(
                    &context,
                    SemanticObservationLevel::Interactive,
                )?;
                let region = observation
                    .regions
                    .iter()
                    .find(|region| region.id == *region_id);
                let matched = region.is_some_and(|region| !region.targets.is_empty());
                Ok((matched, format!("region={region_id};ready={matched}"), None))
            }
            WaitCondition::JavaScript(expression) => {
                self.policy.require(PolicyCapability::Evaluate)?;
                let value = self.evaluate_value(expression).await?;
                let matched = value
                    .as_bool()
                    .ok_or("wait JavaScript predicate must return a boolean")?;
                Ok((matched, matched.to_string(), None))
            }
            WaitCondition::TargetAttached(target)
            | WaitCondition::TargetVisible(target)
            | WaitCondition::TargetHidden(target)
            | WaitCondition::TargetEnabled(target)
            | WaitCondition::TargetStable(target) => {
                self.check_target_wait(condition, target, previous_geometry)
                    .await
            }
            WaitCondition::NetworkQuiet(_) => unreachable!("handled by wait"),
        }
    }

    async fn check_target_wait(
        &self,
        condition: &WaitCondition,
        target: &str,
        previous_geometry: Option<&str>,
    ) -> BrowserResult<(bool, String, Option<String>)> {
        let element = match self.resolve_element(target).await {
            Ok(element) => element,
            Err(error)
                if error
                    .downcast_ref::<TargetError>()
                    .is_some_and(|error| error.kind == TargetErrorKind::NotFound) =>
            {
                let matched = matches!(condition, WaitCondition::TargetHidden(_));
                return Ok((matched, "detached".to_string(), None));
            }
            Err(error) => return Err(error),
        };
        if matches!(condition, WaitCondition::TargetAttached(_)) {
            return Ok((true, "attached".to_string(), None));
        }
        let object_id = self
            .cdp
            .resolve_node_object(element.node_id, element.backend_dom_node_id)
            .await?;
        let remote = RemoteObjectGuard::new(self.cdp.clone(), object_id);
        let raw = self
            .cdp
            .call_on_object(&remote.object_id, WAIT_TARGET_STATE_FUNCTION)
            .await;
        let value = runtime_value(&raw?)?;
        let visible = value["visible"].as_bool().unwrap_or(false);
        let enabled = value["enabled"].as_bool().unwrap_or(false);
        let geometry = value["geometry"].as_str().map(str::to_string);
        let matched = match condition {
            WaitCondition::TargetVisible(_) => visible,
            WaitCondition::TargetHidden(_) => !visible,
            WaitCondition::TargetEnabled(_) => visible && enabled,
            WaitCondition::TargetStable(_) => {
                visible
                    && geometry
                        .as_deref()
                        .is_some_and(|geometry| previous_geometry == Some(geometry))
            }
            _ => unreachable!(),
        };
        Ok((matched, value.to_string(), geometry))
    }

    async fn wait_for_network_quiet(
        &self,
        quiet: Duration,
        deadline: Duration,
    ) -> BrowserResult<WaitOutcome> {
        if quiet.is_zero() {
            return Err("network quiet duration must be positive".into());
        }
        let mut events = self.cdp.subscribe_events_with_params();
        let mut guard =
            NetworkDomainGuard::acquire(self.cdp.clone(), Arc::clone(&self.network_wait_leases))
                .await?;
        let started = tokio::time::Instant::now();
        let expires = started + deadline;
        let mut empty_since = started;
        let mut in_flight = HashSet::new();
        let mut overflowed = false;
        loop {
            self.ensured_route_identity().await?;
            let now = tokio::time::Instant::now();
            if in_flight.is_empty() && !overflowed && now.duration_since(empty_since) >= quiet {
                guard.disable().await?;
                let (target_id, frame_id) = self.route_identity().await?;
                return Ok(WaitOutcome {
                    condition: "network_quiet".to_string(),
                    elapsed_ms: started.elapsed().as_millis() as u64,
                    last_state: "in_flight=0".to_string(),
                    target_id,
                    frame_id,
                });
            }
            if now >= expires {
                return Err(WaitTimeout {
                    condition: "network_quiet".to_string(),
                    deadline_ms: deadline.as_millis() as u64,
                    last_state: if overflowed {
                        "in_flight=overflow".to_string()
                    } else {
                        format!("in_flight={}", in_flight.len())
                    },
                    observed_page: self.page_info().await.ok(),
                    reason: "deadline_exceeded",
                }
                .into());
            }
            tokio::select! {
                _ = tokio::time::sleep(expires.saturating_duration_since(now).min(WAIT_POLL_INTERVAL)) => {}
                event = events.recv() => match event {
                    Ok(event) => {
                        let request_id = event.params["requestId"].as_str();
                        match event.method.as_str() {
                            "Network.requestWillBeSent" => {
                                if is_ignored_network_resource_type(event.params["type"].as_str()) {
                                    continue;
                                }
                                if let Some(id) = request_id {
                                    if in_flight.len() < NETWORK_IN_FLIGHT_LIMIT {
                                        in_flight.insert(id.to_string());
                                    } else {
                                        overflowed = true;
                                    }
                                }
                            }
                            "Network.loadingFinished" | "Network.loadingFailed" => {
                                if let Some(id) = request_id {
                                    in_flight.remove(id);
                                }
                                if in_flight.is_empty() && !overflowed {
                                    empty_since = tokio::time::Instant::now();
                                }
                            }
                            _ => {}
                        }
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {
                        return Err("network wait event stream lagged".into());
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                        return Err("network wait event stream closed".into());
                    }
                }
            }
        }
    }
}

fn causal_popup_opened(target_count: usize, baseline: Option<&VerificationBaseline>) -> bool {
    match baseline {
        Some(baseline) => target_count > baseline.target_count,
        None => target_count > 1,
    }
}

fn causal_download_started(sequence: u64, baseline: Option<&VerificationBaseline>) -> bool {
    match baseline {
        Some(baseline) => sequence > baseline.download_sequence,
        None => sequence > 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn popup_opened_is_causal_when_a_baseline_is_present() {
        let baseline = VerificationBaseline {
            target_count: 2,
            download_sequence: 4,
        };
        assert!(!causal_popup_opened(2, Some(&baseline)));
        assert!(causal_popup_opened(3, Some(&baseline)));
        assert!(causal_popup_opened(2, None));
        assert!(!causal_popup_opened(1, None));
    }

    #[test]
    fn download_started_is_causal_when_a_baseline_is_present() {
        let baseline = VerificationBaseline {
            target_count: 1,
            download_sequence: 3,
        };
        assert!(!causal_download_started(3, Some(&baseline)));
        assert!(causal_download_started(4, Some(&baseline)));
        assert!(causal_download_started(1, None));
        assert!(!causal_download_started(0, None));
    }
}
