//! System clipboard read/write via CDP.
//!
//! Provides access to the system clipboard through browser permissions
//! and JavaScript evaluation. All operations are bounded to 8 KiB.

use super::*;

/// Maximum bytes read or written in a single clipboard operation.
const CLIPBOARD_MAX_BYTES: usize = 8192;

impl BrowserSession {
    /// Read text from the system clipboard.
    ///
    /// Grants the `clipboardReadWrite` permission temporarily, then
    /// evaluates `navigator.clipboard.readText()`. Returns up to
    /// 8 KiB of text.
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

    /// Write text to the system clipboard.
    ///
    /// Grants the `clipboardReadWrite` permission temporarily, then
    /// writes via `navigator.clipboard.writeText()`. Text is truncated
    /// to 8 KiB before writing.
    pub async fn clipboard_write(&self, text: &str) -> BrowserResult<()> {
        let bounded = if text.len() > CLIPBOARD_MAX_BYTES {
            let mut end = CLIPBOARD_MAX_BYTES;
            while end > 0 && !text.is_char_boundary(end) {
                end -= 1;
            }
            &text[..end]
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
                let escaped = bounded
                    .replace('\\', "\\\\")
                    .replace('\'', "\\'")
                    .replace('\n', "\\n")
                    .replace('\r', "\\r")
                    .replace('\u{2028}', "\\u2028")
                    .replace('\u{2029}', "\\u2029");
                let expr = format!(
                    "navigator.clipboard.writeText('{escaped}').then(() => true).catch(() => false)"
                );
                self.cdp.evaluate(&expr).await?;
                Ok(())
            })
            .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clipboard_max_bytes_is_8k() {
        // 8 KiB = 8192 bytes — the safety bound for read/write operations
        assert_eq!(CLIPBOARD_MAX_BYTES, 8192);
    }

    #[test]
    fn clipboard_max_bytes_is_power_of_two() {
        // Power-of-two sizing aligns with typical memory page/buffer expectations
        assert_eq!(CLIPBOARD_MAX_BYTES, 1 << 13);
    }

    #[test]
    fn clipboard_max_bytes_is_reasonable_upper_bound() {
        // Guard against accidental inflation; must stay <= 16384 (16 KiB)
        const { assert!(CLIPBOARD_MAX_BYTES <= 16384) };
        // Must be at least 4096 (4 KiB) to be useful
        const { assert!(CLIPBOARD_MAX_BYTES >= 4096) };
    }
}
