use super::*;

impl NativeResourceLoader {
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
            )?;
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
        self.load_image_with_request_policy(
            document_url,
            src,
            object_url,
            referrer_policy,
            NativeImageRequestPolicy::HTML_IMAGE,
        )
        .await
    }

    pub(crate) async fn load_video_poster_async_with_referrer_policy(
        &mut self,
        document_url: &str,
        src: &str,
        referrer_policy: NativeFetchReferrerPolicy,
    ) -> Result<Option<NativeImage>, NativeEngineError> {
        self.load_image_with_request_policy(
            document_url,
            src,
            None,
            referrer_policy,
            NativeImageRequestPolicy::VIDEO_POSTER,
        )
        .await
    }

    pub(super) async fn load_image_with_request_policy(
        &mut self,
        document_url: &str,
        src: &str,
        object_url: Option<&NativeObjectUrlResource>,
        referrer_policy: NativeFetchReferrerPolicy,
        request_policy: NativeImageRequestPolicy,
    ) -> Result<Option<NativeImage>, NativeEngineError> {
        tracing::trace!(
            destination = request_policy.destination,
            initiator_type = request_policy.initiator_type,
            credentials_mode = request_policy.credentials_mode.as_str(),
            use_url_credentials = request_policy.use_url_credentials,
            "loading native image subresource"
        );
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
        let Some(mut target_url) = (if request_policy.use_url_credentials {
            resolve_video_poster_url(&document_url, src)?
        } else {
            resolve_subresource_url_with_blob(&document_url, src)?
        }) else {
            return Ok(None);
        };
        let authorization = if request_policy.use_url_credentials {
            basic_authorization_from_url(&target_url)?
        } else {
            None
        };
        let authorization_origin = authorization.as_ref().map(|_| target_url.origin());
        let mut authorization_removed_after_cross_origin_redirect = false;
        if authorization.is_some() {
            clear_url_credentials(&mut target_url)?;
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
        let cache_enabled = authorization.is_none();
        let stale_cached_image = cache_enabled
            .then(|| {
                self.network
                    .image_cache
                    .get(&requested_cache_key)
                    .cloned()
                    .filter(|cached| !cached.is_fresh(Instant::now()))
            })
            .flatten();
        if cache_enabled
            && let Some(cached) = self.network.image_cache.get(&requested_cache_key)
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
        let mut has_set_cookie = false;
        let response = loop {
            let mut request_url = current_url.clone();
            request_url.set_fragment(None);
            let mut request = self.apply_environment_headers(client.get(request_url).header(
                reqwest::header::ACCEPT,
                "image/avif,image/webp,image/apng,image/svg+xml,image/jpeg,image/png,image/*;q=0.8, */*;q=0.5",
            ))?;
            if let Some(referrer) = request_referrer.as_deref() {
                request = request.header(reqwest::header::REFERER, referrer);
            }
            if !authorization_removed_after_cross_origin_redirect
                && authorization_origin.as_ref() == Some(&current_url.origin())
                && let Some(authorization) = authorization.as_deref()
            {
                request = request.header(reqwest::header::AUTHORIZATION, authorization);
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
            if fetch_credentials_for_url(
                Some(request_policy.credentials_mode),
                false,
                &document_url,
                &current_url,
            ) && let Some(cookie) = self.network.cookie_header_for_request(
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
                    has_set_cookie = true;
                    self.cookie_changes
                        .extend(self.network.store_cookie(&current_url, cookie));
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
            if current_url.origin() != next_url.origin() {
                authorization_removed_after_cross_origin_redirect = true;
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
        if response.status() == reqwest::StatusCode::NOT_MODIFIED {
            let Some(cached) = stale_cached_image else {
                return Ok(None);
            };
            let cached_image = cached.image.clone();
            if redirects != 0 {
                return Ok(None);
            }
            if cache_enabled {
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
        if cache_enabled {
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
        }
        Ok(Some(image))
    }
}
