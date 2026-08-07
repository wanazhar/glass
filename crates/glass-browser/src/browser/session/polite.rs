//! Opt-in robots.txt and crawl-delay enforcement.
//!
//! URL policy preflight and runtime robots policy are deliberately separate:
//! ordinary URL preflight can allow a URL while this runtime check later
//! denies it because robots.txt is unavailable or disallows the path.

use super::*;
use futures_util::StreamExt;
use std::collections::HashSet;
use std::fmt;

const POLITE_MIN_DELAY: Duration = Duration::from_secs(1);
const POLITE_MAX_DELAY: Duration = Duration::from_secs(30);
const ROBOTS_REQUEST_TIMEOUT: Duration = Duration::from_secs(3);
const MAX_ROBOTS_BODY_BYTES: usize = 128 * 1024;
const MAX_ROBOTS_REDIRECTS: usize = 2;

/// Stable runtime category for a polite-navigation result.
///
/// This is crate-visible so live smoke reporting can match policy outcomes
/// without parsing human-readable error messages.
#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum PoliteNavigationClassification {
    NotApplicable,
    UrlPolicy,
    RobotsUnavailable { status: Option<u16> },
    RobotsStatus { status: u16 },
    RobotsPathDenied,
    CrawlDelayEnforced,
    Unknown,
}

/// Successful runtime result from the polite navigation gate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum PoliteNavigationOutcome {
    NotApplicable,
    Allowed {
        robots_status: u16,
    },
    CrawlDelayEnforced {
        robots_status: u16,
        delay_ms: u64,
        waited_ms: u64,
    },
}

#[allow(dead_code)]
impl PoliteNavigationOutcome {
    pub(crate) const fn classification(self) -> PoliteNavigationClassification {
        match self {
            Self::NotApplicable => PoliteNavigationClassification::NotApplicable,
            Self::Allowed { robots_status } => {
                if robots_status == 404 {
                    PoliteNavigationClassification::RobotsUnavailable {
                        status: Some(robots_status),
                    }
                } else {
                    PoliteNavigationClassification::RobotsStatus {
                        status: robots_status,
                    }
                }
            }
            Self::CrawlDelayEnforced { .. } => PoliteNavigationClassification::CrawlDelayEnforced,
        }
    }
}

/// Typed fail-closed error from the runtime polite-navigation gate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum PoliteNavigationError {
    UrlPolicy { reason: String },
    RobotsStatus { status: u16 },
    RobotsPathDenied { path: String },
    RobotsUnavailable { reason: String },
    RobotsRedirectRejected { reason: String },
    RobotsBodyTooLarge,
}

impl PoliteNavigationError {
    pub(crate) const fn classification(&self) -> PoliteNavigationClassification {
        match self {
            Self::UrlPolicy { .. } => PoliteNavigationClassification::UrlPolicy,
            Self::RobotsStatus { status } => {
                if *status == 404 {
                    PoliteNavigationClassification::RobotsUnavailable {
                        status: Some(*status),
                    }
                } else {
                    PoliteNavigationClassification::RobotsStatus { status: *status }
                }
            }
            Self::RobotsPathDenied { .. } => PoliteNavigationClassification::RobotsPathDenied,
            Self::RobotsUnavailable { .. }
            | Self::RobotsRedirectRejected { .. }
            | Self::RobotsBodyTooLarge => {
                PoliteNavigationClassification::RobotsUnavailable { status: None }
            }
        }
    }
}

impl fmt::Display for PoliteNavigationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UrlPolicy { reason } => write!(formatter, "{reason}"),
            Self::RobotsStatus { status } => {
                let status = reqwest::StatusCode::from_u16(*status)
                    .map(|status| status.to_string())
                    .unwrap_or_else(|_| status.to_string());
                write!(
                    formatter,
                    "polite navigation denied: robots.txt returned {status}"
                )
            }
            Self::RobotsPathDenied { path } => {
                write!(
                    formatter,
                    "polite navigation denied by robots.txt for {path}"
                )
            }
            Self::RobotsUnavailable { .. } => {
                write!(
                    formatter,
                    "polite navigation denied: robots.txt unavailable"
                )
            }
            Self::RobotsRedirectRejected { reason } => {
                write!(
                    formatter,
                    "polite navigation denied: robots.txt redirect rejected: {reason}"
                )
            }
            Self::RobotsBodyTooLarge => {
                write!(
                    formatter,
                    "polite navigation denied: robots.txt body is too large"
                )
            }
        }
    }
}

impl std::error::Error for PoliteNavigationError {}

