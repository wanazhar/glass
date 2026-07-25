use super::*;

impl BrowserSession {
    pub async fn wait(
        &self,
        condition: WaitCondition,
        deadline: Duration,
    ) -> BrowserResult<WaitOutcome> {
        self.cdp
            .with_current_route(async {
                validate_wait_deadline(deadline)?;
                condition.validate()?;
                if let WaitCondition::NetworkQuiet(quiet) = condition {
                    return tokio::time::timeout(
                        deadline,
                        self.wait_for_network_quiet(quiet, deadline),
                    )
                    .await
                    .map_err(|_| {
                        wait_timeout("network_quiet", deadline, "network_check_pending")
                    })?;
                }
                let mut events = self.cdp.subscribe_events();
                self.wait_loop(condition, deadline, deadline, &mut events, false)
                    .await
            })
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
                return Err(wait_timeout(&description, reported_deadline, &last_state).into());
            }
            let remaining = expires - now;
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
            let remaining = expires - now;
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
            WaitCondition::JavaScript(expression) => {
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
        let raw = self
            .cdp
            .call_on_object(&object_id, WAIT_TARGET_STATE_FUNCTION)
            .await;
        let _ = self.cdp.release_object(&object_id).await;
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
                    reason: "deadline_exceeded",
                }
                .into());
            }
            tokio::select! {
                _ = tokio::time::sleep((expires - now).min(WAIT_POLL_INTERVAL)) => {}
                event = events.recv() => match event {
                    Ok(event) => {
                      let request_id = event.params["requestId"].as_str();
                      match event.method.as_str() {
                        "Network.requestWillBeSent" => {
                            if let Some(id) = request_id {
                                if in_flight.len() < NETWORK_IN_FLIGHT_LIMIT {
                                    in_flight.insert(id.to_string());
                                } else {
                                    overflowed = true;
                                }
                            }
                        }
                        "Network.loadingFinished" | "Network.loadingFailed" => {
                            if let Some(id) = request_id { in_flight.remove(id); }
                            if in_flight.is_empty() && !overflowed { empty_since = tokio::time::Instant::now(); }
                        }
                        _ => {}
                      }
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => return Err("network wait event stream lagged".into()),
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => return Err("network wait event stream closed".into()),
                }
            }
        }
    }
}
