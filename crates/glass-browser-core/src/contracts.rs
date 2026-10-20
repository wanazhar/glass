//! Data contracts shared across browser engines and transport adapters.

use serde::{Deserialize, Serialize, de::Visitor};
use std::error::Error;

pub type BrowserResult<T> = Result<T, Box<dyn Error>>;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Cookie {
    pub name: String,
    pub value: String,
    pub domain: String,
    pub path: String,
    #[serde(default)]
    pub expires: f64,
    #[serde(default, rename = "httpOnly")]
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

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct GeoLocation {
    pub latitude: f64,
    pub longitude: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub accuracy: Option<f64>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct NetworkConditions {
    pub offline: bool,
    pub latency_ms: f64,
    pub download_throughput_bytes: f64,
    pub upload_throughput_bytes: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub connection_type: Option<String>,
}

impl NetworkConditions {
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PendingDialog {
    #[serde(rename = "type")]
    pub dialog_type: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_value: Option<String>,
    pub url: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct DownloadOutcome {
    pub guid: String,
    pub suggested_filename: String,
    pub state: String,
    pub received_bytes: u64,
    pub total_bytes: u64,
    pub target_id: String,
    pub frame_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sha256: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum StorageScope {
    Cookies,
    Local,
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
pub enum StorageOperation {
    Read,
    Write {
        #[serde(deserialize_with = "bounded_string")]
        key: String,
        #[serde(deserialize_with = "bounded_string")]
        value: String,
    },
    Clear,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PromptDecision {
    Accept,
    Dismiss,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PromptResult {
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
