use super::*;

const CLIPBOARD_MAX_BYTES: usize = 8192;

impl BrowserSession {
    /// Read text from the system clipboard.
    /// Returns up to 8 KiB of text.
    pub async fn clipboard_read(&self) -> BrowserResult<String> {
        self.cdp.with_current_route(async {
            let _ = self.cdp.send("Browser.grantPermissions", Some(serde_json::json!({
                "permissions": ["clipboardReadWrite"], "origin": "*"
            }))).await;
            let expr = format!(
                "navigator.clipboard.readText().then(t => t.slice(0,{CLIPBOARD_MAX_BYTES})).catch(() => '')"
            );
            let raw = self.cdp.evaluate(&expr).await?;
            let value = runtime_value(&raw)?;
            Ok(value.as_str().unwrap_or("").to_string())
        }).await
    }

    /// Write text to the system clipboard. Truncated to 8 KiB.
    pub async fn clipboard_write(&self, text: &str) -> BrowserResult<()> {
        let bounded = if text.len() > CLIPBOARD_MAX_BYTES {
            &text[..CLIPBOARD_MAX_BYTES]
        } else {
            text
        };
        self.cdp
            .with_current_route(async {
                let _ = self
                    .cdp
                    .send(
                        "Browser.grantPermissions",
                        Some(serde_json::json!({
                            "permissions": ["clipboardReadWrite"], "origin": "*"
                        })),
                    )
                    .await;
                let escaped = bounded.replace('\\', "\\\\").replace('\'', "\\'");
                let expr = format!(
                    "navigator.clipboard.writeText('{escaped}').then(() => true).catch(() => false)"
                );
                self.cdp.evaluate(&expr).await?;
                Ok(())
            })
            .await
    }
}
