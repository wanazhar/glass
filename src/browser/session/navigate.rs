//! Page navigation and JavaScript evaluation.
//!
//! Navigates the active page target to a URL with configurable timeouts,
//! and evaluates JavaScript expressions in the page context.

use super::*;
use crate::browser::cdp::CdpEventWithParams;
use url::Url;

const MAX_NAVIGATION_EVIDENCE_EVENTS: usize = 256;

pub(crate) struct NavigationRedirectCollector {
    requested_url: String,
    frame_id: Option<String>,
    request_id: Option<String>,
    redirect_count: Option<u16>,
}

impl NavigationRedirectCollector {
    pub(crate) fn new(requested_url: &str, frame_id: Option<&str>) -> Self {
        Self {
            requested_url: requested_url.to_string(),
            frame_id: frame_id.map(str::to_string),
            request_id: None,
            redirect_count: None,
        }
    }

    pub(crate) fn observe(&mut self, event: &CdpEventWithParams) {
        if event.method != "Network.requestWillBeSent" {
            return;
        }
        let Some(request_id) = event.params["requestId"].as_str() else {
            return;
        };
        if let Some(frame_id) = self.frame_id.as_deref()
            && let Some(event_frame_id) = event.params["frameId"].as_str()
            && event_frame_id != frame_id
        {
            return;
        }
        if self.request_id.as_deref() == Some(request_id) {
            if event
                .params
                .get("redirectResponse")
                .is_some_and(serde_json::Value::is_object)
            {
                let count = self.redirect_count.get_or_insert(0);
                *count = count.saturating_add(1);
            }
            return;
        }
        let request = &event.params["request"];
        if request["type"]
            .as_str()
            .is_some_and(|kind| kind != "Document")
            || !navigation_urls_match(
                request["url"].as_str().unwrap_or_default(),
                &self.requested_url,
            )
        {
            return;
        }
        if self.request_id.is_none() {
            self.request_id = Some(request_id.to_string());
            self.redirect_count = Some(u16::from(
                event
                    .params
                    .get("redirectResponse")
                    .is_some_and(serde_json::Value::is_object),
            ));
        }
    }

    pub(crate) fn finish(self) -> (Option<u16>, NavigationRedirectEvidence) {
        match self.redirect_count {
            Some(count) => (
                Some(count),
                NavigationRedirectEvidence {
                    status: NavigationRedirectStatus::Observed,
                    source: Some("Network.requestWillBeSent".to_string()),
                },
            ),
            None => (
                None,
                NavigationRedirectEvidence {
                    status: NavigationRedirectStatus::Unknown,
                    source: Some("bounded network evidence unavailable".to_string()),
                },
            ),
        }
    }
}

fn navigation_urls_match(observed: &str, requested: &str) -> bool {
    match (Url::parse(observed), Url::parse(requested)) {
        (Ok(observed), Ok(requested)) => observed == requested,
        _ => observed == requested,
    }
}

/// Derive same-origin metadata from parsed URLs. Opaque origins (for example
/// `about:blank`) are intentionally reported as unknown rather than guessed.
pub(crate) fn navigation_same_origin(requested: &str, observed: &str) -> Option<bool> {
    let requested = Url::parse(requested).ok()?;
    let observed = Url::parse(observed).ok()?;
    let requested_origin = requested.origin();
    let observed_origin = observed.origin();
    if requested_origin.ascii_serialization() == "null"
        || observed_origin.ascii_serialization() == "null"
    {
        return None;
    }
    Some(requested_origin == observed_origin)
}

fn safe_navigation_url(value: &str) -> String {
    redact_diagnostic_url(value)
}

