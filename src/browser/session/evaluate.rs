//! Policy-gated JavaScript evaluation for the active page route.

use super::*;

impl BrowserSession {
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
