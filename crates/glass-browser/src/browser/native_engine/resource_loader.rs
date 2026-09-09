use super::config::{
    MAX_NATIVE_DOCUMENT_BYTES, NativeEngineConfig, canonical_fixture_url, is_network_url,
    validate_url_text, without_fragment,
};
use super::error::NativeEngineError;
use super::origin::NativeOrigin;
use base64::Engine as _;
use futures_util::StreamExt;
use reqwest::header::HeaderMap;
use std::collections::BTreeMap;
use std::fmt;
use std::time::{Duration, Instant};
use url::Url;

const MAX_NATIVE_NETWORK_REDIRECTS: usize = 8;
const NATIVE_NETWORK_TIMEOUT: Duration = Duration::from_secs(30);
const MAX_NATIVE_CACHE_ENTRIES: usize = 32;
const MAX_NATIVE_COOKIES: usize = 128;
const MAX_NATIVE_COOKIE_BYTES: usize = 4096;

/// A bounded HTML resource accepted by the native engine.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NativeResource {
    pub url: String,
    pub origin: NativeOrigin,
    pub body: String,
}

/// Bounded resource loader for local documents and HTTP(S) HTML responses.
#[derive(Clone, PartialEq, Eq)]
pub struct NativeResourceLoader {
    fixtures: BTreeMap<String, String>,
    max_document_bytes: usize,
    network: NativeNetworkState,
}

