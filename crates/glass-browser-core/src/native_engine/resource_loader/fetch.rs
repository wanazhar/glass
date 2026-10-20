use super::*;

impl NativeResourceLoader {
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
            fetch_response_cache_key(NativeFetchResponseCacheKeyInput {
                document_url: &document_url,
                target_url: &target_url,
                method: &method,
                request_headers: &current_headers,
                content_type: content_type.as_deref(),
                credentials: initial_credentials,
                cors_mode,
                request_cookie: request_cookie.as_deref(),
                request_referrer: initial_referrer.as_deref(),
                referrer_policy: effective_referrer_policy.as_str(),
            })
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
        let mut streaming_request_body_sent = request_body.is_some()
            || body
                .as_ref()
                .is_some_and(|body| matches!(body, NativeRequestBody::StreamedBytes(_)));
        let mut current_body = body;
        let mut current_request_body = request_body;
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
                self.verify_cors_preflight(NativeCorsPreflightRequest {
                    client: &client,
                    document_url: &document_url,
                    target_url: &request_url,
                    method: &current_method,
                    requested_headers: &requested_headers,
                    credentials: current_credentials,
                    referrer: request_referrer.as_deref(),
                })
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
            )?;
            if let Some(body) = current_request_body.take() {
                request = request.body(body);
            } else if let Some(body) = current_body.as_ref() {
                request = match body {
                    NativeRequestBody::Text(body) => request.body(body.clone()),
                    NativeRequestBody::Bytes(body) | NativeRequestBody::StreamedBytes(body) => {
                        request.body(body.clone())
                    }
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
            for (cookie_url, cookie) in pending_cookies {
                self.cookie_changes
                    .extend(self.network.store_cookie(&cookie_url, &cookie));
            }
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
        preflight: NativeCorsPreflightRequest<'_>,
    ) -> Result<(), NativeEngineError> {
        let NativeCorsPreflightRequest {
            client,
            document_url,
            target_url,
            method,
            requested_headers,
            credentials,
            referrer,
        } = preflight;
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
        )?;
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
}
