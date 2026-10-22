//! Data contracts shared across browser engines and transport adapters.

use serde::{Deserialize, Serialize, de::Visitor};
use std::error::Error;

/// Result type used by browser adapter operations.
pub type BrowserResult<T> = Result<T, Box<dyn Error>>;

/// Cookie data returned by a browser backend.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Cookie {
    /// Cookie name.
    pub name: String,
    /// Cookie value.
    pub value: String,
    /// Cookie domain.
    pub domain: String,
    /// Cookie path.
    pub path: String,
    /// Expiration time in Unix seconds, or zero for a session cookie.
    #[serde(default)]
    pub expires: f64,
    /// Whether the cookie is restricted to HTTP APIs.
    #[serde(default, rename = "httpOnly")]
    pub http_only: bool,
    /// Whether the cookie is restricted to secure connections.
    #[serde(default)]
    pub secure: bool,
    /// Backend SameSite value, when supplied.
    #[serde(default, rename = "sameSite")]
    pub same_site: Option<String>,
    /// Whether the cookie is limited to the current session.
    #[serde(default, rename = "session")]
    pub is_session: bool,
    /// Serialized cookie size reported by the backend, when available.
    #[serde(default)]
    pub size: Option<i64>,
    /// Backend cookie priority, when supplied.
    #[serde(default, rename = "priority")]
    pub priority: Option<String>,
}

/// Geographic coordinates used by browser geolocation emulation.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct GeoLocation {
    /// Latitude in degrees.
    pub latitude: f64,
    /// Longitude in degrees.
    pub longitude: f64,
    /// Accuracy radius in meters, when supplied.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub accuracy: Option<f64>,
}

/// Network throttling parameters used by browser emulation.
///
/// # Examples
///
/// ```
/// use glass_browser_core::contracts::NetworkConditions;
///
/// let slow = NetworkConditions::preset("slow-3g").unwrap();
/// assert_eq!(slow.latency_ms, 400.0);
/// ```
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct NetworkConditions {
    /// Whether the backend should emulate a disconnected network.
    pub offline: bool,
    /// Added request latency in milliseconds.
    pub latency_ms: f64,
    /// Download rate in bytes per second.
    pub download_throughput_bytes: f64,
    /// Upload rate in bytes per second.
    pub upload_throughput_bytes: f64,
    /// Backend connection type label, when supplied.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub connection_type: Option<String>,
}

impl NetworkConditions {
    /// Return one of the built-in `slow-3g`, `fast-3g`, or `offline` profiles.
    ///
    /// Returns an error when `name` does not match one of those profiles.
    pub fn preset(name: &str) -> BrowserResult<Self> {
        match name {
            "slow-3g" => Ok(Self {
                offline: false,
                latency_ms: 400.0,
                download_throughput_bytes: 50_000.0,
                upload_throughput_bytes: 50_000.0,
                connection_type: Some("cellular3g".into()),
            }),
            "fast-3g" => Ok(Self {
                offline: false,
                latency_ms: 100.0,
                download_throughput_bytes: 1_500_000.0,
                upload_throughput_bytes: 750_000.0,
                connection_type: Some("cellular3g".into()),
            }),
            "offline" => Ok(Self {
                offline: true,
                latency_ms: 0.0,
                download_throughput_bytes: 0.0,
                upload_throughput_bytes: 0.0,
                connection_type: Some("none".into()),
            }),
            _ => Err("network preset must be slow-3g, fast-3g, or offline".into()),
        }
    }
}

/// A JavaScript dialog waiting for a decision from the host application.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PendingDialog {
    /// Dialog kind, serialized as `type`.
    #[serde(rename = "type")]
    pub dialog_type: String,
    /// Text displayed by the dialog.
    pub message: String,
    /// Initial prompt text, when the dialog accepts a value.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_value: Option<String>,
    /// Page URL that opened the dialog.
    pub url: String,
}