/// Classify a navigation error without relying on its display string.
pub(crate) fn classify_polite_navigation_error(
    error: &(dyn std::error::Error + 'static),
) -> PoliteNavigationClassification {
    if let Some(error) = error.downcast_ref::<PoliteNavigationError>() {
        return error.classification();
    }
    if error.downcast_ref::<PolicyError>().is_some() {
        return PoliteNavigationClassification::UrlPolicy;
    }
    PoliteNavigationClassification::Unknown
}

impl BrowserSession {
    /// Run the bounded runtime robots check and report its typed outcome.
    ///
    /// The URL policy preflight performed by [`BrowserPolicy::preflight_navigation`]
    /// is advisory and does not replace this fail-closed runtime check.
    pub(crate) async fn polite_navigation_outcome(
        &self,
        url: &str,
    ) -> BrowserResult<PoliteNavigationOutcome> {
        if !self.policy.is_polite() {
            return Ok(PoliteNavigationOutcome::NotApplicable);
        }
        let parsed = url::Url::parse(url).map_err(|_| PoliteNavigationError::UrlPolicy {
            reason: "polite navigation URL is invalid".to_string(),
        })?;
        parsed
            .host_str()
            .ok_or_else(|| PoliteNavigationError::UrlPolicy {
                reason: "polite navigation requires a host".to_string(),
            })?;

        let mut robots_url = parsed.clone();
        // Policy normally rejects credential-bearing navigation URLs first;
        // strip them here as a defense in depth for direct crate callers.
        let _ = robots_url.set_username("");
        let _ = robots_url.set_password(None);
        robots_url.set_path("/robots.txt");
        robots_url.set_query(None);
        robots_url.set_fragment(None);
        let client = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(ROBOTS_REQUEST_TIMEOUT)
            .user_agent(format!(
                "Glass/{} (+https://github.com/wanazhar/glass)",
                env!("CARGO_PKG_VERSION")
            ))
            .build()?;
        let (status, body) = fetch_robots(&client, &parsed, robots_url).await?;
        let rules = RobotsRules::parse(&body);
        let path = parsed.path();
        if rules.disallows(path) {
            return Err(PoliteNavigationError::RobotsPathDenied {
                path: bounded_path(path),
            }
            .into());
        }

        let delay = bounded_crawl_delay(rules.crawl_delay);
        let mut last = self.polite_last_request.lock().await;
        let waited = if let Some(previous) = *last {
            let elapsed = previous.elapsed();
            if elapsed < delay {
                tokio::time::sleep(delay - elapsed).await;
                delay.saturating_sub(elapsed)
            } else {
                Duration::ZERO
            }
        } else {
            Duration::ZERO
        };
        *last = Some(tokio::time::Instant::now());
        if waited.is_zero() {
            Ok(PoliteNavigationOutcome::Allowed {
                robots_status: status,
            })
        } else {
            Ok(PoliteNavigationOutcome::CrawlDelayEnforced {
                robots_status: status,
                delay_ms: delay.as_millis() as u64,
                waited_ms: waited.as_millis() as u64,
            })
        }
    }

    pub(crate) async fn enforce_polite_navigation(&self, url: &str) -> BrowserResult<()> {
        self.polite_navigation_outcome(url).await.map(|_| ())
    }
}
async fn fetch_robots(
    client: &reqwest::Client,
    requested: &url::Url,
    initial: url::Url,
) -> BrowserResult<(u16, String)> {
    let mut current = initial;
    let mut visited = HashSet::new();

    for redirect_count in 0..=MAX_ROBOTS_REDIRECTS {
        if !visited.insert(current.clone()) {
            return Err(PoliteNavigationError::RobotsRedirectRejected {
                reason: "redirect loop detected".to_string(),
            }
            .into());
        }
        let response = client.get(current.clone()).send().await.map_err(|_| {
            PoliteNavigationError::RobotsUnavailable {
                reason: "robots.txt request failed".to_string(),
            }
        })?;
        let status = response.status();
        if matches!(
            status,
            reqwest::StatusCode::MOVED_PERMANENTLY
                | reqwest::StatusCode::FOUND
                | reqwest::StatusCode::SEE_OTHER
                | reqwest::StatusCode::TEMPORARY_REDIRECT
                | reqwest::StatusCode::PERMANENT_REDIRECT
        ) {
            let location = response
                .headers()
                .get(reqwest::header::LOCATION)
                .and_then(|value| value.to_str().ok())
                .ok_or_else(|| PoliteNavigationError::RobotsRedirectRejected {
                    reason: "redirect location is missing or invalid".to_string(),
                })?;
            current =
                next_robots_redirect(requested, &current, location, redirect_count, &visited)?;
            continue;
        }
        let status = status.as_u16();
        let body = if status == reqwest::StatusCode::NOT_FOUND.as_u16() {
            String::new()
        } else if (200..400).contains(&status) {
            read_bounded_body(response).await?
        } else {
            return Err(PoliteNavigationError::RobotsStatus { status }.into());
        };
        return Ok((status, body));
    }
    unreachable!("robots redirect loop always returns")
}

fn resolve_robots_redirect(
    requested: &url::Url,
    current: &url::Url,
    location: &str,
) -> Result<url::Url, PoliteNavigationError> {
    let mut destination =
        current
            .join(location)
            .map_err(|_| PoliteNavigationError::RobotsRedirectRejected {
                reason: "redirect location is invalid".to_string(),
            })?;
    let _ = destination.set_username("");
    let _ = destination.set_password(None);
    destination.set_query(None);
    destination.set_fragment(None);
    if !same_origin(requested, &destination) {
        return Err(PoliteNavigationError::RobotsRedirectRejected {
            reason: "redirect leaves requested origin".to_string(),
        });
    }
    Ok(destination)
}
fn next_robots_redirect(
    requested: &url::Url,
    current: &url::Url,
    location: &str,
    redirect_count: usize,
    visited: &HashSet<url::Url>,
) -> Result<url::Url, PoliteNavigationError> {
    if redirect_count >= MAX_ROBOTS_REDIRECTS {
        return Err(PoliteNavigationError::RobotsRedirectRejected {
            reason: "redirect budget exhausted".to_string(),
        });
    }
    let destination = resolve_robots_redirect(requested, current, location)?;
    if visited.contains(&destination) {
        return Err(PoliteNavigationError::RobotsRedirectRejected {
            reason: "redirect loop detected".to_string(),
        });
    }
    Ok(destination)
}

fn same_origin(left: &url::Url, right: &url::Url) -> bool {
    left.scheme() == right.scheme()
        && left.host_str() == right.host_str()
        && left.port_or_known_default() == right.port_or_known_default()
}

async fn read_bounded_body(response: reqwest::Response) -> BrowserResult<String> {
    let content_length = response.content_length();
    if content_length.is_some_and(|length| length > MAX_ROBOTS_BODY_BYTES as u64) {
        return Err(PoliteNavigationError::RobotsBodyTooLarge.into());
    }
    let mut stream = response.bytes_stream();
    let mut bytes = Vec::with_capacity(
        content_length
            .unwrap_or(0)
            .min(MAX_ROBOTS_BODY_BYTES as u64) as usize,
    );
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|_| PoliteNavigationError::RobotsUnavailable {
            reason: "robots.txt response failed".to_string(),
        })?;
        if bytes.len().saturating_add(chunk.len()) > MAX_ROBOTS_BODY_BYTES {
            return Err(PoliteNavigationError::RobotsBodyTooLarge.into());
        }
        bytes.extend_from_slice(&chunk);
    }
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

