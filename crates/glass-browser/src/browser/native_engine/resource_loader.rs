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
    NativeCookieProfileEntry, load_cookie_profile, validate_navigation_preload_header_value,
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
use std::sync::{Arc, OnceLock};
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

#[allow(dead_code)]
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
pub(crate) enum NativeModuleResourceType {
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

#[allow(dead_code)]
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
pub(crate) struct NativeObjectUrlResource {
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
pub(crate) struct NativeObjectUrlTransfer {
    pub(crate) href: String,
    pub(crate) resource: NativeObjectUrlResource,
}

/// Metadata admitted for one media element resource. The native engine keeps
/// this separate from decoded audio/video frames: callers can inspect source
/// ownership and media readiness without retaining an unbounded codec buffer
/// in the document wire.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct NativeMediaMetadata {
    pub(crate) content_type: String,
    pub(crate) byte_length: usize,
    pub(crate) duration_millis: Option<u64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub(crate) enum NativeNavigationMethod {
    Get,
    Head,
    Post,
    Put,
    Patch,
    Delete,
    Options,
}

impl Default for NativeNavigationMethod {
    fn default() -> Self {
        Self::Get
    }
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
pub(crate) struct NativeNavigationRequest {
    pub(crate) method: NativeNavigationMethod,
    pub(crate) url: String,
    pub(crate) body: Option<NativeRequestBody>,
    pub(crate) body_content_type: Option<String>,
    pub(crate) replace_history: bool,
    pub(crate) target: Option<String>,
    pub(crate) object_url: Option<NativeObjectUrlResource>,
}

impl NativeNavigationRequest {
    pub(crate) fn get(url: impl Into<String>) -> Self {
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
pub(crate) enum NativeRequestBody {
    Text(String),
    Bytes(#[serde(with = "native_request_body_bytes")] Vec<u8>),
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
            Self::Bytes(body) => body.len(),
        }
    }
}

pub(crate) fn validate_target_navigation_payload(
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

pub(crate) struct NativeWebSocketTarget {
    pub(crate) url: Url,
    pub(crate) cookie: Option<String>,
    pub(crate) csp_violations: Vec<NativeCspViolation>,
    pub(crate) csp_report_deliveries: Vec<NativeCspReportDelivery>,
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
pub(crate) struct NativeCspViolation {
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
pub(crate) struct NativeCspReportDelivery {
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
pub(crate) fn schedule_native_csp_report_deliveries(deliveries: Vec<NativeCspReportDelivery>) {
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
    cookies: Vec<NativeCookie>,
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
            cookies: Vec::new(),
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

#[derive(Debug, Clone, PartialEq, Eq, Default)]
struct NativeCspDirectives {
    style_sources: Option<Vec<String>>,
    style_element_sources: Option<Vec<String>>,
    style_attribute_sources: Option<Vec<String>>,
    script_sources: Option<Vec<String>>,
    script_element_sources: Option<Vec<String>>,
    script_attribute_sources: Option<Vec<String>>,
    image_sources: Option<Vec<String>>,
    font_sources: Option<Vec<String>>,
    media_sources: Option<Vec<String>>,
    frame_sources: Option<Vec<String>>,
    child_sources: Option<Vec<String>>,
    connect_sources: Option<Vec<String>>,
    worker_sources: Option<Vec<String>>,
    form_action_sources: Option<Vec<String>>,
    navigate_to_sources: Option<Vec<String>>,
    default_sources: Option<Vec<String>>,
    report_uris: Vec<String>,
    report_to: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct NativeCspDeclaration {
    directives: NativeCspDirectives,
    original_policy: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
struct NativeCspPolicy {
    policies: Vec<NativeCspDirectives>,
    header_policy_count: usize,
    meta_policy_sources: Vec<String>,
    rooted_file_self_root: Option<PathBuf>,
    report_only_policies: Vec<NativeCspDeclaration>,
    reporting_endpoints: BTreeMap<String, Vec<String>>,
}

impl NativeCspDirectives {
    fn allows(&self, kind: NativeSubresourceKind, document_url: &Url, resource_url: &Url) -> bool {
        let sources = self.sources_for(kind).or(self.default_sources.as_ref());
        csp_sources_allow(sources.map(Vec::as_slice), document_url, resource_url)
    }

    fn allows_redirect(
        &self,
        kind: NativeSubresourceKind,
        document_url: &Url,
        resource_url: &Url,
    ) -> bool {
        let sources = self.sources_for(kind).or(self.default_sources.as_ref());
        csp_sources_allow_for_redirect(sources.map(Vec::as_slice), document_url, resource_url)
    }

    fn allows_script(
        &self,
        document_url: &Url,
        resource_url: &Url,
        parser_inserted: bool,
        nonce: Option<&str>,
    ) -> bool {
        let sources = self
            .sources_for(NativeSubresourceKind::Script)
            .or(self.default_sources.as_ref());
        csp_script_sources_allow(
            sources.map(Vec::as_slice),
            document_url,
            resource_url,
            parser_inserted,
            nonce,
        )
    }

    fn allows_rooted_file_script(
        &self,
        document_url: &Url,
        resource_url: &Url,
        parser_inserted: bool,
        nonce: Option<&str>,
        same_root: bool,
    ) -> bool {
        let sources = self
            .sources_for(NativeSubresourceKind::Script)
            .or(self.default_sources.as_ref());
        csp_script_sources_allow_for_rooted_file(
            sources.map(Vec::as_slice),
            document_url,
            resource_url,
            parser_inserted,
            nonce,
            same_root,
        )
    }

    fn allows_rooted_file_style(
        &self,
        document_url: &Url,
        resource_url: &Url,
        nonce: Option<&str>,
        same_root: bool,
    ) -> bool {
        let sources = self
            .style_element_sources
            .as_deref()
            .or(self.style_sources.as_deref())
            .or(self.default_sources.as_deref());
        csp_sources_allow_for_rooted_file(sources, document_url, resource_url, nonce, same_root)
    }

    fn allows_rooted_file_resource(
        &self,
        kind: NativeSubresourceKind,
        document_url: &Url,
        resource_url: &Url,
        same_root: bool,
    ) -> bool {
        let sources = self.sources_for(kind).or(self.default_sources.as_ref());
        csp_sources_allow_for_rooted_file(
            sources.map(Vec::as_slice),
            document_url,
            resource_url,
            None,
            same_root,
        )
    }

    fn allows_script_redirect(
        &self,
        document_url: &Url,
        resource_url: &Url,
        parser_inserted: bool,
        nonce: Option<&str>,
    ) -> bool {
        let sources = self
            .sources_for(NativeSubresourceKind::Script)
            .or(self.default_sources.as_ref());
        csp_script_sources_allow_for_redirect(
            sources.map(Vec::as_slice),
            document_url,
            resource_url,
            parser_inserted,
            nonce,
        )
    }

    fn allows_navigation(
        &self,
        kind: NativeNavigationPolicyKind,
        document_url: &Url,
        target_url: &Url,
    ) -> bool {
        self.navigation_sources_for(kind).is_none_or(|sources| {
            csp_sources_allow(Some(sources.as_slice()), document_url, target_url)
        })
    }

    fn navigation_sources_for(&self, kind: NativeNavigationPolicyKind) -> Option<&Vec<String>> {
        match kind {
            NativeNavigationPolicyKind::FormAction => self.form_action_sources.as_ref(),
            NativeNavigationPolicyKind::NavigateTo => self.navigate_to_sources.as_ref(),
        }
    }

    fn directive_for(&self, kind: NativeSubresourceKind) -> Option<(&'static str, &Vec<String>)> {
        let specific = match kind {
            NativeSubresourceKind::Style => self
                .style_element_sources
                .as_ref()
                .map(|sources| ("style-src-elem", sources))
                .or_else(|| {
                    self.style_sources
                        .as_ref()
                        .map(|sources| ("style-src", sources))
                }),
            NativeSubresourceKind::Script => self
                .script_element_sources
                .as_ref()
                .map(|sources| ("script-src-elem", sources))
                .or_else(|| {
                    self.script_sources
                        .as_ref()
                        .map(|sources| ("script-src", sources))
                }),
            NativeSubresourceKind::Image => self
                .image_sources
                .as_ref()
                .map(|sources| ("img-src", sources)),
            NativeSubresourceKind::Font => self
                .font_sources
                .as_ref()
                .map(|sources| ("font-src", sources)),
            NativeSubresourceKind::Media => self
                .media_sources
                .as_ref()
                .map(|sources| ("media-src", sources)),
            NativeSubresourceKind::Frame => self
                .frame_sources
                .as_ref()
                .map(|sources| ("frame-src", sources))
                .or_else(|| {
                    self.child_sources
                        .as_ref()
                        .map(|sources| ("child-src", sources))
                }),
            NativeSubresourceKind::Connect => self
                .connect_sources
                .as_ref()
                .map(|sources| ("connect-src", sources)),
            NativeSubresourceKind::Worker => self
                .worker_sources
                .as_ref()
                .map(|sources| ("worker-src", sources))
                .or_else(|| {
                    self.child_sources
                        .as_ref()
                        .map(|sources| ("child-src", sources))
                })
                .or_else(|| {
                    self.script_sources
                        .as_ref()
                        .map(|sources| ("script-src", sources))
                }),
        };
        specific.or_else(|| {
            self.default_sources
                .as_ref()
                .map(|sources| ("default-src", sources))
        })
    }

    fn sources_for(&self, kind: NativeSubresourceKind) -> Option<&Vec<String>> {
        match kind {
            NativeSubresourceKind::Style => self
                .style_element_sources
                .as_ref()
                .or(self.style_sources.as_ref()),
            NativeSubresourceKind::Script => self
                .script_element_sources
                .as_ref()
                .or(self.script_sources.as_ref()),
            NativeSubresourceKind::Image => self.image_sources.as_ref(),
            NativeSubresourceKind::Font => self.font_sources.as_ref(),
            NativeSubresourceKind::Media => self.media_sources.as_ref(),
            NativeSubresourceKind::Frame => {
                self.frame_sources.as_ref().or(self.child_sources.as_ref())
            }
            NativeSubresourceKind::Connect => self.connect_sources.as_ref(),
            NativeSubresourceKind::Worker => self
                .worker_sources
                .as_ref()
                .or(self.child_sources.as_ref())
                .or(self.script_sources.as_ref()),
        }
    }

    fn inline_sources_for(&self, kind: NativeInlineCspKind) -> Option<&Vec<String>> {
        match kind {
            NativeInlineCspKind::ScriptElement => self
                .script_element_sources
                .as_ref()
                .or(self.script_sources.as_ref())
                .or(self.default_sources.as_ref()),
            NativeInlineCspKind::ScriptAttribute => self
                .script_attribute_sources
                .as_ref()
                .or(self.script_sources.as_ref())
                .or(self.default_sources.as_ref()),
            NativeInlineCspKind::StyleElement => self
                .style_element_sources
                .as_ref()
                .or(self.style_sources.as_ref())
                .or(self.default_sources.as_ref()),
            NativeInlineCspKind::StyleAttribute => self
                .style_attribute_sources
                .as_ref()
                .or(self.style_sources.as_ref())
                .or(self.default_sources.as_ref()),
        }
    }

    fn inline_directive_for(
        &self,
        kind: NativeInlineCspKind,
    ) -> Option<(&'static str, &Vec<String>)> {
        let specific = match kind {
            NativeInlineCspKind::ScriptElement => self
                .script_element_sources
                .as_ref()
                .map(|sources| ("script-src-elem", sources))
                .or_else(|| {
                    self.script_sources
                        .as_ref()
                        .map(|sources| ("script-src", sources))
                }),
            NativeInlineCspKind::ScriptAttribute => self
                .script_attribute_sources
                .as_ref()
                .map(|sources| ("script-src-attr", sources))
                .or_else(|| {
                    self.script_sources
                        .as_ref()
                        .map(|sources| ("script-src", sources))
                }),
            NativeInlineCspKind::StyleElement => self
                .style_element_sources
                .as_ref()
                .map(|sources| ("style-src-elem", sources))
                .or_else(|| {
                    self.style_sources
                        .as_ref()
                        .map(|sources| ("style-src", sources))
                }),
            NativeInlineCspKind::StyleAttribute => self
                .style_attribute_sources
                .as_ref()
                .map(|sources| ("style-src-attr", sources))
                .or_else(|| {
                    self.style_sources
                        .as_ref()
                        .map(|sources| ("style-src", sources))
                }),
        };
        specific.or_else(|| {
            self.default_sources
                .as_ref()
                .map(|sources| ("default-src", sources))
        })
    }

    fn allows_inline(&self, kind: NativeInlineCspKind, source: &str, nonce: Option<&str>) -> bool {
        inline_csp_sources_allow(
            self.inline_sources_for(kind).map(Vec::as_slice),
            source,
            nonce,
            matches!(
                kind,
                NativeInlineCspKind::ScriptAttribute | NativeInlineCspKind::StyleAttribute
            ),
            matches!(
                kind,
                NativeInlineCspKind::ScriptElement | NativeInlineCspKind::ScriptAttribute
            ),
        )
    }
}

impl NativeCspPolicy {
    fn allows(&self, kind: NativeSubresourceKind, document_url: &Url, resource_url: &Url) -> bool {
        self.policies
            .iter()
            .all(|policy| policy.allows(kind, document_url, resource_url))
    }

    fn allows_redirect(
        &self,
        kind: NativeSubresourceKind,
        document_url: &Url,
        resource_url: &Url,
    ) -> bool {
        self.policies
            .iter()
            .all(|policy| policy.allows_redirect(kind, document_url, resource_url))
    }

    fn allows_script(
        &self,
        document_url: &Url,
        resource_url: &Url,
        parser_inserted: bool,
        nonce: Option<&str>,
    ) -> bool {
        self.policies
            .iter()
            .all(|policy| policy.allows_script(document_url, resource_url, parser_inserted, nonce))
    }

    fn allows_rooted_file_script(
        &self,
        document_url: &Url,
        resource_url: &Url,
        parser_inserted: bool,
        nonce: Option<&str>,
        resource_root: Option<&Path>,
    ) -> bool {
        let same_root = self
            .rooted_file_self_root
            .as_deref()
            .is_some_and(|root| resource_root.is_some_and(|resource_root| resource_root == root));
        self.policies.iter().all(|policy| {
            policy.allows_rooted_file_script(
                document_url,
                resource_url,
                parser_inserted,
                nonce,
                same_root,
            )
        })
    }

    fn allows_rooted_file_style(
        &self,
        document_url: &Url,
        resource_url: &Url,
        nonce: Option<&str>,
        resource_root: Option<&Path>,
    ) -> bool {
        let same_root = self
            .rooted_file_self_root
            .as_deref()
            .is_some_and(|root| resource_root.is_some_and(|resource_root| resource_root == root));
        self.policies.iter().all(|policy| {
            policy.allows_rooted_file_style(document_url, resource_url, nonce, same_root)
        })
    }

    fn allows_rooted_file_resource(
        &self,
        kind: NativeSubresourceKind,
        document_url: &Url,
        resource_url: &Url,
        resource_root: Option<&Path>,
    ) -> bool {
        let same_root = self
            .rooted_file_self_root
            .as_deref()
            .is_some_and(|root| resource_root.is_some_and(|resource_root| resource_root == root));
        self.policies.iter().all(|policy| {
            policy.allows_rooted_file_resource(kind, document_url, resource_url, same_root)
        })
    }

    fn allows_script_redirect(
        &self,
        document_url: &Url,
        resource_url: &Url,
        parser_inserted: bool,
        nonce: Option<&str>,
    ) -> bool {
        self.policies.iter().all(|policy| {
            policy.allows_script_redirect(document_url, resource_url, parser_inserted, nonce)
        })
    }

    fn allows_navigation(
        &self,
        kind: NativeNavigationPolicyKind,
        document_url: &Url,
        target_url: &Url,
    ) -> bool {
        self.policies
            .iter()
            .all(|policy| policy.allows_navigation(kind, document_url, target_url))
    }

    fn report_only_navigation_violations(
        &self,
        kind: NativeNavigationPolicyKind,
        document_url: &Url,
        target_url: &Url,
    ) -> Vec<(&NativeCspDeclaration, &'static str)> {
        self.report_only_policies
            .iter()
            .filter_map(|declaration| {
                let sources = declaration.directives.navigation_sources_for(kind)?;
                (!csp_sources_allow(Some(sources.as_slice()), document_url, target_url))
                    .then_some((declaration, kind.directive()))
            })
            .collect()
    }

    fn allows_inline(&self, kind: NativeInlineCspKind, source: &str, nonce: Option<&str>) -> bool {
        self.policies
            .iter()
            .all(|policy| policy.allows_inline(kind, source, nonce))
    }

    fn frame_source_groups(&self) -> Option<Vec<Vec<String>>> {
        let groups = self
            .policies
            .iter()
            .filter_map(|policy| {
                policy
                    .sources_for(NativeSubresourceKind::Frame)
                    .or(policy.default_sources.as_ref())
                    .cloned()
            })
            .collect::<Vec<_>>();
        (!groups.is_empty()).then_some(groups)
    }

    fn navigation_source_groups(
        &self,
        kind: NativeNavigationPolicyKind,
    ) -> Option<Vec<Vec<String>>> {
        let groups = self
            .policies
            .iter()
            .filter_map(|policy| policy.navigation_sources_for(kind).cloned())
            .collect::<Vec<_>>();
        (!groups.is_empty()).then_some(groups)
    }

    fn report_only_url_violations(
        &self,
        kind: NativeSubresourceKind,
        document_url: &Url,
        resource_url: &Url,
        parser_inserted: bool,
        nonce: Option<&str>,
    ) -> Vec<(&NativeCspDeclaration, &'static str)> {
        self.report_only_url_violations_with_path(
            kind,
            document_url,
            resource_url,
            parser_inserted,
            nonce,
            false,
        )
    }

    fn report_only_url_violations_for_redirect(
        &self,
        kind: NativeSubresourceKind,
        document_url: &Url,
        resource_url: &Url,
        parser_inserted: bool,
        nonce: Option<&str>,
    ) -> Vec<(&NativeCspDeclaration, &'static str)> {
        self.report_only_url_violations_with_path(
            kind,
            document_url,
            resource_url,
            parser_inserted,
            nonce,
            true,
        )
    }

    fn report_only_url_violations_with_path(
        &self,
        kind: NativeSubresourceKind,
        document_url: &Url,
        resource_url: &Url,
        parser_inserted: bool,
        nonce: Option<&str>,
        ignore_path: bool,
    ) -> Vec<(&NativeCspDeclaration, &'static str)> {
        self.report_only_policies
            .iter()
            .filter_map(|declaration| {
                let (directive, sources) = declaration.directives.directive_for(kind)?;
                let allowed = if kind == NativeSubresourceKind::Script {
                    if ignore_path {
                        declaration.directives.allows_script_redirect(
                            document_url,
                            resource_url,
                            parser_inserted,
                            nonce,
                        )
                    } else {
                        declaration.directives.allows_script(
                            document_url,
                            resource_url,
                            parser_inserted,
                            nonce,
                        )
                    }
                } else {
                    let sources = Some(sources.as_slice());
                    if ignore_path {
                        csp_sources_allow_for_redirect(sources, document_url, resource_url)
                    } else {
                        csp_sources_allow(sources, document_url, resource_url)
                    }
                };
                (!allowed).then_some((declaration, directive))
            })
            .collect()
    }

    fn report_only_websocket_violations(
        &self,
        document_url: &Url,
        websocket_url: &Url,
        policy_url: &Url,
    ) -> Vec<(&NativeCspDeclaration, &'static str)> {
        self.report_only_policies
            .iter()
            .filter_map(|declaration| {
                let (directive, sources) = declaration
                    .directives
                    .directive_for(NativeSubresourceKind::Connect)?;
                let allowed =
                    csp_sources_allow(Some(sources.as_slice()), document_url, websocket_url)
                        || csp_sources_allow(Some(sources.as_slice()), document_url, policy_url);
                (!allowed).then_some((declaration, directive))
            })
            .collect()
    }

    fn report_only_inline_violations(
        &self,
        kind: NativeInlineCspKind,
        source: &str,
        nonce: Option<&str>,
    ) -> Vec<(&NativeCspDeclaration, &'static str)> {
        self.report_only_policies
            .iter()
            .filter_map(|declaration| {
                let (directive, sources) = declaration.directives.inline_directive_for(kind)?;
                (!inline_csp_sources_allow(
                    Some(sources.as_slice()),
                    source,
                    nonce,
                    matches!(
                        kind,
                        NativeInlineCspKind::ScriptAttribute | NativeInlineCspKind::StyleAttribute
                    ),
                    matches!(
                        kind,
                        NativeInlineCspKind::ScriptElement | NativeInlineCspKind::ScriptAttribute
                    ),
                ))
                .then_some((declaration, directive))
            })
            .collect()
    }

    fn replace_meta_policies(&mut self, values: &[String]) -> Result<(), NativeEngineError> {
        if values.len() > MAX_NATIVE_CSP_POLICIES {
            return Err(NativeEngineError::limit(
                "CSP meta policies",
                MAX_NATIVE_CSP_POLICIES,
                values.len(),
            ));
        }
        if self.header_policy_count.saturating_add(values.len()) > MAX_NATIVE_CSP_POLICIES {
            return Err(NativeEngineError::limit(
                "CSP policies",
                MAX_NATIVE_CSP_POLICIES,
                self.header_policy_count.saturating_add(values.len()),
            ));
        }
        self.policies.truncate(self.header_policy_count);
        self.policies
            .extend(values.iter().map(|value| parse_csp_directives(value)));
        self.meta_policy_sources = values.to_vec();
        Ok(())
    }

    fn append_meta_policies(&mut self, values: &[String]) -> Result<(), NativeEngineError> {
        if values.len() > MAX_NATIVE_CSP_POLICIES {
            return Err(NativeEngineError::limit(
                "CSP meta policies",
                MAX_NATIVE_CSP_POLICIES,
                values.len(),
            ));
        }
        let next_len = self.policies.len().saturating_add(values.len());
        if next_len > MAX_NATIVE_CSP_POLICIES {
            return Err(NativeEngineError::limit(
                "CSP policies",
                MAX_NATIVE_CSP_POLICIES,
                next_len,
            ));
        }
        self.policies
            .extend(values.iter().map(|value| parse_csp_directives(value)));
        self.meta_policy_sources.extend(values.iter().cloned());
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct NativeInlineScriptPolicy {
    policies: Vec<NativeCspDirectives>,
    report_only_policies: Vec<NativeCspDeclaration>,
    reporting_endpoints: BTreeMap<String, Vec<String>>,
}

impl NativeInlineScriptPolicy {
    pub(crate) fn append_meta_policy(&mut self, value: &str) -> Result<(), NativeEngineError> {
        let next_len = self.policies.len().saturating_add(1);
        if next_len > MAX_NATIVE_CSP_POLICIES {
            return Err(NativeEngineError::limit(
                "CSP policies",
                MAX_NATIVE_CSP_POLICIES,
                next_len,
            ));
        }
        self.policies.push(parse_csp_directives(value));
        Ok(())
    }

    pub(crate) fn allows(&self, source: &str, nonce: Option<&str>) -> bool {
        self.policies
            .iter()
            .all(|policy| policy.allows_inline(NativeInlineCspKind::ScriptElement, source, nonce))
    }

    pub(crate) fn allows_attribute(&self, source: &str) -> bool {
        self.policies
            .iter()
            .all(|policy| policy.allows_inline(NativeInlineCspKind::ScriptAttribute, source, None))
    }

    pub(crate) fn report_only_inline_script_violations(
        &self,
        document_url: &str,
        source: &str,
        nonce: Option<&str>,
    ) -> Vec<NativeCspViolation> {
        let Ok(document_url) = Url::parse(without_fragment(document_url)) else {
            return Vec::new();
        };
        if !is_network_url(document_url.as_str()) {
            return Vec::new();
        }
        let sample = source.chars().take(40).collect::<String>();
        let mut violations = Vec::new();
        let mut deliveries = Vec::new();
        for declaration in &self.report_only_policies {
            let Some((directive, sources)) = declaration
                .directives
                .inline_directive_for(NativeInlineCspKind::ScriptElement)
            else {
                continue;
            };
            if inline_csp_sources_allow(Some(sources.as_slice()), source, nonce, false, true) {
                continue;
            }
            if violations.len() >= MAX_NATIVE_CSP_VIOLATIONS {
                break;
            }
            let violation = NativeCspViolation {
                document_uri: without_fragment(document_url.as_str()).to_owned(),
                referrer: String::new(),
                blocked_uri: "inline".into(),
                effective_directive: directive.into(),
                violated_directive: directive.into(),
                original_policy: declaration.original_policy.clone(),
                source_file: String::new(),
                sample: sample.clone(),
                disposition: "report".into(),
                status_code: 0,
                line_number: 0,
                column_number: 0,
            };
            deliveries.extend(csp_report_deliveries_for_declaration(
                &self.reporting_endpoints,
                declaration,
                &document_url,
                &violation,
            ));
            violations.push(violation);
        }
        schedule_native_csp_report_deliveries(deliveries);
        violations
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct NativeCspHostSource<'a> {
    scheme: Option<&'a str>,
    host: &'a str,
    port: Option<&'a str>,
    path: Option<&'a str>,
}

/// Evaluate the bounded CSP source-expression grammar shared by document
/// subresources and parent-owned frame navigation. Keeping this matcher in
/// the loader prevents the content worker and frame registry from drifting
/// into different CSP decisions.
pub(crate) fn csp_sources_allow(
    sources: Option<&[String]>,
    document_url: &Url,
    resource_url: &Url,
) -> bool {
    csp_sources_allow_with_path(sources, document_url, resource_url, false)
}

/// Evaluate a CSP URL source expression for an HTTP redirect. CSP ignores a
/// host-source path on redirect hops to avoid leaking path information while
/// retaining scheme, host, and port authorization.
pub(crate) fn csp_sources_allow_for_redirect(
    sources: Option<&[String]>,
    document_url: &Url,
    resource_url: &Url,
) -> bool {
    csp_sources_allow_with_path(sources, document_url, resource_url, true)
}

fn csp_sources_allow_with_path(
    sources: Option<&[String]>,
    document_url: &Url,
    resource_url: &Url,
    ignore_path: bool,
) -> bool {
    let Some(sources) = sources else {
        return true;
    };
    if sources.is_empty() {
        return false;
    }

    if sources.len() == 1 && sources[0].eq_ignore_ascii_case("'none'") {
        return false;
    }

    for source in sources {
        if source.eq_ignore_ascii_case("'none'") {
            continue;
        }
        if csp_source_expression_matches(source, document_url, resource_url, ignore_path) {
            return true;
        }
    }
    false
}

fn csp_script_sources_allow(
    sources: Option<&[String]>,
    document_url: &Url,
    resource_url: &Url,
    parser_inserted: bool,
    nonce: Option<&str>,
) -> bool {
    csp_script_sources_allow_with_path(
        sources,
        document_url,
        resource_url,
        parser_inserted,
        nonce,
        false,
    )
}

fn csp_script_sources_allow_for_redirect(
    sources: Option<&[String]>,
    document_url: &Url,
    resource_url: &Url,
    parser_inserted: bool,
    nonce: Option<&str>,
) -> bool {
    csp_script_sources_allow_with_path(
        sources,
        document_url,
        resource_url,
        parser_inserted,
        nonce,
        true,
    )
}

fn csp_script_sources_allow_with_path(
    sources: Option<&[String]>,
    document_url: &Url,
    resource_url: &Url,
    parser_inserted: bool,
    nonce: Option<&str>,
    ignore_path: bool,
) -> bool {
    let Some(sources) = sources else {
        return true;
    };
    if nonce.is_some_and(|nonce| {
        sources
            .iter()
            .any(|candidate| csp_nonce_matches(candidate, nonce))
    }) {
        return true;
    }
    if sources
        .iter()
        .any(|candidate| candidate.eq_ignore_ascii_case("'strict-dynamic'"))
    {
        return !parser_inserted;
    }
    csp_sources_allow_with_path(Some(sources), document_url, resource_url, ignore_path)
}

fn csp_sources_allow_for_rooted_file(
    sources: Option<&[String]>,
    document_url: &Url,
    resource_url: &Url,
    nonce: Option<&str>,
    same_root: bool,
) -> bool {
    let Some(sources) = sources else {
        return true;
    };
    if nonce.is_some_and(|nonce| {
        sources
            .iter()
            .any(|candidate| csp_nonce_matches(candidate, nonce))
    }) {
        return true;
    }
    sources.iter().any(|source| {
        if source.eq_ignore_ascii_case("'none'") {
            return false;
        }
        if source.eq_ignore_ascii_case("'self'") {
            return same_root;
        }
        csp_source_expression_matches(source, document_url, resource_url, false)
    })
}

fn csp_script_sources_allow_for_rooted_file(
    sources: Option<&[String]>,
    document_url: &Url,
    resource_url: &Url,
    parser_inserted: bool,
    nonce: Option<&str>,
    same_root: bool,
) -> bool {
    let Some(sources) = sources else {
        return true;
    };
    if nonce.is_some_and(|nonce| {
        sources
            .iter()
            .any(|candidate| csp_nonce_matches(candidate, nonce))
    }) {
        return true;
    }
    if sources
        .iter()
        .any(|candidate| candidate.eq_ignore_ascii_case("'strict-dynamic'"))
    {
        return !parser_inserted;
    }
    sources.iter().any(|source| {
        if source.eq_ignore_ascii_case("'none'") {
            return false;
        }
        if source.eq_ignore_ascii_case("'self'") {
            return same_root;
        }
        csp_source_expression_matches(source, document_url, resource_url, false)
    })
}

fn csp_source_expression_matches(
    expression: &str,
    document_url: &Url,
    resource_url: &Url,
    ignore_path: bool,
) -> bool {
    if expression.is_empty() || expression.len() > MAX_NATIVE_CSP_SOURCE_EXPRESSION_BYTES {
        return false;
    }
    if expression == "*" {
        return matches!(resource_url.scheme(), "http" | "https")
            || resource_url
                .scheme()
                .eq_ignore_ascii_case(document_url.scheme());
    }

    if expression.eq_ignore_ascii_case("'self'") {
        return csp_self_source_matches(document_url, resource_url);
    }

    if let Some(scheme) = expression.strip_suffix(':')
        && !expression.contains("://")
        && csp_scheme_part_is_valid(scheme)
    {
        return csp_scheme_part_matches(scheme, resource_url.scheme());
    }

    let Some(source) = parse_csp_host_source(expression) else {
        return false;
    };
    let Some(resource_host) = resource_url.host_str() else {
        return false;
    };
    let required_scheme = source.scheme.unwrap_or(document_url.scheme());
    if !csp_scheme_part_matches(required_scheme, resource_url.scheme())
        || !csp_host_part_matches(source.host, resource_host)
        || !csp_port_part_matches(source.port, resource_url)
    {
        return false;
    }

    source
        .path
        .is_none_or(|path| ignore_path || csp_path_part_matches(path, resource_url.path()))
}

fn parse_csp_host_source(expression: &str) -> Option<NativeCspHostSource<'_>> {
    if expression.is_empty() || !expression.is_ascii() {
        return None;
    }

    let (scheme, authority_and_path) = if let Some((scheme, rest)) = expression.split_once("://") {
        (Some(scheme), rest)
    } else {
        (None, expression)
    };
    if scheme.is_some_and(|scheme| !csp_scheme_part_is_valid(scheme)) {
        return None;
    }

    let (authority, path) =
        authority_and_path
            .find('/')
            .map_or((authority_and_path, None), |path_start| {
                (
                    &authority_and_path[..path_start],
                    Some(&authority_and_path[path_start..]),
                )
            });
    if authority.is_empty() {
        return None;
    }

    let (host, port) = match authority.rsplit_once(':') {
        None => (authority, None),
        Some((host, port)) => {
            if port.is_empty()
                || (port != "*"
                    && (!port.bytes().all(|byte| byte.is_ascii_digit())
                        || port.parse::<u16>().is_err()))
            {
                return None;
            }
            (host, Some(port))
        }
    };
    if !csp_host_part_is_valid(host) {
        return None;
    }
    if path.is_some_and(|path| !csp_path_part_is_valid(path)) {
        return None;
    }

    Some(NativeCspHostSource {
        scheme,
        host,
        port,
        path,
    })
}

fn csp_scheme_part_is_valid(value: &str) -> bool {
    let mut bytes = value.bytes();
    bytes.next().is_some_and(|byte| byte.is_ascii_alphabetic())
        && bytes.all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'+' | b'-' | b'.'))
}

fn csp_scheme_part_matches(pattern: &str, resource_scheme: &str) -> bool {
    pattern.eq_ignore_ascii_case(resource_scheme)
        || (pattern.eq_ignore_ascii_case("http") && resource_scheme.eq_ignore_ascii_case("https"))
        || (pattern.eq_ignore_ascii_case("ws")
            && matches!(
                resource_scheme.to_ascii_lowercase().as_str(),
                "wss" | "http" | "https"
            ))
        || (pattern.eq_ignore_ascii_case("wss") && resource_scheme.eq_ignore_ascii_case("https"))
}

fn csp_host_part_is_valid(value: &str) -> bool {
    if value == "*" {
        return true;
    }
    let host = value.strip_prefix("*.").unwrap_or(value);
    if host.is_empty() || host.contains('*') {
        return false;
    }
    let host = host.strip_suffix('.').unwrap_or(host);
    !host.is_empty()
        && host.split('.').all(|label| {
            !label.is_empty()
                && label
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
        })
}

fn csp_host_part_matches(pattern: &str, resource_host: &str) -> bool {
    let pattern = pattern.trim_end_matches('.').to_ascii_lowercase();
    let resource_host = resource_host.trim_end_matches('.').to_ascii_lowercase();
    if pattern == "*" {
        return true;
    }
    if let Some(suffix) = pattern.strip_prefix("*.") {
        return resource_host.len() > suffix.len()
            && resource_host
                .strip_suffix(suffix)
                .is_some_and(|prefix| prefix.ends_with('.'));
    }
    pattern.eq_ignore_ascii_case(&resource_host)
}

fn csp_default_port(scheme: &str) -> Option<u16> {
    match scheme.to_ascii_lowercase().as_str() {
        "http" | "ws" => Some(80),
        "https" | "wss" => Some(443),
        "ftp" => Some(21),
        _ => None,
    }
}

fn csp_port_part_matches(port: Option<&str>, resource_url: &Url) -> bool {
    if port == Some("*") {
        return true;
    }

    let resource_port = resource_url.port();
    let default_port = csp_default_port(resource_url.scheme());
    let effective_resource_port = resource_port.or(default_port);
    match port {
        None => resource_port.is_none() || resource_port == default_port,
        Some(port) => port
            .parse::<u16>()
            .ok()
            .is_some_and(|port| Some(port) == effective_resource_port),
    }
}

fn csp_path_part_is_valid(path: &str) -> bool {
    if !path.starts_with('/') || !path.is_ascii() {
        return false;
    }
    let bytes = path.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        let byte = bytes[index];
        if byte == b'%' {
            if index + 2 >= bytes.len()
                || hex_value(bytes[index + 1]).is_none()
                || hex_value(bytes[index + 2]).is_none()
            {
                return false;
            }
            index += 3;
            continue;
        }
        if !(byte.is_ascii_alphanumeric()
            || matches!(
                byte,
                b'-' | b'.'
                    | b'_'
                    | b'~'
                    | b'!'
                    | b'$'
                    | b'&'
                    | b'\''
                    | b'('
                    | b')'
                    | b'*'
                    | b'+'
                    | b'='
                    | b':'
                    | b'@'
                    | b'/'
            ))
        {
            return false;
        }
        index += 1;
    }
    true
}

fn csp_percent_decode_path_piece(value: &str) -> Option<Vec<u8>> {
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
            return None;
        }
        let high = hex_value(bytes[index + 1])?;
        let low = hex_value(bytes[index + 2])?;
        decoded.push((high << 4) | low);
        index += 3;
    }
    Some(decoded)
}

fn csp_path_part_matches(pattern: &str, resource_path: &str) -> bool {
    if pattern.is_empty() || (pattern == "/" && resource_path.is_empty()) {
        return true;
    }

    let exact_match = !pattern.ends_with('/');
    let mut pattern_pieces = pattern.split('/').collect::<Vec<_>>();
    let resource_pieces = resource_path.split('/').collect::<Vec<_>>();
    if pattern_pieces.len() > resource_pieces.len()
        || (exact_match && pattern_pieces.len() != resource_pieces.len())
    {
        return false;
    }
    if !exact_match {
        pattern_pieces.pop();
    }

    pattern_pieces
        .iter()
        .zip(resource_pieces.iter())
        .all(|(pattern_piece, resource_piece)| {
            csp_percent_decode_path_piece(pattern_piece)
                == csp_percent_decode_path_piece(resource_piece)
        })
}

fn csp_self_source_matches(document_url: &Url, resource_url: &Url) -> bool {
    if resource_url.scheme().eq_ignore_ascii_case("blob") {
        return false;
    }
    if document_url.origin() == resource_url.origin() {
        return true;
    }

    let Some(document_host) = document_url.host_str() else {
        return false;
    };
    let Some(resource_host) = resource_url.host_str() else {
        return false;
    };
    if !document_host.eq_ignore_ascii_case(resource_host)
        || !csp_upgrade_ports_compatible(document_url, resource_url)
    {
        return false;
    }

    match (
        document_url.scheme().to_ascii_lowercase().as_str(),
        resource_url.scheme().to_ascii_lowercase().as_str(),
    ) {
        ("http" | "https", "https" | "wss") | ("http", "ws") => true,
        _ => false,
    }
}

fn csp_upgrade_ports_compatible(document_url: &Url, resource_url: &Url) -> bool {
    let document_default = csp_default_port(document_url.scheme());
    let resource_default = csp_default_port(resource_url.scheme());
    let document_effective = document_url.port().or(document_default);
    let resource_effective = resource_url.port().or(resource_default);
    document_effective == resource_effective
        || (document_url
            .port()
            .is_none_or(|port| Some(port) == document_default)
            && resource_url
                .port()
                .is_none_or(|port| Some(port) == resource_default))
}

fn inline_csp_sources_allow(
    sources: Option<&[String]>,
    source: &str,
    nonce: Option<&str>,
    style_attribute: bool,
    strict_dynamic: bool,
) -> bool {
    let Some(sources) = sources else {
        return true;
    };
    if sources.is_empty() {
        return false;
    }
    let has_nonce_or_hash = sources
        .iter()
        .any(|source| csp_nonce_or_hash_source(source));
    let strict_dynamic = strict_dynamic
        && sources
            .iter()
            .any(|candidate| candidate.eq_ignore_ascii_case("'strict-dynamic'"));
    if !has_nonce_or_hash
        && !strict_dynamic
        && sources
            .iter()
            .any(|candidate| candidate.eq_ignore_ascii_case("'unsafe-inline'"))
    {
        return true;
    }
    if !style_attribute
        && nonce.is_some_and(|nonce| {
            sources
                .iter()
                .any(|candidate| csp_nonce_matches(candidate, nonce))
        })
    {
        return true;
    }
    let unsafe_hashes = sources
        .iter()
        .any(|candidate| candidate.eq_ignore_ascii_case("'unsafe-hashes'"));
    if style_attribute && !unsafe_hashes {
        return false;
    }
    sources
        .iter()
        .any(|candidate| csp_hash_matches(candidate, source))
}

fn csp_nonce_or_hash_source(source: &str) -> bool {
    let source = source.to_ascii_lowercase();
    source.ends_with('\'')
        && ["'nonce-", "'sha256-", "'sha384-", "'sha512-"]
            .iter()
            .any(|prefix| source.starts_with(prefix))
}

fn csp_nonce_matches(source: &str, nonce: &str) -> bool {
    let Some(value) = source.strip_suffix('\'').and_then(|source| source.get(7..)) else {
        return false;
    };
    source
        .get(..7)
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case("'nonce-"))
        && value == nonce
}

fn csp_hash_matches(source: &str, value: &str) -> bool {
    let Some(source) = source.strip_suffix('\'') else {
        return false;
    };
    let Some((algorithm, expected)) = source.get(1..).and_then(|source| source.split_once('-'))
    else {
        return false;
    };
    let digest = match algorithm.to_ascii_lowercase().as_str() {
        "sha256" => Sha256::digest(value.as_bytes()).to_vec(),
        "sha384" => Sha384::digest(value.as_bytes()).to_vec(),
        "sha512" => Sha512::digest(value.as_bytes()).to_vec(),
        _ => return false,
    };
    base64::engine::general_purpose::STANDARD.encode(digest) == expected
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

impl NativeResourceLoader {
    pub(crate) fn new(config: &NativeEngineConfig) -> Result<Self, NativeEngineError> {
        config.validate()?;
        let fixtures = config
            .fixtures
            .iter()
            .map(|fixture| (fixture.url.clone(), fixture.html.clone()))
            .collect();
        let allowed_file_roots = canonical_file_roots(&config.allowed_file_roots)?;
        let cookies = load_cookie_profile(config.storage_path.as_deref())?;
        Ok(Self {
            fixtures,
            allowed_file_roots,
            max_document_bytes: config.limits.max_document_bytes,
            network: NativeNetworkState::from_profile(cookies)?,
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
    ) -> reqwest::RequestBuilder {
        request
            .header(reqwest::header::USER_AGENT, self.environment.user_agent())
            .header(
                reqwest::header::ACCEPT_LANGUAGE,
                self.environment.accept_language(),
            )
    }

    pub(crate) fn max_document_bytes(&self) -> usize {
        self.max_document_bytes
    }

    pub(crate) fn take_csp_violations(&mut self) -> Vec<NativeCspViolation> {
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

    fn csp_violation_record(
        document_url: &Url,
        blocked_uri: impl Into<String>,
        effective_directive: &str,
        original_policy: &str,
        sample: impl Into<String>,
    ) -> NativeCspViolation {
        NativeCspViolation {
            document_uri: without_fragment(document_url.as_str()).to_owned(),
            referrer: String::new(),
            blocked_uri: blocked_uri.into(),
            effective_directive: effective_directive.to_owned(),
            violated_directive: effective_directive.to_owned(),
            original_policy: original_policy.to_owned(),
            source_file: String::new(),
            sample: sample.into(),
            disposition: "report".into(),
            status_code: 0,
            line_number: 0,
            column_number: 0,
        }
    }

    fn queue_csp_violation(
        &mut self,
        policy: &NativeCspPolicy,
        declaration: &NativeCspDeclaration,
        document_url: &Url,
        blocked_uri: impl Into<String>,
        effective_directive: &str,
        original_policy: &str,
        sample: impl Into<String>,
    ) {
        if self.csp_violations.len() >= MAX_NATIVE_CSP_VIOLATIONS {
            return;
        }
        let violation = Self::csp_violation_record(
            document_url,
            blocked_uri,
            effective_directive,
            original_policy,
            sample,
        );
        schedule_native_csp_report_deliveries(csp_report_deliveries_for_declaration(
            &policy.reporting_endpoints,
            declaration,
            document_url,
            &violation,
        ));
        self.csp_violations.push(violation);
    }

    fn record_report_only_url_violations(
        &mut self,
        policy: &NativeCspPolicy,
        kind: NativeSubresourceKind,
        document_url: &Url,
        resource_url: &Url,
    ) {
        self.record_report_only_url_violations_with_metadata(
            policy,
            kind,
            document_url,
            resource_url,
            true,
            None,
        );
    }

    fn record_report_only_url_violations_for_redirect(
        &mut self,
        policy: &NativeCspPolicy,
        kind: NativeSubresourceKind,
        document_url: &Url,
        resource_url: &Url,
        parser_inserted: bool,
        nonce: Option<&str>,
    ) {
        self.record_report_only_url_violations_with_metadata_and_path(
            policy,
            kind,
            document_url,
            resource_url,
            parser_inserted,
            nonce,
            true,
        );
    }

    fn record_report_only_url_violations_with_metadata(
        &mut self,
        policy: &NativeCspPolicy,
        kind: NativeSubresourceKind,
        document_url: &Url,
        resource_url: &Url,
        parser_inserted: bool,
        nonce: Option<&str>,
    ) {
        self.record_report_only_url_violations_with_metadata_and_path(
            policy,
            kind,
            document_url,
            resource_url,
            parser_inserted,
            nonce,
            false,
        );
    }

    fn record_report_only_url_violations_with_metadata_and_path(
        &mut self,
        policy: &NativeCspPolicy,
        kind: NativeSubresourceKind,
        document_url: &Url,
        resource_url: &Url,
        parser_inserted: bool,
        nonce: Option<&str>,
        ignore_path: bool,
    ) {
        let blocked_uri = without_fragment(resource_url.as_str()).to_owned();
        let violations = if ignore_path {
            policy.report_only_url_violations_for_redirect(
                kind,
                document_url,
                resource_url,
                parser_inserted,
                nonce,
            )
        } else {
            policy.report_only_url_violations(
                kind,
                document_url,
                resource_url,
                parser_inserted,
                nonce,
            )
        };
        for (declaration, directive) in violations {
            self.queue_csp_violation(
                policy,
                declaration,
                document_url,
                blocked_uri.clone(),
                directive,
                &declaration.original_policy,
                "",
            );
        }
    }

    fn record_report_only_navigation_violations(
        &mut self,
        policy: &NativeCspPolicy,
        kind: NativeNavigationPolicyKind,
        document_url: &Url,
        target_url: &Url,
    ) {
        let blocked_uri = without_fragment(target_url.as_str()).to_owned();
        for (declaration, directive) in
            policy.report_only_navigation_violations(kind, document_url, target_url)
        {
            self.queue_csp_violation(
                policy,
                declaration,
                document_url,
                blocked_uri.clone(),
                directive,
                &declaration.original_policy,
                "",
            );
        }
    }

    fn record_report_only_inline_violations(
        &mut self,
        policy: &NativeCspPolicy,
        kind: NativeInlineCspKind,
        document_url: &Url,
        source: &str,
        nonce: Option<&str>,
    ) {
        let sample = source.chars().take(40).collect::<String>();
        for (declaration, directive) in policy.report_only_inline_violations(kind, source, nonce) {
            self.queue_csp_violation(
                policy,
                declaration,
                document_url,
                "inline",
                directive,
                &declaration.original_policy,
                sample.clone(),
            );
        }
    }

    pub(crate) fn inline_script_policy(
        &self,
        document_url: &str,
    ) -> Result<NativeInlineScriptPolicy, NativeEngineError> {
        Ok(self
            .document_policy(document_url)?
            .map(|policy| NativeInlineScriptPolicy {
                policies: policy.policies.clone(),
                report_only_policies: policy.report_only_policies.clone(),
                reporting_endpoints: policy.reporting_endpoints.clone(),
            })
            .unwrap_or_default())
    }

    pub(crate) fn allows_inline_script(
        &mut self,
        document_url: &str,
        source: &str,
        nonce: Option<&str>,
    ) -> Result<bool, NativeEngineError> {
        let Some((document_url, _)) = self.csp_document_owner(document_url, "CSP document URL")?
        else {
            return Ok(true);
        };
        let policy = self
            .network
            .document_policies
            .get(&cache_key(&document_url))
            .cloned()
            .unwrap_or_default();
        self.record_report_only_inline_violations(
            &policy,
            NativeInlineCspKind::ScriptElement,
            &document_url,
            source,
            nonce,
        );
        Ok(policy.allows_inline(NativeInlineCspKind::ScriptElement, source, nonce))
    }

    pub(crate) fn allows_inline_style_element(
        &mut self,
        document_url: &str,
        source: &str,
        nonce: Option<&str>,
    ) -> Result<bool, NativeEngineError> {
        self.allows_inline_style_element_with_reporting(document_url, source, nonce, true)
    }

    pub(crate) fn allows_inline_style_element_silent(
        &mut self,
        document_url: &str,
        source: &str,
        nonce: Option<&str>,
    ) -> Result<bool, NativeEngineError> {
        self.allows_inline_style_element_with_reporting(document_url, source, nonce, false)
    }

    fn allows_inline_style_element_with_reporting(
        &mut self,
        document_url: &str,
        source: &str,
        nonce: Option<&str>,
        report: bool,
    ) -> Result<bool, NativeEngineError> {
        let Some((document_url, _)) = self.csp_document_owner(document_url, "CSP document URL")?
        else {
            return Ok(true);
        };
        let policy = self
            .network
            .document_policies
            .get(&cache_key(&document_url))
            .cloned()
            .unwrap_or_default();
        if report {
            self.record_report_only_inline_violations(
                &policy,
                NativeInlineCspKind::StyleElement,
                &document_url,
                source,
                nonce,
            );
        }
        Ok(policy.allows_inline(NativeInlineCspKind::StyleElement, source, nonce))
    }

    pub(crate) fn allows_inline_style_attribute(
        &mut self,
        document_url: &str,
        source: &str,
    ) -> Result<bool, NativeEngineError> {
        self.allows_inline_style_attribute_with_reporting(document_url, source, true)
    }

    pub(crate) fn allows_inline_style_attribute_silent(
        &mut self,
        document_url: &str,
        source: &str,
    ) -> Result<bool, NativeEngineError> {
        self.allows_inline_style_attribute_with_reporting(document_url, source, false)
    }

    fn allows_inline_style_attribute_with_reporting(
        &mut self,
        document_url: &str,
        source: &str,
        report: bool,
    ) -> Result<bool, NativeEngineError> {
        let Some((document_url, _)) = self.csp_document_owner(document_url, "CSP document URL")?
        else {
            return Ok(true);
        };
        let policy = self
            .network
            .document_policies
            .get(&cache_key(&document_url))
            .cloned()
            .unwrap_or_default();
        if report {
            self.record_report_only_inline_violations(
                &policy,
                NativeInlineCspKind::StyleAttribute,
                &document_url,
                source,
                None,
            );
        }
        Ok(policy.allows_inline(NativeInlineCspKind::StyleAttribute, source, None))
    }

    fn document_policy(
        &self,
        document_url: &str,
    ) -> Result<Option<&NativeCspPolicy>, NativeEngineError> {
        let Some((document_url, _)) = self.csp_document_owner(document_url, "CSP document URL")?
        else {
            return Ok(None);
        };
        Ok(self
            .network
            .document_policies
            .get(&cache_key(&document_url)))
    }

    fn csp_document_owner(
        &self,
        document_url: &str,
        resource_name: &str,
    ) -> Result<Option<(Url, Option<PathBuf>)>, NativeEngineError> {
        validate_url_text(resource_name, document_url)?;
        let document_url = Url::parse(without_fragment(document_url)).map_err(|_| {
            NativeEngineError::UnsupportedUrl {
                reason: format!("{resource_name} is not valid URL syntax"),
            }
        })?;
        reject_credentials(&document_url)?;
        if is_network_url(document_url.as_str()) {
            return Ok(Some((document_url, None)));
        }
        if !is_file_url(document_url.as_str()) {
            return Ok(None);
        }
        let Some((_, path)) = self.local_file_subresource_path(
            document_url.as_str(),
            document_url.as_str(),
            resource_name,
        )?
        else {
            return Ok(None);
        };
        let file_root = self
            .allowed_file_root_for_path(&path)
            .map(Path::to_path_buf);
        Ok(file_root.map(|file_root| (document_url, Some(file_root))))
    }

    pub(crate) fn websocket_target(
        &self,
        document_url: &str,
        href: &str,
    ) -> Result<NativeWebSocketTarget, NativeEngineError> {
        validate_url_text("WebSocket owner URL", document_url)?;
        validate_url_text("WebSocket URL", href)?;
        let document_url = Url::parse(without_fragment(document_url)).map_err(|_| {
            NativeEngineError::UnsupportedUrl {
                reason: "WebSocket owner URL is not valid HTTP(S) syntax".into(),
            }
        })?;
        if !is_network_url(document_url.as_str()) {
            return Err(NativeEngineError::UnsupportedUrl {
                reason: "WebSocket requires an HTTP(S) document owner".into(),
            });
        }
        reject_credentials(&document_url)?;
        let target_url =
            document_url
                .join(href)
                .map_err(|_| NativeEngineError::UnsupportedUrl {
                    reason: "WebSocket URL could not be resolved against the document".into(),
                })?;
        reject_credentials(&target_url)?;
        if !matches!(target_url.scheme(), "ws" | "wss") {
            return Err(NativeEngineError::UnsupportedUrl {
                reason: "WebSocket URL must use ws or wss".into(),
            });
        }
        if document_url.scheme() == "https" && target_url.scheme() == "ws" {
            return Err(NativeEngineError::Network {
                operation: "WebSocket policy".into(),
                reason: "HTTPS documents cannot open insecure ws connections".into(),
            });
        }
        let policy = self
            .network
            .document_policies
            .get(&cache_key(&document_url))
            .cloned()
            .unwrap_or_default();
        let mut policy_target = target_url.clone();
        policy_target
            .set_scheme(if target_url.scheme() == "wss" {
                "https"
            } else {
                "http"
            })
            .map_err(|_| NativeEngineError::UnsupportedUrl {
                reason: "WebSocket policy URL could not be normalized".into(),
            })?;
        let blocked_uri = without_fragment(target_url.as_str()).to_owned();
        let mut csp_violations = Vec::new();
        let mut csp_report_deliveries = Vec::new();
        for (declaration, directive) in policy
            .report_only_websocket_violations(&document_url, &target_url, &policy_target)
            .into_iter()
            .take(MAX_NATIVE_CSP_VIOLATIONS)
        {
            let violation = Self::csp_violation_record(
                &document_url,
                blocked_uri.clone(),
                directive,
                &declaration.original_policy,
                "",
            );
            csp_report_deliveries.extend(csp_report_deliveries_for_declaration(
                &policy.reporting_endpoints,
                declaration,
                &document_url,
                &violation,
            ));
            csp_violations.push(violation);
        }
        if !policy.allows(NativeSubresourceKind::Connect, &document_url, &target_url)
            && !policy.allows(
                NativeSubresourceKind::Connect,
                &document_url,
                &policy_target,
            )
        {
            return Err(NativeEngineError::Network {
                operation: "WebSocket policy".into(),
                reason: "document CSP blocked the WebSocket connect target".into(),
            });
        }
        let mut cookie_target = target_url.clone();
        cookie_target
            .set_scheme(if target_url.scheme() == "wss" {
                "https"
            } else {
                "http"
            })
            .map_err(|_| NativeEngineError::UnsupportedUrl {
                reason: "WebSocket cookie URL could not be normalized".into(),
            })?;
        Ok(NativeWebSocketTarget {
            url: target_url,
            cookie: self.network.cookie_header_for_request(
                &cookie_target,
                Some(&document_url),
                false,
                NativeNavigationMethod::Get,
            ),
            csp_violations,
            csp_report_deliveries,
        })
    }

    pub(crate) fn apply_websocket_response_cookies(
        &mut self,
        websocket_url: &Url,
        set_cookie_headers: &[String],
    ) -> Result<(), NativeEngineError> {
        if set_cookie_headers.len() > MAX_NATIVE_RESPONSE_HEADERS {
            return Err(NativeEngineError::limit(
                "WebSocket Set-Cookie response headers",
                MAX_NATIVE_RESPONSE_HEADERS,
                set_cookie_headers.len(),
            ));
        }
        let mut cookie_url = websocket_url.clone();
        cookie_url
            .set_scheme(if websocket_url.scheme() == "wss" {
                "https"
            } else if websocket_url.scheme() == "ws" {
                "http"
            } else {
                return Err(NativeEngineError::UnsupportedUrl {
                    reason: "WebSocket cookie response URL must use ws or wss".into(),
                });
            })
            .map_err(|_| NativeEngineError::UnsupportedUrl {
                reason: "WebSocket cookie response URL could not be normalized".into(),
            })?;

        for header in set_cookie_headers {
            if header.len() > MAX_NATIVE_RESPONSE_HEADER_VALUE_BYTES {
                return Err(NativeEngineError::limit(
                    "WebSocket Set-Cookie response header bytes",
                    MAX_NATIVE_RESPONSE_HEADER_VALUE_BYTES,
                    header.len(),
                ));
            }
            let secure_cookie = header
                .split(';')
                .skip(1)
                .any(|attribute| attribute.trim().eq_ignore_ascii_case("secure"));
            if cookie_url.scheme() == "http" && secure_cookie {
                continue;
            }
            self.cookie_changes
                .extend(self.network.store_cookie(&cookie_url, header));
        }
        Ok(())
    }

    /// Open one EventSource response through the shared URL, CSP, mixed
    /// content, referrer, redirect, cookie, and CORS policy owner. The
    /// response body is intentionally returned as a stream to the content
    /// process; buffering it here would defeat Server-Sent Events semantics.
    pub(crate) async fn open_event_source_async(
        &mut self,
        initiator_url: &str,
        href: &str,
        with_credentials: bool,
        last_event_id: &str,
    ) -> Result<(Url, reqwest::Response), NativeEngineError> {
        validate_url_text("EventSource initiator URL", initiator_url)?;
        validate_url_text("EventSource URL", href)?;
        if last_event_id.len() > 128
            || last_event_id
                .bytes()
                .any(|byte| matches!(byte, b'\r' | b'\n'))
        {
            return Err(NativeEngineError::invalid(
                "EventSource last event ID",
                "must be a bounded value without line breaks",
            ));
        }
        let document_url = Url::parse(without_fragment(initiator_url)).map_err(|_| {
            NativeEngineError::UnsupportedUrl {
                reason: "EventSource initiator URL is not valid HTTP(S) syntax".into(),
            }
        })?;
        if !is_network_url(document_url.as_str()) {
            return Err(NativeEngineError::UnsupportedUrl {
                reason: "EventSource requires an HTTP(S) initiator".into(),
            });
        }
        reject_credentials(&document_url)?;
        let Some(target_url) = resolve_subresource_url(&document_url, href)? else {
            return Err(NativeEngineError::UnsupportedUrl {
                reason: "EventSource URL must be an HTTP(S) resource".into(),
            });
        };
        if !mixed_content_allowed(&document_url, &target_url) {
            return Err(NativeEngineError::Network {
                operation: "EventSource policy".into(),
                reason: "HTTPS documents cannot open an HTTP EventSource".into(),
            });
        }
        let policy = self
            .network
            .document_policies
            .get(&cache_key(&document_url))
            .cloned()
            .unwrap_or_default();
        self.record_report_only_url_violations(
            &policy,
            NativeSubresourceKind::Connect,
            &document_url,
            &target_url,
        );
        if !policy.allows(NativeSubresourceKind::Connect, &document_url, &target_url) {
            return Err(NativeEngineError::Network {
                operation: "EventSource policy".into(),
                reason: "document CSP blocked the EventSource connect target".into(),
            });
        }
        let client = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .connect_timeout(NATIVE_NETWORK_TIMEOUT)
            .build()
            .map_err(|error| network_error("EventSource client construction", error))?;
        let mut current_url = target_url;
        let mut request_referrer = normalize_referrer(Some(document_url.as_str()), &current_url)?;
        let mut redirects = 0;
        let response = loop {
            let mut request_url = current_url.clone();
            request_url.set_fragment(None);
            let mut request = self.apply_environment_headers(
                client
                    .get(request_url)
                    .header(reqwest::header::ACCEPT, "text/event-stream")
                    .header(reqwest::header::CACHE_CONTROL, "no-cache"),
            );
            if current_url.origin() != document_url.origin() {
                request = request.header(
                    reqwest::header::ORIGIN,
                    document_url.origin().ascii_serialization(),
                );
            }
            if let Some(referrer) = request_referrer.as_deref() {
                request = request.header(reqwest::header::REFERER, referrer);
            }
            let same_origin = current_url.origin() == document_url.origin();
            if (with_credentials || same_origin)
                && let Some(cookie) = self.network.cookie_header_for_request(
                    &current_url,
                    Some(&document_url),
                    false,
                    NativeNavigationMethod::Get,
                )
            {
                request = request.header(reqwest::header::COOKIE, cookie);
            }
            if !last_event_id.is_empty() {
                request = request.header("last-event-id", last_event_id);
            }
            self.before_request(0).await?;
            let response = request
                .send()
                .await
                .map_err(|error| network_error("EventSource request", error))?;
            if !is_http_redirect(response.status()) {
                break response;
            }
            if redirects >= MAX_NATIVE_NETWORK_REDIRECTS {
                return Err(NativeEngineError::Network {
                    operation: "EventSource redirect".into(),
                    reason: "EventSource redirect chain exceeded the native limit".into(),
                });
            }
            let location = response
                .headers()
                .get(reqwest::header::LOCATION)
                .and_then(|value| value.to_str().ok())
                .filter(|value| !value.is_empty())
                .ok_or_else(|| NativeEngineError::Network {
                    operation: "EventSource redirect".into(),
                    reason: "EventSource redirect did not provide a valid location".into(),
                })?;
            let next_url = current_url
                .join(location)
                .map_err(|_| NativeEngineError::Network {
                    operation: "EventSource redirect".into(),
                    reason: "EventSource redirect location is not valid URL syntax".into(),
                })?;
            reject_credentials(&next_url)?;
            if !is_network_url(without_fragment(next_url.as_str()))
                || !mixed_content_allowed(&document_url, &next_url)
                || {
                    self.record_report_only_url_violations_for_redirect(
                        &policy,
                        NativeSubresourceKind::Connect,
                        &document_url,
                        &next_url,
                        true,
                        None,
                    );
                    !policy.allows_redirect(
                        NativeSubresourceKind::Connect,
                        &document_url,
                        &next_url,
                    )
                }
            {
                return Err(NativeEngineError::Network {
                    operation: "EventSource redirect policy".into(),
                    reason: "EventSource redirect was blocked by URL, CSP, or mixed content".into(),
                });
            }
            request_referrer = normalize_referrer(Some(current_url.as_str()), &next_url)?;
            current_url = next_url;
            redirects += 1;
        };
        if response.status() != reqwest::StatusCode::OK {
            return Err(NativeEngineError::Network {
                operation: "EventSource request".into(),
                reason: format!("server returned HTTP {}", response.status().as_u16()),
            });
        }
        if !content_type_is(
            response.headers().get(reqwest::header::CONTENT_TYPE),
            "text/event-stream",
        )? {
            return Err(NativeEngineError::Network {
                operation: "EventSource response".into(),
                reason: "response content type is not text/event-stream".into(),
            });
        }
        if !cors_response_allowed(
            response.headers(),
            &document_url,
            &current_url,
            with_credentials,
        ) {
            return Err(NativeEngineError::Network {
                operation: "EventSource CORS policy".into(),
                reason: "EventSource response did not authorize the document origin".into(),
            });
        }
        for value in response
            .headers()
            .get_all(reqwest::header::SET_COOKIE)
            .iter()
        {
            if let Ok(cookie) = value.to_str() {
                self.cookie_changes
                    .extend(self.network.store_cookie(&current_url, cookie));
            }
        }
        Ok((current_url, response))
    }

    pub(crate) fn frame_sources_for_document(
        &self,
        document_url: &str,
    ) -> Result<Option<Vec<Vec<String>>>, NativeEngineError> {
        validate_url_text("frame policy owner URL", document_url)?;
        let document_url = Url::parse(without_fragment(document_url)).map_err(|_| {
            NativeEngineError::UnsupportedUrl {
                reason: "frame policy owner URL is not valid URL syntax".into(),
            }
        })?;
        if !is_network_url(document_url.as_str()) {
            return Ok(None);
        }
        reject_credentials(&document_url)?;
        Ok(self
            .network
            .document_policies
            .get(&cache_key(&document_url))
            .and_then(NativeCspPolicy::frame_source_groups))
    }

    /// Check the live CSP policy container for a frame requested by a
    /// configured-root file document. Unlike the network frame snapshot, this
    /// reads the current policy ledger so same-turn meta insertions are seen
    /// before the child document is initialized.
    pub(crate) fn allows_rooted_file_frame_navigation(
        &self,
        document_url: &str,
        target_url: &str,
    ) -> Result<bool, NativeEngineError> {
        validate_url_text("frame policy owner URL", document_url)?;
        validate_url_text("embedded frame URL", target_url)?;
        if without_fragment(target_url) == "about:blank" {
            return Ok(true);
        }
        let Some((document_url, Some(_))) =
            self.csp_document_owner(document_url, "frame policy owner URL")?
        else {
            return Ok(true);
        };
        let target_url = Url::parse(without_fragment(target_url)).map_err(|_| {
            NativeEngineError::UnsupportedUrl {
                reason: "embedded frame URL is not valid URL syntax".into(),
            }
        })?;
        reject_credentials(&target_url)?;
        let Some(policy) = self
            .network
            .document_policies
            .get(&cache_key(&document_url))
        else {
            return Ok(true);
        };

        let resource_root = if is_file_url(target_url.as_str()) {
            let path = match self.allowed_file_path(&target_url, "file frame") {
                Ok(path) => path,
                Err(NativeEngineError::UnsupportedUrl { .. }) => return Ok(false),
                Err(error) => return Err(error),
            };
            let Some(root) = self.allowed_file_root_for_path(&path) else {
                return Ok(false);
            };
            Some(root.to_path_buf())
        } else {
            None
        };

        Ok(policy.allows_rooted_file_resource(
            NativeSubresourceKind::Frame,
            &document_url,
            &target_url,
            resource_root.as_deref(),
        ))
    }

    pub(crate) fn navigation_sources_for_document(
        &self,
        document_url: &str,
        kind: NativeNavigationPolicyKind,
    ) -> Result<Option<Vec<Vec<String>>>, NativeEngineError> {
        validate_url_text("navigation policy owner URL", document_url)?;
        let document_url = Url::parse(without_fragment(document_url)).map_err(|_| {
            NativeEngineError::UnsupportedUrl {
                reason: "navigation policy owner URL is not valid URL syntax".into(),
            }
        })?;
        if !is_network_url(document_url.as_str()) {
            return Ok(None);
        }
        reject_credentials(&document_url)?;
        Ok(self
            .network
            .document_policies
            .get(&cache_key(&document_url))
            .and_then(|policy| policy.navigation_source_groups(kind)))
    }

    pub(crate) fn apply_meta_content_security_policies(
        &mut self,
        document_url: &str,
        policies: &[String],
    ) -> Result<(), NativeEngineError> {
        let Some((document_url, file_root)) =
            self.csp_document_owner(document_url, "CSP meta policy owner URL")?
        else {
            return Ok(());
        };
        if policies.len() > MAX_NATIVE_CSP_POLICIES {
            return Err(NativeEngineError::limit(
                "CSP meta policies",
                MAX_NATIVE_CSP_POLICIES,
                policies.len(),
            ));
        }
        let key = cache_key(&document_url);
        let policy = self.network.document_policies.entry(key).or_default();
        policy.rooted_file_self_root = file_root;
        policy.replace_meta_policies(policies)
    }

    pub(crate) fn append_meta_content_security_policies(
        &mut self,
        document_url: &str,
        policies: &[String],
    ) -> Result<(), NativeEngineError> {
        if policies.is_empty() {
            return Ok(());
        }
        let Some((document_url, file_root)) =
            self.csp_document_owner(document_url, "CSP meta policy owner URL")?
        else {
            return Ok(());
        };
        if policies.len() > MAX_NATIVE_CSP_POLICIES {
            return Err(NativeEngineError::limit(
                "CSP meta policies",
                MAX_NATIVE_CSP_POLICIES,
                policies.len(),
            ));
        }
        let key = cache_key(&document_url);
        let policy = self.network.document_policies.entry(key).or_default();
        policy.rooted_file_self_root = file_root;
        policy.append_meta_policies(policies)
    }

    pub(crate) fn document_meta_content_security_policies(
        &self,
        document_url: &str,
    ) -> Result<Vec<String>, NativeEngineError> {
        let Some((document_url, _)) =
            self.csp_document_owner(document_url, "CSP meta policy owner URL")?
        else {
            return Ok(Vec::new());
        };
        Ok(self
            .network
            .document_policies
            .get(&cache_key(&document_url))
            .map(|policy| policy.meta_policy_sources.clone())
            .unwrap_or_default())
    }

    pub(crate) fn set_document_content_security_policy_from_pairs(
        &mut self,
        document_url: &str,
        headers: &[(String, String)],
    ) -> Result<(), NativeEngineError> {
        validate_url_text("CSP response policy owner URL", document_url)?;
        let document_url = Url::parse(without_fragment(document_url)).map_err(|_| {
            NativeEngineError::UnsupportedUrl {
                reason: "CSP response policy owner URL is not valid URL syntax".into(),
            }
        })?;
        if !is_network_url(document_url.as_str()) {
            return Ok(());
        }
        reject_credentials(&document_url)?;
        let mut policy = NativeCspPolicy::default();
        for (name, value) in headers {
            if value.len() > MAX_NATIVE_RESPONSE_HEADER_VALUE_BYTES {
                return Err(NativeEngineError::limit(
                    "CSP response policy",
                    MAX_NATIVE_RESPONSE_HEADER_VALUE_BYTES,
                    value.len(),
                ));
            }
            if name.eq_ignore_ascii_case("content-security-policy") {
                policy.policies.push(parse_csp_directives(value));
            } else if name.eq_ignore_ascii_case("content-security-policy-report-only") {
                policy.report_only_policies.push(NativeCspDeclaration {
                    directives: parse_csp_directives(value),
                    original_policy: value.clone(),
                });
            } else if name.eq_ignore_ascii_case("reporting-endpoints")
                || name.eq_ignore_ascii_case("report-to")
            {
                add_reporting_endpoint_header(&mut policy, name, value);
            }
        }
        policy.header_policy_count = policy.policies.len();
        let document_key = cache_key(&document_url);
        self.network
            .store_document_policy(document_key.clone(), policy);
        self.network.store_document_referrer_policy(
            document_key,
            referrer_policy_from_header_pairs(headers),
        );
        Ok(())
    }

    pub(crate) fn document_referrer_policy(
        &self,
        document_url: &str,
    ) -> Result<NativeFetchReferrerPolicy, NativeEngineError> {
        let document_url =
            Url::parse(document_url).map_err(|_| NativeEngineError::UnsupportedUrl {
                reason: "Document referrer policy owner URL is not valid URL syntax".into(),
            })?;
        reject_credentials(&document_url)?;
        Ok(self
            .network
            .document_referrer_policies
            .get(&cache_key(&document_url))
            .copied()
            .unwrap_or(NativeFetchReferrerPolicy::StrictOriginWhenCrossOrigin))
    }

    pub(crate) fn document_response_policy_headers(
        &self,
        document_url: &str,
    ) -> Result<Vec<(String, String)>, NativeEngineError> {
        let document_url = Url::parse(without_fragment(document_url)).map_err(|_| {
            NativeEngineError::UnsupportedUrl {
                reason: "Document response policy owner URL is not valid URL syntax".into(),
            }
        })?;
        reject_credentials(&document_url)?;
        Ok(self
            .network
            .document_response_policy_headers
            .get(&cache_key(&document_url))
            .cloned()
            .unwrap_or_default())
    }

    pub(crate) fn enforce_service_worker_connect_policy(
        &self,
        document_url: &Url,
        target_url: &Url,
    ) -> Result<(), NativeEngineError> {
        if !mixed_content_allowed(document_url, target_url) {
            return Err(NativeEngineError::Network {
                operation: "service worker fetch policy".into(),
                reason: "HTTPS documents cannot fetch HTTP resources".into(),
            });
        }
        let policy = self
            .network
            .document_policies
            .get(&cache_key(document_url))
            .cloned()
            .unwrap_or_default();
        if !policy.allows(NativeSubresourceKind::Connect, document_url, target_url) {
            return Err(NativeEngineError::Network {
                operation: "service worker fetch policy".into(),
                reason: "document CSP blocked the connect target".into(),
            });
        }
        Ok(())
    }

    pub(crate) fn enforce_service_worker_font_policy(
        &self,
        document_url: &Url,
        target_url: &Url,
    ) -> Result<(), NativeEngineError> {
        if !mixed_content_allowed(document_url, target_url) {
            return Err(NativeEngineError::Network {
                operation: "service worker font policy".into(),
                reason: "HTTPS documents cannot fetch HTTP resources".into(),
            });
        }
        let policy = self
            .network
            .document_policies
            .get(&cache_key(document_url))
            .cloned()
            .unwrap_or_default();
        if !policy.allows(NativeSubresourceKind::Font, document_url, target_url) {
            return Err(NativeEngineError::Network {
                operation: "service worker font policy".into(),
                reason: "document CSP blocked the font target".into(),
            });
        }
        Ok(())
    }

    pub(crate) fn allows_navigation(
        &mut self,
        document_url: &str,
        target_url: &str,
        kind: NativeNavigationPolicyKind,
    ) -> Result<bool, NativeEngineError> {
        let (document_url, target_url, policy) =
            self.navigation_policy(document_url, target_url)?;
        self.record_report_only_navigation_violations(&policy, kind, &document_url, &target_url);
        Ok(policy.allows_navigation(kind, &document_url, &target_url))
    }

    pub(crate) fn allows_navigation_silent(
        &self,
        document_url: &str,
        target_url: &str,
        kind: NativeNavigationPolicyKind,
    ) -> Result<bool, NativeEngineError> {
        let (document_url, target_url, policy) =
            self.navigation_policy(document_url, target_url)?;
        Ok(policy.allows_navigation(kind, &document_url, &target_url))
    }

    fn navigation_policy(
        &self,
        document_url: &str,
        target_url: &str,
    ) -> Result<(Url, Url, NativeCspPolicy), NativeEngineError> {
        validate_url_text("CSP navigation owner URL", document_url)?;
        validate_url_text("CSP navigation target URL", target_url)?;
        let document_url = Url::parse(without_fragment(document_url)).map_err(|_| {
            NativeEngineError::UnsupportedUrl {
                reason: "CSP navigation owner URL is not valid URL syntax".into(),
            }
        })?;
        let target_url = Url::parse(without_fragment(target_url)).map_err(|_| {
            NativeEngineError::UnsupportedUrl {
                reason: "CSP navigation target URL is not valid URL syntax".into(),
            }
        })?;
        reject_credentials(&document_url)?;
        reject_credentials(&target_url)?;
        if !is_network_url(document_url.as_str()) {
            return Ok((document_url, target_url, NativeCspPolicy::default()));
        }
        let policy = self
            .network
            .document_policies
            .get(&cache_key(&document_url))
            .cloned()
            .unwrap_or_default();
        Ok((document_url, target_url, policy))
    }

    pub(crate) fn report_service_worker_connect_policy(
        &mut self,
        document_url: &Url,
        target_url: &Url,
    ) {
        let policy = self
            .network
            .document_policies
            .get(&cache_key(document_url))
            .cloned()
            .unwrap_or_default();
        self.record_report_only_url_violations(
            &policy,
            NativeSubresourceKind::Connect,
            document_url,
            target_url,
        );
    }

    pub(crate) fn report_service_worker_font_policy(
        &mut self,
        document_url: &Url,
        target_url: &Url,
    ) {
        let policy = self
            .network
            .document_policies
            .get(&cache_key(document_url))
            .cloned()
            .unwrap_or_default();
        self.record_report_only_url_violations(
            &policy,
            NativeSubresourceKind::Font,
            document_url,
            target_url,
        );
    }

    pub(crate) fn cookie_profile(&self) -> Vec<NativeCookieProfileEntry> {
        self.network.cookie_profile()
    }

    pub(crate) fn cookies_for_document(
        &self,
        document_url: &str,
    ) -> Result<Vec<NativeCookieProfileEntry>, NativeEngineError> {
        validate_url_text("cookie owner URL", document_url)?;
        let document_url = Url::parse(without_fragment(document_url)).map_err(|_| {
            NativeEngineError::UnsupportedUrl {
                reason: "cookie owner URL is not valid URL syntax".into(),
            }
        })?;
        if !is_network_url(document_url.as_str()) {
            return Ok(Vec::new());
        }
        reject_credentials(&document_url)?;
        Ok(self
            .network
            .matching_cookies(&document_url, true)
            .into_iter()
            .filter_map(NativeCookie::to_profile)
            .collect())
    }

    pub(crate) fn set_cookie_profiles(
        &mut self,
        profiles: &[NativeCookieProfileEntry],
    ) -> Result<(), NativeEngineError> {
        if !self.network.cookie_authority_enabled {
            return Err(NativeEngineError::Worker {
                operation: "install content-process cookies".into(),
                reason: "cookie profiles are owned by the browser process".into(),
            });
        }
        if profiles.len() > MAX_NATIVE_COOKIE_PROFILE_ENTRIES {
            return Err(NativeEngineError::limit(
                "native cookie profile entries",
                MAX_NATIVE_COOKIE_PROFILE_ENTRIES,
                profiles.len(),
            ));
        }
        for profile in profiles {
            let Some(cookie) = NativeCookie::from_profile(profile.clone())? else {
                if self
                    .network
                    .remove_cookie(&profile.name, &profile.domain, &profile.path)
                {
                    self.cookie_changes.push(NativeCookieChange {
                        name: profile.name.clone(),
                        domain: profile.domain.clone(),
                        path: profile.path.clone(),
                        cookie: None,
                    });
                }
                continue;
            };
            if let Some(evicted) = self.network.set_cookie_profile(cookie)? {
                self.cookie_changes.push(NativeCookieChange {
                    name: evicted.name,
                    domain: evicted.domain,
                    path: evicted.path,
                    cookie: None,
                });
            }
            self.cookie_changes.push(NativeCookieChange {
                name: profile.name.clone(),
                domain: profile.domain.clone(),
                path: profile.path.clone(),
                cookie: Some(profile.clone()),
            });
        }
        Ok(())
    }

    pub(crate) fn replace_cookie_profiles(
        &mut self,
        profiles: &[NativeCookieProfileEntry],
    ) -> Result<(), NativeEngineError> {
        if !self.network.cookie_authority_enabled {
            return Err(NativeEngineError::Worker {
                operation: "replace content-process cookies".into(),
                reason: "cookie profiles are owned by the browser process".into(),
            });
        }
        if profiles.len() > MAX_NATIVE_COOKIE_PROFILE_ENTRIES {
            return Err(NativeEngineError::limit(
                "native cookie profile entries",
                MAX_NATIVE_COOKIE_PROFILE_ENTRIES,
                profiles.len(),
            ));
        }
        self.network.cookies = NativeNetworkState::from_profile(profiles.to_vec())?.cookies;
        self.cookie_changes.clear();
        Ok(())
    }

    pub(crate) fn clear_cookies(&mut self) {
        self.network.clear_cookies(&mut self.cookie_changes);
    }

    pub(crate) fn take_cookie_changes(&mut self) -> Vec<NativeCookieChange> {
        std::mem::take(&mut self.cookie_changes)
    }

    pub(crate) fn restore_cookie_changes(&mut self, changes: Vec<NativeCookieChange>) {
        self.cookie_changes.extend(changes);
    }

    pub(crate) fn apply_cookie_changes(
        &mut self,
        changes: &[NativeCookieChange],
    ) -> Result<(), NativeEngineError> {
        if !self.network.cookie_authority_enabled {
            return Err(NativeEngineError::Worker {
                operation: "apply content-process cookie changes".into(),
                reason: "cookie changes must be applied by the browser process".into(),
            });
        }
        for change in changes {
            match &change.cookie {
                Some(profile) => {
                    let Some(cookie) = NativeCookie::from_profile(profile.clone())? else {
                        self.network
                            .remove_cookie(&change.name, &change.domain, &change.path);
                        self.cookie_changes.push(change.clone());
                        continue;
                    };
                    self.network.set_cookie_profile(cookie)?;
                }
                None => {
                    self.network
                        .remove_cookie(&change.name, &change.domain, &change.path);
                }
            }
            self.cookie_changes.push(change.clone());
        }
        Ok(())
    }

    pub(crate) fn document_cookie(&self, document_url: &str) -> Result<String, NativeEngineError> {
        validate_url_text("document URL", document_url)?;
        let document_url = Url::parse(without_fragment(document_url)).map_err(|_| {
            NativeEngineError::UnsupportedUrl {
                reason: "document.cookie owner URL is not valid URL syntax".into(),
            }
        })?;
        if !is_network_url(document_url.as_str()) {
            return Ok(String::new());
        }
        reject_credentials(&document_url)?;
        Ok(self
            .network
            .document_cookie_header(&document_url)
            .unwrap_or_default())
    }

    pub(crate) fn set_document_cookie(
        &mut self,
        document_url: &str,
        value: &str,
    ) -> Result<(), NativeEngineError> {
        validate_url_text("document URL", document_url)?;
        validate_url_text("document.cookie value", value)?;
        let document_url = Url::parse(without_fragment(document_url)).map_err(|_| {
            NativeEngineError::UnsupportedUrl {
                reason: "document.cookie owner URL is not valid URL syntax".into(),
            }
        })?;
        if !is_network_url(document_url.as_str()) {
            return Ok(());
        }
        reject_credentials(&document_url)?;
        if value
            .split(';')
            .skip(1)
            .any(|attribute| attribute.trim().eq_ignore_ascii_case("httponly"))
        {
            return Ok(());
        }
        self.cookie_changes
            .extend(self.network.store_cookie(&document_url, value));
        Ok(())
    }

    /// Load one supported local resource without network access.
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
        if is_file_url(resource_url) {
            return self.load_file_document(url);
        }
        Err(NativeEngineError::UnsupportedUrl {
            reason: "HTTP(S) loading requires asynchronous native navigation; other URL schemes are unsupported".into(),
        })
    }

    fn load_file_document(&self, url: &str) -> Result<NativeResource, NativeEngineError> {
        let parsed =
            Url::parse(without_fragment(url)).map_err(|_| NativeEngineError::UnsupportedUrl {
                reason: "file document URL is not valid URL syntax".into(),
            })?;
        reject_credentials(&parsed)?;
        let path = self.allowed_file_path(&parsed, "file document")?;
        let bytes = read_bounded_file(&path, self.max_document_bytes, "file document")?;
        let body = decode_html_body(&bytes, None, self.max_document_bytes)?;
        Ok(NativeResource {
            url: url.into(),
            origin: NativeOrigin::from_url(&parsed)?,
            body,
        })
    }

    /// Load one bounded HTML document over HTTP(S) without a referrer.
    ///
    /// This is deliberately a document-only network slice: it follows a
    /// bounded redirect chain, rejects credentials and non-HTML responses,
    /// enforces the configured document limit while streaming, and does not
    /// fetch subresources or execute page code.
    pub async fn load_async(&mut self, url: &str) -> Result<NativeResource, NativeEngineError> {
        self.load_async_with_referrer(url, None).await
    }

    pub(crate) async fn load_async_with_referrer(
        &mut self,
        url: &str,
        referrer: Option<&str>,
    ) -> Result<NativeResource, NativeEngineError> {
        self.load_async_request_with_referrer(&NativeNavigationRequest::get(url), referrer)
            .await
    }

    pub(crate) async fn load_async_request_with_referrer(
        &mut self,
        navigation: &NativeNavigationRequest,
        referrer: Option<&str>,
    ) -> Result<NativeResource, NativeEngineError> {
        self.load_async_request_with_referrer_policy(
            navigation,
            referrer,
            NativeFetchReferrerPolicy::StrictOriginWhenCrossOrigin,
        )
        .await
    }

    pub(crate) async fn load_async_request_with_referrer_policy(
        &mut self,
        navigation: &NativeNavigationRequest,
        referrer: Option<&str>,
        referrer_policy: NativeFetchReferrerPolicy,
    ) -> Result<NativeResource, NativeEngineError> {
        if navigation.object_url.is_some() {
            return self.load_object_url_request(navigation);
        }
        let request_method = navigation.method;
        if !request_method.is_document_method() {
            return Err(NativeEngineError::UnsupportedUrl {
                reason: format!(
                    "HTTP(S) document navigation does not support {} requests",
                    request_method.as_str()
                ),
            });
        }
        let request_body = match request_method {
            NativeNavigationMethod::Get => {
                if navigation.body.is_some() {
                    return Err(NativeEngineError::invalid(
                        "GET navigation body",
                        "must be absent",
                    ));
                }
                if navigation.body_content_type.is_some() {
                    return Err(NativeEngineError::invalid(
                        "GET navigation content type",
                        "must be absent",
                    ));
                }
                None
            }
            NativeNavigationMethod::Post => {
                let body = navigation.body.clone().ok_or_else(|| {
                    NativeEngineError::invalid(
                        "POST navigation body",
                        "must be present for a form submission",
                    )
                })?;
                if body.len() > MAX_NATIVE_FORM_BODY_BYTES {
                    return Err(NativeEngineError::limit(
                        "form submission body",
                        MAX_NATIVE_FORM_BODY_BYTES,
                        body.len(),
                    ));
                }
                Some(body)
            }
            _ => unreachable!("document method was validated above"),
        };
        let request_content_type = match request_method {
            NativeNavigationMethod::Get => None,
            NativeNavigationMethod::Post => Some(
                navigation
                    .body_content_type
                    .as_deref()
                    .unwrap_or("application/x-www-form-urlencoded")
                    .to_owned(),
            ),
            _ => unreachable!("document method was validated above"),
        };
        let url = navigation.url.as_str();
        validate_url_text("navigation URL", url)?;
        let resource_url = without_fragment(url);
        if !is_network_url(resource_url) {
            return self.load(url);
        }
        let parsed = Url::parse(resource_url).map_err(|_| NativeEngineError::UnsupportedUrl {
            reason: "HTTP(S) navigation URL is not valid URL syntax".into(),
        })?;
        reject_credentials(&parsed)?;
        let referrer_source = parse_navigation_referrer_source(referrer)?;
        let navigation_initiator = referrer_source.as_ref();
        let mut effective_referrer_policy = referrer_policy;
        let mut request_referrer =
            fetch_referrer_for_target(referrer_source.as_ref(), &parsed, effective_referrer_policy);
        let document_cache_key = cache_key(&parsed);
        let stale_cached_document = if request_method == NativeNavigationMethod::Get {
            self.network
                .cache
                .get(&document_cache_key)
                .cloned()
                .filter(|cached| !cached.is_fresh(Instant::now()))
        } else {
            None
        };
        if request_method == NativeNavigationMethod::Get
            && let Some(cached) = self.network.cache.get(&document_cache_key)
            && cached.is_fresh(Instant::now())
        {
            return with_original_fragment(cached.resource.clone(), url);
        }
        let client = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(NATIVE_NETWORK_TIMEOUT)
            .build()
            .map_err(|error| network_error("HTTP client construction", error))?;
        let mut current_url = parsed.clone();
        let mut current_method = request_method;
        let mut current_body = request_body;
        let mut current_content_type = request_content_type;
        let mut redirects = 0;
        let mut pending_cookies = Vec::new();
        let response = loop {
            let mut request_url = current_url.clone();
            request_url.set_fragment(None);
            let mut request = self.apply_environment_headers(
                client
                    .request(current_method.reqwest_method(), request_url)
                    .header(reqwest::header::ACCEPT, "text/html,application/xhtml+xml"),
            );
            if let Some(body) = current_body.clone() {
                request = match body {
                    NativeRequestBody::Text(body) => request.body(body),
                    NativeRequestBody::Bytes(body) => request.body(body),
                };
            }
            if let Some(content_type) = current_content_type.as_deref() {
                request = request.header(reqwest::header::CONTENT_TYPE, content_type);
            }
            if let Some(referrer) = request_referrer.as_deref() {
                request = request.header(reqwest::header::REFERER, referrer);
            }
            if redirects == 0
                && let Some(cached) = stale_cached_document.as_ref()
            {
                if let Some(etag) = cached.etag.as_deref() {
                    request = request.header(reqwest::header::IF_NONE_MATCH, etag);
                }
                if let Some(last_modified) = cached.last_modified.as_deref() {
                    request = request.header(reqwest::header::IF_MODIFIED_SINCE, last_modified);
                }
            }
            if let Some(cookie) = self.network.cookie_header_for_request(
                &current_url,
                navigation_initiator,
                true,
                current_method,
            ) {
                request = request.header(reqwest::header::COOKIE, cookie);
            }
            self.before_request(current_body.as_ref().map_or(0, NativeRequestBody::len))
                .await?;
            let response = request
                .send()
                .await
                .map_err(|error| network_error("HTTP document request", error))?;
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
                return Err(NativeEngineError::Network {
                    operation: "HTTP redirect".into(),
                    reason: "HTTP(S) redirect chain exceeded the native limit".into(),
                });
            }
            let location = response
                .headers()
                .get(reqwest::header::LOCATION)
                .and_then(|value| value.to_str().ok())
                .filter(|value| !value.is_empty())
                .ok_or_else(|| NativeEngineError::Network {
                    operation: "HTTP redirect".into(),
                    reason: "HTTP(S) redirect did not provide a valid location".into(),
                })?;
            let next_url = current_url
                .join(location)
                .map_err(|_| NativeEngineError::Network {
                    operation: "HTTP redirect".into(),
                    reason: "HTTP(S) redirect location is not valid URL syntax".into(),
                })?;
            reject_credentials(&next_url)?;
            if !is_network_url(without_fragment(next_url.as_str())) {
                return Err(NativeEngineError::UnsupportedUrl {
                    reason: "HTTP(S) navigation redirected to a non-HTTP(S) URL".into(),
                });
            }
            if let Some(policy) = referrer_policy_from_headers(response.headers()) {
                effective_referrer_policy = policy;
            }
            request_referrer = fetch_referrer_for_target(
                referrer_source.as_ref(),
                &next_url,
                effective_referrer_policy,
            );
            if matches!(
                response.status(),
                reqwest::StatusCode::MOVED_PERMANENTLY
                    | reqwest::StatusCode::FOUND
                    | reqwest::StatusCode::SEE_OTHER
            ) && current_method == NativeNavigationMethod::Post
            {
                current_method = NativeNavigationMethod::Get;
                current_body = None;
                current_content_type = None;
            }
            current_url = next_url;
            redirects += 1;
        };
        let response_headers = response.headers().clone();
        let has_set_cookie = !pending_cookies.is_empty();
        if response.status() == reqwest::StatusCode::NOT_MODIFIED {
            let Some(cached) = stale_cached_document else {
                return Err(NativeEngineError::Network {
                    operation: "HTTP document revalidation".into(),
                    reason: "server returned HTTP 304 without a stale cached document".into(),
                });
            };
            let cached_resource = cached.resource.clone();
            if request_method != NativeNavigationMethod::Get || redirects != 0 {
                return Err(NativeEngineError::Network {
                    operation: "HTTP document revalidation".into(),
                    reason: "HTTP 304 was received after an unsupported navigation transition"
                        .into(),
                });
            }
            for (cookie_url, cookie) in pending_cookies {
                self.cookie_changes
                    .extend(self.network.store_cookie(&cookie_url, &cookie));
            }
            let policy_header_updates = document_response_policy_headers(&response_headers)?;
            if !policy_header_updates.is_empty() {
                let mut headers = self
                    .network
                    .document_response_policy_headers
                    .remove(&document_cache_key)
                    .unwrap_or_default();
                merge_document_response_policy_headers(&mut headers, policy_header_updates);
                self.network
                    .store_document_response_policy_headers(document_cache_key.clone(), headers);
            }
            if response_headers.contains_key("referrer-policy") {
                self.network.store_document_referrer_policy(
                    document_cache_key.clone(),
                    referrer_policy_from_headers(&response_headers),
                );
            }
            if has_set_cookie {
                self.network.remove_cache(&document_cache_key);
            } else if let Some(entry) =
                cached.refresh_from_not_modified(&response_headers, Instant::now())
            {
                self.network.store_document_cache(document_cache_key, entry);
            } else {
                self.network.remove_cache(&document_cache_key);
            }
            return with_original_fragment(cached_resource, url);
        }
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
        let final_url = current_url;
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
            self.after_response_chunk(chunk.len()).await;
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
        let navigation_url = append_original_fragment(&final_url, url);
        let origin =
            NativeOrigin::from_url(&Url::parse(without_fragment(&navigation_url)).map_err(
                |_| NativeEngineError::UnsupportedUrl {
                    reason: "HTTP(S) navigation produced invalid final URL syntax".into(),
                },
            )?)?;
        let resource = NativeResource {
            url: navigation_url,
            origin,
            body,
        };
        let cache_resource = NativeResource {
            url: without_fragment(&resource.url).to_owned(),
            origin: resource.origin.clone(),
            body: resource.body.clone(),
        };
        for (cookie_url, cookie) in pending_cookies {
            self.cookie_changes
                .extend(self.network.store_cookie(&cookie_url, &cookie));
        }
        self.network.store_document_policy(
            cache_key(&final_url),
            content_security_policy(&response_headers),
        );
        self.network.store_document_response_policy_headers(
            cache_key(&final_url),
            document_response_policy_headers(&response_headers)?,
        );
        self.network.store_document_referrer_policy(
            cache_key(&final_url),
            referrer_policy_from_headers(&response_headers),
        );
        if request_method == NativeNavigationMethod::Get {
            if !has_set_cookie
                && let Some(entry) = NativeDocumentCacheEntry::from_response(
                    cache_resource,
                    &response_headers,
                    Instant::now(),
                )
            {
                self.network.store_document_cache(document_cache_key, entry);
            } else {
                self.network.remove_cache(&document_cache_key);
            }
        }
        Ok(resource)
    }

    pub(crate) fn load_navigation(
        &self,
        navigation: &NativeNavigationRequest,
    ) -> Result<NativeResource, NativeEngineError> {
        if navigation.object_url.is_some() {
            return self.load_object_url_request(navigation);
        }
        self.load(&navigation.url)
    }

    fn load_object_url_request(
        &self,
        navigation: &NativeNavigationRequest,
    ) -> Result<NativeResource, NativeEngineError> {
        if navigation.method != NativeNavigationMethod::Get {
            return Err(NativeEngineError::UnsupportedUrl {
                reason: "native Blob document navigation supports only GET".into(),
            });
        }
        if navigation.body.is_some() || navigation.body_content_type.is_some() {
            return Err(NativeEngineError::invalid(
                "native Blob document navigation",
                "must not carry a request body or content type",
            ));
        }
        let Some(object_url) = navigation.object_url.as_ref() else {
            unreachable!("object URL navigation payload was checked above")
        };
        if object_url.body.len() > self.max_document_bytes {
            return Err(NativeEngineError::limit(
                "Blob document",
                self.max_document_bytes,
                object_url.body.len(),
            ));
        }
        if object_url
            .content_type
            .as_ref()
            .is_some_and(|content_type| content_type.len() > MAX_NATIVE_SCRIPT_BYTES)
        {
            return Err(NativeEngineError::limit(
                "Blob document content type",
                MAX_NATIVE_SCRIPT_BYTES,
                object_url.content_type.as_ref().map_or(0, String::len),
            ));
        }
        let resource_url = without_fragment(&navigation.url);
        let origin = NativeOrigin::from_blob_url(resource_url)?;
        let body = String::from_utf8(object_url.body.clone()).map_err(|_| {
            NativeEngineError::UnsupportedUrl {
                reason: "native Blob document must contain valid UTF-8 HTML".into(),
            }
        })?;
        Ok(NativeResource {
            url: navigation.url.clone(),
            origin,
            body,
        })
    }

    pub(crate) async fn fetch_async(
        &mut self,
        document_url: &str,
        href: &str,
        credentials: bool,
    ) -> Result<NativeFetchResponse, NativeEngineError> {
        self.fetch_request_async(
            document_url,
            href,
            NativeNavigationMethod::Get,
            None,
            None,
            credentials,
        )
        .await
    }

    pub(crate) async fn fetch_request_async(
        &mut self,
        document_url: &str,
        href: &str,
        method: NativeNavigationMethod,
        body: Option<String>,
        content_type: Option<String>,
        credentials: bool,
    ) -> Result<NativeFetchResponse, NativeEngineError> {
        self.fetch_request_with_headers_async(NativeFetchRequest {
            document_url,
            href,
            referrer_url: None,
            referrer_policy: None,
            method: NativeFetchMethod::from_navigation_method(method),
            body: body.map(NativeRequestBody::Text),
            content_type,
            request_headers: BTreeMap::new(),
            credentials,
            credentials_mode: None,
            cors_mode: NativeCorsMode::Cors,
            redirect_mode: NativeFetchRedirectMode::Follow,
            cache_mode: NativeFetchCacheMode::Default,
            timeout: None,
            max_response_bytes: None,
        })
        .await
    }

    /// Fetch one browser download response through the navigation policy.
    /// Unlike script `no-cors`, a download must retain cross-origin bytes for
    /// the parent-owned file writer; the response is never exposed to page
    /// JavaScript.
    pub(crate) async fn download_async(
        &mut self,
        document_url: &str,
        href: &str,
    ) -> Result<NativeFetchResponse, NativeEngineError> {
        validate_url_text("download owner URL", document_url)?;
        validate_url_text("download URL", href)?;
        let owner_url = Url::parse(without_fragment(document_url)).map_err(|_| {
            NativeEngineError::UnsupportedUrl {
                reason: "download owner URL is not valid URL syntax".into(),
            }
        })?;
        if owner_url.scheme().eq_ignore_ascii_case("file") {
            let Some((target_url, path)) =
                self.local_file_subresource_path(document_url, href, "file download")?
            else {
                return Err(NativeEngineError::UnsupportedUrl {
                    reason: "file download URL must use the file scheme".into(),
                });
            };
            let body = read_bounded_file(&path, MAX_NATIVE_DOWNLOAD_BYTES, "file download")?;
            return Ok(NativeFetchResponse {
                url: without_fragment(target_url.as_str()).to_owned(),
                status: 200,
                status_text: "OK".into(),
                content_type: file_media_type(&path).map(str::to_owned),
                headers: Vec::new(),
                body,
                redirected: false,
                opaque: false,
                opaque_redirect: false,
            });
        }
        self.fetch_request_with_headers_async(NativeFetchRequest {
            document_url,
            href,
            referrer_url: None,
            referrer_policy: None,
            method: NativeFetchMethod::get(),
            body: None,
            content_type: None,
            request_headers: BTreeMap::new(),
            credentials: true,
            credentials_mode: None,
            cors_mode: NativeCorsMode::Navigation,
            redirect_mode: NativeFetchRedirectMode::Follow,
            cache_mode: NativeFetchCacheMode::NoStore,
            timeout: None,
            max_response_bytes: Some(MAX_NATIVE_DOWNLOAD_BYTES),
        })
        .await
    }

    pub(crate) async fn fetch_request_with_headers_async(
        &mut self,
        request: NativeFetchRequest<'_>,
    ) -> Result<NativeFetchResponse, NativeEngineError> {
        self.fetch_request_with_navigation_context_async(request, false)
            .await
    }

    pub(crate) async fn fetch_navigation_preload_async(
        &mut self,
        target_url: &str,
        referrer: Option<&str>,
        referrer_policy: NativeFetchReferrerPolicy,
        header_value: &str,
    ) -> Result<NativeFetchResponse, NativeEngineError> {
        validate_navigation_preload_header_value(header_value)?;
        let referrer = referrer.filter(|value| !value.is_empty());
        let owner_url = referrer.unwrap_or(target_url);
        let referrer_url = referrer.map(str::to_owned).unwrap_or_default();
        let request = NativeFetchRequest {
            document_url: owner_url,
            href: target_url,
            referrer_url: Some(referrer_url),
            referrer_policy: Some(referrer_policy.as_str().to_owned()),
            method: NativeFetchMethod::get(),
            body: None,
            content_type: None,
            request_headers: BTreeMap::from([
                (
                    "accept".to_owned(),
                    "text/html,application/xhtml+xml".to_owned(),
                ),
                (
                    "service-worker-navigation-preload".to_owned(),
                    header_value.to_owned(),
                ),
            ]),
            credentials: true,
            credentials_mode: Some(NativeFetchCredentialsMode::Include),
            cors_mode: NativeCorsMode::Navigation,
            redirect_mode: NativeFetchRedirectMode::Follow,
            cache_mode: NativeFetchCacheMode::Default,
            timeout: None,
            max_response_bytes: Some(self.max_document_bytes.min(MAX_NATIVE_FORM_BODY_BYTES)),
        };
        self.fetch_request_with_navigation_context_async(request, true)
            .await
    }

    pub(crate) async fn fetch_request_with_navigation_context_async(
        &mut self,
        request: NativeFetchRequest<'_>,
        top_level_navigation: bool,
    ) -> Result<NativeFetchResponse, NativeEngineError> {
        let opened = self
            .open_fetch_response_stream_with_context_async(request, None, top_level_navigation)
            .await?;
        self.finish_fetch_response_stream_async(opened).await
    }

    pub(crate) async fn fetch_request_with_body_async(
        &mut self,
        request: NativeFetchRequest<'_>,
        request_body: reqwest::Body,
    ) -> Result<NativeFetchResponse, NativeEngineError> {
        let opened = self
            .open_fetch_response_stream_with_body_async(request, Some(request_body))
            .await?;
        self.finish_fetch_response_stream_async(opened).await
    }

    async fn finish_fetch_response_stream_async(
        &mut self,
        opened: NativeFetchResponseStream,
    ) -> Result<NativeFetchResponse, NativeEngineError> {
        let NativeFetchResponseStream {
            mut response,
            body: response_body,
            cached_body,
            max_response_bytes,
        } = opened;
        let body = if let Some(body) = cached_body {
            body
        } else {
            let Some(body) = response_body else {
                return Err(NativeEngineError::Worker {
                    operation: "fetch response body".into(),
                    reason: "native fetch response had no readable body".into(),
                });
            };
            let mut stream = body.bytes_stream();
            let mut body = Vec::new();
            while let Some(chunk) = stream.next().await {
                let chunk = chunk.map_err(|error| network_error("fetch response body", error))?;
                self.after_response_chunk(chunk.len()).await;
                let next_len = body.len().saturating_add(chunk.len());
                if next_len > max_response_bytes {
                    return Err(NativeEngineError::limit(
                        "fetch response",
                        max_response_bytes,
                        next_len,
                    ));
                }
                body.extend_from_slice(&chunk);
            }
            body
        };
        response.body = body;
        Ok(response)
    }

    pub(crate) async fn open_fetch_response_stream_async(
        &mut self,
        request: NativeFetchRequest<'_>,
    ) -> Result<NativeFetchResponseStream, NativeEngineError> {
        self.open_fetch_response_stream_with_context_async(request, None, false)
            .await
    }

    /// Open a fetch using a body that is already wired to a bounded native
    /// request stream. The policy and redirect machinery remains shared with
    /// buffered requests; only the first HTTP request may consume the stream.
    /// A 301/302/303 can switch to GET, but a 307/308 cannot replay a one-shot
    /// JavaScript ReadableStream and therefore fails explicitly.
    pub(crate) async fn open_fetch_response_stream_with_body_async(
        &mut self,
        request: NativeFetchRequest<'_>,
        request_body: Option<reqwest::Body>,
    ) -> Result<NativeFetchResponseStream, NativeEngineError> {
        self.open_fetch_response_stream_with_context_async(request, request_body, false)
            .await
    }

    async fn open_fetch_response_stream_with_context_async(
        &mut self,
        request: NativeFetchRequest<'_>,
        request_body: Option<reqwest::Body>,
        top_level_navigation: bool,
    ) -> Result<NativeFetchResponseStream, NativeEngineError> {
        let NativeFetchRequest {
            document_url,
            href,
            referrer_url,
            referrer_policy,
            method,
            body,
            content_type,
            request_headers,
            credentials,
            credentials_mode,
            cors_mode,
            redirect_mode,
            cache_mode,
            timeout,
            max_response_bytes,
        } = request;
        let request_body = request_body;
        let max_response_bytes = max_response_bytes.unwrap_or(self.max_document_bytes);
        if max_response_bytes == 0 || max_response_bytes > MAX_NATIVE_DOWNLOAD_BYTES {
            return Err(NativeEngineError::invalid(
                "fetch response limit",
                format!("must be between 1 and {MAX_NATIVE_DOWNLOAD_BYTES}"),
            ));
        }
        validate_url_text("fetch owner URL", document_url)?;
        validate_url_text("fetch URL", href)?;
        if let Some(referrer_policy) = referrer_policy.as_deref() {
            NativeFetchReferrerPolicy::parse(referrer_policy)?;
        }
        let mut current_headers = validate_fetch_request_headers(&request_headers)?;
        if current_headers.contains_key("content-type") {
            return Err(NativeEngineError::invalid(
                "fetch request headers",
                "content-type must use the dedicated content type field",
            ));
        }
        if method.is_bodyless()
            && (body.is_some() || content_type.is_some() || request_body.is_some())
        {
            return Err(NativeEngineError::invalid(
                format!("{} fetch body", method.as_str()),
                "must be absent",
            ));
        }
        if body
            .as_ref()
            .is_some_and(|body| body.len() > MAX_NATIVE_FORM_BODY_BYTES)
        {
            return Err(NativeEngineError::limit(
                "fetch request body",
                MAX_NATIVE_FORM_BODY_BYTES,
                body.as_ref().map_or(0, NativeRequestBody::len),
            ));
        }
        if content_type
            .as_ref()
            .is_some_and(|content_type| content_type.is_empty())
        {
            return Err(NativeEngineError::invalid(
                "fetch content type",
                "must be non-empty when present",
            ));
        }
        let document_url = Url::parse(without_fragment(document_url)).map_err(|_| {
            NativeEngineError::UnsupportedUrl {
                reason: "fetch owner URL is not valid HTTP(S) syntax".into(),
            }
        })?;
        if !is_network_url(document_url.as_str()) {
            if document_url.scheme() != "fixture" {
                return Err(NativeEngineError::UnsupportedUrl {
                    reason: "native fetch requires an HTTP(S) or fixture document owner".into(),
                });
            }
            if request_body.is_some() {
                return Err(NativeEngineError::UnsupportedUrl {
                    reason: "fixture fetch does not support streaming request bodies".into(),
                });
            }
            let target_url = Url::parse(href)
                .or_else(|_| document_url.join(href))
                .map_err(|_| NativeEngineError::UnsupportedUrl {
                    reason: "fixture fetch URL could not be resolved against its owner".into(),
                })?;
            if target_url.scheme() != "fixture"
                || target_url.host_str() != document_url.host_str()
                || target_url.port_or_known_default() != document_url.port_or_known_default()
            {
                return Err(NativeEngineError::UnsupportedUrl {
                    reason: "fixture fetch must remain on its registered fixture host".into(),
                });
            }
            let target_url = without_fragment(target_url.as_str()).to_owned();
            let resource = self.load(&target_url)?;
            let body = resource.body.into_bytes();
            if body.len() > max_response_bytes {
                return Err(NativeEngineError::limit(
                    "fetch response",
                    max_response_bytes,
                    body.len(),
                ));
            }
            return Ok(NativeFetchResponseStream {
                response: NativeFetchResponse {
                    url: target_url,
                    status: 200,
                    status_text: "OK".into(),
                    content_type: None,
                    headers: Vec::new(),
                    body: Vec::new(),
                    redirected: false,
                    opaque: false,
                    opaque_redirect: false,
                },
                body: None,
                cached_body: Some(body),
                max_response_bytes,
            });
        }
        let referrer_source = fetch_referrer_source(referrer_url.as_deref(), &document_url)?;
        reject_credentials(&document_url)?;
        let Some(target_url) = resolve_subresource_url(&document_url, href)? else {
            return Err(NativeEngineError::UnsupportedUrl {
                reason: "native fetch URL is not an HTTP(S) resource".into(),
            });
        };
        let cross_origin = document_url.origin() != target_url.origin();
        if cors_mode == NativeCorsMode::SameOrigin && cross_origin {
            return Err(NativeEngineError::Network {
                operation: "fetch mode".into(),
                reason: "same-origin fetch target has a different origin".into(),
            });
        }
        if !top_level_navigation && !mixed_content_allowed(&document_url, &target_url) {
            return Err(NativeEngineError::Network {
                operation: "fetch policy".into(),
                reason: "HTTPS documents cannot fetch HTTP resources".into(),
            });
        }
        let policy = self
            .network
            .document_policies
            .get(&cache_key(&document_url))
            .cloned()
            .unwrap_or_default();
        if cors_mode != NativeCorsMode::Navigation {
            self.record_report_only_url_violations(
                &policy,
                NativeSubresourceKind::Connect,
                &document_url,
                &target_url,
            );
        }
        if cors_mode != NativeCorsMode::Navigation
            && !policy.allows(NativeSubresourceKind::Connect, &document_url, &target_url)
        {
            return Err(NativeEngineError::Network {
                operation: "fetch policy".into(),
                reason: "document CSP blocked the connect target".into(),
            });
        }
        if cache_mode.is_only_if_cached() && cors_mode != NativeCorsMode::SameOrigin {
            return Err(NativeEngineError::Network {
                operation: "fetch cache mode".into(),
                reason: "only-if-cached requires same-origin fetch mode".into(),
            });
        }
        let cacheable_request = matches!(method.as_str(), "GET" | "HEAD")
            && body.is_none()
            && redirect_mode == NativeFetchRedirectMode::Follow
            && (cors_mode != NativeCorsMode::Navigation || top_level_navigation);
        let initial_credentials =
            fetch_credentials_for_url(credentials_mode, credentials, &document_url, &target_url);
        let document_referrer_policy = self
            .network
            .document_referrer_policies
            .get(&cache_key(&document_url))
            .copied()
            .unwrap_or(NativeFetchReferrerPolicy::StrictOriginWhenCrossOrigin);
        let mut effective_referrer_policy = match referrer_policy.as_deref() {
            Some(policy) if !policy.is_empty() => NativeFetchReferrerPolicy::parse(policy)?,
            Some(_) | None => document_referrer_policy,
        };
        let initial_referrer = fetch_referrer_for_target(
            referrer_source.as_ref(),
            &target_url,
            effective_referrer_policy,
        );
        let cookie_initiator = if top_level_navigation {
            referrer_source.as_ref()
        } else {
            Some(&document_url)
        };
        let request_cookie = if cacheable_request && initial_credentials {
            self.network.cookie_header_for_request(
                &target_url,
                cookie_initiator,
                top_level_navigation,
                method.clone(),
            )
        } else {
            None
        };
        let fetch_cache_key = cacheable_request.then(|| {
            fetch_response_cache_key(
                &document_url,
                &target_url,
                &method,
                &current_headers,
                content_type.as_deref(),
                initial_credentials,
                cors_mode,
                request_cookie.as_deref(),
                initial_referrer.as_deref(),
                effective_referrer_policy.as_str(),
            )
        });
        let cached_fetch = fetch_cache_key
            .as_ref()
            .and_then(|key| self.network.fetch_cache.get(key))
            .cloned()
            .filter(|cached| cached.response.body.len() <= max_response_bytes);
        let cache_hit = cached_fetch.clone().filter(|cached| {
            cache_mode.can_read()
                && !cache_mode.requires_revalidation()
                && (matches!(
                    cache_mode,
                    NativeFetchCacheMode::ForceCache | NativeFetchCacheMode::OnlyIfCached
                ) || cached.is_fresh(Instant::now()))
        });
        if let Some(cached) = cache_hit {
            let mut response = cached.response;
            let body = std::mem::take(&mut response.body);
            return Ok(NativeFetchResponseStream {
                response,
                body: None,
                cached_body: Some(body),
                max_response_bytes,
            });
        }
        if cache_mode.is_only_if_cached() {
            return Err(NativeEngineError::Network {
                operation: "fetch cache mode".into(),
                reason: "no usable response is available in the native cache".into(),
            });
        }
        let stale_cached_fetch = cached_fetch.filter(|cached| {
            cache_mode.can_read()
                && !matches!(
                    cache_mode,
                    NativeFetchCacheMode::ForceCache | NativeFetchCacheMode::OnlyIfCached
                )
                && (cache_mode.requires_revalidation() || !cached.is_fresh(Instant::now()))
        });

        let client = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(
                timeout
                    .unwrap_or(NATIVE_NETWORK_TIMEOUT)
                    .min(NATIVE_NETWORK_TIMEOUT),
            )
            .build()
            .map_err(|error| network_error("fetch client construction", error))?;
        let mut current_url = target_url;
        let mut current_method = method;
        let mut current_body = body;
        let mut current_request_body = request_body;
        let mut streaming_request_body_sent = current_request_body.is_some();
        let mut current_content_type = content_type;
        let mut request_referrer = initial_referrer;
        let mut redirects = 0;
        let mut redirected = false;
        let mut no_cors_cross_origin = cross_origin;
        let mut pending_cookies = Vec::new();
        let response = loop {
            let mut request_url = current_url.clone();
            request_url.set_fragment(None);
            let current_credentials = fetch_credentials_for_url(
                credentials_mode,
                credentials,
                &document_url,
                &current_url,
            );
            let requested_headers =
                cors_preflight_request_headers(current_content_type.as_deref(), &current_headers);
            let cross_origin_request = document_url.origin() != current_url.origin();
            let simple_method = current_method.is_simple();
            if cors_mode == NativeCorsMode::NoCors
                && cross_origin_request
                && (!simple_method || !requested_headers.is_empty())
            {
                return Err(NativeEngineError::Network {
                    operation: "fetch mode".into(),
                    reason:
                        "no-cors fetch contains a non-safelisted method, request header, or content type".into(),
                });
            }
            if (!simple_method || !requested_headers.is_empty())
                && cors_mode == NativeCorsMode::Cors
                && cors_origin_header(&document_url, &current_url, NativeCorsMode::Cors).is_some()
            {
                self.verify_cors_preflight(
                    &client,
                    &document_url,
                    &request_url,
                    &current_method,
                    &requested_headers,
                    current_credentials,
                    request_referrer.as_deref(),
                )
                .await?;
            }
            let accept = current_headers
                .get("accept")
                .map(String::as_str)
                .unwrap_or("*/*");
            let mut request = self.apply_environment_headers(
                client
                    .request(current_method.reqwest_method(), request_url)
                    .header(reqwest::header::ACCEPT, accept),
            );
            if let Some(body) = current_request_body.take() {
                request = request.body(body);
            } else if let Some(body) = current_body.as_ref() {
                request = match body {
                    NativeRequestBody::Text(body) => request.body(body.clone()),
                    NativeRequestBody::Bytes(body) => request.body(body.clone()),
                };
            }
            if let Some(content_type) = current_content_type.as_deref() {
                request = request.header(reqwest::header::CONTENT_TYPE, content_type);
            }
            if redirects == 0
                && let Some(cache_control) = cache_mode.request_cache_control()
                && !current_headers.contains_key("cache-control")
            {
                request = request.header(reqwest::header::CACHE_CONTROL, cache_control);
            }
            for (name, value) in &current_headers {
                if name == "accept" {
                    continue;
                }
                request = request.header(name, value);
            }
            if redirects == 0
                && let Some(cached) = stale_cached_fetch.as_ref()
            {
                if let Some(etag) = cached.etag.as_deref() {
                    request = request.header(reqwest::header::IF_NONE_MATCH, etag);
                }
                if let Some(last_modified) = cached.last_modified.as_deref() {
                    request = request.header(reqwest::header::IF_MODIFIED_SINCE, last_modified);
                }
            }
            if let Some(origin) = cors_origin_header(&document_url, &current_url, cors_mode) {
                request = request.header(reqwest::header::ORIGIN, origin);
            }
            if let Some(referrer) = request_referrer.as_deref() {
                request = request.header(reqwest::header::REFERER, referrer);
            }
            if current_credentials
                && let Some(cookie) = self.network.cookie_header_for_request(
                    &current_url,
                    cookie_initiator,
                    top_level_navigation,
                    current_method.clone(),
                )
            {
                request = request.header(reqwest::header::COOKIE, cookie);
            }
            self.before_request(current_body.as_ref().map_or(0, NativeRequestBody::len))
                .await?;
            let response = request
                .send()
                .await
                .map_err(|error| network_error("fetch request", error))?;
            if current_credentials {
                for value in response
                    .headers()
                    .get_all(reqwest::header::SET_COOKIE)
                    .iter()
                {
                    if let Ok(cookie) = value.to_str() {
                        pending_cookies.push((current_url.clone(), cookie.to_owned()));
                    }
                }
            }
            if !is_http_redirect(response.status()) {
                break response;
            }
            if redirect_mode == NativeFetchRedirectMode::Error {
                return Err(NativeEngineError::Network {
                    operation: "fetch redirect".into(),
                    reason: "fetch redirect is disallowed by redirect mode".into(),
                });
            }
            if redirect_mode == NativeFetchRedirectMode::Manual {
                for (cookie_url, cookie) in pending_cookies {
                    self.cookie_changes
                        .extend(self.network.store_cookie(&cookie_url, &cookie));
                }
                return Ok(NativeFetchResponseStream {
                    response: NativeFetchResponse {
                        url: without_fragment(current_url.as_str()).to_owned(),
                        status: response.status().as_u16(),
                        status_text: response
                            .status()
                            .canonical_reason()
                            .unwrap_or_default()
                            .to_owned(),
                        content_type: None,
                        headers: Vec::new(),
                        body: Vec::new(),
                        redirected: false,
                        opaque: false,
                        opaque_redirect: true,
                    },
                    body: Some(response),
                    cached_body: None,
                    max_response_bytes,
                });
            }
            if redirects >= MAX_NATIVE_NETWORK_REDIRECTS {
                return Err(NativeEngineError::Network {
                    operation: "fetch redirect".into(),
                    reason: "fetch redirect chain exceeded the native limit".into(),
                });
            }
            let location = response
                .headers()
                .get(reqwest::header::LOCATION)
                .and_then(|value| value.to_str().ok())
                .filter(|value| !value.is_empty())
                .ok_or_else(|| NativeEngineError::Network {
                    operation: "fetch redirect".into(),
                    reason: "fetch redirect did not provide a valid location".into(),
                })?;
            let next_url = current_url
                .join(location)
                .map_err(|_| NativeEngineError::Network {
                    operation: "fetch redirect".into(),
                    reason: "fetch redirect location is not valid URL syntax".into(),
                })?;
            reject_credentials(&next_url)?;
            if cors_mode != NativeCorsMode::Navigation {
                self.record_report_only_url_violations_for_redirect(
                    &policy,
                    NativeSubresourceKind::Connect,
                    &document_url,
                    &next_url,
                    true,
                    None,
                );
            }
            if !is_network_url(without_fragment(next_url.as_str()))
                || !mixed_content_allowed(&document_url, &next_url)
                || (cors_mode != NativeCorsMode::Navigation
                    && !policy.allows_redirect(
                        NativeSubresourceKind::Connect,
                        &document_url,
                        &next_url,
                    ))
            {
                return Err(NativeEngineError::Network {
                    operation: "fetch redirect policy".into(),
                    reason: "fetch redirect was blocked by URL, CSP, or mixed-content policy"
                        .into(),
                });
            }
            if cors_mode == NativeCorsMode::SameOrigin && document_url.origin() != next_url.origin()
            {
                return Err(NativeEngineError::Network {
                    operation: "fetch redirect mode".into(),
                    reason: "same-origin fetch redirect has a different origin".into(),
                });
            }
            if let Some(policy) = referrer_policy_from_headers(response.headers()) {
                effective_referrer_policy = policy;
            }
            request_referrer = fetch_referrer_for_target(
                referrer_source.as_ref(),
                &next_url,
                effective_referrer_policy,
            );
            if current_url.origin() != next_url.origin() {
                current_headers.remove("authorization");
            }
            no_cors_cross_origin |= document_url.origin() != next_url.origin();
            if matches!(response.status().as_u16(), 301..=303)
                && !matches!(current_method.as_str(), "GET" | "HEAD")
            {
                current_method = NativeFetchMethod::get();
                current_body = None;
                current_request_body = None;
                streaming_request_body_sent = false;
                current_content_type = None;
            } else if streaming_request_body_sent {
                return Err(NativeEngineError::Network {
                    operation: "fetch redirect".into(),
                    reason: "a streaming request body cannot be replayed across this redirect"
                        .into(),
                });
            }
            current_url = next_url;
            redirects += 1;
            redirected = true;
        };
        let final_url = current_url;
        let final_credentials =
            fetch_credentials_for_url(credentials_mode, credentials, &document_url, &final_url);
        let status = response.status().as_u16();
        let status_text = response
            .status()
            .canonical_reason()
            .unwrap_or_default()
            .to_owned();
        let response_headers = response.headers().clone();
        let has_set_cookie = !pending_cookies.is_empty();
        if response.status() == reqwest::StatusCode::NOT_MODIFIED {
            let Some(cached) = stale_cached_fetch else {
                return Err(NativeEngineError::Network {
                    operation: "fetch cache revalidation".into(),
                    reason: "server returned HTTP 304 without a stale cached response".into(),
                });
            };
            if redirects != 0 {
                return Err(NativeEngineError::Network {
                    operation: "fetch cache revalidation".into(),
                    reason: "HTTP 304 was received after a fetch redirect".into(),
                });
            }
            for (cookie_url, cookie) in pending_cookies {
                self.cookie_changes
                    .extend(self.network.store_cookie(&cookie_url, &cookie));
            }
            let mut cached_response = cached.response.clone();
            let cached_body = std::mem::take(&mut cached_response.body);
            if has_set_cookie {
                if let Some(key) = fetch_cache_key.as_deref() {
                    self.network.remove_fetch_cache(key);
                }
            } else if let Some(entry) =
                cached.refresh_from_not_modified(&response_headers, Instant::now())
            {
                if let Some(key) = fetch_cache_key.as_deref() {
                    self.network.store_fetch_cache(key.to_owned(), entry);
                }
            } else if let Some(key) = fetch_cache_key.as_deref() {
                self.network.remove_fetch_cache(key);
            }
            return Ok(NativeFetchResponseStream {
                response: cached_response,
                body: None,
                cached_body: Some(cached_body),
                max_response_bytes,
            });
        }
        if cors_mode == NativeCorsMode::Cors
            && !cors_response_allowed(
                &response_headers,
                &document_url,
                &final_url,
                final_credentials,
            )
        {
            return Err(NativeEngineError::Network {
                operation: "fetch CORS policy".into(),
                reason: "cross-origin fetch response did not authorize the document origin".into(),
            });
        }
        let content_length = response.content_length();
        if content_length.is_some_and(|length| length > max_response_bytes as u64) {
            return Err(NativeEngineError::limit(
                "fetch response",
                max_response_bytes,
                content_length
                    .and_then(|length| usize::try_from(length).ok())
                    .unwrap_or(usize::MAX),
            ));
        }
        for (cookie_url, cookie) in pending_cookies {
            self.cookie_changes
                .extend(self.network.store_cookie(&cookie_url, &cookie));
        }
        let opaque = cors_mode == NativeCorsMode::NoCors && no_cors_cross_origin;
        let content_type = if opaque {
            None
        } else {
            response_headers
                .get(reqwest::header::CONTENT_TYPE)
                .and_then(|value| value.to_str().ok())
                .map(str::to_owned)
        };
        let headers = if opaque {
            Vec::new()
        } else {
            exposed_response_headers(
                &response_headers,
                top_level_navigation || document_url.origin() == final_url.origin(),
                final_credentials,
            )?
        };
        let mut fetch_response = NativeFetchResponse {
            url: without_fragment(final_url.as_str()).to_owned(),
            status,
            status_text,
            content_type,
            headers,
            body: Vec::new(),
            redirected,
            opaque,
            opaque_redirect: false,
        };
        let should_cache_response = fetch_cache_key.is_some()
            && cache_mode.can_store()
            && !has_set_cookie
            && !opaque
            && status != reqwest::StatusCode::PARTIAL_CONTENT.as_u16()
            && response_cache_metadata_present(&response_headers);
        if should_cache_response {
            let mut stream = response.bytes_stream();
            let mut body = Vec::new();
            while let Some(chunk) = stream.next().await {
                let chunk = chunk.map_err(|error| network_error("fetch response body", error))?;
                self.after_response_chunk(chunk.len()).await;
                let next_len = body.len().saturating_add(chunk.len());
                if next_len > max_response_bytes {
                    return Err(NativeEngineError::limit(
                        "fetch response",
                        max_response_bytes,
                        next_len,
                    ));
                }
                body.extend_from_slice(&chunk);
            }
            fetch_response.body = body.clone();
            if let Some(entry) = NativeFetchCacheEntry::from_response(
                fetch_response.clone(),
                &response_headers,
                Instant::now(),
            ) {
                if let Some(key) = fetch_cache_key.as_deref() {
                    self.network.store_fetch_cache(key.to_owned(), entry);
                }
            } else if let Some(key) = fetch_cache_key.as_deref() {
                self.network.remove_fetch_cache(key);
            }
            fetch_response.body.clear();
            return Ok(NativeFetchResponseStream {
                response: fetch_response,
                body: None,
                cached_body: Some(body),
                max_response_bytes,
            });
        }
        if cache_mode.can_store()
            && let Some(key) = fetch_cache_key.as_deref()
        {
            self.network.remove_fetch_cache(key);
        }
        Ok(NativeFetchResponseStream {
            response: fetch_response,
            body: Some(response),
            cached_body: None,
            max_response_bytes,
        })
    }

    async fn verify_cors_preflight(
        &mut self,
        client: &reqwest::Client,
        document_url: &Url,
        target_url: &Url,
        method: &NativeFetchMethod,
        requested_headers: &[String],
        credentials: bool,
        referrer: Option<&str>,
    ) -> Result<(), NativeEngineError> {
        let Some(origin) = cors_origin_header(document_url, target_url, NativeCorsMode::Cors)
        else {
            return Ok(());
        };
        let method = method.as_str();
        let cache_key = cors_preflight_cache_key(
            document_url,
            target_url,
            method,
            requested_headers,
            credentials,
        );
        let now = Instant::now();
        if self
            .network
            .preflight_cache
            .get(&cache_key)
            .is_some_and(|expires_at| *expires_at > now)
        {
            return Ok(());
        }
        self.network.preflight_cache.remove(&cache_key);
        let mut request = self.apply_environment_headers(
            client
                .request(reqwest::Method::OPTIONS, target_url.clone())
                .header("Origin", origin)
                .header("Access-Control-Request-Method", method),
        );
        if let Some(referrer) = referrer {
            request = request.header(reqwest::header::REFERER, referrer);
        }
        if !requested_headers.is_empty() {
            request = request.header(
                "Access-Control-Request-Headers",
                requested_headers.join(", "),
            );
        }
        self.before_request(0).await?;
        let response = request
            .send()
            .await
            .map_err(|error| network_error("fetch preflight request", error))?;
        if !response.status().is_success()
            || !cors_preflight_response_allowed(
                response.headers(),
                document_url,
                target_url,
                method,
                requested_headers,
                credentials,
            )
        {
            return Err(NativeEngineError::Network {
                operation: "fetch preflight".into(),
                reason: "cross-origin preflight did not authorize the request".into(),
            });
        }
        if let Some(age) = cors_preflight_cache_age(response.headers())
            && !age.is_zero()
        {
            if !self.network.preflight_cache.contains_key(&cache_key)
                && self.network.preflight_cache.len() >= MAX_NATIVE_PREFLIGHT_CACHE_ENTRIES
                && let Some(oldest) = self.network.preflight_cache.keys().next().cloned()
            {
                self.network.preflight_cache.remove(&oldest);
            }
            self.network.preflight_cache.insert(cache_key, now + age);
        }
        Ok(())
    }

    /// Load a Blob-backed image for a local document without entering the
    /// asynchronous network or HTTP-cache path.
    pub(crate) fn load_local_blob_image(
        &mut self,
        document_url: &str,
        src: &str,
        object_url: Option<&NativeObjectUrlResource>,
    ) -> Result<Option<NativeImage>, NativeEngineError> {
        validate_url_text("document URL", document_url)?;
        validate_url_text("image URL", src)?;
        let document_url = Url::parse(without_fragment(document_url)).map_err(|_| {
            NativeEngineError::UnsupportedUrl {
                reason: "image owner URL is not valid URL syntax".into(),
            }
        })?;
        reject_credentials(&document_url)?;
        if is_network_url(document_url.as_str()) {
            return Ok(None);
        }
        let Some(target_url) = resolve_subresource_url_with_blob(&document_url, src)? else {
            return Ok(None);
        };
        if !target_url.scheme().eq_ignore_ascii_case("blob") {
            return Ok(None);
        }
        let Some(object_url) = object_url else {
            return Ok(None);
        };
        NativeOrigin::from_blob_url(without_fragment(target_url.as_str()))?;
        let Some(media_type) = object_url
            .content_type
            .as_deref()
            .and_then(supported_image_media_type_text)
        else {
            return Ok(None);
        };
        Ok(decode_image_bytes(
            &object_url.body,
            media_type,
            MAX_NATIVE_IMAGE_TRANSFER_BYTES,
        ))
    }

    /// Load a rooted file image for a local file document. The path is
    /// canonicalized before reading so symlink escapes cannot widen the
    /// configured filesystem capability.
    pub(crate) fn load_local_file_image(
        &self,
        document_url: &str,
        src: &str,
    ) -> Result<Option<NativeImage>, NativeEngineError> {
        validate_url_text("document URL", document_url)?;
        validate_url_text("image URL", src)?;
        let Some((target_url, path)) =
            self.local_file_subresource_path(document_url, src, "file image")?
        else {
            return Ok(None);
        };
        if !self.rooted_file_subresource_allowed(
            document_url,
            &target_url,
            &path,
            NativeSubresourceKind::Image,
            "file image CSP owner URL",
        )? {
            return Ok(None);
        }
        let bytes = read_bounded_file(&path, MAX_NATIVE_IMAGE_TRANSFER_BYTES, "file image")?;
        let Some(media_type) = file_image_media_type(&path) else {
            return Ok(None);
        };
        Ok(decode_image_bytes(
            &bytes,
            media_type,
            MAX_NATIVE_IMAGE_TRANSFER_BYTES,
        ))
    }

    /// Load a Blob-backed stylesheet for a local document without entering the
    /// asynchronous network or HTTP-cache path.
    pub(crate) fn load_local_blob_stylesheet(
        &mut self,
        document_url: &str,
        href: &str,
        integrity: Option<&str>,
        object_url: Option<&NativeObjectUrlResource>,
    ) -> Result<Option<String>, NativeEngineError> {
        validate_url_text("document URL", document_url)?;
        validate_url_text("stylesheet URL", href)?;
        let document_url = Url::parse(without_fragment(document_url)).map_err(|_| {
            NativeEngineError::UnsupportedUrl {
                reason: "stylesheet owner URL is not valid URL syntax".into(),
            }
        })?;
        reject_credentials(&document_url)?;
        if is_network_url(document_url.as_str()) {
            return Ok(None);
        }
        let Some(target_url) = resolve_subresource_url_with_blob(&document_url, href)? else {
            return Ok(None);
        };
        if !target_url.scheme().eq_ignore_ascii_case("blob") {
            return Ok(None);
        }
        let Some(object_url) = object_url else {
            return Ok(None);
        };
        NativeOrigin::from_blob_url(without_fragment(target_url.as_str()))?;
        if object_url
            .content_type
            .as_deref()
            .is_some_and(|content_type| {
                !content_type.is_empty() && !stylesheet_content_type_text_allowed(content_type)
            })
        {
            return Ok(None);
        }
        if object_url.body.len() > self.max_document_bytes {
            return Err(NativeEngineError::limit(
                "Blob CSS subresource",
                self.max_document_bytes,
                object_url.body.len(),
            ));
        }
        if !subresource_integrity_matches(integrity, &object_url.body) {
            return Ok(None);
        }
        let body =
            String::from_utf8(object_url.body.clone()).map_err(|_| NativeEngineError::Network {
                operation: "Blob CSS subresource decoding".into(),
                reason: "Blob CSS subresource is not valid UTF-8".into(),
            })?;
        Ok(Some(body))
    }

    /// Load a bounded rooted file stylesheet for a local file document.
    /// Local files do not carry an HTTP response MIME header, so the
    /// stylesheet link's resource type is represented by the caller and the
    /// bytes are admitted as UTF-8 CSS after root, CSP, and integrity checks.
    pub(crate) fn load_local_file_stylesheet(
        &self,
        document_url: &str,
        href: &str,
        integrity: Option<&str>,
    ) -> Result<Option<String>, NativeEngineError> {
        self.load_local_file_stylesheet_with_nonce(document_url, href, integrity, None)
    }

    pub(crate) fn load_local_file_stylesheet_with_nonce(
        &self,
        document_url: &str,
        href: &str,
        integrity: Option<&str>,
        nonce: Option<&str>,
    ) -> Result<Option<String>, NativeEngineError> {
        validate_url_text("document URL", document_url)?;
        validate_url_text("stylesheet URL", href)?;
        let Some((target_url, path)) =
            self.local_file_subresource_path(document_url, href, "file stylesheet")?
        else {
            return Ok(None);
        };
        if let Some((policy_owner_url, Some(_))) =
            self.csp_document_owner(document_url, "file stylesheet CSP owner URL")?
        {
            let Some(resource_root) = self.allowed_file_root_for_path(&path) else {
                return Ok(None);
            };
            let policy = self
                .network
                .document_policies
                .get(&cache_key(&policy_owner_url))
                .cloned()
                .unwrap_or_default();
            if !policy.allows_rooted_file_style(
                &policy_owner_url,
                &target_url,
                nonce,
                Some(resource_root),
            ) {
                return Ok(None);
            }
        }
        let bytes = read_bounded_file(&path, self.max_document_bytes, "file CSS subresource")?;
        if !subresource_integrity_matches(integrity, &bytes) {
            return Ok(None);
        }
        let body = String::from_utf8(bytes).map_err(|_| NativeEngineError::Network {
            operation: "file CSS subresource decoding".into(),
            reason: "file CSS subresource is not valid UTF-8".into(),
        })?;
        Ok(Some(body))
    }

    pub(crate) async fn load_stylesheet_async(
        &mut self,
        document_url: &str,
        href: &str,
        integrity: Option<&str>,
        crossorigin: Option<&str>,
    ) -> Result<Option<NativeStylesheetResource>, NativeEngineError> {
        self.load_stylesheet_async_with_object_url(document_url, href, integrity, crossorigin, None)
            .await
    }

    pub(crate) async fn load_stylesheet_async_with_object_url(
        &mut self,
        document_url: &str,
        href: &str,
        integrity: Option<&str>,
        crossorigin: Option<&str>,
        object_url: Option<&NativeObjectUrlResource>,
    ) -> Result<Option<NativeStylesheetResource>, NativeEngineError> {
        self.load_stylesheet_async_with_object_url_and_referrer_policy(
            document_url,
            href,
            integrity,
            crossorigin,
            object_url,
            None,
        )
        .await
    }

    pub(crate) async fn load_stylesheet_async_with_object_url_and_referrer_policy(
        &mut self,
        document_url: &str,
        href: &str,
        integrity: Option<&str>,
        crossorigin: Option<&str>,
        object_url: Option<&NativeObjectUrlResource>,
        referrer_policy: Option<NativeFetchReferrerPolicy>,
    ) -> Result<Option<NativeStylesheetResource>, NativeEngineError> {
        validate_url_text("document URL", document_url)?;
        validate_url_text("stylesheet URL", href)?;
        let document_url = Url::parse(without_fragment(document_url)).map_err(|_| {
            NativeEngineError::UnsupportedUrl {
                reason: "stylesheet owner URL is not valid HTTP(S) syntax".into(),
            }
        })?;
        if !is_network_url(document_url.as_str()) {
            return Ok(None);
        }
        reject_credentials(&document_url)?;
        let Some(target_url) = resolve_subresource_url_with_blob(&document_url, href)? else {
            return Ok(None);
        };
        if !mixed_content_allowed(&document_url, &target_url) {
            return Ok(None);
        }
        let integrity_required = integrity_metadata_is_enforced(integrity);
        let cross_origin_target = document_url.origin() != target_url.origin();
        let can_reuse_cached_stylesheet =
            !(cross_origin_target && (integrity_required || crossorigin.is_some()));
        let policy = self
            .network
            .document_policies
            .get(&cache_key(&document_url))
            .cloned()
            .unwrap_or_default();
        self.record_report_only_url_violations(
            &policy,
            NativeSubresourceKind::Style,
            &document_url,
            &target_url,
        );
        if !policy.allows(NativeSubresourceKind::Style, &document_url, &target_url) {
            return Ok(None);
        }
        if target_url.scheme().eq_ignore_ascii_case("blob") {
            let Some(object_url) = object_url else {
                return Ok(None);
            };
            NativeOrigin::from_blob_url(without_fragment(target_url.as_str()))?;
            if object_url
                .content_type
                .as_deref()
                .is_some_and(|content_type| {
                    !content_type.is_empty() && !stylesheet_content_type_text_allowed(content_type)
                })
            {
                return Ok(None);
            }
            if object_url.body.len() > self.max_document_bytes {
                return Err(NativeEngineError::limit(
                    "Blob CSS subresource",
                    self.max_document_bytes,
                    object_url.body.len(),
                ));
            }
            if !subresource_integrity_matches(integrity, &object_url.body) {
                return Ok(None);
            }
            let body = String::from_utf8(object_url.body.clone()).map_err(|_| {
                NativeEngineError::Network {
                    operation: "Blob CSS subresource decoding".into(),
                    reason: "Blob CSS subresource is not valid UTF-8".into(),
                }
            })?;
            return Ok(Some(NativeStylesheetResource {
                url: without_fragment(target_url.as_str()).to_owned(),
                body,
            }));
        }
        let requested_cache_key = cache_key(&target_url);
        let stale_cached_stylesheet = self
            .network
            .stylesheet_cache
            .get(&requested_cache_key)
            .cloned()
            .filter(|cached| can_reuse_cached_stylesheet && !cached.is_fresh(Instant::now()));
        if let Some(cached) = self.network.stylesheet_cache.get(&requested_cache_key)
            && can_reuse_cached_stylesheet
            && cached.is_fresh(Instant::now())
        {
            return Ok(
                subresource_integrity_matches(integrity, cached.body.as_bytes()).then_some(
                    NativeStylesheetResource {
                        url: cached.url.clone(),
                        body: cached.body.clone(),
                    },
                ),
            );
        }

        let client = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(NATIVE_NETWORK_TIMEOUT)
            .build()
            .map_err(|error| network_error("CSS client construction", error))?;
        let mut current_url = target_url;
        let mut effective_referrer_policy = referrer_policy;
        let mut request_referrer = match effective_referrer_policy {
            Some(policy) => fetch_referrer_for_target(Some(&document_url), &current_url, policy),
            None => normalize_referrer(Some(document_url.as_str()), &current_url)?,
        };
        let mut redirects = 0;
        let mut pending_cookies = Vec::new();
        let response = loop {
            let mut request_url = current_url.clone();
            request_url.set_fragment(None);
            let mut request = self.apply_environment_headers(
                client
                    .get(request_url)
                    .header(reqwest::header::ACCEPT, "text/css"),
            );
            if let Some(referrer) = request_referrer.as_deref() {
                request = request.header(reqwest::header::REFERER, referrer);
            }
            if crossorigin.is_some()
                && let Some(origin) =
                    cors_origin_header(&document_url, &current_url, NativeCorsMode::Cors)
            {
                request = request.header(reqwest::header::ORIGIN, origin);
            }
            if redirects == 0
                && let Some(cached) = stale_cached_stylesheet.as_ref()
            {
                if let Some(etag) = cached.etag.as_deref() {
                    request = request.header(reqwest::header::IF_NONE_MATCH, etag);
                }
                if let Some(last_modified) = cached.last_modified.as_deref() {
                    request = request.header(reqwest::header::IF_MODIFIED_SINCE, last_modified);
                }
            }
            let credentials = subresource_credentials(crossorigin).unwrap_or(false);
            if (document_url.origin() == current_url.origin() || credentials)
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
                .map_err(|error| network_error("CSS subresource request", error))?;
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
                return Err(NativeEngineError::Network {
                    operation: "CSS redirect".into(),
                    reason: "CSS subresource redirect chain exceeded the native limit".into(),
                });
            }
            let location = response
                .headers()
                .get(reqwest::header::LOCATION)
                .and_then(|value| value.to_str().ok())
                .filter(|value| !value.is_empty())
                .ok_or_else(|| NativeEngineError::Network {
                    operation: "CSS redirect".into(),
                    reason: "CSS subresource redirect did not provide a valid location".into(),
                })?;
            let next_url = current_url
                .join(location)
                .map_err(|_| NativeEngineError::Network {
                    operation: "CSS redirect".into(),
                    reason: "CSS subresource redirect location is not valid URL syntax".into(),
                })?;
            reject_credentials(&next_url)?;
            self.record_report_only_url_violations_for_redirect(
                &policy,
                NativeSubresourceKind::Style,
                &document_url,
                &next_url,
                true,
                None,
            );
            if !is_network_url(without_fragment(next_url.as_str()))
                || !mixed_content_allowed(&document_url, &next_url)
                || !policy.allows_redirect(NativeSubresourceKind::Style, &document_url, &next_url)
            {
                return Ok(None);
            }
            if let Some(policy) = referrer_policy_from_headers(response.headers())
                && effective_referrer_policy.is_some()
            {
                effective_referrer_policy = Some(policy);
            }
            request_referrer = match effective_referrer_policy {
                Some(policy) => fetch_referrer_for_target(Some(&document_url), &next_url, policy),
                None => normalize_referrer(Some(current_url.as_str()), &next_url)?,
            };
            current_url = next_url;
            redirects += 1;
        };
        let response_headers = response.headers().clone();
        let has_set_cookie = !pending_cookies.is_empty();
        if response.status() == reqwest::StatusCode::NOT_MODIFIED {
            let Some(cached) = stale_cached_stylesheet else {
                return Err(NativeEngineError::Network {
                    operation: "CSS subresource revalidation".into(),
                    reason: "server returned HTTP 304 without a stale cached stylesheet".into(),
                });
            };
            if redirects != 0 {
                return Err(NativeEngineError::Network {
                    operation: "CSS subresource revalidation".into(),
                    reason: "HTTP 304 was received after a CSS redirect".into(),
                });
            }
            for (cookie_url, cookie) in pending_cookies {
                self.cookie_changes
                    .extend(self.network.store_cookie(&cookie_url, &cookie));
            }
            let cached_url = cached.url.clone();
            let cached_body = cached.body.clone();
            if !subresource_integrity_matches(integrity, cached_body.as_bytes()) {
                return Ok(None);
            }
            if has_set_cookie {
                self.network.remove_stylesheet_cache(&requested_cache_key);
            } else if let Some(entry) =
                cached.refresh_from_not_modified(&response_headers, Instant::now())
            {
                self.network
                    .store_stylesheet_cache(requested_cache_key, entry);
            } else {
                self.network.remove_stylesheet_cache(&requested_cache_key);
            }
            return Ok(Some(NativeStylesheetResource {
                url: cached_url,
                body: cached_body,
            }));
        }
        if !response.status().is_success() {
            return Err(NativeEngineError::Network {
                operation: "CSS subresource request".into(),
                reason: format!("server returned HTTP {}", response.status().as_u16()),
            });
        }
        if !subresource_response_allowed(
            &response_headers,
            &document_url,
            &current_url,
            integrity,
            crossorigin,
        ) {
            return Ok(None);
        }
        if !content_type_is(
            response.headers().get(reqwest::header::CONTENT_TYPE),
            "text/css",
        )? {
            return Err(NativeEngineError::UnsupportedUrl {
                reason: "CSS subresource returned an unsupported content type".into(),
            });
        }
        let content_length = response.content_length();
        if content_length.is_some_and(|length| length > self.max_document_bytes as u64) {
            return Err(NativeEngineError::limit(
                "CSS subresource",
                self.max_document_bytes,
                content_length
                    .and_then(|length| usize::try_from(length).ok())
                    .unwrap_or(usize::MAX),
            ));
        }
        let mut stream = response.bytes_stream();
        let mut bytes = Vec::with_capacity(
            content_length
                .unwrap_or_default()
                .min(self.max_document_bytes as u64) as usize,
        );
        while let Some(chunk) = stream.next().await {
            let chunk = chunk.map_err(|error| network_error("CSS subresource body", error))?;
            self.after_response_chunk(chunk.len()).await;
            let next_len = bytes.len().saturating_add(chunk.len());
            if next_len > self.max_document_bytes {
                return Err(NativeEngineError::limit(
                    "CSS subresource",
                    self.max_document_bytes,
                    next_len,
                ));
            }
            bytes.extend_from_slice(&chunk);
        }
        if !subresource_integrity_matches(integrity, &bytes) {
            return Ok(None);
        }
        let body = String::from_utf8(bytes).map_err(|_| NativeEngineError::Network {
            operation: "CSS subresource decoding".into(),
            reason: "CSS subresource is not valid UTF-8".into(),
        })?;
        for (cookie_url, cookie) in pending_cookies {
            self.cookie_changes
                .extend(self.network.store_cookie(&cookie_url, &cookie));
        }
        if !has_set_cookie
            && let Some(entry) = NativeTextCacheEntry::from_response(
                current_url.to_string(),
                body.clone(),
                &response_headers,
                Instant::now(),
            )
        {
            self.network
                .store_stylesheet_cache(requested_cache_key, entry.clone());
            self.network
                .store_stylesheet_cache(cache_key(&current_url), entry);
        } else {
            self.network.remove_stylesheet_cache(&requested_cache_key);
            self.network
                .remove_stylesheet_cache(&cache_key(&current_url));
        }
        Ok(Some(NativeStylesheetResource {
            url: without_fragment(current_url.as_str()).to_owned(),
            body,
        }))
    }

    pub(crate) async fn load_image_async(
        &mut self,
        document_url: &str,
        src: &str,
    ) -> Result<Option<NativeImage>, NativeEngineError> {
        self.load_image_async_with_object_url_and_referrer_policy(
            document_url,
            src,
            None,
            NativeFetchReferrerPolicy::StrictOriginWhenCrossOrigin,
        )
        .await
    }

    pub(crate) async fn load_image_async_with_object_url(
        &mut self,
        document_url: &str,
        src: &str,
        object_url: Option<&NativeObjectUrlResource>,
    ) -> Result<Option<NativeImage>, NativeEngineError> {
        self.load_image_async_with_object_url_and_referrer_policy(
            document_url,
            src,
            object_url,
            NativeFetchReferrerPolicy::StrictOriginWhenCrossOrigin,
        )
        .await
    }

    pub(crate) async fn load_image_async_with_object_url_and_referrer_policy(
        &mut self,
        document_url: &str,
        src: &str,
        object_url: Option<&NativeObjectUrlResource>,
        referrer_policy: NativeFetchReferrerPolicy,
    ) -> Result<Option<NativeImage>, NativeEngineError> {
        validate_url_text("document URL", document_url)?;
        validate_url_text("image URL", src)?;
        let document_url = Url::parse(without_fragment(document_url)).map_err(|_| {
            NativeEngineError::UnsupportedUrl {
                reason: "image owner URL is not valid HTTP(S) syntax".into(),
            }
        })?;
        if !is_network_url(document_url.as_str()) {
            return Ok(None);
        }
        reject_credentials(&document_url)?;
        let Some(target_url) = resolve_subresource_url_with_blob(&document_url, src)? else {
            return Ok(None);
        };
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
            NativeSubresourceKind::Image,
            &document_url,
            &target_url,
        );
        if !policy.allows(NativeSubresourceKind::Image, &document_url, &target_url) {
            return Ok(None);
        }
        if target_url.scheme().eq_ignore_ascii_case("blob") {
            let Some(object_url) = object_url else {
                return Ok(None);
            };
            NativeOrigin::from_blob_url(without_fragment(target_url.as_str()))?;
            let Some(media_type) = object_url
                .content_type
                .as_deref()
                .and_then(supported_image_media_type_text)
            else {
                return Ok(None);
            };
            return Ok(decode_image_bytes(
                &object_url.body,
                media_type,
                MAX_NATIVE_IMAGE_TRANSFER_BYTES,
            ));
        }
        let requested_cache_key = cache_key(&target_url);
        let stale_cached_image = self
            .network
            .image_cache
            .get(&requested_cache_key)
            .cloned()
            .filter(|cached| !cached.is_fresh(Instant::now()));
        if let Some(cached) = self.network.image_cache.get(&requested_cache_key)
            && cached.is_fresh(Instant::now())
        {
            return Ok(Some(cached.image.clone()));
        }

        let client = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(NATIVE_NETWORK_TIMEOUT)
            .build()
            .map_err(|error| network_error("image client construction", error))?;
        let mut current_url = target_url;
        let mut effective_referrer_policy = referrer_policy;
        let mut request_referrer =
            fetch_referrer_for_target(Some(&document_url), &current_url, effective_referrer_policy);
        let mut redirects = 0;
        let mut pending_cookies = Vec::new();
        let response = loop {
            let mut request_url = current_url.clone();
            request_url.set_fragment(None);
            let mut request = self.apply_environment_headers(client.get(request_url).header(
                reqwest::header::ACCEPT,
                "image/avif,image/webp,image/apng,image/svg+xml,image/jpeg,image/png,image/*;q=0.8, */*;q=0.5",
            ));
            if let Some(referrer) = request_referrer.as_deref() {
                request = request.header(reqwest::header::REFERER, referrer);
            }
            if redirects == 0
                && let Some(cached) = stale_cached_image.as_ref()
            {
                if let Some(etag) = cached.etag.as_deref() {
                    request = request.header(reqwest::header::IF_NONE_MATCH, etag);
                }
                if let Some(last_modified) = cached.last_modified.as_deref() {
                    request = request.header(reqwest::header::IF_MODIFIED_SINCE, last_modified);
                }
            }
            if let Some(cookie) = self.network.cookie_header_for_request(
                &current_url,
                Some(&document_url),
                false,
                NativeNavigationMethod::Get,
            ) {
                request = request.header(reqwest::header::COOKIE, cookie);
            }
            self.before_request(0).await?;
            let response = request
                .send()
                .await
                .map_err(|error| network_error("image subresource request", error))?;
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
                    operation: "image redirect".into(),
                    reason: "image subresource redirect location is not valid URL syntax".into(),
                })?;
            reject_credentials(&next_url)?;
            self.record_report_only_url_violations_for_redirect(
                &policy,
                NativeSubresourceKind::Image,
                &document_url,
                &next_url,
                true,
                None,
            );
            if !is_network_url(without_fragment(next_url.as_str()))
                || !mixed_content_allowed(&document_url, &next_url)
                || !policy.allows_redirect(NativeSubresourceKind::Image, &document_url, &next_url)
            {
                return Ok(None);
            }
            if let Some(policy) = referrer_policy_from_headers(response.headers()) {
                effective_referrer_policy = policy;
            }
            request_referrer = fetch_referrer_for_target(
                Some(&document_url),
                &next_url,
                effective_referrer_policy,
            );
            current_url = next_url;
            redirects += 1;
        };
        let response_headers = response.headers().clone();
        let has_set_cookie = !pending_cookies.is_empty();
        if response.status() == reqwest::StatusCode::NOT_MODIFIED {
            let Some(cached) = stale_cached_image else {
                return Ok(None);
            };
            let cached_image = cached.image.clone();
            if redirects != 0 {
                return Ok(None);
            }
            for (cookie_url, cookie) in pending_cookies {
                self.cookie_changes
                    .extend(self.network.store_cookie(&cookie_url, &cookie));
            }
            if has_set_cookie {
                self.network.remove_image_cache(&requested_cache_key);
            } else if let Some(entry) =
                cached.refresh_from_not_modified(&response_headers, Instant::now())
            {
                self.network
                    .store_image_cache_entry(requested_cache_key, entry);
            } else {
                self.network.remove_image_cache(&requested_cache_key);
            }
            return Ok(Some(cached_image));
        }
        if !response.status().is_success() {
            return Ok(None);
        }
        let Some(media_type) =
            supported_image_media_type(response.headers().get(reqwest::header::CONTENT_TYPE))?
        else {
            return Ok(None);
        };
        let content_length = response.content_length();
        if content_length.is_some_and(|length| length > MAX_NATIVE_IMAGE_TRANSFER_BYTES as u64) {
            return Ok(None);
        }
        let mut stream = response.bytes_stream();
        let mut bytes = Vec::with_capacity(
            content_length
                .unwrap_or_default()
                .min(MAX_NATIVE_IMAGE_TRANSFER_BYTES as u64) as usize,
        );
        while let Some(chunk) = stream.next().await {
            let chunk = chunk.map_err(|error| network_error("image subresource body", error))?;
            self.after_response_chunk(chunk.len()).await;
            let next_len = bytes.len().saturating_add(chunk.len());
            if next_len > MAX_NATIVE_IMAGE_TRANSFER_BYTES {
                return Ok(None);
            }
            bytes.extend_from_slice(&chunk);
        }
        let Some(image) = decode_image_bytes(&bytes, media_type, MAX_NATIVE_IMAGE_TRANSFER_BYTES)
        else {
            return Ok(None);
        };
        for (cookie_url, cookie) in pending_cookies {
            self.cookie_changes
                .extend(self.network.store_cookie(&cookie_url, &cookie));
        }
        if !has_set_cookie
            && let Some(entry) = NativeImageCacheEntry::from_response(
                image.clone(),
                &response_headers,
                Instant::now(),
            )
        {
            self.network
                .store_image_cache_entry(requested_cache_key, entry.clone());
            self.network
                .store_image_cache_entry(cache_key(&current_url), entry);
        } else {
            self.network.remove_image_cache(&requested_cache_key);
            self.network.remove_image_cache(&cache_key(&current_url));
        }
        Ok(Some(image))
    }

    /// Admit a Blob-backed media resource. Blob URLs are resolved through the
    /// caller's JavaScript realm; no network or HTTP cache lookup is performed
    /// for this path.
    pub(crate) fn load_local_blob_media(
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
        let Some(target_url) = resolve_subresource_url_with_blob(&document_url, src)? else {
            return Ok(None);
        };
        if !target_url.scheme().eq_ignore_ascii_case("blob") {
            return Ok(None);
        }
        if is_network_url(document_url.as_str()) {
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
        }
        let Some(object_url) = object_url else {
            return Ok(None);
        };
        NativeOrigin::from_blob_url(without_fragment(target_url.as_str()))?;
        media_metadata_from_bytes(object_url.content_type.as_deref(), &object_url.body)
    }

    /// Admit a file-backed media resource for an explicitly rooted file
    /// document. The file is canonicalized and bounded before MIME metadata
    /// is exposed; no file bytes enter the network or HTTP cache owners.
    pub(crate) fn load_local_file_media(
        &self,
        document_url: &str,
        src: &str,
    ) -> Result<Option<NativeMediaMetadata>, NativeEngineError> {
        validate_url_text("document URL", document_url)?;
        validate_url_text("media URL", src)?;
        let document_url = Url::parse(without_fragment(document_url)).map_err(|_| {
            NativeEngineError::UnsupportedUrl {
                reason: "media owner URL is not valid URL syntax".into(),
            }
        })?;
        reject_credentials(&document_url)?;
        if !is_file_url(document_url.as_str()) {
            return Ok(None);
        }
        let Some(target_url) = resolve_media_url(&document_url, src)? else {
            return Ok(None);
        };
        if !target_url.scheme().eq_ignore_ascii_case("file") {
            return Ok(None);
        }
        let path = match self.allowed_file_path(&target_url, "file media") {
            Ok(path) => path,
            Err(NativeEngineError::UnsupportedUrl { .. }) => return Ok(None),
            Err(error) => return Err(error),
        };
        if !self.rooted_file_subresource_allowed(
            document_url.as_str(),
            &target_url,
            &path,
            NativeSubresourceKind::Media,
            "file media CSP owner URL",
        )? {
            return Ok(None);
        }
        let bytes = read_bounded_file(&path, MAX_NATIVE_MEDIA_BYTES, "file media")?;
        media_metadata_from_bytes(file_media_type(&path), &bytes)
    }

    fn load_file_media_url(
        &self,
        target_url: &Url,
    ) -> Result<Option<NativeMediaMetadata>, NativeEngineError> {
        let path = self.allowed_file_path(target_url, "file media")?;
        let bytes = read_bounded_file(&path, MAX_NATIVE_MEDIA_BYTES, "file media")?;
        media_metadata_from_bytes(file_media_type(&path), &bytes)
    }

    pub(crate) async fn load_media_async(
        &mut self,
        document_url: &str,
        src: &str,
    ) -> Result<Option<NativeMediaMetadata>, NativeEngineError> {
        self.load_media_async_with_object_url(document_url, src, None)
            .await
    }

    /// Load a data URL media resource without entering the network owner.
    /// Embedded bytes remain subject to the same bounded media metadata
    /// admission as Blob and HTTP(S) resources.
    pub(crate) fn load_data_media(
        &self,
        document_url: &str,
        src: &str,
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
        if !target_url.scheme().eq_ignore_ascii_case("data") {
            return Ok(None);
        }
        data_media_metadata(&target_url)
    }

    /// Load a non-network font source selected by a native `@font-face` rule.
    /// The resource is admitted here, where the document URL, file roots,
    /// object-URL registry, and CSP policy are still available; the font
    /// parser only receives bounded bytes.
    pub(crate) fn load_font(
        &mut self,
        document_url: &str,
        src: &str,
        object_url: Option<&NativeObjectUrlResource>,
    ) -> Result<Option<Vec<u8>>, NativeEngineError> {
        validate_url_text("document URL", document_url)?;
        validate_url_text("font URL", src)?;
        let document_url = Url::parse(without_fragment(document_url)).map_err(|_| {
            NativeEngineError::UnsupportedUrl {
                reason: "font owner URL is not valid URL syntax".into(),
            }
        })?;
        reject_credentials(&document_url)?;
        let Some(target_url) = resolve_media_url(&document_url, src)? else {
            return Ok(None);
        };
        if target_url.scheme().eq_ignore_ascii_case("data") {
            if is_network_url(document_url.as_str()) {
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
                    NativeSubresourceKind::Font,
                    &document_url,
                    &target_url,
                );
                if !policy.allows(NativeSubresourceKind::Font, &document_url, &target_url) {
                    return Ok(None);
                }
            }
            return data_font_bytes(&target_url);
        }
        if target_url.scheme().eq_ignore_ascii_case("file") {
            if !is_file_url(document_url.as_str()) {
                return Ok(None);
            }
            let path = self.allowed_file_path(&target_url, "file font")?;
            if !self.rooted_file_subresource_allowed(
                document_url.as_str(),
                &target_url,
                &path,
                NativeSubresourceKind::Font,
                "file font CSP owner URL",
            )? {
                return Ok(None);
            }
            let bytes = read_bounded_file(&path, MAX_NATIVE_FONT_BYTES, "file font")?;
            return Ok((!bytes.is_empty()).then_some(bytes));
        }
        if target_url.scheme().eq_ignore_ascii_case("blob") {
            if is_network_url(document_url.as_str()) {
                let policy = self
                    .network
                    .document_policies
                    .get(&cache_key(&document_url))
                    .cloned()
                    .unwrap_or_default();
                self.record_report_only_url_violations(
                    &policy,
                    NativeSubresourceKind::Font,
                    &document_url,
                    &target_url,
                );
                if !policy.allows(NativeSubresourceKind::Font, &document_url, &target_url) {
                    return Ok(None);
                }
            }
            let Some(object_url) = object_url else {
                return Ok(None);
            };
            NativeOrigin::from_blob_url(without_fragment(target_url.as_str()))?;
            if object_url
                .content_type
                .as_deref()
                .is_some_and(|content_type| !font_content_type_text_allowed(content_type))
            {
                return Ok(None);
            }
            if object_url.body.is_empty() {
                return Ok(None);
            }
            if object_url.body.len() > MAX_NATIVE_FONT_BYTES {
                return Err(NativeEngineError::limit(
                    "Blob font",
                    MAX_NATIVE_FONT_BYTES,
                    object_url.body.len(),
                ));
            }
            return Ok(Some(object_url.body.clone()));
        }
        Ok(None)
    }

    /// Load a font source for an HTTP(S) document while retaining the response
    /// metadata needed by the dynamic `FontFace` fetch path.
    pub(crate) async fn load_font_response_async(
        &mut self,
        document_url: &str,
        src: &str,
        object_url: Option<&NativeObjectUrlResource>,
    ) -> Result<Option<NativeFetchResponse>, NativeEngineError> {
        validate_url_text("document URL", document_url)?;
        validate_url_text("font URL", src)?;
        let document_url = Url::parse(without_fragment(document_url)).map_err(|_| {
            NativeEngineError::UnsupportedUrl {
                reason: "font owner URL is not valid URL syntax".into(),
            }
        })?;
        reject_credentials(&document_url)?;
        let Some(target_url) = resolve_media_url(&document_url, src)? else {
            return Ok(None);
        };
        if is_network_url(target_url.as_str()) {
            return self
                .load_network_font_response_async(&document_url, &target_url)
                .await;
        }
        let body = self.load_font(document_url.as_str(), src, object_url)?;
        Ok(body.map(|body| NativeFetchResponse {
            url: without_fragment(target_url.as_str()).to_owned(),
            status: 200,
            status_text: "OK".into(),
            content_type: target_url
                .scheme()
                .eq_ignore_ascii_case("blob")
                .then(|| object_url.and_then(|resource| resource.content_type.clone()))
                .flatten(),
            headers: Vec::new(),
            body,
            redirected: false,
            opaque: false,
            opaque_redirect: false,
        }))
    }

    /// Load a font source for an HTTP(S) document. Local sources reuse the
    /// synchronous owner above; network sources use a bounded redirect and
    /// response loop with font-specific CSP and CORS checks.
    pub(crate) async fn load_font_async(
        &mut self,
        document_url: &str,
        src: &str,
        object_url: Option<&NativeObjectUrlResource>,
    ) -> Result<Option<Vec<u8>>, NativeEngineError> {
        self.load_font_response_async(document_url, src, object_url)
            .await
            .map(|response| response.map(|response| response.body))
    }

    async fn load_network_font_response_async(
        &mut self,
        document_url: &Url,
        target_url: &Url,
    ) -> Result<Option<NativeFetchResponse>, NativeEngineError> {
        if !is_network_url(document_url.as_str())
            || !is_network_url(without_fragment(target_url.as_str()))
            || !mixed_content_allowed(document_url, target_url)
        {
            return Ok(None);
        }
        let policy = self
            .network
            .document_policies
            .get(&cache_key(document_url))
            .cloned()
            .unwrap_or_default();
        self.record_report_only_url_violations(
            &policy,
            NativeSubresourceKind::Font,
            document_url,
            target_url,
        );
        if !policy.allows(NativeSubresourceKind::Font, document_url, target_url) {
            return Ok(None);
        }
        let mut requested_url = target_url.clone();
        requested_url.set_fragment(None);
        let request_cookie = self.network.cookie_header_for_request(
            &requested_url,
            Some(document_url),
            false,
            NativeNavigationMethod::Get,
        );
        let requested_cache_key =
            font_response_cache_key(document_url, &requested_url, request_cookie.as_deref());
        let cached_font = self
            .network
            .font_cache
            .get(&requested_cache_key)
            .cloned()
            .filter(|cached| cached.body.len() <= MAX_NATIVE_FONT_BYTES);
        if let Some(cached) = cached_font.as_ref().filter(|cached| {
            cached.is_fresh(Instant::now())
                && Url::parse(&cached.url).is_ok_and(|cached_url| {
                    is_network_url(cached_url.as_str())
                        && mixed_content_allowed(document_url, &cached_url)
                        && policy.allows(NativeSubresourceKind::Font, document_url, &cached_url)
                })
        }) {
            return Ok(Some(cached.response()));
        }
        let stale_cached_font = cached_font.filter(|cached| {
            !cached.is_fresh(Instant::now())
                && Url::parse(&cached.url).is_ok_and(|cached_url| {
                    is_network_url(cached_url.as_str())
                        && mixed_content_allowed(document_url, &cached_url)
                        && policy.allows(NativeSubresourceKind::Font, document_url, &cached_url)
                })
        });
        let client = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(NATIVE_NETWORK_TIMEOUT)
            .build()
            .map_err(|error| network_error("font client construction", error))?;
        let mut current_url = requested_url;
        current_url.set_fragment(None);
        let mut request_referrer = normalize_referrer(Some(document_url.as_str()), &current_url)?;
        let mut redirects = 0;
        let mut pending_cookies = Vec::new();
        let response = loop {
            let mut request_url = current_url.clone();
            request_url.set_fragment(None);
            let mut request = self.apply_environment_headers(client.get(request_url).header(
                reqwest::header::ACCEPT,
                "font/woff2,font/woff,font/otf,font/ttf,application/font-woff,*/*;q=0.1",
            ));
            if let Some(referrer) = request_referrer.as_deref() {
                request = request.header(reqwest::header::REFERER, referrer);
            }
            if redirects == 0
                && let Some(cached) = stale_cached_font.as_ref()
            {
                if let Some(etag) = cached.etag.as_deref() {
                    request = request.header(reqwest::header::IF_NONE_MATCH, etag);
                }
                if let Some(last_modified) = cached.last_modified.as_deref() {
                    request = request.header(reqwest::header::IF_MODIFIED_SINCE, last_modified);
                }
            }
            if current_url.origin() == document_url.origin()
                && let Some(cookie) = self.network.cookie_header_for_request(
                    &current_url,
                    Some(document_url),
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
                .map_err(|error| network_error("font subresource request", error))?;
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
                    operation: "font redirect".into(),
                    reason: "font redirect location is not valid URL syntax".into(),
                })?;
            reject_credentials(&next_url)?;
            self.record_report_only_url_violations_for_redirect(
                &policy,
                NativeSubresourceKind::Font,
                document_url,
                &next_url,
                true,
                None,
            );
            if !is_network_url(without_fragment(next_url.as_str()))
                || !mixed_content_allowed(document_url, &next_url)
                || !policy.allows_redirect(NativeSubresourceKind::Font, document_url, &next_url)
            {
                return Ok(None);
            }
            request_referrer = normalize_referrer(Some(current_url.as_str()), &next_url)?;
            current_url = next_url;
            redirects += 1;
        };
        let response_headers = response.headers().clone();
        let has_set_cookie = !pending_cookies.is_empty();
        if response.status() == reqwest::StatusCode::NOT_MODIFIED {
            let Some(cached) = stale_cached_font else {
                return Ok(None);
            };
            if redirects != 0 {
                return Ok(None);
            }
            for (cookie_url, cookie) in pending_cookies {
                self.cookie_changes
                    .extend(self.network.store_cookie(&cookie_url, &cookie));
            }
            let cached_response = cached.response();
            if has_set_cookie {
                self.network.remove_font_cache(&requested_cache_key);
            } else if let Some(entry) =
                cached.refresh_from_not_modified(&response_headers, Instant::now())
            {
                self.network.store_font_cache(requested_cache_key, entry);
            } else {
                self.network.remove_font_cache(&requested_cache_key);
            }
            return Ok(Some(cached_response));
        }
        if !response.status().is_success() {
            return Ok(None);
        }
        if document_url.origin() != current_url.origin()
            && !cors_response_allowed(response.headers(), document_url, &current_url, false)
        {
            return Ok(None);
        }
        if response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .is_some_and(|value| !font_content_type_text_allowed(value))
        {
            return Ok(None);
        }
        if response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .is_some_and(|value| value.to_str().is_err())
        {
            return Ok(None);
        }
        let content_length = response.content_length();
        if content_length.is_some_and(|length| length > MAX_NATIVE_FONT_BYTES as u64) {
            return Ok(None);
        }
        let response_status = response.status();
        let response_status_text = response_status
            .canonical_reason()
            .unwrap_or_default()
            .to_owned();
        let response_content_type = response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .map(str::to_owned);
        let exposed_headers = exposed_response_headers(
            &response_headers,
            document_url.origin() == current_url.origin(),
            false,
        )?;
        let response_url = without_fragment(current_url.as_str()).to_owned();
        let redirected = redirects != 0;
        let mut stream = response.bytes_stream();
        let mut bytes = Vec::with_capacity(
            content_length
                .unwrap_or_default()
                .min(MAX_NATIVE_FONT_BYTES as u64) as usize,
        );
        while let Some(chunk) = stream.next().await {
            let chunk = chunk.map_err(|error| network_error("font subresource body", error))?;
            self.after_response_chunk(chunk.len()).await;
            let next_len = bytes.len().saturating_add(chunk.len());
            if next_len > MAX_NATIVE_FONT_BYTES {
                return Ok(None);
            }
            bytes.extend_from_slice(&chunk);
        }
        for (cookie_url, cookie) in pending_cookies {
            self.cookie_changes
                .extend(self.network.store_cookie(&cookie_url, &cookie));
        }
        let response = NativeFetchResponse {
            url: response_url,
            status: response_status.as_u16(),
            status_text: response_status_text,
            content_type: response_content_type,
            headers: exposed_headers,
            body: bytes,
            redirected,
            opaque: false,
            opaque_redirect: false,
        };
        if !has_set_cookie && response_status != reqwest::StatusCode::PARTIAL_CONTENT {
            if let Some(entry) =
                NativeFontCacheEntry::from_response(&response, &response_headers, Instant::now())
            {
                self.network.store_font_cache(requested_cache_key, entry);
            } else {
                self.network.remove_font_cache(&requested_cache_key);
            }
        } else {
            self.network.remove_font_cache(&requested_cache_key);
        }
        if response.body.is_empty() {
            return Ok(None);
        }
        Ok(Some(response))
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
            ));
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

