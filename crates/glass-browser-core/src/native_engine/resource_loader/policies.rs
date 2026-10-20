use super::*;

impl NativeResourceLoader {
    pub(super) fn csp_violation_record(
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

    pub(super) fn queue_csp_violation(
        &mut self,
        context: NativeCspViolationContext<'_>,
        blocked_uri: impl Into<String>,
        effective_directive: &str,
        original_policy: &str,
        sample: impl Into<String>,
    ) {
        if self.csp_violations.len() >= MAX_NATIVE_CSP_VIOLATIONS {
            return;
        }
        let violation = Self::csp_violation_record(
            context.document_url,
            blocked_uri,
            effective_directive,
            original_policy,
            sample,
        );
        schedule_native_csp_report_deliveries(csp_report_deliveries_for_declaration(
            &context.policy.reporting_endpoints,
            context.declaration,
            context.document_url,
            &violation,
        ));
        self.csp_violations.push(violation);
    }

    pub(super) fn record_report_only_url_violations(
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

    pub(super) fn record_report_only_url_violations_for_redirect(
        &mut self,
        policy: &NativeCspPolicy,
        kind: NativeSubresourceKind,
        document_url: &Url,
        resource_url: &Url,
        parser_inserted: bool,
        nonce: Option<&str>,
    ) {
        self.record_report_only_url_violations_with_metadata_and_path(NativeCspUrlCheck {
            policy,
            kind,
            document_url,
            resource_url,
            parser_inserted,
            nonce,
            ignore_path: true,
        });
    }

    pub(super) fn record_report_only_url_violations_with_metadata(
        &mut self,
        policy: &NativeCspPolicy,
        kind: NativeSubresourceKind,
        document_url: &Url,
        resource_url: &Url,
        parser_inserted: bool,
        nonce: Option<&str>,
    ) {
        self.record_report_only_url_violations_with_metadata_and_path(NativeCspUrlCheck {
            policy,
            kind,
            document_url,
            resource_url,
            parser_inserted,
            nonce,
            ignore_path: false,
        });
    }

    pub(super) fn record_report_only_url_violations_with_metadata_and_path(
        &mut self,
        check: NativeCspUrlCheck<'_>,
    ) {
        let blocked_uri = without_fragment(check.resource_url.as_str()).to_owned();
        let violations = if check.ignore_path {
            check.policy.report_only_url_violations_for_redirect(
                check.kind,
                check.document_url,
                check.resource_url,
                check.parser_inserted,
                check.nonce,
            )
        } else {
            check.policy.report_only_url_violations(
                check.kind,
                check.document_url,
                check.resource_url,
                check.parser_inserted,
                check.nonce,
            )
        };
        for (declaration, directive) in violations {
            self.queue_csp_violation(
                NativeCspViolationContext {
                    policy: check.policy,
                    declaration,
                    document_url: check.document_url,
                },
                blocked_uri.clone(),
                directive,
                &declaration.original_policy,
                "",
            );
        }
    }

    pub(super) fn record_report_only_navigation_violations(
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
                NativeCspViolationContext {
                    policy,
                    declaration,
                    document_url,
                },
                blocked_uri.clone(),
                directive,
                &declaration.original_policy,
                "",
            );
        }
    }

    pub(super) fn record_report_only_inline_violations(
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
                NativeCspViolationContext {
                    policy,
                    declaration,
                    document_url,
                },
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

    pub(super) fn allows_inline_style_element_with_reporting(
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

    pub(super) fn allows_inline_style_attribute_with_reporting(
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

    pub(super) fn document_policy(
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

    pub(super) fn csp_document_owner(
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

    pub fn websocket_target(
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

    pub fn apply_websocket_response_cookies(
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
    pub async fn open_event_source_async(
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
            )?;
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
            // Process each network response before following a redirect or
            // returning an EventSource error. This keeps redirect cookies
            // visible to the next parent-owned hop and processes final
            // response cookies independently of CORS exposure.
            if with_credentials || current_url.origin() == document_url.origin() {
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
            }
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

    pub(super) fn navigation_policy(
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
}
