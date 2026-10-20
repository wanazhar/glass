//! Dynamic document-script and module-fetch orchestration helpers.

use super::*;

pub(super) fn register_page_import_map_source(
    map_source: NativePageImportMapSource,
    import_map: &mut NativeModuleImportMap,
    loader: &mut NativeResourceLoader,
    document_url: &str,
    sources: &mut Vec<(NativePageScriptTiming, NativePageScript)>,
    resource_events: &mut Vec<(u32, NativeEventKind)>,
) -> Result<(), NativeEngineError> {
    let node_index = map_source.node_index;
    let Some(source) = map_source.source else {
        resource_events.push((node_index, NativeEventKind::Error));
        return Ok(());
    };
    if !loader.allows_inline_script(document_url, &source, map_source.nonce.as_deref())? {
        resource_events.push((node_index, NativeEventKind::Error));
        return Ok(());
    }
    let parsed = match NativeModuleImportMap::parse(&source, document_url) {
        Ok(parsed) => parsed,
        Err(_) => {
            resource_events.push((node_index, NativeEventKind::Error));
            return Ok(());
        }
    };
    let mut merged = import_map.clone();
    if merged.merge(parsed).is_err() {
        resource_events.push((node_index, NativeEventKind::Error));
        return Ok(());
    }
    *import_map = merged;
    sources.push((
        NativePageScriptTiming::ParserBlocking,
        NativePageScript::ImportMap(import_map.clone()),
    ));
    Ok(())
}

pub(super) fn refresh_parent_broker_meta_csp(
    loader: &NativeResourceLoader,
    broker: Option<&mut NativeContentFetchBroker<'_>>,
) -> Result<(), NativeEngineError> {
    let Some(broker) = broker else {
        return Ok(());
    };
    let owner_document_url = broker.owner.document_url.clone();
    broker.page_meta_content_security_policies =
        loader.document_meta_content_security_policies(&owner_document_url)?;
    Ok(())
}

pub(crate) fn missing_parent_network_authority(operation: &'static str) -> NativeEngineError {
    NativeEngineError::Network {
        operation: operation.into(),
        reason: "parent cookie and network authority is unavailable".into(),
    }
}

/// Load and execute dynamic external/module sources discovered by a dynamic
/// script. Inline descendants are handled synchronously by the shared
/// scheduler; only sources that require the content-process loader are
/// returned by each scheduler turn.
pub(super) async fn execute_dynamic_page_scripts_with_loader(
    document: &mut NativeDocument,
    runtime: &NativeJavaScriptRuntime,
    mut sources: Vec<NativePageScriptSource>,
    loader: &mut NativeResourceLoader,
    document_url: &str,
    document_origin: &NativeOrigin,
    viewport: Viewport,
    mut parent_fetch_broker: Option<&mut NativeContentFetchBroker<'_>>,
) -> Result<NativePageScriptResult, NativeEngineError> {
    let mut aggregate = NativePageScriptResult {
        pending_fetches: Vec::new(),
        service_worker_commands: Vec::new(),
        websocket_commands: Vec::new(),
        event_source_commands: Vec::new(),
        pending_script_sources: Vec::new(),
        scroll_commands: Vec::new(),
        navigation: None,
        dialogs: Vec::new(),
        events: Vec::new(),
    };
    let mut batches = 0usize;
    loop {
        batches = batches.saturating_add(1);
        if batches > super::super::javascript::MAX_NATIVE_INLINE_SCRIPTS {
            return Err(NativeEngineError::limit(
                "native dynamic script loader turns",
                super::super::javascript::MAX_NATIVE_INLINE_SCRIPTS,
                batches,
            ));
        }
        let (scripts, resource_events) = load_dynamic_page_script_sources(
            sources,
            loader,
            document_url,
            runtime,
            parent_fetch_broker.as_deref_mut(),
        )
        .await?;
        let csp_violations = loader.take_csp_violations();
        let mut result = execute_dynamic_page_scripts(NativeDynamicPageScriptRequest {
            document,
            runtime,
            sources: scripts,
            document_url,
            document_origin,
            viewport,
            resource_events: &resource_events,
            csp_violations: &csp_violations,
        })?;
        apply_pending_meta_content_security_policies(document, loader, document_url)?;
        refresh_parent_broker_meta_csp(loader, parent_fetch_broker.as_deref_mut())?;
        let next_sources = std::mem::take(&mut result.pending_script_sources);
        merge_dynamic_page_script_result(&mut aggregate, result)?;
        if next_sources.is_empty() {
            return Ok(aggregate);
        }
        sources = next_sources;
    }
}

