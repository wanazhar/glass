use super::*;

impl NativeResourceLoader {
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
            )?;
            if let Some(body) = current_body.clone() {
                request = match body {
                    NativeRequestBody::Text(body) => request.body(body),
                    NativeRequestBody::Bytes(body) | NativeRequestBody::StreamedBytes(body) => {
                        request.body(body)
                    }
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
}