    /// Load a Blob-backed classic script for a local document without entering
    /// the asynchronous network loader. The JavaScript runtime remains the
    /// registry authority; this method only validates and copies the bounded
    /// bytes needed by the local script turn.
    pub(crate) fn load_local_blob_script(
        &mut self,
        document_url: &str,
        href: &str,
        max_source_bytes: usize,
        integrity: Option<&str>,
        object_url: Option<&NativeObjectUrlResource>,
    ) -> Result<Option<NativeScriptResource>, NativeEngineError> {
        validate_url_text("document URL", document_url)?;
        validate_url_text("script URL", href)?;
        if max_source_bytes == 0 {
            return Err(NativeEngineError::invalid(
                "script source limit",
                "must be positive",
            ));
        }
        let document_url = Url::parse(without_fragment(document_url)).map_err(|_| {
            NativeEngineError::UnsupportedUrl {
                reason: "script owner URL is not valid URL syntax".into(),
            }
        })?;
        reject_credentials(&document_url)?;
        if is_network_url(document_url.as_str()) {
            return Ok(None);
        }
        let Some(target_url) = resolve_subresource_url_with_blob(&document_url, href)? else {
            return Ok(None);
        };
        if !target_url.scheme().eq_ignore_ascii_case("blob") {
            return Ok(None);
        }
        let Some(object_url) = object_url else {
            return Ok(None);
        };
        NativeOrigin::from_blob_url(without_fragment(target_url.as_str()))?;
        if object_url
            .content_type
            .as_deref()
            .is_some_and(|content_type| {
                !content_type.is_empty() && !javascript_mime_essence_allowed(content_type)
            })
        {
            return Ok(None);
        }
        if object_url.body.len() > max_source_bytes {
            return Err(NativeEngineError::limit(
                "Blob script subresource",
                max_source_bytes,
                object_url.body.len(),
            ));
        }
        if !subresource_integrity_matches(integrity, &object_url.body) {
            return Ok(None);
        }
        let body =
            String::from_utf8(object_url.body.clone()).map_err(|_| NativeEngineError::Network {
                operation: "Blob script subresource decoding".into(),
                reason: "Blob script subresource is not valid UTF-8".into(),
            })?;
        Ok(Some(NativeScriptResource {
            url: target_url.to_string(),
            body,
            response_referrer_policy: None,
        }))
    }

