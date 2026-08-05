//! Page navigation and JavaScript evaluation.
//!
//! Navigates the active page target to a URL with configurable timeouts,
//! and evaluates JavaScript expressions in the page context.

use super::*;
use crate::browser::cdp::CdpEventWithParams;
use url::Url;

const NAVIGATION_WAIT_EVIDENCE_RESERVE: Duration = Duration::from_millis(50);

struct NavigationCollectorTask(Option<tokio::task::JoinHandle<()>>);

impl NavigationCollectorTask {
    fn new(task: tokio::task::JoinHandle<()>) -> Self {
        Self(Some(task))
    }

    async fn stop(mut self) {
        if let Some(task) = self.0.take() {
            task.abort();
            let _ = task.await;
        }
    }
}

impl Drop for NavigationCollectorTask {
    fn drop(&mut self) {
        if let Some(task) = self.0.take() {
            task.abort();
        }
    }
}

pub(crate) struct NavigationRedirectCollector {
    requested_url: String,
    frame_id: Option<String>,
    request_id: Option<String>,
    redirect_count: Option<u16>,
}

/// Bounded readiness status for a navigation execution.
#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum NavigationReadinessStatus {
    Complete,
    Partial,
}

/// Phase reached by a bounded navigation execution.
#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum NavigationReadinessPhase {
    Document,
    Lifecycle,
}

/// Readiness evidence collected while navigating.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct NavigationReadiness {
    pub status: NavigationReadinessStatus,
    pub phase: NavigationReadinessPhase,
    pub lifecycle_complete: bool,
    pub timeout_ms: u64,
}

