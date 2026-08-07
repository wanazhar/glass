//! Conservative advisory page-state classification.
//!
//! Classification is evidence for the next observation only. It is never an
//! authorization to act, and page content may change immediately after it is
//! returned.

use super::{CompactAccessibilitySnapshot, PageInfo};
use serde::{Deserialize, Serialize};

/// Stable, advisory page-state vocabulary.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PageState {
    Normal,
    Loading,
    Challenge,
    Consent,
    AccessDenied,
    LoginRequired,
    Empty,
    Unknown,
}

/// Bounded evidence code supporting a page-state classification.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PageStateEvidence {
    /// Stable machine-readable signal, never page text or a selector.
    pub signal: String,
    /// Short bounded explanation suitable for agent display.
    pub detail: String,
}

/// Advisory next step. Callers must re-observe before any action.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PageStateNextStep {
    Inspect,
    Reobserve,
    Wait,
    ReviewConsent,
    Authenticate,
    InspectChallenge,
}

/// Classification output attached to bounded observations.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PageClassification {
    pub state: PageState,
    /// Conservative confidence label, not an authorization level.
    pub confidence: &'static str,
    /// At most four bounded evidence entries.
    pub evidence: Vec<PageStateEvidence>,
    pub next_step: PageStateNextStep,
}

/// A small landmark projection used as supporting evidence by the classifier.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PageStateLandmark {
    pub role: String,
    pub name: String,
}

const MAX_EVIDENCE: usize = 4;
const MAX_EVIDENCE_BYTES: usize = 160;

/// Classify a page from bounded metadata, visible text, and optional landmarks.
///
/// Challenge and consent states require corroborating evidence to avoid
/// treating ordinary content mentioning verification or cookies as an
/// interstitial. The result is advisory: content can change and callers must
/// obtain a fresh authoritative observation before acting.
pub fn classify_page_state(
    page: &PageInfo,
    visible_text: &str,
    landmarks: &[PageStateLandmark],
) -> PageClassification {
    let title = normalize(&page.title);
    let text = normalize(visible_text);
    let ready = normalize(&page.ready_state);
    let landmark_text = landmarks
        .iter()
        .take(32)
        .map(|landmark| format!("{} {}", landmark.role, landmark.name))
        .collect::<Vec<_>>()
        .join(" ");
    let landmark_text = normalize(&landmark_text);

    let has_password = landmarks.iter().take(32).any(|landmark| {
        landmark.role.eq_ignore_ascii_case("textbox")
            && landmark.name.to_ascii_lowercase().contains("password")
    });
    let has_login_control = landmarks.iter().take(32).any(|landmark| {
        let name = landmark.name.to_ascii_lowercase();
        matches!(
            landmark.role.to_ascii_lowercase().as_str(),
            "button" | "link"
        ) && ["sign in", "log in", "login", "continue"]
            .iter()
            .any(|marker| name.contains(marker))
    });
    let has_consent_control = landmarks.iter().take(32).any(|landmark| {
        let name = landmark.name.to_ascii_lowercase();
        matches!(
            landmark.role.to_ascii_lowercase().as_str(),
            "button" | "link"
        ) && ["accept", "reject", "allow", "cookie", "privacy"]
            .iter()
            .any(|marker| name.contains(marker))
    });

    let challenge_markers = [
        (
            "challenge.just_a_moment",
            contains_any(&title, &text, &["just a moment"]),
        ),
        (
            "challenge.browser_check",
            contains_any(
                &title,
                &text,
                &[
                    "checking your browser",
                    "checking if the site connection is secure",
                ],
            ),
        ),
        (
            "challenge.human_verification",
            contains_any(
                &title,
                &text,
                &[
                    "verify you are human",
                    "verify that you are human",
                    "human verification",
                ],
            ),
        ),
        (
            "challenge.security_verification",
            contains_any(
                &title,
                &text,
                &["performing security verification", "security verification"],
            ),
        ),
        (
            "challenge.javascript_cookies",
            contains_any(
                &title,
                &text,
                &[
                    "enable javascript and cookies",
                    "enable javascript to continue",
                ],
            ),
        ),
        (
            "challenge.attention_required",
            contains_any(&title, &text, &["attention required"]),
        ),
    ];
    let challenge_count = challenge_markers.iter().filter(|(_, found)| *found).count();
    let challenge_supported = challenge_count >= 2
        || (challenge_markers[0].1
            && (title.contains("cloudflare")
                || text.contains("checking your browser")
                || text.contains("security verification")));
    if challenge_supported {
        let evidence = challenge_markers
            .iter()
            .filter(|(_, found)| *found)
            .map(|(signal, _)| evidence(signal, "bounded interstitial marker"))
            .collect();
        return classification(
            PageState::Challenge,
            "high",
            evidence,
            PageStateNextStep::InspectChallenge,
        );
    }

    let consent_marker = contains_any(
        &title,
        &text,
        &[
            "cookie consent",
            "accept cookies",
            "we use cookies",
            "privacy preferences",
            "your privacy choices",
        ],
    );
    if consent_marker && has_consent_control {
        return classification(
            PageState::Consent,
            "high",
            vec![
                evidence("consent.marker", "bounded consent text"),
                evidence("consent.control", "consent landmark"),
            ],
            PageStateNextStep::ReviewConsent,
        );
    }

    let denied_marker = contains_any(
        &title,
        &text,
        &[
            "access denied",
            "forbidden",
            "request blocked",
            "error 403",
            "not authorized",
        ],
    );
    if denied_marker {
        return classification(
            PageState::AccessDenied,
            "high",
            vec![evidence("access_denied.marker", "bounded denial text")],
            PageStateNextStep::Inspect,
        );
    }

    let login_marker = contains_any(
        &title,
        &text,
        &[
            "authentication required",
            "sign in to continue",
            "log in to continue",
            "login required",
        ],
    );
    if login_marker
        || (has_password
            && (has_login_control || text.contains("sign in") || text.contains("log in")))
    {
        return classification(
            PageState::LoginRequired,
            "high",
            vec![evidence("login.marker", "bounded authentication evidence")],
            PageStateNextStep::Authenticate,
        );
    }

    if ready == "loading" || (ready.is_empty() && text.is_empty() && title.is_empty()) {
        return classification(
            PageState::Loading,
            "medium",
            vec![evidence("loading.ready_state", "document is still loading")],
            PageStateNextStep::Wait,
        );
    }

    if text.trim().is_empty() && title.trim().is_empty() && landmarks.is_empty() {
        return classification(
            PageState::Empty,
            "high",
            vec![evidence(
                "empty.no_content",
                "no bounded content or landmarks",
            )],
            PageStateNextStep::Reobserve,
        );
    }

    if !text.trim().is_empty() || !landmark_text.trim().is_empty() || !title.trim().is_empty() {
        return classification(
            PageState::Normal,
            "medium",
            vec![evidence("normal.content", "bounded page content present")],
            PageStateNextStep::Inspect,
        );
    }

    classification(
        PageState::Unknown,
        "low",
        vec![evidence(
            "unknown.insufficient",
            "bounded evidence was inconclusive",
        )],
        PageStateNextStep::Reobserve,
    )
}