pub(super) fn execute_dynamic_page_scripts_without_loader(
    document: &mut NativeDocument,
    runtime: &NativeJavaScriptRuntime,
    mut sources: Vec<NativePageScriptSource>,
    document_url: &str,
    document_origin: &NativeOrigin,
    viewport: Viewport,
) -> Result<NativePageScriptResult, NativeEngineError> {
    let mut aggregate = NativePageScriptResult::default();
    let mut batches = 0usize;
    loop {
        batches = batches.saturating_add(1);
        if batches > super::super::javascript::MAX_NATIVE_INLINE_SCRIPTS {
            return Err(NativeEngineError::limit(
                "native dynamic script turns",
                super::super::javascript::MAX_NATIVE_INLINE_SCRIPTS,
                batches,
            ));
        }

        let mut import_map = runtime.module_import_map()?;
        let mut scripts = vec![NativePageScript::ImportMap(import_map.clone())];
        let mut resource_events = Vec::new();
        for source in sources {
            match source {
                NativePageScriptSource::ImportMap {
                    source,
                    node_index,
                    nonce: _,
                } => {
                    let Some(source) = source else {
                        resource_events.push((node_index, NativeEventKind::Error));
                        continue;
                    };
                    let Ok(parsed) = NativeModuleImportMap::parse(&source, document_url) else {
                        resource_events.push((node_index, NativeEventKind::Error));
                        continue;
                    };
                    let mut merged = import_map.clone();
                    if merged.merge(parsed).is_err() {
                        resource_events.push((node_index, NativeEventKind::Error));
                        continue;
                    }
                    import_map = merged;
                    scripts.push(NativePageScript::ImportMap(import_map.clone()));
                }
                NativePageScriptSource::External { .. }
                | NativePageScriptSource::ModuleExternal { .. } => {
                    return Err(NativeEngineError::UnsupportedUrl {
                        reason:
                            "dynamic external/module scripts require a process-backed HTTP(S) document"
                                .into(),
                    });
                }
                source => {
                    scripts.extend(page_script_sources_to_scripts(
                        vec![source],
                        document_url,
                        "glass-dynamic-module",
                    ));
                }
            }
        }

        let mut result = execute_dynamic_page_scripts(NativeDynamicPageScriptRequest {
            document,
            runtime,
            sources: scripts,
            document_url,
            document_origin,
            viewport,
            resource_events: &resource_events,
            csp_violations: &[],
        })?;
        let next_sources = std::mem::take(&mut result.pending_script_sources);
        merge_dynamic_page_script_result(&mut aggregate, result)?;
        if next_sources.is_empty() {
            return Ok(aggregate);
        }
        sources = next_sources;
    }
}

pub(crate) fn merge_dynamic_page_script_result(
    aggregate: &mut NativePageScriptResult,
    result: NativePageScriptResult,
) -> Result<(), NativeEngineError> {
    aggregate.pending_fetches.extend(result.pending_fetches);
    aggregate
        .service_worker_commands
        .extend(result.service_worker_commands);
    aggregate
        .websocket_commands
        .extend(result.websocket_commands);
    aggregate
        .event_source_commands
        .extend(result.event_source_commands);
    aggregate.scroll_commands.extend(result.scroll_commands);
    aggregate.dialogs.extend(result.dialogs);
    aggregate.events.extend(result.events);
    if let Some(navigation) = result.navigation {
        if aggregate.navigation.is_some() {
            return Err(NativeEngineError::TargetNotActionable {
                reason: "one dynamic page-script batch cannot activate multiple navigations".into(),
            });
        }
        aggregate.navigation = Some(navigation);
    }
    Ok(())
}

