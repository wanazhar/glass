//! Browser storage inspection (cookies, localStorage, sessionStorage).
//!
//! Provides read/write access to browser cookies and read access to
//! DOM storage via CDP and JavaScript evaluation. All storage
//! operations require the `PersistentProfile` policy capability.

use super::*;
use serde::{Deserialize, Serialize};

/// A browser cookie as returned by CDP `Network.getCookies`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Cookie {
    /// Cookie name.
    pub name: String,
    /// Cookie value.
    pub value: String,
    /// Domain the cookie belongs to.
    pub domain: String,
    /// Path the cookie is scoped to.
    pub path: String,
    /// Expiration time as a Unix timestamp in seconds.
    #[serde(default)]
    pub expires: f64,
    /// Whether the cookie is HTTP-only (inaccessible to JavaScript).
    #[serde(default)]
    pub http_only: bool,
    /// Whether the cookie is only sent over HTTPS.
    #[serde(default)]
    pub secure: bool,
    /// SameSite attribute: `"Strict"`, `"Lax"`, or `"None"`.
    #[serde(default, rename = "sameSite")]
    pub same_site: Option<String>,
    /// Whether this is a session cookie (deleted when the browser closes).
    #[serde(default, rename = "session")]
    pub is_session: bool,
    /// Approximate size of the cookie in bytes.
    #[serde(default)]
    pub size: Option<i64>,
    /// Cookie priority: `"Low"`, `"Medium"`, or `"High"`.
    #[serde(default, rename = "priority")]
    pub priority: Option<String>,
}

/// Key-value snapshot of DOM storage (localStorage or sessionStorage).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageItems {
    /// The collected key-value entries.
    pub items: Vec<StorageEntry>,
    /// Whether the result was truncated (exceeded 64 entries).
    #[serde(default)]
    pub truncated: bool,
    /// Total number of keys in the storage (may exceed `items.len()`).
    pub count: usize,
}

/// A single key-value pair from DOM storage.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageEntry {
    /// Storage key.
    pub key: String,
    /// Storage value (truncated at 1 KiB).
    pub value: String,
}