/// Bounded navigation metadata for crate-internal smoke and diagnostics.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct NavigationExecution {
    pub page: PageInfo,
    pub redirect_count: Option<u16>,
    pub redirect_evidence: NavigationRedirectEvidence,
    pub readiness: NavigationReadiness,
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
    pub(crate) fn set_frame_id(&mut self, frame_id: Option<&str>) {
        if self.frame_id.as_deref() != frame_id {
            self.request_id = None;
            self.redirect_count = None;
        }
        self.frame_id = frame_id.map(str::to_string);
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
    fn bounded_partial_navigation_page(mut page: PageInfo) -> PageInfo {
        page.url = truncate_utf8_bytes(&safe_navigation_url(&page.url), BOOTSTRAP_URL_MAX_BYTES);
        page.title = truncate_utf8_bytes(&page.title, BOOTSTRAP_TITLE_MAX_BYTES);
        page.ready_state = truncate_utf8_bytes(&page.ready_state, BOOTSTRAP_TITLE_MAX_BYTES);
        page.target_id = bounded_topology_id(&page.target_id);
        page.frame_id = bounded_topology_id(&page.frame_id);
        page
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
        let execution = self
            .navigate_with_deadline_and_readiness(url, deadline)
            .await?;
        if !execution.readiness.lifecycle_complete {
            return Err(wait_timeout("lifecycle", deadline, "lifecycle_incomplete").into());
        }
        Ok((
            execution.page,
            (execution.redirect_count, execution.redirect_evidence),
        ))
    }

    /// Navigate with bounded lifecycle and page-readiness evidence.
    ///
    /// A partial result is returned only when the navigation command succeeds,
    /// the lifecycle wait reaches its deadline, and a bounded page-info probe
    /// succeeds. It is advisory evidence and does not verify an action.
    pub(crate) async fn navigate_with_deadline_and_readiness(
        &self,
        url: &str,
        deadline: Duration,
    ) -> BrowserResult<NavigationExecution> {
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
                    let started = tokio::time::Instant::now();
                    let mut events = self.cdp.subscribe_events();
                    let payload_events = self.cdp.subscribe_events_with_params();
                    let route_session_id = self.cdp.current_session_id();
                    let remaining = deadline.saturating_sub(started.elapsed());
                    tokio::time::timeout(remaining, self.cdp.enable_network())
                        .await
                        .map_err(|_| {
                            wait_timeout("lifecycle", deadline, "enable_network_pending")
                        })??;
                    self.mark_lifecycle_phase(LifecyclePhase::NavigationStarted);

                    let remaining = deadline.saturating_sub(started.elapsed());
                    let initial_frame_id = tokio::time::timeout(remaining, async {
                        self.topology.lock().await.active_frame_id.clone()
                    })
                    .await
                    .ok()
                    .flatten();
                    let redirect_collector = Arc::new(tokio::sync::Mutex::new(
                        NavigationRedirectCollector::new(&url, initial_frame_id.as_deref()),
                    ));
                    let collector_for_task = Arc::clone(&redirect_collector);
                    let collector_task = NavigationCollectorTask::new(tokio::spawn(async move {
                        let mut payload_events = payload_events;
                        loop {
                            match payload_events.recv().await {
                                Ok(event)
                                    if route_session_id.as_deref()
                                        == event.session_id.as_deref()
                                        || route_session_id.is_none() =>
                                {
                                    collector_for_task.lock().await.observe(&event);
                                }
                                Ok(_) => {}
                                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {}
                                Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                            }
                        }
                    }));

                    let remaining = deadline.saturating_sub(started.elapsed());
                    let navigation =
                        match tokio::time::timeout(remaining, self.cdp.navigate(&url)).await {
                            Err(_) => {
                                collector_task.stop().await;
                                return Err(wait_timeout(
                                    "lifecycle",
                                    deadline,
                                    "navigate_command_pending",
                                )
                                .into());
                            }
                            Ok(Err(error)) => {
                                collector_task.stop().await;
                                return Err(error.into());
                            }
                            Ok(Ok(navigation)) => navigation,
                        };

                    if let Some(frame_id) = navigation.frame_id.as_deref() {
                        if let Err(error) = validate_topology_id(frame_id) {
                            collector_task.stop().await;
                            return Err(error);
                        }
                        let remaining = deadline.saturating_sub(started.elapsed());
                        if tokio::time::timeout(remaining, async {
                            self.topology.lock().await.active_frame_id = Some(frame_id.to_string());
                        })
                        .await
                        .is_err()
                        {
                            collector_task.stop().await;
                            return Err(wait_timeout(
                                "lifecycle",
                                deadline,
                                "topology_update_pending",
                            )
                            .into());
                        }
                        self.cdp
                            .set_active_frame_context(Some(frame_id.to_string()), None);
                        let remaining = deadline.saturating_sub(started.elapsed());
                        if tokio::time::timeout(remaining, async {
                            redirect_collector.lock().await.set_frame_id(Some(frame_id));
                        })
                        .await
                        .is_err()
                        {
                            collector_task.stop().await;
                            return Err(wait_timeout(
                                "lifecycle",
                                deadline,
                                "redirect_evidence_pending",
                            )
                            .into());
                        }
                    }

                    let remaining = deadline.saturating_sub(started.elapsed());
                    let wait_budget = remaining.saturating_sub(NAVIGATION_WAIT_EVIDENCE_RESERVE);
                    let wait_result = if wait_budget.is_zero() {
                        Err(wait_timeout("lifecycle", deadline, "lifecycle_pending").into())
                    } else {
                        match tokio::time::timeout(
                            remaining,
                            self.wait_loop(
                                WaitCondition::Lifecycle("complete".to_string()),
                                wait_budget,
                                deadline,
                                &mut events,
                                true,
                            ),
                        )
                        .await
                        {
                            Ok(result) => result,
                            Err(_) => {
                                Err(wait_timeout("lifecycle", deadline, "lifecycle_pending").into())
                            }
                        }
                    };

                    let mut observed_page = None;
                    let lifecycle_complete = match wait_result {
                        Ok(_) => true,
                        Err(error) => {
                            if let Some(timeout) = error.downcast_ref::<WaitTimeout>() {
                                observed_page = timeout.observed_page.clone();
                                false
                            } else {
                                collector_task.stop().await;
                                return Err(error);
                            }
                        }
                    };

                    collector_task.stop().await;
                    let redirect_collector = Arc::try_unwrap(redirect_collector)
                        .map_err(|_| "navigation redirect collector still in use")?
                        .into_inner();
                    let (redirect_count, redirect_evidence) = redirect_collector.finish();

                    let page = if lifecycle_complete {
                        let remaining = deadline.saturating_sub(started.elapsed());
                        let frames = tokio::time::timeout(remaining, self.list_frames())
                            .await
                            .map_err(|_| {
                            wait_timeout("lifecycle", deadline, "frames_pending")
                        })??;
                        let main_frame = frames
                            .into_iter()
                            .find(|frame| frame.parent_id.is_none())
                            .ok_or("navigated target returned no main frame")?;
                        let remaining = deadline.saturating_sub(started.elapsed());
                        tokio::time::timeout(remaining, self.select_frame(&main_frame.id))
                            .await
                            .map_err(|_| wait_timeout("lifecycle", deadline, "frame_pending"))??;
                        let remaining = deadline.saturating_sub(started.elapsed());
                        tokio::time::timeout(remaining, self.page_info())
                            .await
                            .map_err(|_| {
                                wait_timeout("lifecycle", deadline, "page_info_pending")
                            })??
                    } else if let Some(page) = observed_page {
                        page
                    } else {
                        let remaining = deadline.saturating_sub(started.elapsed());
                        if remaining.is_zero() {
                            return Err(
                                wait_timeout("lifecycle", deadline, "page_info_pending").into()
                            );
                        }
                        tokio::time::timeout(remaining, self.page_info())
                            .await
                            .map_err(|_| {
                                wait_timeout("lifecycle", deadline, "page_info_pending")
                            })??
                    };

                    self.mark_lifecycle_phase(LifecyclePhase::EvidenceReady);
                    let page = if lifecycle_complete {
                        page
                    } else {
                        Self::bounded_partial_navigation_page(page)
                    };

                    self.invalidate_observation().await;
                    self.record_audit("navigate", safe_navigation_url(&url));
                    Ok(NavigationExecution {
                        page,
                        redirect_count,
                        redirect_evidence,
                        readiness: NavigationReadiness {
                            status: if lifecycle_complete {
                                NavigationReadinessStatus::Complete
                            } else {
                                NavigationReadinessStatus::Partial
                            },
                            phase: if lifecycle_complete {
                                NavigationReadinessPhase::Lifecycle
                            } else {
                                NavigationReadinessPhase::Document
                            },
                            lifecycle_complete,
                            timeout_ms: deadline.as_millis() as u64,
                        },
                    })
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