pub(super) async fn load_module_dependencies(
    owner_url: &str,
    module_identity: &str,
    module_base_url: &str,
    source: &str,
    referrer_policy: Option<NativeFetchReferrerPolicy>,
    timing: NativePageScriptTiming,
    loader: &mut NativeResourceLoader,
    runtime: Option<&NativeJavaScriptRuntime>,
    import_map: &mut NativeModuleImportMap,
    scripts: &mut Vec<(NativePageScriptTiming, NativePageScript)>,
    seen: &mut BTreeSet<String>,
    total_bytes: &mut usize,
    mut parent_fetch_broker: Option<&mut NativeContentFetchBroker<'_>>,
) -> Result<(), NativeEngineError> {
    let mut pending = vec![(
        module_identity.to_owned(),
        module_base_url.to_owned(),
        source.to_owned(),
        referrer_policy,
    )];
    let mut requested_urls = seen.clone();
    while let Some((module_identity, current_base_url, current_source, current_policy)) =
        pending.pop()
    {
        if module_identity.starts_with(NATIVE_JSON_MODULE_NAME_PREFIX) {
            continue;
        }
        let requests = static_module_requests(&current_source)?;
        for request in requests {
            if request.module_type == NativeModuleResourceType::Unsupported {
                return Err(NativeEngineError::UnsupportedUrl {
                    reason: "module import attributes request an unsupported module type".into(),
                });
            }
            let Some(target) =
                resolve_module_specifier(&current_base_url, &request.specifier, import_map)?
            else {
                continue;
            };
            let name = native_module_loader_name(&target, request.module_type);
            if seen.contains(&name) || !requested_urls.insert(name.clone()) {
                continue;
            }
            if requested_urls.len() > MAX_NATIVE_MODULE_IMPORTS {
                return Err(NativeEngineError::limit(
                    "module graph entries",
                    MAX_NATIVE_MODULE_IMPORTS,
                    requested_urls.len(),
                ));
            }
            let object_url = runtime
                .map(|runtime| runtime.object_url_resource(&target))
                .transpose()?
                .flatten();
            let integrity = import_map.integrity_for_url(&target);
            let resource =
                if object_url.is_none() && is_parent_owned_network_target(owner_url, &target) {
                    if let Some(parent_fetch_broker) = parent_fetch_broker.as_deref_mut() {
                        parent_fetch_broker
                            .load_page_script(
                                owner_url,
                                &target,
                                Some(&current_base_url),
                                MAX_NATIVE_SCRIPT_BYTES,
                                true,
                                None,
                                integrity,
                                Some("anonymous"),
                                Some(request.module_type),
                                current_policy,
                            )
                            .await?
                    } else {
                        return Err(missing_parent_network_authority("module dependency"));
                    }
                } else {
                    loader
                        .load_module_dependency_with_referrer_async(NativeScriptLoadRequest {
                            document_url: owner_url,
                            referrer_url: Some(&current_base_url),
                            href: &target,
                            max_source_bytes: MAX_NATIVE_SCRIPT_BYTES,
                            subresource_kind: NativeSubresourceKind::Script,
                            parser_inserted: true,
                            nonce: None,
                            integrity,
                            crossorigin: Some("anonymous"),
                            object_url: object_url.as_ref(),
                            module_type: Some(request.module_type),
                            worker_credentials_mode: None,
                            referrer_policy: current_policy,
                        })
                        .await?
                };
            let Some(resource) = resource else {
                return Err(NativeEngineError::Network {
                    operation: "module dependency".into(),
                    reason: "module dependency could not be loaded".into(),
                });
            };
            if !seen.insert(name.clone()) {
                continue;
            }
            if seen.len() > MAX_NATIVE_MODULE_IMPORTS {
                return Err(NativeEngineError::limit(
                    "module graph entries",
                    MAX_NATIVE_MODULE_IMPORTS,
                    seen.len(),
                ));
            }
            let base_url = resource.url;
            let source = resource.body;
            let dependency_referrer_policy = resource.response_referrer_policy.or(current_policy);
            *total_bytes = total_bytes.saturating_add(source.len());
            if *total_bytes > MAX_NATIVE_SCRIPT_BYTES.saturating_mul(MAX_NATIVE_MODULE_IMPORTS) {
                return Err(NativeEngineError::limit(
                    "module graph bytes",
                    MAX_NATIVE_SCRIPT_BYTES.saturating_mul(MAX_NATIVE_MODULE_IMPORTS),
                    *total_bytes,
                ));
            }
            scripts.push((
                timing,
                NativePageScript::ModuleDependency {
                    name: name.clone(),
                    source: source.clone(),
                    base_url: base_url.clone(),
                    referrer_policy: dependency_referrer_policy,
                },
            ));
            pending.push((name, base_url, source, dependency_referrer_policy));
        }
    }
    Ok(())
}

pub(super) fn form_action_allows(
    loader: Option<&mut NativeResourceLoader>,
    document_url: &str,
    target_url: &str,
) -> Result<bool, NativeEngineError> {
    match loader {
        Some(loader) => loader.allows_navigation(
            document_url,
            target_url,
            NativeNavigationPolicyKind::FormAction,
        ),
        None => Ok(true),
    }
}

fn resolve_module_specifier(
    module_url: &str,
    specifier: &str,
    import_map: &mut NativeModuleImportMap,
) -> Result<Option<String>, NativeEngineError> {
    let target = import_map
        .resolve_and_record(module_url, specifier)
        .map_err(|reason| NativeEngineError::UnsupportedUrl {
            reason: reason.into(),
        })?;
    let target = Url::parse(&target).map_err(|_| NativeEngineError::UnsupportedUrl {
        reason: "module URL is invalid".into(),
    })?;
    if !is_network_url(target.as_str()) && !target.scheme().eq_ignore_ascii_case("blob") {
        return Ok(None);
    }
    Ok(Some(target.to_string()))
}