    /// Load a bounded rooted file script for a local file document. File
    /// scripts are text resources owned by the configured root; they do not
    /// enter the HTTP cache or network transport.
    pub(crate) fn load_local_file_script(
        &self,
        document_url: &str,
        href: &str,
        max_source_bytes: usize,
        integrity: Option<&str>,
    ) -> Result<Option<NativeScriptResource>, NativeEngineError> {
        self.load_local_file_script_internal(document_url, href, max_source_bytes, integrity, None)
    }

    pub(crate) fn load_local_file_script_with_policy(
        &self,
        document_url: &str,
        href: &str,
        max_source_bytes: usize,
        parser_inserted: bool,
        nonce: Option<&str>,
        integrity: Option<&str>,
    ) -> Result<Option<NativeScriptResource>, NativeEngineError> {
        self.load_local_file_script_internal(
            document_url,
            href,
            max_source_bytes,
            integrity,
            Some((parser_inserted, nonce)),
        )
    }

    fn load_local_file_script_internal(
        &self,
        document_url: &str,
        href: &str,
        max_source_bytes: usize,
        integrity: Option<&str>,
        script_metadata: Option<(bool, Option<&str>)>,
    ) -> Result<Option<NativeScriptResource>, NativeEngineError> {
        validate_url_text("document URL", document_url)?;
        validate_url_text("script URL", href)?;
        if max_source_bytes == 0 {
            return Err(NativeEngineError::invalid(
                "script source limit",
                "must be positive",
            ));
        }
        let Some((target_url, path)) =
            self.local_file_subresource_path(document_url, href, "file script")?
        else {
            return Ok(None);
        };
        if let Some((parser_inserted, nonce)) = script_metadata {
            let Some((document_url, _)) =
                self.csp_document_owner(document_url, "file script CSP owner URL")?
            else {
                return Ok(None);
            };
            let Some(resource_root) = self.allowed_file_root_for_path(&path) else {
                return Ok(None);
            };
            let policy = self
                .network
                .document_policies
                .get(&cache_key(&document_url))
                .cloned()
                .unwrap_or_default();
            if !policy.allows_rooted_file_script(
                &document_url,
                &target_url,
                parser_inserted,
                nonce,
                Some(resource_root),
            ) {
                return Ok(None);
            }
        }
        let bytes = read_bounded_file(&path, max_source_bytes, "file script subresource")?;
        if !subresource_integrity_matches(integrity, &bytes) {
            return Ok(None);
        }
        let body = String::from_utf8(bytes).map_err(|_| NativeEngineError::Network {
            operation: "file script subresource decoding".into(),
            reason: "file script subresource is not valid UTF-8".into(),
        })?;
        Ok(Some(NativeScriptResource {
            url: target_url.to_string(),
            body,
            response_referrer_policy: None,
        }))
    }