impl fmt::Debug for NativeResourceLoader {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("NativeResourceLoader")
            .field("fixture_count", &self.fixtures.len())
            .field("max_document_bytes", &self.max_document_bytes)
            .field("cached_document_count", &self.network.cache.len())
            .field("cookie_count", &self.network.cookies.len())
            .field(
                "document_policy_count",
                &self.network.document_policies.len(),
            )
            .finish()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
struct NativeNetworkState {
    cache: BTreeMap<String, NativeResource>,
    cookies: Vec<NativeCookie>,
    document_policies: BTreeMap<String, NativeCspPolicy>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct NativeCookie {
    name: String,
    value: String,
    domain: String,
    path: String,
    host_only: bool,
    secure: bool,
    expires_at: Option<Instant>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
struct NativeCspPolicy {
    style_sources: Option<Vec<String>>,
    default_sources: Option<Vec<String>>,
}

impl NativeCspPolicy {
    fn allows_style(&self, document_url: &Url, resource_url: &Url) -> bool {
        let Some(sources) = self
            .style_sources
            .as_ref()
            .or(self.default_sources.as_ref())
        else {
            return true;
        };
        if sources.is_empty() {
            return false;
        }
        let document_origin = document_url.origin();
        for source in sources {
            if source == "'none'" {
                return false;
            }
            if source == "*" {
                return true;
            }
            if source == "'self'" && resource_url.origin() == document_origin {
                return true;
            }
            if source.ends_with(':')
                && source[..source.len().saturating_sub(1)]
                    .eq_ignore_ascii_case(resource_url.scheme())
            {
                return true;
            }
            if Url::parse(source)
                .is_ok_and(|source_url| source_url.origin() == resource_url.origin())
            {
                return true;
            }
        }
        false
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
        Ok(Self {
            fixtures,
            max_document_bytes: config.limits.max_document_bytes,
            network: NativeNetworkState::default(),
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
            network: NativeNetworkState::default(),
        })
    }

    pub(crate) fn max_document_bytes(&self) -> usize {
        self.max_document_bytes
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
        if let Some(cached) = self.network.cache.get(&cache_key(&parsed)).cloned() {
            return with_original_fragment(cached, url);
        }
        let client = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(NATIVE_NETWORK_TIMEOUT)
            .build()
            .map_err(|error| network_error("HTTP client construction", error))?;
        let mut current_url = parsed.clone();
        let mut request_referrer = referrer;
        let mut redirects = 0;
        let mut pending_cookies = Vec::new();
        let response = loop {
            let mut request_url = current_url.clone();
            request_url.set_fragment(None);
            let mut request = client
                .get(request_url)
                .header(reqwest::header::ACCEPT, "text/html,application/xhtml+xml");
            if let Some(referrer) = request_referrer.as_deref() {
                request = request.header(reqwest::header::REFERER, referrer);
            }
            if let Some(cookie) = self.network.cookie_header(&current_url) {
                request = request.header(reqwest::header::COOKIE, cookie);
            }
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
            current_url = next_url;
            redirects += 1;
        };
        if !response.status().is_success() {
            return Err(NativeEngineError::Network {
                operation: "HTTP document request".into(),
                reason: format!("server returned HTTP {}", response.status().as_u16()),
            });
        }
        let charset = content_type_charset(response.headers().get(reqwest::header::CONTENT_TYPE))?;
        let cacheable = cacheable_response(response.headers());
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
        let response_headers = response.headers().clone();
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
        let has_set_cookie = !pending_cookies.is_empty();
        for (cookie_url, cookie) in pending_cookies {
            self.network.store_cookie(&cookie_url, &cookie);
        }
        self.network.store_document_policy(
            cache_key(&final_url),
            content_security_policy(&response_headers),
        );
        if cacheable && !has_set_cookie {
            self.network.store_cache(cache_key(&parsed), cache_resource);
        }
        Ok(resource)
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
        let target_url =
            document_url
                .join(href)
                .map_err(|_| NativeEngineError::UnsupportedUrl {
                    reason: "stylesheet URL could not be resolved against the document".into(),
                })?;
        reject_credentials(&target_url)?;
        if !is_network_url(without_fragment(target_url.as_str()))
            || (document_url.scheme().eq_ignore_ascii_case("https")
                && target_url.scheme().eq_ignore_ascii_case("http"))
        {
            return Ok(None);
        }
        let policy = self
            .network
            .document_policies
            .get(&cache_key(&document_url))
            .cloned()
            .unwrap_or_default();
        if !policy.allows_style(&document_url, &target_url) {
            return Ok(None);
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
            let mut request = client
                .get(request_url)
                .header(reqwest::header::ACCEPT, "text/css");
            if let Some(referrer) = request_referrer.as_deref() {
                request = request.header(reqwest::header::REFERER, referrer);
            }
            if let Some(cookie) = self.network.cookie_header(&current_url) {
                request = request.header(reqwest::header::COOKIE, cookie);
            }
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
                || (document_url.scheme().eq_ignore_ascii_case("https")
                    && next_url.scheme().eq_ignore_ascii_case("http"))
                || !policy.allows_style(&document_url, &next_url)
            {
                return Ok(None);
            }
            request_referrer = normalize_referrer(Some(current_url.as_str()), &next_url)?;
            current_url = next_url;
            redirects += 1;
        };
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
            self.network.store_cookie(&cookie_url, &cookie);
        }
        Ok(Some(body))
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
            let sources = parts.map(str::to_ascii_lowercase).collect::<Vec<_>>();
            match name.to_ascii_lowercase().as_str() {
                "style-src" => policy.style_sources = Some(sources),
                "default-src" => policy.default_sources = Some(sources),
                _ => {}
            }
        }
    }
    policy
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

fn cacheable_response(headers: &HeaderMap) -> bool {
    if headers
        .get(reqwest::header::CACHE_CONTROL)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| {
            value.split(',').any(|directive| {
                matches!(
                    directive
                        .split_once('=')
                        .map_or(directive, |(name, _)| name)
                        .trim()
                        .to_ascii_lowercase()
                        .as_str(),
                    "no-store" | "no-cache" | "max-age=0"
                )
            })
        })
    {
        return false;
    }
    if headers
        .get(reqwest::header::PRAGMA)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| {
            value
                .split(',')
                .any(|directive| directive.trim().eq_ignore_ascii_case("no-cache"))
        })
    {
        return false;
    }
    !headers
        .get(reqwest::header::VARY)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| {
            value
                .split(',')
                .any(|field| field.trim() == "*" || field.trim().eq_ignore_ascii_case("cookie"))
        })
}

