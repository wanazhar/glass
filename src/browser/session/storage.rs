use super::*;
use serde::{Deserialize, Serialize};

/// A browser cookie as returned by CDP `Network.getCookies`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Cookie {
    pub name: String,
    pub value: String,
    pub domain: String,
    pub path: String,
    #[serde(default)]
    pub expires: f64,
    #[serde(default)]
    pub http_only: bool,
    #[serde(default)]
    pub secure: bool,
    #[serde(default, rename = "sameSite")]
    pub same_site: Option<String>,
    #[serde(default, rename = "session")]
    pub is_session: bool,
    #[serde(default)]
    pub size: Option<i64>,
    #[serde(default, rename = "priority")]
    pub priority: Option<String>,
}

/// Key-value snapshot of DOM storage (localStorage or sessionStorage).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageItems {
    pub items: Vec<StorageEntry>,
    #[serde(default)]
    pub truncated: bool,
    pub count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageEntry {
    pub key: String,
    pub value: String,
}

/// Maximum UTF-8 bytes of a single storage value before truncation.
const STORAGE_VALUE_MAX_BYTES: usize = 1024;
/// Maximum number of storage entries returned.
const STORAGE_MAX_ENTRIES: usize = 64;

impl BrowserSession {
    /// Read all browser cookies for the current page URL.
    ///
    /// Uses CDP `Network.getCookies`. Policy-gated: requires
    /// `PersistentProfile` capability.
    pub async fn cookies(&self) -> BrowserResult<Vec<Cookie>> {
        self.policy.require(PolicyCapability::PersistentProfile)?;
        self.cdp
            .with_current_route(async {
                let raw = self.cdp.get_cookies().await?;
                let cookies: Vec<Cookie> = raw["cookies"]
                    .as_array()
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|c| serde_json::from_value(c.clone()).ok())
                            .collect()
                    })
                    .unwrap_or_default();
                Ok(cookies)
            })
            .await
    }

    /// Set browser cookies.
    ///
    /// Each cookie must have at least `name`, `value`, and `domain`.
    /// Uses CDP `Network.setCookies`. Requires `PersistentProfile`.
    pub async fn set_cookies(&self, cookies: &[Cookie]) -> BrowserResult<()> {
        self.policy.require(PolicyCapability::PersistentProfile)?;
        if cookies.is_empty() {
            return Ok(());
        }
        self.cdp
            .with_current_route(async {
                let value = serde_json::to_value(cookies)?;
                self.cdp.set_cookies(value).await?;
                Ok(())
            })
            .await
    }

    /// Clear all browser cookies.
    ///
    /// Uses CDP `Network.clearBrowserCookies`. Requires `PersistentProfile`.
    pub async fn clear_cookies(&self) -> BrowserResult<()> {
        self.policy.require(PolicyCapability::PersistentProfile)?;
        self.cdp
            .with_current_route(async {
                self.cdp.clear_browser_cookies().await?;
                Ok(())
            })
            .await
    }

    /// Read localStorage items for the current page.
    ///
    /// Bounded to 64 entries; each value capped at 1 KiB.
    /// Requires `PersistentProfile`.
    pub async fn local_storage(&self) -> BrowserResult<StorageItems> {
        self.read_dom_storage("localStorage").await
    }

    /// Read sessionStorage items for the current page.
    ///
    /// Bounded to 64 entries; each value capped at 1 KiB.
    /// Requires `PersistentProfile`.
    pub async fn session_storage(&self) -> BrowserResult<StorageItems> {
        self.read_dom_storage("sessionStorage").await
    }

    async fn read_dom_storage(&self, storage_type: &str) -> BrowserResult<StorageItems> {
        self.policy.require(PolicyCapability::PersistentProfile)?;
        let expression = format!(
            r#"JSON.stringify((function() {{
            const store = window.{storage_type};
            if (!store) return JSON.stringify({{items:[], count:0}});
            const keys = Object.keys(store).slice(0, {STORAGE_MAX_ENTRIES});
            const items = keys.map(k => {{
                let v = store.getItem(k) || '';
                if (v.length > {STORAGE_VALUE_MAX_BYTES}) {{
                    v = v.slice(0, {STORAGE_VALUE_MAX_BYTES}) + '…';
                }}
                return {{key: k, value: v}};
            }});
            return JSON.stringify({{
                items: items,
                truncated: Object.keys(store).length > {STORAGE_MAX_ENTRIES},
                count: Object.keys(store).length
            }});
        }})())"#
        );
        self.cdp
            .with_current_route(async {
                let raw = self.cdp.evaluate(&expression).await?;
                let value = runtime_value(&raw)?;
                let json = value
                    .as_str()
                    .ok_or("DOM storage evaluation returned a non-string value")?;
                Ok(serde_json::from_str(json)?)
            })
            .await
    }
}
