use super::config::{NativeEngineConfig, canonical_fixture_url, validate_url_text};
use super::error::NativeEngineError;
use super::origin::NativeOrigin;
use std::collections::BTreeMap;

/// A bounded HTML resource accepted by the Phase 1 engine.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NativeResource {
    pub url: String,
    pub origin: NativeOrigin,
    pub body: String,
}

/// Local-only resource loader for about, data, and registered fixture URLs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NativeResourceLoader {
    fixtures: BTreeMap<String, String>,
    max_document_bytes: usize,
}

impl NativeResourceLoader {
    pub(crate) fn new(config: &NativeEngineConfig) -> Result<Self, NativeEngineError> {
        config.validate()?;
        let fixtures = config
            .fixtures
            .iter()
            .map(|fixture| (fixture.url.clone(), fixture.html.clone()))
            .collect();
        Ok(Self {
            fixtures,
            max_document_bytes: config.limits.max_document_bytes,
        })
    }

    /// Load one supported local resource without filesystem or network access.
    pub fn load(&self, url: &str) -> Result<NativeResource, NativeEngineError> {
        validate_url_text("navigation URL", url)?;
        if url == "about:blank" {
            return Ok(NativeResource {
                url: url.into(),
                origin: NativeOrigin::Opaque,
                body: String::new(),
            });
        }
        if let Some(data) = url.strip_prefix("data:") {
            return self.load_data_url(url, data);
        }
        if url.starts_with("fixture:") {
            let canonical = canonical_fixture_url(url)?;
            let Some(body) = self.fixtures.get(&canonical) else {
                return Err(NativeEngineError::UnsupportedUrl {
                    reason: "fixture URL is not registered in the engine configuration".into(),
                });
            };
            return Ok(NativeResource {
                url: canonical,
                origin: NativeOrigin::Opaque,
                body: body.clone(),
            });
        }
        Err(NativeEngineError::UnsupportedUrl {
            reason: "Phase 1 does not load network, filesystem, or other URL schemes".into(),
        })
    }

    fn load_data_url(&self, url: &str, data: &str) -> Result<NativeResource, NativeEngineError> {
        let Some((metadata, payload)) = data.split_once(',') else {
            return Err(NativeEngineError::UnsupportedUrl {
                reason: "data URL is missing its comma-separated payload".into(),
            });
        };
        let mut metadata_parts = metadata.split(';');
        let media_type = metadata_parts.next().unwrap_or_default();
        if !media_type.eq_ignore_ascii_case("text/html") {
            return Err(NativeEngineError::UnsupportedUrl {
                reason: "only data:text/html resources are supported".into(),
            });
        }
        if metadata_parts.any(|part| part.eq_ignore_ascii_case("base64")) {
            return Err(NativeEngineError::UnsupportedUrl {
                reason: "base64 data URLs are not supported in Phase 1".into(),
            });
        }
        let body = percent_decode(payload)?;
        if body.len() > self.max_document_bytes {
            return Err(NativeEngineError::limit(
                "data document",
                self.max_document_bytes,
                body.len(),
            ));
        }
        Ok(NativeResource {
            url: url.into(),
            origin: NativeOrigin::Opaque,
            body,
        })
    }
}

fn percent_decode(value: &str) -> Result<String, NativeEngineError> {
    let bytes = value.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] != b'%' {
            decoded.push(bytes[index]);
            index += 1;
            continue;
        }
        if index + 2 >= bytes.len() {
            return Err(NativeEngineError::UnsupportedUrl {
                reason: "data URL contains an incomplete percent escape".into(),
            });
        }
        let Some(high) = hex_value(bytes[index + 1]) else {
            return Err(NativeEngineError::UnsupportedUrl {
                reason: "data URL contains an invalid percent escape".into(),
            });
        };
        let Some(low) = hex_value(bytes[index + 2]) else {
            return Err(NativeEngineError::UnsupportedUrl {
                reason: "data URL contains an invalid percent escape".into(),
            });
        };
        decoded.push((high << 4) | low);
        index += 3;
    }
    String::from_utf8(decoded).map_err(|_| NativeEngineError::UnsupportedUrl {
        reason: "data URL payload is not valid UTF-8 HTML".into(),
    })
}

fn hex_value(value: u8) -> Option<u8> {
    match value {
        b'0'..=b'9' => Some(value - b'0'),
        b'a'..=b'f' => Some(value - b'a' + 10),
        b'A'..=b'F' => Some(value - b'A' + 10),
        _ => None,
    }
}