/// Final state and byte counts for one browser download.
#[derive(Debug, Clone, Serialize)]
pub struct DownloadOutcome {
    /// Backend identifier for the download.
    pub guid: String,
    /// Suggested output filename.
    pub suggested_filename: String,
    /// Backend download state.
    pub state: String,
    /// Bytes received so far.
    pub received_bytes: u64,
    /// Total bytes when the backend knows the size.
    pub total_bytes: u64,
    /// Page target that initiated the download.
    pub target_id: String,
    /// Frame that initiated the download.
    pub frame_id: String,
    /// SHA-256 digest when the backend calculated one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sha256: Option<String>,
}

/// Browser storage area used by a storage operation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum StorageScope {
    /// Cookies associated with the browser context.
    Cookies,
    /// Persistent local storage.
    Local,
    /// Per-tab session storage.
    Session,
}

struct BoundedStringVisitor<const REQUIRED: bool>;

impl<'de, const REQUIRED: bool> Visitor<'de> for BoundedStringVisitor<REQUIRED> {
    type Value = String;

    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("a bounded UTF-8 string")
    }

    fn visit_borrowed_str<E: serde::de::Error>(self, value: &'de str) -> Result<String, E> {
        self.visit_str(value)
    }

    fn visit_str<E: serde::de::Error>(self, value: &str) -> Result<String, E> {
        if REQUIRED && value.is_empty() {
            return Err(E::custom("string must not be empty"));
        }
        if value.len() > crate::limits::MAX_TEXT_BYTES {
            return Err(E::custom("string exceeds the bounded payload size"));
        }
        Ok(value.to_owned())
    }

    fn visit_string<E: serde::de::Error>(self, value: String) -> Result<String, E> {
        if REQUIRED && value.is_empty() {
            return Err(E::custom("string must not be empty"));
        }
        if value.len() > crate::limits::MAX_TEXT_BYTES {
            return Err(E::custom("string exceeds the bounded payload size"));
        }
        Ok(value)
    }
}

fn bounded_string<'de, D: serde::Deserializer<'de>>(deserializer: D) -> Result<String, D::Error> {
    deserializer.deserialize_str(BoundedStringVisitor::<true>)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
/// Operation to perform on browser storage.
pub enum StorageOperation {
    /// Read the selected storage area.
    Read,
    /// Write one key/value pair. Deserialization requires non-empty strings
    /// that fit within [`crate::limits::MAX_TEXT_BYTES`].
    Write {
        /// Storage key.
        #[serde(deserialize_with = "bounded_string")]
        key: String,
        /// Value to store.
        #[serde(deserialize_with = "bounded_string")]
        value: String,
    },
    /// Clear the selected storage area.
    Clear,
}

/// Decision returned by the host for a browser prompt.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PromptDecision {
    /// Accept the dialog's default or entered value.
    Accept,
    /// Dismiss the dialog.
    Dismiss,
}

/// Result of delivering a host decision to a browser prompt.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PromptResult {
    /// Whether the host handled a pending prompt.
    pub handled: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn storage_write_rejects_empty_key_and_value() {
        assert!(
            serde_json::from_str::<StorageOperation>(r#"{"write":{"key":"","value":"x"}}"#)
                .is_err()
        );
        assert!(
            serde_json::from_str::<StorageOperation>(r#"{"write":{"key":"x","value":""}}"#)
                .is_err()
        );
    }

    #[test]
    fn storage_write_rejects_over_limit_key_and_value() {
        let too_long = "x".repeat(crate::limits::MAX_TEXT_BYTES + 1);
        let key = serde_json::json!({"write":{"key":too_long,"value":"x"}});
        let value = serde_json::json!({"write":{"key":"x","value":too_long}});
        assert!(serde_json::from_value::<StorageOperation>(key).is_err());
        assert!(serde_json::from_value::<StorageOperation>(value).is_err());
    }
}
