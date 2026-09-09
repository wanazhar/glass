use super::config::{
    MAX_NATIVE_DOCUMENT_BYTES, NativeEngineConfig, canonical_fixture_url, is_network_url,
    validate_url_text, without_fragment,
};
use super::error::NativeEngineError;
use super::origin::NativeOrigin;
use base64::Engine as _;
use futures_util::StreamExt;
use std::collections::BTreeMap;
use std::time::Duration;
use url::Url;

const MAX_NATIVE_NETWORK_REDIRECTS: usize = 8;
const NATIVE_NETWORK_TIMEOUT: Duration = Duration::from_secs(30);

/// A bounded HTML resource accepted by the native engine.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NativeResource {
    pub url: String,
    pub origin: NativeOrigin,
    pub body: String,
}

/// Bounded resource loader for local documents and HTTP(S) HTML responses.
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

    pub(crate) fn for_content_process(
        max_document_bytes: usize,
    ) -> Result<Self, NativeEngineError> {
        if max_document_bytes == 0 || max_document_bytes > MAX_NATIVE_DOCUMENT_BYTES {
            return Err(NativeEngineError::invalid(
                "content-process document limit",
                format!("must be between 1 and {MAX_NATIVE_DOCUMENT_BYTES}"),
            ));
        }
        Ok(Self {
            fixtures: BTreeMap::new(),
            max_document_bytes,
        })
    }

    /// Load one supported local resource without filesystem or network access.
    /// HTTP(S) resources use [`Self::load_async`] so blocking transport never
    /// enters the synchronous deterministic path.
    pub fn load(&self, url: &str) -> Result<NativeResource, NativeEngineError> {
        validate_url_text("navigation URL", url)?;
        let resource_url = without_fragment(url);
        if resource_url == "about:blank" {
            return Ok(NativeResource {
                url: url.into(),
                origin: NativeOrigin::Opaque,
                body: String::new(),
            });
        }
        if let Some(data) = resource_url.strip_prefix("data:") {
            return self.load_data_url(url, data);
        }
        if resource_url.starts_with("fixture:") {
            let canonical = canonical_fixture_url(resource_url)?;
            let Some(body) = self.fixtures.get(&canonical) else {
                return Err(NativeEngineError::UnsupportedUrl {
                    reason: "fixture URL is not registered in the engine configuration".into(),
                });
            };
            return Ok(NativeResource {
                url: fixture_navigation_url(&canonical, url),
                origin: NativeOrigin::Opaque,
                body: body.clone(),
            });
        }
        Err(NativeEngineError::UnsupportedUrl {
            reason: "HTTP(S) loading requires asynchronous native navigation; filesystem and other URL schemes are unsupported".into(),
        })
    }

    /// Load one bounded HTML document over HTTP(S).
    ///
    /// This is deliberately a document-only network slice: it follows a
    /// bounded redirect chain, rejects credentials and non-HTML responses,
    /// enforces the configured document limit while streaming, and does not
    /// fetch subresources or execute page code.
    pub async fn load_async(&self, url: &str) -> Result<NativeResource, NativeEngineError> {
        validate_url_text("navigation URL", url)?;
        let resource_url = without_fragment(url);
        if !is_network_url(resource_url) {
            return self.load(url);
        }
        let parsed = Url::parse(resource_url).map_err(|_| NativeEngineError::UnsupportedUrl {
            reason: "HTTP(S) navigation URL is not valid URL syntax".into(),
        })?;
        reject_credentials(&parsed)?;
        let client = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::limited(
                MAX_NATIVE_NETWORK_REDIRECTS,
            ))
            .timeout(NATIVE_NETWORK_TIMEOUT)
            .build()
            .map_err(|error| network_error("HTTP client construction", error))?;
        let response = client
            .get(parsed)
            .header(reqwest::header::ACCEPT, "text/html,application/xhtml+xml")
            .send()
            .await
            .map_err(|error| network_error("HTTP document request", error))?;
        if !response.status().is_success() {
            return Err(NativeEngineError::Network {
                operation: "HTTP document request".into(),
                reason: format!("server returned HTTP {}", response.status().as_u16()),
            });
        }
        validate_html_content_type(response.headers().get(reqwest::header::CONTENT_TYPE))?;
        let content_length = response.content_length();
        if content_length.is_some_and(|length| length > self.max_document_bytes as u64) {
            return Err(NativeEngineError::limit(
                "HTTP document",
                self.max_document_bytes,
                content_length
                    .and_then(|length| usize::try_from(length).ok())
                    .unwrap_or(usize::MAX),
            ));
        }
        let final_url = response.url().clone();
        reject_credentials(&final_url)?;
        if !is_network_url(final_url.as_str()) {
            return Err(NativeEngineError::UnsupportedUrl {
                reason: "HTTP(S) navigation redirected to a non-HTTP(S) URL".into(),
            });
        }
        let mut stream = response.bytes_stream();
        let mut bytes = Vec::with_capacity(
            content_length
                .unwrap_or_default()
                .min(self.max_document_bytes as u64) as usize,
        );
        while let Some(chunk) = stream.next().await {
            let chunk = chunk.map_err(|error| network_error("HTTP document body", error))?;
            let next_len = bytes.len().saturating_add(chunk.len());
            if next_len > self.max_document_bytes {
                return Err(NativeEngineError::limit(
                    "HTTP document",
                    self.max_document_bytes,
                    next_len,
                ));
            }
            bytes.extend_from_slice(&chunk);
        }
        let body = String::from_utf8(bytes).map_err(|_| NativeEngineError::Network {
            operation: "HTTP document decoding".into(),
            reason: "HTTP(S) document is not valid UTF-8; charset decoding is not yet implemented"
                .into(),
        })?;
        let navigation_url = append_original_fragment(final_url, url);
        let origin =
            NativeOrigin::from_url(&Url::parse(without_fragment(&navigation_url)).map_err(
                |_| NativeEngineError::UnsupportedUrl {
                    reason: "HTTP(S) navigation produced invalid final URL syntax".into(),
                },
            )?)?;
        Ok(NativeResource {
            url: navigation_url,
            origin,
            body,
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
        let body = if metadata_parts.any(|part| part.eq_ignore_ascii_case("base64")) {
            decode_base64(payload, self.max_document_bytes)?
        } else {
            percent_decode(payload)?
        };
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

fn reject_credentials(url: &Url) -> Result<(), NativeEngineError> {
    if !url.username().is_empty() || url.password().is_some() {
        return Err(NativeEngineError::UnsupportedUrl {
            reason: "HTTP(S) navigation URLs must not contain userinfo credentials".into(),
        });
    }
    Ok(())
}

fn validate_html_content_type(
    value: Option<&reqwest::header::HeaderValue>,
) -> Result<(), NativeEngineError> {
    let Some(value) = value else {
        return Ok(());
    };
    let value = value.to_str().map_err(|_| NativeEngineError::Network {
        operation: "HTTP content-type validation".into(),
        reason: "HTTP content type is not valid ASCII".into(),
    })?;
    let media_type = value.split(';').next().unwrap_or_default().trim();
    if media_type.eq_ignore_ascii_case("text/html")
        || media_type.eq_ignore_ascii_case("application/xhtml+xml")
    {
        return Ok(());
    }
    Err(NativeEngineError::UnsupportedUrl {
        reason: format!("HTTP(S) navigation returned unsupported content type {media_type:?}"),
    })
}

fn append_original_fragment(final_url: Url, original_url: &str) -> String {
    let Some((_, fragment)) = original_url.split_once('#') else {
        return final_url.to_string();
    };
    let mut final_url = final_url;
    final_url.set_fragment(Some(fragment));
    final_url.to_string()
}

fn network_error(operation: &str, _error: impl std::fmt::Display) -> NativeEngineError {
    NativeEngineError::Network {
        operation: operation.into(),
        reason: "request failed without exposing response data".into(),
    }
}

fn fixture_navigation_url(canonical: &str, original: &str) -> String {
    original.split_once('#').map_or_else(
        || canonical.to_owned(),
        |(_, fragment)| format!("{canonical}#{fragment}"),
    )
}

fn decode_base64(payload: &str, max_document_bytes: usize) -> Result<String, NativeEngineError> {
    let max_encoded_bytes = (max_document_bytes.saturating_add(2) / 3).saturating_mul(4);
    if payload.len() > max_encoded_bytes {
        return Err(NativeEngineError::limit(
            "base64 data payload",
            max_encoded_bytes,
            payload.len(),
        ));
    }
    let decoded = base64::engine::general_purpose::STANDARD
        .decode(payload)
        .map_err(|_| NativeEngineError::UnsupportedUrl {
            reason: "data URL contains an invalid standard base64 payload".into(),
        })?;
    if decoded.len() > max_document_bytes {
        return Err(NativeEngineError::limit(
            "data document",
            max_document_bytes,
            decoded.len(),
        ));
    }
    String::from_utf8(decoded).map_err(|_| NativeEngineError::UnsupportedUrl {
        reason: "base64 data URL payload is not valid UTF-8 HTML".into(),
    })
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