/// Maximum UTF-8 bytes of a single storage value before truncation.
const STORAGE_VALUE_MAX_BYTES: usize = 1024;
/// Maximum number of storage entries returned.
const STORAGE_MAX_ENTRIES: usize = 64;
/// Maximum number of cookies read or written by one operation.
const COOKIE_MAX_ENTRIES: usize = 256;

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
                let array = raw["cookies"].as_array().cloned().unwrap_or_default();
                if array.len() > COOKIE_MAX_ENTRIES {
                    return Err(format!(
                        "browser returned more than {COOKIE_MAX_ENTRIES} cookies; narrow the profile before exporting"
                    )
                    .into());
                }
                let cookies: Vec<Cookie> = array
                    .into_iter()
                    .map(serde_json::from_value)
                    .collect::<Result<_, _>>()?;
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
        if cookies.len() > COOKIE_MAX_ENTRIES {
            return Err(
                format!("cookie import exceeds the {COOKIE_MAX_ENTRIES}-cookie limit").into(),
            );
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cookie_deserializes_from_cdp_json() {
        let json = serde_json::json!({
            "name": "session",
            "value": "abc123",
            "domain": ".example.com",
            "path": "/",
            "expires": 1735689600.0,
            "http_only": true,
            "secure": true,
            "sameSite": "Lax",
            "session": false,
            "size": 64,
            "priority": "Medium"
        });
        let cookie: Cookie = serde_json::from_value(json).unwrap();
        assert_eq!(cookie.name, "session");
        assert_eq!(cookie.value, "abc123");
        assert_eq!(cookie.domain, ".example.com");
        assert_eq!(cookie.path, "/");
        assert!((cookie.expires - 1735689600.0).abs() < 1.0);
        assert!(cookie.http_only);
        assert!(cookie.secure);
        assert_eq!(cookie.same_site, Some("Lax".to_string()));
        assert!(!cookie.is_session);
        assert_eq!(cookie.size, Some(64));
        assert_eq!(cookie.priority, Some("Medium".to_string()));
    }

    #[test]
    fn cookie_deserializes_with_minimal_fields() {
        let json = serde_json::json!({
            "name": "minimal",
            "value": "val",
            "domain": "example.com",
            "path": "/"
        });
        let cookie: Cookie = serde_json::from_value(json).unwrap();
        assert_eq!(cookie.name, "minimal");
        assert_eq!(cookie.expires, 0.0);
        assert!(!cookie.http_only);
        assert!(!cookie.secure);
        assert!(cookie.same_site.is_none());
    }

    #[test]
    fn storage_items_deserializes_from_json() {
        let json = serde_json::json!({
            "items": [
                {"key": "token", "value": "eyJhbGciOiJIUzI1NiJ9"},
                {"key": "theme", "value": "dark"}
            ],
            "truncated": false,
            "count": 2
        });
        let items: StorageItems = serde_json::from_value(json).unwrap();
        assert_eq!(items.count, 2);
        assert!(!items.truncated);
        assert_eq!(items.items.len(), 2);
        assert_eq!(items.items[0].key, "token");
        assert_eq!(items.items[1].value, "dark");
    }

    #[test]
    fn storage_items_defaults_truncated_to_false() {
        let json = serde_json::json!({
            "items": [],
            "count": 0
        });
        let items: StorageItems = serde_json::from_value(json).unwrap();
        assert!(!items.truncated);
        assert_eq!(items.count, 0);
        assert!(items.items.is_empty());
    }

    #[test]
    fn storage_entry_serializes_to_json() {
        let entry = StorageEntry {
            key: "session_token".to_string(),
            value: "abc.def.ghi".to_string(),
        };
        let json = serde_json::to_value(&entry).unwrap();
        assert_eq!(json["key"], "session_token");
        assert_eq!(json["value"], "abc.def.ghi");
    }

    #[test]
    fn storage_entry_roundtrip_through_json() {
        let original = StorageEntry {
            key: "theme".to_string(),
            value: "dark".to_string(),
        };
        let json = serde_json::to_string(&original).unwrap();
        let parsed: StorageEntry = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.key, "theme");
        assert_eq!(parsed.value, "dark");
    }

    #[test]
    fn storage_max_entries_is_64() {
        assert_eq!(STORAGE_MAX_ENTRIES, 64);
    }

    #[test]
    fn storage_max_entries_is_reasonable() {
        // Must be at least 1
        const { assert!(STORAGE_MAX_ENTRIES >= 1) };
        // Must be at most 256
        const { assert!(STORAGE_MAX_ENTRIES <= 256) };
    }

    #[test]
    fn storage_max_entries_is_power_of_two() {
        assert_eq!(STORAGE_MAX_ENTRIES.count_ones(), 1);
    }

    #[test]
    fn storage_items_serializes_with_truncated_flag() {
        let items = StorageItems {
            items: vec![StorageEntry {
                key: "k1".to_string(),
                value: "v1".to_string(),
            }],
            truncated: true,
            count: 100,
        };
        let json = serde_json::to_value(&items).unwrap();
        assert_eq!(json["truncated"], true);
        assert_eq!(json["count"], 100);
        assert_eq!(json["items"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn cookie_roundtrip_through_json() {
        let original = Cookie {
            name: "sid".to_string(),
            value: "secret".to_string(),
            domain: "example.com".to_string(),
            path: "/app".to_string(),
            expires: 2000000000.0,
            http_only: true,
            secure: true,
            same_site: Some("Strict".to_string()),
            is_session: false,
            size: Some(32),
            priority: Some("High".to_string()),
        };
        let json = serde_json::to_string(&original).unwrap();
        let parsed: Cookie = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.name, "sid");
        assert_eq!(parsed.value, "secret");
        assert_eq!(parsed.same_site, Some("Strict".to_string()));
        assert!(parsed.http_only);
        assert_eq!(parsed.priority, Some("High".to_string()));
    }

    #[test]
    fn cookie_entry_budget_is_bounded_and_power_of_two() {
        assert_eq!(COOKIE_MAX_ENTRIES, 256);
        assert_eq!(COOKIE_MAX_ENTRIES.count_ones(), 1);
    }
}