pub(super) fn retain_font_face_network_commands(
    dynamic_result: &mut NativePageScriptResult,
    commands: Vec<NativeScriptCommand>,
) {
    for command in commands {
        match &command {
            NativeScriptCommand::Fetch { .. } => {
                dynamic_result.pending_fetches.push(command.clone())
            }
            NativeScriptCommand::WebSocketOpen { .. }
            | NativeScriptCommand::WebSocketSend { .. }
            | NativeScriptCommand::WebSocketClose { .. } => {
                dynamic_result.websocket_commands.push(command.clone())
            }
            NativeScriptCommand::EventSourceOpen { .. }
            | NativeScriptCommand::EventSourceClose { .. } => {
                dynamic_result.event_source_commands.push(command.clone())
            }
            _ => {}
        }
    }
}

pub(super) fn retain_font_face_follow_up_commands(
    effective_commands: &mut Vec<NativeScriptCommand>,
    scroll_commands: &mut Vec<NativeScriptCommand>,
    dynamic_result: &mut NativePageScriptResult,
    commands: Vec<NativeScriptCommand>,
) {
    scroll_commands.extend(extract_scroll_commands(&commands));
    for command in &commands {
        effective_commands.push(command.clone());
    }
    retain_font_face_network_commands(dynamic_result, commands);
}

pub(super) fn fetch_commands(
    commands: &[NativeScriptCommand],
) -> Result<VecDeque<NativeScriptFetch>, NativeEngineError> {
    commands
        .iter()
        .filter_map(|command| match command {
            NativeScriptCommand::Fetch {
                request_id,
                worker_id: _,
                href,
                credentials,
                credentials_mode,
                referrer,
                referrer_url,
                referrer_policy,
                method,
                headers,
                body,
                body_base64,
                content_type,
                mode,
                redirect,
                cache,
                timeout_ms,
                upload_stream_id,
                destination,
                module_referrer,
                module_type,
            } => Some((
                *request_id,
                href.clone(),
                method,
                headers.clone(),
                body.clone(),
                body_base64.clone(),
                content_type.clone(),
                mode.clone(),
                redirect.clone(),
                cache.clone(),
                *timeout_ms,
                *upload_stream_id,
                *credentials,
                credentials_mode.clone(),
                referrer.clone(),
                referrer_url.clone(),
                referrer_policy.clone(),
                destination.clone(),
                module_referrer.clone(),
                *module_type,
            )),
            _ => None,
        })
        .map(
            |(
                request_id,
                href,
                method,
                headers,
                body,
                body_base64,
                content_type,
                mode,
                redirect,
                cache,
                timeout_ms,
                upload_stream_id,
                credentials,
                credentials_mode,
                referrer,
                referrer_url,
                referrer_policy,
                destination,
                module_referrer,
                module_type,
            )| {
                if timeout_ms.is_some_and(|value| value > MAX_NATIVE_XHR_TIMEOUT_MS) {
                    return Err(NativeEngineError::invalid(
                        "script fetch timeout",
                        "must not exceed the native XHR timeout limit",
                    ));
                }
                let timeout = timeout_ms.map(|value| Duration::from_millis(u64::from(value)));
                let cors_mode = match mode.as_deref().unwrap_or("cors") {
                    "cors" => NativeCorsMode::Cors,
                    "no-cors" => NativeCorsMode::NoCors,
                    "same-origin" => NativeCorsMode::SameOrigin,
                    _ => {
                        return Err(NativeEngineError::invalid(
                            "script fetch mode",
                            "must be cors, no-cors, or same-origin",
                        ));
                    }
                };
                let redirect_mode = match redirect.as_deref().unwrap_or("follow") {
                    "follow" => NativeFetchRedirectMode::Follow,
                    "error" => NativeFetchRedirectMode::Error,
                    "manual" => NativeFetchRedirectMode::Manual,
                    _ => {
                        return Err(NativeEngineError::invalid(
                            "script fetch redirect mode",
                            "must be follow, error, or manual",
                        ));
                    }
                };
                let cache_mode = NativeFetchCacheMode::from_option(cache.as_deref())?;
                let credentials_mode = credentials_mode
                    .as_deref()
                    .map(NativeFetchCredentialsMode::parse)
                    .transpose()?;
                if let Some(referrer) = referrer.as_deref()
                    && !referrer.is_empty()
                {
                    validate_url_text("script fetch referrer", referrer)?;
                }
                if let Some(referrer_url) = referrer_url.as_deref()
                    && !referrer_url.is_empty()
                {
                    validate_url_text("script fetch referrer URL", referrer_url)?;
                }
                if let Some(referrer_policy) = referrer_policy.as_deref() {
                    super::super::resource_loader::NativeFetchReferrerPolicy::parse(
                        referrer_policy,
                    )?;
                }
                let method =
                    NativeFetchMethod::from_fetch_method(method.as_str()).map_err(|_| {
                        NativeEngineError::invalid(
                            "script fetch method",
                            "must be a valid HTTP token other than CONNECT, TRACE, or TRACK",
                        )
                    })?;
                let body = match body_base64 {
                    Some(encoded) => {
                        let decoded = base64::engine::general_purpose::STANDARD
                            .decode(encoded)
                            .map_err(|_| {
                                NativeEngineError::invalid(
                                    "script fetch binary body",
                                    "must be valid base64",
                                )
                            })?;
                        if decoded.len() > MAX_NATIVE_FORM_BODY_BYTES {
                            return Err(NativeEngineError::limit(
                                "script fetch binary body",
                                MAX_NATIVE_FORM_BODY_BYTES,
                                decoded.len(),
                            ));
                        }
                        Some(NativeRequestBody::Bytes(decoded))
                    }
                    None => body.map(NativeRequestBody::Text),
                };
                let (font_destination, module_destination) = match destination.as_deref() {
                    None | Some("") => (false, false),
                    Some("font") => (true, false),
                    Some("module") => (false, true),
                    Some(_) => {
                        return Err(NativeEngineError::invalid(
                            "script fetch destination",
                            "must be empty, font, or module",
                        ));
                    }
                };
                if module_destination != module_referrer.is_some() {
                    return Err(NativeEngineError::invalid(
                        "module fetch referrer",
                        "must be present only for a module-destination request",
                    ));
                }
                if (!module_destination && module_type.is_some())
                    || module_type == Some(NativeModuleResourceType::Unsupported)
                {
                    return Err(NativeEngineError::invalid(
                        "dynamic module resource type",
                        "must be supported and present only for module-destination requests",
                    ));
                }
                Ok((
                    request_id,
                    href,
                    method,
                    headers,
                    body,
                    content_type,
                    cors_mode,
                    redirect_mode,
                    cache_mode,
                    timeout,
                    upload_stream_id,
                    credentials,
                    credentials_mode,
                    referrer,
                    referrer_url,
                    referrer_policy,
                    font_destination,
                    module_referrer,
                    module_destination,
                    module_type,
                ))
            },
        )
        .collect()
}

