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
    const named = [...document.querySelectorAll('[role=dialog],[role=alertdialog],[role=banner],dialog,[id],[class],[aria-label]')].filter(el => {
        const label = ['id', 'class', 'aria-label'].map(name => el.getAttribute(name) || '').join(' ');
        const role = el.getAttribute('role');
        return /cookie|consent|gdpr/i.test(label) &&
            (el.matches('dialog') || role === 'dialog' || role === 'alertdialog' || role === 'banner' ||
             /(?:cookie|consent|gdpr)[-_ ]*(?:banner|dialog|notice)|(?:banner|dialog|notice)[-_ ]*(?:cookie|consent|gdpr)/i.test(label));
    });
    const accept = /^(accept|accept all|allow all|i agree|agree|got it|ok|okay|reject all|decline|(?:accept(?: all)? cookies|allow(?: all)? cookies|reject(?: all)? cookies)(?: and continue)?)$/i;
    for (const box of named.filter(visible).slice(0, 20)) {
        const label = ['id', 'class', 'aria-label'].map(name => box.getAttribute(name) || '').join(' ');
        const context = box.cloneNode(true);
        context.querySelectorAll('button,a,[role=button],input,script,style').forEach(el => el.remove());
        if (!/cookie/i.test(label) && !/\bcookies?\b/i.test(context.textContent || '')) continue;
        const button = [...box.querySelectorAll('button,a,[role=button],input[type=button],input[type=submit]')]
            .filter(visible)
            .find(b => accept.test((b.innerText || b.value || '').trim()));
        if (button) { button.click(); return JSON.stringify({framework:'generic', dismissed:true}); }
    }
    return JSON.stringify({framework:null, dismissed:false});
}"#;

impl CdpBrowserSession {
    pub(crate) fn consent_dismiss_expression() -> String {
        format!("({CONSENT_DISMISS_FUNCTION})()")
    }

    /// Dismiss a visible OneTrust/Cookiebot consent control, if recognized.
    /// This only clicks documented consent controls, or an accept/reject
    /// button inside an element named as a cookie or consent banner; it does
    /// not bypass bot protection or search arbitrary page buttons.
    pub async fn dismiss_consent(&self) -> BrowserResult<ConsentDismissalOutcome> {
        self.policy.require(PolicyCapability::ConsentDismissal)?;
        let raw = self
            .evaluate_value(&Self::consent_dismiss_expression())
            .await?;
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
