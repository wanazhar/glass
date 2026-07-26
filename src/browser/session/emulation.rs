//! Browser emulation controls via CDP.
//!
//! This module exposes page PDF generation and environment overrides
//! (geolocation, timezone) through the [`BrowserSession`](super::BrowserSession)
//! extension methods.

use super::*;

/// Options for page PDF generation via CDP `Page.printToPDF`.
///
/// All fields are optional; omitted fields use the browser default.
/// Use [`PdfOptions::letter`] for a US Letter preset with background printing.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct PdfOptions {
    /// Paper width in inches.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub paper_width: Option<f64>,
    /// Paper height in inches.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub paper_height: Option<f64>,
    /// Whether to print background graphics.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub print_background: Option<bool>,
    /// Scale factor (1.0 = 100%).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scale: Option<f64>,
    /// Whether to display header and footer.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display_header_footer: Option<bool>,
    /// Top margin in inches.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub margin_top: Option<f64>,
    /// Bottom margin in inches.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub margin_bottom: Option<f64>,
    /// Left margin in inches.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub margin_left: Option<f64>,
    /// Right margin in inches.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub margin_right: Option<f64>,
}

impl PdfOptions {
    /// US Letter (8.5×11″) with background printing enabled.
    pub fn letter() -> Self {
        Self {
            paper_width: Some(8.5),
            paper_height: Some(11.0),
            print_background: Some(true),
            ..Default::default()
        }
    }
}

/// Geolocation coordinates for CDP emulation.
///
/// Pass to [`BrowserSession::set_geolocation`] to override the
/// browser's reported position.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct GeoLocation {
    /// Latitude in decimal degrees (-90 to 90).
    pub latitude: f64,
    /// Longitude in decimal degrees (-180 to 180).
    pub longitude: f64,
    /// Accuracy radius in meters. Defaults to 100 when omitted.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub accuracy: Option<f64>,
}

/// Named or explicit network emulation settings for one session.
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
                connection_type: Some("cellular3g".to_string()),
            }),
            "fast-3g" => Ok(Self {
                offline: false,
                latency_ms: 100.0,
                download_throughput_bytes: 1_500_000.0,
                upload_throughput_bytes: 750_000.0,
                connection_type: Some("cellular3g".to_string()),
            }),
            "offline" => Ok(Self {
                offline: true,
                latency_ms: 0.0,
                download_throughput_bytes: 0.0,
                upload_throughput_bytes: 0.0,
                connection_type: Some("none".to_string()),
            }),
            _ => Err("network preset must be slow-3g, fast-3g, or offline".into()),
        }
    }
}

impl BrowserSession {
    /// Apply session-scoped network throttling. Call with `None` to reset.
    pub async fn set_network_conditions(
        &self,
        conditions: Option<&NetworkConditions>,
    ) -> BrowserResult<()> {
        self.cdp
            .with_current_route(async {
                let params = if let Some(conditions) = conditions {
                    serde_json::json!({
                        "offline": conditions.offline,
                        "latency": conditions.latency_ms,
                        "downloadThroughput": conditions.download_throughput_bytes,
                        "uploadThroughput": conditions.upload_throughput_bytes,
                        "connectionType": conditions.connection_type.clone().unwrap_or_else(|| "none".to_string())
                    })
                } else {
                    serde_json::json!({
                        "offline": false,
                        "latency": 0,
                        "downloadThroughput": -1,
                        "uploadThroughput": -1,
                        "connectionType": "none"
                    })
                };
                self.cdp.send("Network.emulateNetworkConditions", Some(params)).await?;
                Ok(())
            })
            .await
    }

    /// Apply a session-scoped CPU throttling multiplier. `None` resets to 1.
    pub async fn set_cpu_throttling(&self, rate: Option<f64>) -> BrowserResult<()> {
        let rate = rate.unwrap_or(1.0);
        if !rate.is_finite() || rate <= 0.0 || rate > 20.0 {
            return Err("CPU throttling rate must be in (0, 20]".into());
        }
        self.cdp
            .with_current_route(async {
                self.cdp
                    .send(
                        "Emulation.setCPUThrottlingRate",
                        Some(serde_json::json!({"rate": rate})),
                    )
                    .await?;
                Ok(())
            })
            .await
    }

