use super::error::NativeEngineError;
use url::Url;

/// Origin state carried by a native document.
///
/// Local fixture and data documents intentionally use an opaque origin. HTTP
/// and HTTPS documents retain a normalized scheme/host/effective-port tuple;
/// this is the first origin primitive for the later security workstream.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NativeOrigin {
    Opaque,
    Tuple {
        scheme: String,
        host: String,
        port: u16,
    },
}

impl NativeOrigin {
    pub fn from_url(url: &Url) -> Result<Self, NativeEngineError> {
        let scheme = url.scheme();
        if !matches!(scheme, "http" | "https") {
            return Ok(Self::Opaque);
        }
        let host = url.host_str().ok_or_else(|| NativeEngineError::Network {
            operation: "origin construction".into(),
            reason: "HTTP(S) URL is missing a host".into(),
        })?;
        let port = url
            .port_or_known_default()
            .ok_or_else(|| NativeEngineError::Network {
                operation: "origin construction".into(),
                reason: "HTTP(S) URL is missing an effective port".into(),
            })?;
        Ok(Self::Tuple {
            scheme: scheme.into(),
            host: host.into(),
            port,
        })
    }

    /// Recover the creator origin encoded in a native Blob URL. The Blob URL
    /// itself is not a tuple origin in `url::Url`; its first component carries
    /// the origin of the realm that created it instead.
    pub(crate) fn from_blob_url(url: &str) -> Result<Self, NativeEngineError> {
        let parsed = Url::parse(url).map_err(|_| NativeEngineError::UnsupportedUrl {
            reason: "native Blob URL is not valid URL syntax".into(),
        })?;
        if parsed.scheme() != "blob" {
            return Err(NativeEngineError::UnsupportedUrl {
                reason: "native object URL navigation requires a blob URL".into(),
            });
        }
        let inner = url
            .strip_prefix("blob:")
            .and_then(|value| value.split('#').next())
            .unwrap_or_default();
        if inner == "null" || inner.starts_with("null/") {
            return Ok(Self::Opaque);
        }
        let inner_url = Url::parse(inner).map_err(|_| NativeEngineError::UnsupportedUrl {
            reason: "native Blob URL creator origin is invalid".into(),
        })?;
        Self::from_url(&inner_url)
    }

    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Opaque => "opaque",
            Self::Tuple { .. } => "tuple",
        }
    }

    pub fn serialized(&self) -> String {
        match self {
            Self::Opaque => "null".into(),
            Self::Tuple { scheme, host, port } => {
                let default_port =
                    matches!((scheme.as_str(), *port), ("http", 80) | ("https", 443));
                if default_port {
                    format!("{scheme}://{host}")
                } else {
                    format!("{scheme}://{host}:{port}")
                }
            }
        }
    }
}
