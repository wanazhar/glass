//! JavaScript dialog (alert, confirm, prompt) handling.
//!
//! Inspects and dismisses JavaScript dialogs via CDP `Page.javascriptDialogOpening`
//! events. Supports accept and dismiss with optional prompt text.

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
        self.invalidate_observation().await;
        Ok(())
    }

    /// Accept the currently pending JavaScript dialog only when the supplied
    /// observation revision is still current.
    ///
    /// This guarded boundary is used by Task Protocol mutations. The
    /// compatibility-preserving [`accept_dialog`](Self::accept_dialog) method
    /// intentionally remains unguarded.
    pub async fn accept_dialog_with_revision(&self, expected_revision: u64) -> BrowserResult<()> {
        self.cdp
            .with_current_route(async {
                self.require_expected_revision(Some(expected_revision))?;
                self.cdp.handle_javascript_dialog(true).await?;
                Ok::<(), Box<dyn std::error::Error>>(())
            })
            .await?;
        self.invalidate_observation().await;
        Ok(())
    }

    /// Dismiss the currently pending JavaScript dialog only when the supplied
    /// observation revision is still current.
    ///
    /// This guarded boundary is used by Task Protocol mutations. The
    /// compatibility-preserving [`dismiss_dialog`](Self::dismiss_dialog)
    /// method intentionally remains unguarded.
    pub async fn dismiss_dialog_with_revision(&self, expected_revision: u64) -> BrowserResult<()> {
        self.cdp
            .with_current_route(async {
                self.require_expected_revision(Some(expected_revision))?;
                self.cdp.handle_javascript_dialog(false).await?;
                Ok::<(), Box<dyn std::error::Error>>(())
            })
            .await?;
        self.invalidate_observation().await;
        Ok(())
    }

    /// Dismiss (cancel) the currently pending JavaScript dialog.
    ///
    /// Invalidates the observation cache after handling.
    pub async fn dismiss_dialog(&self) -> BrowserResult<()> {
        self.cdp.handle_javascript_dialog(false).await?;
        self.invalidate_observation().await;
        Ok(())
    }
}
