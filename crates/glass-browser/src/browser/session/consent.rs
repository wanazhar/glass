//! Bounded consent-wall assistance for common, visible UX frameworks.

use super::*;

/// Result of a consent-wall dismissal attempt.
#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ConsentDismissalOutcome {
    Dismissed,
    NoConsentFound,
    UnrecognizedFramework,
}

const CONSENT_DISMISS_FUNCTION: &str = r#"function() {
    const visible = (el) => {
        if (!el) return false;
        const style = getComputedStyle(el);
        const rect = el.getBoundingClientRect();
        return style.display !== 'none' && style.visibility !== 'hidden' &&
            Number(style.opacity) !== 0 && rect.width > 0 && rect.height > 0;
    };
    const click = (root, selectors) => {
        for (const selector of selectors) {
            const el = root.querySelector(selector);
            if (visible(el)) { el.click(); return true; }
        }
        return false;
    };
    const oneTrust = document.querySelector('#onetrust-banner-sdk, #onetrust-consent-sdk, .onetrust-pc-dark-filter');
    if (oneTrust) {
        return JSON.stringify({framework:'onetrust', dismissed: click(document, [
            '#onetrust-accept-btn-handler', '#onetrust-reject-all-handler'
        ])});
    }
    const cookiebot = document.querySelector('#CybotCookiebotDialog, [data-template="cookiebot"]');
    if (cookiebot) {
        return JSON.stringify({framework:'cookiebot', dismissed: click(cookiebot, [
            '#CybotCookiebotDialogBodyLevelButtonAccept', '#CybotCookiebotDialogBodyButtonDecline'
        ])});
    }
    return JSON.stringify({framework:null, dismissed:false});
}"#;

impl BrowserSession {
    /// Dismiss a visible OneTrust/Cookiebot consent control, if recognized.
    /// This only clicks documented consent controls; it does not bypass bot
    /// protection or search arbitrary page buttons.
    pub async fn dismiss_consent(&self) -> BrowserResult<ConsentDismissalOutcome> {
        self.policy.require(PolicyCapability::ConsentDismissal)?;
        let raw = self.evaluate_value(CONSENT_DISMISS_FUNCTION).await?;
        let value = raw.as_str().unwrap_or_default();
        let parsed: Value = serde_json::from_str(value).unwrap_or_default();
        let outcome = match (
            parsed["framework"].as_str(),
            parsed["dismissed"].as_bool().unwrap_or(false),
        ) {
            (_, true) => ConsentDismissalOutcome::Dismissed,
            (Some("onetrust" | "cookiebot"), false) => {
                ConsentDismissalOutcome::UnrecognizedFramework
            }
            (None, false) => ConsentDismissalOutcome::NoConsentFound,
            _ => ConsentDismissalOutcome::UnrecognizedFramework,
        };
        if matches!(outcome, ConsentDismissalOutcome::Dismissed) {
            self.invalidate_observation().await;
            self.record_audit("dismiss_consent", "recognized_framework");
        }
        Ok(outcome)
    }
}
