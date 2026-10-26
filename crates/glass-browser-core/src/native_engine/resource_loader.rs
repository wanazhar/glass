use super::config::{
    MAX_NATIVE_DOCUMENT_BYTES, NativeEngineConfig, canonical_fixture_url, is_file_url,
    is_network_url, validate_url_text, without_fragment,
};
use super::environment::NativeEnvironmentOverrides;
use super::error::NativeEngineError;
use super::font::MAX_NATIVE_FONT_BYTES;
use super::image::{MAX_NATIVE_IMAGE_TRANSFER_BYTES, NativeImage, decode_image_bytes};
use super::interaction::MAX_NATIVE_FORM_BODY_BYTES;
use super::javascript::{
    MAX_NATIVE_COOKIE_PROFILE_ENTRIES, MAX_NATIVE_SCRIPT_BYTES, NativeCookieChange,
    NativeCookieProfileEntry, load_cookie_profile,
};
use super::origin::NativeOrigin;
use base64::Engine as _;
use futures_util::StreamExt;
use reqwest::header::HeaderMap;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256, Sha384, Sha512};
use std::collections::BTreeMap;
use std::fmt;
use std::fs::{self, File};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard, OnceLock};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tokio::sync::Semaphore;
use url::Url;

const MAX_NATIVE_NETWORK_REDIRECTS: usize = 8;
const NATIVE_NETWORK_TIMEOUT: Duration = Duration::from_secs(30);
const MAX_NATIVE_CACHE_ENTRIES: usize = 32;
const MAX_NATIVE_PREFLIGHT_CACHE_ENTRIES: usize = 64;
const MAX_NATIVE_PREFLIGHT_CACHE_AGE: Duration = Duration::from_secs(600);
const MAX_NATIVE_COOKIE_BYTES: usize = 4096;
const MAX_NATIVE_FETCH_HEADERS: usize = 16;
mod cookie_methods;
mod cookie_policy;
mod data_url;
mod fetch;
mod navigation_load;
mod response_encoding;
use data_url::*;
use response_encoding::*;
mod csp;
mod csp_headers;
use cookie_policy::*;
pub(crate) use csp_headers::resolve_subresource_url;
use csp_headers::*;
mod fonts;
mod images;
mod media;
mod scripts;

mod policies;
mod referrer;
use csp::*;
pub(crate) use csp::{NativeInlineScriptPolicy, csp_sources_allow, csp_sources_allow_for_redirect};
#[cfg(test)]
use referrer::referrer_for_navigation;
pub(crate) use referrer::referrer_for_navigation_with_policy;
use referrer::{
    fetch_referrer_for_target, fetch_referrer_source, normalize_referrer,
    parse_navigation_referrer_source, referrer_policy_from_header_pairs,
    referrer_policy_from_headers,
};

pub(crate) const MAX_NATIVE_FETCH_METHOD_BYTES: usize = 64;
const MAX_NATIVE_FETCH_HEADER_NAME_BYTES: usize = 128;
const MAX_NATIVE_FETCH_HEADER_VALUE_BYTES: usize = 64 * 1024;
const MAX_NATIVE_FETCH_HEADER_BYTES: usize = 128 * 1024;
pub(crate) const MAX_NATIVE_RESPONSE_HEADERS: usize = 64;
pub(crate) const MAX_NATIVE_RESPONSE_HEADER_NAME_BYTES: usize = 128;
pub(crate) const MAX_NATIVE_RESPONSE_HEADER_VALUE_BYTES: usize = 64 * 1024;
pub(crate) const MAX_NATIVE_RESPONSE_HEADER_BYTES: usize = 128 * 1024;
pub(crate) const MAX_NATIVE_DOWNLOAD_BYTES: usize = 16 * 1024 * 1024;
pub(crate) const MAX_NATIVE_CSP_POLICIES: usize = 16;
pub(crate) const MAX_NATIVE_CSP_VIOLATIONS: usize = 128;
const MAX_NATIVE_CSP_REPORT_ENDPOINTS: usize = 8;
const MAX_NATIVE_CSP_REPORT_ENDPOINT_BYTES: usize = 2048;
const MAX_NATIVE_CSP_REPORT_PAYLOAD_BYTES: usize = 128 * 1024;
const MAX_NATIVE_CSP_REPORT_TASKS: usize = 32;
const NATIVE_CSP_REPORT_TIMEOUT: Duration = Duration::from_secs(5);
const MAX_NATIVE_CSP_SOURCE_EXPRESSION_BYTES: usize = 2048;
pub(crate) const MAX_NATIVE_MEDIA_BYTES: usize = 16 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum NativeSubresourceKind {
    Style,
    Script,
    Image,
    Font,
    Media,
    Frame,
    Connect,
    Worker,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum NativeModuleResourceType {
    JavaScript,
    Json,
    Unsupported,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum NativeNavigationPolicyKind {
    FormAction,
    NavigateTo,
}

impl NativeNavigationPolicyKind {
    const fn directive(self) -> &'static str {
        match self {
            Self::FormAction => "form-action",
            Self::NavigateTo => "navigate-to",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum NativeInlineCspKind {
    ScriptElement,
    ScriptAttribute,
    StyleElement,
    StyleAttribute,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum NativeCorsMode {
    NoCors,
    Cors,
    SameOrigin,
    /// A browser navigation/download request. It is not a script-readable
    /// CORS fetch, so cross-origin bytes remain available to the parent
    /// download owner while page script receives no response object.
    Navigation,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum NativeFetchRedirectMode {
    Follow,
    Error,
    Manual,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum NativeFetchCacheMode {
    #[default]
    Default,
    NoStore,
    Reload,
    NoCache,
    ForceCache,
    OnlyIfCached,
}

impl NativeFetchCacheMode {
    pub(crate) fn from_option(value: Option<&str>) -> Result<Self, NativeEngineError> {
        match value.unwrap_or("default") {
            "default" => Ok(Self::Default),
            "no-store" => Ok(Self::NoStore),
            "reload" => Ok(Self::Reload),
            "no-cache" => Ok(Self::NoCache),
            "force-cache" => Ok(Self::ForceCache),
            "only-if-cached" => Ok(Self::OnlyIfCached),
            _ => Err(NativeEngineError::invalid(
                "fetch cache mode",
                "must be default, no-store, reload, no-cache, force-cache, or only-if-cached",
            )),
        }
    }

    fn can_read(self) -> bool {
        !matches!(self, Self::NoStore | Self::Reload)
    }

    fn can_store(self) -> bool {
        !matches!(self, Self::NoStore)
    }

    fn requires_revalidation(self) -> bool {
        matches!(self, Self::NoCache)
    }

    fn is_only_if_cached(self) -> bool {
        matches!(self, Self::OnlyIfCached)
    }

    fn request_cache_control(self) -> Option<&'static str> {
        match self {
            Self::NoStore => Some("no-store"),
            Self::Reload | Self::NoCache => Some("no-cache"),
            Self::Default | Self::ForceCache | Self::OnlyIfCached => None,
        }
    }
}

/// A bounded HTML resource accepted by the native engine.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NativeResource {
    pub url: String,
    pub origin: NativeOrigin,
    pub body: String,
}

/// Bytes captured from a realm-owned Blob URL for a document navigation.
///
/// The resource is deliberately transferred as bytes rather than as a live
/// JavaScript object. That makes the handoff finite and lets the destination
/// document receive a fresh realm, matching full-navigation lifetime rules.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NativeObjectUrlResource {
    #[serde(default)]
    pub(crate) content_type: Option<String>,
    #[serde(with = "native_object_url_bytes")]
    pub(crate) body: Vec<u8>,
}

/// A bounded snapshot of one Blob URL referenced by a structured message.
///
/// Object URLs are realm-owned strings, so a destination worker cannot look
/// up the source realm's registry directly. The browser boundary transfers
/// the finite resource bytes and lets the destination install a fresh entry
/// under the same URL before delivering the message.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NativeObjectUrlTransfer {
    pub(crate) href: String,
    pub(crate) resource: NativeObjectUrlResource,
}

/// Metadata and bounded decoded media admitted for one media element resource.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct NativeMediaMetadata {
    pub(crate) content_type: String,
    pub(crate) byte_length: usize,
    pub(crate) duration_millis: Option<u64>,
    #[serde(default)]
    pub(crate) video_frames: Vec<NativeVideoFrame>,
    #[serde(default)]
    pub(crate) audio_track: Option<NativeAudioTrack>,
}

/// A decoded, bounded PCM audio track. Samples are interleaved signed 16-bit
/// little-endian values so the client can package them as a playable WAV
/// without depending on a host audio device.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct NativeAudioTrack {
    pub(crate) channels: u16,
    pub(crate) sample_rate: u32,
    pub(crate) pcm_s16le: Vec<u8>,
}

impl NativeAudioTrack {
    pub(crate) fn as_wav(&self) -> Option<Vec<u8>> {
        if (self.channels != 1 && self.channels != 2)
            || !(8_000..=96_000).contains(&self.sample_rate)
            || self.pcm_s16le.is_empty()
            || self.pcm_s16le.len() > MAX_NATIVE_AUDIO_DECODED_BYTES
            || !self
                .pcm_s16le
                .len()
                .is_multiple_of(usize::from(self.channels) * 2)
        {
            return None;
        }
        let data_length = u32::try_from(self.pcm_s16le.len()).ok()?;
        let block_align = self.channels.checked_mul(2)?;
        let byte_rate = self.sample_rate.checked_mul(u32::from(block_align))?;
        let mut bytes = Vec::with_capacity(self.pcm_s16le.len().checked_add(44)?);
        bytes.extend_from_slice(b"RIFF");
        bytes.extend_from_slice(&data_length.checked_add(36)?.to_le_bytes());
        bytes.extend_from_slice(b"WAVEfmt ");
        bytes.extend_from_slice(&16_u32.to_le_bytes());
        bytes.extend_from_slice(&1_u16.to_le_bytes());
        bytes.extend_from_slice(&self.channels.to_le_bytes());
        bytes.extend_from_slice(&self.sample_rate.to_le_bytes());
        bytes.extend_from_slice(&byte_rate.to_le_bytes());
        bytes.extend_from_slice(&block_align.to_le_bytes());
        bytes.extend_from_slice(&16_u16.to_le_bytes());
        bytes.extend_from_slice(b"data");
        bytes.extend_from_slice(&data_length.to_le_bytes());
        bytes.extend_from_slice(&self.pcm_s16le);
        Some(bytes)
    }
}

/// Playable PCM audio decoded by the native browser, with the source kept
/// private to the browser owner for redacted client-side provenance labels.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NativeAudioClip {
    pub source: String,
    pub wav_bytes: Vec<u8>,
}

/// One decoded, bounded VP8 frame retained for native WebM video playback.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct NativeVideoFrame {
    pub(crate) timestamp_millis: u64,
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) pixels: Vec<u8>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "UPPERCASE")]
#[derive(Default)]
pub enum NativeNavigationMethod {
    #[default]
    Get,
    Head,
    Post,
    Put,
    Patch,
    Delete,
    Options,
}

impl NativeNavigationMethod {
    pub(crate) fn from_fetch_method(method: &str) -> Result<Self, NativeEngineError> {
        match method {
            "GET" => Ok(Self::Get),
            "HEAD" => Ok(Self::Head),
            "POST" => Ok(Self::Post),
            "PUT" => Ok(Self::Put),
            "PATCH" => Ok(Self::Patch),
            "DELETE" => Ok(Self::Delete),
            "OPTIONS" => Ok(Self::Options),
            _ => Err(NativeEngineError::invalid(
                "fetch method",
                "must be GET, HEAD, POST, PUT, PATCH, DELETE, or OPTIONS",
            )),
        }
    }

    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Get => "GET",
            Self::Head => "HEAD",
            Self::Post => "POST",
            Self::Put => "PUT",
            Self::Patch => "PATCH",
            Self::Delete => "DELETE",
            Self::Options => "OPTIONS",
        }
    }

    fn reqwest_method(self) -> reqwest::Method {
        reqwest::Method::from_bytes(self.as_str().as_bytes())
            .expect("native fetch method is a valid HTTP token")
    }

    const fn is_document_method(self) -> bool {
        matches!(self, Self::Get | Self::Post)
    }

    const fn is_safe_cookie_navigation(self) -> bool {
        matches!(self, Self::Get | Self::Head)
    }
}

/// A bounded native HTTP method token. Document navigation keeps its
/// deliberately closed method enum because forms and history have a smaller
/// contract; Fetch and XHR can carry any valid HTTP token.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct NativeFetchMethod(String);

impl Default for NativeFetchMethod {
    fn default() -> Self {
        Self::get()
    }
}

impl NativeFetchMethod {
    pub(crate) fn get() -> Self {
        Self("GET".into())
    }

    pub(crate) fn from_fetch_method(method: &str) -> Result<Self, NativeEngineError> {
        if method.is_empty() || method.len() > MAX_NATIVE_FETCH_METHOD_BYTES {
            return Err(NativeEngineError::invalid(
                "fetch method",
                format!(
                    "must be a non-empty HTTP token no longer than {MAX_NATIVE_FETCH_METHOD_BYTES} bytes"
                ),
            ));
        }
        let normalized = method.to_ascii_uppercase();
        if !normalized.bytes().all(is_http_token_byte) {
            return Err(NativeEngineError::invalid(
                "fetch method",
                "must contain only HTTP token characters",
            ));
        }
        if matches!(normalized.as_str(), "CONNECT" | "TRACE" | "TRACK") {
            return Err(NativeEngineError::invalid(
                "fetch method",
                "CONNECT, TRACE, and TRACK are forbidden",
            ));
        }
        Ok(Self(normalized))
    }

    pub(crate) fn from_navigation_method(method: NativeNavigationMethod) -> Self {
        Self(method.as_str().into())
    }

    pub(crate) fn as_str(&self) -> &str {
        &self.0
    }

    fn reqwest_method(&self) -> reqwest::Method {
        reqwest::Method::from_bytes(self.0.as_bytes())
            .expect("native fetch method is a valid HTTP token")
    }

    fn is_bodyless(&self) -> bool {
        matches!(self.as_str(), "GET" | "HEAD")
    }

    fn is_simple(&self) -> bool {
        matches!(self.as_str(), "GET" | "HEAD" | "POST")
    }
}

fn is_http_token_byte(byte: u8) -> bool {
    matches!(
        byte,
        b'0'..=b'9'
            | b'A'..=b'Z'
            | b'a'..=b'z'
            | b'!'
            | b'#'
            | b'$'
            | b'%'
            | b'&'
            | b'\''
            | b'*'
            | b'+'
            | b'-'
            | b'.'
            | b'^'
            | b'_'
            | b'`'
            | b'|'
            | b'~'
    )
}

trait NativeCookieMethod {
    fn is_safe_cookie_method(&self) -> bool;
}

impl NativeCookieMethod for NativeNavigationMethod {
    fn is_safe_cookie_method(&self) -> bool {
        self.is_safe_cookie_navigation()
    }
}