pub(super) fn fetch_response_payload(
    result: Result<NativeFetchResponse, NativeEngineError>,
) -> Value {
    match result {
        Ok(response) => json!({
            "url": response.url,
            "status": response.status,
            "statusText": response.status_text,
            "contentType": response.content_type,
            "headers": response.headers,
            "body": String::from_utf8_lossy(&response.body),
            "bodyBase64": base64::engine::general_purpose::STANDARD.encode(&response.body),
            "redirected": response.redirected,
            "opaque": response.opaque,
            "opaqueRedirect": response.opaque_redirect,
        }),
        Err(error) => json!({
            "error": error.to_string(),
            "timeout": matches!(
                error,
                NativeEngineError::Network { reason, .. } if reason == "request timed out"
            ),
        }),
    }
}

pub(super) fn fetch_response_payload_with_csp_violations(
    result: Result<NativeFetchResponse, NativeEngineError>,
    csp_violations: Vec<NativeCspViolation>,
) -> Value {
    let mut payload = fetch_response_payload(result);
    payload["cspViolations"] =
        serde_json::to_value(csp_violations).expect("CSP violation records are serializable");
    payload
}

pub(super) fn font_fetch_response_payload(
    result: Result<Option<NativeFetchResponse>, NativeEngineError>,
) -> Value {
    match result {
        Ok(Some(response)) => fetch_response_payload(Ok(response)),
        Ok(None) => fetch_response_payload(Err(NativeEngineError::Network {
            operation: "font fetch".into(),
            reason: "font resource was blocked or unavailable".into(),
        })),
        Err(error) => fetch_response_payload(Err(error)),
    }
}

fn fetch_stream_response_payload(response: NativeFetchResponse, stream_id: u32) -> Value {
    let mut payload = fetch_response_payload(Ok(response));
    if let Some(object) = payload.as_object_mut() {
        object.insert("bodyStreamId".into(), Value::from(stream_id));
    }
    payload
}