    pub(crate) async fn load_script_async_with_metadata_and_object_url(
        &mut self,
        document_url: &str,
        href: &str,
        max_source_bytes: usize,
        parser_inserted: bool,
        nonce: Option<&str>,
        integrity: Option<&str>,
        crossorigin: Option<&str>,
        object_url: Option<&NativeObjectUrlResource>,
        referrer_policy: Option<NativeFetchReferrerPolicy>,
    ) -> Result<Option<NativeScriptResource>, NativeEngineError> {
        self.load_script_like_async(
            document_url,
            href,
            max_source_bytes,
            NativeSubresourceKind::Script,
            parser_inserted,
            nonce,
            integrity,
            crossorigin,
            object_url,
            None,
            None,
            referrer_policy,
        )
        .await
    }

    pub(crate) async fn load_module_dependency_with_referrer_async(
        &mut self,
        document_url: &str,
        referrer_url: &str,
        href: &str,
        max_source_bytes: usize,
        parser_inserted: bool,
        integrity: Option<&str>,
        crossorigin: Option<&str>,
        object_url: Option<&NativeObjectUrlResource>,
        module_type: NativeModuleResourceType,
        referrer_policy: Option<NativeFetchReferrerPolicy>,
    ) -> Result<Option<NativeScriptResource>, NativeEngineError> {
        self.load_script_like_async_with_referrer_source(
            document_url,
            href,
            max_source_bytes,
            NativeSubresourceKind::Script,
            parser_inserted,
            None,
            integrity,
            crossorigin,
            object_url,
            Some(module_type),
            None,
            referrer_policy,
            Some(referrer_url),
        )
        .await
    }

