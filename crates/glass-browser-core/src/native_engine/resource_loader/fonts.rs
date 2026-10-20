use super::*;

impl NativeResourceLoader {
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

    pub(super) async fn load_network_font_response_async(
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
            ))?;
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
}