pub(super) async fn fetch_opened_response_payload(
    opened: Result<NativeFetchResponseStream, NativeEngineError>,
    request_id: u32,
    fetch_stream_connections: &mut BTreeMap<u32, NativeFetchStreamConnection>,
) -> Value {
    match opened {
        Ok(mut opened)
            if !opened.response.opaque
                && !opened.response.opaque_redirect
                && opened.body.is_some() =>
        {
            if let std::collections::btree_map::Entry::Vacant(entry) =
                fetch_stream_connections.entry(request_id)
            {
                let response = opened.response;
                let stream = if let Some(body) = opened.body.take() {
                    spawn_native_fetch_stream(body, opened.max_response_bytes)
                } else {
                    spawn_native_fetch_bytes_stream(
                        opened
                            .cached_body
                            .take()
                            .expect("fetch stream cached body is present"),
                        opened.max_response_bytes,
                    )
                };
                entry.insert(stream);
                fetch_stream_response_payload(response, request_id)
            } else {
                fetch_response_payload(Err(NativeEngineError::Network {
                    operation: "fetch response stream".into(),
                    reason: "fetch response stream identifier is already active".into(),
                }))
            }
        }
        Ok(mut opened) => {
            if let Some(body) = opened.cached_body.take() {
                opened.response.body = body;
            }
            fetch_response_payload(Ok(opened.response))
        }
        Err(error) => fetch_response_payload(Err(error)),
    }
}

/// Activate network commands emitted by a dynamically attached script. The
/// DOM/event effects are already committed by `mutate_script_document`; this
/// handoff only installs persistent transports and returns fetches to the
/// existing resolver queue.
pub(super) async fn activate_dynamic_page_script_network(
    result: NativePageScriptResult,
    websocket_connections: &mut BTreeMap<u32, NativeWebSocketConnection>,
    event_source_connections: &mut BTreeMap<u32, NativeEventSourceConnection>,
    loader: Option<&NativeResourceLoader>,
    document_url: &str,
    mut parent_fetch_broker: Option<&mut NativeContentFetchBroker<'_>>,
) -> Result<(Vec<NativeScriptCommand>, bool), NativeEngineError> {
    if !result.pending_script_sources.is_empty() {
        return Err(NativeEngineError::Worker {
            operation: "dynamic page script".into(),
            reason: "dynamic external/module script sources were not fully loaded".into(),
        });
    }
    let has_background_transport =
        !result.websocket_commands.is_empty() || !result.event_source_commands.is_empty();
    let retained_websocket = process_websocket_commands(
        result.websocket_commands,
        websocket_connections,
        document_url,
        parent_fetch_broker.as_deref_mut(),
    )
    .await?;
    if !retained_websocket.is_empty() {
        return Err(NativeEngineError::Worker {
            operation: "dynamic page script WebSocket commands".into(),
            reason: "dynamic WebSocket command handoff retained an unexpected command".into(),
        });
    }
    let retained_event_source = process_event_source_commands(
        result.event_source_commands,
        event_source_connections,
        loader,
        document_url,
        parent_fetch_broker,
    )
    .await?;
    if !retained_event_source.is_empty() {
        return Err(NativeEngineError::Worker {
            operation: "dynamic page script EventSource commands".into(),
            reason: "dynamic EventSource command handoff retained an unexpected command".into(),
        });
    }
    Ok((result.pending_fetches, has_background_transport))
}

