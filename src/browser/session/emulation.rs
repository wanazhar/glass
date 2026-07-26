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

impl BrowserSession {
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
}