    /// Override the user agent and optional Accept-Language for this session.
    /// Passing `None` restores Chrome's default user agent.
    pub async fn set_user_agent(
        &self,
        user_agent: Option<&str>,
        accept_language: Option<&str>,
        platform: Option<&str>,
    ) -> BrowserResult<()> {
        if user_agent.is_some_and(|value| value.len() > 512)
            || accept_language.is_some_and(|value| value.len() > 128)
            || platform.is_some_and(|value| value.len() > 128)
        {
            return Err("user-agent override is too long".into());
        }
        let restore = user_agent.is_none();
        let original = if restore {
            self.user_agent_original.lock().await.clone()
        } else {
            let existing = self.user_agent_original.lock().await.clone();
            if let Some(existing) = existing {
                Some(existing)
            } else {
                let current = self
                    .evaluate_value("navigator.userAgent")
                    .await?
                    .as_str()
                    .unwrap_or_default()
                    .to_string();
                *self.user_agent_original.lock().await = Some(current.clone());
                Some(current)
            }
        };
        if restore && original.is_none() {
            return Ok(());
        }
        self.cdp
            .with_current_route(async {
                let user_agent = user_agent.or(original.as_deref()).unwrap_or_default();
                let mut params = serde_json::json!({"userAgent": user_agent});
                if let Some(language) = accept_language {
                    params["acceptLanguage"] = serde_json::Value::String(language.to_string());
                }
                if let Some(platform) = platform {
                    params["platform"] = serde_json::Value::String(platform.to_string());
                }
                self.cdp
                    .send("Network.setUserAgentOverride", Some(params))
                    .await?;
                Ok::<(), crate::browser::cdp::CdpError>(())
            })
            .await?;
        if restore {
            *self.user_agent_original.lock().await = None;
        }
        Ok(())
    }

    /// Generate a PDF of the current page. Returns base64-encoded PDF data.
    ///
    /// Uses CDP `Page.printToPDF`. All [`PdfOptions`] fields are optional;
    /// omitted fields use the browser's default values.
    pub async fn print_to_pdf(&self, options: &PdfOptions) -> BrowserResult<String> {
        let mut params = serde_json::Map::new();
        if let Some(v) = options.paper_width {
            params.insert("paperWidth".into(), v.into());
        }
        if let Some(v) = options.paper_height {
            params.insert("paperHeight".into(), v.into());
        }
        if let Some(v) = options.print_background {
            params.insert("printBackground".into(), v.into());
        }
        if let Some(v) = options.scale {
            params.insert("scale".into(), v.into());
        }
        if let Some(v) = options.display_header_footer {
            params.insert("displayHeaderFooter".into(), v.into());
        }
        if let Some(v) = options.margin_top {
            params.insert("marginTop".into(), v.into());
        }
        if let Some(v) = options.margin_bottom {
            params.insert("marginBottom".into(), v.into());
        }
        if let Some(v) = options.margin_left {
            params.insert("marginLeft".into(), v.into());
        }
        if let Some(v) = options.margin_right {
            params.insert("marginRight".into(), v.into());
        }
        let params = serde_json::Value::Object(params);
        self.cdp
            .with_current_route(async {
                let result = self.cdp.send("Page.printToPDF", Some(params)).await?;
                Ok(result["data"]
                    .as_str()
                    .ok_or("Page.printToPDF returned no data")?
                    .to_string())
            })
            .await
    }

    /// Override the browser's reported geolocation.
    ///
    /// Pass `Some(GeoLocation)` to set coordinates; pass `None` to clear
    /// the override and restore the real position.
    pub async fn set_geolocation(&self, location: Option<&GeoLocation>) -> BrowserResult<()> {
        self.cdp
            .with_current_route(async {
                if let Some(loc) = location {
                    self.cdp
                        .send(
                            "Emulation.setGeolocationOverride",
                            Some(serde_json::json!({
                                "latitude": loc.latitude,
                                "longitude": loc.longitude,
                                "accuracy": loc.accuracy.unwrap_or(100.0)
                            })),
                        )
                        .await?;
                } else {
                    self.cdp
                        .send("Emulation.clearGeolocationOverride", None)
                        .await?;
                }
                Ok(())
            })
            .await
    }