pub(super) async fn process_page_fetch_resolution(
    request_id: u32,
    mut payload: Value,
    runtime: &NativeJavaScriptRuntime,
    service_workers: &mut NativeServiceWorkerRegistry,
    websocket_connections: &mut BTreeMap<u32, NativeWebSocketConnection>,
    fetch_stream_connections: &mut BTreeMap<u32, NativeFetchStreamConnection>,
    fetch_upload_connections: &mut BTreeMap<u32, NativeFetchUploadConnection>,
    event_source_connections: &mut BTreeMap<u32, NativeEventSourceConnection>,
    loader: &mut NativeResourceLoader,
    next: &mut NativeDocument,
    mutation: &mut NativeContentMutation,
    current_url: &mut String,
    document_origin: &NativeOrigin,
    viewport: Viewport,
    top_level_await_pending: bool,
    resolved_value: &mut Option<Value>,
    pump_background_events: &mut bool,
    pending: &mut VecDeque<NativeScriptFetch>,
    mut parent_fetch_broker: Option<&mut NativeContentFetchBroker<'_>>,
) -> Result<(), NativeEngineError> {
    let mut csp_violations = loader.take_csp_violations();
    if let Some(parent_violations) = payload
        .as_object_mut()
        .and_then(|payload| payload.remove("cspViolations"))
    {
        let parent_violations: Vec<NativeCspViolation> = serde_json::from_value(parent_violations)
            .map_err(|_| {
                NativeEngineError::worker_failure(
                    "decode parent Fetch CSP violations",
                    NativeWorkerFailureKind::Protocol,
                    "parent Fetch response contained invalid CSP violations",
                )
            })?;
        csp_violations.extend(parent_violations);
    }
    let page_events = NativePageEventBatch {
        service_worker_client_messages: service_workers.take_client_messages(),
        csp_violations,
        ..NativePageEventBatch::default()
    };
    let resolved = runtime.resolve_fetch(
        request_id,
        &payload,
        NativePageEventContext {
            document: next,
            document_url: current_url,
            origin: document_origin,
            viewport,
            csp_violations: &[],
        },
        &page_events,
    )?;
    if top_level_await_pending && let Some(value) = runtime.take_top_level_await_result()? {
        *resolved_value = Some(value);
    }
    let resolved_commands = process_websocket_commands(
        resolved.commands,
        websocket_connections,
        current_url,
        parent_fetch_broker.as_deref_mut(),
    )
    .await?;
    let resolved_commands = process_event_source_commands(
        resolved_commands,
        event_source_connections,
        Some(&*loader),
        current_url,
        parent_fetch_broker.as_deref_mut(),
    )
    .await?;
    let resolved_commands =
        process_fetch_stream_commands(resolved_commands, fetch_stream_connections)?;
    let resolved_commands =
        process_fetch_upload_commands(resolved_commands, fetch_upload_connections)?;
    let resolved_history = extract_history_commands(&resolved_commands);
    if !resolved_history.is_empty() {
        let mut url = Some(current_url.clone());
        apply_content_runtime_history(&resolved_history, &mut url, document_origin, runtime)?;
        *current_url = url.ok_or_else(|| NativeEngineError::Worker {
            operation: "content process fetch history".into(),
            reason: "fetch callback history lost its document URL".into(),
        })?;
        mutation.history.extend(resolved_history);
    }
    let (resolved_next, resolved_mutation, dynamic_result) = mutate_script_document(
        next,
        runtime,
        current_url,
        document_origin,
        viewport,
        &resolved_commands,
        Some(loader),
        parent_fetch_broker.as_deref_mut(),
    )
    .await?;
    *next = resolved_next;
    mutation.events.extend(resolved_mutation.events);
    mutation
        .scroll_commands
        .extend(resolved_mutation.scroll_commands);
    if !resolved_mutation.history.is_empty() {
        let base_url = current_url.clone();
        *current_url = resolve_content_history_document_url(
            &resolved_mutation.history,
            &base_url,
            document_origin,
        )?;
        mutation.history.extend(resolved_mutation.history);
    }
    if mutation.navigation.is_none() {
        mutation.navigation = resolved_mutation.navigation;
    } else if resolved_mutation.navigation.is_some() {
        return Err(NativeEngineError::TargetNotActionable {
            reason: "one script event-loop fetch batch cannot activate multiple navigations".into(),
        });
    }
    let (dynamic_fetches, dynamic_background) = activate_dynamic_page_script_network(
        dynamic_result,
        websocket_connections,
        event_source_connections,
        Some(&*loader),
        current_url,
        parent_fetch_broker,
    )
    .await?;
    pending.extend(fetch_commands(&resolved_commands)?);
    pending.extend(fetch_commands(&dynamic_fetches)?);
    *pump_background_events |= dynamic_background;
    Ok(())
}