pub(crate) fn navigation_identity(
    requested_url: &str,
    page: &PageInfo,
    redirect_count: Option<u16>,
    redirect_evidence: NavigationRedirectEvidence,
) -> NavigationIdentityMetadata {
    let observed_final_url = safe_navigation_url(&page.url);
    NavigationIdentityMetadata {
        requested_url: safe_navigation_url(requested_url),
        same_origin: navigation_same_origin(requested_url, &page.url),
        observed_final_url,
        redirect_count,
        redirect_evidence,
        classification: Some(classify_page_state(page, "", &[])),
    }
}

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
        self.navigate_with_deadline_and_metadata(url, deadline)
            .await
            .map(|(mut page, _)| {
                page.url = safe_navigation_url(&page.url);
                self.mark_lifecycle_phase(LifecyclePhase::ActionVerified);
                page
            })
    }

    /// Run the bounded navigation lifecycle and retain raw page metadata plus
    /// redirect evidence for crate-internal callers.
    pub(crate) async fn navigate_with_deadline_and_metadata(
        &self,
        url: &str,
        deadline: Duration,
    ) -> BrowserResult<(PageInfo, (Option<u16>, NavigationRedirectEvidence))> {
        self.cdp
            .with_current_target_route(async {
                validate_wait_deadline(deadline)?;
                let url = normalize_url(url);
                self.policy.require_url(&url).await?;
                self.enforce_polite_navigation(&url).await?;
                if let Some(interception) = &self.policy_interception
                    && let Some(error) = interception.take_denial().await
                {
                    return Err(error.into());
                }
                let result = async {
                    let mut events = self.cdp.subscribe_events();
                    let mut payload_events = self.cdp.subscribe_events_with_params();
                    let route_session_id = self.cdp.current_session_id();
                    self.cdp.enable_network().await?;
                    let started = tokio::time::Instant::now();
                    self.mark_lifecycle_phase(LifecyclePhase::NavigationStarted);
                    let navigation = tokio::time::timeout(deadline, self.cdp.navigate(&url))
                        .await
                        .map_err(|_| {
                            wait_timeout("lifecycle", deadline, "navigate_command_pending")
                        })??;
                    if let Some(frame_id) = navigation.frame_id.as_deref() {
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
                    let mut redirect_collector =
                        NavigationRedirectCollector::new(&url, navigation.frame_id.as_deref());
                    for _ in 0..MAX_NAVIGATION_EVIDENCE_EVENTS {
                        match payload_events.try_recv() {
                            Ok(event)
                                if route_session_id.as_deref() == event.session_id.as_deref()
                                    || route_session_id.is_none() =>
                            {
                                redirect_collector.observe(&event);
                            }
                            Ok(_) => {}
                            Err(_) => break,
                        }
                    }
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
                    self.mark_lifecycle_phase(LifecyclePhase::EvidenceReady);
                    self.invalidate_observation().await;
                    self.record_audit("navigate", safe_navigation_url(&url));
                    Ok((page, redirect_collector.finish()))
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

    /// Navigate only when the supplied observation revision is still current.
    /// This is the revision-safe counterpart to the compatibility-preserving
    /// [`navigate_with_deadline`](Self::navigate_with_deadline) API.
    pub async fn navigate_with_revision(
        &self,
        url: &str,
        deadline: Duration,
        expected_revision: u64,
    ) -> BrowserResult<NavigationOutcome> {
        self.require_expected_revision(Some(expected_revision))?;
        let previous_revision = self.page_revision.load(Ordering::Relaxed);
        let before = self.page_info().await.ok();
        let normalized_url = normalize_url(url);
        let (page, (redirect_count, redirect_evidence)) = self
            .navigate_with_deadline_and_metadata(url, deadline)
            .await?;
        let current_revision = self.page_revision.load(Ordering::Relaxed);
        let identity =
            navigation_identity(&normalized_url, &page, redirect_count, redirect_evidence);
        let verification = ActionVerificationEvidence {
            revision_delta: current_revision.saturating_sub(previous_revision),
            url_changed: before.as_ref().is_some_and(|before| before.url != page.url),
            title_changed: before
                .as_ref()
                .is_some_and(|before| before.title != page.title),
            target_changed: before
                .as_ref()
                .is_some_and(|before| before.target_id != page.target_id),
            frame_changed: before
                .as_ref()
                .is_some_and(|before| before.frame_id != page.frame_id),
            ..ActionVerificationEvidence::default()
        };
        let mut safe_page = page;
        safe_page.url = safe_navigation_url(&safe_page.url);
        self.mark_lifecycle_phase(LifecyclePhase::ActionVerified);
        Ok(NavigationOutcome {
            status: ActionStatus::Succeeded,
            action: ActionKind::Navigate,
            execution_id: self.next_execution_id(),
            page: safe_page,
            previous_revision,
            current_revision,
            browser_load_completed: true,
            application_ready: false,
            identity,
            verification,
        })
    }
}