fn bounded_path(path: &str) -> String {
    const MAX_PATH_BYTES: usize = 512;
    if path.len() <= MAX_PATH_BYTES {
        return path.to_string();
    }
    let mut end = MAX_PATH_BYTES;
    while !path.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}…", &path[..end])
}

fn bounded_crawl_delay(delay: Duration) -> Duration {
    delay.max(POLITE_MIN_DELAY).min(POLITE_MAX_DELAY)
}

#[derive(Debug, Default)]
struct RobotsRules {
    disallow: Vec<String>,
    crawl_delay: Duration,
}

impl RobotsRules {
    fn parse(body: &str) -> Self {
        let mut rules = Self::default();
        let mut applies = false;
        for raw in body.lines().take(512) {
            let line = raw.split('#').next().unwrap_or_default().trim();
            let Some((key, value)) = line.split_once(':') else {
                continue;
            };
            let key = key.trim().to_ascii_lowercase();
            let value = value.trim();
            match key.as_str() {
                "user-agent" => applies = value == "*" || value.eq_ignore_ascii_case("glass"),
                "disallow" if applies && !value.is_empty() => {
                    if rules.disallow.len() < 64 && value.len() <= 512 {
                        rules.disallow.push(value.to_string());
                    }
                }
                "crawl-delay" if applies => {
                    if let Ok(seconds) = value.parse::<f64>()
                        && seconds.is_finite()
                        && (0.0..=30.0).contains(&seconds)
                    {
                        rules.crawl_delay = Duration::from_secs_f64(seconds);
                    }
                }
                _ => {}
            }
        }
        rules
    }

