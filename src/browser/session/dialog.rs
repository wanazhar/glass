use super::*;

impl BrowserSession {
    /// Return the currently pending JavaScript dialog content, if any.
    ///
    /// Agents should read this before calling `accept_dialog` or
    /// `dismiss_dialog` to determine the dialog type, message, and
    /// default value. The dialog is cleared when it is handled or closed.
    pub async fn pending_dialog(&self) -> Option<PendingDialog> {
        self.topology.lock().await.pending_dialog.clone()
    }

    /// Accept (confirm) the currently pending JavaScript dialog.
    ///
    /// For `prompt` dialogs, the default prompt value is submitted.
    /// Invalidates the observation cache after handling.
    pub async fn accept_dialog(&self) -> BrowserResult<()> {
        self.cdp.handle_javascript_dialog(true).await?;
        self.invalidate_observation();
        Ok(())
    }

    /// Dismiss (cancel) the currently pending JavaScript dialog.
    ///
    /// Invalidates the observation cache after handling.
    pub async fn dismiss_dialog(&self) -> BrowserResult<()> {
        self.cdp.handle_javascript_dialog(false).await?;
        self.invalidate_observation();
        Ok(())
    }
}