    pub(crate) async fn load_worker_async(
        &mut self,
        document_url: &str,
        href: &str,
        max_source_bytes: usize,
    ) -> Result<Option<NativeScriptResource>, NativeEngineError> {
        self.load_worker_async_with_referrer_policy(document_url, href, max_source_bytes, None)
            .await
    }

    pub(crate) async fn load_worker_async_with_referrer_policy(
        &mut self,
        document_url: &str,
        href: &str,
        max_source_bytes: usize,
        referrer_policy: Option<NativeFetchReferrerPolicy>,
    ) -> Result<Option<NativeScriptResource>, NativeEngineError> {
        self.load_script_like_async(
            document_url,
            href,
            max_source_bytes,
            NativeSubresourceKind::Worker,
            true,
            None,
            None,
            None,
            None,
            None,
            None,
            referrer_policy,
        )
        .await
    }

    pub(crate) async fn load_worker_async_with_referrer_source(
        &mut self,
        document_url: &str,
        referrer_url: &str,
        href: &str,
        max_source_bytes: usize,
        referrer_policy: Option<NativeFetchReferrerPolicy>,
    ) -> Result<Option<NativeScriptResource>, NativeEngineError> {
        self.load_script_like_async_with_referrer_source(
            document_url,
            href,
            max_source_bytes,
            NativeSubresourceKind::Worker,
            true,
            None,
            None,
            None,
            None,
            None,
            None,
            referrer_policy,
            Some(referrer_url),
        )
        .await
    }

    pub(crate) async fn load_shared_worker_module_async(
        &mut self,
        document_url: &str,
        href: &str,
        max_source_bytes: usize,
        credentials_mode: &str,
        referrer_policy: Option<NativeFetchReferrerPolicy>,
    ) -> Result<Option<NativeScriptResource>, NativeEngineError> {
        let crossorigin = worker_module_crossorigin(credentials_mode)?;
        self.load_script_like_async(
            document_url,
            href,
            max_source_bytes,
            NativeSubresourceKind::Worker,
            true,
            None,
            None,
            Some(crossorigin),
            None,
            Some(NativeModuleResourceType::JavaScript),
            Some(credentials_mode),
            referrer_policy,
        )
        .await
    }

    pub(crate) async fn load_worker_script_dependency_async_with_credentials(
        &mut self,
        document_url: &str,
        href: &str,
        max_source_bytes: usize,
        module_type: Option<NativeModuleResourceType>,
        credentials_mode: Option<&str>,
    ) -> Result<Option<NativeScriptResource>, NativeEngineError> {
        self.load_worker_script_dependency_async_with_referrer_source(
            document_url,
            document_url,
            href,
            max_source_bytes,
            module_type,
            credentials_mode,
            None,
        )
        .await
    }

    pub(crate) async fn load_worker_script_dependency_async_with_referrer_source(
        &mut self,
        document_url: &str,
        referrer_url: &str,
        href: &str,
        max_source_bytes: usize,
        module_type: Option<NativeModuleResourceType>,
        credentials_mode: Option<&str>,
        referrer_policy: Option<NativeFetchReferrerPolicy>,
    ) -> Result<Option<NativeScriptResource>, NativeEngineError> {
        validate_url_text("document URL", document_url)?;
        validate_url_text("worker script referrer URL", referrer_url)?;
        validate_url_text("script URL", href)?;
        let crossorigin = credentials_mode
            .map(worker_module_crossorigin)
            .transpose()?;
        if max_source_bytes == 0 {
            return Err(NativeEngineError::invalid(
                "script source limit",
                "must be positive",
            ));
        }
        if let Ok(owner_url) = Url::parse(without_fragment(referrer_url))
            && owner_url.scheme().eq_ignore_ascii_case("fixture")
        {
            let target_url = Url::parse(href)
                .or_else(|_| owner_url.join(href))
                .map_err(|_| NativeEngineError::UnsupportedUrl {
                    reason: "worker script URL could not be resolved against its owner".into(),
                })?;
            if !target_url.scheme().eq_ignore_ascii_case("fixture") {
                return Ok(None);
            }
            let resource = self.load(without_fragment(target_url.as_str()))?;
            if resource.body.len() > max_source_bytes {
                return Err(NativeEngineError::limit(
                    "worker script dependency",
                    max_source_bytes,
                    resource.body.len(),
                ));
            }
            return Ok(Some(NativeScriptResource {
                url: resource.url,
                body: resource.body,
                response_referrer_policy: None,
            }));
        }
        self.load_script_like_async_with_referrer_source(
            document_url,
            href,
            max_source_bytes,
            NativeSubresourceKind::Script,
            true,
            None,
            None,
            crossorigin,
            None,
            module_type,
            credentials_mode,
            referrer_policy,
            Some(referrer_url),
        )
        .await
    }

    async fn load_script_like_async(
        &mut self,
        document_url: &str,
        href: &str,
        max_source_bytes: usize,
        subresource_kind: NativeSubresourceKind,
        parser_inserted: bool,
        nonce: Option<&str>,
        integrity: Option<&str>,
        crossorigin: Option<&str>,
        object_url: Option<&NativeObjectUrlResource>,
        module_type: Option<NativeModuleResourceType>,
        worker_credentials_mode: Option<&str>,
        referrer_policy: Option<NativeFetchReferrerPolicy>,
    ) -> Result<Option<NativeScriptResource>, NativeEngineError> {
        self.load_script_like_async_with_referrer_source(
            document_url,
            href,
            max_source_bytes,
            subresource_kind,
            parser_inserted,
            nonce,
            integrity,
            crossorigin,
            object_url,
            module_type,
            worker_credentials_mode,
            referrer_policy,
            None,
        )
        .await
    }

