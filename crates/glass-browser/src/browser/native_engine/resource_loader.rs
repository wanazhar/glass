use super::config::{
    MAX_NATIVE_DOCUMENT_BYTES, NativeEngineConfig, canonical_fixture_url, is_network_url,
    validate_url_text, without_fragment,
};
use super::error::NativeEngineError;
use super::origin::NativeOrigin;
use base64::Engine as _;
use futures_util::StreamExt;
use std::collections::BTreeMap;
use std::io;
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
            .redirect(reqwest::redirect::Policy::custom(native_redirect_policy))
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
        let charset = content_type_charset(response.headers().get(reqwest::header::CONTENT_TYPE))?;
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
        let body = decode_html_body(&bytes, charset.as_deref(), self.max_document_bytes)?;
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

fn content_type_charset(
    value: Option<&reqwest::header::HeaderValue>,
) -> Result<Option<String>, NativeEngineError> {
    let Some(value) = value else {
        return Ok(None);
    };
    let value = value.to_str().map_err(|_| NativeEngineError::Network {
        operation: "HTTP content-type validation".into(),
        reason: "HTTP content type is not valid ASCII".into(),
    })?;
    let media_type = value.split(';').next().unwrap_or_default().trim();
    if media_type.eq_ignore_ascii_case("text/html")
        || media_type.eq_ignore_ascii_case("application/xhtml+xml")
    {
        let charset = value
            .split(';')
            .skip(1)
            .filter_map(|part| part.split_once('='))
            .find_map(|(name, value)| {
                name.trim()
                    .eq_ignore_ascii_case("charset")
                    .then(|| value.trim().trim_matches(['"', '\'']).to_ascii_lowercase())
            });
        return Ok(charset);
    }
    Err(NativeEngineError::UnsupportedUrl {
        reason: format!("HTTP(S) navigation returned unsupported content type {media_type:?}"),
    })
}

fn native_redirect_policy(attempt: reqwest::redirect::Attempt<'_>) -> reqwest::redirect::Action {
    if attempt.previous().len() >= MAX_NATIVE_NETWORK_REDIRECTS {
        return attempt.stop();
    }
    let url = attempt.url();
    if !is_network_url(without_fragment(url.as_str()))
        || !url.username().is_empty()
        || url.password().is_some()
    {
        return attempt.error(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "native navigation redirect violates the HTTP(S) URL policy",
        ));
    }
    attempt.follow()
}

fn decode_html_body(
    bytes: &[u8],
    charset: Option<&str>,
    max_document_bytes: usize,
) -> Result<String, NativeEngineError> {
    let label = charset.unwrap_or_default().trim().to_ascii_lowercase();
    let decoded = if label.is_empty() {
        if bytes.starts_with(&[0xef, 0xbb, 0xbf]) {
            String::from_utf8(bytes[3..].to_vec()).map_err(|_| ())
        } else if bytes.starts_with(&[0xff, 0xfe]) {
            decode_utf16(&bytes[2..], true)
        } else if bytes.starts_with(&[0xfe, 0xff]) {
            decode_utf16(&bytes[2..], false)
        } else {
            String::from_utf8(bytes.to_vec()).map_err(|_| ())
        }
    } else {
        match label.as_str() {
            "utf-8" | "utf8" => String::from_utf8(bytes.to_vec()).map_err(|_| ()),
            "utf-16" | "utf-16le" => decode_utf16(bytes, true),
            "utf-16be" => decode_utf16(bytes, false),
            "us-ascii" => {
                if bytes.iter().any(|byte| *byte > 0x7f) {
                    return Err(charset_error(&label));
                }
                String::from_utf8(bytes.to_vec()).map_err(|_| ())
            }
            "iso-8859-1" | "latin1" | "latin-1" => {
                Ok(bytes.iter().map(|byte| char::from(*byte)).collect())
            }
            "windows-1252" | "cp1252" => Ok(decode_windows_1252(bytes)),
            _ => return Err(charset_error(&label)),
        }
    }
    .map_err(|_| charset_error(if label.is_empty() { "utf-8" } else { &label }))?;
    if decoded.len() > max_document_bytes {
        return Err(NativeEngineError::limit(
            "decoded HTML document",
            max_document_bytes,
            decoded.len(),
        ));
    }
    Ok(decoded)
}

fn decode_utf16(bytes: &[u8], little_endian: bool) -> Result<String, ()> {
    if !bytes.len().is_multiple_of(2) {
        return Err(());
    }
    let units = bytes
        .chunks_exact(2)
        .map(|chunk| {
            if little_endian {
                u16::from_le_bytes([chunk[0], chunk[1]])
            } else {
                u16::from_be_bytes([chunk[0], chunk[1]])
            }
        })
        .collect::<Vec<_>>();
    String::from_utf16(&units).map_err(|_| ())
}

fn decode_windows_1252(bytes: &[u8]) -> String {
    const SPECIAL: [char; 32] = [
        '\u{20ac}', '\u{0081}', '\u{201a}', '\u{0192}', '\u{201e}', '\u{2026}', '\u{2020}',
        '\u{2021}', '\u{02c6}', '\u{2030}', '\u{0160}', '\u{2039}', '\u{0152}', '\u{008d}',
        '\u{017d}', '\u{008f}', '\u{0090}', '\u{2018}', '\u{2019}', '\u{201c}', '\u{201d}',
        '\u{2022}', '\u{2013}', '\u{2014}', '\u{02dc}', '\u{2122}', '\u{0161}', '\u{203a}',
        '\u{0153}', '\u{009d}', '\u{017e}', '\u{0178}',
    ];
    bytes
        .iter()
        .map(|byte| {
            if (0x80..=0x9f).contains(byte) {
                SPECIAL[usize::from(*byte - 0x80)]
            } else {
                char::from(*byte)
            }
        })
        .collect()
}

fn charset_error(charset: &str) -> NativeEngineError {
    NativeEngineError::Network {
        operation: "HTTP document decoding".into(),
        reason: format!("HTTP(S) document charset {charset:?} is unsupported or invalid"),
    }
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

#[cfg(test)]
mod tests {
    use super::decode_html_body;

    #[test]
    fn decodes_bounded_declared_html_charsets() {
        assert_eq!(
            decode_html_body(b"caf\xe9", Some("windows-1252"), 32).unwrap(),
            "café"
        );
        assert_eq!(
            decode_html_body(&[0xff, 0xfe, b'c', 0, b'a', 0, b'f', 0], None, 32).unwrap(),
            "caf"
        );
        assert!(decode_html_body(b"\xff", Some("us-ascii"), 32).is_err());
        assert!(decode_html_body(b"text", Some("x-unknown"), 32).is_err());
    }
}