    fn disallows(&self, path: &str) -> bool {
        self.disallow.iter().any(|prefix| path.starts_with(prefix))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_robots_statuses_without_error_strings() {
        let forbidden = PoliteNavigationError::RobotsStatus { status: 403 };
        assert_eq!(
            forbidden.classification(),
            PoliteNavigationClassification::RobotsStatus { status: 403 }
        );
        let missing = PoliteNavigationOutcome::Allowed { robots_status: 404 };
        assert_eq!(
            missing.classification(),
            PoliteNavigationClassification::RobotsUnavailable { status: Some(404) }
        );
    }

    #[test]
    fn classifies_disallowed_path() {
        let error = PoliteNavigationError::RobotsPathDenied {
            path: "/private/page".into(),
        };
        assert_eq!(
            error.classification(),
            PoliteNavigationClassification::RobotsPathDenied
        );
    }

    #[test]
    fn bounds_crawl_delay_and_classifies_enforcement() {
        assert_eq!(bounded_crawl_delay(Duration::ZERO), POLITE_MIN_DELAY);
        assert_eq!(
            bounded_crawl_delay(Duration::from_secs(120)),
            POLITE_MAX_DELAY
        );
        let outcome = PoliteNavigationOutcome::CrawlDelayEnforced {
            robots_status: 200,
            delay_ms: 30_000,
            waited_ms: 30_000,
        };
        assert_eq!(
            outcome.classification(),
            PoliteNavigationClassification::CrawlDelayEnforced
        );
    }

    #[test]
    fn classifies_url_policy_errors_structurally() {
        let error = PolicyError::Denied {
            operation: "navigate".into(),
            reason: "host is denied".into(),
        };
        assert_eq!(
            classify_polite_navigation_error(&error),
            PoliteNavigationClassification::UrlPolicy
        );
        let robots = PoliteNavigationError::RobotsStatus { status: 403 };
        assert_eq!(
            classify_polite_navigation_error(&robots),
            PoliteNavigationClassification::RobotsStatus { status: 403 }
        );
    }

    #[test]
    fn follows_same_origin_redirect_after_stripping_sensitive_url_parts() {
        let requested = url::Url::parse("https://example.test/start").unwrap();
        let current = url::Url::parse("https://example.test/robots.txt").unwrap();
        let visited = HashSet::from([current.clone()]);
        let destination = next_robots_redirect(
            &requested,
            &current,
            "https://user:secret@example.test/policy?token=redacted#fragment",
            0,
            &visited,
        )
        .unwrap();
        assert_eq!(destination.as_str(), "https://example.test/policy");
        assert!(destination.username().is_empty());
        assert!(destination.password().is_none());
        assert!(destination.query().is_none());
        assert!(destination.fragment().is_none());
    }

    #[test]
    fn rejects_cross_origin_robots_redirects() {
        let requested = url::Url::parse("https://example.test/start").unwrap();
        let current = url::Url::parse("https://example.test/robots.txt").unwrap();
        let error = next_robots_redirect(
            &requested,
            &current,
            "https://other.test/robots.txt",
            0,
            &HashSet::from([current.clone()]),
        )
        .unwrap_err();
        assert!(matches!(
            &error,
            PoliteNavigationError::RobotsRedirectRejected { .. }
        ));
        assert!(error.to_string().contains("leaves requested origin"));
    }

    #[test]
    fn rejects_redirect_loops_and_budget_exhaustion() {
        let requested = url::Url::parse("https://example.test/start").unwrap();
        let current = url::Url::parse("https://example.test/robots.txt").unwrap();
        let visited = HashSet::from([current.clone()]);
        let loop_error =
            next_robots_redirect(&requested, &current, "/robots.txt", 0, &visited).unwrap_err();
        assert!(loop_error.to_string().contains("loop"));

        let budget_error = next_robots_redirect(
            &requested,
            &current,
            "/next.txt",
            MAX_ROBOTS_REDIRECTS,
            &visited,
        )
        .unwrap_err();
        assert!(budget_error.to_string().contains("budget"));
    }

    #[test]
    fn denies_terminal_418_and_403_robots_responses() {
        for status in [418, 403] {
            let error = PoliteNavigationError::RobotsStatus { status };
            assert_eq!(
                error.classification(),
                PoliteNavigationClassification::RobotsStatus { status }
            );
        }
    }

    #[test]
    fn parses_glass_rules_and_bounded_delay() {
        let rules = RobotsRules::parse(
            "User-agent: *\nDisallow: /private\nUser-agent: Glass\nCrawl-delay: 2.5\n",
        );
        assert!(rules.disallows("/private/page"));
        assert!(!rules.disallows("/public"));
        assert_eq!(rules.crawl_delay, Duration::from_secs_f64(2.5));
    }

    #[test]
    fn ignores_other_user_agents_and_malformed_delays() {
        let rules =
            RobotsRules::parse("User-agent: OtherBot\nDisallow: /other\nCrawl-delay: nope\n");
        assert!(!rules.disallows("/other"));
        assert_eq!(rules.crawl_delay, Duration::ZERO);
    }
}
