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
        cdp.send_to_session(
            &session_id,
            "Fetch.enable",
            Some(serde_json::json!({
                "patterns": [fetch_request_pattern(pattern)]
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

fn fetch_request_pattern(pattern: &RequestPattern) -> serde_json::Value {
    let mut value = serde_json::json!({
        "urlPattern": pattern.url_pattern,
        "requestStage": pattern.request_stage
    });
    if let Some(resource_type) = &pattern.resource_type {
        value["resourceType"] = serde_json::Value::String(resource_type.clone());
    }
    value
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_pattern_default_matches_all_urls() {
        let pattern = RequestPattern::default();
        assert_eq!(pattern.url_pattern, "*");
        assert_eq!(pattern.request_stage, "Request");
        assert!(pattern.resource_type.is_none());
        let payload = fetch_request_pattern(&pattern);
        assert_eq!(payload["urlPattern"], "*");
        assert_eq!(payload["requestStage"], "Request");
        assert!(payload.get("resourceType").is_none());
    }

    #[test]
    fn fetch_pattern_includes_explicit_resource_type() {
        let payload = fetch_request_pattern(&RequestPattern {
            url_pattern: "https://example.test/*".into(),
            resource_type: Some("XHR".into()),
            request_stage: "Response".into(),
        });
        assert_eq!(payload["resourceType"], "XHR");
        assert_eq!(payload["requestStage"], "Response");
    }

    #[test]
    fn request_pattern_custom_resource_type() {
        let pattern = RequestPattern {
            url_pattern: "https://example.com/*".to_string(),
            resource_type: Some("XHR".to_string()),
            request_stage: "Response".to_string(),
        };
        assert_eq!(pattern.url_pattern, "https://example.com/*");
        assert_eq!(pattern.resource_type.as_deref(), Some("XHR"));
        assert_eq!(pattern.request_stage, "Response");
    }

    #[test]
    fn request_pattern_is_cloneable() {
        let pattern = RequestPattern {
            url_pattern: "/api/*".to_string(),
            resource_type: Some("Fetch".to_string()),
            request_stage: "Request".to_string(),
        };
        let cloned = pattern.clone();
        assert_eq!(cloned.url_pattern, "/api/*");
        assert_eq!(cloned.resource_type.as_deref(), Some("Fetch"));
        assert_eq!(cloned.request_stage, "Request");
        // Verify original is unmodified
        assert_eq!(pattern.url_pattern, "/api/*");
    }

    #[test]
    fn request_pattern_debug_output_includes_fields() {
        let pattern = RequestPattern::default();
        let debug = format!("{:?}", pattern);
        assert!(debug.contains("*"));
        assert!(debug.contains("Request"));
    }
}
