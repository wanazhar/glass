//! CDP Fetch domain request interception.
//!
//! Provides scoped network request interception via the CDP `Fetch`
//! domain. Use [`BrowserSession::intercept_request`] to enable
//! interception with a [`RequestPattern`] and obtain an
//! [`InterceptGuard`] that disables interception on drop.
//!
//! This is distinct from the built-in policy Fetch interception used
//! for URL filtering.

use super::*;

/// Pattern for intercepting network requests via CDP Fetch domain.
#[derive(Debug, Clone)]
pub struct RequestPattern {
    /// Glob pattern or exact URL to match. "*" matches all URLs.
    pub url_pattern: String,
    /// Optional resource type filter (e.g. "Document", "XHR", "Script").
    /// When None, all resource types are intercepted.
    pub resource_type: Option<String>,
    /// When to intercept: "Request" or "Response". Defaults to "Request".
    pub request_stage: String,
}

impl Default for RequestPattern {
    fn default() -> Self {
        Self {
            url_pattern: "*".to_string(),
            resource_type: None,
            request_stage: "Request".to_string(),
        }
    }
}

/// Scoped lease that enables CDP Fetch domain interception.
///
/// While this guard is alive, the Fetch domain is enabled for the
/// active session with the configured patterns. Paused requests
/// fire Fetch.requestPaused events observable via diagnostics.
///
/// On drop, Fetch is disabled for the session.
pub struct InterceptGuard {
    cdp: CdpClient,
    session_id: String,
    armed: bool,
}

impl InterceptGuard {
    /// Enable the Fetch domain and begin interception with the given pattern.
    async fn enable(cdp: CdpClient, pattern: &RequestPattern) -> BrowserResult<Self> {
        let session_id = cdp
            .current_session_id()
            .ok_or("intercept requires an active page session")?;
        let resource_type = pattern.resource_type.as_deref().unwrap_or("*");
        cdp.send_to_session(
            &session_id,
            "Fetch.enable",
            Some(serde_json::json!({
                "patterns": [{
                    "urlPattern": pattern.url_pattern,
                    "resourceType": resource_type,
                    "requestStage": pattern.request_stage
                }]
            })),
        )
        .await?;
        Ok(Self {
            cdp,
            session_id,
            armed: true,
        })
    }

    /// Manually disable interception before the guard drops.
    pub async fn disable(mut self) -> BrowserResult<()> {
        self.armed = false;
        disable_fetch_for(&self.cdp, Some(&self.session_id)).await
    }
}

impl Drop for InterceptGuard {
    fn drop(&mut self) {
        if self.armed {
            let cdp = self.cdp.clone();
            let session_id = self.session_id.clone();
            tokio::spawn(async move {
                let _ = disable_fetch_for(&cdp, Some(&session_id)).await;
            });
        }
    }
}

impl BrowserSession {
    /// Enable CDP Fetch domain interception for the active page session.
    ///
    /// Returns a scoped guard that disables interception on drop.
    /// While active, matching requests fire Fetch.requestPaused events.
    /// Distinct from the built-in policy Fetch interception.
    pub async fn intercept_request(
        &self,
        pattern: &RequestPattern,
    ) -> BrowserResult<InterceptGuard> {
        self.cdp
            .with_current_route(async { InterceptGuard::enable(self.cdp.clone(), pattern).await })
            .await
    }
}
