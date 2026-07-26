//! Page navigation and JavaScript evaluation.
//!
//! Navigates the active page target to a URL with configurable timeouts,
//! and evaluates JavaScript expressions in the page context.

use super::*;

impl BrowserSession {
    /// Return the current page's URL, title, and ready state.
    ///
    /// Evaluates `location.href`, `document.title`, and `document.readyState`
    /// in the active page context. Includes the current target and frame IDs.
    pub async fn page_info(&self) -> BrowserResult<PageInfo> {
        self.cdp.with_current_route(async {
        let raw = self
            .cdp
            .evaluate(
                "JSON.stringify({url: location.href, title: document.title, ready_state: document.readyState})",
            )
            .await?;
        let value = runtime_value(&raw)?;
        let json = value
            .as_str()
            .ok_or("document state evaluation returned a non-string value")?;
                let mut page: PageInfo = serde_json::from_str(json)?;
                (page.target_id, page.frame_id) = self.route_identity().await?;
                Ok(page)
        }).await
    }

    /// Navigate the active target to a URL with a 20-second deadline.
    ///
    /// Waits for the `Page.loadEventFired` lifecycle event before returning.
    /// The URL is normalized and validated against the active policy.
    pub async fn navigate(&self, url: &str) -> BrowserResult<PageInfo> {
        self.navigate_with_deadline(url, Duration::from_secs(20))
            .await
    }

    /// Navigate to a URL with an explicit deadline.
    ///
    /// Like [`navigate`](Self::navigate), but with a caller-specified timeout.
    /// `deadline` must be between 1 ms and 30 seconds.
    pub async fn navigate_with_deadline(
        &self,
        url: &str,
        deadline: Duration,
    ) -> BrowserResult<PageInfo> {
        self.cdp
            .with_current_target_route(async {
                validate_wait_deadline(deadline)?;
                let url = normalize_url(url);
                self.policy.require_url(&url).await?;
                if let Some(interception) = &self.policy_interception
                    && let Some(error) = interception.take_denial().await
                {
                    return Err(error.into());
                }
                let result = async {
                    let mut events = self.cdp.subscribe_events();
                    let started = tokio::time::Instant::now();
                    let navigation = tokio::time::timeout(deadline, self.cdp.navigate(&url))
                        .await
                        .map_err(|_| {
                            wait_timeout("lifecycle", deadline, "navigate_command_pending")
                        })??;
                    if let Some(frame_id) = navigation["frameId"].as_str() {
                        validate_topology_id(frame_id)?;
                        self.topology.lock().await.active_frame_id = Some(frame_id.to_string());
                        self.cdp
                            .set_active_frame_context(Some(frame_id.to_string()), None);
                    }
                    let remaining = deadline.saturating_sub(started.elapsed());
                    self.wait_loop(
                        WaitCondition::Lifecycle("complete".to_string()),
                        remaining,
                        deadline,
                        &mut events,
                        true,
                    )
                    .await?;
                    let remaining = deadline.saturating_sub(started.elapsed());
                    let main_frame = self
                        .list_frames()
                        .await?
                        .into_iter()
                        .find(|frame| frame.parent_id.is_none())
                        .ok_or("navigated target returned no main frame")?;
                    self.select_frame(&main_frame.id).await?;
                    let page = tokio::time::timeout(remaining, self.page_info())
                        .await
                        .map_err(|_| wait_timeout("lifecycle", deadline, "page_info_pending"))??;
                    self.invalidate_observation();
                    self.record_audit("navigate", url);
                    Ok(page)
                }
                .await;
                if let Some(error) = match &self.policy_interception {
                    Some(interception) => interception.take_denial().await,
                    None => None,
                } {
                    return Err(error.into());
                }
                result
            })
            .await
    }

    /// Evaluate arbitrary JavaScript in the active page context.
    ///
    /// Policy-gated: requires the `Evaluate` capability. Invalidates the
    /// observation cache after execution since arbitrary JS may mutate DOM.
    pub async fn evaluate(&self, expression: &str) -> BrowserResult<Value> {
        self.policy.require(PolicyCapability::Evaluate)?;
        self.cdp
            .with_current_route(async {
                let result = self.evaluate_value(expression).await;
                // Arbitrary JavaScript may mutate DOM, styles, form state, or history.
                // Invalidate synchronously so the next cached observation cannot race
                // the asynchronous CDP mutation event stream.
                self.invalidate_observation();
                self.record_audit("evaluate", redact_diagnostic_text(expression));
                result
            })
            .await
    }
}
