//! Per-session environment state shared by the native owner and its content
//! worker.
//!
//! The native backend must not emulate an environment by changing only one
//! surface. A user-agent override, for example, has to agree in request
//! headers and in `navigator`; network conditions have to reach both top-level
//! navigation and script-owned subresources. These small, serializable types
//! are the single handoff used by those owners.

use super::error::NativeEngineError;
use crate::browser::session::{GeoLocation, NetworkConditions};
use serde::{Deserialize, Serialize};
use std::time::Duration;

pub(crate) const DEFAULT_NATIVE_USER_AGENT: &str = "GlassNative";
pub(crate) const DEFAULT_NATIVE_PLATFORM: &str = "GlassNative";
pub(crate) const DEFAULT_NATIVE_ACCEPT_LANGUAGE: &str = "en-US,en;q=0.9";

const MAX_NATIVE_USER_AGENT_BYTES: usize = 512;
const MAX_NATIVE_ACCEPT_LANGUAGE_BYTES: usize = 128;
const MAX_NATIVE_PLATFORM_BYTES: usize = 128;
const MAX_NATIVE_TIMEZONE_BYTES: usize = 128;
const MAX_NATIVE_LATENCY_MS: f64 = 120_000.0;
const MAX_NATIVE_CPU_THROTTLING_RATE: f64 = 20.0;

fn default_cpu_throttling_rate() -> f64 {
    1.0
}

/// Session-scoped network shaping owned by the native resource loader.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct NativeNetworkConditions {
    pub(crate) offline: bool,
    pub(crate) latency_ms: f64,
    pub(crate) download_throughput_bytes: f64,
    pub(crate) upload_throughput_bytes: f64,
    pub(crate) connection_type: Option<String>,
}

impl Default for NativeNetworkConditions {
    fn default() -> Self {
        Self {
            offline: false,
            latency_ms: 0.0,
            download_throughput_bytes: -1.0,
            upload_throughput_bytes: -1.0,
            connection_type: None,
        }
    }
}

impl NativeNetworkConditions {
    pub(crate) fn from_public(
        conditions: Option<&NetworkConditions>,
    ) -> Result<Self, NativeEngineError> {
        let result = conditions.map_or_else(Self::default, |conditions| Self {
            offline: conditions.offline,
            latency_ms: conditions.latency_ms,
            download_throughput_bytes: conditions.download_throughput_bytes,
            upload_throughput_bytes: conditions.upload_throughput_bytes,
            connection_type: conditions.connection_type.clone(),
        });
        result.validate()?;
        Ok(result)
    }

    pub(crate) fn validate(&self) -> Result<(), NativeEngineError> {
        if !self.latency_ms.is_finite() || !(0.0..=MAX_NATIVE_LATENCY_MS).contains(&self.latency_ms)
        {
            return Err(NativeEngineError::invalid(
                "network latency",
                format!("must be finite and between 0 and {MAX_NATIVE_LATENCY_MS} ms"),
            ));
        }
        for (field, value) in [
            ("download throughput", self.download_throughput_bytes),
            ("upload throughput", self.upload_throughput_bytes),
        ] {
            if !value.is_finite() || (value != -1.0 && value < 0.0) {
                return Err(NativeEngineError::invalid(
                    field,
                    "must be -1 (unlimited) or a non-negative finite byte-per-second rate",
                ));
            }
        }
        if self
            .connection_type
            .as_deref()
            .is_some_and(|value| value.len() > MAX_NATIVE_PLATFORM_BYTES)
        {
            return Err(NativeEngineError::limit(
                "network connection type",
                MAX_NATIVE_PLATFORM_BYTES,
                self.connection_type.as_ref().map_or(0, String::len),
            ));
        }
        Ok(())
    }

    pub(crate) fn request_delay(&self, bytes: usize) -> Duration {
        let latency = Duration::from_secs_f64(self.latency_ms / 1000.0);
        let upload = if self.upload_throughput_bytes > 0.0 {
            Duration::from_secs_f64(bytes as f64 / self.upload_throughput_bytes)
        } else {
            Duration::ZERO
        };
        latency.saturating_add(upload)
    }
}

/// Browser-visible location override passed to the native JavaScript realm.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct NativeGeolocation {
    pub(crate) latitude: f64,
    pub(crate) longitude: f64,
    pub(crate) accuracy: f64,
}

impl NativeGeolocation {
    pub(crate) fn from_public(location: &GeoLocation) -> Result<Self, NativeEngineError> {
        let value = Self {
            latitude: location.latitude,
            longitude: location.longitude,
            accuracy: location.accuracy.unwrap_or(100.0),
        };
        value.validate()?;
        Ok(value)
    }