pub(super) async fn load_dynamic_page_module(
    request_id: u32,
    document_url: &str,
    module_referrer: &str,
    referrer_policy: Option<NativeFetchReferrerPolicy>,
    specifier: &str,
    module_type: Option<NativeModuleResourceType>,
    runtime: &NativeJavaScriptRuntime,
    loader: &mut NativeResourceLoader,
    mut parent_fetch_broker: Option<&mut NativeContentFetchBroker<'_>>,
) -> Result<String, NativeEngineError> {
    if !runtime.is_dynamic_import_referrer_allowed(module_referrer)? {
        return Err(NativeEngineError::UnsupportedUrl {
            reason: "dynamic module referrer is not an active page script".into(),
        });
    }
    let mut import_map = runtime.module_import_map()?;
    let target = resolve_module_specifier(module_referrer, specifier, &mut import_map)?;
    runtime.set_module_import_map(import_map.clone());
    let Some(target) = target else {
        return Err(NativeEngineError::UnsupportedUrl {
            reason: "dynamic module URL is outside the supported page module schemes".into(),
        });
    };
    let module_type = module_type.unwrap_or(NativeModuleResourceType::JavaScript);
    if module_type == NativeModuleResourceType::Unsupported {
        return Err(NativeEngineError::UnsupportedUrl {
            reason: "dynamic module import attributes request an unsupported module type".into(),
        });
    }
    let module_name = native_module_loader_name(&target, module_type);

    let (module_sources, _) = runtime.module_sources_snapshot()?;
    if module_sources.contains_key(&module_name) {
        return runtime.register_dynamic_module_alias(request_id, &target);
    }
    if module_sources.len() >= MAX_NATIVE_MODULE_IMPORTS {
        return Err(NativeEngineError::limit(
            "page module graph entries",
            MAX_NATIVE_MODULE_IMPORTS,
            module_sources.len().saturating_add(1),
        ));
    }

    let object_url = runtime.object_url_resource(&target)?;
    let integrity = import_map.integrity_for_url(&target);
    let resource = if object_url.is_none() && is_parent_owned_network_target(document_url, &target)
    {
        if let Some(parent_fetch_broker) = parent_fetch_broker.as_deref_mut() {
            parent_fetch_broker
                .load_page_script(
                    document_url,
                    &target,
                    Some(module_referrer),
                    MAX_NATIVE_SCRIPT_BYTES,
                    false,
                    None,
                    integrity,
                    Some("anonymous"),
                    Some(module_type),
                    referrer_policy,
                )
                .await?
        } else {
            return Err(missing_parent_network_authority("dynamic module import"));
        }
    } else {
        loader
            .load_module_dependency_with_referrer_async(NativeScriptLoadRequest {
                document_url,
                referrer_url: Some(module_referrer),
                href: &target,
                max_source_bytes: MAX_NATIVE_SCRIPT_BYTES,
                subresource_kind: NativeSubresourceKind::Script,
                parser_inserted: false,
                nonce: None,
                integrity,
                crossorigin: Some("anonymous"),
                object_url: object_url.as_ref(),
                module_type: Some(module_type),
                worker_credentials_mode: None,
                referrer_policy,
            })
            .await?
    }
    .ok_or_else(|| NativeEngineError::Network {
        operation: "dynamic module import".into(),
        reason: "dynamic module resource could not be loaded".into(),
    })?;

    let base_url = resource.url;
    let source = resource.body;
    let initial_referrer_policy = resource.response_referrer_policy.or(referrer_policy);
    let (existing_sources, _) = runtime.module_sources_snapshot()?;
    let mut seen = existing_sources.keys().cloned().collect::<BTreeSet<_>>();
    if !seen.insert(module_name.clone()) {
        return runtime.register_dynamic_module_alias(request_id, &target);
    }
    let mut total_bytes = existing_sources.values().map(String::len).sum::<usize>();
    total_bytes = total_bytes.saturating_add(source.len());
    let maximum_module_bytes = MAX_NATIVE_SCRIPT_BYTES.saturating_mul(MAX_NATIVE_MODULE_IMPORTS);
    if total_bytes > maximum_module_bytes {
        return Err(NativeEngineError::limit(
            "page module graph bytes",
            maximum_module_bytes,
            total_bytes,
        ));
    }

    let mut graph = vec![(
        NativePageScriptTiming::ParserBlocking,
        NativePageScript::ModuleDependency {
            name: module_name.clone(),
            source: source.clone(),
            base_url: base_url.clone(),
            referrer_policy: initial_referrer_policy,
        },
    )];
    load_module_dependencies(
        document_url,
        &module_name,
        &base_url,
        &source,
        initial_referrer_policy,
        NativePageScriptTiming::ParserBlocking,
        loader,
        Some(runtime),
        &mut import_map,
        &mut graph,
        &mut seen,
        &mut total_bytes,
        parent_fetch_broker,
    )
    .await?;

    let mut new_sources = BTreeMap::new();
    let mut new_base_urls = BTreeMap::new();
    let mut new_referrer_policies = BTreeMap::new();
    for (_, script) in graph {
        if let NativePageScript::ModuleDependency {
            name,
            source,
            base_url,
            referrer_policy,
        } = script
        {
            new_sources.insert(name.clone(), source);
            new_base_urls.insert(name.clone(), base_url);
            if let Some(referrer_policy) = referrer_policy {
                new_referrer_policies.insert(name, referrer_policy);
            }
        }
    }
    runtime.extend_module_sources(new_sources, new_base_urls)?;
    runtime.extend_module_referrer_policies(new_referrer_policies)?;
    runtime.set_module_import_map(import_map);
    runtime.register_dynamic_module_alias(request_id, &target)
}