/// Classify a compact accessibility observation without exposing its raw tree
/// as classifier evidence.
pub fn classify_compact_observation(
    page: &PageInfo,
    visible_text: &str,
    accessibility: &CompactAccessibilitySnapshot,
) -> PageClassification {
    let landmarks = accessibility
        .roots
        .iter()
        .flat_map(flatten_landmarks)
        .chain(
            accessibility
                .interactive
                .iter()
                .map(|element| PageStateLandmark {
                    role: element.role.clone(),
                    name: element.name.clone(),
                }),
        )
        .take(32)
        .collect::<Vec<_>>();
    classify_page_state(page, visible_text, &landmarks)
}

fn flatten_landmarks(node: &crate::browser::dom::CompactAxNode) -> Vec<PageStateLandmark> {
    let mut result = Vec::new();
    if !node.role.is_empty() || !node.name.is_empty() {
        result.push(PageStateLandmark {
            role: node.role.clone(),
            name: node.name.clone(),
        });
    }
    for child in node.children.iter().take(32) {
        if result.len() >= 32 {
            break;
        }
        result.extend(flatten_landmarks(child));
    }
    result.truncate(32);
    result
}

fn classification(
    state: PageState,
    confidence: &'static str,
    evidence: Vec<PageStateEvidence>,
    next_step: PageStateNextStep,
) -> PageClassification {
    PageClassification {
        state,
        confidence,
        evidence: evidence.into_iter().take(MAX_EVIDENCE).collect(),
        next_step,
    }
}

fn evidence(signal: &str, detail: &str) -> PageStateEvidence {
    PageStateEvidence {
        signal: bounded(signal),
        detail: bounded(detail),
    }
}

fn bounded(value: &str) -> String {
    value.chars().take(MAX_EVIDENCE_BYTES).collect()
}

fn normalize(value: &str) -> String {
    value
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_ascii_lowercase()
}

fn contains_any(title: &str, text: &str, markers: &[&str]) -> bool {
    markers
        .iter()
        .any(|marker| title.contains(marker) || text.contains(marker))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn page(title: &str, ready_state: &str) -> PageInfo {
        PageInfo {
            url: "https://example.test/path".into(),
            title: title.into(),
            ready_state: ready_state.into(),
            target_id: "t".into(),
            frame_id: "f".into(),
        }
    }

    fn landmark(role: &str, name: &str) -> PageStateLandmark {
        PageStateLandmark {
            role: role.into(),
            name: name.into(),
        }
    }

    #[test]
    fn challenge_requires_corroborrating_evidence() {
        let result = classify_page_state(
            &page("Just a moment", "complete"),
            "Checking your browser before accessing",
            &[],
        );
        assert_eq!(result.state, PageState::Challenge);
        assert!(result.evidence.len() <= MAX_EVIDENCE);
    }

    #[test]
    fn generic_verification_copy_is_not_challenge() {
        let result = classify_page_state(
            &page("Account", "complete"),
            "Verify your email address to finish setup",
            &[landmark("button", "Continue")],
        );
        assert_eq!(result.state, PageState::Normal);
    }

    #[test]
    fn normal_and_empty_are_distinguished() {
        assert_eq!(
            classify_page_state(&page("Docs", "complete"), "Welcome to the docs", &[]).state,
            PageState::Normal
        );
        assert_eq!(
            classify_page_state(&page("", "complete"), "", &[]).state,
            PageState::Empty
        );
    }

    #[test]
    fn consent_requires_control_evidence() {
        assert_eq!(
            classify_page_state(
                &page("Store", "complete"),
                "We use cookies to improve the site",
                &[]
            )
            .state,
            PageState::Normal
        );
        assert_eq!(
            classify_page_state(
                &page("Store", "complete"),
                "We use cookies to improve the site",
                &[landmark("button", "Accept cookies")]
            )
            .state,
            PageState::Consent
        );
    }

    #[test]
    fn login_requires_authentication_evidence() {
        assert_eq!(
            classify_page_state(&page("Login required", "complete"), "", &[]).state,
            PageState::LoginRequired
        );
        assert_eq!(
            classify_page_state(
                &page("Account", "complete"),
                "Sign in",
                &[
                    landmark("textbox", "Password"),
                    landmark("button", "Sign in")
                ]
            )
            .state,
            PageState::LoginRequired
        );
    }
}