    fn validate(&self) -> Result<(), NativeEngineError> {
        if !self.latitude.is_finite() || !(-90.0..=90.0).contains(&self.latitude) {
            return Err(NativeEngineError::invalid(
                "geolocation latitude",
                "must be finite and between -90 and 90",
            ));
        }
        if !self.longitude.is_finite() || !(-180.0..=180.0).contains(&self.longitude) {
            return Err(NativeEngineError::invalid(
                "geolocation longitude",
                "must be finite and between -180 and 180",
            ));
        }
        if !self.accuracy.is_finite() || self.accuracy < 0.0 {
            return Err(NativeEngineError::invalid(
                "geolocation accuracy",
                "must be finite and non-negative",
            ));
        }
        Ok(())
    }
}

/// Environment values that must remain consistent across network, script,
/// and content-process surfaces.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct NativeEnvironmentOverrides {
    pub(crate) network: NativeNetworkConditions,
    pub(crate) user_agent: Option<String>,
    pub(crate) accept_language: Option<String>,
    pub(crate) platform: Option<String>,
    pub(crate) geolocation: Option<NativeGeolocation>,
    pub(crate) timezone_id: Option<String>,
    #[serde(default = "default_cpu_throttling_rate")]
    pub(crate) cpu_throttling_rate: f64,
}

impl Default for NativeEnvironmentOverrides {
    fn default() -> Self {
        Self {
            network: NativeNetworkConditions::default(),
            user_agent: None,
            accept_language: None,
            platform: None,
            geolocation: None,
            timezone_id: None,
            cpu_throttling_rate: 1.0,
        }
    }
}

impl NativeEnvironmentOverrides {
    pub(crate) fn validate(&self) -> Result<(), NativeEngineError> {
        self.network.validate()?;
        validate_optional_text(
            "user-agent override",
            self.user_agent.as_deref(),
            MAX_NATIVE_USER_AGENT_BYTES,
        )?;
        validate_optional_text(
            "accept-language override",
            self.accept_language.as_deref(),
            MAX_NATIVE_ACCEPT_LANGUAGE_BYTES,
        )?;
        validate_optional_text(
            "platform override",
            self.platform.as_deref(),
            MAX_NATIVE_PLATFORM_BYTES,
        )?;
        validate_optional_text(
            "timezone override",
            self.timezone_id.as_deref(),
            MAX_NATIVE_TIMEZONE_BYTES,
        )?;
        if !self.cpu_throttling_rate.is_finite()
            || !(0.0..=MAX_NATIVE_CPU_THROTTLING_RATE).contains(&self.cpu_throttling_rate)
            || self.cpu_throttling_rate == 0.0
        {
            return Err(NativeEngineError::invalid(
                "CPU throttling rate",
                format!("must be finite and in (0, {MAX_NATIVE_CPU_THROTTLING_RATE}]"),
            ));
        }
        if let Some(geolocation) = &self.geolocation {
            geolocation.validate()?;
        }
        Ok(())
    }

    pub(crate) fn user_agent(&self) -> &str {
        self.user_agent
            .as_deref()
            .unwrap_or(DEFAULT_NATIVE_USER_AGENT)
    }

    pub(crate) fn platform(&self) -> &str {
        self.platform.as_deref().unwrap_or(DEFAULT_NATIVE_PLATFORM)
    }

    pub(crate) fn accept_language(&self) -> &str {
        self.accept_language
            .as_deref()
            .unwrap_or(DEFAULT_NATIVE_ACCEPT_LANGUAGE)
    }
}

fn validate_optional_text(
    field: &str,
    value: Option<&str>,
    max_bytes: usize,
) -> Result<(), NativeEngineError> {
    let Some(value) = value else {
        return Ok(());
    };
    if value.is_empty() || value.len() > max_bytes || value.contains('\0') {
        return Err(NativeEngineError::invalid(
            field,
            format!("must be non-empty, NUL-free, and at most {max_bytes} bytes"),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{NativeEnvironmentOverrides, NativeNetworkConditions};

    #[test]
    fn default_environment_uses_native_identity() {
        let environment = NativeEnvironmentOverrides::default();
        assert_eq!(environment.user_agent(), "GlassNative");
        assert_eq!(environment.platform(), "GlassNative");
        assert!(!environment.network.offline);
    }

    #[test]
    fn network_conditions_reject_invalid_rates() {
        let conditions = NativeNetworkConditions {
            download_throughput_bytes: -0.1,
            ..Default::default()
        };
        assert!(conditions.validate().is_err());
        let valid = NativeNetworkConditions {
            download_throughput_bytes: 0.0,
            latency_ms: 50.0,
            ..Default::default()
        };
        assert!(valid.validate().is_ok());
    }
}
