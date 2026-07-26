use super::*;

/// Options for page PDF generation via CDP Page.printToPDF.
#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct PdfOptions {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub paper_width: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub paper_height: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub print_background: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scale: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display_header_footer: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub margin_top: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub margin_bottom: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub margin_left: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub margin_right: Option<f64>,
}

impl PdfOptions {
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
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct GeoLocation {
    pub latitude: f64,
    pub longitude: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub accuracy: Option<f64>,
}

impl BrowserSession {
    /// Generate a PDF of the current page. Returns base64-encoded PDF data.
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

    /// Override geolocation. Call with None to clear.
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

    /// Override timezone. Accepts IANA IDs. None clears.
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