    async fn load_script_like_async_with_referrer_source(
        &mut self,
        document_url: &str,
        href: &str,
        max_source_bytes: usize,
        subresource_kind: NativeSubresourceKind,
        parser_inserted: bool,
        nonce: Option<&str>,
        integrity: Option<&str>,
        crossorigin: Option<&str>,
        object_url: Option<&NativeObjectUrlResource>,
        module_type: Option<NativeModuleResourceType>,
        worker_credentials_mode: Option<&str>,
        referrer_policy: Option<NativeFetchReferrerPolicy>,
        referrer_source_url: Option<&str>,
    ) -> Result<Option<NativeScriptResource>, NativeEngineError> {
        if let Some(mode) = worker_credentials_mode
            && !matches!(mode, "omit" | "same-origin" | "include")
        {
            return Err(NativeEngineError::invalid(
                "SharedWorker credentials mode",
                "must be omit, same-origin, or include",
            ));
        }
        validate_url_text("document URL", document_url)?;
        validate_url_text(
            if subresource_kind == NativeSubresourceKind::Worker {
                "worker URL"
            } else {
                "script URL"
            },
            href,
        )?;
        if max_source_bytes == 0 {
            return Err(NativeEngineError::invalid(
                "script source limit",
                "must be positive",
            ));
        }
        let document_url = Url::parse(without_fragment(document_url)).map_err(|_| {
            NativeEngineError::UnsupportedUrl {
                reason: "script owner URL is not valid URL syntax".into(),
            }
        })?;
        reject_credentials(&document_url)?;
        let target_url = if subresource_kind == NativeSubresourceKind::Worker
            && !is_network_url(document_url.as_str())
        {
            let target_url = Url::parse(href)
                .or_else(|_| document_url.join(href))
                .map_err(|_| NativeEngineError::UnsupportedUrl {
                    reason: "worker URL could not be resolved against the document".into(),
                })?;
            (target_url.scheme().eq_ignore_ascii_case("fixture")
                || (target_url.scheme().eq_ignore_ascii_case("file")
                    && document_url.scheme().eq_ignore_ascii_case("file")))
            .then_some(target_url)
        } else {
            resolve_subresource_url_with_blob(&document_url, href)?
        };
        let Some(target_url) = target_url else {
            return Ok(None);
        };
        if !is_network_url(document_url.as_str()) {
            if subresource_kind == NativeSubresourceKind::Worker
                && target_url.scheme().eq_ignore_ascii_case("fixture")
            {
                let resource = self.load(target_url.as_str())?;
                if resource.body.len() > max_source_bytes {
                    return Err(NativeEngineError::limit(
                        "worker source",
                        max_source_bytes,
                        resource.body.len(),
                    ));
                }
                return Ok(Some(NativeScriptResource {
                    url: resource.url,
                    body: resource.body,
                    response_referrer_policy: None,
                }));
            }
            if subresource_kind == NativeSubresourceKind::Worker
                && target_url.scheme().eq_ignore_ascii_case("file")
            {
                return self.load_local_file_script(
                    document_url.as_str(),
                    target_url.as_str(),
                    max_source_bytes,
                    None,
                );
            }
            return Ok(None);
        }
        if !mixed_content_allowed(&document_url, &target_url) {
            return Ok(None);
        }
        let integrity_required = integrity_metadata_is_enforced(integrity);
        let cross_origin_target = document_url.origin() != target_url.origin();
        let can_reuse_cached_script = worker_credentials_mode.is_none()
            && !(cross_origin_target && (integrity_required || crossorigin.is_some()));
        let policy = self
            .network
            .document_policies
            .get(&cache_key(&document_url))
            .cloned()
            .unwrap_or_default();
        self.record_report_only_url_violations_with_metadata(
            &policy,
            subresource_kind,
            &document_url,
            &target_url,
            parser_inserted,
            nonce,
        );
        let allowed = if subresource_kind == NativeSubresourceKind::Script {
            policy.allows_script(&document_url, &target_url, parser_inserted, nonce)
        } else {
            policy.allows(subresource_kind, &document_url, &target_url)
        };
        if !allowed {
            return Ok(None);
        }
        if target_url.scheme().eq_ignore_ascii_case("blob") {
            if subresource_kind != NativeSubresourceKind::Script {
                return Ok(None);
            }
            let Some(object_url) = object_url else {
                return Ok(None);
            };
            NativeOrigin::from_blob_url(without_fragment(target_url.as_str()))?;
            let content_type_allowed = match module_type {
                Some(module_type) => module_content_type_text_allowed(
                    module_type,
                    object_url.content_type.as_deref(),
                ),
                None => object_url
                    .content_type
                    .as_deref()
                    .is_none_or(|content_type| {
                        content_type.is_empty() || javascript_mime_essence_allowed(content_type)
                    }),
            };
            if !content_type_allowed {
                return Ok(None);
            }
            if object_url.body.len() > max_source_bytes {
                return Err(NativeEngineError::limit(
                    "Blob script subresource",
                    max_source_bytes,
                    object_url.body.len(),
                ));
            }
            if !subresource_integrity_matches(integrity, &object_url.body) {
                return Ok(None);
            }
            let body = String::from_utf8(object_url.body.clone()).map_err(|_| {
                NativeEngineError::Network {
                    operation: "Blob script subresource decoding".into(),
                    reason: "Blob script subresource is not valid UTF-8".into(),
                }
            })?;
            return Ok(Some(NativeScriptResource {
                url: target_url.to_string(),
                body,
                response_referrer_policy: None,
            }));
        }
        let requested_cache_key = cache_key(&target_url);
        let stale_cached_script = (subresource_kind == NativeSubresourceKind::Script)
            .then(|| {
                self.network
                    .script_cache
                    .get(&requested_cache_key)
                    .cloned()
                    .filter(|cached| {
                        can_reuse_cached_script
                            && !cached.is_fresh(Instant::now())
                            && cached_resource_content_type_text_allowed(
                                module_type,
                                cached.content_type.as_deref(),
                            )
                    })
            })
            .flatten();
        if subresource_kind == NativeSubresourceKind::Script
            && can_reuse_cached_script
            && let Some(cached) = self.network.script_cache.get(&requested_cache_key)
            && cached.is_fresh(Instant::now())
            && cached_resource_content_type_text_allowed(
                module_type,
                cached.content_type.as_deref(),
            )
        {
            return if subresource_integrity_matches(integrity, cached.body.as_bytes()) {
                Ok(Some(NativeScriptResource {
                    url: cached.url.clone(),
                    body: cached.body.clone(),
                    response_referrer_policy: cached.referrer_policy,
                }))
            } else {
                Ok(None)
            };
        }

        let client = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(NATIVE_NETWORK_TIMEOUT)
            .build()
            .map_err(|error| network_error("script client construction", error))?;
        let mut current_url = target_url;
        let mut effective_referrer_policy = referrer_policy;
        let initial_referrer_source_url = match referrer_source_url {
            Some(source) => Url::parse(without_fragment(source)).map_err(|_| {
                NativeEngineError::UnsupportedUrl {
                    reason: "module referrer URL is not valid URL syntax".into(),
                }
            })?,
            None => document_url.clone(),
        };
        let mut request_referrer = match effective_referrer_policy {
            Some(policy) => {
                fetch_referrer_for_target(Some(&initial_referrer_source_url), &current_url, policy)
            }
            None => normalize_referrer(Some(initial_referrer_source_url.as_str()), &current_url)?,
        };
        let mut redirects = 0;
        let mut has_set_cookie = false;
        let response = loop {
            let mut request_url = current_url.clone();
            request_url.set_fragment(None);
            let accept = match module_type {
                Some(NativeModuleResourceType::Json) => "application/json, text/json, */*",
                Some(
                    NativeModuleResourceType::JavaScript | NativeModuleResourceType::Unsupported,
                )
                | None => "text/javascript, application/javascript, application/ecmascript, */*",
            };
            let mut request = self.apply_environment_headers(
                client
                    .get(request_url)
                    .header(reqwest::header::ACCEPT, accept),
            );
            if let Some(referrer) = request_referrer.as_deref() {
                request = request.header(reqwest::header::REFERER, referrer);
            }
            if crossorigin.is_some()
                && let Some(origin) =
                    cors_origin_header(&document_url, &current_url, NativeCorsMode::Cors)
            {
                request = request.header(reqwest::header::ORIGIN, origin);
            }
            if redirects == 0
                && let Some(cached) = stale_cached_script.as_ref()
            {
                if let Some(etag) = cached.etag.as_deref() {
                    request = request.header(reqwest::header::IF_NONE_MATCH, etag);
                }
                if let Some(last_modified) = cached.last_modified.as_deref() {
                    request = request.header(reqwest::header::IF_MODIFIED_SINCE, last_modified);
                }
            }
            let credentials = subresource_request_has_credentials(
                worker_credentials_mode,
                crossorigin,
                &document_url,
                &current_url,
            );
            if credentials
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
                .map_err(|error| network_error("script subresource request", error))?;
            if credentials {
                for value in response
                    .headers()
                    .get_all(reqwest::header::SET_COOKIE)
                    .iter()
                {
                    if let Ok(cookie) = value.to_str() {
                        has_set_cookie = true;
                        self.cookie_changes
                            .extend(self.network.store_cookie(&current_url, cookie));
                    }
                }
            }
            if !is_http_redirect(response.status()) {
                break response;
            }
            if let Some(policy) = referrer_policy_from_headers(response.headers())
                && effective_referrer_policy.is_some()
            {
                effective_referrer_policy = Some(policy);
            }
            if redirects >= MAX_NATIVE_NETWORK_REDIRECTS {
                return Err(NativeEngineError::Network {
                    operation: "script redirect".into(),
                    reason: "script subresource redirect chain exceeded the native limit".into(),
                });
            }
            let location = response
                .headers()
                .get(reqwest::header::LOCATION)
                .and_then(|value| value.to_str().ok())
                .filter(|value| !value.is_empty())
                .ok_or_else(|| NativeEngineError::Network {
                    operation: "script redirect".into(),
                    reason: "script subresource redirect did not provide a valid location".into(),
                })?;
            let next_url = current_url
                .join(location)
                .map_err(|_| NativeEngineError::Network {
                    operation: "script redirect".into(),
                    reason: "script subresource redirect location is not valid URL syntax".into(),
                })?;
            reject_credentials(&next_url)?;
            self.record_report_only_url_violations_for_redirect(
                &policy,
                subresource_kind,
                &document_url,
                &next_url,
                parser_inserted,
                nonce,
            );
            let allowed = if subresource_kind == NativeSubresourceKind::Script {
                policy.allows_script_redirect(&document_url, &next_url, parser_inserted, nonce)
            } else {
                policy.allows_redirect(subresource_kind, &document_url, &next_url)
            };
            if !is_network_url(without_fragment(next_url.as_str()))
                || !mixed_content_allowed(&document_url, &next_url)
                || !allowed
            {
                return Ok(None);
            }
            let next_referrer_source_url = if referrer_source_url.is_some() {
                initial_referrer_source_url.as_str()
            } else {
                current_url.as_str()
            };
            request_referrer = match effective_referrer_policy {
                Some(policy) => {
                    fetch_referrer_for_target(Some(&initial_referrer_source_url), &next_url, policy)
                }
                None => normalize_referrer(Some(next_referrer_source_url), &next_url)?,
            };
            current_url = next_url;
            redirects += 1;
        };
        let response_headers = response.headers().clone();
        if response.status() == reqwest::StatusCode::NOT_MODIFIED {
            let Some(cached) = stale_cached_script else {
                return Err(NativeEngineError::Network {
                    operation: "script subresource revalidation".into(),
                    reason: "server returned HTTP 304 without a stale cached script".into(),
                });
            };
            if redirects != 0 {
                return Err(NativeEngineError::Network {
                    operation: "script subresource revalidation".into(),
                    reason: "HTTP 304 was received after a script redirect".into(),
                });
            }
            let cached_resource = NativeScriptResource {
                url: cached.url.clone(),
                body: cached.body.clone(),
                response_referrer_policy: if response_headers.contains_key("referrer-policy") {
                    referrer_policy_from_headers(&response_headers)
                } else {
                    cached.referrer_policy
                },
            };
            if !cached_resource_content_type_text_allowed(
                module_type,
                cached.content_type.as_deref(),
            ) {
                return Ok(None);
            }
            if !subresource_integrity_matches(integrity, cached_resource.body.as_bytes()) {
                return Ok(None);
            }
            if has_set_cookie {
                self.network.remove_script_cache(&requested_cache_key);
            } else if let Some(entry) =
                cached.refresh_from_not_modified(&response_headers, Instant::now())
            {
                self.network.store_script_cache(requested_cache_key, entry);
            } else {
                self.network.remove_script_cache(&requested_cache_key);
            }
            return Ok(Some(cached_resource));
        }
        if !response.status().is_success() {
            return Err(NativeEngineError::Network {
                operation: "script subresource request".into(),
                reason: format!("server returned HTTP {}", response.status().as_u16()),
            });
        }
        if !subresource_response_allowed(
            &response_headers,
            &document_url,
            &current_url,
            integrity,
            crossorigin,
        ) {
            return Ok(None);
        }
        let content_type_allowed = match module_type {
            Some(module_type) => module_content_type_allowed(
                module_type,
                response.headers().get(reqwest::header::CONTENT_TYPE),
            )?,
            None => {
                script_content_type_allowed(response.headers().get(reqwest::header::CONTENT_TYPE))?
            }
        };
        if !content_type_allowed {
            return Ok(None);
        }
        let content_length = response.content_length();
        if content_length.is_some_and(|length| length > max_source_bytes as u64) {
            return Err(NativeEngineError::limit(
                "script subresource",
                max_source_bytes,
                content_length
                    .and_then(|length| usize::try_from(length).ok())
                    .unwrap_or(usize::MAX),
            ));
        }
        let mut stream = response.bytes_stream();
        let mut bytes = Vec::with_capacity(
            content_length
                .unwrap_or_default()
                .min(max_source_bytes as u64) as usize,
        );
        while let Some(chunk) = stream.next().await {
            let chunk = chunk.map_err(|error| network_error("script subresource body", error))?;
            self.after_response_chunk(chunk.len()).await;
            let next_len = bytes.len().saturating_add(chunk.len());
            if next_len > max_source_bytes {
                return Err(NativeEngineError::limit(
                    "script subresource",
                    max_source_bytes,
                    next_len,
                ));
            }
            bytes.extend_from_slice(&chunk);
        }
        if !subresource_integrity_matches(integrity, &bytes) {
            return Ok(None);
        }
        let body = String::from_utf8(bytes).map_err(|_| NativeEngineError::Network {
            operation: "script subresource decoding".into(),
            reason: "script subresource is not valid UTF-8".into(),
        })?;
        if subresource_kind == NativeSubresourceKind::Worker {
            self.network.store_document_policy(
                cache_key(&current_url),
                content_security_policy(&response_headers),
            );
        }
        let resource = NativeScriptResource {
            url: current_url.to_string(),
            body,
            response_referrer_policy: referrer_policy_from_headers(&response_headers),
        };
        if subresource_kind == NativeSubresourceKind::Script && worker_credentials_mode.is_none() {
            if !has_set_cookie
                && let Some(entry) = NativeTextCacheEntry::from_response(
                    resource.url.clone(),
                    resource.body.clone(),
                    &response_headers,
                    Instant::now(),
                )
            {
                self.network
                    .store_script_cache(requested_cache_key, entry.clone());
                self.network
                    .store_script_cache(cache_key(&current_url), entry);
            } else {
                self.network.remove_script_cache(&requested_cache_key);
                self.network.remove_script_cache(&cache_key(&current_url));
            }
        }
        Ok(Some(resource))
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

fn content_security_policy(headers: &HeaderMap) -> NativeCspPolicy {
    let mut policy = NativeCspPolicy::default();
    for value in headers
        .get_all(reqwest::header::CONTENT_SECURITY_POLICY)
        .iter()
    {
        let Ok(value) = value.to_str() else {
            // Preserve the historical fail-closed treatment of an invalid
            // header value without widening it into a new transport error.
            policy.policies.push(NativeCspDirectives {
                style_sources: Some(Vec::new()),
                ..NativeCspDirectives::default()
            });
            continue;
        };
        policy.policies.push(parse_csp_directives(value));
    }
    for value in headers
        .get_all("content-security-policy-report-only")
        .iter()
    {
        let Ok(value) = value.to_str() else {
            continue;
        };
        if value.len() > MAX_NATIVE_RESPONSE_HEADER_VALUE_BYTES {
            continue;
        }
        policy.report_only_policies.push(NativeCspDeclaration {
            directives: parse_csp_directives(value),
            original_policy: value.to_owned(),
        });
    }
    for value in headers.get_all("reporting-endpoints").iter() {
        if let Ok(value) = value.to_str() {
            add_reporting_endpoint_header(&mut policy, "reporting-endpoints", value);
        }
    }
    for value in headers.get_all("report-to").iter() {
        if let Ok(value) = value.to_str() {
            add_reporting_endpoint_header(&mut policy, "report-to", value);
        }
    }
    policy.header_policy_count = policy.policies.len();
    policy
}

fn document_response_policy_headers(
    headers: &HeaderMap,
) -> Result<Vec<(String, String)>, NativeEngineError> {
    const POLICY_HEADERS: [&str; 5] = [
        "content-security-policy",
        "content-security-policy-report-only",
        "reporting-endpoints",
        "report-to",
        "referrer-policy",
    ];
    let mut pairs = Vec::new();
    let mut total_bytes = 0usize;
    for name in POLICY_HEADERS {
        for value in headers.get_all(name).iter() {
            let value = value.to_str().map_err(|_| NativeEngineError::Network {
                operation: "document response policy".into(),
                reason: "response contains a non-text policy header".into(),
            })?;
            if value.len() > MAX_NATIVE_RESPONSE_HEADER_VALUE_BYTES {
                return Err(NativeEngineError::limit(
                    "document response policy header value",
                    MAX_NATIVE_RESPONSE_HEADER_VALUE_BYTES,
                    value.len(),
                ));
            }
            total_bytes = total_bytes
                .saturating_add(name.len())
                .saturating_add(value.len());
            if pairs.len() >= MAX_NATIVE_RESPONSE_HEADERS {
                return Err(NativeEngineError::limit(
                    "document response policy headers",
                    MAX_NATIVE_RESPONSE_HEADERS,
                    pairs.len().saturating_add(1),
                ));
            }
            if total_bytes > MAX_NATIVE_RESPONSE_HEADER_BYTES {
                return Err(NativeEngineError::limit(
                    "document response policy header bytes",
                    MAX_NATIVE_RESPONSE_HEADER_BYTES,
                    total_bytes,
                ));
            }
            pairs.push((name.to_owned(), value.to_owned()));
        }
    }
    Ok(pairs)
}

fn merge_document_response_policy_headers(
    current: &mut Vec<(String, String)>,
    updates: Vec<(String, String)>,
) {
    let updated_names = updates
        .iter()
        .map(|(name, _)| name.to_ascii_lowercase())
        .collect::<std::collections::BTreeSet<_>>();
    current.retain(|(name, _)| !updated_names.contains(&name.to_ascii_lowercase()));
    current.extend(updates);
}

fn parse_csp_directives(value: &str) -> NativeCspDirectives {
    let mut policy = NativeCspDirectives::default();
    for directive in value.split(';') {
        let mut parts = directive.split_ascii_whitespace();
        let Some(name) = parts.next() else {
            continue;
        };
        let sources = parts.map(str::to_owned).collect::<Vec<_>>();
        match name.to_ascii_lowercase().as_str() {
            "style-src" => policy.style_sources = Some(sources),
            "style-src-elem" => policy.style_element_sources = Some(sources),
            "style-src-attr" => policy.style_attribute_sources = Some(sources),
            "script-src" => policy.script_sources = Some(sources),
            "script-src-elem" => policy.script_element_sources = Some(sources),
            "script-src-attr" => policy.script_attribute_sources = Some(sources),
            "img-src" => policy.image_sources = Some(sources),
            "font-src" => policy.font_sources = Some(sources),
            "media-src" => policy.media_sources = Some(sources),
            "frame-src" => policy.frame_sources = Some(sources),
            "child-src" => policy.child_sources = Some(sources),
            "connect-src" => policy.connect_sources = Some(sources),
            "worker-src" => policy.worker_sources = Some(sources),
            "form-action" => policy.form_action_sources = Some(sources),
            "navigate-to" => policy.navigate_to_sources = Some(sources),
            "default-src" => policy.default_sources = Some(sources),
            "report-uri" => {
                policy.report_uris = sources
                    .into_iter()
                    .take(MAX_NATIVE_CSP_REPORT_ENDPOINTS)
                    .collect()
            }
            "report-to" => policy.report_to = sources.into_iter().next(),
            _ => {}
        }
    }
    policy
}

fn add_reporting_endpoint_header(policy: &mut NativeCspPolicy, name: &str, value: &str) {
    if value.len() > MAX_NATIVE_RESPONSE_HEADER_VALUE_BYTES {
        return;
    }
    let entries = if name.eq_ignore_ascii_case("reporting-endpoints") {
        parse_reporting_endpoints_header(value)
    } else {
        parse_legacy_report_to_header(value)
    };
    for (group, endpoint) in entries {
        if group.is_empty()
            || group.len() > MAX_NATIVE_CSP_REPORT_ENDPOINT_BYTES
            || endpoint.len() > MAX_NATIVE_CSP_REPORT_ENDPOINT_BYTES
        {
            continue;
        }
        let endpoints = policy.reporting_endpoints.entry(group).or_default();
        if endpoints.len() < MAX_NATIVE_CSP_REPORT_ENDPOINTS
            && !endpoints.iter().any(|current| current == &endpoint)
        {
            endpoints.push(endpoint);
        }
    }
}

fn parse_reporting_endpoints_header(value: &str) -> Vec<(String, String)> {
    value
        .split(',')
        .filter_map(|member| {
            let (group, endpoint) = member.split_once('=')?;
            let group = group.trim();
            let endpoint = endpoint.trim();
            let endpoint = endpoint
                .strip_prefix('"')
                .and_then(|endpoint| endpoint.strip_suffix('"'))
                .unwrap_or(endpoint);
            (!group.is_empty() && !endpoint.is_empty())
                .then(|| (group.to_owned(), endpoint.to_owned()))
        })
        .take(MAX_NATIVE_CSP_REPORT_ENDPOINTS)
        .collect()
}

fn parse_legacy_report_to_header(value: &str) -> Vec<(String, String)> {
    let Ok(parsed) = serde_json::from_str::<serde_json::Value>(value) else {
        return Vec::new();
    };
    let objects: Vec<&serde_json::Value> = match &parsed {
        serde_json::Value::Array(values) => values.iter().collect(),
        serde_json::Value::Object(_) => vec![&parsed],
        _ => Vec::new(),
    };
    let mut entries = Vec::new();
    for object in objects {
        if object
            .get("max_age")
            .and_then(serde_json::Value::as_u64)
            .unwrap_or_default()
            == 0
        {
            continue;
        }
        let Some(group) = object.get("group").and_then(serde_json::Value::as_str) else {
            continue;
        };
        let Some(endpoints) = object
            .get("endpoints")
            .and_then(serde_json::Value::as_array)
        else {
            continue;
        };
        for endpoint in endpoints.iter().take(MAX_NATIVE_CSP_REPORT_ENDPOINTS) {
            let Some(url) = endpoint.get("url").and_then(serde_json::Value::as_str) else {
                continue;
            };
            if !url.is_empty() {
                entries.push((group.to_owned(), url.to_owned()));
            }
        }
        if entries.len() >= MAX_NATIVE_CSP_REPORT_ENDPOINTS {
            break;
        }
    }
    entries
}

pub(crate) fn resolve_subresource_url(
    document_url: &Url,
    href: &str,
) -> Result<Option<Url>, NativeEngineError> {
    let target_url = document_url
        .join(href)
        .map_err(|_| NativeEngineError::UnsupportedUrl {
            reason: "subresource URL could not be resolved against the document".into(),
        })?;
    reject_credentials(&target_url)?;
    if !is_network_url(without_fragment(target_url.as_str())) {
        return Ok(None);
    }
    Ok(Some(target_url))
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
    let duration_millis = (content_type == "audio/wav")
        .then(|| wav_duration_millis(bytes))
        .flatten();
    Ok(Some(NativeMediaMetadata {
        content_type: content_type.to_owned(),
        byte_length: bytes.len(),
        duration_millis,
    }))
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
        .and_then(|(_, data)| Some(data))
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

fn wav_duration_millis(bytes: &[u8]) -> Option<u64> {
    if bytes.len() < 12 || &bytes[..4] != b"RIFF" || &bytes[8..12] != b"WAVE" {
        return None;
    }
    let mut offset = 12usize;
    let mut byte_rate = None;
    let mut data_length = None;
    while offset.checked_add(8)? <= bytes.len() {
        let chunk_length = usize::try_from(u32::from_le_bytes(
            bytes[offset + 4..offset + 8].try_into().ok()?,
        ))
        .ok()?;
        let start = offset.checked_add(8)?;
        let end = start.checked_add(chunk_length)?.min(bytes.len());
        match &bytes[offset..offset + 4] {
            b"fmt " if end.saturating_sub(start) >= 12 => {
                byte_rate = Some(u32::from_le_bytes(
                    bytes[start + 8..start + 12].try_into().ok()?,
                ));
            }
            b"data" => data_length = Some(end.saturating_sub(start) as u64),
            _ => {}
        }
        if end == bytes.len() {
            break;
        }
        offset = end.saturating_add(chunk_length % 2);
    }
    let byte_rate = u64::from(byte_rate?);
    if byte_rate > 0 {
        data_length.map(|length| length.saturating_mul(1000) / byte_rate)
    } else {
        None
    }
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

#[cfg(test)]
pub(crate) fn referrer_for_navigation(
    current_url: &str,
    target_url: &str,
) -> Result<Option<String>, NativeEngineError> {
    referrer_for_navigation_with_policy(
        current_url,
        target_url,
        NativeFetchReferrerPolicy::StrictOriginWhenCrossOrigin,
    )
}

pub(crate) fn referrer_for_navigation_with_policy(
    current_url: &str,
    target_url: &str,
    policy: NativeFetchReferrerPolicy,
) -> Result<Option<String>, NativeEngineError> {
    validate_url_text("navigation URL", target_url)?;
    let target = Url::parse(without_fragment(target_url)).map_err(|_| {
        NativeEngineError::UnsupportedUrl {
            reason: "HTTP(S) navigation URL is not valid URL syntax".into(),
        }
    })?;
    if !is_network_url(target.as_str()) {
        return Ok(None);
    }
    reject_credentials(&target)?;
    let Ok(current) = Url::parse(without_fragment(current_url)) else {
        return Ok(None);
    };
    if !is_network_url(current.as_str()) {
        return Ok(None);
    }
    reject_credentials(&current)?;
    Ok(fetch_referrer_for_target(Some(&current), &target, policy))
}

fn normalize_referrer(
    referrer: Option<&str>,
    target: &Url,
) -> Result<Option<String>, NativeEngineError> {
    let Some(referrer) = referrer else {
        return Ok(None);
    };
    validate_url_text("navigation referrer", referrer)?;
    let source =
        Url::parse(without_fragment(referrer)).map_err(|_| NativeEngineError::UnsupportedUrl {
            reason: "navigation referrer is not valid URL syntax".into(),
        })?;
    if !is_network_url(source.as_str()) {
        return Ok(None);
    }
    reject_credentials(&source)?;
    if source.scheme().eq_ignore_ascii_case("https") && target.scheme().eq_ignore_ascii_case("http")
    {
        return Ok(None);
    }
    if source.origin() == target.origin() {
        let mut full = source;
        full.set_fragment(None);
        Ok(Some(full.to_string()))
    } else {
        Ok(Some(source.origin().ascii_serialization()))
    }
}

fn parse_navigation_referrer_source(
    referrer: Option<&str>,
) -> Result<Option<Url>, NativeEngineError> {
    let Some(referrer) = referrer else {
        return Ok(None);
    };
    validate_url_text("navigation referrer", referrer)?;
    let mut source =
        Url::parse(without_fragment(referrer)).map_err(|_| NativeEngineError::UnsupportedUrl {
            reason: "navigation referrer is not valid URL syntax".into(),
        })?;
    if !is_network_url(source.as_str()) {
        return Ok(None);
    }
    reject_credentials(&source)?;
    source.set_fragment(None);
    Ok(Some(source))
}

fn fetch_referrer_source(
    supplied_referrer: Option<&str>,
    owner_url: &Url,
) -> Result<Option<Url>, NativeEngineError> {
    let Some(supplied_referrer) = supplied_referrer else {
        return Ok(Some(owner_url.clone()));
    };
    if supplied_referrer.is_empty() {
        return Ok(None);
    }
    validate_url_text("fetch referrer URL", supplied_referrer)?;
    let mut referrer = Url::parse(supplied_referrer)
        .map_err(|_| NativeEngineError::invalid("fetch referrer URL", "must be an absolute URL"))?;
    if !is_network_url(referrer.as_str()) {
        return Ok(None);
    }
    if referrer.origin() != owner_url.origin() {
        return Err(NativeEngineError::invalid(
            "fetch referrer URL",
            "must have the request owner's origin",
        ));
    }
    let _ = referrer.set_username("");
    let _ = referrer.set_password(None);
    referrer.set_fragment(None);
    Ok(Some(referrer))
}

fn fetch_referrer_for_target(
    referrer_source: Option<&Url>,
    target_url: &Url,
    policy: NativeFetchReferrerPolicy,
) -> Option<String> {
    let mut source = referrer_source?.clone();
    source.set_fragment(None);
    if !is_network_url(source.as_str()) {
        return None;
    }
    let downgrade = is_potentially_trustworthy_http_url(&source)
        && !is_potentially_trustworthy_http_url(target_url);
    let same_origin = source.origin() == target_url.origin();
    let mut origin = source.origin().ascii_serialization();
    origin.push('/');
    match policy {
        NativeFetchReferrerPolicy::NoReferrer => None,
        NativeFetchReferrerPolicy::NoReferrerWhenDowngrade if downgrade => None,
        NativeFetchReferrerPolicy::NoReferrerWhenDowngrade
        | NativeFetchReferrerPolicy::UnsafeUrl => Some(source.to_string()),
        NativeFetchReferrerPolicy::SameOrigin if same_origin => Some(source.to_string()),
        NativeFetchReferrerPolicy::SameOrigin => None,
        NativeFetchReferrerPolicy::Origin => Some(origin),
        NativeFetchReferrerPolicy::StrictOrigin if !downgrade => Some(origin),
        NativeFetchReferrerPolicy::StrictOrigin => None,
        NativeFetchReferrerPolicy::OriginWhenCrossOrigin if same_origin => Some(source.to_string()),
        NativeFetchReferrerPolicy::OriginWhenCrossOrigin => Some(origin),
        NativeFetchReferrerPolicy::StrictOriginWhenCrossOrigin if downgrade => None,
        NativeFetchReferrerPolicy::StrictOriginWhenCrossOrigin if same_origin => {
            Some(source.to_string())
        }
        NativeFetchReferrerPolicy::StrictOriginWhenCrossOrigin => Some(origin),
    }
}

fn is_potentially_trustworthy_http_url(url: &Url) -> bool {
    if url.scheme() == "https" {
        return true;
    }
    if url.scheme() != "http" {
        return false;
    }
    let Some(host) = url.host_str() else {
        return false;
    };
    host == "localhost"
        || host.ends_with(".localhost")
        || host
            .parse::<std::net::IpAddr>()
            .is_ok_and(|address| address.is_loopback())
}

fn referrer_policy_from_headers(headers: &HeaderMap) -> Option<NativeFetchReferrerPolicy> {
    headers
        .get_all("referrer-policy")
        .iter()
        .filter_map(|value| value.to_str().ok())
        .flat_map(|value| value.split(','))
        .filter_map(|value| {
            let value = value.trim();
            (!value.is_empty())
                .then(|| NativeFetchReferrerPolicy::parse(value).ok())
                .flatten()
        })
        .last()
}

fn referrer_policy_from_header_pairs(
    headers: &[(String, String)],
) -> Option<NativeFetchReferrerPolicy> {
    headers
        .iter()
        .filter(|(name, _)| name.eq_ignore_ascii_case("referrer-policy"))
        .flat_map(|(_, value)| value.split(','))
        .filter_map(|value| {
            let value = value.trim();
            (!value.is_empty())
                .then(|| NativeFetchReferrerPolicy::parse(value).ok())
                .flatten()
        })
        .last()
}

fn cache_key(url: &Url) -> String {
    let mut key = url.clone();
    key.set_fragment(None);
    key.to_string()
}

fn fetch_response_cache_key(
    document_url: &Url,
    target_url: &Url,
    method: &NativeFetchMethod,
    request_headers: &BTreeMap<String, String>,
    content_type: Option<&str>,
    credentials: bool,
    cors_mode: NativeCorsMode,
    request_cookie: Option<&str>,
    request_referrer: Option<&str>,
    referrer_policy: &str,
) -> String {
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
        let mut state = Self::default();
        for cookie in profile {
            let Some(cookie) = NativeCookie::from_profile(cookie)? else {
                continue;
            };
            state.cookies.push(cookie);
        }
        Ok(state)
    }

    fn cookie_profile(&self) -> Vec<NativeCookieProfileEntry> {
        if !self.cookie_authority_enabled {
            return Vec::new();
        }
        self.cookies
            .iter()
            .filter_map(NativeCookie::to_profile)
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

    fn matching_cookies(&self, url: &Url, include_http_only: bool) -> Vec<&NativeCookie> {
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
            .iter()
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
        let had_existing = self.cookies.iter().any(same_cookie);
        self.cookies.retain(|candidate| !same_cookie(candidate));
        let evicted = if !had_existing && self.cookies.len() >= MAX_NATIVE_COOKIE_PROFILE_ENTRIES {
            self.cookies.remove(0).to_profile()
        } else {
            None
        };
        self.cookies.push(cookie);
        if self.cookies.len() > MAX_NATIVE_COOKIE_PROFILE_ENTRIES {
            return Err(NativeEngineError::limit(
                "native cookie profile entries",
                MAX_NATIVE_COOKIE_PROFILE_ENTRIES,
                self.cookies.len(),
            ));
        }
        Ok(evicted)
    }

    fn remove_cookie(&mut self, name: &str, domain: &str, path: &str) -> bool {
        if !self.cookie_authority_enabled {
            return false;
        }
        let before = self.cookies.len();
        self.cookies
            .retain(|cookie| cookie.name != name || cookie.domain != domain || cookie.path != path);
        self.cookies.len() != before
    }

    fn clear_cookies(&mut self, changes: &mut Vec<NativeCookieChange>) {
        if !self.cookie_authority_enabled {
            return;
        }
        for cookie in self.cookies.drain(..) {
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
        if max_age.is_some_and(|age| age <= 0) {
            let removed = self.cookies.iter().any(same_cookie);
            self.cookies.retain(|cookie| !same_cookie(cookie));
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
        let had_existing = self.cookies.iter().any(same_cookie);
        self.cookies.retain(|cookie| !same_cookie(cookie));
        let mut changes = Vec::new();
        if !had_existing && self.cookies.len() >= MAX_NATIVE_COOKIE_PROFILE_ENTRIES {
            let removed = self.cookies.remove(0);
            changes.push(NativeCookieChange {
                name: removed.name,
                domain: removed.domain,
                path: removed.path,
                cookie: None,
            });
        }
        self.cookies.push(NativeCookie {
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

fn cookie_same_site_allows<M: NativeCookieMethod>(
    cookie: &NativeCookie,
    request_url: &Url,
    initiator_url: Option<&Url>,
    top_level_navigation: bool,
    method: &M,
) -> bool {
    let same_site = initiator_url.is_none_or(|initiator| {
        schemeful_site(initiator).is_some()
            && schemeful_site(initiator) == schemeful_site(request_url)
    });
    match cookie_same_site(cookie.same_site.as_deref()) {
        NativeCookieSameSite::None => true,
        NativeCookieSameSite::Strict => same_site,
        NativeCookieSameSite::Lax => {
            same_site || (top_level_navigation && method.is_safe_cookie_method())
        }
    }
}

/// Return the schemeful site used by SameSite cookie policy. This keeps the
/// request policy independent from origin ports while still separating HTTP
/// and HTTPS and grouping ordinary subdomains under their registrable host.
fn schemeful_site(url: &Url) -> Option<String> {
    let host = url.host_str()?.trim_end_matches('.').to_ascii_lowercase();
    if host.is_empty() {
        return None;
    }
    let site_host =
        if host == "localhost" || host.parse::<std::net::IpAddr>().is_ok() || host.contains(':') {
            host
        } else {
            let labels = host.split('.').collect::<Vec<_>>();
            if labels.len() >= 2 {
                format!("{}.{}", labels[labels.len() - 2], labels[labels.len() - 1])
            } else {
                host
            }
        };
    Some(format!(
        "{}://{site_host}",
        url.scheme().to_ascii_lowercase()
    ))
}

fn valid_cookie_text(value: &str, name: bool) -> bool {
    !value.bytes().any(|byte| {
        if byte.is_ascii_control() {
            return true;
        }
        name && matches!(
            byte,
            b'(' | b')'
                | b'<'
                | b'>'
                | b'@'
                | b','
                | b';'
                | b':'
                | b'\\'
                | b'"'
                | b'/'
                | b'['
                | b']'
                | b'?'
                | b'='
                | b'{'
                | b'}'
                | b' '
                | b'\t'
        )
    })
}

fn unix_time_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_secs())
}

fn valid_cookie_domain(domain: &str) -> bool {
    domain
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b':'))
}

fn domain_matches(cookie: &NativeCookie, host: &str) -> bool {
    host_matches_domain(host, &cookie.domain) && (!cookie.host_only || host == cookie.domain)
}

fn host_matches_domain(host: &str, domain: &str) -> bool {
    host == domain
        || host
            .strip_suffix(domain)
            .is_some_and(|prefix| prefix.ends_with('.'))
}

fn path_matches(request_path: &str, cookie_path: &str) -> bool {
    request_path == cookie_path
        || (request_path.starts_with(cookie_path)
            && (cookie_path.ends_with('/')
                || request_path.as_bytes().get(cookie_path.len()) == Some(&b'/')))
}

fn default_cookie_path(url: &Url) -> String {
    let path = url.path();
    if path.is_empty() || path == "/" || !path.starts_with('/') {
        return "/".into();
    }
    path.rfind('/').map_or_else(
        || "/".into(),
        |index| {
            if index == 0 {
                "/".into()
            } else {
                path[..index].to_owned()
            }
        },
    )
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

fn is_http_redirect(status: reqwest::StatusCode) -> bool {
    matches!(
        status,
        reqwest::StatusCode::MOVED_PERMANENTLY
            | reqwest::StatusCode::FOUND
            | reqwest::StatusCode::SEE_OTHER
            | reqwest::StatusCode::TEMPORARY_REDIRECT
            | reqwest::StatusCode::PERMANENT_REDIRECT
    )
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

fn decode_base64_bytes(
    payload: &str,
    max_bytes: usize,
    resource_name: &'static str,
) -> Result<Vec<u8>, NativeEngineError> {
    let max_encoded_bytes = (max_bytes.saturating_add(2) / 3).saturating_mul(4);
    if payload.len() > max_encoded_bytes {
        return Err(NativeEngineError::limit(
            resource_name,
            max_encoded_bytes,
            payload.len(),
        ));
    }
    let decoded = base64::engine::general_purpose::STANDARD
        .decode(payload)
        .map_err(|_| NativeEngineError::UnsupportedUrl {
            reason: "data URL contains an invalid standard base64 payload".into(),
        })?;
    if decoded.len() > max_bytes {
        return Err(NativeEngineError::limit(
            resource_name,
            max_bytes,
            decoded.len(),
        ));
    }
    Ok(decoded)
}

fn decode_base64(payload: &str, max_document_bytes: usize) -> Result<String, NativeEngineError> {
    let decoded = decode_base64_bytes(payload, max_document_bytes, "base64 data payload")?;
    String::from_utf8(decoded).map_err(|_| NativeEngineError::UnsupportedUrl {
        reason: "base64 data URL payload is not valid UTF-8 HTML".into(),
    })
}

fn percent_decode(value: &str) -> Result<String, NativeEngineError> {
    let decoded = percent_decode_bytes(value, usize::MAX)?;
    String::from_utf8(decoded).map_err(|_| NativeEngineError::UnsupportedUrl {
        reason: "data URL payload is not valid UTF-8 HTML".into(),
    })
}

fn percent_decode_bytes(value: &str, max_bytes: usize) -> Result<Vec<u8>, NativeEngineError> {
    let bytes = value.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len().min(max_bytes));
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] != b'%' {
            decoded.push(bytes[index]);
            if decoded.len() > max_bytes {
                return Err(NativeEngineError::limit(
                    "percent-encoded media payload",
                    max_bytes,
                    decoded.len(),
                ));
            }
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
        if decoded.len() > max_bytes {
            return Err(NativeEngineError::limit(
                "percent-encoded media payload",
                max_bytes,
                decoded.len(),
            ));
        }
        index += 3;
    }
    Ok(decoded)
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
    use super::super::javascript::{
        NativeCookieChange, NativeIndexedDbState, NativeWebStorageState, load_cookie_profile,
        migrate_cookie_profile_from_web_storage, save_web_storage_profile,
    };
    use super::{
        JAVASCRIPT_MIME_TYPE_ESSENCES, MAX_NATIVE_CACHE_ENTRIES,
        MAX_NATIVE_CSP_SOURCE_EXPRESSION_BYTES, MAX_NATIVE_MEDIA_BYTES, NativeCookieProfileEntry,
        NativeCorsMode, NativeCspDirectives, NativeCspPolicy, NativeEngineConfig,
        NativeEngineError, NativeFetchMethod, NativeFetchReferrerPolicy, NativeInlineCspKind,
        NativeModuleResourceType, NativeNavigationMethod, NativeNavigationPolicyKind,
        NativeNetworkState, NativeObjectUrlResource, NativeRequestBody, NativeResource,
        NativeResourceLoader, NativeSubresourceKind, NativeTextCacheEntry, cache_control_max_age,
        cache_control_requires_revalidation, cached_resource_content_type_text_allowed,
        content_security_policy, cors_origin_header, cors_preflight_response_allowed,
        cors_response_allowed, csp_report_deliveries_for_declaration,
        csp_script_sources_allow_for_rooted_file, csp_sources_allow,
        csp_sources_allow_for_redirect, csp_sources_allow_for_rooted_file, data_font_bytes,
        data_media_metadata, decode_html_body, document_cache_fresh_until,
        document_cache_storage_allowed, fetch_referrer_for_target, fetch_referrer_source,
        javascript_mime_essence_allowed, javascript_mime_type_essence_match,
        media_metadata_from_bytes, mixed_content_allowed, module_content_type_text_allowed,
        referrer_for_navigation, referrer_for_navigation_with_policy, resolve_subresource_url,
        subresource_integrity_matches, supported_media_type_text,
    };
    use base64::Engine as _;
    use reqwest::header::{
        ACCESS_CONTROL_ALLOW_CREDENTIALS, ACCESS_CONTROL_ALLOW_HEADERS,
        ACCESS_CONTROL_ALLOW_METHODS, ACCESS_CONTROL_ALLOW_ORIGIN, CACHE_CONTROL,
        CONTENT_SECURITY_POLICY, HeaderMap, HeaderName, HeaderValue, PRAGMA, VARY,
    };
    use sha2::{Digest, Sha256, Sha384, Sha512};
    use std::fs;
    use std::path::PathBuf;
    use std::time::Instant;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;
    use url::Url;

    #[test]
    fn javascript_mime_type_attributes_and_response_headers_treat_parameters_differently() {
        assert_eq!(JAVASCRIPT_MIME_TYPE_ESSENCES.len(), 16);
        for essence in JAVASCRIPT_MIME_TYPE_ESSENCES {
            assert!(javascript_mime_type_essence_match(essence), "{essence}");
            assert!(javascript_mime_essence_allowed(essence), "{essence}");

            let parameterized = format!("{essence}; charset=utf-8");
            assert!(
                javascript_mime_essence_allowed(&parameterized),
                "external Content-Type should ignore parameters for {essence}"
            );
            assert!(
                !javascript_mime_type_essence_match(&parameterized),
                "script type attributes must not accept parameters for {essence}"
            );
        }
        assert!(!javascript_mime_essence_allowed("application/json"));
        assert!(!javascript_mime_type_essence_match("application/json"));
    }

    #[test]
    fn json_module_mime_type_uses_the_mime_sniffing_json_group() {
        for content_type in [
            "application/json",
            "application/json; charset=utf-8",
            "text/json",
            "application/problem+json",
            "text/vnd.api+json; charset=utf-8",
        ] {
            assert!(
                module_content_type_text_allowed(
                    NativeModuleResourceType::Json,
                    Some(content_type)
                ),
                "{content_type}"
            );
        }
        for content_type in ["text/plain", "text/javascript", "application/octet-stream"] {
            assert!(
                !module_content_type_text_allowed(
                    NativeModuleResourceType::Json,
                    Some(content_type)
                ),
                "{content_type}"
            );
        }
        assert!(!module_content_type_text_allowed(
            NativeModuleResourceType::Json,
            None
        ));
        assert!(!module_content_type_text_allowed(
            NativeModuleResourceType::JavaScript,
            None
        ));
        assert!(!module_content_type_text_allowed(
            NativeModuleResourceType::Unsupported,
            Some("application/json")
        ));
        assert!(cached_resource_content_type_text_allowed(None, None));
        assert!(!cached_resource_content_type_text_allowed(
            None,
            Some("application/json")
        ));
    }

    #[test]
    fn fetch_method_accepts_bounded_custom_http_tokens_without_widening_navigation() {
        for (input, expected) in [("report", "REPORT"), ("X-Glass-Method", "X-GLASS-METHOD")] {
            let method = NativeFetchMethod::from_fetch_method(input).unwrap();
            assert_eq!(method.as_str(), expected);
        }
        for input in ["", "bad method", "CONNECT", "TRACE", "TRACK"] {
            assert!(
                NativeFetchMethod::from_fetch_method(input).is_err(),
                "{input:?}"
            );
        }
        assert!(NativeFetchMethod::from_fetch_method(&"A".repeat(65)).is_err());
        assert!(NativeNavigationMethod::from_fetch_method("REPORT").is_err());
    }

    #[test]
    fn binary_navigation_body_uses_compact_base64_wire_encoding() {
        let body = NativeRequestBody::Bytes((0_u8..=255).collect());
        let encoded = serde_json::to_value(&body).unwrap();
        assert_eq!(encoded["kind"], "bytes");
        let encoded_value = encoded["value"].as_str().unwrap();
        assert_eq!(encoded_value.len(), 344);
        assert!(encoded_value.len() < 256 * 2);
        let decoded: NativeRequestBody = serde_json::from_value(encoded).unwrap();
        assert_eq!(decoded, body);
    }

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

    #[test]
    fn rooted_file_loader_reads_documents_and_denies_escape() {
        let root =
            std::env::temp_dir().join(format!("glass-native-file-loader-{}", std::process::id()));
        let outside = std::env::temp_dir().join(format!(
            "glass-native-file-loader-outside-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&root);
        let _ = fs::remove_dir_all(&outside);
        fs::create_dir_all(&root).unwrap();
        fs::create_dir_all(&outside).unwrap();
        let page_path = root.join("index.html");
        let media_path = root.join("clip.wav");
        let outside_path = outside.join("outside.html");
        fs::write(&page_path, "<p>rooted</p>").unwrap();
        fs::write(&media_path, b"RIFFxxxxWAVE").unwrap();
        fs::write(&outside_path, "<p>outside</p>").unwrap();
        let page_url = Url::from_file_path(&page_path).unwrap().to_string();
        let outside_url = Url::from_file_path(&outside_path).unwrap().to_string();
        let config = NativeEngineConfig::default().with_allowed_file_root(root.clone());
        let loader = NativeResourceLoader::new(&config).unwrap();

        assert_eq!(loader.load(&page_url).unwrap().body, "<p>rooted</p>");
        assert!(matches!(
            loader.load(&outside_url),
            Err(NativeEngineError::UnsupportedUrl { .. })
        ));
        let metadata = loader
            .load_local_file_media(&page_url, "clip.wav")
            .unwrap()
            .unwrap();
        assert_eq!(metadata.content_type, "audio/wav");
        assert_eq!(metadata.byte_length, 12);

        fs::remove_dir_all(root).unwrap();
        fs::remove_dir_all(outside).unwrap();
    }

    #[test]
    fn media_metadata_accepts_supported_types_and_bounds_payloads() {
        let mut wav = vec![0_u8; 44 + 8000];
        wav[0..4].copy_from_slice(b"RIFF");
        wav[4..8].copy_from_slice(&(8036_u32).to_le_bytes());
        wav[8..12].copy_from_slice(b"WAVE");
        wav[12..16].copy_from_slice(b"fmt ");
        wav[16..20].copy_from_slice(&(16_u32).to_le_bytes());
        wav[20..22].copy_from_slice(&(1_u16).to_le_bytes());
        wav[22..24].copy_from_slice(&(1_u16).to_le_bytes());
        wav[24..28].copy_from_slice(&(8000_u32).to_le_bytes());
        wav[28..32].copy_from_slice(&(8000_u32).to_le_bytes());
        wav[32..34].copy_from_slice(&(1_u16).to_le_bytes());
        wav[34..36].copy_from_slice(&(8_u16).to_le_bytes());
        wav[36..40].copy_from_slice(b"data");
        wav[40..44].copy_from_slice(&(8000_u32).to_le_bytes());
        wav[44..].fill(128);

        assert_eq!(
            supported_media_type_text("audio/mp3; codecs=1"),
            Some("audio/mpeg")
        );
        let metadata = media_metadata_from_bytes(Some("audio/wav"), &wav)
            .unwrap()
            .unwrap();
        assert_eq!(metadata.content_type, "audio/wav");
        assert_eq!(metadata.byte_length, wav.len());
        assert_eq!(metadata.duration_millis, Some(1000));
        assert_eq!(
            media_metadata_from_bytes(None, b"not a media resource").unwrap(),
            None
        );
        assert_eq!(
            media_metadata_from_bytes(Some("application/octet-stream"), &wav).unwrap(),
            None
        );
        assert!(media_metadata_from_bytes(None, &vec![0_u8; MAX_NATIVE_MEDIA_BYTES + 1]).is_err());
    }

    #[test]
    fn data_media_decodes_bounded_binary_payloads() {
        let url = Url::parse(
            "data:audio/wav;base64,UklGRiUAAABXQVZFZm10IBAAAAABAAEAQB8AAEAfAAABAAgAZGF0YQEAAACA",
        )
        .unwrap();
        let metadata = data_media_metadata(&url).unwrap().unwrap();
        assert_eq!(metadata.content_type, "audio/wav");
        assert_eq!(metadata.byte_length, 45);
        assert_eq!(metadata.duration_millis, Some(0));

        let sniffed = Url::parse("data:;base64,T2dnUw==").unwrap();
        assert_eq!(
            data_media_metadata(&sniffed).unwrap().unwrap().content_type,
            "application/ogg"
        );
        assert!(
            data_media_metadata(&Url::parse("data:application/octet-stream,%00").unwrap())
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn data_font_decoding_is_bounded_and_csp_aware() {
        let payload = base64::engine::general_purpose::STANDARD.encode([0_u8, 1, 2, 3]);
        let url = Url::parse(&format!("data:font/ttf;base64,{payload}")).unwrap();
        assert_eq!(data_font_bytes(&url).unwrap(), Some(vec![0, 1, 2, 3]));
        assert!(
            data_font_bytes(&Url::parse("data:application/octet-stream,AAAA").unwrap())
                .unwrap()
                .is_none()
        );

        let config = NativeEngineConfig::default();
        let mut loader = NativeResourceLoader::new(&config).unwrap();
        assert_eq!(
            loader
                .load_font("https://app.test/index.html", url.as_str(), None)
                .unwrap(),
            Some(vec![0, 1, 2, 3])
        );
        loader
            .set_document_content_security_policy_from_pairs(
                "https://app.test/index.html",
                &[("Content-Security-Policy".into(), "font-src 'none'".into())],
            )
            .unwrap();
        assert!(
            loader
                .load_font("https://app.test/index.html", url.as_str(), None)
                .unwrap()
                .is_none()
        );
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn rooted_file_and_blob_font_sources_are_bounded() {
        let root = std::path::PathBuf::from("/usr/share/fonts/truetype/dejavu");
        if !root.is_dir() || !root.join("DejaVuSans.ttf").is_file() {
            return;
        }
        let page_url = Url::from_file_path(root.join("index.html"))
            .unwrap()
            .to_string();
        let config = NativeEngineConfig::default().with_allowed_file_root(root);
        let mut loader = NativeResourceLoader::new(&config).unwrap();
        let file_bytes = loader
            .load_font(&page_url, "DejaVuSans.ttf", None)
            .unwrap()
            .unwrap();
        assert!(!file_bytes.is_empty());
        assert!(
            loader
                .load_font("https://app.test/index.html", "file:///etc/hosts", None)
                .unwrap()
                .is_none()
        );

        let object_url = NativeObjectUrlResource {
            content_type: Some("font/ttf".into()),
            body: vec![1, 2, 3, 4],
        };
        assert_eq!(
            loader
                .load_font(
                    "fixture://app.test/index.html",
                    "blob:https://app.test/native-font",
                    Some(&object_url),
                )
                .unwrap(),
            Some(vec![1, 2, 3, 4])
        );
    }

    #[tokio::test]
    async fn network_font_response_preserves_url_headers_and_body() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut request = [0_u8; 2048];
            let _ = stream.read(&mut request).await.unwrap();
            let response = concat!(
                "HTTP/1.1 200 OK\r\n",
                "Content-Type: font/ttf\r\n",
                "Access-Control-Allow-Origin: *\r\n",
                "Content-Length: 4\r\n",
                "Connection: close\r\n\r\n",
                "font"
            );
            stream.write_all(response.as_bytes()).await.unwrap();
        });
        let config = NativeEngineConfig::default();
        let mut loader = NativeResourceLoader::new(&config).unwrap();
        let source = format!("http://{address}/font.ttf");
        let response = loader
            .load_font_response_async("http://app.test/index.html", &source, None)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(response.url, source);
        assert_eq!(response.status, 200);
        assert_eq!(response.status_text, "OK");
        assert_eq!(response.content_type.as_deref(), Some("font/ttf"));
        assert!(
            response
                .headers
                .iter()
                .any(|(name, value)| name == "content-type" && value == "font/ttf")
        );
        assert_eq!(response.body, b"font");
        assert!(!response.redirected);
        server.await.unwrap();
    }

    #[tokio::test]
    async fn network_font_cache_reuses_fresh_response_without_refetching() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut request = [0_u8; 2048];
            let read = stream.read(&mut request).await.unwrap();
            assert!(read > 0);
            let response = concat!(
                "HTTP/1.1 200 OK\r\n",
                "Content-Type: font/ttf\r\n",
                "Access-Control-Allow-Origin: *\r\n",
                "Cache-Control: public, max-age=60\r\n",
                "Content-Length: 4\r\n",
                "Connection: close\r\n\r\n",
                "font"
            );
            stream.write_all(response.as_bytes()).await.unwrap();
        });
        let config = NativeEngineConfig::default();
        let mut loader = NativeResourceLoader::new(&config).unwrap();
        let source = format!("http://{address}/font.ttf");
        let first = loader
            .load_font_response_async("http://app.test/index.html", &source, None)
            .await
            .unwrap();
        let second = loader
            .load_font_response_async("http://app.test/index.html", &source, None)
            .await
            .unwrap();
        assert_eq!(
            first.as_ref().map(|response| response.body.as_slice()),
            Some(&b"font"[..])
        );
        assert_eq!(second, first);
        assert_eq!(
            first
                .as_ref()
                .and_then(|response| response.content_type.as_deref()),
            Some("font/ttf")
        );
        assert_eq!(loader.network.font_cache.len(), 1);
        assert_eq!(
            loader
                .load_font_async("http://app.test/index.html", &source, None)
                .await
                .unwrap(),
            Some(b"font".to_vec())
        );
        server.await.unwrap();
    }

    #[tokio::test]
    async fn network_font_cache_revalidates_with_etag_and_304() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (mut first_stream, _) = listener.accept().await.unwrap();
            let mut first_request = [0_u8; 2048];
            let first_read = first_stream.read(&mut first_request).await.unwrap();
            assert!(first_read > 0);
            let first_response = concat!(
                "HTTP/1.1 200 OK\r\n",
                "Content-Type: font/ttf\r\n",
                "Access-Control-Allow-Origin: *\r\n",
                "Cache-Control: no-cache\r\n",
                "ETag: \"font-v1\"\r\n",
                "Content-Length: 4\r\n",
                "Connection: close\r\n\r\n",
                "font"
            );
            first_stream
                .write_all(first_response.as_bytes())
                .await
                .unwrap();

            let (mut second_stream, _) = listener.accept().await.unwrap();
            let mut second_request = [0_u8; 2048];
            let second_read = second_stream.read(&mut second_request).await.unwrap();
            let second_request = String::from_utf8_lossy(&second_request[..second_read]);
            assert!(
                second_request
                    .to_ascii_lowercase()
                    .contains("if-none-match: \"font-v1\"")
            );
            let second_response = concat!(
                "HTTP/1.1 304 Not Modified\r\n",
                "Cache-Control: public, max-age=60\r\n",
                "ETag: \"font-v1\"\r\n",
                "Connection: close\r\n\r\n"
            );
            second_stream
                .write_all(second_response.as_bytes())
                .await
                .unwrap();
        });
        let config = NativeEngineConfig::default();
        let mut loader = NativeResourceLoader::new(&config).unwrap();
        let source = format!("http://{address}/font.ttf");
        let first = loader
            .load_font_async("http://app.test/index.html", &source, None)
            .await
            .unwrap();
        let second = loader
            .load_font_async("http://app.test/index.html", &source, None)
            .await
            .unwrap();
        assert_eq!(first, Some(b"font".to_vec()));
        assert_eq!(second, first);
        let response = loader
            .load_font_response_async("http://app.test/index.html", &source, None)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(response.url, source);
        assert_eq!(response.content_type.as_deref(), Some("font/ttf"));
        assert_eq!(response.body, b"font");
        assert_eq!(loader.network.font_cache.len(), 1);
        server.await.unwrap();
    }

    #[test]
    fn subresource_integrity_uses_valid_strongest_hash_metadata() {
        let body = b"globalThis.integrity = true;";
        let sha256 = base64::engine::general_purpose::STANDARD.encode(Sha256::digest(body));
        let sha384 = base64::engine::general_purpose::STANDARD.encode(Sha384::digest(body));
        let sha512 = base64::engine::general_purpose::STANDARD.encode(Sha512::digest(body));
        let wrong_sha256 = base64::engine::general_purpose::STANDARD.encode([0_u8; 32]);
        let wrong_sha512 = base64::engine::general_purpose::STANDARD.encode([0_u8; 64]);

        assert!(subresource_integrity_matches(None, body));
        assert!(subresource_integrity_matches(
            Some(&format!("sha256-{sha256}")),
            body
        ));
        assert!(subresource_integrity_matches(
            Some(&format!("sha384-{sha384}?future-option sha256-{sha256}")),
            body
        ));
        assert!(subresource_integrity_matches(
            Some(&format!("sha512-{sha512} sha384-wrong")),
            body
        ));
        assert!(!subresource_integrity_matches(
            Some(&format!("sha512-{wrong_sha512} sha256-{sha256}")),
            body
        ));
        assert!(subresource_integrity_matches(Some("sha999-unknown"), body));
        assert!(!subresource_integrity_matches(
            Some(&format!("sha256-{wrong_sha256}")),
            body
        ));
    }

    #[test]
    fn content_process_loader_cannot_read_or_change_cookie_state() {
        let mut loader = NativeResourceLoader::for_content_process(1024, None, &[]).unwrap();
        let url = Url::parse("https://app.test/page").unwrap();

        assert_eq!(
            loader.network.cookie_header_for_request(
                &url,
                Some(&url),
                false,
                NativeNavigationMethod::Get,
            ),
            None
        );
        assert!(
            loader
                .network
                .store_cookie(&url, "secret=child; HttpOnly; Path=/")
                .is_empty()
        );
        loader
            .set_document_cookie(url.as_str(), "visible=child; Path=/")
            .unwrap();

        assert_eq!(loader.document_cookie(url.as_str()).unwrap(), "");
        assert!(loader.cookie_profile().is_empty());
        assert!(loader.take_cookie_changes().is_empty());
        let mut parent_loader = NativeResourceLoader::new(&NativeEngineConfig::default()).unwrap();
        parent_loader
            .set_document_cookie(url.as_str(), "parent=owned; Path=/")
            .unwrap();
        let parent_profile = parent_loader.cookie_profile();
        assert_eq!(parent_profile.len(), 1);
        assert!(loader.set_cookie_profiles(&parent_profile).is_err());
        assert!(loader.replace_cookie_profiles(&parent_profile).is_err());
        assert!(
            loader
                .apply_cookie_changes(&[NativeCookieChange {
                    name: "secret".into(),
                    domain: "app.test".into(),
                    path: "/".into(),
                    cookie: None,
                }])
                .is_err()
        );
        assert!(loader.merge_fetch_task_state(parent_loader).is_err());
        assert!(loader.cookie_profile().is_empty());
    }

    #[test]
    fn cookie_state_obeys_scope_security_expiration_and_bounds() {
        let mut state = NativeNetworkState::default();
        let page = Url::parse("http://example.test/account/login").unwrap();
        state.store_cookie(&page, "session=alpha; Path=/account");
        let secure_page = Url::parse("https://example.test/account/login").unwrap();
        state.store_cookie(&secure_page, "secure=secret; Secure; Path=/");
        state.store_cookie(&page, "hidden=value; HttpOnly; Path=/account");
        assert_eq!(
            state.cookie_header_for_request(
                &Url::parse("http://example.test/account/home").unwrap(),
                None,
                false,
                NativeNavigationMethod::Get,
            ),
            Some("hidden=value; session=alpha".into())
        );
        assert_eq!(
            state.cookie_header_for_request(
                &Url::parse("http://example.test/public").unwrap(),
                None,
                false,
                NativeNavigationMethod::Get,
            ),
            None
        );
        assert_eq!(
            state.cookie_header_for_request(
                &Url::parse("https://example.test/account/home").unwrap(),
                None,
                false,
                NativeNavigationMethod::Get,
            ),
            Some("hidden=value; session=alpha; secure=secret".into())
        );
        assert_eq!(
            state.document_cookie_header(&Url::parse("https://example.test/account/home").unwrap()),
            Some("session=alpha; secure=secret".into())
        );

        state.store_cookie(&page, "session=gone; Path=/account; Max-Age=0");
        assert_eq!(
            state.cookie_header_for_request(
                &Url::parse("http://example.test/account/home").unwrap(),
                None,
                false,
                NativeNavigationMethod::Get,
            ),
            Some("hidden=value".into())
        );

        for index in 0..(MAX_NATIVE_CACHE_ENTRIES + 1) {
            state.store_document_cache(
                format!("http://example.test/{index}"),
                super::NativeDocumentCacheEntry {
                    resource: NativeResource {
                        url: format!("http://example.test/{index}"),
                        origin: super::NativeOrigin::from_url(
                            &Url::parse("http://example.test/").unwrap(),
                        )
                        .unwrap(),
                        body: index.to_string(),
                    },
                    fresh_until: None,
                    etag: None,
                    last_modified: None,
                },
            );
        }
        assert_eq!(state.cache.len(), MAX_NATIVE_CACHE_ENTRIES);
    }

    #[test]
    fn cookie_same_site_policy_preserves_metadata_and_filters_cross_site_requests() {
        let mut state = NativeNetworkState::default();
        let app = Url::parse("https://app.example.test/account/page").unwrap();
        let api = Url::parse("https://api.example.test/data").unwrap();
        let evil = Url::parse("https://evil.test/data").unwrap();

        state.store_cookie(
            &app,
            "strict=1; Domain=example.test; Path=/; SameSite=Strict; Priority=High",
        );
        state.store_cookie(
            &app,
            "lax=1; Domain=example.test; Path=/; SameSite=Lax; Priority=Medium",
        );
        state.store_cookie(
            &app,
            "none=1; Domain=example.test; Path=/; SameSite=None; Secure; Priority=Low",
        );
        state.store_cookie(&evil, "strict=1; Path=/; SameSite=Strict");
        state.store_cookie(&evil, "lax=1; Path=/; SameSite=Lax");
        state.store_cookie(&evil, "default=1; Path=/");
        state.store_cookie(&evil, "none=1; Path=/; SameSite=None; Secure");

        assert_eq!(
            state.cookie_header_for_request(&api, Some(&app), false, NativeNavigationMethod::Get),
            Some("lax=1; none=1; strict=1".into())
        );
        assert_eq!(
            state.cookie_header_for_request(&evil, Some(&app), false, NativeNavigationMethod::Get,),
            Some("none=1".into())
        );
        assert_eq!(
            state.cookie_header_for_request(&evil, Some(&app), true, NativeNavigationMethod::Get,),
            Some("default=1; lax=1; none=1".into())
        );
        assert_eq!(
            state.cookie_header_for_request(&evil, Some(&app), true, NativeNavigationMethod::Post,),
            Some("none=1".into())
        );

        let profiles = state.cookie_profile();
        let strict = profiles
            .iter()
            .find(|cookie| cookie.name == "strict" && cookie.domain == "example.test")
            .unwrap();
        assert_eq!(strict.same_site.as_deref(), Some("Strict"));
        assert_eq!(strict.priority.as_deref(), Some("High"));

        assert!(
            state
                .store_cookie(&evil, "invalid=1; Path=/; SameSite=None")
                .is_empty()
        );
        assert!(
            NativeNetworkState::from_profile(vec![NativeCookieProfileEntry {
                name: "invalid".into(),
                value: "1".into(),
                domain: "evil.test".into(),
                path: "/".into(),
                host_only: true,
                secure: false,
                http_only: false,
                same_site: Some("None".into()),
                priority: None,
                expires_at_unix_seconds: None,
            }])
            .is_err()
        );
    }

    #[test]
    fn cookie_profile_merges_stale_key_changes() {
        let profile_path = std::env::temp_dir().join(format!(
            "glass-native-web-storage-{}-cookie-merge.json",
            std::process::id()
        ));
        let cookie_path = PathBuf::from(format!("{}.cookies", profile_path.display()));
        let lock_path = profile_path.with_extension("lock");
        let cookie_lock_path = cookie_path.with_extension("lock");
        let _ = fs::remove_file(&profile_path);
        let _ = fs::remove_file(&cookie_path);
        let _ = fs::remove_file(&lock_path);
        let _ = fs::remove_file(&cookie_lock_path);
        let config = NativeEngineConfig::default().with_storage_path(profile_path.clone());
        let page = "https://example.test/account/page";
        let mut first = NativeResourceLoader::new(&config).unwrap();
        let mut second = NativeResourceLoader::new(&config).unwrap();
        first
            .set_document_cookie(page, "first=one; Path=/")
            .unwrap();
        second
            .set_document_cookie(page, "second=two; Path=/")
            .unwrap();

        let first_state = first.cookie_profile();
        let first_changes = first.take_cookie_changes();
        save_web_storage_profile(
            Some(&profile_path),
            &NativeWebStorageState::default(),
            &[],
            &first_state,
            &first_changes,
            &NativeIndexedDbState::default(),
            &[],
        )
        .unwrap();
        let second_state = second.cookie_profile();
        let second_changes = second.take_cookie_changes();
        save_web_storage_profile(
            Some(&profile_path),
            &NativeWebStorageState::default(),
            &[],
            &second_state,
            &second_changes,
            &NativeIndexedDbState::default(),
            &[],
        )
        .unwrap();

        let reopened = NativeResourceLoader::new(&config).unwrap();
        assert_eq!(
            reopened.document_cookie(page).unwrap(),
            "first=one; second=two"
        );
        let web_profile: serde_json::Value =
            serde_json::from_slice(&fs::read(&profile_path).unwrap()).unwrap();
        let cookie_profile: serde_json::Value =
            serde_json::from_slice(&fs::read(&cookie_path).unwrap()).unwrap();
        assert_eq!(web_profile["cookies"], serde_json::json!([]));
        assert_eq!(cookie_profile["cookies"].as_array().unwrap().len(), 2);

        let content_loader =
            NativeResourceLoader::for_content_process(256 * 1024, Some(&profile_path), &[])
                .unwrap();
        assert!(
            content_loader
                .cookies_for_document(page)
                .unwrap()
                .is_empty()
        );

        let _ = fs::remove_file(profile_path);
        let _ = fs::remove_file(cookie_path);
        let _ = fs::remove_file(lock_path);
        let _ = fs::remove_file(cookie_lock_path);
    }

    #[test]
    fn legacy_cookie_profile_migrates_out_of_content_readable_storage_file() {
        let profile_path = std::env::temp_dir().join(format!(
            "glass-native-web-storage-{}-cookie-migrate.json",
            std::process::id()
        ));
        let cookie_path = PathBuf::from(format!("{}.cookies", profile_path.display()));
        let lock_path = profile_path.with_extension("lock");
        let cookie_lock_path = cookie_path.with_extension("lock");
        let _ = fs::remove_file(&profile_path);
        let _ = fs::remove_file(&cookie_path);
        let _ = fs::remove_file(&lock_path);
        let _ = fs::remove_file(&cookie_lock_path);
        let legacy_cookie = serde_json::json!({
            "name": "legacy_secret",
            "value": "must-stay-parent-owned",
            "domain": "example.test",
            "path": "/",
            "host_only": true,
            "secure": false,
            "http_only": true,
            "same_site": "Lax",
            "priority": null,
            "expires_at_unix_seconds": null,
        });
        fs::write(
            &profile_path,
            serde_json::to_vec(&serde_json::json!({
                "version": 1,
                "revision": 7,
                "local": {},
                "cookies": [legacy_cookie],
            }))
            .unwrap(),
        )
        .unwrap();

        migrate_cookie_profile_from_web_storage(Some(&profile_path)).unwrap();

        let web_profile: serde_json::Value =
            serde_json::from_slice(&fs::read(&profile_path).unwrap()).unwrap();
        assert_eq!(web_profile["revision"], 7);
        assert_eq!(web_profile["cookies"], serde_json::json!([]));
        assert_eq!(
            load_cookie_profile(Some(&profile_path))
                .unwrap()
                .first()
                .map(|cookie| cookie.value.as_str()),
            Some("must-stay-parent-owned")
        );

        let _ = fs::remove_file(profile_path);
        let _ = fs::remove_file(cookie_path);
        let _ = fs::remove_file(lock_path);
        let _ = fs::remove_file(cookie_lock_path);
    }

    #[test]
    fn cache_policy_rejects_private_or_stale_variants() {
        let mut headers = HeaderMap::new();
        assert!(document_cache_storage_allowed(&headers));
        assert!(!cache_control_requires_revalidation(&headers));
        headers.insert(
            CACHE_CONTROL,
            HeaderValue::from_static("public, max-age=60"),
        );
        assert!(document_cache_storage_allowed(&headers));
        assert!(!cache_control_requires_revalidation(&headers));
        headers.insert(CACHE_CONTROL, HeaderValue::from_static("no-store"));
        assert!(!document_cache_storage_allowed(&headers));
        headers.remove(CACHE_CONTROL);
        headers.insert(PRAGMA, HeaderValue::from_static("no-cache"));
        assert!(document_cache_storage_allowed(&headers));
        assert!(cache_control_requires_revalidation(&headers));
        headers.remove(PRAGMA);
        headers.insert(VARY, HeaderValue::from_static("Accept-Encoding, Cookie"));
        assert!(!document_cache_storage_allowed(&headers));
    }

    #[test]
    fn document_cache_tracks_freshness_and_revalidation_metadata() {
        let now = Instant::now();
        let mut headers = HeaderMap::new();
        headers.insert(
            CACHE_CONTROL,
            HeaderValue::from_static("public, max-age=60"),
        );
        assert_eq!(cache_control_max_age(&headers), Some(60));
        assert!(document_cache_storage_allowed(&headers));
        assert!(document_cache_fresh_until(&headers, now).is_some_and(|deadline| deadline > now));

        headers.insert(CACHE_CONTROL, HeaderValue::from_static("no-cache"));
        assert!(document_cache_storage_allowed(&headers));
        assert_eq!(document_cache_fresh_until(&headers, now), Some(now));

        headers.insert(CACHE_CONTROL, HeaderValue::from_static("no-store"));
        assert!(!document_cache_storage_allowed(&headers));
    }

    #[test]
    fn script_cache_preserves_module_response_referrer_policy_across_revalidation() {
        let now = Instant::now();
        let mut response_headers = HeaderMap::new();
        response_headers.insert(
            CACHE_CONTROL,
            HeaderValue::from_static("public, max-age=60"),
        );
        response_headers.insert(
            HeaderName::from_static("referrer-policy"),
            HeaderValue::from_static("origin"),
        );
        let entry = NativeTextCacheEntry::from_response(
            "https://module.test/entry.js".into(),
            "export {};".into(),
            &response_headers,
            now,
        )
        .expect("cacheable module response metadata must be retained");
        assert_eq!(
            entry.referrer_policy,
            Some(NativeFetchReferrerPolicy::Origin)
        );

        let absent_header = HeaderMap::new();
        let unchanged = entry
            .clone()
            .refresh_from_not_modified(&absent_header, now)
            .unwrap();
        assert_eq!(
            unchanged.referrer_policy,
            Some(NativeFetchReferrerPolicy::Origin)
        );

        let mut replacement_headers = HeaderMap::new();
        replacement_headers.insert(
            HeaderName::from_static("referrer-policy"),
            HeaderValue::from_static("no-referrer"),
        );
        let replaced = entry
            .clone()
            .refresh_from_not_modified(&replacement_headers, now)
            .unwrap();
        assert_eq!(
            replaced.referrer_policy,
            Some(NativeFetchReferrerPolicy::NoReferrer)
        );

        replacement_headers.insert(
            HeaderName::from_static("referrer-policy"),
            HeaderValue::from_static("not-a-policy"),
        );
        let ignored = entry
            .refresh_from_not_modified(&replacement_headers, now)
            .unwrap();
        assert_eq!(ignored.referrer_policy, None);
    }

    #[test]
    fn referrer_policy_is_full_same_origin_origin_only_cross_origin_and_none_on_downgrade() {
        assert_eq!(
            referrer_for_navigation(
                "http://source.test/path/page?query=1#fragment",
                "http://source.test/next",
            )
            .unwrap(),
            Some("http://source.test/path/page?query=1".into())
        );
        assert_eq!(
            referrer_for_navigation("http://source.test/path/page", "http://target.test/next")
                .unwrap(),
            Some("http://source.test/".into())
        );
        assert_eq!(
            referrer_for_navigation("https://source.test/path", "http://target.test/next").unwrap(),
            None
        );
        assert_eq!(
            referrer_for_navigation("data:text/html,<p>local</p>", "http://target.test/next")
                .unwrap(),
            None
        );
    }

    #[test]
    fn navigation_referrer_policy_is_applied_before_cross_origin_requests() {
        let source = "https://source.test/path/page?token=secret#fragment";
        let target = "https://target.test/next";
        assert_eq!(
            referrer_for_navigation_with_policy(
                source,
                target,
                NativeFetchReferrerPolicy::NoReferrer,
            )
            .unwrap(),
            None
        );
        assert_eq!(
            referrer_for_navigation_with_policy(source, target, NativeFetchReferrerPolicy::Origin,)
                .unwrap(),
            Some("https://source.test/".into())
        );
        assert_eq!(
            referrer_for_navigation_with_policy(
                source,
                target,
                NativeFetchReferrerPolicy::UnsafeUrl,
            )
            .unwrap(),
            Some("https://source.test/path/page?token=secret".into())
        );
    }

    #[test]
    fn fetch_referrer_policy_accepts_all_standard_tokens_and_empty_default() {
        let cases = [
            ("", NativeFetchReferrerPolicy::StrictOriginWhenCrossOrigin),
            ("no-referrer", NativeFetchReferrerPolicy::NoReferrer),
            (
                "no-referrer-when-downgrade",
                NativeFetchReferrerPolicy::NoReferrerWhenDowngrade,
            ),
            ("same-origin", NativeFetchReferrerPolicy::SameOrigin),
            ("origin", NativeFetchReferrerPolicy::Origin),
            ("strict-origin", NativeFetchReferrerPolicy::StrictOrigin),
            (
                "origin-when-cross-origin",
                NativeFetchReferrerPolicy::OriginWhenCrossOrigin,
            ),
            (
                "strict-origin-when-cross-origin",
                NativeFetchReferrerPolicy::StrictOriginWhenCrossOrigin,
            ),
            ("unsafe-url", NativeFetchReferrerPolicy::UnsafeUrl),
        ];

        for (token, expected) in cases {
            assert_eq!(NativeFetchReferrerPolicy::parse(token).unwrap(), expected);
        }
        assert!(NativeFetchReferrerPolicy::parse("invalid").is_err());
    }

    #[test]
    fn fetch_referrer_policies_compute_each_target_from_the_original_source() {
        let source = Url::parse("https://source.test/account?token=hidden#fragment").unwrap();
        let same_origin = Url::parse("https://source.test/next").unwrap();
        let cross_origin = Url::parse("https://target.test/next").unwrap();
        let downgrade = Url::parse("http://target.test/next").unwrap();
        let full = "https://source.test/account?token=hidden";
        let origin = "https://source.test/";

        assert_eq!(
            fetch_referrer_for_target(
                Some(&source),
                &same_origin,
                NativeFetchReferrerPolicy::NoReferrer
            ),
            None
        );
        assert_eq!(
            fetch_referrer_for_target(
                Some(&source),
                &cross_origin,
                NativeFetchReferrerPolicy::NoReferrerWhenDowngrade,
            ),
            Some(full.into())
        );
        assert_eq!(
            fetch_referrer_for_target(
                Some(&source),
                &downgrade,
                NativeFetchReferrerPolicy::NoReferrerWhenDowngrade,
            ),
            None
        );
        assert_eq!(
            fetch_referrer_for_target(
                Some(&source),
                &same_origin,
                NativeFetchReferrerPolicy::SameOrigin
            ),
            Some(full.into())
        );
        assert_eq!(
            fetch_referrer_for_target(
                Some(&source),
                &cross_origin,
                NativeFetchReferrerPolicy::SameOrigin
            ),
            None
        );
        assert_eq!(
            fetch_referrer_for_target(
                Some(&source),
                &cross_origin,
                NativeFetchReferrerPolicy::Origin
            ),
            Some(origin.into())
        );
        assert_eq!(
            fetch_referrer_for_target(
                Some(&source),
                &downgrade,
                NativeFetchReferrerPolicy::StrictOrigin
            ),
            None
        );
        assert_eq!(
            fetch_referrer_for_target(
                Some(&source),
                &cross_origin,
                NativeFetchReferrerPolicy::OriginWhenCrossOrigin,
            ),
            Some(origin.into())
        );
        assert_eq!(
            fetch_referrer_for_target(
                Some(&source),
                &downgrade,
                NativeFetchReferrerPolicy::OriginWhenCrossOrigin,
            ),
            Some(origin.into())
        );
        assert_eq!(
            fetch_referrer_for_target(
                Some(&source),
                &same_origin,
                NativeFetchReferrerPolicy::StrictOriginWhenCrossOrigin,
            ),
            Some(full.into())
        );
        assert_eq!(
            fetch_referrer_for_target(
                Some(&source),
                &downgrade,
                NativeFetchReferrerPolicy::StrictOriginWhenCrossOrigin,
            ),
            None
        );
        assert_eq!(
            fetch_referrer_for_target(
                Some(&source),
                &downgrade,
                NativeFetchReferrerPolicy::UnsafeUrl
            ),
            Some(full.into())
        );
    }

    #[test]
    fn fetch_referrer_source_removes_credentials_and_fragment_and_enforces_owner_origin() {
        let owner = Url::parse("https://source.test/page").unwrap();
        let referrer = fetch_referrer_source(
            Some("https://user:secret@source.test/path?query=1#fragment"),
            &owner,
        )
        .unwrap()
        .unwrap();
        assert_eq!(referrer.as_str(), "https://source.test/path?query=1");
        assert!(fetch_referrer_source(Some(""), &owner).unwrap().is_none());
        assert!(fetch_referrer_source(Some("https://other.test/path"), &owner).is_err());
    }

    #[test]
    fn csp_source_lists_cover_resource_families_and_default_fallback() {
        let mut headers = HeaderMap::new();
        headers.insert(
            CONTENT_SECURITY_POLICY,
            HeaderValue::from_static(
                "default-src 'none'; script-src https://scripts.test; img-src *; connect-src 'self'",
            ),
        );
        let policy = content_security_policy(&headers);
        let document = Url::parse("https://app.test/index.html").unwrap();
        let script = Url::parse("https://scripts.test/app.js").unwrap();
        let image = Url::parse("https://images.test/logo.png").unwrap();
        let socket = Url::parse("https://api.test/data").unwrap();

        assert!(policy.allows(NativeSubresourceKind::Script, &document, &script));
        assert!(policy.allows(NativeSubresourceKind::Image, &document, &image));
        assert!(!policy.allows(NativeSubresourceKind::Style, &document, &document));
        assert!(policy.allows(NativeSubresourceKind::Connect, &document, &document));
        assert!(!policy.allows(NativeSubresourceKind::Connect, &document, &socket));
        assert!(!policy.allows(NativeSubresourceKind::Font, &document, &document));
    }

    #[test]
    fn csp_source_expressions_match_scheme_host_port_path_and_upgrade_rules() {
        let document = Url::parse("http://app.test/index.html").unwrap();
        let wildcard = vec!["*.Example.test".to_owned()];
        assert!(csp_sources_allow(
            Some(&wildcard),
            &document,
            &Url::parse("https://cdn.example.test/assets/app.js").unwrap()
        ));
        assert!(!csp_sources_allow(
            Some(&wildcard),
            &document,
            &Url::parse("https://example.test/assets/app.js").unwrap()
        ));
        assert!(!csp_sources_allow(
            Some(&wildcard),
            &document,
            &Url::parse("https://badexample.test/assets/app.js").unwrap()
        ));

        let path = vec!["https://cdn.example.test/assets/".to_owned()];
        assert!(csp_sources_allow(
            Some(&path),
            &document,
            &Url::parse("https://cdn.example.test/assets/app.js?cache=1#fragment").unwrap()
        ));
        assert!(!csp_sources_allow(
            Some(&path),
            &document,
            &Url::parse("https://cdn.example.test/asset/app.js").unwrap()
        ));
        let exact_path = vec!["https://cdn.example.test/assets/app%2Ejs".to_owned()];
        assert!(csp_sources_allow(
            Some(&exact_path),
            &document,
            &Url::parse("https://cdn.example.test/assets/app.js").unwrap()
        ));
        assert!(!csp_sources_allow(
            Some(&exact_path),
            &document,
            &Url::parse("https://cdn.example.test/assets/app.js/extra").unwrap()
        ));
        assert!(!csp_sources_allow(
            Some(&exact_path),
            &document,
            &Url::parse("https://cdn.example.test/redirected.js").unwrap()
        ));
        assert!(csp_sources_allow_for_redirect(
            Some(&exact_path),
            &document,
            &Url::parse("https://cdn.example.test/redirected.js").unwrap()
        ));
        assert!(!csp_sources_allow_for_redirect(
            Some(&exact_path),
            &document,
            &Url::parse("https://other.example.test/redirected.js").unwrap()
        ));

        let default_port = vec!["https://api.test".to_owned()];
        assert!(csp_sources_allow(
            Some(&default_port),
            &document,
            &Url::parse("https://api.test/").unwrap()
        ));
        assert!(!csp_sources_allow(
            Some(&default_port),
            &document,
            &Url::parse("https://api.test:8443/").unwrap()
        ));
        let explicit_port = vec!["https://api.test:8443".to_owned()];
        assert!(csp_sources_allow(
            Some(&explicit_port),
            &document,
            &Url::parse("https://api.test:8443/").unwrap()
        ));
        let any_port = vec!["https://api.test:*".to_owned()];
        assert!(csp_sources_allow(
            Some(&any_port),
            &document,
            &Url::parse("https://api.test:8443/").unwrap()
        ));

        let http_scheme = vec!["http:".to_owned()];
        assert!(csp_sources_allow(
            Some(&http_scheme),
            &document,
            &Url::parse("https://cdn.test/app.js").unwrap()
        ));
        assert!(!csp_sources_allow(
            Some(&http_scheme),
            &document,
            &Url::parse("ftp://cdn.test/app.js").unwrap()
        ));
        let https_scheme = vec!["https:".to_owned()];
        assert!(!csp_sources_allow(
            Some(&https_scheme),
            &document,
            &Url::parse("http://cdn.test/app.js").unwrap()
        ));

        let self_source = vec!["'self'".to_owned()];
        assert!(csp_sources_allow(
            Some(&self_source),
            &document,
            &Url::parse("https://app.test/secure").unwrap()
        ));
        assert!(!csp_sources_allow(
            Some(&self_source),
            &document,
            &Url::parse("https://app.test:8443/secure").unwrap()
        ));

        let mixed_none = vec!["'none'".to_owned(), "https://cdn.test".to_owned()];
        assert!(csp_sources_allow(
            Some(&mixed_none),
            &document,
            &Url::parse("https://cdn.test/app.js").unwrap()
        ));
        let malformed = vec![
            "https://user:pass@example.test".to_owned(),
            "https://example.test:bad".to_owned(),
            "https://example.test/path?query".to_owned(),
            "https://example.test/path%ZZ".to_owned(),
            "https://example.test/path;comma".to_owned(),
            "https://example.test:70000".to_owned(),
        ];
        assert!(!csp_sources_allow(
            Some(&malformed),
            &document,
            &Url::parse("https://example.test/path").unwrap()
        ));
        let oversized = vec![format!(
            "https://{}.test",
            "a".repeat(MAX_NATIVE_CSP_SOURCE_EXPRESSION_BYTES)
        )];
        assert!(!csp_sources_allow(
            Some(&oversized),
            &document,
            &Url::parse("https://example.test/path").unwrap()
        ));
    }

    #[test]
    fn csp_strict_dynamic_uses_parser_metadata_and_external_nonces() {
        let document = Url::parse("https://app.test/index.html").unwrap();
        let target = Url::parse("https://unlisted.test/app.js").unwrap();
        let mut headers = HeaderMap::new();
        headers.insert(
            CONTENT_SECURITY_POLICY,
            HeaderValue::from_static(
                "default-src 'none'; script-src 'strict-dynamic' 'nonce-Boot123' https:",
            ),
        );
        let policy = content_security_policy(&headers);

        assert!(policy.allows_script(&document, &target, true, Some("Boot123")));
        assert!(!policy.allows_script(&document, &target, true, Some("wrong")));
        assert!(!policy.allows_script(&document, &target, true, None));
        assert!(policy.allows_script(&document, &target, false, None));

        let image = Url::parse("https://unlisted.test/logo.png").unwrap();
        assert!(!policy.allows(NativeSubresourceKind::Image, &document, &image));
    }

    #[test]
    fn rooted_file_csp_self_uses_the_admitting_configured_root() {
        let document = Url::parse("file:///trusted/site/index.html").unwrap();
        let same_root = Url::parse("file:///trusted/site/app.js").unwrap();
        let other_root = Url::parse("file:///trusted/other/app.js").unwrap();
        let self_source = vec!["'self'".to_owned()];

        assert!(csp_script_sources_allow_for_rooted_file(
            Some(&self_source),
            &document,
            &same_root,
            true,
            None,
            true,
        ));
        assert!(!csp_script_sources_allow_for_rooted_file(
            Some(&self_source),
            &document,
            &other_root,
            true,
            None,
            false,
        ));

        let explicit_file_scheme = vec!["file:".to_owned()];
        assert!(csp_script_sources_allow_for_rooted_file(
            Some(&explicit_file_scheme),
            &document,
            &other_root,
            true,
            None,
            false,
        ));
    }

    #[test]
    fn rooted_file_style_csp_uses_fallback_nonce_and_root() {
        let document_url = Url::parse("file:///trusted/site/index.html").unwrap();
        let same_root_url = Url::parse("file:///trusted/site/style.css").unwrap();
        let other_root_url = Url::parse("file:///trusted/other/style.css").unwrap();
        let same_root = PathBuf::from("/trusted/site");
        let other_root = PathBuf::from("/trusted/other");

        let style_fallback = NativeCspDirectives {
            style_sources: Some(vec!["'self'".to_owned()]),
            default_sources: Some(vec!["'none'".to_owned()]),
            ..NativeCspDirectives::default()
        };
        assert!(
            style_fallback.allows_rooted_file_style(&document_url, &same_root_url, None, true,)
        );
        assert!(!style_fallback.allows_rooted_file_style(
            &document_url,
            &other_root_url,
            None,
            false,
        ));

        let element_override = NativeCspDirectives {
            style_sources: Some(vec!["'self'".to_owned()]),
            style_element_sources: Some(vec!["'none'".to_owned()]),
            ..NativeCspDirectives::default()
        };
        assert!(!element_override.allows_rooted_file_style(
            &document_url,
            &same_root_url,
            None,
            true,
        ));

        let default_fallback = NativeCspDirectives {
            default_sources: Some(vec!["'self'".to_owned()]),
            ..NativeCspDirectives::default()
        };
        assert!(default_fallback.allows_rooted_file_style(
            &document_url,
            &same_root_url,
            None,
            true,
        ));
        assert!(!default_fallback.allows_rooted_file_style(
            &document_url,
            &other_root_url,
            None,
            false,
        ));

        let nonce_policy = NativeCspDirectives {
            style_element_sources: Some(vec!["'nonce-Style123'".to_owned()]),
            ..NativeCspDirectives::default()
        };
        assert!(nonce_policy.allows_rooted_file_style(
            &document_url,
            &other_root_url,
            Some("Style123"),
            false,
        ));
        assert!(!nonce_policy.allows_rooted_file_style(
            &document_url,
            &other_root_url,
            Some("wrong"),
            false,
        ));

        let mut conjunctive = NativeCspPolicy {
            policies: vec![nonce_policy, style_fallback],
            rooted_file_self_root: Some(same_root.clone()),
            ..NativeCspPolicy::default()
        };
        assert!(conjunctive.allows_rooted_file_style(
            &document_url,
            &same_root_url,
            Some("Style123"),
            Some(&same_root),
        ));
        assert!(!conjunctive.allows_rooted_file_style(
            &document_url,
            &other_root_url,
            Some("Style123"),
            Some(&other_root),
        ));
        conjunctive.policies.clear();
        assert!(csp_sources_allow_for_rooted_file(
            Some(&["file:".to_owned()]),
            &document_url,
            &other_root_url,
            None,
            false,
        ));
    }

    #[test]
    fn rooted_file_csp_denies_script_before_reading_bytes() {
        let root = std::env::temp_dir().join(format!(
            "glass-native-rooted-file-csp-{}",
            std::process::id()
        ));
        let site = root.join("site");
        let other = root.join("other");
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&site).unwrap();
        fs::create_dir_all(&other).unwrap();
        let document_path = site.join("index.html");
        let denied_script_path = other.join("invalid-utf8.js");
        fs::write(&document_path, "<!doctype html>").unwrap();
        fs::write(&denied_script_path, [0xff]).unwrap();
        let document_url = Url::from_file_path(&document_path).unwrap().to_string();
        let denied_script_url = Url::from_file_path(&denied_script_path)
            .unwrap()
            .to_string();
        let config = NativeEngineConfig::default()
            .with_allowed_file_root(site)
            .with_allowed_file_root(other);
        let mut loader = NativeResourceLoader::new(&config).unwrap();
        loader
            .apply_meta_content_security_policies(&document_url, &["script-src 'self'".to_owned()])
            .unwrap();

        assert!(
            loader
                .load_local_file_script_with_policy(
                    &document_url,
                    &denied_script_url,
                    1024,
                    true,
                    None,
                    None,
                )
                .unwrap()
                .is_none()
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn rooted_file_style_csp_denies_css_before_reading_bytes() {
        let root = std::env::temp_dir().join(format!(
            "glass-native-rooted-file-style-csp-{}",
            std::process::id()
        ));
        let site = root.join("site");
        let other = root.join("other");
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&site).unwrap();
        fs::create_dir_all(&other).unwrap();
        let document_path = site.join("index.html");
        let denied_stylesheet_path = other.join("invalid-utf8.css");
        fs::write(&document_path, "<!doctype html>").unwrap();
        fs::write(&denied_stylesheet_path, [0xff]).unwrap();
        let document_url = Url::from_file_path(&document_path).unwrap().to_string();
        let denied_stylesheet_url = Url::from_file_path(&denied_stylesheet_path)
            .unwrap()
            .to_string();
        let config = NativeEngineConfig::default()
            .with_allowed_file_root(&site)
            .with_allowed_file_root(&other);
        let mut loader = NativeResourceLoader::new(&config).unwrap();
        loader
            .apply_meta_content_security_policies(&document_url, &["style-src 'self'".to_owned()])
            .unwrap();

        assert!(
            loader
                .load_local_file_stylesheet(&document_url, &denied_stylesheet_url, None)
                .unwrap()
                .is_none()
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn rooted_file_image_font_media_csp_uses_directives_fallback_and_roots() {
        let root = std::env::temp_dir().join(format!(
            "glass-native-rooted-file-image-font-media-csp-{}",
            std::process::id()
        ));
        let site = root.join("site");
        let other = root.join("other");
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&site).unwrap();
        fs::create_dir_all(&other).unwrap();
        fs::write(site.join("index.html"), "<!doctype html>").unwrap();
        for directory in [&site, &other] {
            fs::write(
                directory.join("image.jpg"),
                include_bytes!("../../../tests/fixtures/remote-android-concept.jpg"),
            )
            .unwrap();
            fs::write(directory.join("font.ttf"), b"bounded font bytes").unwrap();
            fs::write(directory.join("media.wav"), b"bounded media bytes").unwrap();
        }

        let document_url = Url::from_file_path(site.join("index.html"))
            .unwrap()
            .to_string();
        let other_image_url = Url::from_file_path(other.join("image.jpg"))
            .unwrap()
            .to_string();
        let other_font_url = Url::from_file_path(other.join("font.ttf"))
            .unwrap()
            .to_string();
        let other_media_url = Url::from_file_path(other.join("media.wav"))
            .unwrap()
            .to_string();
        let config = NativeEngineConfig::default()
            .with_allowed_file_root(&site)
            .with_allowed_file_root(&other);
        let mut loader = NativeResourceLoader::new(&config).unwrap();
        loader
            .apply_meta_content_security_policies(
                &document_url,
                &["default-src 'none'; img-src 'self'; font-src 'self'; media-src 'self'".into()],
            )
            .unwrap();

        assert!(
            loader
                .load_local_file_image(&document_url, "image.jpg")
                .unwrap()
                .is_some()
        );
        assert!(
            loader
                .load_font(&document_url, "font.ttf", None)
                .unwrap()
                .is_some()
        );
        assert!(
            loader
                .load_local_file_media(&document_url, "media.wav")
                .unwrap()
                .is_some()
        );
        assert!(
            loader
                .load_local_file_image(&document_url, &other_image_url)
                .unwrap()
                .is_none()
        );
        assert!(
            loader
                .load_font(&document_url, &other_font_url, None)
                .unwrap()
                .is_none()
        );
        assert!(
            loader
                .load_local_file_media(&document_url, &other_media_url)
                .unwrap()
                .is_none()
        );

        loader
            .apply_meta_content_security_policies(&document_url, &["img-src 'none'".into()])
            .unwrap();
        assert!(
            loader
                .load_local_file_image(&document_url, "image.jpg")
                .unwrap()
                .is_none()
        );

        let mut fallback_loader = NativeResourceLoader::new(&config).unwrap();
        fallback_loader
            .apply_meta_content_security_policies(&document_url, &["default-src 'self'".into()])
            .unwrap();
        assert!(
            fallback_loader
                .load_local_file_image(&document_url, "image.jpg")
                .unwrap()
                .is_some()
        );
        assert!(
            fallback_loader
                .load_font(&document_url, "font.ttf", None)
                .unwrap()
                .is_some()
        );
        assert!(
            fallback_loader
                .load_local_file_media(&document_url, "media.wav")
                .unwrap()
                .is_some()
        );
        assert!(
            fallback_loader
                .load_local_file_image(&document_url, &other_image_url)
                .unwrap()
                .is_none()
        );
        assert!(
            fallback_loader
                .load_font(&document_url, &other_font_url, None)
                .unwrap()
                .is_none()
        );
        assert!(
            fallback_loader
                .load_local_file_media(&document_url, &other_media_url)
                .unwrap()
                .is_none()
        );

        let mut file_scheme_loader = NativeResourceLoader::new(&config).unwrap();
        file_scheme_loader
            .apply_meta_content_security_policies(
                &document_url,
                &["img-src file:; font-src file:; media-src file:".into()],
            )
            .unwrap();
        assert!(
            file_scheme_loader
                .load_local_file_image(&document_url, &other_image_url)
                .unwrap()
                .is_some()
        );
        assert!(
            file_scheme_loader
                .load_font(&document_url, &other_font_url, None)
                .unwrap()
                .is_some()
        );
        assert!(
            file_scheme_loader
                .load_local_file_media(&document_url, &other_media_url)
                .unwrap()
                .is_some()
        );

        let unconfigured_root = root.join("unconfigured");
        fs::create_dir_all(&unconfigured_root).unwrap();
        let unconfigured_image = unconfigured_root.join("image.jpg");
        fs::write(
            &unconfigured_image,
            include_bytes!("../../../tests/fixtures/remote-android-concept.jpg"),
        )
        .unwrap();
        let unconfigured_image_url = Url::from_file_path(unconfigured_image).unwrap().to_string();
        assert!(matches!(
            file_scheme_loader.load_local_file_image(&document_url, &unconfigured_image_url),
            Err(NativeEngineError::UnsupportedUrl { .. })
        ));

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn rooted_file_frame_csp_uses_fallback_conjunction_and_configured_roots() {
        let root = std::env::temp_dir().join(format!(
            "glass-native-rooted-file-frame-csp-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let site = root.join("site");
        let other = root.join("other");
        let outside = root.join("outside");
        for directory in [&site, &other, &outside] {
            fs::create_dir_all(directory).unwrap();
            fs::write(directory.join("child.html"), "<title>child</title>").unwrap();
        }
        fs::write(site.join("index.html"), "<title>parent</title>").unwrap();
        let document_url = Url::from_file_path(site.join("index.html"))
            .unwrap()
            .to_string();
        let same_root_url = Url::from_file_path(site.join("child.html"))
            .unwrap()
            .to_string();
        let other_root_url = Url::from_file_path(other.join("child.html"))
            .unwrap()
            .to_string();
        let outside_root_url = Url::from_file_path(outside.join("child.html"))
            .unwrap()
            .to_string();
        let config = NativeEngineConfig::default()
            .with_allowed_file_root(&site)
            .with_allowed_file_root(&other);
        let mut loader = NativeResourceLoader::new(&config).unwrap();

        loader
            .apply_meta_content_security_policies(&document_url, &["frame-src 'self'".into()])
            .unwrap();
        assert!(
            loader
                .allows_rooted_file_frame_navigation(&document_url, &same_root_url)
                .unwrap()
        );
        assert!(
            !loader
                .allows_rooted_file_frame_navigation(&document_url, &other_root_url)
                .unwrap()
        );
        assert!(
            !loader
                .allows_rooted_file_frame_navigation(&document_url, &outside_root_url)
                .unwrap()
        );

        loader
            .apply_meta_content_security_policies(
                &document_url,
                &["child-src 'self'; default-src 'none'".into()],
            )
            .unwrap();
        assert!(
            loader
                .allows_rooted_file_frame_navigation(&document_url, &same_root_url)
                .unwrap()
        );
        assert!(
            !loader
                .allows_rooted_file_frame_navigation(&document_url, &other_root_url)
                .unwrap()
        );

        loader
            .apply_meta_content_security_policies(&document_url, &["default-src 'self'".into()])
            .unwrap();
        assert!(
            loader
                .allows_rooted_file_frame_navigation(&document_url, &same_root_url)
                .unwrap()
        );
        assert!(
            !loader
                .allows_rooted_file_frame_navigation(&document_url, &other_root_url)
                .unwrap()
        );

        loader
            .apply_meta_content_security_policies(
                &document_url,
                &["frame-src 'none'; child-src 'self'; default-src 'self'".into()],
            )
            .unwrap();
        assert!(
            !loader
                .allows_rooted_file_frame_navigation(&document_url, &same_root_url)
                .unwrap()
        );
        assert!(
            loader
                .allows_rooted_file_frame_navigation(&document_url, "about:blank")
                .unwrap()
        );

        loader
            .apply_meta_content_security_policies(
                &document_url,
                &["frame-src 'self'".into(), "frame-src 'none'".into()],
            )
            .unwrap();
        assert!(
            !loader
                .allows_rooted_file_frame_navigation(&document_url, &same_root_url)
                .unwrap()
        );

        loader
            .apply_meta_content_security_policies(&document_url, &["frame-src file:".into()])
            .unwrap();
        assert!(
            loader
                .allows_rooted_file_frame_navigation(&document_url, &other_root_url)
                .unwrap()
        );
        assert!(
            !loader
                .allows_rooted_file_frame_navigation(&document_url, &outside_root_url)
                .unwrap()
        );
        assert!(
            loader
                .allows_rooted_file_frame_navigation(&document_url, "about:blank")
                .unwrap()
        );

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn rooted_file_inline_style_csp_enforces_fallback_nonce_hash_and_conjunction() {
        let root = std::env::temp_dir().join(format!(
            "glass-native-rooted-file-inline-style-csp-{}",
            std::process::id()
        ));
        let site = root.join("site");
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&site).unwrap();
        let document_path = site.join("index.html");
        fs::write(&document_path, "<!doctype html>").unwrap();
        let document_url = Url::from_file_path(&document_path).unwrap().to_string();
        let style_element = "#element { color: red; }";
        let style_attribute = "color: blue;";
        let style_element_hash = base64::engine::general_purpose::STANDARD
            .encode(Sha256::digest(style_element.as_bytes()));
        let style_attribute_hash = base64::engine::general_purpose::STANDARD
            .encode(Sha256::digest(style_attribute.as_bytes()));
        let config = NativeEngineConfig::default().with_allowed_file_root(&site);
        let mut loader = NativeResourceLoader::new(&config).unwrap();

        loader
            .apply_meta_content_security_policies(
                &document_url,
                &[format!(
                    "default-src 'none'; style-src 'self'; style-src-elem 'nonce-Style123' 'sha256-{style_element_hash}'; style-src-attr 'unsafe-hashes' 'sha256-{style_attribute_hash}'"
                )],
            )
            .unwrap();
        assert!(
            loader
                .allows_inline_style_element(
                    &document_url,
                    "#element { color: green; }",
                    Some("Style123")
                )
                .unwrap()
        );
        assert!(
            loader
                .allows_inline_style_element(&document_url, style_element, None)
                .unwrap()
        );
        assert!(
            !loader
                .allows_inline_style_element(&document_url, "#element { color: red; } ", None)
                .unwrap()
        );
        assert!(
            loader
                .allows_inline_style_attribute(&document_url, style_attribute)
                .unwrap()
        );
        assert!(
            !loader
                .allows_inline_style_attribute(&document_url, "color: green;")
                .unwrap()
        );

        loader
            .apply_meta_content_security_policies(
                &document_url,
                &["style-src 'unsafe-inline'".to_owned()],
            )
            .unwrap();
        assert!(
            loader
                .allows_inline_style_element(&document_url, style_element, None)
                .unwrap()
        );
        assert!(
            loader
                .allows_inline_style_attribute(&document_url, style_attribute)
                .unwrap()
        );

        loader
            .apply_meta_content_security_policies(&document_url, &["default-src 'none'".to_owned()])
            .unwrap();
        assert!(
            !loader
                .allows_inline_style_element(&document_url, style_element, None)
                .unwrap()
        );
        assert!(
            !loader
                .allows_inline_style_attribute(&document_url, style_attribute)
                .unwrap()
        );

        loader
            .apply_meta_content_security_policies(
                &document_url,
                &[
                    "style-src-elem 'unsafe-inline'; style-src-attr 'unsafe-inline'".to_owned(),
                    "default-src 'none'".to_owned(),
                ],
            )
            .unwrap();
        assert!(
            !loader
                .allows_inline_style_element(&document_url, style_element, None)
                .unwrap()
        );
        assert!(
            !loader
                .allows_inline_style_attribute(&document_url, style_attribute)
                .unwrap()
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn csp_strict_dynamic_inline_precedence_and_report_only_match_enforcement() {
        let source = "globalThis.boot = true;";
        let mut headers = HeaderMap::new();
        headers.insert(
            CONTENT_SECURITY_POLICY,
            HeaderValue::from_static(
                "script-src 'unsafe-inline' 'strict-dynamic' 'nonce-Boot123'; style-src 'unsafe-inline'",
            ),
        );
        headers.insert(
            HeaderName::from_static("content-security-policy-report-only"),
            HeaderValue::from_static("script-src 'strict-dynamic' https://listed.test"),
        );
        let policy = content_security_policy(&headers);
        assert!(!policy.allows_inline(NativeInlineCspKind::ScriptElement, source, None));
        assert!(policy.allows_inline(NativeInlineCspKind::ScriptElement, source, Some("Boot123")));
        assert!(policy.allows_inline(
            NativeInlineCspKind::StyleElement,
            "body { color: red; }",
            None
        ));

        let document = Url::parse("https://app.test/index.html").unwrap();
        let target = Url::parse("https://unlisted.test/app.js").unwrap();
        assert_eq!(
            policy
                .report_only_url_violations(
                    NativeSubresourceKind::Script,
                    &document,
                    &target,
                    false,
                    None,
                )
                .len(),
            0
        );
        assert_eq!(
            policy
                .report_only_url_violations(
                    NativeSubresourceKind::Script,
                    &document,
                    &target,
                    true,
                    None,
                )
                .len(),
            1
        );
    }

    #[test]
    fn csp_header_policies_are_intersected_and_frame_groups_are_preserved() {
        let mut headers = HeaderMap::new();
        headers.append(
            CONTENT_SECURITY_POLICY,
            HeaderValue::from_static(
                "script-src https://scripts-one.test; frame-src https://frame-one.test",
            ),
        );
        headers.append(
            CONTENT_SECURITY_POLICY,
            HeaderValue::from_static(
                "script-src https://scripts-two.test; frame-src https://frame-two.test",
            ),
        );
        let policy = content_security_policy(&headers);
        let document = Url::parse("https://app.test/index.html").unwrap();
        let script_one = Url::parse("https://scripts-one.test/app.js").unwrap();
        let script_two = Url::parse("https://scripts-two.test/app.js").unwrap();
        let frame_one = Url::parse("https://frame-one.test/child").unwrap();
        let frame_two = Url::parse("https://frame-two.test/child").unwrap();
        assert!(!policy.allows(NativeSubresourceKind::Script, &document, &script_one));
        assert!(!policy.allows(NativeSubresourceKind::Script, &document, &script_two));
        assert_eq!(
            policy.frame_source_groups(),
            Some(vec![
                vec!["https://frame-one.test".into()],
                vec!["https://frame-two.test".into()],
            ])
        );
        assert!(
            !policy
                .frame_source_groups()
                .unwrap()
                .iter()
                .all(|sources| csp_sources_allow(Some(sources), &document, &frame_one))
        );
        assert!(
            !policy
                .frame_source_groups()
                .unwrap()
                .iter()
                .all(|sources| csp_sources_allow(Some(sources), &document, &frame_two))
        );
    }

    #[test]
    fn service_worker_interception_uses_the_document_connect_policy_once() {
        let mut loader = NativeResourceLoader::for_content_process(
            NativeEngineConfig::default().limits.max_document_bytes,
            None,
            &[],
        )
        .unwrap();
        let document = Url::parse("http://app.test/page").unwrap();
        let target = Url::parse("http://app.test/api").unwrap();
        loader
            .set_document_content_security_policy_from_pairs(
                document.as_str(),
                &[(
                    "Content-Security-Policy".into(),
                    "connect-src 'none'".into(),
                )],
            )
            .unwrap();
        assert!(
            loader
                .enforce_service_worker_connect_policy(&document, &target)
                .is_err()
        );

        loader
            .set_document_content_security_policy_from_pairs(
                document.as_str(),
                &[
                    (
                        "Content-Security-Policy".into(),
                        "connect-src 'self'".into(),
                    ),
                    (
                        "Content-Security-Policy-Report-Only".into(),
                        "connect-src 'none'".into(),
                    ),
                ],
            )
            .unwrap();
        loader
            .enforce_service_worker_connect_policy(&document, &target)
            .unwrap();
        loader.report_service_worker_connect_policy(&document, &target);
        let reports = loader.take_csp_violations();
        assert_eq!(reports.len(), 1);
        assert_eq!(reports[0].effective_directive, "connect-src");
        assert_eq!(reports[0].blocked_uri, target.as_str());
    }

    #[test]
    fn service_worker_font_interception_uses_the_document_font_policy() {
        let mut loader = NativeResourceLoader::for_content_process(
            NativeEngineConfig::default().limits.max_document_bytes,
            None,
            &[],
        )
        .unwrap();
        let document = Url::parse("http://app.test/page").unwrap();
        let target = Url::parse("http://app.test/font.ttf").unwrap();
        loader
            .set_document_content_security_policy_from_pairs(
                document.as_str(),
                &[("Content-Security-Policy".into(), "font-src 'none'".into())],
            )
            .unwrap();
        assert!(
            loader
                .enforce_service_worker_font_policy(&document, &target)
                .is_err()
        );

        loader
            .set_document_content_security_policy_from_pairs(
                document.as_str(),
                &[
                    ("Content-Security-Policy".into(), "font-src 'self'".into()),
                    (
                        "Content-Security-Policy-Report-Only".into(),
                        "font-src 'none'".into(),
                    ),
                ],
            )
            .unwrap();
        loader
            .enforce_service_worker_font_policy(&document, &target)
            .unwrap();
        loader.report_service_worker_font_policy(&document, &target);
        let reports = loader.take_csp_violations();
        assert_eq!(reports.len(), 1);
        assert_eq!(reports[0].effective_directive, "font-src");
        assert_eq!(reports[0].blocked_uri, target.as_str());
    }

    #[test]
    fn csp_report_delivery_selects_endpoint_format_and_rejects_unsafe_targets() {
        let document = Url::parse("https://app.test/index.html").unwrap();
        let violation = NativeResourceLoader::csp_violation_record(
            &document,
            "https://api.test/data",
            "connect-src",
            "connect-src 'none'; report-uri /legacy; report-to modern",
            "",
        );
        let mut headers = HeaderMap::new();
        headers.insert(
            HeaderName::from_static("content-security-policy-report-only"),
            HeaderValue::from_static("connect-src 'none'; report-uri /legacy; report-to modern"),
        );
        headers.insert(
            HeaderName::from_static("reporting-endpoints"),
            HeaderValue::from_static("modern=\"https://reports.test/csp\""),
        );
        let policy = content_security_policy(&headers);
        let declaration = &policy.report_only_policies[0];
        let deliveries = csp_report_deliveries_for_declaration(
            &policy.reporting_endpoints,
            declaration,
            &document,
            &violation,
        );
        assert_eq!(deliveries.len(), 1);
        assert_eq!(deliveries[0].endpoint, "https://reports.test/csp");
        assert_eq!(deliveries[0].content_type, "application/reports+json");
        let payload: serde_json::Value = serde_json::from_slice(&deliveries[0].body).unwrap();
        assert_eq!(payload[0]["type"], "csp-violation");
        assert_eq!(payload[0]["body"]["blocked-uri"], "https://api.test/data");

        let mut legacy_headers = HeaderMap::new();
        legacy_headers.insert(
            HeaderName::from_static("content-security-policy-report-only"),
            HeaderValue::from_static("connect-src 'none'; report-uri /legacy"),
        );
        let legacy_policy = content_security_policy(&legacy_headers);
        let legacy_declaration = &legacy_policy.report_only_policies[0];
        let legacy_deliveries = csp_report_deliveries_for_declaration(
            &legacy_policy.reporting_endpoints,
            legacy_declaration,
            &document,
            &violation,
        );
        assert_eq!(legacy_deliveries.len(), 1);
        assert_eq!(legacy_deliveries[0].endpoint, "https://app.test/legacy");
        assert_eq!(legacy_deliveries[0].content_type, "application/csp-report");

        let mut unsafe_headers = HeaderMap::new();
        unsafe_headers.insert(
            HeaderName::from_static("content-security-policy-report-only"),
            HeaderValue::from_static(
                "connect-src 'none'; report-uri https://user:secret@reports.test/csp; report-to missing",
            ),
        );
        let unsafe_policy = content_security_policy(&unsafe_headers);
        let unsafe_declaration = &unsafe_policy.report_only_policies[0];
        assert!(
            csp_report_deliveries_for_declaration(
                &unsafe_policy.reporting_endpoints,
                unsafe_declaration,
                &document,
                &violation,
            )
            .is_empty()
        );

        let mut legacy_group_headers = HeaderMap::new();
        legacy_group_headers.insert(
            HeaderName::from_static("content-security-policy-report-only"),
            HeaderValue::from_static("connect-src 'none'; report-to modern"),
        );
        legacy_group_headers.insert(
            HeaderName::from_static("report-to"),
            HeaderValue::from_static(
                "[{\"group\":\"modern\",\"max_age\":60,\"endpoints\":[{\"url\":\"https://reports.test/legacy-group\"}]}]",
            ),
        );
        let legacy_group_policy = content_security_policy(&legacy_group_headers);
        let legacy_group_declaration = &legacy_group_policy.report_only_policies[0];
        let legacy_group_deliveries = csp_report_deliveries_for_declaration(
            &legacy_group_policy.reporting_endpoints,
            legacy_group_declaration,
            &document,
            &violation,
        );
        assert_eq!(legacy_group_deliveries.len(), 1);
        assert_eq!(
            legacy_group_deliveries[0].endpoint,
            "https://reports.test/legacy-group"
        );
    }

    #[test]
    fn csp_meta_policies_replace_prior_document_meta_state() {
        let mut loader = NativeResourceLoader::for_content_process(
            NativeEngineConfig::default().limits.max_document_bytes,
            None,
            &[],
        )
        .unwrap();
        let document_url = "http://app.test/page";
        loader
            .set_document_content_security_policy_from_pairs(
                document_url,
                &[(
                    "Content-Security-Policy".into(),
                    "script-src 'unsafe-inline'".into(),
                )],
            )
            .unwrap();
        loader
            .apply_meta_content_security_policies(
                document_url,
                &["script-src 'nonce-first'".into()],
            )
            .unwrap();
        let first = loader.inline_script_policy(document_url).unwrap();
        assert!(first.allows("globalThis.value = 1", Some("first")));
        assert!(!first.allows("globalThis.value = 1", None));

        loader
            .apply_meta_content_security_policies(
                document_url,
                &["script-src 'nonce-second'".into()],
            )
            .unwrap();
        let second = loader.inline_script_policy(document_url).unwrap();
        assert!(!second.allows("globalThis.value = 1", Some("first")));
        assert!(second.allows("globalThis.value = 1", Some("second")));
    }

    #[test]
    fn csp_meta_policies_append_without_relaxing_prior_state() {
        let mut loader = NativeResourceLoader::for_content_process(
            NativeEngineConfig::default().limits.max_document_bytes,
            None,
            &[],
        )
        .unwrap();
        let document_url = "http://app.test/page";
        loader
            .set_document_content_security_policy_from_pairs(
                document_url,
                &[(
                    "Content-Security-Policy".into(),
                    "script-src 'unsafe-inline'".into(),
                )],
            )
            .unwrap();
        loader
            .apply_meta_content_security_policies(
                document_url,
                &["script-src 'nonce-first'".into()],
            )
            .unwrap();
        loader
            .append_meta_content_security_policies(document_url, &["script-src 'none'".into()])
            .unwrap();

        let policy = loader.inline_script_policy(document_url).unwrap();
        assert!(!policy.allows("globalThis.value = 1", Some("first")));
        assert!(!policy.allows("globalThis.value = 1", None));
    }

    #[test]
    fn csp_inline_sources_preserve_nonce_hash_and_element_directive_semantics() {
        let script = "globalThis.cspInline = true;";
        let nonce_script = "globalThis.cspNonce = true;";
        let style_element = "body { color: red; }";
        let style_attribute = "color: blue";
        let script_attribute = "globalThis.cspAttribute = true;";
        let script_hash =
            base64::engine::general_purpose::STANDARD.encode(Sha256::digest(script.as_bytes()));
        let style_element_hash = base64::engine::general_purpose::STANDARD
            .encode(Sha256::digest(style_element.as_bytes()));
        let style_attribute_hash = base64::engine::general_purpose::STANDARD
            .encode(Sha256::digest(style_attribute.as_bytes()));
        let script_attribute_hash = base64::engine::general_purpose::STANDARD
            .encode(Sha256::digest(script_attribute.as_bytes()));
        let mut headers = HeaderMap::new();
        let policy_header = format!(
            "default-src 'none'; script-src 'nonce-Wrong'; script-src-elem 'nonce-AbC123' 'sha256-{script_hash}'; script-src-attr 'unsafe-hashes' 'sha256-{script_attribute_hash}'; style-src 'none'; style-src-elem 'nonce-Style123' 'sha256-{style_element_hash}'; style-src-attr 'unsafe-hashes' 'sha256-{style_attribute_hash}'"
        );
        headers.insert(
            CONTENT_SECURITY_POLICY,
            HeaderValue::from_str(&policy_header).unwrap(),
        );
        let policy = content_security_policy(&headers);

        assert!(policy.allows_inline(NativeInlineCspKind::ScriptElement, script, Some("AbC123"),));
        assert!(!policy.allows_inline(
            NativeInlineCspKind::ScriptElement,
            nonce_script,
            Some("abc123"),
        ));
        assert!(policy.allows_inline(NativeInlineCspKind::ScriptElement, script, None));
        assert!(
            policy.allows_inline(NativeInlineCspKind::ScriptAttribute, script_attribute, None,)
        );
        assert!(!policy.allows_inline(
            NativeInlineCspKind::ScriptAttribute,
            "globalThis.cspAttribute = false;",
            None,
        ));
        assert!(policy.allows_inline(
            NativeInlineCspKind::StyleElement,
            style_element,
            Some("Style123"),
        ));
        assert!(policy.allows_inline(NativeInlineCspKind::StyleElement, style_element, None,));
        assert!(policy.allows_inline(NativeInlineCspKind::StyleAttribute, style_attribute, None,));
        assert!(!policy.allows_inline(NativeInlineCspKind::StyleAttribute, "color: green", None,));
    }

    #[test]
    fn csp_frame_src_takes_precedence_over_child_src_and_default_src() {
        let mut headers = HeaderMap::new();
        headers.insert(
            CONTENT_SECURITY_POLICY,
            HeaderValue::from_static(
                "default-src 'none'; child-src https://child.test; frame-src https://frame.test",
            ),
        );
        let policy = content_security_policy(&headers);
        let document = Url::parse("https://app.test/index.html").unwrap();
        let frame = Url::parse("https://frame.test/child").unwrap();
        let child = Url::parse("https://child.test/child").unwrap();
        assert!(policy.allows(NativeSubresourceKind::Frame, &document, &frame));
        assert!(!policy.allows(NativeSubresourceKind::Frame, &document, &child));

        let mut child_only_headers = HeaderMap::new();
        child_only_headers.insert(
            CONTENT_SECURITY_POLICY,
            HeaderValue::from_static("default-src 'none'; child-src https://child.test"),
        );
        let child_only_policy = content_security_policy(&child_only_headers);
        assert!(child_only_policy.allows(NativeSubresourceKind::Frame, &document, &child));
        assert!(!child_only_policy.allows(NativeSubresourceKind::Frame, &document, &frame));
    }

    #[test]
    fn csp_worker_creation_uses_worker_src_fallback_chain() {
        let document = Url::parse("https://app.test/index.html").unwrap();
        let worker = Url::parse("https://app.test/worker.js").unwrap();
        let cross_origin_worker = Url::parse("https://worker.test/worker.js").unwrap();
        for (header, expected_directive) in [
            (
                "worker-src 'self'; child-src 'none'; script-src 'none'; default-src 'none'",
                "worker-src",
            ),
            (
                "child-src 'self'; script-src 'none'; default-src 'none'",
                "child-src",
            ),
            ("script-src 'self'; default-src 'none'", "script-src"),
            ("default-src 'self'", "default-src"),
        ] {
            let mut headers = HeaderMap::new();
            headers.insert(
                CONTENT_SECURITY_POLICY,
                HeaderValue::from_str(header).unwrap(),
            );
            let policy = content_security_policy(&headers);
            assert!(policy.allows(NativeSubresourceKind::Worker, &document, &worker));
            assert!(!policy.allows(
                NativeSubresourceKind::Worker,
                &document,
                &cross_origin_worker
            ));
            assert_eq!(
                policy.policies[0]
                    .directive_for(NativeSubresourceKind::Worker)
                    .map(|(name, _)| name),
                Some(expected_directive)
            );
        }
    }

    #[test]
    fn csp_worker_imports_use_script_policy_and_report_only_does_not_block() {
        let document = Url::parse("https://app.test/worker.js").unwrap();
        let imported_script = Url::parse("https://app.test/imported.js").unwrap();
        let mut headers = HeaderMap::new();
        headers.insert(
            CONTENT_SECURITY_POLICY,
            HeaderValue::from_static("worker-src 'self'; script-src 'none'; default-src 'self'"),
        );
        let enforced = content_security_policy(&headers);
        assert!(enforced.allows(NativeSubresourceKind::Worker, &document, &imported_script));
        assert!(!enforced.allows(NativeSubresourceKind::Script, &document, &imported_script));
        assert_eq!(
            enforced.policies[0]
                .directive_for(NativeSubresourceKind::Script)
                .map(|(name, _)| name),
            Some("script-src")
        );

        let mut report_only_headers = HeaderMap::new();
        report_only_headers.insert(
            HeaderName::from_static("content-security-policy-report-only"),
            HeaderValue::from_static("script-src 'none'; worker-src 'self'"),
        );
        let report_only = content_security_policy(&report_only_headers);
        assert!(report_only.allows(NativeSubresourceKind::Script, &document, &imported_script));
        let violations = report_only.report_only_url_violations(
            NativeSubresourceKind::Script,
            &document,
            &imported_script,
            true,
            None,
        );
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].1, "script-src");
        assert!(
            report_only
                .report_only_url_violations(
                    NativeSubresourceKind::Worker,
                    &document,
                    &imported_script,
                    true,
                    None,
                )
                .is_empty()
        );
    }

    #[test]
    fn csp_form_action_is_explicit_and_intersected_across_headers() {
        let mut headers = HeaderMap::new();
        headers.append(
            CONTENT_SECURITY_POLICY,
            HeaderValue::from_static("default-src 'none'; form-action https://submit-one.test"),
        );
        headers.append(
            CONTENT_SECURITY_POLICY,
            HeaderValue::from_static("form-action https://submit-two.test"),
        );
        let policy = content_security_policy(&headers);
        let document = Url::parse("https://app.test/form").unwrap();
        let first = Url::parse("https://submit-one.test/receive").unwrap();
        let second = Url::parse("https://submit-two.test/receive").unwrap();
        assert!(!policy.allows_navigation(
            NativeNavigationPolicyKind::FormAction,
            &document,
            &first,
        ));
        assert!(!policy.allows_navigation(
            NativeNavigationPolicyKind::FormAction,
            &document,
            &second,
        ));

        let mut unrestricted_headers = HeaderMap::new();
        unrestricted_headers.insert(
            CONTENT_SECURITY_POLICY,
            HeaderValue::from_static("default-src 'none'"),
        );
        let unrestricted = content_security_policy(&unrestricted_headers);
        assert!(unrestricted.allows_navigation(
            NativeNavigationPolicyKind::FormAction,
            &document,
            &first,
        ));

        let mut navigation_headers = HeaderMap::new();
        navigation_headers.insert(
            CONTENT_SECURITY_POLICY,
            HeaderValue::from_static("default-src *; navigate-to https://destination.test"),
        );
        let navigation_policy = content_security_policy(&navigation_headers);
        let destination = Url::parse("https://destination.test/result").unwrap();
        assert!(navigation_policy.allows_navigation(
            NativeNavigationPolicyKind::NavigateTo,
            &document,
            &destination,
        ));
        assert!(!navigation_policy.allows_navigation(
            NativeNavigationPolicyKind::NavigateTo,
            &document,
            &first,
        ));
    }

    #[test]
    fn subresource_resolution_and_mixed_content_fail_closed() {
        let document = Url::parse("https://app.test/path/index.html").unwrap();
        let same_origin = resolve_subresource_url(&document, "../styles/site.css")
            .unwrap()
            .unwrap();
        assert_eq!(same_origin.as_str(), "https://app.test/styles/site.css");
        let insecure = resolve_subresource_url(&document, "http://cdn.test/site.css")
            .unwrap()
            .unwrap();
        assert!(!mixed_content_allowed(&document, &insecure));
        assert!(
            resolve_subresource_url(&document, "data:text/css,body%7B%7D")
                .unwrap()
                .is_none()
        );
        assert!(resolve_subresource_url(&document, "https://user:pass@cdn.test/site.css").is_err());
    }

    #[test]
    fn cors_response_requires_explicit_cross_origin_authorization() {
        let document = Url::parse("https://app.test/index.html").unwrap();
        let resource = Url::parse("https://api.test/data.json").unwrap();
        assert_eq!(
            cors_origin_header(&document, &resource, NativeCorsMode::Cors),
            Some("https://app.test".into())
        );
        assert_eq!(
            cors_origin_header(&document, &resource, NativeCorsMode::NoCors),
            None
        );

        let mut headers = HeaderMap::new();
        headers.insert(ACCESS_CONTROL_ALLOW_ORIGIN, HeaderValue::from_static("*"));
        assert!(cors_response_allowed(&headers, &document, &resource, false));
        assert!(!cors_response_allowed(&headers, &document, &resource, true));

        headers.insert(
            ACCESS_CONTROL_ALLOW_ORIGIN,
            HeaderValue::from_static("https://app.test"),
        );
        assert!(!cors_response_allowed(&headers, &document, &resource, true));
        headers.insert(
            ACCESS_CONTROL_ALLOW_CREDENTIALS,
            HeaderValue::from_static("true"),
        );
        assert!(cors_response_allowed(&headers, &document, &resource, true));
        assert!(cors_response_allowed(
            &HeaderMap::new(),
            &document,
            &document,
            true
        ));
    }

    #[test]
    fn cors_preflight_requires_allowed_method_and_requested_header() {
        let document = Url::parse("https://app.test/index.html").unwrap();
        let resource = Url::parse("https://api.test/data.json").unwrap();
        let mut headers = HeaderMap::new();
        headers.insert(
            ACCESS_CONTROL_ALLOW_ORIGIN,
            HeaderValue::from_static("https://app.test"),
        );
        headers.insert(
            ACCESS_CONTROL_ALLOW_METHODS,
            HeaderValue::from_static("POST"),
        );
        headers.insert(
            ACCESS_CONTROL_ALLOW_HEADERS,
            HeaderValue::from_static("content-type"),
        );
        assert!(cors_preflight_response_allowed(
            &headers,
            &document,
            &resource,
            "POST",
            &["content-type".into()],
            false,
        ));
        headers.remove(ACCESS_CONTROL_ALLOW_HEADERS);
        assert!(!cors_preflight_response_allowed(
            &headers,
            &document,
            &resource,
            "POST",
            &["content-type".into()],
            false,
        ));
        headers.insert(
            ACCESS_CONTROL_ALLOW_HEADERS,
            HeaderValue::from_static("content-type"),
        );
        headers.insert(
            ACCESS_CONTROL_ALLOW_METHODS,
            HeaderValue::from_static("GET"),
        );
        assert!(!cors_preflight_response_allowed(
            &headers,
            &document,
            &resource,
            "POST",
            &["content-type".into()],
            false,
        ));
    }
}
