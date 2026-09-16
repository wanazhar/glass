use super::config::{
    MAX_NATIVE_DOCUMENT_BYTES, NativeEngineConfig, canonical_fixture_url, is_network_url,
    validate_url_text, without_fragment,
};
use super::environment::NativeEnvironmentOverrides;
use super::error::NativeEngineError;
use super::image::{MAX_NATIVE_IMAGE_TRANSFER_BYTES, NativeImage, decode_image_bytes};
use super::interaction::MAX_NATIVE_FORM_BODY_BYTES;
use super::javascript::{
    MAX_NATIVE_COOKIE_PROFILE_ENTRIES, NativeCookieChange, NativeCookieProfileEntry,
    load_cookie_profile,
};
use super::origin::NativeOrigin;
use base64::Engine as _;
use futures_util::StreamExt;
use reqwest::header::HeaderMap;
use sha2::{Digest, Sha256, Sha384, Sha512};
use std::collections::BTreeMap;
use std::fmt;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use url::Url;

const MAX_NATIVE_NETWORK_REDIRECTS: usize = 8;
const NATIVE_NETWORK_TIMEOUT: Duration = Duration::from_secs(30);
const MAX_NATIVE_CACHE_ENTRIES: usize = 32;
const MAX_NATIVE_PREFLIGHT_CACHE_ENTRIES: usize = 64;
const MAX_NATIVE_PREFLIGHT_CACHE_AGE: Duration = Duration::from_secs(600);
const MAX_NATIVE_COOKIE_BYTES: usize = 4096;
const MAX_NATIVE_FETCH_HEADERS: usize = 16;
const MAX_NATIVE_FETCH_HEADER_NAME_BYTES: usize = 128;
const MAX_NATIVE_FETCH_HEADER_VALUE_BYTES: usize = 64 * 1024;
const MAX_NATIVE_FETCH_HEADER_BYTES: usize = 128 * 1024;
pub(crate) const MAX_NATIVE_RESPONSE_HEADERS: usize = 64;
pub(crate) const MAX_NATIVE_RESPONSE_HEADER_NAME_BYTES: usize = 128;
pub(crate) const MAX_NATIVE_RESPONSE_HEADER_VALUE_BYTES: usize = 64 * 1024;
pub(crate) const MAX_NATIVE_RESPONSE_HEADER_BYTES: usize = 128 * 1024;
pub(crate) const MAX_NATIVE_DOWNLOAD_BYTES: usize = 16 * 1024 * 1024;

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum NativeNavigationMethod {
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

    const fn is_bodyless(self) -> bool {
        matches!(self, Self::Get | Self::Head)
    }

    const fn is_document_method(self) -> bool {
        matches!(self, Self::Get | Self::Post)
    }

    const fn is_safe_cookie_navigation(self) -> bool {
        matches!(self, Self::Get | Self::Head)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct NativeNavigationRequest {
    pub(crate) method: NativeNavigationMethod,
    pub(crate) url: String,
    pub(crate) body: Option<NativeRequestBody>,
    pub(crate) body_content_type: Option<String>,
    pub(crate) replace_history: bool,
}

impl NativeNavigationRequest {
    pub(crate) fn get(url: impl Into<String>) -> Self {
        Self {
            method: NativeNavigationMethod::Get,
            url: url.into(),
            body: None,
            body_content_type: None,
            replace_history: false,
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
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum NativeRequestBody {
    Text(String),
    Bytes(Vec<u8>),
}

impl NativeRequestBody {
    pub(crate) fn len(&self) -> usize {
        match self {
            Self::Text(body) => body.len(),
            Self::Bytes(body) => body.len(),
        }
    }
}

pub(crate) struct NativeFetchRequest<'a> {
    pub(crate) document_url: &'a str,
    pub(crate) href: &'a str,
    pub(crate) method: NativeNavigationMethod,
    pub(crate) body: Option<NativeRequestBody>,
    pub(crate) content_type: Option<String>,
    pub(crate) request_headers: BTreeMap<String, String>,
    pub(crate) credentials: bool,
    pub(crate) cors_mode: NativeCorsMode,
    pub(crate) redirect_mode: NativeFetchRedirectMode,
    pub(crate) cache_mode: NativeFetchCacheMode,
    pub(crate) timeout: Option<Duration>,
    pub(crate) max_response_bytes: Option<usize>,
}

pub(crate) struct NativeWebSocketTarget {
    pub(crate) url: Url,
    pub(crate) cookie: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct NativeScriptResource {
    pub(crate) url: String,
    pub(crate) body: String,
}

/// A bounded response returned by the native GET/fetch primitive.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NativeFetchResponse {
    pub url: String,
    pub status: u16,
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
    max_document_bytes: usize,
    network: NativeNetworkState,
    environment: NativeEnvironmentOverrides,
    cookie_changes: Vec<NativeCookieChange>,
}

impl fmt::Debug for NativeResourceLoader {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("NativeResourceLoader")
            .field("fixture_count", &self.fixtures.len())
            .field("max_document_bytes", &self.max_document_bytes)
            .field("cached_document_count", &self.network.cache.len())
            .field("cached_image_count", &self.network.image_cache.len())
            .field(
                "cached_stylesheet_count",
                &self.network.stylesheet_cache.len(),
            )
            .field("cached_script_count", &self.network.script_cache.len())
            .field("cached_fetch_count", &self.network.fetch_cache.len())
            .field("cookie_count", &self.network.cookies.len())
            .field(
                "document_policy_count",
                &self.network.document_policies.len(),
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

#[derive(Debug, Clone, PartialEq, Eq, Default)]
struct NativeNetworkState {
    cache: BTreeMap<String, NativeDocumentCacheEntry>,
    image_cache: BTreeMap<String, NativeImageCacheEntry>,
    stylesheet_cache: BTreeMap<String, NativeTextCacheEntry>,
    script_cache: BTreeMap<String, NativeTextCacheEntry>,
    fetch_cache: BTreeMap<String, NativeFetchCacheEntry>,
    cookies: Vec<NativeCookie>,
    document_policies: BTreeMap<String, NativeCspPolicy>,
    preflight_cache: BTreeMap<String, Instant>,
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
    fresh_until: Option<Instant>,
    etag: Option<String>,
    last_modified: Option<String>,
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
struct NativeCspPolicy {
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
    default_sources: Option<Vec<String>>,
}

impl NativeCspPolicy {
    fn allows(&self, kind: NativeSubresourceKind, document_url: &Url, resource_url: &Url) -> bool {
        let sources = self.sources_for(kind).or(self.default_sources.as_ref());
        csp_sources_allow(sources.map(Vec::as_slice), document_url, resource_url)
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
            NativeSubresourceKind::Worker => self.worker_sources.as_ref(),
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

    fn allows_inline(&self, kind: NativeInlineCspKind, source: &str, nonce: Option<&str>) -> bool {
        inline_csp_sources_allow(
            self.inline_sources_for(kind).map(Vec::as_slice),
            source,
            nonce,
            matches!(
                kind,
                NativeInlineCspKind::ScriptAttribute | NativeInlineCspKind::StyleAttribute
            ),
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct NativeInlineScriptPolicy {
    element_sources: Option<Vec<String>>,
    attribute_sources: Option<Vec<String>>,
}

impl NativeInlineScriptPolicy {
    pub(crate) fn allows(&self, source: &str, nonce: Option<&str>) -> bool {
        inline_csp_sources_allow(self.element_sources.as_deref(), source, nonce, false)
    }

    pub(crate) fn allows_attribute(&self, source: &str) -> bool {
        inline_csp_sources_allow(self.attribute_sources.as_deref(), source, None, true)
    }
}

/// Evaluate the bounded source-expression subset shared by document
/// subresources and parent-owned frame navigation. Keeping this matcher in
/// the loader prevents the content worker and frame registry from drifting
/// into different CSP decisions.
pub(crate) fn csp_sources_allow(
    sources: Option<&[String]>,
    document_url: &Url,
    resource_url: &Url,
) -> bool {
    let Some(sources) = sources else {
        return true;
    };
    if sources.is_empty() {
        return false;
    }
    let document_origin = document_url.origin();
    for source in sources {
        if source.eq_ignore_ascii_case("'none'") {
            return false;
        }
        if source == "*" {
            return true;
        }
        if source.eq_ignore_ascii_case("'self'") && resource_url.origin() == document_origin {
            return true;
        }
        if source.ends_with(':')
            && source[..source.len().saturating_sub(1)].eq_ignore_ascii_case(resource_url.scheme())
        {
            return true;
        }
        if Url::parse(source).is_ok_and(|source_url| source_url.origin() == resource_url.origin()) {
            return true;
        }
    }
    false
}

fn inline_csp_sources_allow(
    sources: Option<&[String]>,
    source: &str,
    nonce: Option<&str>,
    style_attribute: bool,
) -> bool {
    let Some(sources) = sources else {
        return true;
    };
    if sources.is_empty() {
        return false;
    }
    if sources
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

impl NativeResourceLoader {
    pub(crate) fn new(config: &NativeEngineConfig) -> Result<Self, NativeEngineError> {
        config.validate()?;
        let fixtures = config
            .fixtures
            .iter()
            .map(|fixture| (fixture.url.clone(), fixture.html.clone()))
            .collect();
        let cookies = load_cookie_profile(config.storage_path.as_deref())?;
        Ok(Self {
            fixtures,
            max_document_bytes: config.limits.max_document_bytes,
            network: NativeNetworkState::from_profile(cookies)?,
            environment: NativeEnvironmentOverrides::default(),
            cookie_changes: Vec::new(),
        })
    }

    pub(crate) fn for_content_process(
        max_document_bytes: usize,
        storage_path: Option<&std::path::Path>,
    ) -> Result<Self, NativeEngineError> {
        if max_document_bytes == 0 || max_document_bytes > MAX_NATIVE_DOCUMENT_BYTES {
            return Err(NativeEngineError::invalid(
                "content-process document limit",
                format!("must be between 1 and {MAX_NATIVE_DOCUMENT_BYTES}"),
            ));
        }
        let cookies = load_cookie_profile(storage_path)?;
        Ok(Self {
            fixtures: BTreeMap::new(),
            max_document_bytes,
            network: NativeNetworkState::from_profile(cookies)?,
            environment: NativeEnvironmentOverrides::default(),
            cookie_changes: Vec::new(),
        })
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

    pub(crate) fn inline_script_policy(
        &self,
        document_url: &str,
    ) -> Result<NativeInlineScriptPolicy, NativeEngineError> {
        Ok(self
            .document_policy(document_url)?
            .map(|policy| NativeInlineScriptPolicy {
                element_sources: policy
                    .inline_sources_for(NativeInlineCspKind::ScriptElement)
                    .cloned(),
                attribute_sources: policy
                    .inline_sources_for(NativeInlineCspKind::ScriptAttribute)
                    .cloned(),
            })
            .unwrap_or_default())
    }

    pub(crate) fn allows_inline_script(
        &self,
        document_url: &str,
        source: &str,
        nonce: Option<&str>,
    ) -> Result<bool, NativeEngineError> {
        Ok(self.document_policy(document_url)?.is_none_or(|policy| {
            policy.allows_inline(NativeInlineCspKind::ScriptElement, source, nonce)
        }))
    }

    pub(crate) fn allows_inline_style_element(
        &self,
        document_url: &str,
        source: &str,
        nonce: Option<&str>,
    ) -> Result<bool, NativeEngineError> {
        Ok(self.document_policy(document_url)?.is_none_or(|policy| {
            policy.allows_inline(NativeInlineCspKind::StyleElement, source, nonce)
        }))
    }

    pub(crate) fn allows_inline_style_attribute(
        &self,
        document_url: &str,
        source: &str,
    ) -> Result<bool, NativeEngineError> {
        Ok(self.document_policy(document_url)?.is_none_or(|policy| {
            policy.allows_inline(NativeInlineCspKind::StyleAttribute, source, None)
        }))
    }

    fn document_policy(
        &self,
        document_url: &str,
    ) -> Result<Option<&NativeCspPolicy>, NativeEngineError> {
        validate_url_text("CSP document URL", document_url)?;
        let document_url = Url::parse(without_fragment(document_url)).map_err(|_| {
            NativeEngineError::UnsupportedUrl {
                reason: "CSP document URL is not valid URL syntax".into(),
            }
        })?;
        if !is_network_url(document_url.as_str()) {
            return Ok(None);
        }
        reject_credentials(&document_url)?;
        Ok(self
            .network
            .document_policies
            .get(&cache_key(&document_url)))
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
        })
    }

    /// Open one EventSource response through the shared URL, CSP, mixed
    /// content, referrer, redirect, cookie, and CORS policy owner. The
    /// response body is intentionally returned as a stream to the content
    /// process; buffering it here would defeat Server-Sent Events semantics.
    pub(crate) async fn open_event_source_async(
        &mut self,
        document_url: &str,
        href: &str,
        with_credentials: bool,
        last_event_id: &str,
    ) -> Result<(Url, reqwest::Response), NativeEngineError> {
        validate_url_text("EventSource owner URL", document_url)?;
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
        let document_url = Url::parse(without_fragment(document_url)).map_err(|_| {
            NativeEngineError::UnsupportedUrl {
                reason: "EventSource owner URL is not valid HTTP(S) syntax".into(),
            }
        })?;
        if !is_network_url(document_url.as_str()) {
            return Err(NativeEngineError::UnsupportedUrl {
                reason: "EventSource requires an HTTP(S) document owner".into(),
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
                || !policy.allows(NativeSubresourceKind::Connect, &document_url, &next_url)
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
    ) -> Result<Option<Vec<String>>, NativeEngineError> {
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
            .and_then(|policy| {
                policy
                    .sources_for(NativeSubresourceKind::Frame)
                    .or(policy.default_sources.as_ref())
                    .cloned()
            }))
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

    pub(crate) fn clear_cookies(&mut self) {
        self.network.clear_cookies(&mut self.cookie_changes);
    }

    pub(crate) fn take_cookie_changes(&mut self) -> Vec<NativeCookieChange> {
        std::mem::take(&mut self.cookie_changes)
    }

    pub(crate) fn apply_cookie_changes(
        &mut self,
        changes: &[NativeCookieChange],
    ) -> Result<(), NativeEngineError> {
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
        let referrer = normalize_referrer(referrer, &parsed)?;
        let navigation_initiator = referrer
            .as_deref()
            .and_then(|value| Url::parse(without_fragment(value)).ok());
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
        let mut request_referrer = referrer;
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
                navigation_initiator.as_ref(),
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
            request_referrer = normalize_referrer(Some(current_url.as_str()), &next_url)?;
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
            method,
            body: body.map(NativeRequestBody::Text),
            content_type,
            request_headers: BTreeMap::new(),
            credentials,
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
        self.fetch_request_with_headers_async(NativeFetchRequest {
            document_url,
            href,
            method: NativeNavigationMethod::Get,
            body: None,
            content_type: None,
            request_headers: BTreeMap::new(),
            credentials: true,
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
        let opened = self.open_fetch_response_stream_async(request).await?;
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
        let NativeFetchRequest {
            document_url,
            href,
            method,
            body,
            content_type,
            request_headers,
            credentials,
            cors_mode,
            redirect_mode,
            cache_mode,
            timeout,
            max_response_bytes,
        } = request;
        let max_response_bytes = max_response_bytes.unwrap_or(self.max_document_bytes);
        if max_response_bytes == 0 || max_response_bytes > MAX_NATIVE_DOWNLOAD_BYTES {
            return Err(NativeEngineError::invalid(
                "fetch response limit",
                format!("must be between 1 and {MAX_NATIVE_DOWNLOAD_BYTES}"),
            ));
        }
        validate_url_text("fetch owner URL", document_url)?;
        validate_url_text("fetch URL", href)?;
        let mut current_headers = validate_fetch_request_headers(&request_headers)?;
        if current_headers.contains_key("content-type") {
            return Err(NativeEngineError::invalid(
                "fetch request headers",
                "content-type must use the dedicated content type field",
            ));
        }
        match method {
            method if method.is_bodyless() && (body.is_some() || content_type.is_some()) => {
                return Err(NativeEngineError::invalid(
                    format!("{} fetch body", method.as_str()),
                    "must be absent",
                ));
            }
            _ => {
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
            }
        }
        let document_url = Url::parse(without_fragment(document_url)).map_err(|_| {
            NativeEngineError::UnsupportedUrl {
                reason: "fetch owner URL is not valid HTTP(S) syntax".into(),
            }
        })?;
        if !is_network_url(document_url.as_str()) {
            return Err(NativeEngineError::UnsupportedUrl {
                reason: "native fetch requires an HTTP(S) document owner".into(),
            });
        }
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
        if !mixed_content_allowed(&document_url, &target_url) {
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
        let cacheable_request = matches!(
            method,
            NativeNavigationMethod::Get | NativeNavigationMethod::Head
        ) && body.is_none()
            && redirect_mode == NativeFetchRedirectMode::Follow
            && cors_mode != NativeCorsMode::Navigation;
        let request_cookie = if cacheable_request && credentials {
            self.network
                .cookie_header_for_request(&target_url, Some(&document_url), false, method)
        } else {
            None
        };
        let fetch_cache_key = cacheable_request.then(|| {
            fetch_response_cache_key(
                &document_url,
                &target_url,
                method,
                &current_headers,
                content_type.as_deref(),
                credentials,
                cors_mode,
                request_cookie.as_deref(),
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
        let mut current_content_type = content_type;
        let mut request_referrer = normalize_referrer(Some(document_url.as_str()), &current_url)?;
        let mut redirects = 0;
        let mut redirected = false;
        let mut no_cors_cross_origin = cross_origin;
        let mut pending_cookies = Vec::new();
        let response = loop {
            let mut request_url = current_url.clone();
            request_url.set_fragment(None);
            let requested_headers =
                cors_preflight_request_headers(current_content_type.as_deref(), &current_headers);
            let cross_origin_request = document_url.origin() != current_url.origin();
            let simple_method = matches!(
                current_method,
                NativeNavigationMethod::Get
                    | NativeNavigationMethod::Head
                    | NativeNavigationMethod::Post
            );
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
                    current_method,
                    &requested_headers,
                    credentials,
                )
                .await?;
            }
            let mut request = self.apply_environment_headers(
                client
                    .request(current_method.reqwest_method(), request_url)
                    .header(reqwest::header::ACCEPT, "*/*"),
            );
            if let Some(body) = current_body.as_ref() {
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
            if credentials
                && let Some(cookie) = self.network.cookie_header_for_request(
                    &current_url,
                    Some(&document_url),
                    false,
                    current_method,
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
            if !is_network_url(without_fragment(next_url.as_str()))
                || !mixed_content_allowed(&document_url, &next_url)
                || (cors_mode != NativeCorsMode::Navigation
                    && !policy.allows(NativeSubresourceKind::Connect, &document_url, &next_url))
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
            request_referrer = normalize_referrer(Some(current_url.as_str()), &next_url)?;
            if current_url.origin() != next_url.origin() {
                current_headers.remove("authorization");
            }
            no_cors_cross_origin |= document_url.origin() != next_url.origin();
            if matches!(response.status().as_u16(), 301..=303)
                && !matches!(
                    current_method,
                    NativeNavigationMethod::Get | NativeNavigationMethod::Head
                )
            {
                current_method = NativeNavigationMethod::Get;
                current_body = None;
                current_content_type = None;
            }
            current_url = next_url;
            redirects += 1;
            redirected = true;
        };
        let final_url = current_url;
        let status = response.status().as_u16();
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
            && !cors_response_allowed(&response_headers, &document_url, &final_url, credentials)
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
                document_url.origin() == final_url.origin(),
                credentials,
            )?
        };
        let mut fetch_response = NativeFetchResponse {
            url: without_fragment(final_url.as_str()).to_owned(),
            status,
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
        method: NativeNavigationMethod,
        requested_headers: &[String],
        credentials: bool,
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

    pub(crate) async fn load_stylesheet_async(
        &mut self,
        document_url: &str,
        href: &str,
    ) -> Result<Option<String>, NativeEngineError> {
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
        let Some(target_url) = resolve_subresource_url(&document_url, href)? else {
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
        if !policy.allows(NativeSubresourceKind::Style, &document_url, &target_url) {
            return Ok(None);
        }
        let requested_cache_key = cache_key(&target_url);
        let stale_cached_stylesheet = self
            .network
            .stylesheet_cache
            .get(&requested_cache_key)
            .cloned()
            .filter(|cached| !cached.is_fresh(Instant::now()));
        if let Some(cached) = self.network.stylesheet_cache.get(&requested_cache_key)
            && cached.is_fresh(Instant::now())
        {
            return Ok(Some(cached.body.clone()));
        }

        let client = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(NATIVE_NETWORK_TIMEOUT)
            .build()
            .map_err(|error| network_error("CSS client construction", error))?;
        let mut current_url = target_url;
        let mut request_referrer = normalize_referrer(Some(document_url.as_str()), &current_url)?;
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
            if !is_network_url(without_fragment(next_url.as_str()))
                || !mixed_content_allowed(&document_url, &next_url)
                || !policy.allows(NativeSubresourceKind::Style, &document_url, &next_url)
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
            let cached_body = cached.body.clone();
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
            return Ok(Some(cached_body));
        }
        if !response.status().is_success() {
            return Err(NativeEngineError::Network {
                operation: "CSS subresource request".into(),
                reason: format!("server returned HTTP {}", response.status().as_u16()),
            });
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
        Ok(Some(body))
    }

    pub(crate) async fn load_image_async(
        &mut self,
        document_url: &str,
        src: &str,
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
        let Some(target_url) = resolve_subresource_url(&document_url, src)? else {
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
        if !policy.allows(NativeSubresourceKind::Image, &document_url, &target_url) {
            return Ok(None);
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
        let mut request_referrer = normalize_referrer(Some(document_url.as_str()), &current_url)?;
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
            if !is_network_url(without_fragment(next_url.as_str()))
                || !mixed_content_allowed(&document_url, &next_url)
                || !policy.allows(NativeSubresourceKind::Image, &document_url, &next_url)
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

    pub(crate) async fn load_script_async(
        &mut self,
        document_url: &str,
        href: &str,
        max_source_bytes: usize,
    ) -> Result<Option<NativeScriptResource>, NativeEngineError> {
        self.load_script_like_async(
            document_url,
            href,
            max_source_bytes,
            NativeSubresourceKind::Script,
        )
        .await
    }

    pub(crate) async fn load_worker_async(
        &mut self,
        document_url: &str,
        href: &str,
        max_source_bytes: usize,
    ) -> Result<Option<NativeScriptResource>, NativeEngineError> {
        self.load_script_like_async(
            document_url,
            href,
            max_source_bytes,
            NativeSubresourceKind::Worker,
        )
        .await
    }

    async fn load_script_like_async(
        &mut self,
        document_url: &str,
        href: &str,
        max_source_bytes: usize,
        subresource_kind: NativeSubresourceKind,
    ) -> Result<Option<NativeScriptResource>, NativeEngineError> {
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
                reason: "script owner URL is not valid HTTP(S) syntax".into(),
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
            (target_url.scheme() == "fixture").then_some(target_url)
        } else {
            resolve_subresource_url(&document_url, href)?
        };
        let Some(target_url) = target_url else {
            return Ok(None);
        };
        if !is_network_url(document_url.as_str()) {
            if subresource_kind == NativeSubresourceKind::Worker && target_url.scheme() == "fixture"
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
                }));
            }
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
        if !policy.allows(subresource_kind, &document_url, &target_url) {
            return Ok(None);
        }
        let requested_cache_key = cache_key(&target_url);
        let stale_cached_script = (subresource_kind == NativeSubresourceKind::Script)
            .then(|| {
                self.network
                    .script_cache
                    .get(&requested_cache_key)
                    .cloned()
                    .filter(|cached| !cached.is_fresh(Instant::now()))
            })
            .flatten();
        if subresource_kind == NativeSubresourceKind::Script
            && let Some(cached) = self.network.script_cache.get(&requested_cache_key)
            && cached.is_fresh(Instant::now())
        {
            return Ok(Some(NativeScriptResource {
                url: cached.url.clone(),
                body: cached.body.clone(),
            }));
        }

        let client = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(NATIVE_NETWORK_TIMEOUT)
            .build()
            .map_err(|error| network_error("script client construction", error))?;
        let mut current_url = target_url;
        let mut request_referrer = normalize_referrer(Some(document_url.as_str()), &current_url)?;
        let mut redirects = 0;
        let mut pending_cookies = Vec::new();
        let response = loop {
            let mut request_url = current_url.clone();
            request_url.set_fragment(None);
            let mut request = self.apply_environment_headers(client.get(request_url).header(
                reqwest::header::ACCEPT,
                "text/javascript, application/javascript, application/ecmascript, */*",
            ));
            if let Some(referrer) = request_referrer.as_deref() {
                request = request.header(reqwest::header::REFERER, referrer);
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
                .map_err(|error| network_error("script subresource request", error))?;
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
            if !is_network_url(without_fragment(next_url.as_str()))
                || !mixed_content_allowed(&document_url, &next_url)
                || !policy.allows(subresource_kind, &document_url, &next_url)
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
            for (cookie_url, cookie) in pending_cookies {
                self.cookie_changes
                    .extend(self.network.store_cookie(&cookie_url, &cookie));
            }
            let cached_resource = NativeScriptResource {
                url: cached.url.clone(),
                body: cached.body.clone(),
            };
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
        if !script_content_type_allowed(response.headers().get(reqwest::header::CONTENT_TYPE))? {
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
        let body = String::from_utf8(bytes).map_err(|_| NativeEngineError::Network {
            operation: "script subresource decoding".into(),
            reason: "script subresource is not valid UTF-8".into(),
        })?;
        for (cookie_url, cookie) in pending_cookies {
            self.cookie_changes
                .extend(self.network.store_cookie(&cookie_url, &cookie));
        }
        let resource = NativeScriptResource {
            url: current_url.to_string(),
            body,
        };
        if subresource_kind == NativeSubresourceKind::Script {
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
            policy.style_sources = Some(Vec::new());
            continue;
        };
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
                "default-src" => policy.default_sources = Some(sources),
                _ => {}
            }
        }
    }
    policy
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
    Ok(if media_type.eq_ignore_ascii_case("image/png") {
        Some("image/png")
    } else if media_type.eq_ignore_ascii_case("image/jpeg") {
        Some("image/jpeg")
    } else if media_type.eq_ignore_ascii_case("image/webp") {
        Some("image/webp")
    } else if media_type.eq_ignore_ascii_case("image/gif") {
        Some("image/gif")
    } else if media_type.eq_ignore_ascii_case("image/apng") {
        Some("image/apng")
    } else if media_type.eq_ignore_ascii_case("image/svg+xml") {
        Some("image/svg+xml")
    } else {
        None
    })
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
    Ok(matches!(
        value
            .split(';')
            .next()
            .unwrap_or_default()
            .trim()
            .to_ascii_lowercase()
            .as_str(),
        "text/javascript"
            | "application/javascript"
            | "application/ecmascript"
            | "text/ecmascript"
            | "application/x-javascript"
    ))
}

pub(crate) fn referrer_for_navigation(
    current_url: &str,
    target_url: &str,
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
    normalize_referrer(Some(current.as_str()), &target)
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

fn cache_key(url: &Url) -> String {
    let mut key = url.clone();
    key.set_fragment(None);
    key.to_string()
}

fn fetch_response_cache_key(
    document_url: &Url,
    target_url: &Url,
    method: NativeNavigationMethod,
    request_headers: &BTreeMap<String, String>,
    content_type: Option<&str>,
    credentials: bool,
    cors_mode: NativeCorsMode,
    request_cookie: Option<&str>,
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
    ] {
        append_fetch_cache_key_part(&mut key, &value);
    }
    for (name, value) in request_headers {
        append_fetch_cache_key_part(&mut key, name);
        append_fetch_cache_key_part(&mut key, value);
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

    fn cookie_header_for_request(
        &self,
        url: &Url,
        initiator_url: Option<&Url>,
        top_level_navigation: bool,
        method: NativeNavigationMethod,
    ) -> Option<String> {
        self.cookie_header_with_visibility(url, true, initiator_url, top_level_navigation, method)
    }

    fn matching_cookies(&self, url: &Url, include_http_only: bool) -> Vec<&NativeCookie> {
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

    fn cookie_header_with_visibility(
        &self,
        url: &Url,
        include_http_only: bool,
        initiator_url: Option<&Url>,
        top_level_navigation: bool,
        method: NativeNavigationMethod,
    ) -> Option<String> {
        let matching = self
            .matching_cookies(url, include_http_only)
            .into_iter()
            .filter(|cookie| {
                cookie_same_site_allows(cookie, url, initiator_url, top_level_navigation, method)
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
        let before = self.cookies.len();
        self.cookies
            .retain(|cookie| cookie.name != name || cookie.domain != domain || cookie.path != path);
        self.cookies.len() != before
    }

    fn clear_cookies(&mut self, changes: &mut Vec<NativeCookieChange>) {
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
        if line.len() > MAX_NATIVE_COOKIE_BYTES {
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

    fn store_document_policy(&mut self, key: String, policy: NativeCspPolicy) {
        if !self.document_policies.contains_key(&key)
            && self.document_policies.len() >= MAX_NATIVE_CACHE_ENTRIES
            && let Some(oldest) = self.document_policies.keys().next().cloned()
        {
            self.document_policies.remove(&oldest);
        }
        self.document_policies.insert(key, policy);
    }
}

fn cookie_same_site_allows(
    cookie: &NativeCookie,
    request_url: &Url,
    initiator_url: Option<&Url>,
    top_level_navigation: bool,
    method: NativeNavigationMethod,
) -> bool {
    let same_site = initiator_url.is_none_or(|initiator| {
        schemeful_site(initiator).is_some()
            && schemeful_site(initiator) == schemeful_site(request_url)
    });
    match cookie_same_site(cookie.same_site.as_deref()) {
        NativeCookieSameSite::None => true,
        NativeCookieSameSite::Strict => same_site,
        NativeCookieSameSite::Lax => {
            same_site || (top_level_navigation && method.is_safe_cookie_navigation())
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
    use super::super::javascript::{
        NativeIndexedDbState, NativeWebStorageState, save_web_storage_profile,
    };
    use super::{
        MAX_NATIVE_CACHE_ENTRIES, NativeCookieProfileEntry, NativeCorsMode, NativeEngineConfig,
        NativeInlineCspKind, NativeNavigationMethod, NativeNetworkState, NativeResource,
        NativeResourceLoader, NativeSubresourceKind, cache_control_max_age,
        cache_control_requires_revalidation, content_security_policy, cors_origin_header,
        cors_preflight_response_allowed, cors_response_allowed, decode_html_body,
        document_cache_fresh_until, document_cache_storage_allowed, mixed_content_allowed,
        referrer_for_navigation, resolve_subresource_url,
    };
    use base64::Engine as _;
    use reqwest::header::{
        ACCESS_CONTROL_ALLOW_CREDENTIALS, ACCESS_CONTROL_ALLOW_HEADERS,
        ACCESS_CONTROL_ALLOW_METHODS, ACCESS_CONTROL_ALLOW_ORIGIN, CACHE_CONTROL,
        CONTENT_SECURITY_POLICY, HeaderMap, HeaderValue, PRAGMA, VARY,
    };
    use sha2::{Digest, Sha256};
    use std::fs;
    use std::time::Instant;
    use url::Url;

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
        let lock_path = profile_path.with_extension("lock");
        let _ = fs::remove_file(&profile_path);
        let _ = fs::remove_file(&lock_path);
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
        let _ = fs::remove_file(profile_path);
        let _ = fs::remove_file(lock_path);
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
            Some("http://source.test".into())
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
