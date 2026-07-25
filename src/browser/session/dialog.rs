use super::*;

impl BrowserSession {
    /// Return the currently pending JavaScript dialog content, if any.
    ///
    /// Agents should read this before calling [`accept_dialog`] or
    /// [`dismiss_dialog`] to determine the dialog type, message, and
    /// default value. The dialog is cleared when it is handled or closed.
    pub async fn pending_dialog(&self) -> Option<PendingDialog> {
        self.topology.lock().await.pending_dialog.clone()
    }

    pub async fn accept_dialog(&self) -> BrowserResult<()> {
        self.cdp.handle_javascript_dialog(true).await?;
        self.invalidate_observation();
        Ok(())
    }

    pub async fn dismiss_dialog(&self) -> BrowserResult<()> {
        self.cdp.handle_javascript_dialog(false).await?;
        self.invalidate_observation();
        Ok(())
    }
}
