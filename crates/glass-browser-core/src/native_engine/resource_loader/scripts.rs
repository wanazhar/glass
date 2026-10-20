use super::*;

impl NativeResourceLoader {
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

    pub(super) fn load_local_file_script_internal(
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
        request: NativeScriptLoadRequest<'_>,
    ) -> Result<Option<NativeScriptResource>, NativeEngineError> {
        self.load_script_like_async(request).await
    }

    pub(crate) async fn load_module_dependency_with_referrer_async(
        &mut self,
        request: NativeScriptLoadRequest<'_>,
    ) -> Result<Option<NativeScriptResource>, NativeEngineError> {
        self.load_script_like_async_with_referrer_source(request)
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
        self.load_script_like_async(NativeScriptLoadRequest {
            document_url,
            referrer_url: None,
            href,
            max_source_bytes,
            subresource_kind: NativeSubresourceKind::Worker,
            parser_inserted: true,
            nonce: None,
            integrity: None,
            crossorigin: None,
            object_url: None,
            module_type: None,
            worker_credentials_mode: None,
            referrer_policy,
        })
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
        self.load_script_like_async_with_referrer_source(NativeScriptLoadRequest {
            document_url,
            referrer_url: Some(referrer_url),
            href,
            max_source_bytes,
            subresource_kind: NativeSubresourceKind::Worker,
            parser_inserted: true,
            nonce: None,
            integrity: None,
            crossorigin: None,
            object_url: None,
            module_type: None,
            worker_credentials_mode: None,
            referrer_policy,
        })
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
        self.load_script_like_async(NativeScriptLoadRequest {
            document_url,
            referrer_url: None,
            href,
            max_source_bytes,
            subresource_kind: NativeSubresourceKind::Worker,
            parser_inserted: true,
            nonce: None,
            integrity: None,
            crossorigin: Some(crossorigin),
            object_url: None,
            module_type: Some(NativeModuleResourceType::JavaScript),
            worker_credentials_mode: Some(credentials_mode),
            referrer_policy,
        })
        .await
    }

    pub(crate) async fn load_worker_script_dependency_async_with_referrer_source(
        &mut self,
        request: NativeScriptLoadRequest<'_>,
    ) -> Result<Option<NativeScriptResource>, NativeEngineError> {
        let NativeScriptLoadRequest {
            document_url,
            referrer_url,
            href,
            max_source_bytes,
            module_type,
            worker_credentials_mode: credentials_mode,
            referrer_policy,
            ..
        } = request;
        let referrer_url = referrer_url.unwrap_or(document_url);
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
        self.load_script_like_async_with_referrer_source(NativeScriptLoadRequest {
            document_url,
            referrer_url: Some(referrer_url),
            href,
            max_source_bytes,
            subresource_kind: NativeSubresourceKind::Script,
            parser_inserted: true,
            nonce: None,
            integrity: None,
            crossorigin,
            object_url: None,
            module_type,
            worker_credentials_mode: credentials_mode,
            referrer_policy,
        })
        .await
    }

    pub(super) async fn load_script_like_async(
        &mut self,
        request: NativeScriptLoadRequest<'_>,
    ) -> Result<Option<NativeScriptResource>, NativeEngineError> {
        self.load_script_like_async_with_referrer_source(NativeScriptLoadRequest {
            referrer_url: None,
            ..request
        })
        .await
    }

    pub(super) async fn load_script_like_async_with_referrer_source(
        &mut self,
        request: NativeScriptLoadRequest<'_>,
    ) -> Result<Option<NativeScriptResource>, NativeEngineError> {
        let NativeScriptLoadRequest {
            document_url,
            referrer_url: referrer_source_url,
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
        } = request;
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
        // A worker's dependency URLs are relative to the worker script, while
        // `document_url` remains the owning document used for policy checks.
        // Keep those roles separate for rooted file workers: resolving against
        // the page URL would look in the wrong directory, and routing through
        // the network path would reject the local script as unavailable.
        if let Some(referrer_source_url) = referrer_source_url
            && document_url.scheme().eq_ignore_ascii_case("file")
            && let Ok(referrer_url) = Url::parse(without_fragment(referrer_source_url))
            && referrer_url.scheme().eq_ignore_ascii_case("file")
        {
            let target_url = Url::parse(href)
                .or_else(|_| referrer_url.join(href))
                .map_err(|_| NativeEngineError::UnsupportedUrl {
                    reason:
                        "worker script dependency URL could not be resolved against its referrer"
                            .into(),
                })?;
            if target_url.scheme().eq_ignore_ascii_case("file") {
                return self.load_local_file_script_internal(
                    document_url.as_str(),
                    target_url.as_str(),
                    max_source_bytes,
                    integrity,
                    Some((parser_inserted, nonce)),
                );
            }
        }
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
}