    /// Override the browser's reported timezone.
    ///
    /// Accepts IANA timezone IDs (e.g. `"America/New_York"`). Pass
    /// `None` to clear the override and restore the system timezone.
    pub async fn set_timezone(&self, timezone_id: Option<&str>) -> BrowserResult<()> {
        self.cdp
            .with_current_route(async {
                let tz = timezone_id.unwrap_or("");
                self.cdp
                    .send(
                        "Emulation.setTimezoneOverride",
                        Some(serde_json::json!({"timezoneId": tz})),
                    )
                    .await?;
                Ok(())
            })
            .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pdf_options_letter_is_us_letter_with_background() {
        let opts = PdfOptions::letter();
        assert_eq!(opts.paper_width, Some(8.5));
        assert_eq!(opts.paper_height, Some(11.0));
        assert_eq!(opts.print_background, Some(true));
        assert_eq!(opts.scale, None);
        assert_eq!(opts.display_header_footer, None);
        assert_eq!(opts.margin_top, None);
    }

    #[test]
    fn pdf_options_default_is_all_none() {
        let opts = PdfOptions::default();
        assert!(opts.paper_width.is_none());
        assert!(opts.paper_height.is_none());
        assert!(opts.print_background.is_none());
        assert!(opts.scale.is_none());
        assert!(opts.margin_top.is_none());
        assert!(opts.margin_bottom.is_none());
        assert!(opts.margin_left.is_none());
        assert!(opts.margin_right.is_none());
    }

    #[test]
    fn geo_location_serializes_without_accuracy_when_none() {
        let loc = GeoLocation {
            latitude: 37.7749,
            longitude: -122.4194,
            accuracy: None,
        };
        let json = serde_json::to_value(&loc).unwrap();
        assert_eq!(json["latitude"], 37.7749);
        assert_eq!(json["longitude"], -122.4194);
        assert!(json.get("accuracy").is_none());
    }

    #[test]
    fn geo_location_serializes_accuracy_when_present() {
        let loc = GeoLocation {
            latitude: 51.5074,
            longitude: -0.1278,
            accuracy: Some(42.0),
        };
        let json = serde_json::to_value(&loc).unwrap();
        assert_eq!(json["accuracy"], 42.0);
    }

    #[test]
    fn geo_location_roundtrip_through_json() {
        let original = GeoLocation {
            latitude: -33.8688,
            longitude: 151.2093,
            accuracy: Some(5.0),
        };
        let json = serde_json::to_string(&original).unwrap();
        let parsed: GeoLocation = serde_json::from_str(&json).unwrap();
        assert!((parsed.latitude - original.latitude).abs() < 1e-10);
        assert!((parsed.longitude - original.longitude).abs() < 1e-10);
        assert!((parsed.accuracy.unwrap() - 5.0).abs() < 1e-10);
    }

    #[test]
    fn geo_location_roundtrip_without_accuracy() {
        let original = GeoLocation {
            latitude: 35.6762,
            longitude: 139.6503,
            accuracy: None,
        };
        let json = serde_json::to_string(&original).unwrap();
        let parsed: GeoLocation = serde_json::from_str(&json).unwrap();
        assert!((parsed.latitude - 35.6762).abs() < 1e-10);
        assert!((parsed.longitude - 139.6503).abs() < 1e-10);
        assert!(parsed.accuracy.is_none());
    }

    #[test]
    fn pdf_options_letter_serializes_correctly() {
        let opts = PdfOptions::letter();
        let json = serde_json::to_value(&opts).unwrap();
        assert_eq!(json["paper_width"], 8.5);
        assert_eq!(json["paper_height"], 11.0);
        assert_eq!(json["print_background"], true);
        // Omitted fields should be absent, not null
        assert!(json.get("scale").is_none());
        assert!(json.get("margin_top").is_none());
    }

    #[test]
    fn pdf_options_default_serializes_empty_object() {
        let opts = PdfOptions::default();
        let json = serde_json::to_value(&opts).unwrap();
        // All fields are None, so serialization should skip them all
        assert!(json.as_object().unwrap().is_empty());
    }
}