impl NativeNetworkState {
    fn cookie_header(&mut self, url: &Url) -> Option<String> {
        let host = url.host_str()?.to_ascii_lowercase();
        let request_path = if url.path().is_empty() {
            "/"
        } else {
            url.path()
        };
        let secure_request = url.scheme().eq_ignore_ascii_case("https");
        let now = Instant::now();
        self.cookies
            .retain(|cookie| cookie.expires_at.is_none_or(|expires_at| expires_at > now));

        let mut matching = self
            .cookies
            .iter()
            .filter(|cookie| {
                domain_matches(cookie, &host)
                    && path_matches(request_path, &cookie.path)
                    && (!cookie.secure || secure_request)
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

    fn store_cookie(&mut self, url: &Url, line: &str) {
        if line.len() > MAX_NATIVE_COOKIE_BYTES {
            return;
        }
        let Some(host) = url.host_str().map(str::to_ascii_lowercase) else {
            return;
        };
        let (pair, attributes) = line.split_once(';').unwrap_or((line, ""));
        let Some((name, value)) = pair.trim().split_once('=') else {
            return;
        };
        let name = name.trim();
        let value = value.trim();
        if name.is_empty()
            || !valid_cookie_text(name, true)
            || !valid_cookie_text(value, false)
            || name.len().saturating_add(value.len()) > MAX_NATIVE_COOKIE_BYTES
        {
            return;
        }

        let mut domain = host.clone();
        let mut host_only = true;
        let mut path = default_cookie_path(url);
        let mut secure = false;
        let mut max_age = None;
        for attribute in attributes.split(';').map(str::trim) {
            if attribute.eq_ignore_ascii_case("secure") {
                secure = true;
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
                        return;
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
                _ => {}
            }
        }
        if secure && !url.scheme().eq_ignore_ascii_case("https") {
            return;
        }

        let same_cookie = |cookie: &NativeCookie| {
            cookie.name == name && cookie.domain == domain && cookie.path == path
        };
        if max_age.is_some_and(|age| age <= 0) {
            self.cookies.retain(|cookie| !same_cookie(cookie));
            return;
        }
        let expires_at = max_age.map(|age| {
            let seconds = u64::try_from(age)
                .unwrap_or_default()
                .min(60 * 60 * 24 * 365 * 10);
            Instant::now() + Duration::from_secs(seconds)
        });
        self.cookies.retain(|cookie| !same_cookie(cookie));
        if self.cookies.len() >= MAX_NATIVE_COOKIES {
            self.cookies.remove(0);
        }
        self.cookies.push(NativeCookie {
            name: name.to_owned(),
            value: value.to_owned(),
            domain,
            path,
            host_only,
            secure,
            expires_at,
        });
    }

    fn store_cache(&mut self, key: String, resource: NativeResource) {
        if !self.cache.contains_key(&key) && self.cache.len() >= MAX_NATIVE_CACHE_ENTRIES {
            if let Some(oldest) = self.cache.keys().next().cloned() {
                self.cache.remove(&oldest);
            }
        }
        self.cache.insert(key, resource);
    }

    fn store_document_policy(&mut self, key: String, policy: NativeCspPolicy) {
        if !self.document_policies.contains_key(&key)
            && self.document_policies.len() >= MAX_NATIVE_CACHE_ENTRIES
        {
            if let Some(oldest) = self.document_policies.keys().next().cloned() {
                self.document_policies.remove(&oldest);
            }
        }
        self.document_policies.insert(key, policy);
    }
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
    use super::{
        MAX_NATIVE_CACHE_ENTRIES, NativeNetworkState, NativeResource, cacheable_response,
        decode_html_body, referrer_for_navigation,
    };
    use reqwest::header::{CACHE_CONTROL, HeaderMap, HeaderValue, PRAGMA, VARY};
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
        assert_eq!(
            state.cookie_header(&Url::parse("http://example.test/account/home").unwrap()),
            Some("session=alpha".into())
        );
        assert_eq!(
            state.cookie_header(&Url::parse("http://example.test/public").unwrap()),
            None
        );
        assert_eq!(
            state.cookie_header(&Url::parse("https://example.test/account/home").unwrap()),
            Some("session=alpha; secure=secret".into())
        );

        state.store_cookie(&page, "session=gone; Path=/account; Max-Age=0");
        assert_eq!(
            state.cookie_header(&Url::parse("http://example.test/account/home").unwrap()),
            None
        );

        for index in 0..(MAX_NATIVE_CACHE_ENTRIES + 1) {
            state.store_cache(
                format!("http://example.test/{index}"),
                NativeResource {
                    url: format!("http://example.test/{index}"),
                    origin: super::NativeOrigin::from_url(
                        &Url::parse("http://example.test/").unwrap(),
                    )
                    .unwrap(),
                    body: index.to_string(),
                },
            );
        }
        assert_eq!(state.cache.len(), MAX_NATIVE_CACHE_ENTRIES);
    }

    #[test]
    fn cache_policy_rejects_private_or_stale_variants() {
        let mut headers = HeaderMap::new();
        assert!(cacheable_response(&headers));
        headers.insert(
            CACHE_CONTROL,
            HeaderValue::from_static("public, max-age=60"),
        );
        assert!(cacheable_response(&headers));
        headers.insert(CACHE_CONTROL, HeaderValue::from_static("no-store"));
        assert!(!cacheable_response(&headers));
        headers.remove(CACHE_CONTROL);
        headers.insert(PRAGMA, HeaderValue::from_static("no-cache"));
        assert!(!cacheable_response(&headers));
        headers.remove(PRAGMA);
        headers.insert(VARY, HeaderValue::from_static("Accept-Encoding, Cookie"));
        assert!(!cacheable_response(&headers));
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
}