impl NativeCookieMethod for NativeFetchMethod {
    fn is_safe_cookie_method(&self) -> bool {
        matches!(self.as_str(), "GET" | "HEAD")
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NativeNavigationRequest {
    pub method: NativeNavigationMethod,
    pub url: String,
    pub body: Option<NativeRequestBody>,
    pub body_content_type: Option<String>,
    pub replace_history: bool,
    pub target: Option<String>,
    pub object_url: Option<NativeObjectUrlResource>,
}

impl NativeNavigationRequest {
    pub fn get(url: impl Into<String>) -> Self {
        Self {
            method: NativeNavigationMethod::Get,
            url: url.into(),
            body: None,
            body_content_type: None,
            replace_history: false,
            target: None,
            object_url: None,
        }
    }

    pub(crate) fn post_with_body(
        url: impl Into<String>,
        body: NativeRequestBody,
        content_type: String,
    ) -> Result<Self, NativeEngineError> {
        if body.len() > MAX_NATIVE_FORM_BODY_BYTES {
            return Err(NativeEngineError::limit(
                "form submission body",
                MAX_NATIVE_FORM_BODY_BYTES,
                body.len(),
            ));
        }
        if content_type.is_empty() || content_type.len() > MAX_NATIVE_FORM_BODY_BYTES {
            return Err(NativeEngineError::invalid(
                "form submission content type",
                "must be a non-empty bounded value",
            ));
        }
        Ok(Self {
            method: NativeNavigationMethod::Post,
            url: url.into(),
            body: Some(body),
            body_content_type: Some(content_type),
            replace_history: false,
            target: None,
            object_url: None,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
pub enum NativeRequestBody {
    Text(String),
    Bytes(#[serde(with = "native_request_body_bytes")] Vec<u8>),
    /// Buffered bytes collected from a one-shot JavaScript ReadableStream.
    /// The request bytes cross IPC as a bounded payload, while Fetch redirect
    /// handling must retain the stream's non-replayable semantics.
    StreamedBytes(#[serde(with = "native_request_body_bytes")] Vec<u8>),
}

mod native_request_body_bytes {
    use base64::Engine as _;
    use base64::engine::general_purpose::STANDARD;
    use serde::{Deserialize, Deserializer, Serializer};

    pub(super) fn serialize<S>(bytes: &Vec<u8>, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&STANDARD.encode(bytes))
    }

    pub(super) fn deserialize<'de, D>(deserializer: D) -> Result<Vec<u8>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let encoded = String::deserialize(deserializer)?;
        STANDARD.decode(encoded).map_err(serde::de::Error::custom)
    }
}

mod native_object_url_bytes {
    use base64::Engine as _;
    use base64::engine::general_purpose::STANDARD;
    use serde::{Deserialize, Deserializer, Serializer};

    pub(super) fn serialize<S>(bytes: &Vec<u8>, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&STANDARD.encode(bytes))
    }

    pub(super) fn deserialize<'de, D>(deserializer: D) -> Result<Vec<u8>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let encoded = String::deserialize(deserializer)?;
        STANDARD.decode(encoded).map_err(serde::de::Error::custom)
    }
}

impl NativeRequestBody {
    pub(crate) fn len(&self) -> usize {
        match self {
            Self::Text(body) => body.len(),
            Self::Bytes(body) | Self::StreamedBytes(body) => body.len(),
        }
    }
}

pub fn validate_target_navigation_payload(
    method: NativeNavigationMethod,
    body: Option<&NativeRequestBody>,
    body_content_type: Option<&str>,
    operation: &str,
) -> Result<(), NativeEngineError> {
    if method != NativeNavigationMethod::Get && method != NativeNavigationMethod::Post {
        return Err(NativeEngineError::UnsupportedUrl {
            reason: format!("{operation} supports only GET and POST form navigations"),
        });
    }
    if method == NativeNavigationMethod::Get {
        if body.is_some() || body_content_type.is_some() {
            return Err(NativeEngineError::invalid(
                operation,
                "GET target navigation must not carry a request body",
            ));
        }
        return Ok(());
    }
    let Some(body) = body else {
        return Err(NativeEngineError::invalid(
            operation,
            "POST target navigation must carry a request body",
        ));
    };
    if body.len() > MAX_NATIVE_FORM_BODY_BYTES {
        return Err(NativeEngineError::limit(
            "target navigation request body",
            MAX_NATIVE_FORM_BODY_BYTES,
            body.len(),
        ));
    }
    let Some(content_type) = body_content_type else {
        return Err(NativeEngineError::invalid(
            operation,
            "POST target navigation must carry a content type",
        ));
    };
    if content_type.is_empty() || content_type.len() > MAX_NATIVE_FORM_BODY_BYTES {
        return Err(NativeEngineError::invalid(
            operation,
            "POST target navigation content type must be non-empty and bounded",
        ));
    }
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum NativeFetchCredentialsMode {
    Omit,
    SameOrigin,
    Include,
}

impl NativeFetchCredentialsMode {
    pub(crate) const fn for_xhr(with_credentials: bool) -> Self {
        if with_credentials {
            Self::Include
        } else {
            Self::SameOrigin
        }
    }

    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Omit => "omit",
            Self::SameOrigin => "same-origin",
            Self::Include => "include",
        }
    }

    pub(crate) fn parse(value: &str) -> Result<Self, NativeEngineError> {
        match value {
            "omit" => Ok(Self::Omit),
            "same-origin" => Ok(Self::SameOrigin),
            "include" => Ok(Self::Include),
            _ => Err(NativeEngineError::invalid(
                "native Fetch credentials mode",
                "must be omit, same-origin, or include",
            )),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum NativeFetchReferrerPolicy {
    NoReferrer,
    NoReferrerWhenDowngrade,
    SameOrigin,
    Origin,
    StrictOrigin,
    OriginWhenCrossOrigin,
    StrictOriginWhenCrossOrigin,
    UnsafeUrl,
}

impl NativeFetchReferrerPolicy {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::NoReferrer => "no-referrer",
            Self::NoReferrerWhenDowngrade => "no-referrer-when-downgrade",
            Self::SameOrigin => "same-origin",
            Self::Origin => "origin",
            Self::StrictOrigin => "strict-origin",
            Self::OriginWhenCrossOrigin => "origin-when-cross-origin",
            Self::StrictOriginWhenCrossOrigin => "strict-origin-when-cross-origin",
            Self::UnsafeUrl => "unsafe-url",
        }
    }

    pub(crate) fn parse(value: &str) -> Result<Self, NativeEngineError> {
        match value {
            "" | "strict-origin-when-cross-origin" => Ok(Self::StrictOriginWhenCrossOrigin),
            "no-referrer" => Ok(Self::NoReferrer),
            "no-referrer-when-downgrade" => Ok(Self::NoReferrerWhenDowngrade),
            "same-origin" => Ok(Self::SameOrigin),
            "origin" => Ok(Self::Origin),
            "strict-origin" => Ok(Self::StrictOrigin),
            "origin-when-cross-origin" => Ok(Self::OriginWhenCrossOrigin),
            "unsafe-url" => Ok(Self::UnsafeUrl),
            _ => Err(NativeEngineError::invalid(
                "native Fetch referrer policy",
                "must be a supported Referrer Policy token",
            )),
        }
    }
}

fn fetch_credentials_for_url(
    mode: Option<NativeFetchCredentialsMode>,
    legacy_credentials: bool,
    owner_url: &Url,
    request_url: &Url,
) -> bool {
    match mode {
        Some(NativeFetchCredentialsMode::Omit) => false,
        Some(NativeFetchCredentialsMode::SameOrigin) => owner_url.origin() == request_url.origin(),
        Some(NativeFetchCredentialsMode::Include) => true,
        None => legacy_credentials,
    }
}

#[derive(Debug, Clone, Copy)]
struct NativeImageRequestPolicy {
    destination: &'static str,
    initiator_type: &'static str,
    credentials_mode: NativeFetchCredentialsMode,
    use_url_credentials: bool,
}

impl NativeImageRequestPolicy {
    const HTML_IMAGE: Self = Self {
        destination: "image",
        initiator_type: "img",
        credentials_mode: NativeFetchCredentialsMode::Include,
        use_url_credentials: false,
    };

    const VIDEO_POSTER: Self = Self {
        destination: "image",
        initiator_type: "video",
        credentials_mode: NativeFetchCredentialsMode::Include,
        use_url_credentials: true,
    };
}

pub(crate) struct NativeFetchRequest<'a> {
    pub(crate) document_url: &'a str,
    pub(crate) href: &'a str,
    pub(crate) referrer_url: Option<String>,
    pub(crate) referrer_policy: Option<String>,
    pub(crate) method: NativeFetchMethod,
    pub(crate) body: Option<NativeRequestBody>,
    pub(crate) content_type: Option<String>,
    pub(crate) request_headers: BTreeMap<String, String>,
    pub(crate) credentials: bool,
    pub(crate) credentials_mode: Option<NativeFetchCredentialsMode>,
    pub(crate) cors_mode: NativeCorsMode,
    pub(crate) redirect_mode: NativeFetchRedirectMode,
    pub(crate) cache_mode: NativeFetchCacheMode,
    pub(crate) timeout: Option<Duration>,
    pub(crate) max_response_bytes: Option<usize>,
}

pub struct NativeWebSocketTarget {
    pub url: Url,
    pub(crate) cookie: Option<String>,
    pub csp_violations: Vec<NativeCspViolation>,
    pub csp_report_deliveries: Vec<NativeCspReportDelivery>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct NativeScriptResource {
    pub(crate) url: String,
    pub(crate) body: String,
    pub(crate) response_referrer_policy: Option<NativeFetchReferrerPolicy>,
}

/// A bounded report-only CSP violation waiting for delivery to the owning
/// page realm. This is deliberately a data record: the JavaScript owner
/// creates the platform event only after the record has crossed its boundary.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct NativeCspViolation {
    pub(crate) document_uri: String,
    pub(crate) referrer: String,
    pub(crate) blocked_uri: String,
    pub(crate) effective_directive: String,
    pub(crate) violated_directive: String,
    pub(crate) original_policy: String,
    pub(crate) source_file: String,
    pub(crate) sample: String,
    pub(crate) disposition: String,
    pub(crate) status_code: u16,
    pub(crate) line_number: u32,
    pub(crate) column_number: u32,
}

/// One already-serialized CSP report that can be sent without retaining the
/// owning loader or any page credentials. Keeping the payload here also makes
/// the delivery task independent from later policy mutations or navigation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NativeCspReportDelivery {
    pub(crate) endpoint: String,
    pub(crate) content_type: &'static str,
    pub(crate) body: Vec<u8>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum NativeCspReportFormat {
    Legacy,
    ReportingApi,
}

fn csp_violation_json(violation: &NativeCspViolation) -> serde_json::Value {
    serde_json::json!({
        "document-uri": violation.document_uri,
        "referrer": violation.referrer,
        "blocked-uri": violation.blocked_uri,
        "effective-directive": violation.effective_directive,
        "violated-directive": violation.violated_directive,
        "original-policy": violation.original_policy,
        "source-file": violation.source_file,
        "script-sample": violation.sample,
        "disposition": violation.disposition,
        "status-code": violation.status_code,
        "line-number": violation.line_number,
        "column-number": violation.column_number,
    })
}

fn csp_report_delivery(
    endpoint: String,
    violation: &NativeCspViolation,
    format: NativeCspReportFormat,
) -> Option<NativeCspReportDelivery> {
    let violation_json = csp_violation_json(violation);
    let (content_type, payload) = match format {
        NativeCspReportFormat::Legacy => (
            "application/csp-report",
            serde_json::json!({ "csp-report": violation_json }),
        ),
        NativeCspReportFormat::ReportingApi => (
            "application/reports+json",
            serde_json::json!([{
                "age": 0,
                "type": "csp-violation",
                "url": violation.document_uri,
                "user_agent": "",
                "body": violation_json,
            }]),
        ),
    };
    let body = serde_json::to_vec(&payload).ok()?;
    (body.len() <= MAX_NATIVE_CSP_REPORT_PAYLOAD_BYTES).then_some(NativeCspReportDelivery {
        endpoint,
        content_type,
        body,
    })
}

fn resolve_csp_report_endpoint(document_url: &Url, reference: &str) -> Option<String> {
    let reference = reference.trim();
    if reference.is_empty() || reference.len() > MAX_NATIVE_CSP_REPORT_ENDPOINT_BYTES {
        return None;
    }
    let mut endpoint = document_url.join(reference).ok()?;
    endpoint.set_fragment(None);
    if !is_network_url(endpoint.as_str())
        || !endpoint.username().is_empty()
        || endpoint.password().is_some()
        || !mixed_content_allowed(document_url, &endpoint)
    {
        return None;
    }
    let endpoint = endpoint.as_str().to_owned();
    (endpoint.len() <= MAX_NATIVE_CSP_REPORT_ENDPOINT_BYTES).then_some(endpoint)
}

fn csp_report_deliveries_for_declaration(
    reporting_endpoints: &BTreeMap<String, Vec<String>>,
    declaration: &NativeCspDeclaration,
    document_url: &Url,
    violation: &NativeCspViolation,
) -> Vec<NativeCspReportDelivery> {
    let (references, format) = if let Some(group) = declaration.directives.report_to.as_deref() {
        (
            reporting_endpoints
                .get(group)
                .map(Vec::as_slice)
                .unwrap_or(&[]),
            NativeCspReportFormat::ReportingApi,
        )
    } else {
        (
            declaration.directives.report_uris.as_slice(),
            NativeCspReportFormat::Legacy,
        )
    };
    let mut deliveries = Vec::new();
    for reference in references.iter().take(MAX_NATIVE_CSP_REPORT_ENDPOINTS) {
        let Some(endpoint) = resolve_csp_report_endpoint(document_url, reference) else {
            continue;
        };
        if deliveries.iter().any(|delivery: &NativeCspReportDelivery| {
            delivery.endpoint == endpoint
                && delivery.content_type
                    == match format {
                        NativeCspReportFormat::Legacy => "application/csp-report",
                        NativeCspReportFormat::ReportingApi => "application/reports+json",
                    }
        }) {
            continue;
        }
        if let Some(delivery) = csp_report_delivery(endpoint, violation, format) {
            deliveries.push(delivery);
        }
    }
    deliveries
}

fn native_csp_report_client() -> Option<&'static reqwest::Client> {
    static CLIENT: OnceLock<Option<reqwest::Client>> = OnceLock::new();
    CLIENT
        .get_or_init(|| {
            reqwest::Client::builder()
                .redirect(reqwest::redirect::Policy::none())
                .timeout(NATIVE_CSP_REPORT_TIMEOUT)
                .build()
                .ok()
        })
        .as_ref()
}

/// Start bounded, best-effort report POSTs without retaining the resource
/// loader or any page-owned credentials. Report delivery failures are not
/// browser-visible and cannot change the triggering operation.
pub fn schedule_native_csp_report_deliveries(deliveries: Vec<NativeCspReportDelivery>) {
    if deliveries.is_empty() {
        return;
    }
    let Ok(handle) = tokio::runtime::Handle::try_current() else {
        return;
    };
    let Some(client) = native_csp_report_client().cloned() else {
        return;
    };
    static PERMITS: OnceLock<Arc<Semaphore>> = OnceLock::new();
    let permits =
        Arc::clone(PERMITS.get_or_init(|| Arc::new(Semaphore::new(MAX_NATIVE_CSP_REPORT_TASKS))));
    for delivery in deliveries {
        let Ok(permit) = Arc::clone(&permits).try_acquire_owned() else {
            break;
        };
        let client = client.clone();
        handle.spawn(async move {
            let _permit = permit;
            let _ = client
                .post(delivery.endpoint)
                .header(reqwest::header::CONTENT_TYPE, delivery.content_type)
                .body(delivery.body)
                .send()
                .await;
        });
    }
}

/// A bounded response returned by the native GET/fetch primitive.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NativeFetchResponse {
    pub url: String,
    pub status: u16,
    /// The HTTP reason phrase, or an empty string for synthetic responses.
    /// This is kept separate from the numeric status so Fetch and XHR can
    /// expose the platform's status text without manufacturing one.
    pub status_text: String,
    pub content_type: Option<String>,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
    pub redirected: bool,
    pub opaque: bool,
    pub opaque_redirect: bool,
}

pub(crate) struct NativeFetchResponseStream {
    pub(crate) response: NativeFetchResponse,
    pub(crate) body: Option<reqwest::Response>,
    pub(crate) cached_body: Option<Vec<u8>>,
    pub(crate) max_response_bytes: usize,
}

/// Bounded resource loader for local documents and HTTP(S) HTML responses.
#[derive(Clone, PartialEq)]
pub struct NativeResourceLoader {
    fixtures: BTreeMap<String, String>,
    allowed_file_roots: Vec<PathBuf>,
    max_document_bytes: usize,
    network: NativeNetworkState,
    environment: NativeEnvironmentOverrides,
    cookie_changes: Vec<NativeCookieChange>,
    csp_violations: Vec<NativeCspViolation>,
}

impl fmt::Debug for NativeResourceLoader {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("NativeResourceLoader")
            .field("fixture_count", &self.fixtures.len())
            .field("allowed_file_root_count", &self.allowed_file_roots.len())
            .field("max_document_bytes", &self.max_document_bytes)
            .field("cached_document_count", &self.network.cache.len())
            .field("cached_image_count", &self.network.image_cache.len())
            .field(
                "cached_stylesheet_count",
                &self.network.stylesheet_cache.len(),
            )
            .field("cached_script_count", &self.network.script_cache.len())
            .field("cached_fetch_count", &self.network.fetch_cache.len())
            .field("cached_font_count", &self.network.font_cache.len())
            .field("cookie_count", &self.network.cookies.len())
            .field(
                "document_policy_count",
                &self.network.document_policies.len(),
            )
            .field(
                "document_referrer_policy_count",
                &self.network.document_referrer_policies.len(),
            )
            .field("preflight_cache_count", &self.network.preflight_cache.len())
            .field("offline", &self.environment.network.offline)
            .field(
                "user_agent_overridden",
                &self.environment.user_agent.is_some(),
            )
            .finish()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct NativeNetworkState {
    cookie_authority_enabled: bool,
    cache: BTreeMap<String, NativeDocumentCacheEntry>,
    image_cache: BTreeMap<String, NativeImageCacheEntry>,
    stylesheet_cache: BTreeMap<String, NativeTextCacheEntry>,
    script_cache: BTreeMap<String, NativeTextCacheEntry>,
    fetch_cache: BTreeMap<String, NativeFetchCacheEntry>,
    font_cache: BTreeMap<String, NativeFontCacheEntry>,
    cookies: NativeCookieJar,
    document_policies: BTreeMap<String, NativeCspPolicy>,
    document_response_policy_headers: BTreeMap<String, Vec<(String, String)>>,
    document_referrer_policies: BTreeMap<String, NativeFetchReferrerPolicy>,
    preflight_cache: BTreeMap<String, Instant>,
}

impl Default for NativeNetworkState {
    fn default() -> Self {
        Self {
            cookie_authority_enabled: true,
            cache: BTreeMap::new(),
            image_cache: BTreeMap::new(),
            stylesheet_cache: BTreeMap::new(),
            script_cache: BTreeMap::new(),
            fetch_cache: BTreeMap::new(),
            font_cache: BTreeMap::new(),
            cookies: NativeCookieJar::default(),
            document_policies: BTreeMap::new(),
            document_response_policy_headers: BTreeMap::new(),
            document_referrer_policies: BTreeMap::new(),
            preflight_cache: BTreeMap::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct NativeImageCacheEntry {
    image: NativeImage,
    fresh_until: Option<Instant>,
    etag: Option<String>,
    last_modified: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct NativeTextCacheEntry {
    url: String,
    body: String,
    content_type: Option<String>,
    fresh_until: Option<Instant>,
    etag: Option<String>,
    last_modified: Option<String>,
    referrer_policy: Option<NativeFetchReferrerPolicy>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct NativeFontCacheEntry {
    url: String,
    status: u16,
    status_text: String,
    redirected: bool,
    body: Vec<u8>,
    content_type: Option<String>,
    headers: Vec<(String, String)>,
    fresh_until: Option<Instant>,
    etag: Option<String>,
    last_modified: Option<String>,
}

impl NativeFontCacheEntry {
    fn from_response(
        response: &NativeFetchResponse,
        headers: &HeaderMap,
        now: Instant,
    ) -> Option<Self> {
        if response.body.is_empty()
            || response.body.len() > MAX_NATIVE_FONT_BYTES
            || !response_cache_metadata_present(headers)
            || !document_cache_storage_allowed(headers)
        {
            return None;
        }
        Some(Self {
            url: response.url.clone(),
            status: response.status,
            status_text: response.status_text.clone(),
            redirected: response.redirected,
            body: response.body.clone(),
            content_type: response.content_type.clone(),
            headers: response.headers.clone(),
            fresh_until: document_cache_fresh_until(headers, now),
            etag: response_header_text(headers, reqwest::header::ETAG),
            last_modified: response_header_text(headers, reqwest::header::LAST_MODIFIED),
        })
    }

    fn response(&self) -> NativeFetchResponse {
        NativeFetchResponse {
            url: self.url.clone(),
            status: self.status,
            status_text: self.status_text.clone(),
            content_type: self.content_type.clone(),
            headers: self.headers.clone(),
            body: self.body.clone(),
            redirected: self.redirected,
            opaque: false,
            opaque_redirect: false,
        }
    }

    fn is_fresh(&self, now: Instant) -> bool {
        self.fresh_until.is_none_or(|deadline| now < deadline)
    }

    fn refresh_from_not_modified(mut self, headers: &HeaderMap, now: Instant) -> Option<Self> {
        if !document_cache_storage_allowed(headers) {
            return None;
        }
        if cache_control_requires_revalidation(headers) {
            self.fresh_until = Some(now);
        } else if let Some(fresh_until) = document_cache_fresh_until(headers, now) {
            self.fresh_until = Some(fresh_until);
        }
        if let Some(etag) = response_header_text(headers, reqwest::header::ETAG) {
            self.etag = Some(etag);
        }
        if let Some(last_modified) = response_header_text(headers, reqwest::header::LAST_MODIFIED) {
            self.last_modified = Some(last_modified);
        }
        if let Some(content_type) = response_header_text(headers, reqwest::header::CONTENT_TYPE)
            .filter(|value| font_content_type_text_allowed(value))
        {
            self.content_type = Some(content_type);
        }
        Some(self)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct NativeStylesheetResource {
    pub(crate) url: String,
    pub(crate) body: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct NativeFetchCacheEntry {
    response: NativeFetchResponse,
    fresh_until: Option<Instant>,
    etag: Option<String>,
    last_modified: Option<String>,
}

impl NativeFetchCacheEntry {
    fn from_response(
        response: NativeFetchResponse,
        headers: &HeaderMap,
        now: Instant,
    ) -> Option<Self> {
        if response.opaque || response.opaque_redirect {
            return None;
        }
        if !headers.contains_key(reqwest::header::CACHE_CONTROL)
            && !headers.contains_key(reqwest::header::PRAGMA)
            && !headers.contains_key(reqwest::header::ETAG)
            && !headers.contains_key(reqwest::header::LAST_MODIFIED)
        {
            return None;
        }
        if !document_cache_storage_allowed(headers) {
            return None;
        }
        Some(Self {
            response,
            fresh_until: document_cache_fresh_until(headers, now),
            etag: response_header_text(headers, reqwest::header::ETAG),
            last_modified: response_header_text(headers, reqwest::header::LAST_MODIFIED),
        })
    }

    fn is_fresh(&self, now: Instant) -> bool {
        self.fresh_until.is_none_or(|deadline| now < deadline)
    }

    fn refresh_from_not_modified(mut self, headers: &HeaderMap, now: Instant) -> Option<Self> {
        if !document_cache_storage_allowed(headers) {
            return None;
        }
        if cache_control_requires_revalidation(headers) {
            self.fresh_until = Some(now);
        } else if let Some(fresh_until) = document_cache_fresh_until(headers, now) {
            self.fresh_until = Some(fresh_until);
        }
        if let Some(etag) = response_header_text(headers, reqwest::header::ETAG) {
            self.etag = Some(etag);
        }
        if let Some(last_modified) = response_header_text(headers, reqwest::header::LAST_MODIFIED) {
            self.last_modified = Some(last_modified);
        }
        Some(self)
    }
}

impl NativeTextCacheEntry {
    fn from_response(url: String, body: String, headers: &HeaderMap, now: Instant) -> Option<Self> {
        if !headers.contains_key(reqwest::header::CACHE_CONTROL)
            && !headers.contains_key(reqwest::header::PRAGMA)
            && !headers.contains_key(reqwest::header::ETAG)
            && !headers.contains_key(reqwest::header::LAST_MODIFIED)
        {
            return None;
        }
        if !document_cache_storage_allowed(headers) {
            return None;
        }
        Some(Self {
            url,
            body,
            content_type: response_header_text(headers, reqwest::header::CONTENT_TYPE),
            fresh_until: document_cache_fresh_until(headers, now),
            etag: response_header_text(headers, reqwest::header::ETAG),
            last_modified: response_header_text(headers, reqwest::header::LAST_MODIFIED),
            referrer_policy: referrer_policy_from_headers(headers),
        })
    }

    fn is_fresh(&self, now: Instant) -> bool {
        self.fresh_until.is_none_or(|deadline| now < deadline)
    }

    fn refresh_from_not_modified(mut self, headers: &HeaderMap, now: Instant) -> Option<Self> {
        if !document_cache_storage_allowed(headers) {
            return None;
        }
        if cache_control_requires_revalidation(headers) {
            self.fresh_until = Some(now);
        } else if let Some(fresh_until) = document_cache_fresh_until(headers, now) {
            self.fresh_until = Some(fresh_until);
        }
        if let Some(etag) = response_header_text(headers, reqwest::header::ETAG) {
            self.etag = Some(etag);
        }
        if let Some(last_modified) = response_header_text(headers, reqwest::header::LAST_MODIFIED) {
            self.last_modified = Some(last_modified);
        }
        if let Some(content_type) = response_header_text(headers, reqwest::header::CONTENT_TYPE) {
            self.content_type = Some(content_type);
        }
        if headers.contains_key("referrer-policy") {
            self.referrer_policy = referrer_policy_from_headers(headers);
        }
        Some(self)
    }
}

impl NativeImageCacheEntry {
    fn from_response(image: NativeImage, headers: &HeaderMap, now: Instant) -> Option<Self> {
        if !document_cache_storage_allowed(headers) {
            return None;
        }
        Some(Self {
            image,
            fresh_until: document_cache_fresh_until(headers, now),
            etag: response_header_text(headers, reqwest::header::ETAG),
            last_modified: response_header_text(headers, reqwest::header::LAST_MODIFIED),
        })
    }

    fn is_fresh(&self, now: Instant) -> bool {
        self.fresh_until.is_none_or(|deadline| now < deadline)
    }

    fn refresh_from_not_modified(mut self, headers: &HeaderMap, now: Instant) -> Option<Self> {
        if !document_cache_storage_allowed(headers) {
            return None;
        }
        if cache_control_requires_revalidation(headers) {
            self.fresh_until = Some(now);
        } else if let Some(fresh_until) = document_cache_fresh_until(headers, now) {
            self.fresh_until = Some(fresh_until);
        }
        if let Some(etag) = response_header_text(headers, reqwest::header::ETAG) {
            self.etag = Some(etag);
        }
        if let Some(last_modified) = response_header_text(headers, reqwest::header::LAST_MODIFIED) {
            self.last_modified = Some(last_modified);
        }
        Some(self)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct NativeDocumentCacheEntry {
    resource: NativeResource,
    fresh_until: Option<Instant>,
    etag: Option<String>,
    last_modified: Option<String>,
}

impl NativeDocumentCacheEntry {
    fn from_response(resource: NativeResource, headers: &HeaderMap, now: Instant) -> Option<Self> {
        if !document_cache_storage_allowed(headers) {
            return None;
        }
        Some(Self {
            resource,
            fresh_until: document_cache_fresh_until(headers, now),
            etag: response_header_text(headers, reqwest::header::ETAG),
            last_modified: response_header_text(headers, reqwest::header::LAST_MODIFIED),
        })
    }

    fn is_fresh(&self, now: Instant) -> bool {
        self.fresh_until.is_none_or(|deadline| now < deadline)
    }

    fn refresh_from_not_modified(mut self, headers: &HeaderMap, now: Instant) -> Option<Self> {
        if !document_cache_storage_allowed(headers) {
            return None;
        }
        if cache_control_requires_revalidation(headers) {
            self.fresh_until = Some(now);
        } else if let Some(fresh_until) = document_cache_fresh_until(headers, now) {
            self.fresh_until = Some(fresh_until);
        }
        if let Some(etag) = response_header_text(headers, reqwest::header::ETAG) {
            self.etag = Some(etag);
        }
        if let Some(last_modified) = response_header_text(headers, reqwest::header::LAST_MODIFIED) {
            self.last_modified = Some(last_modified);
        }
        Some(self)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct NativeCookie {
    name: String,
    value: String,
    domain: String,
    path: String,
    host_only: bool,
    secure: bool,
    http_only: bool,
    same_site: Option<String>,
    priority: Option<String>,
    expires_at: Option<Instant>,
    expires_at_unix_seconds: Option<u64>,
}

/// The browser parent owns this in-memory jar and may explicitly share its
/// handle with sibling parent-side frame/target engines. Ordinary `Clone`
/// snapshots it, keeping loader copies such as the SharedWorker coordinator
/// isolated unless sharing is deliberately requested.
#[derive(Debug, Default)]
pub struct NativeCookieJar(Arc<Mutex<Vec<NativeCookie>>>);

impl NativeCookieJar {
    fn lock(&self) -> MutexGuard<'_, Vec<NativeCookie>> {
        self.0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    fn snapshot(&self) -> Vec<NativeCookie> {
        self.lock().clone()
    }

    fn shared_clone(&self) -> Self {
        Self(Arc::clone(&self.0))
    }

    fn len(&self) -> usize {
        self.lock().len()
    }

    fn apply_changes(&self, changes: &[NativeCookieChange]) -> Result<(), NativeEngineError> {
        let mut state = NativeNetworkState {
            cookies: self.shared_clone(),
            ..NativeNetworkState::default()
        };
        for change in changes {
            match &change.cookie {
                Some(profile) => match NativeCookie::from_profile(profile.clone())? {
                    Some(cookie) => {
                        state.set_cookie_profile(cookie)?;
                    }
                    None => {
                        state.remove_cookie(&change.name, &change.domain, &change.path);
                    }
                },
                None => {
                    state.remove_cookie(&change.name, &change.domain, &change.path);
                }
            }
        }
        Ok(())
    }
}

impl Clone for NativeCookieJar {
    fn clone(&self) -> Self {
        Self(Arc::new(Mutex::new(self.snapshot())))
    }
}

impl PartialEq for NativeCookieJar {
    fn eq(&self, other: &Self) -> bool {
        self.snapshot() == other.snapshot()
    }
}

impl Eq for NativeCookieJar {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum NativeCookieSameSite {
    Strict,
    Lax,
    None,
}

fn normalize_cookie_same_site(value: Option<&str>) -> Result<Option<String>, NativeEngineError> {
    match value.map(str::trim) {
        None => Ok(None),
        Some(value) if value.eq_ignore_ascii_case("strict") => Ok(Some("Strict".into())),
        Some(value) if value.eq_ignore_ascii_case("lax") => Ok(Some("Lax".into())),
        Some(value) if value.eq_ignore_ascii_case("none") => Ok(Some("None".into())),
        Some(_) => Err(NativeEngineError::invalid(
            "native cookie SameSite",
            "must be Strict, Lax, or None",
        )),
    }
}

fn normalize_cookie_priority(value: Option<&str>) -> Result<Option<String>, NativeEngineError> {
    match value.map(str::trim) {
        None => Ok(None),
        Some(value) if value.eq_ignore_ascii_case("low") => Ok(Some("Low".into())),
        Some(value) if value.eq_ignore_ascii_case("medium") => Ok(Some("Medium".into())),
        Some(value) if value.eq_ignore_ascii_case("high") => Ok(Some("High".into())),
        Some(_) => Err(NativeEngineError::invalid(
            "native cookie priority",
            "must be Low, Medium, or High",
        )),
    }
}

fn cookie_same_site(value: Option<&str>) -> NativeCookieSameSite {
    match value {
        Some(value) if value.eq_ignore_ascii_case("strict") => NativeCookieSameSite::Strict,
        Some(value) if value.eq_ignore_ascii_case("none") => NativeCookieSameSite::None,
        _ => NativeCookieSameSite::Lax,
    }
}

impl NativeCookie {
    fn from_profile(profile: NativeCookieProfileEntry) -> Result<Option<Self>, NativeEngineError> {
        if profile.name.is_empty()
            || !valid_cookie_text(&profile.name, true)
            || !valid_cookie_text(&profile.value, false)
            || profile.value.contains(';')
            || profile.name.len().saturating_add(profile.value.len()) > MAX_NATIVE_COOKIE_BYTES
            || profile.domain.is_empty()
            || !valid_cookie_domain(&profile.domain)
            || profile.path.is_empty()
            || !profile.path.starts_with('/')
        {
            return Err(NativeEngineError::invalid(
                "native cookie profile",
                "contains an invalid cookie entry",
            ));
        }
        let same_site = normalize_cookie_same_site(profile.same_site.as_deref())?;
        if same_site
            .as_deref()
            .is_some_and(|value| value.eq_ignore_ascii_case("none"))
            && !profile.secure
        {
            return Err(NativeEngineError::invalid(
                "native cookie SameSite",
                "SameSite=None cookies must be Secure",
            ));
        }
        let priority = normalize_cookie_priority(profile.priority.as_deref())?;
        let expires_at = profile.expires_at_unix_seconds.map(|expires_at| {
            Instant::now() + Duration::from_secs(expires_at.saturating_sub(unix_time_seconds()))
        });
        if profile
            .expires_at_unix_seconds
            .is_some_and(|expires_at| expires_at <= unix_time_seconds())
        {
            return Ok(None);
        }
        Ok(Some(Self {
            name: profile.name,
            value: profile.value,
            domain: profile.domain,
            path: profile.path,
            host_only: profile.host_only,
            secure: profile.secure,
            http_only: profile.http_only,
            same_site,
            priority,
            expires_at,
            expires_at_unix_seconds: profile.expires_at_unix_seconds,
        }))
    }

    fn to_profile(&self) -> Option<NativeCookieProfileEntry> {
        if self
            .expires_at_unix_seconds
            .is_some_and(|expires_at| expires_at <= unix_time_seconds())
        {
            return None;
        }
        Some(NativeCookieProfileEntry {
            name: self.name.clone(),
            value: self.value.clone(),
            domain: self.domain.clone(),
            path: self.path.clone(),
            host_only: self.host_only,
            secure: self.secure,
            http_only: self.http_only,
            same_site: self.same_site.clone(),
            priority: self.priority.clone(),
            expires_at_unix_seconds: self.expires_at_unix_seconds,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum NativeIntegrityAlgorithm {
    Sha256,
    Sha384,
    Sha512,
}

impl NativeIntegrityAlgorithm {
    fn priority(self) -> u8 {
        match self {
            Self::Sha256 => 1,
            Self::Sha384 => 2,
            Self::Sha512 => 3,
        }
    }

    fn digest(self, body: &[u8]) -> Vec<u8> {
        match self {
            Self::Sha256 => Sha256::digest(body).to_vec(),
            Self::Sha384 => Sha384::digest(body).to_vec(),
            Self::Sha512 => Sha512::digest(body).to_vec(),
        }
    }

    fn digest_len(self) -> usize {
        match self {
            Self::Sha256 => 32,
            Self::Sha384 => 48,
            Self::Sha512 => 64,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct NativeIntegrityMetadata {
    algorithm: NativeIntegrityAlgorithm,
    digest: Vec<u8>,
}

fn parse_integrity_metadata(value: &str) -> Vec<NativeIntegrityMetadata> {
    value
        .split_ascii_whitespace()
        .filter_map(|token| {
            let (hash_expression, options) = token.split_once('?').unwrap_or((token, ""));
            if !options.bytes().all(|byte| (0x21..=0x7e).contains(&byte)) {
                return None;
            }
            let (algorithm, digest) = hash_expression.split_once('-')?;
            let algorithm = match algorithm.to_ascii_lowercase().as_str() {
                "sha256" => NativeIntegrityAlgorithm::Sha256,
                "sha384" => NativeIntegrityAlgorithm::Sha384,
                "sha512" => NativeIntegrityAlgorithm::Sha512,
                _ => return None,
            };
            let digest = base64::engine::general_purpose::STANDARD
                .decode(digest)
                .ok()?;
            (digest.len() == algorithm.digest_len())
                .then_some(NativeIntegrityMetadata { algorithm, digest })
        })
        .collect()
}

fn integrity_metadata_is_enforced(integrity: Option<&str>) -> bool {
    integrity.is_some_and(|value| !parse_integrity_metadata(value).is_empty())
}

/// Verify an element's SRI metadata against the raw response representation.
/// Unknown or malformed metadata is ignored as required for forward-compatible
/// hash algorithm negotiation; a recognized hash turns a mismatch into a
/// failed resource load.
pub(crate) fn subresource_integrity_matches(integrity: Option<&str>, body: &[u8]) -> bool {
    let Some(integrity) = integrity else {
        return true;
    };
    let metadata = parse_integrity_metadata(integrity);
    let Some(strongest_priority) = metadata
        .iter()
        .map(|entry| entry.algorithm.priority())
        .max()
    else {
        return true;
    };
    metadata
        .iter()
        .filter(|entry| entry.algorithm.priority() == strongest_priority)
        .any(|entry| entry.digest == entry.algorithm.digest(body))
}

fn subresource_credentials(crossorigin: Option<&str>) -> Option<bool> {
    crossorigin.map(|value| value.trim().eq_ignore_ascii_case("use-credentials"))
}

fn worker_module_crossorigin(credentials_mode: &str) -> Result<&'static str, NativeEngineError> {
    match credentials_mode {
        "omit" | "same-origin" => Ok("anonymous"),
        "include" => Ok("use-credentials"),
        _ => Err(NativeEngineError::invalid(
            "SharedWorker credentials mode",
            "must be omit, same-origin, or include",
        )),
    }
}

fn subresource_request_has_credentials(
    worker_credentials_mode: Option<&str>,
    crossorigin: Option<&str>,
    document_url: &Url,
    resource_url: &Url,
) -> bool {
    match worker_credentials_mode {
        Some("omit") => false,
        Some("same-origin") => document_url.origin() == resource_url.origin(),
        Some("include") => true,
        Some(_) => false,
        None => {
            document_url.origin() == resource_url.origin()
                || subresource_credentials(crossorigin).unwrap_or(false)
        }
    }
}

fn subresource_response_allowed(
    headers: &HeaderMap,
    document_url: &Url,
    resource_url: &Url,
    integrity: Option<&str>,
    crossorigin: Option<&str>,
) -> bool {
    if document_url.origin() == resource_url.origin() {
        return true;
    }
    if integrity_metadata_is_enforced(integrity) && crossorigin.is_none() {
        return false;
    }
    match subresource_credentials(crossorigin) {
        Some(credentials) => {
            cors_response_allowed(headers, document_url, resource_url, credentials)
        }
        None => true,
    }
}

fn canonical_file_roots(roots: &[PathBuf]) -> Result<Vec<PathBuf>, NativeEngineError> {
    let mut canonical_roots = Vec::with_capacity(roots.len());
    for root in roots {
        let canonical = fs::canonicalize(root).map_err(|_| {
            NativeEngineError::invalid("allowed file root", "must point to an existing directory")
        })?;
        if !canonical.is_dir() {
            return Err(NativeEngineError::invalid(
                "allowed file root",
                "must point to an existing directory",
            ));
        }
        if !canonical_roots.iter().any(|current| current == &canonical) {
            canonical_roots.push(canonical);
        }
    }
    Ok(canonical_roots)
}

fn file_url_path(url: &Url, resource_name: &str) -> Result<PathBuf, NativeEngineError> {
    if !url.scheme().eq_ignore_ascii_case("file") {
        return Err(NativeEngineError::UnsupportedUrl {
            reason: format!("{resource_name} URL must use the file scheme"),
        });
    }
    if url
        .host_str()
        .is_some_and(|host| !host.eq_ignore_ascii_case("localhost"))
    {
        return Err(NativeEngineError::UnsupportedUrl {
            reason: format!("{resource_name} URL host must be empty or localhost"),
        });
    }
    url.to_file_path()
        .map_err(|_| NativeEngineError::UnsupportedUrl {
            reason: format!("{resource_name} URL could not be converted to a local path"),
        })
}

fn read_bounded_file(
    path: &Path,
    max_bytes: usize,
    resource_name: &str,
) -> Result<Vec<u8>, NativeEngineError> {
    let mut file = File::open(path).map_err(|_| NativeEngineError::UnsupportedUrl {
        reason: format!("{resource_name} is not readable"),
    })?;
    let read_limit = u64::try_from(max_bytes.saturating_add(1)).unwrap_or(u64::MAX);
    let mut bytes = Vec::with_capacity(max_bytes.min(8192));
    file.by_ref()
        .take(read_limit)
        .read_to_end(&mut bytes)
        .map_err(|_| NativeEngineError::UnsupportedUrl {
            reason: format!("{resource_name} could not be read"),
        })?;
    if bytes.len() > max_bytes {
        return Err(NativeEngineError::limit(
            resource_name,
            max_bytes,
            bytes.len(),
        ));
    }
    Ok(bytes)
}

fn file_media_type(path: &Path) -> Option<&'static str> {
    let extension = path.extension()?.to_str()?.to_ascii_lowercase();
    match extension.as_str() {
        "mp3" => Some("audio/mpeg"),
        "ogg" | "oga" | "opus" => Some("audio/ogg"),
        "wav" | "wave" => Some("audio/wav"),
        "weba" => Some("audio/webm"),
        "mp4" | "m4a" | "m4v" => Some(if extension == "m4v" {
            "video/mp4"
        } else {
            "audio/mp4"
        }),
        "ogv" => Some("video/ogg"),
        "webm" => Some("video/webm"),
        "m3u8" => Some("application/vnd.apple.mpegurl"),
        _ => None,
    }
}

fn file_image_media_type(path: &Path) -> Option<&'static str> {
    let extension = path.extension()?.to_str()?.to_ascii_lowercase();
    match extension.as_str() {
        "png" => Some("image/png"),
        "apng" => Some("image/apng"),
        "jpg" | "jpeg" => Some("image/jpeg"),
        "webp" => Some("image/webp"),
        "gif" => Some("image/gif"),
        "svg" => Some("image/svg+xml"),
        _ => None,
    }
}

struct NativeCspViolationContext<'a> {
    policy: &'a NativeCspPolicy,
    declaration: &'a NativeCspDeclaration,
    document_url: &'a Url,
}

struct NativeCspUrlCheck<'a> {
    policy: &'a NativeCspPolicy,
    kind: NativeSubresourceKind,
    document_url: &'a Url,
    resource_url: &'a Url,
    parser_inserted: bool,
    nonce: Option<&'a str>,
    ignore_path: bool,
}

struct NativeCorsPreflightRequest<'a> {
    client: &'a reqwest::Client,
    document_url: &'a Url,
    target_url: &'a Url,
    method: &'a NativeFetchMethod,
    requested_headers: &'a [String],
    credentials: bool,
    referrer: Option<&'a str>,
}

pub(crate) struct NativeScriptLoadRequest<'a> {
    pub(crate) document_url: &'a str,
    pub(crate) referrer_url: Option<&'a str>,
    pub(crate) href: &'a str,
    pub(crate) max_source_bytes: usize,
    pub(crate) subresource_kind: NativeSubresourceKind,
    pub(crate) parser_inserted: bool,
    pub(crate) nonce: Option<&'a str>,
    pub(crate) integrity: Option<&'a str>,
    pub(crate) crossorigin: Option<&'a str>,
    pub(crate) object_url: Option<&'a NativeObjectUrlResource>,
    pub(crate) module_type: Option<NativeModuleResourceType>,
    pub(crate) worker_credentials_mode: Option<&'a str>,
    pub(crate) referrer_policy: Option<NativeFetchReferrerPolicy>,
}

impl NativeResourceLoader {
    pub(crate) fn new(config: &NativeEngineConfig) -> Result<Self, NativeEngineError> {
        Self::new_with_cookie_jar(config, None)
    }

    pub(crate) fn new_with_shared_cookie_jar(
        config: &NativeEngineConfig,
        cookie_jar: NativeCookieJar,
    ) -> Result<Self, NativeEngineError> {
        Self::new_with_cookie_jar(config, Some(cookie_jar))
    }

    fn new_with_cookie_jar(
        config: &NativeEngineConfig,
        shared_cookie_jar: Option<NativeCookieJar>,
    ) -> Result<Self, NativeEngineError> {
        config.validate()?;
        let fixtures = config
            .fixtures
            .iter()
            .map(|fixture| (fixture.url.clone(), fixture.html.clone()))
            .collect();
        let allowed_file_roots = canonical_file_roots(&config.allowed_file_roots)?;
        let network = if let Some(cookie_jar) = shared_cookie_jar {
            NativeNetworkState {
                cookies: cookie_jar,
                ..NativeNetworkState::default()
            }
        } else {
            NativeNetworkState::from_profile(load_cookie_profile(config.storage_path.as_deref())?)?
        };
        Ok(Self {
            fixtures,
            allowed_file_roots,
            max_document_bytes: config.limits.max_document_bytes,
            network,
            environment: NativeEnvironmentOverrides::default(),
            cookie_changes: Vec::new(),
            csp_violations: Vec::new(),
        })
    }

    pub(crate) fn for_content_process(
        max_document_bytes: usize,
        _storage_path: Option<&std::path::Path>,
        allowed_file_roots: &[PathBuf],
    ) -> Result<Self, NativeEngineError> {
        if max_document_bytes == 0 || max_document_bytes > MAX_NATIVE_DOCUMENT_BYTES {
            return Err(NativeEngineError::invalid(
                "content-process document limit",
                format!("must be between 1 and {MAX_NATIVE_DOCUMENT_BYTES}"),
            ));
        }
        let allowed_file_roots = canonical_file_roots(allowed_file_roots)?;
        Ok(Self {
            fixtures: BTreeMap::new(),
            allowed_file_roots,
            max_document_bytes,
            // The parent keeps the durable cookie jar. Content-process cookie
            // state is disabled entirely: even an accidentally unbrokered
            // request cannot read or create a transient child cookie.
            network: NativeNetworkState {
                cookie_authority_enabled: false,
                ..NativeNetworkState::default()
            },
            environment: NativeEnvironmentOverrides::default(),
            cookie_changes: Vec::new(),
            csp_violations: Vec::new(),
        })
    }

    pub(crate) fn allowed_file_roots(&self) -> &[PathBuf] {
        &self.allowed_file_roots
    }

    fn allowed_file_path(
        &self,
        url: &Url,
        resource_name: &str,
    ) -> Result<PathBuf, NativeEngineError> {
        let requested_path = file_url_path(url, resource_name)?;
        let canonical_path =
            fs::canonicalize(&requested_path).map_err(|_| NativeEngineError::UnsupportedUrl {
                reason: format!("{resource_name} is not an accessible local file"),
            })?;
        if !canonical_path.is_file() {
            return Err(NativeEngineError::UnsupportedUrl {
                reason: format!("{resource_name} must resolve to a regular file"),
            });
        }
        if !self
            .allowed_file_roots
            .iter()
            .any(|root| canonical_path.starts_with(root))
        {
            return Err(NativeEngineError::UnsupportedUrl {
                reason: format!("{resource_name} is outside the configured file roots"),
            });
        }
        Ok(canonical_path)
    }

    fn allowed_file_root_for_path(&self, path: &Path) -> Option<&Path> {
        self.allowed_file_roots
            .iter()
            .filter(|root| path.starts_with(root))
            .max_by_key(|root| root.components().count())
            .map(PathBuf::as_path)
    }

    fn local_file_subresource_path(
        &self,
        document_url: &str,
        href: &str,
        resource_name: &str,
    ) -> Result<Option<(Url, PathBuf)>, NativeEngineError> {
        let document_url = Url::parse(without_fragment(document_url)).map_err(|_| {
            NativeEngineError::UnsupportedUrl {
                reason: format!("{resource_name} owner URL is not valid URL syntax"),
            }
        })?;
        reject_credentials(&document_url)?;
        if !is_file_url(document_url.as_str()) {
            return Ok(None);
        }
        let Some(target_url) = resolve_local_file_subresource_url(&document_url, href)? else {
            return Ok(None);
        };
        let path = self.allowed_file_path(&target_url, resource_name)?;
        Ok(Some((target_url, path)))
    }

    fn rooted_file_subresource_allowed(
        &self,
        document_url: &str,
        resource_url: &Url,
        resource_path: &Path,
        kind: NativeSubresourceKind,
        resource_name: &str,
    ) -> Result<bool, NativeEngineError> {
        let Some((policy_owner_url, Some(_))) =
            self.csp_document_owner(document_url, resource_name)?
        else {
            return Ok(true);
        };
        let Some(resource_root) = self.allowed_file_root_for_path(resource_path) else {
            return Ok(false);
        };
        let policy = self
            .network
            .document_policies
            .get(&cache_key(&policy_owner_url))
            .cloned()
            .unwrap_or_default();
        Ok(policy.allows_rooted_file_resource(
            kind,
            &policy_owner_url,
            resource_url,
            Some(resource_root),
        ))
    }

    pub(crate) fn set_environment(
        &mut self,
        environment: &NativeEnvironmentOverrides,
    ) -> Result<(), NativeEngineError> {
        environment.validate()?;
        self.environment = environment.clone();
        Ok(())
    }

    async fn before_request(&self, request_bytes: usize) -> Result<(), NativeEngineError> {
        if self.environment.network.offline {
            return Err(NativeEngineError::Network {
                operation: "native network emulation".into(),
                reason: "the native session is offline".into(),
            });
        }
        let delay = self.environment.network.request_delay(request_bytes);
        if !delay.is_zero() {
            tokio::time::sleep(delay).await;
        }
        Ok(())
    }

    async fn after_response_chunk(&self, bytes: usize) {
        let rate = self.environment.network.download_throughput_bytes;
        if rate > 0.0 {
            let delay = Duration::from_secs_f64(bytes as f64 / rate);
            if !delay.is_zero() {
                tokio::time::sleep(delay).await;
            }
        }
    }

    fn apply_environment_headers(
        &self,
        request: reqwest::RequestBuilder,
    ) -> Result<reqwest::RequestBuilder, NativeEngineError> {
        if !self.network.cookie_authority_enabled {
            return Err(NativeEngineError::Worker {
                operation: "content-process direct network request".into(),
                reason: "HTTP(S) requests must use the browser parent broker".into(),
            });
        }
        Ok(request
            .header(reqwest::header::USER_AGENT, self.environment.user_agent())
            .header(
                reqwest::header::ACCEPT_LANGUAGE,
                self.environment.accept_language(),
            ))
    }

    pub(crate) fn max_document_bytes(&self) -> usize {
        self.max_document_bytes
    }

    pub fn take_csp_violations(&mut self) -> Vec<NativeCspViolation> {
        std::mem::take(&mut self.csp_violations)
    }

    /// Merge state produced by an asynchronously-owned fetch back into the
    /// live loader. The request task starts from a snapshot so the content
    /// process can continue servicing upload demand; only fetch/preflight
    /// caches and observable cookie/CSP side effects cross that boundary.
    pub(crate) fn merge_fetch_task_state(
        &mut self,
        mut task_loader: NativeResourceLoader,
    ) -> Result<(), NativeEngineError> {
        if self.network.cookie_authority_enabled != task_loader.network.cookie_authority_enabled {
            return Err(NativeEngineError::Worker {
                operation: "merge resource-loader task state".into(),
                reason: "request task crossed the parent/content-process cookie authority boundary"
                    .into(),
            });
        }
        self.network
            .fetch_cache
            .extend(std::mem::take(&mut task_loader.network.fetch_cache));
        self.network
            .preflight_cache
            .extend(std::mem::take(&mut task_loader.network.preflight_cache));
        let cookie_changes = task_loader.take_cookie_changes();
        self.apply_cookie_changes(&cookie_changes)?;
        let csp_violations = task_loader.take_csp_violations();
        let next_len = self
            .csp_violations
            .len()
            .saturating_add(csp_violations.len());
        if next_len > MAX_NATIVE_CSP_VIOLATIONS {
            return Err(NativeEngineError::limit(
                "native CSP violations",
                MAX_NATIVE_CSP_VIOLATIONS,
                next_len,
            ));
        }
        self.csp_violations.extend(csp_violations);
        Ok(())
    }

    pub(crate) async fn load_media_async_with_object_url(
        &mut self,
        document_url: &str,
        src: &str,
        object_url: Option<&NativeObjectUrlResource>,
    ) -> Result<Option<NativeMediaMetadata>, NativeEngineError> {
        validate_url_text("document URL", document_url)?;
        validate_url_text("media URL", src)?;
        let document_url = Url::parse(without_fragment(document_url)).map_err(|_| {
            NativeEngineError::UnsupportedUrl {
                reason: "media owner URL is not valid URL syntax".into(),
            }
        })?;
        reject_credentials(&document_url)?;
        let Some(target_url) = resolve_media_url(&document_url, src)? else {
            return Ok(None);
        };
        if target_url.scheme().eq_ignore_ascii_case("file") {
            if !is_file_url(document_url.as_str()) {
                return Ok(None);
            }
            return self.load_file_media_url(&target_url);
        }
        if !is_network_url(document_url.as_str())
            && !target_url.scheme().eq_ignore_ascii_case("data")
        {
            return Ok(None);
        }
        if !mixed_content_allowed(&document_url, &target_url) {
            return Ok(None);
        }
        let policy = self
            .network
            .document_policies
            .get(&cache_key(&document_url))
            .cloned()
            .unwrap_or_default();
        self.record_report_only_url_violations(
            &policy,
            NativeSubresourceKind::Media,
            &document_url,
            &target_url,
        );
        if !policy.allows(NativeSubresourceKind::Media, &document_url, &target_url) {
            return Ok(None);
        }
        if target_url.scheme().eq_ignore_ascii_case("data") {
            return data_media_metadata(&target_url);
        }
        if target_url.scheme().eq_ignore_ascii_case("blob") {
            let Some(object_url) = object_url else {
                return Ok(None);
            };
            NativeOrigin::from_blob_url(without_fragment(target_url.as_str()))?;
            return media_metadata_from_bytes(object_url.content_type.as_deref(), &object_url.body);
        }
        if !is_network_url(target_url.as_str()) {
            return Ok(None);
        }

        let client = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(NATIVE_NETWORK_TIMEOUT)
            .build()
            .map_err(|error| network_error("media client construction", error))?;
        let mut current_url = target_url;
        let mut request_referrer = normalize_referrer(Some(document_url.as_str()), &current_url)?;
        let mut redirects = 0;
        let mut pending_cookies = Vec::new();
        let response = loop {
            let mut request_url = current_url.clone();
            request_url.set_fragment(None);
            let mut request = self.apply_environment_headers(client.get(request_url).header(
                reqwest::header::ACCEPT,
                "audio/*,video/*,application/ogg,application/vnd.apple.mpegurl,*/*;q=0.5",
            ))?;
            if let Some(referrer) = request_referrer.as_deref() {
                request = request.header(reqwest::header::REFERER, referrer);
            }
            if document_url.origin() == current_url.origin()
                && let Some(cookie) = self.network.cookie_header_for_request(
                    &current_url,
                    Some(&document_url),
                    false,
                    NativeNavigationMethod::Get,
                )
            {
                request = request.header(reqwest::header::COOKIE, cookie);
            }
            self.before_request(0).await?;
            let response = request
                .send()
                .await
                .map_err(|error| network_error("media subresource request", error))?;
            for value in response
                .headers()
                .get_all(reqwest::header::SET_COOKIE)
                .iter()
            {
                if let Ok(cookie) = value.to_str() {
                    pending_cookies.push((current_url.clone(), cookie.to_owned()));
                }
            }
            if !is_http_redirect(response.status()) {
                break response;
            }
            if redirects >= MAX_NATIVE_NETWORK_REDIRECTS {
                return Ok(None);
            }
            let Some(location) = response
                .headers()
                .get(reqwest::header::LOCATION)
                .and_then(|value| value.to_str().ok())
                .filter(|value| !value.is_empty())
            else {
                return Ok(None);
            };
            let next_url = current_url
                .join(location)
                .map_err(|_| NativeEngineError::Network {
                    operation: "media redirect".into(),
                    reason: "media redirect location is not valid URL syntax".into(),
                })?;
            reject_credentials(&next_url)?;
            self.record_report_only_url_violations_for_redirect(
                &policy,
                NativeSubresourceKind::Media,
                &document_url,
                &next_url,
                true,
                None,
            );
            if !is_network_url(without_fragment(next_url.as_str()))
                || !mixed_content_allowed(&document_url, &next_url)
                || !policy.allows_redirect(NativeSubresourceKind::Media, &document_url, &next_url)
            {
                return Ok(None);
            }
            request_referrer = normalize_referrer(Some(current_url.as_str()), &next_url)?;
            current_url = next_url;
            redirects += 1;
        };
        if !response.status().is_success() {
            return Ok(None);
        }
        let content_type = response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .map(|value| {
                value
                    .to_str()
                    .map(str::to_owned)
                    .map_err(|_| NativeEngineError::Network {
                        operation: "media content-type validation".into(),
                        reason: "media content type is not valid ASCII".into(),
                    })
            })
            .transpose()?;
        let content_length = response.content_length();
        if content_length.is_some_and(|length| length > MAX_NATIVE_MEDIA_BYTES as u64) {
            return Ok(None);
        }
        let mut stream = response.bytes_stream();
        let mut bytes = Vec::with_capacity(
            content_length
                .unwrap_or_default()
                .min(MAX_NATIVE_MEDIA_BYTES as u64) as usize,
        );
        while let Some(chunk) = stream.next().await {
            let chunk = chunk.map_err(|error| network_error("media subresource body", error))?;
            self.after_response_chunk(chunk.len()).await;
            let next_len = bytes.len().saturating_add(chunk.len());
            if next_len > MAX_NATIVE_MEDIA_BYTES {
                return Ok(None);
            }
            bytes.extend_from_slice(&chunk);
        }
        let metadata = media_metadata_from_bytes(content_type.as_deref(), &bytes)?;
        if metadata.is_some() {
            for (cookie_url, cookie) in pending_cookies {
                self.cookie_changes
                    .extend(self.network.store_cookie(&cookie_url, &cookie));
            }
        }
        Ok(metadata)
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

fn resolve_video_poster_url(
    document_url: &Url,
    href: &str,
) -> Result<Option<Url>, NativeEngineError> {
    let target_url = document_url
        .join(href)
        .map_err(|_| NativeEngineError::UnsupportedUrl {
            reason: "video poster URL could not be resolved against its owner Document".into(),
        })?;
    if !is_network_url(without_fragment(target_url.as_str())) {
        return Ok(None);
    }
    Ok(Some(target_url))
}

fn clear_url_credentials(url: &mut Url) -> Result<(), NativeEngineError> {
    url.set_username("")
        .and_then(|()| url.set_password(None))
        .map_err(|()| NativeEngineError::UnsupportedUrl {
            reason: "video poster URL credentials could not be removed from the request URL".into(),
        })
}

fn basic_authorization_from_url(url: &Url) -> Result<Option<String>, NativeEngineError> {
    if url.username().is_empty() && url.password().is_none() {
        return Ok(None);
    }
    let mut user_pass = percent_decode_url_credential(url.username())?;
    user_pass.push(b':');
    if let Some(password) = url.password() {
        user_pass.extend(percent_decode_url_credential(password)?);
    }
    if user_pass.len() > MAX_NATIVE_FETCH_HEADER_VALUE_BYTES {
        return Err(NativeEngineError::limit(
            "video poster URL credentials",
            MAX_NATIVE_FETCH_HEADER_VALUE_BYTES,
            user_pass.len(),
        ));
    }
    Ok(Some(format!(
        "Basic {}",
        base64::engine::general_purpose::STANDARD.encode(user_pass)
    )))
}

fn percent_decode_url_credential(value: &str) -> Result<Vec<u8>, NativeEngineError> {
    let bytes = value.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            let Some(high) = bytes
                .get(index + 1)
                .and_then(|byte| (*byte as char).to_digit(16))
            else {
                return Err(NativeEngineError::UnsupportedUrl {
                    reason: "video poster URL contains malformed credentials".into(),
                });
            };
            let Some(low) = bytes
                .get(index + 2)
                .and_then(|byte| (*byte as char).to_digit(16))
            else {
                return Err(NativeEngineError::UnsupportedUrl {
                    reason: "video poster URL contains malformed credentials".into(),
                });
            };
            decoded.push(((high << 4) | low) as u8);
            index += 3;
        } else {
            decoded.push(bytes[index]);
            index += 1;
        }
    }
    Ok(decoded)
}

fn resolve_subresource_url_with_blob(
    document_url: &Url,
    href: &str,
) -> Result<Option<Url>, NativeEngineError> {
    let target_url = document_url
        .join(href)
        .map_err(|_| NativeEngineError::UnsupportedUrl {
            reason: "subresource URL could not be resolved against the document".into(),
        })?;
    reject_credentials(&target_url)?;
    if !is_network_url(without_fragment(target_url.as_str()))
        && !target_url.scheme().eq_ignore_ascii_case("blob")
    {
        return Ok(None);
    }
    Ok(Some(target_url))
}

fn resolve_local_file_subresource_url(
    document_url: &Url,
    href: &str,
) -> Result<Option<Url>, NativeEngineError> {
    let target_url = document_url
        .join(href)
        .map_err(|_| NativeEngineError::UnsupportedUrl {
            reason: "file subresource URL could not be resolved against the document".into(),
        })?;
    reject_credentials(&target_url)?;
    if !target_url.scheme().eq_ignore_ascii_case("file") {
        return Ok(None);
    }
    Ok(Some(target_url))
}

fn resolve_media_url(document_url: &Url, href: &str) -> Result<Option<Url>, NativeEngineError> {
    let target_url = document_url
        .join(href)
        .map_err(|_| NativeEngineError::UnsupportedUrl {
            reason: "media URL could not be resolved against the document".into(),
        })?;
    reject_credentials(&target_url)?;
    if !is_network_url(without_fragment(target_url.as_str()))
        && !target_url.scheme().eq_ignore_ascii_case("blob")
        && !target_url.scheme().eq_ignore_ascii_case("data")
        && !target_url.scheme().eq_ignore_ascii_case("file")
    {
        return Ok(None);
    }
    Ok(Some(target_url))
}

pub(crate) fn mixed_content_allowed(document_url: &Url, resource_url: &Url) -> bool {
    !(document_url.scheme().eq_ignore_ascii_case("https")
        && resource_url.scheme().eq_ignore_ascii_case("http"))
}

pub(crate) fn cors_origin_header(
    document_url: &Url,
    resource_url: &Url,
    mode: NativeCorsMode,
) -> Option<String> {
    (mode == NativeCorsMode::Cors && document_url.origin() != resource_url.origin())
        .then(|| document_url.origin().ascii_serialization())
}

fn is_simple_fetch_content_type(value: &str) -> bool {
    matches!(
        value
            .split(';')
            .next()
            .unwrap_or_default()
            .trim()
            .to_ascii_lowercase()
            .as_str(),
        "application/x-www-form-urlencoded" | "multipart/form-data" | "text/plain"
    )
}

fn is_forbidden_fetch_request_header(name: &str) -> bool {
    matches!(
        name,
        "accept-charset"
            | "accept-encoding"
            | "access-control-request-headers"
            | "access-control-request-method"
            | "connection"
            | "content-length"
            | "cookie"
            | "cookie2"
            | "date"
            | "dnt"
            | "expect"
            | "host"
            | "keep-alive"
            | "origin"
            | "referer"
            | "te"
            | "trailer"
            | "transfer-encoding"
            | "upgrade"
            | "user-agent"
            | "via"
    ) || name.starts_with("proxy-")
        || name.starts_with("sec-")
}

fn validate_fetch_request_headers(
    headers: &BTreeMap<String, String>,
) -> Result<BTreeMap<String, String>, NativeEngineError> {
    if headers.len() > MAX_NATIVE_FETCH_HEADERS {
        return Err(NativeEngineError::limit(
            "fetch request headers",
            MAX_NATIVE_FETCH_HEADERS,
            headers.len(),
        ));
    }
    let mut normalized: BTreeMap<String, String> = BTreeMap::new();
    let mut total_bytes = 0usize;
    for (name, value) in headers {
        if name.len() > MAX_NATIVE_FETCH_HEADER_NAME_BYTES {
            return Err(NativeEngineError::limit(
                "fetch request header name",
                MAX_NATIVE_FETCH_HEADER_NAME_BYTES,
                name.len(),
            ));
        }
        let normalized_name = name.to_ascii_lowercase();
        if is_forbidden_fetch_request_header(&normalized_name) {
            return Err(NativeEngineError::invalid(
                "fetch request header",
                "name is forbidden",
            ));
        }
        if normalized_name == "content-type" {
            return Err(NativeEngineError::invalid(
                "fetch request header",
                "content-type must use the dedicated content type field",
            ));
        }
        reqwest::header::HeaderName::from_bytes(normalized_name.as_bytes()).map_err(|_| {
            NativeEngineError::invalid("fetch request header name", "must be a valid token")
        })?;
        if value.len() > MAX_NATIVE_FETCH_HEADER_VALUE_BYTES {
            return Err(NativeEngineError::limit(
                "fetch request header value",
                MAX_NATIVE_FETCH_HEADER_VALUE_BYTES,
                value.len(),
            ));
        }
        reqwest::header::HeaderValue::from_str(value).map_err(|_| {
            NativeEngineError::invalid(
                "fetch request header value",
                "must not contain controls or invalid bytes",
            )
        })?;
        total_bytes = total_bytes
            .saturating_add(normalized_name.len())
            .saturating_add(value.len());
        if total_bytes > MAX_NATIVE_FETCH_HEADER_BYTES {
            return Err(NativeEngineError::limit(
                "fetch request headers",
                MAX_NATIVE_FETCH_HEADER_BYTES,
                total_bytes,
            ));
        }
        normalized
            .entry(normalized_name)
            .and_modify(|existing| {
                existing.push_str(", ");
                existing.push_str(value);
            })
            .or_insert_with(|| value.clone());
    }
    Ok(normalized)
}

fn cors_safelisted_request_header(name: &str, value: &str) -> bool {
    match name {
        "accept" => {
            value.len() <= 128
                && !value.bytes().any(|byte| {
                    matches!(
                        byte,
                        0x00..=0x08
                            | 0x0a..=0x1f
                            | 0x7f
                            | b'"'
                            | b'('
                            | b')'
                            | b':'
                            | b'<'
                            | b'>'
                            | b'?'
                            | b'@'
                            | b'['
                            | b'\\'
                            | b']'
                            | b'{'
                            | b'}'
                    )
                })
        }
        "accept-language" | "content-language" => {
            value.len() <= 128
                && value.bytes().all(|byte| {
                    byte.is_ascii_alphanumeric()
                        || matches!(byte, b' ' | b'*' | b',' | b'-' | b'.' | b';' | b'=')
                })
        }
        "range" => {
            let Some(range) = value.strip_prefix("bytes=") else {
                return false;
            };
            let Some((start, end)) = range.split_once('-') else {
                return false;
            };
            !start.is_empty()
                && start.bytes().all(|byte| byte.is_ascii_digit())
                && (end.is_empty() || end.bytes().all(|byte| byte.is_ascii_digit()))
        }
        _ => false,
    }
}

fn cors_preflight_request_headers(
    content_type: Option<&str>,
    request_headers: &BTreeMap<String, String>,
) -> Vec<String> {
    let mut requested = request_headers
        .iter()
        .filter(|(name, value)| !cors_safelisted_request_header(name, value))
        .map(|(name, _)| name.clone())
        .collect::<Vec<_>>();
    if content_type.is_some_and(|value| !is_simple_fetch_content_type(value)) {
        requested.push("content-type".into());
    }
    requested.sort_unstable();
    requested.dedup();
    requested
}

fn cors_preflight_cache_key(
    document_url: &Url,
    resource_url: &Url,
    method: &str,
    requested_headers: &[String],
    credentials: bool,
) -> String {
    format!(
        "{}|{}|{}|{}|{}",
        document_url.origin().ascii_serialization(),
        without_fragment(resource_url.as_str()),
        method,
        if credentials { "include" } else { "omit" },
        requested_headers.join(",")
    )
}

fn cors_preflight_cache_age(headers: &HeaderMap) -> Option<Duration> {
    let seconds = headers
        .get("access-control-max-age")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.trim().parse::<u64>().ok())?;
    Some(Duration::from_secs(
        seconds.min(MAX_NATIVE_PREFLIGHT_CACHE_AGE.as_secs()),
    ))
}

fn cors_preflight_response_allowed(
    headers: &HeaderMap,
    document_url: &Url,
    resource_url: &Url,
    method: &str,
    requested_headers: &[String],
    credentials: bool,
) -> bool {
    if !cors_response_allowed(headers, document_url, resource_url, credentials) {
        return false;
    }
    let method_allowed = header_contains_token(
        headers,
        "access-control-allow-methods",
        method,
        !credentials,
    );
    let headers_allowed = requested_headers.iter().all(|header| {
        header_contains_token(
            headers,
            "access-control-allow-headers",
            header,
            !credentials,
        )
    });
    method_allowed && headers_allowed
}

fn header_contains_token(
    headers: &HeaderMap,
    name: &str,
    expected: &str,
    allow_wildcard: bool,
) -> bool {
    headers
        .get(name)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| {
            value.split(',').any(|token| {
                let token = token.trim();
                token.eq_ignore_ascii_case(expected) || (allow_wildcard && token == "*")
            })
        })
}

pub(crate) fn cors_response_allowed(
    headers: &HeaderMap,
    document_url: &Url,
    resource_url: &Url,
    credentials: bool,
) -> bool {
    if document_url.origin() == resource_url.origin() {
        return true;
    }
    let Some(allow_origin) = headers
        .get(reqwest::header::ACCESS_CONTROL_ALLOW_ORIGIN)
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
    else {
        return false;
    };
    let expected_origin = document_url.origin().ascii_serialization();
    let origin_allowed = allow_origin == expected_origin || (!credentials && allow_origin == "*");
    origin_allowed
        && (!credentials
            || headers
                .get(reqwest::header::ACCESS_CONTROL_ALLOW_CREDENTIALS)
                .and_then(|value| value.to_str().ok())
                .is_some_and(|value| value.trim().eq_ignore_ascii_case("true")))
}

fn exposed_response_headers(
    headers: &HeaderMap,
    same_origin: bool,
    credentials: bool,
) -> Result<Vec<(String, String)>, NativeEngineError> {
    let mut exposed = Vec::new();
    let mut total_bytes = 0usize;
    for (name, value) in headers {
        let name = name.as_str();
        if matches!(name, "set-cookie" | "set-cookie2")
            || (!same_origin && !cors_response_header_exposed(headers, name, credentials))
        {
            continue;
        }
        let Ok(value) = value.to_str() else {
            continue;
        };
        if name.len() > MAX_NATIVE_RESPONSE_HEADER_NAME_BYTES {
            return Err(NativeEngineError::limit(
                "response header name",
                MAX_NATIVE_RESPONSE_HEADER_NAME_BYTES,
                name.len(),
            ));
        }
        if value.len() > MAX_NATIVE_RESPONSE_HEADER_VALUE_BYTES {
            return Err(NativeEngineError::limit(
                "response header value",
                MAX_NATIVE_RESPONSE_HEADER_VALUE_BYTES,
                value.len(),
            ));
        }
        let next_bytes = total_bytes
            .saturating_add(name.len())
            .saturating_add(value.len());
        if next_bytes > MAX_NATIVE_RESPONSE_HEADER_BYTES {
            return Err(NativeEngineError::limit(
                "response headers",
                MAX_NATIVE_RESPONSE_HEADER_BYTES,
                next_bytes,
            ));
        }
        if exposed.len() >= MAX_NATIVE_RESPONSE_HEADERS {
            return Err(NativeEngineError::limit(
                "response headers",
                MAX_NATIVE_RESPONSE_HEADERS,
                exposed.len().saturating_add(1),
            ));
        }
        total_bytes = next_bytes;
        exposed.push((name.to_owned(), value.to_owned()));
    }
    Ok(exposed)
}

fn cors_response_header_exposed(headers: &HeaderMap, name: &str, credentials: bool) -> bool {
    if matches!(
        name,
        "cache-control"
            | "content-language"
            | "content-length"
            | "content-type"
            | "expires"
            | "last-modified"
            | "pragma"
    ) {
        return true;
    }
    let allow_wildcard = !credentials;
    headers
        .get_all("access-control-expose-headers")
        .iter()
        .filter_map(|value| value.to_str().ok())
        .flat_map(|value| value.split(','))
        .map(str::trim)
        .any(|token| token == "*" && allow_wildcard || token.eq_ignore_ascii_case(name))
}

fn content_type_is(
    value: Option<&reqwest::header::HeaderValue>,
    expected: &str,
) -> Result<bool, NativeEngineError> {
    let Some(value) = value else {
        return Ok(false);
    };
    let value = value.to_str().map_err(|_| NativeEngineError::Network {
        operation: "HTTP content-type validation".into(),
        reason: "HTTP content type is not valid ASCII".into(),
    })?;
    Ok(value
        .split(';')
        .next()
        .unwrap_or_default()
        .trim()
        .eq_ignore_ascii_case(expected))
}

fn stylesheet_content_type_text_allowed(value: &str) -> bool {
    value
        .split(';')
        .next()
        .unwrap_or_default()
        .trim()
        .eq_ignore_ascii_case("text/css")
}

fn supported_image_media_type(
    value: Option<&reqwest::header::HeaderValue>,
) -> Result<Option<&'static str>, NativeEngineError> {
    let Some(value) = value else {
        return Ok(None);
    };
    let value = value.to_str().map_err(|_| NativeEngineError::Network {
        operation: "HTTP image content-type validation".into(),
        reason: "HTTP image content type is not valid ASCII".into(),
    })?;
    let media_type = value.split(';').next().unwrap_or_default().trim();
    Ok(supported_image_media_type_text(media_type))
}

fn supported_image_media_type_text(value: &str) -> Option<&'static str> {
    if value.eq_ignore_ascii_case("image/png") {
        Some("image/png")
    } else if value.eq_ignore_ascii_case("image/jpeg") {
        Some("image/jpeg")
    } else if value.eq_ignore_ascii_case("image/webp") {
        Some("image/webp")
    } else if value.eq_ignore_ascii_case("image/gif") {
        Some("image/gif")
    } else if value.eq_ignore_ascii_case("image/apng") {
        Some("image/apng")
    } else if value.eq_ignore_ascii_case("image/svg+xml") {
        Some("image/svg+xml")
    } else {
        None
    }
}

fn media_metadata_from_bytes(
    declared_type: Option<&str>,
    bytes: &[u8],
) -> Result<Option<NativeMediaMetadata>, NativeEngineError> {
    if bytes.is_empty() {
        return Ok(None);
    }
    if bytes.len() > MAX_NATIVE_MEDIA_BYTES {
        return Err(NativeEngineError::limit(
            "native media resource",
            MAX_NATIVE_MEDIA_BYTES,
            bytes.len(),
        ));
    }
    let content_type = match declared_type
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        Some(declared_type) => supported_media_type_text(declared_type),
        None => sniff_media_type(bytes),
    };
    let Some(content_type) = content_type else {
        return Ok(None);
    };
    let audio_track = if content_type == "audio/wav" {
        decode_bounded_pcm_wav(bytes)?
    } else {
        None
    };
    if (content_type.starts_with("audio/")
        || content_type == "application/ogg"
        || content_type == "application/vnd.apple.mpegurl")
        && audio_track.is_none()
    {
        return Ok(None);
    }
    let duration_millis = audio_track.as_ref().map(|track| {
        let frames = track.pcm_s16le.len() / (usize::from(track.channels) * 2);
        (frames as u64).saturating_mul(1000) / u64::from(track.sample_rate)
    });
    let video_frames = if content_type == "video/webm" {
        decode_bounded_vp8_webm(bytes)?
    } else {
        Vec::new()
    };
    if content_type.starts_with("video/")
        && (content_type != "video/webm" || video_frames.is_empty())
    {
        return Ok(None);
    }
    let video_duration = if content_type == "video/webm" {
        let duration = matroska_demuxer::MatroskaFile::open(std::io::Cursor::new(bytes))
            .ok()
            .and_then(|file| {
                file.info().duration().map(|duration| {
                    (duration * file.info().timestamp_scale().get() as f64 / 1_000_000.0).max(0.0)
                        as u64
                })
            });
        duration.or_else(|| {
            video_frames
                .last()
                .map(|frame| frame.timestamp_millis.saturating_add(100))
        })
    } else {
        None
    };
    let duration_millis = if content_type == "video/webm" && video_frames.is_empty() {
        None
    } else {
        video_duration.or(duration_millis)
    };
    Ok(Some(NativeMediaMetadata {
        content_type: content_type.to_owned(),
        byte_length: bytes.len(),
        duration_millis,
        video_frames,
        audio_track,
    }))
}

pub(crate) const MAX_NATIVE_VIDEO_FRAME_COUNT: usize = 60;
pub(crate) const MAX_NATIVE_VIDEO_DIMENSION: u32 = 640;
pub(crate) const MAX_NATIVE_VIDEO_DECODED_BYTES: usize = 2 * 1024 * 1024;
pub(crate) const MAX_NATIVE_DOCUMENT_VIDEO_FRAMES: usize = 120;
pub(crate) const MAX_NATIVE_DOCUMENT_VIDEO_BYTES: usize = 4 * 1024 * 1024;
pub(crate) const MAX_NATIVE_AUDIO_DECODED_BYTES: usize = 2 * 1024 * 1024 - 44;
pub(crate) const MAX_NATIVE_DOCUMENT_AUDIO_DECODED_BYTES: usize = 4 * 1024 * 1024;
pub(crate) const MAX_NATIVE_AUDIO_DURATION_MILLIS: u64 = 30_000;

fn decode_bounded_pcm_wav(bytes: &[u8]) -> Result<Option<NativeAudioTrack>, NativeEngineError> {
    if bytes.len() < 12 || &bytes[..4] != b"RIFF" || &bytes[8..12] != b"WAVE" {
        return Ok(None);
    }
    let riff_bytes =
        usize::try_from(u32::from_le_bytes(bytes[4..8].try_into().map_err(
            |_| NativeEngineError::invalid("WAV", "invalid RIFF length"),
        )?))
        .unwrap_or(usize::MAX)
        .saturating_add(8);
    if riff_bytes > bytes.len() || riff_bytes < 12 {
        return Ok(None);
    }
    let mut offset = 12usize;
    let mut format: Option<(u16, u16, u32, u16, u16)> = None;
    let mut sample_bytes: Option<&[u8]> = None;
    while offset.checked_add(8).is_some_and(|end| end <= riff_bytes) {
        let chunk_len = usize::try_from(u32::from_le_bytes(
            bytes[offset + 4..offset + 8]
                .try_into()
                .expect("fixed WAV chunk length"),
        ))
        .unwrap_or(usize::MAX);
        let start = offset + 8;
        let Some(end) = start.checked_add(chunk_len) else {
            return Ok(None);
        };
        if end > riff_bytes {
            return Ok(None);
        }
        match &bytes[offset..offset + 4] {
            b"fmt " if chunk_len >= 16 => {
                format = Some((
                    u16::from_le_bytes([bytes[start], bytes[start + 1]]),
                    u16::from_le_bytes([bytes[start + 2], bytes[start + 3]]),
                    u32::from_le_bytes(
                        bytes[start + 4..start + 8]
                            .try_into()
                            .expect("fixed WAV sample rate"),
                    ),
                    u16::from_le_bytes(
                        bytes[start + 12..start + 14]
                            .try_into()
                            .expect("fixed WAV block alignment"),
                    ),
                    u16::from_le_bytes(
                        bytes[start + 14..start + 16]
                            .try_into()
                            .expect("fixed WAV bit depth"),
                    ),
                ));
            }
            b"data" => sample_bytes = Some(&bytes[start..end]),
            _ => {}
        }
        offset = end.saturating_add(chunk_len & 1);
    }
    let Some((encoding, channels, sample_rate, block_align, bits_per_sample)) = format else {
        return Ok(None);
    };
    let Some(sample_bytes) = sample_bytes else {
        return Ok(None);
    };
    if encoding != 1
        || !(channels == 1 || channels == 2)
        || !(8_000..=96_000).contains(&sample_rate)
        || !matches!(bits_per_sample, 8 | 16 | 24 | 32)
    {
        return Ok(None);
    }
    let bytes_per_sample = usize::from(bits_per_sample / 8);
    let expected_align = usize::from(channels).saturating_mul(bytes_per_sample);
    if usize::from(block_align) != expected_align || sample_bytes.len() % expected_align != 0 {
        return Ok(None);
    }
    let decoded_len = sample_bytes
        .len()
        .checked_div(bytes_per_sample)
        .and_then(|samples| samples.checked_mul(2))
        .unwrap_or(usize::MAX);
    if decoded_len > MAX_NATIVE_AUDIO_DECODED_BYTES {
        return Err(NativeEngineError::limit(
            "decoded native audio samples",
            MAX_NATIVE_AUDIO_DECODED_BYTES,
            decoded_len,
        ));
    }
    let frame_count = sample_bytes.len() / expected_align;
    let duration_millis = (frame_count as u64).saturating_mul(1000) / u64::from(sample_rate);
    if duration_millis > MAX_NATIVE_AUDIO_DURATION_MILLIS {
        return Ok(None);
    }
    let mut pcm_s16le = Vec::with_capacity(decoded_len);
    for sample in sample_bytes.chunks_exact(bytes_per_sample) {
        let value = match bits_per_sample {
            8 => (i32::from(sample[0]) - 128) << 8,
            16 => i32::from(i16::from_le_bytes([sample[0], sample[1]])),
            24 => {
                let sign = if sample[2] & 0x80 == 0 { 0 } else { 0xff };
                let raw = i32::from_le_bytes([sample[0], sample[1], sample[2], sign]);
                raw >> 8
            }
            32 => i32::from_le_bytes(sample.try_into().expect("fixed 32-bit WAV sample")) >> 16,
            _ => unreachable!("WAV bit depth validated above"),
        };
        pcm_s16le.extend_from_slice(&(value as i16).to_le_bytes());
    }
    Ok(Some(NativeAudioTrack {
        channels,
        sample_rate,
        pcm_s16le,
    }))
}

fn decode_bounded_vp8_webm(bytes: &[u8]) -> Result<Vec<NativeVideoFrame>, NativeEngineError> {
    use std::io::Cursor;
    use zvidlib_core::{
        CancellationToken, Codec, CodecProfile, ColorRange, EncodedVideoSample, ErrorKind,
        FrameIndex, HardwarePreference, Limits, PixelFormat, VideoDecoderConfig,
        VideoDecoderFactory, VideoDimensions,
    };
    use zvidlib_vp8::native_vp8_video_decoder_factory;

    let mut file = match matroska_demuxer::MatroskaFile::open(Cursor::new(bytes)) {
        Ok(file) if file.ebml_header().doc_type().eq_ignore_ascii_case("webm") => file,
        _ => return Ok(Vec::new()),
    };
    let Some((track_number, width, height)) = file
        .tracks()
        .iter()
        .find(|track| track.codec_id() == "V_VP8" && track.video().is_some())
        .and_then(|track| {
            let video = track.video()?;
            Some((
                track.track_number().get(),
                u32::try_from(video.pixel_width().get()).ok()?,
                u32::try_from(video.pixel_height().get()).ok()?,
            ))
        })
    else {
        return Ok(Vec::new());
    };
    if width == 0
        || height == 0
        || width > MAX_NATIVE_VIDEO_DIMENSION
        || height > MAX_NATIVE_VIDEO_DIMENSION
    {
        return Ok(Vec::new());
    }
    let limits = Limits {
        max_width: MAX_NATIVE_VIDEO_DIMENSION,
        max_height: MAX_NATIVE_VIDEO_DIMENSION,
        max_allocation_bytes: MAX_NATIVE_VIDEO_DECODED_BYTES as u64,
        max_cached_frames: MAX_NATIVE_VIDEO_FRAME_COUNT as u32,
        max_decode_samples_per_seek: MAX_NATIVE_VIDEO_FRAME_COUNT as u32,
        ..Limits::default()
    };
    let dimensions = VideoDimensions::new(width, height, &limits)
        .map_err(|error| NativeEngineError::invalid("native VP8 dimensions", error.to_string()))?;
    let config = VideoDecoderConfig {
        codec: Codec::Vp8,
        profile: CodecProfile::Vp8,
        coded_dimensions: dimensions,
        output_format: PixelFormat::Rgba8,
        color_range: ColorRange::Limited,
        hardware: HardwarePreference::Avoid,
        configuration: Vec::new(),
    };
    let factory = native_vp8_video_decoder_factory();
    let mut decoder = factory
        .create(&config, &limits)
        .map_err(|error| NativeEngineError::invalid("native VP8 decoder", error.to_string()))?;
    let mut frame = matroska_demuxer::Frame::default();
    let mut output = Vec::new();
    let mut retained_bytes = 0usize;
    let timestamp_scale = file.info().timestamp_scale().get();
    let mut index = 0u64;
    while file
        .next_frame(&mut frame)
        .map_err(|error| NativeEngineError::invalid("native WebM demux", error.to_string()))?
    {
        if frame.track != track_number {
            continue;
        }
        if output.len() >= MAX_NATIVE_VIDEO_FRAME_COUNT {
            return Ok(Vec::new());
        }
        let decoded = match decoder.submit(
            &EncodedVideoSample {
                presentation_index: FrameIndex(index),
                random_access: frame.is_keyframe.unwrap_or(index == 0),
                data: frame.data.clone(),
            },
            &CancellationToken::new(),
        ) {
            Ok(decoded) => decoded,
            Err(error) if error.kind() == ErrorKind::ResourceLimit => return Ok(Vec::new()),
            Err(error) => {
                return Err(NativeEngineError::invalid(
                    "native VP8 decode",
                    error.to_string(),
                ));
            }
        };
        let timestamp_millis = frame.timestamp.saturating_mul(timestamp_scale) / 1_000_000;
        for decoded_frame in decoded {
            let plane = decoded_frame.frame.planes.first().ok_or_else(|| {
                NativeEngineError::invalid("native VP8 decode", "decoder returned no pixel plane")
            })?;
            let required = plane.stride.checked_mul(height as usize).ok_or_else(|| {
                NativeEngineError::limit(
                    "native decoded video bytes",
                    MAX_NATIVE_VIDEO_DECODED_BYTES,
                    usize::MAX,
                )
            })?;
            if plane.data.len() < required || plane.stride < width as usize * 4 {
                return Err(NativeEngineError::invalid(
                    "native VP8 decode",
                    "decoder returned malformed RGBA pixels",
                ));
            }
            let mut pixels = Vec::with_capacity(width as usize * height as usize * 4);
            for row in plane.data.chunks(plane.stride).take(height as usize) {
                pixels.extend_from_slice(&row[..width as usize * 4]);
            }
            retained_bytes = retained_bytes.saturating_add(pixels.len());
            if retained_bytes > MAX_NATIVE_VIDEO_DECODED_BYTES {
                return Ok(Vec::new());
            }
            output.push(NativeVideoFrame {
                timestamp_millis,
                width,
                height,
                pixels,
            });
        }
        index = index.saturating_add(1);
    }
    Ok(output)
}

fn data_media_metadata(url: &Url) -> Result<Option<NativeMediaMetadata>, NativeEngineError> {
    let data =
        url.as_str()
            .strip_prefix("data:")
            .ok_or_else(|| NativeEngineError::UnsupportedUrl {
                reason: "media data URL has an invalid scheme".into(),
            })?;
    let Some((metadata, payload)) = data.split_once(',') else {
        return Err(NativeEngineError::UnsupportedUrl {
            reason: "media data URL is missing its comma-separated payload".into(),
        });
    };
    let mut metadata_parts = metadata.split(';');
    let declared_type = metadata_parts
        .next()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let is_base64 = metadata_parts.any(|part| part.eq_ignore_ascii_case("base64"));
    let bytes = if is_base64 {
        decode_base64_bytes(payload, MAX_NATIVE_MEDIA_BYTES, "base64 media payload")?
    } else {
        percent_decode_bytes(payload, MAX_NATIVE_MEDIA_BYTES)?
    };
    media_metadata_from_bytes(declared_type, &bytes)
}

fn data_font_bytes(url: &Url) -> Result<Option<Vec<u8>>, NativeEngineError> {
    let data = url
        .as_str()
        .split_once(':')
        .map(|(_, data)| data)
        .ok_or_else(|| NativeEngineError::UnsupportedUrl {
            reason: "font data URL has an invalid scheme".into(),
        })?;
    let Some((metadata, payload)) = data.split_once(',') else {
        return Err(NativeEngineError::UnsupportedUrl {
            reason: "font data URL is missing its comma-separated payload".into(),
        });
    };
    let mut metadata_parts = metadata.split(';');
    let declared_type = metadata_parts.next().map(str::trim).unwrap_or_default();
    if !supported_font_media_type_text(declared_type) {
        return Ok(None);
    }
    let is_base64 = metadata_parts.any(|part| part.eq_ignore_ascii_case("base64"));
    let bytes = if is_base64 {
        decode_base64_bytes(payload, MAX_NATIVE_FONT_BYTES, "base64 font payload")?
    } else {
        percent_decode_bytes(payload, MAX_NATIVE_FONT_BYTES)?
    };
    (!bytes.is_empty())
        .then_some(bytes)
        .map_or(Ok(None), |bytes| Ok(Some(bytes)))
}

fn supported_font_media_type_text(value: &str) -> bool {
    matches!(
        value.to_ascii_lowercase().as_str(),
        "" | "font/ttf"
            | "font/otf"
            | "font/woff"
            | "font/woff2"
            | "application/font-woff"
            | "application/x-font-ttf"
            | "application/x-font-opentype"
            | "application/vnd.ms-fontobject"
    )
}

fn font_content_type_text_allowed(value: &str) -> bool {
    let media_type = value.split(';').next().unwrap_or_default().trim();
    supported_font_media_type_text(media_type)
}

fn supported_media_type_text(value: &str) -> Option<&'static str> {
    let value = value.split(';').next().unwrap_or_default().trim();
    match value.to_ascii_lowercase().as_str() {
        "audio/mpeg" | "audio/mp3" => Some("audio/mpeg"),
        "audio/ogg" => Some("audio/ogg"),
        "application/ogg" => Some("application/ogg"),
        "audio/wav" | "audio/wave" | "audio/x-wav" => Some("audio/wav"),
        "audio/webm" => Some("audio/webm"),
        "audio/mp4" => Some("audio/mp4"),
        "video/mp4" => Some("video/mp4"),
        "video/ogg" => Some("video/ogg"),
        "video/webm" => Some("video/webm"),
        "application/vnd.apple.mpegurl" | "application/x-mpegurl" => {
            Some("application/vnd.apple.mpegurl")
        }
        _ => None,
    }
}

fn sniff_media_type(bytes: &[u8]) -> Option<&'static str> {
    if bytes.len() >= 12 && &bytes[..4] == b"RIFF" && &bytes[8..12] == b"WAVE" {
        return Some("audio/wav");
    }
    if bytes.len() >= 4 && &bytes[..4] == b"OggS" {
        return Some("application/ogg");
    }
    if bytes.len() >= 4 && &bytes[..4] == b"\x1A\x45\xDF\xA3" {
        return Some("video/webm");
    }
    if bytes.len() >= 12 && &bytes[4..8] == b"ftyp" {
        return Some("video/mp4");
    }
    if bytes.len() >= 3 && &bytes[..3] == b"ID3" {
        return Some("audio/mpeg");
    }
    None
}

fn script_content_type_allowed(
    value: Option<&reqwest::header::HeaderValue>,
) -> Result<bool, NativeEngineError> {
    let Some(value) = value else {
        return Ok(true);
    };
    let value = value.to_str().map_err(|_| NativeEngineError::Network {
        operation: "script content-type validation".into(),
        reason: "script content type is not valid ASCII".into(),
    })?;
    Ok(javascript_mime_essence_allowed(value))
}

fn module_content_type_allowed(
    module_type: NativeModuleResourceType,
    value: Option<&reqwest::header::HeaderValue>,
) -> Result<bool, NativeEngineError> {
    let Some(value) = value else {
        return Ok(false);
    };
    let value = value.to_str().map_err(|_| NativeEngineError::Network {
        operation: "module content-type validation".into(),
        reason: "module content type is not valid ASCII".into(),
    })?;
    Ok(module_content_type_text_allowed(module_type, Some(value)))
}

fn module_content_type_text_allowed(
    module_type: NativeModuleResourceType,
    value: Option<&str>,
) -> bool {
    let Some(value) = value else {
        return false;
    };
    let essence = value.split(';').next().unwrap_or_default().trim();
    match module_type {
        NativeModuleResourceType::JavaScript => javascript_mime_essence_allowed(essence),
        NativeModuleResourceType::Json => {
            if essence.eq_ignore_ascii_case("application/json")
                || essence.eq_ignore_ascii_case("text/json")
            {
                return true;
            }
            essence
                .split_once('/')
                .is_some_and(|(media_type, subtype)| {
                    mime_type_token(media_type)
                        && mime_type_token(subtype)
                        && subtype.to_ascii_lowercase().ends_with("+json")
                })
        }
        NativeModuleResourceType::Unsupported => false,
    }
}

fn mime_type_token(value: &str) -> bool {
    !value.is_empty()
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"!#$%&'*+-.^_`|~".contains(&byte))
}

fn cached_resource_content_type_text_allowed(
    module_type: Option<NativeModuleResourceType>,
    value: Option<&str>,
) -> bool {
    match module_type {
        Some(module_type) => module_content_type_text_allowed(module_type, value),
        None => value.is_none_or(javascript_mime_essence_allowed),
    }
}

pub(crate) const JAVASCRIPT_MIME_TYPE_ESSENCES: &[&str] = &[
    "application/ecmascript",
    "application/javascript",
    "application/x-ecmascript",
    "application/x-javascript",
    "text/ecmascript",
    "text/javascript",
    "text/javascript1.0",
    "text/javascript1.1",
    "text/javascript1.2",
    "text/javascript1.3",
    "text/javascript1.4",
    "text/javascript1.5",
    "text/jscript",
    "text/livescript",
    "text/x-ecmascript",
    "text/x-javascript",
];

/// External response Content-Type parameters are ignored when matching a
/// JavaScript MIME type, unlike the `script[type]` attribute.
pub(crate) fn javascript_mime_essence_allowed(value: &str) -> bool {
    let essence = value.split(';').next().unwrap_or_default().trim();
    JAVASCRIPT_MIME_TYPE_ESSENCES
        .iter()
        .any(|candidate| essence.eq_ignore_ascii_case(candidate))
}

/// The script element type attribute matches only the essence string itself:
/// MIME parameters or surrounding whitespace prevent an essence match.
pub(crate) fn javascript_mime_type_essence_match(value: &str) -> bool {
    JAVASCRIPT_MIME_TYPE_ESSENCES
        .iter()
        .any(|candidate| value.eq_ignore_ascii_case(candidate))
}

fn cache_key(url: &Url) -> String {
    let mut key = url.clone();
    key.set_fragment(None);
    key.to_string()
}

struct NativeFetchResponseCacheKeyInput<'a> {
    document_url: &'a Url,
    target_url: &'a Url,
    method: &'a NativeFetchMethod,
    request_headers: &'a BTreeMap<String, String>,
    content_type: Option<&'a str>,
    credentials: bool,
    cors_mode: NativeCorsMode,
    request_cookie: Option<&'a str>,
    request_referrer: Option<&'a str>,
    referrer_policy: &'a str,
}

fn fetch_response_cache_key(input: NativeFetchResponseCacheKeyInput<'_>) -> String {
    let NativeFetchResponseCacheKeyInput {
        document_url,
        target_url,
        method,
        request_headers,
        content_type,
        credentials,
        cors_mode,
        request_cookie,
        request_referrer,
        referrer_policy,
    } = input;
    let mut key = String::new();
    let cors_mode_key = if document_url.origin() == target_url.origin() {
        "same-origin"
    } else {
        match cors_mode {
            NativeCorsMode::NoCors => "no-cors",
            NativeCorsMode::Cors => "cors",
            NativeCorsMode::SameOrigin => "same-origin",
            NativeCorsMode::Navigation => "navigation",
        }
    };
    for value in [
        document_url.origin().ascii_serialization(),
        cache_key(target_url),
        method.as_str().to_owned(),
        if credentials { "include" } else { "omit" }.to_owned(),
        cors_mode_key.to_owned(),
        content_type.unwrap_or_default().to_owned(),
        request_cookie.unwrap_or_default().to_owned(),
        request_referrer.unwrap_or_default().to_owned(),
        referrer_policy.to_owned(),
    ] {
        append_fetch_cache_key_part(&mut key, &value);
    }
    for (name, value) in request_headers {
        append_fetch_cache_key_part(&mut key, name);
        append_fetch_cache_key_part(&mut key, value);
    }
    key
}

fn font_response_cache_key(
    document_url: &Url,
    target_url: &Url,
    request_cookie: Option<&str>,
) -> String {
    let mut key = String::new();
    for value in [
        document_url.origin().ascii_serialization(),
        cache_key(target_url),
        request_cookie.unwrap_or_default().to_owned(),
    ] {
        append_fetch_cache_key_part(&mut key, &value);
    }
    key
}

fn append_fetch_cache_key_part(key: &mut String, value: &str) {
    key.push_str(&value.len().to_string());
    key.push(':');
    key.push_str(value);
    key.push('|');
}

fn response_cache_metadata_present(headers: &HeaderMap) -> bool {
    headers.contains_key(reqwest::header::CACHE_CONTROL)
        || headers.contains_key(reqwest::header::PRAGMA)
        || headers.contains_key(reqwest::header::ETAG)
        || headers.contains_key(reqwest::header::LAST_MODIFIED)
}

fn with_original_fragment(
    mut resource: NativeResource,
    original_url: &str,
) -> Result<NativeResource, NativeEngineError> {
    let final_url = Url::parse(&resource.url).map_err(|_| NativeEngineError::UnsupportedUrl {
        reason: "cached HTTP(S) resource has invalid URL syntax".into(),
    })?;
    resource.url = append_original_fragment(&final_url, original_url);
    Ok(resource)
}

fn response_header_text(headers: &HeaderMap, name: reqwest::header::HeaderName) -> Option<String> {
    headers
        .get(name)
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .filter(|value| value.len() <= MAX_NATIVE_RESPONSE_HEADER_VALUE_BYTES)
        .map(ToOwned::to_owned)
}

fn cache_control_has_directive(headers: &HeaderMap, expected: &str) -> bool {
    headers
        .get(reqwest::header::CACHE_CONTROL)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| {
            value.split(',').any(|directive| {
                directive
                    .split_once('=')
                    .map_or(directive, |(name, _)| name)
                    .trim()
                    .eq_ignore_ascii_case(expected)
            })
        })
}

fn cache_control_max_age(headers: &HeaderMap) -> Option<u64> {
    headers
        .get(reqwest::header::CACHE_CONTROL)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| {
            value.split(',').find_map(|directive| {
                let (name, value) = directive.split_once('=')?;
                if !name.trim().eq_ignore_ascii_case("max-age") {
                    return None;
                }
                value.trim().trim_matches('"').parse::<u64>().ok()
            })
        })
}

fn cache_control_requires_revalidation(headers: &HeaderMap) -> bool {
    cache_control_has_directive(headers, "no-cache")
        || cache_control_max_age(headers) == Some(0)
        || headers
            .get(reqwest::header::PRAGMA)
            .and_then(|value| value.to_str().ok())
            .is_some_and(|value| {
                value
                    .split(',')
                    .any(|directive| directive.trim().eq_ignore_ascii_case("no-cache"))
            })
}

fn document_cache_storage_allowed(headers: &HeaderMap) -> bool {
    !cache_control_has_directive(headers, "no-store")
        && !headers
            .get(reqwest::header::VARY)
            .and_then(|value| value.to_str().ok())
            .is_some_and(|value| {
                value
                    .split(',')
                    .any(|field| field.trim() == "*" || field.trim().eq_ignore_ascii_case("cookie"))
            })
}

fn document_cache_fresh_until(headers: &HeaderMap, now: Instant) -> Option<Instant> {
    if cache_control_requires_revalidation(headers) {
        return Some(now);
    }
    cache_control_max_age(headers)
        .map(|seconds| now.checked_add(Duration::from_secs(seconds)).unwrap_or(now))
}

impl NativeNetworkState {
    fn from_profile(profile: Vec<NativeCookieProfileEntry>) -> Result<Self, NativeEngineError> {
        let state = Self::default();
        for cookie in profile {
            let Some(cookie) = NativeCookie::from_profile(cookie)? else {
                continue;
            };
            state.cookies.lock().push(cookie);
        }
        Ok(state)
    }

    fn cookie_profile(&self) -> Vec<NativeCookieProfileEntry> {
        if !self.cookie_authority_enabled {
            return Vec::new();
        }
        self.cookies
            .snapshot()
            .into_iter()
            .filter_map(|cookie| cookie.to_profile())
            .collect()
    }

    fn document_cookie_header(&self, url: &Url) -> Option<String> {
        self.cookie_header_with_visibility(
            url,
            false,
            Some(url),
            false,
            NativeNavigationMethod::Get,
        )
    }

    fn cookie_header_for_request<M: NativeCookieMethod>(
        &self,
        url: &Url,
        initiator_url: Option<&Url>,
        top_level_navigation: bool,
        method: M,
    ) -> Option<String> {
        self.cookie_header_with_visibility(url, true, initiator_url, top_level_navigation, method)
    }

    fn matching_cookies(&self, url: &Url, include_http_only: bool) -> Vec<NativeCookie> {
        if !self.cookie_authority_enabled {
            return Vec::new();
        }
        let Some(host) = url.host_str().map(str::to_ascii_lowercase) else {
            return Vec::new();
        };
        let request_path = if url.path().is_empty() {
            "/"
        } else {
            url.path()
        };
        let secure_request = url.scheme().eq_ignore_ascii_case("https");
        let now = Instant::now();
        let mut matching = self
            .cookies
            .snapshot()
            .into_iter()
            .filter(|cookie| {
                domain_matches(cookie, &host)
                    && path_matches(request_path, &cookie.path)
                    && (!cookie.secure || secure_request)
                    && (include_http_only || !cookie.http_only)
                    && cookie.expires_at.is_none_or(|expires_at| expires_at > now)
            })
            .collect::<Vec<_>>();
        matching.sort_by(|left, right| {
            right
                .path
                .len()
                .cmp(&left.path.len())
                .then_with(|| left.name.cmp(&right.name))
                .then_with(|| left.domain.cmp(&right.domain))
        });
        matching
    }

    fn cookie_header_with_visibility<M: NativeCookieMethod>(
        &self,
        url: &Url,
        include_http_only: bool,
        initiator_url: Option<&Url>,
        top_level_navigation: bool,
        method: M,
    ) -> Option<String> {
        let matching = self
            .matching_cookies(url, include_http_only)
            .into_iter()
            .filter(|cookie| {
                cookie_same_site_allows(cookie, url, initiator_url, top_level_navigation, &method)
            });

        let mut header = String::new();
        for cookie in matching {
            let pair = format!("{}={}", cookie.name, cookie.value);
            let separator = usize::from(!header.is_empty()) * 2;
            if header
                .len()
                .saturating_add(separator)
                .saturating_add(pair.len())
                > MAX_NATIVE_COOKIE_BYTES
            {
                break;
            }
            if !header.is_empty() {
                header.push_str("; ");
            }
            header.push_str(&pair);
        }
        (!header.is_empty()).then_some(header)
    }

    fn set_cookie_profile(
        &mut self,
        cookie: NativeCookie,
    ) -> Result<Option<NativeCookieProfileEntry>, NativeEngineError> {
        if !self.cookie_authority_enabled {
            return Err(NativeEngineError::Worker {
                operation: "install content-process cookies".into(),
                reason: "cookie profiles are owned by the browser process".into(),
            });
        }
        let same_cookie = |candidate: &NativeCookie| {
            candidate.name == cookie.name
                && candidate.domain == cookie.domain
                && candidate.path == cookie.path
        };
        let mut cookies = self.cookies.lock();
        let had_existing = cookies.iter().any(same_cookie);
        cookies.retain(|candidate| !same_cookie(candidate));
        let evicted = if !had_existing && cookies.len() >= MAX_NATIVE_COOKIE_PROFILE_ENTRIES {
            cookies.remove(0).to_profile()
        } else {
            None
        };
        cookies.push(cookie);
        if cookies.len() > MAX_NATIVE_COOKIE_PROFILE_ENTRIES {
            return Err(NativeEngineError::limit(
                "native cookie profile entries",
                MAX_NATIVE_COOKIE_PROFILE_ENTRIES,
                cookies.len(),
            ));
        }
        Ok(evicted)
    }

    fn remove_cookie(&mut self, name: &str, domain: &str, path: &str) -> bool {
        if !self.cookie_authority_enabled {
            return false;
        }
        let mut cookies = self.cookies.lock();
        let before = cookies.len();
        cookies
            .retain(|cookie| cookie.name != name || cookie.domain != domain || cookie.path != path);
        cookies.len() != before
    }

    fn clear_cookies(&mut self, changes: &mut Vec<NativeCookieChange>) {
        if !self.cookie_authority_enabled {
            return;
        }
        for cookie in self.cookies.lock().drain(..) {
            changes.push(NativeCookieChange {
                name: cookie.name,
                domain: cookie.domain,
                path: cookie.path,
                cookie: None,
            });
        }
    }

    fn store_cookie(&mut self, url: &Url, line: &str) -> Vec<NativeCookieChange> {
        if !self.cookie_authority_enabled || line.len() > MAX_NATIVE_COOKIE_BYTES {
            return Vec::new();
        }
        let Some(host) = url.host_str().map(str::to_ascii_lowercase) else {
            return Vec::new();
        };
        let (pair, attributes) = line.split_once(';').unwrap_or((line, ""));
        let Some((name, value)) = pair.trim().split_once('=') else {
            return Vec::new();
        };
        let name = name.trim();
        let value = value.trim();
        if name.is_empty()
            || !valid_cookie_text(name, true)
            || !valid_cookie_text(value, false)
            || name.len().saturating_add(value.len()) > MAX_NATIVE_COOKIE_BYTES
        {
            return Vec::new();
        }

        let mut domain = host.clone();
        let mut host_only = true;
        let mut path = default_cookie_path(url);
        let mut secure = false;
        let mut http_only = false;
        let mut same_site = None;
        let mut priority = None;
        let mut max_age = None;
        for attribute in attributes.split(';').map(str::trim) {
            if attribute.eq_ignore_ascii_case("secure") {
                secure = true;
                continue;
            }
            if attribute.eq_ignore_ascii_case("httponly") {
                http_only = true;
                continue;
            }
            let Some((key, attribute_value)) = attribute.split_once('=') else {
                continue;
            };
            match key.trim().to_ascii_lowercase().as_str() {
                "domain" => {
                    let candidate = attribute_value
                        .trim()
                        .trim_start_matches('.')
                        .to_ascii_lowercase();
                    if candidate.is_empty()
                        || !valid_cookie_domain(&candidate)
                        || !host_matches_domain(&host, &candidate)
                    {
                        return Vec::new();
                    }
                    domain = candidate;
                    host_only = false;
                }
                "path" => {
                    if attribute_value.trim().starts_with('/') {
                        path = attribute_value.trim().to_owned();
                    }
                }
                "max-age" => {
                    max_age = attribute_value.trim().parse::<i64>().ok();
                }
                "samesite" => {
                    same_site = Some(attribute_value.trim().to_owned());
                }
                "priority" => {
                    priority = Some(attribute_value.trim().to_owned());
                }
                _ => {}
            }
        }
        if secure && !url.scheme().eq_ignore_ascii_case("https") {
            return Vec::new();
        }
        let Ok(same_site) = normalize_cookie_same_site(same_site.as_deref()) else {
            return Vec::new();
        };
        if same_site
            .as_deref()
            .is_some_and(|value| value.eq_ignore_ascii_case("none"))
            && !secure
        {
            return Vec::new();
        }
        let Ok(priority) = normalize_cookie_priority(priority.as_deref()) else {
            return Vec::new();
        };

        let same_cookie = |cookie: &NativeCookie| {
            cookie.name == name && cookie.domain == domain && cookie.path == path
        };
        let change_key = || NativeCookieChange {
            name: name.to_owned(),
            domain: domain.clone(),
            path: path.clone(),
            cookie: None,
        };
        let mut cookies = self.cookies.lock();
        if max_age.is_some_and(|age| age <= 0) {
            let removed = cookies.iter().any(same_cookie);
            cookies.retain(|cookie| !same_cookie(cookie));
            return removed.then(change_key).into_iter().collect();
        }
        let expires_at_unix_seconds = max_age.map(|age| {
            let seconds = u64::try_from(age)
                .unwrap_or_default()
                .min(60 * 60 * 24 * 365 * 10);
            unix_time_seconds().saturating_add(seconds)
        });
        let expires_at = expires_at_unix_seconds.map(|expires_at| {
            Instant::now() + Duration::from_secs(expires_at.saturating_sub(unix_time_seconds()))
        });
        let profile = NativeCookieProfileEntry {
            name: name.to_owned(),
            value: value.to_owned(),
            domain: domain.clone(),
            path: path.clone(),
            host_only,
            secure,
            http_only,
            same_site: same_site.clone(),
            priority: priority.clone(),
            expires_at_unix_seconds,
        };
        let had_existing = cookies.iter().any(same_cookie);
        cookies.retain(|cookie| !same_cookie(cookie));
        let mut changes = Vec::new();
        if !had_existing && cookies.len() >= MAX_NATIVE_COOKIE_PROFILE_ENTRIES {
            let removed = cookies.remove(0);
            changes.push(NativeCookieChange {
                name: removed.name,
                domain: removed.domain,
                path: removed.path,
                cookie: None,
            });
        }
        cookies.push(NativeCookie {
            name: profile.name.clone(),
            value: profile.value.clone(),
            domain: profile.domain.clone(),
            path: profile.path.clone(),
            host_only,
            secure,
            http_only,
            same_site,
            priority,
            expires_at,
            expires_at_unix_seconds,
        });
        changes.push(NativeCookieChange {
            name: profile.name.clone(),
            domain: profile.domain.clone(),
            path: profile.path.clone(),
            cookie: Some(profile),
        });
        changes
    }

    fn store_document_cache(&mut self, key: String, entry: NativeDocumentCacheEntry) {
        if !self.cache.contains_key(&key)
            && self.cache.len() >= MAX_NATIVE_CACHE_ENTRIES
            && let Some(oldest) = self.cache.keys().next().cloned()
        {
            self.cache.remove(&oldest);
        }
        self.cache.insert(key, entry);
    }

    fn remove_cache(&mut self, key: &str) {
        self.cache.remove(key);
    }

    fn store_image_cache_entry(&mut self, key: String, entry: NativeImageCacheEntry) {
        if !self.image_cache.contains_key(&key)
            && self.image_cache.len() >= MAX_NATIVE_CACHE_ENTRIES
            && let Some(oldest) = self.image_cache.keys().next().cloned()
        {
            self.image_cache.remove(&oldest);
        }
        self.image_cache.insert(key, entry);
    }

    fn remove_image_cache(&mut self, key: &str) {
        self.image_cache.remove(key);
    }

    fn store_stylesheet_cache(&mut self, key: String, entry: NativeTextCacheEntry) {
        if !self.stylesheet_cache.contains_key(&key)
            && self.stylesheet_cache.len() >= MAX_NATIVE_CACHE_ENTRIES
            && let Some(oldest) = self.stylesheet_cache.keys().next().cloned()
        {
            self.stylesheet_cache.remove(&oldest);
        }
        self.stylesheet_cache.insert(key, entry);
    }

    fn remove_stylesheet_cache(&mut self, key: &str) {
        self.stylesheet_cache.remove(key);
    }

    fn store_script_cache(&mut self, key: String, entry: NativeTextCacheEntry) {
        if !self.script_cache.contains_key(&key)
            && self.script_cache.len() >= MAX_NATIVE_CACHE_ENTRIES
            && let Some(oldest) = self.script_cache.keys().next().cloned()
        {
            self.script_cache.remove(&oldest);
        }
        self.script_cache.insert(key, entry);
    }

    fn remove_script_cache(&mut self, key: &str) {
        self.script_cache.remove(key);
    }

    fn store_fetch_cache(&mut self, key: String, entry: NativeFetchCacheEntry) {
        if !self.fetch_cache.contains_key(&key)
            && self.fetch_cache.len() >= MAX_NATIVE_CACHE_ENTRIES
            && let Some(oldest) = self.fetch_cache.keys().next().cloned()
        {
            self.fetch_cache.remove(&oldest);
        }
        self.fetch_cache.insert(key, entry);
    }

    fn remove_fetch_cache(&mut self, key: &str) {
        self.fetch_cache.remove(key);
    }

    fn store_font_cache(&mut self, key: String, entry: NativeFontCacheEntry) {
        if !self.font_cache.contains_key(&key)
            && self.font_cache.len() >= MAX_NATIVE_CACHE_ENTRIES
            && let Some(oldest) = self.font_cache.keys().next().cloned()
        {
            self.font_cache.remove(&oldest);
        }
        self.font_cache.insert(key, entry);
    }

    fn remove_font_cache(&mut self, key: &str) {
        self.font_cache.remove(key);
    }

    fn store_document_policy(&mut self, key: String, policy: NativeCspPolicy) {
        if !self.document_policies.contains_key(&key)
            && self.document_policies.len() >= MAX_NATIVE_CACHE_ENTRIES
            && let Some(oldest) = self.document_policies.keys().next().cloned()
        {
            self.document_policies.remove(&oldest);
        }
        self.document_policies.insert(key, policy);
    }

    fn store_document_response_policy_headers(
        &mut self,
        key: String,
        headers: Vec<(String, String)>,
    ) {
        if headers.is_empty() {
            self.document_response_policy_headers.remove(&key);
            return;
        }
        if !self.document_response_policy_headers.contains_key(&key)
            && self.document_response_policy_headers.len() >= MAX_NATIVE_CACHE_ENTRIES
            && let Some(oldest) = self.document_response_policy_headers.keys().next().cloned()
        {
            self.document_response_policy_headers.remove(&oldest);
        }
        self.document_response_policy_headers.insert(key, headers);
    }

    fn store_document_referrer_policy(
        &mut self,
        key: String,
        policy: Option<NativeFetchReferrerPolicy>,
    ) {
        let Some(policy) = policy else {
            self.document_referrer_policies.remove(&key);
            return;
        };
        if !self.document_referrer_policies.contains_key(&key)
            && self.document_referrer_policies.len() >= MAX_NATIVE_CACHE_ENTRIES
            && let Some(oldest) = self.document_referrer_policies.keys().next().cloned()
        {
            self.document_referrer_policies.remove(&oldest);
        }
        self.document_referrer_policies.insert(key, policy);
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

fn append_original_fragment(final_url: &Url, original_url: &str) -> String {
    let Some((_, fragment)) = original_url.split_once('#') else {
        return final_url.to_string();
    };
    let mut final_url = final_url.clone();
    final_url.set_fragment(Some(fragment));
    final_url.to_string()
}

fn network_error(operation: &str, error: reqwest::Error) -> NativeEngineError {
    NativeEngineError::Network {
        operation: operation.into(),
        reason: if error.is_timeout() {
            "request timed out".into()
        } else {
            "request failed without exposing response data".into()
        },
    }
}

fn fixture_navigation_url(canonical: &str, original: &str) -> String {
    original.split_once('#').map_or_else(
        || canonical.to_owned(),
        |(_, fragment)| format!("{canonical}#{fragment}"),
    )
}

#[cfg(test)]
#[path = "resource_loader/tests.rs"]
mod tests;
